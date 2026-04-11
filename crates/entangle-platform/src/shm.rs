use std::ptr::NonNull;

use nix::fcntl::OFlag;
use nix::sys::mman::{mmap, munmap, shm_open, shm_unlink, MapFlags, ProtFlags};
use nix::sys::stat::{fstat, Mode};
use nix::unistd::ftruncate;
use tracing::{debug, warn};

use crate::error::PlatformError;

/// POSIX shared memory wrapper.
///
/// Uses `nix` crate to minimize unsafe. Automatically unmaps on drop,
/// and unlinks the shared memory segment if this instance is the owner.
pub struct SharedMemory {
    ptr: NonNull<u8>,
    size: usize,
    name: String,
    is_owner: bool,
}

// Safety: SharedMemory is designed for cross-process sharing.
// Internal access is synchronized by lock-free data structures at higher layers.
unsafe impl Send for SharedMemory {}
unsafe impl Sync for SharedMemory {}

impl SharedMemory {
    /// Create a new shared memory segment.
    ///
    /// The caller becomes the owner and the segment will be unlinked on drop.
    pub fn create(name: &str, size: usize) -> Result<Self, PlatformError> {
        let shm_name = Self::normalize_name(name);
        debug!(name = %shm_name, size, "creating shared memory");

        let fd = shm_open(
            shm_name.as_str(),
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )
        .map_err(|e| PlatformError::SharedMemoryCreate {
            name: shm_name.clone(),
            reason: e.to_string(),
        })?;

        ftruncate(&fd, size as i64).map_err(|e| PlatformError::SharedMemoryCreate {
            name: shm_name.clone(),
            reason: format!("ftruncate: {e}"),
        })?;

        let ptr = Self::do_mmap(&fd, size, &shm_name)?;

        // Zero-initialize the memory
        // Safety: ptr is valid for `size` bytes, just allocated via mmap
        unsafe {
            std::ptr::write_bytes(ptr.as_ptr(), 0, size);
        }

        Ok(Self {
            ptr,
            size,
            name: shm_name,
            is_owner: true,
        })
    }

    /// Open an existing shared memory segment.
    pub fn open(name: &str) -> Result<Self, PlatformError> {
        let shm_name = Self::normalize_name(name);
        debug!(name = %shm_name, "opening shared memory");

        let fd = shm_open(shm_name.as_str(), OFlag::O_RDWR, Mode::empty()).map_err(|e| {
            PlatformError::SharedMemoryOpen {
                name: shm_name.clone(),
                reason: e.to_string(),
            }
        })?;

        let stat = fstat(fd.as_raw_fd()).map_err(|e| PlatformError::SharedMemoryOpen {
            name: shm_name.clone(),
            reason: format!("fstat: {e}"),
        })?;
        let size = stat.st_size as usize;

        let ptr = Self::do_mmap(&fd, size, &shm_name)?;

        Ok(Self {
            ptr,
            size,
            name: shm_name,
            is_owner: false,
        })
    }

    /// Unlink (remove) a shared memory segment by name without opening it.
    pub fn unlink(name: &str) -> Result<(), PlatformError> {
        let shm_name = Self::normalize_name(name);
        shm_unlink(shm_name.as_str()).map_err(|e| PlatformError::SharedMemoryUnlink {
            name: shm_name,
            reason: e.to_string(),
        })
    }

    /// Returns a raw pointer to the mapped memory region.
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    /// Returns a mutable raw pointer to the mapped memory region.
    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Returns the size of the shared memory segment in bytes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the name of the shared memory segment.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether this instance owns (and will unlink) the segment.
    pub fn is_owner(&self) -> bool {
        self.is_owner
    }

    /// Get a typed slice from the shared memory at the given byte offset.
    ///
    /// # Safety
    /// The caller must ensure:
    /// - The offset + count * `size_of::<T>()` does not exceed the segment size
    /// - The memory at that offset is properly initialized for type T
    /// - No mutable aliases exist for the same region
    pub unsafe fn as_slice<T: Copy>(&self, offset: usize, count: usize) -> &[T] {
        let byte_len = count * std::mem::size_of::<T>();
        debug_assert!(offset + byte_len <= self.size);
        debug_assert!(offset.is_multiple_of(std::mem::align_of::<T>()));
        let ptr = self.ptr.as_ptr().add(offset) as *const T;
        std::slice::from_raw_parts(ptr, count)
    }

