//! The decisive half of `mkfs.mfs`: layout computation for an empty version-3 file
//! system, image construction, and prototype seeding.
//!
//! C counterpart: `minix3/minix/usr.sbin/mkfs.mfs/mkfs.c` — layout arithmetic
//! follows `super()` (mkfs.c:602-716), the default inode count follows
//! `main`'s KB ladder (mkfs.c:344-357), the root directory follows `rootdir`
//! (mkfs.c:718-731: one data zone, two entries `.` and `..`, link count two).
//! Prototype tree filling follows `eat_dir`/`eat_file` (mkfs.c:765-870) and the
//! three helper groups for directories, inodes and allocation
//! (mkfs.c:873-1165); host file content is injected through the [`ProtoHost`]
//! seam — the decisive half never opens files, the thin shell supplies the
//! reader.
//!
//! # Layout (default face: 4096 bytes per block, `zone_shift = 0`)
//!
//! ```text
//! block 0        boot block (all zero; the superblock sits at a 1024-byte
//!                offset inside it)
//! block 2..      inode bitmap (imap_blocks blocks)
//! ..             zone bitmap (zmap_blocks blocks)
//! ..             inode table (inode_table_blocks blocks, bs/64 each)
//! data area starts at first_data_zone
//! ```
//!
//! Bitmaps are low-bit-first per Minix convention: bit 0 is always set
//! (inode 0 and zone 0 are "allocated but unused"), and the root inode
//! (number 1) and the root data zone each occupy bit 1.
//!
//! # Prototype grammar (C `get_line`/`mode_con`, mkfs.c:1222-1256 and :1195-1215)
//!
//! ```text
//! <boot block line, read and discarded>
//! <block count> <inode count>
//! <type><setuid><setgid><three octal digits> <uid> <gid>     <- root directory
//! <name> <mode> <uid> <gid> [args...]                       <- entries per directory, `$` terminates
//! ```
//!
//! The mode string is six characters (C `mkproto` mkproto.c:303-315 emits it
//! with `%c%c%c%03o`): a type letter (`d`/`b`/`c`/`s`/`l`/`-`), a set-uid
//! marker (`u` or `-`), a set-gid marker (`g` or `-`), and three octal
//! permission digits. Entry lines dispatch on the type letter (`eat_dir`
//! mkfs.c:793 ff.): directories recurse, block/char devices carry
//! `major minor [size-in-blocks]`, symlinks carry a target, and everything
//! else is a regular file read from the host. Names longer than 60 bytes are
//! truncated (same shape as C `dir_try_enter`'s strncpy).

use crate::inode::{DiskInode, DIRECTORY_ENTRY_SIZE, NAME_CAPACITY, TOTAL_ZONES, TYPE_DIRECTORY};
use crate::superblock::{DiskSuperblock, DIRECT_ZONE_COUNT, FLAG_CLEAN, MAGIC_V3, START_BLOCK};
use alloc::vec::Vec;
use alloc::vec;

/// Default block size (C `DEFAULT_BLOCK_SIZE`, the 4096 in `usr.sbin/mkfs.mfs/v3/const.h:9`).
pub const DEFAULT_BLOCK_SIZE: u64 = 4096;
/// Minimum acceptable block count (C `if (blocks < 5)`, mkfs.c:356).
pub const MIN_BLOCKS: u64 = 5;
/// Bits per block (`FS_BITS_PER_BLOCK`).
fn bits_per_block(block_size: u64) -> u64 {
    block_size * 8
}

/// Disk inodes per block (`INODES_PER_BLOCK(b) = b / 64`, const.h:39).
fn inodes_per_block(block_size: u64) -> u64 {
    block_size / crate::superblock::INODE_DISK_SIZE as u64
}

/// Indirect entries per block (`INDIRECTS(b) = b / 4`, const.h:38).
fn indirects_per_zone(block_size: u64) -> u64 {
    block_size / 4
}

/// Number of blocks a bitmap occupies (C `bitmapsize`, mkfs.c: round up to whole blocks).
fn bitmap_size(nr_bits: u64, block_size: u64) -> u64 {
    let per = bits_per_block(block_size);
    let blocks = nr_bits / per;
    if blocks * per < nr_bits {
        blocks + 1
    } else {
        blocks
    }
}

/// Everything layout computation yields — shared by image construction and `mfs` mount validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MkfsPlan {
    /// Block size in bytes.
    pub block_size: u64,
    /// Total block count.
    pub blocks: u64,
    /// Total zone count (equals the block count when `zone_shift = 0`).
    pub zones: u64,
    /// Usable inode count (already rounded up to a multiple of inodes per block).
    pub inodes: u64,
    /// Inode bitmap block count.
    pub inode_map_blocks: u64,
    /// Zone bitmap block count.
    pub zone_map_blocks: u64,
    /// Inode table block count.
    pub inode_table_blocks: u64,
    /// Number of the first data zone.
    pub first_data_zone: u64,
    /// Maximum file size.
    pub max_size: u64,
}

/// Why layout computation failed (messages match C's `errx` text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MkfsError {
    /// Too few blocks (C "Block count too small", mkfs.c:356).
    TooSmall,
    /// Too few inodes (C "Inode count too small", mkfs.c:358).
    TooFewInodes,
    /// Bitmaps squeeze out the data area (C "bit maps too large", mkfs.c:626).
    MapsTooLarge,
    /// The image buffer cannot hold the whole file system.
    BufferTooSmall,
    /// The block argument is smaller than what the prototype header declares (the errx at C mkfs.c:305-308).
    ProtoTooLarge {
        /// Block count given by the caller (C's `%d`).
        given: u64,
        /// Block count declared by the prototype header (C's `%ld`).
        needed: u64,
    },
    /// Inodes exhausted (C alloc_inode's "File system does not have enough
    /// inodes", mkfs.c:1118-1120).
    NotEnoughInodes {
        /// Total inodes the file system holds (the `%llu` in C's message).
        have: u64,
    },
    /// Data zones exhausted (C alloc_zone's "File system not big enough for all
    /// the files", mkfs.c:1157-1158).
    NotEnoughZones,
    /// Prototype line too long / too many words / missing words / bad number —
    /// the C counterpart is `get_line`'s "Line too long" (mkfs.c:1236-1237) plus
    /// atoi's silent garbage; everything here is rejected instead of reproducing
    /// that garbage.
    ProtoBadLine,
    /// Prototype mode string is not in the seven-character form (C `mode_con`
    /// reads past the separator for short strings — a latent out-of-bounds read;
    /// the Rust side rejects it as a grammar error).
    ProtoBadMode,
    /// Prototype ends before `$` (C get_line's "Unexpected end-of-file",
    /// mkfs.c:1239-1240).
    ProtoUnexpectedEnd,
    /// Symlink target is not shorter than one block (C enter_symlink's "symlink too long",
    /// mkfs.c:736-738).
    SymlinkTooLong,
    /// A directory cannot hold a new entry even beyond single indirect (C enter_dir's pexit,
    /// mkfs.c:947-949).
    DirectoryTooLarge,
    /// File exceeds double indirect (C add_zone's pexit, mkfs.c:1052-1053).
    FileTooLarge,
    /// Link count overflow (C incr_link's "Too many links to a directory",
    /// mkfs.c:1071-1072).
    TooManyLinks,
    /// Inode size overflow (C incr_size's "File has become too big to be
    /// handled by MFS", mkfs.c:1096-1098).
    SizeTooLarge,
    /// Host file cannot be read (C eat_dir's "Can't open %s"; the thin shell records
    /// the path — this enum carries no data).
    HostFile,
}

