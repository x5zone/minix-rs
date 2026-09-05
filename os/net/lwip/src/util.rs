//! Small tools: error mapping, root check, time conversion.
//!
//! C correspondence: the error map (`util_convert_err`,
//! `util.c:140-165`, translating stack errors to negative errno
//! values), the root check (`util_is_root`, `util.c:128-133`,
//! comparing the caller user identifier against the root
//! identifier), and the time conversion (`util_timeval_to_ticks`
//! and `util_ticks_to_timeval`, `util.c:17-46`, with one million
//! microseconds per second, `US`, `util.c:5`).
//!
//! System calls stay in the service binary; this module owns the
//! pure mapping half: which error becomes which number and how time
//! units convert.

/// Microseconds in one second (`US`).
pub const USEC_PER_SEC: u64 = 1_000_000;

/// Stack error: everything is fine.
pub const STACK_OK: i32 = 0;
/// Stack error: out of memory.
pub const STACK_NO_MEMORY: i32 = -1;
/// Stack error: buffer error.
pub const STACK_NO_BUFFERS: i32 = -2;
/// Stack error: would block.
pub const STACK_WOULD_BLOCK: i32 = -3;
/// Stack error: connection aborted.
pub const STACK_ABORTED: i32 = -4;
/// Stack error: connection reset.
pub const STACK_RESET: i32 = -5;

/// System error numbers used by the mapping (Minix3 negative errno).
pub const ERR_NO_MEMORY: i32 = -12;
/// No buffer space left.
pub const ERR_NO_BUFFERS: i32 = -105;
/// Operation would block.
pub const ERR_WOULD_BLOCK: i32 = -11;
/// Software caused connection abort.
pub const ERR_CONN_ABORTED: i32 = -103;
/// Connection reset by peer.
pub const ERR_CONN_RESET: i32 = -104;
/// Invalid argument.
pub const ERR_INVALID: i32 = -22;

/// Map a stack error to a system error number (`util_convert_err`,
/// `util.c:140-165`); unknown errors become a generic failure.
pub fn convert_error(stack_error: i32) -> i32 {
    match stack_error {
        STACK_OK => 0,
        STACK_NO_MEMORY => ERR_NO_MEMORY,
        STACK_NO_BUFFERS => ERR_NO_BUFFERS,
        STACK_WOULD_BLOCK => ERR_WOULD_BLOCK,
        STACK_ABORTED => ERR_CONN_ABORTED,
        STACK_RESET => ERR_CONN_RESET,
        _ => ERR_INVALID,
    }
}

/// Convert seconds plus microseconds to ticks, rounding up
/// (`util_timeval_to_ticks`, `util.c:18-32`).
pub fn timeval_to_ticks(seconds: u64, microseconds: u64, ticks_per_second: u64) -> u64 {
    seconds * ticks_per_second + (microseconds * ticks_per_second + USEC_PER_SEC - 1) / USEC_PER_SEC
}

/// Convert ticks back to seconds plus microseconds
/// (`util_ticks_to_timeval`, `util.c:40-45`).
pub fn ticks_to_timeval(ticks: u64, ticks_per_second: u64) -> (u64, u64) {
    (ticks / ticks_per_second, (ticks % ticks_per_second) * USEC_PER_SEC / ticks_per_second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_map_matches_converter() {
        assert_eq!(convert_error(STACK_OK), 0);
        assert_eq!(convert_error(STACK_NO_MEMORY), ERR_NO_MEMORY);
        assert_eq!(convert_error(STACK_NO_BUFFERS), ERR_NO_BUFFERS);
        assert_eq!(convert_error(STACK_WOULD_BLOCK), ERR_WOULD_BLOCK);
        assert_eq!(convert_error(STACK_ABORTED), ERR_CONN_ABORTED);
        assert_eq!(convert_error(STACK_RESET), ERR_CONN_RESET);
        assert_eq!(convert_error(-99), ERR_INVALID);
    }

    #[test]
    fn test_time_round_trips() {
        assert_eq!(USEC_PER_SEC, 1_000_000);
        assert_eq!(timeval_to_ticks(1, 500_000, 100), 150);
        assert_eq!(timeval_to_ticks(0, 1, 100), 1);
        assert_eq!(ticks_to_timeval(150, 100), (1, 500_000));
        assert_eq!(ticks_to_timeval(0, 100), (0, 0));
    }
}
