//! Shared stateless helpers: error translation, privilege check, time conversion.
//!
//! C correspondence: `minix3/minix/net/lwip/util.c` (251 lines). This module
//! owns the pure computation half that every socket layer reuses. Operations
//! that touch user memory or the management tree stay in the service binary:
//! `util_copy_data` (`util.c:61-100`, scatter copy through the socket driver),
//! `util_pcblist` (`util.c:173-251`, protocol control block listing for the
//! management tree). What stays here is what can be decided from numbers
//! alone: which stack error becomes which system error, whether a caller is
//! the superuser, how seconds and microseconds convert to clock ticks.
//!
//! Wire convention: the service runs in the system task context, where Minix
//! error codes are negative (`_SIGN` is `-` under `_SYSTEM`,
//! `minix3/sys/sys/errno.h:189`). The shared `minix-types` crate exposes the
//! same numbers as positive user-space constants. This module returns the
//! negative wire form so the service can forward it directly, and documents
//! the matching positive constant next to every value.
//!
//! Single-threaded event loop: all functions are pure and reentrant, no
//! shared mutable state.

// C library correspondence for the stack error numbers:
// `minix3/minix/lib/liblwip/dist/src/include/lwip/err.h:63-96`.
/// Stack success (`ERR_OK`).
pub const STACK_OK: i32 = 0;
/// Stack out of memory (`ERR_MEM`).
pub const STACK_NO_MEMORY: i32 = -1;
/// Stack buffer shortage (`ERR_BUF`).
pub const STACK_NO_BUFFERS: i32 = -2;
/// Stack timeout (`ERR_TIMEOUT`).
pub const STACK_TIMEOUT: i32 = -3;
/// Stack routing failure (`ERR_RTE`).
pub const STACK_NO_ROUTE: i32 = -4;
/// Stack operation in progress (`ERR_INPROGRESS`).
pub const STACK_IN_PROGRESS: i32 = -5;
/// Stack illegal value (`ERR_VAL`).
pub const STACK_BAD_VALUE: i32 = -6;
/// Stack would block (`ERR_WOULDBLOCK`).
pub const STACK_WOULD_BLOCK: i32 = -7;
/// Stack address in use (`ERR_USE`).
pub const STACK_IN_USE: i32 = -8;
/// Stack already in progress (`ERR_ALREADY`).
pub const STACK_ALREADY: i32 = -9;
/// Stack already connected (`ERR_ISCONN`).
pub const STACK_IS_CONNECTED: i32 = -10;
/// Stack not connected (`ERR_CONN`).
pub const STACK_NOT_CONNECTED: i32 = -11;
/// Stack interface error (`ERR_IF`).
pub const STACK_INTERFACE: i32 = -12;
/// Stack connection aborted (`ERR_ABRT`).
pub const STACK_ABORTED: i32 = -13;
/// Stack connection reset (`ERR_RST`).
pub const STACK_RESET: i32 = -14;
/// Stack closed (`ERR_CLSD`).
pub const STACK_CLOSED: i32 = -15;
/// Stack bad argument (`ERR_ARG`).
pub const STACK_BAD_ARGUMENT: i32 = -16;

