//! E5(b) — the VM↔VFS `FDCLOSE` round trip (host side, request and reply built
//! to the wire).
//!
//! VM borrows an fd from VFS for a file mapping (`FDLOOKUP` stores a copy in its
//! own process table) and hands it back when done (`FDCLOSE`). This round trip
//! has only two cross-service actions, and all of VFS's semantics for it fall on
//! one message in and one message out, so it can be tested on the host close to
//! real-machine fidelity.
//!
//! # Chain surface (three real code pieces; only VM's sending half is replaced
//! by a wire-shaped construction)
//!
//! 1. **VM sending half**: the production implementation is
//!    `VfsRequestQueue::take_pending_vfs_call`
//!    (`os/servers/vm/src/vfs_queue.rs:161-203`), which writes six semantic
//!    fields into `minix_types::MessVmVfsCall`. The whole queue is
//!    `pub(crate)`, so host tests cannot link to it; this file therefore builds
//!    the request against the same wire structure — both sides write the same
//!    struct, so this construction *is* the VM↔VFS byte convention itself (the
//!    56-byte layout of `MessVmVfsCall` has its own offset witness in
//!    `minix-types/src/ipc/vm.rs:1811-1821`).
//! 2. **VFS receiving half**: `minix_vfs::misc::decode_vm_call`, the real
//!    decoder, extracts the six fields (`misc.rs:422-437`), then follows the
//!    `VMVFSREQ_FDCLOSE` branch of C's `do_vm_call`
//!    (`minix3/minix/servers/vfs/misc.c:452-462`) into `close_fd`'s real
//!    semantics (`filedes.rs:202`, C `open.c:690`).
//! 3. **VM receiving half**: `minix_types::VmVfsReplyIn::decode_message` — the
//!    very decoder VM's main loop uses to parse replies
//!    (`os/servers/vm/src/ipc/dispatcher.rs:773`).
//!
//! # Division of labour with each side's own unit tests (not copied here)
//!
//! - `decode_vm_call`'s pure decode and reject behaviour, plus the literal
//!   value of `VM_VFS_REPLY`, are already covered by the round-trip test at
//!   `os/servers/vfs/src/misc.rs:1355-1399` and the pinned constants at
//!   `minix-types/src/ipc/vm.rs:1804-1808`, so they are not repeated here;
//! - `close_fd`'s own fd-bit clear / `cloexec` clear / reference-count
//!   decrement is already covered by `test_close_ok` at
//!   `os/servers/vfs/src/filedes.rs:501-522`;
//! - VM queue revocation by owner (`purge_by_owner`, the dead-process fallback)
//!   is a VM-crate internal face, `pub(crate)` and unlinkable, covered by
//!   `os/servers/vm/src/vfs_queue.rs:459-528`'s own tests.
//!
//! What this file tests is **the seam between those three pieces**: the
//! request's fd and `req_id` come off the wire, the close result goes back into
//! the reply, and the reply is understood by the peer's decoder. The
//! real-machine round trip (VM truly initiating, VFS truly replying) is tracked
//! under T2 / E5(b).

use minix_types::{
    Endpoint, Message, MessVmVfsCall, MessVmVfsReply, UserSlot, VmVfsReplyIn, EBADF, OK,
    VFS_VMCALL, VMVFSREQ_FDCLOSE, VM_VFS_REPLY,
};
use minix_vfs::filedes::{close_fd, Fd, FdError};
use minix_vfs::filp::FilpTable;
use minix_vfs::fproc::FProcTable;
use minix_vfs::misc::{decode_vm_call, VmVfsReq};

/// The fd VM borrows (stored in VM's own `fproc`).
const VM_FD: u8 = 3;
/// Another fd right next to it — used to prove the close touches only the one
/// named.
const NEIGHBOUR_FD: u8 = 4;
/// Request id: VM uses it to match the reply back to the pending request.
///
/// The VM side requires `reqid > 0` (0 means "no pending request", a negative
/// value is an error sentinel, `os/servers/vm/src/ipc/dispatcher.rs:327-332`),
/// and the fixture follows that.
const REQ_ID: u32 = 0x2A;

