//! TSL2550 ambient-light sensor: ADC polling and the lux calculation.
//!
//! C correspondence: the command set and ADC masks
//! (`tsl2550.c:12-37`), `tsl2550_init` (`tsl2550.c:252-290`: power-up,
//! the power-on test value, normal range), `adc_read`
//! (`tsl2550.c:172-250`: trigger one channel, poll until the valid bit
//! appears, one final read at the deadline), and `measure_lux`
//! (`tsl2550.c:100-170`: the two lookup tables from the TAOS designer's
//! note, the ratio, and the standard-mode clamp).
//!
//! The C poll is a 400-millisecond spin (`spin_init`/`spin_check`); the
//! model bounds the poll by [`ADC_POLL_ATTEMPTS`] reads because the
//! uptime source rides the clock seam, and keeps C's final read at the
//! deadline. `measure_lux` keeps C's unsigned arithmetic, including the
//! wrap when the second channel's count exceeds the first (which the
//! clamp then pulls back to the standard-mode ceiling).

use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::Endpoint;

/// Power down (`CMD_PWR_DOWN`).
pub const CMD_POWER_DOWN: u8 = 0x00;
/// Power up (`CMD_PWR_UP`).
pub const CMD_POWER_UP: u8 = 0x03;
/// Extended range (`CMD_EXT_RANGE`).
pub const CMD_EXTENDED_RANGE: u8 = 0x1D;
/// Normal range (`CMD_NORM_RANGE`).
pub const CMD_NORMAL_RANGE: u8 = 0x18;
/// Read ADC channel zero (`CMD_READ_ADC0`).
pub const CMD_READ_ADC0: u8 = 0x43;
/// Read ADC channel one (`CMD_READ_ADC1`).
pub const CMD_READ_ADC1: u8 = 0x83;

/// Power-up test value (`EXPECTED_PWR_UP_TEST_VAL`).
pub const EXPECTED_POWER_UP_VALUE: u8 = 0x03;
/// Standard-mode ceiling in lux (`MAX_LUX_STD_MODE`).
pub const MAX_LUX_STANDARD: u32 = 1846;

/// Valid bit (`ADC_VALID_MASK`).
pub const ADC_VALID_MASK: u8 = 1 << 7;
/// Chord bits (`ADC_CHORD_MASK`).
pub const ADC_CHORD_MASK: u8 = (1 << 6) | (1 << 5) | (1 << 4);
/// Step bits (`ADC_STEP_MASK`).
pub const ADC_STEP_MASK: u8 = 0x0F;

/// Reads before the final attempt (the C spin's 400 ms stand-in).
pub const ADC_POLL_ATTEMPTS: usize = 400;

/// ADC value to count (`adc_counts_lut`).
pub const ADC_COUNTS_LUT: [u32; 128] = [
    0, 1, 2, 3, 4, 5, 6, 7,
    8, 9, 10, 11, 12, 13, 14, 15,
    16, 18, 20, 22, 24, 26, 28, 30,
    32, 34, 36, 38, 40, 42, 44, 46,
    49, 53, 57, 61, 65, 69, 73, 77,
    81, 85, 89, 93, 97, 101, 105, 109,
    115, 123, 131, 139, 147, 155, 163, 171,
    179, 187, 195, 203, 211, 219, 227, 235,
    247, 263, 279, 295, 311, 327, 343, 359,
    375, 391, 407, 423, 439, 455, 471, 487,
    511, 543, 575, 607, 639, 671, 703, 735,
    767, 799, 831, 863, 895, 927, 959, 991,
    1039, 1103, 1167, 1231, 1295, 1359, 1423, 1487,
    1551, 1615, 1679, 1743, 1807, 1871, 1935, 1999,
    2095, 2223, 2351, 2479, 2607, 2735, 2863, 2991,
    3119, 3247, 3375, 3503, 3631, 3759, 3887, 4015,
];

