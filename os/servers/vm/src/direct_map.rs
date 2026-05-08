use minix_types::VirBytes;
use crate::phys_mem::PhysBytes;

pub(crate) const VM_DIRECT_MAP_BASE: u64 = 0x0000_0000_8000_0000;
pub(crate) const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

#[inline]
pub(crate) fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes {
    VirBytes(phys.as_u64() + VM_DIRECT_MAP_BASE)
}

#[inline]
pub(crate) fn kernel_phys_to_virt(phys: PhysBytes) -> VirBytes {
    VirBytes(phys.as_u64() + KERNEL_DIRECT_MAP_BASE)
}

#[inline]
pub(crate) fn virt_to_phys(virt: VirBytes) -> PhysBytes {
    if virt.0 >= KERNEL_DIRECT_MAP_BASE {
        PhysBytes::new(virt.0 - KERNEL_DIRECT_MAP_BASE)
    } else {
        PhysBytes::new(virt.0 - VM_DIRECT_MAP_BASE)
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
        let phys = PhysBytes::new(0x1000);
        let virt = vm_phys_to_virt(phys);
        assert_eq!(virt.0, VM_DIRECT_MAP_BASE + 0x1000);
    }

    #[test]
    fn test_kernel_phys_to_virt() {
        let phys = PhysBytes::new(0x1000);
        let virt = kernel_phys_to_virt(phys);
        assert_eq!(virt.0, KERNEL_DIRECT_MAP_BASE + 0x1000);
    }

    #[test]
    fn test_virt_to_phys_roundtrip() {
        let phys = PhysBytes::new(0x2000);
        assert_eq!(virt_to_phys(vm_phys_to_virt(phys)), phys);
        assert_eq!(virt_to_phys(kernel_phys_to_virt(phys)), phys);
    }

    #[test]
    fn test_is_direct_map_virt() {
        assert!(is_direct_map_virt(VirBytes(VM_DIRECT_MAP_BASE)));
        assert!(is_direct_map_virt(VirBytes(VM_DIRECT_MAP_BASE + 0x1000)));
        assert!(is_direct_map_virt(VirBytes(KERNEL_DIRECT_MAP_BASE)));
        assert!(!is_direct_map_virt(VirBytes(0x7000_0000)));
    }
}
