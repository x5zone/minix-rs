//! PM service IPC message types.
//!
//! Defines the messages exchanged between PM and other services (Kernel, VM, VFS).

use crate::{EAGAIN, EINVAL, EIO, ENOMEM, ENOSYS, EPERM, ESRCH};

/// PM error type (the user-visible form of fork coordination errors; the
/// two unused zero-use enums `PmRequest`/`PmResponse` were removed in
/// favour of the typed `Mess*` structs that carry the PM wire surface, see
/// below).
/// PM error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmError {
    /// Process table is full.
    ProcTableFull,
    /// Out of memory.
    OutOfMemory,
    /// Invalid endpoint.
    InvalidEndpoint,
    /// Slot is already in use.
    SlotInUse,
    /// Permission denied (EPERM, e.g., non-RS srv_fork).
    PermissionDenied,
    /// Internal error.
    InternalError,
    /// Operation not implemented.
    NotImplemented,
}

impl crate::types::ToErrno for PmError {
    fn to_errno(&self) -> crate::types::Errno {
        crate::types::Errno::from_i32(self.to_errno())
    }
}

impl PmError {
    /// Converts error to errno value.
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::ProcTableFull => EAGAIN,
            Self::OutOfMemory => ENOMEM,
            Self::InvalidEndpoint => ESRCH,
            Self::SlotInUse => EINVAL,
            Self::PermissionDenied => EPERM,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
        }
    }
}

/// PM system call enumeration — the single source of call numbers (C:
/// `callnr.h:14-60`, `PM_BASE + 1` through `PM_BASE + 47`).
///
/// The discriminant is the call number in the message's `m_type`
/// (`callnr.h:9`: `PM_BASE = 0x000`). This enum and the `Mess*` payloads
/// below share one library and one source: the sender (`minix-sys` wire
/// wrappers), the receiver (PM dispatch) and the future signal-proxy loop
/// (kernel → PM → RS) all reference it here instead of copying numbers by
/// hand. Every `PM_*` constant below is derived from this enum, and the C
/// absolute-value pins hold the enum discriminants down at the same time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[allow(clippy::upper_case_acronyms)] // Keeps the shape of C's `PM_*` macro names.
pub enum PmCall {
    /// PM_EXIT — _exit(2)
    Exit = 1,
    /// PM_FORK — fork(2)
    Fork = 2,
    /// PM_WAIT4 — wait4(2)
    Wait4 = 3,
    /// PM_GETPID — get[p]pid(2)
    GetPid = 4,
    /// PM_SETUID — setuid(2)
    SetUid = 5,
    /// PM_GETUID — get[e]uid(2)
    GetUid = 6,
    /// PM_STIME — stime(2)
    Stime = 7,
    /// PM_PTRACE — ptrace(2)
    Ptrace = 8,
    /// PM_SETGROUPS — setgroups(2)
    SetGroups = 9,
    /// PM_GETGROUPS — getgroups(2)
    GetGroups = 10,
    /// PM_KILL — kill(2)
    Kill = 11,
    /// PM_SETGID — setgid(2)
    SetGid = 12,
    /// PM_GETGID — get[e]gid(2)
    GetGid = 13,
    /// PM_EXEC — execve(2)
    Exec = 14,
    /// PM_SETSID — setsid(2)
    SetSid = 15,
    /// PM_GETPGRP — getpgrp(2)
    GetPgrp = 16,
    /// PM_ITIMER — [gs]etitimer(2)
    Itimer = 17,
    /// PM_GETMCONTEXT — getmcontext(2)
    GetMContext = 18,
    /// PM_SETMCONTEXT — setmcontext(2)
    SetMContext = 19,
    /// PM_SIGACTION — sigaction(2)
    SigAction = 20,
    /// PM_SIGSUSPEND — sigsuspend(2)
    SigSuspend = 21,
    /// PM_SIGPENDING — sigpending(2)
    SigPending = 22,
    /// PM_SIGPROCMASK — sigprocmask(2)
    SigProcMask = 23,
    /// PM_SIGRETURN — sigreturn(2)
    SigReturn = 24,
    /// PM_SYSUNAME — sysuname(2) (obsolete)
    SysUname = 25,
    /// PM_GETPRIORITY — getpriority(2)
    GetPriority = 26,
    /// PM_SETPRIORITY — setpriority(2)
    SetPriority = 27,
    /// PM_GETTIMEOFDAY — gettimeofday(2)
    GetTimeOfDay = 28,
    /// PM_SETEUID — seteuid(2)
    SetEUid = 29,
    /// PM_SETEGID — setegid(2)
    SetEGid = 30,
    /// PM_ISSETUGID — issetugid(2)
    IsSetUid = 31,
    /// PM_GETSID — getsid(2)
    GetSid = 32,
    /// PM_CLOCK_GETRES — clock_getres(2)
    ClockGetRes = 33,
    /// PM_CLOCK_GETTIME — clock_gettime(2)
    ClockGetTime = 34,
    /// PM_CLOCK_SETTIME — clock_settime(2)
    ClockSetTime = 35,
    /// PM_GETRUSAGE — getrusage(2)
    GetRUsage = 36,
    /// PM_REBOOT — reboot(2)
    Reboot = 37,
    /// PM_SVRCTL — svrctl(2)
    SvrCtl = 38,
    /// PM_SPROF — sprofile(2)
    SProf = 39,
    /// PM_PROCEVENTMASK — proceventmask(2)
    ProcEventMask = 40,
    /// PM_SRV_FORK — srv_fork(2)
    SrvFork = 41,
    /// PM_SRV_KILL — srv_kill(2)
    SrvKill = 42,
    /// PM_EXEC_NEW — the new exec path (newexec)
    ExecNew = 43,
    /// PM_EXEC_RESTART — the exec restart path
    ExecRestart = 44,
    /// PM_GETEPINFO — getepinfo(2)
    GetEpInfo = 45,
    /// PM_GETPROCNR — getprocnr(2)
    GetProcNr = 46,
    /// PM_GETSYSINFO — getsysinfo(2)
    GetSysInfo = 47,
}

