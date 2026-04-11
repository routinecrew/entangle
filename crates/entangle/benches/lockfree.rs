//! Benchmark: Lock-free data structure operations.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use entangle_lockfree::{SpscQueue, UniqueIndexSet};

fn bench_unique_index_set_acquire_release(c: &mut Criterion) {
    let set = UniqueIndexSet::new(1024);

    c.bench_function("index_set_acquire_release", |b| {
        b.iter(|| {
            let idx = set.acquire().unwrap();
            black_box(idx);
            set.release(idx);
        });
    });
}

fn bench_unique_index_set_burst(c: &mut Criterion) {
    let set = UniqueIndexSet::new(256);

    c.bench_function("index_set_burst_64", |b| {
        b.iter(|| {
            let mut indices = [0u32; 64];
            for slot in indices.iter_mut() {
                *slot = set.acquire().unwrap();
            }
            for &idx in indices.iter() {
                set.release(idx);
            }
            black_box(&indices);
        });
    });
}

fn bench_spsc_queue_push_pop(c: &mut Criterion) {
    let queue = SpscQueue::new(1024);

    c.bench_function("spsc_push_pop", |b| {
        b.iter(|| {
            queue.push(black_box(42u64));
            let val = queue.pop().unwrap();
            black_box(val);
        });
    });
}

fn bench_spsc_queue_burst(c: &mut Criterion) {
    let queue = SpscQueue::new(1024);

    c.bench_function("spsc_burst_64", |b| {
        b.iter(|| {
            for i in 0u64..64 {
                queue.push(black_box(i));
            }
            for _ in 0..64 {
                let val = queue.pop().unwrap();
                black_box(val);
            }
        });
    });
}

criterion_group!(
    benches,
    bench_unique_index_set_acquire_release,
    bench_unique_index_set_burst,
    bench_spsc_queue_push_pop,
    bench_spsc_queue_burst,
);
criterion_main!(benches);
