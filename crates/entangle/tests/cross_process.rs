//! Cross-process simulation tests using std::thread.
//!
//! These tests simulate multi-process IPC scenarios by running producers,
//! consumers, servers, and clients on separate threads with shared-memory
//! backed entangle services.

use std::sync::Arc;
use std::thread;

use entangle::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct SensorData {
    timestamp: u64,
    value: f64,
    flags: u32,
    _pad: u32,
}

unsafe impl ZeroCopySafe for SensorData {}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct AddRequest {
    a: u64,
    b: u64,
}
unsafe impl ZeroCopySafe for AddRequest {}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct AddResponse {
    result: u64,
}
unsafe impl ZeroCopySafe for AddResponse {}

fn unique_shm_root(test_name: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("/tmp/entangle_test_{test_name}_{ts}/")
}

fn test_config(test_name: &str) -> entangle::config::EntangleConfig {
    entangle::config::EntangleConfig {
        shm_root: unique_shm_root(test_name),
        ..Default::default()
    }
}

// ==========================================================================
// a) Producer thread sends N messages, consumer thread receives all N in order
// ==========================================================================

#[test]
fn test_cross_thread_pubsub() {
    let config = test_config("cross_pubsub");
    let node = Node::builder().config(config).create().unwrap();

    let n = 200u64;

    let service = node
        .service("cross/pubsub")
        .publish_subscribe::<u64>()
        .max_loaned_samples(32)
        .open_or_create()
        .unwrap();

    let mut publisher = service.publisher().create().unwrap();
    let subscriber = service.subscriber().create().unwrap();

    let producer = thread::spawn(move || {
        for i in 0..n {
            loop {
                match publisher.loan() {
                    Ok(mut sample) => {
                        *sample = i;
                        sample.send().unwrap();
                        break;
                    }
                    Err(_) => {
                        publisher.reclaim();
                        std::hint::spin_loop();
                    }
                }
            }
        }
        publisher
    });

    let consumer = thread::spawn(move || {
        let mut received = Vec::with_capacity(n as usize);
        while received.len() < n as usize {
            if let Ok(Some(sample)) = subscriber.receive() {
                received.push(*sample);
            } else {
                std::hint::spin_loop();
            }
        }
        received
    });

    let _pub_port = producer.join().unwrap();
    let received = consumer.join().unwrap();

    // Verify FIFO ordering and completeness.
    let expected: Vec<u64> = (0..n).collect();
    assert_eq!(received, expected);
}

// ==========================================================================
// b) Server thread handles requests, client thread sends and verifies
// ==========================================================================

#[test]
fn test_cross_thread_reqres() {
    let config = test_config("cross_reqres");
    let node = Node::builder().config(config).create().unwrap();

    let service = node
        .service("cross/reqres")
        .request_response::<AddRequest, AddResponse>()
        .open_or_create()
        .unwrap();

    let server = service.server().create().unwrap();
    let client = service.client().create().unwrap();

    let request_count = 50u64;

    // Server thread: receive requests and respond with a + b.
    let server_handle = thread::spawn(move || {
        for _ in 0..request_count {
            loop {
                if let Ok(Some(active_req)) = server.receive() {
                    let sum = active_req.request().a + active_req.request().b;
                    active_req.respond(&AddResponse { result: sum }).unwrap();
                    break;
                }
                std::hint::spin_loop();
            }
        }
    });

    // Client thread: send requests and verify responses.
    let client_handle = thread::spawn(move || {
        for i in 0..request_count {
            let req = AddRequest { a: i, b: i * 3 };
            let pending = client.send(&req).unwrap();

            let resp = pending.receive().unwrap();
            assert_eq!(resp.result, i + i * 3);
        }
    });

    server_handle.join().unwrap();
    client_handle.join().unwrap();
}

// ==========================================================================
// c) Writer thread updates rapidly, reader thread verifies no torn reads
// ==========================================================================

