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

use crate::device_map::{DmapTable, SmapTable, DEV_IMGRD};
use crate::fcntl::LockTable;
use crate::filp::FilpTable;
use crate::fproc::{BlockedOn, FProcTable, FpFlags, PID_FREE};
use crate::fs_comm::{CommError, FsTransport, GlobalComm};
use crate::mount::{DevCodec, FsSuperblock, MountError, SuperblockReader};
use crate::request::WireFsClient;
use crate::vnode::VnodeTable;
use crate::vmnt::{VmntFlags, VmntTable};
use minix_sef::SefEvent;
use minix_sys::ipc::IpcTransport as _;
use minix_sys::syscall::KernelCallTransport;
use crate::worker::WorkerPool;
use minix_types::{DevId, Endpoint, Gid, Message, NO_DEV, NR_PROCS, Uid, UserSlot, VfsPmInit, VfsPmInitError};

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
    /// `struct statvfs` 的 VFS 侧缓冲（C `fill_statvfs` 的局部 `struct
    /// statvfs buf`）：FS 经 direct grant 整块回填它，VFS 补本地字段后再整块
    /// 拷给用户。放在状态里而不是栈上，是因为 grant 要指向一个**稳定地址**。
    pub statvfs_buf: minix_types::StatvfsBuf,
    /// `selecttab[MAXSELECTS]`（C `select.c:31`）——每个挂起的 `select` 一个槽，
    /// 存调用者、三张 fd 集与阻塞/超时状态。
    pub select_table: crate::select::SelectTable,
    /// 驱动标签 → 端点的**本地目录**（C `do_mapdriver` 的
    /// `ds_retrieve_label_endpt` 那一跳）。生产填充面（DS 事件里带标签时写入）
    /// 还没接线——见 `LabelDir` 的注记；空表下 `mapdriver` 对任何标签都回
    /// EINVAL（fail-closed，不假装解析成功）。
    pub driver_labels: alloc::vec::Vec<(alloc::string::String, Endpoint)>,
    /// 根文件系统的端点（C `glo.h:21` 的 `EXTERN int ROOT_FS_E`）。
    ///
    /// 块设备 open 要用它选 `v_bfs_e`（设备没被别的挂载占着就归根）并决定
    /// 要不要补发 `REQ_NEW_DRIVER`（open.c:186-216）。**赋值面在根挂载**
    /// （`mount.c:326-328` 的 `ROOT_FS_E = fs_e`，NS4 已接：见
    /// [`Self::do_init_root`]）；在此之前恒为 `NONE`，块设备 open 在"设备未被
    /// 挂载占着"这条路上按 C 的 newdriver 失败路径收尾（`bdev_close` +
    /// `ENXIO`），不假装通知成功。
    pub root_fs_e: Endpoint,
    /// 根文件系统的设备号（C `glo.h:20` 的 `EXTERN dev_t ROOT_DEV`）。
    ///
    /// 与 [`Self::root_fs_e`] 同点赋值（`mount.c:326-327`），boot 链上即
    /// `DEV_IMGRD`。
    pub root_dev: DevId,
    /// 根挂载计数（C `glo.h:19` 的 `EXTERN int have_root`）。
    ///
    /// `mount_fs` 用它区分"根可挂两次"（ramdisk/boot 盘，mount.c:208-210）
    /// 的 `mount_root` 判定；boot 根挂载完成后为 1。
    pub have_root: u32,
    /// `none` 伪设备池（C `mount.c:33-37` 的 nonedev 位图）。
    ///
    /// `mount_pfs` 要从这里分一个伪设备（`find_free_nonedev`）。
    pub nonedev: crate::mount::NonedevBitmap,
    pub pending_fs: Option<PendingFs>,
    /// 待投递的 `REQ_PUTNODE` 队列（`put_vnode` 慢路径的通知面；C 是
    /// worker 同步 `fs_sendrec`，模型里臂/续接上下文不能同步发——排队
    /// 由主循环统一投，回复 C 只 printf，丢弃无损）。
    pub pending_puts: alloc::vec::Vec<crate::vnode::PutNodeReq>,

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
            select_table: crate::select::SelectTable::new(),
            driver_labels: alloc::vec::Vec::new(),
            root_fs_e: Endpoint::NONE,
            root_dev: NO_DEV,
            have_root: 0,
            nonedev: crate::mount::NonedevBitmap::default(),
            statvfs_buf: minix_types::StatvfsBuf::new(),
            pending_fs: None,
            pending_puts: alloc::vec::Vec::new(),

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
    pub fn finish_init(
        &mut self,
        kernel: &impl KernelCallTransport,
        transport: &impl minix_sys::ipc::IpcTransport,
    ) -> Result<(), MountError> {
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
        //                  （dmap 的 boot 装配半 = E-RPROCTAB 消费侧；见
        //                  new_edge3 新登记——RS 侧的 wire 契约归 NS2，消费
        //                  适配在其落地时接。boot 链上 dmap[MEMORY_MAJOR]
        //                  为空 ⇒ 根挂载按 C 同款 EINVAL 失败。）

        // main.c:468-483 — fp_lock（槽位锁，单线程下归 07）+ filp/rd/wd 清零。
        self.fproc_table.init_phase2();

        // main.c:485-489 — init_vnodes()/init_vmnts()/init_select()/init_filps()
        //                  （表结构归 04~06，DEFERRED）。

        // main.c:492-497 — worker_start(fproc_addr(VFS_PROC_NR), do_init_root, ...)。
        // C 的 do_init_root 失败即 panic（main.c:519-520）——这里把 Err 交给
        // 调用方（run() panic），决策面保持可测。
        self.do_init_root(kernel, transport)?;

        assert_eq!(self.boot_phase, BootPhase::Running);
        self.initialized = true;
        Ok(())
    }

    /// Root mount sequence (main.c:501-527) — NS4/W6 执行编排。
    ///
    /// 拒绝新请求（`worker_allow(FALSE)`）后挂 pipe fs 与根 fs：PFS 的
    /// `req_readsuper` 失败只记录（C printf，挂载照旧），根挂载失败返回
    /// [`MountError`]（C `panic("Failed to initialize root")`，main.c:519-520
    /// 的 panic 归调用方）。成功后相位推进 `Running`。
    pub fn do_init_root(
        &mut self,
        kernel: &impl KernelCallTransport,
        transport: &impl minix_sys::ipc::IpcTransport,
    ) -> Result<(), MountError> {
        assert_eq!(
            self.boot_phase,
            BootPhase::InitTables,
            "do_init_root requires post-handshake phase"
        );
        self.boot_phase = BootPhase::Mounting;

        // main.c:503 — worker_allow(FALSE)：挂载期间拒绝新请求（含 init(8)）。
        self.set_accept_requests(false);

        // main.c:505 — mount_pfs()：失败容忍（C printf 后继续）。
        // C: printf("VFS: unable to mount PFS (%d)\n", r)——诊断输出，
        // 无行为权重；no_std 载体无 stdout 承诺，这里静默同权。
        let _ = self.mount_pfs(kernel, transport);

        // main.c:508-518 — mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR,
        //                   0, "mfs", "fs_imgrd")；失败 = C panic 路径。
        let r = self.mount_fs_root(kernel, transport);

        match r {
            Ok(()) => {
                // main.c:525 — worker_allow(TRUE)：根文件系统就绪。
                self.set_accept_requests(true);
                self.boot_phase = BootPhase::Running;
                Ok(())
            }
            // C: panic("Failed to initialize root")——门不复位（进程即将
            // 终止，worker 门无观察者）；错误交调用方决定处置。
            Err(e) => Err(e),
        }
    }

    /// `mount_pfs`（`mount.c:391-425`）：PFS 以固定身份领一个 `none` 伪设备
    /// 与一个 vmnt 槽，`req_readsuper` 确认后仅回填 `m_fs_flags`（C 忽略
    /// 节点明细）。库存不足按 C 的 panic 路径返回 Err（调用方终止 boot）。
    fn mount_pfs(
        &mut self,
        kernel: &impl KernelCallTransport,
        transport: &impl minix_sys::ipc::IpcTransport,
    ) -> Result<(), MountError> {
        // find_free_nonedev + get_free_vmnt（C panic → 这里 Err 上抛）。
        let dev = crate::mount::alloc_nonedev(&mut self.nonedev)?;
        let slot = self.vmnt_table.alloc().map_err(|_| MountError::NoMem)?;

        let plan = crate::mount::pfs_mount_plan(dev, slot.0 as u8);
        {
            let v = self.vmnt_table.get_mut(slot).ok_or(MountError::NoMem)?;
            v.fs = Endpoint::PFS;
            v.dev = plan.dev;
            v.label = plan.label.into();
            v.mount_path = plan.mount_path.into();
            v.mount_dev = plan.mount_dev.into();
            v.fs_flags = 0;
        }

        // req_readsuper(vmp, "", dev, FALSE, FALSE)（mount.c:417）——确认往返，
        // 成功只回填 fs_flags；节点明细按 C 忽略。
        let mut client = WireFsClient { grants: &mut self.grants, kernel, ipc: transport };
        let mut sb = FsSuperblock { client: &mut client, fs_e: Endpoint::PFS, label: String::new() };
        match sb.read_super(plan.dev, false, false) {
            Ok(info) => {
                if let Some(v) = self.vmnt_table.get_mut(slot) {
                    v.fs_flags = info.fs_flags;
                }
                Ok(())
            }
            // C: printf + 挂载照旧——PFS 缺席不阻塞 boot（管道面后补）。
            Err(e) => Err(e),
        }
    }

    /// `mount_fs` 的根路径（`mount.c:156-350` 的 `mount_root` 分支）：
    /// DEV_IMGRD 上的 mfs 挂为 `/`，装配 ROOT_DEV/ROOT_FS_E、根 vnode、
    /// MAKEROOT 全表与 bspec 扫描。挂载点查找/胶水阶段（非根路径）归
    /// 18-mount 的 `do_mount`。
    fn mount_fs_root(
        &mut self,
        kernel: &impl KernelCallTransport,
        transport: &impl minix_sys::ipc::IpcTransport,
    ) -> Result<(), MountError> {
        let dev = DEV_IMGRD;
        let fs_e = Endpoint::MFS;
        let mount_root = true; // mount_path "/" 且 boot 期 have_root < 2

        // mount.c:170-183 — 非 none 设备先查 dmap 拿驱动标签；无驱动 = EINVAL
        // （"no driver for dev"）。boot 链上 dmap[MEMORY_MAJOR] 的装配半
        // （rproctab 消费）未接时，这里按 C 同款失败——不假装有盘。
        let major = DevCodec::major(dev);
        let label = {
            let entry = self
                .dmap_table
                .get(major)
                .filter(|e| e.is_mapped())
                .ok_or(MountError::Inval)?;
            let end = entry.label.iter().position(|&b| b == 0).unwrap_or(entry.label.len());
            alloc::string::String::from_utf8_lossy(&entry.label[..end]).into_owned()
        };

        // mount.c:190-200 — 设备已挂 → EBUSY；领空闲 vmnt 槽 → ENOMEM。
        if self.vmnt_table.find_by_dev(dev).is_some() {
            return Err(MountError::Busy);
        }
        let slot = self.vmnt_table.alloc().map_err(|_| MountError::NoMem)?;

        // mount.c:248-251 — 根 vnode（mount_root 跳过挂载点查找/胶水）。
        let root_vn = self.vnode_table.alloc().map_err(|_| MountError::NoMem)?;

        // mount.c:256-262 — isokendpt(fs_e) + 记系统进程旗标。
        let fs_slot = UserSlot::new(fs_e.slot() as usize);
        if self.fproc_table.get_mut(fs_slot).is_none() {
            self.vmnt_table.mark_free(slot);
            return Err(MountError::Inval);
        }
        self.fproc_table
            .get_mut(fs_slot)
            .unwrap()
            .flags
            .insert(FpFlags::SRV_PROC);

        // mount.c:264-269 — vmnt 基础数据 + MOUNTING 旗标。
        {
            let v = self.vmnt_table.get_mut(slot).ok_or(MountError::NoMem)?;
            v.fs = fs_e;
            v.dev = dev;
            v.mount_path = alloc::string::String::from("/");
            v.mount_dev = alloc::string::String::from("bootramdisk");
            v.fstype = alloc::string::String::from("mfs");
            v.flags.insert(VmntFlags::MOUNTING);
        }

        // mount.c:270-272 — req_readsuper 往返（grant+sendrec+revoke）。
        let read = {
            let mut client =
                WireFsClient { grants: &mut self.grants, kernel, ipc: transport };
            let mut sb = FsSuperblock {
                client: &mut client,
                fs_e,
                label: label.clone(),
            };
            sb.read_super(dev, false, mount_root)
        };
        if let Some(v) = self.vmnt_table.get_mut(slot) {
            v.flags.remove(VmntFlags::MOUNTING); // mount.c:273
        }

        let info = match read {
            Ok(info) => info,
            // mount.c:277-281 — 失败：释放 vmnt（根 vnode ref==0，天然回
            // 自由表，与 C 的 get_free_vnode 语义一致），errno 上抛。
            Err(e) => {
                self.vmnt_table.mark_free(slot);
                return Err(e);
            }
        };

        // mount.c:266 + 297-303 — fs_flags 与并发窗口（threaded → NR_WTHREADS）。
        // mount.c:268-270 的 update_statvfs（statvfs 缓存首填）归 stadir
        // 消费面（getvfsstat 接线波次，见 18-mount 实现表）。
        {
            let v = self.vmnt_table.get_mut(slot).ok_or(MountError::NoMem)?;
            v.fs_flags = info.fs_flags;
            let window = &mut self.comm.vmnts[slot.get()];
            window.max_reqs = info.max_reqs();
            window.cur_reqs = 0;
        }

        // mount.c:283-296 — 根 vnode 七连填 + 引用基线。
        {
            let vn = self.vnode_table.get_mut(root_vn).ok_or(MountError::NoMem)?;
            vn.fs = info.node.fs_e;
            vn.ino = info.node.ino;
            vn.mode = info.node.mode;
            vn.uid = info.node.uid;
            vn.gid = info.node.gid;
            vn.size = info.node.size;
            vn.sdev = NO_DEV;
            vn.fs_count = 1;
            vn.ref_count = 1;
            vn.vmnt = Some(crate::vnode::VmntId(slot.0));
            vn.dev = dev;
        }

        // mount.c:305 — VMNT_CANSTAT：此后可对外报告该文件系统。
        if let Some(v) = self.vmnt_table.get_mut(slot) {
            v.flags.insert(VmntFlags::CANSTAT);
        }

        // mount.c:318-325 — 挂载落位：根节点、无父挂载点、标签。
        {
            let v = self.vmnt_table.get_mut(slot).ok_or(MountError::NoMem)?;
            v.root = Some(root_vn.get());
            v.mounted_on = None;
            v.label = alloc::string::String::from("fs_imgrd");
        }

        // mount.c:331 — update_bspec(dev, fs_e, 0)：已有块特殊文件改路
        // （boot 期表空，扫描为空转；语义随挂载提供）。
        self.vnode_table.route_block_special(dev, fs_e);

        // mount.c:326-328 — ROOT_DEV/ROOT_FS_E（NS4 赋值面核心）。
        self.root_dev = dev;
        self.root_fs_e = fs_e;

        // mount.c:331-345 — MAKEROOT 全表：既有进程的 rd/wd 换根。boot 期
        // 指针为空（None），`put_vnode` 半按 C 同款跳过；ref 基线即上面的
        // `ref_count = 1`（C 首个 MAKEROOT 的 dup_vnode 同源）。
        for i in 0..NR_PROCS {
            if let Some(fp) = self.fproc_table.get_mut(UserSlot::new(i))
                && fp.pid != PID_FREE
            {
                fp.root_dir = Some(root_vn.get());
                fp.work_dir = Some(root_vn.get());
            }
        }

        // mount.c:349 — have_root++。
        self.have_root += 1;
        Ok(())
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
            Route::Bdev | Route::Cdev | Route::Sdev => {
                // C main.c:126-134 —— 块/字符/套接字驱动的回复各走
                // `bdev_reply`/`cdev_reply`/`sdev_reply`，三者的公共前半是
                // "找等这个驱动的 worker 槽 → 落槽 → 唤醒"。找不到就软失败
                // （C 里是 printf + return，主循环继续）。
                let _ = self.handle_drv_reply(msg);
            }
            Route::Notify { source, .. } => {
                // CLOCK 通知 = select 的超时闹钟到点（单闹钟近似，见
                // `select_timeout_check`）；其余通知（DS/KERNEL）暂不消费。
                if source == Endpoint::CLOCK {
                    self.select_timeout_check();
                }
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
                self.finish_worker_job_value(idx, fp_slot, fd.get() as i32);
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
            crate::open::OpenOutcome::Delegate(crate::open::DeviceClass::Char) => {
                // C `common_open` 的 `S_IFCHR` 支（open.c:162-168）→
                // `cdev_open(fd, vp->v_sdev, bits | (oflags & O_NOCTTY))`
                // → `cdev_opcl`（cdev.c:236-249）：`cdev_map` 的 `/dev/tty`
                // 重定向 → dmap 按 major 找驱动（没有 → ENXIO）→ `CTTY_MAJOR`
                // 例外（`/dev/tty` 直接成功，不打扰驱动）→ `O_NOCTTY` 三条规则
                // → 认领 fd/filp → `CDEV_OPEN` → worker 等待。
                let dev = node.dev;
                // `cdev_map`：`/dev/tty` 换成进程的控制终端（`tty_redirect`）。
                let is_ctty = ((dev & 0x000fff00) >> 8) as u32
                    == crate::device_map::CTTY_MAJOR as u32;
                let fp_tty = fp_slot
                    .and_then(|s| self.fproc_table.get(s))
                    .map(|fp| fp.tty)
                    .filter(|t| *t != minix_types::NO_DEV);
                let major_valid = (((dev & 0x000fff00) >> 8) as usize)
                    < crate::device_map::NR_DEVICES;
                let dev = match crate::cdev::tty_redirect(dev, is_ctty, fp_tty, major_valid) {
                    crate::cdev::RedirectVerdict::Keep(d)
                    | crate::cdev::RedirectVerdict::Substitute(d) => d,
                    crate::cdev::RedirectVerdict::NoDev => {
                        self.finish_worker_job(idx, fp_slot, minix_types::ENXIO);
                        return;
                    }
                };
                // CTTY 例外：`/dev/tty` 不真发请求（`cdev.c:174`）。**判定键是
                // 原始设备的 major**——C 的这一行看的是**未映射**的 `dev`
                // （`cdev_get` 只在内部换算 minor），所以"有控制终端的
                // `/dev/tty` open"在这里就返回 OK，绝不打扰真实 tty 的驱动
                // （C 注释：否则 setsid() 之后再开 `/dev/tty`，那个真实设备
                // 会被永久打开）。重定向后的设备号只用于算 minor/dmap 行。
                if is_ctty {
                    // 仍要认领 fd/filp（C 是在类型分派之前认领的）。
                    let Some(slot) = fp_slot else {
                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                        return;
                    };
                    let Some(vnode_idx) = self.intern_vnode(node) else {
                        self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                        return;
                    };
                    let fd = {
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
                            Ok((fd, filp_id)) => {
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
                                fd.get() as i32
                            }
                            Err(_) => {
                                self.finish_worker_job(idx, fp_slot, minix_types::EMFILE);
                                return;
                            }
                        }
                    };
                    self.finish_worker_job_value(idx, fp_slot, fd);
                    return;
                }
                let major = ((dev & 0x000fff00) >> 8) as u32;
                let minor = (((dev & 0xfff0_0000) >> 12) | (dev & 0xff)) as u32;
                let drv_e = match crate::device_map::get_by_major(&self.dmap_table, major)
                    .and_then(|row| row.driver)
                {
                    Some(e) => e,
                    None => {
                        self.finish_worker_job(idx, fp_slot, minix_types::ENXIO);
                        return;
                    }
                };
                // `O_NOCTTY` 三条规则（`noctty_force`）：非会话首进程、已有控制
                // 终端、或这个驱动见过 TTY 且别处已把它设成控制终端 → 强制加上。
                let (is_leader, has_tty) = fp_slot
                    .and_then(|s| self.fproc_table.get(s))
                    .map(|fp| {
                        (
                            fp.flags.contains(crate::fproc::FpFlags::SESLDR),
                            fp.tty != minix_types::NO_DEV,
                        )
                    })
                    .unwrap_or((false, false));
                let requested = oflags & crate::open::OpenFlags::NOCTTY.bits() != 0;
                let seen_elsewhere = self.dmap_table.get(major).map(|r| r.seen_tty).unwrap_or(false)
                    && (0..minix_types::NR_PROCS).any(|i| {
                        self.fproc_table
                            .get(minix_types::UserSlot::new(i))
                            .is_some_and(|fp| fp.pid != 0 && fp.tty == dev)
                    });
                let noctty = crate::cdev::noctty_force(is_leader, has_tty, requested, seen_elsewhere);
                let access = crate::cdev::access_bits(
                    bits.bits() & crate::open::R_BIT != 0,
                    bits.bits() & crate::open::W_BIT != 0,
                    noctty,
                );
                let Some(slot) = fp_slot else {
                    self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                    return;
                };
                let Some(vnode_idx) = self.intern_vnode(node) else {
                    self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                    return;
                };
                // 认领 fd/filp（C 在类型分派之前就认领；失败路径由续接体放开）。
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
                    // C `open.c:138-139`：`O_CLOEXEC` 位在**类型分派之前**
                    // 就记进 `fp_cloexec_set`，三种设备类型同样适用。
                    if oflags & crate::open::OpenFlags::CLOEXEC.bits() != 0 {
                        fp.cloexec_set.set(fd.get(), true);
                    }
                }
                let user_e = self
                    .fproc_table
                    .get(slot)
                    .map(|fp| fp.endpoint)
                    .unwrap_or(Endpoint::NONE);
                // `CDEV_OPEN`（形状由 `cdev::open_request` 钉住：`id`/`user`
                // 是调用者端点，`minor` 在第三格——位次发错，驱动开的就是
                // 次设备号 0）。
                let m = crate::cdev::open_request(minor, user_e, access);
                if let Some(wp) = self.worker_pool.get_mut(idx) {
                    wp.cont = Some(crate::worker::WorkerCont::CdevOpen {
                        fd: fd.get() as u32,
                        filp: filp_id.get(),
                        dev,
                    });
                }
                if let Err(e) = self.send_drv_for_slot(idx, fp_slot, drv_e, &m) {
                    // 发不出去：走 C 的失败尾（放开 fd/filp + 放回 vnode）再回错。
                    self.release_open_claim(fp_slot, fd.get() as u32, filp_id.get(), vnode_idx);
                    self.finish_worker_job(idx, fp_slot, e);
                }
                return;
            }
            crate::open::OpenOutcome::Delegate(crate::open::DeviceClass::Block) => {
                // C `common_open` 的 `S_IFBLK` 支（open.c:172-217）→
                // `bdev_open(dev, bits)`（bdev.c:79-112）：major 界内**且**
                // dmap 有驱动（缺一即 ENXIO）→ `BDEV_OPEN`（minor/access/id）
                // → **worker 等待**（C 里块驱动不许挂起，调用线程直接阻塞在
                // `drv_sendrec` 上；本模型的对应物是 worker 槽等待，续接体
                // `WorkerCont::BdevOpen` 收尾）。
                //
                // 驱动过了之后还有第二段：选 `v_bfs_e`（这个设备被哪个 FS
                // 管），没被别的挂载占着时补一条 `REQ_NEW_DRIVER`——都在
                // `bdev_open_bfs_stage` 里。
                let dev = node.dev;
                let major = ((dev & 0x000fff00) >> 8) as u32;
                let minor = (((dev & 0xfff0_0000) >> 12) | (dev & 0xff)) as u32;
                let major_valid = (major as usize) < crate::device_map::NR_DEVICES;
                let driver = self.dmap_table.get(major).and_then(|row| row.driver);
                // C `bdev_open:86-89` 的两道门（major 界内、dmap 行上有驱动）
                // 走决策层（它带着 ENXIO 的判据）；端点本身取 dmap 行的
                // `Endpoint`。
                if crate::bdev::resolve_driver(major_valid, driver.map(|e| e.get())).is_err() {
                    self.finish_worker_job(
                        idx,
                        fp_slot,
                        crate::bdev::BdevError::NoDev.to_errno(),
                    );
                    return;
                }
                let Some(drv_e) = driver else {
                    self.finish_worker_job(
                        idx,
                        fp_slot,
                        crate::bdev::BdevError::NoDev.to_errno(),
                    );
                    return;
                };
                let access = crate::bdev::access_bits(
                    bits.bits() & crate::open::R_BIT != 0,
                    bits.bits() & crate::open::W_BIT != 0,
                );
                let Some(slot) = fp_slot else {
                    self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                    return;
                };
                let Some(vnode_idx) = self.intern_vnode(node) else {
                    self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                    return;
                };
                // 认领 fd/filp（C 在类型分派之前认领；失败路径由续接体放开）。
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
                // `BDEV_OPEN`（形状由 `bdev::open_request` 钉住：块族首格是
                // `pos`，`minor` 在 8——按字符族的位次发，驱动收到的就是
                // "次设备号 0"）。
                let m = crate::bdev::open_request(minor, access);
                if let Some(wp) = self.worker_pool.get_mut(idx) {
                    wp.cont = Some(crate::worker::WorkerCont::BdevOpen {
                        fd: fd.get() as u32,
                        filp: filp_id.get(),
                        vnode: vnode_idx,
                        dev,
                        minor,
                        access,
                        retries: 0,
                    });
                }
                if let Err(e) = self.send_drv_for_slot(idx, fp_slot, drv_e, &m) {
                    // 发不出去：走 C 的失败尾（放开认领 + 放回 vnode）再回错。
                    // 传输层把内核状态折成一个 `EIO`——C 在
                    // `EDEADSRCDST`/`EDEADEPT` 时还会 `dmap_unmap_by_endpt`
                    // 解映射死驱动（`bdev.c:60-64`），那一步要传输层给出分类
                    // （`bdev::classify_send` 的输入），本批未接。
                    self.release_open_claim(fp_slot, fd.get() as u32, filp_id.get(), vnode_idx);
                    self.finish_worker_job(idx, fp_slot, e);
                }
            }
            // FIFO 配对（`S_IFIFO` 支：`map_vnode(PFS)` + `pipe_open`）与
            // 未识别类型：本批未接线，诚实拒绝。
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
        // C `advance`（path.c:98-106）：`v_sdev = res.dev`（**特殊设备号**，
        // 设备节点靠它认驱动）、`v_dev = vmp->m_dev`（**挂载分区的设备号**）。
        // 两者是不同字段：先前把 `res.dev` 写进 `v_dev` 是错的——设备节点的
        // `v_sdev` 一直是 0，`cdev_get`/`bdev_ioctl` 那些按 `v_sdev` 找驱动的
        // 地方就都找不到。
        let mount_dev = self
            .vmnt_table
            .find_by_fs(node.fs_e)
            .and_then(|id| self.vmnt_table.get(id))
            .map(|v| v.dev)
            .unwrap_or(minix_types::NO_DEV);
        if let Some(v) = self.vnode_table.get_mut(scratch) {
            v.fs = node.fs_e;
            v.ino = node.ino;
            v.mode = node.mode;
            v.size = node.size;
            v.uid = node.uid;
            v.gid = node.gid;
            v.dev = mount_dev;
            v.sdev = node.dev;
            v.fs_count = 1;
            v.ref_count = 1;
        }
        Some(scratch.0)
    }

    /// `common_open` 的**失败尾**（C open.c:275-285）：放开刚认领的 fd 与
    /// filp，再放回 vnode（`put_vnode`）。
    ///
    /// 设备类分支（字符/块）在驱动或 FS 拒绝之后都走这一处——不放开就等于
    /// 漏一个 fd 槽、一个 filp 引用与一个 vnode 引用，攒够了
    /// `vnode_table.alloc()` 就开始回 `ENFILE`。
    ///
    /// vnode 的放回走 `VnodeTable::put` 的**快速路径**（`ref>1 → ref--`）；
    /// 慢路径（`ref==1` → `req_putnode`）与 `change_into`/`close_filp` 是
    /// 同一处待办（`DeferredPutNode` 把那条 FS 通知留空）。
    /// `put_vnode` 的 VFS 半（C vnode.c:240-290）：快速路径（`ref>1`）
    /// 只减引用；慢路径（`ref==1`）把 `REQ_PUTNODE` 排进
    /// [`VfsState::pending_puts`]（`clean_refs` 的批量归还同路收集），
    /// 主循环统一投递。返回 `put` 的结果（`Ok(true)` = 槽已释放）。
    pub fn put_vnode_deferred(&mut self, id: crate::vnode::VnodeId) -> Result<bool, i32> {
        let mut q: alloc::vec::Vec<crate::vnode::PutNodeReq> = alloc::vec::Vec::new();
        let mut sink = PutNodeSink(&mut q);
        let r = self
            .vnode_table
            .put(id, &mut sink)
            .map_err(|_| minix_types::EINVAL);
        self.pending_puts.append(&mut q);
        r
    }

    pub fn release_open_claim(
        &mut self,
        fp_slot: Option<minix_types::UserSlot>,
        fd: u32,
        filp: usize,
        vnode: usize,
    ) {
        self.filp_table.dec_count(crate::filp::FilpId(filp));
        if let Some(slot) = fp_slot
            && let Some(fp) = self.fproc_table.get_mut(slot)
        {
            fp.filps[fd as usize] = None;
            fp.cloexec_set.set(fd as usize, false);
        }
        let _ = self.put_vnode_deferred(crate::vnode::VnodeId(vnode));
    }

    /// 块设备 open 成功后的**第二段**（C `open.c:186-216`）：定 `v_bfs_e`，
    /// 需要时补发 `REQ_NEW_DRIVER`。
    ///
    /// `v_bfs_e` 的选法照抄 C（[`crate::vmnt::VmntTable::bfs_for_device`]）：
    /// 默认根文件系统，被别的挂载占着就用那个挂载的 FS。**只有归根时**才
    /// 补发驱动标签——别的 FS 在挂载时已经从 readsuper 的标签参数认识这个
    /// 块驱动了。
    ///
    /// 三值返回：`Ok(true)` = 已挂上 `WorkerCont::BdevNewDriver`（在等 FS
    /// 回复，调用方**不要**收尾）；`Ok(false)` = 不需要通知，调用方直接回
    /// fd；`Err(errno)` = 这一段失败，调用方走失败尾再回错。
    fn bdev_open_bfs_stage(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        vnode: usize,
        fd: u32,
        filp: usize,
        dev: u64,
    ) -> Result<bool, i32> {
        let bfs_e = self.vmnt_table.bfs_for_device(dev, self.root_fs_e);
        if let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode)) {
            v.bfs = bfs_e;
        }
        if bfs_e != self.root_fs_e {
            // C 的 `if (vp->v_bfs_e != ROOT_FS_E) { unlock_bsf(); break; }`。
            return Ok(false);
        }
        // 根文件系统这一支要发 `req_newdriver`。根挂载还没执行过
        // （`root_fs_e` 还是 `NONE`）时没有可发的对象——按 C 的 newdriver
        // 失败路径收尾（`bdev_close` + `ENXIO`），不假装通知成功。
        if bfs_e == Endpoint::NONE {
            return Err(minix_types::ENXIO);
        }
        let major = ((dev & 0x000fff00) >> 8) as u32;
        let Some(label) = self.dmap_table.get(major).map(|row| row.label) else {
            return Err(minix_types::ENXIO);
        };
        let Some(vmnt) = self.vmnt_table.find_by_fs(bfs_e) else {
            return Err(minix_types::ENXIO);
        };
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        let (grant, label_len) = {
            let wp = self.worker_pool.get_mut(idx).ok_or(minix_types::EIO)?;
            // C 的标签是 dmap 行里的定长数组，要授权的是 `strlen(label)+1`
            // 字节（含结尾 NUL）；槽内的 `path_scratch` 是 VFS 自己的内存，
            // direct grant 必须指向一个**稳定地址**。
            let n = label.iter().position(|b| *b == 0).unwrap_or(label.len());
            let len = n + 1;
            if len > wp.path_scratch.len() {
                return Err(minix_types::EINVAL);
            }
            wp.path_scratch[..n].copy_from_slice(&label[..n]);
            wp.path_scratch[n] = 0;
            let addr = wp.path_scratch.as_ptr() as u64;
            // C `cpf_grant_direct(fs_e, (vir_bytes) label, len, CPF_READ)`。
            let grant = self
                .grants
                .grant_direct(
                    &minix_sys::syscall::DirectKernelCallTransport,
                    bfs_e.get(),
                    addr,
                    len as u64,
                    minix_types::CpFlags::READ,
                )
                .map_err(|_| minix_types::EIO)?;
            (grant, len)
        };
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::BdevNewDriver {
                fd,
                filp,
                vnode,
                dev,
            });
        }
        self.pending_fs = Some(PendingFs {
            vmnt: vmnt.0,
            fs_e: bfs_e,
            worker: idx,
            grant,
            user,
            req: crate::request::encode_new_driver(dev, grant, label_len),
        });
        Ok(true)
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

    /// `change_into`（C stadir.c:120-140）：把 `fp_wd`/`fp_rd` 换成新 vnode。
    /// 同一个 vnode 直接成功；不是目录 → ENOTDIR；不可搜索 → EACCES；都过了
    /// 才换（旧目录 `put_vnode`、新目录 `dup_vnode`）。
    ///
    /// 旧目录的释放走 `VnodeTable::put` 的**快速路径**（`ref>1 → ref--`）；
    /// 慢路径（`ref==1` → `req_putnode`）与 `filp.rs` 里 `close_filp` 的同一处
    /// 待办一样**还没接线**，`DeferredPutNode` 把那条 FS 通知留空——这是登记
    /// 在案的缺口，不是"已完成"。
    pub fn change_into(
        &mut self,
        fp_slot: Option<minix_types::UserSlot>,
        new_vnode: usize,
        into_root: bool,
    ) -> i32 {
        let Some(slot) = fp_slot else {
            return minix_types::EINVAL;
        };
        let old = match self.fproc_table.get(slot) {
            Some(fp) => {
                if into_root {
                    fp.root_dir
                } else {
                    fp.work_dir
                }
            }
            None => return minix_types::EINVAL,
        };
        if old == Some(new_vnode) {
            return 0; // C `if (*result == vp) return(OK);`
        }
        let (mode, uid, gid) = match self.vnode_table.get(crate::vnode::VnodeId(new_vnode)) {
            Some(v) => (v.mode, v.uid, v.gid),
            None => return minix_types::EINVAL,
        };
        // C `change_into`：目录类型门 → `forbidden(fp, vp, X_BIT)`。
        if mode & crate::open::S_IFMT != crate::open::S_IFDIR {
            return minix_types::ENOTDIR;
        }
        let (real_uid, real_gid, eff_uid, eff_gid, supp) =
            match self.fproc_table.get(slot) {
                Some(fp) => (
                    fp.real_uid,
                    fp.real_gid,
                    fp.eff_uid,
                    fp.eff_gid,
                    fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                ),
                None => return minix_types::EINVAL,
            };
        let readonly_fs = self
            .vmnt_table
            .find_by_fs(self.vnode_table.get(crate::vnode::VnodeId(new_vnode)).map(|v| v.fs).unwrap_or(Endpoint::NONE))
            .and_then(|v| self.vmnt_table.get(v))
            .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
            .unwrap_or(false);
        let forbid = crate::protect::forbidden_decision(&crate::protect::ForbidInput {
            real_uid,
            real_gid,
            eff_uid,
            eff_gid,
            is_access_call: false,
            file_uid: uid,
            file_gid: gid,
            mode,
            access: crate::open::X_BIT as u8,
            is_dir: true,
            supp: &supp,
            readonly_fs,
        });
        if let Err(e) = forbid {
            return e.to_errno();
        }
        // 换：旧目录 put（快速路径）、新目录已经 dup 过（`intern_vnode`）。
        if let Some(old_id) = old {
            let _ = self.put_vnode_deferred(crate::vnode::VnodeId(old_id));
        }
        if let Some(fp) = self.fproc_table.get_mut(slot) {
            if into_root {
                fp.root_dir = Some(new_vnode);
            } else {
                fp.work_dir = Some(new_vnode);
            }
        }
        0
    }

    /// 扫出 `sync`/`fsync` 要通知的挂载（C `do_sync`/`do_fsync` 的循环过滤）：
    /// `m_dev != NO_DEV && m_fs_e != NONE && m_root_node != NULL`；`dev` 给了
    /// 就再按设备号过滤（fsync 用文件的 `v_dev`）。
    pub fn sync_targets(&self, dev: Option<minix_types::DevId>) -> alloc::vec::Vec<Endpoint> {
        let mut out = alloc::vec::Vec::new();
        for idx in 0..crate::vmnt::NR_MNTS {
            let Some(v) = self.vmnt_table.get(crate::vmnt::VmntId(idx)) else {
                continue;
            };
            if v.dev == minix_types::NO_DEV || v.fs == Endpoint::NONE || v.root.is_none() {
                continue;
            }
            if let Some(want) = dev
                && v.dev != want
            {
                continue;
            }
            out.push(v.fs);
        }
        out
    }

    /// `fill_statvfs` 的**本地半**（C stadir.c:255-272 的尾部）：补上只读位、
    /// `f_fsid`/`f_fsidx` 与三个名字字段，然后整块拷给用户。
    ///
    /// `sys_datacopy` 是跨空间的，宿主构建下不可达 → 诚实回浮 EIO（不假装
    /// 拷成功）。
    fn finish_statvfs_copy(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        user_buf: u64,
        vmnt_idx: usize,
    ) -> i32 {
        let (dev, readonly, fstype, mount_path, mount_dev) = match self
            .vmnt_table
            .get(crate::vmnt::VmntId(vmnt_idx))
        {
            Some(v) => (
                v.dev,
                v.flags.contains(crate::vmnt::VmntFlags::READONLY),
                v.fstype.clone(),
                v.mount_path.clone(),
                v.mount_dev.clone(),
            ),
            None => return minix_types::EIO,
        };
        let b = &mut self.statvfs_buf;
        if readonly {
            let f = b.get_u64(minix_types::statvfs_off::FLAG);
            b.set_u64(minix_types::statvfs_off::FLAG, f | minix_types::ST_RDONLY);
        }
        // `f_fsid` 与 `f_fsidx.__fsid_val[0]` 都是设备号（C 的注释说这是
        // NetBSD 的做法）。
        b.set_u64(minix_types::statvfs_off::FSID, dev);
        b.set_u32(minix_types::statvfs_off::FSIDX, dev as u32);
        b.set_u32(minix_types::statvfs_off::FSIDX + 4, 0);
        b.set_name(minix_types::statvfs_off::FSTYPENAME, &fstype);
        b.set_name(minix_types::statvfs_off::MNTONNAME, &mount_path);
        b.set_name(minix_types::statvfs_off::MNTFROMNAME, &mount_dev);
        // 拷给用户：`sys_datacopy_wrapper(SELF, &buf, endpt, buf_addr, sizeof)`。
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        let src = self.statvfs_buf.as_bytes().as_ptr() as u64;
        let _ = idx;
        match minix_sys::syscall::sys_datacopy(
            &minix_sys::syscall::DirectKernelCallTransport,
            minix_types::Endpoint::SELF.0,
            src,
            user.0,
            user_buf,
            minix_types::STATVFS_SIZE as u64,
        ) {
            Ok(()) => 0,
            Err(_) => minix_types::EIO,
        }
    }

    /// 把挂载行的缓存抄进 VFS 侧缓冲（C `fill_statvfs` 的 `ST_NOWAIT` 分支：
    /// `memset(&buf, 0, ...)` 之后逐字段抄 `m_stats`）。
    fn statvfs_fill_from_cache(&mut self, vmnt_idx: usize) {
        use minix_types::statvfs_off as off;
        self.statvfs_buf = minix_types::StatvfsBuf::new();
        let stats = self
            .vmnt_table
            .get(crate::vmnt::VmntId(vmnt_idx))
            .map(|v| v.stats)
            .unwrap_or_default();
        let b = &mut self.statvfs_buf;
        b.set_u64(off::FLAG, stats.f_flag);
        b.set_u64(off::BSIZE, stats.f_bsize);
        b.set_u64(off::FRSIZE, stats.f_frsize);
        b.set_u64(off::IOSIZE, stats.f_iosize);
        b.set_u64(off::BLOCKS, stats.f_blocks);
        b.set_u64(off::BFREE, stats.f_bfree);
        b.set_u64(off::BAVAIL, stats.f_bavail);
        b.set_u64(off::BRESVD, stats.f_bresvd);
        b.set_u64(off::FILES, stats.f_files);
        b.set_u64(off::FFREE, stats.f_ffree);
        b.set_u64(off::FFAVAIL, stats.f_favail);
        b.set_u64(off::FRESVD, stats.f_fresvd);
        b.set_u64(off::SYNCREADS, stats.f_syncreads);
        b.set_u64(off::SYNCWRITES, stats.f_syncwrites);
        b.set_u64(off::ASYNCREADS, stats.f_asyncreads);
        b.set_u64(off::ASYNCWRITES, stats.f_asyncwrites);
        b.set_u64(off::NAMEMAX, stats.f_namemax);
    }

    /// 给某个挂载发一条 `REQ_STATVFS`（direct grant 指向 VFS 侧缓冲），并登记
    /// 续接（含 `getvfsstat` 的序列状态）。C `req_statvfs`（request.c:232-247）。
    #[allow(clippy::too_many_arguments)]
    fn send_statvfs_request(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        vmnt_idx: usize,
        user_buf: u64,
        seq: ([usize; crate::vmnt::NR_MNTS], u8, u8),
    ) -> Result<(), i32> {
        let fs_e = self
            .vmnt_table
            .get(crate::vmnt::VmntId(vmnt_idx))
            .map(|v| v.fs)
            .ok_or(minix_types::EIO)?;
        let addr = self.statvfs_buf.as_bytes().as_ptr() as u64;
        let grant = self
            .grants
            .grant_direct(
                &minix_sys::syscall::DirectKernelCallTransport,
                fs_e.get(),
                addr,
                minix_types::STATVFS_SIZE as u64,
                minix_types::CpFlags::WRITE,
            )
            .map_err(|_| minix_types::EIO)?;
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::Statvfs {
                grant,
                user_buf,
                vmnt: vmnt_idx,
                seq: seq.0,
                seq_count: seq.1,
                seq_at: seq.2,
            });
        }
        self.pending_fs = Some(PendingFs {
            vmnt: vmnt_idx,
            fs_e,
            worker: idx,
            grant,
            user,
            req: crate::request::encode_statvfs(grant),
        });
        Ok(())
    }

    /// `fill_statvfs` 的**入口**（C stadir.c:230-253）：`ST_NOWAIT` 就用挂载行
    /// 缓存（不打扰 FS），否则给 VFS 侧缓冲做 direct grant 并发 `REQ_STATVFS`，
    /// 统计量由续接体收（`WorkerCont::Statvfs`）。
    pub fn begin_statvfs(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        vmnt_idx: usize,
        user_buf: u64,
        flags: i32,
    ) -> Result<(), i32> {
        if flags & minix_types::ST_NOWAIT != 0 {
            self.statvfs_fill_from_cache(vmnt_idx);
            let status = self.finish_statvfs_copy(idx, fp_slot, user_buf, vmnt_idx);
            self.finish_worker_job(idx, fp_slot, status);
            return Ok(());
        }
        self.send_statvfs_request(
            idx,
            fp_slot,
            vmnt_idx,
            user_buf,
            ([0usize; crate::vmnt::NR_MNTS], 0, 0),
        )
    }

    /// `make_sock_fd` 的前半（C socket.c:86-176）：锁 PFS → 预留 vnode →
    /// 认领 fd/filp → `req_newnode(PFS, effuid, effgid, S_IFSOCK|ACCESSPERMS,
    /// dev)`。后半（填 vnode/filp）在 `WorkerCont::SockFd` 的续接体里。
    pub fn begin_make_sock_fd(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        dev: u64,
        flags: u32,
        addr_len_out: Option<u32>,
        pair: Option<crate::worker::PairState>,
    ) -> Result<(), i32> {
        let Some(slot) = fp_slot else {
            return Err(minix_types::EINVAL);
        };
        // C `find_vmnt(PFS_PROC_NR)`：拿不到就 panic（"PFS gone"）；Rust 侧
        // fail-closed（PFS 是另一条线的服务器）。
        let Some(vmnt_id) = self.vmnt_table.find_by_fs(minix_types::Endpoint::PFS) else {
            return Err(minix_types::EIO);
        };
        let vnode = self
            .vnode_table
            .alloc()
            .map_err(|_| minix_types::ENFILE)?;
        // C `get_fd(fp, 0, R_BIT | W_BIT, &fd, &filp)`。
        let (fd, filp) = {
            use crate::filedes::FdAllocPolicy;
            let fp = self.fproc_table.get_mut(slot).ok_or(minix_types::EINVAL)?;
            let idx_fd = crate::filedes::LowestFree
                .allocate(&fp.filps, 0)
                .ok_or(minix_types::EMFILE)?;
            let fd = crate::filedes::Fd::new(idx_fd).ok_or(minix_types::EMFILE)?;
            let filp = self
                .filp_table
                .alloc_filp(crate::open::R_BIT | crate::open::W_BIT)
                .map_err(|_| minix_types::ENFILE)?;
            self.filp_table.inc_count(filp);
            let fp = self.fproc_table.get_mut(slot).ok_or(minix_types::EINVAL)?;
            fp.filps[idx_fd] = Some(filp.get());
            (fd, filp)
        };
        let (uid, gid) = self
            .fproc_table
            .get(slot)
            .map(|fp| (fp.eff_uid, fp.eff_gid))
            .unwrap_or((0, 0));
        let user = self
            .fproc_table
            .get(slot)
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::SockFd {
                filp: filp.get(),
                fd: fd.get() as u32,
                flags,
                vnode: vnode.get(),
                dev,
                addr_len_out,
                pair,
            });
        }
        self.pending_fs = Some(PendingFs {
            vmnt: vmnt_id.0,
            fs_e: minix_types::Endpoint::PFS,
            worker: idx,
            grant: 0,
            user,
            // C socket.c:134-136：`S_IFSOCK | ACCESSPERMS` 作为节点模式。
            req: crate::request::encode_newnode(
                dev,
                crate::open::S_IFSOCK | 0o777,
                uid,
                gid,
            ),
        });
        Ok(())
    }

    /// 套接字驱动的 getset 族（C `sdev_setsockopt` sdev.c:450-495 /
    /// `sdev_get` sdev.c:500-555）：把用户缓冲做成 magic grant 交给驱动
    /// （`set` 方向 `CPF_READ`、`get` 方向 `CPF_WRITE`），回复号必须是
    /// `SDEV_REPLY`，**状态在载荷里**；`get` 方向的状态就是新长度。
    #[allow(clippy::too_many_arguments)]
    pub fn send_sdev_getset(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        dev: u64,
        req_type: i32,
        level: i32,
        name: i32,
        buf: u64,
        len: u32,
        write_dir: bool,
    ) -> Result<(), i32> {
        let drv_e = crate::device_map::smap_endpt_by_dev(&self.smap_table, dev)
            .ok_or(minix_types::EIO)?;
        let (_, sock_id) = crate::device_map::split_smap_dev(dev).ok_or(minix_types::EIO)?;
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        // C 的 `cpf_grant_magic(drv, who_e, addr, len, CPF_READ|CPF_WRITE)`。
        let access = if write_dir {
            minix_types::CpFlags::WRITE
        } else {
            minix_types::CpFlags::READ
        };
        let grant = self
            .grant_user_buffer(drv_e, user, buf, len as u64, access)
            .map_err(|_| minix_types::EIO)?;
        let mut req = minix_types::Message {
            m_type: req_type,
            ..minix_types::Message::default()
        };
        // SAFETY: `mess_vfs_lsockdriver_getset { int32_t req_id; int32_t
        // sock_id; int level; int name; cp_grant_id_t grant; unsigned int len; }`
        // （ipc.h:2272-2281）。
        unsafe {
            let raw = &mut req.m_u.raw;
            raw[0..4].copy_from_slice(&user.0.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..12].copy_from_slice(&level.to_le_bytes());
            raw[12..16].copy_from_slice(&name.to_le_bytes());
            raw[16..20].copy_from_slice(&grant.to_le_bytes());
            raw[20..24].copy_from_slice(&len.to_le_bytes());
        }
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::SdevGetSet { grant, write_dir });
        }
        match self.send_drv_for_slot(idx, fp_slot, drv_e, &req) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = self.revoke_grant(grant);
                Err(e)
            }
        }
    }

    /// 套接字驱动的"简单请求"（C `sdev_simple` sdev.c:245-276）：`listen`/
    /// `shutdown`/`close` 共用——`{req_id, sock_id, param}` 发给驱动，回复号
    /// 必须是 `SDEV_REPLY`，**状态在回复载荷里**。
    ///
    /// `dev` 是套接字设备号（`v_sdev`），靠 `smap_endpt_by_dev` 认驱动与
    /// `sock_id`（C `get_smap_by_dev`）。
    pub fn send_sdev_simple(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        dev: u64,
        req_type: i32,
        param: i32,
    ) -> Result<(), i32> {
        let drv_e = crate::device_map::smap_endpt_by_dev(&self.smap_table, dev)
            .ok_or(minix_types::EIO)?;
        let (_, sock_id) = crate::device_map::split_smap_dev(dev).ok_or(minix_types::EIO)?;
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        let mut req = minix_types::Message {
            m_type: req_type,
            ..minix_types::Message::default()
        };
        // SAFETY: `mess_vfs_lsockdriver_simple { int32_t req_id; sock_id_t
        // sock_id; int param; }`（ipc.h:2318-2326）。
        unsafe {
            let raw = &mut req.m_u.raw;
            raw[0..4].copy_from_slice(&user.0.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..12].copy_from_slice(&param.to_le_bytes());
        }
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::SdevSimple);
        }
        self.send_drv_for_slot(idx, fp_slot, drv_e, &req)
    }

    /// `get_sock`（C socket.c:276-302）：fd → filp → **必须是套接字**
    /// （否则 `ENOTSOCK`），返回它的设备号与打开标志。套接字族共用的第一道门
    /// ——全本地判定，没有驱动对话。
    pub fn get_sock(
        &self,
        fp_slot: minix_types::UserSlot,
        fd: i32,
    ) -> Result<(u64, i32), i32> {
        if fd < 0 {
            return Err(minix_types::EBADF);
        }
        let fp = self.fproc_table.get(fp_slot).ok_or(minix_types::EINVAL)?;
        let filp_idx = fp
            .filps
            .get(fd as usize)
            .copied()
            .flatten()
            .ok_or(minix_types::EBADF)?;
        let filp = self
            .filp_table
            .get(crate::filp::FilpId(filp_idx))
            .ok_or(minix_types::EBADF)?;
        let vnode_idx = filp.vnode.ok_or(minix_types::EBADF)?;
        let v = self
            .vnode_table
            .get(crate::vnode::VnodeId(vnode_idx))
            .ok_or(minix_types::EBADF)?;
        // C `!S_ISSOCK(filp->filp_vno->v_mode)` → ENOTSOCK。
        if v.mode & crate::open::S_IFMT != crate::open::S_IFSOCK {
            return Err(minix_types::ENOTSOCK);
        }
        Ok((v.sdev, filp.flags))
    }

    /// `do_mapdriver` 的**主体**（C dmap.c:106-177 的取标签之后那半）：
    /// 标签 → 端点（`resolve_driver`）→ 标成服务进程（`FP_SRV_PROC`）→
    /// `map_driver`（major 有效时）→ `smap_map`（有域时；失败要**撤销** dmap）。
    ///
    /// `caller` 是消息来源（C 的 `who_e`）：只有 RS 能映射驱动
    /// （`check_mapper`）。
    #[allow(clippy::too_many_arguments)]
    /// `ds_retrieve_label_endpt` 那一跳的**填充半**（C `do_mapdriver`
    /// dmap.c:148-152 的前置）：拿标签去 DS 查端点，查到就**upsert** 进
    /// `driver_labels`（同名更新、新名追加），返回端点；DS 不可达或标签
    /// 未登记返回 `None`——调用方回退到本地表（空表语义仍是 EINVAL，
    /// fail-closed 链不变）。
    pub fn ds_fill_label(&mut self, label: &str) -> Option<Endpoint> {
        let mut ds = minix_sys::ds::DsClient::new(
            minix_sys::ipc::DirectTrapTransport,
            minix_sys::syscall::DirectKernelCallTransport,
            Endpoint::DS,
        );
        let (endpoint, _flags) = ds.retrieve_label_endpt(label).ok()?;
        // 幂等 upsert：同名更新（驱动重启换端点），新名追加。
        match self
            .driver_labels
            .iter_mut()
            .find(|(name, _)| name == label)
        {
            Some(entry) => entry.1 = endpoint,
            None => self
                .driver_labels
                .push((alloc::string::String::from(label), endpoint)),
        }
        Some(endpoint)
    }

    pub fn finish_mapdriver(
        &mut self,
        caller: Endpoint,
        label: &str,
        major: u32,
        domains: &[i32],
    ) -> i32 {
        if crate::device_map::check_mapper(caller).is_err() {
            return minix_types::EPERM;
        }
        // C dmap.c:148-152 —— 标签 → 端点：先走 DS 那一跳（查到即填充
        // 本地表），DS 没有再回退本地表（空表语义仍是 EINVAL）。
        if self.ds_fill_label(label).is_none()
            && !self.driver_labels.iter().any(|(name, _)| name == label)
        {
            let dir = LabelDir(&self.driver_labels);
            if let Err(e) = crate::device_map::resolve_driver(&dir, label) {
                return e.to_errno();
            }
        }
        let dir = LabelDir(&self.driver_labels);
        let endpoint = match crate::device_map::resolve_driver(&dir, label) {
            Ok(e) => e,
            Err(e) => return e.to_errno(),
        };
        // C dmap.c:154-158 —— 端点必须是已知进程（`isokendpt`），并标成服务。
        let Some(slot) = endpoint.to_user_slot() else {
            return minix_types::EINVAL;
        };
        if self.fproc_table.get(slot).is_none() {
            return minix_types::EINVAL;
        }
        if let Some(fp) = self.fproc_table.get_mut(slot) {
            fp.flags |= crate::fproc::FpFlags::SRV_PROC;
        }
        // C dmap.c:161-165 —— major 有效就写 dmap 行。
        if major != minix_types::NO_DEV as u32 {
            if let Err(e) = crate::device_map::map_driver(
                &mut self.dmap_table,
                Some(label.as_bytes()),
                major,
                Some(endpoint),
            ) {
                return e.to_errno();
            }
        }
        // C dmap.c:166-173 —— 有域就写 smap；失败要把刚才的 dmap 撤销。
        if !domains.is_empty() {
            let ndomains = domains.len();
            if ndomains > 8 {
                // `NR_DOMAIN`（config.h:61 = 8）。
                if major != minix_types::NO_DEV as u32 {
                    let _ = crate::device_map::map_driver(
                        &mut self.dmap_table,
                        None,
                        major,
                        None,
                    );
                }
                return minix_types::EINVAL;
            }
            // 逐域检查（C smap.c:74-84）：越界/UNSPEC → EINVAL、被别人占了 → EBUSY。
            let existing = crate::device_map::find_slot_by_label(&self.smap_table, label.as_bytes());
            let free = crate::device_map::find_free_slot(&self.smap_table);
            let checks: alloc::vec::Vec<crate::device_map::DomainCheck> = domains
                .iter()
                .map(|d| crate::device_map::check_domain(&self.smap_table, *d, existing))
                .collect();
            let old_endpt = existing
                .and_then(|s| self.smap_table.entries[s as usize].endpt);
            let plan = match crate::device_map::register_plan(
                existing,
                free,
                &checks,
                old_endpt,
                endpoint,
            ) {
                Ok(p) => p,
                Err(e) => {
                    if major != minix_types::NO_DEV as u32 {
                        let _ = crate::device_map::map_driver(
                            &mut self.dmap_table,
                            None,
                            major,
                            None,
                        );
                    }
                    return e.to_errno();
                }
            };
            // 应用：写行（标签 + 端点）与 `pfmap[domain] = 行`。
            let slot = plan.slot as usize;
            {
                let row = &mut self.smap_table.entries[slot];
                row.endpt = Some(endpoint);
                let n = label.len().min(crate::device_map::LABEL_MAX - 1);
                row.label = [0u8; crate::device_map::LABEL_MAX];
                row.label[..n].copy_from_slice(&label.as_bytes()[..n]);
            }
            // 替换时先解掉旧实例的域映射（C smap.c:105-118 的 unmap 段）。
            for d in 0..crate::device_map::PF_MAX {
                if self.smap_table.pfmap[d] == Some(plan.slot) {
                    self.smap_table.pfmap[d] = None;
                }
            }
            for d in domains {
                if (*d as usize) < crate::device_map::PF_MAX {
                    self.smap_table.pfmap[*d as usize] = Some(plan.slot);
                }
            }
        }
        0
    }

    /// `getvfsstat` 的**入口**（C `do_getvfsstat` stadir.c:330-403）：`buf == 0`
    /// 时只数个数（不打扰 FS）；否则按 `bufsize / sizeof(struct statvfs)` 截断
    /// 挂载列表，逐个 `fill_statvfs` 到 `buf + i*sizeof`，返回**个数**。
    pub fn begin_getvfsstat(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        buf_addr: u64,
        bufsize: u64,
        flags: i32,
    ) -> Result<(), i32> {
        // C 的过滤：`m_dev != NO_DEV && (m_flags & VMNT_CANSTAT)`。
        let mut targets = alloc::vec::Vec::new();
        for i in 0..crate::vmnt::NR_MNTS {
            let Some(v) = self.vmnt_table.get(crate::vmnt::VmntId(i)) else {
                continue;
            };
            if v.dev == minix_types::NO_DEV {
                continue;
            }
            if !v.flags.contains(crate::vmnt::VmntFlags::CANSTAT) {
                continue;
            }
            targets.push(i);
        }
        if buf_addr == 0 {
            // 只报个数：C 直接 `return count`。
            self.finish_worker_job_value(idx, fp_slot, targets.len() as i32);
            return Ok(());
        }
        // 空间不足的截断（C 的 `if (bufsize < sizeof) break;`）。
        let fits = (bufsize / minix_types::STATVFS_SIZE as u64) as usize;
        targets.truncate(fits);
        if targets.is_empty() {
            self.finish_worker_job(idx, fp_slot, 0);
            return Ok(());
        }
        let mut arr = [0usize; crate::vmnt::NR_MNTS];
        for (i, t) in targets.iter().enumerate().take(crate::vmnt::NR_MNTS) {
            arr[i] = *t;
        }
        let count = targets.len().min(crate::vmnt::NR_MNTS) as u8;
        if flags & minix_types::ST_NOWAIT != 0 {
            // 缓存分支：逐个就地填 + 拷，不发请求。
            for (at, vmnt_idx) in targets.iter().enumerate() {
                self.statvfs_fill_from_cache(*vmnt_idx);
                let dst = buf_addr + (at as u64) * minix_types::STATVFS_SIZE as u64;
                let status = self.finish_statvfs_copy(idx, fp_slot, dst, *vmnt_idx);
                if status != 0 {
                    self.finish_worker_job(idx, fp_slot, status);
                    return Ok(());
                }
            }
            self.finish_worker_job_value(idx, fp_slot, count as i32);
            return Ok(());
        }
        self.send_statvfs_request(idx, fp_slot, targets[0], buf_addr, (arr, count, 0))
    }

    /// `rename` 的阶段 1 → 阶段 2 转场：切出 name2 的最后组件，从进程根起走
    /// 它的父目录（C `do_rename:218-231` 的第二趟 `last_dir`）。
    fn rename_stage_two(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        old_fs_e: Endpoint,
        old_ino: u64,
        old_name: alloc::string::String,
        new_path: &str,
    ) {
        let new_entry = match crate::path::last_dir_split(new_path) {
            Ok(sp) => sp.entry,
            Err(e) => {
                self.finish_worker_job(idx, fp_slot, e.to_errno());
                return;
            }
        };
        if let Err(e) = self.start_parent_walk(
            idx,
            fp_slot,
            new_path,
            crate::worker::PathFollow::RenameNew {
                old_fs_e,
                old_ino,
                old_name,
                new_entry,
            },
        ) {
            self.finish_worker_job(idx, fp_slot, e);
        }
    }

    /// 发一条 `REQ_RENAME`（C `req_rename` — request.c:927-955）：两个名字各占
    /// 槽内 scratch 的一半（`PATH_MAX` 1024 → 每半 512 ≥ `NAME_MAX+1`），各做
    /// 一张 **direct grant**（两张都指向 VFS 自己的内存）；回复只有状态。
    #[allow(clippy::too_many_arguments)]
    pub fn send_rename_for_slot(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        fs_e: Endpoint,
        dir_old: u64,
        dir_new: u64,
        old_name: &str,
        new_entry: &str,
    ) -> Result<(), i32> {
        let vmnt = self.vmnt_table.find_by_fs(fs_e).ok_or(minix_types::EIO)?.0;
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        let half = crate::path::PATH_MAX / 2;
        let (grant_old, len_old, grant_new, len_new) = {
            let wp = self.worker_pool.get_mut(idx).ok_or(minix_types::EIO)?;
            let ob = old_name.as_bytes();
            let on = ob.len().min(half - 1);
            wp.path_scratch[..on].copy_from_slice(&ob[..on]);
            wp.path_scratch[on] = 0;
            let nb = new_entry.as_bytes();
            let nn = nb.len().min(half - 1);
            wp.path_scratch[half..half + nn].copy_from_slice(&nb[..nn]);
            wp.path_scratch[half + nn] = 0;
            let addr_old = wp.path_scratch.as_ptr() as u64;
            let addr_new = wp.path_scratch[half..].as_ptr() as u64;
            let g_old = self
                .grants
                .grant_direct(
                    &minix_sys::syscall::DirectKernelCallTransport,
                    fs_e.get(),
                    addr_old,
                    (on + 1) as u64,
                    minix_types::CpFlags::READ,
                )
                .map_err(|_| minix_types::EIO)?;
            let g_new = match self.grants.grant_direct(
                &minix_sys::syscall::DirectKernelCallTransport,
                fs_e.get(),
                addr_new,
                (nn + 1) as u64,
                minix_types::CpFlags::READ,
            ) {
                Ok(g) => g,
                Err(_) => {
                    let _ = self.revoke_grant(g_old);
                    return Err(minix_types::EIO);
                }
            };
            (g_old, on + 1, g_new, nn + 1)
        };
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::Status);
        }
        self.pending_fs = Some(PendingFs {
            vmnt,
            fs_e,
            worker: idx,
            grant: grant_old,
            user,
            req: crate::request::encode_rename(
                dir_old, dir_new, len_old, len_new, grant_old, grant_new,
            ),
        });
        Ok(())
    }

    /// 从进程根/工作目录起走一条路径的**父目录**（`last_dir` 的目录前缀），
    /// 现场换成 `follow` 并发出首条 lookup。C 里 rename/link 这些"换条路径
    /// 再来一趟"的地方都是这个形状（新 `resolve` + 同一套 `last_dir`）。
    pub fn start_parent_walk(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        path: &str,
        follow: crate::worker::PathFollow,
    ) -> Result<(), i32> {
        let rd = self.root_dir_of(fp_slot);
        let split = crate::path::last_dir_split(path).map_err(|e| e.to_errno())?;
        let resolve =
            crate::path::Lookup::new(split.dir_path.clone(), crate::path::LookupFlags::NOFLAGS)
                .map_err(|e| e.to_errno())?;
        let start = if resolve.path.starts_with('/') {
            crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
        } else {
            let wd = match fp_slot {
                Some(slot) => self.work_dir_of(slot),
                None => rd,
            };
            crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
        };
        let (uid, gid) = match fp_slot.and_then(|s| self.fproc_table.get(s)) {
            Some(fp) => (fp.eff_uid, fp.eff_gid),
            None => return Err(minix_types::EINVAL),
        };
        let (walk, step) = crate::path::LookupWalk::begin(start, resolve, rd, uid, gid)
            .map_err(|e| e.to_errno())?;
        let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
            return Err(minix_types::EIO);
        };
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::Path);
            wp.path = Some(crate::worker::PathPending { walk, grant: 0, follow });
        }
        self.send_lookup_for_slot(idx, fp_slot, fs_e, dir_ino, root_ino)
    }

    /// 给驱动发一条请求并让槽等它（C `sdev_sendrec`/`cdev_opcl`/`bdev_sendrec`
    /// 的公共前半）：`asynsend3(drv_e, m, AMF_NOREPLY)` 是**发完不等**，线程随后
    /// 在 `worker_wait` 里等回复——本模型里就是"槽的 `task` 指向驱动、
    /// `sendrec` 放着请求、状态 `WaitingForFs`"，回复到达由
    /// [`Self::handle_drv_reply`] 落槽并唤醒。
    ///
    /// 发送失败（宿主构建下 trap 不可达）返回 EIO，调用方按错误收尾——不假装
    /// 发出去了。
    pub fn send_drv_for_slot(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        drv_e: Endpoint,
        req: &Message,
    ) -> Result<(), i32> {
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            // C `self->w_task = sp->smap_endpt; self->w_drv_sendrec = m_ptr`。
            wp.task = Some(drv_e);
            wp.sendrec = Some(*req);
            wp.state = crate::worker::WorkerState::WaitingForFs;
        }
        let _ = fp_slot;
        // C `drv_sendrec` 失败后的分类（bdev.c:60-68）：死端点
        // （`EDEADSRCDST`/`EDEADEPT`）先 `dmap_unmap_by_endpt` 解映射，
        // 一切类别统一折 EIO。内核状态在 TrapStatus 里（正号域），宿主
        // 传输恒 EIO(5) → `Fatal` 类 → 不解映射（不误伤活表）。
        match minix_sys::ipc::DirectTrapTransport.send(drv_e, req) {
            Ok(()) => Ok(()),
            Err(st) => {
                if crate::bdev::classify_send(st.0) == crate::bdev::SendFault::Dead {
                    crate::device_map::unmap_by_endpt(&mut self.dmap_table, drv_e);
                }
                Err(minix_types::EIO)
            }
        }
    }

    /// 套接字读写的驱动请求（C `sdev_readwrite` sdev.c:336-410）：三张可选的
    /// magic grant（数据/控制/地址；发送方向 `CPF_READ`、接收方向 `CPF_WRITE`），
    /// 发出去**不等**（这类调用可能等很久），进程级挂起。
    #[allow(clippy::too_many_arguments)]
    pub fn send_sdev_readwrite(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        dev: u64,
        data: Option<(u64, u64)>,
        ctl: Option<(u64, u64)>,
        addr: Option<(u64, u64)>,
        flags: i32,
        writing: bool,
        call: crate::fproc::SdevCall,
    ) -> Result<(), i32> {
        self.send_sdev_readwrite_aux(
            idx,
            fp_slot,
            dev,
            data,
            ctl,
            addr,
            flags,
            writing,
            call,
            crate::fproc::SdevAux::None,
        )
    }

    /// 同上，但允许指定现场里的 `aux`（`recvmsg` 要把 `msg_buf` 带下去——
    /// 收尾要回写 msghdr 的几个字段，C `sdev_readwrite` 的最后一参）。
    #[allow(clippy::too_many_arguments)]
    pub fn send_sdev_readwrite_aux(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        dev: u64,
        data: Option<(u64, u64)>,
        ctl: Option<(u64, u64)>,
        addr: Option<(u64, u64)>,
        flags: i32,
        writing: bool,
        call: crate::fproc::SdevCall,
        aux: crate::fproc::SdevAux,
    ) -> Result<(), i32> {
        let drv_e = crate::device_map::smap_endpt_by_dev(&self.smap_table, dev)
            .ok_or(minix_types::EIO)?;
        let (_, sock_id) = crate::device_map::split_smap_dev(dev).ok_or(minix_types::EIO)?;
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        let access = if writing {
            minix_types::CpFlags::READ
        } else {
            minix_types::CpFlags::WRITE
        };
        let mut grants: [Option<i32>; 3] = [None, None, None];
        let mut make = |spec: Option<(u64, u64)>, slot: usize| -> Result<(i32, u64), i32> {
            match spec {
                None => Ok((minix_types::GRANT_INVALID, 0)),
                Some((buf, len)) => {
                    // C 只对**非零**缓冲建 grant（`if (data_buf != 0)`）。
                    if buf == 0 {
                        return Ok((minix_types::GRANT_INVALID, 0));
                    }
                    let g = self.grant_user_buffer(drv_e, user, buf, len, access)?;
                    grants[slot] = Some(g);
                    Ok((g, len))
                }
            }
        };
        let (data_grant, data_len) = make(data, 0)?;
        let (ctl_grant, ctl_len) = match make(ctl, 1) {
            Ok(p) => p,
            Err(e) => {
                for g in grants.iter().flatten() {
                    let _ = self.revoke_grant(*g);
                }
                return Err(e);
            }
        };
        let (addr_grant, addr_len) = match make(addr, 2) {
            Ok(p) => p,
            Err(e) => {
                for g in grants.iter().flatten() {
                    let _ = self.revoke_grant(*g);
                }
                return Err(e);
            }
        };
        let mut req = minix_types::Message {
            m_type: if writing {
                minix_sockdriver::sdev::SdevRequest::Send as i32
            } else {
                minix_sockdriver::sdev::SdevRequest::Receive as i32
            },
            ..minix_types::Message::default()
        };
        // SAFETY: `mess_vfs_lsockdriver_sendrecv { int32_t req_id; int32_t
        // sock_id; cp_grant_id_t data_grant; size_t data_len; cp_grant_id_t
        // ctl_grant; unsigned int ctl_len; cp_grant_id_t addr_grant; unsigned
        // int addr_len; endpoint_t user_endpt; int flags; }`（ipc.h:2304-2317）。
        unsafe {
            let raw = &mut req.m_u.raw;
            raw[0..4].copy_from_slice(&user.0.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..12].copy_from_slice(&data_grant.to_le_bytes());
            raw[16..24].copy_from_slice(&data_len.to_le_bytes());
            raw[24..28].copy_from_slice(&ctl_grant.to_le_bytes());
            raw[28..32].copy_from_slice(&(ctl_len as u32).to_le_bytes());
            raw[32..36].copy_from_slice(&addr_grant.to_le_bytes());
            raw[36..40].copy_from_slice(&(addr_len as u32).to_le_bytes());
            raw[40..44].copy_from_slice(&user.0.to_le_bytes());
            raw[44..48].copy_from_slice(&flags.to_le_bytes());
        }
        if minix_sys::ipc::IpcTransport::send(&minix_sys::ipc::DirectTrapTransport, drv_e, &req)
            .is_err()
        {
            for g in grants.iter().flatten() {
                let _ = self.revoke_grant(*g);
            }
            return Err(minix_types::EIO);
        }
        let block = crate::fproc::SdevBlock {
            dev,
            call,
            // `GrantId` 是 `i32` 的别名（minix-types `types::id`），直接放。
            grants: [grants[0], grants[1], grants[2]],
            aux,
        };
        self.suspend_on_sdev(fp_slot, idx, block)
    }

    /// **进程级挂起**的套接字调用（C `sdev_suspend` sdev.c:82-112）：把调用进程
    /// 标成 `FP_BLOCKED_ON_SDEV` 并**释放 worker 槽**（这类调用可能等很久——
    /// 比如 connect 等对端——C 不肯拿 worker 槽去等），回复到达后由
    /// [`Self::finish_sdev_blocked`] 收尾。
    pub fn suspend_on_sdev(
        &mut self,
        fp_slot: Option<minix_types::UserSlot>,
        idx: usize,
        block: crate::fproc::SdevBlock,
    ) -> Result<(), i32> {
        let Some(slot) = fp_slot else {
            return Err(minix_types::EINVAL);
        };
        let fp = self.fproc_table.get_mut(slot).ok_or(minix_types::EINVAL)?;
        fp.blocked_on = crate::fproc::BlockedOn::Sdev(block);
        // 释放槽（C 的 `suspend()` 之后 worker 就空了）；`run_once` 见
        // `Suspend` 不再重复释放，也不回用户——回复由驱动那侧来。
        self.worker_pool.release(idx);
        if self.current_worker == Some(idx) {
            self.current_worker = None;
        }
        Ok(())
    }

    /// 驱动回复唤醒**进程级挂起**的套接字调用（C `sdev_reply` 的第二条路 +
    /// `sdev_finish` 的 bind/connect 组）：按**设备号的驱动**找到那个被挂起的
    /// 进程，清掉挂起态、撤销它留下的 grant，再把状态回给用户。
    ///
    /// 返回 `true` 表示认领了这条回复。
    pub fn finish_sdev_blocked(&mut self, msg: &Message) -> bool {
        // C `sdev_reply`：回复里的 `req_id` 是调用方端点——本模型里直接按
        // "谁被挂起 + 设备号的驱动是谁"找（两者等价且更严）。
        let mut found: Option<(minix_types::UserSlot, crate::fproc::SdevBlock)> = None;

        for i in 0..minix_types::NR_PROCS {
            let slot = minix_types::UserSlot::new(i);
            let Some(fp) = self.fproc_table.get(slot) else {
                continue;
            };
            let crate::fproc::BlockedOn::Sdev(block) = fp.blocked_on else {
                continue;
            };
            // 这个设备的驱动就是回复方吗？
            if crate::device_map::smap_endpt_by_dev(&self.smap_table, block.dev)
                == Some(msg.m_source)
            {
                found = Some((slot, block));
                break;
            }
        }
        let Some((slot, block)) = found else {
            return false;
        };
        // 清挂起态（C 的 `rfp->fp_blocked_on = FP_BLOCKED_ON_NONE`）。
        if let Some(fp) = self.fproc_table.get_mut(slot) {
            fp.blocked_on = crate::fproc::BlockedOn::None;
        }
        // 撤销留下的 grant（C 的 `sdev_finish` 之前先 revoke 那三张）。
        for g in block.grants.iter().flatten() {
            let _ = self.revoke_grant(*g);
        }
        let target = self
            .fproc_table
            .get(slot)
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        // 状态与收尾按**调用**分流（C `sdev_finish` 的 switch）：
        // - 发送方向（`VFS_SENDTO`）用 `SDEV_REPLY`，状态就是结果；
        // - 接收方向（`VFS_RECVFROM`）用 `SDEV_RECV_REPLY`，载荷里还有
        //   `ctl_len`/`addr_len`/`flags`——`recvfrom` 要把 **addr_len 放进
        //   回复载荷**（`m_vfs_lc_socklen { len }`，C `resume_recvfrom`）。
        // `accept` 有**自己的三态收尾**（C `resume_accept` socket.c:367-465）：
        // ① 失败且没建套接字 → 只回错误；② 失败但驱动已建套接字 → 关掉它再回
        // 错误；③ 成功 → 现场**再开一个 worker** 去做 `make_sock_fd`（C 注释：
        // 收尾里还要阻塞调用，主线程不能做），成功回 fd + 对端地址长度。
        if msg.m_type == minix_sockdriver::sdev::SdevReply::AcceptReply as i32 {
            // SAFETY: `mess_lsockdriver_vfs_accept_reply { int32_t req_id@0;
            // int32_t sock_id@4; int status@8; unsigned int len@12 }`。
            let raw = unsafe { &msg.m_u.raw };
            let sock_id = i32::from_le_bytes(raw[4..8].try_into().unwrap());
            let status = i32::from_le_bytes(raw[8..12].try_into().unwrap());
            let addr_len = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            // `split_smap_dev` 给 `(num, sockid)`——这里要的是**行号**。
            let smap_num = match crate::device_map::split_smap_dev(block.dev) {
                Some((num, _)) => num,
                None => {
                    self.queue_reply(
                        target,
                        crate::call_table::SyscallResult::Error(minix_types::EIO),
                    );
                    return true;
                }
            };
            if sock_id < 0 {
                // case ①：没建套接字，只回错误。
                self.queue_reply(
                    target,
                    crate::call_table::SyscallResult::Error(if status != 0 {
                        status
                    } else {
                        minix_types::EIO
                    }),
                );
                return true;
            }
            let dev = crate::device_map::make_smap_dev(smap_num, sock_id as u32);
            // case ②/③ 都要一个 worker（收尾里可能要再发驱动请求 / PFS 请求）。
            let Some(new_idx) = self.worker_pool.assign_first_fit(
                slot,
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            ) else {
                // 槽位耗尽：C 在这里会尽力而为；本模型里诚实回 EAGAIN 并记
                // 缺口（新套接字由驱动侧留着，等驱动死亡回收）。
                self.queue_reply(
                    target,
                    crate::call_table::SyscallResult::Error(minix_types::EAGAIN),
                );
                return true;
            };
            self.current_worker = Some(new_idx);
            // 监听套接字还在吗？它的打开标志要被新套接字继承（C `resume_accept`
            // 的 `get_sock(listen_fd, &ldev, &flags)`）。
            let listen_fd = match block.aux {
                crate::fproc::SdevAux::Fd(fd) => fd as i32,
                _ => -1,
            };
            let (_, listen_flags) = match self.get_sock(slot, listen_fd) {
                Ok(pair) => pair,
                Err(_) => {
                    self.queue_reply(
                        target,
                        crate::call_table::SyscallResult::Error(minix_types::EIO),
                    );
                    self.finish_worker_job(new_idx, Some(slot), minix_types::EIO);
                    return true;
                }
            };
            if status != 0 {
                // case ②：驱动建了套接字但整体失败 → 关掉它，回错误。
                if let Some(wp) = self.worker_pool.get_mut(new_idx) {
                    wp.cont = Some(crate::worker::WorkerCont::SdevCloseThenReply { status });
                }
                if self
                    .send_sdev_simple(
                        new_idx,
                        Some(slot),
                        dev,
                        minix_sockdriver::sdev::SdevRequest::Close as i32,
                        0,
                    )
                    .is_err()
                {
                    // 关不掉也要把错误回给用户（C 是 `(void)sdev_close`）。
                    self.finish_worker_job(new_idx, Some(slot), status);
                }
                return true;
            }
            // case ③：继承监听套接字的三个标志位（C 的 `flags &=
            // O_CLOEXEC | O_NONBLOCK | O_NOSIGPIPE`）。
            let inherit = (crate::open::OpenFlags::CLOEXEC.bits()
                | crate::fcntl::O_NONBLOCK
                | crate::fcntl::O_NOSIGPIPE)
                & (listen_flags as u32);
            if let Err(e) =
                self.begin_make_sock_fd(new_idx, Some(slot), dev, inherit, Some(addr_len), None)
            {
                // 建 fd 失败 → 也要关掉新套接字（C 的同一个分支）。
                if let Some(wp) = self.worker_pool.get_mut(new_idx) {
                    wp.cont = Some(crate::worker::WorkerCont::SdevCloseThenReply { status: e });
                }
                if self
                    .send_sdev_simple(
                        new_idx,
                        Some(slot),
                        dev,
                        minix_sockdriver::sdev::SdevRequest::Close as i32,
                        0,
                    )
                    .is_err()
                {
                    self.finish_worker_job(new_idx, Some(slot), e);
                }
            }
            return true;
        }

        let recv_reply = minix_sockdriver::sdev::SdevReply::ReceiveReply as i32;
        let mut status;
        let mut addr_len_out: Option<u32> = None;
        if msg.m_type == minix_sockdriver::sdev::SdevReply::Reply as i32 {
            // SAFETY: `mess_lsockdriver_vfs_reply { int32_t req_id; int status; }`。
            let raw = unsafe { &msg.m_u.raw };
            status = i32::from_le_bytes(raw[4..8].try_into().unwrap());
        } else if msg.m_type == recv_reply {
            // SAFETY: `mess_lsockdriver_vfs_recv_reply { req_id@0, status@4,
            // ctl_len@8, addr_len@12, flags@16 }`。
            let raw = unsafe { &msg.m_u.raw };
            status = i32::from_le_bytes(raw[4..8].try_into().unwrap());
            let ctl_len = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let addr_len = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            let rflags = i32::from_le_bytes(raw[16..20].try_into().unwrap());
            match block.call {
                crate::fproc::SdevCall::Recvfrom if status >= 0 => {
                    addr_len_out = Some(addr_len);
                }
                crate::fproc::SdevCall::Recvmsg if status >= 0 => {
                    // C `resume_recvmsg`（socket.c:600-660）：改 msghdr 的三个
                    // 字段（`msg_controllen`/`msg_flags`/`msg_namelen`，后者只在
                    // `addr_len > 0` 时改）再**整块拷回用户**；拷贝失败就把那个
                    // 错误当状态回（C 的 `status = r`）。宿主下这次拷贝不可达，
                    // 所以这里如实回 EIO/EINVAL。
                    let update =
                        crate::socket::recvmsg_update(ctl_len, rflags as u32, addr_len);
                    let _ = update;
                    if let crate::fproc::SdevAux::Buf(msgbuf) = block.aux {
                        // 取回用户那份 msghdr（生产件是跨空间拷贝）。
                        use crate::socket::MsgHdrFetcher as _;
                        let fetcher = crate::socket::SysMsgHdrFetcher { who: target };
                        if let Ok(mh) = fetcher.fetch_msghdr(msgbuf.0) {
                            let mut raw_out = [0u8; crate::socket::MSGHDR_SIZE];
                            {
                                let mh = crate::socket::MsgHdr {
                                    controllen: update.ctl_len,
                                    flags: update.flags as i32,
                                    namelen: update.addr_len.unwrap_or(mh.namelen),
                                    ..mh
                                };
                                raw_out = crate::socket::encode_msghdr(&mh);
                            }
                            if minix_sys::syscall::sys_datacopy(
                                &minix_sys::syscall::DirectKernelCallTransport,
                                minix_types::Endpoint::SELF.0,
                                raw_out.as_ptr() as u64,
                                target.0,
                                msgbuf.0,
                                crate::socket::MSGHDR_SIZE as u64,
                            )
                            .is_err()
                            {
                                // 负号：这个函数的 `status` 按"线上带符号
                                // 状态"解释，正号会被当成成功值/fd。
                                status = -minix_types::EIO;
                            }
                        } else {
                            status = -minix_types::EIO;
                        }
                    }
                }
                _ => {}
            }
        } else if msg.m_type < 0 {
            status = msg.m_type;
        } else {
            // 回复号不是认识的任何一种：折 EIO——**负号**（这个函数的
            // `status` 按"线上带符号状态"解释，正号会被当成成功值/fd）。
            status = -minix_types::EIO;
        }
        let result = if status < 0 {
            crate::call_table::SyscallResult::Error(status)
        } else if let Some(len) = addr_len_out {
            let mut m = Message {
                m_type: status,
                ..Message::default()
            };
            // SAFETY: `mess_vfs_lc_socklen { unsigned int len; }` 在负载区首字。
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&len.to_le_bytes());
            }
            self.queue_reply_msg(target, m);
            return true;
        } else {
            crate::call_table::SyscallResult::Ok(status)
        };
        self.queue_reply(target, result);
        true
    }

    // ───────────────────────── select 的等待半 ─────────────────────────
    // C select.c 的回复/超时/重启机械（do_select 的请求半在 syscalls.rs）。
    // 查询是 `asynsend` 一发不等（`cdev_select`/`sdev_select`），所以这半
    // 不经 worker 槽：回复按 dmap/smap 的 `sel_busy` 认领，超时按 CLOCK
    // 通知收尾，进程级挂起（`BlockedOn::Select`）与 bind/accept 同模式。

    /// 发一张 select 查询（C `cdev_select` cdev.c:350-377 /
    /// `sdev_select` sdev.c:647-668）：`asynsend` 一发不等。
    ///
    /// 消息形状：字符 `mess_vfs_lchardriver_select { devminor_t minor@0;
    /// int ops@4 }`（ipc.h，`minor` 是 4 字节域）；套接字
    /// `mess_vfs_lsockdriver_select { int32 sock_id@0; int ops@4 }`。
    pub fn send_select_query(
        &mut self,
        is_char: bool,
        dev: u64,
        rops: crate::select::SelOps,
    ) -> Result<(), i32> {
        let mut m = Message {
            m_type: if is_char {
                minix_chardriver::protocol::CdevRequest::Select as i32
            } else {
                minix_sockdriver::sdev::SdevRequest::Select as i32
            },
            ..Message::default()
        };
        // SAFETY: 两族 select 请求的前两格都是 `int32 id/minor@0; int
        // ops@4`（ipc.h:2216-2222 / lsockdriver_select）。
        unsafe {
            let raw = &mut m.m_u.raw;
            let id = if is_char {
                (((dev & 0xfff0_0000) >> 12) | (dev & 0xff)) as u32
            } else {
                match crate::device_map::split_smap_dev(dev) {
                    Some((_, sock_id)) => sock_id as u32,
                    None => return Err(minix_types::EIO),
                }
            };
            raw[0..4].copy_from_slice(&id.to_le_bytes());
            raw[4..8].copy_from_slice(&(rops.to_status()).to_le_bytes());
        }
        let drv_e = if is_char {
            let major = ((dev & 0x000fff00) >> 8) as u32;
            crate::device_map::get_by_major(&self.dmap_table, major)
                .map(|row| row.driver)
                .unwrap_or(None)
                .ok_or(minix_types::ENXIO)?
        } else {
            crate::device_map::smap_endpt_by_dev(&self.smap_table, dev)
                .ok_or(minix_types::EIO)?
        };
        // C `asynsend3(dmap_driver, &mess, AMF_NOREPLY)`（失败 panic——
        // 模型里传输失败折 EIO，调用方记进 `se->error`）。
        use minix_sys::ipc::IpcTransport as _;
        minix_sys::ipc::DirectTrapTransport
            .sendnb(drv_e, &m)
            .map_err(|_| minix_types::EIO)
    }

    /// `select_request_char`/`select_request_sock` 的共用体
    /// （select.c:459-565）：`/dev/tty` 重映射 → `filp_select_dev` 冲突门 →
    /// `select_filter`（决策件）→ 驱动 busy 门 → 发查询 → 记账。
    ///
    /// 返回 `Ok(ready)` = 本轮就绪位（空 = 在途或暂无）；`Err(e)` =
    /// C 的 `r != OK && r != SUSPEND`（记进 `se->error` 的那类）。
    pub(crate) fn select_request_driver(
        &mut self,
        fp_slot: Option<minix_types::UserSlot>,
        filp_idx: usize,
        is_char: bool,
        want: crate::select::SelOps,
        block: bool,
    ) -> Result<crate::select::SelOps, i32> {
        use crate::select::{filter_step, FilterOutcome, SelOps as SO};
        let sdev = {
            let filp = self
                .filp_table
                .get(crate::filp::FilpId(filp_idx))
                .ok_or(minix_types::EIO)?;
            let vnode_idx = filp.vnode.ok_or(minix_types::EIO)?;
            let v = self
                .vnode_table
                .get(crate::vnode::VnodeId(vnode_idx))
                .ok_or(minix_types::EIO)?;
            v.sdev
        };
        // C `cdev_map`（select_request_char:470-473）：字符族做 `/dev/tty`
        // 重映射；套接字族原样（select.c:539）。
        let dev = if is_char {
            let major = ((sdev & 0x000fff00) >> 8) as u32;
            let is_ctty = major == crate::device_map::CTTY_MAJOR;
            let fp_tty = fp_slot
                .and_then(|sl| self.fproc_table.get(sl))
                .map(|fp| fp.tty)
                .filter(|t| *t != minix_types::NO_DEV);
            let major_valid = (major as usize) < crate::device_map::NR_DEVICES;
            match crate::cdev::tty_redirect(sdev, is_ctty, fp_tty, major_valid) {
                crate::cdev::RedirectVerdict::Keep(d)
                | crate::cdev::RedirectVerdict::Substitute(d) => d,
                crate::cdev::RedirectVerdict::NoDev => return Err(minix_types::ENXIO),
            }
        } else {
            sdev
        };
        let (old_dev, flags, held_ops) = {
            let filp = self
                .filp_table
                .get(crate::filp::FilpId(filp_idx))
                .ok_or(minix_types::EIO)?;
            (
                filp.select_dev,
                crate::filp::FsfFlags::from_bits_truncate(filp.select_flags as u32),
                crate::select::SelOps::from_bits_truncate(filp.select_ops),
            )
        };
        // C select.c:475-487：一张 filp 挂了两个控制终端的错乱门。
        if old_dev != 0 && old_dev != dev {
            return Err(minix_types::EIO);
        }
        {
            let filp = self
                .filp_table
                .get_mut(crate::filp::FilpId(filp_idx))
                .ok_or(minix_types::EIO)?;
            filp.select_dev = dev; // set before possibly suspending
        }
        match filter_step(flags, want, block) {
            FilterOutcome::ReadyNone | FilterOutcome::Suspend => {
                // 空手而回（0 就绪位）——filter 判"现在没得问"或在途。
                Ok(SO::empty())
            }
            FilterOutcome::Query { rops, clear_update, set_busy, set_block, .. } => {
                // 驱动 busy 门（select.c:508-510/547-549）：同一驱动同一
                // 时刻只许一张查询在途。
                let busy = if is_char {
                    let major = ((dev & 0x000fff00) >> 8) as u32;
                    self.dmap_table.get(major).map(|r| r.sel_busy).unwrap_or(true)
                } else {
                    let (num, _) = crate::device_map::split_smap_dev(dev)
                        .ok_or(minix_types::EIO)?;
                    self.smap_table
                        .entries
                        .iter()
                        .find(|r| r.num == num)
                        .map(|r| r.sel_busy)
                        .unwrap_or(true)
                };
                if busy {
                    return Ok(SO::empty());
                }
                if clear_update {
                    let filp = self
                        .filp_table
                        .get_mut(crate::filp::FilpId(filp_idx))
                        .ok_or(minix_types::EIO)?;
                    filp.select_flags &= !(crate::filp::FsfFlags::UPDATE.bits() as u8);
                }
                match self.send_select_query(is_char, dev, rops) {
                    Ok(()) => {
                        // 成功：标驱动在途 + filp 的 BUSY/阻塞监视位
                        // （C select.c:519-522/555-558 的三连义务）。
                        let block_bits = if set_block.contains(SO::RD) {
                            crate::filp::FsfFlags::RD_BLOCK.bits()
                        } else {
                            0
                        } | if set_block.contains(SO::WR) {
                            crate::filp::FsfFlags::WR_BLOCK.bits()
                        } else {
                            0
                        } | if set_block.contains(SO::ERR) {
                            crate::filp::FsfFlags::ERR_BLOCK.bits()
                        } else {
                            0
                        };
                        let filp = self
                            .filp_table
                            .get_mut(crate::filp::FilpId(filp_idx))
                            .ok_or(minix_types::EIO)?;
                        filp.select_flags |=
                            (block_bits | if set_busy { crate::filp::FsfFlags::BUSY.bits() } else { 0 })
                                as u8;
                        if is_char {
                            let major = ((dev & 0x000fff00) >> 8) as u32;
                            if let Some(row) = self.dmap_table.get_mut(major) {
                                row.sel_busy = true;
                                row.sel_owner = Some(filp_idx);
                            }
                        } else {
                            let (num, _) = crate::device_map::split_smap_dev(dev)
                                .ok_or(minix_types::EIO)?;
                            if let Some(row) =
                                self.smap_table.entries.iter_mut().find(|r| r.num == num)
                            {
                                row.sel_busy = true;
                                row.sel_owner = Some(filp_idx);
                            }
                        }
                        let _ = held_ops;
                        Ok(SO::empty())
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// 槽的"有 fd 在等驱动答复吗"（`is_deferred` 的第二参：任何参与 filp
    /// 带 `FSF_UPDATE | FSF_BUSY`）。
    pub(crate) fn select_any_update_or_busy(&self, s: usize) -> bool {
        let Some(se) = self.select_table.get(s) else { return false };
        se.filps.iter().flatten().any(|e| {
            self.filp_table
                .get(crate::filp::FilpId(e.filp))
                .map(|f| {
                    crate::filp::FsfFlags::from_bits_truncate(f.select_flags as u32)
                        .intersects(crate::filp::FsfFlags::UPDATE | crate::filp::FsfFlags::BUSY)
                })
                .unwrap_or(false)
        })
    }

    /// `restart_proc`（select.c:1303-1312）：有结果且不再 deferred 就收尾。
    fn select_restart_proc(&mut self, s: usize) {
        let Some(se) = self.select_table.get(s) else { return };
        let (nready, error, block) = (se.nready, se.error, se.block);
        let deferred = self.select_any_update_or_busy(s);
        if crate::select::should_return(nready, error != 0, block, deferred) {
            self.select_return(s);
        }
    }

    /// `select_cancel_all`（select.c:712-738）：逐 filp 释放选择账
    /// （`cancel_one` 决策件），最后一任清 stale 的 dmap/smap 归属
    /// （busy **保持**——查询还在途，回复落地时只清状态），清超时，放槽，
    /// 清进程挂起态。
    fn select_cancel_all(&mut self, s: usize) {
        let filps: alloc::vec::Vec<usize> = self
            .select_table
            .get(s)
            .map(|se| se.filps.iter().flatten().map(|e| e.filp).collect())
            .unwrap_or_default();
        for filp_idx in filps {
            let stale = {
                let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(filp_idx)) else {
                    continue;
                };
                let mut sel = crate::select::FilpSel {
                    selectors: f.selectors as u32,
                    ops: crate::select::SelOps::from_bits_truncate(f.select_ops),
                    flags: crate::filp::FsfFlags::from_bits_truncate(f.select_flags as u32),
                    pipe_ops: crate::select::SelOps::from_bits_truncate(f.pipe_select_ops),
                    dev: if f.select_dev != 0 { Some(f.select_dev) } else { None },
                };
                let out = crate::select::cancel_one(&mut sel);
                f.selectors = sel.selectors as u8;
                f.select_ops = sel.ops.bits();
                f.select_flags = sel.flags.bits() as u8;
                f.pipe_select_ops = sel.pipe_ops.bits();
                f.select_dev = sel.dev.unwrap_or(0);
                out
            };
            if let Some(dev) = stale {
                let major = ((dev & 0x000fff00) >> 8) as u32;
                if let Some(row) = self.dmap_table.get_mut(major)
                    && row.sel_owner == Some(filp_idx)
                {
                    row.sel_owner = None; // leave _busy set（C select.c:763）
                }
                if let Some((num, _)) = crate::device_map::split_smap_dev(dev)
                    && let Some(row) =
                        self.smap_table.entries.iter_mut().find(|r| r.num == num)
                    && row.sel_owner == Some(filp_idx)
                {
                    row.sel_owner = None;
                }
            }
        }
        let requestor = self.select_table.get(s).and_then(|se| se.requestor);
        if let Some(se) = self.select_table.get_mut(s) {
            se.expiry = 0;
            se.filps.clear();
        }
        self.select_table.release(s);
        // 清进程挂起态（C 由 revive 的唤醒机制承担）。
        if let Some(slot) = requestor
            && let Some(fp) = self.fproc_table.get_mut(slot)
            && fp.blocked_on == crate::fproc::BlockedOn::Select
        {
            fp.blocked_on = crate::fproc::BlockedOn::None;
        }
    }

    /// `select_return`（select.c:1090-1107）：取消 → 结果集拷回 →
    /// `revive(req_endpt, r)`（模型里 = queue_reply + 清挂起态，见
    /// [`Self::select_cancel_all`] 尾部）。
    fn select_return(&mut self, s: usize) {
        // CLOCK/回复路径没有可注入的 io——构造生产件（宿主下 store 不可
        // 达时按错误收尾，簿记仍完整）。
        let who = self
            .select_table
            .get(s)
            .and_then(|se| se.requestor)
            .and_then(|sl| self.fproc_table.get(sl))
            .map(|fp| fp.endpoint);
        if let Some(who) = who {
            use crate::call_table::SyscallResult;
            let io = crate::select::SysFdSetIo { who };
            let r = self.select_finish(s, &io);
            self.queue_reply(
                who,
                match r {
                    Ok(v) => SyscallResult::Ok(v),
                    Err(e) => SyscallResult::Error(e),
                },
            );
        }
    }

    /// 超时布防 + 进程级挂起（C `do_select:319-335` 的 `set_timer` +
    /// `suspend(FP_BLOCKED_ON_SELECT)`）。`Until` 计划经 `sys_setalarm`
    /// 设**一个**内核闹钟（多 select 并存时重设为最早到期——全表近似
    /// 见 [`Self::select_timeout_check`]）。
    pub(crate) fn select_arm_and_suspend(
        &mut self,
        slot_idx: usize,
        fp_slot: minix_types::UserSlot,
        plan: crate::select::TimeoutPlan,
    ) -> Result<(), i32> {
        if let crate::select::TimeoutPlan::Until { ticks } = plan {
            if let Some(se) = self.select_table.get_mut(slot_idx) {
                se.expiry = ticks;
            }
            // 单闹钟：有并存 select 时重设为最早到期。
            let next = (0..crate::select::MAXSELECTS)
                .filter_map(|s| self.select_table.get(s))
                .filter(|se| se.requestor.is_some() && se.expiry > 0)
                .map(|se| se.expiry)
                .min()
                .unwrap_or(ticks);
            let _ = minix_sys::syscall::sys_setalarm(
                &minix_sys::syscall::DirectKernelCallTransport,
                next,
                false,
            );
        }
        let fp = self.fproc_table.get_mut(fp_slot).ok_or(minix_types::EINVAL)?;
        fp.blocked_on = crate::fproc::BlockedOn::Select;
        // 释放 worker 槽（进程级挂起；与 `suspend_on_sdev` 同模式——
        // C 的 `suspend()` 之后 worker 作业即告终）。
        if let Some(idx) = self.current_worker {
            self.worker_pool.release(idx);
            self.current_worker = None;
        }
        Ok(())
    }

    /// `select_return` 的可注入 io 版（do_select 的立即返回路径用它，
    /// 拿臂的 FdSetIo 缝让宿主可测）。返回用户拿到的那只值。
    pub(crate) fn select_finish(
        &mut self,
        s: usize,
        io: &impl crate::select::FdSetIo,
    ) -> Result<i32, i32> {
        // 释放前取走收尾要的数据（cancel 会放掉槽）。
        let (error, nready, vir, sets) = {
            let Some(se) = self.select_table.get(s) else { return Ok(0) };
            (
                se.error,
                se.nready,
                (se.vir_readfds, se.vir_writefds, se.vir_errorfds),
                (
                    se.ready_readfds.clone(),
                    se.ready_writefds.clone(),
                    se.ready_errorfds.clone(),
                ),
            )
        };
        let (vir_read, vir_write, vir_err) = vir;
        let (rd, wr, er) = sets;
        self.select_cancel_all(s);
        // C：error 时**不拷**结果集，直接回错误（select.c:1101-1105）。
        // C：error 时**不拷**结果集，直接回错误（select.c:1101-1105）。
        if error != 0 {
            return Err(error);
        }
        // C `copy_fdsets(se, se->nfds, TO_PROC)`：只拷回用户预期的
        // 字节数——三张集就是按这个字节数分配的，整集拷回。
        if vir_read != 0
            && let Err(e) = io.store(vir_read, &rd)
        {
            return Err(e);
        }
        if vir_write != 0
            && let Err(e) = io.store(vir_write, &wr)
        {
            return Err(e);
        }
        if vir_err != 0
            && let Err(e) = io.store(vir_err, &er)
        {
            return Err(e);
        }
        Ok(nready as i32)
    }

    /// `filp_status`（select.c:1283-1301）：把一个 filp 的新状态广播给
    /// 所有选它的槽。`status < 0` 记错；否则记就绪位。
    fn select_filp_status(&mut self, filp_idx: usize, status: i32) {
        let mut found = alloc::vec::Vec::new();
        for s in 0..crate::select::MAXSELECTS {
            let Some(se) = self.select_table.get(s) else { continue };
            if se.requestor.is_none() {
                continue;
            }
            for fd in 0..se.filps.len() {
                if se.filps[fd].map(|e| e.filp) != Some(filp_idx) {
                    continue;
                }
                if status < 0 {
                    let se = self.select_table.get_mut(s).unwrap();
                    se.error = status;
                } else {
                    let ops = crate::select::SelOps::from_status(status);
                    let se = self.select_table.get_mut(s).unwrap();
                    let nfds = se.nfds;
                    if fd < nfds {
                        let want = (
                            se.vir_readfds != 0
                                && crate::select::bit_of(&se.readfds, fd),
                            se.vir_writefds != 0
                                && crate::select::bit_of(&se.writefds, fd),
                            se.vir_errorfds != 0
                                && crate::select::bit_of(&se.errorfds, fd),
                        );
                        let (a, b, c, mut n) = (&mut se.ready_readfds, &mut se.ready_writefds, &mut se.ready_errorfds, se.nready);
                        crate::select::ops2tab_store(ops, fd, want, (a, b, c), &mut n);
                        se.nready = n;
                    }
                }
                found.push(s);
                break;
            }
        }
        for s in found {
            self.select_restart_proc(s);
        }
    }

    /// `select_cdev_reply1`（select.c:1004-1070）：字符驱动的一型回复。
    /// 设备不匹配时**保持**在途标记（C 同款：等真正的回复）。
    pub fn select_cdev_reply1(&mut self, driver_e: Endpoint, minor: u32, status: i32) {
        let Some(major) = crate::device_map::get_by_endpt(&self.dmap_table, driver_e)
        else {
            return;
        };
        let dev = (((major as u64) << 8) & 0x000fff00) | ((minor as u64) & 0xff);
        let Some(row) = self.dmap_table.get(major) else { return };
        if !row.sel_busy {
            return; // 没人等这张回复
        }
        let owner = row.sel_owner;
        if let Some(filp_idx) = owner {
            let ok = self
                .filp_table
                .get(crate::filp::FilpId(filp_idx))
                .map(|f| f.select_dev == dev)
                .unwrap_or(false);
            if !ok {
                return; // 驱动答非所问：保持在途
            }
        }
        if let Some(row) = self.dmap_table.get_mut(major) {
            row.sel_busy = false;
            row.sel_owner = None;
        }
        if let Some(filp_idx) = owner {
            self.select_reply1(filp_idx, status);
        }
        self.select_restart_filps();
    }

    /// `select_sdev_reply1`（select.c:1072-1107）：套接字驱动的一型回复。
    pub fn select_sdev_reply1(&mut self, dev: u64, status: i32) {
        let (num, _) = match crate::device_map::split_smap_dev(dev) {
            Some(p) => p,
            None => return,
        };
        let Some(row) = self.smap_table.entries.iter().find(|r| r.num == num) else {
            return;
        };
        if !row.sel_busy {
            return;
        }
        let owner = row.sel_owner;
        if let Some(filp_idx) = owner {
            let ok = self
                .filp_table
                .get(crate::filp::FilpId(filp_idx))
                .map(|f| f.select_dev == dev)
                .unwrap_or(false);
            if !ok {
                return;
            }
        }
        let _ = status;
        if let Some(row) = self.smap_table.entries.iter_mut().find(|r| r.num == num) {
            row.sel_busy = false;
            row.sel_owner = None;
        }
        if let Some(filp_idx) = owner {
            self.select_reply1(filp_idx, status);
        }
        self.select_restart_filps();
    }

    /// `select_reply1`（select.c:956-999）：一型回复的 filp 记账 +
    /// 广播（`reply1_step` 决策件）。
    fn select_reply1(&mut self, filp_idx: usize, status: i32) {
        let (flags, ops) = {
            let Some(f) = self.filp_table.get(crate::filp::FilpId(filp_idx)) else {
                return;
            };
            (
                crate::filp::FsfFlags::from_bits_truncate(f.select_flags as u32),
                crate::select::SelOps::from_bits_truncate(f.select_ops),
            )
        };
        let out = crate::select::reply1_step(flags, ops, status);
        if let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(filp_idx)) {
            f.select_ops = out.ops.bits();
            f.select_flags = out.flags.bits() as u8;
        }
        let broadcast = if status < 0 { status } else { out.broadcast.to_status() };
        self.select_filp_status(filp_idx, broadcast);
    }

    /// `select_cdev_reply2`（select.c:1160-1192）+ `select_reply2`
    /// （select.c:1109-1158）：二型（就绪通知）回复，扫全部槽里盯着这个
    /// 设备的 fd。
    pub fn select_cdev_reply2(&mut self, driver_e: Endpoint, minor: u32, status: i32) {
        if status == 0 {
            return; // C：weird status
        }
        let Some(major) = crate::device_map::get_by_endpt(&self.dmap_table, driver_e)
        else {
            return;
        };
        let dev = (((major as u64) << 8) & 0x000fff00) | ((minor as u64) & 0xff);
        self.select_reply2(dev, status);
    }

    /// `select_sdev_reply2`（select.c:1194-1210）。
    pub fn select_sdev_reply2(&mut self, dev: u64, status: i32) {
        if status == 0 {
            return;
        }
        self.select_reply2(dev, status);
    }

    fn select_reply2(&mut self, dev: u64, status: i32) {
        for s in 0..crate::select::MAXSELECTS {
            // 拷出本槽的 fd 关联（后面要对表做可变借用，不跨写持借用）。
            let filps = match self.select_table.get(s) {
                Some(se) if se.requestor.is_some() => se.filps.clone(),
                _ => continue,
            };
            let mut found = false;
            for (fd, e) in filps.iter().enumerate() {
                let Some(e) = e else { continue };
                let dev_hit = self
                    .filp_table
                    .get(crate::filp::FilpId(e.filp))
                    .map(|f| f.select_dev == dev)
                    .unwrap_or(false);
                if !dev_hit {
                    continue;
                }
                let (flags, ops) = {
                    let f = self.filp_table.get(crate::filp::FilpId(e.filp)).unwrap();
                    (
                        crate::filp::FsfFlags::from_bits_truncate(f.select_flags as u32),
                        crate::select::SelOps::from_bits_truncate(f.select_ops),
                    )
                };
                let hit = crate::select::reply2_hit(flags, ops, true, status);
                if let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(e.filp)) {
                    f.select_ops = hit.ops.bits();
                    f.select_flags = hit.flags.bits() as u8;
                }
                let se = self.select_table.get_mut(s).unwrap();
                if let Some(err) = hit.error {
                    se.error = err;
                } else if !hit.ready.is_empty_real() {
                    let nfds = se.nfds;
                    if fd < nfds {
                        let want = (
                            se.vir_readfds != 0
                                && crate::select::bit_of(&se.readfds, fd),
                            se.vir_writefds != 0
                                && crate::select::bit_of(&se.writefds, fd),
                            se.vir_errorfds != 0
                                && crate::select::bit_of(&se.errorfds, fd),
                        );
                        let (a, b, c, mut n) = (
                            &mut se.ready_readfds,
                            &mut se.ready_writefds,
                            &mut se.ready_errorfds,
                            se.nready,
                        );
                        crate::select::ops2tab_store(hit.ready, fd, want, (a, b, c), &mut n);
                        se.nready = n;
                    }
                }
                found = true;
            }
            if found {
                self.select_restart_proc(s);
            }
        }
        self.select_restart_filps();
    }

    /// `select_restart_filps`（select.c:1212-1260）：重启 deferred 的
    /// 查询（INIT 拆两张卡的更新态 filp——SUSPEND 期间先答一个，回复
    /// 落地后再问下一个）。
    pub fn select_restart_filps(&mut self) {
        for s in 0..crate::select::MAXSELECTS {
            let (filps, block, requestor) = match self.select_table.get(s) {
                Some(se) if se.requestor.is_some() => {
                    (se.filps.clone(), se.block, se.requestor)
                }
                _ => continue,
            };
            let deferred = self.select_any_update_or_busy(s);
            if !deferred {
                continue;
            }
            for (fd, e) in filps.iter().enumerate() {
                let Some(e) = e else { continue };
                let (busy, update, ops, kind) = {
                    let Some(f) = self.filp_table.get(crate::filp::FilpId(e.filp)) else {
                        continue;
                    };
                    let fl = crate::filp::FsfFlags::from_bits_truncate(f.select_flags as u32);
                    (
                        fl.contains(crate::filp::FsfFlags::BUSY),
                        fl.contains(crate::filp::FsfFlags::UPDATE),
                        crate::select::SelOps::from_bits_truncate(f.select_ops),
                        e.kind,
                    )
                };
                if busy || !update {
                    continue;
                }
                if !matches!(kind, crate::select::FdKind::Char | crate::select::FdKind::Sock) {
                    // C 断言只处理字符/套接字（select.c:1237-1240）。
                    continue;
                }
                let is_char = kind == crate::select::FdKind::Char;
                match self.select_request_driver(
                    requestor,
                    e.filp,
                    is_char,
                    ops,
                    block,
                ) {
                    Ok(ready) => {
                        if !ready.is_empty_real() {
                            let se = self.select_table.get_mut(s).unwrap();
                            let want = (
                                se.vir_readfds != 0
                                    && crate::select::bit_of(&se.readfds, fd),
                                se.vir_writefds != 0
                                    && crate::select::bit_of(&se.writefds, fd),
                                se.vir_errorfds != 0
                                    && crate::select::bit_of(&se.errorfds, fd),
                            );
                            let (a, b, c, mut n) = (
                                &mut se.ready_readfds,
                                &mut se.ready_writefds,
                                &mut se.ready_errorfds,
                                se.nready,
                            );
                            crate::select::ops2tab_store(ready, fd, want, (a, b, c), &mut n);
                            se.nready = n;
                        }
                    }
                    Err(err) => {
                        let se = self.select_table.get_mut(s).unwrap();
                        se.error = err;
                        self.select_restart_proc(s);
                        break;
                    }
                }
            }
        }
    }

    /// `select_timeout_check`（select.c:861-880）的单闹钟近似：CLOCK 通知
    /// 到点时取**最早到期**的槽收尾；其余槽的 expiry 同减这段时间；还
    /// 有挂着的就重设闹钟。
    ///
    /// [ARCH: select 单闹钟近似] C 的每槽 `timer`（set_timer 链）在这里
    /// 收敛成"一个内核闹钟 + 到点全表扫描"：同一到期时刻的槽一起收尾，
    /// 数学与 timer 链一致，省掉 per-slot 内核闹钟面。三处一致标注随
    /// 本批 doc 同步。
    pub fn select_timeout_check(&mut self) -> bool {
        // 找最小非零 expiry。
        let mut min: Option<(usize, u64)> = None;
        for s in 0..crate::select::MAXSELECTS {
            let Some(se) = self.select_table.get(s) else { continue };
            if se.requestor.is_none() || se.expiry == 0 {
                continue;
            }
            if min.map(|(_, t)| se.expiry < t).unwrap_or(true) {
                min = Some((s, se.expiry));
            }
        }
        let Some((_, elapsed)) = min else { return false };
        // 其余槽同减这段时间；到期的槽（expiry == elapsed）收尾。
        let mut fired = alloc::vec::Vec::new();
        for s in 0..crate::select::MAXSELECTS {
            let Some(se) = self.select_table.get(s) else { continue };
            if se.requestor.is_none() || se.expiry == 0 {
                continue;
            }
            let left = se.expiry.saturating_sub(elapsed);
            let se = self.select_table.get_mut(s).unwrap();
            se.expiry = left;
            if left == 0 {
                fired.push(s);
            }
        }
        for s in fired {
            let Some(se) = self.select_table.get(s) else { continue };
            if se.requestor.is_none() {
                continue;
            }
            let se = self.select_table.get_mut(s).unwrap();
            se.expiry = 0;
            let deferred = self.select_any_update_or_busy(s);
            if deferred {
                // 定时器来得太早：转非阻塞重试（C 同款）。
                let se = self.select_table.get_mut(s).unwrap();
                se.block = false;
                self.select_restart_proc(s);
            } else {
                self.select_return(s);
            }
        }
        // 还有挂着的就重设闹钟。
        let next = (0..crate::select::MAXSELECTS)
            .filter_map(|s| self.select_table.get(s))
            .filter(|se| se.requestor.is_some() && se.expiry > 0)
            .map(|se| se.expiry)
            .min();
        if let Some(ticks) = next {
            let _ = minix_sys::syscall::sys_setalarm(
                &minix_sys::syscall::DirectKernelCallTransport,
                ticks,
                false,
            );
        }
        true
    }

    /// 驱动回复落槽（C `sdev_reply`/`cdev_reply`/`bdev_reply` 的公共前半）：
    /// 找到**正在等这个驱动**的 worker 槽，把回复落进它的 `sendrec` 并唤醒
    /// （状态转 `Busy`，与 FS 回复同一套续接机制）。
    ///
    /// 找不到等它的槽就按"没有 worker 在等"忽略（`device_map::check_reply`
    /// 的 `ReplyIgnore::NoWorker` 语义）——软失败，主循环继续。
    pub fn handle_drv_reply(&mut self, msg: &Message) -> Result<usize, FsReplyError> {
        // select 的两型回复没有 worker 在等（查询是 asynsend 一发不等），
        // 先于 worker 槽匹配拦截（C 在 cdev_reply/sdev_reply 里各自分派
        // SEL1/SEL2——cdev.c:494-503）。`usize::MAX` = 已由 select 层收尾。
        // SAFETY: 一型 `mess_lchardriver_vfs_sel1 { int status@0; int32
        // minor@4 }`；套接字两型 `mess_lsockdriver_vfs_select_reply {
        // int32 sock_id@0; int status@4 }`（ipc.h）。
        let raw = unsafe { &msg.m_u.raw };
        match msg.m_type {
            t if t == minix_chardriver::protocol::CdevReplyKind::SelectImmediate
                as i32 =>
            {
                let status = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                let minor = u32::from_le_bytes(raw[4..8].try_into().unwrap());
                self.select_cdev_reply1(msg.m_source, minor, status);
                return Ok(usize::MAX);
            }
            t if t == minix_chardriver::protocol::CdevReplyKind::SelectNotify as i32 => {
                let status = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                let minor = u32::from_le_bytes(raw[4..8].try_into().unwrap());
                self.select_cdev_reply2(msg.m_source, minor, status);
                return Ok(usize::MAX);
            }
            t if t == minix_sockdriver::sdev::SdevReply::SelectReply1 as i32
                || t == minix_sockdriver::sdev::SdevReply::SelectReply2 as i32 => {
                // C sdev.c:1017-1028：先按来源找 smap 行（`sp`），再拼
                // `make_smap_dev(sp->smap_num, sock_id)`——注意用行的
                // **一基 num 字段**，`smap_by_endpt` 给的是数组位置。
                let sock_id = u32::from_le_bytes(raw[0..4].try_into().unwrap());
                let status = i32::from_le_bytes(raw[4..8].try_into().unwrap());
                let sdev_dev = crate::device_map::smap_by_endpt(&self.smap_table, msg.m_source)
                    .and_then(|pos| self.smap_table.entries.get(pos as usize))
                    .map(|row| crate::device_map::make_smap_dev(row.num, sock_id));
                if let Some(dev) = sdev_dev {
                    if msg.m_type == minix_sockdriver::sdev::SdevReply::SelectReply1 as i32 {
                        self.select_sdev_reply1(dev, status);
                    } else {
                        self.select_sdev_reply2(dev, status);
                    }
                }
                return Ok(usize::MAX);
            }
            _ => {}
        }
        let slot = (0..crate::worker::NR_WTHREADS).find(|i| {
            self.worker_pool
                .get(*i)
                .is_some_and(|w| w.task == Some(msg.m_source))
        });
        let Some(slot) = slot else {
            // 没有 worker 在等：试试**进程级挂起**那条路（C `sdev_reply` 的
            // 第二条分支——bind/connect/accept/recvfrom 这些调用的回复）。
            if self.finish_sdev_blocked(msg) {
                return Ok(usize::MAX); // 已由被挂起的进程收尾
            }
            return Err(FsReplyError::WrongTask);
        };
        let wp = self
            .worker_pool
            .get_mut(slot)
            .ok_or(FsReplyError::SlotOutOfRange)?;
        // 驱动回复的 `m_type` 就是状态/回复号（C 的 `w_drv_sendrec` 原样收），
        // 不像 FS 回复那样带 transid——所以这里**不剥**。
        wp.sendrec = Some(*msg);
        wp.task = None;
        wp.state = crate::worker::WorkerState::Busy;
        Ok(slot)
    }

    /// 给槽发一条 `REQ_SYNC`（C `req_sync`：空载荷、状态回复）。
    ///
    /// **不动槽上的续接标识**：调用方（`begin_sync_sequence` 与
    /// `SyncMounts` 续接体）自己把它设成序列状态——这里若顺手写成
    /// `Status`，序列的"还剩几条"就丢了（本轮踩过）。
    pub fn send_sync_for_slot(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        fs_e: Endpoint,
    ) -> Result<(), i32> {
        let vmnt = self.vmnt_table.find_by_fs(fs_e).ok_or(minix_types::EIO)?.0;
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        self.pending_fs = Some(PendingFs {
            vmnt,
            fs_e,
            worker: idx,
            grant: 0, // 无数据面
            user,
            req: Message {
                m_type: minix_types::REQ_SYNC,
                ..Message::default()
            },
        });
        Ok(())
    }

    /// 启动一串 `REQ_SYNC`（`sync`/`fsync` 共用）：把目标挂载收进定长数组，
    /// 发第一条，剩下的留给 `WorkerCont::SyncMounts` 续接体逐条发。
    pub fn begin_sync_sequence(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        dev: Option<minix_types::DevId>,
    ) -> Result<(), i32> {
        let targets = self.sync_targets(dev);
        let mut arr = [Endpoint::NONE; crate::vmnt::NR_MNTS];
        for (i, ep) in targets.iter().enumerate().take(crate::vmnt::NR_MNTS) {
            arr[i] = *ep;
        }
        if targets.is_empty() {
            // C 的循环一条都没发：直接成功（没有挂载要同步不是错误）。
            return Ok(());
        }
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::SyncMounts {
                targets: arr,
                count: targets.len().min(crate::vmnt::NR_MNTS) as u8,
                at: 1, // 第 0 条由下面发
                first_err: 0,
            });
        }
        self.send_sync_for_slot(idx, fp_slot, targets[0])
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

    /// 收尾一个挂起的作业并回用户（**错误收尾**与相位 2 之后的统一出口）。
    ///
    /// `status` 是**错误码**：正号的 errno 常量（`minix_types::EINVAL` 一类，
    /// crate 惯例）或从 FS/驱动回复带来的**负值**线上状态——两种写法都收，
    /// 非零一律按错误收（[`Self::queue_reply`] 在边界折成负号）。成功**值**
    /// （fd、字节数、条数）走 [`Self::finish_worker_job_value`]。
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
            } else if status > 0 {
                crate::call_table::SyscallResult::Error(status)
            } else {
                // 线上带来的负值状态：先翻回正号 errno，边界再统一折负。
                crate::call_table::SyscallResult::Error(-status)
            };
            self.queue_reply(target, result);
        }
    }

    /// [`Self::finish_worker_job`] 的成功**值**出口：`open`/`accept` 回 fd、
    /// `read`/`write` 回字节数、`getvfsstat` 回挂载条数——这些都是非负的
    /// 成功值（C 的 `reply(who, r)` 里同一个 `r`），但走错误收尾会被边界
    /// 折成负号，用户拿到的就成了一个错误。0 也走这里（等于 `Ok(0)`）。
    pub fn finish_worker_job_value(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        value: i32,
    ) {
        debug_assert!(value >= 0, "成功值非负；错误走 finish_worker_job");
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
            self.queue_reply(target, crate::call_table::SyscallResult::Ok(value));
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

    /// 把排队的 `REQ_PUTNODE` 逐条发出（`put_vnode` 慢路径的投递半）。
    ///
    /// C 在 worker 里同步 `fs_sendrec`（vnode.c:278），失败 `printf`
    /// （vnode.c:281-283）——不回用户、不重试。模型里回复被忽略，发送
    /// 失败同样丢弃：对用户可观测的行为一致（FS 侧引用计数由 FS 自己
    /// 的回收面兜底）。
    pub fn flush_pending_puts(&mut self, transport: &impl minix_sys::ipc::IpcTransport) {
        let puts: alloc::vec::Vec<crate::vnode::PutNodeReq> =
            core::mem::take(&mut self.pending_puts);
        for p in puts {
            // C `req_putnode`（request.c:699-711）：sendrec，回复只查错。
            // 队列按挂载行过滤（挂载没了 = 无处可发，丢弃）。
            if self.vmnt_table.find_by_fs(p.fs_e).is_none() {
                continue;
            }
            let _ = transport.sendrec(
                p.fs_e,
                &mut crate::request::encode_putnode(p.ino, p.count as i32),
            );
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
            if status == -(minix_types::ERESTART) {
                // C `comm.c:161-163` 的 `r = reqmp->m_type; if (r == ERESTART)
                // r = EIO;`——FS 的回复 m_type 在线上带负号（服务端都是
                // `_SYSTEM` 构建），比较与结果都要用负号。
                status = -(minix_types::EIO);
            }
            // `status` 何时是**成功值**（字节数/条数）而不是错误码：读/写与
            // 目录读取的收尾把实际字节数、`getvfsstat` 把挂载条数放进同一个
            // 槽——置位的臂由收尾按 `Ok(value)` 发（负号折算只认错误码）。
            let mut value_reply = false;
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
                crate::worker::WorkerCont::Freesp { vnode, zero_len, start } => {
                    // C misc.c:236-237：`F_FREESP` 的零长（`l_len == 0`）
                    // 在 `req_ftrunc` 成功后把 `v_size` 收到 `start`。
                    if status == 0 && zero_len
                        && let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode))
                        && (start < 0 || (start as u64) <= v.size)
                    {
                        v.size = if start < 0 { 0 } else { start as u64 };
                    }
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
                crate::worker::WorkerCont::SdevGetSet { grant, write_dir } => {
                    // C `sdev_setsockopt`/`sdev_get` 的收尾：撤 grant →
                    // 回复号必须是 `SDEV_REPLY` → 状态取载荷首字；`get` 方向
                    // 那个状态就是**新长度**，塞进回复载荷
                    // （`m_vfs_lc_socklen { len }`，C `do_getsockopt:696-697`）。
                    let _ = self.revoke_grant(grant);
                    if status != minix_sockdriver::sdev::SdevReply::Reply as i32 {
                        status = minix_types::EIO;
                    } else {
                        // SAFETY: `mess_lsockdriver_vfs_reply { int32_t
                        // req_id; int status; }`——状态在第二格（`req_id` 在前）。
                        let raw = unsafe { &reply.m_u.raw };
                        status = i32::from_le_bytes(raw[4..8].try_into().unwrap());
                        if write_dir && status >= 0 {
                            let len = status as u32;
                            let mut m = Message {
                                m_type: 0,
                                ..Message::default()
                            };
                            // SAFETY: `mess_vfs_lc_socklen { unsigned int len; }`
                            // 在负载区首字。
                            unsafe {
                                m.m_u.raw[0..4].copy_from_slice(&len.to_le_bytes());
                            }
                            reply_payload = Some(m);
                            status = 0;
                        }
                    }
                }
                crate::worker::WorkerCont::CdevOpen { fd, filp, dev } => {
                    // C `cdev_opcl` 的收尾（cdev.c:236-249）：状态取
                    // `mess_lchardriver_vfs_reply.status`（**首字**）；`>= 0`
                    // 时低位是两个效果位——`CDEV_CLONED`（要 PFS 建克隆节点，
                    // 未接）与 `CDEV_CTTY`（把设备记成控制终端）。
                    // SAFETY: `mess_lchardriver_vfs_reply { int status;
                    // uint32_t id; }`——状态在首字。
                    let raw = unsafe { &reply.m_u.raw };
                    let dstatus = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    let vnode = self
                        .filp_table
                        .get(crate::filp::FilpId(filp))
                        .and_then(|f| f.vnode)
                        .unwrap_or(0);
                    if dstatus < 0 {
                        // 驱动拒绝：走 C 的失败尾（放开 fd/filp + 放回 vnode），
                        // 把错误回用户。
                        self.release_open_claim(fp_slot, fd, filp, vnode);
                        self.finish_worker_job(idx, fp_slot, dstatus);
                        continue;
                    }
                    let effects = crate::cdev::open_effects(dstatus, dev);
                    if effects.clone_minor.is_some() {
                        // `cdev_clone`（cdev.c:100-145）要 PFS 建一个新节点
                        // （`req_newnode(PFS_PROC_NR, ...)`）——PFS 是另一条线，
                        // 本批未接：诚实拒绝并走失败尾（不假装打开了一个克隆
                        // 设备）。
                        self.release_open_claim(fp_slot, fd, filp, vnode);
                        self.finish_worker_job(idx, fp_slot, minix_types::ENOSYS);
                        continue;
                    }
                    if let Some(tty_dev) = effects.grant_tty {
                        // C：`fp->fp_tty = dev; dp->dmap_seen_tty = TRUE;`
                        if let Some(slot) = fp_slot
                            && let Some(fp) = self.fproc_table.get_mut(slot)
                        {
                            fp.tty = tty_dev;
                        }
                        let major = ((tty_dev & 0x000fff00) >> 8) as u32;
                        if let Some(row) = self.dmap_table.get_mut(major) {
                            row.seen_tty = true;
                        }
                    }
                    // 成功：用户拿到的是 fd（C `r = OK` 之后 `common_open`
                    // 把 `r = fd` 回上去）。
                    self.finish_worker_job_value(idx, fp_slot, fd as i32);
                    continue;
                }
                crate::worker::WorkerCont::BdevOpen {
                    fd,
                    filp,
                    vnode,
                    dev,
                    minor,
                    access,
                    retries,
                } => {
                    // C `bdev_sendrec`（bdev.c:36-58）的对话半：状态取
                    // `mess_lblockdriver_lbdev_reply.status`（**首字**）。
                    // SAFETY: `{ int status; int id; }`（ipc.h:356-361）。
                    let raw = unsafe { &reply.m_u.raw };
                    let mut dstatus = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    let mut finished = false;
                    if dstatus == crate::bdev::SEND_RESTART {
                        // 驱动回 `ERESTART` 就原样重发，五次为限；烧断即 EIO
                        // （C 的 `retry_count < 5` 保险丝）。
                        match crate::bdev::RetryState(retries).step(dstatus) {
                            crate::bdev::RetryVerdict::Again => {
                                let major = ((dev & 0x000fff00) >> 8) as u32;
                                let drv_e =
                                    self.dmap_table.get(major).and_then(|row| row.driver);
                                let mut resent = false;
                                if let Some(drv_e) = drv_e {
                                    // 重发**同一条**请求（C 的 `*mess_ptr =
                                    // mess_retry`），计数留在续接体里。
                                    let m = crate::bdev::open_request(minor, access);
                                    if let Some(wp) = self.worker_pool.get_mut(idx) {
                                        wp.cont = Some(crate::worker::WorkerCont::BdevOpen {
                                            fd,
                                            filp,
                                            vnode,
                                            dev,
                                            minor,
                                            access,
                                            retries: retries + 1,
                                        });
                                    }
                                    resent = self
                                        .send_drv_for_slot(idx, fp_slot, drv_e, &m)
                                        .is_ok();
                                }
                                if resent {
                                    continue;
                                }
                                // 驱动没了或发不出去：C 的 `bdev_sendrec` 在这
                                // 两处都回 EIO。
                                dstatus = minix_types::EIO;
                            }
                            _ => dstatus = minix_types::EIO,
                        }
                    }
                    if dstatus != 0 {
                        self.release_open_claim(fp_slot, fd, filp, vnode);
                        self.finish_worker_job(idx, fp_slot, dstatus);
                        finished = true;
                    }
                    if finished {
                        continue;
                    }
                    // 驱动放行：第二段（`v_bfs_e` 选择 + 可能的
                    // `REQ_NEW_DRIVER`）。挂上 FS 对话时这一段返回 `true`，
                    // 作业留给 `BdevNewDriver` 续接体收尾。
                    match self.bdev_open_bfs_stage(idx, fp_slot, vnode, fd, filp, dev) {
                        Ok(true) => continue,
                        Ok(false) => {
                            self.finish_worker_job_value(idx, fp_slot, fd as i32);
                            continue;
                        }
                        Err(e) => {
                            self.release_open_claim(fp_slot, fd, filp, vnode);
                            self.finish_worker_job(idx, fp_slot, e);
                            continue;
                        }
                    }
                }
                crate::worker::WorkerCont::BdevNewDriver { fd, filp, vnode, dev } => {
                    // C `open.c:210-216`：`req_newdriver` 成功就照常回 fd；
                    // 失败要**先给驱动发 `BDEV_CLOSE`** 把刚打开的设备关掉，
                    // 再回 `ENXIO`（close 的返回值被丢掉）。
                    if status == 0 {
                        self.finish_worker_job_value(idx, fp_slot, fd as i32);
                        continue;
                    }
                    let major = ((dev & 0x000fff00) >> 8) as u32;
                    let minor = (((dev & 0xfff0_0000) >> 12) | (dev & 0xff)) as u32;
                    let drv_e = self.dmap_table.get(major).and_then(|row| row.driver);
                    let m = crate::bdev::close_request(minor);
                    if let Some(wp) = self.worker_pool.get_mut(idx) {
                        // 等驱动的 close 回复到了再收尾——**不**把这条请求
                        // 发出去就撒手（撒手会让回复落到别的作业头上）。
                        wp.cont = Some(crate::worker::WorkerCont::BdevCloseThenReply {
                            status: minix_types::ENXIO,
                            fd,
                            filp,
                            vnode,
                        });
                    }
                    let sent = match drv_e {
                        Some(drv_e) => self.send_drv_for_slot(idx, fp_slot, drv_e, &m).is_ok(),
                        None => false,
                    };
                    if !sent {
                        // 驱动没了或发不出去：C 的 `bdev_close` 在这里回 ENXIO，
                        // 调用点丢掉它——直接走失败尾。
                        self.release_open_claim(fp_slot, fd, filp, vnode);
                        self.finish_worker_job(idx, fp_slot, minix_types::ENXIO);
                    }
                    continue;
                }
                crate::worker::WorkerCont::BdevCloseThenReply {
                    status,
                    fd,
                    filp,
                    vnode,
                } => {
                    // 补偿的 `BDEV_CLOSE` 回来了：走 C 的失败尾，再回原来的
                    // 错误（`open.c:213` 的 `r = ENXIO`）。close 的状态不看
                    // （C 是 `(void)bdev_close(dev)`）。
                    self.release_open_claim(fp_slot, fd, filp, vnode);
                    self.finish_worker_job(idx, fp_slot, status);
                    continue;
                }
                crate::worker::WorkerCont::BdevIoctl { grant, filp } => {
                    // C `bdev_ioctl` 的收尾：撤 grant → 清 `filp_ioctl_fp` →
                    // 状态取 `mess_lblockdriver_lbdev_reply.status`（**首字**，
                    // 与字符设备同族、与套接字不同族）。
                    let _ = self.revoke_grant(grant);
                    if let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(filp)) {
                        f.ioctl_holder = None;
                    }
                    if status == 0 {
                        // SAFETY: `mess_lblockdriver_lbdev_reply { int status;
                        // int id; }`（ipc.h:356-361）——状态在首字。
                        let raw = unsafe { &reply.m_u.raw };
                        status = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    }
                }
                crate::worker::WorkerCont::CdevIoctl { grant } => {
                    // C `cdev_io` 的收尾：撤 grant → 状态取
                    // `mess_lchardriver_vfs_reply.status`（**首字**，这个回复
                    // 结构体的第一格就是状态，不是 `req_id`）。
                    let _ = self.revoke_grant(grant);
                    if status == 0 {
                        // SAFETY: `mess_lchardriver_vfs_reply { int status;
                        // uint32_t id; }`（ipc.h:943-948）——状态在首字。
                        let raw = unsafe { &reply.m_u.raw };
                        status = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    }
                }
                crate::worker::WorkerCont::SdevSimple => {
                    // C `sdev_simple` 的收尾：回复号必须是 `SDEV_REPLY`，
                    // 状态在**载荷**里（`mess_lsockdriver_vfs_reply.status`）。
                    if status != minix_sockdriver::sdev::SdevReply::Reply as i32 {
                        status = minix_types::EIO;
                    } else {
                        // SAFETY: `mess_lsockdriver_vfs_reply { int32_t
                        // req_id; int status; }`（ipc.h:1023-1028）——**状态在
                        // 第二格**（`req_id` 在前），不是首字。
                        let raw = unsafe { &reply.m_u.raw };
                        status = i32::from_le_bytes(raw[4..8].try_into().unwrap());
                    }
                }
                crate::worker::WorkerCont::SdevSocket { pair, flags, smap_num } => {
                    // C `sdev_socket`（sdev.c:140-170）：回复号必须是
                    // `SDEV_SOCKET_REPLY`（否则 EIO），`sock_id < 0` 就是驱动
                    // 报的错误；成功后设备号 = `make_smap_dev(行号, sock_id)`。
                    // 回复号必须是 `SDEV_SOCKET_REPLY`（`SdevReply::SocketReply`）。
                    if status != minix_sockdriver::sdev::SdevReply::SocketReply as i32 {
                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                        continue;
                    }
                    // SAFETY: 回复载荷 `mess_lsockdriver_vfs_socket_reply`
                    // （req_id@0、sock_id@4、sock_id2@8）。
                    let (sock_id, sock_id2) = unsafe {
                        let raw = &reply.m_u.raw;
                        (
                            i32::from_le_bytes(raw[4..8].try_into().unwrap()),
                            i32::from_le_bytes(raw[8..12].try_into().unwrap()),
                        )
                    };
                    if sock_id < 0 {
                        self.finish_worker_job(idx, fp_slot, sock_id);
                        continue;
                    }
                    let dev = crate::device_map::make_smap_dev(smap_num, sock_id as u32);
                    if pair {
                        // 成对：两个设备号都要 `make_sock_fd`。先建第一个，
                        // 第二个由 `SockFd` 的第一半收尾接着建（C socket.c:
                        // 249-266）——`pending_fs` 一次只装一条请求，所以两半
                        // 是**串行**的，不是并发。
                        if sock_id2 < 0 {
                            // C：`sock_id2 < 0` 是协议错误 → 先关掉 dev0 再 EIO。
                            if let Some(wp) = self.worker_pool.get_mut(idx) {
                                wp.cont = Some(crate::worker::WorkerCont::SdevCloseThenReply {
                                    status: minix_types::EIO,
                                });
                            }
                            if self
                                .send_sdev_simple(
                                    idx,
                                    fp_slot,
                                    dev,
                                    minix_sockdriver::sdev::SdevRequest::Close as i32,
                                    0,
                                )
                                .is_err()
                            {
                                self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                            }
                            continue;
                        }
                        let dev1 = crate::device_map::make_smap_dev(smap_num, sock_id2 as u32);
                        if let Err(e) = self.begin_make_sock_fd(
                            idx,
                            fp_slot,
                            dev,
                            flags,
                            None,
                            Some(crate::worker::PairState {
                                other_dev: dev1,
                                fd0: 0,
                                second: false,
                            }),
                        ) {
                            self.finish_worker_job(idx, fp_slot, e);
                        }
                        continue;
                    }
                    if let Err(e) = self.begin_make_sock_fd(idx, fp_slot, dev, flags, None, None) {
                        self.finish_worker_job(idx, fp_slot, e);
                    }
                    continue;
                }
                crate::worker::WorkerCont::SockFd {
                    filp,
                    fd,
                    flags,
                    vnode,
                    dev,
                    addr_len_out,
                    pair,
                } => {
                    // C `make_sock_fd` 的后半（socket.c:140-176）：用回复的
                    // `node_details` 填 vnode（`v_sdev` 是套接字设备号）与 filp，
                    // 再按 `flags` 置 CLOEXEC。**用户拿到的返回值是 fd**。
                    if status != 0 {
                        self.filp_table.dec_count(crate::filp::FilpId(filp));
                        if let Some(slot) = fp_slot
                            && let Some(fp) = self.fproc_table.get_mut(slot)
                        {
                            fp.filps[fd as usize] = None;
                        }
                        if let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode)) {
                            v.ref_count = 0;
                        }
                        self.finish_worker_job(idx, fp_slot, status);
                        continue;
                    }
                    let node = crate::request::decode_lookup_reply(status, &reply);
                    let Some(crate::path::LookupRes::Ok { ino, mode, .. }) = node else {
                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                        continue;
                    };
                    if let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode)) {
                        v.fs = minix_types::Endpoint::PFS;
                        v.map_fs = minix_types::Endpoint::PFS;
                        v.ino = ino;
                        v.map_ino = ino;
                        v.mode = mode;
                        v.fs_count = 1;
                        v.mapfs_count = 1;
                        v.ref_count = 1;
                        v.size = 0;
                        v.dev = minix_types::NO_DEV;
                        // C socket.c:157 —— `vp->v_sdev = dev`：套接字靠它认驱动。
                        v.sdev = dev;
                    }
                    if let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(filp)) {
                        f.vnode = Some(vnode);
                        f.flags = flags as i32;
                    }
                    if let Some(slot) = fp_slot
                        && let Some(fp) = self.fproc_table.get_mut(slot)
                        && flags & crate::open::OpenFlags::CLOEXEC.bits() != 0
                    {
                        fp.cloexec_set.set(fd as usize, true);
                    }
                    // `socketpair` 的成对编排（C `do_socketpair:249-266`）：
                    // - 第一半成功 → 接着建第二半（把 fd0 与"这是第二半"带过去）；
                    // - 第二半成功 → 回复 `m_vfs_lc_fdpair { fd0, fd1 }`；
                    // - 第二半失败 → `close_fd(fp, fd0)` + 关掉 dev1（C 的
                    //   `close_fd` + `(void)sdev_close(dev[1])`）。
                    if let Some(pair_state) = pair {
                        if !pair_state.second {
                            if let Err(e) = self.begin_make_sock_fd(
                                idx,
                                fp_slot,
                                pair_state.other_dev,
                                flags,
                                None,
                                Some(crate::worker::PairState {
                                    other_dev: 0,
                                    fd0: fd as u32,
                                    second: true,
                                }),
                            ) {
                                // 第二半起不来：关掉第一半的 fd + 那个设备。
                                // C 的 `close_fd(fp, fd0, may_suspend=FALSE)`：
                                // 清掉第一半的 fd（本地动作，不发驱动请求——
                                // C 的 `may_suspend=FALSE` 就是"别在这里阻塞"）。
                                if let Some(slot) = fp_slot
                                    && let Some(fd0) = crate::filedes::Fd::new(fd as usize)
                                    && let Some(fp) = self.fproc_table.get_mut(slot)
                                {
                                    let _ = crate::filedes::close_fd(fp, fd0, &mut self.filp_table);
                                }
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont =
                                        Some(crate::worker::WorkerCont::SdevCloseThenReply {
                                            status: e,
                                        });
                                }
                                if self
                                    .send_sdev_simple(
                                        idx,
                                        fp_slot,
                                        pair_state.other_dev,
                                        minix_sockdriver::sdev::SdevRequest::Close as i32,
                                        0,
                                    )
                                    .is_err()
                                {
                                    self.finish_worker_job(idx, fp_slot, e);
                                }
                            }
                            continue;
                        }
                        // 第二半成功：拼 fd 对回复。
                        let mut m = Message {
                            m_type: 0,
                            ..Message::default()
                        };
                        // SAFETY: `mess_vfs_lc_fdpair { int fd0; int fd1; }`
                        // （ipc.h:2198-2203）在负载区前两字。
                        unsafe {
                            m.m_u.raw[0..4].copy_from_slice(&(pair_state.fd0 as i32).to_le_bytes());
                            m.m_u.raw[4..8].copy_from_slice(&(fd as i32).to_le_bytes());
                        }
                        let target = fp_slot
                            .and_then(|s| self.fproc_table.get(s))
                            .map(|fp| fp.endpoint)
                            .unwrap_or(Endpoint::NONE);
                        self.queue_reply_msg(target, m);
                        self.finish_worker_job(idx, fp_slot, 0);
                        continue;
                    }
                    if let Some(len) = addr_len_out {
                        // `accept` 的收尾：fd 是状态、**对端地址长度在载荷里**
                        // （C `resume_accept` 末段的 `m_vfs_lc_socklen.len`）。
                        let mut m = Message {
                            m_type: fd as i32,
                            ..Message::default()
                        };
                        // SAFETY: `mess_vfs_lc_socklen { unsigned int len; }`。
                        unsafe {
                            m.m_u.raw[0..4].copy_from_slice(&len.to_le_bytes());
                        }
                        self.queue_reply_msg(
                            fp_slot
                                .and_then(|s| self.fproc_table.get(s))
                                .map(|fp| fp.endpoint)
                                .unwrap_or(Endpoint::NONE),
                            m,
                        );
                        self.finish_worker_job_value(idx, fp_slot, fd as i32);
                        continue;
                    }
                    self.finish_worker_job_value(idx, fp_slot, fd as i32);
                    continue;
                }
                crate::worker::WorkerCont::SdevCloseThenReply { status } => {
                    // 关掉那个"多出来的"套接字之后，回**原来的错误**
                    // （C `resume_accept` 的 `(void)sdev_close(dev, ...)`）。
                    self.finish_worker_job(idx, fp_slot, status);
                    continue;
                }
                crate::worker::WorkerCont::Pipe2 {
                    filp0,
                    filp1,
                    fd0,
                    fd1,
                    flags,
                    vnode,
                } => {
                    // C `create_pipe` 的后半（pipe.c:117-131）：拿新节点的
                    // `node_details` 填 vnode 与两个 filp，再把 fd 对放进回复
                    // 载荷。失败时按 `rollback_for(Node)` 回滚两端。
                    if status != 0 {
                        let plan = crate::pipe::rollback_for(crate::pipe::CreateStage::Node);
                        let _ = plan;
                        if let Some(slot) = fp_slot {
                            if let Some(fp) = self.fproc_table.get_mut(slot) {
                                if plan.free_read {
                                    fp.filps[fd0 as usize] = None;
                                }
                                if plan.free_write {
                                    fp.filps[fd1 as usize] = None;
                                }
                            }
                        }
                        // C 用 `filp_count = 0` 把 filp 标回空闲；Rust 侧就是
                        // `dec_count`（归零即释放）。
                        self.filp_table.dec_count(crate::filp::FilpId(filp0));
                        self.filp_table.dec_count(crate::filp::FilpId(filp1));
                        if let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode)) {
                            v.ref_count = 0;
                            v.fs_count = 0;
                        }
                        self.finish_worker_job(idx, fp_slot, status);
                        continue;
                    }
                    // 回复的 `node_details`（`mess_fs_vfs_newnode`：file_size/
                    // device/inode/mode/uid/gid）——字段序与 `lookup_reply_off`
                    // 的前六域一致，复用那张表。
                    let node = crate::request::decode_lookup_reply(status, &reply);
                    let Some(crate::path::LookupRes::Ok { ino, mode, .. }) = node else {
                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                        continue;
                    };
                    if let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode)) {
                        v.fs = minix_types::Endpoint::PFS;
                        v.map_fs = minix_types::Endpoint::PFS;
                        v.ino = ino;
                        v.map_ino = ino;
                        v.mode = mode;
                        v.fs_count = 1;
                        v.mapfs_count = 1;
                        v.ref_count = 1;
                        v.size = 0;
                        v.dev = minix_types::NO_DEV;
                    }
                    // 两个 filp：读端 `O_RDONLY | (flags & ~O_ACCMODE)`、写端
                    // `O_WRONLY | ...`；vnode 再 dup 一次（两端各持一个引用）。
                    let extra = (flags as u32 & !crate::open::O_ACCMODE) as i32;
                    if let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(filp0)) {
                        f.vnode = Some(vnode);
                        f.flags = (crate::open::O_RDONLY as i32) | extra;
                    }
                    if let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(filp1)) {
                        f.vnode = Some(vnode);
                        f.flags = (crate::open::O_WRONLY as i32) | extra;
                    }
                    self.vnode_table.dup(crate::vnode::VnodeId(vnode));
                    if let Some(slot) = fp_slot
                        && let Some(fp) = self.fproc_table.get_mut(slot)
                        && flags & crate::open::OpenFlags::CLOEXEC.bits() as i32 != 0
                    {
                        fp.cloexec_set.set(fd0 as usize, true);
                        fp.cloexec_set.set(fd1 as usize, true);
                    }
                    // 回复载荷：`m_vfs_lc_fdpair { fd0, fd1 }`（用户拿到的
                    // 就是这两个 fd；C `do_pipe2:48-51`）。
                    let mut m = Message {
                        m_type: status,
                        ..Message::default()
                    };
                    // SAFETY: `mess_vfs_lc_fdpair { int fd0; int fd1; }`
                    // （ipc.h:2198-2203）在负载区前两字。
                    unsafe {
                        m.m_u.raw[0..4].copy_from_slice(&(fd0 as i32).to_le_bytes());
                        m.m_u.raw[4..8].copy_from_slice(&(fd1 as i32).to_le_bytes());
                    }
                    reply_payload = Some(m);
                }
                crate::worker::WorkerCont::Statvfs {
                    grant,
                    user_buf,
                    vmnt,
                    seq,
                    seq_count,
                    seq_at,
                } => {
                    // C `update_statvfs` + `fill_statvfs` 的收尾：撤销 grant →
                    // 把 FS 那 17 个字段存进挂载行缓存 → 补本地字段 → 整块拷给
                    // 用户（状态照 C：`update_statvfs` 失败就 EIO，否则看拷贝）。
                    let _ = self.revoke_grant(grant);
                    if status == 0 {
                        use minix_types::statvfs_off as off;
                        let b = self.statvfs_buf;
                        let cached = crate::vmnt::VmntStats {
                            f_flag: b.get_u64(off::FLAG),
                            f_bsize: b.get_u64(off::BSIZE),
                            f_frsize: b.get_u64(off::FRSIZE),
                            f_iosize: b.get_u64(off::IOSIZE),
                            f_blocks: b.get_u64(off::BLOCKS),
                            f_bfree: b.get_u64(off::BFREE),
                            f_bavail: b.get_u64(off::BAVAIL),
                            f_bresvd: b.get_u64(off::BRESVD),
                            f_files: b.get_u64(off::FILES),
                            f_ffree: b.get_u64(off::FFREE),
                            f_favail: b.get_u64(off::FFAVAIL),
                            f_fresvd: b.get_u64(off::FRESVD),
                            f_syncreads: b.get_u64(off::SYNCREADS),
                            f_syncwrites: b.get_u64(off::SYNCWRITES),
                            f_asyncreads: b.get_u64(off::ASYNCREADS),
                            f_asyncwrites: b.get_u64(off::ASYNCWRITES),
                            f_namemax: b.get_u64(off::NAMEMAX),
                        };
                        if let Some(v) = self.vmnt_table.get_mut(crate::vmnt::VmntId(vmnt)) {
                            v.stats = cached;
                        }
                        status = self.finish_statvfs_copy(idx, fp_slot, user_buf, vmnt);
                    }
                    // `getvfsstat` 的序列：还有下一个就继续（用户缓冲按
                    // `i*sizeof(struct statvfs)` 推进），否则收尾回**个数**
                    // （C `do_getvfsstat` 的 `return count`）。
                    if status == 0 && seq_count > 0 {
                        let next = (seq_at + 1) as usize;
                        if next < seq_count as usize {
                            let dst = user_buf + (next as u64) * minix_types::STATVFS_SIZE as u64;
                            if let Err(e) = self.send_statvfs_request(
                                idx,
                                fp_slot,
                                seq[next],
                                dst,
                                (seq, seq_count, seq_at + 1),
                            ) {
                                self.finish_worker_job(idx, fp_slot, e);
                            }
                            continue;
                        }
                        status = seq_count as i32;
                        value_reply = true;
                    }
                }
                crate::worker::WorkerCont::SyncMounts { targets, count, at, first_err } => {
                    // C `do_sync`/`do_fsync` 的循环在单线程模型里的形态：每条
                    // 回复到达就发下一条，发完报 `first_err`（C 把 `req_sync`
                    // 的返回值丢掉，只留加锁错误）。
                    let _ = status; // 单条 REQ_SYNC 的结果不影响用户可见值
                    if (at as usize) < (count as usize) {
                        let next = targets[at as usize];
                        if let Some(wp) = self.worker_pool.get_mut(idx) {
                            wp.cont = Some(crate::worker::WorkerCont::SyncMounts {
                                targets,
                                count,
                                at: at + 1,
                                first_err,
                            });
                        }
                        if let Err(e) = self.send_sync_for_slot(idx, fp_slot, next) {
                            self.finish_worker_job(idx, fp_slot, e);
                        }
                        continue;
                    }
                    // 序列跑完：C 的 `r` 就是加锁结果（这里恒为 first_err）。
                    status = first_err;
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
                        value_reply = true;
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
                        value_reply = true;
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
                            crate::worker::PathFollow::Chdir { into_root } => {
                                // C `do_chdir`/`do_chroot` 的收尾：走完就
                                // `change_into`（本地改本进程的当前/根目录，
                                // 没有 FS 往返）。
                                let Some(vnode) = self.intern_vnode(&node) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                                    continue;
                                };
                                let status = self.change_into(fp_slot, vnode, into_root);
                                self.finish_worker_job(idx, fp_slot, status);
                                continue;
                            }
                            crate::worker::PathFollow::Statvfs { user_buf, flags } => {
                                // C `do_statvfs:318-323`：走完拿 vnode 的
                                // `v_vmnt` 再进 `fill_statvfs`。
                                let Some(vmnt_idx) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                    continue;
                                };
                                match self.begin_statvfs(idx, fp_slot, vmnt_idx.0, user_buf, flags) {
                                    Ok(()) => {}
                                    Err(e) => self.finish_worker_job(idx, fp_slot, e),
                                }
                                continue;
                            }
                            crate::worker::PathFollow::RenameOld { entry, new_path } => {
                                // C `do_rename:194-215`：旧父目录带粘滞位就先做
                                // **子遍历**取受害者属主（与 unlink 同款），
                                // 否则直接转阶段 2。
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
                                    let (uid, gid) = match fp_slot
                                        .and_then(|s| self.fproc_table.get(s))
                                    {
                                        Some(fp) => (fp.eff_uid, fp.eff_gid),
                                        None => {
                                            self.finish_worker_job(
                                                idx,
                                                fp_slot,
                                                minix_types::EINVAL,
                                            );
                                            continue;
                                        }
                                    };
                                    let (walk2, step2) = match crate::path::LookupWalk::begin(
                                        start, resolve, rd, uid, gid,
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
                                            follow: crate::worker::PathFollow::RenameOldSticky {
                                                entry,
                                                new_path,
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
                                self.rename_stage_two(idx, fp_slot, node.fs_e, node.ino, entry, &new_path);
                                continue;
                            }
                            crate::worker::PathFollow::RenameOldSticky { entry, new_path } => {
                                // 子遍历走通：C `do_rename:205-210` 的属主门
                                // （决策函数 `link::sticky_check`），过了转阶段 2。
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
                                self.rename_stage_two(idx, fp_slot, node.fs_e, node.ino, entry, &new_path);
                                continue;
                            }
                            crate::worker::PathFollow::RenameNew {
                                old_fs_e,
                                old_ino,
                                old_name,
                                new_entry,
                            } => {
                                // C `do_rename:242-259`：跨设备门（EXDEV）→
                                // 两个父目录的 `W|X` 门 → `req_rename`。
                                if node.fs_e != old_fs_e {
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
                                // 新父目录的 `W|X` 门（旧父目录那道在阶段 1 的
                                // `rename_stage_two` 里过——C 把两道写在一处，
                                // 但两道的输入不同，Rust 侧按阶段各判一次，
                                // 结果与 C 等价：任一不过就是那个错误码）。
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
                                if let Err(e) = self.send_rename_for_slot(
                                    idx,
                                    fp_slot,
                                    node.fs_e,
                                    old_ino,
                                    node.ino,
                                    &old_name,
                                    &new_entry,
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
                        value_reply = true;
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
                        let result = if value_reply {
                            // 成功值（字节数/条数）：非负，原样回。
                            crate::call_table::SyscallResult::Ok(status)
                        } else if status == 0 {
                            crate::call_table::SyscallResult::Ok(0)
                        } else if status > 0 {
                            crate::call_table::SyscallResult::Error(status)
                        } else {
                            // 线上带来的负值状态：先翻回正号 errno，边界再统一折负。
                            crate::call_table::SyscallResult::Error(-status)
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
    /// 映射照 C 的 `do_work` 尾部 `reply(who_e, result)`（main.c:297 →
    /// `reply:638` 的 `m_out->m_type = result; ipc_sendnb(...)`），但**符号要
    /// 在边界折一次**：C 的 VFS 是 `_SYSTEM` 构建，`errno.h:187-192` 的
    /// `_SIGN` 让 `EINVAL` 这些常量本身带负号（`errno.h:64`），所以 C 写进
    /// `m_type` 的错误码是负值，用户侧 `_syscall` 按 `m_type < 0` 判错
    /// （`minix3/minix/lib/libc/sys/syscall.c:9-25`；Rust 用户侧同约定，
    /// 见 `minix-sys/src/syscall.rs:107`）。Rust 的臂内部用**正号**常量
    /// （crate 惯例，`minix_types::EINVAL == 22`），于是这里统一折成负号。
    ///
    /// 驱动与 FS 回复带来的状态**已经是负值**（那些进程同样是 `_SYSTEM`
    /// 构建），原样透传——`Error(e)` 只在 `e > 0` 时取负，两条来源不会互相
    /// 打架。成功值是 fd/字节数/0，一律非负，`Ok(v)` 不动。
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
            SyscallResult::Error(e) => {
                if e > 0 {
                    -e
                } else {
                    e
                }
            }
            SyscallResult::Nosys => -minix_types::ENOSYS,
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
    // main.c:492-497 — do_init_root 经 worker_start 启动；C 失败即 panic
    // （main.c:519-520），这里同权。
    state.finish_init(
        &minix_sys::syscall::DirectKernelCallTransport,
        &minix_sys::ipc::DirectTrapTransport,
    )
    .unwrap_or_else(|e| panic!("vfs: failed to initialize root: {e:?}"));

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
                // put_vnode 慢路径的 REQ_PUTNODE 投递（sendrec 直连，
                // 回复只查错——C vnode.c:278 的 worker 内同步 sendrec）。
                state.flush_pending_puts(&minix_sys::ipc::DirectTrapTransport);
            }
            SefEvent::Signal(_) => {}
            // E-BIRTHFACE（NS1）出生应答半：C 的 RS_INIT 在 sef_local_startup
            // 内消费后由 process_init 尾部回 RS_INIT+result（sef_init.c:113-117），
            // 这条应答是 RS boot step3 释放的条件。result 按 C 注册面
            // （main.c:377-381）：fresh 实体（本树构造即跑过 ≡ state.init_fresh，
            // init_restart ≡ init_fresh 亦已文档化）→ OK；LU 有注册
            //（sef_cb_init_lu）但 Rust 侧 LU 机制未建模 → 诚实 ENOSYS。
            SefEvent::Init(init_type) => {
                let result = match init_type {
                    0 | 2 => 0, // SEF_INIT_FRESH / SEF_INIT_RESTART — sef.h:93-95
                    _ => minix_types::ENOSYS,
                };
                if result == 0 {
                    state.init_fresh();
                }
                send_reply(Endpoint::RS, minix_sef::sef_init_reply(result));
            }
            SefEvent::PingInvalid => {}
        }
    }
}

/// `EndpointDirectory` 的生产实现：读 `VfsState::driver_labels`。
///
/// C 的对应物是 `ds_retrieve_label_endpt`（dmap.c:148-152）——一次 **DS 往返**。
/// 单线程模型里那需要新的路由面（DS 回复的落槽与续接），本批没接；所以这里先
/// 用本地表，**表由谁填**（DS 事件里带标签时写入 / 或把 DS 往返接起来）是登记
/// 在案的缺口。空表语义与 C 的"标签未知"一致：EINVAL。
struct LabelDir<'a>(&'a [(alloc::string::String, Endpoint)]);

impl crate::device_map::EndpointDirectory for LabelDir<'_> {
    fn lookup(&self, label: &str) -> Option<Endpoint> {
        self.0
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, e)| *e)
    }
}

