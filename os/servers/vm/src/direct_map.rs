use minix_types::VirBytes;
use crate::phys_mem::PhysBytes;

pub(crate) const VM_DIRECT_MAP_BASE: u64 = 0x0000_0000_8000_0000;
pub(crate) const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

#[inline]
#[cfg(not(test))]
pub(crate) fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes {
    VirBytes(phys.as_u64() + VM_DIRECT_MAP_BASE)
}

#[cfg(test)]
pub mod mock_map {
    use std::sync::atomic::{AtomicU64, Ordering};

    static MOCK_OFFSET: AtomicU64 = AtomicU64::new(0);

    pub fn set_offset(offset: u64) {
        MOCK_OFFSET.store(offset, Ordering::Relaxed);
    }

    pub fn offset() -> u64 {
        MOCK_OFFSET.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
pub fn set_mock_phys_base(base: u64) {
    mock_map::set_offset(base);
}

#[inline]
#[cfg(test)]
pub(crate) fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes {
    VirBytes(phys.as_u64() + mock_map::offset())
}

#[inline]
pub(crate) fn kernel_phys_to_virt(phys: PhysBytes) -> VirBytes {
    VirBytes(phys.as_u64() + KERNEL_DIRECT_MAP_BASE)
}

#[inline]
pub(crate) fn virt_to_phys(virt: VirBytes) -> PhysBytes {
    #[cfg(not(test))]
    {
        if virt.0 >= KERNEL_DIRECT_MAP_BASE {
            PhysBytes::new(virt.0 - KERNEL_DIRECT_MAP_BASE)
        } else {
            PhysBytes::new(virt.0 - VM_DIRECT_MAP_BASE)
        }
    }
    #[cfg(test)]
    {
        let offset = mock_map::offset();
        if virt.0 >= KERNEL_DIRECT_MAP_BASE {
            PhysBytes::new(virt.0 - KERNEL_DIRECT_MAP_BASE)
        } else if virt.0 >= offset {
            PhysBytes::new(virt.0 - offset)
        } else {
            PhysBytes::new(virt.0)
        }
    }
}

#[inline]
pub(crate) fn is_direct_map_virt(virt: VirBytes) -> bool {
    virt.0 >= KERNEL_DIRECT_MAP_BASE || (virt.0 >= VM_DIRECT_MAP_BASE && virt.0 < VM_DIRECT_MAP_BASE + (1u64 << 30))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_phys_to_virt() {
        set_mock_phys_base(0x1000_0000);
        let phys = PhysBytes::new(0x1000);
        let virt = vm_phys_to_virt(phys);
        assert_eq!(virt.0, 0x1000_0000 + 0x1000);
    }

    #[test]
    fn test_kernel_phys_to_virt() {
        let phys = PhysBytes::new(0x1000);
        let virt = kernel_phys_to_virt(phys);
        assert_eq!(virt.0, KERNEL_DIRECT_MAP_BASE + 0x1000);
    }

    #[test]
    fn test_virt_to_phys_roundtrip() {
        set_mock_phys_base(0x1000_0000);
        let phys = PhysBytes::new(0x2000);
        assert_eq!(virt_to_phys(vm_phys_to_virt(phys)), phys);
        assert_eq!(virt_to_phys(kernel_phys_to_virt(phys)), phys);
    }

    #[test]
    fn test_is_direct_map_virt() {
        assert!(is_direct_map_virt(VirBytes(KERNEL_DIRECT_MAP_BASE)));
        assert!(!is_direct_map_virt(VirBytes(0x7000_0000)));
    }
}
