use entangle_lockfree::UniqueIndexSet;

use crate::contracts::{LoanError, PointerOffset};

/// Pool allocator for fixed-size chunks within a shared memory data segment.
///
/// Each chunk has a fixed size and alignment. Allocation/deallocation is
/// lock-free via `UniqueIndexSet`.
pub struct PoolAllocator {
    index_set: UniqueIndexSet,
    chunk_size: usize,
    segment_id: u16,
    base_offset: usize,
}

impl PoolAllocator {
    /// Create a new pool allocator.
    ///
    /// - `capacity`: number of chunks
    /// - `chunk_size`: size of each chunk in bytes (must be > 0)
    /// - `segment_id`: segment ID for pointer offset encoding
    /// - `base_offset`: byte offset within the segment where chunks start
    pub fn new(capacity: u32, chunk_size: usize, segment_id: u16, base_offset: usize) -> Self {
        assert!(chunk_size > 0);
        Self {
            index_set: UniqueIndexSet::new(capacity),
            chunk_size,
            segment_id,
            base_offset,
        }
    }

    /// Allocate a chunk. Returns a `PointerOffset` identifying the chunk.
    pub fn allocate(&self) -> Result<PointerOffset, LoanError> {
        let index = self
            .index_set
            .acquire()
            .map_err(|_| LoanError::OutOfMemory)?;
        let offset = self.base_offset + (index as usize) * self.chunk_size;
        Ok(PointerOffset::new(self.segment_id, offset))
    }

    /// Deallocate a chunk by its pointer offset.
    pub fn deallocate(&self, offset: PointerOffset) {
        let local = offset.offset() - self.base_offset;
        let index = (local / self.chunk_size) as u32;
        self.index_set.release(index);
    }

    /// Number of currently allocated chunks.
    pub fn allocated_count(&self) -> u32 {
        self.index_set.borrowed_count()
    }

    /// Number of available chunks.
    pub fn available(&self) -> u32 {
        self.index_set.available()
    }

    /// Total chunk capacity.
    pub fn capacity(&self) -> u32 {
        self.index_set.capacity()
    }

    /// Size of each chunk in bytes.
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Segment ID.
    pub fn segment_id(&self) -> u16 {
        self.segment_id
    }

    /// Resolve a pointer offset to a byte offset within the segment.
    pub fn resolve_offset(&self, offset: PointerOffset) -> usize {
        offset.offset()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_and_deallocate() {
        let alloc = PoolAllocator::new(4, 256, 0, 0);
        assert_eq!(alloc.available(), 4);

        let o1 = alloc.allocate().unwrap();
        let o2 = alloc.allocate().unwrap();
        assert_ne!(o1, o2);
        assert_eq!(alloc.allocated_count(), 2);

        alloc.deallocate(o1);
        assert_eq!(alloc.allocated_count(), 1);
        assert_eq!(alloc.available(), 3);
    }

    #[test]
    fn exhaust() {
        let alloc = PoolAllocator::new(2, 64, 0, 0);
        alloc.allocate().unwrap();
        alloc.allocate().unwrap();
        assert!(alloc.allocate().is_err());
    }

    #[test]
    fn offset_encoding() {
        let alloc = PoolAllocator::new(4, 1024, 3, 4096);
        let o = alloc.allocate().unwrap();
        assert_eq!(o.segment_id(), 3);
        assert_eq!(o.offset(), 4096); // first chunk at base_offset
    }
}
