//! Production `KernelApi` — the trap backend implementation of RS's external
//! boundary.
//!
//! The only production implementation of [`crate::boot::KernelApi`]'s five-domain
//! face: each method delegates to a real minix-sys wrapper (no stubs remain).
//! The powered-up form is the final form (the real-trap feature is enabled
//! uniformly by the image build chain, and host builds answer ±EIO on both legs —
//! failures are explicitly observable, never panics).
//!
//! # Two transport legs
//!
//! minix-sys has two independent transport channels, and the five-domain face
//! spans both:
//!
//! - **IPC leg** `DirectTrapTransport` (SEF/PM/VM/SCHED messages) — the `*_via`
//!   wrappers in `pm.rs`/`vm.rs` and `IpcApi` go through here;
//! - **Kernel-call leg** `DirectKernelCallTransport` (`sys_*` calls) — every
//!   `sys_*` wrapper in `syscall.rs` takes `&impl KernelCallTransport`.
//!
//! # Three error conventions (see 19-rs-external-interfaces.md)
//!
//! 1. `*_via` taskcall wrappers: already folded, they return `Result<_, Errno>`;
//! 2. `sys_*` kernel-call wrappers: bare i32, negative = errno → `Errno::from_i32(-r)`;
//! 3. IPC leg `TrapStatus`: positive errno → `Errno::from_i32(s.0)`.
//!
//! C counterpart: the libsys call face of `minix3/minix/servers/rs/main.c`
//! (sys_getmachine/sys_privctl/sched_start/...) and the `_taskcall(PM/VM)` face
//! of `minix3/minix/servers/rs/manager.c`.

use minix_sys::ipc::{AsyncSendQueue, AsyncSlotFlags, IpcTransport as _};
use minix_sys::syscall::{self, DirectKernelCallTransport};
use minix_sys::{pm as sys_pm, vm as sys_vm};
use minix_types::{Clock, Endpoint, Errno, GrantId, Message, Pid};

use crate::boot::{IpcApi, Machine, PmApi, SchedApi, SysApi, VmApi, VmRsMemReq};
use crate::privilege::{CallMask, PrivCtlOp, PrivFlags, PrivId, Privilege, SysMap, TrapMask};
use crate::sched::SchedulerConfig;

// ── Wire image of privctl SetSys ─────────────────────────────────────

/// Wire image of `IoRange` (kernel kpriv.rs:21-24, repr(C), 8 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
struct WireIoRange {
    base: u32,
    limit: u32,
}

/// Wire image of `MemRange` (kernel kpriv.rs:39-42, repr(C), 16 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
struct WireMemRange {
    base: u64,
    limit: u64,
}

/// Payload image of `SYS_PRIV_SET_SYS`/`UPDATE_SYS`.
///
/// Authoritative kernel-side layout: `PrivUpdateRequest` in
/// `os/kernel/src/kpriv.rs` (repr(C); the kernel comment states it is an
/// independent struct that only needs to be internally consistent with the RS
/// server's payload layout). This image maps domain by domain onto rs's
/// [`Privilege`](crate::privilege::Privilege) (privilege.rs:362-395, "comparable
/// per domain"). **Field widths, not just field order, must match the kernel**:
/// the kernel has `s_id: SysId = u16` (kpriv.rs:10), so the image must be u16 —
/// an old defect confirmed on real hardware: an i32 `s_id` shifted the repr(C)
/// header by 4 bytes and the kernel read the high halfword of rs's s_id as
/// s_flags (always 0); after SET_SYS overwrote it, every service's SYS_PROC
/// became 0 → sched `sys_setalarm` EPERM panic (main.rs init_scheduling arm).
/// `s_ipc_to` happens to realign at byte 24, so the IPC mask stayed correct and
/// the symptom appeared only on the flags/sig_mgr legs — highly misleading. The
/// `offset_of!` guards pin the three header offsets; any drift fails the build.
#[repr(C)]
struct WirePrivUpdate {
    s_id: u16,
    s_flags: u16,
    s_init_flags: i32,
    s_sig_mgr: i32,
    s_bak_sig_mgr: i32,
    s_trap_mask: u16,
    s_ipc_to: u64,
    s_k_call_mask: [u32; 2],
    s_nr_io_range: i32,
    s_io_tab: [WireIoRange; 64],
    s_nr_irq: i32,
    s_irq_tab: [i32; 16],
    s_nr_mem_range: i32,
    s_mem_tab: [WireMemRange; 20],
}

