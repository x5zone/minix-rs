//! IPC message structure definitions.
//!
//! Minix3 uses fixed-size messages for inter-process communication.

use crate::types::Endpoint;
use crate::types::GrantId;

/// Message payload size (bytes).
///
/// C: `sizeof(message) - sizeof(m_source) - sizeof(m_type) = 64 - 4 - 4 = 56`
///
/// In 32-bit Minix3, the `message` union payload is 56 bytes, making the total
/// message 64 bytes. In 64-bit Minix-RS, we keep the same 56-byte payload size
/// for IPC protocol compatibility, adjusting field layouts (e.g. shrinking
/// char arrays, using 8-byte pointers) to fit.
pub const MESSAGE_PAYLOAD_SIZE: usize = 56;

/// Total IPC message size (bytes).
///
/// C: `sizeof(message) = 64` (m_source:4 + m_type:4 + union:56)
pub const MESSAGE_SIZE: usize = 64;

/// IPC message.
///
/// All inter-process communication in Minix3 uses this message structure.
///
/// # Memory Layout
/// ```text
/// | Field     | Size    | Offset |
/// |-----------|---------|--------|
/// | m_source  | 4 bytes | 0      |
/// | m_type    | 4 bytes | 4      |
/// | m_u       | 56 bytes| 8      |
/// | Total     | 64 bytes|        |
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Message {
    /// Message sender endpoint.
    pub m_source: Endpoint,
    /// Message type (positive=request, negative=response/error).
    pub m_type: i32,
    /// Message payload.
    pub m_u: MessageUnion,
}

/// Message payload union.
///
/// Contains multiple message formats, select the appropriate format based on `m_type`.
#[derive(Clone, Copy)]
#[repr(C)]
pub union MessageUnion {
    /// Format 1: Mixed types (int + pointer).
    pub m_m1: MessageM1,
    /// Format 2: Mixed types (int + long).
    pub m_m2: MessageM2,
    /// Format 3: Mixed types (int + char array).
    pub m_m3: MessageM3,
    /// Format 4: Pure long types.
    pub m_m4: MessageM4,
    /// Format 5: Mixed types (char + int + long).
    pub m_m5: MessageM5,
    /// Format 7: Five int args + two pointer args (VFS_PM_INIT and others
    /// in the `com.h` VFS-PM / VM-Signal / VM-Trace families).
    pub m_m7: MessageM7,
    /// Kernel: SYS_VIRCOPY / SYS_PHYSCOPY.
    pub m_lsys_krn_sys_copy: MessLsysKrnSysCopy,
    /// Kernel: SYS_UMAP / SYS_UMAP_REMOTE.
    pub m_lsys_krn_sys_umap: MessLsysKrnSysUmap,
    /// Kernel: SYS_SAFECOPYFROM / SYS_SAFECOPYTO.
    pub m_lsys_kern_safecopy: MessLsysKernSafecopy,
    /// Kernel: SYS_MEMSET.
    pub m_lsys_krn_sys_memset: MessLsysKrnSysMemset,
    /// Kernel: SYS_SAFEMEMSET.
    pub m_sys_safememset: MessSysSafememset,
    /// Kernel: SYS_VUMAP.
    pub m_lsys_krn_sys_vumap: MessLsysKrnSysVumap,
    /// Kernel: SYS_VSAFECOPY.
    pub m_lsys_kern_vsafecopy: MessLsysKernVsafecopy,
    /// Kernel: SYS_UMAP / SYS_UMAP_REMOTE reply (dst_addr writeback).
    pub m_krn_lsys_sys_umap: MessKrnLsysSysUmap,
    /// Kernel: SYS_VUMAP reply (pcount writeback).
    pub m_krn_lsys_sys_vumap: MessKrnLsysSysVumap,
    /// Kernel: SYS_GETINFO GET_WHOAMI reply.
    pub m_krn_lsys_sys_getwhoami: MessKrnLsysSysGetwhoami,
    /// Kernel: SYS_GETKSIG / SYS_ENDKSIG / SYS_KILL / SYS_SIGSEND / SYS_SIGRETURN.
    pub m_sigcalls: MessSigcalls,
    /// Kernel: SYS_STATECTL.
    pub m_lsys_krn_sys_statectl: MessLsysKrnSysStatectl,
    /// Kernel: SYS_SCHEDCTL.
    pub m_lsys_krn_schedctl: MessLsysKrnSchedctl,
    /// Kernel: SYS_TRACE.
    pub m_lsys_krn_sys_trace: MessLsysKrnSysTrace,
    /// Kernel: SYS_GETINFO.
    pub m_lsys_krn_sys_getinfo: MessLsysKrnSysGetinfo,
    /// Kernel: SYS_IRQCTL.
    pub m_lsys_krn_sys_irqctl: MessLsysKrnSysIrqctl,
    /// Kernel: SYS_SCHEDULE (user-space scheduler → kernel).
    pub m_lsys_krn_schedule: MessLsysKrnSchedule,
    /// Kernel: SCHEDULING_NO_QUANTUM (kernel → user-space scheduler).
    /// C: `mess_krn_lsys_schedule` — ipc.h:261-272.
    pub m_krn_lsys_schedule: MessKrnLsysSchedule,
    /// Scheduler: SCHEDULING_START (PM/RS → scheduler).
    /// C: `mess_lsys_sched_scheduling_start` — ipc.h:1428-1437.
    pub m_lsys_sched_scheduling_start: MessLsysSchedSchedulingStart,
    /// Scheduler: SCHEDULING_STOP (PM → scheduler).
    /// C: `mess_lsys_sched_scheduling_stop` — ipc.h:1439-1444.
    pub m_lsys_sched_scheduling_stop: MessLsysSchedSchedulingStop,
    /// Scheduler: SCHEDULING_SET_NICE (PM → scheduler).
    /// C: `mess_pm_sched_scheduling_set_nice` — ipc.h:1819-1827.
    pub m_pm_sched_scheduling_set_nice: MessPmSchedSchedulingSetNice,
    /// Scheduler: SCHEDULING_START reply (scheduler → PM).
    /// C: `mess_sched_lsys_scheduling_start` — ipc.h:1904-1912.
    pub m_sched_lsys_scheduling_start: MessSchedLsysSchedulingStart,
    /// Kernel: SYS_GETMCONTEXT / SYS_SETMCONTEXT.
    pub m_lsys_krn_sys_mcontext: MessLsysKrnSysMcontext,
    /// Kernel: SYS_EXEC.
    pub m_lsys_krn_sys_exec: MessLsysKrnSysExec,
    /// Kernel: SYS_TIMES request.
    pub m_lsys_krn_sys_times: MessLsysKrnSysTimes,
    /// Kernel: SYS_TIMES reply.
    pub m_krn_lsys_sys_times: MessKrnLsysSysTimes,
    /// Kernel: SYS_SETALARM request/reply.
    pub m_lsys_krn_sys_setalarm: MessLsysKrnSysSetalarm,
    /// TTY: TTY_FKEY_CONTROL request (IS → TTY).
    /// C: `mess_lsys_tty_fkey_ctl` — ipc.h:1447-1454.
    pub m_lsys_tty_fkey_ctl: MessLsysTtyFkeyCtl,
    /// TTY: TTY_FKEY_CONTROL reply (TTY → IS, written back in place).
    /// C: `mess_tty_lsys_fkey_ctl` — ipc.h:1925-1931.
    pub m_tty_lsys_fkey_ctl: MessTtyLsysFkeyCtl,
    /// Kernel: SYS_STIME request.
    pub m_lsys_krn_sys_stime: MessLsysKrnSysStime,
    /// Kernel: SYS_SETTIME request.
    pub m_lsys_krn_sys_settime: MessLsysKrnSysSettime,
    /// Kernel: SYS_SETGRANT request.
    pub m_lsys_krn_sys_setgrant: MessLsysKrnSysSetgrant,
    /// Kernel: SYS_DIAGCTL request.
    pub m_lsys_krn_sys_diagctl: MessLsysKrnSysDiagctl,
    /// Kernel: SYS_DEVIO request.
    pub m_lsys_krn_sys_devio: MessLsysKrnSysDevio,
    /// Kernel: SYS_DEVIO reply (input result).
    pub m_krn_lsys_sys_devio: MessKrnLsysSysDevio,
    /// Kernel: SYS_SDEVIO request (batch I/O).
    pub m_lsys_krn_sys_sdevio: MessLsysKrnSysSdevio,
    /// Kernel: SYS_VDEVIO request (batch I/O).
    pub m_lsys_krn_sys_vdevio: MessLsysKrnSysVdevio,
    /// Kernel: SYS_READBIOS request.
    pub m_lsys_krn_readbios: MessLsysKrnReadbios,
    /// Kernel: SYS_SPROF request.
    pub m_lsys_krn_sys_sprof: MessLsysKrnSysSprof,
    /// VM_PAGEFAULT notification (kernel → VM).
    pub m_vm_pagefault: MessVmPagefault,
    /// BRK request (user process → VM). C: `message.m_lc_vm_brk` — ipc.h:925
    pub m_lc_vm_brk: MessLcVmBrk,
    /// MMAP request/reply (user process ↔ VM). C: `message.m_mmap` — ipc.h:1582
    pub m_mmap: MessMmap,
    /// VFS-initiated file mapping (VFS → VM). C: `message.m_vm_vfs_mmap` — ipc.h:2369
    pub m_vm_vfs_mmap: MessVmVfsMmap,
    /// Physical memory mapping (driver → VM). C: `message.m_lsys_vm_map_phys` — ipc.h:1504
    pub m_lsys_vm_map_phys: MessLsysVmMapPhys,
    /// Remap shared region (driver → VM). C: `message.m_lsys_vm_vmremap` — ipc.h:1537
    pub m_lsys_vm_vmremap: MessLsysVmVmremap,
    /// Unmap physical mapping (driver → VM). C: `message.m_lsys_vm_unmap_phys` — ipc.h:1521
    pub m_lsys_vm_unmap_phys: MessLsysVmUnmapPhys,
    /// Unmap shared memory region (process → VM). C: `message.m_lc_vm_shm_unmap` — ipc.h:934
    pub m_lc_vm_shm_unmap: MessLcVmShmUnmap,
    /// Process-control payload (VFS/RS → VM). C: `message.m_m9` for
    /// VM_PROCCTL — `mess_9` layout (ipc.h:77-83, com.h:753-757).
    pub m_lc_vm_procctl: MessLcVmProcctl,
    /// VFS call completion payload (VFS → VM). C: `message.m_m10` for
    /// VM_VFS_REPLY — `mess_10` layout (ipc.h:86-92, com.h:708-714).
    pub m_vm_vfs_reply: MessVmVfsReply,
    /// Cache-block request payload (VFS → VM). C: `message.m_m2` for
    /// VM_MAPCACHEPAGE / VM_SETCACHEPAGE / VM_FORGETCACHEPAGE /
    /// VM_CLEARCACHE — `mess_vmmcp` layout (ipc.h:2383-2393).
    pub m_vmmcp: MessVmmcp,
    /// Cache-block map reply payload (VM → VFS) for VM_MAPCACHEPAGE.
    /// C: `message.m_m2` — `mess_vmmcp_reply` layout (ipc.h:2396-2400).
    pub m_vmmcp_reply: MessVmmcpReply,
    /// PM: procevent mask (subscriber → PM). C: `mess_lsys_pm_proceventmask` — ipc.h:1414-1420
    pub m_lsys_pm_proceventmask: MessLsysPmProceventmask,
    /// PM: process event (PM → subscriber). C: `mess_pm_lsys_proc_event` — ipc.h:1800-1813
    pub m_pm_lsys_proc_event: MessPmLsysProcEvent,
    /// PM: srv_fork params (RS → PM). C: `mess_lsys_pm_srv_fork` — ipc.h:1422-1428
    pub m_lsys_pm_srv_fork: MessLsysPmSrvFork,
    /// PM: exit status (user → PM). C: `mess_lc_pm_exit` — ipc.h:445-451
    pub m_lc_pm_exit: MessLcPmExit,
    /// PM: wait4 params (user → PM). C: `mess_lc_pm_wait4` — ipc.h:460-470
    pub m_lc_pm_wait4: MessLcPmWait4,
    /// PM: kill params (user → PM). C: `mess_lc_pm_kill` (pid, signo) — ipc.h:1880 (rs) / 535 (sig) overlay
    pub m_lc_pm_kill: MessLcPmKill,
    /// PM: srv_kill params (RS → PM). C: `mess_rs_pm_srv_kill` — ipc.h:1880-1885
    pub m_rs_pm_srv_kill: MessRsPmSrvKill,
    /// Asynchronous notification payload (mini_notify / BuildNotifyMessage).
    /// C: `mess_notify m_notify` — ipc.h:2598
    pub m_notify: crate::ipc::notify::MessNotify,
    /// MIB: sysctl(2) request (user/libc → MIB). C: `message.m_lc_mib_sysctl` — ipc.h:424
    pub m_lc_mib_sysctl: MessLcMibSysctl,
    /// MIB: sysctl(2) reply (MIB → user/libc). C: `message.m_mib_lc_sysctl` — ipc.h:1548
    pub m_mib_lc_sysctl: MessMibLcSysctl,
    /// MIB: subtree mount/unmount (service → MIB, one-way). C: `message.m_lsys_mib_register` — ipc.h:1373
    pub m_lsys_mib_register: MessLsysMibRegister,
    /// MIB: remote reply (service → MIB). C: `message.m_lsys_mib_reply` — ipc.h:1384
    pub m_lsys_mib_reply: MessLsysMibReply,
    /// MIB: relayed sysctl call (MIB → remote service). C: `message.m_mib_lsys_call` — ipc.h:1554
    pub m_mib_lsys_call: MessMibLsysCall,
    /// MIB: subtree description fetch (MIB → remote service). C: `message.m_mib_lsys_info` — ipc.h:1571
    pub m_mib_lsys_info: MessMibLsysInfo,
    /// Raw bytes.
    pub raw: [u8; MESSAGE_PAYLOAD_SIZE],
}

