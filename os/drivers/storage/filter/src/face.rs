//! The filter block face: the top layer — alignment gate, staging buffer,
//! forwarding loop, and the size control.
//!
//! C correspondence: `filter_open`/`filter_close` (`main.c:94-107`: both
//! no-ops), `filter_transfer` (`main.c:148-196`: total size from the
//! iovec, the sector-alignment gate, the staging buffer, the caller carry
//! in/out, and the `transfer`/`check_driver` retry loop) and
//! `filter_ioctl` (`main.c:200-...`: `DIOCSETP`/`DIOCTIMEOUT`/`DIOCOPENCT`
//! refused, `DIOCGETP` answering the checksum-adjusted size, anything else
//! "inappropriate control"), with `convert` (`sum.c:607-620`) and the
//! group layout from [`crate::checksum`].
//!
//! Registered seam: the middle layer (`driver.c`'s `transfer`: the
//! checksum insert/verify, the mirror member choice, and the asynchronous
//! retry/restart/timeout machinery, `driver.c:1-1051`) sits behind
//! [`LowerLeg`]. The rewrite's production leg forwards synchronously
//! through the block client; the checksum engine and the async driver
//! management are the remaining piece, named here rather than faked — a
//! leg that cannot serve answers an error, never a fabricated transfer.

use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::protocol::{BlockDriverType, DeviceExtent, DeviceMinor, PartitionGeometry};
use minix_driver_rt::plane::TransferPlane;
use minix_types::{EINVAL, EIO, ENOTTY, OK};

use crate::checksum::{GroupLayout, NR_RESTARTS};

/// The staging buffer (`BUF_SIZE`, `inc.h`).
pub const BUF_SIZE: usize = 64 * 4096;
/// Sector size (`SECTOR_SIZE`).
pub const SECTOR_SIZE: u64 = 512;
/// Checksum sectors per group by default (`NR_SUM_SEC`, `main.c:22`).
pub const DEFAULT_NR_SUM_SEC: u32 = 8;

/// What the top layer needs from the middle layer (`driver.c`).
///
/// C's `transfer` takes the whole request, applies the checksum layout and
/// the mirror policy, and talks to the lower drivers with retries; the
/// leg reports how much it moved (or a `REDO`-style signal so the top
/// layer checks the drivers and tries again).
pub trait LowerLeg {
    /// Move one request (`transfer`, `driver.c`). `size` is the request
    /// length; the answer is the number of bytes moved. `Err(RET_REDO)`
    /// asks the top layer to check the drivers and retry.
    fn transfer(
        &mut self,
        position: u64,
        buf: &mut [u8],
        size: u64,
        do_write: bool,
    ) -> Result<u64, i32>;

    /// Check the lower drivers after a `REDO` (`check_driver`,
    /// `driver.c`).
    fn check_drivers(&mut self) -> Result<(), i32>;

    /// The raw partition size (`get_raw_size`, `driver.c`).
    fn raw_size(&mut self) -> u64;
}

/// The retry signal (`RET_REDO`, `inc.h:57`).
pub const RET_REDO: i32 = 1;

/// The filter face.
pub struct FilterFace<L: LowerLeg> {
    leg: L,
    /// Whether the on-disk checksum layout is in use (`USE_SUM_LAYOUT`).
    sum_layout: bool,
    /// Checksum sectors per group (`NR_SUM_SEC`).
    nr_sum_sec: u32,
    /// The staging buffer (`buffer`).
    buf: [u8; BUF_SIZE],
}

impl<L: LowerLeg> FilterFace<L> {
    /// A face over the given middle layer with the documented defaults
    /// (no checksum layout, the standard group size).
    pub fn new(leg: L) -> Self {
        FilterFace {
            leg,
            sum_layout: false,
            nr_sum_sec: DEFAULT_NR_SUM_SEC,
            buf: [0; BUF_SIZE],
        }
    }

    /// Turn the checksum layout on (`layout` on the command line).
    pub fn with_sum_layout(mut self, sum_layout: bool, nr_sum_sec: u32) -> Self {
        self.sum_layout = sum_layout;
        self.nr_sum_sec = nr_sum_sec;
        self
    }

