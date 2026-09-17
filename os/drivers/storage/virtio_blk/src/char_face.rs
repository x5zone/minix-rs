//! Blockdriver wiring: the virtio_blk driver as a blockdriver device.
//!
//! C correspondence: `virtio_blk.c`'s `bdr_*` table — open (`virtio_blk.c:
//! 265-280`), transfer planning (`virtio_blk.c:283-364`), status
//! translation (`virtio_blk.c:549-563`), and the read-only feature gate.
//!
//! Data-movement split: the hooks plan chains and answer counts; the
//! descriptor chains, queue traffic, and host notifications are the
//! service's virtio transport (`minix-virtio` policy + service queues).

use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::protocol::DeviceMinor;
use minix_chardriver::protocol::OpenDeviceSet;

use crate::geometry::{DriveGeometry, OpenCount};
use crate::request::{plan_transfer, status_to_code, ChainPlan};

/// The virtio_blk driver's block face over its geometry and open count.
pub struct VirtioBlkFace {
    pub geometry: DriveGeometry,
    pub opens: OpenCount,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
    /// The last planned chain (the service builds descriptors from it).
    pub last_plan: Option<ChainPlan>,
    /// Whether the device advertised the read-only feature.
    pub read_only: bool,
    /// Whether the device advertised the flush feature.
    pub flush: bool,
}

impl VirtioBlkFace {
    /// A face over a device with the given sector capacity.
    pub fn new(sectors: u64, read_only: bool, flush: bool) -> Self {
        VirtioBlkFace {
            geometry: DriveGeometry { sectors, read_only },
            opens: OpenCount::new(),
            opened: OpenDeviceSet::new(),
            last_plan: None,
            read_only,
            flush,
        }
    }
}

/// Partition capacity in bytes the geometry reports.
const fn capacity_bytes(geometry: &DriveGeometry) -> u64 {
    geometry.bytes()
}

impl BlockDriver for VirtioBlkFace {
    fn open(&mut self, minor: DeviceMinor, _access: i32) -> i32 {
        self.opens = self.opens.clone().opened();
        self.opened.insert_raw(minor.0);
        0
    }

    fn close(&mut self, _minor: DeviceMinor) -> i32 {
        self.opens = self.opens.clone().closed();
        0
    }

    fn transfer(
        &mut self,
        _minor: DeviceMinor,
        do_write: bool,
        position: u64,
        count: u64,
        _flags: i32,
        _id: minix_blockdriver::protocol::RequestId,
        _vectored: bool,
    ) -> i64 {
        // Read-only gate comes first (writes refused before planning).
        if do_write && self.read_only {
            return -(minix_types::EROFS as i64);
        }
        match plan_transfer(position, count, capacity_bytes(&self.geometry), do_write) {
            Ok(plan) => {
                self.last_plan = Some(plan);
                plan.bytes as i64
            }
            Err(code) => code as i64,
        }
    }

    fn ioctl(&mut self, _minor: DeviceMinor, request: u64, _grant: u64, _user: i64) -> i32 {
        // Flush rides the feature gate: without the feature, refuse.
        let _ = request;
        if self.flush {
            0
        } else {
            -(minix_types::EIO as i32)
        }
    }

    fn partition(&mut self, _minor: DeviceMinor) -> Option<minix_blockdriver::protocol::DeviceExtent> {
        Some(minix_blockdriver::protocol::DeviceExtent {
            base: 0,
            size: capacity_bytes(&self.geometry),
        })
    }
}

/// Translate a device status (re-exported at the face for the service).
pub use crate::request::status_to_code as face_status_to_code;

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> VirtioBlkFace {
        // 1024 sectors = 512 KiB, read-write with flush.
        VirtioBlkFace::new(1024, false, true)
    }

    #[test]
    fn test_open_close_flow_through_the_counter() {
        let mut face = face();
        assert_eq!(face.open(DeviceMinor(0), 0), 0);
        assert_eq!(face.opens.count(), 1);
        assert!(face.opened.contains_raw(0));
        assert_eq!(face.close(DeviceMinor(0)), 0);
    }

    #[test]
    fn test_transfer_plans_and_answers_bytes() {
        let mut face = face();
        face.open(DeviceMinor(0), 0);
        let moved = BlockDriver::transfer(
            &mut face,
            DeviceMinor(0),
            true,
            0,
            4096,
            0,
            minix_blockdriver::protocol::RequestId(1),
            false,
        );
        assert_eq!(moved, 4096);
        let plan = face.last_plan.unwrap();
        assert_eq!(plan.sector, 0);
        assert_eq!(plan.bytes, 4096);
        // Straddling the end truncates to the sector-aligned remainder.
        let moved = BlockDriver::transfer(
            &mut face,
            DeviceMinor(0),
            true,
            (1024 - 2) * 512,
            4096,
            0,
            minix_blockdriver::protocol::RequestId(1),
            false,
        );
        assert_eq!(moved, 1024);
    }

    #[test]
    fn test_read_only_device_refuses_writes() {
        let mut face = VirtioBlkFace::new(1024, true, false);
        face.open(DeviceMinor(0), 0);
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                DeviceMinor(0),
                true,
                0,
                512,
                0,
                minix_blockdriver::protocol::RequestId(1),
                false,
            ),
            -(minix_types::EROFS as i64)
        );
        // Reads still work.
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                DeviceMinor(0),
                false,
                0,
                512,
                0,
                minix_blockdriver::protocol::RequestId(1),
                false,
            ),
            512
        );
    }

    #[test]
    fn test_flush_gates_the_ioctl() {
        let mut face = face();
        assert_eq!(face.ioctl(DeviceMinor(0), 0x9001, 0, 0), 0);
        let mut ro = VirtioBlkFace::new(1024, true, false);
        assert_eq!(ro.ioctl(DeviceMinor(0), 0x9001, 0, 0), -(minix_types::EIO as i32));
    }
}