impl PmCall {
    /// Decode a message `m_type` into a registered PM call.
    ///
    /// Returns `None` when `nr` is outside 1..=47 (including the reserved
    /// value `nr == 0` and unregistered numbers); the caller (the dispatch
    /// layer) answers `None` with ENOSYS, matching C's
    /// `call_index >= NR_PM_CALLS || call_vec[call_index] == NULL → ENOSYS`
    /// (main.c:94-101).
    pub fn from_call_nr(nr: i32) -> Option<PmCall> {
        // Match discriminants inside 1..=47; the discriminant is the call
        // number.
        match nr {
            1 => Some(PmCall::Exit),
            2 => Some(PmCall::Fork),
            3 => Some(PmCall::Wait4),
            4 => Some(PmCall::GetPid),
            5 => Some(PmCall::SetUid),
            6 => Some(PmCall::GetUid),
            7 => Some(PmCall::Stime),
            8 => Some(PmCall::Ptrace),
            9 => Some(PmCall::SetGroups),
            10 => Some(PmCall::GetGroups),
            11 => Some(PmCall::Kill),
            12 => Some(PmCall::SetGid),
            13 => Some(PmCall::GetGid),
            14 => Some(PmCall::Exec),
            15 => Some(PmCall::SetSid),
            16 => Some(PmCall::GetPgrp),
            17 => Some(PmCall::Itimer),
            18 => Some(PmCall::GetMContext),
            19 => Some(PmCall::SetMContext),
            20 => Some(PmCall::SigAction),
            21 => Some(PmCall::SigSuspend),
            22 => Some(PmCall::SigPending),
            23 => Some(PmCall::SigProcMask),
            24 => Some(PmCall::SigReturn),
            25 => Some(PmCall::SysUname),
            26 => Some(PmCall::GetPriority),
            27 => Some(PmCall::SetPriority),
            28 => Some(PmCall::GetTimeOfDay),
            29 => Some(PmCall::SetEUid),
            30 => Some(PmCall::SetEGid),
            31 => Some(PmCall::IsSetUid),
            32 => Some(PmCall::GetSid),
            33 => Some(PmCall::ClockGetRes),
            34 => Some(PmCall::ClockGetTime),
            35 => Some(PmCall::ClockSetTime),
            36 => Some(PmCall::GetRUsage),
            37 => Some(PmCall::Reboot),
            38 => Some(PmCall::SvrCtl),
            39 => Some(PmCall::SProf),
            40 => Some(PmCall::ProcEventMask),
            41 => Some(PmCall::SrvFork),
            42 => Some(PmCall::SrvKill),
            43 => Some(PmCall::ExecNew),
            44 => Some(PmCall::ExecRestart),
            45 => Some(PmCall::GetEpInfo),
            46 => Some(PmCall::GetProcNr),
            47 => Some(PmCall::GetSysInfo),
            _ => None,
        }
    }

    /// Return the call number (the message `m_type` value, C: `PM_BASE + N`).
    pub fn call_nr(self) -> i32 {
        self as i32
    }
}

// ── Miscellaneous calls 18/19/25/37/38/39/47 ──
//
// C ground truth: callnr.h 18/19/25/37/38/39/47 plus the matching
// `mess_lc_pm_*` / `mess_lc_svrctl` structures in ipc.h.
// 45/46 (getepinfo/getprocnr) already landed with the E9 PmApi slice;
// the 37 constant was already anchored, and the structures are added here.
// Every constant value is derived from the [`PmCall`] discriminant (single
// source), and the pin tests hold the derived result down against the C
// absolute values.

/// C: `PM_GETMCONTEXT (PM_BASE + 18)` — callnr.h:31.
pub const PM_GETMCONTEXT: i32 = PmCall::GetMContext as i32;
/// C: `PM_SETMCONTEXT (PM_BASE + 19)` — callnr.h:32.
pub const PM_SETMCONTEXT: i32 = PmCall::SetMContext as i32;
/// C: `PM_SYSUNAME (PM_BASE + 25)` — callnr.h:38 (C notes it as obsolete; behaviour is kept).
pub const PM_SYSUNAME: i32 = PmCall::SysUname as i32;
/// C: `PM_SVRCTL (PM_BASE + 38)` — callnr.h:51.
pub const PM_SVRCTL: i32 = PmCall::SvrCtl as i32;
/// C: `PM_SPROF (PM_BASE + 39)` — callnr.h:52.
pub const PM_SPROF: i32 = PmCall::SProf as i32;
// PM_GETSYSINFO (PM_BASE + 47, callnr.h:60) already has its authoritative
// definition in `ipc/sysinfo.rs` (the PM table request, E-MIBPROD/E-ISPROD
// area) — it is not repeated here.

/// The `mcontext_t` pointer payload (shared by GETMCONTEXT/SETMCONTEXT).
///
/// C: `mess_lc_pm_mcontext` — ipc.h:477-481. LP64: the pointer widens from
/// 4 to 8 bytes and padding goes 52→48, keeping the total at 56.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmMcontext {
    /// `mcontext_t *` — the user-space context storage address.
    pub ctx: u64,
    _pad: [u8; 48],
}

