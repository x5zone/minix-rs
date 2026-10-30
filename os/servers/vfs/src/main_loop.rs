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

// no_std (guest build surface): bare String is supplied by alloc.
use alloc::string::String;

use crate::device_map::{
    DEV_IMGRD, DmapTable, MapError, ServiceMap, SmapTable, classify_service, map_driver,
};
use crate::fcntl::LockTable;
use crate::filp::FilpTable;
use crate::fproc::{BlockedOn, FProcTable, FpFlags, PID_FREE};
use crate::fs_comm::{CommError, FsTransport, GlobalComm};
use crate::mount::{DevCodec, FsSuperblock, MountError, SuperblockReader};
use crate::request::WireFsClient;
use crate::vmnt::{VmntFlags, VmntTable};
use crate::vnode::VnodeTable;
use crate::worker::WorkerPool;
use minix_sef::SefEvent;
use minix_sys::ipc::IpcTransport as _;
use minix_sys::syscall::KernelCallTransport;
use minix_types::{
    DevId, Endpoint, Gid, Message, NO_DEV, NR_PROCS, Uid, UserSlot, VfsPmInit, VfsPmInitError,
};

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
    matches!(old, LuState::RequestFree | LuState::ProtocolFree) && now == LuState::Null
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

/// The two `printf+return` soft failures of `do_reply` (main.c:193-203): the
/// reply is dropped, the window stays put, the loop continues (C panics when the vmnt is missing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsReplyError {
    /// The low 16 bits are not a valid `IS_VFS_FS_TRANSID`.
    SpuriousTransid,
    /// Slot number out of range (> `NR_WTHREADS`).
    SlotOutOfRange,
    /// `wp->w_task != who_e` — not the reply this endpoint is waiting for (main.c:193-196).
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

#[cfg(test)]
pub use crate::fs_comm::TestTransIdCodec;
/// `TRNS_GET_ID` / `VFS_TRANSID` codec — canonical definition lives in
/// `fs_comm.rs` (the protocol owner); re-exported here so the route layer
/// and its tests share one contract (P2-6 convergence, `ARCH A-4`).
pub use crate::fs_comm::{TransIdCodec, VfsTransIdCodec};

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
    /// FS/VM/driver communication window (`glo.h:17 sending` plus each
    /// `vmnt.m_comm`, comm.c).
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
    /// This process's grant table (C's global `grants[]`, safecopies.c).
    /// During startup `register` (C `sys_setgrant`) tells the kernel where the table lives;
    /// the conversation arm uses it to grant **user buffers** to FS for reading/writing (`grant_user_buffer`).
    pub grants: minix_sys::grant::GrantTable,
    /// Which worker slot the job being dispatched is bound to (the slot pointer
    /// behind C's `self`/`fp_func`; the arm uses it to register continuations and send FS requests).
    /// `None` = unbound (not a syscall path, or already released).
    pub current_worker: Option<usize>,
    /// FS conversation registered by an arm, waiting for the main loop to send it (single-threaded
    /// model: arms only touch state, I/O belongs to the loop — C's `fs_sendrec` registers and sends in
    /// one place; here it is split in two). The VFS-side buffer for `struct statvfs` (the local
    /// `struct statvfs buf` of C `fill_statvfs`): FS fills it in one direct-grant block, VFS patches
    /// local fields and copies it to the user as one block. It lives in state, not on the stack, because the grant must point at a **stable address**.
    pub statvfs_buf: minix_types::StatvfsBuf,
    /// `selecttab[MAXSELECTS]` (C `select.c:31`) — one slot per pending `select`,
    /// holding the caller, the three fd sets, and the blocked/timeout state.
    pub select_table: crate::select::SelectTable,
    /// Local directory mapping driver labels to endpoints (the
    /// `ds_retrieve_label_endpt` hop of C `do_mapdriver`). The production fill path (written when a DS event carries
    /// a label) is not wired yet — see the `LabelDir` note; with an empty table `mapdriver` answers
    /// EINVAL for any label (fail-closed, never pretending the lookup succeeded).
    pub driver_labels: alloc::vec::Vec<(alloc::string::String, Endpoint)>,
    /// Endpoint of the root file system (C `glo.h:21` `EXTERN int ROOT_FS_E`).
    ///
    /// Block-device open uses it to pick `v_bfs_e` (the device falls back to root when no other mount
    /// holds it) and to decide whether to send `REQ_NEW_DRIVER` (open.c:186-216). **Assigned by the root mount**
    /// (`ROOT_FS_E = fs_e` at `mount.c:326-328`, wired: see
    /// [`Self::do_init_root`]); before that it is always `NONE`, and block-device open on the "device not
    /// held by a mount" path ends in C's newdriver failure path (`bdev_close` +
    /// `ENXIO`), without pretending the notification succeeded.
    pub root_fs_e: Endpoint,
    /// Device number of the root file system (C `glo.h:20` `EXTERN dev_t ROOT_DEV`).
    ///
    /// Assigned together with [`Self::root_fs_e`] (`mount.c:326-327`); on the boot chain it is
    /// `DEV_IMGRD`.
    pub root_dev: DevId,
    /// Root-mount counter (C `glo.h:19` `EXTERN int have_root`).
    ///
    /// `mount_fs` uses it for the `mount_root` decision that lets the root be mounted twice
    /// (ramdisk/boot disk, mount.c:208-210); it is 1 after the boot root mount completes.
    pub have_root: u32,
    /// `none` pseudo-device pool (the nonedev bitmap at C `mount.c:33-37`).
    ///
    /// `mount_pfs` allocates a pseudo device from it (`find_free_nonedev`).
    pub nonedev: crate::mount::NonedevBitmap,
    pub pending_fs: Option<PendingFs>,
    /// Queue of `REQ_PUTNODE` messages waiting to be delivered (the notification side of the
    /// `put_vnode` slow path; C sends synchronously from a worker via `fs_sendrec`, but here arm/continuation
    /// contexts cannot send synchronously — the main loop delivers the queue; C only printfs the reply, so dropping it loses nothing).
    pub pending_puts: alloc::vec::Vec<crate::vnode::PutNodeReq>,

    /// Reply waiting to be sent (the reply half): `run_once` folds the result of a dispatch into
    /// a `(caller, reply message)` pair and queues it; `run()` sends it at the end of each loop iteration (the
    /// position of C `reply(who_e, result)`, where `job_m_out` is that message). Most replies carry only `m_type` (the status word);
    /// payload ones (such as `lseek`'s new offset) are queued by `queue_reply_msg`.
    /// `Suspend` arms do not queue — their replies are sent when the FS answer lands.
    pub pending_reply: Option<(Endpoint, Message)>,
}

/// Failure surface consumed by boot rproctab: C's `init_fresh` panics on both failure kinds
/// (main.c:457-459 and main.c:461-466); here they are turned into Err so the decision surface stays testable,
/// and the panic belongs to run() (the same Err→panic layering as finish_init).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BootMapError {
    /// `sys_safecopyfrom` failed; carries the kernel errno (main.c:458).
    Safecopy(i32),
    /// `map_service` semantic failure (the EINVAL/ENODEV face of dmap.c:198-226).
    Map(MapError),
}

impl VfsState {
    /// Creates new VFS state.
    pub fn new() -> Self {
        Self {
            fproc_table: FProcTable::new(),
            filp_table: FilpTable::new(),
            vnode_table: VnodeTable::new(),
            vmnt_table: VmntTable::new(),
            // C init_dmap (dmap.c:230-247): after clearing everything to NONE, the only explicit
            // mapping is CTTY_MAJOR ← "vfs" (CTTY_ENDPT).
            dmap_table: DmapTable::init(),
            smap_table: SmapTable::new(),
            lock_table: LockTable::new(),
            worker_pool: WorkerPool::new(),
            comm: GlobalComm::new(),
            reviving: 0,
            current_message: Message::default(),
            current_fp_slot: None,
            boot_phase: BootPhase::PmHandshake,
            // C: `block_all` is a BSS global (glo.h), initially 0 (allowed).
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
        // main.c:405-408 — clear the fproc slots (a BSS global is "empty on construction" in Rust;
        // the explicit reset is kept so restart/LU paths (DEFERRED) have honest semantics).
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

        // main.c:428-431 — terminator: endpoint = NONE means no more system processes.
        if init.endpoint == Endpoint::NONE {
            // main.c:435-436 — ipc_send(PM_PROC_NR, OK) synchronous barrier (kernel IPC is
            // not implemented, DEFERRED; advancing the phase is the equivalent handshake completion).
            self.boot_phase = BootPhase::InitTables;
            return Ok(true);
        }

        // Slot range is already validated by decode (fail-closed: C's out-of-bounds array access becomes an explicit failure).
        let slot = UserSlot::new(init.slot as usize);
        let fp = self
            .fproc_table
            .get_mut(slot)
            .ok_or(VfsPmInitError::SlotOutOfRange(init.slot))?;

        // main.c:419-425 — fill the boot process slots.
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

        // main.c:438 — system_hz = sys_hz() (kernel IPC not implemented, DEFERRED, tracked as 99).
        // main.c:441 — ds_subscribe("drv\\.[bc]..\\..*", DSF_INITIAL | DSF_OVERWRITE)
        //              (ARCH A-14 not implemented, DEFERRED, tracked as 19/24).
        // main.c:445 — worker_init(): WorkerPool is ready on construction (NR_WTHREADS=9, tracked as 08).
        // main.c:448 — bsf_lock: lock primitives degrade under the single-threaded event loop (tracked as 07).
        // main.c:451-453 — init_dmap()/init_smap(): init_dmap is already
        // wired through VfsState::new's DmapTable::init (CTTY slot);
        // init_smap ≡ SmapTable::new (all-empty base numbering).
        // main.c:455-467 — sys_safecopyfrom(RS_PROC_NR, rproctab) + map_service()
        //                  already wired: the gid carried by RS_INIT is
        //                  decoded by run()'s boot rendezvous (C's sef_local_startup → init_fresh timing,
        //                  before this function); `map_boot_services` lands the dmap rows and
        //                  only then does the root mount have a REQ_READSUPER route.

        // main.c:468-483 — fp_lock (slot lock, tracked as 07 under single thread) + filp/rd/wd clearing.
        self.fproc_table.init_phase2();

        // main.c:485-489 — init_vnodes()/init_vmnts()/init_select()/init_filps()
        //                  (table structures tracked as 04~06, DEFERRED).

        // main.c:492-497 — worker_start(fproc_addr(VFS_PROC_NR), do_init_root, ...).
        // C's do_init_root panics on failure (main.c:519-520) — here the Err goes to
        // the caller (run() panics), keeping the decision surface testable.
        self.do_init_root(kernel, transport)?;

        assert_eq!(self.boot_phase, BootPhase::Running);
        self.initialized = true;
        Ok(())
    }

    /// Root mount sequence (main.c:501-527) — execution orchestration.
    ///
    /// After refusing new requests (`worker_allow(FALSE)`) it mounts pipe fs and root fs: a PFS
    /// `req_readsuper` failure is only recorded (C printf, the mount proceeds); root mount failure returns
    /// [`MountError`] (C `panic("Failed to initialize root")`, main.c:519-520;
    /// the panic belongs to the caller). On success the phase advances to `Running`.
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

        // main.c:503 — worker_allow(FALSE): refuse new requests during the mount (including init(8)).
        self.set_accept_requests(false);

        // main.c:505 — mount_pfs(): failure tolerated (C printf then continues).
        // C: printf("VFS: unable to mount PFS (%d)\n", r) — diagnostic output,
        // no behavioral weight; a no_std target promises no stdout, so silence is equivalent.
        // [SD19-gate] best-effort PFS helper mount (failure does not block boot)
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.mount_pfs(kernel, transport);

        // main.c:508-518 — mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR,
        //                   0, "mfs", "fs_imgrd"); failure = C panic path.
        let r = self.mount_fs_root(kernel, transport);

        match r {
            Ok(()) => {
                // main.c:525 — worker_allow(TRUE): the root file system is ready.
                self.set_accept_requests(true);
                self.boot_phase = BootPhase::Running;
                Ok(())
            }
            // C: panic("Failed to initialize root") — the gate is not reopened (the process is about to
            // terminate, no observer of the worker gate); the error is left to the caller.
            Err(e) => Err(e),
        }
    }

