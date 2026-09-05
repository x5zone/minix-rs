//! Block address decomposition across three indirection levels
//! (`read.c` address thresholds, `write.c` symmetric writer).
//!
//! Twelve direct blocks come first, then one single-indirect block of
//! addresses, then a double-indirect square, then a triple-indirect
//! cube. Given the addresses-per-block count, any file block number
//! decomposes into exactly one path: direct slot, single slot plus
//! index, double slot plus two indexes, or triple slot plus three
//! indexes. Decomposition is pure arithmetic over the thresholds the C
//! code computes once per lookup (`read.c:218-228`); storage walks stay
//! with the caller.

use super::inode::{DIRECT_POINTERS, DOUBLE_INDIRECT_SLOT, SINGLE_INDIRECT_SLOT, TRIPLE_INDIRECT_SLOT};

/// Where a file block lives, as index steps from the inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockPath {
    /// Direct pointer slot.
    Direct {
        /// Slot in the pointer array.
        slot: usize,
    },
    /// Single indirect: pointer slot plus index inside that block.
    Single {
        /// Slot holding the indirect block.
        slot: usize,
        /// Index inside the indirect block.
        index: u64,
    },
    /// Double indirect: pointer slot plus two indexes.
    Double {
        /// Slot holding the double-indirect block.
        slot: usize,
        /// Index inside the double-indirect block.
        outer: u64,
        /// Index inside the single-indirect block.
        inner: u64,
    },
    /// Triple indirect: pointer slot plus three indexes.
    Triple {
        /// Slot holding the triple-indirect block.
        slot: usize,
        /// Index inside the triple-indirect block.
        outer: u64,
        /// Index inside the double-indirect block.
        middle: u64,
        /// Index inside the single-indirect block.
        inner: u64,
    },
}

/// Decompose a file block number (`read_map` thresholds). Addresses per
/// block of zero refuses: without it no level has a size. The triple
/// cube bounds the file; past it reports too big (`EFBIG` at the
/// caller's layer).
pub fn decompose(file_block: u64, addresses_per_block: u64) -> Result<BlockPath, crate::MappingError> {
    use crate::MappingError;
    if addresses_per_block == 0 {
        return Err(MappingError::Invalid);
    }
    let direct = DIRECT_POINTERS as u64;
    if file_block < direct {
        return Ok(BlockPath::Direct { slot: file_block as usize });
    }
    let mut rest = file_block - direct;
    if rest < addresses_per_block {
        return Ok(BlockPath::Single { slot: SINGLE_INDIRECT_SLOT, index: rest });
    }
    rest -= addresses_per_block;
    let square = addresses_per_block.saturating_mul(addresses_per_block);
    if rest < square {
        return Ok(BlockPath::Double {
            slot: DOUBLE_INDIRECT_SLOT,
            outer: rest / addresses_per_block,
            inner: rest % addresses_per_block,
        });
    }
    rest -= square;
    let cube = square.saturating_mul(addresses_per_block);
    if rest < cube {
        return Ok(BlockPath::Triple {
            slot: TRIPLE_INDIRECT_SLOT,
            outer: rest / square,
            middle: (rest % square) / addresses_per_block,
            inner: rest % addresses_per_block,
        });
    }
    Err(MappingError::TooBig)
}

/// Largest file block number the triple cube addresses.
pub fn last_addressable(addresses_per_block: u64) -> u64 {
    let direct = DIRECT_POINTERS as u64;
    direct + addresses_per_block
        + addresses_per_block.saturating_mul(addresses_per_block)
        + addresses_per_block
            .saturating_mul(addresses_per_block)
            .saturating_mul(addresses_per_block)
        - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_zone() {
        assert_eq!(decompose(0, 256).unwrap(), BlockPath::Direct { slot: 0 });
        assert_eq!(decompose(11, 256).unwrap(), BlockPath::Direct { slot: 11 });
    }

    #[test]
    fn test_single_then_double() {
        assert_eq!(
            decompose(12, 256).unwrap(),
            BlockPath::Single { slot: 12, index: 0 }
        );
        assert_eq!(
            decompose(12 + 255, 256).unwrap(),
            BlockPath::Single { slot: 12, index: 255 }
        );
        assert_eq!(
            decompose(12 + 256, 256).unwrap(),
            BlockPath::Double { slot: 13, outer: 0, inner: 0 }
        );
    }

    #[test]
    fn test_triple_entry_and_cap() {
        let per_block = 256u64;
        let triple_start = 12 + per_block + per_block * per_block;
        assert_eq!(
            decompose(triple_start, per_block).unwrap(),
            BlockPath::Triple { slot: 14, outer: 0, middle: 0, inner: 0 }
        );
        assert_eq!(
            decompose(triple_start - 1, per_block).unwrap(),
            BlockPath::Double {
                slot: 13,
                outer: 255,
                inner: 255
            }
        );
        assert_eq!(
            decompose(last_addressable(per_block) + 1, per_block).unwrap_err(),
            crate::MappingError::TooBig
        );
        assert_eq!(decompose(0, 0).unwrap_err(), crate::MappingError::Invalid);
    }
}
