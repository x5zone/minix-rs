//! VM dump domain (10-is-dump-vm.md §4.1).
//!
//! C: `minix3/minix/servers/is/dmp_vm.c` (157 lines). Batched address-space
//! dumping: first-screen headers, contiguous-region folding, dual-cursor
//! paging, LINES bounds. Bodies await the A-6 output channel; this module
//! delivers snapshots, the fold/batch state machines, and format constants.
//!
//! `[ARCH: A-4]` / `[ARCH: 26-D1]`: the VM-side producer ships query
//! results in the reply message's M1 slots (value channel — no copy into
//! the caller's address space), and the IS acquire client decodes those
//! slots into the snapshots below (`SysVmInfo`, 04 §3.2/§4.2b). The three
//! snapshots stay IS-side types: unlike the table channels (where producer
//! and consumer share one struct), VM_INFO has no byte-shaped payload to
//! share — the values travel as message slots.
//!
//! Open gap (`26-vm-queries.md` §4.8 D7): the REGION entry **array** is not
//! in the reply yet (the handler computes it; the encoder writes only
//! count/next). Until then the acquire client reports `-ENOTSUP` for a
//! non-empty batch instead of fabricating an empty address space.

/// VM statistics snapshot.
///
/// C: `struct vm_stats_info` — `minix3/minix/include/minix/vm.h:40-46` (subset).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct VmStatsSnap {
    /// C: `vsi_pagesize` (vm.h:41).
    pub vsi_pagesize: u32,
    /// C: `vsi_total` (vm.h:42).
    pub vsi_total: u64,
    /// C: `vsi_free` (vm.h:43).
    pub vsi_free: u64,
    /// C: `vsi_largest` (vm.h:44).
    pub vsi_largest: u64,
    /// C: `vsi_cached` (vm.h:45).
    pub vsi_cached: u64,
}

/// VM per-process usage snapshot.
///
/// C: `struct vm_usage_info` — vm.h:48-52 (subset: the three printed fields).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct VmUsageSnap {
    /// C: `vui_total` (vm.h:49).
    pub vui_total: u64,
    /// C: `vui_common` (vm.h:50).
    pub vui_common: u64,
    /// C: `vui_shared` (vm.h:51).
    pub vui_shared: u64,
}

/// VM region snapshot.
///
/// C: `struct vm_region_info` — vm.h:59-64.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct VmRegionSnap {
    /// C: `vri_addr` (vm.h:60).
    pub vri_addr: u64,
    /// C: `vri_length` (vm.h:61).
    pub vri_length: u64,
    /// C: `vri_prot` (vm.h:62).
    pub vri_prot: i32,
    /// C: `vri_flags` (vm.h:63).
    pub vri_flags: i32,
}

/// Protection bits. C: `PROT_READ 0x01` / `WRITE 0x02` / `EXEC 0x04` —
/// sys/sys/mman.h:62-65 (POSIX values).
pub const PROT_READ: i32 = 0x01;
/// See [`PROT_READ`].
pub const PROT_WRITE: i32 = 0x02;
/// See [`PROT_READ`].
pub const PROT_EXEC: i32 = 0x04;

/// Protection characters (`rwx`).
///
/// C: `(prot & PROT_X) ? 'x' : '-'` ×3 — dmp_vm.c:27-29.
pub const fn prot_chars(prot: i32) -> [u8; 3] {
    [
        if prot & PROT_READ != 0 { b'r' } else { b'-' },
        if prot & PROT_WRITE != 0 { b'w' } else { b'-' },
        if prot & PROT_EXEC != 0 { b'x' } else { b'-' },
    ]
}

/// One fold step outcome.
///
/// C: `print_region` — dmp_vm.c:11-50. `None` input flushes (end of list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldAction {
    /// Absorbed into a contiguous run (counter bumped, nothing printed).
    Buffered,
    /// Emit the repeat line (`vri_count` more times), then continue.
    FlushRepeat(u32),
    /// Emit a region line.
    FlushRegion,
    /// End of list, nothing pending.
    FlushEnd,
}

/// Contiguous-region folder: pure replacement for the three statics
/// (`vri_count`, `vri_prev_set`, `vri_prev` — dmp_vm.c:13-15).
#[derive(Debug, Clone, Copy, Default)]
pub struct FoldState {
    count: u32,
    prev: Option<VmRegionSnap>,
}

impl FoldState {
    pub const fn new() -> Self {
        Self { count: 0, prev: None }
    }