impl Default for MessageUnion {
    fn default() -> Self {
        Self {
            raw: [0u8; MESSAGE_PAYLOAD_SIZE],
        }
    }
}

impl MessageUnion {
    /// Const-constructible zeroed payload (for `const fn` table init).
    pub const fn zeroed() -> Self {
        Self {
            raw: [0u8; MESSAGE_PAYLOAD_SIZE],
        }
    }
}

impl Message {
    /// Const-constructible zeroed message (for `const fn` table init).
    ///
    /// `m_source` is `Endpoint(0)` (slot 0); callers that need a
    /// specific source overwrite it after construction. Used by
    /// `KProcess::new()` so that `ProcessTable` can be a `static mut`.
    pub const fn zeroed() -> Self {
        Self {
            m_source: Endpoint(0),
            m_type: 0,
            m_u: MessageUnion::zeroed(),
        }
    }

    // ── Safe payload accessors (FIX-08: R-04) ──
    //
    // These methods centralize the `debug_assert!` on `m_type` before
    // accessing the `m_u` union. In debug builds, a mismatch panics —
    // catching dispatch table bugs. In release builds, the check is
    // compiled out (the dispatch table guarantees correctness).
    //
    // The `unsafe` union access is still inside the closure, but the
    // `m_type` verification is guaranteed by this method. This is the
    // "at least centralize" option from R-04's improvement plan.
    //
    // # Why not per-field accessors?
    //
    // Per-field accessors (e.g. `fn as_sys_times(&self) -> &MessLsysKrnSysTimes`)
    // would require ~40 methods plus a syscall-constant module in
    // minix-types (currently the `Syscall` enum lives in the kernel
    // crate). The closure approach is generic and doesn't create a
    // dependency cycle.

    /// Access the message payload by reference after verifying `m_type`.
    ///
    /// # Debug-only check
    ///
    /// Panics if `m_type != expected_m_type` in debug builds. In release
    /// builds, the check is a no-op — the dispatch table guarantees the
    /// correct handler runs for each `m_type`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let req = msg.payload_ref(Syscall::Times as i32, |m| unsafe {
    ///     &m.m_u.m_lsys_krn_sys_times
    /// });
    /// ```
    #[inline]
    pub fn payload_ref<T, F, R>(&self, expected_m_type: i32, accessor: F) -> R
    where
        F: FnOnce(&Self) -> R,
    {
        debug_assert_eq!(
            self.m_type, expected_m_type,
            "m_type mismatch: expected {}, got {}",
            expected_m_type, self.m_type
        );
        accessor(self)
    }

    /// Access the message payload by mutable reference after verifying `m_type`.
    ///
    /// Same debug-only check as [`payload_ref`](Self::payload_ref).
    #[inline]
    pub fn payload_mut<T, F, R>(&mut self, expected_m_type: i32, accessor: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        debug_assert_eq!(
            self.m_type, expected_m_type,
            "m_type mismatch: expected {}, got {}",
            expected_m_type, self.m_type
        );
        accessor(self)
    }

    /// Verify `m_type` matches one of several expected values (debug-only).
    ///
    /// For union fields shared by multiple syscalls (e.g. `m_lsys_krn_sys_copy`
    /// is used by both `SYS_VIRCOPY` and `SYS_PHYSCOPY`).
    ///
    /// # Example
    ///
    /// ```ignore
    /// msg.debug_check_m_type_any(&[Syscall::Vircopy as i32, Syscall::Physcopy as i32]);
    /// let req = unsafe { &msg.m_u.m_lsys_krn_sys_copy };
    /// ```
    #[inline]
    pub fn debug_check_m_type_any(&self, expected: &[i32]) {
        debug_assert!(
            expected.contains(&self.m_type),
            "m_type {} not in expected set {:?}",
            self.m_type,
            expected
        );
    }
}

impl core::fmt::Debug for MessageUnion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "MessageUnion {{ ... }}")
    }
}

/// Message format 1: Mixed types.
///
/// Used for syscalls that need to pass pointers (e.g. read/write).
///
/// C: `mess_1` — ipc.h (64-bit layout)
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | m1i1    | i32  | 0      |
/// | m1i2    | i32  | 4      |
/// | m1i3    | i32  | 8      |
/// | (pad)   |      | 12     |
/// | m1p1    | u64  | 16     |
/// | m1p2    | u64  | 24     |
/// | m1p3    | u64  | 32     |
/// | (pad)   | 16B  | 40     |
/// | Total   | 56B  |        |
/// ```
///
/// **Note**: C's `mess_1` has `m1ull1` at offset 0 and `m1p4` at offset 48,
/// making fields shift by 8 bytes. Kernel syscalls that use specialized
/// overlays (e.g. `mess_lsys_krn_sys_irqctl`) should use dedicated types,
/// not this generic format. See `MessLsysKrnSysIrqctl` etc.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM1 {
    /// Integer argument 1.
    pub m1i1: i32,
    /// Integer argument 2.
    pub m1i2: i32,
    /// Integer argument 3.
    pub m1i3: i32,
    /// Pointer argument 1 (64-bit).
    pub m1p1: u64,
    /// Pointer argument 2 (64-bit).
    pub m1p2: u64,
    /// Pointer argument 3 (64-bit).
    pub m1p3: u64,
    /// Padding to 56 bytes (C: `mess_1` union payload size).
    pub _padding: [u8; 16],
}

/// Message format 2: Mixed types.
///
/// Used for syscalls that need to pass long type arguments.
///
/// C: `mess_2` — ipc.h (64-bit layout)
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | m2i1    | i32  | 0      |
/// | m2i2    | i32  | 4      |
/// | m2i3    | i32  | 8      |
/// | (pad)   |      | 12     |
/// | m2l1    | i64  | 16     |
/// | m2l2    | i64  | 24     |
/// | (pad)   | 24B  | 32     |
/// | Total   | 56B  |        |
/// ```
///
/// **Note**: C's `mess_2` has `m2ll1` at offset 0, shifting all fields by 8
/// bytes. It also has `m2p1`, `sigset_t`, and `m2s1` which don't fit in 56
/// bytes on 64-bit. Signal syscalls should use `MessSigcalls` instead.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM2 {
    /// Integer argument 1.
    pub m2i1: i32,
    /// Integer argument 2.
    pub m2i2: i32,
    /// Integer argument 3.
    pub m2i3: i32,
    /// Long argument 1.
    pub m2l1: i64,
    /// Long argument 2.
    pub m2l2: i64,
    /// Padding to 56 bytes (C: `mess_2` union payload size).
    pub _padding: [u8; 24],
}

/// Message format 3: Mixed types.
///
/// Used for syscalls that need to pass strings/paths (e.g. open).
///
/// C: `mess_3` — ipc.h (64-bit layout)
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type    | Offset |
/// |---------|---------|--------|
/// | m3i1    | i32     | 0      |
/// | m3i2    | i32     | 4      |
/// | m3ca1   | [u8;48] | 8      |
/// | Total   | 56B     |        |
/// ```
///
/// **Note**: C's 32-bit `mess_3` has `m3p1` (char*) and `m3ca1[44]`.
/// On 64-bit, the pointer grows from 4→8 bytes, so `m3ca1` shrinks
/// from 44→48 bytes (absorbing the pointer into the char array area)
/// to keep total at 56 bytes.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessageM3 {
    /// Integer argument 1.
    pub m3i1: i32,
    /// Integer argument 2.
    pub m3i2: i32,
    /// Character array (pathname, etc.). C: `m3ca1[44]` + `m3p1` → merged.
    pub m3ca1: [u8; 48],
}

impl Default for MessageM3 {
    fn default() -> Self {
        Self {
            m3i1: 0,
            m3i2: 0,
            m3ca1: [0u8; 48],
        }
    }
}

/// Message format 4: Pure long types.
///
/// Used for syscalls that only need to pass long type arguments.
///
/// C: `mess_4` — ipc.h (64-bit layout)
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | m4l1    | i64  | 0      |
/// | m4l2    | i64  | 8      |
/// | m4l3    | i64  | 16     |
/// | m4l4    | i64  | 24     |
/// | m4l5    | i64  | 32     |
/// | (pad)   | 16B  | 40     |
/// | Total   | 56B  |        |
/// ```
///
/// **Note**: C's `mess_4` has `m4ll1` at offset 0, shifting `m4l1-5` by 8
/// bytes. Kernel syscalls using this format should verify field offsets.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM4 {
    /// Long argument 1.
    pub m4l1: i64,
    /// Long argument 2.
    pub m4l2: i64,
    /// Long argument 3.
    pub m4l3: i64,
    /// Long argument 4.
    pub m4l4: i64,
    /// Long argument 5.
    pub m4l5: i64,
    /// Padding to 56 bytes (C: `mess_4` union payload size).
    pub _padding: [u8; 16],
}

/// Message format 5: Mixed types.
///
/// Used for syscalls that need to pass multiple type arguments.
///
/// **Note**: C has no `mess_5`. This format is a Minix-RS convenience
/// type. Consider using `MessageM7` (C: `mess_7`) instead for
/// IPC compatibility with user-space servers.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM5 {
    /// Character array.
    pub m5c1: [u8; 8],
    /// Integer argument 1.
    pub m5i1: i32,
    /// Integer argument 2.
    pub m5i2: i32,
    /// Integer argument 3.
    pub m5i3: i32,
    /// Integer argument 4.
    pub m5i4: i32,
    /// Long argument 1.
    pub m5l1: i64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 24],
}

/// Format 7: Five `i32` args + two `u64` pointer args + padding.
///
/// C: `mess_7` (com.h / ipc.h). The 64-bit Rust receiver widens the
/// `void*` slots to `u64` while the on-wire bytes for 32-bit pointers
/// remain zero-extended. Used by `VFS_PM_INIT` (`m7_i1..m7_i3`) and
/// the VM signal/trace families.
///
/// Wire layout follows the 32-bit C sender (`mess_7`, ipc.h):
/// 5 × `i32` at offsets 0/4/8/12/16, 2 × `u64` pointer slots at
/// offsets 20/28 (64-bit `usize`, zero-extended from the on-wire 32-bit
/// `char *` of `m_m7.m7_p1/m7_p2`), padding @36..56. The 64-bit Rust
/// receiver widens the 32-bit pointer slots; the high 32 bits are
/// always zero on the i386 boot path (all boot addresses are in low
/// 4 GiB and the kernel still treats the top half as "kernel VM tags"
/// for its direct map — see `kernel_info`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MessageM7 {
    /// C: `m7_i1` (payload offset 0).
    pub m7i1: i32,
    /// C: `m7_i2` (payload offset 4).
    pub m7i2: i32,
    /// C: `m7_i3` (payload offset 8).
    pub m7i3: i32,
    /// C: `m7_i4` (payload offset 12).
    pub m7i4: i32,
    /// C: `m7_i5` (payload offset 16).
    pub m7i5: i32,
    /// C: `m7_p1` (payload offset 20, `char *` on i386).
    pub m7p1: u64,
    /// C: `m7_p2` (payload offset 28, `char *` on i386).
    pub m7p2: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 20],
}

// ── Kernel-specific message types ──
//
// These are dedicated overlay types for kernel system calls.
// They correspond to the C `mess_lsys_krn_sys_*` structs in ipc.h.
// On 64-bit, the field layout differs from the generic MessageM* formats
// because C structs have different padding when mixing i32 and u64 fields.

