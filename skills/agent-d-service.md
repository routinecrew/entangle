# Agent D: entangle (서비스 + 사용자 API) 개발 스킬

## 너의 역할
entangle 프로젝트의 **서비스 계층과 사용자 API**를 만든다.
최종 사용자가 `Node → ServiceBuilder → Publisher/Subscriber`로 zero-copy IPC를 사용하는
인체공학적 API를 제공한다. iceoryx2와 동일한 사용성을 목표로 하되,
코드 중복(W-06)과 에러 폭발(W-04)을 해결한다.

## 반드시 지킬 것
- `contracts/shared_types.rs`의 QoS, 에러 피라미드, 패턴 타입 사용
- `entangle-transport`의 ZeroCopyChannel, DataSegment 사용 (또는 mock)
- `entangle-platform`의 SharedMemory, ProcessMonitor 사용 (또는 mock)
- ServiceLifecycle trait으로 4개 패턴의 공통 로직 통합 (iceoryx2의 W-06 보완)
- `thiserror` 기반 에러 피라미드 (iceoryx2의 W-04 보완)

## 구현 대상

### 1. Node (node.rs)
```rust
/// 프로세스당 하나의 Node. 서비스 생명주기 관리.
pub struct Node {
    id: NodeId,
    name: Option<String>,
    monitor: ProcessMonitor,
    config: EntangleConfig,
}

impl Node {
    pub fn builder() -> NodeBuilder;
    pub fn service<'a>(&'a self, name: &str) -> ServiceBuilder<'a>;
    pub fn id(&self) -> NodeId;
    pub fn name(&self) -> Option<&str>;
}

pub struct NodeBuilder { /* ... */ }
impl NodeBuilder {
    pub fn name(self, name: &str) -> Self;
    pub fn config(self, config: EntangleConfig) -> Self;
    pub fn create(self) -> Result<Node, IpcError>;
}
```

### 2. ServiceBuilder (service/mod.rs)
```rust
pub struct ServiceBuilder<'a> {
    node: &'a Node,
    name: ServiceName,
}

impl<'a> ServiceBuilder<'a> {
    pub fn publish_subscribe<T: ZeroCopySafe>(self) -> PubSubBuilder<'a, T>;
    pub fn event(self) -> EventBuilder<'a>;
    pub fn request_response<Req: ZeroCopySafe, Res: ZeroCopySafe>(self) -> ReqResBuilder<'a, Req, Res>;
    pub fn blackboard<K: ZeroCopySafe, V: ZeroCopySafe>(self) -> BlackboardBuilder<'a, K, V>;
}
```

### 3. ServiceLifecycle trait (service/lifecycle.rs)
```rust
/// 4개 패턴 공통 로직을 trait 기본 구현으로 통합.
/// iceoryx2에서 각 Builder에 70% 중복되던 코드를 제거.
trait ServiceLifecycle: Sized {
    type Config: serde::Serialize + serde::de::DeserializeOwned;
    type Service;

    fn validate_config(&self, existing: &Self::Config) -> Result<(), ServiceError>;
    fn init_dynamic_state(&self, shm: &SharedMemory) -> Result<(), ServiceError>;

    fn create(&self) -> Result<Self::Service, ServiceError> { /* 공통 로직 */ }
    fn open(&self) -> Result<Self::Service, ServiceError> { /* 공통 로직 */ }
    fn open_or_create(&self) -> Result<Self::Service, ServiceError> {
        match self.open() {
            Ok(s) => Ok(s),
            Err(ServiceError::NotFound { .. }) => self.create(),
            Err(e) => Err(e),
        }
    }
}
```

### 4. PubSub (service/pubsub.rs + port/publisher.rs + port/subscriber.rs)
```rust
pub struct PubSubBuilder<'a, T: ZeroCopySafe> { /* ... */ }
impl<'a, T: ZeroCopySafe> PubSubBuilder<'a, T> {
    pub fn history_size(self, size: usize) -> Self;
    pub fn max_publishers(self, max: usize) -> Self;
    pub fn max_subscribers(self, max: usize) -> Self;
    pub fn open_or_create(self) -> Result<PubSubService<T>, ServiceError>;
}

pub struct PubSubService<T: ZeroCopySafe> { /* ... */ }
impl<T: ZeroCopySafe> PubSubService<T> {
    pub fn publisher(&self) -> PublisherBuilder<T>;
    pub fn subscriber(&self) -> SubscriberBuilder<T>;
}

pub struct Publisher<T: ZeroCopySafe> { /* ... */ }
impl<T: ZeroCopySafe> Publisher<T> {
    pub fn loan(&self) -> Result<SampleMut<T>, LoanError>;
    pub fn send_copy(&self, value: T) -> Result<(), SendError>;
}

pub struct Subscriber<T: ZeroCopySafe> { /* ... */ }
impl<T: ZeroCopySafe> Subscriber<T> {
    pub fn receive(&self) -> Result<Option<Sample<T>>, ReceiveError>;
}
```

