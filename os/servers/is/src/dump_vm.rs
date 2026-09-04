//! VM dump domain (10-is-dump-vm.md §4.1).
//!
//! C: `minix3/minix/servers/is/dmp_vm.c` (157 lines). Batched address-space
//! dumping: first-screen headers, contiguous-region folding, dual-cursor
//! paging, LINES bounds. Bodies await the A-6 output channel; this module
//! delivers snapshots, the fold/batch state machines, and format constants.
//!
//! `[ARCH: A-4]`: snapshots are wire-contract proposals (`#[repr(C)]`,
//! used-fields subsets); the VM-side producer aligns to them (pending
//! VM-crate work, explicit TODO below).

// TODO(P1): [code] [factual] Align VM-crate VM_INFO producers with the
// snapshots below — see 10 §4. (current state: IS-side wire proposals)
// (fix: VM `do_vm_info` stats/usage/region payloads match; A-4).

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
