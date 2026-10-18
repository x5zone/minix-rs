//! The DEV hardware behind a trait: capability access, the exclusion
//! table, and the address translation.
//!
//! C correspondence: `find_dev` (`amddev.c:203-263`: walk the PCI
//! capability list for the AMD secure device and reserve it),
//! `read_reg`/`write_reg` (`amddev.c:265-276`: the selector/data pair
//! behind the capability pointer), `init_domain`'s contiguous allocation
//! (`amddev.c:278-316`), `sys_umap_remote` in `do_add4pci`
//! (`amddev.c:420-427`), and `add_range` (`amddev.c:434-446`).

use minix_types::Endpoint;

/// One AMD DEV controller.
pub trait IommuController {
    /// Find and reserve the AMD DEV (`find_dev`); a refusal keeps the
    /// driver from starting (C's fresh init returns the zero).
    fn probe(&mut self) -> Result<(), i32>;

    /// Read one DEV function register (`read_reg`).
    fn read_reg(&mut self, function: u32, index: u32) -> u32;

    /// Write one DEV function register (`write_reg`).
    fn write_reg(&mut self, function: u32, index: u32, value: u32);

    /// Allocate, zero, and head-set the exclusion table
    /// (`init_domain`); returns the table's bus address.
    fn allocate_table(&mut self) -> Result<u64, i32>;

    /// Translate the caller's virtual range to a bus address
    /// (`sys_umap_remote(proc, SELF, VM_D, ...)`).
    fn translate(&mut self, caller: Endpoint, start: u64, size: usize) -> Result<u64, i32>;

    /// Clear one range's exclusion bits (`add_range`).
    fn clear_pages(&mut self, busaddr: u64, size: usize);
}

/// A machine without an AMD DEV: probing refuses, so the driver honestly
/// does not start.
pub struct NullController;

impl IommuController for NullController {
    fn probe(&mut self) -> Result<(), i32> {
        Err(-minix_types::ENODEV)
    }

    fn read_reg(&mut self, _function: u32, _index: u32) -> u32 {
        0
    }

    fn write_reg(&mut self, _function: u32, _index: u32, _value: u32) {}

    fn allocate_table(&mut self) -> Result<u64, i32> {
        Err(-minix_types::EIO)
    }

    fn translate(&mut self, _caller: Endpoint, _start: u64, _size: usize) -> Result<u64, i32> {
        Err(-minix_types::EIO)
    }

    fn clear_pages(&mut self, _busaddr: u64, _size: usize) {}
}
