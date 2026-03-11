# Agent C: entangle-transport 개발 스킬

## 너의 역할
entangle 프로젝트의 **전송 계층**을 만든다.
공유 메모리 위에 zero-copy 채널을 구축하고, 데이터 세그먼트를 관리한다.
Publisher가 loan()으로 메모리를 빌리고, send()로 인덱스만 전달하면
Subscriber가 동일 메모리를 복사 없이 읽는 것이 핵심이다.

## 반드시 지킬 것
- `entangle-platform`의 `SharedMemory`를 사용 (또는 mock)
- `entangle-lockfree`의 `SpscQueue`, `UniqueIndexSet`, `AtomicBitSet` 사용 (또는 mock)
- `contracts/shared_types.rs`의 `PointerOffset`, `ChannelState` 타입 사용
- 타입 상태 패턴으로 채널 생명주기 관리 (Creating → Connected → Disconnected)
- 세그먼트 ID + 오프셋 기반 주소 해석 (절대 포인터 저장 금지)

## 구현 대상

### 1. ZeroCopyChannel (channel.rs)
```rust
/// Zero-copy 통신 채널.
///
/// 공유 메모리 레이아웃:
///   [ChannelHeader][SpscQueue(send)][SpscQueue(return)][AtomicBitSet(borrowed)]
///
/// 타입 상태 패턴으로 상태별 메서드 제한:
/// - Creating: create(), connect() 허용
/// - Connected: send(), receive(), reclaim() 허용
/// - Disconnected: reconnect() 또는 drop만 허용
pub struct ZeroCopyChannel<State = Connected> {
    shm: SharedMemory,  // 또는 MockSharedMemory
    header: *const ChannelHeader,
    send_queue: *const SpscQueue,
    return_queue: *const SpscQueue,
    borrowed_tracker: *const AtomicBitSet,
    _state: PhantomData<State>,
}

pub struct Creating;
pub struct Connected;
pub struct Disconnected;

impl ZeroCopyChannel<Creating> {
    pub fn create(name: &str, config: ChannelConfig) -> Result<Self, ChannelError>;
    pub fn connect(self) -> Result<ZeroCopyChannel<Connected>, ChannelError>;
}

impl ZeroCopyChannel<Connected> {
    /// sender → receiver 방향으로 인덱스 전달
    pub fn send(&self, offset: PointerOffset) -> Result<(), SendError>;
    /// receiver가 인덱스 수신
    pub fn receive(&self) -> Option<PointerOffset>;
    /// sender가 반환된 인덱스 회수
    pub fn reclaim(&self) -> Option<PointerOffset>;
    /// 연결 상태 확인
    pub fn is_connected(&self) -> bool;
}
```

### 2. ChannelConfig
```rust
pub struct ChannelConfig {
    /// send 큐 용량 (인덱스 개수)
    pub send_buffer_size: usize,
    /// return 큐 용량
    pub return_buffer_size: usize,
    /// 최대 동시 빌림 수
    pub max_borrowed: usize,
}
```

### 3. DataSegment (data_segment.rs)
```rust
/// 실제 페이로드가 저장되는 공유 메모리 영역.
///
/// 고정 크기 청크로 분할된 메모리 풀.
/// PoolAllocator로 빈 청크를 관리.
pub struct DataSegment {
    segments: Vec<SharedMemory>,
    allocator: PoolAllocator,
    chunk_layout: Layout,
}

impl DataSegment {
    pub fn create(name: &str, chunk_size: usize, chunk_count: usize) -> Result<Self, SegmentError>;

    /// 빈 청크를 빌림 → SampleMut<T>로 RAII 관리
    pub fn loan<T: ZeroCopySafe>(&self) -> Result<(PointerOffset, *mut T), LoanError>;

    /// 인덱스로 실제 포인터 해석
    pub fn resolve<T: ZeroCopySafe>(&self, offset: PointerOffset) -> *const T;

    /// 청크 반환
    pub fn release(&self, offset: PointerOffset);

    /// 동적 확장 (기존 포인터 무효화 없음)
    pub fn grow(&mut self, additional_chunks: usize) -> Result<(), SegmentError>;
}
```

### 4. PoolAllocator (pool_alloc.rs)
```rust
/// 고정 크기 청크 풀 할당자.
/// UniqueIndexSet 기반으로 빈 슬롯을 lock-free로 관리.
pub struct PoolAllocator {
    index_set: UniqueIndexSet,  // 또는 MockIndexAllocator
    chunk_size: usize,
    chunk_count: usize,
}

impl PoolAllocator {
    pub fn allocate(&self) -> Result<u32, LoanError>;
    pub fn deallocate(&self, index: u32);
    pub fn available(&self) -> u32;
    pub fn chunk_offset(&self, index: u32) -> usize;
}
```

### 5. SegmentManager (segment_mgr.rs)
```rust
/// 다중 세그먼트 관리. 동적 확장 지원.
/// 세그먼트 ID + 로컬 오프셋으로 PointerOffset 구성.
pub struct SegmentManager {
    segments: Vec<DataSegment>,
    max_segments: u16,
}

impl SegmentManager {
    pub fn resolve(&self, offset: PointerOffset) -> *const u8;
    pub fn resolve_mut(&self, offset: PointerOffset) -> *mut u8;
    pub fn add_segment(&mut self, chunk_size: usize, chunk_count: usize) -> Result<u16, SegmentError>;
}
```

## 의존성 (Cargo.toml)
```toml
[dependencies]
entangle-platform = { path = "../entangle-platform" }
entangle-lockfree = { path = "../entangle-lockfree" }
thiserror = "2.0"
tracing = "0.1"
```

## entangle-platform / entangle-lockfree 없이 먼저 개발하는 방법
```rust
// mock으로 SharedMemory 대체
use crate::mock::MockSharedMemory;
// mock으로 IndexAllocator 대체
use crate::mock::MockIndexAllocator;

// 채널 레이아웃과 데이터 흐름 로직은 mock으로 완전히 검증 가능
```

## 테스트 시나리오
1. ZeroCopyChannel: create → connect → send(offset) → receive() → 동일 offset 확인
2. DataSegment: loan<u64>() → write → resolve() → read → 동일 데이터 확인
3. PoolAllocator: 전부 allocate → OutOfMemory → deallocate 1개 → allocate 성공
4. SegmentManager: 세그먼트 추가 → PointerOffset이 올바른 세그먼트로 해석
5. 타입 상태: Connected 상태에서만 send/receive 가능 (컴파일 타임 검증)
6. 동적 확장: grow() 후 기존 offset이 여전히 유효

## 완료 기준
- `cargo test -p entangle-transport` 전부 통과
- 타입 상태 패턴으로 잘못된 상태 전이가 컴파일 에러
- mock 기반 독립 테스트 + 실제 SharedMemory 통합 테스트 모두 작성
- 다른 에이전트가 `use entangle_transport::ZeroCopyChannel;`로 사용 가능
