use std::sync::atomic::{AtomicBool, Ordering};

use nix::sys::signal::{self, SigHandler, Signal as NixSignal};
use tracing::debug;

use crate::error::PlatformError;

/// Global flag set when SIGTERM or SIGINT is received.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Global flag for SIGTERM specifically.
static SIGTERM_RECEIVED: AtomicBool = AtomicBool::new(false);

/// Global flag for SIGINT specifically.
static SIGINT_RECEIVED: AtomicBool = AtomicBool::new(false);

/// Signal handler that sets atomic flags.
///
/// # Safety
/// This is a signal handler — only async-signal-safe operations (atomic stores) are used.
extern "C" fn signal_handler(sig: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    match sig {
        libc::SIGTERM => SIGTERM_RECEIVED.store(true, Ordering::SeqCst),
        libc::SIGINT => SIGINT_RECEIVED.store(true, Ordering::SeqCst),
        _ => {}
    }
}

/// Signal management for graceful shutdown.
///
/// Registers handlers for SIGTERM and SIGINT that set atomic flags.
/// The process can poll `SignalHandler::shutdown_requested()` to detect signals.
pub struct SignalHandler {
    _private: (),
}

impl SignalHandler {
    /// Install signal handlers for SIGTERM and SIGINT.
    ///
    /// Safe to call multiple times — subsequent calls are no-ops if handlers
    /// are already installed.
    pub fn install() -> Result<Self, PlatformError> {
        let handler = SigHandler::Handler(signal_handler);

        // Safety: signal_handler is async-signal-safe (only atomic stores)
        unsafe {
            signal::signal(NixSignal::SIGTERM, handler).map_err(|e| PlatformError::Signal {
                reason: format!("failed to install SIGTERM handler: {e}"),
            })?;

            signal::signal(NixSignal::SIGINT, handler).map_err(|e| PlatformError::Signal {
                reason: format!("failed to install SIGINT handler: {e}"),
            })?;
        }

        debug!("signal handlers installed for SIGTERM, SIGINT");
        Ok(Self { _private: () })
    }

    /// Check if a shutdown signal has been received.
    pub fn shutdown_requested() -> bool {
        SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
    }

    /// Check if SIGTERM was received.
    pub fn sigterm_received() -> bool {
        SIGTERM_RECEIVED.load(Ordering::SeqCst)
    }

    /// Check if SIGINT was received.
    pub fn sigint_received() -> bool {
        SIGINT_RECEIVED.load(Ordering::SeqCst)
    }

    /// Reset all signal flags (useful for tests).
    pub fn reset() {
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
        SIGTERM_RECEIVED.store(false, Ordering::SeqCst);
        SIGINT_RECEIVED.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_handlers() {
        SignalHandler::reset();
        let _handler = SignalHandler::install().unwrap();
        assert!(!SignalHandler::shutdown_requested());
    }

    #[test]
    fn manual_flag_set() {
        SignalHandler::reset();
        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
        assert!(SignalHandler::shutdown_requested());
        SignalHandler::reset();
        assert!(!SignalHandler::shutdown_requested());
    }
}
