//! Firmware-agnostic kernel/module loading.
//!
//! Both UEFI and OpenSBI+U-Boot paths need to do exactly the same thing
//! once "file bytes" are available:
//!   1. Read kernel.elf bytes.
//!   2. Parse PT_LOAD via `minix_elf`.
//!   3. Copy each segment to its physical address and zero-fill BSS.
//!   4. Read each module's bytes.
//!   5. Place module bytes in physical memory and record `BootModule`.
//!
//! What differs is **only** how raw file bytes are obtained:
//! - UEFI uses the `SimpleFileSystem` protocol on the ESP.
//! - U-Boot pre-loads files into RAM with `fatload`; the shim builds its own
//!   `BootFileTable` (in `.bss`, from the shared layout constants) describing
//!   where each file lives.
//!
//! We capture that difference in the [`FileLoader`] trait, and then write
//! the loading logic **once**, generically over `L: FileLoader`. Both
//! firmware paths invoke the exact same code path; only the loader value
//! differs.

use alloc::string::String;
use alloc::vec::Vec;
use minix_boot::BootModule;
use minix_types::{PhysBytes, VirBytes};

/// Path to the kernel ELF (same convention for UEFI ESP and U-Boot FAT).
///
/// Uses forward slashes as the canonical format. The `UefiFileLoader`
/// converts to backslashes before passing to UEFI's `SimpleFileSystem`.
pub const KERNEL_PATH: &str = "/EFI/minix/kernel.elf";

/// Platform DTB descriptor blob (optional): when present on the ESP it takes
/// part in the memmap out-of-bounds gate and in source printing; when absent
/// the behavior is unchanged. On the build side it is injected by
/// `xtask image` (once that side is free to do so) or by a test-script
/// mcopy, and originates from
/// `qemu-system-aarch64 -machine virt,dumpdtb=`.
pub const DUMPDTB_PATH: &str = "/dumpdtb.dtb";

/// Directory containing boot modules (same convention for both paths).
pub const MODULES_DIR: &str = "/EFI/minix/modules";

/// Boot modules to load, **in the order the kernel consumes them**.
///
/// The kernel reads these modules positionally: the `i`-th entry here becomes
/// the `i`-th [`BootModule`] in `kernel_info.boot_modules`, which the kernel
/// maps to process slot `BOOT_MODULE_PROC_NRS[i]` while building the process
/// table (see `BOOT_MODULE_PROC_NRS` in `os/kernel/src/proc.rs`). The kernel
/// also asserts at boot that the number of modules equals `NR_BOOT_MODULES`
/// (12) (see the `expected {} boot modules` assert there). So **both the
/// order and the count of
/// this list are load-bearing** — they are not free to reorder, and a missing
/// entry is a boot-time failure, not a cosmetic gap.
///
/// The authoritative source is the C kernel boot image table
/// `minix3/minix/kernel/table.c:44-64` (`struct boot_image image[]`). Its
/// user-space section, in order, is: `ds, rs, pm, sched, vfs, memory, tty,
/// mib, vm, pfs, mfs, init`. This list reproduces exactly that order. Note
/// that a module's process number is **not** its position — `BOOT_MODULE_PROC_NRS`
/// maps position to the fixed `com.h` number (e.g. `ds` is position 0 but
/// process number 6).
///
/// Each name doubles as the file the boot-shim reads under [`MODULES_DIR`] and
/// as the `p_name` the booted process carries, so the image-packaging step
/// must install a file with this exact name.
/// The shared canonical list lives in `minix-boot` so the kernel's strict
/// gate can compare `BootModule.name` against the same positions without
/// depending on boot-shim (DESIGN-02). The array's type pins the count at
/// compile time, so a count change fails both sides.
pub use minix_boot::MODULE_NAMES;

/// Result of computing the kernel's load layout from its ELF header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelLoadResult {
    /// Lowest physical address across all PT_LOAD segments.
    pub kern_phys_base: PhysBytes,
    /// Lowest virtual address across all PT_LOAD segments.
    pub kern_virt_base: VirBytes,
    /// Total span from lowest paddr to highest `paddr + memsz`.
    pub kern_size: u64,
    /// Kernel entry point (`e_entry` from ELF header).
    pub entry_point: u64,
}

/// Bytes of one boot file, as either a borrow of loader-owned storage or an
/// owned read buffer.
///
/// The explicit enum is the R18 D-4 contract: the loader says which side owns
/// the bytes instead of forcing every read to copy into a `Vec`. The U-Boot
/// leg already holds the file in RAM (U-Boot `fatload`ed it), so it returns
/// [`FileBytes::Borrowed`]; the UEFI leg must read through the firmware
/// protocol and returns [`FileBytes::Owned`]. While a `Borrowed` value is
/// alive the loader's backing storage must not be overwritten (the UEFI flow
/// never borrows; the U-Boot table is written before the shim runs and is not
/// modified afterwards).
pub enum FileBytes<'a> {
    /// Bytes owned by the loader / firmware staging area.
    Borrowed(&'a [u8]),
    /// Bytes read into a fresh buffer by the loader.
    Owned(Vec<u8>),
}

impl<'a> FileBytes<'a> {
    /// The bytes, regardless of ownership.
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }

    /// Byte length.
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Whether the file is empty.
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Take an owned buffer, copying only when the bytes are borrowed.
    pub fn into_owned(self) -> Vec<u8> {
        match self {
            Self::Owned(bytes) => bytes,
            Self::Borrowed(bytes) => bytes.to_vec(),
        }
    }
}

