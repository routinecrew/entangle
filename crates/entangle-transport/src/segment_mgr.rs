use crate::contracts::{LoanError, PointerOffset};
use crate::data_segment::DataSegment;

/// Manages multiple data segments for dynamic capacity expansion.
///
/// When the initial segment runs out of chunks, additional segments
/// can be added. Each segment has a unique ID embedded in the PointerOffset,
/// allowing resolution across segments.
pub struct SegmentManager {
    segments: Vec<DataSegment>,
    name_prefix: String,
    chunk_size: usize,
    chunks_per_segment: u32,
}

impl SegmentManager {
    /// Create a segment manager with an initial segment.
    pub fn create(
        name_prefix: &str,
        chunk_size: usize,
        chunks_per_segment: u32,
    ) -> Result<Self, entangle_platform::PlatformError> {
        let seg_name = format!("{name_prefix}_seg_0");
        let initial = DataSegment::create(&seg_name, chunk_size, chunks_per_segment, 0)?;

        Ok(Self {
            segments: vec![initial],
            name_prefix: name_prefix.to_string(),
            chunk_size,
            chunks_per_segment,
        })
    }

    /// Allocate a chunk, expanding to a new segment if necessary.
    pub fn allocate(&mut self) -> Result<PointerOffset, LoanError> {
        // Try existing segments
        for seg in &self.segments {
            if let Ok(offset) = seg.allocate() {
                return Ok(offset);
            }
        }

        // All full — create a new segment
        let seg_id = self.segments.len() as u16;
        let seg_name = format!("{}_seg_{seg_id}", self.name_prefix);
        let new_seg =
            DataSegment::create(&seg_name, self.chunk_size, self.chunks_per_segment, seg_id)
                .map_err(|_| LoanError::OutOfMemory)?;

        let offset = new_seg.allocate()?;
        self.segments.push(new_seg);
        Ok(offset)
    }

    /// Deallocate a chunk.
    pub fn deallocate(&self, offset: PointerOffset) {
        let seg_id = offset.segment_id() as usize;
        if seg_id < self.segments.len() {
            self.segments[seg_id].deallocate(offset);
        }
    }

    /// Resolve a pointer offset to a raw pointer.
    ///
    /// # Safety
    /// The offset must have been allocated from this manager.
    pub unsafe fn resolve_ptr(&self, offset: PointerOffset) -> std::ptr::NonNull<u8> {
        let seg_id = offset.segment_id() as usize;
        self.segments[seg_id].resolve_ptr(offset)
    }

    /// Resolve to a typed reference.
    ///
    /// # Safety
    /// Same requirements as `DataSegment::resolve_ref`.
    pub unsafe fn resolve_ref<T: Copy>(&self, offset: PointerOffset) -> &T {
        let seg_id = offset.segment_id() as usize;
        self.segments[seg_id].resolve_ref(offset)
    }

    /// Resolve to a mutable typed reference.
    ///
    /// # Safety
    /// Same requirements as `DataSegment::resolve_mut`.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn resolve_mut<T: Copy>(&self, offset: PointerOffset) -> &mut T {
        let seg_id = offset.segment_id() as usize;
        self.segments[seg_id].resolve_mut(offset)
    }

    /// Total number of segments.
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Total available chunks across all segments.
    pub fn total_available(&self) -> u32 {
        self.segments.iter().map(|s| s.available()).sum()
    }

    /// Total allocated chunks across all segments.
    pub fn total_allocated(&self) -> u32 {
        self.segments.iter().map(|s| s.allocated_count()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_prefix(test: &str) -> String {
        use std::time::SystemTime;
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("entangle_sm_{test}_{ts}")
    }

    #[test]
    fn single_segment() {
        let prefix = unique_prefix("single");
        let mut mgr = SegmentManager::create(&prefix, 64, 4).unwrap();

        let o = mgr.allocate().unwrap();
        assert_eq!(o.segment_id(), 0);

        unsafe {
            let v: &mut u64 = mgr.resolve_mut(o);
            *v = 42;
            assert_eq!(*mgr.resolve_ref::<u64>(o), 42);
        }

        mgr.deallocate(o);
    }

    #[test]
    fn auto_expand() {
        let prefix = unique_prefix("expand");
        let mut mgr = SegmentManager::create(&prefix, 32, 2).unwrap();

        let o1 = mgr.allocate().unwrap();
        let o2 = mgr.allocate().unwrap();
        assert_eq!(mgr.segment_count(), 1);

        // This should trigger segment expansion
        let o3 = mgr.allocate().unwrap();
        assert_eq!(mgr.segment_count(), 2);
        assert_eq!(o3.segment_id(), 1);

        mgr.deallocate(o1);
        mgr.deallocate(o2);
        mgr.deallocate(o3);
    }
}
