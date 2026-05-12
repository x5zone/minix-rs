//! Direct Map address translation.
//!
//! Provides bidirectional conversion between physical addresses (`AlignedPhysBytes`)
//! and virtual addresses (`VirBytes`) via the Direct Map region.
//! Delegates to `minix_arch::CurrentDirectMap` — the compile-time selected
//! architecture's `DirectMapArch` implementation.

use minix_types::VirBytes;
use minix_arch::{CurrentDirectMap, DirectMapArch};
use crate::phys_mem::AlignedPhysBytes;

pub(crate) const VM_DIRECT_MAP_BASE: u64 = CurrentDirectMap::VM_DIRECT_MAP_BASE;
pub(crate) const KERNEL_DIRECT_MAP_BASE: u64 = CurrentDirectMap::KERNEL_DIRECT_MAP_BASE;

pub(crate) const VM_DIRECT_MAP_SIZE: u64 = 1 << 30;

#[inline]
pub(crate) fn vm_phys_to_virt(phys: AlignedPhysBytes) -> VirBytes {
    CurrentDirectMap::vm_phys_to_virt(phys.into())
}

#[inline]
pub(crate) fn kernel_phys_to_virt(phys: AlignedPhysBytes) -> VirBytes {
    CurrentDirectMap::kernel_phys_to_virt(phys.into())
}

#[inline]
pub(crate) fn virt_to_phys(virt: VirBytes) -> AlignedPhysBytes {
    let phys = CurrentDirectMap::virt_to_phys(virt);
    AlignedPhysBytes::new_unchecked(phys.get())
}

#[inline]
pub(crate) fn is_direct_map_virt(virt: VirBytes) -> bool {
    virt.0 >= KERNEL_DIRECT_MAP_BASE || (virt.0 >= VM_DIRECT_MAP_BASE && virt.0 < VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_phys_to_virt() {
        minix_arch::direct_map::set_mock_vm_base(0x1000_0000);
        let phys = AlignedPhysBytes::new(0x1000);
        let virt = vm_phys_to_virt(phys);
        assert_eq!(virt.0, 0x1000_0000 + 0x1000);
    }

    #[test]
    fn test_vm_phys_to_virt_with_real_constant() {
        minix_arch::direct_map::set_mock_vm_base(VM_DIRECT_MAP_BASE);
        let phys = AlignedPhysBytes::new(0x1000);
        let virt = vm_phys_to_virt(phys);
        assert_eq!(virt.0, VM_DIRECT_MAP_BASE + 0x1000);
        assert_eq!(virt_to_phys(virt), phys);
    }

    #[test]
    fn test_kernel_phys_to_virt() {
        let phys = AlignedPhysBytes::new(0x1000);
        let virt = kernel_phys_to_virt(phys);
        assert_eq!(virt.0, KERNEL_DIRECT_MAP_BASE + 0x1000);
    }

    #[test]
    fn test_virt_to_phys_roundtrip() {
        minix_arch::direct_map::set_mock_vm_base(0x1000_0000);
        let phys = AlignedPhysBytes::new(0x2000);
        assert_eq!(virt_to_phys(vm_phys_to_virt(phys)), phys);
        assert_eq!(virt_to_phys(kernel_phys_to_virt(phys)), phys);
    }

    #[test]
    fn test_is_direct_map_virt() {
        assert!(is_direct_map_virt(VirBytes(VM_DIRECT_MAP_BASE)));
        assert!(is_direct_map_virt(VirBytes(KERNEL_DIRECT_MAP_BASE)));
        assert!(!is_direct_map_virt(VirBytes(0x7000_0000)));
    }
}
