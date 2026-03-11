# ZeroCopy IPC — 순수 Rust 아키텍처 설계서

> iceoryx2 (eclipse-iceoryx/iceoryx2) 정밀 분석 기반  
> 287,395줄 → ~20,000줄 핵심 재설계  
> iceoryx2의 발견된 약점 37건을 보완한 차세대 아키텍처

---

## 1. 설계 철학

### 1.1 iceoryx2에서 배운 것

iceoryx2는 자동차 산업에서 시작된 zero-copy IPC 라이브러리로, Rust 코어 위에 C/C++/Python 바인딩을 제공한다. GitHub 레포 분석 결과 총 287,395줄(Rust 206K, C++ 64K, Python 9K, 빌드 8K)이며, 그 중 핵심 IPC 로직은 약 20,000줄에 불과하다. 나머지 267,000줄은 다국어 FFI, OS별 시스템콜 래핑, 중복 빌드 시스템에 의한 것이다.

### 1.2 우리의 원칙

| 원칙 | iceoryx2 | 우리 |
|------|----------|------|
| 언어 | Rust + C FFI + C++ + Python | **순수 Rust only** |
| OS 추상화 | 직접 POSIX 래핑 (40,000줄) | **nix/libc 크레이트 활용** |
| 빌드 시스템 | Cargo + CMake + Bazel | **Cargo only** |
| Lock-free 검증 | Loom/Kani/Miri "계획됨" (미적용) | **Day 1부터 Loom + Miri 적용** |
| 에러 처리 | 37개 커스텀 에러 enum (thiserror 미사용) | **thiserror + 에러 피라미드** |
| 로깅 | 커스텀 매크로 (1,481줄) | **tracing 크레이트** |
| 컨테이너 | 전부 직접 구현 (6,014줄) | **std + SHM용 최소 커스텀** |
| 설정 | 커스텀 파서 | **serde + ron** |
| no_std | 부분 지원 | **std 우선, no_std는 feature gate** |

### 1.3 목표 지표

| 지표 | iceoryx2 | 목표 |
|------|----------|------|
| 코드 총량 | 287,395줄 | **< 25,000줄** |
| unsafe 블록 | 4,800개 | **< 200개** |
| 동시성 버그 (알려진) | 4건 (todo.md) | **0건 (Loom 검증)** |
| 빌드 의존성 | Cargo + CMake + cbindgen | **Cargo only** |
| 첫 빌드 시간 | 수 분 (C++ 컴파일 포함) | **< 30초** |

---

## 2. iceoryx2 약점 분석 및 보완 전략

iceoryx2의 todo.md, 코드 분석, 이슈 트래커에서 발견된 약점을 체계적으로 분류하고, 각각에 대한 보완 전략을 제시한다.

### 2.1 동시성 / 안전성 약점 (Critical)

**약점 W-01: Lock-free 코드에 형식 검증 미적용**

iceoryx2의 ROADMAP.md에 "Use Kani, Loom and Miri for tests of lock-free constructs"가 미완료 항목으로 남아있다. 실제 코드에서 Loom은 의존성에 추가되어 있으나 대부분 `unimplemented!()` 처리이며, Kani/Miri는 사용 흔적이 0건이다. 이로 인해 todo.md에 최소 4건의 동시성 버그가 미해결 상태다.

```
보완: 모든 lock-free 구조체에 Loom 테스트를 필수화한다.
CI에서 `cargo +nightly miri test`를 기본 실행한다.
CAS 루프마다 Ordering 근거를 주석으로 문서화한다.
```

**약점 W-02: Windows 플랫폼 동시성 버그**

publisher_block_when_unable_to_deliver_blocks가 Windows에서 간헐적 데드락. condition_variable에서 WaitOnAddress 타임아웃이 spurious wakeup을 유발. CTRL+C가 Windows에서 작동하지 않아 리소스 정리 불가.

```
보완: Windows에서는 POSIX 에뮬레이션 대신 네이티브 API를 직접 사용한다.
WaitOnAddress 대신 WaitForSingleObject + 이벤트 객체 조합.
CtrlHandler를 등록하여 시그널 처리를 OS 네이티브로 구현.
```

**약점 W-03: unsafe 블록 과다 (4,800개)**

iceoryx2 전체에 unsafe 블록이 4,800개, unsafe fn이 1,602개다. 이는 POSIX 시스템콜을 직접 래핑하고, 공유 메모리 포인터를 수동 관리하기 때문이다.

```
보완: nix 크레이트를 사용하여 시스템콜 래핑의 unsafe를 제거한다.
공유 메모리 접근을 ShmSlice<T> 타입으로 캡슐화하여 unsafe 경계를 최소화한다.
목표: unsafe 블록 200개 미만.
```

### 2.2 아키텍처 / 설계 약점 (Major)

**약점 W-04: 에러 타입 폭발 (37개 독립 enum)**

코어 로직에만 37개의 에러 타입이 있으며, thiserror를 사용하지 않고 모든 Display/Error trait를 수동 구현한다. todo.md에 "Refactor error handling — Error pyramid concept"이 미완료 항목이다. anyhow/eyre와의 호환성도 없다.

```
보완: thiserror 기반 에러 피라미드를 도입한다.
최상위 IpcError → 중간 ServiceError/PortError → 하위 구체적 에러.
모든 에러가 std::error::Error를 구현하여 ? 연산자와 anyhow 호환.
```