/// VM's `fproc` slot.
///
/// C's `do_vm_call` closes the **serving process's own** fd table in the FDCLOSE
/// branch (`close_fd(fp, req_fd, FALSE)`, `misc.c:454`, where `fp` is the
/// current session process, i.e. VM), while the message's `endpoint` field is
/// used only by FDLOOKUP to locate the referenced process (`misc.c:405`'s
/// `assert(rfp != vmf)` is itself the proof that the two must not be
/// conflated).
fn vm_slot() -> UserSlot {
    Endpoint::VM
        .to_user_slot()
        .expect("VM 是用户态进程，端点可解出槽位")
}

/// Build one VM→VFS request message (same structure as VM's sending half).
fn vm_call_message(req: i32, fd: i32, endpoint: Endpoint, req_id: u32) -> Message {
    let mut msg = Message {
        m_type: VFS_VMCALL,
        ..Message::default()
    };
    msg.m_u.m_vm_vfs_call = MessVmVfsCall {
        offset: 0,
        req,
        fd,
        req_id: req_id as i32,
        endpoint: endpoint.0,
        _l1: 0,
        _l2: 0,
        length: 0,
        _padding: [0; 20],
    };
    msg
}

/// Build a VFS→VM reply message (same field order as C `do_vm_call:487-494`).
fn vfs_reply_message(endpoint: Endpoint, result: i32, req_id: u32) -> Message {
    let mut msg = Message {
        m_type: VM_VFS_REPLY as i32,
        ..Message::default()
    };
    msg.m_u.m_vm_vfs_reply = MessVmVfsReply {
        ull1: 0,
        endpoint: endpoint.0,
        result,
        reqid: req_id as i32,
        dev: 0,
        ino: 0,
        fd: 0,
        size_pages: 0,
        _padding: [0; 20],
    };
    msg
}

// ---------------------------------------------------------------------------
// E5(b).1 success round trip: the borrowed fd closes, the refcount drops, the
// reply returns to VM under the request id
// ---------------------------------------------------------------------------

