//! VFS dump domain (07-is-dump-vfs.md §4.1).
//!
//! C: `minix3/minix/servers/is/dmp_fs.c` (83 lines). Two dumps over two
//! tables (`SI_PROC_TAB` + `SI_DMAP_TAB`): fd counting, session/revived
//! bits, blocked-on endpoints, driver mapping. Bodies await the A-6 output
//! channel; this module delivers snapshots, pure logic, and format
//! constants.
//!
//! `[ARCH: A-4]`: snapshots are wire-contract proposals (`#[repr(C)]`,
//! used-fields subsets); the VFS-side GETSYSINFO producer aligns to them
//! (SI_PROC_TAB half closed: `FProcSnap` is the minix-types authority and
//! VFS `do_getsysinfo` serves both via the shared snapshots — `FProcSnap`
//! （E-MIBPROD）与 `DmapSnap`（DMAP 臂按整表宽度逐行序列化，空槽记 `NONE`）。

/// VFS process snapshot (used fields only).
///
/// C: `struct fproc` — `minix3/minix/servers/vfs/fproc.h` (subset).
// FProcSnap 已上收 `minix_types::FProcSnap`（E-MIBPROD 快照权威单点，
// 与 MprocWire/ProcInfoStruct 同型）。本 crate 改 import 消费。
pub use minix_types::FProcSnap;

use crate::PCStr;
use core::fmt;
use minix_types::Endpoint;

/// Max open files per process. C: `OPEN_MAX 255` — sys/syslimits.h:38.
pub const OPEN_MAX_FD: usize = 255;

/// Session-leader bit. C: `FP_SESLDR 0004` — fproc.h:96.
pub const FP_SESLDR: u32 = 0o4;
/// Revived bit. C: `FP_REVIVED 0002` — fproc.h:94.
pub const FP_REVIVED: u32 = 0o2;

/// Blocked-on reason.
///
/// C: `FP_BLOCKED_ON_*` — `minix3/minix/servers/vfs/const.h:19-25`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedOn {
    None,
    Pipe,
    Flock,
    Popen,
    Select,
    Cdev,
    Sdev,
}

impl BlockedOn {
    /// Wire value.
    pub const fn code(self) -> i32 {
        match self {
            BlockedOn::None => 0,
            BlockedOn::Pipe => 1,
            BlockedOn::Flock => 2,
            BlockedOn::Popen => 3,
            BlockedOn::Select => 4,
            BlockedOn::Cdev => 5,
            BlockedOn::Sdev => 6,
        }
    }

    /// Decodes a wire value; unknown values are `None` (forward-compatible,
    /// cf. C's open `int` comparison chain).
    pub const fn decode(v: i32) -> Option<Self> {
        match v {
            0 => Some(BlockedOn::None),
            1 => Some(BlockedOn::Pipe),
            2 => Some(BlockedOn::Flock),
            3 => Some(BlockedOn::Popen),
            4 => Some(BlockedOn::Select),
            5 => Some(BlockedOn::Cdev),
            6 => Some(BlockedOn::Sdev),
            _ => None,
        }
    }
}

/// Resolves the proc column for a blocked process.
///
/// C: `fp_blocked_on == FP_BLOCKED_ON_CDEV → fp_cdev.endpt`, else `" nil"`
/// (dmp_fs.c:60-63), with the source comment
/// `/* TODO: for FP_BLOCKED_ON_SDEV we do not have the endpoint.. */`.
/// The SDEV gap is inherited explicitly as `None` (not this doc's debt).
pub const fn blocked_endpoint(on: BlockedOn, cdev_endpt: i32) -> Option<i32> {
    match on {
        BlockedOn::Cdev => Some(cdev_endpt),
        _ => None,
    }
}

