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
    /// 待发送的回复（W3 回复半）：`run_once` 分发完把结果折成
    /// `(调用方, m_type)` 入队，`run()` 在每轮循环尾用
    /// [`send_reply`] 发出（C `reply(who_e, result)` 的位置）。
    /// `Suspend` 的臂不入队——它们的回复在 FS 应答落地时发。
    pub pending_reply: Option<(Endpoint, i32)>,
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
                    let result = crate::syscalls::dispatch_syscall(self, call);
                    self.queue_reply(msg.m_source, result);
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
        route
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
        self.pending_reply = Some((target, code));
    }

    /// 取走待发回复（`run()` 每轮循环尾调用）。
    pub fn take_reply(&mut self) -> Option<(Endpoint, i32)> {
        self.pending_reply.take()
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
        // C main.c:204-206 — `*w_sendrec = m_in` 后清 w_task；槽模型里
        // "是否在等"由 task 承载，回复体留在 sendrec 供续接读取。
        wp.sendrec = Some(*msg);
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
fn send_reply(target: Endpoint, code: i32) {
    use minix_sys::ipc::IpcTransport;
    let reply = Message { m_type: code, ..Message::default() };
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
    send_reply(Endpoint::PM, minix_types::OK);
    state.finish_init();

    // 启动段(main.c:441):向 DS 订阅驱动上线事件(失败远端忽略)。
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
                if let Some((target, code)) = state.take_reply() {
                    send_reply(target, code);
                }
            }
            SefEvent::Signal(_) => {}
            // init_restart ≡ init_fresh(已文档化);LU prepare/rollback 的
            // 决策函数就位,RS 推进面挂通电。
            SefEvent::Init(_) => state.init_fresh(),
            SefEvent::PingInvalid => {}
        }
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

    /// W3 回复半：`SyscallResult` → 回复入队的映射（C `do_work` 尾部
    /// `reply(who_e, result)`）。`Suspend` 不入队（回复在 FS 应答落地时
    /// 发），`Nosys` 显式回 ENOSYS，目标为 NONE 时不发。
    #[test]
    fn test_queue_reply_maps_syscall_results() {
        use crate::call_table::SyscallResult;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();

        state.queue_reply(user, SyscallResult::Ok(7));
        assert_eq!(state.take_reply(), Some((user, 7)));

        state.queue_reply(user, SyscallResult::Error(minix_types::EINVAL));
        assert_eq!(state.take_reply(), Some((user, minix_types::EINVAL)));

        state.queue_reply(user, SyscallResult::Nosys);
        assert_eq!(state.take_reply(), Some((user, minix_types::ENOSYS)));

        state.queue_reply(user, SyscallResult::Suspend);
        assert_eq!(state.take_reply(), None, "Suspend 的回复在 FS 应答时发");

        state.queue_reply(Endpoint::NONE, SyscallResult::Ok(0));
        assert_eq!(state.take_reply(), None, "无调用方不发");
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
        assert_eq!(delivered.m_type as u32, crate::fs_comm::TransId::add(0x503, 2));
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