#[test]
fn vm_fdclose_round_trip_closes_borrowed_fd_and_replies_ok() {
    // ── VM-side state: one borrowed fd plus one neighbouring fd ──
    let mut filps = FilpTable::new();
    let borrowed = filps
        .alloc_filp(minix_vfs::open::R_BIT | minix_vfs::open::W_BIT)
        .expect("空表必有空 filp 槽");
    filps.inc_count(borrowed);
    let peer_filp = filps
        .alloc_filp(minix_vfs::open::R_BIT)
        .expect("空表必有空 filp 槽");
    filps.inc_count(peer_filp);
    filps.inc_count(peer_filp);

    let referenced_ep = Endpoint::from_generation_slot(1, 11);
    let referenced_slot = referenced_ep
        .to_user_slot()
        .expect("被引用端点是用户态进程");

    let mut fproc = FProcTable::new();
    {
        let vm = fproc.get_mut(vm_slot()).expect("VM 槽存在");
        vm.pid = 8;
        vm.endpoint = Endpoint::VM;
        vm.filps[VM_FD as usize] = Some(borrowed.get());
        vm.cloexec_set.set(VM_FD as usize, true);
        vm.filps[NEIGHBOUR_FD as usize] = Some(borrowed.get());
    }
    {
        // The referenced process (the one the message's `endpoint` field points
        // at) sits in the same table, and its **same fd number** also carries a
        // filp — that is what gives "the table being closed is VM's" an
        // observable surface: if the close landed on the wrong object, this fd
        // would be cleared and this refcount would drop.
        let peer = fproc.get_mut(referenced_slot).expect("被引用槽存在");
        peer.pid = 11;
        peer.endpoint = referenced_ep;
        peer.filps[VM_FD as usize] = Some(peer_filp.get());
    }
    // The borrowed filp is pointed at by two of VM's fds (VM_FD and
    // NEIGHBOUR_FD); the peer fd points at another filp, with count 2.
    filps.inc_count(borrowed);

    // ── Step one: VFS receives VM's request and decodes it ──
    let request = vm_call_message(
        VMVFSREQ_FDCLOSE,
        VM_FD as i32,
        referenced_ep,
        REQ_ID,
    );
    let call = decode_vm_call(&request).expect("FDCLOSE 请求应解码成功");
    assert_eq!(call.req, VmVfsReq::FdClose, "opcode 102 归一为 FdClose");
    assert_eq!(call.fd, VM_FD as i32, "fd 域来自 wire");
    assert_eq!(call.req_id, REQ_ID, "req_id 域来自 wire");
    assert_eq!(call.endpoint, referenced_ep.0, "endpoint 域来自 wire");

    // ── Step two: close this fd along C's branch ──
    // C: `result = close_fd(fp, req_fd, FALSE /*may_suspend*/);` (misc.c:454),
    // where `fp` is the current session process's (VM's) fproc, not the
    // referenced process named in the message.
    let outcome = {
        let vm = fproc.get_mut(vm_slot()).expect("VM 槽存在");
        close_fd(
            vm,
            Fd::new(call.fd as usize).expect("fd 落在 0..OPEN_MAX"),
            &mut filps,
        )
    };
    assert!(outcome.is_ok(), "借用的 fd 有效，关闭应成功");

    // Close semantics (cf. C `open.c:699-710`): the fd bit is cleared, the
    // cloexec bit is cleared, and the filp slot is released once the refcount
    // reaches 0.
    let vm = fproc.get(vm_slot()).expect("VM 槽存在");
    assert!(
        vm.filps[VM_FD as usize].is_none(),
        "fp_filp[fd] 置 NULL（先于 close_filp，重复关闭将得 EBADF）"
    );
    assert!(
        !vm.cloexec_set.get(VM_FD as usize),
        "FD_CLR(fd, &fp_cloexec_set)"
    );
    assert_eq!(
        vm.filps[NEIGHBOUR_FD as usize],
        Some(borrowed.get()),
        "相邻 fd 不受影响"
    );
    let closed = filps.get(borrowed).expect("filp 槽仍在表内");
    assert_eq!(
        closed.count, 1,
        "借来的 filp 还有 VM 的另一个 fd 指着，计数只递减一格"
    );
    assert_eq!(
        filps.get(peer_filp).expect("对端 filp 仍在表内").count,
        2,
        "对端的 filp 引用计数不动"
    );
    // The referenced process is slot 11 in this same table: the entry at the
    // same fd number stays exactly as it was. This is the counter-proof that
    // "the fd table being closed is VM's own" — had the close hit the endpoint
    // named in the message, this fd would be cleared.
    let peer = fproc.get(referenced_slot).expect("被引用槽存在");
    assert_eq!(
        peer.filps[VM_FD as usize],
        Some(peer_filp.get()),
        "被引用进程同一 fd 号的表项原样保留"
    );

    // The borrowed filp has only VM's NEIGHBOUR_FD left; closing that too is the
    // final close, at which point the filp slot is released (the
    // `--filp_count == 0` branch at C `filedes.c:496`).
    {
        let vm = fproc.get_mut(vm_slot()).expect("VM 槽存在");
        close_fd(
            vm,
            Fd::new(NEIGHBOUR_FD as usize).expect("界内"),
            &mut filps,
        )
        .expect("第二个 fd 同样有效");
    }
    let closed = filps.get(borrowed).expect("filp 槽仍在表内");
    assert_eq!(closed.count, 0, "最后一个引用消失，filp_count 归零");
    assert_eq!(
        closed.mode,
        minix_vfs::filp::FILP_CLOSED,
        "最后一个引用消失时 filp 置 CLOSED（filedes.c:496 的 dec_count）"
    );

    // ── Step three: the reply returns to VM and the peer decoder understands it ──
    // C fills VMV_ENDPOINT / VMV_RESULT / VMV_REQID at `reqdone`, then sends
    // `VM_VFS_REPLY` asynchronously (misc.c:487-494).
    let reply = vfs_reply_message(referenced_ep, OK, call.req_id);
    assert_eq!(reply.m_type, VM_VFS_REPLY as i32);
    let decoded = VmVfsReplyIn::decode_message(&reply);
    assert_eq!(decoded.reqid as u32, REQ_ID, "应答按 req_id 回到原请求");
    assert_eq!(decoded.result, OK, "成功码原样带回");
    assert_eq!(
        decoded.endpoint, referenced_ep,
        "VMV_ENDPOINT 即被引用进程，VM 用它把结果对回挂起的请求"
    );
}

