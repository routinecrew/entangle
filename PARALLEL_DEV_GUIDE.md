# entangle — 병렬 개발 가이드

> 5개 에이전트가 동시에 개발하기 위한 운영 매뉴얼

---

## 1. 전체 구조 요약

```
                    ┌─────────────────────────────────────┐
                    │        contracts/shared_types.rs     │
                    │  (모든 에이전트가 공유하는 타입/trait)  │
                    └──────────────┬──────────────────────┘
                                   │
           ┌───────────────────────┼───────────────────────┐
           │                       │                       │
     ┌─────▼─────┐          ┌─────▼─────┐          ┌──────▼─────┐
     │  Agent A   │          │  Agent B  │          │  Agent E   │
     │ platform   │          │ lockfree  │          │  derive    │
     │ (OS 추상화) │          │(lock-free)│          │(proc-macro)│
     └─────┬─────┘          └─────┬─────┘          └────────────┘
           │                       │
           └───────────┬───────────┘
                 ┌─────▼─────┐
                 │  Agent C   │
                 │ transport  │
                 │(전송 계층)  │
                 └─────┬─────┘
                       │
                 ┌─────▼─────┐
                 │  Agent D   │
                 │  entangle  │
                 │(서비스+API)│
                 └───────────┘
```

---

## 2. 에이전트 역할 배정

| 에이전트 | 크레이트 | 핵심 역할 | 스킬 파일 |
|----------|----------|----------|-----------|
| **Agent A** | `entangle-platform` | SharedMemory, FileLock, EventFd, Signal, ProcessMonitor | `skills/agent-a-platform.md` |
| **Agent B** | `entangle-lockfree` | UniqueIndexSet, SpscQueue, MpmcContainer, AtomicBitSet + Loom | `skills/agent-b-lockfree.md` |
| **Agent C** | `entangle-transport` | ZeroCopyChannel, DataSegment, PoolAllocator, SegmentManager | `skills/agent-c-transport.md` |
| **Agent D** | `entangle` | Node, ServiceBuilder, Publisher/Subscriber, WaitSet, Error pyramid | `skills/agent-d-service.md` |
| **Agent E** | `entangle-derive` | ZeroCopySafe derive macro + 예제 + 벤치마크 + 통합 테스트 | `skills/agent-e-derive.md` |

---

## 3. 의존성 그래프와 병렬화 전략

```
Week 1-2:  [A: platform]  [B: lockfree]  [E: derive macro]
                │                │
Week 3-4:      └────────┬───────┘
                   [C: transport 시작]     [D: service mock 개발]
                         │
Week 5-6:          [C: transport 완성] ──▶ [D: service 통합]
                         │                       │
Week 7-8:  ◀──── 전체 통합 테스트 + 벤치마크 ────▶ [E: 예제+벤치마크]
```

### 핵심 원칙: Mock으로 독립 개발

각 크레이트는 하위 의존성의 mock을 사용하여 동시 개발한다.

```rust
// Agent C (transport)는 A, B 없이 mock으로 개발:
use crate::mock::{MockSharedMemory, MockIndexAllocator};

// Agent D (service)는 C 없이 mock으로 개발:
use crate::mock::MockZeroCopyChannel;
```

**의존성을 trait로 추상화한 이유가 이것이다.**
mock만 교체하면 상위 크레이트를 독립 실행할 수 있다.

---

## 4. 병렬성 분석

### A, B, E: 완전 독립 (Week 1~2)

| 크레이트 | 의존 | 비고 |
|----------|------|------|
| entangle-platform | 없음 | OS 시스템콜 래핑만 |
| entangle-lockfree | 없음 | 순수 lock-free 알고리즘 |
| entangle-derive | 없음 | proc-macro는 독립적 |

→ **3개 에이전트 동시 시작 가능**

### C: A+B 필요 (Week 3~)

transport는 platform의 SharedMemory + lockfree의 SpscQueue/UniqueIndexSet를 사용.
하지만 mock으로 대체하면 **Week 1부터 인터페이스 설계 가능**.
실제 통합은 A, B 완성 후.

### D: C 필요 (Week 5~)

service는 transport의 ZeroCopyChannel + DataSegment를 사용.
하지만 mock으로 대체하면 **Week 1부터 API 설계 + ServiceLifecycle 로직 개발 가능**.

---

## 5. Claude Code 에이전트 실행 방법

### 5.1 사전 준비

```bash
cd entangle
cargo build --workspace  # 초기 빌드 확인
```

### 5.2 에이전트 실행

```bash
# 터미널 1 — Agent A: Platform
./run-agents.sh a

# 터미널 2 — Agent B: Lock-free
./run-agents.sh b

# 터미널 3 — Agent E: Derive
./run-agents.sh e

# (A, B 완성 후)
# 터미널 4 — Agent C: Transport
./run-agents.sh c

# 터미널 5 — Agent D: Service
./run-agents.sh d
```

---

## 6. 통합 순서

