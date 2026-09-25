//! The block sources a file server starts on.
//!
//! C correspondence: none for the fail-closed arm — the C file servers
//! reach their block driver through the bdev label channel from the first
//! mount. The Rust block seam (`minix-fs`'s `BlockSource` to a real block
//! driver) is the separately tracked E-FSBDEV item; until it lands, a
//! server wired with [`PendingBlockSource`] starts, handshakes, dispatches,
//! and answers every block read or write with "input/output error" —
//! honest failure, never a pretend disk. The block *size* is pure
//! configuration (512 is MFS's smallest block, `minix3/minix/fs/mfs/
//! const.h`'s block era) and is reported without pretending any medium
//! exists.
//!
//! [`ImgrdBlockSource`] is the boot image RAM disk: real storage, the
//! form the packaged image fills once image assembly (E-IMGPKG) lands.
//! [`BootBlockSource`] is the construction-time choice between the two.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use minix_fs::cache::{BlockKey, BlockSource};
use minix_types::{EIO, Errno};

/// A block source that refuses every transfer.
#[derive(Debug, Default, Clone, Copy)]
pub struct PendingBlockSource;

impl BlockSource for PendingBlockSource {
    fn block_size(&self) -> usize {
        512
    }

    fn read_block(&self, _key: BlockKey, _out: &mut [u8]) -> Result<(), Errno> {
        Err(Errno::from_i32(EIO))
    }

    fn write_block(&mut self, _key: BlockKey, _data: &[u8]) -> Result<(), Errno> {
        Err(Errno::from_i32(EIO))
    }
}

/// The boot image RAM disk (`/dev/imgrd`) as a [`BlockSource`].
///
/// C correspondence: the memory driver serves imgrd out of its own image —
/// the device geometry *is* the linked image (`memory.c:148-150`, the
/// `_binary_imgrd_mfs_*` symbols of `drivers/storage/memory/local.h:5-9`),
/// and transfers obey `m_block_transfer`'s edge rules (`memory.c:442-477`):
/// a transfer reaching past the device end copies the surviving bytes, a
/// request entirely past the end answers zero.
///
/// Adaptation note (the E-FSBDEV minimal half): in C the image bytes live
/// in the memory driver's data segment and MFS reaches them over the bdev
/// channel. The bdev IPC transport is the separately tracked full half of
/// that item, so this source hands the image to the file server process
/// directly; when the channel lands, servers switch to `minix-fs`'s
/// `BdevBlockSource` and this type stays the boot-image form factor.
///
/// B11: the packaged imgrd (8 MB) is embedded in the MFS binary via
/// `include_bytes!` (static rdata). To avoid a 8 MB heap allocation that
/// exceeds the slab pool, `from_static` wraps the reference zero-copy.
///
/// B15: C's RAM disk is writable — the memory driver serves imgrd out of
/// its own buffer, and clean-rw mount clears `FLAG_CLEAN` then stores
/// block 0 back (`mfs/src/mount.rs:417-420`, mirroring C `mount.c:90-95`).
/// The static rdata base cannot be mutated, so a bounded copy-on-write
/// `overlay` shadows modified blocks: reads check the overlay first and
/// fall through to the base; static writes land in the overlay (clip at
/// the device end, matching `memory.c:442-443`). Owned writes stay
/// in-place and never populate the overlay. Dirty-mount touches only
/// block 0, so the overlay holds a handful of block-size entries — well
/// within the slab pool.
#[derive(Debug)]
pub struct ImgrdBlockSource {
    inner: ImageData,
    block_size: usize,
    /// B15: CoW overlay keyed by block number. Only the static arm
    /// populates it; owned writes mutate `inner` in place.
    overlay: BTreeMap<u64, Vec<u8>>,
}

#[derive(Debug)]
enum ImageData {
    /// Owned copy (tests, or future write-back scenarios).
    Owned(Vec<u8>),
    /// Zero-copy static reference (packaged boot image, E-IMGPKG B11).
    Static(&'static [u8]),
}

impl ImageData {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Owned(v) => v,
            Self::Static(s) => s,
        }
    }
}

