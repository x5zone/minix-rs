//! Kernel dump domain (05-is-dump-kernel.md §4.1).
//!
//! C: `minix3/minix/servers/is/dmp_kernel.c` (396 lines). Covers the eight
//! kernel dumps + four helpers + pagination macros. Dump *bodies* (which
//! need the A-6 output channel) land with 05~10's execution pass; this
//! module delivers the wire snapshots, flag encoders, pagination state
//! machine, and format constants — everything testable without printing.
//!
//! `[ARCH: A-4]`: the snapshot structs below are minix-rs wire-contract
//! proposals (`#[repr(C)]`, C declaration order, used-fields subset — the
//! full C structs carry pointers/arch segments no snapshot can mirror).
//! Kernel-side GETINFO producers align to them (pending kernel-crate
//! work, explicit TODO below). Layouts: C `int→i32`, `endpoint_t→i32`,
//! `char[N]→[u8;N]`, `u32_t→u32`, `clock_t→i32` (32-bit Minix).

// TODO(P1): [code] [factual] Align kernel-crate GETINFO producers with the
// snapshot layouts below (dump_kernel.rs:KwSnap structs) — see 05 §4.
// (current state: snapshots are IS-side wire proposals)
// (fix: kernel `do_getinfo` arms emit these layouts; see A-4).

use minix_types::Endpoint;

/// Kernel process-table snapshot (used fields only).
///
/// C: `struct proc` — `minix3/minix/kernel/proc.h:22-82` (subset).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct KProcSnap {
    /// C: `p_rts_flags` (proc.h:27).
    pub p_rts_flags: u32,
    /// C: `p_getfrom_e` (proc.h:75).
    pub p_getfrom_e: i32,
    /// C: `p_sendto_e` (proc.h:76).
    pub p_sendto_e: i32,
    /// C: `p_name[PROC_NAME_LEN]` (proc.h:80, type.h:145: 16).
    pub p_name: [u8; 16],
    /// C: `p_priority` (proc.h:30).
    pub p_priority: i8,
    /// C: `p_quantum_size_ms` (proc.h:32).
    pub p_quantum_size_ms: u32,
    /// C: `p_user_time` (proc.h:59).
    pub p_user_time: i32,
    /// C: `p_sys_time` (proc.h:60).
    pub p_sys_time: i32,
    /// C: `p_endpoint` (proc.h:82).
    pub p_endpoint: i32,
    /// C: `p_nr` (proc.h:25).
    pub p_nr: i32,
}

/// Kernel privilege snapshot (used fields only).
///
/// C: `struct priv` — `minix3/minix/kernel/priv.h:21-61` (subset).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct KPrivSnap {
    /// C: `s_proc_nr` (priv.h:22).
    pub s_proc_nr: i32,
    /// C: `s_id` (priv.h:23).
    pub s_id: i32,
    /// C: `s_flags` (priv.h:24).
    pub s_flags: i16,
    /// C: `s_trap_mask` (priv.h:34).
    pub s_trap_mask: i16,
    /// C: `s_grant_entries` (priv.h:62).
    pub s_grant_entries: i32,
}

/// Boot-image entry snapshot.
///
/// C: `struct boot_image` — `minix3/minix/include/minix/type.h:148-154`
/// (subset: the dump prints only these two — dmp_kernel.c:180-183, even
/// though the header names flags/stack columns).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct BootImageSnap {
    /// C: `proc_nr` (type.h:149).
    pub proc_nr: i32,
    /// C: `proc_name[PROC_NAME_LEN]` (type.h:150).
    pub proc_name: [u8; 16],
}

/// Kernel info snapshot (printed fields only).
///
/// C: `struct kinfo` — `minix3/minix/include/minix/param.h:14-54` (subset).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct KinfoSnap {
    /// C: `nr_procs` (param.h:40).
    pub nr_procs: i32,
    /// C: `nr_tasks` (param.h:41).
    pub nr_tasks: i32,
    /// C: `release[6]` (param.h:42).
    pub release: [u8; 6],
    /// C: `version[6]` (param.h:43).
    pub version: [u8; 6],
}

