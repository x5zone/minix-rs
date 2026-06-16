//! KernelInfo — shared struct passed from boot-loader to kernel.
//!
//! Corresponds to Minix3's `kinfo_t` (minix/param.h, pre_init.c).
//! Stripped of multiboot-specific fields — UEFI provides memory map directly.

use minix_types::{VirBytes, PhysBytes};

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
    ///
    /// `None` means the boot-shim has not yet computed this value.
    /// In Minix3 C, `pg_mapkernel()` returns the index; the Rust boot-shim
    /// currently does not compute it, so it is set to `None` and the kernel
    /// must validate it before use.
    pub free_upper_idx: Option<usize>,

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

    /// Physical address of the bootstrap (boot-shim) memory region.
    /// C: kinfo.bootstrap_start = &_kern_unpaged_start — pre_init.c:114
    /// This memory is reclaimed via add_memmap() after boot completes.
    pub bootstrap_start: PhysBytes,

    /// Length of the bootstrap (boot-shim) memory region.
    /// C: kinfo.bootstrap_len = &_kern_unpaged_end - &_kern_unpaged_start — pre_init.c:115-116
    /// Added to free memory pool by add_memmap() in kmain Phase F.
    pub bootstrap_len: u64,
}

impl KernelInfo {
    /// Returns the first free page table root-level index after
    /// identity+kernel maps, if it has been computed by the boot-shim.
    ///
    /// C: `kinfo.freepde_start` — set by `pg_mapkernel()` return value.
    /// Returns `None` if the boot-shim has not yet computed this value.
    pub fn free_upper_idx(&self) -> Option<usize> {
        self.free_upper_idx
    }
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
