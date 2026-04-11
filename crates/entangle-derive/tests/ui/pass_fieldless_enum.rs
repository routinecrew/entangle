use entangle::prelude::*;

#[derive(ZeroCopySafe, Clone, Copy)]
#[repr(u8)]
enum Status {
    Active = 0,
    Inactive = 1,
    Error = 2,
}
fn main() {}
