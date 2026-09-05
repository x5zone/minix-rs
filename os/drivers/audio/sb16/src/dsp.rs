//! Digital-signal-processor commands: reset, version, speed bytes.
//!
//! C correspondence: the port offsets (`DSP_RESET`, `DSP_READ`,
//! `DSP_WRITE`, `DSP_STATUS`, `sb16.h:87-93`, base `0x220`,
//! `sb16.h:20`), the command bytes (`DSP_INPUT_RATE 0x42`,
//! `DSP_OUTPUT_RATE 0x41`, speaker on/off, halt/continue, version
//! `DSP_GET_VERSION 0xE1`, `sb16.h:102-110`), the reset handshake (`drv_reset`,
//! `sb16.c:117-127`: raise reset, lower it, then read `0xAA`),
//! and the speed setup (`dsp_set_speed`, `sb16.c:345-364`: bounds
//! check, then input/output rate command plus high and low bytes).
//!
//! Port traffic stays in the service binary; this module owns the
//! pure command half: which bytes mean what and how a rate splits
//! into two bytes.

/// Expected answer after a reset handshake (`sb16.c:125-127`).
pub const RESET_ANSWER: u8 = 0xAA;

/// Command asking the processor for its version (`GET_VERSION`).
pub const CMD_GET_VERSION: u8 = 0xE1;

/// Command setting the input sample rate (`INPUT_RATE`).
pub const CMD_INPUT_RATE: u8 = 0x42;

/// Command setting the output sample rate (`OUTPUT_RATE`).
pub const CMD_OUTPUT_RATE: u8 = 0x41;

/// Command turning the speaker on (`SPKON`).
pub const CMD_SPEAKER_ON: u8 = 0xD1;

/// Command turning the speaker off (`SPKOFF`).
pub const CMD_SPEAKER_OFF: u8 = 0xD3;

/// Highest rate the card accepts (`sb16.h:167`).
pub const MAX_RATE_HZ: u32 = 44100;

/// Lowest rate the card accepts (`sb16.h:168`).
pub const MIN_RATE_HZ: u32 = 4000;

/// Direction of a speed setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDirection {
    /// Capture: microphone and line input.
    Input,
    /// Playback: speaker and line output.
    Output,
}

/// Command byte selecting the direction (`sb16.c:359-364`).
pub fn rate_command(direction: StreamDirection) -> u8 {
    match direction {
        StreamDirection::Input => CMD_INPUT_RATE,
        StreamDirection::Output => CMD_OUTPUT_RATE,
    }
}

/// Split a rate into the high and low bytes sent after the rate
/// command (`sb16.c:359-364`); `None` when out of band.
pub fn speed_bytes(direction: StreamDirection, rate_hz: u32) -> Option<(u8, u8, u8)> {
    if !(MIN_RATE_HZ..=MAX_RATE_HZ).contains(&rate_hz) {
        return None;
    }
    Some((rate_command(direction), (rate_hz >> 8) as u8, rate_hz as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_bytes_match_header() {
        assert_eq!(CMD_GET_VERSION, 0xE1);
        assert_eq!(CMD_INPUT_RATE, 0x42);
        assert_eq!(CMD_OUTPUT_RATE, 0x41);
        assert_eq!(CMD_SPEAKER_ON, 0xD1);
        assert_eq!(CMD_SPEAKER_OFF, 0xD3);
        assert_eq!(RESET_ANSWER, 0xAA);
    }

    #[test]
    fn test_directions_select_own_commands() {
        assert_eq!(rate_command(StreamDirection::Input), 0x42);
        assert_eq!(rate_command(StreamDirection::Output), 0x41);
    }

    #[test]
    fn test_speed_splits_high_low_bytes() {
        assert_eq!(speed_bytes(StreamDirection::Output, 44100), Some((0x41, 0xAC, 0x44)));
        assert_eq!(speed_bytes(StreamDirection::Input, 4000), Some((0x42, 0x0F, 0xA0)));
    }

    #[test]
    fn test_speed_rejects_outside_band() {
        assert_eq!(speed_bytes(StreamDirection::Output, 3999), None);
        assert_eq!(speed_bytes(StreamDirection::Output, 44101), None);
    }
}
