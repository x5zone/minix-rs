//! MIB server assembly: the loop, the seams, the three arms.
//!
//! Mirrors `main()` (`minix3/minix/servers/mib/main.c:433-492`) and the
//! register/deregister glue (`remote.c:197-307`). The sysctl arm
//! decodes through 01's six gates and delegates to the walker (10);
//! register/deregister run the remote slot table with a mount that
//! reserves the target window and stamps the covering node (12).
//!
//! Two seams, SCHED/DS house shape: [`MibIpc`] (messages with peers)
//! plus the kernel/service verb traits from
//! [`transport`](crate::transport). A real end delegates to
//! `minix-sys`'s `IpcTransport` — the shape is committed, the power
//! arrives with the trap layer (edge E1).
//!
//! 01-mib-init-main.md (loop); 12-mib-remote-subtrees.md (arms).

use alloc::vec;
use alloc::vec::Vec;
use minix_sys::ipc::{CALL_SENDREC, STATUS_CALL_MASK};
use minix_types::{
    CTLFLAG_PERMANENT, CTLTYPE_NODE, EDONTREPLY, EINVAL, Endpoint, Message, MessLcMibSysctl,
    MessLsysMibRegister, OK,
};

use crate::auth::CallAuth;
use crate::dispatch::{
    Incoming, MibCall, check_namelen, classify_name, default_outcome, map_sysctl_reply, pair_newp,
    pair_oldp, should_reply, triage,
};
use crate::heap::MibBudget;
use crate::io::copy::{Newp, Oldp};
use crate::remote::MIB_ENDPTS;
use crate::transport::{MibKernel, MibServices};
use crate::tree::arena::{MibTree, NodeId};
use crate::tree::flag::CTLFLAG_REMOTE;
use crate::tree::mount::{check_head, check_target, head_code};
use crate::remote::{EndptSlot, SlotVerdict};
use crate::walker::{self, MibCtx, Request};

// The real end drives minix-sys's transport trait methods.
use minix_sys::ipc::IpcTransport as _;

/// Kernel-flagged status: a notification from the kernel. C:
/// `NOTIFY_MESSAGE` — com.h:92.
pub const NOTIFY_MESSAGE: u32 = 0x1000;

/// C: `is_ipc_notify(status)` — com.h:93: the status call field
/// (`IPC_STATUS_CALL`, low `0x3F` bits — ipcconst.h:21-22) equals
/// `NOTIFY` (4 — ipcconst.h:10). The status word is authoritative: MIB
/// reads with `sef_receive_status`, no call-number guessing. (The
/// previous form subtracted `NOTIFY_MESSAGE` and tested a 0x100 band —
/// that is com.h:94 `is_notify`, the m_type-band macro, not the status
/// macro main.c:449 calls.)
pub const fn status_is_notify(status: u32) -> bool {
    (status & 0x3F) == minix_sys::ipc::CALL_NOTIFY
}

/// C: `IPC_STATUS_CALL(status) == SENDREC` — the caller is blocked in
/// `sendrec` and is owed a reply.
pub const fn status_is_sendrec(status: u32) -> bool {
    (status & STATUS_CALL_MASK) == CALL_SENDREC
}

/// The message seam (C: `sef_receive_status`/`ipc_sendnb`/`ipc_sendrec`).
pub trait MibIpc {
    /// Receive the next message with its status word. The status is
    /// what makes MIB's notify test exact (`is_ipc_notify(ipc_status)`
    /// reads the status, no call-number guessing — main.c:449) and
    /// what tells a SENDREC caller (owed a reply) from a one-way send.
    fn receive_status(&mut self, message: &mut Message) -> Result<u32, i32>;
    /// Non-blocking reply send. C: `ipc_sendnb` — main.c:484-486.
    fn send_nb(&mut self, to: Endpoint, message: &Message) -> Result<(), i32>;
    /// Send-and-receive against a peer service (remote relay, 12).
    /// C: `ipc_sendrec` — remote.c:422-436.
    fn send_rec(&mut self, peer: Endpoint, message: &mut Message) -> Result<(), i32>;
    /// Notification (the SEF ping pong). C: `ipc_notify` — sef_ping.c:61.
    fn notify(&mut self, to: Endpoint) -> Result<(), i32>;
}

/// Receive-failure bound (DS's honest-death discipline: a transient
/// transport error loses a turn, a broken seam dies at 64).
pub const MAX_CONSECUTIVE_RECV_FAILURES: u32 = 64;