**약점 W-05: Service trait의 과도한 제네릭**

iceoryx2의 Service trait는 12개의 associated type을 가진다 (StaticStorage, DynamicStorage, SharedMemory, Connection, ...). 이로 인해 모든 함수 시그니처가 `fn foo<Service: service::Service>(...)` 형태로 오염되며, 컴파일 타임이 증가하고 에러 메시지가 불투명해진다.

```
보완: associated type을 줄이고, 런타임 다형성과 컴파일타임 다형성을 분리한다.
IPC vs Local은 enum 디스패치로 처리 (현재 iceoryx2도 FFI에서 이렇게 함).
플랫폼 추상화는 cfg 컴파일 분기로 처리.
```

**약점 W-06: Builder 패턴 코드 중복**

4개 메시징 패턴(PubSub, Event, ReqRes, Blackboard)의 Builder가 각각 500~1,500줄이며, 서비스 생성/열기/검증 로직이 70% 이상 중복된다. 특히 open()과 create()의 설정 검증 로직이 각 Builder에서 반복된다.

```
보완: 공통 ServiceLifecycle trait를 도입한다.
validate_config(), create_dynamic_storage(), open_static_config() 등을
trait 기본 구현으로 제공하고, 패턴별 차이만 override.
```

**약점 W-07: 파일 시스템 기반 서비스 레지스트리의 한계**

iceoryx2는 /tmp/iceoryx2/ 디렉토리에 파일을 생성하여 서비스를 등록한다. 이는 단순하지만, 디렉토리 스캔이 O(n)이고, 파일 시스템 이벤트 감시가 불가하며, NFS 등 네트워크 파일 시스템에서 문제가 발생한다. todo.md에 "Service should not be in corrupted state when listed and currently being removed"가 미해결이다.

```
보완: 서비스 레지스트리를 전용 공유 메모리 영역으로 이전한다.
SlotMap 구조의 lock-free 레지스트리를 공유 메모리에 배치.
inotify/kqueue 기반 변경 감시도 옵션으로 제공.
파일 시스템 모드는 fallback으로 유지.
```

### 2.3 코드 품질 약점 (Minor)

**약점 W-08: Non-ASCII 파일명 패닉**

todo.md에 명시: "Directory panics when a directory contains non-ascii (UTF-8) characters". iceoryx2가 std::path를 사용하지 않고 커스텀 FileName/Path 타입을 사용하기 때문이다.

```
보완: std::path::Path와 OsStr을 직접 사용한다.
커스텀 SemanticString 타입을 제거하고, 검증이 필요한 경우 newtype으로 감싼다.
```

**약점 W-09: 스택 오버플로우 위험**

todo.md에 "MAX_NUMBER_OF_ENTRIES can lead to stack overflow when too large". 고정 크기 배열을 스택에 할당하기 때문이다.

```
보완: 큰 배열은 Vec::with_capacity()로 힙에 할당하고,
공유 메모리 내 고정 크기 구조체만 스택/SHM에 직접 배치한다.
```

**약점 W-10: History 전달의 지연**

todo.md에 "Publisher delivers history only when calling send — New subscriber may wait a long time". 새 구독자가 연결되어도 발행자가 send()를 호출할 때까지 이전 데이터를 받지 못한다.

```
보완: update_connections()에서 history 전달을 즉시 수행한다.
새 연결 감지 시 history buffer를 자동으로 전송하는 옵션을 기본 활성화.
```

**약점 W-11: 테스트 커버리지 불균형**

todo.md에 "Test all QoS of all services", "Write tests for Sample & SampleMut", "is publisher history tested?" 등이 미완료. Windows 테스트 대부분이 ignore 처리되어 있다.

```
보완: QoS 설정의 모든 조합을 property-based testing으로 자동 생성한다.
플랫폼별 CI를 Linux/macOS/Windows 모두 필수화.
```

### 2.4 약점 보완의 기대 효과

| 분류 | 약점 수 | 보완 후 효과 |
|------|---------|-------------|
| 동시성 | 3건 | 데드락/data race 0건 보장 (Loom+Miri) |
| 아키텍처 | 4건 | 코드량 90% 감소, 유지보수성 향상 |
| 코드품질 | 4건 | 패닉 0건, 크로스플랫폼 안정성 |

---

## 3. 전체 아키텍처

### 3.1 계층 구조

```
┌─────────────────────────────────────────────┐
│              사용자 API (User API)            │
│  Node → ServiceBuilder → Publisher/Subscriber │
│  PubSub, Event, ReqRes, Blackboard           │
├─────────────────────────────────────────────┤
│           서비스 계층 (Service Layer)          │
│  ServiceRegistry, LifecycleManager           │
│  StaticConfig, DynamicConfig                 │
├─────────────────────────────────────────────┤
│            포트 계층 (Port Layer)             │
│  Sender<T>, Receiver<T>                      │
│  SampleMut<T>, Sample<T>                     │
├─────────────────────────────────────────────┤
│         전송 계층 (Transport Layer)           │
│  ZeroCopyChannel, DataSegment                │
│  PoolAllocator, SegmentManager               │
├─────────────────────────────────────────────┤
│         Lock-free 계층 (Lock-free Layer)      │
│  IndexSet, SpscQueue, MpmcContainer          │
│  AtomicBitSet                                │
├─────────────────────────────────────────────┤
│         플랫폼 계층 (Platform Layer)          │
│  SharedMemory, FileLock, EventFd             │
│  nix::sys::mman, nix::fcntl                  │
└─────────────────────────────────────────────┘
```