impl Default for MessLcPmMcontext {
    fn default() -> Self {
        Self { ctx: 0, _pad: [0; 48] }
    }
}
impl MessLcPmMcontext {
    pub const fn new() -> Self {
        Self { ctx: 0, _pad: [0; 48] }
    }
}

/// REBOOT payload.
///
/// C: `mess_lc_pm_reboot` — ipc.h:503-507 (int how + pad52, the same
/// 56-byte shape under LP64).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmReboot {
    /// Reboot mode (RBT_HALT/RBT_REBOOT/RBT_PANIC).
    pub how: i32,
    _pad: [u8; 52],
}
impl MessLcPmReboot {
    pub const fn new(how: i32) -> Self {
        Self { how, _pad: [0; 52] }
    }
}

/// SYSUNAME payload.
///
/// C: `mess_lc_pm_sysuname` — ipc.h:565-571 (req/field/len/value).
/// LP64: len is 8-aligned → @8, value @16, padding 40→28, total 56.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSysuname {
    /// Operation code (the uname namespace request).
    pub req: i32,
    /// Field selector.
    pub field: i32,
    /// Buffer length.
    pub len: u64,
    /// User buffer pointer.
    pub value: u64,
    _pad: [u8; 28],
}

/// SVRCTL payload (PM server control; the same shape as the kernel/RS
/// `mess_lc_svrctl`).
///
/// C: `mess_lc_svrctl` — ipc.h:603-608 (unsigned long request +
/// vir_bytes arg). LP64: request@0, arg@8, padding 48, total 56.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSvrctl {
    /// SVRCTL request code.
    pub request: u64,
    /// Request argument (a user pointer or a numeric value).
    pub arg: u64,
    _pad: [u8; 40],
}

/// SPROF payload (PM-side statistical profile control).
///
/// C: `mess_lc_pm_sprof` — ipc.h:550-558. LP64: three ints plus three
/// 8-byte fields, padding 32→16, total 56.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSprof {
    /// Action code (start/stop).
    pub action: i32,
    /// Sampling frequency.
    pub freq: i32,
    /// Interrupt type.
    pub intr_type: i32,
    /// Control-block pointer.
    pub ctl_ptr: u64,
    /// Memory-region pointer.
    pub mem_ptr: u64,
    /// Memory-region size.
    pub mem_size: u64,
    _pad: [u8; 16],
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_pm_error_to_errno() {
        assert_eq!(PmError::ProcTableFull.to_errno(), EAGAIN);
        assert_eq!(PmError::OutOfMemory.to_errno(), ENOMEM);
        assert_eq!(PmError::InvalidEndpoint.to_errno(), ESRCH);
        assert_eq!(PmError::NotImplemented.to_errno(), ENOSYS);
    }
}

// ── E7: credential call numbers and wire structures (callnr.h:15-45 + ipc.h) ──
//
// PM_BASE = 0 (no offset base), so the call numbers here are the absolute
// values 4..=37.
// Layout: the i386 C header's m_lc_pm_* is all 4-byte fields plus padding
// to 56; under LP64 only the vir_bytes pointer needs 8-byte alignment
// (groups.ptr), and the padding shrinks accordingly (the rs_start LP64
// precedent).

/// C: `PM_GETPID` — callnr.h:17.
pub const PM_GETPID: i32 = PmCall::GetPid as i32;
/// C: `PM_SETUID` — callnr.h:18.
pub const PM_SETUID: i32 = PmCall::SetUid as i32;
/// C: `PM_GETUID` — callnr.h:19.
pub const PM_GETUID: i32 = PmCall::GetUid as i32;
/// C: `PM_PTRACE` — callnr.h:21.
pub const PM_PTRACE: i32 = PmCall::Ptrace as i32;
/// C: `PM_SETGROUPS` — callnr.h:22.
pub const PM_SETGROUPS: i32 = PmCall::SetGroups as i32;
/// C: `PM_GETGROUPS` — callnr.h:23.
pub const PM_GETGROUPS: i32 = PmCall::GetGroups as i32;
/// C: `PM_SETGID` — callnr.h:25.
pub const PM_SETGID: i32 = PmCall::SetGid as i32;
/// C: `PM_GETGID` — callnr.h:26.
pub const PM_GETGID: i32 = PmCall::GetGid as i32;
/// C: `PM_SETSID` — callnr.h:28.
pub const PM_SETSID: i32 = PmCall::SetSid as i32;
/// C: `PM_GETPRIORITY` — callnr.h:41.
pub const PM_GETPRIORITY: i32 = PmCall::GetPriority as i32;
/// C: `PM_SETPRIORITY` — callnr.h:42.
pub const PM_SETPRIORITY: i32 = PmCall::SetPriority as i32;
/// C: `PM_SETEUID` — callnr.h:44.
pub const PM_SETEUID: i32 = PmCall::SetEUid as i32;
/// C: `PM_SETEGID` — callnr.h:45.
pub const PM_SETEGID: i32 = PmCall::SetEGid as i32;
/// C: `PM_GETSID` — callnr.h:46.
pub const PM_GETSID: i32 = PmCall::GetSid as i32;
/// C: `PM_REBOOT` — callnr.h:52.
pub const PM_REBOOT: i32 = PmCall::Reboot as i32;

