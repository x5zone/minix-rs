//! Row views over the pulled process tables.
//!
//! C: the static arrays `proc_tab`/`mproc_tab`/`fproc_tab` of `proc.c:18-20`,
//! read in place by `fill_wmesg`/`get_lwp_stat`/`fill_lwp_common`. Here the
//! tables arrive as opaque byte buffers ([`crate::proc::Tables`]); this
//! module turns slots into the typed rows the fill halves judge on. The row
//! *layouts* are the producers' contracts — kernel
//! [`minix_types::ProcInfoStruct`], PM [`minix_types::MProcSnap`] — read by
//! name, never by hand-picked offsets. The VFS light rows are still
//! fail-closed on the producer side (the A-7/C-21 defer), so the light view
//! answers "idle" until that wire authority lands.
//!
//! 16-mib-proc-tables.md + 17-mib-proc-lwp.md.

use alloc::vec;
use alloc::string::ToString;
use alloc::vec::Vec;
use minix_types::{mp_flags, Endpoint, MProcSnap, ProcInfoStruct};

use super::tables::{LIGHT_ROW, PM_ROW, KERN_ROW};

/// Tasks occupy the slots *before* the process slots. C: `proc_tab[NR_TASKS
/// + mslot]` — proc.c:225; `NR_TASKS` tasks then `NR_PROCS` processes.
pub const NR_TASKS: usize = minix_types::NR_TASKS;

/// A row view over the pulled kernel table.
///
/// C: `proc_tab[kslot]` with the `PMAGIC` check at pull time (proc.c:81-87;
/// the Rust pull trusts the shared layout instead — mproc.rs' header note).
pub struct KernelRows<'a> {
    bytes: &'a [u8],
}

impl<'a> KernelRows<'a> {
    /// View a kernel table. Empty (or a partial tail row) answers `None`
    /// per slot, like C reading a short table would be refused upstream.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Row count the buffer holds. C: `NR_TASKS + NR_PROCS` — proc.c:18.
    pub fn len(&self) -> usize {
        self.bytes.len() / KERN_ROW
    }

    /// Whether the buffer holds no rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Decode one row. C: `&proc_tab[kslot]`.
    pub fn row(&self, kslot: usize) -> Option<ProcInfoStruct> {
        if kslot >= self.len() {
            return None;
        }
        let off = kslot * KERN_ROW;
        // SAFETY: `ProcInfoStruct` is `#[repr(C)]` Copy POD; the byte window
        // is exactly `KERN_ROW` long and in bounds (checked above).
        let mut row: ProcInfoStruct =
            unsafe { core::ptr::read_unaligned(self.bytes.as_ptr().add(off) as *const _) };
        // Rows shorter than the real `p_name` keep the buffer's NULs; the
        // name helper terminates regardless. Normalize the row number to the
        // slot (the kernel fills it, but a zeroed tail row must not mint a
        // process 0 name lookup).
        if row.p_name.iter().all(|b| *b == 0) && row.p_endpoint == 0 && row.p_rts_flags == 0 {
            row.p_nr = -1;
        }
        Some(row)
    }

    /// `proc_is_runnable` — `p_rts_flags == 0` (proc.h:170).
    pub fn is_runnable(row: &ProcInfoStruct) -> bool {
        row.p_rts_flags == 0
    }

    /// `P_BLOCKEDON` — the macro at kernel/proc.h:187-198 ported:
    /// SENDING wins, then RECEIVING, else `NONE`.
    pub fn blocked_on(row: &ProcInfoStruct) -> i32 {
        if row.p_rts_flags & minix_types::RTS_SENDING != 0 {
            row.p_sendto_e
        } else if row.p_rts_flags & minix_types::RTS_RECEIVING != 0 {
            row.p_getfrom_e
        } else {
            Endpoint::NONE.0
        }
    }

