use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use tracing::debug;

use entangle_transport::{DataSegment, ZeroCopyChannel};

use crate::config::EntangleConfig;
use crate::contracts::{NodeId, OverflowStrategy, PatternType, PubSubQos, ZeroCopySafe};
use crate::error::IpcError;
use crate::port::publisher::PublisherBuilder;
use crate::port::subscriber::SubscriberBuilder;
use crate::service::config::StaticConfig;
use crate::service::lifecycle::ServiceLifecycle;

/// Shared state connecting publishers and subscribers within a PubSub service.
pub(crate) struct PubSubShared {
    pub(crate) inner: Mutex<PubSubWiring>,
    pub(crate) service_name: String,
    pub(crate) qos: PubSubQos,
    /// Monotonic counter for unique publisher segment names.
    pub(crate) publisher_counter: std::sync::atomic::AtomicU32,
}

pub(crate) struct PubSubWiring {
    pub(crate) publishers: Vec<PublisherRegistration>,
}

pub(crate) struct PublisherRegistration {
    pub(crate) node_id: NodeId,
    pub(crate) segment: Arc<DataSegment>,
    /// Channels to subscribers (publisher is send side). Grows when new subscribers connect.
    pub(crate) channels: Arc<Mutex<Vec<Arc<ZeroCopyChannel>>>>,
}

/// Builder for creating/opening a Publish-Subscribe service.
pub struct PubSubBuilder<T: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    qos: PubSubQos,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> PubSubBuilder<T> {
    pub(crate) fn new(service_name: String, config: EntangleConfig, node_id: NodeId) -> Self {
        let qos = config.default_pubsub_qos.clone();
        Self {
            service_name,
            config,
            node_id,
            qos,
            _marker: PhantomData,
        }
    }

    /// Set the history size (number of past samples delivered to new subscribers).
    pub fn history_size(mut self, size: usize) -> Self {
        self.qos.history_size = size;
        self
    }

    /// Set the maximum number of publishers.
    pub fn max_publishers(mut self, n: usize) -> Self {
        self.qos.max_publishers = n;
        self
    }

    /// Set the maximum number of subscribers.
    pub fn max_subscribers(mut self, n: usize) -> Self {
        self.qos.max_subscribers = n;
        self
    }

    /// Set the subscriber overflow strategy.
    pub fn subscriber_overflow(mut self, strategy: OverflowStrategy) -> Self {
        self.qos.subscriber_overflow = strategy;
        self
    }

    /// Set the maximum number of loaned samples per publisher.
    pub fn max_loaned_samples(mut self, n: usize) -> Self {
        self.qos.max_loaned_samples = n;
        self
    }

    /// Open or create the service.
    pub fn open_or_create(self) -> Result<PubSubService<T>, IpcError> {
        let static_config = StaticConfig::new(
            PatternType::PubSub,
            &self.service_name,
            std::any::type_name::<T>(),
            std::mem::size_of::<T>(),
            std::mem::align_of::<T>(),
        );

        let lifecycle = ServiceLifecycle::new(&self.config.shm_root_path());
        let _config = lifecycle.open_or_create(&static_config)?;

        debug!(
            service = %self.service_name,
            payload_type = %std::any::type_name::<T>(),
            payload_size = std::mem::size_of::<T>(),
            "pubsub service ready"
        );

        let shared = Arc::new(PubSubShared {
            inner: Mutex::new(PubSubWiring {
                publishers: Vec::new(),
            }),
            service_name: self.service_name.clone(),
            qos: self.qos.clone(),
            publisher_counter: std::sync::atomic::AtomicU32::new(0),
        });

        Ok(PubSubService {
            service_name: self.service_name,
            config: self.config,
            node_id: self.node_id,
            qos: self.qos,
            shared,
            _marker: PhantomData,
        })
    }

    /// Create the service (fail if exists).
    pub fn create(self) -> Result<PubSubService<T>, IpcError> {
        let static_config = StaticConfig::new(
            PatternType::PubSub,
            &self.service_name,
            std::any::type_name::<T>(),
            std::mem::size_of::<T>(),
            std::mem::align_of::<T>(),
        );

        let lifecycle = ServiceLifecycle::new(&self.config.shm_root_path());
        lifecycle.create(&static_config)?;

        let shared = Arc::new(PubSubShared {
            inner: Mutex::new(PubSubWiring {
                publishers: Vec::new(),
            }),
            service_name: self.service_name.clone(),
            qos: self.qos.clone(),
            publisher_counter: std::sync::atomic::AtomicU32::new(0),
        });

        Ok(PubSubService {
            service_name: self.service_name,
            config: self.config,
            node_id: self.node_id,
            qos: self.qos,
            shared,
            _marker: PhantomData,
        })
    }

    /// Open an existing service (fail if not found).
    pub fn open(self) -> Result<PubSubService<T>, IpcError> {
        let static_config = StaticConfig::new(
            PatternType::PubSub,
            &self.service_name,
            std::any::type_name::<T>(),
            std::mem::size_of::<T>(),
            std::mem::align_of::<T>(),
        );

        let lifecycle = ServiceLifecycle::new(&self.config.shm_root_path());
        lifecycle.open(&static_config)?;

        let shared = Arc::new(PubSubShared {
            inner: Mutex::new(PubSubWiring {
                publishers: Vec::new(),
            }),
            service_name: self.service_name.clone(),
            qos: self.qos.clone(),
            publisher_counter: std::sync::atomic::AtomicU32::new(0),
        });

        Ok(PubSubService {
            service_name: self.service_name,
            config: self.config,
            node_id: self.node_id,
            qos: self.qos,
            shared,
            _marker: PhantomData,
        })
    }
}

/// A Publish-Subscribe service instance.
pub struct PubSubService<T: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    qos: PubSubQos,
    pub(crate) shared: Arc<PubSubShared>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> PubSubService<T> {
    /// Create a publisher builder.
    pub fn publisher(&self) -> PublisherBuilder<T> {
        PublisherBuilder::new(
            self.service_name.clone(),
            self.config.clone(),
            self.node_id,
            self.qos.clone(),
            self.shared.clone(),
        )
    }

    /// Create a subscriber builder.
    pub fn subscriber(&self) -> SubscriberBuilder<T> {
        SubscriberBuilder::new(
            self.service_name.clone(),
            self.config.clone(),
            self.node_id,
            self.qos.clone(),
            self.shared.clone(),
        )
    }

    /// Service name.
    pub fn name(&self) -> &str {
        &self.service_name
    }
}
