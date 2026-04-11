use std::fs;
use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use crate::contracts::NodeId;
use crate::error::PlatformError;
use crate::file_lock::FileLock;

/// Process liveness monitor based on file locks.
///
/// Each node registers itself by acquiring an exclusive file lock.
/// When the process exits (even on crash), the OS automatically releases
/// the fcntl lock, allowing other processes to detect the death.
pub struct ProcessMonitor {
    lock: FileLock,
    node_id: NodeId,
    #[allow(dead_code)]
    root_dir: PathBuf,
}

impl ProcessMonitor {
    /// Register the current process with the given node ID.
    ///
    /// Creates a lock file at `{root}/nodes/{node_id}` and acquires an
    /// exclusive lock. The lock is held for the lifetime of this struct.
    pub fn register(node_id: NodeId, root_dir: &Path) -> Result<Self, PlatformError> {
        let nodes_dir = root_dir.join("nodes");
        fs::create_dir_all(&nodes_dir).map_err(|e| PlatformError::ProcessMonitor {
            reason: format!("cannot create nodes dir: {e}"),
        })?;

        let lock_path = nodes_dir.join(format!("{:032x}", node_id.0));
        let lock = FileLock::acquire(&lock_path).map_err(|e| PlatformError::ProcessMonitor {
            reason: format!("cannot acquire node lock: {e}"),
        })?;

        debug!(node_id = %format!("{:032x}", node_id.0), "process registered");

        Ok(Self {
            lock,
            node_id,
            root_dir: root_dir.to_path_buf(),
        })
    }

    /// Check if a node is still alive.
    pub fn is_alive(node_id: &NodeId, root_dir: &Path) -> bool {
        let lock_path = root_dir.join("nodes").join(format!("{:032x}", node_id.0));
        FileLock::is_locked(&lock_path)
    }

    /// List all registered node IDs (alive or dead).
    pub fn list_nodes(root_dir: &Path) -> Result<Vec<NodeId>, PlatformError> {
        let nodes_dir = root_dir.join("nodes");
        if !nodes_dir.exists() {
            return Ok(Vec::new());
        }

        let mut nodes = Vec::new();
        let entries = fs::read_dir(&nodes_dir).map_err(|e| PlatformError::ProcessMonitor {
            reason: format!("cannot read nodes dir: {e}"),
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| PlatformError::ProcessMonitor {
                reason: format!("cannot read dir entry: {e}"),
            })?;
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(id) = u128::from_str_radix(name, 16) {
                    nodes.push(NodeId(id));
                }
            }
        }

        Ok(nodes)
    }

    /// Clean up dead nodes: remove lock files for processes that are no longer alive.
    pub fn cleanup_dead_nodes(root_dir: &Path) -> Result<Vec<NodeId>, PlatformError> {
        let nodes = Self::list_nodes(root_dir)?;
        let mut dead = Vec::new();

        for node_id in &nodes {
            if !Self::is_alive(node_id, root_dir) {
                let lock_path = root_dir.join("nodes").join(format!("{:032x}", node_id.0));
                if let Err(e) = fs::remove_file(&lock_path) {
                    warn!(
                        node_id = %format!("{:032x}", node_id.0),
                        error = %e,
                        "failed to clean up dead node file"
                    );
                } else {
                    debug!(node_id = %format!("{:032x}", node_id.0), "cleaned up dead node");
                    dead.push(*node_id);
                }
            }
        }

        Ok(dead)
    }

    /// The node ID of this process.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Deregister and remove the lock file.
    pub fn deregister(self) -> Result<(), PlatformError> {
        self.lock
            .release_and_remove()
            .map_err(|e| PlatformError::ProcessMonitor {
                reason: format!("deregister failed: {e}"),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("entangle_pm_test_{ts}"))
    }

    #[test]
    fn register_and_list() {
        let root = test_root();
        let node_id = NodeId(42);

        let monitor = ProcessMonitor::register(node_id, &root).unwrap();

        let nodes = ProcessMonitor::list_nodes(&root).unwrap();
        assert!(nodes.contains(&node_id));

        drop(monitor);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cleanup_dead() {
        let root = test_root();
        let node_id = NodeId(99);

        // Create and immediately drop the monitor to simulate a dead process.
        // Note: flock from the same process won't detect our own lock,
        // so after drop the lock is released and cleanup will find it dead.
        let monitor = ProcessMonitor::register(node_id, &root).unwrap();
        drop(monitor);

        let dead = ProcessMonitor::cleanup_dead_nodes(&root).unwrap();
        assert!(dead.contains(&node_id));

        let _ = fs::remove_dir_all(&root);
    }
}
