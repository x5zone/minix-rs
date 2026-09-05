//! Pin database: claim, mode, level, and interrupt reads.
//!
//! C correspondence: `gpio_claim`, `gpio_pin_mode`, `gpio_read`,
//! `gpio_set`, `gpio_intr_read`, and `gpio_init` behind
//! `minix3/minix/include/minix/gpio.h`, as used by
//! `minix3/minix/drivers/system/gpio/gpio.c` (claim plus mode in
//! `add_gpio_inode`, reads in `read_hook`). Register access stays in the
//! service crate behind the [`PinHardware`] trait; this database owns the
//! claim table.

/// Pin direction and interrupt mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinMode {
    /// Digital input.
    Input,
    /// Digital output.
    Output,
}

impl PinMode {
    /// C name of the mode (`GPIO_MODE_INPUT` / `GPIO_MODE_OUTPUT`).
    pub const fn name(self) -> &'static str {
        match self {
            PinMode::Input => "GPIO_MODE_INPUT",
            PinMode::Output => "GPIO_MODE_OUTPUT",
        }
    }
}

/// Hardware behavior for one board: levels behind pin numbers.
///
/// C: the `gpio_*` functions over memory-mapped registers. Levels are
/// plain booleans here; registers never surface.
pub trait PinHardware {
    /// Drive an output pin to this level.
    fn drive(&mut self, pin: u32, high: bool) -> Result<(), PinError>;
    /// Sample an input pin level.
    fn sample(&mut self, pin: u32) -> Result<bool, PinError>;
    /// Sample a latched interrupt flag (clears on read, like hardware).
    fn sample_interrupt(&mut self, pin: u32) -> Result<bool, PinError>;
}

/// Pin failure: unmapped pin or board refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinError {
    /// No such pin on this board.
    NoSuchPin,
    /// Board refused the operation.
    BoardFault,
}

/// One claimed pin: owner, mode, last driven level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Claim {
    pin: u32,
    owner: u32,
    mode: PinMode,
}

/// Claim table plus hardware handle.
///
/// C: the claim in `add_gpio_inode` (`gpio.c`): claiming an already
/// claimed pin fails (`gpio_claim` nonzero), and the mode is programmed
/// at claim time (`gpio_pin_mode`).
pub struct PinDb<H> {
    hardware: H,
    claims: alloc::vec::Vec<Claim>,
}

/// Owner identifier for the filesystem exporter.
pub const FILESYSTEM_OWNER: u32 = 1;

impl<H: PinHardware> PinDb<H> {
    /// Fresh database over this hardware.
    pub fn new(hardware: H) -> PinDb<H> {
        PinDb {
            hardware,
            claims: alloc::vec::Vec::new(),
        }
    }

    /// Claim a pin in a mode; fails when already claimed.
    pub fn claim(&mut self, owner: u32, pin: u32, mode: PinMode) -> Result<(), PinError> {
        if self.claims.iter().any(|claim| claim.pin == pin) {
            return Err(PinError::BoardFault);
        }
        self.claims.push(Claim { pin, owner, mode });
        Ok(())
    }

    /// Mode of a claimed pin, if any.
    pub fn mode_of(&self, pin: u32) -> Option<PinMode> {
        self.claims
            .iter()
            .find(|claim| claim.pin == pin)
            .map(|claim| claim.mode)
    }

    /// Read a pin level (any claimed pin).
    pub fn read(&mut self, pin: u32) -> Result<bool, PinError> {
        if self.mode_of(pin).is_none() {
            return Err(PinError::NoSuchPin);
        }
        self.hardware.sample(pin)
    }

    /// Drive a claimed output pin; input pins refuse.
    ///
    /// C: `gpio_set` on an output-mode pin (`read_hook` on/off branch,
    /// `gpio.c`).
    pub fn drive(&mut self, pin: u32, high: bool) -> Result<(), PinError> {
        match self.mode_of(pin) {
            Some(PinMode::Output) => self.hardware.drive(pin, high),
            Some(PinMode::Input) => Err(PinError::BoardFault),
            None => Err(PinError::NoSuchPin),
        }
    }

