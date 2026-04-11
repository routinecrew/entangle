//! Basic Publish-Subscribe example.
//!
//! Demonstrates the loan -> write -> send -> receive pattern
//! with 10 SensorData samples.
//!
//! Run: cargo run --example pubsub

use entangle::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct SensorData {
    timestamp: u64,
    value: f64,
}

// Safety: SensorData is #[repr(C)], all fields are ZeroCopySafe, no heap, no Drop.
unsafe impl ZeroCopySafe for SensorData {}

fn unique_shm_root(name: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("/tmp/entangle_example_{name}_{ts}/")
}

fn main() {
    let config = entangle::config::EntangleConfig {
        shm_root: unique_shm_root("pubsub"),
        ..Default::default()
    };
    let node = Node::builder().config(config).create().unwrap();

    let service = node
        .service("sensor/temperature")
        .publish_subscribe::<SensorData>()
        .max_loaned_samples(16)
        .open_or_create()
        .unwrap();

    let mut publisher = service.publisher().create().unwrap();
    let subscriber = service.subscriber().create().unwrap();

    // Publish 10 sensor readings.
    for i in 0..10u64 {
        let mut sample = publisher.loan().unwrap();
        sample.timestamp = i * 100;
        sample.value = 20.0 + (i as f64) * 0.5;
        sample.send().unwrap();
        println!(
            "sent: timestamp={}, value={:.1}",
            i * 100,
            20.0 + (i as f64) * 0.5
        );
    }

    // Receive all samples (FIFO order).
    for _ in 0..10 {
        if let Some(sample) = subscriber.receive().unwrap() {
            println!(
                "received: timestamp={}, value={:.1}",
                sample.timestamp, sample.value
            );
            drop(sample);
        }
    }

    // No more data.
    assert!(subscriber.receive().unwrap().is_none());

    // Reclaim all returned slots.
    publisher.reclaim();
    println!("active loans after reclaim: {}", publisher.active_loans());
}
