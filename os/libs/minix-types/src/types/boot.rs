//! Boot image types.
//!
//! Types for boot-time process information, shared across kernel and services.
//!
//! Corresponds to Minix3's `struct boot_image` in `minix/include/minix/type.h`.

use crate::Endpoint;

pub const PROC_NAME_LEN: usize = 16;
pub const NR_BOOT_PROCS: usize = 17; // C: param.h — NR_TASKS(5) + LAST_SPECIAL_PROC_NR(11) + 1

/// Fixed virtual address where the kernel maps the VM boot handoff page
/// into VM's initial address space.
///
/// The value sits above the boot identity mapping (`[0, 4 GiB)`, which is
/// built from huge pages that a 4 KiB `map()` cannot split), below the
/// kernel high mapping, and below every architecture's user-VA limit
/// (Sv39 caps user VA at 2^38). VM reads the handoff at this address
/// during startup; the kernel writes it once before scheduling VM.
pub const VM_BOOT_HANDOFF_VA: u64 = 0x1_0000_0000;

/// `VmBootHandoff::magic` — marks the page as a VM boot handoff.
pub const VM_BOOT_HANDOFF_MAGIC: u32 = 0x564D_4248; // "VMBH"

/// `VmBootHandoff::version` — handoff layout version.
///
/// Version history:
/// - 1: `root_paddr` only (A1 identity hand-off).
/// - 2: full boot contract — free regions (A2 classification), the
///   LiveBootstrap deduction record, reserved modules, the boot image
///   table and the kernel footprint.
/// - 3: kernel text/data span for per-process kernel mappings (V11/E3).
/// - 4: `user_sp` — the initial user stack top, C `kinfo.user_sp`
///   (pre_init.c:156). VM builds every boot process's initial stack
///   frame downward from it (E-BOOTFRAME; C exec_bootproc reads the
///   same value out of `kernel_boot_info`, main.c:372).
/// - 5: `kern_dm_vbase` / `kern_dm_pages` — the kernel Direct Map
///   window's real VA base and PA coverage (fix26). Version ≤ 4
///   handoffs carried hardcoded sentinels here (wrong base + 4-page
///   window): a process page table built from them left nearly the
///   whole Direct Map not-present, so kernel physical-memory access
///   after a CR3 switch to such a table was broken (the sentinel-era
///   tables are unusable either way — see v6 for what actually killed
///   the first switch). The kernel now computes both from the same
///   candidate union `establish_boot_dm` maps
///   (`dm_coverage::kernel_dm_pa_end`).
/// - 6: `kern_ident` / `kern_ident_count` — the kernel runtime
///   identity windows (fix27). NK4-A forensics (int_fix26f.log)
///   established the real first-switch death cause: the kernel is
///   statically linked into the UEFI boot-shim PE image and EXECUTES
///   from the firmware-relocated low addresses (text ~0x1dac_xxxx,
///   GDT/IDT/TSS ~0x1db9_xxxx after `mov cr3` succeeded, the very
///   next instruction fetch at IP == CR2 hit a not-present page →
///   #PF → #DF → triple fault). The higher-half standalone
///   kernel.elf copy is loaded but never executed, so the v3+
///   kern_* image span alone leaves the live code and tables
///   unmapped in every VM-built page table. v6 ships the occupied
///   (non-conventional) physical regions snapshotted from the UEFI
///   memory map so `map_kernel` replays them identity (VA = PA,
///   supervisor-only) into every process page table.
pub const VM_BOOT_HANDOFF_VERSION: u32 = 6;

/// Maximum kernel identity-window entries the handoff page carries
/// (fix27, version 6). The UEFI memory map's non-conventional RAM
/// typically occupies a few dozen descriptors; the kernel merges
/// adjacent ranges before publishing and fails fast on overflow.
pub const VM_BOOT_HANDOFF_MAX_IDENT: usize = 64;

/// Maximum free-region entries the handoff page carries.
///
/// The kernel asserts the post-classification region count fits (fail-fast)
/// rather than silently truncating — a truncated region would silently
/// shrink VM's allocatable memory.
pub const VM_BOOT_HANDOFF_MAX_REGIONS: usize = 64;