// Header offset guards: byte-for-byte alignment evidence against the kernel's
// PrivUpdateRequest (kpriv.rs, repr(C), SysId=u16). s_flags must be @2,
// s_init_flags @4, s_ipc_to @24 (coinciding with the kernel's first 8-byte
// aligned field).
const _: () = assert!(core::mem::offset_of!(WirePrivUpdate, s_flags) == 2);
const _: () = assert!(core::mem::offset_of!(WirePrivUpdate, s_init_flags) == 4);
const _: () = assert!(core::mem::offset_of!(WirePrivUpdate, s_ipc_to) == 24);
const _: () = assert!(core::mem::size_of::<WirePrivUpdate>() == core::mem::size_of::<[u64; 119]>());

impl WirePrivUpdate {
    /// Full [`Privilege`] → wire encoding (the SetSys/UpdateSys payload).
    fn encode(priv_: &Privilege) -> Self {
        Self {
            s_id: priv_.id.0 as u16,
            s_flags: priv_.flags.bits(),
            s_init_flags: priv_.init_flags as i32,
            s_sig_mgr: priv_.sig_mgr.0,
            s_bak_sig_mgr: priv_.bak_sig_mgr.0,
            s_trap_mask: priv_.trap_mask.bits(),
            s_ipc_to: priv_.ipc_to.0,
            s_k_call_mask: [
                priv_.k_call_mask.0 as u32,
                (priv_.k_call_mask.0 >> 32) as u32,
            ],
            s_nr_io_range: priv_.nr_io_range,
            s_io_tab: core::array::from_fn(|i| {
                let r = &priv_.io_ranges[i];
                // The rs side means "count len"; the wire means "last port limit"
                // (C io_range; the kernel kpriv compares raw values). len==0 → stay
                // all zero (unused entry).
                let limit = if r.len == 0 { 0 } else { r.base + r.len - 1 };
                WireIoRange {
                    base: r.base,
                    limit,
                }
            }),
            s_nr_irq: priv_.nr_irq,
            s_irq_tab: priv_.irqs,
            s_nr_mem_range: priv_.nr_mem_range,
            s_mem_tab: core::array::from_fn(|i| {
                let r = &priv_.mem_ranges[i];
                let limit = if r.len == 0 { 0 } else { r.base + r.len - 1 };
                WireMemRange {
                    base: r.base,
                    limit,
                }
            }),
        }
    }
}

// ── Production structure ─────────────────────────────────────────────────

