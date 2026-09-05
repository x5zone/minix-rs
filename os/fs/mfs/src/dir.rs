//! Directory blocks: entry codec and single-component search.
//!
//! C correspondence: `search_dir` plus the `struct direct` layout in
//! `minix3/minix/fs/mfs/path.c:92-240` and `mfsdir.h:15-18`. A directory is
//! a flat array of sixty-four-byte entries ordered by slot; lookup, entry,
//! deletion, and emptiness check are four modes of one walk. Name comparison
//! only ever sees the first sixty bytes: truncation happens here and nowhere
//! else.
//!
//! The walk runs over caller-supplied block images in directory order. Block
//! fetching (`get_block_map`) and allocation (`new_block`) belong to the
//! data path (documents 14 and 15), which will feed real cache bytes into
//! the same walk; until then the images parameter keeps this stage complete
//! and tested without them.

use alloc::vec::Vec;

use minix_types::{EFBIG, EINVAL, ENOENT, ENOTDIR, ENOTEMPTY, EROFS, Errno};

use crate::inode::{NO_LINK, TYPE_DIRECTORY, TYPE_MASK};

/// Name bytes per entry (`MFS_DIRSIZ`, sixty).
pub const ENTRY_NAME_SIZE: usize = 60;
/// Entry size in bytes (four-byte number plus sixty-byte name, packed).
pub const ENTRY_SIZE: usize = 64;

/// Absent entry marker (`NO_ENTRY`, zero).
pub const NO_ENTRY_NUMBER: u32 = 0;

/// Search modes (`LOOK_UP`, `ENTER`, `DELETE`, `IS_EMPTY` in `const.h:34-37`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOp {
    /// Find a name, report its number.
    Lookup,
    /// Add a name with a number, extending the directory if needed.
    Enter,
    /// Remove a name, stashing its number for recovery.
    Delete,
    /// Succeed only when nothing but dot entries remains.
    IsEmpty,
}

/// Why a directory search failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirError {
    /// Target is not a directory (`ENOTDIR`, `path.c:116-118`).
    NotDirectory,
    /// Write on a read-only mount (`EROFS`, `path.c:120-121`).
    ReadOnly,
    /// Name absent (`ENOENT`).
    NotFound,
    /// Directory not empty (`ENOTEMPTY`, `IS_EMPTY` mode).
    NotEmpty,
    /// Directory size would overflow its slot count (`EFBIG`).
    TooBig,
    /// Bad input (empty name for advance, short images).
    Invalid,
}

impl DirError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::NotDirectory => Errno::from_i32(ENOTDIR),
            Self::ReadOnly => Errno::from_i32(EROFS),
            Self::NotFound => Errno::from_i32(ENOENT),
            Self::NotEmpty => Errno::from_i32(ENOTEMPTY),
            Self::TooBig => Errno::from_i32(EFBIG),
            Self::Invalid => Errno::from_i32(EINVAL),
        }
    }
}

/// One directory entry: number plus blank-padded name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirEntry {
    /// Inode number; zero marks a free slot.
    pub ino: u32,
    /// Name bytes; unused tail is zero.
    pub name: [u8; ENTRY_NAME_SIZE],
}

impl DirEntry {
    /// Parse one sixty-four-byte record (number little-endian).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DirError> {
        if bytes.len() < ENTRY_SIZE {
            return Err(DirError::Invalid);
        }
        let ino = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let mut name = [0u8; ENTRY_NAME_SIZE];
        name.copy_from_slice(&bytes[4..64]);
        Ok(Self { ino, name })
    }

    /// Serialize one sixty-four-byte record.
    pub fn to_bytes(&self) -> [u8; ENTRY_SIZE] {
        let mut out = [0u8; ENTRY_SIZE];
        out[0..4].copy_from_slice(&self.ino.to_le_bytes());
        out[4..64].copy_from_slice(&self.name);
        out
    }

    /// Name length up to the first zero byte.
    pub fn name_len(&self) -> usize {
        self.name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(ENTRY_NAME_SIZE)
    }

    /// Whether this slot holds `.` or `..`.
    pub fn is_dot_entry(&self) -> bool {
        let length = self.name_len();
        (length == 1 && self.name[0] == b'.')
            || (length == 2 && self.name[0] == b'.' && self.name[1] == b'.')
    }
}