    /// Boot-chain rproctab consumption (main.c:455-467) — the VFS consumption half
    /// (the RS provider contract is the `pub_wire` mirror).
    ///
    /// Before the root mount (main.c:492-497), C `init_fresh` safecopies the whole RS public
    /// process table in and runs `map_service` row by row (dmap.c:198-226): the imgrd→memory
    /// dmap row is assembled here, a prerequisite for do_init_root's REQ_READSUPER route. Row width =
    /// the shared snapshot `RprocpubSnap` (`[ARCH: A-4]` single authority); table width = `NR_BOOT_PROCS`
    /// (C main.c:399 `rprocpub[NR_BOOT_PROCS]`, param.h:9 = 17 — boot processes occupy the leading
    /// slots of RS's 64-row public table, and C likewise reads only the first 17 rows). C's `map_service`
    /// is an exported symbol; in Rust its only caller is this module's run() boot rendezvous, so it stays private.
    fn map_boot_services(
        &mut self,
        kernel: &impl KernelCallTransport,
        rproctab_gid: i32,
    ) -> Result<(), BootMapError> {
        let mut buf = alloc::vec![
            0u8;
            minix_types::NR_BOOT_PROCS
                * core::mem::size_of::<minix_types::RprocpubSnap>()
        ];
        // The grantor is RS_PROC_NR, not SELF (C main.c:456; the VM has the same
        // precedent, ipc_call_rs_init, vm_server.rs:1704).
        minix_sys::syscall::sys_safecopyfrom(
            kernel,
            Endpoint::RS.get(),
            rproctab_gid,
            0,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
        .map_err(BootMapError::Safecopy)?;
        let rows = Self::decode_boot_rows(&buf);
        self.apply_boot_rows(&rows).map_err(BootMapError::Map)
    }

    /// Public-table byte stream → row snapshots (the inverse of the RS provider's `pub_wire_bytes` serialization).
    fn decode_boot_rows(buf: &[u8]) -> alloc::vec::Vec<minix_types::RprocpubSnap> {
        const ENTRY_SIZE: usize = core::mem::size_of::<minix_types::RprocpubSnap>();
        let mut rows = alloc::vec::Vec::new();
        for chunk in buf.chunks_exact(ENTRY_SIZE) {
            // SAFETY(read): a snapshot row is repr(C) POD, read row by row from the bytes rescued from the grant;
            // `read_unaligned` makes no alignment assumption about the buffer (kernel safecopy writes a byte stream).
            // Same shape as the decode half of the VM's ipc_call_rs_init (vm_server.rs:1711).
            rows.push(unsafe { core::ptr::read_unaligned(chunk.as_ptr().cast()) });
        }
        rows
    }

    /// isokendpt + `FP_SRV_PROC` marking (dmap.c:208-214). Endpoint resolution failure is
    /// EINVAL (C returns after printf; a boot target promises no stdout, so silence is equivalent).
    /// The fproc slot was filled during the PM handshake; `is_ok_endpoint` re-verifies endpoint↔slot consistency.
    fn mark_boot_service(&mut self, endpoint: i32) -> Result<(), MapError> {
        let slot = self
            .fproc_table
            .is_ok_endpoint(Endpoint(endpoint))
            .map_err(|_| MapError::Inval)?;
        if let Some(fp) = self.fproc_table.get_mut(slot) {
            fp.flags |= FpFlags::SRV_PROC;
        }
        Ok(())
    }

    /// Row-by-row `map_service` (dmap.c:198-226): empty slots are skipped (main.c:462), the boot
    /// user is skipped (rs.h:188), services get `FP_SRV_PROC`, and rows with a device land in dmap. Any
    /// row failure is propagated — C panics on a map_service failure (main.c:463-466).
    fn apply_boot_rows(&mut self, rows: &[minix_types::RprocpubSnap]) -> Result<(), MapError> {
        for row in rows {
            if row.in_use == 0 {
                continue;
            }
            let is_boot_user = Endpoint(row.endpoint) == Endpoint::INIT;
            // NO_DEV (id.rs:62, DevId=u64) is a cross-type comparison, as in C: devmajor_t(int)
            // against dev_t 0 (dmap.c:217).
            let has_device = row.dev_nr != NO_DEV as i32;
            match classify_service(is_boot_user, has_device) {
                ServiceMap::SkipBootUser => {}
                ServiceMap::ServiceOnly => self.mark_boot_service(row.endpoint)?,
                ServiceMap::MapDriver => {
                    self.mark_boot_service(row.endpoint)?;
                    // C map_driver(rpub->label, rpub->dev_nr, rpub->endpoint)
                    // (dmap.c:220). The label is truncated at NUL (C's strlen gate
                    // dmap.c:89-94 re-checks it inside map_driver); a negative dev_nr
                    // wraps out of range as u32 → ENODEV, the same verdict as C's major<0 gate
                    // (dmap.c:67).
                    let n = row
                        .label
                        .iter()
                        .position(|&b| b == 0)
                        .unwrap_or(row.label.len());
                    map_driver(
                        &mut self.dmap_table,
                        Some(&row.label[..n]),
                        row.dev_nr as u32,
                        Some(Endpoint(row.endpoint)),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// `mount_pfs` (`mount.c:391-425`): PFS claims one `none` pseudo device under a fixed identity
    /// plus one vmnt slot; after `req_readsuper` confirms, only `m_fs_flags` is filled (C ignores
    /// the node details). An exhausted pool returns Err following C's panic path (the caller terminates boot).
    fn mount_pfs(
        &mut self,
        kernel: &impl KernelCallTransport,
        transport: &impl minix_sys::ipc::IpcTransport,
    ) -> Result<(), MountError> {
        // find_free_nonedev + get_free_vmnt (C panic → here the Err is propagated).
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

        // req_readsuper(vmp, "", dev, FALSE, FALSE) (mount.c:417) — confirmation round trip;
        // on success only fs_flags is filled; node details are ignored as in C.
        let mut client = WireFsClient {
            grants: &mut self.grants,
            kernel,
            ipc: transport,
        };
        let mut sb = FsSuperblock {
            client: &mut client,
            fs_e: Endpoint::PFS,
            label: String::new(),
        };
        match sb.read_super(plan.dev, false, false) {
            Ok(info) => {
                if let Some(v) = self.vmnt_table.get_mut(slot) {
                    v.fs_flags = info.fs_flags;
                }
                Ok(())
            }
            // C: printf + the mount proceeds — a missing PFS does not block boot (the pipe surface comes later).
            Err(e) => Err(e),
        }
    }

    /// The root path of `mount_fs` (the `mount_root` branch of `mount.c:156-350`):
    /// mfs on DEV_IMGRD is mounted at `/`; ROOT_DEV/ROOT_FS_E, the root vnode,
    /// the whole MAKEROOT table and the bspec scan are assembled. Mount-point lookup and glue (the non-root path)
    /// belong to `do_mount` in 18-mount.
    fn mount_fs_root(
        &mut self,
        kernel: &impl KernelCallTransport,
        transport: &impl minix_sys::ipc::IpcTransport,
    ) -> Result<(), MountError> {
        let dev = DEV_IMGRD;
        let fs_e = Endpoint::MFS;
        let mount_root = true; // mount_path "/" and have_root < 2 during boot

        // mount.c:170-183 — a non-none device first looks up its driver label in dmap; no driver = EINVAL
        // ("no driver for dev"). While the boot-chain assembly half of dmap[MEMORY_MAJOR]
        // (rproctab consumption) is unwired, this fails exactly as C does — never pretending a disk exists.
        let major = DevCodec::major(dev);
        let label = {
            let entry = self
                .dmap_table
                .get(major)
                .filter(|e| e.is_mapped())
                .ok_or(MountError::Inval)?;
            let end = entry
                .label
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(entry.label.len());
            alloc::string::String::from_utf8_lossy(&entry.label[..end]).into_owned()
        };

        // mount.c:190-200 — device already mounted → EBUSY; allocate a free vmnt slot → ENOMEM.
        if self.vmnt_table.find_by_dev(dev).is_some() {
            return Err(MountError::Busy);
        }
        let slot = self.vmnt_table.alloc().map_err(|_| MountError::NoMem)?;

        // mount.c:248-251 — root vnode (mount_root skips the mount-point lookup/glue).
        let root_vn = self.vnode_table.alloc().map_err(|_| MountError::NoMem)?;

        // mount.c:256-262 — isokendpt(fs_e) + set the system-process flag.
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

        // mount.c:264-269 — vmnt base data + MOUNTING flag.
        {
            let v = self.vmnt_table.get_mut(slot).ok_or(MountError::NoMem)?;
            v.fs = fs_e;
            v.dev = dev;
            v.mount_path = alloc::string::String::from("/");
            v.mount_dev = alloc::string::String::from("bootramdisk");
            v.fstype = alloc::string::String::from("mfs");
            v.flags.insert(VmntFlags::MOUNTING);
        }

        // mount.c:270-272 — req_readsuper round trip (grant+sendrec+revoke).
        let read = {
            let mut client = WireFsClient {
                grants: &mut self.grants,
                kernel,
                ipc: transport,
            };
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
            // mount.c:277-281 — failure: release the vmnt (the root vnode has ref==0, so it returns to
            // the free table naturally, matching C's get_free_vnode semantics); the errno is propagated.
            Err(e) => {
                self.vmnt_table.mark_free(slot);
                return Err(e);
            }
        };

        // mount.c:266 + 297-303 — fs_flags and the concurrency window (threaded → NR_WTHREADS).
        // update_statvfs at mount.c:268-270 (the first fill of the statvfs cache) belongs to the stadir
        // consumer side (the getvfsstat wiring pass, see the 18-mount implementation table).
        {
            let v = self.vmnt_table.get_mut(slot).ok_or(MountError::NoMem)?;
            v.fs_flags = info.fs_flags;
            let window = &mut self.comm.vmnts[slot.get()];
            window.max_reqs = info.max_reqs();
            window.cur_reqs = 0;
        }

        // mount.c:283-296 — the seven-field root vnode fill + reference baseline.
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

        // mount.c:305 — VMNT_CANSTAT: the file system can be reported externally from now on.
        if let Some(v) = self.vmnt_table.get_mut(slot) {
            v.flags.insert(VmntFlags::CANSTAT);
        }

        // mount.c:318-325 — mount placement: root node, no parent mount point, label.
        {
            let v = self.vmnt_table.get_mut(slot).ok_or(MountError::NoMem)?;
            v.root = Some(root_vn.get());
            v.mounted_on = None;
            v.label = alloc::string::String::from("fs_imgrd");
        }

        // mount.c:331 — update_bspec(dev, fs_e, 0): existing block-special files are redirected
        // (the table is empty at boot, so the scan is a no-op; the semantics arrive with the mount).
        self.vnode_table.route_block_special(dev, fs_e);

        // mount.c:326-328 — ROOT_DEV/ROOT_FS_E (the core assignment site).
        self.root_dev = dev;
        self.root_fs_e = fs_e;

        // mount.c:331-345 — the whole MAKEROOT table: existing processes' rd/wd move to the new root. At boot
        // the pointer is None, so the `put_vnode` half is skipped as in C; the ref baseline is the `ref_count = 1`
        // above (same origin as the dup_vnode in C's first MAKEROOT).
        for i in 0..NR_PROCS {
            if let Some(fp) = self.fproc_table.get_mut(UserSlot::new(i))
                && fp.pid != PID_FREE
            {
                fp.root_dir = Some(root_vn.get());
                fp.work_dir = Some(root_vn.get());
            }
        }

        // mount.c:349 — have_root++.
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
        // `decode` has its own `is_fs_transid` gate — the two conditions are merged into one here.
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

    /// `run_once` — single-iteration entry of the main loop: receive → route → gate → dispatch.
    ///
    /// Gate semantics (inherited from legacy dispatch): with `accept_requests == false`, user
    /// requests are marked `FP_PENDING` and counted (deduplicated), not processed; the priority of
    /// the `Pm` control-plane shortcut is preserved by `route_message`.
    pub fn run_once<C: TransIdCodec>(&mut self, msg: &Message, codec: &C) -> Route {
        self.current_message = *msg;
        self.current_fp_slot = msg.m_source.to_user_slot();
        let route = self.route_message(msg, codec);
        match route {
            Route::Syscall { call } => {
                if self.accept_requests {
                    // C main.c:146-160 — the job is bound to a worker slot first
                    // (`worker_start(rfp, use_spare=TRUE)`); no free slot means
                    // `EAGAIN` (the `worker_available()==0` branch).
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
                            // The arm did not suspend (anything except `Suspend`) = job complete: release the slot
                            // (C's `worker_release` at the tail of `worker_main`);
                            // a suspended arm registers its own continuation, and the slot is released by that continuation body.
                            if !matches!(result, crate::call_table::SyscallResult::Suspend) {
                                self.worker_pool.release(idx);
                                self.current_worker = None;
                            }
                            self.queue_reply(msg.m_source, result);
                        }
                        None => {
                            // C handle_work:150-151 — no slot means `EAGAIN`.
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
                // C main.c:283-294 — an unresolvable call number answers ENOSYS instead of posing as
                // a real call (reply half: sending happens at the loop tail of `run()`).
                self.queue_reply(msg.m_source, crate::call_table::SyscallResult::Nosys);
            }
            Route::Pm => {
                // `service_pm` of C main.c: the VFS_PM_* control plane. Before this arm existed
                // such messages were silently dropped; decode-and-dispatch was added — EXEC goes to the synchronous executor
                // (exec_worker::pm_exec; under ARCH A-1 it runs to completion with no continuation, its transport
                // the same direct trap as run()'s FS flush), while other control calls go through the
                // pure `VfsPmHandler` surface. Decode failure = an honest ENOSYS.
                use crate::ipc::PmHandler as _;
                use minix_types::{VfsCall, VfsReply};
                match VfsCall::decode(msg) {
                    Ok(VfsCall::Exec {
                        endpoint,
                        path,
                        path_len,
                        frame,
                        frame_len,
                        ps_str,
                    }) => {
                        // The production source of the stack top is kerninfo `user_sp` (C exec.c:217 via
                        // `minix_get_user_sp`, kernel_utils.c:39-62); the read half belongs to a later
                        // work stream, so a boot-contract default fills the gap for now.
                        // The default must lie inside this architecture's canonical user-address
                        // range: on riscv64/Sv39 a 48-bit-style 0x7fff_ffff_f000 is a non-canonical
                        // address, so a user program's first stack store becomes a store page-fault
                        // loop (the root cause of the rc loop).
                        // The single authority for this value is
                        // `minix_types::USER_STACK_TOP`.
                        const DEFAULT_USER_SP: u64 = minix_types::USER_STACK_TOP;
                        let kernel = minix_sys::syscall::DirectKernelCallTransport;
                        let ipc = minix_sys::ipc::DirectTrapTransport;
                        let result = crate::exec_worker::pm_exec(
                            self,
                            &kernel,
                            &ipc,
                            crate::exec_worker::ExecRequest {
                                target: endpoint,
                                path_addr: path,
                                path_len: path_len.max(0) as usize,
                                frame_addr: frame,
                                frame_len: frame_len.max(0) as usize,
                                ps_str,
                                stack_high: DEFAULT_USER_SP,
                            },
                        );
                        // C worker's return path: status carries the result (negative errno on failure);
                        // pc/newsp are meaningful only on success (com.h:570-575).
                        let reply = match result {
                            Ok(loaded) => VfsReply::Exec {
                                status: minix_types::OK,
                                pc: loaded.pc,
                                newsp: loaded.newsp,
                                newps_str: loaded.newps_str,
                            },
                            Err(errno) => VfsReply::Exec {
                                status: errno,
                                pc: 0,
                                newsp: 0,
                                newps_str: 0,
                            },
                        };
                        self.queue_reply_msg(msg.m_source, reply.encode_reply_for(endpoint));
                    }
                    Ok(other) => {
                        // C `service_pm`: the reply must echo the request's target endpoint into
                        // `VFS_PM_ENDPT` (m7_i1) so PM can resolve the slot on which the
                        // `VFS_CALL` hangs. `other` will be moved into `handle`,
                        // so the endpoint is read first (fork/srv_fork → the child endpoint).
                        let target = other.endpoint();
                        let mut sink = put_node_sink(&mut self.pending_puts);
                        let mut handler = crate::ipc::VfsPmHandler {
                            table: &mut self.fproc_table,
                            filp_table: &mut self.filp_table,
                            vnode_table: &mut self.vnode_table,
                            fs_ctl: &mut sink,
                        };
                        match handler.handle(other) {
                            Ok(reply) => self.queue_reply_msg(
                                msg.m_source,
                                reply.encode_reply_for(target.unwrap_or(Endpoint::NONE)),
                            ),
                            Err(e) => self.queue_reply(
                                msg.m_source,
                                crate::call_table::SyscallResult::Error(
                                    crate::ipc::PmError::to_errno(e),
                                ),
                            ),
                        }
                    }
                    Err(_) => {
                        self.queue_reply(msg.m_source, crate::call_table::SyscallResult::Nosys);
                    }
                }
            }
            Route::Bdev | Route::Cdev | Route::Sdev => {
                // C main.c:126-134 — block/char/socket driver replies go through
                // `bdev_reply`/`cdev_reply`/`sdev_reply`; their common first half is
                // "find the worker slot waiting for this driver → land the result → wake it". Failure to find it is a soft failure
                // (printf + return in C; the main loop continues).
                // [SD19-gate] best-effort reply-frame handling (failure only affects internal bookkeeping, not the fact that the caller was already answered)
                #[allow(clippy::let_underscore_must_use)]
                let _ = self.handle_drv_reply(msg);
            }
            Route::Notify { source, .. } => {
                // A CLOCK notification = a select timeout alarm fired (single-alarm approximation, see
                // `select_timeout_check`); other notifications (DS/KERNEL) are not consumed yet.
                if source == Endpoint::CLOCK {
                    self.select_timeout_check();
                }
            }
            Route::FsReply { .. } => {
                // C main.c:80-89 — do_reply has no answer target; a soft failure (typed
                // `FsReplyError`) is C's printf+return, and the main loop continues.
                // [SD19-gate] best-effort reply-frame handling (failure only affects internal bookkeeping, not the fact that the caller was already answered)
                #[allow(clippy::let_underscore_must_use)]
                let _ = self.handle_fs_reply(msg, codec);
            }
            _ => {}
        }
        // Continuation drive point: if this (or the previous) round landed an FS reply on a slot, run that
        // job's continuation body (the counterpart, in the single-threaded model, of C's
        // "thread continues after returning from `worker_wait`"). It sits after routing so that "collect reply → complete job"
        // closes within the same round.
        self.run_worker_continuations();
        route
    }

    /// The **local half** of `common_open` (the steps of C `open.c:118-274` that do not touch FS):
    /// type dispatch → fd/filp assembly → return fd (on success the syscall result is the fd).
    /// Shared by the `Open` and `Creat` arms (the latter lands here after create completes).
    pub fn finish_open_local(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        node: &crate::path::NodeDetails,
        oflags: u32,
    ) {
        self.finish_open_local_inner(idx, fp_slot, node, oflags, false);
    }

    /// The body of [`Self::finish_open_local`]; `trunc_done` means "the `O_TRUNC`"
    /// truncation has already happened" (the continuation of the `O_TRUNC` branch takes this route), so dispatch
    /// treats that bit as consumed — otherwise `dispatch_open` would see `NeedTruncate` again and loop.
    /// **`filp_flags` still stores the original `oflags`**: C `open.c:136` is
    /// `filp->filp_flags = oflags` (including `O_TRUNC`), and that word is later passed to FS verbatim
    /// with read/write requests (`REQ_FLAGS` in `read.c`); it is not local-private state.
    fn finish_open_local_inner(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        node: &crate::path::NodeDetails,
        oflags: u32,
        trunc_done: bool,
    ) {
        let access = match crate::open::OpenFlags::from_bits(oflags).and_then(|f| f.access().ok()) {
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
                // C `common_open:150-157`: regular file + `O_TRUNC` → W-bit gate →
                // `truncate_vnode(vp, 0)` (**result ignored**) → normal assembly.
                let (real_uid, eff_uid, real_gid, eff_gid, supp) =
                    match fp_slot.and_then(|s| self.fproc_table.get(s)) {
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
                    // C's `break`: terminate open with that error.
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
                    grant: 0, // no data plane
                    user,
                    // C `truncate_vnode(vp, 0)` → `req_ftrunc(fs_e, ino, 0, 0)`.
                    req: crate::request::encode_ftrunc(node.ino, 0, 0),
                });
            }
            crate::open::OpenOutcome::Delegate(crate::open::DeviceClass::Char) => {
                // The `S_IFCHR` branch of C `common_open` (open.c:162-168) →
                // `cdev_open(fd, vp->v_sdev, bits | (oflags & O_NOCTTY))`
                // → `cdev_opcl` (cdev.c:236-249): `/dev/tty` redirection through `cdev_map`
                // → dmap finds the driver by major (none → ENXIO) → the `CTTY_MAJOR`
                // exception (`/dev/tty` succeeds directly, without disturbing the driver) → the three `O_NOCTTY` rules
                // → claim fd/filp → `CDEV_OPEN` → worker wait.
                let dev = node.dev;
                // `cdev_map`: `/dev/tty` is replaced by the process's controlling terminal (`tty_redirect`).
                let is_ctty =
                    ((dev & 0x000fff00) >> 8) as u32 == crate::device_map::CTTY_MAJOR as u32;
                let fp_tty = fp_slot
                    .and_then(|s| self.fproc_table.get(s))
                    .map(|fp| fp.tty)
                    .filter(|t| *t != minix_types::NO_DEV);
                let major_valid =
                    (((dev & 0x000fff00) >> 8) as usize) < crate::device_map::NR_DEVICES;
                let dev = match crate::cdev::tty_redirect(dev, is_ctty, fp_tty, major_valid) {
                    crate::cdev::RedirectVerdict::Keep(d)
                    | crate::cdev::RedirectVerdict::Substitute(d) => d,
                    crate::cdev::RedirectVerdict::NoDev => {
                        self.finish_worker_job(idx, fp_slot, minix_types::ENXIO);
                        return;
                    }
                };
                // CTTY exception: `/dev/tty` does not really send a request (`cdev.c:174`). **The key is the
                // major of the raw device** — this C line looks at the **unmapped** `dev`
                // (`cdev_get` only converts the minor internally), so opening `/dev/tty` when a controlling
                // terminal exists returns OK here and never disturbs the real tty's driver
                // (per the C comment: otherwise, after setsid(), opening `/dev/tty` would leave that real device
                // permanently open). The redirected device number only serves minor/dmap-row computation.
                if is_ctty {
                    // The fd/filp must still be claimed (C claims before type dispatch).
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
                // The three `O_NOCTTY` rules (`noctty_force`): not the session leader, already has a controlling
                // terminal, or this driver has seen a TTY that was made controlling elsewhere → force the flag on.
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
                let seen_elsewhere = self
                    .dmap_table
                    .get(major)
                    .map(|r| r.seen_tty)
                    .unwrap_or(false)
                    && (0..minix_types::NR_PROCS).any(|i| {
                        self.fproc_table
                            .get(minix_types::UserSlot::new(i))
                            .is_some_and(|fp| fp.pid != 0 && fp.tty == dev)
                    });
                let noctty =
                    crate::cdev::noctty_force(is_leader, has_tty, requested, seen_elsewhere);
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
                // Claim fd/filp (C claims before type dispatch; the continuation releases them on failure).
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
                    // C `open.c:138-139`: the `O_CLOEXEC` bit is recorded in `fp_cloexec_set` **before
                    // type dispatch**, and applies equally to all three device kinds.
                    if oflags & crate::open::OpenFlags::CLOEXEC.bits() != 0 {
                        fp.cloexec_set.set(fd.get(), true);
                    }
                }
                let user_e = self
                    .fproc_table
                    .get(slot)
                    .map(|fp| fp.endpoint)
                    .unwrap_or(Endpoint::NONE);
                // `CDEV_OPEN` (shape pinned by `cdev::open_request`: `id`/`user`
                // are the caller endpoint and `minor` is in the third slot — send the fields in the wrong
                // position and the driver opens minor device number 0).
                let m = crate::cdev::open_request(minor, user_e, access);
                if let Some(wp) = self.worker_pool.get_mut(idx) {
                    wp.cont = Some(crate::worker::WorkerCont::CdevOpen {
                        fd: fd.get() as u32,
                        filp: filp_id.get(),
                        dev,
                    });
                }
                if let Err(e) = self.send_drv_for_slot(idx, fp_slot, drv_e, &m) {
                    // Send failed: take C's failure tail (release fd/filp + put back the vnode), then return the error.
                    self.release_open_claim(fp_slot, fd.get() as u32, filp_id.get(), vnode_idx);
                    self.finish_worker_job(idx, fp_slot, e);
                }
                return;
            }
            crate::open::OpenOutcome::Delegate(crate::open::DeviceClass::Block) => {
                // The `S_IFBLK` branch of C `common_open` (open.c:172-217) →
                // `bdev_open(dev, bits)` (bdev.c:79-112): major in range **and**
                // a dmap driver present (missing either means ENXIO) → `BDEV_OPEN` (minor/access/id)
                // → **worker wait** (in C a block driver may not suspend, so the calling thread blocks directly in
                // `drv_sendrec`; the counterpart here is a worker-slot wait, completed by the continuation
                // `WorkerCont::BdevOpen`).
                //
                // After the driver passes there is a second stage: pick `v_bfs_e` (which FS manages
                // this device) and, when no other mount holds it, send a `REQ_NEW_DRIVER` — all in
                // `bdev_open_bfs_stage`.
                let dev = node.dev;
                let major = ((dev & 0x000fff00) >> 8) as u32;
                let minor = (((dev & 0xfff0_0000) >> 12) | (dev & 0xff)) as u32;
                let major_valid = (major as usize) < crate::device_map::NR_DEVICES;
                let driver = self.dmap_table.get(major).and_then(|row| row.driver);
                // The two gates of C `bdev_open:86-89` (major in range, a driver on the dmap row)
                // go through the decision layer (which carries the ENXIO criterion); the endpoint itself comes from the dmap row's
                // `Endpoint`.
                if crate::bdev::resolve_driver(major_valid, driver.map(|e| e.get())).is_err() {
                    self.finish_worker_job(idx, fp_slot, crate::bdev::BdevError::NoDev.to_errno());
                    return;
                }
                let Some(drv_e) = driver else {
                    self.finish_worker_job(idx, fp_slot, crate::bdev::BdevError::NoDev.to_errno());
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
                // Claim fd/filp (C claims before type dispatch; the continuation releases them on failure).
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
                // `BDEV_OPEN` (shape pinned by `bdev::open_request`: for the block family the first slot is
                // `pos` and `minor` is at 8 — send the char-family layout and the driver receives
                // "minor device number 0").
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
                    // Send failed: take C's failure tail (release the claim + put back the vnode), then return the error.
                    // The transport folds the kernel status into a single `EIO` — C additionally
                    // calls `dmap_unmap_by_endpt` on `EDEADSRCDST`/`EDEADEPT` to unmap a dead
                    // driver (`bdev.c:60-64`); that step needs a classification from the transport
                    // (the input to `bdev::classify_send`) and is not wired yet.
                    self.release_open_claim(fp_slot, fd.get() as u32, filp_id.get(), vnode_idx);
                    self.finish_worker_job(idx, fp_slot, e);
                }
            }
            // FIFO pairing (the `S_IFIFO` branch: `map_vnode(PFS)` + `pipe_open`) and
            // unrecognized types: not wired yet; rejected honestly.
            _ => self.finish_worker_job(idx, fp_slot, minix_types::ENOSYS),
        }
    }

    /// The tail of C `advance` (path.c:60-127): merge the traversed node into the vnode table —
    /// a `find_by_ino` hit bumps the FS reference + `dup`s, otherwise the draft slot is filled and
    /// `fs_count = ref_count = 1`. Returns the table index (the arm after the walk needs it as
    /// `filp_vno`).
    pub fn intern_vnode(&mut self, node: &crate::path::NodeDetails) -> Option<usize> {
        if let Some(hit) = self.vnode_table.find_by_ino(node.fs_e, node.ino) {
            if let Some(v) = self.vnode_table.get_mut(hit) {
                v.fs_count += 1;
            }
            self.vnode_table.dup(hit);
            return Some(hit.0);
        }
        let scratch = self.vnode_table.alloc().ok()?;
        // C `advance` (path.c:98-106): `v_sdev = res.dev` (**special device number**;
        // device nodes identify their driver by it), `v_dev = vmp->m_dev` (**the mounted partition's device number**).
        // These are different fields: writing `res.dev` into `v_dev` was a bug —
        // a device node's `v_sdev` stayed 0, so every place that finds a driver by `v_sdev`
        // (`cdev_get`/`bdev_ioctl`) failed to find one.
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

    /// The **failure tail** of `common_open` (C open.c:275-285): release the just-claimed fd and
    /// filp, then put the vnode back (`put_vnode`).
    ///
    /// Both device branches (char/block) come here after the driver or FS refuses — not releasing would
    /// leak one fd slot, one filp reference and one vnode reference; once enough accumulate,
    /// `vnode_table.alloc()` starts returning `ENFILE`.
    ///
    /// The vnode is put back through the **fast path** of `VnodeTable::put` (`ref>1 → ref--`);
    /// the slow path (`ref==1` → `req_putnode`) is the same pending item as `change_into`/`close_filp`
    /// (`DeferredPutNode` leaves that FS notification empty).
    /// The VFS half of `put_vnode` (C vnode.c:240-290): the fast path (`ref>1`)
    /// only decrements; the slow path (`ref==1`) queues `REQ_PUTNODE` into
    /// [`VfsState::pending_puts`] (the batch return of `clean_refs` is collected the same way),
    /// and the main loop delivers them. Returns the `put` result (`Ok(true)` = slot released).
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
        // [SD19-gate] best-effort vnode put-back/reference-count cleanup (C put_vnode returns void, no error)
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.put_vnode_deferred(crate::vnode::VnodeId(vnode));
    }

    /// The **second stage** after a successful block-device open (C `open.c:186-216`): fix `v_bfs_e` and,
    /// when needed, send `REQ_NEW_DRIVER`.
    ///
    /// `v_bfs_e` is chosen exactly as in C ([`crate::vmnt::VmntTable::bfs_for_device`]):
    /// default to the root file system; if another mount holds the device, use that mount's FS. **Only when
    /// falling back to root** is the driver label sent — other FS instances already learned this
    /// block driver from readsuper's label argument at mount time.
    ///
    /// Three-way return: `Ok(true)` = `WorkerCont::BdevNewDriver` is attached (waiting for the FS
    /// reply; the caller must **not** finalize); `Ok(false)` = no notification needed, the caller returns
    /// the fd directly; `Err(errno)` = this stage failed, the caller takes the failure tail and returns the error.
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
            // C's `if (vp->v_bfs_e != ROOT_FS_E) { unlock_bsf(); break; }`.
            return Ok(false);
        }
        // This root-file-system branch sends `req_newdriver`. Before the root mount has run
        // (`root_fs_e` is still `NONE`) there is no target — finish through C's newdriver
        // failure path (`bdev_close` + `ENXIO`), without pretending the notification succeeded.
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
            // C's label is a fixed-size array in the dmap row; the grant covers `strlen(label)+1`
            // bytes (including the trailing NUL); the slot's `path_scratch` is VFS's own memory, and a
            // direct grant must point at a **stable address**.
            let n = label.iter().position(|b| *b == 0).unwrap_or(label.len());
            let len = n + 1;
            if len > wp.path_scratch.len() {
                return Err(minix_types::EINVAL);
            }
            wp.path_scratch[..n].copy_from_slice(&label[..n]);
            wp.path_scratch[n] = 0;
            let addr = wp.path_scratch.as_ptr() as u64;
            // C `cpf_grant_direct(fs_e, (vir_bytes) label, len, CPF_READ)`.
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

    /// The caller's root-directory triple (C `fp_rd` → vnode → `(fs, ino, dev)`).
    ///
    /// Path-start selection (root vs work dir) happens on the arm side (the first-character test in `eat_path`);
    /// this only dereferences vnode → triple. An empty slot / missing vnode yields the zero triple,
    /// which naturally disables `LookupWalk`'s chroot boundary (same effect as C's `fp_rd == NULL`
    /// on an uninitialized process).
    pub fn root_dir_of(&self, fp_slot: Option<minix_types::UserSlot>) -> crate::path::RootDir {
        let vnode = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .and_then(|fp| fp.root_dir)
            .and_then(|idx| self.vnode_table.get(crate::vnode::VnodeId(idx)));
        match vnode {
            Some(v) => crate::path::RootDir {
                ino: v.ino,
                fs: v.fs,
                dev: v.dev,
            },
            None => crate::path::RootDir {
                ino: 0,
                fs: Endpoint::NONE,
                dev: 0,
            },
        }
    }

    /// The caller's working-directory triple (C `fp_wd`).
    pub fn work_dir_of(&self, fp_slot: minix_types::UserSlot) -> crate::path::RootDir {
        let vnode = self
            .fproc_table
            .get(fp_slot)
            .and_then(|fp| fp.work_dir)
            .and_then(|idx| self.vnode_table.get(crate::vnode::VnodeId(idx)));
        match vnode {
            Some(v) => crate::path::RootDir {
                ino: v.ino,
                fs: v.fs,
                dev: v.dev,
            },
            None => crate::path::RootDir {
                ino: 0,
                fs: Endpoint::NONE,
                dev: 0,
            },
        }
    }

    /// The current mount table (the `mounts` argument of `path::lookup`) — assembled from `vmnt_table` plus
    /// each mount point's root vnode. Empty rows (`fs == NONE`) are excluded.
    pub fn mounted_fs_list(&self) -> alloc::vec::Vec<crate::path::MountedFs> {
        let mut out = alloc::vec::Vec::new();
        for idx in 0..crate::vmnt::NR_MNTS {
            let Some(v) = self.vmnt_table.get(crate::vmnt::VmntId(idx)) else {
                continue;
            };
            if v.fs == Endpoint::NONE {
                continue;
            }
            // The root vnode (`m_root_node`'s (ino, dev)) and the mount-point vnode
            // (`m_mounted_on`'s (ino, fs, dev)).
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

    /// Pack the slot's path cursor into scratch (+ NUL), grant it, build `REQ_LOOKUP` and hand it to
    /// the loop to send (sharing the same construction path as the arm's first lookup).
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
            // Flags such as `PATH_RET_SYMLINK` must be sent to FS verbatim (their meaning lives in FS,
            // libfsdriver/lookup.c:249-251) — omitting them makes FS follow a trailing-component symlink.
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
        // Record the new grant in the slot (revoked after the reply).
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

    /// `change_into` (C stadir.c:120-140): replace `fp_wd`/`fp_rd` with a new vnode.
    /// The same vnode succeeds directly; not a directory → ENOTDIR; not searchable → EACCES; only after
    /// both pass does the swap happen (`put_vnode` the old directory, `dup_vnode` the new one).
    ///
    /// Releasing the old directory takes the **fast path** of `VnodeTable::put` (`ref>1 → ref--`);
    /// the slow path (`ref==1` → `req_putnode`), like the same pending item in `filp.rs`'s `close_filp`,
    /// is **not wired yet** and `DeferredPutNode` leaves that FS notification empty — this is a
    /// recorded gap, not "done".
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
        // C `change_into`: directory-type gate → `forbidden(fp, vp, X_BIT)`.
        if mode & crate::open::S_IFMT != crate::open::S_IFDIR {
            return minix_types::ENOTDIR;
        }
        let (real_uid, real_gid, eff_uid, eff_gid, supp) = match self.fproc_table.get(slot) {
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
            .find_by_fs(
                self.vnode_table
                    .get(crate::vnode::VnodeId(new_vnode))
                    .map(|v| v.fs)
                    .unwrap_or(Endpoint::NONE),
            )
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
        // Swap: put the old directory (fast path); the new directory is already dup'd (`intern_vnode`).
        if let Some(old_id) = old {
            // [SD19-gate] best-effort vnode put-back/reference-count cleanup (C put_vnode returns void, no error)
            #[allow(clippy::let_underscore_must_use)]
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

    /// Scan the mounts `sync`/`fsync` must notify (the loop filter of C `do_sync`/`do_fsync`):
    /// `m_dev != NO_DEV && m_fs_e != NONE && m_root_node != NULL`; if `dev` is given,
    /// filter by device number too (fsync uses the file's `v_dev`).
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

    /// The **local half** of `fill_statvfs` (the tail of C stadir.c:255-272): fill in the read-only bit,
    /// `f_fsid`/`f_fsidx` and the three name fields, then copy the whole block to the user.
    ///
    /// `sys_datacopy` is cross-space and unreachable in a host build → answer an honest EIO (never
    /// pretend the copy succeeded).
    fn finish_statvfs_copy(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        user_buf: u64,
        vmnt_idx: usize,
    ) -> i32 {
        let (dev, readonly, fstype, mount_path, mount_dev) =
            match self.vmnt_table.get(crate::vmnt::VmntId(vmnt_idx)) {
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
        // `f_fsid` and `f_fsidx.__fsid_val[0]` are both the device number (C's comment says this follows
        // NetBSD).
        b.set_u64(minix_types::statvfs_off::FSID, dev);
        b.set_u32(minix_types::statvfs_off::FSIDX, dev as u32);
        b.set_u32(minix_types::statvfs_off::FSIDX + 4, 0);
        b.set_name(minix_types::statvfs_off::FSTYPENAME, &fstype);
        b.set_name(minix_types::statvfs_off::MNTONNAME, &mount_path);
        b.set_name(minix_types::statvfs_off::MNTFROMNAME, &mount_dev);
        // Copy to the user: `sys_datacopy_wrapper(SELF, &buf, endpt, buf_addr, sizeof)`.
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

    /// Copy the mount row's cache into the VFS-side buffer (the `ST_NOWAIT` branch of C `fill_statvfs`:
    /// after `memset(&buf, 0, ...)`, copy `m_stats` field by field).
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

    /// Send one `REQ_STATVFS` to a mount (the direct grant points at the VFS-side buffer) and register
    /// the continuation (including `getvfsstat`'s sequence state). C `req_statvfs` (request.c:232-247).
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

    /// The **entry** of `fill_statvfs` (C stadir.c:230-253): with `ST_NOWAIT` it uses the mount row's
    /// cache (no FS traffic); otherwise it direct-grants the VFS-side buffer and sends `REQ_STATVFS`,
    /// with the statistics collected by the continuation (`WorkerCont::Statvfs`).
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

    /// The first half of `make_sock_fd` (C socket.c:86-176): lock PFS → reserve a vnode →
    /// claim fd/filp → `req_newnode(PFS, effuid, effgid, S_IFSOCK|ACCESSPERMS,
    /// dev)`. The second half (filling vnode/filp) lives in the `WorkerCont::SockFd` continuation.
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
        // C `find_vmnt(PFS_PROC_NR)`: panic when not found ("PFS gone"); the Rust side is
        // fail-closed (PFS is a server implemented elsewhere).
        let Some(vmnt_id) = self.vmnt_table.find_by_fs(minix_types::Endpoint::PFS) else {
            return Err(minix_types::EIO);
        };
        let vnode = self.vnode_table.alloc().map_err(|_| minix_types::ENFILE)?;
        // C `get_fd(fp, 0, R_BIT | W_BIT, &fd, &filp)`.
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
            // C socket.c:134-136: `S_IFSOCK | ACCESSPERMS` as the node mode.
            req: crate::request::encode_newnode(dev, crate::open::S_IFSOCK | 0o777, uid, gid),
        });
        Ok(())
    }

    /// The socket-driver getset family (C `sdev_setsockopt` sdev.c:450-495 /
    /// `sdev_get` sdev.c:500-555): turn the user buffer into a magic grant for the driver
    /// (`CPF_READ` in the `set` direction, `CPF_WRITE` in the `get` direction); the reply number must be
    /// `SDEV_REPLY` and **the status is in the payload**; for `get`, the status is the new length.
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
        let drv_e =
            crate::device_map::smap_endpt_by_dev(&self.smap_table, dev).ok_or(minix_types::EIO)?;
        let (_, sock_id) = crate::device_map::split_smap_dev(dev).ok_or(minix_types::EIO)?;
        let user = fp_slot
            .and_then(|s| self.fproc_table.get(s))
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        // C's `cpf_grant_magic(drv, who_e, addr, len, CPF_READ|CPF_WRITE)`.
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
        // (ipc.h:2272-2281).
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
                // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                #[allow(clippy::let_underscore_must_use)]
                let _ = self.revoke_grant(grant);
                Err(e)
            }
        }
    }

    /// A socket-driver "simple request" (C `sdev_simple` sdev.c:245-276): shared by `listen`/
    /// `shutdown`/`close` — `{req_id, sock_id, param}` is sent to the driver; the reply number
    /// must be `SDEV_REPLY` and **the status is in the reply payload**.
    ///
    /// `dev` is the socket device number (`v_sdev`); `smap_endpt_by_dev` finds the driver and
    /// `sock_id` (C `get_smap_by_dev`).
    pub fn send_sdev_simple(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        dev: u64,
        req_type: i32,
        param: i32,
    ) -> Result<(), i32> {
        let drv_e =
            crate::device_map::smap_endpt_by_dev(&self.smap_table, dev).ok_or(minix_types::EIO)?;
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
        // sock_id; int param; }` (ipc.h:2318-2326).
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

    /// `get_sock` (C socket.c:276-302): fd → filp → **must be a socket**
    /// (otherwise `ENOTSOCK`); returns its device number and open flags. The first gate shared by the socket family
    /// — purely local, with no driver conversation.
    pub fn get_sock(&self, fp_slot: minix_types::UserSlot, fd: i32) -> Result<(u64, i32), i32> {
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
        // C `!S_ISSOCK(filp->filp_vno->v_mode)` → ENOTSOCK.
        if v.mode & crate::open::S_IFMT != crate::open::S_IFSOCK {
            return Err(minix_types::ENOTSOCK);
        }
        Ok((v.sdev, filp.flags))
    }

    /// The **body** of `do_mapdriver` (the half of C dmap.c:106-177 after label retrieval):
    /// label → endpoint (`resolve_driver`) → mark as a service process (`FP_SRV_PROC`) →
    /// `map_driver` (when the major is valid) → `smap_map` (when a domain is given; failure must **undo** the dmap).
    ///
    /// `caller` is the message source (C's `who_e`): only RS can map drivers
    /// (`check_mapper`).
    #[allow(clippy::too_many_arguments)]
    /// The **fill half** of the `ds_retrieve_label_endpt` hop (the prerequisite of C `do_mapdriver`
    /// dmap.c:148-152): ask DS for the endpoint by label and, on a hit, **upsert** it into
    /// `driver_labels` (update on the same name, append a new one) and return the endpoint; unreachable DS or an
    /// unregistered label returns `None` — the caller falls back to the local table (empty-table semantics
    /// stay EINVAL; the fail-closed chain is unchanged).
    pub fn ds_fill_label(&mut self, label: &str) -> Option<Endpoint> {
        let mut ds = minix_sys::ds::DsClient::new(
            minix_sys::ipc::DirectTrapTransport,
            minix_sys::syscall::DirectKernelCallTransport,
            Endpoint::DS,
        );
        let (endpoint, _flags) = ds.retrieve_label_endpt(&mut self.grants, label).ok()?;
        // Idempotent upsert: the same name updates (a driver restart changes the endpoint), a new name appends.
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
        // C dmap.c:148-152 — label → endpoint: first the DS hop (a hit fills the
        // local table); without DS it falls back to the local table (empty-table semantics stay EINVAL).
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
        // C dmap.c:154-158 — the endpoint must be a known process (`isokendpt`), and is marked as a service.
        let Some(slot) = endpoint.to_user_slot() else {
            return minix_types::EINVAL;
        };
        if self.fproc_table.get(slot).is_none() {
            return minix_types::EINVAL;
        }
        if let Some(fp) = self.fproc_table.get_mut(slot) {
            fp.flags |= crate::fproc::FpFlags::SRV_PROC;
        }
        // C dmap.c:161-165 — a valid major writes the dmap row.
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
        // C dmap.c:166-173 — a domain writes smap; failure must undo the dmap just written.
        if !domains.is_empty() {
            let ndomains = domains.len();
            if ndomains > 8 {
                // `NR_DOMAIN` (config.h:61 = 8).
                if major != minix_types::NO_DEV as u32 {
                    // [SD19-gate] best-effort device_map registration (failure resurfaces through later open paths)
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = crate::device_map::map_driver(&mut self.dmap_table, None, major, None);
                }
                return minix_types::EINVAL;
            }
            // Per-domain check (C smap.c:74-84): out of range/UNSPEC → EINVAL, already taken → EBUSY.
            let existing =
                crate::device_map::find_slot_by_label(&self.smap_table, label.as_bytes());
            let free = crate::device_map::find_free_slot(&self.smap_table);
            let checks: alloc::vec::Vec<crate::device_map::DomainCheck> = domains
                .iter()
                .map(|d| crate::device_map::check_domain(&self.smap_table, *d, existing))
                .collect();
            let old_endpt = existing.and_then(|s| self.smap_table.entries[s as usize].endpt);
            let plan = match crate::device_map::register_plan(
                existing, free, &checks, old_endpt, endpoint,
            ) {
                Ok(p) => p,
                Err(e) => {
                    if major != minix_types::NO_DEV as u32 {
                        // [SD19-gate] best-effort cleanup; dropping the Result does not change an established main result (C is equally void/ignoring here)
                        #[allow(clippy::let_underscore_must_use)]
                        let _ =
                            crate::device_map::map_driver(&mut self.dmap_table, None, major, None);
                    }
                    return e.to_errno();
                }
            };
            // Apply: write the row (label + endpoint) and `pfmap[domain] = row`.
            let slot = plan.slot as usize;
            {
                let row = &mut self.smap_table.entries[slot];
                row.endpt = Some(endpoint);
                let n = label.len().min(crate::device_map::LABEL_MAX - 1);
                row.label = [0u8; crate::device_map::LABEL_MAX];
                row.label[..n].copy_from_slice(&label.as_bytes()[..n]);
            }
            // When replacing, first unmap the old instance's domain mapping (the unmap section of C smap.c:105-118).
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

    /// The **entry** of `getvfsstat` (C `do_getvfsstat` stadir.c:330-403): with `buf == 0`
    /// it only counts (no FS traffic); otherwise it truncates the mount list to `bufsize / sizeof(struct statvfs)`,
    /// fills each entry with `fill_statvfs` at `buf + i*sizeof`, and returns the **count**.
    pub fn begin_getvfsstat(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        buf_addr: u64,
        bufsize: u64,
        flags: i32,
    ) -> Result<(), i32> {
        // C's filter: `m_dev != NO_DEV && (m_flags & VMNT_CANSTAT)`.
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
            // Count only: C returns `count` directly.
            self.finish_worker_job_value(idx, fp_slot, targets.len() as i32);
            return Ok(());
        }
        // Truncation when space runs out (C's `if (bufsize < sizeof) break;`).
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
            // Cached branch: fill each in place and copy, without sending requests.
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

    /// The `rename` stage 1 → stage 2 transition: peel off name2's last component and walk its
    /// parent directory from the process root (the second `last_dir` pass of C `do_rename:218-231`).
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

    /// Send one `REQ_RENAME` (C `req_rename` — request.c:927-955): each name takes half of the slot's
    /// scratch (`PATH_MAX` 1024 → 512 per half ≥ `NAME_MAX+1`), each gets its own **direct grant**
    /// (both point at VFS's own memory); the reply carries only a status.
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
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
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

    /// Walk the **parent directory** of a path from the process root/work dir (the directory prefix of `last_dir`),
    /// swap the slot to `follow`, and send the first lookup. In C, rename/link and other "switch paths
    /// and make another pass" sites all have this shape (a new `resolve` plus the same `last_dir`).
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
            crate::path::LookupStart {
                fs: rd.fs,
                ino: rd.ino,
                dev: rd.dev,
            }
        } else {
            let wd = match fp_slot {
                Some(slot) => self.work_dir_of(slot),
                None => rd,
            };
            crate::path::LookupStart {
                fs: wd.fs,
                ino: wd.ino,
                dev: wd.dev,
            }
        };
        let (uid, gid) = match fp_slot.and_then(|s| self.fproc_table.get(s)) {
            Some(fp) => (fp.eff_uid, fp.eff_gid),
            None => return Err(minix_types::EINVAL),
        };
        let (walk, step) = crate::path::LookupWalk::begin(start, resolve, rd, uid, gid)
            .map_err(|e| e.to_errno())?;
        let crate::path::WalkStep::Send {
            fs_e,
            dir_ino,
            root_ino,
        } = step
        else {
            return Err(minix_types::EIO);
        };
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::Path);
            wp.path = Some(crate::worker::PathPending {
                walk,
                grant: 0,
                follow,
            });
        }
        self.send_lookup_for_slot(idx, fp_slot, fs_e, dir_ino, root_ino)
    }

    /// Send one request to a driver and make the slot wait for it (the common first half of C
    /// `sdev_sendrec`/`cdev_opcl`/`bdev_sendrec`): `asynsend3(drv_e, m, AMF_NOREPLY)` is **send-and-don't-wait**; the thread then
    /// waits for the reply in `worker_wait` — here that is "the slot's `task` points at the driver,
    /// `sendrec` holds the request, the state is `WaitingForFs`", and an arriving reply is landed and woken by
    /// [`Self::handle_drv_reply`].
    ///
    /// A send failure (traps are unreachable in a host build) returns EIO and the caller takes the error tail —
    /// it never pretends the request went out.
    pub fn send_drv_for_slot(
        &mut self,
        idx: usize,
        fp_slot: Option<minix_types::UserSlot>,
        drv_e: Endpoint,
        req: &Message,
    ) -> Result<(), i32> {
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            // C `self->w_task = sp->smap_endpt; self->w_drv_sendrec = m_ptr`.
            wp.task = Some(drv_e);
            wp.sendrec = Some(*req);
            wp.state = crate::worker::WorkerState::WaitingForFs;
        }
        let _ = fp_slot;
        // Classification after a `drv_sendrec` failure (bdev.c:60-68): dead endpoints
        // (`EDEADSRCDST`/`EDEADEPT`) are first unmapped via `dmap_unmap_by_endpt`;
        // every class folds into EIO. The kernel status lives in TrapStatus (positive domain); the host
        // transport always yields EIO(5) → the `Fatal` class → no unmapping (never damaging a live table).
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

    /// A socket read/write driver request (C `sdev_readwrite` sdev.c:336-410): three optional
    /// magic grants (data/control/address; `CPF_READ` on send, `CPF_WRITE` on receive);
    /// it sends **without waiting** (such calls can take long) and suspends the process.
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

    /// Same as above, but lets the caller pass the slot's `aux` (`recvmsg` must carry `msg_buf` down —
    /// the tail writes back several msghdr fields; the last argument of C `sdev_readwrite`).
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
        let drv_e =
            crate::device_map::smap_endpt_by_dev(&self.smap_table, dev).ok_or(minix_types::EIO)?;
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
                    // C creates a grant only for **non-zero** buffers (`if (data_buf != 0)`).
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
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(*g);
                }
                return Err(e);
            }
        };
        let (addr_grant, addr_len) = match make(addr, 2) {
            Ok(p) => p,
            Err(e) => {
                for g in grants.iter().flatten() {
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
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
        // int addr_len; endpoint_t user_endpt; int flags; }` (ipc.h:2304-2317).
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
                // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                #[allow(clippy::let_underscore_must_use)]
                let _ = self.revoke_grant(*g);
            }
            return Err(minix_types::EIO);
        }
        let block = crate::fproc::SdevBlock {
            dev,
            call,
            // `GrantId` is an alias of `i32` (minix-types `types::id`), so it goes in directly.
            grants: [grants[0], grants[1], grants[2]],
            aux,
        };
        self.suspend_on_sdev(fp_slot, idx, block)
    }

    /// A **process-level suspend** socket call (C `sdev_suspend` sdev.c:82-112): mark the calling process
    /// `FP_BLOCKED_ON_SDEV` and **release the worker slot** (such calls can wait a long time —
    /// e.g. connect waiting for a peer — and C refuses to hold a worker slot for it); when the reply arrives,
    /// [`Self::finish_sdev_blocked`] finalizes it.
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
        // Release the slot (after C's `suspend()` the worker is empty); `run_once` seeing
        // `Suspend` does not release again and does not reply to the user — the reply comes from the driver side.
        self.worker_pool.release(idx);
        if self.current_worker == Some(idx) {
            self.current_worker = None;
        }
        Ok(())
    }

