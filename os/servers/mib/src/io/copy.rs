//! Copy verdicts: ranges, lengths, chunks, string scans.
//!
//! The copy face: opaque sinks/sources (`Oldp`/`Newp`) and the pure
//! span math under them.
//!
//! Mirrors `mib_inrange` / `mib_getoldlen` / `mib_copyout` /
//! `mib_setoldlen` / `mib_getnewlen` / `mib_copyin` / `mib_copyin_aux`
//! (`main.c:90-202`) and `mib_copyin_str` (`tree.c:371-420`). C's
//! `struct mib_oldp`/`mib_newp` are opaque to handlers on purpose
//! ("prevent accidental mixups", main.c:75-77) — the Rust transcriptions
//! are [`Oldp`]/[`Newp`], and the verbs (`copyout`/`copyin`/`copyin_str`)
//! are methods that drive the [`MibKernel`](crate::transport::MibKernel)
//! copy transport. The span math stays free and const so it stays
//! testable without a transport double; `[ARCH: ...]` C's global
//! `mib_oldp` pointers become values the per-call context owns.
//!
//! 06-mib-copy-io.md.

use minix_types::{EINVAL, Endpoint, OK};

use crate::transport::MibKernel;

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

/// The old-data sink: where answers travel, and how much fits.
///
/// C: `struct mib_oldp` — main.c:71-75 (`oldp_endpt`/`oldp_addr`/
/// `oldp_len`). Field names follow the C struct so the doc tables map
/// one to one; the sink is immutable for the life of the call —
/// `copyout` takes the absolute offset and never advances anything
/// (C's own shape, main.c:120-141).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Oldp {
    /// The caller holding the sink. C: `oldp_endpt`.
    pub endpt: Endpoint,
    /// Sink base address in the caller's space. C: `oldp_addr`.
    pub addr: u64,
    /// Sink capacity. C: `oldp_len`.
    pub left: u64,
}

impl Oldp {
    /// Copy `src` to sink offset `off`, clamped and reported C-style.
    ///
    /// C: `mib_copyout` — main.c:120-141. The clamp (`copyout_span`)
    /// decides how much moves; the transport decides whether the move
    /// succeeds, and its errno surfaces unchanged (:137-138). The
    /// reported length is always `src.len()` — "nothing to do" past the
    /// end is success, not error.
    pub fn copyout(&self, t: &mut impl MibKernel, off: u64, src: &[u8]) -> Result<u64, i32> {
        let span = copyout_span(Some(self.left), off, src.len() as u64);
        if span.xfer > 0 {
            t.datacopy_to(self.endpt, self.addr + off, &src[..span.xfer as usize])?;
        }
        Ok(span.report)
    }
}

/// The new-data source: what the caller wants written, exactly.
///
/// C: `struct mib_newp` — main.c:78-82. "Same structure, different
/// type" is C's comment for why the two structs exist; Rust's separate
/// types make the mixup unrepresentable rather than commented away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Newp {
    /// The caller holding the data. C: `newp_endpt`.
    pub endpt: Endpoint,
    /// Data base address in the caller's space. C: `newp_addr`.
    pub addr: u64,
    /// Data length. C: `newp_len`.
    pub len: u64,
}

impl Newp {
    /// Pull the caller's data, exact length required.
    ///
    /// C: `mib_copyin` — main.c:172-185. The exact-match judgement is
    /// [`check_copyin`]; the zero-length short circuit skips the
    /// transport (:180-181); anything else moves `buf.len()` bytes.
    pub fn copyin(&self, t: &mut impl MibKernel, buf: &mut [u8]) -> Result<(), i32> {
        let judged = check_copyin(Some(self.len), buf.len() as u64);
        if judged != OK {
            return Err(judged);
        }
        if buf.is_empty() {
            return Ok(());
        }
        t.datacopy_from(self.endpt, self.addr, buf)
    }

