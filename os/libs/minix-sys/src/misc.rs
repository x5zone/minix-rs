//! Miscellaneous calls: sleeping, server control, and direct clock reads.
//!
//! Not every user-space need fits a service group. This module gathers the
//! leftovers that the stage plan assigns to the miscellaneous document:
//!
//! 1. Sleeping without a sleep call: `nanosleep` reuses the descriptor-wait
//!    call with an empty set and a timeout (C:
//!    `minix3/minix/lib/libc/sys/nanosleep.c`).
//! 2. Server control: one function splits on the request's group character
//!    and forwards to the process manager or the file system (C:
//!    `minix3/minix/lib/libc/sys/svrctl.c`).
//! 3. Clock reads without any round trip: tick count, uptime triple, and
//!    wall-clock time come straight from the kernel information page (C:
//!    `minix3/minix/lib/libsys/getticks.c`, `getuptime.c`, `clock_time.c`,
//!    all consumers of the 01-stage page).
//! 4. Cycle counter readout: two 32-bit halves combined into one 64-bit
//!    stamp (C: `minix3/minix/lib/libc/gen/read_tsc_64.c`).
//!
//! The management-information query (`__sysctl`) travels to its own server;
//! only the client-side validation lives here, since the server belongs to
//! another stage.
//!
//! Every item is pure logic over caller-supplied values, so unit tests cover
//! each rule without sleeping, without servers, and without hardware.
//!
//! # Execution model
//!
//! No shared state: all functions take explicit inputs and return explicit
//! outputs. Time values are snapshots; callers re-read for fresh data.

use minix_types::Errno;

/// Management information server endpoint.
///
/// C: `MIB_PROC_NR ((endpoint_t) 7)` (`minix3/minix/include/minix/com.h:66`).
pub const MIB_ENDPOINT_NUMBER: i32 = 7;

/// System control query. C: `MIB_SYSCTL (MIB_BASE + 0)` (`com.h:1026`).
pub const MIB_CALL_SYSCTL: i32 = 0x600;

/// Longest sysctl name carried inside the message.
///
/// C: `CTL_SHORTNAME 8` (`minix3/minix/include/minix/ipc.h:15`): names this
/// long or shorter travel inline; longer names travel by pointer only.
pub const SYSCTL_SHORT_NAME_LENGTH: usize = 8;

/// Nanoseconds per microsecond (also microseconds per millisecond, and
/// milliseconds per second — the three identical conversion steps in
/// `nanosleep.c:15-17`).
pub const NANOSECONDS_PER_MICROSECOND: i64 = 1000;
/// Nanoseconds per second (the range bound for the sub-second field, see
/// `nanosleep.c:19-20`).
pub const NANOSECONDS_PER_SECOND: i64 = 1_000_000_000;

/// A sleep request: whole seconds plus a sub-second fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SleepRequest {
    /// Whole seconds (must not be negative).
    pub seconds: i64,
    /// Sub-second nanoseconds in `0..NANOSECONDS_PER_SECOND`.
    pub nanoseconds: i64,
}

/// A validated timeout ready for the descriptor-wait call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SleepTimeout {
    /// Whole seconds.
    pub seconds: i64,
    /// Microseconds, rounded up from the requested nanoseconds.
    pub microseconds: i64,
}

/// Validates a sleep request and converts it to a wait timeout.
///
/// This mirrors the parameter checks and conversion in `nanosleep`
/// (`minix3/minix/lib/libc/sys/nanosleep.c:28-56`): a missing request is a
/// bad address, a negative or out-of-range fraction is an invalid argument,
/// and the microsecond count rounds up (`(nsec + 999) / 1000`) so a
/// nonzero request never becomes a zero wait.
pub const fn validate_sleep_request(request: Option<SleepRequest>) -> Result<SleepTimeout, Errno> {
    let Some(request) = request else {
        return Err(Errno::from_i32(minix_types::EFAULT));
    };
    if request.seconds < 0
        || request.nanoseconds < 0
        || request.nanoseconds >= NANOSECONDS_PER_SECOND
    {
        return Err(Errno::from_i32(minix_types::EINVAL));
    }
    Ok(SleepTimeout {
        seconds: request.seconds,
        microseconds: (request.nanoseconds + NANOSECONDS_PER_MICROSECOND - 1)
            / NANOSECONDS_PER_MICROSECOND,
    })
}

