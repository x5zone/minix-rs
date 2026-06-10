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
//! - U-Boot pre-loads files into RAM with `fatload` and hands the boot-shim
//!   a `BootFileTable` describing where each file lives.
//!
//! We capture that difference in the [`FileLoader`] trait, and then write
//! the loading logic **once**, generically over `L: FileLoader`. Both
//! firmware paths invoke the exact same code path; only the loader value
//! differs.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::BootModule;

/// Path to the kernel ELF (same convention for UEFI ESP and U-Boot FAT).
///
/// Uses forward slashes as the canonical format. The `UefiFileLoader`
/// converts to backslashes before passing to UEFI's `SimpleFileSystem`.
pub const KERNEL_PATH: &str = "/EFI/minix/kernel.elf";

/// Directory containing boot modules (same convention for both paths).
pub const MODULES_DIR: &str = "/EFI/minix/modules";

/// Boot modules to load, in order. Names match files under `MODULES_DIR`.
pub const MODULE_NAMES: &[&str] = &["vm", "pm", "vfs", "rs", "ds", "inet"];

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

/// Firmware-agnostic file access.
///
/// Implementations:
/// - `UefiFileLoader` — calls `SimpleFileSystem::read()` via the `uefi` crate.
/// - `UbootFileLoader` — looks up pre-loaded files in a `BootFileTable`
///   placed in physical memory by U-Boot's `fatload` commands.
///
/// Both implementations return owned `Vec<u8>` so the caller (the shared
/// loader code) can process the bytes without caring about the data source.
///
/// `read` returns `None` when the file is absent (non-fatal for optional
/// modules) or unreadable. Callers use `read_required` to make absence
/// fatal (e.g. for the kernel itself).
pub trait FileLoader {
    /// Read a file by path. Returns `None` if the file does not exist or
    /// cannot be read.
    fn read(&self, path: &str) -> Option<Vec<u8>>;

