use std::marker::PhantomData;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use entangle_transport::DataSegment;

use crate::config::EntangleConfig;
use crate::contracts::{NodeId, PatternType, ZeroCopySafe};
use crate::error::IpcError;
use crate::port::reader::ReaderBuilder;
use crate::port::writer::WriterBuilder;
use crate::service::config::StaticConfig;
use crate::service::lifecycle::ServiceLifecycle;

/// Shared state for the Blackboard pattern.
pub(crate) struct BlackboardShared {
    pub(crate) inner: Mutex<BlackboardWiring>,
    pub(crate) service_name: String,
}

pub(crate) struct BlackboardWiring {
    pub(crate) writer: Option<WriterRegistration>,
}

pub(crate) struct WriterRegistration {
    pub(crate) segment: Arc<DataSegment>,
    /// Seqlock sequence number. Even = idle, odd = writing.
    /// Starts at 0 (no data written yet).
    pub(crate) sequence: Arc<AtomicU64>,
    /// Offset of the data slot within the segment.
    pub(crate) data_offset: entangle_transport::PointerOffset,
}

/// Builder for creating/opening a Blackboard (shared state) service.
pub struct BlackboardBuilder<T: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> BlackboardBuilder<T> {
    pub(crate) fn new(service_name: String, config: EntangleConfig, node_id: NodeId) -> Self {
        Self {
            service_name,
            config,
            node_id,
            _marker: PhantomData,
        }
    }

    pub fn open_or_create(self) -> Result<BlackboardService<T>, IpcError> {
        let static_config = StaticConfig::new(
            PatternType::Blackboard,
            &self.service_name,
            std::any::type_name::<T>(),
            std::mem::size_of::<T>(),
            std::mem::align_of::<T>(),
        );

        let lifecycle = ServiceLifecycle::new(&self.config.shm_root_path());
        lifecycle.open_or_create(&static_config)?;

        let shared = Arc::new(BlackboardShared {
            inner: Mutex::new(BlackboardWiring { writer: None }),
            service_name: self.service_name.clone(),
        });

        Ok(BlackboardService {
            service_name: self.service_name,
            config: self.config,
            node_id: self.node_id,
            shared,
            _marker: PhantomData,
        })
    }
}

/// A Blackboard service instance (shared latest-value access).
pub struct BlackboardService<T: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    pub(crate) shared: Arc<BlackboardShared>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> BlackboardService<T> {
    pub fn writer(&self) -> WriterBuilder<T> {
        WriterBuilder::new(
            self.service_name.clone(),
            self.config.clone(),
            self.node_id,
            self.shared.clone(),
        )
    }

    pub fn reader(&self) -> ReaderBuilder<T> {
        ReaderBuilder::new(self.service_name.clone(), self.node_id, self.shared.clone())
    }

    pub fn name(&self) -> &str {
        &self.service_name
    }
}
