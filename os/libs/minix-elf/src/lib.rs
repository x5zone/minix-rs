//! Minimal ELF64 parser for Minix-RS.
//!
//! Parses ELF64 headers and program headers to extract PT_LOAD segments.
//! Shared by boot-shim (loading kernel from ESP) and kernel (loading boot modules).
//!
//! This parser is intentionally minimal — it only supports what Minix-RS needs:
//! - ELF64 little-endian (x86-64, aarch64, riscv64)
//! - Program headers of type PT_LOAD
//! - No section header parsing, no symbol resolution, no relocation
//!
//! # Why not use `xmas-elf` or `object`?
//!
//! - `xmas-elf` returns `&str` error messages (requires `alloc` or formatting)
//! - `object` pulls in `std` dependencies
//! - Both are over-engineered for our single use case:
//!   "read PT_LOAD segments, copy them to the right physical addresses"
//!
//! # Zero-allocation design
//!
//! This parser uses an iterator (`SegmentIter`) instead of returning a `Vec`.
//! Boot-shim runs before the kernel heap is available; the only allocator
//! available is UEFI's `AllocatePages` (page-granularity). The kernel itself
//! may use this parser early in boot before its own heap is ready. An iterator
//! lets the caller process segments one-by-one without collecting them.
//!
//! # Safety
//!
//! All parsing uses safe slice access with bounds checking.
//! The only `unsafe` is the caller's responsibility: ensuring that
//! the ELF image bytes are valid and that destination addresses
//! are writable physical memory.

#![no_std]
#![cfg_attr(test, allow(internal_features))]
#![allow(clippy::identity_op)]

#[cfg(test)]
extern crate std;

// ── ELF64 constants ──

/// ELF magic: `\x7fELF`
pub const ELFMAG: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class: 64-bit
pub const ELFCLASS64: u8 = 2;

/// ELF data: little-endian
pub const ELFDATA2LSB: u8 = 1;

/// ELF type: executable
pub const ET_EXEC: u16 = 2;

/// Program header type: loadable segment
pub const PT_LOAD: u32 = 1;

// ── ELF64 header sizes ──

/// ELF64 Ehdr size: 64 bytes
const ELF64_EHDR_SIZE: usize = 64;

/// ELF64 Phdr size: 56 bytes
const ELF64_PHDR_SIZE: usize = 56;

// ── ELF64 structures (read from byte slices, no padding issues) ──

/// Parsed ELF64 file header.
///
/// Only contains fields needed by Minix-RS.
/// Source: ELF-64 Object File Format, System V ABI, Figure 4-2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elf64Ehdr {
    /// e_type — object file type (must be ET_EXEC)
    pub e_type: u16,
    /// e_machine — architecture (EM_X86_64=62, EM_AARCH64=183, EM_RISCV=243)
    pub e_machine: u16,
    /// e_entry — virtual address of entry point
    pub e_entry: u64,
    /// e_phoff — program header table file offset
    pub e_phoff: u64,
    /// e_phentsize — program header table entry size
    pub e_phentsize: u16,
    /// e_phnum — program header table entry count
    pub e_phnum: u16,
}

/// Parsed ELF64 program header.
///
/// Only contains fields needed by Minix-RS.
/// Source: ELF-64 Object File Format, System V ABI, Figure 4-5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elf64Phdr {
    /// p_type — segment type (PT_LOAD for loadable segments)
    pub p_type: u32,
    /// p_flags — segment flags (PF_R=4, PF_W=2, PF_X=1)
    pub p_flags: u32,
    /// p_offset — segment file offset
    pub p_offset: u64,
    /// p_vaddr — segment virtual address
    pub p_vaddr: u64,
    /// p_paddr — segment physical address (relevant for boot loading)
    pub p_paddr: u64,
    /// p_filesz — segment size in file
    pub p_filesz: u64,
    /// p_memsz — segment size in memory (≥ p_filesz; BSS = memsz - filesz)
    pub p_memsz: u64,
    /// p_align — segment alignment (0 or 1 means no alignment constraint)
    pub p_align: u64,
}

/// A loadable segment extracted from an ELF binary.
///
/// This is the output that callers actually use:
/// copy `file_data[offset..offset+filesz]` to physical address `paddr`,
/// then zero-fill `[paddr+filesz, paddr+memsz)` for BSS.
///
/// The `vaddr` field is needed by the kernel's page table setup to create
/// the correct virtual-to-physical mappings (kern_virt_base → kern_phys_base).
/// The `flags` field carries segment permissions (PF_R|PF_W|PF_X) for
/// setting page table entry attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSegment {
    /// Physical address where this segment should be loaded.
    pub paddr: u64,
    /// Virtual address where this segment expects to run.
    pub vaddr: u64,
    /// File offset of segment data.
    pub offset: u64,
    /// Number of bytes to copy from file.
    pub filesz: u64,
    /// Total memory size (filesz + BSS).
    pub memsz: u64,
    /// Segment alignment.
    pub align: u64,
    /// Segment flags (PF_R=4, PF_W=2, PF_X=1).
    pub flags: u32,
}

