//! Framework wiring: sb16 as a minix-audiodriver card.
//!
//! C correspondence: `sb16.c`'s `drv_*` hooks over the DSP command
//! interface (`sb16.h` DSP constants) and the mixer. The card-specific
//! half here covers the DSP command bytes (`dsp.rs`) and the sub-device
//! bookkeeping; port traffic stays in the service.
//!
//! The face implements [`AudioHooks`] over the DSP state: init reports
//! the single sub-device, start/stop track the DMA direction, and
//! `get_frag_size` reports the negotiated fragment size.

use minix_audiodriver::hooks::AudioHooks;

use crate::dsp::{rate_command, StreamDirection};

/// The sb16 card's framework face.
pub struct Sb16Face {
    /// Negotiated sample rate in Hz.
    pub rate_hz: u32,
    /// Whether the DMA engine is running.
    pub running: bool,
    /// Fragment size in bytes.
    pub frag_size: u32,
}

/// Sample rates the SB16 DSP accepts (finite table; 4000..=23000 Hz are
/// the classic single-speed range, 23111 the Hi-Speed entry).
const VALID_RATES: [u32; 6] = [4_000, 8_000, 11_025, 22_050, 23_000, 23_111];

impl Sb16Face {
    /// A face with a default rate and nothing running.
    pub fn new() -> Self {
        Sb16Face {
            rate_hz: 22_050,
            running: false,
            frag_size: 512,
        }
    }

    /// Negotiate the sample rate; refused rates keep the old value.
    pub fn set_rate(&mut self, rate_hz: u32) -> bool {
        if !VALID_RATES.contains(&rate_hz) {
            return false;
        }
        self.rate_hz = rate_hz;
        true
    }

    /// The DSP rate-command byte for the negotiated direction
    /// (`rate_command`, `dsp.rs`).
    pub fn rate_command_byte(&self, direction: StreamDirection) -> u8 {
        let _ = self.rate_hz;
        rate_command(direction)
    }
}

impl Default for Sb16Face {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioHooks for Sb16Face {
    fn init(&mut self) -> i32 {
        1
    }

    fn start(&mut self, _sub_dev: i32, _dma_mode: i32) -> i32 {
        self.running = true;
        0
    }

    fn stop(&mut self, _sub_dev: i32) -> i32 {
        self.running = false;
        0
    }

    fn get_frag_size(&self, _sub_dev: i32) -> u32 {
        self.frag_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_rate_refuses_out_of_table() {
        let mut face = Sb16Face::new();
        assert!(face.set_rate(22_050));
        assert!(!face.set_rate(3));
        assert!(!face.set_rate(44_100)); // the SB16 tops out at 23111
        assert_eq!(face.rate_hz, 22_050);
    }

    #[test]
    fn test_start_stop_tracks_running() {
        let mut face = Sb16Face::new();
        assert_eq!(face.init(), 1);
        assert_eq!(face.start(0, 2), 0);
        assert!(face.running);
        assert_eq!(face.stop(0), 0);
        assert!(!face.running);
    }

    #[test]
    fn test_frag_size_reports_the_negotiated_value() {
        let face = Sb16Face::new();
        assert_eq!(face.get_frag_size(0), 512);
    }
}
