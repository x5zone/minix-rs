//! VFS main loop and message dispatch.
//!
//! Corresponds to Minix3's main loop, message reception, and request dispatch mechanism in `main.c`.
//!
//! # Main Loop Model
//!
//! VFS main loop is message-driven, following the three-phase "receive request → process → reply" model:
//!
//! 1. `worker_yield()` — Let other threads run first.
//! 2. `send_work()` — Dispatch pending PM deferred requests.
//! 3. `get_work()` — Receive new messages.
//!
//! After receiving a message, dispatch based on message source:
//! - FS reply → `do_reply()`
//! - PM message → `service_pm()`
//! - Notification → Various notification handlers
//! - Device reply → `bdev_reply()/cdev_reply()/sdev_reply()`
//! - Normal syscall → `handle_work(do_work)`
//!
//! # Startup Chain
//!
//! `VfsState::init_fresh()` carries the boot sequence equivalent to
//! `sef_cb_init_fresh()` (main.c:393-499): fproc reset → VFS_PM_INIT
//! handshake → worker/device tables → root mount gate. The SEF
//! registration framework (sef_local_startup + sef_startup state machine)
//! is eliminated (design decision D1, see 01-vfs-init-main.md §3.1).
//!
//! # Difference from Kernel Main Loop
//!
//! - Kernel is single-core interrupt-driven—triggered by hardware interrupts.
//! - VFS is multi-threaded in Minix3—main thread receives messages, worker threads process.
//!   minix-rs models this as a single-threaded event loop with request-slot
//!   state machines (ARCH A-1, see 01-vfs-init-main.md §3.4).

extern crate alloc;

use crate::device_map::{DmapTable, SmapTable};
use crate::fcntl::LockTable;
use crate::filp::FilpTable;
use crate::fproc::{BlockedOn, FProcTable, FpFlags, PID_FREE};
use crate::fs_comm::{CommError, FsTransport, GlobalComm};
use crate::vnode::VnodeTable;
use crate::vmnt::VmntTable;
use minix_sef::SefEvent;
use minix_sys::ipc::IpcTransport as _;
use crate::worker::WorkerPool;
use minix_types::{Endpoint, Gid, Message, Uid, UserSlot, VfsPmInit, VfsPmInitError};

/// C: `const.h:16-17` — uid_t/gid_t for system processes and INIT.
const SYS_UID: Uid = 0;
/// C: `const.h:17` — gid_t for system processes and INIT.
const SYS_GID: Gid = 0;

/// PM message type.
///
/// Corresponds to Minix3's `VFS_PM_*` message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmMessageType {
    /// fork syscall.
    Fork,
    /// Server process fork.
    SrvFork,
    /// exec syscall.
    Exec,
    /// exit syscall.
    Exit,
    /// setuid call.
    Setuid,
    /// setgid call.
    Setgid,
    /// setsid call.
    Setsid,
    /// setgroups call.
    Setgroups,
    /// Core dump.
    Dumpcore,
    /// Unpause.
    Unpause,
    /// Reboot.
    Reboot,
    /// Unknown PM message.
    Unknown(i32),
}

/// Message dispatch result (legacy simple three-way).
///
/// New code should prefer [`Route`] which encodes the eight-way priority.
/// Kept for compatibility with 01-vfs-init-main's mock dispatch.
/// Eight-way dispatch route — the priority chain of `main:80-138`.
///
/// SEF live-update target states (`minix3/minix/include/minix/sef.h:213-217`),
/// the subset VFS's three LU callbacks distinguish.  `init_restart` is not
/// modeled: VFS restarts stateless, so RS re-runs `init_fresh` (design D1,
/// 01-vfs-init-main.md §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuState {
    /// `SEF_LU_STATE_NULL` — no update in progress (the rollback target).
    Null,
    /// `SEF_LU_STATE_REQUEST_FREE`.
    RequestFree,
    /// `SEF_LU_STATE_PROTOCOL_FREE`.
    ProtocolFree,
    /// `WORK_FREE`/`EVAL` and everything else — VFS refuses to prepare.
    Other,
}

/// `sef_cb_lu_prepare` (`main.c:303-322`): may we enter `state`?
///
/// Only request-free/protocol-free are preparable, and only when every
/// request slot is idle; anything else answers `ENOTREADY`.  C then runs
/// `worker_cleanup()` — under ARCH A-1 the slots are data, so "cleanup" is
/// exactly the idle state the gate just verified, and the rollback
/// re-creation (`sef_cb_lu_state_changed`, `main.c:325-340`) is a no-op by
/// construction.
pub fn lu_prepare(all_idle: bool, state: LuState) -> Result<(), UnblockError> {
    match state {
        LuState::RequestFree | LuState::ProtocolFree => {
            if all_idle {
                Ok(())
            } else {
                Err(UnblockError::NotReady)
            }
        }
        LuState::Null | LuState::Other => Err(UnblockError::NotReady),
    }
}

/// `sef_cb_lu_state_changed` (`main.c:325-340`): does a failed update back
/// to `Null` require re-creating the workers?  C answers yes when leaving a
/// request-free state; ARCH A-1 makes the re-creation itself a no-op (slots
/// are data), so this predicate only documents the C branch.
pub fn lu_rollback_needs_workers(old: LuState, now: LuState) -> bool {
    matches!(old, LuState::RequestFree | LuState::ProtocolFree)
        && now == LuState::Null
}

/// `sef_cb_init_lu` (`main.c:343-358`): does the new instance re-create
/// workers after the state transfer?  Same ARCH A-1 no-op as above.
pub fn init_lu_needs_workers(prepare_state: LuState) -> bool {
    matches!(prepare_state, LuState::RequestFree | LuState::ProtocolFree)
}

/// Order matters: `FsReply` (transid), then `Pm`, `Notify`, `TaskIgnored`,
/// then the device replies, then `Syscall`.  The variant order in this enum
/// matches the C `if/else if` short-circuit order so that
/// `route_message` can `match` exhaustively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// FS async reply: `TRNS_GET_ID(m_type)` is `IS_VFS_FS_TRANSID` → `do_reply`.
    FsReply { transid: u32, worker_slot: usize },
    /// PM control message: `who_e == PM_PROC_NR` → `service_pm`.
    Pm,
    /// Kernel notification: `is_notify(call_nr)` → `DS/KERNEL/CLOCK`.
    Notify { source: Endpoint, call_nr: i32 },
    /// Task message with `who_p < 0` → ignored (tasks must `notify`).
    TaskIgnored { source: Endpoint },
    /// Block-device reply: `IS_BDEV_RS(call_nr)` → `bdev_reply`.
    Bdev,
    /// Char-device reply: `IS_CDEV_RS(call_nr)` → `cdev_reply`.
    Cdev,
    /// Socket-driver reply: `IS_SDEV_RS(call_nr)` → `sdev_reply`.
    Sdev,
    /// Normal syscall: `handle_work(do_work)` → `call_vec` dispatch.
    Syscall { call: crate::call_table::VfsCallNum },
    /// Unresolvable raw past the `VFS_BASE` gate — C answers `ENOSYS`
    /// (`main.c:283-294`, `call_index >= NR_VFS_CALLS`).  Never a silent
    /// stand-in for a real call.
    Enosys { raw: u32 },
}

/// `do_reply` 的两类 `printf+return` 软失败（main.c:193-203）——回复
/// 不落地、窗口不动、主循环继续。硬失败（找不到 vmnt）按 C panic。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsReplyError {
    /// 低 16 位不是合法 `IS_VFS_FS_TRANSID`。
    SpuriousTransid,
    /// 槽号越界（>`NR_WTHREADS`）。
    SlotOutOfRange,
    /// `wp->w_task != who_e`——不是该端点在等的回复（main.c:193-196）。
    WrongTask,
}

/// Notify source inside [`Route::Notify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    Ds,
    Kernel,
    Clock { timestamp: u32 },
    Other { endpoint: Endpoint },
}

/// `TRNS_GET_ID` / `VFS_TRANSID` codec — canonical definition lives in
/// `fs_comm.rs` (the protocol owner); re-exported here so the route layer
/// and its tests share one contract (P2-6 convergence, `ARCH A-4`).
pub use crate::fs_comm::{TransIdCodec, VfsTransIdCodec};
#[cfg(test)]
pub use crate::fs_comm::TestTransIdCodec;

/// VFS startup phase.
///
/// Mirrors the internal sequence of `sef_cb_init_fresh()` (main.c:393-499)
/// plus `do_init_root()` (main.c:501-527). Making the phase explicit turns
/// "which facilities are already available" into checkable state instead of
/// an implicit ordering (design decision D3, 01-vfs-init-main.md §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    /// VFS_PM_INIT handshake in progress (main.c:410-436).
    PmHandshake,
    /// Handshake done; core tables being initialized (main.c:438-489).
    InitTables,
    /// Root mount in progress; requests gated off (main.c:501-527).
    Mounting,
    /// Fully booted; requests accepted.
    Running,
}

/// Reply intent for a dispatched request.
///
/// Corresponds to Minix3's `SUSPEND` return convention: `Reply(i32)` is
/// `reply(endpoint, code)`, `ReplyLater` is C `SUSPEND` (later `revive` or
/// driver `*_reply` replies), `NoReply` is the `PID_FREE` drop.
///
/// `ARCH A-5` (plan.md): the `SUSPEND` sentinel is typed here; the three
/// revival paths (`pipe.c:revive`, `select.c:select_return`, `cdev/sdev`)
/// are DEFERRED to 17/23/21/22.  Until then the contract is declared but
/// not consumed — fail-closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyIntent {
    /// Reply immediately with the given status.
    Reply(i32),
    /// C `SUSPEND`: do not reply now; a later revive path replies.
    ReplyLater,
    /// No reply is expected.
    NoReply,
}

/// Outcome of `unblock` — the two branches of `main.c:965-973`.
///
/// `Pipe` suspends cannot be replayed as-is and need `do_pending_pipe` in a
/// worker; `Flock` is replayed as the original `VFS_FCNTL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnblockOutcome {
    /// `FP_BLOCKED_ON_PIPE` → `worker_start(..., do_pending_pipe)` → `FALSE` (keep polling).
    QueuedPipe { slot: UserSlot },
    /// `FP_BLOCKED_ON_FLOCK` → `fp = rfp` → `TRUE` (replay as `VFS_FCNTL`).
    RevivedLock { slot: UserSlot },
}

/// Error from `unblock`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnblockError {
    NotBlocked,
    UnknownBlockedOn(u8),
    SlotFree,
    /// SEF live-update cannot enter the requested state right now —
    /// `ENOTREADY` (`main.c:312/321` break-then-return).
    NotReady,
}

impl minix_types::ToErrno for UnblockError {
    fn to_errno(&self) -> minix_types::Errno {
        minix_types::Errno::from_i32((*self).to_errno())
    }
}

impl UnblockError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::NotBlocked => minix_types::EINVAL,
            Self::UnknownBlockedOn(_) => minix_types::EINVAL,
            Self::SlotFree => minix_types::ESRCH,
            Self::NotReady => minix_types::ENOTREADY,
        }
    }
}

/// Poll result for `VfsState::poll_next` — models `get_work:580`'s
/// `TRUE`/`FALSE` dual return plus the `reviving` fast-path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollResult {
    /// `get_work` returned `TRUE` with `m_in` ready.
    Ready,
    /// `reviving != 0` and `unblock` queued a pipe job → `main:77 continue`.
    RevivedPipe { slot: UserSlot },
    /// `reviving != 0` and `unblock` revived a lock → re-execute request.
    RevivedLock { slot: UserSlot },
}

/// VFS server state.
///
/// Aggregates all VFS subsystem states (ARCH A-4: glo.h globals → VfsState).
pub struct VfsState {
    /// Process table.
    pub fproc_table: FProcTable,
    /// Open-descriptor pool (`filp[1024]`, `file.h:33`).
    pub filp_table: FilpTable,
    /// Vnode table (`vnode[]`, `vnode.h`).
    pub vnode_table: VnodeTable,
    /// Mount table (`vmnt[8]`, `vmnt.h`).
    pub vmnt_table: VmntTable,
    /// Device↔driver table (`dmap[NR_DEVICES]`, `dmap.h`).
    pub dmap_table: DmapTable,
    /// Socket-driver table (`smap[NR_SOCKDEVS]`, `smap.h`).
    pub smap_table: SmapTable,
    /// POSIX record-lock table (`file_lock[NR_LOCKS]`, `fcntl.h`).
    pub lock_table: LockTable,
    /// Worker thread pool.
    pub worker_pool: WorkerPool,
    /// FS/VM/驱动通信窗口（`glo.h:17 sending` + 各 `vmnt.m_comm`,
    /// comm.c;S12 W1 入态）。
    pub comm: GlobalComm,
    /// Revive counter (number of blocked processes revived).
    pub reviving: usize,
    /// Current message.
    pub current_message: Message,
    /// Current process's fproc slot.
    pub current_fp_slot: Option<UserSlot>,
    /// Startup phase (see [`BootPhase`]).
    pub boot_phase: BootPhase,
    /// Whether new requests may be assigned to worker slots.
    ///
    /// Corresponds to `worker_allow()` (worker.c:155-183): `block_all = !allow`.
    /// Requests arriving while gated are marked `FP_PENDING` instead of
    /// being processed. The pending-drain loop is 08/09 territory.
    pub accept_requests: bool,
    /// Number of requests marked pending while the gate was closed.
    ///
    /// Corresponds to the `pending` global in worker.c.
    pub pending: usize,
    /// Whether boot completed (`finish_init()` ran).
    pub initialized: bool,
    /// 本进程的 grant 表（C 的 `grants[]` 全局数组，safecopies.c）。
    /// 启动段经 `register`（C 的 `sys_setgrant`）把表位置告知内核；
    /// 对话臂用它把**用户缓冲**授权给 FS 读写（`grant_user_buffer`）。
    pub grants: minix_sys::grant::GrantTable,
    /// 当前分发的作业绑在哪个 worker 槽（C 的 `self`／`fp_func` 背后的
    /// 槽指针；臂用它登记续接、发送 FS 请求）。`None` = 未绑（非 syscall
    /// 路径或已释放）。
    pub current_worker: Option<usize>,
    /// 臂登记、待主循环发出的 FS 对话（单线程模型：臂只碰状态，I/O 归
    /// 循环——C 的 `fs_sendrec` 在同一处既登记又发送，这里拆成两半）。
    pub pending_fs: Option<PendingFs>,
    /// 待发送的回复（W3 回复半）：`run_once` 分发完把结果折成
    /// `(调用方, 回复消息)` 入队，`run()` 在每轮循环尾发出（C `reply(who_e,
    /// result)` 的位置，`job_m_out` 就是那条消息）。多数回复只有 `m_type`
    /// （状态字）；带载荷的（如 `lseek` 的新位置）由 `queue_reply_msg` 入队。
    /// `Suspend` 的臂不入队——它们的回复在 FS 应答落地时发。
    pub pending_reply: Option<(Endpoint, Message)>,
}

impl VfsState {
    /// Creates new VFS state.
    pub fn new() -> Self {
        Self {
            fproc_table: FProcTable::new(),
            filp_table: FilpTable::new(),
            vnode_table: VnodeTable::new(),
            vmnt_table: VmntTable::new(),
            // C init_dmap(dmap.c:230-247):清零全 NONE 后唯一显式
            // 映射 CTTY_MAJOR ← "vfs"(CTTY_ENDPT)——W5/S13 接线。
            dmap_table: DmapTable::init(),
            smap_table: SmapTable::new(),
            lock_table: LockTable::new(),
            worker_pool: WorkerPool::new(),
            comm: GlobalComm::new(),
            reviving: 0,
            current_message: Message::default(),
            current_fp_slot: None,
            boot_phase: BootPhase::PmHandshake,
            // C: `block_all` 为 BSS 全局（glo.h），初始为 0（允许）。
            accept_requests: true,
            pending: 0,
            initialized: false,
            pending_reply: None,
            grants: minix_sys::grant::GrantTable::new(),
            current_worker: None,
            pending_fs: None,
        }
    }

    /// SEF initialization—fresh start.
    ///
    /// Corresponds to Minix3's `sef_cb_init_fresh()` (main.c:393-499).
    ///
    /// Phase 1 (main.c:405-408): reset every fproc slot to the unused state
    /// (`fp_endpoint = NONE; fp_pid = PID_FREE`). Phase 2, the VFS_PM_INIT
    /// handshake (main.c:410-436), is driven by [`Self::pm_handshake_step`]
    /// because it is message-driven; the post-handshake sequence is
    /// [`Self::finish_init`].
    pub fn init_fresh(&mut self) {
        assert!(!self.initialized, "init_fresh called on an initialized VFS");
        self.boot_phase = BootPhase::PmHandshake;
        // main.c:405-408 — fproc 槽清零（BSS 全局在 Rust 中即"构造即空"；
        // 显式重置保留，使 restart/LU 路径（DEFERRED）语义诚实）。
        self.fproc_table.reset_all();
    }

    /// Processes one VFS_PM_INIT handshake message (main.c:416-436).
    ///
    /// Each PM message fills one fproc slot (`slot`/`pid`/`endpoint`, plus the
    /// boot-time credentials main.c:419-425). The terminal message with
    /// `endpoint = NONE` completes the handshake and returns `Ok(true)`.
    ///
    /// # Return
    /// - `Ok(false)` — slot filled, more messages expected.
    /// - `Ok(true)` — NONE terminator seen; phase advanced to `InitTables`.
    /// - `Err(_)` — malformed message (fail-closed, never index out of range).
    pub fn pm_handshake_step(&mut self, msg: &Message) -> Result<bool, VfsPmInitError> {
        assert_eq!(
            self.boot_phase,
            BootPhase::PmHandshake,
            "pm_handshake_step outside handshake phase"
        );

        let init = VfsPmInit::decode(msg)?;

        // main.c:428-431 — 终止：endpoint = NONE 表示没有更多系统进程。
        if init.endpoint == Endpoint::NONE {
            // main.c:435-436 — ipc_send(PM_PROC_NR, OK) 同步屏障（内核 IPC
            // 未落地，DEFERRED；阶段推进即等价握手完成）。
            self.boot_phase = BootPhase::InitTables;
            return Ok(true);
        }

        // 槽号范围已由 decode 校验（fail-closed，等价 C 越界数组访问改为显式失败）。
        let slot = UserSlot::new(init.slot as usize);
        let fp = self
            .fproc_table
            .get_mut(slot)
            .ok_or(VfsPmInitError::SlotOutOfRange(init.slot))?;

        // main.c:419-425 — 填充 boot 进程槽位。
        fp.flags = FpFlags::NOFLAGS;
        fp.pid = init.pid;
        fp.endpoint = init.endpoint;
        fp.blocked_on = BlockedOn::None;
        fp.real_uid = SYS_UID;
        fp.eff_uid = SYS_UID;
        fp.real_gid = SYS_GID;
        fp.eff_gid = SYS_GID;
        fp.umask = !0;

        Ok(false)
    }

    /// Completes the post-handshake boot sequence (main.c:438-497).
    ///
    /// Requires the handshake to have terminated (phase `InitTables`).
    pub fn finish_init(&mut self) {
        assert_eq!(
            self.boot_phase,
            BootPhase::InitTables,
            "finish_init requires completed VFS_PM_INIT handshake"
        );

        // main.c:438 — system_hz = sys_hz()（内核 IPC 未落地，DEFERRED，归 99）。
        // main.c:441 — ds_subscribe("drv\\.[bc]..\\..*", DSF_INITIAL | DSF_OVERWRITE)
        //              （ARCH A-14 未实现，DEFERRED，归 19/24）。
        // main.c:445 — worker_init()：WorkerPool 构造即就绪（NR_WTHREADS=9，归 08）。
        // main.c:448 — bsf_lock：单线程事件循环下锁原语降级（归 07）。
        // main.c:451-453 — init_dmap()/init_smap():init_dmap 已随
        // VfsState::new 的 DmapTable::init 接线(CTTY 槽,S13 W5);
        // init_smap ≡ SmapTable::new(全空基编号)。
        // main.c:455-467 — sys_safecopyfrom(RS_PROC_NR, rproctab) + map_service()
        //                  （归 19，DEFERRED，依赖 sys_safecopyfrom 内核原语）。

        // main.c:468-483 — fp_lock（槽位锁，单线程下归 07）+ filp/rd/wd 清零。
        self.fproc_table.init_phase2();

        // main.c:485-489 — init_vnodes()/init_vmnts()/init_select()/init_filps()
        //                  （表结构归 04~06，DEFERRED）。

        // main.c:492-497 — worker_start(fproc_addr(VFS_PROC_NR), do_init_root, ...)。
        self.do_init_root();

        assert_eq!(self.boot_phase, BootPhase::Running);
        self.initialized = true;
    }

    /// Root mount sequence (main.c:501-527).
    ///
    /// Establishes the worker gate contract: requests are refused while the
    /// root file system is being mounted, then re-enabled. The actual
    /// `mount_pfs()`/`mount_fs()` IPC is DEFERRED to 18-mount.
    pub fn do_init_root(&mut self) {
        assert_eq!(
            self.boot_phase,
            BootPhase::InitTables,
            "do_init_root requires post-handshake phase"
        );
        self.boot_phase = BootPhase::Mounting;

        // main.c:503 — worker_allow(FALSE)：挂载期间拒绝新请求（含 init(8)）。
        self.set_accept_requests(false);

        // main.c:505 — mount_pfs()（执行编排归 18，决策件 mount.rs 已备）。
        // main.c:508-518 — mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR,
        //                   0, "mfs", "fs_imgrd")：req_readsuper 往返件已备
        //                   (S14:FsSuperblock 经 FsClient 的
        //                   send_with_retry,SuperInfo.con_reqs 与
        //                   max_reqs 窗口规则 mount.c:307-312);挂启动段
        //                   执行归通电面。

        // main.c:525 — worker_allow(TRUE)：根文件系统就绪，恢复接受请求。
        self.set_accept_requests(true);

        self.boot_phase = BootPhase::Running;
    }

    /// Sets whether new requests may be assigned to worker slots.
    ///
    /// Corresponds to `worker_allow()` (worker.c:155-183): `block_all = !allow`.
    /// While disallowed, incoming user requests are marked `FP_PENDING`
    /// (worker.c:169-178) instead of being processed. The pending-drain loop
    /// on re-enable is 08/09 territory.
    pub fn set_accept_requests(&mut self, accept: bool) {
        self.accept_requests = accept;
    }

    /// `sef_cb_lu_prepare` against this instance's slot pool
    /// (`main.c:303-322`; `all_idle` is the `worker_idle()` analogue).
    pub fn lu_prepare(&mut self, state: LuState) -> Result<(), UnblockError> {
        lu_prepare(self.worker_pool.all_idle(), state)
    }

    /// Marks a request pending (worker.c:169-178).
    ///
    /// Sets `FP_PENDING` on the target slot and bumps the pending counter.
    /// No-op if the slot is out of range or already pending.
    fn mark_request_pending(&mut self, slot: UserSlot) {
        if let Some(fp) = self.fproc_table.get_mut(slot)
            && !fp.flags.contains(FpFlags::PENDING)
        {
            fp.flags |= FpFlags::PENDING;
            self.pending += 1;
        }
    }

    /// Gets current fproc's endpoint.
    pub fn current_endpoint(&self) -> Endpoint {
        self.current_message.m_source
    }

    /// Checks if message is from PM.
    pub fn is_from_pm(&self) -> bool {
        self.current_endpoint() == Endpoint::PM
    }

    /// Handles PM fork message.
    ///
    /// Corresponds to Minix3's `VFS_PM_FORK` branch in `service_pm()`.
    pub fn handle_pm_fork(
        &mut self,
        parent_ep: Endpoint,
        child_ep: Endpoint,
        child_pid: minix_types::Pid,
    ) -> Result<(), &'static str> {
        let _parent_slot = parent_ep.to_user_slot().ok_or("Invalid parent endpoint")?;
        let child_slot = child_ep.to_user_slot().ok_or("Invalid child endpoint")?;

        let child_fp = self
            .fproc_table
            .get_mut(child_slot)
            .ok_or("Child slot out of range")?;

        if child_fp.pid != PID_FREE {
            return Err("Child slot is not free");
        }

        child_fp.pid = child_pid;
        child_fp.endpoint = child_ep;
        child_fp.flags = FpFlags::NOFLAGS;