// Wire-form system errors (negative `_SYSTEM` form). Positive user-space twin
// in parentheses, matching `os/libs/minix-types/src/types/errno.rs`.
/// Success (`OK`, 0).
pub const ERR_OK: i32 = 0;
/// Cannot allocate memory (`ENOMEM`, 12).
pub const ERR_NO_MEMORY: i32 = -12;
/// No buffer space available (`ENOBUFS`, 55).
pub const ERR_NO_BUFFERS: i32 = -55;
/// Operation timed out (`ETIMEDOUT`, 60).
pub const ERR_TIMED_OUT: i32 = -60;
/// No route to host (`EHOSTUNREACH`, 65).
pub const ERR_HOST_UNREACHABLE: i32 = -65;
/// Invalid argument (`EINVAL`, 22).
pub const ERR_INVALID: i32 = -22;
/// Address already in use (`EADDRINUSE`, 48).
pub const ERR_ADDRESS_IN_USE: i32 = -48;
/// Operation already in progress (`EALREADY`, 37).
pub const ERR_ALREADY: i32 = -37;
/// Socket already connected (`EISCONN`, 56).
pub const ERR_IS_CONNECTED: i32 = -56;
/// Socket not connected (`ENOTCONN`, 57).
pub const ERR_NOT_CONNECTED: i32 = -57;
/// Network is down (`ENETDOWN`, 50).
pub const ERR_NETWORK_DOWN: i32 = -50;
/// Software caused connection abort (`ECONNABORTED`, 53).
pub const ERR_CONNECTION_ABORTED: i32 = -53;
/// Connection reset by peer (`ECONNRESET`, 54).
pub const ERR_CONNECTION_RESET: i32 = -54;
/// Operation now in progress (`EINPROGRESS`, 36).
pub const ERR_IN_PROGRESS: i32 = -36;
/// Operation would block (`EWOULDBLOCK` equals `EAGAIN`, 35).
pub const ERR_WOULD_BLOCK: i32 = -35;
/// Generic error (`EGENERIC`, 204).
pub const ERR_GENERIC: i32 = -204;
/// Numerical argument out of domain (`EDOM`, 33).
pub const ERR_DOMAIN: i32 = -33;
/// Argument list too long (`E2BIG`, 7).
pub const ERR_TOO_BIG: i32 = -7;

/// Microseconds in one second (`US`, `util.c:5`).
pub const USEC_PER_SEC: u64 = 1_000_000;

/// Root user identifier (`ROOT_EUID`, 0).
pub const ROOT_UID: u32 = 0;

/// Translate a lightweight IP stack error into a Minix system error
/// (`util_convert_err`, `util.c:140-165`).
///
/// The table has 16 explicit rows. Two rows carry a warning in the original
/// source: `ERR_INPROGRESS` and `ERR_WOULDBLOCK` are marked "should not be
/// thrown" (`util.c:157-158`), meaning the upper layers never expect them,
/// but the translation still exists so no stack value falls through silently.
/// `ERR_CLSD` and any unknown value fall to the default branch, which logs
/// the unexpected value in C and returns the generic error. This function
/// returns the generic error for that case; logging stays in the service
/// binary because this library has no console access.
pub fn convert_error(stack_error: i32) -> i32 {
    match stack_error {
        STACK_OK => ERR_OK,
        STACK_NO_MEMORY => ERR_NO_MEMORY,
        STACK_NO_BUFFERS => ERR_NO_BUFFERS,
        STACK_TIMEOUT => ERR_TIMED_OUT,
        STACK_NO_ROUTE => ERR_HOST_UNREACHABLE,
        STACK_BAD_VALUE => ERR_INVALID,
        STACK_IN_USE => ERR_ADDRESS_IN_USE,
        STACK_ALREADY => ERR_ALREADY,
        STACK_IS_CONNECTED => ERR_IS_CONNECTED,
        STACK_NOT_CONNECTED => ERR_NOT_CONNECTED,
        STACK_INTERFACE => ERR_NETWORK_DOWN,
        STACK_ABORTED => ERR_CONNECTION_ABORTED,
        STACK_RESET => ERR_CONNECTION_RESET,
        STACK_IN_PROGRESS => ERR_IN_PROGRESS,
        STACK_WOULD_BLOCK => ERR_WOULD_BLOCK,
        STACK_BAD_ARGUMENT => ERR_INVALID,
        // Covers STACK_CLOSED and any future stack value.
        _ => ERR_GENERIC,
    }
}

/// Whether a caller runs with superuser privileges
/// (`util_is_root`, `util.c:129-133`).
///
/// The original compares the caller endpoint numeric user identifier against
/// the root identifier (`getnuid(endpt) == ROOT_EUID`). The endpoint lookup
/// itself stays in the service binary; this helper owns the pure comparison
/// so it can be tested without a process table.
pub fn is_root(caller_uid: u32) -> bool {
    caller_uid == ROOT_UID
}

