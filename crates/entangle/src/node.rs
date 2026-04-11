use entangle_platform::{ProcessMonitor, SignalHandler};
use tracing::debug;

use crate::config::EntangleConfig;
use crate::contracts::NodeId;
use crate::error::IpcError;
use crate::service::blackboard::BlackboardBuilder;
use crate::service::event::EventBuilder;
use crate::service::pubsub::PubSubBuilder;
use crate::service::reqres::ReqResBuilder;

/// A Node represents a process participating in entangle IPC.
///
/// Each node has a unique ID, registers with the process monitor,
/// and can create/open services. When the node is dropped, its
/// process monitor registration is released.
pub struct Node {
    id: NodeId,
    config: EntangleConfig,
    monitor: ProcessMonitor,
    _signal_handler: SignalHandler,
}

impl Node {
    /// Create a new Node builder.
    pub fn builder() -> NodeBuilder {
        NodeBuilder {
            config: EntangleConfig::default(),
            name: None,
        }
    }

    /// Unique node identifier.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Node name (if set).
    pub fn name(&self) -> Option<&str> {
        self.config.node_name.as_deref()
    }

    /// Create a service builder for the given service name.
    pub fn service(&self, name: &str) -> ServiceSelector {
        ServiceSelector {
            service_name: name.to_string(),
            config: self.config.clone(),
            node_id: self.id,
        }
    }

    /// The configuration for this node.
    pub fn config(&self) -> &EntangleConfig {
        &self.config
    }

    /// Check if a shutdown signal has been received.
    pub fn shutdown_requested(&self) -> bool {
        SignalHandler::shutdown_requested()
    }

    fn generate_id() -> NodeId {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::SystemTime;

        let mut hasher = DefaultHasher::new();
        std::process::id().hash(&mut hasher);
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .hash(&mut hasher);
        // Use two hashes to fill u128
        let h1 = hasher.finish();
        std::thread::current().id().hash(&mut hasher);
        let h2 = hasher.finish();
        NodeId(((h1 as u128) << 64) | (h2 as u128))
    }
}

/// Builder for creating a Node.
pub struct NodeBuilder {
    config: EntangleConfig,
    name: Option<String>,
}

impl NodeBuilder {
    /// Set the node name.
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Load configuration from a RON file.
    pub fn config_file(mut self, path: &str) -> Self {
        self.config = EntangleConfig::load(path);
        self
    }

    /// Set configuration directly.
    pub fn config(mut self, config: EntangleConfig) -> Self {
        self.config = config;
        self
    }

    /// Create the node.
    pub fn create(mut self) -> Result<Node, IpcError> {
        if let Some(ref name) = self.name {
            self.config.node_name = Some(name.clone());
        }

        let id = Node::generate_id();
        let root = self.config.shm_root_path();

        let platform_id = entangle_platform::contracts::NodeId(id.0);
        let monitor = ProcessMonitor::register(platform_id, &root)?;
        let signal_handler = SignalHandler::install()?;

        debug!(
            node_id = %format!("{:032x}", id.0),
            name = ?self.config.node_name,
            "node created"
        );

        Ok(Node {
            id,
            config: self.config,
            monitor,
            _signal_handler: signal_handler,
        })
    }
}

/// Intermediate type for selecting a messaging pattern.
pub struct ServiceSelector {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
}

impl ServiceSelector {
    /// Select the Publish-Subscribe pattern.
    pub fn publish_subscribe<T: crate::contracts::ZeroCopySafe>(self) -> PubSubBuilder<T> {
        PubSubBuilder::new(self.service_name, self.config, self.node_id)
    }

    /// Select the Event pattern.
    pub fn event(self) -> EventBuilder {
        EventBuilder::new(self.service_name, self.config, self.node_id)
    }

    /// Select the Request-Response pattern.
    pub fn request_response<Req, Res>(self) -> ReqResBuilder<Req, Res>
    where
        Req: crate::contracts::ZeroCopySafe,
        Res: crate::contracts::ZeroCopySafe,
    {
        ReqResBuilder::new(self.service_name, self.config, self.node_id)
    }

    /// Select the Blackboard (shared latest-value) pattern.
    pub fn blackboard<T: crate::contracts::ZeroCopySafe>(self) -> BlackboardBuilder<T> {
        BlackboardBuilder::new(self.service_name, self.config, self.node_id)
    }
}
