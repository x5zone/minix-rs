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

use crate::arch::boot_arch::BootArch;
use crate::paging::Paging;
use crate::paging_ext::PageSize;
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

    /// Physical span the kernel Direct Map window can cover.
    ///
    /// This is a capacity, not the system's physical-address ceiling: a
    /// candidate must fit both this capacity and `Paging::phys_addr_limit()`.
    /// `KERNEL_DIRECT_MAP_BASE + KERNEL_DIRECT_MAP_SIZE` must be
    /// representable and must not exceed `KERNEL_DM_VA_END_LIMIT`.
    const KERNEL_DIRECT_MAP_SIZE: u64;

    /// Project-chosen exclusive upper bound of the kernel DM virtual window.
    ///
    /// Not the hardware canonical-space ceiling: it must lie inside the
    /// MMU mode this project enables. The highest page is reserved so the
    /// window end stays representable in `u64`.
    const KERNEL_DM_VA_END_LIMIT: u64;

    /// Base virtual address of the VM HeapArena region.
    /// Located immediately after the VM Direct Map window.
    const VM_HEAP_BASE: u64;

    /// Size in bytes of the VM-accessible Direct Map window.
    ///
    /// Invariant (asserted per architecture below): `VM_HEAP_BASE ==
    /// VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE`. Previously hardcoded in
    /// the VM server (`direct_map.rs`), which silently drifted on
    /// architectures with a larger window.
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

/// Both Direct Map windows of one architecture, validated as a unit.
///
/// Runtime PA↔VA arithmetic adds the architecture's `DirectMapArch`
/// constants directly. A policy that installed leaves from a layout which
/// merely "looked consistent" but differed from those constants would make
/// the mapping and the arithmetic disagree, so `validate()` requires every
/// field to equal the architecture constants it came from. The other checks
/// cover geometry a future architecture could get wrong even with equal
/// constants: zero capacity, end mismatch, unaligned capacity, illegal
/// virtual ranges, and overlapping windows.
pub struct DirectMapLayout<A: BootArch> {
    kernel_base: u64,
    kernel_pa_capacity: u64,
    kernel_va_end: u64,
    vm_base: u64,
    vm_pa_capacity: u64,
    vm_va_end: u64,
    _arch: core::marker::PhantomData<fn() -> A>,
}

impl<A: BootArch> Copy for DirectMapLayout<A> {}

impl<A: BootArch> Clone for DirectMapLayout<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: BootArch> core::fmt::Debug for DirectMapLayout<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DirectMapLayout")
            .field("kernel_base", &self.kernel_base)
            .field("kernel_pa_capacity", &self.kernel_pa_capacity)
            .field("kernel_va_end", &self.kernel_va_end)
            .field("vm_base", &self.vm_base)
            .field("vm_pa_capacity", &self.vm_pa_capacity)
            .field("vm_va_end", &self.vm_va_end)
            .finish()
    }
}

impl<A: BootArch> DirectMapLayout<A> {
    /// Build the layout from the architecture's own constants.
    pub fn from_arch() -> Self {
        let kernel_base = <A::DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_BASE;
        let kernel_pa_capacity = <A::DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_SIZE;
        let kernel_va_end = kernel_base
            .checked_add(kernel_pa_capacity)
            .expect("DirectMapArch: KERNEL_DIRECT_MAP_BASE + KERNEL_DIRECT_MAP_SIZE must fit in u64");
        Self {
            kernel_base,
            kernel_pa_capacity,
            kernel_va_end,
            vm_base: <A::DirectMap as DirectMapArch>::VM_DIRECT_MAP_BASE,
            vm_pa_capacity: <A::DirectMap as DirectMapArch>::VM_DIRECT_MAP_SIZE,
            vm_va_end: <A::DirectMap as DirectMapArch>::VM_HEAP_BASE,
            _arch: core::marker::PhantomData,
        }
    }

