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
/// Writes land in the image bytes and live as long as the process does —
/// the RAM disk contract (contents lost at shutdown), like C's
/// `m_vaddrs`-backed device. A read of untouched image bytes returns what
/// was booted; the source never invents blocks it was not given.
#[derive(Debug)]
pub struct ImgrdBlockSource {
    image: Vec<u8>,
    block_size: usize,
}

impl ImgrdBlockSource {
    /// Wrap `image` bytes as the device. `block_size` is the file system
    /// block size the mount path validates against the superblock (C's
    /// `mount.c:52` adoption check — the Rust cache compares sizes instead
    /// of reconfiguring). An empty image or a zero block size is `EINVAL`.
    pub fn new(image: Vec<u8>, block_size: usize) -> Result<Self, Errno> {
        if image.is_empty() || block_size == 0 {
            return Err(Errno::EINVAL);
        }
        Ok(Self { image, block_size })
    }

    /// Device size in bytes — the imgrd geometry (`memory.c:149`).
    pub fn size_bytes(&self) -> u64 {
        self.image.len() as u64
    }

    /// The image bytes as loaded.
    pub fn image(&self) -> &[u8] {
        &self.image
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
        let start = key.block.saturating_mul(self.block_size as u64);
        if start >= self.image.len() as u64 {
            // Entirely past the device end: the transfer answers zero
            // (`memory.c:442-443`), so the block reads as zeros.
            out.fill(0);
            return Ok(());
        }
        let start = start as usize;
        let surviving = (self.image.len() - start).min(self.block_size);
        out[..surviving].copy_from_slice(&self.image[start..start + surviving]);
        out[surviving..].fill(0);
        Ok(())
    }

    fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<(), Errno> {
        if data.len() != self.block_size {
            return Err(Errno::EINVAL);
        }
        let start = key.block.saturating_mul(self.block_size as u64);
        if start >= self.image.len() as u64 {
            // Entirely past the end: nothing survives, zero bytes copied
            // (`memory.c:442-443`).
            return Ok(());
        }
        let start = start as usize;
        let surviving = (self.image.len() - start).min(self.block_size);
        self.image[start..start + surviving].copy_from_slice(&data[..surviving]);
        Ok(())
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
    /// start fail-closed; anything else becomes the imgrd device.
    pub fn from_boot_image(image: &'static [u8], block_size: usize) -> Self {
        match ImgrdBlockSource::new(image.to_vec(), block_size) {
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
        // Writes reach the enum's imgrd arm: the block reads back through
        // the same enum with the written bytes.
        src.write_block(BlockKey::new(0, 0), &[0x11u8; BS]).unwrap();
        let mut back = [0u8; BS];
        src.read_block(BlockKey::new(0, 0), &mut back).unwrap();
        assert!(back.iter().all(|&b| b == 0x11));
    }
}
