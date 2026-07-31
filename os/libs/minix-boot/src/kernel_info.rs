//! KernelInfo — shared struct passed from boot-loader to kernel.
//!
//! Corresponds to Minix3's `kinfo_t` (minix/param.h, pre_init.c).
//! Stripped of multiboot-specific fields — UEFI provides memory map directly.

use minix_types::{VirBytes, PhysBytes};

use crate::platform::PlatformDescSource;

// ── Data structures ──

#[derive(Clone, Copy)]
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
    /// tss_init(0, &k_boot_stktop) in prot_init() — arch/i386/protect.c:338
    /// x86-64: TSS.sp0, aarch64: SP_EL1, riscv64: sscratch
    pub kern_stack_top: VirBytes,

    /// System call entry point (virtual address).
    /// System call handler entry virtual address (= kern_virt_base + offset).
    /// Kernel metadata recorded for all architectures; only x86-64 uses it
    /// to configure LSTAR MSR. aarch64/riscv64 determine the entry at
    /// compile time (exception/trap vector), so this field is for reference only.
    /// C: STAR MSR (AMD_MSR_STAR, 32-bit AMD SYSCALL) — set by tss_init() SYSCALL
    /// MSR setup — arch/i386/protect.c:189-205. Note: C is 32-bit, uses STAR;
    /// Rust x86-64 long mode uses LSTAR (different MSR, same semantic role).
    pub syscall_entry: VirBytes,

    /// Boot process images (PM, VM, VFS, RS etc.).
    /// C: kinfo.module_list[] — pre_init.c memcpy from GRUB
    pub boot_modules: &'static [BootModule],

    /// Physical address of the bootstrap (unpaged-kernel) memory region.
    /// C: kinfo.bootstrap_start = &_kern_unpaged_start — pre_init.c:114
    /// In Minix3 C this is reclaimed via add_memmap() after boot completes.
    /// In the Rust port the kernel runs in higher-half from the first
    /// instruction — no separate unpaged section exists. Callers therefore
    /// pass `(PhysBytes(0), 0)` and the kernel's `if bootstrap_len > 0`
    /// guard skips the reclaim entirely. See 01-boot-shim-bootstrap.md
    /// §X (TODO-01-1 fix) for the rationale.
    pub bootstrap_start: PhysBytes,

    /// Length of the bootstrap (unpaged-kernel) memory region.
    /// C: kinfo.bootstrap_len = &_kern_unpaged_end - &_kern_unpaged_start — pre_init.c:115-116
    /// In the Rust port: must be `0` (see bootstrap_start for rationale).
    /// A non-zero value triggers add_memmap() in kmain Phase F and would
    /// reclaim physical memory; an incorrect range would corrupt firmware
    /// regions such as OpenSBI/DTB/U-Boot.
    pub bootstrap_len: u64,

    /// Platform descriptor sources — opaque handles carrying firmware table
    /// pointers (DTB, RSDP, or both). Ordered by boot-shim's preference;
    /// the kernel takes the first source that parses successfully.
    ///
    /// Empty slice means boot-shim did not provide any — kernel falls back
    /// to `QemuVirtDesc` (dev) or panics (release). See `plat-design.md` §4.
    ///
    /// # Cross-binary safety (TODO-01-2 fix, 2026-07-16)
    ///
    /// Each `PlatformDescSource` is pure data `(u32 kind, u64 phys_addr)` —
    /// safe to pass from boot-shim to kernel even when they are separate
    /// ELF binaries (TODO-02-3). No function pointers cross the boundary.
    ///
    /// # DTB + RSDP coexistence
    ///
    /// Real-world ARM64 servers (SBBR) may provide both DTB and ACPI. The
    /// list supports this: boot-shim passes `[dtb_source, rsdp_source]` (or
    /// `[rsdp_source, dtb_source]` if ACPI preferred). The kernel tries
    /// each in order, using the first that parses successfully — matching
    /// Linux's `acpi=on/off/force` model.
    pub platform_sources: &'static [PlatformDescSource],
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
