//! Per-drive open/minor state policy (U-6 阶段二 increment 6).
//!
//! C correspondence: `w_prepare` (at_wini.c:519-540) maps a block minor to a
//! `(drive, sub-device)` slot; `w_do_open` (:454-486) admits an open unless the
//! drive was already probed-and-ignored (returns `ENXIO`) and bumps the drive's
//! `open_ct`; `w_do_close` (:1460-1471) decrements it and, when it drops to
//! zero on an ATAPI drive, is the point the ATAPI device is closed.
//!
//! This is the pure *state* layer: the minor map and the open reference count.
//! It deliberately excludes the hardware side (the on-first-open `w_identify`
//! and the reset-on-deaf path live in the service binary's I/O phase and are a
//! later increment) — so the mapping/counting is verifiable on the host without
//! a controller.

use crate::pio::IdentifyInfo;
use minix_types::ENXIO;

/// Minors per drive: whole disk plus `NR_PARTITIONS` primary partitions.
/// C: `DEV_PER_DRIVE (1 + NR_PARTITIONS)`, `NR_PARTITIONS 4` — drvlib.h:10,
/// partition.h:21.
pub const DEV_PER_DRIVE: i32 = 5;
/// Physical IDE drives per controller instance. C: `MAX_DRIVES 4` — at_wini.h:184.
pub const MAX_DRIVES: usize = 4;
/// Drive nodes for minor numbering. C: `MAX_DRIVENODES 8` — at_wini.h:185.
const MAX_DRIVENODES: i32 = 8;
/// Total primary minors. C: `NR_MINORS (MAX_DRIVENODES * DEV_PER_DRIVE)` = 40 —
/// at_wini.h:188.
pub const NR_MINORS: i32 = MAX_DRIVENODES * DEV_PER_DRIVE;
/// Sub-device minors per drive. C: `SUB_PER_DRIVE (NR_PARTITIONS * NR_PARTITIONS)`
/// = 16 — drvlib.h:11.
const SUB_PER_DRIVE: i32 = 16;
/// Total sub-device minors. C: `NR_SUBDEVS (MAX_DRIVENODES * SUB_PER_DRIVE)` —
/// at_wini.h:189.
const NR_SUBDEVS: i32 = MAX_DRIVENODES * SUB_PER_DRIVE;
/// First sub-device (slice) minor. C: `MINOR_d0p0s0 128` — drvlib.h:14.
const MINOR_D0P0S0: i32 = 128;

/// Resolve a block minor to its `(drive, slot)` device, mirroring C
/// `w_prepare` (at_wini.c:519-540).
///
/// Minors `0..NR_MINORS` are `d{drive}[/p{slot}]` (whole disk plus primary
/// partitions); `MINOR_D0P0S0..` are logical sub-device slices. Returns `None`
/// for a minor out of either range or one whose drive index is past
/// `MAX_DRIVES` (the four real IDE units).
pub const fn resolve_minor(minor: i32) -> Option<(usize, usize)> {
    if minor >= 0 && minor < NR_MINORS {
        let drive = minor / DEV_PER_DRIVE;
        if drive >= MAX_DRIVES as i32 {
            None
        } else {
            Some((drive as usize, (minor % DEV_PER_DRIVE) as usize))
        }
    } else {
        let sub = minor - MINOR_D0P0S0;
        if sub >= 0 && sub < NR_SUBDEVS {
            let drive = sub / SUB_PER_DRIVE;
            if drive >= MAX_DRIVES as i32 {
                None
            } else {
                Some((drive as usize, (sub % SUB_PER_DRIVE) as usize))
            }
        } else {
            None
        }
    }
}

/// One drive's open/identify lifecycle state.
///
/// C: the `wini` `open_ct` and the `state` bits `IDENTIFIED` / `IGNORING` /
/// `DEAF` / `ATAPI` (at_wini.c:60-67, 454-486, 1460-1471).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriveState {
    open_ct: u32,
    identified: bool,
    ignoring: bool,
    deaf: bool,
    atapi: bool,
}

impl DriveState {
    /// A drive seen for the first time: not yet identified, nothing open.
    pub const fn fresh() -> Self {
        DriveState {
            open_ct: 0,
            identified: false,
            ignoring: false,
            deaf: false,
            atapi: false,
        }
    }

    /// Current open count. C: `wn->open_ct`.
    pub const fn open_ct(&self) -> u32 {
        self.open_ct
    }

    pub const fn is_identified(&self) -> bool {
        self.identified
    }