/// `FsCtl` 的**队列收集**实现：`VnodeTable::put` 慢路径（`ref==1` →
/// `req_putnode`）与 `clean_refs`（`fs_count > 256` 的批量归还）把通知
/// 收进缓冲，调用方再并入 [`VfsState::pending_puts`] 统一投递。
struct PutNodeSink<'a>(&'a mut alloc::vec::Vec<crate::vnode::PutNodeReq>);

impl crate::vnode::FsCtl for PutNodeSink<'_> {
    fn put_node(
        &mut self,
        fs: Endpoint,
        ino: u64,
        count: usize,
    ) -> Result<(), crate::vnode::VnodeError> {
        self.0.push(crate::vnode::PutNodeReq { fs_e: fs, ino, count });
        Ok(())
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
            Some((user, -minix_types::ENOTDIR))
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
            Some((user, -minix_types::EEXIST))
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

    /// `sync`/`fsync` 的多挂载序列（C `do_sync`/`do_fsync` 的循环）：
    /// 目标过滤（`dev != NO_DEV && fs != NONE && root != NULL`，fsync 再按
    /// 设备号收窄）→ 逐条发 `REQ_SYNC` → 发完报 `first_err`（C 丢掉单条的
    /// 返回值，只留加锁错误）。
    #[test]
    fn test_sync_sequence_over_mounts() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        // 行 0：有效（MFS、dev 1、有根）。行 1：另一个 FS、dev 2。行 2：缺根
        // ——必须被过滤掉。
        let vid = state.vnode_table.find_by_ino(Endpoint::MFS, 1).unwrap();
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
            v.root = Some(vid.get());
        }
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
            v.fs = Endpoint::from_generation_slot(0, 7);
            v.dev = 2;
            v.root = Some(vid.get());
        }
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(2)).unwrap();
            v.fs = Endpoint::from_generation_slot(0, 8);
            v.dev = 3;
            v.root = None; // 没根 → 不过滤进来
        }
        // 全量：两行（行 0 与行 1）。
        assert_eq!(
            state.sync_targets(None),
            alloc::vec![Endpoint::MFS, Endpoint::from_generation_slot(0, 7)]
        );
        // 按设备收窄：只要 dev 2 那一行（fsync 的形态）。
        assert_eq!(
            state.sync_targets(Some(2)),
            alloc::vec![Endpoint::from_generation_slot(0, 7)]
        );

        // 序列跑起来：第一条 REQ_SYNC 已登记，现场带两条目标。
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
        state.begin_sync_sequence(idx, Some(slot), None).unwrap();
        let p = state.pending_fs.as_ref().expect("第一条 REQ_SYNC");
        assert_eq!(p.req.m_type, minix_types::REQ_SYNC);
        assert_eq!(p.fs_e, Endpoint::MFS, "先发列表里的第一个");
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().cont,
            Some(WorkerCont::SyncMounts { count: 2, at: 1, .. })
        ));

        // 第一条回复到达 → 发第二条。
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("第二条 REQ_SYNC");
        assert_eq!(p.req.m_type, minix_types::REQ_SYNC);
        assert_eq!(p.fs_e, Endpoint::from_generation_slot(0, 7));
        assert!(state.take_reply().is_none(), "序列没跑完不回用户");

        // 第二条回复到达 → 序列结束，回 0 并释放槽。
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

    /// `rename` 的三段链（C `do_rename` link.c:166-280）：阶段 1 走旧父目录
    /// →（旧父目录带粘滞位时插一段子遍历取受害者属主）→ 阶段 2 走新父目录 →
    /// 跨设备门 + `W|X` 门 + `REQ_RENAME`（**两个 direct grant**，旧名是阶段 1
    /// 保存下来的）。
    #[test]
    fn test_path_follow_rename_chain_and_request() {
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
                crate::path::Lookup::new("/a".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                0,
                0,
            )
            .unwrap();
            (idx, walk)
        };
        let done_reply = |ino: u64, mode: u32, uid: u32| {
            let mut reply = Message { m_type: minix_types::OK, ..Message::default() };
            // SAFETY(test): 按 lookup_reply_off 填 ino/mode/uid/gid。
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&ino.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&mode.to_le_bytes());
                raw[minix_types::lookup_reply_off::UID..minix_types::lookup_reply_off::UID + 4]
                    .copy_from_slice(&uid.to_le_bytes());
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
        let clear_slot = |state: &mut VfsState, idx: usize| {
            {
                let wp = state.worker_pool.get_mut(idx).unwrap();
                wp.cont = None;
                wp.path = None;
                wp.sendrec = None;
                wp.task = None;
            }
            state.worker_pool.release(idx);
            state.pending_fs = None;
        };

        // ① 阶段 1 走通、旧父目录无粘滞位 → 直接转阶段 2（旧名已保存）。
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(0x11, crate::open::S_IFDIR | 0o755, 0),
            PathFollow::RenameOld {
                entry: "old".to_string(),
                new_path: "/b/new".to_string(),
            },
        );
        state.run_worker_continuations();
        assert!(state.take_reply().is_none(), "转场不回用户");
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().path.as_ref().map(|p| &p.follow),
            Some(PathFollow::RenameNew { old_name, new_entry, .. })
                if old_name == "old" && new_entry == "new"
        ));
        assert_eq!(
            state.pending_fs.as_ref().map(|p| p.req.m_type),
            Some(minix_types::REQ_LOOKUP),
            "阶段 2 起走新父目录"
        );
        clear_slot(&mut state, idx);

        // ② 旧父目录带粘滞位 → 先插一段子遍历。
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(0x11, crate::open::S_IFDIR | 0o755 | crate::open::S_ISVTX, 0),
            PathFollow::RenameOld {
                entry: "old".to_string(),
                new_path: "/b/new".to_string(),
            },
        );
        state.run_worker_continuations();
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().path.as_ref().map(|p| &p.follow),
            Some(PathFollow::RenameOldSticky { .. })
        ));
        assert_eq!(
            state.pending_fs.as_ref().map(|p| p.req.m_type),
            Some(minix_types::REQ_LOOKUP)
        );
        clear_slot(&mut state, idx);

        // ③ 子遍历走通：受害者属主不是调用方 → EPERM。
        let (idx, walk) = mk(&mut state, 2000);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(0x33, crate::open::S_IFREG | 0o644, 1000),
            PathFollow::RenameOldSticky {
                entry: "old".to_string(),
                new_path: "/b/new".to_string(),
            },
        );
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EPERM))
        );

        // ④ 阶段 2：跨设备 → EXDEV。
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        {
            let start = crate::path::LookupStart {
                fs: Endpoint::from_generation_slot(0, 9),
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir { ino: 1, fs: Endpoint::MFS, dev: 0 };
            let (walk2, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/b".to_string(), crate::path::LookupFlags::NOFLAGS)
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
                done_reply(0x22, crate::open::S_IFDIR | 0o755, 0),
                PathFollow::RenameNew {
                    old_fs_e: Endpoint::MFS,
                    old_ino: 0x11,
                    old_name: "old".to_string(),
                    new_entry: "new".to_string(),
                },
            );
            let _ = walk;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EXDEV))
        );

        // ⑤ 阶段 2：非属主对新父目录无写权 → EACCES。
        let (idx, walk) = mk(&mut state, 2000);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(0x22, crate::open::S_IFDIR | 0o755, 0),
            PathFollow::RenameNew {
                old_fs_e: Endpoint::MFS,
                old_ino: 0x11,
                old_name: "old".to_string(),
                new_entry: "new".to_string(),
            },
        );
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EACCES))
        );

        // ⑥ 全过 → REQ_RENAME（两个 direct grant + 两个名字长度）+ 状态回复。
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(0x22, crate::open::S_IFDIR | 0o755, 0),
            PathFollow::RenameNew {
                old_fs_e: Endpoint::MFS,
                old_ino: 0x11,
                old_name: "old".to_string(),
                new_entry: "new".to_string(),
            },
        );
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_RENAME");
        assert_eq!(p.req.m_type, minix_types::REQ_RENAME);
        // SAFETY(test): 按 rename_req_off 读回六域。
        unsafe {
            let raw = &p.req.m_u.raw;
            let dir_old = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let dir_new = u64::from_le_bytes(raw[8..16].try_into().unwrap());
            let len_old = u64::from_le_bytes(raw[16..24].try_into().unwrap());
            let len_new = u64::from_le_bytes(raw[24..32].try_into().unwrap());
            let grant_old = i32::from_le_bytes(raw[32..36].try_into().unwrap());
            let grant_new = i32::from_le_bytes(raw[36..40].try_into().unwrap());
            assert_eq!(dir_old, 0x11, "dir_old 是旧父目录（阶段 1 记下的）");
            assert_eq!(dir_new, 0x22, "dir_new 是新父目录");
            assert_eq!(len_old, 4, "旧名含 NUL（\"old\" → 4 字节）");
            assert_eq!(len_new, 4, "新名含 NUL（\"new\" → 4 字节）");
            assert_ne!(grant_old, grant_new, "两个名字各一张 grant");
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
            Some((user, -minix_types::EXDEV))
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
            Some((user, -minix_types::EACCES))
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
            Some((user, -minix_types::EACCES))
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

    /// `intern_vnode` 的**设备号归属**（C `advance` path.c:98-106）：
    /// `v_sdev = res.dev`（特殊设备号，设备节点靠它认驱动）、
    /// `v_dev = vmp->m_dev`（挂载分区的设备号）。两者是不同字段——先前把
    /// `res.dev` 写进 `v_dev` 是错的（`v_sdev` 一直为 0，`cdev_get` 那类按
    /// `v_sdev` 找驱动的地方就都找不到）。
    #[test]
    fn test_intern_vnode_splits_device_numbers() {
        use minix_types::Endpoint;
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        // 挂载行的设备号是 1（`seed_vmnt0` 播的）；节点带特殊设备号 0x0507。
        let node = crate::path::NodeDetails {
            fs_e: Endpoint::MFS,
            ino: 0x42,
            mode: crate::open::S_IFCHR | 0o644,
            size: 0,
            uid: 0,
            gid: 0,
            dev: 0x0507,
        };
        let idx = state.intern_vnode(&node).expect("并表");
        let v = state.vnode_table.get(crate::vnode::VnodeId(idx)).unwrap();
        assert_eq!(v.sdev, 0x0507, "v_sdev 是节点带回来的特殊设备号");
        assert_eq!(v.dev, 1, "v_dev 是挂载行的设备号");
    }

    /// Open 的字符设备分支（`common_open` 的 `S_IFCHR` 支）：`/dev/tty` 例外
    /// 的判定键是**原始 major**——有控制终端就直接成功、不打扰真实 tty 驱动
    /// （C `cdev.c:174`）；无控制终端 → ENXIO；普通设备无驱动 → ENXIO；
    /// 有驱动而宿主发不出去 → EIO，且**认领全部放开**（C `common_open`
    /// 失败尾的 `put_vnode`——不放开就漏 fd 与 vnode 引用）。
    #[test]
    fn test_open_char_device_branch() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let tty_dev = 0x0401u64; // 控制终端：major 4 / minor 1

        // (a) `/dev/tty`（major 5）+ 有控制终端：直接成功，无驱动请求。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.tty = tty_dev;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let node = crate::path::NodeDetails {
            fs_e: Endpoint::MFS,
            ino: 7,
            mode: crate::open::S_IFCHR | 0o600,
            size: 0,
            uid: 0,
            gid: 0,
            dev: (crate::device_map::CTTY_MAJOR as u64) << 8, // makedev(5, 0)
        };
        state.finish_open_local(idx, Some(slot), &node, crate::open::O_RDONLY);
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0)),
            "CTTY 例外直接回 fd（这里是 0 号 fd），不回错误"
        );
        assert!(
            state.worker_pool.get(idx).unwrap().cont.is_none(),
            "例外路径不挂任何驱动续接"
        );
        assert!(
            state.fproc_table.get(slot).unwrap().filps[0].is_some(),
            "fd 仍被认领"
        );

        // (b) `/dev/tty` 但没有控制终端：ENXIO（C `cdev_map` → NO_DEV）。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            // fp.tty 默认 NO_DEV
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        state.finish_open_local(idx, Some(slot), &node, crate::open::O_RDONLY);
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENXIO))
        );

        // (c) 普通字符设备（major 4）但 dmap 行没有驱动：ENXIO。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let plain = crate::path::NodeDetails { dev: 0x0405, ..node };
        state.finish_open_local(idx, Some(slot), &plain, crate::open::O_RDONLY);
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENXIO))
        );

        // (d) 有驱动而宿主发不出去：EIO，且 fd/filp/vnode 全部放开
        // （vnode 放回后 ref_count 归 0，槽可复用——C `common_open` 失败尾）。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let drv = Endpoint::from_generation_slot(0, 11);
        state.dmap_table.get_mut(4).unwrap().driver = Some(drv);
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        state.finish_open_local(idx, Some(slot), &plain, crate::open::O_RDONLY);
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EIO))
        );
        assert!(state.fproc_table.get(slot).unwrap().filps[0].is_none());
        assert!(
            state.vnode_table.find_by_ino(Endpoint::MFS, 7).is_none()
                || state
                    .vnode_table
                    .find_by_ino(Endpoint::MFS, 7)
                    .and_then(|id| state.vnode_table.get(id))
                    .map(|v| v.ref_count == 0)
                    .unwrap_or(true),
            "放回 vnode 后引用归零"
        );
    }

    /// Open 的块设备分支（`common_open` 的 `S_IFBLK` 支）：无驱动 → ENXIO；
    /// 驱动放行后选 `v_bfs_e`（被挂载占着就用那个挂载的 FS，不再发
    /// `REQ_NEW_DRIVER`）；设备空闲时 `v_bfs_e` 归根文件系统——根挂载未接
    /// （`root_fs_e` 为 NONE）→ 按 C 的 newdriver 失败路径收尾（ENXIO）。
    #[test]
    fn test_open_block_device_branch() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let slot = minix_types::UserSlot::new(0);
        let drv = Endpoint::from_generation_slot(0, 12);
        let dev = 0x0301u64; // major 3 / minor 1
        let node = crate::path::NodeDetails {
            fs_e: Endpoint::MFS,
            ino: 9,
            mode: crate::open::S_IFBLK | 0o600,
            size: 0,
            uid: 0,
            gid: 0,
            dev,
        };

        // (a) dmap 行没有驱动：ENXIO（C `bdev_open:88-89` 的门）。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        state.finish_open_local(idx, Some(slot), &node, crate::open::O_RDONLY);
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENXIO))
        );

        // (b) 有驱动而宿主发不出去：EIO + 认领放开。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        state.dmap_table.get_mut(3).unwrap().driver = Some(drv);
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        state.finish_open_local(idx, Some(slot), &node, crate::open::O_RDONLY);
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EIO))
        );
        assert!(state.fproc_table.get(slot).unwrap().filps[0].is_none());

        // (c) 驱动放行 + 设备被挂载占着：`v_bfs_e` 用那个挂载的 FS，直接回
        // fd（C `open.c:208-210`：不是根文件系统就不再发 newdriver）。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        state.dmap_table.get_mut(3).unwrap().driver = Some(drv);
        let holder_fs = Endpoint::from_generation_slot(0, 9);
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
            v.fs = holder_fs;
            v.dev = dev;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let vnode_idx = state.intern_vnode(&node).unwrap();
        let fid = state
            .filp_table
            .alloc_filp(crate::open::R_BIT)
            .unwrap();
        state.filp_table.inc_count(fid);
        state.fproc_table.get_mut(slot).unwrap().filps[0] = Some(fid.get());
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vnode_idx);
        let mut reply = Message { m_type: 0x580, ..Message::default() }; // BDEV_REPLY
        // SAFETY(test): `mess_lblockdriver_lbdev_reply { int status; int id; }`
        // ——状态在首字，OK 为 0。
        unsafe {
            reply.m_u.raw[0..4].copy_from_slice(&0i32.to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::BdevOpen {
                fd: 0,
                filp: fid.get(),
                vnode: vnode_idx,
                dev,
                minor: 1,
                access: crate::bdev::BDEV_R_BIT as u8,
                retries: 0,
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0)),
            "成功回 fd（0 号）"
        );
        assert_eq!(
            state
                .vnode_table
                .get(crate::vnode::VnodeId(vnode_idx))
                .unwrap()
                .bfs,
            holder_fs,
            "v_bfs_e 用占着设备的挂载行的 FS"
        );

        // (d) 设备空闲、根挂载未接（`root_fs_e` = NONE）：按 C 的 newdriver
        // 失败路径收尾——放开认领并回 ENXIO（不假装通知成功）。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        state.dmap_table.get_mut(3).unwrap().driver = Some(drv);
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let vnode_idx = state.intern_vnode(&node).unwrap();
        let fid = state
            .filp_table
            .alloc_filp(crate::open::R_BIT)
            .unwrap();
        state.filp_table.inc_count(fid);
        state.fproc_table.get_mut(slot).unwrap().filps[0] = Some(fid.get());
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vnode_idx);
        let reply = Message { m_type: 0x580, ..Message::default() };
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::BdevOpen {
                fd: 0,
                filp: fid.get(),
                vnode: vnode_idx,
                dev,
                minor: 1,
                access: crate::bdev::BDEV_R_BIT as u8,
                retries: 0,
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENXIO)),
            "根挂载未接：newdriver 无处可发 → C 的失败路径"
        );
        assert!(state.fproc_table.get(slot).unwrap().filps[0].is_none());

        // (e) 驱动回 ERESTART：按 `bdev_sendrec` 的保险丝重发；宿主发不出 →
        // EIO 收尾（不悬挂、认领放开）。
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        state.dmap_table.get_mut(3).unwrap().driver = Some(drv);
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let vnode_idx = state.intern_vnode(&node).unwrap();
        let fid = state
            .filp_table
            .alloc_filp(crate::open::R_BIT)
            .unwrap();
        state.filp_table.inc_count(fid);
        state.fproc_table.get_mut(slot).unwrap().filps[0] = Some(fid.get());
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vnode_idx);
        let mut reply = Message { m_type: 0x580, ..Message::default() };
        // SAFETY(test): ERESTART 在载荷首字（线上是负值）。
        unsafe {
            reply.m_u.raw[0..4]
                .copy_from_slice(&(-minix_types::ERESTART).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::BdevOpen {
                fd: 0,
                filp: fid.get(),
                vnode: vnode_idx,
                dev,
                minor: 1,
                access: crate::bdev::BDEV_R_BIT as u8,
                retries: 0,
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EIO))
        );
        assert!(state.fproc_table.get(slot).unwrap().filps[0].is_none());
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
            Some((user, -minix_types::ENOTDIR))
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
            Some((user, -minix_types::EACCES))
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
            Some((user, -minix_types::EACCES))
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
            Some((user, -minix_types::ENOTDIR))
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
            Some((user, -minix_types::EACCES))
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
            Some((user, -minix_types::EPERM))
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
            Some((user, -minix_types::EPERM))
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
            Some((user, -minix_types::EROFS))
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
            Some((user, -minix_types::EINVAL))
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
            Some((user, -minix_types::ENOENT))
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
            -minix_types::EACCES,
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
            -minix_types::EROFS
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
            Some((user, -minix_types::EACCES))
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
            Some((user, -minix_types::EIO)),
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
        // FS 报错：位置不动，错误原样回用户（FS 的线上状态是负值）。
        assert_eq!(
            run(&mut state, -minix_types::ENOTDIR, 0x200, 12),
            -minix_types::ENOTDIR
        );
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
            // FS 回复的线上状态带负号（服务端都是 `_SYSTEM` 构建）。
            wp.sendrec = Some(Message {
                m_type: -(minix_types::ERESTART),
                ..Message::default()
            });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EIO))
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
            Some((user, -minix_types::EINVAL)),
            "错误码在边界折成负号（C 的 _SYSTEM 构建里常量本身就带负号）"
        );

        state.queue_reply(user, SyscallResult::Nosys);
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENOSYS))
        );

        // 驱动/FS 带来的状态已经是负值：**不二次取负**。
        state.queue_reply(user, SyscallResult::Error(-minix_types::ENXIO));
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENXIO))
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
        seed_imgrd_driver(&mut state);
        let ipc = BootScriptedIpc::default();
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        state.finish_init(&kernel, &ipc).unwrap();

        assert!(state.initialized);
        assert_eq!(state.boot_phase, BootPhase::Running);
        assert!(state.accept_requests);
        assert_eq!(state.root_fs_e, Endpoint::MFS);
        assert_eq!(state.root_dev, DEV_IMGRD);
        assert_eq!(state.have_root, 1);
    }

    #[test]
    #[should_panic(expected = "finish_init requires completed VFS_PM_INIT handshake")]
    fn test_finish_init_requires_handshake() {
        let mut state = VfsState::new();
        state.init_fresh();
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        let ipc = BootScriptedIpc::default();
        state.finish_init(&kernel, &ipc);
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
        seed_imgrd_driver(&mut state);

        let ipc = BootScriptedIpc::default();
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        state.do_init_root(&kernel, &ipc).unwrap();
        assert_eq!(state.boot_phase, BootPhase::Running);
        assert!(state.accept_requests);
        assert_eq!(state.root_fs_e, Endpoint::MFS);
    }

    // ── NS4/W6 根挂载编排 ────────────────────────────────────────────

    /// 给 dmap 播种 memory 驱动行（DEV_IMGRD 的 major 归属，C boot 链上
    /// 由 rproctab/map_service 装配——E-RPROCTAB 消费半，这里测试直填）。
    fn seed_imgrd_driver(state: &mut VfsState) {
        let mut entry = crate::device_map::DmapEntry::empty();
        entry.driver = Some(Endpoint::MEM);
        entry.label[..7].copy_from_slice(b"memory\0");
        assert!(state.dmap_table.set(crate::device_map::MEMORY_MAJOR, entry));
    }

    /// 根 readsuper 的脚本回复（ipc.h:198-211 布局：file_size@0、device@8、
    /// inode@16、flags@24、mode@28、uid@32、gid@36、con_reqs@40）。
    fn readsuper_reply(ino: u64, mode: u32, size: u64) -> Message {
        let mut m = Message::default();
        {
            // SAFETY: 按 request.rs:1140 解码器的既有偏移写字节面。
            let raw = unsafe { &mut m.m_u.raw };
            raw[0..8].copy_from_slice(&size.to_le_bytes());
            raw[16..24].copy_from_slice(&ino.to_le_bytes());
            raw[24..28].copy_from_slice(&0u32.to_le_bytes());
            raw[28..32].copy_from_slice(&mode.to_le_bytes());
            raw[32..36].copy_from_slice(&0u32.to_le_bytes());
            raw[36..40].copy_from_slice(&0u32.to_le_bytes());
        }
        m
    }

    /// boot 段脚本 IPC：`sendrec` 依次弹出回复，空脚本给默认成功回复
    /// （目录根节点、非 threaded）。
    struct BootScriptedIpc {
        queue: core::cell::RefCell<alloc::collections::VecDeque<Option<Message>>>,
        /// (目的地, 发出的请求) 按序记录。
        pub sent: core::cell::RefCell<alloc::vec::Vec<(Endpoint, Message)>>,
        /// 非 None 时下一次 sendrec 报这个正 errno（TrapStatus 契约）。
        pub fail_next: core::cell::Cell<Option<i32>>,
    }

    impl Default for BootScriptedIpc {
        fn default() -> Self {
            Self {
                queue: core::cell::RefCell::new(alloc::collections::VecDeque::new()),
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                fail_next: core::cell::Cell::new(None),
            }
        }
    }

    impl BootScriptedIpc {
        fn push(&self, m: Option<Message>) {
            self.queue.borrow_mut().push_back(m);
        }
    }

    impl minix_sys::ipc::IpcTransport for BootScriptedIpc {
        fn send(
            &self,
            _d: Endpoint,
            _m: &Message,
        ) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn receive(
            &self,
            _s: Endpoint,
            _m: &mut Message,
        ) -> Result<minix_sys::ipc::IpcStatus, minix_sys::ipc::TrapStatus> {
            unimplemented!("boot 段不收消息")
        }
        fn sendrec(
            &self,
            destination: Endpoint,
            message: &mut Message,
        ) -> Result<(), minix_sys::ipc::TrapStatus> {
            self.sent.borrow_mut().push((destination, message.clone()));
            if let Some(e) = self.fail_next.take() {
                return Err(minix_sys::ipc::TrapStatus(e));
            }
            match self.queue.borrow_mut().pop_front() {
                Some(Some(m)) => *message = m,
                _ => *message = readsuper_reply(1, crate::open::S_IFDIR | 0o755, 64),
            }
            Ok(())
        }
        fn notify(&self, _d: Endpoint) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn sendnb(
            &self,
            _d: Endpoint,
            _m: &Message,
        ) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn senda(
            &self,
            _t: &[minix_sys::ipc::AsyncSlot],
        ) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn query_kerninfo_page(&self) -> Result<u64, minix_sys::ipc::TrapStatus> {
            Err(minix_sys::ipc::TrapStatus(minix_types::ENOSYS))
        }
    }

    /// boot 前置：一个 live 进程槽（槽 0，供 MAKEROOT 断言）+ 握手终止
    /// + dmap 驱动行（根挂载的脚本环境）。
    fn boot_ready_state() -> (VfsState, BootScriptedIpc) {
        let mut state = VfsState::new();
        state.init_fresh();
        let first =
            VfsPmInit { slot: 0, pid: 8, endpoint: Endpoint::from_generation_slot(1, 0) }
                .encode();
        state.pm_handshake_step(&first).unwrap();
        let terminator = VfsPmInit { slot: 0, pid: 0, endpoint: Endpoint::NONE }.encode();
        state.pm_handshake_step(&terminator).unwrap();
        seed_imgrd_driver(&mut state);
        (state, BootScriptedIpc::default())
    }

    #[test]
    fn test_root_mount_full_assembly() {
        let (mut state, ipc) = boot_ready_state();
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        // 脚本序:PFS 先答（明细按 C 忽略），MFS 后答（根 inode 1、目录、
        // 串行窗口 = 1——flags@24 零即无 RES_THREADED）。
        ipc.push(Some(readsuper_reply(1, crate::open::S_IFDIR | 0o755, 32)));
        ipc.push(Some(readsuper_reply(1, crate::open::S_IFDIR | 0o755, 4096)));

        state.do_init_root(&kernel, &ipc).unwrap();

        // 赋值面（mount.c:326-328）。
        assert_eq!(state.root_fs_e, Endpoint::MFS);
        assert_eq!(state.root_dev, DEV_IMGRD);
        assert_eq!(state.have_root, 1);

        // vmnt 行：标签/路径/设备 + CANSTAT + 窗口（mount.c:297-305/318-325）。
        let id = state.vmnt_table.find_by_dev(DEV_IMGRD).expect("root vmnt row");
        let v = state.vmnt_table.get(id).unwrap();
        assert_eq!(v.fs, Endpoint::MFS);
        assert_eq!(v.dev, DEV_IMGRD);
        assert!(v.flags.contains(VmntFlags::CANSTAT));
        assert!(!v.flags.contains(VmntFlags::MOUNTING));
        assert_eq!(v.label, "fs_imgrd");
        assert_eq!(v.mount_path, "/");
        assert_eq!(v.fstype, "mfs");
        assert_eq!(v.mounted_on, None);
        assert_eq!(state.comm.vmnts[id.get()].max_reqs, 1);

        // 根 vnode 七连填（mount.c:283-296）。
        let root_idx = v.root.expect("root vnode set");
        let vn = state.vnode_table.get(crate::vnode::VnodeId(root_idx)).unwrap();
        assert_eq!(vn.ino, 1);
        assert_eq!(vn.mode, crate::open::S_IFDIR | 0o755);
        assert_eq!(vn.size, 4096);
        assert_eq!(vn.ref_count, 1);
        assert_eq!(vn.fs_count, 1);
        assert_eq!(vn.sdev, NO_DEV);
        assert_eq!(vn.vmnt, Some(crate::vnode::VmntId(id.get())));
        assert_eq!(vn.dev, DEV_IMGRD);

        // MAKEROOT：live fproc 槽（槽 0 已被握手播种）rd/wd 归根。
        let fp = state.fproc_table.get(UserSlot::new(0)).unwrap();
        assert_eq!(fp.root_dir, Some(root_idx));
        assert_eq!(fp.work_dir, Some(root_idx));
    }

    #[test]
    fn test_root_mount_without_driver_is_inval_and_gate_stays_closed() {
        let (mut state, ipc) = boot_ready_state();
        // 拔掉 dmap 行：boot 装配半（rproctab 消费）缺位时的诚实失败。
        state
            .dmap_table
            .set(crate::device_map::MEMORY_MAJOR, crate::device_map::DmapEntry::empty());
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();

        let r = state.do_init_root(&kernel, &ipc);
        assert!(matches!(r, Err(MountError::Inval)));
        // C panic 前 worker 门不复位；root 未赋值。
        assert!(!state.accept_requests);
        assert_eq!(state.root_fs_e, Endpoint::NONE);
        assert_eq!(state.have_root, 0);
    }

    #[test]
    fn test_pfs_readsuper_failure_tolerated() {
        let (mut state, ipc) = boot_ready_state();
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        // 第一发（PFS）失败，第二发（MFS）走默认回复。
        ipc.fail_next.set(Some(minix_types::EIO));

        state.do_init_root(&kernel, &ipc).unwrap();
        assert_eq!(state.have_root, 1);
        assert_eq!(state.root_fs_e, Endpoint::MFS);
        // PFS 行仍站着（C：printf 后挂载照旧），fs_flags 未回填。
        let pfs = state.vmnt_table.find_by_fs(Endpoint::PFS).expect("pfs row stands");
        assert_eq!(state.vmnt_table.get(pfs).unwrap().fs_flags, 0);
    }

    #[test]
    fn test_root_mount_wire_shape() {
        let (mut state, ipc) = boot_ready_state();
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        state.do_init_root(&kernel, &ipc).unwrap();

        let sent = ipc.sent.borrow();
        assert_eq!(sent.len(), 2, "PFS + MFS 各一次 readsuper");
        let (pfs_dst, pfs_req) = &sent[0];
        let (mfs_dst, mfs_req) = &sent[1];
        assert_eq!(*pfs_dst, Endpoint::PFS);
        assert_eq!(*mfs_dst, Endpoint::MFS);
        assert_eq!(pfs_req.m_type, minix_types::REQ_READSUPER as i32);
        assert_eq!(mfs_req.m_type, minix_types::REQ_READSUPER as i32);

        // 线上形状（request.c:780-813）：device@0、flags@8、path_len@16、grant@24。
        let raw = unsafe { &mfs_req.m_u.raw };
        let dev = u64::from_le_bytes(raw[0..8].try_into().unwrap());
        let flags = u32::from_le_bytes(raw[8..12].try_into().unwrap());
        let path_len = u64::from_le_bytes(raw[16..24].try_into().unwrap());
        let grant = i32::from_le_bytes(raw[24..28].try_into().unwrap());
        assert_eq!(dev, DEV_IMGRD);
        assert_ne!(flags & crate::request::REQ_ISROOT, 0, "根挂载带 REQ_ISROOT");
        // label "memory\0" = 7 字节（dmap 行播种的驱动标签）。
        assert_eq!(path_len, 7);
        assert!(grant >= 0, "grant 已建（CPF_READ 直授权）");
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