    /// Feeds one region (`None` = end of list).
    ///
    /// C: repeat test (dmp_vm.c:18-23) → absorb (:24-27) → flush-repeat
    /// (:29-33) → NULL/end return (:35-36) → region line + `n++` (:38-49).
    /// The `n++` counting belongs to the batch machine; this returns the
    /// action so the caller counts exactly the C sites.
    pub fn push(&mut self, vri: Option<&VmRegionSnap>) -> FoldAction {
        let is_repeat = match (vri, self.prev) {
            (Some(cur), Some(prev)) => {
                cur.vri_prot == prev.vri_prot
                    && cur.vri_flags == prev.vri_flags
                    && cur.vri_length == prev.vri_length
                    && cur.vri_addr == prev.vri_addr + prev.vri_length
            }
            _ => false,
        };
        if let Some(cur) = vri {
            self.prev = Some(*cur);
        } else {
            self.prev = None;
        }
        if is_repeat {
            self.count += 1;
            return FoldAction::Buffered;
        }
        if self.count > 0 {
            let c = self.count;
            self.count = 0;
            // C prints the repeat line BEFORE handling the current region;
            // the current region stays buffered as `prev` for the next call.
            // To keep single-step semantics, report the flush; the caller
            // re-feeds the same region afterwards (documented contract).
            return FoldAction::FlushRepeat(c);
        }
        match vri {
            Some(_) => FoldAction::FlushRegion,
            None => FoldAction::FlushEnd,
        }
    }
}

/// Batch rows per screen. C: `LINES 24` — dmp_vm.c:18 (differs from 05's 22).
pub const VM_LINES: u32 = 24;

/// Dual-cursor batch state.
///
/// C: `prev_i = -1` + `prev_base = 0` (dmp_vm.c:62-63); `first =
/// prev_base == 0` (:91); capacity precheck `n + 1 + r > LINES →
/// prev_base = 0 + break` (:97-100); tail `i = -1, prev_base = 0` vs
/// `"--more--\r"` (:140-145).
#[derive(Debug, Clone, Copy, Default)]
pub struct BatchCursor {
    /// Process index cursor (`prev_i`).
    pub prev_i: i32,
    /// Region base cursor (`prev_base`).
    pub prev_base: u64,
}

impl BatchCursor {
    pub const fn new() -> Self {
        Self { prev_i: -1, prev_base: 0 }
    }

    /// First screen (stats + blank lines) consumed. C: dmp_vm.c:64-80.
    pub const fn first_screen_done(&self) -> bool {
        self.prev_i != -1
    }

    /// Whether this process dump opens with a header line.
    /// C: `first = prev_base == 0` — dmp_vm.c:91.
    pub const fn is_first_batch(&self) -> bool {
        self.prev_base == 0
    }

    /// Capacity precheck for a first batch of `r` regions at `n` rows used.
    /// C: `if (n + 1 + r > LINES) { prev_base = 0; break; }` — :97-100.
    /// Returns `true` when the batch fits (caller prints header).
    pub const fn fits_first_batch(&mut self, n: u32, r: u32) -> bool {
        if n + 1 + r > VM_LINES {
            self.prev_base = 0;
            return false;
        }
        true
    }
}

/// C: stats line — dmp_vm.c:68-72 (`%lu kB` ×4, `pagesize / 1024` scale).
pub const VM_STATS_LABEL: &str = "Total";
/// C: process header words — dmp_vm.c:111-115.
pub const VM_PROC_WORDS: &str = "total";
/// C: repeat line words — dmp_vm.c:30 (`contiguously repeated %d more times`).
pub const VM_REPEAT_WORDS: &str = "contiguously repeated";
/// C: wipe line — dmp_vm.c:137 (`"        \n"`, 8 spaces).
pub const VM_WIPE_LINE: &str = "        \n";
/// C: defensive branch — dmp_vm.c:135 (`n > LINES` "internal error").
/// Documented unreachable-under-contract (batches are prechecked); kept as
/// a named constant so the defense stays greppable.
pub const VM_INTERNAL_ERROR: &str = "IS: internal error\n";

// ── render (V1-P1-3 execution face) ───────────────────────────────

use crate::acquire::VmInfoTransport;
use crate::dump_kernel::{ProcInfoStruct, NR_TASKS, RTS_SLOT_FREE};
use crate::PCStr;
use core::fmt;
use minix_types::{Endpoint, OK};

/// C: `printf("--more--\r")` — dmp_vm.c:144.
pub const MORE_CR_VM: &str = "--more--\r";

/// Writes one region line. C: `print_region` tail — dmp_vm.c:38-49
/// (`"  %08lx-%08lx %c%c%c (%lu kB)\n"`).
fn write_region_line(out: &mut dyn fmt::Write, vri: &VmRegionSnap) -> fmt::Result {
    let [r, w, x] = prot_chars(vri.vri_prot);
    writeln!(
        out,
        "  {:08x}-{:08x} {}{}{} ({})",
        vri.vri_addr,
        vri.vri_addr + vri.vri_length,
        r as char,
        w as char,
        x as char,
        vri.vri_length / 1024
    )
}

