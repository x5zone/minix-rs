//! Address validation and register operation sequences.
//!
//! C correspondence: the environment parse bounds (`i2cdriver.c:47-58`:
//! bus 1..=3, address 0x0000..=0x03ff against a caller-supplied valid
//! set), and the register helpers `i2creg_read8` through
//! `i2creg_clear_bits8` (`i2cdriver.h:20-28`) which layer two-operation
//! exchanges (write the register address, then read or write the data)
//! on the raw exec request.

/// One I2C bus operation: read or write a single byte.
///
/// C: an element of the exec request's operation list; the bus driver
/// walks the list as start/stop framed byte exchanges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusOp {
    /// Write one byte to the device.
    Write(u8),
    /// Read one byte from the device (last read in a sequence ends with
    /// a NACK from the controller).
    Read,
}

/// Validate a device address against the accepted set.
///
/// C: `i2cdriver_env_parse` bounds the address at 0x0000..=0x03ff
/// (`i2cdriver.c:54-57`) and the caller passes its accepted addresses.
pub const fn address_valid(address: u16, valid_addrs: &[u16]) -> bool {
    if address > 0x03ff {
        return false;
    }
    let mut index = 0;
    while index < valid_addrs.len() {
        if valid_addrs[index] == address {
            return true;
        }
        index += 1;
    }
    false
}

/// Operations for reading one register: write the register address,
/// then read one data byte (`i2creg_read8`, `i2cdriver.h:21`).
pub const fn read8_ops(reg: u8) -> [BusOp; 2] {
    [BusOp::Write(reg), BusOp::Read]
}

/// Operations for writing one register: write the register address,
/// then the value (`i2creg_write8`, `i2cdriver.h:23`).
pub const fn write8_ops(reg: u8, value: u8) -> [BusOp; 2] {
    [BusOp::Write(reg), BusOp::Write(value)]
}

/// Operations for a two-byte big-endian register read
/// (`i2creg_read16`, `i2cdriver.h:24`).
pub const fn read16_ops(reg: u8) -> [BusOp; 3] {
    [BusOp::Write(reg), BusOp::Read, BusOp::Read]
}

/// Operations for a three-byte big-endian register read
/// (`i2creg_read24`, `i2cdriver.h:25`).
pub const fn read24_ops(reg: u8) -> [BusOp; 4] {
    [BusOp::Write(reg), BusOp::Read, BusOp::Read, BusOp::Read]
}

/// Operations to set bits inside one register: read, modify, write back
/// (`i2creg_set_bits8`, `i2cdriver.h:26-27`). The read result arrives
/// from the bus; [`apply_set_bits`] does the pure modify half.
pub const fn set_bits8_ops(reg: u8) -> [BusOp; 2] {
    [BusOp::Write(reg), BusOp::Read]
}

/// The modify half of a set-bits exchange: OR the bits in.
pub const fn apply_set_bits(current: u8, bits: u8) -> u8 {
    current | bits
}

/// Operations to clear bits inside one register
/// (`i2creg_clear_bits8`, `i2cdriver.h:28`).
pub const fn clear_bits8_ops(reg: u8) -> [BusOp; 2] {
    [BusOp::Write(reg), BusOp::Read]
}

/// The modify half of a clear-bits exchange: AND the bits out.
pub const fn apply_clear_bits(current: u8, bits: u8) -> u8 {
    current & !bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_bounds_and_valid_set() {
        assert!(address_valid(0x0050, &[0x0050, 0x0060]));
        assert!(!address_valid(0x0051, &[0x0050, 0x0060]));
        assert!(!address_valid(0x0400, &[0x0400 & 0x03ff])); // past the bound
    }

    #[test]
    fn test_read8_sequence_writes_register_then_reads() {
        let ops = read8_ops(0x1D);
        assert_eq!(ops[0], BusOp::Write(0x1D));
        assert_eq!(ops[1], BusOp::Read);
    }

    #[test]
    fn test_bit_modify_halves() {
        assert_eq!(apply_set_bits(0b0000_0100, 0b0000_0011), 0b0000_0111);
        assert_eq!(apply_clear_bits(0b0000_0111, 0b0000_0010), 0b0000_0101);
    }
}
