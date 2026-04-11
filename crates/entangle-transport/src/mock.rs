// 테스트용 mock — contracts/mock.rs에서 복사
use std::collections::VecDeque;
use std::sync::Mutex;

/// 테스트용 zero-copy 채널
pub struct MockZeroCopyChannel {
    send_queue: Mutex<VecDeque<u64>>,
    return_queue: Mutex<VecDeque<u64>>,
}

impl Default for MockZeroCopyChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl MockZeroCopyChannel {
    pub fn new() -> Self {
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
