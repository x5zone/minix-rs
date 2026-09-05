//! Clock hardware: binary-coded-decimal helpers and the clock trait.
//!
//! C correspondence: `bcd_to_dec` and `dec_to_bcd`
//! (`readclock.c:167-177`), the `struct rtc` operations table
//! (`readclock.h`), the architecture setup (`arch_setup`), and the
//! forward operations `fwd_get_time`, `fwd_set_time`, `fwd_pwr_off`,
//! `fwd_init`, `fwd_exit` (`forward.c:52-119`).
//!
//! Port and register access stay in the service crate behind this trait:
//! the operating-system side describes what time means, the board side
//! implements how the chip holds it.

use super::protocol::{BrokenTime, NO_FLAGS};

/// Binary-coded decimal to decimal (two digits packed per byte).
///
/// C: `bcd_to_dec` (`readclock.c:167-171`): high nibble tens, low nibble
/// ones.
pub const fn bcd_to_decimal(packed: u8) -> u8 {
    ((packed >> 4) & 0x0F) * 10 + (packed & 0x0F)
}

/// Decimal to binary-coded decimal.
///
/// C: `dec_to_bcd` (`readclock.c:173-177`).
pub const fn decimal_to_bcd(value: u8) -> u8 {
    ((value / 10) << 4) | (value % 10)
}

/// Real-time-clock hardware behavior.
///
/// C: `struct rtc` (`readclock.h`): init, get, set, power-off, exit. The
/// grant-flavored requests (`_G`) share these operations (the grant is a
/// transport detail resolved before the call).
pub trait RealTimeClock {
    /// Prepare the chip; false means setup failed (init refused).
    fn init(&mut self) -> bool {
        true
    }

    /// Read the current time.
    fn get_time(&mut self, flags: i32) -> Result<BrokenTime, i32>;

    /// Write the current time.
    fn set_time(&mut self, time: &BrokenTime, flags: i32) -> Result<(), i32>;

    /// Program the power-off time.
    fn power_off(&mut self) -> Result<(), i32> {
        Ok(())
    }

    /// Release the chip.
    fn exit(&mut self) {}
}

/// Null clock: every operation fails (no chip wired).
///
/// C equivalent: an `arch_setup` that finds no clock (init refused).
#[derive(Debug, Default, Clone, Copy)]
pub struct NullClock;

impl RealTimeClock for NullClock {
    fn init(&mut self) -> bool {
        false
    }

    fn get_time(&mut self, _flags: i32) -> Result<BrokenTime, i32> {
        Err(-eio_code())
    }

    fn set_time(&mut self, _time: &BrokenTime, _flags: i32) -> Result<(), i32> {
        Err(-eio_code())
    }

    fn power_off(&mut self) -> Result<(), i32> {
        Err(-eio_code())
    }
}

/// Memory clock: the time lives in a field (tests and virtual boards).
///
/// Behavior differs from [`NullClock`] (which always fails): reads return
/// the stored time, writes store it, power-off succeeds.
#[derive(Debug, Clone, Copy)]
pub struct MemClock {
    time: BrokenTime,
    initialized: bool,
}

impl MemClock {
    /// Fresh clock holding this time, already initialized.
    pub const fn new(time: BrokenTime) -> MemClock {
        MemClock {
            time,
            initialized: true,
        }
    }

    /// Stored time (test inspection).
    pub const fn stored(self) -> BrokenTime {
        self.time
    }
}

impl RealTimeClock for MemClock {
    fn init(&mut self) -> bool {
        self.initialized = true;
        true
    }

    fn get_time(&mut self, _flags: i32) -> Result<BrokenTime, i32> {
        if !self.initialized {
            return Err(-eio_code());
        }
        Ok(self.time)
    }

    fn set_time(&mut self, time: &BrokenTime, _flags: i32) -> Result<(), i32> {
        if !time.is_plausible() {
            return Err(-einval_code());
        }
        self.time = *time;
        Ok(())
    }
}

/// Forward target: the label of the chip driver behind this one.
///
/// C: `target_label` with `fwd_set_label`/`fwd_exit` (`forward.c:43-50,
/// 116-119`). A missing label refuses initialization (`fwd_init`,
/// `forward.c:52-59`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForwardTarget {
    /// Label present (the string itself lives in the service crate).
    pub has_label: bool,
}

impl ForwardTarget {
    /// No target configured.
    pub const fn none() -> ForwardTarget {
        ForwardTarget { has_label: false }
    }

    /// Target configured.
    pub const fn labeled() -> ForwardTarget {
        ForwardTarget { has_label: true }
    }

    /// Initialization verdict: missing label is invalid.
    pub const fn init(self) -> Result<(), i32> {
        if !self.has_label {
            return Err(-einval_code());
        }
        Ok(())
    }
}

/// Flags used by the forward power-off call (none).
pub const fn forward_power_flags() -> i32 {
    NO_FLAGS
}

/// Input-output error for missing hardware.
const fn eio_code() -> i32 {
    minix_types::EIO
}

/// Invalid-argument code for bad labels and implausible times.
const fn einval_code() -> i32 {
    minix_types::EINVAL
}

#[cfg(test)]
mod tests {
    use super::super::protocol::BrokenTime;
    use super::*;

    fn noon() -> BrokenTime {
        BrokenTime {
            seconds: 0,
            minutes: 0,
            hours: 12,
            day: 1,
            month: 0,
            year: 126,
        }
    }

    #[test]
    fn test_bcd_round_trips_two_digits() {
        assert_eq!(bcd_to_decimal(0x59), 59);
        assert_eq!(decimal_to_bcd(59), 0x59);
        assert_eq!(bcd_to_decimal(decimal_to_bcd(0)), 0);
        assert_eq!(bcd_to_decimal(decimal_to_bcd(99)), 99);
    }

    #[test]
    fn test_null_clock_refuses_everything() {
        let mut clock = NullClock;
        assert!(!clock.init());
        assert!(clock.get_time(0).is_err());
        assert!(clock.set_time(&noon(), 0).is_err());
        assert!(clock.power_off().is_err());
    }

    #[test]
    fn test_memory_clock_stores_and_returns() {
        let mut clock = MemClock::new(noon());
        assert!(clock.init());
        assert_eq!(clock.get_time(0).unwrap(), noon());
        let mut evening = noon();
        evening.hours = 20;
        clock.set_time(&evening, 0).unwrap();
        assert_eq!(clock.stored().hours, 20);
        let mut bad = noon();
        bad.month = 12;
        assert!(clock.set_time(&bad, 0).is_err());
    }

    #[test]
    fn test_forward_target_needs_a_label() {
        assert!(ForwardTarget::none().init().is_err());
        assert!(ForwardTarget::labeled().init().is_ok());
        assert_eq!(forward_power_flags(), NO_FLAGS);
    }
}
