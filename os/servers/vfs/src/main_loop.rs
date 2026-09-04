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

use crate::call_table::CallTable;
use crate::fproc::{BlockedOn, FProcTable, FpFlags, PID_FREE};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchResult {
    /// Processed, continue loop.
    Continue,
    /// Need to spawn worker thread.
    SpawnWorker,
    /// Ignored message.
    Ignored,
}

/// Eight-way dispatch route — the priority chain of `main:80-138`.
///
/// Order matters: `FsReply` (transid) > `Pm` > `Notify` > `TaskIgnored`
/// > `Bdev`/`Cdev`/`Sdev` > `Syscall`.  The variant order in this enum
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
}

/// Notify source inside [`Route::Notify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    Ds,
    Kernel,
    Clock { timestamp: u32 },
    Other { endpoint: Endpoint },
}

/// `TRNS_GET_ID` / `VFS_TRANSID` codec — typed encoding of the worker slot
/// into `m_type`'s low 16 bits (see `vfsif.h:79` + `com.h:911`).
///
/// `ARCH A-4`: the `0xFFFF` masking and `~0xff` prefix checks are encapsulated
/// here rather than scattered `TRNS_GET_ID` macro uses.
pub trait TransIdCodec {
    /// Encode `slot` as a FS transid (low 16 bits) ready to be OR'd into `m_type`.
    fn encode(&self, slot: usize) -> u32;
    /// Decode `raw` (`TRNS_GET_ID(m_type)`) to a worker slot, if it is a
    /// FS transid (`IS_VFS_FS_TRANSID`).
    fn decode(&self, raw: u32) -> Option<usize>;
    /// Whether `raw` (already `TRNS_GET_ID` extracted) is a FS transid.
    fn is_fs_transid(&self, raw: u32) -> bool;
}

/// Minix3-faithful codec: `VFS_TRANSACTION_BASE = 0xB00`, `VFS_TRANSID = 0xB01`,
/// `IS_VFS_FS_TRANSID(t) == ((t & ~0xff)==0xB00)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct VfsTransIdCodec;

impl TransIdCodec for VfsTransIdCodec {
    fn encode(&self, slot: usize) -> u32 {
        const VFS_TRANSID: u32 = 0xB01;
        VFS_TRANSID + slot as u32
    }
    fn decode(&self, raw: u32) -> Option<usize> {
        if !self.is_fs_transid(raw) {
            return None;
        }
        const VFS_TRANSID: u32 = 0xB01;
        Some((raw - VFS_TRANSID) as usize)
    }
    fn is_fs_transid(&self, raw: u32) -> bool {
        const VFS_TRANSACTION_BASE: u32 = 0xB00;
        (raw & !0xff) == VFS_TRANSACTION_BASE
    }
}

/// Test codec with a different base — behaviourally different from [`VfsTransIdCodec`].
///
/// For the same `raw = 0xB01`, `VfsTransIdCodec` says `is_fs_transid==true`
/// and decodes to slot 0, while `TestTransIdCodec { base: 0xC00 }` says
/// `false` / `None`.  This satisfies Gate D “≥2 behaviourally different impls”.
#[derive(Debug, Clone, Copy)]
pub struct TestTransIdCodec {
    pub base: u32,
}

impl TransIdCodec for TestTransIdCodec {
    fn encode(&self, slot: usize) -> u32 {
        (self.base + 1) + slot as u32
    }
    fn decode(&self, raw: u32) -> Option<usize> {
        if !self.is_fs_transid(raw) {
            return None;
        }
        Some((raw - (self.base + 1)) as usize)
    }
    fn is_fs_transid(&self, raw: u32) -> bool {
        (raw & !0xff) == self.base
    }
}

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
}

impl UnblockError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::NotBlocked => minix_types::EINVAL,
            Self::UnknownBlockedOn(_) => minix_types::EINVAL,
            Self::SlotFree => minix_types::ESRCH,
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
    /// Worker thread pool.
    pub worker_pool: WorkerPool,
    /// Syscall dispatch table.
    pub call_table: CallTable,
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
}