/// Errors that can occur during ELF parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    /// Image is too short to contain an ELF header.
    TooShort,
    /// Magic bytes don't match `\x7fELF`.
    BadMagic,
    /// Not a 64-bit ELF (e_ident[EI_CLASS] != ELFCLASS64).
    Not64Bit,
    /// Not little-endian (e_ident[EI_DATA] != ELFDATA2LSB).
    NotLittleEndian,
    /// Not an executable (e_type != ET_EXEC).
    NotExecutable,
    /// Program header offset + size exceeds image bounds.
    PhdrOutOfBounds,
    /// Program header entry size is too small.
    PhdrEntryTooSmall,
    /// No PT_LOAD segments found.
    NoLoadSegments,
}

// ── Parsing functions ──

/// Parse the ELF64 file header from a byte slice.
///
/// Validates magic, class, endianness, and type.
/// Returns the parsed header or an error.
pub fn parse_ehdr(image: &[u8]) -> Result<Elf64Ehdr, ElfError> {
    if image.len() < ELF64_EHDR_SIZE {
        return Err(ElfError::TooShort);
    }

    // Validate magic
    if image[0..4] != ELFMAG {
        return Err(ElfError::BadMagic);
    }

    // e_ident[4] = EI_CLASS
    if image[4] != ELFCLASS64 {
        return Err(ElfError::Not64Bit);
    }

    // e_ident[5] = EI_DATA
    if image[5] != ELFDATA2LSB {
        return Err(ElfError::NotLittleEndian);
    }

    // e_type at offset 16 (2 bytes, little-endian)
    let e_type = u16::from_le_bytes([image[16], image[17]]);
    if e_type != ET_EXEC {
        return Err(ElfError::NotExecutable);
    }

    // e_machine at offset 18
    let e_machine = u16::from_le_bytes([image[18], image[19]]);

    // e_entry at offset 24
    let e_entry = u64::from_le_bytes([
        image[24], image[25], image[26], image[27],
        image[28], image[29], image[30], image[31],
    ]);

    // e_phoff at offset 32
    let e_phoff = u64::from_le_bytes([
        image[32], image[33], image[34], image[35],
        image[36], image[37], image[38], image[39],
    ]);

    // e_phentsize at offset 54
    let e_phentsize = u16::from_le_bytes([image[54], image[55]]);

    // e_phnum at offset 56
    let e_phnum = u16::from_le_bytes([image[56], image[57]]);

    Ok(Elf64Ehdr {
        e_type,
        e_machine,
        e_entry,
        e_phoff,
        e_phentsize,
        e_phnum,
    })
}

/// Parse a single program header from a byte slice.
///
/// The `offset` is the byte offset within `image` where this Phdr starts.
/// The `entsize` is the declared entry size (must be >= 56 bytes for ELF64).
fn parse_phdr(image: &[u8], offset: usize, entsize: u16) -> Result<Elf64Phdr, ElfError> {
    if (entsize as usize) < ELF64_PHDR_SIZE {
        return Err(ElfError::PhdrEntryTooSmall);
    }
    if offset + (entsize as usize) > image.len() {
        return Err(ElfError::PhdrOutOfBounds);
    }

    let d = &image[offset..];

    // ELF64 Phdr layout (56 bytes):
    //   offset 0:  p_type   (u32)
    //   offset 4:  p_flags  (u32)
    //   offset 8:  p_offset (u64)
    //   offset 16: p_vaddr  (u64)
    //   offset 24: p_paddr  (u64)
    //   offset 32: p_filesz (u64)
    //   offset 40: p_memsz  (u64)
    //   offset 48: p_align  (u64)
    let p_type = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
    let p_flags = u32::from_le_bytes([d[4], d[5], d[6], d[7]]);
    let p_offset = u64::from_le_bytes([
        d[8], d[9], d[10], d[11], d[12], d[13], d[14], d[15],
    ]);
    let p_vaddr = u64::from_le_bytes([
        d[16], d[17], d[18], d[19], d[20], d[21], d[22], d[23],
    ]);
    let p_paddr = u64::from_le_bytes([
        d[24], d[25], d[26], d[27], d[28], d[29], d[30], d[31],
    ]);
    let p_filesz = u64::from_le_bytes([
        d[32], d[33], d[34], d[35], d[36], d[37], d[38], d[39],
    ]);
    let p_memsz = u64::from_le_bytes([
        d[40], d[41], d[42], d[43], d[44], d[45], d[46], d[47],
    ]);
    let p_align = u64::from_le_bytes([
        d[48], d[49], d[50], d[51], d[52], d[53], d[54], d[55],
    ]);

    Ok(Elf64Phdr {
        p_type,
        p_flags,
        p_offset,
        p_vaddr,
        p_paddr,
        p_filesz,
        p_memsz,
        p_align,
    })
}

// ── Segment iterator (zero-allocation) ──

/// Iterator over PT_LOAD segments in an ELF64 image.
///
/// Created by [`segment_iter`]. Yields `LoadSegment` values in program
/// header order (not sorted by paddr — the caller can sort if needed).
///
/// # Example
///
/// ```ignore
/// let iter = minix_elf::segment_iter(&image)?;
/// for seg in iter {
///     // copy image[seg.offset..seg.offset+seg.filesz] to seg.paddr
///     // zero-fill [seg.paddr+seg.filesz, seg.paddr+seg.memsz)
/// }
/// ```
pub struct SegmentIter<'a> {
    image: &'a [u8],
    ehdr: Elf64Ehdr,
    /// Current program header index (0..e_phnum)
    index: u16,
}

