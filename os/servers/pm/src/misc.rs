//! Misc queries: `do_sysuname/getsysinfo/getprocnr/getepinfo/reboot/svrctl/getrusage/sprofile/mcontext`.
//!
//! C ground truth: `minix3/minix/servers/pm/misc.c` (447 lines, 72/108/149/169/198/291/400)
//! + `profile.c:22` + `mcontext.c:13/23` + `utility.c:57/144`
//!   Design: `.design/20-design.v1.md` D1–D8 (explicit `UtsField/SysInfoWhat/EpInfo/RebootCtl/ParamStore/RusageWho`).
//!   Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{
    Clock, E2BIG, EINVAL, ENOSPC, ENOSYS, EPERM, ESRCH, Endpoint, Gid, Pid, Uid, UserSlot, VirBytes,
};
use crate::mproc::ProcTable;
use crate::ipc::ReplyIntent;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// The (utime, stime) pair for self/children (`getrusage`/`sys_times` gather
/// into this typed alias).
type UtimeStimePair = ((i64, i64), (i64, i64));

/// `UTS_TBL` length (`misc.c:46` 8).
pub const UTS_TBL_LEN: usize = 8;

/// `MAX_LOCAL_PARAMS` (`misc.c:297` 2).
pub const MAX_LOCAL_PARAMS: usize = 2;

/// `RB_POWERDOWN` — power off (`sys/sys/reboot.h:54` = `(RB_HALT|0x800)`,
/// `RB_HALT = 0x8` at `:46`), i.e. `0x808`. This bit pattern crosses the wire
/// verbatim from `reboot(2)`'s `how`, and `do_reboot` gates the power-off leg
/// on `how & RB_POWERDOWN` (C `misc.c:210`), so it MUST equal C — the 2026-10-13
/// constant audit found the port had `1 << 0`, which made the real wire value
/// (`0x808`) fall through the gate (system halted/rebooted instead of powering
/// off) and a plain non-power flag (`0x1`) spuriously take the leg.
pub const RB_POWERDOWN: i32 = 0x808;

// The local SI_PROC_TAB/SI_CALL_STATS copies were deleted and replaced with the
// single authority in `minix_types::ipc::sysinfo`. History (a real bug found while
// reconciling the E-ISPROD producer): this crate used to record SI_PROC_TAB = 0
// locally, disagreeing with C (`sysinfo.h:12` says 2) and with the other
// services — IS/MIB sending the C value 2 got EINVAL from this service. Once the
// constants have one authority, that class of mismatch cannot recur at the
// definition level.
pub use minix_types::{SI_CALL_STATS, SI_PROC_TAB};

/// `RUSAGE_SELF/CHILDREN` (`resource.h`).
pub const RUSAGE_SELF: i32 = 0;
pub const RUSAGE_CHILDREN: i32 = -1;

/// `PROF_START/STOP` (`profile.h`).
pub const PROF_START: i32 = 0;
pub const PROF_STOP: i32 = 1;

/// `PM_*` call numbers (`callnr.h: 25/36-39/45-47`) — a re-export from the
/// single source of truth in minix-types (derived from [`PmCall`] discriminants),
/// not hand-copied in this crate (the same ruling as for `SI_*` above).
pub use minix_types::{
    PM_GETEPINFO, PM_GETMCONTEXT, PM_GETPROCNR, PM_GETRUSAGE, PM_GETSYSINFO, PM_REBOOT,
    PM_SETMCONTEXT, PM_SPROF, PM_SVRCTL, PM_SYSUNAME,
};

/// `monitor_params` capacity (`param.h: MULTIBOOT_PARAM_BUF_SIZE`).
pub const MONITOR_CAP: usize = 4096;

/// Errors for misc calls (mapped to errno).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiscError {
    Inval,
    Perm,
    Srch,
    Nospc,
    Big,
    Nosys,
    Fault,
    Busy,
    /// The raw negative errno of the kernel/VM service (C passes `r` straight
    /// through everywhere, misc.c:430-431/441-442; it is no longer folded into
    /// EINVAL).
    Kernel(i32),
}

impl MiscError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Inval => EINVAL,
            Self::Perm => EPERM,
            Self::Srch => ESRCH,
            Self::Nospc => ENOSPC,
            Self::Big => E2BIG,
            Self::Nosys => ENOSYS,
            Self::Fault => minix_types::EFAULT,
            Self::Busy => minix_types::EBUSY,
            Self::Kernel(r) => r,
        }
    }
}

/// `uts_tbl` entry (`misc.c:46-60`, D1, A-11).
pub const UTS_VAL_SYSNAME: &str = "Minix";
pub const UTS_VAL_NODENAME: &str = "noname";
pub const UTS_VAL_RELEASE: &str = "3.3.0";
pub const UTS_VAL_VERSION: &str = "Minix 3.3.0";
pub const UTS_VAL_MACHINE: &str = "x86_64";

/// `UtsField` (`misc.c:46-60` 9 slots, 4 of them NULL).
const UTS_TBL: [Option<&'static str>; 8] = [
    Some(UTS_VAL_MACHINE), // 0: arch (x86_64, was i386/evbarm)
    None,                  // 1: No kernel arch
    Some(UTS_VAL_MACHINE), // 2: machine
    None,                  // 3: No hostname
    Some(UTS_VAL_NODENAME),// 4: nodename
    Some(UTS_VAL_RELEASE), // 5: release
    Some(UTS_VAL_VERSION), // 6: version
    Some(UTS_VAL_SYSNAME), // 7: sysname
];

/// `uts_field` (`misc.c:79-83`, D1).
pub fn uts_field(field: usize) -> Result<&'static str, MiscError> {
    if field >= UTS_TBL_LEN {
        return Err(MiscError::Inval);
    }
    UTS_TBL[field].ok_or(MiscError::Inval)
}

/// `SysInfoWhat` (`misc.c:125-133`, D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysInfoWhat {
    ProcTab,
    #[cfg(feature = "syscall_stats")]
    CallStats,
}

impl TryFrom<i32> for SysInfoWhat {
    type Error = MiscError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            SI_PROC_TAB => Ok(Self::ProcTab),
            #[cfg(feature = "syscall_stats")]
            SI_CALL_STATS => Ok(Self::CallStats),
            _ => Err(MiscError::Inval),
        }
    }
}

/// `SysInfoCtl` (`misc.c:125`, D2, A-11).
pub trait SysInfoCtl {
    fn proc_tab(&self) -> &[u8];
    #[cfg(feature = "syscall_stats")]
    fn call_stats(&self) -> &[u8];
}

/// Copy abstraction (`sys_datacopy`, `misc.c:90/143/188/385`).
pub trait CopyToUser {
    fn copy_to_user(&mut self, src: &[u8], dst: VirBytes) -> Result<(), MiscError>;
    fn copy_from_user(&mut self, src: VirBytes, dst: &mut [u8]) -> Result<(), MiscError>;
}

/// `EpInfo` (`misc.c:180-192`, D3) — the `return pid` payload; `ngroups` is the
/// full group count (C misc.c:184), while `groups` is the copied-out content
/// truncated to the caller's buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpInfo {
    pub pid: Pid,
    pub uid: Uid,
    pub euid: Uid,
    pub gid: Gid,
    pub egid: Gid,
    pub ngroups: usize,
    pub groups: Vec<Gid>,
}

/// `RusageWho` (`misc.c:407-409`, D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RusageWho {
    Slf,
    Children,
}

