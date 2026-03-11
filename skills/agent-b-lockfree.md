# Agent B: entangle-lockfree 개발 스킬

## 너의 역할
entangle 프로젝트의 **lock-free 자료구조 계층**을 만든다.
공유 메모리 내에서 프로세스 간 안전한 동시 접근을 보장하는 lock-free 구조체들이다.
iceoryx2의 lock-free 코드를 참조하되, **Loom 테스트를 Day 1부터 필수화**하여
iceoryx2의 약점 W-01(형식 검증 미적용)을 보완한다.

## 반드시 지킬 것
- 모든 lock-free 구조체에 `#[cfg(loom)]` 테스트 필수
- 모든 `Ordering` 사용에 근거 주석 필수 (왜 Acquire/Release/AcqRel인지)
- `#[repr(C)]` 필수 (공유 메모리에 배치되므로)
- 포인터 대신 `RelocatablePtr` 사용 (공유 메모리 주소 재배치 대응)
- 스택 오버플로우 방지: 큰 배열은 공유 메모리에, 스택에 직접 할당 금지

## 구현 대상

### 1. RelocatablePtr (relocatable.rs)
```rust
/// 공유 메모리 내 재배치 가능 포인터.
/// 절대 주소 대신 자신으로부터의 오프셋을 저장.
/// 다른 프로세스에서 mmap 주소가 달라도 동작.
#[repr(C)]
pub struct RelocatablePtr<T> {
    offset: isize,  // self 주소로부터의 바이트 오프셋
    _marker: PhantomData<*const T>,
}

impl<T> RelocatablePtr<T> {
    /// 대상 포인터와의 오프셋 계산 후 저장
    pub unsafe fn init(&mut self, target: *const T);
    /// 현재 주소 기준으로 대상 포인터 복원
    pub fn get(&self) -> *const T;
    pub fn get_mut(&mut self) -> *mut T;
}
```

### 2. UniqueIndexSet (index_set.rs)
```rust
/// MPMC lock-free 인덱스 집합. 공유 메모리 내 슬롯 할당에 사용.
///
/// iceoryx2 대비 개선:
/// - ABA 방지 태그를 24비트로 확장 (iceoryx2는 16비트)
/// - Loom 테스트 완전 구현
/// - cache line padding으로 false sharing 방지
#[repr(C)]
pub struct UniqueIndexSet {
    head: CacheAligned<AtomicU64>,
    // 비트 레이아웃: [head:24][tag:24][borrowed:16]
    capacity: u32,
    data: RelocatablePtr<AtomicU32>,
}

impl UniqueIndexSet {
    pub fn acquire(&self) -> Result<u32, AcquireError>;
    pub fn release(&self, index: u32);
    pub fn capacity(&self) -> u32;
    pub fn borrowed_count(&self) -> u32;
}
```

### 3. SpscQueue (spsc_queue.rs)
```rust
/// SPSC lock-free 큐. Publisher→Subscriber 간 샘플 인덱스 전달.
///
/// iceoryx2 대비 개선:
/// - cache line padding으로 false sharing 방지
/// - Loom 테스트 완전 구현
#[repr(C)]
pub struct SpscQueue {
    write_pos: CacheAligned<AtomicU64>,
    read_pos: CacheAligned<AtomicU64>,
    capacity: usize,
    data: RelocatablePtr<AtomicU64>,
}

impl SpscQueue {
    pub fn push(&self, value: u64) -> Result<(), QueueFullError>;
    pub fn pop(&self) -> Option<u64>;
    pub fn is_empty(&self) -> bool;
    pub fn len(&self) -> usize;
    pub fn capacity(&self) -> usize;
}
```

### 4. MpmcContainer (mpmc_container.rs)
```rust
/// MPMC 동적 포트 목록. 서비스에 연결된 포트 관리.
/// add()로 등록, remove()로 제거, iter()로 순회.
#[repr(C)]
pub struct MpmcContainer<T: Copy> {
    slots: RelocatablePtr<Slot<T>>,
    active_set: AtomicBitSet,
    capacity: u32,
}

impl<T: Copy> MpmcContainer<T> {
    pub fn add(&self, value: T) -> Result<u32, ContainerFullError>;
    pub fn remove(&self, index: u32) -> Option<T>;
    pub fn get(&self, index: u32) -> Option<T>;
    pub fn iter(&self) -> MpmcContainerIter<T>;
    pub fn len(&self) -> usize;
}
```

### 5. AtomicBitSet (atomic_bitset.rs)
```rust
/// 원자적 비트 집합. MpmcContainer의 활성 슬롯 추적에 사용.
#[repr(C)]
pub struct AtomicBitSet {
    words: RelocatablePtr<AtomicU64>,
    word_count: u32,
    bit_count: u32,
}

impl AtomicBitSet {
    pub fn set(&self, index: u32) -> bool;      // true if was unset
    pub fn clear(&self, index: u32) -> bool;     // true if was set
    pub fn test(&self, index: u32) -> bool;
    pub fn iter_set(&self) -> BitSetIter;
    pub fn count_set(&self) -> u32;
}
```

### 6. CacheAligned (lib.rs에 정의)
```rust
#[repr(C, align(64))]
pub struct CacheAligned<T>(pub T);
```

## Loom 테스트 패턴
```rust
#[cfg(loom)]
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
                        set.release(idx);
                    }
                })
            }).collect();
            for h in handles { h.join().unwrap(); }
        });
    }
}
```

## 의존성 (Cargo.toml)
```toml
[dependencies]
tracing = "0.1"

[dev-dependencies]
loom = "0.7"
proptest = "1.0"
```

## 핵심 설계 원칙
- 모든 구조체는 `#[repr(C)]`로 공유 메모리 호환
- `RelocatablePtr`로 프로세스 간 주소 독립성 보장
- `CacheAligned`로 false sharing 방지
- `#[cfg(loom)]`으로 std::sync::atomic ↔ loom::sync::atomic 전환
- ABA 문제 방지를 위한 태그 비트 확장

## entangle-platform 없이 먼저 개발하는 방법
Lock-free 자료구조는 플랫폼 독립적이다.
공유 메모리 배치 테스트만 mock으로 대체하면 독립 개발 가능:
```rust
// 힙에 할당하여 공유 메모리 시뮬레이션
fn mock_shm_layout<T>(count: usize) -> Vec<u8> {
    vec![0u8; std::mem::size_of::<T>() * count]
}
```

## 테스트 시나리오
1. UniqueIndexSet: N개 acquire → 모두 고유 → N개 release → 다시 acquire 가능
2. SpscQueue: push N개 → pop N개 → 순서 보존 확인
3. Loom: 3 스레드 동시 acquire/release → 중복 인덱스 0건
4. Loom: 1 producer + 1 consumer SpscQueue → 데이터 손실 0건
5. AtomicBitSet: set/clear/test 동시 접근 → 일관성 확인
6. MpmcContainer: 동시 add/remove → 누수 0건

## 완료 기준
- `cargo test -p entangle-lockfree` 전부 통과
- `RUSTFLAGS="--cfg loom" cargo test -p entangle-lockfree` 전부 통과
- `cargo +nightly miri test -p entangle-lockfree` 통과
- 모든 Ordering에 근거 주석
- unsafe 블록 < 30개 (RelocatablePtr 관련)