/// Maximum LiveBootstrap deduction-record entries the handoff page carries.
///
/// Upper bound of the record: kernel image + self root + boot bump region +
/// per-pool-region used-frame suffixes (8) + reserved boot modules (11).
pub const VM_BOOT_HANDOFF_MAX_DEDUCTED: usize = 32;

/// Maximum reserved-module entries the handoff page carries.
///
/// C: `NR_BOOT_MODULES` (com.h — user-space boot modules, DS..INIT = 12;
/// the VM entry itself is reclaimed and therefore not carried).
pub const VM_BOOT_HANDOFF_MAX_MODULES: usize = 12;

/// One physical memory range in the handoff page (plain data — the handoff
/// crosses the kernel→VM binary boundary, so no fat pointers or `usize`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffMemRegion {
    /// Page-aligned physical start.
    pub base: u64,
    /// Length in bytes (page-aligned).
    pub size: u64,
}

impl HandoffMemRegion {
    pub const ZERO: Self = Self { base: 0, size: 0 };
}

/// One reserved boot module blob in the handoff page.
///
/// C: `struct multiboot_module_t` — a blob the kernel still reserves at VM
/// start (its owner loads the ELF later and frees the blob).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffModule {
    pub start_addr: u64,
    pub len: u64,
}

impl HandoffModule {
    pub const ZERO: Self = Self {
        start_addr: 0,
        len: 0,
    };
}

/// Top of the initial user stack, per architecture.
///
/// C: `USR_STACKTOP` = `USR_DATATOP` (`minix3/minix/kernel/const.h:32-43`;
/// i386 `0xF0000000`) is the kernel's stack-top constant; it reaches
/// userland as `kinfo.user_sp` (`minix3/minix/kernel/arch/i386/pre_init.c:156`)
/// and drives both the boot processes' initial stacks
/// (`minix3/minix/servers/vm/main.c:391-411`; kernel leg
/// `minix3/minix/kernel/arch/i386/protect.c:408`) and the exec path
/// (`minix_get_user_sp`, `minix3/minix/lib/libc/sys/kernel_utils.c:39-62`).
/// The rewrite's LP64 adaptation pins a page-aligned top inside each
/// architecture's canonical user range:
///
/// - x86_64 / aarch64 (48-bit user VA): `0x7fff_ffff_f000` — the classic
///   LP64 user top, unchanged since the first boot legs.
/// - riscv64 (Sv39, 38-bit user VA): `0x3f_ffff_f000` — one page below the
///   Sv39 user top (`0x3f_ffff_ffff`). A 48-bit-style top is NOT canonical
///   under Sv39 (bits 63:39 must equal bit 38), so the CPU takes a store
///   page fault on the very first user stack access no matter what the
///   page tables contain; in-guest software page-table walks do not check
///   canonicality, so nothing on the guest side notices. History and full
///   evidence: `notes/rewrite/fork-syscall-rewrite/NK4C-BUG-RISCV64-TRANSIENT-PTE.md`.
///
/// This is the single authority: the boot shims publish it as
/// `KernelInfo::user_sp` and the exec path uses it as the boot-contract
/// fallback while the kerninfo read leg (NS5-B) is unwired. The
/// `cfg!`-form keeps this documentation visible on every target (a
/// `#[cfg]`-split pair would hide it from `cargo doc` on the other arch).
pub const USER_STACK_TOP: u64 = if cfg!(target_arch = "riscv64") {
    0x0000_003f_ffff_f000
} else {
    0x7fff_ffff_f000
};

