//! Boot initialization — the 4-step boot state machine.
//!
//! Mirrors `sef_cb_init_fresh()` (`minix3/minix/servers/rs/main.c:158-494`):
//! the RS server's fresh-boot initialization. The four steps are strictly
//! ordered — each step only relies on facilities established by the previous
//! step (see 01-rs-boot-init.md §1.1):
//!
//! 1. **Step 1** (`main.c:244-346`): establish priv/sys/dev attributes for
//!    every boot service in the local slot table.
//! 2. **Step 2** (`main.c:348-399`): allow services to run (`sched_init_proc`
//!    + `SYS_PRIV_ALLOW` + `init_service`; RS/VM exception).
//! 3. **Step 3** (`main.c:401-407`): catch all remaining init-ready messages.
//! 4. **Step 4** (`main.c:409-433`): `getnpid` for every service +
//!    `sys_setalarm(RS_DELTA_T)`.
//!
//! The C code touches global state (`rproc[]`/`rprocpub[]`, `rinit`, `rupdate`)
//! directly. Rust keeps the same state inside [`BootInit`], making the
//! dependencies explicit and the machine testable with a mock kernel API.
//!
//! # External boundary API
//!
//! All external interactions go through [`KernelApi`] — the union of five
//! domain faces ([`SysApi`] kernel calls, [`SchedApi`] scheduler face,
//! [`PmApi`] PM process lifecycle, [`VmApi`] VM messages, [`IpcApi`] RS's own
//! receive/reply). The production impl is [`crate::trap_api::TrapKernelApi`]
//! (19 号换装:每方法委托 minix-sys 真实 wrapper);tests use
//! `MockKernelApi`.

use minix_types::{BootImage, Clock, Endpoint, Errno, GrantId, Message, Pid};

use crate::privilege::{CallMask, PrivFlags};
use crate::process_table::RProcTable;
use crate::sched::SchedulerConfig;
use crate::service_slot::{SlotId, SysFlags};

use crate::table::{
    BOOT_IMAGE_DEV_TABLE, BOOT_IMAGE_PRIV_TABLE, BOOT_IMAGE_SYS_TABLE, BootImageDev, BootImagePriv,
    BootImageSys, DEFAULT_DEV, DEFAULT_SYS,
};

/// Machine information. C: `struct machine` — `minix3/minix/include/minix/type.h:122-131`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Machine {
    /// How many cpus are available. C: `machine.processors_count`.
    pub processors_count: u32,
    /// Id of the bootstrap cpu. C: `machine.bsp_id`.
    pub bsp_id: u32,
}

// Privilege types live in `privilege` (03-rs-privilege.md): `PrivCtlOp` is the
// full 11-opcode enum (com.h:342-353), `Privilege` is the modeled `struct priv`.
pub use crate::privilege::{PrivCtlOp, Privilege};

/// Kernel system-call face — the C `sys_*` libsys calls to `SYSTASK`.
///
/// Every method maps to one C call site:
///
/// | Method | C call site |
/// |--------|-------------|
/// | [`SysApi::get_machine`] | `sys_getmachine` — main.c:53 |
/// | [`SysApi::get_hz`] | `sys_getinfo(GET_HZ, ...)` — main.c:181 |
/// | [`SysApi::get_ticks`] | `getticks()` — main.c:333 (S2, alive_tm) |
/// | [`SysApi::privctl`] | `sys_privctl` — main.c:287/379 (boot) |
/// | [`SysApi::getpriv`] | `sys_getpriv` — main.c:294 |
/// | [`SysApi::setalarm`] | `sys_setalarm(RS_DELTA_T, 0)` — main.c:433 |
/// | [`SysApi::sys_kill`] | `sys_kill(rpub->endpoint, SIGKILL)` — manager.c:399 (crash_service) |
/// | [`SysApi::sys_update`] | `srv_update(src_ep, dst_ep, flags)` — update.c:272-275 (libsys wrapper over SYS_UPDATE) |
///
/// Errors are errno values (`minix-types` constants). Production impl:
/// `crate::trap_api::TrapKernelApi`(19 号换装已落).
pub trait SysApi {
    fn get_machine(&mut self) -> Result<Machine, Errno>;
    fn get_hz(&mut self) -> Result<u32, Errno>;
    fn get_ticks(&mut self) -> Result<Clock, Errno>;
    fn privctl(
        &mut self,
        proc: Endpoint,
        op: PrivCtlOp,
        priv_: Option<&Privilege>,
    ) -> Result<(), Errno>;
    fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, Errno>;
    fn setalarm(&mut self, delay_ticks: u32) -> Result<(), Errno>;

    /// Signals a process by endpoint (kernel sys_kill).
    ///
    /// Wired 19.
    fn sys_kill(&mut self, proc: Endpoint, signo: i32) -> Result<(), Errno>;

    /// Swaps two process identities in the kernel (SYS_UPDATE).
    ///
    /// Wired 19.
    fn sys_update(
        &mut self,
        src: Endpoint,
        dst: Endpoint,
        flags: crate::service_slot::SysFlags,
    ) -> Result<(), Errno>;

    /// Requests a stacktrace dump from a service.
    ///
    /// C: `sys_diagctl_stacktrace(target)` — main.c:681-683 (the signal
    /// manager's stacktrace-signal branch). Wired
    /// 19-rs-external-interfaces.md (production impl: `crate::trap_api::TrapKernelApi`).
    fn diagctl_stacktrace(&mut self, target: Endpoint) -> Result<(), Errno>;

    /// Emits one diagnostic line to the kernel console ([ARCH: A-6]).
    ///
    /// This is the universal observability seam for the whole `rs_verbose`
    /// family (C `DEBUG_DEFAULT`/`PRIV_DEBUG_DEFAULT`, const.h:6-7): C
    /// prints through printf, the rewrite routes server diagnostics through
    /// `sys_diagctl` code 1 (`minix_sys::sys_diagctl_write`), keeping the
    /// kernel console as the single sink. One call = one bounded chunk
    /// (kernel copies at most DIAGBUFSIZE per call — chunking is the
    /// caller's job). Wired 19-rs-external-interfaces.md.
    fn diag_write(&mut self, text: &str) -> Result<(), Errno>;
}

/// Scheduler face — scheduling a process has a *composite* transport target
/// in C: `sched_start` issues `sys_schedctl(SCHEDCTL_FLAG_KERNEL, ...)` when
/// the scheduler is the kernel, and a `SCHEDULING_START` message to the
/// scheduler endpoint otherwise (`sched_start.c:46-88`); `sched_stop` sends
/// `SCHEDULING_STOP` to the scheduler endpoint (`sched_stop.c:9-28`). In this
/// rewrite the scheduler is the SCHED server (06-stage-sched), so the two
/// calls form a face of their own, distinct from both [`SysApi`] and
/// [`PmApi`]. Wired 19.
pub trait SchedApi {
    fn sched_init_proc(&mut self, cfg: &SchedulerConfig) -> Result<Endpoint, Errno>;
    fn sched_stop(&mut self, scheduler: Endpoint, proc: Endpoint) -> Result<(), Errno>;
}

/// PM process-lifecycle face — the C calls that reach `_taskcall(PM_PROC_NR,
/// ...)` (or the POSIX routines PM serves). RS creates, inspects, execs,
/// signals, and reaps service processes through this face:
///
/// | Method | C call site |
/// |--------|-------------|
/// | [`PmApi::getnuid`] | `getnuid` → `PM_GETEPINFO` (lib/libsys/getepinfo.c:15-47) — manager.c:29 (04) |
/// | [`PmApi::getnpid`] | `getnpid` → `PM_GETEPINFO` (lib/libsys/getepinfo.c:29-33) — main.c:426 |
/// | [`PmApi::getprocnr`] | `getprocnr(pid, &endpoint)` → PM_GETEPINFO — manager.c:584 |
/// | [`PmApi::srv_fork`] | `PM_SRV_FORK` (lib/libsys/srv_fork.c) — manager.c:576 |
/// | [`PmApi::srv_execve`] | `srv_execve` — manager.c:634 (composite, see below) |
/// | [`PmApi::srv_kill`] | `srv_kill(rp->r_pid, SIGKILL)` — manager.c:469 (cleanup_service) |
/// | [`PmApi::waitpid`] | `waitpid(-1, &status, WNOHANG)` — request.c:1063 (do_sigchld's drain loop) |
/// | [`PmApi::setuid`] | `setuid(0)` — manager.c:656 |
pub trait PmApi {
    fn getnuid(&mut self, proc: Endpoint) -> Result<u32, Errno>;
    fn getnpid(&mut self, proc: Endpoint) -> Result<i32, Errno>;

    /// Resolves a pid to an endpoint. Wired 19.
    fn getprocnr(&mut self, pid: Pid) -> Result<Endpoint, Errno>;

    /// Forks a child service process.
    ///
    /// C: `srv_fork(uid, 0)` — manager.c:576. ARCH A-1: the no_std server has
    /// no libc `fork`; the external behavior (child created by PM) is
    /// preserved through the PM message face. Wired 19.
    fn srv_fork(&mut self, uid: u32, gid: u32) -> Result<Pid, Errno>;

    /// Execs a freshly forked child service process.
    ///
    /// C: `srv_execve(child_proc_nr_e, rp->r_exec, rp->r_exec_len,
    /// rpub->proc_name, rp->r_argv, environ)` — manager.c:634. The C
    /// implementation is a composite that runs *inside* RS
    /// (`minix3/minix/servers/rs/exec.c:21-64`): the libexec loader parses
    /// the ELF, segments are allocated and copied through kernel calls, then
    /// PM takes over the process (`libexec_pm_newexec` — exec.c:102) and the
    /// restart handshake closes the exec (`PM_EXEC_RESTART` — exec.c:127).
    /// The seam models the whole operation; `args`/`argc` carry the rebuilt
    /// argv layout (`rebuild_args`). Wired 19. ARCH: C also passes RS's own
    /// `environ`; this rewrite models no environment inheritance (deviation
    /// recorded in 10-rs-service-create.md §3).
    fn srv_execve(
        &mut self,
        proc: Endpoint,
        exec: &[u8],
        progname: &crate::service_slot::Label,
        args: &[u8],
        argc: usize,
    ) -> Result<(), Errno>;

    /// Asks PM to signal a service process (by pid through PM). Wired 19.
    fn srv_kill(&mut self, pid: Pid, signo: i32) -> Result<(), Errno>;

    /// Re-register the grant table with the kernel after a fork
    /// (C `cpf_reload` — safecopies.c:373-381;RS self-update main.c:464).
    /// Empty-table guard mirrors C's `if (grants)`.
    fn cpf_reload(&mut self) -> Result<(), Errno>;

    /// Non-blocking waitpid: the next exited child, if any.
    ///
    /// Wired 19; mock supplies canned children.
    fn waitpid(&mut self) -> Option<Pid>;

    /// Sets RS's own uid — the VFS non-blocking-fork workaround.
    ///
    /// C: `setuid(0)` — manager.c:656; the C comment marks it removable once
    /// VFS is fixed. Retained verbatim (PM face, wired 19).
    fn setuid(&mut self, uid: u32) -> Result<(), Errno>;
}

/// VM message face — the C calls that reach `_taskcall(VM_PROC_NR, ...)`.
pub trait VmApi {
    /// RS memory control on a process.
    ///
    /// C: `vm_memctl(ep, VM_RS_MEM_*, ...)` — manager.c:604, 612, 628, 636,
    /// 663, 680, 693. Wired 19.
    fn vm_memctl(
        &mut self,
        proc: Endpoint,
        req: VmRsMemReq,
        a: usize,
        b: usize,
    ) -> Result<(), Errno>;

    /// Sets the VM call mask of a process.
    ///
    /// C: `vm_set_priv(ep, &vm_call_mask[0], TRUE)` — manager.c:698
    /// (lib/libsys/vm_set_priv.c:7). Wired 19.
    fn vm_set_priv(
        &mut self,
        proc: Endpoint,
        vm_call_mask: CallMask,
        allow: bool,
    ) -> Result<(), Errno>;
}

/// RS's own IPC face — the receive/reply primitives of the main loop.
pub trait IpcApi {
    /// Blocking receive with the IPC status word.
    ///
    /// C: `sef_receive_status(ANY, &m, &ipc_status)` — main.c:826-833 via
    /// get_work (utility.c 全局接收原语, 06). The third element is the notify
    /// timestamp (ipc.h:1715) extracted safely — the union field cannot be
    /// read in no_std user code (R25 note). Wired 19.
    fn receive(
        &mut self,
        endpoint: Endpoint,
    ) -> Result<(minix_types::Message, crate::dispatch::IpcStatus, Clock), Errno>;

    /// Sends a reply message to a service.
    ///
    /// C: `reply(who, rp, m_ptr)` — utility.c:318-345: the handler-mutated
    /// request message (or a fresh one — `late_reply`, utility.c:332-349)
    /// is sent back with `m_type = result`. `payload` carries that message
    /// so payload replies (RS_LOOKUP's endpoint, request.c:1174) survive
    /// the seam.
    fn reply(
        &mut self,
        target: Endpoint,
        result: i32,
        payload: &minix_types::Message,
    ) -> Result<(), Errno>;

    /// Notifies a service — the do_period status ping (06/07).
    ///
    /// C: `ipc_notify(rpub->endpoint)` — request.c:1035; the kernel fills
    /// the notify timestamp (ipc.h:1715). Wired
    /// 19-rs-external-interfaces.md (production impl: `crate::trap_api::TrapKernelApi`).
    fn notify(&mut self, endpoint: Endpoint) -> Result<(), Errno>;

    /// Asynchronous non-blocking send.
    ///
    /// C: `rs_asynsend(rp, &m, 1)` — utility.c 全局异步原语；RS 用它发
    /// `RS_INIT` 初始化消息（utility.c:62，boot Step 2 与服务创建路径）。
    /// Wired 19-rs-external-interfaces.md (production impl: `crate::trap_api::TrapKernelApi`).
    fn asynsend(&mut self, endpoint: Endpoint, message: &minix_types::Message)
    -> Result<(), Errno>;