    /// Whether the row is stopped for tracing (`RTS_P_STOP`). C:
    /// `RTS_ISSET(kp, RTS_P_STOP)` — proc.c:250.
    pub fn is_p_stopped(row: &ProcInfoStruct) -> bool {
        row.p_rts_flags & minix_types::RTS_P_STOP != 0
    }
}

/// A row view over the pulled PM table.
pub struct PmRows<'a> {
    bytes: &'a [u8],
}

impl<'a> PmRows<'a> {
    /// View a PM table.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Row count the buffer holds. C: `NR_PROCS` — proc.c:19.
    pub fn len(&self) -> usize {
        self.bytes.len() / PM_ROW
    }

    /// Whether the buffer holds no rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Decode one row. C: `&mproc_tab[mslot]`.
    pub fn row(&self, mslot: usize) -> Option<MProcSnap> {
        if mslot >= self.len() {
            return None;
        }
        let off = mslot * PM_ROW;
        // SAFETY: `MProcSnap` is `#[repr(C)]` Copy POD; the byte window is
        // exactly `PM_ROW` long and in bounds (checked above).
        Some(unsafe { core::ptr::read_unaligned(self.bytes.as_ptr().add(off) as *const _) })
    }

    /// `IN_USE` verdict for one slot. C: `mp_flags & IN_USE` — the list
    /// filters at proc.c:577 (`(flags & (IN_USE|TRACE_ZOMBIE|ZOMBIE)) ==
    /// IN_USE`).
    pub fn in_use(row: &MProcSnap) -> bool {
        row.mp_flags & mp_flags::IN_USE != 0
    }

    /// Zombie in either flavor. C: `mp_flags & (TRACE_ZOMBIE | ZOMBIE)`.
    pub fn is_zombie(row: &MProcSnap) -> bool {
        row.mp_flags & (mp_flags::TRACE_ZOMBIE | mp_flags::ZOMBIE) != 0
    }
}

/// A view over the VFS light table.
///
/// `[ARCH: A-7]` the producer is fail-closed today (C-21 second half), so
/// every slot answers "idle" — `get_lwp_stat`'s VFS lane then never fires,
/// exactly as if `fpl_blocked_on` were `FP_BLOCKED_ON_NONE`. When the wire
/// authority lands ([`LIGHT_ROW`] rows), this view decodes them here.
pub struct LightRows<'a> {
    #[allow(dead_code)]
    bytes: &'a [u8],
}

impl<'a> LightRows<'a> {
    /// View a light table.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Row count the buffer holds.
    pub fn len(&self) -> usize {
        self.bytes.len() / LIGHT_ROW
    }

    /// Whether the buffer holds no rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The pid index C builds once per `update_tables` (proc.c:126-139):
/// buckets → first mslot-or-`NO_SLOT`, mslot → next-or-`NO_SLOT`, and the
/// per-slot pids the chain compares. Rebuilt per request here — two short
/// vectors over 256 rows.
pub fn build_pid_hash(pm: &PmRows) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let nr_procs = minix_types::NR_PROCS;
    let slots_count = super::tables::hash_slots(nr_procs as u32) as usize;
    let mut hash = vec![super::tables::NO_SLOT; slots_count];
    let mut next = vec![super::tables::NO_SLOT; nr_procs];
    let mut pids = vec![0; nr_procs];
    for mslot in 0..nr_procs {
        let Some(row) = pm.row(mslot) else { continue };
        pids[mslot] = row.mp_pid;
        if row.mp_flags & mp_flags::IN_USE != 0 && row.mp_pid > 0 {
            let hslot = (row.mp_pid as u32 % slots_count as u32) as usize;
            next[mslot] = hash[hslot];
            hash[hslot] = mslot as i32;
        }
    }
    (hash, next, pids)
}