    /// Get a mutable typed slice from the shared memory.
    ///
    /// # Safety
    /// Same as `as_slice`, plus the caller must ensure exclusive access.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn as_mut_slice<T: Copy>(&self, offset: usize, count: usize) -> &mut [T] {
        let byte_len = count * std::mem::size_of::<T>();
        debug_assert!(offset + byte_len <= self.size);
        debug_assert!(offset.is_multiple_of(std::mem::align_of::<T>()));
        let ptr = self.ptr.as_ptr().add(offset) as *mut T;
        std::slice::from_raw_parts_mut(ptr, count)
    }

    /// Get a reference to a value at the given byte offset.
    ///
    /// # Safety
    /// The caller must ensure proper alignment, initialization, and no mutable aliases.
    pub unsafe fn get_ref<T: Copy>(&self, offset: usize) -> &T {
        debug_assert!(offset + std::mem::size_of::<T>() <= self.size);
        debug_assert!(offset.is_multiple_of(std::mem::align_of::<T>()));
        &*(self.ptr.as_ptr().add(offset) as *const T)
    }

    /// Get a mutable reference to a value at the given byte offset.
    ///
    /// # Safety
    /// The caller must ensure proper alignment, initialization, and exclusive access.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_mut<T: Copy>(&self, offset: usize) -> &mut T {
        debug_assert!(offset + std::mem::size_of::<T>() <= self.size);
        debug_assert!(offset.is_multiple_of(std::mem::align_of::<T>()));
        &mut *(self.ptr.as_ptr().add(offset) as *mut T)
    }

    fn do_mmap(
        fd: &impl std::os::fd::AsFd,
        size: usize,
        name: &str,
    ) -> Result<NonNull<u8>, PlatformError> {
        let len = std::num::NonZeroUsize::new(size).ok_or_else(|| PlatformError::Mmap {
            reason: "size must be non-zero".into(),
        })?;

        // Safety: fd is a valid shared memory file descriptor, size > 0
        let ptr = unsafe {
            mmap(
                None,
                len,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                fd,
                0,
            )
        }
        .map_err(|e| PlatformError::Mmap {
            reason: format!("mmap for '{name}': {e}"),
        })?;

        NonNull::new(ptr.as_ptr() as *mut u8).ok_or_else(|| PlatformError::Mmap {
            reason: "mmap returned null".into(),
        })
    }

    /// Ensure name starts with '/' for POSIX shm and is within macOS 30-char limit.
    fn normalize_name(name: &str) -> String {
        let n = if name.starts_with('/') {
            name.to_string()
        } else {
            format!("/{name}")
        };

        // macOS shm_open has a 30-character name limit (including leading '/')
        if cfg!(target_os = "macos") && n.len() > 30 {
            // Hash long names to fit within the limit
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            n.hash(&mut hasher);
            format!("/e_{:016x}", hasher.finish())
        } else {
            n
        }
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        // Safety: ptr and size were obtained from a successful mmap call
        let result = unsafe {
            munmap(
                NonNull::new(self.ptr.as_ptr() as *mut libc::c_void).unwrap(),
                self.size,
            )
        };
        if let Err(e) = result {
            warn!(name = %self.name, error = %e, "munmap failed on drop");
        }

        if self.is_owner {
            if let Err(e) = shm_unlink(self.name.as_str()) {
                warn!(name = %self.name, error = %e, "shm_unlink failed on drop");
            }
        }
    }
}

use std::os::fd::AsRawFd;

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_name(test: &str) -> String {
        use std::time::SystemTime;
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("/entangle_test_{test}_{ts}")
    }

    #[test]
    fn create_and_open() {
        let name = unique_name("create_open");
        let size = 4096;

        let shm = SharedMemory::create(&name, size).unwrap();
        assert_eq!(shm.size(), size);
        assert!(shm.is_owner());

        let shm2 = SharedMemory::open(&name).unwrap();
        assert_eq!(shm2.size(), size);
        assert!(!shm2.is_owner());

        drop(shm2);
        drop(shm);
    }

    #[test]
    fn write_and_read() {
        let name = unique_name("write_read");
        let size = 4096;

        let shm = SharedMemory::create(&name, size).unwrap();

        // Safety: writing within bounds, no concurrent access in test
        unsafe {
            let val: &mut u64 = shm.get_mut(0);
            *val = 0xDEAD_BEEF;
        }

        let shm2 = SharedMemory::open(&name).unwrap();
        unsafe {
            let val: &u64 = shm2.get_ref(0);
            assert_eq!(*val, 0xDEAD_BEEF);
        }

        drop(shm2);
        drop(shm);
    }

    #[test]
    fn double_create_fails() {
        let name = unique_name("double_create");
        let _shm = SharedMemory::create(&name, 4096).unwrap();
        assert!(SharedMemory::create(&name, 4096).is_err());
    }
}