    /// Copies a request payload from the caller's address space.
    ///
    /// C: `sys_datacopy(src_e, addr, SELF, dst, len)` — manager.c:141
    /// (`copy_rs_start`) / manager.c:160 (`copy_label`). Wired
    /// 19-rs-external-interfaces.md (production impl: `crate::trap_api::TrapKernelApi`).
    fn safecopy_from(&mut self, source: Endpoint, addr: usize, buf: &mut [u8])
    -> Result<(), Errno>;

    /// Copies RS-owned bytes into a requester's address space.
    ///
    /// C: `sys_datacopy(SELF, src, dst_e, dst_addr, len)` — request.c:1122
    /// (do_getsysinfo's table copy-out) and request.c:862 (grant-backed
    /// state data). Production impl: `crate::trap_api::TrapKernelApi`.
    fn safecopy_to(&mut self, dest: Endpoint, addr: usize, buf: &[u8]) -> Result<(), Errno>;

    /// Resolves a service label to its endpoint through the Data Store.
    ///
    /// C: `ds_retrieve_label_endpt` — the state-data eval/filter labels
    /// (request.c:802, manager.c:265) and the recovery label lookups. The
    /// store is reached by the DS taskcall leg (19 号 DS 缝);`None` when
    /// the label is unknown. Production impl: `DsClient` in
    /// `crate::trap_api`.
    fn ds_lookup_by_label(&mut self, label: &str) -> Option<Endpoint>;

    /// Grants READ access to RS's own memory for the update instance
    /// ([ARCH: request.c:815-838] `cpf_grant_direct(rpub->endpoint, …,
    /// CPF_READ)` — the state-data eval/filter bytes travel by grant).
    /// The grant stays alive until revoked or the process dies.
    fn grant_read(&mut self, dest: Endpoint, addr: u64, len: u64) -> Result<GrantId, Errno>;

    /// Revokes a grant (C: `cpf_revoke` — the update cleanup half).
    fn grant_revoke(&mut self, grant: GrantId);
    /// Requests a service to prepare for (or cancel) its live update.
    ///
    /// C: `request_prepare_update_service` — update.c:185-233: m_type
    /// `RS_LU_PREPARE`, payload `m_rs_update{flags, state_data_gid,
    /// state}` delivered via `rs_asynsend`.
    fn lu_request_prepare(
        &mut self,
        ep: Endpoint,
        flags: i32,
        state_data_gid: Option<GrantId>,
        state: i32,
    ) -> Result<(), Errno>;

    /// Asks VM to prepare a new instance based on an old one.
    ///
    /// C: `vm_prepare` — libsys vm_prepare.c: `_taskcall(VM,
    /// VM_RS_PREPARE, {src, dst, flags})`.
    fn vm_prepare(&mut self, src: Endpoint, dst: Endpoint, flags: i32) -> Result<(), Errno>;

    /// Blocking send-and-receive to an arbitrary peer (19 号:devman
    /// unbind 的 DEVMAN_UNBIND 交换等)。reply 覆盖 msg,m_type 携带
    /// 结果码(C ipc_sendrec 语义)。
    fn sendrec_to(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), Errno>;
}

/// The external boundary of the RS server — the union of the five domain
/// faces ([`SysApi`]/[`SchedApi`]/[`PmApi`]/[`VmApi`]/[`IpcApi`]).
///
/// C's libsys free functions reach four different message targets (kernel,
/// scheduler endpoint, PM, VM) plus RS's own IPC. The single `KernelApi` name
/// keeps every call site unchanged (`&mut dyn KernelApi` — supertrait methods
/// are callable through the composite object) while the domain traits make
/// the target of each call explicit and let the 19 wiring implement the faces
/// one transport at a time. This replaces the former monolithic 22-method
/// trait (R9, todo §14/§18.10 E-2).
pub trait KernelApi: SysApi + SchedApi + PmApi + VmApi + IpcApi {}
impl<T> KernelApi for T where T: SysApi + SchedApi + PmApi + VmApi + IpcApi {}

/// VM RS-memory-control requests.
///
/// C: `VM_RS_MEM_*` — `minix3/minix/include/minix/com.h:741-745`.
/// 10/16 use these; `HeapPrealloc`/`MapPrealloc`/`GetPreallocMap` are
/// live-update faces (16-rs-live-update.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmRsMemReq {
    /// Pin process memory. C: `VM_RS_MEM_PIN` — com.h:741.
    Pin = 0,
    /// Make a VM instance. C: `VM_RS_MEM_MAKE_VM` — com.h:742.
    MakeVm = 1,
    /// Preallocate heap regions. C: `VM_RS_MEM_HEAP_PREALLOC` — com.h:743.
    HeapPrealloc = 2,
    /// Preallocate mmapped regions. C: `VM_RS_MEM_MAP_PREALLOC` — com.h:744.
    MapPrealloc = 3,
    /// Get preallocated mmapped regions. C: `VM_RS_MEM_GET_PREALLOC_MAP` — com.h:745.
    GetPreallocMap = 4,
}

/// Fail-closed external boundary: every method of every domain face returns
/// `ENOSYS`.
///
/// Selected until the `minix-sys` wiring lands (19-rs-external-interfaces.md).
/// RS is a root system process — a panic is a system-wide outage (the kernel
/// does not restart RS, `RSYS_F`); returning `Err(Errno::ENOSYS)` fails closed
/// while keeping the server alive so the wiring gap is visible at the call
/// site, not as a process crash (T2, 19-rs-external-interfaces.md).
pub struct UnimplementedKernelApi;