/// One past the last valid user virtual address — the user-half limit.
///
/// Derivation per architecture (T13 地址常量审计 R1：
/// `notes/rewrite/fork-syscall-rewrite/ADDRESS-CONSTANT-AUDIT.md` §1/§3):
///
/// - x86_64 (four-level paging, LA57 absent): the user half is
///   `[0, 2^47)`; `2^47` is the PML4[256] kernel-half base. A canonical
///   address at or above `2^47` is kernel space.
/// - aarch64 (programmable boundary): the TCR_EL1 programmed in
///   `os/arch/src/arm64/paging.rs` uses `T0SZ=16`, so the user half is
///   `[0, 2^48)`; TTBR1 maps `[2^48, 2^64)`. That file cross-asserts this
///   constant against the same shift (`TCR_T0SZ`), so the two cannot drift.
/// - riscv64 (Sv39): the user half is `[0, 2^38)`; bit 38 is the first
///   VA bit that must equal bit 63 (canonicality — QEMU
///   `target/riscv/cpu_helper.c` masked_msbs gate). The §续-338 defect
///   class was exactly a user address above this limit.
///
/// This is the single authority for the user-half bound: the kernel-side
/// user-copy checks, the VM mmap window top, and every new per-arch
/// address decision derive from (or assert against) this constant. The
/// `cfg!`-form keeps this documentation visible on every target.
pub const USER_VA_LIMIT: u64 = if cfg!(target_arch = "riscv64") {
    1u64 << 38
} else if cfg!(target_arch = "aarch64") {
    1u64 << 48
} else {
    1u64 << 47
};

// Compile-time invariants: a wrong value here is the §续-338 defect class
// (an untranslatable user address), so pin it at build time on every target.
const _: () = assert!(USER_STACK_TOP.is_multiple_of(4096), "USER_STACK_TOP must be page-aligned");
const _: () = assert!(USER_STACK_TOP < USER_VA_LIMIT, "USER_STACK_TOP must stay in the user half");
#[cfg(target_arch = "riscv64")]
const _: () = assert!(
    USER_STACK_TOP < (1u64 << 38),
    "USER_STACK_TOP must be Sv39-canonical for riscv64 (below 2^38, the user half)"
);
#[cfg(target_arch = "riscv64")]
const _: () = assert!(
    USER_VA_LIMIT == (1u64 << 38),
    "USER_VA_LIMIT must be exactly the Sv39 user-half bound (2^38)"
);
#[cfg(target_arch = "aarch64")]
const _: () = assert!(
    USER_VA_LIMIT == (1u64 << 48),
    "aarch64 USER_VA_LIMIT must match TCR T0SZ=16 (2^48); the paging module cross-asserts this"
);
#[cfg(target_arch = "x86_64")]
const _: () = assert!(
    USER_VA_LIMIT == (1u64 << 47),
    "x86_64 USER_VA_LIMIT must be the PML4[256] kernel-half base (2^47)"
);