/// Device number decomposition, Minix3/Linux-compatible `dev_t` layout:
/// `major(x) = ((x & 0x000fff00) >> 8)`, `minor(x) = ((x & 0xfff00000) >> 12)
/// | (x & 0xff)` (tools/compat/compat_defs.h:1254-1261 mirrors the OS
/// encoding; fproc_dmp prints `major(fp_tty)`/`minor(fp_tty)`).
pub const fn major_of(dev: i32) -> i32 {
    (((dev as u32) & 0x000f_ff00) >> 8) as i32
}

/// See [`major_of`].
pub const fn minor_of(dev: i32) -> i32 {
    (((dev as u32) & 0xfff0_0000) >> 12 | (dev as u32) & 0xff) as i32
}

/// `fproc_dmp` row loop (dmp_fs.c:42-69): skip `pid <= 0`, `++n > 22`
/// breaks, the CDEV endpoint column otherwise prints ` nil`.
pub fn render_fproc(
    out: &mut dyn fmt::Write,
    tab: &[FProcSnap],
    cur: &mut VfsCursor,
) -> fmt::Result {
    out.write_str(FPROC_TITLE)?;
    out.write_str(FPROC_COLUMNS)?;
    let mut exhausted = true;
    for (i, fp) in tab.iter().enumerate() {
        match cur.push(fp.fp_pid, i) {
            VfsAction::Skip => continue,
            VfsAction::More => {
                exhausted = false;
                break;
            }
            VfsAction::Emit => {}
        }
        let on = BlockedOn::decode(fp.fp_blocked_on);
        let ldr = u32::from(fp.fp_flags & FP_SESLDR != 0);
        let rev = u32::from(fp.fp_flags & FP_REVIVED != 0);
        write!(
            out,
            "{:3}  {:4}  {}/{}  0x{:05x} {:2} ({:2}) {:2} ({:2}) {:3} {:3} {:3} {:3} ",
            i as i32,
            fp.fp_pid,
            major_of(fp.fp_tty),
            minor_of(fp.fp_tty),
            fp.fp_umask,
            fp.fp_realuid as i32,
            fp.fp_effuid as i32,
            fp.fp_realgid as i32,
            fp.fp_effgid as i32,
            ldr,
            fp.nfds,
            fp.fp_blocked_on,
            rev
        )?;
        match on.and_then(|on| blocked_endpoint(on, fp.fp_cdev_endpt)) {
            Some(ep) => writeln!(out, "{:4}", ep)?,
            None => out.write_str(NIL_ENDPOINT)?,
        }
    }
    cur.finish(exhausted);
    Ok(())
}

/// `dtab_dmp` (dmp_fs.c:66-83): one sparse screen over the device map —
/// `NONE` slots skipped, no pagination.
pub fn render_dtab(out: &mut dyn fmt::Write, dmaps: &[DmapSnap]) -> fmt::Result {
    out.write_str(DMAP_TITLE)?;
    out.write_str(DMAP_COLUMNS)?;
    for (i, d) in dmaps.iter().enumerate() {
        if dmap_skipped(d.dmap_driver, NONE_ENDPOINT) {
            continue;
        }
        writeln!(out, "{:>13} {:5} {:10}", PCStr(&d.dmap_label), i as i32, d.dmap_driver)?;
    }
    Ok(())
}

/// Whether an fproc row is skipped.
///
/// C: `if (fp->fp_pid <= 0) continue` — dmp_fs.c:43. Note the difference
/// from PM (`== 0` + slot-0 exception): VFS has no exception.
pub const fn fproc_skipped(pid: i32) -> bool {
    pid <= 0
}

pub use minix_types::{DmapSnap, DMAP_LABEL_LEN, NR_DEVICES};

/// Whether a dmap row is skipped. C: `dmap[i].dmap_driver == NONE` —
/// dmp_fs.c:78.
pub const fn dmap_skipped(driver: i32, none: i32) -> bool {
    driver == none
}

/// One fproc pagination step. C: dmp_fs.c:42-47 (`continue` on skip, the
/// `++n > 22` break). Three states — a `bool` could not tell "skipped,
/// keep going" from "screen full, stop" (V1-P1-3 fix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsAction {
    /// pid <= 0: skipped, uncounted, loop continues.
    Skip,
    /// Row printed.
    Emit,
    /// 23rd candidate: stop, `--more--\r`, resume here next round.
    More,
}