impl SysApi for UnimplementedKernelApi {
    fn get_machine(&mut self) -> Result<Machine, Errno> {
        Err(Errno::ENOSYS)
    }
    fn get_hz(&mut self) -> Result<u32, Errno> {
        Err(Errno::ENOSYS)
    }
    fn get_ticks(&mut self) -> Result<Clock, Errno> {
        Err(Errno::ENOSYS)
    }
    fn privctl(
        &mut self,
        _proc: Endpoint,
        _op: PrivCtlOp,
        _priv_: Option<&Privilege>,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn getpriv(&mut self, _proc: Endpoint) -> Result<Privilege, Errno> {
        Err(Errno::ENOSYS)
    }
    fn setalarm(&mut self, _delay_ticks: u32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn sys_kill(&mut self, _proc: Endpoint, _signo: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn sys_update(
        &mut self,
        _src: Endpoint,
        _dst: Endpoint,
        _flags: crate::service_slot::SysFlags,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    // E-11/E9: sys_diagctl_stacktrace's real transport is 19's wiring; fail-closed.
    fn diagctl_stacktrace(&mut self, _target: Endpoint) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    // [ARCH: A-6] the seam is declared here; the producing transport lands
    // with the KernelApi swap (19's wiring) — fail-closed until then.
    fn diag_write(&mut self, _text: &str) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

impl SchedApi for UnimplementedKernelApi {
    fn sched_init_proc(&mut self, _cfg: &SchedulerConfig) -> Result<Endpoint, Errno> {
        Err(Errno::ENOSYS)
    }
    fn sched_stop(&mut self, _scheduler: Endpoint, _proc: Endpoint) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

impl PmApi for UnimplementedKernelApi {
    fn getnuid(&mut self, _proc: Endpoint) -> Result<u32, Errno> {
        Err(Errno::ENOSYS)
    }
    fn getnpid(&mut self, _proc: Endpoint) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }
    fn getprocnr(&mut self, _pid: Pid) -> Result<Endpoint, Errno> {
        Err(Errno::ENOSYS)
    }
    fn srv_fork(&mut self, _uid: u32, _gid: u32) -> Result<Pid, Errno> {
        Err(Errno::ENOSYS)
    }
    fn srv_execve(
        &mut self,
        _proc: Endpoint,
        _exec: &[u8],
        _progname: &crate::service_slot::Label,
        _args: &[u8],
        _argc: usize,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn srv_kill(&mut self, _pid: Pid, _signo: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cpf_reload(&mut self) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn waitpid(&mut self) -> Option<Pid> {
        None
    }
    fn setuid(&mut self, _uid: u32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

impl VmApi for UnimplementedKernelApi {
    fn vm_memctl(
        &mut self,
        _proc: Endpoint,
        _req: VmRsMemReq,
        _a: usize,
        _b: usize,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn vm_set_priv(
        &mut self,
        _proc: Endpoint,
        _vm_call_mask: CallMask,
        _allow: bool,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

impl IpcApi for UnimplementedKernelApi {
    fn receive(
        &mut self,
        _endpoint: Endpoint,
    ) -> Result<(minix_types::Message, crate::dispatch::IpcStatus, Clock), Errno> {
        Err(Errno::ENOSYS)
    }
    fn reply(
        &mut self,
        _target: Endpoint,
        _result: i32,
        _payload: &minix_types::Message,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn notify(&mut self, _endpoint: Endpoint) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    // E-11/E9: rs_asynsend's real transport is 19's wiring; fail-closed.
    fn asynsend(
        &mut self,
        _endpoint: Endpoint,
        _message: &minix_types::Message,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn safecopy_from(
        &mut self,
        _source: Endpoint,
        _addr: usize,
        _buf: &mut [u8],
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn safecopy_to(&mut self, _dest: Endpoint, _addr: usize, _buf: &[u8]) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    fn ds_lookup_by_label(&mut self, _label: &str) -> Option<Endpoint> {
        None
    }

    fn grant_read(&mut self, _dest: Endpoint, _addr: u64, _len: u64) -> Result<GrantId, Errno> {
        Err(Errno::ENOSYS)
    }

    fn grant_revoke(&mut self, _grant: GrantId) {}

    fn lu_request_prepare(
        &mut self,
        _ep: Endpoint,
        _flags: i32,
        _state_data_gid: Option<GrantId>,
        _state: i32,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn vm_prepare(&mut self, _src: Endpoint, _dst: Endpoint, _flags: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sendrec_to(&mut self, _dest: Endpoint, _msg: &mut Message) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

/// Boot image + boot tables bundle.
///
/// C: `sys_getimage()` copy of the kernel `boot_image[]` (main.c:196) plus the
/// RS-owned priv/sys/dev tables (table.c). The image is injected at
/// construction (ARCH A-13); the three tables are `'static` constants.
#[derive(Debug, Clone, Copy)]
pub struct BootTables<'a> {
    /// C: `boot_image[]` copied by `sys_getimage` — main.c:196.
    pub image: &'a [BootImage],
    /// C: `boot_image_priv_table` — table.c:15-30.
    pub priv_table: &'static [BootImagePriv],
    /// C: `boot_image_sys_table` — table.c:33-42.
    pub sys_table: &'static [BootImageSys],
    /// C: `boot_image_dev_table` — table.c:45-50.
    pub dev_table: &'static [BootImageDev],
}

impl<'a> BootTables<'a> {
    /// Boot tables with the RS-owned static tables and the given image.
    pub const fn new(image: &'a [BootImage]) -> Self {
        Self {
            image,
            priv_table: BOOT_IMAGE_PRIV_TABLE,
            sys_table: BOOT_IMAGE_SYS_TABLE,
            dev_table: BOOT_IMAGE_DEV_TABLE,
        }
    }

    /// Placeholder boot tables（测试/宿主开发用值）。
    ///
    /// Mirrors VM's `BootParams::placeholder()` strategy
    /// (02-stage-vm/01-vm-init-main.md §4.1): a minimal but valid boot image
    /// so the production binary has a legal boot input. 生产 main 用
    /// [`BootTables::acquire_from`] 换内核 GET_IMAGE 真值。
    pub fn placeholder() -> BootTables<'static> {
        BootTables::new(BOOT_IMAGE_PLACEHOLDER)
    }

    /// 真实启动获取（D-02 消费半，S42 ② boot 真值链）。
    ///
    /// C main.c:196 `sys_getimage(image)`：内核 GET_IMAGE 的应答是 boot
    /// 真值（do_getinfo.c:86-90 的 `struct boot_image[NR_BOOT_PROCS]`
    /// 整表），按 minix-types wire 权威（`BootImageStruct`，40 字节 × 17）
    /// 解码为建模行。失败回 `Err(原始 errno)`——C 对应 panic
    /// （main.c:196-198），生产 main 据此停车。
    ///
    /// 生命周期：RS 常驻，boot 表随进程同寿，`Box::leak` 换 `'static`
    /// 借用是单进程服务器的诚实形态（C 的 `image[]` 也是静态存储）。
    pub fn acquire_from(
        transport: &impl minix_sys::syscall::KernelCallTransport,
    ) -> Result<BootTables<'static>, i32> {
        let mut wire =
            [minix_types::BootImageStruct::default(); minix_types::NR_BOOT_PROCS];
        // SAFETY: `[BootImageStruct; N]` 是 repr(C) 的 POD 数组
        //（minix-types boot_image.rs 布局冻结测试钉值），整块按字节
        // 可写；内核 GET_IMAGE 按同一布局填充。
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(
                wire.as_mut_ptr() as *mut u8,
                core::mem::size_of::<
                    [minix_types::BootImageStruct; minix_types::NR_BOOT_PROCS],
                >(),
            )
        };
        minix_sys::syscall::sys_getimage(transport, bytes)?;

        let image: alloc::vec::Vec<BootImage> = wire
            .iter()
            .map(BootImage::from_wire)
            .collect();
        Ok(BootTables::new(alloc::boxed::Box::leak(image.into_boxed_slice())))
    }

    /// Validates that image and priv tables describe the same system services.
    ///
    /// C: `nr_image_srvs != nr_image_priv_srvs → panic` — main.c:225-227.
    /// Kernel tasks (negative endpoint slots) are excluded on both sides.
    /// R17: every image row must also satisfy `endpoint.slot() == proc_nr` —
    /// the kernel derives boot endpoints as `_ENDPOINT(0, proc_nr)`, so a
    /// hand-edited row with a mismatched pair would boot the wrong endpoint.
    pub fn validate_tables(&self) -> Result<(), BootError> {
        if self.image.iter().any(|ip| ip.endpoint.slot() != ip.proc_nr) {
            return Err(BootError::EndpointMismatch); // boot protocol violation (R17)
        }
        let image_srvs = self
            .image
            .iter()
            .filter(|ip| !ip.endpoint.is_kernel_task())
            .count();
        let priv_srvs = self
            .priv_table
            .iter()
            .filter(|e| !e.endpoint.is_kernel_task())
            .count();
        if image_srvs != priv_srvs {
            return Err(BootError::CountMismatch); // C panics (main.c:226)
        }
        Ok(())
    }
}

/// Boot-parameter lookup over the GET_MONPARAMS buffer.
///
/// C `env_get_param` 的读半（libsys env_get_prm.c:26-84 的
/// `find_key` 扫描对位）：参数缓冲是 NUL 分隔的 `key=value` 条目流，
/// 按条目步进，key 前缀相等且紧跟 `=` 才算命中（strncmp + `'='`
/// 界定，`env_get_prm.c:73-76`）——`rs_verbose_x=…` 不会命中
/// `rs_verbose`。返回值切片（`=` 之后、条目 NUL 之前）。
///
/// 这是 boot 参数面的读取半：GET_MONPARAMS 由内核 GET_MONPARAMS 臂
/// 提供（do_getinfo.c:143-146，S42 批一真值链）。pre-VFS 阶段这是
/// 服务读 boot 参数的唯一通道；`/etc/rc` 等盘上配置面随 T3/T4 通电
/// 后由 init/RS 同源读取（edge4 §6 OQ-3 裁决）——两条通道同源衔接。
pub fn find_boot_param<'a>(params: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut envp = params;
    while let Some(&first) = envp.first() {
        if first == 0 {
            break; // C: for (envp = params; *envp != 0;) — NUL 即表尾
        }
        let key_matched = envp.len() > key.len()
            && envp.starts_with(key)
            && envp[key.len()] == b'=';
        if key_matched {
            let rest = &envp[key.len() + 1..];
            let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
            return Some(&rest[..end]);
        }
        // 跳过本条到 NUL（C: while (*envp++ != 0)）；缓冲中途截断
        // （无 NUL）则无更多条目。
        let next = envp.iter().position(|&b| b == 0)?;
        envp = &envp[next + 1..];
    }
    None
}

/// C main.c:171 — `env_parse("rs_verbose", "d", 0, &rs_verbose, 0, 1)`:
/// RS verbose 开关的 boot 参数解析。env_parse 的 "d" 数值域只认十进制
/// 串；缺省/解析失败落 0（verbose off），值域 [0,1] 外亦按失败计。
pub fn parse_rs_verbose(params: &[u8]) -> bool {
    matches!(find_boot_param(params, b"rs_verbose"), Some(b"1"))
}

/// Builds a placeholder `BootImage` entry with a padded name.
const fn boot_img(proc_nr: i32, endpoint: Endpoint, name: &str) -> BootImage {
    let mut proc_name = [0u8; 16];
    let bytes = name.as_bytes();
    // R17: clamp to the 16-byte field (C truncates with `strlcpy` when the
    // kernel fills `proc_name`); an over-long name must not panic const
    // evaluation with an out-of-range index.
    let n = if bytes.len() > 16 { 16 } else { bytes.len() };
    let mut i = 0;
    while i < n {
        proc_name[i] = bytes[i];
        i += 1;
    }
    BootImage {
        proc_nr,
        proc_name,
        endpoint,
        start_addr: 0,
        len: 0,
    }
}

/// Minimal valid boot image for `placeholder()`.
///
/// Contains the full boot-order service set (RS → ... → INIT) so the image and
/// priv tables describe the same 12 services (main.c:225-227 count check).
/// The exact `start_addr`/`len` are placeholders (kernel ELF blobs are
/// reported by `sys_getimage` at runtime).
static BOOT_IMAGE_PLACEHOLDER: &[BootImage] = &[
    boot_img(2, Endpoint::RS, "rs"),
    boot_img(8, Endpoint::VM, "vm"),
    boot_img(0, Endpoint::PM, "pm"),
    boot_img(4, Endpoint::SCHED, "sched"),
    boot_img(1, Endpoint::VFS, "vfs"),
    boot_img(6, Endpoint::DS, "ds"),
    boot_img(5, Endpoint::TTY, "tty"),
    boot_img(3, Endpoint::MEM, "memory"),
    boot_img(7, Endpoint::MIB, "mib"),
    boot_img(9, Endpoint::PFS, "pfs"),
    boot_img(10, Endpoint::MFS, "mfs"),
    boot_img(11, Endpoint::INIT, "init"),
];

/// Failure modes of the boot table lookups.
///
/// C: `panic("boot image table lookup failed")` / `panic("boot image priv table
/// lookup failed")` — main.c:731, 746. Sys/dev lookups never fail: they fall
/// back to the default entry (main.c:753-777).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupError {
    /// Image table lookup failed (C panic, main.c:731).
    ImageTable,
    /// Priv table lookup failed (C panic, main.c:746).
    PrivTable,
}

/// Failure modes of the boot sequence itself (E-6, T3 残留).
///
/// C has a single failure mode for boot — `panic()` (main.c:226, 427-429;
/// lookup panics main.c:731/746) — because a half-booted RS cannot run. The
/// Rust shell keeps the fail-closed `Err` contract (T2) but separates two
/// families that the flat `ENOSYS` used to conflate: **kernel-call failures**
/// ([`BootError::Kernel`] — e.g. `ENOSYS` while the 19 wiring is pending)
/// and **boot invariant violations** (the rest — the boot tables or the
/// kernel's answer are unusable, the C panic family).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootError {
    /// A boot table lookup missed. C: `panic("boot image table lookup
    /// failed")` / priv variant — main.c:731/746.
    Lookup(LookupError),
    /// The image and priv boot tables disagree about the system-service set.
    /// C: `panic` — main.c:225-227.
    CountMismatch,
    /// An image row's endpoint is not derived from its proc_nr (R17) — the
    /// kernel derives boot endpoints as `_ENDPOINT(0, proc_nr)`, so this row
    /// would boot the wrong endpoint. C has no equivalent check (the kernel
    /// builds its own table); the check exists because the Rust placeholder
    /// tables are hand-written.
    EndpointMismatch,
    /// `getnpid` returned a non-positive pid for a boot service. C:
    /// `panic("unable to get pid")` — main.c:427-429.
    InvalidPid(Pid),
    /// A kernel call failed during boot. Distinct from the invariant family:
    /// with the 19 wiring pending this is `Kernel(Errno::ENOSYS)` — "the
    /// mechanism does not exist yet", not "the boot data is corrupt".
    Kernel(Errno),
}

impl From<Errno> for BootError {
    fn from(e: Errno) -> Self {
        BootError::Kernel(e)
    }
}

impl From<LookupError> for BootError {
    fn from(e: LookupError) -> Self {
        BootError::Lookup(e)
    }
}

impl From<BootError> for Errno {
    fn from(e: BootError) -> Self {
        match e {
            // Kernel-call failures keep their errno (the wire face of the
            // failed call).
            BootError::Kernel(errno) => errno,
            // C has no errno for boot invariant violations — it panics
            // (main.c:225-227/427-429). EINVAL keeps the wire face honest:
            // `ENOSYS` stays reserved for "mechanism not wired" (T2), while
            // "boot data unusable" reads as invalid input.
            BootError::Lookup(_)
            | BootError::CountMismatch
            | BootError::EndpointMismatch
            | BootError::InvalidPid(_) => Errno::EINVAL,
        }
    }
}

/// Looks up an entry in the boot image table.
///
/// C: `boot_image_info_lookup(..., ip, ...)` image branch — main.c:723-733.
pub fn lookup_image(image: &[BootImage], endpoint: Endpoint) -> Result<&BootImage, LookupError> {
    image
        .iter()
        .find(|ip| ip.endpoint == endpoint)
        .ok_or(LookupError::ImageTable)
}

/// Looks up an entry in the boot image priv table.
///
/// C: `boot_image_info_lookup(..., pp, ...)` priv branch — main.c:735-748.
pub fn lookup_priv(
    table: &[BootImagePriv],
    endpoint: Endpoint,
) -> Result<&BootImagePriv, LookupError> {
    table
        .iter()
        .find(|e| e.endpoint == endpoint)
        .ok_or(LookupError::PrivTable)
}

/// Looks up an entry in the boot image sys table, falling back to the default.
///
/// C: `boot_image_info_lookup(..., sp, ...)` sys branch — main.c:753-762
/// ("accept the default entry").
pub fn lookup_sys(table: &[BootImageSys], endpoint: Endpoint) -> &BootImageSys {
    table
        .iter()
        .find(|e| e.endpoint == endpoint)
        .unwrap_or(&DEFAULT_SYS)
}

/// Looks up an entry in the boot image dev table, falling back to the default.
///
/// C: `boot_image_info_lookup(..., dp, ...)` dev branch — main.c:768-777.
pub fn lookup_dev(table: &[BootImageDev], endpoint: Endpoint) -> &BootImageDev {
    table
        .iter()
        .find(|e| e.endpoint == endpoint)
        .unwrap_or(&DEFAULT_DEV)
}

/// Global init descriptor carried from boot to the ready protocol.
///
/// C: `rinit` (`servers/rs/glo.h`) — full semantics in 12-rs-init-run.md.
/// This module only carries the grant creation point (main.c:185);
/// `init_service` (12) consumes `rproctab_gid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RinitState {
    /// C: `rinit.rproctab_gid = cpf_grant_direct(ANY, rprocpub, ...)` — main.c:185.
    ///
    /// Filled by `step0_prepare` (the grant covers the table's wire mirror);
    /// `None` = grant not created yet (fresh `BootInit`). Consumption:
    /// 12-rs-init-run.md — the 12 wiring copies **this** field into
    /// `InitMessage::rproctab_gid` (ready.rs; R32.2: one global, one payload
    /// — the wiring is the only bridge, keep them from drifting into two
    /// independent sources).
    pub rproctab_gid: Option<u32>,
}

/// The 4-step boot state machine.
///
/// C: `sef_cb_init_fresh` — main.c:158-494. [`BootInit::init_fresh`] is the
/// single entry; the four steps are private and strictly ordered.
pub struct BootInit<'a> {
    tables: BootTables<'a>,
    /// C: `machine` — main.c:53 (`sys_getmachine`). Startup-time snapshot of
    /// the machine topology (N3): `check_request` resolves `RS_CPU_BSP` /
    /// cpu > processors_count against it (request.c:1286-1296).
    machine: Machine,
    /// C: `rinit` — main.c:185 (grant creation point).
    rinit: RinitState,
    /// C: `rproc[]`/`rprocpub[]` + `rproc_ptr[]` — the service table with
    /// endpoint→slot index (02-rs-process-table.md §4.2, ARCH A-4).
    table: RProcTable,
    /// C: `shutting_down` — main.c:193.
    shutting_down: bool,
    /// C: `system_hz` — main.c:181.
    system_hz: u32,
    /// Number of init-ready messages still expected (Step 2 → Step 3).
    /// C: `nr_uncaught_init_srvs` — main.c:349-406.
    nr_uncaught_init_srvs: usize,
}

impl<'a> BootInit<'a> {
    /// Creates a fresh boot machine with the given boot tables.
    pub fn new(tables: BootTables<'a>) -> Self {
        Self {
            tables,
            machine: Machine::default(),
            rinit: RinitState::default(),
            table: RProcTable::new(),
            shutting_down: false,
            system_hz: 0,
            nr_uncaught_init_srvs: 0,
        }
    }

    /// Hands the runtime state over to the server (T1).
    ///
    /// C: the boot state IS the runtime state — `rproc[]`, `system_hz`,
    /// `shutting_down` and `rinit` are `glo.h` globals that keep living after
    /// boot; there is no separate post-boot copy. Consuming `self` makes the
    /// transition explicit: after `init_fresh` the main loop owns the state,
    /// and a boot-phase value can no longer be mutated post-handover.
    pub fn into_state(self) -> crate::ServerState<'a> {
        crate::ServerState {
            tables: self.tables,
            machine: self.machine,
            rinit: self.rinit,
            table: self.table,
            shutting_down: self.shutting_down,
            system_hz: self.system_hz,
            nr_uncaught_init_srvs: self.nr_uncaught_init_srvs,
            update: Default::default(),
        }
    }

    /// Runs the 4-step boot. C: `sef_cb_init_fresh` — main.c:158-494.
    ///
    /// Step order is fixed and cannot be reordered from outside:
    /// `step1_set_attrs` → `step2_allow_run` → `step3_catch_init_ready` →
    /// `step4_finish`.
    pub fn init_fresh(&mut self, sys: &mut dyn KernelApi) -> Result<(), BootError> {
        self.step0_prepare(sys)?;
        self.step1_set_attrs(sys)?;
        self.step2_allow_run(sys)?;
        self.step3_catch_init_ready(sys)?;
        self.step4_finish(sys)?;
        Ok(())
    }

    /// Step 0 — preparation: config, frequency, grant, resets, image copy.
    ///
    /// C: main.c:178-237.
    fn step0_prepare(&mut self, sys: &mut dyn KernelApi) -> Result<(), BootError> {
        // C: sys_getmachine(&machine) — main.c:53. Fetched once at startup
        // (before the main loop), not per request — `check_request`'s CPU
        // resolution (request.c:1286-1296) reads this snapshot (N3).
        self.machine = sys.get_machine()?;
        // C: env_parse("rs_verbose", ...) — main.c:179 (config injection; A-11).
        // C: sys_getinfo(GET_HZ, &system_hz, ...) — main.c:181-183.
        self.system_hz = sys.get_hz()?;
        // C: RUPDATE_INIT() + shutting_down = FALSE — main.c:192-193.
        //   The update descriptor (16) is reset at construction
        //   (`RupdateState::default()` in 16's module); we reset the flag here.
        self.shutting_down = false;
        // C: sys_getimage(image) — main.c:196: the image is injected as
        //   `self.tables.image` (ARCH A-13).
        // C: count comparison — main.c:225-227.
        self.tables.validate_tables()?;
        // C: process table reset — main.c:230-237. Rebuilt from scratch
        //   (full field semantics: 02-rs-process-table.md §2.12).
        self.table = RProcTable::new();
        // C: rinit.rproctab_gid = cpf_grant_direct(ANY, rprocpub,
        //   sizeof(rprocpub), CPF_READ) — main.c:185-189; VM reads the table
        //   through this grant at RS_INIT (main.c:246). A failed creation
        //   stops the boot (C panics, main.c:186-189; here the Errno reaches
        //   main() as the Kernel BootError, same fatal stop). Order note: C
        //   grants *before* the table reset because the granted `rprocpub`
        //   is a static array the reset cannot move; the Rust grant covers
        //   the table's derived wire mirror, so the reset (which allocates
        //   that mirror) must come first — the grant pins the mirror's
        //   address, step 1 fills its rows (`sync_pub_wire`).
        let wire = self.table.pub_wire_bytes();
        let gid = sys.grant_read(Endpoint::ANY, wire.as_ptr() as u64, wire.len() as u64)?;
        self.rinit.rproctab_gid = Some(gid as u32);
        Ok(())
    }

    /// The global init descriptor prepared at boot.
    ///
    /// C: `rinit` — main.c:185. Consumed by the ready protocol
    /// (`init_service`, 12-rs-init-run.md).
    pub fn rinit(&self) -> &RinitState {
        &self.rinit
    }

    /// Step 1 — establish priv/sys/dev attributes for every boot service.
    ///
    /// C: main.c:244-346. RS/VM skip `SYS_PRIV_SET_SYS` (main.c:285-291) —
    /// they are already running. The priv-structure construction itself
    /// (send mask, call masks, sig mgr) belongs to 03/05.
    fn step1_set_attrs(&mut self, sys: &mut dyn KernelApi) -> Result<(), BootError> {
        let tables = self.tables;
        for (slot_nr, priv_) in tables.priv_table.iter().enumerate() {
            if priv_.endpoint.is_kernel_task() {
                continue; // C: iskerneln skip — main.c:248-250
            }
            // C: boot_image_info_lookup(ep, image, &ip, NULL, &sys, &dev) — main.c:253-254.
            let ip = lookup_image(tables.image, priv_.endpoint)?;
            let sys_ = lookup_sys(tables.sys_table, priv_.endpoint);
            let dev = lookup_dev(tables.dev_table, priv_.endpoint);

            // C: boot Step 1 priv assembly — main.c:264-296 (03-rs-privilege.md §4.2).
            // N10: `priv_.flags` is already the typed `PrivFlags` (table.rs);
            // the old `from_bits_truncate(flags as u16)` silently dropped
            // unknown high bits (C main.c:269 assigns s_flags verbatim).
            let mut privilege = Privilege::boot_priv(priv_.flags, priv_.endpoint.slot());
            // C: sys_privctl(SYS_PRIV_SET_SYS) — RS/VM exception — main.c:285-291.
            if priv_.endpoint != Endpoint::RS && priv_.endpoint != Endpoint::VM {
                sys.privctl(priv_.endpoint, PrivCtlOp::SetSys, Some(&privilege))?;
            }
            // C: sys_getpriv — main.c:293-296. The kernel may have rewritten
            //   the structure (id/proc_nr/pending); take the synced version.
            privilege = sys.getpriv(priv_.endpoint)?;
            // C: rp->r_alive_tm = getticks() — main.c:333 (S2; 07 heartbeat).
            let ticks = sys.get_ticks()?;

            // C: slot population + activation — main.c:258-345. The boot slot
            //   index equals the priv-table index (main.c:255); mechanism
            //   ownership: 02-rs-process-table.md §4.2 `activate_boot_slot`.
            // C: strlcpy(rpub->proc_name, ip->proc_name) — main.c:313.
            let proc_name = crate::service_slot::Label::from_bytes(&ip.proc_name);
            self.table.activate_boot_slot(
                SlotId::new(slot_nr),
                priv_.endpoint,
                proc_name,
                priv_,
                sys_,
                dev,
                privilege,
                ticks,
            )?;
        }
        // C fills `rprocpub[i]` in place during registration (main.c:258-345);
        // the wire mirror the rproctab grant covers is derived, so refresh it
        // once the boot rows exist — VM reads it when RS_INIT arrives (step 2).
        self.table.sync_pub_wire();
        Ok(())
    }

    /// Step 2 — allow every boot service to run.
    ///
    /// C: main.c:348-399. RS/VM go through `init_service` (12) directly;
    /// other services get `sched_init_proc` + `SYS_PRIV_ALLOW` first.
    // NK4-A Task C 取证（task1-close 裁决删除）：分辨 RS null-deref 两假设
    // ——(1) exec 期 .rodata（priv_table 表）填充错位/漏填 → endpoint 读出
    // 垃圾；(2) self.table 基址本身为 NULL。打印切片基址+长度、每项
    // endpoint 原值、table 基址。mock 门本 crate 未声明 feature，放行零新增警告。
    #[allow(unexpected_cfgs)]
    fn step2_allow_run(&mut self, sys: &mut dyn KernelApi) -> Result<(), BootError> {
        let tables = self.tables;
        let mut nr_uncaught_init_srvs = 0usize;

        // NK4-A Task C 取证（task1-close 裁决删除）：表基址/长度 + 每项 endpoint。
        #[cfg(not(feature = "mock"))]
        {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static P2_LOG: AtomicUsize = AtomicUsize::new(0);
            if P2_LOG.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                let _ = minix_sys::syscall::sys_diagctl_write(
                    &minix_sys::syscall::DirectKernelCallTransport,
                    &alloc::format!(
                        "nk4a: rs-step2 pt={:p} len={} tbl={:p}\n",
                        tables.priv_table.as_ptr(),
                        tables.priv_table.len(),
                        &self.table as *const RProcTable,
                    ),
                );
            }
        }

        for priv_ in tables.priv_table {
            // NK4-A Task C 取证（task1-close 裁决删除）：每入口 endpoint 原值。
            #[cfg(not(feature = "mock"))]
            {
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                static P2E_LOG: AtomicUsize = AtomicUsize::new(0);
                if P2E_LOG.fetch_add(1, AtomicOrd::Relaxed) < 16 {
                    let _ = minix_sys::syscall::sys_diagctl_write(
                        &minix_sys::syscall::DirectKernelCallTransport,
                        &alloc::format!("nk4a: rs-step2 ep={:#x} slot={}\n",
                            priv_.endpoint.0, priv_.endpoint.slot()),
                    );
                }
            }
            if priv_.endpoint.is_kernel_task() {
                continue; // C: iskerneln skip — main.c:354-356
            }
            let id = self.table.endpoint_slot(priv_.endpoint).unwrap_or_else(|| {
                panic!(
                    "boot service slot missing at step 2: endpoint {:#x} slot {}; indexed={:?}",
                    priv_.endpoint.0,
                    priv_.endpoint.slot(),
                    self.table.indexed_endpoints()
                )
            });
            let init_flags = self.table.get(id).priv_.init_flags;
            let gid = self.rinit.rproctab_gid;
            let ticks = sys.get_ticks()?;

            // RS/VM are already running as we speak — C: main.c:362-373.
            // init_service marks them initializing; for RS itself it stops
            // right there (ROOT_SYS_PROC early return — utility.c:29-31),
            // VM additionally receives the RS_INIT message and counts.
            if priv_.endpoint == Endpoint::RS || priv_.endpoint == Endpoint::VM {
                // C: init_service(rp, SEF_INIT_FRESH, r_priv.s_init_flags)
                // — main.c:365-367. A boot slot has no old incarnation
                // (old_endpoint NONE, prepare_state SEF_LU_STATE_NULL —
                // utility.c:34-42 via the r_old_rp/r_prev_rp NULL path).
                // The async-send seam: the RS_INIT message leaves through
                // IpcApi::asynsend (E-11 — the wire is 19's; the decision
                // is live here). C: rs_asynsend(rp, &m, 0) — utility.c:62.
                let mut asynsend = |ep: Endpoint, msg: &crate::ready::InitMessage| {
                    sys.asynsend(ep, &msg.encode_message())
                };
                crate::service_create::init_service(
                    self.table.get_mut(id),
                    crate::service_create::InitSpec {
                        old_endpoint: None,
                        init_type: crate::sef::SefInitType::Fresh,
                        init_flags,
                        gid,
                        prepare_state: crate::live_update::SEF_LU_STATE_NULL,
                    },
                    ticks,
                    &mut asynsend,
                )?;
                if priv_.endpoint != Endpoint::RS {
                    // VM will still send an RS_INIT message — main.c:369-370.
                    nr_uncaught_init_srvs += 1;
                }
                continue;
            }
            // C: sched_init_proc — main.c:376; sys_privctl(SYS_PRIV_ALLOW) — main.c:379.
            // NK4-A Task C 取证（task1-close 裁决删除）：非 RS/VM 分支逐步打点，
            // 定位把表基址算成 0 的确切子步。
            #[cfg(not(feature = "mock"))]
            {
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                static P2S_LOG: AtomicUsize = AtomicUsize::new(0);
                if P2S_LOG.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                    let _ = minix_sys::syscall::sys_diagctl_write(
                        &minix_sys::syscall::DirectKernelCallTransport,
                        &alloc::format!("nk4a: rs-pm pre-sched ep={:#x}\n", priv_.endpoint.0),
                    );
                }
            }
            sys.sched_init_proc(&SchedulerConfig::boot_defaults(priv_.endpoint))?;
            #[cfg(not(feature = "mock"))]
            {
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                static P2P_LOG: AtomicUsize = AtomicUsize::new(0);
                if P2P_LOG.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                    let _ = minix_sys::syscall::sys_diagctl_write(
                        &minix_sys::syscall::DirectKernelCallTransport,
                        "nk4a: rs-pm post-sched pre-privctl\n",
                    );
                }
            }
            sys.privctl(priv_.endpoint, PrivCtlOp::Allow, None)?;
            #[cfg(not(feature = "mock"))]
            {
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                static P2A_LOG: AtomicUsize = AtomicUsize::new(0);
                if P2A_LOG.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                    let _ = minix_sys::syscall::sys_diagctl_write(
                        &minix_sys::syscall::DirectKernelCallTransport,
                        "nk4a: rs-pm post-privctl pre-initsrv\n",
                    );
                }
            }

            if priv_.flags.contains(PrivFlags::SYS_PROC) {
                // C: init_service — main.c:387: mark initializing + send the
                // RS_INIT message (asynsend seam, utility.c:62).
                let mut asynsend = |ep: Endpoint, msg: &crate::ready::InitMessage| {
                    sys.asynsend(ep, &msg.encode_message())
                };
                crate::service_create::init_service(
                    self.table.get_mut(id),
                    crate::service_create::InitSpec {
                        old_endpoint: None,
                        init_type: crate::sef::SefInitType::Fresh,
                        init_flags,
                        gid,
                        prepare_state: crate::live_update::SEF_LU_STATE_NULL,
                    },
                    ticks,
                    &mut asynsend,
                )?;
                if lookup_sys(tables.sys_table, priv_.endpoint)
                    .flags
                    .contains(SysFlags::SYNCH_BOOT)
                {
                    // C: SF_SYNCH_BOOT → catch_boot_init_ready — main.c:390-392:
                    // a blocking receive for THIS service's init-ready before
                    // boot proceeds.
                    self.catch_boot_init_ready(sys, priv_.endpoint)?;
                } else {
                    // C: main.c:393-394 — the count is the *else* branch: a
                    // SYNCH service's ready reply was already consumed by the
                    // inline catch, so counting it too would leave step 3
                    // blocked forever on a reply that never comes.
                    nr_uncaught_init_srvs += 1;
                }
            }
        }
        self.nr_uncaught_init_srvs = nr_uncaught_init_srvs;
        Ok(())
    }

    /// Step 3 — catch all remaining init-ready messages.
    ///
    /// C: `while(nr_uncaught_init_srvs) { catch_boot_init_ready(ANY); ... }` —
    /// main.c:401-407: a blocking receive per outstanding init-ready. The
    /// receive mechanism is 12-rs-init-run.md.
    fn step3_catch_init_ready(&mut self, sys: &mut dyn KernelApi) -> Result<(), BootError> {
        // C: main.c:401-407 — block for each outstanding init-ready, in
        // order. The counter is C's `nr_uncaught_init_srvs`; a service that
        // never answers blocks the boot forever (fail-closed, and C's
        // watchdog picks it up from the ping path later).
        while self.nr_uncaught_init_srvs > 0 {
            self.catch_boot_init_ready(sys, minix_types::Endpoint::ANY)?;
            self.nr_uncaught_init_srvs -= 1;
        }
        Ok(())
    }

    /// C: `catch_boot_init_ready` — main.c:789-830: block for one init-ready
    /// message from `endpoint` (a specific service for the `SF_SYNCH_BOOT`
    /// path, `ANY` for step 3), verify it, unblock the service, and mark the
    /// slot initialized. The three failure shapes are C `panic`s verbatim
    /// (R34.23): receive failure, wrong message type, non-OK result.
    fn catch_boot_init_ready(
        &mut self,
        sys: &mut dyn KernelApi,
        endpoint: minix_types::Endpoint,
    ) -> Result<(), BootError> {
        use crate::service_slot::SlotMutations;
        use minix_types::RS_INIT;
        let (m, _ipc_status, _ts) = sys.receive(endpoint).map_err(BootError::Kernel)?;
        if m.m_type != RS_INIT {
            // C: main.c:799-801.
            panic!("unexpected reply from service: {m:?}");
        }
        let Some(result) = m.rs_init_result() else {
            // The typed decode refuses non-RS_INIT messages — with the
            // m_type check above this is unreachable; a panic keeps the C
            // shape (wrong-arm reads are program errors).
            panic!("unexpected reply from service: {m:?}");
        };
        if result != 0 {
            // C: main.c:805-807 — a failed boot-time init is fatal for RS.
            panic!("unable to complete init for service: {m:?}");
        }
        // C: main.c:810-816 — unblock the service with the echo of its own
        // RS_INIT message (m_type = OK), except VM (its reply was
        // asynchronous; a synchronous reply could deadlock).
        if m.m_source != minix_types::Endpoint::VM {
            let _ = sys.reply(m.m_source, 0, &m);
        }
        // C: main.c:819-822 — mark the slot no longer initializing.
        let id = self
            .table
            .endpoint_slot(m.m_source)
            .expect("init ready from a registered service");
        SlotMutations {
            clear: crate::service_slot::RFlags::INITIALIZING,
            check_tm: Some(0),
            alive_tm: Some(sys.get_ticks()?),
            ..Default::default()
        }
        .apply(self.table.get_mut(id));
        Ok(())
    }

    /// Step 4 — pid lookup + periodic alarm.
    ///
    /// C: main.c:409-433. `getnpid` signature: 19; alarm semantics: 07.
    fn step4_finish(&mut self, sys: &mut dyn KernelApi) -> Result<(), BootError> {
        let tables = self.tables;
        for priv_ in tables.priv_table {
            if priv_.endpoint.is_kernel_task() {
                continue; // C: iskerneln skip — main.c:416-418
            }
            // C: rp = &rproc[boot_image_priv - boot_image_priv_table] — main.c:422;
            //   slot located through the A-4 endpoint index (02 §4.2).
            let id = self
                .table
                .endpoint_slot(priv_.endpoint)
                .expect("Step 1 activated every boot service");
            // C: rp->r_pid = getnpid(rpub->endpoint) — main.c:426.
            let pid = sys.getnpid(priv_.endpoint)?;
            if pid < 0 {
                // C: panic("unable to get pid") — main.c:427-429.
                return Err(BootError::InvalidPid(pid));
            }
            self.table.get_mut(id).pid = Some(pid);
        }
        // C: sys_setalarm(RS_DELTA_T, 0) — main.c:433. RS_DELTA_T = system_hz
        // (const.h:49); period/heartbeat semantics: 07-rs-period-heartbeat.md.
        sys.setalarm(self.system_hz)?;
        Ok(())
    }

    /// RS self-upgrade after boot (USE_LIVEUPDATE) — the S18 chain.
    ///
    /// C: main.c:436-491. The fork's two sides are both modelled (the
    /// single-process rewrite decides by `srv_fork`'s return, exactly like
    /// C decides by `pid == 0`): the **new instance** side runs
    /// `update_service(RS_SWAP)` + `cpf_reload` + `cleanup_service` +
    /// `vm_memctl(PIN)` (main.c:456-472); the **old instance** side sets the
    /// replica's privileges, initialises its scheduling and yields
    /// (main.c:477-489). C panics on every failure; this returns `Err` and
    /// leaves the panic boundary to the boot caller (E-10 fail-fast).
    #[cfg(feature = "live-update")]
    pub fn self_update(
        &mut self,
        sys: &mut dyn KernelApi,
        update: &mut crate::live_update::UpdateState,
    ) -> Result<(), Errno> {
        // C: clone_slot(rp, &replica_rp) — main.c:441.
        let rp = self
            .table
            .endpoint_slot(Endpoint::RS)
            .ok_or(Errno::ESRCH)?;
        let replica = crate::service_create::clone_slot(&mut self.table, rp)?;

        // C: srv_fork(0, 0) — main.c:446 (root:wheel).
        let pid = sys.srv_fork(0, 0)?;

        // C main.c:447-455 — replica_pid = pid ? pid : getpid();
        // getprocnr 后写回 replica 槽的 pid/endpoint。
        let replica_pid = if pid == 0 { sys.getnpid(Endpoint::RS)? } else { pid };
        let replica_ep = sys.getprocnr(replica_pid)?;
        {
            let r = self.table.get_mut(replica);
            r.pid = Some(replica_pid);
            r.pub_.endpoint = replica_ep;
        }

        match crate::self_lifecycle::self_upgrade_role(pid) {
            // C: main.c:456-473 — the new RS instance live-updates the old
            // one into itself, reloads the grant table, cleans up the old
            // instance and asks VM to pin its memory.
            crate::self_lifecycle::SelfUpgradeRole::NewInstance => {
                update.update_service(
                    &mut self.table,
                    sys,
                    rp,
                    replica,
                    1, // RS_SWAP (const.h:80)
                    SysFlags::empty(),
                )?;
                sys.cpf_reload()?;
                let mut no_script = |_s: &mut crate::service_slot::ServiceSlot| Ok(());
                crate::recovery::cleanup_service(&mut self.table, rp, sys, &mut no_script);
                sys.vm_memctl(Endpoint::RS, VmRsMemReq::Pin, 0, 0)?;
            }
            // C: main.c:474-489 — the old RS instance primes the replica
            // and yields control (NOT_REACHABLE in C).
            crate::self_lifecycle::SelfUpgradeRole::OldInstance => {
                let priv_snapshot = self.table.get(replica).priv_.clone();
                sys.privctl(replica_ep, PrivCtlOp::SetSys, Some(&priv_snapshot))?;
                let cfg = {
                    let s = self.table.get(replica);
                    crate::sched::SchedulerConfig::from_slot(
                        s.scheduler,
                        s.pub_.endpoint,
                        s.priority,
                        s.quantum,
                        s.cpu,
                    )
                };
                sys.sched_init_proc(&cfg)?;
                sys.privctl(replica_ep, PrivCtlOp::Yield, None)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::*;
    use crate::testutil::{Call, MockKernelApi};
    use alloc::vec::Vec;
    use minix_types::BootImage;

    /// D-02 消费半（S42 ② boot 真值链）：`acquire_from` 的 GET_IMAGE
    /// wire 形状（request=1、整表定容）与失败路径（Err 原始 errno）。
    #[test]
    fn test_boot_tables_acquire_wire_shape() {
        use minix_sys::syscall::CannedKernelCallTransport;
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let tables = BootTables::acquire_from(&canned).unwrap();
        {
            let sent = canned.sent.borrow();
            assert_eq!(sent.len(), 1);
            // SAFETY(test): GETINFO 载荷臂。
            let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
            assert_eq!(gi.request, minix_types::GET_IMAGE);
            assert_eq!(
                gi.val_len,
                (minix_types::NR_BOOT_PROCS
                    * core::mem::size_of::<minix_types::BootImageStruct>())
                    as i32
            );
        }
        // canned 应答零填缓冲 → 17 行 proc_nr=0 的建模行，priv/sys/dev
        // 三表仍是 RS 静态真值（acquire 只换 image 半）。
        assert_eq!(tables.image.len(), minix_types::NR_BOOT_PROCS);
        assert_eq!(tables.priv_table.len(), BOOT_IMAGE_PRIV_TABLE.len());

        // 内核拒绝 → Err 透传，零静默。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-minix_types::EPERM);
        assert_eq!(
            BootTables::acquire_from(&canned).unwrap_err(),
            -minix_types::EPERM
        );
    }

    const fn boot_image(proc_nr: i32, endpoint: Endpoint) -> BootImage {
        BootImage {
            proc_nr,
            proc_name: *b"x\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            endpoint,
            start_addr: 0,
            len: 0,
        }
    }

    #[test]
    fn test_unimplemented_kernel_api_fails_closed() {
        // T2: the production placeholder must fail closed with ENOSYS, not
        // panic — RS is a root system process and a panic is a system-wide
        // outage (kernel does not restart RS, RSYS_F).
        let mut sys = UnimplementedKernelApi;
        assert_eq!(sys.get_hz(), Err(Errno::ENOSYS));
        assert_eq!(sys.get_machine(), Err(Errno::ENOSYS));
        assert_eq!(
            sys.sched_init_proc(&SchedulerConfig::boot_defaults(Endpoint::PM)),
            Err(Errno::ENOSYS)
        );
        assert_eq!(sys.getnuid(Endpoint::PM), Err(Errno::ENOSYS));
        assert_eq!(sys.srv_fork(0, 0), Err(Errno::ENOSYS));
        assert_eq!(
            sys.vm_set_priv(Endpoint::VM, CallMask::empty(), false),
            Err(Errno::ENOSYS)
        );
    }

    #[test]
    fn test_domain_face_implementable_in_isolation() {
        // E-2 payoff: a test double implements only the face it exercises —
        // the former monolithic trait forced a full 22-method mock for every
        // shell test. `SysOnly` speaks just the kernel-call face and is used
        // as `&mut dyn SysApi`.
        struct SysOnly {
            hz: u32,
        }
        impl SysApi for SysOnly {
            fn diagctl_stacktrace(&mut self, _target: Endpoint) -> Result<(), Errno> {
                Err(Errno::ENOSYS)
            }
            fn diag_write(&mut self, _text: &str) -> Result<(), Errno> {
                Err(Errno::ENOSYS)
            }
            fn get_machine(&mut self) -> Result<Machine, Errno> {
                Err(Errno::ENOSYS)
            }
            fn get_hz(&mut self) -> Result<u32, Errno> {
                Ok(self.hz)
            }
            fn get_ticks(&mut self) -> Result<Clock, Errno> {
                Err(Errno::ENOSYS)
            }
            fn privctl(
                &mut self,
                _: Endpoint,
                _: PrivCtlOp,
                _: Option<&Privilege>,
            ) -> Result<(), Errno> {
                Err(Errno::ENOSYS)
            }
            fn getpriv(&mut self, _: Endpoint) -> Result<Privilege, Errno> {
                Err(Errno::ENOSYS)
            }
            fn setalarm(&mut self, _: u32) -> Result<(), Errno> {
                Err(Errno::ENOSYS)
            }
            fn sys_kill(&mut self, _: Endpoint, _: i32) -> Result<(), Errno> {
                Err(Errno::ENOSYS)
            }
            fn sys_update(
                &mut self,
                _: Endpoint,
                _: Endpoint,
                _: crate::service_slot::SysFlags,
            ) -> Result<(), Errno> {
                Err(Errno::ENOSYS)
            }
        }
        let mut mock = SysOnly { hz: 60 };
        let face: &mut dyn SysApi = &mut mock;
        assert_eq!(face.get_hz(), Ok(60));
        assert_eq!(face.get_machine(), Err(Errno::ENOSYS));
    }

    #[test]
    fn test_init_fresh_step_order() {
        // Boot tables must describe the same service set (image vs priv).
        let image: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
            boot_image(1, Endpoint::VFS),
            boot_image(6, Endpoint::DS),
            boot_image(5, Endpoint::TTY),
            boot_image(3, Endpoint::MEM),
            boot_image(7, Endpoint::MIB),
            boot_image(9, Endpoint::PFS),
            boot_image(10, Endpoint::MFS),
            boot_image(11, Endpoint::INIT),
        ];
        let tables = BootTables::new(image);
        tables
            .validate_tables()
            .expect("placeholder tables consistent");

        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        // 12 wiring: step 3 now blocks on the receive seam — with no canned
        // message the seam fails closed (Kernel(ENOSYS)) instead of the boot
        // "completing" without the messages actually arriving (main.c:401-407).
        assert_eq!(
            boot.init_fresh(&mut sys),
            Err(BootError::Kernel(Errno::ENOSYS)),
            "step 3 blocks on the receive seam: an undelivered init-ready fails the boot"
        );

        // Step 1: SYS_PRIV_SET_SYS for every service except RS/VM (10 services).
        let set_sys = sys
            .calls
            .iter()
            .filter(|c| matches!(c, Call::PrivCtl(_, PrivCtlOp::SetSys)))
            .count();
        assert_eq!(set_sys, 10, "RS/VM skip SYS_PRIV_SET_SYS (main.c:285-291)");

        // Step 2: sched_init_proc + SYS_PRIV_ALLOW for every non-RS/VM SYS_PROC.
        let sched = sys
            .calls
            .iter()
            .filter(|c| matches!(c, Call::SchedInitProc(_)))
            .count();
        let allow = sys
            .calls
            .iter()
            .filter(|c| matches!(c, Call::PrivCtl(_, PrivCtlOp::Allow)))
            .count();
        assert_eq!(sched, 10);
        assert_eq!(allow, 10);

        // Step 1: getpriv for all 12 services (RS/VM included) — main.c:293-296.
        let getpriv = sys
            .calls
            .iter()
            .filter(|c| matches!(c, Call::GetPriv(_)))
            .count();
        assert_eq!(
            getpriv, 12,
            "getpriv syncs all boot services (main.c:293-296)"
        );

        // Step 1: every slot's priv_ is the kernel-synced version returned by
        // sys_getpriv — 03-rs-privilege.md §1.3. The mock echoes back the
        // structure RS pushed with SetSys **for that endpoint** (R22a:
        // per-endpoint echo storage); RS/VM skip SetSys (main.c:285-291), so
        // their slots hold the vacant fallback.
        for ep in image.iter().map(|ip| ip.endpoint) {
            let id = boot.table.endpoint_slot(ep).expect("boot slot indexed");
            match sys
                .set_privs
                .iter()
                .rev()
                .find(|(e, _)| *e == ep)
                .map(|(_, p)| p.clone())
            {
                Some(pushed) => assert_eq!(boot.table.get(id).priv_, pushed),
                None => assert_eq!(
                    boot.table.get(id).priv_,
                    Privilege::vacant(),
                    "RS/VM skip SetSys — vacant echo expected"
                ),
            }
        }

        // Step 4 (getnpid ×12 + setalarm(system_hz)) must NOT have run: the
        // boot aborted at step 3, so no pid lookup or alarm happened.
        assert!(!sys.calls.iter().any(|c| matches!(c, Call::SetAlarm(_))));
        assert!(
            !sys.calls.iter().any(|c| matches!(c, Call::GetNpid(_))),
            "step 4 must not run when step 3 fails closed"
        );
    }

    #[test]
    fn test_init_fresh_populates_table() {
        let image: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
            boot_image(1, Endpoint::VFS),
            boot_image(6, Endpoint::DS),
            boot_image(5, Endpoint::TTY),
            boot_image(3, Endpoint::MEM),
            boot_image(7, Endpoint::MIB),
            boot_image(9, Endpoint::PFS),
            boot_image(10, Endpoint::MFS),
            boot_image(11, Endpoint::INIT),
        ];
        let tables = BootTables::new(image);
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        // Steps 1-2 only (step 3 blocks on the live receive seam — an empty
        // inbox fails the boot); the table population happens in step 1
        // (main.c:244-346).
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");

        // Step 1: the 12 boot services occupy slots 0..11 (priv-table order,
        // main.c:255), marked IN_USE|ACTIVE, and the A-4 index hits.
        use crate::service_slot::RFlags;
        let boot_eps = [
            Endpoint::RS,
            Endpoint::VM,
            Endpoint::PM,
            Endpoint::SCHED,
            Endpoint::VFS,
            Endpoint::DS,
            Endpoint::TTY,
            Endpoint::MEM,
            Endpoint::MIB,
            Endpoint::PFS,
            Endpoint::MFS,
            Endpoint::INIT,
        ];
        for (i, ep) in boot_eps.iter().enumerate() {
            let id = boot.table.endpoint_slot(*ep).expect("boot slot indexed");
            assert_eq!(id, SlotId::new(i), "slot == priv-table index");
            let slot = boot.table.get(id);
            assert!(slot.flags.contains(RFlags::IN_USE | RFlags::ACTIVE));
            assert_eq!(slot.pub_.endpoint, *ep);
        }

        // Non-boot slots stay vacant.
        for i in 12..boot.table.len() {
            let slot = boot.table.get(SlotId::new(i));
            assert!(slot.flags.is_empty());
            assert!(!slot.pub_.in_use);
        }
    }

    #[test]
    fn test_step4_sets_pid() {
        let image: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
            boot_image(1, Endpoint::VFS),
            boot_image(6, Endpoint::DS),
            boot_image(5, Endpoint::TTY),
            boot_image(3, Endpoint::MEM),
            boot_image(7, Endpoint::MIB),
            boot_image(9, Endpoint::PFS),
            boot_image(10, Endpoint::MFS),
            boot_image(11, Endpoint::INIT),
        ];
        let tables = BootTables::new(image);
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        // Drive steps 0/1/2/4 directly; step 3 (receive) is fail-closed until
        // 12 (T6) and would abort the boot.
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");
        boot.step4_finish(&mut sys).expect("step 4");

        // Step 4: every boot slot carries the pid returned by getnpid
        // (main.c:426; mock returns 100).
        let boot_eps = [
            Endpoint::RS,
            Endpoint::VM,
            Endpoint::PM,
            Endpoint::SCHED,
            Endpoint::VFS,
            Endpoint::DS,
            Endpoint::TTY,
            Endpoint::MEM,
            Endpoint::MIB,
            Endpoint::PFS,
            Endpoint::MFS,
            Endpoint::INIT,
        ];
        for ep in boot_eps {
            let id = boot.table.endpoint_slot(ep).expect("boot slot indexed");
            assert_eq!(boot.table.get(id).pid, Some(100));
        }
    }

    #[test]
    fn test_step0_creates_rproctab_grant_over_wire_mirror() {
        // C main.c:185-189 — boot step 0 creates the READ grant for ANY over
        // `rprocpub`; step 2 carries the gid in RS_INIT and VM safecopies the
        // table through it (main.c:246). The Rust grant covers the table's
        // wire mirror: 64 × 40-byte rows, granted here at the reset-fresh
        // (all-zero) content — step 1's sync fills the rows, the address is
        // what the grant pins.
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(BootTables::placeholder());
        boot.step0_prepare(&mut sys).expect("step 0");

        let (_, addr, len) = sys.last_grant.expect("step0 must create the rproctab grant");
        assert_eq!(
            len as usize,
            64 * core::mem::size_of::<minix_types::RprocpubSnap>(),
            "grant covers sizeof(rprocpub) = NR_SYS_PROCS × row width"
        );
        assert_eq!(
            addr,
            boot.table.pub_wire_bytes().as_ptr() as u64,
            "grant target = the wire mirror the kernel safecopies from"
        );
        assert_eq!(
            boot.rinit().rproctab_gid,
            Some(1),
            "MockKernelApi grant ids start at 1 (testutil grant_read)"
        );
    }

    #[test]
    fn test_step4_negative_pid_reports_invalid_pid() {
        // E-6: C panics on a non-positive pid ("unable to get pid",
        // main.c:427-429); the Rust shell fails closed with the typed cause,
        // and the wire face reads EINVAL — not the wiring-gap ENOSYS.
        let mut sys = MockKernelApi::new(100);
        sys.pids = vec![-1]; // getnpid pops this for the first boot service
        let mut boot = BootInit::new(BootTables::placeholder());
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");
        let err = boot.step4_finish(&mut sys).expect_err("negative pid");
        assert_eq!(err, BootError::InvalidPid(-1));
        assert_eq!(
            Errno::from(err),
            Errno::EINVAL,
            "wire face: boot invariant violation, not a wiring gap"
        );
    }

    #[test]
    fn test_boot_error_wire_mapping() {
        // E-6: kernel-call failures keep their errno; boot invariant
        // violations read EINVAL at the wire face — `ENOSYS` stays reserved
        // for "mechanism not wired" (T2), so the two families that the flat
        // ENOSYS used to conflate are distinguishable from the outside too.
        assert_eq!(Errno::from(BootError::Kernel(Errno::ENOSYS)), Errno::ENOSYS);
        assert_eq!(Errno::from(BootError::CountMismatch), Errno::EINVAL);
        assert_eq!(Errno::from(BootError::EndpointMismatch), Errno::EINVAL);
        assert_eq!(Errno::from(BootError::InvalidPid(-1)), Errno::EINVAL);
        assert_eq!(
            Errno::from(BootError::Lookup(LookupError::ImageTable)),
            Errno::EINVAL
        );
    }

    #[test]
    fn test_boot_failure_propagates_from_step0() {
        // E-10/R34.18: kernel-call failures abort the boot at the failing
        // step with the typed cause (E-6) — C treats every boot-stage
        // kernel failure as fatal (main.c:53/181 have no recovery path).
        let mut sys = MockKernelApi::new(100);
        sys.fail_calls = vec![Call::GetMachine];
        let mut boot = BootInit::new(BootTables::placeholder());
        assert_eq!(
            boot.init_fresh(&mut sys),
            Err(BootError::Kernel(Errno::ENOSYS))
        );
        // `get_machine` is the first boot call (main.c:53) — nothing ran
        // after it.
        assert_eq!(sys.calls.first(), Some(&Call::GetMachine));
        assert!(!sys.calls.contains(&Call::GetHz));
    }

    #[test]
    fn test_step1_lookup_failure_propagates() {
        // E-10/R34.18: a priv-table entry missing from the image table
        // aborts the boot with the typed lookup cause. The counts match
        // (2 = 2, so validate_tables passes) but the *members* disagree —
        // C has no miss branch to inherit (main.c:253-254 → panic at 731
        // on a corrupt table).
        let image: &[BootImage] = &[boot_image(2, Endpoint::RS), boot_image(0, Endpoint::PM)];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[BootImageSys {
            endpoint: Endpoint::RS,
            flags: crate::service_slot::SRVR_SF,
        }];
        let tables = BootTables {
            image,
            priv_table,
            sys_table,
            dev_table: &[],
        };
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        assert_eq!(
            boot.step1_set_attrs(&mut sys),
            Err(BootError::Lookup(LookupError::ImageTable))
        );
    }

    #[test]
    fn test_boot_failure_propagates_setalarm() {
        // E-10/R34.18: the final boot step fails → the whole boot fails.
        // C checks the setalarm result and panics (main.c:433-434).
        let mut sys = MockKernelApi::new(100);
        sys.fail_calls = vec![Call::SetAlarm(0)];
        let mut boot = BootInit::new(BootTables::placeholder());
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");
        assert_eq!(
            boot.step4_finish(&mut sys),
            Err(BootError::Kernel(Errno::ENOSYS))
        );
        // The alarm was attempted with the configured hz — the failure
        // happened at the seam, not in an earlier step.
        assert!(sys.calls.contains(&Call::SetAlarm(100)));
    }

    #[test]
    fn test_step1_skips_privctl_for_rs_vm() {
        // Custom tables: only RS + VM are boot services.
        let image: &[BootImage] = &[boot_image(2, Endpoint::RS), boot_image(8, Endpoint::VM)];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: crate::service_slot::VM_SF,
            },
        ];
        let dev_table: &[BootImageDev] = &[];
        let tables = BootTables {
            image,
            priv_table,
            sys_table,
            dev_table,
        };
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");

        let set_sys: Vec<Endpoint> = sys
            .calls
            .iter()
            .filter_map(|c| match c {
                Call::PrivCtl(ep, PrivCtlOp::SetSys) => Some(*ep),
                _ => None,
            })
            .collect();
        assert!(set_sys.is_empty(), "RS and VM must skip SYS_PRIV_SET_SYS");
    }

    #[test]
    fn test_validate_tables_mismatch() {
        // Image describes 2 services; priv table describes 12 → mismatch.
        let image: &[BootImage] = &[boot_image(2, Endpoint::RS), boot_image(8, Endpoint::VM)];
        let tables = BootTables::new(image);
        assert_eq!(
            tables.validate_tables(),
            Err(BootError::CountMismatch),
            "main.c:225-227 mismatch check (E-6: typed cause)"
        );
    }

    #[test]
    fn test_validate_tables_rejects_proc_nr_endpoint_mismatch() {
        // R17: C derives boot endpoints as `_ENDPOINT(0, proc_nr)` — a row
        // whose proc_nr disagrees with its endpoint slot boots the wrong
        // endpoint, so the placeholder must fail validation (fail-closed).
        let image: &[BootImage] = &[boot_image(2, Endpoint::PM)]; // proc_nr 2, slot 0
        let tables = BootTables::new(image);
        assert_eq!(
            tables.validate_tables(),
            Err(BootError::EndpointMismatch),
            "R17 mismatch check (E-6: typed cause)"
        );
    }

    #[test]
    fn test_boot_img_truncates_long_name() {
        // R17: a name longer than the 16-byte field is clamped instead of
        // panicking const evaluation with an out-of-range index.
        let img = boot_img(2, Endpoint::RS, "this-name-is-way-too-long");
        assert_eq!(&img.proc_name[..], b"this-name-is-way"); // 16 bytes
    }

    #[test]
    fn test_lookup_image_not_found() {
        let image: &[BootImage] = &[];
        assert!(matches!(
            lookup_image(image, Endpoint::RS),
            Err(LookupError::ImageTable)
        ));
    }

    #[test]
    fn test_lookup_priv_found() {
        let found = lookup_priv(BOOT_IMAGE_PRIV_TABLE, Endpoint::RS).expect("RS in priv table");
        assert_eq!(found.label, "rs");
    }

    #[test]
    fn test_lookup_priv_not_found() {
        assert_eq!(
            lookup_priv(BOOT_IMAGE_PRIV_TABLE, Endpoint::NONE),
            Err(LookupError::PrivTable)
        );
    }

    #[test]
    fn test_lookup_sys_default_fallback() {
        let sys = lookup_sys(BOOT_IMAGE_SYS_TABLE, Endpoint::INIT);
        assert_eq!(
            sys.flags,
            crate::service_slot::SRV_SF,
            "INIT is not in sys table → default (main.c:753-762)"
        );
    }

    #[test]
    fn test_lookup_dev_default_fallback() {
        let dev = lookup_dev(BOOT_IMAGE_DEV_TABLE, Endpoint::RS);
        assert_eq!(
            dev.dev_nr, 0,
            "RS is not in dev table → default (main.c:768-777)"
        );
    }

    #[cfg(feature = "live-update")]
    #[test]
    fn test_self_update_old_instance_leg() {
        // C main.c:474-489(pid > 0):旧实例给 replica 设特权 → 调度 →
        // YIELD;新实例腿的四步不出现。
        static IMAGE: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
            boot_image(1, Endpoint::VFS),
            boot_image(6, Endpoint::DS),
            boot_image(5, Endpoint::TTY),
            boot_image(3, Endpoint::MEM),
            boot_image(7, Endpoint::MIB),
            boot_image(9, Endpoint::PFS),
            boot_image(10, Endpoint::MFS),
            boot_image(11, Endpoint::INIT),
        ];
        let mut boot = BootInit::new(BootTables::new(IMAGE));
        let mut sys = MockKernelApi::new(100);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        sys.calls.clear();
        sys.fork_pid = Some(7);
        sys.child_endpoint = Some(Endpoint(8));
        sys.vm_ok = true;
        let mut update = crate::live_update::UpdateState::default();
        boot.self_update(&mut sys, &mut update)
            .expect("old-instance leg");

        let calls = &sys.calls;
        let pos_set_sys = calls
            .iter()
            .position(|c| matches!(c, Call::PrivCtl(ep, PrivCtlOp::SetSys) if *ep == Endpoint(8)))
            .expect("privctl(SetSys) 必发");
        let pos_sched = calls
            .iter()
            .position(|c| matches!(c, Call::SchedInitProc(ep) if *ep == Endpoint(8)))
            .expect("sched_init_proc 必发");
        let pos_yield = calls
            .iter()
            .position(|c| matches!(c, Call::PrivCtl(ep, PrivCtlOp::Yield) if *ep == Endpoint(8)))
            .expect("privctl(Yield) 必发");
        assert!(pos_set_sys < pos_sched && pos_sched < pos_yield, "C 顺序:设权→调度→让渡");
        assert!(
            !calls.iter().any(|c| matches!(c, Call::SysUpdate(_, _) | Call::CpfReload)),
            "旧实例腿不得走 update/cpf_reload"
        );
    }

    #[cfg(feature = "live-update")]
    #[test]
    fn test_self_update_new_instance_leg() {
        // C main.c:456-472(pid == 0):update_service(RS_SWAP,内核走
        // sys_update)→ cpf_reload → cleanup_service → vm_memctl(PIN)。
        static IMAGE2: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
            boot_image(1, Endpoint::VFS),
            boot_image(6, Endpoint::DS),
            boot_image(5, Endpoint::TTY),
            boot_image(3, Endpoint::MEM),
            boot_image(7, Endpoint::MIB),
            boot_image(9, Endpoint::PFS),
            boot_image(10, Endpoint::MFS),
            boot_image(11, Endpoint::INIT),
        ];
        let mut boot = BootInit::new(BootTables::new(IMAGE2));
        let mut sys = MockKernelApi::new(100);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        sys.calls.clear();
        sys.fork_pid = Some(0);
        sys.child_endpoint = Some(Endpoint(8));
        sys.vm_ok = true;
        let mut update = crate::live_update::UpdateState::default();
        boot.self_update(&mut sys, &mut update)
            .expect("new-instance leg");

        let calls = &sys.calls;
        let pos_update = calls
            .iter()
            .position(|c| matches!(c, Call::SysUpdate(_, _)))
            .expect("RS_SWAP 的 sys_update 必发");
        let pos_reload = calls
            .iter()
            .position(|c| matches!(c, Call::CpfReload))
            .expect("cpf_reload 必发");
        assert!(pos_update < pos_reload, "C 顺序:update→reload→cleanup→pin");
        assert!(
            !calls.iter().any(|c| matches!(c, Call::PrivCtl(_, PrivCtlOp::Yield))),
            "新实例腿不得 YIELD"
        );
    }

