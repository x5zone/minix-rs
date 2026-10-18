//! TPS65950 register map and identification values.
//!
//! C correspondence: `tps65950.h` — the four slave-address sub-modules
//! (`ID1` through `ID4`, `tps65950.h:8-30`), the chip-identifier
//! registers and revision values (`tps65950.h:32-45`), the unlock pair
//! (`tps65950.h:43-46`), and the RTC register block with its control and
//! status bits (`tps65950.h:52-66`).

/// Slave address of the USB sub-module (`ID1`).
pub const ID1: usize = 0;
/// Slave address of the audio/GPIO/interface sub-module (`ID2`).
pub const ID2: usize = 1;
/// Slave address of the charger/LED/keypad sub-module (`ID3`).
pub const ID3: usize = 2;
/// Slave address of the backup/PM sub-module (`ID4`).
pub const ID4: usize = 3;

/// The four fixed slave addresses (`addresses`, `tps65950.c:38-40`).
pub const ADDRESSES: [u16; 4] = [0x48, 0x49, 0x4A, 0x4B];

/// IDCODE byte 0 (`IDCODE_7_0_REG`).
pub const IDCODE_7_0_REG: u8 = 0x85;
/// IDCODE byte 1 (`IDCODE_15_8_REG`).
pub const IDCODE_15_8_REG: u8 = 0x86;
/// IDCODE byte 2 (`IDCODE_23_16_REG`).
pub const IDCODE_23_16_REG: u8 = 0x87;
/// IDCODE byte 3 (`IDCODE_31_24_REG`).
pub const IDCODE_31_24_REG: u8 = 0x88;

/// Revision 1.0 identifier (`IDCODE_REV_1_0`).
pub const IDCODE_REV_1_0: u32 = 0x0009_002F;
/// Revision 1.1 identifier (`IDCODE_REV_1_1`).
pub const IDCODE_REV_1_1: u32 = 0x1009_002F;
/// Revision 1.2 identifier (`IDCODE_REV_1_2`).
pub const IDCODE_REV_1_2: u32 = 0x3009_002F;

/// Unlock register for IDCODE/DIEID reads (`UNLOCK_TEST_REG`).
pub const UNLOCK_TEST_REG: u8 = 0x97;
/// Value that unlocks the reads (`UNLOCK_TEST_CODE`).
pub const UNLOCK_TEST_CODE: u8 = 0x49;

/// Seconds register (`SECONDS_REG`).
pub const SECONDS_REG: u8 = 0x1C;
/// Minutes register (`MINUTES_REG`).
pub const MINUTES_REG: u8 = 0x1D;
/// Hours register (`HOURS_REG`).
pub const HOURS_REG: u8 = 0x1E;
/// Days register (`DAYS_REG`).
pub const DAYS_REG: u8 = 0x1F;
/// Months register (`MONTHS_REG`).
pub const MONTHS_REG: u8 = 0x20;
/// Years register (`YEARS_REG`).
pub const YEARS_REG: u8 = 0x21;

/// RTC control register (`RTC_CTRL_REG`).
pub const RTC_CTRL_REG: u8 = 0x29;
/// Latch-the-time bit (`GET_TIME_BIT`).
pub const GET_TIME_BIT: u8 = 6;
/// Stop bit written at init (`STOP_RTC_BIT`).
pub const STOP_RTC_BIT: u8 = 0;

/// RTC status register (`RTC_STATUS_REG`).
pub const RTC_STATUS_REG: u8 = 0x2A;
/// RTC-running bit (`RUN_BIT`).
pub const RUN_BIT: u8 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_map_matches_the_header() {
        assert_eq!(ADDRESSES, [0x48, 0x49, 0x4A, 0x4B]);
        assert_eq!(IDCODE_7_0_REG, 0x85);
        assert_eq!(IDCODE_31_24_REG, 0x88);
        assert_eq!(UNLOCK_TEST_REG, 0x97);
        assert_eq!(UNLOCK_TEST_CODE, 0x49);
        assert_eq!(SECONDS_REG, 0x1C);
        assert_eq!(YEARS_REG, 0x21);
        assert_eq!(RTC_CTRL_REG, 0x29);
        assert_eq!(RTC_STATUS_REG, 0x2A);
        assert_eq!(1 << STOP_RTC_BIT, 0x01);
        assert_eq!(1 << GET_TIME_BIT, 0x40);
        assert_eq!(1 << RUN_BIT, 0x02);
    }

    #[test]
    fn test_revision_values_match_the_header() {
        assert_eq!(IDCODE_REV_1_0, 0x0009_002F);
        assert_eq!(IDCODE_REV_1_1, 0x1009_002F);
        assert_eq!(IDCODE_REV_1_2, 0x3009_002F);
        assert_eq!(ID2, 1);
        assert_eq!(ID4, 3);
    }
}
