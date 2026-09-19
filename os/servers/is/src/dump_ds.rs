//! DS dump domain (09-is-dump-ds.md §4.1).
//!
//! C: `minix3/minix/servers/is/dmp_ds.c` (52 lines). One dump over the key-
//! value store: four typed rows, ring paging cursor (skip-empty + wrap),
//! unknown-type abort. Bodies await the A-6 output channel; this module
//! delivers snapshots, type decoding, cursor, and format constants.
//!
//! `[ARCH: A-4]`: the table row is [`minix_types::DsEntrySnap`] — the single
//! authority shared with the DS-side `SI_DATA_STORE` producer
//! (`os/servers/ds/src/server.rs` 的 `render_image`); this module re-exports
//! it (and the slot/flag constants) rather than keeping divergent copies.
pub use minix_types::{DsEntrySnap, DSF_IN_USE, DSF_MASK_TYPE, DS_MAX_KEYLEN, NR_DS_KEYS};

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

// ── render (V1-P1-3 execution face) ───────────────────────────────

use crate::PCStr;
use core::fmt;

/// `data_store_dmp` (dmp_ds.c:9-51): ring-paged inventory — skip unused
/// slots, print the scalar face per type, abort replay-style on an unknown
/// type. STR deviation (`// MINIX3 BUG:`): C's `%12s` prints
/// `p->u.mem.data` — a pointer that only means something inside the DS
/// process, so the copied table makes C print a dangling address; the
/// rewrite prints the scalar word instead (pattern 78, documented here and
/// in 09 §2.2).
pub fn render_data_store(
    out: &mut dyn fmt::Write,
    tab: &[DsEntrySnap],
    cur: &mut DsCursor,
) -> fmt::Result {
    out.write_str(DS_TITLE)?;
    out.write_str(DS_COLUMNS)?;
    let mut exhausted = true;
    let mut aborted = false;
    for (i, e) in tab.iter().enumerate() {
        let used = !ds_skipped(e.flags);
        if !cur.push(used) {
            exhausted = false;
            break;
        }
        if !used {
            continue;
        }
        write!(
            out,
            "{:>6} {:<25} {:<15} ",
            i as i32,
            PCStr(&e.key),
            PCStr(&e.owner)
        )?;
        match DsValKind::decode(e.flags) {
            Some(kind) => match kind {
                DsValKind::U32 | DsValKind::Label => {
                    writeln!(out, "{:<10} {:>12}", kind.word(), e.scalar)?;
                }
                // MINIX3 BUG: dmp_ds.c:37 `%12s` of a foreign pointer.
                DsValKind::Str | DsValKind::Mem => {
                    writeln!(out, "{:<10} {:>12}", kind.word(), e.scalar)?;
                }
            },
            None => {
                aborted = true;
                break;
            }
        }
        cur.step_index(i);
    }
    cur.finish(exhausted, aborted);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_data_store_rows_and_abort() {
        // C: dmp_ds.c:24-50 — four-typed rows; unknown type aborts with the
        // cursor replaying (MINIX3 BUG: the STR column prints the scalar,
        // see render_data_store).
        let mut tab = [DsEntrySnap::default(); 4];
        tab[0].flags = 0x010; // unused (no IN_USE) → skipped
        tab[1].flags = 0x011; // U32
        tab[1].key = key(b"answer");
        tab[1].scalar = 42;
        tab[1].owner = owner(b"pm");
        tab[2].flags = 0x041; // MEM
        tab[2].key = key(b"blob");
        tab[2].scalar = 4096;
        tab[3].flags = 0x201; // unknown type 0x200 → abort
        tab[3].key = key(b"junk");
        let mut out = String::new();
        let mut cur = DsCursor::new();
        render_data_store(&mut out, &tab, &mut cur).unwrap();
        assert!(out.contains(
            "     2 blob                                      MEM                4096\n"
        ));
        assert!(out.contains("     3 junk"), "partial slot line prints before the abort");
        assert!(cur.aborted() && cur.next() == 3, "abort keeps prev_i for replay");
    }

    fn key(k: &[u8]) -> [u8; DS_MAX_KEYLEN] {
        let mut n = [0u8; DS_MAX_KEYLEN];
        n[..k.len()].copy_from_slice(k);
        n
    }

    fn owner(o: &[u8]) -> [u8; DS_MAX_KEYLEN] {
        key(o)
    }

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
