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

use crate::acquire::DiagctlTransport;
use crate::PCStr;
use core::fmt;
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
    /// C: `s_ipc_to` (priv.h:39) — `NR_SYS_PROCS` (64) bits = 2 words;
    /// privileges_dmp prints each word `%08x` (dmp_kernel.c:284-286).
    /// (V1-P1-3: added — the dump prints these columns, the old snapshot
    /// predated the execution face.)
    pub s_ipc_to: [u32; 2],
    /// C: `s_k_call_mask` (priv.h:47) — `NR_SYS_CALLS` (58) bits = 2 words,
    /// printed `%08x` per `BITCHUNK_BITS` chunk (dmp_kernel.c:289-291).
    pub s_k_call_mask: [u32; 2],
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

    /// Counts a line the cursor did not emit: procstack's `pagelines++`
    /// after the row (dmp_kernel.c:377 — the stack trace below it occupies
    /// screen lines the counter cannot see; no break check here, the next
    /// `push_row` trips `>= LINES`).
    pub const fn count_extra(&mut self) {
        self.lines += 1;
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
    // C (dmp_kernel.c:107-111): `do { e += strlen(e); *e++ = '\n'; }
    // while (*e != 0)` — every string-terminating NUL is overwritten with
    // '\n' (including the last string's), and the loop stops when the byte
    // AFTER a written newline is NUL again (the list terminator).
    let mut written = 0;
    let mut i = 0;
    loop {
        while i < input.len() && input[i] != 0 {
            if written >= out.len() {
                return written; // deliberate hardening: C would overrun
            }
            out[written] = input[i];
            written += 1;
            i += 1;
        }
        if i >= input.len() {
            break; // input ended without a terminator NUL
        }
        if written >= out.len() {
            return written; // hardening, as above
        }
        out[written] = b'\n';
        written += 1;
        i += 1;
        if i >= input.len() || input[i] == 0 {
            break; // C `while (*e != 0)`: double NUL ends the list
        }
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

// ── table lengths (fetch capacities = the C tables) ───────────────

/// C: `NR_TASKS 5` — com.h:56.
pub const NR_TASKS: i32 = 5;
/// C: `NR_PROCS 256` — sys_config.h:8.
pub const NR_PROCS: i32 = 256;
/// C: `proc[NR_TASKS + NR_PROCS]` — dmp_kernel.c:55 (fetch capacity).
pub const PROC_TABLE_LEN: usize = (NR_TASKS + NR_PROCS) as usize;
/// C: `NR_BOOT_PROCS (NR_TASKS + LAST_SPECIAL_PROC_NR + 1)` = 5+11+1 —
/// param.h:9, com.h:70.
pub const NR_BOOT_PROCS: usize = 17;
/// C: `NR_SYS_PROCS 64` — sys_config.h:9.
pub const NR_SYS_PROCS: usize = 64;
/// C: `NR_SYS_CALLS 58` — com.h:270.
pub const NR_SYS_CALLS: i32 = 58;
/// C: `BITCHUNK_BITS (sizeof(bitchunk_t) * CHAR_BIT)` = 32 — bitmap.h:12.
pub const PRIV_CHUNK_BITS: u32 = 32;
/// C: `USER_PRIV_ID = static_priv_id(ROOT_USR_PROC_NR)` = 5 + 11 —
/// priv.h:12,18 + com.h:72; the privileges fallback row (dmp_kernel.c:278).
pub const USER_PRIV_ID: usize = 16;
/// C: `NR_IRQ_HOOKS 16` (x86 config) — kernel/config.h:59.
pub const NR_IRQ_HOOKS: usize = 16;
/// C: `MULTIBOOT_PARAM_BUF_SIZE 1024` — multiboot.h:240.
pub const MULTIBOOT_PARAM_BUF: usize = 1024;
/// C: `RTS_SENDING 0x04` — proc.h:145 (PRINTRTS from/to selector).
pub const RTS_SENDING: u32 = 0x04;
/// C: `RTS_RECEIVING 0x08` — proc.h:146.
pub const RTS_RECEIVING: u32 = 0x08;

/// One non-empty PROCLOOP row: emits the row head and hands the row over.
///
/// C: the three-way head — `(%2d) ` idle / `[%2d] ` task / ` %2d  ` user
/// (dmp_kernel.c:38-40) — plus the `--more--` breakpoint (the caller's
/// loop breaks on [`PageAction::More`]).
fn write_row_head(out: &mut dyn fmt::Write, p: &KProcSnap) -> fmt::Result {
    match row_head(p.p_nr) {
        RowHead::Idle => write!(out, "({:2}) ", p.p_nr),
        RowHead::Task => write!(out, "[{:2}] ", p.p_nr),
        RowHead::User => write!(out, " {:2}  ", p.p_nr),
    }
}

/// PRINTRTS (dmp_kernel.c:20-28): `" %s"` flag code + `" %-7.7s"` peer
/// name — SENDING prints the send-to peer, RECEIVING the get-from peer,
/// anything else an empty name resolved as `%-7.7s` of `""`.
fn write_printrts(out: &mut dyn fmt::Write, tab: &[KProcSnap], p: &KProcSnap) -> fmt::Result {
    write!(out, " {} ", PCStr(&p_rts_flags_str(p.p_rts_flags)))?;
    let peer = if p.p_rts_flags & RTS_SENDING != 0 {
        Some(p.p_sendto_e)
    } else if p.p_rts_flags & RTS_RECEIVING != 0 {
        Some(p.p_getfrom_e)
    } else {
        None
    };
    match peer {
        Some(ep) => write_peer_name(out, tab, Endpoint(ep).slot()),
        None => write!(out, "{:<7.7}", ""),
    }
}

/// `proc_name(nr)` (dmp_kernel.c:385-395) against a fetched table copy:
/// ANY/NONE/BOGUS are literals, an empty slot prints `EMPTY`, else the
/// row's `p_name` — through [`PCStr`] with the caller's format spec.
fn write_peer_name(out: &mut dyn fmt::Write, tab: &[KProcSnap], slot: i32) -> fmt::Result {
    let idx = (slot + NR_TASKS) as usize;
    let is_empty = match tab.get(idx) {
        Some(p) => p.p_rts_flags == RTS_SLOT_FREE,
        None => true,
    };
    match classify_proc_name(slot, is_empty) {
        NameClass::Any => write!(out, "{:<7.7}", "ANY"),
        NameClass::None => write!(out, "{:<7.7}", "NONE"),
        NameClass::Bogus => write!(out, "{:<7.7}", "BOGUS"),
        NameClass::Empty => write!(out, "{:<7.7}", "EMPTY"),
        NameClass::Named => {
            let name = tab[idx].p_name;
            write!(out, "{:<7.7}", PCStr(&name))
        }
    }
}

/// `proctab_dmp` (dmp_kernel.c:318-346, i386 arm; A-7 excludes the arm
/// twin). Header + PROCLOOP rows: gen/endpoint, name, priority, quantum,
/// user/sys times, then PRINTRTS.
pub fn render_proctab(
    out: &mut dyn fmt::Write,
    tab: &[KProcSnap],
    cur: &mut PageCursor,
) -> fmt::Result {
    out.write_str(PROCTAB_COLUMNS)?;
    for p in tab {
        match cur.push_row(p.p_rts_flags, p.p_nr) {
            PageAction::Skip => continue,
            PageAction::More => {
                out.write_str(MORE_MARKER)?;
                break;
            }
            PageAction::Emit(_) => {}
        }
        let generation = Endpoint(p.p_endpoint).generation();
        write_row_head(out, p)?;
        write!(out, " {:5} {:10} ", generation, p.p_endpoint)?;
        // C prints unsigned (%5u/%6u): the i8/i32 snapshot fields render
        // value-preserving promoted, then reinterpreted — wraparound kept.
        write!(
            out,
            "{:<8.8} {:5} {:5} {:6} {:6} ",
            PCStr(&p.p_name),
            (p.p_priority as i32) as u32,
            p.p_quantum_size_ms,
            p.p_user_time as u32,
            p.p_sys_time as u32
        )?;
        write_printrts(out, tab, p)?;
        out.write_str("\n")?;
    }
    Ok(())
}

/// `procstack_dmp` (dmp_kernel.c:358-380): proctab's loop with PRINTRTS +
/// a blank line + `pagelines++` + a stack-trace request per row.
pub fn render_procstack(
    out: &mut dyn fmt::Write,
    tab: &[KProcSnap],
    cur: &mut PageCursor,
    diag: &mut dyn DiagctlTransport,
) -> fmt::Result {
    out.write_str(PROCSTACK_COLUMNS)?;
    for p in tab {
        match cur.push_row(p.p_rts_flags, p.p_nr) {
            PageAction::Skip => continue,
            PageAction::More => {
                out.write_str(MORE_MARKER)?;
                break;
            }
            PageAction::Emit(_) => {}
        }
        write_row_head(out, p)?;
        write_printrts(out, tab, p)?;
        out.write_str("\n")?;
        cur.count_extra();
        let _ = diag.stacktrace(Endpoint(p.p_endpoint));
    }
    Ok(())
}

/// `privileges_dmp` (dmp_kernel.c:252-295): per row, a linear priv match
/// on `s_proc_nr` with the `USER_PRIV_ID` fallback, then the flag/trap
/// codes, grant count and the `%08x` bitmap chunks.
pub fn render_privileges(
    out: &mut dyn fmt::Write,
    privs: &[KPrivSnap],
    tab: &[KProcSnap],
    cur: &mut PageCursor,
) -> fmt::Result {
    debug_assert!(privs.len() > USER_PRIV_ID, "fallback row must exist");
    out.write_str(PRIV_COLUMNS)?;
    for p in tab {
        match cur.push_row(p.p_rts_flags, p.p_nr) {
            PageAction::Skip => continue,
            PageAction::More => {
                out.write_str(MORE_MARKER)?;
                break;
            }
            PageAction::Emit(_) => {}
        }
        let sp = privs
            .iter()
            .find(|s| s.s_proc_nr == p.p_nr)
            .unwrap_or(&privs[USER_PRIV_ID]);
        write!(
            out,
            "({:02}) {:<7.7} {} {} {:6}",
            sp.s_id as u32,
            PCStr(&p.p_name),
            PCStr(&s_flags_str(sp.s_flags)),
            PCStr(&s_traps_str(sp.s_trap_mask)),
            sp.s_grant_entries
        )?;
        for w in sp.s_ipc_to {
            write!(out, " {:08x}", w)?;
        }
        out.write_str(" ")?;
        for w in sp.s_k_call_mask {
            write!(out, " {:08x}", w)?;
        }
        out.write_str("\n")?;
    }
    Ok(())
}

/// `image_dmp` (dmp_kernel.c:168-185): the header promises four columns,
/// the rows print two (`%8s %4d`) — the header's lie is kept verbatim
/// (05 §2.5).
pub fn render_image(out: &mut dyn fmt::Write, image: &[BootImageSnap]) -> fmt::Result {
    out.write_str(IMAGE_TITLE)?;
    out.write_str(IMAGE_COLUMNS)?;
    for ip in image {
        writeln!(out, "{:>8} {:4}", PCStr(&ip.proc_name), ip.proc_nr)?;
    }
    out.write_str("\n")
}

/// `irqtab_dmp` (dmp_kernel.c:121-163, `#if 0` actids block excluded).
/// `masked` is decided by `actids[irq] & id`.
pub fn render_irqtab(
    out: &mut dyn fmt::Write,
    hooks: &[IrqHookSnap],
    actids: &[i32],
) -> fmt::Result {
    out.write_str(IRQTAB_TITLE)?;
    out.write_str(IRQTAB_COLUMNS)?;
    for (i, e) in hooks.iter().enumerate() {
        write!(out, "{:3}", i as i32)?;
        if e.proc_nr_e == Endpoint::NONE.get() {
            out.write_str("    <unused>\n")?;
            continue;
        }
        write!(out, "{:10}  ", e.proc_nr_e)?;
        write!(out, "    ({:02}) ", e.irq)?;
        if e.policy & IRQ_REENABLE != 0 {
            out.write_str("  reenable")?;
        } else {
            out.write_str("      -   ")?;
        }
        write!(out, "   {:4}", e.notify_id)?;
        if e.irq >= 0
            && (e.irq as usize) < actids.len()
            && (actids[e.irq as usize] & e.id) != 0
        {
            out.write_str("       masked")?;
        }
        out.write_str("\n")?;
    }
    out.write_str("\n")
}

/// `kmessages_dmp` (dmp_kernel.c:62-88): rotate the ring copy into print
/// order via [`kmess_start`], then stream it raw (C builds `print_buf`
/// first — same bytes, one write fewer). `km_size` is clamped to the
/// buffer (deliberate hardening: C trusts the kernel copy).
pub fn render_kmessages(
    out: &mut dyn fmt::Write,
    meta: &KmessagesSnap,
    ring: &[u8],
) -> fmt::Result {
    out.write_str(KMESSAGES_TITLE)?;
    let start = kmess_start(meta.km_next, meta.km_size);
    let size = meta.km_size.clamp(0, ring.len() as i32) as usize;
    for i in 0..size {
        let idx = ((start + i as i32) % KMESS_BUF_SIZE) as usize;
        out.write_char(ring[idx] as char)?;
    }
    Ok(())
}

/// `monparams_dmp` (dmp_kernel.c:93-116): expand NUL separators to
/// newlines (see [`expand_newlines`]) then print `\n%s\n`.
pub fn render_monparams(out: &mut dyn fmt::Write, blob: &[u8]) -> fmt::Result {
    let mut expanded = [0u8; MULTIBOOT_PARAM_BUF];
    let n = expand_newlines(blob, &mut expanded);
    out.write_str(MONPARAMS_TITLE)?;
    out.write_str("\n")?;
    for b in &expanded[..n] {
        out.write_char(*b as char)?;
    }
    out.write_str("\n")
}

/// `kenv_dmp` (dmp_kernel.c:191-213): kinfo's four printed fields only —
/// C fetches a machine struct too but never reads it (05 §2.6 exclusion;
/// structurally absent here, so its failure branch has no counterpart).
pub fn render_kenv(out: &mut dyn fmt::Write, kinfo: &KinfoSnap) -> fmt::Result {
    out.write_str(KENV_TITLE)?;
    writeln!(out, "- nr_procs:     {:3}", kinfo.nr_procs as u32)?;
    writeln!(out, "- nr_tasks:     {:3}", kinfo.nr_tasks as u32)?;
    writeln!(out, "- release:      {:.6}", PCStr(&kinfo.release))?;
    writeln!(out, "- version:      {:.6}", PCStr(&kinfo.version))?;
    out.write_str("\n")
}

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
        // C: dmp_kernel.c:107-111 — every string NUL becomes '\n'
        // (including the last string's); the loop stops when the byte after
        // a written newline is NUL again. "a\0b\0\0" → "a\nb\n".
        let mut out = [0u8; 16];
        let n = expand_newlines(b"a\x00b\x00\x00", &mut out);
        assert_eq!(&out[..n], b"a\nb\n");
        // Single string: its terminator becomes the trailing newline.
        let n = expand_newlines(b"ab\x00", &mut out);
        assert_eq!(&out[..n], b"ab\n");
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

    fn snap(slot: i32, rts: u32, name: &[u8]) -> KProcSnap {
        let mut n = [0u8; 16];
        n[..name.len()].copy_from_slice(name);
        KProcSnap {
            p_rts_flags: rts,
            p_getfrom_e: 0,
            p_sendto_e: 0,
            p_name: n,
            p_priority: 3,
            p_quantum_size_ms: 100,
            p_user_time: 7,
            p_sys_time: 2,
            p_endpoint: slot,
            p_nr: slot,
        }
    }

    /// One PROCLOOP row, assembled segment-by-segment: each piece is one
    /// C printf (dmp_kernel.c:38-40/336-343/20-28), so the expected string
    /// pins segment order and widths without re-running the impl.
    fn proctab_row(head: &str, nr: i32, name: &str, flags: &str, peer: Option<&str>) -> String {
        let mut s = String::from(head);
        s.push_str(&format!(" {:5} {:10} ", 0, nr));
        s.push_str(&format!("{:<8.8}", name));
        s.push_str(&format!(" {:5} {:5} {:6} {:6} ", 3u32, 100u32, 7u32, 2u32));
        s.push_str(&format!(" {} ", flags));
        s.push_str(&format!("{:<7.7}", peer.unwrap_or("")));
        s.push('\n');
        s
    }

    #[test]
    fn test_render_proctab_rows_heads_and_pagination() {
        // C: dmp_kernel.c:333-344. Row heads (%2d three forms), %5d/%10d
        // gen+endpoint, %-8.8s name, %5u/%5u/%6u/%6u, PRINTRTS with no
        // peer (empty %-7.7s), empty slots skipped, 22 rows per screen.
        let tab = [
            snap(-5, RTS_SLOT_FREE, b""),           // skipped, uncounted
            snap(-4, 0, b""),                        // IDLE head
            snap(-3, 0, b"task"),                    // task head
            snap(-2, RTS_SLOT_FREE, b""),           // skipped
            snap(-1, 0, b"rs"),                      // task head
            snap(0, RTS_SLOT_FREE, b"pm"),          // skipped
            snap(1, 0, b"init"),                     // user head
        ];
        let mut out = String::new();
        let mut cur = PageCursor::new();
        render_proctab(&mut out, &tab, &mut cur).unwrap();
        let expected = PROCTAB_COLUMNS.to_string()
            + &proctab_row("(-4) ", -4, "", "-------", None)
            + &proctab_row("[-3] ", -3, "task", "-------", None)
            + &proctab_row("[-1] ", -1, "rs", "-------", None)
            + &proctab_row("  1  ", 1, "init", "-------", None);
        assert_eq!(out, expected);
        assert_eq!(
            cur.push_row(0, 3),
            PageAction::Emit(RowHead::User),
            "4 rows so far: still below LINES"
        );
    }

    #[test]
    fn test_render_proctab_printrts_peer_resolution() {
        // C: PRINTRTS (dmp_kernel.c:20-28) — SENDING prints the send-to
        // peer name; the empty peer prints %-7.7s of "".
        // Table slices are absolutely indexed (slot + NR_TASKS), like the
        // C copy — build an 8-slot window with PM at slot 0, RS at slot 2.
        let mut tab = [snap(-5, RTS_SLOT_FREE, b""); 8];
        tab[5] = snap(0, 0, b"pm");
        let mut p = snap(2, 0, b"rs");
        p.p_rts_flags = RTS_SENDING;
        p.p_sendto_e = 0; // raw endpoint 0 → slot 0 (PM)
        tab[7] = p;
        let mut out = String::new();
        let mut cur = PageCursor::new();
        render_proctab(&mut out, &tab, &mut cur).unwrap();
        let expected = PROCTAB_COLUMNS.to_string()
            + &proctab_row("  0  ", 0, "pm", "-------", None)
            + &proctab_row("  2  ", 2, "rs", "-S-----", Some("pm"));
        assert_eq!(out, expected);
    }

    #[test]
    fn test_render_proctab_more_breakpoint_persists() {
        // C: PROCLOOP's static oldrp — after --more--, the next press
        // resumes after the breakpoint row.
        let mut tab = [KProcSnap::default(); 261];
        let mut filled = 0usize;
        for (i, slot) in (-4..).enumerate().take(261) {
            let mut row = snap(slot, 0, b"p");
            if slot == 2 {
                row.p_rts_flags = RTS_SLOT_FREE; // one skipped row for realism
            } else {
                filled += 1;
            }
            tab[i] = row;
        }
        let mut out = String::new();
        let mut cur = PageCursor::new();
        render_proctab(&mut out, &tab, &mut cur).unwrap();
        assert!(out.ends_with("--more--\n"), "first screen ends with the marker");
        let first_rows = filled.min(21);
        assert_eq!(out.matches('\n').count(), first_rows + 3, "blank + header + 21 rows + marker");

        let mut out2 = String::new();
        render_proctab(&mut out2, &tab, &mut cur).unwrap();
        assert!(out2.ends_with("--more--\n"));
        assert_ne!(out, out2, "second screen resumes past the breakpoint");
    }

    #[test]
    fn test_render_privileges_match_fallback_and_chunks() {
        // C: dmp_kernel.c:270-292 — matched priv row + the USER_PRIV_ID
        // fallback, bitmap words as %08x.
        let mut privs = [KPrivSnap::default(); NR_SYS_PROCS];
        privs[0] = KPrivSnap {
            s_proc_nr: 1,
            s_id: 7,
            s_flags: -1, // all bits → PBDSIQM
            s_trap_mask: 0x1E,
            s_grant_entries: 42,
            s_ipc_to: [0xdeadbeef, 0x12345678],
            s_k_call_mask: [0xffffffff, 0x0000ffff],
        };
        let tab = [snap(-5, RTS_SLOT_FREE, b""), snap(1, 0, b"vfs"), snap(2, 0, b"who")];
        let mut out = String::new();
        let mut cur = PageCursor::new();
        render_privileges(&mut out, &privs, &tab, &mut cur).unwrap();
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines[1], "(07) vfs     PBDSIQM S-RBN     42 deadbeef 12345678  ffffffff 0000ffff");
        // Row 2 matches no priv → the USER_PRIV_ID fallback row (all zero).
        assert_eq!(lines[2], "(00) who     ------- -----      0 00000000 00000000  00000000 00000000");
    }

    #[test]
    fn test_render_image_rows() {
        // C: dmp_kernel.c:178-184 — %8s %4d rows, header promises four
        // columns and lies (kept verbatim).
        let image = [BootImageSnap { proc_nr: -4, proc_name: name8(b"idle") }];
        let mut out = String::new();
        render_image(&mut out, &image).unwrap();
        assert_eq!(
            out,
            format!("{}{}    idle   -4\n\n", IMAGE_TITLE, IMAGE_COLUMNS)
        );
    }

    #[test]
    fn test_render_irqtab_unused_and_masked() {
        // C: dmp_kernel.c:145-162 — <unused> rows, policy word, masked.
        let hooks = [
            IrqHookSnap { proc_nr_e: Endpoint::NONE.get(), irq: 0, policy: 0, notify_id: 0, id: 0 },
            IrqHookSnap { proc_nr_e: 3, irq: 1, policy: IRQ_REENABLE, notify_id: 9, id: 0x10 },
        ];
        let actids = [0i32, 0x30]; // irq 1 masked by 0x10
        let mut out = String::new();
        render_irqtab(&mut out, &hooks, &actids).unwrap();
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines[2], "  0    <unused>");
        assert_eq!(lines[3], "  1         3      (01)   reenable      9       masked");
        assert_eq!(lines[4], "");
        assert_eq!(lines[5], "");
    }

    #[test]
    fn test_render_kenv_fields() {
        // C: dmp_kernel.c:206-212.
        let kinfo = KinfoSnap {
            nr_procs: 256,
            nr_tasks: 5,
            release: *b"7.99.2",
            version: *b"r12345",
        };
        let mut out = String::new();
        render_kenv(&mut out, &kinfo).unwrap();
        assert_eq!(
            out,
            "Dump of kinfo structure.\n\nKernel info structure:\n".to_string()
                + "- nr_procs:     256\n"
                + "- nr_tasks:       5\n"
                + "- release:      7.99.2\n"
                + "- version:      r12345\n"
                + "\n"
        );
    }

    #[test]
    fn test_render_kmessages_rotation() {
        // C: dmp_kernel.c:77-87 — start = ((next + SIZE) - size) % SIZE,
        // then the ring prints in logical order.
        let mut ring = [0u8; KMESS_BUF_SIZE as usize];
        ring[9998] = b'o';
        ring[9999] = b'k';
        ring[0] = b'!';
        let meta = KmessagesSnap { km_next: 1, km_size: 3 };
        let mut out = String::new();
        render_kmessages(&mut out, &meta, &ring).unwrap();
        assert!(out.starts_with("Dump of all messages generated by the kernel.\n\n"));
        assert!(out.ends_with("ok!"), "wrap-around prints in logical order");
    }

    #[test]
    fn test_render_monparams_matches_c_expansion() {
        // C: dmp_kernel.c:114-115 — "\n%s\n" over the expanded blob
        // (every string NUL becomes a newline, last one included).
        let mut out = String::new();
        render_monparams(&mut out, b"a\x00b\x00\x00").unwrap();
        assert_eq!(out, "Dump of kernel environment strings set by boot monitor.\n\na\nb\n\n");
    }

    #[test]
    fn debug_printrts_actual() {
        let mut tab = [snap(-5, RTS_SLOT_FREE, b""); 8];
        tab[5] = snap(0, 0, b"pm");
        let mut p = snap(2, 0, b"rs");
        p.p_rts_flags = RTS_SENDING;
        p.p_sendto_e = 0;
        tab[7] = p;
        let mut out = String::new();
        let mut cur = PageCursor::new();
        render_proctab(&mut out, &tab, &mut cur).unwrap();
        eprintln!("ACTUAL: {:?}", out);
    }

    #[test]
    fn test_pcstr_format_specs() {
        // C %s family: precision truncates, width pads, `-` left-aligns,
        // default is right-aligned like C's %s.
        let name = b"vfs\x00tail";
        assert_eq!(format!("{:<8.8}", PCStr(&name[..])), "vfs     ");
        assert_eq!(format!("{:>8.3}", PCStr(&name[..])), "     vfs");
        assert_eq!(format!("{:6}", PCStr(b"ab\x00")), "    ab");
    }

    fn name8(name: &[u8]) -> [u8; 16] {
        let mut n = [0u8; 16];
        n[..name.len()].copy_from_slice(name);
        n
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

