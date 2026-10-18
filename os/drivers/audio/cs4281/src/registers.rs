//! The CS4281 register map: offsets, command bits, and PCI identifiers.
//!
//! C correspondence: `minix3/minix/drivers/audio/cs4281/cs4281.h` — the
//! sub-device numbers and PCI identifiers (`:17-24`), the interrupt
//! status bits (`:32-33`), the register offsets (`:36-88`), the codec and
//! command bits (`:90-125`), and the six-entry sample-rate table
//! (`:128-130`).
//!
//! This card's register window is **memory-mapped**: its `io.h` compiles
//! the `DMA_BASE_IOMAP` branch (`cs4281.h:5`), so every access is a
//! volatile load or store to an address the driver mapped out of the PCI
//! BAR (`cs4281.c:305-318`), not a port I/O call.

/// PCI vendor identifier (`VENDOR_ID`).
pub const VENDOR_ID: u16 = 0x1013;
/// PCI device identifier (`DEVICE_ID`).
pub const DEVICE_ID: u16 = 0x6005;
/// The driver's name (`DRIVER_NAME`).
pub const DRIVER_NAME: &str = "CS4281";

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
pub const INTR_STS_DAC: u32 = 0x0100;
/// The capture interrupt bit (`INTR_STS_ADC`).
pub const INTR_STS_ADC: u32 = 0x0200;

/// Interrupt status (`REG_INTR_STS`).
pub const REG_INTR_STS: u32 = 0x0000;
/// Interrupt control (`REG_INTR_CTRL`).
pub const REG_INTR_CTRL: u32 = 0x0008;
/// Interrupt mask (`REG_INTR_MASK`).
pub const REG_INTR_MASK: u32 = 0x000c;
/// Configuration write (`REG_CONF_WRITE`).
pub const REG_CONF_WRITE: u32 = 0x03e0;
/// Extended power (`REG_POWER_EXT`).
pub const REG_POWER_EXT: u32 = 0x03e4;
/// Sound power control (`REG_SPOWER_CTRL`).
pub const REG_SPOWER_CTRL: u32 = 0x03ec;
/// Configuration load (`REG_CONF_LOAD`).
pub const REG_CONF_LOAD: u32 = 0x03f0;
/// Clock control (`REG_CLK_CTRL`).
pub const REG_CLK_CTRL: u32 = 0x0400;
/// Master control (`REG_MASTER_CTRL`).
pub const REG_MASTER_CTRL: u32 = 0x0420;
/// Codec control (`REG_CODEC_CTRL`).
pub const REG_CODEC_CTRL: u32 = 0x0460;
/// Codec status (`REG_CODEC_STATUS`).
pub const REG_CODEC_STATUS: u32 = 0x0464;
/// Codec output slot valid (`REG_CODEC_OSV`).
pub const REG_CODEC_OSV: u32 = 0x0468;
/// Codec register address (`REG_CODEC_ADDR`).
pub const REG_CODEC_ADDR: u32 = 0x046c;
/// Codec register data (`REG_CODEC_DATA`).
pub const REG_CODEC_DATA: u32 = 0x0470;
/// Codec serial data (`REG_CODEC_SDA`).
pub const REG_CODEC_SDA: u32 = 0x047c;
/// Sound power (`REG_SOUND_POWER`).
pub const REG_SOUND_POWER: u32 = 0x0740;
/// Playback sample rate (`REG_DAC_SAMPLE_RATE`).
pub const REG_DAC_SAMPLE_RATE: u32 = 0x0744;
/// Capture sample rate (`REG_ADC_SAMPLE_RATE`).
pub const REG_ADC_SAMPLE_RATE: u32 = 0x0748;
/// Sample-rate converter slot (`REG_SRC_SLOT`).
pub const REG_SRC_SLOT: u32 = 0x075c;
/// PCM left volume (`REG_PCM_LVOL`).
pub const REG_PCM_LVOL: u32 = 0x0760;
/// PCM right volume (`REG_PCM_RVOL`).
pub const REG_PCM_RVOL: u32 = 0x0764;