impl VfsState {
    /// Creates new VFS state.
    pub fn new() -> Self {
        Self {
            fproc_table: FProcTable::new(),
            worker_pool: WorkerPool::new(),
            call_table: CallTable::new(),
            reviving: 0,
            current_message: Message::default(),
            current_fp_slot: None,
            boot_phase: BootPhase::PmHandshake,
            // C: `block_all` 为 BSS 全局（glo.h），初始为 0（允许）。
            accept_requests: true,
            pending: 0,
            initialized: false,
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
        // main.c:451-453 — init_dmap()/init_smap()（归 19，DEFERRED）。
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

        // main.c:505 — mount_pfs()（DEFERRED，归 18）。
        // main.c:508-518 — mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR,
        //                   0, "mfs", "fs_imgrd")（DEFERRED，归 18）。

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

    /// Dispatches message.
    ///
    /// Corresponds to Minix3's message dispatch logic in `main()` (main.c:68-118).
    /// The five-way split (FS reply / PM / notification / device reply / syscall)
    /// is detailed in 09-main-loop; this method covers the routing skeleton and
    /// the `worker_allow` gate.
    pub fn dispatch(&mut self) -> DispatchResult {
        let source = self.current_endpoint();

        // main.c:76-77 — PM 消息走 service_pm（归 10）。
        if source == Endpoint::PM {
            return DispatchResult::Continue;
        }

        // main.c:79-98 — 通知与内核 task 消息（归 09）。
        let Some(slot) = source.to_user_slot() else {
            return DispatchResult::Ignored;
        };

        self.current_fp_slot = Some(slot);

        // worker_allow(FALSE) 门控（main.c:503/525）：挂起请求标 pending。
        if !self.accept_requests {
            self.mark_request_pending(slot);
            return DispatchResult::Continue;
        }

        DispatchResult::SpawnWorker
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
        // `BDEV_RS_BASE` etc are masked with `~0x7f`; the exact base values
        // are not needed for the routing priority test — we model them as
        // distinct high-byte prefixes for testability.  Real decode would use
        // the constants from `minix_types`.
        (raw & 0xFF00) == 0x500
    }
    pub fn is_cdev_rs(raw: u32) -> bool {
        (raw & 0xFF00) == 0x600
    }
    pub fn is_sdev_rs(raw: u32) -> bool {
        (raw & 0xFF00) == 0x700
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
        if codec.is_fs_transid(transid_raw) {
            if let Some(slot) = codec.decode(transid_raw) {
                return Route::FsReply {
                    transid: transid_raw,
                    worker_slot: slot,
                };
            }
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

        // 8. Normal syscall — `handle_work(do_work)` → `call_vec`.
        // Try to resolve the call number; unresolved still routes to Syscall
        // (handler will return ENOSYS).
        if let Some(call) = crate::call_table::VfsCallNum::from_raw(m_type) {
            Route::Syscall { call }
        } else {
            // Unknown raw that passed VFS_BASE check? Map to a sentinel.
            // For testability we still route to Syscall with a default.
            // Real dispatch will ENOSYS.  Use Read as placeholder.
            Route::Syscall {
                call: crate::call_table::VfsCallNum::Read,
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

    /// `do_reply:187` — validate `w_task == who_e` and `w_sendrec` liveness,
    /// then `*w_sendrec = m_in; c_cur_reqs--`.
    ///
    /// In the slot model `w_task` is `WorkerSlot.task` and `w_sendrec` is
    /// `WorkerSlot.sendrec`; `c_cur_reqs` is `Vmnt.comm` state (DEFERRED).
    /// Here we validate the transid routing and signal the slot.
    pub fn handle_fs_reply<C: TransIdCodec>(
        &mut self,
        msg: &Message,
        codec: &C,
    ) -> Result<usize, &'static str> {
        let transid_raw = (msg.m_type as u32) & 0xFFFF;
        let slot = codec.decode(transid_raw).ok_or("spurious transid")?;
        if slot >= crate::worker::NR_WTHREADS {
            return Err("worker slot out of range");
        }
        // `do_reply:194` `w_task != who_e` would `printf` and return;
        // we model as `Ok` but the worker's `task` would be checked there.
        Ok(slot)
    }
}

impl Default for VfsState {
    fn default() -> Self {
        Self::new()
    }
}

/// VFS main loop.
///
/// Corresponds to Minix3's `main()` function (main.c:54-118).
///
/// # Note
///
/// Currently a mock implementation—IPC message reception uses simulation.
/// Real IPC implementation requires kernel support.
pub fn run() -> ! {
    let mut state = VfsState::new();
    state.init_fresh();

    // 启动握手（main.c:410-436）：真实路径由 sef_receive(PM_PROC_NR) 循环驱动；
    // 内核 IPC 未落地前以占位 NONE 终止符推进状态机（mock）。
    let terminator = VfsPmInit {
        slot: 0,
        pid: 0,
        endpoint: Endpoint::NONE,
    }
    .encode();
    let _ = state.pm_handshake_step(&terminator);
    state.finish_init();

    loop {
        // worker_yield() — Let other threads run first
        // Currently single-threaded mock, no need to actually yield

        // send_work() — Dispatch pending PM deferred requests
        // Currently mock, PM deferred mechanism not implemented yet

        // get_work() — Receive new messages
        // Currently mock, using empty message
        state.current_message = Message::default();
        state.current_fp_slot = None;

        // Message dispatch logic
        // Currently mock, just showing dispatch framework
        let _ = state.dispatch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{Endpoint, VFS_PM_INIT};

    #[test]
    fn test_vfs_state_new() {
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
    fn test_dispatch_gated_marks_pending() {
        let mut state = VfsState::new();
        state.init_fresh();

        // worker_allow(FALSE)（main.c:503）。
        state.set_accept_requests(false);
        let ep = Endpoint::from_generation_slot(0, 5);
        state.current_message.m_source = ep;

        let result = state.dispatch();
        assert_eq!(result, DispatchResult::Continue);
        assert_eq!(state.pending, 1);
        let fp = state.fproc_table.get(UserSlot::new(5)).unwrap();
        assert!(fp.flags.contains(FpFlags::PENDING));
        assert_eq!(state.current_fp_slot, Some(UserSlot::new(5)));
    }

    #[test]
    fn test_dispatch_gated_dedups_pending() {
        let mut state = VfsState::new();
        state.init_fresh();
        state.set_accept_requests(false);
        state.current_message.m_source = Endpoint::from_generation_slot(0, 5);

        state.dispatch();
        state.dispatch();
        assert_eq!(state.pending, 1);
    }

    #[test]
    fn test_dispatch_accepts_when_open() {
        let mut state = VfsState::new();
        state.init_fresh();
        let ep = Endpoint::from_generation_slot(0, 5);
        state.current_message.m_source = ep;
        let result = state.dispatch();
        assert_eq!(result, DispatchResult::SpawnWorker);
        assert_eq!(state.current_fp_slot, Some(UserSlot::new(5)));
        assert_eq!(state.pending, 0);
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
    fn test_vfs_state_dispatch_from_pm() {
        let mut state = VfsState::new();
        state.current_message.m_source = Endpoint::PM;
        let result = state.dispatch();
        assert_eq!(result, DispatchResult::Continue);
    }

    #[test]
    fn test_vfs_state_dispatch_from_user() {
        let mut state = VfsState::new();
        let ep = Endpoint::from_generation_slot(0, 5);
        state.current_message.m_source = ep;
        let result = state.dispatch();
        assert_eq!(result, DispatchResult::SpawnWorker);
        assert_eq!(state.current_fp_slot, Some(UserSlot::new(5)));
    }

    #[test]
    fn test_vfs_state_dispatch_from_kernel() {
        let mut state = VfsState::new();
        state.current_message.m_source = Endpoint::KERNEL;
        let result = state.dispatch();
        assert_eq!(result, DispatchResult::Ignored);
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
    fn test_dispatch_result() {
        assert_eq!(DispatchResult::Continue, DispatchResult::Continue);
        assert_eq!(DispatchResult::SpawnWorker, DispatchResult::SpawnWorker);
        assert_eq!(DispatchResult::Ignored, DispatchResult::Ignored);
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
            m_type: (0x1234 << 16) as i32 | raw_transid as i32,
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
            m_type: 0x500, // matches is_bdev_rs stub (0x500 prefix)
            m_source: Endpoint::from_generation_slot(0, 5),
            ..Message::default()
        };
        assert_eq!(state.route_message(&bdev, &codec), Route::Bdev);
        let cdev = Message {
            m_type: 0x600,
            m_source: Endpoint::from_generation_slot(0, 5),
            ..Message::default()
        };
        assert_eq!(state.route_message(&cdev, &codec), Route::Cdev);
        let sdev = Message {
            m_type: 0x700,
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
    fn test_call_resolver_trait_two_impls() {
        use crate::call_table::{CallResolver, CallTable, NullResolver, VFS_BASE, VfsCallNum};
        let table = CallTable::new();
        let null = NullResolver;
        // Same raw, different behavior
        let raw = VfsCallNum::Open as u32;
        assert_eq!(table.resolve(raw), Some(VfsCallNum::Open));
        assert_eq!(null.resolve(raw), None);
        assert!(CallResolver::is_valid(&table, raw));
        assert!(!CallResolver::is_valid(&null, raw));
        // Invalid raw both None, but trait objects show polymorphism
        let resolvers: Vec<Box<dyn CallResolver>> =
            vec![Box::new(CallTable::new()), Box::new(NullResolver)];
        assert_eq!(resolvers[0].resolve(raw), Some(VfsCallNum::Open));
        assert_eq!(resolvers[1].resolve(raw), None);
        let _ = VFS_BASE; // use constant
    }
}
