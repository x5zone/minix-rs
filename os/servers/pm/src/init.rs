//! The PM startup chain: SEF callback registration plus a `sef_cb_init_fresh`
//! equivalent.
//!
//! C counterpart: `minix3/minix/servers/pm/main.c:131-244` (main /
//! sef_local_startup / sef_cb_init_fresh) plus
//! `minix3/minix/servers/pm/schedule.c:36-69` (the sched_init call site).
//! Documents: `rewrite-notes/04-stage-pm/01-pm-init-main.md`.
//!
//! The main loop (run) is detailed in 04-ipc-dispatch.md; the VFS asynchronous
//! reply state machine in 05-vfs-interaction.md; the scheduling protocol in
//! 16-scheduling.md.
//!
//! # Single-thread model
//!
//! PM is a userland server (a single-threaded event loop). All state in this
//! module is held exclusively by `PmServer`, with no cross-thread sharing;
//! `Cell`/`&mut` are safe under one thread.

use crate::event::EventRegistry;
use crate::ipc::{
    IpcTransport, KernelIpcTransport, PmCall, PmServices, ReplyIntent, dispatch_message,
    handle_vfs_reply, is_vfs_pm_rs,
};
use crate::mproc::{Guardianship, INIT_PID, Lifecycle, Privilege, ProcTable, SigSet};
use minix_types::{Endpoint, Message, NR_BOOT_PROCS, NR_PROCS, UserSlot};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

// ── Constants (C: minix3/minix/servers/pm + include) ──

/// C: `MULTIBOOT_PARAM_BUF_SIZE` — include/arch/earm/include/multiboot.h:240。
pub const MULTIBOOT_PARAM_BUF_SIZE: usize = 1024;

/// C: `VFS_PM_RQ_BASE` / `VFS_PM_INIT` — include/minix/com.h:513/520。
///
/// The protocol constants and the `VfsPmInit` codec have moved up to
/// minix-types (the single source of truth shared by PM and VFS).
pub use minix_types::{VFS_PM_INIT, VFS_PM_RQ_BASE, VfsPmInit};

/// C: `VFS_PROC_NR` — include/minix/com.h:61。
pub const VFS_PROC_NR: i32 = 1;
/// C: `RS_PROC_NR` — include/minix/com.h:62。
pub const RS_PROC_NR: i32 = 2;
/// C: `INIT_PROC_NR` — include/minix/com.h:72（= LAST_SPECIAL_PROC_NR）。
pub const INIT_PROC_NR: i32 = 11;

/// C: `NR_SCHED_QUEUES` — include/minix/config.h:66。
pub const NR_SCHED_QUEUES: i32 = 16;
/// C: `MAX_USER_Q` — include/minix/config.h:68。
pub const MAX_USER_Q: i32 = 0;
/// C: `MIN_USER_Q` — include/minix/config.h:71（= NR_SCHED_QUEUES - 1）。
pub const MIN_USER_Q: i32 = NR_SCHED_QUEUES - 1;

/// C: `USER_QUANTUM` — include/minix/config.h:74 (the user-process time slice).
pub const USER_QUANTUM: i32 = 200;
/// C: `USER_Q` — include/minix/config.h:69 (the default user queue).
pub const USER_Q: i32 = (MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q;
/// C: `USR_Q` — include/minix/priv.h:95 (user process, = USER_Q).
pub const USR_Q: i32 = USER_Q;
/// C: `SRV_Q` — include/minix/priv.h:93 (system service, = USER_Q).
pub const SRV_Q: i32 = USER_Q;

/// C: `PRIO_MIN` / `PRIO_MAX` — sys/sys/resource.h:43-44 (provided by libc).
pub const PRIO_MIN: i32 = -20;
pub const PRIO_MAX: i32 = 20;

// ── Signal numbers (C: sys/sys/signal.h) ──
//
// Only used by this module to build the compile-time signal sets; the full
// signal semantics are in 11-signal-core.md / 12-signal-handlers.md.

/// C: `SIGQUIT` — signal.h:48。
const SIGQUIT: i32 = 3;
/// C: `SIGILL` — signal.h:49。
const SIGILL: i32 = 4;
/// C: `SIGTRAP` — signal.h:50。
const SIGTRAP: i32 = 5;
/// C: `SIGABRT` — signal.h:51。
const SIGABRT: i32 = 6;
/// C: `SIGEMT` — signal.h:52。
const SIGEMT: i32 = 7;
/// C: `SIGFPE` — signal.h:53。
const SIGFPE: i32 = 8;
/// C: `SIGBUS` — signal.h:56。
const SIGBUS: i32 = 10;
/// C: `SIGSEGV` — signal.h:57。
const SIGSEGV: i32 = 11;
/// C: `SIGCONT` — signal.h:63。
const SIGCONT: i32 = 19;
/// C: `SIGCHLD` — signal.h:64。
const SIGCHLD: i32 = 20;
/// C: `SIGWINCH` — signal.h:72。
const SIGWINCH: i32 = 28;
/// C: `SIGINFO` — signal.h:73。
const SIGINFO: i32 = 29;

/// Build the bit for a single signal. The bit base matches C
/// `__sigmask(n) = 1 << ((n-1) & 31)` (`sys/sys/sigtypes.h:67`, the semantics of
/// `sigaddset`/`sigismember`): bit i corresponds to signal i+1, so this crate's
/// `SigSet` value is bit-for-bit comparable with C's `sigset_t`. The bit base may
/// appear only in this function — callers always pass a signal number.
pub(crate) const fn sig_bit(sig: i32) -> SigSet {
    1u64 << (sig - 1)
}

/// The set of signals that cause a core dump.
///
/// C: `core_sigs[]` — main.c:137-138：SIGQUIT/SIGILL/SIGTRAP/SIGABRT/SIGEMT/
/// SIGFPE/SIGBUS/SIGSEGV。
#[allow(dead_code)] // consumer: 11-signal-core.md (signal.rs sig_proc_exit).
pub(crate) const CORE_SIGSET: SigSet = sig_bit(SIGQUIT)
    | sig_bit(SIGILL)
    | sig_bit(SIGTRAP)
    | sig_bit(SIGABRT)
    | sig_bit(SIGEMT)
    | sig_bit(SIGFPE)
    | sig_bit(SIGBUS)
    | sig_bit(SIGSEGV);

/// The set of signals ignored by default.
///
/// C: `ign_sigs[]` — main.c:139：SIGCHLD/SIGWINCH/SIGCONT/SIGINFO。
#[allow(dead_code)] // consumer: 11-signal-core.md (signal.rs check_sig).
pub(crate) const IGN_SIGSET: SigSet =
    sig_bit(SIGCHLD) | sig_bit(SIGWINCH) | sig_bit(SIGCONT) | sig_bit(SIGINFO);

/// The set of signals that cannot be ignored (the default action is forced even
/// if ignoring was requested).
///
/// C: `noign_sigs[]` — main.c:140-141：SIGILL/SIGTRAP/SIGEMT/SIGFPE/SIGBUS/SIGSEGV。
#[allow(dead_code)] // consumer: 11-signal-core.md (signal.rs check_sig).
pub(crate) const NOIGN_SIGSET: SigSet = sig_bit(SIGILL)
    | sig_bit(SIGTRAP)
    | sig_bit(SIGEMT)
    | sig_bit(SIGFPE)
    | sig_bit(SIGBUS)
    | sig_bit(SIGSEGV);

// ── Boot parameters ──

/// PM boot parameters.
///
/// In C, three kernel IPCs — `sys_getmonparams` (main.c:167-170),
/// `sys_getimage` (main.c:172-176) and `sys_hz` (main.c:238) — fill global or
/// static buffers; the Rust side aggregates them explicitly into a boot contract
/// (the same shape as VM's `BootParams`).
#[derive(Debug, Clone)]
pub struct BootParams {
    /// The boot monitor parameter buffer.
    ///
    /// C: `monitor_params` (glo.h:10, `MULTIBOOT_PARAM_BUF_SIZE` bytes). Its
    /// semantics (find_param) are in 20-misc-queries.md.
    pub monitor_params: [u8; MULTIBOOT_PARAM_BUF_SIZE],
    /// The kernel boot image table (defined in kernel/table.c, copied by
    /// sys_getimage).
    ///
    /// C: `static struct boot_image image[NR_BOOT_PROCS]` — main.c:135。
    pub boot_image: [minix_types::BootImage; NR_BOOT_PROCS],
    /// The system clock frequency (HZ).
    ///
    /// C: `system_hz = sys_hz()` — main.c:238; timer semantics are in
    /// 14-itimer.md.
    pub system_hz: u32,
    /// The VFS process endpoint (VFS_PROC_NR).
    pub vfs_endpoint: Endpoint,
}

impl BootParams {
    /// Placeholder parameters (values for tests and development before the kernel
    /// IPCs land).
    ///
    /// The boot image is all empty entries (`endpoint == NONE`, so
    /// `fill_boot_procs` skips every one — placeholder parameters produce no
    /// process at all). The production main replaces them with real kernel answers
    /// through [`BootParams::acquire_from`].
    pub fn placeholder() -> Self {
        Self {
            monitor_params: [0; MULTIBOOT_PARAM_BUF_SIZE],
            boot_image: [minix_types::BootImage::empty(); NR_BOOT_PROCS],
            system_hz: 100,
            vfs_endpoint: Endpoint::VFS,
        }
    }

    /// Acquire the real boot values.
    ///
    /// The three kernel IPCs of C main.c:167-238 — `sys_getmonparams` (:167-170),
    /// `sys_getimage` (:172-176) and `sys_hz` (:238) — are fetched through the
    /// minix-sys wrappers and decoded per the wire authority: monitor params at
    /// the full `MULTIBOOT_PARAM_BUF_SIZE` length (kernel do_getinfo.c:143-146's
    /// `length = sizeof(kinfo.param_buf)`), the image as
    /// `NR_BOOT_PROCS × BootImageStruct` (do_getinfo.c:86-90). Any kernel call
    /// failure folds into `Err(the original errno)` — the C counterpart panics
    /// ("get monitor params failed"/"couldn't get image table"), and the
    /// production main stops on it rather than letting half-true data into the
    /// boot contract.
    pub fn acquire_from(
        transport: &impl minix_sys::syscall::KernelCallTransport,
    ) -> Result<Self, i32> {
        use minix_sys::syscall::{sys_get_hz, sys_getimage, sys_getmonparams};

        let mut monitor_params = [0u8; MULTIBOOT_PARAM_BUF_SIZE];
        sys_getmonparams(transport, &mut monitor_params)?;

        let mut wire = [minix_types::BootImageStruct::default(); NR_BOOT_PROCS];
        // SAFETY: `[BootImageStruct; N]` is a repr(C) POD array (the layout is
        // pinned by the freeze test in minix-types boot_image.rs), so the whole
        // block is byte-writable; the kernel GET_IMAGE fills it in the same
        // layout.
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(
                wire.as_mut_ptr() as *mut u8,
                core::mem::size_of::<[minix_types::BootImageStruct; NR_BOOT_PROCS]>(),
            )
        };
        sys_getimage(transport, bytes)?;

        let mut boot_image = [minix_types::BootImage::empty(); NR_BOOT_PROCS];
        for (dst, src) in boot_image.iter_mut().zip(wire.iter()) {
            *dst = minix_types::BootImage::from_wire(src);
        }

        let system_hz = sys_get_hz(transport)? as u32;
        Ok(Self {
            monitor_params,
            boot_image,
            system_hz,
            // The sync with VFS always goes to the VFS_PROC_NR constant
            // (main.c:229/:234), not placeholder data.
            vfs_endpoint: Endpoint::VFS,
        })
    }
}