#[test]
fn test_cross_thread_blackboard() {
    let config = test_config("cross_bb");
    let node = Node::builder().config(config).create().unwrap();

    let service = node
        .service("cross/blackboard")
        .blackboard::<SensorData>()
        .open_or_create()
        .unwrap();

    let writer = Arc::new(service.writer().create().unwrap());
    let reader = Arc::new(service.reader().create().unwrap());

    let n = 2000u64;

    // Writer thread: rapidly update with consistent data.
    // Each write has timestamp == flags (as u32), so reader can verify consistency.
    let w = {
        let writer = writer.clone();
        thread::spawn(move || {
            for i in 0..n {
                let data = SensorData {
                    timestamp: i,
                    value: i as f64 * 1.5,
                    flags: i as u32,
                    _pad: 0,
                };
                writer.write(&data).unwrap();
            }
        })
    };

    // Reader thread: verify that every read is internally consistent (no torn reads).
    let r = {
        let reader = reader.clone();
        thread::spawn(move || {
            let mut reads = 0u64;
            while reads < 500 {
                if let Ok(Some(data)) = reader.read() {
                    // Consistency check: timestamp and flags must correspond.
                    assert_eq!(
                        data.timestamp as u32, data.flags,
                        "torn read detected: timestamp={} flags={}",
                        data.timestamp, data.flags
                    );
                    // Value must match the formula.
                    let expected_value = data.timestamp as f64 * 1.5;
                    assert!(
                        (data.value - expected_value).abs() < f64::EPSILON,
                        "torn read in value field"
                    );
                    assert!(
                        data.timestamp < n,
                        "read out-of-range timestamp: {}",
                        data.timestamp
                    );
                    reads += 1;
                }
                std::hint::spin_loop();
            }
        })
    };

    w.join().unwrap();
    r.join().unwrap();
}

// ==========================================================================
// d) Multiple services running simultaneously across threads
// ==========================================================================

#[test]
fn test_multiple_services_concurrent() {
    let config = test_config("multi_svc");
    let node = Node::builder().config(config).create().unwrap();

    let msg_count = 50u64;

    // Service 1: PubSub with u64
    let pubsub_svc = node
        .service("multi/pubsub")
        .publish_subscribe::<u64>()
        .max_loaned_samples(16)
        .open_or_create()
        .unwrap();

    let mut pub1 = pubsub_svc.publisher().create().unwrap();
    let sub1 = pubsub_svc.subscriber().create().unwrap();

    // Service 2: ReqRes
    let reqres_svc = node
        .service("multi/reqres")
        .request_response::<AddRequest, AddResponse>()
        .open_or_create()
        .unwrap();

    let server2 = reqres_svc.server().create().unwrap();
    let client2 = reqres_svc.client().create().unwrap();

    // Service 3: Blackboard
    let bb_svc = node
        .service("multi/blackboard")
        .blackboard::<u64>()
        .open_or_create()
        .unwrap();

    let writer3 = Arc::new(bb_svc.writer().create().unwrap());
    let reader3 = Arc::new(bb_svc.reader().create().unwrap());

    // Thread 1: PubSub producer
    let t1 = thread::spawn(move || {
        for i in 0..msg_count {
            loop {
                match pub1.loan() {
                    Ok(mut sample) => {
                        *sample = i;
                        sample.send().unwrap();
                        break;
                    }
                    Err(_) => {
                        pub1.reclaim();
                        std::hint::spin_loop();
                    }
                }
            }
        }
    });

    // Thread 2: PubSub consumer
    let t2 = thread::spawn(move || {
        let mut received = Vec::with_capacity(msg_count as usize);
        while received.len() < msg_count as usize {
            if let Ok(Some(sample)) = sub1.receive() {
                received.push(*sample);
            } else {
                std::hint::spin_loop();
            }
        }
        let expected: Vec<u64> = (0..msg_count).collect();
        assert_eq!(received, expected);
    });

    // Thread 3: ReqRes server
    let t3 = thread::spawn(move || {
        for _ in 0..msg_count {
            loop {
                if let Ok(Some(active_req)) = server2.receive() {
                    let sum = active_req.request().a + active_req.request().b;
                    active_req.respond(&AddResponse { result: sum }).unwrap();
                    break;
                }
                std::hint::spin_loop();
            }
        }
    });

    // Thread 4: ReqRes client
    let t4 = thread::spawn(move || {
        for i in 0..msg_count {
            let pending = client2.send(&AddRequest { a: i, b: 1 }).unwrap();
            let resp = pending.receive().unwrap();
            assert_eq!(resp.result, i + 1);
        }
    });

    // Thread 5: Blackboard writer
    let t5 = {
        let writer3 = writer3.clone();
        thread::spawn(move || {
            for i in 0..msg_count {
                writer3.write(&i).unwrap();
            }
        })
    };

    // Thread 6: Blackboard reader
    let t6 = {
        let reader3 = reader3.clone();
        thread::spawn(move || {
            let mut reads = 0u64;
            while reads < 30 {
                if let Ok(Some(val)) = reader3.read() {
                    assert!(val < msg_count, "blackboard read out-of-range: {val}");
                    reads += 1;
                }
                std::hint::spin_loop();
            }
        })
    };

    t1.join().unwrap();
    t2.join().unwrap();
    t3.join().unwrap();
    t4.join().unwrap();
    t5.join().unwrap();
    t6.join().unwrap();
}

