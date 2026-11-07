//! BootShim trait — firmware-agnostic boot preparation interface.
//!
//! Implemented by firmware-specific modules (UEFI aux form, U-Boot
//! production leg). The kernel does **not** depend on this trait — its
//! entry surface is `arch_boot` / `enter_kernel_image_v1` (doc 01 §3.2).

use minix_types::PhysBytes;
use crate::kernel_info::KernelInfo;

/// Firmware-agnostic result of boot preparation.
///
/// Contains everything the kernel needs to set up paging and jump to kmain:
/// - `kernel_info`: memory map, kernel location, boot modules
/// - `root_page`: physical address of the root page table page
/// - `bump_base` / `bump_end`: bump region for boot-stage page table allocation
///
/// This struct is produced by a `BootShim` implementation and consumed **in
/// the same address space**. Since T2 landed, both UEFI legs are cross-image
/// producers: the struct is built in the boot-shim process, and the
/// cross-image authority is the wire protocol `BootHandoffV1`
/// ([`crate::handoff_v1`]), whose envelope carries the same semantic fields in
/// fixed-width wire types.
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
/// Implemented by firmware-specific modules (UEFI, OpenSBI, ...). The kernel
/// does **not** depend on this trait (zero references in `os/kernel`) — its
/// entry surface is `arch_boot` / `enter_kernel_image_v1`. Production load:
/// only the U-Boot leg (`RiscvUbootBootShim`, called from `uboot_main.rs`);
/// the UEFI trait impl is an auxiliary form with no production caller.
///
/// What the trait actually buys (doc 01 §3.2, with evidence strength):
/// 1. **Uniform entry shape** — U-Boot leg & test forms only.
/// 2. **Testability** — `MockLoader` feeds file bytes in loader tests.
/// 3. **cfg-gate locality** — 8 gate sites, none in business logic.
/// 4. **"New firmware, kernel untouched"** — design promise, no automated
///    counterexample yet. Type safety/encapsulation come from the feature
///    gates, not from this trait.
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