/// Firmware-agnostic file access.
///
/// Implementations:
/// - `UefiFileLoader` — calls `SimpleFileSystem::read()` via the `uefi` crate.
/// - `UbootFileLoader` — looks up pre-loaded files in a `BootFileTable`
///   placed in physical memory by U-Boot's `fatload` commands.
///
/// Both implementations return [`FileBytes`] so the caller (the shared loader
/// code) can process the bytes without caring about ownership or the data
/// source; only the firmware-specific leg decides whether a copy is needed.
///
/// `read` returns `None` when the file is absent (non-fatal for optional
/// modules) or unreadable. Callers use `read_required` to make absence
/// fatal (e.g. for the kernel itself).
pub trait FileLoader {
    /// Read a file by path. Returns `None` if the file does not exist or
    /// cannot be read.
    fn read<'a>(&'a self, path: &str) -> Option<FileBytes<'a>>;

    /// Read a file that must exist. Default panics with a useful message.
    fn read_required<'a>(&'a self, path: &str) -> FileBytes<'a> {
        match self.read(path) {
            Some(d) => d,
            None => panic!("boot-shim: required file not found: {}", path),
        }
    }
}

/// Strategy for placing loaded module bytes in physical memory.
///
/// UEFI uses `boot::allocate_pages(LOADER_DATA)`; U-Boot has no dynamic
/// allocator at this point, so the OpenSBI path uses a small bump allocator
/// over a region carved from DRAM. We hide that behind a function pointer
/// to keep the shared loading logic single-source.
///
/// The allocator must return a page-aligned physical address that points
/// to `num_pages` contiguous, writable pages reserved for module data.
pub type PageAllocator = fn(num_pages: usize) -> Option<u64>;

/// Parse an ELF image and compute kernel base addresses and size.
///
/// Pure function — does not touch memory. Used by both firmware paths
/// and by unit tests.
pub fn compute_kernel_layout(elf_data: &[u8]) -> Result<KernelLoadResult, minix_elf::ElfError> {
    let mut iter = minix_elf::segment_iter(elf_data)?;
    let entry_point = iter.ehdr().e_entry;

    let mut kern_phys_base = u64::MAX;
    let mut kern_virt_base = u64::MAX;
    let mut kern_end_phys = 0u64;
    let mut found_load = false;

    while let Some(seg_result) = iter.next() {
        // A malformed program header (truncated segment, unreadable phdr,
        // filesz > memsz) means the image cannot be trusted for a boot
        // layout: bubbling the error up beats the old silent skip, which
        // produced a smaller-but-bootable layout (I-48 / D-084). The
        // filesz > memsz overwrite guard now lives inside the iterator, so
        // every consumer gets it without a per-caller check.
        let seg = seg_result?;
        found_load = true;
        let seg_end = seg
            .paddr
            .checked_add(seg.memsz)
            .ok_or(minix_elf::ElfError::InvalidSegment)?;
        kern_phys_base = kern_phys_base.min(seg.paddr);
        kern_virt_base = kern_virt_base.min(seg.vaddr);
        kern_end_phys = kern_end_phys.max(seg_end);
    }

    if !found_load {
        return Err(minix_elf::ElfError::NoLoadSegments);
    }

    let kern_size = kern_end_phys
        .checked_sub(kern_phys_base)
        .ok_or(minix_elf::ElfError::InvalidSegment)?;

    Ok(KernelLoadResult {
        kern_phys_base: PhysBytes(kern_phys_base),
        kern_virt_base: VirBytes(kern_virt_base),
        kern_size,
        entry_point,
    })
}

/// Load PT_LOAD segments from an ELF image into a destination buffer.
///
/// Pure function — used by tests; production code uses
/// [`load_segments_into_phys_memory`] to write directly to physical
/// addresses.
///
/// Returns the number of segments loaded, or an error for a malformed
/// image (unparsable header/phdr table, or a malformed program header
/// surfaced by the iterator). The old form folded every failure into
/// "`0` segments", which made "empty image" indistinguishable from
/// "broken image" (I-48 / D-084).
pub fn load_segments_into_buffer(
    elf_data: &[u8],
    buf: &mut [u8],
    buf_base_paddr: u64,
) -> Result<usize, minix_elf::ElfError> {
    let iter = minix_elf::segment_iter(elf_data)?;

    let mut count = 0;
    for seg_result in iter {
        let seg = seg_result?;
        // A segment below the buffer base is a fixture/input error, not a
        // reason to copy it to offset 0; skip it rather than silently
        // relocating it.
        let Some(seg_offset_in_buf) = seg.paddr.checked_sub(buf_base_paddr) else {
            continue;
        };
        let seg_offset_in_buf = seg_offset_in_buf as usize;
        let Some(file_end) = seg_offset_in_buf.checked_add(seg.filesz as usize) else {
            continue;
        };
        let Some(mem_end) = seg_offset_in_buf.checked_add(seg.memsz as usize) else {
            continue;
        };

        if file_end <= buf.len() {
            let Some(src_end) = (seg.offset as usize).checked_add(seg.filesz as usize) else {
                continue;
            };
            let src = &elf_data[seg.offset as usize..src_end];
            buf[seg_offset_in_buf..file_end].copy_from_slice(src);
        }

        if mem_end <= buf.len() && seg.memsz > seg.filesz {
            buf[file_end..mem_end].fill(0);
        }

        count += 1;
    }
    Ok(count)
}

