//! WaitSet multiplexing example.
//!
//! Attaches multiple event listeners to a WaitSet and demonstrates
//! multiplexed waiting with poll(2) under the hood.
//!
//! Run: cargo run --example waitset

use std::thread;
use std::time::Duration;

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
        shm_root: unique_shm_root("waitset"),
        ..Default::default()
    };
    let node = Node::builder().config(config).create().unwrap();

    // Create two independent event services.
    let svc_a = node
        .service("event/sensor-a")
        .event()
        .open_or_create()
        .unwrap();
    let svc_b = node
        .service("event/sensor-b")
        .event()
        .open_or_create()
        .unwrap();

    let notifier_a = svc_a.notifier().create().unwrap();
    let notifier_b = svc_b.notifier().create().unwrap();
    let listener_a = svc_a.listener().create().unwrap();
    let listener_b = svc_b.listener().create().unwrap();

    // Build a WaitSet and attach both listeners.
    let mut waitset = WaitSet::new();
    let id_a = waitset.attach_listener(&listener_a);
    let id_b = waitset.attach_listener(&listener_b);

    println!("attached listener A as {:?}", id_a);
    println!("attached listener B as {:?}", id_b);

    // Fire notifier B from a separate thread after a short delay.
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        println!("notifier B: sending event");
        notifier_b.notify().unwrap();
    });

    // Wait with a timeout. Only listener B should trigger.
    println!("waitset: waiting for events (timeout 500ms)...");
    let triggered = waitset.wait(Some(Duration::from_millis(500)));

    for id in &triggered {
        if *id == id_a {
            println!("  -> listener A triggered");
        } else if *id == id_b {
            println!("  -> listener B triggered");
        }
    }

    if triggered.is_empty() {
        println!("  -> no events (timed out)");
    }

    // Also fire notifier A so nothing is left dangling.
    notifier_a.notify().unwrap();
    println!("done");
}
