use std::ptr::NonNull;

use entangle_platform::SharedMemory;

use crate::contracts::{LoanError, PointerOffset};
use crate::pool_alloc::PoolAllocator;

/// Header stored at the start of each data segment.
#[derive(Clone, Copy)]
#[repr(C)]
struct SegmentHeader {
    chunk_size: u32,
    chunk_count: u32,
    _reserved: [u8; 56], // pad to 64 bytes (cache line)
}

const HEADER_SIZE: usize = std::mem::size_of::<SegmentHeader>();

/// Data segment: the shared memory region where payload data is stored.
///
/// Publisher allocates chunks (loan), writes data directly into shared memory,
/// and sends only the `PointerOffset` to subscribers. Subscribers read
/// directly from the same memory. Zero copies.
pub struct DataSegment {
    shm: SharedMemory,
    allocator: PoolAllocator,
    chunk_size: usize,
    chunk_count: u32,
}

impl DataSegment {
    /// Create a new data segment backed by shared memory.
    ///
    /// - `name`: shared memory segment name
    /// - `chunk_size`: size of each payload chunk
    /// - `chunk_count`: number of chunks
    /// - `segment_id`: segment ID for pointer offset encoding
    pub fn create(
        name: &str,
        chunk_size: usize,
        chunk_count: u32,
        segment_id: u16,
    ) -> Result<Self, entangle_platform::PlatformError> {
        // Align chunk size to 8 bytes
        let aligned_chunk_size = (chunk_size + 7) & !7;
        let total_size = HEADER_SIZE + (aligned_chunk_size * chunk_count as usize);

        let shm = SharedMemory::create(name, total_size)?;

        // Write header
        // Safety: we just created the shm, have exclusive access, header fits
        unsafe {
            let header: &mut SegmentHeader = shm.get_mut(0);
            header.chunk_size = aligned_chunk_size as u32;
            header.chunk_count = chunk_count;
        }

        let allocator =
            PoolAllocator::new(chunk_count, aligned_chunk_size, segment_id, HEADER_SIZE);

        Ok(Self {
            shm,
            allocator,
            chunk_size: aligned_chunk_size,
            chunk_count,
        })
    }

    /// Open an existing data segment.
    pub fn open(name: &str, segment_id: u16) -> Result<Self, entangle_platform::PlatformError> {
        let shm = SharedMemory::open(name)?;

        // Safety: reading a header written by create()
        let (chunk_size, chunk_count) = unsafe {
            let header: &SegmentHeader = shm.get_ref(0);
            (header.chunk_size as usize, header.chunk_count)
        };

        let allocator = PoolAllocator::new(chunk_count, chunk_size, segment_id, HEADER_SIZE);

        Ok(Self {
            shm,
            allocator,
            chunk_size,
            chunk_count,
        })
    }

    /// Allocate a chunk. Returns the pointer offset identifying the chunk.
    pub fn allocate(&self) -> Result<PointerOffset, LoanError> {
        self.allocator.allocate()
    }

    /// Deallocate a chunk.
    pub fn deallocate(&self, offset: PointerOffset) {
        self.allocator.deallocate(offset);
    }

    /// Resolve a pointer offset to a raw pointer within this segment.
    ///
    /// # Safety
    /// The caller must ensure the offset was allocated from this segment.
    pub unsafe fn resolve_ptr(&self, offset: PointerOffset) -> NonNull<u8> {
        let byte_offset = offset.offset();
        debug_assert!(byte_offset >= HEADER_SIZE);
        debug_assert!(byte_offset + self.chunk_size <= self.shm.size());
        NonNull::new_unchecked(self.shm.as_mut_ptr().add(byte_offset))
    }

    /// Resolve a pointer offset to a typed reference.
    ///
    /// # Safety
    /// The caller must ensure:
    /// - The offset was allocated from this segment
    /// - The memory is properly initialized for type T
    /// - No mutable aliases exist
    pub unsafe fn resolve_ref<T: Copy>(&self, offset: PointerOffset) -> &T {
        let ptr = self.resolve_ptr(offset);
        &*(ptr.as_ptr() as *const T)
    }

    /// Resolve a pointer offset to a mutable typed reference.
    ///
    /// # Safety
    /// Same as `resolve_ref`, plus exclusive access required.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn resolve_mut<T: Copy>(&self, offset: PointerOffset) -> &mut T {
        let ptr = self.resolve_ptr(offset);
        &mut *(ptr.as_ptr() as *mut T)
    }

    /// Number of allocated chunks.
    pub fn allocated_count(&self) -> u32 {
        self.allocator.allocated_count()
    }

    /// Number of available chunks.
    pub fn available(&self) -> u32 {
        self.allocator.available()
    }

    /// Chunk size in bytes.
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Total chunk count.
    pub fn chunk_count(&self) -> u32 {
        self.chunk_count
    }

    /// The underlying shared memory segment name.
    pub fn shm_name(&self) -> &str {
        self.shm.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_name(test: &str) -> String {
        use std::time::SystemTime;
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("entangle_ds_{test}_{ts}")
    }

    #[test]
    fn create_allocate_write_read() {
        let name = unique_name("basic");
        let ds = DataSegment::create(&name, 64, 4, 0).unwrap();

        let offset = ds.allocate().unwrap();

        // Safety: we just allocated, exclusive access in test
        unsafe {
            let val: &mut u64 = ds.resolve_mut(offset);
            *val = 0xCAFE_BABE;
        }

        unsafe {
            let val: &u64 = ds.resolve_ref(offset);
            assert_eq!(*val, 0xCAFE_BABE);
        }

        ds.deallocate(offset);
        assert_eq!(ds.available(), 4);
    }

    #[test]
    fn exhaust_and_recover() {
        let name = unique_name("exhaust");
        let ds = DataSegment::create(&name, 32, 2, 0).unwrap();

        let o1 = ds.allocate().unwrap();
        let o2 = ds.allocate().unwrap();
        assert!(ds.allocate().is_err());

        ds.deallocate(o1);
        let o3 = ds.allocate().unwrap();
        assert!(ds.allocate().is_err());

        ds.deallocate(o2);
        ds.deallocate(o3);
    }
}