### Phase 1: Platform + Lock-free (기반 계층 완성)
```bash
# Agent A의 SharedMemory + Agent B의 UniqueIndexSet/SpscQueue 통합
cargo test -p entangle-platform
cargo test -p entangle-lockfree
```

### Phase 2: Transport 통합 (전송 계층 완성)
```bash
# Agent C가 mock → 실제 SharedMemory + SpscQueue로 교체
cargo test -p entangle-transport
```

### Phase 3: Service 통합 (사용자 API 완성)
```bash
# Agent D가 mock → 실제 ZeroCopyChannel + DataSegment로 교체
cargo test -p entangle
```

### Phase 4: 전체 통합 + 벤치마크
```bash
# 모든 크레이트 통합
cargo test --workspace

# 벤치마크
cargo bench -p entangle

# Loom + Miri
RUSTFLAGS="--cfg loom" cargo test -p entangle-lockfree
cargo +nightly miri test --workspace
```

---

## 7. 충돌 방지 규칙

### 7.1 파일 소유권

| 디렉토리 | 소유 에이전트 | 다른 에이전트 접근 |
|----------|-------------|-----------------|
| `crates/entangle-platform/` | Agent A | 읽기만 |
| `crates/entangle-lockfree/` | Agent B | 읽기만 |
| `crates/entangle-transport/` | Agent C | 읽기만 |
| `crates/entangle/` | Agent D | 읽기만 |
| `crates/entangle-derive/` | Agent E | 읽기만 |
| `examples/`, `benches/`, `tests/` | Agent E | 읽기만 |
| `contracts/` | **공동 소유** | 변경 시 PR 필수 |

### 7.2 contracts 변경 프로토콜

1. 변경이 필요한 에이전트가 `contracts/shared_types.rs` 수정 PR 생성
2. PR 설명에 "영향받는 에이전트: B, C" 등 명시
3. 다른 에이전트가 확인 후 자기 크레이트의 `src/contracts.rs` 업데이트
4. 모든 크레이트 `cargo test` 통과 확인 후 merge

### 7.3 Git 브랜치 전략

```
main ─────────────────────────────────────────▶
  │
  ├── agent-a/platform ── Agent A 작업 ──── PR → main
  ├── agent-b/lockfree ── Agent B 작업 ──── PR → main
  ├── agent-c/transport ─ Agent C 작업 ──── PR → main
  ├── agent-d/service ─── Agent D 작업 ──── PR → main
  └── agent-e/derive ──── Agent E 작업 ──── PR → main
```

---

## 8. 체크리스트

### Week 1-2 체크리스트
- [ ] contracts/shared_types.rs 확정
- [ ] Cargo workspace 빌드 성공
- [ ] Agent A: SharedMemory create/open/drop + FileLock
- [ ] Agent B: UniqueIndexSet + SpscQueue + Loom 테스트
- [ ] Agent E: ZeroCopySafe derive macro + trybuild 테스트

### Week 3-4 체크리스트
- [ ] Agent A: EventNotifier + ProcessMonitor + Signal
- [ ] Agent B: MpmcContainer + AtomicBitSet + Loom 테스트
- [ ] Agent C: ZeroCopyChannel + DataSegment (mock 기반)
- [ ] Agent D: Node + ServiceLifecycle + Error pyramid (mock 기반)
- [ ] Agent E: 예제 프레임워크 + compile-fail 테스트

### Week 5-6 체크리스트
- [ ] Platform + Lock-free 통합 성공
- [ ] Transport 통합 (실제 SharedMemory + SpscQueue)
- [ ] Service 통합 (실제 ZeroCopyChannel)
- [ ] PubSub 엔드투엔드 동작

### Week 7-8 체크리스트
- [ ] 전체 통합 테스트 통과
- [ ] Loom + Miri 전체 통과
- [ ] 벤치마크: latency < 1μs, throughput 벤치
- [ ] 예제 6개 컴파일 & 동작
- [ ] cross-process 테스트 통과

---

## 9. 트러블슈팅

### "다른 에이전트의 크레이트가 컴파일 안 돼요"
→ 자기 크레이트만 빌드: `cargo build -p entangle-lockfree`
→ 다른 크레이트 의존은 mock으로 대체

### "contracts 타입이 부족해요"
→ contracts 변경 PR 생성 → 다른 에이전트에게 리뷰 요청
→ 임시로 자기 크레이트 내부에 확장 타입 정의 (나중에 contracts로 이동)

### "Loom 테스트가 느려요"
→ 스레드 수를 2~3으로 제한 (4 이상은 상태 공간 폭발)
→ `loom::model`의 preemption bound 조정

### "Miri에서 UB 감지됨"
→ unsafe 블록의 Safety 전제조건 재검토
→ Ordering이 올바른지 확인 (Relaxed가 아닌 Acquire/Release 필요할 수 있음)
→ `addr_of!` 매크로로 raw pointer 생성 시 참조를 거치지 않도록

### "macOS에서 shm_open 이름 제한"
→ 서비스 이름을 SHA-256 해시로 변환하여 31자 이내로
→ `/tmp/entangle/` 경로로 fallback (shm_open 대신 파일 + mmap)
