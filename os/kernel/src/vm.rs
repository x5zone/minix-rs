//! Kernel virtual memory module.

/// Address space.
pub struct AddressSpace;

impl AddressSpace {
    pub fn new() -> Self {
        Self
    }
}

/// Copies address space.
pub fn copy_address_space(_as: &AddressSpace) -> AddressSpace {
    // TODO: Implement address space copy
    AddressSpace::new()
}
