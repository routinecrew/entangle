<p align="center">
  <img src="https://img.shields.io/badge/lang-Rust-orange?style=flat-square&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/zero--copy-IPC-blue?style=flat-square" alt="Zero-Copy IPC" />
  <img src="https://img.shields.io/badge/lock--free-Loom%20verified-green?style=flat-square" alt="Loom Verified" />
  <img src="https://img.shields.io/badge/unsafe-%3C%20200-red?style=flat-square" alt="Unsafe < 200" />
  <img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-purple?style=flat-square" alt="License" />
</p>

# entangle

**Zero-copy inter-process communication for Rust.**

entangle delivers shared-memory IPC with **zero serialization, zero allocation, and zero copying** at the application level. Inspired by [iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2), redesigned from scratch as **pure Rust** — achieving the same functionality in 10x less code with formally verified lock-free concurrency.

```rust
use entangle::prelude::*;

// Process A: Publisher
let node = Node::builder().create()?;
let service = node.service("sensor/temperature")
    .publish_subscribe::<SensorData>()
    .open_or_create()?;

let publisher = service.publisher().create()?;
let mut sample = publisher.loan()?;   // borrow from shared memory
*sample = SensorData { timestamp: 42, value: 23.5 };
sample.send()?;                        // only a pointer offset is transferred
                                       // ZERO bytes copied
```

```rust
// Process B: Subscriber (separate process)
let subscriber = service.subscriber().create()?;
if let Some(sample) = subscriber.receive()? {
    println!("{}: {}°C", sample.timestamp, sample.value);
    // reads directly from shared memory — no deserialization
}
```

---

## Why entangle?

Modern IPC solutions force a painful choice: **fast but unsafe** (raw shared memory) or **safe but slow** (serialization + copying). entangle eliminates this trade-off.

### vs. Traditional IPC

| | Unix Socket | gRPC | entangle |
|--|:-:|:-:|:-:|
| Latency | ~10 μs | ~100 μs | **< 1 μs** |
| Copies per message | 2-4 | 3+ (serialize/deserialize) | **0** |
| Scales with payload size | Linear | Linear | **Constant** |

Sending a 1 MB image frame? Unix sockets copy it twice. gRPC serializes, copies, deserializes. entangle passes a **64-bit pointer offset** — same cost whether it's 8 bytes or 8 megabytes.

### vs. iceoryx2

entangle is a ground-up redesign informed by a deep audit of iceoryx2's 287,395-line codebase. We identified [37 weaknesses](zerocopy-ipc-architecture.md) and addressed them systematically:

| | iceoryx2 | entangle |
|--|:-:|:-:|
| **Total code** | 287,395 lines | **< 25,000 lines** |
| **Languages** | Rust + C++ + Python | **Pure Rust** |
| **unsafe blocks** | 4,800 | **< 200** |
| **Known concurrency bugs** | 4 (in todo.md) | **0 (Loom verified)** |
| **Build system** | Cargo + CMake + Bazel | **Cargo only** |
| **Build time** | Minutes (C++ compilation) | **< 30 seconds** |
| **Error types** | 37 custom enums (manual impl) | **thiserror pyramid** |
| **Lock-free verification** | "Planned" (unimplemented) | **Day 1: Loom + Miri** |

---

## Key Features

### True Zero-Copy

Data is written once into shared memory by the publisher. Subscribers read it directly — no intermediate buffers, no serialization framework, no kernel involvement.

```
Publisher                              Subscriber
    │                                      │
    │  1. loan() → SampleMut<T>            │
    │     (acquire slot from shared mem)    │
    │                                      │
    │  2. *sample = data                   │
    │     (write directly to shared mem)    │
    │                                      │
    │  3. send() → push pointer offset     │
    │                 ────────────────────→ │
    │                                      │  4. receive() → Sample<T>
    │                                      │     (read from same shared mem)
    │                                      │     ZERO copies. ZERO allocations.
```

### Formally Verified Lock-Free

