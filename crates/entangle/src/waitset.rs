use std::os::fd::RawFd;
use std::time::Duration;

/// WaitSet multiplexes multiple event sources (subscribers, listeners).
///
/// Uses poll(2) to wait for activity on any attached port's file descriptor.
/// When triggered, the caller can iterate over ready sources.
pub struct WaitSet {
    fds: Vec<WaitEntry>,
}

struct WaitEntry {
    fd: RawFd,
    id: usize,
}

/// Identifies a source attached to the WaitSet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttachmentId(usize);

impl WaitSet {
    /// Create a new empty WaitSet.
    pub fn new() -> Self {
        Self { fds: Vec::new() }
    }

    /// Attach a file descriptor (from a Listener or other pollable source).
    /// Returns an ID for identifying which source triggered.
    pub fn attach_fd(&mut self, fd: RawFd) -> AttachmentId {
        let id = self.fds.len();
        self.fds.push(WaitEntry { fd, id });
        AttachmentId(id)
    }

    /// Attach a Listener port.
    pub fn attach_listener(&mut self, listener: &crate::port::listener::Listener) -> AttachmentId {
        self.attach_fd(listener.raw_fd())
    }

    /// Wait for any attached source to become ready.
    /// Returns the IDs of all triggered sources.
    ///
    /// If `timeout` is `None`, blocks indefinitely.
    pub fn wait(&self, timeout: Option<Duration>) -> Vec<AttachmentId> {
        if self.fds.is_empty() {
            return Vec::new();
        }

        let mut pollfds: Vec<libc::pollfd> = self
            .fds
            .iter()
            .map(|entry| libc::pollfd {
                fd: entry.fd,
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();

        let timeout_ms = match timeout {
            Some(d) => d.as_millis() as i32,
            None => -1,
        };

        // Safety: pollfds is a valid array of pollfd structs
        let ret = unsafe {
            libc::poll(
                pollfds.as_mut_ptr(),
                pollfds.len() as libc::nfds_t,
                timeout_ms,
            )
        };

        if ret <= 0 {
            return Vec::new();
        }

        pollfds
            .iter()
            .enumerate()
            .filter(|(_, pfd)| pfd.revents & libc::POLLIN != 0)
            .map(|(i, _)| AttachmentId(i))
            .collect()
    }
}

impl Default for WaitSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_waitset_returns_empty() {
        let ws = WaitSet::new();
        let triggered = ws.wait(Some(Duration::from_millis(10)));
        assert!(triggered.is_empty());
    }
}
