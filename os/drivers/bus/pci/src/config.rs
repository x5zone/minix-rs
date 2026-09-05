//! Configuration space: addresses, registers, and access width.
//!
//! C correspondence: the port-level register helpers `pcii_rreg8/16/32`
//! and `pcii_wreg8/16/32` plus the index-level `__pci_attr_*` wrappers in
//! `minix3/minix/drivers/bus/pci/pci.c:149-377`.
//!
//! Port numbers never enter the operating-system layer: the trait moves
//! values at (bus, device, function, offset) with an explicit width, and
//! the board side implements how a port cycle works.

/// Width of one configuration access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessWidth {
    /// Eight bits.
    Bits8,
    /// Sixteen bits.
    Bits16,
    /// Thirty-two bits.
    Bits32,
}

impl AccessWidth {
    /// Bytes moved by one access of this width.
    pub const fn bytes(self) -> usize {
        match self {
            AccessWidth::Bits8 => 1,
            AccessWidth::Bits16 => 2,
            AccessWidth::Bits32 => 4,
        }
    }
}

/// Configuration address: bus, device, function, register offset.
///
/// Offsets must be naturally aligned for the access width (the C helpers
/// take separate eight, sixteen, and thirty-two-bit entries, so
/// misaligned access has no defined meaning).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConfigAddress {
    /// Bus number (zero-based).
    pub bus: u8,
    /// Device number on the bus.
    pub device: u8,
    /// Function number within the device.
    pub function: u8,
    /// Register offset within the 256-byte space.
    pub offset: u8,
    /// Access width.
    pub width: AccessWidth,
}

impl ConfigAddress {
    /// True when the offset suits the width (aligned, inside the space).
    pub const fn is_valid(self) -> bool {
        let mask = match self.width {
            AccessWidth::Bits8 => 0,
            AccessWidth::Bits16 => 1,
            AccessWidth::Bits32 => 3,
        };
        (self.offset & mask) == 0
    }
}

/// Configuration-space access failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// Misaligned offset or unknown device.
    Invalid,
    /// Board refused the cycle.
    BoardFault,
}

/// Configuration-space behavior for one machine.
///
/// C: the `pcii_*` family reads and writes; the `__pci_attr_*` family
/// resolves an enumerated index to an address first. Index resolution
/// lives with the database module; this trait moves values.
pub trait ConfigSpace {
    /// Read one register.
    fn read(&mut self, address: ConfigAddress) -> Result<u32, ConfigError>;
    /// Write one register (value truncated to the access width).
    fn write(&mut self, address: ConfigAddress, value: u32) -> Result<(), ConfigError>;
}

/// Null space: every cycle fails as invalid (no hardware wired).
#[derive(Debug, Default, Clone, Copy)]
pub struct NullConfigSpace;

impl ConfigSpace for NullConfigSpace {
    fn read(&mut self, _address: ConfigAddress) -> Result<u32, ConfigError> {
        Err(ConfigError::Invalid)
    }

    fn write(&mut self, _address: ConfigAddress, _value: u32) -> Result<(), ConfigError> {
        Err(ConfigError::Invalid)
    }
}

/// Memory space for tests: 256 registers of thirty-two bits per device
/// key, held in a vector of (key, cells) pairs.
#[derive(Debug, Default, Clone)]
pub struct MemConfigSpace {
    devices: alloc::vec::Vec<([u8; 3], [u32; 64])>,
}

impl MemConfigSpace {
    /// Fresh empty space.
    pub fn new() -> MemConfigSpace {
        MemConfigSpace {
            devices: alloc::vec::Vec::new(),
        }
    }

    fn cells(&mut self, address: ConfigAddress) -> &mut [u32; 64] {
        let key = [address.bus, address.device, address.function];
        let position = self.devices.iter().position(|entry| entry.0 == key);
        let index = match position {
            Some(index) => index,
            None => {
                self.devices.push((key, [0xFFFF_FFFF; 64]));
                self.devices.len() - 1
            }
        };
        &mut self.devices[index].1
    }
}

impl ConfigSpace for MemConfigSpace {
    fn read(&mut self, address: ConfigAddress) -> Result<u32, ConfigError> {
        if !address.is_valid() {
            return Err(ConfigError::Invalid);
        }
        let slot = address.offset as usize / 4;
        let raw = self.cells(address)[slot];
        Ok(match address.width {
            AccessWidth::Bits8 => {
                let shift = (address.offset % 4) * 8;
                (raw >> shift) & 0xFF
            }
            AccessWidth::Bits16 => {
                let shift = (address.offset % 4) * 8;
                (raw >> shift) & 0xFFFF
            }
            AccessWidth::Bits32 => raw,
        })
    }

    fn write(&mut self, address: ConfigAddress, value: u32) -> Result<(), ConfigError> {
        if !address.is_valid() {
            return Err(ConfigError::Invalid);
        }
        let slot = address.offset as usize / 4;
        let cells = self.cells(address);
        match address.width {
            AccessWidth::Bits8 => {
                let shift = (address.offset % 4) * 8;
                cells[slot] = (cells[slot] & !(0xFF << shift)) | ((value & 0xFF) << shift);
            }
            AccessWidth::Bits16 => {
                cells[slot] = (cells[slot] & 0xFFFF_0000) | (value & 0xFFFF);
            }
            AccessWidth::Bits32 => {
                cells[slot] = value;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(offset: u8, width: AccessWidth) -> ConfigAddress {
        ConfigAddress {
            bus: 0,
            device: 5,
            function: 0,
            offset,
            width,
        }
    }

    #[test]
    fn test_alignment_rules() {
        assert!(addr(0, AccessWidth::Bits32).is_valid());
        assert!(!addr(2, AccessWidth::Bits32).is_valid());
        assert!(addr(2, AccessWidth::Bits16).is_valid());
        assert!(addr(3, AccessWidth::Bits8).is_valid());
    }

    #[test]
    fn test_null_space_refuses_everything() {
        let mut space = NullConfigSpace;
        assert_eq!(
            space.read(addr(0, AccessWidth::Bits32)),
            Err(ConfigError::Invalid)
        );
    }

    #[test]
    fn test_memory_space_round_trips_widths() {
        let mut space = MemConfigSpace::new();
        space
            .write(addr(0, AccessWidth::Bits32), 0x1234_5678)
            .unwrap();
        assert_eq!(
            space.read(addr(0, AccessWidth::Bits32)).unwrap(),
            0x1234_5678
        );
        space.write(addr(0, AccessWidth::Bits8), 0xAA).unwrap();
        assert_eq!(
            space.read(addr(0, AccessWidth::Bits32)).unwrap(),
            0x1234_56AA
        );
        assert_eq!(space.read(addr(2, AccessWidth::Bits16)).unwrap(), 0x1234);
    }

    #[test]
    fn test_misaligned_access_is_invalid() {
        let mut space = MemConfigSpace::new();
        assert_eq!(
            space.read(addr(1, AccessWidth::Bits32)),
            Err(ConfigError::Invalid)
        );
    }
}