Every lock-free data structure (UniqueIndexSet, SpscQueue, MpmcContainer) is tested with [Loom](https://github.com/tokio-rs/loom) — an exhaustive concurrency testing tool that explores all possible thread interleavings. Additionally validated with [Miri](https://github.com/rust-lang/miri) for undefined behavior detection.

```bash
# Exhaustive concurrency verification
RUSTFLAGS="--cfg loom" cargo test -p entangle-lockfree

# Undefined behavior detection
cargo +nightly miri test -p entangle-lockfree
```

### Type-Safe Shared Memory

The `ZeroCopySafe` derive macro ensures at **compile time** that only memory-safe types enter shared memory. Pointers, heap allocations, and non-`repr(C)` types are rejected:

```rust
#[derive(ZeroCopySafe)]
#[repr(C)]
struct SensorData {
    timestamp: u64,
    value: f64,
}  // Compiles

#[derive(ZeroCopySafe)]
struct BadType {
    name: String,  // Compile error: String cannot be shared via shared memory
}
```

### Four Messaging Patterns

| Pattern | Use Case | Ports |
|---------|----------|-------|
| **Publish-Subscribe** | Sensor data streaming, video frames | Publisher / Subscriber |
| **Event** | Lightweight notifications, signaling | Notifier / Listener |
| **Request-Response** | RPC, query-reply workflows | Client / Server |
| **Blackboard** | Shared state, latest-value access | Writer / Reader |

### Process Lifecycle Safety

When a process crashes, entangle automatically detects it via file-lock monitoring and reclaims all shared memory resources. No zombie segments, no leaked slots.

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│              User API (entangle)                 │
│  Node → ServiceBuilder → Publisher / Subscriber  │
│  PubSub, Event, ReqRes, Blackboard              │
├─────────────────────────────────────────────────┤
│          Transport (entangle-transport)           │
│  ZeroCopyChannel, DataSegment, PoolAllocator     │
├────────────────────┬────────────────────────────┤
│  Lock-free         │  Platform                   │
│  (entangle-lockfree│  (entangle-platform)        │
│  UniqueIndexSet,   │  SharedMemory, FileLock,    │
│  SpscQueue, MPMC)  │  EventFd, ProcessMonitor    │
└────────────────────┴────────────────────────────┘
         entangle-derive: #[derive(ZeroCopySafe)]
```

Five focused crates, each independently testable:

```
crates/
├── entangle/            # User-facing API — the only crate users import
├── entangle-transport/  # Zero-copy channels over shared memory
├── entangle-lockfree/   # Lock-free data structures (Loom tested)
├── entangle-platform/   # OS abstraction (POSIX shm, file locks)
└── entangle-derive/     # Proc-macro for compile-time safety checks
```

---

## Getting Started

Add to your `Cargo.toml`:

```toml
[dependencies]
entangle = "0.1"
```

### Publish-Subscribe Example

```rust
use entangle::prelude::*;

#[derive(ZeroCopySafe)]
#[repr(C)]
struct Temperature {
    sensor_id: u32,
    celsius: f64,
    timestamp: u64,
}

fn main() -> Result<(), IpcError> {
    let node = Node::builder().name("weather-station").create()?;

    let service = node.service("weather/temperature")
        .publish_subscribe::<Temperature>()
        .history_size(10)        // new subscribers get last 10 samples
        .max_subscribers(32)
        .open_or_create()?;

    let publisher = service.publisher().create()?;

    loop {
        let mut sample = publisher.loan()?;
        *sample = Temperature {
            sensor_id: 1,
            celsius: read_sensor(),
            timestamp: now(),
        };
        sample.send()?;
    }
}
```

### Event Notification

```rust
// Notifier
let service = node.service("system/shutdown")
    .event()
    .open_or_create()?;
let notifier = service.notifier().create()?;
notifier.notify()?;

// Listener
let listener = service.listener().create()?;
listener.wait()?;  // blocks until notified
```

### WaitSet (Multiplexing)

```rust
let mut waitset = WaitSet::new();
waitset.attach_subscriber(&temp_sub);
waitset.attach_subscriber(&pressure_sub);
waitset.attach_listener(&shutdown_listener);

loop {
    let triggered = waitset.wait();
    for id in triggered {
        // handle each ready source
    }
}
```

---

## Build & Test

```bash
cargo build --workspace
cargo test --workspace

# Individual crates
cargo test -p entangle-platform
cargo test -p entangle-lockfree
cargo test -p entangle-transport
cargo test -p entangle
cargo test -p entangle-derive

# Lock-free verification
RUSTFLAGS="--cfg loom" cargo test -p entangle-lockfree

# Unsafe verification
cargo +nightly miri test --workspace
```

---

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| **Pure Rust, no FFI** | Eliminates 267K lines of C++/Python bindings. Single toolchain. |
| **`nix` crate for syscalls** | Reduces unsafe from 4,800 to < 200. Battle-tested wrappers. |
| **`thiserror` error pyramid** | Replaces 37 hand-rolled error enums. `?` operator just works. |
| **Loom from Day 1** | iceoryx2 has 4 known concurrency bugs with Loom "planned". We ship verified. |
| **`#[repr(C)]` + derive macro** | Compile-time rejection of unsafe types. No runtime checks needed. |
| **Type-state pattern for channels** | `Creating → Connected → Disconnected` enforced at compile time. |
| **Trait-based service lifecycle** | iceoryx2 duplicates 70% of builder code across 4 patterns. We share it. |

---

## Use Cases

- **Robotics & Autonomous Vehicles** — Sub-microsecond sensor data sharing between perception, planning, and control processes
- **Real-time Audio/Video** — Zero-copy frame passing between capture, processing, and rendering pipelines
- **High-Frequency Trading** — Market data distribution with deterministic latency
- **Game Engines** — Physics, rendering, and AI subsystems communicating without serialization overhead
- **Embedded Systems** — Efficient IPC on resource-constrained hardware

---

## Roadmap

- [x] Project scaffold & workspace setup
- [x] Platform layer (SharedMemory, FileLock, EventFd)
- [x] Lock-free primitives (UniqueIndexSet, SpscQueue) + Loom tests
- [x] Transport layer (ZeroCopyChannel, DataSegment)
- [x] Service layer + PubSub/Event patterns
- [x] ZeroCopySafe derive macro
- [x] Request-Response & Blackboard patterns
- [x] WaitSet (reactor)
- [x] Cross-process integration tests
- [x] Benchmarks (criterion)
- [x] CI/CD (GitHub Actions)
- [ ] `no_std` support (feature-gated)
- [ ] Windows support

---

## Platform Support

| Platform | Architecture | Status |
|----------|-------------|--------|
| Linux | x86_64, aarch64 | Supported |
| macOS | x86_64, Apple Silicon | Supported |
| Windows | - | Not supported (POSIX shm dependency) |

**macOS limitations:** `shm_open` name limited to 31 characters; no `/dev/shm` filesystem.

---

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

```bash
cargo build --workspace
cargo test --workspace
```

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

---

<p align="center">
  <b>entangle</b> — Because your data shouldn't be copied just to cross a process boundary.
</p>
