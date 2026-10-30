//! Block control-plane service face.
//!
//! Assembles at_wini into a [`BlockDriver`]: the disk-kind routing, the
//! open/close reference counting (delegated to [`crate::service`]), and the
//! partition / geometry reports (served from the identify result + resolved
//! extents installed after probe). This is the *control plane* — the part that
//! is host-verifiable without a controller.
//!
//! The data plane (`transfer`) and the interrupt/alarm hooks stay at the trait
//! defaults: the physical grant copy between a caller buffer and the drive, and
//! the IDE IRQ14 / synchronous-alarm wiring, are the same registered driver
//! data-plane seam the reference `memory` driver carries (`memory/src/service.rs`
//! documents "a real boot still needs that copy"). They are wired in the
//! on-target launch increment, not fabricated here.

use crate::pio::IdentifyInfo;
use crate::service::{Drives, compute_geometry, resolve_minor};
use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::partition::PRIMARY_SLOTS;
use minix_blockdriver::protocol::{BlockDriverType, DeviceExtent, DeviceMinor, PartitionGeometry};

/// OK return for the block open/close hooks (C `OK`).
const OK: i32 = 0;

/// A drive's post-probe description: what it is and how it is partitioned.
#[derive(Debug, Clone, Copy)]
pub struct ProbedDrive {
    /// The IDENTIFY result (geometry + LBA capability + capacity).
    pub identify: IdentifyInfo,
    /// Whether this is an ATAPI device (drives the geometry special case).
    pub atapi: bool,
    /// Byte extents for the drive's `PRIMARY_SLOTS` minors (whole + primaries).
    pub extents: [Option<DeviceExtent>; PRIMARY_SLOTS],
}

/// The at_wini block face: per-drive open state + probed description.
#[derive(Debug)]
pub struct WiniBlock {
    drives: Drives,
    probed: [Option<ProbedDrive>; crate::service::MAX_DRIVES],
}

impl Default for WiniBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl WiniBlock {
    /// A fresh service: nothing probed, nothing open.
    pub const fn new() -> Self {
        WiniBlock {
            drives: Drives::new(),
            probed: [const { None }; crate::service::MAX_DRIVES],
        }
    }

    /// Record a successful probe for `drive` (the on-target
    /// identify/partition path calls this once it has IDENTIFY + the
    /// partition table). Marks the drive identified so opens are admitted.
    pub fn install_probe(&mut self, drive: usize, probed: ProbedDrive) {
        if drive < crate::service::MAX_DRIVES {
            self.probed[drive] = Some(probed);
            if let Some(state) = self.drives.drive_mut(drive) {
                state.note_identified();
            }
        }
    }

    fn slot_extent(&self, minor: u32) -> Option<(usize, usize, DeviceExtent)> {
        let (drive, slot) = resolve_minor(minor as i32)?;
        let extent = self
            .probed
            .get(drive)?
            .and_then(|p| p.extents.get(slot).copied()?)?;
        Some((drive, slot, extent))
    }
}

impl BlockDriver for WiniBlock {
    fn driver_type(&self) -> BlockDriverType {
        // Only whole-disk + primary-partition minors are served as a disk here;
        // sub-device (slice) minors resolve through the same drive. C: `w_dtab`
        // is `BLOCKDRIVER_TYPE_DISK`.
        BlockDriverType::Disk
    }

    fn open(&mut self, minor: DeviceMinor, _access: i32) -> i32 {
        match self.drives.open(minor.0 as i32) {
            Ok(()) => OK,
            Err(errno) => errno,
        }
    }

    fn close(&mut self, minor: DeviceMinor) -> i32 {
        // The ATAPI close signal is consumed by the on-target interrupt/service
        // loop; the reference-count bookkeeping is what the control plane owns
        // here.
        match self.drives.close(minor.0 as i32) {
            Ok(_atapi_close) => OK,
            Err(errno) => errno,
        }
    }

    fn partition(&mut self, minor: DeviceMinor) -> Option<DeviceExtent> {
        self.slot_extent(minor.0)
            .map(|(_drive, _slot, extent)| extent)
    }

