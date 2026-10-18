//! The chip-identity and interrupt decisions.
//!
//! C correspondence: `check_revision` (`tps65217.c:112-152`: the four
//! accepted chip masks, anything else refused) and `intr_handler`'s
//! status read (`tps65217.c:225-250`: the power-button bit means "tell
//! init"; any other status only re-enables the interrupt).

use crate::registers::{
    PBI_MASK, TPS65217A_CHIP_MASK, TPS65217B_CHIP_MASK, TPS65217C_CHIP_MASK, TPS65217D_CHIP_MASK,
};

/// Whether the chip byte names a TPS65217 revision the driver serves.
pub const fn chip_recognized(chipid: u8) -> bool {
    matches!(
        chipid & 0xF0,
        TPS65217A_CHIP_MASK | TPS65217B_CHIP_MASK | TPS65217C_CHIP_MASK | TPS65217D_CHIP_MASK
    )
}

/// What one interrupt-status read means to the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    /// The power button fired: init is told to shut down.
    PowerButton,
    /// Some other (or no) status: the interrupt is simply re-enabled.
    Other,
}

/// Decide from the interrupt register value (`intr_handler`).
pub const fn interrupt_of(value: u8) -> Interrupt {
    if value & PBI_MASK != 0 {
        Interrupt::PowerButton
    } else {
        Interrupt::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{DEFAULT_INT_MASK, PWR_OFF_MASK};

    #[test]
    fn test_only_the_four_documented_chips_pass() {
        assert!(chip_recognized(0x70));
        assert!(chip_recognized(0xF5));
        assert!(chip_recognized(0xE0));
        assert!(chip_recognized(0x61));
        assert!(!chip_recognized(0x00));
        assert!(!chip_recognized(0x10));
        assert!(!chip_recognized(0x80));
    }

    #[test]
    fn test_power_button_bit_decides_the_path() {
        assert_eq!(interrupt_of(0x04), Interrupt::PowerButton);
        assert_eq!(interrupt_of(0x07), Interrupt::PowerButton);
        assert_eq!(interrupt_of(0x00), Interrupt::Other);
        assert_eq!(interrupt_of(0x02), Interrupt::Other);
        // The written masks never carry the status bit.
        assert_eq!(DEFAULT_INT_MASK & PBI_MASK, 0);
        assert_eq!(PWR_OFF_MASK & PBI_MASK, 0);
    }
}
