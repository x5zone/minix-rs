//! VFS service IPC message types.
//!
//! Defines the messages exchanged between VFS and other services (PM, Kernel).

use crate::{EAGAIN, EINVAL, EIO, EMFILE, ENOSYS, ESRCH, Endpoint};

/// VFS request message types.
///
/// These are the requests that VFS receives from other services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsRequest {
    /// Fork request from PM.
    ///
    /// PM sends this to duplicate the parent's file descriptor table.
    Fork {
        /// Parent process endpoint.
        parent_endpoint: Endpoint,
        /// Child process endpoint.
        child_endpoint: Endpoint,
    },
}

/// VFS response message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsResponse {
    /// Fork succeeded.
    ForkOk,
    /// Operation failed.
    Error(VfsError),
}

/// VFS error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    /// Process table is full.
    ProcTableFull,
    /// Invalid endpoint.
    InvalidEndpoint,
    /// Slot is already in use.
    SlotInUse,
    /// Too many open files.
    TooManyOpenFiles,
    /// Internal error.
    InternalError,
    /// Operation not implemented.
    NotImplemented,
}

impl VfsError {
    /// Converts error to errno value.
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::ProcTableFull => EAGAIN,
            Self::InvalidEndpoint => ESRCH,
            Self::SlotInUse => EINVAL,
            Self::TooManyOpenFiles => EMFILE,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_fork_request() {
        let req = VfsRequest::Fork {
            parent_endpoint: Endpoint::PM,
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        };

        match req {
            VfsRequest::Fork {
                parent_endpoint,
                child_endpoint,
            } => {
                assert_eq!(parent_endpoint, Endpoint::PM);
            }
        }
    }

    #[test]
    fn test_vfs_response_fork_ok() {
        let resp = VfsResponse::ForkOk;
        assert!(matches!(resp, VfsResponse::ForkOk));
    }

    #[test]
    fn test_vfs_error_to_errno() {
        assert_eq!(VfsError::ProcTableFull.to_errno(), EAGAIN);
        assert_eq!(VfsError::TooManyOpenFiles.to_errno(), EMFILE);
        assert_eq!(VfsError::NotImplemented.to_errno(), ENOSYS);
    }
}