### 5. Sample / SampleMut (sample.rs)
```rust
/// 읽기 전용 샘플. Drop 시 자동으로 슬롯 반환.
pub struct Sample<T: ZeroCopySafe> {
    ptr: *const T,
    offset: PointerOffset,
    channel: Arc<ZeroCopyChannel>,
}

/// 쓰기 가능 샘플. send()로 발행하거나 drop으로 폐기.
pub struct SampleMut<T: ZeroCopySafe> {
    ptr: *mut T,
    offset: PointerOffset,
    segment: Arc<DataSegment>,
    channels: Vec<Arc<ZeroCopyChannel>>,
}

impl<T: ZeroCopySafe> SampleMut<T> {
    pub fn send(self) -> Result<(), SendError>;
}

impl<T: ZeroCopySafe> Deref for Sample<T> { type Target = T; }
impl<T: ZeroCopySafe> Deref for SampleMut<T> { type Target = T; }
impl<T: ZeroCopySafe> DerefMut for SampleMut<T> {}
```

### 6. Error Pyramid (error.rs)
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IpcError {
    #[error("service error: {0}")]
    Service(#[from] ServiceError),
    #[error("port error: {0}")]
    Port(#[from] PortError),
    #[error("platform error: {0}")]
    Platform(#[from] PlatformError),
}
// ... (contracts의 에러 타입을 thiserror로 구현)
```

### 7. WaitSet (waitset.rs)
```rust
/// Reactor 패턴. 여러 구독자/리스너를 하나의 이벤트 루프로 관리.
pub struct WaitSet { /* ... */ }
impl WaitSet {
    pub fn new() -> Self;
    pub fn attach_subscriber<T>(&mut self, sub: &Subscriber<T>) -> AttachmentId;
    pub fn attach_listener(&mut self, listener: &Listener) -> AttachmentId;
    pub fn wait(&self) -> Vec<AttachmentId>;
    pub fn wait_timeout(&self, timeout: Duration) -> Vec<AttachmentId>;
}
```

### 8. Config (config.rs)
```rust
/// 전역 설정. serde + ron 직렬화.
impl Default for EntangleConfig {
    fn default() -> Self {
        Self {
            shm_root: "/tmp/entangle/".to_string(),
            node_name: None,
            default_pubsub_qos: PubSubQos { /* defaults */ },
            default_event_qos: EventQos { /* defaults */ },
        }
    }
}
```

## 사용 예시 (최종 목표 API)
```rust
use entangle::prelude::*;

fn main() -> Result<(), IpcError> {
    let node = Node::builder().create()?;

    let service = node.service("sensor/temperature")?
        .publish_subscribe::<SensorData>()
        .history_size(10)
        .max_subscribers(8)
        .open_or_create()?;

    let publisher = service.publisher().create()?;
    let mut sample = publisher.loan()?;
    *sample = SensorData { timestamp: 42, value: 23.5 };
    sample.send()?;

    Ok(())
}
```

## 의존성 (Cargo.toml)
```toml
[dependencies]
entangle-platform = { path = "../entangle-platform" }
entangle-lockfree = { path = "../entangle-lockfree" }
entangle-transport = { path = "../entangle-transport" }
entangle-derive = { path = "../entangle-derive" }
serde = { version = "1.0", features = ["derive"] }
ron = "0.8"
thiserror = "2.0"
tracing = "0.1"
```

## 하위 크레이트 없이 먼저 개발하는 방법
```rust
// contracts/mock.rs의 MockSharedMemory, MockIndexAllocator, MockZeroCopyChannel로
// 서비스 계층 로직을 완전히 독립 개발 가능.
// 통합 시에만 mock → 실제 구현으로 교체.
```

## 테스트 시나리오
1. Node 생성/삭제 → ProcessMonitor 등록/해제 확인
2. PubSub: publisher.loan() → write → send() → subscriber.receive() → 동일 데이터
3. ServiceLifecycle: open_or_create() → 서비스 없으면 create, 있으면 open
4. QoS 불일치: 다른 QoS로 open 시도 → IncompatibleQos 에러
5. Error pyramid: 모든 에러가 ? 연산자로 전파 가능
6. WaitSet: 2개 subscriber → wait() → 데이터 있는 것만 반환

## 완료 기준
- `cargo test -p entangle` 전부 통과
- 사용 예시 코드가 컴파일 & 실행 가능
- ServiceLifecycle로 4개 패턴의 코드 중복 제거
- thiserror 기반 에러 피라미드 완전 구현
- 다른 사용자가 `use entangle::prelude::*;`로 바로 사용 가능
