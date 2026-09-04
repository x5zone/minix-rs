//! DS dump domain (09-is-dump-ds.md §4.1).
//!
//! C: `minix3/minix/servers/is/dmp_ds.c` (52 lines). One dump over the key-
//! value store: four typed rows, ring paging cursor (skip-empty + wrap),
//! unknown-type abort. Bodies await the A-6 output channel; this module
//! delivers snapshots, type decoding, cursor, and format constants.
//!
//! `[ARCH: A-4]`: snapshots are wire-contract proposals (`#[repr(C)]`,
//! used-fields subsets); the DS-side GETSYSINFO producer aligns to them
//! (pending DS-crate work — the consumer half of `07-stage-ds` A-10,
//! explicit TODO below).

// TODO(P1): [code] [factual] Align DS-crate GETSYSINFO producers with the
// snapshots below — see 09 §4. (current state: IS-side wire proposals)
// (fix: DS `do_getsysinfo` SI_DATA_STORE payload matches; A-4/A-10).

/// Number of store slots. C: `NR_DS_KEYS (2*NR_SYS_PROCS)` — store.h:12
/// (`NR_SYS_PROCS 64` — sys_config.h:9).
pub const NR_DS_KEYS: usize = 128;

/// Max key/owner length. C: `DS_MAX_KEYLEN 80` — ds.h:29.
pub const DS_MAX_KEYLEN: usize = 80;

/// In-use bit. C: `DSF_IN_USE 0x001` — ds.h:12.
pub const DSF_IN_USE: u32 = 0x001;
/// Type mask. C: `DSF_MASK_TYPE 0xFF0` — ds.h:22.
pub const DSF_MASK_TYPE: u32 = 0xFF0;

/// Data-store entry snapshot (scalar face only).
///
/// C: `struct data_store` — `minix3/minix/servers/ds/store.h:16-29`
/// (subset: the `STR` data pointer streams through the output layer,
/// like 08's `r_args`).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct DsEntrySnap {
    /// C: `flags` (store.h:17).
    pub flags: i32,
    /// C: `key[DS_MAX_KEYLEN]` (store.h:18).
    pub key: [u8; DS_MAX_KEYLEN],
    /// C: `owner[DS_MAX_KEYLEN]` (store.h:19).
    pub owner: [u8; DS_MAX_KEYLEN],
    /// Scalar payload (`u.u32`, or `u.mem.length` for `MEM`).
    pub scalar: u32,
}

impl Default for DsEntrySnap {
    // Manual: `[u8; 80]` has no `Default` — neighbouring-payload convention.
    fn default() -> Self {
        Self { flags: 0, key: [0u8; DS_MAX_KEYLEN], owner: [0u8; DS_MAX_KEYLEN], scalar: 0 }
    }
}

/// Value kind.
///
/// C: `DSF_TYPE_U32 0x010` / `STR 0x020` / `MEM 0x040` / `LABEL 0x100` —
/// ds.h:17-20. `LABEL` shares `u32` storage with `U32` (same value,
/// different tag — 08's dual-source trap in miniature).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsValKind {
    U32,
    Str,
    Mem,
    Label,
}

impl DsValKind {
    /// Decodes the masked flags; unknown types are `None` (C `default:`
    /// aborts the whole dump — dmp_ds.c:46-47).
    pub const fn decode(flags: i32) -> Option<Self> {
        match (flags as u32) & DSF_MASK_TYPE {
            0x010 => Some(DsValKind::U32),
            0x020 => Some(DsValKind::Str),
            0x040 => Some(DsValKind::Mem),
            0x100 => Some(DsValKind::Label),
            _ => None,
        }
    }

    /// Printed type word. C: `"U32"`/`"STR"`/`"MEM"`/`"LABEL"` — dmp_ds.c:39-45.
    pub const fn word(self) -> &'static str {
        match self {
            DsValKind::U32 => "U32",
            DsValKind::Str => "STR",
            DsValKind::Mem => "MEM",
            DsValKind::Label => "LABEL",
        }
    }
}

/// Whether a slot is skipped. C: `!(flags & DSF_IN_USE)` — dmp_ds.c:32-33.
pub const fn ds_skipped(flags: i32) -> bool {
    (flags as u32) & DSF_IN_USE == 0
}