/// Ratio scaling factors (`ratio_lut`).
pub const RATIO_LUT: [u32; 129] = [
    100, 100, 100, 100, 100, 100, 100, 100,
    100, 100, 100, 100, 100, 100, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 98, 98, 98, 98, 98,
    98, 98, 97, 97, 97, 97, 97, 96,
    96, 96, 96, 95, 95, 95, 94, 94,
    93, 93, 93, 92, 92, 91, 91, 90,
    89, 89, 88, 87, 87, 86, 85, 84,
    83, 82, 81, 80, 79, 78, 77, 75,
    74, 73, 71, 69, 68, 66, 64, 62,
    60, 58, 56, 54, 52, 49, 47, 44,
    42, 41, 40, 40, 39, 39, 38, 38,
    37, 37, 37, 36, 36, 36, 35, 35,
    35, 35, 34, 34, 34, 34, 33, 33,
    33, 33, 32, 32, 32, 32, 32, 31,
    31, 31, 31, 31, 30, 30, 30, 30,
    30,
];

/// Compute the illuminance from the two channel values (`measure_lux`).
///
/// C's unsigned arithmetic is kept: when the second channel's count
/// exceeds the first, the difference wraps and the clamp pulls the
/// result back to [`MAX_LUX_STANDARD`].
pub fn measure_lux(adc0: u8, adc1: u8) -> u32 {
    let count0 = ADC_COUNTS_LUT[(adc0 & !ADC_VALID_MASK) as usize];
    let count1 = ADC_COUNTS_LUT[(adc1 & !ADC_VALID_MASK) as usize];
    let mut ratio = 128u32;
    if count0 != 0 && count1 <= count0 {
        ratio = count1 * 128 / count0;
    }
    if ratio > 128 {
        ratio = 128;
    }
    let mut lux = count0
        .wrapping_sub(count1)
        .wrapping_mul(RATIO_LUT[ratio as usize])
        / 256;
    if lux > MAX_LUX_STANDARD {
        lux = MAX_LUX_STANDARD;
    }
    lux
}

/// Read one ADC channel (`adc_read`).
pub fn adc_read<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    adc: u8,
) -> Result<u8, i32> {
    let command = match adc {
        0 => CMD_READ_ADC0,
        1 => CMD_READ_ADC1,
        _ => return Err(minix_types::EINVAL),
    };
    client::write_raw_byte(transport, bus, address, command)?;
    for _ in 0..ADC_POLL_ATTEMPTS {
        let value = client::read_raw_byte(transport, bus, address)?;
        if value & ADC_VALID_MASK != 0 {
            return Ok(value);
        }
    }
    let value = client::read_raw_byte(transport, bus, address)?;
    if value & ADC_VALID_MASK != 0 {
        Ok(value)
    } else {
        Err(-minix_types::EIO)
    }
}

/// Run the startup sequence (`tsl2550_init`).
pub fn init<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
) -> Result<(), i32> {
    client::write_raw_byte(transport, bus, address, CMD_POWER_UP)?;
    let value = client::read_raw_byte(transport, bus, address)?;
    if value != EXPECTED_POWER_UP_VALUE {
        return Err(-minix_types::EIO);
    }
    client::write_raw_byte(transport, bus, address, CMD_NORMAL_RANGE)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_tables_match_the_driver_note() {
        assert_eq!(ADC_COUNTS_LUT[0], 0);
        assert_eq!(ADC_COUNTS_LUT[64], 247);
        assert_eq!(ADC_COUNTS_LUT[127], 4015);
        assert_eq!(RATIO_LUT[0], 100);
        assert_eq!(RATIO_LUT[14], 99);
        assert_eq!(RATIO_LUT[128], 30);
    }

    #[test]
    fn test_measure_lux_pins_the_formula() {
        assert_eq!(measure_lux(0x80, 0x80), 0);
        assert_eq!(measure_lux(0xC0, 0xA0), 76);
        assert_eq!(measure_lux(0xFF, 0x80), 1568);
        // The second channel above the first wraps in C; the clamp stops
        // the result at the standard-mode ceiling.
        assert_eq!(measure_lux(0xFF, 0xC0), 1471);
        assert_eq!(measure_lux(0x80, 0xFF), MAX_LUX_STANDARD);
    }
}
