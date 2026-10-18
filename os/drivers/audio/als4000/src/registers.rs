//! The ALS4000 register map: offsets, command bytes, and PCI identifiers.
//!
//! C correspondence: `minix3/minix/drivers/audio/als4000/als4000.h` — the
//! sub-device numbers and PCI identifiers (`:17-24`), the interrupt
//! status bits (`:32-33`), the register offsets (`:36-61`), the command
//! bytes (`:63-79`), and the eight-entry sample-rate table (`:82-84`).

/// PCI vendor identifier (`VENDOR_ID`).
pub const VENDOR_ID: u16 = 0x4005;
/// PCI device identifier (`DEVICE_ID`).
pub const DEVICE_ID: u16 = 0x4000;
/// The driver's name (`DRIVER_NAME`).
pub const DRIVER_NAME: &str = "ALS4000";

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
pub const INTR_STS_DAC: u32 = 0x80;
/// The capture interrupt bit (`INTR_STS_ADC`).
pub const INTR_STS_ADC: u32 = 0x40;

/// Mixer register selector (`REG_MIXER_ADDR`).
pub const REG_MIXER_ADDR: u16 = 0x04;
/// Mixer register data (`REG_MIXER_DATA`).
pub const REG_MIXER_DATA: u16 = 0x05;
/// Global control register data (`REG_GCR_DATA`).
pub const REG_GCR_DATA: u16 = 0x08;
/// Global control register index (`REG_GCR_INDEX`).
pub const REG_GCR_INDEX: u16 = 0x0c;
/// Interrupt status (`REG_INTR_STS`).
pub const REG_INTR_STS: u16 = 0x0e;
/// Interrupt control (`REG_INTR_CTRL`).
pub const REG_INTR_CTRL: u16 = 0x8c;
/// DMA extended-mode control (`REG_DMA_EM_CTRL`).
pub const REG_DMA_EM_CTRL: u16 = 0x99;
/// Playback DMA address (`REG_DAC_DMA_ADDR`).
pub const REG_DAC_DMA_ADDR: u16 = 0x91;
/// Playback DMA length (`REG_DAC_DMA_LEN`).
pub const REG_DAC_DMA_LEN: u16 = 0x92;
/// Playback current address (`REG_DAC_CUR_ADDR`).
pub const REG_DAC_CUR_ADDR: u16 = 0xa0;
/// Capture DMA address (`REG_ADC_DMA_ADDR`).
pub const REG_ADC_DMA_ADDR: u16 = 0xa2;
/// Capture DMA length (`REG_ADC_DMA_LEN`).
pub const REG_ADC_DMA_LEN: u16 = 0xa3;
/// Capture current address (`REG_ADC_CUR_ADDR`).
pub const REG_ADC_CUR_ADDR: u16 = 0xa4;

/// Sound Blaster configuration (`REG_SB_CONFIG`).
pub const REG_SB_CONFIG: u16 = 0x00;
/// Sound Blaster reset (`REG_SB_RESET`).
pub const REG_SB_RESET: u16 = 0x06;
/// Sound Blaster read port (`REG_SB_READ`).
pub const REG_SB_READ: u16 = 0x0a;
/// Sound Blaster command port (`REG_SB_CMD`).
pub const REG_SB_CMD: u16 = 0x0c;
/// Sound Blaster data port (`REG_SB_DATA`).
pub const REG_SB_DATA: u16 = 0x0e;
/// The Sound Blaster register window's base (`REG_SB_BASE`).
pub const REG_SB_BASE: u16 = 0x10;
/// FIFO length, low byte (`REG_SB_FIFO_LEN_LO`).
pub const REG_SB_FIFO_LEN_LO: u16 = 0x1c;
/// FIFO length, high byte (`REG_SB_FIFO_LEN_HI`).
pub const REG_SB_FIFO_LEN_HI: u16 = 0x1d;
/// FIFO control (`REG_SB_FIFO_CTRL`).
pub const REG_SB_FIFO_CTRL: u16 = 0x1e;
/// DMA setup (`REG_SB_DMA_SETUP`).
pub const REG_SB_DMA_SETUP: u16 = 0x81;
/// Sound Blaster interrupt status (`REG_SB_IRQ_STATUS`).
pub const REG_SB_IRQ_STATUS: u16 = 0x82;
/// Sound Blaster control (`REG_SB_CTRL`).
pub const REG_SB_CTRL: u16 = 0xc0;

