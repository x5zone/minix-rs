//! Sample-rate policy: bounds, channel routing, converter base.
//!
//! C correspondence: the rate limits (`DEFAULT_RATE 44100`,
//! `MAX_RATE 44100`, `MIN_RATE 4000`, `es1371.h:106-111`), the bounds
//! check (`es1371.c:525`), the channel-to-converter mapping
//! (`es1371.c:530-532`: first analog-to-digital input, first
//! synthesizer output, and second digital-to-analog output each name
//! their sample-rate-converter base register), the converter setup
//! (`src_set_rate`, `sample_rate_converter.c:149`, with the
//! fixed converter rate `SRC_RATE 48000`, `SRC.c:3`), and the
//! converter base addresses (synthesizer `0x70`, digital-to-analog
//! `0x74`, analog-to-digital `0x78`, `sample_rate_converter.h:11-13`).
//!
//! Register writes stay in the service binary; this module owns the
//! pure policy half: which rate is legal and which converter a
//! channel routes to.

/// Default playback and capture rate in hertz (`DEFAULT_RATE`).
pub const DEFAULT_RATE_HZ: u32 = 44100;

/// Highest rate the chip accepts (`MAX_RATE`).
pub const MAX_RATE_HZ: u32 = 44100;

/// Lowest rate the chip accepts (`MIN_RATE`).
pub const MIN_RATE_HZ: u32 = 4000;

/// Fixed internal converter rate in hertz (`SRC_RATE`).
pub const CONVERTER_RATE_HZ: u32 = 48000;

/// Audio channel: which converter a stream routes to
/// (`es1371.c:530-532`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// First analog-to-digital input (capture).
    AdcInput,
    /// Synthesizer output.
    SynthOutput,
    /// Second digital-to-analog output (playback).
    DacOutput,
}

/// Base register of the converter behind one channel
/// (`sample_rate_converter.h:11-13`).
pub fn converter_base(channel: Channel) -> u16 {
    match channel {
        Channel::AdcInput => 0x78,
        Channel::SynthOutput => 0x70,
        Channel::DacOutput => 0x74,
    }
}

/// Whether a requested rate is inside the accepted band
/// (`es1371.c:525`).
pub fn rate_allowed(rate_hz: u32) -> bool {
    (MIN_RATE_HZ..=MAX_RATE_HZ).contains(&rate_hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_band_matches_header_limits() {
        assert_eq!((MIN_RATE_HZ, DEFAULT_RATE_HZ, MAX_RATE_HZ), (4000, 44100, 44100));
        assert_eq!(CONVERTER_RATE_HZ, 48000);
        assert!(rate_allowed(4000));
        assert!(rate_allowed(44100));
    }

    #[test]
    fn test_rate_guard_rejects_outside_band() {
        assert!(!rate_allowed(0));
        assert!(!rate_allowed(3999));
        assert!(!rate_allowed(44101));
        assert!(!rate_allowed(48000));
    }

    #[test]
    fn test_channels_route_to_own_converters() {
        assert_eq!(converter_base(Channel::AdcInput), 0x78);
        assert_eq!(converter_base(Channel::SynthOutput), 0x70);
        assert_eq!(converter_base(Channel::DacOutput), 0x74);
    }
}