/// Production implementation of RS's external boundary (both legs plus the
/// privctl/mask scratch payloads).
///
/// Why the scratch fields exist: `sys_privctl(SetSys)` and `vm_rs_set_priv` both
/// pass **the address of RS-owned memory** (the kernel/VM safecopy/datacopy that
/// address during the synchronous call). Struct fields keep the address stable
/// for the duration of the call — a stack temporary's address would also work,
/// but a field makes "payload lifetime ≥ call duration" a type-level fact.
pub struct TrapKernelApi {
    /// IPC leg (the `*_via` wrappers of pm/vm/sched and [`IpcApi`]).
    ipc: minix_sys::ipc::DirectTrapTransport,
    /// Kernel-call leg (the `sys_*` wrappers).
    kernel: DirectKernelCallTransport,
    /// DS client (label resolution, the publish/retrieve family).
    ds: minix_sys::ds::DsClient<minix_sys::ipc::DirectTrapTransport, DirectKernelCallTransport>,
    /// Grant table (the cpf_grant_direct/revoke lifeline).
    grants: minix_sys::grant::GrantTable,
    /// Stable payload for `sys_privctl(SetSys/UpdateSys)`.
    priv_wire: WirePrivUpdate,
    /// Mask buffer for `vm_rs_set_priv` (VM datacopies back 8 bytes, 2×u32 LE).
    mask_wire: [u8; 8],
    /// Persistent SENDA ring for asynchronous sends — a faithful port of C's
    /// `static asynmsg_t msgtable[ASYN_NR]` (`asynsend.c:18`; the same shape as
    /// VM's `KernelIpcTransport::queue`).
    /// The kernel `senda` records only the user-space address of this table; when
    /// the target reaches receive it reads that address back and writes
    /// `result|AMF_DONE` (ipc.rs `deliver_async`), so a slot must outlive the
    /// `asynsend` call. A stack-local single slot is flushed when the next `senda`
    /// re-registers `s_asyntab` before the old message is delivered (RS sends
    /// RS_INIT several times at boot; the dst=VFS entry was overwritten by the next
    /// send and its stack frame died → RS_INIT lost forever → VFS never left
    /// receive(RS) → INIT→VFS→RS boot deadlock). Capacity 32 covers the boot
    /// fan-out (all SYS_PROC servers, ~12) with margin.
    senda_queue: AsyncSendQueue<32>,
}

impl TrapKernelApi {
    // Not const: DsClient::new constructs a GrantTable (not const).
    pub fn new() -> Self {
        Self {
            ipc: minix_sys::ipc::DirectTrapTransport,
            kernel: DirectKernelCallTransport,
            ds: minix_sys::ds::DsClient::new(
                minix_sys::ipc::DirectTrapTransport,
                DirectKernelCallTransport,
                minix_types::Endpoint::DS,
            ),
            grants: minix_sys::grant::GrantTable::new(),
            priv_wire: WirePrivUpdate {
                s_id: 0,
                s_flags: 0,
                s_init_flags: 0,
                s_sig_mgr: 0,
                s_bak_sig_mgr: 0,
                s_trap_mask: 0,
                s_ipc_to: 0,
                s_k_call_mask: [0; 2],
                s_nr_io_range: 0,
                s_io_tab: [WireIoRange { base: 0, limit: 0 }; 64],
                s_nr_irq: 0,
                s_irq_tab: [0; 16],
                s_nr_mem_range: 0,
                s_mem_tab: [WireMemRange { base: 0, limit: 0 }; 20],
            },
            mask_wire: [0; 8],
            senda_queue: AsyncSendQueue::new(),
        }
    }
}

impl Default for TrapKernelApi {
    fn default() -> Self {
        Self::new()
    }
}

impl TrapKernelApi {
    /// User stack top (C: `minix_get_user_sp`, kernel_utils.c:40-52 reads
    /// kuserinfo.kui_user_sp from the kerninfo page) — Rust substitutes a
    /// GET_KINFO struct copy (KinfoStruct.user_sp@8, the same pattern as
    /// clock_time's SYS_TIMES substitution).
    fn user_sp(&self) -> Result<u64, Errno> {
        let mut buf = [0u8; core::mem::size_of::<minix_types::KinfoStruct>()];
        syscall::sys_getinfo_into(&self.kernel, minix_types::GET_KINFO, &mut buf, 0)
            .map_err(Errno::from_i32)?;
        Ok(i64::from_le_bytes(buf[8..16].try_into().unwrap()) as u64)
    }
}

/// Fold a kernel-call wrapper's bare i32 (negative = errno; precedent in pm/src/exit.rs).
fn sys_result(r: i32) -> Result<(), Errno> {
    if r < 0 {
        Err(Errno::from_i32(-r))
    } else {
        Ok(())
    }
}

