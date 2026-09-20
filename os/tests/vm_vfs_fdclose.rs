//! E5(b) — VM↔VFS 的 `FDCLOSE` 往返（宿主态，按 wire 构造请求与应答）。
//!
//! VM 为文件映射向 VFS 借用一个 fd（`FDLOOKUP` 在自己的进程表里落一个副本），
//! 用完再把它还回去（`FDCLOSE`）。这条往返只有两个跨服务动作，VFS 侧的全部
//! 语义恰好落在一条消息的一进一出上，所以它能在宿主态测到接近真机的程度。
//!
//! # 链路面（三段真代码，只有 VM 的发送半以 wire 形状代替）
//!
//! 1. **VM 发送半**：生产实现是 `VfsRequestQueue::take_pending_vfs_call`
//!    （`os/servers/vm/src/vfs_queue.rs:161-203`），它把六个语义域写进
//!    `minix_types::MessVmVfsCall`。整个队列是 `pub(crate)`，宿主测试链接不到，
//!    因此本文件按同一 wire 结构构造请求——两边写的是同一个结构体，所以这条
//!    构造就是 VM↔VFS 的字节约定本身（`MessVmVfsCall` 的 56 字节布局另有
//!    `minix-types/src/ipc/vm.rs:1811-1821` 的偏移见证）。
//! 2. **VFS 接收半**：`minix_vfs::misc::decode_vm_call` 真解码器解出六域
//!    （`misc.rs:422-437`），随后按 C `do_vm_call` 的 `VMVFSREQ_FDCLOSE` 分支
//!    （`minix3/minix/servers/vfs/misc.c:452-462`）走 `close_fd` 真语义
//!    （`filedes.rs:202`，C `open.c:690`）。
//! 3. **VM 接收半**：`minix_types::VmVfsReplyIn::decode_message`——即 VM 主循环
//!    从消息解应答用的那个解码器（`os/servers/vm/src/ipc/dispatcher.rs:773`）。
//!
//! # 与两侧自有单测的分工（本文件不抄的部分）
//!
//! - `decode_vm_call` 的纯解码与拒绝行为、`VM_VFS_REPLY` 的字面值，已由
//!   `os/servers/vfs/src/misc.rs:1355-1399` 的往返测试与
//!   `minix-types/src/ipc/vm.rs:1804-1808` 的常量钉值覆盖，此处不再重复；
//! - `close_fd` 自身的 fd 清位 / `cloexec` 清位 / 引用计数递减，已由
//!   `os/servers/vfs/src/filedes.rs:501-522` 的 `test_close_ok` 覆盖；
//! - VM 队列按属主撤销（`purge_by_owner`，死进程兜底）是 VM crate 内部面，
//!   `pub(crate)` 不可链接，由 `os/servers/vm/src/vfs_queue.rs:459-528` 自测覆盖。
//!
//! 本文件测的是**这三段之间的接缝**：请求的 fd 与 `req_id` 从 wire 里来、
//! 关闭结果回到应答里、应答被对端解码器读懂。真机往返（VM 真实发起、VFS 真实
//! 应答）挂 edge4 T2 / E5(b)。

use minix_types::{
    Endpoint, Message, MessVmVfsCall, MessVmVfsReply, UserSlot, VmVfsReplyIn, EBADF, OK,
    VFS_VMCALL, VMVFSREQ_FDCLOSE, VM_VFS_REPLY,
};
use minix_vfs::filedes::{close_fd, Fd, FdError};
use minix_vfs::filp::FilpTable;
use minix_vfs::fproc::FProcTable;
use minix_vfs::misc::{decode_vm_call, VmVfsReq};

/// VM 借来用的那个 fd（落在 VM 自己的 `fproc` 里）。
const VM_FD: u8 = 3;
/// 与它相邻的另一个 fd——用来证明关闭只动被点名的那个。
const NEIGHBOUR_FD: u8 = 4;
/// 请求号：VM 用它把应答对回挂起的请求。
///
/// VM 侧要求 `reqid > 0`（0 表示"无挂起请求"，负值是错误哨兵，
/// `os/servers/vm/src/ipc/dispatcher.rs:327-332`），夹具照此取值。
const REQ_ID: u32 = 0x2A;

/// VM 的 `fproc` 槽位。
///
/// C 的 `do_vm_call` 在 FDCLOSE 分支里关的是**服务进程自己的** fd 表
/// （`close_fd(fp, req_fd, FALSE)`，`misc.c:454`，其中 `fp` 是当前会话进程即 VM），
/// 而消息里的 `endpoint` 域只被 FDLOOKUP 用作被引用进程的定位（`misc.c:405`
/// 的 `assert(rfp != vmf)` 就是这两者不可混同的自证）。
fn vm_slot() -> UserSlot {
    Endpoint::VM
        .to_user_slot()
        .expect("VM 是用户态进程，端点可解出槽位")
}