### 3.2 Cargo Workspace 구조

```
zerocopy-ipc/
├── Cargo.toml                    # workspace root
├── crates/
│   ├── zerocopy-ipc/             # 메인 라이브러리 (사용자 API)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── node.rs           # Node: 프로세스 수명 관리
│   │   │   ├── service/
│   │   │   │   ├── mod.rs        # ServiceBuilder 진입점
│   │   │   │   ├── registry.rs   # 서비스 디스커버리
│   │   │   │   ├── lifecycle.rs  # 생성/열기/검증 공통 로직
│   │   │   │   ├── config.rs     # StaticConfig + DynamicConfig
│   │   │   │   ├── pubsub.rs     # PubSub 패턴 빌더
│   │   │   │   ├── event.rs      # Event 패턴 빌더
│   │   │   │   ├── reqres.rs     # ReqRes 패턴 빌더
│   │   │   │   └── blackboard.rs # Blackboard 패턴 빌더
│   │   │   ├── port/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── publisher.rs  # Publisher<T>
│   │   │   │   ├── subscriber.rs # Subscriber<T>
│   │   │   │   ├── client.rs     # Client<Req, Res>
│   │   │   │   ├── server.rs     # Server<Req, Res>
│   │   │   │   ├── notifier.rs   # Notifier
│   │   │   │   ├── listener.rs   # Listener
│   │   │   │   ├── reader.rs     # Reader<K, V>
│   │   │   │   └── writer.rs     # Writer<K, V>
│   │   │   ├── sample.rs         # Sample<T>, SampleMut<T>
│   │   │   ├── waitset.rs        # WaitSet (reactor)
│   │   │   ├── config.rs         # 전역 설정 (serde + ron)
│   │   │   ├── error.rs          # 통합 에러 피라미드
│   │   │   └── prelude.rs        # 편의 re-export
│   │   └── Cargo.toml
│   │
│   ├── zerocopy-transport/       # 전송 계층
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── channel.rs        # ZeroCopyChannel (sender↔receiver)
│   │   │   ├── data_segment.rs   # 공유 메모리 데이터 영역
│   │   │   ├── pool_alloc.rs     # 공유 메모리용 풀 할당자
│   │   │   └── segment_mgr.rs    # 동적 세그먼트 확장
│   │   └── Cargo.toml
│   │
│   ├── zerocopy-lockfree/        # Lock-free 기본 구조
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── index_set.rs      # MPMC UniqueIndexSet
│   │   │   ├── spsc_queue.rs     # SPSC IndexQueue
│   │   │   ├── mpmc_container.rs # MPMC Container
│   │   │   └── atomic_bitset.rs  # Atomic BitSet
│   │   ├── tests/
│   │   │   ├── loom_index_set.rs # Loom 검증 테스트
│   │   │   ├── loom_spsc.rs
│   │   │   └── loom_mpmc.rs
│   │   └── Cargo.toml
│   │
│   └── zerocopy-platform/        # 플랫폼 추상화
│       ├── src/
│       │   ├── lib.rs
│       │   ├── shm.rs            # SharedMemory (nix 기반)
│       │   ├── file_lock.rs      # FileLock (프로세스 사망 감지)
│       │   ├── event.rs          # EventFd / 이벤트 알림
│       │   └── signal.rs         # 시그널 핸들링
│       └── Cargo.toml
│
├── examples/
│   ├── pubsub.rs
│   ├── event.rs
│   ├── reqres.rs
│   └── blackboard.rs
│
├── benches/
│   ├── latency.rs                # iceoryx2와 비교 벤치마크
│   └── throughput.rs
│
└── tests/
    ├── integration/
    │   ├── pubsub_tests.rs
    │   ├── process_death_tests.rs
    │   └── cross_process_tests.rs
    └── proptest/
        └── qos_property_tests.rs  # QoS 조합 자동 테스트
```

---

## 4. 핵심 모듈 상세 설계

### 4.1 Lock-free 계층 (zerocopy-lockfree)

iceoryx2의 lock-free 구조체 3개를 참조하되, 약점 W-01을 보완하여 Loom 검증을 필수화한다.

#### 4.1.1 UniqueIndexSet (MPMC)

iceoryx2의 `iceoryx2-bb/lock-free/src/mpmc/unique_index_set.rs` (693줄) 참조. 공유 메모리 내 슬롯 할당/반환을 lock-free로 관리한다.

```rust
/// Lock-free 인덱스 집합. 공유 메모리에서 슬롯 할당에 사용.
/// 
/// iceoryx2 대비 개선:
/// - Loom 테스트 필수 (#[cfg(loom)] 완전 구현)
/// - ABA 방지를 위한 태그 비트를 32비트로 확장 (iceoryx2는 16비트)
/// - capacity 제한을 compile-time assert로 검증
#[repr(C)]
pub struct UniqueIndexSet {
    head: AtomicU64,
    // 비트 레이아웃: [head:24][tag:24][borrowed:16]
    // iceoryx2는 [head:24][aba:16][borrowed:24]
    // → tag를 24비트로 확장하여 ABA 문제 가능성을 2^16 → 2^24으로 감소
    capacity: u32,
    data: RelocatablePtr<AtomicU32>,
}

impl UniqueIndexSet {
    /// CAS 루프로 인덱스 획득.
    /// Ordering 근거: head를 읽을 때 Acquire로 이전 release와 동기화,
    /// 성공 시 AcqRel로 양방향 동기화 보장.
    pub fn acquire(&self) -> Result<u32, AcquireError> {
        let mut old = self.head.load(Ordering::Acquire);
        loop {
            let head = Self::extract_head(old);
            if head >= self.capacity {
                return Err(AcquireError::Empty);
            }
            let next = self.data[head as usize].load(Ordering::Acquire);
            let new = Self::pack(next, Self::extract_tag(old) + 1, 
                                 Self::extract_borrowed(old) + 1);
            match self.head.compare_exchange_weak(
                old, new, Ordering::AcqRel, Ordering::Acquire
            ) {
                Ok(_) => return Ok(head),
                Err(current) => old = current,
            }
        }
    }
}
```

