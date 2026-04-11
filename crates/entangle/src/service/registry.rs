use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use tracing::debug;

use crate::error::ServiceError;
use crate::service::config::StaticConfig;

/// File-based service registry for service discovery.
///
/// Services register by writing their StaticConfig as a RON file under
/// `{shm_root}/services/{hash}/`. Other processes can discover services
/// by scanning this directory.
pub struct ServiceRegistry {
    root: PathBuf,
}

impl ServiceRegistry {
    pub fn new(shm_root: &Path) -> Self {
        Self {
            root: shm_root.join("services"),
        }
    }

    /// Register a service by writing its config.
    ///
    /// Uses `O_CREAT | O_EXCL` for atomic creation — no TOCTOU race.
    pub fn register(&self, config: &StaticConfig) -> Result<PathBuf, ServiceError> {
        fs::create_dir_all(&self.root)
            .map_err(|e| ServiceError::Platform(entangle_platform::PlatformError::Io(e)))?;

        let path = self.service_path(&config.service_name);

        let serialized = ron::to_string(config).map_err(|e| ServiceError::Corrupted {
            reason: format!("serialization failed: {e}"),
        })?;

        // Atomic: create_new(true) maps to O_CREAT | O_EXCL — fails if file exists.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    ServiceError::AlreadyExists {
                        name: config.service_name.clone(),
                    }
                } else {
                    ServiceError::Platform(entangle_platform::PlatformError::Io(e))
                }
            })?;

        file.write_all(serialized.as_bytes())
            .map_err(|e| ServiceError::Platform(entangle_platform::PlatformError::Io(e)))?;

        debug!(service = %config.service_name, "service registered");
        Ok(path)
    }

    /// Look up a service by name.
    pub fn lookup(&self, name: &str) -> Result<StaticConfig, ServiceError> {
        let path = self.service_path(name);
        let data = fs::read_to_string(&path).map_err(|_| ServiceError::NotFound {
            name: name.to_string(),
        })?;

        ron::from_str(&data).map_err(|e| ServiceError::Corrupted {
            reason: format!("deserialization failed: {e}"),
        })
    }

    /// Remove a service registration.
    pub fn unregister(&self, name: &str) -> Result<(), ServiceError> {
        let path = self.service_path(name);
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| ServiceError::Platform(entangle_platform::PlatformError::Io(e)))?;
            debug!(service = %name, "service unregistered");
        }
        Ok(())
    }

    /// List all registered services.
    pub fn list(&self) -> Result<Vec<StaticConfig>, ServiceError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        let mut configs = Vec::new();
        let entries = fs::read_dir(&self.root)
            .map_err(|e| ServiceError::Platform(entangle_platform::PlatformError::Io(e)))?;

        for entry in entries.flatten() {
            if let Ok(data) = fs::read_to_string(entry.path()) {
                if let Ok(config) = ron::from_str::<StaticConfig>(&data) {
                    configs.push(config);
                }
            }
        }

        Ok(configs)
    }

    fn service_path(&self, name: &str) -> PathBuf {
        // Replace '/' in service names with '_' for filesystem safety
        let safe_name = name.replace('/', "_");
        self.root.join(format!("{safe_name}.ron"))
    }
}