impl SysApi for TrapKernelApi {
    fn get_machine(&mut self) -> Result<Machine, Errno> {
        // The kernel's getinfo_machine copies MachineStruct (32 bytes;
        // kernel/misc.rs:301): processors_count u32@0, bsp_id u32@4.
        let mut buf = [0u8; 32];
        syscall::sys_get_machine(&self.kernel, &mut buf).map_err(Errno::from_i32)?;
        Ok(Machine {
            processors_count: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            bsp_id: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
        })
    }

    fn get_hz(&mut self) -> Result<u32, Errno> {
        let hz = syscall::sys_get_hz(&self.kernel).map_err(Errno::from_i32)?;
        Ok(hz as u32)
    }

    fn get_ticks(&mut self) -> Result<Clock, Errno> {
        // SYS_TIMES is the carrier (C getticks's kernel request) — the kernel
        // replaces SELF with the caller. RS consumes only boot_ticks (note at
        // syscall.rs:1798).
        let times = syscall::sys_times(&self.kernel, Endpoint::SELF.0).map_err(Errno::from_i32)?;
        Ok(times.boot_ticks as Clock)
    }

    fn privctl(
        &mut self,
        proc: Endpoint,
        op: PrivCtlOp,
        priv_: Option<&Privilege>,
    ) -> Result<(), Errno> {
        // SetSys/UpdateSys: the kernel safecopies the whole WirePrivUpdate from
        // arg_ptr (the arg_ptr != 0 branch of kernel/syscall.rs privctl_set_sys);
        // other opcodes pass only opcode+endpoint with arg_ptr=0.
        let arg_ptr = match (op, priv_) {
            (PrivCtlOp::SetSys | PrivCtlOp::UpdateSys, Some(p)) => {
                self.priv_wire = WirePrivUpdate::encode(p);
                (&raw const self.priv_wire) as u64
            }
            _ => 0,
        };
        syscall::sys_privctl(&self.kernel, proc.0, op as i32, arg_ptr).map_err(Errno::from_i32)
    }

    fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, Errno> {
        // The on-wire form is PrivInfoStruct (56 bytes; kernel/misc.rs:807): no
        // init_flags/bak_sig_mgr/io/irq/mem — incremental read-back is the agreed
        // division of labor (see the WirePrivUpdate note).
        let mut buf = [0u8; 56];
        syscall::sys_get_priv(&self.kernel, proc.0, &mut buf).map_err(Errno::from_i32)?;
        let rd32 = |off: usize| u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        let rd64 = |off: usize| u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
        let k_mask = [rd32(48), rd32(52)];
        Ok(Privilege {
            id: PrivId(rd32(4) as i32),
            flags: PrivFlags::from_bits_truncate(rd32(8) as u16),
            init_flags: 0,
            sig_mgr: Endpoint(rd32(20) as i32),
            bak_sig_mgr: Endpoint::NONE,
            trap_mask: TrapMask::from_bits_retain(rd32(12) as u16),
            ipc_to: SysMap(rd64(40)),
            k_call_mask: CallMask(k_mask[0] as u64 | ((k_mask[1] as u64) << 32)),
            ..Privilege::vacant()
        })
    }

    fn setalarm(&mut self, delay_ticks: u32) -> Result<(), Errno> {
        // C main.c:433 does not check the old alarm — ignore the returned (time_left, uptime).
        syscall::sys_setalarm(&self.kernel, delay_ticks as u64, false)
            .map(|_| ())
            .map_err(Errno::from_i32)
    }

    fn sys_kill(&mut self, proc: Endpoint, signo: i32) -> Result<(), Errno> {
        sys_result(syscall::sys_kill(&self.kernel, proc.0, signo))
    }

    fn sys_update(
        &mut self,
        src: Endpoint,
        dst: Endpoint,
        flags: crate::service_slot::SysFlags,
    ) -> Result<(), Errno> {
        syscall::sys_update(&self.kernel, src.0, dst.0, flags.bits() as i32)
            .map_err(Errno::from_i32)
    }

