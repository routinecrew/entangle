use core::sync::atomic::Ordering;

#[cfg(not(loom))]
use core::sync::atomic::AtomicU64;
#[cfg(loom)]
use loom::sync::atomic::AtomicU64;

/// Lock-free atomic bitset for tracking borrowed/used slots.
///
/// Uses an array of `AtomicU64` words, each tracking 64 bits.
/// All operations are lock-free using CAS loops.
pub struct AtomicBitSet {
    words: Box<[AtomicU64]>,
    capacity: u32,
}

impl AtomicBitSet {
    /// Create a new bitset with the given capacity (number of bits).
    pub fn new(capacity: u32) -> Self {
        let word_count = (capacity as usize).div_ceil(64);
        let words: Vec<AtomicU64> = (0..word_count).map(|_| AtomicU64::new(0)).collect();
        Self {
            words: words.into_boxed_slice(),
            capacity,
        }
    }

    /// Set a bit. Returns `true` if it was previously unset.
    ///
    /// Ordering: AcqRel on success to synchronize with concurrent clear operations.
    pub fn set(&self, index: u32) -> bool {
        debug_assert!(index < self.capacity);
        let word_idx = (index / 64) as usize;
        let bit = 1u64 << (index % 64);

        let mut old = self.words[word_idx].load(Ordering::Acquire);
        loop {
            if old & bit != 0 {
                return false; // already set
            }
            // Ordering: AcqRel ensures the set is visible to concurrent readers.
            match self.words[word_idx].compare_exchange_weak(
                old,
                old | bit,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(current) => old = current,
            }
        }
    }

    /// Clear a bit. Returns `true` if it was previously set.
    pub fn clear(&self, index: u32) -> bool {
        debug_assert!(index < self.capacity);
        let word_idx = (index / 64) as usize;
        let bit = 1u64 << (index % 64);

        let mut old = self.words[word_idx].load(Ordering::Acquire);
        loop {
            if old & bit == 0 {
                return false;
            }
            match self.words[word_idx].compare_exchange_weak(
                old,
                old & !bit,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(current) => old = current,
            }
        }
    }

    /// Check if a bit is set.
    pub fn is_set(&self, index: u32) -> bool {
        debug_assert!(index < self.capacity);
        let word_idx = (index / 64) as usize;
        let bit = 1u64 << (index % 64);
        self.words[word_idx].load(Ordering::Acquire) & bit != 0
    }

    /// Count the number of set bits.
    pub fn count_set(&self) -> u32 {
        self.words
            .iter()
            .map(|w| w.load(Ordering::Acquire).count_ones())
            .sum()
    }

    /// Capacity of the bitset.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Find the first set bit and clear it atomically. Returns the index.
    pub fn find_and_clear_first_set(&self) -> Option<u32> {
        for (word_idx, word) in self.words.iter().enumerate() {
            let mut old = word.load(Ordering::Acquire);
            loop {
                if old == 0 {
                    break;
                }
                let bit_pos = old.trailing_zeros();
                let bit = 1u64 << bit_pos;
                match word.compare_exchange_weak(
                    old,
                    old & !bit,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        let index = (word_idx as u32) * 64 + bit_pos;
                        if index < self.capacity {
                            return Some(index);
                        }
                        word.fetch_or(bit, Ordering::Release);
                        return None;
                    }
                    Err(current) => old = current,
                }
            }
        }
        None
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn set_and_check() {
        let bs = AtomicBitSet::new(128);
        assert!(!bs.is_set(0));
        assert!(bs.set(0));
        assert!(bs.is_set(0));
        assert!(!bs.set(0));
    }

    #[test]
    fn clear() {
        let bs = AtomicBitSet::new(128);
        bs.set(42);
        assert!(bs.clear(42));
        assert!(!bs.is_set(42));
        assert!(!bs.clear(42));
    }

    #[test]
    fn count() {
        let bs = AtomicBitSet::new(128);
        bs.set(0);
        bs.set(63);
        bs.set(64);
        bs.set(127);
        assert_eq!(bs.count_set(), 4);
    }

    #[test]
    fn find_and_clear() {
        let bs = AtomicBitSet::new(128);
        bs.set(5);
        bs.set(70);
        assert_eq!(bs.find_and_clear_first_set(), Some(5));
        assert_eq!(bs.find_and_clear_first_set(), Some(70));
        assert_eq!(bs.find_and_clear_first_set(), None);
    }

    #[test]
    fn concurrent_set() {
        use std::sync::Arc;
        use std::thread;

        let bs = Arc::new(AtomicBitSet::new(256));
        let handles: Vec<_> = (0..4)
            .map(|t| {
                let bs = bs.clone();
                thread::spawn(move || {
                    for i in 0..64 {
                        bs.set(t * 64 + i);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(bs.count_set(), 256);
    }
}

#[cfg(all(test, loom))]
mod loom_tests {
    use super::*;
    use loom::sync::Arc;
    use loom::thread;

    #[test]
    fn concurrent_set_clear() {
        loom::model(|| {
            let bs = Arc::new(AtomicBitSet::new(4));
            let bs2 = bs.clone();

            let t1 = thread::spawn(move || {
                bs2.set(0);
                bs2.set(1);
            });

            let t2 = {
                let bs = bs.clone();
                thread::spawn(move || {
                    bs.set(2);
                    bs.set(3);
                })
            };

            t1.join().unwrap();
            t2.join().unwrap();
            assert_eq!(bs.count_set(), 4);
        });
    }
}