/// SYS_VIRCOPY / SYS_PHYSCOPY message payload.
///
/// C: `mess_lsys_krn_sys_copy` — ipc.h
///
/// # 64-bit Layout
/// ```text
/// | Field      | Type | Offset |
/// |------------|------|--------|
/// | src_endpt  | i32  | 0      |
/// | (padding)  |      | 4      |
/// | src_addr   | u64  | 8      |
/// | dst_endpt  | i32  | 16     |
/// | (padding)  |      | 20     |
/// | dst_addr   | u64  | 24     |
/// | nr_bytes   | u64  | 32     |
/// | flags      | i32  | 40     |
/// | (padding)  |      | 44     |
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnSysCopy {
    /// Source endpoint. C: `src_endpt`
    pub src_endpt: i32,
    /// Source virtual/physical address. C: `src_addr`
    pub src_addr: u64,
    /// Destination endpoint. C: `dst_endpt`
    pub dst_endpt: i32,
    /// Destination virtual/physical address. C: `dst_addr`
    pub dst_addr: u64,
    /// Number of bytes to copy. C: `nr_bytes`
    pub nr_bytes: u64,
    /// Copy flags (e.g. CP_FLAG_TRY). C: `flags`
    pub flags: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 8],
}

/// SYS_UMAP / SYS_UMAP_REMOTE message payload.
///
/// C: `mess_lsys_krn_sys_umap` — ipc.h
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnSysUmap {
    /// Source endpoint. C: `src_endpt`
    pub src_endpt: i32,
    /// Segment type + index. C: `segment`
    pub segment: i32,
    /// Source virtual address / grant offset. C: `src_addr`
    pub src_addr: u64,
    /// Destination endpoint (grantee for UMAP_REMOTE). C: `dst_endpt`
    pub dst_endpt: i32,
    /// Number of bytes. C: `nr_bytes`
    pub nr_bytes: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 32],
}

/// SYS_SAFECOPYFROM / SYS_SAFECOPYTO message payload.
///
/// C: `mess_lsys_kern_safecopy` — ipc.h
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKernSafecopy {
    /// Granter endpoint. C: `from_to`
    pub from_to: i32,
    /// Grant ID. C: `gid`
    pub grant_id: i32,
    /// Offset within grant. C: `offset`
    pub offset: u64,
    /// Target address in caller's space. C: `address`
    pub address: u64,
    /// Number of bytes. C: `bytes`
    pub bytes: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 24],
}

/// SYS_MEMSET message payload.
///
/// C: `mess_lsys_krn_sys_memset` — ipc.h
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnSysMemset {
    /// Base address. C: `base`
    pub base: u64,
    /// Byte count. C: `count`
    pub count: u64,
    /// Fill pattern. C: `pattern`
    pub pattern: u64,
    /// Target process endpoint. C: `process`
    pub process: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 24],
}

/// SYS_SAFEMEMSET message payload (uses MessageM2 format).
///
/// C: `SMS_*` macros map to m2 fields — com.h:363-367
/// SMS_DST=m2_i1, SMS_PATTERN=m2_i2, SMS_GID=m2_i3,
/// SMS_OFFSET=m2_l1, SMS_BYTES=m2_l2
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessSysSafememset {
    /// Destination endpoint. C: `SMS_DST` = m2_i1
    pub dst_endpt: i32,
    /// Fill pattern. C: `SMS_PATTERN` = m2_i2
    pub pattern: i32,
    /// Grant ID. C: `SMS_GID` = m2_i3
    pub grant_id: i32,
    /// Offset within grant. C: `SMS_OFFSET` = m2_l1
    pub offset: i64,
    /// Number of bytes. C: `SMS_BYTES` = m2_l2
    pub bytes: i64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 24],
}

/// SYS_VUMAP message payload.
///
/// C: `mess_lsys_krn_sys_vumap` — ipc.h
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnSysVumap {
    /// Target endpoint. C: `endpt`
    pub endpt: i32,
    /// Virtual address of vumap_vir vector. C: `vaddr`
    pub vaddr: u64,
    /// Vector count. C: `vcount`
    pub vcount: i32,
    /// Physical address output buffer. C: `paddr`
    pub paddr: u64,
    /// Maximum physical entries. C: `pmax`
    pub pmax: i32,
    /// Access flags. C: `access`
    pub access: i32,
    /// Starting offset. C: `offset`
    pub offset: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 8],
}

/// SYS_UMAP / SYS_UMAP_REMOTE reply payload.
///
/// C: `mess_krn_lsys_sys_umap` — ipc.h:324-328
///
/// Only `dst_addr` is meaningful; the request fields are overwritten on reply.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessKrnLsysSysUmap {
    /// Resolved physical address. C: `dst_addr` (phys_bytes)
    pub dst_addr: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessKrnLsysSysUmap {
    fn default() -> Self {
        Self {
            dst_addr: 0,
            _padding: [0u8; 48],
        }
    }
}

/// SYS_VUMAP reply payload.
///
/// C: `mess_krn_lsys_sys_vumap` — ipc.h:331-335
///
/// Only `pcount` is meaningful; the request fields are overwritten on reply.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessKrnLsysSysVumap {
    /// Number of physical vector elements filled. C: `pcount` (int)
    pub pcount: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

impl Default for MessKrnLsysSysVumap {
    fn default() -> Self {
        Self {
            pcount: 0,
            _padding: [0u8; 52],
        }
    }
}

/// SYS_VSAFECOPY message payload.
///
/// C: `mess_lsys_kern_vsafecopy` — ipc.h
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKernVsafecopy {
    /// Vector address. C: `vec_addr`
    pub vec_addr: u64,
    /// Vector size. C: `vec_size`
    pub vec_size: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 40],
}

impl Default for MessLsysKernVsafecopy {
    fn default() -> Self {
        Self {
            vec_addr: 0,
            vec_size: 0,
            _padding: [0u8; 40],
        }
    }
}

/// SYS_STATECTL message payload.
///
/// C: `mess_lsys_krn_sys_statectl` — ipc.h
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | request | i32  | 0      |
/// | (pad)   |      | 4      |
/// | address | u64  | 8      |
/// | length  | i32  | 16     |
/// | padding | 36B  | 20     |
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysStatectl {
    /// Request type. C: `int request`
    pub request: i32,
    /// Address (state table or filter). C: `void *address`
    pub address: u64,
    /// Length (entries or bytes). C: `int length`
    pub length: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 36],
}

impl Default for MessLsysKrnSysStatectl {
    fn default() -> Self {
        Self {
            request: 0,
            address: 0,
            length: 0,
            _padding: [0u8; 36],
        }
    }
}

/// SYS_SCHEDCTL message payload.
///
/// C: `mess_lsys_krn_schedctl` — ipc.h:1093-1101
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | flags   | u32  | 0      |
/// | endpoint| i32  | 4      |
/// | priority| i32  | 8      |
/// | quantum | i32  | 12     |
/// | cpu     | i32  | 16     |
/// | padding | 36B  | 20     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`/`MessageM2`. Using
/// `m1.m1i1`/`m1.m1i2`/`m1.m1i3` for `flags`/`endpoint`/`priority` works
/// (same offsets), but `m2.m2i1`/`m2.m2i2` for `quantum`/`cpu` is WRONG —
/// `m2` is a union overlay at offset 0, so `m2.m2i1` reads `flags` and
/// `m2.m2i2` reads `endpoint`. Always use `MessLsysKrnSchedctl` for
/// `SYS_SCHEDCTL`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSchedctl {
    /// Schedctl flags. C: `uint32_t flags` (only `SCHEDCTL_FLAG_KERNEL` defined).
    pub flags: u32,
    /// Target process endpoint. C: `endpoint_t endpoint`.
    pub endpoint: i32,
    /// Scheduling priority (-1 = keep current). C: `int priority`.
    pub priority: i32,
    /// Scheduling quantum in ms (-1 = keep current). C: `int quantum`.
    pub quantum: i32,
    /// Target CPU (-1 = keep current). C: `int cpu`.
    pub cpu: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 36],
}

impl Default for MessLsysKrnSchedctl {
    fn default() -> Self {
        Self {
            flags: 0,
            endpoint: 0,
            priority: 0,
            quantum: 0,
            cpu: 0,
            _padding: [0u8; 36],
        }
    }
}

/// SYS_TRACE message payload.
///
/// C: `mess_lsys_krn_sys_trace` — ipc.h:1320-1330
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | request | i32  | 0      |
/// | endpt   | i32  | 4      |
/// | address | u64  | 8      |
/// | data    | i64  | 16     |
/// | padding | 40B  | 24     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`. Using `m1.m1i1` for
/// `endpt` is WRONG — `m1.m1i1` maps to offset 0 (`request`), while `endpt`
/// is at offset 4 (`m1.m1i2`). Always use `MessLsysKrnSysTrace` for SYS_TRACE.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysTrace {
    /// Trace request (T_*). C: `int request`.
    pub request: i32,
    /// Target process endpoint. C: `endpoint_t endpt`.
    pub endpt: i32,
    /// Trace address. C: `vir_bytes address` (64-bit).
    pub address: u64,
    /// Trace data. C: `long int data` (64-bit).
    pub data: i64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 40],
}

/// SYS_GETINFO message payload.
///
/// C: `mess_lsys_krn_sys_getinfo` — ipc.h:1180-1189
///
/// # 64-bit Layout
/// ```text
/// | Field    | Type | Offset |
/// |----------|------|--------|
/// | request  | i32  | 0      |
/// | endpt    | i32  | 4      |
/// | val_ptr  | u64  | 8      |
/// | val_len  | i32  | 16     |
/// | val_ptr2 | u64  | 24     |
/// | val_len2 | i32  | 32     |
/// | padding  | 20B  | 36     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`. Using `m1.m1p1` for
/// `val_ptr` is WRONG — `m1.m1p1` maps to offset 16 (`val_len`), while
/// `val_ptr` is at offset 8. Always use `MessLsysKrnSysGetinfo` for SYS_GETINFO.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysGetinfo {
    /// Info request type. C: `int request`.
    pub request: i32,
    /// Endpoint. C: `endpoint_t endpt`.
    pub endpt: i32,
    /// Pointer to store info. C: `vir_bytes val_ptr` (64-bit).
    pub val_ptr: u64,
    /// Length of info. C: `int val_len`.
    pub val_len: i32,
    /// Second pointer (optional). C: `vir_bytes val_ptr2` (64-bit).
    pub val_ptr2: u64,
    /// Second length or endpoint. C: `int val_len2_e`.
    pub val_len2_e: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 20],
}

/// SYS_IRQCTL message payload.
///
/// C: `mess_lsys_krn_sys_irqctl` — ipc.h:1220-1227
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | request | i32  | 0      |
/// | vector  | i32  | 4      |
/// | policy  | i32  | 8      |
/// | hook_id | i32  | 12     |
/// | padding | 40B  | 16     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`. Using `m1.m1p1` for
/// `hook_id` is WRONG — `m1.m1p1` maps to offset 16 (padding), while `hook_id`
/// is at offset 12. Always use `MessLsysKrnSysIrqctl` for SYS_IRQCTL.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysIrqctl {
    /// IRQ request type. C: `int request`.
    pub request: i32,
    /// IRQ vector. C: `int vector`.
    pub vector: i32,
    /// IRQ policy. C: `int policy`.
    pub policy: i32,
    /// IRQ hook ID. C: `int hook_id`.
    pub hook_id: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 40],
}

/// SYS_SCHEDULE message payload (user-space scheduler → kernel).
///
/// C: `mess_lsys_krn_schedule` — ipc.h:1102-1112
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | endpoint| i32  | 0      |
/// | quantum | i32  | 4      |
/// | priority| i32  | 8      |
/// | cpu     | i32  | 12     |
/// | niced   | i32  | 16     |
/// | padding | 36B  | 20     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`. Using `m1.m1p1` for
/// `cpu` is WRONG — `m1.m1p1` maps to offset 16 (`niced`), while `cpu` is at
/// offset 12. Always use `MessLsysKrnSchedule` for SYS_SCHEDULE.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSchedule {
    /// Target process endpoint. C: `endpoint_t endpoint`.
    pub endpoint: i32,
    /// Scheduling quantum. C: `int quantum`.
    pub quantum: i32,
    /// Scheduling priority. C: `int priority`.
    pub priority: i32,
    /// Target CPU. C: `int cpu`.
    pub cpu: i32,
    /// Niced flag. C: `int niced`.
    pub niced: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 36],
}