/// Copy each PT_LOAD segment to its physical address and zero-fill BSS.
///
/// # Safety
///
/// Caller must ensure that the physical addresses described by the ELF
/// (`seg.paddr` .. `seg.paddr + seg.memsz`) are mapped, writable, and
/// do not overlap with the boot-shim itself or any firmware structures
/// still in use. In our boot paths firmware identity-maps physical RAM
/// during boot, and the kernel linker script guarantees non-overlapping
/// LMA placement.
///
/// The caller must also have run [`compute_kernel_layout`] (or an equivalent
/// validation) first: this function itself does not reject `filesz > memsz`
/// or address arithmetic overflow. The production path goes through
/// [`load_kernel_with_loader`], which calls `compute_kernel_layout` before
/// this function; direct callers must uphold the same precondition.
///
/// Overlap detection: this function rejects ELFs whose PT_LOAD segments
/// overlap in physical memory, and panics when any segment's file-byte range
/// overlaps any segment's destination span (the copy/BSS zero-fill would
/// otherwise corrupt the source buffer mid-load). Without this, a buggy or
/// maliciously crafted ELF would silently overwrite earlier segments or the
/// heap buffer it is read from.
pub unsafe fn load_segments_into_phys_memory(elf_data: &[u8]) -> Result<(), minix_elf::ElfError> {
    let iter = minix_elf::segment_iter(elf_data)?;
    // Explicit `Result` collection (I-48 / D-084): a bare `collect()` on a
    // `Result`-item iterator would silently keep `Err` entries as
    // `Vec<Result<..>>` — the same "malformed looks like fewer segments"
    // disease one layer down.
    let segments: Vec<_> = iter.collect::<Result<Vec<_>, minix_elf::ElfError>>()?;
    if let Some(_) = first_overlapping_pair(&segments) {
        return Err(minix_elf::ElfError::InvalidSegment);
    }
    // D01-R9: the ELF bytes live in one heap allocation; under the boot
    // identity map the pointer value is physical. Any segment's destination
    // span (including its BSS zero-fill) must stay clear of *every* segment's
    // file bytes, not just its own: a cross-segment overlap is the same UB
    // and silently corrupts the source mid-load.
    let src_base = elf_data.as_ptr() as u64;
    if let Some((src_idx, dst_idx)) = first_source_destination_overlap(src_base, &segments) {
        panic!(
            "boot-shim: kernel ELF segment {src_idx} source overlaps segment {dst_idx} destination LMA"
        );
    }
    for seg in &segments {
        let dest = seg.paddr as *mut u8;
        let src = &elf_data[seg.offset as usize..(seg.offset + seg.filesz) as usize];
        unsafe {
            dest.copy_from_nonoverlapping(src.as_ptr(), seg.filesz as usize);
            if seg.memsz > seg.filesz {
                let bss_start = dest.add(seg.filesz as usize);
                let bss_size = (seg.memsz - seg.filesz) as usize;
                core::ptr::write_bytes(bss_start, 0, bss_size);
            }
        }
    }
    Ok(())
}

/// Pure helper: detect the first (source segment, destination segment) pair
/// whose ranges overlap: source = `elf_base + offset..+ filesz`, destination
/// = `paddr..+ memsz` (the copy plus the BSS zero-fill).
fn first_source_destination_overlap(
    elf_base: u64,
    segments: &[minix_elf::LoadSegment],
) -> Option<(usize, usize)> {
    for (dst_idx, dst) in segments.iter().enumerate() {
        if dst.memsz == 0 {
            continue;
        }
        let d_lo = dst.paddr;
        let d_hi = dst.paddr.saturating_add(dst.memsz);
        for (src_idx, src) in segments.iter().enumerate() {
            if src.filesz == 0 {
                continue;
            }
            let s_lo = elf_base.saturating_add(src.offset);
            let s_hi = s_lo.saturating_add(src.filesz);
            if s_lo < d_hi && d_lo < s_hi {
                return Some((src_idx, dst_idx));
            }
        }
    }
    None
}

/// Pure helper: detect the first pair of PT_LOAD segments whose physical
/// memory ranges overlap. Returns `Some((a, b))` on the first collision,
/// or `None` if all segments are disjoint. Exposed for unit testing.
fn first_overlapping_pair(segments: &[minix_elf::LoadSegment]) -> Option<(usize, usize)> {
    for (i, a) in segments.iter().enumerate() {
        if a.memsz == 0 {
            continue;
        }
        let a_end = a.paddr.saturating_add(a.memsz);
        for (j, b) in segments.iter().enumerate().skip(i + 1) {
            if b.memsz == 0 {
                continue;
            }
            let b_end = b.paddr.saturating_add(b.memsz);
            // Two half-open ranges [a.paddr, a_end) and [b.paddr, b_end)
            // overlap iff a.paddr < b_end && b.paddr < a_end.
            if a.paddr < b_end && b.paddr < a_end {
                return Some((i, j));
            }
        }
    }
    None
}

/// Load the kernel ELF: read bytes via `loader`, compute layout, copy segments.
///
/// This is the shared kernel-loading path used by both UEFI and OpenSBI.
/// All firmware-specific concerns (where the bytes come from) are
/// encapsulated in the `FileLoader` argument.
pub fn load_kernel_with_loader<L: FileLoader>(
    loader: &L,
) -> Result<KernelLoadResult, minix_elf::ElfError> {
    load_kernel_with_loader_gated(loader, |_| {})
}

