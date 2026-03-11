# Agent E: entangle-derive + 예제 + 벤치마크 개발 스킬

## 너의 역할
entangle 프로젝트의 **proc-macro 크레이트**와 **예제/벤치마크/통합 테스트**를 만든다.
`#[derive(ZeroCopySafe)]` 매크로로 사용자가 안전하게 공유 메모리 타입을 정의할 수 있게 하고,
프로젝트 전체의 품질을 검증하는 예제, 벤치마크, 통합 테스트를 작성한다.

## 반드시 지킬 것
- derive 매크로는 `#[repr(C)]` 어트리뷰트 없으면 컴파일 에러
- 힙 참조 타입 (String, Vec, Box, Arc 등) 필드가 있으면 컴파일 에러
- 모든 필드가 `ZeroCopySafe`를 구현하는지 검증
- 에러 메시지가 사용자 친화적이어야 함

## 구현 대상

### 1. ZeroCopySafe Derive (lib.rs)
```rust
/// 사용 예시:
/// #[derive(ZeroCopySafe)]
/// #[repr(C)]
/// struct SensorData {
///     timestamp: u64,
///     value: f64,
/// }
///
/// 다음은 컴파일 에러:
/// #[derive(ZeroCopySafe)]
/// struct BadType {          // #[repr(C)] 없음 → 에러
///     name: String,         // String은 ZeroCopySafe 아님 → 에러
/// }
#[proc_macro_derive(ZeroCopySafe)]
pub fn derive_zero_copy_safe(input: TokenStream) -> TokenStream {
    // 1. #[repr(C)] 또는 #[repr(transparent)] 확인
    // 2. 각 필드에 T: ZeroCopySafe bound 생성
    // 3. unsafe impl ZeroCopySafe for Foo {} 생성
    // 4. 제네릭 타입이 있으면 where T: ZeroCopySafe 추가
}
```

검증 로직:
```rust
fn check_repr(attrs: &[Attribute]) -> Result<(), syn::Error> {
    // #[repr(C)] 또는 #[repr(transparent)] 필수
    // 없으면: "ZeroCopySafe requires #[repr(C)] or #[repr(transparent)]"
}

fn check_fields(fields: &Fields) -> Result<(), syn::Error> {
    // 각 필드 타입 검사
    // String, Vec, Box, Arc, Rc, &, *const, *mut 등 → 에러
    // "field `name` has type `String` which cannot be safely shared via shared memory"
}
```

### 2. 예제 (examples/)
```
examples/
├── pubsub.rs         — 기본 PubSub: 1 publisher + 1 subscriber
├── event.rs          — Event 알림: notifier + listener
├── reqres.rs         — Request-Response: client + server
├── blackboard.rs     — Blackboard: reader + writer
├── multi_process.rs  — 2개 프로세스 간 통신 (fork 또는 별도 바이너리)
└── waitset.rs        — WaitSet으로 다중 구독 관리
```

### 3. 벤치마크 (benches/)
```rust
// benches/latency.rs
// 1:1 PubSub 왕복 지연 측정
// 목표: < 1μs (iceoryx2 수준)

// benches/throughput.rs
// 1:1 PubSub 처리량 측정
// 다양한 메시지 크기: 8B, 64B, 1KB, 64KB, 1MB

// benches/compare_iceoryx2.rs (선택)
// iceoryx2와 동일 조건 비교
```

### 4. 통합 테스트 (tests/)
```rust
// tests/integration/pubsub_tests.rs
// - 1 publisher + N subscriber 동시 통신
// - 히스토리 전달 확인
// - QoS 설정 검증

// tests/integration/process_death_tests.rs
// - 발행자 프로세스 사망 → 구독자가 감지 → 정리
// - 구독자 사망 → 발행자가 감지 → 리소스 회수

// tests/integration/cross_process_tests.rs
// - fork로 자식 프로세스 생성 → 부모-자식 간 IPC 검증

// tests/proptest/qos_property_tests.rs
// - QoS 설정의 모든 조합을 property-based testing으로 자동 생성
```

## 의존성 (Cargo.toml)
```toml
# entangle-derive/Cargo.toml
[lib]
proc-macro = true

[dependencies]
proc-macro2 = "1.0"
quote = "1.0"
syn = { version = "2.0", features = ["full"] }
```

## proc-macro 개발 팁
- `syn::DeriveInput`으로 입력 파싱
- `quote::quote!`로 코드 생성
- `proc_macro_error` 또는 `syn::Error::to_compile_error()`로 에러 리포팅
- 제네릭 타입 파라미터는 `Generics`에서 추출하여 where clause에 추가

## 다른 크레이트 없이 먼저 개발하는 방법
```rust
// proc-macro는 독립적. 컴파일 타임에만 동작하므로 다른 크레이트 불필요.
// ZeroCopySafe trait 정의만 테스트용으로 로컬에 두면 된다.

// 예제와 벤치마크는 entangle 크레이트의 mock을 사용하여
// 실제 공유 메모리 없이도 API 인체공학 검증 가능.
```

## 테스트 시나리오
1. derive: `#[repr(C)]` struct → 성공
2. derive: `#[repr(C)]` 없는 struct → 컴파일 에러 (메시지 확인)
3. derive: String 필드 → 컴파일 에러 (메시지 확인)
4. derive: 제네릭 `struct Pair<T> { a: T, b: T }` → where T: ZeroCopySafe 자동 추가
5. derive: 중첩 struct (`struct Outer { inner: Inner }`, Inner도 ZeroCopySafe) → 성공
6. compile-fail 테스트: trybuild 크레이트로 컴파일 실패 케이스 자동 검증

## 완료 기준
- `cargo test -p entangle-derive` 전부 통과
- trybuild로 compile-fail 테스트 통과
- 모든 예제가 컴파일 가능 (mock 기반)
- 벤치마크 프레임워크 설정 완료
- 통합 테스트 프레임워크 설정 완료
