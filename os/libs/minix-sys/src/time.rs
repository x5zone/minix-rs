//! Process CPU-time accounting and the wall clock: `times`, built on
//! `getrusage`.
//!
//! C correspondence spans three libc files, and none of them adds a wire
//! call of its own:
//!
//! * `minix3/minix/lib/libc/sys/getrusage.c:9-19` — one SENDREC to the
//!   process manager (`PM_GETRUSAGE`, `callnr.h:51`) whose payload is
//!   `mess_lc_pm_rusage` (`ipc.h:510-515`: the `who` selector at byte 0,
//!   the caller's buffer address at byte 8). The manager assembles the
//!   full `struct rusage` and copies it into the caller's address space;
//!   this side only supplies the sink.
//! * `minix3/minix/lib/libc/gen/times.c:65-92` — the composition this
//!   module exists for: `getrusage(RUSAGE_SELF)` fills `tms_utime`/
//!   `tms_stime`, `getrusage(RUSAGE_CHILDREN)` fills `tms_cutime`/
//!   `tms_cstime`, and one `gettimeofday` supplies the return value. A
//!   failure anywhere answers `-1` with the failing call's errno — which
//!   field of `tp` was already written when the failure hit is not
//!   rolled back (C does not either).
//! * `minix3/minix/lib/libc/gen/sysconf.c:104-117` — where the tick rate
//!   comes from. C's `CLK_TCK` expands to `sysconf(_SC_CLK_TCK)`, a
//!   sysctl of `kern.clockrate` whose `hz` lane is the answer; C caches
//!   it in a function static purely to save the call. This face re-reads
//!   it per `times` call: the library keeps no state, and the rate is
//!   fixed at boot (registered deviation — the cache is an optimization,
//!   not behavior). A non-positive rate is refused as `EINVAL`; C would
//!   walk into a division by zero there (`times.c:56` divides by
//!   `1000000 / clk_tck`), which is unreachable in practice because the
//!   manager's own clock runs at that rate.
//!
//! The tick conversion is C's own (`times.c:51-56`): seconds times the
//! rate plus microseconds divided by the precomputed `1000000 / hz` —
//! integer division in both places, so the sub-tick remainder truncates
//! (999999 µs at hz 100 is 99 ticks, not 100). The comment in C rejects
//! the overflow-free `(usec * hz) / 1000000` form on 32-bit grounds; the
//! division form is kept anyway because it *is* the observable behavior.
//!
//! # Wire contracts consumed
//!
//! * The manager's `GetRUsage` arm copies 128 bytes with LP64 lanes
//!   (seconds and microseconds 8 bytes each; the server decision lives in
//!   `servers/pm/src/misc.rs` `RUSAGE_LEN`). The 14 `long` fields of C's
//!   `struct rusage` (`sys/resource.h:57-76`) therefore span 128 bytes
//!   here — the last two C fields (`ru_nvcsw`/`ru_nivcsw`) sit past the
//!   copied window and are never written by any party, so the view ends
//!   at `ru_nsignals`.
//! * The wall-clock reply overlay is C `mess_pm_lc_time`
//!   (`ipc.h:1769-1773`: `sec`@0, `nsec`@8 on LP64) — written by the
//!   manager's `GetTimeOfDay` arm and read through
//!   [`crate::pm::gettimeofday_via`], which divides the nanoseconds down
//!   exactly as C's wrapper does (`gettimeofday.c:30`).

use crate::Errno;
use crate::ipc::IpcTransport;
use crate::pm::{PM_CALL_GETRUSAGE, gettimeofday_via, pm_endpoint};
use crate::syscall::{cleared_message, perform_syscall};
use minix_types::{Clock, Clockinfo, Timeval};

/// Report on the calling process itself.
///
/// C: `RUSAGE_SELF 0` (`sys/resource.h:56`).
pub const RUSAGE_SELF: i32 = 0;
/// Report on the caller's reaped children.
///
/// C: `RUSAGE_CHILDREN -1` (`sys/resource.h:57`).
pub const RUSAGE_CHILDREN: i32 = -1;