        Ok(())
    }

    // -----------------------------------------------------------------
    // 09-main-loop: typed routing, transid, revive, reply
    // -----------------------------------------------------------------

    /// Whether `raw_m_type` is a VFS-call (`IS_VFS_CALL`).
    ///
    /// `callnr.h:70` `((type & ~0xff)==VFS_BASE)`.
    pub fn is_vfs_call(raw: u32) -> bool {
        const VFS_BASE: u32 = 0x100;
        (raw & !0xff) == VFS_BASE
    }

    /// Whether `raw_m_type` is a device RS reply prefix.
    ///
    /// `com.h:923/967/1041` `IS_BDEV/CDEV/SDEV_RS`.
    pub fn is_bdev_rs(raw: u32) -> bool {
        (raw & !0x7f) == 0x580 // `BDEV_RS_BASE` (com.h:963-964)
    }
    pub fn is_cdev_rs(raw: u32) -> bool {
        (raw & !0x7f) == 0x480 // `CDEV_RS_BASE` (com.h:919-920)
    }
    pub fn is_sdev_rs(raw: u32) -> bool {
        (raw & !0x7f) == 0x1980 // `SDEV_RS_BASE` (com.h:1038)
    }

    /// Whether `raw` is a `NOTIFY` (`com.h:93` `(a-NOTIFY)<0x100`).
    pub fn is_notify(raw: i32) -> bool {
        const NOTIFY_MESSAGE: i32 = 0x1000; // `NOTIFY_MESSAGE` in com.h
        ((raw - NOTIFY_MESSAGE) as u32) < 0x100
    }

    /// Priority route — the eight-way short-circuit of `main:80-138`.
    ///
    /// Unlike the legacy [`Self::dispatch`] three-way, this encodes the full
    /// priority: `FsReply > Pm > Notify > TaskIgnored > Bdev/Cdev/Sdev > Syscall`.
    /// The `transid` path uses `C: TransIdCodec` so the codec is testable
    /// (Gate D).
    pub fn route_message<C: TransIdCodec>(&self, msg: &Message, codec: &C) -> Route {
        let m_type = msg.m_type as u32;
        let src = msg.m_source;

        // 1. FS reply via transid — `TRNS_GET_ID` + `IS_VFS_FS_TRANSID`.
        let transid_raw = m_type & 0xFFFF;
        // `decode` 自带 `is_fs_transid` 守门——两层条件在此合并为一层。
        if let Some(slot) = codec.decode(transid_raw) {
            return Route::FsReply {
                transid: transid_raw,
                worker_slot: slot,
            };
        }

        // 2. PM control — `who_e == PM_PROC_NR`.
        if src == Endpoint::PM {
            return Route::Pm;
        }

        // 3. Notify — `is_notify(call_nr)` → DS/KERNEL/CLOCK.
        if Self::is_notify(msg.m_type) {
            return Route::Notify {
                source: src,
                call_nr: msg.m_type,
            };
        }

        // 4. Task ignore — `who_p < 0` (tasks must `notify`).
        if src.to_user_slot().is_none() {
            // `KERNEL` (-1), `CLOCK` (-3) are already handled as Notify above;
            // remaining task endpoints fall here.
            return Route::TaskIgnored { source: src };
        }

        // 5-7. Driver RS replies.
        if Self::is_bdev_rs(m_type) {
            return Route::Bdev;
        }
        if Self::is_cdev_rs(m_type) {
            return Route::Cdev;
        }
        if Self::is_sdev_rs(m_type) {
            return Route::Sdev;
        }

        // 8. Normal syscall — `handle_work(do_work)` → `call_vec`.  An
        // unresolvable call number answers `ENOSYS` (main.c:283-294); it
        // must never masquerade as a real call.
        match crate::call_table::VfsCallNum::from_raw(m_type) {
            Some(call) => Route::Syscall { call },
            None => Route::Enosys { raw: m_type },
        }
    }

    /// `run_once`——主循环单轮入口：接收消息 → 路由 → 门控 → 分发。
    ///
    /// 门语义（legacy dispatch 迁入）：`accept_requests == false` 时用户
    /// 请求标 `FP_PENDING` 并计数（去重），不处理；`Pm` 控制面短路的
    /// 优先序由 `route_message` 保持。
    pub fn run_once<C: TransIdCodec>(&mut self, msg: &Message, codec: &C) -> Route {
        self.current_message = *msg;
        self.current_fp_slot = msg.m_source.to_user_slot();
        let route = self.route_message(msg, codec);
        match route {
            Route::Syscall { call } => {
                if self.accept_requests {
                    // C main.c:146-160 —— 作业先绑 worker 槽
                    // （`worker_start(rfp, use_spare=TRUE)`）；没有空槽即
                    // `EAGAIN`（`worker_available()==0` 的分支）。
                    let assigned = self.current_fp_slot.and_then(|fs| {
                        self.worker_pool.assign_first_fit(
                            fs,
                            crate::worker::WorkerFunc::DoWork,
                            msg,
                        )
                    });
                    match assigned {
                        Some(idx) => {
                            self.current_worker = Some(idx);
                            let result = crate::syscalls::dispatch_syscall(self, call);
                            // 臂没挂起（`Suspend` 之外）= 作业完结：释放槽
                            // （C `worker_main` 尾部的 `worker_release`）；
                            // 挂起的臂自己登记续接，槽留给续接体释放。
                            if !matches!(result, crate::call_table::SyscallResult::Suspend) {
                                self.worker_pool.release(idx);
                                self.current_worker = None;
                            }
                            self.queue_reply(msg.m_source, result);
                        }
                        None => {
                            // C handle_work:150-151 —— 无槽即 `EAGAIN`。
                            self.queue_reply(
                                msg.m_source,
                                crate::call_table::SyscallResult::Error(minix_types::EAGAIN),
                            );
                        }
                    }
                } else if let Some(slot) = self.current_fp_slot {
                    self.mark_request_pending(slot);
                }
            }
            Route::Enosys { .. } => {
                // C main.c:283-294——不可解析的调用号回 ENOSYS 而不是装成
                // 真调用（W3 回复半：发送在 `run()` 的循环尾）。
                self.queue_reply(msg.m_source, crate::call_table::SyscallResult::Nosys);
            }
            Route::FsReply { .. } => {
                // C main.c:80-89 — do_reply 无应答对象；软失败（typed
                // `FsReplyError`）即 C 的 printf+return，主循环继续。
                let _ = self.handle_fs_reply(msg, codec);
            }
            _ => {}
        }
        // 续接驱动点：本轮（或上一轮）的 FS 回复若已落在某个槽上，就跑该
        // 作业的续接体（C 的"线程从 `worker_wait` 返回后继续"在单线程模型
        // 里的对应物）。放在路由之后，保证同一轮里"收回复 → 完成作业"闭
        // 环。
        self.run_worker_continuations();
        route
    }

    /// `common_open` 的**本地半**（C `open.c:118-274` 里不碰 FS 的那些步）：
    /// 类型分派 → fd/filp 装配 → 回 fd（成功时 syscall 结果就是 fd）。
    /// `Open` 与 `Creat` 两条臂共用（后者走完 create 之后落到这里）。
    pub fn finish_open_local(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        node: &crate::path::NodeDetails,
        oflags: u32,
    ) {
        self.finish_open_local_inner(idx, fp_slot, node, oflags, false);
    }

    /// [`Self::finish_open_local`] 的本体；`trunc_done` 表示"`O_TRUNC` 的
    /// 截断已经做过"（`O_TRUNC` 分支的续接体走这条），于是分派时把该位当
    /// 已消费——否则 `dispatch_open` 会再判一次 `NeedTruncate` 而成环。
    /// **`filp_flags` 仍写原始 `oflags`**：C `open.c:136` 是
    /// `filp->filp_flags = oflags`（含 `O_TRUNC`），这个字随后随读写请求
    /// 原样传给 FS（`read.c` 的 `REQ_FLAGS`），不是本地私有的。
    fn finish_open_local_inner(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        node: &crate::path::NodeDetails,
        oflags: u32,
        trunc_done: bool,
    ) {
        let access = match crate::open::OpenFlags::from_bits(oflags)
            .and_then(|f| f.access().ok())
        {
            Some(a) => a,
            None => {
                self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                return;
            }
        };
        let bits: crate::open::AccessBits = access.into();
        let ft = crate::open::FileType::from(node.mode);
        let mut dispatch_flags = crate::open::OpenFlags::from_bits_truncate(oflags);
        if trunc_done {
            dispatch_flags.remove(crate::open::OpenFlags::TRUNC);
        }
        match crate::open::dispatch_open(ft, bits, dispatch_flags) {
            crate::open::OpenOutcome::Proceed => {
                let vnode_idx = match self.intern_vnode(node) {
                    Some(i) => i,
                    None => {
                        self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                        return;
                    }
                };
                let Some(slot) = fp_slot else {
                    self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                    return;
                };
                let (fd, filp_id) = {
                    let Some(fp) = self.fproc_table.get_mut(slot) else {
                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                        return;
                    };
                    match crate::filedes::get_fd(
                        fp,
                        0,
                        &crate::filedes::LowestFree,
                        &mut self.filp_table,
                        bits.bits(),
                    ) {
                        Ok(pair) => pair,
                        Err(_) => {
                            self.finish_worker_job(idx, fp_slot, minix_types::EMFILE);
                            return;
                        }
                    }
                };
                if let Some(f) = self.filp_table.get_mut(filp_id) {
                    f.vnode = Some(vnode_idx);
                    f.flags = oflags as i32;
                }
                self.filp_table.inc_count(filp_id);
                if let Some(fp) = self.fproc_table.get_mut(slot) {
                    fp.filps[fd.get()] = Some(filp_id.get());
                    if oflags & crate::open::OpenFlags::CLOEXEC.bits() != 0 {
                        fp.cloexec_set.set(fd.get(), true);
                    }
                }
                self.finish_worker_job(idx, fp_slot, fd.get() as i32);
            }
            crate::open::OpenOutcome::Reject(e) => {
                self.finish_worker_job(idx, fp_slot, e.to_errno());
            }
            crate::open::OpenOutcome::NeedTruncate => {
                // C `common_open:150-157`：常规文件 + `O_TRUNC` → W 位门 →
                // `truncate_vnode(vp, 0)`（**结果忽略**）→ 照常装配。
                let (real_uid, eff_uid, real_gid, eff_gid, supp) = match fp_slot
                    .and_then(|s| self.fproc_table.get(s))
                {
                    Some(fp) => (
                        fp.real_uid,
                        fp.eff_uid,
                        fp.real_gid,
                        fp.eff_gid,
                        fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                    ),
                    None => {
                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                        return;
                    }
                };
                let readonly_fs = self
                    .vmnt_table
                    .find_by_fs(node.fs_e)
                    .and_then(|v| self.vmnt_table.get(v))
                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                    .unwrap_or(false);
                let forbid = crate::protect::forbidden_decision(&crate::protect::ForbidInput {
                    real_uid,
                    real_gid,
                    eff_uid,
                    eff_gid,
                    is_access_call: false,
                    file_uid: node.uid,
                    file_gid: node.gid,
                    mode: node.mode,
                    access: crate::open::W_BIT as u8,
                    is_dir: false,
                    supp: &supp,
                    readonly_fs,
                });
                if let Err(e) = forbid {
                    // C 的 `break`：带该错误结束 open。
                    self.finish_worker_job(idx, fp_slot, e.to_errno());
                    return;
                }
                let vmnt = match self.vmnt_table.find_by_fs(node.fs_e) {
                    Some(v) => v.0,
                    None => {
                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                        return;
                    }
                };
                let user = fp_slot
                    .and_then(|s| self.fproc_table.get(s))
                    .map(|fp| fp.endpoint)
                    .unwrap_or(Endpoint::NONE);
                if let Some(wp) = self.worker_pool.get_mut(idx) {
                    wp.cont = Some(crate::worker::WorkerCont::OpenTrunc {
                        node: *node,
                        oflags,
                    });
                }
                self.pending_fs = Some(PendingFs {
                    vmnt,
                    fs_e: node.fs_e,
                    worker: idx,
                    grant: 0, // 无数据面
                    user,
                    // C `truncate_vnode(vp, 0)` → `req_ftrunc(fs_e, ino, 0, 0)`。
                    req: crate::request::encode_ftrunc(node.ino, 0, 0),
                });
            }
            // 设备 open（19-22）与 FIFO 配对：本批未接线，诚实拒绝。
            _ => self.finish_worker_job(idx, fp_slot, minix_types::ENOSYS),
        }
    }

    /// C `advance`（path.c:60-127）的收尾：把走完的节点并进 vnode 表——
    /// `find_by_ino` 命中则抬 FS 引用 + `dup`，否则填草稿槽并置
    /// `fs_count = ref_count = 1`。返回表下标（走完之后的臂要拿它当
    /// `filp_vno`）。
    pub fn intern_vnode(&mut self, node: &crate::path::NodeDetails) -> Option<usize> {
        if let Some(hit) = self.vnode_table.find_by_ino(node.fs_e, node.ino) {
            if let Some(v) = self.vnode_table.get_mut(hit) {
                v.fs_count += 1;
            }
            self.vnode_table.dup(hit);
            return Some(hit.0);
        }
        let scratch = self.vnode_table.alloc().ok()?;
        if let Some(v) = self.vnode_table.get_mut(scratch) {
            v.fs = node.fs_e;
            v.ino = node.ino;
            v.mode = node.mode;
            v.size = node.size;
            v.uid = node.uid;
            v.gid = node.gid;
            v.dev = node.dev;
            v.fs_count = 1;
            v.ref_count = 1;
        }
        Some(scratch.0)
    }

    /// 调用方的根目录三元组（C `fp_rd` → vnode → `(fs, ino, dev)`）。
    ///
    /// 路径起点选择（root vs work dir）在臂侧（`eat_path` 的首字符判断），
    /// 这里只做 vnode → 三元组的解引用；槽空/vnode 缺失回零三元组，
    /// `LookupWalk` 的 chroot 边界判定自然失效（与 C 的 `fp_rd == NULL`
    /// 在未初始化进程上的效果一致）。
    pub fn root_dir_of(&self, fp_slot: Option<minix_types::UserSlot>) -> crate::path::RootDir {
        let vnode = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .and_then(|fp| fp.root_dir)
            .and_then(|idx| self.vnode_table.get(crate::vnode::VnodeId(idx)));
        match vnode {
            Some(v) => crate::path::RootDir { ino: v.ino, fs: v.fs, dev: v.dev },
            None => crate::path::RootDir { ino: 0, fs: Endpoint::NONE, dev: 0 },
        }
    }

    /// 调用方的工作目录三元组（C `fp_wd`）。
    pub fn work_dir_of(&self, fp_slot: minix_types::UserSlot) -> crate::path::RootDir {
        let vnode = self
            .fproc_table
            .get(fp_slot)
            .and_then(|fp| fp.work_dir)
            .and_then(|idx| self.vnode_table.get(crate::vnode::VnodeId(idx)));
        match vnode {
            Some(v) => crate::path::RootDir { ino: v.ino, fs: v.fs, dev: v.dev },
            None => crate::path::RootDir { ino: 0, fs: Endpoint::NONE, dev: 0 },
        }
    }

    /// 当前挂载表（`path::lookup` 的 `mounts` 参数）——从 `vmnt_table` +
    /// 各挂载点的根 vnode 组装。空行（`fs == NONE`）不进表。
    pub fn mounted_fs_list(&self) -> alloc::vec::Vec<crate::path::MountedFs> {
        let mut out = alloc::vec::Vec::new();
        for idx in 0..crate::vmnt::NR_MNTS {
            let Some(v) = self.vmnt_table.get(crate::vmnt::VmntId(idx)) else {
                continue;
            };
            if v.fs == Endpoint::NONE {
                continue;
            }
            // 根 vnode（`m_root_node` 的 (ino, dev)）与挂载点 vnode
            // （`m_mounted_on` 的 (ino, fs, dev)）。
            let root_vn = v
                .root
                .and_then(|i| self.vnode_table.get(crate::vnode::VnodeId(i)));
            let (root_ino, root_dev) = root_vn.map_or((0, v.dev), |rv| (rv.ino, rv.dev));
            let mounted_on = v.mounted_on.and_then(|i| {
                self.vnode_table
                    .get(crate::vnode::VnodeId(i))
                    .map(|mv| (mv.ino, mv.fs, mv.dev))
            });
            out.push(crate::path::MountedFs {
                fs: v.fs,
                dev: v.dev,
                root: (root_ino, root_dev),
                mounted_on,
            });
        }
        out
    }

    /// 把槽内的路径游标装进 scratch（+ NUL）、授权、组 `REQ_LOOKUP` 并交给
    /// 循环发送（与臂的首条 lookup 共用同一条构造路径）。
    pub fn send_lookup_for_slot(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        fs_e: Endpoint,
        dir_ino: u64,
        root_ino: u64,
    ) -> Result<(), i32> {
        let vmnt = self.vmnt_table.find_by_fs(fs_e).ok_or(minix_types::EIO)?.0;
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        let (grant, path_len, flags) = {
            let wp = self.worker_pool.get_mut(idx).ok_or(minix_types::EIO)?;
            let walk_path = wp
                .path
                .as_ref()
                .map(|p| p.walk.resolve().path.clone())
                .unwrap_or_default();
            // `PATH_RET_SYMLINK` 等位要原样发给 FS（语义在 FS 侧，
            // libfsdriver/lookup.c:249-251）——漏发等于让 FS 跟进末组件符号链接。
            let flags = wp
                .path
                .as_ref()
                .map(|p| p.walk.resolve().flags.bits())
                .unwrap_or(0);
            let bytes = walk_path.as_bytes();
            let n = bytes.len().min(crate::path::PATH_MAX - 1);
            wp.path_scratch[..n].copy_from_slice(&bytes[..n]);
            wp.path_scratch[n] = 0;
            let addr = wp.path_scratch.as_ptr() as u64;
            let len = n + 1;
            let grant = self
                .grants
                .grant_direct(
                    &minix_sys::syscall::DirectKernelCallTransport,
                    fs_e.get(),
                    addr,
                    len as u64,
                    minix_types::CpFlags::READ,
                )
                .map_err(|_| minix_types::EIO)?;
            (grant, len, flags)
        };
        // 现场里记录新 grant（回复后 revoke）。
        if let Some(wp) = self.worker_pool.get_mut(idx)
            && let Some(p) = wp.path.as_mut()
        {
            p.grant = grant;
        }
        self.pending_fs = Some(PendingFs {
            vmnt,
            fs_e,
            worker: idx,
            grant,
            user,
            req: crate::request::encode_lookup(grant, path_len, dir_ino, root_ino, flags),
        });
        Ok(())
    }

    /// 发一条 `REQ_UNLINK`/`REQ_RMDIR`（C `req_unlink`/`req_rmdir`，
    /// request.c:1149-1175 / 966-989）：组件名写进槽内 scratch 并做
    /// **direct grant**（名字在 VFS 内存里，不是 magic grant），父目录 ino
    /// 与名字长度随请求带上；回复只有状态，所以续接标识是 `Status`。
    pub fn send_unlink_for_slot(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        fs_e: Endpoint,
        dir_ino: u64,
        entry: &str,
        rmdir: bool,
    ) -> Result<(), i32> {
        let vmnt = self.vmnt_table.find_by_fs(fs_e).ok_or(minix_types::EIO)?.0;
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        let (grant, name_len) = {
            let wp = self.worker_pool.get_mut(idx).ok_or(minix_types::EIO)?;
            let bytes = entry.as_bytes();
            let n = bytes.len().min(crate::path::PATH_MAX - 1);
            wp.path_scratch[..n].copy_from_slice(&bytes[..n]);
            wp.path_scratch[n] = 0;
            let addr = wp.path_scratch.as_ptr() as u64;
            let len = n + 1;
            // C `cpf_grant_direct(fs_e, lastc, len, CPF_READ)`。
            let grant = self
                .grants
                .grant_direct(
                    &minix_sys::syscall::DirectKernelCallTransport,
                    fs_e.get(),
                    addr,
                    len as u64,
                    minix_types::CpFlags::READ,
                )
                .map_err(|_| minix_types::EIO)?;
            (grant, len)
        };
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::Status);
        }
        let req = if rmdir {
            crate::request::encode_rmdir(dir_ino, grant, name_len)
        } else {
            crate::request::encode_unlink(dir_ino, grant, name_len)
        };
        self.pending_fs = Some(PendingFs {
            vmnt,
            fs_e,
            worker: idx,
            grant,
            user,
            req,
        });
        Ok(())
    }

    /// `do_chown` 的**共用体**（C protect.c:24-110 的 path 与 fd 两半只差
    /// vnode 的来源）：只读门 → 三条属主规则（`protect::chown_gate`）→
    /// `-1` 折算（`protect::keep_id`）→ 界检查 → `REQ_CHOWN`。
    #[allow(clippy::too_many_arguments)]
    pub fn finish_chown(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        fs_e: Endpoint,
        ino: u64,
        node_uid: u32,
        node_gid: u32,
        uid: u32,
        gid: u32,
        vnode: usize,
    ) {
        let (eff_uid, eff_gid) = match fp_slot.and_then(|s| self.fproc_table.get(s)) {
            Some(fp) => (fp.eff_uid, fp.eff_gid),
            None => {
                self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                return;
            }
        };
        let readonly_fs = self
            .vmnt_table
            .find_by_fs(fs_e)
            .and_then(|v| self.vmnt_table.get(v))
            .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
            .unwrap_or(false);
        // C `do_chown:140-151`：先只读门，再三条规则（都按**消息里的原始
        // 值**判：非超级用户给 `-1` 也会在"no giving away"那条上被拒）。
        let verdict = crate::protect::chown_gate(
            eff_uid == crate::link::SU_UID,
            node_uid == eff_uid,
            node_uid == uid,
            eff_gid == gid,
            !readonly_fs,
        );
        if let Err(e) = verdict {
            self.finish_worker_job(idx, fp_slot, e.to_errno());
            return;
        }
        // C `do_chown:154-158`：`-1` 折算成现有值，然后界检查。
        let new_uid = crate::protect::keep_id(
            if uid == crate::protect::ID_EXPIRED { None } else { Some(uid) },
            node_uid,
        );
        let new_gid = crate::protect::keep_id(
            if gid == crate::protect::ID_EXPIRED { None } else { Some(gid) },
            node_gid,
        );
        if crate::protect::check_id_bounds(new_uid, new_gid).is_err() {
            self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
            return;
        }
        let vmnt = match self.vmnt_table.find_by_fs(fs_e) {
            Some(v) => v.0,
            None => {
                self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                return;
            }
        };
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::Chown {
                vnode,
                uid: new_uid,
                gid: new_gid,
            });
        }
        self.pending_fs = Some(PendingFs {
            vmnt,
            fs_e,
            worker: idx,
            grant: 0, // 无数据面
            user,
            req: crate::request::encode_chown(ino, new_uid, new_gid),
        });
    }

    /// `truncate_vnode` 的**发送半**（C link.c:365-381，`do_truncate` 与
    /// `do_ftruncate` 共用）：大小不变且是常规文件 → 不发请求（POSIX 文件
    /// 时间）→ 类型门（REG/FIFO）→ 64 位能力门 → `REQ_FTRUNC`。
    ///
    /// 返回值三态：`Ok(true)` = 已登记请求（调用方报挂起）、`Ok(false)` =
    /// 大小不变（调用方回 0）、`Err(errno)` = 门没过。
    #[allow(clippy::too_many_arguments)]
    pub fn send_ftrunc_for_vnode(
        &mut self,
        idx: Option<usize>,
        fp_slot: Option<minix_types::UserSlot>,
        fs_e: Endpoint,
        ino: u64,
        mode: u32,
        size: u64,
        length: i64,
        vnode: usize,
    ) -> Result<bool, i32> {
        // C link.c:349-354 —— 大小不变则不打扰 FS（POSIX 文件时间）。
        if mode & crate::open::S_IFMT == crate::open::S_IFREG && size == length as u64 {
            return Ok(false);
        }
        // C `truncate_vnode:375-376` —— 只服务常规文件与管道。
        let ftype = mode & crate::open::S_IFMT;
        if ftype != crate::open::S_IFREG && ftype != crate::open::S_IFIFO {
            return Err(minix_types::EINVAL);
        }
        let vmnt_id = self.vmnt_table.find_by_fs(fs_e).ok_or(minix_types::EIO)?;
        // C request.c:274-278 —— 未声明 64 位且长度越过 INT_MAX 即 EINVAL。
        let fs_flags = self
            .vmnt_table
            .get(vmnt_id)
            .map(|v| v.fs_flags)
            .unwrap_or(0);
        if fs_flags & crate::request::FsFlags::IS64BIT.bits() == 0 && length > i32::MAX as i64 {
            return Err(minix_types::EINVAL);
        }
        // worker 槽到**真要发请求**时才算数：大小不变那条路不该因为"没槽"
        // 而变成 EAGAIN（C 里 `truncate_vnode` 才是需要线程的地方）。
        let Some(idx) = idx else {
            return Err(minix_types::EAGAIN);
        };
        let user_e = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::Ftrunc {
                vnode,
                newsize: length,
            });
        }
        self.pending_fs = Some(PendingFs {
            vmnt: vmnt_id.0,
            fs_e,
            worker: idx,
            grant: 0, // 无数据面，不发 grant
            user: user_e,
            req: crate::request::encode_ftrunc(ino, length, 0),
        });
        Ok(true)
    }

    /// `do_utimens` 的**共用体**（C time.c:44-160 的 path 与 fd 两半只差
    /// vnode 的来源）：属主/超级用户门（EPERM，**但两个纳秒都是 `UTIME_NOW`
    /// 时退化为写权限检查**）→ 只读门（EROFS）→ 纳秒折算（`UTIME_NOW` 取
    /// 当前时间、`UTIME_OMIT` 原样带下、其余校验 < 1e9 否则 EINVAL）→
    /// `REQ_UTIME`。
    ///
    /// 时钟：C 的 `clock_time` 读 kerninfo 页（不会失败），Rust 侧同源数据走
    /// 内核调用（`clock_time_via`）——宿主构建下取不到，这里**诚实上浮
    /// EIO**，不用 0 当"现在"（写进文件系统的是 1970 时间戳，比报错更难查）。
    #[allow(clippy::too_many_arguments)]
    pub fn finish_utimens(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        fs_e: Endpoint,
        ino: u64,
        node_uid: u32,
        node_gid: u32,
        node_mode: u32,
        atime: (i64, i64),
        mtime: (i64, i64),
    ) {
        let (real_uid, real_gid, eff_uid, eff_gid, supp) =
            match fp_slot.and_then(|s| self.fproc_table.get(s)) {
                Some(fp) => (
                    fp.real_uid,
                    fp.real_gid,
                    fp.eff_uid,
                    fp.eff_gid,
                    fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                ),
                None => {
                    self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                    return;
                }
            };
        let readonly_fs = self
            .vmnt_table
            .find_by_fs(fs_e)
            .and_then(|v| self.vmnt_table.get(v))
            .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
            .unwrap_or(false);
        // C `time.c:123-130` 的三条（顺序即优先级：属主门 → 写权限退化门 →
        // 只读门覆盖一切）。
        let mut verdict: Result<(), i32> =
            if node_uid == eff_uid || eff_uid == crate::link::SU_UID {
                Ok(())
            } else {
                Err(minix_types::EPERM)
            };
        if verdict.is_err()
            && atime.1 == crate::open::UTIME_NOW
            && mtime.1 == crate::open::UTIME_NOW
        {
            // 两个都是"现在"＝touch：退化成写权限检查（C `time.c:126-128`
            // 的 `forbidden(fp, vp, W_BIT)`——用的是节点的真实模式与属组，
            // 不是"只判权限位"）。
            let forbid = crate::protect::forbidden_decision(&crate::protect::ForbidInput {
                real_uid,
                real_gid,
                eff_uid,
                eff_gid,
                is_access_call: false,
                file_uid: node_uid,
                file_gid: node_gid,
                mode: node_mode,
                access: crate::open::W_BIT as u8,
                is_dir: node_mode & crate::open::S_IFMT == crate::open::S_IFDIR,
                supp: &supp,
                readonly_fs: false,
            });
            verdict = forbid.map_err(|e| e.to_errno());
        }
        if readonly_fs {
            verdict = Err(minix_types::EROFS);
        }
        if let Err(e) = verdict {
            self.finish_worker_job(idx, fp_slot, e);
            return;
        }
        // C `time.c:135-158`：需要"现在"时取一次时钟（只在有 NOW 时取）。
        let needs_now = |nsec: i64| nsec == crate::open::UTIME_NOW;
        let now = if needs_now(atime.1) || needs_now(mtime.1) {
            match minix_sys::syscall::clock_time_via(&minix_sys::syscall::DirectKernelCallTransport)
            {
                Ok(sec) => sec as i64,
                Err(_) => {
                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                    return;
                }
            }
        } else {
            0
        };
        let resolve_one = |sec: i64, nsec: i64, now: i64| -> Result<(i64, i64), i32> {
            match nsec {
                n if n == crate::open::UTIME_NOW => Ok((now, 0)),
                n if n == crate::open::UTIME_OMIT => Ok((now, n)),
                n if (n as u64) < 1_000_000_000 => Ok((sec, n)),
                _ => Err(minix_types::EINVAL),
            }
        };
        let (actime, acnsec) = match resolve_one(atime.0, atime.1, now) {
            Ok(pair) => pair,
            Err(e) => {
                self.finish_worker_job(idx, fp_slot, e);
                return;
            }
        };
        let (modtime, modnsec) = match resolve_one(mtime.0, mtime.1, now) {
            Ok(pair) => pair,
            Err(e) => {
                self.finish_worker_job(idx, fp_slot, e);
                return;
            }
        };
        let vmnt = match self.vmnt_table.find_by_fs(fs_e) {
            Some(v) => v.0,
            None => {
                self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                return;
            }
        };
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::Status);
        }
        self.pending_fs = Some(PendingFs {
            vmnt,
            fs_e,
            worker: idx,
            grant: 0, // 无数据面
            user,
            req: crate::request::encode_utime(
                ino,
                actime,
                modtime,
                acnsec as u32,
                modnsec as u32,
            ),
        });
    }

    /// `do_chmod` 的**共用体**（C protect.c:62-133 的 path 与 fd 两半只差
    /// vnode 的来源）：属主/超级用户门（EPERM）→ 只读门（EROFS）→ setgid
    /// 清位（`protect::strip_setgid`）→ `REQ_CHMOD`；回复带**整字模式**，
    /// 续接体写回 vnode 缓存。
    #[allow(clippy::too_many_arguments)]
    pub fn finish_chmod(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        fs_e: Endpoint,
        ino: u64,
        node_uid: u32,
        node_gid: u32,
        mode: u32,
        vnode: usize,
    ) {
        let (eff_uid, eff_gid) = match fp_slot.and_then(|s| self.fproc_table.get(s)) {
            Some(fp) => (fp.eff_uid, fp.eff_gid),
            None => {
                self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                return;
            }
        };
        // C `do_chmod:112-114`：只有属主或超级用户能改模式。
        if node_uid != eff_uid && eff_uid != crate::link::SU_UID {
            self.finish_worker_job(idx, fp_slot, minix_types::EPERM);
            return;
        }
        let readonly_fs = self
            .vmnt_table
            .find_by_fs(fs_e)
            .and_then(|v| self.vmnt_table.get(v))
            .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
            .unwrap_or(false);
        if readonly_fs {
            self.finish_worker_job(idx, fp_slot, minix_types::EROFS);
            return;
        }
        // 非超级用户且文件不在自己的组里就清 setgid（protect.c:120-121）。
        let new_mode = crate::protect::strip_setgid(
            eff_uid == crate::link::SU_UID,
            node_gid,
            eff_gid,
            mode,
        );
        let vmnt = match self.vmnt_table.find_by_fs(fs_e) {
            Some(v) => v.0,
            None => {
                self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                return;
            }
        };
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::Chmod { vnode });
        }
        self.pending_fs = Some(PendingFs {
            vmnt,
            fs_e,
            worker: idx,
            grant: 0, // 无数据面
            user,
            req: crate::request::encode_chmod(ino, new_mode),
        });
    }

    /// 收尾一个挂起的作业并回用户（错误路径与相位 2 之后的统一出口）。
    pub fn finish_worker_job(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        status: i32,
    ) {
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = None;
            wp.path = None;
            wp.sendrec = None;
            wp.task = None;
        }
        self.worker_pool.release(idx);
        if self.current_worker == Some(idx) {
            self.current_worker = None;
        }
        if let Some(fp_slot) = fp_slot
            && let Some(fp) = self.fproc_table.get(fp_slot)
        {
            let target = fp.endpoint;
            let result = if status == 0 {
                crate::call_table::SyscallResult::Ok(0)
            } else {
                crate::call_table::SyscallResult::Error(status)
            };
            self.queue_reply(target, result);
        }
    }

    /// 把臂登记的 FS 对话发出去（C `fs_sendrec` 的发送半；臂里做不到，
    /// 因为臂没有 transport 句柄）。
    ///
    /// 成功：`fs_sendrec` 把 worker 槽置 `WaitingForFs` 并投递，作业挂起
    /// 等回复（由 [`Self::handle_fs_reply`] 落地、[`Self::run_worker_continuations`]
    /// 收尾）。
    /// 失败：C 的臂会走错误回复并结束作业——这里照做：撤 grant、清续接、
    /// 释放槽、回错误给用户。
    pub fn flush_pending_fs(&mut self, transport: &mut impl crate::fs_comm::FsTransport) {
        let Some(p) = self.pending_fs.take() else {
            return;
        };
        match self.fs_sendrec(transport, p.vmnt, p.fs_e, p.worker, &p.req) {
            Ok(()) => {}
            Err(e) => {
                if let Some(wp) = self.worker_pool.get_mut(p.worker) {
                    wp.cont = None;
                    wp.sendrec = None;
                    wp.task = None;
                }
                let _ = self.revoke_grant(p.grant);
                self.worker_pool.release(p.worker);
                if self.current_worker == Some(p.worker) {
                    self.current_worker = None;
                }
                // C 的错误面：comm.c 的 `find_vmnt` 失败给 EIO、自死锁给
                // EDEADLK；本原语把两者都折成 EIO（对用户可观测的都是
                // "这次对话没成"）。
                let _ = e;
                self.queue_reply(
                    p.user,
                    crate::call_table::SyscallResult::Error(minix_types::EIO),
                );
            }
        }
    }

    /// 跑所有"回复已到、续接未跑"的作业。
    ///
    /// 判定：槽上有续接标识（`cont`）且回复已落在 `sendrec`（由
    /// [`Self::handle_fs_reply`] 写入）。每个这样的作业按续接标识完成：
    /// 撤销 grant、按需写回状态、把结果回给用户，然后释放槽。
    ///
    /// C 锚点：`fs_sendrec` 返回后的那一半（`read.c:181-190` 的位置推进、
    /// `request.c:1109` 的 revoke），状态字取 `reqmp->m_type`，
    /// `ERESTART` 折成 `EIO`（comm.c:161-163）。
    pub fn run_worker_continuations(&mut self) {
        for idx in 0..crate::worker::NR_WTHREADS {
            let (cont, reply, fp_slot) = match self.worker_pool.get_mut(idx) {
                Some(wp) => {
                    let (Some(cont), Some(reply)) = (wp.cont, wp.sendrec) else {
                        continue;
                    };
                    // 必须"回复已落槽"才跑续接：`sendrec` 在 C 里是双向缓冲
                    // ——`fs_sendrec` 送出时装的是**请求**（`set_waiting`），
                    // `do_reply` 收到时才被回复覆写。C 的线程在
                    // `worker_wait` 里阻塞，只有 `worker_signal`（本模型里
                    // `handle_fs_reply` 置 `Busy`）才会醒；若只按
                    // `cont + sendrec` 判定，另一个客户端的调用进来时会拿
                    // 请求码当状态，把挂起的作业假完成。
                    if wp.state != crate::worker::WorkerState::Busy {
                        continue;
                    }
                    (cont, reply, wp.fp_slot)
                }
                None => continue,
            };
            let mut status = reply.m_type;
            if status == minix_types::ERESTART {
                status = minix_types::EIO;
            }
            // 带载荷的回复（少数臂需要，如 lseek 的新位置）：臂把它填进
            // 这里，由下面的统一收尾发出——**不能**在臂里直接
            // `queue_reply_msg`，否则会被收尾的裸回复覆盖。
            let mut reply_payload: Option<Message> = None;
            match cont {
                crate::worker::WorkerCont::Fstat { grant } => {
                    let _ = self.revoke_grant(grant);
                }
                crate::worker::WorkerCont::OpenTrunc { node, oflags } => {
                    // C `common_open:150-157` 的 `truncate_vnode(vp, 0)` 结果
                    // **被忽略**——截断失败不拦 open；随后照常装配。清掉
                    // `O_TRUNC` 位再进本地半（否则 dispatch 会再判一次）。
                    let _ = status;
                    self.finish_open_local_inner(idx, fp_slot, &node, oflags, true);
                    continue;
                }
                crate::worker::WorkerCont::Create { user, oflags } => {
                    // 阶段 3：`REQ_CREATE` 的回复是新建节点的 node_details
                    // （C `req_create` 收尾）→ 本地半（fd/filp 装配）。
                    // `EEXIST` 一类错误按状态收尾（`O_EXCL` 的 FS 侧答复）。
                    if status != 0 {
                        self.finish_worker_job(idx, fp_slot, status);
                        continue;
                    }
                    let node = crate::request::decode_create_reply(&reply);
                    let _ = user;
                    self.finish_open_local(idx, fp_slot, &node, oflags);
                    continue;
                }
                crate::worker::WorkerCont::Status => {
                    // 纯状态：无载荷、无副作用——收尾的默认路径就够了。
                }
                crate::worker::WorkerCont::InhibRead { offset } => {
                    // 位置已在臂里改好（C `actual_lseek:640`）；这里只把新位置
                    // 填进回复载荷（C `do_lseek` 的 `m_vfs_lc_lseek.offset`）。
                    // 请求失败时按 C 返回错误（位置仍已改动）。
                    let mut m = Message {
                        m_type: status,
                        ..Message::default()
                    };
                    if status == 0 {
                        // SAFETY: `mess_vfs_lc_lseek { off_t offset; }`
                        // （ipc.h:2206-2210）在负载区首字。
                        unsafe {
                            m.m_u.raw[0..8].copy_from_slice(&offset.to_le_bytes());
                        }
                    }
                    reply_payload = Some(m);
                }
                crate::worker::WorkerCont::Chown { vnode, uid, gid } => {
                    // C `do_chown:159-163`：成功时把 uid/gid 写进 vnode、模式
                    // 取回复里的新值（FS 可能清掉 setuid/setgid 位）。
                    if status == 0 {
                        // SAFETY: 回复载荷按 LP64 域序写在负载区（共享表）。
                        let raw = unsafe { &reply.m_u.raw };
                        let mut b4 = [0u8; 4];
                        b4.copy_from_slice(
                            &raw[minix_types::chown_reply_off::MODE
                                ..minix_types::chown_reply_off::MODE + 4],
                        );
                        let new_mode = u32::from_le_bytes(b4);
                        if let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode)) {
                            v.uid = uid;
                            v.gid = gid;
                            v.mode = new_mode;
                        }
                    }
                }
                crate::worker::WorkerCont::Chmod { vnode } => {
                    // C `do_chmod` 的收尾（protect.c:126-128）：成功时把
                    // **FS 回的实际模式**写回 vnode 缓存——FS 可能收窄
                    // （例如没有权限位就清掉 setgid），缓存跟着 FS 走。
                    if status == 0 {
                        // SAFETY: 回复载荷按 LP64 域序写在负载区（共享表）。
                        let raw = unsafe { &reply.m_u.raw };
                        let mut b4 = [0u8; 4];
                        b4.copy_from_slice(
                            &raw[minix_types::chmod_reply_off::MODE
                                ..minix_types::chmod_reply_off::MODE + 4],
                        );
                        let actual = u32::from_le_bytes(b4);
                        if let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode)) {
                            v.mode = actual;
                        }
                    }
                }
                crate::worker::WorkerCont::Rdlink { grant } => {
                    // C `req_rdlink_actual`（request.c:743-747）：撤销 grant，
                    // 然后**从载荷取字节数**（`mess_fs_vfs_rdlink.nbytes`），
                    // `m_type` 只是 OK——用户拿到的长度不是状态字。
                    let _ = self.revoke_grant(grant);
                    if status == 0 {
                        // SAFETY: 回复载荷按 LP64 域序写在负载区（共享表）。
                        let raw = unsafe { &reply.m_u.raw };
                        let mut b8 = [0u8; 8];
                        b8.copy_from_slice(
                            &raw[minix_types::rdlink_reply_off::NBYTES
                                ..minix_types::rdlink_reply_off::NBYTES + 8],
                        );
                        status = i64::from_le_bytes(b8) as i32;
                    }
                }
                crate::worker::WorkerCont::Getdents { grant, filp } => {
                    // C `req_getdents_actual` 的收尾（request.c:330-336）：
                    // 回复的 `seek_pos` 是下一趟的位置、`nbytes` 是本次写出
                    // 的字节数，而**位置只在 `nbytes > 0` 时推进**
                    // （read.c:311-313 的 `if (r > 0) rfilp->filp_pos =
                    // new_pos;`）——空目录/缓冲满都是 `nbytes == 0`，位置
                    // 必须留在原处，否则下一趟会跳过条目。
                    let _ = self.revoke_grant(grant);
                    if status == 0 {
                        // SAFETY: 回复载荷按 LP64 域序写在负载区（共享表）。
                        let raw = unsafe { &reply.m_u.raw };
                        let mut b8 = [0u8; 8];
                        b8.copy_from_slice(
                            &raw[minix_types::getdents_reply_off::SEEK_POS
                                ..minix_types::getdents_reply_off::SEEK_POS + 8],
                        );
                        let new_pos = i64::from_le_bytes(b8);
                        b8.copy_from_slice(
                            &raw[minix_types::getdents_reply_off::NBYTES
                                ..minix_types::getdents_reply_off::NBYTES + 8],
                        );
                        let nbytes = i64::from_le_bytes(b8);
                        if nbytes > 0
                            && let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(filp))
                        {
                            f.pos = new_pos;
                        }
                        // 用户拿到的是字节数（C 返回 `nbytes`，不是 0）。
                        status = nbytes as i32;
                    }
                }
                crate::worker::WorkerCont::Ftrunc { vnode, newsize } => {
                    // 成功时更新 vnode 大小（C `truncate_vnode` 尾部的
                    // `vp->v_size = newsize`；失败不动）。
                    if status == 0
                        && let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode))
                    {
                        v.size = newsize as u64;
                    }
                }
                crate::worker::WorkerCont::Path => {
                    // 路径遍历的续走：取出现场 → revoke → 解回复 →
                    // `walk.resume` → 再发一条 lookup（放回现场、继续挂起）
                    // 或做相位 2（`PathFollow`）。
                    let Some(mut pending) = self
                        .worker_pool
                        .get_mut(idx)
                        .and_then(|wp| wp.path.take())
                    else {
                        continue;
                    };
                    let _ = self.revoke_grant(pending.grant);
                    // `PathFollow` 非 `Copy`（`Mkdir` 带组件名）——克隆一份
                    // 给相位 2，`pending.walk` 仍留在原地供续走。
                    let follow = pending.follow.clone();
                    let walk_err = |e: crate::path::PathError| e.to_errno();
                    // 回复解码：特殊码/OK 交给状态机；其余状态（含负 errno）
                    // 按错误收尾。
                    let resumed: Result<crate::path::WalkStep, PathFail> =
                        match crate::request::decode_lookup_reply(status, &reply) {
                            Some(res) => {
                                let rd = self.root_dir_of(fp_slot);
                                let mounts = self.mounted_fs_list();
                                pending.walk.resume(res, rd, &mounts).map_err(PathFail::Path)
                            }
                            None => Err(PathFail::Status(status)),
                        };
                    match resumed {
                        Ok(crate::path::WalkStep::Send { fs_e, dir_ino, root_ino }) => {
                            // 再发一条 REQ_LOOKUP（路径从 walk 的游标拷进槽内
                            // scratch + NUL），现场放回，继续挂起。
                            if let Some(wp) = self.worker_pool.get_mut(idx) {
                                wp.path = Some(pending);
                            }
                            if self
                                .send_lookup_for_slot(idx, fp_slot, fs_e, dir_ino, root_ino)
                                .is_err()
                            {
                                // 发送前失败（无 vmnt/无槽/grant 失败）：收尾。
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.path = None;
                                }
                                self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                            }
                            continue;
                        }
                        Ok(crate::path::WalkStep::Done(node)) => match follow {
                            crate::worker::PathFollow::Unlink { entry, rmdir } => {
                                // C `do_unlink`（link.c:122-159）阶段 1 走通：
                                // 父目录类型门 → `X|W` 权限门 → 粘滞位门。
                                if node.mode & crate::open::S_IFMT != crate::open::S_IFDIR {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENOTDIR);
                                    continue;
                                }
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) =
                                    match fp_slot.and_then(|s| self.fproc_table.get(s)) {
                                        Some(fp) => (
                                            fp.real_uid,
                                            fp.real_gid,
                                            fp.eff_uid,
                                            fp.eff_gid,
                                            fp.supplemental_groups[..fp.ngroups.min(16)]
                                                .to_vec(),
                                        ),
                                        None => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EINVAL,
                                            );
                                            continue;
                                        }
                                    };
                                let readonly_fs = self
                                    .vmnt_table
                                    .find_by_fs(node.fs_e)
                                    .and_then(|v| self.vmnt_table.get(v))
                                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                                    .unwrap_or(false);
                                // C `forbidden(fp, dirp, X_BIT | W_BIT)`。
                                let forbid = crate::protect::forbidden_decision(
                                    &crate::protect::ForbidInput {
                                        real_uid,
                                        real_gid,
                                        eff_uid,
                                        eff_gid,
                                        is_access_call: false,
                                        file_uid: node.uid,
                                        file_gid: node.gid,
                                        mode: node.mode,
                                        access: (crate::open::X_BIT | crate::open::W_BIT) as u8,
                                        is_dir: true,
                                        supp: &supp,
                                        readonly_fs,
                                    },
                                );
                                if let Err(e) = forbid {
                                    self.finish_worker_job(idx, fp_slot, e.to_errno());
                                    continue;
                                }
                                // C `do_unlink:132-152`：粘滞位目录上要先把
                                // 受害者查出来（`advance(dirp, stickycheck)`），
                                // 属主不对就是 EPERM——这一步是**子遍历**，
                                // 所以在 Rust 里是又一段挂起。
                                if node.mode & crate::open::S_ISVTX != 0 {
                                    let start = crate::path::LookupStart {
                                        fs: node.fs_e,
                                        ino: node.ino,
                                        dev: node.dev,
                                    };
                                    let rd = self.root_dir_of(fp_slot);
                                    let resolve = match crate::path::Lookup::new(
                                        entry.clone(),
                                        crate::path::LookupFlags::RET_SYMLINK,
                                    ) {
                                        Ok(l) => l,
                                        Err(e) => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                e.to_errno(),
                                            );
                                            continue;
                                        }
                                    };
                                    let (walk2, step2) = match crate::path::LookupWalk::begin(
                                        start, resolve, rd, eff_uid, eff_gid,
                                    ) {
                                        Ok(pair) => pair,
                                        Err(e) => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                e.to_errno(),
                                            );
                                            continue;
                                        }
                                    };
                                    let crate::path::WalkStep::Send {
                                        fs_e,
                                        dir_ino,
                                        root_ino,
                                    } = step2
                                    else {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    };
                                    if let Some(wp) = self.worker_pool.get_mut(idx) {
                                        wp.cont = Some(crate::worker::WorkerCont::Path);
                                        wp.path = Some(crate::worker::PathPending {
                                            walk: walk2,
                                            grant: 0,
                                            follow: crate::worker::PathFollow::UnlinkSticky {
                                                entry,
                                                rmdir,
                                                dir_fs_e: node.fs_e,
                                                dir_ino: node.ino,
                                            },
                                        });
                                    }
                                    if self
                                        .send_lookup_for_slot(idx, fp_slot, fs_e, dir_ino, root_ino)
                                        .is_err()
                                    {
                                        if let Some(wp) = self.worker_pool.get_mut(idx) {
                                            wp.path = None;
                                            wp.cont = None;
                                        }
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                    }
                                    continue;
                                }
                                if let Err(e) = self.send_unlink_for_slot(
                                    idx,
                                    fp_slot,
                                    node.fs_e,
                                    node.ino,
                                    &entry,
                                    rmdir,
                                ) {
                                    self.finish_worker_job(idx, fp_slot, e);
                                }
                                continue;
                            }
                            crate::worker::PathFollow::UnlinkSticky {
                                entry,
                                rmdir,
                                dir_fs_e,
                                dir_ino,
                            } => {
                                // C `do_unlink:137-141`：粘滞位目录里只有受害者
                                // 属主或超级用户能删（决策函数 `link::sticky_check`）。
                                // 受害者查不到时走上一条 Err 分支（C 的
                                // `else r = err_code`），这里不会到达。
                                let eff_uid = match fp_slot.and_then(|s| self.fproc_table.get(s)) {
                                    Some(fp) => fp.eff_uid,
                                    None => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                                        continue;
                                    }
                                };
                                if let Err(e) = crate::link::sticky_check(true, node.uid, eff_uid) {
                                    self.finish_worker_job(idx, fp_slot, e.to_errno());
                                    continue;
                                }
                                if let Err(e) = self.send_unlink_for_slot(
                                    idx,
                                    fp_slot,
                                    dir_fs_e,
                                    dir_ino,
                                    &entry,
                                    rmdir,
                                ) {
                                    self.finish_worker_job(idx, fp_slot, e);
                                }
                                continue;
                            }
                            crate::worker::PathFollow::LinkSrc { dst_path } => {
                                // C `do_link:188-196` 阶段 1 走通：转阶段 2——
                                // 切出 name2 的父目录与组件名，**从根/工作目录
                                // 重新起走**（C 是同一个 `resolve` 换路径再来
                                // 一趟 `last_dir`）。
                                let rd = self.root_dir_of(fp_slot);
                                let split = match crate::path::last_dir_split(&dst_path) {
                                    Ok(sp) => sp,
                                    Err(e) => {
                                        self.finish_worker_job(idx, fp_slot, e.to_errno());
                                        continue;
                                    }
                                };
                                let resolve = match crate::path::Lookup::new(
                                    split.dir_path.clone(),
                                    crate::path::LookupFlags::NOFLAGS,
                                ) {
                                    Ok(l) => l,
                                    Err(e) => {
                                        self.finish_worker_job(idx, fp_slot, e.to_errno());
                                        continue;
                                    }
                                };
                                let start = if resolve.path.starts_with('/') {
                                    crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
                                } else {
                                    let wd = fp_slot
                                        .map(|s| self.work_dir_of(s))
                                        .unwrap_or(crate::path::RootDir {
                                            ino: rd.ino,
                                            fs: rd.fs,
                                            dev: rd.dev,
                                        });
                                    crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
                                };
                                let (uid, gid) = match fp_slot.and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (fp.eff_uid, fp.eff_gid),
                                    None => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                                        continue;
                                    }
                                };
                                let (walk2, step2) = match crate::path::LookupWalk::begin(
                                    start, resolve, rd, uid, gid,
                                ) {
                                    Ok(pair) => pair,
                                    Err(e) => {
                                        self.finish_worker_job(idx, fp_slot, e.to_errno());
                                        continue;
                                    }
                                };
                                let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step2
                                else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                    continue;
                                };
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont = Some(crate::worker::WorkerCont::Path);
                                    wp.path = Some(crate::worker::PathPending {
                                        walk: walk2,
                                        grant: 0,
                                        follow: crate::worker::PathFollow::LinkDst {
                                            src_fs_e: node.fs_e,
                                            src_ino: node.ino,
                                            entry: split.entry,
                                        },
                                    });
                                }
                                if self
                                    .send_lookup_for_slot(idx, fp_slot, fs_e, dir_ino, root_ino)
                                    .is_err()
                                {
                                    if let Some(wp) = self.worker_pool.get_mut(idx) {
                                        wp.path = None;
                                        wp.cont = None;
                                    }
                                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                }
                                continue;
                            }
                            crate::worker::PathFollow::LinkDst {
                                src_fs_e,
                                src_ino,
                                entry,
                            } => {
                                // C `do_link:203-211`：跨设备门（EXDEV）→
                                // `W|X` 门 → `req_link`。
                                if node.fs_e != src_fs_e {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EXDEV);
                                    continue;
                                }
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) =
                                    match fp_slot.and_then(|s| self.fproc_table.get(s)) {
                                        Some(fp) => (
                                            fp.real_uid,
                                            fp.real_gid,
                                            fp.eff_uid,
                                            fp.eff_gid,
                                            fp.supplemental_groups[..fp.ngroups.min(16)]
                                                .to_vec(),
                                        ),
                                        None => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EINVAL,
                                            );
                                            continue;
                                        }
                                    };
                                let readonly_fs = self
                                    .vmnt_table
                                    .find_by_fs(node.fs_e)
                                    .and_then(|v| self.vmnt_table.get(v))
                                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                                    .unwrap_or(false);
                                let forbid = crate::protect::forbidden_decision(
                                    &crate::protect::ForbidInput {
                                        real_uid,
                                        real_gid,
                                        eff_uid,
                                        eff_gid,
                                        is_access_call: false,
                                        file_uid: node.uid,
                                        file_gid: node.gid,
                                        mode: node.mode,
                                        access: (crate::open::W_BIT | crate::open::X_BIT) as u8,
                                        is_dir: true,
                                        supp: &supp,
                                        readonly_fs,
                                    },
                                );
                                if let Err(e) = forbid {
                                    self.finish_worker_job(idx, fp_slot, e.to_errno());
                                    continue;
                                }
                                let (grant, name_len) = {
                                    let Some(wp) = self.worker_pool.get_mut(idx) else {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    };
                                    let bytes = entry.as_bytes();
                                    let n = bytes.len().min(crate::path::PATH_MAX - 1);
                                    wp.path_scratch[..n].copy_from_slice(&bytes[..n]);
                                    wp.path_scratch[n] = 0;
                                    let addr = wp.path_scratch.as_ptr() as u64;
                                    let len = n + 1;
                                    match self.grants.grant_direct(
                                        &minix_sys::syscall::DirectKernelCallTransport,
                                        node.fs_e.get(),
                                        addr,
                                        len as u64,
                                        minix_types::CpFlags::READ,
                                    ) {
                                        Ok(g) => (g, len),
                                        Err(_) => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EIO,
                                            );
                                            continue;
                                        }
                                    }
                                };
                                let Some(vmnt) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    let _ = self.revoke_grant(grant);
                                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                    continue;
                                };
                                let user = fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                    .map(|fp| fp.endpoint)
                                    .unwrap_or(Endpoint::NONE);
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont = Some(crate::worker::WorkerCont::Status);
                                }
                                self.pending_fs = Some(PendingFs {
                                    vmnt: vmnt.0,
                                    fs_e: node.fs_e,
                                    worker: idx,
                                    grant,
                                    user,
                                    req: crate::request::encode_link(
                                        src_ino, node.ino, grant, name_len,
                                    ),
                                });
                                continue;
                            }
                            crate::worker::PathFollow::Slink { entry, target_addr, target_len } => {
                                // C `do_slink:411-414`：`forbidden(fp, vp,
                                // W_BIT|X_BIT)` 过了才发 `req_slink`（父目录
                                // 的类型由遍历/FS 把关，这里没有单独的类型门）。
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) =
                                    match fp_slot.and_then(|s| self.fproc_table.get(s)) {
                                        Some(fp) => (
                                            fp.real_uid,
                                            fp.real_gid,
                                            fp.eff_uid,
                                            fp.eff_gid,
                                            fp.supplemental_groups[..fp.ngroups.min(16)]
                                                .to_vec(),
                                        ),
                                        None => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EINVAL,
                                            );
                                            continue;
                                        }
                                    };
                                let readonly_fs = self
                                    .vmnt_table
                                    .find_by_fs(node.fs_e)
                                    .and_then(|v| self.vmnt_table.get(v))
                                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                                    .unwrap_or(false);
                                let forbid = crate::protect::forbidden_decision(
                                    &crate::protect::ForbidInput {
                                        real_uid,
                                        real_gid,
                                        eff_uid,
                                        eff_gid,
                                        is_access_call: false,
                                        file_uid: node.uid,
                                        file_gid: node.gid,
                                        mode: node.mode,
                                        access: (crate::open::W_BIT | crate::open::X_BIT) as u8,
                                        is_dir: true,
                                        supp: &supp,
                                        readonly_fs,
                                    },
                                );
                                if let Err(e) = forbid {
                                    self.finish_worker_job(idx, fp_slot, e.to_errno());
                                    continue;
                                }
                                let user = fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                    .map(|fp| fp.endpoint)
                                    .unwrap_or(Endpoint::NONE);
                                // 名字：VFS 内存里的 direct grant。
                                let (grant_path, name_len) = {
                                    let Some(wp) = self.worker_pool.get_mut(idx) else {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    };
                                    let bytes = entry.as_bytes();
                                    let n = bytes.len().min(crate::path::PATH_MAX - 1);
                                    wp.path_scratch[..n].copy_from_slice(&bytes[..n]);
                                    wp.path_scratch[n] = 0;
                                    let addr = wp.path_scratch.as_ptr() as u64;
                                    let len = n + 1;
                                    match self.grants.grant_direct(
                                        &minix_sys::syscall::DirectKernelCallTransport,
                                        node.fs_e.get(),
                                        addr,
                                        len as u64,
                                        minix_types::CpFlags::READ,
                                    ) {
                                        Ok(g) => (g, len),
                                        Err(_) => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EIO,
                                            );
                                            continue;
                                        }
                                    }
                                };
                                // 目标串：**用户内存**里的 magic grant（首趟带
                                // `CPF_TRY`，C request.c:1061-1062）。
                                let grant_target = match self.grant_user_buffer(
                                    node.fs_e,
                                    user,
                                    target_addr,
                                    target_len,
                                    minix_types::CpFlags::READ | minix_types::CpFlags::TRY,
                                ) {
                                    Ok(g) => g,
                                    Err(_) => {
                                        let _ = self.revoke_grant(grant_path);
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    }
                                };
                                let Some(vmnt) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    let _ = self.revoke_grant(grant_path);
                                    let _ = self.revoke_grant(grant_target);
                                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                    continue;
                                };
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont = Some(crate::worker::WorkerCont::Status);
                                }
                                self.pending_fs = Some(PendingFs {
                                    vmnt: vmnt.0,
                                    fs_e: node.fs_e,
                                    worker: idx,
                                    grant: grant_path,
                                    user,
                                    req: crate::request::encode_slink(
                                        node.ino,
                                        name_len,
                                        target_len as usize,
                                        grant_path,
                                        grant_target,
                                        eff_uid,
                                        eff_gid,
                                    ),
                                });
                                continue;
                            }
                            crate::worker::PathFollow::Utimens { atime, mtime, flags } => {
                                // C `do_utimens` 的路径半（time.c:74-96）：未知
                                // 标志在入口就拒（这里再核一次，防 follow 被
                                // 别处构造）；`AT_SYMLINK_NOFOLLOW` 的遍历语义
                                // 已在入口用于选 flags。
                                let _ = flags;
                                self.finish_utimens(
                                    idx,
                                    fp_slot,
                                    node.fs_e,
                                    node.ino,
                                    node.uid,
                                    node.gid,
                                    node.mode,
                                    atime,
                                    mtime,
                                );
                                continue;
                            }
                            crate::worker::PathFollow::Mknod { entry, mode_bits, dev } => {
                                // C `do_mknod:543-552`：父目录类型门 → `W|X`
                                // 权限门 → `req_mknod`（名字走 direct grant，
                                // 回复只有状态）。
                                if node.mode & crate::open::S_IFMT != crate::open::S_IFDIR {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENOTDIR);
                                    continue;
                                }
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) =
                                    match fp_slot.and_then(|s| self.fproc_table.get(s)) {
                                        Some(fp) => (
                                            fp.real_uid,
                                            fp.real_gid,
                                            fp.eff_uid,
                                            fp.eff_gid,
                                            fp.supplemental_groups[..fp.ngroups.min(16)]
                                                .to_vec(),
                                        ),
                                        None => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EINVAL,
                                            );
                                            continue;
                                        }
                                    };
                                let readonly_fs = self
                                    .vmnt_table
                                    .find_by_fs(node.fs_e)
                                    .and_then(|v| self.vmnt_table.get(v))
                                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                                    .unwrap_or(false);
                                let forbid = crate::protect::forbidden_decision(
                                    &crate::protect::ForbidInput {
                                        real_uid,
                                        real_gid,
                                        eff_uid,
                                        eff_gid,
                                        is_access_call: false,
                                        file_uid: node.uid,
                                        file_gid: node.gid,
                                        mode: node.mode,
                                        access: (crate::open::W_BIT | crate::open::X_BIT) as u8,
                                        is_dir: true,
                                        supp: &supp,
                                        readonly_fs,
                                    },
                                );
                                if let Err(e) = forbid {
                                    self.finish_worker_job(idx, fp_slot, e.to_errno());
                                    continue;
                                }
                                // 名字写进槽内 scratch 并做 direct grant
                                // （C `cpf_grant_direct(fs_e, lastc, len,
                                // CPF_READ)`）。
                                let (grant, name_len) = {
                                    let Some(wp) = self.worker_pool.get_mut(idx) else {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    };
                                    let bytes = entry.as_bytes();
                                    let n = bytes.len().min(crate::path::PATH_MAX - 1);
                                    wp.path_scratch[..n].copy_from_slice(&bytes[..n]);
                                    wp.path_scratch[n] = 0;
                                    let addr = wp.path_scratch.as_ptr() as u64;
                                    let len = n + 1;
                                    match self.grants.grant_direct(
                                        &minix_sys::syscall::DirectKernelCallTransport,
                                        node.fs_e.get(),
                                        addr,
                                        len as u64,
                                        minix_types::CpFlags::READ,
                                    ) {
                                        Ok(g) => (g, len),
                                        Err(_) => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EIO,
                                            );
                                            continue;
                                        }
                                    }
                                };
                                let Some(vmnt) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    let _ = self.revoke_grant(grant);
                                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                    continue;
                                };
                                let user = fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                    .map(|fp| fp.endpoint)
                                    .unwrap_or(Endpoint::NONE);
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont = Some(crate::worker::WorkerCont::Status);
                                }
                                self.pending_fs = Some(PendingFs {
                                    vmnt: vmnt.0,
                                    fs_e: node.fs_e,
                                    worker: idx,
                                    grant,
                                    user,
                                    req: crate::request::encode_mknod(
                                        dev, node.ino, mode_bits, eff_uid, eff_gid, grant,
                                        name_len,
                                    ),
                                });
                                continue;
                            }
                            crate::worker::PathFollow::Truncate { length } => {
                                // C `do_truncate:311-316`：`forbidden(fp, vp,
                                // W_BIT)` 过了才动文件；大小不变那条在发送半里。
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) =
                                    match fp_slot.and_then(|s| self.fproc_table.get(s)) {
                                        Some(fp) => (
                                            fp.real_uid,
                                            fp.real_gid,
                                            fp.eff_uid,
                                            fp.eff_gid,
                                            fp.supplemental_groups[..fp.ngroups.min(16)]
                                                .to_vec(),
                                        ),
                                        None => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EINVAL,
                                            );
                                            continue;
                                        }
                                    };
                                let readonly_fs = self
                                    .vmnt_table
                                    .find_by_fs(node.fs_e)
                                    .and_then(|v| self.vmnt_table.get(v))
                                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                                    .unwrap_or(false);
                                let forbid = crate::protect::forbidden_decision(
                                    &crate::protect::ForbidInput {
                                        real_uid,
                                        real_gid,
                                        eff_uid,
                                        eff_gid,
                                        is_access_call: false,
                                        file_uid: node.uid,
                                        file_gid: node.gid,
                                        mode: node.mode,
                                        access: crate::open::W_BIT as u8,
                                        is_dir: node.mode & crate::open::S_IFMT
                                            == crate::open::S_IFDIR,
                                        supp: &supp,
                                        readonly_fs,
                                    },
                                );
                                if let Err(e) = forbid {
                                    self.finish_worker_job(idx, fp_slot, e.to_errno());
                                    continue;
                                }
                                let Some(vnode) = self.intern_vnode(&node) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                                    continue;
                                };
                                match self.send_ftrunc_for_vnode(
                                    Some(idx),
                                    fp_slot,
                                    node.fs_e,
                                    node.ino,
                                    node.mode,
                                    node.size,
                                    length,
                                    vnode,
                                ) {
                                    // 已登记请求：等 FS 回复（续接体收尾）。
                                    Ok(true) => {}
                                    // 大小不变：C 回 `r = OK`，作业就地完结。
                                    Ok(false) => {
                                        self.finish_worker_job(idx, fp_slot, 0)
                                    }
                                    Err(e) => self.finish_worker_job(idx, fp_slot, e),
                                }
                                continue;
                            }
                            crate::worker::PathFollow::Chown { uid, gid } => {
                                // C `do_chown` 的路径半：走完就交给共用体
                                // （只读门 + 三条属主规则 + 界检查 + 发请求）。
                                let Some(vnode) = self.intern_vnode(&node) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                                    continue;
                                };
                                self.finish_chown(
                                    idx,
                                    fp_slot,
                                    node.fs_e,
                                    node.ino,
                                    node.uid,
                                    node.gid,
                                    uid,
                                    gid,
                                    vnode,
                                );
                                continue;
                            }
                            crate::worker::PathFollow::Chmod { user: _u, mode } => {
                                // C `do_chmod` 的路径半：走完并表 vnode，然后进
                                // 共用体（属主门 + 只读门 + setgid 清位 + 发请求）。
                                let Some(vnode) = self.intern_vnode(&node) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                                    continue;
                                };
                                self.finish_chmod(
                                    idx, fp_slot, node.fs_e, node.ino, node.uid, node.gid, mode,
                                    vnode,
                                );
                                continue;
                            }
                            crate::worker::PathFollow::Rdlink { user, buf, buf_size } => {
                                // C `do_rdlink`（link.c:496-506）：不是符号链接
                                // 就是 EINVAL（`PATH_RET_SYMLINK` 已经让 FS 不
                                // 跟进末组件，所以这里拿到的是链接本身），是
                                // 符号链接才发 `REQ_RDLINK`。
                                if node.mode & crate::open::S_IFMT != crate::open::S_IFLNK {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                                    continue;
                                }
                                let Some(vmnt) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                    continue;
                                };
                                // magic grant：FS 往用户缓冲写链接文本。
                                // C 的首趟带 `CPF_TRY`（request.c:760-761）。
                                let grant = match self.grant_user_buffer(
                                    node.fs_e,
                                    user,
                                    buf,
                                    buf_size,
                                    minix_types::CpFlags::WRITE | minix_types::CpFlags::TRY,
                                ) {
                                    Ok(g) => g,
                                    Err(_) => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    }
                                };
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont = Some(crate::worker::WorkerCont::Rdlink { grant });
                                }
                                self.pending_fs = Some(PendingFs {
                                    vmnt: vmnt.0,
                                    fs_e: node.fs_e,
                                    worker: idx,
                                    grant,
                                    user,
                                    req: crate::request::encode_rdlink(
                                        node.ino,
                                        grant,
                                        buf_size as usize,
                                    ),
                                });
                                continue;
                            }
                            crate::worker::PathFollow::Access { user: _acc_user, access } => {
                                // C `do_access`（protect.c:216-231）的本地半：
                                // 走完就判权限——`forbidden` 用**真实** uid/gid
                                // （protect.c:255-256 的 `job_call_nr ==
                                // VFS_ACCESS` 特例），根用户另有 rwx 全给的面。
                                let _ = _acc_user;
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) =
                                    match fp_slot.and_then(|s| self.fproc_table.get(s)) {
                                        Some(fp) => (
                                            fp.real_uid,
                                            fp.real_gid,
                                            fp.eff_uid,
                                            fp.eff_gid,
                                            fp.supplemental_groups[..fp.ngroups.min(16)]
                                                .to_vec(),
                                        ),
                                        None => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EINVAL,
                                            );
                                            continue;
                                        }
                                    };
                                let readonly_fs = self
                                    .vmnt_table
                                    .find_by_fs(node.fs_e)
                                    .and_then(|v| self.vmnt_table.get(v))
                                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                                    .unwrap_or(false);
                                let verdict = crate::protect::forbidden_decision(
                                    &crate::protect::ForbidInput {
                                        real_uid,
                                        real_gid,
                                        eff_uid,
                                        eff_gid,
                                        is_access_call: true,
                                        file_uid: node.uid,
                                        file_gid: node.gid,
                                        mode: node.mode,
                                        access: access as u8,
                                        is_dir: node.mode & crate::open::S_IFMT
                                            == crate::open::S_IFDIR,
                                        supp: &supp,
                                        readonly_fs,
                                    },
                                );
                                let status = match verdict {
                                    Ok(()) => 0,
                                    Err(e) => e.to_errno(),
                                };
                                self.finish_worker_job(idx, fp_slot, status);
                                continue;
                            }
                            crate::worker::PathFollow::Open { user: _open_user, oflags } => {
                                let _ = _open_user;
                                self.finish_open_local(idx, fp_slot, &node, oflags);
                                continue;
                            }
                            crate::worker::PathFollow::Creat {
                                    user,
                                    oflags,
                                    mode,
                                    path: _creat_path,
                                } => {
                                // `path` 只在阶段 1 失败（ENOENT）时用得到，
                                // 那条路走的是 Err 分支里的同一字段。
                                // 阶段 1 走通＝文件已存在：`O_EXCL` 即
                                // EEXIST（C open.c:118-120 的 `exist` 判定），
                                // 否则按普通 open 收尾。
                                let _ = user;
                                if oflags & crate::open::OpenFlags::EXCL.bits() != 0 {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EEXIST);
                                    continue;
                                }
                                let _ = mode;
                                self.finish_open_local(idx, fp_slot, &node, oflags);
                                continue;
                            }
                            crate::worker::PathFollow::CreatInDir { user, oflags, mode, entry } => {
                                // 阶段 2 走通（父目录在）：发 `REQ_CREATE`
                                // （C `new_node` → `req_create`）。
                                let (grant, name_len) = {
                                    let Some(wp) = self.worker_pool.get_mut(idx) else {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    };
                                    let bytes = entry.as_bytes();
                                    let n = bytes.len().min(crate::path::PATH_MAX - 1);
                                    wp.path_scratch[..n].copy_from_slice(&bytes[..n]);
                                    wp.path_scratch[n] = 0;
                                    let addr = wp.path_scratch.as_ptr() as u64;
                                    let len = n + 1;
                                    let grant = self.grants.grant_direct(
                                        &minix_sys::syscall::DirectKernelCallTransport,
                                        node.fs_e.get(),
                                        addr,
                                        len as u64,
                                        minix_types::CpFlags::READ,
                                    );
                                    match grant {
                                        Ok(g) => (g, len),
                                        Err(_) => {
                                            self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                            continue;
                                        }
                                    }
                                };
                                let vmnt = match self.vmnt_table.find_by_fs(node.fs_e) {
                                    Some(v) => v.0,
                                    None => {
                                        let _ = self.revoke_grant(grant);
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    }
                                };
                                let (uid, gid) = match fp_slot.and_then(|s| self.fproc_table.get(s)) {
                                    Some(fp) => (fp.eff_uid, fp.eff_gid),
                                    None => {
                                        let _ = self.revoke_grant(grant);
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                                        continue;
                                    }
                                };
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont = Some(crate::worker::WorkerCont::Create {
                                        user,
                                        oflags,
                                    });
                                }
                                self.pending_fs = Some(PendingFs {
                                    vmnt,
                                    fs_e: node.fs_e,
                                    worker: idx,
                                    grant,
                                    user,
                                    req: crate::request::encode_create(
                                        node.ino, grant, mode, uid, gid, name_len,
                                    ),
                                });
                                continue; // 等 REQ_CREATE 的回复（Create 续接）
                            }
                            crate::worker::PathFollow::Mkdir { user, entry, mode } => {
                                // C `do_mkdir`（open.c:564-598）：走到的必须是
                                // 目录（否则 ENOTDIR）→ 权限门（W|X）→
                                // `req_mkdir(父 ino, lastc, uid, gid, bits)`。
                                let ftype = node.mode & crate::open::S_IFMT;
                                if ftype != crate::open::S_IFDIR {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENOTDIR);
                                    continue;
                                }
                                // 权限门：`forbidden(fp, vp, W_BIT|X_BIT)`
                                // （C :589）。
                                let (real_uid, eff_uid, real_gid, eff_gid, supp) = match fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (
                                        fp.real_uid,
                                        fp.eff_uid,
                                        fp.real_gid,
                                        fp.eff_gid,
                                        fp.supplemental_groups[..fp.ngroups.min(16)]
                                            .to_vec(),
                                    ),
                                    None => {
                                        self.finish_worker_job(
                                            idx,
                                            fp_slot,
                                            minix_types::EINVAL,
                                        );
                                        continue;
                                    }
                                };
                                let readonly_fs = self
                                    .vmnt_table
                                    .find_by_fs(node.fs_e)
                                    .and_then(|v| self.vmnt_table.get(v))
                                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                                    .unwrap_or(false);
                                let forbid = crate::protect::forbidden_decision(
                                    &crate::protect::ForbidInput {
                                        real_uid,
                                        real_gid,
                                        eff_uid,
                                        eff_gid,
                                        is_access_call: false,
                                        file_uid: node.uid,
                                        file_gid: node.gid,
                                        mode: node.mode,
                                        access: (crate::open::W_BIT | crate::open::X_BIT) as u8,
                                        is_dir: true,
                                        supp: &supp,
                                        readonly_fs,
                                    },
                                );
                                if let Err(e) = forbid {
                                    self.finish_worker_job(
                                        idx,
                                        fp_slot,
                                        e.to_errno(),
                                    );
                                    continue;
                                }
                                // 最后组件名进槽内 scratch + NUL，授权给
                                // FS 读（C `req_mkdir` 的 `cpf_grant_direct`）。
                                let (grant, name_len) = {
                                    let Some(wp) = self.worker_pool.get_mut(idx) else {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    };
                                    let bytes = entry.as_bytes();
                                    let n = bytes.len().min(crate::path::PATH_MAX - 1);
                                    wp.path_scratch[..n].copy_from_slice(&bytes[..n]);
                                    wp.path_scratch[n] = 0;
                                    let addr = wp.path_scratch.as_ptr() as u64;
                                    let len = n + 1;
                                    let grant = self
                                        .grants
                                        .grant_direct(
                                            &minix_sys::syscall::DirectKernelCallTransport,
                                            node.fs_e.get(),
                                            addr,
                                            len as u64,
                                            minix_types::CpFlags::READ,
                                        );
                                    match grant {
                                        Ok(g) => (g, len),
                                        Err(_) => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EIO,
                                            );
                                            continue;
                                        }
                                    }
                                };
                                let _ = name_len;
                                let vmnt = match self.vmnt_table.find_by_fs(node.fs_e) {
                                    Some(v) => v.0,
                                    None => {
                                        let _ = self.revoke_grant(grant);
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    }
                                };
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont = Some(crate::worker::WorkerCont::Status);
                                }
                                self.pending_fs = Some(PendingFs {
                                    vmnt,
                                    fs_e: node.fs_e,
                                    worker: idx,
                                    grant,
                                    user,
                                    req: crate::request::encode_mkdir(
                                        node.ino, grant, mode, eff_uid, eff_gid,
                                    ),
                                });
                                continue; // 等 REQ_MKDIR 的回复（Status 续接收尾）
                            }
                            crate::worker::PathFollow::Stat { user, buf } => {
                                // 相位 2：grant 用户 stat 缓冲 → REQ_STAT →
                                // 续接交棒给 Fstat（已落的那条）。
                                let grant = match self.grant_user_buffer(
                                    node.fs_e,
                                    user,
                                    buf,
                                    88, // LP64 struct stat
                                    minix_types::CpFlags::WRITE | minix_types::CpFlags::TRY,
                                ) {
                                    Ok(g) => g,
                                    Err(_) => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    }
                                };
                                let vmnt = match self.vmnt_table.find_by_fs(node.fs_e) {
                                    Some(v) => v.0,
                                    None => {
                                        let _ = self.revoke_grant(grant);
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    }
                                };
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont = Some(crate::worker::WorkerCont::Fstat { grant });
                                }
                                self.pending_fs = Some(PendingFs {
                                    vmnt,
                                    fs_e: node.fs_e,
                                    worker: idx,
                                    grant,
                                    user,
                                    req: crate::request::encode_stat(node.ino, grant),
                                });
                                continue; // 等 stat 的回复（Fstat 续接收尾）
                            }
                        },
                        Err(e) => {
                            let status = match e {
                                PathFail::Path(p) => walk_err(p),
                                PathFail::Status(st) => st,
                            };
                            // `creat` 阶段 1 的 ENOENT → 阶段 2：**走父目录**
                            // （C `new_node` 的 `last_dir` 那一步）。其余错误
                            // 按状态收尾。
                            if status == minix_types::ENOENT
                                && let crate::worker::PathFollow::Creat {
                                    user,
                                    oflags,
                                    mode,
                                    path,
                                } = follow
                            {
                                match crate::path::last_dir_split(&path) {
                                    Ok(split) => {
                                        let resolve = match crate::path::Lookup::new(
                                            split.dir_path,
                                            crate::path::LookupFlags::NOFLAGS,
                                        ) {
                                            Ok(l) => l,
                                            Err(pe) => {
                                                self.finish_worker_job(
                                                    idx,
                                                    fp_slot,
                                                    pe.to_errno(),
                                                );
                                                continue;
                                            }
                                        };
                                        let rd = self.root_dir_of(fp_slot);
                                        let start = if resolve.path.starts_with('/') {
                                            crate::path::LookupStart {
                                                fs: rd.fs,
                                                ino: rd.ino,
                                                dev: rd.dev,
                                            }
                                        } else {
                                            let wd = fp_slot
                                                .map(|s| self.work_dir_of(s))
                                                .unwrap_or(rd);
                                            crate::path::LookupStart {
                                                fs: wd.fs,
                                                ino: wd.ino,
                                                dev: wd.dev,
                                            }
                                        };
                                        let uid = fp_slot
                                            .and_then(|s| self.fproc_table.get(s))
                                            .map(|fp| fp.eff_uid)
                                            .unwrap_or(0);
                                        let gid = fp_slot
                                            .and_then(|s| self.fproc_table.get(s))
                                            .map(|fp| fp.eff_gid)
                                            .unwrap_or(0);
                                        match crate::path::LookupWalk::begin(
                                            start, resolve, rd, uid, gid,
                                        ) {
                                            Ok((walk2, step2)) => {
                                                let crate::path::WalkStep::Send {
                                                    fs_e,
                                                    dir_ino,
                                                    root_ino,
                                                } = step2
                                                else {
                                                    self.finish_worker_job(
                                                        idx,
                                                        fp_slot,
                                                        minix_types::EIO,
                                                    );
                                                    continue;
                                                };
                                                if let Some(wp) =
                                                    self.worker_pool.get_mut(idx)
                                                {
                                                    wp.path = Some(
                                                        crate::worker::PathPending {
                                                            walk: walk2,
                                                            grant: 0,
                                                            follow: crate::worker::PathFollow::CreatInDir {
                                                                user,
                                                                oflags,
                                                                mode,
                                                                entry: split.entry,
                                                            },
                                                        },
                                                    );
                                                }
                                                if self
                                                    .send_lookup_for_slot(
                                                        idx, fp_slot, fs_e, dir_ino,
                                                        root_ino,
                                                    )
                                                    .is_err()
                                                {
                                                    if let Some(wp) =
                                                        self.worker_pool.get_mut(idx)
                                                    {
                                                        wp.path = None;
                                                        wp.cont = None;
                                                    }
                                                    self.finish_worker_job(
                                                        idx,
                                                        fp_slot,
                                                        minix_types::EIO,
                                                    );
                                                }
                                                continue;
                                            }
                                            Err(pe) => {
                                                self.finish_worker_job(
                                                    idx,
                                                    fp_slot,
                                                    pe.to_errno(),
                                                );
                                                continue;
                                            }
                                        }
                                    }
                                    Err(pe) => {
                                        self.finish_worker_job(idx, fp_slot, pe.to_errno());
                                        continue;
                                    }
                                }
                            }
                            self.finish_worker_job(idx, fp_slot, status);
                            continue;
                        }
                    }
                }
                crate::worker::WorkerCont::Transfer {
                    grant,
                    filp,
                    vnode,
                    orig_pos,
                    write,
                } => {
                    let _ = self.revoke_grant(grant);
                    if status == 0 {
                        // C `req_readwrite_actual` 的成功半：从**回复**取
                        // `seek_pos`/`nbytes`（`mess_fs_vfs_readwrite` —
                        // ipc.h:214-220，偏移取共享权威表），位置写回 filp，
                        // 状态给用户的是"实际读到的字节数"（C 的 `cum_io`）。
                        // SAFETY: 回复载荷按 LP64 域序写在负载区。
                        let raw = unsafe { &reply.m_u.raw };
                        let mut b8 = [0u8; 8];
                        b8.copy_from_slice(
                            &raw[minix_types::transfer_reply_off::SEEK_POS
                                ..minix_types::transfer_reply_off::SEEK_POS + 8],
                        );
                        let new_pos = i64::from_le_bytes(b8);
                        b8.copy_from_slice(
                            &raw[minix_types::transfer_reply_off::NBYTES
                                ..minix_types::transfer_reply_off::NBYTES + 8],
                        );
                        let nbytes = i64::from_le_bytes(b8);
                        if let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(filp)) {
                            f.pos = new_pos;
                        }
                        // C read.c:255-259 —— 写方向且是常规文件/目录时，
                        // 新位置越过旧大小即抬高（读方向不动大小）。
                        if write
                            && let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode))
                            && (v.mode & crate::open::S_IFMT == crate::open::S_IFREG
                                || v.mode & crate::open::S_IFMT == crate::open::S_IFDIR)
                            && new_pos > 0
                            && (new_pos as u64) > v.size
                        {
                            v.size = new_pos as u64;
                        }
                        status = nbytes as i32;
                        let _ = orig_pos; // 位置已由回复给出，原值只作对账
                    }
                }
            }
            // 完成：清槽 + 回用户（目标取 fproc 的端点）。
            if let Some(wp) = self.worker_pool.get_mut(idx) {
                wp.cont = None;
                wp.sendrec = None;
                wp.task = None;
            }
            self.worker_pool.release(idx);
            if self.current_worker == Some(idx) {
                self.current_worker = None;
            }
            if let Some(fp_slot) = fp_slot
                && let Some(fp) = self.fproc_table.get(fp_slot)
            {
                let target = fp.endpoint;
                match reply_payload {
                    Some(m) => self.queue_reply_msg(target, m),
                    None => {
                        let result = if status == 0 {
                            crate::call_table::SyscallResult::Ok(0)
                        } else {
                            crate::call_table::SyscallResult::Error(status)
                        };
                        self.queue_reply(target, result);
                    }
                }
            }
        }
    }

    /// `get_work:590` reviving fast-path — if `reviving>0` find first
    /// `FP_REVIVED` slot, otherwise return `None` (caller should `receive`).
    pub fn next_reviving_slot(&self) -> Option<UserSlot> {
        if self.reviving == 0 {
            return None;
        }
        for idx in 0..minix_types::NR_PROCS {
            let slot = UserSlot::new(idx);
            let Some(fp) = self.fproc_table.get(slot) else {
                continue;
            };
            if fp.pid != PID_FREE && fp.flags.contains(FpFlags::REVIVED) {
                return Some(slot);
            }
        }
        None
    }

    /// `unblock:921` — reconstruct the original request for a revived `fproc`.
    ///
    /// `PIPE` → `QueuedPipe` (needs `do_pending_pipe` worker), `FLOCK` → `RevivedLock`.
    pub fn unblock(&mut self, slot: UserSlot) -> Result<UnblockOutcome, UnblockError> {
        let fp = self
            .fproc_table
            .get_mut(slot)
            .ok_or(UnblockError::SlotFree)?;
        if fp.pid == PID_FREE {
            return Err(UnblockError::SlotFree);
        }
        let blocked = fp.blocked_on;
        if blocked == BlockedOn::None {
            return Err(UnblockError::NotBlocked);
        }

        // Reconstruct is modelled by changing blocked_on / flags; real
        // message reconstruction (`m_in.m_source = endpoint; switch...`) is
        // represented by the `UnblockOutcome` variant.
        match blocked {
            BlockedOn::Pipe(_) => {
                fp.blocked_on = BlockedOn::None;
                fp.flags.remove(FpFlags::REVIVED);
                assert!(self.reviving > 0);
                self.reviving -= 1;
                Ok(UnblockOutcome::QueuedPipe { slot })
            }
            BlockedOn::Flock(_) => {
                fp.blocked_on = BlockedOn::None;
                fp.flags.remove(FpFlags::REVIVED);
                assert!(self.reviving > 0);
                self.reviving -= 1;
                // `main.c:971 fp = rfp; return TRUE` — revive reuses the
                // current fp global.  We model as `current_fp_slot = slot`.
                self.current_fp_slot = Some(slot);
                Ok(UnblockOutcome::RevivedLock { slot })
            }
            _ => Err(UnblockError::UnknownBlockedOn(match blocked {
                BlockedOn::PipeOpen(_) => 3,
                BlockedOn::Select => 4,
                BlockedOn::Cdev(_) => 5,
                BlockedOn::Sdev(_) => 6,
                _ => 0,
            })),
        }
    }

    /// Enqueue a revive — `pipe.c:revive` / `select.c:select_return` set
    /// `fp_flags |= FP_REVIVED` + `reviving++`.
    ///
    /// `ARCH A-5`: `SUSPEND → reviving` is typed here.
    pub fn enqueue_revive(&mut self, slot: UserSlot) -> Result<(), UnblockError> {
        let fp = self
            .fproc_table
            .get_mut(slot)
            .ok_or(UnblockError::SlotFree)?;
        if fp.flags.contains(FpFlags::REVIVED) {
            return Ok(());
        }
        fp.flags.insert(FpFlags::REVIVED);
        self.reviving += 1;
        Ok(())
    }

    /// `poll_next` — typed `get_work:580` dual return.
    ///
    /// If a revived slot exists, `unblock` it and return `Revived*`.
    /// Otherwise the caller should `receive` (we return `Ready` as a
    /// placeholder for “receive then route”).
    pub fn poll_next(&mut self) -> PollResult {
        if let Some(slot) = self.next_reviving_slot() {
            match self.unblock(slot) {
                Ok(UnblockOutcome::QueuedPipe { slot }) => PollResult::RevivedPipe { slot },
                Ok(UnblockOutcome::RevivedLock { slot }) => PollResult::RevivedLock { slot },
                Err(_) => PollResult::Ready,
            }
        } else {
            PollResult::Ready
        }
    }

    /// `reply:638` — non-blocking `ipc_sendnb` stub.
    ///
    /// Real kernel `ipc_sendnb` is DEFERRED; the single-threaded loop
    /// models it as a `ReplySink` trait so tests can inject `BlackHole`.
    pub fn reply(&self, target: Endpoint, result: i32) -> ReplyIntent {
        // In the real loop `reply` would `sendnb`; here we just model the
        // intent.  The caller decides `Reply` vs `ReplyLater`.
        if target == Endpoint::NONE {
            ReplyIntent::NoReply
        } else {
            ReplyIntent::Reply(result)
        }
    }

    /// `replycode:655` — `memset + reply`.
    pub fn reply_code(&self, target: Endpoint, result: i32) -> ReplyIntent {
        self.reply(target, result)
    }

    /// 把一次分发的 [`SyscallResult`](crate::call_table::SyscallResult)
    /// 折成回复并入队（W3 回复半）。
    ///
    /// 映射照 C 的 `do_work` 尾部 `reply(who_e, result)`：
    /// `Ok(v)`/`Error(e)` 都直接当 `m_type` 发（errno 是**正值**——
    /// 与 PM/RS/DS 各服务的应答约定一致），`Nosys` 显式回 `ENOSYS`
    /// （main.c:283-294 的不可解析调用号），`Suspend` 不入队（回复在
    /// 该请求的 FS 应答落地时发）。目标为 `NONE` 或调用方无槽位时不发。
    pub fn queue_reply(
        &mut self,
        target: Endpoint,
        result: crate::call_table::SyscallResult,
    ) {
        use crate::call_table::SyscallResult;
        if target == Endpoint::NONE || target.to_user_slot().is_none() {
            return;
        }
        let code = match result {
            SyscallResult::Ok(v) => v,
            SyscallResult::Error(e) => e,
            SyscallResult::Nosys => minix_types::ENOSYS,
            SyscallResult::Suspend => return,
        };
        // 臂已经自己入过一条**带载荷**的回复（如 `lseek` 位置不变那条路把
        // 新位置写进 `m_vfs_lc_lseek.offset`，C `open.c:665-666`）时，这条
        // 统一收尾的裸回复**不能覆盖它**——覆盖掉用户拿到的就是偏移 0。
        // 一轮只该有一条回复：先入队者为准。
        if self.pending_reply.is_some() {
            return;
        }
        self.queue_reply_msg(target, Message { m_type: code, ..Message::default() });
    }

    /// 入队一条**带载荷**的回复（C 的 `job_m_out` 在返回前被臂填字段，
    /// 如 `lseek` 把新位置写进 `m_vfs_lc_lseek.offset`）。
    pub fn queue_reply_msg(&mut self, target: Endpoint, msg: Message) {
        if target == Endpoint::NONE || target.to_user_slot().is_none() {
            return;
        }
        self.pending_reply = Some((target, msg));
    }

    /// 取走待发回复（`run()` 每轮循环尾调用）。
    pub fn take_reply(&mut self) -> Option<(Endpoint, Message)> {
        self.pending_reply.take()
    }

    /// 给 FS 发一张 **magic grant**：把 `user_e`:`addr` 起的 `bytes` 字节
    /// 授权给 `fs_e` 读写。
    ///
    /// C: `cpf_grant_magic(fs_e, user_e, addr, bytes, access)` ——
    /// `request.c:844`（read/write）与 `:1087`（stat）都是这个形状；权限位
    /// 由调用方给（读是 `CPF_WRITE`——FS 往用户缓冲写；带 `CPF_TRY` 的那一半
    /// 由 `GrantScope` 表达，`ERESTART` 重发路径见 `req_stat`/`req_readwrite`）。
    ///
    /// 失败面：表满/内核调用失败即 `Err`——C 在这两处 `panic`
    /// （"cpf_grant_* failed"），Rust 让臂自己决定（C 语义上这是内部错误，
    /// 臂按 `EIO` 回用户）。
    pub fn grant_user_buffer(
        &mut self,
        fs_e: Endpoint,
        user_e: Endpoint,
        addr: u64,
        bytes: u64,
        access: minix_types::CpFlags,
    ) -> Result<i32, i32> {
        self.grants.grant_magic(
            &minix_sys::syscall::DirectKernelCallTransport,
            fs_e.get(),
            user_e.get(),
            addr,
            bytes,
            access,
        )
    }

    /// 撤销一张 grant（C `cpf_revoke`）。返回值按 C 的调用点语义：只要
    /// 撤销本身不报错就回 `Ok`；`GRANT_FAULTED` 由调用方比对
    /// （C 的 `if (cpf_revoke(grant_id) == GRANT_FAULTED) return ERESTART;`）。
    pub fn revoke_grant(&mut self, grant: i32) -> Result<i32, i32> {
        self.grants.revoke(grant)
    }

    /// `fs_sendrec`（comm.c:134-170）的对话原语——syscall 臂的进入半。
    ///
    /// 与 C 六步对应：`find_vmnt(fs_e)` 校验（`vmnt` 存在且 `fs` 指向
    /// 同一端点，不符即 `EIO`，comm.c:137-140）→ `EDEADLK` 自死锁守门
    /// （调用进程本身就是该 FS，comm.c:142-144）→ `assert(w_sendrec ==
    /// NULL)`（槽不得已在等待，comm.c:146）→ `w_sendrec` 挂接
    /// （[`crate::worker::WorkerSlot::set_waiting`]）→ 窗口二守门下的
    /// `sendmsg`/`queuemsg`（经 [`FsTransport::send_fs`]）。C 的
    /// `worker_wait` 在单线程模型中即"臂返回 Suspend、槽停在
    /// `WaitingForFs`"，回复由 [`Self::handle_fs_reply`] 落地。
    ///
    /// 发送失败时槽的等待标记不清除——与 C 一致（`sendmsg` 失败后
    /// `w_sendrec` 留待作业结束的 `worker_release` 清理），调用方臂
    /// 收到 `Err` 后走错误回复并释放槽。
    pub fn fs_sendrec<T: FsTransport>(
        &mut self,
        transport: &mut T,
        vmnt: usize,
        fs_ep: Endpoint,
        slot: crate::fs_comm::SlotId,
        req: &Message,
    ) -> Result<(), CommError> {
        let v = self
            .vmnt_table
            .get(crate::vmnt::VmntId(vmnt))
            .ok_or(CommError::NoVmnt)?;
        if v.fs != fs_ep {
            return Err(CommError::NoVmnt);
        }
        // comm.c:142-144 — `if (fs_e == fp->fp_endpoint) return EDEADLK`。
        if let Some(fp_slot) = self.worker_pool.get(slot).and_then(|w| w.fp_slot)
            && let Some(fp) = self.fproc_table.get(fp_slot)
            && fp.endpoint == fs_ep
        {
            return Err(CommError::Deadlock);
        }
        {
            let w = self
                .worker_pool
                .get_mut(slot)
                .ok_or(CommError::NoWorker)?;
            // comm.c:146 — `assert(self->w_sendrec == NULL)`。
            assert!(
                w.state != crate::worker::WorkerState::WaitingForFs,
                "fs_sendrec on already-waiting slot (C comm.c:146 assert)"
            );
            w.set_waiting(fs_ep, *req);
        }
        transport.send_fs(vmnt, fs_ep, slot, req, &mut self.comm).map(|_| ())
    }

    /// `send_work`（comm.c:37-47）× `fs_sendmore`（comm.c:66-87）的
    /// 真发送半——窗口放行后把排队的请求补发出去。
    ///
    /// C 的补发由作业结束的 worker 调 `send_work()` 驱动；单线程模型
    /// 下由主循环层在回复落地后调用（`run()` 持 transport 时），本原语
    /// 独立可测。`GlobalComm::fs_sendmore` 承载 C 的守门序与
    /// `pop + sending--`（`c_cur_reqs++` 由其内联）；随后按 `sendmsg`
    /// 的 transid 戳 + `asynsend3` 投递——发送失败以 `(void)` 忽略继续
    /// （comm.c:86 的既有行为，计数已先行递增）。返回实际补发条数。
    pub fn flush_send_queue<T: minix_sys::ipc::IpcTransport>(
        &mut self,
        transport: &mut T,
    ) -> usize {
        let mut sent = 0;
        for idx in 0..crate::vmnt::NR_MNTS {
            // C fs_sendmore:79 `if (vmp->m_fs_e == NONE) return` 的
            // 对应物——空挂载槽不扫。
            if self
                .vmnt_table
                .get(crate::vmnt::VmntId(idx))
                .is_none_or(|v| v.fs == Endpoint::NONE)
            {
                continue;
            }
            while let Some(slot) = self.comm.fs_sendmore(idx) {
                let Some(worker) = self.worker_pool.get(slot) else {
                    continue;
                };
                let Some(req) = worker.sendrec else {
                    continue;
                };
                let fs_ep = self
                    .vmnt_table
                    .get(crate::vmnt::VmntId(idx))
                    .map(|v| v.fs)
                    .unwrap_or(Endpoint::NONE);
                // C sendmsg 的 `TRNS_ADD_ID` 戳（此处补戳排队时未 stamp
                // 的槽内请求）+ `asynsend3`（`(void)` 忽略失败）。
                let mut out = req;
                out.m_type = crate::fs_comm::TransId::add(req.m_type as u32, slot) as i32;
                if transport.sendnb(fs_ep, &out).is_ok() {
                    sent += 1;
                }
            }
        }
        sent
    }

    /// `do_reply:187`（main.c）——FS 回复落地到等待中的 worker 槽。
    ///
    /// C 步骤逐条对应：
    /// 1. `transid` 解码出槽号（`TRNS_GET_ID` + `IS_VFS_FS_TRANSID`）。
    /// 2. `who_e != VM_PROC_NR && find_vmnt(who_e)==NULL → panic`
    ///    （main.c:190-191）——VM 回复跳过 vmnt 查找；找不到挂载点即
    ///    fail-fast（同 C 的 panic 文案）。
    /// 3. `wp->w_task != who_e → printf+return`（main.c:193-196）——
    ///    typed 为 [`FsReplyError::WrongTask`]。
    /// 4. `*w_sendrec = m_in; w_sendrec 转为已交付`（main.c:204-205）——
    ///    槽模型的 `sendrec` 存储被回复覆写，续接侧从同槽读回结果
    ///    （C 的续接在 `fs_sendrec:164` 读 `reqmp->m_type`）。
    /// 5. `w_task = NONE; c_cur_reqs--`（main.c:206-208）——窗口放行。
    /// 6. `worker_signal(wp)`（main.c:209）——槽从 `WaitingForFs` 回
    ///    `Busy`（协程可运行；单线程模型下"可运行"即等待续接分派）。
    pub fn handle_fs_reply<C: TransIdCodec>(
        &mut self,
        msg: &Message,
        codec: &C,
    ) -> Result<usize, FsReplyError> {
        let transid_raw = (msg.m_type as u32) & 0xFFFF;
        let slot = codec
            .decode(transid_raw)
            .ok_or(FsReplyError::SpuriousTransid)?;
        if slot >= crate::worker::NR_WTHREADS {
            return Err(FsReplyError::SlotOutOfRange);
        }
        // C main.c:190 — VM 回复不经 vmnt 查找；其余找不到挂载点 panic。
        let vmnt_idx = if msg.m_source == Endpoint::VM {
            None
        } else {
            match self.vmnt_table.find_by_fs(msg.m_source) {
                Some(id) => Some(id.0),
                None => panic!(
                    "Couldn't find vmnt for endpoint {} (C main.c:191)",
                    msg.m_source.0
                ),
            }
        };
        let wp = self
            .worker_pool
            .get_mut(slot)
            .ok_or(FsReplyError::SlotOutOfRange)?;
        if wp.task != Some(msg.m_source) {
            // C main.c:193-196 — expected X to reply, not Y.
            return Err(FsReplyError::WrongTask);
        }
        // C main.c:88 —— `m_in.m_type = TRNS_DEL_ID(m_in.m_type)`：**进
        // do_reply 之前就把 transid 剥掉**，于是 `*w_sendrec` 里的
        // `m_type` 是服务端的原始状态（含 `EENTERMOUNT` 一类特殊码），
        // 续接体直接拿它判成功/失败/特殊码。
        let mut stripped = *msg;
        stripped.m_type = minix_types::trns_del_id(msg.m_type);
        // C main.c:204-206 — `*w_sendrec = m_in` 后清 w_task；槽模型里
        // "是否在等"由 task 承载，回复体留在 sendrec 供续接读取。
        wp.sendrec = Some(stripped);
        wp.task = None;
        if let Some(idx) = vmnt_idx {
            debug_assert!(
                self.comm.vmnts[idx].cur_reqs > 0,
                "c_cur_reqs underflow (C main.c:207 pairing)"
            );
            self.comm.vmnts[idx].cur_reqs -= 1;
        }
        // C main.c:209 worker_signal —— 线程从 worker_wait 返回。
        wp.state = crate::worker::WorkerState::Busy;
        Ok(slot)
    }
}