/// Remaining sleep after an interruption, in canonical form.
///
/// This mirrors the remainder computation in `nanosleep.c:70-92`: subtract
/// the elapsed wall-clock time from the request, fold a negative fraction
/// back into range by borrowing whole seconds, and clamp a negative result
/// to zero (an interruption that outlasted the request leaves nothing).
pub const fn remaining_sleep(
    requested_seconds: i64,
    requested_nanoseconds: i64,
    elapsed_seconds: i64,
    elapsed_nanoseconds: i64,
) -> (i64, i64) {
    let mut seconds = requested_seconds - elapsed_seconds;
    let mut nanoseconds = requested_nanoseconds - elapsed_nanoseconds;
    while nanoseconds < 0 {
        seconds -= 1;
        nanoseconds += NANOSECONDS_PER_SECOND;
    }
    while nanoseconds > NANOSECONDS_PER_SECOND {
        seconds += 1;
        nanoseconds -= NANOSECONDS_PER_SECOND;
    }
    if seconds < 0 {
        seconds = 0;
        nanoseconds = 0;
    }
    (seconds, nanoseconds)
}

/// Server-control target selected from the request's group character.
///
/// C: `svrctl` (`minix3/minix/lib/libc/sys/svrctl.c`) reads the group
/// character embedded in the request number: the two process-manager groups
/// go to the process manager, the file-system group goes to the file system,
/// and anything else is an invalid argument. The group sits eight bits up
/// (C: `IOCGROUP(x) (((x) >> 8) & 0xff)` in `minix3/sys/sys/ioccom.h:68`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerControlTarget {
    /// Forward to the process manager.
    ProcessManager,
    /// Forward to the file system.
    FileSystem,
}

/// Selects the server-control target from a request number.
pub const fn dispatch_server_control(request: u64) -> Result<ServerControlTarget, Errno> {
    match ((request >> 8) & 0xff) as u8 as char {
        'M' | 'P' => Ok(ServerControlTarget::ProcessManager),
        'F' => Ok(ServerControlTarget::FileSystem),
        _ => Err(Errno::from_i32(minix_types::EINVAL)),
    }
}

/// Snapshot of the kernel clock page fields a reader needs.
///
/// C: `struct kclockinfo` (`minix3/minix/include/minix/type.h`): boot time in
/// epoch seconds, tick counts since boot, and the tick frequency. Readers
/// take this snapshot instead of the raw page, which keeps the 01-stage page
/// handling in one place and the arithmetic here testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSnapshot {
    /// Ticks since boot (C: `uptime`).
    pub uptime_ticks: u64,
    /// Corrected ticks since boot (C: `realtime`).
    pub realtime_ticks: u64,
    /// Boot time in epoch seconds (C: `boottime`).
    pub boottime_seconds: u64,
    /// Ticks per second (C: `hz`).
    pub ticks_per_second: u64,
}

/// Reads the tick count since boot.
///
/// C: `getticks` (`minix3/minix/lib/libsys/getticks.c:7-13`): return the
/// uptime field. The value may wrap on overflow; callers that need
/// differences must subtract with wrapping arithmetic.
pub const fn read_tick_count(snapshot: ClockSnapshot) -> u64 {
    snapshot.uptime_ticks
}