/// Kernel-message ring snapshot (cursor fields only).
///
/// C: `struct kmessages` — `minix3/minix/include/minix/type.h:170-176`
/// (subset; the buffer itself streams through the output channel).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct KmessagesSnap {
    /// C: `km_next` (type.h:171).
    pub km_next: i32,
    /// C: `km_size` (type.h:172).
    pub km_size: i32,
}

/// IRQ hook snapshot (printed fields only).
///
/// C: `struct irq_hook` — `minix3/minix/kernel/type.h:18-26` (subset;
/// `next`/`handler` pointers never cross to IS).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IrqHookSnap {
    /// C: `proc_nr_e` (type.h:23).
    pub proc_nr_e: i32,
    /// C: `irq` (type.h:21).
    pub irq: i32,
    /// C: `policy` (type.h:25).
    pub policy: u32,
    /// C: `notify_id` (type.h:24).
    pub notify_id: u32,
    /// C: `id` (type.h:22).
    pub id: i32,
}

// ── flag encoders (A-12) ──────────────────────────────────────────

/// Privilege-flag characters.
///
/// C: `s_flags_str` — dmp_kernel.c:218-231 (`PREEMPTIBLE 0x002→P`,
/// `BILLABLE 0x004→B`, `DYN_PRIV_ID 0x008→D`, `SYS_PROC 0x010→S`,
/// `CHECK_IO_PORT 0x020→I`, `CHECK_IRQ 0x040→Q`, `CHECK_MEM 0x080→M` —
/// const.h:143-150).
pub const fn s_flags_str(flags: i16) -> [u8; 8] {
    let f = flags as u32;
    [
        if f & 0x002 != 0 { b'P' } else { b'-' },
        if f & 0x004 != 0 { b'B' } else { b'-' },
        if f & 0x008 != 0 { b'D' } else { b'-' },
        if f & 0x010 != 0 { b'S' } else { b'-' },
        if f & 0x020 != 0 { b'I' } else { b'-' },
        if f & 0x040 != 0 { b'Q' } else { b'-' },
        if f & 0x080 != 0 { b'M' } else { b'-' },
        0,
    ]
}

/// Trap-mask characters.
///
/// C: `s_traps_str` — dmp_kernel.c:236-247 (`1<<SEND→S`, `1<<SENDA→A`,
/// `1<<RECEIVE→R`, `1<<SENDREC→B`, `1<<NOTIFY→N`; `SEND=1`, `RECEIVE=2`,
/// `SENDREC=3`, `NOTIFY=4`, `SENDA=16` — ipcconst.h:7-16).
pub const fn s_traps_str(flags: i16) -> [u8; 6] {
    let f = flags as u32;
    [
        if f & (1 << 1) != 0 { b'S' } else { b'-' },
        if f & (1 << 16) != 0 { b'A' } else { b'-' },
        if f & (1 << 2) != 0 { b'R' } else { b'-' },
        if f & (1 << 3) != 0 { b'B' } else { b'-' },
        if f & (1 << 4) != 0 { b'N' } else { b'-' },
        0,
    ]
}

/// Run-state characters.
///
/// C: `p_rts_flags_str` — dmp_kernel.c:300-313 (`RTS_PROC_STOP 0x02→s`,
/// `RTS_SENDING 0x04→S`, `RTS_RECEIVING 0x08→R`, `RTS_SIGNALED 0x10→I`,
/// `RTS_SIG_PENDING 0x20→P`, `RTS_P_STOP 0x40→T`, `RTS_NO_PRIV 0x80→p` —
/// proc.h:143-149).
pub const fn p_rts_flags_str(flags: u32) -> [u8; 8] {
    [
        if flags & 0x02 != 0 { b's' } else { b'-' },
        if flags & 0x04 != 0 { b'S' } else { b'-' },
        if flags & 0x08 != 0 { b'R' } else { b'-' },
        if flags & 0x10 != 0 { b'I' } else { b'-' },
        if flags & 0x20 != 0 { b'P' } else { b'-' },
        if flags & 0x40 != 0 { b'T' } else { b'-' },
        if flags & 0x80 != 0 { b'p' } else { b'-' },
        0,
    ]
}

// ── pagination (PROCLOOP) ─────────────────────────────────────────

/// Paged rows per screen. C: `LINES 22` — dmp_kernel.c:18.
pub const LINES: u32 = 22;