Loom 테스트 예시:

```rust
#[cfg(test)]
mod loom_tests {
    use loom::sync::Arc;
    use loom::thread;
    
    #[test]
    fn concurrent_acquire_release_never_duplicates() {
        loom::model(|| {
            let set = Arc::new(UniqueIndexSet::new(4));
            let handles: Vec<_> = (0..3).map(|_| {
                let set = set.clone();
                thread::spawn(move || {
                    if let Ok(idx) = set.acquire() {
                        // 사용 후 반환
                        set.release(idx);
                    }
                })
            }).collect();
            for h in handles { h.join().unwrap(); }
        });
    }
}
```

#### 4.1.2 SPSC IndexQueue

iceoryx2의 `spsc/index_queue.rs` (477줄) 참조. 단일 생산자-단일 소비자 큐.

```rust
/// SPSC lock-free 큐. Publisher→Subscriber 간 샘플 인덱스 전달에 사용.
///
/// iceoryx2 대비 개선:
/// - cache line padding으로 false sharing 방지
/// - Loom 테스트 완전 구현 (iceoryx2는 일부만)
#[repr(C)]
pub struct SpscQueue {
    // 각각 별도 cache line에 배치하여 false sharing 방지
    write_pos: CacheAligned<AtomicU64>,
    read_pos: CacheAligned<AtomicU64>,
    capacity: usize,
    data: RelocatablePtr<AtomicU64>,
}

#[repr(C, align(64))]
struct CacheAligned<T>(T);
```

#### 4.1.3 MPMC Container

iceoryx2의 `mpmc/container.rs` (573줄) 참조. 동적 포트 목록 관리에 사용.

### 4.2 플랫폼 계층 (zerocopy-platform)

iceoryx2의 PAL(24,162줄) + BB/posix(15,832줄) = 39,994줄을 nix/libc 기반 ~800줄로 대체한다.

#### 4.2.1 SharedMemory

```rust
use nix::sys::mman::{shm_open, shm_unlink, mmap, munmap, MapFlags, ProtFlags};
use nix::sys::stat::Mode;
use nix::fcntl::OFlag;
use nix::unistd::ftruncate;

/// POSIX 공유 메모리 래퍼.
/// 
/// iceoryx2 대비 개선:
/// - nix 크레이트로 unsafe 최소화 (iceoryx2는 libc 직접 호출)
/// - Drop에서 자동 정리 (iceoryx2는 수동 소유권 관리)
/// - 타입 안전한 ShmSlice<T> 제공
pub struct SharedMemory {
    fd: OwnedFd,
    ptr: NonNull<u8>,
    size: usize,
    name: String,
    is_owner: bool,
}

impl SharedMemory {
    pub fn create(name: &str, size: usize) -> Result<Self, ShmError> {
        let fd = shm_open(
            name, 
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )?;
        ftruncate(&fd, size as i64)?;
        let ptr = unsafe {
            mmap(None, size.try_into()?, ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                 MapFlags::MAP_SHARED, &fd, 0)?
        };
        Ok(Self { fd, ptr: NonNull::new(ptr as *mut u8).unwrap(), 
                  size, name: name.to_string(), is_owner: true })
    }
    
    pub fn open(name: &str) -> Result<Self, ShmError> {
        let fd = shm_open(name, OFlag::O_RDWR, Mode::empty())?;
        // stat으로 크기 확인 후 mmap
        // ...
    }
    
    /// 타입 안전한 공유 메모리 슬라이스.
    /// T: ZeroCopySafe를 요구하여 공유 메모리에 안전한 타입만 허용.
    pub fn as_slice<T: ZeroCopySafe>(&self, offset: usize, count: usize) -> &[T] {
        // 정렬 및 범위 검증 후 반환
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        unsafe { munmap(self.ptr.as_ptr() as _, self.size).ok(); }
        if self.is_owner {
            shm_unlink(&self.name).ok();
        }
    }
}
```

#### 4.2.2 ZeroCopySafe trait

