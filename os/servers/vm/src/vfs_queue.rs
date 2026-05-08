//! VFS asynchronous request queue.
//!
//! Handles VM-VFS communication for file-mapped memory operations.
//! VM sends requests to VFS and receives replies asynchronously,
//! allowing VM to process other requests while waiting.
//!
//! Corresponds to Minix3's `first_queued` + VFS callback mechanism.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use minix_types::{Endpoint, VirBytes};
use crate::vmproc::ActiveProc;

pub(crate) type VfsCallback = fn(&mut VfsReply, &mut crate::vm_server::VmServer) -> Result<(), VfsQueueError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VfsRequestType {
    ReadPage,
    WritePage,
    SyncPage,
}

#[derive(Debug)]
pub(crate) struct VfsRequest {
    pub request_type: VfsRequestType,
    pub caller_endpoint: Endpoint,
    pub fd: i32,
    pub offset: u64,
    pub length: usize,
    pub callback: Option<VfsCallback>,
}

#[derive(Debug)]
pub(crate) struct VfsReply {
    pub request_type: VfsRequestType,
    pub caller_endpoint: Endpoint,
    pub result: i32,
    pub data_phys: Option<minix_types::PhysBytes>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VfsQueueError {
    QueueFull,
    InvalidFd,
    IoError,
    NoCallback,
}

pub(crate) struct VfsRequestQueue {
    pending: VecDeque<VfsRequest>,
    max_pending: usize,
}

impl VfsRequestQueue {
    pub(crate) fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            max_pending: 64,
        }
    }

    pub(crate) fn with_capacity(max_pending: usize) -> Self {
        Self {
            pending: VecDeque::with_capacity(max_pending),
            max_pending,
        }
    }

    pub(crate) fn enqueue(&mut self, request: VfsRequest) -> Result<(), VfsQueueError> {
        if self.pending.len() >= self.max_pending {
            return Err(VfsQueueError::QueueFull);
        }
        self.pending.push_back(request);
        Ok(())
    }

    pub(crate) fn dequeue(&mut self) -> Option<VfsRequest> {
        self.pending.pop_front()
    }

    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(crate) fn find_by_caller(&self, endpoint: Endpoint) -> Option<&VfsRequest> {
        self.pending.iter().find(|r| r.caller_endpoint == endpoint)
    }

    pub(crate) fn remove_by_caller(&mut self, endpoint: Endpoint) -> Option<VfsRequest> {
        if let Some(pos) = self.pending.iter().position(|r| r.caller_endpoint == endpoint) {
            self.pending.remove(pos)
        } else {
            None
        }
    }

    pub(crate) fn handle_reply(
        &mut self,
        reply: VfsReply,
        server: &mut crate::vm_server::VmServer,
    ) -> Result<(), VfsQueueError> {
        let request = self.remove_by_caller(reply.caller_endpoint)
            .ok_or(VfsQueueError::NoCallback)?;

        if let Some(callback) = request.callback {
            callback(&mut reply.clone(), server)?;
        }

        Ok(())
    }
}

impl Clone for VfsReply {
    fn clone(&self) -> Self {
        Self {
            request_type: self.request_type,
            caller_endpoint: self.caller_endpoint,
            result: self.result,
            data_phys: self.data_phys,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_queue_enqueue_dequeue() {
        let mut queue = VfsRequestQueue::new();

        let req = VfsRequest {
            request_type: VfsRequestType::ReadPage,
            caller_endpoint: Endpoint::PM,
            fd: 3,
            offset: 0,
            length: 4096,
            callback: None,
        };

        queue.enqueue(req).unwrap();
        assert_eq!(queue.pending_count(), 1);

        let dequeued = queue.dequeue();
        assert!(dequeued.is_some());
        assert_eq!(dequeued.unwrap().fd, 3);
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn test_vfs_queue_full() {
        let mut queue = VfsRequestQueue::with_capacity(2);

        let req1 = VfsRequest {
            request_type: VfsRequestType::ReadPage,
            caller_endpoint: Endpoint(100),
            fd: 1,
            offset: 0,
            length: 4096,
            callback: None,
        };
        let req2 = VfsRequest {
            request_type: VfsRequestType::ReadPage,
            caller_endpoint: Endpoint(101),
            fd: 2,
            offset: 0,
            length: 4096,
            callback: None,
        };
        let req3 = VfsRequest {
            request_type: VfsRequestType::ReadPage,
            caller_endpoint: Endpoint(102),
            fd: 3,
            offset: 0,
            length: 4096,
            callback: None,
        };

        assert!(queue.enqueue(req1).is_ok());
        assert!(queue.enqueue(req2).is_ok());
        assert_eq!(queue.enqueue(req3), Err(VfsQueueError::QueueFull));
    }

    #[test]
    fn test_vfs_queue_find_by_caller() {
        let mut queue = VfsRequestQueue::new();

        let req = VfsRequest {
            request_type: VfsRequestType::ReadPage,
            caller_endpoint: Endpoint(50),
            fd: 5,
            offset: 4096,
            length: 4096,
            callback: None,
        };

        queue.enqueue(req).unwrap();

        let found = queue.find_by_caller(Endpoint(50));
        assert!(found.is_some());
        assert_eq!(found.unwrap().fd, 5);

        assert!(queue.find_by_caller(Endpoint(99)).is_none());
    }

    #[test]
    fn test_vfs_queue_remove_by_caller() {
        let mut queue = VfsRequestQueue::new();

        let req1 = VfsRequest {
            request_type: VfsRequestType::ReadPage,
            caller_endpoint: Endpoint(50),
            fd: 5,
            offset: 0,
            length: 4096,
            callback: None,
        };
        let req2 = VfsRequest {
            request_type: VfsRequestType::WritePage,
            caller_endpoint: Endpoint(51),
            fd: 6,
            offset: 0,
            length: 4096,
            callback: None,
        };

        queue.enqueue(req1).unwrap();
        queue.enqueue(req2).unwrap();
        assert_eq!(queue.pending_count(), 2);

        let removed = queue.remove_by_caller(Endpoint(50));
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().fd, 5);
        assert_eq!(queue.pending_count(), 1);
    }

    #[test]
    fn test_vfs_request_types() {
        let types = [
            VfsRequestType::ReadPage,
            VfsRequestType::WritePage,
            VfsRequestType::SyncPage,
        ];
        assert_eq!(types.len(), 3);
        assert_ne!(VfsRequestType::ReadPage, VfsRequestType::WritePage);
    }
}
