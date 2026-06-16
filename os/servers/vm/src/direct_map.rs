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

pub(crate) const VM_HEAP_BASE: u64 = CurrentDirectMap::VM_HEAP_BASE;
pub(crate) const VM_HEAP_SIZE: u64 = CurrentDirectMap::VM_HEAP_SIZE;
pub(crate) const VM_HEAP_LIMIT: u64 = VM_HEAP_BASE + VM_HEAP_SIZE;

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
pub(crate) mod tests {
    use super::*;

    /// Global mutex to serialize all tests that depend on mock_vm_base.
    /// Without this, parallel tests race on the global mock_vm_base state,
    /// causing double-free panics in BitmapAllocator (which stores bitmap
    /// data at addresses derived from mock_vm_base).
    static MOCK_BASE_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Run a test with mock_vm_base set to the default VM_DIRECT_MAP_BASE.
    /// Acquires MOCK_BASE_MUTEX to prevent parallel test interference.
    /// Uses lock().unwrap_or_else() to recover from mutex poisoning
    /// (which occurs when a #[should_panic] test panics while holding the lock).
    pub(crate) fn with_mock_base_lock<F: FnOnce()>(f: F) {
        let _guard = MOCK_BASE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let saved = minix_arch::direct_map::mock_vm_base();
        minix_arch::direct_map::set_mock_vm_base(VM_DIRECT_MAP_BASE);
        f();
        minix_arch::direct_map::set_mock_vm_base(saved);
    }

    /// Run a test with a custom mock_vm_base value.
    /// Acquires MOCK_BASE_MUTEX to prevent parallel test interference.
    /// Uses lock().unwrap_or_else() to recover from mutex poisoning.
    pub(crate) fn with_custom_mock_base<F: FnOnce()>(base: u64, f: F) {
        let _guard = MOCK_BASE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let saved = minix_arch::direct_map::mock_vm_base();
        minix_arch::direct_map::set_mock_vm_base(base);
        f();
        minix_arch::direct_map::set_mock_vm_base(saved);
    }

    #[test]
    fn test_vm_phys_to_virt() {
        with_mock_base_lock(|| {
            let phys = AlignedPhysBytes::new(0x1000);
            let virt = vm_phys_to_virt(phys);
            assert_eq!(virt.0, VM_DIRECT_MAP_BASE + 0x1000);
        });
    }

    #[test]
    fn test_vm_phys_to_virt_with_real_constant() {
        with_mock_base_lock(|| {
            let phys = AlignedPhysBytes::new(0x1000);
            let virt = vm_phys_to_virt(phys);
            assert_eq!(virt.0, VM_DIRECT_MAP_BASE + 0x1000);
            assert_eq!(virt_to_phys(virt), phys);
        });
    }

    #[test]
    fn test_kernel_phys_to_virt() {
        let phys = AlignedPhysBytes::new(0x1000);
        let virt = kernel_phys_to_virt(phys);
        assert_eq!(virt.0, KERNEL_DIRECT_MAP_BASE + 0x1000);
    }

    #[test]
    fn test_virt_to_phys_roundtrip() {
        with_mock_base_lock(|| {
            let phys = AlignedPhysBytes::new(0x2000);
            assert_eq!(virt_to_phys(vm_phys_to_virt(phys)), phys);
            assert_eq!(virt_to_phys(kernel_phys_to_virt(phys)), phys);
        });
    }

    #[test]
    fn test_is_direct_map_virt() {
        assert!(is_direct_map_virt(VirBytes(VM_DIRECT_MAP_BASE)));
        assert!(is_direct_map_virt(VirBytes(KERNEL_DIRECT_MAP_BASE)));
        assert!(!is_direct_map_virt(VirBytes(0x7000_0000)));
    }
}
