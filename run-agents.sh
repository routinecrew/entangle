#!/bin/bash
# =============================================================
# entangle 에이전트 실행 스크립트
# =============================================================
# 사용법:
#   ./run-agents.sh a    → Agent A (platform) 실행
#   ./run-agents.sh b    → Agent B (lockfree) 실행
#   ./run-agents.sh c    → Agent C (transport) 실행
#   ./run-agents.sh d    → Agent D (service) 실행
#   ./run-agents.sh e    → Agent E (derive) 실행
#
# 실행 순서:
#   1단계: 터미널 1~3에서 → ./run-agents.sh a, b, e  (동시 시작)
#   2단계: A, B 완성 후  → ./run-agents.sh c
#   3단계: C 완성 후     → ./run-agents.sh d
# =============================================================

set -e
cd "$(dirname "$0")"

AGENT="$1"

CLAUDE_CMD="claude --dangerously-skip-permissions -p"

if [ -z "$AGENT" ]; then
  echo "사용법: ./run-agents.sh [a|b|c|d|e]"
  echo ""
  echo "  a  →  Agent A: entangle-platform (SharedMemory, FileLock, EventFd, Signal)"
  echo "  b  →  Agent B: entangle-lockfree (UniqueIndexSet, SpscQueue, Loom 검증)"
  echo "  c  →  Agent C: entangle-transport (ZeroCopyChannel, DataSegment)"
  echo "  d  →  Agent D: entangle (Node, ServiceBuilder, PubSub, Error pyramid)"
  echo "  e  →  Agent E: entangle-derive (ZeroCopySafe macro, 예제, 벤치마크)"
  echo ""
  echo "권장 순서: (a, b, e 동시) → c → d"
  exit 1
fi

case "$AGENT" in
  a)
    echo "🚀 Agent A (entangle-platform) 시작..."
    $CLAUDE_CMD "
당신은 Agent A입니다. entangle 프로젝트의 플랫폼 추상화 계층을 만듭니다.

먼저 다음 파일들을 읽으세요:
- skills/agent-a-platform.md (당신의 스킬)
- contracts/shared_types.rs (공유 타입 계약)
- zerocopy-ipc-architecture.md (시스템 설계서)
- CLAUDE.md (프로젝트 규칙)

crates/entangle-platform/ 디렉토리에 소스코드를 만들어주세요.
Cargo.toml은 이미 존재합니다. 수정이 필요하면 수정하세요.

구현 순서:
1. contracts/shared_types.rs의 플랫폼 관련 타입을 src/contracts.rs로 복사
2. SharedMemory (shm.rs) — nix 기반 POSIX shm_open/mmap + RAII Drop
3. FileLock (file_lock.rs) — fcntl F_SETLK 기반 파일 잠금
4. EventNotifier (event.rs) — 이벤트 알림 (pipe 또는 eventfd)
5. ProcessMonitor (process_monitor.rs) — 프로세스 사망 감지
6. Signal (signal.rs) — SIGINT/SIGTERM 핸들링

각 단계마다 cargo test -p entangle-platform이 통과하게 해주세요.
unsafe 블록마다 Safety 주석 필수. unwrap() 금지. println! 금지. tracing 사용.
"
    ;;

  b)
    echo "🚀 Agent B (entangle-lockfree) 시작..."
    $CLAUDE_CMD "
당신은 Agent B입니다. entangle 프로젝트의 lock-free 자료구조를 만듭니다.

먼저 다음 파일들을 읽으세요:
- skills/agent-b-lockfree.md (당신의 스킬)
- contracts/shared_types.rs (공유 타입 계약)
- zerocopy-ipc-architecture.md (시스템 설계서 — 4.1절 Lock-free 계층)
- CLAUDE.md (프로젝트 규칙)

crates/entangle-lockfree/ 디렉토리에서 작업하세요.
Cargo.toml은 이미 존재합니다. 수정이 필요하면 수정하세요.

이 크레이트는 외부 의존성이 없습니다. 독립적으로 개발하세요.

구현 순서:
1. CacheAligned 래퍼 (lib.rs)
2. RelocatablePtr (relocatable.rs) — 공유 메모리 내 재배치 가능 포인터
3. UniqueIndexSet (index_set.rs) — MPMC 인덱스 집합 + Loom 테스트
4. SpscQueue (spsc_queue.rs) — SPSC 큐 + Loom 테스트
5. AtomicBitSet (atomic_bitset.rs) — 원자적 비트 집합
6. MpmcContainer (mpmc_container.rs) — MPMC 동적 포트 목록 + Loom 테스트

모든 Ordering에 근거 주석 필수.
모든 lock-free 구조체에 #[cfg(loom)] 테스트 필수.
cargo test -p entangle-lockfree가 통과하게 해주세요.
"
    ;;

  c)
    echo "🚀 Agent C (entangle-transport) 시작..."
    $CLAUDE_CMD "
