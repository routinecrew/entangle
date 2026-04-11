use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use entangle_transport::{DataSegment, PointerOffset};
use tracing::debug;

use crate::config::EntangleConfig;
use crate::contracts::{NodeId, ZeroCopySafe};
use crate::error::{IpcError, PortError};
use crate::service::blackboard::{BlackboardShared, WriterRegistration};

/// Builder for creating a Writer port (Blackboard pattern).
pub struct WriterBuilder<T: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    shared: Arc<BlackboardShared>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> WriterBuilder<T> {
    pub(crate) fn new(
        service_name: String,
        config: EntangleConfig,
        node_id: NodeId,
        shared: Arc<BlackboardShared>,
    ) -> Self {
        Self {
            service_name,
            config,
            node_id,
            shared,
            _marker: PhantomData,
        }
    }

    pub fn create(self) -> Result<Writer<T>, IpcError> {
        let chunk_size = std::mem::size_of::<T>().max(8);

        let seg_name = format!(
            "entangle_{}_bb_{:032x}",
            self.service_name.replace('/', "_"),
            self.node_id.0
        );

        // Single chunk for the blackboard value.
        let segment = Arc::new(DataSegment::create(&seg_name, chunk_size, 1, 0)?);

        let data_offset =
            segment
                .allocate()
                .map_err(|e| entangle_platform::PlatformError::ProcessMonitor {
                    reason: format!("blackboard allocate: {e}"),
                })?;

        let sequence = Arc::new(AtomicU64::new(0));

        // Register writer in shared state.
        {
            let mut wiring = self.shared.inner.lock().unwrap();
            if wiring.writer.is_some() {
                return Err(crate::error::ServiceError::AlreadyExists {
                    name: self.service_name.clone(),
                }
                .into());
            }
            wiring.writer = Some(WriterRegistration {
                segment: segment.clone(),
                sequence: sequence.clone(),
                data_offset,
            });
        }

        debug!(service = %self.service_name, "blackboard writer created");

        Ok(Writer {
            service_name: self.service_name,
            node_id: self.node_id,
            segment,
            data_offset,
            sequence,
            _marker: PhantomData,
        })
    }
}

/// A Writer port for the Blackboard (shared state) pattern.
///
/// Uses a seqlock to ensure readers see consistent values.
/// Only one writer is allowed per blackboard service.
pub struct Writer<T: ZeroCopySafe> {
    service_name: String,
    node_id: NodeId,
    segment: Arc<DataSegment>,
    data_offset: PointerOffset,
    /// Seqlock counter. Even = idle, odd = writing.
    sequence: Arc<AtomicU64>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> Writer<T> {
    /// Write a new value to the blackboard.
    ///
    /// Uses a seqlock: increments sequence to odd (writing), writes data,
    /// then increments to even (done). Readers retry if they see an odd
    /// sequence or if the sequence changed during their read.
    pub fn write(&self, value: &T) -> Result<(), PortError> {
        let seq = self.sequence.load(Ordering::Relaxed);

        // Begin write phase (sequence becomes odd).
        // Ordering: Release so readers see the sequence update before
        // any partial data writes.
        self.sequence.store(seq.wrapping_add(1), Ordering::Release);

        // Write the value to shared memory.
        // Safety: we are the sole writer, data_offset was allocated from this segment.
        unsafe {
            let ptr = self.segment.resolve_mut::<T>(self.data_offset);
            std::ptr::write(ptr, *value);
        }

        // End write phase (sequence becomes even).
        // Ordering: Release so readers see all data writes before the
        // sequence update.
        self.sequence.store(seq.wrapping_add(2), Ordering::Release);

        Ok(())
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}