/// `fill_wmesg` with table access: the wait-channel message for one
/// endpoint, NUL-terminated, truncating like `strlcpy`. C: proc.c:185-216 —
/// `ANY`/`SELF`/`NONE` name themselves; a known peer names it (parenthesized
/// when the wait is a direct IPC); an unusable slot prints the raw endpoint
/// number (proc.c:213-214). "Known" per proc.c:202-208: tasks (negative
/// slots) always name, processes only when the PM side has the slot in use.
pub fn write_wmesg_named(
    buf: &mut [u8],
    kern: &KernelRows,
    pm: &PmRows,
    endpt: i32,
    direct_ipc: bool,
) {
    let text: alloc::string::String = if endpt == Endpoint::ANY.0 {
        "any".into()
    } else if endpt == Endpoint::SELF.0 {
        "self".into()
    } else if endpt == Endpoint::NONE.0 {
        "none".into()
    } else {
        let mslot = Endpoint(endpt).slot();
        let slot_ok = mslot >= -(NR_TASKS as i32) && mslot < minix_types::NR_PROCS as i32;
        let named = if slot_ok {
            let kslot = (NR_TASKS as i32 + mslot) as usize;
            let in_use = mslot < 0
                || pm
                    .row(mslot as usize)
                    .map(|r| PmRows::in_use(&r))
                    .unwrap_or(false);
            if in_use {
                kern.row(kslot).map(|row| cstr_to_str(&row.p_name).to_string())
            } else {
                None
            }
        } else {
            None
        };
        named.unwrap_or_else(|| alloc::format!("{}", endpt))
    };
    strlcpy(buf, &text, direct_ipc);
}

/// NUL-terminated name bytes as a string slice (lossy on non-UTF-8, which
/// process names never carry in practice; the fallback is C's `%s` printing
/// raw bytes either way).
fn cstr_to_str(name: &[u8]) -> &str {
    let end = name.iter().position(|b| *b == 0).unwrap_or(name.len());
    core::str::from_utf8(&name[..end]).unwrap_or("?")
}

