//! TPS65217 register map and interrupt bits.
//!
//! C correspondence: the register offsets (`tps65217.c:9-45`), the chip
//! identification masks (`tps65217.c:52-56`: `CHIPID_REG[7:4]`),
//! the interrupt enable bits (`tps65217.c:58-64`), the interrupt status
//! bits (`tps65217.c:66-71`), and the power-off bit
//! (`tps65217.c:73-75`: `STATUS[7]`).

/// Chip identification register (`CHIPID_REG`).
pub const CHIPID_REG: u8 = 0x00;
/// Push-button path register (`PPATH_REG`).
pub const PPATH_REG: u8 = 0x01;
/// Interrupt enable/status register (`INT_REG`).
pub const INT_REG: u8 = 0x02;
/// Charger configuration registers (`CHGCONFIG0_REG`).
pub const CHGCONFIG0_REG: u8 = 0x03;
/// Charger configuration register 1.
pub const CHGCONFIG1_REG: u8 = 0x04;
/// Charger configuration register 2.
pub const CHGCONFIG2_REG: u8 = 0x05;
/// Charger configuration register 3.
pub const CHGCONFIG3_REG: u8 = 0x06;
/// White-LED control register 1 (`WLEDCTRL1_REG`).
pub const WLEDCTRL1_REG: u8 = 0x07;
/// White-LED control register 2.
pub const WLEDCTRL2_REG: u8 = 0x08;
/// Multiplexer control register (`MUXCTRL_REG`).
pub const MUXCTRL_REG: u8 = 0x09;
/// Status register (`STATUS_REG`).
pub const STATUS_REG: u8 = 0x0A;
/// Password register (`PASSWORD_REG`).
pub const PASSWORD_REG: u8 = 0x0B;
/// Power-good register (`PGOOD_REG`).
pub const PGOOD_REG: u8 = 0x0C;
/// Default power-good register (`DEFPG_REG`).
pub const DEFPG_REG: u8 = 0x0D;
/// DCDC converter defaults (`DEFDCDC1_REG`).
pub const DEFDCDC1_REG: u8 = 0x0E;
/// DCDC converter default 2.
pub const DEFDCDC2_REG: u8 = 0x0F;
/// DCDC converter default 3.
pub const DEFDCDC3_REG: u8 = 0x10;
/// Slew-rate default (`DEFSLEW_REG`).
pub const DEFSLEW_REG: u8 = 0x11;
/// LDO default 1 (`DEFLDO1_REG`).
pub const DEFLDO1_REG: u8 = 0x12;
/// LDO default 2.
pub const DEFLDO2_REG: u8 = 0x13;
/// Load-switch default 1 (`DEFLS1_REG`).
pub const DEFLS1_REG: u8 = 0x14;
/// Load-switch default 2.
pub const DEFLS2_REG: u8 = 0x15;
/// Enable register (`ENABLE_REG`).
pub const ENABLE_REG: u8 = 0x16;
/// Undervoltage-lockout default (`DEFUVLO_REG`; no register at 0x17).
pub const DEFUVLO_REG: u8 = 0x18;
/// Sequencer register 1 (`SEQ1_REG`).
pub const SEQ1_REG: u8 = 0x19;
/// Sequencer register 2.
pub const SEQ2_REG: u8 = 0x1A;
/// Sequencer register 3.
pub const SEQ3_REG: u8 = 0x1B;
/// Sequencer register 4.
pub const SEQ4_REG: u8 = 0x1C;
/// Sequencer register 5.
pub const SEQ5_REG: u8 = 0x1D;
/// Sequencer register 6.
pub const SEQ6_REG: u8 = 0x1E;

/// TPS65217A chip mask (`CHIPID_REG[7:4]`).
pub const TPS65217A_CHIP_MASK: u8 = 0x70;
/// TPS65217B chip mask.
pub const TPS65217B_CHIP_MASK: u8 = 0xF0;
/// TPS65217C chip mask.
pub const TPS65217C_CHIP_MASK: u8 = 0xE0;
/// TPS65217D chip mask.
pub const TPS65217D_CHIP_MASK: u8 = 0x60;

/// Power-button interrupt disable bit (`INT_REG[6]`).
pub const PBM_INT_DIS_BIT: u8 = 6;
/// AC-fail interrupt disable bit (`INT_REG[5]`).
pub const ACM_INT_DIS_BIT: u8 = 5;
/// USB-fail interrupt disable bit (`INT_REG[4]`).
pub const USBM_INT_DIS_BIT: u8 = 4;
/// Default interrupt mask: AC and USB disabled, power button enabled.
pub const DEFAULT_INT_MASK: u8 = (1 << ACM_INT_DIS_BIT) | (1 << USBM_INT_DIS_BIT);

/// Power-button interrupt status bit (`INT_REG[2]`).
pub const PBI_BIT: u8 = 2;
/// AC-fail interrupt status bit (`INT_REG[1]`).
pub const ACI_BIT: u8 = 1;
/// USB-fail interrupt status bit (`INT_REG[0]`).
pub const USBI_BIT: u8 = 0;
/// Power-button status mask.
pub const PBI_MASK: u8 = 1 << PBI_BIT;

/// Power-off bit (`STATUS[7]`).
pub const OFF_BIT: u8 = 7;
/// Power-off mask written to the status register.
pub const PWR_OFF_MASK: u8 = 1 << OFF_BIT;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_masks_match_the_driver_header() {
        assert_eq!(TPS65217A_CHIP_MASK, 0x70);
        assert_eq!(TPS65217B_CHIP_MASK, 0xF0);
        assert_eq!(TPS65217C_CHIP_MASK, 0xE0);
        assert_eq!(TPS65217D_CHIP_MASK, 0x60);
        assert_eq!(DEFAULT_INT_MASK, 0x30);
        assert_eq!(PBI_MASK, 0x04);
        assert_eq!(PWR_OFF_MASK, 0x80);
        assert_eq!(CHIPID_REG, 0x00);
        assert_eq!(INT_REG, 0x02);
        assert_eq!(STATUS_REG, 0x0A);
        assert_eq!(SEQ6_REG, 0x1E);
    }
}
