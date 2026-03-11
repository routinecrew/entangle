// ============================================================
// entangle 공유 계약 (Shared Contracts)
// ============================================================
// 모든 에이전트는 이 파일의 타입과 trait을 기준으로 개발한다.
// 이 파일을 수정하려면 반드시 모든 에이전트에게 알려야 한다.
// ============================================================

use std::num::NonZeroUsize;

// ============================================================
// 1. ZeroCopySafe — 공유 메모리 안전 전송 마커 trait
// ============================================================

/// 공유 메모리를 통해 안전하게 전송할 수 있는 타입 마커.
///
/// # Safety
/// 이 trait를 구현하는 타입은 다음을 보장해야 한다:
/// - `#[repr(C)]` 또는 `#[repr(transparent)]`
/// - 모든 필드가 `ZeroCopySafe`
/// - 힙 할당 참조 없음 (포인터, Box, Vec, String 등 불가)
/// - `Drop` 구현 없음
pub unsafe trait ZeroCopySafe: Copy + Send + Sync + 'static {}

// 기본 타입 구현
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

// ============================================================
// 2. NodeId — 프로세스/노드 식별자
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct NodeId(pub u128);

// ============================================================
// 3. ServiceName — 서비스 이름 (검증된 문자열)
// ============================================================

/// 서비스 이름. 최대 255바이트, ASCII 영숫자 + `/`, `-`, `_`, `.` 허용.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServiceName(String);

// ============================================================
// 4. PointerOffset — 공유 메모리 내 오프셋
// ============================================================

/// 세그먼트 ID + 로컬 오프셋으로 구성된 포인터 오프셋.
/// 비트 레이아웃: [segment_id:16][offset:48]
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

// ============================================================
// 5. QoS 설정
// ============================================================

/// Publish-Subscribe QoS 설정
#[derive(Clone, Debug)]
pub struct PubSubQos {
    /// 새 구독자에게 전달할 히스토리 크기
    pub history_size: usize,
    /// 최대 발행자 수
    pub max_publishers: usize,
    /// 최대 구독자 수
    pub max_subscribers: usize,
    /// 구독자가 느릴 때 전략
    pub subscriber_overflow: OverflowStrategy,
    /// 발행자당 최대 loan 수
    pub max_loaned_samples: usize,
    /// 채널 버퍼 크기
    pub buffer_size: NonZeroUsize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverflowStrategy {
    /// 가장 오래된 샘플을 덮어씀
    Overwrite,
    /// 새 샘플을 버림
    DropNewest,
    /// 블로킹 (데드락 주의)
    Block,
}

/// Event QoS 설정
#[derive(Clone, Debug)]
pub struct EventQos {
    pub max_notifiers: usize,
    pub max_listeners: usize,
}

/// Request-Response QoS 설정
#[derive(Clone, Debug)]
pub struct ReqResQos {
    pub max_clients: usize,
    pub max_servers: usize,
    pub max_pending_requests: usize,
}

// ============================================================
// 6. 에러 피라미드
// ============================================================

/// 최상위 에러
#[derive(Debug)]
pub enum IpcError {
    Service(ServiceError),
    Port(PortError),
    Platform(PlatformError),
}

/// 서비스 계층 에러
#[derive(Debug)]
pub enum ServiceError {
    AlreadyExists { name: String },
    NotFound { name: String },
    IncompatibleQos { reason: String },
    Corrupted { reason: String },
    VersionMismatch { expected: u32, found: u32 },
}

/// 포트 계층 에러
#[derive(Debug)]
pub enum PortError {
    Loan(LoanError),
    Send(SendError),
    Receive(ReceiveError),
    ConnectionLost { peer_id: u128 },
}

#[derive(Debug)]
pub enum LoanError {
    OutOfMemory,
    ExceedsMaxLoans { max: usize },
}

#[derive(Debug)]
pub enum SendError {
    ConnectionBroken,
    QueueFull,
}

#[derive(Debug)]
pub enum ReceiveError {
    Empty,
    ConnectionBroken,
}

#[derive(Debug)]
pub enum PlatformError {
    SharedMemoryCreate { reason: String },
    SharedMemoryOpen { reason: String },
    FileLock { reason: String },
    Signal { reason: String },
}

// ============================================================
// 7. Platform trait — 플랫폼 추상화 인터페이스
// ============================================================

/// 공유 메모리 관리 인터페이스
pub trait SharedMemoryProvider: Send + Sync {
    fn create(&self, name: &str, size: usize) -> Result<SharedMemoryHandle, PlatformError>;
    fn open(&self, name: &str) -> Result<SharedMemoryHandle, PlatformError>;
    fn unlink(&self, name: &str) -> Result<(), PlatformError>;
}

/// 공유 메모리 핸들 (구체적 구현은 entangle-platform에서)
pub struct SharedMemoryHandle {
    pub ptr: *mut u8,
    pub size: usize,
    pub name: String,
}

// SharedMemoryHandle은 포인터를 포함하지만 Send/Sync가 필요
// Safety: SharedMemory는 프로세스 간 공유를 위한 것이며,
// 내부 접근은 lock-free 자료구조로 동기화된다.
unsafe impl Send for SharedMemoryHandle {}
unsafe impl Sync for SharedMemoryHandle {}

/// Lock-free 자료구조 인터페이스 (entangle-lockfree에서 구현)
pub trait IndexAllocator: Send + Sync {
    fn acquire(&self) -> Result<u32, LoanError>;
    fn release(&self, index: u32);
    fn capacity(&self) -> u32;
    fn borrowed_count(&self) -> u32;
}

// ============================================================
// 8. 설정 구조체
// ============================================================

/// 전역 설정
#[derive(Clone, Debug)]
pub struct EntangleConfig {
    /// 공유 메모리 루트 경로 (기본: /tmp/entangle/)
    pub shm_root: String,
    /// 노드 이름
    pub node_name: Option<String>,
    /// 기본 PubSub QoS
    pub default_pubsub_qos: PubSubQos,
    /// 기본 Event QoS
    pub default_event_qos: EventQos,
}

// ============================================================
// 9. 공유 메모리 레이아웃 상수
// ============================================================

/// 매직 넘버: "ZCIP" + 버전 1
pub const MAGIC_NUMBER: u64 = 0x5A43_4950_0001;

/// 서비스 패턴 종류
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PatternType {
    PubSub = 1,
    Event = 2,
    ReqRes = 3,
    Blackboard = 4,
}

/// 채널 상태
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelState {
    Creating = 0,
    Connected = 1,
    Disconnected = 2,
}
