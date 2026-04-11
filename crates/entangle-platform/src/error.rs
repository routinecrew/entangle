use thiserror::Error;

/// Platform-level errors for OS abstractions.
#[derive(Error, Debug)]
pub enum PlatformError {
    #[error("shared memory creation failed for '{name}': {reason}")]
    SharedMemoryCreate { name: String, reason: String },

    #[error("shared memory open failed for '{name}': {reason}")]
    SharedMemoryOpen { name: String, reason: String },

    #[error("shared memory unlink failed for '{name}': {reason}")]
    SharedMemoryUnlink { name: String, reason: String },

    #[error("mmap failed: {reason}")]
    Mmap { reason: String },

    #[error("file lock error: {reason}")]
    FileLock { reason: String },

    #[error("event notification error: {reason}")]
    Event { reason: String },

    #[error("signal handling error: {reason}")]
    Signal { reason: String },

    #[error("process monitor error: {reason}")]
    ProcessMonitor { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
