//! Display mode choice: supported list plus monitor intersection.
//!
//! C correspondence: the supported mode table (`omap_supported_modes`
//! with four entries, `fb_arch.c:35-40`: 1024x768, 1280x800,
//! 1400x1050, 1280x720), the intersection pick (`choose_mode`,
//! `fb_arch.c:144-164`, taking the highest resolution shared by the
//! monitor report and the supported table), the monitor-first setup
//! (`configure_with_edid`, `fb_arch.c:166-219`, defaults first then
//! display size, line length, and resolution overwritten), and the
//! defaults-only setup (`configure_with_defaults`, `fb_arch.c:221-233`).
//! The default is 1024x600 at 32 bits per pixel with double buffering
//! (`fb_arch.c:22-25,82-121`).
//!
//! Register writes stay in the service binary; this module owns the
//! pure choice half: which mode wins given a monitor report.

/// Highest resolution the driver prefers when several modes tie.
// Bits per pixel of the default mode (`fb_arch.c:22-25`).
pub const DEFAULT_BITS_PER_PIXEL: u32 = 32;

/// One display mode: resolution in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayMode {
    /// Visible width in pixels.
    pub width: u32,
    /// Visible height in pixels.
    pub height: u32,
}

impl DisplayMode {
    /// Pixel count for comparing resolutions.
    pub fn pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// Modes the hardware supports (`omap_supported_modes`, `fb_arch.c:35-40`).
pub const SUPPORTED_MODES: [DisplayMode; 4] = [
    DisplayMode { width: 1024, height: 768 },
    DisplayMode { width: 1280, height: 800 },
    DisplayMode { width: 1400, height: 1050 },
    DisplayMode { width: 1280, height: 720 },
];

/// Default mode when the monitor report is missing
/// (`fb_arch.c:22-25,82-121`): 1024x600.
pub const DEFAULT_MODE: DisplayMode = DisplayMode { width: 1024, height: 600 };

/// Pick the highest shared resolution (`choose_mode`, `fb_arch.c:144-164`).
///
/// Returns the monitor mode with the most pixels that also appears in
/// the supported table, or `None` when nothing is shared (the caller
/// then falls back to `DEFAULT_MODE`, `fb_arch.c:329-333`).
pub fn choose_mode(monitor: &[DisplayMode]) -> Option<DisplayMode> {
    monitor
        .iter()
        .filter(|mode| SUPPORTED_MODES.contains(mode))
        .max_by_key(|mode| mode.pixels())
        .copied()
}

/// Frame buffer size in bytes (`fb_arch.c:395-401`): virtual width
/// times virtual height times bits per pixel divided by eight.
pub fn frame_buffer_size(width: u32, height: u32, bits_per_pixel: u32) -> u64 {
    width as u64 * height as u64 * bits_per_pixel as u64 / 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_table_holds_four_modes() {
        assert_eq!(SUPPORTED_MODES.len(), 4);
        assert!(SUPPORTED_MODES.contains(&DisplayMode { width: 1280, height: 720 }));
    }

    #[test]
    fn test_choice_picks_highest_shared_mode() {
        let monitor = [
            DisplayMode { width: 1024, height: 768 },
            DisplayMode { width: 1400, height: 1050 },
            DisplayMode { width: 1920, height: 1080 },
        ];
        assert_eq!(
            choose_mode(&monitor),
            Some(DisplayMode { width: 1400, height: 1050 })
        );
    }

    #[test]
    fn test_choice_returns_none_without_overlap() {
        let monitor = [DisplayMode { width: 1920, height: 1080 }];
        assert_eq!(choose_mode(&monitor), None);
        assert_eq!(choose_mode(&[]), None);
    }

    #[test]
    fn test_default_mode_is_1024x600_double_buffered() {
        assert_eq!((DEFAULT_MODE.width, DEFAULT_MODE.height), (1024, 600));
        assert_eq!(DEFAULT_BITS_PER_PIXEL, 32);
        let bytes = frame_buffer_size(1024, 600, 32);
        assert_eq!(bytes, 1024 * 600 * 4);
    }
}