/// Kernel → VM boot handoff page (one-way, one-shot).
///
/// The kernel fills this struct in a dedicated frame before scheduling VM
/// and maps the frame read-only, user-accessible at
/// [`VM_BOOT_HANDOFF_VA`]. VM reads it once during startup; later kernel
/// state changes do not affect the already-read copy.
///
/// `root_paddr` is the address-space identity hand-off (A1): VM's initial
/// page table IS the bootstrap root the kernel built and enabled, so VM
/// constructs its self page-table handle from this physical address
/// instead of creating a fresh root. `root_paddr == 0` is invalid — the
/// bootstrap root is always a real, page-aligned physical page.
///
/// `free_regions` + `deducted` are the resource hand-off (A2,
/// post-bootstrap classification): the kernel finalizes VM's free list by
/// cutting `LiveBootstrap(t_classify)` out of the full memmap and hands
/// the deduction record itself over, so VM can verify
/// `LiveBootstrap ∩ VM-free = ∅` at the consumer boundary instead of
/// trusting the kernel's bookkeeping.
///
/// The struct must fit one 4 KiB page (`size_of::<VmBootHandoff>() <= 4096`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmBootHandoff {
    /// Must be [`VM_BOOT_HANDOFF_MAGIC`].
    pub magic: u32,
    /// Must be [`VM_BOOT_HANDOFF_VERSION`].
    pub version: u32,
    /// Physical address of VM's bootstrap page-table root.
    pub root_paddr: u64,
    /// Bytes the kernel allocated to load VM (ELF segments, stacks, boot
    /// page-table pages). C: `kinfo.vm_allocated_bytes` (param.h) —
    /// consumed by VM's usage queries (region.c:1366-1373).
    pub vm_allocated_bytes: u64,
    /// Kernel image footprint. C: `kinfo.kernel_allocated_bytes`
    /// (pre_init.c:162).
    pub kernel_allocated_static: u64,
    /// Boot page-table pages the kernel allocated while mapping itself.
    /// C: `kinfo.kernel_allocated_bytes_dynamic` (pg_utils.c:154).
    pub kernel_allocated_dynamic: u64,
    /// Kernel text virtual base (V11/E3: per-process kernel mapping
    /// source — replaces VM's hardcoded mock, kernel `kern_virt_base`).
    pub kern_virt_base: u64,
    /// Kernel text physical base (V11/E3; kernel `kern_phys_base`).
    pub kern_phys_base: u64,
    /// Pages in the kernel image span (V11/E3; `kern_size / PAGE_SIZE`).
    /// C splits text/data; minix-rs maps the whole contiguous span.
    pub kern_text_pages: u32,
    /// Pages in the kernel data span (V11/E3; 0 = whole-span-as-text,
    /// the minix-rs kernel image is one contiguous span).
    pub kern_data_pages: u32,
    /// Virtual base of the kernel Direct Map window (fix26, handoff v5;
    /// arch constant `KERNEL_DIRECT_MAP_BASE` — e.g. x86-64
    /// `0xFFFF_8080_0000_0000`, riscv64 differs). VM maps this window
    /// into every process page table so kernel PA access survives a CR3
    /// switch; a wrong base silently breaks the first context switch.
    pub kern_dm_vbase: u64,
    /// Pages the kernel Direct Map window covers in physical address
    /// space (fix26, handoff v5; page-rounded highest PA
    /// `establish_boot_dm` covered in the kernel window, i.e.
    /// `dm_coverage::kernel_dm_pa_end / 4096`). The window maps PA
    /// `[0, kern_dm_pages * 4096)` contiguously — a superset of the
    /// candidate union, holes included (harmless: supervisor-only
    /// translations to unbacked addresses nobody touches).
    pub kern_dm_pages: u64,
    /// Initial user stack top. C: `kinfo.user_sp = USR_STACKTOP`
    /// (pre_init.c:156) — exec_bootproc builds the initial stack frame
    /// downward from this value and the frame's `vsp` becomes the
    /// process's starting stack pointer (main.c:391-411).
    pub user_sp: u64,
    /// Fresh-boot flag (C: `is_first_time()`, main.c:79-88). 1 = fresh.
    pub is_first_time: u32,
    /// Valid entries in `free_regions`.
    pub free_region_count: u32,
    /// Valid entries in `deducted`.
    pub deducted_count: u32,
    /// Valid entries in `modules`.
    pub module_count: u32,
    /// VM's free physical memory (A2 classification output) — page-aligned,
    /// disjoint from `deducted`, clipped to VM's Direct Map window.
    pub free_regions: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_REGIONS],
    /// LiveBootstrap(t_classify) record — the ranges the kernel deducted
    /// from the memmap (kernel image, self root, boot bump region,
    /// bootstrap-used frames, reserved module blobs). VM verifies that no
    /// free region overlaps any of these before enabling its PMM.
    pub deducted: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_DEDUCTED],
    /// Boot module blobs the kernel still reserves at VM start (every
    /// module except VM itself — VM's blob was reclaimed after its ELF was
    /// copied, C: protect.c:450-451).
    pub modules: [HandoffModule; VM_BOOT_HANDOFF_MAX_MODULES],
    /// Valid entries in `kern_ident`.
    pub kern_ident_count: u32,
    /// Kernel runtime identity windows (fix27, version 6): occupied
    /// physical regions the running kernel image and its tables live
    /// in (UEFI link-in execution model — see the version history
    /// above). `map_kernel` maps each region identity (VA = PA)
    /// supervisor-only into every process page table. Empty on
    /// platforms without link-in execution.
    pub kern_ident: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_IDENT],
    /// C: `kinfo.boot_procs[NR_BOOT_PROCS]` — the full boot image table
    /// (kernel tasks with negative proc_nr first, then user-space servers).
    pub boot_procs: [BootImage; NR_BOOT_PROCS],
}