/// Continuation marker. C: `printf("--more--\n")` — dmp_kernel.c:37.
pub const MORE_MARKER: &str = "--more--\n";

/// Free-slot mark. C: `RTS_SLOT_FREE 0x01`, `isemptyp(p)` — proc.h:142,274.
pub const RTS_SLOT_FREE: u32 = 0x01;

/// One pagination step outcome.
///
/// C: `PROCLOOP` — dmp_kernel.c:32-40 (zero counter, skip empties, break
/// with `--more--` at `LINES`, else emit with the three-way row head).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageAction {
    /// Empty slot: skip without counting.
    Skip,
    /// Row emitted, more room on this screen.
    Emit(RowHead),
    /// Screen full: print `--more--`, resume after this row next round.
    More,
}

/// Row-head style.
///
/// C: `IDLE` (`(endpoint_t)-4`, com.h:48) prints `(dd)`; negative slots
/// print `[dd]`; user slots print ` dd ` (dmp_kernel.c:38-40).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowHead {
    Idle,
    Task,
    User,
}

/// Classifies a row head by slot number. C: dmp_kernel.c:38-40.
pub const fn row_head(proc_nr: i32) -> RowHead {
    if proc_nr == -4 {
        RowHead::Idle
    } else if proc_nr < 0 {
        RowHead::Task
    } else {
        RowHead::User
    }
}

/// PROCLOOP cursor: one instance per dump (C keeps three independent
/// `static oldrp` — privileges_dmp:256, proctab_dmp:324, procstack_dmp:364).
#[derive(Debug, Clone, Copy, Default)]
pub struct PageCursor {
    lines: u32,
}

impl PageCursor {
    pub const fn new() -> Self {
        Self { lines: 0 }
    }

    /// Advances past one table row. `rts_flags == RTS_SLOT_FREE` rows are
    /// skipped (C `isemptyp → continue`, which bypasses the counter).
    pub const fn push_row(&mut self, rts_flags: u32, proc_nr: i32) -> PageAction {
        if rts_flags == RTS_SLOT_FREE {
            return PageAction::Skip;
        }
        self.lines += 1;
        if self.lines >= LINES {
            return PageAction::More;
        }
        PageAction::Emit(row_head(proc_nr))
    }
}

// ── kmessages ring ────────────────────────────────────────────────

/// Ring-buffer size. C: `_KMESS_BUF_SIZE 10000` — sys_config.h:22.
pub const KMESS_BUF_SIZE: i32 = 10000;

/// Computes the print start in the circular buffer.
///
/// C: `start = ((km_next + SIZE) - km_size) % SIZE` — dmp_kernel.c:77.
pub const fn kmess_start(next: i32, size: i32) -> i32 {
    ((next + KMESS_BUF_SIZE) - size) % KMESS_BUF_SIZE
}

// ── monparams expansion ───────────────────────────────────────────

/// Expands NUL-separated monitor strings to newline-separated.
///
/// C: the `do { *e++ = '\n'; } while (*e != 0)` loop — dmp_kernel.c:107-111
/// (rewrites each string's terminator in place, stopping at the double
/// NUL). Returns bytes written; truncates silently if `out` fills
/// (C would overrun `val` — the truncation is a deliberate hardening,
/// documented here instead of transliterated).
pub fn expand_newlines(input: &[u8], out: &mut [u8]) -> usize {
    let mut written = 0;
    let mut i = 0;
    while i < input.len() {
        if written >= out.len() {
            break;
        }
        let b = input[i];
        if b == 0 {
            // End of one string: emit newline unless the next byte also
            // ends the list (double NUL = C `while (*e != 0)` stop).
            if i + 1 < input.len() && input[i + 1] != 0 {
                out[written] = b'\n';
                written += 1;
            }
            i += 1;
            // Skip the consumed NUL without emitting (C overwrote it).
            continue;
        }
        out[written] = b;
        written += 1;
        i += 1;
    }
    written
}

// ── proc_name classification ──────────────────────────────────────

