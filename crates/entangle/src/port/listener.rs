use entangle_platform::EventNotification;

use crate::contracts::NodeId;
use crate::error::IpcError;

/// Builder for creating a Listener port.
pub struct ListenerBuilder {
    service_name: String,
    node_id: NodeId,
}

impl ListenerBuilder {
    pub(crate) fn new(service_name: String, node_id: NodeId) -> Self {
        Self {
            service_name,
            node_id,
        }
    }

    pub fn create(self) -> Result<Listener, IpcError> {
        let event = EventNotification::new()?;
        Ok(Listener {
            service_name: self.service_name,
            event,
        })
    }
}

/// A Listener port that waits for event notifications.
pub struct Listener {
    service_name: String,
    event: EventNotification,
}

impl Listener {
    /// Block until a notification is received.
    pub fn wait(&self) -> Result<(), IpcError> {
        self.event.wait()?;
        Ok(())
    }

    /// Check for a pending notification without blocking.
    pub fn try_wait(&self) -> Result<bool, IpcError> {
        Ok(self.event.try_wait()?)
    }

    /// Raw file descriptor for the readable end (for use with WaitSet/poll).
    pub fn raw_fd(&self) -> std::os::fd::RawFd {
        self.event.read_raw_fd()
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}