/// Resource usage of a process or its children — the manager's LP64
/// `struct rusage` view (module header: the copied window is 128 bytes
/// and ends at `ru_nsignals`; C `sys/resource.h:57-76` names the fields).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rusage {
    /// User CPU time. C: `ru_utime` — the only lanes `times` consumes.
    pub ru_utime: Timeval,
    /// System CPU time. C: `ru_stime`.
    pub ru_stime: Timeval,
    /// Peak resident set size in KiB (the manager fills it from the VM
    /// leg). C: `ru_maxrss`.
    pub ru_maxrss: i64,
    /// Integral shared-memory size — never written, always zero.
    pub ru_ixrss: i64,
    /// Integral unshared-data size — never written, always zero.
    pub ru_idrss: i64,
    /// Integral unshared-stack size — never written, always zero.
    pub ru_isrss: i64,
    /// Page reclaims (soft faults, from the VM leg). C: `ru_minflt`.
    pub ru_minflt: i64,
    /// Page faults (hard faults, from the VM leg). C: `ru_majflt`.
    pub ru_majflt: i64,
    /// Swaps — never written, always zero. C: `ru_nswap`.
    pub ru_nswap: i64,
    /// Block input operations — never written, always zero.
    pub ru_inblock: i64,
    /// Block output operations — never written, always zero.
    pub ru_oublock: i64,
    /// Messages sent — never written, always zero. C: `ru_msgsnd`.
    pub ru_msgsnd: i64,
    /// Messages received — never written, always zero. C: `ru_msgrcv`.
    pub ru_msgrcv: i64,
    /// Signals received — never written, always zero. C: `ru_nsignals`.
    pub ru_nsignals: i64,
}

impl Rusage {
    /// The all-zero snapshot: C's `do_getrusage` memsets the structure
    /// before filling it (`servers/pm` `misc.c:423`), so the never-written
    /// fields legitimately read as zero.
    #[must_use]
    pub const fn zeroed() -> Self {
        Self {
            ru_utime: Timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            ru_stime: Timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            ru_maxrss: 0,
            ru_ixrss: 0,
            ru_idrss: 0,
            ru_isrss: 0,
            ru_minflt: 0,
            ru_majflt: 0,
            ru_nswap: 0,
            ru_inblock: 0,
            ru_oublock: 0,
            ru_msgsnd: 0,
            ru_msgrcv: 0,
            ru_nsignals: 0,
        }
    }
}

/// CPU-time report for `times` — C `struct tms`
/// (`sys/sys/times.h:49-54`), four `clock_t` fields.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tms {
    /// User CPU time of the calling process. C: `tms_utime`.
    pub user_time: Clock,
    /// System CPU time of the calling process. C: `tms_stime`.
    pub system_time: Clock,
    /// User CPU time of reaped children. C: `tms_cutime`.
    pub children_user_time: Clock,
    /// System CPU time of reaped children. C: `tms_cstime`.
    pub children_system_time: Clock,
}

/// Asks the manager for a resource-usage snapshot into `r_usage`.
///
/// C: `getrusage` (`sys/getrusage.c:9-19`): the selector and the buffer
/// address travel in `mess_lc_pm_rusage`; the manager validates `who`
/// (`EINVAL` otherwise), assembles the structure, and copies it out. On
/// the wire this is one SENDREC with `PM_GETRUSAGE`
/// (`callnr.h:51`).
pub fn getrusage_via<T: IpcTransport>(
    transport: &T,
    who: i32,
    r_usage: &mut Rusage,
) -> Result<(), Errno> {
    let mut message = cleared_message();
    // SAFETY: lanes follow `mess_lc_pm_rusage` (ipc.h:510-515) — who@0,
    // the caller-side sink address @8; the manager's decode reads the
    // same lanes (servers/pm `decode.rs` `rusage`).
    unsafe {
        message.m_u.raw[0..4].copy_from_slice(&who.to_le_bytes());
        message.m_u.raw[8..16].copy_from_slice(&(r_usage as *mut Rusage as u64).to_le_bytes());
    }
    perform_syscall(transport, pm_endpoint(), PM_CALL_GETRUSAGE, &mut message).map(|_| ())
}