    #[test]
    fn test_placeholder_tables_valid() {
        let tables = BootTables::placeholder();
        assert!(
            tables.validate_tables().is_ok(),
            "placeholder must pass count check"
        );
    }

    fn vm_init_envelope(result: i32) -> (minix_types::Message, crate::dispatch::IpcStatus, Clock) {
        let mut m = minix_types::Message {
            m_source: Endpoint::VM,
            m_type: minix_types::RS_INIT,
            m_u: Default::default(),
        };
        m.m_u.m_rs_init.result = result; // union-field write: a safe bit store
        (m, crate::dispatch::IpcStatus { flags: 0 }, 0)
    }

    #[test]
    fn test_step3_catches_boot_init_ready() {
        // 12 wiring: main.c:401-407 + 789-830 — step 3 blocks for the
        // counted init-ready (VM), verifies it, skips the VM reply
        // (main.c:812-815), and clears INITIALIZING (main.c:819-822).
        // RS/VM-only tables: exactly one counted slot (VM — async RS_INIT).
        let image: &[BootImage] = &[boot_image(2, Endpoint::RS), boot_image(8, Endpoint::VM)];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: crate::service_slot::VM_SF,
            },
        ];
        let tables = BootTables {
            image,
            priv_table,
            sys_table,
            dev_table: &[],
        };
        let mut sys = MockKernelApi::new(100);
        sys.inbox = alloc::vec![vm_init_envelope(0)];
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");
        assert_eq!(boot.nr_uncaught_init_srvs, 1, "VM counted (main.c:369-370)");
        // The VM's init_service mark (12 — the async instance initializes
        // before the ready message arrives).
        let vm = boot.table.endpoint_slot(Endpoint::VM).expect("VM indexed");
        boot.table.get_mut(vm).flags |= crate::service_slot::RFlags::INITIALIZING;
        boot.step3_catch_init_ready(&mut sys).expect("caught");
        assert_eq!(boot.nr_uncaught_init_srvs, 0);
        assert!(
            !boot
                .table
                .get(vm)
                .flags
                .contains(crate::service_slot::RFlags::INITIALIZING)
        );
    }

    #[test]
    #[should_panic(expected = "unexpected reply from service")]
    fn test_step3_panics_on_wrong_message_type() {
        // R34.23: main.c:799-801 — a non-RS_INIT message during the boot
        // catch is a program error; C panics.
        let mut sys = MockKernelApi::new(100);
        let mut m = minix_types::Message {
            m_source: Endpoint::VM,
            m_type: 9999,
            m_u: Default::default(),
        };
        m.m_u.m_rs_init.result = 0;
        sys.inbox = alloc::vec![(m, crate::dispatch::IpcStatus { flags: 0 }, 0)];
        let mut boot = BootInit::new(BootTables::placeholder());
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");
        boot.step3_catch_init_ready(&mut sys).expect("step 3");
    }

    #[test]
    #[should_panic(expected = "unable to complete init for service")]
    fn test_step3_panics_on_failed_result() {
        // R34.23: main.c:805-807 — a failed boot-time init is fatal for RS
        // itself; C panics.
        let mut sys = MockKernelApi::new(100);
        sys.inbox = alloc::vec![vm_init_envelope(5)];
        let mut boot = BootInit::new(BootTables::placeholder());
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");
        boot.step3_catch_init_ready(&mut sys).expect("step 3");
    }

    #[test]
    fn test_step3_fails_closed_when_init_ready_pending() {
        // T6: C blocks on receive per outstanding init-ready (main.c:401-407);
        // without the 12 receive primitive, step 3 must fail closed instead
        // of pretending the messages arrived.
        let image: &[BootImage] = &[boot_image(2, Endpoint::RS), boot_image(8, Endpoint::VM)];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: crate::service_slot::VM_SF,
            },
        ];
        let dev_table: &[BootImageDev] = &[];
        let tables = BootTables {
            image,
            priv_table,
            sys_table,
            dev_table,
        };
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");
        // VM is counted (main.c:369-370) → step 3 has work to do.
        assert_eq!(boot.nr_uncaught_init_srvs, 1);
        assert_eq!(
            boot.step3_catch_init_ready(&mut sys),
            Err(BootError::Kernel(Errno::ENOSYS))
        );
    }

    #[test]
    fn test_step2_synch_boot_fails_closed() {
        // T6: a SF_SYNCH_BOOT service is synchronously caught in C
        // (main.c:390-392, blocking receive). Until 12 lands there is no
        // receive primitive — fail closed instead of silently skipping the
        // sync (which would let boot proceed without that init-ready).
        let image: &[BootImage] = &[boot_image(2, Endpoint::RS), boot_image(0, Endpoint::PM)];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::PM,
                label: "pm",
                flags: crate::privilege::SRV_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::PM,
                flags: SysFlags::SYNCH_BOOT,
            },
        ];
        let dev_table: &[BootImageDev] = &[];
        let tables = BootTables {
            image,
            priv_table,
            sys_table,
            dev_table,
        };
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        assert_eq!(
            boot.step2_allow_run(&mut sys),
            Err(BootError::Kernel(Errno::ENOSYS)),
            "SYNCH_BOOT catch: an undelivered init-ready receive fails the boot (main.c:390-392)"
        );
    }

    #[test]
    fn test_step2_init_service_marks_and_sends() {
        // I2: C main.c:362-399 — every non-kernel boot service goes through
        // init_service: slots are marked INITIALIZING with fresh
        // alive_tm/check_tm (utility.c:19-21), RS's own entry sends nothing
        // (ROOT_SYS_PROC early return — utility.c:29-31), VM and regular
        // SYS_PROC services receive the RS_INIT message (utility.c:62), and
        // VM + non-synch services are counted for step 3 (main.c:369-394).
        static IMAGE: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
        ];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
            BootImagePriv {
                endpoint: Endpoint::PM,
                label: "pm",
                flags: crate::privilege::SRV_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: crate::service_slot::VM_SF,
            },
            BootImageSys {
                endpoint: Endpoint::PM,
                flags: crate::service_slot::SRVR_SF,
            },
        ];
        let dev_table: &[BootImageDev] = &[];
        let tables = BootTables {
            image: IMAGE,
            priv_table,
            sys_table,
            dev_table,
        };
        let mut sys = MockKernelApi::new(100);
        sys.ticks = 500;
        // The kernel already holds RS/VM privilege structures from its own
        // boot processing (C main.c:293-296 getpriv succeeds for them) —
        // seed the mock's kernel-side table.
        sys.kernel_privs.push((
            Endpoint::RS,
            crate::privilege::Privilege::boot_priv(crate::privilege::RSYS_F, 2),
        ));
        sys.kernel_privs.push((
            Endpoint::VM,
            crate::privilege::Privilege::boot_priv(crate::privilege::VM_F, 8),
        ));
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");

        // utility.c:19-21 — every slot is INITIALIZING with fresh timestamps
        // (check_tm = alive_tm + 1).
        for ep in [Endpoint::RS, Endpoint::VM, Endpoint::PM] {
            let id = boot.table.endpoint_slot(ep).unwrap();
            let slot = boot.table.get(id);
            assert!(
                slot.flags
                    .contains(crate::service_slot::RFlags::INITIALIZING),
                "{ep:?} must be INITIALIZING after step 2"
            );
            assert_eq!(slot.alive_tm, 500, "{ep:?} alive_tm = getticks()");
            assert_eq!(slot.check_tm, 501, "{ep:?} check_tm = alive_tm + 1");
        }

        // RS sends nothing (utility.c:29-31); VM and PM receive RS_INIT.
        assert_eq!(
            sys.sent.len(),
            2,
            "exactly VM + PM get the init message: {:?}",
            sys.sent
                .iter()
                .map(|(e, m)| (e.0, m.m_type))
                .collect::<Vec<_>>()
        );
        for (ep, msg) in &sys.sent {
            assert!(*ep != Endpoint::RS, "RS must not receive RS_INIT");
            assert!(
                *ep == Endpoint::VM || *ep == Endpoint::PM,
                "only VM and PM get the init message"
            );
            assert_eq!(msg.m_type, minix_types::RS_INIT);
            let init = minix_types::RsInit::decode_message(msg);
            assert_eq!(init.init_type, 0, "SEF_INIT_FRESH — sef.h:93");
            assert_eq!(init.result, 0);
            assert_eq!(init.rproctab_gid, 1, "step0's rproctab grant id (main.c:185)");
            assert_eq!(init.old_endpoint, Endpoint::NONE, "boot has no old self");
            assert_eq!(init.restarts, 1, "r_restarts 0 + 1 — utility.c:58");
            assert_eq!(init.prepare_state, crate::live_update::SEF_LU_STATE_NULL);
        }

        // VM + non-synch SYS_PROC counted; RS itself is not (main.c:368-394).
        assert_eq!(boot.nr_uncaught_init_srvs, 2);
    }

    #[test]
    fn test_find_boot_param_scans_key_value_entries() {
        // C env_get_prm.c:26-84 find_key —— NUL 分隔 key=value 流的扫描
        //（mb_set_param pre_init.c:35-63 同一 NUL 条目语义：条目以 NUL
        // 结尾，末尾双 NUL 收尾）。值在 = 与条目 NUL 之间。
        let params = *b"processor=2\0rs_verbose=1\0label=rs\0";
        assert_eq!(find_boot_param(&params, b"processor"), Some(&b"2"[..]));
        assert_eq!(
            find_boot_param(&params, b"rs_verbose"),
            Some(&b"1"[..]),
            "命中条目取 = 后至 NUL"
        );
        assert_eq!(find_boot_param(&params, b"label"), Some(&b"rs"[..]));
        // 前缀撞车：key 是另一条目的前缀但无 '=' 界定 → 不命中。
        let prefix_trap = *b"rs_verbose_extra=0\0rs_verbose=1\0";
        assert_eq!(
            find_boot_param(&prefix_trap, b"rs_verbose"),
            Some(&b"1"[..]),
            "rs_verbose_extra 不拦截，第二条才是命中"
        );
        // 值为空、键缺失、空缓冲。
        assert_eq!(find_boot_param(b"k=\0x=1\0", b"k"), Some(&b""[..]));
        assert_eq!(find_boot_param(&params, b"missing"), None);
        assert_eq!(find_boot_param(&[], b"rs_verbose"), None);
        // 缓冲中途无 NUL：不假装截断成功。
        assert_eq!(find_boot_param(b"no-terminator", b"no-terminator"), None);
    }

    #[test]
    fn test_parse_rs_verbose_matches_c_env_parse() {
        // C main.c:171 env_parse("rs_verbose","d",0,&rs_verbose,0,1) —
        // 只有 "1" 打开；缺省/其他值/空值都落 verbose off。
        assert!(parse_rs_verbose(b"rs_verbose=1\0"));
        assert!(!parse_rs_verbose(b"rs_verbose=0\0"));
        assert!(!parse_rs_verbose(b"rs_verbose=\0"), "空值 → off");
        assert!(!parse_rs_verbose(b"rs_verbose=2\0"), "值域外 → off");
        assert!(!parse_rs_verbose(b"other=1\0"), "键缺失 → off");
        assert!(!parse_rs_verbose(&[]), "空缓冲 → off");
    }

    /// 批三序列钉（S42 ③）：RS_INIT 的发送次序 = boot tab 表序。C
    /// table.c:14-15 注释明言 priv 表序即"boot system services are made
    /// runnable and initialized at boot time"的顺序，main.c:344-399 按
    /// 表下标遍历；Rust step2 的 priv_table 迭代即同一消费半。
    #[test]
    fn test_boot_send_order_follows_priv_table_order() {
        static IMAGE: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
        ];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
            BootImagePriv {
                endpoint: Endpoint::PM,
                label: "pm",
                flags: crate::privilege::SRV_F,
            },
            BootImagePriv {
                endpoint: Endpoint::SCHED,
                label: "sched",
                flags: crate::privilege::SRV_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: crate::service_slot::VM_SF,
            },
            BootImageSys {
                endpoint: Endpoint::PM,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::SCHED,
                flags: crate::service_slot::SRVR_SF,
            },
        ];
        let tables = BootTables {
            image: IMAGE,
            priv_table,
            sys_table,
            dev_table: &[],
        };
        let mut sys = MockKernelApi::new(100);
        sys.ticks = 500;
        sys.kernel_privs.push((
            Endpoint::RS,
            crate::privilege::Privilege::boot_priv(crate::privilege::RSYS_F, 2),
        ));
        sys.kernel_privs.push((
            Endpoint::VM,
            crate::privilege::Privilege::boot_priv(crate::privilege::VM_F, 8),
        ));
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");

        // 发送次序恰为表序（RS 不发）——table.c:14-15 + main.c:344-399。
        let order: alloc::vec::Vec<Endpoint> =
            sys.sent.iter().map(|(e, _)| *e).collect();
        assert_eq!(
            order,
            alloc::vec![Endpoint::VM, Endpoint::PM, Endpoint::SCHED],
            "RS_INIT 发送次序 = boot tab 表序"
        );
    }

    /// 批三失败路径钉（S42 ③）：`sched_init_proc` 失败 → boot 在断点
    /// 中止（C main.c:376-377 `panic("unable to initialize scheduling")`
    /// 的 fail-closed 对应），后续服务不再收到 init 发送。
    #[test]
    fn test_boot_aborts_when_sched_init_proc_fails() {
        static IMAGE: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
        ];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
            BootImagePriv {
                endpoint: Endpoint::PM,
                label: "pm",
                flags: crate::privilege::SRV_F,
            },
            BootImagePriv {
                endpoint: Endpoint::SCHED,
                label: "sched",
                flags: crate::privilege::SRV_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: crate::service_slot::VM_SF,
            },
            BootImageSys {
                endpoint: Endpoint::PM,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::SCHED,
                flags: crate::service_slot::SRVR_SF,
            },
        ];
        let tables = BootTables {
            image: IMAGE,
            priv_table,
            sys_table,
            dev_table: &[],
        };
        let mut sys = MockKernelApi::new(100);
        sys.ticks = 500;
        sys.kernel_privs.push((
            Endpoint::RS,
            crate::privilege::Privilege::boot_priv(crate::privilege::RSYS_F, 2),
        ));
        sys.kernel_privs.push((
            Endpoint::VM,
            crate::privilege::Privilege::boot_priv(crate::privilege::VM_F, 8),
        ));
        // PM 的 sched_init_proc 失败（fail-injection 按变体匹配）。
        sys.fail_calls.push(Call::SchedInitProc(Endpoint::PM));
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        assert!(
            matches!(
                boot.step2_allow_run(&mut sys),
                Err(BootError::Kernel(Errno::ENOSYS))
            ),
            "sched_init_proc 失败 → BootError 中止 boot"
        );
        // VM 已发（断点之前），PM/SCHED 未发（断点之后不再推进）。
        let sent_eps: alloc::vec::Vec<Endpoint> =
            sys.sent.iter().map(|(e, _)| *e).collect();
        assert_eq!(
            sent_eps,
            alloc::vec![Endpoint::VM],
            "boot 停在失败服务：后续服务不收 init"
        );
    }

    /// 批三失败路径钉（S42 ③）：`rs_asynsend`（init_service 的发送腿）
    /// 失败 → boot 中止（C main.c:386-388
    /// `panic("unable to initialize service")` 的 fail-closed 对应）。
    #[test]
    fn test_boot_aborts_when_init_asynsend_fails() {
        static IMAGE: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
        ];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
            BootImagePriv {
                endpoint: Endpoint::PM,
                label: "pm",
                flags: crate::privilege::SRV_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: crate::service_slot::VM_SF,
            },
            BootImageSys {
                endpoint: Endpoint::PM,
                flags: crate::service_slot::SRVR_SF,
            },
        ];
        let tables = BootTables {
            image: IMAGE,
            priv_table,
            sys_table,
            dev_table: &[],
        };
        let mut sys = MockKernelApi::new(100);
        sys.ticks = 500;
        sys.kernel_privs.push((
            Endpoint::RS,
            crate::privilege::Privilege::boot_priv(crate::privilege::RSYS_F, 2),
        ));
        sys.kernel_privs.push((
            Endpoint::VM,
            crate::privilege::Privilege::boot_priv(crate::privilege::VM_F, 8),
        ));
        sys.fail_calls
            .push(Call::Asynsend(Endpoint::PM, minix_types::RS_INIT));
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        assert!(
            matches!(
                boot.step2_allow_run(&mut sys),
                Err(BootError::Kernel(Errno::ENOSYS))
            ),
            "init 发送失败 → BootError 中止 boot"
        );
    }

    /// 批三序列钉（S42 ③）：`SF_SYNCH_BOOT` 服务的 init-ready 在 step2
    /// **内联**捕获——阻塞发生在两服务的 init 发送之间（C main.c:390-392
    /// 的顺序语义：PM 的应答消费先于 SCHED 的 init 发送）。
    #[test]
    fn test_synch_boot_catch_is_inline_between_sends() {
        static IMAGE: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
        ];
        static PRIV_TABLE: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
            BootImagePriv {
                endpoint: Endpoint::PM,
                label: "pm",
                flags: crate::privilege::SRV_F,
            },
            BootImagePriv {
                endpoint: Endpoint::SCHED,
                label: "sched",
                flags: crate::privilege::SRV_F,
            },
        ];
        static SYS_TABLE: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: crate::service_slot::VM_SF,
            },
            // PM 挂 SYNCH_BOOT：init-ready 在 step2 内联捕获。
            // （bitflags 的 BitOr 非 const，static 内用 bits 组合还原。）
            BootImageSys {
                endpoint: Endpoint::PM,
                flags: SysFlags::from_bits_retain(
                    crate::service_slot::SRVR_SF.bits() | SysFlags::SYNCH_BOOT.bits(),
                ),
            },
            BootImageSys {
                endpoint: Endpoint::SCHED,
                flags: crate::service_slot::SRVR_SF,
            },
        ];
        let tables = BootTables {
            image: IMAGE,
            priv_table: PRIV_TABLE,
            sys_table: SYS_TABLE,
            dev_table: &[],
        };
        let mut sys = MockKernelApi::new(100);
        sys.ticks = 500;
        sys.kernel_privs.push((
            Endpoint::RS,
            crate::privilege::Privilege::boot_priv(crate::privilege::RSYS_F, 2),
        ));
        sys.kernel_privs.push((
            Endpoint::VM,
            crate::privilege::Privilege::boot_priv(crate::privilege::VM_F, 8),
        ));
        // 预置 PM 的 init-ready 应答（inbox 顺序即消费顺序）。
        let pm_ready = minix_types::RsInit {
            result: 0,
            init_type: 0,
            rproctab_gid: -1,
            old_endpoint: Endpoint::NONE,
            restarts: 1,
            flags: 0,
            buff_addr: minix_types::VirBytes(0),
            buff_len: 0,
            prepare_state: 0,
        }
        .encode_message();
        let mut pm_ready = pm_ready;
        pm_ready.m_source = Endpoint::PM;
        sys.inbox
            .push((pm_ready, crate::dispatch::IpcStatus::default(), 501));
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");

        // 位置序：VM 发 < PM 的应答回声（catch 内联证据）< SCHED 发。
        let calls = &sys.calls;
        let find = |pred: &dyn Fn(&Call) -> bool| {
            calls.iter().position(pred).expect("调用必须存在")
        };
        let vm_send = find(&|c| matches!(c, Call::Asynsend(e, _) if *e == Endpoint::VM));
        let pm_reply = find(&|c| matches!(c, Call::Reply(e, _) if *e == Endpoint::PM));
        let sched_send = find(&|c| matches!(c, Call::Asynsend(e, _) if *e == Endpoint::SCHED));
        assert!(
            vm_send < pm_reply && pm_reply < sched_send,
            "SYNCH_BOOT 的 catch 内联在前后服务的 init 发送之间 \
             (vm_send={vm_send}, pm_reply={pm_reply}, sched_send={sched_send})"
        );
        // VM + SCHED 计入 step3；PM 已内联消费。
        assert_eq!(boot.nr_uncaught_init_srvs, 2);
    }

    #[test]
    fn test_step3_vm_init_roundtrip_clears_initializing() {
        // I2: the full boot init exchange at mock level — step 2 sends
        // RS_INIT to VM, VM's ready reply is scripted in the inbox, step 3
        // catches it, clears INITIALIZING, and never replies to VM
        // (main.c:812-815 — VM's reply was asynchronous; a synchronous
        // reply could deadlock).
        static IMAGE: &[BootImage] = &[boot_image(2, Endpoint::RS), boot_image(8, Endpoint::VM)];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: crate::privilege::RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: crate::privilege::VM_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: crate::service_slot::SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: crate::service_slot::VM_SF,
            },
        ];
        let dev_table: &[BootImageDev] = &[];
        let tables = BootTables {
            image: IMAGE,
            priv_table,
            sys_table,
            dev_table,
        };
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");
        boot.step2_allow_run(&mut sys).expect("step 2");

        // VM answers: RS_INIT with result 0 (main.c:401-407 catch path).
        let mut ready = minix_types::RsInit {
            result: 0,
            init_type: 0,
            rproctab_gid: -1,
            old_endpoint: Endpoint::NONE,
            restarts: 1,
            flags: 0,
            buff_addr: minix_types::VirBytes(0),
            buff_len: 0,
            prepare_state: 0,
        }
        .encode_message();
        ready.m_source = Endpoint::VM;
        sys.inbox
            .push((ready, crate::dispatch::IpcStatus::default(), 501));

        boot.step3_catch_init_ready(&mut sys).expect("step 3");

        let vm_id = boot.table.endpoint_slot(Endpoint::VM).unwrap();
        assert!(
            !boot
                .table
                .get(vm_id)
                .flags
                .contains(crate::service_slot::RFlags::INITIALIZING),
            "a caught init-ready clears INITIALIZING"
        );
        assert_eq!(boot.nr_uncaught_init_srvs, 0);
        assert!(
            !sys.calls
                .iter()
                .any(|c| matches!(c, Call::Reply(Endpoint::VM, _))),
            "no synchronous reply to VM — main.c:812-815"
        );
    }

    #[test]
    fn test_rs_server_handover_after_fresh_init() {
        // T1: after a completed fresh boot (RS-only table → zero pending
        // init-ready, so step 3 passes), the runtime state is owned by the
        // server: the main loop can reach table/system_hz/shutting_down
        // without a getter dump on BootInit.
        static IMAGE: &[BootImage] = &[boot_image(2, Endpoint::RS)];
        let priv_table: &[BootImagePriv] = &[BootImagePriv {
            endpoint: Endpoint::RS,
            label: "rs",
            flags: crate::privilege::RSYS_F,
        }];
        let sys_table: &[BootImageSys] = &[BootImageSys {
            endpoint: Endpoint::RS,
            flags: crate::service_slot::SRVR_SF,
        }];
        let dev_table: &[BootImageDev] = &[];
        let tables = BootTables {
            image: IMAGE,
            priv_table,
            sys_table,
            dev_table,
        };
        let sys = MockKernelApi::new(100);
        // N5: the SEF callback set is the trait implemented by RsServer —
        // no registration value to pass at construction.
        let mut server = crate::RsServer::with_kernel(tables, Box::new(sys));
        server
            .init(crate::SefInitType::Fresh)
            .expect("RS-only boot completes (zero pending init-ready)");

        let state = server.state().expect("handover after init(Fresh)");
        assert_eq!(state.system_hz, 100);
        assert!(!state.shutting_down);
        // N3: the machine snapshot taken at startup (main.c:53) survives the
        // boot→run handover — check_request resolves CPU affinities against
        // it (request.c:1286-1296).
        assert_eq!(state.machine, Machine::default());
        assert_eq!(
            state.table.endpoint_slot(Endpoint::RS),
            Some(SlotId::new(0))
        );
        // The boot machine is consumed by the handover (no double ownership).
        assert!(server.boot.is_none());
        // E-3: the post-boot table is settled-consistent (all 12 boot rows
        // indexed at their own endpoints). A-4: the fresh UpdateState is in
        // play from the handover on — its empty chain trivially matches the
        // boot rows' vacant mirrors.
        state.table.assert_consistent(Some(&state.update));
    }

    #[test]
    fn test_boot_slot_populates_s2_fields() {
        // S2: boot slots carry cmd/args/argc/vm_call_mask/scheduler/priority/
        // quantum/alive_tm (main.c:308-333) — the fields the heartbeat (07)
        // and exec (09/10) paths rely on.
        let mut sys = MockKernelApi::new(100);
        sys.ticks = 4242;
        static IMAGE: &[BootImage] = &[boot_image(2, Endpoint::RS)];
        let priv_table: &[BootImagePriv] = &[BootImagePriv {
            endpoint: Endpoint::RS,
            label: "rs",
            flags: crate::privilege::RSYS_F,
        }];
        let sys_table: &[BootImageSys] = &[BootImageSys {
            endpoint: Endpoint::RS,
            flags: crate::service_slot::SRVR_SF,
        }];
        let dev_table: &[BootImageDev] = &[];
        let tables = BootTables {
            image: IMAGE,
            priv_table,
            sys_table,
            dev_table,
        };
        let mut boot = BootInit::new(tables);
        boot.step0_prepare(&mut sys).expect("step 0");
        boot.step1_set_attrs(&mut sys).expect("step 1");

        let slot = boot.table.get(SlotId::new(0));
        // C: strlcpy(r_cmd, proc_name) + build_cmd_dep — main.c:308-310.
        assert_eq!(slot.cmd[..3], *b"x\0\0");
        assert_eq!(slot.argc, 1);
        assert_eq!(slot.script[0], 0); // r_script[0] = '\0' — main.c:309
        // C: SRV_VC = ALL_C → full vm_call_mask — main.c:317-319, priv.h:78-80.
        assert_eq!(slot.pub_.vm_call_mask, crate::privilege::CallMask::all());
        // C: SRV_SCH=KERNEL / SRV_Q=USER_Q=7 / SRV_QT=USER_QUANTUM=200 — main.c:320-322.
        assert_eq!(slot.scheduler, Endpoint::KERNEL);
        assert_eq!(slot.priority, crate::sched::USER_Q);
        assert_eq!(slot.quantum, crate::sched::USER_QUANTUM);
        // C: r_alive_tm = getticks() — main.c:333.
        assert_eq!(slot.alive_tm, 4242);
    }
}
