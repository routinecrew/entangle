use entangle::prelude::*;

#[derive(ZeroCopySafe)]
#[repr(C)]
struct HasString {
    name: String,
}
fn main() {}