/// Convert seconds plus microseconds into clock ticks, rounding up
/// (`util_timeval_to_ticks`, `util.c:18-33`).
///
/// Parameters mirror the C inputs without globals: `ticks_per_second` plays
/// the role of `sys_hz()`, `max_ticks` plays the role of `TMRDIFF_MAX`
/// (`minix3/minix/include/minix/timers.h:45`, the largest `int` value).
/// Validation order matches the original: microsecond range first
/// (`util.c:22` returns `EINVAL`), then overflow guard (`util.c:25-26`
/// returns `EDOM` when `seconds >= max_ticks / ticks_per_second`). The
/// rounding adds `USEC_PER_SEC - 1` before dividing (`util.c:28`), so even
/// one microsecond consumes a full tick.
pub fn timeval_to_ticks(
    seconds: i64,
    microseconds: i64,
    ticks_per_second: u64,
    max_ticks: u64,
) -> Result<u64, i32> {
    if seconds < 0 || microseconds < 0 || microseconds as u64 >= USEC_PER_SEC {
        return Err(ERR_INVALID);
    }
    if ticks_per_second == 0 {
        return Err(ERR_INVALID);
    }
    if (seconds as u64) >= max_ticks / ticks_per_second {
        return Err(ERR_DOMAIN);
    }
    let ticks = seconds as u64 * ticks_per_second
        + (microseconds as u64 * ticks_per_second).div_ceil(USEC_PER_SEC);
    Ok(ticks)
}

/// Convert clock ticks back into seconds plus microseconds
/// (`util_ticks_to_timeval`, `util.c:40-46`).
///
/// This direction never fails in the original: it clears the output
/// (`util.c:43`), divides for seconds (`util.c:44`), and takes the remainder
/// for microseconds (`util.c:45`). Division by zero cannot happen in the
/// service because `sys_hz()` is always positive; the Rust form documents the
/// same precondition and returns zero for a zero rate instead of trapping.
pub fn ticks_to_timeval(ticks: u64, ticks_per_second: u64) -> (u64, u64) {
    if ticks_per_second == 0 {
        return (0, 0);
    }
    (
        ticks / ticks_per_second,
        (ticks % ticks_per_second) * USEC_PER_SEC / ticks_per_second,
    )
}

