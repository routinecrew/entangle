//! Multi-threaded pub/sub example simulating cross-process IPC.
//!
//! A producer thread publishes 50 values while a consumer thread
//! receives them concurrently, demonstrating the reclaim loop
//! needed when max loaned samples are exhausted.
//!
//! Run: cargo run --example multi_process

use std::thread;

use entangle::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct Message {
    seq: u64,
    payload: [u8; 64],
}
unsafe impl ZeroCopySafe for Message {}

fn unique_shm_root(name: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("/tmp/entangle_example_{name}_{ts}/")
}

fn main() {
    let config = entangle::config::EntangleConfig {
        shm_root: unique_shm_root("multi_process"),
        ..Default::default()
    };
    let node = Node::builder().config(config).create().unwrap();

    let service = node
        .service("ipc/messages")
        .publish_subscribe::<Message>()
        .max_loaned_samples(16)
        .open_or_create()
        .unwrap();

    let total: u64 = 50;

    let mut publisher = service.publisher().create().unwrap();
    let subscriber = service.subscriber().create().unwrap();

    // Producer thread.
    let producer = thread::spawn(move || {
        for i in 0..total {
            loop {
                match publisher.loan() {
                    Ok(mut sample) => {
                        sample.seq = i;
                        sample.payload = [0u8; 64];
                        // Put sequence number in the first 8 bytes.
                        sample.payload[..8].copy_from_slice(&i.to_le_bytes());
                        sample.send().unwrap();
                        break;
                    }
                    Err(_) => {
                        // Max loans reached; reclaim returned slots and retry.
                        publisher.reclaim();
                        std::hint::spin_loop();
                    }
                }
            }
        }
        println!("producer: sent {} messages", total);
        publisher
    });

    // Consumer thread.
    let consumer = thread::spawn(move || {
        let mut received = 0u64;
        while received < total {
            if let Ok(Some(sample)) = subscriber.receive() {
                if received % 10 == 0 {
                    println!(
                        "consumer: seq={}, first payload byte={}",
                        sample.seq, sample.payload[0]
                    );
                }
                received += 1;
            } else {
                std::hint::spin_loop();
            }
        }
        println!("consumer: received {} messages", received);
    });

    let publisher = producer.join().unwrap();
    consumer.join().unwrap();

    publisher.reclaim();
    println!("done: all slots reclaimed");
}
