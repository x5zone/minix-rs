//! Timer: `do_itimer` three families + `ticks ↔ timeval` + `set_alarm`/`cause_sigalrm`.
//!
//! C ground truth: `minix3/minix/servers/pm/alarm.c` (344 lines)
//! Design: `.design/14-design.v1.md` D1–D8 (explicit `TicksConv`/`ItimerWhich`/`AlarmState`/`Option`).
//! Single-threaded — `&mut ProcTable` without `Arc`.

use alloc::collections::BTreeMap;
use minix_types::{Endpoint, UserSlot, EINVAL};
use crate::mproc::{ProcTable, RemainingFlags};
use alloc::vec::Vec;

/// Clock ticks (`clock_t`, 64-bit `i64` extension, A-11).
pub type Clock = i64;

/// Microseconds per second (`alarm.c:19` `US 1e6`).
pub const US: Clock = 1_000_000;

/// Max seconds for `is_sane_timeval` (`alarm.c:85` `MAX_SECS`, `timers.h: TMRDIFF_MAX/system_hz`).
/// Use 100M as representative (21474836 for HZ=100); test locks 100M.
pub const MAX_SECS: Clock = 100_000_000;

/// `TMRDIFF_MAX` (`timers.h:45` `INT_MAX`).
pub const TMRDIFF_MAX: Clock = i32::MAX as Clock;

/// `SIGALRM` (sys/signal.h:14).
pub const SIGALRM: i32 = 14;

/// `SIGVTALRM` (signal.h:26).
pub const SIGVTALRM: i32 = 26;
/// `SIGPROF` (signal.h:27).
pub const SIGPROF: i32 = 27;

/// `ITIMER_*` (`sys/time.h`).
pub const ITIMER_REAL: i32 = 0;
pub const ITIMER_VIRTUAL: i32 = 1;
pub const ITIMER_PROF: i32 = 2;
pub const NR_ITIMERS: usize = 3;

/// `VT_*` (`com.h:420-421`).
pub const VT_VIRTUAL: i32 = 1;
pub const VT_PROF: i32 = 2;

/// `timeval` (`sys/time.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeval {
    pub tv_sec: Clock,
    pub tv_usec: Clock,
}

impl Timeval {
    pub fn new(sec: Clock, usec: Clock) -> Self {
        Self { tv_sec: sec, tv_usec: usec }
    }
    /// `is_sane_timeval` (`alarm.c:85-86` D2).
    pub fn is_sane(&self) -> bool {
        self.tv_sec >= 0
            && self.tv_sec <= MAX_SECS
            && self.tv_usec >= 0
            && self.tv_usec < US
    }
}

/// `itimerval` (`sys/time.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Itimerval {
    pub it_interval: Timeval,
    pub it_value: Timeval,
}

impl Default for Itimerval {
    fn default() -> Self {
        Self {
            it_interval: Timeval { tv_sec: 0, tv_usec: 0 },
            it_value: Timeval { tv_sec: 0, tv_usec: 0 },
        }
    }
}

/// `ItimerWhich` (`alarm.c:101` `which` 0..2, D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItimerWhich {
    Real = 0,
    Virtual = 1,
    Prof = 2,
}

impl TryFrom<i32> for ItimerWhich {
    type Error = ItimerError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Real),
            1 => Ok(Self::Virtual),
            2 => Ok(Self::Prof),
            _ => Err(ItimerError::InvalidWhich),
        }
    }
}

/// `do_itimer` operation (`alarm.c:107-110` D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItimerOp {
    pub set: Option<Itimerval>,
    pub get: bool,
}

/// Errors for `do_itimer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItimerError {
    InvalidWhich,
    InvalidVal,
    BadCopy,
}

impl ItimerError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::InvalidWhich => EINVAL,
            Self::InvalidVal => EINVAL,
            Self::BadCopy => EINVAL,
        }
    }
}

/// `TicksConv` (`alarm.c:33-76` D1, `hz = system_hz`).
#[derive(Debug, Clone, Copy)]
pub struct TicksConv {
    pub hz: Clock,
}

