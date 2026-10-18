//! The SoC-specific bus transfer behind a trait.
//!
//! C correspondence: the `process` function pointer (`i2c.c:56-59`) that
//! every SoC implements (`omap_i2c.c` for the am335x boards); the chip
//! bring-up rides the same seam. The bus service keeps the protocol and
//! the reservation table; the controller owns the registers and timing.

use minix_i2cdriver::client::ExecBlock;

/// One I2C controller.
pub trait I2cController {
    /// One-time chip bring-up (`i2c_arch_init` area); a refusal keeps the
    /// bus driver from starting.
    fn probe(&mut self) -> Result<(), i32>;

    /// Run one validated exec block (`process(&ioctl_exec)`,
    /// `i2c.c:311-319`): the command and buffer arrays are the request,
    /// and a read fills the buffer in place. Returns zero on success,
    /// otherwise the error to report (the caller skips the result copy,
    /// as C does).
    fn process(&mut self, block: &mut ExecBlock) -> i32;
}

/// A machine without a wired controller: probing refuses, so the bus
/// driver honestly does not start.
pub struct NullController;

impl I2cController for NullController {
    fn probe(&mut self) -> Result<(), i32> {
        Err(-minix_types::ENODEV)
    }

    fn process(&mut self, _block: &mut ExecBlock) -> i32 {
        -minix_types::ENOSYS
    }
}