/// Converts a wall/CPU-time pair into clock ticks — C `CONVTCK`
/// (`times.c:51-56`): `sec * hz + usec / (1000000 / hz)`, both divisions
/// integer, so the sub-tick remainder truncates. Callers guarantee a
/// positive rate ([`clock_rate_via`] refuses anything else).
const fn convtck(t: &Timeval, hz: i64) -> Clock {
    t.tv_sec * hz + t.tv_usec / (1_000_000 / hz)
}

/// Reads the kernel tick rate — C `sysconf(_SC_CLK_TCK)`
/// (`sysconf.c:104-117`): a raw sysctl of `{CTL_KERN, KERN_CLOCKRATE}`
/// whose 20-byte `struct clockinfo` answer starts with `hz`
/// (`sys/sys/sysctl.h:206-212`). C caches the value in a function
/// static; this face re-reads it (module header — statelessness, and the
/// rate is boot-fixed). A non-positive answer is `EINVAL`, standing in
/// for the division by zero C would reach at `times.c:56`.
fn clock_rate_via<T: IpcTransport>(transport: &T) -> Result<i64, Errno> {
    let mut buffer = [0u8; core::mem::size_of::<Clockinfo>()];
    let mut length = buffer.len();
    crate::sysctl::sysctl_via(
        transport,
        &[minix_types::CTL_KERN, minix_types::KERN_CLOCKRATE],
        Some(crate::sysctl::SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    )?;
    let hz = i32::from_le_bytes(buffer[0..4].try_into().expect("hz lane"));
    if hz <= 0 {
        return Err(Errno::EINVAL);
    }
    Ok(hz as i64)
}

/// Reports CPU-time accounting in `buffer` and returns the elapsed wall
/// clock since boot in ticks (C: `times`, `gen/times.c:65-92`).
///
/// The order is C's: self usage, then children usage, then the wall
/// clock; a failure aborts with that call's errno and leaves `buffer`
/// exactly as far filled as C would have (`times.c` writes `tp` as it
/// goes and never rolls back). The tick rate is read once per call —
/// see [`clock_rate_via`].
pub fn times_via<T: IpcTransport>(transport: &T, buffer: &mut Tms) -> Result<Clock, Errno> {
    let hz = clock_rate_via(transport)?;
    let mut usage = Rusage::zeroed();
    getrusage_via(transport, RUSAGE_SELF, &mut usage)?;
    buffer.user_time = convtck(&usage.ru_utime, hz);
    buffer.system_time = convtck(&usage.ru_stime, hz);
    getrusage_via(transport, RUSAGE_CHILDREN, &mut usage)?;
    buffer.children_user_time = convtck(&usage.ru_utime, hz);
    buffer.children_system_time = convtck(&usage.ru_stime, hz);
    let (sec, nsec) = gettimeofday_via(transport)?;
    Ok(convtck(
        &Timeval {
            tv_sec: sec,
            tv_usec: nsec / 1000,
        },
        hz,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{IpcStatus, TrapStatus};
    use crate::misc::MIB_CALL_SYSCTL;
    use alloc::vec::Vec;
    use core::mem::{offset_of, size_of};

    /// Scripted three-leg transport for the `times` composition: the
    /// manager legs answer in the reply overlay, the sysctl leg writes
    /// the rate through the caller's sink pointer — hosted, the same
    /// address space the real copy lands in, so the fake replays what
    /// the servers do.
    struct FakeTimesTransport {
        ///_hz the sysctl leg reports.
        hz: i32,
        rusage_self: Option<Result<Rusage, i32>>,
        rusage_children: Option<Result<Rusage, i32>>,
        wall: Option<(i64, i64)>,
        calls: core::cell::RefCell<Vec<(minix_types::Endpoint, i32)>>,
    }

    impl FakeTimesTransport {
        fn new(hz: i32) -> Self {
            Self {
                hz,
                rusage_self: None,
                rusage_children: None,
                wall: None,
                calls: core::cell::RefCell::new(Vec::new()),
            }
        }

        fn reply_type(&self, _call: i32) -> i32 {
            0
        }
    }

    impl IpcTransport for FakeTimesTransport {
        fn sendrec(
            &self,
            destination: minix_types::Endpoint,
            message: &mut minix_types::Message,
        ) -> Result<(), TrapStatus> {
            let call = message.m_type;
            self.calls.borrow_mut().push((destination, call));
            match call {
                x if x == crate::pm::PM_CALL_GETRUSAGE => {
                    // SAFETY: request lanes per mess_lc_pm_rusage.
                    let raw = unsafe { &message.m_u.raw };
                    let who = i32::from_le_bytes(raw[0..4].try_into().unwrap());
                    let sink = u64::from_le_bytes(raw[8..16].try_into().unwrap()) as *mut Rusage;
                    let scripted = if who == RUSAGE_SELF {
                        &self.rusage_self
                    } else {
                        &self.rusage_children
                    };
                    match scripted {
                        Some(Ok(snapshot)) => {
                            // SAFETY: the sink is the caller's own `&mut
                            // Rusage` (hosted same-address-space replay
                            // of the manager's copy-out).
                            unsafe {
                                *sink = *snapshot;
                            }
                        }
                        Some(Err(code)) => {
                            message.m_type = -code;
                            return Ok(());
                        }
                        None => {
                            // No scripted usage: leave zeros.
                        }
                    }
                }
                x if x == MIB_CALL_SYSCTL => {
                    // SAFETY: the sink pointer rides the oldp lane and
                    // points at the caller's 20-byte clockinfo buffer.
                    let raw = unsafe { &message.m_u.raw };
                    let oldp = u64::from_le_bytes(raw[0..8].try_into().unwrap()) as *mut u8;
                    let tick = if self.hz == 0 { 0 } else { 1_000_000 / self.hz };
                    // SAFETY: hosted replay of the service's copy-out.
                    let sink =
                        unsafe { core::slice::from_raw_parts_mut(oldp, size_of::<Clockinfo>()) };
                    sink[0..4].copy_from_slice(&self.hz.to_le_bytes());
                    sink[4..8].copy_from_slice(&tick.to_le_bytes());
                    sink[8..12].copy_from_slice(&tick.to_le_bytes());
                    sink[12..16].copy_from_slice(&self.hz.to_le_bytes());
                    sink[16..20].copy_from_slice(&self.hz.to_le_bytes());
                }
                x if x == crate::pm::PM_CALL_GETTIMEOFDAY => {
                    let (sec, nsec) = self.wall.unwrap_or((0, 0));
                    // SAFETY: reply overlay mess_pm_lc_time (sec@0/nsec@8).
                    let raw = unsafe { &mut message.m_u.raw };
                    raw[0..8].copy_from_slice(&sec.to_ne_bytes());
                    raw[8..16].copy_from_slice(&nsec.to_ne_bytes());
                }
                _ => return Err(TrapStatus(minix_types::EIO)),
            }
            message.m_type = self.reply_type(call);
            Ok(())
        }

        fn send(
            &self,
            _d: minix_types::Endpoint,
            _m: &minix_types::Message,
        ) -> Result<(), TrapStatus> {
            Err(TrapStatus(minix_types::EIO))
        }
        fn receive(
            &self,
            _s: minix_types::Endpoint,
            _m: &mut minix_types::Message,
        ) -> Result<IpcStatus, TrapStatus> {
            Err(TrapStatus(minix_types::EIO))
        }
        fn notify(&self, _d: minix_types::Endpoint) -> Result<(), TrapStatus> {
            Err(TrapStatus(minix_types::EIO))
        }
        fn sendnb(
            &self,
            _d: minix_types::Endpoint,
            _m: &minix_types::Message,
        ) -> Result<(), TrapStatus> {
            Err(TrapStatus(minix_types::EIO))
        }
        fn senda(&self, _t: &[crate::ipc::AsyncSlot]) -> Result<(), TrapStatus> {
            Err(TrapStatus(minix_types::EIO))
        }
        fn query_kerninfo_page(&self) -> Result<u64, TrapStatus> {
            Err(TrapStatus(minix_types::EIO))
        }
    }

    fn self_usage(seconds: i64, microseconds: i64) -> Rusage {
        let mut usage = Rusage::zeroed();
        usage.ru_utime = Timeval {
            tv_sec: seconds,
            tv_usec: microseconds,
        };
        usage
    }

    /// C absolute pins: call numbers (`callnr.h:43/:51`) and the
    /// `who` selectors (`sys/resource.h:56-57`).
    #[test]
    fn test_times_call_numbers_match_c() {
        assert_eq!(PM_CALL_GETRUSAGE, 36); // callnr.h:51
        assert_eq!(crate::pm::PM_CALL_GETTIMEOFDAY, 28); // callnr.h:43
        assert_eq!(RUSAGE_SELF, 0);
        assert_eq!(RUSAGE_CHILDREN, -1);
    }

    /// Layout witnesses: the 128-byte copied window (the manager's
    /// `RUSAGE_LEN`) with the lanes `times` reads, and the four-field
    /// `struct tms`.
    #[test]
    fn test_rusage_and_tms_layout_pins() {
        assert_eq!(size_of::<Rusage>(), 128);
        assert_eq!(offset_of!(Rusage, ru_utime), 0);
        assert_eq!(offset_of!(Rusage, ru_stime), 16);
        assert_eq!(offset_of!(Rusage, ru_maxrss), 32);
        assert_eq!(offset_of!(Rusage, ru_minflt), 64);
        assert_eq!(offset_of!(Rusage, ru_majflt), 72);
        assert_eq!(size_of::<Tms>(), 32);
        assert_eq!(offset_of!(Tms, children_system_time), 24);
    }

    /// C `CONVTCK` (`times.c:51-56`): tick truncation, including the
    /// documented 999999 µs → 99 ticks remainder loss at hz 100.
    #[test]
    fn test_convtck_truncates_at_tick_granularity() {
        let t = |sec: i64, usec: i64| Timeval {
            tv_sec: sec,
            tv_usec: usec,
        };
        assert_eq!(convtck(&t(1, 500_000), 100), 150);
        assert_eq!(convtck(&t(0, 999_999), 100), 99);
        assert_eq!(convtck(&t(0, 10_000), 100), 1);
        assert_eq!(convtck(&t(0, 9_999), 100), 0);
        assert_eq!(
            convtck(&t(1_700_000_000, 123_456), 100),
            170_000_000_000 + 12
        );
    }

    /// The composition over the scripted transport: the rate leg, two
    /// usage legs, and the wall-clock leg land in `tms` and the return
    /// value with C's truncation.
    #[test]
    fn test_times_composes_usage_pair_rate_and_wall_clock() {
        let mut transport = FakeTimesTransport::new(100);
        transport.rusage_self = Some(Ok(self_usage(1, 500_000)));
        transport.rusage_children = Some(Ok(self_usage(2, 250_000)));
        transport.wall = Some((1_700_000_000, 123_456_789));
        let mut buffer = Tms::default();
        let elapsed = times_via(&transport, &mut buffer).expect("times");
        // Self: 1.5 s → 150 ticks; children: 2.25 s → 225 ticks.
        assert_eq!(buffer.user_time, 150);
        assert_eq!(buffer.system_time, 0);
        assert_eq!(buffer.children_user_time, 225);
        assert_eq!(buffer.children_system_time, 0);
        // Wall: 123456789 ns → 123456 µs → 12 ticks.
        assert_eq!(elapsed, 170_000_000_000 + 12);
        // Order is C's: rate, self, children, wall.
        assert_eq!(transport.calls.borrow().len(), 4);
        assert_eq!(transport.calls.borrow()[0].1, MIB_CALL_SYSCTL);
        assert_eq!(transport.calls.borrow()[1].1, PM_CALL_GETRUSAGE);
        assert_eq!(transport.calls.borrow()[2].1, PM_CALL_GETRUSAGE);
        assert_eq!(
            transport.calls.borrow()[3].1,
            crate::pm::PM_CALL_GETTIMEOFDAY
        );
    }

    /// Children usage lands in the `tms_cutime`/`tms_cstime` lanes with
    /// its own conversion (the second `getrusage` carries a different
    /// snapshot than the first).
    #[test]
    fn test_times_children_leg_uses_second_snapshot() {
        let mut transport = FakeTimesTransport::new(100);
        transport.rusage_self = Some(Ok(self_usage(0, 250_000)));
        transport.rusage_children = Some(Ok(self_usage(1, 750_000)));
        transport.wall = Some((0, 0));
        let mut buffer = Tms::default();
        times_via(&transport, &mut buffer).expect("times");
        // Each leg converts its own snapshot: 0.25 s → 25, 1.75 s → 175.
        assert_eq!(buffer.user_time, 25);
        assert_eq!(buffer.children_user_time, 175);
    }

    /// A failed usage leg aborts with its errno (C answers -1 and sets
    /// errno, `times.c:81-82`), and no further call goes out.
    #[test]
    fn test_times_propagates_getrusage_failure() {
        let mut transport = FakeTimesTransport::new(100);
        transport.rusage_self = Some(Err(minix_types::EINVAL));
        let mut buffer = Tms::default();
        assert_eq!(times_via(&transport, &mut buffer), Err(Errno::EINVAL));
        assert_eq!(
            transport.calls.borrow().len(),
            2,
            "rate leg + failing usage leg only"
        );
    }

    /// A non-positive rate is refused: C would divide by zero at
    /// `times.c:56` (unreachable in practice, module header).
    #[test]
    fn test_times_refuses_nonpositive_clock_rate() {
        let transport = FakeTimesTransport::new(0);
        let mut buffer = Tms::default();
        assert_eq!(times_via(&transport, &mut buffer), Err(Errno::EINVAL));
        assert_eq!(
            transport.calls.borrow().len(),
            1,
            "refused after the rate leg"
        );
    }

    /// The usage request carries the selector at byte 0 and the sink
    /// address at byte 8 (`mess_lc_pm_rusage`), with the reply error
    /// mapped to errno.
    #[test]
    fn test_getrusage_request_lanes_and_error() {
        let mut usage = Rusage::zeroed();
        let mut transport = FakeTimesTransport::new(100);
        transport.rusage_children = Some(Err(minix_types::EINVAL));
        assert_eq!(
            getrusage_via(&transport, RUSAGE_CHILDREN, &mut usage),
            Err(Errno::EINVAL)
        );
        let (destination, call) = transport.calls.borrow()[0];
        assert_eq!(destination, pm_endpoint());
        assert_eq!(call, PM_CALL_GETRUSAGE);
    }

    /// The zeroed snapshot mirrors the manager's memset-then-fill shape:
    /// never-written fields read as zero by contract.
    #[test]
    fn test_rusage_zeroed_matches_memset_contract() {
        let usage = Rusage::zeroed();
        assert_eq!(usage.ru_utime.tv_sec, 0);
        assert_eq!(usage.ru_maxrss, 0);
        assert_eq!(usage.ru_nsignals, 0);
    }
}