```rust
/// 공유 메모리를 통해 안전하게 전송할 수 있는 타입 마커.
///
/// iceoryx2 대비 개선:
/// - derive 매크로로 자동 검증 (iceoryx2는 수동 확인)
/// - 포인터, Box, Vec 등 힙 참조 타입을 컴파일 타임에 거부
/// 
/// # Safety
/// 이 trait를 구현하는 타입은 다음을 보장해야 한다:
/// - #[repr(C)] 또는 #[repr(transparent)]
/// - 모든 필드가 ZeroCopySafe
/// - 힙 할당 참조 없음 (포인터, Box, Vec, String 등 불가)
/// - Drop 구현 없음
pub unsafe trait ZeroCopySafe: Copy + Send + Sync + 'static {}

// 기본 타입 구현
unsafe impl ZeroCopySafe for u8 {}
unsafe impl ZeroCopySafe for u16 {}
unsafe impl ZeroCopySafe for u32 {}
unsafe impl ZeroCopySafe for u64 {}
unsafe impl ZeroCopySafe for i8 {}
unsafe impl ZeroCopySafe for i16 {}
unsafe impl ZeroCopySafe for i32 {}
unsafe impl ZeroCopySafe for i64 {}
unsafe impl ZeroCopySafe for f32 {}
unsafe impl ZeroCopySafe for f64 {}
unsafe impl ZeroCopySafe for bool {}
unsafe impl<T: ZeroCopySafe, const N: usize> ZeroCopySafe for [T; N] {}

// derive 매크로 제공 (proc-macro crate)
// #[derive(ZeroCopySafe)]
// #[repr(C)]
// struct SensorData { timestamp: u64, value: f64 }
```

#### 4.2.3 프로세스 사망 감지

```rust
/// 파일 잠금 기반 프로세스 생존 확인.
/// 
/// iceoryx2 대비 개선:
/// - flock 대신 fcntl F_SETLK 사용 (NFS 호환)
/// - 타임아웃 기반 감지 (iceoryx2는 폴링만)
pub struct ProcessMonitor {
    lock_file: File,
    node_id: NodeId,
}

impl ProcessMonitor {
    /// 현재 프로세스를 등록한다. Drop 시 자동 해제.
    pub fn register(node_id: NodeId) -> Result<Self, MonitorError> {
        let path = format!("/tmp/zerocopy-ipc/nodes/{}", node_id);
        let file = File::create(&path)?;
        fcntl_lock(&file, F_SETLK, F_WRLCK)?;  // non-blocking 잠금
        Ok(Self { lock_file: file, node_id })
    }
    
    /// 다른 프로세스의 생존을 확인한다.
    pub fn is_alive(node_id: &NodeId) -> bool {
        let path = format!("/tmp/zerocopy-ipc/nodes/{}", node_id);
        match File::open(&path) {
            Ok(file) => fcntl_lock(&file, F_GETLK, F_WRLCK)
                .map(|lock| lock.l_type != F_UNLCK)
                .unwrap_or(false),
            Err(_) => false,
        }
    }
}
```

### 4.3 전송 계층 (zerocopy-transport)

#### 4.3.1 ZeroCopyChannel

iceoryx2의 `zero_copy_connection/common.rs` (1,068줄) 참조. sender→receiver 간 zero-copy 전송을 관리한다.

```rust
/// Zero-copy 통신 채널.
/// 
/// 구조: 공유 메모리 내에 다음이 배치됨:
///   [Header][SpscQueue(sender→receiver)][SpscQueue(receiver→sender)][UsedChunkList]
///
/// iceoryx2 대비 개선:
/// - 채널 생성/연결을 Builder가 아닌 타입 상태 패턴으로 분리
/// - UsedChunkList를 AtomicBitSet으로 대체하여 메모리 사용량 절감
/// - 연결 상태를 enum으로 명시적 관리
pub struct ZeroCopyChannel<State = Connected> {
    shm: SharedMemory,
    header: &'static ChannelHeader,
    send_queue: &'static SpscQueue,      // 인덱스 전달 (sender → receiver)
    return_queue: &'static SpscQueue,    // 인덱스 반환 (receiver → sender)
    borrowed_tracker: &'static AtomicBitSet,
    _state: PhantomData<State>,
}

// 타입 상태 패턴: 컴파일 타임에 상태별 허용 메서드 제한
pub struct Creating;
pub struct Connected;
pub struct Disconnected;

impl ZeroCopyChannel<Creating> {
    pub fn create(name: &str, config: ChannelConfig) -> Result<Self, ChannelError> { ... }
    pub fn connect(self) -> ZeroCopyChannel<Connected> { ... }
}

impl ZeroCopyChannel<Connected> {
    pub fn send(&self, offset: PointerOffset) -> Result<(), SendError> { ... }
    pub fn receive(&self) -> Option<PointerOffset> { ... }
    pub fn reclaim(&self) -> Option<PointerOffset> { ... }
}
```

#### 4.3.2 DataSegment

```rust
/// 데이터 세그먼트: 실제 페이로드가 저장되는 공유 메모리 영역.
///
/// iceoryx2 대비 개선:
/// - Static/Dynamic을 enum이 아닌 제네릭 전략 패턴으로 분리
/// - 동적 확장 시 기존 포인터 무효화 방지 (세그먼트 ID 기반 주소 해석)
pub struct DataSegment {
    segments: Vec<SharedMemory>,     // 동적 확장 시 세그먼트 추가
    allocator: PoolAllocator,
    chunk_layout: Layout,
}

impl DataSegment {
    /// 메모리를 빌려준다 (loan). SampleMut<T>로 RAII 관리.
    pub fn loan<T: ZeroCopySafe>(&self) -> Result<SampleMut<T>, LoanError> {
        let offset = self.allocator.allocate()?;
        let ptr = self.resolve_offset(offset);
        Ok(SampleMut::new(ptr, offset, self))
    }
    
    /// 세그먼트 ID + 오프셋으로 실제 포인터를 해석한다.
    fn resolve_offset(&self, offset: PointerOffset) -> NonNull<u8> {
        let seg_id = offset.segment_id();
        let local_offset = offset.offset();
        let base = self.segments[seg_id as usize].as_ptr();
        unsafe { NonNull::new_unchecked(base.add(local_offset)) }
    }
}
```