    fn diagctl_stacktrace(&mut self, target: Endpoint) -> Result<(), Errno> {
        syscall::sys_diagctl_stacktrace(&self.kernel, target.0).map_err(Errno::from_i32)
    }

    fn diag_write(&mut self, text: &str) -> Result<(), Errno> {
        syscall::sys_diagctl_write(&self.kernel, text).map_err(Errno::from_i32)
    }
}

impl SchedApi for TrapKernelApi {
    fn sched_init_proc(&mut self, cfg: &SchedulerConfig) -> Result<Endpoint, Errno> {
        // The composite transport target of C sched_start.c:46-88: when the
        // scheduler is the kernel → sys_schedctl; otherwise send a SCHEDULING_START
        // message to the scheduler endpoint.
        if cfg.scheduler == Endpoint::KERNEL {
            syscall::sys_schedctl(
                &self.kernel,
                cfg.endpoint.0,
                cfg.priority,
                cfg.quantum,
                cfg.cpu,
            )
            .map_err(Errno::from_i32)?;
        } else {
            sys_pm::sched_start_via(
                &self.ipc,
                cfg.scheduler,
                cfg.endpoint,
                cfg.parent,
                cfg.priority,
                cfg.quantum,
            )?;
        }
        Ok(cfg.scheduler)
    }

    fn sched_stop(&mut self, scheduler: Endpoint, proc: Endpoint) -> Result<(), Errno> {
        sys_pm::sched_stop_via(&self.ipc, scheduler, proc)
    }
}

impl PmApi for TrapKernelApi {
    fn getnuid(&mut self, proc: Endpoint) -> Result<u32, Errno> {
        let uid = sys_pm::getnuid_via(&self.ipc, proc)?;
        Ok(uid as u32)
    }

    fn getnpid(&mut self, proc: Endpoint) -> Result<i32, Errno> {
        sys_pm::getnpid_via(&self.ipc, proc)
    }

    fn getprocnr(&mut self, pid: Pid) -> Result<Endpoint, Errno> {
        sys_pm::getprocnr_via(&self.ipc, pid)
    }

    fn cpf_reload(&mut self) -> Result<(), Errno> {
        // C cpf_reload (safecopies.c:373-381): after fork, tell the kernel the
        // grant table location again (sys_setgrant); C's `if (grants)` empty-table
        // guard — the table always exists here, so register is called directly.
        self.grants.register(&self.kernel).map_err(Errno::from_i32)
    }

    fn srv_fork(&mut self, uid: u32, gid: u32) -> Result<Pid, Errno> {
        sys_pm::service_fork_via(&self.ipc, uid, gid)
    }

