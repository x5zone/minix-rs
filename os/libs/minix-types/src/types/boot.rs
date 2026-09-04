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
pub const VM_BOOT_HANDOFF_VERSION: u32 = 2;

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
    /// C: `kinfo.boot_procs[NR_BOOT_PROCS]` — the full boot image table
    /// (kernel tasks with negative proc_nr first, then user-space servers).
    pub boot_procs: [BootImage; NR_BOOT_PROCS],
}

impl VmBootHandoff {
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
///
/// # Safety invariants
///
/// - All `*_pages` fields must be small enough that `*_vbase + pages * PAGE_SIZE`
///   does not overflow `u64`.
/// - `kernel_text_pbase` must be page-aligned.
/// - `dm_pages` must not exceed available physical memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelLayout {
    pub kernel_text_vbase: u64,
    pub kernel_text_pbase: u64,
    pub kernel_text_pages: usize,
    pub kernel_data_pages: usize,
    pub dm_vbase: u64,
    pub dm_pages: usize,
}

impl KernelLayout {
    /// Creates a `KernelLayout` from raw boot-provided parameters.
    ///
    /// Intended to be called once during VM server initialization, after
    /// the boot image / multiboot2 / stivale2 headers have been parsed.
    /// The resulting value is stored in a global (see `vm::global`) and
    /// read by every `init_page_table()` call.
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
        }
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
}