/// VFS page cursor: isomorphic to 06's `PmCursor` (22-bound, `\r`, wrap).
/// A fresh type (not an alias): 06 is CONVERGED and its `Pm`-prefixed name
/// would lie here. Logic intentionally duplicated, noted here.
#[derive(Debug, Clone, Copy, Default)]
pub struct VfsCursor {
    next: usize,
    n: u32,
}

impl VfsCursor {
    pub const fn new() -> Self {
        Self { next: 0, n: 0 }
    }

    pub const fn next(&self) -> usize {
        self.next
    }

    /// C: `if (fp->fp_pid <= 0) continue; if (++n > 22) break;` — dmp_fs.c:43-44.
    pub const fn push(&mut self, pid: i32, idx: usize) -> VfsAction {
        if fproc_skipped(pid) {
            return VfsAction::Skip;
        }
        self.n += 1;
        if self.n > 22 {
            self.next = idx;
            return VfsAction::More;
        }
        VfsAction::Emit
    }

    /// C: `if (i >= NR_PROCS) i = 0; else printf("--more--\r"); prev_i = i;`
    pub const fn finish(&mut self, exhausted: bool) {
        if exhausted {
            self.next = 0;
        }
    }
}

/// C: `"File System (FS) process table dump\n"` — dmp_fs.c:50.
pub const FPROC_TITLE: &str = "File System (FS) process table dump\n";
/// C: dmp_fs.c:51.
pub const FPROC_COLUMNS: &str =
    "-nr- -pid- -tty- -umask- --uid-- --gid-- -ldr-fds-sus-rev-proc-\n";
/// C: `"File System (FS) device <-> driver mappings\n"` — dmp_fs.c:73.
pub const DMAP_TITLE: &str = "File System (FS) device <-> driver mappings\n";
/// C: dmp_fs.c:74-75.
pub const DMAP_COLUMNS: &str = "    Label     Major Driver ept\n------------- ----- ----------\n";
/// C: `" nil\n"` non-cdev branch — dmp_fs.c:63.
pub const NIL_ENDPOINT: &str = " nil\n";
/// C: `NONE` endpoint — endpoint.h:55 (`dmap_driver == NONE` skip).
pub const NONE_ENDPOINT: i32 = Endpoint::NONE.get();

#[cfg(test)]
mod tests {
    use super::*;

    fn fproc_row(slot: usize, pid: i32, tty: i32, blocked_on: i32, nfds: u32) -> FProcSnap {
        FProcSnap {
            fp_pid: pid,
            fp_tty: tty,
            fp_umask: 0o022,
            fp_realuid: 1000,
            fp_effuid: 1000,
            fp_realgid: 100,
            fp_effgid: 100,
            fp_flags: FP_SESLDR | FP_REVIVED,
            fp_blocked_on: blocked_on,
            nfds,
            fp_cdev_endpt: 7,
            ..Default::default()
        }
    }

    #[test]
    fn test_render_fproc_row_and_skip() {
        // C: dmp_fs.c:42-69 — %3d slot, %2d/%d tty split, 0x%05x umask,
        // ldr/nfds/blocked/revived columns, CDEV endpoint vs ` nil`.
        let tab = [
            fproc_row(0, 0, 0x400, 0, 0),          // skipped: pid <= 0
            fproc_row(1, 1, 0x400, BlockedOn::Cdev.code(), 3),
            fproc_row(2, 2, 0x400, BlockedOn::Pipe.code(), 1),
        ];
        let mut out = String::new();
        let mut cur = VfsCursor::new();
        render_fproc(&mut out, &tab, &mut cur).unwrap();
        // Slot 1: cdev blocked → the endpoint prints in the last column.
        assert!(out.contains(
            "  1     1  4/0  0x00012 1000 (1000) 100 (100)   1   3   5   1    7\n"
        ));
        // Slot 2: not cdev → ` nil`.
        assert!(out.contains(
            "  2     2  4/0  0x00012 1000 (1000) 100 (100)   1   1   1   1  nil\n"
        ));
        // Slot 0 (pid 0) skipped: title + columns + 2 rows only.
        assert_eq!(out.matches('\n').count(), 4);
    }

