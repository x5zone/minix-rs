//! Copy verdicts: ranges, lengths, chunks, string scans.
//!
//! Mirrors the pure halves of `mib_inrange` / `mib_getoldlen` /
//! `mib_copyout` / `mib_setoldlen` / `mib_getnewlen` / `mib_copyin` /
//! `mib_copyin_aux` (`main.c:90-202`) and `mib_copyin_str`
//! (`tree.c:371-420`). Every function answers "what (would) move" — the
//! moves themselves (`sys_datacopy`) are effects owned by the transport
//! (A-12, `minix-sys`); constructors here build the request values the
//! transport executes.
//!
//! 06-mib-copy-io.md.

use minix_types::{EINVAL, OK};

/// Page size for string-chunk math. C: `PAGE_SIZE` — tree.c:398.
/// All three minix-rs archs use 4096 (03 §test_scratch_budget pins it).
pub const PAGE_SIZE: u64 = 4096;

/// Whether an offset needs preparing at all.
///
/// C: `mib_inrange` — main.c:90-98. A shut sink (`None`, C `NULL`)
/// answers false without touching the offset: handlers use this to skip
/// preparing bytes nobody asked for.
pub const fn in_range(old_len: Option<u64>, off: u64) -> bool {
    match old_len {
        Some(len) => off < len,
        None => false,
    }
}

/// Total requested length; shut sinks report zero.
///
/// C: `mib_getoldlen` — main.c:105-113. Kept separate from `in_range`
/// because unusual handlers need the raw length, not the boolean.
pub const fn get_old_len(old_len: Option<u64>) -> u64 {
    match old_len {
        Some(len) => len,
        None => 0,
    }
}

/// A clamped copy-out: how much travels, what the caller reports.
///
/// C: `mib_copyout` — main.c:120-141. Copies never overrun the sink
/// (`len = min(size, old_len - off)`); past-the-end offsets move nothing
/// yet still report `size` ("nothing to do", :130-131); failures surface
/// as the transport's errno instead of a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopySpan {
    /// Bytes the transport moves (0 = nothing to do).
    pub xfer: u64,
    /// Length the caller reports on success (always `size`).
    pub report: u64,
}

/// Clamp a copy-out to the sink window.
///
/// Infallible at the verdict level: past-the-end offsets move nothing yet
/// still report `size` (C "nothing to do"), and transport errors surface
/// later as the transport's errno — there is no `None` case to report.
pub const fn copyout_span(old_len: Option<u64>, off: u64, size: u64) -> CopySpan {
    match old_len {
        None => CopySpan { xfer: 0, report: size },
        Some(len) => {
            if off >= len {
                return CopySpan { xfer: 0, report: size };
            }
            let mut xfer = size;
            if xfer > len - off {
                xfer = len - off;
            }
            CopySpan { xfer, report: size }
        }
    }
}

/// New-data length; absent writers report zero.
///
/// C: `mib_getnewlen` — main.c:158-166.
pub const fn get_new_len(new_len: Option<u64>) -> u64 {
    match new_len {
        Some(len) => len,
        None => 0,
    }
}

/// Judge an exact-length copy-in.
///
/// C: `mib_copyin` — main.c:172-185. The length must match *exactly*
/// (`len != new_len` → `EINVAL`, :177-178 — a short write names different
/// data, not less data); zero-length copies succeed without touching the
/// transport (:180-181); shut sinks refuse everything (:177).
pub const fn check_copyin(new_len: Option<u64>, len: u64) -> i32 {
    match new_len {
        None => EINVAL,
        Some(want) => {
            if len != want {
                return EINVAL;
            }
            OK
        }
    }
}

/// One string-scan chunk: page-bounded, buffer-clamped.
///
/// C: `mib_copyin_str` chunk math — tree.c:397-400. Chunks stop at page
/// boundaries (a chunk never triggers a fault past the page holding
/// bytes already proven readable — :385-394) and never exceed the
/// remaining buffer. Returns `None` when the buffer is spent (loop ends →
/// "no NUL found" → `EINVAL`, :418-419).
pub const fn next_chunk(addr: u64, bufsize: u64) -> Option<u64> {
    if bufsize == 0 {
        return None;
    }
    let mut chunk = PAGE_SIZE - (addr % PAGE_SIZE);
    if chunk > bufsize {
        chunk = bufsize;
    }
    Some(chunk)
}

/// Total string size once the NUL is found in this chunk.
///
/// C: `:406-410`. `len` bytes scanned before this chunk, NUL at `at`
/// within it; the +1 counts the terminator itself.
pub const fn nul_size(len: u64, at: u64) -> u64 {
    len + at + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_range_and_len() {
        // Shut sinks answer false/zero (main.c:93-97,109-112).
        assert!(!in_range(None, 0));
        assert_eq!(get_old_len(None), 0);
        assert!(in_range(Some(64), 0));
        assert!(in_range(Some(64), 63));
        assert!(!in_range(Some(64), 64));
        assert_eq!(get_old_len(Some(64)), 64);
        assert_eq!(get_new_len(None), 0);
        assert_eq!(get_new_len(Some(9)), 9);
    }

    #[test]
    fn test_copyout_span_clamps() {
        // Inside: full span (main.c:127-134).
        assert_eq!(copyout_span(Some(64), 0, 16), CopySpan { xfer: 16, report: 16 });
        // Tail: clamped, report still full (callers need the full length).
        assert_eq!(
            copyout_span(Some(64), 60, 16),
            CopySpan { xfer: 4, report: 16 }
        );
        // Past the end / shut sink: nothing moves, size still reported.
        assert_eq!(
            copyout_span(Some(64), 64, 16),
            CopySpan { xfer: 0, report: 16 }
        );
        assert_eq!(copyout_span(None, 0, 16), CopySpan { xfer: 0, report: 16 });
    }

    #[test]
    fn test_check_copyin_exact() {
        // Exact match only (main.c:177-181).
        assert_eq!(check_copyin(Some(16), 16), OK);
        assert_eq!(check_copyin(Some(16), 15), EINVAL);
        assert_eq!(check_copyin(Some(16), 17), EINVAL);
        assert_eq!(check_copyin(None, 0), EINVAL);
        assert_eq!(check_copyin(Some(0), 0), OK);
    }

    #[test]
    fn test_next_chunk_page_bounded() {
        // Chunk stops at the page edge (tree.c:398).
        assert_eq!(next_chunk(0x1000, 8192), Some(4096));
        assert_eq!(next_chunk(0x1ffc, 8192), Some(4));
        // Clamped to the buffer (tree.c:399-400).
        assert_eq!(next_chunk(0x1000, 100), Some(100));
        // Spent buffer ends the loop → EINVAL downstream.
        assert_eq!(next_chunk(0x1000, 0), None);
        // NUL accounting includes the terminator (tree.c:408-409).
        assert_eq!(nul_size(4096, 3), 4100);
        assert_eq!(nul_size(0, 0), 1);
    }
}
