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
/// The explicit enum states ownership: the loader says which side owns
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

/// Parsed kernel ELF: the segment table, the original bytes, and the layout
/// computed from them.
///
/// The bytes and segments travel together so a caller cannot compute a
/// layout from one image and then copy segments out of another. Loading is a
/// separate step behind [`KernelImage::validate`]: only a
/// [`ValidatedKernelImage`] is accepted by the physical-address loader.
pub struct KernelImage<'a> {
    elf_data: &'a [u8],
    segments: Vec<minix_elf::LoadSegment>,
    layout: KernelLoadResult,
}

impl<'a> KernelImage<'a> {
    /// The computed layout (bases, span, entry point).
    pub fn layout(&self) -> KernelLoadResult {
        self.layout
    }

    /// Check the destination-side invariants that do not depend on the
    /// destination memory's contents: no two PT_LOAD destinations overlap,
    /// and no destination span covers any segment's file bytes.
    ///
    /// On success the returned value is the only ticket accepted by
    /// [`load_segments_into_phys_memory`].
    pub fn validate(self) -> Result<ValidatedKernelImage<'a>, minix_elf::ElfError> {
        if first_overlapping_pair(&self.segments).is_some() {
            return Err(minix_elf::ElfError::InvalidSegment);
        }
        // D01-R9: the ELF bytes live in one heap allocation; under the boot
        // identity map the pointer value is physical. Any segment's
        // destination span (including its BSS zero-fill) must stay clear of
        // *every* segment's file bytes, not just its own: a cross-segment
        // overlap silently corrupts the source mid-load.
        let src_base = self.elf_data.as_ptr() as u64;
        if let Some((src_idx, dst_idx)) = first_source_destination_overlap(src_base, &self.segments)
        {
            panic!(
                "boot-shim: kernel ELF segment {src_idx} source overlaps segment {dst_idx} destination LMA"
            );
        }
        Ok(ValidatedKernelImage(self))
    }
}

impl core::ops::Deref for KernelImage<'_> {
    type Target = KernelLoadResult;
    fn deref(&self) -> &Self::Target {
        &self.layout
    }
}

/// A [`KernelImage`] whose destination invariants have been checked.
///
/// The inner value is private: [`KernelImage::validate`] is the only
/// constructor, so handing this type to the loader is evidence that the
/// overlap gates ran.
pub struct ValidatedKernelImage<'a>(KernelImage<'a>);

impl ValidatedKernelImage<'_> {
    /// The computed layout (bases, span, entry point).
    pub fn layout(&self) -> KernelLoadResult {
        self.0.layout
    }
}

impl core::ops::Deref for ValidatedKernelImage<'_> {
    type Target = KernelLoadResult;
    fn deref(&self) -> &Self::Target {
        &self.0.layout
    }
}

/// Parse an ELF image and compute kernel base addresses and size.
///
/// Pure function — does not touch memory. Used by both firmware paths
/// and by unit tests. The returned value owns the parsed segment table and
/// borrows the image bytes; call [`KernelImage::validate`] before loading.
pub fn compute_kernel_layout(elf_data: &[u8]) -> Result<KernelImage<'_>, minix_elf::ElfError> {
    let mut iter = minix_elf::segment_iter(elf_data)?;
    let entry_point = iter.ehdr().e_entry;

    let mut kern_phys_base = u64::MAX;
    let mut kern_virt_base = u64::MAX;
    let mut kern_end_phys = 0u64;
    let mut found_load = false;
    let mut segments = Vec::new();

    while let Some(seg_result) = iter.next() {
        // A malformed program header (truncated segment, unreadable phdr,
        // filesz > memsz) means the image cannot be trusted for a boot
        // layout: the load fails with the precise error instead of
        // producing a smaller-but-bootable layout. The
        // filesz > memsz overwrite guard lives inside the iterator, so
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
        segments.push(seg);
    }

    if !found_load {
        return Err(minix_elf::ElfError::NoLoadSegments);
    }

    let kern_size = kern_end_phys
        .checked_sub(kern_phys_base)
        .ok_or(minix_elf::ElfError::InvalidSegment)?;

    Ok(KernelImage {
        elf_data,
        segments,
        layout: KernelLoadResult {
            kern_phys_base: PhysBytes(kern_phys_base),
            kern_virt_base: VirBytes(kern_virt_base),
            kern_size,
            entry_point,
        },
    })
}