impl VmBootHandoff {
    /// Kernel layout for per-process kernel mappings (V11/E3).
    ///
    /// `Some` for version ≥ 3 handoffs (the kernel reports its text/data
    /// span); `None` for version ≤ 2 — the consumer keeps its historical
    /// fallback (VM: the mock `KernelLayout` constants + a warning).
    ///
    /// The Direct Map window is the kernel's real one only from version
    /// 5 on (fix26): v3/v4 handoffs carry no DM fields and this function
    /// returns the historical sentinels, which are wrong on real
    /// hardware (base off by one PML4 slot, 4-page coverage). That is
    /// unreachable in production — [`Self::validate`] hard-asserts the
    /// exact current version — so the sentinels only ever surface in
    /// host tests that fabricate old handoffs.
    pub fn kernel_layout(&self) -> Option<KernelLayout> {
        if self.version < 3 {
            return None;
        }
        let (dm_vbase, dm_pages) = if self.version >= 5 {
            (self.kern_dm_vbase, self.kern_dm_pages as usize)
        } else {
            (0xFFFF_8000_0000_0000, 4) // legacy sentinels (see doc)
        };
        let (ident, ident_count) = if self.version >= 6 {
            (self.kern_ident, self.kern_ident_count as usize)
        } else {
            ([HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_IDENT], 0)
        };
        Some(KernelLayout::new_with_ident(
            self.kern_virt_base,
            self.kern_phys_base,
            self.kern_text_pages as usize,
            self.kern_data_pages as usize,
            dm_vbase,
            dm_pages,
            ident,
            ident_count,
        ))
    }

    /// Total size in bytes — must stay within one 4 KiB page.
    ///
    /// The `const _` assertion below enforces this at compile time.
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// Validates the handoff header.
    ///
    /// A mismatch means the page at `VM_BOOT_HANDOFF_VA` is not a valid
    /// handoff (kernel/VM version skew or a stray mapping) — a boot
    /// contract violation, not a recoverable error.
    pub fn validate(&self) {
        assert_eq!(
            self.magic, VM_BOOT_HANDOFF_MAGIC,
            "VmBootHandoff: bad magic 0x{:08x}",
            self.magic
        );
        assert_eq!(
            self.version, VM_BOOT_HANDOFF_VERSION,
            "VmBootHandoff: unsupported version {}",
            self.version
        );
        assert!(
            self.root_paddr != 0 && self.root_paddr & 0xFFF == 0,
            "VmBootHandoff: root_paddr 0x{:x} must be a non-zero page-aligned physical address",
            self.root_paddr
        );
        assert!(
            self.is_first_time <= 1,
            "VmBootHandoff: is_first_time {} must be 0 or 1",
            self.is_first_time
        );
        assert!(
            (self.free_region_count as usize) <= VM_BOOT_HANDOFF_MAX_REGIONS,
            "VmBootHandoff: free_region_count {} exceeds capacity",
            self.free_region_count
        );
        assert!(
            (self.deducted_count as usize) <= VM_BOOT_HANDOFF_MAX_DEDUCTED,
            "VmBootHandoff: deducted_count {} exceeds capacity",
            self.deducted_count
        );
        assert!(
            (self.module_count as usize) <= VM_BOOT_HANDOFF_MAX_MODULES,
            "VmBootHandoff: module_count {} exceeds capacity",
            self.module_count
        );
        assert!(
            (self.kern_ident_count as usize) <= VM_BOOT_HANDOFF_MAX_IDENT,
            "VmBootHandoff: kern_ident_count {} exceeds capacity",
            self.kern_ident_count
        );
    }
}

const _: () = assert!(
    core::mem::size_of::<VmBootHandoff>() <= 4096,
    "VmBootHandoff must fit one 4 KiB handoff page"
);