impl Default for VfsState {
    fn default() -> Self {
        Self::new()
    }
}

/// 路径续走的两种失败源（状态机错误 vs FS 回复的原始状态）。
enum PathFail {
    /// `LookupWalk` 的错误（`PathError`）。
    Path(crate::path::PathError),
    /// FS 回复的状态不是 OK/三特殊码（普通错误）。
    Status(i32),
}

impl From<crate::path::PathError> for PathFail {
    fn from(e: crate::path::PathError) -> Self {
        Self::Path(e)
    }
}

/// 一条待发的 FS 对话（见 [`VfsState::flush_pending_fs`]）。
#[derive(Debug, Clone, Copy)]
pub struct PendingFs {
    /// 目标挂载窗口（`vmnt` 下标）。
    pub vmnt: usize,
    /// 目标文件系统端点。
    pub fs_e: Endpoint,
    /// 该作业占用的 worker 槽下标。
    pub worker: usize,
    /// 已发给 FS 的 magic grant（发送失败时由循环撤销）。
    pub grant: i32,
    /// 用户端点（回复目的地）。
    pub user: Endpoint,
    /// 请求消息（`REQ_*`）。
    pub req: Message,
}

/// SEF 循环的 IPC 适配:`SefIpc` 只需要 receive/notify 两动词,由
/// trap 直连传输承载(VM 的 `SefAdapter` 同形;S13 W4 接线)。
pub struct VfsIpc {
    inner: minix_sys::ipc::DirectTrapTransport,
}

