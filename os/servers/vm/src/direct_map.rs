//! Direct Map address translation.
//!
//! Provides bidirectional conversion between physical addresses (`AlignedPhysBytes`)
//! and virtual addresses (`VirBytes`) via the Direct Map region.
//! Delegates to `minix_arch::CurrentDirectMap` — the compile-time selected
//! architecture's `DirectMapArch` implementation.

use minix_types::VirBytes;
use minix_arch::{CurrentDirectMap, DirectMapArch};
use crate::phys_mem::AlignedPhysBytes;

// V10-P2-2: production only uses VM_DIRECT_MAP_SIZE + vm_phys_to_virt; the
// base re-exports serve tests and the DEAD virt/phys helpers above.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const VM_DIRECT_MAP_BASE: u64 = CurrentDirectMap::VM_DIRECT_MAP_BASE;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const KERNEL_DIRECT_MAP_BASE: u64 = CurrentDirectMap::KERNEL_DIRECT_MAP_BASE;
// V10-P2-2: size now comes from `DirectMapArch` (per-arch window) instead
// of a VM-server-local hardcode that silently drifted on RISC-V (16 GiB).
pub(crate) const VM_DIRECT_MAP_SIZE: u64 = CurrentDirectMap::VM_DIRECT_MAP_SIZE;

pub(crate) const VM_HEAP_BASE: u64 = CurrentDirectMap::VM_HEAP_BASE;
pub(crate) const VM_HEAP_SIZE: u64 = CurrentDirectMap::VM_HEAP_SIZE;
pub(crate) const VM_HEAP_LIMIT: u64 = VM_HEAP_BASE + VM_HEAP_SIZE;

#[inline]
pub(crate) fn vm_phys_to_virt(phys: AlignedPhysBytes) -> VirBytes {
    #[cfg(test)]
    {
        // V11/T26: in test builds the runtime window base lives in a
        // thread-local so every test thread gets an isolated window (the
        // former process-global mock-base mutex serialization is gone).
        // The arithmetic mirrors `MockDirectMap::vm_phys_to_virt`.
        VirBytes(test_vm_base() + phys.as_u64())
    }
    #[cfg(not(test))]
    {
        CurrentDirectMap::vm_phys_to_virt(phys.into())
    }
}

// V10-P2-2: DEAD in production (only `vm_phys_to_virt` is used); kept for
// tests and the future kernel-IPC wiring. Revisit: either wire into a
// heap_arena self-map assertion or delete.
#[cfg_attr(not(test), allow(dead_code))]
#[inline]
pub(crate) fn kernel_phys_to_virt(phys: AlignedPhysBytes) -> VirBytes {
    CurrentDirectMap::kernel_phys_to_virt(phys.into())
}

