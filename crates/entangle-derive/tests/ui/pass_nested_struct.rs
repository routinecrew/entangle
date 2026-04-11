use entangle::prelude::*;

#[derive(ZeroCopySafe, Clone, Copy)]
#[repr(C)]
struct Inner {
    x: f64,
    y: f64,
}

#[derive(ZeroCopySafe, Clone, Copy)]
#[repr(C)]
struct Outer {
    inner: Inner,
    z: f64,
}
fn main() {}