/// Default inode count KB ladder (C `main` mkfs.c:344-357: one per 2 KB,
/// sparser as capacity grows, finally rounded up to a multiple of inodes per block).
pub fn default_inode_count(blocks: u64, block_size: u64) -> u64 {
    let kb = blocks * block_size / 1024;
    let per_kb = if kb >= 1_000_000_000 {
        12
    } else if kb >= 100_000_000 {
        10
    } else if kb >= 10_000_000 {
        8
    } else if kb >= 1_000_000 {
        6
    } else if kb >= 100_000 {
        4
    } else {
        2
    };
    let raw = kb / per_kb;
    let per_block = inodes_per_block(block_size);
    raw.div_ceil(per_block) * per_block
}

/// Layout computation (the arithmetic half of C `super`, mkfs.c:602-642; `zone_shift = 0`).
pub fn plan_layout(
    blocks: u64,
    inodes: Option<u64>,
    block_size: u64,
) -> Result<MkfsPlan, MkfsError> {
    if blocks < MIN_BLOCKS {
        return Err(MkfsError::TooSmall);
    }
    let per_block = inodes_per_block(block_size);
    let inodes = match inodes {
        Some(n) => n,
        None => default_inode_count(blocks, block_size),
    };
    if inodes < 1 {
        return Err(MkfsError::TooFewInodes);
    }
    let inodes = inodes.div_ceil(per_block) * per_block;
    let zones = blocks;
    let inode_map_blocks = bitmap_size(1 + inodes, block_size);
    let zone_map_blocks = bitmap_size(zones, block_size);
    let inode_table_blocks = inodes / per_block;
    let first_data_zone = crate::superblock::START_BLOCK + inode_map_blocks + zone_map_blocks
        + inode_table_blocks;
    if first_data_zone >= zones {
        return Err(MkfsError::MapsTooLarge);
    }
    // Maximum file size: `NR_DZONES + ind + ind²` zones, capped at `INT32_MAX`
    // (the `MAX_MAX_SIZE` face of C mkfs.c:646-655).
    let ind = indirects_per_zone(block_size);
    let zo = crate::superblock::DIRECT_ZONE_COUNT as u64
        + ind
        + ind.checked_mul(ind).ok_or(MkfsError::MapsTooLarge)?;
    let max_size = if (i32::MAX as u64) / block_size < zo {
        i32::MAX as u64
    } else {
        zo * block_size
    };
    Ok(MkfsPlan {
        block_size,
        blocks,
        zones,
        inodes,
        inode_map_blocks,
        zone_map_blocks,
        inode_table_blocks,
        first_data_zone,
        max_size,
    })
}

/// Set one bit in one bitmap block of the image (`insert_bit`: Minix bitmaps are low-bit-first).
fn set_bit(image: &mut [u8], block_size: u64, block: u64, bit: u64) {
    let base = (block * block_size) as usize;
    let index = (bit / 8) as usize;
    let offset = (bit % 8) as u8;
    image[base + index] |= 1 << offset;
}

/// One directory entry: a 4-byte inode number plus a 60-byte name buffer (NUL padded).
fn dir_entry(name: &[u8], inode: u64) -> [u8; DIRECTORY_ENTRY_SIZE] {
    let mut out = [0u8; DIRECTORY_ENTRY_SIZE];
    out[0..4].copy_from_slice(&(inode as u32).to_le_bytes());
    let n = name.len().min(NAME_CAPACITY);
    out[4..4 + n].copy_from_slice(&name[..n]);
    out
}

/// Build the whole image (C `main`'s write order: zero boot block, superblock, cleared
/// map and inode areas, bit 0 of both bitmaps, root inode, `.` and `..` in the root data zone).
///
/// `now` is the current time stamped into inodes (C's `current_time`; host tests
/// fix it so assertions stay reproducible).
pub fn build_image(plan: &MkfsPlan, now: i32) -> Result<Vec<u8>, MkfsError> {
    build_image_rooted(plan, now, (TYPE_DIRECTORY | 0o777) as u16, 0, 0)
}

/// Like [`build_image`], but the root inode's mode and owner come from the caller —
/// the third line of a prototype file is the root directory's mode line (C `main` mkfs.c:313-318).
fn build_image_rooted(
    plan: &MkfsPlan,
    now: i32,
    root_mode: u16,
    root_uid: u16,
    root_gid: u16,
) -> Result<Vec<u8>, MkfsError> {
    let total = plan
        .blocks
        .checked_mul(plan.block_size)
        .ok_or(MkfsError::BufferTooSmall)?;
    let total = total as usize;
    let mut image = vec![0u8; total];
    let bs = plan.block_size;

    // Superblock: block 1 (byte offset 1024), 31-byte payload.
    let small_first = if plan.first_data_zone <= u16::MAX as u64 {
        plan.first_data_zone as u16
    } else {
        // Write zero when it does not fit (C mkfs.c:632-640: recomputable from other fields).
        0
    };
    let sup = DiskSuperblock {
        inode_count: plan.inodes as u32,
        zone_total_small: 0,
        inode_map_blocks: plan.inode_map_blocks as i16,
        zone_map_blocks: plan.zone_map_blocks as i16,
        first_data_zone_small: small_first,
        log_zone_size: 0,
        flags: FLAG_CLEAN,
        max_size: plan.max_size as i32,
        zones: plan.zones as u32,
        magic: MAGIC_V3,
        pad: 0,
        block_size: plan.block_size as u16,
        disk_version: 0,
    };
    // Superblock: byte offset 1024 (C `SUPER_BLOCK_BYTES`, const.h:50; the same
    // authority as this server's `parse_superblock`) — with 4096-byte blocks it lies inside "block 0".
    let at = crate::superblock::SUPER_BLOCK_OFFSET;
    image[at..at + DiskSuperblock::STORED_BYTES].copy_from_slice(&sup.to_bytes());

    // Bit 0 of both bitmaps is always set (C mkfs.c:705-708).
    set_bit(&mut image, bs, START_BLOCK, 0);
    set_bit(&mut image, bs, START_BLOCK + plan.inode_map_blocks, 0);

    // Root inode: number 1, link count two, size two entries, direct zone pointing at
    // the first data zone (C `alloc_inode` plus `rootdir`'s `add_zone`/`incr_link`×2).
    let inode_table_block = START_BLOCK + plan.inode_map_blocks + plan.zone_map_blocks;
    let root = DiskInode {
        mode: root_mode,
        nlinks: 2,
        owner: root_uid,
        group: root_gid,
        size: 2 * DIRECTORY_ENTRY_SIZE as i32,
        accessed: now,
        modified: now,
        changed: now,
        zones: {
            let mut z = [0u32; TOTAL_ZONES];
            z[0] = plan.first_data_zone as u32;
            z
        },
    };
    let root_at = (inode_table_block * bs) as usize;
    image[root_at..root_at + crate::superblock::INODE_DISK_SIZE]
        .copy_from_slice(&root.to_bytes());

    // Root data zone: `.` and `..` both point at number 1 (C `enter_dir`×2).
    let zone_at = (plan.first_data_zone * bs) as usize;
    image[zone_at..zone_at + DIRECTORY_ENTRY_SIZE].copy_from_slice(&dir_entry(b".", 1));
    image[zone_at + DIRECTORY_ENTRY_SIZE..zone_at + 2 * DIRECTORY_ENTRY_SIZE]
        .copy_from_slice(&dir_entry(b"..", 1));

    // Bitmap accounting: the root inode (bit 1) and the root data zone (bit 1).
    set_bit(&mut image, bs, START_BLOCK, 1);
    set_bit(&mut image, bs, START_BLOCK + plan.inode_map_blocks, 1);

    Ok(image)
}

