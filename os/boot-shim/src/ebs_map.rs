//! UEFI-leg descriptor walker + classifier for the v1 handoff pairing loop.
//!
//! The EBS pairing loop (design §1.5 (6).4, scheme B) obtains the memory map
//! into a caller-owned raw buffer while Boot Services are still alive, then
//! classifies every descriptor and dual-writes the result into the handoff
//! arena: conventional memory becomes the free list (`WireMemoryRegion`
//! partition), everything else (except MMIO windows) becomes the reserved
//! list. This module is the pure, host-testable half: it validates the raw
//! map header and walks the raw bytes without ever forming a
//! `&MemoryDescriptor` (the design's permitted `read_unaligned` variant), so
//! the raw UEFI ABI calls stay in the leg while the classification arithmetic
//! runs — and is tested — on the host.
//!
//! # Validation order (hard contract ③)
//!
//! - `descriptor_version == 1` (the only supported version);
//! - `descriptor_size >= size_of::<RawMemoryDescriptor>()` (40) and
//!   `descriptor_size % 8 == 0` (every later descriptor starts 8-byte
//!   aligned when the buffer is);
//! - `map_size <= buffer.len()` and `map_size % descriptor_size == 0`;
//! - per produced region: `page_count != 0`, `len = page_count * 4096`
//!   checked, `end = phys + len` checked, `end <= pa_limit`.
//!
//! Skipped descriptors (MMIO windows) are not range-validated: no region is
//! produced from them, and firmware MMIO apertures may legitimately sit far
//! above RAM. The ceiling rule matches the earlier collector: reserved
//! descriptors above the conventional RAM top are dropped (the QEMU high
//! chipset hole); descriptors are collected in two passes because
//! GetMemoryMap order is not type-sorted.

use minix_boot::{HandoffV1WriteError, MemoryRegion, Serializer};
use minix_types::PhysBytes;

/// The only supported `EFI_MEMORY_DESCRIPTOR` version.
pub const MEMORY_DESCRIPTOR_VERSION: u32 = 1;

/// `sizeof(EFI_MEMORY_DESCRIPTOR)` — the version-1 prefix; firmware may append
/// a tail, which is why the walker uses `descriptor_size` as the stride.
pub const MEMORY_DESCRIPTOR_SIZE: usize = 40;

/// UEFI page size.
pub const PAGE_SIZE: u64 = 4096;

/// `EfiConventionalMemory`.
const CONVENTIONAL_TYPE: u32 = 7;
/// `EfiMemoryMappedIO` — skipped entirely (device aperture, never RAM).
const MMIO_TYPE: u32 = 11;
/// `EfiMemoryMappedIOPortSpace` — skipped entirely.
const MMIO_PORT_SPACE_TYPE: u32 = 12;

/// Raw descriptor prefix fields, little-endian per the UEFI spec on every
/// supported target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawMemoryDescriptor {
    pub ty: u32,
    pub physical_start: u64,
    pub number_of_pages: u64,
}

/// Why a map was rejected. Every variant is fail-closed: the pairing loop
/// must not fall back to a partial classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    UnsupportedVersion,
    DescriptorTooSmall,
    DescriptorSizeNotAligned,
    MapSizeBeyondBuffer,
    MapSizeNotMultiple,
    ZeroPageCount,
    LengthOverflow,
    RangeOverflow,
    OutOfPhysRange,
    Capacity(HandoffV1WriteError),
}

impl From<HandoffV1WriteError> for MapError {
    fn from(err: HandoffV1WriteError) -> Self {
        MapError::Capacity(err)
    }
}

/// Counts produced by one classification pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    pub conventional: usize,
    pub reserved: usize,
}

fn read_u32(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap())
}

fn descriptor_at(map: &[u8], off: usize) -> RawMemoryDescriptor {
    RawMemoryDescriptor {
        ty: read_u32(map, off),
        physical_start: read_u64(map, off + 8),
        number_of_pages: read_u64(map, off + 24),
    }
}

/// Validate `[phys, phys + pages*4096)` and return its byte length.
fn checked_len(desc: &RawMemoryDescriptor, pa_limit: u64) -> Result<u64, MapError> {
    if desc.number_of_pages == 0 {
        return Err(MapError::ZeroPageCount);
    }
    let len = desc
        .number_of_pages
        .checked_mul(PAGE_SIZE)
        .ok_or(MapError::LengthOverflow)?;
    let end = desc
        .physical_start
        .checked_add(len)
        .ok_or(MapError::RangeOverflow)?;
    if end > pa_limit {
        return Err(MapError::OutOfPhysRange);
    }
    Ok(len)
}

