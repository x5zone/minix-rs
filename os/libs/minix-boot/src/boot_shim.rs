//! BootShim trait — firmware-agnostic boot preparation interface.
//!
//! Implemented by firmware-specific modules (UEFI, OpenSBI, ...).
//! The kernel only depends on this trait — it does not know whether
//! the boot info came from UEFI, OpenSBI, or any other firmware.

use minix_types::PhysBytes;
use crate::kernel_info::KernelInfo;

/// Firmware-agnostic result of boot preparation.
///
/// Contains everything the kernel needs to set up paging and jump to kmain:
/// - `kernel_info`: memory map, kernel location, boot modules
/// - `root_page`: physical address of the root page table page
/// - `bump_base` / `bump_end`: bump region for boot-stage page table allocation
///
/// This struct is produced by any `BootShim` implementation and consumed
/// by the kernel's `arch_boot_impl`. It is the **only** data crossing
/// the boot-shim → kernel boundary.
pub struct BootPrepareResult {
    pub kernel_info: KernelInfo,
    pub root_page: PhysBytes,
    /// Bump region base address (physical).
    pub bump_base: u64,
    /// Bump region end address (physical, exclusive).
    pub bump_end: u64,
}

/// Firmware-agnostic boot preparation trait.
///
/// Implemented by firmware-specific modules (UEFI, OpenSBI, ...).
/// The kernel only depends on this trait — it does not know whether
/// the boot info came from UEFI, OpenSBI, or any other firmware.
///
/// Feature gates in `Cargo.toml` select which implementation gets compiled;
/// code uses the trait uniformly without `#[cfg]` conditionals.
///
/// # Why a trait instead of free functions?
///
/// 1. **Type safety**: The kernel receives a `&dyn BootShim` or generic `B: BootShim`,
///    not a bag of loose functions. The compiler enforces that all required steps
///    (memmap, root page, bump region, exit services) are implemented together.
/// 2. **Encapsulation**: Each firmware's implementation is a cohesive unit —
///    you can't accidentally mix `uefi_helpers::build_memmap` with
///    `opensbi_helpers::alloc_root_page`.
/// 3. **Testability**: Mock implementations can be injected for kernel unit tests.
pub trait BootShim {
    /// Prepare boot: discover memory, load kernel and boot modules,
    /// allocate pages, build KernelInfo, exit firmware services.
    ///
    /// This is called exactly once, before the kernel sets up paging.
    /// After this call, firmware boot services are no longer available.
    ///
    /// The kernel's physical/virtual base and size are determined internally
    /// by the implementation (e.g., by parsing the kernel ELF's PT_LOAD
    /// segments). The caller only specifies how many bump pages to allocate
    /// for boot-stage page table construction.
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult;
}