// ── The PM server ──

/// The PM server: process table + boot parameters + IPC transport.
///
/// C: the globals `mproc[]` + `monitor_params` + `system_hz` (glo.h).
/// Rust: explicitly aggregated into one struct (globals become
/// PmContext/PmServer).
///
/// `T` is the IPC transport implementation (production
/// `KernelIpcTransport`, mocks in tests).
pub struct PmServer<T: IpcTransport = KernelIpcTransport> {
    /// The PM process table (mproc).
    table: ProcTable,
    /// The process event registry (`event.c:60-67`'s aggregate of
    /// `subs/nsubs/nested`, ARCH: A-3).
    ///
    /// Document: `rewrite-notes/04-stage-pm/06-event-subscription.md`.
    event_registry: EventRegistry,
    /// The kernel-call exit (SYS_CLEAR etc.; honest `-EIO` before the E1
    /// power-on).
    kern: Box<dyn crate::exit::KernelGateway>,
    /// Boot parameters.
    params: BootParams,
    /// The IPC transport (VFS_PM_INIT sync etc.).
    transport: T,
    /// The kernel timer exit (CLOCK notify → `expire_timers`, 14-itimer.md;
    /// production `SysTimerCtl` keeps local bookkeeping plus the kernel clock,
    /// tests inject scripted implementations).
    timer: Box<dyn crate::timer::TimerCtl>,
    /// The virtual timer exit (SIGVTALRM/SIGPROF re-arming, 14-itimer.md; the
    /// SIGKSIG pull is consumed through process_ksig; production `SysVTimerCtl`
    /// delegates to sys_vtimer).
    vtimer: Box<dyn crate::timer::VTimerCtl>,
    /// SVRCTL parameter storage (C: the `local_param_overrides` static table plus
    /// the monitor string, misc.c:305-310).
    svrctl_store: crate::misc::ParamStore,
    /// Call counters (C: `ENABLE_SYSCALL_STATS`'s counters[], misc.c:64; counted
    /// per call number and gated by a feature). This byte face is the SI_CALL_STATS
    /// reply.
    call_stats: alloc::vec::Vec<u8>,
    /// Whether `init()` has completed (the precondition asserted by run()).
    initialized: bool,
    /// The kernel abort flag (C: `glo.h:26` `abort_flag`; written by do_reboot,
    /// documented in 20-misc-queries.md).
    ///
    /// Read only in the `VFS_PM_REBOOT_REPLY` special case, through
    /// [`crate::ipc::vfs::PmServices`], and passed to `sys_abort` (see the PM
    /// interaction design notes).
    abort_flag: i32,
}

impl PmServer<KernelIpcTransport> {
    /// Create a server with the production IPC transport (empty process table plus
    /// the given boot parameters).
    pub fn new(params: BootParams) -> Self {
        Self::with_transport(params, KernelIpcTransport::new())
    }
}

impl<T: IpcTransport> PmServer<T> {
    /// Create a server with an explicit transport (tests inject a mock).
    ///
    /// The kernel gateway defaults to the production implementation (trap
    /// channel, `-EIO` before E1); tests can inject a scripted gateway through
    /// [`Self::with_kernel_gateway`].
    pub fn with_transport(params: BootParams, transport: T) -> Self {
        Self::with_kernel_gateway(
            params,
            transport,
            Box::new(crate::exit::TrapKernelGateway::new(
                minix_sys::syscall::DirectKernelCallTransport,
            )),
        )
    }

    /// Construct with an explicit kernel gateway (tests inject a scripted
    /// gateway; the real power-on hangs off edge E1).
    pub fn with_kernel_gateway(
        params: BootParams,
        transport: T,
        kern: Box<dyn crate::exit::KernelGateway>,
    ) -> Self {
        Self::with_timer_ctl(
            params,
            transport,
            kern,
            Box::new(crate::timer::SysTimerCtl::new()),
            Box::new(crate::timer::SysVTimerCtl),
        )
    }

    /// Construct with an explicit kernel gateway and timer exits (tests inject
    /// scripted implementations; the real power-on hangs off edges E1/E6).
    #[allow(clippy::too_many_arguments)]
    pub fn with_timer_ctl(
        params: BootParams,
        transport: T,
        kern: Box<dyn crate::exit::KernelGateway>,
        timer: Box<dyn crate::timer::TimerCtl>,
        vtimer: Box<dyn crate::timer::VTimerCtl>,
    ) -> Self {
        Self {
            // C: the first step (main.c:146-152) initializes the mproc table —
            // ProcTable::new() guarantees empty slots (Lifecycle::Unused) and a
            // ready PID generator.
            call_stats: alloc::vec![0u8; 48 * 4],
            svrctl_store: crate::misc::ParamStore::new(
                String::from_utf8_lossy(
                    &params.monitor_params
                        .iter()
                        .copied()
                        .take_while(|&b| b != 0)
                        .collect::<alloc::vec::Vec<u8>>(),
                )
                .into_owned(),
            ),
            table: ProcTable::new(),
            event_registry: EventRegistry::new(),
            params,
            transport,
            kern,
            timer,
            vtimer,
            initialized: false,
            // The kernel abort flag starts at 0 (no abort); do_reboot writes it
            // (20-misc-queries.md).
            abort_flag: 0,
        }
    }

    /// The process table (for tests and assertions).
    pub fn table(&self) -> &ProcTable {
        &self.table
    }

    /// Mutable process table (used by the harness and integration tests to seed
    /// processes).
    ///
    /// In C, `mproc` is a file-level global, and test processes outside the boot
    /// image are written into the table directly before the main loop; the Rust
    /// integration test (`tests/run_once_integration.rs`) seeds the caller/parent
    /// through this accessor — the production path
    /// (`init_fresh`/`fill_boot_procs`) never goes through it.
    pub fn table_mut(&mut self) -> &mut ProcTable {
        &mut self.table
    }