impl ImgrdBlockSource {
    /// Wrap owned `image` bytes as the device (test/dev path). `block_size`
    /// is the file system block size the mount path validates against the
    /// superblock. An empty image or a zero block size is `EINVAL`.
    pub fn new(image: Vec<u8>, block_size: usize) -> Result<Self, Errno> {
        if image.is_empty() || block_size == 0 {
            return Err(Errno::EINVAL);
        }
        Ok(Self {
            inner: ImageData::Owned(image),
            block_size,
            overlay: BTreeMap::new(),
        })
    }

    /// Wrap a static image reference zero-copy (the packaged boot image,
    /// B11). Avoids heap allocation for large imgrd blobs. Writes are
    /// served by a bounded CoW overlay (B15) so clean-rw mount's
    /// dirty-mark on block 0 succeeds without mutating read-only rdata.
    pub fn from_static(image: &'static [u8], block_size: usize) -> Result<Self, Errno> {
        if image.is_empty() || block_size == 0 {
            return Err(Errno::EINVAL);
        }
        Ok(Self {
            inner: ImageData::Static(image),
            block_size,
            overlay: BTreeMap::new(),
        })
    }

    /// Device size in bytes — the imgrd geometry (`memory.c:149`).
    pub fn size_bytes(&self) -> u64 {
        self.inner.as_bytes().len() as u64
    }

    /// The image bytes as loaded.
    pub fn image(&self) -> &[u8] {
        self.inner.as_bytes()
    }
}

impl BlockSource for ImgrdBlockSource {
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn read_block(&self, key: BlockKey, out: &mut [u8]) -> Result<(), Errno> {
        if out.len() != self.block_size {
            return Err(Errno::EINVAL);
        }
        // B15: the overlay shadows the base for any block that was
        // written through the static arm. A clipped tail (write reaching
        // past the device end) zero-fills the rest, matching the base's
        // own edge rule (`memory.c:442-443`).
        if let Some(buf) = self.overlay.get(&key.block) {
            let n = buf.len().min(out.len());
            out[..n].copy_from_slice(&buf[..n]);
            out[n..].fill(0);
            return Ok(());
        }
        let image = self.inner.as_bytes();
        let start = key.block.saturating_mul(self.block_size as u64);
        if start >= image.len() as u64 {
            // Entirely past the device end: the transfer answers zero
            // (`memory.c:442-443`), so the block reads as zeros.
            out.fill(0);
            return Ok(());
        }
        let start = start as usize;
        let surviving = (image.len() - start).min(self.block_size);
        out[..surviving].copy_from_slice(&image[start..start + surviving]);
        out[surviving..].fill(0);
        Ok(())
    }

    fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<(), Errno> {
        if data.len() != self.block_size {
            return Err(Errno::EINVAL);
        }
        match &mut self.inner {
            ImageData::Owned(v) => {
                let start = key.block.saturating_mul(self.block_size as u64);
                if start >= v.len() as u64 {
                    return Ok(());
                }
                let start = start as usize;
                let surviving = (v.len() - start).min(self.block_size);
                v[start..start + surviving].copy_from_slice(&data[..surviving]);
                Ok(())
            }
            // B15: static rdata is read-only, so a write is served from
            // the CoW overlay — matching C's memory driver mutating its
            // own RAM-disk buffer. Clip at the device end like the base
            // transfer; a write entirely past the end writes nothing.
            ImageData::Static(base) => {
                let start = key.block.saturating_mul(self.block_size as u64);
                if start >= base.len() as u64 {
                    return Ok(());
                }
                let surviving = (base.len() as u64 - start).min(self.block_size as u64) as usize;
                self.overlay.insert(key.block, data[..surviving].to_vec());
                Ok(())
            }
        }
    }
}

