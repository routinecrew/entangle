use core::sync::atomic::Ordering;

#[cfg(not(loom))]
use core::sync::atomic::AtomicIsize;
#[cfg(loom)]
use loom::sync::atomic::AtomicIsize;

/// A pointer stored as a relative offset from its own address.
///
/// When shared memory is mapped at different virtual addresses in different
/// processes, absolute pointers are invalid. A relocatable pointer stores
/// the offset from its own location to the target, which remains valid
/// regardless of where the shared memory is mapped.
///
/// # Layout
/// Stored as a single `AtomicIsize` containing the signed offset.
/// A value of 0 represents null.
#[repr(C)]
pub struct RelocatablePtr {
    offset: AtomicIsize,
}

impl RelocatablePtr {
    /// Create a null relocatable pointer.
    pub fn null() -> Self {
        Self {
            offset: AtomicIsize::new(0),
        }
    }

    /// Create a relocatable pointer pointing to the given absolute address.
    ///
    /// # Safety
    /// The caller must ensure `target` is within the same shared memory
    /// segment (or a valid memory region) as this RelocatablePtr.
    pub unsafe fn from_ptr(&self, target: *const u8) -> isize {
        let self_addr = &self.offset as *const AtomicIsize as *const u8;
        target.offset_from(self_addr)
    }

    /// Store a target address as a relative offset.
    ///
    /// # Safety
    /// Same as `from_ptr`.
    pub unsafe fn store(&self, target: *const u8) {
        let off = self.from_ptr(target);
        // Ordering: Release ensures the pointed-to data is visible
        // to any thread that subsequently loads this offset with Acquire.
        self.offset.store(off, Ordering::Release);
    }

    /// Load the target address.
    /// Returns null if the offset is 0.
    pub fn load(&self) -> *const u8 {
        // Ordering: Acquire pairs with the Release in store(),
        // ensuring visibility of data written before the store.
        let off = self.offset.load(Ordering::Acquire);
        if off == 0 {
            return std::ptr::null();
        }
        let self_addr = &self.offset as *const AtomicIsize as *const u8;
        // Safety: the offset was computed from a valid pointer pair
        unsafe { self_addr.offset(off) }
    }

    /// Load the target address as a mutable pointer.
    pub fn load_mut(&self) -> *mut u8 {
        self.load() as *mut u8
    }

    /// Check if this pointer is null.
    pub fn is_null(&self) -> bool {
        self.offset.load(Ordering::Acquire) == 0
    }

    /// Set to null.
    pub fn clear(&self) {
        self.offset.store(0, Ordering::Release);
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn null_by_default() {
        let rp = RelocatablePtr::null();
        assert!(rp.is_null());
        assert!(rp.load().is_null());
    }

    #[test]
    fn store_and_load() {
        let data: [u8; 16] = [42; 16];
        let rp = RelocatablePtr::null();

        unsafe {
            rp.store(data.as_ptr());
        }

        assert!(!rp.is_null());
        let loaded = rp.load();
        assert_eq!(loaded, data.as_ptr());
    }

    #[test]
    fn clear_resets_to_null() {
        let data: u8 = 99;
        let rp = RelocatablePtr::null();
        unsafe {
            rp.store(&data as *const u8);
        }
        assert!(!rp.is_null());
        rp.clear();
        assert!(rp.is_null());
    }
}