/// The SYS_SETUID / SYS_SETGID request payload (C: `mess_lc_pm_setuid` /
/// `mess_lc_pm_setgid` — ipc.h:525-529, 4-byte fields + padding[52]).
///
/// SETGID and SETUID share the layout (`gid_t` and `uid_t` are the same
/// width), so they share this struct; the call number distinguishes the
/// semantics (callnr.h:18/:25).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSetid {
    /// The new uid/gid. C: `uid_t uid` (setuid) or `gid_t gid` (setgid).
    pub id: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

/// The SYS_GETSID request payload (C: `mess_lc_pm_getsid` — ipc.h:453-457,
/// `pid_t pid` + padding[52]). A pid of 0 queries the calling process's
/// own session.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmGetsid {
    /// Target process pid (0 = the calling process). C: `pid_t pid`.
    pub pid: i32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 52],
}

/// The SYS_SETGROUPS / SYS_GETGROUPS request payload (C:
/// `mess_lc_pm_groups` — ipc.h:459-465: `int num; vir_bytes ptr`). LP64:
/// `ptr` is 8-byte aligned at offset 8 and the padding shrinks to 44 (the
/// total stays 56; the rs_start LP64 precedent).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmGroups {
    /// Group count (SET) or requested group count (GET). C: `int num`.
    pub num: i32,
    /// Pointer to the gid_t array. C: `vir_bytes ptr`.
    pub ptr: u64,
    /// Padding to 56 bytes (LP64: the C i386 value 48 becomes 40).
    pub _padding: [u8; 40],
}

#[cfg(test)]
mod credential_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C absolute-value pin: the credential call numbers
    /// (callnr.h:17-52, PM_BASE=0).
    #[test]
    fn test_pm_credential_call_numbers_match_c() {
        assert_eq!(PM_GETPID, 4); // callnr.h:17
        assert_eq!(PM_SETUID, 5); // callnr.h:18
        assert_eq!(PM_GETUID, 6); // callnr.h:19
        assert_eq!(PM_PTRACE, 8); // callnr.h:21
        assert_eq!(PM_SETGROUPS, 9); // callnr.h:22
        assert_eq!(PM_GETGROUPS, 10); // callnr.h:23
        assert_eq!(PM_SETGID, 12); // callnr.h:25
        assert_eq!(PM_GETGID, 13); // callnr.h:26
        assert_eq!(PM_SETSID, 15); // callnr.h:28
        assert_eq!(PM_GETPRIORITY, 26); // callnr.h:41
        assert_eq!(PM_SETPRIORITY, 27); // callnr.h:42
        assert_eq!(PM_SETEUID, 29); // callnr.h:44
        assert_eq!(PM_SETEGID, 30); // callnr.h:45
        assert_eq!(PM_GETSID, 32); // callnr.h:46
        assert_eq!(PM_REBOOT, 37); // callnr.h:52
    }

    /// Layout witness: all three wire structs are 56 bytes (the union
    /// payload size); under LP64 `groups.ptr` lands at offset 8 (u64
    /// alignment).
    #[test]
    fn test_pm_credential_wire_layouts() {
        assert_eq!(size_of::<MessLcPmSetid>(), 56);
        assert_eq!(offset_of!(MessLcPmSetid, id), 0);
        assert_eq!(size_of::<MessLcPmGetsid>(), 56);
        assert_eq!(offset_of!(MessLcPmGetsid, pid), 0);
        assert_eq!(size_of::<MessLcPmGroups>(), 56);
        assert_eq!(offset_of!(MessLcPmGroups, num), 0);
        assert_eq!(offset_of!(MessLcPmGroups, ptr), 8);
    }
}

// ── E7: signal control call numbers and wire structures (callnr.h:20-24 + ipc.h) ──

/// C: `PM_SIGACTION` — callnr.h:33.
pub const PM_SIGACTION: i32 = PmCall::SigAction as i32;
/// C: `PM_SIGSUSPEND` — callnr.h:34.
pub const PM_SIGSUSPEND: i32 = PmCall::SigSuspend as i32;
/// C: `PM_SIGPENDING` — callnr.h:35.
pub const PM_SIGPENDING: i32 = PmCall::SigPending as i32;
/// C: `PM_SIGPROCMASK` — callnr.h:36.
pub const PM_SIGPROCMASK: i32 = PmCall::SigProcMask as i32;
/// C: `PM_SIGRETURN` — callnr.h:37.
pub const PM_SIGRETURN: i32 = PmCall::SigReturn as i32;
/// C: `PM_KILL` — callnr.h:24 (KILL and SIGACTION share `mess_lc_pm_sig`).
pub const PM_KILL: i32 = PmCall::Kill as i32;

/// The SIGACTION/KILL request payload (C: `mess_lc_pm_sig` — ipc.h:528-540).
///
/// Field family: `pid` (target process, 0 = self), `nr` (signal number),
/// `act`/`oact` (user-space pointers to `struct sigaction`, signal.c:48-84),
/// `ret` (the sigreturn restore stub, signal.c:84). LP64 keeps the i386
/// layout (the 4-byte fields come first, so the 8-byte-aligned pointers
/// land automatically at 8/16/24).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSig {
    /// Target process pid. C: `pid_t pid`.
    pub pid: i32,
    /// Signal number. C: `int nr`.
    pub nr: i32,
    /// User-space `struct sigaction *`. C: `vir_bytes act`.
    pub act: u64,
    /// Out-parameter pointer for the old action. C: `vir_bytes oact`.
    pub oact: u64,
    /// The sigreturn restore stub. C: `vir_bytes ret`.
    pub ret: u64,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 24],
}

