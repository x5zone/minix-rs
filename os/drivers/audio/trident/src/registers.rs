//! The Trident register map: offsets, command bits, and PCI identifiers.
//!
//! C correspondence: `minix3/minix/drivers/audio/trident/trident.h` — the
//! sub-device numbers and PCI identifiers (`:17-24`), the interrupt
//! status bits (`:32-33`), the register offsets (`:36-60`), the codec
//! busy bit (`:62`), and the format bits (`:64-66`).

/// PCI vendor identifier (`VENDOR_ID`).
pub const VENDOR_ID: u16 = 0x1023;
/// PCI device identifier (`DEVICE_ID`).
pub const DEVICE_ID: u16 = 0x2000;
/// The driver's name (`DRIVER_NAME`).
pub const DRIVER_NAME: &str = "Trident";

/// The playback sub-device (`DAC`).
pub const DAC: i32 = 0;
/// The capture sub-device (`ADC`).
pub const ADC: i32 = 1;
/// The mixer sub-device (`MIX`).
pub const MIX: i32 = 2;

/// Read a volume (`GET_VOL`).
pub const GET_VOL: u32 = 0;
/// Set a volume (`SET_VOL`).
pub const SET_VOL: u32 = 1;
/// Enable the interrupt (`INTR_ENABLE`).
pub const INTR_ENABLE: u32 = 1;
/// Disable the interrupt (`INTR_DISABLE`).
pub const INTR_DISABLE: u32 = 0;

/// The playback interrupt bit (`INTR_STS_DAC`).
pub const INTR_STS_DAC: u32 = 0x0020;
/// The capture interrupt bit (`INTR_STS_ADC`).
pub const INTR_STS_ADC: u32 = 0x0004;

/// Playback DMA address, low half (`REG_DMA0`).
pub const REG_DMA0: u16 = 0x00;
/// Playback DMA length and flags (`REG_DMA4`).
pub const REG_DMA4: u16 = 0x04;
/// Playback DMA control (`REG_DMA6`).
pub const REG_DMA6: u16 = 0x06;
/// Capture DMA control (`REG_DMA11`).
pub const REG_DMA11: u16 = 0x0b;
/// Capture DMA enable (`REG_DMA15`).
pub const REG_DMA15: u16 = 0x0f;
/// Codec write port (`REG_CODEC_WRITE`).
pub const REG_CODEC_WRITE: u16 = 0x40;
/// Codec read port (`REG_CODEC_READ`).
pub const REG_CODEC_READ: u16 = 0x44;
/// Codec control (`REG_CODEC_CTRL`).
pub const REG_CODEC_CTRL: u16 = 0x48;
/// Global control (`REG_GCTRL`).
pub const REG_GCTRL: u16 = 0xa0;
/// Sound Blaster delta (`REG_SB_DELTA`).
pub const REG_SB_DELTA: u16 = 0xac;
/// Sound Blaster base (`REG_SB_BASE`).
pub const REG_SB_BASE: u16 = 0xc0;
/// Sound Blaster control (`REG_SB_CTRL`).
pub const REG_SB_CTRL: u16 = 0xc4;
/// The channel register block (`REG_CHAN_BASE`).
pub const REG_CHAN_BASE: u16 = 0xe0;
/// Interrupt status (`REG_INTR_STS`).
pub const REG_INTR_STS: u16 = 0xb0;

/// Playback start (`REG_START_A`).
pub const REG_START_A: u16 = 0x80;
/// Playback stop (`REG_STOP_A`).
pub const REG_STOP_A: u16 = 0x84;
/// Playback sample counter (`REG_CSPF_A`).
pub const REG_CSPF_A: u16 = 0x90;
/// Playback interrupt address (`REG_ADDR_INT_A`).
pub const REG_ADDR_INT_A: u16 = 0x98;
/// Playback interrupt control (`REG_INTR_CTRL_A`).
pub const REG_INTR_CTRL_A: u16 = 0xa4;
/// Capture start (`REG_START_B`).
pub const REG_START_B: u16 = 0xb4;
/// Capture stop (`REG_STOP_B`).
pub const REG_STOP_B: u16 = 0xb8;
/// Capture sample counter (`REG_CSPF_B`).
pub const REG_CSPF_B: u16 = 0xbc;
/// Capture interrupt address (`REG_ADDR_INT_B`).
pub const REG_ADDR_INT_B: u16 = 0xd8;
/// Capture interrupt control (`REG_INTR_CTRL_B`).
pub const REG_INTR_CTRL_B: u16 = 0xdc;

/// The codec reports a transfer in progress (`STS_CODEC_BUSY`).
pub const STS_CODEC_BUSY: u32 = 0x8000;

/// Format: sixteen-bit samples (`CMD_FORMAT_BIT16`).
pub const CMD_FORMAT_BIT16: u32 = 0x08;
/// Format: signed samples (`CMD_FORMAT_SIGN`).
pub const CMD_FORMAT_SIGN: u32 = 0x02;
/// Format: stereo (`CMD_FORMAT_STEREO`).
pub const CMD_FORMAT_STEREO: u32 = 0x04;

/// How many codec polls each wait allows (`trident.c:62`, `:81`).
pub const CODEC_POLL_LIMIT: u32 = 50000;

/// The channel's fixed parameters (`dev_start_channel`,
/// `trident.c:131-136`).
pub const CHAN_FMC: u32 = 3;
/// The channel's right volume (`0x7f`).
pub const CHAN_VOLUME: u32 = 0x7f;

/// The playback FIFO length's high half (`dev_set_format`,
/// `trident.c:126`).
pub const SB_CTRL_FORMAT: u32 = 0x18;

/// The reset value (`dev_reset`, `trident.c:103`).
pub const CODEC_CTRL_RESET: u32 = 0x000a;
/// The global control value (`dev_configure`, `trident.c:109`).
pub const GCTRL_CONFIG: u32 = 0x3000;

/// The sample-rate constant the delta registers are scaled by
/// (`trident.c:114-115`: `(rate << 12) / 48000` and
/// `(48000 << 12) / rate`).
pub const RATE_SCALE: u32 = 48000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_register_map_matches_the_header() {
        assert_eq!(VENDOR_ID, 0x1023);
        assert_eq!(DEVICE_ID, 0x2000);
        assert_eq!(REG_DMA0, 0x00);
        assert_eq!(REG_CODEC_WRITE, 0x40);
        assert_eq!(REG_CODEC_READ, 0x44);
        assert_eq!(REG_GCTRL, 0xa0);
        assert_eq!(REG_INTR_STS, 0xb0);
        assert_eq!(REG_CHAN_BASE, 0xe0);
        assert_eq!(INTR_STS_DAC, 0x0020);
        assert_eq!(INTR_STS_ADC, 0x0004);
        assert_eq!(STS_CODEC_BUSY, 0x8000);
    }

    #[test]
    fn test_the_format_bits_and_channel_constants_match_the_header() {
        assert_eq!(CMD_FORMAT_BIT16, 0x08);
        assert_eq!(CMD_FORMAT_SIGN, 0x02);
        assert_eq!(CMD_FORMAT_STEREO, 0x04);
        assert_eq!(CODEC_CTRL_RESET, 0x000a);
        assert_eq!(GCTRL_CONFIG, 0x3000);
        assert_eq!(CHAN_FMC, 3);
        assert_eq!(CHAN_VOLUME, 0x7f);
        assert_eq!(SB_CTRL_FORMAT, 0x18);
        assert_eq!(RATE_SCALE, 48000);
    }
}