/// The MIB server: tree, budget, and the remote slot table.
pub struct MibServer {
    /// The sysctl tree.
    pub tree: MibTree,
    /// The A-3 byte budget.
    pub budget: MibBudget,
    /// Remote subtree slots. C: `endpts[32]` — remote.c:24-35.
    pub slots: Vec<EndptSlot>,
}

impl Default for MibServer {
    fn default() -> Self {
        Self::new()
    }
}

impl MibServer {
    /// An empty server: the static tree built, every remote slot free.
    pub fn new() -> Self {
        Self {
            tree: MibTree::init(),
            budget: MibBudget::new(),
            slots: (0..MIB_ENDPTS).map(|_| EndptSlot::default()).collect(),
        }
    }
}

/// The assembled server: state plus the three seams.
pub struct Server<K: MibKernel, S: MibServices, I: MibIpc> {
    /// Server state (tree/budget/slots).
    pub server: MibServer,
    /// Message seam.
    pub ipc: I,
    /// Kernel verbs.
    pub kernel: K,
    /// Peer-service verbs.
    pub services: S,
}

/// What one `run_once` turn did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// A message was handled (replied or deliberately silenced).
    Handled,
    /// The receive failed — the loop's failure counter ticks.
    ReceiveFailed,
}

impl<K: MibKernel, S: MibServices, I: MibIpc + minix_sef::SefIpc> Server<K, S, I> {
    /// Assemble from parts.
    pub fn new(server: MibServer, ipc: I, kernel: K, services: S) -> Self {
        Self { server, ipc, kernel, services }
    }

    /// Receive one message and handle it.
    ///
    /// C: main.c:443-487 — triage, dispatch, reply. Returns what the
    /// turn did plus the triage verdict for tests.
    pub fn run_once(&mut self) -> (Turn, Incoming) {
        let mut msg = Message::default();
        // E-ISWIRE: the receive goes through the SEF library — RS pings
        // are ponged (notify) and swallowed inside `sef_receive_status`,
        // never reaching the triage below. C: mib main.c:443 reads with
        // `sef_receive_status`; SYSTEM notifies surface as SefEvent::Signal
        // (MIB registers no signal handler — ignored, same as C's
        // `is_ipc_notify` refusal at main.c:449-454).
        let status = match minix_sef::sef_receive_status(
            &mut self.ipc,
            Endpoint::ANY,
            &mut msg,
            &mut |_sig| {},
        ) {
            Ok(rx) => rx.status as u32,
            Err(_) => return (Turn::ReceiveFailed, Incoming::Unknown),
        };
        let incoming = triage(status_is_notify(status), msg.m_type);
        let is_sendrec = (status & STATUS_CALL_MASK) == minix_sys::ipc::CALL_SENDREC;
        let mut out = Message::default();
        match incoming {
            Incoming::NotifyRefusal => {
                // Log and take the next letter (:449-454). No reply: a
                // notification has no sender waiting.
                return (Turn::Handled, incoming);
            }
            Incoming::Unknown => {
                // Blocking callers hear ENOSYS; one-way sends, silence
                // (:474-479).
                out.m_type = default_outcome(is_sendrec);
            }
            Incoming::Dispatch(MibCall::Sysctl) => {
                let (code, reply) = self.sysctl_arm(&mut msg);
                out = reply;
                out.m_type = code;
            }
            Incoming::Dispatch(MibCall::Register) => {
                let r = self.register_arm(&mut msg, is_sendrec);
                if !should_reply(r) {
                    // EDONTREPLY: one-way protocol or silent drop.
                    return (Turn::Handled, incoming);
                }
                out.m_type = r;
            }
            Incoming::Dispatch(MibCall::Deregister) => {
                let r = self.deregister_arm(&mut msg, is_sendrec);
                if !should_reply(r) {
                    return (Turn::Handled, incoming);
                }
                out.m_type = r;
            }
        }
        // The reply rule (:482-487): every outcome except the silence
        // sentinel goes back; send failures are logged, never fatal.
        if should_reply(out.m_type) {
            let to = msg.m_source;
            let _ = self.ipc.send_nb(to, &out);
        }
        (Turn::Handled, incoming)
    }