/// Check whether scattered input lengths fit a bounded output buffer
/// (`util_coalesce`, `util.c:108-123`).
///
/// The original walks an input-output vector and returns `E2BIG` as soon as
/// one chunk does not fit the remaining space (`util.c:113-114`). This helper
/// takes the chunk lengths explicitly so the service binary keeps ownership
/// of the actual pointers. On success it returns the total copied length.
pub fn coalesce_total(chunk_lengths: &[usize], capacity: usize) -> Result<usize, i32> {
    let mut used = 0_usize;
    let mut remaining = capacity;
    for &length in chunk_lengths {
        if length > remaining {
            return Err(ERR_TOO_BIG);
        }
        used += length;
        remaining -= length;
    }
    Ok(used)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_table_covers_all_named_stack_values() {
        assert_eq!(convert_error(STACK_OK), ERR_OK);
        assert_eq!(convert_error(STACK_NO_MEMORY), ERR_NO_MEMORY);
        assert_eq!(convert_error(STACK_NO_BUFFERS), ERR_NO_BUFFERS);
        assert_eq!(convert_error(STACK_TIMEOUT), ERR_TIMED_OUT);
        assert_eq!(convert_error(STACK_NO_ROUTE), ERR_HOST_UNREACHABLE);
        assert_eq!(convert_error(STACK_BAD_VALUE), ERR_INVALID);
        assert_eq!(convert_error(STACK_IN_USE), ERR_ADDRESS_IN_USE);
        assert_eq!(convert_error(STACK_ALREADY), ERR_ALREADY);
        assert_eq!(convert_error(STACK_IS_CONNECTED), ERR_IS_CONNECTED);
        assert_eq!(convert_error(STACK_NOT_CONNECTED), ERR_NOT_CONNECTED);
        assert_eq!(convert_error(STACK_INTERFACE), ERR_NETWORK_DOWN);
        assert_eq!(convert_error(STACK_ABORTED), ERR_CONNECTION_ABORTED);
        assert_eq!(convert_error(STACK_RESET), ERR_CONNECTION_RESET);
        assert_eq!(convert_error(STACK_IN_PROGRESS), ERR_IN_PROGRESS);
        assert_eq!(convert_error(STACK_WOULD_BLOCK), ERR_WOULD_BLOCK);
        assert_eq!(convert_error(STACK_BAD_ARGUMENT), ERR_INVALID);
    }

    #[test]
    fn test_error_table_wire_values_match_minix_errno() {
        assert_eq!(ERR_NO_MEMORY, -12);
        assert_eq!(ERR_NO_BUFFERS, -55);
        assert_eq!(ERR_TIMED_OUT, -60);
        assert_eq!(ERR_HOST_UNREACHABLE, -65);
        assert_eq!(ERR_INVALID, -22);
        assert_eq!(ERR_ADDRESS_IN_USE, -48);
        assert_eq!(ERR_ALREADY, -37);
        assert_eq!(ERR_IS_CONNECTED, -56);
        assert_eq!(ERR_NOT_CONNECTED, -57);
        assert_eq!(ERR_NETWORK_DOWN, -50);
        assert_eq!(ERR_CONNECTION_ABORTED, -53);
        assert_eq!(ERR_CONNECTION_RESET, -54);
        assert_eq!(ERR_IN_PROGRESS, -36);
        assert_eq!(ERR_WOULD_BLOCK, -35);
        assert_eq!(ERR_GENERIC, -204);
    }

    #[test]
    fn test_closed_and_unknown_stack_values_become_generic() {
        assert_eq!(convert_error(STACK_CLOSED), ERR_GENERIC);
        assert_eq!(convert_error(-99), ERR_GENERIC);
        assert_eq!(convert_error(7), ERR_GENERIC);
    }

    #[test]
    fn test_root_check_compares_against_zero() {
        assert!(is_root(0));
        assert!(!is_root(1));
        assert!(!is_root(1000));
    }

    #[test]
    fn test_time_conversion_rounds_up_and_back() {
        assert_eq!(USEC_PER_SEC, 1_000_000);
        assert_eq!(timeval_to_ticks(1, 500_000, 100, 2_147_483_647), Ok(150));
        assert_eq!(timeval_to_ticks(0, 1, 100, 2_147_483_647), Ok(1));
        assert_eq!(timeval_to_ticks(0, 0, 100, 2_147_483_647), Ok(0));
        assert_eq!(ticks_to_timeval(150, 100), (1, 500_000));
        assert_eq!(ticks_to_timeval(0, 100), (0, 0));
    }

    #[test]
    fn test_time_conversion_rejects_bad_fields() {
        assert_eq!(
            timeval_to_ticks(-1, 0, 100, 2_147_483_647),
            Err(ERR_INVALID)
        );
        assert_eq!(
            timeval_to_ticks(0, -1, 100, 2_147_483_647),
            Err(ERR_INVALID)
        );
        assert_eq!(
            timeval_to_ticks(0, 1_000_000, 100, 2_147_483_647),
            Err(ERR_INVALID)
        );
        assert_eq!(timeval_to_ticks(0, 0, 0, 2_147_483_647), Err(ERR_INVALID));
    }

    #[test]
    fn test_time_conversion_rejects_overflow() {
        // 21474836 seconds at 100 ticks per second would exceed the 32-bit
        // tick ceiling, so the guard returns the domain error.
        assert_eq!(
            timeval_to_ticks(21_474_837, 0, 100, 2_147_483_647),
            Err(ERR_DOMAIN)
        );
    }

    #[test]
    fn test_coalesce_totals_or_reports_too_big() {
        assert_eq!(coalesce_total(&[10, 20, 30], 100), Ok(60));
        assert_eq!(coalesce_total(&[], 100), Ok(0));
        assert_eq!(coalesce_total(&[60, 50], 100), Err(ERR_TOO_BIG));
    }
}