impl<'a> SegmentIter<'a> {
    /// Create a new segment iterator. Validates the ELF header and
    /// program header table bounds.
    pub fn new(image: &'a [u8]) -> Result<Self, ElfError> {
        let ehdr = parse_ehdr(image)?;

        // Validate program header entry size
        if (ehdr.e_phentsize as usize) < ELF64_PHDR_SIZE {
            return Err(ElfError::PhdrEntryTooSmall);
        }

        // Validate program header table fits within image
        let phdr_end = ehdr.e_phoff as usize
            + (ehdr.e_phnum as usize) * (ehdr.e_phentsize as usize);
        if phdr_end > image.len() {
            return Err(ElfError::PhdrOutOfBounds);
        }

        Ok(Self {
            image,
            ehdr,
            index: 0,
        })
    }

    /// Return the parsed ELF header.
    pub fn ehdr(&self) -> &Elf64Ehdr {
        &self.ehdr
    }
}

impl<'a> Iterator for SegmentIter<'a> {
    type Item = LoadSegment;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.ehdr.e_phnum {
            let offset = self.ehdr.e_phoff as usize
                + (self.index as usize) * (self.ehdr.e_phentsize as usize);
            self.index += 1;

            // parse_phdr already validated bounds in new(), but be defensive
            let phdr = match parse_phdr(self.image, offset, self.ehdr.e_phentsize) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if phdr.p_type != PT_LOAD {
                continue;
            }

            // Validate segment data fits within image
            let seg_end = phdr.p_offset as usize + phdr.p_filesz as usize;
            if seg_end > self.image.len() {
                continue;
            }

            return Some(LoadSegment {
                paddr: phdr.p_paddr,
                vaddr: phdr.p_vaddr,
                offset: phdr.p_offset,
                filesz: phdr.p_filesz,
                memsz: phdr.p_memsz,
                align: phdr.p_align,
                flags: phdr.p_flags,
            });
        }
        None
    }
}

/// Create an iterator over PT_LOAD segments in an ELF64 image.
///
/// Convenience wrapper around `SegmentIter::new`.
/// The caller should:
/// 1. Copy `image[seg.offset..seg.offset+seg.filesz]` to `seg.paddr`
/// 2. Zero-fill `[seg.paddr+seg.filesz, seg.paddr+seg.memsz)` for BSS
///
/// # Errors
///
/// Returns `ElfError` if the image is malformed.
pub fn segment_iter(image: &[u8]) -> Result<SegmentIter<'_>, ElfError> {
    SegmentIter::new(image)
}

/// Get the entry point virtual address from an ELF64 image.
///
/// Convenience wrapper around `parse_ehdr` that only returns the entry point.
pub fn entry_point(image: &[u8]) -> Result<u64, ElfError> {
    let ehdr = parse_ehdr(image)?;
    Ok(ehdr.e_entry)
}

