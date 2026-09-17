//! Block-face wiring: the memory driver as a blockdriver device.
//!
//! C correspondence: `m_block_open`/`m_block_close`/`m_block_transfer`/
//! `m_block_ioctl`/`m_block_part` (`memory.c`, registered in `m_bdtab`)
//! serving the RAM minors and the image disk (`/dev/ram0..5`,
//! `/dev/imgrd`).
//!
//! Data-movement split (same as the character face): hooks answer COUNTS
//! and move bytes between the driver's own backend and an internal
//! staging slice; the caller-grant side of the copy is the service's job
//! (plan-guided, transport-executed). A request reaching past the device
//! end is clipped to the surviving bytes (`m_block_transfer`'s
//! "copy what fits" behavior); a request entirely past the end answers
//! zero.

use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::protocol::{DeviceExtent, DeviceMinor};
use minix_chardriver::protocol::OpenDeviceSet;

use crate::device::{resize_policy, DeviceTable, ResizeVerdict};

/// Backing storage of the RAM minors (the driver-owned disk memory).
pub trait RamBackend {
    /// Read `buf.len()` bytes of disk memory at `offset`.
    fn read_disk(&mut self, offset: u64, buf: &mut [u8]);
    /// Write `buf` bytes of disk memory at `offset`.
    fn write_disk(&mut self, offset: u64, buf: &[u8]);
    /// Current disk size in bytes (resizes grow the storage).
    fn disk_size(&self) -> u64;
    /// Grow the disk to `size` bytes (zero-filling the growth).
    fn set_disk_size(&mut self, size: u64);
}

/// A byte-vector RAM disk.
#[derive(Debug, Default, Clone)]
pub struct VecRamDisk(pub alloc::vec::Vec<u8>);

impl RamBackend for VecRamDisk {
    fn read_disk(&mut self, offset: u64, buf: &mut [u8]) {
        let start = offset as usize;
        if let Some(slice) = self.0.get_mut(start..start + buf.len()) {
            buf.copy_from_slice(slice);
        }
    }

    fn write_disk(&mut self, offset: u64, buf: &[u8]) {
        let start = offset as usize;
        if let Some(slice) = self.0.get_mut(start..start + buf.len()) {
            slice.copy_from_slice(buf);
        }
    }

    fn disk_size(&self) -> u64 {
        self.0.len() as u64
    }

    fn set_disk_size(&mut self, size: u64) {
        self.0.resize(size as usize, 0);
    }
}

/// The memory driver's block face over a RAM backend.
pub struct MemoryBlock<B: RamBackend> {
    pub table: DeviceTable,
    pub backend: B,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
    /// Recorded resize verdicts from the last control (test inspection).
    pub last_resize: Option<ResizeVerdict>,
}

impl<B: RamBackend> MemoryBlock<B> {
    /// Fresh block face over the given backend.
    pub fn new(backend: B) -> Self {
        MemoryBlock {
            table: DeviceTable::fresh(),
            backend,
            opened: OpenDeviceSet::new(),
            last_resize: None,
        }
    }

    /// Execute the backend half of one read: fill `buf` from disk memory.
    pub fn execute_read(&mut self, minor: DeviceMinor, position: u64, buf: &mut [u8]) {
        let _ = minor;
        self.backend.read_disk(position, buf);
    }

    /// Execute the backend half of one write.
    pub fn execute_write(&mut self, minor: DeviceMinor, position: u64, buf: &[u8]) {
        let _ = minor;
        self.backend.write_disk(position, buf);
    }
}

impl<B: RamBackend> BlockDriver for MemoryBlock<B> {
    fn open(&mut self, minor: DeviceMinor, _access: i32) -> i32 {
        let result = self.table.open(minor.0, true);
        if result == 0 {
            self.opened.insert_raw(minor.0);
        }
        result
    }

    fn close(&mut self, minor: DeviceMinor) -> i32 {
        self.table.close(minor.0, true)
    }

    fn transfer(
        &mut self,
        minor: DeviceMinor,
        _do_write: bool,
        position: u64,
        count: u64,
        _flags: i32,
        _id: minix_blockdriver::protocol::RequestId,
        _vectored: bool,
    ) -> i64 {
        // Clip against the device extent: bytes past the end answer as a
        // shorter transfer ("copy what fits"), entirely-past answers
        // zero (`m_block_transfer`, `memory.c` block path).
        let Some(extent) = self.table.extent(minor.0) else {
            return -(minix_types::ENXIO as i64);
        };
        if position >= extent.base + extent.size {
            return 0;
        }
        let start = position.max(extent.base);
        let room = extent.base + extent.size - start;
        (count.min(room)) as i64
    }