/// `proc_name` outcome classes.
///
/// C: `proc_name(nr)` — dmp_kernel.c:385-395 (`ANY→"ANY"`, `NONE→"NONE"`,
/// out-of-range→`"BOGUS"`, empty→`"EMPTY"`, else the table name).
/// `ANY`/`NONE` are endpoints (`endpoint.h:54-55`); `NR_TASKS=5`
/// (com.h:56), `NR_PROCS=256` (sys_config.h:8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameClass {
    Any,
    None,
    Bogus,
    Empty,
    Named,
}

/// Classifies a slot number without touching the table.
pub const fn classify_proc_name(nr: i32, is_empty: bool) -> NameClass {
    if nr == Endpoint::ANY.get() {
        NameClass::Any
    } else if nr == Endpoint::NONE.get() {
        NameClass::None
    } else if nr < -5 || nr >= 256 {
        NameClass::Bogus
    } else if is_empty {
        NameClass::Empty
    } else {
        NameClass::Named
    }
}

// ── format constants (verbatim) ───────────────────────────────────

/// C: `printf("Dump of all messages generated by the kernel.\n\n")` — :86.
pub const KMESSAGES_TITLE: &str = "Dump of all messages generated by the kernel.\n\n";
/// C: `printf("Dump of kernel environment strings set by boot monitor.\n")` — :114.
pub const MONPARAMS_TITLE: &str = "Dump of kernel environment strings set by boot monitor.\n";
/// C: irqtab header lines — :145-146.
pub const IRQTAB_TITLE: &str = "IRQ policies dump shows use of kernel's IRQ hooks.\n";
/// C: `:146`.
pub const IRQTAB_COLUMNS: &str = "-h.id- -proc.nr- -irq nr- -policy- -notify id- -masked-\n";
/// C: `"<unused>"` row — :151.
pub const IRQ_UNUSED: &str = "<unused>";
/// C: `IRQ_REENABLE 0x001` policy word — com.h:308.
pub const IRQ_REENABLE: u32 = 0x001;
/// C: image header lines — :178-179.
pub const IMAGE_TITLE: &str = "Image table dump showing all processes included in system image.\n";
/// C: `:179` (note: rows print name+nr only — dmp_kernel.c:182).
pub const IMAGE_COLUMNS: &str = "---name- -nr- flags -stack-\n";
/// C: kenv title lines — :206-207.
pub const KENV_TITLE: &str = "Dump of kinfo structure.\n\nKernel info structure:\n";
/// C: privileges header — :270-271.
pub const PRIV_COLUMNS: &str =
    "-nr- -id- -name-- -flags- traps grants -ipc_to--          -kernel calls-\n";
/// C: proctab header — :333.
pub const PROCTAB_COLUMNS: &str =
    "\n-nr-----gen---endpoint-name--- -prior-quant- -user----sys-rtsflags-from/to-\n";