impl VfsIpc {
    pub const fn new() -> Self {
        Self { inner: minix_sys::ipc::DirectTrapTransport }
    }
}

impl Default for VfsIpc {
    fn default() -> Self {
        Self::new()
    }
}

impl minix_sef::SefIpc for VfsIpc {
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        let sts = self.inner.receive(src, msg).map_err(|t| t.0)?;
        Ok(sts.0 as i32)
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        self.inner.notify(dest).map_err(|t| t.0)
    }
}

/// VFS main loop.
///
/// Corresponds to Minix3's `main()` function (main.c:54-118): SEF 启动 →
/// 握手阻塞循环(main.c:410-436)→ `sef_receive(ANY)` 主循环(main.c:601)。
/// 生产回复发送（C `reply` 的 `ipc_sendnb` 半，main.c）。
///
/// 应答只带 `m_type`（C 的 `reply` 同样是 `memset(&m, 0, sizeof(m));
/// m.m_type = result` 的裸回复）——数据面早已由各臂自己拷出或经
/// FS 应答落地。
fn send_reply(target: Endpoint, reply: Message) {
    use minix_sys::ipc::IpcTransport;
    // 非阻塞发（C `ipc_sendnb`）：调用方在 sendrec 里等着，不会拒绝接收。
    let _ = minix_sys::ipc::DirectTrapTransport.sendnb(target, &reply);
}