/// Get the architecture machine type from an ELF64 image.
///
/// Returns the `e_machine` field (EM_X86_64=62, EM_AARCH64=183, EM_RISCV=243).
pub fn machine_type(image: &[u8]) -> Result<u16, ElfError> {
    let ehdr = parse_ehdr(image)?;
    Ok(ehdr.e_machine)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Maximum number of PT_LOAD segments we expect in test ELFs.
    const MAX_TEST_SEGMENTS: usize = 4;

    /// Collect up to N segments from the iterator into a fixed-size array.
    /// Returns the number of segments found.
    fn collect_segments(iter: &mut SegmentIter<'_>, buf: &mut [LoadSegment; MAX_TEST_SEGMENTS]) -> usize {
        let mut count = 0;
        while let Some(seg) = iter.next() {
            if count < buf.len() {
                buf[count] = seg;
            }
            count += 1;
        }
        count
    }

    /// Build a minimal valid ELF64 image for testing.
    ///
    /// Contains one PT_LOAD segment:
    ///   p_offset=0x1000, p_vaddr=0xFFFFFFFF80000000, p_paddr=0x200000,
    ///   p_filesz=0x1000, p_memsz=0x2000, p_align=0x1000
    fn build_test_elf() -> [u8; 0x2000] {
        let mut image = [0u8; 0x2000];

        // ── ELF header (64 bytes) ──
        image[0] = 0x7f;
        image[1] = b'E';
        image[2] = b'L';
        image[3] = b'F';
        image[4] = ELFCLASS64;
        image[5] = ELFDATA2LSB;
        image[6] = 1; // EI_VERSION
        image[7] = 0; // EI_OSABI

        image[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        image[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
        image[20..24].copy_from_slice(&1u32.to_le_bytes());  // e_version
        image[24..32].copy_from_slice(&0xFFFFFFFF80001000u64.to_le_bytes());
        image[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
        image[40..48].copy_from_slice(&0u64.to_le_bytes());  // e_shoff
        image[48..52].copy_from_slice(&0u32.to_le_bytes());  // e_flags
        image[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
        image[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
        image[56..58].copy_from_slice(&1u16.to_le_bytes());  // e_phnum

        // ── Program header at offset 64 (56 bytes) ──
        let ph = &mut image[64..120];
        ph[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph[4..8].copy_from_slice(&5u32.to_le_bytes());       // PF_R|PF_X
        ph[8..16].copy_from_slice(&0x1000u64.to_le_bytes());
        ph[16..24].copy_from_slice(&0xFFFFFFFF80000000u64.to_le_bytes());
        ph[24..32].copy_from_slice(&0x200000u64.to_le_bytes());
        ph[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        ph[40..48].copy_from_slice(&0x2000u64.to_le_bytes());
        ph[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        image
    }

    #[test]
    fn test_parse_ehdr_valid() {
        let image = build_test_elf();
        let ehdr = parse_ehdr(&image).unwrap();
        assert_eq!(ehdr.e_type, ET_EXEC);
        assert_eq!(ehdr.e_machine, 62);
        assert_eq!(ehdr.e_entry, 0xFFFFFFFF80001000);
        assert_eq!(ehdr.e_phoff, 64);
        assert_eq!(ehdr.e_phentsize, 56);
        assert_eq!(ehdr.e_phnum, 1);
    }

    #[test]
    fn test_parse_ehdr_bad_magic() {
        let mut image = build_test_elf();
        image[0] = 0x00;
        assert_eq!(parse_ehdr(&image), Err(ElfError::BadMagic));
    }

    #[test]
    fn test_parse_ehdr_too_short() {
        let image = [0u8; 10];
        assert_eq!(parse_ehdr(&image), Err(ElfError::TooShort));
    }

    #[test]
    fn test_parse_ehdr_not_64bit() {
        let mut image = build_test_elf();
        image[4] = 1; // ELFCLASS32
        assert_eq!(parse_ehdr(&image), Err(ElfError::Not64Bit));
    }

    #[test]
    fn test_parse_ehdr_not_le() {
        let mut image = build_test_elf();
        image[5] = 2; // ELFDATA2MSB
        assert_eq!(parse_ehdr(&image), Err(ElfError::NotLittleEndian));
    }

    #[test]
    fn test_segment_iter_valid() {
        let image = build_test_elf();
        let mut buf = [LoadSegment { paddr: 0, vaddr: 0, offset: 0, filesz: 0, memsz: 0, align: 0, flags: 0 }; MAX_TEST_SEGMENTS];
        let count = collect_segments(&mut segment_iter(&image).unwrap(), &mut buf);
        assert_eq!(count, 1);
        assert_eq!(buf[0].paddr, 0x200000);
        assert_eq!(buf[0].vaddr, 0xFFFFFFFF80000000);
        assert_eq!(buf[0].offset, 0x1000);
        assert_eq!(buf[0].filesz, 0x1000);
        assert_eq!(buf[0].memsz, 0x2000);
        assert_eq!(buf[0].align, 0x1000);
        assert_eq!(buf[0].flags, 5);
    }

    #[test]
    fn test_segment_iter_no_pt_load() {
        let mut image = build_test_elf();
        image[64..68].copy_from_slice(&0u32.to_le_bytes()); // PT_NULL
        let mut buf = [LoadSegment { paddr: 0, vaddr: 0, offset: 0, filesz: 0, memsz: 0, align: 0, flags: 0 }; MAX_TEST_SEGMENTS];
        let count = collect_segments(&mut segment_iter(&image).unwrap(), &mut buf);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_entry_point() {
        let image = build_test_elf();
        assert_eq!(entry_point(&image).unwrap(), 0xFFFFFFFF80001000);
    }

    #[test]
    fn test_machine_type() {
        let image = build_test_elf();
        assert_eq!(machine_type(&image).unwrap(), 62);
    }

    #[test]
    fn test_segment_iter_two_segments() {
        let mut image = [0u8; 0x4000];

        // ELF header
        image[0] = 0x7f; image[1] = b'E'; image[2] = b'L'; image[3] = b'F';
        image[4] = ELFCLASS64;
        image[5] = ELFDATA2LSB;
        image[6] = 1;
        image[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        image[18..20].copy_from_slice(&62u16.to_le_bytes());
        image[20..24].copy_from_slice(&1u32.to_le_bytes());
        image[24..32].copy_from_slice(&0xFFFFFFFF80001000u64.to_le_bytes());
        image[32..40].copy_from_slice(&64u64.to_le_bytes());
        image[40..48].copy_from_slice(&0u64.to_le_bytes());
        image[48..52].copy_from_slice(&0u32.to_le_bytes());
        image[52..54].copy_from_slice(&64u16.to_le_bytes());
        image[54..56].copy_from_slice(&56u16.to_le_bytes());
        image[56..58].copy_from_slice(&2u16.to_le_bytes()); // e_phnum = 2

        // Phdr 1: paddr=0x400000
        let ph1 = &mut image[64..120];
        ph1[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph1[4..8].copy_from_slice(&5u32.to_le_bytes());
        ph1[8..16].copy_from_slice(&0x2000u64.to_le_bytes());
        ph1[16..24].copy_from_slice(&0xFFFFFFFF80200000u64.to_le_bytes());
        ph1[24..32].copy_from_slice(&0x400000u64.to_le_bytes());
        ph1[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[40..48].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        // Phdr 2: paddr=0x200000
        let ph2 = &mut image[120..176];
        ph2[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph2[4..8].copy_from_slice(&5u32.to_le_bytes());
        ph2[8..16].copy_from_slice(&0x1000u64.to_le_bytes());
        ph2[16..24].copy_from_slice(&0xFFFFFFFF80000000u64.to_le_bytes());
        ph2[24..32].copy_from_slice(&0x200000u64.to_le_bytes());
        ph2[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        ph2[40..48].copy_from_slice(&0x2000u64.to_le_bytes());
        ph2[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        let mut buf = [LoadSegment { paddr: 0, vaddr: 0, offset: 0, filesz: 0, memsz: 0, align: 0, flags: 0 }; MAX_TEST_SEGMENTS];
        let count = collect_segments(&mut segment_iter(&image).unwrap(), &mut buf);
        assert_eq!(count, 2);
        assert_eq!(buf[0].paddr, 0x400000);
        assert_eq!(buf[0].vaddr, 0xFFFFFFFF80200000);
        assert_eq!(buf[1].paddr, 0x200000);
        assert_eq!(buf[1].vaddr, 0xFFFFFFFF80000000);
    }

    /// Simulate the kernel loading logic: parse PT_LOAD segments,
    /// compute kern_phys_base, kern_virt_base, kern_size, and entry_point.
    /// This mirrors what `uefi_helpers::load_kernel_elf` does internally.
    #[test]
    fn test_kernel_load_simulation_single_segment() {
        let image = build_test_elf();
        let mut iter = segment_iter(&image).unwrap();
        let entry = iter.ehdr().e_entry;

        let mut kern_phys_base = u64::MAX;
        let mut kern_virt_base = u64::MAX;
        let mut kern_end_phys = 0u64;

        while let Some(seg) = iter.next() {
            kern_phys_base = kern_phys_base.min(seg.paddr);
            kern_virt_base = kern_virt_base.min(seg.vaddr);
            kern_end_phys = kern_end_phys.max(seg.paddr + seg.memsz);
        }

        assert_eq!(kern_phys_base, 0x200000);
        assert_eq!(kern_virt_base, 0xFFFFFFFF80000000);
        assert_eq!(kern_end_phys, 0x200000 + 0x2000); // paddr + memsz
        assert_eq!(kern_end_phys - kern_phys_base, 0x2000); // kern_size
        assert_eq!(entry, 0xFFFFFFFF80001000);
    }

    /// Simulate kernel loading with two PT_LOAD segments.
    /// Verifies that kern_phys_base is the minimum paddr and
    /// kern_size covers the full span.
    #[test]
    fn test_kernel_load_simulation_two_segments() {
        let mut image = [0u8; 0x4000];

        // ELF header
        image[0] = 0x7f; image[1] = b'E'; image[2] = b'L'; image[3] = b'F';
        image[4] = ELFCLASS64;
        image[5] = ELFDATA2LSB;
        image[6] = 1;
        image[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        image[18..20].copy_from_slice(&62u16.to_le_bytes());
        image[20..24].copy_from_slice(&1u32.to_le_bytes());
        image[24..32].copy_from_slice(&0xFFFFFFFF80001000u64.to_le_bytes());
        image[32..40].copy_from_slice(&64u64.to_le_bytes());
        image[40..48].copy_from_slice(&0u64.to_le_bytes());
        image[48..52].copy_from_slice(&0u32.to_le_bytes());
        image[52..54].copy_from_slice(&64u16.to_le_bytes());
        image[54..56].copy_from_slice(&56u16.to_le_bytes());
        image[56..58].copy_from_slice(&2u16.to_le_bytes());

        // Segment 1: text at paddr=0x200000, vaddr=0xFFFFFFFF80000000, memsz=0x1000
        let ph1 = &mut image[64..120];
        ph1[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph1[4..8].copy_from_slice(&5u32.to_le_bytes()); // PF_R|PF_X
        ph1[8..16].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[16..24].copy_from_slice(&0xFFFFFFFF80000000u64.to_le_bytes());
        ph1[24..32].copy_from_slice(&0x200000u64.to_le_bytes());
        ph1[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[40..48].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        // Segment 2: data at paddr=0x300000, vaddr=0xFFFFFFFF80100000, memsz=0x2000 (BSS)
        let ph2 = &mut image[120..176];
        ph2[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph2[4..8].copy_from_slice(&6u32.to_le_bytes()); // PF_R|PF_W
        ph2[8..16].copy_from_slice(&0x2000u64.to_le_bytes());
        ph2[16..24].copy_from_slice(&0xFFFFFFFF80100000u64.to_le_bytes());
        ph2[24..32].copy_from_slice(&0x300000u64.to_le_bytes());
        ph2[32..40].copy_from_slice(&0x1000u64.to_le_bytes()); // filesz
        ph2[40..48].copy_from_slice(&0x2000u64.to_le_bytes()); // memsz (0x1000 BSS)
        ph2[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        let mut iter = segment_iter(&image).unwrap();
        let entry = iter.ehdr().e_entry;

        let mut kern_phys_base = u64::MAX;
        let mut kern_virt_base = u64::MAX;
        let mut kern_end_phys = 0u64;

        while let Some(seg) = iter.next() {
            kern_phys_base = kern_phys_base.min(seg.paddr);
            kern_virt_base = kern_virt_base.min(seg.vaddr);
            kern_end_phys = kern_end_phys.max(seg.paddr + seg.memsz);
        }

        assert_eq!(kern_phys_base, 0x200000);
        assert_eq!(kern_virt_base, 0xFFFFFFFF80000000);
        // kern_end_phys = max(0x200000+0x1000, 0x300000+0x2000) = 0x302000
        assert_eq!(kern_end_phys, 0x302000);
        // kern_size = 0x302000 - 0x200000 = 0x102000
        assert_eq!(kern_end_phys - kern_phys_base, 0x102000);
        assert_eq!(entry, 0xFFFFFFFF80001000);
    }

    /// Simulate segment data copy and BSS zero-fill.
    /// Uses a buffer as the "physical memory" destination.
    #[test]
    fn test_segment_copy_and_bss_fill() {
        let mut image = build_test_elf();
        // Put recognizable data at offset 0x1000 (segment file data)
        let test_data: &[u8] = b"HELLO_KERNEL_DATA!";
        image[0x1000..0x1000 + test_data.len()].copy_from_slice(test_data);

        let mut iter = segment_iter(&image).unwrap();

        // Simulate loading: allocate a "physical memory" buffer
        // Segment: paddr=0x200000, filesz=0x1000, memsz=0x2000
        let mut phys_mem = [0xAAu8; 0x2000]; // Fill with 0xAA to detect zero-fill

        while let Some(seg) = iter.next() {
            // In real code, dest = seg.paddr as *mut u8
            // Here we simulate by writing into our buffer
            let src = &image[seg.offset as usize..(seg.offset + seg.filesz) as usize];
            phys_mem[..seg.filesz as usize].copy_from_slice(src);

            // Zero-fill BSS
            if seg.memsz > seg.filesz {
                let bss_start = seg.filesz as usize;
                let bss_end = seg.memsz as usize;
                phys_mem[bss_start..bss_end].fill(0);
            }
        }

        // Verify file data was copied
        assert_eq!(&phys_mem[..test_data.len()], test_data);
        // Verify rest of file data is zeros (from build_test_elf)
        for i in test_data.len()..0x1000 {
            assert_eq!(phys_mem[i], 0, "expected zero at offset {i}");
        }
        // Verify BSS was zero-filled (was 0xAA, now 0x00)
        assert_eq!(&phys_mem[0x1000..0x2000], &[0u8; 0x1000]);
    }

    /// Test that segment flags are correctly parsed.
    #[test]
    fn test_segment_flags() {
        let mut image = [0u8; 0x4000];

        // ELF header
        image[0] = 0x7f; image[1] = b'E'; image[2] = b'L'; image[3] = b'F';
        image[4] = ELFCLASS64;
        image[5] = ELFDATA2LSB;
        image[6] = 1;
        image[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        image[18..20].copy_from_slice(&62u16.to_le_bytes());
        image[20..24].copy_from_slice(&1u32.to_le_bytes());
        image[24..32].copy_from_slice(&0xFFFFFFFF80001000u64.to_le_bytes());
        image[32..40].copy_from_slice(&64u64.to_le_bytes());
        image[40..48].copy_from_slice(&0u64.to_le_bytes());
        image[48..52].copy_from_slice(&0u32.to_le_bytes());
        image[52..54].copy_from_slice(&64u16.to_le_bytes());
        image[54..56].copy_from_slice(&56u16.to_le_bytes());
        image[56..58].copy_from_slice(&3u16.to_le_bytes()); // 3 segments

        // Segment 1: text (PF_R|PF_X = 5), offset=0x1000, filesz=0x1000
        let ph1 = &mut image[64..120];
        ph1[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph1[4..8].copy_from_slice(&5u32.to_le_bytes());
        ph1[8..16].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[16..24].copy_from_slice(&0xFFFFFFFF80000000u64.to_le_bytes());
        ph1[24..32].copy_from_slice(&0x200000u64.to_le_bytes());
        ph1[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[40..48].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        // Segment 2: data (PF_R|PF_W = 6), offset=0x2000, filesz=0x1000
        let ph2 = &mut image[120..176];
        ph2[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph2[4..8].copy_from_slice(&6u32.to_le_bytes());
        ph2[8..16].copy_from_slice(&0x2000u64.to_le_bytes());
        ph2[16..24].copy_from_slice(&0xFFFFFFFF80100000u64.to_le_bytes());
        ph2[24..32].copy_from_slice(&0x300000u64.to_le_bytes());
        ph2[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        ph2[40..48].copy_from_slice(&0x1000u64.to_le_bytes());
        ph2[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        // Segment 3: rodata (PF_R = 4), offset=0x3000, filesz=0x800
        let ph3 = &mut image[176..232];
        ph3[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph3[4..8].copy_from_slice(&4u32.to_le_bytes());
        ph3[8..16].copy_from_slice(&0x3000u64.to_le_bytes());
        ph3[16..24].copy_from_slice(&0xFFFFFFFF80200000u64.to_le_bytes());
        ph3[24..32].copy_from_slice(&0x400000u64.to_le_bytes());
        ph3[32..40].copy_from_slice(&0x800u64.to_le_bytes());
        ph3[40..48].copy_from_slice(&0x800u64.to_le_bytes());
        ph3[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        let mut buf = [LoadSegment { paddr: 0, vaddr: 0, offset: 0, filesz: 0, memsz: 0, align: 0, flags: 0 }; MAX_TEST_SEGMENTS];
        let count = collect_segments(&mut segment_iter(&image).unwrap(), &mut buf);
        assert_eq!(count, 3);
        assert_eq!(buf[0].flags, 5); // PF_R|PF_X
        assert_eq!(buf[1].flags, 6); // PF_R|PF_W
        assert_eq!(buf[2].flags, 4); // PF_R
    }

    // ── Boundary and error-path tests ──

    #[test]
    fn test_parse_ehdr_not_executable() {
        let mut image = build_test_elf();
        // Set e_type to ET_REL (1) instead of ET_EXEC (2)
        image[16..18].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(parse_ehdr(&image), Err(ElfError::NotExecutable));
    }

    #[test]
    fn test_segment_iter_phdr_out_of_bounds() {
        let mut image = build_test_elf();
        // Set e_phoff to point beyond image
        image[32..40].copy_from_slice(&0x3000u64.to_le_bytes());
        assert!(matches!(segment_iter(&image), Err(ElfError::PhdrOutOfBounds)));
    }

    #[test]
    fn test_segment_iter_phdr_entry_too_small() {
        let mut image = build_test_elf();
        // Set e_phentsize to something smaller than 56
        image[54..56].copy_from_slice(&32u16.to_le_bytes());
        assert!(matches!(segment_iter(&image), Err(ElfError::PhdrEntryTooSmall)));
    }

    #[test]
    fn test_segment_iter_skips_segment_data_out_of_bounds() {
        let mut image = build_test_elf();
        // Set p_filesz to a huge value so seg_end > image.len()
        // p_filesz is at phdr offset 32 (relative to phdr start at byte 64)
        image[64 + 32..64 + 40].copy_from_slice(&0xFFFF_FFFFu64.to_le_bytes());
        // The segment should be silently skipped (not yielded)
        let mut buf = [LoadSegment { paddr: 0, vaddr: 0, offset: 0, filesz: 0, memsz: 0, align: 0, flags: 0 }; MAX_TEST_SEGMENTS];
        let count = collect_segments(&mut segment_iter(&image).unwrap(), &mut buf);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_segment_iter_empty_image() {
        let image: &[u8] = &[];
        assert!(matches!(segment_iter(image), Err(ElfError::TooShort)));
    }

    #[test]
    fn test_segment_iter_header_only() {
        // Image has valid ELF header but no program headers
        let mut image = [0u8; 64];
        image[0] = 0x7f; image[1] = b'E'; image[2] = b'L'; image[3] = b'F';
        image[4] = ELFCLASS64;
        image[5] = ELFDATA2LSB;
        image[6] = 1;
        image[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        image[18..20].copy_from_slice(&62u16.to_le_bytes());
        image[20..24].copy_from_slice(&1u32.to_le_bytes());
        image[24..32].copy_from_slice(&0xFFFFFFFF80001000u64.to_le_bytes());
        image[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
        image[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
        image[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
        image[56..58].copy_from_slice(&0u16.to_le_bytes());  // e_phnum = 0

        let mut iter = segment_iter(&image).unwrap();
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_segment_with_zero_filesz() {
        // A PT_LOAD segment with filesz=0 and memsz>0 is a pure BSS segment.
        let mut image = [0u8; 0x2000];
        image[0] = 0x7f; image[1] = b'E'; image[2] = b'L'; image[3] = b'F';
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
        ph[4..8].copy_from_slice(&6u32.to_le_bytes()); // PF_R|PF_W
        ph[8..16].copy_from_slice(&0u64.to_le_bytes()); // p_offset = 0
        ph[16..24].copy_from_slice(&0xFFFFFFFF80200000u64.to_le_bytes());
        ph[24..32].copy_from_slice(&0x400000u64.to_le_bytes());
        ph[32..40].copy_from_slice(&0u64.to_le_bytes()); // p_filesz = 0
        ph[40..48].copy_from_slice(&0x1000u64.to_le_bytes()); // p_memsz = 4K (pure BSS)
        ph[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        let mut buf = [LoadSegment { paddr: 0, vaddr: 0, offset: 0, filesz: 0, memsz: 0, align: 0, flags: 0 }; MAX_TEST_SEGMENTS];
        let count = collect_segments(&mut segment_iter(&image).unwrap(), &mut buf);
        assert_eq!(count, 1);
        assert_eq!(buf[0].filesz, 0);
        assert_eq!(buf[0].memsz, 0x1000);
    }

    #[test]
    fn test_mixed_pt_load_and_other() {
        // ELF with PT_INTERP + PT_LOAD + PT_GNU_STACK — only PT_LOAD should be yielded
        let mut image = [0u8; 0x4000];
        image[0] = 0x7f; image[1] = b'E'; image[2] = b'L'; image[3] = b'F';
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
        image[56..58].copy_from_slice(&3u16.to_le_bytes()); // 3 phdrs

        // Phdr 1: PT_INTERP (type=3) — should be skipped
        let ph1 = &mut image[64..120];
        ph1[0..4].copy_from_slice(&3u32.to_le_bytes()); // PT_INTERP
        ph1[4..8].copy_from_slice(&4u32.to_le_bytes());
        ph1[48..56].copy_from_slice(&1u64.to_le_bytes());

        // Phdr 2: PT_LOAD — should be yielded
        let ph2 = &mut image[120..176];
        ph2[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph2[4..8].copy_from_slice(&5u32.to_le_bytes());
        ph2[8..16].copy_from_slice(&0x1000u64.to_le_bytes());
        ph2[16..24].copy_from_slice(&0xFFFFFFFF80000000u64.to_le_bytes());
        ph2[24..32].copy_from_slice(&0x200000u64.to_le_bytes());
        ph2[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        ph2[40..48].copy_from_slice(&0x1000u64.to_le_bytes());
        ph2[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        // Phdr 3: PT_GNU_STACK (type=0x6474e551) — should be skipped
        let ph3 = &mut image[176..232];
        ph3[0..4].copy_from_slice(&0x6474e551u32.to_le_bytes());
        ph3[4..8].copy_from_slice(&6u32.to_le_bytes());
        ph3[48..56].copy_from_slice(&0x10u64.to_le_bytes());

        let mut buf = [LoadSegment { paddr: 0, vaddr: 0, offset: 0, filesz: 0, memsz: 0, align: 0, flags: 0 }; MAX_TEST_SEGMENTS];
        let count = collect_segments(&mut segment_iter(&image).unwrap(), &mut buf);
        assert_eq!(count, 1);
        assert_eq!(buf[0].paddr, 0x200000);
    }

    #[test]
    fn test_kernel_load_simulation_with_bss_segment() {
        // Simulate a kernel with text + data + BSS segments.
        // This mirrors a typical higher-half kernel layout:
        //   text: paddr=0x200000, vaddr=0xFFFFFFFF80000000, filesz=memsz=0x1000
        //   data: paddr=0x300000, vaddr=0xFFFFFFFF80100000, filesz=0x800, memsz=0x2000
        //   bss:  paddr=0x302000, vaddr=0xFFFFFFFF80102000, filesz=0, memsz=0x1000
        let mut image = [0u8; 0x4000];
        image[0] = 0x7f; image[1] = b'E'; image[2] = b'L'; image[3] = b'F';
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
        image[56..58].copy_from_slice(&3u16.to_le_bytes());

        // text segment
        let ph1 = &mut image[64..120];
        ph1[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph1[4..8].copy_from_slice(&5u32.to_le_bytes());
        ph1[8..16].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[16..24].copy_from_slice(&0xFFFFFFFF80000000u64.to_le_bytes());
        ph1[24..32].copy_from_slice(&0x200000u64.to_le_bytes());
        ph1[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[40..48].copy_from_slice(&0x1000u64.to_le_bytes());
        ph1[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        // data segment (with BSS)
        let ph2 = &mut image[120..176];
        ph2[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph2[4..8].copy_from_slice(&6u32.to_le_bytes());
        ph2[8..16].copy_from_slice(&0x2000u64.to_le_bytes());
        ph2[16..24].copy_from_slice(&0xFFFFFFFF80100000u64.to_le_bytes());
        ph2[24..32].copy_from_slice(&0x300000u64.to_le_bytes());
        ph2[32..40].copy_from_slice(&0x800u64.to_le_bytes());
        ph2[40..48].copy_from_slice(&0x2000u64.to_le_bytes());
        ph2[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        // pure BSS segment
        let ph3 = &mut image[176..232];
        ph3[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        ph3[4..8].copy_from_slice(&6u32.to_le_bytes());
        ph3[8..16].copy_from_slice(&0u64.to_le_bytes()); // offset=0 (no file data)
        ph3[16..24].copy_from_slice(&0xFFFFFFFF80102000u64.to_le_bytes());
        ph3[24..32].copy_from_slice(&0x302000u64.to_le_bytes());
        ph3[32..40].copy_from_slice(&0u64.to_le_bytes()); // filesz=0
        ph3[40..48].copy_from_slice(&0x1000u64.to_le_bytes()); // memsz=4K
        ph3[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

        let mut iter = segment_iter(&image).unwrap();
        let mut kern_phys_base = u64::MAX;
        let mut kern_virt_base = u64::MAX;
        let mut kern_end_phys = 0u64;
        let mut seg_count = 0;

        while let Some(seg) = iter.next() {
            seg_count += 1;
            kern_phys_base = kern_phys_base.min(seg.paddr);
            kern_virt_base = kern_virt_base.min(seg.vaddr);
            kern_end_phys = kern_end_phys.max(seg.paddr + seg.memsz);
        }

        assert_eq!(seg_count, 3);
        assert_eq!(kern_phys_base, 0x200000);
        assert_eq!(kern_virt_base, 0xFFFFFFFF80000000);
        // kern_end = max(0x200000+0x1000, 0x300000+0x2000, 0x302000+0x1000) = 0x303000
        assert_eq!(kern_end_phys, 0x303000);
        assert_eq!(kern_end_phys - kern_phys_base, 0x103000);
    }
}