// ---------------------------------------------------------------------------
// E5(b).2 error path: a reply for an invalid fd is still a reply (carrying the
// errno and the request id)
// ---------------------------------------------------------------------------

#[test]
fn vm_fdclose_bad_fd_replies_ebadf_and_leaves_other_fds_alone() {
    let mut filps = FilpTable::new();
    let live = filps.alloc_filp(minix_vfs::open::R_BIT).expect("空槽");
    filps.inc_count(live);

    let mut fproc = FProcTable::new();
    {
        let vm = fproc.get_mut(vm_slot()).expect("VM 槽存在");
        vm.pid = 8;
        vm.filps[VM_FD as usize] = Some(live.get());
    }
    let referenced_ep = Endpoint::from_generation_slot(1, 11);

    // ── invalid fd: the message is legal but names an fd that is not open ──
    let empty_fd = NEIGHBOUR_FD as i32;
    let request = vm_call_message(VMVFSREQ_FDCLOSE, empty_fd, referenced_ep, REQ_ID);
    let call = decode_vm_call(&request).expect("opcode 合法，解码应成功");
    assert_eq!(call.req, VmVfsReq::FdClose);

    let err = {
        let vm = fproc.get_mut(vm_slot()).expect("VM 槽存在");
        close_fd(
            vm,
            Fd::new(call.fd as usize).expect("fd 在界内"),
            &mut filps,
        )
    }
    .expect_err("空 fd 位必须拒绝");
    assert_eq!(err, FdError::BadFd);
    assert_eq!(err.to_errno(), EBADF);

    // C prints one diagnostic line and still returns the error code as usual
    // (`misc.c:456-460`: printf when `result != OK`, then `VMV_RESULT = result`
    // at `reqdone`), so a failure is a normal reply too and must carry the
    // original request id — VM matches the result back to the pending request
    // with it, otherwise the failure becomes a pending request whose "reply was
    // lost". In the on-wire server build the errno value is negative (`_SIGN` at
    // `minix3/sys/sys/errno.h:187-190` expands to `-` under `_SYSTEM`), so the
    // reply carries `-EBADF`; Rust's `FdError::to_errno()` yields the
    // user-visible positive number (`minix-types/src/types/errno.rs:23`), so it
    // is negated before going on the wire.
    let reply = vfs_reply_message(referenced_ep, -err.to_errno(), call.req_id);
    let decoded = VmVfsReplyIn::decode_message(&reply);
    assert_eq!(decoded.result, -EBADF, "VMV_RESULT 携负号 errno");
    assert_eq!(decoded.reqid as u32, REQ_ID, "错误路径同样按请求号回去");

    // A rejection produces no side effects: the named fd was empty to begin
    // with, and the fd in use and its filp refcount must stay untouched —
    // `close_fd` only decrements after its validation passes.
    let vm = fproc.get(vm_slot()).expect("VM 槽存在");
    assert_eq!(
        vm.filps[VM_FD as usize],
        Some(live.get()),
        "被拒绝的关闭不得动别的 fd"
    );
    assert_eq!(
        filps.get(live).expect("filp 槽在").count,
        1,
        "引用计数不动"
    );
}