pub fn run() -> ! {
    let mut state = VfsState::new();
    state.init_fresh();
    let mut ipc = VfsIpc::new();

    // 启动握手(main.c:410-436):`sef_receive(PM_PROC_NR)` do-while——
    // 每条 VFS_PM_INIT 填一个 fproc 槽,endpoint==NONE 终止。
    loop {
        let mut msg = Message::default();
        let recv = minix_sef::sef_receive_status(&mut ipc, Endpoint::PM, &mut msg, &mut |_| {})
            .unwrap_or_else(|e| panic!("vfs: handshake receive failed: {e}"));
        // NONE 终止符也经 step:状态机在此完成 PmHandshake→InitTables
        // 转换(与既有握手测试的 complete 语义一致)。
        let complete = state
            .pm_handshake_step(&recv.message)
            .unwrap_or_else(|e| panic!("vfs: handshake rejected message: {e:?}"));
        if complete {
            break;
        }
    }
    // C main.c:435-436 — `mess.m_type = OK; ipc_send(PM_PROC_NR, &mess)`：
    // 进程表收齐后把成功回给 PM（PM 侧 `vfs_init_sync` 的末条是 sendrec
    // 屏障，等的就是这一条——不发则 PM 启动链停在这里）。
    send_reply(
        Endpoint::PM,
        Message { m_type: minix_types::OK, ..Message::default() },
    );
    state.finish_init();

    // 启动段(main.c:441):向 DS 订阅驱动上线事件(失败远端忽略)。
    // C 进程启动时的 grant 表注册（`sys_setgrant`，safecopies.c 的
    // `grants` 全局在这里告诉内核表位置）；失败无恢复面，仅记录。
    let _ = state
        .grants
        .register(&minix_sys::syscall::DirectKernelCallTransport);

    let mut ds = minix_sys::ds::DsClient::new(
        minix_sys::ipc::DirectTrapTransport,
        minix_sys::syscall::DirectKernelCallTransport,
        Endpoint::DS,
    );
    let _ = ds.subscribe("drv\\.[bc]..\\..*", {
        (minix_types::DsFlags::INITIAL | minix_types::DsFlags::OVERWRITE).bits() as i32
    });

    loop {
        // C main.c:601-602 — sef_receive(ANY):ping 拦截在 SEF 层完成。
        let mut msg = Message::default();
        let recv = minix_sef::sef_receive_status(&mut ipc, Endpoint::ANY, &mut msg, &mut |_| {
            // C VFS 未注册 signal handler(main.c:374-388)——库默认忽略面。
        })
        .unwrap_or_else(|e| panic!("vfs: receive failed: {e}"));
        match recv.event {
            SefEvent::Call(_) => {
                let codec = VfsTransIdCodec;
                let _ = state.run_once(&recv.message, &codec);
                // W3 回复半：本轮分发的回复在循环尾发出（C `do_work`
                // 尾部的 `reply(who_e, result)`）。
                if let Some((target, reply)) = state.take_reply() {
                    send_reply(target, reply);
                }
                // 臂登记的 FS 对话在这一段发出（生产传输：trap 直连 +
                // grant 已由臂发出）。发送失败在 flush 内部收尾。
                let mut fs_ipc = crate::fs_comm::IpcFsTransport {
                    transport: minix_sys::ipc::DirectTrapTransport,
                };
                state.flush_pending_fs(&mut fs_ipc);
            }
            SefEvent::Signal(_) => {}
            // init_restart ≡ init_fresh(已文档化);LU prepare/rollback 的
            // 决策函数就位,RS 推进面挂通电。
            SefEvent::Init(_) => state.init_fresh(),
            SefEvent::PingInvalid => {}
        }
    }
}

/// 热身 grant 表（测试专用）：宿主构建下 `sys_setgrant` 不可达，**每次表
/// 增长后的第一次分配必然失败**，之后再分配就成功（失败路径已经把 freelist
/// 铺好了）。臂测试要断言"真正登记出去的请求"，就得先把这一步趟平——连续做
/// 几轮覆盖多次增长，并把热身用的槽**撤销回 freelist**（不撤销的话表刚好被
/// 热身占满，测试自己的 grant 又要触发一次增长 → 又撞上那个失败）。
/// 测试专用：把状态铺成"臂真的能发出请求"的基线——挂载行（fs=MFS、dev 非
/// NO_DEV）、调用方的根/工作目录 vnode（`root_dir_of`/`work_dir_of` 的输入）、
/// 以及热身过的 grant 表。路径臂的入口测试都要从这里起步，否则会停在
/// "根目录没设"或"grant 没热"上，断言虽然也是 EIO 但**理由不对**。
#[cfg(test)]
pub(crate) fn seed_ready_state(state: &mut VfsState) {
    {
        let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
        v.fs = Endpoint::MFS;
        v.dev = 1;
    }
    let vid = state.vnode_table.alloc().unwrap();
    {
        let vn = state.vnode_table.get_mut(vid).unwrap();
        vn.fs = Endpoint::MFS;
        vn.ino = 1;
        vn.dev = 1;
        vn.mode = crate::open::S_IFDIR | 0o755;
        vn.ref_count = 1;
    }
    if let Some(fp) = state.fproc_table.get_mut(minix_types::UserSlot::new(0)) {
        fp.root_dir = Some(vid.get());
        fp.work_dir = Some(vid.get());
    }
    warm_grants(state);
}

#[cfg(test)]
pub(crate) fn warm_grants(state: &mut VfsState) {
    let mut ids = alloc::vec::Vec::new();
    for _ in 0..8 {
        if let Ok(g) = state.grants.grant_direct(
            &minix_sys::syscall::DirectKernelCallTransport,
            Endpoint::MFS.get(),
            0x1000,
            8,
            minix_types::CpFlags::READ,
        ) {
            ids.push(g);
        }
    }
    for g in ids {
        let _ = state.grants.revoke(g);
    }
}

#[cfg(test)]
mod tests {
    use crate::call_table::VfsCallNum;
    use super::*;
    use crate::worker::WorkerState;
    use minix_types::{Endpoint, VFS_PM_INIT};

    /// 播种挂载点 0:fs=MFS、dev 非 NO_DEV(find_by_fs 的双条件)。
    fn seed_vmnt0(state: &mut VfsState) {
        let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
        v.fs = Endpoint::MFS;
        v.dev = 1; // DevId(u64),非 NO_DEV 即可
    }


    /// 播种 worker 槽并置 `WaitingForFs`(fs_sendrec 的 sendmsg 半)。
    fn seed_waiting(state: &mut VfsState, slot: usize, task: Endpoint) {
        let req = Message { m_type: 0x503, ..Message::default() };
        state.worker_pool.get_mut(slot).unwrap().set_waiting(task, req);
    }

    /// 构造 FS 回复消息(m_type 高 16 请求号、低 16 transid)。
    fn reply_msg(req: u32, slot: usize, source: Endpoint) -> Message {
        let mut m = Message { m_type: crate::fs_comm::TransId::add(req, slot) as i32, ..Message::default() };
        m.m_source = source;
        m
    }