/// Kernel memory layout parameters used to map the kernel into each
/// process's page table.
///
/// Replaces the hardcoded constants previously guarded by the
/// `hardcoded_kernel_layout` feature (hardcoded kernel layout fix). The kernel layout is
/// determined once at boot (from multiboot2/stivale2 headers or linker
/// symbols) and shared across all user processes — the kernel mapping
/// is identical in every page table.
///
/// # Fields
///
/// - `kernel_text_vbase`: Virtual address where kernel text starts.
/// - `kernel_text_pbase`: Physical address where kernel text starts.
/// - `kernel_text_pages`: Number of pages in kernel text segment.
/// - `kernel_data_pages`: Number of pages in kernel data segment
///   (immediately follows text segment in both VA and PA space).
/// - `dm_vbase`: Virtual address where the kernel direct map starts.
/// - `dm_pages`: Number of pages in the kernel direct map.
/// - `ident` / `ident_count`: kernel runtime identity windows (fix27) —
///   occupied physical regions to replay identity (VA = PA) into every
///   process page table. Valid entries are `ident[..ident_count]`.
///
/// # Safety invariants
///
/// - All `*_pages` fields must be small enough that `*_vbase + pages * PAGE_SIZE`
///   does not overflow `u64`.
/// - `kernel_text_pbase` must be page-aligned.
/// - `dm_pages` must not exceed available physical memory.
/// - `ident_count <= ident.len()`; entries page-aligned, sorted, and
///   non-overlapping (the kernel builds them that way in
///   `vm_handoff::build_identity_windows`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelLayout {
    pub kernel_text_vbase: u64,
    pub kernel_text_pbase: u64,
    pub kernel_text_pages: usize,
    pub kernel_data_pages: usize,
    pub dm_vbase: u64,
    pub dm_pages: usize,
    pub ident: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_IDENT],
    pub ident_count: usize,
}

impl KernelLayout {
    /// Creates a `KernelLayout` from raw boot-provided parameters.
    ///
    /// Intended to be called once during VM server initialization, after
    /// the boot image / multiboot2 / stivale2 headers have been parsed.
    /// The resulting value is stored in a global (see `vm::global`) and
    /// read by every `init_page_table()` call.
    ///
    /// Produces a layout with NO identity windows — [`Self::new_with_ident`]
    /// is the fix27 constructor used for version ≥ 6 handoffs. A layout
    /// without identity windows is correct only when the kernel really
    /// executes higher-half (never the UEFI link-in production model).
    pub const fn new(
        kernel_text_vbase: u64,
        kernel_text_pbase: u64,
        kernel_text_pages: usize,
        kernel_data_pages: usize,
        dm_vbase: u64,
        dm_pages: usize,
    ) -> Self {
        Self {
            kernel_text_vbase,
            kernel_text_pbase,
            kernel_text_pages,
            kernel_data_pages,
            dm_vbase,
            dm_pages,
            ident: [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_IDENT],
            ident_count: 0,
        }
    }

    /// Creates a `KernelLayout` including the kernel runtime identity
    /// windows (fix27, handoff version ≥ 6).
    pub const fn new_with_ident(
        kernel_text_vbase: u64,
        kernel_text_pbase: u64,
        kernel_text_pages: usize,
        kernel_data_pages: usize,
        dm_vbase: u64,
        dm_pages: usize,
        ident: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_IDENT],
        ident_count: usize,
    ) -> Self {
        Self {
            kernel_text_vbase,
            kernel_text_pbase,
            kernel_text_pages,
            kernel_data_pages,
            dm_vbase,
            dm_pages,
            ident,
            ident_count,
        }
    }

    /// The valid identity-window prefix (page-aligned `{base, size}`
    /// pairs, sorted and non-overlapping — `map_kernel` input).
    pub fn ident_regions(&self) -> &[HandoffMemRegion] {
        &self.ident[..self.ident_count]
    }
}

/// Boot-time process information.
///
/// Set in kernel/table.c and passed to services during boot.
/// Used by VM, PM, RS, and IS services.
///
/// Corresponds to Minix3's `struct boot_image` in `minix/include/minix/type.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootImage {
    pub proc_nr: i32,
    pub proc_name: [u8; PROC_NAME_LEN],
    pub endpoint: Endpoint,
    pub start_addr: u64,
    pub len: u64,
}