/// Load the kernel ELF with a pre-copy layout gate.
///
/// The gate runs after the ELF program headers are parsed and **before the
/// first segment byte is written**. A leg whose destination memory has fixed
/// partitions (the U-Boot shim's bump pool) uses it to refuse a kernel LMA
/// span that overlaps those partitions: a post-copy check would be too late,
/// because the copy has already overwritten the pool.
pub fn load_kernel_with_loader_gated<L: FileLoader, G: FnOnce(&KernelLoadResult)>(
    loader: &L,
    layout_gate: G,
) -> Result<KernelLoadResult, minix_elf::ElfError> {
    let elf_data = loader.read_required(KERNEL_PATH);
    let layout = compute_kernel_layout(elf_data.as_slice())?;
    // The jump target is `kern_virt_base` (the lowest PT_LOAD VMA), not
    // `entry_point`; cross-check the ELF header here so a link-script or
    // section-order change fails loudly instead of jumping into the wrong
    // address after the page-table switch. `check-layout.sh` also guards
    // this, but the loader is the last place that sees `e_entry`.
    assert_eq!(
        layout.entry_point, layout.kern_virt_base.0,
        "boot-shim: ELF entry does not equal the first PT_LOAD VMA"
    );
    layout_gate(&layout);
    // SAFETY: see `load_segments_into_phys_memory`. Both firmware paths
    // satisfy the contract: physical RAM is identity-mapped at this stage,
    // and the kernel linker script reserves non-overlapping LMA range.
    unsafe { load_segments_into_phys_memory(elf_data.as_slice())? };
    Ok(layout)
}

const ZERO_MODULE: BootModule = BootModule {
    name: "",
    start: PhysBytes(0),
    len: 0,
};

/// Load the boot modules into a caller-owned slot array and return the count.
///
/// `out` must have at least `MODULE_NAMES.len()` slots; the caller commits the
/// returned count to its [`minix_boot::BootStorage`]. Writing into the
/// caller's storage is what removes the `Box::leak` from the UEFI leg (R18
/// D-1): the loader no longer has to hand back a `'static` slice.
pub fn load_boot_modules_into<L: FileLoader>(
    loader: &L,
    alloc_pages: PageAllocator,
    out: &mut [BootModule],
) -> usize {
    assert!(
        out.len() >= MODULE_NAMES.len(),
        "boot-shim: module slot array has {} slots, the contract needs {}",
        out.len(),
        MODULE_NAMES.len()
    );
    for (slot, &name) in out.iter_mut().zip(MODULE_NAMES.iter()) {
        let path = build_module_path(name);
        let data = loader.read(&path).unwrap_or_else(|| {
            panic!(
                "boot-shim: required boot module '{}' not found at {} \
                 (kernel needs all {} in BOOT_MODULE_PROC_NRS order)",
                name,
                path,
                MODULE_NAMES.len()
            )
        });
        let data = data.as_slice();

        let num_pages = (data.len() + 4095) / 4096;
        let phys_base = alloc_pages(num_pages).unwrap_or_else(|| {
            panic!(
                "boot-shim: out of memory loading boot module '{}' ({} pages)",
                name, num_pages
            )
        });

        // SAFETY: `alloc_pages` returned `num_pages` pages of writable
        // physical memory; `data.len() <= num_pages * 4096`.
        unsafe {
            let dest = phys_base as *mut u8;
            dest.copy_from_nonoverlapping(data.as_ptr(), data.len());
            let remaining = num_pages * 4096 - data.len();
            core::ptr::write_bytes(dest.add(data.len()), 0, remaining);
        }

        // `name` 已是 `MODULE_NAMES` 常量表里的 `&'static str`——无需堆分配再泄漏
        // （D01 Round1 D2-08：删掉 clone-to-compile 形）。
        *slot = BootModule {
            name,
            start: PhysBytes(phys_base),
            len: data.len(),
        };
    }
    MODULE_NAMES.len()
}

/// Build "`<MODULES_DIR>/<name>`" without depending on `format!` macros
/// in callers (keeps callers `no_std`-clean and avoids alloc::format
/// across the shared/firmware boundary).
fn build_module_path(name: &str) -> String {
    let mut path = String::with_capacity(MODULES_DIR.len() + 1 + name.len());
    path.push_str(MODULES_DIR);
    path.push('/');
    path.push_str(name);
    path
}