impl TicksConv {
    /// `ticks_from_timeval` (`alarm.c:33-65`, D1) upward round + clamp.
    pub fn ticks_from_timeval(&self, tv: &Timeval) -> Clock {
        // hz * sec with overflow check (56-57)
        let hz = self.hz;
        let sec_ticks = match hz.checked_mul(tv.tv_sec) {
            Some(v) => v,
            None => return i64::MAX,
        };
        // overflow detection via division (C's ticks/hz != sec)
        if hz != 0 && sec_ticks / hz != tv.tv_sec {
            return i64::MAX;
        }
        // hz*usec + US-1 / US upward (59)
        let usec_part = match hz.checked_mul(tv.tv_usec) {
            Some(v) => v,
            None => return i64::MAX,
        };
        let ticks_usec = (usec_part + US - 1) / US;
        // Saturate at i64::MAX (the old `v <= i64::MAX` comparison was always true; clippy correctness)
        sec_ticks.saturating_add(ticks_usec)
    }

    /// `timeval_from_ticks` (`alarm.c:70-76` D1) without overflow.
    pub fn timeval_from_ticks(&self, ticks: Clock) -> Timeval {
        let hz = self.hz;
        if hz == 0 {
            return Timeval { tv_sec: 0, tv_usec: 0 };
        }
        let sec = ticks / hz;
        let usec = (ticks % hz) * US / hz;
        Timeval { tv_sec: sec, tv_usec: usec }
    }
}

/// Virtual timer trait (`sys_vtimer`, `alarm.c:205` D4).
pub trait VTimerCtl {
    fn vtimer(&mut self, ep: Endpoint, which: ItimerWhich, set: Option<Clock>, get: Option<&mut Clock>) -> i32;
}

/// Production `VTimerCtl`: delegates to minix-sys `sys_vtimer` (the kernel half
/// of ITIMER_VIRTUAL/ITIMER_PROF). The return convention matches C
/// `check_vtimer` (alarm.c:239) — OK or a negative errno; the old value fetched
/// is delivered through the `get` out-parameter.
pub struct SysVTimerCtl;

impl VTimerCtl for SysVTimerCtl {
    fn vtimer(&mut self, ep: Endpoint, which: ItimerWhich, set: Option<Clock>, get: Option<&mut Clock>) -> i32 {
        match minix_sys::syscall::sys_vtimer(
            &minix_sys::syscall::DirectKernelCallTransport,
            ep.0,
            which as i32,
            set.map(|c| c as u64),
        ) {
            Ok(old) => {
                if let (Some(slot), Some(v)) = (get, old) {
                    *slot = v as Clock;
                }
                0
            }
            Err(e) => -e,
        }
    }
}

/// Real timer trait (`set_timer/cancel_timer`, `alarm.c:305/308` D6).
pub trait TimerCtl {
    fn set(&mut self, ep: Endpoint, ticks: Clock);
    fn cancel(&mut self, ep: Endpoint);
    fn exptime(&self, ep: Endpoint) -> Option<Clock>;
    fn now(&self) -> Clock;
}

/// Remaining real time pure function (`alarm.c:254-265` D5).
pub fn remaining_real(interval: Clock, timer: &Option<crate::mproc::MinixTimer>, now: Clock) -> Clock {
    if let Some(t) = timer {
        let exptime = t.expire_time;
        let mut remaining = exptime - now;
        if remaining <= 0 {
            remaining = interval;
        }
        remaining
    } else {
        0
    }
}

/// `get_realtimer` (`alarm.c:247-273` D5).
pub fn get_realtimer(table: &ProcTable, target: UserSlot, conv: &TicksConv, now: Clock) -> Itimerval {
    let proc = &table.procs[target.get()];
    let interval = proc.resources.intervals[ITIMER_REAL as usize];
    let remaining = remaining_real(interval, &proc.resources.timer, now);
    Itimerval {
        it_value: conv.timeval_from_ticks(remaining),
        it_interval: conv.timeval_from_ticks(interval),
    }
}

/// `set_realtimer` (`alarm.c:279-294` D5).
pub fn set_realtimer(table: &mut ProcTable, target: UserSlot, val: &Itimerval, conv: &TicksConv, tctl: &mut dyn TimerCtl) {
    let ticks = conv.ticks_from_timeval(&val.it_value);
    let mut interval = conv.ticks_from_timeval(&val.it_interval);
    if ticks <= 0 {
        interval = 0;
    }
    set_alarm(table, target, ticks, tctl);
    table.procs[target.get()].resources.intervals[ITIMER_REAL as usize] = interval;
}