    /// Mark a successful identify: clears the ignore/deaf latches. C:
    /// `w_identify` success path (`state &= ~IGNORING` implicitly via re-probe).
    pub fn note_identified(&mut self) {
        self.identified = true;
        self.ignoring = false;
        self.deaf = false;
    }

    /// A probe already failed for this drive — future opens are refused until a
    /// reset re-clears it. C: `wn->state = IGNORING` (at_wini.c:484).
    pub fn mark_ignoring(&mut self) {
        self.ignoring = true;
    }

    /// The drive stopped answering; forces a re-identify on the next open.
    /// C: `DEAF` state bit (at_wini.c:470).
    pub fn mark_deaf(&mut self) {
        self.deaf = true;
    }

    pub fn set_atapi(&mut self, atapi: bool) {
        self.atapi = atapi;
    }

    /// Whether this drive needs (re-)identify before I/O: never identified, or
    /// gone deaf. C: `!(state & IDENTIFIED) || (state & DEAF)` (at_wini.c:469).
    pub const fn needs_identify(&self) -> bool {
        !self.identified || self.deaf
    }

    /// Admit an open, bumping the reference count. C `w_do_open`: a drive
    /// already ignored answers `ENXIO`. The identify-on-first-open is the
    /// caller's I/O phase (see [`Self::needs_identify`]).
    pub fn open(&mut self) -> Result<(), i32> {
        if self.ignoring {
            return Err(ENXIO);
        }
        self.open_ct += 1;
        Ok(())
    }

    /// Release an open. C `w_do_close`: decrement, and when the last one closes
    /// an ATAPI drive, signal the caller to run `atapi_close()`. Returns whether
    /// the ATAPI close hook fired.
    pub fn close(&mut self) -> bool {
        debug_assert!(self.open_ct > 0, "close below zero open count");
        self.open_ct = self.open_ct.saturating_sub(1);
        self.open_ct == 0 && self.atapi
    }
}

/// The per-drive state table behind the block open/close requests.
#[derive(Debug, Clone)]
pub struct Drives {
    slots: [DriveState; MAX_DRIVES],
}

impl Drives {
    pub const fn new() -> Self {
        Drives {
            slots: [const { DriveState::fresh() }; MAX_DRIVES],
        }
    }
    /// C: `w_wn` selection after `w_prepare`.
    pub fn drive(&self, drive: usize) -> Option<&DriveState> {
        self.slots.get(drive)
    }

    pub fn drive_mut(&mut self, drive: usize) -> Option<&mut DriveState> {
        self.slots.get_mut(drive)
    }

    /// `w_do_open` for a minor: resolve then open that drive. `ENXIO` for an
    /// unknown minor or an ignored drive.
    pub fn open(&mut self, minor: i32) -> Result<(), i32> {
        let (drive, _) = resolve_minor(minor).ok_or(ENXIO)?;
        self.drive_mut(drive).ok_or(ENXIO)?.open()
    }

    /// `w_do_close` for a minor: resolve, then close; returns whether ATAPI
    /// must be closed.
    pub fn close(&mut self, minor: i32) -> Result<bool, i32> {
        let (drive, _) = resolve_minor(minor).ok_or(ENXIO)?;
        self.drive_mut(drive).ok_or(ENXIO).map(DriveState::close)
    }
}

impl Default for Drives {
    fn default() -> Self {
        Self::new()
    }
}

/// Reported CHS geometry for a drive. C: the `cylinders`/`heads`/`sectors`
/// fields of `struct part_geom` that `w_geometry` fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub cylinders: u32,
    pub heads: u32,
    pub sectors: u32,
}

