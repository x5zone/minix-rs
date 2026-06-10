//! Direct Map architecture abstraction
//!
//! Direct Map is an OS kernel software design pattern (`va = pa + BASE`),
//! not a hardware mechanism. Hardware provides only the MMU (page tables,
//! TLB, PTE flags); Direct Map is how the kernel uses that MMU to establish
//! a fixed linear offset mapping from physical to virtual addresses.
//!
//! This trait abstracts the architecture-specific part of Direct Map:
//! the virtual address base constants. Huge page parameters for building
//! the Direct Map mapping are provided by the `HugePages` trait.
//!
//! The trait is implemented per-architecture; upper-layer code uses the trait
//! methods without knowing the specific hardware details.

use minix_types::{PhysBytes, VirBytes};

/// Architecture abstraction for Direct Map address space layout.
///
/// Each architecture places its Direct Map window at a different virtual
/// address range; this trait captures only that layout difference plus
/// the arithmetic conversion functions that follow from it.
///
/// Huge page capabilities (page sizes, PTE flags, CPUID checks) belong
/// to the `HugePages` trait, not here — they are MMU parameters, not
/// address space layout.
pub trait DirectMapArch {
    /// Base virtual address of the VM-accessible Direct Map region.
    const VM_DIRECT_MAP_BASE: u64;

    /// Base virtual address of the kernel Direct Map region.
    const KERNEL_DIRECT_MAP_BASE: u64;

    /// Base virtual address of the VM HeapArena region.
    /// Located immediately after the VM Direct Map window.
    const VM_HEAP_BASE: u64;

    /// Size of the VM HeapArena region in bytes.
    const VM_HEAP_SIZE: u64;

    fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes {
        VirBytes(phys.get() + Self::VM_DIRECT_MAP_BASE)
    }

    fn kernel_phys_to_virt(phys: PhysBytes) -> VirBytes {
        VirBytes(phys.get() + Self::KERNEL_DIRECT_MAP_BASE)
    }

    fn virt_to_phys(virt: VirBytes) -> PhysBytes {
        if virt.get() >= Self::KERNEL_DIRECT_MAP_BASE {
            PhysBytes::new(virt.get() - Self::KERNEL_DIRECT_MAP_BASE)
        } else {
            PhysBytes::new(virt.get() - Self::VM_DIRECT_MAP_BASE)
        }
    }
}

/// x86-64 Direct Map address space layout.
pub struct X86_64DirectMap;

impl DirectMapArch for X86_64DirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_0000_8000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;
    const VM_HEAP_BASE: u64 = 0x0000_0000_C000_0000;
    const VM_HEAP_SIZE: u64 = 64 * 1024 * 1024;
}

/// Mock Direct Map — configurable base addresses for testing.
///
/// Before QEMU integration, all tests run in mock mode. This implementation
/// allows setting the VM Direct Map offset at runtime via `set_vm_base()`,
/// so tests can verify `vm_phys_to_virt()` / `virt_to_phys()` without
/// depending on real architecture constants.
pub struct MockDirectMap;

impl DirectMapArch for MockDirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_0000_8000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;
    const VM_HEAP_BASE: u64 = 0x0000_0000_C000_0000;
    const VM_HEAP_SIZE: u64 = 64 * 1024 * 1024;

    fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes {
        VirBytes(phys.get() + mock_base())
    }

    fn kernel_phys_to_virt(phys: PhysBytes) -> VirBytes {
        VirBytes(phys.get() + Self::KERNEL_DIRECT_MAP_BASE)
    }

    fn virt_to_phys(virt: VirBytes) -> PhysBytes {
        let vm_base = mock_base();
        if virt.get() >= Self::KERNEL_DIRECT_MAP_BASE {
            PhysBytes::new(virt.get() - Self::KERNEL_DIRECT_MAP_BASE)
        } else {
            PhysBytes::new(virt.get() - vm_base)
        }
    }
}

use core::sync::atomic::{AtomicU64, Ordering};

static MOCK_VM_BASE: AtomicU64 = AtomicU64::new(0x0000_0000_8000_0000);

fn mock_base() -> u64 {
    // Relaxed ordering is sufficient: MockDirectMap is only used in
    // single-threaded test code. There is no cross-CPU synchronization
    // requirement for test configuration values.
    MOCK_VM_BASE.load(Ordering::Relaxed)
}

/// Set the mock VM Direct Map base address (test-only).
pub fn set_mock_vm_base(base: u64) {
    MOCK_VM_BASE.store(base, Ordering::Relaxed);
}

/// Get the current mock VM Direct Map base address (test-only).
pub fn mock_vm_base() -> u64 {
    MOCK_VM_BASE.load(Ordering::Relaxed)
}
