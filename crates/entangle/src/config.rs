use std::num::NonZeroUsize;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::contracts::{EventQos, OverflowStrategy, PubSubQos};

/// Global entangle configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntangleConfig {
    /// Root directory for shared memory files and node locks.
    pub shm_root: String,
    /// Node name (optional, for debugging).
    pub node_name: Option<String>,
    /// Default PubSub QoS.
    pub default_pubsub_qos: PubSubQos,
    /// Default Event QoS.
    pub default_event_qos: EventQos,
}

impl Default for EntangleConfig {
    fn default() -> Self {
        Self {
            shm_root: "/tmp/entangle/".into(),
            node_name: None,
            default_pubsub_qos: PubSubQos::default(),
            default_event_qos: EventQos::default(),
        }
    }
}

impl EntangleConfig {
    /// Load config from a RON file, falling back to defaults.
    pub fn load(path: &str) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| ron::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Shared memory root as a `PathBuf`.
    pub fn shm_root_path(&self) -> PathBuf {
        PathBuf::from(&self.shm_root)
    }
}

impl Default for PubSubQos {
    fn default() -> Self {
        Self {
            history_size: 1,
            max_publishers: 8,
            max_subscribers: 16,
            subscriber_overflow: OverflowStrategy::Overwrite,
            max_loaned_samples: 4,
            buffer_size: NonZeroUsize::new(64).unwrap(),
        }
    }
}

impl Default for EventQos {
    fn default() -> Self {
        Self {
            max_notifiers: 8,
            max_listeners: 16,
        }
    }
}
