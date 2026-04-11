//! Benchmark: PubSub throughput with varying payload sizes.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use entangle::prelude::*;

fn unique_shm_root(name: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("/tmp/entangle_bench_{name}_{ts}/")
}

fn bench_config(name: &str) -> entangle::config::EntangleConfig {
    entangle::config::EntangleConfig {
        shm_root: unique_shm_root(name),
        ..Default::default()
    }
}

// --- Payload types of different sizes ---

/// 8 bytes (u64 — already ZeroCopySafe).
type Payload8 = u64;

/// 64 bytes.
#[derive(Clone, Copy)]
#[repr(C)]
struct Payload64 {
    data: [u8; 64],
}
// Safety: #[repr(C)], all fields ZeroCopySafe, no heap, no Drop.
unsafe impl ZeroCopySafe for Payload64 {}

/// 1 KB (1024 bytes).
#[derive(Clone, Copy)]
#[repr(C)]
struct Payload1K {
    data: [u8; 1024],
}
unsafe impl ZeroCopySafe for Payload1K {}

/// 4 KB (4096 bytes).
#[derive(Clone, Copy)]
#[repr(C)]
struct Payload4K {
    data: [u8; 4096],
}
unsafe impl ZeroCopySafe for Payload4K {}

const BATCH_SIZE: u64 = 1000;

fn bench_throughput_u64(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    group.throughput(Throughput::Elements(BATCH_SIZE));

    // 8B payload
    group.bench_function(BenchmarkId::new("send_recv", "8B"), |b| {
        let node = Node::builder()
            .config(bench_config("tp_8b"))
            .create()
            .unwrap();
        let service = node
            .service("bench/tp_8b")
            .publish_subscribe::<Payload8>()
            .max_loaned_samples(16)
            .open_or_create()
            .unwrap();
        let mut publisher = service.publisher().create().unwrap();
        let subscriber = service.subscriber().create().unwrap();

        b.iter(|| {
            for i in 0..BATCH_SIZE {
                let mut sample = publisher.loan().unwrap();
                *sample = black_box(i);
                sample.send().unwrap();

                let received = subscriber.receive().unwrap().unwrap();
                black_box(&*received);
                drop(received);

                publisher.reclaim();
            }
        });
    });

    group.finish();
}

fn bench_throughput_64b(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    group.throughput(Throughput::Bytes(64 * BATCH_SIZE));

    group.bench_function(BenchmarkId::new("send_recv", "64B"), |b| {
        let node = Node::builder()
            .config(bench_config("tp_64b"))
            .create()
            .unwrap();
        let service = node
            .service("bench/tp_64b")
            .publish_subscribe::<Payload64>()
            .max_loaned_samples(16)
            .open_or_create()
            .unwrap();
        let mut publisher = service.publisher().create().unwrap();
        let subscriber = service.subscriber().create().unwrap();

        b.iter(|| {
            for _ in 0..BATCH_SIZE {
                let mut sample = publisher.loan().unwrap();
                sample.data = black_box([0xABu8; 64]);
                sample.send().unwrap();

                let received = subscriber.receive().unwrap().unwrap();
                black_box(&*received);
                drop(received);

                publisher.reclaim();
            }
        });
    });

    group.finish();
}

fn bench_throughput_1k(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    group.throughput(Throughput::Bytes(1024 * BATCH_SIZE));

    group.bench_function(BenchmarkId::new("send_recv", "1KB"), |b| {
        let node = Node::builder()
            .config(bench_config("tp_1k"))
            .create()
            .unwrap();
        let service = node
            .service("bench/tp_1k")
            .publish_subscribe::<Payload1K>()
            .max_loaned_samples(16)
            .open_or_create()
            .unwrap();
        let mut publisher = service.publisher().create().unwrap();
        let subscriber = service.subscriber().create().unwrap();

        b.iter(|| {
            for _ in 0..BATCH_SIZE {
                let mut sample = publisher.loan().unwrap();
                sample.data = black_box([0xCDu8; 1024]);
                sample.send().unwrap();

                let received = subscriber.receive().unwrap().unwrap();
                black_box(&*received);
                drop(received);

                publisher.reclaim();
            }
        });
    });

    group.finish();
}

fn bench_throughput_4k(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    group.throughput(Throughput::Bytes(4096 * BATCH_SIZE));

    group.bench_function(BenchmarkId::new("send_recv", "4KB"), |b| {
        let node = Node::builder()
            .config(bench_config("tp_4k"))
            .create()
            .unwrap();
        let service = node
            .service("bench/tp_4k")
            .publish_subscribe::<Payload4K>()
            .max_loaned_samples(16)
            .open_or_create()
            .unwrap();
        let mut publisher = service.publisher().create().unwrap();
        let subscriber = service.subscriber().create().unwrap();

        b.iter(|| {
            for _ in 0..BATCH_SIZE {
                let mut sample = publisher.loan().unwrap();
                sample.data = black_box([0xEFu8; 4096]);
                sample.send().unwrap();

                let received = subscriber.receive().unwrap().unwrap();
                black_box(&*received);
                drop(received);

                publisher.reclaim();
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_throughput_u64,
    bench_throughput_64b,
    bench_throughput_1k,
    bench_throughput_4k,
);
criterion_main!(benches);