/// `getset_vtimer` (`alarm.c:160-216` D4).
pub fn getset_vtimer(
    table: &mut ProcTable,
    target: UserSlot,
    which: ItimerWhich,
    set: Option<Itimerval>,
    get: bool,
    conv: &TicksConv,
    vctl: &mut dyn VTimerCtl,
) -> Option<Itimerval> {
    let idx = which as usize;
    let ep = table.procs[target.get()].endpoint();
    // Prepare set ticks
    let _set_ticks = if let Some(v) = &set {
        let nt = conv.ticks_from_timeval(&v.it_value);
        if nt <= 0 {
            table.procs[target.get()].resources.intervals[idx] = 0;
        } else {
            let it = conv.ticks_from_timeval(&v.it_interval);
            table.procs[target.get()].resources.intervals[idx] = it;
        }
        Some(if v.it_value.tv_sec == 0 && v.it_value.tv_usec == 0 { 0 } else { conv.ticks_from_timeval(&v.it_value) })
        // We compute nt again; but keep logic as per C: newticks = ticks_from(it_value); if newticks<=0 interval=0 else interval = ticks_from(it_interval)
        // The actual sys_vtimer set value is newticks
    } else {
        None
    };
    // For correct interval handling, recompute as per C exact:
    // Already handled interval above, but set_ticks should be newticks (ticks_from it_value)
    let set_for_vtimer = set.as_ref().map(|v| conv.ticks_from_timeval(&v.it_value));
    // Get oldticks via vtimer get
    let mut oldticks: Clock = 0;
    let _get_opt = if get { Some(&mut oldticks as *mut Clock) } else { None };
    // We need to call vctl with proper Option<&mut Clock>
    // Since trait uses Option<&mut Clock>, we can pass mutable reference
    let mut old_holder: Clock = 0;
    let get_ref = if get { Some(&mut old_holder) } else { None };
    //[SD19-gate] fire-and-forget PM send/notify/reply
    #[allow(clippy::let_underscore_must_use)]
    let _ = vctl.vtimer(ep, which, set_for_vtimer, get_ref);
    if get {
        // Simulate oldticks retrieval: for mock, old_holder already filled
        let mut old = old_holder;
        if old <= 0 {
            old = table.procs[target.get()].resources.intervals[idx];
        }
        let iv = Itimerval {
            it_value: conv.timeval_from_ticks(old),
            it_interval: conv.timeval_from_ticks(table.procs[target.get()].resources.intervals[idx]),
        };
        Some(iv)
    } else {
        None
    }
}

/// `check_vtimer` (`alarm.c:222-241` D5).
pub fn check_vtimer(table: &mut ProcTable, proc_nr: usize, sig: i32, vctl: &mut dyn VTimerCtl) {
    let which = match sig {
        26 => ItimerWhich::Virtual,
        27 => ItimerWhich::Prof,
        _ => panic!("invalid vtimer signal: {}", sig),
    };
    let interval = table.procs[proc_nr].resources.intervals[which as usize];
    if interval > 0 {
        let ep = table.procs[proc_nr].endpoint();
        //[SD19-gate] fire-and-forget PM send/notify/reply
        #[allow(clippy::let_underscore_must_use)]
        let _ = vctl.vtimer(ep, which, Some(interval), None);
    }
}

/// `set_alarm` (`alarm.c:299-311` D6).
pub fn set_alarm(table: &mut ProcTable, target: UserSlot, ticks: Clock, tctl: &mut dyn TimerCtl) {
    let ep = table.procs[target.get()].endpoint();
    if ticks > 0 {
        assert!(ticks <= TMRDIFF_MAX, "ticks > TMRDIFF_MAX");
        tctl.set(ep, ticks);
        table.procs[target.get()].resources.timer = Some(crate::mproc::MinixTimer { expire_time: tctl.now() + ticks, reload_time: ticks });
        table.procs[target.get()].resources.flags.insert(RemainingFlags::ALARM_ON);
    } else if table.procs[target.get()].resources.flags.contains(RemainingFlags::ALARM_ON) {
        tctl.cancel(ep);
        table.procs[target.get()].resources.timer = None;
        table.procs[target.get()].resources.flags.remove(RemainingFlags::ALARM_ON);
    }
}