    /// 挂载表组装（`mounted_fs_list`）与起点三元组（`root_dir_of` /
    /// `work_dir_of`）：从 `vmnt_table` + vnode 表读出的六个事实。
    #[test]
    fn test_mount_list_and_start_triples() {
        let mut state = VfsState::new();
        // 一个挂载行：fs=MFS，dev=7，root vnode=3（ino=30），挂载点 vnode=4
        // （ino=40, fs=VFS, dev=7）。
        let v_root = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(v_root).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 30;
            v.dev = 7;
            // `alloc` 取"ref_count == 0 的第一个槽"，不置 ref → 不抬引用
            // 的话下一次 alloc 会拿到同一个槽（`advance` 的用法是先 alloc
            // 再填再置 ref_count/fs_count）。
            v.ref_count = 1;
        }
        let v_mnt = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(v_mnt).unwrap();
            v.fs = Endpoint::VFS;
            v.ino = 40;
            v.dev = 7;
        }
        let vmnt = state.vmnt_table.alloc().unwrap();
        {
            let m = state.vmnt_table.get_mut(vmnt).unwrap();
            m.fs = Endpoint::MFS;
            m.dev = 7;
            m.root = Some(v_root.get());
            m.mounted_on = Some(v_mnt.get());
        }
        let mounts = state.mounted_fs_list();
        assert_eq!(mounts.len(), 1, "空行不进表");
        assert_eq!(mounts[0].fs, Endpoint::MFS);
        assert_eq!(mounts[0].dev, 7);
        assert_eq!(mounts[0].root, (30, 7));
        assert_eq!(mounts[0].mounted_on, Some((40, Endpoint::VFS, 7)));

        // 起点三元组：根与工作目录各自解到 (fs, ino, dev)。
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.root_dir = Some(v_root.get());
            fp.work_dir = Some(v_mnt.get());
        }
        let rd = state.root_dir_of(Some(slot));
        assert_eq!((rd.fs, rd.ino, rd.dev), (Endpoint::MFS, 30, 7));
        let wd = state.work_dir_of(slot);
        assert_eq!((wd.fs, wd.ino, wd.dev), (Endpoint::VFS, 40, 7));
    }

    /// `PathFollow::Mkdir` 的相位 2：父目录门（非目录 → ENOTDIR）→ 权限门
    /// （W|X，`forbidden_decision`）→ 组 `REQ_MKDIR`（组件名进槽内 scratch +
    /// 只读 grant）并挂起；续接交棒 `WorkerCont::Status`。
    #[test]
    fn test_path_follow_mkdir_gates_and_hands_off() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 0; // root：权限门必过
            fp.umask = 0o022;
        }
        // 挂载行（REQ_MKDIR 要经窗口；dev 必须是真设备号）。
        let vmnt = state.vmnt_table.alloc().unwrap();
        {
            let m = state.vmnt_table.get_mut(vmnt).unwrap();
            m.fs = Endpoint::MFS;
            m.dev = 7;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
        let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };

        // 情形一：走到的不是目录 → ENOTDIR（C open.c:587-588）。
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/d".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
        // SAFETY(test): mode = S_IFREG（非目录）。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFREG | 0o755).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 0,
                follow: PathFollow::Mkdir {
                    user,
                    entry: "new".to_string(),
                    mode: crate::open::S_IFDIR | 0o755,
                },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::ENOTDIR))
        );

        // 情形二：目录 + root：过门 → 组 `REQ_MKDIR`。宿主下要先热身 grant
        // 表（首次增长过不了 `sys_setgrant`，但失败路径已铺好 freelist），
        // 否则这条断言会退化成"第一次发请求必然失败"的顺序产物。
        warm_grants(&mut state);
        let idx2 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let (walk2, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/d".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        let mut reply2 = Message { m_type: minix_types::OK, ..Message::default() };
        // SAFETY(test): mode = S_IFDIR。
        unsafe {
            let raw = &mut reply2.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&9u64.to_le_bytes());
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFDIR | 0o755).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx2).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk: walk2,
                grant: 0,
                follow: PathFollow::Mkdir {
                    user,
                    entry: "new".to_string(),
                    mode: crate::open::S_IFDIR | 0o755,
                },
            });
            wp.sendrec = Some(reply2);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_MKDIR");
        assert_eq!(p.req.m_type, minix_types::REQ_MKDIR);
        assert_eq!(p.worker, idx2);
        // SAFETY(test): 按 mkdir_req_off 读回父目录 ino 与模式。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            assert_eq!(ino, 9, "inode 域是父目录");
            assert_eq!(mode, crate::open::S_IFDIR | 0o755);
        }
        // 回复到达：状态原样回用户，槽释放。
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx2).unwrap();
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert!(state.worker_pool.get_mut(idx2).unwrap().is_idle(), "槽已释放");
        let (t2, m2) = state.take_reply().expect("回复");
        assert_eq!(t2, user);
        assert_eq!(m2.m_type, 0, "REQ_MKDIR 成功 → 用户拿 0");
    }

    /// `WorkerCont::Status`：纯状态续接——回复的状态原样回给用户。
    #[test]
    fn test_worker_continuation_status_replies_verbatim() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Status);
            wp.sendrec = Some(Message { m_type: minix_types::EEXIST, ..Message::default() });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EEXIST))
        );
    }

    /// `PathFollow::Open` 的相位 2（`common_open` 的本地半）：走完 → 类型
    /// 分派 → fd/filp 装配 → 回 fd。**完全本地，宿主可测**——这是 Open 臂
    /// 里唯一不需要内核的部分，也正是最易错的部分（fd 与 filp 两处槽位
    /// 的认领顺序、O_CLOEXEC 位图）。
    #[test]
    fn test_path_follow_open_assembles_fd_and_filp() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        // 现场：一趟已走完的遍历（follow = Open，无 O_TRUNC）。
        let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
        let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        // 回复：OK + ino=5 + mode=S_IFREG|0644（四域 node_details）。
        let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
        // SAFETY(test): 按 lookup_reply_off 填 ino 与 mode。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&5u64.to_le_bytes());
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFREG | 0o644).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Open {
                    user,
                    oflags: 0, // O_RDONLY（access 位为 0）
                },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();

        // 回的是 fd（C `common_open` 返回 fd 即 syscall 结果）。
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, user);
        assert_eq!(reply.m_type, 0, "首个 fd 是 0");
        let fd = reply.m_type as usize;
        // fd 与 filp 两处都认领了：fproc 槽指向 filp，filp 带 vnode 与模式。
        let fp = state.fproc_table.get(slot).unwrap();
        let filp_id = fp.filps[fd].expect("fd 指向 filp");
        let f = state.filp_table.get(crate::filp::FilpId(filp_id)).unwrap();
        assert_eq!(f.vnode.is_some(), true, "filp_vno 已填（并进的 vnode）");
        assert_eq!(f.count, 1, "filp_count = 1（认领）");
        assert_eq!(f.mode & crate::open::R_BIT, crate::open::R_BIT, "只读打开");
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle());
    }

    /// `PathFollow::LinkSrc` → `LinkDst` 的转场 + `LinkDst` 的门与请求：
    /// C `do_link`（link.c:170-230）是**两段遍历**——先 `eat_path` 源文件、
    /// 再 `last_dir` 新名的父目录；跨设备即 EXDEV，父目录要 `W|X`，请求
    /// `REQ_LINK` 的两个 ino 顺序是**文件在前、目录在后**。
    #[test]
    fn test_path_follow_link_chain_and_gates() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mk = |state: &mut VfsState, eff_uid: u32| {
            let slot = minix_types::UserSlot::new(0);
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .unwrap();
            {
                let fp = state.fproc_table.get_mut(slot).unwrap();
                fp.endpoint = user;
                fp.pid = 100;
                fp.eff_uid = eff_uid;
                fp.eff_gid = eff_uid;
                fp.real_uid = eff_uid;
                fp.real_gid = eff_uid;
            }
            let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/s".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                0,
                0,
            )
            .unwrap();
            (idx, walk)
        };
        let done_reply = |ino: u64, mode: u32| {
            let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
            // SAFETY(test): 按 lookup_reply_off 填 ino/mode。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&ino.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&mode.to_le_bytes());
            }
            reply
        };
        let plant = |state: &mut VfsState, idx: usize, walk, reply: Message, follow: PathFollow| {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending { walk, grant: 9, follow });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        };

        // ① 阶段 1 走通（源文件在）→ 转阶段 2：现场换成 LinkDst，且已登记
        // 第二条 REQ_LOOKUP（走新名的父目录）。
        let mut state = VfsState::new();
        // 阶段 2 要从**进程根**重新起走，所以根目录 vnode 必须先铺好
        // （`seed_ready_state` 一并把 grant 表热身）。
        crate::main_loop::seed_ready_state(&mut state);
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(0x55, crate::open::S_IFREG | 0o644),
            PathFollow::LinkSrc { dst_path: "/d/new".to_string() },
        );
        state.run_worker_continuations();
        assert!(state.take_reply().is_none(), "转场不该回用户");
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().path.as_ref().map(|p| &p.follow),
            Some(PathFollow::LinkDst { src_ino: 0x55, .. })
        ));
        assert_eq!(
            state.pending_fs.as_ref().map(|p| p.req.m_type),
            Some(minix_types::REQ_LOOKUP),
            "阶段 2 起走新名的父目录"
        );
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = None;
            wp.path = None;
            wp.sendrec = None;
            wp.task = None;
        }
        state.worker_pool.release(idx);

        // ② 跨设备 → EXDEV（源在 MFS、父目录在别的 FS）。
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        {
            // 让这一趟遍历的 fs 变成"另一个 FS"：换起点重建 walk。
            let start = crate::path::LookupStart {
                fs: Endpoint::from_generation_slot(0, 9),
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk2, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/d".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                0,
                0,
            )
            .unwrap();
            plant(
                &mut state,
                idx,
                walk2,
                done_reply(0x21, crate::open::S_IFDIR | 0o755),
                PathFollow::LinkDst {
                    src_fs_e: Endpoint::MFS,
                    src_ino: 0x55,
                    entry: "new".to_string(),
                },
            );
            let _ = walk;
        }
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EXDEV))
        );

        // ③ 同设备 + 非属主对 0755 无写权 → EACCES。
        let (idx, walk) = mk(&mut state, 2000);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(0x21, crate::open::S_IFDIR | 0o755),
            PathFollow::LinkDst {
                src_fs_e: Endpoint::MFS,
                src_ino: 0x55,
                entry: "new".to_string(),
            },
        );
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EACCES))
        );

        // ④ 全过 → REQ_LINK（文件 ino 在前、目录 ino 在后）+ 状态回复。
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(0x21, crate::open::S_IFDIR | 0o755),
            PathFollow::LinkDst {
                src_fs_e: Endpoint::MFS,
                src_ino: 0x55,
                entry: "new".to_string(),
            },
        );
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_LINK");
        assert_eq!(p.req.m_type, minix_types::REQ_LINK);
        // SAFETY(test): 按 link_req_off 读回四域。
        unsafe {
            let raw = &p.req.m_u.raw;
            let inode = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let dir_ino = u64::from_le_bytes(raw[8..16].try_into().unwrap());
            let path_len = u64::from_le_bytes(raw[24..32].try_into().unwrap());
            assert_eq!(inode, 0x55, "inode 是被链接的源文件");
            assert_eq!(dir_ino, 0x21, "dir_ino 是新名的父目录");
            assert_eq!(path_len, 4, "名字含结尾 NUL（\"new\" → 4 字节）");
        }
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0))
        );
    }

    /// `PathFollow::Slink`（symlink 的父目录半）：`W|X` 权限门（EACCES）过了
    /// 才发 `REQ_SLINK`；请求带**两个 grant**——名字（VFS 内存 direct）与目标
    /// 串（用户内存 magic），`mem_size` 是目标串长度**不含**结尾 NUL
    /// （C `do_slink:411-414` + `req_slink_actual`）。
    #[test]
    fn test_path_follow_slink_gates_and_two_grants() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mk = |state: &mut VfsState, eff_uid: u32| {
            let slot = minix_types::UserSlot::new(0);
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .unwrap();
            {
                let fp = state.fproc_table.get_mut(slot).unwrap();
                fp.endpoint = user;
                fp.pid = 100;
                fp.eff_uid = eff_uid;
                fp.eff_gid = eff_uid;
                fp.real_uid = eff_uid;
                fp.real_gid = eff_uid;
            }
            let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/d".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                0,
                0,
            )
            .unwrap();
            (idx, walk)
        };
        let done_reply = |mode: u32| {
            let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
            // SAFETY(test): 按 lookup_reply_off 填 ino/mode。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&0x21u64.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&mode.to_le_bytes());
            }
            reply
        };
        let plant = |state: &mut VfsState, idx: usize, walk, reply: Message| {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Slink {
                    entry: "l".to_string(),
                    target_addr: 0x6000,
                    target_len: 7, // "target" 不含 NUL
                },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        };

        // ① 目录 0755（other 有 x 无 w）、非属主 → `W|X` 门拒。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        crate::main_loop::warm_grants(&mut state);
        let (idx, walk) = mk(&mut state, 2000);
        plant(&mut state, idx, walk.clone(), done_reply(crate::open::S_IFDIR | 0o755));
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EACCES))
        );

        // ② root → 发 REQ_SLINK，双 grant 都在请求里。
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(&mut state, idx, walk.clone(), done_reply(crate::open::S_IFDIR | 0o755));
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_SLINK");
        assert_eq!(p.req.m_type, minix_types::REQ_SLINK);
        // SAFETY(test): 按 slink_req_off 读回七域。
        unsafe {
            let raw = &p.req.m_u.raw;
            let dir_ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let path_len = u64::from_le_bytes(raw[8..16].try_into().unwrap());
            let mem_size = u64::from_le_bytes(raw[16..24].try_into().unwrap());
            let grant_path = i32::from_le_bytes(raw[24..28].try_into().unwrap());
            let grant_target = i32::from_le_bytes(raw[28..32].try_into().unwrap());
            let uid = u32::from_le_bytes(raw[32..36].try_into().unwrap());
            assert_eq!(dir_ino, 0x21, "inode 域是父目录");
            assert_eq!(path_len, 2, "名字含结尾 NUL（\"l\" → 2 字节）");
            assert_eq!(mem_size, 7, "目标串长度不含 NUL");
            assert_eq!(grant_path, p.grant, "现场记的是名字 grant");
            assert_ne!(grant_target, grant_path, "两个 grant 必须是两张");
            assert_eq!(uid, crate::link::SU_UID);
        }
        // 回复只有状态。
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0))
        );
    }

    /// `PathFollow::Mknod`（mknod 的父目录半）：类型门（ENOTDIR）→ `W|X`
    /// 权限门（EACCES）→ 名字 direct grant → `REQ_MKNOD` 七域随行、回复只有
    /// 状态（C `do_mknod:543-552` + `req_mknod`）。
    #[test]
    fn test_path_follow_mknod_gates_and_request_shape() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mk = |state: &mut VfsState, eff_uid: u32| {
            let slot = minix_types::UserSlot::new(0);
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .unwrap();
            {
                let fp = state.fproc_table.get_mut(slot).unwrap();
                fp.endpoint = user;
                fp.pid = 100;
                fp.eff_uid = eff_uid;
                fp.eff_gid = eff_uid;
                fp.real_uid = eff_uid;
                fp.real_gid = eff_uid;
            }
            let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/d".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                0,
                0,
            )
            .unwrap();
            (idx, walk)
        };
        let done_reply = |mode: u32, uid: u32| {
            let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
            // SAFETY(test): 按 lookup_reply_off 填 ino/mode/uid/gid。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&0x11u64.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&mode.to_le_bytes());
                raw[minix_types::lookup_reply_off::UID..minix_types::lookup_reply_off::UID + 4]
                    .copy_from_slice(&uid.to_le_bytes());
            }
            reply
        };
        let plant = |state: &mut VfsState, idx: usize, walk, reply: Message| {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Mknod {
                    entry: "n".to_string(),
                    mode_bits: crate::open::S_IFCHR | 0o644,
                    dev: 0x0301,
                },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        };

        // ① 父目录不是目录 → ENOTDIR。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        crate::main_loop::warm_grants(&mut state);
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFREG | 0o755, 0),
        );
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::ENOTDIR))
        );

        // ② 目录 0755（other 有 x 无 w）、非属主 → `W|X` 门拒（EACCES）。
        let (idx, walk) = mk(&mut state, 2000);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFDIR | 0o755, 0),
        );
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EACCES))
        );

        // ③ root + 可写目录 → REQ_MKNOD 七域随行；回复只有状态。
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFDIR | 0o755, 0),
        );
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_MKNOD");
        assert_eq!(p.req.m_type, minix_types::REQ_MKNOD);
        // SAFETY(test): 按 mknod_req_off 读回七域。
        unsafe {
            let raw = &p.req.m_u.raw;
            let dev = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let dir_ino = u64::from_le_bytes(raw[8..16].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[16..20].try_into().unwrap());
            let uid = u32::from_le_bytes(raw[20..24].try_into().unwrap());
            let gid = u32::from_le_bytes(raw[24..28].try_into().unwrap());
            let path_len = u64::from_le_bytes(raw[32..40].try_into().unwrap());
            assert_eq!(dev, 0x0301, "设备号随行");
            assert_eq!(dir_ino, 0x11, "inode 域是父目录");
            assert_eq!(mode, crate::open::S_IFCHR | 0o644);
            assert_eq!((uid, gid), (crate::link::SU_UID, crate::link::SU_UID));
            assert_eq!(path_len, 2, "名字含结尾 NUL（\"n\" → 2 字节）");
        }
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0))
        );
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle());
    }

    /// `PathFollow::Truncate`（truncate 的路径半）：走完过 W 位门 → 发送半
    /// （与 `Ftruncate` 共用）。三态：写位门拒 → EACCES；大小不变 → 就地回 0
    /// 且**不发请求**（POSIX 文件时间，C link.c:311-314）；真要截断 → 登记
    /// `REQ_FTRUNC`（宿主下要热身 grant 表才看得到）。
    #[test]
    fn test_path_follow_truncate_gates_and_same_size_skip() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mk = |state: &mut VfsState, eff_uid: u32| {
            let slot = minix_types::UserSlot::new(0);
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .unwrap();
            {
                let fp = state.fproc_table.get_mut(slot).unwrap();
                fp.endpoint = user;
                fp.pid = 100;
                fp.eff_uid = eff_uid;
                fp.real_uid = eff_uid;
                fp.eff_gid = eff_uid;
                fp.real_gid = eff_uid;
            }
            let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/f".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                0,
                0,
            )
            .unwrap();
            (idx, walk)
        };
        let done_reply = |mode: u32, size: u64, uid: u32| {
            let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
            // SAFETY(test): 按 lookup_reply_off 填 ino/mode/size/uid。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::FILE_SIZE
                    ..minix_types::lookup_reply_off::FILE_SIZE + 8]
                    .copy_from_slice(&size.to_le_bytes());
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&0x31u64.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&mode.to_le_bytes());
                raw[minix_types::lookup_reply_off::UID..minix_types::lookup_reply_off::UID + 4]
                    .copy_from_slice(&uid.to_le_bytes());
            }
            reply
        };
        let plant = |state: &mut VfsState, idx: usize, walk, reply: Message, length: i64| {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Truncate { length },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        };

        // ① 非属主对 0644 无写权 → EACCES（C `forbidden(fp, vp, W_BIT)`）。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let (idx, walk) = mk(&mut state, 2000);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFREG | 0o644, 100, 1000),
            10,
        );
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EACCES))
        );

        // ② 属主 + 大小不变 → 就地回 0，不发请求（POSIX 文件时间）。
        let (idx, walk) = mk(&mut state, 1000);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFREG | 0o644, 100, 1000),
            100,
        );
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none(), "大小不变不该打扰 FS");
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0)),
            "C 回 OK（不是新长度）"
        );
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle(), "槽已释放");

        // ③ 属主 + 真要截断 → 登记 REQ_FTRUNC。
        warm_grants(&mut state);
        let (idx, walk) = mk(&mut state, 1000);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFREG | 0o644, 100, 1000),
            10,
        );
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_FTRUNC");
        assert_eq!(p.req.m_type, minix_types::REQ_FTRUNC);
        // SAFETY(test): 按 ftrunc_req_off 读回 ino/start/end。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let start = i64::from_le_bytes(raw[8..16].try_into().unwrap());
            let end = i64::from_le_bytes(raw[16..24].try_into().unwrap());
            assert_eq!((ino, start, end), (0x31, 10, 0), "end=0 即截到 start");
        }
    }

    /// `PathFollow::Chown`（path 半）：走完并表 vnode 后进 `finish_chown`
    /// 共用体——与 fd 半共用同一体，所以这里只钉"path 半真的走到了共用体"
    /// （发 `REQ_CHOWN` 且 vnode 缓存被登记），门与折算的细节在 Fchown 那条
    /// 测试里逐条钉过。
    #[test]
    fn test_path_follow_chown_reaches_shared_body() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        warm_grants(&mut state);
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = crate::link::SU_UID; // root：跳过三条属主规则
            fp.eff_gid = 0;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
        let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/f".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
        // SAFETY(test): 按 lookup_reply_off 填 ino/mode/uid/gid。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&0x77u64.to_le_bytes());
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFREG | 0o644).to_le_bytes());
            raw[minix_types::lookup_reply_off::UID..minix_types::lookup_reply_off::UID + 4]
                .copy_from_slice(&1000u32.to_le_bytes());
            raw[minix_types::lookup_reply_off::GID..minix_types::lookup_reply_off::GID + 4]
                .copy_from_slice(&100u32.to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Chown { uid: 2000, gid: 200 },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_CHOWN");
        assert_eq!(p.req.m_type, minix_types::REQ_CHOWN);
        // SAFETY(test): 按 chown_req_off 读回三域（root 不做 -1 折算以外的改动）。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let uid = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let gid = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            assert_eq!((ino, uid, gid), (0x77, 2000, 200));
        }
        // 并表：vnode 缓存里已经有了这个 ino（C `eat_path` 的临时 vnode）。
        let v = state
            .vnode_table
            .find_by_ino(Endpoint::MFS, 0x77)
            .map(|id| state.vnode_table.get(id).unwrap().ino);
        assert_eq!(v, Some(0x77), "path 半走完要和 vnode 表并上");
    }

    /// `PathFollow::Unlink`（阶段 1）的三道门 + `PathFollow::UnlinkSticky`
    /// （阶段 2，粘滞位目录上的子遍历）：C `do_unlink`（link.c:122-159）。
    ///
    /// 阶段 1 走完父目录后：类型门（不是目录 → ENOTDIR）→ `X|W` 权限门
    /// （EACCES）→ 粘滞位门（开着就转阶段 2，**子遍历**取受害者属主）。
    /// 阶段 2 的属主门在续接体里判：受害者属主 ≠ 有效 id 且不是超级用户
    /// → EPERM。
    ///
    /// 宿主可测到"请求已登记"这一步：grant 表首次增长要过 `sys_setgrant`
    /// （宿主不可达），但失败路径已经把 freelist 铺好，**后续 grant 会成功**
    /// ——所以测试开头先做一次"热身" grant 把这一格确定性化，之后就能断言
    /// 真正登记出去的请求（`REQ_LOOKUP` / `REQ_UNLINK` / `REQ_RMDIR`）。
    #[test]
    fn test_path_follow_unlink_gates_and_sticky_stage() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mk = |state: &mut VfsState| {
            let slot = minix_types::UserSlot::new(0);
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .unwrap();
            let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/d".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                0,
                0,
            )
            .unwrap();
            (slot, idx, walk)
        };
        let done_reply = |mode: u32, uid: u32, gid: u32| {
            let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
            // SAFETY(test): 按 lookup_reply_off 填 ino/mode/uid/gid。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&7u64.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&mode.to_le_bytes());
                raw[minix_types::lookup_reply_off::UID..minix_types::lookup_reply_off::UID + 4]
                    .copy_from_slice(&uid.to_le_bytes());
                raw[minix_types::lookup_reply_off::GID..minix_types::lookup_reply_off::GID + 4]
                    .copy_from_slice(&gid.to_le_bytes());
            }
            reply
        };
        let plant = |state: &mut VfsState, idx: usize, walk, reply: Message, follow: PathFollow| {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending { walk, grant: 9, follow });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        };

        // ① 父目录不是目录（父路径解析到一个常规文件）→ ENOTDIR。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        // 热身：让 grant 表完成一次（失败的）增长，后续 grant 走 freelist
        // 直接成功——把"能不能发请求"从测试变量里去掉。
        warm_grants(&mut state);
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFREG | 0o777, 0, 0),
            PathFollow::Unlink { entry: "x".to_string(), rmdir: false },
        );
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::ENOTDIR))
        );

        // ② 目录 0755（other 有 x 无 w）、调用方 1000 号 → 权限门拒。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 1000;
            fp.eff_gid = 1000;
            fp.real_uid = 1000;
            fp.real_gid = 1000;
        }
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFDIR | 0o755, 0, 0),
            PathFollow::Unlink { entry: "x".to_string(), rmdir: false },
        );
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EACCES))
        );

        // ③ 0777 可写但带粘滞位 → 转阶段 2；宿主下子遍历的 grant 不可达 →
        // EIO（真机上这里是第二条 REQ_LOOKUP）。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 1000;
            fp.eff_gid = 1000;
        }
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFDIR | 0o777 | crate::open::S_ISVTX, 0, 0),
            PathFollow::Unlink { entry: "x".to_string(), rmdir: true },
        );
        state.run_worker_continuations();
        assert!(state.take_reply().is_none(), "子遍历还没走完，不能回用户");
        let p = state.pending_fs.as_ref().expect("已登记第二条 REQ_LOOKUP");
        assert_eq!(p.req.m_type, minix_types::REQ_LOOKUP, "粘滞位门要再走一趟");
        assert_eq!(p.fs_e, Endpoint::MFS);
        // 子遍历带 `PATH_RET_SYMLINK`（C `lookup_init(&stickycheck, ...,
        // PATH_RET_SYMLINK, ...)`，link.c:134）——这是宿主下能观测到的
        // "flags 真的发进消息"的地方（域曾经从来没写过）。
        // SAFETY(test): 按 lookup_req_off 读 FLAGS 域。
        unsafe {
            let raw = &p.req.m_u.raw;
            let flags = u32::from_le_bytes(raw[16..20].try_into().unwrap());
            assert_eq!(flags, minix_types::PATH_RET_SYMLINK);
        }
        // 现场换成了阶段 2（父目录身份随行）。
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().path.as_ref().map(|p| &p.follow),
            Some(PathFollow::UnlinkSticky { dir_ino: 7, rmdir: true, .. })
        ));
        // 清掉这一轮的挂起现场再进下一条用例：留着的话，下一轮
        // `run_worker_continuations` 会拿**陈旧的回复**把这个槽再跑一遍，
        // 两条作业在同一轮完成时只有先入队的那条回复发得出去（模型一轮一条
        // 回复），测试就会读到别人的回复。测试卫生，不是实现问题。
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = None;
            wp.path = None;
            wp.sendrec = None;
            wp.task = None;
        }
        state.worker_pool.release(idx);

        // ④ 0777 无粘滞位 → 直接发请求；宿主下同样停在 grant → EIO。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 1000;
            fp.eff_gid = 1000;
        }
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFDIR | 0o777, 0, 0),
            PathFollow::Unlink { entry: "x".to_string(), rmdir: false },
        );
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_UNLINK");
        assert_eq!(p.req.m_type, minix_types::REQ_UNLINK);
        assert_eq!(p.worker, idx);
        // SAFETY(test): 按 unlink_req_off 读回父目录 ino 与名字长度。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let path_len = u64::from_le_bytes(raw[16..24].try_into().unwrap());
            assert_eq!(ino, 7, "inode 域是**父目录**的节点号");
            assert_eq!(path_len, 2, "名字含结尾 NUL（\"x\" → 2 字节）");
        }
        // 回复只有状态：续接体把状态原样回用户。
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0)),
            "REQ_UNLINK 成功 → 用户拿 0"
        );
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle(), "槽已释放");

        // ⑤ 阶段 2：受害者属主不是调用方（1000 vs 0）→ EPERM。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 1000;
            fp.eff_gid = 1000;
        }
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFREG | 0o644, 0, 0),
            PathFollow::UnlinkSticky {
                entry: "x".to_string(),
                rmdir: false,
                dir_fs_e: Endpoint::MFS,
                dir_ino: 7,
            },
        );
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EPERM))
        );

        // ⑥ 阶段 2：受害者属主就是调用方 → 过粘滞位门，停在 grant → EIO。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 1000;
            fp.eff_gid = 1000;
        }
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFREG | 0o644, 1000, 1000),
            PathFollow::UnlinkSticky {
                entry: "x".to_string(),
                rmdir: true,
                dir_fs_e: Endpoint::MFS,
                dir_ino: 7,
            },
        );
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_RMDIR");
        assert_eq!(p.req.m_type, minix_types::REQ_RMDIR, "rmdir 走自己的请求号");
        // SAFETY(test): 父目录 ino 随现场带过来。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            assert_eq!(ino, 7);
        }
    }

    /// `PathFollow::Chmod` 与 `WorkerCont::Chmod`：`chmod` 的两段。
    /// 走完先过两道门（属主/超级用户 → 否则 EPERM；只读挂载 → EROFS），
    /// 再过 setgid 清位（非超级用户且文件不在自己的组里），才发 `REQ_CHMOD`；
    /// 回复带**实际生效的模式**，续接体回写 vnode 缓存（C protect.c:120-128）。
    #[test]
    fn test_path_follow_chmod_gates_and_cache_writeback() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mk = |state: &mut VfsState| {
            let slot = minix_types::UserSlot::new(0);
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .unwrap();
            let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                0,
                0,
            )
            .unwrap();
            (slot, idx, walk)
        };
        let done_reply = |uid: u32, gid: u32| {
            let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
            // SAFETY(test): 按 lookup_reply_off 填 ino/mode/uid/gid。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&0x21u64.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&(crate::open::S_IFREG | 0o644).to_le_bytes());
                raw[minix_types::lookup_reply_off::UID..minix_types::lookup_reply_off::UID + 4]
                    .copy_from_slice(&uid.to_le_bytes());
                raw[minix_types::lookup_reply_off::GID..minix_types::lookup_reply_off::GID + 4]
                    .copy_from_slice(&gid.to_le_bytes());
            }
            reply
        };
        let plant = |state: &mut VfsState, idx: usize, walk, mode: u32, reply: Message| {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Chmod { user, mode },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        };

        // ① 非属主（有效 id 1000，文件属主 0）→ EPERM，不发请求。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 1000;
            fp.eff_gid = 1000;
        }
        plant(&mut state, idx, walk.clone(), 0o600, done_reply(0, 0));
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none(), "门没过不该发 REQ_CHMOD");
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EPERM))
        );

        // ② 属主但挂载是只读 → EROFS。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 0;
            fp.eff_gid = 0;
        }
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
            v.flags = crate::vmnt::VmntFlags::READONLY;
        }
        plant(&mut state, idx, walk.clone(), 0o600, done_reply(0, 0));
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EROFS))
        );

        // ③ 属主（非超级用户：有效 id 1000 = 文件属主）+ 可写挂载：发请求，
        // 且 setgid 位被清（文件组 0 ≠ 有效组 1000，protect.c:120-121）。
        // 注意这里**不能**用 eff_uid 0——0 就是超级用户，清位那条不适用。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 1000;
            fp.eff_gid = 1000;
        }
        state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap().flags =
            crate::vmnt::VmntFlags::empty();
        plant(&mut state, idx, walk.clone(), 0o2664, done_reply(1000, 0));
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_CHMOD");
        assert_eq!(p.req.m_type, minix_types::REQ_CHMOD);
        // SAFETY(test): 按 chmod_req_off 读回 inode/mode。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            assert_eq!(ino, 0x21);
            assert_eq!(mode, 0o664, "setgid 位被清（非超级用户且不同组）");
        }

        // 续接体：回复带实际模式（FS 收窄成 0o750）→ 回写 vnode 缓存。
        // 注意回复里的模式是**整字**（含类型位）：mfs 的 `fs_chmod` 末尾是
        // `*mode = rip->i_mode`（mfs/protect.c），libfsdriver 原样搬进回复
        // （call.c:787）。只回权限位是错的实现——缓存里的类型位会丢。
        let vnode = match state.worker_pool.get_mut(idx).unwrap().cont {
            Some(WorkerCont::Chmod { vnode }) => vnode,
            _ => panic!("续接标识应是 Chmod"),
        };
        state.pending_fs = None;
        let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
        // SAFETY(test): 按 chmod_reply_off 填实际模式（整字，含 S_IFREG）。
        unsafe {
            reply.m_u.raw[0..4]
                .copy_from_slice(&(crate::open::S_IFREG | 0o750).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0))
        );
        assert_eq!(
            state.vnode_table.get(crate::vnode::VnodeId(vnode)).unwrap().mode,
            crate::open::S_IFREG | 0o750,
            "缓存里的模式跟着 FS 回的实际值走"
        );
    }

    /// `PathFollow::Rdlink` 与 `WorkerCont::Rdlink`：`readlink` 的两段。
    /// 走完先过"是不是符号链接"的门（C link.c:496-501 的 `S_ISLNK`，否则
    /// EINVAL），是链接才发 `REQ_RDLINK`；回复的**字节数在载荷里**而不是
    /// 状态字（C request.c:745），续接体要从 `rdlink_reply_off::NBYTES` 取。
    #[test]
    fn test_path_follow_rdlink_gates_and_continuation() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mk = |state: &mut VfsState| {
            let slot = minix_types::UserSlot::new(0);
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .unwrap();
            let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::RET_SYMLINK)
                    .unwrap(),
                rd,
                0,
                0,
            )
            .unwrap();
            (slot, idx, walk)
        };
        let done_reply = |mode: u32| {
            let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
            // SAFETY(test): 按 lookup_reply_off 填 ino/mode。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&9u64.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&mode.to_le_bytes());
            }
            reply
        };

        // 不是符号链接（常规文件）→ EINVAL，不发任何 FS 请求。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk: walk.clone(),
                grant: 9,
                follow: PathFollow::Rdlink { user, buf: 0x5000, buf_size: 128 },
            });
            wp.sendrec = Some(done_reply(crate::open::S_IFREG | 0o644));
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none(), "非符号链接不该发 REQ_RDLINK");
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EINVAL))
        );

        // 是符号链接：发 `REQ_RDLINK`。宿主下要先热身 grant 表（见 Unlink
        // 测试的说明），否则会退化成"停在 grant"的顺序产物。
        warm_grants(&mut state);
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Rdlink { user, buf: 0x5000, buf_size: 128 },
            });
            wp.sendrec = Some(done_reply(crate::open::S_IFLNK | 0o777));
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_RDLINK");
        assert_eq!(p.req.m_type, minix_types::REQ_RDLINK);
        // SAFETY(test): 按 rdlink_req_off 读回 ino 与窗口大小。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mem_size = u64::from_le_bytes(raw[16..24].try_into().unwrap());
            assert_eq!(ino, 9);
            assert_eq!(mem_size, 128, "窗口大小来自用户给的 bufsize");
        }
        // 清现场再进下一条用例（理由同 Unlink 那条测试：一轮一条回复）。
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = None;
            wp.path = None;
            wp.sendrec = None;
            wp.task = None;
        }
        state.worker_pool.release(idx);

        // 续接体：状态 OK 时长度取自载荷的 nbytes（不是状态字 0）。
        let idx2 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
        // SAFETY(test): 按 rdlink_reply_off 填 nbytes。
        unsafe {
            reply.m_u.raw[0..8].copy_from_slice(&12u64.to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx2).unwrap();
            wp.cont = Some(WorkerCont::Rdlink { grant: 5 });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 12)),
            "用户拿到的是链接文本长度"
        );

        // 续接体：FS 报错时错误原样回（不读载荷）。
        let idx3 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx3).unwrap();
            wp.cont = Some(WorkerCont::Rdlink { grant: 6 });
            wp.sendrec = Some(Message { m_type: minix_types::ENOENT, ..Message::default() });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::ENOENT))
        );
    }

    /// `PathFollow::Access`：走完即判权限，**没有 FS 往返**——`forbidden`
    /// 对 access(2) 用**真实** uid/gid（C protect.c:255-256 的
    /// `job_call_nr == VFS_ACCESS` 特例）。这里的现场特意让真实 id 是文件
    /// 属主、有效 id 不是：若实现错用有效 id，W_OK 会被误拒。
    #[test]
    fn test_path_follow_access_uses_real_ids() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mk = |state: &mut VfsState| {
            let slot = minix_types::UserSlot::new(0);
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .unwrap();
            let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                // 遍历时带的 id 故意等于后面用例里的调用方真实 id（1000）：
                // 若实现按遍历参数而非回复里的属主算，用例 2 的 W_OK 会被
                // 误放行——那条断言因此是区分性的。
                1000,
                1000,
            )
            .unwrap();
            (slot, idx, walk)
        };
        let done_reply = |mode: u32, uid: u32, gid: u32| {
            let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
            // SAFETY(test): 按 lookup_reply_off 填 mode/uid/gid（ino 用不着）。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&5u64.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&mode.to_le_bytes());
                raw[minix_types::lookup_reply_off::UID..minix_types::lookup_reply_off::UID + 4]
                    .copy_from_slice(&uid.to_le_bytes());
                raw[minix_types::lookup_reply_off::GID..minix_types::lookup_reply_off::GID + 4]
                    .copy_from_slice(&gid.to_le_bytes());
            }
            reply
        };
        let run = |state: &mut VfsState, idx: usize, slot: minix_types::UserSlot, walk, access: u32, reply: Message| {
            {
                let wp = state.worker_pool.get_mut(idx).unwrap();
                wp.cont = Some(WorkerCont::Path);
                wp.path = Some(PathPending {
                    walk,
                    grant: 9,
                    follow: PathFollow::Access { user, access },
                });
                wp.sendrec = Some(reply);
                wp.state = crate::worker::WorkerState::Busy;
            }
            state.run_worker_continuations();
            assert!(state.worker_pool.get_mut(idx).unwrap().is_idle(), "槽已释放");
            let _ = slot;
            state.take_reply().expect("回复").1.m_type
        };

        // 真实 id = 属主(0)、有效 id = 1000：W_OK 按真实 id 判 → 允许。
        let mut state = VfsState::new();
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.real_uid = 0;
            fp.eff_uid = 1000;
            fp.real_gid = 0;
            fp.eff_gid = 1000;
        }
        assert_eq!(
            run(&mut state, idx, slot, walk.clone(), 0o2, done_reply(crate::open::S_IFREG | 0o644, 0, 0)),
            0,
            "access 用真实 id 判：真实 id 是属主 → W_OK 允许"
        );

        // 属主是 0、真实 id 也是 1000（换一组）：W_OK 被拒。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.real_uid = 1000;
            fp.real_gid = 1000;
        }
        assert_eq!(
            run(&mut state, idx, slot, walk.clone(), 0o2, done_reply(crate::open::S_IFREG | 0o644, 0, 0)),
            minix_types::EACCES,
            "非属主对 0644 无写权"
        );
        // 同一个 0644：别人的读位在（other r）→ R_OK 允许。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.real_uid = 1000;
            fp.real_gid = 1000;
        }
        assert_eq!(
            run(&mut state, idx, slot, walk.clone(), 0o4, done_reply(crate::open::S_IFREG | 0o644, 0, 0)),
            0
        );

        // 只读挂载 + W_OK → EROFS（C `read_only` 在 forbidden 尾部）。
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.real_uid = 0;
            fp.real_gid = 0;
        }
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
            v.fs = Endpoint::MFS;
            v.dev = 1;
            v.flags = crate::vmnt::VmntFlags::READONLY;
        }
        assert_eq!(
            run(&mut state, idx, slot, walk.clone(), 0o2, done_reply(crate::open::S_IFREG | 0o644, 0, 0)),
            minix_types::EROFS
        );
    }

    /// `open` 的 `O_TRUNC` 分支（C `common_open:150-157`）：常规文件 +
    /// `O_TRUNC` → W 位门过了之后发 `REQ_FTRUNC(ino, 0, 0)`（`end == 0`
    /// 即"截到 start"，mfs `fs_trunc:439-443`）→ 回复到达后照常装配 fd。
    /// 两段都钉在这里：**发出去的是截断请求**（不是直接装配），以及
    /// **截断状态被忽略**（C 没接 `truncate_vnode` 的返回值）——所以回复
    /// 里带错误也照样装配。
    #[test]
    fn test_open_trunc_sends_ftrunc_then_assembles() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 0; // 文件属主，0644 有 W 位
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
        let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
        // 遍历时带的 id 故意与回复里的属主不同（7777 vs 0）：权限判断必须
        // 按 `node_details` 的属主算（C `advance` 的 `v_uid = res.uid`），
        // 按遍历参数算会把"非属主"误判成属主。
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            7777,
            7777,
        )
        .unwrap();
        let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
        // SAFETY(test): 按 lookup_reply_off 填 ino/mode/uid/gid。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&5u64.to_le_bytes());
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFREG | 0o644).to_le_bytes());
            raw[minix_types::lookup_reply_off::UID..minix_types::lookup_reply_off::UID + 4]
                .copy_from_slice(&0u32.to_le_bytes());
            raw[minix_types::lookup_reply_off::GID..minix_types::lookup_reply_off::GID + 4]
                .copy_from_slice(&0u32.to_le_bytes());
        }
        let oflags = crate::open::OpenFlags::TRUNC.bits();
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Open { user, oflags },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();

        // 相位 2 没直接装配：登记的是截断请求，槽继续挂起，用户还没回复。
        assert!(state.take_reply().is_none(), "截断未完成前不得回复 fd");
        let p = state.pending_fs.as_ref().expect("已登记 REQ_FTRUNC");
        assert_eq!(p.req.m_type, minix_types::REQ_FTRUNC);
        assert_eq!(p.worker, idx);
        assert_eq!(p.grant, 0, "截断无数据面");
        // SAFETY(test): 按 ftrunc_req_off 读回三域。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let trc_start = i64::from_le_bytes(raw[8..16].try_into().unwrap());
            let trc_end = i64::from_le_bytes(raw[16..24].try_into().unwrap());
            assert_eq!((ino, trc_start, trc_end), (5, 0, 0), "截到 0（C truncate_vnode(vp, 0)）");
        }
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().cont,
            Some(WorkerCont::OpenTrunc { .. })
        ));
        assert!(!state.worker_pool.get_mut(idx).unwrap().is_idle());

        // 回复到达（故意带错误）：C 忽略截断结果，照常装配。
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message { m_type: minix_types::EIO, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, user);
        assert_eq!(reply.m_type, 0, "首个 fd 是 0（截断失败不拦 open）");
        let fp = state.fproc_table.get(slot).unwrap();
        let filp_id = fp.filps[0].expect("fd 指向 filp");
        let f = state.filp_table.get(crate::filp::FilpId(filp_id)).unwrap();
        assert_eq!(f.flags, oflags as i32, "filp_flags 写原始 oflags（含 O_TRUNC）");
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle());
    }

    /// `O_TRUNC` 的 W 位门（C `common_open:152` 的 `forbidden(fp, vp,
    /// W_BIT)`）：只读打开 + `O_TRUNC` 的文件若不可写，open 直接以
    /// `EACCES` 结束——**不发截断请求**。
    #[test]
    fn test_open_trunc_denied_by_write_gate() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 1000; // 非属主、非 root
            fp.real_uid = 1000;
            fp.eff_gid = 1000;
            fp.real_gid = 1000;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
        let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            // 遍历时带的 id **故意等于调用方的有效 id**（1000）：权限判断若
            // 按遍历参数而非回复里的属主算，就会把非属主当成属主、把这次
            // 打开放行——这条断言因此是区分性的。
            1000,
            1000,
        )
        .unwrap();
        let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
        // SAFETY(test): 0644 且属主是 0 → 1000 号只读。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&5u64.to_le_bytes());
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFREG | 0o644).to_le_bytes());
            raw[minix_types::lookup_reply_off::UID..minix_types::lookup_reply_off::UID + 4]
                .copy_from_slice(&0u32.to_le_bytes());
            raw[minix_types::lookup_reply_off::GID..minix_types::lookup_reply_off::GID + 4]
                .copy_from_slice(&0u32.to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Open {
                    user,
                    oflags: crate::open::OpenFlags::TRUNC.bits(),
                },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none(), "门没过就不该发截断");
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EACCES))
        );
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle(), "槽已释放");
    }

    /// `WorkerCont::Path` 的收尾接线：走完（`Ok`）后进相位 2（这里 grant 在
    /// 宿主不可达 → 以 EIO 收尾），**槽必须被释放、用户必须收到回复**——
    /// 防的是"续接体吞掉作业、槽泄漏、调用方永等"。
    #[test]
    fn test_worker_continuation_path_finishes_on_phase2_failure() {
        use crate::worker::{PathFollow, PathPending, WorkerCont};
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        // 现场：一趟已走完的遍历（路径随便给，续接只用到 follow）。
        let start = crate::path::LookupStart { fs: Endpoint::MFS, ino: 1, dev: 0 };
        let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
        let (walk, _step) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        // 回复：OK + ino=5（四域 node_details）。
        let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
        // SAFETY(test): 按 lookup_reply_off 填 ino。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&5u64.to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Path);
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow: PathFollow::Stat { user, buf: 0x6000 },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle(), "槽已释放");
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EIO)),
            "相位 2 的 grant 在宿主不可达 → EIO 收尾（不悬挂）"
        );
    }

    /// 续接层的**写**分支：除位置推进外还要按 C read.c:255-259 抬高
    /// vnode 大小（位置越过旧大小才动；读方向不动大小）。
    #[test]
    fn test_worker_continuation_write_raises_vnode_size() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.mode = crate::open::S_IFREG | 0o644;
            v.size = 0x100; // 旧大小
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): 按 transfer_reply_off 填回复（写 0x40 字节到 0x140）。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::transfer_reply_off::SEEK_POS
                ..minix_types::transfer_reply_off::SEEK_POS + 8]
                .copy_from_slice(&0x140i64.to_le_bytes());
            raw[minix_types::transfer_reply_off::NBYTES
                ..minix_types::transfer_reply_off::NBYTES + 8]
                .copy_from_slice(&0x40u64.to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Transfer {
                grant: 4,
                filp: fid.get(),
                vnode: vid.get(),
                orig_pos: 0x100,
                write: true,
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();

        assert_eq!(
            state.vnode_table.get(vid).unwrap().size,
            0x140,
            "写方向：新位置越过旧大小 → 抬高 vnode 大小（C read.c:255-259）"
        );
        assert_eq!(state.filp_table.get(fid).unwrap().pos, 0x140);
        assert_eq!(state.take_reply().map(|(t, m)| (t, m.m_type)), Some((user, 0x40)));

        // 反向对照：新位置**未**越过旧大小 → 大小不动。
        let idx2 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let mut reply2 = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): 位置 0x110（小于当前 0x140），写 0x10 字节。
        unsafe {
            let raw = &mut reply2.m_u.raw;
            raw[minix_types::transfer_reply_off::SEEK_POS
                ..minix_types::transfer_reply_off::SEEK_POS + 8]
                .copy_from_slice(&0x110i64.to_le_bytes());
            raw[minix_types::transfer_reply_off::NBYTES
                ..minix_types::transfer_reply_off::NBYTES + 8]
                .copy_from_slice(&0x10u64.to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx2).unwrap();
            wp.cont = Some(WorkerCont::Transfer {
                grant: 5,
                filp: fid.get(),
                vnode: vid.get(),
                orig_pos: 0x100,
                write: true,
            });
            wp.sendrec = Some(reply2);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(state.vnode_table.get(vid).unwrap().size, 0x140, "大小不回退");
        let _ = state.take_reply();
    }

    /// 续接层的 Read 分支：从回复取 `seek_pos`/`nbytes`（C `mess_fs_vfs_readwrite`
    /// 的共享偏移表），位置写回 filp，**状态＝实际读到的字节数**（C 的
    /// `cum_io`），槽释放。
    #[test]
    fn test_worker_continuation_read_updates_filp_and_status() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state.filp_table.get_mut(fid).unwrap().pos = 0x1000;
        let vid = state.vnode_table.alloc().unwrap();

        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        // 回复载荷：seek_pos@0、nbytes@8（共享偏移表）。
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): 按 transfer_reply_off 填回复。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::transfer_reply_off::SEEK_POS
                ..minix_types::transfer_reply_off::SEEK_POS + 8]
                .copy_from_slice(&0x1020i64.to_le_bytes());
            raw[minix_types::transfer_reply_off::NBYTES
                ..minix_types::transfer_reply_off::NBYTES + 8]
                .copy_from_slice(&0x20u64.to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Transfer {
                grant: 3,
                filp: fid.get(),
                vnode: vid.get(),
                orig_pos: 0x1000,
                write: false,
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();

        assert_eq!(
            state.filp_table.get(fid).unwrap().pos,
            0x1020,
            "位置按回复的 seek_pos 推进"
        );
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0x20)),
            "状态＝实际读到的字节数（C cum_io）"
        );
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle());
    }

    /// `WorkerCont::Getdents`：回复的 `seek_pos`/`nbytes` 取共享表
    /// （`getdents_reply_off`），**位置只在 `nbytes > 0` 时推进**
    /// （C read.c:311-313）——空目录或窗口装不下一整条目录项时
    /// `nbytes == 0`，位置若动了下一趟就跳过条目；用户拿到的是字节数。
    #[test]
    fn test_worker_continuation_getdents_position_gate() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state.filp_table.get_mut(fid).unwrap().pos = 0x100;

        let run = |state: &mut VfsState, status: i32, pos: i64, nbytes: i64| {
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .expect("空闲槽");
            let mut reply = Message { m_type: status, ..Message::default() };
            // SAFETY(test): 按 getdents_reply_off 填 seek_pos/nbytes。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[0..8].copy_from_slice(&pos.to_le_bytes());
                raw[8..16].copy_from_slice(&nbytes.to_le_bytes());
            }
            {
                let wp = state.worker_pool.get_mut(idx).unwrap();
                wp.cont = Some(WorkerCont::Getdents { grant: 5, filp: fid.get() });
                wp.sendrec = Some(reply);
                wp.state = crate::worker::WorkerState::Busy;
            }
            state.run_worker_continuations();
            assert!(state.worker_pool.get_mut(idx).unwrap().is_idle(), "槽已释放");
            state.take_reply().expect("回复").1.m_type
        };

        // 空目录（nbytes == 0）：位置**不动**，用户拿到 0。
        assert_eq!(run(&mut state, 0, 0x180, 0), 0);
        assert_eq!(state.filp_table.get(fid).unwrap().pos, 0x100);

        // 真读出条目：位置推进到回复给的新位置，用户拿到字节数。
        assert_eq!(run(&mut state, 0, 0x180, 48), 48);
        assert_eq!(state.filp_table.get(fid).unwrap().pos, 0x180);

        // FS 报错：位置不动，错误原样回用户。
        assert_eq!(run(&mut state, minix_types::ENOTDIR, 0x200, 12), minix_types::ENOTDIR);
        assert_eq!(state.filp_table.get(fid).unwrap().pos, 0x180);
    }

    /// 续接层（`run_worker_continuations`）：回复已落槽的作业按续接标识
    /// 收尾——Fstat 只撤 grant 并把状态回给用户；`ERESTART` 折 `EIO`
    /// （C comm.c:161-163）；槽被释放。
    #[test]
    fn test_worker_continuation_fstat_completes_job() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        // 手工摆出"臂挂起后回复已到"的槽态：cont + sendrec + WaitingForFs。
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .expect("空闲槽");
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Fstat { grant: 7 });
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0)),
            "Fstat 成功：状态 0 回用户"
        );
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle(), "槽已释放");

        // ERESTART 折 EIO（C comm.c:161-163）。
        let idx2 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx2).unwrap();
            wp.cont = Some(WorkerCont::Fstat { grant: 8 });
            wp.sendrec = Some(Message {
                m_type: minix_types::ERESTART,
                ..Message::default()
            });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EIO))
        );
    }

    /// 续接层的门：**只有回复已落槽的作业**才跑续接。C 里 `w_sendrec`
    /// 是双向缓冲——`fs_sendrec` 送出时装的是请求、线程随即在
    /// `worker_wait` 里阻塞；只有 `do_reply`（本模型的
    /// `handle_fs_reply` 置 `Busy`）才会把回复覆写进去并唤醒线程。若少了
    /// 这道门，另一个客户端在这段时间里发来的调用会驱动续接体拿请求码
    /// 当状态，把挂起的作业假完成（用户收到垃圾结果、真回复到达时
    /// `w_task` 已被清而报 WrongTask）。
    #[test]
    fn test_continuation_skips_slot_still_waiting_for_fs() {
        use crate::worker::{WorkerCont, WorkerState};
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .expect("空闲槽");
        // 发送半之后的槽态：sendrec 里是**请求**（`REQ_LOOKUP` 的码），
        // 状态是 `WaitingForFs`（`fs_sendrec` → `set_waiting`）。
        let req = crate::request::encode_lookup(3, 2, 1, 1, 0);
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Fstat { grant: 4 });
            wp.set_waiting(Endpoint::MFS, req);
        }
        assert_eq!(
            state.worker_pool.get_mut(idx).unwrap().state,
            WorkerState::WaitingForFs
        );
        state.run_worker_continuations();
        assert!(state.take_reply().is_none(), "等待中不得回复用户");
        assert!(
            !state.worker_pool.get_mut(idx).unwrap().is_idle(),
            "等待中的槽不得被释放"
        );
        assert!(
            state.worker_pool.get_mut(idx).unwrap().cont.is_some(),
            "续接标识保留给真正的回复"
        );
        // 回复真到了（`handle_fs_reply` 的落槽形态）：这时才跑。
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0))
        );
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle());
    }

    /// W3 回复半：`SyscallResult` → 回复入队的映射（C `do_work` 尾部
    /// `reply(who_e, result)`）。`Suspend` 不入队（回复在 FS 应答落地时
    /// 发），`Nosys` 显式回 ENOSYS，目标为 NONE 时不发。
    #[test]
    fn test_queue_reply_maps_syscall_results() {
        use crate::call_table::SyscallResult;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();

        state.queue_reply(user, SyscallResult::Ok(7));
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 7))
        );

        state.queue_reply(user, SyscallResult::Error(minix_types::EINVAL));
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EINVAL))
        );

        state.queue_reply(user, SyscallResult::Nosys);
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::ENOSYS))
        );

        state.queue_reply(user, SyscallResult::Suspend);
        assert!(state.take_reply().is_none(), "Suspend 的回复在 FS 应答时发");

        state.queue_reply(Endpoint::NONE, SyscallResult::Ok(0));
        assert!(state.take_reply().is_none(), "无调用方不发");
    }

    #[test]
    fn test_handle_fs_reply_delivers_to_waiting_slot() {
        // C do_reply(main.c:187-211)主路径:校验 → 落地 → 窗口放行 →
        // worker_signal。VfsState::comm 入态(S12 W1)后的见证。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;

        let reply = reply_msg(0x503, 2, Endpoint::MFS);
        let r = state.handle_fs_reply(&reply, &VfsTransIdCodec);
        assert_eq!(r, Ok(2));
        let wp = state.worker_pool.get(2).unwrap();
        assert_eq!(wp.state, WorkerState::Busy); // worker_signal 后可运行
        assert_eq!(wp.task, None); // w_task = NONE(main.c:206)
        let delivered = wp.sendrec.expect("reply delivered");
        // C main.c:88 —— transid 在进 do_reply 前被剥掉，槽里留的是服务端
        // 原始状态（0x503 是这条测试造的"状态字"）。
        assert_eq!(delivered.m_type, 0x503);
        assert_eq!(state.comm.vmnts[0].cur_reqs, 0); // c_cur_reqs--(main.c:207)
    }

    #[test]
    fn test_handle_fs_reply_vm_skips_vmnt_lookup() {
        // C main.c:190 — who_e == VM_PROC_NR 不查 vmnt,窗口不动。
        let mut state = VfsState::new();
        seed_waiting(&mut state, 1, Endpoint::VM);
        let reply = reply_msg(0x503, 1, Endpoint::VM);
        assert_eq!(state.handle_fs_reply(&reply, &VfsTransIdCodec), Ok(1));
        assert_eq!(state.worker_pool.get(1).unwrap().state, WorkerState::Busy);
    }

    #[test]
    fn test_handle_fs_reply_wrong_task_is_typed_printf() {
        // C main.c:193-196 — w_task != who_e → printf+return(软失败,
        // 回复不落地、窗口不动)。注意 C 的 find_vmnt(main.c:190)在
        // w_task 检查之前,故错误应答方的 vmnt 也必须存在。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
            v.fs = Endpoint::from_generation_slot(1, 7);
            v.dev = 2;
        }
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;
        let reply = reply_msg(0x503, 2, Endpoint::from_generation_slot(1, 7));
        assert_eq!(
            state.handle_fs_reply(&reply, &VfsTransIdCodec),
            Err(FsReplyError::WrongTask)
        );
        assert_eq!(state.comm.vmnts[0].cur_reqs, 1);
        assert_eq!(state.worker_pool.get(2).unwrap().state, WorkerState::WaitingForFs);
    }

    #[test]
    fn test_handle_fs_reply_spurious_transid() {
        // 低 16 位非 IS_VFS_FS_TRANSID → SpuriousTransid。
        let mut state = VfsState::new();
        let mut msg = Message { m_type: 0x503, ..Message::default() };
        msg.m_source = Endpoint::MFS;
        assert_eq!(
            state.handle_fs_reply(&msg, &VfsTransIdCodec),
            Err(FsReplyError::SpuriousTransid)
        );
    }

    #[test]
    #[should_panic(expected = "Couldn't find vmnt for endpoint")]
    fn test_handle_fs_reply_unknown_fs_panics_like_c() {
        // C main.c:190-191 — 非 VM 回复且 find_vmnt 失败 → panic(fail-fast)。
        let mut state = VfsState::new();
        seed_waiting(&mut state, 2, Endpoint::MFS); // 未播种 vmnt0
        let reply = reply_msg(0x503, 2, Endpoint::MFS);
        let _ = state.handle_fs_reply(&reply, &VfsTransIdCodec);
    }

    #[test]
    fn test_run_once_fs_reply_reaches_slot() {
        // C main.c:80-89 — transid 命中即 do_reply,主循环无应答对象。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;
        let reply = reply_msg(0x503, 2, Endpoint::MFS);
        let route = state.run_once(&reply, &VfsTransIdCodec);
        assert!(matches!(route, Route::FsReply { worker_slot: 2, .. }));
        assert_eq!(state.worker_pool.get(2).unwrap().state, WorkerState::Busy);
        assert_eq!(state.comm.vmnts[0].cur_reqs, 0);
    }

    // ── S12 第二片:fs_sendrec 对话原语 + flush_send_queue 补发 ──

    extern crate alloc;

    /// 可脚本失败的最小 IPC 传输(sendnb 记录 + 可注入失败)。
    struct FlushIpc {
        sent: core::cell::RefCell<alloc::vec::Vec<(Endpoint, Message)>>,
        fail: bool,
    }
    impl minix_sys::ipc::IpcTransport for FlushIpc {
        fn send(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> { Ok(()) }
        fn receive(
            &self,
            _s: Endpoint,
            _m: &mut Message,
        ) -> Result<minix_sys::ipc::IpcStatus, minix_sys::ipc::TrapStatus> {
            Err(minix_sys::ipc::TrapStatus(-1))
        }
        fn sendrec(&self, _d: Endpoint, _m: &mut Message) -> Result<(), minix_sys::ipc::TrapStatus> { Ok(()) }
        fn notify(&self, _d: Endpoint) -> Result<(), minix_sys::ipc::TrapStatus> { Ok(()) }
        fn sendnb(&self, d: Endpoint, m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
            if self.fail {
                return Err(minix_sys::ipc::TrapStatus(-5));
            }
            self.sent.borrow_mut().push((d, *m));
            Ok(())
        }
        fn senda(&self, _t: &[minix_sys::ipc::AsyncSlot]) -> Result<(), minix_sys::ipc::TrapStatus> { Ok(()) }
        fn query_kerninfo_page(&self) -> Result<u64, minix_sys::ipc::TrapStatus> { Ok(0) }
    }

    /// 回复落地 + 补发的闭环Fixture:vmnt0(MFS) + 槽 2 在飞。
    fn dialogue_fixture() -> VfsState {
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;
        state
    }

    #[test]
    fn test_fs_sendrec_send_path_marks_waiting() {
        // comm.c:137-158 主路径:窗口开 → sendnb 投递 + 槽 WaitingForFs。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc { sent: core::cell::RefCell::new(alloc::vec::Vec::new()), fail: false },
        };
        let req = Message { m_type: 0x503, ..Message::default() };
        let r = state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 3, &req);
        assert_eq!(r, Ok(()));
        let w = state.worker_pool.get(3).unwrap();
        assert_eq!(w.state, WorkerState::WaitingForFs);
        assert_eq!(w.task, Some(Endpoint::MFS));
        assert_eq!(state.comm.vmnts[0].cur_reqs, 1);
        assert_eq!(probe.transport.sent.borrow().len(), 1);
        assert_eq!(probe.transport.sent.borrow()[0].1.m_type as u32, crate::fs_comm::TransId::add(0x503, 3));
    }

    #[test]
    fn test_fs_sendrec_vmnt_mismatch_is_eio() {
        // comm.c:137-140 — find_vmnt 找不到该 fs endpoint → EIO。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc { sent: core::cell::RefCell::new(alloc::vec::Vec::new()), fail: false },
        };
        let req = Message::default();
        assert_eq!(
            state.fs_sendrec(&mut probe, 0, Endpoint::from_generation_slot(1, 9), 3, &req),
            Err(CommError::NoVmnt)
        );
        assert!(state.worker_pool.get(3).unwrap().state == WorkerState::Idle);
    }

    #[test]
    fn test_fs_sendrec_edeadlk_when_caller_is_fs() {
        // comm.c:142-144 — 调用进程本身就是目标 FS → EDEADLK。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let caller_ep = Endpoint::from_generation_slot(1, 9);
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
            v.fs = caller_ep;
            v.dev = 2;
        }
        // 槽 3 绑定 fproc,其 endpoint 即目标 FS。
        let fp = state.fproc_table.get_mut(UserSlot::new(3)).unwrap();
        fp.endpoint = caller_ep;
        let w = state.worker_pool.get_mut(3).unwrap();
        w.fp_slot = Some(UserSlot::new(3));
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc { sent: core::cell::RefCell::new(alloc::vec::Vec::new()), fail: false },
        };
        let req = Message::default();
        assert_eq!(
            state.fs_sendrec(&mut probe, 1, caller_ep, 3, &req),
            Err(CommError::Deadlock)
        );
        assert!(probe.transport.sent.borrow().is_empty());
    }

    #[test]
    #[should_panic(expected = "fs_sendrec on already-waiting slot")]
    fn test_fs_sendrec_double_sendrec_asserts() {
        // comm.c:146 — `assert(self->w_sendrec == NULL)`。
        let mut state = dialogue_fixture(); // 槽 2 已 WaitingForFs
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc { sent: core::cell::RefCell::new(alloc::vec::Vec::new()), fail: false },
        };
        let req = Message::default();
        let _ = state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 2, &req);
    }

    #[test]
    fn test_fs_sendrec_queue_path_marks_waiting() {
        // comm.c:156-158 窗口满 → 排队,槽同样 WaitingForFs(sending++,
        // 不投递)。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        state.comm.vmnts[0].cur_reqs = 1; // max=1 已占满
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc { sent: core::cell::RefCell::new(alloc::vec::Vec::new()), fail: false },
        };
        let req = Message { m_type: 0x604, ..Message::default() };
        assert_eq!(state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 4, &req), Ok(()));
        assert!(probe.transport.sent.borrow().is_empty());
        assert_eq!(state.comm.sending, 1);
        assert_eq!(state.comm.vmnts[0].queued(), 1);
        assert_eq!(state.worker_pool.get(4).unwrap().state, WorkerState::WaitingForFs);
    }

    #[test]
    fn test_flush_after_reply_drains_queue() {
        // 闭环:在飞(槽2)+ 排队(槽4)→ 回复落地放行 → flush 补发
        // 槽 4 的请求(transid 戳)→ 窗口再占满。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc { sent: core::cell::RefCell::new(alloc::vec::Vec::new()), fail: false },
        };
        let req = Message { m_type: 0x604, ..Message::default() };
        // 窗口满 → 槽 4 排队(请求存槽内,未 stamp)。
        assert_eq!(state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 4, &req), Ok(()));
        // 回复落地槽 2,窗口放行。
        let reply = reply_msg(0x503, 2, Endpoint::MFS);
        assert_eq!(state.handle_fs_reply(&reply, &VfsTransIdCodec), Ok(2));
        // 补发:槽 4 的请求上瓦,stamped + 投递。
        assert_eq!(state.flush_send_queue(&mut probe.transport), 1);
        assert_eq!(probe.transport.sent.borrow().len(), 1);
        let (dst, sent) = probe.transport.sent.borrow()[0].clone();
        assert_eq!(dst, Endpoint::MFS);
        assert_eq!(sent.m_type as u32, crate::fs_comm::TransId::add(0x604, 4));
        assert_eq!(state.comm.vmnts[0].cur_reqs, 1);
        assert_eq!(state.comm.sending, 0);
        // 再 flush:窗口又满,无补发。
        assert_eq!(state.flush_send_queue(&mut probe.transport), 0);
    }

    #[test]
    fn test_flush_skips_send_failures_and_empty_mounts() {
        // comm.c:86 — 补发的 asynsend3 失败 `(void)` 忽略;空挂载槽不扫。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc { sent: core::cell::RefCell::new(alloc::vec::Vec::new()), fail: true },
        };
        let req = Message::default();
        assert_eq!(state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 4, &req), Ok(()));
        let reply = reply_msg(0x503, 2, Endpoint::MFS);
        assert_eq!(state.handle_fs_reply(&reply, &VfsTransIdCodec), Ok(2));
        assert_eq!(state.flush_send_queue(&mut probe.transport), 0);
        // 计数已随 fs_sendmore 先行递增(C 既有次序)。
        assert_eq!(state.comm.vmnts[0].cur_reqs, 1);
        assert!(probe.transport.sent.borrow().is_empty());
    }

    #[test]
    fn test_vfs_state_dmap_ctty_default() {
        let state = VfsState::new();
        assert_eq!(state.reviving, 0);
        assert!(state.current_fp_slot.is_none());
        assert!(state.worker_pool.all_idle());
        assert_eq!(state.boot_phase, BootPhase::PmHandshake);
        assert!(state.accept_requests);
        assert_eq!(state.pending, 0);
        assert!(!state.initialized);
    }

    #[test]
    fn test_vfs_state_init_fresh() {
        let mut state = VfsState::new();
        state.init_fresh();
        let slot = UserSlot::new(0);
        let fp = state.fproc_table.get(slot).unwrap();
        assert_eq!(fp.pid, PID_FREE);
        assert!(fp.endpoint.is_none());
        assert!(fp.root_dir.is_none());
        assert!(fp.work_dir.is_none());
        for filp in &fp.filps {
            assert!(filp.is_none());
        }
        assert_eq!(state.boot_phase, BootPhase::PmHandshake);
    }

    #[test]
    fn test_pm_handshake_fills_slot() {
        let mut state = VfsState::new();
        state.init_fresh();

        let msg = VfsPmInit {
            slot: 11,
            pid: 1,
            endpoint: Endpoint::INIT,
        }
        .encode();
        let complete = state.pm_handshake_step(&msg).unwrap();
        assert!(!complete);

        let fp = state.fproc_table.get(UserSlot::new(11)).unwrap();
        assert_eq!(fp.pid, 1);
        assert_eq!(fp.endpoint, Endpoint::INIT);
        assert_eq!(fp.flags, FpFlags::NOFLAGS);
        assert_eq!(fp.blocked_on, BlockedOn::None);
        assert_eq!(fp.real_uid, SYS_UID);
        assert_eq!(fp.eff_uid, SYS_UID);
        assert_eq!(fp.real_gid, SYS_GID);
        assert_eq!(fp.eff_gid, SYS_GID);
        assert_eq!(fp.umask, !0);
        assert_eq!(state.boot_phase, BootPhase::PmHandshake);
    }

    #[test]
    fn test_pm_handshake_none_terminates() {
        let mut state = VfsState::new();
        state.init_fresh();

        let terminator = VfsPmInit {
            slot: 0,
            pid: 0,
            endpoint: Endpoint::NONE,
        }
        .encode();
        let complete = state.pm_handshake_step(&terminator).unwrap();
        assert!(complete);
        assert_eq!(state.boot_phase, BootPhase::InitTables);
    }

    #[test]
    fn test_pm_handshake_wrong_type() {
        let mut state = VfsState::new();
        state.init_fresh();

        let msg = Message {
            m_type: VFS_PM_INIT + 1,
            ..Message::default()
        };
        assert_eq!(
            state.pm_handshake_step(&msg),
            Err(VfsPmInitError::WrongMessageType(VFS_PM_INIT + 1))
        );
    }

    #[test]
    fn test_pm_handshake_slot_out_of_range() {
        let mut state = VfsState::new();
        state.init_fresh();

        let msg = VfsPmInit {
            slot: 9999,
            pid: 1,
            endpoint: Endpoint::PM,
        }
        .encode();
        assert_eq!(
            state.pm_handshake_step(&msg),
            Err(VfsPmInitError::SlotOutOfRange(9999))
        );
        // fail-closed：越界消息不得破坏表状态。
        assert_eq!(state.boot_phase, BootPhase::PmHandshake);
    }

    #[test]
    fn test_finish_init_runs_to_running() {
        let mut state = VfsState::new();
        state.init_fresh();

        let terminator = VfsPmInit {
            slot: 0,
            pid: 0,
            endpoint: Endpoint::NONE,
        }
        .encode();
        state.pm_handshake_step(&terminator).unwrap();
        state.finish_init();

        assert!(state.initialized);
        assert_eq!(state.boot_phase, BootPhase::Running);
        assert!(state.accept_requests);
    }

    #[test]
    #[should_panic(expected = "finish_init requires completed VFS_PM_INIT handshake")]
    fn test_finish_init_requires_handshake() {
        let mut state = VfsState::new();
        state.init_fresh();
        state.finish_init();
    }

    #[test]
    fn test_root_mount_gate_contract() {
        let mut state = VfsState::new();
        state.init_fresh();
        let terminator = VfsPmInit {
            slot: 0,
            pid: 0,
            endpoint: Endpoint::NONE,
        }
        .encode();
        state.pm_handshake_step(&terminator).unwrap();

        state.do_init_root();
        assert_eq!(state.boot_phase, BootPhase::Running);
        assert!(state.accept_requests);
    }

    #[test]
    fn test_vfs_state_is_from_pm() {
        let mut state = VfsState::new();
        state.current_message.m_source = Endpoint::PM;
        assert!(state.is_from_pm());

        state.current_message.m_source = Endpoint::VM;
        assert!(!state.is_from_pm());
    }

    #[test]
    fn test_handle_pm_fork() {
        let mut state = VfsState::new();
        let parent_ep = Endpoint::from_generation_slot(1, 5);
        let child_ep = Endpoint::from_generation_slot(1, 10);

        let result = state.handle_pm_fork(parent_ep, child_ep, 1234);
        assert!(result.is_ok());

        let child_slot = UserSlot::new(10);
        let child_fp = state.fproc_table.get(child_slot).unwrap();
        assert_eq!(child_fp.pid, 1234);
        assert_eq!(child_fp.endpoint, child_ep);
        assert_eq!(child_fp.flags, FpFlags::NOFLAGS);
    }

    #[test]
    fn test_handle_pm_fork_invalid_parent() {
        let mut state = VfsState::new();
        state.init_fresh();

        let parent_ep = Endpoint::KERNEL;
        let child_ep = Endpoint::from_generation_slot(1, 10);
        let result = state.handle_pm_fork(parent_ep, child_ep, 1234);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_pm_fork_slot_not_free() {
        let mut state = VfsState::new();
        let parent_ep = Endpoint::from_generation_slot(1, 5);
        let child_ep = Endpoint::from_generation_slot(1, 10);

        let child_slot = UserSlot::new(10);
        state.fproc_table.get_mut(child_slot).unwrap().pid = 999;

        let result = state.handle_pm_fork(parent_ep, child_ep, 1234);
        assert!(result.is_err());
    }

    #[test]
    fn test_pm_message_type() {
        assert_eq!(PmMessageType::Fork, PmMessageType::Fork);
        assert_eq!(PmMessageType::Unknown(99), PmMessageType::Unknown(99));
    }

    #[test]
    fn test_boot_phase_ordering() {
        assert_ne!(BootPhase::PmHandshake, BootPhase::InitTables);
        assert_ne!(BootPhase::InitTables, BootPhase::Mounting);
        assert_ne!(BootPhase::Mounting, BootPhase::Running);
    }

    #[test]
    fn test_reply_intent_variants() {
        assert_eq!(ReplyIntent::Reply(0), ReplyIntent::Reply(0));
        assert_eq!(ReplyIntent::ReplyLater, ReplyIntent::ReplyLater);
        assert_eq!(ReplyIntent::NoReply, ReplyIntent::NoReply);
        assert_ne!(ReplyIntent::Reply(0), ReplyIntent::NoReply);
    }

    // ——— 09-main-loop new tests (route / transid / reviving) ———

    #[test]
    fn test_transid_codec_roundtrip() {
        let codec = VfsTransIdCodec;
        for slot in [0, 1, 7, 8] {
            let raw = codec.encode(slot);
            assert!(codec.is_fs_transid(raw));
            assert_eq!(codec.decode(raw), Some(slot));
        }
        // VFS_READ (0x100) must NOT be a FS transid — encoding not overlapping.
        assert!(!codec.is_fs_transid(0x100));
        assert_eq!(codec.decode(0x100), None);
    }

    #[test]
    fn test_transid_codec_two_impls_differ() {
        let vfs = VfsTransIdCodec;
        let test = TestTransIdCodec { base: 0xC00 };
        let raw = vfs.encode(0);
        assert_eq!(raw, 0xB01);
        assert!(vfs.is_fs_transid(raw));
        assert!(!test.is_fs_transid(raw));
        assert_eq!(vfs.decode(raw), Some(0));
        assert_eq!(test.decode(raw), None);
        // Test codec roundtrip with its own base
        let raw2 = test.encode(2);
        assert!(test.is_fs_transid(raw2));
        assert_eq!(test.decode(raw2), Some(2));
    }

    #[test]
    fn test_route_fs_reply() {
        let state = VfsState::new();
        let codec = VfsTransIdCodec;
        let raw_transid = codec.encode(2); // 0xB03
        let msg = Message {
            m_type: 0x1234 << 16 | raw_transid as i32,
            m_source: Endpoint::MFS,
            ..Message::default()
        };
        let route = state.route_message(&msg, &codec);
        assert!(matches!(route, Route::FsReply { worker_slot: 2, .. }));
    }

    #[test]
    fn test_route_pm() {
        let state = VfsState::new();
        let codec = VfsTransIdCodec;
        let msg = Message {
            m_source: Endpoint::PM,
            m_type: 0x900,
            ..Message::default()
        };
        assert_eq!(state.route_message(&msg, &codec), Route::Pm);
    }

    #[test]
    fn test_route_notify_ds() {
        let state = VfsState::new();
        let codec = VfsTransIdCodec;
        // NOTIFY_MESSAGE base 0x1000 + DS_PROC_NR source
        let msg = Message {
            m_source: Endpoint::DS,
            m_type: 0x1000, // NOTIFY_MESSAGE
            ..Message::default()
        };
        let route = state.route_message(&msg, &codec);
        assert!(matches!(route, Route::Notify { .. }));
    }

    #[test]
    fn test_route_task_ignored() {
        let state = VfsState::new();
        let codec = VfsTransIdCodec;
        // KERNEL is a task (slot -1) and m_type 0x200 is not NOTIFY (0x1000 range)
        // → main.c:118 `who_p < 0` → TaskIgnored
        let msg = Message {
            m_source: Endpoint::KERNEL,
            m_type: 0x200,
            ..Message::default()
        };
        let route = state.route_message(&msg, &codec);
        assert!(matches!(route, Route::TaskIgnored { .. }));
    }

    #[test]
    fn test_route_bdev_cdev_sdev() {
        let state = VfsState::new();
        let codec = VfsTransIdCodec;
        let bdev = Message {
            m_type: 0x580, // BDEV_RS_BASE + BDEV_REPLY (com.h:963-964)
            m_source: Endpoint::from_generation_slot(0, 5),
            ..Message::default()
        };
        assert_eq!(state.route_message(&bdev, &codec), Route::Bdev);
        let cdev = Message {
            m_type: 0x480, // CDEV_RS_BASE + CDEV_REPLY (com.h:919-920)
            m_source: Endpoint::from_generation_slot(0, 5),
            ..Message::default()
        };
        assert_eq!(state.route_message(&cdev, &codec), Route::Cdev);
        let sdev = Message {
            m_type: 0x1980, // SDEV_RS_BASE + SDEV_REPLY (com.h:1038)
            m_source: Endpoint::from_generation_slot(0, 5),
            ..Message::default()
        };
        assert_eq!(state.route_message(&sdev, &codec), Route::Sdev);
    }

    #[test]
    fn test_route_syscall() {
        let state = VfsState::new();
        let codec = VfsTransIdCodec;
        let msg = Message {
            m_type: crate::call_table::VfsCallNum::Open as i32,
            m_source: Endpoint::from_generation_slot(0, 5),
            ..Message::default()
        };
        let route = state.route_message(&msg, &codec);
        assert!(matches!(route, Route::Syscall { .. }));
        if let Route::Syscall { call } = route {
            assert_eq!(call, crate::call_table::VfsCallNum::Open);
        }
    }

    #[test]
    fn test_poll_reviving_priority() {
        let mut state = VfsState::new();
        state.init_fresh();
        // No reviving → Ready
        assert_eq!(state.poll_next(), PollResult::Ready);
        // Enqueue a pipe-blocked process with REVIVED
        let slot = UserSlot::new(3);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.pid = 42;
            fp.endpoint = Endpoint::from_generation_slot(0, 3);
            fp.blocked_on = crate::fproc::BlockedOn::Pipe(crate::fproc::PipeBlock {
                call: crate::fproc::PipeIo::Read,
                fd: 3,
                buf: minix_types::VirBytes::new(0x1000),
                nbytes: 100,
                cum_io: 0,
            });
        }
        state.enqueue_revive(slot).unwrap();
        assert_eq!(state.reviving, 1);
        let pr = state.poll_next();
        assert!(matches!(pr, PollResult::RevivedPipe { slot: s } if s == slot));
        assert_eq!(state.reviving, 0);
    }

    #[test]
    fn test_unblock_pipe_queued() {
        let mut state = VfsState::new();
        state.init_fresh();
        let slot = UserSlot::new(4);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.pid = 10;
            fp.endpoint = Endpoint::from_generation_slot(0, 4);
            fp.blocked_on = crate::fproc::BlockedOn::Pipe(crate::fproc::PipeBlock {
                call: crate::fproc::PipeIo::Write,
                fd: 1,
                buf: minix_types::VirBytes::new(0x2000),
                nbytes: 64,
                cum_io: 0,
            });
        }
        state.enqueue_revive(slot).unwrap();
        let out = state.unblock(slot).unwrap();
        assert_eq!(out, UnblockOutcome::QueuedPipe { slot });
    }

    #[test]
    fn test_unblock_flock_revived() {
        let mut state = VfsState::new();
        state.init_fresh();
        let slot = UserSlot::new(5);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.pid = 11;
            fp.endpoint = Endpoint::from_generation_slot(0, 5);
            fp.blocked_on = crate::fproc::BlockedOn::Flock(crate::fproc::FlockBlock {
                fd: 2,
                cmd: crate::fproc::FlockCmd::SetLkw,
                arg: minix_types::VirBytes::new(0x3000),
            });
        }
        state.enqueue_revive(slot).unwrap();
        let out = state.unblock(slot).unwrap();
        assert_eq!(out, UnblockOutcome::RevivedLock { slot });
        // reviving cleared and current_fp_slot set
        assert_eq!(state.reviving, 0);
        assert_eq!(state.current_fp_slot, Some(slot));
    }

    #[test]
    fn test_reviving_counter() {
        let mut state = VfsState::new();
        state.init_fresh();
        let s1 = UserSlot::new(1);
        let s2 = UserSlot::new(2);
        for s in [s1, s2] {
            let fp = state.fproc_table.get_mut(s).unwrap();
            fp.pid = 20 + s.get() as i32;
            fp.endpoint = Endpoint::from_generation_slot(0, s.get() as i32);
            fp.blocked_on = crate::fproc::BlockedOn::Flock(crate::fproc::FlockBlock {
                fd: 0,
                cmd: crate::fproc::FlockCmd::SetLkw,
                arg: minix_types::VirBytes::new(0),
            });
            state.enqueue_revive(s).unwrap();
        }
        assert_eq!(state.reviving, 2);
        state.unblock(s1).unwrap();
        assert_eq!(state.reviving, 1);
        state.unblock(s2).unwrap();
        assert_eq!(state.reviving, 0);
    }

    #[test]
    fn test_reply_intent_reply_or_later() {
        let state = VfsState::new();
        assert_eq!(
            state.reply(Endpoint::from_generation_slot(0, 1), 0),
            ReplyIntent::Reply(0)
        );
        assert_eq!(state.reply(Endpoint::NONE, 0), ReplyIntent::NoReply);
        assert_eq!(
            state.reply_code(Endpoint::from_generation_slot(0, 1), 5),
            ReplyIntent::Reply(5)
        );
    }

    #[test]
    fn test_callnum_from_raw_single_truth() {
        // P2-2: `CallTable`/`CallResolver` are gone — `from_raw` is the one
        // resolution truth, and unknown numbers stay unknown.
        use crate::call_table::{VFS_BASE, VfsCallNum};
        assert_eq!(
            VfsCallNum::from_raw(VFS_BASE + (VfsCallNum::Open as u32 - VFS_BASE)),
            Some(VfsCallNum::Open)
        );
        assert_eq!(VfsCallNum::from_raw(VFS_BASE + 200), None);
    }
}

    #[test]
    fn test_lu_prepare_matrix() {
        // Idle pool + request-free/protocol-free → ready (`main.c:308-317`).
        assert_eq!(lu_prepare(true, LuState::RequestFree), Ok(()));
        assert_eq!(lu_prepare(true, LuState::ProtocolFree), Ok(()));
        // Busy pool blocks the update (`main.c:310-312`).
        assert_eq!(
            lu_prepare(false, LuState::RequestFree),
            Err(UnblockError::NotReady)
        );
        // Other states refuse (`main.c:320-321`).
        assert_eq!(lu_prepare(true, LuState::Other), Err(UnblockError::NotReady));
        assert_eq!(lu_prepare(true, LuState::Null), Err(UnblockError::NotReady));
        // Rollback: leaving a request-free state back to Null re-creates
        // workers in C (`main.c:330-339`); ARCH A-1 makes it a no-op.
        assert!(lu_rollback_needs_workers(
            LuState::RequestFree,
            LuState::Null
        ));
        assert!(!lu_rollback_needs_workers(LuState::Null, LuState::Null));
        assert!(!lu_rollback_needs_workers(
            LuState::RequestFree,
            LuState::RequestFree
        ));
        // New-instance init (`main.c:349-356`).
        assert!(init_lu_needs_workers(LuState::ProtocolFree));
        assert!(!init_lu_needs_workers(LuState::Other));
        // ENOTREADY is the C answer (`:321`).
        assert_eq!(UnblockError::NotReady.to_errno(), minix_types::ENOTREADY);
    }

    #[test]
    fn test_route_enosys_and_rs_truth() {
        let state = VfsState::new();
        let codec = VfsTransIdCodec;
        // An unknown raw past VFS_BASE answers ENOSYS — never a Read stand-in
        // (`main.c:283-294`; P1-1 placeholder removed).
        let msg = Message {
            m_type: (crate::call_table::VFS_BASE + 200) as i32,
            m_source: Endpoint::from_generation_slot(1, 2),
            ..Message::default()
        };
        assert_eq!(
            state.route_message(&msg, &codec),
            Route::Enosys { raw: (crate::call_table::VFS_BASE + 200) as u32 }
        );
        // RS reply prefixes at the real C bases (`com.h:919/:963/:1038`).
        assert!(VfsState::is_bdev_rs(0x580));
        assert!(VfsState::is_cdev_rs(0x480));
        assert!(VfsState::is_sdev_rs(0x1980));
        // The bases must not collide with the syscall namespace.
        assert!(!VfsState::is_bdev_rs(crate::call_table::VfsCallNum::Open as u32));
        assert!(!VfsState::is_cdev_rs(0xA00)); // FS_REQ namespace (com.h:589)
    }