    /// The transport (for wire assertions).
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Mutable transport (used by integration tests to preset messages and script
    /// replies).
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Run the startup initialization.
    ///
    /// C: `sef_cb_init_fresh` — main.c:131-243. The step order corresponds
    /// one-to-one with C:
    ///
    /// ```text
    /// main.c:146-152  mproc table + timer init -> ProcTable::new() (at construction)
    /// main.c:154-165  signal set construction  -> compile-time constants (CORE_SIGSET etc.)
    /// main.c:167-170  sys_getmonparams         -> BootParams::monitor_params (placeholder)
    /// main.c:172-176  sys_getimage             -> BootParams::boot_image (placeholder)
    /// main.c:177-229  boot image fills mproc   -> fill_boot_procs()
    /// main.c:220-236  VFS_PM_INIT sync         -> vfs_init_sync()
    /// main.c:238      system_hz = sys_hz()     -> BootParams::system_hz (placeholder)
    /// main.c:241      sched_init()             -> init_scheduling()
    /// ```
    pub fn init(&mut self) {
        // Step 5: boot image fill (INIT + system processes).
        self.fill_boot_procs();

        // Step 6: exchange the process table with VFS (one send per entry plus a
        // final sendrec barrier).
        self.vfs_init_sync();

        // Step 8: assign INIT its userland scheduler (the SCHED protocol details
        // are in doc 16).
        self.init_scheduling();

        self.initialized = true;
    }

    /// Enter the main loop (never returns).
    ///
    /// C: `main()`'s main loop — main.c:59-110.
    ///
    /// A single pass lives in [`Self::run_once`] (tests can drive one pass); `run`
    /// only provides the infinite loop plus a receive-failure cap (against
    /// busy-spin, the same shape as VM's `vm_server.rs::run`).
    pub fn run(&mut self) -> ! {
        assert!(self.initialized, "PmServer::run() called before init()");

        let mut consecutive_recv_failures: u32 = 0;
        loop {
            match self.run_once() {
                RunStep::Handled => consecutive_recv_failures = 0,
                RunStep::ReceiveFailed => {
                    // C: sef_receive_status blocks until a message arrives
                    // (main.c:61), so a receive Err means the transport itself is
                    // broken — fail-fast.
                    consecutive_recv_failures = consecutive_recv_failures.saturating_add(1);
                    if consecutive_recv_failures >= MAX_CONSECUTIVE_RECV_FAILURES {
                        panic!(
                            "IPC transport permanently broken: {} consecutive receive failures",
                            consecutive_recv_failures
                        );
                    }
                }
            }
        }
    }

    /// Handle exactly one IPC message (or one receive failure).
    ///
    /// Mirrors one pass of C's main loop (main.c:59-106):
    ///
    /// ```text
    /// sef_receive_status      -> receive() (transport)
    /// is_ipc_notify           -> skip (CLOCK -> expire_timers, doc 14)
    /// pm_isokendpt            -> caller slot validation (doc 03)
    /// EXITING drop            -> a deferred call of an exiting process is dropped
    /// three-way dispatch      -> dispatch_message (VFS/events/PM calls)
    /// result != SUSPEND       -> reply() (ReplyIntent::Reply)
    /// ```
    ///
    /// [ARCH: A-3] C's implicit global context `m_in`/`who_p`/`who_e`/`call_nr`
    /// becomes explicit Rust parameters (`msg`/`rcv_sts`) plus a `UserSlot` slot
    /// (04 doc §3.5).
    ///
    /// Public as a single-step driver: `run()` calls it in a loop, and the
    /// harness/integration test (`tests/run_once_integration.rs`) uses it to drive
    /// the whole message loop from the outside.
    pub fn run_once(&mut self) -> RunStep {
        // C: main.c:61 — sef_receive_status(ANY, &m_in, &ipc_status)。
        let (msg, rcv_sts) = match self.transport.receive() {
            Ok(v) => v,
            Err(_) => return RunStep::ReceiveFailed,
        };

        // The signal face (documented by T2): C's PM does not register a signal
        // handler (grepping `sef_setcb_signal_handler` across servers/pm/ gives
        // zero hits), so every SYSTEM notification PM receives in C is rewritten by
        // SEF's interceptor arm (sef.c:187), consumed by the library's default
        // empty handler, and the framework does `continue` (sef.h:287 +
        // sef_signal.c:156-158 + sef.c:231-235) — C's is_ipc_notify arm only ever
        // sees CLOCK. This tree's transport receives raw traps (no SEF classifier),
        // so SYSTEM notifications surface to the notify arm below and are used as
        // the pull-loop trigger — that divergence is recorded in the [ARCH] note on
        // the SYSTEM arm. The manager shape (SIGS_SIGNAL_RECEIVED messages) is not
        // intercepted either and would fall into the three-way dispatch; today no
        // sender can hand it to PM (RS's signal_manager has no production caller,
        // and PM's own send arm is stuck on the asynsend3 retry table), so the
        // difference is unreachable.
        // C: main.c:65-71 — is_ipc_notify: CLOCK → expire_timers (doc 14).
        // A notification is an asynchronous signal (clock tick / kernel interrupt),
        // not a request message, so endpoint validation is skipped and the loop
        // continues directly.
        if rcv_sts.is_notify() {
            // C main.c:65-71: is_ipc_notify → CLOCK → expire_timers (doc 14).
            // Notifications from other sources are not request messages, so
            // endpoint validation is skipped and the loop continues directly.
            if msg.m_source == Endpoint::CLOCK {
                // C main.c:66-67 uses the kernel timestamp in the notification
                // payload (m_notify.timestamp, stamped on the kernel side); an
                // earlier form used the TimerCtl clock at handling time, which
                // shifted every expiry decision whenever the main loop was
                // congested.
                let now = unsafe { msg.m_u.m_notify.timestamp } as minix_types::Clock;
                crate::timer::handle_clock_notify(
                    &mut self.table,
                    now,
                    self.timer.as_mut(),
                    self.kern.as_mut(),
                    &mut self.transport,
                );
            }
            // C sef_signal.c:104-108 (main.c:121's signal-manager registration):
            // the kernel delivers the `m_notify.sigset` bitmap in a SYSTEM-source
            // notification, where the SIGKSIG (74) bit means "the signal manager
            // has accumulated kernel signals to pull". An earlier form recognised
            // only CLOCK in this branch, silently dropped SIGKSIG notifications,
            // left process_ksig without a production caller, and broke the kernel
            // signal loop.
            //
            // [ARCH] The trigger test follows the Rust kernel's actual behaviour:
            // SigSet(u64) cannot hold bit 73 (SIGKSIG-1) — kernel
            // syscall_signal.rs:88-94 declares that limitation (the bitmap bit is
            // lost; the SYSTEM notification itself is the wakeup), and widening
            // SigSet to 128 bits is a cross-layer wire change (recorded as edge
            // E6). PM therefore uses a SYSTEM-source notification to trigger the
            // pull loop, and the bitmap semantics return once the width is
            // widened.
            if msg.m_source == Endpoint::SYSTEM {
                crate::signal::process_sigmgr_signals(
                    &mut self.table,
                    self.vtimer.as_mut(),
                    self.kern.as_mut(),
                    &mut self.transport,
                );
            }
            return RunStep::Handled;
        }

        // The E-BIRTHFACE (NS1) birth arm: C's RS_INIT is consumed inside
        // sef_local_startup (main.c:56 → sef.c:127-141 blocking on
        // IS_SEF_INIT_REQUEST), so the loop body never sees it; this tree has no
        // separate startup face (server.init() is the fresh body at construction),
        // so it is intercepted here and answered per process_init's tail
        // (sef_init.c:113-117) with RS_INIT+result. Registration face fresh entity
        // + restart STATEFUL (main.c:118-119) → both fresh and restart are OK; an
        // unregistered LU → default ENOSYS (sef_init.c:324-327). A failed answer
        // means RS is gone, which on the C side is equivalent to a lost init reply
        // (the RS boot step-3 timeout) — fail-fast, the same as transport
        // corruption.
        if msg.m_type == minix_types::RS_INIT && msg.m_source == Endpoint::RS {
            // SAFETY: m_type == RS_INIT and the source is RS, so the active union
            // arm is m_rs_init (the same unpack as sef_init.c:193-215).
            let init_type = unsafe { msg.m_u.m_rs_init.type_ };
            // The birth-protocol orchestration lives in the library: paragraph one
            // clears the IPC filters (C `process_init`'s first paragraph,
            // sef_init.c:59-60's `assert(r == OK)`, genuinely wired through
            // `KernelGateway::statectl`), and the reply message is built by the
            // library; the dispatch semantics (fresh entity + restart STATEFUL →
            // both OK; unregistered LU → default ENOSYS, sef_init.c:324-327) are
            // unchanged.
            let birth = minix_sef::process_init(
                &mut |req, addr, len| self.kern.statectl(req, addr, len),
                init_type,
                &mut |t| match t {
                    0 | 2 => 0, // SEF_INIT_FRESH / SEF_INIT_RESTART — sef.h:93-95
                    _ => minix_types::ENOSYS,
                },
            );
            let mut birth = match birth {
                Ok(m) => m,
                // C's first paragraph is an assert; this tree's fail-closed shape
                // is the same as transport corruption (a failed birth protocol has
                // no recovery face).
                Err(e) => panic!("PM: birth statectl failed: {e}"),
            };
            // The birth reply leg is C `sef_cb_init_response_rs_reply` =
            // `ipc_sendrec(RS, &m)` (sef_init.c:458-466), not a plain send.
            self.transport
                .sendrec(Endpoint::RS, &mut birth)
                .expect("PM: can't reply RS_INIT birth report to RS");
            return RunStep::Handled;
        }

        // C: main.c:74-77 — who_e = m_in.m_source; pm_isokendpt validates it.
        // C panics on an invalid endpoint ("PM got message from invalid endpoint");
        // Rust fails fast equivalently (the same behaviour as C, 04 doc §3.6).
        let caller = match self.table.pm_isokendpt(msg.m_source) {
            Ok(slot) => slot,
            Err(_) => panic!(
                "PM got message from invalid endpoint: {}",
                msg.m_source.get()
            ),
        };

        // C: main.c:80-82 — a deferred call of an EXITING process is dropped
        // (continue).
        let caller_proc = self
            .table
            .get(caller.get())
            .expect("pm_isokendpt validated slot");
        if caller_proc.is_exiting() {
            return RunStep::Handled;
        }

        // C: main.c:84-87 — first path: the VFS asynchronous reply (the state
        // machine is in 05-vfs-interaction.md). Note: it must be intercepted before
        // dispatch_message — VFS replies do not share the path with ordinary PM
        // calls.
        if is_vfs_pm_rs(msg.m_type) && msg.m_source == Endpoint::VFS {
            let mut svc = PmServices::new(
                &mut self.table,
                &mut self.transport,
                &mut self.event_registry,
                self.kern.as_mut(),
                self.abort_flag,
            );
            if let Err(e) = handle_vfs_reply(&mut svc, &msg) {
                // C: main.c:317-319 / 324-325 / 417-418 panic on the four kinds of
                // corruption.
                panic!("handle_vfs_reply failed: {:?}", e);
            }
            return RunStep::Handled;
        }

        // Ping-pong suppression: a message from VFS whose m_type is not in the RS
        // family is silently dropped without a reply. The ping-pong amplifier in
        // the fb201 flood on real RISC-V hardware (mt=0xFFFF0FB2 = FS stamped
        // status=-1 leaking into the PM/VFS lane) was PM dispatching such messages
        // → ENOSYS back to VFS → VFS reprocessing and resending → loop. C's
        // counterpart: after the first-path check, main.c replies ENOSYS to an
        // unrecognised call number (main.c:102-103 `result = ENOSYS` →
        // main.c:106 reply) — C assumes VFS is a trusted source and that no
        // garbage travels in that lane, so the ENOSYS reflection is harmless; on
        // real RISC-V hardware the garbage does exist and the reflection is the
        // amplifier, so a silent drop is chosen (a deliberate divergence, not C's
        // panic path — C panics only on an invalid endpoint, main.c:77). Dropping
        // it breaks the "PM replies to VFS" half of the ping-pong, so VFS no longer
        // receives the ENOSYS reflection of its own stamped messages.
        //
        // Narrowing: the drop condition narrows from "any VFS source" to "a call
        // number that is not a registered PM call". VFS's exec chain sends
        // PM_EXEC_NEW (43, callnr.h:56) to PM as an ordinary call number (the
        // do_newexec slot at C table.c:57) — an unconditional VFS drop would
        // swallow that too, leaving the exec worker waiting for an EXEC_NEW reply
        // (the pd:02b01 probe showed PM receiving mt=0x02B from VFS with no reply).
        // Stamped leak messages (negative, or outside 1..=47) make from_call_nr
        // return None and still fall into this branch's silent drop — the ping-pong
        // suppression semantics are unchanged.
        if msg.m_source == Endpoint::VFS && PmCall::from_call_nr(msg.m_type).is_none() {
            return RunStep::Handled;
        }

        // C: main.c:88-103 — second/third path: event replies and the PM call
        // family all go through the single dispatch table (the dispatch_message
        // call below).
        // Call counting (C ENABLE_SYSCALL_STATS, misc.c:64) — counted per call
        // number (callnr.h:62 NR_PM_CALLS=48; PM_BASE=0, so the call number is the
        // index).
        #[cfg(feature = "syscall_stats")]
        if msg.m_type >= 1 && (msg.m_type as usize) < self.call_stats.len() / 4 {
            let off = msg.m_type as usize * 4;
            let cur = u32::from_le_bytes(
                self.call_stats[off..off + 4].try_into().unwrap(),
            );
            self.call_stats[off..off + 4]
                .copy_from_slice(&cur.wrapping_add(1).to_le_bytes());
        }

        // The **single dispatch table** of dispatch_message → dispatch_pm_call
        // (ARCH A-5). The main loop does not inline-intercept any PM call (the old
        // implementation's seven inline blocks have been absorbed, 04 doc §3.6).
        let mut timers = crate::timer::TimerFaces {
            tctl: self.timer.as_mut(),
            vctl: self.vtimer.as_mut(),
            system_hz: self.params.system_hz as i64,
            svrctl_store: &mut self.svrctl_store,
            call_stats: &mut self.call_stats,
            abort_flag: &mut self.abort_flag,
        };
        let intent = dispatch_message(
            &mut self.table,
            &mut self.event_registry,
            &mut self.transport,
            self.kern.as_mut(),
            &mut timers,
            caller,
            &msg,
        );

        // C: main.c:106 — result != SUSPEND → reply(who_p, result).
        if let ReplyIntent::Reply(code) = intent {
            self.reply(caller, code);
        }

        RunStep::Handled
    }

    /// Send a reply to the caller.
    ///
    /// C: `reply(proc_nr, result)` — main.c:250-270: set `mp_reply.m_type =
    /// result`, then `ipc_sendnb(mp_endpoint, &mp_reply)`.
    ///
    /// Difference from C (04 doc §3.5): C reuses a per-process persistent
    /// `mp_reply` buffer (handlers may pre-fill payload fields); Rust builds a
    /// `Message` each time, with payload extensions delivered by the individual
    /// handler docs (07–20). A send failure only prints a warning and does not
    /// panic (the same semantics as C, main.c:267-269).
    ///
    /// [ARCH: A-3] The slot is passed as a `UserSlot` (replacing C's bare
    /// `proc_nr` index).
    fn reply(&mut self, slot: UserSlot, result: i32) {
        let endpoint = self.table.procs[slot.get()].endpoint();
        // C: after `rmp->mp_reply.m_type = result`, `ipc_sendnb(rmp->mp_endpoint, &rmp->mp_reply)`
        // Rust: if handler pre-filled `ipc.reply` (e.g., wait's W_STOPCODE status), reuse it
        let mut msg = self.table.procs[slot.get()]
            .ipc
            .reply
            .take()
            .unwrap_or_default();
        msg.m_type = result;
        // m_source is set by kernel on receive, not needed for send
        if let Err(e) = self.transport.send(endpoint, &msg) {
            // C: printf("PM can't reply to %d (%s): %d") — warn, do not panic.
            //[SD19-gate] PM send/notify/reply is fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = e;
            #[cfg(test)]
            pm_diag!("PM can't reply to {}: {:?}", endpoint.get(), e);
        }
    }

    /// Step 5: fill the mproc table from the boot image.
    ///
    /// C: main.c:177-229. Only entries with `proc_nr >= 0` are handled (a negative
    /// value is a kernel task); the INIT and system-process identity paths are
    /// kept separate.
    fn fill_boot_procs(&mut self) {
        let mut procs_in_use = 0usize;
        for ip in self.params.boot_image.iter() {
            if ip.proc_nr < 0 || ip.endpoint.is_none() {
                // C: main.c:179 — tasks have a negative proc_nr and are skipped.
                // Rust additionally skips padding entries: once the boot image is
                // copied into the fixed-length `[BootImage; NR_BOOT_PROCS]`, an
                // empty slot has `endpoint == NONE` (C's array is filled exactly,
                // minix-types' is not; the same rule as VM's init_boot_procs).
                continue;
            }
            let slot = ip.proc_nr as usize;
            procs_in_use += 1;

            let pid = if ip.proc_nr == INIT_PROC_NR {
                INIT_PID
            } else {
                // C: main.c:209 — system processes get a pid from get_free_pid().
                self.table.pid_generator.get_free_pid(&self.table)
            };

            let proc = self
                .table
                .get_mut(slot)
                .expect("boot image proc_nr within NR_PROCS");

            // C: main.c:183-186 — name plus cleared signal sets (Rust's default is
            // an empty bitmap).
            proc.identity.name = ip.proc_name;
            proc.identity.id.index = UserSlot::new(slot);
            proc.identity.endpoint = ip.endpoint;

            if ip.proc_nr == INIT_PROC_NR {
                // The INIT branch: C main.c:188-201.
                proc.identity.id.pid = INIT_PID;
                proc.identity.procgrp = INIT_PID;
                // C: main.c:189-194 — INIT is "its own father" (PM assumes
                // mp_parent always points at a valid slot; see the adoption
                // semantics in 09-pm-exit.md).
                proc.state.guardianship = Guardianship::Normal {
                    parent: UserSlot::new(INIT_PROC_NR as usize),
                };
                proc.state.lifecycle = Lifecycle::Running; // IN_USE
                proc.resources.privilege = Privilege::User(crate::mproc::Credentials::default());
                // C: main.c:199-200 — INIT is initially scheduled by the kernel.
                proc.resources.scheduler = Endpoint::KERNEL;
                proc.resources.nice = nice_from_queue(USR_Q);
            } else {
                // The system-process branch: C main.c:202-215.
                proc.identity.id.pid = pid;
                // C: main.c:203-208 — RS is its own father INIT; the others have RS
                // as father.
                let parent = if ip.proc_nr == RS_PROC_NR {
                    INIT_PROC_NR as usize
                } else {
                    RS_PROC_NR as usize
                };
                proc.state.guardianship = Guardianship::Normal {
                    parent: UserSlot::new(parent),
                };
                proc.state.lifecycle = Lifecycle::Running; // IN_USE
                proc.resources.privilege = Privilege::Kernel(crate::mproc::Credentials::default()); // PRIV_PROC
                // C: main.c:213-214 — system processes are scheduled by RS
                // (NONE = not specified).
                proc.resources.scheduler = Endpoint::NONE;
                proc.resources.nice = nice_from_queue(SRV_Q);
            }
        }
        self.table.procs_in_use.set(procs_in_use);
    }

    /// Step 6: the VFS_PM_INIT process-table sync.
    ///
    /// C: main.c:220-236. One `ipc_send` per entry (one per boot process) plus a
    /// final `ipc_sendrec` barrier (endpoint = NONE, and VFS's reply must be OK).
    ///
    /// Difference from C: C sends each entry inside the fill loop, while Rust
    /// completes the whole fill first and then sends them together — a send
    /// failure aborts startup either way (C panic ↔ Rust expect), so the
    /// observable behaviour is the same.
    fn vfs_init_sync(&mut self) {
        let messages = self.vfs_init_messages();
        for msg in messages[..messages.len() - 1].iter() {
            self.transport
                .send_blocking(self.params.vfs_endpoint, msg)
                .expect("PM: can't sync up with VFS (per-process send)");
        }
        // C: main.c:231-236 — the final sendrec; the reply's m_type must be OK.
        // The `{:?}` of expect once looped forever formatting an unknown error
        // payload (Debug::fmt spun on `<&T as Debug>::fmt` in an earlier
        // incarnation) — printing only the errno value closes off that formatting
        // surface and exposes the real error code directly.
        let mut barrier = *messages.last().expect("at least final barrier message");
        if let Err(e) = self.transport.sendrec(self.params.vfs_endpoint, &mut barrier) {
            panic!(
                "PM: can't sync up with VFS (final barrier) errno={}",
                e.errno()
            );
        }
        assert_eq!(
            barrier.m_type, 0, /* OK */
            "VFS did not confirm PM init (m_type = {})",
            barrier.m_type
        );
    }

    /// Build all VFS_PM_INIT messages (one per entry plus the final barrier, so it
    /// can be tested independently).
    fn vfs_init_messages(&self) -> Vec<Message> {
        let mut msgs = Vec::new();
        for ip in self.params.boot_image.iter() {
            if ip.proc_nr < 0 || ip.endpoint.is_none() {
                // The same rule as fill_boot_procs: skip kernel tasks and padding
                // entries.
                continue;
            }
            let slot = ip.proc_nr as usize;
            let proc = &self.table.procs[slot];
            msgs.push(
                VfsPmInit {
                    slot: ip.proc_nr,
                    pid: proc.identity.id.pid,
                    endpoint: proc.identity.endpoint,
                }
                .encode(),
            );
        }
        // C: main.c:231-236 — the final entry has endpoint = NONE (no more system
        // processes).
        msgs.push(
            VfsPmInit {
                slot: 0,
                pid: 0,
                endpoint: Endpoint::NONE,
            }
            .encode(),
        );
        msgs
    }

    /// Step 8: assign INIT its userland scheduler (the call site).
    ///
    /// C: `sched_init()` — schedule.c:36-69. It walks mproc and calls
    /// `sched_start` for slots that are `IN_USE` and not `PRIV_PROC` (at boot only
    /// INIT qualifies) — the SCHEDULING_START protocol, SCHED_PROC_NR=4.
    ///
    /// DEFERRED: the actual SCHED transport is in 16-scheduling.md (the
    /// minix_sched client).
    fn init_scheduling(&mut self) {
        for slot in 0..NR_PROCS {
            let (in_use, is_priv, endpoint, parent_slot) = {
                let p = &self.table.procs[slot];
                (
                    p.is_in_use(),
                    p.is_kernel_process(),
                    p.endpoint(),
                    p.parent(),
                )
            };
            if !in_use || is_priv {
                // C: schedule.c:43-44 — system processes (PRIV_PROC) are not taken
                // over.
                continue;
            }
            // C: schedule.c:46 — assert(_ENDPOINT_P(endpoint) == INIT_PROC_NR).
            assert_eq!(
                endpoint.slot(),
                INIT_PROC_NR,
                "only INIT is user-scheduled at boot (slot {} endpoint {:?})",
                slot,
                endpoint
            );
            let parent_endpoint = self.table.procs[parent_slot.get()].endpoint();
            // C: schedule.c:47-48 — assert(parent_e == schedulee_e).
            assert_eq!(parent_endpoint, endpoint, "INIT parent must be itself");

            // C: schedule.c:49-58 — sched_start(SCHED_PROC_NR, schedulee,
            // parent, USER_Q, USER_QUANTUM, -1, &mp_scheduler).
            //
            // DEFERRED: the minix_sched client (16-scheduling.md).
            let new_sched = self.sched_start(endpoint, parent_endpoint);
            match new_sched {
                Ok(scheduler) => self.table.procs[slot].resources.scheduler = scheduler,
                Err(rv) => {
                    // C: schedule.c:60-67 — on failure, print a warning only, do
                    // not panic.
                    #[cfg(test)]
                    pm_diag!(
                        "PM: SCHED denied taking over scheduling of slot {} (rv {})",
                        slot,
                        rv
                    );
                    #[cfg(not(test))]
                    //[SD19-gate] PM send/notify/reply is fire-and-forget
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = rv;
                }
            }
        }
    }

    /// The `sched_start` call site (C libsys/sched_start.c:46-88: SCHEDULING_START
    /// with the four fields endpoint/parent/maxprio/quantum; on success the
    /// scheduler can be handed over through the reply, and the model takes the
    /// **scheduler that was sent** — INIT's scheduler is SCHED itself, so there is
    /// no hand-over case).
    fn sched_start(&mut self, schedulee: Endpoint, parent: Endpoint) -> Result<Endpoint, i32> {
        let mut msg = Message::default();
        msg.m_u.m_lsys_sched_scheduling_start =
            minix_types::ipc::MessLsysSchedSchedulingStart {
                endpoint: schedulee.0,
                parent: parent.0,
                maxprio: USER_Q,
                quantum: USER_QUANTUM,
                _padding: [0; 40],
            };
        msg.m_type = minix_types::SCHEDULING_START;
        // C `_taskcall`'s "block until delivery/reply" semantics: when SCHED has
        // not entered receive yet, sendrec returns Ok with the reply `m_type` =
        // ELOCKED(208), so it must be retried until a real reply arrives (the same
        // shape as the ELOCKED retry in
        // `sched.rs::MinixSchedCtl::taskcall`; boot-time tick preemption guarantees
        // progress, so the two cannot deadlock forever). The 208 test must come
        // before the generic refusal-code arm, or it would be mistaken for a
        // refusal code, return Err, and leave the scheduler unregistered.
        let mut retries: u32 = 0;
        loop {
            match self.transport.sendrec(Endpoint::SCHED, &mut msg) {
                Ok(()) if msg.m_type == 208 => {
                    retries = retries.wrapping_add(1);
                    if retries > 1_000_000 {
                        return Err(-minix_types::EIO);
                    }
                }
                // C libsys/sched_start.c:87 — the code `_taskcall` returns from
                // SCHED is rv: `if ((rv = _taskcall(scheduler_e, SCHEDULING_START,
                // &m))) return rv;`. The old code ignored the reply `m_type` and
                // answered `Ok(SCHED)`, which is the root cause of the reported
                // "INIT falsely registers scheduler=SCHED" (a reply of
                // ENOSYS/EDEADEPT also counted as success → a later fork
                // necessarily hit the parent-Dead live-lock). The reply `m_type`
                // semantics match the
                // `MinixSchedCtl::taskcall` leg (OK=0 / a non-zero refusal code).
                Ok(()) if msg.m_type != minix_types::OK => return Err(msg.m_type),
                // On success, C reads back
                // `*newsched_e = m.m_sched_lsys_scheduling_start.scheduler`; INIT's
                // scheduler is always SCHED (no hand-over case), so return
                // Endpoint::SCHED directly.
                Ok(()) => return Ok(Endpoint::SCHED),
                // The transport itself failed (equally fatal on C's startup path):
                // fold to -EIO and let the caller warn only, per
                // schedule.c:60-67, without panicking.
                Err(_) => return Err(-minix_types::EIO),
            }
        }
    }
}

/// The queue → nice conversion (the call site).
///
/// C: `get_nice_value` — main.c:276-295 (static). The full scheduling semantics
/// (nice_to_priority etc.) are in 16-scheduling.md.
fn nice_from_queue(queue: i32) -> i32 {
    let nice_val = (queue - USER_Q) * (PRIO_MAX - PRIO_MIN + 1) / (MIN_USER_Q - MAX_USER_Q + 1);
    nice_val.clamp(PRIO_MIN, PRIO_MAX)
}

// ── The main loop (04-ipc-dispatch.md) ──

/// The cap on consecutive receive failures.
///
/// C: sef_receive_status blocks waiting (main.c:61), so a failure means the
/// transport is broken; once consecutive failures reach the cap it fails fast with
/// a panic (against busy-spin, the same value and semantics as VM's
/// `MAX_CONSECUTIVE_RECV_FAILURES = 64`).
const MAX_CONSECUTIVE_RECV_FAILURES: u32 = 64;

/// The outcome of one main-loop pass.
///
/// `run()` uses it to maintain the consecutive-failure count, and a test driving a
/// single pass uses it with `run_once`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStep {
    /// One message was handled (or one failure was counted).
    Handled,
    /// receive returned an error (broken transport; `run()` counts it and may
    /// panic).
    ReceiveFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{IpcStatus, TestIpcTransport};
    use crate::mproc::IpcBlockReason;
    use minix_types::{BootImage, ENOSYS, OK, VFS_PM_SETUID_REPLY};