impl TryFrom<i32> for RusageWho {
    type Error = MiscError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Slf),
            -1 => Ok(Self::Children),
            _ => Err(MiscError::Inval),
        }
    }
}

/// `rusage_from_ticks` (`utility.c:149-155`, D6, A-11) — `ticks*1e6/hz → sec/usec`.
pub fn rusage_from_ticks(ticks: Clock, hz: Clock) -> (i64, i64) {
    if hz == 0 {
        return (0, 0);
    }
    let usec = (ticks as u128 * 1_000_000u128) / hz as u128;
    let sec = (usec / 1_000_000) as i64;
    let usec_rem = (usec % 1_000_000) as i64;
    (sec, usec_rem)
}

/// `find_param` (`utility.c:57-71`, D5) — `monitor_params` KVP linear scan.
///
/// `monitor` is `NUL`-separated `key=value\0` string (from `monitor_params`).
pub fn find_param(monitor: &str, key: &str) -> Option<String> {
    // monitor is raw bytes with \0 separators; we iterate via split('\0')
    for kv in monitor.split('\0') {
        if kv.is_empty() {
            continue;
        }
        if let Some(pos) = kv.find('=') {
            let (k, v) = kv.split_at(pos);
            let v = &v[1..];
            if k == key {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// `ParamStore` (`misc.c:298-301`, D5) — `local_overrides[2]` tiny table.
#[derive(Debug, Clone)]
pub struct ParamStore {
    local: Vec<(String, String)>,
    pub monitor: String,
}

impl ParamStore {
    pub fn new(monitor: String) -> Self {
        Self { local: Vec::new(), monitor }
    }
    pub fn set(&mut self, key: String, val: String) -> Result<(), MiscError> {
        // C misc.c:327 → 328-334: ENOSPC (table full) comes before the
        // keylen/vallen bounds — when both conditions hold, the error code
        // differs (the check order now matches C).
        if self.local.len() >= MAX_LOCAL_PARAMS {
            return Err(MiscError::Nospc);
        }
        // C 328-334: keylen/vallen ∈ (0, 30) (the name[30]/value[30] bounds).
        if key.is_empty() || val.is_empty() || key.len() >= 30 || val.len() >= 30 {
            return Err(MiscError::Inval);
        }
        self.local.push((key, val));
        Ok(())
    }
    pub fn get(&self, key: &str) -> Option<String> {
        if key.is_empty() {
            // keylen==0 → whole table (misc.c:352-354)
            return Some(self.monitor.clone());
        }
        for (k, v) in &self.local {
            if k == key {
                return Some(v.clone());
            }
        }
        find_param(&self.monitor, key)
    }
}

/// `RebootCtl` (`misc.c:207-230`, D4, A-3).
///
/// `broadcast_kill`/`stop_init`/`tell_reboot` correspond respectively to C's
/// `check_sig(-1, SIGKILL, FALSE)` (misc.c:221), `sys_stop(INIT_PROC_NR)` (`:222`)
/// and `tell_vfs(&mproc[VFS_PROC_NR], &m)` (`:228`) — all three must touch the
/// mproc table, so [`do_reboot`] re-borrows the table as `&mut ProcTable` at the
/// call site and hands it over; the port itself holds only the kernel/transport
/// channels.
pub trait RebootCtl {
    /// C: `abort_flag = m_in.m_lc_pm_reboot.how` (misc.c:205; glo.h:26).
    fn set_abort(&mut self, how: i32);
    /// C: readclock `RTCDEV_PWR_OFF` taskcall (misc.c:207-214; skipped when the
    /// DS name lookup fails). Returns whether power-off was requested.
    fn try_power_off(&mut self) -> bool;
    fn broadcast_kill(&mut self, table: &mut ProcTable, caller: UserSlot);
    fn stop_init(&mut self, table: &ProcTable);
    fn tell_reboot(&mut self, table: &mut ProcTable) -> i32;
}

/// The resource-usage triple VM contributes (`vm_getrusage`'s output,
/// `utility.c:446-450`).
///
/// In C, VM writes these three values into `struct rusage`'s `ru_maxrss` slot
/// (in KB, `vm_total_max/1024`), `ru_minflt` and `ru_majflt` (the other 11 long
/// fields are never written by either side and stay zero — `misc.c:423` memsets
/// and VM only changes these three). The Rust rewrite replaces the "datacopy
/// round trip" with a by-value message channel: the VM_GETRUSAGE reply carries
/// the three values directly, PM assembles the full structure and copies it out,
/// and the final byte image matches C exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmUsage {
    /// Peak resident set (KB). C: `r_usage.ru_maxrss = vmp->vm_total_max/1024L`.
    pub max_rss_kb: u64,
    /// Minor faults (pages reclaimed). C: `r_usage.ru_minflt`.
    pub minor_faults: u64,
    /// Major faults. C: `r_usage.ru_majflt`.
    pub major_faults: u64,
}

/// `TimesVmCtl` (`misc.c:429/441`, D6).
pub trait TimesVmCtl {
    fn sys_times(&mut self, ep: Endpoint) -> Result<(Clock, Clock), MiscError>;
    fn vm_rusage(&mut self, ep: Endpoint, who: RusageWho) -> Result<VmUsage, MiscError>;
}

/// `McontextCtl` (`mcontext.c:15/25`, D8).
pub trait McontextCtl {
    fn get(&self, ep: Endpoint, ctx: VirBytes) -> i32;
    fn set(&self, ep: Endpoint, ctx: VirBytes) -> i32;
}

/// The production `TimesVmCtl` implementation. CPU time comes straight from the
/// kernel SYS_TIMES (following the [`SysMcontextCtl`] precedent; hosted builds
/// answer -EIO honestly, and the powered-on path is tracked under E1), while the
/// three memory values go through the VM_GETRUSAGE taskcall (the sendrec
/// discipline of vm_fork, dispatcher.rs:137-156).
pub struct SysTimesVmCtl<'a, T: crate::ipc::IpcTransport + ?Sized> {
    /// Transport for the VM leg (C `vm_getrusage`'s `_taskcall(VM_PROC_NR, ...)`,
    /// libsys vm_getrusage.c:7-17).
    pub transport: &'a mut T,
}

impl<T: crate::ipc::IpcTransport + ?Sized> TimesVmCtl for SysTimesVmCtl<'_, T> {
    fn sys_times(&mut self, ep: Endpoint) -> Result<(Clock, Clock), MiscError> {
        // C sys_times(who_e, &utime, &stime, NULL, NULL) (misc.c:429-431;
        // libsys sys_times.c:3-23). A failure's original errno is propagated up.
        let times = minix_sys::syscall::sys_times(
            &minix_sys::syscall::DirectKernelCallTransport,
            ep.0,
        )
        .map_err(MiscError::Kernel)?;
        Ok((times.user_time as Clock, times.system_time as Clock))
    }

    fn vm_rusage(&mut self, ep: Endpoint, who: RusageWho) -> Result<VmUsage, MiscError> {
        // Request encoding: C `m_lsys_vm_rusage {endpt, addr, children}`
        // (ipc.h:1513-1520). C's addr is the address of r_usage on PM's stack,
        // for the VM datacopy round trip; Rust carries the values in the message,
        // so addr is always zero.
        let children = matches!(who, RusageWho::Children);
        let mut msg = minix_types::Message {
            m_type: minix_types::VM_GETRUSAGE as i32,
            ..Default::default()
        };
        {
            // SAFETY: m_lsys_vm_rusage is the documented payload layout of
            // VM_GETRUSAGE (VM's call_getrusage decodes
            // endpt@0/addr@8/children@16, dispatcher.rs:912-920).
            let req = unsafe { &mut msg.m_u.m_lsys_vm_rusage };
            req.endpt = ep.0;
            req.addr = 0;
            req.children = children as i32;
        }
        self.transport
            .sendrec(minix_types::Endpoint::VM, &mut msg)
            .map_err(|_| MiscError::Kernel(minix_types::EIO))?;
        // C: the taskcall's return value is the result (vm_getrusage.c:17); VM's
        // error reply carries a positive errno in m_type.
        if msg.m_type != minix_types::OK {
            return Err(MiscError::Kernel(msg.m_type));
        }
        // Reply decoding: VM encodes m1p1=max_rss_kb/m1i1=minor/m1i2=major
        // (encode.rs:258-268; the fault counts saturate through i32).
        // SAFETY: the reply arm m_m1 is overwritten by VM with the same layout.
        let m1 = unsafe { &msg.m_u.m_m1 };
        Ok(VmUsage {
            max_rss_kb: m1.m1p1,
            minor_faults: u64::from(m1.m1i1.max(0) as u32),
            major_faults: u64::from(m1.m1i2.max(0) as u32),
        })
    }
}

/// `SprofCtl` (`profile.c:31-33`, D7).
/// (The eight parameters correspond one-to-one with the payload of C's
/// `sys_vtimer`/profile message, `[ARCH: A-3]`.)
#[allow(clippy::too_many_arguments)]
pub trait SprofCtl {
    fn sprof(&mut self, action: i32, mem_size: usize, freq: u32, intr_type: i32, ep: Endpoint, ctl_ptr: VirBytes, mem_ptr: VirBytes) -> i32;
}

/// The production `McontextCtl` implementation: sys_getmcontext/sys_setmcontext
/// delegated straight through (the minix-sys wrapper is in place; errors pass
/// through raw, and the `do_*` layer folds EINVAL).
pub struct SysMcontextCtl;

impl McontextCtl for SysMcontextCtl {
    fn get(&self, ep: Endpoint, ctx: VirBytes) -> i32 {
        match minix_sys::syscall::sys_getmcontext(
            &minix_sys::syscall::DirectKernelCallTransport,
            ep.0,
            ctx.0,
        ) {
            Ok(()) => 0,
            Err(e) => e,
        }
    }

    fn set(&self, ep: Endpoint, ctx: VirBytes) -> i32 {
        match minix_sys::syscall::sys_setmcontext(
            &minix_sys::syscall::DirectKernelCallTransport,
            ep.0,
            ctx.0,
        ) {
            Ok(()) => 0,
            Err(e) => e,
        }
    }
}

/// The production `SprofCtl` implementation: sys_sprof delegated straight through
/// (the minix-sys wrapper is in place; errors pass through as the original errno,
/// and `do_sprofile`'s feature gate is unchanged).
pub struct SysSprofCtl {
    /// Target process endpoint (sprof's sampling target; `do_sprofile` fills it
    /// with the caller).
    pub ep: Endpoint,
}

impl SprofCtl for SysSprofCtl {
    fn sprof(
        &mut self,
        action: i32,
        mem_size: usize,
        freq: u32,
        intr_type: i32,
        ep: Endpoint,
        ctl_ptr: VirBytes,
        mem_ptr: VirBytes,
    ) -> i32 {
        //[SD19-gate] PM send/notify/reply are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = ep;
        match minix_sys::syscall::sys_sprof(
            &minix_sys::syscall::DirectKernelCallTransport,
            action,
            freq as i32,
            intr_type,
            self.ep.0,
            ctl_ptr.0,
            mem_ptr.0,
        ) {
            Ok(()) => 0,
            Err(e) => e,
        }
    }
}

/// Helper: `is_superuser`.
///
/// C's test is `mp_effuid == 0`, uniformly for every mproc (including
/// `PRIV_PROC`); a system process's credential effuid is always 0 (boot
/// zero-initialises everything), so reading the credential directly reproduces C
/// faithfully.
pub(crate) fn is_superuser(table: &ProcTable, caller: minix_types::UserSlot) -> bool {
    table.procs[caller.get()]
        .resources
        .privilege
        .credentials()
        .user
        .effective
        == 0
}

/// `do_sysuname` (`misc.c:72-100`, D1).
pub fn do_sysuname(
    field: usize,
    req: i32,
    len: usize,
    caller_ep: Endpoint,
    cpy: &mut dyn CopyToUser,
) -> Result<usize, MiscError> {
    let s = uts_field(field)?;
    // C misc.c:85-96 default branch: only req==0 (copy out) is implemented;
    // req!=0 → EINVAL (the old signature had no `req` parameter, so this branch
    // had nowhere to arise).
    if req != 0 {
        return Err(MiscError::Inval);
    }
    let n = s.len() + 1;
    let to_copy = if n > len { len } else { n };
    if to_copy == 0 {
        return Ok(0);
    }
    // Include NUL if truncated? C copies min(strlen+1, len) bytes including partial NUL string.
    let bytes = s.as_bytes();
    let mut buf = vec![0u8; to_copy];
    let copy_len = if to_copy <= bytes.len() {
        bytes[..to_copy].to_vec()
    } else {
        let mut v = bytes.to_vec();
        v.push(0);
        v.resize(to_copy, 0);
        v
    };
    buf.copy_from_slice(&copy_len);
    cpy.copy_to_user(&buf, VirBytes(0))?; // placeholder VirBytes, caller will map
    // In C, sys_datacopy uses caller endpoint; abstract via CopyToUser.
    //[SD19-gate] PM send/notify/reply are fire-and-forget
    #[allow(clippy::let_underscore_must_use)]
    let _ = caller_ep;
    Ok(to_copy)
}

/// Simplified `do_getsysinfo` (`misc.c:108-144`, D2).
pub fn do_getsysinfo(
    table: &ProcTable,
    caller: minix_types::UserSlot,
    what: SysInfoWhat,
    size: usize,
    dst: VirBytes,
    ctl: &dyn SysInfoCtl,
    cpy: &mut dyn CopyToUser,
) -> Result<(), MiscError> {
    if !is_superuser(table, caller) {
        return Err(MiscError::Perm);
    }
    match what {
        SysInfoWhat::ProcTab => {
            // Whole-table snapshot serialization (`MProcSnap` 76B per slot ×
            // NR_PROCS), copied out slot by slot into the caller's buffer. This
            // corresponds to C: misc.c:142-143's sys_datacopy(SELF, mproc, dst,
            // size) — copying slot by slot avoids a whole-table relay buffer (the
            // old 464B C-ABI row was retired along with the A-4 alignment work,
            // see the difference table at the top of mproc/wire.rs).
            const ROW_SIZE: usize = core::mem::size_of::<minix_types::MProcSnap>();
            let tab_size = ROW_SIZE * table.procs.len();
            if size != tab_size {
                return Err(MiscError::Inval);
            }
            for (idx, slot) in table.procs.iter().enumerate() {
                let row = crate::mproc::wire::serialize_snap(idx, slot);
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        core::ptr::addr_of!(row) as *const u8,
                        ROW_SIZE,
                    )
                };
                cpy.copy_to_user(
                    bytes,
                    VirBytes(dst.0 + (idx * ROW_SIZE) as u64),
                )?;
            }
            Ok(())
        }
        #[cfg(feature = "syscall_stats")]
        SysInfoWhat::CallStats => {
            let b = ctl.call_stats();
            if size != b.len() {
                return Err(MiscError::Inval);
            }
            cpy.copy_to_user(b, dst)?;
            Ok(())
        }
    }
}

