//! Blackboard (shared latest-value) example.
//!
//! A writer updates a position value repeatedly; a reader always
//! sees the most recent value (last-writer-wins semantics).
//!
//! Run: cargo run --example blackboard

use entangle::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct Position {
    x: f64,
    y: f64,
    z: f64,
}
unsafe impl ZeroCopySafe for Position {}

fn unique_shm_root(name: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("/tmp/entangle_example_{name}_{ts}/")
}

fn main() {
    let config = entangle::config::EntangleConfig {
        shm_root: unique_shm_root("blackboard"),
        ..Default::default()
    };
    let node = Node::builder().config(config).create().unwrap();

    let service = node
        .service("state/position")
        .blackboard::<Position>()
        .open_or_create()
        .unwrap();

    let writer = service.writer().create().unwrap();
    let reader = service.reader().create().unwrap();

    // Initially no data.
    assert!(reader.read().unwrap().is_none());
    println!("reader: no data yet");

    // Write several positions; only the latest is retained.
    for i in 0..5u64 {
        let pos = Position {
            x: i as f64,
            y: i as f64 * 2.0,
            z: i as f64 * 3.0,
        };
        writer.write(&pos).unwrap();
        println!(
            "writer: wrote x={:.0}, y={:.0}, z={:.0}",
            pos.x, pos.y, pos.z
        );
    }

    // Reader sees the latest value.
    let latest = reader.read().unwrap().unwrap();
    println!(
        "reader: latest x={:.0}, y={:.0}, z={:.0}",
        latest.x, latest.y, latest.z
    );
    assert_eq!(latest.x, 4.0);

    println!("done");
}