#[cfg(test)]
mod run_once_tests {
    use super::*;
    use crate::call_table::VfsCallNum;
    use minix_types::{Endpoint, Message, MessageM7, MessageUnion};

    #[test]
    fn test_run_once_gated_marks_pending() {
        let mut state = VfsState::new();
        state.init_fresh();
        state.set_accept_requests(false);
        let codec = VfsTransIdCodec;
        let msg = Message {
            m_source: Endpoint::from_generation_slot(0, 5),
            m_type: VfsCallNum::Open as i32,
            ..Message::default()
        };
        let route = state.run_once(&msg, &codec);
        assert!(matches!(route, Route::Syscall { .. }));
        assert_eq!(state.pending, 1);
        let fp = state.fproc_table.get(UserSlot::new(5)).unwrap();
        assert!(fp.flags.contains(FpFlags::PENDING));
        assert_eq!(state.current_fp_slot, Some(UserSlot::new(5)));
    }

    #[test]
    fn test_run_once_gated_dedups_pending() {
        let mut state = VfsState::new();
        state.init_fresh();
        state.set_accept_requests(false);
        let codec = VfsTransIdCodec;
        let msg = Message {
            m_source: Endpoint::from_generation_slot(0, 5),
            m_type: VfsCallNum::Open as i32,
            ..Message::default()
        };
        state.run_once(&msg, &codec);
        state.run_once(&msg, &codec);
        assert_eq!(state.pending, 1);
    }

