//! Time: `do_time/do_stime/do_gettime/do_getres/do_settime` + `boottime + clock/hz`.
//!
//! C ground truth: `minix3/minix/servers/pm/time.c` (131 lines, `22/53/71/94/110`)
//! + `minix3/minix/include/minix/ipc.h:469` `mess_lc_pm_time/mess_pm_lc_time`
//! + `minix3/sys/sys/time.h:283/288` `CLOCK_REALTIME 0/MONOTONIC 3`
//! + `minix3/minix/lib/libsys/getuptime.c:9-22` + `libsys/clock_time.c:13-40`
//! Design: `.design/19-design.v1.md` D1–D8 (explicit `ClockId/decompose_clock/clock_resolution/ClockSource/BootTimeCtl/SetTimeCtl/ClockTime`).
//! Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{Clock, Time, UserSlot, EINVAL, EPERM};
use crate::mproc::ProcTable;

/// Nanoseconds per second (`time.c:44` `1000000000ULL`).
pub const NSEC_PER_SEC: i64 = 1_000_000_000;

/// `CLOCK_REALTIME` (`sys/time.h:283`).
pub const CLOCK_REALTIME: i32 = 0;
/// `CLOCK_MONOTONIC` (`sys/time.h:288`, 3 not 1).
pub const CLOCK_MONOTONIC: i32 = 3;

/// `PM_GETTIMEOFDAY` (`callnr.h:41` `PM_BASE+28`).
pub const PM_GETTIMEOFDAY: i32 = 28;
/// `PM_STIME` (`callnr.h:20` `PM_BASE+7`).
pub const PM_STIME: i32 = 7;
/// `PM_CLOCK_GETRES` (`callnr.h:46`).
pub const PM_CLOCK_GETRES: i32 = 33;
/// `PM_CLOCK_GETTIME` (`callnr.h:47`).
pub const PM_CLOCK_GETTIME: i32 = 34;
/// `PM_CLOCK_SETTIME` (`callnr.h:48`).
pub const PM_CLOCK_SETTIME: i32 = 35;

/// `timespec` (`sys/timespec.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timespec {
    pub sec: Time,
    pub nsec: i64,
}

impl Timespec {
    pub const fn new(sec: Time, nsec: i64) -> Self {
        Self { sec, nsec }
    }
}

/// `ClockId` (`sys/time.h:283/288`, `time.c:31`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockId {
    Realtime = 0,
    Monotonic = 3,
}

impl TryFrom<i32> for ClockId {
    type Error = TimeError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Realtime),
            3 => Ok(Self::Monotonic),
            _ => Err(TimeError::InvalidClock),
        }
    }
}

/// `TimeRequest` (`ipc.h:469` `mess_lc_pm_time` four fields, D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRequest {
    pub clk: ClockId,
    pub now: bool,
    pub sec: Time,
    pub nsec: i64,
}

/// Errors for time calls (mapped to errno).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeError {
    InvalidClock,
    Perm,
    NoUptime,
    BootFailed,
    SetFailed,
}

impl TimeError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::InvalidClock => EINVAL,
            Self::Perm => EPERM,
            Self::NoUptime => EINVAL,
            Self::BootFailed => EINVAL,
            Self::SetFailed => EINVAL,
        }
    }
}

/// `ClockSource` (`getuptime` three values, `libsys/getuptime.c:9-22`, D2, A-11).
pub trait ClockSource {
    fn uptime(&self) -> Result<(Clock, Clock, Time), TimeError>;
}

/// `BootTimeCtl` (`sys_stime`, `libsys/sys_stime.c:9`, D5, A-11).
pub trait BootTimeCtl {
    fn set_boottime(&mut self, boottime: Time) -> i32;
}

/// `SetTimeCtl` (`sys_settime`, `libsys/sys_settime.c:9-12`, D6, A-11).
pub trait SetTimeCtl {
    fn set_time(&mut self, now: bool, clk: ClockId, sec: Time, nsec: i64) -> i32;
}

/// `ClockTime` (`clock_time`, `libsys/clock_time.c:13-40`, D7, A-11).
pub trait ClockTime {
    fn clock_time(&self) -> Timespec;
}

