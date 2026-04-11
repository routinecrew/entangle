# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-06-01

### Added

- 5-crate workspace: entangle, entangle-platform, entangle-lockfree, entangle-transport, entangle-derive
- Four messaging patterns: Publish-Subscribe, Event, Request-Response, Blackboard
- Zero-copy shared memory transport via POSIX shm
- Lock-free data structures: UniqueIndexSet (ABA-tagged), SpscQueue, MpmcContainer, AtomicBitSet
- `#[derive(ZeroCopySafe)]` proc-macro with compile-time safety validation
- Type-state pattern for channel lifecycle (Creating -> Connected -> Disconnected)
- WaitSet reactor for multiplexing multiple subscribers/listeners
- Process lifecycle monitoring with automatic resource cleanup
- POSIX signal handling (SIGINT/SIGTERM)
- RON-based configuration system
- thiserror error pyramid (IpcError -> ServiceError/PortError)
- Loom-verified concurrency for all lock-free CAS loops
- 60+ unit and integration tests
- Criterion benchmarks for latency, throughput, and lock-free primitives
- 6 runnable examples (pubsub, event, reqres, blackboard, waitset, multi_process)
- GitHub Actions CI (fmt, clippy, test, loom, docs)
