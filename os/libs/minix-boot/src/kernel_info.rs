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

    /// Boot monitor parameter buffer — `key=value\0` pairs passed by the
    /// boot loader (multiboot command line, UEFI load options, or SBI
    /// boot hart argument). Returned to user space by `GET_INFO` with
    /// `info_type = GET_MONPARAMS` (`do_getinfo.c:143-146`).
    ///
    /// C: `char kinfo.param_buf[MULTIBOOT_PARAM_BUF_SIZE]` (param.h:28,
    /// `MULTIBOOT_PARAM_BUF_SIZE = 1024` — earm/multiboot.h:240).
    ///
    /// In Minix3 the buffer is populated by `mb_set_param()` in
    /// `arch/i386/pre_init.c:144` / `arch/earm/pre_init.c:269`, parsing the
    /// multiboot info structure's `cmdline` field into `key=value` pairs
    /// (e.g. `bootopts=...`, `memory=...`, `ht=...`).
    ///
    /// The Rust boot-shim currently passes an empty slice (`&[]`) — UEFI
    /// load options are not yet parsed into the `key=value` format. Kernel
    /// callers (`do_getinfo` MonParams) treat an empty slice as "no boot
    /// parameters" and copy zero bytes to the caller (matching C's behavior
    /// when `param_buf[0] == '\0'`).
    ///
    /// # Validation
    ///
    /// `KernelInfo::validate` asserts `param_buf.len() <= 1024` to match
    /// C's `MULTIBOOT_PARAM_BUF_SIZE`. Boot-shims that pass larger buffers
    /// violate the contract and trigger a fail-fast panic.
    pub param_buf: &'static [u8],
}

impl KernelInfo {
    /// Validate kernel-info invariants. Called by `kmain` at entry.
    ///
    /// R-07 (2026-08-12): `KernelInfo` fields remain `pub` for cross-crate
    /// access (boot-shim constructs, kernel reads, tests assert), but this
    /// method enforces runtime invariants that the type system cannot:
    ///
    /// 1. `bootstrap_len == 0` — Rust port runs higher-half from the first
    ///    instruction; non-zero would trigger `add_memmap()` and reclaim
    ///    physical memory that may overlap firmware regions (OpenSBI/DTB).
    /// 2. `kern_size > 0` — boot-shim must have loaded a non-empty kernel.
    /// 3. `kern_stack_top % 16 == 0` — stack alignment (AAPCS64 / SysV ABI).
    /// 4. `!memmap.is_empty()` — at least one free memory region required.
    /// 5. `!boot_modules.is_empty()` — at least PM/VM/VFS/RS required.
    ///
    /// # Panics
    ///
    /// Panics with a descriptive message if any invariant is violated.
    /// This is a fail-fast contract: boot-shim bugs that produce invalid
    /// `KernelInfo` are unrecoverable.
    pub fn validate(&self) {
        // 1. bootstrap_len must be 0 (Rust port has no unpaged section).
        //    C: kinfo.bootstrap_len = &_kern_unpaged_end - &_kern_unparsed_start
        //    Rust: always 0 (see bootstrap_start field doc).
        assert_eq!(
            self.bootstrap_len, 0,
            "KernelInfo: bootstrap_len must be 0 (Rust port has no unpaged section), got {}",
            self.bootstrap_len
        );

        // 2. Kernel size must be non-zero.
        assert!(
            self.kern_size > 0,
            "KernelInfo: kern_size must be non-zero (boot-shim did not load kernel?)"
        );

        // 3. Stack pointer must be 16-byte aligned (AAPCS64 / SysV ABI).
        assert!(
            self.kern_stack_top.0.is_multiple_of(16),
            "KernelInfo: kern_stack_top (0x{:x}) must be 16-byte aligned",
            self.kern_stack_top.0
        );

        // 4. At least one free memory region.
        assert!(
            !self.memmap.is_empty(),
            "KernelInfo: memmap must not be empty (no free memory?)"
        );

        // 5. At least one boot module (PM/VM/VFS/RS).
        assert!(
            !self.boot_modules.is_empty(),
            "KernelInfo: boot_modules must not be empty (no init servers?)"
        );

        // 6. Boot parameter buffer must fit in MULTIBOOT_PARAM_BUF_SIZE.
        //    C: `char kinfo.param_buf[MULTIBOOT_PARAM_BUF_SIZE]` where
        //    MULTIBOOT_PARAM_BUF_SIZE = 1024 (earm/multiboot.h:240).
        //    The Rust variant uses a borrowed slice (`&'static [u8]`) so
        //    we enforce the same upper bound at validation time.
        const MULTIBOOT_PARAM_BUF_SIZE: usize = 1024;
        assert!(
            self.param_buf.len() <= MULTIBOOT_PARAM_BUF_SIZE,
            "KernelInfo: param_buf length {} exceeds MULTIBOOT_PARAM_BUF_SIZE ({})",
            self.param_buf.len(),
            MULTIBOOT_PARAM_BUF_SIZE
        );
    }

