//! Block device trait: memory and empty backends for copy tests.
//!
//! Copying (`dd`), imaging (`isoread`), and verification read fixed size
//! blocks by number. The [`BlockDevice`] trait names that operation; the
//! driver backend (real hardware) lands with block device access. Until
//! then [`SliceDevice`] (a memory image) and [`EmptyDevice`] (no blocks)
//! back every test honestly.

use crate::ImageError;

/// Fixed size block reads by block number.
pub trait BlockDevice {
    /// Bytes per block.
    fn block_size(&self) -> usize;
    /// How many blocks the device holds.
    fn block_count(&self) -> u64;
    /// Copy block `index` into `out` (exactly `block_size` bytes).
    fn read_block(&self, index: u64, out: &mut [u8]) -> Result<(), ImageError>;
}

/// A device backed by a memory image: block `i` is bytes
/// `i * size..(i + 1) * size`. Short trailing images report short reads
/// instead of inventing bytes.
pub struct SliceDevice<'a> {
    /// Raw image bytes.
    pub image: &'a [u8],
    /// Bytes per block.
    pub size: usize,
}

impl BlockDevice for SliceDevice<'_> {
    fn block_size(&self) -> usize {
        self.size
    }

    fn block_count(&self) -> u64 {
        if self.size == 0 {
            0
        } else {
            (self.image.len() / self.size) as u64
        }
    }

    fn read_block(&self, index: u64, out: &mut [u8]) -> Result<(), ImageError> {
        if self.size == 0 || out.len() < self.size {
            return Err(ImageError::InvalidArgument);
        }
        let start = index
            .checked_mul(self.size as u64)
            .ok_or(ImageError::InvalidArgument)? as usize;
        let end = start.checked_add(self.size).ok_or(ImageError::InvalidArgument)?;
        let bytes = self.image.get(start..end).ok_or(ImageError::InvalidArgument)?;
        out[..self.size].copy_from_slice(bytes);
        Ok(())
    }
}

/// A device with no blocks: every read misses. The honest starting point
/// until hardware access lands.
pub struct EmptyDevice {
    /// Bytes per block (shape without content).
    pub size: usize,
}

impl BlockDevice for EmptyDevice {
    fn block_size(&self) -> usize {
        self.size
    }

    fn block_count(&self) -> u64 {
        0
    }

    fn read_block(&self, _index: u64, _out: &mut [u8]) -> Result<(), ImageError> {
        Err(ImageError::InvalidArgument)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_reads_blocks() {
        let image: Vec<u8> = (0..16u8).collect();
        let device = SliceDevice { image: &image, size: 4 };
        assert_eq!(device.block_count(), 4);
        let mut out = [0u8; 4];
        device.read_block(2, &mut out).unwrap();
        assert_eq!(out, [8, 9, 10, 11]);
        assert_eq!(
            device.read_block(4, &mut out),
            Err(ImageError::InvalidArgument)
        );
    }

    #[test]
    fn test_empty_device_misses() {
        let device = EmptyDevice { size: 512 };
        assert_eq!(device.block_count(), 0);
        let mut out = [0u8; 512];
        assert_eq!(
            device.read_block(0, &mut out),
            Err(ImageError::InvalidArgument)
        );
    }
}