// ======================================================================
// Prototype seeding (C's proto-file processing assist group + eat_dir/eat_file)
// ======================================================================

/// Host file reading seam (the equivalent of `open(token[4])` in C `eat_dir`):
/// the decisive half never touches the file system; the thin shell injects regular
/// file content. `HostFile.mtime` lands in the inode's modification time (C
/// `file_time`, mkfs.c:860 — the `-d` branch that forces current_time is handled by
/// the thin shell).
pub trait ProtoHost {
    /// Read a host file's entire content into `out`, returning the byte count and
    /// modification time. Content larger than the buffer is rejected with
    /// [`MkfsError::BufferTooSmall`] (the file does not fit the target file system;
    /// failing early beats writing half). The implementation records failure paths
    /// itself — [`MkfsError::HostFile`] carries no data.
    fn read_file(&mut self, path: &str, out: &mut [u8]) -> Result<HostFile, MkfsError>;
}

/// Result of one host read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostFile {
    /// Content byte count.
    pub len: usize,
    /// Modification time (source of the inode's `mtime`, C `file_time`).
    pub mtime: i32,
}

/// Prototype file header (what the first two content lines yield): the boot block
/// line is read and discarded, the second line gives block and inode counts (C `main`
/// mkfs.c:294-310).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtoHeader {
    /// Block count declared by the prototype (first word of the second line; 0 means the caller decides).
    pub blocks: u64,
    /// Inode count declared by the prototype (0 = use the default ladder).
    pub inodes: u64,
}

/// Parse the header from prototype text (the first two lines, C `main` mkfs.c:294-310).
/// The caller uses the header's inode count (zero means the default ladder) to compute
/// [`plan_layout`], then hands the same text to [`build_image_seeded`].
pub fn proto_header(proto: &str) -> Result<ProtoHeader, MkfsError> {
    let mut lines = proto.lines();
    // The first line is the boot block loader name, read and discarded (C "skip boot block info").
    lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
    let counts = lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
    let tokens = tokenize(counts)?;
    if tokens.len() < 2 {
        return Err(MkfsError::ProtoBadLine);
    }
    Ok(ProtoHeader {
        blocks: parse_number(tokens[0])?,
        inodes: parse_number(tokens[1])?,
    })
}

/// Split a line into words on spaces and tabs (the tokenize half of C `get_line`,
/// mkfs.c:1244 ff.: whitespace becomes separators, at most ten words). An empty line
/// has no words; callers dispatch on the empty table. More than ten words is
/// rejected — C writes `parse[10]` out of bounds there, a latent memory fault; the
/// Rust side rejects it as a grammar error.
fn tokenize(line: &str) -> Result<Vec<&str>, MkfsError> {
    if line.len() > 300 {
        return Err(MkfsError::ProtoBadLine);
    }
    let mut tokens: Vec<&str> = Vec::new();
    for word in line.split([' ', '\t']) {
        if word.is_empty() {
            continue;
        }
        if tokens.len() == 10 {
            return Err(MkfsError::ProtoBadLine);
        }
        tokens.push(word);
    }
    Ok(tokens)
}

/// Decimal parsing (the strict face of C `atoi`/`strtol`: C silently returns zero on
/// garbage; the prototype is construction input, so rejecting bad numbers outright is
/// more honest than reproducing the garbage).
fn parse_number(token: &str) -> Result<u64, MkfsError> {
    token.parse::<u64>().map_err(|_| MkfsError::ProtoBadLine)
}

/// Convert a six-character mode string to permission bits (C `mode_con`,
/// mkfs.c:1195-1215; C `mkproto` mkproto.c:303-315 generates it with `%c%c%c%03o`).
/// The first character fixes the file type (`l` and `s` both map to `S_IFLNK`, the
/// ls-compatibility noted in C), the second and third are the set-uid/set-gid
/// markers, and the last three are octal permissions.
pub fn mode_con(text: &str) -> Result<u16, MkfsError> {
    let bytes = text.as_bytes();
    if bytes.len() < 6 {
        return Err(MkfsError::ProtoBadMode);
    }
    let mut mode: u16 = match bytes[0] {
        b'd' => 0o040000,
        b'b' => 0o060000,
        b'c' => 0o020000,
        b's' | b'l' => 0o120000,
        b'-' => 0o100000,
        _ => return Err(MkfsError::ProtoBadMode),
    };
    if bytes[1] == b'u' {
        mode |= 0o4000;
    }
    if bytes[2] == b'g' {
        mode |= 0o2000;
    }
    let mut perm = 0u16;
    for &digit in &bytes[3..6] {
        if !digit.is_ascii_digit() || digit > b'7' {
            return Err(MkfsError::ProtoBadMode);
        }
        perm = perm * 8 + (digit - b'0') as u16;
    }
    Ok(mode | perm)
}

/// Prototype seeder: the image plus two allocation cursors (C's `next_inode`/
/// `next_zone` statics); its methods map one-to-one onto C's helper function groups.
struct Seeder<'a> {
    image: &'a mut Vec<u8>,
    plan: MkfsPlan,
    /// First inode table block (C's `inode_offset` = `BLK_ILIST`).
    table_block: u64,
    /// First zone bitmap block.
    zmap_block: u64,
    next_inode: u64,
    next_zone: u64,
    now: i32,
}

