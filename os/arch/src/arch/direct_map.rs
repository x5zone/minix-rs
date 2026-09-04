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

/// Architecture abstraction for Direct Map address space layout
/// ([ARCH: A-10]: Minix3's duplicated `ARCH_VM_*` macro families become
/// a per-architecture trait implementation).
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

    /// Size in bytes of the VM-accessible Direct Map window.
    ///
    /// Invariant (asserted per architecture below): `VM_HEAP_BASE ==
    /// VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE`. Previously hardcoded in
    /// the VM server (`direct_map.rs`), which silently drifted on
    /// architectures with a larger window (V10-P2-2).
    const VM_DIRECT_MAP_SIZE: u64;

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
///
/// VM direct map sits at 2GB in user space; kernel direct map occupies the
/// PML4 slot immediately after the kernel image's slot: the kernel image is
/// linked at the kernel-half base `0xFFFF_8000_0000_0000` (PML4[256], see
/// `os/kernel/src/arch/x86_64/link.ld`), so the DM window must NOT share
/// that slot — boot maps the image there with `VA = kern_virt_base + offset`
/// translations that do not follow DM semantics (`VA = DM base + PA`).
/// `0xFFFF_8080_0000_0000` is PML4[257], a slot the bootstrap root never
/// touches outside DM establishment (07-paging_init_design §6.1).
pub struct X86_64DirectMap;

impl DirectMapArch for X86_64DirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_0000_8000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8080_0000_0000;
    const VM_HEAP_BASE: u64 = 0x0000_0000_C000_0000;
    const VM_DIRECT_MAP_SIZE: u64 = 0x0000_0000_4000_0000; // 1 GiB
    const VM_HEAP_SIZE: u64 = 64 * 1024 * 1024;
}

const _: () = assert!(
    <X86_64DirectMap as DirectMapArch>::VM_HEAP_BASE
        == <X86_64DirectMap as DirectMapArch>::VM_DIRECT_MAP_BASE
            + <X86_64DirectMap as DirectMapArch>::VM_DIRECT_MAP_SIZE,
    "x86-64: VM_HEAP_BASE must immediately follow the VM direct map window"
);

/// AArch64 Direct Map address space layout.
///
/// ARM64 uses two translation regions: TTBR0 (user, VA[47:0]) and TTBR1
/// (kernel, VA[63:48]=0xFFFF). The VM direct map is placed in the user
/// region at `0x0000_1000_0000_0000`; the kernel direct map takes the L0
/// slot immediately after the kernel image's slot (`0xFFFF_8080_0000_0000`
/// = L0[257], image linked at L0[256] = `0xFFFF_8000_0000_0000` per
/// `os/kernel/src/arch/aarch64/link.ld`), for cross-arch uniformity with
/// x86-64: the DM window never shares a top-level slot with the image.
///
/// The window spans 2 GiB of PA space: QEMU virt places RAM base at
/// 1 GiB, so a 1 GiB window (PA [0, 1 GiB)) would leave the entire RAM
/// range outside DM representability (eligible = ∅). The 2 GiB window
/// keeps the platform RAM base inside the window per the DM-window
/// admissibility precondition (`07-paging_init_design` §6.1).
///
/// See `07-cross-space-init.md` §4.1 for the address-space layout table.
pub struct AArch64DirectMap;

impl DirectMapArch for AArch64DirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_1000_0000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8080_0000_0000;
    const VM_HEAP_BASE: u64 = 0x0000_1000_8000_0000;
    const VM_DIRECT_MAP_SIZE: u64 = 0x0000_0000_8000_0000; // 2 GiB
    const VM_HEAP_SIZE: u64 = 64 * 1024 * 1024;
}

const _: () = assert!(
    <AArch64DirectMap as DirectMapArch>::VM_HEAP_BASE
        == <AArch64DirectMap as DirectMapArch>::VM_DIRECT_MAP_BASE
            + <AArch64DirectMap as DirectMapArch>::VM_DIRECT_MAP_SIZE,
    "aarch64: VM_HEAP_BASE must immediately follow the VM direct map window"
);

/// RISC-V 64 (Sv39) Direct Map address space layout.
///
/// Sv39 provides a 39-bit virtual address space: VA[38:0]. The kernel
/// image is linked at the Sv39 canonical high base `0xFFFF_FFC0_0000_0000`
/// (VPN[2] = 256, see `os/kernel/src/arch/riscv64/link.ld`); the kernel
/// direct map takes the next top-level slot, VPN[2] = 257
/// (`0xFFFF_FFC0_4000_0000`), so the window never shares a slot with the
/// image. The previous base `0xFFFF_FC00_0000_0000` was a non-canonical
/// address whose 39-bit payload decodes to VPN[2] = 256 — the image's own
/// slot (the same L2-slot-conflict class documented in
/// `02-higher-half-kernel.md` Appendix A). The VM direct map is placed at
/// `0x0000_0010_0000_0000` (64GB offset in the user low half).
///
/// See `07-cross-space-init.md` §4.1 for the address-space layout table.
pub struct Riscv64DirectMap;

impl DirectMapArch for Riscv64DirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_0010_0000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_FFC0_4000_0000;
    const VM_HEAP_BASE: u64 = 0x0000_0014_0000_0000;
    const VM_DIRECT_MAP_SIZE: u64 = 0x0000_0004_0000_0000; // 16 GiB
    const VM_HEAP_SIZE: u64 = 64 * 1024 * 1024;
}

const _: () = assert!(
    <Riscv64DirectMap as DirectMapArch>::VM_HEAP_BASE
        == <Riscv64DirectMap as DirectMapArch>::VM_DIRECT_MAP_BASE
            + <Riscv64DirectMap as DirectMapArch>::VM_DIRECT_MAP_SIZE,
    "riscv64: VM_HEAP_BASE must immediately follow the VM direct map window"
);

/// Mock Direct Map — configurable base addresses for testing.
///
/// Before QEMU integration, all tests run in mock mode. This implementation
/// allows setting the VM Direct Map offset at runtime via `set_vm_base()`,
/// so tests can verify `vm_phys_to_virt()` / `virt_to_phys()` without
/// depending on real architecture constants.
pub struct MockDirectMap;

impl DirectMapArch for MockDirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_0000_8000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8080_0000_0000;
    const VM_HEAP_BASE: u64 = 0x0000_0000_C000_0000;
    const VM_DIRECT_MAP_SIZE: u64 = 0x0000_0000_4000_0000; // 1 GiB
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