    /// The sysctl arm: decode (six gates), fetch a long name if needed,
    /// run the walker, shape the reply. C: `mib_sysctl` — main.c:277-383.
    fn sysctl_arm(&mut self, msg: &mut Message) -> (i32, Message) {
        let mut out = Message::default();
        let caller = msg.m_source;
        let wire: MessLcMibSysctl = unsafe { msg.m_u.m_lc_mib_sysctl };
        // Gate 1: name length bounds (:302-303).
        let namelen = match check_namelen(wire.namelen) {
            Ok(n) => n,
            Err(code) => return (code, out),
        };
        // Gate 2: the name's road — inline lanes or one kernel copy
        // (:309-315; the copy is 06's transport verb).
        let mut name_vec: Vec<i32> = Vec::with_capacity(namelen as usize);
        match classify_name(namelen) {
            crate::dispatch::NamePath::Inline => {
                name_vec.extend_from_slice(&wire.name[..namelen as usize]);
            }
            crate::dispatch::NamePath::Copy => {
                let mut bytes = vec![0u8; namelen as usize * 4];
                if let Err(code) =
                    self.kernel.datacopy_from(caller, wire.namep as u64, &mut bytes)
                {
                    return (code, out);
                }
                for chunk in bytes.chunks_exact(4) {
                    name_vec.push(i32::from_le_bytes(chunk.try_into().unwrap()));
                }
            }
        }
        // Gates 3/4: pair the old/new arguments (:322-340).
        let oldp = match pair_oldp(wire.oldp as u64, wire.oldlen as u64) {
            crate::dispatch::OldpPresence::Present => Some(Oldp {
                endpt: caller,
                addr: wire.oldp as u64,
                left: wire.oldlen as u64,
            }),
            crate::dispatch::OldpPresence::Absent => None,
        };
        let newp = match pair_newp(wire.newp as u64, wire.newlen as u64) {
            crate::dispatch::NewpPresence::Present => Some(Newp {
                endpt: caller,
                addr: wire.newp as u64,
                len: wire.newlen as u64,
            }),
            crate::dispatch::NewpPresence::Absent => None,
        };
        // Gate 5: the walker resolves and acts (10).
        let mut ctx = MibCtx {
            tree: &mut self.server.tree,
            budget: &mut self.server.budget,
            kernel: &mut self.kernel,
            svc: &mut self.services,
            caller,
            auth: CallAuth::Unknown,
        };
        let mut request = Request { name: &name_vec, oldp, newp };
        let outcome = walker::sysctl(&mut ctx, &mut request);
        // Gate 6: shape the reply (:368-377).
        let (code, out_len) = map_sysctl_reply(
            outcome,
            wire.oldp as u64,
            wire.oldlen as u64,
        );
        out.m_type = code;
        let reply: &mut minix_types::MessMibLcSysctl =
            unsafe { &mut out.m_u.m_mib_lc_sysctl };
        reply.oldlen = out_len as u32;
        (code, out)
    }

    /// Mount a remote subtree (register). C: `mib_register` —
    /// remote.c:197-233.
    fn register_arm(&mut self, msg: &mut Message, is_sendrec: bool) -> i32 {
        // SENDREC is refused — the register protocol is one-way
        // (:210-211), avoiding a MIB→service call cycle.
        if let Err(code) = crate::remote::register_gate(is_sendrec) {
            return code;
        }
        let view = {
            let wire: &MessLsysMibRegister = unsafe { &msg.m_u.m_lsys_mib_register };
            match minix_types::MountRequest::decode_register(msg.m_source, wire) {
                Ok(v) => v,
                Err(EDONTREPLY) => return EDONTREPLY, // >8 lanes: silent drop
                Err(code) => return code,
            }
        };
        if view.miblen == 0 {
            return EDONTREPLY; // empty path: nothing to mount (:894-897)
        }
        // The service's label, fetched from DS. Failures stay silent —
        // a service whose label cannot be resolved is not here yet
        // (:217-218).
        let mut label_buf = [0u8; crate::remote::MIB_LABEL_MAX];
        let label_len = match self
            .services
            .ds_retrieve_label_name(view.mount.service, &mut label_buf)
        {
            Ok(n) => n,
            Err(_) => return EDONTREPLY, // :217-218 (label unresolvable)
        };
        if label_len + 1 > crate::remote::MIB_LABEL_MAX {
            return EDONTREPLY; // :95-99 (surfaced as the silent drop)
        }
        // Slot: reuse the service's seat, or claim a fresh one.
        let mut label_arr = [0u8; crate::remote::MIB_LABEL_MAX];
        label_arr[..label_len].copy_from_slice(&label_buf[..label_len]);
        let label = crate::remote::Label { bytes: label_arr };
        let eid = match crate::remote::locate_slot(&self.server.slots, view.mount.service.0, label)
        {
            SlotVerdict::Reuse { eid } | SlotVerdict::Fresh { eid } => eid,
            SlotVerdict::ReapThenReuse { eid } => {
                // The old mount of this label died with its endpoint:
                // tear the previous forest down first (:64-68).
                self.unmount_exec(eid);
                eid
            }
            SlotVerdict::Full => return EDONTREPLY, // table full (:140-148)
        };
        // Mount: reserve the target window, fetch the node identity
        // from the service, place the covering node. The slot's
        // occupancy (endpt + label) is recorded here — locate_slot only
        // *judges* (:229-232).
        let mount_result = self.mount_exec(eid, &view);
        match mount_result {
            Ok(()) => {
                self.server.slots[eid].endpt = Some(view.mount.service.0);
                self.server.slots[eid].label.bytes = label_arr;
                EDONTREPLY // one-way: no reply ever (:231-232)
            }
            Err(code) => code,
        }
    }