/// The SIGPROCMASK/SIGSUSPEND request payload (C: `mess_lc_pm_sigset` —
/// ipc.h:543-549).
///
/// `set` is a C `sigset_t` (sigtypes.h:59-62: `u32 bits[4]`, 16 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSigset {
    /// SIG_BLOCK/UNBLOCK/SETMASK/INQUIRE. C: `int how`.
    pub how: i32,
    /// Padding to offset 8 (LP64 `vir_bytes` alignment; i386 has no pad here).
    pub _pad: [u8; 4],
    /// Restore-stub pointer. C: `vir_bytes ctx`.
    pub ctx: u64,
    /// Signal mask. C: `sigset_t set` (16 bytes).
    pub set: [u32; 4],
    /// Padding to 56 bytes (LP64: the C i386 value 32 becomes 24).
    pub _padding: [u8; 24],
}

/// The SIGPROCMASK/SIGPENDING reply payload (C: `mess_pm_lc_sigset` — the
/// old mask / pending set comes back in the reply message, signal.c:117/97).
/// LP64: 16 bytes of `set` plus padding to 56.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessPmLcSigset {
    /// Old mask (SIGPROCMASK) or pending set (SIGPENDING). C: `sigset_t set`.
    pub set: [u32; 4],
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 40],
}

impl MessPmLcSigset {
    /// Zeroed form (tests and raw overlays).
    pub const fn zeroed() -> Self {
        Self { set: [0; 4], _padding: [0; 40] }
    }
}

/// Payload used by the signal manager to forward an "already received"
/// signal to the target service.
///
/// C: `mess_pm_lsys_sigs_signal` — `ipc.h:1815-1819` (`int num` + 52 bytes
/// of padding = 56). The sender is the signal manager (PM acts as the
/// service proxy: `pm/signal.c:470-473`; RS likewise: `rs/main.c:699-701`),
/// the message type is `SIGS_SIGNAL_RECEIVED`, and the receiver's SEF
/// hands this single number to its signal callback (`sef_signal.c:115-128`).
/// It is one of the two shapes of a signal request, the other being the
/// bitmap in the notify payload: the bitmap form comes from the kernel
/// `SYSTEM`, this form comes from the signal manager.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessPmLsysSigsSignal {
    /// The forwarded signal number. C: `int num`.
    pub num: i32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

#[cfg(test)]
mod sig_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C absolute-value pin: the signal control call numbers
    /// (callnr.h:33-37 + :24).
    #[test]
    fn test_pm_signal_call_numbers_match_c() {
        assert_eq!(PM_SIGACTION, 20); // callnr.h:33
        assert_eq!(PM_SIGSUSPEND, 21); // callnr.h:34
        assert_eq!(PM_SIGPENDING, 22); // callnr.h:35
        assert_eq!(PM_SIGPROCMASK, 23); // callnr.h:36
        assert_eq!(PM_SIGRETURN, 24); // callnr.h:37
        assert_eq!(PM_KILL, 11); // callnr.h:24
    }

    /// Layout witness: MessLcPmSig is 56 bytes (mess_lc_pm_sig —
    /// ipc.h:528-540), with the pointer fields at 8/16/24 under LP64.
    #[test]
    fn test_mess_lc_pm_sig_layout() {
        assert_eq!(size_of::<MessLcPmSig>(), 56);
        assert_eq!(offset_of!(MessLcPmSig, pid), 0);
        assert_eq!(offset_of!(MessLcPmSig, nr), 4);
        assert_eq!(offset_of!(MessLcPmSig, act), 8);
        assert_eq!(offset_of!(MessLcPmSig, oact), 16);
        assert_eq!(offset_of!(MessLcPmSig, ret), 24);
        assert_eq!(offset_of!(MessLcPmSig, _padding), 32);
    }

    /// Layout witness: MessLcPmSigset is 56 bytes (how/ctx/set/padding).
    #[test]
    fn test_mess_lc_pm_sigset_layout() {
        assert_eq!(size_of::<MessLcPmSigset>(), 56);
        assert_eq!(offset_of!(MessLcPmSigset, how), 0);
        assert_eq!(offset_of!(MessLcPmSigset, ctx), 8);
        assert_eq!(offset_of!(MessLcPmSigset, set), 16);
        assert_eq!(offset_of!(MessLcPmSigset, _padding), 32);
    }

    /// Layout witness: MessPmLsysSigsSignal is 56 bytes and `num` lands at
    /// offset 0 (C `int num` is the struct's first field — ipc.h:1815-1819).
    #[test]
    fn test_mess_pm_lsys_sigs_signal_layout() {
        assert_eq!(size_of::<MessPmLsysSigsSignal>(), 56);
        assert_eq!(offset_of!(MessPmLsysSigsSignal, num), 0); // ipc.h:1816
        assert_eq!(offset_of!(MessPmLsysSigsSignal, _padding), 4); // ipc.h:1818
    }
}

// ── E7: time call numbers and wire structures (callnr.h:19/40-43 + ipc.h:574-583) ──
//
// The six time calls (STIME 7 / GETTIMEOFDAY 28 / CLOCK_GETRES 33 /
// CLOCK_GETTIME 34 / CLOCK_SETTIME 35 / GETRUSAGE 36) share one
// `mess_lc_pm_time` arm in C, dispatched on `clk_id` and `now`
// (time.c:31/55/79).

