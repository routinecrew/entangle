use serde::{Deserialize, Serialize};

use crate::contracts::PatternType;

/// Static configuration written once when a service is created.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaticConfig {
    pub magic: u64,
    pub version: u32,
    pub pattern: PatternType,
    pub service_name: String,
    pub payload_type_name: String,
    pub payload_size: usize,
    pub payload_align: usize,
}

impl StaticConfig {
    pub fn new(
        pattern: PatternType,
        service_name: &str,
        payload_type_name: &str,
        payload_size: usize,
        payload_align: usize,
    ) -> Self {
        Self {
            magic: crate::contracts::MAGIC_NUMBER,
            version: 1,
            pattern,
            service_name: service_name.to_string(),
            payload_type_name: payload_type_name.to_string(),
            payload_size,
            payload_align,
        }
    }

    pub fn validate_compatibility(
        &self,
        other: &StaticConfig,
    ) -> Result<(), crate::error::ServiceError> {
        if self.magic != other.magic {
            return Err(crate::error::ServiceError::Corrupted {
                reason: "invalid magic number".into(),
            });
        }
        if self.version != other.version {
            return Err(crate::error::ServiceError::VersionMismatch {
                expected: self.version,
                found: other.version,
            });
        }
        if self.pattern != other.pattern {
            return Err(crate::error::ServiceError::IncompatibleQos {
                reason: format!(
                    "pattern mismatch: expected {:?}, found {:?}",
                    self.pattern, other.pattern
                ),
            });
        }
        if self.payload_size != other.payload_size || self.payload_align != other.payload_align {
            return Err(crate::error::ServiceError::IncompatibleQos {
                reason: format!(
                    "payload layout mismatch: {}B/{} vs {}B/{}",
                    self.payload_size, self.payload_align, other.payload_size, other.payload_align,
                ),
            });
        }
        Ok(())
    }
}