    fn srv_execve(
        &mut self,
        proc: Endpoint,
        exec: &[u8],
        _progname: &crate::service_slot::Label,
        args: &[u8],
        argc: usize,
    ) -> Result<(), Errno> {
        // The five-step assembly of C rs/exec.c:21-64 srv_execve + do_exec
        // (:64-127). This implementation runs in the child process (self-exec):
        // segments and stack are written into its own address space, and
        // exec_restart hands over control once PM takes charge.
        use alloc::vec::Vec;
        use minix_elf;
        use minix_sys::stack::{stack_fill, stack_params};

        // 1. Argument decoding: `args` is a NUL-separated argv pack (the first
        //    argc entries are arguments). C's envp inheritance is not modeled per
        //    the ARCH decision (10-rs-service-convert.md §3).
        let mut argv: Vec<&str> = Vec::new();
        for part in args.split(|&b| b == 0) {
            if argv.len() == argc || part.is_empty() {
                break;
            }
            argv.push(core::str::from_utf8(part).map_err(|_| Errno::EINVAL)?);
        }
        if argv.len() < argc {
            return Err(Errno::EINVAL);
        }
        let empty: Vec<&str> = Vec::new();

        // 2. Stack frame budget and image construction (stack_utils.c discipline;
        //    user_sp comes from GET_KINFO's USR_STACKTOP).
        let params = stack_params(&argv, &empty);
        let mut frame = alloc::vec![0u8; params.frame_size];
        let user_sp = self.user_sp()?;
        let placement = stack_fill(&argv, &empty, params.frame_size, user_sp, &mut frame).map_err(
            |e| match e {
                minix_sys::stack::StackFillError::FrameTooSmall => Errno::E2BIG,
                minix_sys::stack::StackFillError::SizeMismatch => Errno::EINVAL,
            },
        )?;

        // 3. ELF segment loading (on-demand semantics: pages are fault-allocated
        //    on first access; filesz bytes are copied from the image into its own
        //    vaddr; the same shape as the sys_datacopy self-copy in exec.c:130-150
        //    read_seg).
        let mut copied_any = false;
        for seg in minix_elf::segment_iter(exec).map_err(|_| Errno::ENOEXEC)? {
            let src = &exec[seg.offset as usize..(seg.offset + seg.filesz) as usize];
            syscall::sys_datacopy(
                &self.kernel,
                syscall::SELF,
                src.as_ptr() as u64,
                syscall::SELF,
                seg.vaddr,
                seg.filesz,
            )
            .map_err(Errno::from_i32)?;
            copied_any = true;
        }
        if !copied_any {
            return Err(Errno::ENOEXEC);
        }

        // 4. PM takeover (PM_EXEC_NEW semantics continue through exec_restart's RS
        //    gate; stack frame bytes are delivered via exec_restart's ps_str/pc
        //    contract).
        // [SD19-gate] Best-effort finish; discarding the Result does not change the established primary result (C is void/ignored at the same spot)
        #[allow(clippy::let_underscore_must_use)]
        let _ = proc;

        // 5. exec_restart handshake (pm/exec.c:150: exec_restart(proc, OK,
        //    execi.pc, ps_str)).
        let pc = minix_elf::entry_point(exec).map_err(|_| Errno::ENOEXEC)?;
        sys_pm::exec_restart_via(&self.ipc, proc, 0, pc, placement.ps_str)
    }

    fn srv_kill(&mut self, pid: Pid, signo: i32) -> Result<(), Errno> {
        sys_pm::service_kill_via(&self.ipc, pid, signo)
    }

    fn waitpid(&mut self) -> Option<Pid> {
        // C request.c:1063 do_sigchld drain loop: waitpid(-1, &status, WNOHANG) > 0.
        // The trait has no error channel: ECHILD (no more children) ends the drain
        // normally; other errors also end it (matching C's loop-exit shape).
        // With WNOHANG, a live child that has not exited returns pid==0 — C's `> 0`
        // condition then exits the drain loop. `Some(0)` would make the caller's
        // `while let Some(_)` spin indefinitely (same as init's runetcrc/single_user
        // reap loops), so 0 also maps to None.
        match sys_pm::waitpid_via(&self.ipc, -1, minix_sys::wait::WNOHANG, 0) {
            Ok((pid, _status)) if pid > 0 => Some(pid),
            _ => None,
        }
    }

    fn setuid(&mut self, uid: u32) -> Result<(), Errno> {
        sys_pm::setuid_via(&self.ipc, uid)
    }
}

impl VmApi for TrapKernelApi {
    fn vm_memctl(
        &mut self,
        proc: Endpoint,
        req: VmRsMemReq,
        a: usize,
        b: usize,
    ) -> Result<(), Errno> {
        // a/b are consumed only by HeapPrealloc/MapPrealloc (req 2/3): addr goes
        // in m1p1, len in m1i3 (VM decoding, dispatcher.rs:885-895).
        sys_vm::vm_rs_memctl_via(&self.ipc, proc, req as i32, a as u64, b as u64)
    }

