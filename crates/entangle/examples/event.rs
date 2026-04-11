//! Event notification example.
//!
//! Demonstrates lightweight event signalling between a notifier
//! and a listener running in separate threads.
//!
//! Run: cargo run --example event

use std::thread;

use entangle::prelude::*;

fn unique_shm_root(name: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("/tmp/entangle_example_{name}_{ts}/")
}

fn main() {
    let config = entangle::config::EntangleConfig {
        shm_root: unique_shm_root("event"),
        ..Default::default()
    };
    let node = Node::builder().config(config).create().unwrap();

    let service = node
        .service("system/shutdown")
        .event()
        .open_or_create()
        .unwrap();

    let notifier = service.notifier().create().unwrap();
    let listener = service.listener().create().unwrap();

    // Listener thread: wait for the event.
    let listener_handle = thread::spawn(move || {
        println!("listener: waiting for event...");
        listener.wait().unwrap();
        println!("listener: event received");
    });

    // Brief pause so the listener is ready.
    thread::sleep(std::time::Duration::from_millis(50));

    // Notify from the main thread.
    println!("notifier: sending event");
    notifier.notify().unwrap();

    listener_handle.join().unwrap();
    println!("done");
}
