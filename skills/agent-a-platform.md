# Agent A: entangle-platform 개발 스킬

## 너의 역할
entangle 프로젝트의 **플랫폼 추상화 계층**을 만든다.
POSIX 공유 메모리, 파일 잠금, 이벤트 알림, 시그널 처리 — 모든 상위 크레이트가 의존하는 OS 인터페이스다.
iceoryx2가 40,000줄로 직접 래핑한 것을 `nix` 크레이트 활용으로 ~800줄에 구현한다.

## 반드시 지킬 것
- `contracts/shared_types.rs`의 `SharedMemoryProvider` trait, `PlatformError`, `NodeId` 사용
- `nix` 크레이트로 시스템콜 래핑하여 unsafe 최소화
- unsafe 블록마다 Safety 주석 필수
- Drop에서 리소스 자동 정리 (RAII)
- 모든 public API에 doc comment

## 구현 대상

### 1. SharedMemory (shm.rs)
```rust
pub struct SharedMemory {
    fd: OwnedFd,
    ptr: NonNull<u8>,
    size: usize,
    name: String,
    is_owner: bool,
}

impl SharedMemory {
    /// POSIX 공유 메모리 생성. O_CREAT | O_EXCL로 중복 방지.
    pub fn create(name: &str, size: usize) -> Result<Self, PlatformError>;

    /// 기존 공유 메모리 열기.
    pub fn open(name: &str) -> Result<Self, PlatformError>;

    /// 타입 안전한 슬라이스 뷰. T: ZeroCopySafe 필수.
    pub fn as_slice<T: ZeroCopySafe>(&self, offset: usize, count: usize) -> &[T];
    pub fn as_slice_mut<T: ZeroCopySafe>(&mut self, offset: usize, count: usize) -> &mut [T];

    /// 원시 포인터 접근
    pub fn as_ptr(&self) -> *const u8;
    pub fn as_mut_ptr(&mut self) -> *mut u8;
    pub fn size(&self) -> usize;
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        // munmap + shm_unlink (소유자만)
    }
}
```

### 2. FileLock (file_lock.rs)
```rust
/// fcntl F_SETLK 기반 파일 잠금. 프로세스 사망 시 OS가 자동 해제.
pub struct FileLock {
    file: File,
    path: PathBuf,
}

impl FileLock {
    pub fn acquire(path: &Path) -> Result<Self, PlatformError>;
    pub fn try_acquire(path: &Path) -> Result<Option<Self>, PlatformError>;
    pub fn is_locked(path: &Path) -> bool;
}
```

### 3. EventFd (event.rs)
```rust
/// 이벤트 알림 메커니즘.
/// Linux: eventfd, macOS: pipe, 범용: socketpair
pub struct EventNotifier {
    // 플랫폼별 구현
}

impl EventNotifier {
    pub fn new() -> Result<Self, PlatformError>;
    pub fn notify(&self) -> Result<(), PlatformError>;
    pub fn wait(&self) -> Result<(), PlatformError>;
    pub fn try_wait(&self) -> Result<bool, PlatformError>;
    pub fn fd(&self) -> RawFd;  // epoll/kqueue 등록용
}
```

### 4. ProcessMonitor (process_monitor.rs)
```rust
/// 파일 잠금 기반 프로세스 생존 확인.
pub struct ProcessMonitor {
    lock: FileLock,
    node_id: NodeId,
}

impl ProcessMonitor {
    pub fn register(node_id: NodeId) -> Result<Self, PlatformError>;
    pub fn is_alive(node_id: &NodeId) -> bool;
    pub fn list_alive_nodes() -> Vec<NodeId>;
    pub fn cleanup_dead_nodes() -> Result<Vec<NodeId>, PlatformError>;
}
```

### 5. Signal (signal.rs)
```rust
/// 시그널 핸들링. SIGINT/SIGTERM에서 안전한 정리 보장.
pub fn install_signal_handler() -> Result<(), PlatformError>;
pub fn is_shutdown_requested() -> bool;
```

## 의존성 (Cargo.toml)
```toml
[dependencies]
nix = { version = "0.29", features = ["mman", "fs", "signal", "process"] }
libc = "0.2"
thiserror = "2.0"
tracing = "0.1"
```

## 핵심 설계 원칙
- iceoryx2는 POSIX 시스템콜을 직접 래핑하여 unsafe 4,800개. 우리는 nix로 감싼다.
- SharedMemory의 Drop에서 자동 정리 (iceoryx2는 수동 소유권 관리)
- 타입 안전한 `as_slice<T: ZeroCopySafe>()` 제공 (raw 포인터 노출 최소화)
- macOS 공유메모리 이름 31자 제한 → 해시 기반 이름 생성으로 해결

## 테스트 시나리오
1. SharedMemory: create → write → open(다른 핸들) → read → 동일 데이터 확인
2. SharedMemory: create 중복 → 에러 반환 확인
3. FileLock: acquire → is_locked(true) → drop → is_locked(false)
4. ProcessMonitor: register → is_alive(true) → (프로세스 종료 시뮬) → cleanup
5. EventNotifier: notify → wait → 수신 확인
6. SharedMemory Drop: drop 후 shm_unlink 확인

## 완료 기준
- `cargo test -p entangle-platform` 전부 통과
- `cargo +nightly miri test -p entangle-platform` 통과 (unsafe 검증)
- unsafe 블록 < 20개
- 다른 에이전트가 `use entangle_platform::SharedMemory;`로 사용 가능
