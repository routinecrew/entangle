use entangle::prelude::*;

#[derive(ZeroCopySafe, Clone, Copy)]
#[repr(C)]
struct Pair<T: Copy> {
    a: T,
    b: T,
}
fn main() {
    let _p: Pair<u64> = Pair { a: 1, b: 2 };
}
