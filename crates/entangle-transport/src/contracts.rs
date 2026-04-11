use thiserror::Error;

/// Segment ID + local offset packed into a single u64.
///
/// Bit layout: \[segment_id:16\]\[offset:48\]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct PointerOffset(u64);

impl PointerOffset {
    pub fn new(segment_id: u16, offset: usize) -> Self {
        debug_assert!(offset < (1 << 48));
        Self(((segment_id as u64) << 48) | (offset as u64 & 0x0000_FFFF_FFFF_FFFF))
    }

    pub fn segment_id(self) -> u16 {
        (self.0 >> 48) as u16
    }

    pub fn offset(self) -> usize {
        (self.0 & 0x0000_FFFF_FFFF_FFFF) as usize
    }

    pub fn raw(self) -> u64 {
        self.0
    }

    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// Channel state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelState {
    Creating = 0,
    Connected = 1,
    Disconnected = 2,
}

/// Loan error.
#[derive(Error, Debug, Clone)]
pub enum LoanError {
    #[error("out of memory in data segment")]
    OutOfMemory,
    #[error("max loaned samples ({max}) exceeded")]
    ExceedsMaxLoans { max: usize },
}

/// Send error.
#[derive(Error, Debug, Clone)]
pub enum SendError {
    #[error("connection broken: receiver no longer exists")]
    ConnectionBroken,
    #[error("receiver queue is full")]
    QueueFull,
}

/// Receive error.
#[derive(Error, Debug, Clone)]
pub enum ReceiveError {
    #[error("no data available")]
    Empty,
    #[error("connection broken: sender no longer exists")]
    ConnectionBroken,
}