    /// Unmount a service's subtrees (deregister). C: `mib_deregister` —
    /// remote.c:239-307.
    fn deregister_arm(&mut self, msg: &mut Message, is_sendrec: bool) -> i32 {
        if let Err(code) = crate::remote::register_gate(is_sendrec) {
            return code;
        }
        let view = {
            let wire: &MessLsysMibRegister = unsafe { &msg.m_u.m_lsys_mib_register };
            minix_types::MountRequest::decode_deregister(msg.m_source, wire)
        };
        match crate::remote::dereg_slot(&self.server.slots, view.service.0) {
            None => OK, // unknown endpoint: already gone (:250-255)
            Some(eid) => {
                self.unmount_exec(eid);
                OK
            }
        }
    }

    /// Place the covering node for a mount: walk the parent chain,
    /// check the target, stamp `REMOTE` with the service's identity.
    /// C: `mib_mount` — tree.c:1543-1780.
    fn mount_exec(
        &mut self,
        eid: usize,
        view: &minix_types::RegisterView,
    ) -> Result<(), i32> {
        // Head gates: path length, flag window, child window
        // (:1572-1598).
        head_code(check_head(
            view.miblen,
            view.head_flags,
            view.head_csize,
            view.head_clen,
        ))?;
        // Walk all but the last component: each must be an existing
        // parent (:1572-1601 — a missing interior name is EINVAL).
        let mut cur = self.server.tree.root();
        for comp in view.mib[..view.miblen as usize - 1].iter() {
            cur = self.server.tree.child(cur, *comp).ok_or(EINVAL)?;
        }
        // The mount point itself: free → place fresh; occupied → the
        // C obscure rules decide (types match → cover; mismatch →
        // EPERM; busy → EBUSY, rmibtest :109-113).
        let last = view.mib[view.miblen as usize - 1];
        let mut kind = crate::tree::arena::MountKind::InWindow;
        let node = match self.server.tree.child(cur, last) {
            Some(existing) => {
                let t = self.server.tree.slot(existing);
                match check_target(
                    t.flags,
                    view.head_flags | crate::tree::flag::CTLFLAG_PARENT,
                    t.size as u32,
                    t.csize,
                ) {
                    crate::tree::mount::TargetVerdict::Mismatch => {
                        return Err(minix_types::EPERM); // :1666
                    }
                    crate::tree::mount::TargetVerdict::Busy => {
                        return Err(minix_types::EBUSY); // :1677
                    }
                    crate::tree::mount::TargetVerdict::Obscure => {
                        // Cover in place: the node keeps its original
                        // name (the unmount restores visibility by
                        // clearing the stamp alone).
                        kind = crate::tree::arena::MountKind::Obscured;
                        existing
                    }
                }
            }
            None => {
                // Fresh mount. C links a dynode into the parent's
                // dynamic chain — ids beyond the static table are
                // exactly what the chain is for (tree.c:1739-1767).
                let (base, csize) = {
                    let p = self.server.tree.slot(cur);
                    (p.child_base, p.csize)
                };
                if base != u32::MAX && (last as u32) < csize {
                    // Inside a reserved window: an empty slot takes the
                    // mount (its flag was zero — mib_find's "skip").
                    let node = NodeId(base + last as u32);
                    self.server.tree.place(
                        cur,
                        "",
                        last,
                        view.head_flags | CTLFLAG_PERMANENT | CTLTYPE_NODE,
                    );
                    node
                } else {
                    // Beyond the window: a dynamic child on the slab.
                    kind = crate::tree::arena::MountKind::Dynamic;
                    self.server.tree.create_dynode(
                        cur,
                        last,
                        0,
                        &[],
                        view.head_flags | CTLFLAG_PERMANENT | CTLTYPE_NODE,
                        None,
                    )
                }
            }
        };
        // The node identity (name) comes from the service itself:
        // COMMON_MIB_INFO round trip (:316-355) — fresh mounts take
        // the fetched name; obscuring keeps the covered node's own
        // (unmount restores visibility by clearing the stamp alone).
        if kind == crate::tree::arena::MountKind::InWindow {
            let mut name_buf = [0u8; 32];
            self.services
                .remote_info(view.mount.service, &mut name_buf, &mut [])?;
            let n = name_buf.iter().position(|&b| b == 0).unwrap_or(name_buf.len());
            self.server.tree.slot_mut(node).name = name_buf[..n].to_vec().into_boxed_slice();
        }
        {
            let s = self.server.tree.slot_mut(node);
            s.flags |= CTLFLAG_REMOTE;
            s.peer = Some(view.mount.service);
            s.mount_root = Some(view.mount.root_id);
        }
        // Chain the mount into the slot's root list (:189-191) and
        // count it (tree.c:1776).
        self.server.slots[eid].roots.push(crate::tree::arena::MountRoot { node, kind });
        self.server.tree.counts = self.server.tree.counts.remote_added();
        Ok(())
    }

