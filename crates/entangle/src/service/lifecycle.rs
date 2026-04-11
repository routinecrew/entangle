use std::path::Path;

use crate::error::ServiceError;
use crate::service::config::StaticConfig;
use crate::service::registry::ServiceRegistry;

/// Common service lifecycle operations shared by all messaging patterns.
///
/// Eliminates the 70% code duplication that iceoryx2 has across its
/// PubSub/Event/ReqRes/Blackboard builders.
pub struct ServiceLifecycle {
    registry: ServiceRegistry,
}

impl ServiceLifecycle {
    pub fn new(shm_root: &Path) -> Self {
        Self {
            registry: ServiceRegistry::new(shm_root),
        }
    }

    /// Create a new service. Fails if it already exists.
    pub fn create(&self, config: &StaticConfig) -> Result<(), ServiceError> {
        self.registry.register(config)?;
        Ok(())
    }

    /// Open an existing service. Validates compatibility.
    pub fn open(&self, expected: &StaticConfig) -> Result<StaticConfig, ServiceError> {
        let existing = self.registry.lookup(&expected.service_name)?;
        expected.validate_compatibility(&existing)?;
        Ok(existing)
    }

    /// Open or create a service.
    pub fn open_or_create(&self, config: &StaticConfig) -> Result<StaticConfig, ServiceError> {
        match self.open(config) {
            Ok(existing) => Ok(existing),
            Err(ServiceError::NotFound { .. }) => {
                self.create(config)?;
                Ok(config.clone())
            }
            Err(e) => Err(e),
        }
    }

    /// Unregister a service.
    pub fn destroy(&self, name: &str) -> Result<(), ServiceError> {
        self.registry.unregister(name)
    }

    /// Access the underlying registry.
    pub fn registry(&self) -> &ServiceRegistry {
        &self.registry
    }
}
