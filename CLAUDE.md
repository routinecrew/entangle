# entangle — Claude Code 지침

## 프로젝트 개요
iceoryx2(eclipse-iceoryx/iceoryx2) 분석 기반 순수 Rust zero-copy IPC 라이브러리.
287,395줄 → ~20,000줄 핵심 재설계. iceoryx2의 약점 37건을 보완한 차세대 아키텍처.

## 반드시 읽어야 할 파일 (우선순위 순)
1. `contracts/shared_types.rs` — 공유 타입/trait. **절대 임의 변경 금지.**
2. `contracts/mock.rs` — 독립 개발용 mock 구현
3. `zerocopy-ipc-architecture.md` — 전체 아키텍처 설계서
4. `PARALLEL_DEV_GUIDE.md` — 병렬 개발 운영 가이드
5. `skills/agent-*.md` — 본인 담당 에이전트의 상세 스킬

## 아키텍처

**5-crate workspace** 계층 구조:

```
contracts/shared_types.rs  ← Single source of truth
        │
    entangle-platform      ← OS 추상화 (SharedMemory, FileLock, EventFd, Signal)
    entangle-lockfree      ← Lock-free 자료구조 (UniqueIndexSet, SpscQueue, MpmcContainer)
        │
    entangle-transport     ← 전송 계층 (ZeroCopyChannel, DataSegment, PoolAllocator)
        │
    entangle              ← 서비스+사용자 API (Node, ServiceBuilder, Publisher/Subscriber)
    entangle-derive        ← proc-macro (ZeroCopySafe derive)
```

**계층 의존 관계:**
```
사용자 API (entangle) → 전송 (entangle-transport) → lock-free + platform
                      → derive (entangle-derive)
```

## 에이전트 배정

| 에이전트 | 크레이트 | 역할 | 브랜치 |
|----------|----------|------|--------|
| Agent A | `entangle-platform` | OS 추상화: SharedMemory, FileLock, EventFd, Signal, ProcessMonitor | `agent-a/platform` |
| Agent B | `entangle-lockfree` | Lock-free 자료구조 + Loom 검증 | `agent-b/lockfree` |
| Agent C | `entangle-transport` | Zero-copy 채널, DataSegment, PoolAllocator | `agent-c/transport` |
| Agent D | `entangle` | 서비스 계층 + 사용자 API (Node, PubSub, Event, ReqRes) | `agent-d/service` |
| Agent E | `entangle-derive` | ZeroCopySafe derive 매크로 + 예제 + 벤치마크 | `agent-e/derive` |

## 코딩 규칙
- **`unwrap()` 금지** — `thiserror` 기반 에러 전파
- **`println!` 금지** — `tracing` 크레이트 사용
- **`unsafe` 사용 시** 반드시 Safety 주석으로 근거 명시
- **public API에 doc comment** 필수
- **Loom 테스트** 모든 lock-free CAS 루프에 필수 (#[cfg(loom)])
- **Ordering 근거** 모든 atomic operation에 주석으로 문서화

## 빌드/테스트
```bash
# 전체
cargo build --workspace
cargo test --workspace

# 개별 크레이트
cargo test -p entangle-platform
cargo test -p entangle-lockfree
cargo test -p entangle-transport
cargo test -p entangle
cargo test -p entangle-derive

# Loom 테스트 (lock-free)
RUSTFLAGS="--cfg loom" cargo test -p entangle-lockfree

# Miri (unsafe 검증)
cargo +nightly miri test -p entangle-lockfree
cargo +nightly miri test -p entangle-platform

# 벤치마크
cargo bench -p entangle
```

## contracts 변경 절차
1. 변경 필요성 설명과 함께 PR 생성
2. 영향받는 에이전트 목록 명시
3. 모든 크레이트 `cargo test --workspace` 통과 확인 후 merge

## 독립 개발 방법
각 크레이트는 `contracts/shared_types.rs`의 필요한 타입을 `src/contracts.rs`에 복사하고,
`contracts/mock.rs`의 mock 구현을 `src/mock.rs`에 복사하여 독립 빌드/테스트한다.
통합 시에만 mock → 실제 구현으로 교체.

## Token Optimization
- **서브에이전트(Agent tool) 사용 금지** — 직접 Glob, Grep, Read 등 기본 도구로 해결
- **응답은 최소한으로** — 코드 변경 시 변경 사항만 간결히 설명
- **파일은 필요한 부분만 읽기** — offset/limit 활용
- **병렬 도구 호출 활용** — 독립적인 도구 호출은 반드시 병렬 실행
