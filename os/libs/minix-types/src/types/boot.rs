//! Boot image types.
//!
//! Types for boot-time process information, shared across kernel and services.
//!
//! Corresponds to Minix3's `struct boot_image` in `minix/include/minix/type.h`.

use crate::Endpoint;

pub const PROC_NAME_LEN: usize = 16;
pub const NR_BOOT_PROCS: usize = 32;

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
#[derive(Debug, Clone, Copy)]
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
        let len = self.proc_name.iter().position(|&b| b == 0).unwrap_or(PROC_NAME_LEN);
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
