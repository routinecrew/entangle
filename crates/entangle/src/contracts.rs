// contracts/shared_types.rs 전체 복사 — 독립 빌드용
// canonical source: contracts/shared_types.rs

pub use std::num::NonZeroUsize;

/// 공유 메모리 안전 전송 마커
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct PointerOffset(u64);

impl PointerOffset {
    pub fn new(segment_id: u16, offset: usize) -> Self {
        debug_assert!(offset < (1 << 48));
        Self(((segment_id as u64) << 48) | (offset as u64 & 0x0000_FFFF_FFFF_FFFF))
    }
    pub fn segment_id(self) -> u16 { (self.0 >> 48) as u16 }
    pub fn offset(self) -> usize { (self.0 & 0x0000_FFFF_FFFF_FFFF) as usize }
}

#[derive(Clone, Debug)]
pub struct PubSubQos {
    pub history_size: usize,
    pub max_publishers: usize,
    pub max_subscribers: usize,
    pub subscriber_overflow: OverflowStrategy,
    pub max_loaned_samples: usize,
    pub buffer_size: NonZeroUsize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverflowStrategy {
    Overwrite,
    DropNewest,
    Block,
}

#[derive(Clone, Debug)]
pub struct EventQos {
    pub max_notifiers: usize,
    pub max_listeners: usize,
}

#[derive(Clone, Debug)]
pub struct ReqResQos {
    pub max_clients: usize,
    pub max_servers: usize,
    pub max_pending_requests: usize,
}

pub const MAGIC_NUMBER: u64 = 0x5A43_4950_0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PatternType {
    PubSub = 1,
    Event = 2,
    ReqRes = 3,
    Blackboard = 4,
}