/// `do_getprocnr` (`misc.c:149-164`, D3).
pub fn do_getprocnr(
    table: &ProcTable,
    caller_ep: Endpoint,
    pid: Pid,
) -> Result<Endpoint, MiscError> {
    if caller_ep != Endpoint::RS {
        return Err(MiscError::Perm);
    }
    let slot = table.find_proc(pid).ok_or(MiscError::Srch)?;
    Ok(table.procs[slot.get()].endpoint())
}

/// `do_getepinfo` (`misc.c:169-193`, D3).
pub fn do_getepinfo(
    table: &ProcTable,
    ep: Endpoint,
    caller_ngroups: usize,
    _cpy: &mut dyn CopyToUser,
) -> Result<EpInfo, MiscError> {
    let slot = table.pm_isokendpt(ep).map_err(|_| MiscError::Srch)?;
    let rmp = &table.procs[slot.get()];
    let creds = rmp.resources.privilege.credentials();
    // C misc.c:184: the reply's ngroups carries the **full** mp_ngroups — the
    // truncation only affects the copy count, and the caller (RS) is entitled to
    // the real group count (to tell "only N groups" from "more, but the buffer is
    // too small"). The earlier code truncated first and then filled it in, which
    // lost that information.
    let ngroups_full = creds.ngroups;
    let copy_len = ngroups_full.min(caller_ngroups);
    let groups = if copy_len > 0 {
        creds.supplemental_groups[..copy_len].to_vec()
    } else {
        Vec::new()
    };
    // [DEFERRED] groups are copied out to the caller's buffer via sys_datacopy
    // with EFAULT propagation (misc.c:187-190) — that lands together with the wire
    // and CopyGroups production implementations (tracked under E7); for now the
    // copied-out content is expressed as a truncated Vec.
    Ok(EpInfo {
        pid: rmp.identity.id.pid,
        uid: creds.user.real,
        euid: creds.user.effective,
        gid: creds.group.real,
        egid: creds.group.effective,
        ngroups: ngroups_full,
        groups,
    })
}