    /// Test-only constructor for validating the boundary itself: values that
    /// differ from the architecture constants must be rejected by `validate`.
    #[cfg(any(test, feature = "test-support"))]
    pub const fn from_parts(
        kernel_base: u64,
        kernel_pa_capacity: u64,
        kernel_va_end: u64,
        vm_base: u64,
        vm_pa_capacity: u64,
        vm_va_end: u64,
    ) -> Self {
        Self {
            kernel_base,
            kernel_pa_capacity,
            kernel_va_end,
            vm_base,
            vm_pa_capacity,
            vm_va_end,
            _arch: core::marker::PhantomData,
        }
    }

    /// Full geometric validation. `plan_dm_policy` calls this before a layout
    /// can enter a policy, so injected or future-architecture layouts cannot
    /// bypass it.
    pub fn validate(&self) -> Result<(), DmLayoutError> {
        let base = PageSize::try_from_usize(A::Paging::PAGE_SIZE)
            .ok_or(DmLayoutError::InvalidLayout)?;
        if self.kernel_pa_capacity == 0 || self.vm_pa_capacity == 0 {
            return Err(DmLayoutError::InvalidLayout);
        }
        if self.kernel_base != <A::DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_BASE
            || self.kernel_pa_capacity != <A::DirectMap as DirectMapArch>::KERNEL_DIRECT_MAP_SIZE
            || self.kernel_base.checked_add(self.kernel_pa_capacity) != Some(self.kernel_va_end)
            || self.kernel_va_end > <A::DirectMap as DirectMapArch>::KERNEL_DM_VA_END_LIMIT
            || self.kernel_pa_capacity % base.bytes() != 0
            || A::Paging::validate_virtual_range(self.kernel_base, self.kernel_pa_capacity)
                .is_err()
        {
            return Err(DmLayoutError::InvalidLayout);
        }
        if self.vm_base != <A::DirectMap as DirectMapArch>::VM_DIRECT_MAP_BASE
            || self.vm_pa_capacity != <A::DirectMap as DirectMapArch>::VM_DIRECT_MAP_SIZE
            || self.vm_va_end != <A::DirectMap as DirectMapArch>::VM_HEAP_BASE
            || self.vm_base.checked_add(self.vm_pa_capacity) != Some(self.vm_va_end)
            || self.vm_pa_capacity % base.bytes() != 0
            || A::Paging::validate_virtual_range(self.vm_base, self.vm_pa_capacity).is_err()
        {
            return Err(DmLayoutError::InvalidLayout);
        }
        if self.kernel_va_end > self.vm_base && self.vm_va_end > self.kernel_base {
            return Err(DmLayoutError::InvalidLayout);
        }
        Ok(())
    }

    pub const fn kernel_base(&self) -> u64 {
        self.kernel_base
    }

    pub const fn kernel_pa_capacity(&self) -> u64 {
        self.kernel_pa_capacity
    }

    pub const fn kernel_va_end(&self) -> u64 {
        self.kernel_va_end
    }

    pub const fn vm_base(&self) -> u64 {
        self.vm_base
    }

    pub const fn vm_pa_capacity(&self) -> u64 {
        self.vm_pa_capacity
    }

    pub const fn vm_va_end(&self) -> u64 {
        self.vm_va_end
    }
}

/// Layout geometry error. Defined next to the type so the layout layer does
/// not depend on the planning layer's error enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmLayoutError {
    InvalidLayout,
}

impl core::fmt::Display for DmLayoutError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid direct-map layout")
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
#[derive(Default)]
pub struct X86_64DirectMap;

impl DirectMapArch for X86_64DirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_0000_8000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8080_0000_0000;
    /// PML4[257] through the last representable page (255 GiB - 4 KiB).
    const KERNEL_DIRECT_MAP_SIZE: u64 = 0x0000_7F7F_FFFF_F000;
    const KERNEL_DM_VA_END_LIMIT: u64 = 0xFFFF_FFFF_FFFF_F000;
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