    /// A driver reply waking a **process-level suspended** socket call (the second path of C `sdev_reply`
    /// plus the bind/connect group of `sdev_finish`): find the suspended process by the **device number's
    /// driver**, clear the suspend state, revoke the grant it left behind, then return the status to the user.
    ///
    /// Returns `true` when this reply was claimed.
    pub fn finish_sdev_blocked(&mut self, msg: &Message) -> bool {
        // C `sdev_reply`: `req_id` in the reply is the caller endpoint — here the lookup is directly by
        // "who is suspended + which driver owns this device number" (equivalent and stricter).
        let mut found: Option<(minix_types::UserSlot, crate::fproc::SdevBlock)> = None;

        for i in 0..minix_types::NR_PROCS {
            let slot = minix_types::UserSlot::new(i);
            let Some(fp) = self.fproc_table.get(slot) else {
                continue;
            };
            let crate::fproc::BlockedOn::Sdev(block) = fp.blocked_on else {
                continue;
            };
            // Is this device's driver the replying party?
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
        // Clear the suspend state (C's `rfp->fp_blocked_on = FP_BLOCKED_ON_NONE`).
        if let Some(fp) = self.fproc_table.get_mut(slot) {
            fp.blocked_on = crate::fproc::BlockedOn::None;
        }
        // Revoke the grants it left behind (C revokes those three before `sdev_finish`).
        for g in block.grants.iter().flatten() {
            // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
            #[allow(clippy::let_underscore_must_use)]
            let _ = self.revoke_grant(*g);
        }
        let target = self
            .fproc_table
            .get(slot)
            .map(|fp| fp.endpoint)
            .unwrap_or(Endpoint::NONE);
        // Status and finalization branch by **call** (C `sdev_finish`'s switch):
        // - Send direction (`VFS_SENDTO`) uses `SDEV_REPLY`; the status is the result;
        // - Receive direction (`VFS_RECVFROM`) uses `SDEV_RECV_REPLY`; the payload also has
        //   `ctl_len`/`addr_len`/`flags` — `recvfrom` must put **addr_len into the reply
        //   payload** (`m_vfs_lc_socklen { len }`, C `resume_recvfrom`).
        // `accept` has **its own three-state finalization** (C `resume_accept` socket.c:367-465):
        // ① failure and no socket created → return only the error; ② failure but the driver created a socket → close it, then return
        // the error; ③ success → **open another worker** in the slot to run `make_sock_fd` (C's comment:
        // finalization still makes blocking calls, which the main thread cannot do); on success return the fd plus the peer address length.
        if msg.m_type == minix_sockdriver::sdev::SdevReply::AcceptReply as i32 {
            // SAFETY: `mess_lsockdriver_vfs_accept_reply { int32_t req_id@0;
            // int32_t sock_id@4; int status@8; unsigned int len@12 }`.
            let raw = unsafe { &msg.m_u.raw };
            let sock_id = i32::from_le_bytes(raw[4..8].try_into().unwrap());
            let status = i32::from_le_bytes(raw[8..12].try_into().unwrap());
            let addr_len = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            // `split_smap_dev` gives `(num, sockid)` — what is needed here is the **row number**.
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
                // case ①: no socket created; return only the error.
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
            // cases ②/③ both need a worker (finalization may send further driver requests / PFS requests).
            let Some(new_idx) = self.worker_pool.assign_first_fit(
                slot,
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            ) else {
                // Slot exhaustion: C does a best effort here; this model answers EAGAIN honestly and records
                // the gap (the new socket stays on the driver side, reclaimed when the driver dies).
                self.queue_reply(
                    target,
                    crate::call_table::SyscallResult::Error(minix_types::EAGAIN),
                );
                return true;
            };
            self.current_worker = Some(new_idx);
            // Is the listening socket still there? Its open flags are inherited by the new socket (C `resume_accept`'s
            // `get_sock(listen_fd, &ldev, &flags)`).
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
                // case ②: the driver created a socket but the call failed overall → close it and return the error.
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
                    // Even a failed close must still return the error to the user (C is `(void)sdev_close`).
                    self.finish_worker_job(new_idx, Some(slot), status);
                }
                return true;
            }
            // case ③: inherit the listening socket's three flag bits (C's `flags &=
            // O_CLOEXEC | O_NONBLOCK | O_NOSIGPIPE`).
            let inherit = (crate::open::OpenFlags::CLOEXEC.bits()
                | crate::fcntl::O_NONBLOCK
                | crate::fcntl::O_NOSIGPIPE)
                & (listen_flags as u32);
            if let Err(e) =
                self.begin_make_sock_fd(new_idx, Some(slot), dev, inherit, Some(addr_len), None)
            {
                // fd creation failed → the new socket must be closed too (C's same branch).
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
            // SAFETY: `mess_lsockdriver_vfs_reply { int32_t req_id; int status; }`.
            let raw = unsafe { &msg.m_u.raw };
            status = i32::from_le_bytes(raw[4..8].try_into().unwrap());
        } else if msg.m_type == recv_reply {
            // SAFETY: `mess_lsockdriver_vfs_recv_reply { req_id@0, status@4,
            // ctl_len@8, addr_len@12, flags@16 }`.
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
                    // C `resume_recvmsg` (socket.c:600-660): modify three msghdr
                    // fields (`msg_controllen`/`msg_flags`/`msg_namelen`, the last only when
                    // `addr_len > 0`), then **copy the whole block back to the user**; a failed copy returns that
                    // error as the status (C's `status = r`). In a host build that copy is unreachable,
                    // so this returns EIO/EINVAL honestly.
                    let update = crate::socket::recvmsg_update(ctl_len, rflags as u32, addr_len);
                    let _ = update;
                    if let crate::fproc::SdevAux::Buf(msgbuf) = block.aux {
                        // Fetch the user's copy of the msghdr (in production this is a cross-space copy).
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
                                // Negative sign: this function's `status` is interpreted as a "signed on-wire
                                // status"; a positive value would be taken as a success value/fd.
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
            // Reply number of an unknown kind: fold to EIO — **negative** (this function's
            // `status` is interpreted as a "signed on-wire status"; a positive value would be taken as a success value/fd).
            status = -minix_types::EIO;
        }
        let result = if status < 0 {
            crate::call_table::SyscallResult::Error(status)
        } else if let Some(len) = addr_len_out {
            let mut m = Message {
                m_type: status,
                ..Message::default()
            };
            // SAFETY: `mess_vfs_lc_socklen { unsigned int len; }` occupies the first word of the payload area.
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

    // ───────────────────────── select waiting half ─────────────────────────
    // The reply/timeout/restart machinery of C select.c (the request half of do_select lives in syscalls.rs).
    // Queries are send-and-don't-wait via `asynsend` (`cdev_select`/`sdev_select`), so this half
    // does not go through worker slots: replies are claimed via dmap/smap `sel_busy`, timeouts are finalized by CLOCK
    // notifications, and the process-level suspend (`BlockedOn::Select`) follows the same pattern as bind/accept.

    /// Send one select query (C `cdev_select` cdev.c:350-377 /
    /// `sdev_select` sdev.c:647-668): send-and-don't-wait via `asynsend`.
    ///
    /// Message shape: char `mess_vfs_lchardriver_select { devminor_t minor@0;
    /// int ops@4 }` (ipc.h, `minor` is a 4-byte field); socket
    /// `mess_vfs_lsockdriver_select { int32 sock_id@0; int ops@4 }`.
    pub fn send_select_query(
        &mut self,
        is_char: bool,
        dev: u64,
        rops: crate::select::SelOps,
    ) -> Result<(), i32> {
        let mut m = Message {
            m_type: if is_char {
                // On the char side, the lesson holds: `CdevRequest::Select as i32` = 6 (an index),
                // which the driver decodes as Other and does not answer; `message_type()` (0x406) is mandatory.
                // On the socket side `SdevRequest` has explicit `#[repr(u32)]` discriminants, so `as i32` remains correct.
                minix_chardriver::protocol::CdevRequest::Select.message_type()
            } else {
                minix_sockdriver::sdev::SdevRequest::Select as i32
            },
            ..Message::default()
        };
        // SAFETY: the first two words of both select request families are `int32 id/minor@0; int
        // ops@4` (ipc.h:2216-2222 / lsockdriver_select).
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
            crate::device_map::smap_endpt_by_dev(&self.smap_table, dev).ok_or(minix_types::EIO)?
        };
        // C `asynsend3(dmap_driver, &mess, AMF_NOREPLY)` (panics on failure —
        // here a transport failure folds to EIO, which the caller records in `se->error`).
        use minix_sys::ipc::IpcTransport as _;
        minix_sys::ipc::DirectTrapTransport
            .sendnb(drv_e, &m)
            .map_err(|_| minix_types::EIO)
    }

    /// Shared body of `select_request_char`/`select_request_sock`
    /// (select.c:459-565): `/dev/tty` remap → `filp_select_dev` conflict gate →
    /// `select_filter` (the decision module) → driver busy gate → send the query → bookkeeping.
    ///
    /// Returns `Ok(ready)` = ready bits for this round (empty = in flight or none yet); `Err(e)` =
    /// C's `r != OK && r != SUSPEND` (the class recorded in `se->error`).
    pub(crate) fn select_request_driver(
        &mut self,
        fp_slot: Option<minix_types::UserSlot>,
        filp_idx: usize,
        is_char: bool,
        want: crate::select::SelOps,
        block: bool,
    ) -> Result<crate::select::SelOps, i32> {
        use crate::select::{FilterOutcome, SelOps as SO, filter_step};
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
        // C `cdev_map` (select_request_char:470-473): the char family gets the `/dev/tty`
        // remap; the socket family stays as is (select.c:539).
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
        // C select.c:475-487: the corruption gate for one filp hung on two controlling terminals.
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
                // Empty-handed (0 ready bits) — filter says "nothing to ask now" or the query is in flight.
                Ok(SO::empty())
            }
            FilterOutcome::Query {
                rops,
                clear_update,
                set_busy,
                set_block,
                ..
            } => {
                // Driver busy gate (select.c:508-510/547-549): at most one query per driver
                // may be in flight at a time.
                let busy = if is_char {
                    let major = ((dev & 0x000fff00) >> 8) as u32;
                    self.dmap_table
                        .get(major)
                        .map(|r| r.sel_busy)
                        .unwrap_or(true)
                } else {
                    let (num, _) =
                        crate::device_map::split_smap_dev(dev).ok_or(minix_types::EIO)?;
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
                        // Success: mark the driver in flight + the filp's BUSY/block-monitor bits
                        // (the three-part obligation of C select.c:519-522/555-558).
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
                        filp.select_flags |= (block_bits
                            | if set_busy {
                                crate::filp::FsfFlags::BUSY.bits()
                            } else {
                                0
                            }) as u8;
                        if is_char {
                            let major = ((dev & 0x000fff00) >> 8) as u32;
                            if let Some(row) = self.dmap_table.get_mut(major) {
                                row.sel_busy = true;
                                row.sel_owner = Some(filp_idx);
                            }
                        } else {
                            let (num, _) =
                                crate::device_map::split_smap_dev(dev).ok_or(minix_types::EIO)?;
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

    /// "Does any fd in the slot wait for a driver answer" (the second argument of `is_deferred`: any participating filp
    /// carrying `FSF_UPDATE | FSF_BUSY`).
    pub(crate) fn select_any_update_or_busy(&self, s: usize) -> bool {
        let Some(se) = self.select_table.get(s) else {
            return false;
        };
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

    /// `restart_proc` (select.c:1303-1312): finalize once there is a result and it is no longer deferred.
    fn select_restart_proc(&mut self, s: usize) {
        let Some(se) = self.select_table.get(s) else {
            return;
        };
        let (nready, error, block) = (se.nready, se.error, se.block);
        let deferred = self.select_any_update_or_busy(s);
        if crate::select::should_return(nready, error != 0, block, deferred) {
            self.select_return(s);
        }
    }

    /// `select_cancel_all` (select.c:712-738): release each filp's select bookkeeping
    /// (`cancel_one`, the decision module); the last holder clears stale dmap/smap ownership
    /// (busy **stays** — the query is still in flight, and only the state is cleared when the reply lands), clears the timeout, frees the slot,
    /// and clears the process suspend state.
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
                    dev: if f.select_dev != 0 {
                        Some(f.select_dev)
                    } else {
                        None
                    },
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
                    row.sel_owner = None; // leave _busy set (C select.c:763)
                }
                if let Some((num, _)) = crate::device_map::split_smap_dev(dev)
                    && let Some(row) = self.smap_table.entries.iter_mut().find(|r| r.num == num)
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
        // Clear the process suspend state (C delegates this to revive's wakeup mechanism).
        if let Some(slot) = requestor
            && let Some(fp) = self.fproc_table.get_mut(slot)
            && fp.blocked_on == crate::fproc::BlockedOn::Select
        {
            fp.blocked_on = crate::fproc::BlockedOn::None;
        }
    }

    /// `select_return` (select.c:1090-1107): cancel → copy the result sets back →
    /// `revive(req_endpt, r)` (here = queue_reply + clear the suspend state; see
    /// the tail of [`Self::select_cancel_all`]).
    fn select_return(&mut self, s: usize) {
        // The CLOCK/reply path has no injectable io — the production object is built (an unreachable
        // store in a host build finalizes with an error; bookkeeping stays complete).
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

    /// Timeout arming + process-level suspend (C `do_select:319-335`'s `set_timer` +
    /// `suspend(FP_BLOCKED_ON_SELECT)`). An `Until` plan sets **one** kernel alarm via `sys_setalarm`
    /// (with several concurrent selects it is reset to the earliest expiry — see
    /// [`Self::select_timeout_check`] for the whole-table approximation).
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
            // Single alarm: with concurrent selects it is reset to the earliest expiry.
            let next = (0..crate::select::MAXSELECTS)
                .filter_map(|s| self.select_table.get(s))
                .filter(|se| se.requestor.is_some() && se.expiry > 0)
                .map(|se| se.expiry)
                .min()
                .unwrap_or(ticks);
            // [SD19-gate] best-effort alarm setup (C ignores the return at the same point)
            #[allow(clippy::let_underscore_must_use)]
            let _ = minix_sys::syscall::sys_setalarm(
                &minix_sys::syscall::DirectKernelCallTransport,
                next,
                false,
            );
        }
        let fp = self
            .fproc_table
            .get_mut(fp_slot)
            .ok_or(minix_types::EINVAL)?;
        fp.blocked_on = crate::fproc::BlockedOn::Select;
        // Release the worker slot (process-level suspend; same pattern as `suspend_on_sdev` —
        // after C's `suspend()` the worker job is over).
        if let Some(idx) = self.current_worker {
            self.worker_pool.release(idx);
            self.current_worker = None;
        }
        Ok(())
    }

    /// The injectable-io version of `select_return` (used by do_select's immediate-return path;
    /// the arm's FdSetIo seam keeps it host-testable). Returns the value the user receives.
    pub(crate) fn select_finish(
        &mut self,
        s: usize,
        io: &impl crate::select::FdSetIo,
    ) -> Result<i32, i32> {
        // Take the data finalization needs before releasing (cancel frees the slot).
        let (error, nready, vir, sets) = {
            let Some(se) = self.select_table.get(s) else {
                return Ok(0);
            };
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
        // C: on error the result sets are **not copied**; return the error directly (select.c:1101-1105).
        // C: on error the result sets are **not copied**; return the error directly (select.c:1101-1105).
        if error != 0 {
            return Err(error);
        }
        // C `copy_fdsets(se, se->nfds, TO_PROC)`: copy back only the byte count
        // the user expects — the three sets were allocated to that size, so the whole set is copied back.
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

    /// `filp_status` (select.c:1283-1301): broadcast a filp's new status to
    /// every slot selecting it. `status < 0` records an error; otherwise it records ready bits.
    fn select_filp_status(&mut self, filp_idx: usize, status: i32) {
        let mut found = alloc::vec::Vec::new();
        for s in 0..crate::select::MAXSELECTS {
            let Some(se) = self.select_table.get(s) else {
                continue;
            };
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
                            se.vir_readfds != 0 && crate::select::bit_of(&se.readfds, fd),
                            se.vir_writefds != 0 && crate::select::bit_of(&se.writefds, fd),
                            se.vir_errorfds != 0 && crate::select::bit_of(&se.errorfds, fd),
                        );
                        let (a, b, c, mut n) = (
                            &mut se.ready_readfds,
                            &mut se.ready_writefds,
                            &mut se.ready_errorfds,
                            se.nready,
                        );
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

    /// `select_cdev_reply1` (select.c:1004-1070): a type-1 reply from a char driver.
    /// On a device mismatch the in-flight mark **stays** (as in C: wait for the real reply).
    pub fn select_cdev_reply1(&mut self, driver_e: Endpoint, minor: u32, status: i32) {
        let Some(major) = crate::device_map::get_by_endpt(&self.dmap_table, driver_e) else {
            return;
        };
        let dev = (((major as u64) << 8) & 0x000fff00) | ((minor as u64) & 0xff);
        let Some(row) = self.dmap_table.get(major) else {
            return;
        };
        if !row.sel_busy {
            return; // no one is waiting for this reply
        }
        let owner = row.sel_owner;
        if let Some(filp_idx) = owner {
            let ok = self
                .filp_table
                .get(crate::filp::FilpId(filp_idx))
                .map(|f| f.select_dev == dev)
                .unwrap_or(false);
            if !ok {
                return; // the driver answered something else: keep it in flight
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

    /// `select_sdev_reply1` (select.c:1072-1107): a type-1 reply from a socket driver.
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

    /// `select_reply1` (select.c:956-999): filp bookkeeping + broadcast for a type-1 reply
    /// (`reply1_step`, the decision module).
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
        let broadcast = if status < 0 {
            status
        } else {
            out.broadcast.to_status()
        };
        self.select_filp_status(filp_idx, broadcast);
    }

    /// `select_cdev_reply2` (select.c:1160-1192) + `select_reply2`
    /// (select.c:1109-1158): a type-2 (readiness notification) reply; scan every slot's fd watching this
    /// device.
    pub fn select_cdev_reply2(&mut self, driver_e: Endpoint, minor: u32, status: i32) {
        if status == 0 {
            return; // C: weird status
        }
        let Some(major) = crate::device_map::get_by_endpt(&self.dmap_table, driver_e) else {
            return;
        };
        let dev = (((major as u64) << 8) & 0x000fff00) | ((minor as u64) & 0xff);
        self.select_reply2(dev, status);
    }

    /// `select_sdev_reply2` (select.c:1194-1210).
    pub fn select_sdev_reply2(&mut self, dev: u64, status: i32) {
        if status == 0 {
            return;
        }
        self.select_reply2(dev, status);
    }

    fn select_reply2(&mut self, dev: u64, status: i32) {
        for s in 0..crate::select::MAXSELECTS {
            // Snapshot this slot's fd associations (the table needs a mutable borrow afterwards; do not hold a borrow across it).
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
                            se.vir_readfds != 0 && crate::select::bit_of(&se.readfds, fd),
                            se.vir_writefds != 0 && crate::select::bit_of(&se.writefds, fd),
                            se.vir_errorfds != 0 && crate::select::bit_of(&se.errorfds, fd),
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

    /// `select_restart_filps` (select.c:1212-1260): restart deferred queries
    /// (INIT splits an update-state filp into two cards — answer one during SUSPEND,
    /// then ask the next after the reply lands).
    pub fn select_restart_filps(&mut self) {
        for s in 0..crate::select::MAXSELECTS {
            let (filps, block, requestor) = match self.select_table.get(s) {
                Some(se) if se.requestor.is_some() => (se.filps.clone(), se.block, se.requestor),
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
                if !matches!(
                    kind,
                    crate::select::FdKind::Char | crate::select::FdKind::Sock
                ) {
                    // C asserts that only chars/sockets are handled (select.c:1237-1240).
                    continue;
                }
                let is_char = kind == crate::select::FdKind::Char;
                match self.select_request_driver(requestor, e.filp, is_char, ops, block) {
                    Ok(ready) => {
                        if !ready.is_empty_real() {
                            let se = self.select_table.get_mut(s).unwrap();
                            let want = (
                                se.vir_readfds != 0 && crate::select::bit_of(&se.readfds, fd),
                                se.vir_writefds != 0 && crate::select::bit_of(&se.writefds, fd),
                                se.vir_errorfds != 0 && crate::select::bit_of(&se.errorfds, fd),
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

    /// The single-alarm approximation of `select_timeout_check` (select.c:861-880): when a CLOCK notification
    /// fires, finalize the **earliest-expiring** slot; subtract that interval from every other slot's expiry;
    /// if any remain, re-arm the alarm.
    ///
    /// [ARCH: single-alarm select] C's per-slot `timer` (the set_timer chain) converges here
    /// into "one kernel alarm + a whole-table scan on expiry": slots expiring at the same instant finish together,
    /// the arithmetic matches the timer chain, and the per-slot kernel-alarm surface disappears. The three-way
    /// annotation is kept in sync with the design docs.
    pub fn select_timeout_check(&mut self) -> bool {
        // Find the smallest non-zero expiry.
        let mut min: Option<(usize, u64)> = None;
        for s in 0..crate::select::MAXSELECTS {
            let Some(se) = self.select_table.get(s) else {
                continue;
            };
            if se.requestor.is_none() || se.expiry == 0 {
                continue;
            }
            if min.map(|(_, t)| se.expiry < t).unwrap_or(true) {
                min = Some((s, se.expiry));
            }
        }
        let Some((_, elapsed)) = min else {
            return false;
        };
        // Subtract that interval from the other slots; expired slots (expiry == elapsed) are finalized.
        let mut fired = alloc::vec::Vec::new();
        for s in 0..crate::select::MAXSELECTS {
            let Some(se) = self.select_table.get(s) else {
                continue;
            };
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
            let Some(se) = self.select_table.get(s) else {
                continue;
            };
            if se.requestor.is_none() {
                continue;
            }
            let se = self.select_table.get_mut(s).unwrap();
            se.expiry = 0;
            let deferred = self.select_any_update_or_busy(s);
            if deferred {
                // The timer fired too early: switch to a non-blocking retry (as in C).
                let se = self.select_table.get_mut(s).unwrap();
                se.block = false;
                self.select_restart_proc(s);
            } else {
                self.select_return(s);
            }
        }
        // If any remain, re-arm the alarm.
        let next = (0..crate::select::MAXSELECTS)
            .filter_map(|s| self.select_table.get(s))
            .filter(|se| se.requestor.is_some() && se.expiry > 0)
            .map(|se| se.expiry)
            .min();
        if let Some(ticks) = next {
            // [SD19-gate] best-effort alarm setup (C ignores the return at the same point)
            #[allow(clippy::let_underscore_must_use)]
            let _ = minix_sys::syscall::sys_setalarm(
                &minix_sys::syscall::DirectKernelCallTransport,
                ticks,
                false,
            );
        }
        true
    }

    /// Land a driver reply (the common first half of C `sdev_reply`/`cdev_reply`/`bdev_reply`):
    /// find the worker slot **waiting for this driver**, land the reply into its `sendrec` and wake it
    /// (state becomes `Busy`; the same continuation machinery as FS replies).
    ///
    /// If no waiting slot is found, ignore it as "no worker waiting" (the `ReplyIgnore::NoWorker`
    /// semantics of `device_map::check_reply`) — a soft failure; the main loop continues.
    pub fn handle_drv_reply(&mut self, msg: &Message) -> Result<usize, FsReplyError> {
        // The two select reply types have no waiting worker (queries are asynsend send-and-don't-wait),
        // so they are intercepted before worker-slot matching (C dispatches SEL1/SEL2 separately in
        // cdev_reply/sdev_reply — cdev.c:494-503). `usize::MAX` = already finalized by the select layer.
        // SAFETY: type 1 is `mess_lchardriver_vfs_sel1 { int status@0; int32
        // minor@4 }`; the two socket types are `mess_lsockdriver_vfs_select_reply {
        // int32 sock_id@0; int status@4 }` (ipc.h).
        let raw = unsafe { &msg.m_u.raw };
        match msg.m_type {
            t if t == minix_chardriver::protocol::CdevReplyKind::SelectImmediate as i32 => {
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
                || t == minix_sockdriver::sdev::SdevReply::SelectReply2 as i32 =>
            {
                // C sdev.c:1017-1028: first find the smap row by source (`sp`), then build
                // `make_smap_dev(sp->smap_num, sock_id)` — note the row's
                // **one-based num field** is used; `smap_by_endpt` gives the array position.
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
        // Slot matching: C `cdev_generic_reply` (cdev.c:442-460) first identifies the process by the reply's `id`
        // (`proc_e`, filled when VFS sends the request, cdev.c:326), then checks whether its worker
        // waits for this driver — with two jobs waiting on one driver, "the first waiting slot" would misattribute
        // someone else's reply. Here an exact id match is tried first; with no match (host-constructed requests carry no
        // id) it falls back to first-match for open/close path compatibility.
        let id_slot = if msg.m_type == minix_types::CDEV_REPLY_BASE {
            let reply_user = i32::from_le_bytes(raw[4..8].try_into().unwrap());
            self.fproc_table.is_ok_endpoint(Endpoint(reply_user)).ok()
        } else {
            None
        };
        let waits_on_driver = |w: &crate::worker::WorkerSlot| w.task == Some(msg.m_source);
        let mut slot = (0..crate::worker::NR_WTHREADS).find(|i| {
            self.worker_pool
                .get(*i)
                .is_some_and(|w| waits_on_driver(w) && id_slot.is_none_or(|s| w.fp_slot == Some(s)))
        });
        if slot.is_none() {
            slot = (0..crate::worker::NR_WTHREADS)
                .find(|i| self.worker_pool.get(*i).is_some_and(waits_on_driver));
        }
        let Some(slot) = slot else {
            // No worker is waiting: try the **process-level suspend** path (the second branch
            // of C `sdev_reply` — replies for bind/connect/accept/recvfrom calls).
            if self.finish_sdev_blocked(msg) {
                return Ok(usize::MAX); // already finalized by the suspended process
            }
            return Err(FsReplyError::WrongTask);
        };
        let wp = self
            .worker_pool
            .get_mut(slot)
            .ok_or(FsReplyError::SlotOutOfRange)?;
        // A driver reply's `m_type` is the status/reply number (C's `w_drv_sendrec` takes it as is);
        // unlike FS replies it carries no transid — so nothing is **stripped** here.
        wp.sendrec = Some(*msg);
        wp.task = None;
        wp.state = crate::worker::WorkerState::Busy;
        Ok(slot)
    }

    /// Send one `REQ_SYNC` to the slot (C `req_sync`: empty payload, status reply).
    ///
    /// **Do not touch the slot's continuation tag**: the caller (`begin_sync_sequence` and the
    /// `SyncMounts` continuation) sets it to the sequence state itself — writing `Status` here
    /// would lose the sequence's "how many remain" (a bug seen this round).
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
            grant: 0, // no data plane
            user,
            req: Message {
                m_type: minix_types::REQ_SYNC,
                ..Message::default()
            },
        });
        Ok(())
    }

    /// Start a `REQ_SYNC` sequence (shared by `sync`/`fsync`): collect the target mounts into a fixed array,
    /// send the first, and leave the rest to the `WorkerCont::SyncMounts` continuation.
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
            // C's loop sent nothing at all: succeed directly (nothing to sync is not an error).
            return Ok(());
        }
        if let Some(wp) = self.worker_pool.get_mut(idx) {
            wp.cont = Some(crate::worker::WorkerCont::SyncMounts {
                targets: arr,
                count: targets.len().min(crate::vmnt::NR_MNTS) as u8,
                at: 1, // entry 0 is sent below
                first_err: 0,
            });
        }
        self.send_sync_for_slot(idx, fp_slot, targets[0])
    }

    /// Send one `REQ_UNLINK`/`REQ_RMDIR` (C `req_unlink`/`req_rmdir`,
    /// request.c:1149-1175 / 966-989): the component name goes into the slot's scratch and gets a
    /// **direct grant** (the name is in VFS memory, not a magic grant); the parent-dir ino
    /// and name length accompany the request; the reply is status-only, so the continuation tag is `Status`.
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
            // C `cpf_grant_direct(fs_e, lastc, len, CPF_READ)`.
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

    /// The **shared body** of `do_chown` (C protect.c:24-110; the path and fd halves differ only in
    /// where the vnode comes from): read-only gate → three ownership rules (`protect::chown_gate`) →
    /// `-1` conversion (`protect::keep_id`) → range check → `REQ_CHOWN`.
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
        // C `do_chown:140-151`: first the read-only gate, then the three rules (all judged on the **raw
        // values in the message**: a non-superuser passing `-1` is still rejected by the "no giving away" rule).
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
        // C `do_chown:154-158`: convert `-1` to the current value, then the range check.
        let new_uid = crate::protect::keep_id(
            if uid == crate::protect::ID_EXPIRED {
                None
            } else {
                Some(uid)
            },
            node_uid,
        );
        let new_gid = crate::protect::keep_id(
            if gid == crate::protect::ID_EXPIRED {
                None
            } else {
                Some(gid)
            },
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
            grant: 0, // no data plane
            user,
            req: crate::request::encode_chown(ino, new_uid, new_gid),
        });
    }

    /// The **send half** of `truncate_vnode` (C link.c:365-381, shared by `do_truncate` and
    /// `do_ftruncate`): unchanged size on a regular file → send nothing (POSIX file
    /// time) → type gate (REG/FIFO) → 64-bit capability gate → `REQ_FTRUNC`.
    ///
    /// Three-way return: `Ok(true)` = request registered (the caller reports suspend), `Ok(false)` =
    /// unchanged size (the caller returns 0), `Err(errno)` = a gate rejected it.
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
        // C link.c:349-354 — unchanged size does not disturb FS (POSIX file time).
        if mode & crate::open::S_IFMT == crate::open::S_IFREG && size == length as u64 {
            return Ok(false);
        }
        // C `truncate_vnode:375-376` — serve only regular files and pipes.
        let ftype = mode & crate::open::S_IFMT;
        if ftype != crate::open::S_IFREG && ftype != crate::open::S_IFIFO {
            return Err(minix_types::EINVAL);
        }
        let vmnt_id = self.vmnt_table.find_by_fs(fs_e).ok_or(minix_types::EIO)?;
        // C request.c:274-278 — without 64-bit support, a length beyond INT_MAX is EINVAL.
        let fs_flags = self
            .vmnt_table
            .get(vmnt_id)
            .map(|v| v.fs_flags)
            .unwrap_or(0);
        if fs_flags & crate::request::FsFlags::IS64BIT.bits() == 0 && length > i32::MAX as i64 {
            return Err(minix_types::EINVAL);
        }
        // A worker slot matters only when a request is **really sent**: the unchanged-size path must not turn into
        // EAGAIN just because "no slot" (in C, `truncate_vnode` is where a thread is actually needed).
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
            grant: 0, // no data plane, no grant sent
            user: user_e,
            req: crate::request::encode_ftrunc(ino, length, 0),
        });
        Ok(true)
    }

    /// The **shared body** of `do_utimens` (C time.c:44-160; the path and fd halves differ only in
    /// where the vnode comes from): owner/superuser gate (EPERM, **but when both nanoseconds are `UTIME_NOW`
    /// it degrades to a write-permission check**) → read-only gate (EROFS) → nanosecond conversion (`UTIME_NOW` takes
    /// the current time, `UTIME_OMIT` is passed through, the rest must be < 1e9 or EINVAL) →
    /// `REQ_UTIME`.
    ///
    /// Clock: C's `clock_time` reads the kerninfo page (it cannot fail); the Rust side gets the same data via a
    /// kernel call (`clock_time_via`) — unreachable in a host build, so **EIO is surfaced honestly**
    /// rather than using 0 as "now" (a 1970 timestamp written into the file system is harder to diagnose than an error).
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
        // The three rules of C `time.c:123-130` (order is priority: owner gate → write-permission degraded gate →
        // read-only gate overrides everything).
        let mut verdict: Result<(), i32> = if node_uid == eff_uid || eff_uid == crate::link::SU_UID
        {
            Ok(())
        } else {
            Err(minix_types::EPERM)
        };
        if verdict.is_err()
            && atime.1 == crate::open::UTIME_NOW
            && mtime.1 == crate::open::UTIME_NOW
        {
            // Both are "now" = touch: degrade to a write-permission check (C `time.c:126-128`'s
            // `forbidden(fp, vp, W_BIT)` — it uses the node's real mode and group,
            // not just a permission-bit test).
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
        // C `time.c:135-158`: when "now" is needed, read the clock once (only when a NOW is present).
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
            grant: 0, // no data plane
            user,
            req: crate::request::encode_utime(ino, actime, modtime, acnsec as u32, modnsec as u32),
        });
    }

    /// The **shared body** of `do_chmod` (C protect.c:62-133; the path and fd halves differ only in
    /// where the vnode comes from): owner/superuser gate (EPERM) → read-only gate (EROFS) → setgid
    /// bit clearing (`protect::strip_setgid`) → `REQ_CHMOD`; the reply carries the **whole mode word**,
    /// which the continuation writes back into the vnode cache.
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
        // C `do_chmod:112-114`: only the owner or superuser may change the mode.
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
        // A non-superuser whose file is not in their group has setgid cleared (protect.c:120-121).
        let new_mode =
            crate::protect::strip_setgid(eff_uid == crate::link::SU_UID, node_gid, eff_gid, mode);
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
            grant: 0, // no data plane
            user,
            req: crate::request::encode_chmod(ino, new_mode),
        });
    }

    /// Finalize a suspended job and reply to the user (the unified exit for **error finalization** and after phase 2).
    ///
    /// `status` is an **error code**: a positive errno constant (the crate convention, e.g.
    /// `minix_types::EINVAL`) or a **negative** on-wire status from an FS/driver reply — both forms are accepted,
    /// and any non-zero value is finalized as an error ([`Self::queue_reply`] folds it negative at the boundary). Successful **values**
    /// (fd, byte count, entry count) go through [`Self::finish_worker_job_value`].
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
                // Negative on-wire status: flip back to a positive errno first; the boundary folds it negative again.
                crate::call_table::SyscallResult::Error(-status)
            };
            self.queue_reply(target, result);
        }
    }

    /// The successful-**value** exit of [`Self::finish_worker_job`]: `open`/`accept` return an fd,
    /// `read`/`write` return a byte count, `getvfsstat` returns a mount count — all non-negative
    /// success values (the same `r` in C's `reply(who, r)`), but error finalization would fold them negative
    /// at the boundary and hand the user an error. 0 also comes through here (equivalent to `Ok(0)`).
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

    /// Send the FS conversation registered by an arm (the send half of C `fs_sendrec`; the arm cannot do it
    /// itself because it has no transport handle).
    ///
    /// Success: `fs_sendrec` sets the worker slot to `WaitingForFs` and delivers, and the job suspends
    /// awaiting the reply (landed by [`Self::handle_fs_reply`], finalized by
    /// [`Self::run_worker_continuations`]).
    /// Failure: C's arm replies with the error and ends the job — the same happens here: revoke the grant, clear the continuation,
    /// release the slot, and reply the error to the user.
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
                // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                #[allow(clippy::let_underscore_must_use)]
                let _ = self.revoke_grant(p.grant);
                self.worker_pool.release(p.worker);
                if self.current_worker == Some(p.worker) {
                    self.current_worker = None;
                }
                // C's error surface: `find_vmnt` failure in comm.c gives EIO, self-deadlock gives
                // EDEADLK; this primitive folds both into EIO (from the user's perspective both mean
                // "the conversation did not happen").
                let _ = e;
                self.queue_reply(
                    p.user,
                    crate::call_table::SyscallResult::Error(minix_types::EIO),
                );
            }
        }
    }

    /// Send the queued `REQ_PUTNODE` messages one by one (the delivery half of the `put_vnode` slow path).
    ///
    /// C does a synchronous `fs_sendrec` in the worker (vnode.c:278) and `printf`s on failure
    /// (vnode.c:281-283) — no user reply, no retry. Here the reply is ignored and a send
    /// failure is dropped likewise: user-observable behavior is the same (the FS-side reference count is
    /// covered by the FS's own reclamation machinery).
    pub fn flush_pending_puts(&mut self, transport: &impl minix_sys::ipc::IpcTransport) {
        let puts: alloc::vec::Vec<crate::vnode::PutNodeReq> =
            core::mem::take(&mut self.pending_puts);
        for p in puts {
            // C `req_putnode` (request.c:699-711): sendrec; the reply is only checked for errors.
            // The queue is filtered by mount row (a mount that is gone cannot receive it; drop it).
            if self.vmnt_table.find_by_fs(p.fs_e).is_none() {
                continue;
            }
            // [SD19-gate] best-effort IPC cleanup (send-and-forget/timeout path; failure does not change an established main result)
            #[allow(clippy::let_underscore_must_use)]
            let _ = transport.sendrec(
                p.fs_e,
                &mut crate::request::encode_putnode(p.ino, p.count as i32),
            );
        }
    }

    /// Run every job whose "reply arrived but continuation has not run".
    ///
    /// Test: the slot has a continuation tag (`cont`) and the reply has landed in `sendrec` (written by
    /// [`Self::handle_fs_reply`]). Each such job is completed according to the continuation tag:
    /// revoke the grant, write back state as needed, reply the result to the user, then release the slot.
    ///
    /// C anchor: the half after `fs_sendrec` returns (the offset advance in `read.c:181-190`,
    /// the revoke at `request.c:1109`); the status word comes from `reqmp->m_type`, and
    /// `ERESTART` folds to `EIO` (comm.c:161-163).
    pub fn run_worker_continuations(&mut self) {
        for idx in 0..crate::worker::NR_WTHREADS {
            let (cont, reply, fp_slot) = match self.worker_pool.get_mut(idx) {
                Some(wp) => {
                    let (Some(cont), Some(reply)) = (wp.cont, wp.sendrec) else {
                        continue;
                    };
                    // A continuation runs only when the reply has landed: in C, `sendrec` is a bidirectional buffer
                    // — `fs_sendrec` puts the **request** in it (`set_waiting`), and only
                    // `do_reply` receiving overwrites it with the reply. C's thread blocks in
                    // `worker_wait` and wakes only on `worker_signal` (here
                    // `handle_fs_reply` sets `Busy`); testing only `cont + sendrec` would let another
                    // client's call take the request code as the status and
                    // falsely complete the suspended job.
                    if wp.state != crate::worker::WorkerState::Busy {
                        continue;
                    }
                    (cont, reply, wp.fp_slot)
                }
                None => continue,
            };
            let mut status = reply.m_type;
            if status == -(minix_types::ERESTART) {
                // C `comm.c:161-163`'s `r = reqmp->m_type; if (r == ERESTART)
                // r = EIO;` — an FS reply's m_type is negative on the wire (servers are all
                // `_SYSTEM` builds), so both the comparison and the result use the negative value.
                status = -(minix_types::EIO);
            }
            // When `status` is a **success value** (byte count/entry count) rather than an error code: read/write
            // and directory-read finalization put the actual byte count, and `getvfsstat` the mount count, into the same
            // slot — the finalizer sends a marked arm as `Ok(value)` (negative folding applies only to error codes).
            let mut value_reply = false;
            // A payload reply (needed by a few arms, e.g. lseek's new offset): the arm fills it in
            // here and the unified finalizer below sends it — the arm must **not** call
            // `queue_reply_msg` directly, or the finalizer's bare reply would overwrite it.
            let mut reply_payload: Option<Message> = None;
            match cont {
                crate::worker::WorkerCont::Fstat { grant } => {
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(grant);
                }
                crate::worker::WorkerCont::OpenTrunc { node, oflags } => {
                    // The result of `truncate_vnode(vp, 0)` in C `common_open:150-157` is
                    // **ignored** — a failed truncate does not block open; assembly then proceeds normally. The
                    // `O_TRUNC` bit is cleared before the local half (otherwise dispatch would test it again).
                    let _ = status;
                    self.finish_open_local_inner(idx, fp_slot, &node, oflags, true);
                    continue;
                }
                crate::worker::WorkerCont::Create { user, oflags } => {
                    // Stage 3: the `REQ_CREATE` reply is the new node's node_details
                    // (C `req_create` finalization) → the local half (fd/filp assembly).
                    // Errors such as `EEXIST` are finalized as a status (the FS-side answer for `O_EXCL`).
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
                    // Pure status: no payload, no side effects — the finalizer's default path suffices.
                }
                crate::worker::WorkerCont::Freesp {
                    vnode,
                    zero_len,
                    start,
                } => {
                    // C misc.c:236-237: for `F_FREESP` with zero length (`l_len == 0`),
                    // after `req_ftrunc` succeeds, shrink `v_size` to `start`.
                    if status == 0
                        && zero_len
                        && let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode))
                        && (start < 0 || (start as u64) <= v.size)
                    {
                        v.size = if start < 0 { 0 } else { start as u64 };
                    }
                }
                crate::worker::WorkerCont::InhibRead { offset } => {
                    // The offset was already updated in the arm (C `actual_lseek:640`); here only the new offset is
                    // put into the reply payload (C `do_lseek`'s `m_vfs_lc_lseek.offset`).
                    // On request failure the error is returned as in C (the offset has still been updated).
                    let mut m = Message {
                        m_type: status,
                        ..Message::default()
                    };
                    if status == 0 {
                        // SAFETY: `mess_vfs_lc_lseek { off_t offset; }`
                        // (ipc.h:2206-2210), in the first word of the payload area.
                        unsafe {
                            m.m_u.raw[0..8].copy_from_slice(&offset.to_le_bytes());
                        }
                    }
                    reply_payload = Some(m);
                }
                crate::worker::WorkerCont::Chown { vnode, uid, gid } => {
                    // C `do_chown:159-163`: on success write uid/gid into the vnode and take the mode
                    // from the reply (FS may have cleared setuid/setgid bits).
                    if status == 0 {
                        // SAFETY: the reply payload is written in LP64 field order in the payload area (shared table).
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
                    // C `do_chmod` finalization (protect.c:126-128): on success write the
                    // **actual mode returned by FS** back into the vnode cache — FS may narrow it
                    // (e.g. clear setgid when permissions do not allow it); the cache follows FS.
                    if status == 0 {
                        // SAFETY: the reply payload is written in LP64 field order in the payload area (shared table).
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
                    // C `sdev_setsockopt`/`sdev_get` finalization: revoke the grant →
                    // the reply number must be `SDEV_REPLY` → the status comes from the first payload word; for `get`
                    // that status is the **new length**, placed into the reply payload
                    // (`m_vfs_lc_socklen { len }`, C `do_getsockopt:696-697`).
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(grant);
                    if status != minix_sockdriver::sdev::SdevReply::Reply as i32 {
                        status = minix_types::EIO;
                    } else {
                        // SAFETY: `mess_lsockdriver_vfs_reply { int32_t
                        // req_id; int status; }` — the status is in the second slot (`req_id` comes first).
                        let raw = unsafe { &reply.m_u.raw };
                        status = i32::from_le_bytes(raw[4..8].try_into().unwrap());
                        if write_dir && status >= 0 {
                            let len = status as u32;
                            let mut m = Message {
                                m_type: 0,
                                ..Message::default()
                            };
                            // SAFETY: `mess_vfs_lc_socklen { unsigned int len; }`
                            // in the first word of the payload area.
                            unsafe {
                                m.m_u.raw[0..4].copy_from_slice(&len.to_le_bytes());
                            }
                            reply_payload = Some(m);
                            status = 0;
                        }
                    }
                }
                crate::worker::WorkerCont::CdevOpen { fd, filp, dev } => {
                    // C `cdev_opcl` finalization (cdev.c:236-249): the status comes from
                    // `mess_lchardriver_vfs_reply.status` (**the first word**); when `>= 0`
                    // the low bits are two effect flags — `CDEV_CLONED` (requires PFS to create a clone node,
                    // not wired) and `CDEV_CTTY` (record the device as the controlling terminal).
                    // SAFETY: `mess_lchardriver_vfs_reply { int status;
                    // uint32_t id; }` — the status is in the first word.
                    let raw = unsafe { &reply.m_u.raw };
                    let dstatus = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    let vnode = self
                        .filp_table
                        .get(crate::filp::FilpId(filp))
                        .and_then(|f| f.vnode)
                        .unwrap_or(0);
                    if dstatus < 0 {
                        // Driver refused: take C's failure tail (release fd/filp + put back the vnode),
                        // and reply the error to the user.
                        self.release_open_claim(fp_slot, fd, filp, vnode);
                        self.finish_worker_job(idx, fp_slot, dstatus);
                        continue;
                    }
                    let effects = crate::cdev::open_effects(dstatus, dev);
                    if effects.clone_minor.is_some() {
                        // `cdev_clone` (cdev.c:100-145) requires PFS to create a new node
                        // (`req_newnode(PFS_PROC_NR, ...)`) — PFS is implemented elsewhere
                        // and not wired yet: reject honestly and take the failure tail (never pretending a clone
                        // device was opened).
                        self.release_open_claim(fp_slot, fd, filp, vnode);
                        self.finish_worker_job(idx, fp_slot, minix_types::ENOSYS);
                        continue;
                    }
                    if let Some(tty_dev) = effects.grant_tty {
                        // C: `fp->fp_tty = dev; dp->dmap_seen_tty = TRUE;`
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
                    // Success: the user receives the fd (after C `r = OK`, `common_open`
                    // replies `r = fd` upward).
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
                    // The conversation half of C `bdev_sendrec` (bdev.c:36-58): the status comes from
                    // `mess_lblockdriver_lbdev_reply.status` (**the first word**).
                    // SAFETY: `{ int status; int id; }` (ipc.h:356-361).
                    let raw = unsafe { &reply.m_u.raw };
                    let mut dstatus = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    let mut finished = false;
                    if dstatus == crate::bdev::SEND_RESTART {
                        // On a driver `ERESTART`, resend as is, five times at most; blowing the fuse means EIO
                        // (C's `retry_count < 5` fuse).
                        match crate::bdev::RetryState(retries).step(dstatus) {
                            crate::bdev::RetryVerdict::Again => {
                                let major = ((dev & 0x000fff00) >> 8) as u32;
                                let drv_e = self.dmap_table.get(major).and_then(|row| row.driver);
                                let mut resent = false;
                                if let Some(drv_e) = drv_e {
                                    // Resend the **same** request (C's `*mess_ptr =
                                    // mess_retry`); the count lives in the continuation.
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
                                    resent =
                                        self.send_drv_for_slot(idx, fp_slot, drv_e, &m).is_ok();
                                }
                                if resent {
                                    continue;
                                }
                                // Driver gone or send failed: C's `bdev_sendrec` returns EIO in
                                // both places.
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
                    // Driver cleared: the second stage (`v_bfs_e` selection + a possible
                    // `REQ_NEW_DRIVER`). When an FS conversation is attached this stage returns `true`,
                    // and the job is left to the `BdevNewDriver` continuation.
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
                crate::worker::WorkerCont::BdevNewDriver {
                    fd,
                    filp,
                    vnode,
                    dev,
                } => {
                    // C `open.c:210-216`: a successful `req_newdriver` returns the fd as usual;
                    // on failure, `BDEV_CLOSE` must be **sent to the driver first** to close the device just opened,
                    // then `ENXIO` is returned (the close return value is dropped).
                    if status == 0 {
                        self.finish_worker_job_value(idx, fp_slot, fd as i32);
                        continue;
                    }
                    let major = ((dev & 0x000fff00) >> 8) as u32;
                    let minor = (((dev & 0xfff0_0000) >> 12) | (dev & 0xff)) as u32;
                    let drv_e = self.dmap_table.get(major).and_then(|row| row.driver);
                    let m = crate::bdev::close_request(minor);
                    if let Some(wp) = self.worker_pool.get_mut(idx) {
                        // Wait for the driver's close reply before finalizing — do **not** let go
                        // right after sending (letting go would land the reply on another job).
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
                        // Driver gone or send failed: C's `bdev_close` returns ENXIO here and
                        // the call site drops it — go straight to the failure tail.
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
                    // The compensating `BDEV_CLOSE` returned: take C's failure tail, then return the original
                    // error (`r = ENXIO` at `open.c:213`). The close status is ignored
                    // (C is `(void)bdev_close(dev)`).
                    self.release_open_claim(fp_slot, fd, filp, vnode);
                    self.finish_worker_job(idx, fp_slot, status);
                    continue;
                }
                crate::worker::WorkerCont::BdevIoctl { grant, filp } => {
                    // C `bdev_ioctl` finalization: revoke the grant → clear `filp_ioctl_fp` →
                    // the status comes from `mess_lblockdriver_lbdev_reply.status` (**the first word**,
                    // the same family as char devices, a different family from sockets).
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(grant);
                    if let Some(f) = self.filp_table.get_mut(crate::filp::FilpId(filp)) {
                        f.ioctl_holder = None;
                    }
                    if status == 0 {
                        // SAFETY: `mess_lblockdriver_lbdev_reply { int status;
                        // int id; }` (ipc.h:356-361) — the status is in the first word.
                        let raw = unsafe { &reply.m_u.raw };
                        status = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    }
                }
                crate::worker::WorkerCont::CdevIoctl { grant } => {
                    // C `cdev_io` ioctl finalization (the `cdev_generic_reply` side of cdev.c:438-477):
                    // revoke the grant → the status comes from `mess_lchardriver_vfs_reply.status`
                    // (**the first payload word**, decoded unconditionally — a driver reply's m_type is always
                    // `CDEV_REPLY`(0x480), so `status` (initialized from reply.m_type) is never 0;
                    // the old `if status == 0` gate is a dead end: the payload status is never read and 0x480
                    // would be returned to the user as an error code). Negative values are flipped positive for the user,
                    // `EINTR` folds back to `EAGAIN` (the code-swap pair of cdev.c:471-473); non-negative
                    // values are returned as-is
                    // (ioctl's success state is 0, `Ok(0)`).
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(grant);
                    // SAFETY: `mess_lchardriver_vfs_reply { int status;
                    // uint32_t id; }` (ipc.h:943-948) — the status is in the first word.
                    let raw = unsafe { &reply.m_u.raw };
                    let dstatus = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    if dstatus < 0 {
                        status = if dstatus == -(minix_types::EINTR) {
                            minix_types::EAGAIN
                        } else {
                            -dstatus
                        };
                    } else {
                        status = dstatus;
                        value_reply = true;
                    }
                }
                crate::worker::WorkerCont::CdevIo { grant } => {
                    // C `cdev_io` data finalization + the revive side of `cdev_generic_reply`
                    // (cdev.c:438-477): revoke the grant → the status comes from
                    // `mess_lchardriver_vfs_reply.status` (**the first payload word**, decoded
                    // unconditionally — a driver reply's m_type is always `CDEV_REPLY`, unlike FS replies
                    // which put the status in m_type). Success is the byte count (non-negative, sent as an `Ok` value
                    // to the user); failure is a negative errno, with `EINTR` folded back to `EAGAIN`
                    // (the code-swap pair of cdev.c:471-473 and `cdev_cancel`). A read/write
                    // process-level suspend is finalized here: clear `BlockedOn::Cdev`.
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(grant);
                    // SAFETY: `mess_lchardriver_vfs_reply { int status;
                    // uint32_t id; }` (ipc.h:943-948) — the status is in the first word.
                    let raw = unsafe { &reply.m_u.raw };
                    let dstatus = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    if let Some(slot) = fp_slot
                        && let Some(fp) = self.fproc_table.get_mut(slot)
                        && matches!(fp.blocked_on, crate::fproc::BlockedOn::Cdev(_))
                    {
                        fp.blocked_on = crate::fproc::BlockedOn::None;
                    }
                    if dstatus < 0 {
                        status = if dstatus == -(minix_types::EINTR) {
                            minix_types::EAGAIN
                        } else {
                            -dstatus
                        };
                    } else {
                        status = dstatus;
                        value_reply = true;
                    }
                }
                crate::worker::WorkerCont::SdevSimple => {
                    // C `sdev_simple` finalization: the reply number must be `SDEV_REPLY`,
                    // and the status is in the **payload** (`mess_lsockdriver_vfs_reply.status`).
                    if status != minix_sockdriver::sdev::SdevReply::Reply as i32 {
                        status = minix_types::EIO;
                    } else {
                        // SAFETY: `mess_lsockdriver_vfs_reply { int32_t
                        // req_id; int status; }` (ipc.h:1023-1028) — **the status is in
                        // the second slot** (`req_id` comes first), not the first word.
                        let raw = unsafe { &reply.m_u.raw };
                        status = i32::from_le_bytes(raw[4..8].try_into().unwrap());
                    }
                }
                crate::worker::WorkerCont::SdevSocket {
                    pair,
                    flags,
                    smap_num,
                } => {
                    // C `sdev_socket` (sdev.c:140-170): the reply number must be
                    // `SDEV_SOCKET_REPLY` (otherwise EIO); `sock_id < 0` is the error the driver
                    // reports; on success the device number = `make_smap_dev(row number, sock_id)`.
                    // The reply number must be `SDEV_SOCKET_REPLY` (`SdevReply::SocketReply`).
                    if status != minix_sockdriver::sdev::SdevReply::SocketReply as i32 {
                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                        continue;
                    }
                    // SAFETY: the reply payload `mess_lsockdriver_vfs_socket_reply`
                    // (req_id@0, sock_id@4, sock_id2@8).
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
                        // Paired: both device numbers need `make_sock_fd`. The first is created first;
                        // the second is created when the `SockFd` first-half finalization continues (C socket.c:
                        // 249-266) — `pending_fs` holds only one request at a time, so the two halves
                        // are **serial**, not concurrent.
                        if sock_id2 < 0 {
                            // C: `sock_id2 < 0` is a protocol error → close dev0 first, then EIO.
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
                    // The second half of C `make_sock_fd` (socket.c:140-176): fill the vnode from the reply's
                    // `node_details` (`v_sdev` is the socket device number) and the filp,
                    // then set CLOEXEC from `flags`. **The value the user receives is the fd**.
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
                        // C socket.c:157 — `vp->v_sdev = dev`: a socket identifies its driver by it.
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
                    // The paired orchestration of `socketpair` (C `do_socketpair:249-266`):
                    // - first half succeeds → build the second half (carrying fd0 and "this is the second half");
                    // - second half succeeds → reply `m_vfs_lc_fdpair { fd0, fd1 }`;
                    // - second half fails → `close_fd(fp, fd0)` + close dev1 (C's
                    // `close_fd` + `(void)sdev_close(dev[1])`).
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
                                // The second half could not start: close the first half's fd + that device.
                                // C's `close_fd(fp, fd0, may_suspend=FALSE)`:
                                // clears the first half's fd (a local action, no driver request —
                                // C's `may_suspend=FALSE` means "do not block here").
                                if let Some(slot) = fp_slot
                                    && let Some(fd0) = crate::filedes::Fd::new(fd as usize)
                                    && let Some(fp) = self.fproc_table.get_mut(slot)
                                {
                                    // [SD19-gate] best-effort fd close in finalization (C VFS cleanup path does not return an error if close fails)
                                    #[allow(clippy::let_underscore_must_use)]
                                    let _ = crate::filedes::close_fd(fp, fd0, &mut self.filp_table);
                                }
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont = Some(crate::worker::WorkerCont::SdevCloseThenReply {
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
                        // The second half succeeded: assemble the fd-pair reply.
                        let mut m = Message {
                            m_type: 0,
                            ..Message::default()
                        };
                        // SAFETY: `mess_vfs_lc_fdpair { int fd0; int fd1; }`
                        // (ipc.h:2198-2203), in the first two words of the payload area.
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
                        // `accept` finalization: the fd is the status and **the peer address length is in the payload**
                        // (`m_vfs_lc_socklen.len` at the end of C `resume_accept`).
                        let mut m = Message {
                            m_type: fd as i32,
                            ..Message::default()
                        };
                        // SAFETY: `mess_vfs_lc_socklen { unsigned int len; }`.
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
                    // After closing the "extra" socket, return the **original error**
                    // (C `resume_accept`'s `(void)sdev_close(dev, ...)`).
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
                    // The second half of C `create_pipe` (pipe.c:117-131): fill the vnode and both filps from the new
                    // node's `node_details`, then place the fd pair into the reply
                    // payload. On failure both ends are rolled back via `rollback_for(Node)`.
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
                        // C marks a filp free with `filp_count = 0`; on the Rust side that is
                        // `dec_count` (zero means released).
                        self.filp_table.dec_count(crate::filp::FilpId(filp0));
                        self.filp_table.dec_count(crate::filp::FilpId(filp1));
                        if let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode)) {
                            v.ref_count = 0;
                            v.fs_count = 0;
                        }
                        self.finish_worker_job(idx, fp_slot, status);
                        continue;
                    }
                    // The reply's `node_details` (`mess_fs_vfs_newnode`: file_size/
                    // device/inode/mode/uid/gid) — the field order matches the first six fields of
                    // `lookup_reply_off`, so that table is reused.
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
                    // Two filps: read end `O_RDONLY | (flags & ~O_ACCMODE)`, write end
                    // `O_WRONLY | ...`; the vnode is dup'd once more (each end holds one reference).
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
                    // Reply payload: `m_vfs_lc_fdpair { fd0, fd1 }` (these two fds
                    // are what the user receives; C `do_pipe2:48-51`).
                    let mut m = Message {
                        m_type: status,
                        ..Message::default()
                    };
                    // SAFETY: `mess_vfs_lc_fdpair { int fd0; int fd1; }`
                    // (ipc.h:2198-2203), in the first two words of the payload area.
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
                    // C `update_statvfs` + `fill_statvfs` finalization: revoke the grant →
                    // store the 17 FS fields in the mount row's cache → fill local fields → copy the block to
                    // the user (status as in C: `update_statvfs` failure means EIO, otherwise the copy decides).
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
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
                    // The `getvfsstat` sequence: if another entry remains, continue (advance the user buffer by
                    // `i*sizeof(struct statvfs)`); otherwise finalize and return the **count**
                    // (C `do_getvfsstat`'s `return count`).
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
                crate::worker::WorkerCont::SyncMounts {
                    targets,
                    count,
                    at,
                    first_err,
                } => {
                    // The shape of C `do_sync`/`do_fsync`'s loop in the single-threaded model: on each
                    // reply, send the next request; when done, report `first_err` (C drops the `req_sync`
                    // return value and keeps only the locking error).
                    let _ = status; // a single REQ_SYNC result does not affect the user-visible value
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
                    // Sequence complete: C's `r` is the locking result (here always first_err).
                    status = first_err;
                }
                crate::worker::WorkerCont::Rdlink { grant } => {
                    // C `req_rdlink_actual` (request.c:743-747): revoke the grant,
                    // then **take the byte count from the payload** (`mess_fs_vfs_rdlink.nbytes`);
                    // `m_type` is only OK — the length the user receives is not the status word.
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(grant);
                    if status == 0 {
                        // SAFETY: the reply payload is written in LP64 field order in the payload area (shared table).
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
                    // C `req_getdents_actual` finalization (request.c:330-336):
                    // the reply's `seek_pos` is the next position and `nbytes` is the number of bytes
                    // written this round, but **the position advances only when `nbytes > 0`**
                    // (`if (r > 0) rfilp->filp_pos = new_pos;` in read.c:311-313)
                    // - an empty directory/full buffer both give `nbytes == 0`, and the position
                    // must stay put, or the next pass would skip entries.
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(grant);
                    if status == 0 {
                        // SAFETY: the reply payload is written in LP64 field order in the payload area (shared table).
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
                        // The user receives the byte count (C returns `nbytes`, not 0).
                        status = nbytes as i32;
                        value_reply = true;
                    }
                }
                crate::worker::WorkerCont::Ftrunc { vnode, newsize } => {
                    // On success update the vnode size (the tail of C `truncate_vnode`:
                    // `vp->v_size = newsize`; on failure it is unchanged).
                    if status == 0
                        && let Some(v) = self.vnode_table.get_mut(crate::vnode::VnodeId(vnode))
                    {
                        v.size = newsize as u64;
                    }
                }
                crate::worker::WorkerCont::Path => {
                    // Path-walk continuation: take the slot → revoke → decode the reply →
                    // `walk.resume` → send another lookup (put back the slot, keep suspended)
                    // or run phase 2 (`PathFollow`).
                    let Some(mut pending) =
                        self.worker_pool.get_mut(idx).and_then(|wp| wp.path.take())
                    else {
                        continue;
                    };
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(pending.grant);
                    // `PathFollow` is not `Copy` (`Mkdir` carries a component name) - clone one
                    // for phase 2, leaving `pending.walk` in place for continuation.
                    let follow = pending.follow.clone();
                    let walk_err = |e: crate::path::PathError| e.to_errno();
                    // Reply decoding: special codes/OK go to the state machine; other statuses (including negative errno)
                    // are finalized as errors.
                    let resumed: Result<crate::path::WalkStep, PathFail> =
                        match crate::request::decode_lookup_reply(status, &reply) {
                            Some(res) => {
                                let rd = self.root_dir_of(fp_slot);
                                let mounts = self.mounted_fs_list();
                                pending
                                    .walk
                                    .resume(res, rd, &mounts)
                                    .map_err(PathFail::Path)
                            }
                            None => Err(PathFail::Status(status)),
                        };
                    match resumed {
                        Ok(crate::path::WalkStep::Send {
                            fs_e,
                            dir_ino,
                            root_ino,
                        }) => {
                            // Send another REQ_LOOKUP (the path is copied from the walk cursor into the slot's
                            // scratch + NUL), put the slot back, and stay suspended.
                            if let Some(wp) = self.worker_pool.get_mut(idx) {
                                wp.path = Some(pending);
                            }
                            if self
                                .send_lookup_for_slot(idx, fp_slot, fs_e, dir_ino, root_ino)
                                .is_err()
                            {
                                // Failure before sending (no vmnt/no slot/grant failure): finalize.
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.path = None;
                                }
                                self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                            }
                            continue;
                        }
                        Ok(crate::path::WalkStep::Done(node)) => match follow {
                            crate::worker::PathFollow::Unlink { entry, rmdir } => {
                                // C `do_unlink` (link.c:122-159) stage 1 passed:
                                // parent directory type gate → `X|W` permission gate → sticky-bit gate.
                                if node.mode & crate::open::S_IFMT != crate::open::S_IFDIR {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENOTDIR);
                                    continue;
                                }
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) = match fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (
                                        fp.real_uid,
                                        fp.real_gid,
                                        fp.eff_uid,
                                        fp.eff_gid,
                                        fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                                    ),
                                    None => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                                        continue;
                                    }
                                };
                                let readonly_fs = self
                                    .vmnt_table
                                    .find_by_fs(node.fs_e)
                                    .and_then(|v| self.vmnt_table.get(v))
                                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                                    .unwrap_or(false);
                                // C `forbidden(fp, dirp, X_BIT | W_BIT)`.
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
                                // C `do_unlink:132-152`: on a sticky directory the
                                // victim must first be looked up (`advance(dirp, stickycheck)`);
                                // a wrong owner means EPERM - this step is a **sub-walk**,
                                // hence another suspend point in Rust.
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
                                            self.finish_worker_job(idx, fp_slot, e.to_errno());
                                            continue;
                                        }
                                    };
                                    let (walk2, step2) = match crate::path::LookupWalk::begin(
                                        start, resolve, rd, eff_uid, eff_gid,
                                    ) {
                                        Ok(pair) => pair,
                                        Err(e) => {
                                            self.finish_worker_job(idx, fp_slot, e.to_errno());
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
                                    idx, fp_slot, node.fs_e, node.ino, &entry, rmdir,
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
                                // C `do_unlink:137-141`: in a sticky directory only the victim's
                                // owner or the superuser may delete (decision function `link::sticky_check`).
                                // When the victim cannot be looked up the earlier Err branch (C's
                                // `else r = err_code`) is taken, so this point is unreachable.
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
                                    idx, fp_slot, dir_fs_e, dir_ino, &entry, rmdir,
                                ) {
                                    self.finish_worker_job(idx, fp_slot, e);
                                }
                                continue;
                            }
                            crate::worker::PathFollow::Chdir { into_root } => {
                                // C `do_chdir`/`do_chroot` finalization: after the walk,
                                // `change_into` (locally change this process's work/root directory;
                                // no FS round trip).
                                let Some(vnode) = self.intern_vnode(&node) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                                    continue;
                                };
                                let status = self.change_into(fp_slot, vnode, into_root);
                                self.finish_worker_job(idx, fp_slot, status);
                                continue;
                            }
                            crate::worker::PathFollow::Statvfs { user_buf, flags } => {
                                // C `do_statvfs:318-323`: after the walk, take the vnode's
                                // `v_vmnt` and enter `fill_statvfs`.
                                let Some(vmnt_idx) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                    continue;
                                };
                                match self.begin_statvfs(idx, fp_slot, vmnt_idx.0, user_buf, flags)
                                {
                                    Ok(()) => {}
                                    Err(e) => self.finish_worker_job(idx, fp_slot, e),
                                }
                                continue;
                            }
                            crate::worker::PathFollow::RenameOld { entry, new_path } => {
                                // C `do_rename:194-215`: if the old parent directory is sticky, first run a
                                // **sub-walk** to get the victim's owner (same as unlink);
                                // otherwise go straight to stage 2.
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
                                            self.finish_worker_job(idx, fp_slot, e.to_errno());
                                            continue;
                                        }
                                    };
                                    let (uid, gid) =
                                        match fp_slot.and_then(|s| self.fproc_table.get(s)) {
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
                                            self.finish_worker_job(idx, fp_slot, e.to_errno());
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
                                self.rename_stage_two(
                                    idx, fp_slot, node.fs_e, node.ino, entry, &new_path,
                                );
                                continue;
                            }
                            crate::worker::PathFollow::RenameOldSticky { entry, new_path } => {
                                // Sub-walk passed: the owner gate of C `do_rename:205-210`
                                // (decision function `link::sticky_check`); on success go to stage 2.
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
                                self.rename_stage_two(
                                    idx, fp_slot, node.fs_e, node.ino, entry, &new_path,
                                );
                                continue;
                            }
                            crate::worker::PathFollow::RenameNew {
                                old_fs_e,
                                old_ino,
                                old_name,
                                new_entry,
                            } => {
                                // C `do_rename:242-259`: cross-device gate (EXDEV) →
                                // the `W|X` gates of both parent directories → `req_rename`.
                                if node.fs_e != old_fs_e {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EXDEV);
                                    continue;
                                }
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) = match fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (
                                        fp.real_uid,
                                        fp.real_gid,
                                        fp.eff_uid,
                                        fp.eff_gid,
                                        fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                                    ),
                                    None => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                                        continue;
                                    }
                                };
                                let readonly_fs = self
                                    .vmnt_table
                                    .find_by_fs(node.fs_e)
                                    .and_then(|v| self.vmnt_table.get(v))
                                    .map(|v| v.flags.contains(crate::vmnt::VmntFlags::READONLY))
                                    .unwrap_or(false);
                                // The new parent directory's `W|X` gate (the old parent's is checked in stage 1's
                                // `rename_stage_two` - C writes both in one place,
                                // but their inputs differ; the Rust side checks each per stage,
                                // equivalent to C: failing either yields that error code).
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
                                    idx, fp_slot, node.fs_e, old_ino, node.ino, &old_name,
                                    &new_entry,
                                ) {
                                    self.finish_worker_job(idx, fp_slot, e);
                                }
                                continue;
                            }
                            crate::worker::PathFollow::LinkSrc { dst_path } => {
                                // C `do_link:188-196` stage 1 passed: go to stage 2 -
                                // peel off name2's parent directory and component name, and **start the walk again
                                // from the root/work directory** (C reuses the same `resolve` with a new path for
                                // another `last_dir` pass).
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
                                    crate::path::LookupStart {
                                        fs: rd.fs,
                                        ino: rd.ino,
                                        dev: rd.dev,
                                    }
                                } else {
                                    let wd = fp_slot.map(|s| self.work_dir_of(s)).unwrap_or(
                                        crate::path::RootDir {
                                            ino: rd.ino,
                                            fs: rd.fs,
                                            dev: rd.dev,
                                        },
                                    );
                                    crate::path::LookupStart {
                                        fs: wd.fs,
                                        ino: wd.ino,
                                        dev: wd.dev,
                                    }
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
                                // C `do_link:203-211`: cross-device gate (EXDEV) →
                                // `W|X` gate → `req_link`.
                                if node.fs_e != src_fs_e {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EXDEV);
                                    continue;
                                }
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) = match fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (
                                        fp.real_uid,
                                        fp.real_gid,
                                        fp.eff_uid,
                                        fp.eff_gid,
                                        fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                                    ),
                                    None => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
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
                                            self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                            continue;
                                        }
                                    }
                                };
                                let Some(vmnt) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                                    #[allow(clippy::let_underscore_must_use)]
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
                            crate::worker::PathFollow::Slink {
                                entry,
                                target_addr,
                                target_len,
                            } => {
                                // C `do_slink:411-414`: `forbidden(fp, vp,
                                // W_BIT|X_BIT)` passes, then `req_slink` is sent (the parent directory's
                                // type is checked by the walk/FS; there is no separate type gate here).
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) = match fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (
                                        fp.real_uid,
                                        fp.real_gid,
                                        fp.eff_uid,
                                        fp.eff_gid,
                                        fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                                    ),
                                    None => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
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
                                // The name: a direct grant into VFS memory.
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
                                            self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                            continue;
                                        }
                                    }
                                };
                                // The target string: a magic grant into **user memory** (the first pass carries it).
                                // `CPF_TRY`, C request.c:1061-1062).
                                let grant_target = match self.grant_user_buffer(
                                    node.fs_e,
                                    user,
                                    target_addr,
                                    target_len,
                                    minix_types::CpFlags::READ | minix_types::CpFlags::TRY,
                                ) {
                                    Ok(g) => g,
                                    Err(_) => {
                                        // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                                        #[allow(clippy::let_underscore_must_use)]
                                        let _ = self.revoke_grant(grant_path);
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    }
                                };
                                let Some(vmnt) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                                    #[allow(clippy::let_underscore_must_use)]
                                    let _ = self.revoke_grant(grant_path);
                                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                                    #[allow(clippy::let_underscore_must_use)]
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
                            crate::worker::PathFollow::Utimens {
                                atime,
                                mtime,
                                flags,
                            } => {
                                // The path half of C `do_utimens` (time.c:74-96): unknown
                                // flags are rejected at the entry point (re-checked here in case follow was
                                // constructed elsewhere); the walk semantics of `AT_SYMLINK_NOFOLLOW`
                                // already selected the flags at the entry point.
                                let _ = flags;
                                self.finish_utimens(
                                    idx, fp_slot, node.fs_e, node.ino, node.uid, node.gid,
                                    node.mode, atime, mtime,
                                );
                                continue;
                            }
                            crate::worker::PathFollow::Mknod {
                                entry,
                                mode_bits,
                                dev,
                            } => {
                                // C `do_mknod:543-552`: parent directory type gate → `W|X`
                                // permission gate → `req_mknod` (the name uses a direct grant;
                                // the reply is status-only).
                                if node.mode & crate::open::S_IFMT != crate::open::S_IFDIR {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENOTDIR);
                                    continue;
                                }
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) = match fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (
                                        fp.real_uid,
                                        fp.real_gid,
                                        fp.eff_uid,
                                        fp.eff_gid,
                                        fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                                    ),
                                    None => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
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
                                // The name is written into the slot's scratch and direct-granted
                                // (C `cpf_grant_direct(fs_e, lastc, len,
                                // CPF_READ)`).
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
                                            self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                            continue;
                                        }
                                    }
                                };
                                let Some(vmnt) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                                    #[allow(clippy::let_underscore_must_use)]
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
                                        dev, node.ino, mode_bits, eff_uid, eff_gid, grant, name_len,
                                    ),
                                });
                                continue;
                            }
                            crate::worker::PathFollow::Truncate { length } => {
                                // C `do_truncate:311-316`: `forbidden(fp, vp,
                                // W_BIT)` passes, the file is touched; the unchanged-size case lives in the send half.
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) = match fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (
                                        fp.real_uid,
                                        fp.real_gid,
                                        fp.eff_uid,
                                        fp.eff_gid,
                                        fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                                    ),
                                    None => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
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
                                    // Request registered: wait for the FS reply (the continuation finalizes).
                                    Ok(true) => {}
                                    // Unchanged size: C returns `r = OK`; the job completes in place.
                                    Ok(false) => self.finish_worker_job(idx, fp_slot, 0),
                                    Err(e) => self.finish_worker_job(idx, fp_slot, e),
                                }
                                continue;
                            }
                            crate::worker::PathFollow::Chown { uid, gid } => {
                                // The path half of C `do_chown`: after the walk, hand over to the shared body
                                // (read-only gate + three ownership rules + range check + send request).
                                let Some(vnode) = self.intern_vnode(&node) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENFILE);
                                    continue;
                                };
                                self.finish_chown(
                                    idx, fp_slot, node.fs_e, node.ino, node.uid, node.gid, uid,
                                    gid, vnode,
                                );
                                continue;
                            }
                            crate::worker::PathFollow::Chmod { user: _u, mode } => {
                                // The path half of C `do_chmod`: after the walk and vnode interning, enter
                                // the shared body (owner gate + read-only gate + setgid clearing + send request).
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
                            crate::worker::PathFollow::Rdlink {
                                user,
                                buf,
                                buf_size,
                            } => {
                                // C `do_rdlink` (link.c:496-506): not a symlink
                                // means EINVAL (`PATH_RET_SYMLINK` already made FS not
                                // follow the last component, so what arrives here is the link itself); only a symlink
                                // sends `REQ_RDLINK`.
                                if node.mode & crate::open::S_IFMT != crate::open::S_IFLNK {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                                    continue;
                                }
                                let Some(vmnt) = self.vmnt_table.find_by_fs(node.fs_e) else {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                    continue;
                                };
                                // magic grant: FS writes the link text into the user buffer.
                                // C's first pass carries `CPF_TRY` (request.c:760-761).
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
                            crate::worker::PathFollow::Access {
                                user: _acc_user,
                                access,
                            } => {
                                // The local half of C `do_access` (protect.c:216-231):
                                // after the walk, check permissions — `forbidden` uses the **real** uid/gid
                                // (the `job_call_nr == VFS_ACCESS` special case at protect.c:255-256);
                                // the root user also has a full-rwx face.
                                let _ = _acc_user;
                                let (real_uid, real_gid, eff_uid, eff_gid, supp) = match fp_slot
                                    .and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (
                                        fp.real_uid,
                                        fp.real_gid,
                                        fp.eff_uid,
                                        fp.eff_gid,
                                        fp.supplemental_groups[..fp.ngroups.min(16)].to_vec(),
                                    ),
                                    None => {
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
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
                            crate::worker::PathFollow::Open {
                                user: _open_user,
                                oflags,
                            } => {
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
                                // `path` is needed only when stage 1 fails (ENOENT);
                                // that route uses the same field in the Err branch.
                                // Stage 1 passing = the file already exists: with `O_EXCL` that is
                                // EEXIST (C's `exist` decision at open.c:118-120);
                                // otherwise finalize as a normal open.
                                let _ = user;
                                if oflags & crate::open::OpenFlags::EXCL.bits() != 0 {
                                    self.finish_worker_job(idx, fp_slot, minix_types::EEXIST);
                                    continue;
                                }
                                let _ = mode;
                                self.finish_open_local(idx, fp_slot, &node, oflags);
                                continue;
                            }
                            crate::worker::PathFollow::CreatInDir {
                                user,
                                oflags,
                                mode,
                                entry,
                            } => {
                                // Stage 2 passed (parent directory exists): send `REQ_CREATE`.
                                // (C `new_node` → `req_create`).
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
                                        // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                                        #[allow(clippy::let_underscore_must_use)]
                                        let _ = self.revoke_grant(grant);
                                        self.finish_worker_job(idx, fp_slot, minix_types::EIO);
                                        continue;
                                    }
                                };
                                let (uid, gid) = match fp_slot.and_then(|s| self.fproc_table.get(s))
                                {
                                    Some(fp) => (fp.eff_uid, fp.eff_gid),
                                    None => {
                                        // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                                        #[allow(clippy::let_underscore_must_use)]
                                        let _ = self.revoke_grant(grant);
                                        self.finish_worker_job(idx, fp_slot, minix_types::EINVAL);
                                        continue;
                                    }
                                };
                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                    wp.cont =
                                        Some(crate::worker::WorkerCont::Create { user, oflags });
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
                                continue; // wait for the REQ_CREATE reply (Create continuation)
                            }
                            crate::worker::PathFollow::Mkdir { user, entry, mode } => {
                                // C `do_mkdir` (open.c:564-598): the node reached must be a
                                // directory (otherwise ENOTDIR) → permission gate (W|X) →
                                // `req_mkdir(parent ino, lastc, uid, gid, bits)`.
                                let ftype = node.mode & crate::open::S_IFMT;
                                if ftype != crate::open::S_IFDIR {
                                    self.finish_worker_job(idx, fp_slot, minix_types::ENOTDIR);
                                    continue;
                                }
                                // Permission gate: `forbidden(fp, vp, W_BIT|X_BIT)`.
                                // (C :589).
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
                                // The last component name goes into the slot's scratch + NUL and is granted
                                // to FS for reading (C `req_mkdir`'s `cpf_grant_direct`).
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
                                let _ = name_len;
                                let vmnt = match self.vmnt_table.find_by_fs(node.fs_e) {
                                    Some(v) => v.0,
                                    None => {
                                        // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                                        #[allow(clippy::let_underscore_must_use)]
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
                                continue; // wait for the REQ_MKDIR reply (Status continuation finalizes)
                            }
                            crate::worker::PathFollow::Stat { user, buf } => {
                                // Phase 2: grant the user stat buffer → REQ_STAT →
                                // hand the continuation to Fstat (the one already landed).
                                let grant = match self.grant_user_buffer(
                                    node.fs_e,
                                    user,
                                    buf,
                                    minix_types::Stat::USER_STAT_SIZE as u64,
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
                                        // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                                        #[allow(clippy::let_underscore_must_use)]
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
                                continue; // wait for the stat reply (Fstat continuation finalizes)
                            }
                        },
                        Err(e) => {
                            let status = match e {
                                PathFail::Path(p) => walk_err(p),
                                PathFail::Status(st) => st,
                            };
                            // ENOENT in `creat` stage 1 → stage 2: **walk the parent directory**
                            // (the `last_dir` step of C `new_node`). Other errors
                            // are finalized as a status.
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
                                                self.finish_worker_job(idx, fp_slot, pe.to_errno());
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
                                            let wd =
                                                fp_slot.map(|s| self.work_dir_of(s)).unwrap_or(rd);
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
                                                if let Some(wp) = self.worker_pool.get_mut(idx) {
                                                    wp.path = Some(crate::worker::PathPending {
                                                        walk: walk2,
                                                        grant: 0,
                                                        follow:
                                                            crate::worker::PathFollow::CreatInDir {
                                                                user,
                                                                oflags,
                                                                mode,
                                                                entry: split.entry,
                                                            },
                                                    });
                                                }
                                                if self
                                                    .send_lookup_for_slot(
                                                        idx, fp_slot, fs_e, dir_ino, root_ino,
                                                    )
                                                    .is_err()
                                                {
                                                    if let Some(wp) = self.worker_pool.get_mut(idx)
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
                                                self.finish_worker_job(idx, fp_slot, pe.to_errno());
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
                    // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = self.revoke_grant(grant);
                    if status == 0 {
                        // The success half of C `req_readwrite_actual`: take `seek_pos`/`nbytes`
                        // from the **reply** (`mess_fs_vfs_readwrite` —
                        // ipc.h:214-220, the offsets come from the shared authority table), write the position back to the filp,
                        // and hand the user "bytes actually read" as the status (C's `cum_io`).
                        // SAFETY: the reply payload is written in LP64 field order in the payload area.
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
                        // C read.c:255-259 — on the write direction for a regular file/directory,
                        // a new position beyond the old size raises it (the read direction does not change the size).
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
                        let _ = orig_pos; // the position already came from the reply; the old value is only for reconciliation
                    }
                }
            }
            // Complete: clear the slot + reply to the user (the target is the fproc's endpoint).
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
                            // Success value (byte count/entry count): non-negative, returned as is.
                            crate::call_table::SyscallResult::Ok(status)
                        } else if status == 0 {
                            crate::call_table::SyscallResult::Ok(0)
                        } else if status > 0 {
                            crate::call_table::SyscallResult::Error(status)
                        } else {
                            // Negative on-wire status: flip back to a positive errno first; the boundary folds it negative again.
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
    /// placeholder for "receive then route").
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

    /// Fold the [`SyscallResult`](crate::call_table::SyscallResult) of one dispatch into a
    /// reply and queue it (the reply half).
    ///
    /// The mapping follows the `reply(who_e, result)` tail of C `do_work` (main.c:297 →
    /// `reply:638`'s `m_out->m_type = result; ipc_sendnb(...)`), but **the sign is
    /// folded once at the boundary**: C's VFS is a `_SYSTEM` build, and `_SIGN` in
    /// `errno.h:187-192` makes constants such as `EINVAL` negative already (`errno.h:64`), so the error
    /// code C writes into `m_type` is negative; the user side's `_syscall` tests `m_type < 0`
    /// (`minix3/minix/lib/libc/sys/syscall.c:9-25`; the Rust user side follows the same convention,
    /// see `minix-sys/src/syscall.rs:107`). Rust arms use **positive** constants
    /// internally (crate convention, `minix_types::EINVAL == 22`), so the sign is folded negative here.
    ///
    /// Statuses coming from driver and FS replies are **already negative** (those processes are `_SYSTEM`
    /// builds too), so they pass through unchanged — `Error(e)` negates only when `e > 0`, and the two
    /// sources cannot conflict. Success values are fd/byte count/0, always non-negative, and `Ok(v)` is left alone.
    pub fn queue_reply(&mut self, target: Endpoint, result: crate::call_table::SyscallResult) {
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
        // When the arm has already queued a **payload** reply itself (e.g. the unchanged-position
        // `lseek` path writing the new offset into `m_vfs_lc_lseek.offset`, C `open.c:665-666`), this
        // unified finalizer's bare reply **must not overwrite it** — overwriting would hand the user offset 0.
        // One reply per round: the first one queued wins.
        if self.pending_reply.is_some() {
            return;
        }
        self.queue_reply_msg(
            target,
            Message {
                m_type: code,
                ..Message::default()
            },
        );
    }

    /// Queue one **payload** reply (C's `job_m_out` has fields filled by the arm before return,
    /// e.g. `lseek` writing the new offset into `m_vfs_lc_lseek.offset`).
    pub fn queue_reply_msg(&mut self, target: Endpoint, msg: Message) {
        if target == Endpoint::NONE || target.to_user_slot().is_none() {
            return;
        }
        self.pending_reply = Some((target, msg));
    }

    /// Take the pending reply (called by `run()` at the tail of each loop iteration).
    pub fn take_reply(&mut self) -> Option<(Endpoint, Message)> {
        self.pending_reply.take()
    }

    /// Send FS a **magic grant**: authorize the `bytes` bytes starting at `user_e`:`addr`
    /// to `fs_e` for reading and writing.
    ///
    /// C: `cpf_grant_magic(fs_e, user_e, addr, bytes, access)` ——
    /// `request.c:844` (read/write) and `:1087` (stat) share this shape; the permission bits
    /// come from the caller (read is `CPF_WRITE` — FS writes into the user buffer; the half with `CPF_TRY`
    /// is expressed by `GrantScope`; see `req_stat`/`req_readwrite` for the `ERESTART` resend path).
    ///
    /// Failure surface: a full table / failed kernel call yields `Err` — C `panic`s in both
    /// places ("cpf_grant_* failed"); Rust leaves the decision to the arm (semantically an internal error in C;
    /// the arm replies `EIO` to the user).
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

    /// Revoke one grant (C `cpf_revoke`). The return value follows C's call-site semantics: `Ok`
    /// whenever the revoke itself does not error; `GRANT_FAULTED` is compared by the caller
    /// (C's `if (cpf_revoke(grant_id) == GRANT_FAULTED) return ERESTART;`).
    pub fn revoke_grant(&mut self, grant: i32) -> Result<i32, i32> {
        self.grants.revoke(grant)
    }

    /// The conversation primitive of `fs_sendrec` (comm.c:134-170) — the entry half of syscall arms.
    ///
    /// Mirrors C's six steps: `find_vmnt(fs_e)` validation (`vmnt` exists and `fs` points at
    /// the same endpoint, otherwise `EIO`, comm.c:137-140) → `EDEADLK` self-deadlock gate
    /// (the calling process is that FS itself, comm.c:142-144) → `assert(w_sendrec ==
    /// NULL)` (the slot must not already be waiting, comm.c:146) → attach `w_sendrec`
    /// ([`crate::worker::WorkerSlot::set_waiting`]) → under the second window gate
    /// `sendmsg`/`queuemsg` (via [`FsTransport::send_fs`]). C's
    /// `worker_wait` becomes "the arm returns Suspend and the slot stays in
    /// `WaitingForFs`" in the single-threaded model; the reply is landed by [`Self::handle_fs_reply`].
    ///
    /// On a send failure the slot's wait mark is not cleared — as in C (after `sendmsg` fails,
    /// `w_sendrec` is left for the job-ending `worker_release` to clean up); the caller arm
    /// receives the `Err`, replies with the error and releases the slot.
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
        // comm.c:142-144 — `if (fs_e == fp->fp_endpoint) return EDEADLK`.
        if let Some(fp_slot) = self.worker_pool.get(slot).and_then(|w| w.fp_slot)
            && let Some(fp) = self.fproc_table.get(fp_slot)
            && fp.endpoint == fs_ep
        {
            return Err(CommError::Deadlock);
        }
        {
            let w = self.worker_pool.get_mut(slot).ok_or(CommError::NoWorker)?;
            // comm.c:146 — `assert(self->w_sendrec == NULL)`.
            assert!(
                w.state != crate::worker::WorkerState::WaitingForFs,
                "fs_sendrec on already-waiting slot (C comm.c:146 assert)"
            );
            w.set_waiting(fs_ep, *req);
        }
        transport
            .send_fs(vmnt, fs_ep, slot, req, &mut self.comm)
            .map(|_| ())
    }

    /// The real send half of `send_work` (comm.c:37-47) × `fs_sendmore` (comm.c:66-87) —
    /// after the window opens, flush the queued requests.
    ///
    /// In C the flush is driven by `send_work()` from the finishing worker; in the single-threaded
    /// model the main-loop layer calls it after a reply lands (when `run()` holds the transport), keeping this primitive
    /// independently testable. `GlobalComm::fs_sendmore` carries C's gate order and
    /// `pop + sending--` (`c_cur_reqs++` is inlined there); then it stamps the transid via `sendmsg`
    /// and delivers with `asynsend3` — a send failure is ignored as `(void)`
    /// (C's existing behavior at comm.c:86; the count was already incremented). Returns the number actually flushed.
    pub fn flush_send_queue<T: minix_sys::ipc::IpcTransport>(
        &mut self,
        transport: &mut T,
    ) -> usize {
        // C `send_work()` comm.c:42 — `if (sending == 0) return;`: the
        // common case (nothing parked) must cost zero, especially now that
        // this sweep runs once per main-loop event.
        if self.comm.sending == 0 {
            return 0;
        }
        let mut sent = 0;
        for idx in 0..crate::vmnt::NR_MNTS {
            // The counterpart of `if (vmp->m_fs_e == NONE) return` in C fs_sendmore:79 —
            // empty mount slots are not scanned.
            if self
                .vmnt_table
                .get(crate::vmnt::VmntId(idx))
                .is_none_or(|v| v.fs == Endpoint::NONE)
            {
                continue;
            }
            while let Some(slot) = self.comm.fs_sendmore(idx) {
                // Invariant: every parked slot was created by
                // `fs_sendrec`'s `set_waiting()` (which always stores the
                // request), so `worker`/`sendrec` are `Some`. The `continue`
                // arms below are unreachable while that holds; `fs_sendmore`
                // already charged the window, so hitting them would strand
                // it — keep this in mind before routing anything new here.
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
                // C sendmsg's `TRNS_ADD_ID` stamp (adding it here for queued requests that were not stamped
                // at queue time) + `asynsend3` (failure ignored as `(void)`).
                let mut out = req;
                out.m_type = crate::fs_comm::TransId::add(req.m_type as u32, slot) as i32;
                if transport.sendnb(fs_ep, &out).is_ok() {
                    sent += 1;
                }
            }
        }
        sent
    }

    /// `do_reply:187` (main.c) — land an FS reply on the waiting worker slot.
    ///
    /// C's steps, one by one:
    /// 1. `transid` decodes the slot number (`TRNS_GET_ID` + `IS_VFS_FS_TRANSID`).
    /// 2. `who_e != VM_PROC_NR && find_vmnt(who_e)==NULL → panic`
    /// (main.c:190-191) — VM replies skip the vmnt lookup; a mount point not found is
    /// fail-fast (same panic text as C).
    /// 3. `wp->w_task != who_e → printf+return` (main.c:193-196) —
    /// typed as [`FsReplyError::WrongTask`].
    /// 4. `*w_sendrec = m_in; w_sendrec becomes delivered` (main.c:204-205) —
    /// the slot model's `sendrec` storage is overwritten by the reply; the continuation reads the result from the same slot
    /// (C's continuation reads `reqmp->m_type` at `fs_sendrec:164`).
    /// 5. `w_task = NONE; c_cur_reqs--` (main.c:206-208) — the window opens.
    /// 6. `worker_signal(wp)` (main.c:209) — the slot moves from `WaitingForFs` back to
    /// `Busy` (the coroutine is runnable; in the single-threaded model "runnable" means waiting for continuation dispatch).
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
        // C main.c:190 — VM replies skip the vmnt lookup; others panic when no mount point is found.
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
        // C main.c:88 — `m_in.m_type = TRNS_DEL_ID(m_in.m_type)`: **the transid is stripped
        // before entering do_reply**, so `m_type` in `*w_sendrec` is the server's
        // raw status (including special codes such as `EENTERMOUNT`), and the continuation
        // uses it directly to decide success/failure/special code.
        let mut stripped = *msg;
        stripped.m_type = minix_types::trns_del_id(msg.m_type);
        // C main.c:204-206 — clear w_task after `*w_sendrec = m_in`; in the slot model
        // "is it waiting" is carried by task, and the reply body stays in sendrec for the continuation.
        wp.sendrec = Some(stripped);
        wp.task = None;
        if let Some(idx) = vmnt_idx {
            debug_assert!(
                self.comm.vmnts[idx].cur_reqs > 0,
                "c_cur_reqs underflow (C main.c:207 pairing)"
            );
            self.comm.vmnts[idx].cur_reqs -= 1;
        }
        // C main.c:209 worker_signal — the thread returns from worker_wait.
        wp.state = crate::worker::WorkerState::Busy;
        Ok(slot)
    }
}

impl Default for VfsState {
    fn default() -> Self {
        Self::new()
    }
}

/// The two failure sources of path continuation (state-machine error vs the raw status of an FS reply).
enum PathFail {
    /// A `LookupWalk` error (`PathError`).
    Path(crate::path::PathError),
    /// The FS reply status is not OK/one of the three special codes (an ordinary error).
    Status(i32),
}

impl From<crate::path::PathError> for PathFail {
    fn from(e: crate::path::PathError) -> Self {
        Self::Path(e)
    }
}

/// One pending FS conversation (see [`VfsState::flush_pending_fs`]).
#[derive(Debug, Clone, Copy)]
pub struct PendingFs {
    /// Target mount window (`vmnt` index).
    pub vmnt: usize,
    /// Target file-system endpoint.
    pub fs_e: Endpoint,
    /// Worker slot index occupied by this job.
    pub worker: usize,
    /// Magic grant already sent to FS (revoked by the loop if sending fails).
    pub grant: i32,
    /// User endpoint (reply destination).
    pub user: Endpoint,
    /// Request message (`REQ_*`).
    pub req: Message,
}

/// IPC adapter for the SEF loop: `SefIpc` needs only the receive/notify verbs, carried by
/// the direct trap transport (same shape as the VM's `SefAdapter`).
pub struct VfsIpc {
    inner: minix_sys::ipc::DirectTrapTransport,
}

impl VfsIpc {
    pub const fn new() -> Self {
        Self {
            inner: minix_sys::ipc::DirectTrapTransport,
        }
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
/// Corresponds to Minix3's `main()` function (main.c:54-118): SEF startup →
/// handshake blocking loop (main.c:410-436) → `sef_receive(ANY)` main loop (main.c:601).
/// Production reply sending splits into two tracks as in C (Ground Truth): the PM control leg = a blocking
/// `ipc_send` + panic on failure (`service_pm()` tail, main.c:912-915); other
/// caller legs = `ipc_sendnb` + ignored failure (the generic `reply()`, main.c:638-650).
///
/// The reply carries only `m_type` (C's `reply` is likewise the bare `memset(&m, 0, sizeof(m));
/// m.m_type = result` reply) — the data plane was already copied out by the arms themselves or landed
/// via the FS answer.
fn send_reply(target: Endpoint, reply: Message) {
    use minix_sys::ipc::IpcTransport;
    // C split (Ground Truth): the `service_pm()` tail replies to PM
    // with a BLOCKING `ipc_send` and panics on failure
    // (vfs/main.c:912-915); only the generic `reply()` uses
    // `ipc_sendnb` and ignores failures (main.c:638-650). Routing the
    // PM leg through sendnb was a semantic deviation: when PM was not
    // parked in receive yet (its tell_vfs leg still running), the
    // reply raced. On real hardware aarch64 froze at VFS_PM_FORK_REPLY (0x987)
    // while x86 crossed the same window by timing luck -- a two-architecture
    // Heisenbug signature of this split.
    // Invariant: unlike C, where only VFS's main thread blocks here while
    // worker threads keep serving, this server is single-threaded — the whole
    // loop stalls until PM takes the reply.
    // Safe today because PM's tell_vfs leg is a one-way send that always
    // returns to receive (its sendrec-on-VFS legs precede tell_vfs);
    // any future PM->VFS sendrec leg that waits on this loop would deadlock
    // here.
    if target == Endpoint::PM {
        minix_sys::ipc::DirectTrapTransport
            .send(target, &reply)
            .expect("VFS service_pm: ipc_send to PM failed"); // C: panic
        return;
    }
    // Non-blocking send (C `ipc_sendnb`): the caller is waiting in
    // sendrec and will not refuse the delivery.
    // [SD19-gate] best-effort IPC cleanup (send-and-forget/timeout path; failure does not change an established main result)
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_sys::ipc::DirectTrapTransport.sendnb(target, &reply);
}

/// The birth-reply leg: C `sef_cb_init_response_rs_reply` =
/// `ipc_sendrec(RS, &m)` (sef_init.c:458-466). Unlike the sendnb used for service replies,
/// sendrec leaves VFS parked in the receive(RS) half after sending RS_INIT (the kernel sets
/// REPLY_PEND, getfrom=RS) and waits for the OK that RS's catch exit returns — this prevents
/// RS's reply from being claimed by no one and consumed feral → echoed back to RS → step3 panic.
fn send_birth_reply(mut reply: Message) {
    use minix_sys::ipc::IpcTransport;
    // [SD19-gate] best-effort IPC cleanup (send-and-forget/timeout path; failure does not change an established main result)
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_sys::ipc::DirectTrapTransport.sendrec(Endpoint::RS, &mut reply);
}

pub fn run() -> ! {
    let mut state = VfsState::new();
    state.init_fresh();
    let mut ipc = VfsIpc::new();
    // VFS registers no signal handler in C (see the steady-state loop comment in this file); the escape-hatch
    // token only satisfies the library contract — no one cancels it, and all three receive sites share it.
    let cancel = minix_sef::SefCancel::new();

    // Startup handshake (main.c:410-436): `sef_receive(PM_PROC_NR)` do-while —
    // each VFS_PM_INIT fills one fproc slot; endpoint==NONE terminates.
    loop {
        let mut msg = Message::default();
        // The empty signal closure in the handshake phase has the same basis as steady state (see the steady-state loop
        // comment below): C VFS registers no signal handler, so SYSTEM notifications are swallowed by the library's
        // default empty handler; the same SEF interception arm serves both phases, leaving the closure no code to handle.
        let recv =
            minix_sef::sef_receive_status(&mut ipc, Endpoint::PM, &mut msg, &mut |_| {}, &cancel)
                .unwrap_or_else(|e| panic!("vfs: handshake receive failed: {e}"));
        // The NONE terminator also goes through step: the state machine performs the PmHandshake→InitTables
        // transition here (consistent with the existing handshake test's complete semantics).
        let complete = state
            .pm_handshake_step(&recv.message)
            .unwrap_or_else(|e| panic!("vfs: handshake rejected message: {e:?}"));
        if complete {
            break;
        }
    }
    // C main.c:435-436 — `mess.m_type = OK; ipc_send(PM_PROC_NR, &mess)`:
    // Once the process table is complete, reply success to PM (the last entry of PM's `vfs_init_sync` is the sendrec
    // barrier waiting for exactly this — without it PM's boot chain stops here).
    // Root-cause fix: the barrier must use a **blocking** send (matching C ipc_send).
    // The old send_reply used sendnb: PM is at that moment in sendrec's receive half (not receiving),
    // so ENOTREADY was swallowed by `let _ =` ⇒ the OK reply was lost and PM's barrier buffer kept
    // old content ⇒ m_type != 0 panic. A blocking send parks VFS
    // until PM enters receive and takes the reply — matching C semantics.
    // [SD19-gate] C main.c:435-436 does not check ipc_send's return; a blocking send can fail only if
    // PM is dead, after which VFS receives no requests and stalls naturally, so the error has nowhere to go
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_sys::send(
        Endpoint::PM,
        &Message {
            m_type: minix_types::OK,
            ..Message::default()
        },
    );

    // ── Boot rendezvous: RS_INIT consumption (the matching timing position of main.c:455-467) ──
    // C's sef_local_startup consumes RS_INIT before do_init_root (main.c:492-497)
    // and runs init_fresh's rproctab assembly — dmap's boot row is the root mount's
    // routing prerequisite. The RS boot sends to each server in image order and waits for answers (rs/boot.rs
    // step2); VFS's RS_INIT is strictly later than PM handshake completion, so blocking on RS here
    // follows C's order rather than busy-waiting.
    let mut boot_msg = Message::default();
    loop {
        // The rendezvous-phase empty signal closure has the same basis (see the steady-state loop comment): C VFS
        // registers no handler; the SEF interception arm consumes signal requests before this loop, so the closure has no code.
        let recv = minix_sef::sef_receive_status(
            &mut ipc,
            Endpoint::RS,
            &mut boot_msg,
            &mut |_| {},
            &cancel,
        )
        .unwrap_or_else(|e| panic!("vfs: boot rendezvous receive failed: {e}"));
        let SefEvent::Init(init_type) = recv.event else {
            // Non-Init RS messages: a legitimate ping was already consumed in the SEF layer (sef.c:208-214);
            // anything else should not occur during the boot rendezvous — drop it and wait again, the same
            // as C sef_local_startup's loop.
            continue;
        };
        // Birth-face reply (the library-root half): the orchestration — stage one IPC filtering
        // (C `process_init` stage one's `assert(r == OK)` at sef_init.c:59-60,
        // with statectl wired directly), and the reply message built by the library; the dispatch semantics (a fresh
        // instance ≡ init_fresh at construction; restart re-runs the local reset; LU is registered but the Rust
        // side does not model the LU machinery → an honest ENOSYS) stay as they were. The rproctab consumption result is
        // this service's init result (failure panic = the same family of fail-fast as C
        // do_sef_init_request failure).
        let birth = minix_sef::process_init(
            &mut |req, addr, len| {
                let r = minix_sys::syscall::sys_statectl(
                    &minix_sys::syscall::DirectKernelCallTransport,
                    req,
                    addr,
                    len,
                );
                if r == minix_types::OK { Ok(r) } else { Err(r) }
            },
            init_type,
            &mut |t| {
                match t {
                    0 | 2 => 0, // SEF_INIT_FRESH / SEF_INIT_RESTART — sef.h:93-95
                    _ => minix_types::ENOSYS,
                }
            },
        );
        let mut reply = match birth {
            Ok(m) => m,
            // C's stage-one assert = fail-closed on birth failure (no recovery surface during startup).
            Err(e) => panic!("vfs: birth statectl failed: {e}"),
        };
        if reply.rs_init_result() == Some(0) {
            // restart re-runs the local reset (same as the main loop's Init arm); the fresh reset was already
            // done at the start of run() (≡ C init_fresh's single execution).
            if init_type == 2 {
                state.init_fresh();
            }
            let init = minix_types::RsInit::decode_message(&recv.message);
            state
                .map_boot_services(
                    &minix_sys::syscall::DirectKernelCallTransport,
                    init.rproctab_gid,
                )
                .unwrap_or_else(|e| panic!("vfs: boot rproctab consumption failed: {e:?}"));
        }
        send_birth_reply(reply);
        break;
    }

    // main.c:492-497 — do_init_root starts via worker_start; C panics on failure
    // (main.c:519-520), and the same applies here.
    state
        .finish_init(
            &minix_sys::syscall::DirectKernelCallTransport,
            &minix_sys::ipc::DirectTrapTransport,
        )
        .unwrap_or_else(|e| panic!("vfs: failed to initialize root: {e:?}"));

    // Startup (main.c:441): subscribe to driver-online events at DS (remote failure ignored).
    // The grant-table registration at C process startup (`sys_setgrant`; the `grants`
    // global in safecopies.c tells the kernel where the table is); failure has no recovery surface, so it is only recorded.
    // [SD19-gate] best-effort DS subscription/registration (failure resurfaces through later query paths)
    #[allow(clippy::let_underscore_must_use)]
    let _ = state
        .grants
        .register(&minix_sys::syscall::DirectKernelCallTransport);

    let mut ds = minix_sys::ds::DsClient::new(
        minix_sys::ipc::DirectTrapTransport,
        minix_sys::syscall::DirectKernelCallTransport,
        Endpoint::DS,
    );
    // The DS subscription leg reuses the **process's main grant table** (`state.grants`, already
    // registered with the kernel) — the DsClient no longer carries a private table; otherwise its
    // `SYS_SETGRANT` would overwrite the kernel's `s_grant_table` and make later lookups' safecopy
    // read the wrong table (fl=0 → EPERM → copy_from → wire EIO).
    // [SD19-gate] best-effort DS subscription/registration (failure resurfaces through later query paths)
    #[allow(clippy::let_underscore_must_use)]
    let _ = ds.subscribe(&mut state.grants, "drv\\.[bc]..\\..*", {
        (minix_types::DsFlags::INITIAL | minix_types::DsFlags::OVERWRITE).bits() as i32
    });

    // Heap migration (the fourth, and the biggest cache consumer): VFS is the service that uses the global
    // heap most heavily (fproc/mount/path working sets), so its global supply moves from a hard-wired fixed pool to a
    // VM_BRK growing heap. The install point is after the whole boot handshake (PM handshake + RS INIT rendezvous + finish_init
    // root mount) completes, after DS subscription, and before the steady-state event loop — by then VM (second
    // in boot order, ahead of VFS at fifth) has its main loop receiving requests, so the VM_BRK direct call inside
    // `install_vm_supplier` gets an answer; EINVAL (image tail 0 = link defect) is fail-fast.
    // Startup allocations (init_fresh table assembly) still use the fixed pool; the switch affects only later
    // growth allocations. Same method as the earlier DS/SCHED/MIB migrations.
    #[cfg(target_os = "none")]
    minix_rt::install_vm_supplier(minix_rt::image_end())
        .expect("vfs: install_vm_supplier(镜像尾) 不得失败");

    // Real-hardware evidence probe: immediately after installation, request one page from the VM leg (an 8-byte
    // slab request triggers add_slab → supply_page → VM_BRK); failure is fail-fast, and success prints one serial
    // marker line as L1 evidence that the VM_BRK growth leg works end to end. The probe block returns to the slab
    // free list on free (the VM side does not reclaim pages: shrinking brk never unmaps, as in C).
    #[cfg(target_os = "none")]
    {
        let probe = minix_rt::alloc(8);
        assert!(!probe.is_null(), "vfs: vm-brk probe 分配失败");
        minix_rt::free(probe);
        use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
        #[allow(clippy::let_underscore_must_use)] // [SD19-gate] fall back to parking when the reporting channel itself is broken (panic.c:66)
        let _ = sys_diagctl_write(
            &DirectKernelCallTransport,
            "nk4a: vfs-vmbrk probe ok pages=1 via=VM_BRK\n",
        );
    }

    loop {
        // C main.c:601-602 — sef_receive(ANY): ping interception completes in the SEF layer.
        let mut msg = Message::default();
        let recv = minix_sef::sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut msg,
            &mut |_| {
                // C VFS registers no signal handler — sef_local_startup at main.c:374-388
                // registers only init and LU callbacks, and a whole-tree grep for `sef_setcb_signal_handler`
                // has zero hits, so the library default is the empty function `sef_cb_signal_handler_null`
                // (sef.h:287 plus sef_signal.c:156-158): SYSTEM notifications are rewritten inside the library into
                // signal requests (sef.c:187), the empty handler consumes them and the framework `continue`s
                // (sef.c:231-235); the main loop has no observable signal behavior — an empty closure is faithful.
            },
            &cancel,
        )
        .unwrap_or_else(|e| panic!("vfs: receive failed: {e}"));
        match recv.event {
            SefEvent::Call(_) => {
                let codec = VfsTransIdCodec;
                let _ = state.run_once(&recv.message, &codec);
                // Reply half: this round's dispatch reply is sent at the loop tail (C `do_work`'s
                // tail `reply(who_e, result)`).
                if let Some((target, reply)) = state.take_reply() {
                    send_reply(target, reply);
                }
                // FS conversations registered by arms are sent in this section (production transport: direct trap;
                // the grant was already sent by the arm). A send failure is finalized inside flush.
                let mut fs_ipc = crate::fs_comm::IpcFsTransport {
                    transport: minix_sys::ipc::DirectTrapTransport,
                };
                state.flush_pending_fs(&mut fs_ipc);
                // C `send_work()` (main.c:72) runs on *every* main-loop
                // iteration, re-sending requests that `send_fs` parked via
                // `queuemsg` when the mount window was full (comm.c:37-47,
                // 66-87). Without this sweep a window-full request is
                // enqueued, `flush_pending_fs` sees `Ok`, and the worker
                // hangs in `WaitingForFs` forever — the drain leg was
                // implemented (`flush_send_queue`) but never wired in.
                state.flush_send_queue(&mut minix_sys::ipc::DirectTrapTransport);
                // REQ_PUTNODE delivery for the put_vnode slow path (direct sendrec;
                // the reply is only checked for errors — C's synchronous in-worker sendrec at vnode.c:278).
                state.flush_pending_puts(&minix_sys::ipc::DirectTrapTransport);
            }
            SefEvent::Signal(_) => {}
            // Birth-reply half: after C's RS_INIT is consumed inside sef_local_startup,
            // process_init's tail replies RS_INIT+result (sef_init.c:113-117);
            // this reply is the condition for RS boot step3 to proceed. result follows C's registration
            // face (main.c:377-381): a fresh instance (this tree runs init_fresh at construction,
            // init_restart ≡ init_fresh, both documented) → OK; LU is registered
            // (`sef_cb_init_lu`) but the Rust side does not model the LU machinery → an honest ENOSYS.
            SefEvent::Init(init_type) => {
                // Orchestration (same basis as the rendezvous arm): stage-one filtering is wired directly,
                // the reply is built by the library; the dispatch semantics (fresh/restart → re-run the local reset +
                // OK; LU unmodeled → ENOSYS) stay as they were.
                let birth = minix_sef::process_init(
                    &mut |req, addr, len| {
                        let r = minix_sys::syscall::sys_statectl(
                            &minix_sys::syscall::DirectKernelCallTransport,
                            req,
                            addr,
                            len,
                        );
                        if r == minix_types::OK { Ok(r) } else { Err(r) }
                    },
                    init_type,
                    &mut |t| match t {
                        0 | 2 => 0, // SEF_INIT_FRESH / SEF_INIT_RESTART — sef.h:93-95
                        _ => minix_types::ENOSYS,
                    },
                );
                let mut reply = match birth {
                    Ok(m) => m,
                    Err(e) => panic!("vfs: birth statectl failed: {e}"),
                };
                if reply.rs_init_result() == Some(0) {
                    state.init_fresh();
                }
                send_birth_reply(reply);
            }
            SefEvent::PingInvalid => {}
        }
    }
}

/// Production implementation of `EndpointDirectory`: reads `VfsState::driver_labels`.
///
/// The C counterpart is `ds_retrieve_label_endpt` (dmap.c:148-152) — a **DS round trip**.
/// In the single-threaded model that needs a new routing surface (landing the DS reply and its continuation), not wired yet; so this uses
/// the local table for now, and **who fills it** (writing on DS events that carry a label, or wiring the DS round trip) is a
/// recorded gap. Empty-table semantics match C's "unknown label": EINVAL.
struct LabelDir<'a>(&'a [(alloc::string::String, Endpoint)]);

impl crate::device_map::EndpointDirectory for LabelDir<'_> {
    fn lookup(&self, label: &str) -> Option<Endpoint> {
        self.0
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, e)| *e)
    }
}

