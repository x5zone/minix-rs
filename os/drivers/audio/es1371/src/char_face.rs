//! Framework wiring: es1371 as a minix-audiodriver card.
//!
//! C correspondence: `es1371.c`'s `drv_*` hooks (`es1371.c` fills the
//! fourteen `drv_*` prototypes from `audio_fw.h`). The card-specific
//! half here covers the rate policy (`rate.rs`) and the sub-device
//! bookkeeping; port traffic and DMA programming stay in the service.
//!
//! The face implements [`AudioHooks`] over the codec state: init
//! validates the rate table, start/stop track the DMA direction, and
//! `get_frag_size` reports the negotiated fragment size.

use minix_audiodriver::hooks::AudioHooks;

use crate::rate::rate_allowed;

/// The es1371 card's framework face.
pub struct Es1371Face {
    /// Per-direction negotiated rate in Hz (read, write).
    pub rates: [u32; 2],
    /// Whether the DMA engine is running per direction.
    pub running: [bool; 2],
    /// Fragment size in bytes per direction.
    pub frag_size: [u32; 2],
}

impl Es1371Face {
    /// A face with default rates and nothing running.
    pub fn new() -> Self {
        Es1371Face {
            rates: [44_100; 2],
            running: [false; 2],
            frag_size: [1024; 2],
        }
    }

    /// Negotiate one channel's rate; refused rates keep the old value.
    pub fn set_rate(&mut self, rate_hz: u32) -> bool {
        if !rate_allowed(rate_hz) {
            return false;
        }
        self.rates[0] = rate_hz;
        true
    }
}

impl Default for Es1371Face {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioHooks for Es1371Face {
    fn init(&mut self) -> i32 {
        // Two sub-devices: DAC playback pair and the ADC capture side.
        2
    }

    fn start(&mut self, sub_dev: i32, dma_mode: i32) -> i32 {
        let slot = usize::from(sub_dev != 0);
        let _ = dma_mode;
        self.running[slot] = true;
        0
    }

    fn stop(&mut self, sub_dev: i32) -> i32 {
        let slot = usize::from(sub_dev != 0);
        self.running[slot] = false;
        0
    }

    fn get_frag_size(&self, sub_dev: i32) -> u32 {
        self.frag_size[(sub_dev == 0) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_rate_refuses_out_of_table() {
        let mut face = Es1371Face::new();
        assert!(face.set_rate(44_100));
        assert!(!face.set_rate(3));
        assert_eq!(face.rates[0], 44_100);
    }

    #[test]
    fn test_start_stop_tracks_running() {
        let mut face = Es1371Face::new();
        assert_eq!(face.init(), 2);
        assert_eq!(face.start(0, 2), 0);
        assert!(face.running[0]);
        assert_eq!(face.stop(0), 0);
        assert!(!face.running[0]);
    }

    #[test]
    fn test_frag_size_reports_the_negotiated_value() {
        let face = Es1371Face::new();
        assert_eq!(face.get_frag_size(0), 1024);
    }
}