/// File offset one past the last PT_LOAD's file bytes — the runtime file
/// length of an ELF image (kernel or module).
///
/// The ELF trim lives here, the only layer of module loading
/// that legitimately needs ELF semantics. `None` = not an ELF, no PT_LOAD,
/// a malformed program header, or an all-zero file end —
/// the caller keeps the full length: a non-ELF payload must not be
/// shortened by an ELF rule, and a malformed image is the runtime parser's
/// to reject.
fn elf_file_end(data: &[u8]) -> Option<usize> {
    let iter = minix_elf::segment_iter(data).ok()?;
    let mut end = 0usize;
    for seg in iter {
        let seg = seg.ok()?;
        // `Ok` items are iterator-validated (offset+filesz fits inside the
        // image), but `checked_add` keeps this helper honest on its own.
        end = end.max((seg.offset as usize).checked_add(seg.filesz as usize)?);
    }
    if end == 0 {
        return None;
    }
    Some(end.min(data.len()))
}

/// Load PT_LOAD segments from an ELF image into a destination buffer.
///
/// Pure function — used by tests; production code uses
/// [`load_segments_into_phys_memory`] to write directly to physical
/// addresses.
///
/// Follows C's `libexec_load_elf`
/// (`minix3/minix/lib/libexec/exec_elf.c:127`) two-loop shape —
///
/// 1. **Sanity-check loop** (C: "sanity check binary before wiping out
///    the target process", `:160-165`): every segment is collected and
///    checked against the buffer *before any byte is written*. A malformed
///    program header (an `Err` item) or a segment whose destination
///    extent does not fit in `[0, buf.len())` fails the whole load with
///    the precise error — nothing is half-loaded.
/// 2. **Load loop** (C: `clearproc` + per-segment copy/clear): copy file
///    bytes, zero-fill `[filesz, memsz)`, count.
///
/// Returns the number of segments loaded. Shared with the physical-memory
/// loader: segment-shape validation (the iterator), destination/source span
/// derivation (`segment_spans`, including checked arithmetic), BSS
/// zero-fill. Not shared: the physical-address overlap gates and the
/// pointer-width copy (this loader writes a caller-owned buffer at a
/// caller-chosen base; segments below `buf_base_paddr` are skipped as a
/// fixture error).
pub fn load_segments_into_buffer(
    elf_data: &[u8],
    buf: &mut [u8],
    buf_base_paddr: u64,
) -> Result<usize, minix_elf::ElfError> {
    // ── Sanity-check loop: validate everything before wiping any byte. ──
    let mut segs = alloc::vec::Vec::new();
    for seg_result in minix_elf::segment_iter(elf_data)? {
        let seg = seg_result?;
        let spans = segment_spans(&seg)?;
        // A segment below the buffer base is a fixture error, silently
        // skipped rather than relocated to offset 0.
        let Some(dst_off) = spans.dst_start.checked_sub(buf_base_paddr) else {
            continue;
        };
        let dst_off = usize::try_from(dst_off)
            .map_err(|_| minix_elf::ElfError::InvalidSegment)?;
        let file_end = usize::try_from(spans.file_end - buf_base_paddr)
            .map_err(|_| minix_elf::ElfError::InvalidSegment)?;
        let mem_end = usize::try_from(spans.mem_end - buf_base_paddr)
            .map_err(|_| minix_elf::ElfError::InvalidSegment)?;
        if mem_end > buf.len() {
            return Err(minix_elf::ElfError::TargetBufferTooSmall {
                needed_end: mem_end,
                buf_len: buf.len(),
            });
        }
        segs.push((spans, dst_off, file_end, mem_end));
    }

    // ── Load loop: the buffer is proven to hold every segment. ──
    let mut count = 0;
    for (spans, dst_off, file_end, mem_end) in segs {
        let src = &elf_data[spans.src_start..spans.src_end];
        buf[dst_off..file_end].copy_from_slice(src);

        if mem_end > file_end {
            buf[file_end..mem_end].fill(0);
        }

        count += 1;
    }
    Ok(count)
}

/// Destination and source spans for one PT_LOAD segment.
///
/// Shared by both loaders so "where the file bytes go, where the BSS starts,
/// how far the destination reaches" has a single checked derivation. The
/// ELF iterator already rejects `filesz > memsz`; this helper additionally
/// rejects arithmetic overflow instead of letting the copies wrap.
struct SegmentSpans {
    /// Absolute destination start (the segment's physical address).
    dst_start: u64,
    /// First byte after the copied file bytes.
    file_end: u64,
    /// First byte after the whole in-memory extent (copy + BSS).
    mem_end: u64,
    /// Source range inside the ELF image.
    src_start: usize,
    src_end: usize,
}