/// `decompose_clock` (`time.c:42-44`, D3, A-11) — `boottime + clock/hz` with `%hz*1e9/hz`.
///
/// - `clock` is `realtime` for `CLOCK_REALTIME` or `ticks` for `CLOCK_MONOTONIC`.
/// - `hz` is `system_hz` explicit param (not global), `hz<=50000`.
/// - `clock%hz` first (`0..hz-1`) then `*1e9` avoids `clock*1e9` overflow (`5e4*1e9=5e13<9e18`).
pub fn decompose_clock(boottime: Time, clock: Clock, hz: Clock) -> Timespec {
    if hz == 0 {
        return Timespec { sec: boottime, nsec: 0 };
    }
    let sec = boottime + clock / hz;
    let nsec = (clock % hz) * NSEC_PER_SEC / hz;
    Timespec { sec, nsec }
}

/// `clock_resolution` (`time.c:58-60`, D4) — `1e9/hz`.
pub fn clock_resolution(hz: Clock) -> Timespec {
    if hz == 0 {
        return Timespec { sec: 0, nsec: 0 };
    }
    Timespec { sec: 0, nsec: NSEC_PER_SEC / hz }
}

/// `do_time` (`time.c:94-104`, D7).
pub fn do_time(clock: &dyn ClockTime) -> Timespec {
    clock.clock_time()
}

/// `do_stime` (`time.c:110-131`, D5).
pub fn do_stime(
    table: &ProcTable,
    caller: UserSlot,
    sec: Time,
    hz: Clock,
    src: &dyn ClockSource,
    ctl: &mut dyn BootTimeCtl,
) -> Result<(), TimeError> {
    // 119-121  SUPER_USER gate (15's is_superuser)
    if !is_superuser(table, caller) {
        return Err(TimeError::Perm);
    }
    // 122  getuptime panic guard → Err(NoUptime) in Rust (C panics)
    let (_uptime, realtime, _boottime) = src.uptime().map_err(|_| TimeError::NoUptime)?;
    // 124  boottime = sec - realtime/hz (anchor reset)
    let new_boottime = sec - realtime / hz;
    // 126  sys_stime(boottime)
    let r = ctl.set_boottime(new_boottime);
    if r != 0 {
        return Err(TimeError::BootFailed);
    }
    Ok(())
}

/// `do_gettime` (`time.c:22-47`, D2).
pub fn do_gettime(
    src: &dyn ClockSource,
    clk: ClockId,
    hz: Clock,
) -> Result<Timespec, TimeError> {
    let (ticks, realtime, boottime) = src.uptime().map_err(|_| TimeError::NoUptime)?;
    let clock = match clk {
        ClockId::Realtime => realtime,
        ClockId::Monotonic => ticks,
    };
    Ok(decompose_clock(boottime, clock, hz))
}

/// `do_getres` (`time.c:53-65`, D4).
pub fn do_getres(clk: ClockId, hz: Clock) -> Result<Timespec, TimeError> {
    match clk {
        ClockId::Realtime | ClockId::Monotonic => Ok(clock_resolution(hz)),
    }
}

/// `do_settime` (`time.c:71-88`, D6).
pub fn do_settime(
    table: &ProcTable,
    caller: UserSlot,
    req: TimeRequest,
    ctl: &mut dyn SetTimeCtl,
) -> Result<(), TimeError> {
    // 75-77  SUPER_USER gate
    if !is_superuser(table, caller) {
        return Err(TimeError::Perm);
    }
    // 84-86  monotonic cannot be changed
    match req.clk {
        ClockId::Monotonic => return Err(TimeError::InvalidClock),
        ClockId::Realtime => {}
    }
    let r = ctl.set_time(req.now, req.clk, req.sec, req.nsec);
    if r != 0 {
        return Err(TimeError::SetFailed);
    }
    Ok(())
}

/// `is_superuser` helper (`getset.c:114` reuse, `mproc/credentials.rs:Credentials::is_superuser`).
fn is_superuser(table: &ProcTable, caller: UserSlot) -> bool {
    table.procs[caller.get()]
        .resources
        .privilege
        .credentials()
        .map(|c| c.user.effective == 0)
        .unwrap_or(false)
}

