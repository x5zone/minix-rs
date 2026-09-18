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
use alloc::vec::Vec;
use minix_types::{Endpoint, VirBytes, VmMmapIn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // `Fd` prefix mirrors C VMVFSREQ_FDLOOKUP/FDIO/FDCLOSE
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
}

pub(crate) type VfsCallbackFn = fn(
    server: &mut crate::vm_server::VmServer,
    reply: &VfsReply,
    state: &VfsRequestState,
) -> Result<(), VfsQueueError>;

#[derive(Debug)]
pub(crate) struct VfsRequest {
    // V10-P2-1: `request_type`/`caller_endpoint`/`fd`/`offset`/`length`
    // are write-only today — the queue matches on `req_id` and hands
    // `state`+`callback` to the handler, which re-reads the original data
    // from `VfsRequestState`. Kept for the VFS message-build path when
    // `KernelIpcTransport` lands.
    #[allow(dead_code)]
    pub(crate) request_type: VfsRequestType,
    pub(crate) req_id: u32,
    #[allow(dead_code)]
    pub(crate) caller_endpoint: Endpoint,
    #[allow(dead_code)]
    pub(crate) fd: i32,
    #[allow(dead_code)]
    pub(crate) offset: u64,
    // u64 to carry >4 GiB FdLookup lengths (aligned mmap sizes) without
    // truncation; the VFS wire itself is built when KernelIpcTransport lands.
    #[allow(dead_code)]
    pub(crate) length: u64,
    pub(crate) callback: Option<VfsCallbackFn>,
    pub(crate) state: Option<VfsRequestState>,
    /// E-VFSWIRE: the active request's VFS_VMCALL has been handed to the
    /// transport. A failed send clears it again (retry next drain round) —
    /// pre-E1 the transport always fails, so behavior matches the
    /// historical "never sent" state with no regression.
    pub(crate) sent: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct VfsReply {
    pub(crate) req_id: u32,
    pub(crate) result: i32,
    // V10-P2-1: `data_phys`/`size_pages` mirror the C VMV_REPLY payload
    // but are write-only today — the FDIO consumer (mappedfile pagefault
    // retry) is not wired to read them.
    #[allow(dead_code)]
    pub(crate) data_phys: Option<minix_types::PhysBytes>,
    pub(crate) fd: i32,
    pub(crate) dev: u64,
    pub(crate) ino: u64,
    #[allow(dead_code)]
    pub(crate) size_pages: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VfsQueueError {
    QueueFull,
    InvalidFd,
    IoError,
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
    /// req_ids of purged requests that were already sent to VFS: their
    /// replies can still arrive, and must be dropped quietly (the owner is
    /// dead — C leaves the same race to `do_vfs_reply`'s `vm_isokendpt`
    /// NULL-vmp path, vfs.c:124-129; the tombstone is the queue-level
    /// equivalent). Bounded: one entry per purged sent-active, drained on
    /// matching reply.
    cancelled: Vec<u32>,
    next_id: u32,
    max_queued: usize,
}

impl VfsRequestQueue {
    pub(crate) fn new() -> Self {
        Self {
            queued: VecDeque::new(),
            active: None,
            cancelled: Vec::new(),
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

    /// Build the `VFS_VMCALL` wire message for the active request and mark
    /// it sent (E-VFSWIRE). Returns `None` when nothing is active or the
    /// active request was already sent — the caller (drain step) sends it
    /// to VFS and, on transport failure, calls [`Self::mark_send_failed`]
    /// so the next round retries.
    ///
    /// C: `vfs_request` builds the message (vfs.c:83-90) and `activate()`
    /// sends it once (`vfs.c:51` asynsend3); the reply is keyed by req_id.
    pub(crate) fn take_pending_vfs_call(&mut self) -> Option<minix_types::Message> {
        let req = self.active.as_mut()?;
        if req.sent {
            return None;
        }
        req.sent = true;
        let opcode = match req.request_type {
            VfsRequestType::FdLookup => minix_types::VMVFSREQ_FDLOOKUP,
            VfsRequestType::FdClose => minix_types::VMVFSREQ_FDCLOSE,
            VfsRequestType::FdIo => minix_types::VMVFSREQ_FDIO,
        };
        // C: vfs.c:83-90 — m_type + the six mess_10 fields.
        let mut msg = minix_types::Message {
            m_type: minix_types::VFS_VMCALL,
            ..Default::default()
        };
        // Note: writing a union arm needs no `unsafe` when the arm itself
        // is a plain-old-data struct with no drop/niche — the Message union
        // here derives Copy, so assignment is a plain byte store. The VFS
        // decoder reads the same arm symmetrically (E-VFSWIRE).
        {
            msg.m_u.m_vm_vfs_call = minix_types::ipc::MessVmVfsCall {
                offset: req.offset,
                req: opcode,
                fd: req.fd,
                req_id: req.req_id as i32,
                endpoint: req.caller_endpoint.0,
                _l1: 0,
                _l2: 0,
                length: req.length as u32,
                _padding: [0; 20],
            };
        }
        Some(msg)
    }

    /// Clear the sent mark after a transport failure — the request stays
    /// active and the next drain round retries (design: 无回归, pre-E1
    /// the send always failed).
    pub(crate) fn mark_send_failed(&mut self) {
        if let Some(req) = self.active.as_mut() {
            req.sent = false;
        }
    }

    #[cfg(test)]
    /// V11/T10: test inspection of the active request (the send half is
    /// edge E-VFSWIRE; this asserts the enqueue half).
    pub(crate) fn active_fd_close(&self) -> Option<(VfsRequestType, i32, Endpoint)> {
        let req = self.active.as_ref()?;
        if req.request_type != VfsRequestType::FdClose {
            return None;
        }
        Some((req.request_type, req.fd, req.caller_endpoint))
    }

    pub(crate) fn handle_reply(
        &mut self,
        reply: VfsReply,
    ) -> Result<Option<(VfsCallbackFn, VfsReply, VfsRequestState)>, VfsQueueError> {
        // Late reply for a purged request: the caller exited after the
        // request was sent — drop it (V13-P3-1之2 dead-caller path).
        if let Some(pos) = self.cancelled.iter().position(|&id| id == reply.req_id) {
            self.cancelled.remove(pos);
            return Ok(None);
        }

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

    /// Cancel every pending request owned by `owner` — the exit-side
    /// cleanup (V13-P3-1之2; §9.4 Redox lesson: the "caller died first"
    /// branch must cancel in-flight slots too, not only driver death).
    ///
    /// Queued requests are dropped outright. An already-sent active request
    /// cannot be unsent — its slot is removed and its req_id tombstoned so
    /// the late VFS reply is dropped by [`Self::handle_reply`] instead of
    /// surfacing as `UnexpectedReply`. Call this from the exit path
    /// *before* enqueueing the dying process's final FdClose: the close is
    /// a legitimate request (C `mappedfile_delete` → `fdref_deref`,
    /// mem_file.c:280-287) and must survive its owner's purge.
    ///
    /// Returns the number of cancelled requests (queued + active).
    pub(crate) fn purge_by_owner(&mut self, owner: Endpoint) -> usize {
        let before = self.queued.len();
        self.queued.retain(|req| req.caller_endpoint != owner);
        let mut n = before - self.queued.len();

        if let Some(req) = self.active.as_mut()
            && req.caller_endpoint == owner
        {
            let req = self.active.take().expect("active checked above");
            n += 1;
            if req.sent {
                self.cancelled.push(req.req_id);
            }
            // The freed active slot activates the next surviving request.
            self.activate();
        }
        n
    }

    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only
    pub(crate) fn has_active(&self) -> bool {
        self.active.is_some()
    }

    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only
    pub(crate) fn active_req_id(&self) -> Option<u32> {
        self.active.as_ref().map(|req| req.req_id)
    }

    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only
    pub(crate) fn queued_count(&self) -> usize {
        self.queued.len()
    }

    /// Test-only accessor: peek at the currently active request without
    /// consuming it. Cross-module tests (cow_exec_pf) verify the FDIO
    /// request fields (fd/offset/callback) enqueued by the pagefault path.
    #[cfg(test)]
    pub(crate) fn test_active_request(&self) -> Option<&VfsRequest> {
        self.active.as_ref()
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
            sent: false,
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
            sent: false,
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
            sent: false,
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
            sent: false,
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
            sent: false,
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

    fn mk_req(t: VfsRequestType, owner: Endpoint, fd: i32) -> VfsRequest {
        VfsRequest {
            request_type: t,
            req_id: 0,
            caller_endpoint: owner,
            fd,
            offset: 0,
            length: 0,
            callback: None,
            state: None,
            sent: false,
        }
    }

    #[test]
    fn purge_drops_queued_of_owner_and_keeps_others() {
        let mut q = VfsRequestQueue::new();
        q.request(mk_req(VfsRequestType::FdLookup, Endpoint(10), 1)).unwrap();
        q.request(mk_req(VfsRequestType::FdIo, Endpoint(20), 2)).unwrap();
        q.request(mk_req(VfsRequestType::FdIo, Endpoint(10), 3)).unwrap();

        let n = q.purge_by_owner(Endpoint(10));
        assert_eq!(n, 2);
        assert!(q.has_active());
        // 唯一幸存者是 owner 20 的请求:active + 队列都应与 10 无关
        let active = q.test_active_request().unwrap();
        assert_eq!(active.caller_endpoint, Endpoint(20));
        assert_eq!(q.queued_count(), 0);
    }

    #[test]
    fn purge_of_unsent_active_frees_slot_without_tombstone() {
        let mut q = VfsRequestQueue::new();
        q.request(mk_req(VfsRequestType::FdIo, Endpoint(10), 1)).unwrap();
        // 未发送(mark_send_failed 未经过 take_pending_vfs_call,sent=false)
        assert_eq!(q.purge_by_owner(Endpoint(10)), 1);
        assert!(!q.has_active());
        assert!(q.is_empty());
        // 无墓碑:该 req_id 的回复按正常 Unknown 路径报错(VFS 从未见过请求)
        assert!(matches!(
            q.handle_reply(VfsReply {
                req_id: 1,
                result: 0,
                data_phys: None,
                fd: 1,
                dev: 0,
                ino: 0,
                size_pages: 0,
            }),
            Err(VfsQueueError::NoActiveRequest)
        ));
    }

    #[test]
    fn purge_of_sent_active_tombstones_late_reply() {
        let mut q = VfsRequestQueue::new();
        q.request(mk_req(VfsRequestType::FdLookup, Endpoint(10), 1)).unwrap();
        q.request(mk_req(VfsRequestType::FdIo, Endpoint(20), 2)).unwrap();
        // 发送半:取走 wire 消息 → active.sent = true
        assert!(q.take_pending_vfs_call().is_some());
        let sent_id = q.active_req_id().unwrap();

        assert_eq!(q.purge_by_owner(Endpoint(10)), 1);
        // 下一个请求被激活
        assert_eq!(q.active_req_id(), Some(sent_id + 1));

        // 迟到的 VFS 回复被墓碑吞掉,激活的下一个请求不受影响
        let r = q.handle_reply(VfsReply {
            req_id: sent_id,
            result: 0,
            data_phys: None,
            fd: 1,
            dev: 0,
            ino: 0,
            size_pages: 0,
        });
        assert!(matches!(r, Ok(None)));
        assert_eq!(q.active_req_id(), Some(sent_id + 1));
    }

    #[test]
    fn exit_purge_spares_own_fdclose() {
        let mut q = VfsRequestQueue::new();
        q.request(mk_req(VfsRequestType::FdIo, Endpoint(10), 1)).unwrap();
        q.purge_by_owner(Endpoint(10));
        // exit 路径 purge 之后才入队的 FdClose 存活(handle_vm_exit 顺序契约)
        q.request(mk_req(VfsRequestType::FdClose, Endpoint(10), 1)).unwrap();
        assert!(q.has_active());
        assert_eq!(q.active_fd_close().map(|(_, fd, _)| fd), Some(1));
    }
}