fn segment_spans(seg: &minix_elf::LoadSegment) -> Result<SegmentSpans, minix_elf::ElfError> {
    let file_end = seg
        .paddr
        .checked_add(seg.filesz)
        .ok_or(minix_elf::ElfError::InvalidSegment)?;
    let mem_end = seg
        .paddr
        .checked_add(seg.memsz)
        .ok_or(minix_elf::ElfError::InvalidSegment)?;
    if mem_end < file_end {
        return Err(minix_elf::ElfError::InvalidSegment);
    }
    let src_start = usize::try_from(seg.offset).map_err(|_| minix_elf::ElfError::InvalidSegment)?;
    let src_len = usize::try_from(seg.filesz).map_err(|_| minix_elf::ElfError::InvalidSegment)?;
    let src_end = src_start
        .checked_add(src_len)
        .ok_or(minix_elf::ElfError::InvalidSegment)?;
    Ok(SegmentSpans {
        dst_start: seg.paddr,
        file_end,
        mem_end,
        src_start,
        src_end,
    })
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
/// Segment-shape validation is built into the iterator: it
/// self-checks `filesz > memsz` (`FileszExceedsMemsz`), phdr-table
/// arithmetic (`PhdrOutOfBounds`) and file bytes past the image end
/// (`SegmentOutOfBounds`), and every consumer — this function included —
/// surfaces those as `Err` before any write. The caller's remaining
/// obligation is the physical one below, not segment-shape validation.
///
/// Overlap detection runs one step earlier: [`KernelImage::validate`]
/// rejects PT_LOAD segments that overlap in physical memory and panics when
/// any segment's file-byte range overlaps any segment's destination span
/// (the copy/BSS zero-fill would otherwise corrupt the source buffer
/// mid-load). This function accepts only a [`ValidatedKernelImage`], so the
/// type itself is the evidence that those gates ran; without them a buggy or
/// maliciously crafted ELF would silently overwrite earlier segments or the
/// heap buffer it is read from.
pub unsafe fn load_segments_into_phys_memory(
    image: &ValidatedKernelImage<'_>,
) -> Result<(), minix_elf::ElfError> {
    for seg in &image.0.segments {
        let spans = segment_spans(seg)?;
        let file_len = usize::try_from(spans.file_end - spans.dst_start)
            .map_err(|_| minix_elf::ElfError::InvalidSegment)?;
        let bss_len = usize::try_from(spans.mem_end - spans.file_end)
            .map_err(|_| minix_elf::ElfError::InvalidSegment)?;
        let dest = spans.dst_start as *mut u8;
        let src = &image.0.elf_data[spans.src_start..spans.src_end];
        unsafe {
            dest.copy_from_nonoverlapping(src.as_ptr(), file_len);
            if bss_len > 0 {
                let bss_start = dest.add(file_len);
                core::ptr::write_bytes(bss_start, 0, bss_len);
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
    let image = compute_kernel_layout(elf_data.as_slice())?;
    // The jump target is `kern_virt_base` (the lowest PT_LOAD VMA), not
    // `entry_point`; cross-check the ELF header here so a link-script or
    // section-order change fails loudly instead of jumping into the wrong
    // address after the page-table switch. `check-layout.sh` also guards
    // this, but the loader is the last place that sees `e_entry`.
    assert_eq!(
        image.entry_point, image.kern_virt_base.0,
        "boot-shim: ELF entry does not equal the first PT_LOAD VMA"
    );
    let image = image.validate()?;
    layout_gate(&image);
    // SAFETY: see `load_segments_into_phys_memory`. Both firmware paths
    // satisfy the contract: physical RAM is identity-mapped at this stage,
    // and the kernel linker script reserves non-overlapping LMA range.
    unsafe { load_segments_into_phys_memory(&image)? };
    Ok(image.layout())
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
/// caller's storage keeps the UEFI leg free of `Box::leak`: the loader
/// hands modules back through the caller's slots, so no `'static` slice is
/// needed.
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

        // Trim to the ELF's runtime file length (the end of the last
        // PT_LOAD's file range) — this is the only layer in module loading
        // that needs ELF semantics, and both legs' inputs converge here:
        // the U-Boot leg's `entry.len` is already the trimmed
        // `staged_elf_used_len` (idempotent here), while the UEFI leg gets
        // the exact file length from firmware (trimmed here), so
        // `BootModule.len` is consistent across legs. A non-ELF or
        // malformed image keeps its full length (fail-safe: the runtime
        // parser will reject it). A check of 36 module ELFs (12 × 3
        // architectures) found no non-NOBITS ALLOC content after the last
        // PT_LOAD file end; only .comment/.symtab/.debug_* are trimmed.
        let used = elf_file_end(data).unwrap_or(data.len());
        let data = &data[..used];

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

        // `name` is already a `&'static str` from the `MODULE_NAMES`
        // constant table — no heap allocation and leak needed.
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
        let result = compute_kernel_layout(&elf).and_then(|image| image.validate().map(|_| ()));
        assert!(
            matches!(result, Err(minix_elf::ElfError::SegmentOutOfBounds)),
            "truncated second segment must fail before the physical load"
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

    // ── Sanity check: the load loop must not write any byte ──

    /// Two-segment ELF where the FIRST segment fits but the SECOND's BSS
    /// extent runs past the buffer. A load loop that writes segment 1 and
    /// silently skips segment 2 leaves a half-loaded image; the ruling
    /// requires the C shape: the whole load fails BEFORE any byte is
    /// written. Proven byte-for-byte: the buffer must be untouched.
    #[test]
    fn test_load_segments_into_buffer_too_small_writes_nothing() {
        let mut elf = build_minimal_elf(); // paddr=0x200000, filesz=0x800, memsz=0x800
        // Widen segment 1's memsz so its BSS extent runs past the buffer.
        // buf will be 0x1000 bytes at base 0x200000; memsz 0x1800 →
        // mem_end = 0x1800 > 0x1000.
        elf[64 + 40..64 + 48].copy_from_slice(&0x1800u64.to_le_bytes()); // p_memsz
        let mut buf = vec![0xC3u8; 0x1000];
        let before = buf.clone();

        let result = load_segments_into_buffer(&elf, &mut buf, 0x200000);
        assert_eq!(
            result,
            Err(minix_elf::ElfError::TargetBufferTooSmall {
                needed_end: 0x1800,
                buf_len: 0x1000,
            }),
            "the over-BSS segment must fail the whole load with the precise variant"
        );
        assert_eq!(buf, before, "sanity-check failure must write zero bytes");
    }

    /// The pass-through case still loads and counts: a buffer that fits
    /// both file bytes and BSS produces count == segment count with correct
    /// content (D-089 ③: count semantics settle automatically once every
    /// segment is proven to fit).
    #[test]
    fn test_load_segments_into_buffer_fitting_load_counts_all() {
        let mut elf = build_minimal_elf();
        elf[64 + 40..64 + 48].copy_from_slice(&0xC00u64.to_le_bytes()); // p_memsz (BSS 0x400)
        // Fill the file-byte region so the copy is distinguishable from the
        // BSS clear (build_minimal_elf's file region is zero by default).
        for byte in elf[0x1000..0x1800].iter_mut() {
            *byte = 0xAB;
        }
        let mut buf = vec![0xC3u8; 0x1000];
        let count = load_segments_into_buffer(&elf, &mut buf, 0x200000)
            .expect("buffer covers file and BSS extent");
        assert_eq!(count, 1);
        assert_eq!(buf[0], 0xAB, "file bytes copied");
        assert_eq!(buf[0x7ff], 0xAB, "file bytes copied (last byte)");
        assert_eq!(buf[0x800], 0, "BSS zero-filled");
        assert_eq!(buf[0xBFF], 0, "BSS zero-filled (last byte)");
        assert_eq!(buf[0xC00], 0xC3, "sentinel past memsz untouched");
    }

    /// F04: the buffer and physical-address loaders must agree byte-for-byte
    /// on the copied file bytes and the zero-filled BSS tail. Both now derive
    /// their spans from `segment_spans`; this test loads the same ELF twice,
    /// once into a buffer at `buf_base_paddr` and once through the
    /// physical-address path whose `p_paddr` is the target allocation's
    /// address. Distinct allocations guarantee the source/destination overlap
    /// gate sees no overlap.
    #[test]
    fn test_buffer_and_phys_loaders_share_segment_spans() {
        let mut elf = build_minimal_elf();
        let mut phys = vec![0xC3u8; 0xC00];
        let phys_base = phys.as_mut_ptr() as u64;
        elf[64 + 24..64 + 32].copy_from_slice(&phys_base.to_le_bytes()); // p_paddr
        elf[64 + 40..64 + 48].copy_from_slice(&0xC00u64.to_le_bytes()); // p_memsz (BSS 0x400)
        for (i, byte) in elf[0x1000..0x1800].iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }

        let mut buffer = vec![0xC3u8; 0xC00];
        let count = load_segments_into_buffer(&elf, &mut buffer, phys_base)
            .expect("buffer covers the whole segment extent");
        assert_eq!(count, 1);

        let image = compute_kernel_layout(&elf)
            .expect("synthetic ELF has a computable layout")
            .validate()
            .expect("distinct allocations keep source and destination disjoint");
        // SAFETY: `phys_base` is the start of the live `phys` allocation above
        // and its extent (`0xC00`) matches the segment's `memsz`; the ELF
        // lives in a different allocation, so the source/destination overlap
        // gate is satisfied.
        unsafe { load_segments_into_phys_memory(&image) }
            .expect("physical loader accepts the same synthetic image");

        assert_eq!(buffer, phys, "both loaders must produce identical bytes");
        assert_eq!(buffer[0x800], 0, "BSS tail zero-filled by both paths");
        assert_eq!(buffer[0xBFF], 0, "BSS tail zero-filled to the last byte");
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
    /// and `KernelImage::validate`) had no test constructing it. The overlap
    /// guard runs before any copy, so validating an image with overlapping
    /// segments returns `Err(InvalidSegment)` without dereferencing the
    /// (unmapped) segment addresses — safe to invoke in a unit test. The
    /// `first_overlapping_pair` predicate that drives it already has
    /// dedicated tests; this one pins the variant is actually produced on the
    /// physical-load path.
    #[test]
    fn test_load_segments_into_phys_memory_rejects_overlapping_elf() {
        let elf = build_overlapping_elf();
        let result = compute_kernel_layout(&elf)
            .expect("the ELF iterator accepts two structurally valid segments")
            .validate();
        assert!(
            matches!(result, Err(minix_elf::ElfError::InvalidSegment)),
            "two overlapping PT_LOAD segments must fail validation with InvalidSegment"
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
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compute_kernel_layout(&elf)
                .expect("the image is structurally valid")
                .validate()
        }));
        assert!(
            result.is_err(),
            "a destination overlapping the ELF source must halt validation"
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
            // Non-ELF payloads keep the full length (I-36 / D-082: the ELF
            // trim must not shorten a non-ELF module).
            assert_eq!(modules[i].len, 100);
        }
    }

    // ── I-36 / D-082: the ELF trim lives in the module-loading layer ──

    /// `elf_file_end` returns one past the last PT_LOAD's file bytes.
    /// `build_minimal_elf`: single PT_LOAD at p_offset=0x1000, filesz=0x800.
    #[test]
    fn test_elf_file_end_returns_last_load_file_end() {
        let elf = build_minimal_elf();
        assert_eq!(elf_file_end(&elf), Some(0x1800));
    }

    /// Non-ELF bytes → `None` → the caller keeps the full length.
    #[test]
    fn test_elf_file_end_non_elf_is_none() {
        assert_eq!(elf_file_end(&[0x42u8; 100]), None);
    }

    /// A malformed program header (I-48 `Err` item) makes the trim
    /// untrustworthy → `None` → full length; the runtime parser rejects it.
    #[test]
    fn test_elf_file_end_malformed_segment_is_none() {
        let elf = build_two_segment_elf_second_truncated();
        assert_eq!(elf_file_end(&elf), None);
    }

    /// `load_boot_modules_into` trims an ELF module carrying trailing
    /// non-loaded bytes down to the last PT_LOAD's file end — the UEFI
    /// leg's new behavior (its `read` returns the exact firmware length,
    /// which includes .comment/.symtab tails). A non-ELF sibling module
    /// keeps its full length.
    #[test]
    fn test_load_boot_modules_into_trims_elf_to_file_end() {
        let mut elf = build_minimal_elf(); // runtime content ends at 0x1800
        elf.extend_from_slice(&[0xDDu8; 0x100]); // non-loaded trailing bytes
        let mut loader = MockLoader::new();
        loader.insert(&build_module_path("ds"), elf);
        for (i, name) in MODULE_NAMES.iter().enumerate() {
            if *name != "ds" {
                loader.insert(&build_module_path(name), vec![0x10 + i as u8; 100]);
            }
        }

        let mut slots = [ZERO_MODULE; MODULE_NAMES.len()];
        let n = load_boot_modules_into(&loader, mock_alloc, &mut slots);
        let modules = &slots[..n];
        assert_eq!(modules.len(), MODULE_NAMES.len());
        assert_eq!(
            modules[0].len, 0x1800,
            "ELF module must be trimmed to the last PT_LOAD's file end"
        );
        assert_eq!(
            modules[1].len, 100,
            "non-ELF module must keep its full length"
        );
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