    /// Read a latched interrupt flag.
    ///
    /// C: `gpio_intr_read` (`read_hook` interrupt branch, `gpio.c`).
    pub fn read_interrupt(&mut self, pin: u32) -> Result<bool, PinError> {
        if self.mode_of(pin).is_none() {
            return Err(PinError::NoSuchPin);
        }
        self.hardware.sample_interrupt(pin)
    }

    /// Borrow the hardware (test inspection).
    pub fn hardware(&self) -> &H {
        &self.hardware
    }

    /// Mutably borrow the hardware (raising test interrupts).
    pub fn hardware_mut(&mut self) -> &mut H {
        &mut self.hardware
    }
}

/// Memory board for tests: levels in a vector, interrupt flags latched.
#[derive(Debug, Default, Clone)]
pub struct MemBoard {
    levels: alloc::vec::Vec<(u32, bool)>,
    interrupts: alloc::vec::Vec<u32>,
}

impl MemBoard {
    /// Fresh board (all levels low, no interrupts).
    pub fn new() -> MemBoard {
        MemBoard {
            levels: alloc::vec::Vec::new(),
            interrupts: alloc::vec::Vec::new(),
        }
    }

    /// Raise an interrupt flag for a pin.
    pub fn raise_interrupt(&mut self, pin: u32) {
        if !self.interrupts.contains(&pin) {
            self.interrupts.push(pin);
        }
    }
}

impl PinHardware for MemBoard {
    fn drive(&mut self, pin: u32, high: bool) -> Result<(), PinError> {
        match self.levels.iter_mut().find(|entry| entry.0 == pin) {
            Some(entry) => entry.1 = high,
            None => self.levels.push((pin, high)),
        }
        Ok(())
    }

    fn sample(&mut self, pin: u32) -> Result<bool, PinError> {
        Ok(self
            .levels
            .iter()
            .find(|entry| entry.0 == pin)
            .map(|entry| entry.1)
            .unwrap_or(false))
    }

    fn sample_interrupt(&mut self, pin: u32) -> Result<bool, PinError> {
        let position = self.interrupts.iter().position(|flag| *flag == pin);
        match position {
            Some(index) => {
                self.interrupts.remove(index);
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

/// Null board: every operation fails (no hardware wired).
#[derive(Debug, Default, Clone, Copy)]
pub struct NullBoard;

impl PinHardware for NullBoard {
    fn drive(&mut self, _pin: u32, _high: bool) -> Result<(), PinError> {
        Err(PinError::BoardFault)
    }

    fn sample(&mut self, _pin: u32) -> Result<bool, PinError> {
        Err(PinError::NoSuchPin)
    }

    fn sample_interrupt(&mut self, _pin: u32) -> Result<bool, PinError> {
        Err(PinError::NoSuchPin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claim_twice_is_refused() {
        let mut db = PinDb::new(MemBoard::new());
        assert!(db.claim(FILESYSTEM_OWNER, 149, PinMode::Output).is_ok());
        assert!(db.claim(2, 149, PinMode::Input).is_err());
        assert_eq!(db.mode_of(149), Some(PinMode::Output));
        assert_eq!(db.mode_of(150), None);
    }

    #[test]
    fn test_output_drives_and_input_refuses_drive() {
        let mut db = PinDb::new(MemBoard::new());
        db.claim(FILESYSTEM_OWNER, 139, PinMode::Output).unwrap();
        db.claim(FILESYSTEM_OWNER, 4, PinMode::Input).unwrap();
        assert!(db.drive(139, true).is_ok());
        assert!(db.drive(4, true).is_err());
        assert_eq!(db.read(139), Ok(true));
        assert!(db.read(200).is_err());
    }

    #[test]
    fn test_interrupt_flags_latch_and_clear() {
        let mut db = PinDb::new(MemBoard::new());
        db.claim(FILESYSTEM_OWNER, 144, PinMode::Input).unwrap();
        assert_eq!(db.read_interrupt(144), Ok(false));
        db.hardware_mut().raise_interrupt(144);
        assert_eq!(db.read_interrupt(144), Ok(true));
        assert_eq!(db.read_interrupt(144), Ok(false));
    }

    #[test]
    fn test_null_board_refuses_everything() {
        let mut db = PinDb::new(NullBoard);
        db.claim(FILESYSTEM_OWNER, 1, PinMode::Output).unwrap();
        assert!(db.drive(1, true).is_err());
        assert!(db.read(1).is_err());
    }
}