### 4.4 서비스 계층

#### 4.4.1 에러 피라미드 (약점 W-04 보완)

```rust
use thiserror::Error;

/// 최상위 에러: 사용자에게 노출
#[derive(Error, Debug)]
pub enum IpcError {
    #[error("service error: {0}")]
    Service(#[from] ServiceError),
    #[error("port error: {0}")]
    Port(#[from] PortError),
    #[error("platform error: {0}")]
    Platform(#[from] PlatformError),
}

/// 서비스 계층 에러
#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("service '{name}' already exists")]
    AlreadyExists { name: String },
    #[error("service '{name}' not found")]
    NotFound { name: String },
    #[error("incompatible QoS: {reason}")]
    IncompatibleQos { reason: String },
    #[error("service corrupted: {reason}")]
    Corrupted { reason: String },
    #[error("version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: u32, found: u32 },
}

/// 포트 계층 에러
#[derive(Error, Debug)]
pub enum PortError {
    #[error("loan failed: {0}")]
    Loan(#[from] LoanError),
    #[error("send failed: {0}")]
    Send(#[from] SendError),
    #[error("receive failed: {0}")]
    Receive(#[from] ReceiveError),
    #[error("connection lost to peer {peer_id}")]
    ConnectionLost { peer_id: u128 },
}

#[derive(Error, Debug)]
pub enum LoanError {
    #[error("out of memory in data segment")]
    OutOfMemory,
    #[error("max loaned samples ({max}) exceeded")]
    ExceedsMaxLoans { max: usize },
}

#[derive(Error, Debug)]
pub enum SendError {
    #[error("connection broken: receiver no longer exists")]
    ConnectionBroken,
    #[error("loan failed: {0}")]
    Loan(#[from] LoanError),
}
```

#### 4.4.2 ServiceBuilder (약점 W-06 보완)

```rust
/// 서비스 생성의 공통 로직을 trait로 통합.
/// iceoryx2에서는 PubSub/Event/ReqRes/Blackboard 각각의 Builder에
/// open/create/validate 로직이 70% 중복되었음.
trait ServiceLifecycle: Sized {
    type Config: serde::Serialize + serde::de::DeserializeOwned;
    type Service;
    
    /// 패턴별 설정 검증. 각 패턴이 override.
    fn validate_config(&self, existing: &Self::Config) -> Result<(), ServiceError>;
    
    /// 패턴별 동적 상태 초기화. 각 패턴이 override.
    fn init_dynamic_state(&self, shm: &SharedMemory) -> Result<(), ServiceError>;
    
    // 공통 로직: trait 기본 구현으로 제공
    fn create(&self) -> Result<Self::Service, ServiceError> {
        // 1. 정적 설정 직렬화 → 파일/SHM에 저장
        // 2. 동적 상태 공유 메모리 생성
        // 3. init_dynamic_state() 호출
        // 4. 서비스 레지스트리에 등록
        // → 이 로직이 iceoryx2에서는 4번 복제되어 있었음
    }
    
    fn open(&self) -> Result<Self::Service, ServiceError> {
        // 1. 정적 설정 읽기 → 역직렬화
        // 2. validate_config() 호출
        // 3. 동적 상태 공유 메모리 열기
        // 4. 버전/호환성 확인
    }
    
    fn open_or_create(&self) -> Result<Self::Service, ServiceError> {
        match self.open() {
            Ok(s) => Ok(s),
            Err(ServiceError::NotFound { .. }) => self.create(),
            Err(e) => Err(e),
        }
    }
}
```

#### 4.4.3 사용자 API

```rust
// 사용 예시 — iceoryx2와 동일한 인체공학적 API
use zerocopy_ipc::prelude::*;

fn main() -> Result<(), IpcError> {
    let node = Node::builder().create()?;
    
    // Publish-Subscribe
    let service = node.service("sensor/temperature")?
        .publish_subscribe::<SensorData>()
        .history_size(10)
        .max_subscribers(8)
        .open_or_create()?;
    
    let publisher = service.publisher().create()?;
    
    // zero-copy 전송: loan → write → send
    let mut sample = publisher.loan()?;
    *sample = SensorData { timestamp: 42, value: 23.5 };
    sample.send()?;
    
    Ok(())
}

#[derive(ZeroCopySafe)]
#[repr(C)]
struct SensorData {
    timestamp: u64,
    value: f64,
}
```

---

## 5. 공유 메모리 레이아웃

### 5.1 서비스 메타데이터 영역

```
서비스 "sensor/temperature"의 공유 메모리 레이아웃:

/dev/shm/zerocopy_ipc_<hash>_static
┌─────────────────────────────────┐
│ MagicNumber (8B)                │ ← 0x5A43_4950_0001 (ZCIP v1)
│ Version (4B)                    │
│ PatternType (4B)                │ ← PubSub / Event / ReqRes / Blackboard
│ ServiceName (256B)              │
│ PayloadTypeInfo (64B)           │ ← TypeId, size, align
│ QoS Config (variable)          │ ← ron 직렬화
└─────────────────────────────────┘

/dev/shm/zerocopy_ipc_<hash>_dynamic
┌─────────────────────────────────┐
│ ReferenceCount (AtomicU32)      │ ← 연결된 노드 수
│ PublisherSlots (UniqueIndexSet)  │ ← 최대 publisher 수만큼
│ SubscriberSlots (UniqueIndexSet) │ ← 최대 subscriber 수만큼
│ ConnectionMatrix                │ ← publisher×subscriber 연결 상태
│ NodeRegistry (SlotMap)          │ ← 연결된 노드 ID 목록
└─────────────────────────────────┘
```

