//! KernelInfo — shared struct passed from boot-loader to kernel.
//!
//! Corresponds to Minix3's `kinfo_t` (minix/param.h, pre_init.c).
//! Stripped of multiboot-specific fields — UEFI provides memory map directly.

use crate::types::{VirBytes, PhysBytes};

pub struct KernelInfo {
    /// Free physical memory regions (kernel + modules already excluded).
    /// C: kinfo.memmap[] — pg_utils.c add_memmap/cut_memmap
    pub memmap: &'static [MemoryRegion],

    /// Kernel virtual base address.
    /// C: kinfo.vir_kern_start = &_kern_vir_base — pre_init.c:114
    pub kern_virt_base: VirBytes,

    /// Kernel physical base address.
    /// C: kern_phys_start = &_kern_phys_base — pg_utils.c linker symbol
    pub kern_phys_base: PhysBytes,

    /// Kernel total size in bytes.
    /// C: kern_kernlen = &_kern_size — pg_utils.c linker symbol
    pub kern_size: usize,

    /// First free page table root-level index after identity+kernel maps.
    /// C: kinfo.freepde_start = pg_mapkernel() — pre_init.c:271
    /// x86-64: PML4 index, aarch64: TTBR1 L0 index, riscv64: Sv39 VPN[2]
    pub free_upper_idx: usize,

    /// User-space stack top.
    /// C: kinfo.user_sp = USR_STACKTOP — pre_init.c:143
    pub user_sp: VirBytes,

    /// Boot process images (PM, VM, VFS, RS etc.).
    /// C: kinfo.module_list[] — pre_init.c memcpy from GRUB
    pub boot_modules: &'static [BootModule],
}

pub struct MemoryRegion {
    pub base: PhysBytes,
    pub len: usize,
}

pub struct BootModule {
    pub name: &'static str,
    pub start: PhysBytes,
    pub len: usize,
}