/// Playback head status (`REG_DAC_HDSR`).
pub const REG_DAC_HDSR: u32 = 0x00f0;
/// Playback current address (`REG_DAC_DCC`).
pub const REG_DAC_DCC: u32 = 0x0114;
/// Playback DMA mode (`REG_DAC_DMR`).
pub const REG_DAC_DMR: u32 = 0x0150;
/// Playback DMA control (`REG_DAC_DCR`).
pub const REG_DAC_DCR: u32 = 0x0154;
/// Playback FIFO control (`REG_DAC_FCR`).
pub const REG_DAC_FCR: u32 = 0x0180;
/// Playback FIFO slot count (`REG_DAC_FSIC`).
pub const REG_DAC_FSIC: u32 = 0x0214;
/// Capture head status (`REG_ADC_HDSR`).
pub const REG_ADC_HDSR: u32 = 0x00f4;
/// Capture current address (`REG_ADC_DCC`).
pub const REG_ADC_DCC: u32 = 0x0124;
/// Capture DMA mode (`REG_ADC_DMR`).
pub const REG_ADC_DMR: u32 = 0x0158;
/// Capture DMA control (`REG_ADC_DCR`).
pub const REG_ADC_DCR: u32 = 0x015c;
/// Capture FIFO control (`REG_ADC_FCR`).
pub const REG_ADC_FCR: u32 = 0x0184;
/// Capture FIFO slot count (`REG_ADC_FSIC`).
pub const REG_ADC_FSIC: u32 = 0x0214;
/// Playback DMA address (`REG_DAC_DMA_ADDR`).
pub const REG_DAC_DMA_ADDR: u32 = 0x0118;
/// Playback DMA length (`REG_DAC_DMA_LEN`).
pub const REG_DAC_DMA_LEN: u32 = 0x011c;
/// Capture DMA address (`REG_ADC_DMA_ADDR`).
pub const REG_ADC_DMA_ADDR: u32 = 0x0128;
/// Capture DMA length (`REG_ADC_DMA_LEN`).
pub const REG_ADC_DMA_LEN: u32 = 0x012c;

/// The codec's power register (`CODEC_REG_POWER`).
pub const CODEC_REG_POWER: u32 = 0x26;
/// The codec answered (`STS_CODEC_DONE`).
pub const STS_CODEC_DONE: u32 = 0x0008;
/// The codec's data is valid (`STS_CODEC_VALID`).
pub const STS_CODEC_VALID: u32 = 0x0002;

/// Power down (`CMD_POWER_DOWN`).
pub const CMD_POWER_DOWN: u32 = 1 << 14;
/// Port timing (`CMD_PORT_TIMING`).
pub const CMD_PORT_TIMING: u32 = 1 << 16;
/// AC97 mode (`CMD_AC97_MODE`).
pub const CMD_AC97_MODE: u32 = 1 << 1;
/// Master serial (`CMD_MASTER_SERIAL`).
pub const CMD_MASTER_SERIAL: u32 = 1 << 0;
/// Interrupt enable (`CMD_INTR_ENABLE`).
pub const CMD_INTR_ENABLE: u32 = 0x03;
/// DMA interrupt mask (`CMD_INTR_DMA`).
pub const CMD_INTR_DMA: u32 = 0x0004_0000;
/// Playback DMA interrupt (`CMD_INTR_DMA0`).
pub const CMD_INTR_DMA0: u32 = 0x0100;
/// Capture DMA interrupt (`CMD_INTR_DMA1`).
pub const CMD_INTR_DMA1: u32 = 0x0200;
/// The DMA mode register's initial value (`CMD_DMR_INIT`).
pub const CMD_DMR_INIT: u32 = 0x50;
/// DMA mode: write (`CMD_DMR_WRITE`).
pub const CMD_DMR_WRITE: u32 = 0x08;
/// DMA mode: read (`CMD_DMR_READ`).
pub const CMD_DMR_READ: u32 = 0x04;
/// DMA mode: eight-bit samples (`CMD_DMR_BIT8`).
pub const CMD_DMR_BIT8: u32 = 1 << 16;
/// DMA mode: mono (`CMD_DMR_MONO`).
pub const CMD_DMR_MONO: u32 = 1 << 17;
/// DMA mode: unsigned samples (`CMD_DMR_UNSIGN`).
pub const CMD_DMR_UNSIGN: u32 = 1 << 19;
/// DMA mode: thirty-two-bit samples (`CMD_DMR_BIT32`).
pub const CMD_DMR_BIT32: u32 = 1 << 20;
/// DMA mode: byte swap (`CMD_DMR_SWAP`).
pub const CMD_DMR_SWAP: u32 = 1 << 22;
/// DMA mode: poll (`CMD_DMR_POLL`).
pub const CMD_DMR_POLL: u32 = 1 << 28;
/// DMA mode: run (`CMD_DMR_DMA`).
pub const CMD_DMR_DMA: u32 = 1 << 29;
/// DMA control's mask bit (`CMD_DCR_MASK`).
pub const CMD_DCR_MASK: u32 = 1 << 0;
/// FIFO control: enable (`CMD_FCR_FEN`).
pub const CMD_FCR_FEN: u32 = 1 << 31;
/// The playback FIFO's initial value (`CMD_DAC_FCR_INIT`).
pub const CMD_DAC_FCR_INIT: u32 = 0x0100_2000;
/// The capture FIFO's initial value (`CMD_ADC_FCR_INIT`).
pub const CMD_ADC_FCR_INIT: u32 = 0x0b0a_2020;
/// The DMA control register's initial value (`dev_start_channel`,
/// `cs4281.c:161`: `dcr_data = 0x30001`).
pub const CMD_DCR_INIT: u32 = 0x30001;