    fn ioctl(&mut self, minor: DeviceMinor, request: u64, _grant: u64, _user: i64) -> i32 {
        // MIOCRAMSIZE rides the resize policy; the value arrives via the
        // grant and is copied by the service, which then calls
        // `apply_resize` with the decoded want.
        let _ = (minor, request);
        minix_types::ENOTTY
    }

    fn partition(&mut self, minor: DeviceMinor) -> Option<DeviceExtent> {
        self.table
            .extent(minor.0)
            .map(|extent| DeviceExtent { base: extent.base, size: extent.size })
    }

    fn geometry(&mut self, _minor: DeviceMinor) -> Option<minix_blockdriver::protocol::PartitionGeometry> {
        None
    }
}

impl<B: RamBackend> MemoryBlock<B> {
    /// Apply a resize control once the service decoded the wanted size:
    /// policy first (`resize_policy`), then the backend growth.
    ///
    /// C: `m_block_ioctl` (`memory.c:512-599`): non-RAM minors and bad
    /// arguments answer "invalid argument", a disk opened by somebody
    /// else answers "busy", no-change answers success, and a real resize
    /// tears down the old backing and allocates the new size.
    pub fn apply_resize(&mut self, minor: DeviceMinor, is_image: bool, want: u64) -> i32 {
        let verdict = resize_policy(
            minor.0,
            is_image,
            self.table.open_count(minor.0),
            self.backend.disk_size(),
            want,
        );
        self.last_resize = Some(verdict);
        match verdict {
            // Success (resized or already there): the extent table records
            // the size — C's resize rewrites `dv_size` (memory.c:580-590).
            ResizeVerdict::Resize | ResizeVerdict::NoChange => {
                self.backend.set_disk_size(want);
                self.table.set_extent(
                    minor.0,
                    crate::device::DeviceExtent { base: 0, size: want },
                );
                0
            }
            ResizeVerdict::NotRamdisk => -minix_types::EINVAL,
            ResizeVerdict::Busy => -minix_types::EBUSY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> MemoryBlock<VecRamDisk> {
        let mut disk = VecRamDisk::default();
        disk.0.resize(4096, 0xEE);
        MemoryBlock::new(disk)
    }

    #[test]
    fn test_block_open_close_flow_through_the_table() {
        let mut face = face();
        let minor = DeviceMinor(7); // /dev/ram0, block face
        assert_eq!(face.open(minor, 0), 0);
        assert_eq!(face.table.open_count(7), 1);
        assert!(face.opened.contains_raw(7));
        assert_eq!(face.close(minor), 0);
        assert_eq!(face.table.open_count(7), 0);
    }

    #[test]
    fn test_transfer_clips_at_device_end() {
        let mut face = face();
        face.open(DeviceMinor(7), 0);
        // A RAM minor has no storage until its resize control lands: give
        // it 4 KiB (the control answers success and records the extent).
        assert_eq!(face.apply_resize(DeviceMinor(7), false, 4096), 0);
        // Whole request inside: moves everything.
        assert_eq!(
            BlockDriver::transfer(&mut face, DeviceMinor(7), true, 4096 - 512, 512, 0, minix_blockdriver::protocol::RequestId(1), false),
            512
        );
        // Straddles the end: clipped to the surviving bytes.
        assert_eq!(
            BlockDriver::transfer(&mut face, DeviceMinor(7), true, 4096 - 100, 512, 0, minix_blockdriver::protocol::RequestId(1), false),
            100
        );
        // Entirely past the end: zero moved.
        assert_eq!(
            BlockDriver::transfer(&mut face, DeviceMinor(7), true, 8192, 512, 0, minix_blockdriver::protocol::RequestId(1), false),
            0
        );
    }

    #[test]
    fn test_execute_round_trip_through_backend() {
        let mut face = face();
        face.open(DeviceMinor(7), 0);
        let mut buf = [0u8; 4];
        face.execute_read(DeviceMinor(7), 0, &mut buf);
        assert_eq!(buf, [0xEE; 4]);
        face.execute_write(DeviceMinor(7), 0, &[1, 2, 3, 4]);
        face.execute_read(DeviceMinor(7), 0, &mut buf);
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    #[test]
    fn test_resize_policy_gates_the_backend_growth() {
        let mut face = face();
        face.open(DeviceMinor(7), 0);
        // The control itself holds the only open: resize allowed.
        assert_eq!(face.apply_resize(DeviceMinor(7), false, 8192), 0);
        assert_eq!(face.backend.disk_size(), 8192);
        // Non-RAM minors refuse.
        assert_eq!(face.apply_resize(DeviceMinor(1), false, 8192), -minix_types::EINVAL);
        assert_eq!(face.last_resize, Some(ResizeVerdict::NotRamdisk));
    }
}