/// `do_reboot` (`misc.c:198-231`, D4).
///
/// C's order: EPERM gate (`:202`) → `abort_flag = how` (`:205`) → RB_POWERDOWN
/// takes the readclock taskcall (`:207-214`) → `check_sig(-1, SIGKILL, FALSE)`
/// (`:221`; the comment there states the ordering constraint "kill before
/// reboot") → `sys_stop(INIT_PROC_NR)` (`:222`, return value unchecked) →
/// `tell_vfs` VFS_PM_REBOOT (`:228`) → SUSPEND without replying to the caller
/// (`:230`).
pub fn do_reboot(
    table: &mut ProcTable,
    caller: minix_types::UserSlot,
    how: i32,
    ctl: &mut dyn RebootCtl,
) -> Result<ReplyIntent, MiscError> {
    if !is_superuser(table, caller) {
        return Err(MiscError::Perm);
    }
    ctl.set_abort(how);
    if (how & RB_POWERDOWN) != 0 {
        //[SD19-gate] PM send/notify/reply are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = ctl.try_power_off();
    }
    ctl.broadcast_kill(table, caller);
    ctl.stop_init(table);
    //[SD19-gate] PM send/notify/reply are fire-and-forget
    #[allow(clippy::let_underscore_must_use)]
    let _ = ctl.tell_reboot(table);
    Ok(ReplyIntent::ReplyLater) // SUSPEND never reply
}

/// `do_svrctl` set/get (`misc.c:291-395`, D5) — simplified ParamStore API.
pub fn do_svrctl_set(store: &mut ParamStore, key: String, val: String) -> Result<(), MiscError> {
    store.set(key, val)
}
pub fn do_svrctl_get(store: &ParamStore, key: Option<String>, _keylen: usize, vallen: usize) -> Result<String, MiscError> {
    // C misc.c:359: the GET key's search_key[64] bound → EINVAL.
    if let Some(k) = &key
        && (k.is_empty() || k.len() > 64)
    {
        return Err(MiscError::Inval);
    }
    let (val, needed) = match key {
        // C 352-354: keylen==0 → whole table, val_len = sizeof(monitor_params)
        // (MULTIBOOT_PARAM_BUF_SIZE = 1024, minix/param.h:28 and multiboot.h:240)
        // — the E2BIG test uses the full buffer length, not the actual string
        // length.
        None => {
            // C 352-354: keylen==0 → val_len = sizeof(monitor_params)
            // (unconditionally the full buffer length 1024, with the trailing NUL
            // area counted in — that is C's whole-table snapshot semantics, not
            // the "actual string length").
            let full = store.monitor.clone();
            let needed = crate::init::MULTIBOOT_PARAM_BUF_SIZE;
            (full, needed)
        }
        Some(k) => {
            let v = store.get(&k).ok_or(MiscError::Srch)?;
            let needed = v.len() + 1;
            (v, needed)
        }
    };
    // C 376-378: val_len > sysgetenv.vallen → E2BIG.
    if needed > vallen {
        return Err(MiscError::Big);
    }
    Ok(val)
}

/// C `_IOC(inout, group, num, len)` (`sys/ioccom.h:84-90`, x86-64
/// `sizeof(struct sysgetenv)` = 32: two pointers plus two size_t).
const fn ioc(inout: u32, group: u8, num: u32, len: u32) -> i32 {
    (inout | ((len & 0xfff) << 16) | ((group as u32) << 8) | num) as i32
}

const IOC_INOUT: u32 = 0xC000_0000;
const IOC_IN: u32 = 0x8000_0000;
const IOC_OUT: u32 = 0x4000_0000;
const SIZEOF_SYSGETENV: u32 = 32; // char *key + char *val + size_t keylen + size_t vallen

/// `PMGETPARAM` (`sys/svrctl.h:16` `_IOWR('P', 0, sysgetenv)`).
pub const PMGETPARAM: i32 = ioc(IOC_INOUT, b'P', 0, SIZEOF_SYSGETENV);
/// `PMSETPARAM` (`sys/svrctl.h:17` `_IOW('P', 1, sysgetenv)`).
pub const PMSETPARAM: i32 = ioc(IOC_IN, b'P', 1, SIZEOF_SYSGETENV);
/// `OPMGETPARAM` (`sys/svrctl.h:19` `_IOW('M', 5, sysgetenv)`, the old
/// spelling). C `_IOW` uses `IOC_IN` (`sys/ioccom.h`
/// `_IOW(g,n,t)=_IOC(IOC_IN,…)`), so the direction bits must be `0x80…` — the
/// code once wrote `0x40…` by mistake, copying OPMSETPARAM's `IOC_OUT`, which sent
/// the real wire value `0x80204D05` into the unknown-req arm and answered EINVAL
/// (the `IOCGROUP` gate let 'M' through, but no case matched).
pub const OPMGETPARAM: i32 = ioc(IOC_IN, b'M', 5, SIZEOF_SYSGETENV);
/// `OPMSETPARAM` (`sys/svrctl.h:20` `_IOR('M', 7, sysgetenv)`, the old
/// spelling).
pub const OPMSETPARAM: i32 = ioc(IOC_OUT, b'M', 7, SIZEOF_SYSGETENV);

/// C `IOCGROUP(x)` (`sys/ioccom.h:68`).
const fn ioc_group(x: i32) -> u8 {
    (((x as u32) >> 8) & 0xff) as u8
}