#[cfg(target_arch = "x86_64")]
const _: () = {
    use crate::arch::direct_map::X86_64DirectMap as D;
    use crate::paging_ext::{HugePages, PageSize};
    use crate::x86_64::paging::X86_64Paging as P;
    let base = <D as DirectMapArch>::KERNEL_DIRECT_MAP_BASE;
    let size = <D as DirectMapArch>::KERNEL_DIRECT_MAP_SIZE;
    let preferred = <P as HugePages>::HUGE_PAGE_SIZE;
    let limit = <D as DirectMapArch>::KERNEL_DM_VA_END_LIMIT;
    assert!(preferred.bytes() > 0);
    assert!(base % preferred.bytes() == 0);
    assert!(size > 0);
    assert!(size % PageSize::KIB_4.bytes() == 0);
    assert!(match base.checked_add(size) {
        Some(end) => end <= limit,
        None => false,
    });
};

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
#[derive(Default)]
pub struct AArch64DirectMap;

impl DirectMapArch for AArch64DirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_1000_0000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8080_0000_0000;
    /// TTBR1 slot 257 through the last representable page.
    const KERNEL_DIRECT_MAP_SIZE: u64 = 0x0000_7F7F_FFFF_F000;
    const KERNEL_DM_VA_END_LIMIT: u64 = 0xFFFF_FFFF_FFFF_F000;
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

#[cfg(target_arch = "aarch64")]
const _: () = {
    use crate::arch::direct_map::AArch64DirectMap as D;
    use crate::arm64::paging::AArch64Paging as P;
    use crate::paging_ext::{HugePages, PageSize};
    let base = <D as DirectMapArch>::KERNEL_DIRECT_MAP_BASE;
    let size = <D as DirectMapArch>::KERNEL_DIRECT_MAP_SIZE;
    let preferred = <P as HugePages>::HUGE_PAGE_SIZE;
    let limit = <D as DirectMapArch>::KERNEL_DM_VA_END_LIMIT;
    assert!(preferred.bytes() > 0);
    assert!(base % preferred.bytes() == 0);
    assert!(size > 0);
    assert!(size % PageSize::KIB_4.bytes() == 0);
    assert!(match base.checked_add(size) {
        Some(end) => end <= limit,
        None => false,
    });
};

/// RISC-V 64 (Sv39) Direct Map address space layout.
///
/// Sv39 provides a 39-bit virtual address space: VA[38:0]. The kernel
/// image is linked at the Sv39 canonical high base `0xFFFF_FFC0_0000_0000`
/// (VPN[2] = 256, see `os/kernel/src/arch/riscv64/link.ld`); the kernel
/// direct map takes the next top-level slot, VPN[2] = 257
/// (`0xFFFF_FFC0_4000_0000`), so the window never shares a slot with the
/// image. The previous base `0xFFFF_FC00_0000_0000` was a non-canonical
/// address for Sv39 — its 39-bit payload is `0x0` (VPN[2] = 0), so it landed
/// in no legal top-level slot at all and the DM formula subtracted out
/// garbage. That is a different fault from the x86-64 image-L2-slot collision
/// documented in `02-higher-half-kernel.md` Appendix A. (Only the kernel
/// image base `0xFFFF_FFC0_0000_0000` sits at VPN[2] = 256.) The VM direct map is placed at
/// `0x0000_0010_0000_0000` (64GB offset in the user low half).
///
/// See `07-cross-space-init.md` §4.1 for the address-space layout table.
#[derive(Default)]
pub struct Riscv64DirectMap;

impl DirectMapArch for Riscv64DirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_0010_0000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_FFC0_4000_0000;
    /// VPN[2] = 257 through the Sv39 canonical top: 255 GiB - 4 KiB.
    const KERNEL_DIRECT_MAP_SIZE: u64 = 0x0000_003F_BFFF_F000;
    const KERNEL_DM_VA_END_LIMIT: u64 = 0xFFFF_FFFF_FFFF_F000;
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