/// Walk a raw `GetMemoryMap` buffer and dual-write the classified map into
/// `serializer`.
///
/// `pa_limit` is the implementation-supported physical address ceiling for
/// *produced* regions (the leg passes its arch limit); it is not a firmware
/// claim.
pub fn classify_map(
    map: &[u8],
    map_size: usize,
    descriptor_size: usize,
    descriptor_version: u32,
    pa_limit: u64,
    serializer: &mut Serializer<'_>,
) -> Result<Classification, MapError> {
    if descriptor_version != MEMORY_DESCRIPTOR_VERSION {
        return Err(MapError::UnsupportedVersion);
    }
    if descriptor_size < MEMORY_DESCRIPTOR_SIZE {
        return Err(MapError::DescriptorTooSmall);
    }
    if !descriptor_size.is_multiple_of(8) {
        return Err(MapError::DescriptorSizeNotAligned);
    }
    if map_size > map.len() {
        return Err(MapError::MapSizeBeyondBuffer);
    }
    if !map_size.is_multiple_of(descriptor_size) {
        return Err(MapError::MapSizeNotMultiple);
    }

    let entries = map_size / descriptor_size;

    // Pass 1: RAM ceiling from the conventional entries (descriptor order is
    // not type-sorted, so the ceiling needs its own pass).
    let mut ram_top = 0u64;
    for i in 0..entries {
        let desc = descriptor_at(&map[..map_size], i * descriptor_size);
        if desc.ty != CONVENTIONAL_TYPE {
            continue;
        }
        let len = checked_len(&desc, pa_limit)?;
        let end = desc.physical_start + len;
        if end > ram_top {
            ram_top = end;
        }
    }

    // Pass 2: classify.
    let mut conventional = 0usize;
    let mut reserved = 0usize;
    for i in 0..entries {
        let desc = descriptor_at(&map[..map_size], i * descriptor_size);
        if desc.ty == MMIO_TYPE || desc.ty == MMIO_PORT_SPACE_TYPE {
            continue;
        }
        let len = checked_len(&desc, pa_limit)?;
        let region = MemoryRegion {
            base: PhysBytes(desc.physical_start),
            len: len as usize,
        };
        if desc.ty == CONVENTIONAL_TYPE {
            serializer.push_memmap(region)?;
            conventional += 1;
        } else if desc.physical_start < ram_top {
            serializer.push_reserved(region)?;
            reserved += 1;
        }
    }
    serializer.finish_memmap();
    serializer.finish_reserved();
    Ok(Classification {
        conventional,
        reserved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_boot::{
        Serializer, ARENA_CAP, SEMANTIC_MEMMAP_OFFSET, SEMANTIC_RESERVED_OFFSET,
        WIRE_MEMMAP_OFFSET, WIRE_RESERVED_OFFSET,
    };

    /// Build one descriptor of `desc_size` bytes (40 + zero padding).
    fn push_desc(buf: &mut [u8], index: usize, desc_size: usize, ty: u32, phys: u64, pages: u64) {
        let off = index * desc_size;
        buf[off..off + 4].copy_from_slice(&ty.to_le_bytes());
        buf[off + 4..off + 8].copy_from_slice(&0u32.to_le_bytes());
        buf[off + 8..off + 16].copy_from_slice(&phys.to_le_bytes());
        buf[off + 16..off + 24].copy_from_slice(&0u64.to_le_bytes());
        buf[off + 24..off + 32].copy_from_slice(&pages.to_le_bytes());
        buf[off + 32..off + 40].copy_from_slice(&0u64.to_le_bytes());
        for byte in &mut buf[off + 40..off + desc_size] {
            *byte = 0;
        }
    }

    fn get_u64(arena: &[u8], off: usize) -> u64 {
        u64::from_le_bytes(arena[off..off + 8].try_into().unwrap())
    }

    const PA_LIMIT: u64 = 1 << 48;

    #[test]
    fn classifies_conventional_and_reserved_below_ram_top() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let desc_size = 40usize;
        let mut map = [0u8; 40 * 5];
        // conventional [1 MiB, +2 MiB)
        push_desc(&mut map, 0, desc_size, CONVENTIONAL_TYPE, 0x10_0000, 0x200);
        // loader data [4 MiB, +4 KiB) → reserved (below ram_top)
        push_desc(&mut map, 1, desc_size, 4, 0x40_0000, 1);
        // MMIO aperture → skipped entirely
        push_desc(&mut map, 2, desc_size, MMIO_TYPE, 0xFE00_0000, 0x100);
        // reserved hole above the RAM top → dropped
        push_desc(&mut map, 3, desc_size, 3, 0xFD00_0000, 0x100);
        // conventional [64 MiB, +1 MiB)
        push_desc(&mut map, 4, desc_size, CONVENTIONAL_TYPE, 0x400_0000, 0x100);

        let c = classify_map(&map, map.len(), desc_size, 1, PA_LIMIT, &mut s).unwrap();
        assert_eq!(
            c,
            Classification {
                conventional: 2,
                reserved: 1
            }
        );
        let b = s.as_bytes();
        assert_eq!(get_u64(b, 128), 32, "memmap descriptor = 2 × 16");
        assert_eq!(get_u64(b, 136), 16, "reserved descriptor = 1 × 16");
        assert_eq!(get_u64(b, WIRE_MEMMAP_OFFSET), 0x10_0000);
        assert_eq!(get_u64(b, WIRE_MEMMAP_OFFSET + 8), 0x200 * 4096);
        assert_eq!(get_u64(b, WIRE_MEMMAP_OFFSET + 16), 0x400_0000);
        assert_eq!(get_u64(b, WIRE_RESERVED_OFFSET), 0x40_0000);
        assert_eq!(get_u64(b, WIRE_RESERVED_OFFSET + 8), 4096);
        // Semantic partition mirrors the same entries.
        assert_eq!(get_u64(b, SEMANTIC_MEMMAP_OFFSET), 0x10_0000);
        assert_eq!(get_u64(b, SEMANTIC_RESERVED_OFFSET), 0x40_0000);
    }

    #[test]
    fn padded_descriptor_stride_is_honored() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let desc_size = 48usize;
        let mut map = [0u8; 48 * 2];
        push_desc(&mut map, 0, desc_size, CONVENTIONAL_TYPE, 0x10_0000, 0x10);
        push_desc(&mut map, 1, desc_size, CONVENTIONAL_TYPE, 0x20_0000, 0x10);
        let c = classify_map(&map, map.len(), desc_size, 1, PA_LIMIT, &mut s).unwrap();
        assert_eq!(c.conventional, 2);
        assert_eq!(get_u64(s.as_bytes(), 128), 32);
    }

    fn classify_err(map: &[u8], map_size: usize, desc_size: usize, version: u32) -> MapError {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        classify_map(map, map_size, desc_size, version, PA_LIMIT, &mut s).unwrap_err()
    }

    #[test]
    fn rejects_unsupported_version() {
        let map = [0u8; 40];
        assert_eq!(
            classify_err(&map, 40, 40, 2),
            MapError::UnsupportedVersion
        );
    }

    #[test]
    fn rejects_descriptor_smaller_than_prefix() {
        let map = [0u8; 80];
        assert_eq!(
            classify_err(&map, 80, 39, 1),
            MapError::DescriptorTooSmall
        );
    }

    #[test]
    fn rejects_unaligned_descriptor_size() {
        let map = [0u8; 84];
        assert_eq!(
            classify_err(&map, 82, 41, 1),
            MapError::DescriptorSizeNotAligned
        );
    }

    #[test]
    fn rejects_map_size_beyond_buffer() {
        let map = [0u8; 40];
        assert_eq!(
            classify_err(&map, 80, 40, 1),
            MapError::MapSizeBeyondBuffer
        );
    }

    #[test]
    fn rejects_map_size_not_multiple() {
        let map = [0u8; 120];
        assert_eq!(
            classify_err(&map, 60, 40, 1),
            MapError::MapSizeNotMultiple
        );
    }

    #[test]
    fn rejects_zero_page_count_on_produced_region() {
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, CONVENTIONAL_TYPE, 0x10_0000, 0);
        assert_eq!(classify_err(&map, 40, 40, 1), MapError::ZeroPageCount);
    }

    #[test]
    fn rejects_page_count_overflow() {
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, CONVENTIONAL_TYPE, 0x10_0000, u64::MAX);
        assert_eq!(classify_err(&map, 40, 40, 1), MapError::LengthOverflow);
    }

    #[test]
    fn rejects_range_overflow() {
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, CONVENTIONAL_TYPE, u64::MAX - 0x1000, 2);
        assert_eq!(classify_err(&map, 40, 40, 1), MapError::RangeOverflow);
    }

    #[test]
    fn rejects_region_above_phys_limit() {
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, CONVENTIONAL_TYPE, PA_LIMIT, 1);
        assert_eq!(classify_err(&map, 40, 40, 1), MapError::OutOfPhysRange);
    }

    #[test]
    fn rejects_capacity_overflow() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let entries = 129usize;
        let mut map = [0u8; 40 * 129];
        for i in 0..entries {
            push_desc(&mut map, i, 40, CONVENTIONAL_TYPE, 0x10_0000 + i as u64 * 0x2000, 1);
        }
        assert_eq!(
            classify_map(&map, map.len(), 40, 1, PA_LIMIT, &mut s).unwrap_err(),
            MapError::Capacity(HandoffV1WriteError::TooManyMemmap)
        );
    }
}
