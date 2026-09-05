//! Random device: the single minor, chunked transfers, always-ready poll.
//!
//! C correspondence: the single minor `RANDOM_DEV 0` and `NR_DEVS 1`
//! (`main.c:13-14`), the chunked read and write in `r_read` and `r_write`
//! (`main.c:120-172`), the open check in `r_open` (`main.c:177-185`), the
//! alarm cadence `KRANDOM_PERIOD 1` with the seeded factor of five hundred
//! (`main.c:16,239`), and the always-ready `r_select`
//! (`main.c:259-268`).
//!
//! Only one minor exists: there is no separate unblocking device in this
//! driver (callers needing non-blocking behavior poll first). Reads before
//! the first reseed answer "try again".

use minix_types::{EAGAIN, EINVAL, EIO, ENXIO, OK};

/// The only minor device: `/dev/random`.
///
/// C: `RANDOM_DEV 0` (`main.c:14`).
pub const RANDOM_MINOR: u32 = 0;

/// Transfer chunk in bytes: blocks are generated aside and copied out per
/// chunk so the service crate needs only a small staging buffer.
///
/// C: `RANDOM_BUF_SIZE 1024` (`main.c:46`).
pub const TRANSFER_CHUNK: usize = 1024;

/// Ticks between kernel-sample harvests before seeding.
///
/// C: `KRANDOM_PERIOD 1` (`main.c:16`).
pub const HARVEST_PERIOD_UNSEEDED: u32 = 1;

/// Harvests slow down once seeded (entropy maintenance, not bootstrap).
///
/// C: `KRANDOM_PERIOD*500` when seeded (`main.c:239`).
pub const HARVEST_SLOWDOWN: u32 = 500;

/// Current harvest period in ticks for this seed state.
///
/// C: `nextperiod` in `r_random` (`main.c:239`).
pub const fn harvest_period(seeded: bool) -> u32 {
    if seeded {
        HARVEST_PERIOD_UNSEEDED * HARVEST_SLOWDOWN
    } else {
        HARVEST_PERIOD_UNSEEDED
    }
}

/// Split a transfer length into chunk sizes for the staging buffer.
///
/// C: `chunk = MIN(size - offset, RANDOM_BUF_SIZE)` in both `r_read` and
/// `r_write` (`main.c:133,160`). Returns the chunk count and the last
/// (possibly short) chunk; empty for a zero length.
pub fn chunk_plan(total: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let full = total / TRANSFER_CHUNK;
    let tail = total % TRANSFER_CHUNK;
    if tail == 0 {
        (full, TRANSFER_CHUNK)
    } else {
        (full + 1, tail)
    }
}

/// Check a read: unknown minor is "input-output error", unseeded is "try
/// again".
///
/// C: `r_read` (`main.c:120-143`): the minor check comes first, then the
/// seed gate. Position and flags play no role (the stream has no positions;
/// blocking is intrinsic).
pub const fn check_read(minor: u32, seeded: bool) -> i32 {
    if minor != RANDOM_MINOR {
        return -EIO;
    }
    if !seeded {
        return -EAGAIN;
    }
    OK
}

/// Check a write (seeding): unknown minor refused, seed state irrelevant.
///
/// C: `r_write` (`main.c:151-172`): writes feed pool zero and never need a
/// seed.
pub const fn check_write(minor: u32) -> i32 {
    if minor != RANDOM_MINOR {
        return -EIO;
    }
    OK
}

/// Check an open: only minor zero exists.
///
/// C: `r_open` (`main.c:177-185`) answers "no such device" outside range.
pub const fn check_open(minor: u32) -> i32 {
    if minor != RANDOM_MINOR {
        return -ENXIO;
    }
    OK
}

/// Poll readiness: reads and writes are always instantly possible.
///
/// C: `r_select` (`main.c:259-268`): the device never blocks (reads answer
/// "try again" instead of parking), so every watched operation is ready at
/// once; the error bit is ignored.
pub const fn poll(ops: u32) -> u32 {
    ops & (OP_READ | OP_WRITE)
}

/// Operation bit: read readiness.
pub const OP_READ: u32 = 0x01;
/// Operation bit: write readiness.
pub const OP_WRITE: u32 = 0x02;

/// Zero-length transfers are refused upstream; kept here for the check.
pub const ZERO_LENGTH: i32 = -EINVAL;

/// Success marker.
pub const SUCCESS: i32 = OK;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_accepts_only_minor_zero() {
        assert_eq!(check_open(0), OK);
        assert_eq!(check_open(1), -ENXIO);
    }

    #[test]
    fn test_unseeded_reads_ask_to_retry() {
        assert_eq!(check_read(0, false), -EAGAIN);
        assert_eq!(check_read(0, true), OK);
        assert_eq!(check_read(3, true), -EIO);
    }

    #[test]
    fn test_writes_never_need_a_seed() {
        assert_eq!(check_write(0), OK);
        assert_eq!(check_write(2), -EIO);
    }

    #[test]
    fn test_poll_reports_everything_ready() {
        assert_eq!(poll(OP_READ | OP_WRITE), OP_READ | OP_WRITE);
        assert_eq!(poll(OP_READ), OP_READ);
        assert_eq!(poll(0), 0);
    }

    #[test]
    fn test_chunk_plan_covers_edges() {
        assert_eq!(chunk_plan(0), (0, 0));
        assert_eq!(chunk_plan(1024), (1, 1024));
        assert_eq!(chunk_plan(1025), (2, 1));
        assert_eq!(chunk_plan(2048), (2, 1024));
    }

    #[test]
    fn test_harvest_slows_after_seeding() {
        assert_eq!(harvest_period(false), 1);
        assert_eq!(harvest_period(true), 500);
        assert_eq!(TRANSFER_CHUNK, 1024);
    }
}