impl<'a> Seeder<'a> {
    fn new(image: &'a mut Vec<u8>, plan: &MkfsPlan, now: i32) -> Self {
        let table_block = START_BLOCK + plan.inode_map_blocks + plan.zone_map_blocks;
        let zmap_block = START_BLOCK + plan.inode_map_blocks;
        Seeder {
            image,
            plan: *plan,
            table_block,
            zmap_block,
            next_inode: 2, // root inode already present in the base image
            next_zone: plan.first_data_zone + 1, // root data zone already taken
            now,
        }
    }

    fn bs(&self) -> u64 {
        self.plan.block_size
    }

    /// Position of inode `n`'s 64-byte record in the image.
    fn inode_at(&self, n: u64) -> usize {
        let per_block = self.bs() / crate::superblock::INODE_DISK_SIZE as u64;
        let block = self.table_block + (n - 1) / per_block;
        ((block * self.bs()) + ((n - 1) % per_block) * crate::superblock::INODE_DISK_SIZE as u64)
            as usize
    }

    fn read_inode(&self, n: u64) -> Result<DiskInode, MkfsError> {
        let at = self.inode_at(n);
        DiskInode::from_bytes(&self.image[at..at + crate::superblock::INODE_DISK_SIZE])
            .map_err(|_| MkfsError::BufferTooSmall)
    }

    fn write_inode(&mut self, inode: &DiskInode, n: u64) {
        let at = self.inode_at(n);
        let bytes = inode.to_bytes();
        self.image[at..at + crate::superblock::INODE_DISK_SIZE].copy_from_slice(&bytes);
    }

    fn set_imap_bit(&mut self, bit: u64) {
        let bs = self.bs();
        set_bit(self.image, bs, START_BLOCK, bit);
    }

    /// Zone number to bitmap bit (C's `zoff = s_firstdatazone - 1` accounting:
    /// the first data zone occupies bit 1).
    fn set_zmap_bit(&mut self, zone: u64) {
        let bs = self.bs();
        set_bit(self.image, bs, self.zmap_block, zone + 1 - self.plan.first_data_zone);
    }

    /// `alloc_inode` (C mkfs.c:1112-1145): take a number, write mode and owner, set the bitmap bit.
    fn alloc_inode(&mut self, mode: u16, uid: u16, gid: u16) -> Result<u64, MkfsError> {
        let num = self.next_inode;
        if num > self.plan.inodes {
            return Err(MkfsError::NotEnoughInodes { have: self.plan.inodes });
        }
        self.next_inode += 1;
        let inode = DiskInode {
            mode,
            nlinks: 0,
            owner: uid,
            group: gid,
            size: 0,
            accessed: 0,
            modified: 0,
            changed: 0,
            zones: [0; TOTAL_ZONES],
        };
        self.write_inode(&inode, num);
        self.set_imap_bit(num);
        Ok(num)
    }

    /// `alloc_zone` (C mkfs.c:1147-1165): take a number, zero the whole zone, set the bitmap bit.
    fn alloc_zone(&mut self) -> Result<u64, MkfsError> {
        let zone = self.next_zone;
        if zone >= self.plan.blocks {
            return Err(MkfsError::NotEnoughZones);
        }
        self.next_zone += 1;
        let bs = self.bs() as usize;
        let at = (zone * self.bs()) as usize;
        self.image[at..at + bs].fill(0);
        self.set_zmap_bit(zone);
        Ok(zone)
    }

    /// `incr_link` (C mkfs.c:1057-1080): increment the link count, error on overflow.
    fn incr_link(&mut self, n: u64) -> Result<(), MkfsError> {
        let mut inode = self.read_inode(n)?;
        inode.nlinks = inode.nlinks.checked_add(1).ok_or(MkfsError::TooManyLinks)?;
        self.write_inode(&inode, n);
        Ok(())
    }

    /// `incr_size` (C mkfs.c:1082-1110): each directory entry accepted grows the size by 64 bytes.
    fn incr_size(&mut self, n: u64, count: i32) -> Result<(), MkfsError> {
        let mut inode = self.read_inode(n)?;
        inode.size = inode
            .size
            .checked_add(count)
            .ok_or(MkfsError::SizeTooLarge)?;
        self.write_inode(&inode, n);
        Ok(())
    }

    /// `add_zone` (C mkfs.c:957-1055): hang a zone in the inode's first free slot;
    /// when the direct zones are full use single indirect, then double indirect; size
    /// and times are recorded as zones are attached.
    fn add_zone(&mut self, n: u64, zone: u64, bytes: i32, mtime: i32) -> Result<(), MkfsError> {
        let mut inode = self.read_inode(n)?;
        inode.size = inode
            .size
            .checked_add(bytes)
            .ok_or(MkfsError::SizeTooLarge)?;
        inode.modified = mtime;
        inode.accessed = self.now;
        inode.changed = self.now;
        for slot in 0..DIRECT_ZONE_COUNT {
            if inode.zones[slot] == 0 {
                inode.zones[slot] = zone as u32;
                self.write_inode(&inode, n);
                return Ok(());
            }
        }
        // Single indirect: allocate the indirect zone when the slot is empty (alloc_zone already zeroes and marks it).
        if inode.zones[DIRECT_ZONE_COUNT] == 0 {
            inode.zones[DIRECT_ZONE_COUNT] = self.alloc_zone()? as u32;
        }
        let indirect = inode.zones[DIRECT_ZONE_COUNT] as u64;
        self.write_inode(&inode, n);
        if self.hang_zone(indirect, zone)? {
            return Ok(());
        }
        // Double indirect: allocate the double zone when the slot is empty, then hang in its single-indirect layer.
        if inode.zones[DIRECT_ZONE_COUNT + 1] == 0 {
            let mut inode = self.read_inode(n)?;
            inode.zones[DIRECT_ZONE_COUNT + 1] = self.alloc_zone()? as u32;
            self.write_inode(&inode, n);
        }
        let double = self.read_inode(n)?.zones[DIRECT_ZONE_COUNT + 1] as u64;
        let entries = (self.bs() / 4) as usize;
        for i in 0..entries {
            let mid = self.indirect_entry(double, i)?;
            if mid == 0 {
                let fresh = self.alloc_zone()? as u32;
                self.set_indirect_entry(double, i, fresh)?;
            }
            let mid = self.indirect_entry(double, i)?;
            if self.hang_zone(mid, zone)? {
                return Ok(());
            }
        }
        Err(MkfsError::FileTooLarge)
    }

