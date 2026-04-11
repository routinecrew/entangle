use tracing::debug;

use crate::config::EntangleConfig;
use crate::contracts::{EventQos, NodeId, PatternType};
use crate::error::IpcError;
use crate::port::listener::ListenerBuilder;
use crate::port::notifier::NotifierBuilder;
use crate::service::config::StaticConfig;
use crate::service::lifecycle::ServiceLifecycle;

/// Builder for creating/opening an Event service.
pub struct EventBuilder {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    qos: EventQos,
}

impl EventBuilder {
    pub(crate) fn new(service_name: String, config: EntangleConfig, node_id: NodeId) -> Self {
        let qos = config.default_event_qos.clone();
        Self {
            service_name,
            config,
            node_id,
            qos,
        }
    }

    pub fn max_notifiers(mut self, n: usize) -> Self {
        self.qos.max_notifiers = n;
        self
    }

    pub fn max_listeners(mut self, n: usize) -> Self {
        self.qos.max_listeners = n;
        self
    }

    pub fn open_or_create(self) -> Result<EventService, IpcError> {
        let static_config =
            StaticConfig::new(PatternType::Event, &self.service_name, "event", 0, 1);

        let lifecycle = ServiceLifecycle::new(&self.config.shm_root_path());
        lifecycle.open_or_create(&static_config)?;

        debug!(service = %self.service_name, "event service ready");

        Ok(EventService {
            service_name: self.service_name,
            config: self.config,
            node_id: self.node_id,
            qos: self.qos,
        })
    }
}

/// An Event service instance.
pub struct EventService {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    qos: EventQos,
}

impl EventService {
    /// Create a notifier builder.
    pub fn notifier(&self) -> NotifierBuilder {
        NotifierBuilder::new(self.service_name.clone(), self.node_id)
    }

    /// Create a listener builder.
    pub fn listener(&self) -> ListenerBuilder {
        ListenerBuilder::new(self.service_name.clone(), self.node_id)
    }

    pub fn name(&self) -> &str {
        &self.service_name
    }
}