/// How many codec polls each wait allows (`cs4281.c:59`, `:73`, `:81`).
pub const CODEC_POLL_LIMIT: u32 = 50000;
/// The settling wait between codec polls (`micro_delay(10)`).
pub const CODEC_POLL_DELAY_MICROS: u32 = 10;
/// The codec power-down poll's settling wait (`micro_delay(100)`,
/// `mixer.c:236`).
pub const AC97_READY_DELAY_MICROS: u32 = 100;

/// The sample rates the card accepts (`g_sample_rate`, `cs4281.h:128-130`).
pub const SAMPLE_RATES: [u32; 6] = [48000, 44100, 22050, 16000, 11025, 8000];

/// The rate code for a rate (`dev_set_sample_rate`, `cs4281.c:137-147`):
/// C leaves the index at zero when nothing matches, so an unsupported
/// rate programs the first entry (48000 Hz).
pub fn rate_code(rate: u32) -> u32 {
    SAMPLE_RATES
        .iter()
        .position(|entry| *entry == rate)
        .map(|index| index as u32)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_register_map_matches_the_header() {
        assert_eq!(VENDOR_ID, 0x1013);
        assert_eq!(DEVICE_ID, 0x6005);
        assert_eq!(REG_INTR_STS, 0x0000);
        assert_eq!(REG_INTR_CTRL, 0x0008);
        assert_eq!(REG_CODEC_CTRL, 0x0460);
        assert_eq!(REG_CODEC_SDA, 0x047c);
        assert_eq!(REG_DAC_DMR, 0x0150);
        assert_eq!(REG_ADC_DCR, 0x015c);
        assert_eq!(REG_DAC_DMA_LEN, 0x011c);
        assert_eq!(INTR_STS_DAC, 0x0100);
        assert_eq!(INTR_STS_ADC, 0x0200);
    }

    #[test]
    fn test_the_command_bits_match_the_header() {
        assert_eq!(CMD_POWER_DOWN, 1 << 14);
        assert_eq!(CMD_INTR_ENABLE, 0x03);
        assert_eq!(CMD_DMR_INIT, 0x50);
        assert_eq!(CMD_DMR_DMA, 1 << 29);
        assert_eq!(CMD_DMR_POLL, 1 << 28);
        assert_eq!(CMD_DCR_MASK, 1);
        assert_eq!(CMD_DCR_INIT, 0x30001);
        assert_eq!(CMD_FCR_FEN, 1 << 31);
        assert_eq!(CMD_DAC_FCR_INIT, 0x0100_2000);
        assert_eq!(CMD_ADC_FCR_INIT, 0x0b0a_2020);
        // The codec's status bits are distinct.
        assert_eq!(STS_CODEC_DONE & STS_CODEC_VALID, 0);
    }

    #[test]
    fn test_the_rate_table_and_lookup() {
        assert_eq!(SAMPLE_RATES, [48000, 44100, 22050, 16000, 11025, 8000]);
        assert_eq!(rate_code(48000), 0);
        assert_eq!(rate_code(44100), 1);
        assert_eq!(rate_code(8000), 5);
        // C leaves the index at zero for an unknown rate.
        assert_eq!(rate_code(12345), 0);
    }
}