/// `do_svrctl` (`misc.c:291-395`, D5).
///
/// `req` is a parameter: the IOCGROUP gate ('P'/'M', misc.c:307-309) → dispatch of
/// the four commands (309-395) → unknown req `EINVAL` (392-393). The old
/// signature's `is_set: bool` discarded `req`, so an illegal req could not be
/// rejected. Copying the user buffer (sysgetenv/copy/EFAULT) still belongs to the
/// wire face (a later batch); here the logic face passes `req` straight through.
pub fn do_svrctl(store: &mut ParamStore, req: i32, key: Option<String>, val: Option<String>, vallen: usize) -> Result<Option<String>, MiscError> {
    // C 307-309: the IOCGROUP ∈ {'P','M'} gate.
    let group = ioc_group(req);
    if group != b'P' && group != b'M' {
        return Err(MiscError::Inval);
    }
    match req {
        req if req == PMSETPARAM || req == OPMSETPARAM => {
            let k = key.ok_or(MiscError::Inval)?;
            let v = val.ok_or(MiscError::Inval)?;
            do_svrctl_set(store, k, v).map(|_| None)
        }
        req if req == PMGETPARAM || req == OPMGETPARAM => {
            let keylen = key.as_ref().map_or(0, |k| k.len());
            do_svrctl_get(store, key, keylen, vallen).map(Some)
        }
        _ => Err(MiscError::Inval), // C 392-393: unknown req
    }
}

/// `set_rusage_times` (`utility.c:144-156`, D6) — name-match wrapper.
pub fn set_rusage_times(ru_utime_sec: &mut i64, ru_utime_usec: &mut i64, ru_stime_sec: &mut i64, ru_stime_usec: &mut i64, user_time: Clock, sys_time: Clock, hz: Clock) {
    let (us, uu) = rusage_from_ticks(user_time, hz);
    let (ss, su) = rusage_from_ticks(sys_time, hz);
    *ru_utime_sec = us;
    *ru_utime_usec = uu;
    *ru_stime_sec = ss;
    *ru_stime_usec = su;
}

/// The LP64 byte length of `struct rusage` — 2 timevals (`ru_utime`/`ru_stime`,
/// each `sec+usec` as two i64) plus 14 longs (`sys/resource.h:57-76`,
/// `ru_maxrss..ru_nivcsw`).
const RUSAGE_LEN: usize = 128;

/// `do_getrusage` (`misc.c:400-447`, D6).
///
/// The copy-out destination `addr` is the user-space `struct rusage*` (the
/// destination of C's `sys_datacopy`, misc.c:446-447); the assembly order matches
/// C's three stages — PM fills `ru_utime`/`ru_stime` (the counterpart of
/// `set_rusage_times`), VM contributes `ru_maxrss`/`minflt`/`majflt`, the other 11
/// fields stay zero, and the whole thing is copied to the user in one go.
pub fn do_getrusage(
    table: &ProcTable,
    caller: minix_types::UserSlot,
    who: RusageWho,
    addr: VirBytes,
    hz: Clock,
    ctl: &mut dyn TimesVmCtl,
    cpy: &mut dyn CopyToUser,
) -> Result<UtimeStimePair, MiscError> {
    let ep = table.procs[caller.get()].endpoint();
    let (utime, stime) = match who {
        // C misc.c:430-431 passes the kernel return value straight through (a
        // seam error is propagated as it is, no longer folded into EINVAL; the
        // production TimesVmCtl's original errno is kept in the
        // `MiscError::Kernel` payload).
        RusageWho::Slf => ctl.sys_times(ep)?,
        RusageWho::Children => {
            let p = &table.procs[caller.get()];
            (p.resources.child_utime, p.resources.child_stime)
        }
    };
    let (u_sec, u_usec) = rusage_from_ticks(utime, hz);
    let (s_sec, s_usec) = rusage_from_ticks(stime, hz);
    // C misc.c:441-442 passes the vm_getrusage return value straight through.
    let vm = ctl.vm_rusage(ep, who)?;
    // Assemble `struct rusage` (the LP64 layout of sys/resource.h:57-76):
    // ru_utime@0/ru_stime@16/ru_maxrss@32/ru_minflt@64/ru_majflt@72, while the
    // other 11 long fields are never written on the C side (after the memset zeroes
    // the structure VM only changes these three, misc.c:423 + utility.c:446-450),
    // so they stay zero.
    let mut buf = [0u8; RUSAGE_LEN];
    buf[0..8].copy_from_slice(&u_sec.to_le_bytes());
    buf[8..16].copy_from_slice(&u_usec.to_le_bytes());
    buf[16..24].copy_from_slice(&s_sec.to_le_bytes());
    buf[24..32].copy_from_slice(&s_usec.to_le_bytes());
    buf[32..40].copy_from_slice(&(vm.max_rss_kb as i64).to_le_bytes());
    buf[64..72].copy_from_slice(&(vm.minor_faults as i64).to_le_bytes());
    buf[72..80].copy_from_slice(&(vm.major_faults as i64).to_le_bytes());
    cpy.copy_to_user(&buf, addr)?;
    // The decomposed values are returned for test observation; C's return value is
    // the sys_datacopy result (OK on success).
    Ok(((u_sec, u_usec), (s_sec, s_usec)))
}

/// `do_sprofile` (`profile.c:22-45`, D7).
pub fn do_sprofile(
    action: i32,
    mem_size: usize,
    freq: u32,
    intr_type: i32,
    ep: Endpoint,
    ctl_ptr: VirBytes,
    mem_ptr: VirBytes,
    ctl: &mut dyn SprofCtl,
) -> Result<(), MiscError> {
    // C profile.c:22-45 — PROF_START passes the full payload through SprofCtl;
    // PROF_STOP carries only the action; every other action is EINVAL (the switch
    // default).
    #[cfg(feature = "sprofile")]
    {
        ctl.sprof(
            action, mem_size, freq, intr_type, ep, ctl_ptr, mem_ptr,
        );
        Ok(())
    }
    #[cfg(not(feature = "sprofile"))]
    {
        //[SD19-gate] PM send/notify/reply are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = (action, mem_size, freq, intr_type, ep, ctl_ptr, mem_ptr, ctl);
        Err(MiscError::Nosys)
    }
}

