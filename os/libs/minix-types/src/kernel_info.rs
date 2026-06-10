//! KernelInfo — shared struct passed from boot-loader to kernel.
//!
//! Corresponds to Minix3's `kinfo_t` (minix/param.h, pre_init.c).
//! Stripped of multiboot-specific fields — UEFI provides memory map directly.
//!
//! Also defines the `BootShim` trait — the firmware-agnostic interface
//! that boot-shim implementations (UEFI, OpenSBI, ...) must satisfy.
//! The kernel only depends on this trait, not on any concrete boot implementation.

use crate::types::{VirBytes, PhysBytes};

// ── Data structures ──

pub struct KernelInfo {
    /// Free physical memory regions (kernel + modules already excluded).
    /// C: kinfo.memmap[] — pg_utils.c add_memmap/cut_memmap
    pub memmap: &'static [MemoryRegion],

    /// Kernel virtual base address.
    /// C: kinfo.vir_kern_start = &_kern_vir_base — pre_init.c:113
    pub kern_virt_base: VirBytes,

    /// Kernel physical base address.
    /// C: kern_phys_start = &_kern_phys_base — pg_utils.c linker symbol
    pub kern_phys_base: PhysBytes,

    /// Kernel total size in bytes.
    /// C: kern_kernlen = &_kern_size — pg_utils.c linker symbol
    /// Uses u64 instead of usize to avoid truncation on 32-bit targets.
    pub kern_size: u64,

    /// First free page table root-level index after identity+kernel maps.
    /// C: kinfo.freepde_start = pg_mapkernel() — pre_init.c:233
    /// x86-64: PML4 index, aarch64: TTBR1 L0 index, riscv64: Sv39 VPN[2]
    pub free_upper_idx: usize,

    /// User-space stack top.
    /// C: kinfo.user_sp = USR_STACKTOP — pre_init.c:156
    pub user_sp: VirBytes,

    /// Kernel initial stack top (virtual address).
    /// C: k_boot_stktop / k_initial_stktop — linker symbol, used by
    /// tss_init(0, &k_boot_stktop) in prot_init() — protect.c:338
    /// x86-64: TSS.sp0, aarch64: SP_EL1, riscv64: sscratch
    pub kern_stack_top: VirBytes,

    /// System call entry point (virtual address).
    /// System call handler entry virtual address (= kern_virt_base + offset).
    /// Kernel metadata recorded for all architectures; only x86-64 uses it
    /// to configure LSTAR MSR. aarch64/riscv64 determine the entry at
    /// compile time (exception/trap vector), so this field is for reference only.
    /// C: LSTAR MSR — set by tss_init() SYSCALL MSR setup — protect.c:189-205
    pub syscall_entry: VirBytes,

    /// Boot process images (PM, VM, VFS, RS etc.).
    /// C: kinfo.module_list[] — pre_init.c memcpy from GRUB
    pub boot_modules: &'static [BootModule],
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base: PhysBytes,
    pub len: usize,
}

pub struct BootModule {
    pub name: &'static str,
    pub start: PhysBytes,
    pub len: usize,
}

// ── BootShim trait ──

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