/// Writes the contiguous-repeat line. C: dmp_vm.c:29-33.
fn write_repeat_line(out: &mut dyn fmt::Write, count: u32) -> fmt::Result {
    writeln!(out, "  (contiguously repeated {} more times)", count)
}

/// Drives the fold for one region (`None` = end of list). C's
/// `print_region` flushes the repeat count and prints the current region
/// in the same call; [`FoldState`] is single-step, so the caller loops
/// (documented contract — doc 10 §3 D2).
fn feed_region(
    out: &mut dyn fmt::Write,
    fold: &mut FoldState,
    vri: Option<&VmRegionSnap>,
    n: &mut u32,
) -> fmt::Result {
    loop {
        match fold.push(vri) {
            FoldAction::Buffered => return Ok(()),
            FoldAction::FlushEnd => return Ok(()),
            FoldAction::FlushRepeat(c) => {
                write_repeat_line(out, c)?;
                *n += 1;
            }
            FoldAction::FlushRegion => {
                if let Some(v) = vri {
                    write_region_line(out, v)?;
                    *n += 1;
                }
                return Ok(());
            }
        }
    }
}

/// `vm_dmp` (dmp_vm.c:54-157). The one body that cannot be fetch-then-
/// render: region batches are pulled per process mid-loop, so this render
/// holds the transport. State = proctab copy + [`FoldState`] +
/// [`BatchCursor`], all carried across key presses like the C statics.
pub fn render_vm(
    out: &mut dyn fmt::Write,
    tab: &[ProcInfoStruct],
    acq: &mut dyn VmInfoTransport,
    fold: &mut FoldState,
    cur: &mut BatchCursor,
) -> fmt::Result {
    let mut n: u32 = 0;

    // First press opens with the system stats screen (dmp_vm.c:56-80).
    if !cur.first_screen_done() {
        let mut vsi = VmStatsSnap::default();
        let r = acq.vm_stats(&mut vsi);
        if r != OK {
            writeln!(out, "IS: warning: couldn't talk to VM: {r}")?;
            return Ok(());
        }
        let page_k = (vsi.vsi_pagesize / 1024) as u64;
        writeln!(
            out,
            "Total {} kB, free {} kB, largest free {} kB, cached {} kB",
            vsi.vsi_total * page_k,
            vsi.vsi_free * page_k,
            vsi.vsi_largest * page_k,
            vsi.vsi_cached * page_k
        )?;
        n += 1;
        writeln!(out)?;
        n += 1;
        cur.prev_i += 1;
    }

    let mut vri = [VmRegionSnap::default(); VM_LINES as usize];

    let mut i = cur.prev_i;
    while (i as usize) < tab.len() && n < VM_LINES {
        if (i as usize) < NR_TASKS as usize || tab[i as usize].p_rts_flags == RTS_SLOT_FREE {
            i += 1;
            cur.prev_base = 0;
            continue;
        }
        let p = &tab[i as usize];
        let first = cur.is_first_batch();

        // Region batch #1 for this process (dmp_vm.c:92-94).
        let cap = (VM_LINES as usize) - if first { 0 } else { 1 };
        let (r, next, count) = acq.vm_region(Endpoint(p.p_endpoint), &mut vri[..cap], cur.prev_base);
        if r < 0 {
            writeln!(out, "Process {} ({}): error {}", p.p_endpoint, PCStr(&p.p_name), r)?;
            n += 1;
            i += 1;
            cur.prev_base = 0;
            continue;
        }
        cur.prev_base = next;
        let mut r = count;

        // The whole first batch (header + rows) must fit on the screen;
        // otherwise restart this process on the next page (dmp_vm.c:96-100).
        if first {
            if n + 1 + r as u32 > VM_LINES {
                cur.prev_base = 0;
                break;
            }
            let mut vui = VmUsageSnap::default();
            let r2 = acq.vm_usage(Endpoint(p.p_endpoint), &mut vui);
            if r2 != OK {
                writeln!(out, "Process {} ({}): error {}", p.p_endpoint, PCStr(&p.p_name), r2)?;
                n += 1;
                i += 1;
                cur.prev_base = 0;
                continue;
            }
            writeln!(
                out,
                "Process {} ({}): total {} kB, common {} kB, shared {} kB",
                p.p_endpoint,
                PCStr(&p.p_name),
                vui.vui_total / 1024,
                vui.vui_common / 1024,
                vui.vui_shared / 1024
            )?;
            n += 1;
        }

        // Region batches until the screen fills (dmp_vm.c:118-138).
        while r > 0 {
            for v in vri.iter().take(r as usize) {
                feed_region(out, fold, Some(v), &mut n)?;
            }
            if VM_LINES as i32 - n as i32 - 1 <= 0 {
                break;
            }
            let cap = (VM_LINES as i32 - n as i32 - 1).max(0) as usize;
            let (st, next, count) = acq.vm_region(Endpoint(p.p_endpoint), &mut vri[..cap], cur.prev_base);
            if st < 0 {
                writeln!(out, "Process {} ({}): error {}", p.p_endpoint, PCStr(&p.p_name), st)?;
                n += 1;
                break;
            }
            cur.prev_base = next;
            r = count;
        }
        feed_region(out, fold, None, &mut n)?;

        if n > VM_LINES {
            writeln!(out, "{}", VM_INTERNAL_ERROR)?;
        }
        if n == VM_LINES {
            break;
        }

        // May wipe the "--more--" from below (dmp_vm.c:136-137).
        out.write_str(VM_WIPE_LINE)?;
        n += 1;
        i += 1;
        cur.prev_base = 0;
    }

    if i as usize >= tab.len() {
        cur.prev_i = -1;
        cur.prev_base = 0;
    } else {
        out.write_str(MORE_CR_VM)?;
    }
    cur.prev_i = i;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(addr: u64, len: u64) -> VmRegionSnap {
        VmRegionSnap { vri_addr: addr, vri_length: len, vri_prot: 3, vri_flags: 0 }
    }

    #[test]
    fn test_fold_absorb_and_flush() {
        // C: dmp_vm.c:18-33 (adjacent + identical → absorb).
        let mut f = FoldState::new();
        assert_eq!(f.push(Some(&region(0x1000, 0x1000))), FoldAction::FlushRegion);
        assert_eq!(f.push(Some(&region(0x2000, 0x1000))), FoldAction::Buffered);
        assert_eq!(f.push(Some(&region(0x3000, 0x1000))), FoldAction::Buffered);
        // New region flushes the repeat count first (contract: re-feed it).
        assert_eq!(f.push(Some(&region(0x9000, 0x2000))), FoldAction::FlushRepeat(2));
        assert_eq!(f.push(Some(&region(0x9000, 0x2000))), FoldAction::FlushRegion);
        assert_eq!(f.push(None), FoldAction::FlushEnd);
    }

    #[test]
    fn test_fold_breaks_on_gap_or_attr_change() {
        let mut f = FoldState::new();
        assert_eq!(f.push(Some(&region(0x1000, 0x1000))), FoldAction::FlushRegion);
        // Gap: not contiguous.
        assert_eq!(f.push(Some(&region(0x5000, 0x1000))), FoldAction::FlushRegion);
        // Attr change.
        let mut other = region(0x6000, 0x1000);
        other.vri_prot = 1;
        assert_eq!(f.push(Some(&other)), FoldAction::FlushRegion);
    }

    #[test]
    fn test_fold_state_survives_across_lists() {
        // C: statics persist across processes/lists (no reset per list).
        let mut f = FoldState::new();
        assert_eq!(f.push(Some(&region(0x1000, 0x1000))), FoldAction::FlushRegion);
        assert_eq!(f.push(None), FoldAction::FlushEnd);
        // Same region again is NOT a repeat (prev cleared by NULL).
        assert_eq!(f.push(Some(&region(0x1000, 0x1000))), FoldAction::FlushRegion);
    }

    #[test]
    fn test_batch_cursor_first_and_precheck() {
        // C: dmp_vm.c:62-63 init, :91 first, :97-100 precheck.
        let mut b = BatchCursor::new();
        assert!(!b.first_screen_done());
        b.prev_i = 0;
        assert!(b.first_screen_done());
        assert!(b.is_first_batch());
        assert!(b.fits_first_batch(10, 5));
        let mut b2 = BatchCursor { prev_i: 3, prev_base: 0x8000 };
        assert!(!b2.is_first_batch());
        assert!(!b2.fits_first_batch(20, 5)); // 20+1+5=26 > 24 → restart
        assert_eq!(b2.prev_base, 0);
    }

    #[test]
    fn test_prot_chars() {
        // C: dmp_vm.c:27-29 (PROT 1/2/4).
        assert_eq!(prot_chars(0x7), [b'r', b'w', b'x']);
        assert_eq!(prot_chars(0x1), [b'r', b'-', b'-']);
        assert_eq!(prot_chars(0), [b'-', b'-', b'-']);
        assert_eq!((PROT_READ, PROT_WRITE, PROT_EXEC), (0x01, 0x02, 0x04));
    }

    #[test]
    fn test_format_words_verbatim() {
        assert_eq!(VM_WIPE_LINE, "        \n");
        assert_eq!(VM_INTERNAL_ERROR, "IS: internal error\n");
        assert_eq!(VM_REPEAT_WORDS, "contiguously repeated");
        assert_eq!(VM_LINES, 24);
    }
}