당신은 Agent C입니다. entangle 프로젝트의 전송 계층을 만듭니다.

먼저 다음 파일들을 읽으세요:
- skills/agent-c-transport.md (당신의 스킬)
- contracts/shared_types.rs (공유 타입 계약)
- contracts/mock.rs (Mock 구현)
- zerocopy-ipc-architecture.md (시스템 설계서 — 4.3절 전송 계층, 5절 메모리 레이아웃)
- CLAUDE.md (프로젝트 규칙)

crates/entangle-transport/ 디렉토리에서 작업하세요.
Cargo.toml은 이미 존재합니다. 수정이 필요하면 수정하세요.

entangle-platform, entangle-lockfree가 아직 미완성이면
contracts/mock.rs의 MockSharedMemory, MockIndexAllocator를 사용하세요.

구현 순서:
1. 공유 타입/mock을 src/contracts.rs, src/mock.rs로 복사
2. PoolAllocator (pool_alloc.rs) — UniqueIndexSet 기반 풀 할당
3. DataSegment (data_segment.rs) — 공유 메모리 데이터 영역
4. ZeroCopyChannel (channel.rs) — 타입 상태 패턴으로 sender↔receiver 통신
5. SegmentManager (segment_mgr.rs) — 다중 세그먼트 + 동적 확장
6. 단위 테스트

cargo test -p entangle-transport가 통과해야 합니다.
unwrap() 금지. println! 금지. tracing 사용.
"
    ;;

  d)
    echo "🚀 Agent D (entangle service) 시작..."
    $CLAUDE_CMD "
당신은 Agent D입니다. entangle 프로젝트의 서비스 계층과 사용자 API를 만듭니다.

먼저 다음 파일들을 읽으세요:
- skills/agent-d-service.md (당신의 스킬)
- contracts/shared_types.rs (공유 타입 계약)
- contracts/mock.rs (Mock 구현)
- zerocopy-ipc-architecture.md (시스템 설계서 — 4.4절 서비스 계층, 6절 데이터 흐름)
- CLAUDE.md (프로젝트 규칙)

crates/entangle/ 디렉토리에서 작업하세요.
Cargo.toml은 이미 존재합니다. 수정이 필요하면 수정하세요.

하위 크레이트가 아직 미완성이면 contracts/mock.rs의 mock을 사용하세요.

구현 순서:
1. Error pyramid (error.rs) — thiserror 기반 에러 계층
2. Config (config.rs) — serde + ron 직렬화
3. ServiceLifecycle trait (service/lifecycle.rs) — 4개 패턴 공통 로직
4. ServiceRegistry (service/registry.rs)
5. PubSub Builder + Publisher + Subscriber (service/pubsub.rs, port/publisher.rs, port/subscriber.rs)
6. Sample / SampleMut (sample.rs) — RAII 래퍼
7. Node (node.rs) — 프로세스 수명 관리
8. Event (service/event.rs, port/notifier.rs, port/listener.rs)
9. WaitSet (waitset.rs) — reactor 패턴
10. prelude (prelude.rs)

cargo test -p entangle가 통과해야 합니다.
unwrap() 금지. println! 금지. tracing 사용.
"
    ;;

  e)
    echo "🚀 Agent E (entangle-derive) 시작..."
    $CLAUDE_CMD "
당신은 Agent E입니다. entangle 프로젝트의 derive 매크로와 예제/벤치마크를 만듭니다.

먼저 다음 파일들을 읽으세요:
- skills/agent-e-derive.md (당신의 스킬)
- contracts/shared_types.rs (공유 타입 계약)
- zerocopy-ipc-architecture.md (시스템 설계서 — 4.2.2절 ZeroCopySafe trait)
- CLAUDE.md (프로젝트 규칙)

crates/entangle-derive/ 디렉토리에서 작업하세요 (proc-macro).
예제는 examples/, 벤치마크는 benches/, 통합 테스트는 tests/ 디렉토리에.
Cargo.toml은 이미 존재합니다. 수정이 필요하면 수정하세요.

이 크레이트는 독립적입니다. 다른 크레이트 없이 바로 개발하세요.

구현 순서:
1. ZeroCopySafe derive macro (lib.rs)
   - #[repr(C)] 검증
   - 필드 타입 검증 (힙 참조 타입 거부)
   - 제네릭 지원 (where T: ZeroCopySafe)
2. compile-fail 테스트 (trybuild)
3. 예제 프레임워크: examples/pubsub.rs (mock 기반)
4. 벤치마크 프레임워크: benches/latency.rs (프레임워크만)
5. 통합 테스트 프레임워크: tests/integration/pubsub_tests.rs

cargo test -p entangle-derive가 통과해야 합니다.
"
    ;;

  *)
    echo "❌ 알 수 없는 에이전트: $AGENT"
    echo "사용법: ./run-agents.sh [a|b|c|d|e]"
    exit 1
    ;;
esac