/// Mutable directory scan state: byte size plus the enter hint.
///
/// Mirrors `i_size` and `i_last_dpos` as the walk consults them. The caller
/// (owning the inode slot) copies these in and out around the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirScan {
    /// Directory size in bytes.
    pub size: u64,
    /// Byte offset where the next enter starts looking.
    pub last_dpos: u64,
}

/// Outcome of a successful search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOutcome {
    /// Lookup hit; carries the inode number.
    Found(u32),
    /// Enter stored the name.
    Entered,
    /// Delete cleared the name.
    Deleted,
    /// Is-empty confirmed emptiness.
    Empty,
}

/// Walk directory block images in one of the four modes.
///
/// C: `search_dir` (`path.c:92-240`). `blocks` holds the directory content
/// in order; every image must be exactly `block_size` bytes. `scan.size`
/// bounds the walk, `scan.last_dpos` seeds enter scans (a few percent
/// faster, `path.c:208-211`). `number` is input for enter, output for
/// lookup. Read-only mounts refuse enter and delete. Only the first sixty
/// name bytes ever compare.
///
/// Delete stashes the erased number inside the name tail (sixty minus four)
/// for recovery (`path.c:173-176`); enter on a full tail appends a zeroed
/// block and grows the size, reporting directory-too-big on slot-count
/// overflow (`path.c:216-238`).
#[allow(clippy::too_many_arguments)]
pub fn search_blocks(
    blocks: &mut Vec<Vec<u8>>,
    block_size: usize,
    scan: &mut DirScan,
    is_dir: bool,
    read_only: bool,
    op: SearchOp,
    name: &[u8],
    number: &mut u32,
) -> Result<SearchOutcome, DirError> {
    if !is_dir {
        return Err(DirError::NotDirectory);
    }
    if (op == SearchOp::Enter || op == SearchOp::Delete) && read_only {
        return Err(DirError::ReadOnly);
    }
    for image in blocks.iter() {
        if image.len() != block_size {
            return Err(DirError::Invalid);
        }
    }
    let entries_per_block = block_size / ENTRY_SIZE;
    if entries_per_block == 0 {
        return Err(DirError::Invalid);
    }
    let old_slots = scan.size / ENTRY_SIZE as u64;
    let mut new_slots = 0u64;
    let mut entered_at: Option<(usize, usize)> = None;

    let mut position = 0u64;
    if op == SearchOp::Enter && scan.last_dpos < scan.size {
        position = scan.last_dpos;
        // Count from the hint like the C code: entries before it were
        // already visited in an earlier scan (`path.c:130-133`).
        new_slots = position / ENTRY_SIZE as u64;
    }
    // Block index and entry index walk together; positions stay in bytes.
    let mut block_index = (position as usize / block_size).min(blocks.len());
    while position < scan.size {
        if block_index >= blocks.len() {
            break;
        }
        // Byte offset of this block's start: the enter hint for later.
        let block_start = block_index * block_size;
        let mut entry_index = if position > block_start as u64 {
            ((position - block_start as u64) / ENTRY_SIZE as u64) as usize
        } else {
            0
        };
        position = block_start as u64 + entry_index as u64 * ENTRY_SIZE as u64;
        let mut stop_block = false;
        while entry_index < entries_per_block {
            // Count every visited position first, like the C pre-increment:
            // passing the old count means past the directory end.
            new_slots += 1;
            if new_slots > old_slots {
                if op == SearchOp::Enter {
                    entered_at = Some((block_index, entry_index));
                    stop_block = true;
                }
                break;
            }
            let base = entry_index * ENTRY_SIZE;
            let record = DirEntry::from_bytes(&blocks[block_index][base..base + ENTRY_SIZE])?;
            match op {
                SearchOp::Lookup => {
                    if record.ino != NO_ENTRY_NUMBER && names_equal(&record.name, name) {
                        *number = record.ino;
                        return Ok(SearchOutcome::Found(record.ino));
                    }
                }
                SearchOp::IsEmpty => {
                    if record.ino != NO_ENTRY_NUMBER && !record.is_dot_entry() {
                        return Err(DirError::NotEmpty);
                    }
                }
                SearchOp::Delete => {
                    if record.ino != NO_ENTRY_NUMBER && names_equal(&record.name, name) {
                        // Stash the number in the name tail, then clear.
                        let mut cleared = record;
                        let stash_at = ENTRY_NAME_SIZE - 4;
                        cleared.name[stash_at..].copy_from_slice(&record.ino.to_le_bytes());
                        cleared.ino = NO_ENTRY_NUMBER;
                        let bytes = cleared.to_bytes();
                        blocks[block_index][base..base + ENTRY_SIZE].copy_from_slice(&bytes);
                        if position < scan.last_dpos {
                            scan.last_dpos = position;
                        }
                        return Ok(SearchOutcome::Deleted);
                    }
                }
                SearchOp::Enter => {
                    if record.ino == 0 {
                        entered_at = Some((block_index, entry_index));
                        stop_block = true;
                        break;
                    }
                }
            }
            entry_index += 1;
            position += ENTRY_SIZE as u64;
        }
        if stop_block {
            position = block_start as u64;
            break;
        }
        block_index += 1;
        position = block_index as u64 * block_size as u64;
    }

    if op != SearchOp::Enter {
        return if op == SearchOp::IsEmpty {
            Ok(SearchOutcome::Empty)
        } else {
            Err(DirError::NotFound)
        };
    }

    // Enter resumes from the block start next time (`path.c:208-211`).
    scan.last_dpos = position.min(scan.size + ENTRY_SIZE as u64);
    if entered_at.is_none() {
        // No free slot anywhere: extend by one entry (`path.c:216-223`).
        new_slots += 1;
        if new_slots > u32::MAX as u64 {
            return Err(DirError::TooBig);
        }
        blocks.push(alloc::vec![0u8; block_size]);
        entered_at = Some((blocks.len() - 1, 0));
    }
    let (block_index, entry_index) = entered_at.expect("enter always lands");
    let mut record = DirEntry {
        ino: NO_ENTRY_NUMBER,
        name: [0u8; ENTRY_NAME_SIZE],
    };
    let take = name.len().min(ENTRY_NAME_SIZE);
    record.name[..take].copy_from_slice(&name[..take]);
    record.ino = *number;
    let base = entry_index * ENTRY_SIZE;
    let bytes = record.to_bytes();
    blocks[block_index][base..base + ENTRY_SIZE].copy_from_slice(&bytes);
    if new_slots > old_slots {
        scan.size = new_slots * ENTRY_SIZE as u64;
    }
    Ok(SearchOutcome::Entered)
}