/// SCHEDULING_NO_QUANTUM message payload (kernel → user-space scheduler).
///
/// C: `mess_krn_lsys_schedule` — ipc.h:261-272. Sent by the kernel's
/// `notify_scheduler()` (proc.c:1860-1891) when a user-scheduled process
/// exhausts its quantum. Carries accounting stats so the user-space
/// scheduler can make informed re-scheduling decisions.
///
/// # Layout (matches C `mess_krn_lsys_schedule`)
/// ```text
/// | Field        | Type  | Offset | C field            |
/// |--------------|-------|--------|--------------------|
/// | acnt_queue   | u64   | 0      | time_in_queue (ms) |
/// | acnt_deqs    | u32   | 8      | dequeues           |
/// | acnt_ipc_sync| u32   | 12     | ipc_sync count     |
/// | acnt_ipc_async|u32   | 16     | ipc_async count    |
/// | acnt_preempt | u32   | 20     | preempted count    |
/// | acnt_cpu     | u32   | 24     | cpuid              |
/// | acnt_cpu_load| u32   | 28     | cpu_load (0..100)  |
/// | _padding     | 24B   | 32     | (C: uint8_t[24])   |
/// ```
///
/// `acnt_queue` is `time_t` (64-bit on this target) in C — converted from
/// `p_accounting.time_in_queue` (cycles) to milliseconds via
/// `cpu_time_to_ms` before sending.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessKrnLsysSchedule {
    /// Time spent in ready queue (milliseconds).
    /// C: `time_t acnt_queue` — `cpu_time_2_ms(p->p_accounting.time_in_queue)`.
    pub acnt_queue: u64,
    /// Number of times dequeued. C: `unsigned long acnt_deqs`.
    pub acnt_deqs: u32,
    /// Synchronous IPC count. C: `unsigned long acnt_ipc_sync`.
    pub acnt_ipc_sync: u32,
    /// Asynchronous IPC count. C: `unsigned long acnt_ipc_async`.
    pub acnt_ipc_async: u32,
    /// Preemption count. C: `unsigned long acnt_preempt`.
    pub acnt_preempt: u32,
    /// CPU id where quantum expired. C: `uint32_t acnt_cpu`.
    pub acnt_cpu: u32,
    /// Instantaneous CPU load (0..100). C: `uint32_t acnt_cpu_load`.
    pub acnt_cpu_load: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 24],
}

/// SCHEDULING_START message payload (PM/RS → user-space scheduler).
///
/// C: `mess_lsys_sched_scheduling_start` — ipc.h:1428-1437. Carries the
/// process to schedule, its parent (inheritance source), the maximum
/// priority, and the time quantum.
///
/// # Layout (matches C `mess_lsys_sched_scheduling_start`)
/// ```text
/// | Field    | Type | Offset | C field   |
/// |----------|------|--------|-------------|
/// | endpoint | i32  | 0      | endpoint    |
/// | parent   | i32  | 4      | parent      |
/// | maxprio  | i32  | 8      | maxprio     |
/// | quantum  | i32  | 12     | quantum     |
/// | _padding | 40B  | 16     | (C: uint8_t[40]) |
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysSchedSchedulingStart {
    /// Process to schedule. C: `endpoint_t endpoint`.
    pub endpoint: i32,
    /// Inheritance source (parent endpoint). C: `endpoint_t parent`.
    pub parent: i32,
    /// Maximum priority. C: `int maxprio`.
    pub maxprio: i32,
    /// Time quantum in ms. C: `int quantum`.
    pub quantum: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 40],
}

impl Default for MessLsysSchedSchedulingStart {
    fn default() -> Self {
        Self {
            endpoint: 0,
            parent: 0,
            maxprio: 0,
            quantum: 0,
            _padding: [0u8; 40],
        }
    }
}

/// SCHEDULING_STOP message payload (PM → user-space scheduler).
///
/// C: `mess_lsys_sched_scheduling_stop` — ipc.h:1439-1444. Names the
/// process to stop scheduling (exit path).
///
/// # Layout (matches C `mess_lsys_sched_scheduling_stop`)
/// ```text
/// | Field    | Type | Offset |
/// |----------|------|--------|
/// | endpoint | i32  | 0      |
/// | _padding | 52B  | 4      |
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysSchedSchedulingStop {
    /// Process to stop scheduling. C: `endpoint_t endpoint`.
    pub endpoint: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

impl Default for MessLsysSchedSchedulingStop {
    fn default() -> Self {
        Self {
            endpoint: 0,
            _padding: [0u8; 52],
        }
    }
}

/// SCHEDULING_SET_NICE message payload (PM → user-space scheduler).
///
/// C: `mess_pm_sched_scheduling_set_nice` — ipc.h:1819-1827. Names the
/// process and the new maximum priority.
///
/// # Layout (matches C `mess_pm_sched_scheduling_set_nice`)
/// ```text
/// | Field    | Type | Offset |
/// |----------|------|--------|
/// | endpoint | i32  | 0      |
/// | maxprio  | u32  | 4      |
/// | _padding | 48B  | 8      |
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessPmSchedSchedulingSetNice {
    /// Target process endpoint. C: `endpoint_t endpoint`.
    pub endpoint: i32,
    /// New maximum priority. C: `uint32_t maxprio`.
    pub maxprio: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessPmSchedSchedulingSetNice {
    fn default() -> Self {
        Self {
            endpoint: 0,
            maxprio: 0,
            _padding: [0u8; 48],
        }
    }
}

/// SCHEDULING_START reply payload (scheduler → PM).
///
/// C: `mess_sched_lsys_scheduling_start` — ipc.h:1904-1912. Reports which
/// scheduler took over the process (`SCHED_PROC_NR` on success).
///
/// # Layout (matches C `mess_sched_lsys_scheduling_start`)
/// ```text
/// | Field     | Type | Offset |
/// |-----------|------|--------|
/// | scheduler | i32  | 0      |
/// | _padding  | 52B  | 4      |
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessSchedLsysSchedulingStart {
    /// Scheduler endpoint that took over. C: `endpoint_t scheduler`.
    pub scheduler: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

impl Default for MessSchedLsysSchedulingStart {
    fn default() -> Self {
        Self {
            scheduler: 0,
            _padding: [0u8; 52],
        }
    }
}

/// DS value: one 32-bit lane, three meanings (`ipc.h:94-99`).
///
/// C: `union ds_val { grant; u32; ep }` — three arms share one 32-bit lane;
/// which arm reads depends on the letter (LABEL letters read the endpoint
/// arm, U32 letters the number arm, MEM/key lanes the grant arm). Reading
/// the wrong arm of a C union is UB with no guardrail, so the Rust form
/// is a newtype: constructing names the arm, reading returns its meaning.
///
/// # Layout (matches C `union ds_val`)
/// ```text
/// | Field | Type | Offset |
/// |-------|------|--------|
/// | raw   | i32  | 0      |
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct DsVal(i32);

impl DsVal {
    /// A safecopy grant lane. C: `grant` arm — `key_grant`, MEM values.
    pub const fn grant(id: GrantId) -> Self {
        Self(id)
    }

    /// A plain number lane. C: `u32` arm — U32 values.
    pub const fn number(value: u32) -> Self {
        Self(value as i32)
    }

    /// An endpoint lane. C: `ep` arm — LABEL values.
    pub const fn endpoint(ep: Endpoint) -> Self {
        Self(ep.0)
    }

    /// Read the grant lane.
    pub const fn as_grant(self) -> GrantId {
        self.0
    }

    /// Read the number lane.
    pub const fn as_number(self) -> u32 {
        self.0 as u32
    }

    /// Read the endpoint lane.
    pub const fn as_endpoint(self) -> Endpoint {
        Endpoint(self.0)
    }
}

/// DS request payload (client → DS).
///
/// C: `mess_ds_req` — ipc.h:107-115. Six lanes in order: the key grant,
/// the key length, the flags, the input value, the value length, and the
/// owner. Every DS letter fills this same shape; unused lanes stay zeroed
/// (`memset`, libsys/ds.c).
///
/// # Layout (matches C `mess_ds_req`)
/// ```text
/// | Field     | Type | Offset |
/// |-----------|------|--------|
/// | key_grant | i32  | 0      |
/// | key_len   | i32  | 4      |
/// | flags     | i32  | 8      |
/// | val_in    | i32  | 12     |
/// | val_len   | i32  | 16     |
/// | owner     | i32  | 20     |
/// | _padding  | 32B  | 24     |
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessDsReq {
    /// Key safecopy grant. C: `cp_grant_id_t key_grant`.
    pub key_grant: GrantId,
    /// Key length. C: `int key_len`.
    pub key_len: i32,
    /// Request flags. C: `int flags` (`DSF_*`, ds.h:12-26).
    pub flags: i32,
    /// Input value (arm by letter). C: `union ds_val val_in`.
    pub val_in: DsVal,
    /// Value length. C: `int val_len`.
    pub val_len: i32,
    /// Requesting owner. C: `endpoint_t owner` (wire shape; readers name it).
    pub owner: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 32],
}

/// DS reply payload (DS → client).
///
/// C: `mess_ds_reply` — ipc.h:100-105. Two lanes: the output value and
/// its length. Replies carry no key and no flags of their own (`do_check`
/// writes back into the *request* lanes instead — 10's domain).
///
/// # Layout (matches C `mess_ds_reply`)
/// ```text
/// | Field     | Type | Offset |
/// |-----------|------|--------|
/// | val_out   | i32  | 0      |
/// | val_len   | i32  | 4      |
/// | _padding  | 48B  | 8      |
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessDsReply {
    /// Output value (arm by letter). C: `union ds_val val_out`.
    pub val_out: DsVal,
    /// Value length. C: `int val_len`.
    pub val_len: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessDsReply {
    // Manual: `[u8; 48]` has no `Default` (arrays past 32 don't derive),
    // so `#[derive(Default)]` cannot apply — same as the neighbouring
    // 48/52-byte payloads in this file.
    fn default() -> Self {
        Self {
            val_out: DsVal::default(),
            val_len: 0,
            _padding: [0u8; 48],
        }
    }
}

/// FKEY control request payload (IS → TTY).
///
/// C: `mess_lsys_tty_fkey_ctl` — `minix3/minix/include/minix/ipc.h:1447-1454`
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | request | i32  | 0      |
/// | fkeys   | i32  | 4      |
/// | sfkeys  | i32  | 8      |
/// | padding | 44B  | 12     |
/// | Total   | 56B  |        |
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysTtyFkeyCtl {
    /// Sub-command (`FKEY_MAP`/`FKEY_UNMAP`/`FKEY_EVENTS`). C: `int request`.
    pub request: i32,
    /// F1-F12 bitmap, bits 1-12 (bit 0 unused). C: `int fkeys`.
    pub fkeys: i32,
    /// Shift F1-F12 bitmap, bits 1-12. C: `int sfkeys`.
    pub sfkeys: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 44],
}

impl Default for MessLsysTtyFkeyCtl {
    // Manual `Default` — same neighbouring-payload convention as above.
    fn default() -> Self {
        Self {
            request: 0,
            fkeys: 0,
            sfkeys: 0,
            _padding: [0u8; 44],
        }
    }
}

/// FKEY control reply payload (TTY → IS, written back in place).
///
/// C: `mess_tty_lsys_fkey_ctl` — `minix3/minix/include/minix/ipc.h:1925-1931`
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | fkeys   | i32  | 0      |
/// | sfkeys  | i32  | 4      |
/// | padding | 48B  | 8      |
/// | Total   | 56B  |        |
/// ```
///
/// Only successfully consumed bits are cleared (`bit_unset`); leftover bits
/// are returned as-is for retry/diagnosis
/// (`minix3/minix/drivers/tty/tty/arch/i386/keyboard.c:439-526`).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessTtyLsysFkeyCtl {
    /// Remaining (unconsumed) F1-F12 bits. C: `int fkeys`.
    pub fkeys: i32,
    /// Remaining Shift F1-F12 bits. C: `int sfkeys`.
    pub sfkeys: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessTtyLsysFkeyCtl {
    // Manual `Default` — same neighbouring-payload convention as above.
    fn default() -> Self {
        Self {
            fkeys: 0,
            sfkeys: 0,
            _padding: [0u8; 48],
        }
    }
}

/// SYS_GETMCONTEXT / SYS_SETMCONTEXT message payload.
///
/// C: `mess_lsys_krn_sys_getmcontext` / `mess_lsys_krn_sys_setmcontext` — ipc.h:1188-1198, 1264-1274
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | endpt   | i32  | 0      |
/// | ctx_ptr | u64  | 8      |
/// | padding | 48B  | 16     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`. Using `m1.m1p1` for
/// `ctx_ptr` is WRONG — `m1.m1p1` maps to offset 16 (padding), while `ctx_ptr`
/// is at offset 8. Always use `MessLsysKrnSysMcontext` for GET/SETMCONTEXT.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysMcontext {
    /// Target process endpoint. C: `endpoint_t endpt`.
    pub endpt: i32,
    /// Pointer to machine context. C: `vir_bytes ctx_ptr` (64-bit).
    pub ctx_ptr: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

/// SYS_EXEC message payload.
///
/// C: `mess_lsys_krn_sys_exec` — ipc.h:1159-1168
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | endpt   | i32  | 0      |
/// | (pad)   | 4B   | 4      |
/// | ip      | u64  | 8      |
/// | stack   | u64  | 16     |
/// | name    | u64  | 24     |
/// | ps_str  | u64  | 32     |
/// | padding | 16B  | 40     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`. Using `m1.m1p1` for
/// `name` is WRONG — `m1.m1p1` maps to offset 16 (`stack`), while `name` is
/// at offset 24. Always use `MessLsysKrnSysExec` for SYS_EXEC.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysExec {
    /// Target process endpoint. C: `endpoint_t endpt`.
    pub endpt: i32,
    /// New instruction pointer. C: `vir_bytes ip` (64-bit).
    /// Auto-padded to offset 8 by `repr(C)`.
    pub ip: u64,
    /// New stack pointer. C: `vir_bytes stack`.
    pub stack: u64,
    /// Pointer to process name (in caller's address space). C: `vir_bytes name`.
    pub name: u64,
    /// ps_strings pointer. C: `vir_bytes ps_str`.
    pub ps_str: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 16],
}

