use entangle_platform::EventNotification;

use crate::contracts::NodeId;
use crate::error::IpcError;

/// Builder for creating a Notifier port.
pub struct NotifierBuilder {
    service_name: String,
    node_id: NodeId,
}

impl NotifierBuilder {
    pub(crate) fn new(service_name: String, node_id: NodeId) -> Self {
        Self {
            service_name,
            node_id,
        }
    }

    pub fn create(self) -> Result<Notifier, IpcError> {
        let event = EventNotification::new()?;
        Ok(Notifier {
            service_name: self.service_name,
            event,
        })
    }
}

/// A Notifier port for lightweight event signaling.
pub struct Notifier {
    service_name: String,
    event: EventNotification,
}

impl Notifier {
    /// Send an event notification to all listeners.
    pub fn notify(&self) -> Result<(), IpcError> {
        self.event.notify()?;
        Ok(())
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}