impl BootImage {
    pub const fn empty() -> Self {
        Self {
            proc_nr: 0,
            proc_name: [0; PROC_NAME_LEN],
            endpoint: Endpoint::NONE,
            start_addr: 0,
            len: 0,
        }
    }

    pub fn name(&self) -> &str {
        let len = self
            .proc_name
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(PROC_NAME_LEN);
        core::str::from_utf8(&self.proc_name[..len]).unwrap_or("<invalid>")
    }

    /// Decode one GET_IMAGE wire row ([`crate::types::boot_image::
    /// BootImageStruct`], 40 字节,C `struct boot_image` — type.h:148-154)
    /// 为内部建模行。GET_IMAGE 消费方(PM `BootParams` / RS `BootTables`)
    /// 共用此转换,wire 布局只有 `BootImageStruct` 一个权威。
    pub fn from_wire(wire: &crate::types::boot_image::BootImageStruct) -> Self {
        Self {
            proc_nr: wire.proc_nr,
            proc_name: wire.proc_name,
            endpoint: Endpoint(wire.endpoint),
            start_addr: wire.start_addr,
            len: wire.len,
        }
    }
}

impl Default for BootImage {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_image_empty() {
        let img = BootImage::empty();
        assert_eq!(img.proc_nr, 0);
        assert_eq!(img.endpoint, Endpoint::NONE);
        assert_eq!(img.start_addr, 0);
        assert_eq!(img.len, 0);
    }

    /// from_wire：GET_IMAGE 线上行（BootImageStruct）逐域折为建模行。
    #[test]
    fn test_boot_image_from_wire() {
        use crate::types::boot_image::BootImageStruct;
        let wire = BootImageStruct {
            proc_nr: 2,
            proc_name: {
                let mut n = [0u8; PROC_NAME_LEN];
                n[..2].copy_from_slice(b"rs");
                n
            },
            endpoint: 2,
            start_addr: 0x11_0000,
            len: 0x2000,
        };
        let img = BootImage::from_wire(&wire);
        assert_eq!(img.proc_nr, 2);
        assert_eq!(img.endpoint, Endpoint(2));
        assert_eq!(img.name(), "rs");
        assert_eq!(img.start_addr, 0x11_0000);
        assert_eq!(img.len, 0x2000);
    }

    #[test]
    fn test_boot_image_name() {
        let mut img = BootImage::empty();
        img.proc_name[0] = b'p';
        img.proc_name[1] = b'm';
        img.proc_name[2] = 0;
        assert_eq!(img.name(), "pm");
    }

    #[test]
    fn test_boot_image_name_truncated() {
        let mut img = BootImage::empty();
        for i in 0..PROC_NAME_LEN {
            img.proc_name[i] = b'a';
        }
        assert_eq!(img.name(), "aaaaaaaaaaaaaaaa");
    }

    #[test]
    fn test_kernel_layout_new() {
        let layout = KernelLayout::new(
            0xFFFF_FFFF_8000_0000,
            0x100_0000,
            8,
            8,
            0xFFFF_8000_0000_0000,
            4,
        );
        assert_eq!(layout.kernel_text_vbase, 0xFFFF_FFFF_8000_0000);
        assert_eq!(layout.kernel_text_pbase, 0x100_0000);
        assert_eq!(layout.kernel_text_pages, 8);
        assert_eq!(layout.kernel_data_pages, 8);
        assert_eq!(layout.dm_vbase, 0xFFFF_8000_0000_0000);
        assert_eq!(layout.dm_pages, 4);
    }

