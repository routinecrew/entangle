use entangle::prelude::*;

#[derive(ZeroCopySafe)]
#[repr(C)]
struct HasVec {
    data: Vec<u8>,
}
fn main() {}
