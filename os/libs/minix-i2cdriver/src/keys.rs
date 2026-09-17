//! Data-store keys and subscription patterns for I2C drivers.
//!
//! C correspondence: `i2cdriver_announce` publishes
//! `drv.i2c.<bus>.<label>` (example key `drv.i2c.1.cat24c256.0x50`,
//! `i2cdriver.c:24-36`); `i2cdriver_bus_endpoint` looks up `i2c.<bus>`
//! (`i2cdriver.c:82-99`); `i2cdriver_subscribe_bus_updates` subscribes to
//! the regex `drv\.chr\.i2c\.<bus>` (`i2cdriver.c:101-116`).

use alloc::string::String;

/// Key prefix announcing an I2C device driver (`i2cdriver.c:17`).
pub const DRIVER_PREFIX: &str = "drv.i2c.";

/// Label prefix of the I2C bus driver itself (`i2cdriver.c:84`).
pub const BUS_LABEL_PREFIX: &str = "i2c.";

/// The data-store key a driver publishes on startup:
/// `drv.i2c.<bus>.<label>` (`i2cdriver.c:33-35`).
pub fn announce_key(bus: u32, label: &str) -> String {
    let mut key = String::from(DRIVER_PREFIX);
    key.push_str(itoa(bus).as_str());
    key.push('.');
    key.push_str(label);
    key
}

/// The data-store label of the I2C bus driver itself: `i2c.<bus>`
/// (`i2cdriver.c:88-90`).
pub fn bus_label(bus: u32) -> String {
    let mut label = String::from(BUS_LABEL_PREFIX);
    label.push_str(itoa(bus).as_str());
    label
}

/// The subscription regex capturing one bus's driver events:
/// `drv\.chr\.i2c\.<bus>` (`i2cdriver.c:105-108`, dots escaped).
pub fn subscribe_regex(bus: u32) -> String {
    let mut regex = String::from("drv\\.chr\\.i2c\\.");
    regex.push_str(itoa(bus).as_str());
    regex
}

/// Decimal formatting without pulling in formatting machinery.
fn itoa(mut value: u32) -> String {
    if value == 0 {
        return String::from("0");
    }
    let mut digits = alloc::vec::Vec::new();
    while value > 0 {
        digits.push(b'0' + (value % 10) as u8);
        value /= 10;
    }
    digits.reverse();
    String::from_utf8(digits).expect("decimal digits are ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_announce_key_composes_prefix_bus_label() {
        // C: "example key: drv.i2c.1.cat24c245.0x50" (i2cdriver.c:34).
        assert_eq!(announce_key(1, "cat24c256.0x50"), "drv.i2c.1.cat24c256.0x50");
        assert_eq!(announce_key(12, "abc"), "drv.i2c.12.abc");
    }

    #[test]
    fn test_bus_label_and_subscribe_regex() {
        assert_eq!(bus_label(2), "i2c.2");
        assert_eq!(subscribe_regex(3), "drv\\.chr\\.i2c\\.3");
    }

    #[test]
    fn test_itoa_zero() {
        assert_eq!(itoa(0), "0");
    }
}