// ── Tests ──
// Pure-logic tests live next to the pure-logic functions. Tests that
// require firmware mocks live in firmware-specific modules.

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use minix_elf::{ELFCLASS64, ELFDATA2LSB, ET_EXEC, PT_LOAD};

    /// Mock loader backed by a small in-memory table.
    struct MockLoader {
        files: Vec<(String, Vec<u8>)>,
    }

    impl MockLoader {
        fn new() -> Self {
            Self { files: Vec::new() }
        }
        fn insert(&mut self, path: &str, data: Vec<u8>) {
            self.files.push((path.to_string(), data));
        }
    }

    impl FileLoader for MockLoader {
        fn read<'a>(&'a self, path: &str) -> Option<FileBytes<'a>> {
            self.files
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, d)| FileBytes::Owned(d.clone()))
        }
    }

    /// Build a minimal valid ELF64 image: one text segment at p_paddr=0x200000.
    fn build_minimal_elf() -> Vec<u8> {
        let mut image = vec![0u8; 0x2000];
        image[0] = 0x7f;
        image[1] = b'E';
        image[2] = b'L';
        image[3] = b'F';
        image[4] = ELFCLASS64;
        image[5] = ELFDATA2LSB;
        image[6] = 1;
        image[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        image[18..20].copy_from_slice(&62u16.to_le_bytes());
        image[20..24].copy_from_slice(&1u32.to_le_bytes());
        image[24..32].copy_from_slice(&0xFFFFFFFF80001000u64.to_le_bytes());
        image[32..40].copy_from_slice(&64u64.to_le_bytes());
        image[52..54].copy_from_slice(&64u16.to_le_bytes());
        image[54..56].copy_from_slice(&56u16.to_le_bytes());
        image[56..58].copy_from_slice(&1u16.to_le_bytes());

        let ph = &mut image[64..120];
        ph[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph[4..8].copy_from_slice(&5u32.to_le_bytes());
        ph[8..16].copy_from_slice(&0x1000u64.to_le_bytes());
        ph[16..24].copy_from_slice(&0xFFFFFFFF80000000u64.to_le_bytes());
        ph[24..32].copy_from_slice(&0x200000u64.to_le_bytes());
        ph[32..40].copy_from_slice(&0x800u64.to_le_bytes());
        ph[40..48].copy_from_slice(&0x800u64.to_le_bytes());
        ph[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        image
    }

    #[test]
    fn test_build_module_path_joins_with_slash() {
        assert_eq!(build_module_path("vm"), "/EFI/minix/modules/vm");
        assert_eq!(build_module_path("memory"), "/EFI/minix/modules/memory");
    }

    #[test]
    fn test_mock_loader_returns_none_for_missing() {
        let loader = MockLoader::new();
        assert!(loader.read("anything").is_none());
    }

    #[test]
    fn test_mock_loader_returns_inserted_bytes() {
        let mut loader = MockLoader::new();
        loader.insert("/foo", vec![1, 2, 3]);
        assert_eq!(
            loader.read("/foo").map(|bytes| bytes.as_slice().to_vec()),
            Some(vec![1, 2, 3])
        );
    }

    #[test]
    fn test_compute_kernel_layout_from_mock_elf() {
        let mut loader = MockLoader::new();
        loader.insert(KERNEL_PATH, build_minimal_elf());

        // We can't perform the unsafe phys-memory copy in unit tests; instead
        // call `compute_kernel_layout` directly through the same code path.
        let elf = loader.read_required(KERNEL_PATH);
        let layout = compute_kernel_layout(elf.as_slice()).unwrap();

        assert_eq!(layout.kern_phys_base, PhysBytes(0x200000));
        assert_eq!(layout.kern_virt_base, VirBytes(0xFFFFFFFF80000000));
        assert_eq!(layout.kern_size, 0x800);
        assert_eq!(layout.entry_point, 0xFFFFFFFF80001000);
    }

    #[test]
    fn test_read_required_panics_on_missing_kernel() {
        let loader = MockLoader::new();
        let result = std::panic::catch_unwind(|| {
            loader.read_required(KERNEL_PATH);
        });
        assert!(result.is_err(), "should panic on missing kernel");
    }

    /// T-9: `compute_kernel_layout` must surface `NoLoadSegments` for an
    /// image with no `PT_LOAD`. Flip the single program header from
    /// `PT_LOAD` to `PT_NULL` so the PT_LOAD iterator yields nothing — the
    /// same `!found_load` path in `compute_kernel_layout` that
    /// `load_kernel_with_loader` reports. Covers the previously-untested
    /// `NoLoadSegments` variant.
    #[test]
    fn test_compute_kernel_layout_rejects_image_without_load_segments() {
        let mut elf = build_minimal_elf();
        elf[64..68].copy_from_slice(&0u32.to_le_bytes()); // p_type = PT_NULL
        assert!(
            matches!(
                compute_kernel_layout(&elf),
                Err(minix_elf::ElfError::NoLoadSegments)
            ),
            "a PT_LOAD-less image must fail with NoLoadSegments"
        );
    }

    /// A file segment larger than its memory image is rejected before any
    /// arithmetic or copy; it would otherwise copy past the declared range.
    #[test]
    fn test_compute_kernel_layout_rejects_filesz_greater_than_memsz() {
        let mut elf = build_minimal_elf();
        elf[64 + 32..64 + 40].copy_from_slice(&0x1000u64.to_le_bytes()); // p_filesz
        elf[64 + 40..64 + 48].copy_from_slice(&0x800u64.to_le_bytes()); // p_memsz
        assert!(
            matches!(
                compute_kernel_layout(&elf),
                Err(minix_elf::ElfError::FileszExceedsMemsz)
            ),
            "filesz > memsz must be rejected with FileszExceedsMemsz \
             (I-48 / D-084 moved the guard into the iterator, which reports \
             the precise variant instead of the caller's generic InvalidSegment)"
        );
    }

    /// `paddr + memsz` overflow must become `InvalidSegment`, not a wrapped
    /// small `kern_size` that later mis-maps the image.
    #[test]
    fn test_compute_kernel_layout_rejects_paddr_memsz_overflow() {
        let mut elf = build_minimal_elf();
        elf[64 + 24..64 + 32].copy_from_slice(&(u64::MAX - 0x10).to_le_bytes()); // p_paddr
        elf[64 + 32..64 + 40].copy_from_slice(&0u64.to_le_bytes()); // p_filesz
        elf[64 + 40..64 + 48].copy_from_slice(&0x100u64.to_le_bytes()); // p_memsz
        assert!(
            matches!(
                compute_kernel_layout(&elf),
                Err(minix_elf::ElfError::InvalidSegment)
            ),
            "paddr + memsz overflow must be rejected with InvalidSegment"
        );
    }

    // ── I-48 / D-084 panel tests: a mid-image malformed PT_LOAD must
    // reject the whole image, not shrink it silently. ──

    /// Build a two-segment ELF whose SECOND PT_LOAD's file bytes extend
    /// past the image end (the classic truncation/corruption shape). The
    /// first segment is fully valid — the old silent-skip behavior would
    /// produce a one-segment layout with a smaller `kern_size` and boot
    /// anyway.
    fn build_two_segment_elf_second_truncated() -> Vec<u8> {
        let mut image = vec![0u8; 0x2000];
        image[0] = 0x7f;
        image[1] = b'E';
        image[2] = b'L';
        image[3] = b'F';
        image[4] = ELFCLASS64;
        image[5] = ELFDATA2LSB;
        image[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        image[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
        image[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
        image[56..58].copy_from_slice(&2u16.to_le_bytes());  // e_phnum = 2

        // Phdr 1: valid — p_offset=0x1000, filesz=0x800, memsz=0x800.
        let ph1 = &mut image[64..120];
        ph1[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph1[8..16].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[24..32].copy_from_slice(&0x200000u64.to_le_bytes());
        ph1[32..40].copy_from_slice(&0x800u64.to_le_bytes());
        ph1[40..48].copy_from_slice(&0x800u64.to_le_bytes());
        ph1[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        // Phdr 2: truncated — p_offset=0x1800 + filesz=0x1000 → 0x2800 >
        // image length 0x2000.
        let ph2 = &mut image[120..176];
        ph2[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph2[8..16].copy_from_slice(&0x1800u64.to_le_bytes());
        ph2[24..32].copy_from_slice(&0x300000u64.to_le_bytes());
        ph2[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        ph2[40..48].copy_from_slice(&0x1000u64.to_le_bytes());
        ph2[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        image
    }

    /// Boot layout consumer: a truncated mid-image segment must bubble the
    /// iterator's error up (a smaller-but-bootable layout is the exact
    /// silent failure I-48 exists to kill).
    #[test]
    fn test_compute_kernel_layout_rejects_truncated_mid_segment() {
        let elf = build_two_segment_elf_second_truncated();
        assert!(
            matches!(
                compute_kernel_layout(&elf),
                Err(minix_elf::ElfError::SegmentOutOfBounds)
            ),
            "truncated second segment must reject the layout, not shrink kern_size"
        );
    }

    /// Physical-memory load consumer: the same image must fail the load
    /// before any segment write.
    #[test]
    fn test_load_segments_into_phys_memory_rejects_truncated_mid_segment() {
        let elf = build_two_segment_elf_second_truncated();
        let result = unsafe { load_segments_into_phys_memory(&elf) };
        assert!(
            matches!(result, Err(minix_elf::ElfError::SegmentOutOfBounds)),
            "truncated second segment must fail the physical load"
        );
    }

    /// Buffer load consumer: the error must propagate as `Err`, not fold
    /// into "`0` segments" (the old `usize` return's blindness).
    #[test]
    fn test_load_segments_into_buffer_propagates_truncated_mid_segment() {
        let elf = build_two_segment_elf_second_truncated();
        let mut buf = vec![0u8; 0x4000];
        let result = load_segments_into_buffer(&elf, &mut buf, 0x200000);
        assert!(
            matches!(result, Err(minix_elf::ElfError::SegmentOutOfBounds)),
            "truncated second segment must propagate as Err, not count as 0 loaded"
        );
    }

    /// A zero-length memory image at a non-zero paddr yields `kern_size == 0`
    /// at this layer; the later boot-validation gate (`kern_size > 0`) is what
    /// rejects it. This also documents why the `checked_sub` guard is
    /// defensive: with at least one segment, `max(paddr + memsz)` cannot be
    /// less than `min(paddr)`.
    #[test]
    fn test_compute_kernel_layout_zero_size_segment_yields_zero_kern_size() {
        let mut elf = build_minimal_elf();
        elf[64 + 32..64 + 40].copy_from_slice(&0u64.to_le_bytes()); // p_filesz
        elf[64 + 40..64 + 48].copy_from_slice(&0u64.to_le_bytes()); // p_memsz
        let layout = compute_kernel_layout(&elf).expect("zero-size PT_LOAD is structurally valid");
        assert_eq!(layout.kern_size, 0);
    }

    /// A two-`PT_LOAD` image whose segments share the same physical range,
    /// built by duplicating `build_minimal_elf`'s single program header into
    /// a second slot and bumping `e_phnum`. The two identical half-open
    /// ranges overlap.
    fn build_overlapping_elf() -> Vec<u8> {
        let mut image = build_minimal_elf();
        image.copy_within(64..120, 120); // phdr[1] = phdr[0]
        image[56..58].copy_from_slice(&2u16.to_le_bytes()); // e_phnum = 2
        image
    }

    /// T-9: the `InvalidSegment` error variant (in `compute_kernel_layout`
    /// and `load_segments_into_phys_memory`) had no test
    /// constructing it. The overlap guard runs *before* the copy loop, so
    /// calling the real loader with overlapping segments returns
    /// `Err(InvalidSegment)` without dereferencing the (unmapped) segment
    /// addresses — safe to invoke in a unit test. The `first_overlapping_pair`
    /// predicate that drives it already has dedicated tests; this one pins
    /// the variant is actually produced by `load_segments_into_phys_memory`.
    #[test]
    fn test_load_segments_into_phys_memory_rejects_overlapping_elf() {
        let elf = build_overlapping_elf();
        let result = unsafe { load_segments_into_phys_memory(&elf) };
        assert!(
            matches!(result, Err(minix_elf::ElfError::InvalidSegment)),
            "two overlapping PT_LOAD segments must fail with InvalidSegment"
        );
    }

    /// D-7 negative control: when a segment's destination LMA overlaps the
    /// ELF source buffer, the loader must stop before the first segment
    /// write. The proof is byte-for-byte: the destination points into the
    /// same allocation as the ELF bytes, so a write that ran before the
    /// overlap gate would leave the buffer changed when the panic unwinds.
    #[test]
    fn source_destination_overlap_stops_before_any_segment_write() {
        let mut elf = build_minimal_elf();
        let base = elf.as_ptr() as u64;
        // The minimal image's file bytes live at [base+0x1000, base+0x1800).
        // A destination at base+0x1400 spans [0x1400, 0x1c00): it crosses the
        // source range inside the same allocation.
        elf[64 + 24..64 + 32].copy_from_slice(&(base + 0x1400).to_le_bytes());
        let before = elf.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            load_segments_into_phys_memory(&elf)
        }));
        assert!(
            result.is_err(),
            "a destination overlapping the ELF source must halt the loader"
        );
        assert_eq!(
            elf, before,
            "the overlap gate must run before the first segment write"
        );
    }

    #[test]
    fn test_bss_region_is_zeroed_after_load() {
        // Construct an ELF whose first PT_LOAD has filesz < memsz.
        // The bytes in [filesz, memsz) must be zero-filled by the loader
        // even though no source bytes exist there (BSS contract).
        let buf_base: u64 = 0x4000;
        let filesz: u64 = 0x80;
        let bss_extra: u64 = 0x180;
        let memsz: u64 = filesz + bss_extra;

        // Craft a minimal ELF with one PT_LOAD at paddr=buf_base.
        let mut elf = vec![0u8; 0x1000];
        elf[0] = 0x7f;
        elf[1] = b'E';
        elf[2] = b'L';
        elf[3] = b'F';
        elf[4] = ELFCLASS64;
        elf[5] = ELFDATA2LSB;
        elf[6] = 1;
        // [16..18] e_type = ET_EXEC
        elf[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        // [18..20] e_machine = EM_X86_64 (62)
        elf[18..20].copy_from_slice(&62u16.to_le_bytes());
        // [24..32] e_entry (unused, but spec-compliant)
        elf[24..32].copy_from_slice(&0u64.to_le_bytes());
        // [32..40] e_phoff = 64
        elf[32..40].copy_from_slice(&64u64.to_le_bytes());
        // [54..56] e_phentsize = 56
        elf[54..56].copy_from_slice(&56u16.to_le_bytes());
        // [56..58] e_phnum = 1
        elf[56..58].copy_from_slice(&1u16.to_le_bytes());

        // ELF header is 64 bytes, phdr table starts at offset 64.
        // We place file bytes right after the phdr (offset 0x80) so
        // that the file's segment bytes don't collide with the header.
        let ph_off = 64;
        let file_bytes_off: u64 = 0x80;
        // p_type at ph_off+0
        elf[ph_off..ph_off + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        // p_flags at ph_off+4: PF_R | PF_X = 5
        elf[ph_off + 4..ph_off + 8].copy_from_slice(&5u32.to_le_bytes());
        // p_offset at ph_off+8: file offset of file bytes
        elf[ph_off + 8..ph_off + 16].copy_from_slice(&file_bytes_off.to_le_bytes());
        // p_vaddr at ph_off+16: kernel sees high address
        elf[ph_off + 16..ph_off + 24].copy_from_slice(&buf_base.to_le_bytes());
        // p_paddr at ph_off+24: physical load address (matches buffer base)
        elf[ph_off + 24..ph_off + 32].copy_from_slice(&buf_base.to_le_bytes());
        // p_filesz at ph_off+32
        elf[ph_off + 32..ph_off + 40].copy_from_slice(&filesz.to_le_bytes());
        // p_memsz at ph_off+40: bigger than filesz by bss_extra
        elf[ph_off + 40..ph_off + 48].copy_from_slice(&memsz.to_le_bytes());
        // p_align at ph_off+48
        elf[ph_off + 48..ph_off + 56].copy_from_slice(&0x1000u64.to_le_bytes());

        // Pre-fill file bytes with 0xAB so we can prove the loader
        // propagates them into [0, filesz) of the buffer unchanged.
        for i in 0..filesz as usize {
            elf[file_bytes_off as usize + i] = 0xAB;
        }

        // Buffer: target physical memory. Pre-fill with 0xCC so we can
        // detect whether the loader's BSS clear reaches every byte in
        // [filesz, memsz). Also keep one byte past memsz as a sentinel
        // to verify the loader does NOT touch bytes beyond the segment.
        let mut buf = vec![0xCCu8; memsz as usize + 1];

        // Run the safe loader against the buffer; it must clear [filesz, memsz).
        let count = load_segments_into_buffer(&elf, &mut buf, buf_base)
            .expect("fixture ELF must load cleanly");
        assert_eq!(count, 1, "exactly one PT_LOAD should be loaded");

        // (a) filesz region preserved: file bytes copied as-is.
        for i in 0..filesz as usize {
            assert_eq!(buf[i], 0xAB, "file bytes must be preserved at offset {i}");
        }
        // (b) BSS region is all zero — this is the contract fix #3 asserts.
        for i in filesz as usize..memsz as usize {
            assert_eq!(
                buf[i], 0x00,
                "BSS byte at offset {i} must be zero, was {}",
                buf[i]
            );
        }
        // (c) bytes past memsz are untouched.
        assert_eq!(buf[memsz as usize], 0xCC, "buf tail must be untouched");
    }

    // Bump allocator backed by a Vec so we never write to real physical
    // memory in unit tests: the returned address points into a leaked Vec
    // kept alive in STORAGE for the duration of the test.
    use std::sync::Mutex;
    static STORAGE: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

    fn mock_alloc(num_pages: usize) -> Option<u64> {
        let mut storage = STORAGE.lock().unwrap();
        let mut buf = vec![0u8; num_pages * 4096];
        let addr = buf.as_mut_ptr() as u64;
        storage.push(buf);
        Some(addr)
    }

    #[test]
    fn test_load_boot_modules_loads_all_in_contract_order() {
        // All 12 contract modules present → they come back in MODULE_NAMES
        // order (the order the kernel maps to BOOT_MODULE_PROC_NRS index).
        let mut loader = MockLoader::new();
        for (i, name) in MODULE_NAMES.iter().enumerate() {
            loader.insert(&build_module_path(name), vec![0x10 + i as u8; 100]);
        }

        let mut slots = [ZERO_MODULE; MODULE_NAMES.len()];
        let n = load_boot_modules_into(&loader, mock_alloc, &mut slots);
        let modules = &slots[..n];
        assert_eq!(modules.len(), MODULE_NAMES.len());
        for (i, name) in MODULE_NAMES.iter().enumerate() {
            assert_eq!(
                modules[i].name, *name,
                "module {i} must keep contract order"
            );
            assert_eq!(modules[i].len, 100);
        }
    }

    #[test]
    fn test_load_boot_modules_panics_on_missing() {
        // The kernel consumes modules positionally and asserts the count is
        // 12, so a missing module must fail fast here — not be silently
        // skipped (the old behaviour, which deferred the error to an opaque
        // kernel assert). Drop the last contract module ("init").
        let mut loader = MockLoader::new();
        for name in &MODULE_NAMES[..MODULE_NAMES.len() - 1] {
            loader.insert(&build_module_path(name), vec![0xAA; 64]);
        }

        let mut slots = [ZERO_MODULE; MODULE_NAMES.len()];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = load_boot_modules_into(&loader, mock_alloc, &mut slots);
        }));
        assert!(
            result.is_err(),
            "missing boot module must panic (fail-fast), not be skipped"
        );
    }

    fn phdr(paddr: u64, memsz: u64) -> minix_elf::LoadSegment {
        minix_elf::LoadSegment {
            offset: 0,
            vaddr: paddr,
            paddr,
            filesz: memsz,
            memsz,
            align: 0x1000,
            flags: 5, // PF_R | PF_X
        }
    }

    #[test]
    fn test_first_overlapping_pair_disjoint() {
        let segs = [
            phdr(0x1000, 0x1000),
            phdr(0x3000, 0x1000),
            phdr(0x5000, 0x1000),
        ];
        assert_eq!(first_overlapping_pair(&segs), None);
    }

    #[test]
    fn test_first_overlapping_pair_overlap() {
        // Segments at 0x1000..0x2000 and 0x1800..0x2800 overlap.
        let segs = [phdr(0x1000, 0x1000), phdr(0x1800, 0x1000)];
        assert_eq!(first_overlapping_pair(&segs), Some((0, 1)));
    }

    #[test]
    fn test_first_overlapping_pair_abutting() {
        // Segments at 0x1000..0x2000 and 0x2000..0x3000 are abutting,
        // not overlapping. Half-open intervals [a, b) and [b, c) don't share.
        let segs = [phdr(0x1000, 0x1000), phdr(0x2000, 0x1000)];
        assert_eq!(first_overlapping_pair(&segs), None);
    }

    #[test]
    fn test_first_source_destination_overlap_detects_cross_segment() {
        // ELF buffer at 0x2ff000. Segment 0 copies to 0x2ff800..0x300100
        // (including its BSS tail); segment 1's file bytes live at
        // 0x300000..0x300010 — inside segment 0's destination span, but not
        // inside segment 1's own. The check must compare every source with
        // every destination.
        let mut seg0 = phdr(0x2ff800, 0x900);
        seg0.offset = 0;
        seg0.filesz = 0x100;
        let mut seg1 = phdr(0x400000, 0x10);
        seg1.offset = 0x1000;
        seg1.filesz = 0x10;
        assert_eq!(
            first_source_destination_overlap(0x2ff000, &[seg0, seg1]),
            Some((1, 0))
        );
    }

    #[test]
    fn test_first_source_destination_overlap_ignores_zero_length() {
        // A zero-length source or destination cannot write or read bytes; it
        // must not be reported as an overlap even when its address sits
        // inside another range.
        // Zero-length source at an address strictly inside the destination:
        // without the skip, the half-open test reports an overlap.
        let mut empty_src = phdr(0x1000, 0x100);
        empty_src.offset = 0x50;
        empty_src.filesz = 0;
        assert_eq!(
            first_source_destination_overlap(0x1000, &[empty_src.clone()]),
            None
        );
        // Zero-length destination strictly inside a live source range.
        let mut live_src = phdr(0x400000, 0x10);
        live_src.offset = 0;
        live_src.filesz = 0x10;
        let mut empty_dst = phdr(0x1008, 0);
        empty_dst.offset = 0;
        empty_dst.filesz = 0;
        assert_eq!(
            first_source_destination_overlap(0x1000, &[live_src, empty_dst]),
            None
        );
    }

    #[test]
    fn test_first_overlapping_pair_ignores_zero_length() {
        // A zero-size segment cannot overwrite anything; it must not be
        // reported as overlapping even when its paddr sits inside another.
        let segs = [phdr(0x1000, 0x1000), phdr(0x1800, 0)];
        assert_eq!(first_overlapping_pair(&segs), None);
        let segs = [phdr(0x1000, 0), phdr(0x1000, 0)];
        assert_eq!(first_overlapping_pair(&segs), None);
    }

    #[test]
    fn test_first_overlapping_pair_returns_first() {
        // Three segments: 0 overlaps 1, 1 overlaps 2 — first is (0, 1).
        let segs = [
            phdr(0x1000, 0x1000),
            phdr(0x1800, 0x1000),
            phdr(0x2000, 0x1000),
        ];
        assert_eq!(first_overlapping_pair(&segs), Some((0, 1)));
    }

    #[test]
    fn test_first_overlapping_pair_empty() {
        let segs: [minix_elf::LoadSegment; 0] = [];
        assert_eq!(first_overlapping_pair(&segs), None);
    }
}