/// The block source a file server binary starts on: the boot image when
/// the packaging supplied one, the fail-closed source otherwise.
///
/// The choice is a construction-time fact — the binary either carries an
/// image or it does not — so it is one enum picked once at birth, not a
/// branch repeated on every request.
#[derive(Debug)]
pub enum BootBlockSource {
    /// No image came with the binary: every block touch answers `EIO`.
    Pending(PendingBlockSource),
    /// The boot image RAM disk.
    Imgrd(ImgrdBlockSource),
}

impl BootBlockSource {
    /// Pick the arm for `image`. Empty slices — the unpackaged default —
    /// start fail-closed; anything else becomes the imgrd device (zero-copy
    /// static reference, B11).
    pub fn from_boot_image(image: &'static [u8], block_size: usize) -> Self {
        match ImgrdBlockSource::from_static(image, block_size) {
            Ok(source) => Self::Imgrd(source),
            Err(_) => Self::Pending(PendingBlockSource),
        }
    }
}

impl BlockSource for BootBlockSource {
    fn block_size(&self) -> usize {
        match self {
            Self::Pending(source) => source.block_size(),
            Self::Imgrd(source) => source.block_size(),
        }
    }

    fn read_block(&self, key: BlockKey, out: &mut [u8]) -> Result<(), Errno> {
        match self {
            Self::Pending(source) => source.read_block(key, out),
            Self::Imgrd(source) => source.read_block(key, out),
        }
    }

    fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<(), Errno> {
        match self {
            Self::Pending(source) => source.write_block(key, data),
            Self::Imgrd(source) => source.write_block(key, data),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    const BS: usize = 512;

    /// A device of `len` bytes whose every byte states its offset (low
    /// eight bits), so a read asserts exact placement, not just length.
    fn imgrd(len: usize) -> ImgrdBlockSource {
        let mut image = vec![0u8; len];
        for (offset, byte) in image.iter_mut().enumerate() {
            *byte = offset as u8;
        }
        ImgrdBlockSource::new(image, BS).unwrap()
    }

    #[test]
    fn test_pending_source_refuses_every_transfer() {
        let mut src = PendingBlockSource;
        assert_eq!(src.block_size(), 512);
        let key = BlockKey::new(1, 512);
        assert_eq!(src.read_block(key, &mut [0u8; 512]).unwrap_err(), Errno::from_i32(EIO));
        assert_eq!(src.write_block(key, &[0u8; 512]).unwrap_err(), Errno::from_i32(EIO));
    }

    #[test]
    fn test_imgrd_source_reads_seeded_bytes_at_block_offsets() {
        let src = imgrd(2 * BS);
        let mut out = [0u8; BS];
        src.read_block(BlockKey::new(0, 0), &mut out).unwrap();
        assert_eq!(out[0], 0);
        assert_eq!(out[BS - 1], (BS - 1) as u8);
        src.read_block(BlockKey::new(0, 1), &mut out).unwrap();
        assert_eq!(out[0], BS as u8);
    }

    #[test]
    fn test_imgrd_source_short_final_block_zero_fills_the_tail() {
        // 600-byte device: block one holds the 88 surviving bytes, zeros
        // beyond (the "copy what fits" edge of `m_block_transfer`).
        let src = imgrd(600);
        let mut out = [0u8; BS];
        src.read_block(BlockKey::new(0, 1), &mut out).unwrap();
        let expected: Vec<u8> = (BS..600).map(|b| b as u8).collect();
        assert_eq!(out[..88], expected[..]);
        assert!(out[88..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_imgrd_source_block_entirely_past_end_reads_zeros() {
        // `memory.c:442-443`: a request entirely past the end answers
        // zero — success with a zeroed block, not an error.
        let src = imgrd(BS);
        let mut out = [0xABu8; BS];
        src.read_block(BlockKey::new(0, 1), &mut out).unwrap();
        assert!(out.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_imgrd_source_write_clips_at_device_end() {
        let mut src = imgrd(600);
        let block = [0xEEu8; BS];
        src.write_block(BlockKey::new(0, 1), &block).unwrap();
        // Only the 88 surviving bytes landed; the device did not grow.
        assert_eq!(src.size_bytes(), 600);
        assert!(src.image()[512..].iter().all(|&b| b == 0xEE));
        assert_eq!(src.image().len(), 600);
    }

    #[test]
    fn test_imgrd_source_write_entirely_past_end_writes_nothing() {
        let mut src = imgrd(BS);
        src.write_block(BlockKey::new(0, 5), &[0xEEu8; BS]).unwrap();
        // The device neither grew nor changed: byte zero still states its
        // offset, the size is one block.
        assert_eq!(src.size_bytes(), BS as u64);
        assert_eq!(src.image()[0], 0);
        assert_eq!(src.image()[BS - 1], (BS - 1) as u8);
    }

    #[test]
    fn test_imgrd_source_rejects_size_mismatched_transfers() {
        let mut src = imgrd(2 * BS);
        let short_read = src.read_block(BlockKey::new(0, 0), &mut [0u8; BS - 1]);
        let long_write = src.write_block(BlockKey::new(0, 0), &[0u8; BS + 1]);
        assert_eq!(short_read.unwrap_err(), Errno::EINVAL);
        assert_eq!(long_write.unwrap_err(), Errno::EINVAL);
    }

    #[test]
    fn test_imgrd_source_rejects_empty_image_and_zero_block_size() {
        let empty = ImgrdBlockSource::new(Vec::new(), BS);
        let zero_bs = ImgrdBlockSource::new(vec![0u8; BS], 0);
        assert_eq!(empty.unwrap_err(), Errno::EINVAL);
        assert_eq!(zero_bs.unwrap_err(), Errno::EINVAL);
    }

    #[test]
    fn test_boot_source_empty_image_starts_fail_closed() {
        let src = BootBlockSource::from_boot_image(&[], BS);
        assert!(matches!(src, BootBlockSource::Pending(_)));
        assert_eq!(src.block_size(), BS);
        let refused = src.read_block(BlockKey::new(0, 0), &mut [0u8; BS]);
        assert_eq!(refused.unwrap_err(), Errno::from_i32(EIO));
    }

    #[test]
    fn test_boot_source_supplied_image_serves_imgrd_arm() {
        static IMAGE: [u8; 2 * BS] = [0x5Au8; 2 * BS];
        let mut src = BootBlockSource::from_boot_image(&IMAGE, BS);
        assert!(matches!(src, BootBlockSource::Imgrd(_)));
        let mut out = [0u8; BS];
        src.read_block(BlockKey::new(0, 1), &mut out).unwrap();
        assert!(out.iter().all(|&b| b == 0x5A));
        // B15: the static arm is served by a bounded CoW overlay —
        // writes succeed and shadow the base on subsequent reads,
        // untouched blocks still read from the base.
        src.write_block(BlockKey::new(0, 0), &[0x11u8; BS]).unwrap();
        let mut back = [0u8; BS];
        src.read_block(BlockKey::new(0, 0), &mut back).unwrap();
        assert!(back.iter().all(|&b| b == 0x11));
        let mut other = [0u8; BS];
        src.read_block(BlockKey::new(0, 1), &mut other).unwrap();
        assert!(other.iter().all(|&b| b == 0x5A));
    }

    #[test]
    fn test_imgrd_source_owned_variant_supports_writes() {
        // The owned variant (used in tests / future writable scenarios)
        // does support write_block in-place.
        let image = vec![0x5Au8; 2 * BS];
        let mut src = ImgrdBlockSource::new(image, BS).unwrap();
        src.write_block(BlockKey::new(0, 0), &[0x11u8; BS]).unwrap();
        let mut back = [0u8; BS];
        src.read_block(BlockKey::new(0, 0), &mut back).unwrap();
        assert!(back.iter().all(|&b| b == 0x11));
    }

    #[test]
    fn test_static_source_overlay_write_shadows_base_readback() {
        // B15: writes on the static arm land in the CoW overlay and are
        // served by subsequent reads, without mutating the base image.
        static IMAGE: [u8; 4 * BS] = [0x5Au8; 4 * BS];
        let mut src = ImgrdBlockSource::from_static(&IMAGE, BS).unwrap();
        let mut before = [0u8; BS];
        src.read_block(BlockKey::new(0, 2), &mut before).unwrap();
        assert!(before.iter().all(|&b| b == 0x5A));
        src.write_block(BlockKey::new(0, 2), &[0xABu8; BS]).unwrap();
        let mut after = [0u8; BS];
        src.read_block(BlockKey::new(0, 2), &mut after).unwrap();
        assert!(after.iter().all(|&b| b == 0xAB));
        // Adjacent blocks still read the base.
        let mut neigh = [0u8; BS];
        src.read_block(BlockKey::new(0, 1), &mut neigh).unwrap();
        assert!(neigh.iter().all(|&b| b == 0x5A));
        src.read_block(BlockKey::new(0, 3), &mut neigh).unwrap();
        assert!(neigh.iter().all(|&b| b == 0x5A));
        // Base image untouched.
        assert!(src.image().iter().all(|&b| b == 0x5A));
    }

    #[test]
    fn test_static_source_overlay_write_clip_short_final_block() {
        // A write reaching past the device end is clipped (matches C
        // `memory.c:442-443`); the overlay stores only the surviving
        // bytes and reads zero-fill the rest.
        static IMAGE: [u8; 600] = [0x5Au8; 600];
        let mut src = ImgrdBlockSource::from_static(&IMAGE, BS).unwrap();
        src.write_block(BlockKey::new(0, 1), &[0xEEu8; BS]).unwrap();
        let mut out = [0u8; BS];
        src.read_block(BlockKey::new(0, 1), &mut out).unwrap();
        assert!(out[..88].iter().all(|&b| b == 0xEE));
        assert!(out[88..].iter().all(|&b| b == 0));
        assert_eq!(src.size_bytes(), 600);
    }

    #[test]
    fn test_static_source_overlay_write_entirely_past_end_is_noop() {
        // B15 keeps the base's edge rule: a write entirely past the
        // device end writes nothing; a read of that block still
        // answers zero (via base fall-through).
        static IMAGE: [u8; BS] = [0x5Au8; BS];
        let mut src = ImgrdBlockSource::from_static(&IMAGE, BS).unwrap();
        src.write_block(BlockKey::new(0, 5), &[0xEEu8; BS]).unwrap();
        let mut out = [0xABu8; BS];
        src.read_block(BlockKey::new(0, 5), &mut out).unwrap();
        assert!(out.iter().all(|&b| b == 0));
        assert!(src.image().iter().all(|&b| b == 0x5A));
    }

    #[test]
    fn test_static_source_overlay_multiple_writes_independent_blocks() {
        // Dirty-mark writes block 0; a future write to block 3 would
        // shadow independently, keeping the overlay per-block.
        static IMAGE: [u8; 4 * BS] = [0u8; 4 * BS];
        let mut src = ImgrdBlockSource::from_static(&IMAGE, BS).unwrap();
        src.write_block(BlockKey::new(0, 0), &[0x11u8; BS]).unwrap();
        src.write_block(BlockKey::new(0, 3), &[0x33u8; BS]).unwrap();
        let mut b0 = [0u8; BS];
        src.read_block(BlockKey::new(0, 0), &mut b0).unwrap();
        assert!(b0.iter().all(|&b| b == 0x11));
        let mut b3 = [0u8; BS];
        src.read_block(BlockKey::new(0, 3), &mut b3).unwrap();
        assert!(b3.iter().all(|&b| b == 0x33));
        let mut b1 = [0u8; BS];
        src.read_block(BlockKey::new(0, 1), &mut b1).unwrap();
        assert!(b1.iter().all(|&b| b == 0x00));
    }
}
