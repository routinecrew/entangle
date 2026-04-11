// Canonical source: contracts/shared_types.rs
// Local copy for independent build.

pub use std::num::NonZeroUsize;

use serde::{Deserialize, Serialize};

/// Shared memory safe type marker.
///
/// # Safety
/// Implementing types must be `#[repr(C)]` or `#[repr(transparent)]`,
/// with all fields also `ZeroCopySafe`, no heap allocations, no `Drop`.
pub unsafe trait ZeroCopySafe: Copy + Send + Sync + 'static {}

unsafe impl ZeroCopySafe for u8 {}
unsafe impl ZeroCopySafe for u16 {}
unsafe impl ZeroCopySafe for u32 {}
unsafe impl ZeroCopySafe for u64 {}
unsafe impl ZeroCopySafe for u128 {}
unsafe impl ZeroCopySafe for i8 {}
unsafe impl ZeroCopySafe for i16 {}
unsafe impl ZeroCopySafe for i32 {}
unsafe impl ZeroCopySafe for i64 {}
unsafe impl ZeroCopySafe for i128 {}
unsafe impl ZeroCopySafe for f32 {}
unsafe impl ZeroCopySafe for f64 {}
unsafe impl ZeroCopySafe for bool {}
unsafe impl<T: ZeroCopySafe, const N: usize> ZeroCopySafe for [T; N] {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct NodeId(pub u128);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServiceName(pub(crate) String);

impl ServiceName {
    /// Validate and create a service name.
    /// Max 255 bytes, ASCII alphanumeric + `/`, `-`, `_`, `.`.
    pub fn new(name: &str) -> Result<Self, String> {
        if name.is_empty() || name.len() > 255 {
            return Err("service name must be 1-255 bytes".into());
        }
        if !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
        {
            return Err("service name contains invalid characters".into());
        }
        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PubSubQos {
    pub history_size: usize,
    pub max_publishers: usize,
    pub max_subscribers: usize,
    pub subscriber_overflow: OverflowStrategy,
    pub max_loaned_samples: usize,
    pub buffer_size: NonZeroUsize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverflowStrategy {
    Overwrite,
    DropNewest,
    Block,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventQos {
    pub max_notifiers: usize,
    pub max_listeners: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReqResQos {
    pub max_clients: usize,
    pub max_servers: usize,
    pub max_pending_requests: usize,
}

pub const MAGIC_NUMBER: u64 = 0x5A43_4950_0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum PatternType {
    PubSub = 1,
    Event = 2,
    ReqRes = 3,
    Blackboard = 4,
}