    #[test]
    fn test_render_dtab_rows() {
        // C: dmp_fs.c:73-82 — %13s label, %5d major, %10d driver ept; NONE
        // slots skipped; no pagination (sparse single screen).
        let mut dmaps = [DmapSnap::default(); 4];
        dmaps[0].dmap_driver = NONE_ENDPOINT;
        dmaps[1].dmap_label = name16(b"cd");
        dmaps[1].dmap_driver = 3;
        dmaps[2].dmap_label = name16(b"floppy");
        dmaps[2].dmap_driver = 2;
        dmaps[3].dmap_driver = NONE_ENDPOINT;
        let mut out = String::new();
        render_dtab(&mut out, &dmaps).unwrap();
        assert!(out.contains("           cd     1          3\n"));
        assert!(out.contains("       floppy     2          2\n"));
        assert!(!out.contains("  0 "), "empty slots skipped");
    }



    fn name16(name: &[u8]) -> [u8; 16] {
        let mut n = [0u8; 16];
        n[..name.len()].copy_from_slice(name);
        n
    }

    #[test]
    fn test_major_minor_split() {
        // Minix3/Linux-compatible dev_t (compat_defs.h:1254-1261):
        // console = major 4, minor 0 → dev 0x400.
        assert_eq!((major_of(0x400), minor_of(0x400)), (4, 0));
        assert_eq!((major_of(0x302), minor_of(0x302)), (3, 2));
        assert_eq!(OPEN_MAX_FD, 255);
    }

    #[test]
    fn test_skip_rule_no_exception() {
        // C: `fp_pid <= 0` — dmp_fs.c:43 (vs PM's ==0+exception).
        assert!(fproc_skipped(0));
        assert!(fproc_skipped(-3));
        assert!(!fproc_skipped(1));
    }

    #[test]
    fn test_fp_bits() {
        // C: fproc.h:94-96 (octal 0002/0004).
        assert_eq!((FP_SESLDR, FP_REVIVED), (0o4, 0o2));
        assert_eq!(0x10u32 & FP_SESLDR, 0);
        assert_ne!(0x04u32 & FP_SESLDR, 0);
    }

    #[test]
    fn test_blocked_on_codes_and_endpoint() {
        // C: const.h:19-25 + dmp_fs.c:60-63 (+ SDEV TODO).
        assert_eq!(BlockedOn::decode(5), Some(BlockedOn::Cdev));
        assert_eq!(BlockedOn::decode(6), Some(BlockedOn::Sdev));
        assert_eq!(BlockedOn::decode(99), None);
        assert_eq!(blocked_endpoint(BlockedOn::Cdev, 7), Some(7));
        assert_eq!(blocked_endpoint(BlockedOn::Sdev, 7), None);
        assert_eq!(blocked_endpoint(BlockedOn::Pipe, 7), None);
    }

    #[test]
    fn test_vfs_cursor_pages_like_pm() {
        let mut c = VfsCursor::new();
        for i in 0..22 {
            assert_eq!(c.push(1, i), VfsAction::Emit);
        }
        assert_eq!(c.push(1, 22), VfsAction::More);
        assert_eq!(c.next(), 22);
        assert_eq!(c.push(0, 23), VfsAction::Skip, "skipped rows don't count or break");
        c.finish(true);
        assert_eq!(c.next(), 0);
    }

    #[test]
    fn test_dmap_skip_and_columns() {
        assert!(dmap_skipped(-99, -99));
        assert!(!dmap_skipped(5, -99));
        assert!(DMAP_COLUMNS.contains("Label     Major Driver ept"));
        assert_eq!(NIL_ENDPOINT, " nil\n");
    }
}