    fn vm_set_priv(
        &mut self,
        proc: Endpoint,
        vm_call_mask: CallMask,
        allow: bool,
    ) -> Result<(), Errno> {
        // VM datacopies back 8 bytes = 2×u32 bitchunks assembled LE into a u64
        // (vm dispatcher.rs:805-846, NR_VM_CALLS=49 → 2 chunks).
        self.mask_wire = vm_call_mask.0.to_le_bytes();
        sys_vm::vm_rs_set_priv_via(
            &self.ipc,
            proc,
            (&raw const self.mask_wire) as u64,
            allow as i32,
        )
    }
}

impl IpcApi for TrapKernelApi {
    fn receive(
        &mut self,
        endpoint: Endpoint,
    ) -> Result<(Message, crate::dispatch::IpcStatus, Clock), Errno> {
        let mut msg = Message::default();
        let sts = self
            .ipc
            .receive(endpoint, &mut msg)
            .map_err(|t| Errno::from_i32(t.0))?;
        let status = crate::dispatch::IpcStatus { flags: sts.0 };
        let stamp = if status.is_notify() {
            // Kernel timestamp of a notification payload (ipc.h:1715); the union
            // arm is read only after a successful receive classified as a
            // notification, matching the arm the sender filled.
            (unsafe { msg.m_u.m_notify.timestamp }) as Clock
        } else {
            0
        };
        Ok((msg, status, stamp))
    }

    fn reply(&mut self, target: Endpoint, result: i32, payload: &Message) -> Result<(), Errno> {
        // C: `reply` is a **non-blocking** `ipc_sendnb(who, m_ptr)`
        // (utility.c:324); the handler-mutated request message is sent back with
        // the result as m_type. An earlier note wrongly called it "blocking
        // ipc_send" (utility.c uses sendnb throughout); the birth-report handshake
        // requires the service side to park in receive(RS) with sendrec, and this
        // single sendnb at RS's exit delivers directly via Path A to wake the
        // parked service — isomorphic to C.
        let mut m = *payload;
        m.m_type = result;
        self.ipc
            .sendnb(target, &m)
            .map_err(|t| Errno::from_i32(t.0))
    }

    fn notify(&mut self, endpoint: Endpoint) -> Result<(), Errno> {
        self.ipc.notify(endpoint).map_err(|t| Errno::from_i32(t.0))
    }

    fn asynsend(&mut self, endpoint: Endpoint, message: &Message) -> Result<(), Errno> {
        // C: both branches of rs_asynsend (utility.c:223-242) go through asynsend3
        // → the persistent multi-entry `static msgtable[ASYN_NR]` (asynsend.c:18):
        // a new message is appended at next_slot and never flushes the previous
        // one; senda_reload re-registers the whole unfinished table
        // (`&msgtable[first_slot]`, len) with the kernel each round
        // (asynsend.c:144-155). An earlier version misread it as a "one-shot single
        // slot" (built on the stack); the next senda would overwrite s_asyntab and
        // invalidate the old stack frame — exactly the path taken when RS fires
        // RS_INIT in a boot burst. It now holds a persistent AsyncSendQueue field:
        // first enqueue (VALID is OR'd in last by enqueue, matching
        // asynsend.c:124-131), then hand pending_slice to senda.
        self.senda_queue
            .enqueue(endpoint, *message, AsyncSlotFlags::NO_REPLY)
            .map_err(|_| Errno::from_i32(minix_types::EBUSY))?;
        self.ipc
            .senda(self.senda_queue.pending_slice())
            .map_err(|t| Errno::from_i32(t.0))
    }