/// SYS_GETKSIG / SYS_ENDKSIG / SYS_KILL / SYS_SIGSEND / SYS_SIGRETURN message payload.
///
/// C: `mess_sigcalls` — ipc.h
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | map     | u64  | 0      |
/// | endpt   | i32  | 8      |
/// | sig     | i32  | 12     |
/// | sigctx  | u64  | 16     |
/// | padding | 32B  | 24     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`. Using `m1.m1i1` for
/// `endpt` is WRONG — `m1.m1i1` maps to offset 0 (low 4 bytes of `map`),
/// while `endpt` is at offset 8. Always use `MessSigcalls` for signal syscalls.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessSigcalls {
    /// Signal bitmap. C: `sigset_t map`
    pub map: u64,
    /// Process endpoint. C: `endpoint_t endpt`
    pub endpt: i32,
    /// Signal number. C: `int sig`
    pub sig: i32,
    /// Pointer to signal context. C: `void *sigctx`
    pub sigctx: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 32],
}

/// SYS_GETINFO (GET_WHOAMI) reply message payload.
///
/// C: `mess_krn_lsys_sys_getwhoami` — ipc.h
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessKrnLsysSysGetwhoami {
    /// Caller endpoint. C: `endpt`
    pub endpt: i32,
    /// Caller privilege flags. C: `privflags`
    pub privflags: i32,
    /// Caller init flags. C: `initflags`
    pub initflags: i32,
    /// Caller process name. C: `name[44]`
    pub name: [u8; 44],
}

impl Default for MessKrnLsysSysGetwhoami {
    fn default() -> Self {
        Self {
            endpt: 0,
            privflags: 0,
            initflags: 0,
            name: [0u8; 44],
        }
    }
}

/// SYS_TIMES request message payload.
///
/// C: `mess_lsys_krn_sys_times` — ipc.h
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | endpt   | i32  | 0      |
/// | padding | 52B  | 4      |
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysTimes {
    /// Target process endpoint. C: `endpoint_t endpt`
    pub endpt: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

impl Default for MessLsysKrnSysTimes {
    fn default() -> Self {
        Self {
            endpt: 0,
            _padding: [0u8; 52],
        }
    }
}

/// SYS_TIMES reply message payload.
///
/// C: `mess_krn_lsys_sys_times` — ipc.h
///
/// # 64-bit Layout
/// ```text
/// | Field       | Type | Offset |
/// |-------------|------|--------|
/// | real_ticks  | u64  | 0      |
/// | boot_ticks  | u64  | 8      |
/// | user_time   | u64  | 16     |
/// | system_time | u64  | 24     |
/// | boot_time   | u64  | 32     |
/// | padding     | 16B  | 40     |
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessKrnLsysSysTimes {
    /// Wall-clock ticks since boot. C: `clock_t real_ticks`
    pub real_ticks: u64,
    /// Monotonic ticks since boot. C: `clock_t boot_ticks`
    pub boot_ticks: u64,
    /// User-mode time in ticks. C: `clock_t user_time`
    pub user_time: u64,
    /// System-mode time in ticks. C: `clock_t system_time`
    pub system_time: u64,
    /// Boot time in seconds since epoch. C: `time_t boot_time`
    pub boot_time: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 16],
}

/// SYS_SETALARM request/reply message payload.
///
/// C: `mess_lsys_krn_sys_setalarm` — ipc.h
///
/// # 64-bit Layout
/// ```text
/// | Field     | Type | Offset |
/// |-----------|------|--------|
/// | exp_time  | u64  | 0      |
/// | time_left | u64  | 8      |
/// | uptime    | u64  | 16     |
/// | abs_time  | i32  | 24     |
/// | padding   | 28B  | 28     |
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnSysSetalarm {
    /// Expiration time for the alarm. C: `clock_t exp_time`
    pub exp_time: u64,
    /// Ticks left on previous alarm (reply). C: `clock_t time_left`
    pub time_left: u64,
    /// Current uptime (reply). C: `clock_t uptime`
    pub uptime: u64,
    /// Whether exp_time is absolute. C: `int abs_time`
    pub abs_time: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 28],
}

/// SYS_STIME request message payload.
///
/// C: `mess_lsys_krn_sys_stime` — ipc.h
///
/// # 64-bit Layout
/// ```text
/// | Field     | Type | Offset |
/// |-----------|------|--------|
/// | boot_time | u64  | 0      |
/// | padding   | 48B  | 8      |
/// ```
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysStime {
    /// Boot time in seconds since epoch. C: `time_t boot_time`
    pub boot_time: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessLsysKrnSysStime {
    fn default() -> Self {
        Self {
            boot_time: 0,
            _padding: [0u8; 48],
        }
    }
}

/// SYS_SETTIME request message payload.
///
/// C: `mess_lsys_krn_sys_settime` — ipc.h
///
/// # 64-bit Layout
/// ```text
/// | Field    | Type | Offset |
/// |----------|------|--------|
/// | sec      | u64  | 0      |
/// | nsec     | i64  | 8      |
/// | now      | i32  | 16     |
/// | clock_id | i32  | 20     |
/// | padding  | 32B  | 24     |
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnSysSettime {
    /// Time in seconds since 1970. C: `time_t sec`
    pub sec: u64,
    /// Nanosecond component. C: `long int nsec`
    pub nsec: i64,
    /// Non-zero for immediate set, 0 for adjtime. C: `int now`
    pub now: i32,
    /// Clock ID (CLOCK_REALTIME=0). C: `clockid_t clock_id`
    pub clock_id: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 32],
}

/// Kernel: SYS_SETGRANT request.
///
/// C: `mess_lsys_krn_sys_setgrant` in ipc.h:1260-1265.
/// Sets the grant table location in the caller's privilege structure.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysSetgrant {
    /// Address of grant table in caller's address space (cp_grant_t *).
    pub addr: u64,
    /// Number of grant entries.
    pub size: i32,
    /// Padding to 56 bytes (C: union payload size).
    _padding: [u8; 44],
}

impl Default for MessLsysKrnSysSetgrant {
    fn default() -> Self {
        Self {
            addr: 0,
            size: 0,
            _padding: [0u8; 44],
        }
    }
}

/// Kernel: SYS_DIAGCTL request.
///
/// C: `mess_lsys_krn_sys_diagctl` in ipc.h.
/// Diagnostic control: kernel message output, stack traces, signal registration.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnSysDiagctl {
    /// Request code (DIAGCTL_CODE_DIAG/STACKTRACE/REGISTER/UNREGISTER).
    pub code: i32,
    /// Buffer address for DIAG output.
    pub buf: u64,
    /// Buffer length for DIAG output.
    pub len: u64,
    /// Target endpoint for STACKTRACE.
    pub endpt: i32,
    /// Padding to 56 bytes (C: union payload size).
    _padding: [u8; 28],
}

/// Kernel: SYS_DEVIO request.
///
/// C: `mess_lsys_krn_sys_devio` — ipc.h:1139-1147
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | request | i32  | 0      |
/// | port    | i32  | 4      |
/// | value   | u32  | 8      |
/// | padding | 44B  | 12     |
/// ```
///
/// C field types are `int`/`uint32_t` — no 64-bit widening applies, so the
/// layout matches C on both i386 and x86_64. `value` lives at offset 8,
/// which is `MessageM1::m1i3` (NOT `m1p1` @16 — reading `m1p1` would land
/// in padding, the same field-mapping bug class documented in
/// `MessLsysKrnSysIrqctl`). Always use this dedicated variant for SYS_DEVIO.
///
/// The kernel reply uses the separate `MessKrnLsysSysDevio` (value@0).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysDevio {
    /// Request type + direction (_DIO_TYPEMASK | _DIO_DIRMASK).
    pub request: i32,
    /// I/O port address. C: `int port`
    pub port: i32,
    /// Value to write (output). C: `uint32_t value`
    pub value: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 44],
}

impl Default for MessLsysKrnSysDevio {
    fn default() -> Self {
        Self {
            request: 0,
            port: 0,
            value: 0,
            _padding: [0u8; 44],
        }
    }
}

/// Kernel: SYS_DEVIO reply.
///
/// C: `mess_krn_lsys_sys_devio` — ipc.h:276-280
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | value   | u32  | 0      |
/// | padding | 52B  | 4      |
/// ```
///
/// Separate reply struct: the kernel writes the input result at offset 0
/// (i.e. `MessageM1::m1i1`), NOT at `m1p1` @16.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessKrnLsysSysDevio {
    /// Value read (input). C: `uint32_t value`
    pub value: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

impl Default for MessKrnLsysSysDevio {
    fn default() -> Self {
        Self {
            value: 0,
            _padding: [0u8; 52],
        }
    }
}

/// Kernel: SYS_VDEVIO request (batch I/O).
///
/// C: `mess_lsys_krn_sys_vdevio` — ipc.h:1343-1351
///
/// # 64-bit Layout
/// ```text
/// | Field    | Type | Offset |
/// |----------|------|--------|
/// | request  | i32  | 0      |
/// | vec_size | i32  | 4      |
/// | vec_addr | u64  | 8      |
/// | padding  | 40B  | 16     |
/// ```
///
/// C `vec_addr` is `vir_bytes` (`unsigned long`); widened to u64 per the
/// project 64-bit convention — offset 8 is unchanged on both i386 and
/// x86_64. `MessageM1` cannot express this layout (no u64 field at offset
/// 8); `m1p1` @16 would misparse. Always use this dedicated variant.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessLsysKrnSysVdevio {
    /// Request type + direction (_DIO_TYPEMASK | _DIO_DIRMASK).
    pub request: i32,
    /// Number of (port,value) pairs. C: `int vec_size`
    pub vec_size: i32,
    /// User-space pair array. C: `vir_bytes vec_addr` (pv{b,w,l}_pair_t *)
    pub vec_addr: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 40],
}

impl Default for MessLsysKrnSysVdevio {
    fn default() -> Self {
        Self {
            request: 0,
            vec_size: 0,
            vec_addr: 0,
            _padding: [0u8; 40],
        }
    }
}

/// Kernel: SYS_SDEVIO request (batch I/O).
///
/// C: `mess_lsys_krn_sys_sdevio` — ipc.h:1239-1247
///
/// # 64-bit Layout
/// ```text
/// | Field     | Type | Offset |
/// |-----------|------|--------|
/// | request   | i32  | 0      |
/// | (pad)     | 4B   | 4      |
/// | port      | i64  | 8      |
/// | vec_endpt | i32  | 16     |
/// | (pad)     | 4B   | 20     |
/// | vec_addr  | u64  | 24     |
/// | vec_size  | u64  | 32     |
/// | offset    | u64  | 40     |
/// | (pad)     | 8B   | 48     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`. Always use
/// `MessLsysKrnSysSdevio` for SYS_SDEVIO, not the generic `m1` overlay.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnSysSdevio {
    /// Request type + direction + safe flag. C: `int request`
    pub request: i32,
    pub _pad0: [u8; 4],
    /// I/O port address. C: `long int port` (64-bit long on 64-bit platforms)
    pub port: i64,
    /// Target process endpoint for buffer. C: `endpoint_t vec_endpt`
    pub vec_endpt: i32,
    pub _pad1: [u8; 4],
    /// Virtual address of buffer or grant ID. C: `phys_bytes vec_addr`
    pub vec_addr: u64,
    /// Number of elements. C: `vir_bytes vec_size`
    pub vec_size: u64,
    /// Offset into the grant. C: `vir_bytes offset`
    pub offset: u64,
    pub _padding: [u8; 8],
}

/// Kernel: SYS_READBIOS request.
///
/// C: `mess_lsys_krn_readbios` — ipc.h:1076-1081
///
/// # 64-bit Layout
/// ```text
/// | Field   | Type | Offset |
/// |---------|------|--------|
/// | size    | u64  | 0      |
/// | addr    | u64  | 8      |
/// | buf     | u64  | 16     |
/// | padding | 32B  | 24     |
/// ```
///
/// **IMPORTANT**: This layout differs from `MessageM1`. Always use
/// `MessLsysKrnReadbios` for SYS_READBIOS, not the generic `m1` overlay.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnReadbios {
    /// Number of bytes to copy. C: `size_t size`
    pub size: u64,
    /// Absolute address in BIOS area. C: `phys_bytes addr`
    pub addr: u64,
    /// Buffer address in requesting process. C: `vir_bytes buf`
    pub buf: u64,
    pub _padding: [u8; 32],
}

