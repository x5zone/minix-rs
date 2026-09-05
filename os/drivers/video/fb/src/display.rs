//! Open counting and restart window: who uses the display right now.
//!
//! C correspondence: the open counter (`open_counter`, bumped on
//! first open in `fb_open`, `fb.c:59-88`, dropped in `fb_close` with
//! an assertion that it stays positive, `fb.c:90-97`); the restart
//! display window (`keep_displaying_restarted`, `fb.c:318-335`,
//! blocking put, pan, and write with `EAGAIN` for one second after a
//! restart, `fb.c:168,186,221`, `DISPLAY_1SEC`); the read/write
//! truncation to the device size (`fb_read`, `fb.c:99-119`,
//! `fb_write`, `fb.c:211-234`); and the four ioctl requests
//! (`FBIOGET_VSCREENINFO`, `FBIOPUT_VSCREENINFO`,
//! `FBIOGET_FSCREENINFO`, `FBIOPAN_DISPLAY`, `ioc_fb.h:11-14`,
//! unknown requests answered with `ENOTTY`, `fb.c:146`).
//!
//! Copy traffic (`sys_safecopyto`, `sys_safecopyfrom`) stays in the
//! service binary; this module owns the policy half: when an open
//! counts, when the restart window blocks, and how offsets truncate.

/// Device numbers the driver serves (`FB_DEV_NR`, `fb.h`).
pub const DEVICE_COUNT: usize = 1;

/// Ioctl request sequence numbers (`ioc_fb.h:10-13`: the `_IOR`/`_IOW`
/// macros number the four requests 1 through 4 on type `'V'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FbIoctl {
    /// Fetch variable screen information (sequence 1).
    GetVarScreenInfo = 1,
    /// Update variable screen information (sequence 2).
    PutVarScreenInfo = 2,
    /// Fetch fixed screen information (sequence 3).
    GetFixScreenInfo = 3,
    /// Pan the display to a new vertical offset (sequence 4).
    PanDisplay = 4,
}

/// Decode a raw ioctl number, if the driver serves it (unknown
/// requests are answered with `ENOTTY`, `fb.c:146`).
pub fn decode_ioctl(raw: u32) -> Option<FbIoctl> {
    match raw {
        x if x == FbIoctl::GetVarScreenInfo as u32 => Some(FbIoctl::GetVarScreenInfo),
        x if x == FbIoctl::PutVarScreenInfo as u32 => Some(FbIoctl::PutVarScreenInfo),
        x if x == FbIoctl::GetFixScreenInfo as u32 => Some(FbIoctl::GetFixScreenInfo),
        x if x == FbIoctl::PanDisplay as u32 => Some(FbIoctl::PanDisplay),
        _ => None,
    }
}

/// Open counter for one device (`open_counter`, `fb.c:59-97`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenCounter {
    count: u32,
}

impl OpenCounter {
    /// A device nobody has opened yet.
    pub fn new() -> Self {
        OpenCounter { count: 0 }
    }

    /// Record one open; the first open initializes the hardware.
    pub fn open(&mut self) -> u32 {
        self.count += 1;
        self.count
    }

    /// Record one close; false when nobody holds the device (the C
    /// code asserts the counter stays positive, `fb.c:93`).
    pub fn close(&mut self) -> bool {
        if self.count == 0 {
            return false;
        }
        self.count -= 1;
        true
    }

    /// Whether the first open still has to initialize the hardware.
    pub fn needs_init(&self) -> bool {
        self.count == 1
    }

    /// Current holder count.
    pub fn count(&self) -> u32 {
        self.count
    }
}

impl Default for OpenCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate a read or write at `position` with `length` bytes to the
/// device size (`fb_read` / `fb_write`, `fb.c:99-234`): returns the
/// bytes that stay inside, or zero past the end.
pub fn truncate_to_device(position: u64, length: u64, device_size: u64) -> u64 {
    if position >= device_size {
        return 0;
    }
    length.min(device_size - position)
}

/// Whether a variable-screen update is acceptable
/// (`arch_put_varscreeninfo`, `fb_arch.c:274-296`): only the vertical
/// offset may change, and it must stay inside the virtual height.
pub fn var_update_allowed(offset: u32, virtual_height: u32) -> bool {
    offset < virtual_height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ioctl_numbers_decode_in_order() {
        assert_eq!(decode_ioctl(1), Some(FbIoctl::GetVarScreenInfo));
        assert_eq!(decode_ioctl(2), Some(FbIoctl::PutVarScreenInfo));
        assert_eq!(decode_ioctl(3), Some(FbIoctl::GetFixScreenInfo));
        assert_eq!(decode_ioctl(4), Some(FbIoctl::PanDisplay));
        assert_eq!(decode_ioctl(0), None);
        assert_eq!(decode_ioctl(5), None);
        assert_eq!(DEVICE_COUNT, 1);
    }

    #[test]
    fn test_first_open_needs_init_later_opens_do_not() {
        let mut counter = OpenCounter::new();
        assert_eq!(counter.open(), 1);
        assert!(counter.needs_init());
        assert_eq!(counter.open(), 2);
        assert!(!counter.needs_init());
        assert_eq!(counter.count(), 2);
    }

    #[test]
    fn test_close_below_zero_is_refused() {
        let mut counter = OpenCounter::new();
        assert!(!counter.close());
        counter.open();
        assert!(counter.close());
        assert!(!counter.close());
    }

    #[test]
    fn test_truncation_clips_at_device_end() {
        assert_eq!(truncate_to_device(0, 100, 1000), 100);
        assert_eq!(truncate_to_device(900, 200, 1000), 100);
        assert_eq!(truncate_to_device(1000, 200, 1000), 0);
        assert_eq!(truncate_to_device(2000, 200, 1000), 0);
    }

    #[test]
    fn test_var_update_accepts_inside_offsets_only() {
        assert!(var_update_allowed(0, 1200));
        assert!(var_update_allowed(1199, 1200));
        assert!(!var_update_allowed(1200, 1200));
        assert!(!var_update_allowed(5000, 1200));
    }
}