#[cfg_attr(not(test), allow(dead_code))]
#[inline]
pub(crate) fn virt_to_phys(virt: VirBytes) -> AlignedPhysBytes {
    #[cfg(test)]
    {
        let phys = if virt.0 >= KERNEL_DIRECT_MAP_BASE {
            virt.0 - KERNEL_DIRECT_MAP_BASE
        } else {
            virt.0 - test_vm_base()
        };
        AlignedPhysBytes::new_unchecked(phys)
    }
    #[cfg(not(test))]
    {
        let phys = CurrentDirectMap::virt_to_phys(virt);
        AlignedPhysBytes::new_unchecked(phys.get())
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[inline]
pub(crate) fn is_direct_map_virt(virt: VirBytes) -> bool {
    // VM window is bounded by VM_DIRECT_MAP_SIZE (V10-P2-2). The kernel
    // window has no explicit size constant yet (kernel layout constants
    // live in BootParams/KERNEL_LAYOUT); the high-half check is kept and
    // documented as such — the function is DEAD in production today.
    virt.0 >= KERNEL_DIRECT_MAP_BASE
        || (virt.0 >= VM_DIRECT_MAP_BASE && virt.0 < VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE)
}

// ── Test support: per-thread direct-map window (V11/T26) ────────────
//
// The VM's direct-map window base is runtime state — the kernel grants the
// window to the userspace VM at boot (pre-E3 the arch crate's placeholder
// stands in). Tests need the window pointed at real, writable memory, and
// libtest runs each test on its own thread, so the base lives in a
// thread-local: every test thread installs its own window, and no global
// serialization is needed. The former process-global mutex + save/restore
// + catch_unwind machinery existed to protect a process-global; per-thread
// storage removes the sharing that required protecting.

#[cfg(test)]
thread_local! {
    static TEST_VM_BASE: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

/// The direct-map window base in effect on the current test thread.
#[cfg(test)]
pub(crate) fn test_vm_base() -> u64 {
    TEST_VM_BASE.with(|c| c.get())
}

/// Run `f` with the direct-map window pointed at a freshly leaked,
/// CLICK_SIZE-aligned buffer of `pages` pages.
///
/// Each call owns its buffer exclusively: the current thread's window is
/// the only writer, and the buffer dies with the test process (leaked on
/// purpose — the "physical memory" outlives the allocator state under
/// test, mirroring how real physical memory outlives kernel objects).
#[cfg(test)]
pub(crate) fn with_test_window<F: FnOnce() -> R, R>(pages: usize, f: F) -> R {
    use crate::phys_mem::CLICK_SIZE;
    let mock_phys: alloc::vec::Vec<u8> = alloc::vec![0u8; pages * CLICK_SIZE + CLICK_SIZE];
    let leaked = alloc::boxed::Box::leak(mock_phys.into_boxed_slice());
    let raw = leaked.as_ptr() as usize;
    let aligned = ((raw + CLICK_SIZE - 1) & !(CLICK_SIZE - 1)) as u64;
    TEST_VM_BASE.with(|c| c.set(aligned));
    f()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn test_vm_phys_to_virt() {
        let base = with_test_window(8, || {
            let phys = AlignedPhysBytes::new(0x1000);
            vm_phys_to_virt(phys).0
        });
        assert!(base > 0x1000, "window base must be real (leaked) memory");
    }

    #[test]
    fn test_vm_phys_to_virt_with_real_constant() {
        let (base, virt) = with_test_window(8, || {
            let phys = AlignedPhysBytes::new(0x1000);
            (test_vm_base(), vm_phys_to_virt(phys).0)
        });
        assert_eq!(virt, base + 0x1000);
    }

    #[test]
    fn test_kernel_phys_to_virt() {
        let phys = AlignedPhysBytes::new(0x1000);
        let virt = kernel_phys_to_virt(phys);
        assert_eq!(virt.0, KERNEL_DIRECT_MAP_BASE + 0x1000);
    }

    #[test]
    fn test_virt_to_phys_roundtrip() {
        with_test_window(8, || {
            let phys = AlignedPhysBytes::new(0x2000);
            assert_eq!(virt_to_phys(vm_phys_to_virt(phys)), phys);
            assert_eq!(virt_to_phys(kernel_phys_to_virt(phys)), phys);
        });
    }

    #[test]
    fn test_is_direct_map_virt() {
        assert!(is_direct_map_virt(VirBytes(VM_DIRECT_MAP_BASE)));
        assert!(is_direct_map_virt(VirBytes(VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE - 1)));
        assert!(!is_direct_map_virt(VirBytes(VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE)));
        assert!(is_direct_map_virt(VirBytes(KERNEL_DIRECT_MAP_BASE)));
        assert!(!is_direct_map_virt(VirBytes(0x7000_0000)));
    }

    #[test]
    fn test_layout_invariant_heap_follows_direct_map() {
        // V10-P2-2: VM_HEAP_BASE must immediately follow the VM direct map
        // window (per-arch const-assert also enforces this at compile time).
        assert_eq!(VM_HEAP_BASE, VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE);
    }

    #[test]
    fn test_windows_are_per_thread_and_independent() {
        // V11/T26: two sequential windows on one thread get independent
        // bases (the second install replaces the first), proving tests no
        // longer share window state.
        let first = with_test_window(2, || test_vm_base());
        let second = with_test_window(2, || {
            let phys = AlignedPhysBytes::new(0x1000);
            vm_phys_to_virt(phys).0 - 0x1000
        });
        assert_ne!(first, second);
        assert_eq!(second, test_vm_base());
    }
}