/// C: `PM_STIME` — callnr.h:19 (PM_BASE + 7).
pub const PM_STIME: i32 = PmCall::Stime as i32;
/// C: `PM_GETTIMEOFDAY` — callnr.h:43 (PM_BASE + 28).
pub const PM_GETTIMEOFDAY: i32 = PmCall::GetTimeOfDay as i32;
/// C: `PM_CLOCK_GETRES` — callnr.h:48 (PM_BASE + 33).
pub const PM_CLOCK_GETRES: i32 = PmCall::ClockGetRes as i32;
/// C: `PM_CLOCK_GETTIME` — callnr.h:49 (PM_BASE + 34).
pub const PM_CLOCK_GETTIME: i32 = PmCall::ClockGetTime as i32;
/// C: `PM_CLOCK_SETTIME` — callnr.h:50 (PM_BASE + 35).
pub const PM_CLOCK_SETTIME: i32 = PmCall::ClockSetTime as i32;
/// C: `PM_GETRUSAGE` — callnr.h:51 (PM_BASE + 36).
pub const PM_GETRUSAGE: i32 = PmCall::GetRUsage as i32;

/// The payload shared by the time calls (C: `mess_lc_pm_time` —
/// ipc.h:574-583).
///
/// LP64 layout: `time_t`/`long` are both 8 bytes, and `_padding` shrinks
/// from the C i386 value 36 to 32 to keep the 56-byte payload (the A-3
/// precedent, following rs_start).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmTime {
    /// Seconds. C: `time_t sec`.
    pub sec: u64,
    /// POSIX clock id (CLOCK_REALTIME etc.). C: `clockid_t clk_id`.
    pub clk_id: i32,
    /// 0 = gradual mode (adjtime), non-zero = absolute set. C: `int now`.
    pub now: i32,
    /// Nanoseconds part. C: `long nsec`.
    pub nsec: i64,
    /// Padding to 56 bytes (LP64: the C i386 value 36 becomes 32).
    pub _padding: [u8; 32],
}

#[cfg(test)]
mod time_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C absolute-value pin: the time call numbers (callnr.h:19/43/48-51).
    #[test]
    fn test_pm_time_call_numbers_match_c() {
        assert_eq!(PM_STIME, 7); // callnr.h:19
        assert_eq!(PM_GETTIMEOFDAY, 28); // callnr.h:43
        assert_eq!(PM_CLOCK_GETRES, 33); // callnr.h:48
        assert_eq!(PM_CLOCK_GETTIME, 34); // callnr.h:49
        assert_eq!(PM_CLOCK_SETTIME, 35); // callnr.h:50
        assert_eq!(PM_GETRUSAGE, 36); // callnr.h:51
    }

    /// Layout witness: MessLcPmTime is 56 bytes; sec u64@0, clk_id@8,
    /// now@12, nsec@16 (under LP64 `time_t`/`long` are both 8 bytes, and
    /// padding 36→32 keeps the payload size).
    #[test]
    fn test_mess_lc_pm_time_layout() {
        assert_eq!(size_of::<MessLcPmTime>(), 56);
        assert_eq!(offset_of!(MessLcPmTime, sec), 0);
        assert_eq!(offset_of!(MessLcPmTime, clk_id), 8);
        assert_eq!(offset_of!(MessLcPmTime, now), 12);
        assert_eq!(offset_of!(MessLcPmTime, nsec), 16);
    }
}

// ── E7: itimer and exec call numbers and wire structures ──

/// C: `PM_ITIMER` — callnr.h:30 (PM_BASE + 17).
pub const PM_ITIMER: i32 = PmCall::Itimer as i32;
/// C: `PM_EXEC` — callnr.h:27 (PM_BASE + 14).
pub const PM_EXEC: i32 = PmCall::Exec as i32;
/// C: `PM_EXEC_NEW` — callnr.h:56 (PM_BASE + 43).
pub const PM_EXEC_NEW: i32 = PmCall::ExecNew as i32;
/// C: `PM_EXEC_RESTART` — callnr.h:57 (PM_BASE + 44).
pub const PM_EXEC_RESTART: i32 = PmCall::ExecRestart as i32;

// ── E9 PmApi area: RS service process-management call numbers and wire ──

/// `PM_SRV_FORK` — fork a child process for a system service
/// (callnr.h:54, PM_BASE + 41). The entry point of RS's self-upgrade chain
/// (C manager.c:576 `srv_fork`).
pub const PM_SRV_FORK: i32 = PmCall::SrvFork as i32;
/// `PM_GETEPINFO` — look up a process's credentials by endpoint
/// (callnr.h:58, PM_BASE + 45).
pub const PM_GETEPINFO: i32 = PmCall::GetEpInfo as i32;
/// `PM_GETPROCNR` — look up an endpoint by pid (callnr.h:59, PM_BASE + 46).
pub const PM_GETPROCNR: i32 = PmCall::GetProcNr as i32;

/// getepinfo request payload (C: `mess_lsys_pm_getepinfo` — ipc.h:1398-1406).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MessLsysPmGetepinfo {
    /// Target process endpoint. C: `endpoint_t endpt`.
    pub endpt: i32,
    /// LP64: alignment pad after `vir_bytes` was widened.
    pub _pad: u32,
    /// groups buffer pointer (0 = do not fetch the group list). C: `vir_bytes groups`.
    pub groups: u64,
    /// Buffer capacity (number of groups). C: `int ngroups`.
    pub ngroups: i32,
    /// LP64 alignment pad.
    pub _pad2: u32,
    /// LP64: the fields total 24, padding shrinks from 44 to 32, total 56.
    pub _padding: [u8; 32],
}

