# Contributing to entangle

Thank you for your interest in contributing to entangle! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- Rust stable (latest)
- Rust nightly (for Miri)

```bash
rustup install stable nightly
```

### Build & Test

```bash
# Full workspace
cargo build --workspace
cargo test --workspace

# Individual crates
cargo test -p entangle-platform
cargo test -p entangle-lockfree
cargo test -p entangle-transport
cargo test -p entangle
cargo test -p entangle-derive

# Lock-free verification (Loom)
RUSTFLAGS="--cfg loom" cargo test -p entangle-lockfree

# Unsafe verification (Miri)
cargo +nightly miri test -p entangle-lockfree

# Benchmarks
cargo bench -p entangle

# Lint
cargo fmt --check
cargo clippy --workspace -- -D warnings
```

## Architecture

```
entangle              User-facing API (Node, Service, Publisher/Subscriber)
entangle-transport    Zero-copy channels over shared memory
entangle-lockfree     Lock-free data structures (Loom verified)
entangle-platform     OS abstraction (POSIX shm, file locks)
entangle-derive       #[derive(ZeroCopySafe)] proc-macro
```

Each crate is independently testable using local copies of `contracts.rs` and `mock.rs`.

## Coding Conventions

- **No `unwrap()` in library code** — propagate errors with `thiserror`
- **No `println!`** — use `tracing` crate (`info!`, `debug!`, `warn!`, `error!`)
- **`unsafe` blocks** require a `// Safety:` comment explaining the invariants
- **Doc comments** required on all public APIs
- **Loom tests** required for all lock-free CAS loops (`#[cfg(loom)]`)
- **Atomic ordering** must be documented with a comment on every operation

## Contracts

The canonical type definitions live in `contracts/shared_types.rs`. Each crate copies the types it needs into a local `src/contracts.rs`.

**To change a shared type:**

1. Open a PR with the change and rationale
2. List all affected crates
3. Ensure `cargo test --workspace` passes after updating all local copies
4. Get review before merging

## Pull Request Process

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes following the coding conventions
4. Ensure all tests pass: `cargo test --workspace`
5. Ensure formatting: `cargo fmt`
6. Ensure no warnings: `cargo clippy --workspace -- -D warnings`
7. Open a PR with a clear description of the change

## License

By contributing to entangle, you agree that your contributions will be dual-licensed under the MIT and Apache 2.0 licenses.