### 5.2 데이터 세그먼트 레이아웃

```
Publisher의 데이터 세그먼트:

/dev/shm/zerocopy_ipc_<hash>_data_<publisher_id>
┌─────────────────────────────────┐
│ SegmentHeader                   │
│   chunk_size: u32               │
│   chunk_count: u32              │
│   allocator_state: [u8; N]      │ ← PoolAllocator 상태
├─────────────────────────────────┤
│ Chunk 0: [Header][Payload]      │ ← chunk_size 정렬
│ Chunk 1: [Header][Payload]      │
│ Chunk 2: [Header][Payload]      │
│ ...                             │
│ Chunk N: [Header][Payload]      │
└─────────────────────────────────┘

ChunkHeader:
┌─────────────────────────────────┐
│ sequence_number: AtomicU64      │ ← 발행 순서
│ publisher_id: u128              │ ← 발행자 식별
│ payload_size: u32               │ ← 실제 페이로드 크기
│ user_header_size: u32           │ ← 사용자 정의 헤더
└─────────────────────────────────┘
```

### 5.3 연결(Connection) 레이아웃

```
Publisher A → Subscriber B 연결:

/dev/shm/zerocopy_ipc_<hash>_conn_<pub_id>_<sub_id>
┌─────────────────────────────────┐
│ ChannelHeader                   │
│   state: AtomicU8               │ ← Created/Connected/Disconnected
│   sender_id: u128               │
│   receiver_id: u128             │
├─────────────────────────────────┤
│ SendQueue (SpscQueue)           │ ← pub→sub 인덱스 전달
│   write_pos: AtomicU64          │
│   read_pos: AtomicU64           │
│   buffer: [AtomicU64; N]        │
├─────────────────────────────────┤
│ ReturnQueue (SpscQueue)         │ ← sub→pub 인덱스 반환
│   write_pos: AtomicU64          │
│   read_pos: AtomicU64           │
│   buffer: [AtomicU64; M]        │
├─────────────────────────────────┤
│ BorrowedTracker (AtomicBitSet)  │ ← 현재 빌려간 슬롯 추적
└─────────────────────────────────┘
```

---

## 6. 메시징 패턴별 데이터 흐름

### 6.1 Publish-Subscribe

```
Publisher                          Subscriber
    │                                  │
    │ 1. loan()                        │
    │    DataSegment에서 빈 슬롯 획득    │
    │    → UniqueIndexSet.acquire()     │
    │    → SampleMut<T> 반환            │
    │                                  │
    │ 2. *sample = data                │
    │    공유 메모리에 직접 쓰기          │
    │    (복사 0회)                      │
    │                                  │
    │ 3. sample.send()                 │
    │    각 Subscriber의 연결에          │
    │    PointerOffset을 push           │
    │    → SpscQueue.push(offset)      │
    │                ──────────────────→│
    │                                  │ 4. subscriber.receive()
    │                                  │    SpscQueue.pop() → offset
    │                                  │    offset로 공유 메모리 직접 읽기
    │                                  │    → Sample<T> 반환 (복사 0회)
    │                                  │
    │                ←──────────────────│ 5. drop(sample)
    │    SpscQueue(return)에             │    ReturnQueue.push(offset)
    │    offset 반환                     │
    │                                  │
    │ 6. publisher.reclaim()           │
    │    반환된 슬롯을 재사용 가능으로     │
    │    → UniqueIndexSet.release()     │
```

### 6.2 Request-Response

```
Client                             Server
    │                                  │
    │ 1. client.loan_request()         │
    │    요청 데이터 세그먼트에서 슬롯    │
    │                                  │
    │ 2. request.send()                │
    │    ──────────────────────────────→│
    │                                  │ 3. server.receive()
    │                                  │    요청을 읽고 응답 준비
    │                                  │
    │                                  │ 4. active_request.loan_response()
    │                                  │    응답 데이터 세그먼트에서 슬롯
    │                                  │
    │                ←──────────────────│ 5. response.send()
    │ 6. pending.receive()             │
    │    응답 읽기                       │
```

---

## 7. 의존성 목록

```toml
[workspace.dependencies]
# 플랫폼
nix = { version = "0.29", features = ["mman", "fs", "signal", "process"] }
libc = "0.2"

# 직렬화 / 설정
serde = { version = "1.0", features = ["derive"] }
ron = "0.8"

# 에러 처리
thiserror = "2.0"

# 로깅
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Lock-free 검증 (dev)
loom = { version = "0.7" }

# 테스트
proptest = { version = "1.0" }  # property-based testing

# derive 매크로
proc-macro2 = "1.0"
quote = "1.0"
syn = { version = "2.0", features = ["full"] }
```

---

## 8. 구현 로드맵

### Phase 1: 기반 (2주)

- zerocopy-platform: SharedMemory, FileLock, 시그널 핸들링
- ZeroCopySafe trait + derive 매크로
- Config 구조체 (serde + ron)
- 에러 피라미드

