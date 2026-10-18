//! Chip identification (`check_revision`).
//!
//! C correspondence: `check_revision` (`tps65950.c:78-140`) — the unlock
//! write to `UNLOCK_TEST_REG`, the four IDCODE bytes, and the accepted
//! revision values. C logs a failed IDCODE read and keeps going (the
//! bytes then hold uninitialized values); the Rust sequence propagates
//! the failing read instead.

use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::Endpoint;

use crate::registers::{
    IDCODE_7_0_REG, IDCODE_15_8_REG, IDCODE_23_16_REG, IDCODE_31_24_REG, IDCODE_REV_1_0,
    IDCODE_REV_1_1, IDCODE_REV_1_2, UNLOCK_TEST_CODE, UNLOCK_TEST_REG,
};

/// Zero-based revision index for an IDCODE value, or `None` for an
/// unknown chip.
pub const fn revision_index(idcode: u32) -> Option<u8> {
    match idcode {
        IDCODE_REV_1_0 => Some(0),
        IDCODE_REV_1_1 => Some(1),
        IDCODE_REV_1_2 => Some(2),
        _ => None,
    }
}

/// Unlock, read the four IDCODE bytes, and accept only the three known
/// revisions (`check_revision`).
pub fn check_revision<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    id2_address: u16,
) -> Result<u8, i32> {
    client::write_byte(transport, bus, id2_address, UNLOCK_TEST_REG, UNLOCK_TEST_CODE)?;
    let low = client::read_byte(transport, bus, id2_address, IDCODE_7_0_REG)? as u32;
    let mid_low = client::read_byte(transport, bus, id2_address, IDCODE_15_8_REG)? as u32;
    let mid_high = client::read_byte(transport, bus, id2_address, IDCODE_23_16_REG)? as u32;
    let high = client::read_byte(transport, bus, id2_address, IDCODE_31_24_REG)? as u32;
    let idcode = (high << 24) | (mid_high << 16) | (mid_low << 8) | low;
    revision_index(idcode).ok_or(-minix_types::EIO)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_the_three_revisions_pass() {
        assert_eq!(revision_index(0x0009_002F), Some(0));
        assert_eq!(revision_index(0x1009_002F), Some(1));
        assert_eq!(revision_index(0x3009_002F), Some(2));
        assert_eq!(revision_index(0), None);
        assert_eq!(revision_index(0xFFFF_FFFF), None);
    }
}