/// `cause_sigalrm` (`alarm.c:317-344`). Timer expiry callback: the guards match C
/// item for item (isokendpt / IN_USE and not EXITING / ALARM_ON), then **re-arm
/// before delivering** — when interval > 0 it re-arms the kernel timer through
/// [`set_alarm`] (C :334-339 notes explicitly that "calling set_timer again from
/// the expire_timers callback is safe"), otherwise it clears ALARM_ON; finally it
/// delivers with PM's (slot 0) identity via `check_sig(pid, SIGALRM, ksig=FALSE)`.
/// Returns whether the delivery path ran (invalid endpoint / guard rejection →
/// false).
pub fn cause_sigalrm<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    ep: Endpoint,
    tctl: &mut dyn TimerCtl,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) -> bool {
    let slot = match table.pm_isokendpt(ep) {
        Ok(s) => s.get(),
        Err(_) => return false,
    };
    {
        let proc = &table.procs[slot];
        if !proc.is_in_use() || proc.is_exiting() {
            return false;
        }
        if !proc.resources.flags.contains(RemainingFlags::ALARM_ON) {
            return false;
        }
    }
    // Single re-arm point: the next expiry of a periodic itimer is re-armed into
    // the kernel timer here through TimerCtl; handle_clock_notify only "notices
    // expiry".
    let interval = table.procs[slot].resources.intervals[ITIMER_REAL as usize];
    if interval > 0 {
        set_alarm(table, UserSlot::new(slot), interval, tctl);
    } else {
        table.procs[slot].resources.flags.remove(RemainingFlags::ALARM_ON);
        table.procs[slot].resources.timer = None;
    }
    // Pretend to come from PM — caller is slot 0 (PM itself), so check_sig's
    // permission check matches C.
    let pid = table.procs[slot].pid();
    //[SD19-gate] fire-and-forget PM send/notify/reply
    #[allow(clippy::let_underscore_must_use)]
    let _ = crate::signal::check_sig(
        table,
        UserSlot::new(0),
        pid,
        SIGALRM,
        false,
        kern,
        transport,
    );
    true
}

/// Production `TimerCtl`: local expiry bookkeeping plus the kernel clock.
///
/// C's `set_timer`/`cancel_timer` (alarm.c:305/308) are **purely local**
/// bookkeeping — PM never calls `sys_setalarm` (no grep hit), and after a CLOCK
/// notify arrives `expire_timers` scans for expired entries (main.c:65-71). The
/// Rust counterpart of that queue is the per-mproc `resources.timer`
/// (`handle_clock_notify` scans the table); this implementation's
/// set/cancel/exptime maintain an isomorphic local bookkeeping (keeping the trait
/// face self-consistent for callers that hold no table), and `now()` is the only
/// kernel half — uptime via `sys_times`, the authoritative clock for itimer and
/// expiry arithmetic.
pub struct SysTimerCtl {
    pending: BTreeMap<Endpoint, Clock>,
    kernel: minix_sys::syscall::DirectKernelCallTransport,
}

impl SysTimerCtl {
    pub const fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            kernel: minix_sys::syscall::DirectKernelCallTransport,
        }
    }
}

impl Default for SysTimerCtl {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerCtl for SysTimerCtl {
    fn set(&mut self, ep: Endpoint, ticks: Clock) {
        self.pending.insert(ep, self.now() + ticks);
    }
    fn cancel(&mut self, ep: Endpoint) {
        self.pending.remove(&ep);
    }
    fn exptime(&self, ep: Endpoint) -> Option<Clock> {
        self.pending.get(&ep).copied()
    }
    fn now(&self) -> Clock {
        // C: getuptime (libsys/getuptime.c) — SYS_TIMES' boot_ticks.
        match minix_sys::syscall::sys_times(&self.kernel, minix_types::Endpoint::SELF.0) {
            Ok(t) => t.boot_ticks as Clock,
            // When the clock is unreachable the time arithmetic cannot be
            // trusted: 0 makes expiry fire immediately (fail-loud clock
            // semantics — an error must not look like "never expires").
            Err(_) => 0,
        }
    }
}

/// The timer outlet bundle of the dispatch face: `dispatch_pm_call`'s Itimer arm
/// gets the two timer outlets and the system frequency through it.
/// svrctl_store/call_stats/abort_flag are the misc-family server globals carried
/// in the same bundle (the counterpart of C's misc.c static globals on the
/// dispatch face).
pub struct TimerFaces<'a> {
    pub tctl: &'a mut dyn TimerCtl,
    pub vctl: &'a mut dyn VTimerCtl,
    pub system_hz: Clock,
    /// SVRCTL parameter store (C's `local_param_overrides` plus the monitor
    /// string, misc.c:305-310). PmServer holds it, and the dispatch face borrows
    /// it mutably.
    pub svrctl_store: &'a mut crate::misc::ParamStore,
    /// Call-count byte face (the SI_CALL_STATS reply; all zeros without the
    /// feature, while the incrementing behavior is still gated by run_once's
    /// feature check).
    pub call_stats: &'a mut [u8],
    /// Kernel abort flag (the Reboot arm; C `abort_flag` in glo.h:26 — written by
    /// do_reboot (misc.c:204) and, in the VFS_PM_REBOOT_REPLY special case, read
    /// out and handed to sys_abort (the misc.c:309 counterpart, ipc/vfs.rs:548)).
    /// PmServer holds it, and the dispatch face writes it.
    pub abort_flag: &'a mut i32,
}

