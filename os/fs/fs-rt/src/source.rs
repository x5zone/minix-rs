//! The fail-closed block source for servers whose driver channel is
//! pending.
//!
//! C correspondence: none — the C file servers reach their block driver
//! through the bdev label channel from the first mount. The Rust block seam
//! (`minix-fs`'s `BlockSource` to a real block driver) is the separately
//! tracked E-FSBDEV item; until it lands, a server wired with this source
//! starts, handshakes, dispatches, and answers every block read or write
//! with "input/output error" — honest failure, never a pretend disk. The
//! block *size* is pure configuration (512 is MFS's smallest block,
//! `minix3/minix/fs/mfs/const.h`'s block era) and is reported without
//! pretending any medium exists.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_source_refuses_every_transfer() {
        let mut src = PendingBlockSource;
        assert_eq!(src.block_size(), 512);
        let key = BlockKey::new(1, 512);
        assert_eq!(src.read_block(key, &mut [0u8; 512]).unwrap_err(), Errno::from_i32(EIO));
        assert_eq!(src.write_block(key, &[0u8; 512]).unwrap_err(), Errno::from_i32(EIO));
    }
}