/// Kernel: SYS_SPROF request.
///
/// C: `mess_lsys_krn_sys_sprof` in ipc.h.
/// Start/stop statistical profiling.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessLsysKrnSysSprof {
    /// Action: PROF_START (0) or PROF_STOP (1). C: `action`
    pub action: i32,
    /// Requested sample frequency. C: `freq`
    pub freq: i32,
    /// Interrupt source: PROF_RTC (0) or PROF_NMI (1). C: `intr_type`
    pub intr_type: i32,
    /// Endpoint of caller. C: `endpt`
    pub endpt: i32,
    /// User address of info struct. C: `ctl_ptr`
    pub ctl_ptr: u64,
    /// User address of memory for data. C: `mem_ptr`
    pub mem_ptr: u64,
    /// Available memory for data. C: `mem_size`
    pub mem_size: u64,
    /// Padding to 56 bytes (C: union payload size).
    _padding: [u8; 16],
}

/// VM_PAGEFAULT notification (kernel → VM).
///
/// C: `message.m_vm_pagefault` — kernel/ipc.h + kernel/proto.h
///
/// Sent by the assembly trap handler (`pagefault()` in exception.c)
/// when a user-mode page fault occurs. VM resolves the PTE and replies
/// (implicit) by clearing `RTS_PAGEFAULT` via `SYS_VMCTL ClearPageFault`.
///
/// Size: 8 (vpf_addr) + 4 (vpf_flags) + 4 (vpf_padding) = 16 bytes.
/// Padded to 56 bytes to match C union size.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessVmPagefault {
    /// Faulting virtual address. C: `VPF_ADDR`
    pub vpf_addr: u64,
    /// Error code from CPU (x86 errcode; ARM DFSR; RISC-V stval). C: `VPF_FLAGS`
    pub vpf_flags: u32,
    /// Padding to align to 56 bytes.
    pub vpf_padding: u32,
    /// Reserved bytes for union size alignment.
    _padding: [u8; 40],
}

impl Default for MessVmPagefault {
    fn default() -> Self {
        Self {
            vpf_addr: 0,
            vpf_flags: 0,
            vpf_padding: 0,
            _padding: [0u8; 40],
        }
    }
}

/// BRK request payload (user process → VM).
///
/// C: `message.m_lc_vm_brk` — `mess_lc_vm_brk { void *addr; uint8_t padding[52]; }`
/// (minix3/minix/include/minix/ipc.h:918-926).
///
/// libc `brk()` sends only the new break address; the caller endpoint is
/// carried in `m_source` (kernel-set, spoof-proof) — the same convention as
/// `VM_PAGEFAULT`. A 32-bit C sender writes `addr` at payload offset 0 and
/// zeroes the rest (libc brk.c does `memset(&m, 0, sizeof(m))`), so the
/// 64-bit u64 read zero-extends correctly.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessLcVmBrk {
    /// New break address. C: `mess_lc_vm_brk.addr`
    pub addr: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessLcVmBrk {
    fn default() -> Self {
        Self {
            addr: 0,
            _padding: [0; 48],
        }
    }
}

/// MMAP request/reply payload (user process ↔ VM).
///
/// C: `message.m_mmap` — `mess_mmap { off_t offset; void *addr; size_t len;
/// int prot; int flags; int fd; endpoint_t forwhom; void *retaddr;
/// u32_t padding[5]; }` (minix3/minix/include/minix/ipc.h:1582-1592).
///
/// Wire layout follows the 32-bit C sender (libc `minix_mmap_for`, libc/sys/mmap.c):
/// `offset` is 64-bit at payload offset 0; the remaining fields are 32-bit on
/// the wire (`size_t`/`void*`/`int`/`endpoint_t` are 4 bytes on i386). The
/// 64-bit Rust receiver zero-extends the 32-bit fields. `retaddr` is the reply
/// field the kernel echoes back to libc after a successful map.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MessMmap {
    /// File offset. C: `mess_mmap.offset` (payload offset 0)
    pub offset: u64,
    /// Requested address hint. C: `mess_mmap.addr` (payload offset 8)
    pub addr: u32,
    /// Mapping length in bytes. C: `mess_mmap.len` (payload offset 12)
    pub len: u32,
    /// Protection flags (PROT_*). C: `mess_mmap.prot` (payload offset 16)
    pub prot: i32,
    /// Mapping flags (MAP_*). C: `mess_mmap.flags` (payload offset 20)
    pub flags: i32,
    /// File descriptor, -1 for anonymous. C: `mess_mmap.fd` (payload offset 24)
    pub fd: i32,
    /// Target endpoint for MAP_THIRDPARTY. C: `mess_mmap.forwhom` (payload offset 28)
    pub forwhom: i32,
    /// Reply: mapped address. C: `mess_mmap.retaddr` (payload offset 32)
    pub retaddr: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 20],
}

/// VFS-initiated file mapping payload (VFS → VM).
///
/// C: `message.m_vm_vfs_mmap` — `mess_vm_vfs_mmap { off_t offset; dev_t dev;
/// ino_t ino; endpoint_t who; u32_t vaddr; u32_t len; u32_t flags; u32_t fd;
/// u16_t clearend; uint8_t padding[8]; }` (minix3/minix/include/minix/ipc.h:2369-2380).
///
/// Wire layout follows the 32-bit C sender (`minix_vfs_mmap`, libc/sys/mmap.c):
/// the three 64-bit fields (`off_t`/`dev_t`/`ino_t`) sit at offsets 0/8/16,
/// the 32-bit fields follow at 24-43, `clearend` at 44.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MessVmVfsMmap {
    /// File offset. C: `mess_vm_vfs_mmap.offset` (payload offset 0)
    pub offset: u64,
    /// Device number. C: `mess_vm_vfs_mmap.dev` (payload offset 8)
    pub dev: u64,
    /// Inode number. C: `mess_vm_vfs_mmap.ino` (payload offset 16)
    pub ino: u64,
    /// Target process endpoint. C: `mess_vm_vfs_mmap.who` (payload offset 24)
    pub who: i32,
    /// Virtual address to map at. C: `mess_vm_vfs_mmap.vaddr` (payload offset 28)
    pub vaddr: u32,
    /// Mapping length in bytes. C: `mess_vm_vfs_mmap.len` (payload offset 32)
    pub len: u32,
    /// Writable flag (bit 0) passed through to `mmap_file`. C: `flags` (payload offset 36)
    pub flags: u32,
    /// File descriptor. C: `mess_vm_vfs_mmap.fd` (payload offset 40)
    pub fd: u32,
    /// Zero-padding at the end of the last page (COW block). C: `clearend` (payload offset 44)
    pub clearend: u16,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 8],
}

/// Physical memory mapping payload (driver → VM).
///
/// C: `message.m_lsys_vm_map_phys` — `mess_lsys_vm_map_phys { endpoint_t ep;
/// phys_bytes phaddr; size_t len; void *reply; uint8_t padding[40]; }`
/// (minix3/minix/include/minix/ipc.h:1504-1510).
///
/// Wire layout follows the 32-bit C sender (`sys_vm_map_phys` / driver libs):
/// `ep` at offset 0, `phaddr` (`unsigned long`, 4 bytes on i386) at offset 4,
/// `len` at offset 8, `reply` at offset 12.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessLsysVmMapPhys {
    /// Target process endpoint. C: `mess_lsys_vm_map_phys.ep` (payload offset 0)
    pub ep: i32,
    /// Physical address to map. C: `mess_lsys_vm_map_phys.phaddr` (payload offset 4)
    pub phaddr: u32,
    /// Length in bytes. C: `mess_lsys_vm_map_phys.len` (payload offset 8)
    pub len: u32,
    /// Reply: mapped virtual address. C: `mess_lsys_vm_map_phys.reply` (payload offset 12)
    pub reply: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 40],
}

impl Default for MessLsysVmMapPhys {
    fn default() -> Self {
        Self {
            ep: 0,
            phaddr: 0,
            len: 0,
            reply: 0,
            _padding: [0; 40],
        }
    }
}

/// Shared-region remap payload (driver → VM).
///
/// C: `message.m_lsys_vm_vmremap` — `mess_lsys_vm_vmremap { endpoint_t destination;
/// endpoint_t source; void *dest_addr; void *src_addr; size_t size;
/// void *ret_addr; uint8_t padding[32]; }` (minix3/minix/include/minix/ipc.h:1537-1545).
///
/// Wire layout follows the 32-bit C sender (`vm_remap`/`vm_remap_ro`, libc/sys/mmap.c):
/// both endpoints at offsets 0/4, both addresses at offsets 8/12, `size` at 16,
/// `ret_addr` at 20. The destination endpoint is an explicit message field,
/// NOT derived from `m_source` (the IPC server remaps into its client:
/// minix3/minix/servers/ipc/shm.c:159).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MessLsysVmVmremap {
    /// Destination process endpoint (receives the shared region).
    /// C: `mess_lsys_vm_vmremap.destination` (payload offset 0)
    pub destination: i32,
    /// Source process endpoint (owns the region being shared).
    /// C: `mess_lsys_vm_vmremap.source` (payload offset 4)
    pub source: i32,
    /// Requested destination address, 0 = anywhere. C: `dest_addr` (payload offset 8)
    pub dest_addr: u32,
    /// Source address, must be the start of a region. C: `src_addr` (payload offset 12)
    pub src_addr: u32,
    /// Size of the region. C: `mess_lsys_vm_vmremap.size` (payload offset 16)
    pub size: u32,
    /// Reply: mapped destination address. C: `mess_lsys_vm_vmremap.ret_addr` (payload offset 20)
    pub ret_addr: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 32],
}

/// Unmap-physical-mapping payload (driver → VM).
///
/// C: `message.m_lsys_vm_unmap_phys` — `mess_lsys_vm_unmap_phys { endpoint_t ep;
/// void *vaddr; uint8_t padding[48]; }` (minix3/minix/include/minix/ipc.h:1521-1527).
///
/// Wire layout follows the 32-bit C sender (`vm_unmap_phys`, libc/sys/mmap.c):
/// `ep` at offset 0, `vaddr` at offset 4. The target endpoint is an explicit
/// message field — unlike VM_MUNMAP, the driver unmaps on behalf of another
/// process (e.g. a device driver that mapped memory for a client).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessLsysVmUnmapPhys {
    /// Target process endpoint. C: `mess_lsys_vm_unmap_phys.ep` (payload offset 0)
    pub ep: i32,
    /// Virtual address to unmap. C: `mess_lsys_vm_unmap_phys.vaddr` (payload offset 4)
    pub vaddr: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessLsysVmUnmapPhys {
    fn default() -> Self {
        Self {
            ep: 0,
            vaddr: 0,
            _padding: [0; 48],
        }
    }
}

/// Shared-memory-unmap payload (process → VM).
///
/// C: `message.m_lc_vm_shm_unmap` — `mess_lc_vm_shm_unmap { endpoint_t forwhom;
/// void *addr; uint8_t padding[48]; }` (minix3/minix/include/minix/ipc.h:934-940).
///
/// Wire layout follows the 32-bit C sender (`vm_shm_unmap`, libc/sys/mmap.c):
/// `forwhom` at offset 0, `addr` at offset 4. `forwhom` is an explicit message
/// field so a process can unmap a shared region it mapped for another process.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessLcVmShmUnmap {
    /// Target process endpoint. C: `mess_lc_vm_shm_unmap.forwhom` (payload offset 0)
    pub forwhom: i32,
    /// Address of the shared region to unmap. C: `mess_lc_vm_shm_unmap.addr` (payload offset 4)
    pub addr: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessLcVmShmUnmap {
    fn default() -> Self {
        Self {
            forwhom: 0,
            addr: 0,
            _padding: [0; 48],
        }
    }
}

