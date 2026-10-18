//! SHT21 humidity/temperature sensor: transactions, CRC, and conversions.
//!
//! C correspondence: the command set and masks (`sht21.c:20-96`), the
//! startup sequence (`sht21_init`, `sht21.c:137-166`: soft reset, user
//! register read, end-of-battery and power-up checks), the measurement
//! reads (`sensor_read`, `sht21.c:176-230`: a hold-mode trigger, a
//! twenty-four-bit read whose low byte is the CRC, the status-bit mask,
//! and the two linear conversions), and the CRC-8 helpers
//! (`crc8`/`checksum`, `sht21.c:300-330`).

use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::Endpoint;

/// Temperature trigger, hold mode (`CMD_TRIG_T_HOLD`).
pub const CMD_TRIGGER_TEMPERATURE: u8 = 0xE3;
/// Humidity trigger, hold mode (`CMD_TRIG_RH_HOLD`).
pub const CMD_TRIGGER_HUMIDITY: u8 = 0xE5;
/// Temperature trigger, no-hold mode (`CMD_TRIG_T_NOHOLD`; the C driver
/// uses the hold variants).
pub const CMD_TRIGGER_TEMPERATURE_NO_HOLD: u8 = 0xF3;
/// Humidity trigger, no-hold mode (`CMD_TRIG_RH_NOHOLD`).
pub const CMD_TRIGGER_HUMIDITY_NO_HOLD: u8 = 0xF5;
/// Write the user register (`CMD_WR_USR_REG`).
pub const CMD_WRITE_USER_REGISTER: u8 = 0xE6;
/// Read the user register (`CMD_RD_USR_REG`).
pub const CMD_READ_USER_REGISTER: u8 = 0xE7;
/// Soft reset (`CMD_SOFT_RESET`).
pub const CMD_SOFT_RESET: u8 = 0xFE;

/// Status bits cleared before the conversion (`STATUS_BITS_MASK`).
pub const STATUS_BITS_MASK: u16 = 0x0003;
/// Reserved user-register bits preserved across writes
/// (`USR_REG_RESERVED_MASK`).
pub const USER_REGISTER_RESERVED_MASK: u8 = (1 << 3) | (1 << 4) | (1 << 5);
/// End-of-battery flag (`USR_REG_EOB_MASK`).
pub const USER_REGISTER_END_OF_BATTERY: u8 = 1 << 6;
/// Power-up value of the non-reserved user-register bits
/// (`EXPECTED_PWR_UP_TEST_VAL`).
pub const EXPECTED_POWER_UP_VALUE: u8 = 1 << 1;
/// CRC-8 polynomial (`CRC8_POLYNOMIAL 0x131`; the low byte is what the
/// eight-bit shift register sees).
pub const CRC8_POLYNOMIAL: u8 = 0x31;
/// Initial CRC (`CRC8_INITIAL_CRC`).
pub const CRC8_INITIAL: u8 = 0x00;
/// Soft-reset settling time in microseconds (15 ms).
pub const RESET_DELAY_MICROS: u32 = 15_000;

/// Which quantity a measurement reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensor {
    /// Temperature.
    Temperature,
    /// Relative humidity.
    Humidity,
}

/// One CRC-8 step (`crc8`, `sht21.c:250-280`).
pub fn crc8(crc: u8, byte: u8) -> u8 {
    let mut crc = crc ^ byte;
    for _ in 0..8 {
        crc = if crc & 0x80 != 0 {
            (crc << 1) ^ CRC8_POLYNOMIAL
        } else {
            crc << 1
        };
    }
    crc
}

/// Validate a reading's checksum (`checksum`).
///
/// C returns `EINVAL` on a mismatch.
pub fn checksum(bytes: &[u8], expected: u8) -> Result<(), i32> {
    let mut crc = CRC8_INITIAL;
    for byte in bytes {
        crc = crc8(crc, *byte);
    }
    if crc == expected {
        Ok(())
    } else {
        Err(minix_types::EINVAL)
    }
}

/// Convert a raw temperature to millidegrees Celsius
/// (`sensor_read`'s temperature branch; C casts the float product).
pub fn temperature_millidegrees(raw: u16) -> i32 {
    ((-46.85 + (175.72 / 65536.0) * raw as f64) * 1000.0) as i32
}

/// Convert a raw humidity reading to milli-percent
/// (`sensor_read`'s humidity branch).
pub fn humidity_millipercent(raw: u16) -> i32 {
    ((-6.0 + (125.0 / 65536.0) * raw as f64) * 1000.0) as i32
}

/// Run the startup sequence (`sht21_init`).
///
/// A soft reset, the settling wait, then the user register must show no
/// end-of-battery flag and the documented power-up pattern on the
/// non-reserved bits.
pub fn init<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
) -> Result<(), i32> {
    client::write_raw_byte(transport, bus, address, CMD_SOFT_RESET)?;
    transport.delay_micros(RESET_DELAY_MICROS)?;
    let value = client::read_byte(transport, bus, address, CMD_READ_USER_REGISTER)?;
    if value & USER_REGISTER_END_OF_BATTERY != 0 {
        return Err(-minix_types::EIO);
    }
    if value & !USER_REGISTER_RESERVED_MASK != EXPECTED_POWER_UP_VALUE {
        return Err(-minix_types::EIO);
    }
    Ok(())
}

/// Read and convert one sensor (`sensor_read`).
///
/// The twenty-four-bit read carries the value in the high two bytes and
/// the CRC in the low byte; the status bits are cleared after the
/// checksum passes.
pub fn sensor_read<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    sensor: Sensor,
) -> Result<i32, i32> {
    let command = match sensor {
        Sensor::Temperature => CMD_TRIGGER_TEMPERATURE,
        Sensor::Humidity => CMD_TRIGGER_HUMIDITY,
    };
    let value = client::read_u24(transport, bus, address, command)?;
    let expected_crc = (value & 0xFF) as u8;
    let raw = ((value >> 8) & 0xFFFF) as u16;
    checksum(&[(raw >> 8) as u8, raw as u8], expected_crc)?;
    let raw = raw & !STATUS_BITS_MASK;
    Ok(match sensor {
        Sensor::Temperature => temperature_millidegrees(raw),
        Sensor::Humidity => humidity_millipercent(raw),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc8_matches_the_reference() {
        // CRC-8 over the sample reading bytes 0x68 0x32.
        assert_eq!(crc8(crc8(CRC8_INITIAL, 0x68), 0x32), 0xC5);
        assert_eq!(checksum(&[0x68, 0x32], 0xC5), Ok(()));
        assert_eq!(checksum(&[0x68, 0x32], 0xC4), Err(minix_types::EINVAL));
    }

    #[test]
    fn test_conversions_match_the_datasheet_formula() {
        assert_eq!(temperature_millidegrees(0), -4_6850);
        assert_eq!(temperature_millidegrees(26674), 24_670);
        assert_eq!(humidity_millipercent(0), -6_000);
        assert_eq!(humidity_millipercent(32768), 56_500);
    }

    #[test]
    fn test_masks_match_the_c_header() {
        assert_eq!(STATUS_BITS_MASK, 0x0003);
        assert_eq!(USER_REGISTER_RESERVED_MASK, 0x38);
        assert_eq!(USER_REGISTER_END_OF_BATTERY, 0x40);
        assert_eq!(EXPECTED_POWER_UP_VALUE, 0x02);
        assert_eq!(CRC8_POLYNOMIAL, 0x31);
    }
}
