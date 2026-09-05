//! On-disk inode: the one-hundred-twenty-eight-byte record
//! (`type.h` `d_inode`, `inode.c` transfers).
//!
//! The record mirrors the Linux layout: mode, owner, size, three times,
//! deletion time, group, link count, sector count, flags, an operating
//! system union, fifteen block pointers (twelve direct plus single,
//! double, and triple indirect), generation, access lists, fragment
//! address, and a second union. Only the first half travels in the old
//! revision; the rest reads as zero. All integers are little-endian on
//! disk (`main.c:38-41` asserts a little-endian processor at startup).

/// Stored record size (`EXT2_GOOD_OLD_INODE_SIZE`, one hundred
/// twenty-eight bytes).
pub const RECORD_BYTES: usize = 128;
/// Block pointers per inode (`EXT2_N_BLOCKS`, fifteen: twelve direct
/// plus three indirect levels).
pub const BLOCK_POINTERS: usize = 15;
/// Direct pointers (`EXT2_NDIR_BLOCKS`, twelve).
pub const DIRECT_POINTERS: usize = 12;
/// Single indirect slot (`EXT2_IND_BLOCK`, twelve).
pub const SINGLE_INDIRECT_SLOT: usize = 12;
/// Double indirect slot (`EXT2_DIND_BLOCK`, thirteen).
pub const DOUBLE_INDIRECT_SLOT: usize = 13;
/// Triple indirect slot (`EXT2_TIND_BLOCK`, fourteen).
pub const TRIPLE_INDIRECT_SLOT: usize = 14;

/// In-memory view of the traveling fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiskInode {
    /// File mode.
    pub mode: u16,
    /// Owner user identifier (low sixteen bits).
    pub uid: u16,
    /// File size in bytes.
    pub size: u32,
    /// Access time.
    pub accessed: u32,
    /// Status-change time.
    pub changed: u32,
    /// Modification time.
    pub modified: u32,
    /// Deletion time (zero while linked).
    pub deleted_at: u32,
    /// Group identifier (low sixteen bits).
    pub gid: u16,
    /// Link count.
    pub links: u16,
    /// Five-hundred-twelve-byte sectors used.
    pub sectors: u32,
    /// File flags.
    pub flags: u32,
    /// Fifteen block pointers.
    pub blocks: [u32; BLOCK_POINTERS],
    /// Generation number.
    pub generation: u32,
}

/// Decode a record from little-endian bytes. Short buffers refuse: a
/// torn inode image must not yield a half-filled structure.
pub fn decode(image: &[u8]) -> Result<DiskInode, crate::InodeError> {
    use crate::InodeError;
    if image.len() < RECORD_BYTES {
        return Err(InodeError::Invalid);
    }
    let half = |at: usize| u16::from_le_bytes([image[at], image[at + 1]]);
    let word = |at: usize| u32::from_le_bytes([image[at], image[at + 1], image[at + 2], image[at + 3]]);
    let mut blocks = [0u32; BLOCK_POINTERS];
    for (slot, value) in blocks.iter_mut().enumerate() {
        *value = word(40 + slot * 4);
    }
    Ok(DiskInode {
        mode: half(0),
        uid: half(2),
        size: word(4),
        accessed: word(8),
        changed: word(12),
        modified: word(16),
        deleted_at: word(20),
        gid: half(24),
        links: half(26),
        sectors: word(28),
        flags: word(32),
        blocks,
        generation: word(100),
    })
}

/// Encode a record to little-endian bytes. Operating-system unions and
/// access lists travel as zero: this server neither reads nor writes
/// fragments, access lists, or high identifier bits.
pub fn encode(inode: &DiskInode) -> [u8; RECORD_BYTES] {
    let mut out = [0u8; RECORD_BYTES];
    out[0..2].copy_from_slice(&inode.mode.to_le_bytes());
    out[2..4].copy_from_slice(&inode.uid.to_le_bytes());
    out[4..8].copy_from_slice(&inode.size.to_le_bytes());
    out[8..12].copy_from_slice(&inode.accessed.to_le_bytes());
    out[12..16].copy_from_slice(&inode.changed.to_le_bytes());
    out[16..20].copy_from_slice(&inode.modified.to_le_bytes());
    out[20..24].copy_from_slice(&inode.deleted_at.to_le_bytes());
    out[24..26].copy_from_slice(&inode.gid.to_le_bytes());
    out[26..28].copy_from_slice(&inode.links.to_le_bytes());
    out[28..32].copy_from_slice(&inode.sectors.to_le_bytes());
    out[32..36].copy_from_slice(&inode.flags.to_le_bytes());
    for (slot, value) in inode.blocks.iter().enumerate() {
        out[40 + slot * 4..44 + slot * 4].copy_from_slice(&value.to_le_bytes());
    }
    out[100..104].copy_from_slice(&inode.generation.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_constants() {
        assert_eq!(RECORD_BYTES, 128);
        assert_eq!(BLOCK_POINTERS, 15);
        assert_eq!(DIRECT_POINTERS, 12);
        assert_eq!(TRIPLE_INDIRECT_SLOT, 14);
    }

    #[test]
    fn test_round_trip() {
        let mut inode = DiskInode {
            mode: 0o100644,
            uid: 100,
            size: 1234,
            links: 2,
            sectors: 4,
            generation: 9,
            ..DiskInode::default()
        };
        inode.blocks[0] = 55;
        inode.blocks[14] = 77;
        let bytes = encode(&inode);
        assert_eq!(bytes.len(), 128);
        assert_eq!(decode(&bytes).unwrap(), inode);
    }

    #[test]
    fn test_short_buffer_refuses() {
        assert_eq!(decode(&[0u8; 64]).unwrap_err(), crate::InodeError::Invalid);
    }
}
