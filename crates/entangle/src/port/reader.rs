use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use entangle_transport::{DataSegment, PointerOffset};

use crate::contracts::{NodeId, ZeroCopySafe};
use crate::error::{IpcError, PortError};
use crate::service::blackboard::BlackboardShared;

/// Builder for creating a Reader port (Blackboard pattern).
pub struct ReaderBuilder<T: ZeroCopySafe> {
    service_name: String,
    node_id: NodeId,
    shared: Arc<BlackboardShared>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> ReaderBuilder<T> {
    pub(crate) fn new(
        service_name: String,
        node_id: NodeId,
        shared: Arc<BlackboardShared>,
    ) -> Self {
        Self {
            service_name,
            node_id,
            shared,
            _marker: PhantomData,
        }
    }

    pub fn create(self) -> Result<Reader<T>, IpcError> {
        let wiring = self.shared.inner.lock().unwrap();
        let writer_reg =
            wiring
                .writer
                .as_ref()
                .ok_or_else(|| crate::error::ServiceError::NotFound {
                    name: self.service_name.clone(),
                })?;

        Ok(Reader {
            service_name: self.service_name,
            node_id: self.node_id,
            segment: writer_reg.segment.clone(),
            data_offset: writer_reg.data_offset,
            sequence: writer_reg.sequence.clone(),
            _marker: PhantomData,
        })
    }
}

/// A Reader port for the Blackboard (shared state) pattern.
///
/// Reads the latest value written by the Writer using a seqlock
/// to ensure consistency.
pub struct Reader<T: ZeroCopySafe> {
    service_name: String,
    node_id: NodeId,
    segment: Arc<DataSegment>,
    data_offset: PointerOffset,
    /// Shared seqlock counter with the Writer.
    sequence: Arc<AtomicU64>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> Reader<T> {
    /// Read the latest value from the blackboard.
    ///
    /// Returns `None` if nothing has been written yet (sequence == 0).
    /// Retries automatically if a concurrent write is detected (seqlock).
    pub fn read(&self) -> Result<Option<T>, PortError> {
        loop {
            // Ordering: Acquire so we see the writer's data after the sequence.
            let s1 = self.sequence.load(Ordering::Acquire);
            if s1 == 0 {
                // No data has been written yet.
                return Ok(None);
            }
            if s1 & 1 != 0 {
                // Writer is in the middle of writing — spin retry.
                std::hint::spin_loop();
                continue;
            }

            // Safety: the writer has written valid T data at this offset.
            // We copy it (T: Copy) to avoid holding a reference across
            // the sequence recheck.
            let value = unsafe { *self.segment.resolve_ref::<T>(self.data_offset) };

            // Ordering: Acquire to pair with the writer's Release after write.
            let s2 = self.sequence.load(Ordering::Acquire);
            if s1 == s2 {
                // Sequence didn't change — we read a consistent value.
                return Ok(Some(value));
            }
            // Sequence changed during our read — retry.
            std::hint::spin_loop();
        }
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}
