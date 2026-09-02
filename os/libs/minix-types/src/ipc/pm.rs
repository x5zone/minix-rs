//! PM service IPC message types.
//!
//! Defines the messages exchanged between PM and other services (Kernel, VM, VFS).

use crate::{EAGAIN, EINVAL, EIO, ENOMEM, ENOSYS, EPERM, ESRCH, Endpoint};

/// PM request message types.
///
/// These are the requests that PM receives from other services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmRequest {
    /// Fork request from user process (via kernel).
    ///
    /// Kernel sends this when a process calls fork().
    Fork {
        /// Caller process endpoint.
        caller: Endpoint,
    },
}

/// PM response message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmResponse {
    /// Fork succeeded (returned to parent process).
    ForkParent {
        /// Child process PID.
        child_pid: i32,
    },
    /// Fork succeeded (returned to child process).
    ForkChild,
    /// Operation failed.
    Error(PmError),
}

/// PM error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmError {
    /// Process table is full.
    ProcTableFull,
    /// Out of memory.
    OutOfMemory,
    /// Invalid endpoint.
    InvalidEndpoint,
    /// Slot is already in use.
    SlotInUse,
    /// Permission denied (EPERM, e.g., non-RS srv_fork).
    PermissionDenied,
    /// Internal error.
    InternalError,
    /// Operation not implemented.
    NotImplemented,
}

impl PmError {
    /// Converts error to errno value.
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::ProcTableFull => EAGAIN,
            Self::OutOfMemory => ENOMEM,
            Self::InvalidEndpoint => ESRCH,
            Self::SlotInUse => EINVAL,
            Self::PermissionDenied => EPERM,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pm_fork_request() {
        let req = PmRequest::Fork {
            caller: Endpoint::PM,
        };

        match req {
            PmRequest::Fork { caller } => {
                assert_eq!(caller, Endpoint::PM);
            }
        }
    }

    #[test]
    fn test_pm_response_fork_parent() {
        let resp = PmResponse::ForkParent { child_pid: 100 };

        match resp {
            PmResponse::ForkParent { child_pid } => {
                assert_eq!(child_pid, 100);
            }
            _ => panic!("expected ForkParent"),
        }
    }

    #[test]
    fn test_pm_response_fork_child() {
        let resp = PmResponse::ForkChild;
        assert!(matches!(resp, PmResponse::ForkChild));
    }

    #[test]
    fn test_pm_error_to_errno() {
        assert_eq!(PmError::ProcTableFull.to_errno(), EAGAIN);
        assert_eq!(PmError::OutOfMemory.to_errno(), ENOMEM);
        assert_eq!(PmError::InvalidEndpoint.to_errno(), ESRCH);
        assert_eq!(PmError::NotImplemented.to_errno(), ENOSYS);
    }
}