/// 构造一条 VM→VFS 的请求消息（与 VM 发送半同结构）。
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

/// 构造 VFS→VM 的应答消息（与 C `do_vm_call:487-494` 的字段顺序同）。
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
// E5(b).1 成功往返：借用的 fd 关掉，引用计数递减，应答按请求号回到 VM
// ---------------------------------------------------------------------------

#[test]
fn vm_fdclose_round_trip_closes_borrowed_fd_and_replies_ok() {
    // ── VM 侧的状态：一个借来的 fd + 一个相邻 fd ──
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
        // 被引用进程（消息 `endpoint` 域指向的那个）放在同一张表里，
        // 且**同一个 fd 号**上也放一个 filp——这样"关的是 VM 的表"才有观测面：
        // 若关闭落错了对象，这里的 fd 会被清掉、引用计数会掉。
        let peer = fproc.get_mut(referenced_slot).expect("被引用槽存在");
        peer.pid = 11;
        peer.endpoint = referenced_ep;
        peer.filps[VM_FD as usize] = Some(peer_filp.get());
    }
    // 借来的 filp 有 VM 的两个 fd 指向它（VM_FD 与 NEIGHBOUR_FD），
    // 对端 fd 指向另一个 filp，计数为 2。
    filps.inc_count(borrowed);

    // ── 第一步：VFS 收到 VM 的请求并解码 ──
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

    // ── 第二步：按 C 的分支关掉这个 fd ──
    // C: `result = close_fd(fp, req_fd, FALSE /*may_suspend*/);`（misc.c:454），
    // `fp` 是本会话进程（VM）的 fproc，不是消息里那个被引用进程。
    let outcome = {
        let vm = fproc.get_mut(vm_slot()).expect("VM 槽存在");
        close_fd(
            vm,
            Fd::new(call.fd as usize).expect("fd 落在 0..OPEN_MAX"),
            &mut filps,
        )
    };
    assert!(outcome.is_ok(), "借用的 fd 有效，关闭应成功");

    // 关闭语义（对照 C `open.c:699-710`）：fd 位清空、cloexec 位清空、
    // 引用计数递减到 0 时 filp 槽释放。
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
    // 被引用进程就是这张表里的 11 号槽：同一 fd 号上的表项原样保留。
    // 这一条是"关的是 VM 自己的 fd 表"的反证——若错关到消息里那个 endpoint，
    // 这里的 fd 会被清空。
    let peer = fproc.get(referenced_slot).expect("被引用槽存在");
    assert_eq!(
        peer.filps[VM_FD as usize],
        Some(peer_filp.get()),
        "被引用进程同一 fd 号的表项原样保留"
    );

    // 借来的 filp 只剩 VM 的 NEIGHBOUR_FD 一个引用；把它也关掉才是最后关闭，
    // 此时 filp 槽释放（C `filedes.c:496` 的 `--filp_count == 0` 分支）。
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

    // ── 第三步：应答回到 VM 并被对端解码器读懂 ──
    // C 在 `reqdone` 处填 VMV_ENDPOINT / VMV_RESULT / VMV_REQID 后以
    // `VM_VFS_REPLY` 异步发出（misc.c:487-494）。
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
// E5(b).2 错误路径：无效 fd 的应答仍是应答（携 errno 与请求号）
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

    // ── 无效 fd：消息合法但点名一个没有的 fd ──
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

    // C 打印一行诊断后照常回错误码（`misc.c:456-460`：`result != OK` 时
    // printf，随后 `reqdone` 处 `VMV_RESULT = result`），所以失败也是一次
    // 正常应答，且必须带着原请求号——VM 靠它把结果对回挂起的请求，
    // 否则失败会变成一次"应答丢失"的挂起。上线的服务器构建里 errno 值是负的
    // （`minix3/sys/sys/errno.h:187-190` 的 `_SIGN` 在 `_SYSTEM` 下展开为 `-`），
    // 故应答携带 `-EBADF`；Rust 的 `FdError::to_errno()` 给的是用户可见的正号
    // 编号（`minix-types/src/types/errno.rs:23`），上线前取负。
    let reply = vfs_reply_message(referenced_ep, -err.to_errno(), call.req_id);
    let decoded = VmVfsReplyIn::decode_message(&reply);
    assert_eq!(decoded.result, -EBADF, "VMV_RESULT 携负号 errno");
    assert_eq!(decoded.reqid as u32, REQ_ID, "错误路径同样按请求号回去");

    // 拒绝不产生副作用：点名的 fd 本来就是空的，在用的那个 fd 与它的 filp
    // 引用计数都必须原封不动——`close_fd` 的校验在校验之后才允许递减。
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
