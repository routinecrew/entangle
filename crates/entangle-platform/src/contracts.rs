// contracts/shared_types.rs에서 복사한 플랫폼 관련 타입
// 독립 빌드를 위해 필요한 타입만 포함

/// 공유 메모리를 통해 안전하게 전송할 수 있는 타입 마커.
///
/// # Safety
/// 이 trait를 구현하는 타입은 다음을 보장해야 한다:
/// - `#[repr(C)]` 또는 `#[repr(transparent)]`
/// - 모든 필드가 `ZeroCopySafe`
/// - 힙 할당 참조 없음
/// - `Drop` 구현 없음
pub unsafe trait ZeroCopySafe: Copy + Send + Sync + 'static {}

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

/// 노드 식별자
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct NodeId(pub u128);

/// 플랫폼 에러
#[derive(Debug)]
pub enum PlatformError {
    SharedMemoryCreate { reason: String },
    SharedMemoryOpen { reason: String },
    FileLock { reason: String },
    Signal { reason: String },
}