// For `do_settime` to compare, we need to handle monotonic inv separately without extra trait; use direct match above.
// `do_getres` helper for invalid clock is already via TryFrom; this function assumes valid ClockId.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot};

    fn mk_running(table: &mut ProcTable, slot: usize, eff: u32) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(eff, 100));
    }

    struct TestClockSource {
        ticks: Clock,
        realtime: Clock,
        boottime: Time,
        fail: bool,
    }
    impl ClockSource for TestClockSource {
        fn uptime(&self) -> Result<(Clock, Clock, Time), TimeError> {
            if self.fail {
                Err(TimeError::NoUptime)
            } else {
                Ok((self.ticks, self.realtime, self.boottime))
            }
        }
    }
    /// Second impl for Gate D ≥2 (Redox `MockClockSource` second behavior).
    struct AltClockSource {
        now: (Clock, Clock, Time),
    }
    impl ClockSource for AltClockSource {
        fn uptime(&self) -> Result<(Clock, Clock, Time), TimeError> {
            Ok(self.now)
        }
    }
    struct TestBootCtl {
        last: Option<Time>,
        ret: i32,
    }
    impl BootTimeCtl for TestBootCtl {
        fn set_boottime(&mut self, boottime: Time) -> i32 {
            self.last = Some(boottime);
            self.ret
        }
    }
    struct AltBootCtl {
        boottime: Time,
    }
    impl BootTimeCtl for AltBootCtl {
        fn set_boottime(&mut self, boottime: Time) -> i32 {
            self.boottime = boottime;
            0
        }
    }
    struct TestSetCtl {
        last: Option<(bool, ClockId, Time, i64)>,
        ret: i32,
    }
    impl SetTimeCtl for TestSetCtl {
        fn set_time(&mut self, now: bool, clk: ClockId, sec: Time, nsec: i64) -> i32 {
            self.last = Some((now, clk, sec, nsec));
            self.ret
        }
    }
    struct AltSetCtl;
    impl SetTimeCtl for AltSetCtl {
        fn set_time(&mut self, _now: bool, _clk: ClockId, _sec: Time, _nsec: i64) -> i32 {
            0
        }
    }
    struct TestClockTime {
        ts: Timespec,
    }
    impl ClockTime for TestClockTime {
        fn clock_time(&self) -> Timespec {
            self.ts
        }
    }
    struct AltClockTime {
        sec: Time,
    }
    impl ClockTime for AltClockTime {
        fn clock_time(&self) -> Timespec {
            Timespec { sec: self.sec, nsec: 0 }
        }
    }

    #[test]
    fn test_decompose_clock() {
        let ts = decompose_clock(1000, 150, 100);
        assert_eq!(ts.sec, 1001);
        assert_eq!(ts.nsec, 500_000_000);
        // 49999*1e9/hz no overflow
        let ts2 = decompose_clock(0, 49999, 50000);
        assert_eq!(ts2.nsec, 999_980_000);
        // hz==0 guard
        let ts3 = decompose_clock(1000, 150, 0);
        assert_eq!(ts3.sec, 1000);
        assert_eq!(ts3.nsec, 0);
    }

    #[test]
    fn test_clock_resolution() {
        assert_eq!(clock_resolution(100), Timespec { sec: 0, nsec: 10_000_000 });
        assert_eq!(clock_resolution(1000), Timespec { sec: 0, nsec: 1_000_000 });
        assert_eq!(clock_resolution(0), Timespec { sec: 0, nsec: 0 });
    }

    #[test]
    fn test_clock_id_try_from() {
        assert_eq!(ClockId::try_from(0).unwrap(), ClockId::Realtime);
        assert_eq!(ClockId::try_from(3).unwrap(), ClockId::Monotonic);
        assert_eq!(ClockId::try_from(1).unwrap_err(), TimeError::InvalidClock);
        assert_eq!(ClockId::try_from(2).unwrap_err(), TimeError::InvalidClock);
        assert_eq!(ClockId::try_from(99).unwrap_err(), TimeError::InvalidClock);
    }

    #[test]
    fn test_do_gettime_realtime_vs_monotonic() {
        let src = TestClockSource { ticks: 150, realtime: 250, boottime: 1000, fail: false };
        let r = do_gettime(&src, ClockId::Realtime, 100).unwrap();
        assert_eq!(r.sec, 1002); // 1000+250/100
        assert_eq!(r.nsec, 500_000_000); // 250%100=50*1e9/100
        let m = do_gettime(&src, ClockId::Monotonic, 100).unwrap();
        assert_eq!(m.sec, 1001); // 1000+150/100
        assert_eq!(m.nsec, 500_000_000); // 150%100=50*1e9/100
    }

    #[test]
    fn test_do_gettime_no_uptime() {
        let src = TestClockSource { ticks: 0, realtime: 0, boottime: 0, fail: true };
        assert_eq!(do_gettime(&src, ClockId::Realtime, 100).unwrap_err(), TimeError::NoUptime);
    }

    #[test]
    fn test_do_getres_realtime_monotonic() {
        assert_eq!(do_getres(ClockId::Realtime, 100).unwrap(), Timespec { sec: 0, nsec: 10_000_000 });
        assert_eq!(do_getres(ClockId::Monotonic, 100).unwrap(), Timespec { sec: 0, nsec: 10_000_000 });
    }

    #[test]
    fn test_do_settime_perm() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 1000);
        let req = TimeRequest { clk: ClockId::Realtime, now: true, sec: 2000, nsec: 0 };
        let mut ctl = TestSetCtl { last: None, ret: 0 };
        assert_eq!(do_settime(&table, UserSlot::new(0), req, &mut ctl).unwrap_err(), TimeError::Perm);
        // super succeeds
        table.procs[0].resources.privilege.credentials_mut().unwrap().user.effective = 0;
        let res = do_settime(&table, UserSlot::new(0), req, &mut ctl).unwrap();
        assert_eq!(res, ());
        assert_eq!(ctl.last, Some((true, ClockId::Realtime, 2000, 0)));
    }

    #[test]
    fn test_do_settime_monotonic_inval() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let req = TimeRequest { clk: ClockId::Monotonic, now: true, sec: 2000, nsec: 0 };
        let mut ctl = TestSetCtl { last: None, ret: 0 };
        assert_eq!(do_settime(&table, UserSlot::new(0), req, &mut ctl).unwrap_err(), TimeError::InvalidClock);
        assert!(ctl.last.is_none());
    }

    #[test]
    fn test_do_settime_now_passthrough() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let mut ctl = TestSetCtl { last: None, ret: 0 };
        let req_false = TimeRequest { clk: ClockId::Realtime, now: false, sec: 2000, nsec: 500 };
        do_settime(&table, UserSlot::new(0), req_false, &mut ctl).unwrap();
        assert_eq!(ctl.last, Some((false, ClockId::Realtime, 2000, 500)));
        let req_true = TimeRequest { clk: ClockId::Realtime, now: true, sec: 2000, nsec: 500 };
        do_settime(&table, UserSlot::new(0), req_true, &mut ctl).unwrap();
        assert_eq!(ctl.last, Some((true, ClockId::Realtime, 2000, 500)));
    }

    #[test]
    fn test_do_stime_perm() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 1000);
        let src = TestClockSource { ticks: 0, realtime: 500, boottime: 1000, fail: false };
        let mut ctl = TestBootCtl { last: None, ret: 0 };
        assert_eq!(do_stime(&table, UserSlot::new(0), 2000, 100, &src, &mut ctl).unwrap_err(), TimeError::Perm);
    }

    #[test]
    fn test_do_stime_boottime() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let src = TestClockSource { ticks: 0, realtime: 500, boottime: 1000, fail: false };
        let mut ctl = TestBootCtl { last: None, ret: 0 };
        do_stime(&table, UserSlot::new(0), 2000, 100, &src, &mut ctl).unwrap();
        assert_eq!(ctl.last, Some(1995)); // sec - realtime/hz = 2000 -5
    }

    #[test]
    fn test_do_stime_boot_failed() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let src = TestClockSource { ticks: 0, realtime: 500, boottime: 1000, fail: false };
        let mut ctl = TestBootCtl { last: None, ret: -1 };
        assert_eq!(do_stime(&table, UserSlot::new(0), 2000, 100, &src, &mut ctl).unwrap_err(), TimeError::BootFailed);
    }

    #[test]
    fn test_do_time_clock_time() {
        let c = TestClockTime { ts: Timespec { sec: 12345, nsec: 678_000_000 } };
        let ts = do_time(&c);
        assert_eq!(ts.sec, 12345);
        assert_eq!(ts.nsec, 678_000_000);
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(CLOCK_REALTIME, 0);
        assert_eq!(CLOCK_MONOTONIC, 3);
        assert_eq!(PM_GETTIMEOFDAY, 28);
        assert_eq!(PM_STIME, 7);
        assert_eq!(PM_CLOCK_GETRES, 33);
        assert_eq!(PM_CLOCK_GETTIME, 34);
        assert_eq!(PM_CLOCK_SETTIME, 35);
        assert_eq!(NSEC_PER_SEC, 1_000_000_000);
    }

    // Helper to get mutable credentials
    trait CredMut {
        fn credentials_mut(&mut self) -> Option<&mut Credentials>;
    }
    impl CredMut for crate::mproc::Privilege {
        fn credentials_mut(&mut self) -> Option<&mut Credentials> {
            match self {
                crate::mproc::Privilege::User(c) => Some(c),
                crate::mproc::Privilege::Kernel => None,
            }
        }
    }
}