/// Reads uptime, corrected time, and boot time together.
///
/// C: `getuptime` (`getuptime.c`): copy whichever fields the caller asked
/// for (null pointers mean "skip this one"). Without null pointers in safe
/// Rust, each field travels in an `Option`: `None` skips, `Some` receives.
pub fn read_uptime_triple(
    snapshot: ClockSnapshot,
    uptime: Option<&mut u64>,
    realtime: Option<&mut u64>,
    boottime: Option<&mut u64>,
) {
    if let Some(slot) = uptime {
        *slot = snapshot.uptime_ticks;
    }
    if let Some(slot) = realtime {
        *slot = snapshot.realtime_ticks;
    }
    if let Some(slot) = boottime {
        *slot = snapshot.boottime_seconds;
    }
}

/// Converts a clock snapshot to wall-clock seconds plus a fraction.
///
/// C: `clock_time` (`minix3/minix/lib/libsys/clock_time.c`): whole seconds
/// are boot time plus corrected ticks divided by frequency; the fraction
/// multiplies the remainder up to nanoseconds in two steps (`* 40000 / hz *
/// 25000`, since 40000 times 25000 is one billion) to avoid overflowing on
/// high-frequency clocks.
///
/// Two deliberate hardenings over C: a zero frequency returns the boot time
/// with a zero fraction instead of dividing by zero (an uninitialized clock
/// page must not crash its reader), and the two-step multiply uses checked
/// arithmetic with a zero fallback, mirroring the C "bad, but what's better"
/// branch.
pub const fn wall_clock_time(snapshot: ClockSnapshot) -> (u64, u64) {
    if snapshot.ticks_per_second == 0 {
        return (snapshot.boottime_seconds, 0);
    }
    let seconds = snapshot.boottime_seconds + snapshot.realtime_ticks / snapshot.ticks_per_second;
    let remainder = snapshot.realtime_ticks % snapshot.ticks_per_second;
    let mut fraction = 0;
    if let Some(scaled) = remainder.checked_mul(40_000) {
        if let Some(per_unit) = scaled.checked_div(snapshot.ticks_per_second) {
            if let Some(nanoseconds) = per_unit.checked_mul(25_000) {
                fraction = nanoseconds;
            }
        }
    }
    (seconds, fraction)
}

/// Combines a split cycle-counter stamp into one 64-bit value.
///
/// C: `read_tsc_64` (`minix3/minix/lib/libc/gen/read_tsc_64.c`): read the low
/// and high halves from hardware, then join them low-first. The hardware
/// read itself stays behind the architecture layer; only the joining rule
/// lives here, where tests can check it.
pub const fn combine_timestamp(high_half: u32, low_half: u32) -> u64 {
    ((high_half as u64) << 32) | low_half as u64
}