### Phase 2: Lock-free (3주)

- UniqueIndexSet + Loom 테스트
- SpscQueue + Loom 테스트
- MpmcContainer + Loom 테스트
- `cargo +nightly miri test` CI 설정
- AtomicBitSet

### Phase 3: 전송 (3주)

- ZeroCopyChannel (sender/receiver)
- DataSegment + PoolAllocator
- SampleMut<T> / Sample<T> RAII 래퍼
- 기본 벤치마크 (latency)

### Phase 4: 서비스 (3주)

- ServiceLifecycle trait + 공통 로직
- PubSub Builder + Publisher/Subscriber
- Node + ProcessMonitor
- ServiceRegistry
- 프로세스 사망 시 자동 정리

### Phase 5: 확장 패턴 (2주)

- Event (Notifier/Listener)
- Request-Response (Client/Server)
- WaitSet (reactor 패턴)

### Phase 6: 품질 (2주)

- Blackboard (Reader/Writer)
- Property-based QoS 테스트
- iceoryx2와 비교 벤치마크
- 문서화 + 예제

### 예상 결과

| 항목 | iceoryx2 | 우리 |
|------|----------|------|
| 총 코드량 | 287,395줄 | ~20,000줄 |
| unsafe 블록 | 4,800개 | < 200개 |
| 외부 의존성 | 최소 (자체 구현) | 적극 활용 (nix, thiserror, serde) |
| Loom 테스트 | unimplemented!() | 모든 CAS 루프 검증 |
| 빌드 시간 | 수 분 | < 30초 |
| 지원 패턴 | PubSub, Event, ReqRes, Blackboard | 동일 |
| 지원 플랫폼 | Linux, macOS, Windows, FreeBSD, QNX | Linux 우선 (macOS 추후) |

---

## 부록 A: iceoryx2 약점 전체 목록

iceoryx2의 todo.md + 코드 분석에서 발견된 37건의 약점 전체 목록:

### 동시성 (4건)

1. W-01: Lock-free 코드에 Loom/Kani/Miri 미적용
2. W-02: Windows publisher 간헐적 데드락
3. W-02b: Windows condition_variable spurious wakeup
4. W-02c: Windows CTRL+C 미작동 (리소스 미정리)

### 아키텍처 (8건)

5. W-04: 에러 타입 37개 폭발 (thiserror 미사용)
6. W-05: Service trait associated type 12개 (과도한 제네릭)
7. W-06: Builder 코드 70% 중복 (4개 패턴)
8. W-07: 파일 시스템 기반 레지스트리 (O(n) 스캔, race condition)
9. PortFactory 네이밍 혼란 (todo에서 Service로 리네임 제안)
10. NamedConceptBuilder 분리 미완료 (Opener/Creator)
11. ZeroCopyConnection blocking_send 제거 필요
12. CommunicationChannel safe_overflow 제거 필요

### 코드 품질 (10건)

13. W-08: Non-ASCII 파일명 패닉
14. W-09: MAX_NUMBER_OF_ENTRIES 스택 오버플로우
15. W-10: Publisher history 전달 지연
16. W-11: 테스트 커버리지 불균형 (Windows ignore)
17. Sample/SampleMut 테스트 미작성
18. QoS 전체 조합 테스트 미완료
19. Publisher history 테스트 불확실
20. macOS pthread_cond_timedwait 타임아웃 미구현
21. macOS 공유메모리 이름 30자 제한 미해결
22. Windows 공유메모리 파일 백업 미구현

### 설계 부채 (15건)

23. Stale service 처리 미완료
24. Event concept: process_local bitset 미구현
25. Subscriber/Publisher KeepAlive 타이밍 미구현
26. SampleMut TypeStatePattern 리팩토링 미완료
27. Subscriber 다중 publisher 전략 미구현 (priority/timestamp)
28. global_config 리팩토링 미완료
29. RelocatableContainer 메모리 검증 미완료
30. CommunicationChannel → NamedPipe 리네임 미완료
31. BasePort 추상화 미도입
32. ShmIpc trait + derive 매크로 미완료
33. 시그널 핸들러 싱글톤 패턴 미적용
34. POSIX 스레드 래퍼 힙 사용 (mempool 미도입)
35. UnixDatagramSocket ancillary data 분리 미완료
36. POSIX 래퍼 builder 패턴 정리 미완료
37. mpmc verify_memory_initialization 성능 오버헤드

---

## 부록 B: iceoryx2 코드 통계

```
=== 언어별 코드량 ===
Rust:     206,505줄  (71.9%)
C++/C:     64,018줄  (22.3%)
Python:     8,985줄  (3.1%)
Build:      7,887줄  (2.7%)
Total:    287,395줄

=== 핵심 vs 포장 ===
핵심 IPC 로직:    ~20,000줄  (7.0%)
FFI/바인딩:       ~84,000줄  (29.2%)
OS 래핑:          ~40,000줄  (13.9%)
테스트:           ~53,000줄  (18.4%)
예제:             ~12,000줄  (4.2%)
빌드/설정:         ~8,000줄  (2.8%)
기타 유틸:        ~70,000줄  (24.4%)

=== 복잡도 마커 ===
unsafe 블록:        4,800개
unsafe fn:          1,602개
compare_exchange:      77개
Atomic Ordering:      860개
#[repr(C)]:           400개
ManuallyDrop:         541개
에러 타입:             37개
```
