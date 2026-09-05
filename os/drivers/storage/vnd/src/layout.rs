//! Virtual node disk layout: chunked file copies plus geometry fallback.
//!
//! C correspondence: the transfer loop (`vnd_transfer`, `vnd.c:212`),
//! which copies in chunks of at most `VND_BUF_SIZE` (`vnd.c:12`,
//! `MIN(bytes - off, VND_BUF_SIZE)`, `vnd.c:253`) through `pread`
//! (`vnd.c:259`) and `pwrite` (`vnd.c:279`) with an `fsync` after
//! writes (`vnd.c:299`); the file grab (`VNDIOCSET`, `vnd.c:369`,
//! stealing the descriptor with `copyfd`, `vnd.c:387`, sizing with
//! `fstat`, `vnd.c:393`); and the geometry path (`vnd_layout`,
//! `vnd.c:311`: hardware geometry through the disk ioctl when the
//! file carries `VNDIOF_HASGEOM`, `vnd.c:317-328`, otherwise derived
//! from the sector count, `vnd.c:336-342`).
//!
//! File descriptor traffic stays in the service binary; this module
//! owns the pure layout half: how a transfer splits into chunks and
//! how geometry is derived when the file carries none.

/// Largest single chunk moved per copy step (`VND_BUF_SIZE`, `vnd.c:12`).
pub const CHUNK_SIZE: u64 = 65536;

/// Disk sector size in bytes.
pub const SECTOR_SIZE: u64 = 512;

/// Fallback heads when the file carries no geometry (`vnd.c:336-342`).
pub const FALLBACK_HEADS: u32 = 64;

/// Fallback sectors per track when the file carries no geometry.
pub const FALLBACK_SECTORS_PER_TRACK: u32 = 32;

/// Drive geometry: cylinders, heads, sectors per track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// Cylinders of the virtual disk.
    pub cylinders: u32,
    /// Heads of the virtual disk.
    pub heads: u32,
    /// Sectors on each track.
    pub sectors_per_track: u32,
}

/// Derive geometry from a sector count (`vnd.c:336-342`).
///
/// Large images get a 64-head, 32-sector layout; tiny images fall
/// back to a single head and a single sector per track.
pub fn derive_geometry(sectors: u64) -> Geometry {
    if sectors >= FALLBACK_HEADS as u64 * FALLBACK_SECTORS_PER_TRACK as u64 {
        let per_cylinder = FALLBACK_HEADS * FALLBACK_SECTORS_PER_TRACK;
        Geometry {
            cylinders: (sectors / per_cylinder as u64) as u32,
            heads: FALLBACK_HEADS,
            sectors_per_track: FALLBACK_SECTORS_PER_TRACK,
        }
    } else {
        Geometry { cylinders: sectors.max(1) as u32, heads: 1, sectors_per_track: 1 }
    }
}

/// Split a transfer of `total` bytes starting at file offset `offset`
/// into chunk lengths (`MIN(bytes - off, VND_BUF_SIZE)`, `vnd.c:253`).
///
/// Returns the length of each copy step, in order.
pub fn split_chunks(offset: u64, total: u64) -> alloc::vec::Vec<u64> {
    let mut chunks = alloc::vec::Vec::new();
    let mut remaining = total;
    let mut _position = offset;
    while remaining > 0 {
        let step = remaining.min(CHUNK_SIZE);
        chunks.push(step);
        remaining -= step;
        _position += step;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_transfer_is_one_chunk() {
        assert_eq!(split_chunks(0, 512), alloc::vec![512]);
    }

    #[test]
    fn test_large_transfer_splits_at_chunk_size() {
        let chunks = split_chunks(0, CHUNK_SIZE * 2 + 100);
        assert_eq!(chunks, alloc::vec![CHUNK_SIZE, CHUNK_SIZE, 100]);
    }

    #[test]
    fn test_empty_transfer_needs_no_chunks() {
        assert!(split_chunks(100, 0).is_empty());
    }

    #[test]
    fn test_large_image_gets_fallback_geometry() {
        let geometry = derive_geometry(1024 * 1024);
        assert_eq!(geometry.heads, FALLBACK_HEADS);
        assert_eq!(geometry.sectors_per_track, FALLBACK_SECTORS_PER_TRACK);
        assert!(geometry.cylinders > 0);
    }

    #[test]
    fn test_tiny_image_gets_minimal_geometry() {
        let geometry = derive_geometry(10);
        assert_eq!((geometry.heads, geometry.sectors_per_track), (1, 1));
        assert_eq!(geometry.cylinders, 10);
    }
}