    // ── Getter methods (R-07: preferred API over direct field access) ──
    //
    // These getters are the recommended API. Direct field access remains
    // `pub` for backward compatibility with boot-shim construction and
    // test assertions, but new code should prefer these methods.

    /// Free physical memory regions (kernel + modules already excluded).
    /// C: `kinfo.memmap[]` — pg_utils.c add_memmap/cut_memmap
    #[inline]
    pub fn memmap(&self) -> &'static [MemoryRegion] { self.memmap }

    /// Kernel virtual base address.
    /// C: `kinfo.vir_kern_start = &_kern_vir_base` — pre_init.c:113
    #[inline]
    pub fn kern_virt_base(&self) -> VirBytes { self.kern_virt_base }

    /// Kernel physical base address.
    /// C: `kern_phys_start = &_kern_phys_base` — pg_utils.c linker symbol
    #[inline]
    pub fn kern_phys_base(&self) -> PhysBytes { self.kern_phys_base }

    /// Kernel total size in bytes.
    /// C: `kern_kernlen = &_kern_size` — pg_utils.c linker symbol
    #[inline]
    pub fn kern_size(&self) -> u64 { self.kern_size }

    /// First free page table root-level index after identity+kernel maps.
    /// C: `kinfo.freepde_start = pg_mapkernel()` — pre_init.c:233
    /// Returns `None` if boot-shim has not computed this value.
    #[inline]
    pub fn free_upper_idx(&self) -> Option<usize> { self.free_upper_idx }

    /// User-space stack top.
    /// C: `kinfo.user_sp = USR_STACKTOP` — pre_init.c:156
    #[inline]
    pub fn user_sp(&self) -> VirBytes { self.user_sp }

    /// Kernel initial stack top (virtual address).
    /// C: `k_boot_stktop` / `k_initial_stktop` — linker symbol
    #[inline]
    pub fn kern_stack_top(&self) -> VirBytes { self.kern_stack_top }

    /// System call entry point (virtual address).
    /// x86-64: configures LSTAR MSR; aarch64/riscv64: reference only.
    #[inline]
    pub fn syscall_entry(&self) -> VirBytes { self.syscall_entry }

    /// Boot process images (PM, VM, VFS, RS etc.).
    /// C: `kinfo.module_list[]` — pre_init.c memcpy from GRUB
    #[inline]
    pub fn boot_modules(&self) -> &'static [BootModule] { self.boot_modules }

    /// Physical address of the bootstrap (unpaged-kernel) memory region.
    /// Rust port: always `PhysBytes(0)` (see `bootstrap_start` field doc).
    #[inline]
    pub fn bootstrap_start(&self) -> PhysBytes { self.bootstrap_start }

    /// Length of the bootstrap (unpaged-kernel) memory region.
    /// Rust port: always `0` (validated by [`Self::validate`]).
    #[inline]
    pub fn bootstrap_len(&self) -> u64 { self.bootstrap_len }