/// getepinfo reply payload (PM → caller. C: `mess_pm_lsys_getepinfo` —
/// ipc.h:1788-1798: all 4-byte fields, padding 36, total 56).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessPmLsysGetepinfo {
    /// Real uid. C: `uid_t uid`.
    pub uid: i32,
    /// Effective uid. C: `uid_t euid`.
    pub euid: i32,
    /// Real gid. C: `gid_t gid`.
    pub gid: i32,
    /// Effective gid. C: `gid_t egid`.
    pub egid: i32,
    /// Group count (meaningful when the groups buffer is non-empty). C: `int ngroups`.
    pub ngroups: i32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 36],
}

/// getprocnr request payload (C: `mess_lsys_pm_getprocnr` — ipc.h:1407-1412).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLsysPmGetprocnr {
    /// Process id. C: `pid_t pid`.
    pub pid: i32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 52],
}

/// getprocnr reply payload (PM → caller. C: `mess_pm_lsys_getprocnr` —
/// ipc.h:1800-1805).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessPmLsysGetprocnr {
    /// Process endpoint. C: `endpoint_t endpt`.
    pub endpt: i32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 52],
}

/// itimer request payload (C: `mess_lc_pm_itimer` — ipc.h:468-474).
///
/// `which`: ITIMER_REAL/ITIMER_VIRTUAL/ITIMER_PROF; `value`/`ovalue` are
/// user-space pointers to `struct itimerval`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmItimer {
    /// Timer type. C: `int which`.
    pub which: i32,
    /// New-value pointer (`struct itimerval *`). C: `vir_bytes value`.
    pub value: u64,
    /// Out-parameter pointer for the old value. C: `vir_bytes ovalue`.
    pub ovalue: u64,
    /// LP64: value/ovalue widen to 8 bytes (fields total 24), padding
    /// shrinks from 44 to 32, total 56.
    pub _padding: [u8; 32],
}

/// exec request payload (C: `mess_lc_pm_exec` — ipc.h:435-443:
/// name/namelen/frame/framelen/ps_str, five fields + padding[36]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmExec {
    /// Pointer to the new image name. C: `vir_bytes name`.
    pub name: u64,
    /// Name length. C: `size_t namelen`.
    pub namelen: u64,
    /// Pointer to the initial stack frame. C: `vir_bytes frame`.
    pub frame: u64,
    /// Stack frame length. C: `size_t framelen`.
    pub framelen: u64,
    /// ps_strings pointer. C: `vir_bytes ps_str`.
    pub ps_str: u64,
    /// LP64: the five 4-byte fields widen to 8 bytes (40 total), padding
    /// shrinks from 36 to 16, total 56.
    pub _padding: [u8; 16],
}

/// exec-restart reply payload (RS→PM). C: `mess_rs_pm_exec_restart` —
/// ipc.h:1869-1876: endpt/result/pc/ps_str + padding[40].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessRsPmExecRestart {
    /// Target process endpoint. C: `endpoint_t endpt`.
    pub endpt: i32,
    /// exec result. C: `int result`.
    pub result: i32,
    /// New entry PC. C: `vir_bytes pc`.
    pub pc: u64,
    /// ps_strings pointer. C: `vir_bytes ps_str`.
    pub ps_str: u64,
    /// LP64: pc/ps_str widen to 8 bytes (fields total 24), padding
    /// shrinks from 40 to 32, total 56.
    pub _padding: [u8; 32],
}

#[cfg(test)]
mod itimer_exec_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C absolute-value pin: the itimer/exec call numbers.
    #[test]
    fn test_itimer_exec_call_numbers_match_c() {
        assert_eq!(PM_ITIMER, 17); // callnr.h:30
        assert_eq!(PM_EXEC, 14); // callnr.h:27
        assert_eq!(PM_EXEC_NEW, 43); // callnr.h:56
        assert_eq!(PM_EXEC_RESTART, 44); // callnr.h:57
    }

    /// Layout witness: MessLcPmItimer is 56 bytes (which@0/value@8/ovalue@16).
    #[test]
    fn test_mess_lc_pm_itimer_layout() {
        assert_eq!(size_of::<MessLcPmItimer>(), 56);
        assert_eq!(offset_of!(MessLcPmItimer, which), 0);
        assert_eq!(offset_of!(MessLcPmItimer, value), 8);
        assert_eq!(offset_of!(MessLcPmItimer, ovalue), 16);
    }

    /// Layout witness: MessLcPmExec is 56 bytes (name/namelen/frame/framelen/ps_str).
    #[test]
    fn test_mess_lc_pm_exec_layout() {
        assert_eq!(size_of::<MessLcPmExec>(), 56);
        assert_eq!(offset_of!(MessLcPmExec, name), 0);
        assert_eq!(offset_of!(MessLcPmExec, namelen), 8);
        assert_eq!(offset_of!(MessLcPmExec, frame), 16);
        assert_eq!(offset_of!(MessLcPmExec, framelen), 24);
        assert_eq!(offset_of!(MessLcPmExec, ps_str), 32);
    }

    /// Layout witness: MessRsPmExecRestart is 56 bytes (endpt/result/pc/ps_str).
    #[test]
    fn test_mess_rs_pm_exec_restart_layout() {
        assert_eq!(size_of::<MessRsPmExecRestart>(), 56);
        assert_eq!(offset_of!(MessRsPmExecRestart, endpt), 0);
        assert_eq!(offset_of!(MessRsPmExecRestart, result), 4);
        assert_eq!(offset_of!(MessRsPmExecRestart, pc), 8);
        assert_eq!(offset_of!(MessRsPmExecRestart, ps_str), 16);
    }
}