    fn geometry(&mut self, minor: DeviceMinor) -> Option<PartitionGeometry> {
        let (drive, _slot, extent) = self.slot_extent(minor.0)?;
        let probed = self.probed[drive]?;
        let disk_sectors = probed.identify.total_sectors as u64;
        let geo = compute_geometry(&probed.identify, probed.atapi, disk_sectors);
        Some(PartitionGeometry {
            base: extent.base,
            size: extent.size,
            cylinders: geo.cylinders,
            heads: geo.heads,
            sectors: geo.sectors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_blockdriver::partition::{MbrEntry, parse_primary_table, primary_extents};

    const ACTIVE: u8 = 0x80;
    const MINIX_PART: u8 = 0x81;

    /// An 8 GiB-class disk (16383 cyl × 16 head × 63 spt) with one primary
    /// partition at LBA 2048 spanning 1000 sectors, as IDENTIFY would report.
    fn probed_with_partition() -> ProbedDrive {
        let identify = IdentifyInfo {
            cylinders: 16383,
            heads: 16,
            sectors_per_track: 63,
            lba: true,
            lba48: false,
            total_sectors: 100_000,
        };
        let mut mbr = [0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        let e = minix_blockdriver::partition::PART_TABLE_OFF;
        mbr[e] = ACTIVE;
        mbr[e + 4] = MINIX_PART;
        mbr[e + 8..e + 12].copy_from_slice(&2048u32.to_le_bytes());
        mbr[e + 12..e + 16].copy_from_slice(&1000u32.to_le_bytes());
        let table = parse_primary_table(&mbr).unwrap();
        ProbedDrive {
            identify,
            atapi: false,
            extents: primary_extents(&table, 100_000),
        }
    }

    fn entry(start: u32, sectors: u32) -> Option<MbrEntry> {
        Some(MbrEntry {
            bootable: false,
            sysind: MINIX_PART,
            start_lba: start,
            sectors,
        })
    }

    #[test]
    fn unprobed_drive_has_no_partition_and_denies_nothing() {
        let mut s = WiniBlock::new();
        assert_eq!(s.open(DeviceMinor(0), 0), OK);
        // Never probed → no extent yet (a real driver probes on first open).
        assert_eq!(s.partition(DeviceMinor(0)), None);
        assert_eq!(s.geometry(DeviceMinor(0)), None);
    }

    #[test]
    fn served_after_probe_reports_whole_and_partition() {
        let mut s = WiniBlock::new();
        s.install_probe(0, probed_with_partition());

        // Whole disk minor 0: extent [0, 100_000*512).
        assert_eq!(
            s.partition(DeviceMinor(0)),
            Some(DeviceExtent {
                base: 0,
                size: 100_000 * 512
            })
        );
        // First primary partition is minor 1 (drive 0, slot 1).
        assert_eq!(
            s.partition(DeviceMinor(1)),
            Some(DeviceExtent {
                base: 2048 * 512,
                size: 1000 * 512
            })
        );
        // Geometry folds 16383 cylinders down to <= 1024.
        let g = s.geometry(DeviceMinor(0)).unwrap();
        assert_eq!(g.base, 0);
        assert_eq!(g.size, 100_000 * 512);
        assert_eq!(g.cylinders, 1023);
        assert_eq!(g.heads, 256);
        assert_eq!(g.sectors, 63);
    }

    #[test]
    fn open_close_reference_count_flows_through_faces() {
        let mut s = WiniBlock::new();
        s.install_probe(0, probed_with_partition());
        assert_eq!(s.open(DeviceMinor(0), 0), OK);
        assert_eq!(s.open(DeviceMinor(1), 0), OK); // same drive, count 2
        assert_eq!(s.close(DeviceMinor(0)), OK);
        assert_eq!(s.close(DeviceMinor(1)), OK);
        // Two opens, two closes: the drive's count returns to zero.
        assert_eq!(s.drives.drive(0).unwrap().open_ct(), 0);
    }

    #[test]
    fn unknown_minor_maps_to_no_extent() {
        let mut s = WiniBlock::new();
        s.install_probe(0, probed_with_partition());
        // Drive index >= MAX_DRIVES (minor 20 = drive 4) resolves to nothing.
        assert_eq!(s.partition(DeviceMinor(20)), None);
        // A synthetic check keeping the helpers referenced.
        assert_eq!(
            primary_extents(&[entry(0, 0), None, None, None], 10).len(),
            PRIMARY_SLOTS
        );
    }
}