    #[test]
    fn test_run_once_accepts_when_open() {
        let mut state = VfsState::new();
        state.init_fresh();
        let codec = VfsTransIdCodec;
        let msg = Message {
            m_source: Endpoint::from_generation_slot(0, 5),
            m_type: VfsCallNum::Open as i32,
            ..Message::default()
        };
        let route = state.run_once(&msg, &codec);
        assert!(matches!(route, Route::Syscall { .. }));
        assert_eq!(state.pending, 0);
    }

    #[test]
    fn test_run_once_enosys_for_unknown() {
        // 未知调用号 → Enosys 路由（main.c:283-294 的 ENOSYS 语义）。
        let mut state = VfsState::new();
        state.init_fresh();
        let codec = VfsTransIdCodec;
        let msg = Message {
            m_source: Endpoint::from_generation_slot(0, 5),
            m_type: (VfsCallNum::Open as i32) + 200,
            ..Message::default()
        };
        let route = state.run_once(&msg, &codec);
        assert!(matches!(route, Route::Enosys { .. }));
    }

    #[test]
    fn test_run_once_pm_short_circuit() {
        let mut state = VfsState::new();
        state.init_fresh();
        let codec = VfsTransIdCodec;
        let msg = Message {
            m_source: Endpoint::PM,
            m_type: VfsCallNum::Open as i32,
            ..Message::default()
        };
        let route = state.run_once(&msg, &codec);
        assert!(matches!(route, Route::Pm));
    }
}