/// C: procstack header — :373.
pub const PROCSTACK_COLUMNS: &str = "\n-nr-rts flags--      --stack--\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s_flags_full_and_zero() {
        // C: dmp_kernel.c:218-231.
        assert_eq!(&s_flags_str(0), b"-------\x00");
        assert_eq!(&s_flags_str(0x0FF), b"PBDSIQM\x00");
        assert_eq!(&s_flags_str(0x010), b"---S---\x00");
    }

    #[test]
    fn test_s_traps_full_and_zero() {
        // C: dmp_kernel.c:236-247; ipcconst.h:7-16. Note `1 << SENDA`
        // (bit 16) only fires when the promoted short is negative —
        // C promotes `short` to `int` value-preserving, as does `as u32`.
        assert_eq!(&s_traps_str(0), b"-----\x00");
        assert_eq!(&s_traps_str(0x1E), b"S-RBN\x00");
        assert_eq!(&s_traps_str(-1), b"SARBN\x00");
    }

    #[test]
    fn test_p_rts_flags_full_and_zero() {
        // C: dmp_kernel.c:300-313; proc.h:143-149.
        assert_eq!(&p_rts_flags_str(0), b"-------\x00");
        assert_eq!(&p_rts_flags_str(0xFE), b"sSRIPTp\x00");
    }

    #[test]
    fn test_cursor_counts_21_then_more() {
        // C: PROCLOOP — dmp_kernel.c:32-40 (LINES=22).
        let mut c = PageCursor::new();
        for _ in 0..21 {
            assert!(matches!(c.push_row(0, 3), PageAction::Emit(_)));
        }
        assert_eq!(c.push_row(0, 3), PageAction::More);
    }

    #[test]
    fn test_cursor_skips_empties_without_counting() {
        let mut c = PageCursor::new();
        for _ in 0..100 {
            assert_eq!(c.push_row(RTS_SLOT_FREE, 3), PageAction::Skip);
        }
        // Counter untouched: still 21 emits before More.
        for _ in 0..21 {
            assert!(matches!(c.push_row(0, 3), PageAction::Emit(_)));
        }
        assert_eq!(c.push_row(0, 3), PageAction::More);
    }

    #[test]
    fn test_cursors_are_independent() {
        // C: three separate `static oldrp` (dmp_kernel.c:256/324/364).
        let mut a = PageCursor::new();
        let mut b = PageCursor::new();
        for _ in 0..21 {
            let _ = a.push_row(0, 3);
        }
        assert_eq!(a.push_row(0, 3), PageAction::More);
        assert!(matches!(b.push_row(0, 3), PageAction::Emit(_)));
    }

    #[test]
    fn test_row_head_three_forms() {
        // C: dmp_kernel.c:38-40 (IDLE=(endpoint_t)-4, com.h:48).
        assert_eq!(row_head(-4), RowHead::Idle);
        assert_eq!(row_head(-2), RowHead::Task);
        assert_eq!(row_head(0), RowHead::User);
        assert_eq!(row_head(11), RowHead::User);
    }

    #[test]
    fn test_kmess_start_wraps() {
        // C: dmp_kernel.c:77 (SIZE=10000).
        assert_eq!(kmess_start(100, 100), 0);
        assert_eq!(kmess_start(50, 100), 9950);
        assert_eq!(kmess_start(0, 0), 0);
    }

    #[test]
    fn test_expand_newlines() {
        // C: dmp_kernel.c:107-111 ("a\0b\0\0" → "a\nb").
        let mut out = [0u8; 16];
        let n = expand_newlines(b"a\x00b\x00\x00", &mut out);
        assert_eq!(&out[..n], b"a\nb");
        let n = expand_newlines(b"", &mut out);
        assert_eq!(n, 0);
        // Truncation instead of C's overrun (hardening, documented).
        let mut tiny = [0u8; 2];
        let n = expand_newlines(b"abcd\x00", &mut tiny);
        assert_eq!((n, &tiny[..]), (2, &b"ab"[..]));
    }

    #[test]
    fn test_name_classification_matrix() {
        // C: dmp_kernel.c:385-395 (ANY/NONE endpoints, NR_TASKS=5, NR_PROCS=256).
        assert_eq!(classify_proc_name(Endpoint::ANY.get(), false), NameClass::Any);
        assert_eq!(classify_proc_name(Endpoint::NONE.get(), false), NameClass::None);
        assert_eq!(classify_proc_name(-6, false), NameClass::Bogus);
        assert_eq!(classify_proc_name(256, false), NameClass::Bogus);
        assert_eq!(classify_proc_name(-5, false), NameClass::Named);
        assert_eq!(classify_proc_name(255, false), NameClass::Named);
        assert_eq!(classify_proc_name(3, true), NameClass::Empty);
        assert_eq!(classify_proc_name(3, false), NameClass::Named);
    }

    #[test]
    fn test_format_strings_verbatim() {
        assert!(PROCTAB_COLUMNS.starts_with("\n-nr-----gen---endpoint-name---"));
        assert!(PRIV_COLUMNS.starts_with("-nr- -id- -name--"));
        assert_eq!(MORE_MARKER, "--more--\n");
        assert_eq!(IRQ_UNUSED, "<unused>");
        assert_eq!(IRQ_REENABLE, 0x001);
        assert_eq!(KMESS_BUF_SIZE, 10000);
    }

    #[test]
    fn test_snapshots_default_zeroed() {
        let p = KProcSnap::default();
        assert_eq!((p.p_rts_flags, p.p_nr, p.p_endpoint), (0, 0, 0));
        assert_eq!(p.p_name.len(), 16);
        let k = KinfoSnap::default();
        assert_eq!(k.release.len(), 6);
        let b = BootImageSnap::default();
        assert_eq!(b.proc_name.len(), 16);
    }
}
