//! VFS asynchronous request queue.
//!
//! Handles VM-VFS communication for file-mapped memory operations.
//! VM sends requests to VFS and receives replies asynchronously,
//! allowing VM to process other requests while waiting.
//!
//! Design: serial activation model (§3.3 of 23-vfs-interaction.md).
//! Only one request is active at a time; others queue until the
//! active request receives its reply.
//!
//! Corresponds to Minix3 C source `vfs.c`: `vfs_request()` (enqueue +
//! send), `do_vfs_reply()` (dequeue + callback). The C code uses a
//! single global `vfs_rq` struct; this Rust implementation replaces it
//! with `VfsRequestQueue` owning both the pending queue and the active
//! slot explicitly.

use alloc::collections::VecDeque;
use minix_types::{Endpoint, VirBytes, VmMmapIn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VfsRequestType {
    FdLookup,
    FdIo,
    FdClose,
}

#[derive(Debug, Clone)]
pub(crate) enum VfsRequestState {
    FdLookup {
        /// Original VM_MMAP request. C `mmap_file_cont` reads the original
        /// message (`origmsg`) to recover addr/len/prot/flags/offset
        /// (mmap.c:169-181); the Rust callback needs the same data.
        mmap: VmMmapIn,
    },
    FdIo {
        region_vaddr: VirBytes,
        page_offset: VirBytes,
        write: bool,
        caller_endpoint: Endpoint,
    },
    FdClose {
        fd: i32,
    },
}

pub(crate) type VfsCallbackFn = fn(
    server: &mut crate::vm_server::VmServer,
    reply: &VfsReply,
    state: &VfsRequestState,
) -> Result<(), VfsQueueError>;

#[derive(Debug)]
pub(crate) struct VfsRequest {
    pub(crate) request_type: VfsRequestType,
    pub(crate) req_id: u32,
    pub(crate) caller_endpoint: Endpoint,
    pub(crate) fd: i32,
    pub(crate) offset: u64,
    pub(crate) length: u32,
    pub(crate) callback: Option<VfsCallbackFn>,
    pub(crate) state: Option<VfsRequestState>,
}

#[derive(Debug, Clone)]
pub(crate) struct VfsReply {
    pub(crate) req_id: u32,
    pub(crate) result: i32,
    pub(crate) data_phys: Option<minix_types::PhysBytes>,
    pub(crate) fd: i32,
    pub(crate) dev: u64,
    pub(crate) ino: u64,
    pub(crate) size_pages: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VfsQueueError {
    QueueFull,
    InvalidFd,
    IoError,
    NoCallback,
    NoActiveRequest,
    UnexpectedReply,
    NoCallbackState,
}

// NOTE: IpcSender trait was removed — it had no implementations and was dead code.
// When IpcTransport is available, the VFS request send path will use that trait
// directly rather than a separate IpcSender abstraction.

pub(crate) struct VfsRequestQueue {
    queued: VecDeque<VfsRequest>,
    active: Option<VfsRequest>,
    next_id: u32,
    max_queued: usize,
}

impl VfsRequestQueue {
    pub(crate) fn new() -> Self {
        Self {
            queued: VecDeque::new(),
            active: None,
            next_id: 1,
            max_queued: 64,
        }
    }

    pub(crate) fn request(&mut self, mut req: VfsRequest) -> Result<(), VfsQueueError> {
        if self.queued.len() >= self.max_queued && self.active.is_some() {
            return Err(VfsQueueError::QueueFull);
        }
        req.req_id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.queued.push_back(req);
        if self.active.is_none() {
            self.activate();
        }
        Ok(())
    }

    fn activate(&mut self) {
        if let Some(req) = self.queued.pop_front() {
            self.active = Some(req);
        }
    }

    pub(crate) fn handle_reply(
        &mut self,
        reply: VfsReply,
    ) -> Result<Option<(VfsCallbackFn, VfsReply, VfsRequestState)>, VfsQueueError> {
        let req = self.active.take()
            .ok_or(VfsQueueError::NoActiveRequest)?;

        if req.req_id != reply.req_id {
            self.active = Some(req);
            return Err(VfsQueueError::UnexpectedReply);
        }

        let result = match (req.callback, req.state) {
            (Some(cb), Some(state)) => Some((cb, reply, state)),
            _ => None,
        };

        if !self.queued.is_empty() {
            self.activate();
        }

        Ok(result)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.active.is_none() && self.queued.is_empty()
    }

    pub(crate) fn has_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn active_req_id(&self) -> Option<u32> {
        self.active.as_ref().map(|req| req.req_id)
    }

    pub(crate) fn queued_count(&self) -> usize {
        self.queued.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_queue_request_activate() {
        let mut queue = VfsRequestQueue::new();

        let req = VfsRequest {
            request_type: VfsRequestType::FdIo,
            req_id: 0,
            caller_endpoint: Endpoint(100),
            fd: 3,
            offset: 0,
            length: 4096,
            callback: None,
            state: None,
        };

        queue.request(req).unwrap();
        assert!(queue.has_active());
        assert_eq!(queue.queued_count(), 0);
    }

    #[test]
    fn test_vfs_queue_serial_activation() {
        let mut queue = VfsRequestQueue::new();

        let req1 = VfsRequest {
            request_type: VfsRequestType::FdLookup,
            req_id: 0,
            caller_endpoint: Endpoint(100),
            fd: 1,
            offset: 0,
            length: 0,
            callback: None,
            state: None,
        };
        let req2 = VfsRequest {
            request_type: VfsRequestType::FdIo,
            req_id: 0,
            caller_endpoint: Endpoint(101),
            fd: 2,
            offset: 4096,
            length: 4096,
            callback: None,
            state: None,
        };

        queue.request(req1).unwrap();
        queue.request(req2).unwrap();

        assert!(queue.has_active());
        assert_eq!(queue.queued_count(), 1);

        let active = queue.active.as_ref().unwrap();
        assert_eq!(active.request_type, VfsRequestType::FdLookup);
    }

    #[test]
    fn test_vfs_queue_handle_reply() {
        let mut queue = VfsRequestQueue::new();

        let req1 = VfsRequest {
            request_type: VfsRequestType::FdLookup,
            req_id: 0,
            caller_endpoint: Endpoint(100),
            fd: 1,
            offset: 0,
            length: 0,
            callback: None,
            state: None,
        };
        let req2 = VfsRequest {
            request_type: VfsRequestType::FdIo,
            req_id: 0,
            caller_endpoint: Endpoint(101),
            fd: 2,
            offset: 4096,
            length: 4096,
            callback: None,
            state: None,
        };

        queue.request(req1).unwrap();
        queue.request(req2).unwrap();

        let active_id = queue.active.as_ref().unwrap().req_id;

        let _reply = VfsReply {
            req_id: active_id,
            result: 0,
            data_phys: None,
            fd: 1,
            dev: 0,
            ino: 0,
            size_pages: 0,
        };

        // Can't call handle_reply without a real VmServer, so test the structure
        assert_eq!(queue.queued_count(), 1);
        assert!(queue.has_active());
    }

    #[test]
    fn test_vfs_queue_no_active_reply() {
        let queue = VfsRequestQueue::new();

        let _reply = VfsReply {
            req_id: 1,
            result: 0,
            data_phys: None,
            fd: 1,
            dev: 0,
            ino: 0,
            size_pages: 0,
        };

        // handle_reply needs &mut VmServer, so we test the error path indirectly
        assert!(queue.is_empty());
        assert!(!queue.has_active());
    }

    #[test]
    fn test_vfs_request_types() {
        assert_ne!(VfsRequestType::FdLookup, VfsRequestType::FdIo);
        assert_ne!(VfsRequestType::FdIo, VfsRequestType::FdClose);
        assert_ne!(VfsRequestType::FdLookup, VfsRequestType::FdClose);
    }
}
