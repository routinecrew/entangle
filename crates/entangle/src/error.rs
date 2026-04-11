use thiserror::Error;

/// Top-level error type exposed to users.
#[derive(Error, Debug)]
pub enum IpcError {
    #[error("service error: {0}")]
    Service(#[from] ServiceError),
    #[error("port error: {0}")]
    Port(#[from] PortError),
    #[error("platform error: {0}")]
    Platform(#[from] entangle_platform::PlatformError),
}

/// Service layer errors.
#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("service '{name}' already exists")]
    AlreadyExists { name: String },
    #[error("service '{name}' not found")]
    NotFound { name: String },
    #[error("incompatible QoS: {reason}")]
    IncompatibleQos { reason: String },
    #[error("service corrupted: {reason}")]
    Corrupted { reason: String },
    #[error("version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: u32, found: u32 },
    #[error("platform error: {0}")]
    Platform(#[from] entangle_platform::PlatformError),
}

/// Port layer errors.
#[derive(Error, Debug)]
pub enum PortError {
    #[error("loan failed: {0}")]
    Loan(#[from] entangle_transport::LoanError),
    #[error("send failed: {0}")]
    Send(#[from] entangle_transport::SendError),
    #[error("receive failed: {0}")]
    Receive(#[from] entangle_transport::ReceiveError),
    #[error("connection lost to peer {peer_id:032x}")]
    ConnectionLost { peer_id: u128 },
}
