//! Benchmark: PubSub single-message round-trip latency.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
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

fn bench_pubsub_roundtrip_u64(c: &mut Criterion) {
    let node = Node::builder()
        .config(bench_config("latency_u64"))
        .create()
        .unwrap();

    let service = node
        .service("bench/latency_u64")
        .publish_subscribe::<u64>()
        .max_loaned_samples(16)
        .open_or_create()
        .unwrap();

    let mut publisher = service.publisher().create().unwrap();
    let subscriber = service.subscriber().create().unwrap();

    c.bench_function("pubsub_roundtrip_u64", |b| {
        b.iter(|| {
            let mut sample = publisher.loan().unwrap();
            *sample = black_box(42u64);
            sample.send().unwrap();

            let received = subscriber.receive().unwrap().unwrap();
            black_box(&*received);
            drop(received);

            publisher.reclaim();
        });
    });
}

/// A larger payload for latency testing.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct SensorData {
    timestamp: u64,
    value: f64,
    flags: u32,
    _pad: u32,
}

// Safety: SensorData is #[repr(C)], all fields are ZeroCopySafe, no heap, no Drop.
unsafe impl ZeroCopySafe for SensorData {}

fn bench_pubsub_roundtrip_sensor(c: &mut Criterion) {
    let node = Node::builder()
        .config(bench_config("latency_sensor"))
        .create()
        .unwrap();

    let service = node
        .service("bench/latency_sensor")
        .publish_subscribe::<SensorData>()
        .max_loaned_samples(16)
        .open_or_create()
        .unwrap();

    let mut publisher = service.publisher().create().unwrap();
    let subscriber = service.subscriber().create().unwrap();

    c.bench_function("pubsub_roundtrip_sensor_24B", |b| {
        b.iter(|| {
            let mut sample = publisher.loan().unwrap();
            sample.timestamp = black_box(1000);
            sample.value = black_box(42.5);
            sample.flags = black_box(1);
            sample._pad = 0;
            sample.send().unwrap();

            let received = subscriber.receive().unwrap().unwrap();
            black_box(&*received);
            drop(received);

            publisher.reclaim();
        });
    });
}

fn bench_loan_only(c: &mut Criterion) {
    let node = Node::builder()
        .config(bench_config("latency_loan"))
        .create()
        .unwrap();

    let service = node
        .service("bench/latency_loan")
        .publish_subscribe::<u64>()
        .max_loaned_samples(16)
        .open_or_create()
        .unwrap();

    let mut publisher = service.publisher().create().unwrap();
    let _subscriber = service.subscriber().create().unwrap();

    c.bench_function("loan_drop_cycle", |b| {
        b.iter(|| {
            let sample = publisher.loan().unwrap();
            black_box(&*sample);
            drop(sample);
        });
    });
}

criterion_group!(
    benches,
    bench_pubsub_roundtrip_u64,
    bench_pubsub_roundtrip_sensor,
    bench_loan_only,
);
criterion_main!(benches);
