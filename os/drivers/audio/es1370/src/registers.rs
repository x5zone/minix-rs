//! The ES1370 register map: offsets, channel bits, and PCI identifiers.
//!
//! C correspondence: `minix3/minix/drivers/audio/es1370/es1370.h` — the
//! PCI identifiers (`:11-13`), the channel numbers (`:17-20`), the PCI
//! command bits (`:23-25`), the chip-select control and status bits
//! (`:28-45`), the memory-page and buffer registers (`:47-64`), the
//! serial-interface bits (`:66-80`), the sample-count registers
//! (`:82-87`), and the rate bounds (`:115-119`).

/// PCI vendor identifier (`VENDOR_ID`).
pub const VENDOR_ID: u16 = 0x1274;
/// PCI device identifier (`DEVICE_ID`).
pub const DEVICE_ID: u16 = 0x5000;
/// The driver's name (`DRIVER_NAME`).
pub const DRIVER_NAME: &str = "ES1370";

/// The first playback channel (`DAC1_CHAN`).
pub const DAC1_CHAN: i32 = 0;
/// The capture channel (`ADC1_CHAN`).
pub const ADC1_CHAN: i32 = 1;
/// The mixer sub-device (`MIXER`).
pub const MIXER: i32 = 2;
/// The second playback channel (`DAC2_CHAN`).
pub const DAC2_CHAN: i32 = 3;

/// PCI command bit: SERR# driver (`SERR_EN`).
pub const SERR_EN: u16 = 0x0100;
/// PCI command bit: bus mastering (`PCI_MASTER`).
pub const PCI_MASTER: u16 = 0x0004;
/// PCI command bit: I/O space access (`IO_ACCESS`).
pub const IO_ACCESS: u16 = 0x0001;

/// Interrupt and chip-select control (`CHIP_SEL_CTRL`).
pub const CHIP_SEL_CTRL: u16 = 0x00;
/// The 44.1 kHz clock select (`FREQ_44K100`).
pub const FREQ_44K100: u16 = 0x3000;
/// Codec enable (`CDC_EN`).
pub const CDC_EN: u16 = 0x0002;
/// Capture channel enable (`ADC1_EN`).
pub const ADC1_EN: u16 = 0x0010;
/// First playback channel enable (`DAC1_EN`).
pub const DAC1_EN: u16 = 0x0040;
/// Second playback channel enable (`DAC2_EN`).
pub const DAC2_EN: u16 = 0x0020;
/// Codec control bit (`XCTL0`).
pub const XCTL0: u16 = 0x0100;
/// Codec interrupt mask (`CCB_INTRM`).
pub const CCB_INTRM: u16 = 0x0400;

/// Interrupt and chip-select status (`INTERRUPT_STATUS`).
pub const INTERRUPT_STATUS: u16 = 0x04;
/// Capture interrupt bit (`ADC`).
pub const ADC: u32 = 0x0001;
/// Second playback interrupt bit (`DAC2`).
pub const DAC2: u32 = 0x0002;
/// First playback interrupt bit (`DAC1`).
pub const DAC1: u32 = 0x0004;
/// Codec busy or write-in-progress (`CSTAT`).
pub const CSTAT: u32 = 0x0400;
/// Codec write in progress (`CWRIP`).
pub const CWRIP: u32 = 0x0100;
/// The interrupt summary bit (`INTR`).
pub const INTR: u32 = 0x8000_0000;

/// The codec's write port (`CODEC_WRITE_ADDRESS`).
pub const CODEC_WRITE_ADDRESS: u16 = 0x10;
/// The legacy control block (`LEGACY`).
pub const LEGACY: u16 = 0x18;

/// The memory page selector (`MEM_PAGE`).
pub const MEM_PAGE: u16 = 0x0c;
/// The capture channel's memory page (`ADC_MEM_PAGE`).
pub const ADC_MEM_PAGE: u8 = 0x0d;
/// The playback channels' memory page (`DAC_MEM_PAGE`).
pub const DAC_MEM_PAGE: u8 = 0x0c;
/// The memory window (`MEMORY`).
pub const MEMORY: u16 = 0x30;
/// The capture channel's buffer size (`ADC_BUFFER_SIZE`).
pub const ADC_BUFFER_SIZE: u16 = 0x34;
/// The first playback channel's buffer size (`DAC1_BUFFER_SIZE`).
pub const DAC1_BUFFER_SIZE: u16 = 0x34;
/// The second playback channel's buffer size (`DAC2_BUFFER_SIZE`).
pub const DAC2_BUFFER_SIZE: u16 = 0x3c;
/// The capture channel's PCI address register (`ADC_PCI_ADDRESS`).
pub const ADC_PCI_ADDRESS: u16 = 0x30;
/// The first playback channel's PCI address register
/// (`DAC1_PCI_ADDRESS`).
pub const DAC1_PCI_ADDRESS: u16 = 0x30;
/// The second playback channel's PCI address register
/// (`DAC2_PCI_ADDRESS`).
pub const DAC2_PCI_ADDRESS: u16 = 0x38;