    fn boot_image_with(entries: &[(i32, [u8; 16], Endpoint)]) -> [BootImage; NR_BOOT_PROCS] {
        let mut image = [BootImage::empty(); NR_BOOT_PROCS];
        for (i, (nr, name, ep)) in entries.iter().enumerate() {
            image[i] = BootImage {
                proc_nr: *nr,
                proc_name: *name,
                endpoint: *ep,
                start_addr: 0,
                len: 0,
            };
        }
        image
    }

    fn name(s: &str) -> [u8; 16] {
        let mut n = [0u8; 16];
        n[..s.len()].copy_from_slice(s.as_bytes());
        n
    }

    fn test_params() -> BootParams {
        BootParams {
            monitor_params: [0; MULTIBOOT_PARAM_BUF_SIZE],
            boot_image: boot_image_with(&[
                (0, name("pm"), Endpoint::PM),
                (1, name("vfs"), Endpoint::VFS),
                (2, name("rs"), Endpoint::RS),
                (11, name("init"), Endpoint::INIT),
                (-3, name("clock"), Endpoint::CLOCK),
            ]),
            system_hz: 100,
            vfs_endpoint: Endpoint::VFS,
        }
    }

    /// The consume half of the boot truth chain: `acquire_from`'s three kernel
    /// calls, their order and wire shapes (monparams 1024 → the whole image table
    /// → hz), and the failure path returning the original errno without issuing
    /// the later calls.
    #[test]
    fn test_boot_params_acquire_wire_shapes() {
        use minix_sys::syscall::{CannedKernelCallTransport, KernelCallTransport};
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0); // GET_MONPARAMS
        canned.reply(0); // GET_IMAGE
        canned.reply(0); // GET_HZ
        let params = BootParams::acquire_from(&canned).unwrap();
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 3);
        // SAFETY(test): all three are GETINFO payload arms.
        let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
        assert_eq!(gi.request, minix_types::GET_MONPARAMS);
        assert_eq!(gi.val_len, MULTIBOOT_PARAM_BUF_SIZE as i32);
        let gi = unsafe { sent[1].m_u.m_lsys_krn_sys_getinfo };
        assert_eq!(gi.request, minix_types::GET_IMAGE);
        assert_eq!(
            gi.val_len,
            (NR_BOOT_PROCS * core::mem::size_of::<minix_types::BootImageStruct>()) as i32
        );
        let gi = unsafe { sent[2].m_u.m_lsys_krn_sys_getinfo };
        assert_eq!(gi.request, minix_types::GET_HZ);
        assert_eq!(params.vfs_endpoint, Endpoint::VFS);

        // The first kernel call fails: Err passes the original errno through, and
        // the later calls are not issued.
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-minix_types::EPERM);
        assert_eq!(
            BootParams::acquire_from(&canned).unwrap_err(),
            -minix_types::EPERM
        );
        assert_eq!(canned.sent.borrow().len(), 1);
    }

    #[test]
    fn test_signal_sets_match_c() {
        // C: main.c:154-165 (built with sigaddset) + sys/sys/sigtypes.h:67
        // (__sigmask(n) = 1<<((n-1)&31), bit i corresponding to signal i+1).
        // The asserted values are the bit-for-bit values of C's sigset_t, pinned
        // together with the bit-base convention.
        assert_eq!(
            CORE_SIGSET,
            (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 6) | (1 << 7) | (1 << 9) | (1 << 10)
        );
        assert_eq!(IGN_SIGSET, (1 << 18) | (1 << 19) | (1 << 27) | (1 << 28));
        assert_eq!(
            NOIGN_SIGSET,
            (1 << 3) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 9) | (1 << 10)
        );
        // Every signal number is ≤ 29 < _NSIG = 64 (inside the u64 bitmap).
        let highest_used = CORE_SIGSET | IGN_SIGSET | NOIGN_SIGSET;
        assert_eq!(highest_used.ilog2() as i32, 28);
    }

    /// Per-signal reconciliation: membership is decided with C `__sigmask`'s raw
    /// mask expression (`1 << (signo-1)`) rather than through `sig_bit` — this test
    /// and test_signal_sets_match_c independently pin the set values, so if the
    /// bit-base convention is broken again, both tests fail.
    #[test]
    fn test_signal_set_membership_matches_c_arrays() {
        // C main.c:137-141: the original values of the three arrays
        // core_sigs/ign_sigs/noign_sigs.
        let core = [SIGQUIT, SIGILL, SIGTRAP, SIGABRT, SIGEMT, SIGFPE, SIGBUS, SIGSEGV];
        let ign = [SIGCHLD, SIGWINCH, SIGCONT, SIGINFO];
        let noign = [SIGILL, SIGTRAP, SIGEMT, SIGFPE, SIGBUS, SIGSEGV];
        for signo in 1..=31 {
            let c_mask = 1u64 << (signo - 1);
            assert_eq!(
                CORE_SIGSET & c_mask != 0,
                core.contains(&signo),
                "core membership mismatch for signal {signo}"
            );
            assert_eq!(
                IGN_SIGSET & c_mask != 0,
                ign.contains(&signo),
                "ign membership mismatch for signal {signo}"
            );
            assert_eq!(
                NOIGN_SIGSET & c_mask != 0,
                noign.contains(&signo),
                "noign membership mismatch for signal {signo}"
            );
        }
    }

    #[test]
    fn test_nice_from_queue_default_queues() {
        // USR_Q = SRV_Q = USER_Q = 7 → nice 0 (main.c:200/214).
        assert_eq!(nice_from_queue(USR_Q), 0);
        assert_eq!(nice_from_queue(SRV_Q), 0);
        // Linear scaling at the endpoints (main.c:281-289): (q-7)*41/16, truncated
        // toward zero as C does.
        assert_eq!(nice_from_queue(MAX_USER_Q), -17);
        assert_eq!(nice_from_queue(MIN_USER_Q), 20);
    }

    /// C-fidelity regression: PM's `sched_start` must check SCHED's reply code
    /// (C libsys/sched_start.c:87 `if ((rv = _taskcall(...))) return rv;`). The old
    /// code ignored the reply `m_type` and answered `Ok(SCHED)` → INIT falsely
    /// registered a scheduler → a later fork necessarily hit the parent-Dead
    /// live-lock.
    #[test]
    fn test_sched_start_propagates_denied_reply_code() {
        // 1) SCHED refuses (ENOSYS) → Err(ENOSYS), not passed through as
        // Ok(SCHED).
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.transport.set_reply_type(ENOSYS);
        assert_eq!(
            server.sched_start(Endpoint::INIT, Endpoint::INIT),
            Err(ENOSYS),
            "SCHED 拒绝 INIT START 时 PM 不得假登记调度器"
        );
        // 2) SCHED accepts (OK) → Ok(SCHED).
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.transport.set_reply_type(OK);
        assert_eq!(server.sched_start(Endpoint::INIT, Endpoint::INIT), Ok(Endpoint::SCHED));
    }

    #[test]
    fn test_fill_boot_init_identity() {
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.fill_boot_procs();

        let init = server.table().get(INIT_PROC_NR as usize).unwrap();
        assert_eq!(init.identity.id.pid, 1); // INIT_PID
        assert_eq!(init.identity.procgrp, 1);
        assert_eq!(init.parent(), UserSlot::new(11)); // its own father
        assert!(init.is_in_use());
        assert_eq!(init.resources.scheduler, Endpoint::KERNEL);
        assert_eq!(init.resources.nice, 0);
        assert_eq!(init.identity.endpoint, Endpoint::INIT);
    }

    #[test]
    fn test_fill_boot_system_procs() {
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.fill_boot_procs();

        // System processes: parent = RS (slot 2), PRIV_PROC, scheduler = NONE.
        let pm = server.table().get(0).unwrap();
        assert_eq!(pm.parent(), UserSlot::new(2));
        assert!(pm.is_kernel_process());
        assert_eq!(pm.resources.scheduler, Endpoint::NONE);
        assert_eq!(pm.identity.endpoint, Endpoint::PM);

        // RS is its own father INIT (main.c:204-206).
        let rs = server.table().get(2).unwrap();
        assert_eq!(rs.parent(), UserSlot::new(11));

        // PID assignment: PM→3, VFS→4, RS→5 (the get_free_pid order; under C's
        // phase the first allocated value is INIT_PID+2=3 and pid 2 is never used —
        // utility.c:38 increments before returning, the corrected phase).
        assert_eq!(server.table().get(0).unwrap().identity.id.pid, 3);
        assert_eq!(server.table().get(1).unwrap().identity.id.pid, 4);
        assert_eq!(server.table().get(2).unwrap().identity.id.pid, 5);

        // Negative proc_nr (kernel tasks) are skipped and not counted in
        // procs_in_use.
        assert_eq!(server.table().procs_in_use.get(), 4);
    }

    #[test]
    fn test_vfs_init_message_fields() {
        // com.h:520/547-551 — VFS_PM_ENDPT=m7_i1, VFS_PM_SLOT=m7_i2,
        // VFS_PM_PID=m7_i3。
        let msg = VfsPmInit {
            slot: 11,
            pid: 1,
            endpoint: Endpoint::INIT,
        }
        .encode();
        assert_eq!(msg.m_type, VFS_PM_INIT);
        // SAFETY: m_m7 was just written in full by encode().
        let m7 = unsafe { msg.m_u.m_m7 };
        assert_eq!(m7.m7i1, Endpoint::INIT.get());
        assert_eq!(m7.m7i2, 11);
        assert_eq!(m7.m7i3, 1);
    }

    #[test]
    fn test_vfs_init_messages_order_and_final() {
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.fill_boot_procs();
        let msgs = server.vfs_init_messages();

        // Four per-entry messages (PM/VFS/RS/INIT) plus one final barrier.
        assert_eq!(msgs.len(), 5);
        for (i, m) in msgs.iter().enumerate() {
            assert_eq!(m.m_type, VFS_PM_INIT, "msgs[{}].m_type", i);
            assert_eq!(m.m_source, Endpoint::NONE, "msgs[{}].m_source", i);
        }
        let last = msgs.last().unwrap();
        assert_eq!(last.m_type, VFS_PM_INIT);
        // SAFETY: the final entry's m_m7 is written by encode().
        let m7 = unsafe { last.m_u.m_m7 };
        assert_eq!(m7.m7i1, Endpoint::NONE.get());
        assert_eq!(m7.m7i2, 0);
        assert_eq!(m7.m7i3, 0);
    }

    // ── The main loop (04-ipc-dispatch.md) ──

    fn running_proc_at(table: &mut ProcTable, slot: usize, ep: Endpoint) {
        let p = &mut table.procs[slot];
        p.identity.endpoint = ep;
        p.state.lifecycle = Lifecycle::Running;
    }

    #[test]
    fn test_run_once_skips_notify() {
        // C: main.c:65-71 — is_ipc_notify: a notification from a non-CLOCK source
        // produces no reply and skips endpoint validation.
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let mut msg = Message { m_type: 0x1000, ..Message::default() }; // NOTIFY_MESSAGE（com.h:90）
        msg.m_source = Endpoint::RS; // not CLOCK
        server.transport.queue_receive(msg, IpcStatus { flags: 4 });
        assert_eq!(server.run_once(), RunStep::Handled);
        assert!(server.transport.sent().is_empty());
    }

    /// The birth test gateway: only statectl is accepted (paragraph one of the birth
    /// orchestration), and no other kernel verb should be touched by the birth path
    /// — a panic is the bell that the test's intent has drifted. The host trap
    /// answers `-EIO` honestly (no faked success before the E1 power-on), and the
    /// birth orchestration test cares about the report shape, not the kernel round
    /// trip.
    struct BirthOnlyGateway;
    impl crate::exit::KernelGateway for BirthOnlyGateway {
        fn statectl(&mut self, _: i32, _: u64, _: i32) -> Result<i32, i32> {
            Ok(minix_types::OK)
        }
        fn sys_kill(&mut self, _: Endpoint, _: i32) -> Result<(), i32> {
            panic!("birth test: unexpected sys_kill")
        }
        fn sys_clear(&mut self, _: Endpoint) -> Result<(), i32> {
            panic!("birth test: unexpected sys_clear")
        }
        fn sys_abort(&mut self, _: i32) -> Result<(), i32> {
            panic!("birth test: unexpected sys_abort")
        }
        fn proc_times(
            &mut self,
            _: Endpoint,
        ) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
            panic!("birth test: unexpected proc_times")
        }
        fn copy_to_user(&mut self, _: &[u8], _: Endpoint, _: u64) -> Result<(), i32> {
            panic!("birth test: unexpected copy_to_user")
        }
        fn sys_resume(&mut self, _: Endpoint) -> Result<(), i32> {
            panic!("birth test: unexpected sys_resume")
        }
        fn sys_delay_stop(&mut self, _: Endpoint) -> Result<(), i32> {
            panic!("birth test: unexpected sys_delay_stop")
        }
        fn sys_trace(&mut self, _: i32, _: Endpoint, _: u64, _: &mut i64) -> Result<(), i32> {
            panic!("birth test: unexpected sys_trace")
        }
        fn sys_vircopy(
            &mut self,
            _: Endpoint,
            _: u64,
            _: Endpoint,
            _: u64,
            _: u64,
        ) -> Result<(), i32> {
            panic!("birth test: unexpected sys_vircopy")
        }
        fn copy_from_user(&mut self, _: Endpoint, _: u64, _: &mut [u8]) -> Result<(), i32> {
            panic!("birth test: unexpected copy_from_user")
        }
        fn get_ksig(&mut self) -> Result<Option<(Endpoint, minix_types::SigSetBits)>, i32> {
            panic!("birth test: unexpected get_ksig")
        }
        fn end_ksig(&mut self, _: Endpoint, _: i32) -> Result<(), i32> {
            panic!("birth test: unexpected end_ksig")
        }
        fn sys_sigsend(&mut self, _: Endpoint, _: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> {
            panic!("birth test: unexpected sys_sigsend")
        }
        fn sys_diagctl_stacktrace(&mut self, _: Endpoint) -> Result<(), i32> {
            panic!("birth test: unexpected sys_diagctl_stacktrace")
        }
    }

    #[test]
    fn test_run_once_birth_request_answers_rs_init_report() {
        // The E-BIRTHFACE (NS1): RS_INIT from RS → birth answer (the orchestration
        // face: paragraph-one statectl → dispatch → the library builds the report),
        // not falling into the pm_isokendpt/dispatch path — on the C side this
        // request is consumed by sef_local_startup (main.c:56). Registration face
        // fresh + restart STATEFUL (main.c:118-119) → both OK; LU → ENOSYS.
        let mut server = PmServer::with_kernel_gateway(
            test_params(),
            TestIpcTransport::new(),
            Box::new(BirthOnlyGateway),
        );
        let mut birth = Message {
            m_type: minix_types::RS_INIT,
            ..Message::default()
        };
        birth.m_source = Endpoint::RS;
        // Union write is safe: the RS_INIT tag selects the m_rs_init arm.
        birth.m_u.m_rs_init.type_ = 0; // SEF_INIT_FRESH
        server.transport.queue_receive(birth, IpcStatus { flags: 1 });
        assert_eq!(server.run_once(), RunStep::Handled);
        let sent = server.transport.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, Endpoint::RS);
        assert_eq!(sent[0].1.m_type, minix_types::RS_INIT);
        assert_eq!(sent[0].1.rs_init_result(), Some(0));

        let mut server2 = PmServer::with_kernel_gateway(
            test_params(),
            TestIpcTransport::new(),
            Box::new(BirthOnlyGateway),
        );
        let mut lu = Message {
            m_type: minix_types::RS_INIT,
            ..Message::default()
        };
        lu.m_source = Endpoint::RS;
        lu.m_u.m_rs_init.type_ = 1; // SEF_INIT_LU
        server2.transport.queue_receive(lu, IpcStatus { flags: 1 });
        server2.run_once();
        assert_eq!(server2.transport.sent()[0].1.rs_init_result(), Some(minix_types::ENOSYS));
    }

    /// A CLOCK notify enters the expire_timers path (handle_clock_notify); with no
    /// timer due there is zero side effect and zero reply.
    #[test]
    fn test_run_once_clock_notify_drives_expire_timers() {
        struct StubTimer;
        impl crate::timer::TimerCtl for StubTimer {
            fn set(&mut self, _ep: Endpoint, _ticks: minix_types::Clock) { panic!("no arm expected"); }
            fn cancel(&mut self, _ep: Endpoint) {}
            fn exptime(&self, _ep: Endpoint) -> Option<minix_types::Clock> { None }
            fn now(&self) -> minix_types::Clock { 42 }
        }
        let mut server = PmServer::with_timer_ctl(
            test_params(),
            TestIpcTransport::new(),
            Box::new(crate::exit::TrapKernelGateway::new(minix_sys::syscall::DirectKernelCallTransport)),
            Box::new(StubTimer),
            Box::new(crate::timer::SysVTimerCtl),
        );
        let mut msg = Message { m_type: 0x1000, ..Message::default() };
        msg.m_source = Endpoint::CLOCK;
        server.transport.queue_receive(msg, IpcStatus { flags: 4 });
        assert_eq!(server.run_once(), RunStep::Handled);
        assert!(server.transport.sent().is_empty());
    }

    #[test]
    #[should_panic(expected = "invalid endpoint")]
    fn test_run_once_invalid_endpoint_panics() {
        // C: main.c:75-76 — a pm_isokendpt failure panics (fail-fast).
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let mut msg = Message { m_type: 2, ..Message::default() }; // PM_FORK
        msg.m_source = Endpoint::from_generation_slot(9, 9); // unregistered slot
        server.transport.queue_receive(msg, IpcStatus::default());
        //[SD19-gate] PM send/notify/reply is fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = server.run_once();
    }

    #[test]
    fn test_run_once_drops_exiting_caller() {
        // C: main.c:80-82 — a deferred call of an EXITING process is dropped
        // (continue).
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let ep = Endpoint::from_generation_slot(1, 5);
        let p = &mut server.table.procs[5];
        p.identity.endpoint = ep;
        p.state.lifecycle = Lifecycle::Exiting {
            exit_code: 0,
            sig_status: 0,
        };
        let mut msg = Message { m_type: 2, ..Message::default() }; // PM_FORK
        msg.m_source = ep;
        server.transport.queue_receive(msg, IpcStatus::default());
        assert_eq!(server.run_once(), RunStep::Handled);
        assert!(server.transport.sent().is_empty());
    }

    #[test]
    fn test_run_once_replies_enosys_to_unimplemented_call() {
        // C: main.c:102-103 — call_index out of range (an unregistered number) →
        // result = ENOSYS; the main loop replies(who_p, result) (main.c:106).
        // (1..=47 are all wired now, so the "registered but unimplemented" class no
        // longer exists and ENOSYS has only the unregistered-number entry.)
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let ep = Endpoint::from_generation_slot(1, 5);
        running_proc_at(&mut server.table, 5, ep);
        let mut msg = Message { m_type: 48, ..Message::default() };
        msg.m_source = ep;
        server.transport.queue_receive(msg, IpcStatus::default());
        assert_eq!(server.run_once(), RunStep::Handled);
        let sent = server.transport.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, ep); // the reply goes to the caller endpoint
        // An unwired PM number is symmetrically negated, so m_type<0 on the wire is
        // the errno.
        assert_eq!(sent[0].1.m_type, -ENOSYS);
    }

    #[test]
    fn test_run_once_fork_no_sync_reply() {
        // C: do_fork returns SUSPEND (forkexit.c:139) — fork does not reply
        // synchronously; the reply comes through the 05 VFS_PM_FORK_REPLY. PM must
        // first vm_fork (sendrec to VM) and then tell_vfs the child (VFS_CALL).
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let ep = Endpoint::from_generation_slot(1, 5);
        running_proc_at(&mut server.table, 5, ep);
        // Scripted VM_FORK reply: empty table with cursor 0 → child slot 1 (the
        // parent is at 5).
        let mut vm_reply = Message { m_type: minix_types::OK, ..Message::default() };
        vm_reply.m_u.m_m1.m1i3 = Endpoint::from_generation_slot(2, 1).0;
        server.transport.queue_sendrec_reply(vm_reply);
        let mut msg = Message { m_type: 2, ..Message::default() }; // PM_FORK
        msg.m_source = ep;
        server.transport.queue_receive(msg, IpcStatus::default());
        assert_eq!(server.run_once(), RunStep::Handled);
        // Sent in order: VM_FORK (sendrec, before any non-rollbackable change) →
        // VFS_PM_FORK (the child's VFS_CALL); no synchronous reply to the parent
        // (SUSPEND).
        assert_eq!(server.transport.sent().len(), 2);
        assert_eq!(server.transport.sent()[0].0, Endpoint::VM);
        assert_eq!(server.transport.sent()[0].1.m_type, minix_types::VM_FORK as i32);
        assert_eq!(server.transport.sent()[1].0, Endpoint::VFS);
        assert_eq!(server.transport.sent()[1].1.m_type, minix_types::VFS_PM_FORK);
        // The child slot should be in VFS_CALL (the continuation hangs on the
        // child)
        assert!(server
            .table
            .procs
            .iter()
            .any(|p| p.state.block.is_vfs_blocked()));
    }

    #[test]
    fn test_run_once_vfs_reply_no_sync_reply() {
        // C: main.c:84-87 — a VFS asynchronous reply is handled immediately by the
        // handle_vfs_reply state machine, with no synchronous reply to VFS; the
        // state machine applies effects to "the process being replied to" (here
        // reply(OK)).
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        // m_source validation (pm_isokendpt) requires the VFS slot's endpoint to be
        // registered.
        running_proc_at(&mut server.table, 1, Endpoint::VFS);
        // The process being replied to (slot 5) is in the VFS_CALL state, and its
        // endpoint is written into the reply's m7_i1.
        let ep = Endpoint::from_generation_slot(1, 5);
        running_proc_at(&mut server.table, 5, ep);
        server.table.procs[5].state.block.ipc_blocked =
            Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        // The SETUID branch: reply(OK) plus the trailing restart_signals
        let mut msg = Message { m_type: VFS_PM_SETUID_REPLY, ..Message::default() };
        msg.m_source = Endpoint::VFS;
        // SAFETY: the reply message's m7_i1 carries the target process endpoint
        // (main.c:315-321).
        msg.m_u.m_m7.m7i1 = ep.get();
        server.transport.queue_receive(msg, IpcStatus::default());
        assert_eq!(server.run_once(), RunStep::Handled);
        // The state machine has already applied the effect: reply OK to the slot-5
        // process (not a synchronous reply to VFS).
        let sent = server.transport.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, ep); // the reply goes to the process being replied to
        assert_eq!(sent[0].1.m_type, OK);
        // The VFS_CALL flag has been cleared by the state machine (take_vfs_call).
        assert_eq!(server.table.procs[5].state.block.ipc_blocked, None);
    }

    #[test]
    fn test_run_once_receive_failure_reported() {
        // No message on receive → ReceiveFailed (run() counts it, against
        // busy-spin).
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        assert_eq!(server.run_once(), RunStep::ReceiveFailed);
    }

    #[test]
    #[should_panic(expected = "permanently broken")]
    fn test_run_panics_after_consecutive_receive_failures() {
        // C: sef_receive_status blocks waiting (main.c:61); on a broken transport it
        // fails fast (the same shape as VM).
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.initialized = true;
        //[SD19-gate] PM send/notify/reply is fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = server.run();
    }

    #[test]
    fn test_init_completes_with_mock_transport() {
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.init();
        assert!(server.initialized);

        // VFS sync: four per-entry sends plus one sendrec (also recorded in sent);
        // once the card-C leg was converted to the real thing, `init_scheduling`
        // sends one more SCHEDULING_START to SCHED (the boot chain is genuinely
        // wired: INIT's scheduling start is no longer a placeholder).
        let sent = server.transport.sent();
        assert_eq!(sent.len(), 6);
        assert_eq!(sent[0].0, Endpoint::VFS);
        assert_eq!(sent[0].1.m_type, VFS_PM_INIT);
        let (sched_e, sched_m) = sent.last().expect("SCHEDULING_START 已发");
        assert_eq!(*sched_e, Endpoint::SCHED);
        assert_eq!(sched_m.m_type, minix_types::SCHEDULING_START);
    }
}