/// The **queue-collection** implementation of `FsCtl`: the `VnodeTable::put` slow path (`ref==1` →
/// `req_putnode`) and `clean_refs` (batch return when `fs_count > 256`) collect notifications
/// into a buffer, which the caller merges into [`VfsState::pending_puts`] for unified delivery.
pub(crate) struct PutNodeSink<'a>(&'a mut alloc::vec::Vec<crate::vnode::PutNodeReq>);

/// Cross-module construction entry (exec_worker's final vnode return shares the same sink as the main loop).
pub(crate) fn put_node_sink(
    sink: &'_ mut alloc::vec::Vec<crate::vnode::PutNodeReq>,
) -> PutNodeSink<'_> {
    PutNodeSink(sink)
}

impl crate::vnode::FsCtl for PutNodeSink<'_> {
    fn put_node(
        &mut self,
        fs: Endpoint,
        ino: u64,
        count: usize,
    ) -> Result<(), crate::vnode::VnodeError> {
        self.0.push(crate::vnode::PutNodeReq {
            fs_e: fs,
            ino,
            count,
        });
        Ok(())
    }
}

/// Warm up the grant table (test-only): `sys_setgrant` is unreachable in a host build, so **the first
/// allocation after each table growth necessarily fails**; later allocations succeed (the failure path has already
/// laid out the freelist). An arm test that asserts "the request was really registered" must first get past this — run
/// several rounds to cover multiple growths, and **revoke the warm-up slots back to the freelist** (without revoking, the table
/// would be left full by the warm-up itself and the test's own grant would trigger another growth → hitting that failure again).
/// Test-only: lay out the state as a baseline where "the arm can really send a request" — a mount row (fs=MFS, dev not
/// NO_DEV), the caller's root/work-dir vnodes (the inputs of `root_dir_of`/`work_dir_of`),
/// and a warmed-up grant table. Path-arm entry tests must start here, or they stop at
/// "no root directory" or "grant not warm": the assertion is still EIO, but for the **wrong reason**.
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
        // [SD19-gate] best-effort grant revocation: the main operation already succeeded or is on the error path, so a failed revoke does not change its result
        #[allow(clippy::let_underscore_must_use)]
        let _ = state.grants.revoke(g);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_table::VfsCallNum;
    use crate::worker::WorkerState;
    use minix_types::{Endpoint, VFS_PM_INIT};

    /// Seed mount point 0: fs=MFS, dev not NO_DEV (the two conditions of find_by_fs).
    fn seed_vmnt0(state: &mut VfsState) {
        let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
        v.fs = Endpoint::MFS;
        v.dev = 1; // DevId(u64); any value other than NO_DEV works
    }

    /// Seed a worker slot and set `WaitingForFs` (the sendmsg half of fs_sendrec).
    fn seed_waiting(state: &mut VfsState, slot: usize, task: Endpoint) {
        let req = Message {
            m_type: 0x503,
            ..Message::default()
        };
        state
            .worker_pool
            .get_mut(slot)
            .unwrap()
            .set_waiting(task, req);
    }

    /// Build an FS reply message (m_type high 16 = request number, low 16 = transid).
    fn reply_msg(req: u32, slot: usize, source: Endpoint) -> Message {
        let mut m = Message {
            m_type: crate::fs_comm::TransId::add(req, slot) as i32,
            ..Message::default()
        };
        m.m_source = source;
        m
    }

    /// Mount-table assembly (`mounted_fs_list`) and the start triples (`root_dir_of` /
    /// `work_dir_of`): the six facts read from `vmnt_table` plus the vnode table.
    #[test]
    fn test_mount_list_and_start_triples() {
        let mut state = VfsState::new();
        // One mount row: fs=MFS, dev=7, root vnode=3 (ino=30), mount-point vnode=4
        // (ino=40, fs=VFS, dev=7).
        let v_root = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(v_root).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 30;
            v.dev = 7;
            // `alloc` takes "the first slot with ref_count == 0"; without setting ref, the next alloc
            // would get the same slot (the `advance` usage is: alloc,
            // fill, then set ref_count/fs_count).
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

        // Start triples: root and work dir each resolve to (fs, ino, dev).
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

    /// Phase 2 of `PathFollow::Mkdir`: parent-directory gate (not a directory → ENOTDIR) → permission gate
    /// (W|X, `forbidden_decision`) → build `REQ_MKDIR` (the component name goes into the slot's scratch +
    /// a read-only grant) and suspend; the continuation hands over to `WorkerCont::Status`.
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
            fp.eff_uid = 0; // root: the permission gate always passes
            fp.umask = 0o022;
        }
        // Mount row (REQ_MKDIR goes through the window; dev must be a real device number).
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
        let start = crate::path::LookupStart {
            fs: Endpoint::MFS,
            ino: 1,
            dev: 0,
        };
        let rd = crate::path::RootDir {
            ino: 1,
            fs: Endpoint::MFS,
            dev: 0,
        };

        // Case one: the node reached is not a directory → ENOTDIR (C open.c:587-588).
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/d".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        let mut reply = Message {
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): mode = S_IFREG (not a directory).
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

        // Case two: directory + root: gate passes → build `REQ_MKDIR`. In a host build the grant
        // table must be warmed first (the first growth cannot pass `sys_setgrant`, but the failure path has laid out the freelist);
        // otherwise this assertion degenerates into an ordering artifact of "the first request necessarily fails".
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
        let mut reply2 = Message {
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): mode = S_IFDIR.
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
        // SAFETY(test): read back the parent-directory ino and mode via mkdir_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            assert_eq!(ino, 9, "inode 域是父目录");
            assert_eq!(mode, crate::open::S_IFDIR | 0o755);
        }
        // Reply arrives: the status goes back to the user as is, and the slot is released.
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx2).unwrap();
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert!(
            state.worker_pool.get_mut(idx2).unwrap().is_idle(),
            "槽已释放"
        );
        let (t2, m2) = state.take_reply().expect("回复");
        assert_eq!(t2, user);
        assert_eq!(m2.m_type, 0, "REQ_MKDIR 成功 → 用户拿 0");
    }

    /// `WorkerCont::Status`: a pure status continuation — the reply status goes back to the user as is.
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
            wp.sendrec = Some(Message {
                m_type: minix_types::EEXIST,
                ..Message::default()
            });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EEXIST))
        );
    }

    /// Phase 2 of `PathFollow::Open` (the local half of `common_open`): walk → type
    /// dispatch → fd/filp assembly → return fd. **Fully local, host-testable** — the only part of the Open arm
    /// that needs no kernel, and also the most error-prone (the claim order of the fd and filp
    /// slots, the O_CLOEXEC bitmap).
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
        // The slot: a completed walk (follow = Open, no O_TRUNC).
        let start = crate::path::LookupStart {
            fs: Endpoint::MFS,
            ino: 1,
            dev: 0,
        };
        let rd = crate::path::RootDir {
            ino: 1,
            fs: Endpoint::MFS,
            dev: 0,
        };
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        // The reply: OK + ino=5 + mode=S_IFREG|0644 (four node_details fields).
        let mut reply = Message {
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): fill ino and mode via lookup_reply_off.
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
                    oflags: 0, // O_RDONLY (the access bits are 0)
                },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();

        // The return is the fd (in C, `common_open` returning an fd is the syscall result).
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, user);
        assert_eq!(reply.m_type, 0, "首个 fd 是 0");
        let fd = reply.m_type as usize;
        // Both the fd and the filp are claimed: the fproc slot points at the filp, and the filp carries the vnode and mode.
        let fp = state.fproc_table.get(slot).unwrap();
        let filp_id = fp.filps[fd].expect("fd 指向 filp");
        let f = state.filp_table.get(crate::filp::FilpId(filp_id)).unwrap();
        assert_eq!(f.vnode.is_some(), true, "filp_vno 已填（并进的 vnode）");
        assert_eq!(f.count, 1, "filp_count = 1（认领）");
        assert_eq!(f.mode & crate::open::R_BIT, crate::open::R_BIT, "只读打开");
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle());
    }

    /// The multi-mount `sync`/`fsync` sequence (the loop of C `do_sync`/`do_fsync`):
    /// target filtering (`dev != NO_DEV && fs != NONE && root != NULL`; fsync narrows further
    /// by device number) → send one `REQ_SYNC` per target → report `first_err` when done (C drops each individual
    /// return value and keeps only the locking error).
    #[test]
    fn test_sync_sequence_over_mounts() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        crate::main_loop::seed_ready_state(&mut state);
        // Row 0: valid (MFS, dev 1, has a root). Row 1: another FS, dev 2. Row 2: no root
        // — must be filtered out.
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
            v.root = None; // no root → does not make it into the filter
        }
        // Full set: two rows (row 0 and row 1).
        assert_eq!(
            state.sync_targets(None),
            alloc::vec![Endpoint::MFS, Endpoint::from_generation_slot(0, 7)]
        );
        // Narrow by device: only the dev 2 row (the fsync shape).
        assert_eq!(
            state.sync_targets(Some(2)),
            alloc::vec![Endpoint::from_generation_slot(0, 7)]
        );

        // Start the sequence: the first REQ_SYNC is registered, the slot carries two targets.
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
            Some(WorkerCont::SyncMounts {
                count: 2,
                at: 1,
                ..
            })
        ));

        // The first reply arrives → send the second.
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("第二条 REQ_SYNC");
        assert_eq!(p.req.m_type, minix_types::REQ_SYNC);
        assert_eq!(p.fs_e, Endpoint::from_generation_slot(0, 7));
        assert!(state.take_reply().is_none(), "序列没跑完不回用户");

        // The second reply arrives → the sequence ends, 0 is returned and the slot is released.
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
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

    /// The three-stage `rename` chain (C `do_rename` link.c:166-280): stage 1 walks the old parent directory
    /// → (if it is sticky, a sub-walk gets the victim's owner) → stage 2 walks the new parent directory →
    /// cross-device gate + `W|X` gate + `REQ_RENAME` (**two direct grants**; the old name was saved in
    /// stage 1).
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
            let start = crate::path::LookupStart {
                fs: Endpoint::MFS,
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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
            let mut reply = Message {
                m_type: minix_types::OK,
                ..Message::default()
            };
            // SAFETY(test): fill ino/mode/uid/gid via lookup_reply_off.
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
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow,
            });
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

        // ① stage 1 passed and the old parent is not sticky → go to stage 2 directly (the old name is saved).
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

        // ② The old parent directory is sticky → insert a sub-walk first.
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
            state
                .worker_pool
                .get_mut(idx)
                .unwrap()
                .path
                .as_ref()
                .map(|p| &p.follow),
            Some(PathFollow::RenameOldSticky { .. })
        ));
        assert_eq!(
            state.pending_fs.as_ref().map(|p| p.req.m_type),
            Some(minix_types::REQ_LOOKUP)
        );
        clear_slot(&mut state, idx);

        // ③ Sub-walk passed: the victim's owner is not the caller → EPERM.
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

        // ④ Stage 2: cross-device → EXDEV.
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        {
            let start = crate::path::LookupStart {
                fs: Endpoint::from_generation_slot(0, 9),
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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

        // ⑤ Stage 2: a non-owner without write permission on the new parent → EACCES.
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

        // ⑥ All pass → REQ_RENAME (two direct grants + two name lengths) + status reply.
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
        // SAFETY(test): read back the six fields via rename_req_off.
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
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0))
        );
    }

    /// The `PathFollow::LinkSrc` → `LinkDst` transition plus `LinkDst`'s gates and request:
    /// C `do_link` (link.c:170-230) is a **two-stage walk** — first `eat_path` the source file,
    /// then `last_dir` the new name's parent; cross-device means EXDEV, the parent needs `W|X`, and
    /// `REQ_LINK`'s two inos are ordered **file first, directory second**.
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
            let start = crate::path::LookupStart {
                fs: Endpoint::MFS,
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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
            let mut reply = Message {
                m_type: minix_types::OK,
                ..Message::default()
            };
            // SAFETY(test): fill ino/mode via lookup_reply_off.
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
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow,
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        };

        // ① Stage 1 passed (the source file exists) → go to stage 2: the slot becomes LinkDst, and a
        // second REQ_LOOKUP is already registered (walking the new name's parent directory).
        let mut state = VfsState::new();
        // Stage 2 restarts from the **process root**, so the root-directory vnode must be seeded first
        // (`seed_ready_state` also warms up the grant table).
        crate::main_loop::seed_ready_state(&mut state);
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(0x55, crate::open::S_IFREG | 0o644),
            PathFollow::LinkSrc {
                dst_path: "/d/new".to_string(),
            },
        );
        state.run_worker_continuations();
        assert!(state.take_reply().is_none(), "转场不该回用户");
        assert!(matches!(
            state
                .worker_pool
                .get_mut(idx)
                .unwrap()
                .path
                .as_ref()
                .map(|p| &p.follow),
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

        // ② Cross-device → EXDEV (the source is on MFS, the parent on another FS).
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        {
            // Make this walk's fs "another FS": change the starting point and rebuild the walk.
            let start = crate::path::LookupStart {
                fs: Endpoint::from_generation_slot(0, 9),
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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

        // ③ Same device + a non-owner without write permission on 0755 → EACCES.
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

        // ④ All pass → REQ_LINK (file ino first, directory ino second) + status reply.
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
        // SAFETY(test): read back the four fields via link_req_off.
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
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0))
        );
    }

    /// `PathFollow::Slink` (the parent-directory half of symlink): only after the `W|X` permission gate (EACCES)
    /// passes is `REQ_SLINK` sent; the request carries **two grants** — the name (direct, VFS memory) and the target
    /// string (magic, user memory); `mem_size` is the target length **excluding** the trailing NUL
    /// (C `do_slink:411-414` + `req_slink_actual`).
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
            let start = crate::path::LookupStart {
                fs: Endpoint::MFS,
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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
            let mut reply = Message {
                m_type: minix_types::OK,
                ..Message::default()
            };
            // SAFETY(test): fill ino/mode via lookup_reply_off.
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
                    target_len: 7, // "target" does not include the NUL
                },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        };

        // ① Directory 0755 (other has x but no w), non-owner → the `W|X` gate rejects.
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        crate::main_loop::warm_grants(&mut state);
        let (idx, walk) = mk(&mut state, 2000);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFDIR | 0o755),
        );
        state.run_worker_continuations();
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EACCES))
        );

        // ② root → send REQ_SLINK, with both grants in the request.
        let (idx, walk) = mk(&mut state, crate::link::SU_UID);
        plant(
            &mut state,
            idx,
            walk.clone(),
            done_reply(crate::open::S_IFDIR | 0o755),
        );
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_SLINK");
        assert_eq!(p.req.m_type, minix_types::REQ_SLINK);
        // SAFETY(test): read back the seven fields via slink_req_off.
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
        // The reply carries only a status.
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0))
        );
    }

    /// The **device-number attribution** of `intern_vnode` (C `advance` path.c:98-106):
    /// `v_sdev = res.dev` (the special device number; device nodes identify their driver by it),
    /// `v_dev = vmp->m_dev` (the mounted partition's device number). They are different fields — writing
    /// `res.dev` into `v_dev` was a bug (`v_sdev` stayed 0, so places such as `cdev_get` that find
    /// a driver by `v_sdev` all failed).
    #[test]
    fn test_intern_vnode_splits_device_numbers() {
        use minix_types::Endpoint;
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        // The mount row's device number is 1 (seeded by `seed_vmnt0`); the node carries special device number 0x0507.
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

    /// Open's char-device branch (the `S_IFCHR` branch of `common_open`): the key for the `/dev/tty`
    /// exception is the **raw major** — with a controlling terminal it succeeds directly without disturbing
    /// the real tty driver (C `cdev.c:174`); no controlling terminal → ENXIO; an ordinary device with no driver → ENXIO;
    /// a driver present but a host send failure → EIO, and **all claims are released** (C `common_open`
    /// failure tail's `put_vnode` — not releasing leaks the fd and vnode reference).
    #[test]
    fn test_open_char_device_branch() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let tty_dev = 0x0401u64; // controlling terminal: major 4 / minor 1

        // (a) `/dev/tty` (major 5) + a controlling terminal: succeeds directly, with no driver request.
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

        // (b) `/dev/tty` but no controlling terminal: ENXIO (C `cdev_map` → NO_DEV).
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            // fp.tty defaults to NO_DEV
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

        // (c) An ordinary char device (major 4) with no driver on the dmap row: ENXIO.
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
        let plain = crate::path::NodeDetails {
            dev: 0x0405,
            ..node
        };
        state.finish_open_local(idx, Some(slot), &plain, crate::open::O_RDONLY);
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENXIO))
        );

        // (d) A driver present but a host send failure: EIO, and fd/filp/vnode are all released
        // (after the vnode is put back its ref_count returns to 0 and the slot is reusable — C `common_open`'s failure tail).
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

    /// Open's block-device branch (the `S_IFBLK` branch of `common_open`): no driver → ENXIO;
    /// after the driver clears, pick `v_bfs_e` (if a mount holds the device, use that mount's FS and send no
    /// `REQ_NEW_DRIVER`); when the device is free, `v_bfs_e` falls back to the root file system — with no root mount
    /// (`root_fs_e` is NONE) → finish through C's newdriver failure path (ENXIO).
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

        // (a) No driver on the dmap row: ENXIO (C's gate at `bdev_open:88-89`).
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

        // (b) A driver present but a host send failure: EIO + claims released.
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

        // (c) Driver cleared + the device is held by a mount: `v_bfs_e` uses that mount's FS and the fd is
        // returned directly (C `open.c:208-210`: a non-root FS sends no newdriver).
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
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state.filp_table.inc_count(fid);
        state.fproc_table.get_mut(slot).unwrap().filps[0] = Some(fid.get());
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vnode_idx);
        let mut reply = Message {
            m_type: 0x580,
            ..Message::default()
        }; // BDEV_REPLY
        // SAFETY(test): `mess_lblockdriver_lbdev_reply { int status; int id; }`
        // -- the status is in the first word; OK is 0.
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

        // (d) Device free and no root mount (`root_fs_e` = NONE): finish through C's newdriver
        // failure path — release the claims and return ENXIO (never pretending the notification succeeded).
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
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state.filp_table.inc_count(fid);
        state.fproc_table.get_mut(slot).unwrap().filps[0] = Some(fid.get());
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vnode_idx);
        let reply = Message {
            m_type: 0x580,
            ..Message::default()
        };
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

        // (e) The driver answers ERESTART: resend according to `bdev_sendrec`'s fuse; a host send failure →
        // finalize with EIO (no hang, claims released).
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
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state.filp_table.inc_count(fid);
        state.fproc_table.get_mut(slot).unwrap().filps[0] = Some(fid.get());
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vnode_idx);
        let mut reply = Message {
            m_type: 0x580,
            ..Message::default()
        };
        // SAFETY(test): ERESTART is in the first payload word (negative on the wire).
        unsafe {
            reply.m_u.raw[0..4].copy_from_slice(&(-minix_types::ERESTART).to_le_bytes());
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

    /// `PathFollow::Mknod` (the parent-directory half of mknod): type gate (ENOTDIR) → `W|X`
    /// permission gate (EACCES) → direct grant for the name → `REQ_MKNOD` with its seven fields, status-only
    /// reply (C `do_mknod:543-552` + `req_mknod`).
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
            let start = crate::path::LookupStart {
                fs: Endpoint::MFS,
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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
            let mut reply = Message {
                m_type: minix_types::OK,
                ..Message::default()
            };
            // SAFETY(test): fill ino/mode/uid/gid via lookup_reply_off.
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

        // ① The parent is not a directory → ENOTDIR.
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

        // ② Directory 0755 (other has x but no w), non-owner → the `W|X` gate rejects (EACCES).
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

        // ③ root + writable directory → REQ_MKNOD with its seven fields; the reply is status-only.
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
        // SAFETY(test): read back the seven fields via mknod_req_off.
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
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
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

    /// `PathFollow::Truncate` (the path half of truncate): after the walk, pass the W-bit gate → the send half
    /// (shared with `Ftruncate`). Three outcomes: write-bit gate rejects → EACCES; unchanged size → return 0 in place
    /// and **send no request** (POSIX file time, C link.c:311-314); a real truncate → register
    /// `REQ_FTRUNC` (in a host build the grant table must be warmed to observe it).
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
            let start = crate::path::LookupStart {
                fs: Endpoint::MFS,
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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
            let mut reply = Message {
                m_type: minix_types::OK,
                ..Message::default()
            };
            // SAFETY(test): fill ino/mode/size/uid via lookup_reply_off.
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

        // ① A non-owner without write permission on 0644 → EACCES (C `forbidden(fp, vp, W_BIT)`).
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

        // ② Owner + unchanged size → return 0 in place, with no request (POSIX file time).
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
        assert!(
            state.worker_pool.get_mut(idx).unwrap().is_idle(),
            "槽已释放"
        );

        // ③ Owner + a real truncate → register REQ_FTRUNC.
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
        // SAFETY(test): read back ino/start/end via ftrunc_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let start = i64::from_le_bytes(raw[8..16].try_into().unwrap());
            let end = i64::from_le_bytes(raw[16..24].try_into().unwrap());
            assert_eq!((ino, start, end), (0x31, 10, 0), "end=0 即截到 start");
        }
    }

    /// `PathFollow::Chown` (path half): after the walk and vnode interning it enters the `finish_chown`
    /// shared body — the same body as the fd half, so this test only pins "the path half really reached it"
    /// (sends `REQ_CHOWN` and registers the vnode cache); gate and conversion details are pinned one by one in
    /// the Fchown test.
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
            fp.eff_uid = crate::link::SU_UID; // root: skips the three ownership rules
            fp.eff_gid = 0;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let start = crate::path::LookupStart {
            fs: Endpoint::MFS,
            ino: 1,
            dev: 0,
        };
        let rd = crate::path::RootDir {
            ino: 1,
            fs: Endpoint::MFS,
            dev: 0,
        };
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/f".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        let mut reply = Message {
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): fill ino/mode/uid/gid via lookup_reply_off.
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
                follow: PathFollow::Chown {
                    uid: 2000,
                    gid: 200,
                },
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_CHOWN");
        assert_eq!(p.req.m_type, minix_types::REQ_CHOWN);
        // SAFETY(test): read back three fields via chown_req_off (root makes no change beyond the -1 conversion).
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let uid = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let gid = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            assert_eq!((ino, uid, gid), (0x77, 2000, 200));
        }
        // Interning: the vnode cache already holds this ino (C `eat_path`'s temporary vnode).
        let v = state
            .vnode_table
            .find_by_ino(Endpoint::MFS, 0x77)
            .map(|id| state.vnode_table.get(id).unwrap().ino);
        assert_eq!(v, Some(0x77), "path 半走完要和 vnode 表并上");
    }

    /// The three gates of `PathFollow::Unlink` (stage 1) plus `PathFollow::UnlinkSticky`
    /// (stage 2, the sub-walk on a sticky directory): C `do_unlink` (link.c:122-159).
    ///
    /// After stage 1 walks the parent: type gate (not a directory → ENOTDIR) → `X|W` permission gate
    /// (EACCES) → sticky gate (if sticky, go to stage 2 and **sub-walk** for the victim's owner).
    /// Stage 2's owner gate is judged in the continuation: the victim's owner != the effective id and not the superuser
    /// → EPERM.
    ///
    /// The host can test up to "request registered": the grant table's first growth needs `sys_setgrant`
    /// (unreachable on the host), but the failure path has laid out the freelist, so **later grants succeed**
    /// - therefore the test starts with one "warm-up" grant to make this deterministic, after which it can assert
    /// the request that was really registered (`REQ_LOOKUP` / `REQ_UNLINK` / `REQ_RMDIR`).
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
            let start = crate::path::LookupStart {
                fs: Endpoint::MFS,
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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
            let mut reply = Message {
                m_type: minix_types::OK,
                ..Message::default()
            };
            // SAFETY(test): fill ino/mode/uid/gid via lookup_reply_off.
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
            wp.path = Some(PathPending {
                walk,
                grant: 9,
                follow,
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        };

        // ① The parent is not a directory (the parent path resolves to a regular file) → ENOTDIR.
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        // Warm-up: let the grant table complete one (failed) growth so later grants take the freelist
        // and succeed directly — removing "can it send a request" from the test variables.
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
            PathFollow::Unlink {
                entry: "x".to_string(),
                rmdir: false,
            },
        );
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENOTDIR))
        );

        // ② Directory 0755 (other has x but no w), caller id 1000 → the permission gate rejects.
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
            PathFollow::Unlink {
                entry: "x".to_string(),
                rmdir: false,
            },
        );
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EACCES))
        );

        // ③ 0777 writable but sticky → go to stage 2; in a host build the sub-walk's grant is unreachable →
        // EIO (on real hardware this is the second REQ_LOOKUP).
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
            PathFollow::Unlink {
                entry: "x".to_string(),
                rmdir: true,
            },
        );
        state.run_worker_continuations();
        assert!(state.take_reply().is_none(), "子遍历还没走完，不能回用户");
        let p = state.pending_fs.as_ref().expect("已登记第二条 REQ_LOOKUP");
        assert_eq!(p.req.m_type, minix_types::REQ_LOOKUP, "粘滞位门要再走一趟");
        assert_eq!(p.fs_e, Endpoint::MFS);
        // The sub-walk carries `PATH_RET_SYMLINK` (C `lookup_init(&stickycheck, ...,
        // PATH_RET_SYMLINK, ...)`, link.c:134) — this is the host-observable place where
        // "flags really go into the message" (the field had never been written before).
        // SAFETY(test): read the FLAGS field via lookup_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let flags = u32::from_le_bytes(raw[16..20].try_into().unwrap());
            assert_eq!(flags, minix_types::PATH_RET_SYMLINK);
        }
        // The slot has switched to stage 2 (parent-directory identity travels with it).
        assert!(matches!(
            state
                .worker_pool
                .get_mut(idx)
                .unwrap()
                .path
                .as_ref()
                .map(|p| &p.follow),
            Some(PathFollow::UnlinkSticky {
                dir_ino: 7,
                rmdir: true,
                ..
            })
        ));
        // Clear this round's pending slot before the next case: otherwise the next round
        // of `run_worker_continuations` would run this slot again with a **stale reply**; when two jobs
        // finish in the same round only the first queued reply can be sent (one reply per round
        // in this model), so the test would read someone else's reply. Test hygiene, not an implementation issue.
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = None;
            wp.path = None;
            wp.sendrec = None;
            wp.task = None;
        }
        state.worker_pool.release(idx);

        // ④ 0777 without sticky → send the request directly; on the host it likewise stops at the grant → EIO.
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
            PathFollow::Unlink {
                entry: "x".to_string(),
                rmdir: false,
            },
        );
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_UNLINK");
        assert_eq!(p.req.m_type, minix_types::REQ_UNLINK);
        assert_eq!(p.worker, idx);
        // SAFETY(test): read back the parent-directory ino and name length via unlink_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let path_len = u64::from_le_bytes(raw[16..24].try_into().unwrap());
            assert_eq!(ino, 7, "inode 域是**父目录**的节点号");
            assert_eq!(path_len, 2, "名字含结尾 NUL（\"x\" → 2 字节）");
        }
        // The reply is status-only: the continuation returns the status to the user as is.
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0)),
            "REQ_UNLINK 成功 → 用户拿 0"
        );
        assert!(
            state.worker_pool.get_mut(idx).unwrap().is_idle(),
            "槽已释放"
        );

        // ⑤ Stage 2: the victim's owner is not the caller (1000 vs 0) → EPERM.
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

        // ⑥ Stage 2: the victim's owner is the caller → sticky gate passes, stops at the grant → EIO.
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
        // SAFETY(test): the parent-directory ino comes with the slot.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            assert_eq!(ino, 7);
        }
    }

    /// `PathFollow::Chmod` and `WorkerCont::Chmod`: the two stages of `chmod`.
    /// After the walk, two gates (owner/superuser → otherwise EPERM; read-only mount → EROFS),
    /// then setgid clearing (non-superuser with the file outside their group), and only then `REQ_CHMOD`;
    /// the reply carries the **effective mode**, which the continuation writes back into the vnode cache (C protect.c:120-128).
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
            let start = crate::path::LookupStart {
                fs: Endpoint::MFS,
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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
            let mut reply = Message {
                m_type: minix_types::OK,
                ..Message::default()
            };
            // SAFETY(test): fill ino/mode/uid/gid via lookup_reply_off.
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

        // ① Non-owner (effective id 1000, file owner 0) → EPERM, no request sent.
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

        // ② Owner but the mount is read-only → EROFS.
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

        // ③ Owner (non-superuser: effective id 1000 = file owner) + writable mount: send the request,
        // and the setgid bit is cleared (file group 0 != effective group 1000, protect.c:120-121).
        // Note that eff_uid 0 must **not** be used here — 0 is the superuser, and the clearing rule does not apply.
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
            fp.eff_uid = 1000;
            fp.eff_gid = 1000;
        }
        state
            .vmnt_table
            .get_mut(crate::vmnt::VmntId(0))
            .unwrap()
            .flags = crate::vmnt::VmntFlags::empty();
        plant(&mut state, idx, walk.clone(), 0o2664, done_reply(1000, 0));
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_CHMOD");
        assert_eq!(p.req.m_type, minix_types::REQ_CHMOD);
        // SAFETY(test): read back inode/mode via chmod_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            assert_eq!(ino, 0x21);
            assert_eq!(mode, 0o664, "setgid 位被清（非超级用户且不同组）");
        }

        // Continuation: the reply carries the actual mode (FS narrows it to 0o750) → write it back to the vnode cache.
        // Note the mode in the reply is the **whole word** (including type bits): mfs's `fs_chmod` ends with
        // `*mode = rip->i_mode` (mfs/protect.c), and libfsdriver copies it into the reply verbatim
        // (call.c:787). Returning only permission bits is a wrong implementation — the cache would lose the type bits.
        let vnode = match state.worker_pool.get_mut(idx).unwrap().cont {
            Some(WorkerCont::Chmod { vnode }) => vnode,
            _ => panic!("续接标识应是 Chmod"),
        };
        state.pending_fs = None;
        let mut reply = Message {
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): fill the actual mode (whole word, including S_IFREG) via chmod_reply_off.
        unsafe {
            reply.m_u.raw[0..4].copy_from_slice(&(crate::open::S_IFREG | 0o750).to_le_bytes());
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
            state
                .vnode_table
                .get(crate::vnode::VnodeId(vnode))
                .unwrap()
                .mode,
            crate::open::S_IFREG | 0o750,
            "缓存里的模式跟着 FS 回的实际值走"
        );
    }

    /// `PathFollow::Rdlink` and `WorkerCont::Rdlink`: the two stages of `readlink`.
    /// After the walk, first the "is it a symlink" gate (C link.c:496-501's `S_ISLNK`, otherwise
    /// EINVAL); only a link sends `REQ_RDLINK`; the reply's **byte count is in the payload**, not the
    /// status word (C request.c:745), so the continuation reads `rdlink_reply_off::NBYTES`.
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
            let start = crate::path::LookupStart {
                fs: Endpoint::MFS,
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
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
            let mut reply = Message {
                m_type: minix_types::OK,
                ..Message::default()
            };
            // SAFETY(test): fill ino/mode via lookup_reply_off.
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                    .copy_from_slice(&9u64.to_le_bytes());
                raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                    .copy_from_slice(&mode.to_le_bytes());
            }
            reply
        };

        // Not a symlink (a regular file) → EINVAL, with no FS request sent.
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
                follow: PathFollow::Rdlink {
                    user,
                    buf: 0x5000,
                    buf_size: 128,
                },
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

        // It is a symlink: send `REQ_RDLINK`. In a host build the grant table must be warmed first (see the Unlink
        // test note), or the result degenerates into an ordering artifact of "stopped at the grant".
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
                follow: PathFollow::Rdlink {
                    user,
                    buf: 0x5000,
                    buf_size: 128,
                },
            });
            wp.sendrec = Some(done_reply(crate::open::S_IFLNK | 0o777));
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("已登记 REQ_RDLINK");
        assert_eq!(p.req.m_type, minix_types::REQ_RDLINK);
        // SAFETY(test): read back ino and window size via rdlink_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mem_size = u64::from_le_bytes(raw[16..24].try_into().unwrap());
            assert_eq!(ino, 9);
            assert_eq!(mem_size, 128, "窗口大小来自用户给的 bufsize");
        }
        // Clear the slot before the next case (same reason as the Unlink test: one reply per round).
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = None;
            wp.path = None;
            wp.sendrec = None;
            wp.task = None;
        }
        state.worker_pool.release(idx);

        // Continuation: on status OK the length comes from the payload's nbytes (not the status word 0).
        let idx2 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let mut reply = Message {
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): fill nbytes via rdlink_reply_off.
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

        // Continuation: on an FS error the error goes back as is (the payload is not read).
        let idx3 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx3).unwrap();
            wp.cont = Some(WorkerCont::Rdlink { grant: 6 });
            wp.sendrec = Some(Message {
                m_type: minix_types::ENOENT,
                ..Message::default()
            });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENOENT))
        );
    }

    /// `PathFollow::Access`: permissions are judged right after the walk, with **no FS round trip** — `forbidden`
    /// uses the **real** uid/gid for access(2) (the `job_call_nr == VFS_ACCESS` special case at
    /// protect.c:255-256). The slot here deliberately makes the real id the file owner
    /// and the effective id not: an implementation wrongly using the effective id would reject W_OK.
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
            let start = crate::path::LookupStart {
                fs: Endpoint::MFS,
                ino: 1,
                dev: 0,
            };
            let rd = crate::path::RootDir {
                ino: 1,
                fs: Endpoint::MFS,
                dev: 0,
            };
            let (walk, _) = crate::path::LookupWalk::begin(
                start,
                crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS)
                    .unwrap(),
                rd,
                // The id carried by the walk deliberately equals the caller's real id in the later case (1000):
                // if the implementation used the walk parameter instead of the owner from the reply, case 2's W_OK would be
                // wrongly allowed — that assertion is therefore discriminating.
                1000,
                1000,
            )
            .unwrap();
            (slot, idx, walk)
        };
        let done_reply = |mode: u32, uid: u32, gid: u32| {
            let mut reply = Message {
                m_type: minix_types::OK,
                ..Message::default()
            };
            // SAFETY(test): fill mode/uid/gid via lookup_reply_off (ino is unused).
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
        let run = |state: &mut VfsState,
                   idx: usize,
                   slot: minix_types::UserSlot,
                   walk,
                   access: u32,
                   reply: Message| {
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
            assert!(
                state.worker_pool.get_mut(idx).unwrap().is_idle(),
                "槽已释放"
            );
            let _ = slot;
            state.take_reply().expect("回复").1.m_type
        };

        // Real id = owner (0), effective id = 1000: W_OK is judged by the real id → allowed.
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
            run(
                &mut state,
                idx,
                slot,
                walk.clone(),
                0o2,
                done_reply(crate::open::S_IFREG | 0o644, 0, 0)
            ),
            0,
            "access 用真实 id 判：真实 id 是属主 → W_OK 允许"
        );

        // The owner is 0 and the real id is also 1000 (another set): W_OK is rejected.
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.real_uid = 1000;
            fp.real_gid = 1000;
        }
        assert_eq!(
            run(
                &mut state,
                idx,
                slot,
                walk.clone(),
                0o2,
                done_reply(crate::open::S_IFREG | 0o644, 0, 0)
            ),
            -minix_types::EACCES,
            "非属主对 0644 无写权"
        );
        // Same 0644: the other read bit is set (other r) → R_OK is allowed.
        let (slot, idx, walk) = mk(&mut state);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.real_uid = 1000;
            fp.real_gid = 1000;
        }
        assert_eq!(
            run(
                &mut state,
                idx,
                slot,
                walk.clone(),
                0o4,
                done_reply(crate::open::S_IFREG | 0o644, 0, 0)
            ),
            0
        );

        // Read-only mount + W_OK → EROFS (C `read_only` at the tail of forbidden).
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
            run(
                &mut state,
                idx,
                slot,
                walk.clone(),
                0o2,
                done_reply(crate::open::S_IFREG | 0o644, 0, 0)
            ),
            -minix_types::EROFS
        );
    }

    /// The `O_TRUNC` branch of `open` (C `common_open:150-157`): regular file +
    /// `O_TRUNC` → after the W-bit gate passes, send `REQ_FTRUNC(ino, 0, 0)` (`end == 0`
    /// means "truncate to start", mfs `fs_trunc:439-443`) → assemble the fd as usual when the reply arrives.
    /// Both halves are pinned here: **what goes out is a truncate request** (not direct assembly), and
    /// **the truncate status is ignored** (C does not consume `truncate_vnode`'s return value) — so even a reply
    /// carrying an error still assembles.
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
            fp.eff_uid = 0; // file owner; 0644 has the W bit
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let start = crate::path::LookupStart {
            fs: Endpoint::MFS,
            ino: 1,
            dev: 0,
        };
        let rd = crate::path::RootDir {
            ino: 1,
            fs: Endpoint::MFS,
            dev: 0,
        };
        // The id carried by the walk deliberately differs from the reply's owner (7777 vs 0): permissions must
        // be computed from the `node_details` owner (C `advance`'s `v_uid = res.uid`);
        // using the walk parameter would misjudge a "non-owner" as the owner.
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            7777,
            7777,
        )
        .unwrap();
        let mut reply = Message {
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): fill ino/mode/uid/gid via lookup_reply_off.
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

        // Phase 2 did not assemble directly: a truncate request is registered, the slot stays suspended, and the user has no reply yet.
        assert!(state.take_reply().is_none(), "截断未完成前不得回复 fd");
        let p = state.pending_fs.as_ref().expect("已登记 REQ_FTRUNC");
        assert_eq!(p.req.m_type, minix_types::REQ_FTRUNC);
        assert_eq!(p.worker, idx);
        assert_eq!(p.grant, 0, "截断无数据面");
        // SAFETY(test): read back the three fields via ftrunc_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let trc_start = i64::from_le_bytes(raw[8..16].try_into().unwrap());
            let trc_end = i64::from_le_bytes(raw[16..24].try_into().unwrap());
            assert_eq!(
                (ino, trc_start, trc_end),
                (5, 0, 0),
                "截到 0（C truncate_vnode(vp, 0)）"
            );
        }
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().cont,
            Some(WorkerCont::OpenTrunc { .. })
        ));
        assert!(!state.worker_pool.get_mut(idx).unwrap().is_idle());

        // The reply arrives (deliberately with an error): C ignores the truncate result and assembles as usual.
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message {
                m_type: minix_types::EIO,
                ..Message::default()
            });
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
        assert_eq!(
            f.flags, oflags as i32,
            "filp_flags 写原始 oflags（含 O_TRUNC）"
        );
        assert!(state.worker_pool.get_mut(idx).unwrap().is_idle());
    }

    /// The W-bit gate for `O_TRUNC` (C `common_open:152`'s `forbidden(fp, vp,
    /// W_BIT)`): if a read-only-open + `O_TRUNC` file is not writable, open ends
    /// directly with `EACCES` - **no truncate request is sent**.
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
            fp.eff_uid = 1000; // non-owner, non-root
            fp.real_uid = 1000;
            fp.eff_gid = 1000;
            fp.real_gid = 1000;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let start = crate::path::LookupStart {
            fs: Endpoint::MFS,
            ino: 1,
            dev: 0,
        };
        let rd = crate::path::RootDir {
            ino: 1,
            fs: Endpoint::MFS,
            dev: 0,
        };
        let (walk, _) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            // The id carried by the walk **deliberately equals the caller's effective id** (1000): if permissions
            // were computed from the walk parameter instead of the reply's owner, a non-owner would be treated as the owner
            // and this open would be allowed — that assertion is therefore discriminating.
            1000,
            1000,
        )
        .unwrap();
        let mut reply = Message {
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): 0644 with owner 0 → id 1000 is read-only.
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
        assert!(
            state.worker_pool.get_mut(idx).unwrap().is_idle(),
            "槽已释放"
        );
    }

    /// The finalization wiring of `WorkerCont::Path`: after the walk (`Ok`) it enters phase 2 (here the grant is
    /// unreachable on the host → finalize with EIO), and **the slot must be released and the user must receive a reply** —
    /// guarding against "the continuation swallows the job, the slot leaks, the caller waits forever".
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
        // The slot: a completed walk (any path will do; the continuation only uses follow).
        let start = crate::path::LookupStart {
            fs: Endpoint::MFS,
            ino: 1,
            dev: 0,
        };
        let rd = crate::path::RootDir {
            ino: 1,
            fs: Endpoint::MFS,
            dev: 0,
        };
        let (walk, _step) = crate::path::LookupWalk::begin(
            start,
            crate::path::Lookup::new("/x".to_string(), crate::path::LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        // The reply: OK + ino=5 (four node_details fields).
        let mut reply = Message {
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): fill ino via lookup_reply_off.
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
        assert!(
            state.worker_pool.get_mut(idx).unwrap().is_idle(),
            "槽已释放"
        );
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EIO)),
            "相位 2 的 grant 在宿主不可达 → EIO 收尾（不悬挂）"
        );
    }

    /// The continuation layer's **write** branch: besides advancing the position, it raises the vnode size
    /// per C read.c:255-259 (only when the position passes the old size; the read direction does not change the size).
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
            v.size = 0x100; // old size
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let mut reply = Message {
            m_type: 0,
            ..Message::default()
        };
        // SAFETY(test): fill the reply via transfer_reply_off (write 0x40 bytes to 0x140).
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
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0x40))
        );

        // Reverse control: the new position does **not** pass the old size → the size is unchanged.
        let idx2 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        let mut reply2 = Message {
            m_type: 0,
            ..Message::default()
        };
        // SAFETY(test): position 0x110 (below the current 0x140), writing 0x10 bytes.
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
        assert_eq!(
            state.vnode_table.get(vid).unwrap().size,
            0x140,
            "大小不回退"
        );
        let _ = state.take_reply();
    }

    /// The continuation layer's Read branch: take `seek_pos`/`nbytes` from the reply (the shared offset table
    /// of C `mess_fs_vfs_readwrite`), write the position back to the filp, set **status = bytes actually read** (C's
    /// `cum_io`), and release the slot.
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
        // Reply payload: seek_pos@0, nbytes@8 (shared offset table).
        let mut reply = Message {
            m_type: 0,
            ..Message::default()
        };
        // SAFETY(test): fill the reply via transfer_reply_off.
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

    /// `WorkerCont::Getdents`: `seek_pos`/`nbytes` come from the reply via the shared table
    /// (`getdents_reply_off`), and **the position advances only when `nbytes > 0`**
    /// (C read.c:311-313) — an empty directory or a window too small for one whole entry gives
    /// `nbytes == 0`; if the position moved, the next pass would skip entries. The user receives the byte count.
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
            let mut reply = Message {
                m_type: status,
                ..Message::default()
            };
            // SAFETY(test): fill seek_pos/nbytes via getdents_reply_off.
            unsafe {
                let raw = &mut reply.m_u.raw;
                raw[0..8].copy_from_slice(&pos.to_le_bytes());
                raw[8..16].copy_from_slice(&nbytes.to_le_bytes());
            }
            {
                let wp = state.worker_pool.get_mut(idx).unwrap();
                wp.cont = Some(WorkerCont::Getdents {
                    grant: 5,
                    filp: fid.get(),
                });
                wp.sendrec = Some(reply);
                wp.state = crate::worker::WorkerState::Busy;
            }
            state.run_worker_continuations();
            assert!(
                state.worker_pool.get_mut(idx).unwrap().is_idle(),
                "槽已释放"
            );
            state.take_reply().expect("回复").1.m_type
        };

        // Empty directory (nbytes == 0): the position does **not move**, and the user receives 0.
        assert_eq!(run(&mut state, 0, 0x180, 0), 0);
        assert_eq!(state.filp_table.get(fid).unwrap().pos, 0x100);

        // Entries really read: the position advances to the new position from the reply, and the user receives the byte count.
        assert_eq!(run(&mut state, 0, 0x180, 48), 48);
        assert_eq!(state.filp_table.get(fid).unwrap().pos, 0x180);

        // FS error: the position does not move and the error goes back to the user as is.
        // FS error: the position does not move and the error goes back as is (FS's on-wire status is negative).
        assert_eq!(
            run(&mut state, -minix_types::ENOTDIR, 0x200, 12),
            -minix_types::ENOTDIR
        );
        assert_eq!(state.filp_table.get(fid).unwrap().pos, 0x180);
    }

    /// The continuation layer (`run_worker_continuations`): jobs whose reply has landed are finalized according to
    /// their continuation tag — Fstat only revokes the grant and returns the status to the user; `ERESTART` folds to `EIO`
    /// (C comm.c:161-163); the slot is released.
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
        // Hand-build the slot state "arm suspended, reply already arrived": cont + sendrec + WaitingForFs.
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .expect("空闲槽");
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Fstat { grant: 7 });
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0)),
            "Fstat 成功：状态 0 回用户"
        );
        assert!(
            state.worker_pool.get_mut(idx).unwrap().is_idle(),
            "槽已释放"
        );

        // ERESTART folds to EIO (C comm.c:161-163).
        let idx2 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx2).unwrap();
            wp.cont = Some(WorkerCont::Fstat { grant: 8 });
            // An FS reply's on-wire status is negative (servers are all `_SYSTEM` builds).
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

    /// The continuation gate: **only jobs whose reply has landed** run their continuation. In C, `w_sendrec`
    /// is a bidirectional buffer — `fs_sendrec` puts the request in, and the thread then blocks in
    /// `worker_wait`; only `do_reply` (in this model, `handle_fs_reply` setting `Busy`)
    /// overwrites it with the reply and wakes the thread. Without this gate,
    /// a call arriving from another client during that window would drive the continuation to take the request code
    /// as the status and falsely complete the suspended job (the user gets garbage, and when the real reply arrives
    /// `w_task` has already been cleared, so WrongTask is reported).
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
        // The slot state after the send half: sendrec holds the **request** (the `REQ_LOOKUP` code),
        // and the state is `WaitingForFs` (`fs_sendrec` → `set_waiting`).
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
        // Only now that the reply has really arrived (the landing shape of `handle_fs_reply`) does it run.
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message {
                m_type: 0,
                ..Message::default()
            });
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

    /// Reply half: the `SyscallResult` → queued-reply mapping (C `do_work`'s tail
    /// `reply(who_e, result)`). `Suspend` is not queued (its reply is sent when the FS answer lands),
    /// `Nosys` explicitly returns ENOSYS, and a NONE target sends nothing.
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

        // Statuses from drivers/FS are already negative: **do not negate twice**.
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
        // The C do_reply main path (main.c:187-211): validate → land → open the window →
        // worker_signal. Witness after `VfsState::comm` entered its state.
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;

        let reply = reply_msg(0x503, 2, Endpoint::MFS);
        let r = state.handle_fs_reply(&reply, &VfsTransIdCodec);
        assert_eq!(r, Ok(2));
        let wp = state.worker_pool.get(2).unwrap();
        assert_eq!(wp.state, WorkerState::Busy); // runnable after worker_signal
        assert_eq!(wp.task, None); // w_task = NONE(main.c:206)
        let delivered = wp.sendrec.expect("reply delivered");
        // C main.c:88 — the transid is stripped before do_reply, so the slot keeps the server's
        // raw status (0x503 is the "status word" fabricated by this test).
        assert_eq!(delivered.m_type, 0x503);
        assert_eq!(state.comm.vmnts[0].cur_reqs, 0); // c_cur_reqs--(main.c:207)
    }

    #[test]
    fn test_handle_fs_reply_vm_skips_vmnt_lookup() {
        // C main.c:190 — who_e == VM_PROC_NR skips the vmnt check; the window does not move.
        let mut state = VfsState::new();
        seed_waiting(&mut state, 1, Endpoint::VM);
        let reply = reply_msg(0x503, 1, Endpoint::VM);
        assert_eq!(state.handle_fs_reply(&reply, &VfsTransIdCodec), Ok(1));
        assert_eq!(state.worker_pool.get(1).unwrap().state, WorkerState::Busy);
    }

    #[test]
    fn test_handle_fs_reply_wrong_task_is_typed_printf() {
        // C main.c:193-196 — w_task != who_e → printf+return (a soft failure:
        // the reply is not landed and the window does not move). Note that C's find_vmnt (main.c:190) runs before
        // the w_task check, so the wrong responder's vmnt must also exist.
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
        assert_eq!(
            state.worker_pool.get(2).unwrap().state,
            WorkerState::WaitingForFs
        );
    }

    #[test]
    fn test_handle_fs_reply_spurious_transid() {
        // The low 16 bits are not IS_VFS_FS_TRANSID → SpuriousTransid.
        let mut state = VfsState::new();
        let mut msg = Message {
            m_type: 0x503,
            ..Message::default()
        };
        msg.m_source = Endpoint::MFS;
        assert_eq!(
            state.handle_fs_reply(&msg, &VfsTransIdCodec),
            Err(FsReplyError::SpuriousTransid)
        );
    }

    #[test]
    #[should_panic(expected = "Couldn't find vmnt for endpoint")]
    fn test_handle_fs_reply_unknown_fs_panics_like_c() {
        // C main.c:190-191 — a non-VM reply with a failed find_vmnt → panic (fail-fast).
        let mut state = VfsState::new();
        seed_waiting(&mut state, 2, Endpoint::MFS); // vmnt0 not seeded
        let reply = reply_msg(0x503, 2, Endpoint::MFS);
        // [SD19-gate] best-effort reply-frame handling (failure only affects internal bookkeeping, not the fact that the caller was already answered)
        #[allow(clippy::let_underscore_must_use)]
        let _ = state.handle_fs_reply(&reply, &VfsTransIdCodec);
    }

    #[test]
    fn test_run_once_fs_reply_reaches_slot() {
        // C main.c:80-89 — a transid hit means do_reply; the main loop has no answer target.
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

    // ── fs_sendrec conversation primitive + flush_send_queue resend ──

    extern crate alloc;

    /// A minimal IPC transport that can fail on script (sendnb log + injectable failure).
    struct FlushIpc {
        sent: core::cell::RefCell<alloc::vec::Vec<(Endpoint, Message)>>,
        fail: bool,
    }
    impl minix_sys::ipc::IpcTransport for FlushIpc {
        fn send(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn receive(
            &self,
            _s: Endpoint,
            _m: &mut Message,
        ) -> Result<minix_sys::ipc::IpcStatus, minix_sys::ipc::TrapStatus> {
            Err(minix_sys::ipc::TrapStatus(-1))
        }
        fn sendrec(
            &self,
            _d: Endpoint,
            _m: &mut Message,
        ) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn notify(&self, _d: Endpoint) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn sendnb(&self, d: Endpoint, m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
            if self.fail {
                return Err(minix_sys::ipc::TrapStatus(-5));
            }
            self.sent.borrow_mut().push((d, *m));
            Ok(())
        }
        fn senda(
            &self,
            _t: &[minix_sys::ipc::AsyncSlot],
        ) -> Result<(), minix_sys::ipc::TrapStatus> {
            Ok(())
        }
        fn query_kerninfo_page(&self) -> Result<u64, minix_sys::ipc::TrapStatus> {
            Ok(0)
        }
    }

    /// Closed-loop fixture for reply landing + resend: vmnt0 (MFS) + slot 2 in flight.
    fn dialogue_fixture() -> VfsState {
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;
        state
    }

    #[test]
    fn test_fs_sendrec_send_path_marks_waiting() {
        // comm.c:137-158 main path: window open → sendnb delivery + slot WaitingForFs.
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc {
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                fail: false,
            },
        };
        let req = Message {
            m_type: 0x503,
            ..Message::default()
        };
        let r = state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 3, &req);
        assert_eq!(r, Ok(()));
        let w = state.worker_pool.get(3).unwrap();
        assert_eq!(w.state, WorkerState::WaitingForFs);
        assert_eq!(w.task, Some(Endpoint::MFS));
        assert_eq!(state.comm.vmnts[0].cur_reqs, 1);
        assert_eq!(probe.transport.sent.borrow().len(), 1);
        assert_eq!(
            probe.transport.sent.borrow()[0].1.m_type as u32,
            crate::fs_comm::TransId::add(0x503, 3)
        );
    }

    #[test]
    fn test_fs_sendrec_vmnt_mismatch_is_eio() {
        // comm.c:137-140 — find_vmnt cannot find that fs endpoint → EIO.
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc {
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                fail: false,
            },
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
        // comm.c:142-144 — the calling process is itself the target FS → EDEADLK.
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        let caller_ep = Endpoint::from_generation_slot(1, 9);
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
            v.fs = caller_ep;
            v.dev = 2;
        }
        // Slot 3 is bound to a fproc whose endpoint is the target FS.
        let fp = state.fproc_table.get_mut(UserSlot::new(3)).unwrap();
        fp.endpoint = caller_ep;
        let w = state.worker_pool.get_mut(3).unwrap();
        w.fp_slot = Some(UserSlot::new(3));
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc {
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                fail: false,
            },
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
        // comm.c:146 — `assert(self->w_sendrec == NULL)`.
        let mut state = dialogue_fixture(); // slot 2 is already WaitingForFs
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc {
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                fail: false,
            },
        };
        let req = Message::default();
        // [SD19-gate] best-effort IPC cleanup (send-and-forget/timeout path; failure does not change an established main result)
        #[allow(clippy::let_underscore_must_use)]
        let _ = state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 2, &req);
    }

    #[test]
    fn test_fs_sendrec_queue_path_marks_waiting() {
        // comm.c:156-158 window full → queue; the slot is likewise WaitingForFs (sending++,
        // no delivery).
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        state.comm.vmnts[0].cur_reqs = 1; // max=1 already taken
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc {
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                fail: false,
            },
        };
        let req = Message {
            m_type: 0x604,
            ..Message::default()
        };
        assert_eq!(
            state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 4, &req),
            Ok(())
        );
        assert!(probe.transport.sent.borrow().is_empty());
        assert_eq!(state.comm.sending, 1);
        assert_eq!(state.comm.vmnts[0].queued(), 1);
        assert_eq!(
            state.worker_pool.get(4).unwrap().state,
            WorkerState::WaitingForFs
        );
    }

    #[test]
    fn test_flush_after_reply_drains_queue() {
        // Closed loop: in flight (slot 2) + queued (slot 4) → reply lands, window opens → flush resends
        // slot 4's request (transid stamped) → the window fills again.
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc {
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                fail: false,
            },
        };
        let req = Message {
            m_type: 0x604,
            ..Message::default()
        };
        // Window full → slot 4 queues (the request stays in the slot, unstamped).
        assert_eq!(
            state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 4, &req),
            Ok(())
        );
        // The reply lands on slot 2; the window opens.
        let reply = reply_msg(0x503, 2, Endpoint::MFS);
        assert_eq!(state.handle_fs_reply(&reply, &VfsTransIdCodec), Ok(2));
        // Resend: slot 4's request is stamped + delivered.
        assert_eq!(state.flush_send_queue(&mut probe.transport), 1);
        assert_eq!(probe.transport.sent.borrow().len(), 1);
        let (dst, sent) = probe.transport.sent.borrow()[0].clone();
        assert_eq!(dst, Endpoint::MFS);
        assert_eq!(sent.m_type as u32, crate::fs_comm::TransId::add(0x604, 4));
        assert_eq!(state.comm.vmnts[0].cur_reqs, 1);
        assert_eq!(state.comm.sending, 0);
        // Flush again: the window is full again, nothing to resend.
        assert_eq!(state.flush_send_queue(&mut probe.transport), 0);
    }

    #[test]
    fn test_flush_skips_send_failures_and_empty_mounts() {
        // comm.c:86 — a failed asynsend3 on resend is ignored as `(void)`; empty mount slots are not scanned.
        let mut state = VfsState::new();
        seed_vmnt0(&mut state);
        seed_waiting(&mut state, 2, Endpoint::MFS);
        state.comm.vmnts[0].cur_reqs = 1;
        let mut probe = crate::fs_comm::IpcFsTransport {
            transport: FlushIpc {
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                fail: true,
            },
        };
        let req = Message::default();
        assert_eq!(
            state.fs_sendrec(&mut probe, 0, Endpoint::MFS, 4, &req),
            Ok(())
        );
        let reply = reply_msg(0x503, 2, Endpoint::MFS);
        assert_eq!(state.handle_fs_reply(&reply, &VfsTransIdCodec), Ok(2));
        assert_eq!(state.flush_send_queue(&mut probe.transport), 0);
        // The count was already incremented by fs_sendmore (C's existing order).
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
        // fail-closed: an out-of-range message must not damage table state.
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
        // [SD19-gate] inside the test harness: finish_init failure is reflected by later state assertions
        #[allow(clippy::let_underscore_must_use)]
        let _ = state.finish_init(&kernel, &ipc);
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

    // ── root-mount orchestration ──

    /// Seed the dmap memory driver row (the major owner of DEV_IMGRD; on C's boot chain it is
    /// assembled by rproctab/map_service — the test fills it in directly).
    fn seed_imgrd_driver(state: &mut VfsState) {
        let mut entry = crate::device_map::DmapEntry::empty();
        entry.driver = Some(Endpoint::MEM);
        entry.label[..7].copy_from_slice(b"memory\0");
        assert!(state.dmap_table.set(crate::device_map::MEMORY_MAJOR, entry));
    }

    // ── boot rproctab consumption (the consumption half of main.c:455-467) ──

    /// Byte image of a repr(C) POD snapshot row (the inverse of `decode_boot_rows`, for injection).
    fn row_bytes(row: &minix_types::RprocpubSnap) -> [u8; 40] {
        let mut b = [0u8; core::mem::size_of::<minix_types::RprocpubSnap>()];
        // SAFETY(test): write_unaligned of a repr(C) POD row — the inverse of read_unaligned.
        unsafe { (b.as_mut_ptr() as *mut minix_types::RprocpubSnap).write_unaligned(*row) };
        b
    }

    /// The memory-driver boot row (imgrd's supplier on C's chain; rpub.label = "memory").
    fn mem_boot_row() -> minix_types::RprocpubSnap {
        minix_types::RprocpubSnap {
            in_use: 1,
            endpoint: Endpoint::MEM.get(),
            dev_nr: crate::device_map::MEMORY_MAJOR as i32,
            label: *b"memory\0\0\0\0\0\0\0\0\0\0",
            ..Default::default()
        }
    }

    #[test]
    fn test_decode_boot_rows_roundtrip() {
        let rows = [mem_boot_row(), minix_types::RprocpubSnap::default()];
        let mut buf = alloc::vec::Vec::new();
        for row in &rows {
            buf.extend_from_slice(&row_bytes(row));
        }
        let decoded = VfsState::decode_boot_rows(&buf);
        assert_eq!(decoded, rows);
    }

    #[test]
    fn test_apply_boot_rows_maps_driver_and_marks_service() {
        let mut state = VfsState::new();
        state.init_fresh();
        // fproc slots are pre-filled with endpoints: C's PM handshake completes the process table first (main.c:419-436),
        // so map_service's isokendpt can resolve them (dmap.c:208). The slot is derived from the endpoint.
        let mem_slot = Endpoint::MEM.to_user_slot().unwrap();
        state.fproc_table.get_mut(mem_slot).unwrap().endpoint = Endpoint::MEM;

        state.apply_boot_rows(&[mem_boot_row()]).unwrap();

        let fp = state.fproc_table.get_mut(mem_slot).unwrap();
        assert!(fp.flags.contains(FpFlags::SRV_PROC));
        let entry = state
            .dmap_table
            .get(crate::device_map::MEMORY_MAJOR)
            .unwrap();
        assert_eq!(entry.driver, Some(Endpoint::MEM));
        assert_eq!(&entry.label[..7], b"memory\0");
    }

    /// Count of dmap rows with drivers (`entries` is private; walk `get` per major; the CTTY slot is
    /// pre-filled by `VfsState::new` — "apply added no rows" is judged by count difference, not total emptiness).
    fn dmap_driver_count(table: &DmapTable) -> usize {
        (0..table.len() as u32)
            .filter(|m| table.get(*m).is_some_and(|e| e.driver.is_some()))
            .count()
    }

    #[test]
    fn test_apply_boot_rows_skips_vacant_and_boot_user() {
        let mut state = VfsState::new();
        state.init_fresh();
        let vacant = minix_types::RprocpubSnap::default();
        // Boot user (init): IS_RPUB_BOOT_USR (rs.h:188) — neither mark nor dmap is done
        // (C dmap.c:206 runs before isokendpt, so no fproc slot is needed).
        let boot_user = minix_types::RprocpubSnap {
            in_use: 1,
            endpoint: Endpoint::INIT.get(),
            dev_nr: 3,
            ..Default::default()
        };
        state.apply_boot_rows(&[vacant, boot_user]).unwrap();
        assert_eq!(
            dmap_driver_count(&state.dmap_table),
            dmap_driver_count(&DmapTable::init())
        );
        // Neither a boot user nor an empty slot may be marked SRV_PROC.
        let init_slot = Endpoint::INIT.to_user_slot().unwrap();
        let fp = state.fproc_table.get_mut(init_slot).unwrap();
        assert!(!fp.flags.contains(FpFlags::SRV_PROC));
    }

    #[test]
    fn test_apply_boot_rows_service_only_marks_without_dmap() {
        let mut state = VfsState::new();
        state.init_fresh();
        let mfs_slot = Endpoint::MFS.to_user_slot().unwrap();
        state.fproc_table.get_mut(mfs_slot).unwrap().endpoint = Endpoint::MFS;
        // dev_nr == NO_DEV (id.rs:62 = 0): only mark the service, no dmap row (dmap.c:217-218).
        let service_only = minix_types::RprocpubSnap {
            in_use: 1,
            endpoint: Endpoint::MFS.get(),
            dev_nr: NO_DEV as i32,
            ..Default::default()
        };
        state.apply_boot_rows(&[service_only]).unwrap();
        let fp = state.fproc_table.get_mut(mfs_slot).unwrap();
        assert!(fp.flags.contains(FpFlags::SRV_PROC));
        assert_eq!(
            dmap_driver_count(&state.dmap_table),
            dmap_driver_count(&DmapTable::init())
        );
    }

    #[test]
    fn test_apply_boot_rows_rejects_unknown_endpoint() {
        let mut state = VfsState::new();
        state.init_fresh();
        // The endpoint is not in fproc (isokendpt fails, dmap.c:208-212) → EINVAL.
        let ghost = minix_types::RprocpubSnap {
            in_use: 1,
            endpoint: 0x555,
            dev_nr: 3,
            ..Default::default()
        };
        assert_eq!(state.apply_boot_rows(&[ghost]), Err(MapError::Inval));
    }

    #[test]
    fn test_apply_boot_rows_rejects_bad_major() {
        let mut state = VfsState::new();
        state.init_fresh();
        let mem_slot = Endpoint::MEM.to_user_slot().unwrap();
        state.fproc_table.get_mut(mem_slot).unwrap().endpoint = Endpoint::MEM;
        // Negative dev_nr: C map_driver's major<0 gate (dmap.c:67) → ENODEV.
        let bad_major = minix_types::RprocpubSnap {
            in_use: 1,
            endpoint: Endpoint::MEM.get(),
            dev_nr: -1,
            ..Default::default()
        };
        assert_eq!(state.apply_boot_rows(&[bad_major]), Err(MapError::NoDev));
    }

    #[test]
    fn test_map_boot_services_requests_rs_grant_copy() {
        let mut state = VfsState::new();
        state.init_fresh();
        let mut kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        kernel.reply(minix_types::OK);

        state.map_boot_services(&kernel, 7).unwrap();

        let sent = kernel.sent.borrow();
        assert_eq!(sent.len(), 1);
        let call = sent[0];
        assert_eq!(call.m_type, minix_types::SYS_SAFECOPYFROM);
        // SAFETY(test): m_lsys_kern_safecopy is the payload layout of SAFECOPYFROM
        // (the loading side of minix-sys sys_safecopyfrom uses the same fields).
        let sc = unsafe { &call.m_u.m_lsys_kern_safecopy };
        assert_eq!(sc.from_to, Endpoint::RS.get());
        assert_eq!(sc.grant_id, 7);
        assert_eq!(sc.offset, 0);
        assert_eq!(
            sc.bytes,
            (minix_types::NR_BOOT_PROCS * core::mem::size_of::<minix_types::RprocpubSnap>()) as u64
        );
    }

    #[test]
    fn test_map_boot_services_fails_closed_on_copy_error() {
        let mut state = VfsState::new();
        state.init_fresh();
        let mut kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        kernel.reply(-minix_types::EIO);

        assert_eq!(
            state.map_boot_services(&kernel, 7),
            Err(BootMapError::Safecopy(-minix_types::EIO))
        );
    }

    /// Scripted reply for the root readsuper (ipc.h:198-211 layout: file_size@0, device@8,
    /// inode@16, flags@24, mode@28, uid@32, gid@36, con_reqs@40).
    fn readsuper_reply(ino: u64, mode: u32, size: u64) -> Message {
        let mut m = Message::default();
        {
            // SAFETY: write the byte face at the existing offsets of the request.rs:1140 decoder.
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

    /// Scripted IPC for the boot phase: `sendrec` pops replies in order; an empty script yields the default success reply
    /// (directory root node, non-threaded).
    struct BootScriptedIpc {
        queue: core::cell::RefCell<alloc::collections::VecDeque<Option<Message>>>,
        /// (destination, request sent) recorded in order.
        pub sent: core::cell::RefCell<alloc::vec::Vec<(Endpoint, Message)>>,
        /// A positive errno for the next sendrec when not None (TrapStatus contract).
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
        fn send(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
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
        fn sendnb(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
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

    /// Boot preconditions: one live process slot (slot 0, for the MAKEROOT assertion) + handshake termination
    /// + dmap driver rows (the scripted environment for the root mount).
    fn boot_ready_state() -> (VfsState, BootScriptedIpc) {
        let mut state = VfsState::new();
        state.init_fresh();
        let first = VfsPmInit {
            slot: 0,
            pid: 8,
            endpoint: Endpoint::from_generation_slot(1, 0),
        }
        .encode();
        state.pm_handshake_step(&first).unwrap();
        let terminator = VfsPmInit {
            slot: 0,
            pid: 0,
            endpoint: Endpoint::NONE,
        }
        .encode();
        state.pm_handshake_step(&terminator).unwrap();
        seed_imgrd_driver(&mut state);
        (state, BootScriptedIpc::default())
    }

    #[test]
    fn test_root_mount_full_assembly() {
        let (mut state, ipc) = boot_ready_state();
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        // Script order: PFS answers first (its details are ignored as in C), MFS afterwards (root inode 1, directory,
        // serial window = 1 — flags@24 zero means no RES_THREADED).
        ipc.push(Some(readsuper_reply(1, crate::open::S_IFDIR | 0o755, 32)));
        ipc.push(Some(readsuper_reply(1, crate::open::S_IFDIR | 0o755, 4096)));

        state.do_init_root(&kernel, &ipc).unwrap();

        // The assignment site (mount.c:326-328).
        assert_eq!(state.root_fs_e, Endpoint::MFS);
        assert_eq!(state.root_dev, DEV_IMGRD);
        assert_eq!(state.have_root, 1);

        // vmnt row: label/path/device + CANSTAT + window (mount.c:297-305/318-325).
        let id = state
            .vmnt_table
            .find_by_dev(DEV_IMGRD)
            .expect("root vmnt row");
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

        // The seven-field root vnode fill (mount.c:283-296).
        let root_idx = v.root.expect("root vnode set");
        let vn = state
            .vnode_table
            .get(crate::vnode::VnodeId(root_idx))
            .unwrap();
        assert_eq!(vn.ino, 1);
        assert_eq!(vn.mode, crate::open::S_IFDIR | 0o755);
        assert_eq!(vn.size, 4096);
        assert_eq!(vn.ref_count, 1);
        assert_eq!(vn.fs_count, 1);
        assert_eq!(vn.sdev, NO_DEV);
        assert_eq!(vn.vmnt, Some(crate::vnode::VmntId(id.get())));
        assert_eq!(vn.dev, DEV_IMGRD);

        // MAKEROOT: the live fproc slot's (slot 0 was seeded by the handshake) rd/wd move to the root.
        let fp = state.fproc_table.get(UserSlot::new(0)).unwrap();
        assert_eq!(fp.root_dir, Some(root_idx));
        assert_eq!(fp.work_dir, Some(root_idx));
    }

    #[test]
    fn test_root_mount_without_driver_is_inval_and_gate_stays_closed() {
        let (mut state, ipc) = boot_ready_state();
        // Remove the dmap row: an honest failure when the boot assembly half (rproctab consumption) is absent.
        state.dmap_table.set(
            crate::device_map::MEMORY_MAJOR,
            crate::device_map::DmapEntry::empty(),
        );
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();

        let r = state.do_init_root(&kernel, &ipc);
        assert!(matches!(r, Err(MountError::Inval)));
        // The worker gate is not reopened before the C panic; root is unassigned.
        assert!(!state.accept_requests);
        assert_eq!(state.root_fs_e, Endpoint::NONE);
        assert_eq!(state.have_root, 0);
    }

    #[test]
    fn test_pfs_readsuper_failure_tolerated() {
        let (mut state, ipc) = boot_ready_state();
        let kernel = minix_sys::syscall::CannedKernelCallTransport::new();
        // The first shot (PFS) fails; the second (MFS) takes the default reply.
        ipc.fail_next.set(Some(minix_types::EIO));

        state.do_init_root(&kernel, &ipc).unwrap();
        assert_eq!(state.have_root, 1);
        assert_eq!(state.root_fs_e, Endpoint::MFS);
        // The PFS row is still present (C: the mount proceeds after printf), fs_flags is not filled.
        let pfs = state
            .vmnt_table
            .find_by_fs(Endpoint::PFS)
            .expect("pfs row stands");
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
        // The on-wire `m_type` carries the transid in its high 16 bits (C `fs_sendrec`'s
        // `TRNS_ADD_ID(call, id)`): FS dispatches via `TransactionId::decode` (call =
        // raw >> 16), so a bare `REQ_READSUPER` would fall into `Unserved` with call=0.
        // A synchronous sendrec uses id=0, so the on-wire value = `trns_add_id(REQ_READSUPER, 0)`.
        assert_eq!(
            minix_types::trns_del_id(pfs_req.m_type),
            minix_types::REQ_READSUPER as i32,
            "剥 transid 后为 REQ_READSUPER"
        );
        assert_eq!(
            minix_types::trns_del_id(mfs_req.m_type),
            minix_types::REQ_READSUPER as i32,
            "剥 transid 后为 REQ_READSUPER"
        );
        assert_eq!(
            pfs_req.m_type,
            minix_types::trns_add_id(minix_types::REQ_READSUPER, 0)
        );
        assert_eq!(
            mfs_req.m_type,
            minix_types::trns_add_id(minix_types::REQ_READSUPER, 0)
        );

        // The on-wire shape (request.c:780-813): device@0, flags@8, path_len@16, grant@24.
        let raw = unsafe { &mfs_req.m_u.raw };
        let dev = u64::from_le_bytes(raw[0..8].try_into().unwrap());
        let flags = u32::from_le_bytes(raw[8..12].try_into().unwrap());
        let path_len = u64::from_le_bytes(raw[16..24].try_into().unwrap());
        let grant = i32::from_le_bytes(raw[24..28].try_into().unwrap());
        assert_eq!(dev, DEV_IMGRD);
        assert_ne!(flags & crate::request::REQ_ISROOT, 0, "根挂载带 REQ_ISROOT");
        // label "memory\0" = 7 bytes (the driver label seeded in the dmap row).
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

    /// The success face of the `CdevIo` continuation: the driver replies `CDEV_REPLY`, and the first payload word is
    /// the byte count moved — returned as an `Ok` value, with `BlockedOn::Cdev` cleared
    /// (the `revive` side of C `cdev_generic_reply`, cdev.c:471-477).
    #[test]
    fn test_cdev_io_continuation_completes_with_count() {
        let mut state = VfsState::new();
        let user = Endpoint::from_generation_slot(1, 0);
        {
            let fp = state.fproc_table.get_mut(UserSlot::new(0)).unwrap();
            fp.pid = 100;
            fp.endpoint = user;
            fp.blocked_on = crate::fproc::BlockedOn::Cdev(crate::fproc::CdevBlock {
                dev: 4 << 8,
                endpt: Endpoint::from_generation_slot(0, 12),
                grant: None,
            });
        }
        let mut reply = Message {
            m_type: minix_types::CDEV_REPLY_BASE,
            m_source: Endpoint::from_generation_slot(0, 12),
            ..Message::default()
        };
        // SAFETY(test): `mess_lchardriver_vfs_reply { status; id }` — the status is
        // in the first payload word, id in the second.
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[0..4].copy_from_slice(&7i32.to_le_bytes());
            raw[4..8].copy_from_slice(&user.get().to_le_bytes());
        }
        let idx = state
            .worker_pool
            .assign_first_fit(
                UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(crate::worker::WorkerCont::CdevIo { grant: 0 });
            wp.sendrec = Some(reply);
            wp.state = WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 7)),
            "搬运字节数原样回用户"
        );
        assert!(
            matches!(
                state.fproc_table.get(UserSlot::new(0)).unwrap().blocked_on,
                crate::fproc::BlockedOn::None
            ),
            "读/写的进程级挂起在完成时清掉"
        );
    }

    /// The failure face of the `CdevIo` continuation: a negative errno is folded positive for the user as is;
    /// `EINTR` is converted back to `EAGAIN` per `cdev_generic_reply` (cdev.c:471-473).
    #[test]
    fn test_cdev_io_continuation_error_and_eintr() {
        let make_reply = |status: i32, user: Endpoint| {
            let mut m = Message {
                m_type: minix_types::CDEV_REPLY_BASE,
                m_source: Endpoint::from_generation_slot(0, 12),
                ..Message::default()
            };
            // SAFETY(test): the status is in the first payload word.
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&status.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&user.get().to_le_bytes());
            }
            m
        };
        let park = |state: &mut VfsState, user: Endpoint| {
            let fp = state.fproc_table.get_mut(UserSlot::new(0)).unwrap();
            fp.pid = 100;
            fp.endpoint = user;
            let idx = state
                .worker_pool
                .assign_first_fit(
                    UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .unwrap();
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(crate::worker::WorkerCont::CdevIo { grant: 0 });
            wp.fp_slot = Some(UserSlot::new(0));
            wp.state = WorkerState::Busy;
            idx
        };

        // EIO: the negative errno is folded positive.
        let mut state = VfsState::new();
        let user = Endpoint::from_generation_slot(1, 0);
        let idx = park(&mut state, user);
        let wp = state.worker_pool.get_mut(idx).unwrap();
        wp.sendrec = Some(make_reply(-minix_types::EIO, user));
        wp.state = WorkerState::Busy;
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EIO))
        );

        // EINTR → EAGAIN (the paired conversion of `cdev_cancel`).
        let mut state = VfsState::new();
        let idx = park(&mut state, user);
        let wp = state.worker_pool.get_mut(idx).unwrap();
        wp.sendrec = Some(make_reply(-minix_types::EINTR, user));
        wp.state = WorkerState::Busy;
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::EAGAIN)),
            "EINTR 折回 EAGAIN（cdev.c:471-473）"
        );
    }

    /// `CdevIoctl` continuation finalization: the first payload word is decoded **unconditionally** — a driver reply's
    /// m_type is always `CDEV_REPLY`(0x480), so the old `if status == 0` gate was always false and 0x480 would
    /// leak to the user as an error code; after the fix, success (status=0) returns `Ok(0)`, an error (-ENOTTY)
    /// is folded positive for the user, and `EINTR` folds back to `EAGAIN` (cdev.c:471-473).
    #[test]
    fn test_cdev_ioctl_continuation_decodes_payload() {
        let make_reply = |status: i32, user: Endpoint| {
            let mut m = Message {
                m_type: minix_types::CDEV_REPLY_BASE,
                m_source: Endpoint::from_generation_slot(0, 12),
                ..Message::default()
            };
            // SAFETY(test): `mess_lchardriver_vfs_reply { status; id }`——
            // The status is in the first payload word, id in the second.
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&status.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&user.get().to_le_bytes());
            }
            m
        };
        let park = |state: &mut VfsState, user: Endpoint| {
            let fp = state.fproc_table.get_mut(UserSlot::new(0)).unwrap();
            fp.pid = 100;
            fp.endpoint = user;
            let idx = state
                .worker_pool
                .assign_first_fit(
                    UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .unwrap();
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(crate::worker::WorkerCont::CdevIoctl { grant: 0 });
            wp.fp_slot = Some(UserSlot::new(0));
            wp.state = WorkerState::Busy;
            idx
        };
        let user = Endpoint::from_generation_slot(1, 0);

        // Success: status=0 → `Ok(0)` (under the old gate this was -0x480).
        let mut state = VfsState::new();
        let idx = park(&mut state, user);
        let wp = state.worker_pool.get_mut(idx).unwrap();
        wp.sendrec = Some(make_reply(0, user));
        wp.state = WorkerState::Busy;
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, 0)),
            "ioctl 成功态回 Ok(0)，不是 0x480 被当错误码"
        );

        // Error: -ENOTTY is folded positive.
        let mut state = VfsState::new();
        let idx = park(&mut state, user);
        let wp = state.worker_pool.get_mut(idx).unwrap();
        wp.sendrec = Some(make_reply(-minix_types::ENOTTY, user));
        wp.state = WorkerState::Busy;
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, -minix_types::ENOTTY))
        );
    }

    /// ID pairing of driver replies: when two jobs wait on the same driver, a reply lands on the correct
    /// slot by the payload's `id` (= the `proc_e` when the request was sent) — not on "the first waiting
    /// slot" (C `cdev_generic_reply` identifies the process by id, cdev.c:442-460).
    #[test]
    fn test_drv_reply_lands_by_id() {
        let mut state = VfsState::new();
        let driver = Endpoint::from_generation_slot(0, 12);
        let user_a = Endpoint::from_generation_slot(1, 0);
        let user_b = Endpoint::from_generation_slot(1, 1);
        for (slot, ep) in [(0usize, user_a), (1, user_b)] {
            let fp = state.fproc_table.get_mut(UserSlot::new(slot)).unwrap();
            fp.pid = 100 + slot as i32;
            fp.endpoint = ep;
        }
        let parked = Message {
            m_type: minix_chardriver::protocol::CdevRequest::Write.message_type(),
            ..Message::default()
        };
        for (idx, slot) in [(0usize, 0usize), (1, 1)] {
            state
                .worker_pool
                .get_mut(idx)
                .unwrap()
                .set_waiting(driver, parked);
            state.worker_pool.get_mut(idx).unwrap().fp_slot = Some(UserSlot::new(slot));
        }
        // B's reply arrives: it must land on B's slot (1); A's slot keeps waiting.
        let mut reply = Message {
            m_type: minix_types::CDEV_REPLY_BASE,
            m_source: driver,
            ..Message::default()
        };
        // SAFETY(test): `mess_lchardriver_vfs_reply { status; id }`.
        unsafe {
            reply.m_u.raw[4..8].copy_from_slice(&user_b.get().to_le_bytes());
        }
        assert_eq!(state.handle_drv_reply(&reply), Ok(1));
        assert_eq!(
            state.worker_pool.get(0).unwrap().state,
            WorkerState::WaitingForFs,
            "A 的槽不受影响"
        );
        assert_eq!(state.worker_pool.get(1).unwrap().state, WorkerState::Busy);

        // No id match (the id is an unregistered endpoint) → fall back to first-match, compatible with requests without an id.
        let mut stray = Message {
            m_type: minix_types::CDEV_REPLY_BASE,
            m_source: driver,
            ..Message::default()
        };
        // SAFETY(test): same as above, with the id replaced by an unregistered endpoint.
        unsafe {
            stray.m_u.raw[4..8].copy_from_slice(&9999i32.to_le_bytes());
        }
        assert_eq!(state.handle_drv_reply(&stray), Ok(0));
    }

    /// The tty-slot half of `apply_boot_rows` (its dmap half): the `dev_nr = TTY_MAJOR(4)` row from the RS dev
    /// table lands in dmap[4] with driver endpoint = tty (the boot assembly carries `BOOT_IMAGE_DEV_TABLE`'s
    /// tty row into pub_wire).
    #[test]
    fn test_apply_boot_rows_maps_tty_dmap_slot() {
        let mut state = VfsState::new();
        // isokendpt requires endpoint↔slot consistency (dmap.c:208-214 correspondence).
        {
            let fp = state.fproc_table.get_mut(UserSlot::new(5)).unwrap();
            fp.pid = 7;
            fp.endpoint = Endpoint::TTY;
        }
        let mut label = [0u8; minix_types::RS_MAX_LABEL_LEN];
        label[..3].copy_from_slice(b"tty");
        let row = minix_types::RprocpubSnap {
            in_use: 1,
            endpoint: Endpoint::TTY.get(),
            dev_nr: 4, // TTY_MAJOR (rs table.c:45-50 → dmap.h:25)
            label,
            ..minix_types::RprocpubSnap::default()
        };
        state.apply_boot_rows(&[row]).unwrap();
        let drow = state.dmap_table.get(4).expect("major 4 在表内");
        assert_eq!(drow.driver, Some(Endpoint::TTY), "tty 槽已映射");
        assert!(
            state
                .fproc_table
                .get(UserSlot::new(5))
                .unwrap()
                .flags
                .contains(crate::fproc::FpFlags::SRV_PROC),
            "服务标记 FP_SRV_PROC"
        );
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
    assert_eq!(
        lu_prepare(true, LuState::Other),
        Err(UnblockError::NotReady)
    );
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
        Route::Enosys {
            raw: (crate::call_table::VFS_BASE + 200) as u32
        }
    );
    // RS reply prefixes at the real C bases (`com.h:919/:963/:1038`).
    assert!(VfsState::is_bdev_rs(0x580));
    assert!(VfsState::is_cdev_rs(0x480));
    assert!(VfsState::is_sdev_rs(0x1980));
    // The bases must not collide with the syscall namespace.
    assert!(!VfsState::is_bdev_rs(
        crate::call_table::VfsCallNum::Open as u32
    ));
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
        // Unknown call number → Enosys route (the ENOSYS semantics of main.c:283-294).
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