    /// Tear a service's mounts down (its `mib_down`). C: remote.c:55-74.
    fn unmount_exec(&mut self, eid: usize) {
        let roots = core::mem::take(&mut self.server.slots[eid].roots);
        for root in roots {
            match root.kind {
                // Fresh in-window placement: the slot returns to empty
                // (flags zero — mib_find's "skip" view, tree.c:48-88).
                crate::tree::arena::MountKind::InWindow => {
                    self.server.tree.slot_mut(root.node).flags = 0;
                }
                // Obscured original: strip the stamp, the node's own
                // view comes back (:1789-1842).
                crate::tree::arena::MountKind::Obscured => {
                    self.server.tree.slot_mut(root.node).flags &= !CTLFLAG_REMOTE;
                }
                // Dynamic child: unlink from the parent's map.
                crate::tree::arena::MountKind::Dynamic => {
                    // The parent rides the node itself (set at create).
                    let parent = self.server.tree.slot(root.node).parent;
                    if let Some(p) = parent {
                        self.server.tree.unlink_dynode(p, root.node);
                    }
                }
            }
            self.server.tree.counts = self.server.tree.counts.remote_removed();
        }
        self.server.slots[eid].endpt = None;
    }
}

/// The real end: delegates to `minix-sys`'s `IpcTransport` — final
/// shape, powered when the trap layer lands (edge E1). Until then every
/// receive reports the transport's failure and the loop dies at the
/// bound; a sysctl server that cannot receive must not pretend otherwise.
#[derive(Debug, Default)]
pub struct SysIpc {
    /// The minix-sys direct trap transport.
    pub transport: minix_sys::ipc::DirectTrapTransport,
}

impl MibIpc for SysIpc {
    fn receive_status(&mut self, message: &mut Message) -> Result<u32, i32> {
        self.transport
            .receive(Endpoint::ANY, message)
            .map(|status| status.0)
            .map_err(|status| status.0)
    }

    fn send_nb(&mut self, to: Endpoint, message: &Message) -> Result<(), i32> {
        self.transport
            .sendnb(to, message)
            .map_err(|status| status.0)
    }

    fn send_rec(&mut self, peer: Endpoint, message: &mut Message) -> Result<(), i32> {
        self.transport
            .sendrec(peer, message)
            .map_err(|status| status.0)
    }

    fn notify(&mut self, to: Endpoint) -> Result<(), i32> {
        self.transport.notify(to).map_err(|status| status.0)
    }
}

/// The SEF library reads through the same verbs (E-ISWIRE: the receive
/// half of the MIB loop is `minix_sef::sef_receive_status`, so RS pings
/// are ponged and swallowed inside the library — C mib main.c:443 reads
/// with `sef_receive_status` for exactly this reason).
impl minix_sef::SefIpc for SysIpc {
    fn receive(&mut self, _src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        <Self as MibIpc>::receive_status(self, msg).map(|s| s as i32)
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        <Self as MibIpc>::notify(self, dest)
    }
}