    /// Pull a NUL-terminated string, page chunk by page chunk.
    ///
    /// C: `mib_copyin_str` — tree.c:371-420. Each round moves one page-
    /// bounded chunk ([`next_chunk`]) into the scratch room, scans for
    /// the NUL ([`memchr`-equivalent `position`]), and reports
    /// [`nul_size`] on hit. A scratch spent with no NUL is `EINVAL`
    /// (:418-419); a transport error surfaces unchanged (:404-405).
    pub fn copyin_str(&self, t: &mut impl MibKernel, buf: &mut [u8]) -> Result<u64, i32> {
        let mut done = 0usize;
        while done < buf.len() {
            let room = (buf.len() - done) as u64;
            // `room > 0` here, so `next_chunk` always answers `Some`.
            let chunk = next_chunk(self.addr + done as u64, room).unwrap_or(room) as usize;
            t.datacopy_from(self.endpt, self.addr + done as u64, &mut buf[done..done + chunk])?;
            if let Some(at) = buf[done..done + chunk].iter().position(|&byte| byte == 0) {
                return Ok(nul_size(done as u64, at as u64));
            }
            done += chunk;
        }
        Err(EINVAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::recording::{Call, Recorder};
    use minix_types::Endpoint;

    const CALLER: Endpoint = Endpoint::PM;

    #[test]
    fn test_oldp_copyout_clamps_and_reports() {
        let sink = Oldp {
            endpt: CALLER,
            addr: 0x2000,
            left: 16,
        };
        let mut t = Recorder::default();
        // Fits whole: everything moves, full length reported (:120-141).
        assert_eq!(sink.copyout(&mut t, 0, &[0xAA; 16]).unwrap(), 16);
        // Partially past the end: only the fitting part moves, the
        // report is still the full size — the caller learns how much
        // was asked for, the wire carries the fit.
        assert_eq!(sink.copyout(&mut t, 8, &[0xBB; 16]).unwrap(), 16);
        // Past the end: nothing moves, still success ("nothing to do").
        assert_eq!(sink.copyout(&mut t, 99, &[0xCC; 4]).unwrap(), 4);
        let calls = t.calls.borrow();
        assert_eq!(calls.len(), 2, "the past-the-end copy moved nothing");
        assert!(matches!(
            calls[0],
            Call::DatacopyTo(CALLER, 0x2000, 16)
        ));
        assert!(matches!(
            calls[1],
            Call::DatacopyTo(CALLER, 0x2008, 8)
        ));
    }

    #[test]
    fn test_oldp_copyout_transport_error_surfaces() {
        let sink = Oldp {
            endpt: CALLER,
            addr: 0x2000,
            left: 64,
        };
        let mut t = Recorder::default();
        t.fail = true;
        assert_eq!(sink.copyout(&mut t, 0, &[0u8; 8]), Err(minix_types::EIO));
    }

    #[test]
    fn test_newp_copyin_exact_zero_mismatch() {
        let src = Newp {
            endpt: CALLER,
            addr: 0x3000,
            len: 8,
        };
        let mut t = Recorder::default();
        // Exact: moves (:172-185).
        let mut buf = [0u8; 8];
        assert_eq!(src.copyin(&mut t, &mut buf), Ok(()));
        // Wrong size: EINVAL before any bytes move (:177-178) — even a
        // zero buffer against a nonzero region is a mismatch, not "no-op".
        assert_eq!(src.copyin(&mut t, &mut [0u8; 4]), Err(EINVAL));
        assert_eq!(src.copyin(&mut t, &mut []), Err(EINVAL));
        // Zero region with zero buffer: the genuine short circuit
        // (:180-181) — success without touching the transport.
        let empty = Newp {
            endpt: CALLER,
            addr: 0x3000,
            len: 0,
        };
        assert_eq!(empty.copyin(&mut t, &mut []), Ok(()));
        assert_eq!(t.calls.borrow().len(), 1);
        assert!(matches!(
            t.calls.borrow()[0],
            Call::DatacopyFrom(CALLER, 0x3000, 8)
        ));
    }

    #[test]
    fn test_newp_copyin_str_finds_nul_across_chunks() {
        // Base 6 bytes before a page edge: the first chunk is page-
        // clamped to 4090, so a 5000-byte scratch spans two chunks
        // (4090 + 910) and the NUL lives in the second one.
        let src = Newp {
            endpt: CALLER,
            addr: 4096 - 6,
            len: 0, // C never checks the region length here; buf bounds it
        };
        let mut t = Recorder::default();
        let mut scratch = vec![b'x'; 5000];
        scratch[4090 + 5] = 0; // NUL inside the second chunk
        let n = src.copyin_str(&mut t, &mut scratch).unwrap();
        assert_eq!(n, 4090 + 5 + 1);
        assert_eq!(t.calls.borrow().len(), 2);
    }

    #[test]
    fn test_newp_copyin_str_exhausted_is_einval() {
        let src = Newp {
            endpt: CALLER,
            addr: 0x4000,
            len: 0,
        };
        let mut t = Recorder::default();
        // No NUL anywhere in the scratch: buffer spent → EINVAL (:418-419).
        let mut scratch = [b'x'; 6];
        assert_eq!(src.copyin_str(&mut t, &mut scratch), Err(EINVAL));
    }

    #[test]
    fn test_newp_copyin_str_transport_error_surfaces() {
        let src = Newp {
            endpt: CALLER,
            addr: 0x4000,
            len: 0,
        };
        let mut t = Recorder::default();
        t.fail = true;
        let mut scratch = [0u8; 6];
        assert_eq!(src.copyin_str(&mut t, &mut scratch), Err(minix_types::EIO));
    }

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
