use entangle::prelude::*;

#[derive(ZeroCopySafe, Clone, Copy)]
#[repr(C)]
struct SensorData {
    timestamp: u64,
    value: f64,
}
fn main() {}