/// Process-control payload (VFS/RS → VM) for `VM_PROCCTL`.
///
/// C: `message.m_m9` — `mess_9 { uint64_t m9ull1, m9ull2; long m9l1..m9l5;
/// short m9s1..m9s4; uint8_t padding[12]; }` (minix3/minix/include/minix/ipc.h:77-83).
/// The `VMPCTL_*` field macros alias the five longs
/// (minix3/minix/include/minix/com.h:753-757).
///
/// Wire layout follows the 32-bit C sender (`vm_procctl`, libsys/vm_procctl.c;
/// `vm_vfs_procctl_handlemem`, vfs/comm.c:198-217): on i386 `long` is 4 bytes,
/// so `VMPCTL_PARAM` is at offset 16, `VMPCTL_WHO` at 20, `VMPCTL_M1` at 24,
/// `VMPCTL_LEN` at 28, `VMPCTL_FLAGS` at 32. The 64-bit Rust receiver reads
/// the 32-bit fields from the same offsets.
///
/// Do NOT decode VM_PROCCTL from `MessageM1` — same wire-format family as
/// 21-P1-1 / 19-P1-1 / 16-P0-1 (the old M1 decode re-mapped the five fields
/// to different offsets, breaking C-layout compatibility).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MessLcVmProcctl {
    /// C: `mess_9.m9ull1` (payload offset 0) — unused by VM_PROCCTL.
    pub ull1: u64,
    /// C: `mess_9.m9ull2` (payload offset 8) — unused by VM_PROCCTL.
    pub ull2: u64,
    /// Operation code. C: `VMPCTL_PARAM` = `m9_l1` (payload offset 16)
    pub param: i32,
    /// Target endpoint. C: `VMPCTL_WHO` = `m9_l2` (payload offset 20)
    pub who: i32,
    /// User-space pointer / extra parameter. C: `VMPCTL_M1` = `m9_l3`
    /// (payload offset 24; `vir_bytes` is 4 bytes on i386)
    pub m1: u32,
    /// Byte count. C: `VMPCTL_LEN` = `m9_l4` (payload offset 28)
    pub len: i32,
    /// Write flag for `VMPPARAM_HANDLEMEM`. C: `VMPCTL_FLAGS` = `m9_l5`
    /// (payload offset 32)
    pub flags: i32,
    /// C: `mess_9.m9s1..m9s4` (payload offset 36) — unused by VM_PROCCTL.
    pub shorts: [i16; 4],
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 12],
}

/// VFS call completion payload (VFS → VM) for `VM_VFS_REPLY`.
///
/// C: `message.m_m10` — `mess_10 { uint64_t m10ull1; int m10i1..m10i4;
/// long m10l1..m10l3; uint8_t padding[20]; }`
/// (minix3/minix/include/minix/ipc.h:86-92). The `VMV_*` field macros
/// alias these members (minix3/minix/include/minix/com.h:708-714).
///
/// Wire layout follows the 32-bit C sender (`do_vm_call`, vfs/misc.c:383-473):
/// on i386 `long` is 4 bytes, so `VMV_ENDPOINT` is at payload offset 8,
/// `VMV_RESULT` at 12, `VMV_REQID` at 16, `VMV_DEV` at 20, `VMV_INO` at 24,
/// `VMV_FD` at 28, `VMV_SIZE_PAGES` at 32. The 64-bit Rust receiver reads
/// the fields from the same offsets.
///
/// Do NOT decode VM_VFS_REPLY from `MessageM1` — same wire-format family as
/// 22-P0-1 / 21-P1-1 / 19-P1-1 / 16-P0-1 (the old M1 decode read the m10
/// payload at mess_1 offsets, shifting every field and mapping `reqid` to
/// the real `VMV_ENDPOINT`).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MessVmVfsReply {
    /// C: `mess_10.m10ull1` (payload offset 0) — unused by VM_VFS_REPLY.
    pub ull1: u64,
    /// Endpoint that completed the call. C: `VMV_ENDPOINT` = `m10_i1`
    /// (payload offset 8)
    pub endpoint: i32,
    /// Result of the VFS call. C: `VMV_RESULT` = `m10_i2` (payload offset 12)
    pub result: i32,
    /// Request id (matches `VFS_VMCALL_REQID`). C: `VMV_REQID` = `m10_i3`
    /// (payload offset 16)
    pub reqid: i32,
    /// Device number. C: `VMV_DEV` = `m10_i4` (payload offset 20)
    pub dev: i32,
    /// Inode number. C: `VMV_INO` = `m10_l1` (payload offset 24)
    pub ino: u32,
    /// File descriptor. C: `VMV_FD` = `m10_l2` (payload offset 28)
    pub fd: u32,
    /// File size in pages. C: `VMV_SIZE_PAGES` = `m10_l3` (payload offset 32)
    pub size_pages: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 20],
}

/// Cache-block request payload (VFS → VM) for `VM_MAPCACHEPAGE` /
/// `VM_SETCACHEPAGE` / `VM_FORGETCACHEPAGE` / `VM_CLEARCACHE`.
///
/// C: `message.m_m2` — `mess_vmmcp` (minix3/minix/include/minix/ipc.h:2383-2393):
/// ```c
/// typedef struct {
///     dev_t dev;          /* 64-bit */
///     off_t dev_offset;   /* 64-bit */
///     off_t ino_offset;   /* 64-bit */
///     ino_t ino;          /* 64-bit */
///     void *block;        /* 32-bit on i386 */
///     u32_t *flags_ptr;   /* 32-bit on i386 */
///     u8_t pages;
///     u8_t flags;
///     uint8_t padding[12];
/// } mess_vmmcp;
/// ```
/// Wire layout follows the 32-bit C sender (`vm_cachecall`, libsys/vm_cache.c:8-43):
/// `dev` @0 (u64), `dev_offset` @8 (i64), `ino_offset` @16 (i64), `ino` @24
/// (u64), `block` @32 (u32), `flags_ptr` @36 (u32), `pages` @40 (u8),
/// `flags` @41 (u8), padding @42..56. The 64-bit Rust receiver reads the
/// same offsets.
///
/// Do NOT decode cache requests from `MessageM1` — same wire-format family
/// as 23-P0-1 / 22-P0-1 / 21-P1-1 / 19-P1-1 / 16-P0-1 (the old M1 decode
/// read `dev` from `m1p1` @16 and hardcoded `ino`/`ino_offset`/`block` to 0).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MessVmmcp {
    /// Device number. C: `mess_vmmcp.dev` (payload offset 0)
    pub dev: u64,
    /// Offset within the device. C: `mess_vmmcp.dev_offset` (payload offset 8)
    pub dev_offset: i64,
    /// Offset within the inode. C: `mess_vmmcp.ino_offset` (payload offset 16)
    pub ino_offset: i64,
    /// Inode number (`VMC_NO_INODE` = 0 for raw device blocks).
    /// C: `mess_vmmcp.ino` (payload offset 24)
    pub ino: u64,
    /// User-space block address (setcache only). C: `mess_vmmcp.block`
    /// (payload offset 32, 32-bit pointer on i386)
    pub block: u32,
    /// Flags pointer (unused by VM handlers). C: `mess_vmmcp.flags_ptr`
    /// (payload offset 36, 32-bit pointer on i386)
    pub flags_ptr: u32,
    /// Number of pages. C: `mess_vmmcp.pages` (payload offset 40)
    pub pages: u8,
    /// Flags (`VMSF_ONCE`). C: `mess_vmmcp.flags` (payload offset 41)
    pub flags: u8,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 14],
}

/// Cache-block map reply payload (VM → VFS) for `VM_MAPCACHEPAGE`.
///
/// C: `message.m_m2` — `mess_vmmcp_reply` (ipc.h:2396-2400):
/// ```c
/// typedef struct {
///     void *addr;          /* 32-bit on i386 */
///     u8_t flags;
///     uint8_t padding[51];
/// } mess_vmmcp_reply;
/// ```
/// Wire layout: `addr` @0 (u32), `flags` @4 (u8), padding @5..56.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessVmmcpReply {
    /// Mapped virtual address of the cache block in the caller's address
    /// space. C: `mess_vmmcp_reply.addr` (payload offset 0, 32-bit pointer)
    pub addr: u32,
    /// Reserved reply flags. C: `mess_vmmcp_reply.flags` (payload offset 4)
    pub flags: u8,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 51],
}

impl Default for MessVmmcpReply {
    fn default() -> Self {
        Self {
            addr: 0,
            flags: 0,
            _padding: [0; 51],
        }
    }
}

/// PM: procevent mask (subscriber → PM) — `mess_lsys_pm_proceventmask`.
///
/// C: `mess_lsys_pm_proceventmask` — ipc.h:1414-1420:
/// ```c
/// typedef struct {
///     unsigned int mask;   // event mask (PROC_EVENT_EXIT|SIGNAL)
///     uint8_t padding[52];
/// } mess_lsys_pm_proceventmask;
/// ```
///
/// Wire layout: `mask` @0 (u32), padding @4..56.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessLsysPmProceventmask {
    /// Event mask (bitwise OR of `PROC_EVENT_EXIT` / `PROC_EVENT_SIGNAL`).
    /// C: `unsigned int mask` (payload offset 0)
    pub mask: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

impl Default for MessLsysPmProceventmask {
    fn default() -> Self {
        Self {
            mask: 0,
            _padding: [0; 52],
        }
    }
}

/// PM: process event (PM → subscriber) — `mess_pm_lsys_proc_event`.
///
/// C: `mess_pm_lsys_proc_event` — ipc.h:1800-1813:
/// ```c
/// typedef struct {
///     endpoint_t endpt;      // target process endpoint
///     unsigned int event;    // PROC_EVENT_EXIT or SIGNAL
///     uint8_t padding[48];
/// } mess_pm_lsys_proc_event;
/// ```
///
/// Wire layout: `endpt` @0 (i32), `event` @4 (u32), padding @8..56.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessPmLsysProcEvent {
    /// Target process endpoint. C: `endpoint_t endpt` (payload offset 0)
    pub endpt: i32,
    /// Event type. C: `unsigned int event` (payload offset 4)
    pub event: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessPmLsysProcEvent {
    fn default() -> Self {
        Self {
            endpt: 0,
            event: 0,
            _padding: [0; 48],
        }
    }
}

/// PM: srv_fork params (RS → PM) — `mess_lsys_pm_srv_fork`.
///
/// C: `mess_lsys_pm_srv_fork` — ipc.h:1422-1428:
/// ```c
/// typedef struct {
///     uid_t uid;                 // real/eff/saved uid
///     gid_t gid;                 // real/eff/saved gid
///     uint8_t padding[48];
/// } mess_lsys_pm_srv_fork;
/// ```
///
/// Wire layout: `uid` @0 (u32), `gid` @4 (u32), padding @8..56.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessLsysPmSrvFork {
    /// User ID for new service. C: `uid_t uid` (payload offset 0)
    pub uid: u32,
    /// Group ID for new service. C: `gid_t gid` (payload offset 4)
    pub gid: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 48],
}

impl Default for MessLsysPmSrvFork {
    fn default() -> Self {
        Self {
            uid: 0,
            gid: 0,
            _padding: [0; 48],
        }
    }
}

/// PM: exit status (user → PM) — `mess_lc_pm_exit`.
///
/// C: `mess_lc_pm_exit` — ipc.h:445-451:
/// ```c
/// typedef struct {
///     int status;                // exit status
///     uint8_t padding[52];
/// } mess_lc_pm_exit;
/// ```
///
/// Wire layout: `status` @0 (i32), padding @4..56.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessLcPmExit {
    /// Exit status. C: `int status` (payload offset 0)
    pub status: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

impl Default for MessLcPmExit {
    fn default() -> Self {
        Self {
            status: 0,
            _padding: [0; 52],
        }
    }
}

/// PM: wait4 params (user → PM) — `mess_lc_pm_wait4`.
///
/// C: `mess_lc_pm_wait4` — ipc.h:460-470:
/// ```c
/// typedef struct {
///     pid_t pid;                 // wait target
///     int options;               // WNOHANG etc.
///     vir_bytes addr;            // rusage addr
///     uint8_t padding[40];
/// } mess_lc_pm_wait4;
/// ```
///
/// Wire layout: `pid` @0 (i32), `options` @4 (i32), `addr` @8 (u64), padding @16..56.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessLcPmWait4 {
    /// Wait target pid. C: `pid_t pid` (payload offset 0)
    pub pid: i32,
    /// Wait options (WNOHANG). C: `int options` (payload offset 4)
    pub options: i32,
    /// Rusage struct addr. C: `vir_bytes addr` (payload offset 8, u64 on 64-bit)
    pub addr: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 40],
}

impl Default for MessLcPmWait4 {
    fn default() -> Self {
        Self {
            pid: 0,
            options: 0,
            addr: 0,
            _padding: [0; 40],
        }
    }
}

/// PM: kill params (user → PM) — `mess_lc_pm_kill`.
///
/// C: `mess_lc_pm_kill` (pid, signo) — ipc.h:1880 (rs) overlay, used by `do_kill` via `m_lc_pm_sig` union overlay.
/// ```c
/// typedef struct { pid_t pid; int nr; uint8_t padding[48]; } mess_lc_pm_kill;
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessLcPmKill {
    /// Target pid. C: `pid_t pid` (payload offset 0)
    pub pid: i32,
    /// Signal number. C: `int nr` (payload offset 4)
    pub signo: i32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 48],
}

impl Default for MessLcPmKill {
    fn default() -> Self {
        Self {
            pid: 0,
            signo: 0,
            _padding: [0; 48],
        }
    }
}

/// PM: srv_kill params (RS → PM) — `mess_rs_pm_srv_kill`.
///
/// C: `mess_rs_pm_srv_kill` — ipc.h:1880-1885:
/// ```c
/// typedef struct { pid_t pid; int nr; uint8_t padding[48]; } mess_rs_pm_srv_kill;
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessRsPmSrvKill {
    /// Target pid. C: `pid_t pid` (payload offset 0)
    pub pid: i32,
    /// Signal number. C: `int nr` (payload offset 4)
    pub signo: i32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 48],
}

