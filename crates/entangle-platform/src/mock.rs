// 테스트용 mock 구현 — contracts/mock.rs에서 복사

/// 테스트용 가짜 공유 메모리. 실제 shm_open 대신 힙 할당.
pub struct MockSharedMemory {
    pub name: String,
    pub data: Vec<u8>,
    pub size: usize,
}

impl MockSharedMemory {
    pub fn create(name: &str, size: usize) -> Self {
        Self {
            name: name.to_string(),
            data: vec![0u8; size],
            size,
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }
}
