//! Chardriver wiring: the gpio driver as a chardriver device.
//!
//! C correspondence: `gpio.c` — the VTreeFS-exported pin files
//! (`gpio.c:287 run_vtreefs`) with claim answering EIO on collision
//! (`gpio.c:87-97`), reads rendering `%d\n` with an offset EOF
//! (`gpio.c:240-255`), and the On/Off/Intr suffix actions
//! (`gpio.c:127-157`).
//!
//! The VTreeFS host is the service's filesystem machinery; this face
//! runs the same per-file semantics over the pin database so the
//! service's node handlers stay one-liners. Data-movement split: reads
//! answer from the plan (`files::render`), writes drive through the
//! pin database.

use crate::files::{render, ExportedFile};
use crate::pins::PinDb;

/// The gpio driver's face over its pin database.
pub struct GpioFace<H> {
    pub pins: PinDb<H>,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
}

impl<H: crate::pins::PinHardware> GpioFace<H> {
    /// A face over the given hardware database.
    pub fn new(pins: PinDb<H>) -> Self {
        GpioFace {
            pins,
            opened: OpenDeviceSet::new(),
        }
    }

    /// Read one exported file at `offset`; answers the response bytes.
    ///
    /// C: the read branch (`gpio.c:240-255`) — level files render
    /// `%d\n` at the offset, actions answer zero bytes.
    pub fn read_file(&mut self, pin: u32, file: ExportedFile, offset: usize) -> Option<(u8, usize)> {
        // Level/interrupt files render; action files answer zero bytes.
        match file {
            ExportedFile::Read => {
                let value = self.pins.read(pin).ok()?;
                let (byte, eof_at, total) = render(value, offset);
                let _ = (eof_at, total);
                Some((byte, 0))
            }
            ExportedFile::Interrupt => {
                let latch = self.pins.read_interrupt(pin).ok()?;
                let (byte, eof_at, total) = render(latch, offset);
                let _ = (eof_at, total);
                Some((byte, 0))
            }
            ExportedFile::TurnOn | ExportedFile::TurnOff => None,
        }
    }

    /// Write one action file: drive the pin high or low.
    ///
    /// C: the write branch (`gpio.c:127-157`).
    pub fn write_action(&mut self, pin: u32, file: ExportedFile) -> Result<(), crate::pins::PinError> {
        match file {
            ExportedFile::TurnOn => self.pins.drive(pin, true),
            ExportedFile::TurnOff => self.pins.drive(pin, false),
            _ => Ok(()),
        }
    }
}

use minix_chardriver::protocol::OpenDeviceSet;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pins::{MemBoard, PinMode};

    fn face() -> GpioFace<MemBoard> {
        GpioFace::new(PinDb::new(MemBoard::default()))
    }

    #[test]
    fn test_level_read_renders_after_claim() {
        let mut face = face();
        face.pins.claim(1, 0, PinMode::Output).unwrap();
        face.pins.drive(0, true).unwrap();
        let (byte, _) = face.read_file(0, ExportedFile::Read, 0).unwrap();
        assert_eq!(byte, b'1');
        face.pins.drive(0, false).unwrap();
        let (byte, _) = face.read_file(0, ExportedFile::Read, 0).unwrap();
        assert_eq!(byte, b'0');
    }

    #[test]
    fn test_unclaimed_pin_read_is_none() {
        let mut face = face();
        assert!(face.read_file(3, ExportedFile::Read, 0).is_none());
    }

    #[test]
    fn test_action_write_drives_the_pin() {
        let mut face = face();
        face.pins.claim(1, 2, PinMode::Output).unwrap();
        assert!(face.write_action(2, ExportedFile::TurnOn).is_ok());
        let (byte, _) = face.read_file(2, ExportedFile::Read, 0).unwrap();
        assert_eq!(byte, b'1');
    }
}