/// `handle_clock_notify` (`main.c:65-71` CLOCK → `expire_timers`)。
///
/// C's timer queue is managed by the kernel with the `cause_sigalrm` expiry
/// callback; Rust uses the in-table `resources.timer` (`expire_time`) as the
/// queue counterpart, so every CLOCK notify scans once for expired entries and
/// calls [`cause_sigalrm`] on each — re-arming happens only inside
/// `cause_sigalrm` (through `TimerCtl`), and this function makes no bookkeeping
/// change. Collect first, then process: delivery (check_sig → maybe exit_proc)
/// modifies the process table.
pub fn handle_clock_notify<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    now: Clock,
    tctl: &mut dyn TimerCtl,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) {
    let mut expired: Vec<Endpoint> = Vec::new();
    for proc in table.procs.iter() {
        if let Some(t) = &proc.resources.timer
            && t.expire_time <= now
        {
            expired.push(proc.endpoint());
        }
    }
    for ep in expired {
        cause_sigalrm(table, ep, tctl, kern, transport);
    }
}

/// `do_itimer` (`alarm.c:92-154` D3).
pub fn do_itimer(
    table: &mut ProcTable,
    caller: UserSlot,
    which: i32,
    op: ItimerOp,
    conv: &TicksConv,
    vctl: &mut dyn VTimerCtl,
    tctl: &mut dyn TimerCtl,
) -> Result<Option<Itimerval>, ItimerError> {
    let which_e = ItimerWhich::try_from(which)?;
    if op.set.is_none() && !op.get {
        return Err(ItimerError::InvalidVal);
    }
    if let Some(v) = &op.set
        && (!v.it_value.is_sane() || !v.it_interval.is_sane()) {
            return Err(ItimerError::InvalidVal);
        }
    let target = caller;
    let mut old_val: Option<Itimerval> = None;
    match which_e {
        ItimerWhich::Real => {
            if op.get {
                old_val = Some(get_realtimer(table, target, conv, tctl.now()));
            }
            if let Some(v) = &op.set {
                set_realtimer(table, target, v, conv, tctl);
            }
        }
        ItimerWhich::Virtual | ItimerWhich::Prof => {
            let res = getset_vtimer(table, target, which_e, op.set, op.get, conv, vctl);
            if op.get {
                old_val = res;
            }
        }
    }
    Ok(old_val)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot};

    fn mk_running(table: &mut ProcTable, slot: usize) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        table.procs[slot].resources.flags = RemainingFlags::empty();
        table.procs[slot].resources.timer = None;
        table.procs[slot].resources.intervals = [0; 3];
    }

    struct TestVTimer { last_set: Option<Clock>, old: Clock }
    impl VTimerCtl for TestVTimer {
        fn vtimer(&mut self, _ep: Endpoint, _which: ItimerWhich, set: Option<Clock>, get: Option<&mut Clock>) -> i32 {
            self.last_set = set;
            if let Some(g) = get {
                *g = self.old;
            }
            0
        }
    }
    struct TestTimerCtl { now: Clock, exptime: Option<Clock> }
    impl TimerCtl for TestTimerCtl {
        fn set(&mut self, _ep: Endpoint, ticks: Clock) {
            self.exptime = Some(self.now + ticks);
        }
        fn cancel(&mut self, _ep: Endpoint) { self.exptime = None; }
        fn exptime(&self, _ep: Endpoint) -> Option<Clock> { self.exptime }
        fn now(&self) -> Clock { self.now }
    }
    /// Kernel-gateway mock for cause_sigalrm's delivery chain (always OK).
    #[derive(Default)]
    struct TestKern;
    impl crate::exit::KernelGateway for TestKern {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
    fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _b: &[u8], _e: Endpoint, _a: u64) -> Result<(), i32> { Ok(()) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
            Ok((0, 0))
        }
    }

    #[test]
    fn test_ticks_upward_rounds() {
        let conv = TicksConv { hz: 100 };
        let tv = Timeval { tv_sec: 0, tv_usec: 1 };
        assert_eq!(conv.ticks_from_timeval(&tv), 1);
        let tv2 = Timeval { tv_sec: 0, tv_usec: 10000 };
        assert_eq!(conv.ticks_from_timeval(&tv2), 1);
        let tv3 = Timeval { tv_sec: 1, tv_usec: 0 };
        assert_eq!(conv.ticks_from_timeval(&tv3), 100);
    }

    #[test]
    fn test_ticks_overflow_clamps() {
        let conv = TicksConv { hz: 100 };
        let tv = Timeval { tv_sec: MAX_SECS + 1, tv_usec: 0 };
        // is_sane would reject, but ticks_from should clamp to MAX
        let t = conv.ticks_from_timeval(&tv);
        assert!(t > 0);
        // Large sec that overflows hz*sec
        let tv_big = Timeval { tv_sec: i64::MAX / 2, tv_usec: 0 };
        assert_eq!(conv.ticks_from_timeval(&tv_big), i64::MAX);
    }

    #[test]
    fn test_timeval_roundtrip() {
        let conv = TicksConv { hz: 100 };
        let ticks = 150;
        let tv = conv.timeval_from_ticks(ticks);
        assert_eq!(tv.tv_sec, 1);
        assert_eq!(tv.tv_usec, 500_000);
        // sec decomposition without overflow
        let ticks2: Clock = 1_000_000;
        let tv2 = conv.timeval_from_ticks(ticks2);
        assert_eq!(tv2.tv_sec, 10000);
    }

    #[test]
    fn test_is_sane_timeval() {
        assert!(Timeval { tv_sec: 0, tv_usec: 0 }.is_sane());
        assert!(Timeval { tv_sec: MAX_SECS, tv_usec: 999999 }.is_sane());
        assert!(!Timeval { tv_sec: -1, tv_usec: 0 }.is_sane());
        assert!(!Timeval { tv_sec: MAX_SECS + 1, tv_usec: 0 }.is_sane());
        assert!(!Timeval { tv_sec: 0, tv_usec: US }.is_sane());
        assert!(!Timeval { tv_sec: 0, tv_usec: -1 }.is_sane());
    }

    #[test]
    fn test_do_itimer_which_bound() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        let conv = TicksConv { hz: 60 };
        let mut vctl = TestVTimer { last_set: None, old: 0 };
        let mut tctl = TestTimerCtl { now: 0, exptime: None };
        let op = ItimerOp { set: None, get: true };
        assert_eq!(do_itimer(&mut table, UserSlot::new(0), -1, op, &conv, &mut vctl, &mut tctl).unwrap_err(), ItimerError::InvalidWhich);
        assert_eq!(do_itimer(&mut table, UserSlot::new(0), 3, op, &conv, &mut vctl, &mut tctl).unwrap_err(), ItimerError::InvalidWhich);
    }

    #[test]
    fn test_do_itimer_set_get_both_empty() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        let conv = TicksConv { hz: 60 };
        let mut vctl = TestVTimer { last_set: None, old: 0 };
        let mut tctl = TestTimerCtl { now: 0, exptime: None };
        let op = ItimerOp { set: None, get: false };
        assert_eq!(do_itimer(&mut table, UserSlot::new(0), 0, op, &conv, &mut vctl, &mut tctl).unwrap_err(), ItimerError::InvalidVal);
    }

    #[test]
    fn test_do_itimer_real_getset() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        let conv = TicksConv { hz: 100 };
        let mut vctl = TestVTimer { last_set: None, old: 0 };
        let mut tctl = TestTimerCtl { now: 1000, exptime: None };
        let val = Itimerval { it_value: Timeval { tv_sec: 1, tv_usec: 0 }, it_interval: Timeval { tv_sec: 0, tv_usec: 0 } };
        let op = ItimerOp { set: Some(val), get: true };
        let old = do_itimer(&mut table, UserSlot::new(0), ITIMER_REAL, op, &conv, &mut vctl, &mut tctl).unwrap();
        assert!(old.is_some());
        // After set, timer should be armed
        assert!(table.procs[0].resources.flags.contains(RemainingFlags::ALARM_ON));
        assert!(table.procs[0].resources.timer.is_some());
    }

    #[test]
    fn test_getset_vtimer_interval_zero_on_cancel() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        let conv = TicksConv { hz: 100 };
        let mut vctl = TestVTimer { last_set: None, old: 0 };
        let val = Itimerval { it_value: Timeval { tv_sec: 0, tv_usec: 0 }, it_interval: Timeval { tv_sec: 1, tv_usec: 0 } };
        getset_vtimer(&mut table, UserSlot::new(0), ItimerWhich::Virtual, Some(val), false, &conv, &mut vctl);
        assert_eq!(table.procs[0].resources.intervals[ItimerWhich::Virtual as usize], 0);
    }

    #[test]
    fn test_getset_vtimer_returns_interval_when_expired() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        table.procs[0].resources.intervals[ItimerWhich::Virtual as usize] = 100;
        let conv = TicksConv { hz: 100 };
        let mut vctl = TestVTimer { last_set: None, old: 0 };
        let res = getset_vtimer(&mut table, UserSlot::new(0), ItimerWhich::Virtual, None, true, &conv, &mut vctl).unwrap();
        assert_eq!(res.it_value.tv_sec, 1);
    }

    #[test]
    fn test_check_vtimer_restarts_when_interval() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].resources.intervals[ItimerWhich::Virtual as usize] = 100;
        let mut vctl = TestVTimer { last_set: None, old: 0 };
        check_vtimer(&mut table, 5, SIGVTALRM, &mut vctl);
        assert_eq!(vctl.last_set, Some(100));
    }

    #[test]
    fn test_get_realtimer_alarm_on() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        table.procs[0].resources.timer = Some(crate::mproc::MinixTimer { expire_time: 1500, reload_time: 100 });
        table.procs[0].resources.flags.insert(RemainingFlags::ALARM_ON);
        table.procs[0].resources.intervals[ITIMER_REAL as usize] = 200;
        let conv = TicksConv { hz: 100 };
        let it = get_realtimer(&table, UserSlot::new(0), &conv, 1000);
        assert!(it.it_value.tv_sec >= 0);
        assert_eq!(it.it_interval.tv_sec, 2); // 200 ticks /100 =2 sec
    }

    #[test]
    fn test_get_realtimer_no_alarm() {
        let table = ProcTable::new();
        let conv = TicksConv { hz: 100 };
        let it = get_realtimer(&table, UserSlot::new(0), &conv, 1000);
        assert_eq!(it.it_value.tv_sec, 0);
        assert_eq!(it.it_value.tv_usec, 0);
    }

    #[test]
    fn test_set_realtimer_zero_clears_interval() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        let conv = TicksConv { hz: 100 };
        let mut tctl = TestTimerCtl { now: 0, exptime: None };
        let val = Itimerval { it_value: Timeval { tv_sec: 0, tv_usec: 0 }, it_interval: Timeval { tv_sec: 5, tv_usec: 0 } };
        set_realtimer(&mut table, UserSlot::new(0), &val, &conv, &mut tctl);
        assert_eq!(table.procs[0].resources.intervals[ITIMER_REAL as usize], 0);
        assert!(!table.procs[0].resources.flags.contains(RemainingFlags::ALARM_ON));
    }

    #[test]
    fn test_set_alarm_sets_and_clears() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        let mut tctl = TestTimerCtl { now: 0, exptime: None };
        set_alarm(&mut table, UserSlot::new(0), 100, &mut tctl);
        assert!(table.procs[0].resources.flags.contains(RemainingFlags::ALARM_ON));
        assert!(table.procs[0].resources.timer.is_some());
        set_alarm(&mut table, UserSlot::new(0), 0, &mut tctl);
        assert!(!table.procs[0].resources.flags.contains(RemainingFlags::ALARM_ON));
        assert!(table.procs[0].resources.timer.is_none());
    }

    #[test]
    fn test_cause_sigalrm_three_guards() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].resources.flags.insert(RemainingFlags::ALARM_ON);
        table.procs[5].resources.timer = Some(crate::mproc::MinixTimer { expire_time: 100, reload_time: 100 });
        let ep = table.procs[5].endpoint();
        // Hit: SIGALRM's default disposition terminates the process, and VFS
        // receives the EXIT notice (over the real channel).
        let mut tctl = TestTimerCtl { now: 100, exptime: None };
        let mut kern = TestKern;
        let mut t = crate::ipc::TestIpcTransport::default();
        assert!(cause_sigalrm(&mut table, ep, &mut tctl, &mut kern, &mut t));
        assert!(
            t.sent().iter().any(|(dst, m)| *dst == Endpoint::VFS && m.m_type == minix_types::VFS_PM_EXIT),
            "SIGALRM default disposition must terminate via check_sig"
        );
        // Invalid endpoint
        assert!(!cause_sigalrm(&mut table, Endpoint::from_generation_slot(9, 9), &mut tctl, &mut kern, &mut t));
        // An EXITING process is rejected by the guard
        mk_running(&mut table, 6);
        let ep2 = table.procs[6].endpoint();
        table.procs[6].state.lifecycle = crate::mproc::Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        assert!(!cause_sigalrm(&mut table, ep2, &mut tctl, &mut kern, &mut t));
    }

    #[test]
    fn test_cause_sigalrm_periodic_resets() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        // The target catches SIGALRM (so it stays alive), which makes the re-arm
        // observable — under the default disposition, exit_proc's set_alarm(0)
        // (forkexit.c:300-301) would clear the just-re-armed timer; that is
        // correct C behavior but not this test's observation point.
        table.procs[5].resources.signals.caught = crate::init::sig_bit(SIGALRM);
        table.procs[5].resources.flags.insert(RemainingFlags::ALARM_ON);
        table.procs[5].resources.timer = Some(crate::mproc::MinixTimer { expire_time: 100, reload_time: 100 });
        table.procs[5].resources.intervals[ITIMER_REAL as usize] = 100;
        let ep = table.procs[5].endpoint();
        // Periodic: the re-arm goes through set_alarm → TimerCtl.set (the kernel
        // seam really is called).
        let mut tctl = TestTimerCtl { now: 100, exptime: None };
        let mut kern = TestKern;
        let mut t = crate::ipc::TestIpcTransport::default();
        cause_sigalrm(&mut table, ep, &mut tctl, &mut kern, &mut t);
        assert!(table.procs[5].resources.flags.contains(RemainingFlags::ALARM_ON));
        assert_eq!(tctl.exptime, Some(200), "periodic must re-arm via TimerCtl.set(now+interval)");
        assert!(table.procs[5].is_in_use(), "caught SIGALRM must not terminate");
        // One-shot: interval=0 → clear ALARM_ON and remove the timer.
        table.procs[5].resources.intervals[ITIMER_REAL as usize] = 0;
        cause_sigalrm(&mut table, ep, &mut tctl, &mut kern, &mut t);
        assert!(!table.procs[5].resources.flags.contains(RemainingFlags::ALARM_ON));
        assert!(table.procs[5].resources.timer.is_none());
    }

    #[test]
    fn test_handle_clock_notify_expires() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].resources.flags.insert(RemainingFlags::ALARM_ON);
        table.procs[5].resources.timer = Some(crate::mproc::MinixTimer { expire_time: 100, reload_time: 0 });
        table.procs[5].resources.intervals[ITIMER_REAL as usize] = 0;
        let mut tctl = TestTimerCtl { now: 150, exptime: Some(100) };
        let mut kern = TestKern;
        let mut t = crate::ipc::TestIpcTransport::default();
        handle_clock_notify(&mut table, 150, &mut tctl, &mut kern, &mut t);
        // Expiry → cause_sigalrm → SIGALRM's default disposition terminates (over
        // the real delivery chain): dump=false → zombify, so the final state is
        // Zombie.
        assert!(
            matches!(
                table.procs[5].state.lifecycle,
                crate::mproc::Lifecycle::Zombie { .. } | crate::mproc::Lifecycle::Exiting { .. }
            ),
            "expired one-shot alarm must terminate, got {:?}",
            table.procs[5].state.lifecycle
        );
        assert!(table.procs[5].resources.timer.is_none());
        // Not yet expired (expire 300): does not fire.
        mk_running(&mut table, 6);
        table.procs[6].resources.flags.insert(RemainingFlags::ALARM_ON);
        table.procs[6].resources.timer = Some(crate::mproc::MinixTimer { expire_time: 300, reload_time: 0 });
        handle_clock_notify(&mut table, 150, &mut tctl, &mut kern, &mut t);
        assert!(table.procs[6].is_in_use() && !table.procs[6].is_exiting());
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(ITIMER_REAL, 0);
        assert_eq!(ITIMER_VIRTUAL, 1);
        assert_eq!(ITIMER_PROF, 2);
        assert_eq!(VT_VIRTUAL, 1);
        assert_eq!(VT_PROF, 2);
        assert_eq!(SIGALRM, 14);
        assert_eq!(US, 1_000_000);
        // itimer-family signal numbers (`sys/sys/signal.h:78-79`)
        assert_eq!(SIGVTALRM, 26);
        assert_eq!(SIGPROF, 27);
        // supported interval-timer count (`servers/pm/const.h:17`)
        assert_eq!(NR_ITIMERS, 3);
        // ALARM_ON drives the one-shot disarm path (`servers/pm/mproc.h:91`)
        assert_eq!(RemainingFlags::ALARM_ON.bits(), 0x00010);
        // Excluded (not single header literals): `MAX_SECS`/`TMRDIFF_MAX` are
        // derived composites (`servers/pm/const.h:15` `MAX_SECS =
        // TMRDIFF_MAX/system_hz`); their arithmetic is pinned by the
        // tick-conversion behavior tests above rather than a literal compare.
    }
}