    /// Find a free slot in a single-indirect zone for a data zone; returns true when hung.
    fn hang_zone(&mut self, indirect: u64, zone: u64) -> Result<bool, MkfsError> {
        let entries = (self.bs() / 4) as usize;
        for i in 0..entries {
            if self.indirect_entry(indirect, i)? == 0 {
                self.set_indirect_entry(indirect, i, zone as u32)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn indirect_entry(&self, indirect: u64, index: usize) -> Result<u64, MkfsError> {
        let at = (indirect * self.bs()) as usize + index * 4;
        let bytes: [u8; 4] = self
            .image
            .get(at..at + 4)
            .and_then(|s| s.try_into().ok())
            .ok_or(MkfsError::BufferTooSmall)?;
        Ok(u32::from_le_bytes(bytes) as u64)
    }

    fn set_indirect_entry(&mut self, indirect: u64, index: usize, zone: u32) -> Result<(), MkfsError> {
        let at = (indirect * self.bs()) as usize + index * 4;
        let end = at + 4;
        let slice = self.image.get_mut(at..end).ok_or(MkfsError::BufferTooSmall)?;
        slice.copy_from_slice(&zone.to_le_bytes());
        Ok(())
    }

    /// `dir_try_enter` (C mkfs.c:873-903): find the first free slot (`d_inum == 0`)
    /// in a directory zone and write the entry there. Names longer than 60 bytes are
    /// truncated (same shape as C's strncpy; the (verbose) warning does not enter the
    /// decisive half).
    fn dir_try_enter(&mut self, zone: u64, child: u64, name: &[u8]) -> Result<bool, MkfsError> {
        let bs = self.bs() as usize;
        let base = (zone * self.bs()) as usize;
        let entries = bs / DIRECTORY_ENTRY_SIZE;
        for i in 0..entries {
            let at = base + i * DIRECTORY_ENTRY_SIZE;
            let busy = u32::from_le_bytes(
                self.image[at..at + 4].try_into().map_err(|_| MkfsError::BufferTooSmall)?,
            ) != 0;
            if busy {
                continue;
            }
            let entry = dir_entry(&name[..name.len().min(NAME_CAPACITY)], child);
            self.image[at..at + DIRECTORY_ENTRY_SIZE].copy_from_slice(&entry);
            return Ok(true);
        }
        Ok(false)
    }

    /// `enter_dir` (C mkfs.c:906-949): scan direct zones first (an empty one is used
    /// immediately), fall through to single indirect when full; if it still does not
    /// fit, reject as C's pexit does.
    fn enter_dir(&mut self, parent: u64, name: &[u8], child: u64) -> Result<(), MkfsError> {
        let mut inode = self.read_inode(parent)?;
        for slot in 0..DIRECT_ZONE_COUNT {
            if inode.zones[slot] == 0 {
                inode.zones[slot] = self.alloc_zone()? as u32;
                self.write_inode(&inode, parent);
            }
            let zone = self.read_inode(parent)?.zones[slot] as u64;
            if self.dir_try_enter(zone, child, name)? {
                return Ok(());
            }
        }
        if inode.zones[DIRECT_ZONE_COUNT] == 0 {
            inode.zones[DIRECT_ZONE_COUNT] = self.alloc_zone()? as u32;
            self.write_inode(&inode, parent);
        }
        let indirect = self.read_inode(parent)?.zones[DIRECT_ZONE_COUNT] as u64;
        let entries = (self.bs() / 4) as usize;
        for i in 0..entries {
            let zone = self.indirect_entry(indirect, i)?;
            let zone = if zone == 0 {
                let fresh = self.alloc_zone()?;
                self.set_indirect_entry(indirect, i, fresh as u32)?;
                fresh
            } else {
                zone
            };
            if self.dir_try_enter(zone, child, name)? {
                return Ok(());
            }
        }
        Err(MkfsError::DirectoryTooLarge)
    }

    /// The recursive half of `rootdir` plus `eat_dir` (C mkfs.c:765-830): read the
    /// prototype line by line, `$` closes the current directory; directories recurse,
    /// devices hang their device number, symlinks hang their target, and regular files
    /// are read through the [`ProtoHost`] seam.
    fn eat_dir<'p, H: ProtoHost>(
        &mut self,
        parent: u64,
        lines: &mut core::str::Lines<'p>,
        host: &mut H,
        scratch: &mut [u8],
    ) -> Result<(), MkfsError> {
        loop {
            let line = lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
            let tokens = tokenize(line)?;
            let Some(name) = tokens.first() else {
                continue; // empty line has no words — C dereferences NULL here; treat as skip
            };
            if *name == "$" {
                return Ok(());
            }
            let mode_text = tokens.get(1).ok_or(MkfsError::ProtoBadLine)?;
            let uid = parse_number(tokens.get(2).ok_or(MkfsError::ProtoBadLine)?)?;
            let gid = parse_number(tokens.get(3).ok_or(MkfsError::ProtoBadLine)?)?;
            let mode = mode_con(mode_text)?;
            let n = self.alloc_inode(mode, uid as u16, gid as u16)?;
            self.enter_dir(parent, name.as_bytes(), n)?;
            self.incr_size(parent, DIRECTORY_ENTRY_SIZE as i32)?;
            self.incr_link(n)?;
            match mode_text.as_bytes()[0] {
                b'd' => {
                    let zone = self.alloc_zone()?;
                    self.add_zone(n, zone, 2 * DIRECTORY_ENTRY_SIZE as i32, self.now)?;
                    self.enter_dir(n, b".", n)?;
                    self.enter_dir(n, b"..", parent)?;
                    self.incr_link(parent)?;
                    self.incr_link(n)?;
                    self.eat_dir(n, lines, host, scratch)?;
                }
                b'b' | b'c' => {
                    let major = parse_number(tokens.get(4).ok_or(MkfsError::ProtoBadLine)?)?;
                    let minor = parse_number(tokens.get(5).ok_or(MkfsError::ProtoBadLine)?)?;
                    let blocks = match tokens.get(6) {
                        Some(text) => parse_number(text)?,
                        None => 0,
                    };
                    // C `add_zone(n, makedev(maj, min), …)`: the zone-number slot
                    // holds the device number (major<<8|minor) and does not use the bitmap.
                    let dev = (major << 8) | (minor & 0xff);
                    self.add_zone(n, dev, (blocks as u32)
                        .checked_mul(self.bs() as u32)
                        .ok_or(MkfsError::SizeTooLarge)? as i32, self.now)?;
                }
                b's' => {
                    let target = tokens.get(4).ok_or(MkfsError::ProtoBadLine)?;
                    self.enter_symlink(n, target.as_bytes())?;
                }
                _ => {
                    let path = tokens.get(4).ok_or(MkfsError::ProtoBadLine)?;
                    let file = host.read_file(path, scratch)?;
                    self.eat_file(n, &scratch[..file.len], file.mtime)?;
                }
            }
        }
    }

    /// `enter_symlink` (C mkfs.c:734-749): write the target into a new zone; the size
    /// is the target length (without NUL).
    fn enter_symlink(&mut self, n: u64, target: &[u8]) -> Result<(), MkfsError> {
        if target.len() >= self.bs() as usize {
            return Err(MkfsError::SymlinkTooLong);
        }
        let zone = self.alloc_zone()?;
        let at = (zone * self.bs()) as usize;
        self.image[at..at + target.len()].copy_from_slice(target);
        self.add_zone(n, zone, target.len() as i32, self.now)
    }

    /// `eat_file` (C mkfs.c:851-870): write content zone by zone in block-sized
    /// chunks, chaining each zone; zero-length files occupy no zone. The times come
    /// from the host file's modification time.
    fn eat_file(&mut self, n: u64, data: &[u8], mtime: i32) -> Result<(), MkfsError> {
        let bs = self.bs() as usize;
        if data.is_empty() {
            return Ok(());
        }
        for chunk in data.chunks(bs) {
            let zone = self.alloc_zone()?;
            let at = (zone * self.bs()) as usize;
            self.image[at..at + chunk.len()].copy_from_slice(chunk);
            self.add_zone(n, zone, chunk.len() as i32, mtime)?;
        }
        Ok(())
    }
}

/// Build an image with prototype seeding (the `simple == 0` branch of C `main`,
/// mkfs.c:294-424): the first two lines fix the size and root mode, remaining lines
/// seed directory by directory. `plan` is computed by the caller from the
/// [`proto_header`] header — a declared block count larger than `plan.blocks` is
/// rejected (the errx at C mkfs.c:304-308). Returns the image.
pub fn build_image_seeded<H: ProtoHost>(
    plan: &MkfsPlan,
    proto: &str,
    host: &mut H,
    now: i32,
) -> Result<Vec<u8>, MkfsError> {
    let mut lines = proto.lines();
    lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?; // boot block line
    let counts = lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
    let tokens = tokenize(counts)?;
    if tokens.len() < 2 {
        return Err(MkfsError::ProtoBadLine);
    }
    let header_blocks = parse_number(tokens[0])?;
    if header_blocks > plan.blocks {
        return Err(MkfsError::ProtoTooLarge { given: plan.blocks, needed: header_blocks });
    }
    let root_line = lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
    let root_tokens = tokenize(root_line)?;
    if root_tokens.len() < 3 {
        return Err(MkfsError::ProtoBadLine);
    }
    let root_mode = mode_con(root_tokens[0])?;
    let root_uid = parse_number(root_tokens[1])?;
    let root_gid = parse_number(root_tokens[2])?;

    let mut image = build_image_rooted(plan, now, root_mode, root_uid as u16, root_gid as u16)?;
    let mut scratch = vec![0u8; image.len()];
    let mut seeder = Seeder::new(&mut image, plan, now);
    seeder.eat_dir(1, &mut lines, host, &mut scratch)?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::superblock::INODE_DISK_SIZE;
    use crate::superblock::ROOT_INODE_NUMBER;

    /// Layout arithmetic checked item by item against C `super` (mkfs.c:602-655):
    /// 64 blocks, 64 inodes, 4096-byte blocks → one block per bitmap, one inode table
    /// block, data area starting at 5.
    #[test]
    fn test_plan_layout_matches_c_arithmetic() {
        let plan = plan_layout(64, Some(64), 4096).unwrap();
        assert_eq!(plan.zones, 64);
        assert_eq!(plan.inodes, 64);
        assert_eq!(plan.inode_map_blocks, 1, "imap = ceil(65/32768)");
        assert_eq!(plan.zone_map_blocks, 1, "zmap = ceil(64/32768)");
        assert_eq!(plan.inode_table_blocks, 1, "64 inode / 每块 64 个");
        assert_eq!(plan.first_data_zone, 5);
        // Maximum file size: 7 + 1024 + 1024² zones × 4096, capped at INT32_MAX.
        assert_eq!(plan.max_size, i32::MAX as u64);
        // Trailing offset holds: the `1,2+3` formula is also 5 in the 8-byte field.
        assert_eq!(crate::superblock::START_BLOCK, 2);
    }

    /// Default inode count ladder and rounding (C mkfs.c:344-357).
    #[test]
    fn test_default_inode_count_ladder() {
        // 64 blocks × 4096 = 256 KB → one per 2 KB = 128.
        assert_eq!(default_inode_count(64, 4096), 128);
        // Ladder switch point: from 100_000 KB, one per 4 KB.
        let blocks_100m = 100_000 * 1024 / 4096;
        // 25_000 rounded up to a multiple of 64 (the same rounding as C mkfs.c:355-357).
        assert_eq!(default_inode_count(blocks_100m, 4096), 25_024);
        // Too small: rejected outright.
        assert_eq!(plan_layout(4, None, 4096), Err(MkfsError::TooSmall));
        // Bitmaps squeeze the data area: 5 blocks cannot hold 2 bitmaps plus the inode table plus data.
        assert_eq!(
            plan_layout(5, Some(64), 4096),
            Err(MkfsError::MapsTooLarge)
        );
    }

    /// Bit-level image check: the superblock parses and fields read back identically,
    /// bits 0/1 of both bitmaps are set, root inode and two directory entries are in
    /// place (C `main` write order).
    #[test]
    fn test_image_round_trips_through_our_own_parser() {
        let plan = plan_layout(64, Some(64), 4096).unwrap();
        let image = build_image(&plan, 1_000).unwrap();
        assert_eq!(image.len() as u64, 64 * 4096);
        // The part of the boot block before the superblock stays zero (C's put_block(zero)).
        assert!(image[..1024].iter().all(|b| *b == 0));
        // Superblock: first through the server's mount-entry parser (which pins the
        // 1024 offset, taking the mounter's runtime facts as arguments) to confirm the
        // image can be mounted by this server; then read back field by field.
        crate::superblock::parse_superblock(&image, 0, false).unwrap();
        let at = crate::superblock::SUPER_BLOCK_OFFSET;
        let sup = DiskSuperblock::from_bytes(&image[at..at + DiskSuperblock::STORED_BYTES])
            .unwrap();
        assert_eq!(sup.magic, MAGIC_V3);
        assert_eq!(sup.inode_count, 64);
        assert_eq!(sup.zones, 64);
        assert_eq!(sup.inode_map_blocks, 1);
        assert_eq!(sup.zone_map_blocks, 1);
        assert_eq!(sup.first_data_zone_small, 5);
        assert_eq!(sup.block_size, 4096);
        assert_eq!(sup.flags, FLAG_CLEAN);
        assert_eq!(sup.log_zone_size, 0);
        assert_eq!(sup.disk_version, 0);
        // Bitmaps: bit 0 of imap/zmap and bit 1 for the root inode/root data zone.
        let imap_at = (crate::superblock::START_BLOCK * 4096) as usize;
        assert_eq!(image[imap_at], 0b11, "imap 位 0 与 1（根 inode）");
        let zmap_at = imap_at + 4096;
        assert_eq!(image[zmap_at], 0b11, "zmap 位 0 与 1（根数据区）");
        // Root inode: directory, link count two, size two entries, direct zone pointing at the data zone.
        let table_at = (crate::superblock::START_BLOCK + 2) * 4096;
        let root = DiskInode::from_bytes(
            &image[table_at as usize..table_at as usize + INODE_DISK_SIZE],
        )
        .unwrap();
        assert_eq!(root.mode, (TYPE_DIRECTORY | 0o777) as u16);
        assert_eq!(root.nlinks, 2);
        assert_eq!(root.size, 2 * DIRECTORY_ENTRY_SIZE as i32);
        assert_eq!(root.zones[0], 5);
        assert_eq!(root.accessed, 1_000);
        // Directory entries: `.` and `..` both point at the root.
        let zone_at = (plan.first_data_zone * 4096) as usize;
        assert_eq!(u32::from_le_bytes(image[zone_at..zone_at + 4].try_into().unwrap()), 1);
        assert_eq!(&image[zone_at + 4..zone_at + 5], b".");
        let second = zone_at + DIRECTORY_ENTRY_SIZE;
        assert_eq!(
            u32::from_le_bytes(image[second..second + 4].try_into().unwrap()),
            ROOT_INODE_NUMBER as u32
        );
        assert_eq!(&image[second + 4..second + 6], b"..");
        assert!(image[second + 6..second + DIRECTORY_ENTRY_SIZE]
            .iter()
            .all(|b| *b == 0));
    }

    // ---------------- Prototype seeding ----------------

    /// In-memory host: a stand-in mapping paths to (content, modification time) —
    /// the test half of the `ProtoHost` seam, corresponding to the thin shell's std
    /// file reader.
    struct MemHost {
        files: Vec<(&'static str, &'static [u8], i32)>,
    }

    impl ProtoHost for MemHost {
        fn read_file(&mut self, path: &str, out: &mut [u8]) -> Result<HostFile, MkfsError> {
            let Some((_, bytes, mtime)) = self.files.iter().find(|(p, _, _)| *p == path) else {
                return Err(MkfsError::HostFile);
            };
            if bytes.len() > out.len() {
                return Err(MkfsError::BufferTooSmall);
            }
            out[..bytes.len()].copy_from_slice(bytes);
            Ok(HostFile { len: bytes.len(), mtime: *mtime })
        }
    }

    /// Parse directory entry `i` from the direct zones of inode `n` in the image
    /// (test walkthrough helper).
    fn dir_slot(image: &[u8], plan: &MkfsPlan, inode_number: u64, slot: usize) -> (u32, Vec<u8>) {
        let bs = plan.block_size as usize;
        let per_block = bs / INODE_DISK_SIZE;
        let table = (START_BLOCK + plan.inode_map_blocks + plan.zone_map_blocks) as usize;
        let at = table * bs
            + ((inode_number - 1) as usize / per_block) * INODE_DISK_SIZE
            + ((inode_number - 1) as usize % per_block) * INODE_DISK_SIZE;
        let inode = DiskInode::from_bytes(&image[at..at + INODE_DISK_SIZE]).unwrap();
        let zone = inode.zones[slot / (bs / DIRECTORY_ENTRY_SIZE)] as usize;
        let base = zone * bs + (slot % (bs / DIRECTORY_ENTRY_SIZE)) * DIRECTORY_ENTRY_SIZE;
        let number =
            u32::from_le_bytes(image[base..base + 4].try_into().unwrap());
        let mut name = Vec::new();
        for &b in &image[base + 4..base + DIRECTORY_ENTRY_SIZE] {
            if b == 0 {
                break;
            }
            name.push(b);
        }
        (number, name)
    }

    /// Conversion face of six-character mode strings (C `mode_con` plus mkproto's generated form).
    #[test]
    fn test_mode_con_six_character_forms() {
        assert_eq!(mode_con("d--755").unwrap(), 0o040755);
        assert_eq!(mode_con("-u-644").unwrap(), 0o100644 | 0o4000);
        assert_eq!(mode_con("--g664").unwrap(), 0o100664 | 0o2000);
        assert_eq!(mode_con("s--777").unwrap(), 0o120777);
        assert_eq!(mode_con("c--660").unwrap(), 0o020660);
        // Short strings read past the separator in C (latent out-of-bounds read) — rejected as a grammar error.
        assert_eq!(mode_con("d755"), Err(MkfsError::ProtoBadMode));
        assert_eq!(mode_con("d--888"), Err(MkfsError::ProtoBadMode));
        assert_eq!(mode_con("q--755"), Err(MkfsError::ProtoBadMode));
    }

    /// Seeding a whole tree from a prototype: directory recursion, regular file
    /// content, symlinks, device files, link counts and bitmap accounting, all read
    /// back through the server's own parser (the semantic face of C
    /// `eat_dir`/`eat_file`).
    #[test]
    fn test_proto_seeds_tree_with_all_entry_types() {
        let proto = "boot\n\
                     64 16\n\
                     d--750 0 0\n\
                     etc d--755 2 2\n\
                     $\n\
                     motd ---644 3 3 /host/motd\n\
                     tty c--660 0 0 4 0\n\
                     fast s--777 4 4 /host/motd\n\
                     $\n";
        let mut host =
            MemHost { files: vec![("/host/motd", b"hello proto\n", 1_700)] };
        let header = proto_header(proto).unwrap();
        assert_eq!(header, ProtoHeader { blocks: 64, inodes: 16 });
        let plan = plan_layout(64, Some(16), 4096).unwrap();
        let image = build_image_seeded(&plan, proto, &mut host, 1_000).unwrap();

        // Root directory: mode line from the header's third line (d-u750); one subdirectory, link count three.
        let bs = 4096usize;
        let per_block = bs / INODE_DISK_SIZE;
        let table = (START_BLOCK + plan.inode_map_blocks + plan.zone_map_blocks) as usize;
        let inode_at = |n: u64| {
            let per = per_block as u64;
            table * bs
                + (((n - 1) / per) * INODE_DISK_SIZE as u64) as usize
                + (((n - 1) % per) * INODE_DISK_SIZE as u64) as usize
        };
        let root = DiskInode::from_bytes(&image[inode_at(1)..inode_at(1) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(root.mode, 0o040750);
        assert_eq!(root.nlinks, 3, "根目录含子目录 etc：`.`、`..` 加 etc 的 `..`");
        assert_eq!(root.size as usize, 6 * DIRECTORY_ENTRY_SIZE);
        // The root's first two entries are still `.`/`..`, then four entries (order follows the prototype lines).
        assert_eq!(dir_slot(&image, &plan, 1, 0), (1, b".".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 1), (1, b"..".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 2), (2, b"etc".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 3), (3, b"motd".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 4), (4, b"tty".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 5), (5, b"fast".to_vec()));

        // etc directory: empty directory (`$` follows immediately), `.`/`..` point correctly, link count two.
        assert_eq!(dir_slot(&image, &plan, 2, 0), (2, b".".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 2, 1), (1, b"..".to_vec()));
        let etc = DiskInode::from_bytes(&image[inode_at(2)..inode_at(2) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(etc.mode, 0o040755);
        assert_eq!(etc.owner, 2);
        assert_eq!(etc.group, 2);
        assert_eq!(etc.nlinks, 2);
        assert_eq!(etc.size as usize, 2 * DIRECTORY_ENTRY_SIZE);

        // Regular file: content read from the host, size 13, mtime from the host file.
        let motd = DiskInode::from_bytes(&image[inode_at(3)..inode_at(3) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(motd.mode, 0o100644);
        assert_eq!(motd.size as usize, b"hello proto\n".len());
        assert_eq!(motd.modified, 1_700);
        let data_at = motd.zones[0] as usize * bs;
        assert_eq!(&image[data_at..data_at + 12], b"hello proto\n");

        // Character device: `major<<8|minor` goes into the zone-number slot, size zero, no data zone used.
        let tty = DiskInode::from_bytes(&image[inode_at(4)..inode_at(4) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(tty.mode, 0o020660);
        // major=4, minor=0 encoded into the zone-number slot (`major<<8|minor`,
        // C `proto.c`'s dev_t packing); minor is 0, so no always-true `| 0` is written.
        assert_eq!(tty.zones[0], 4 << 8);
        assert_eq!(tty.size, 0);

        // Symlink: the target string goes into a data zone, size is the target length.
        let fast = DiskInode::from_bytes(&image[inode_at(5)..inode_at(5) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(fast.mode, 0o120777);
        assert_eq!(fast.size as usize, "/host/motd".len());
        let target_at = fast.zones[0] as usize * bs;
        assert_eq!(&image[target_at..target_at + 10], b"/host/motd");

        // Bitmap accounting: all five inodes are in imap; four data zones (root, etc,
        // motd, fast) are in zmap; tty has no data zone.
        let imap = &image[(START_BLOCK * 4096) as usize..];
        for ino in 1..=5u64 {
            assert_eq!(imap[(ino / 8) as usize] >> (ino % 8) & 1, 1, "imap 位 {ino}");
        }
        let zmap_at = (START_BLOCK * 4096) as usize + bs;
        let zmap = &image[zmap_at..];
        assert_eq!(zmap[0] >> 1 & 0b1111, 0b1111, "位 1..4：根、etc、motd、fast 四区");
    }

    /// Seeding error faces: block argument below the header, prototype ends early,
    /// missing host file, bad mode line, inodes exhausted.
    #[test]
    fn test_proto_errors_are_rejected() {
        let mut host = MemHost { files: vec![] };
        let plan = plan_layout(16, Some(8), 4096).unwrap();
        // Header declares 64 blocks, argument gives only 16 (the errx at C mkfs.c:305-308).
        let proto_big = "boot\n64 8\nd--755 0 0\n$\n";
        assert_eq!(
            build_image_seeded(&plan, proto_big, &mut host, 0),
            Err(MkfsError::ProtoTooLarge { given: 16, needed: 64 })
        );
        // Prototype ends before `$` (C get_line's Unexpected end-of-file).
        let plan64 = plan_layout(64, Some(16), 4096).unwrap();
        let proto_short = "boot\n64 8\nd--755 0 0\n";
        assert_eq!(
            build_image_seeded(&plan64, proto_short, &mut host, 0),
            Err(MkfsError::ProtoUnexpectedEnd)
        );
        // Missing host file (C "Can't open %s").
        let proto_missing = "boot\n64 8\nd--755 0 0\nmotd ---644 0 0 /nope\n$\n";
        assert_eq!(
            build_image_seeded(&plan64, proto_missing, &mut host, 0),
            Err(MkfsError::HostFile)
        );
        // Bad mode line (missing words).
        let proto_mode = "boot\n64 8\nd--755\n$\n";
        assert_eq!(
            build_image_seeded(&plan64, proto_mode, &mut host, 0),
            Err(MkfsError::ProtoBadLine)
        );
        // Inodes exhausted: with 4096-byte blocks the rounded inode count is 64,
        // which cannot hold the root plus 70 entries (C alloc_inode's not enough inodes).
        let mut many = alloc::string::String::from("boot\n64 16\nd--755 0 0\n");
        use core::fmt::Write as _;
        for k in 0..70 {
            writeln!(many, "f{k} ---644 0 0 /host/motd").unwrap();
        }
        many.push('$');
        // Empty-content files take no zone, so inodes run out first.
        let mut host2 = MemHost { files: vec![("/host/motd", b"", 0)] };
        let header = proto_header(&many).unwrap();
        let plan_tight = plan_layout(64, Some(header.inodes), 4096).unwrap();
        assert_eq!(plan_tight.inodes, 64, "16 向上取整到 inode 块倍数");
        assert_eq!(
            build_image_seeded(&plan_tight, &many, &mut host2, 0),
            Err(MkfsError::NotEnoughInodes { have: 64 })
        );
    }

    /// Zero-length file and bare root directory: zero length uses no zone (C
    /// eat_file's ct==0 branch), and the bare root has only `.`/`..`.
    #[test]
    fn test_proto_empty_file_and_bare_root() {
        let mut host = MemHost { files: vec![("/empty", b"", 5)] };
        let proto = "boot\n32 8\nd-u755 0 0\nempty --u644 0 0 /empty\n$\n";
        let plan = plan_layout(32, Some(8), 4096).unwrap();
        let image = build_image_seeded(&plan, proto, &mut host, 7).unwrap();
        let bs = plan.block_size as usize;
        let per_block = bs / INODE_DISK_SIZE;
        let table = (START_BLOCK + 2) as usize;
        let at = table * bs + (2 - 1) / per_block * INODE_DISK_SIZE + (2 - 1) % per_block * INODE_DISK_SIZE;
        let file = DiskInode::from_bytes(&image[at..at + INODE_DISK_SIZE]).unwrap();
        assert_eq!(file.size, 0);
        assert_eq!(file.zones[0], 0, "零长文件不占区");
        assert_eq!(file.modified, 0, "无区可挂，mtime 不落");
        // zmap has only the root data zone's data bit.
        let zmap_at = (START_BLOCK as usize * bs) + plan.inode_map_blocks as usize * bs;
        assert_eq!(image[zmap_at], 0b11, "位 0 恒置 + 位 1 = 根数据区");
    }
}
