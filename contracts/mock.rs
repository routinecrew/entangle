// ============================================================
// entangle 공유 Mock 구현
// ============================================================
// 각 에이전트가 독립 개발할 때 사용하는 mock 구현체.
// 각 크레이트의 src/mock.rs로 복사하여 사용한다.
// ============================================================

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::ptr;

// shared_types에서 필요한 타입 import (각 크레이트에서 경로 조정)
// use crate::contracts::*;

// ============================================================
// MockSharedMemory — 힙 기반 가짜 공유 메모리
// ============================================================

/// 테스트용 가짜 공유 메모리. 실제 shm_open 대신 힙 할당 사용.
pub struct MockSharedMemory {
    pub name: String,
    pub data: Vec<u8>,
    pub ptr: *mut u8,
    pub size: usize,
}

impl MockSharedMemory {
    pub fn create(name: &str, size: usize) -> Self {
        let mut data = vec![0u8; size];
        let ptr = data.as_mut_ptr();
        Self {
            name: name.to_string(),
            data,
            ptr,
            size,
        }
    }
}

// ============================================================
// MockSharedMemoryProvider
// ============================================================

pub struct MockSharedMemoryProvider {
    segments: Mutex<HashMap<String, Vec<u8>>>,
}

impl MockSharedMemoryProvider {
    pub fn new() -> Self {
        Self {
            segments: Mutex::new(HashMap::new()),
        }
    }
}

// ============================================================
// MockIndexAllocator — 단순 카운터 기반 할당자
// ============================================================

/// 테스트용 인덱스 할당자. Lock-free가 아닌 Mutex 기반.
pub struct MockIndexAllocator {
    free_list: Mutex<Vec<u32>>,
    capacity: u32,
    borrowed: Mutex<u32>,
}

impl MockIndexAllocator {
    pub fn new(capacity: u32) -> Self {
        let free_list = (0..capacity).rev().collect();
        Self {
            free_list: Mutex::new(free_list),
            capacity,
            borrowed: Mutex::new(0),
        }
    }

    pub fn acquire(&self) -> Option<u32> {
        let mut free = self.free_list.lock().unwrap();
        let idx = free.pop()?;
        *self.borrowed.lock().unwrap() += 1;
        Some(idx)
    }

    pub fn release(&self, index: u32) {
        self.free_list.lock().unwrap().push(index);
        *self.borrowed.lock().unwrap() -= 1;
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn borrowed_count(&self) -> u32 {
        *self.borrowed.lock().unwrap()
    }
}

// ============================================================
// MockZeroCopyChannel — 간단한 인메모리 채널
// ============================================================

use std::collections::VecDeque;

/// 테스트용 zero-copy 채널. 실제 공유 메모리 대신 VecDeque 사용.
pub struct MockZeroCopyChannel {
    send_queue: Mutex<VecDeque<u64>>,
    return_queue: Mutex<VecDeque<u64>>,
}

impl MockZeroCopyChannel {
    pub fn new(_capacity: usize) -> Self {
        Self {
            send_queue: Mutex::new(VecDeque::new()),
            return_queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn send(&self, offset: u64) {
        self.send_queue.lock().unwrap().push_back(offset);
    }

    pub fn receive(&self) -> Option<u64> {
        self.send_queue.lock().unwrap().pop_front()
    }

    pub fn return_slot(&self, offset: u64) {
        self.return_queue.lock().unwrap().push_back(offset);
    }

    pub fn reclaim(&self) -> Option<u64> {
        self.return_queue.lock().unwrap().pop_front()
    }
}