#[cfg(target_arch = "riscv64")]
const _: () = {
    use crate::arch::direct_map::Riscv64DirectMap as D;
    use crate::paging_ext::{HugePages, PageSize};
    use crate::riscv64::paging::Riscv64Paging as P;
    let base = <D as DirectMapArch>::KERNEL_DIRECT_MAP_BASE;
    let size = <D as DirectMapArch>::KERNEL_DIRECT_MAP_SIZE;
    let preferred = <P as HugePages>::HUGE_PAGE_SIZE;
    let limit = <D as DirectMapArch>::KERNEL_DM_VA_END_LIMIT;
    assert!(preferred.bytes() > 0);
    assert!(base % preferred.bytes() == 0);
    assert!(size > 0);
    assert!(size % PageSize::KIB_4.bytes() == 0);
    assert!(match base.checked_add(size) {
        Some(end) => end <= limit,
        None => false,
    });
};

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
    const KERNEL_DIRECT_MAP_SIZE: u64 = 0x0000_7F7F_FFFF_F000;
    const KERNEL_DM_VA_END_LIMIT: u64 = 0xFFFF_FFFF_FFFF_F000;
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

#[cfg(all(test, feature = "runtime-window"))]
mod layout_tests {
    use super::*;
    use crate::arch::boot_arch::MockBootArch;

    #[test]
    fn architecture_layout_validates() {
        let layout = DirectMapLayout::<MockBootArch>::from_arch();
        assert_eq!(layout.validate(), Ok(()));
        assert_eq!(layout.kernel_base(), MockDirectMap::KERNEL_DIRECT_MAP_BASE);
        assert_eq!(
            layout.kernel_va_end(),
            MockDirectMap::KERNEL_DIRECT_MAP_BASE + MockDirectMap::KERNEL_DIRECT_MAP_SIZE
        );
    }

    #[test]
    fn layout_off_the_architecture_constants_is_rejected() {
        let good = DirectMapLayout::<MockBootArch>::from_arch();
        let shifted = DirectMapLayout::<MockBootArch>::from_parts(
            good.kernel_base() - PageSize::KIB_4.bytes(),
            good.kernel_pa_capacity(),
            good.kernel_va_end() - PageSize::KIB_4.bytes(),
            good.vm_base(),
            good.vm_pa_capacity(),
            good.vm_va_end(),
        );
        assert_eq!(shifted.validate(), Err(DmLayoutError::InvalidLayout));
    }

    #[test]
    fn zero_capacity_is_rejected_even_when_the_end_is_consistent() {
        let good = DirectMapLayout::<MockBootArch>::from_arch();
        let zero = DirectMapLayout::<MockBootArch>::from_parts(
            good.kernel_base(),
            0,
            good.kernel_base(),
            good.vm_base(),
            good.vm_pa_capacity(),
            good.vm_va_end(),
        );
        assert_eq!(zero.validate(), Err(DmLayoutError::InvalidLayout));
    }

    #[test]
    fn unaligned_capacity_and_broken_end_are_rejected() {
        let good = DirectMapLayout::<MockBootArch>::from_arch();
        let unaligned = DirectMapLayout::<MockBootArch>::from_parts(
            good.kernel_base(),
            good.kernel_pa_capacity() - 1,
            good.kernel_va_end() - 1,
            good.vm_base(),
            good.vm_pa_capacity(),
            good.vm_va_end(),
        );
        assert_eq!(unaligned.validate(), Err(DmLayoutError::InvalidLayout));

        let broken_end = DirectMapLayout::<MockBootArch>::from_parts(
            good.kernel_base(),
            good.kernel_pa_capacity(),
            good.kernel_va_end() - PageSize::KIB_4.bytes(),
            good.vm_base(),
            good.vm_pa_capacity(),
            good.vm_va_end(),
        );
        assert_eq!(broken_end.validate(), Err(DmLayoutError::InvalidLayout));
    }
}