/// Serial interface control (`SERIAL_INTERFACE_CTRL`).
pub const SERIAL_INTERFACE_CTRL: u16 = 0x20;
/// First playback channel: stereo (`P1_S_MB`).
pub const P1_S_MB: u16 = 0x0001;
/// First playback channel: sixteen-bit samples (`P1_S_EB`).
pub const P1_S_EB: u16 = 0x0002;
/// Second playback channel: stereo (`P2_S_MB`).
pub const P2_S_MB: u16 = 0x0004;
/// Second playback channel: sixteen-bit samples (`P2_S_EB`).
pub const P2_S_EB: u16 = 0x0008;
/// Capture channel: stereo (`R1_S_MB`).
pub const R1_S_MB: u16 = 0x0010;
/// Capture channel: sixteen-bit samples (`R1_S_EB`).
pub const R1_S_EB: u16 = 0x0020;
/// First playback channel: interrupt enable (`P1_INTR_EN`).
pub const P1_INTR_EN: u16 = 0x0100;
/// Second playback channel: interrupt enable (`P2_INTR_EN`).
pub const P2_INTR_EN: u16 = 0x0200;
/// Capture channel: interrupt enable (`R1_INT_EN`).
pub const R1_INT_EN: u16 = 0x0400;
/// First playback channel: pause (`P1_PAUSE`).
pub const P1_PAUSE: u16 = 0x0800;
/// Second playback channel: pause (`P2_PAUSE`).
pub const P2_PAUSE: u16 = 0x1000;

/// First playback channel's sample count (`DAC1_SAMP_CT`).
pub const DAC1_SAMP_CT: u16 = 0x24;
/// First playback channel's current sample count
/// (`DAC1_CURR_SAMP_CT`).
pub const DAC1_CURR_SAMP_CT: u16 = 0x26;
/// Second playback channel's sample count (`DAC2_SAMP_CT`).
pub const DAC2_SAMP_CT: u16 = 0x28;
/// Second playback channel's current sample count
/// (`DAC2_CURR_SAMP_CT`).
pub const DAC2_CURR_SAMP_CT: u16 = 0x2a;
/// Capture channel's sample count (`ADC_SAMP_CT`).
pub const ADC_SAMP_CT: u16 = 0x2c;
/// Capture channel's current sample count (`ADC_CURR_SAMP_CT`).
pub const ADC_CURR_SAMP_CT: u16 = 0x2e;

/// The default sample rate (`DEFAULT_RATE`).
pub const DEFAULT_RATE: u32 = 44100;
/// The default sample width (`DEFAULT_NR_OF_BITS`).
pub const DEFAULT_NR_OF_BITS: u32 = 16;
/// The default sample sign (`DEFAULT_SIGNED`).
pub const DEFAULT_SIGNED: u32 = 0;
/// The default channel mode (`DEFAULT_STEREO`).
pub const DEFAULT_STEREO: u32 = 1;
/// The highest rate the card accepts (`MAX_RATE`).
pub const MAX_RATE: u32 = 44100;
/// The lowest rate the card accepts (`MIN_RATE`).
pub const MIN_RATE: u32 = 4000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_register_map_matches_the_header() {
        assert_eq!(VENDOR_ID, 0x1274);
        assert_eq!(DEVICE_ID, 0x5000);
        assert_eq!(CHIP_SEL_CTRL, 0x00);
        assert_eq!(INTERRUPT_STATUS, 0x04);
        assert_eq!(MEM_PAGE, 0x0c);
        assert_eq!(CODEC_WRITE_ADDRESS, 0x10);
        assert_eq!(SERIAL_INTERFACE_CTRL, 0x20);
        assert_eq!(DAC2_PCI_ADDRESS, 0x38);
        assert_eq!(ADC_SAMP_CT, 0x2c);
        assert_eq!(INTR, 0x8000_0000);
    }

    #[test]
    fn test_the_channel_bits_stay_disjoint() {
        // The three enable bits and the three interrupt bits are distinct,
        // so one channel's start cannot wake another's interrupt.
        assert_eq!(ADC1_EN | DAC1_EN | DAC2_EN, 0x0070);
        assert_eq!(ADC | DAC2 | DAC1, 0x0007);
        assert_eq!(P1_INTR_EN | P2_INTR_EN | R1_INT_EN, 0x0700);
        assert_eq!(
            P1_S_MB | P1_S_EB | P2_S_MB | P2_S_EB | R1_S_MB | R1_S_EB,
            0x003f
        );
        // The two playback channels share the memory page and the buffer
        // address register but not the frame-count register.
        assert_eq!(DAC_MEM_PAGE, 0x0c);
        assert_ne!(ADC_MEM_PAGE, DAC_MEM_PAGE);
        assert_eq!(DAC1_BUFFER_SIZE, ADC_BUFFER_SIZE);
        assert_ne!(DAC2_BUFFER_SIZE, DAC1_BUFFER_SIZE);
    }

    #[test]
    fn test_the_rate_bounds_match_the_header() {
        assert_eq!(MIN_RATE, 4000);
        assert_eq!(MAX_RATE, 44100);
        assert_eq!(DEFAULT_RATE, 44100);
        assert_eq!(DEFAULT_NR_OF_BITS, 16);
        assert_eq!(DEFAULT_STEREO, 1);
    }
}