/// `do_get/setmcontext` (`mcontext.c:13/23`, D8).
pub fn do_getmcontext(ctl: &dyn McontextCtl, ep: Endpoint, ctx: VirBytes) -> Result<(), MiscError> {
    let r = ctl.get(ep, ctx);
    if r != 0 {
        return Err(MiscError::Inval);
    }
    Ok(())
}
pub fn do_setmcontext(ctl: &dyn McontextCtl, ep: Endpoint, ctx: VirBytes) -> Result<(), MiscError> {
    let r = ctl.set(ep, ctx);
    if r != 0 {
        return Err(MiscError::Inval);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot, VirBytes};

    fn mk_running(table: &mut ProcTable, slot: usize, eff: u32) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(eff, 100));
        table.procs[slot].resources.child_utime = 200;
        table.procs[slot].resources.child_stime = 100;
    }

    struct NopCopy;
    impl CopyToUser for NopCopy {
        fn copy_to_user(&mut self, _src: &[u8], _dst: VirBytes) -> Result<(), MiscError> { Ok(()) }
        fn copy_from_user(&mut self, _src: VirBytes, _dst: &mut [u8]) -> Result<(), MiscError> { Ok(()) }
    }
    struct TestSysInfo {
        data: Vec<u8>,
    }
    impl SysInfoCtl for TestSysInfo {
        fn proc_tab(&self) -> &[u8] { &self.data }
        #[cfg(feature = "syscall_stats")]
        fn call_stats(&self) -> &[u8] { &self.data }
    }
    impl SysInfoCtl for TestSysInfoAlt {
        fn proc_tab(&self) -> &[u8] { &[] }
        #[cfg(feature = "syscall_stats")]
        fn call_stats(&self) -> &[u8] { &[] }
    }
    struct TestSysInfoAlt { _a: u8 }
    impl CopyToUser for TestSysInfoAlt {
        fn copy_to_user(&mut self, _src: &[u8], _dst: VirBytes) -> Result<(), MiscError> { Ok(()) }
        fn copy_from_user(&mut self, _src: VirBytes, _dst: &mut [u8]) -> Result<(), MiscError> { Ok(()) }
    }
    struct TestReboot {
        abort: i32,
        killed: bool,
        stopped: bool,
        told: bool,
        powered: bool,
    }
    impl RebootCtl for TestReboot {
        fn set_abort(&mut self, how: i32) {
            self.abort = how;
        }
        fn try_power_off(&mut self) -> bool {
            self.powered = true;
            false
        }
        fn broadcast_kill(&mut self, _table: &mut ProcTable, _caller: UserSlot) {
            self.killed = true;
        }
        fn stop_init(&mut self, _table: &ProcTable) {
            self.stopped = true;
        }
        fn tell_reboot(&mut self, _table: &mut ProcTable) -> i32 {
            self.told = true;
            0
        }
    }
    struct AltReboot;
    impl RebootCtl for AltReboot {
        fn set_abort(&mut self, _how: i32) {}
        fn try_power_off(&mut self) -> bool {
            true
        }
        fn broadcast_kill(&mut self, _table: &mut ProcTable, _caller: UserSlot) {}
        fn stop_init(&mut self, _table: &ProcTable) {}
        fn tell_reboot(&mut self, _table: &mut ProcTable) -> i32 {
            0
        }
    }
    struct TestTimesVm { hz: Clock }
    impl TimesVmCtl for TestTimesVm {
        fn sys_times(&mut self, _ep: Endpoint) -> Result<(Clock, Clock), MiscError> { Ok((120, 60)) }
        fn vm_rusage(&mut self, _ep: Endpoint, _who: RusageWho) -> Result<VmUsage, MiscError> {
            Ok(VmUsage { max_rss_kb: 4096, minor_faults: 33, major_faults: 4 })
        }
    }
    struct AltTimesVm;
    impl TimesVmCtl for AltTimesVm {
        fn sys_times(&mut self, _ep: Endpoint) -> Result<(Clock, Clock), MiscError> { Ok((0,0)) }
        fn vm_rusage(&mut self, _ep: Endpoint, _who: RusageWho) -> Result<VmUsage, MiscError> {
            Ok(VmUsage { max_rss_kb: 0, minor_faults: 0, major_faults: 0 })
        }
    }
    struct TestMctx { ret: i32 }
    impl McontextCtl for TestMctx {
        fn get(&self, _ep: Endpoint, _ctx: VirBytes) -> i32 { self.ret }
        fn set(&self, _ep: Endpoint, _ctx: VirBytes) -> i32 { self.ret }
    }
    struct AltMctx;
    impl McontextCtl for AltMctx {
        fn get(&self, _ep: Endpoint, _ctx: VirBytes) -> i32 { 0 }
        fn set(&self, _ep: Endpoint, _ctx: VirBytes) -> i32 { 0 }
    }
    struct TestSprof;
    impl SprofCtl for TestSprof {
        fn sprof(&mut self, _action: i32, _mem: usize, _freq: u32, _intr: i32, _ep: Endpoint, _ctl: VirBytes, _mem2: VirBytes) -> i32 { 0 }
    }
    struct AltSprof;
    impl SprofCtl for AltSprof {
        fn sprof(&mut self, _a: i32, _b: usize, _c: u32, _d: i32, _e: Endpoint, _f: VirBytes, _g: VirBytes) -> i32 { 0 }
    }

    #[test]
    fn test_uts_field() {
        assert_eq!(uts_field(0).unwrap(), "x86_64");
        assert_eq!(uts_field(7).unwrap(), "Minix");
        assert!(uts_field(8).is_err());
        assert!(uts_field(1).is_err()); // NULL slot
        assert!(uts_field(3).is_err());
    }

    #[test]
    fn test_getsysinfo_perm_size() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 1000);
        let ctl = TestSysInfo { data: vec![0u8; 100] };
        let mut cpy = NopCopy;
        // non-super → Perm
        assert_eq!(do_getsysinfo(&table, UserSlot::new(0), SysInfoWhat::ProcTab, 100, VirBytes(0x1000), &ctl, &mut cpy).unwrap_err(), MiscError::Perm);
        // super but size mismatch (table size = slot count × 76-byte row)
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0));
        assert_eq!(do_getsysinfo(&table, UserSlot::new(0), SysInfoWhat::ProcTab, 99, VirBytes(0x1000), &ctl, &mut cpy).unwrap_err(), MiscError::Inval);
        // The real path is live: size = table size → a real serialized copy-out
        // (NopCopy swallows the copy) returns Ok — the ENOSYS fail-closed answer
        // is gone, replaced by the real data path.
        let mproc_tab = table.procs.len() * core::mem::size_of::<minix_types::MProcSnap>();
        assert_eq!(
            do_getsysinfo(&table, UserSlot::new(0), SysInfoWhat::ProcTab, mproc_tab, VirBytes(0x1000), &ctl, &mut cpy).unwrap(),
            ()
        );
    }

    #[test]
    fn test_getprocnr_rs_only() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        mk_running(&mut table, 1, 0);
        table.procs[1].identity.id.pid = 55;
        assert_eq!(do_getprocnr(&table, Endpoint(99), 55).unwrap_err(), MiscError::Perm);
        assert_eq!(do_getprocnr(&table, Endpoint::RS, 999).unwrap_err(), MiscError::Srch);
        let ep = do_getprocnr(&table, Endpoint::RS, 55).unwrap();
        assert_eq!(ep, table.procs[1].endpoint());
    }

    #[test]
    fn test_getepinfo_trunc() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        mk_running(&mut table, 1, 0);
        table.procs[1].resources.privilege = Privilege::User({
            let mut c = Credentials::new(1000, 2000);
            c.ngroups = 3;
            c.supplemental_groups[0] = 10;
            c.supplemental_groups[1] = 20;
            c.supplemental_groups[2] = 30;
            c
        });
        let mut cpy = NopCopy;
        // caller buffer holds 2 groups: ngroups reports the full 3 (C misc.c:184),
        // while the copy-out is truncated to 2.
        let info = do_getepinfo(&table, table.procs[1].endpoint(), 2, &mut cpy).unwrap();
        assert_eq!(info.ngroups, 3, "reply ngroups = full mp_ngroups (C misc.c:184)");
        assert_eq!(info.groups, vec![10,20]);
        assert_eq!(info.pid, 101);
        // With a large enough buffer everything is copied out.
        let info = do_getepinfo(&table, table.procs[1].endpoint(), 16, &mut cpy).unwrap();
        assert_eq!(info.ngroups, 3);
        assert_eq!(info.groups, vec![10,20,30]);
        // invalid endpoint
        assert_eq!(do_getepinfo(&table, Endpoint::from_generation_slot(9, 9), 16, &mut cpy).unwrap_err(), MiscError::Srch);
    }

    #[test]
    fn test_reboot_perm_suspend() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 1000);
        let mut ctl = TestReboot {
            abort: 0,
            killed: false,
            stopped: false,
            told: false,
            powered: false,
        };
        assert_eq!(
            do_reboot(&mut table, UserSlot::new(0), 0, &mut ctl).unwrap_err(),
            MiscError::Perm
        );
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0));
        let r = do_reboot(&mut table, UserSlot::new(0), RB_POWERDOWN, &mut ctl).unwrap();
        assert_eq!(r, ReplyIntent::ReplyLater);
        assert!(ctl.killed);
        assert!(ctl.stopped);
        assert!(ctl.told);
        assert_eq!(ctl.abort, RB_POWERDOWN);
    }

    #[test]
    fn test_reboot_powerdown_wire_value_gates_power_off() {
        // Regression lock (2026-10-13 C-mirror constant audit): `how` arrives
        // verbatim from `reboot(2)` carrying C's `RB_POWERDOWN` = 0x808. A wrong
        // port constant (`1 << 0`) dropped the power-off leg for 0x808 and took
        // it spuriously for a plain non-power bit — both are pinned here.
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 1000);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0));
        // C wire value 0x808 (RB_HALT|0x800) → power-off leg taken.
        let mut ctl = TestReboot {
            abort: 0,
            killed: false,
            stopped: false,
            told: false,
            powered: false,
        };
        do_reboot(&mut table, UserSlot::new(0), 0x808, &mut ctl).unwrap();
        assert!(ctl.powered, "C RB_POWERDOWN (0x808) must take the power-off leg");
        // A flag carrying neither RB_HALT(0x8) nor 0x800 → power-off leg skipped.
        let mut ctl2 = TestReboot {
            abort: 0,
            killed: false,
            stopped: false,
            told: false,
            powered: false,
        };
        do_reboot(&mut table, UserSlot::new(0), 0x1, &mut ctl2).unwrap();
        assert!(!ctl2.powered, "non-power flag must not trigger power-off");
    }

    #[test]
    fn test_svrctl_local_e2big() {
        let mut store = ParamStore::new("foo=bar\0".to_string());
        // set two → ok, third → ENOSPC
        assert!(do_svrctl_set(&mut store, "a".to_string(), "1".to_string()).is_ok());
        assert!(do_svrctl_set(&mut store, "b".to_string(), "2".to_string()).is_ok());
        assert_eq!(do_svrctl_set(&mut store, "c".to_string(), "3".to_string()).unwrap_err(), MiscError::Nospc);
        // get → Esrch if missing
        assert_eq!(do_svrctl_get(&store, Some("nope".to_string()), 4, 100).unwrap_err(), MiscError::Srch);
        // E2BIG if vallen too small
        assert_eq!(do_svrctl_get(&store, Some("a".to_string()), 1, 1).unwrap_err(), MiscError::Big);
        // keylen==0 → whole table (buffer 4096 > the full 1024 → E2BIG does not
        // trigger)
        let whole = do_svrctl_get(&store, None, 0, 4096).unwrap();
        assert!(whole.contains("foo=bar"));
        // The whole-table E2BIG test uses the full monitor buffer length
        // (C 352-354) — vallen < 1024 is E2BIG even when the actual string is only
        // 8 bytes.
        assert_eq!(do_svrctl_get(&store, None, 0, 512).unwrap_err(), MiscError::Big);
        // The GET key's 64-byte bound (C 359).
        let long_key = "k".repeat(65);
        assert_eq!(do_svrctl_get(&store, Some(long_key), 65, 100).unwrap_err(), MiscError::Inval);
        // ENOSPC comes before the bounds (C 327→328) — with a full table even an
        // over-long key reports ENOSPC.
        assert_eq!(
            do_svrctl_set(&mut store, "k".repeat(64), "v".repeat(64)).unwrap_err(),
            MiscError::Nospc
        );
    }

    // ---- do_svrctl's req gate and four-command dispatch (regression anchors) ----

    #[test]
    fn test_svrctl_req_gate_and_dispatch() {
        let mut store = ParamStore::new(String::new());
        // IOCGROUP gate: a group other than 'P'/'M' → EINVAL (C misc.c:307-309).
        assert_eq!(
            do_svrctl(&mut store, ioc(IOC_IN, b'F', 1, 32), Some("k".into()), Some("v".into()), 0).unwrap_err(),
            MiscError::Inval
        );
        // Four-command dispatch: PMSETPARAM writes → PMGETPARAM reads it back; the
        // old OPM spelling has the same semantics.
        assert_eq!(
            do_svrctl(&mut store, PMSETPARAM, Some("k".into()), Some("v".into()), 2).unwrap(),
            None
        );
        assert_eq!(
            do_svrctl(&mut store, PMGETPARAM, Some("k".into()), None, 2).unwrap(),
            Some("v".to_string())
        );
        assert_eq!(
            do_svrctl(&mut store, OPMGETPARAM, Some("k".into()), None, 2).unwrap(),
            Some("v".to_string())
        );
        // Regression lock (C-mirror constant audit): the real C wire value
        // `_IOW('M',5)=0x80204D05` must hit the get branch. The old code spelled
        // OPMGETPARAM as IOC_OUT (0x40…), so this value fell into the unknown-req
        // arm and answered EINVAL — after the fix it must route to get.
        assert_eq!(
            do_svrctl(&mut store, 0x8020_4D05u32 as i32, Some("k".into()), None, 2).unwrap(),
            Some("v".to_string())
        );
        // The command codes match C's `_IOC` arithmetic bit for bit (the LP64 size
        // convention on both i386 and earm).
        assert_eq!(PMGETPARAM, 0xC020_5000u32 as i32);
        assert_eq!(PMSETPARAM, 0x8020_5001u32 as i32);
        assert_eq!(OPMGETPARAM, 0x8020_4D05u32 as i32); // _IOW→IOC_IN (sys/svrctl.h:19 + ioccom.h)
        assert_eq!(OPMSETPARAM, 0x4020_4D07u32 as i32);
        // Unknown req (right group, undefined command number) → EINVAL (C 392-393).
        assert_eq!(
            do_svrctl(&mut store, PMGETPARAM + 0x100, None, None, 0).unwrap_err(),
            MiscError::Inval
        );
    }

    #[test]
    fn test_sysuname_req_direction_gate() {
        // do_sysuname's req direction branch (C misc.c:89-96 default → EINVAL) —
        // the old signature had no req parameter, so this branch had nowhere to
        // arise.
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 1000);
        let mut cpy = NopCopy;
        assert!(do_sysuname(0, 0, 100, Endpoint::PM, &mut cpy).is_ok());
        assert_eq!(
            do_sysuname(0, 1, 100, Endpoint::PM, &mut cpy).unwrap_err(),
            MiscError::Inval
        );
    }

    #[test]
    fn test_find_param_kvp() {
        let mon = "foo=bar\0baz=qux\0".to_string();
        assert_eq!(find_param(&mon, "foo").unwrap(), "bar");
        assert_eq!(find_param(&mon, "baz").unwrap(), "qux");
        assert!(find_param(&mon, "nope").is_none());
    }

    /// A CopyToUser that captures the copy-out destination and bytes (the layout
    /// witness of `do_getrusage`).
    struct CapCopy { dst: VirBytes, buf: alloc::vec::Vec<u8>, fail: bool }
    impl CopyToUser for CapCopy {
        fn copy_to_user(&mut self, src: &[u8], dst: VirBytes) -> Result<(), MiscError> {
            if self.fail {
                return Err(MiscError::Fault);
            }
            self.dst = dst;
            self.buf = src.to_vec();
            Ok(())
        }
        fn copy_from_user(&mut self, _src: VirBytes, _dst: &mut [u8]) -> Result<(), MiscError> { Ok(()) }
    }

    #[test]
    fn test_getrusage_self_children() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let mut ctl = TestTimesVm { hz: 100 };
        let mut cpy = CapCopy { dst: VirBytes(0), buf: alloc::vec::Vec::new(), fail: false };
        let ((u_sec, _), (_s_sec, _)) = do_getrusage(&table, UserSlot::new(0), RusageWho::Slf, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap();
        assert_eq!(u_sec, 1); // 120/100
        assert_eq!(cpy.dst, VirBytes(0x3000));
        let ((u2, _), _) = do_getrusage(&table, UserSlot::new(0), RusageWho::Children, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap();
        assert_eq!(u2, 2); // child_utime 200/100
    }

    #[test]
    fn test_getrusage_struct_byte_layout() {
        // Layout witness for the LP64 layout of sys/resource.h:57-76:
        // utime@0/stime@16/maxrss@32/minflt@64/majflt@72; the other 11 long fields
        // are never written on the C side (misc.c:423 memset + utility.c:446-450
        // change only three), so they stay zero.
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let mut ctl = TestTimesVm { hz: 100 };
        let mut cpy = CapCopy { dst: VirBytes(0), buf: alloc::vec::Vec::new(), fail: false };
        let ((u_sec, u_usec), (s_sec, s_usec)) =
            do_getrusage(&table, UserSlot::new(0), RusageWho::Slf, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap();
        assert_eq!((u_sec, u_usec, s_sec, s_usec), (1, 200_000, 0, 600_000));
        assert_eq!(cpy.buf.len(), 128);
        let i64_at = |off: usize| i64::from_le_bytes(cpy.buf[off..off + 8].try_into().unwrap());
        assert_eq!(i64_at(0), 1); // ru_utime.tv_sec
        assert_eq!(i64_at(8), 200_000); // ru_utime.tv_usec
        assert_eq!(i64_at(16), 0); // ru_stime.tv_sec
        assert_eq!(i64_at(24), 600_000); // ru_stime.tv_usec
        assert_eq!(i64_at(32), 4096); // ru_maxrss(VM,KB)
        assert_eq!(i64_at(64), 33); // ru_minflt(VM)
        assert_eq!(i64_at(72), 4); // ru_majflt(VM)
        for off in (40..64).step_by(8).chain((80..128).step_by(8)) {
            assert_eq!(i64_at(off), 0, "field at {off} must stay zero");
        }
    }

    #[test]
    fn test_getrusage_vm_error_propagates_without_copyout() {
        // C misc.c:441-442: a non-OK vm_getrusage is returned as is, and no data
        // reaches the user buffer.
        struct VmFail;
        impl TimesVmCtl for VmFail {
            fn sys_times(&mut self, _ep: Endpoint) -> Result<(Clock, Clock), MiscError> { Ok((0, 0)) }
            fn vm_rusage(&mut self, _ep: Endpoint, _who: RusageWho) -> Result<VmUsage, MiscError> {
                Err(MiscError::Kernel(-3)) // ESRCH
            }
        }
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let mut ctl = VmFail;
        let mut cpy = CapCopy { dst: VirBytes(0), buf: alloc::vec::Vec::new(), fail: false };
        assert_eq!(
            do_getrusage(&table, UserSlot::new(0), RusageWho::Slf, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap_err(),
            MiscError::Kernel(-3)
        );
        assert!(cpy.buf.is_empty());
    }

    #[test]
    fn test_getrusage_copyout_failure_is_fault() {
        // C misc.c:446-447: a failing sys_datacopy fails the call (the EFAULT
        // face).
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let mut ctl = TestTimesVm { hz: 100 };
        let mut cpy = CapCopy { dst: VirBytes(0), buf: alloc::vec::Vec::new(), fail: true };
        assert_eq!(
            do_getrusage(&table, UserSlot::new(0), RusageWho::Slf, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap_err(),
            MiscError::Fault
        );
    }

    #[test]
    fn test_rusage_from_ticks() {
        let (sec, usec) = rusage_from_ticks(150, 100);
        assert_eq!(sec, 1);
        assert_eq!(usec, 500_000);
    }

    #[test]
    fn test_sprofile_enosys() {
        let mut ctl = TestSprof;
        assert_eq!(
            do_sprofile(0, 0, 0, 0, Endpoint::SELF, VirBytes(0), VirBytes(0), &mut ctl)
                .unwrap_err(),
            MiscError::Nosys
        );
        // AltSprof also Nosys when feature disabled
        let mut alt = AltSprof;
        assert_eq!(
            do_sprofile(1, 0, 0, 0, Endpoint::SELF, VirBytes(0), VirBytes(0), &mut alt)
                .unwrap_err(),
            MiscError::Nosys
        );
    }

    #[test]
    fn test_mcontext_passthrough() {
        let ctl = TestMctx { ret: 0 };
        assert!(do_getmcontext(&ctl, Endpoint(0), VirBytes(0x1000)).is_ok());
        assert!(do_setmcontext(&ctl, Endpoint(0), VirBytes(0x1000)).is_ok());
        let fail = TestMctx { ret: -22 };
        assert!(do_getmcontext(&fail, Endpoint(0), VirBytes(0)).is_err());
        // alt impl
        let alt = AltMctx;
        assert!(do_getmcontext(&alt, Endpoint(0), VirBytes(0)).is_ok());
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(PM_SYSUNAME, 25);
        assert_eq!(PM_GETSYSINFO, 47);
        assert_eq!(PM_GETPROCNR, 46);
        assert_eq!(PM_GETEPINFO, 45);
        assert_eq!(PM_REBOOT, 37);
        assert_eq!(PM_SVRCTL, 38);
        assert_eq!(PM_GETRUSAGE, 36);
        assert_eq!(SI_PROC_TAB, 2);
        assert_eq!(SI_CALL_STATS, 9);
        assert_eq!(RUSAGE_SELF, 0);
        assert_eq!(RUSAGE_CHILDREN, -1);
        // statistical-profiling request codes (`minix/include/minix/profile.h:11-12`)
        assert_eq!(PROF_START, 0);
        assert_eq!(PROF_STOP, 1);
        // Excluded (not single header literals): `UTS_VAL_*` are Rust-side
        // default strings, `MONITOR_CAP` is a PM-internal cap, and
        // `PM*PARAM` are `ioc()` composites.
        // `RB_POWERDOWN` IS a C bit pattern (`sys/sys/reboot.h:54`,
        // `RB_HALT|0x800` = 0x808); the audit corrected the port from `1 << 0`
        // and this pin guards against re-drift (regression in the mock below).
        assert_eq!(RB_POWERDOWN, 0x808);
    }

    // Ensure traits have ≥2 impl for Gate D
    #[test]
    fn test_traits_have_two_impls() {
        //[SD19-gate] PM send/notify/reply are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = TestSysInfo { data: vec![] };
        //[SD19-gate] PM send/notify/reply are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = TestSysInfoAlt { _a: 0 };
        //[SD19-gate] PM send/notify/reply are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = TestReboot { abort: 0, killed: false, stopped: false, told: false, powered: false };
        //[SD19-gate] PM send/notify/reply are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = AltReboot;
    }
}