impl Default for MessRsPmSrvKill {
    fn default() -> Self {
        Self {
            pid: 0,
            signo: 0,
            _padding: [0; 48],
        }
    }
}

// ── MIB wire payloads (02-mib-message-contract.md) ──
//
// All six are wire-identical 32-bit layouts: every lane is 4 bytes, so
// each struct is exactly the 56-byte union payload. `vir_bytes`/`size_t`
// travel as `u32` — the senders (libc `__sysctl`, libsys `rmib.c`) are
// 32-bit C, and the 56-byte budget cannot hold 64-bit addresses next to
// `name[8]` anyway. A future 64-bit userland is an A-4 exchange-ABI
// decision, not a silent widening: widening here would desynchronise
// every sender at once.

/// MIB sysctl(2) request payload (user/libc → MIB).
///
/// C: `mess_lc_mib_sysctl` — ipc.h:424-433.
///
/// # Layout (matches C, all lanes 4 bytes)
/// ```text
/// | Field   | Type    | Offset |
/// |---------|---------|--------|
/// | oldp    | u32     | 0      |
/// | oldlen  | u32     | 4      |
/// | newp    | u32     | 8      |
/// | newlen  | u32     | 12     |
/// | namelen | u32     | 16     |
/// | namep   | u32     | 20     |
/// | name    | [i32;8] | 24     |
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MessLcMibSysctl {
    /// Old-data address (0 = no sink). C: `vir_bytes oldp`.
    pub oldp: u32,
    /// Old-data length. C: `size_t oldlen`.
    pub oldlen: u32,
    /// New-data address. C: `vir_bytes newp`.
    pub newp: u32,
    /// New-data length. C: `size_t newlen`.
    pub newlen: u32,
    /// Name length in components. C: `unsigned int namelen`.
    pub namelen: u32,
    /// Name address for long names. C: `vir_bytes namep`.
    pub namep: u32,
    /// Inline name (`namelen <= CTL_SHORTNAME`). C: `int name[CTL_SHORTNAME]`.
    pub name: [i32; 8],
}

/// MIB sysctl(2) reply payload (MIB → user/libc).
///
/// C: `mess_mib_lc_sysctl` — ipc.h:1548-1552. One lane: the full result
/// length (or the staged `call_reslen` on error) — see 01 §2.4.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessMibLcSysctl {
    /// Result length. C: `size_t oldlen` (payload offset 0).
    pub oldlen: u32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 52],
}

impl Default for MessMibLcSysctl {
    // Manual: `[u8; 52]` has no `Default` — same as the neighbouring
    // 48/52-byte payloads in this file.
    fn default() -> Self {
        Self {
            oldlen: 0,
            _padding: [0; 52],
        }
    }
}

/// MIB subtree mount/unmount payload (service → MIB, one-way).
///
/// C: `mess_lsys_mib_register` — ipc.h:1373-1382. Shared by `MIB_REGISTER`
/// and `MIB_DEREGISTER`: deregister reads only `root_id` (remote.c:303).
///
/// # Layout (matches C, all lanes 4 bytes)
/// ```text
/// | Field  | Type    | Offset |
/// |--------|---------|--------|
/// | root_id| u32     | 0      |
/// | flags  | u32     | 4      |
/// | csize  | u32     | 8      |
/// | clen   | u32     | 12     |
/// | miblen | u32     | 16     |
/// | mib    | [i32;8] | 20     |
/// | pad    | 4B      | 52     |
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MessLsysMibRegister {
    /// Remote root id. C: `uint32_t root_id`.
    pub root_id: u32,
    /// Mount flags. C: `uint32_t flags`.
    pub flags: u32,
    /// Remote root child slots. C: `unsigned int csize`.
    pub csize: u32,
    /// Remote root children. C: `unsigned int clen`.
    pub clen: u32,
    /// Mount-path length (≤ 8, else silently dropped). C: `unsigned int miblen`.
    pub miblen: u32,
    /// Mount path. C: `int mib[CTL_SHORTNAME]`.
    pub mib: [i32; 8],
    /// Padding to 56 bytes.
    pub _padding: [u8; 4],
}

/// MIB remote reply payload (service → MIB).
///
/// C: `mess_lsys_mib_reply` — ipc.h:1384-1389. Replies are keyed by
/// `req_id` (the call this answers); a zero `req_id` means "no reply
/// expected" (remote.c:361,364).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLsysMibReply {
    /// Request id being answered. C: `uint32_t req_id`.
    pub req_id: u32,
    /// Handler status (may be `ERESTART`). C: `ssize_t status`.
    pub status: i32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 48],
}

impl Default for MessLsysMibReply {
    // Manual: `[u8; 48]` has no `Default` — same as `MessDsReply`.
    fn default() -> Self {
        Self {
            req_id: 0,
            status: 0,
            _padding: [0; 48],
        }
    }
}

/// MIB relayed sysctl call (MIB → remote service).
///
/// C: `mess_mib_lsys_call` — ipc.h:1554-1569. MIB re-grants the user
/// regions to the service instead of copying: three grants, three
/// lengths, no data bytes on this message.
///
/// # Layout (matches C, all lanes 4 bytes)
/// ```text
/// | Field      | Type | Offset |
/// |------------|------|--------|
/// | req_id     | u32  | 0      |
/// | root_id    | u32  | 4      |
/// | name_grant | i32  | 8      |
/// | name_len   | u32  | 12     |
/// | oldp_grant | i32  | 16     |
/// | oldp_len   | u32  | 20     |
/// | newp_grant | i32  | 24     |
/// | newp_len   | u32  | 28     |
/// | user_endpt | i32  | 32     |
/// | flags      | u32  | 36     |
/// | root_ver   | u32  | 40     |
/// | tree_ver   | u32  | 44     |
/// | pad        | 8B   | 48     |
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MessMibLsysCall {
    /// Request id (echoed in the reply). C: `uint32_t req_id`.
    pub req_id: u32,
    /// Remote root id. C: `uint32_t root_id`.
    pub root_id: u32,
    /// Grant for the remaining name. C: `cp_grant_id_t name_grant`.
    pub name_grant: GrantId,
    /// Remaining name length. C: `unsigned int name_len`.
    pub name_len: u32,
    /// Grant for the old-data region (or invalid). C: `cp_grant_id_t oldp_grant`.
    pub oldp_grant: GrantId,
    /// Old-data length. C: `size_t oldp_len`.
    pub oldp_len: u32,
    /// Grant for the new-data region (or invalid). C: `cp_grant_id_t newp_grant`.
    pub newp_grant: GrantId,
    /// New-data length. C: `size_t newp_len`.
    pub newp_len: u32,
    /// Original user endpoint. C: `endpoint_t user_endpt`.
    pub user_endpt: i32,
    /// Call flags. C: `uint32_t flags`.
    pub flags: u32,
    /// Remote root version seen at mount. C: `uint32_t root_ver`.
    pub root_ver: u32,
    /// Tree version seen at mount. C: `uint32_t tree_ver`.
    pub tree_ver: u32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 8],
}

/// MIB subtree description fetch (MIB → remote service).
///
/// C: `mess_mib_lsys_info` — ipc.h:1571-1580. Asks the service for the
/// name and description of its subtree root (mount time).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MessMibLsysInfo {
    /// Request id. C: `uint32_t req_id`.
    pub req_id: u32,
    /// Remote root id. C: `uint32_t root_id`.
    pub root_id: u32,
    /// Grant for the name buffer. C: `cp_grant_id_t name_grant`.
    pub name_grant: GrantId,
    /// Name buffer size. C: `size_t name_size`.
    pub name_size: u32,
    /// Grant for the description buffer. C: `cp_grant_id_t desc_grant`.
    pub desc_grant: GrantId,
    /// Description buffer size. C: `size_t desc_size`.
    pub desc_size: u32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn test_sched_message_layouts() {
        // C: ipc.h `_ASSERT_MSG_SIZE` — every payload is 56 bytes.
        assert_eq!(size_of::<MessLsysSchedSchedulingStart>(), 56);
        assert_eq!(size_of::<MessLsysSchedSchedulingStop>(), 56);
        assert_eq!(size_of::<MessPmSchedSchedulingSetNice>(), 56);
        assert_eq!(size_of::<MessSchedLsysSchedulingStart>(), 56);
        // Field offsets pin the C layouts (ipc.h:1428-1444,1819-1827,1904-1912).
        let start = MessLsysSchedSchedulingStart {
            endpoint: 7,
            parent: 3,
            maxprio: 12,
            quantum: 200,
            _padding: [0u8; 40],
        };
        assert_eq!(
            (start.endpoint, start.parent, start.maxprio, start.quantum),
            (7, 3, 12, 200)
        );
        let stop = MessLsysSchedSchedulingStop {
            endpoint: 7,
            _padding: [0u8; 52],
        };
        assert_eq!(stop.endpoint, 7);
        let nice = MessPmSchedSchedulingSetNice {
            endpoint: 7,
            maxprio: 12,
            _padding: [0u8; 48],
        };
        assert_eq!((nice.endpoint, nice.maxprio), (7, 12));
        let reply = MessSchedLsysSchedulingStart {
            scheduler: 9,
            _padding: [0u8; 52],
        };
        assert_eq!(reply.scheduler, 9);
    }

    #[test]
    fn test_ds_val_shapes() {
        // C: ipc.h:94-99 — three arms share one 32-bit lane.
        assert_eq!(size_of::<DsVal>(), 4);
        assert_eq!(DsVal::grant(11).as_grant(), 11);
        assert_eq!(DsVal::number(0xDEAD_BEEF).as_number(), 0xDEAD_BEEF);
        assert_eq!(DsVal::endpoint(Endpoint(6)).as_endpoint(), Endpoint(6));
    }

    #[test]
    fn test_ds_req_layout() {
        // C: ipc.h:107-115 `_ASSERT_MSG_SIZE` — six lanes, 56 bytes.
        assert_eq!(size_of::<MessDsReq>(), 56);
        // Field order pins the C layout (key_grant/key_len/flags/val_in/
        // val_len/owner).
        let req = MessDsReq {
            key_grant: 11,
            key_len: 9,
            flags: 0x010,
            val_in: DsVal::number(42),
            val_len: 4,
            owner: 6,
            _padding: [0u8; 32],
        };
        assert_eq!(
            (req.key_grant, req.key_len, req.flags, req.owner),
            (11, 9, 0x010, 6)
        );
        assert_eq!(req.val_in.as_number(), 42);
        assert_eq!(req.val_len, 4);
    }

    #[test]
    fn test_ds_reply_layout() {
        // C: ipc.h:100-105 `_ASSERT_MSG_SIZE` — two lanes, 56 bytes.
        assert_eq!(size_of::<MessDsReply>(), 56);
        let reply = MessDsReply {
            val_out: DsVal::endpoint(Endpoint(6)),
            val_len: 4,
            _padding: [0u8; 48],
        };
        assert_eq!(reply.val_out.as_endpoint(), Endpoint(6));
        assert_eq!(reply.val_len, 4);
    }

    #[test]
    fn test_mib_wire_layouts() {
        // C: ipc.h `_ASSERT_MSG_SIZE` — every MIB payload is 56 bytes.
        assert_eq!(size_of::<MessLcMibSysctl>(), 56);
        assert_eq!(size_of::<MessMibLcSysctl>(), 56);
        assert_eq!(size_of::<MessLsysMibRegister>(), 56);
        assert_eq!(size_of::<MessLsysMibReply>(), 56);
        assert_eq!(size_of::<MessMibLsysCall>(), 56);
        assert_eq!(size_of::<MessMibLsysInfo>(), 56);
        // Field order pins the C layouts (ipc.h:424-433,1548-1580,1373-1389).
        let req = MessLcMibSysctl {
            oldp: 0x1000,
            oldlen: 64,
            newp: 0,
            newlen: 0,
            namelen: 3,
            namep: 0,
            name: [1, 7, 12, 0, 0, 0, 0, 0],
            ..Default::default()
        };
        assert_eq!((req.oldp, req.oldlen, req.namelen), (0x1000, 64, 3));
        assert_eq!(req.name[2], 12);
        let reg = MessLsysMibRegister {
            root_id: 9,
            flags: 0,
            csize: 4,
            clen: 2,
            miblen: 2,
            mib: [4, 20, 0, 0, 0, 0, 0, 0],
            ..Default::default()
        };
        assert_eq!((reg.root_id, reg.miblen), (9, 2));
        assert_eq!(reg.mib[1], 20);
        let call = MessMibLsysCall {
            req_id: 41,
            root_id: 9,
            user_endpt: 5,
            ..Default::default()
        };
        assert_eq!((call.req_id, call.root_id, call.user_endpt), (41, 9, 5));
    }
}