    fn safecopy_from(
        &mut self,
        source: Endpoint,
        addr: usize,
        buf: &mut [u8],
    ) -> Result<(), Errno> {
        // The sys_datacopy shape of C manager.c:141/160 (SELF = -2, replaced by the kernel).
        syscall::sys_datacopy(
            &self.kernel,
            source.0,
            addr as u64,
            syscall::SELF,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
        .map_err(Errno::from_i32)
    }

    fn safecopy_to(&mut self, dest: Endpoint, addr: usize, buf: &[u8]) -> Result<(), Errno> {
        syscall::sys_datacopy(
            &self.kernel,
            syscall::SELF,
            buf.as_ptr() as u64,
            dest.0,
            addr as u64,
            buf.len() as u64,
        )
        .map_err(Errno::from_i32)
    }

    fn ds_lookup_by_label(&mut self, label: &str) -> Option<Endpoint> {
        // DS lookup reuses the process's main grant table (DsClient no longer
        // keeps a private table; otherwise its registration would overwrite the
        // kernel's `s_grant_table` and diverge from `self.grants`, which this
        // struct's `grant_read` uses — same root cause as VFS).
        self.ds
            .retrieve_label_endpt(&mut self.grants, label)
            .ok()
            .map(|(ep, _)| ep)
    }

    fn sendrec_to(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), Errno> {
        self.ipc
            .sendrec(dest, msg)
            .map(|_| ())
            .map_err(|t| Errno::from_i32(t.0))
    }

    fn grant_read(&mut self, dest: Endpoint, addr: u64, len: u64) -> Result<GrantId, Errno> {
        let gid = self
            .grants
            .grant_direct(&self.kernel, dest.0, addr, len, minix_types::CpFlags::READ)
            .map_err(Errno::from_i32)?;
        Ok(gid)
    }

    fn grant_revoke(&mut self, grant: GrantId) {
        // [SD19-gate] Best-effort grant revocation
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.grants.revoke(grant);
    }

    fn lu_request_prepare(
        &mut self,
        ep: Endpoint,
        flags: i32,
        state_data_gid: Option<GrantId>,
        state: i32,
    ) -> Result<(), Errno> {
        // C update.c:203-227 — the three m_rs_update domains plus rs_asynsend.
        let mut msg = Message::default();
        msg.m_u.m_rs_update = {
            let mut u = minix_types::MessRsUpdate::default();
            u.result = 0;
            u.state = state;
            u.flags = flags;
            u.state_data_gid = state_data_gid.unwrap_or(-1);
            u
        };
        msg.m_type = minix_types::RS_LU_PREPARE;
        self.asynsend(ep, &msg)
    }

    fn vm_prepare(&mut self, src: Endpoint, dst: Endpoint, flags: i32) -> Result<(), Errno> {
        use minix_sys::syscall::perform_taskcall;
        let mut msg = Message::default();
        {
            // SAFETY: m_lsys_vm_update is the documented payload of VM_RS_PREPARE
            // (minix-types vm.rs:1710-1719).
            let u = unsafe { &mut msg.m_u.m_lsys_vm_update };
            u.src = src.0;
            u.dst = dst.0;
            u.flags = flags;
        }
        perform_taskcall(
            &self.ipc,
            minix_types::Endpoint::VM,
            minix_types::VM_RS_PREPARE as i32,
            &mut msg,
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod trap_api_tests {
    use super::*;

    /// Regression: consecutive `asynsend` calls accumulate into the
    /// persistent ring rather than the later send flushing the earlier one.
    /// The old stack-local single-slot form could only ever hold one entry —
    /// the confirmed boot-deadlock root cause (RS_INIT to VFS dropped by the
    /// next re-registration). Host builds answer EIO on the delegated `senda`,
    /// but `enqueue` succeeds for both, so `pending_count` reaches 2.
    #[test]
    fn asynsend_accumulates_without_flush() {
        let mut api = TrapKernelApi::new();
        let m = Message::default();
        // [SD19-gate] asynsend is fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = api.asynsend(Endpoint::VM, &m);
        // [SD19-gate] asynsend is fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = api.asynsend(Endpoint::DS, &m);
        assert_eq!(
            api.senda_queue.pending_count(),
            2,
            "second asynsend must append, not flush the first (persistent ring)"
        );
    }
}
