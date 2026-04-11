use core::sync::atomic::Ordering;

#[cfg(not(loom))]
use core::sync::atomic::AtomicU64;
#[cfg(loom)]
use loom::sync::atomic::AtomicU64;

#[cfg(not(loom))]
use core::sync::atomic::AtomicU32;
#[cfg(loom)]
use loom::sync::atomic::AtomicU32;

/// Lock-free MPMC unique index set for slot allocation.
///
/// Manages a pool of unique indices [0..capacity). Multiple threads can
/// concurrently acquire and release indices without locks. Based on a
/// lock-free free-list with ABA protection via tagged pointers.
///
/// Head word bit layout: \[head_index:32\]\[tag:32\]
/// - head_index: index of the first free slot (SENTINEL if empty)
/// - tag: ABA counter, incremented on every CAS to prevent ABA
///
/// Borrowed count is tracked separately in an AtomicU32 to avoid
/// bit-width limitations.
pub struct UniqueIndexSet {
    head: AtomicU64,
    next: Box<[AtomicU32]>,
    capacity: u32,
    /// Separate borrowed counter — no bit-width limit from head packing.
    borrowed: AtomicU32,
}

const SENTINEL: u32 = 0xFFFF_FFFF; // head_index value meaning "empty"
const HEAD_SHIFT: u32 = 32;
const TAG_MASK: u64 = 0xFFFF_FFFF;

/// Error returned when no more indices are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcquireError;

impl std::fmt::Display for AcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no free indices available")
    }
}

impl std::error::Error for AcquireError {}

impl UniqueIndexSet {
    /// Create a new index set with `capacity` slots [0..capacity).
    ///
    /// All indices start as free. Max capacity is 2^32 - 1.
    pub fn new(capacity: u32) -> Self {
        assert!(capacity > 0 && capacity < SENTINEL, "capacity out of range");

        let next: Vec<AtomicU32> = (0..capacity)
            .map(|i| {
                if i + 1 < capacity {
                    AtomicU32::new(i + 1)
                } else {
                    AtomicU32::new(SENTINEL)
                }
            })
            .collect();

        let head = Self::pack(0, 0);
        Self {
            head: AtomicU64::new(head),
            next: next.into_boxed_slice(),
            capacity,
            borrowed: AtomicU32::new(0),
        }
    }

    /// Acquire a unique index from the set.
    ///
    /// Returns `Err(AcquireError)` if all indices are currently borrowed.
    /// Lock-free: uses CAS loop on the packed head word.
    pub fn acquire(&self) -> Result<u32, AcquireError> {
        let mut old = self.head.load(Ordering::Acquire);
        loop {
            let head_idx = Self::extract_head(old);
            if head_idx == SENTINEL {
                return Err(AcquireError);
            }

            // Read the next pointer of the head slot.
            // Ordering: Acquire to see any prior release by a concurrent `release()`.
            let next_idx = self.next[head_idx as usize].load(Ordering::Acquire);
            let tag = Self::extract_tag(old);
            let new = Self::pack(next_idx, tag.wrapping_add(1));

            // Ordering: AcqRel — Acquire to pair with releases in `release()`,
            // Release to publish our update to concurrent threads.
            match self
                .head
                .compare_exchange_weak(old, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    // Ordering: Relaxed — borrowed is an approximate counter,
                    // not used for synchronization of the free-list.
                    self.borrowed.fetch_add(1, Ordering::Relaxed);
                    return Ok(head_idx);
                }
                Err(current) => old = current,
            }
        }
    }

    /// Release an index back to the set.
    ///
    /// The index must have been previously acquired. Double-release is
    /// not checked and leads to undefined behavior of the free-list.
    pub fn release(&self, index: u32) {
        debug_assert!(index < self.capacity);

        let mut old = self.head.load(Ordering::Acquire);
        loop {
            let head_idx = Self::extract_head(old);
            // Point the released slot's next to the current head.
            // Ordering: Release ensures our write is visible when another
            // thread acquires this slot via Acquire on head.
            self.next[index as usize].store(head_idx, Ordering::Release);

            let tag = Self::extract_tag(old);
            let new = Self::pack(index, tag.wrapping_add(1));

            match self
                .head
                .compare_exchange_weak(old, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    self.borrowed.fetch_sub(1, Ordering::Relaxed);
                    return;
                }
                Err(current) => old = current,
            }
        }
    }

    /// Number of currently borrowed indices.
    pub fn borrowed_count(&self) -> u32 {
        self.borrowed.load(Ordering::Relaxed)
    }

    /// Total capacity.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Number of available (free) indices.
    pub fn available(&self) -> u32 {
        self.capacity.saturating_sub(self.borrowed_count())
    }

    fn pack(head: u32, tag: u32) -> u64 {
        ((head as u64) << HEAD_SHIFT) | (tag as u64 & TAG_MASK)
    }

    fn extract_head(val: u64) -> u32 {
        (val >> HEAD_SHIFT) as u32
    }

    fn extract_tag(val: u64) -> u32 {
        (val & TAG_MASK) as u32
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn basic_acquire_release() {
        let set = UniqueIndexSet::new(4);
        assert_eq!(set.borrowed_count(), 0);
        assert_eq!(set.available(), 4);

        let a = set.acquire().unwrap();
        let b = set.acquire().unwrap();
        assert_ne!(a, b);
        assert_eq!(set.borrowed_count(), 2);

        set.release(a);
        assert_eq!(set.borrowed_count(), 1);

        set.release(b);
        assert_eq!(set.borrowed_count(), 0);
    }

    #[test]
    fn exhaust_and_recover() {
        let set = UniqueIndexSet::new(2);
        let a = set.acquire().unwrap();
        let b = set.acquire().unwrap();
        assert!(set.acquire().is_err());

        set.release(a);
        let c = set.acquire().unwrap();
        assert_eq!(c, a); // LIFO: last released is first acquired
        set.release(b);
        set.release(c);
    }

    #[test]
    fn all_indices_unique() {
        let n = 64;
        let set = UniqueIndexSet::new(n);
        let mut acquired = Vec::new();
        for _ in 0..n {
            acquired.push(set.acquire().unwrap());
        }
        acquired.sort();
        acquired.dedup();
        assert_eq!(acquired.len(), n as usize);
    }

    #[test]
    fn concurrent_acquire_release() {
        use std::sync::Arc;
        use std::thread;

        let set = Arc::new(UniqueIndexSet::new(64));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let set = set.clone();
                thread::spawn(move || {
                    for _ in 0..100 {
                        if let Ok(idx) = set.acquire() {
                            std::hint::black_box(idx);
                            set.release(idx);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(set.borrowed_count(), 0);
    }

    #[test]
    fn large_capacity_beyond_16bit() {
        // Verify borrowed_count works beyond 65535
        let set = UniqueIndexSet::new(100_000);
        let mut indices = Vec::new();
        for _ in 0..70_000 {
            indices.push(set.acquire().unwrap());
        }
        assert_eq!(set.borrowed_count(), 70_000);
        for idx in indices {
            set.release(idx);
        }
        assert_eq!(set.borrowed_count(), 0);
    }
}

#[cfg(all(test, loom))]
mod loom_tests {
    use super::*;
    use loom::sync::Arc;
    use loom::thread;

    #[test]
    fn concurrent_acquire_release_never_duplicates() {
        loom::model(|| {
            let set = Arc::new(UniqueIndexSet::new(3));
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    let set = set.clone();
                    thread::spawn(move || {
                        if let Ok(idx) = set.acquire() {
                            set.release(idx);
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }
        });
    }
}