/// Validates a sysctl name length for the inline-or-pointer rule.
///
/// Names this long or shorter travel inside the message; longer names travel
/// by pointer only (see `__sysctl.c`: `namelen <= CTL_SHORTNAME` copies
/// inline). The boolean reports "fits inline".
pub const fn sysctl_name_fits_inline(name_length: usize) -> bool {
    name_length <= SYSCTL_SHORT_NAME_LENGTH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_match_c_headers() {
        assert_eq!(MIB_ENDPOINT_NUMBER, 7);
        assert_eq!(MIB_CALL_SYSCTL, 0x600);
        assert_eq!(SYSCTL_SHORT_NAME_LENGTH, 8);
        assert_eq!(NANOSECONDS_PER_SECOND, 1_000_000_000);
    }

    #[test]
    fn test_missing_request_is_bad_address() {
        assert_eq!(
            validate_sleep_request(None),
            Err(Errno::from_i32(minix_types::EFAULT))
        );
    }

    #[test]
    fn test_out_of_range_fraction_is_invalid() {
        assert_eq!(
            validate_sleep_request(Some(SleepRequest { seconds: 1, nanoseconds: -1 })),
            Err(Errno::from_i32(minix_types::EINVAL))
        );
        assert_eq!(
            validate_sleep_request(Some(SleepRequest {
                seconds: 1,
                nanoseconds: NANOSECONDS_PER_SECOND,
            })),
            Err(Errno::from_i32(minix_types::EINVAL))
        );
        assert_eq!(
            validate_sleep_request(Some(SleepRequest { seconds: -1, nanoseconds: 0 })),
            Err(Errno::from_i32(minix_types::EINVAL))
        );
    }

    #[test]
    fn test_microseconds_round_up() {
        // One nanosecond still waits one microsecond; an exact count stays.
        assert_eq!(
            validate_sleep_request(Some(SleepRequest { seconds: 2, nanoseconds: 1 })),
            Ok(SleepTimeout { seconds: 2, microseconds: 1 })
        );
        assert_eq!(
            validate_sleep_request(Some(SleepRequest { seconds: 0, nanoseconds: 1000 })),
            Ok(SleepTimeout { seconds: 0, microseconds: 1 })
        );
        assert_eq!(
            validate_sleep_request(Some(SleepRequest { seconds: 0, nanoseconds: 0 })),
            Ok(SleepTimeout { seconds: 0, microseconds: 0 })
        );
    }

    #[test]
    fn test_remaining_time_canonicalizes_and_clamps() {
        // Borrow one second to fix a negative fraction.
        assert_eq!(remaining_sleep(5, 100, 2, 300), (2, 999_999_800));
        // An overrun clamps to zero instead of going negative.
        assert_eq!(remaining_sleep(1, 0, 5, 0), (0, 0));
    }

    #[test]
    fn test_server_control_dispatch_follows_group() {
        assert_eq!(
            dispatch_server_control((b'P' as u64) << 8),
            Ok(ServerControlTarget::ProcessManager)
        );
        assert_eq!(
            dispatch_server_control((b'M' as u64) << 8),
            Ok(ServerControlTarget::ProcessManager)
        );
        assert_eq!(
            dispatch_server_control((b'F' as u64) << 8),
            Ok(ServerControlTarget::FileSystem)
        );
        assert_eq!(
            dispatch_server_control((b'X' as u64) << 8),
            Err(Errno::from_i32(minix_types::EINVAL))
        );
    }

    #[test]
    fn test_tick_count_reads_uptime_field() {
        let snapshot = ClockSnapshot {
            uptime_ticks: 12345,
            realtime_ticks: 0,
            boottime_seconds: 0,
            ticks_per_second: 100,
        };
        assert_eq!(read_tick_count(snapshot), 12345);
    }

    #[test]
    fn test_uptime_triple_skips_unwanted_fields() {
        let snapshot = ClockSnapshot {
            uptime_ticks: 1,
            realtime_ticks: 2,
            boottime_seconds: 3,
            ticks_per_second: 100,
        };
        let mut uptime = 0;
        let mut boottime = 0;
        read_uptime_triple(snapshot, Some(&mut uptime), None, Some(&mut boottime));
        assert_eq!((uptime, boottime), (1, 3));
    }

    #[test]
    fn test_wall_clock_arithmetic_matches_c_formula() {
        // boottime 1000 + 250/100 = 1002 seconds; fraction (50*40000/100*25000).
        let snapshot = ClockSnapshot {
            uptime_ticks: 0,
            realtime_ticks: 250,
            boottime_seconds: 1000,
            ticks_per_second: 100,
        };
        assert_eq!(wall_clock_time(snapshot), (1002, 500_000_000));
    }

    #[test]
    fn test_zero_frequency_returns_boot_time() {
        let snapshot = ClockSnapshot {
            uptime_ticks: 0,
            realtime_ticks: 250,
            boottime_seconds: 1000,
            ticks_per_second: 0,
        };
        assert_eq!(wall_clock_time(snapshot), (1000, 0));
    }

    #[test]
    fn test_timestamp_joins_low_first() {
        assert_eq!(combine_timestamp(0x0000_0001, 0x0000_0002), 0x1_0000_0002);
        assert_eq!(combine_timestamp(0, 0), 0);
    }

    #[test]
    fn test_sysctl_inline_boundary() {
        assert!(sysctl_name_fits_inline(8));
        assert!(!sysctl_name_fits_inline(9));
    }
}
