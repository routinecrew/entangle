//! Request-Response example.
//!
//! A client sends AddRequest messages to a server, which computes
//! the sum and responds with AddResponse.
//!
//! Run: cargo run --example reqres

use entangle::prelude::*;

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

fn unique_shm_root(name: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("/tmp/entangle_example_{name}_{ts}/")
}

fn main() {
    let config = entangle::config::EntangleConfig {
        shm_root: unique_shm_root("reqres"),
        ..Default::default()
    };
    let node = Node::builder().config(config).create().unwrap();

    let service = node
        .service("calc/add")
        .request_response::<AddRequest, AddResponse>()
        .open_or_create()
        .unwrap();

    let server = service.server().create().unwrap();
    let client = service.client().create().unwrap();

    // Send 5 requests, process each synchronously.
    for i in 0..5u64 {
        let req = AddRequest { a: i, b: i * 10 };
        println!("client: sending {} + {}", req.a, req.b);

        let pending = client.send(&req).unwrap();

        // Server receives and responds.
        let active = server.receive().unwrap().unwrap();
        let sum = active.request().a + active.request().b;
        active.respond(&AddResponse { result: sum }).unwrap();

        // Client gets the result.
        let resp = pending.receive().unwrap();
        println!("client: received result = {}", resp.result);
    }

    println!("done");
}