/// data_store_dmp page cursor: ring flavor (third of three shapes).
///
/// C: `for (i = prev_i; i < NR_DS_KEYS && n < LINES; i++)` + `n++` at the
/// loop bottom (dmp_ds.c:29-49) + `if (i >= NR_DS_KEYS) i = 0; else
/// printf("--more--\r")` (:50-52). Max 22 printed rows; skips don't count;
/// `default:` aborts early WITHOUT updating `prev_i` (next round replays).
#[derive(Debug, Clone, Copy, Default)]
pub struct DsCursor {
    next: usize,
    n: u32,
    aborted: bool,
}

impl DsCursor {
    pub const fn new() -> Self {
        Self { next: 0, n: 0, aborted: false }
    }

    /// Test-only resume constructor (production resumes via `next()`).
    #[cfg(test)]
    pub const fn resume_for_test(next: usize) -> Self {
        Self { next, n: 0, aborted: false }
    }

    pub const fn next(&self) -> usize {
        self.next
    }

    /// Advances past one slot. Returns `false` when the screen is full
    /// (stop feeding rows).
    pub const fn push(&mut self, used: bool) -> bool {
        if !used {
            return self.n < 22;
        }
        if self.n >= 22 {
            return false;
        }
        self.n += 1;
        true
    }

    /// Advances the index past one visited slot (the `i++`).
    pub const fn step_index(&mut self, idx: usize) {
        self.next = idx + 1;
    }

    /// Ends the round: `aborted` (unknown type) keeps `next` for replay;
    /// exhaustion wraps to 0; break keeps the resume index.
    /// C: dmp_ds.c:50-52 (aborted path = bare `return`, prev_i untouched).
    pub const fn finish(&mut self, exhausted: bool, aborted: bool) {
        if aborted {
            self.aborted = true;
        } else if exhausted {
            self.next = 0;
            self.aborted = false;
        } else {
            self.aborted = false;
        }
    }

    pub const fn aborted(&self) -> bool {
        self.aborted
    }
}

/// C: `"Data store contents:\n"` — dmp_ds.c:24.
pub const DS_TITLE: &str = "Data store contents:\n";
/// C: dmp_ds.c:25.
pub const DS_COLUMNS: &str =
    "-slot- -----------key----------- -----owner----- ---type--- ----value---\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_decode_and_words() {
        // C: ds.h:17-22 + dmp_ds.c:38-47.
        assert_eq!(DsValKind::decode(0x001 | 0x010), Some(DsValKind::U32));
        assert_eq!(DsValKind::decode(0x001 | 0x020), Some(DsValKind::Str));
        assert_eq!(DsValKind::decode(0x001 | 0x040), Some(DsValKind::Mem));
        assert_eq!(DsValKind::decode(0x001 | 0x100), Some(DsValKind::Label));
        assert_eq!(DsValKind::decode(0x001 | 0x200), None);
        assert_eq!(DsValKind::Label.word(), "LABEL");
        assert_eq!((NR_DS_KEYS, DS_MAX_KEYLEN), (128, 80));
    }

    #[test]
    fn test_skip_rule() {
        // C: dmp_ds.c:32-33.
        assert!(ds_skipped(0));
        assert!(!ds_skipped(0x001));
    }

    #[test]
    fn test_cursor_pages_22_skips_free_abort_replays() {
        // C: dmp_ds.c:29-52.
        let mut c = DsCursor::new();
        let mut printed = 0;
        for i in 0..128 {
            let used = i % 3 != 0;
            if !c.push(used) {
                break;
            }
            if used {
                printed += 1;
            }
            c.step_index(i);
        }
        assert_eq!(printed, 22);
        // Exhaustion wraps.
        let mut c2 = DsCursor::new();
        c2.step_index(127);
        c2.finish(true, false);
        assert_eq!(c2.next(), 0);
        // Abort keeps the resume index for replay.
        let mut c3 = DsCursor::resume_for_test(40);
        c3.finish(false, true);
        assert_eq!(c3.next(), 40);
        assert!(c3.aborted());
    }

    #[test]
    fn test_format_strings_verbatim() {
        assert_eq!(DS_TITLE, "Data store contents:\n");
        assert!(DS_COLUMNS.contains("---type--- ----value---"));
    }
}
