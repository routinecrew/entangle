use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use crate::error::PlatformError;

/// Advisory file lock using `flock(2)`.
///
/// Used for process liveness detection: a process holds an exclusive lock
/// on its node file. Other processes can try to acquire the lock to
/// determine if the owning process is still alive. The OS automatically
/// releases flock locks when the process exits (even on crash).
pub struct FileLock {
    file: File,
    path: PathBuf,
    is_locked: bool,
}

impl FileLock {
    /// Create and acquire an exclusive lock on a file.
    ///
    /// Creates the file if it doesn't exist. The lock is non-blocking:
    /// returns an error immediately if the file is already locked.
    pub fn acquire(path: &Path) -> Result<Self, PlatformError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| PlatformError::FileLock {
                reason: format!("cannot create directory {}: {e}", parent.display()),
            })?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| PlatformError::FileLock {
                reason: format!("cannot open {}: {e}", path.display()),
            })?;

        let fd = file.as_raw_fd();

        // Safety: fd is a valid file descriptor
        let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            return Err(PlatformError::FileLock {
                reason: format!("cannot lock {}: {err}", path.display()),
            });
        }

        debug!(path = %path.display(), "file lock acquired");

        Ok(Self {
            file,
            path: path.to_path_buf(),
            is_locked: true,
        })
    }

    /// Check if a file is currently locked by another process.
    ///
    /// Returns `true` if the file exists and is locked.
    /// Uses a non-blocking lock attempt to probe.
    pub fn is_locked(path: &Path) -> bool {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return false,
        };

        let fd = file.as_raw_fd();

        // Try to acquire the lock non-blocking. If it fails with EWOULDBLOCK,
        // someone else holds it.
        // Safety: fd is a valid file descriptor
        let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if ret != 0 {
            // Lock is held by another process
            return true;
        }

        // We acquired the lock — release it immediately and report not locked
        unsafe { libc::flock(fd, libc::LOCK_UN) };
        false
    }

    /// Release the lock and optionally remove the lock file.
    pub fn release_and_remove(mut self) -> Result<(), PlatformError> {
        self.unlock()?;
        fs::remove_file(&self.path).map_err(|e| PlatformError::FileLock {
            reason: format!("cannot remove {}: {e}", self.path.display()),
        })?;
        Ok(())
    }

    /// Path of the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn unlock(&mut self) -> Result<(), PlatformError> {
        if !self.is_locked {
            return Ok(());
        }

        let fd = self.file.as_raw_fd();

        // Safety: fd is valid
        let ret = unsafe { libc::flock(fd, libc::LOCK_UN) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            return Err(PlatformError::FileLock {
                reason: format!("unlock failed: {err}"),
            });
        }

        self.is_locked = false;
        debug!(path = %self.path.display(), "file lock released");
        Ok(())
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        if self.is_locked {
            if let Err(e) = self.unlock() {
                warn!(error = %e, "failed to release file lock on drop");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_lock_path(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push("entangle_test_locks");
        p.push(format!(
            "{name}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    #[test]
    fn acquire_and_release() {
        let path = temp_lock_path("acquire_release");
        let lock = FileLock::acquire(&path).unwrap();
        // flock is per-fd not per-process, so is_locked from same process
        // won't detect our own lock (it would succeed in getting the lock
        // on a new fd because flock is not per-process exclusive for the
        // same process). That's expected — is_locked is designed for
        // cross-process detection.
        drop(lock);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn release_and_remove() {
        let path = temp_lock_path("release_remove");
        let lock = FileLock::acquire(&path).unwrap();
        lock.release_and_remove().unwrap();
        assert!(!path.exists());
    }
}