#[cfg(test)]
mod pm_service_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// Absolute-value pin: the three E9 PmApi call numbers
    /// (callnr.h:54/:58/:59).
    #[test]
    fn test_pm_service_call_numbers() {
        assert_eq!(PM_SRV_FORK, 41);
        assert_eq!(PM_GETEPINFO, 45);
        assert_eq!(PM_GETPROCNR, 46);
    }

    /// Layout witness ×4: the getepinfo request/reply and the getprocnr
    /// request/reply (LP64: in the getepinfo request, groups is 8-aligned
    /// after a pad, total 56).
    #[test]
    fn test_pm_service_wire_layouts() {
        assert_eq!(size_of::<MessLsysPmGetepinfo>(), 56);
        assert_eq!(offset_of!(MessLsysPmGetepinfo, endpt), 0);
        assert_eq!(offset_of!(MessLsysPmGetepinfo, groups), 8);
        assert_eq!(offset_of!(MessLsysPmGetepinfo, ngroups), 16);

        assert_eq!(size_of::<MessPmLsysGetepinfo>(), 56);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, uid), 0);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, euid), 4);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, gid), 8);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, egid), 12);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, ngroups), 16);

        assert_eq!(size_of::<MessLsysPmGetprocnr>(), 56);
        assert_eq!(offset_of!(MessLsysPmGetprocnr, pid), 0);
        assert_eq!(size_of::<MessPmLsysGetprocnr>(), 56);
        assert_eq!(offset_of!(MessPmLsysGetprocnr, endpt), 0);
    }
}

#[cfg(test)]
mod g_batch_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// Absolute-value pins for the miscellaneous call numbers (C callnr.h
    /// 31/32/38/51/52/60, PM_BASE = 0).
    #[test]
    fn test_g_batch_call_numbers_match_c() {
        assert_eq!(PM_GETMCONTEXT, 18); // callnr.h:31
        assert_eq!(PM_SETMCONTEXT, 19); // callnr.h:32
        assert_eq!(PM_SYSUNAME, 25); // callnr.h:38
        assert_eq!(PM_SVRCTL, 38); // callnr.h:51
        assert_eq!(PM_SPROF, 39); // callnr.h:52
    }

    /// Layout witness: all five structures are 56 bytes (the PM wire
    /// total-length contract).
    #[test]
    fn test_g_batch_struct_sizes() {
        assert_eq!(size_of::<MessLcPmMcontext>(), 56);
        assert_eq!(size_of::<MessLcPmReboot>(), 56);
        assert_eq!(size_of::<MessLcPmSysuname>(), 56);
        assert_eq!(size_of::<MessLcPmSvrctl>(), 56);
        assert_eq!(size_of::<MessLcPmSprof>(), 56);
    }

    /// Field-offset witness (LP64 precedents: Mcontext/Rebook are
    /// single-field; Sysuname's len is 8-aligned → @8 and value @16;
    /// Svrctl has two 8-byte fields; Sprof has three ints, an alignment
    /// pad, and three 8-byte fields).
    #[test]
    fn test_g_batch_field_offsets() {
        assert_eq!(offset_of!(MessLcPmMcontext, ctx), 0);
        assert_eq!(offset_of!(MessLcPmReboot, how), 0);
        assert_eq!(offset_of!(MessLcPmSysuname, req), 0);
        assert_eq!(offset_of!(MessLcPmSysuname, field), 4);
        assert_eq!(offset_of!(MessLcPmSysuname, len), 8);
        assert_eq!(offset_of!(MessLcPmSysuname, value), 16);
        assert_eq!(offset_of!(MessLcPmSvrctl, request), 0);
        assert_eq!(offset_of!(MessLcPmSvrctl, arg), 8);
        assert_eq!(offset_of!(MessLcPmSprof, action), 0);
        assert_eq!(offset_of!(MessLcPmSprof, freq), 4);
        assert_eq!(offset_of!(MessLcPmSprof, intr_type), 8);
        assert_eq!(offset_of!(MessLcPmSprof, ctl_ptr), 16);
        assert_eq!(offset_of!(MessLcPmSprof, mem_ptr), 24);
        assert_eq!(offset_of!(MessLcPmSprof, mem_size), 32);
    }
}

/// Pin tests that travelled with the single source of call numbers (moved
/// up from the PM server's `ipc/calls.rs`): all 47 discriminants round
/// trip, and unregistered numbers are rejected.
#[cfg(test)]
mod pm_call_nr_tests {
    use super::PmCall;

    #[test]
    fn test_call_nr_roundtrip_all_registered() {
        // callnr.h:14-60 — all 47 call numbers decode and re-encode.
        for nr in 1..=47 {
            let call = PmCall::from_call_nr(nr).unwrap_or_else(|| panic!("nr {} unregistered", nr));
            assert_eq!(call.call_nr(), nr);
        }
    }

    #[test]
    fn test_call_nr_rejects_unregistered() {
        // nr == 0 is reserved (message type 0 is traditionally reserved);
        // 48 and above are out of range and unregistered.
        assert!(PmCall::from_call_nr(0).is_none());
        assert!(PmCall::from_call_nr(48).is_none());
        assert!(PmCall::from_call_nr(0x100).is_none());
        assert!(PmCall::from_call_nr(-1).is_none());
    }
}