/// Compute the logical CHS geometry the driver reports for a drive. C:
/// `w_geometry` (at_wini.c:1700-1721).
///
/// An ATAPI device gets made-up numbers (64 heads, 32 sectors, cylinders
/// derived from the sector count). An ATA device reports its identified
/// geometry, then repeatedly folds cylinders into heads (`heads *= 2`,
/// `cylinders /= 2`) while cylinders exceed 1024 — the INT 13h LBA-assist
/// translation that keeps the reported CHS within BIOS addressable bounds.
/// `disk_sectors` is the whole-disk sector count (used only for the ATAPI case).
pub fn compute_geometry(identify: &IdentifyInfo, atapi: bool, disk_sectors: u64) -> Geometry {
    if atapi {
        // C: (dv_size / SECTOR_SIZE) / (64*32) — dv_size in bytes, /512 = sectors.
        return Geometry {
            cylinders: (disk_sectors / (64 * 32)) as u32,
            heads: 64,
            sectors: 32,
        };
    }
    let mut cylinders = identify.cylinders as u32;
    let mut heads = identify.heads as u32;
    let sectors = identify.sectors_per_track as u32;
    while cylinders > 1024 {
        heads *= 2;
        cylinders /= 2;
    }
    Geometry {
        cylinders,
        heads,
        sectors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_primary_minors() {
        assert_eq!(resolve_minor(0), Some((0, 0))); // d0 whole disk
        assert_eq!(resolve_minor(1), Some((0, 1))); // d0p0
        assert_eq!(resolve_minor(5), Some((1, 0))); // d1
        assert_eq!(resolve_minor(19), Some((3, 4))); // last slot of drive 3
    }

    #[test]
    fn resolve_rejects_past_max_drives_and_subdev_and_garbage() {
        // minor 20 → drive 20/5 = 4 == MAX_DRIVES → no physical unit.
        assert_eq!(resolve_minor(20), None);
        assert_eq!(resolve_minor(-1), None);
        assert_eq!(resolve_minor(NR_MINORS), None);
        // sub-device range: 128 is the first slice of drive 0.
        assert_eq!(resolve_minor(MINOR_D0P0S0), Some((0, 0)));
        assert_eq!(resolve_minor(MINOR_D0P0S0 + SUB_PER_DRIVE), Some((1, 0)));
        // the sub-range only spans NR_SUBDEVS minors.
        assert_eq!(resolve_minor(MINOR_D0P0S0 + NR_SUBDEVS), None);
    }

    #[test]
    fn open_close_reference_count_and_atapi_hook() {
        let mut d = DriveState::fresh();
        assert_eq!(d.open(), Ok(()));
        assert_eq!(d.open(), Ok(()));
        assert_eq!(d.open_ct(), 2);
        assert!(!d.close(), "not last open");
        d.set_atapi(true);
        assert!(d.close(), "last open on an ATAPI drive signals atapi_close");
        assert_eq!(d.open_ct(), 0);
    }

    #[test]
    fn ignored_drive_refuses_open() {
        let mut d = DriveState::fresh();
        d.mark_ignoring();
        assert_eq!(d.open(), Err(ENXIO));
        // a re-identify clears the ignore latch.
        d.note_identified();
        assert_eq!(d.open(), Ok(()));
    }

    #[test]
    fn needs_identify_tracks_state() {
        let mut d = DriveState::fresh();
        assert!(d.needs_identify(), "never identified");
        d.note_identified();
        assert!(!d.needs_identify());
        d.mark_deaf();
        assert!(d.needs_identify(), "deaf forces a re-identify");
    }

    #[test]
    fn drives_table_open_close_by_minor() {
        let mut t = Drives::new();
        assert_eq!(t.open(0), Ok(())); // drive 0
        assert_eq!(t.open(1), Ok(())); // drive 0 (partition) — same drive count
        assert_eq!(t.drive(0).unwrap().open_ct(), 2);
        // unknown minor (past MAX_DRIVES) refuses.
        assert_eq!(t.open(20), Err(ENXIO));
        assert_eq!(t.close(0), Ok(false));
        assert_eq!(t.drive(0).unwrap().open_ct(), 1);
    }

    fn identify(cyl: u16, head: u16, spt: u16) -> IdentifyInfo {
        IdentifyInfo {
            cylinders: cyl,
            heads: head,
            sectors_per_track: spt,
            lba: true,
            lba48: false,
            total_sectors: 0,
        }
    }

    #[test]
    fn ata_geometry_folds_cylinders_into_heads() {
        // C: while cylinders > 1024 { heads *= 2; cylinders /= 2 } (:1716-1719).
        // 16383 → 8191 → 4095 → 2047 → 1023 (four halvings, heads ×16).
        let g = compute_geometry(&identify(16383, 16, 63), false, 0);
        assert_eq!(
            g,
            Geometry {
                cylinders: 1023,
                heads: 256,
                sectors: 63
            }
        );
    }

    #[test]
    fn ata_geometry_under_1024_is_untouched() {
        let g = compute_geometry(&identify(800, 128, 63), false, 0);
        assert_eq!(
            g,
            Geometry {
                cylinders: 800,
                heads: 128,
                sectors: 63
            }
        );
    }

    #[test]
    fn atapi_geometry_is_made_up() {
        // C: ATAPI branch — 64 heads, 32 sectors, cylinders = sectors/(64*32).
        let g = compute_geometry(&identify(0, 0, 0), true, 64 * 32 * 500);
        assert_eq!(
            g,
            Geometry {
                cylinders: 500,
                heads: 64,
                sectors: 32
            }
        );
    }
}
