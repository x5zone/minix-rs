//! Kernel system call IPC message types.
//!
//! Defines the messages exchanged between Kernel and other services (PM, VM, VFS).

use crate::{EAGAIN, EINVAL, EIO, ENOSYS, ESRCH, Endpoint, UserSlot};

/// Kernel system call request types.
///
/// These are requests that Kernel receives from PM for process management.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelRequest {
    /// Fork request from PM.
    ///
    /// PM sends this to create the kernel process structure for the child.
    Fork {
        /// Parent process endpoint.
        parent_endpoint: Endpoint,
        /// Child process endpoint.
        child_endpoint: Endpoint,
        /// Child process slot.
        child_slot: UserSlot,
    },
}

/// Kernel system call response types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelResponse {
    /// Fork succeeded.
    ForkOk,
    /// Operation failed.
    Error(KernelError),
}

/// Kernel error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelError {
    /// Process table is full.
    ProcTableFull,
    /// Invalid endpoint.
    InvalidEndpoint,
    /// Slot is already in use.
    SlotInUse,
    /// Internal error.
    InternalError,
    /// Operation not implemented.
    NotImplemented,
}

impl KernelError {
    /// Converts error to errno value.
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::ProcTableFull => EAGAIN,
            Self::InvalidEndpoint => ESRCH,
            Self::SlotInUse => EINVAL,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_fork_request() {
        let req = KernelRequest::Fork {
            parent_endpoint: Endpoint::PM,
            child_endpoint: Endpoint::from_generation_slot(1, 1),
            child_slot: UserSlot::new(1),
        };

        match req {
            KernelRequest::Fork {
                parent_endpoint,
                child_endpoint,
                child_slot,
            } => {
                assert_eq!(parent_endpoint, Endpoint::PM);
                assert_eq!(child_slot.get(), 1);
            }
        }
    }

    #[test]
    fn test_kernel_response_fork_ok() {
        let resp = KernelResponse::ForkOk;
        assert!(matches!(resp, KernelResponse::ForkOk));
    }

    #[test]
    fn test_kernel_error_to_errno() {
        assert_eq!(KernelError::ProcTableFull.to_errno(), EAGAIN);
        assert_eq!(KernelError::InvalidEndpoint.to_errno(), ESRCH);
        assert_eq!(KernelError::NotImplemented.to_errno(), ENOSYS);
    }
}