// ==========================================================================
// e) Publisher drops, subscriber should eventually get None
// ==========================================================================

#[test]
fn test_publisher_disconnect_detection() {
    let config = test_config("pub_disconnect");
    let node = Node::builder().config(config).create().unwrap();

    let service = node
        .service("cross/disconnect")
        .publish_subscribe::<u64>()
        .max_loaned_samples(8)
        .open_or_create()
        .unwrap();

    let mut publisher = service.publisher().create().unwrap();
    let subscriber = service.subscriber().create().unwrap();

    // Send a few messages.
    for i in 0..3u64 {
        let mut sample = publisher.loan().unwrap();
        *sample = i;
        sample.send().unwrap();
    }

    // Drain all messages.
    let mut received = Vec::new();
    while let Ok(Some(sample)) = subscriber.receive() {
        received.push(*sample);
    }
    assert_eq!(received, vec![0, 1, 2]);

    // Drop the publisher.
    drop(publisher);

    // After publisher is dropped, subscriber should get None (no more data).
    let result = subscriber.receive().unwrap();
    assert!(result.is_none(), "expected None after publisher disconnect");
}

// ==========================================================================
// f) 10,000 messages sent/received with verification
// ==========================================================================

#[test]
fn test_high_throughput_stress() {
    let config = test_config("stress_10k");
    let node = Node::builder().config(config).create().unwrap();

    let n = 10_000u64;

    let service = node
        .service("cross/stress")
        .publish_subscribe::<u64>()
        .max_loaned_samples(64)
        .open_or_create()
        .unwrap();

    let mut publisher = service.publisher().create().unwrap();
    let subscriber = service.subscriber().create().unwrap();

    let producer = thread::spawn(move || {
        for i in 0..n {
            loop {
                match publisher.loan() {
                    Ok(mut sample) => {
                        *sample = i;
                        sample.send().unwrap();
                        break;
                    }
                    Err(_) => {
                        publisher.reclaim();
                        std::hint::spin_loop();
                    }
                }
            }
        }
        publisher
    });

    let consumer = thread::spawn(move || {
        let mut received = Vec::with_capacity(n as usize);
        while received.len() < n as usize {
            if let Ok(Some(sample)) = subscriber.receive() {
                received.push(*sample);
            } else {
                std::hint::spin_loop();
            }
        }
        received
    });

    let _pub_port = producer.join().unwrap();
    let received = consumer.join().unwrap();

    // Verify all 10,000 messages received in FIFO order.
    assert_eq!(received.len(), n as usize);
    let expected: Vec<u64> = (0..n).collect();
    assert_eq!(received, expected);
}
