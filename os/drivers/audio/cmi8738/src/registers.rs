//! The CMI8738 register map: offsets, format bits, and command masks.
//!
//! C correspondence: `cmi8738.h:38-88` — the register offsets
//! (`REG_FUNC_CTRL` through `REG_INTR_STS`), the format bits
//! (`FMT_BIT16`/`FMT_STEREO`), the `CMD_*` masks, and the eight-entry
//! sample-rate table (`g_sample_rate`, `cmi8738.c:90-92`).

/// PCI vendor identifier (`VENDOR_ID`).
pub const VENDOR_ID: u16 = 0x13f6;
/// PCI device identifier (`DEVICE_ID`).
pub const DEVICE_ID: u16 = 0x0111;
/// The driver's name (`DRIVER_NAME`).
pub const DRIVER_NAME: &str = "CMI8738";

/// Function control (`REG_FUNC_CTRL`).
pub const REG_FUNC_CTRL: u16 = 0x00;
/// Function control 1 (`REG_FUNC_CTRL1`).
pub const REG_FUNC_CTRL1: u16 = 0x04;
/// Format (`REG_FORMAT`).
pub const REG_FORMAT: u16 = 0x08;
/// Miscellaneous control (`REG_MISC_CTRL`).
pub const REG_MISC_CTRL: u16 = 0x18;
/// Sound Blaster data port (`REG_SB_DATA`).
pub const REG_SB_DATA: u16 = 0x22;
/// Sound Blaster address port (`REG_SB_ADDR`).
pub const REG_SB_ADDR: u16 = 0x23;
/// Mixer input (`REG_MIX_INPUT`).
pub const REG_MIX_INPUT: u16 = 0x25;
/// Extended miscellaneous (`REG_EXT_MISC`).
pub const REG_EXT_MISC: u16 = 0x90;
/// DAC sample count (`REG_DAC_SAMPLE_COUNT`).
pub const REG_DAC_SAMPLE_COUNT: u16 = 0x86;
/// ADC sample count (`REG_ADC_SAMPLE_COUNT`).
pub const REG_ADC_SAMPLE_COUNT: u16 = 0x8e;
/// DAC DMA address (`REG_DAC_DMA_ADDR`).
pub const REG_DAC_DMA_ADDR: u16 = 0x80;
/// DAC DMA length (`REG_DAC_DMA_LEN`).
pub const REG_DAC_DMA_LEN: u16 = 0x84;
/// ADC DMA address (`REG_ADC_DMA_ADDR`).
pub const REG_ADC_DMA_ADDR: u16 = 0x88;
/// ADC DMA length (`REG_ADC_DMA_LEN`).
pub const REG_ADC_DMA_LEN: u16 = 0x8c;
/// Extended index (`REG_EXT_INDEX`).
pub const REG_EXT_INDEX: u16 = 0xf0;
/// Interrupt control (`REG_INTR_CTRL`).
pub const REG_INTR_CTRL: u16 = 0x0c;
/// Interrupt status (`REG_INTR_STS`).
pub const REG_INTR_STS: u16 = 0x10;

/// 16-bit format bit (`FMT_BIT16`).
pub const FMT_BIT16: u32 = 0x02;
/// Stereo format bit (`FMT_STEREO`).
pub const FMT_STEREO: u32 = 0x01;

/// Power down the device (`CMD_POWER_DOWN`).
pub const CMD_POWER_DOWN: u32 = 0x8000_0000;
/// Reset the device (`CMD_RESET`).
pub const CMD_RESET: u32 = 0x4000_0000;
/// ADC capture source bit 0 (`CMD_ADC_C0`).
pub const CMD_ADC_C0: u32 = 0x0000_0001;
/// ADC capture source bit 1 (`CMD_ADC_C1`).
pub const CMD_ADC_C1: u32 = 0x0000_0002;
/// Four-speaker mode (`CMD_N4SPK3D`).
pub const CMD_N4SPK3D: u32 = 0x0400_0000;
/// S/PDIF enable (`CMD_SPDIF_ENA`).
pub const CMD_SPDIF_ENA: u32 = 0x0000_0200;
/// S/PDIF loopback (`CMD_SPDIF_LOOP`).
pub const CMD_SPDIF_LOOP: u32 = 0x0000_0080;
/// Pause channel 0 (DAC) (`CMD_PAUSE_C0`).
pub const CMD_PAUSE_C0: u32 = 0x0000_0004;
/// Pause channel 1 (ADC) (`CMD_PAUSE_C1`).
pub const CMD_PAUSE_C1: u32 = 0x0000_0008;
/// Enable channel 0 (DAC) (`CMD_ENA_C0`).
pub const CMD_ENA_C0: u32 = 0x0001_0000;
/// Enable channel 1 (ADC) (`CMD_ENA_C1`).
pub const CMD_ENA_C1: u32 = 0x0002_0000;
/// Reset channel 0 (`CMD_RESET_C0`).
pub const CMD_RESET_C0: u32 = 0x0004_0000;
/// Reset channel 1 (`CMD_RESET_C1`).
pub const CMD_RESET_C1: u32 = 0x0008_0000;
/// Both interrupt enables (`CMD_INTR_ENABLE`).
pub const CMD_INTR_ENABLE: u32 = 0x0003_0000;
/// Interrupt status: DAC (`INTR_STS_DAC`).
pub const INTR_STS_DAC: u32 = 0x0000_0001;
/// Interrupt status: ADC (`INTR_STS_ADC`).
pub const INTR_STS_ADC: u32 = 0x0000_0002;

/// The DAC sub-device (`DAC`).
pub const DAC: i32 = 0;
/// The ADC sub-device (`ADC`).
pub const ADC: i32 = 1;
/// The mixer sub-device (`MIX`).
pub const MIX: i32 = 2;

/// The sample rates the card accepts, indexed by the register's rate code
/// (`g_sample_rate`).
pub const SAMPLE_RATES: [u32; 8] = [5512, 11025, 22050, 44100, 8000, 16000, 32000, 48000];

/// The rate code for a rate, if the card accepts it
/// (`dev_set_sample_rate`, `cmi8738.c:99-113`).
pub fn rate_code(rate: u32) -> Option<u32> {
    SAMPLE_RATES
        .iter()
        .position(|entry| *entry == rate)
        .map(|index| index as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_register_map_matches_the_header() {
        assert_eq!(VENDOR_ID, 0x13f6);
        assert_eq!(DEVICE_ID, 0x0111);
        assert_eq!(REG_FUNC_CTRL, 0x00);
        assert_eq!(REG_FORMAT, 0x08);
        assert_eq!(REG_MISC_CTRL, 0x18);
        assert_eq!(REG_DAC_DMA_ADDR, 0x80);
        assert_eq!(REG_ADC_DMA_LEN, 0x8c);
        assert_eq!(REG_INTR_STS, 0x10);
        assert_eq!(FMT_BIT16, 0x02);
        assert_eq!(FMT_STEREO, 0x01);
        assert_eq!(CMD_INTR_ENABLE, CMD_ENA_C0 | CMD_ENA_C1);
        assert_eq!(CMD_PAUSE_C0, 0x04);
        assert_eq!(CMD_PAUSE_C1, 0x08);
    }

    #[test]
    fn test_the_rate_table_and_lookup() {
        assert_eq!(SAMPLE_RATES.len(), 8);
        assert_eq!(rate_code(44100), Some(3));
        assert_eq!(rate_code(48000), Some(7));
        assert_eq!(rate_code(999), None);
    }
}