    /// The user-visible size for a raw size (`convert`, `sum.c:607-620`):
    /// every `nr_sum_sec + 1`-th sector is a checksum sector.
    pub fn convert(&self, size: u64) -> u64 {
        if !self.sum_layout {
            return size;
        }
        let sectors = size / SECTOR_SIZE;
        sectors / (self.nr_sum_sec as u64 + 1) * self.nr_sum_sec as u64 * SECTOR_SIZE
    }

    /// The group layout the checksum engine uses (`GroupLayout`).
    pub fn layout(&self) -> GroupLayout {
        GroupLayout::standard()
    }

    /// The restart budget (`NR_RESTARTS`).
    pub fn restarts(&self) -> u32 {
        NR_RESTARTS
    }

    /// The presented disk size (`filter_ioctl`'s `DIOCGETP` answer).
    pub fn presented_size(&mut self) -> u64 {
        let raw = self.leg.raw_size();
        self.convert(raw)
    }

    /// One transfer (`filter_transfer`, `main.c:148-196`).
    pub fn transfer(
        &mut self,
        plane: &mut dyn TransferPlane,
        do_write: bool,
        position: u64,
        count: u64,
    ) -> i64 {
        // The caller's request must be sector-aligned (`main.c:160-164`).
        if !position.is_multiple_of(SECTOR_SIZE) || !count.is_multiple_of(SECTOR_SIZE) {
            return EINVAL as i64;
        }
        if count as usize > BUF_SIZE {
            // C falls back to a heap buffer (`flt_malloc`); the rewrite's
            // staging area is fixed, so an oversized request is refused
            // rather than truncated.
            return -(minix_types::E2BIG as i64);
        }
        let size = count as usize;
        if do_write && let Err(status) = plane.copy_from_caller(0, &mut self.buf[..size]) {
            return -(status.abs() as i64);
        }
        // The retry loop: a `REDO` answer means "check the drivers, then
        // try again" (`main.c:174-186`).
        let moved: u64;
        loop {
            match self
                .leg
                .transfer(position, &mut self.buf[..size], count, do_write)
            {
                Ok(bytes) => {
                    moved = bytes;
                    break;
                }
                Err(RET_REDO) => {
                    if self.leg.check_drivers().is_err() {
                        return EIO as i64;
                    }
                }
                Err(status) => return -(status.abs() as i64),
            }
        }
        if !do_write && let Err(status) = plane.copy_to_caller(0, &self.buf[..moved as usize]) {
            return -(status.abs() as i64);
        }
        moved as i64
    }
}

impl<L: LowerLeg> BlockDriver for FilterFace<L> {
    fn driver_type(&self) -> BlockDriverType {
        // C's table has no type field set; the filter serves one whole
        // device and no partitions (`filter_part` is absent).
        BlockDriverType::Other
    }

    fn open(&mut self, _minor: DeviceMinor, _access: i32) -> i32 {
        // C: "open is a noop for filter" (`main.c:96`).
        OK
    }

    fn close(&mut self, _minor: DeviceMinor) -> i32 {
        OK
    }

    fn transfer(
        &mut self,
        plane: &mut dyn TransferPlane,
        _minor: DeviceMinor,
        do_write: bool,
        position: u64,
        count: u64,
        _flags: i32,
        _id: minix_blockdriver::protocol::RequestId,
    ) -> i64 {
        self.transfer(plane, do_write, position, count)
    }

    fn transfer_vectored(
        &mut self,
        _plane: &mut dyn TransferPlane,
        _minor: DeviceMinor,
        _do_write: bool,
        _position: u64,
        _elements: &[minix_blockdriver::protocol::BlockIovec],
        _flags: i32,
        _id: minix_blockdriver::protocol::RequestId,
    ) -> i64 {
        // The 2.11 evidence (16 篇 §8) rejected per-segment forwarding as
        // observationally unequal to C's one-vector request; the vectored
        // pair stays refused until a vector-capable downstream leg exists.
        -(minix_types::ENOSYS as i64)
    }