    /// Platform descriptor sources — opaque handles carrying firmware
    /// table pointers (DTB, RSDP, or both).
    #[inline]
    pub fn platform_sources(&self) -> &'static [PlatformDescSource] { self.platform_sources }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// R-07: Helper to build a valid KernelInfo for testing.
    fn make_valid_info() -> KernelInfo {
        KernelInfo {
            memmap: &[MemoryRegion { base: PhysBytes(0x100000), len: 0x1000000 }],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200000),
            kern_size: 0x200000,
            free_upper_idx: Some(256),
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[BootModule {
                name: "vm",
                start: PhysBytes(0x400000),
                len: 0x100000,
            }],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        }
    }

    #[test]
    fn test_validate_accepts_valid_info() {
        // R-07: A valid KernelInfo should pass validate() without panic.
        make_valid_info().validate();
    }

    #[test]
    #[should_panic(expected = "bootstrap_len must be 0")]
    fn test_validate_rejects_nonzero_bootstrap_len() {
        let mut info = make_valid_info();
        info.bootstrap_len = 0x1000;
        info.validate();
    }

    #[test]
    #[should_panic(expected = "kern_size must be non-zero")]
    fn test_validate_rejects_zero_kern_size() {
        let mut info = make_valid_info();
        info.kern_size = 0;
        info.validate();
    }

    #[test]
    #[should_panic(expected = "must be 16-byte aligned")]
    fn test_validate_rejects_misaligned_stack() {
        let mut info = make_valid_info();
        info.kern_stack_top = VirBytes(0xFFFF_8000_0040_0001); // odd address
        info.validate();
    }

    #[test]
    #[should_panic(expected = "memmap must not be empty")]
    fn test_validate_rejects_empty_memmap() {
        let mut info = make_valid_info();
        info.memmap = &[];
        info.validate();
    }

    #[test]
    #[should_panic(expected = "boot_modules must not be empty")]
    fn test_validate_rejects_empty_boot_modules() {
        let mut info = make_valid_info();
        info.boot_modules = &[];
        info.validate();
    }

    #[test]
    #[should_panic(expected = "exceeds MULTIBOOT_PARAM_BUF_SIZE")]
    fn test_validate_rejects_oversized_param_buf() {
        // P9-1: param_buf length must not exceed MULTIBOOT_PARAM_BUF_SIZE (1024).
        // We cannot construct a `&'static [u8]` of length 1025 at compile time
        // without a `const` slice; instead we use a static buffer and slice it.
        static OVERFLOW_BUF: [u8; 1025] = [0; 1025];
        let mut info = make_valid_info();
        info.param_buf = &OVERFLOW_BUF[..];
        info.validate();
    }

    #[test]
    fn test_validate_accepts_max_param_buf() {
        // P9-1: param_buf length == 1024 (exactly MULTIBOOT_PARAM_BUF_SIZE) is OK.
        static MAX_BUF: [u8; 1024] = [0; 1024];
        let mut info = make_valid_info();
        info.param_buf = &MAX_BUF[..];
        info.validate();
    }

    #[test]
    fn test_getters_return_field_values() {
        // R-07: Getter methods should return the same values as direct field access.
        // Slice types lack PartialEq, so compare lengths and first elements.
        let info = make_valid_info();
        // Scalar fields: direct equality.
        assert_eq!(info.kern_virt_base(), info.kern_virt_base);
        assert_eq!(info.kern_phys_base(), info.kern_phys_base);
        assert_eq!(info.kern_size(), info.kern_size);
        assert_eq!(info.free_upper_idx(), info.free_upper_idx);
        assert_eq!(info.user_sp(), info.user_sp);
        assert_eq!(info.kern_stack_top(), info.kern_stack_top);
        assert_eq!(info.syscall_entry(), info.syscall_entry);
        assert_eq!(info.bootstrap_start(), info.bootstrap_start);
        assert_eq!(info.bootstrap_len(), info.bootstrap_len);
        // Slice fields: compare length and first element address.
        assert_eq!(info.memmap().len(), info.memmap.len());
        assert_eq!(info.memmap().as_ptr(), info.memmap.as_ptr());
        assert_eq!(info.boot_modules().len(), info.boot_modules.len());
        assert_eq!(info.boot_modules().as_ptr(), info.boot_modules.as_ptr());
        assert_eq!(info.platform_sources().len(), info.platform_sources.len());
        assert_eq!(info.platform_sources().as_ptr(), info.platform_sources.as_ptr());
    }
}