impl<K: MibKernel, S: MibServices, I: MibIpc + minix_sef::SefIpc> Server<K, S, I> {
    /// Run the server loop until the transport dies for good. C:
    /// `main()` — main.c:433-492 (the receive "cannot fail" panic
    /// becomes a counted bound — the SCHED/DS honest-death discipline).
    pub fn run(&mut self) {
        let mut failures = 0u32;
        loop {
            match self.run_once() {
                (Turn::Handled, _) => failures = 0,
                (Turn::ReceiveFailed, _) => {
                    failures += 1;
                    if failures >= MAX_CONSECUTIVE_RECV_FAILURES {
                        return; // a deaf loop is unrecoverable (main.c:446)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::recording::Recorder;
    use core::cell::RefCell;
    use minix_types::{
        CTL_HW, CTL_KERN, CTL_MINIX, EIO, ENOSYS, HW_MACHINE, KERN_HARDCLOCK_TICKS, MINIX_TEST,
        OK, SYSCTL_VERSION, CTLFLAG_READWRITE, CTLTYPE_NODE,
    };

    /// Scripted message seam: a FIFO of received messages plus a log
    /// of every reply sent and every pong notified.
    struct MockIpc {
        queue: RefCell<Vec<(u32, Message)>>,
        sent: RefCell<Vec<Message>>,
        pongs: RefCell<Vec<Endpoint>>,
        fail_recv: bool,
    }

    impl MockIpc {
        fn new(items: &[(u32, Message)]) -> Self {
            Self {
                queue: RefCell::new(items.to_vec()),
                sent: RefCell::new(Vec::new()),
                pongs: RefCell::new(Vec::new()),
                fail_recv: false,
            }
        }
    }

    impl minix_sef::SefIpc for MockIpc {
        fn receive(&mut self, _src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
            <MockIpc as MibIpc>::receive_status(self, msg).map(|s| s as i32)
        }

        fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
            <MockIpc as MibIpc>::notify(self, dest)
        }
    }

    impl MibIpc for MockIpc {
        fn notify(&mut self, to: Endpoint) -> Result<(), i32> {
            self.pongs.borrow_mut().push(to);
            Ok(())
        }

        fn receive_status(&mut self, message: &mut Message) -> Result<u32, i32> {
            if self.fail_recv {
                return Err(EIO);
            }
            match self.queue.borrow_mut().pop() {
                Some((status, m)) => {
                    *message = m;
                    Ok(status)
                }
                None => Err(EIO),
            }
        }

        fn send_nb(&mut self, _to: Endpoint, message: &Message) -> Result<(), i32> {
            self.sent.borrow_mut().push(*message);
            Ok(())
        }

        fn send_rec(&mut self, _peer: Endpoint, _message: &mut Message) -> Result<(), i32> {
            Ok(())
        }
    }

    fn mib_msg(mtype: i32) -> Message {
        let mut m = Message::default();
        m.m_type = mtype;
        m.m_source = Endpoint::PM;
        m
    }

    /// Build the server with a scripted IPC queue; kernel gets ticks =
    /// 7, services answers DS label "ipc" + remote_info name "ipc".
    fn server_with(items: &[(u32, Message)]) -> Server<Recorder, Recorder, MockIpc> {
        let mut kernel = Recorder::default();
        kernel.getticks_result = Some(7);
        let mut svc = Recorder::default();
        svc.ds_label = Some(b"ipc".to_vec());
        Server::new(MibServer::new(), MockIpc::new(items), kernel, svc)
    }

    /// Notify arrivals are logged and never answered (:449-454).
    #[test]
    fn test_notify_never_replies() {
        let mut s = server_with(&[(minix_sys::ipc::CALL_NOTIFY, mib_msg(minix_types::MIB_SYSCTL))]);
        let (turn, incoming) = s.run_once();
        assert_eq!(turn, Turn::Handled);
        assert_eq!(incoming, Incoming::NotifyRefusal);
        assert!(s.ipc.sent.borrow().is_empty());
    }

    /// RS ping: ponged inside the SEF library and swallowed — the turn is
    /// the NEXT receive failing on the exhausted script, never a triage
    /// verdict, and no reply goes out (E-ISWIRE; C: sef.c:208-214 +
    /// sef_ping.c:21-38 — the loop reads through `sef_receive_status`).
    #[test]
    fn test_rs_ping_ponged_and_swallowed() {
        let mut ping = Message::default();
        ping.m_type = minix_sef::SEF_PING_REQUEST_TYPE as i32;
        ping.m_source = minix_sef::RS_ENDPOINT;
        let mut s = server_with(&[(minix_sys::ipc::CALL_NOTIFY as u32, ping)]);
        let (turn, incoming) = s.run_once();
        assert_eq!(turn, Turn::ReceiveFailed);
        assert_eq!(incoming, Incoming::Unknown);
        assert!(s.ipc.sent.borrow().is_empty());
        assert_eq!(*s.ipc.pongs.borrow(), vec![minix_sef::RS_ENDPOINT]);
    }

    /// RS ping followed by a real call: the ping vanishes, the call is
    /// answered normally (the loop survives the interception).
    #[test]
    fn test_rs_ping_then_call_is_answered() {
        let mut ping = Message::default();
        ping.m_type = minix_sef::SEF_PING_REQUEST_TYPE as i32;
        ping.m_source = minix_sef::RS_ENDPOINT;
        // MockIpc pops LIFO — the ping (listed last) is received first.
        let mut s = server_with(&[
            (minix_sys::ipc::CALL_SENDREC, mib_msg(0x42)),
            (minix_sys::ipc::CALL_NOTIFY as u32, ping),
        ]);
        let (turn, incoming) = s.run_once();
        assert_eq!(turn, Turn::Handled);
        assert_eq!(incoming, Incoming::Unknown);
        let sent = s.ipc.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, ENOSYS);
        assert_eq!(*s.ipc.pongs.borrow(), vec![minix_sef::RS_ENDPOINT]);
    }

    /// Wild number from a blocking caller: ENOSYS goes back (:474-479).
    #[test]
    fn test_unknown_sendrec_gets_enosys() {
        let mut s = server_with(&[(minix_sys::ipc::CALL_SENDREC, mib_msg(0x42))]);
        let (turn, incoming) = s.run_once();
        assert_eq!(turn, Turn::Handled);
        assert_eq!(incoming, Incoming::Unknown);
        let sent = s.ipc.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, ENOSYS);
    }

    /// Wild number one-way: silence — EDONTREPLY (:474-479).
    #[test]
    fn test_unknown_one_way_stays_silent() {
        let mut s = server_with(&[(1, mib_msg(0x42))]); // SEND, not SENDREC
        let (turn, _) = s.run_once();
        assert_eq!(turn, Turn::Handled);
        assert!(s.ipc.sent.borrow().is_empty());
    }

    /// Receive failure: the loop counts it (:445-448 — panic in C, a
    /// counted turn here).
    #[test]
    fn test_receive_failure_counts() {
        let mut s = server_with(&[]);
        s.ipc.fail_recv = true;
        let (turn, _) = s.run_once();
        assert_eq!(turn, Turn::ReceiveFailed);
    }

    /// Full sysctl read: the answer rides the sink and the reply
    /// carries OK with the reported length (:368-374).
    #[test]
    fn test_sysctl_read_replies_ok_with_length() {
        let machine = minix_types::HW_MACHINE;
        let mut m = mib_msg(minix_types::MIB_SYSCTL);
        {
            let w: &mut MessLcMibSysctl = unsafe { &mut m.m_u.m_lc_mib_sysctl };
            w.namelen = 2;
            w.name[..2].copy_from_slice(&[CTL_HW, machine]);
            w.oldp = 0x5000;
            w.oldlen = 64;
        }
        let mut s = server_with(&[(minix_sys::ipc::CALL_SENDREC, m)]);
        let (turn, _) = s.run_once();
        assert_eq!(turn, Turn::Handled);
        let sent = s.ipc.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, OK);
        let reply: &minix_types::MessMibLcSysctl = unsafe { &sent[0].m_u.m_mib_lc_sysctl };
        assert_eq!(reply.oldlen, 7); // "x86_64" + NUL
        // The answer bytes crossed the copy transport.
        let written = s.kernel.written.borrow();
        assert_eq!(&written[..], b"x86_64\0");
    }

    /// Register success: the label resolves, the mount lands, and the
    /// one-way protocol stays silent (:231-232).
    #[test]
    fn test_register_mounts_and_stays_silent() {
        let mut m = mib_msg(minix_types::MIB_REGISTER);
        {
            let w: &mut MessLsysMibRegister = unsafe { &mut m.m_u.m_lsys_mib_register };
            w.root_id = 3;
            w.flags = SYSCTL_VERSION | CTLTYPE_NODE | CTLFLAG_READWRITE;
            w.csize = 16;
            w.clen = 1;
            w.miblen = 2;
            w.mib[..2].copy_from_slice(&[CTL_MINIX, 5]); // free id under minix
        }
        let mut s = server_with(&[(1, m)]); // one-way send
        let (turn, incoming) = s.run_once();
        assert_eq!(turn, Turn::Handled);
        assert_eq!(incoming, Incoming::Dispatch(MibCall::Register));
        // Silent: no reply for a one-way register.
        assert!(s.ipc.sent.borrow().is_empty());
        // The mounted node exists, stamped REMOTE with the peer.
        let minix = s.server.tree.child(s.server.tree.root(), CTL_MINIX).unwrap();
        let mounted = s.server.tree.child(minix, 5).expect("mounted node");
        assert_ne!(
            s.server.tree.slot(mounted).flags & crate::tree::flag::CTLFLAG_REMOTE,
            0
        );
        assert_eq!(
            s.server.tree.slot(mounted).peer,
            Some(Endpoint::PM)
        );
        // The roots chain and the counter moved.
        assert_eq!(s.server.slots[0].roots.len(), 1);
        assert_eq!(s.server.tree.counts.remotes, 1);
    }

    /// Deregister tears the mount down: the node leaves the window and
    /// the counters return (:250-255 silent when unknown).
    #[test]
    fn test_register_then_deregister_restores() {
        let base_remotes = 0;
        let mut reg = mib_msg(minix_types::MIB_REGISTER);
        {
            let w: &mut MessLsysMibRegister = unsafe { &mut reg.m_u.m_lsys_mib_register };
            w.root_id = 3;
            w.flags = SYSCTL_VERSION | CTLTYPE_NODE | CTLFLAG_READWRITE;
            w.csize = 16;
            w.clen = 1;
            w.miblen = 2;
            w.mib[..2].copy_from_slice(&[CTL_MINIX, 5]);
        }
        let mut s = server_with(&[(1, reg)]);
        let _ = s.run_once();
        assert_eq!(s.server.tree.counts.remotes, base_remotes + 1);
        let mut der = mib_msg(minix_types::MIB_DEREGISTER);
        {
            let w: &mut MessLsysMibRegister = unsafe { &mut der.m_u.m_lsys_mib_register };
            w.root_id = 3;
        }
        s.ipc.queue.borrow_mut().push((1, der));
        let (turn, _) = s.run_once();
        assert_eq!(turn, Turn::Handled);
        assert_eq!(s.server.tree.counts.remotes, base_remotes);
        let minix = s.server.tree.child(s.server.tree.root(), CTL_MINIX).unwrap();
        assert!(s.server.tree.child(minix, 5).is_none());
        assert!(s.server.slots[0].endpt.is_none());
    }

    /// Register from a SENDREC caller: ENOSYS, not silence (:210-211).
    #[test]
    fn test_register_sendrec_gets_enosys() {
        let mut m = mib_msg(minix_types::MIB_REGISTER);
        {
            let w: &mut MessLsysMibRegister = unsafe { &mut m.m_u.m_lsys_mib_register };
            w.miblen = 2;
            w.mib[..2].copy_from_slice(&[CTL_MINIX, 5]);
        }
        let mut s = server_with(&[(minix_sys::ipc::CALL_SENDREC, m)]);
        let (turn, _) = s.run_once();
        assert_eq!(turn, Turn::Handled);
        let sent = s.ipc.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, ENOSYS);
    }

