use core::sync::atomic::Ordering;

#[cfg(not(loom))]
use core::sync::atomic::{AtomicU32, AtomicU64};
#[cfg(loom)]
use loom::sync::atomic::{AtomicU32, AtomicU64};

use crate::atomic_bitset::AtomicBitSet;

/// Lock-free MPMC container for dynamic port registration.
///
/// Manages a fixed-capacity set of slots where multiple threads can
/// concurrently add and remove entries. Each slot stores a u64 value
/// (typically a pointer offset or port ID).
///
/// Internally uses an `AtomicBitSet` to track which slots are occupied
/// and a `UniqueIndexSet`-like free list for slot allocation.
pub struct MpmcContainer {
    data: Box<[AtomicU64]>,
    occupied: AtomicBitSet,
    free_head: AtomicU64,
    next: Box<[AtomicU32]>,
    capacity: u32,
}

const SENTINEL: u32 = u32::MAX;

impl MpmcContainer {
    /// Create a new container with the given capacity.
    pub fn new(capacity: u32) -> Self {
        assert!(capacity > 0);

        let data: Vec<AtomicU64> = (0..capacity).map(|_| AtomicU64::new(0)).collect();
        let next: Vec<AtomicU32> = (0..capacity)
            .map(|i| {
                if i + 1 < capacity {
                    AtomicU32::new(i + 1)
                } else {
                    AtomicU32::new(SENTINEL)
                }
            })
            .collect();

        // Pack: [head:32][tag:32]
        let free_head = Self::pack_head(0, 0);

        Self {
            data: data.into_boxed_slice(),
            occupied: AtomicBitSet::new(capacity),
            free_head: AtomicU64::new(free_head),
            next: next.into_boxed_slice(),
            capacity,
        }
    }

    /// Add a value to the container. Returns the slot index, or `None` if full.
    pub fn add(&self, value: u64) -> Option<u32> {
        // Allocate a free slot
        let slot = self.alloc_slot()?;

        // Store the value
        // Ordering: Release so that the value is visible to concurrent readers
        // who see this slot as occupied.
        self.data[slot as usize].store(value, Ordering::Release);
        self.occupied.set(slot);

        Some(slot)
    }

    /// Remove a value by slot index. Returns the stored value.
    pub fn remove(&self, slot: u32) -> Option<u64> {
        if !self.occupied.clear(slot) {
            return None; // slot was not occupied
        }

        let value = self.data[slot as usize].load(Ordering::Acquire);
        self.free_slot(slot);
        Some(value)
    }

    /// Get the value at a slot. Returns `None` if the slot is not occupied.
    pub fn get(&self, slot: u32) -> Option<u64> {
        if self.occupied.is_set(slot) {
            Some(self.data[slot as usize].load(Ordering::Acquire))
        } else {
            None
        }
    }

    /// Iterate over all occupied slots, calling `f` with (index, value).
    pub fn for_each<F: FnMut(u32, u64)>(&self, mut f: F) {
        for i in 0..self.capacity {
            if self.occupied.is_set(i) {
                let value = self.data[i as usize].load(Ordering::Acquire);
                f(i, value);
            }
        }
    }

    /// Number of occupied slots.
    pub fn len(&self) -> u32 {
        self.occupied.count_set()
    }

    /// Whether the container is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total capacity.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    fn alloc_slot(&self) -> Option<u32> {
        let mut old = self.free_head.load(Ordering::Acquire);
        loop {
            let head = Self::extract_head(old);
            if head == SENTINEL {
                return None;
            }

            let next_idx = self.next[head as usize].load(Ordering::Acquire);
            let tag = Self::extract_tag(old);
            let new = Self::pack_head(next_idx, tag.wrapping_add(1));

            match self.free_head.compare_exchange_weak(
                old,
                new,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(head),
                Err(current) => old = current,
            }
        }
    }

    fn free_slot(&self, slot: u32) {
        let mut old = self.free_head.load(Ordering::Acquire);
        loop {
            let head = Self::extract_head(old);
            self.next[slot as usize].store(head, Ordering::Release);

            let tag = Self::extract_tag(old);
            let new = Self::pack_head(slot, tag.wrapping_add(1));

            match self.free_head.compare_exchange_weak(
                old,
                new,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(current) => old = current,
            }
        }
    }

    fn pack_head(head: u32, tag: u32) -> u64 {
        ((head as u64) << 32) | (tag as u64)
    }

    fn extract_head(val: u64) -> u32 {
        (val >> 32) as u32
    }

    fn extract_tag(val: u64) -> u32 {
        val as u32
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn add_and_get() {
        let c = MpmcContainer::new(4);
        let s0 = c.add(100).unwrap();
        let s1 = c.add(200).unwrap();

        assert_eq!(c.get(s0), Some(100));
        assert_eq!(c.get(s1), Some(200));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn remove() {
        let c = MpmcContainer::new(4);
        let s = c.add(42).unwrap();
        assert_eq!(c.remove(s), Some(42));
        assert_eq!(c.get(s), None);
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn full() {
        let c = MpmcContainer::new(2);
        c.add(1).unwrap();
        c.add(2).unwrap();
        assert!(c.add(3).is_none());
    }

    #[test]
    fn reuse_after_remove() {
        let c = MpmcContainer::new(2);
        let s = c.add(1).unwrap();
        c.remove(s);
        let s2 = c.add(99).unwrap();
        assert_eq!(c.get(s2), Some(99));
    }

    #[test]
    fn for_each() {
        let c = MpmcContainer::new(4);
        c.add(10);
        c.add(20);
        c.add(30);

        let mut values = Vec::new();
        c.for_each(|_, v| values.push(v));
        values.sort();
        assert_eq!(values, vec![10, 20, 30]);
    }

    #[test]
    fn concurrent_add_remove() {
        use std::sync::Arc;
        use std::thread;

        let c = Arc::new(MpmcContainer::new(64));
        let handles: Vec<_> = (0..4)
            .map(|t| {
                let c = c.clone();
                thread::spawn(move || {
                    for i in 0..50u64 {
                        if let Some(slot) = c.add(t * 100 + i) {
                            std::hint::black_box(slot);
                            c.remove(slot);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(c.len(), 0);
    }
}

#[cfg(all(test, loom))]
mod loom_tests {
    use super::*;
    use loom::sync::Arc;
    use loom::thread;

    #[test]
    fn concurrent_add_remove_loom() {
        loom::model(|| {
            let c = Arc::new(MpmcContainer::new(3));
            let c2 = c.clone();

            let t1 = thread::spawn(move || {
                if let Some(s) = c2.add(1) {
                    c2.remove(s);
                }
            });

            let t2 = {
                let c = c.clone();
                thread::spawn(move || {
                    if let Some(s) = c.add(2) {
                        c.remove(s);
                    }
                })
            };

            t1.join().unwrap();
            t2.join().unwrap();
            assert_eq!(c.len(), 0);
        });
    }
}
