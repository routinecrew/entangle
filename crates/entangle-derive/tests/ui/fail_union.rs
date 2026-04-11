use entangle::prelude::*;

#[derive(ZeroCopySafe)]
#[repr(C)]
union BadUnion {
    a: u32,
    b: f32,
}
fn main() {}