    /// Register with an unresolvable label: silent — the service is
    /// not here yet (:217-218).
    #[test]
    fn test_register_label_failure_is_silent() {
        let mut m = mib_msg(minix_types::MIB_REGISTER);
        {
            let w: &mut MessLsysMibRegister = unsafe { &mut m.m_u.m_lsys_mib_register };
            w.miblen = 2;
            w.mib[..2].copy_from_slice(&[CTL_MINIX, 5]);
        }
        let mut s = server_with(&[(1, m)]);
        s.services.ds_label = None; // DS lookup fails
        let (turn, incoming) = s.run_once();
        assert_eq!(turn, Turn::Handled);
        assert_eq!(incoming, Incoming::Dispatch(MibCall::Register));
        assert!(s.ipc.sent.borrow().is_empty(), "EDONTREPLY: silence");
    }

    /// Sysctl read of a function node rides getticks (smoke over the
    /// registry path through the assembled server).
    #[test]
    fn test_sysctl_func_read_rides_getticks() {
        let mut m = mib_msg(minix_types::MIB_SYSCTL);
        {
            let w: &mut MessLcMibSysctl = unsafe { &mut m.m_u.m_lc_mib_sysctl };
            w.namelen = 2;
            w.name[..2].copy_from_slice(&[CTL_KERN, KERN_HARDCLOCK_TICKS]);
            w.oldp = 0x9000;
            w.oldlen = 4;
        }
        let mut s = server_with(&[(minix_sys::ipc::CALL_SENDREC, m)]);
        let _ = s.run_once();
        let sent = s.ipc.sent.borrow();
        assert_eq!(sent[0].m_type, OK);
        let reply: &minix_types::MessMibLcSysctl = unsafe { &sent[0].m_u.m_mib_lc_sysctl };
        assert_eq!(reply.oldlen, 4);
    }
}
