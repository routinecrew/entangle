use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use tracing::debug;

use crate::error::PlatformError;

/// Cross-process event notification mechanism.
///
/// On Linux, uses `eventfd`. On macOS/BSD, uses a `pipe` pair since
/// eventfd is Linux-specific.
pub struct EventNotification {
    #[cfg(target_os = "linux")]
    fd: OwnedFd,

    #[cfg(not(target_os = "linux"))]
    read_fd: OwnedFd,
    #[cfg(not(target_os = "linux"))]
    write_fd: OwnedFd,
}

impl EventNotification {
    /// Create a new event notification instance.
    pub fn new() -> Result<Self, PlatformError> {
        #[cfg(target_os = "linux")]
        {
            Self::new_eventfd()
        }

        #[cfg(not(target_os = "linux"))]
        {
            Self::new_pipe()
        }
    }

    /// Signal the event (wake up a waiting thread/process).
    pub fn notify(&self) -> Result<(), PlatformError> {
        #[cfg(target_os = "linux")]
        {
            let val: u64 = 1;
            let ret = unsafe {
                libc::write(
                    self.fd.as_raw_fd(),
                    &val as *const u64 as *const libc::c_void,
                    8,
                )
            };
            if ret != 8 {
                return Err(PlatformError::Event {
                    reason: format!("eventfd write failed: {}", io::Error::last_os_error()),
                });
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let buf: [u8; 1] = [1];
            // Safety: write_fd is a valid pipe file descriptor
            let ret = unsafe {
                libc::write(
                    self.write_fd.as_raw_fd(),
                    buf.as_ptr() as *const libc::c_void,
                    1,
                )
            };
            if ret != 1 {
                return Err(PlatformError::Event {
                    reason: format!("pipe write failed: {}", io::Error::last_os_error()),
                });
            }
        }

        Ok(())
    }

    /// Wait for an event notification. Blocks until signaled.
    pub fn wait(&self) -> Result<(), PlatformError> {
        #[cfg(target_os = "linux")]
        {
            let mut val: u64 = 0;
            let ret = unsafe {
                libc::read(
                    self.fd.as_raw_fd(),
                    &mut val as *mut u64 as *mut libc::c_void,
                    8,
                )
            };
            if ret != 8 {
                return Err(PlatformError::Event {
                    reason: format!("eventfd read failed: {}", io::Error::last_os_error()),
                });
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let mut buf: [u8; 1] = [0];
            // Safety: read_fd is a valid pipe file descriptor
            let ret = unsafe {
                libc::read(
                    self.read_fd.as_raw_fd(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    1,
                )
            };
            if ret != 1 {
                return Err(PlatformError::Event {
                    reason: format!("pipe read failed: {}", io::Error::last_os_error()),
                });
            }
        }

        Ok(())
    }

    /// Try to consume a pending notification without blocking.
    /// Returns `true` if an event was consumed, `false` if none pending.
    pub fn try_wait(&self) -> Result<bool, PlatformError> {
        let read_fd = self.read_raw_fd();
        set_nonblocking(read_fd, true)?;
        let result = self.wait();
        set_nonblocking(read_fd, false)?;

        match result {
            Ok(()) => Ok(true),
            Err(PlatformError::Event { .. }) => {
                // EAGAIN/EWOULDBLOCK means no event pending
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    Ok(false)
                } else {
                    Err(PlatformError::Event {
                        reason: format!("try_wait failed: {err}"),
                    })
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Returns the raw file descriptor for the readable end (for poll/select/epoll).
    pub fn read_raw_fd(&self) -> RawFd {
        #[cfg(target_os = "linux")]
        {
            self.fd.as_raw_fd()
        }
        #[cfg(not(target_os = "linux"))]
        {
            self.read_fd.as_raw_fd()
        }
    }

    #[cfg(target_os = "linux")]
    fn new_eventfd() -> Result<Self, PlatformError> {
        // Safety: eventfd is a well-defined Linux syscall
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
        if fd < 0 {
            return Err(PlatformError::Event {
                reason: format!("eventfd creation failed: {}", io::Error::last_os_error()),
            });
        }
        debug!(fd, "eventfd created");
        // Safety: fd is a valid file descriptor just returned by eventfd
        Ok(Self {
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn new_pipe() -> Result<Self, PlatformError> {
        let mut fds = [0i32; 2];
        // Safety: pipe() is a standard POSIX syscall
        let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
        if ret != 0 {
            return Err(PlatformError::Event {
                reason: format!("pipe creation failed: {}", io::Error::last_os_error()),
            });
        }
        debug!(read_fd = fds[0], write_fd = fds[1], "pipe created");
        // Safety: fds are valid file descriptors just returned by pipe
        Ok(Self {
            read_fd: unsafe { OwnedFd::from_raw_fd(fds[0]) },
            write_fd: unsafe { OwnedFd::from_raw_fd(fds[1]) },
        })
    }
}

fn set_nonblocking(fd: RawFd, nonblocking: bool) -> Result<(), PlatformError> {
    // Safety: fd is a valid file descriptor
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(PlatformError::Event {
            reason: format!("fcntl F_GETFL failed: {}", io::Error::last_os_error()),
        });
    }
    let new_flags = if nonblocking {
        flags | libc::O_NONBLOCK
    } else {
        flags & !libc::O_NONBLOCK
    };
    // Safety: valid fd and flags
    let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, new_flags) };
    if ret < 0 {
        return Err(PlatformError::Event {
            reason: format!("fcntl F_SETFL failed: {}", io::Error::last_os_error()),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_and_wait() {
        let event = EventNotification::new().unwrap();
        event.notify().unwrap();
        event.wait().unwrap();
    }

    #[test]
    fn try_wait_empty() {
        let event = EventNotification::new().unwrap();
        assert!(!event.try_wait().unwrap());
    }

    #[test]
    fn try_wait_with_pending() {
        let event = EventNotification::new().unwrap();
        event.notify().unwrap();
        assert!(event.try_wait().unwrap());
        assert!(!event.try_wait().unwrap());
    }
}
