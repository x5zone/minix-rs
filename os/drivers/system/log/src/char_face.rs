//! Chardriver wiring: the log driver as a chardriver device.
//!
//! C correspondence: `log_dtab` (`log.c:29-39` — open, read, write,
//! cancel, select over the single `/dev/klog` minor) plus the write tail
//! (`log.c:171-191`).
//!
//! Data-movement split: the hooks answer counts from the ring cursors
//! and park/wake through the device; the byte copies between the
//! writer's grant and the service-owned ring bytes are the service's
//! job. Byte content lives outside the library (the ring is a counter
//! pair by design, see doc 08 §3.1), so the face exercises the same
//! cursor policy without needing storage.

use minix_chardriver::driver::CharDriver;
use minix_chardriver::protocol::{DeviceMinor, OpenDeviceSet, RequestId};
use minix_types::Errno;

use crate::device::{LogDevice, ReadVerdict};

/// The log driver's chardriver face over one device.
pub struct LogFace {
    pub device: LogDevice,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
    /// Bytes the last write hook admitted (the service then copies that
    /// much from the caller's grant into the ring bytes).
    pub last_admitted: u64,
}

impl LogFace {
    /// A fresh face over an empty device.
    pub fn new() -> Self {
        LogFace {
            device: LogDevice::new(),
            opened: OpenDeviceSet::new(),
            last_admitted: 0,
        }
    }
}

impl Default for LogFace {
    fn default() -> Self {
        Self::new()
    }
}

impl CharDriver for LogFace {
    fn open(&mut self, minor: DeviceMinor, _access: i32, _user: i64) -> i32 {
        let result = LogDevice::open(minor.0);
        if result == 0 {
            self.opened.insert_raw(minor.0);
        }
        result
    }

    fn read(
        &mut self,
        minor: DeviceMinor,
        position: u64,
        _grant: u64,
        size: usize,
        flags: i32,
        id: RequestId,
    ) -> Result<usize, Errno> {
        let _ = position;
        match self.device.read(minor.0, size as u64, 0, id.0, flags != 0) {
            // Bytes available: the service copies them from the ring
            // bytes into the caller's grant.
            Ok(ReadVerdict::Answer(count)) => Ok(count as usize),
            // The parked read answers through the wake-up channel; on the
            // typed lane that is the EDONTREPLY sentinel.
            Ok(ReadVerdict::Park) => Err(Errno::from_i32(minix_types::EDONTREPLY)),
            // The device speaks the C negative-errno dialect; the error
            // lane wants the positive errno.
            Err(code) => Err(Errno::from_i32(-code)),
        }
    }

    fn write(
        &mut self,
        minor: DeviceMinor,
        _position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        _id: RequestId,
    ) -> Result<usize, Errno> {
        // Admission: the ring notes the write (cursor arithmetic and the
        // overflow eviction happen in `ring_mut`), the service then
        // copies the bytes and runs `after_write` for the wake plan.
        if minor.0 != crate::device::KLOG_MINOR {
            return Err(Errno::from_i32(minix_types::EIO));
        }
        self.device.ring_mut().note_written(size);
        self.last_admitted = size as u64;
        Ok(size)
    }

    fn cancel(&mut self, minor: DeviceMinor, id: RequestId) -> i32 {
        match self.device.cancel(minor.0, 0, id.0) {
            Ok(Some(_)) => -minix_types::EINTR, // the parked read answers "interrupted"
            Ok(None) => minix_chardriver::protocol::NO_CANCEL_HOOK, // "let it finish"
            Err(code) => code,
        }
    }

    fn select(&mut self, minor: DeviceMinor, ops: u32) -> i32 {
        match self.device.select(minor.0, ops, 0) {
            Ok(ready) => ready as i32,
            Err(code) => code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> LogFace {
        LogFace::new()
    }

    #[test]
    fn test_open_only_klog_minor() {
        let mut face = face();
        assert_eq!(face.open(DeviceMinor(0), 0, 42), 0);
        assert!(face.opened.contains_raw(0));
        assert_eq!(face.open(DeviceMinor(1), 0, 42), -minix_types::ENXIO);
    }

    #[test]
    fn test_write_admits_then_read_answers() {
        let mut face = face();
        face.open(DeviceMinor(0), 0, 42);
        let moved = CharDriver::write(
            &mut face,
            DeviceMinor(0),
            0,
            0,
            100,
            0,
            RequestId(1),
        );
        assert_eq!(moved, Ok(100));
        assert_eq!(face.last_admitted, 100);
        // Data waits: the reader takes up to `size` bytes from the ring.
        let got = CharDriver::read(
            &mut face,
            DeviceMinor(0),
            0,
            0,
            40,
            0,
            RequestId(2),
        );
        assert_eq!(got, Ok(40));
    }

    #[test]
    fn test_empty_read_parks_then_write_admits() {
        let mut face = face();
        face.open(DeviceMinor(0), 0, 42);
        // Empty: the read parks (the EDONTREPLY sentinel on the error
        // lane).
        assert_eq!(
            CharDriver::read(&mut face, DeviceMinor(0), 0, 0, 40, 0, RequestId(2)),
            Err(Errno::from_i32(minix_types::EDONTREPLY))
        );
        // A second reader while one hangs answers zero (log_read rule).
        assert_eq!(
            CharDriver::read(&mut face, DeviceMinor(0), 0, 0, 40, 0, RequestId(3)),
            Ok(0)
        );
        // Bytes land; the wake plan carries the first reader.
        face.device.ring_mut().note_written(50);
        let plan = face.device.after_write();
        assert_eq!(plan.reader, Some((0, 2, 40)));
    }
}