    fn ioctl(&mut self, _minor: DeviceMinor, request: u64, _grant: u64, _user: i64) -> i32 {
        // The size control is decoded in the service (the control hook has
        // no data plane); the two "do not make sense for us" controls and
        // the unknown ones are answered here.
        match request {
            DIOCSETP | DIOCTIMEOUT | DIOCOPENCT => EINVAL,
            DIOCGETP => EINVAL,
            _ => ENOTTY,
        }
    }

    fn partition(&mut self, _minor: DeviceMinor) -> Option<DeviceExtent> {
        // The filter presents one device; C registers no partition hook.
        None
    }

    fn geometry(&mut self, _minor: DeviceMinor) -> Option<PartitionGeometry> {
        None
    }
}

/// `DIOCSETP`: set partition geometry (`ioc_disk.h`).
pub const DIOCSETP: u64 = 0x8000_6403;
/// `DIOCGETP`: get partition geometry (the size the filter presents).
pub const DIOCGETP: u64 = 0x4000_6404;
/// `DIOCTIMEOUT`: driver timeout control.
pub const DIOCTIMEOUT: u64 = 0x8000_6405;
/// `DIOCOPENCT`: open count.
pub const DIOCOPENCT: u64 = 0x4004_6407;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_driver_rt::plane::{GrantIpc, NullPlane, TransferPlane};
    use minix_types::{Endpoint, Message};

    /// A leg that answers from a scratch model.
    struct FakeLeg {
        size: u64,
        redo_once: bool,
        checks: usize,
        fail_check: bool,
        moved: Vec<(u64, u64, bool)>,
    }

    impl FakeLeg {
        fn new(size: u64) -> Self {
            FakeLeg {
                size,
                redo_once: false,
                checks: 0,
                fail_check: false,
                moved: Vec::new(),
            }
        }
    }

    impl LowerLeg for FakeLeg {
        fn transfer(
            &mut self,
            position: u64,
            buf: &mut [u8],
            size: u64,
            do_write: bool,
        ) -> Result<u64, i32> {
            self.moved.push((position, size, do_write));
            if self.redo_once {
                self.redo_once = false;
                return Err(RET_REDO);
            }
            if !do_write {
                for (index, byte) in buf.iter_mut().enumerate() {
                    *byte = index as u8;
                }
            }
            Ok(size)
        }
        fn check_drivers(&mut self) -> Result<(), i32> {
            self.checks += 1;
            if self.fail_check {
                return Err(-EIO);
            }
            Ok(())
        }
        fn raw_size(&mut self) -> u64 {
            self.size
        }
    }

    /// A plane over a scratch caller buffer.
    struct FakePlane {
        from_bytes: Vec<u8>,
        to_bytes: Vec<u8>,
    }

    impl FakePlane {
        fn new() -> Self {
            FakePlane {
                from_bytes: Vec::new(),
                to_bytes: Vec::new(),
            }
        }
    }

    impl GrantIpc for FakePlane {
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            _address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            Ok(5)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::ENOSYS)
        }
    }

    impl TransferPlane for FakePlane {
        fn caller(&self) -> Endpoint {
            Endpoint(20)
        }
        fn copy_from_caller(&mut self, _offset: u64, buf: &mut [u8]) -> Result<(), i32> {
            buf.copy_from_slice(&self.from_bytes[..buf.len()]);
            Ok(())
        }
        fn copy_to_caller(&mut self, _offset: u64, buf: &[u8]) -> Result<(), i32> {
            self.to_bytes.extend_from_slice(buf);
            Ok(())
        }
    }

    #[test]
    fn test_open_and_close_are_noops() {
        let mut face = FilterFace::new(FakeLeg::new(0));
        assert_eq!(face.open(DeviceMinor(0), 2), OK);
        assert_eq!(face.close(DeviceMinor(0)), OK);
        assert_eq!(face.driver_type(), BlockDriverType::Other);
        assert!(face.partition(DeviceMinor(0)).is_none());
    }

    #[test]
    fn test_unaligned_requests_are_refused() {
        let mut face = FilterFace::new(FakeLeg::new(0));
        let mut plane = FakePlane::new();
        assert_eq!(face.transfer(&mut plane, false, 1, 512), EINVAL as i64);
        assert_eq!(face.transfer(&mut plane, false, 0, 100), EINVAL as i64);
    }

    #[test]
    fn test_a_read_copies_the_legs_bytes_back() {
        let mut face = FilterFace::new(FakeLeg::new(0));
        let mut plane = FakePlane::new();
        assert_eq!(face.transfer(&mut plane, false, 0x2000, 512), 512);
        assert_eq!(plane.to_bytes.len(), 512);
        assert_eq!(plane.to_bytes[3], 3);
    }

    #[test]
    fn test_a_write_carries_the_callers_bytes_in() {
        let mut face = FilterFace::new(FakeLeg::new(0));
        let mut plane = FakePlane::new();
        plane.from_bytes = vec![9u8; 512];
        assert_eq!(face.transfer(&mut plane, true, 0, 512), 512);
        assert_eq!(face.leg.moved, vec![(0, 512, true)]);
        assert_eq!(face.buf[..4], [9, 9, 9, 9]);
    }

    #[test]
    fn test_a_redo_checks_the_drivers_and_retries() {
        let mut leg = FakeLeg::new(0);
        leg.redo_once = true;
        let mut face = FilterFace::new(leg);
        let mut plane = FakePlane::new();
        assert_eq!(face.transfer(&mut plane, false, 0, 512), 512);
        assert_eq!(face.leg.checks, 1, "the drivers were checked once");
        assert_eq!(face.leg.moved.len(), 2, "the request was retried");

        // A failed check ends the request with an I/O error.
        let mut leg = FakeLeg::new(0);
        leg.redo_once = true;
        leg.fail_check = true;
        let mut face = FilterFace::new(leg);
        let mut plane = FakePlane::new();
        assert_eq!(face.transfer(&mut plane, false, 0, 512), EIO as i64);
    }

    #[test]
    fn test_convert_matches_the_checksum_layout() {
        // Without the layout the size is untouched.
        let face = FilterFace::new(FakeLeg::new(0));
        assert_eq!(face.convert(4096), 4096);
        // With it, every ninth sector is a checksum sector.
        let face = FilterFace::new(FakeLeg::new(0)).with_sum_layout(true, 8);
        assert_eq!(face.convert(9 * 512), 8 * 512);
        assert_eq!(face.convert(18 * 512), 16 * 512);
        // The size control reports the converted raw size.
        let mut face = FilterFace::new(FakeLeg::new(9 * 512)).with_sum_layout(true, 8);
        assert_eq!(face.presented_size(), 8 * 512);
        assert_eq!(face.restarts(), NR_RESTARTS);
        // A group stores its data sectors plus the one checksum sector.
        assert_eq!(
            face.layout().stored_sectors(),
            crate::checksum::SECTORS_PER_GROUP + 1
        );
    }

    #[test]
    fn test_the_controls_answer_like_c() {
        let mut face = FilterFace::new(FakeLeg::new(0));
        assert_eq!(face.ioctl(DeviceMinor(0), DIOCSETP, 0, 0), EINVAL);
        assert_eq!(face.ioctl(DeviceMinor(0), DIOCTIMEOUT, 0, 0), EINVAL);
        assert_eq!(face.ioctl(DeviceMinor(0), DIOCOPENCT, 0, 0), EINVAL);
        assert_eq!(face.ioctl(DeviceMinor(0), 0x1234, 0, 0), ENOTTY);
        // The vectored pair is refused (the per-segment equivalence was
        // rejected by the 2.11 evidence; see the `transfer_vectored`
        // override).
        let mut null = NullPlane;
        assert_eq!(
            BlockDriver::transfer_vectored(
                &mut face,
                &mut null,
                DeviceMinor(0),
                false,
                0,
                &[],
                0,
                minix_blockdriver::protocol::RequestId(1)
            ),
            -(minix_types::ENOSYS as i64)
        );
    }
}