/// Compare a stored name against a query over the first sixty bytes.
///
/// Mirrors the bounded `strncmp` in `search_dir` (`path.c:161-162`): bytes
/// past the query length count as zero, so short queries match blank-padded
/// records and vice versa.
fn names_equal(stored: &[u8; ENTRY_NAME_SIZE], query: &[u8]) -> bool {
    for index in 0..ENTRY_NAME_SIZE {
        let left = stored[index];
        let right = if index < query.len() { query[index] } else { 0 };
        if left != right {
            return false;
        }
        if left == 0 {
            return true;
        }
    }
    true
}

/// Single-component lookup over fetched directory images.
///
/// The `fs_lookup` shape (`path.c:16-42`) with block fetching factored out:
/// the caller supplies the directory's block images in order (the data path
/// feeds real cache bytes later). Empty names, removed directories, and
/// misses map to the same errors as `advance` (`path.c:62-76`). Opening the
/// child inode stays with the caller, which owns the table.
pub fn lookup_name(
    dir_mode: u16,
    dir_nlinks: u16,
    dir_blocks: &mut Vec<Vec<u8>>,
    block_size: usize,
    dir_size: u64,
    name: &[u8],
    found: &mut u32,
) -> Result<(), DirError> {
    if name.is_empty() {
        return Err(DirError::NotFound);
    }
    if dir_nlinks == NO_LINK {
        return Err(DirError::NotFound);
    }
    if dir_mode as u32 & TYPE_MASK != TYPE_DIRECTORY {
        return Err(DirError::NotDirectory);
    }
    let mut scan = DirScan {
        size: dir_size,
        last_dpos: 0,
    };
    match search_blocks(
        dir_blocks,
        block_size,
        &mut scan,
        true,
        true,
        SearchOp::Lookup,
        name,
        found,
    ) {
        Ok(SearchOutcome::Found(_)) => Ok(()),
        Ok(_) => Err(DirError::NotFound),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;

    const BLOCK: usize = 64;

    /// Empty directory: no images, zero size. Blocks arrive through
    /// extension, exactly like allocated blocks arrive in the C code.
    fn empty_dir() -> Vec<Vec<u8>> {
        Vec::new()
    }

    fn scan_of(size: u64) -> DirScan {
        DirScan { size, last_dpos: 0 }
    }

    fn enter(
        blocks: &mut Vec<Vec<u8>>,
        scan: &mut DirScan,
        name: &[u8],
        ino: u32,
    ) -> Result<SearchOutcome, DirError> {
        let mut number = ino;
        search_blocks(
            blocks,
            BLOCK,
            scan,
            true,
            false,
            SearchOp::Enter,
            name,
            &mut number,
        )
    }

    #[test]
    fn test_entry_codec_roundtrip() {
        let mut name = [0u8; 60];
        name[..5].copy_from_slice(b"hello");
        let entry = DirEntry { ino: 7, name };
        let back = DirEntry::from_bytes(&entry.to_bytes()).unwrap();
        assert_eq!(entry, back);
        assert_eq!(back.name_len(), 5);
        assert!(!back.is_dot_entry());
        let dot = DirEntry {
            ino: 1,
            name: {
                let mut name = [0u8; 60];
                name[0] = b'.';
                name
            },
        };
        assert!(dot.is_dot_entry());
        assert!(DirEntry::from_bytes(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_lookup_hit_and_miss() {
        let mut blocks = empty_dir();
        let mut scan = scan_of(0);
        enter(&mut blocks, &mut scan, b"hosts", 3).unwrap();
        let mut found = 0;
        let outcome = search_blocks(
            &mut blocks,
            BLOCK,
            &mut scan,
            true,
            false,
            SearchOp::Lookup,
            b"hosts",
            &mut found,
        )
        .unwrap();
        assert_eq!(outcome, SearchOutcome::Found(3));
        assert_eq!(found, 3);
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                true,
                false,
                SearchOp::Lookup,
                b"nope",
                &mut found
            )
            .unwrap_err(),
            DirError::NotFound
        );
        // Empty query never matches (advance rule).
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                true,
                false,
                SearchOp::Lookup,
                b"",
                &mut found
            )
            .unwrap_err(),
            DirError::NotFound
        );
    }

    #[test]
    fn test_name_truncation_at_sixty() {
        let mut blocks = empty_dir();
        let mut scan = scan_of(0);
        let long = alloc::vec![b'a'; 70];
        enter(&mut blocks, &mut scan, &long, 9).unwrap();
        // The first sixty bytes identify it; byte sixty-one is invisible.
        let mut found = 0;
        let outcome = search_blocks(
            &mut blocks,
            BLOCK,
            &mut scan,
            true,
            false,
            SearchOp::Lookup,
            &long[..60],
            &mut found,
        )
        .unwrap();
        assert_eq!(outcome, SearchOutcome::Found(9));
    }

    #[test]
    fn test_delete_stashes_and_reuses() {
        let mut blocks = empty_dir();
        let mut scan = scan_of(0);
        enter(&mut blocks, &mut scan, b"tmp", 4).unwrap();
        let mut number = 0;
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                true,
                false,
                SearchOp::Delete,
                b"tmp",
                &mut number
            )
            .unwrap(),
            SearchOutcome::Deleted
        );
        // Gone now.
        let mut found = 0;
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                true,
                false,
                SearchOp::Lookup,
                b"tmp",
                &mut found
            )
            .unwrap_err(),
            DirError::NotFound
        );
        // The slot is free again: the next enter reuses it without growing.
        let size_before = scan.size;
        enter(&mut blocks, &mut scan, b"new", 5).unwrap();
        assert_eq!(scan.size, size_before);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_is_empty_skips_dots() {
        let mut blocks = empty_dir();
        // Two dot entries occupy the only block: empty.
        let mut scan = scan_of(0);
        enter(&mut blocks, &mut scan, b".", 1).unwrap();
        enter(&mut blocks, &mut scan, b"..", 1).unwrap();
        let mut number = 0;
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                true,
                false,
                SearchOp::IsEmpty,
                b"",
                &mut number
            )
            .unwrap(),
            SearchOutcome::Empty
        );
        // One real entry spoils it.
        enter(&mut blocks, &mut scan, b"file", 2).unwrap();
        // The new entry extended the directory (block size sixty-four holds
        // one entry); emptiness now fails.
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                true,
                false,
                SearchOp::IsEmpty,
                b"",
                &mut number
            )
            .unwrap_err(),
            DirError::NotEmpty
        );
    }

    #[test]
    fn test_enter_extends_directory() {
        // Sixty-four-byte blocks hold one entry each: the second enter must
        // extend.
        let mut blocks = empty_dir();
        let mut scan = scan_of(0);
        enter(&mut blocks, &mut scan, b"a", 1).unwrap();
        assert_eq!(scan.size, 64);
        enter(&mut blocks, &mut scan, b"b", 2).unwrap();
        assert_eq!(scan.size, 128);
        assert_eq!(blocks.len(), 2);
        let mut found = 0;
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                true,
                false,
                SearchOp::Lookup,
                b"b",
                &mut found
            )
            .unwrap(),
            SearchOutcome::Found(2)
        );
    }

    #[test]
    fn test_guards() {
        let mut blocks = empty_dir();
        let mut scan = scan_of(0);
        let mut number = 1;
        // Not a directory.
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                false,
                false,
                SearchOp::Lookup,
                b"x",
                &mut number
            )
            .unwrap_err(),
            DirError::NotDirectory
        );
        // Read-only refuses writes.
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                true,
                true,
                SearchOp::Enter,
                b"x",
                &mut number
            )
            .unwrap_err(),
            DirError::ReadOnly
        );
        assert_eq!(
            search_blocks(
                &mut blocks,
                BLOCK,
                &mut scan,
                true,
                true,
                SearchOp::Delete,
                b"x",
                &mut number
            )
            .unwrap_err(),
            DirError::ReadOnly
        );
        // Bad image sizes refused (nonzero size forces the walk into it).
        let mut bad = alloc::vec![alloc::vec![0u8; 10]];
        scan.size = ENTRY_SIZE as u64;
        assert_eq!(
            search_blocks(
                &mut bad,
                BLOCK,
                &mut scan,
                true,
                false,
                SearchOp::Lookup,
                b"x",
                &mut number
            )
            .unwrap_err(),
            DirError::Invalid
        );
    }

    #[test]
    fn test_lookup_name_helper() {
        let mut blocks = empty_dir();
        let mut scan = scan_of(0);
        enter(&mut blocks, &mut scan, b"etc", 2).unwrap();
        let mut found = 0;
        lookup_name(
            0o040755,
            2,
            &mut blocks,
            BLOCK,
            scan.size,
            b"etc",
            &mut found,
        )
        .unwrap();
        assert_eq!(found, 2);
        assert!(lookup_name(0o040755, 2, &mut blocks, BLOCK, scan.size, b"", &mut found).is_err());
        assert!(
            lookup_name(
                0o100644,
                2,
                &mut blocks,
                BLOCK,
                scan.size,
                b"etc",
                &mut found
            )
            .is_err()
        );
        assert!(
            lookup_name(
                0o040755,
                0,
                &mut blocks,
                BLOCK,
                scan.size,
                b"etc",
                &mut found
            )
            .is_err()
        );
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::{EFBIG, EINVAL, ENOENT, ENOTDIR, ENOTEMPTY, EROFS};
        assert_eq!(DirError::NotDirectory.to_errno().to_i32(), ENOTDIR);
        assert_eq!(DirError::ReadOnly.to_errno().to_i32(), EROFS);
        assert_eq!(DirError::NotFound.to_errno().to_i32(), ENOENT);
        assert_eq!(DirError::NotEmpty.to_errno().to_i32(), ENOTEMPTY);
        assert_eq!(DirError::TooBig.to_errno().to_i32(), EFBIG);
        assert_eq!(DirError::Invalid.to_errno().to_i32(), EINVAL);
    }
}
