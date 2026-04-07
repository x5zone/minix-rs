//! Kernel virtual memory module

/// 地址空间
pub struct AddressSpace;

impl AddressSpace {
    pub fn new() -> Self {
        Self
    }
}

/// 复制地址空间
pub fn copy_address_space(_as: &AddressSpace) -> AddressSpace {
    // TODO: 实现地址空间复制
    AddressSpace::new()
}