/// Mixer write enable (`CMD_MIXER_WRITE_ENABLE`).
pub const CMD_MIXER_WRITE_ENABLE: u32 = 0x80;
/// Sound on (`CMD_SOUND_ON`).
pub const CMD_SOUND_ON: u32 = 0xd1;
/// Interrupt enable in the interrupt control register
/// (`CMD_INTR_ENABLE`).
pub const CMD_INTR_ENABLE: u32 = 0x28000;
/// Sample-rate command (`CMD_SAMPLE_RATE_OUT`).
pub const CMD_SAMPLE_RATE_OUT: u32 = 0x41;
/// Signed mono format (`CMD_SIGN_MONO`).
pub const CMD_SIGN_MONO: u32 = 0x10;
/// Signed stereo format (`CMD_SIGN_STEREO`).
pub const CMD_SIGN_STEREO: u32 = 0x30;
/// Unsigned mono format (`CMD_UNSIGN_MONO`).
pub const CMD_UNSIGN_MONO: u32 = 0x00;
/// Unsigned stereo format (`CMD_UNSIGN_STEREO`).
pub const CMD_UNSIGN_STEREO: u32 = 0x20;
/// Sixteen-bit auto-initialize format (`CMD_BIT16_AI`).
pub const CMD_BIT16_AI: u32 = 0xb6;
/// Sixteen-bit DMA off (`CMD_BIT16_DMA_OFF`).
pub const CMD_BIT16_DMA_OFF: u32 = 0xd5;
/// Sixteen-bit DMA on (`CMD_BIT16_DMA_ON`).
pub const CMD_BIT16_DMA_ON: u32 = 0xd6;
/// Eight-bit auto-initialize format (`CMD_BIT8_AI`).
pub const CMD_BIT8_AI: u32 = 0xc6;
/// Eight-bit DMA off (`CMD_BIT8_DMA_OFF`).
pub const CMD_BIT8_DMA_OFF: u32 = 0xd0;
/// Eight-bit DMA on (`CMD_BIT8_DMA_ON`).
pub const CMD_BIT8_DMA_ON: u32 = 0xd4;
/// Capture: eight-bit width (`CMD_REC_WIDTH8`).
pub const CMD_REC_WIDTH8: u32 = 0x04;
/// Capture: stereo (`CMD_REC_STEREO`).
pub const CMD_REC_STEREO: u32 = 0x20;
/// Capture: signed (`CMD_REC_SIGN`).
pub const CMD_REC_SIGN: u32 = 0x10;

/// The playback DMA length's flag bits (`dev_set_dma`,
/// `als4000.c:205`: `(len - 1) | 0x180000`).
pub const DAC_DMA_LEN_FLAGS: u32 = 0x0018_0000;

/// The sample rates the card accepts (`g_sample_rate`, `als4000.h:82-84`).
pub const SAMPLE_RATES: [u32; 8] = [5512, 11025, 22050, 44100, 8000, 16000, 32000, 48000];

/// The number of tries the Sound Blaster command port waits for
/// (`dev_command`, `als4000.c:61-70`).
pub const COMMAND_TRIES: u32 = 1000;
/// The number of tries the reset echo poll allows (`dev_reset`,
/// `als4000.c:101-110`).
pub const RESET_TRIES: u32 = 1000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_register_map_matches_the_header() {
        assert_eq!(VENDOR_ID, 0x4005);
        assert_eq!(DEVICE_ID, 0x4000);
        assert_eq!(REG_MIXER_ADDR, 0x04);
        assert_eq!(REG_SB_BASE, 0x10);
        assert_eq!(REG_INTR_CTRL, 0x8c);
        assert_eq!(REG_DMA_EM_CTRL, 0x99);
        assert_eq!(REG_DAC_DMA_ADDR, 0x91);
        assert_eq!(REG_ADC_CUR_ADDR, 0xa4);
        assert_eq!(INTR_STS_DAC, 0x80);
        assert_eq!(INTR_STS_ADC, 0x40);
    }

    #[test]
    fn test_the_command_bytes_match_the_header() {
        assert_eq!(CMD_SOUND_ON, 0xd1);
        assert_eq!(CMD_SAMPLE_RATE_OUT, 0x41);
        assert_eq!(CMD_BIT16_AI, 0xb6);
        assert_eq!(CMD_BIT8_AI, 0xc6);
        assert_eq!(CMD_BIT16_DMA_ON, 0xd6);
        assert_eq!(CMD_BIT8_DMA_ON, 0xd4);
        assert_eq!(CMD_BIT16_DMA_OFF, 0xd5);
        assert_eq!(CMD_BIT8_DMA_OFF, 0xd0);
        assert_eq!(CMD_INTR_ENABLE, 0x28000);
        // The capture format bits are disjoint, so one field's bits
        // cannot be mistaken for another's.
        assert_eq!(CMD_REC_WIDTH8 | CMD_REC_STEREO | CMD_REC_SIGN, 0x34);
        // The playback DMA length's flags sit above a sixteen-bit count.
        assert_eq!(DAC_DMA_LEN_FLAGS, 0x0018_0000);
    }

    #[test]
    fn test_the_rate_table_matches_the_header() {
        assert_eq!(
            SAMPLE_RATES,
            [5512, 11025, 22050, 44100, 8000, 16000, 32000, 48000]
        );
        assert_eq!(COMMAND_TRIES, 1000);
        assert_eq!(RESET_TRIES, 1000);
    }
}