    /// Read a file that must exist. Default panics with a useful message.
    fn read_required(&self, path: &str) -> Vec<u8> {
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

    while let Some(seg) = iter.next() {
        found_load = true;
        kern_phys_base = kern_phys_base.min(seg.paddr);
        kern_virt_base = kern_virt_base.min(seg.vaddr);
        kern_end_phys = kern_end_phys.max(seg.paddr + seg.memsz);
    }

    if !found_load {
        return Err(minix_elf::ElfError::NoLoadSegments);
    }

    let kern_size = kern_end_phys - kern_phys_base;

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
/// Returns the number of segments loaded.
pub fn load_segments_into_buffer(elf_data: &[u8], buf: &mut [u8], buf_base_paddr: u64) -> usize {
    let iter = match minix_elf::segment_iter(elf_data) {
        Ok(i) => i,
        Err(_) => return 0,
    };

    let mut count = 0;
    for seg in iter {
        let seg_offset_in_buf = seg.paddr.checked_sub(buf_base_paddr).unwrap_or(0) as usize;
        let file_end = seg_offset_in_buf + seg.filesz as usize;
        let mem_end = seg_offset_in_buf + seg.memsz as usize;

        if file_end <= buf.len() {
            let src = &elf_data[seg.offset as usize..(seg.offset + seg.filesz) as usize];
            buf[seg_offset_in_buf..file_end].copy_from_slice(src);
        }

        if mem_end <= buf.len() && seg.memsz > seg.filesz {
            buf[file_end..mem_end].fill(0);
        }

        count += 1;
    }
    count
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
pub unsafe fn load_segments_into_phys_memory(
    elf_data: &[u8],
) -> Result<(), minix_elf::ElfError> {
    let iter = minix_elf::segment_iter(elf_data)?;
    for seg in iter {
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

/// Load the kernel ELF: read bytes via `loader`, compute layout, copy segments.
///
/// This is the shared kernel-loading path used by both UEFI and OpenSBI.
/// All firmware-specific concerns (where the bytes come from) are
/// encapsulated in the `FileLoader` argument.
pub fn load_kernel_with_loader<L: FileLoader>(
    loader: &L,
) -> Result<KernelLoadResult, minix_elf::ElfError> {
    let elf_data = loader.read_required(KERNEL_PATH);
    let layout = compute_kernel_layout(&elf_data)?;
    // SAFETY: see `load_segments_into_phys_memory`. Both firmware paths
    // satisfy the contract: physical RAM is identity-mapped at this stage,
    // and the kernel linker script reserves non-overlapping LMA range.
    unsafe { load_segments_into_phys_memory(&elf_data)? };
    Ok(layout)
}

/// Read each named module, copy bytes into pages from `alloc_pages`,
/// and return a leaked `'static` slice of `BootModule`.
///
/// Modules that fail to read or allocate are skipped silently — this
/// matches the Minix3 GRUB behaviour where missing modules simply don't
/// appear in `kinfo.module_list`.
pub fn load_boot_modules_with_loader<L: FileLoader>(
    loader: &L,
    alloc_pages: PageAllocator,
) -> &'static [BootModule] {
    let mut modules: Vec<BootModule> = Vec::new();

    for &name in MODULE_NAMES {
        let path = build_module_path(name);
        let data = match loader.read(&path) {
            Some(d) => d,
            None => continue,
        };

        let num_pages = (data.len() + 4095) / 4096;
        let phys_base = match alloc_pages(num_pages) {
            Some(p) => p,
            None => continue,
        };

        // SAFETY: `alloc_pages` returned `num_pages` pages of writable
        // physical memory; `data.len() <= num_pages * 4096`.
        unsafe {
            let dest = phys_base as *mut u8;
            dest.copy_from_nonoverlapping(data.as_ptr(), data.len());
            let remaining = num_pages * 4096 - data.len();
            core::ptr::write_bytes(dest.add(data.len()), 0, remaining);
        }

        let name_static: &'static str = Box::leak(name.to_string().into_boxed_str());
        modules.push(BootModule {
            name: name_static,
            start: PhysBytes(phys_base),
            len: data.len(),
        });
    }

    Box::leak(modules.into_boxed_slice())
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
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.files
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, d)| d.clone())
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
        assert_eq!(build_module_path("inet"), "/EFI/minix/modules/inet");
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
        assert_eq!(loader.read("/foo"), Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_load_kernel_with_loader_computes_layout() {
        let mut loader = MockLoader::new();
        loader.insert(KERNEL_PATH, build_minimal_elf());

        // We can't perform the unsafe phys-memory copy in unit tests; instead
        // call `compute_kernel_layout` directly through the same code path.
        let elf = loader.read_required(KERNEL_PATH);
        let layout = compute_kernel_layout(&elf).unwrap();

        assert_eq!(layout.kern_phys_base, PhysBytes(0x200000));
        assert_eq!(layout.kern_virt_base, VirBytes(0xFFFFFFFF80000000));
        assert_eq!(layout.kern_size, 0x800);
        assert_eq!(layout.entry_point, 0xFFFFFFFF80001000);
    }

    #[test]
    fn test_load_kernel_with_loader_panics_on_missing_kernel() {
        let loader = MockLoader::new();
        let result = std::panic::catch_unwind(|| {
            loader.read_required(KERNEL_PATH);
        });
        assert!(result.is_err(), "should panic on missing kernel");
    }

    #[test]
    fn test_load_boot_modules_with_loader_skips_missing() {
        // Bump allocator backed by a Vec so we don't write to real phys memory.
        // We allocate large enough that addresses are non-overlapping but
        // dummies. To avoid an actual phys write we provide an allocator
        // whose returned address points into a leaked Vec.
        use std::sync::Mutex;
        static STORAGE: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

        fn alloc(num_pages: usize) -> Option<u64> {
            let mut storage = STORAGE.lock().unwrap();
            let mut buf = vec![0u8; num_pages * 4096];
            let addr = buf.as_mut_ptr() as u64;
            storage.push(buf);
            Some(addr)
        }

        let mut loader = MockLoader::new();
        loader.insert("/EFI/minix/modules/vm", vec![0xAA; 100]);
        loader.insert("/EFI/minix/modules/pm", vec![0xBB; 200]);
        // vfs/rs/ds/inet missing → must be skipped, not panic.

        let modules = load_boot_modules_with_loader(&loader, alloc);
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].name, "vm");
        assert_eq!(modules[0].len, 100);
        assert_eq!(modules[1].name, "pm");
        assert_eq!(modules[1].len, 200);
    }
}