/// `strlcpy` into the wait-channel buffer: truncate at `buf.len() - 1`, then
/// NUL. C: `snprintf(wmesg, wmsz, "%s%s%s", ...)` — proc.c:211-215. A
/// one-byte buffer holds just the NUL; an empty buffer is left alone.
fn strlcpy(buf: &mut [u8], text: &str, parens: bool) {
    if buf.is_empty() {
        return;
    }
    let body = if parens {
        alloc::format!("({})", text)
    } else {
        text.to_string()
    };
    let bytes = body.as_bytes();
    let keep = bytes.len().min(buf.len() - 1);
    buf[..keep].copy_from_slice(&bytes[..keep]);
    buf[keep] = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 261-row kernel table with one named task row and one user row.
    fn kern_table() -> Vec<u8> {
        let total = NR_TASKS + minix_types::NR_PROCS;
        let mut v = vec![0u8; total * KERN_ROW];
        // Task slot 1: name "memory".
        let t: ProcInfoStruct = ProcInfoStruct {
            p_name: name_bytes("memory"),
            ..ProcInfoStruct::default()
        };
        put(&mut v, 1, &t);
        // User slot 5 (kslot NR_TASKS+5): endpoint 5-ish, receiving from
        // "nothing" (NONE), runnable.
        let u = ProcInfoStruct {
            p_nr: 5,
            p_endpoint: 5,
            p_getfrom_e: Endpoint::NONE.0,
            p_sendto_e: Endpoint::NONE.0,
            p_name: name_bytes("ps"),
            ..ProcInfoStruct::default()
        };
        put(&mut v, NR_TASKS + 5, &u);
        v
    }

    fn put(buf: &mut [u8], kslot: usize, row: &ProcInfoStruct) {
        // SAFETY(test): `row` is a live repr(C) local; the slot window is in
        // bounds by construction.
        unsafe {
            core::ptr::write_unaligned(
                buf.as_mut_ptr().add(kslot * KERN_ROW) as *mut ProcInfoStruct,
                *row,
            );
        }
    }

    fn name_bytes(name: &str) -> [u8; 16] {
        let mut n = [0u8; 16];
        let b = name.as_bytes();
        n[..b.len()].copy_from_slice(b);
        n
    }

    #[test]
    fn test_row_decode_and_bounds() {
        let tab = kern_table();
        let rows = KernelRows::new(&tab);
        assert_eq!(rows.len(), NR_TASKS + minix_types::NR_PROCS);
        let task = rows.row(1).unwrap();
        assert_eq!(&task.p_name[..7], b"memory\0");
        let user = rows.row(NR_TASKS + 5).unwrap();
        assert!(KernelRows::is_runnable(&user));
        // Out of range: None, never a panic.
        assert!(rows.row(usize::MAX).is_none());
        let empty = KernelRows::new(&[]);
        assert!(empty.is_empty());
        assert!(empty.row(0).is_none());
    }

    #[test]
    fn test_blocked_on_macro() {
        // SENDING wins over RECEIVING (kernel/proc.h:187-198).
        let sending = ProcInfoStruct {
            p_rts_flags: minix_types::RTS_SENDING | minix_types::RTS_RECEIVING,
            p_sendto_e: 9,
            p_getfrom_e: 7,
            ..ProcInfoStruct::default()
        };
        assert_eq!(KernelRows::blocked_on(&sending), 9);
        let receiving = ProcInfoStruct {
            p_rts_flags: minix_types::RTS_RECEIVING,
            p_sendto_e: 9,
            p_getfrom_e: 7,
            ..ProcInfoStruct::default()
        };
        assert_eq!(KernelRows::blocked_on(&receiving), 7);
        let idle = ProcInfoStruct::default();
        assert_eq!(KernelRows::blocked_on(&idle), Endpoint::NONE.0);
    }

    #[test]
    fn test_pm_rows_and_zombie() {
        let mut tab = vec![0u8; minix_types::NR_PROCS * PM_ROW];
        let mut row = MProcSnap::default();
        row.mp_pid = 100;
        row.mp_flags = mp_flags::IN_USE;
        // SAFETY(test): repr(C) POD write into the slot window.
        unsafe {
            core::ptr::write_unaligned(tab.as_mut_ptr() as *mut MProcSnap, row);
        }
        let rows = PmRows::new(&tab);
        let r = rows.row(0).unwrap();
        assert!(PmRows::in_use(&r));
        assert!(!PmRows::is_zombie(&r));
        let mut zomb = MProcSnap::default();
        zomb.mp_flags = mp_flags::IN_USE | mp_flags::ZOMBIE;
        assert!(PmRows::is_zombie(&zomb));
        assert!(rows.row(usize::MAX).is_none());
    }

    #[test]
    fn test_wmesg_lanes() {
        let tab = kern_table();
        let kern = KernelRows::new(&tab);
        let pm = PmRows::new(&[]);
        let mut buf = [0u8; 8];
        // Special lanes name themselves (proc.c:191-200).
        write_wmesg_named(&mut buf, &kern, &pm, Endpoint::ANY.0, false);
        assert_eq!(&buf[..4], b"any\0");
        write_wmesg_named(&mut buf, &kern, &pm, Endpoint::NONE.0, false);
        assert_eq!(&buf[..5], b"none\0");
        // A task peer (slot -4 -> kslot 1, "memory") names with parens for a
        // direct IPC wait (proc.c:211-215); the 8-byte window truncates like
        // strlcpy.
        let task_ep = Endpoint::from_generation_slot(0, -4).0;
        assert_eq!(Endpoint(task_ep).slot(), -4);
        write_wmesg_named(&mut buf, &kern, &pm, task_ep, true);
        assert_eq!(&buf[..8], b"(memory\0");
        // Unknown peer: the raw number (proc.c:213-214).
        write_wmesg_named(&mut buf, &kern, &pm, 321, true);
        assert_eq!(&buf[..6], b"(321)\0");
    }
}
