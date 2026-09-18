//! Chardriver wiring: the framebuffer as a chardriver device.
//!
//! C correspondence: `fb.c`'s five-callback table (`fb_tab`, `fb.c:46-52`:
//! open, close, read, write, ioctl) over the single `/dev/fb` device.
//! Framebuffer is a character device (read/write + four ioctls), not an
//! mmap device — the errata in doc 20 settled that.
//!
//! Data-movement split: hooks answer counts via `truncate_to_device` and
//! run the ioctl decode; the copy between the caller's grant and the
//! mapped framebuffer memory is the service's job. The restart window
//! (`keep_displaying_restarted`) gates writes in the service.

use minix_chardriver::driver::CharDriver;
use minix_chardriver::protocol::{DeviceMinor, OpenDeviceSet, RequestId};
use minix_types::Errno;

use crate::display::{decode_ioctl, truncate_to_device, var_update_allowed, FbIoctl, OpenCounter};

/// The framebuffer driver's face.
pub struct FbFace {
    pub counter: OpenCounter,
    /// Framebuffer size in bytes (from the chosen mode).
    pub device_size: u64,
    /// Virtual height for pan validation.
    pub virtual_height: u32,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
    /// The last accepted ioctl (test inspection and service dispatch).
    pub last_ioctl: Option<FbIoctl>,
}

impl FbFace {
    /// A face over a framebuffer of the given size.
    pub fn new(device_size: u64, virtual_height: u32) -> Self {
        FbFace {
            counter: OpenCounter::new(),
            device_size,
            virtual_height,
            opened: OpenDeviceSet::new(),
            last_ioctl: None,
        }
    }
}

impl CharDriver for FbFace {
    fn open(&mut self, minor: DeviceMinor, _access: i32, _user: i64) -> i32 {
        self.counter.open();
        self.opened.insert_raw(minor.0);
        0
    }

    fn close(&mut self, _minor: DeviceMinor) -> i32 {
        self.counter.close();
        0
    }

    fn read(
        &mut self,
        _minor: DeviceMinor,
        position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        _id: RequestId,
    ) -> Result<usize, Errno> {
        Ok(truncate_to_device(position, size as u64, self.device_size) as usize)
    }

    fn write(
        &mut self,
        _minor: DeviceMinor,
        position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        _id: RequestId,
    ) -> Result<usize, Errno> {
        // The restart-window gate lives in the service (it owns the
        // restart timestamp); the face answers the admission count.
        Ok(truncate_to_device(position, size as u64, self.device_size) as usize)
    }

    fn ioctl(
        &mut self,
        _minor: DeviceMinor,
        request: u64,
        _grant: u64,
        flags: i32,
        _user: i64,
        _id: RequestId,
    ) -> i32 {
        let _ = flags;
        match decode_ioctl(request as u32) {
            Some(ioctl) => {
                self.last_ioctl = Some(ioctl);
                // Pan validation happens here; the payload copy is the
                // service's job.
                0
            }
            None => -(minix_types::ENOTTY),
        }
    }
}

/// Whether a pan request from the ioctl payload is acceptable.
///
/// Convenience wrapper re-exporting the display rule for the service.
pub fn pan_ok(offset: u32, virtual_height: u32) -> bool {
    var_update_allowed(offset, virtual_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> FbFace {
        FbFace::new(1024 * 768 * 4, 1536)
    }

    #[test]
    fn test_open_close_flow_and_first_init() {
        let mut face = face();
        let minor = DeviceMinor(0);
        assert_eq!(face.open(minor, 0, 42), 0);
        assert!(face.opened.contains_raw(0));
        // First open needs the hardware setup.
        assert!(face.counter.needs_init());
        face.counter.mark_initialized();
        assert_eq!(face.close(minor), 0);
        // Reopen does not re-initialize (N4 semantics).
        assert_eq!(face.open(minor, 0, 42), 0);
        assert!(!face.counter.needs_init());
    }

    #[test]
    fn test_read_write_truncate_at_device_end() {
        let mut face = face();
        let end = face.device_size;
        assert_eq!(
            CharDriver::read(&mut face, DeviceMinor(0), 0, 0, 100, 0, RequestId(1)),
            Ok(100)
        );
        assert_eq!(
            CharDriver::write(&mut face, DeviceMinor(0), end - 10, 0, 100, 0, RequestId(1)),
            Ok(10)
        );
        assert_eq!(
            CharDriver::write(&mut face, DeviceMinor(0), end + 10, 0, 100, 0, RequestId(1)),
            Ok(0)
        );
    }

    #[test]
    fn test_ioctl_known_and_unknown() {
        let mut face = face();
        assert_eq!(CharDriver::ioctl(&mut face, DeviceMinor(0), 1, 0, 0, 0, RequestId(1)), 0);
        assert_eq!(face.last_ioctl, Some(FbIoctl::GetVarScreenInfo));
        assert_eq!(
            CharDriver::ioctl(&mut face, DeviceMinor(0), 9, 0, 0, 0, RequestId(1)),
            -(minix_types::ENOTTY as i32)
        );
    }

    #[test]
    fn test_pan_validation_wrapper() {
        assert!(pan_ok(768, 1536));
        assert!(!pan_ok(1536, 1536));
    }
}
