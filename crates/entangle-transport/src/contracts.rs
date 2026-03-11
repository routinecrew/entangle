// contracts/shared_types.rs에서 복사한 전송 계층 관련 타입

/// 세그먼트 ID + 로컬 오프셋
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
}

/// 채널 상태
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelState {
    Creating = 0,
    Connected = 1,
    Disconnected = 2,
}

/// Loan 에러
#[derive(Debug)]
pub enum LoanError {
    OutOfMemory,
    ExceedsMaxLoans { max: usize },
}

/// Send 에러
#[derive(Debug)]
pub enum SendError {
    ConnectionBroken,
    QueueFull,
}
