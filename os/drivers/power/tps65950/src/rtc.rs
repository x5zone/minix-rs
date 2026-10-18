//! The RTC half: BCD conversion and the register sequences.
//!
//! C correspondence: `rtc.c` — `rtc_init` (`rtc.c:30-56`: the control
//! write and the run-bit check), `rtc_get_time` (`rtc.c:58-126`: latch
//! the registers, read the six BCD fields with their masks, and the
//! cold-start rule), `rtc_set_time` (`rtc.c:128-172`: write the six
//! fields), and the two BCD helpers (`rtc.c:174-189`).

use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::Endpoint;

use crate::registers::{
    DAYS_REG, GET_TIME_BIT, HOURS_REG, MINUTES_REG, MONTHS_REG, RUN_BIT, RTC_CTRL_REG,
    RTC_STATUS_REG, SECONDS_REG, STOP_RTC_BIT, YEARS_REG,
};

/// The interpreted `struct tm` fields carried on the wire.
///
/// C passes the whole `struct tm` (nine fields, padded); the TPS65950
/// driver only touches six of them and the wire size is the eight
/// thirty-two-bit words the readclock family uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tm {
    /// Seconds (zero through fifty-nine).
    pub sec: i32,
    /// Minutes.
    pub min: i32,
    /// Hours.
    pub hour: i32,
    /// Day of month.
    pub mday: i32,
    /// Month, zero-based.
    pub mon: i32,
    /// Years since 1900.
    pub year: i32,
    /// Day of week.
    pub wday: i32,
    /// Day of year.
    pub yday: i32,
}

/// Bytes one `Tm` occupies on the wire (eight `i32`s).
pub const TM_WIRE_BYTES: usize = 32;

impl Tm {
    /// Serialize into the wire form (little-endian `i32` fields).
    pub fn to_wire(&self) -> [u8; TM_WIRE_BYTES] {
        let fields = [
            self.sec, self.min, self.hour, self.mday, self.mon, self.year, self.wday, self.yday,
        ];
        let mut bytes = [0u8; TM_WIRE_BYTES];
        for (index, field) in fields.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&field.to_le_bytes());
        }
        bytes
    }

    /// Deserialize from the wire form.
    pub fn from_wire(bytes: &[u8; TM_WIRE_BYTES]) -> Tm {
        let read = |index: usize| {
            let mut word = [0u8; 4];
            word.copy_from_slice(&bytes[index * 4..index * 4 + 4]);
            i32::from_le_bytes(word)
        };
        Tm {
            sec: read(0),
            min: read(1),
            hour: read(2),
            mday: read(3),
            mon: read(4),
            year: read(5),
            wday: read(6),
            yday: read(7),
        }
    }
}

/// Binary from BCD (`bcd_to_dec`, `rtc.c:176-180`).
pub const fn bcd_to_dec(value: u8) -> i32 {
    ((value >> 4) as i32) * 10 + (value & 0x0F) as i32
}

/// BCD from binary (`dec_to_bcd`, `rtc.c:182-188`).
pub const fn dec_to_bcd(value: i32) -> u8 {
    (((value / 10) as u8) << 4) | ((value % 10) as u8)
}

/// Read-modify-write one register's bits (`i2creg_set_bits8`).
fn set_bits<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    register: u8,
    bits: u8,
) -> Result<(), i32> {
    let value = client::read_byte(transport, bus, address, register)?;
    client::write_byte(transport, bus, address, register, value | bits)
}

/// Start the RTC and verify the run bit (`rtc_init`).
///
/// C sets the stop bit here despite the "start" log line; the sequence
/// is copied literally.
pub fn init<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
) -> Result<(), i32> {
    set_bits(transport, bus, address, RTC_CTRL_REG, 1 << STOP_RTC_BIT)?;
    let status = client::read_byte(transport, bus, address, RTC_STATUS_REG)?;
    if status & (1 << RUN_BIT) != (1 << RUN_BIT) {
        return Err(-minix_types::EIO);
    }
    Ok(())
}

/// Latch and read the time (`rtc_get_time`).
pub fn get_time<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
) -> Result<Tm, i32> {
    // The latch is required before each read.
    set_bits(transport, bus, address, RTC_CTRL_REG, 1 << GET_TIME_BIT)?;

    let seconds = client::read_byte(transport, bus, address, SECONDS_REG)?;
    let minutes = client::read_byte(transport, bus, address, MINUTES_REG)?;
    let hours = client::read_byte(transport, bus, address, HOURS_REG)?;
    let days = client::read_byte(transport, bus, address, DAYS_REG)?;
    let months = client::read_byte(transport, bus, address, MONTHS_REG)?;
    let years = client::read_byte(transport, bus, address, YEARS_REG)?;

    let mut tm = Tm {
        sec: bcd_to_dec(seconds & 0x7F),
        min: bcd_to_dec(minutes & 0x7F),
        hour: bcd_to_dec(hours & 0x3F),
        mday: bcd_to_dec(days & 0x3F),
        mon: bcd_to_dec(months & 0x1F) - 1,
        year: bcd_to_dec(years & 0x1F) + 100,
        wday: 0,
        yday: 0,
    };

    if tm.year == 100 {
        // Cold start: no date/time set; default to 2013-01-01 and write
        // it back (`rtc.c:112-125`).
        tm = Tm {
            sec: 0,
            min: 0,
            hour: 0,
            mday: 1,
            mon: 0,
            year: 113,
            wday: 0,
            yday: 0,
        };
        set_time(transport, bus, address, &tm)?;
    }
    Ok(tm)
}

/// Write the six date/time fields (`rtc_set_time`).
pub fn set_time<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    tm: &Tm,
) -> Result<(), i32> {
    client::write_byte(transport, bus, address, SECONDS_REG, dec_to_bcd(tm.sec) & 0x7F)?;
    client::write_byte(transport, bus, address, MINUTES_REG, dec_to_bcd(tm.min) & 0x7F)?;
    client::write_byte(transport, bus, address, HOURS_REG, dec_to_bcd(tm.hour) & 0x3F)?;
    client::write_byte(transport, bus, address, DAYS_REG, dec_to_bcd(tm.mday) & 0x3F)?;
    client::write_byte(
        transport,
        bus,
        address,
        MONTHS_REG,
        dec_to_bcd(tm.mon + 1) & 0x1F,
    )?;
    client::write_byte(
        transport,
        bus,
        address,
        YEARS_REG,
        dec_to_bcd(tm.year % 100),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcd_round_trips() {
        for value in [0i32, 9, 10, 23, 59, 99] {
            assert_eq!(bcd_to_dec(dec_to_bcd(value)), value, "{value}");
        }
        assert_eq!(dec_to_bcd(23), 0x23);
        assert_eq!(bcd_to_dec(0x59), 59);
    }

    #[test]
    fn test_tm_wire_is_eight_words() {
        let tm = Tm {
            sec: 1,
            min: 2,
            hour: 3,
            mday: 4,
            mon: 5,
            year: 106,
            wday: 7,
            yday: 8,
        };
        let bytes = tm.to_wire();
        assert_eq!(bytes.len(), TM_WIRE_BYTES);
        assert_eq!(Tm::from_wire(&bytes), tm);
        assert_eq!(&bytes[..4], &1i32.to_le_bytes());
        assert_eq!(&bytes[20..24], &106i32.to_le_bytes());
    }
}
