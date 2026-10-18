//! The controller behind a trait: PCI configuration space and the mapped
//! socket registers.
//!
//! C correspondence: the `pci_*` configuration helpers
//! (`hw_probe`/`hw_init`, `ti1225.c:117-240`), the mapped aperture
//! (`vm_map_phys` over `TI_CB_BASEADDR`, `ti1225.c:143-148`), and the
//! bus rescan (`pci_rescan_bus`, `ti1225.c:426`). The service keeps the
//! sequence; the controller owns how a register access reaches the chip.

/// One TI1225 CardBus controller.
pub trait CardBusController {
    /// Probe and map the socket aperture (`hw_probe` + `hw_init`'s map);
    /// a refusal keeps the driver from starting (C returns `ENODEV`).
    fn probe(&mut self) -> Result<(), i32>;

    /// Read one PCI configuration byte (`pci_attr_r8`).
    fn config_read8(&mut self, offset: u8) -> u8;

    /// Read one PCI configuration word (`pci_attr_r16`).
    fn config_read16(&mut self, offset: u8) -> u16;

    /// Write one PCI configuration byte (`pci_attr_w8`).
    fn config_write8(&mut self, offset: u8, value: u8);

    /// Write one PCI configuration word (`pci_attr_w16`).
    fn config_write16(&mut self, offset: u8, value: u16);

    /// Read one socket register word from the mapped aperture.
    fn csr_read(&mut self, offset: usize) -> u32;

    /// Write one socket register word to the mapped aperture.
    fn csr_write(&mut self, offset: usize, value: u32);

    /// Rescan the bridge's secondary bus (`pci_rescan_bus`).
    fn rescan_bus(&mut self, bus: u8);

    /// Wait (`micro_delay`); the reset pulse needs two one-microsecond
    /// gaps in C (`ti1225.c:409-424`).
    fn delay_micros(&mut self, micros: u32);
}

/// A machine without a TI1225: probing refuses, so the driver honestly
/// does not start.
pub struct NullController;

impl CardBusController for NullController {
    fn probe(&mut self) -> Result<(), i32> {
        Err(-minix_types::ENODEV)
    }

    fn config_read8(&mut self, _offset: u8) -> u8 {
        0
    }

    fn config_read16(&mut self, _offset: u8) -> u16 {
        0
    }

    fn config_write8(&mut self, _offset: u8, _value: u8) {}

    fn config_write16(&mut self, _offset: u8, _value: u16) {}

    fn csr_read(&mut self, _offset: usize) -> u32 {
        0
    }

    fn csr_write(&mut self, _offset: usize, _value: u32) {}

    fn rescan_bus(&mut self, _bus: u8) {}

    fn delay_micros(&mut self, _micros: u32) {}
}