    #[test]
    fn test_kernel_layout_eq() {
        let a = KernelLayout::new(1, 2, 3, 4, 5, 6);
        let b = KernelLayout::new(1, 2, 3, 4, 5, 6);
        let c = KernelLayout::new(1, 2, 3, 4, 5, 7);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_kernel_layout_copy() {
        let a = KernelLayout::new(1, 2, 3, 4, 5, 6);
        let b = a; // Copy semantics
        assert_eq!(a, b);
    }
    #[cfg(test)]
    mod kernel_layout_tests {
        use super::*;

        /// A minimal handoff skeleton: only the fields kernel_layout()
        /// reads are meaningful for the given version.
        fn handoff(version: u32) -> VmBootHandoff {
            let mut h = VmBootHandoff {
                magic: VM_BOOT_HANDOFF_MAGIC,
                version,
                root_paddr: 0,
                vm_allocated_bytes: 0,
                kernel_allocated_static: 0,
                kernel_allocated_dynamic: 0,
                kern_virt_base: 0,
                kern_phys_base: 0,
                kern_text_pages: 0,
                kern_data_pages: 0,
                kern_dm_vbase: 0,
                kern_dm_pages: 0,
                user_sp: 0,
                is_first_time: 1,
                free_region_count: 0,
                deducted_count: 0,
                module_count: 0,
                free_regions: [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_REGIONS],
                deducted: [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_DEDUCTED],
                modules: [HandoffModule::ZERO; VM_BOOT_HANDOFF_MAX_MODULES],
                kern_ident_count: 0,
                kern_ident: [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_IDENT],
                boot_procs: [BootImage::empty(); NR_BOOT_PROCS],
            };
            // Handoff::validate expects the field to be zeroed when fresh —
            // the tests only read kernel_layout(), so nothing else matters.
            h
        }

        #[test]
        fn test_kernel_layout_v3_reports_span() {
            let mut h = handoff(3);
            h.kern_virt_base = 0xFFFF_FFFF_8000_0000;
            h.kern_phys_base = 0x100_0000;
            h.kern_text_pages = 24;
            h.kern_data_pages = 0;
            let l = h.kernel_layout().expect("v3 must carry the layout");
            assert_eq!(l.kernel_text_vbase, 0xFFFF_FFFF_8000_0000);
            assert_eq!(l.kernel_text_pbase, 0x100_0000);
            assert_eq!(l.kernel_text_pages, 24);
            assert_eq!(l.kernel_data_pages, 0);
            // v3 has no DM fields — the historical sentinels surface
            // (host-test shape only; production validate() pins the
            // current version).
            assert_eq!(l.dm_vbase, 0xFFFF_8000_0000_0000);
            assert_eq!(l.dm_pages, 4);
        }

        /// v5 (fix26): the kernel's real Direct Map window values flow
        /// through — no sentinels. v5 predates the identity windows, so
        /// `ident_regions()` stays empty.
        #[test]
        fn test_kernel_layout_v5_reports_dm_window() {
            let mut h = handoff(5);
            h.kern_virt_base = 0xFFFF_8000_0000_0000;
            h.kern_phys_base = 0x100_0000;
            h.kern_text_pages = 48;
            h.kern_data_pages = 0;
            h.kern_dm_vbase = 0xFFFF_8080_0000_0000;
            h.kern_dm_pages = 0x2_0000; // 512 MiB / 4 KiB
            let l = h.kernel_layout().expect("v5 must carry the layout");
            assert_eq!(l.dm_vbase, 0xFFFF_8080_0000_0000);
            assert_eq!(l.dm_pages, 0x2_0000);
            assert_eq!(l.ident_regions().len(), 0);
        }

        /// v6 (fix27): the kernel runtime identity windows flow through.
        #[test]
        fn test_kernel_layout_v6_reports_identity_windows() {
            let mut h = handoff(VM_BOOT_HANDOFF_VERSION);
            assert_eq!(VM_BOOT_HANDOFF_VERSION, 6);
            h.kern_ident[0] = HandoffMemRegion { base: 0x1da0_0000, size: 0x40_0000 };
            h.kern_ident[1] = HandoffMemRegion { base: 0x1f00_0000, size: 0x1000 };
            h.kern_ident_count = 2;
            let l = h.kernel_layout().expect("v6 must carry the layout");
            assert_eq!(l.ident_regions().len(), 2);
            assert_eq!(l.ident_regions()[0].base, 0x1da0_0000);
            assert_eq!(l.ident_regions()[1].size, 0x1000);
        }

        #[test]
        fn test_kernel_layout_v2_is_none() {
            assert!(
                handoff(2).kernel_layout().is_none(),
                "version ≤ 2 handoffs carry no kernel layout (VM keeps mock)"
            );
        }
    }
}
