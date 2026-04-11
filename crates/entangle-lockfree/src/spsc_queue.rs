use core::sync::atomic::Ordering;

#[cfg(not(loom))]
use core::sync::atomic::AtomicU64;
#[cfg(loom)]
use loom::sync::atomic::AtomicU64;

/// Lock-free SPSC (single-producer, single-consumer) bounded queue.
///
/// Used for passing `PointerOffset` values (as u64) from publisher to subscriber
/// via shared memory. Cache-line aligned to prevent false sharing between
/// producer and consumer.
///
/// The queue uses a ring buffer with separate write and read positions.
/// Only the producer writes to `write_pos` and data slots; only the
/// consumer writes to `read_pos` and reads data slots.
pub struct SpscQueue {
    /// Producer-owned position. Only written by producer.
    write_pos: CacheAligned<AtomicU64>,
    /// Consumer-owned position. Only written by consumer.
    read_pos: CacheAligned<AtomicU64>,
    /// Ring buffer capacity (must be power of 2).
    capacity: usize,
    /// Bitmask for wrapping (capacity - 1).
    mask: usize,
    /// Ring buffer data slots.
    data: Box<[AtomicU64]>,
}

/// Cache-line aligned wrapper to prevent false sharing.
#[repr(C, align(64))]
struct CacheAligned<T>(T);

impl SpscQueue {
    /// Create a new SPSC queue with the given capacity.
    ///
    /// Capacity is rounded up to the next power of 2 for efficient masking.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two().max(2);
        let data: Vec<AtomicU64> = (0..capacity).map(|_| AtomicU64::new(0)).collect();

        Self {
            write_pos: CacheAligned(AtomicU64::new(0)),
            read_pos: CacheAligned(AtomicU64::new(0)),
            capacity,
            mask: capacity - 1,
            data: data.into_boxed_slice(),
        }
    }

    /// Push a value into the queue. Returns `false` if the queue is full.
    ///
    /// Must only be called by the single producer.
    pub fn push(&self, value: u64) -> bool {
        // Ordering: Relaxed is safe because only the producer reads/writes write_pos.
        let write = self.write_pos.0.load(Ordering::Relaxed);
        let read = self.read_pos.0.load(Ordering::Acquire);

        if write.wrapping_sub(read) >= self.capacity as u64 {
            return false; // queue full
        }

        let idx = (write as usize) & self.mask;
        // Ordering: Release ensures the value is visible to the consumer's
        // subsequent Acquire load.
        self.data[idx].store(value, Ordering::Release);
        // Ordering: Release to publish the new write position.
        self.write_pos
            .0
            .store(write.wrapping_add(1), Ordering::Release);
        true
    }

    /// Pop a value from the queue. Returns `None` if empty.
    ///
    /// Must only be called by the single consumer.
    pub fn pop(&self) -> Option<u64> {
        // Ordering: Relaxed because only the consumer reads/writes read_pos.
        let read = self.read_pos.0.load(Ordering::Relaxed);
        let write = self.write_pos.0.load(Ordering::Acquire);

        if read == write {
            return None; // queue empty
        }

        let idx = (read as usize) & self.mask;
        // Ordering: Acquire to see the value stored by the producer's Release.
        let value = self.data[idx].load(Ordering::Acquire);
        // Ordering: Release to publish the new read position.
        self.read_pos
            .0
            .store(read.wrapping_add(1), Ordering::Release);
        Some(value)
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        let read = self.read_pos.0.load(Ordering::Acquire);
        let write = self.write_pos.0.load(Ordering::Acquire);
        read == write
    }

    /// Check if the queue is full.
    pub fn is_full(&self) -> bool {
        let read = self.read_pos.0.load(Ordering::Acquire);
        let write = self.write_pos.0.load(Ordering::Acquire);
        write.wrapping_sub(read) >= self.capacity as u64
    }

    /// Number of elements currently in the queue.
    pub fn len(&self) -> usize {
        let read = self.read_pos.0.load(Ordering::Acquire);
        let write = self.write_pos.0.load(Ordering::Acquire);
        write.wrapping_sub(read) as usize
    }

    /// Capacity of the queue.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn push_pop_basic() {
        let q = SpscQueue::new(4);
        assert!(q.is_empty());

        assert!(q.push(1));
        assert!(q.push(2));
        assert!(q.push(3));
        assert!(q.push(4));
        assert!(q.is_full());
        assert!(!q.push(5)); // full

        assert_eq!(q.pop(), Some(1));
        assert_eq!(q.pop(), Some(2));
        assert_eq!(q.pop(), Some(3));
        assert_eq!(q.pop(), Some(4));
        assert!(q.is_empty());
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn wrap_around() {
        let q = SpscQueue::new(2);
        for round in 0..10 {
            assert!(q.push(round * 2));
            assert!(q.push(round * 2 + 1));
            assert_eq!(q.pop(), Some(round * 2));
            assert_eq!(q.pop(), Some(round * 2 + 1));
        }
    }

    #[test]
    fn len_tracking() {
        let q = SpscQueue::new(4);
        assert_eq!(q.len(), 0);
        q.push(1);
        assert_eq!(q.len(), 1);
        q.push(2);
        assert_eq!(q.len(), 2);
        q.pop();
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn concurrent_producer_consumer() {
        use std::sync::Arc;
        use std::thread;

        let q = Arc::new(SpscQueue::new(64));
        let n = 10_000u64;

        let producer = {
            let q = q.clone();
            thread::spawn(move || {
                for i in 0..n {
                    while !q.push(i) {
                        std::hint::spin_loop();
                    }
                }
            })
        };

        let consumer = {
            let q = q.clone();
            thread::spawn(move || {
                let mut received = Vec::with_capacity(n as usize);
                while received.len() < n as usize {
                    if let Some(v) = q.pop() {
                        received.push(v);
                    } else {
                        std::hint::spin_loop();
                    }
                }
                received
            })
        };

        producer.join().unwrap();
        let received = consumer.join().unwrap();

        // Verify FIFO order
        let expected: Vec<u64> = (0..n).collect();
        assert_eq!(received, expected);
    }
}

#[cfg(all(test, loom))]
mod loom_tests {
    use super::*;
    use loom::sync::Arc;
    use loom::thread;

    #[test]
    fn spsc_fifo_ordering() {
        loom::model(|| {
            let q = Arc::new(SpscQueue::new(4));
            let q2 = q.clone();

            let producer = thread::spawn(move || {
                q2.push(1);
                q2.push(2);
            });

            let consumer = {
                let q = q.clone();
                thread::spawn(move || {
                    let mut values = Vec::new();
                    // Try a few times (loom explores all interleavings)
                    for _ in 0..4 {
                        if let Some(v) = q.pop() {
                            values.push(v);
                        }
                    }
                    values
                })
            };

            producer.join().unwrap();
            let values = consumer.join().unwrap();

            // Values must be in order (FIFO)
            for w in values.windows(2) {
                assert!(w[0] < w[1]);
            }
        });
    }
}
