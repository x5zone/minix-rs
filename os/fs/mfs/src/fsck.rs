//! The deciding half of `fsck.mfs`: read-only file-system consistency checking
//! and reporting.
//!
//! C counterpart: `minix3/minix/commands/fsck.mfs/fsck.c` (1675 lines). This
//! module only does **checking and reporting**: superblock validation
//! (`rw_super` fsck.c:561-599, `chksuper` fsck.c:602-655), the file-tree walk
//! (`chktree`/`descendtree`/`chkinode`/`chkmode`/`chkfile`/`chkzones`/
//! `markzone` and the directory-entry check family, fsck.c:1021-1501), bitmap
//! reconciliation (`chkmap`/`chkword` fsck.c:786-841 — the "expected map" built
//! by the walk compared bit by bit against the on-disk "actual map"), link-count
//! reconciliation (`getcount`/`chkcount`/`counterror` fsck.c:866-908), the
//! free-inode zero check (`chkilist` fsck.c:843-864) and the totals report
//! (`printtotal` fsck.c:1503-1531). The `-l` listing (`list` fsck.c:936-968) and
//! the `-i`/`-z` watch lists (`fillbitmap` fsck.c:739-757) are provided
//! alongside the walk. The repair face (`repair`/`automatic` interactive repair)
//! and parallel preen are **not in this module** — the `yes()` question and
//! `devwrite` do not appear at all, and keep their registration for a later
//! batch.
//!
//! # Check order (the read-only projection of C `chkdev` fsck.c:1538-1614)
//!
//! ```text
//! read superblock → basic sanity (magic/counts/geometry) → chksuper deep check
//! → chktree walk builds the expected maps and the link counts → chkmap(zone) → chkcount
//! → chkmap(inode) → chkilist → printtotal
//! ```
//!
//! # Stance and deviations (all annotated)
//!
//! - Only the third superblock version is accepted (the `SUPER_` version-3
//!   magic): the same stance as `parse_superblock` on the server side. C's
//!   support for the older version carries a host-compilation patch
//!   (`block_size = 8192`, fsck.c:579-581), and this repository neither produces
//!   nor consumes such images. The magic numbers themselves live in
//!   `crate::superblock`.
//! - C `rw_super`'s `s_firstdatazone <= 4` check (fsck.c:592) lands on an
//!   in-memory field — `read` fills only the 31 on-disk bytes, so that field
//!   reads stray bytes from the superblock block. This module applies the same
//!   test to the on-disk small field (`first_data_zone_small`), which is
//!   semantically "the first data zone must not fall inside the metadata area".
//! - The repair question (`yes`) is always "no" in read-only mode (fsck.c:263-266
//!   already behaves that way): no remove/repair/adjust branch appears, checks
//!   still run, reports still go out, and the disk image is untouched.
//! - The host image is resident in memory as a whole (the dual of
//!   `mkfs::build_image`): C streams the device through a block cache, while an
//!   image file has no random-read problem; the streaming seam (a trait
//!   abstraction) waits until the real-machine block-device face (E-FSBDEV)
//!   needs it.

use crate::inode::DiskInode;
use crate::superblock::{
    Bitmap, DiskSuperblock, FLAG_CLEAN, FLAG_MANDATORY_MASK, MAGIC_V1, MAGIC_V2, MAGIC_V3,
    START_BLOCK, SUPER_BLOCK_OFFSET,
};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Maximum file position (C `MAX_FILE_POS`, minix/const.h:124: `0x7FFFFFFF`).
const MAX_FILE_POS: i64 = 0x7FFF_FFFF;

/// Truncation threshold of the bitmap report (C `MAXPRINT`, fsck.c:67): after
/// every `MAXPRINT` differences it stops listing them one by one and reports only
/// the total.
pub const MAXPRINT: u64 = 80;

/// Usage-error exit code (C `FSCK_EXIT_USAGE`, sbin/fsck/exitvalues.h).
pub const EXIT_USAGE: i32 = 1;
/// Check-failed exit code (C `FSCK_EXIT_CHECK_FAILED`, same header; the exit of
/// `fatal()`).
pub const EXIT_CHECK_FAILED: i32 = 8;

/// Number of indirect entries read in one go (C `CINDIR`, fsck.c:68).
const CINDIR: usize = 128;
/// Number of zone levels: `NLEVEL = NR_ZONE_NUMS - NR_DZONE_NUM + 1 = 4`
/// (fsck.c:117; direct / single-indirect / double-indirect / the "further
/// indirect" slot at position 10 on disk).
const ZONE_LEVELS: usize = 4;
/// FIFO type bit (C `I_NAMED_PIPE`, minix/const.h:111).
const TYPE_NAMED_PIPE: u32 = 0o010000;
/// Upper bound on a symlink target's length (C `PATH_MAX`, the 1024 in
/// sys/syslimits.h:64; the self-consistency test of `chksymlinkzone`
/// fsck.c:1175).
const PATH_MAX_LIMIT: i32 = 1024;
/// Link-count ceiling (C `SHRT_MAX`; the same value as `LINK_CEILING` in
/// inode.rs).
const SHRT_MAX: u32 = crate::inode::LINK_CEILING;
/// The `.` presence flag of the directory walk (C `DOT`, fsck.c:141).
const DOT: u8 = 1;
/// The `..` presence flag of the directory walk (C `DOTDOT`, fsck.c:142).
const DOTDOT: u8 = 2;

/// Bits per block (`FS_BITS_PER_BLOCK(b) = b * 8`, converted through
/// `FS_BITMAP_CHUNKS` in const.h:44: b/4 32-bit words × 32 bits).
fn bits_per_block(block_size: u64) -> u64 {
    block_size * 8
}

/// Blocks occupied by a bitmap (C `bitmapsize`, libminlib/fslib.c: rounded up to
/// whole blocks). From the same formula as the `mkfs` name — the build half there,
/// the checking half here.
fn bitmap_size(nr_bits: u64, block_size: u64) -> u64 {
    let per = bits_per_block(block_size);
    let blocks = nr_bits / per;
    if blocks * per < nr_bits {
        blocks + 1
    } else {
        blocks
    }
}

/// Inodes per block (`INODES_PER_BLOCK(b) = b / 64`, const.h:39).
fn inodes_per_block(block_size: u64) -> u64 {
    block_size / crate::superblock::INODE_DISK_SIZE as u64
}

/// The file-system geometry established once validation passes — the common
/// input of the walk and the bitmap reconciliation.
///
/// The fields correspond to the on-disk face of C `struct super_block` plus
/// `chksuper`'s two recomputations (`s_firstdatazone`, the expected `max_size`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuperInfo {
    /// Block size in bytes (the version-3 value comes from the superblock field,
    /// fsck.c:584).
    pub block_size: u64,
    /// Total number of zones (`s_zones`).
    pub zones: u64,
    /// Number of inodes in use (`s_ninodes`).
    pub inodes: u64,
    /// Number of inode bitmap blocks (`s_imap_blocks`).
    pub inode_map_blocks: u64,
    /// Number of zone bitmap blocks (`s_zmap_blocks`).
    pub zone_map_blocks: u64,
    /// Number of inode table blocks (`N_ILIST`, the round-up formula at
    /// fsck.c:107).
    pub inode_table_blocks: u64,
    /// First data zone number: the value adopted after reconciling `chksuper`'s
    /// recomputation with the on-disk small value (fsck.c:629-641; when the disk
    /// says 0 the recomputed value is used, and a mismatch warns and follows the
    /// disk).
    pub first_data_zone: u64,
    /// Base-2 log of blocks per zone (`s_log_zone_size`).
    pub log_zone_size: u32,
    /// The validated maximum file size (the on-disk value, or C's capping formula
    /// at :642-646).
    pub max_size: i64,
    /// The clean flag (`MFSFLAG_CLEAN`): preen's skip criterion, carried here
    /// only.
    pub clean: bool,
}

impl SuperInfo {
    /// First block of the inode table (`BLK_ILIST = 2 + imap + zmap`,
    /// fsck.c:114).
    pub fn inode_table_block(&self) -> u64 {
        START_BLOCK + self.inode_map_blocks + self.zone_map_blocks
    }

    /// Bytes per zone (`ZONE_SIZE = block_size << log_zone_size`, the `ztob` face
    /// of fsck.c:116).
    pub fn zone_bytes(&self) -> u64 {
        self.block_size << self.log_zone_size
    }
}

/// A fatal error: the check cannot continue (C `fatal()`, fsck.c:241-246 — print
/// the message, the "fatal" trailer, and exit [`EXIT_CHECK_FAILED`]).
///
/// For multi-line messages the order is C's output order: the leading detail
/// lines first, closing with `fatal()`'s short message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fatal {
    /// The fatal reason (may contain newlines for leading detail lines).
    pub message: String,
}

/// The kinds of bitmap being reconciled (C `chkmap`'s `type` argument plus its
/// two call sites fsck.c:1586/:1588).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    /// Inode bitmap: block start [`START_BLOCK`], the reported number is the
    /// inode number.
    Inode,
    /// Zone bitmap: block start `2 + imap_blocks`, reported number = bit number +
    /// first data zone - 1 (the inverse of `markzone`'s bit numbering
    /// `zno - FIRST + 1`, fsck.c:1223).
    Zone,
}

impl MapKind {
    fn label(self) -> &'static str {
        match self {
            MapKind::Inode => "inode",
            MapKind::Zone => "zone",
        }
    }

    fn base_block(self, sup: &SuperInfo) -> u64 {
        match self {
            MapKind::Inode => START_BLOCK,
            MapKind::Zone => START_BLOCK + sup.inode_map_blocks,
        }
    }

    fn blocks(self, sup: &SuperInfo) -> u64 {
        match self {
            MapKind::Inode => sup.inode_map_blocks,
            MapKind::Zone => sup.zone_map_blocks,
        }
    }

    /// The shift from bit number to reported number (the `bit` argument of
    /// `chkmap`, fsck.c:1586/:1588).
    fn first_reported(self, sup: &SuperInfo) -> u64 {
        match self {
            MapKind::Inode => 0,
            MapKind::Zone => sup.first_data_zone - 1,
        }
    }
}

/// Walk options (the `-l`/`-i`/`-z` flag face of C; the repair flags are not part
/// of the read-only deciding half).
#[derive(Debug, Clone, Default)]
pub struct FsckOptions {
    /// `-l`: list every inode during the walk (C `listing`).
    pub listing: bool,
    /// `-i`: inode numbers to watch (C `ilist`; a hit only notes it, no error).
    pub watch_inodes: Vec<u64>,
    /// `-z`: zone numbers to watch (C `zlist`; a hit only notes it, no error).
    pub watch_zones: Vec<u64>,
}

/// The statistics face of one complete check (where C `printtotal` gets its
/// numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FsckSummary {
    /// Regular files (`nregular`).
    pub nregular: u64,
    /// Directories (`ndirectory`).
    pub ndirectory: u64,
    /// Block devices (`nblkspec`).
    pub nblkspec: u64,
    /// Character devices (`ncharspec`).
    pub ncharspec: u64,
    /// Inodes with a bad mode (`nbadinode`).
    pub nbadinode: u64,
    /// FIFOs (`npipe`).
    pub npipe: u64,
    /// Unix sockets (`nsock`).
    pub nsock: u64,
    /// Symbolic links (`nsyml`).
    pub nsyml: u64,
    /// Free inodes (`nfreeinode`, which starts the walk at the total inode
    /// count).
    pub nfreeinode: i64,
    /// Free zones (`nfreezone`, which starts the walk at the data-zone total).
    pub nfreezone: i64,
    /// Zones marked per level (`ztype[0..4]`: direct / single / double / further
    /// indirect).
    pub ztype: [u64; ZONE_LEVELS],
    /// Total number of inconsistencies found (summed on the Rust side; C has no
    /// single counter and only reports them one by one).
    pub errors: u64,
}

/// One path-stack frame of the walk (C `struct stack`, fsck.c:133-137).
struct Frame {
    /// The rendered name (`printname` semantics: truncate at NUL, render
    /// non-printable bytes as `?`).
    name: String,
    /// The inode number this frame points at.
    ino: u64,
    /// The `.`/`..` presence flags (`st_presence`).
    presence: u8,
}

/// The `fsck.mfs` checker: holds one image and the validated geometry, and emits
/// messages phase by phase.
///
/// Messages (progress lines, warnings, inconsistency reports) are collected into
/// [`Fsck::messages`] in C's output-stream order; the checker never modifies the
/// image — the repair face is not in this module. The walk state (expected maps,
/// link counts, path stack) is initialised by [`Fsck::run`].
pub struct Fsck<'a> {
    image: &'a [u8],
    sup: SuperInfo,
    messages: Vec<String>,
    /// The inode expected map built by the walk (`imap`).
    imap: Bitmap,
    /// The zone expected map built by the walk (`zmap`).
    zmap: Bitmap,
    /// Table of visited directories (`dirmap`: a directory may not have a second
    /// link).
    dirmap: Bitmap,
    /// The watch-list inode map (`spec_imap`).
    spec_imap: Bitmap,
    /// The watch-list zone map (`spec_zmap`).
    spec_zmap: Bitmap,
    /// Link-count ledger (`count[]`: +1 for every link found, minus the on-disk
    /// link count when the inode is visited; it must be all zeros at the end).
    counts: Vec<i64>,
    /// The path stack (C's `ftop` chain).
    path: Vec<Frame>,
    /// Type and free counts (the numeric side of `printtotal`).
    stats: FsckSummary,
    /// The `-l` listing switch.
    listing: bool,
    /// Whether the listing header has been printed (C `firstlist`).
    firstlist: bool,
    /// Whether the count-error header has been printed (C `firstcnterr`).
    firstcnterr: bool,
}

impl<'a> Fsck<'a> {
    /// Read the superblock and run every static validation (C `rw_super`
    /// fsck.c:561-599 + `chkdev`'s block-size lower bound :1549-1550 +
    /// `chksuper` fsck.c:602-655).
    ///
    /// Fatal items refuse the check outright; non-fatal deviations (bitmap block
    /// counts disagreeing with the formula, first data zone disagreeing with the
    /// recomputed value, max size disagreeing with the capping formula) become
    /// warning messages and the check goes on.
    pub fn new(image: &'a [u8]) -> Result<Self, Fatal> {
        let at = SUPER_BLOCK_OFFSET;
        let disk = DiskSuperblock::from_bytes(&image[at..at + DiskSuperblock::STORED_BYTES])
            .map_err(|_| Fatal {
                message: String::from("couldn't read super block"),
            })?;
        let (sup, warnings) = Self::validate(&disk, image.len())?;
        let mut messages: Vec<String> = Vec::new();
        messages.extend(warnings);
        Ok(Self {
            image,
            sup,
            messages,
            // The walk state is rebuilt from the geometry in run(); bit 0 is
            // always set (allocbitmap's `*bitmap |= 1`, fsck.c:707) and is put in
            // place here already.
            imap: Bitmap::new(1),
            zmap: Bitmap::new(1),
            dirmap: Bitmap::new(1),
            spec_imap: Bitmap::new(1),
            spec_zmap: Bitmap::new(1),
            counts: Vec::new(),
            path: Vec::new(),
            stats: FsckSummary::default(),
            listing: false,
            firstlist: true,
            firstcnterr: true,
        })
    }

    /// C `rw_super`'s sanity group plus `chksuper`'s deep-check group, in C's
    /// decision order. Non-fatal deviations are returned as warnings together with
    /// the geometry.
    fn validate(disk: &DiskSuperblock, image_len: usize) -> Result<(SuperInfo, Vec<String>), Fatal> {
        // The magic number: the first version is refused outright (fsck.c:578);
        // support for the second carries a host-compilation patch, and this
        // repository takes the same stance as the server side — only the third
        // version is accepted (see the module doc's stance and deviations).
        match disk.magic {
            MAGIC_V1 => {
                return Err(Fatal {
                    message: String::from("Cannot handle V1 file systems"),
                })
            }
            MAGIC_V2 => {
                return Err(Fatal {
                    message: String::from("Cannot handle V2 file systems"),
                })
            }
            MAGIC_V3 => {}
            _ => {
                return Err(Fatal {
                    message: String::from("bad magic number in super block"),
                })
            }
        }
        let block_size = disk.block_size as u64;
        let mut warnings: Vec<String> = Vec::new();
        // rw_super's count and geometry sanity (fsck.c:588-598).
        if disk.inode_count == 0 {
            return Err(Fatal {
                message: String::from("no inodes"),
            });
        }
        if disk.zones == 0 {
            return Err(Fatal {
                message: String::from("no zones"),
            });
        }
        if disk.inode_map_blocks <= 0 {
            return Err(Fatal {
                message: String::from("no imap"),
            });
        }
        if disk.zone_map_blocks <= 0 {
            return Err(Fatal {
                message: String::from("no zmap"),
            });
        }
        // C's test at :592 lands on an in-memory field (which reads stray bytes
        // from the superblock block, see the module doc's stance and deviations);
        // here it lands on the on-disk small field.
        if disk.first_data_zone_small != 0 && disk.first_data_zone_small <= 4 {
            return Err(Fatal {
                message: String::from("first data zone too small"),
            });
        }
        if disk.log_zone_size < 0 {
            return Err(Fatal {
                message: String::from("zone size < block size"),
            });
        }
        let mut max_size = disk.max_size as i64;
        if max_size <= 0 {
            warnings.push(format!(
                "warning: invalid max file size {}",
                disk.max_size
            ));
            max_size = i64::MAX;
        }
        // chkdev's block-size lower bound (fsck.c:1549-1550).
        if block_size < SUPER_BLOCK_OFFSET as u64 {
            return Err(Fatal {
                message: String::from("funny block size"),
            });
        }
        let log_zone_size = disk.log_zone_size as u32;
        // The base-2 log of blocks per zone: test the ceiling first, then warn,
        // and only then talk about shifting (fsck.c:625-628; block_nr is 64-bit,
        // so the ceiling is 64).
        if log_zone_size >= 64 {
            return Err(Fatal {
                message: String::from("log_zone_size too large"),
            });
        }
        if log_zone_size > 8 {
            warnings.push(format!(
                "warning: large log_zone_size ({})",
                log_zone_size
            ));
        }
        let scale = 1u64 << log_zone_size;
        let inode_map_blocks = disk.inode_map_blocks as u64;
        let zone_map_blocks = disk.zone_map_blocks as u64;
        let inodes = disk.inode_count as u64;
        let zones = disk.zones as u64;
        let inode_table_blocks = inodes.div_ceil(inodes_per_block(block_size));
        // chksuper: reconcile the bitmap block counts against the formula
        // (fsck.c:607-624).
        let need_imap = bitmap_size(inodes + 1, block_size);
        if inode_map_blocks < need_imap {
            return Err(Fatal {
                // C's original text misspells blocks as "bocks" (fsck.c:611) and
                // that is kept verbatim.
                message: format!(
                    "need {} bocks for inode bitmap; only have {}\ntoo few imap blocks",
                    need_imap, inode_map_blocks
                ),
            });
        }
        if inode_map_blocks != need_imap {
            warnings.push(format!(
                "warning: expected {} imap_blocks instead of {}",
                need_imap, inode_map_blocks
            ));
        }
        let need_zmap = bitmap_size(zones, block_size);
        if zone_map_blocks < need_zmap {
            return Err(Fatal {
                message: String::from("too few zmap blocks"),
            });
        }
        if zone_map_blocks != need_zmap {
            warnings.push(format!(
                "warning: expected {} zmap_blocks instead of {}",
                need_zmap, zone_map_blocks
            ));
        }
        // First data zone: reconcile the recomputed value with the on-disk small
        // value (fsck.c:629-641).
        let computed_first =
            (Self::blk_ilist(inode_map_blocks, zone_map_blocks) + inode_table_blocks + scale - 1)
                >> log_zone_size;
        let first_data_zone = if disk.first_data_zone_small != 0 {
            let old = disk.first_data_zone_small as u64;
            if old >= zones {
                return Err(Fatal {
                    message: String::from("first data zone too large"),
                });
            }
            if old < computed_first {
                return Err(Fatal {
                    message: String::from("first data zone too small"),
                });
            }
            if old != computed_first {
                warnings.push(format!(
                    "warning: expected first data zone to be {} instead of {}",
                    computed_first, old
                ));
            }
            old
        } else {
            computed_first
        };
        // The max-file-size capping formula (fsck.c:642-646): start from
        // `MAX_FILE_POS`, and when that cannot cover `MAX_ZONES` zones, convert
        // through the zone capacity.
        let max_zones = {
            let ind = block_size / 4;
            7 + ind + ind * ind
        };
        let mut expected_max = MAX_FILE_POS as i128;
        if ((expected_max - 1) >> log_zone_size) / block_size as i128 >= max_zones as i128 {
            expected_max = ((max_zones as i128) * (block_size as i128)) << log_zone_size;
        }
        if expected_max <= 0 {
            expected_max = i64::MAX as i128;
        }
        if max_size as i128 != expected_max {
            warnings.push(format!(
                "warning: expected max size to be {} instead of {}",
                expected_max, max_size
            ));
        }
        // Mandatory feature bits: any high bit of the new format refuses the
        // check (fsck.c:652-654).
        if disk.flags & FLAG_MANDATORY_MASK != 0 {
            return Err(Fatal {
                message: String::from("unsupported feature bits - newer fsck needed"),
            });
        }
        // A reality check on the host image (C's counterpart is a failing device
        // read; an image shorter than the geometry would make every later block
        // read zero-filled noise, so refusing the check is the more honest
        // answer).
        let wanted_bytes = ((zones as u128) << log_zone_size) * (block_size as u128);
        if (image_len as u128) < wanted_bytes {
            return Err(Fatal {
                message: String::from("image shorter than the file system geometry"),
            });
        }
        Ok((
            SuperInfo {
                block_size,
                zones,
                inodes,
                inode_map_blocks,
                zone_map_blocks,
                inode_table_blocks,
                first_data_zone,
                log_zone_size,
                max_size,
                clean: disk.flags & FLAG_CLEAN != 0,
            },
            warnings,
        ))
    }

    /// `BLK_ILIST = 2 + imap + zmap` (fsck.c:114; only the inode table is missing
    /// here).
    fn blk_ilist(inode_map_blocks: u64, zone_map_blocks: u64) -> u64 {
        START_BLOCK + inode_map_blocks + zone_map_blocks
    }

    /// The validated geometry.
    pub fn super_info(&self) -> &SuperInfo {
        &self.sup
    }

    /// The messages accumulated so far (progress lines, warnings, inconsistency
    /// reports), in stream order.
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// The superblock listing (the read-only half of C `lsuper` fsck.c:528-559:
    /// the repair question and the write-back are not in this module, so only the
    /// fields are listed).
    pub fn list_super(&self) -> String {
        let at = SUPER_BLOCK_OFFSET;
        let disk = DiskSuperblock::from_bytes(&self.image[at..at + DiskSuperblock::STORED_BYTES]);
        let disk = disk.expect("superblock validated at construction");
        format!(
            "ninodes       = {}\nnzones        = {}\nimap_blocks   = {}\nzmap_blocks   = {}\nfirstdatazone = {}\nlog_zone_size = {}\nmaxsize       = {}\nblock size    = {}\nflags         = {}\n",
            disk.inode_count,
            disk.zones,
            disk.inode_map_blocks,
            disk.zone_map_blocks,
            disk.first_data_zone_small,
            disk.log_zone_size,
            disk.max_size,
            disk.block_size,
            if self.sup.clean { "CLEAN" } else { "DIRTY" },
        )
    }

    /// Bitmap reconciliation (the read-only half of C `chkmap` fsck.c:813-841):
    /// `expected` is the expected map built by the walk, reconciled bit by bit
    /// against the on-disk map.
    ///
    /// Contract: `expected` must be wide enough to cover every whole block of the
    /// on-disk map (`blocks * block_size * 8` bits) — C's constructed map is
    /// allocated per whole block and the comparison walks all blocks, so the tail
    /// of a bitmap block (padding bits beyond the inode count) takes part in the
    /// reconciliation as well. Returns the number of differences (`nerr`).
    pub fn check_bitmap(&mut self, kind: MapKind, expected: &Bitmap) -> u64 {
        compare_map(self.image, &self.sup, kind, expected, &mut self.messages)
    }

    /// Read bytes from the image (zero-filled past the end: C `devread` zero-fills
    /// when a read fails and carries on, fsck.c:429-432; after the geometry check
    /// the boundary is not normally reached).
    fn read_bytes(&self, at: u64, buf: &mut [u8]) {
        let at = at as usize;
        for (index, out) in buf.iter_mut().enumerate() {
            *out = self.image.get(at + index).copied().unwrap_or(0);
        }
    }

    /// Read one inode record (`inoblock`/`inooff` arithmetic, fsck.c:657-665).
    fn read_inode(&self, ino: u64) -> DiskInode {
        let isz = crate::superblock::INODE_DISK_SIZE as u64;
        let offset = (ino - 1) * isz;
        let block = self.sup.inode_table_block() + offset / self.sup.block_size;
        let at = block * self.sup.block_size + offset % self.sup.block_size;
        let mut buf = [0u8; crate::superblock::INODE_DISK_SIZE];
        self.read_bytes(at, &mut buf);
        // A 64-byte buffer cannot be short-read; the Corrupt branch treats it as an
        // all-zero on-disk record.
        DiskInode::from_bytes(&buf).unwrap_or(DiskInode {
            mode: 0,
            nlinks: 0,
            owner: 0,
            group: 0,
            size: 0,
            accessed: 0,
            modified: 0,
            changed: 0,
            zones: [0; crate::inode::TOTAL_ZONES],
        })
    }

    /// Zone number to byte address (`ztob(z) = z << log_zone_size`, then times
    /// the block size, fsck.c:99-100).
    fn zone_byte_at(&self, zno: u64) -> u64 {
        (zno << self.sup.log_zone_size) * self.sup.block_size
    }

    /// The span of a file hole (C `jump`, fsck.c:1262-1271: start from
    /// `ZONE_SIZE` and multiply by the per-block indirect entry count at each
    /// level).
    fn jump(&self, level: u32) -> i64 {
        let mut power = self.sup.zone_bytes() as i64;
        for _ in 0..level {
            power *= (self.sup.block_size / 4) as i64;
        }
        power
    }

    /// The current path (C `printpath(0)`: the root frame alone renders as `/`,
    /// the rest are joined level by level with `/`; the rendering of `printrec`
    /// fsck.c:354-361).
    fn path_none(&self) -> String {
        if self.path.len() <= 1 {
            return String::from("/");
        }
        let mut out = String::new();
        for frame in &self.path[1..] {
            out.push('/');
            out.push_str(&frame.name);
        }
        out
    }

    /// The current path with an open parenthesis (`printpath(1)`:
    /// `{path} (ino = N, `).
    fn path_open(&self) -> String {
        format!(
            "{} (ino = {}, ",
            self.path_none(),
            self.path.last().map_or(0, |f| f.ino)
        )
    }

    /// The current path with a closing parenthesis (`printpath(2)`:
    /// `{path} (ino = N)`).
    fn path_full(&self) -> String {
        format!(
            "{} (ino = {})",
            self.path_none(),
            self.path.last().map_or(0, |f| f.ino)
        )
    }

    /// The common tail of a zone report (C `errzone`, fsck.c:1196-1213).
    fn errzone(&mut self, mess: &str, zno: u64, level: u32, pos: i64) {
        let kind = match level {
            0 => "DATA",
            1 => "SINGLE INDIRECT",
            2 => "DOUBLE INDIRECT",
            _ => "VERY INDIRECT",
        };
        self.messages.push(format!(
            "{} zone in {}zno = {}, type = {}, pos = {})",
            mess,
            self.path_open(),
            zno,
            kind,
            pos
        ));
    }

    /// Establish the walk state (C `initvars` + `getbitmaps` + `fillbitmap` +
    /// `getcount`, fsck.c:226-238/:766-784/:739-757/:866-870).
    fn init_walk(&mut self, options: &FsckOptions) {
        let bs = self.sup.block_size;
        let imap_bits = self.sup.inode_map_blocks * bits_per_block(bs);
        let zmap_bits = self.sup.zone_map_blocks * bits_per_block(bs);
        self.imap = Bitmap::new(imap_bits);
        self.dirmap = Bitmap::new(imap_bits);
        self.spec_imap = Bitmap::new(imap_bits);
        self.zmap = Bitmap::new(zmap_bits);
        self.spec_zmap = Bitmap::new(zmap_bits);
        self.counts = alloc::vec![0; (self.sup.inodes + 1) as usize];
        self.path = Vec::new();
        self.stats = FsckSummary {
            nfreeinode: self.sup.inodes as i64,
            nfreezone: (self.sup.zones - self.sup.first_data_zone) as i64,
            ..FsckSummary::default()
        };
        self.listing = options.listing;
        self.firstlist = true;
        self.firstcnterr = true;
        // Watch lists: out-of-range entries are ignored with a note (fillbitmap's
        // range branch, fsck.c:749-754).
        for &ino in &options.watch_inodes {
            if ino < 1 || ino > self.sup.inodes {
                self.messages
                    .push(format!("inode number {} out of range (ignored)", ino));
            } else {
                self.spec_imap.set(ino);
            }
        }
        for &zno in &options.watch_zones {
            if zno < self.sup.first_data_zone || zno >= self.sup.zones {
                self.messages
                    .push(format!("zone number {} out of range (ignored)", zno));
            } else {
                self.spec_zmap.set(zno - self.sup.first_data_zone + 1);
            }
        }
    }

    /// The complete check (the walk half of the read-only projection of C
    /// `chkdev`, in the order of fsck.c:1556-1591): walk → zone bitmap
    /// reconciliation → count reconciliation → inode bitmap reconciliation →
    /// free-list zero check → totals. Returns [`FsckSummary`]; fatal items (a root
    /// that is not a directory, an over-long symlink) go through `Err`.
    pub fn run(&mut self, options: &FsckOptions) -> Result<FsckSummary, Fatal> {
        self.init_walk(options);
        // chktree (fsck.c:1490-1501): a pseudo directory entry enters the root.
        let root_ok = self.descendtree(crate::superblock::ROOT_INODE_NUMBER, &[])?;
        if !root_ok {
            return Err(Fatal {
                message: String::from("bad root inode"),
            });
        }
        // C's order: zone map → counts → inode map → free list → totals.
        let zerr = compare_map(
            self.image,
            &self.sup,
            MapKind::Zone,
            &self.zmap,
            &mut self.messages,
        );
        self.stats.errors += zerr;
        self.chkcount();
        let ierr = compare_map(
            self.image,
            &self.sup,
            MapKind::Inode,
            &self.imap,
            &mut self.messages,
        );
        self.stats.errors += ierr;
        self.chkilist();
        self.printtotal();
        Ok(self.stats)
    }

    /// Link-count reconciliation (C `chkcount`, fsck.c:901-908).
    fn chkcount(&mut self) {
        for ino in 1..=self.sup.inodes {
            if self.counts[ino as usize] != 0 {
                self.counterror(ino);
            }
        }
    }

    /// Report the count error of one inode (the read-only half of C
    /// `counterror`, fsck.c:873-892: the header once, add the on-disk link count
    /// back, three columns `ino nlink count`).
    fn counterror(&mut self, ino: u64) {
        if self.firstcnterr {
            self.messages
                .push(String::from("INODE NLINK COUNT"));
            self.firstcnterr = false;
        }
        let inode = self.read_inode(ino);
        self.counts[ino as usize] += inode.nlinks as i64;
        self.messages.push(format!(
            "{:5} {:5} {:5}",
            ino,
            inode.nlinks,
            self.counts[ino as usize]
        ));
        self.stats.errors += 1;
    }

    /// The free-inode zero check (the read-only half of C `chkilist`,
    /// fsck.c:843-864).
    fn chkilist(&mut self) {
        self.messages.push(String::from("Checking inode list."));
        for ino in 1..=self.sup.inodes {
            if !self.imap.test(ino) {
                let inode = self.read_inode(ino);
                if inode.mode != crate::inode::NOT_ALLOCATED {
                    self.messages
                        .push(format!("mode inode {} not cleared", ino));
                    self.stats.errors += 1;
                }
            }
        }
    }

    /// The totals report (C `printtotal`, the non-preen face of
    /// fsck.c:1503-1531).
    fn printtotal(&mut self) {
        self.messages.push(format!(
            "blocksize = {:5}        zonesize  = {:5}",
            self.sup.block_size,
            self.sup.zone_bytes()
        ));
        let s = &self.stats;
        let plural = |n: u64| if n == 1 { "" } else { "s" };
        self.messages.push(format!("{:8}    Regular file{}", s.nregular, plural(s.nregular)));
        self.messages.push(format!(
            "{:8}    Director{}",
            s.ndirectory,
            if s.ndirectory == 1 { "y" } else { "ies" }
        ));
        self.messages
            .push(format!("{:8}    Block special file{}", s.nblkspec, plural(s.nblkspec)));
        self.messages.push(format!(
            "{:8}    Character special file{}",
            s.ncharspec,
            plural(s.ncharspec)
        ));
        if s.nbadinode != 0 {
            self.messages
                .push(format!("{:8}    Bad inode{}", s.nbadinode, plural(s.nbadinode)));
        }
        self.messages.push(format!(
            "{:8}    Free inode{}",
            s.nfreeinode,
            plural(s.nfreeinode as u64)
        ));
        self.messages
            .push(format!("{:8}    Named pipe{}", s.npipe, plural(s.npipe)));
        self.messages
            .push(format!("{:8}    Unix socket{}", s.nsock, plural(s.nsock)));
        self.messages
            .push(format!("{:8}    Symbolic link{}", s.nsyml, plural(s.nsyml)));
        self.messages.push(format!(
            "{:8}    Free zone{}",
            s.nfreezone,
            plural(s.nfreezone as u64)
        ));
    }

    /// The walk entry (the read-only half of C `descendtree`, fsck.c:1454-1488):
    /// push the frame, read and check the inode if not yet visited, pop the frame.
    /// `raw_name` is the directory-entry name buffer (may be empty).
    fn descendtree(&mut self, ino: u64, raw_name: &[u8]) -> Result<bool, Fatal> {
        self.path.push(Frame {
            name: render_name(raw_name),
            ino,
            presence: 0,
        });
        if self.spec_imap.test(ino) {
            let path = self.path_none();
            self.messages.push(format!("found inode {}: {}", ino, path));
        }
        let visited = self.imap.test(ino);
        if !visited || self.listing {
            let inode = self.read_inode(ino);
            if self.listing {
                self.list_inode(ino, &inode);
            }
            if !visited && !self.chkinode(ino, &inode)? {
                // Read-only: the remove question is always "no", so note it and
                // carry on.
                self.spec_imap.set(ino);
            }
        }
        self.path.pop();
        Ok(true)
    }

    /// The inode check (C `chkinode`, fsck.c:1427-1451).
    fn chkinode(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        if ino == crate::superblock::ROOT_INODE_NUMBER
            && inode.mode as u32 & crate::inode::TYPE_MASK != crate::inode::TYPE_DIRECTORY
        {
            return Err(Fatal {
                message: format!(
                    "root inode is not a directory (ino = {}, mode = {:o})",
                    ino, inode.mode
                ),
            });
        }
        if inode.nlinks == 0 {
            let path = self.path_full();
            self.messages.push(format!("link count zero of {}", path));
            self.stats.errors += 1;
            return Ok(false);
        }
        self.stats.nfreeinode -= 1;
        self.imap.set(ino);
        if inode.nlinks as u32 > SHRT_MAX {
            let path = self.path_open();
            self.messages.push(format!(
                "link count too big in {}cnt = {})",
                path, inode.nlinks
            ));
            self.counts[ino as usize] -= SHRT_MAX as i64;
            self.spec_imap.set(ino);
            self.stats.errors += 1;
        } else {
            self.counts[ino as usize] -= inode.nlinks as i64;
        }
        self.chkmode(ino, inode)
    }

    /// Dispatch by type (C `chkmode`, fsck.c:1391-1424).
    fn chkmode(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        match inode.mode as u32 & crate::inode::TYPE_MASK {
            crate::inode::TYPE_REGULAR => {
                self.stats.nregular += 1;
                self.chkfile(ino, inode)
            }
            crate::inode::TYPE_DIRECTORY => {
                self.stats.ndirectory += 1;
                self.chkdirectory(ino, inode)
            }
            crate::inode::TYPE_BLOCK => {
                self.stats.nblkspec += 1;
                self.chkspecial(ino, inode)
            }
            crate::inode::TYPE_CHARACTER => {
                self.stats.ncharspec += 1;
                self.chkspecial(ino, inode)
            }
            TYPE_NAMED_PIPE => {
                self.stats.npipe += 1;
                self.chkfile(ino, inode)
            }
            crate::inode::TYPE_SOCKET => {
                self.stats.nsock += 1;
                self.chkfile(ino, inode)
            }
            crate::inode::TYPE_SYMLINK => {
                self.stats.nsyml += 1;
                self.chklink(ino, inode)
            }
            _ => {
                self.stats.nbadinode += 1;
                let path = self.path_open();
                self.messages
                    .push(format!("bad mode of {}mode = {:o})", path, inode.mode));
                self.stats.errors += 1;
                Ok(false)
            }
        }
    }

    /// The zone walk of a file (C `chkfile`, fsck.c:1313-1322): the seven direct
    /// slots at level 0, the other three slots at levels 1..4.
    fn chkfile(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        let mut pos: i64 = 0;
        let direct = inode.zones[..crate::inode::DIRECT_ZONES].to_vec();
        let mut ok = self.chkzones(ino, inode, &mut pos, &direct, 0)?;
        for slot in crate::inode::DIRECT_ZONES..crate::inode::TOTAL_ZONES {
            let level = (slot - crate::inode::DIRECT_ZONES + 1) as u32;
            let single = [inode.zones[slot]];
            ok &= self.chkzones(ino, inode, &mut pos, &single, level)?;
        }
        Ok(ok)
    }

    /// The walk over a zone list (C `chkzones`, fsck.c:1292-1310): holes are
    /// skipped; a failed mark is skipped and counted as an error; on success the
    /// content check follows.
    fn chkzones(
        &mut self,
        ino: u64,
        inode: &DiskInode,
        pos: &mut i64,
        zlist: &[u32],
        level: u32,
    ) -> Result<bool, Fatal> {
        let mut ok = true;
        for &zno in zlist {
            if zno == 0 {
                *pos += self.jump(level);
            } else if !self.markzone(zno, level, *pos) {
                *pos += self.jump(level);
                ok = false;
            } else if !self.zonechk(ino, inode, pos, zno, level)? {
                ok = false;
            }
        }
        Ok(ok)
    }

    /// Mark a zone (C `markzone`, fsck.c:1218-1239): three states — out of range,
    /// duplicate, watch-list hit; a marked zone enters the expected map and
    /// decrements the free count.
    fn markzone(&mut self, zno: u32, level: u32, pos: i64) -> bool {
        let zno = zno as u64;
        self.stats.ztype[level as usize] += 1;
        if zno < self.sup.first_data_zone || zno >= self.sup.zones {
            self.errzone("out-of-range", zno, level, pos);
            self.stats.errors += 1;
            return false;
        }
        let bit = zno - self.sup.first_data_zone + 1;
        if self.zmap.test(bit) {
            self.spec_zmap.set(bit);
            self.errzone("duplicate", zno, level, pos);
            self.stats.errors += 1;
            return false;
        }
        self.stats.nfreezone -= 1;
        if self.spec_zmap.test(bit) {
            // Watch-list hit: a note, not an error (C reports it without counting).
            self.errzone("found", zno, level, pos);
        }
        self.zmap.set(bit);
        true
    }

    /// Dispatch on the content of one zone (C `zonechk`, fsck.c:1276-1289).
    fn zonechk(
        &mut self,
        ino: u64,
        inode: &DiskInode,
        pos: &mut i64,
        zno: u32,
        level: u32,
    ) -> Result<bool, Fatal> {
        if level == 0 {
            let kind = inode.mode as u32 & crate::inode::TYPE_MASK;
            if kind == crate::inode::TYPE_DIRECTORY && !self.chkdirzone(ino, inode, *pos, zno)? {
                return Ok(false);
            }
            if kind == crate::inode::TYPE_SYMLINK
                && !self.chksymlinkzone(ino, inode, *pos, zno)?
            {
                return Ok(false);
            }
            *pos += self.sup.zone_bytes() as i64;
            Ok(true)
        } else {
            self.chkindzone(ino, inode, pos, zno, level)
        }
    }

    /// The indirect-zone walk (C `chkindzone`, fsck.c:1244-1257): read in chunks
    /// of `CINDIR` entries, stopping once the position passes the file size.
    fn chkindzone(
        &mut self,
        ino: u64,
        inode: &DiskInode,
        pos: &mut i64,
        zno: u32,
        level: u32,
    ) -> Result<bool, Fatal> {
        let ind = (self.sup.block_size / 4) as usize;
        let mut chunks = ind / CINDIR;
        let mut offset = 0usize;
        loop {
            let mut buf = alloc::vec![0u8; CINDIR * 4];
            self.read_bytes(self.zone_byte_at(zno as u64) + offset as u64, &mut buf);
            let entries: Vec<u32> = buf
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            if !self.chkzones(ino, inode, pos, &entries, level - 1)? {
                return Ok(false);
            }
            offset += CINDIR * 4;
            chunks -= 1;
            if chunks == 0 || *pos >= inode.size as i64 {
                return Ok(true);
            }
        }
    }

    /// The directory-zone walk (the read-only half of C `chkdirzone`,
    /// fsck.c:1132-1166): dispatch entry by entry to [`Fsck::chkentry`], and
    /// report "not updated" when the trailing size exceeds the inode size.
    fn chkdirzone(
        &mut self,
        ino: u64,
        inode: &DiskInode,
        pos: i64,
        zno: u32,
    ) -> Result<bool, Fatal> {
        let per_block = self.sup.block_size / crate::inode::DIRECTORY_ENTRY_SIZE as u64;
        let total = (1u64 << self.sup.log_zone_size) * per_block;
        let zone_at = self.zone_byte_at(zno as u64);
        let mut entry_pos = pos;
        let mut size: i64 = 0;
        for index in 0..total {
            let mut buf = [0u8; crate::inode::DIRECTORY_ENTRY_SIZE];
            self.read_bytes(
                zone_at + index * crate::inode::DIRECTORY_ENTRY_SIZE as u64,
                &mut buf,
            );
            let inum = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            if inum != crate::inode::NO_ENTRY as u32
                && !self.chkentry(ino, entry_pos, inum as u64, &buf[4..])?
            {
                // C's dirty face only matters during repair; read-only carries
                // straight on.
            }
            entry_pos += crate::inode::DIRECTORY_ENTRY_SIZE as i64;
            if inum != crate::inode::NO_ENTRY as u32 {
                size = entry_pos;
            }
        }
        if size > inode.size as i64 {
            let path = self.path_full();
            self.messages
                .push(format!("size not updated of directory {}", path));
            self.stats.errors += 1;
        }
        Ok(true)
    }

    /// The symlink-target check (C `chksymlinkzone`, fsck.c:1169-1194): a size
    /// beyond PATH_MAX is a self-consistency break of the checker itself (fatal);
    /// a NUL position disagreeing with the size is reported as a bad size.
    fn chksymlinkzone(
        &mut self,
        _ino: u64,
        inode: &DiskInode,
        _pos: i64,
        zno: u32,
    ) -> Result<bool, Fatal> {
        if inode.size > PATH_MAX_LIMIT {
            return Err(Fatal {
                message: String::from("chksymlinkzone: fsck program inconsistency"),
            });
        }
        let mut buf = alloc::vec![0u8; inode.size.max(0) as usize];
        self.read_bytes(self.zone_byte_at(zno as u64), &mut buf);
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        if len as i64 != inode.size as i64 {
            let path = self.path_full();
            self.messages.push(format!(
                "bad size in symbolic link ({} instead of {}) {}",
                inode.size, len, path
            ));
            self.stats.errors += 1;
        }
        Ok(true)
    }

    /// The directory-content and `.`/`..` checks (C `chkdirectory`,
    /// fsck.c:1325-1342).
    fn chkdirectory(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        self.dirmap.set(ino);
        let mut ok = self.chkfile(ino, inode)?;
        let presence = self.path.last().map_or(0, |f| f.presence);
        if presence & DOT == 0 {
            let path = self.path_full();
            self.messages.push(format!(". missing in {}", path));
            self.stats.errors += 1;
            ok = false;
        }
        if presence & DOTDOT == 0 {
            let path = self.path_full();
            self.messages.push(format!(".. missing in {}", path));
            self.stats.errors += 1;
            ok = false;
        }
        Ok(ok)
    }

    /// The whole-symlink check (C `chklink`, fsck.c:1347-1361): both an empty link
    /// and an over-block size are reported.
    fn chklink(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        let mut ok = self.chkfile(ino, inode)?;
        if inode.size <= 0 || inode.size > self.sup.block_size as i32 {
            let path = self.path_full();
            if inode.size == 0 {
                self.messages.push(format!("empty symbolic link {}", path));
            } else {
                self.messages.push(format!(
                    "symbolic link too large (size {}) {}",
                    inode.size, path
                ));
            }
            self.stats.errors += 1;
            ok = false;
        }
        Ok(ok)
    }

    /// The special-file check (C `chkspecial`, fsck.c:1366-1388): device number 0
    /// is illegal, and every other slot must be zeroed.
    fn chkspecial(&mut self, _ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        let mut ok = true;
        let path = self.path_full();
        if inode.zones[0] == 0 {
            self.messages.push(format!(
                "illegal device number {} for special file {}",
                inode.zones[0], path
            ));
            self.stats.errors += 1;
            ok = false;
        }
        for &zno in &inode.zones[1..] {
            if zno != 0 {
                self.messages.push(format!(
                    "nonzero zone number {} for special file {}",
                    zno, path
                ));
                self.stats.errors += 1;
                ok = false;
            }
        }
        Ok(ok)
    }

    /// The directory-entry check (the read-only half of C `chkentry`,
    /// fsck.c:1084-1127): inode number range, count ceiling, `.`/`..`
    /// reconciliation, name validity, a second link to a directory, and
    /// recursion.
    fn chkentry(
        &mut self,
        dir_ino: u64,
        pos: i64,
        inum: u64,
        raw_name: &[u8],
    ) -> Result<bool, Fatal> {
        if inum < crate::superblock::ROOT_INODE_NUMBER || inum > self.sup.inodes {
            let path = self.path_open();
            self.messages.push(format!(
                "bad inode found in directory {}ino found = {}, name = '{}')",
                path,
                inum,
                render_name(raw_name)
            ));
            self.stats.errors += 1;
            return Ok(true);
        }
        if self.counts[inum as usize] == SHRT_MAX as i64 {
            let path = self.path_none();
            self.messages.push(format!(
                "too many links to ino {}\ndiscovered at entry '{}' in directory {}\n",
                inum,
                render_name(raw_name),
                path
            ));
            self.stats.errors += 1;
            // Read-only: the Remove question is always "no", so the count still
            // advances.
        }
        self.counts[inum as usize] += 1;
        let name = cstr_bytes(raw_name);
        if name == b"." {
            if let Some(frame) = self.path.last_mut() {
                frame.presence |= DOT;
            }
            return Ok(self.chkdots(dir_ino, pos, inum, raw_name, dir_ino));
        }
        if name == b".." {
            if let Some(frame) = self.path.last_mut() {
                frame.presence |= DOTDOT;
            }
            // The parent directory number: the root's `..` points at the root,
            // everything else takes the next frame down the stack
            // (fsck.c:1114-1115).
            let expected = if dir_ino == crate::superblock::ROOT_INODE_NUMBER {
                dir_ino
            } else {
                self.path
                    .get(self.path.len() - 2)
                    .map_or(dir_ino, |f| f.ino)
            };
            return Ok(self.chkdots(dir_ino, pos, inum, raw_name, expected));
        }
        if !self.chkname(dir_ino, raw_name) {
            return Ok(false);
        }
        if self.dirmap.test(inum) {
            let path = self.path_open();
            self.messages.push(format!(
                "link to directory discovered in {}name = '{}', dir ino = {})",
                path,
                render_name(raw_name),
                inum
            ));
            self.stats.errors += 1;
            return Ok(true);
        }
        self.descendtree(inum, raw_name)
    }

    /// Reconcile `.`/`..` targets and positions (the read-only half of C `chkdots`,
    /// fsck.c:1022-1053): a wrong target is an error, a wrong position a warning;
    /// the three watch-list marks go out either way.
    fn chkdots(
        &mut self,
        dir_ino: u64,
        pos: i64,
        inum: u64,
        raw_name: &[u8],
        expected: u64,
    ) -> bool {
        let rendered = render_name(raw_name);
        let second = raw_name.get(1).copied().unwrap_or(0);
        let expected_pos = if second != 0 {
            crate::inode::DIRECTORY_ENTRY_SIZE as i64
        } else {
            0
        };
        if inum != expected {
            let path = self.path_open();
            self.messages.push(format!(
                "bad {} in {}{} is linked to {} instead of {})",
                rendered, path, rendered, inum, expected
            ));
            self.spec_imap.set(dir_ino);
            self.spec_imap.set(inum);
            self.spec_imap.set(expected);
            self.stats.errors += 1;
        } else if pos != expected_pos {
            let path = self.path_open();
            self.messages.push(format!(
                "warning: {} has offset {} in {}{} is linked to {})",
                rendered, pos, path, rendered, inum
            ));
            self.spec_imap.set(dir_ino);
            self.spec_imap.set(inum);
            self.spec_imap.set(expected);
            self.stats.errors += 1;
        }
        true
    }

    /// Name validity (the read-only half of C `chkname`, fsck.c:1056-1079): both a
    /// null name and one containing `/` are reported, and the directory is marked.
    fn chkname(&mut self, dir_ino: u64, raw_name: &[u8]) -> bool {
        if raw_name.first().copied().unwrap_or(0) == 0 {
            let path = self.path_none();
            self.messages.push(format!("null name found in {}", path));
            self.spec_imap.set(dir_ino);
            self.stats.errors += 1;
            return true;
        }
        for &byte in cstr_bytes(raw_name) {
            if byte == b'/' {
                let path = self.path_open();
                self.messages.push(format!(
                    "found a '/' in entry of directory {}entry = '{}')",
                    path,
                    render_name(raw_name)
                ));
                self.spec_imap.set(dir_ino);
                self.stats.errors += 1;
                break;
            }
        }
        true
    }

    /// One inode listing line (C `list`, fsck.c:936-968): type character, nine
    /// permission bits, link count, size or device number, path.
    fn list_inode(&mut self, ino: u64, inode: &DiskInode) {
        if self.firstlist {
            self.messages
                .push(String::from(" inode permission link   size name"));
            self.firstlist = false;
        }
        let kind = inode.mode as u32 & crate::inode::TYPE_MASK;
        let type_char = match kind {
            crate::inode::TYPE_REGULAR => '-',
            crate::inode::TYPE_DIRECTORY => 'd',
            crate::inode::TYPE_CHARACTER => 'c',
            crate::inode::TYPE_BLOCK => 'b',
            TYPE_NAMED_PIPE => 'p',
            crate::inode::TYPE_SOCKET => 's',
            crate::inode::TYPE_SYMLINK => 'l',
            _ => '?',
        };
        // printperm: nine permission bits, the special bits overlaid as letters
        // (s/s/t).
        let mut perms = String::new();
        for (shift, special, overlay) in [
            (6u32, 0o4000u32, 's'),
            (3, 0o2000, 's'),
            (0, 0o1000, 't'),
        ] {
            let bit = |mask: u32| inode.mode as u32 >> shift & mask != 0;
            perms.push(if bit(4) { 'r' } else { '-' });
            perms.push(if bit(2) { 'w' } else { '-' });
            perms.push(if inode.mode as u32 & special != 0 {
                overlay
            } else if bit(1) {
                'x'
            } else {
                '-'
            });
        }
        let body = match kind {
            crate::inode::TYPE_CHARACTER | crate::inode::TYPE_BLOCK => format!(
                "  {:2x},{:2x} ",
                inode.zones[0] >> 8,
                inode.zones[0] & 0xff
            ),
            _ => format!("{:7} ", inode.size),
        };
        let path = self.path_none();
        self.messages.push(format!(
            "{:6} {}{} {:3} {}{}",
            ino, type_char, perms, inode.nlinks, body, path
        ));
    }
}

/// The bitmap reconciliation core (the bit-by-bit reconciliation of C `chkmap`
/// fsck.c:813-841): `expected` is compared against the on-disk map bit by bit and
/// reported with `chkword` semantics (on disk but not expected → "missing"; clear
/// on disk but expected → "not free"), truncated by MAXPRINT. Returns the number of
/// differences.
///
/// A free function rather than a method: in `run` the expected map and the message
/// vector are both `self` fields, and the split-field borrows are gathered here.
fn compare_map(
    image: &[u8],
    sup: &SuperInfo,
    kind: MapKind,
    expected: &Bitmap,
    messages: &mut Vec<String>,
) -> u64 {
    messages.push(format!("Checking {} map.", kind.label()));
    let blocks = kind.blocks(sup);
    let total_bits = blocks * bits_per_block(sup.block_size);
    debug_assert_eq!(
        expected.bit_count(),
        total_bits,
        "expected map must cover the full on-disk map blocks"
    );
    let byte_base = (kind.base_block(sup) * sup.block_size) as usize;
    let disk_bit = |bit: u64| {
        image
            .get(byte_base + (bit / 8) as usize)
            .is_some_and(|byte| byte & (1 << (bit % 8)) != 0)
    };
    let mut nerr: u64 = 0;
    let mut report = true;
    for bit in 0..total_bits {
        let on_disk = disk_bit(bit);
        let wanted = expected.test(bit);
        if on_disk == wanted {
            continue;
        }
        nerr += 1;
        // C `chkword`: every MAXPRINT-th difference goes through the silent arm
        // (that one is not listed); after that only counting happens. In read-only
        // mode the listing ends with an "etc.".
        if nerr.is_multiple_of(MAXPRINT) {
            if report {
                report = false;
            }
        } else if report {
            let number = kind.first_reported(sup) + bit;
            match (on_disk, wanted) {
                (true, false) => messages.push(format!(
                    "{} {} is missing",
                    kind.label(),
                    number
                )),
                (false, true) => messages.push(format!(
                    "{} {} is not free",
                    kind.label(),
                    number
                )),
                _ => unreachable!("bits differ by construction"),
            }
        }
    }
    if !report {
        messages.push(String::from("etc."));
    }
    if nerr > MAXPRINT || nerr > 10 {
        messages.push(format!("{} errors found.", nerr));
    }
    nerr
}

/// Render a directory-entry name (C `printname`, fsck.c:337-349: truncate at NUL,
/// turn non-printable bytes into `?`).
fn render_name(raw: &[u8]) -> String {
    let mut out = String::new();
    for &byte in raw.iter().take(crate::inode::NAME_CAPACITY) {
        if byte == 0 {
            break;
        }
        out.push(if (0x20..=0x7E).contains(&byte) {
            byte as char
        } else {
            '?'
        });
    }
    out
}

/// The C-string view of a name buffer (up to the first NUL, or the whole buffer
/// when there is none).
fn cstr_bytes(raw: &[u8]) -> &[u8] {
    let end = raw
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(raw.len());
    &raw[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkfs::{build_image, plan_layout};

    /// The baseline image: 64 blocks, 64 inodes, 4096-byte blocks — one block each
    /// for the imap, the zmap and the inode table, with the first data zone at 5.
    fn base_image() -> (alloc::vec::Vec<u8>, crate::mkfs::MkfsPlan) {
        let plan = plan_layout(64, Some(64), 4096).unwrap();
        let image = build_image(&plan, 1_000).unwrap();
        (image, plan)
    }

    /// The expected maps matching the baseline image: bit 0 is always set, and the
    /// root inode and root data zone occupy position 1.
    fn expected_maps() -> (Bitmap, Bitmap) {
        let bits = 4096 * 8;
        let mut imap = Bitmap::new(bits);
        let mut zmap = Bitmap::new(bits);
        imap.set(1);
        zmap.set(1);
        (imap, zmap)
    }

    /// Extract the fatal error (`unwrap_err` would require the `Ok` side to be
    /// debuggable, and an image is not worth that).
    fn fatal_of(result: Result<Fsck, Fatal>) -> Fatal {
        match result {
            Err(fatal) => fatal,
            Ok(_) => panic!("expected a fatal error"),
        }
    }

    #[test]
    fn clean_image_passes_super_checks() {
        let (image, plan) = base_image();
        let fsck = Fsck::new(&image).unwrap();
        let sup = fsck.super_info();
        assert_eq!(sup.block_size, 4096);
        assert_eq!(sup.zones, 64);
        assert_eq!(sup.inodes, 64);
        assert_eq!(sup.inode_map_blocks, 1);
        assert_eq!(sup.zone_map_blocks, 1);
        assert_eq!(sup.inode_table_blocks, 1);
        assert_eq!(sup.first_data_zone, plan.first_data_zone);
        assert_eq!(sup.log_zone_size, 0);
        assert_eq!(sup.max_size, i32::MAX as i64);
        assert!(sup.clean);
        assert!(fsck.messages().is_empty(), "干净镜像无警告");
    }

    #[test]
    fn clean_maps_agree_bit_for_bit() {
        let (image, _plan) = base_image();
        let (imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 0);
        assert_eq!(fsck.check_bitmap(MapKind::Inode, &imap), 0);
        assert_eq!(
            fsck.messages(),
            ["Checking zone map.", "Checking inode map."]
        );
    }

    #[test]
    fn cleared_zone_bit_reports_not_free() {
        let (mut image, _plan) = base_image();
        // The zone bitmap is in block 3; clear bit 1 (the root data zone).
        let at = 3 * 4096;
        image[at] &= !0b10;
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 1);
        assert!(fsck.messages().contains(&String::from("zone 5 is not free")));
    }

    #[test]
    fn stray_zone_bit_reports_missing() {
        let (mut image, _plan) = base_image();
        // Set the bit of a free zone (bit 7 → zone 11).
        let at = 3 * 4096;
        image[at] |= 0b1000_0000;
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 1);
        assert!(fsck.messages().contains(&String::from("zone 11 is missing")));
    }

    #[test]
    fn padding_bits_beyond_geometry_are_also_checked() {
        let (mut image, _plan) = base_image();
        // The padding bits at the tail of the bitmap block (bit 32000, far beyond
        // the 64 zones) take part in the reconciliation as well.
        let at = 3 * 4096 + (32000 / 8) as usize;
        image[at] |= 1 << (32000 % 8);
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 1);
        // Reported number = bit number + first data zone - 1 = 32000 + 4.
        assert!(fsck
            .messages()
            .contains(&String::from("zone 32004 is missing")));
    }

    #[test]
    fn error_flood_truncates_listing_but_counts_all() {
        let (mut image, _plan) = base_image();
        // Set 100 stray bits (bits 100..200): the first 79 are listed one by one,
        // the 80th takes the silent arm, and after that only counting happens.
        for bit in 100u64..200 {
            let at = 3 * 4096 + (bit / 8) as usize;
            image[at] |= 1 << (bit % 8);
        }
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 100);
        let listed = fsck
            .messages()
            .iter()
            .filter(|m| m.contains(" is missing"))
            .count();
        assert_eq!(listed, (MAXPRINT - 1) as usize, "MAXPRINT 条静音前只列 79 条");
        assert!(fsck.messages().contains(&String::from("etc.")));
        assert!(fsck
            .messages()
            .contains(&String::from("100 errors found.")));
    }

    #[test]
    fn small_error_count_has_no_summary() {
        let (mut image, _plan) = base_image();
        // Two differences (nerr ≤ 10 and ≤ MAXPRINT): no total is reported
        // (fsck.c:838).
        let at = 3 * 4096;
        image[at] |= 0b0001_0000; // bit 4 → zone 8
        image[at] |= 0b0010_0000; // bit 5 → zone 9
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 2);
        assert!(!fsck
            .messages()
            .iter()
            .any(|m| m.contains("errors found.")));
    }

    /// Helper that rewrites superblock fields: rebuild the 31 on-disk bytes and
    /// write them back into the image.
    fn rewrite_super(image: &mut [u8], patch: impl FnOnce(&mut DiskSuperblock)) {
        let at = SUPER_BLOCK_OFFSET;
        let mut sup =
            DiskSuperblock::from_bytes(&image[at..at + DiskSuperblock::STORED_BYTES]).unwrap();
        patch(&mut sup);
        image[at..at + DiskSuperblock::STORED_BYTES].copy_from_slice(&sup.to_bytes());
    }

    #[test]
    fn bad_magic_is_fatal() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.magic = 0x1234);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("bad magic number in super block")
            }
        );
    }

    #[test]
    fn v1_and_v2_magic_are_refused() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.magic = MAGIC_V1);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("Cannot handle V1 file systems")
            }
        );
        rewrite_super(&mut image, |sup| sup.magic = MAGIC_V2);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("Cannot handle V2 file systems")
            }
        );
    }

    #[test]
    fn zero_counters_are_fatal() {
        for (field, message) in [
            // (field name, rw_super's short message)
            ("inodes", "no inodes"),
            ("zones", "no zones"),
        ] {
            let (mut image, _plan) = base_image();
            rewrite_super(&mut image, |sup| match field {
                "inodes" => sup.inode_count = 0,
                _ => sup.zones = 0,
            });
            assert_eq!(
                fatal_of(Fsck::new(&image)),
                Fatal {
                    message: String::from(message)
                }
            );
        }
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.inode_map_blocks = 0);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("no imap")
            }
        );
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.zone_map_blocks = 0);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("no zmap")
            }
        );
    }

    #[test]
    fn too_few_imap_blocks_fatal_names_the_arithmetic() {
        let (mut image, _plan) = base_image();
        // 200_000 inodes need ceil(200_001/32768) = 7 bitmap blocks, but only 1 is
        // present.
        rewrite_super(&mut image, |sup| sup.inode_count = 200_000);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from(
                    "need 7 bocks for inode bitmap; only have 1\ntoo few imap blocks"
                )
            }
        );
    }

    #[test]
    fn surplus_imap_blocks_warn_but_pass() {
        let (mut image, _plan) = base_image();
        // One extra imap block: the inode table shifts, so the recomputed first
        // data zone becomes 6 — the on-disk small value is moved to 6 along with
        // it, leaving only the bitmap-block-count warning.
        rewrite_super(&mut image, |sup| {
            sup.inode_map_blocks = 2;
            sup.first_data_zone_small = 6;
        });
        let fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.super_info().inode_map_blocks, 2);
        assert!(fsck
            .messages()
            .contains(&String::from(
                "warning: expected 1 imap_blocks instead of 2"
            )));
    }

    #[test]
    fn first_data_zone_bounds_are_fatal() {
        let (mut image, _plan) = base_image();
        // old >= zones (fsck.c:631-632).
        rewrite_super(&mut image, |sup| sup.first_data_zone_small = 64);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("first data zone too large")
            }
        );
        // old <= 4 (rw_super's test, landing on the on-disk small field).
        rewrite_super(&mut image, |sup| sup.first_data_zone_small = 3);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("first data zone too small")
            }
        );
        // old < the recomputed value: with 128 inodes the layout's first data zone
        // is 6, so old=5 falls inside the inode table.
        let mut plan = plan_layout(64, Some(64), 4096).unwrap();
        plan.inodes = 128;
        let mut image = build_image(&plan, 1_000).unwrap();
        rewrite_super(&mut image, |sup| sup.first_data_zone_small = 5);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("first data zone too small")
            }
        );
    }

    #[test]
    fn first_data_zone_mismatch_warns_and_keeps_disk_value() {
        let (mut image, _plan) = base_image();
        // A later first data zone that the layout allows: warn and adopt the
        // on-disk value (fsck.c:635-640).
        rewrite_super(&mut image, |sup| sup.first_data_zone_small = 6);
        let fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.super_info().first_data_zone, 6);
        assert!(fsck.messages().contains(&String::from(
            "warning: expected first data zone to be 5 instead of 6"
        )));
    }

    #[test]
    fn log_zone_size_bounds() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.log_zone_size = 64);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("log_zone_size too large")
            }
        );
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| {
            sup.log_zone_size = -1;
        });
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("zone size < block size")
            }
        );
    }

    #[test]
    fn max_size_mismatch_warns() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.max_size = 1_000);
        let fsck = Fsck::new(&image).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "warning: expected max size to be 2147483647 instead of 1000"
        )));
    }

    #[test]
    fn mandatory_feature_bits_are_fatal() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.flags |= 0x0100);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("unsupported feature bits - newer fsck needed")
            }
        );
    }

    #[test]
    fn tiny_block_size_is_fatal() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.block_size = 512);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("funny block size")
            }
        );
    }

    #[test]
    fn short_image_is_fatal() {
        let (image, _plan) = base_image();
        let cut = &image[..4_000];
        assert_eq!(
            fatal_of(Fsck::new(cut)),
            Fatal {
                message: String::from("image shorter than the file system geometry")
            }
        );
    }

    #[test]
    fn list_super_lists_all_disk_fields() {
        let (image, plan) = base_image();
        let fsck = Fsck::new(&image).unwrap();
        let text = fsck.list_super();
        assert!(text.contains(&format!("ninodes       = {}", plan.inodes)));
        assert!(text.contains(&format!("nzones        = {}", plan.zones)));
        assert!(text.contains("firstdatazone = 5"));
        assert!(text.contains("block size    = 4096"));
        assert!(text.contains("flags         = CLEAN"));
    }

    // ---- Part two: the walk, the counts and the directory checks ----
    //
    // Baseline layout (64 blocks, 64 inodes, 4096 bytes): imap in block 2, zmap
    // in block 3, inode table in block 4, first data zone 5. The root is inode 1
    // and the root directory's data zone is 5.

    const IMAP_BLOCK: u64 = 2;
    const ZMAP_BLOCK: u64 = 3;
    const INODE_TABLE_BLOCK: u64 = 4;
    const FIRST_ZONE: u64 = 5;

    fn set_map_bit(image: &mut [u8], map_block: u64, bit: u64) {
        let at = (map_block * 4096 + bit / 8) as usize;
        image[at] |= 1 << (bit % 8);
    }

    fn write_inode(image: &mut [u8], ino: u64, inode: &DiskInode) {
        let at = (INODE_TABLE_BLOCK * 4096 + (ino - 1) * 64) as usize;
        image[at..at + 64].copy_from_slice(&inode.to_bytes());
    }

    fn write_entry(image: &mut [u8], zone: u64, slot: usize, name: &[u8], inum: u64) {
        let at = (zone * 4096 + slot as u64 * 64) as usize;
        image[at..at + 4].copy_from_slice(&(inum as u32).to_le_bytes());
        let n = name.len().min(60);
        image[at + 4..at + 4 + n].copy_from_slice(&name[..n]);
    }

    fn clear_entry(image: &mut [u8], zone: u64, slot: usize) {
        let at = (zone * 4096 + slot as u64 * 64) as usize;
        image[at..at + 64].fill(0);
    }

    /// The standard shape of a directory inode (starting from the `.`/`..`
    /// entries).
    fn dir_inode(nlinks: u16, size: i32, zone: u64) -> DiskInode {
        DiskInode {
            mode: (crate::inode::TYPE_DIRECTORY | 0o777) as u16,
            nlinks,
            owner: 0,
            group: 0,
            size,
            accessed: 1_000,
            modified: 1_000,
            changed: 1_000,
            zones: {
                let mut z = [0u32; crate::inode::TOTAL_ZONES];
                z[0] = zone as u32;
                z
            },
        }
    }

    /// A regular-file inode.
    fn file_inode(nlinks: u16, size: i32, zones: &[u32]) -> DiskInode {
        let mut inode = DiskInode {
            mode: (crate::inode::TYPE_REGULAR | 0o644) as u16,
            nlinks,
            owner: 0,
            group: 0,
            size,
            accessed: 1_000,
            modified: 1_000,
            changed: 1_000,
            zones: [0u32; crate::inode::TOTAL_ZONES],
        };
        for (slot, zone) in zones.iter().enumerate() {
            inode.zones[slot] = *zone;
        }
        inode
    }

    #[test]
    fn clean_empty_fs_full_run_is_clean() {
        let (image, _plan) = base_image();
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert_eq!(summary.errors, 0);
        assert_eq!(summary.ndirectory, 1);
        assert_eq!(summary.nfreeinode, 63);
        assert_eq!(summary.nfreezone, 58, "64 - 首数据区 5 - 根占 1");
        assert_eq!(summary.ztype, [1, 0, 0, 0]);
        // Both the progress lines of the check order and the totals table are
        // present.
        assert!(fsck
            .messages()
            .contains(&String::from("Checking zone map.")));
        assert!(fsck
            .messages()
            .contains(&String::from("Checking inode list.")));
        assert!(fsck.messages().contains(&String::from(
            "blocksize =  4096        zonesize  =  4096"
        )));
        assert!(fsck
            .messages()
            .contains(&String::from("       0    Regular files")));
        assert!(fsck
            .messages()
            .contains(&String::from("       1    Directory")));
        assert!(fsck
            .messages()
            .contains(&String::from("      63    Free inodes")));
        assert!(fsck
            .messages()
            .contains(&String::from("      58    Free zones")));
    }

    #[test]
    fn link_count_mismatch_is_tallied() {
        let (mut image, _plan) = base_image();
        // File f (inode 2) has an on-disk link count of 2 but only one entry in
        // the directory.
        write_inode(
            &mut image,
            1,
            &dir_inode(2, 3 * 64, FIRST_ZONE),
        );
        write_inode(&mut image, 2, &file_inode(2, 0, &[]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert!(summary.errors >= 1);
        assert!(fsck
            .messages()
            .contains(&String::from("INODE NLINK COUNT")));
        // Three columns: ino, on-disk link count, entries found in directories.
        assert!(fsck
            .messages()
            .contains(&String::from("    2     2     1")));
        assert_eq!(summary.nregular, 1);
    }

    #[test]
    fn dangling_entry_inode_reports_bad_inode() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_entry(&mut image, FIRST_ZONE, 2, b"x", 99);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "bad inode found in directory / (ino = 1, ino found = 99, name = 'x')"
        )));
    }

    #[test]
    fn missing_dotdot_reported() {
        let (mut image, _plan) = base_image();
        clear_entry(&mut image, FIRST_ZONE, 1);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck
            .messages()
            .contains(&String::from(".. missing in / (ino = 1)")));
        // `.` is still there while `..` is missing: the root's count is off by
        // one.
        assert!(summary.errors >= 2);
    }

    #[test]
    fn out_of_range_zone_reported() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &file_inode(1, 4096, &[999]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "out-of-range zone in /f (ino = 2, zno = 999, type = DATA, pos = 0)"
        )));
    }

    #[test]
    fn duplicate_zone_reported() {
        let (mut image, _plan) = base_image();
        // Two files both point at the root's data zone 5: the second walk hits a
        // duplicate.
        write_inode(&mut image, 1, &dir_inode(2, 4 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &file_inode(1, 100, &[5]));
        write_inode(&mut image, 3, &file_inode(1, 100, &[5]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        write_entry(&mut image, FIRST_ZONE, 3, b"g", 3);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, IMAP_BLOCK, 3);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "duplicate zone in /f (ino = 2, zno = 5, type = DATA, pos = 0)"
        )));
    }

    #[test]
    fn link_to_directory_reported() {
        let (mut image, _plan) = base_image();
        // Subdirectory d (inode 2) plus a second entry "hard": a directory may not
        // have a second link.
        write_inode(&mut image, 1, &dir_inode(3, 4 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &dir_inode(2, 2 * 64, 6));
        write_entry(&mut image, FIRST_ZONE, 2, b"d", 2);
        write_entry(&mut image, FIRST_ZONE, 3, b"hard", 2);
        write_entry(&mut image, 6, 0, b".", 2);
        write_entry(&mut image, 6, 1, b"..", 1);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, ZMAP_BLOCK, 2); // data zone 6 → bit 2
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "link to directory discovered in / (ino = 1, name = 'hard', dir ino = 2)"
        )));
    }

    #[test]
    fn slash_in_name_reported() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_entry(&mut image, FIRST_ZONE, 2, b"a/b", 3);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "found a '/' in entry of directory / (ino = 1, entry = 'a/b')"
        )));
    }

    #[test]
    fn bad_mode_reported_and_counted() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut bad = file_inode(1, 0, &[]);
        bad.mode = 0o160644; // type bits 160000 match no known type
        write_inode(&mut image, 2, &bad);
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "bad mode of /f (ino = 2, mode = 160644)"
        )));
        assert_eq!(summary.nbadinode, 1);
    }

    #[test]
    fn symlink_bad_size_reported() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut link = file_inode(1, 5, &[6]);
        link.mode = (crate::inode::TYPE_SYMLINK | 0o777) as u16;
        write_inode(&mut image, 2, &link);
        write_entry(&mut image, FIRST_ZONE, 2, b"l", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, ZMAP_BLOCK, 2); // data zone 6
        // Target content "hel\0lo": the NUL is at 3 while the size says 5.
        let at = (6 * 4096) as usize;
        image[at..at + 6].copy_from_slice(b"hel\0lo");
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "bad size in symbolic link (5 instead of 3) /l (ino = 2)"
        )));
    }

    #[test]
    fn empty_symlink_reported() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut link = file_inode(1, 0, &[]);
        link.mode = (crate::inode::TYPE_SYMLINK | 0o777) as u16;
        write_inode(&mut image, 2, &link);
        write_entry(&mut image, FIRST_ZONE, 2, b"l", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck
            .messages()
            .contains(&String::from("empty symbolic link /l (ino = 2)")));
        assert_eq!(summary.nsyml, 1);
    }

    #[test]
    fn special_file_checks() {
        // A legal character device: clean, counted in ncharspec.
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut dev = file_inode(1, 0, &[]);
        dev.mode = (crate::inode::TYPE_CHARACTER | 0o660) as u16;
        dev.zones[0] = (4 << 8) | 5;
        write_inode(&mut image, 2, &dev);
        write_entry(&mut image, FIRST_ZONE, 2, b"null", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck
            .run(&FsckOptions {
                listing: true,
                ..FsckOptions::default()
            })
            .unwrap();
        assert_eq!(summary.errors, 0);
        assert_eq!(summary.ncharspec, 1);
        assert!(fsck.messages().contains(&String::from(
            "     2 crw-rw----   1    4, 5 /null"
        )));
        // An illegal device number 0 plus a leftover zone number: both are
        // reported.
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut dev = file_inode(1, 0, &[]);
        dev.mode = (crate::inode::TYPE_BLOCK | 0o660) as u16;
        dev.zones[1] = 7;
        write_inode(&mut image, 2, &dev);
        write_entry(&mut image, FIRST_ZONE, 2, b"hdb", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "illegal device number 0 for special file /hdb (ino = 2)"
        )));
        assert!(fsck.messages().contains(&String::from(
            "nonzero zone number 7 for special file /hdb (ino = 2)"
        )));
    }

    #[test]
    fn listing_half_lists_root() {
        let (image, _plan) = base_image();
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions {
            listing: true,
            ..FsckOptions::default()
        })
        .unwrap();
        assert_eq!(
            fsck.messages()[0],
            " inode permission link   size name"
        );
        assert_eq!(fsck.messages()[1], "     1 drwxrwxrwx   2     128 /");
    }

    #[test]
    fn watch_lists_hint_without_erroring() {
        let (image, _plan) = base_image();
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck
            .run(&FsckOptions {
                watch_zones: alloc::vec![5],
                watch_inodes: alloc::vec![500],
                ..FsckOptions::default()
            })
            .unwrap();
        // A watch-list hit is a note: marking the root data zone reports "found"
        // without counting an error.
        assert!(fsck.messages().contains(&String::from(
            "found zone in / (ino = 1, zno = 5, type = DATA, pos = 0)"
        )));
        assert!(fsck
            .messages()
            .contains(&String::from("inode number 500 out of range (ignored)")));
        assert_eq!(summary.errors, 0);
    }

    #[test]
    fn size_not_updated_reported() {
        let (mut image, _plan) = base_image();
        // The root's size records one entry while there are actually two: the
        // trailing size passes the field's value.
        write_inode(&mut image, 1, &dir_inode(2, 64, FIRST_ZONE));
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "size not updated of directory / (ino = 1)"
        )));
    }

    #[test]
    fn indirect_zone_walk_clean_and_tampered() {
        // The clean face: single-indirect zone 7 points at data zone 6, and both
        // the expected map and the counts agree.
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &file_inode(1, 100, &[0, 0, 0, 0, 0, 0, 0, 7]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, ZMAP_BLOCK, 2); // data zone 6
        set_map_bit(&mut image, ZMAP_BLOCK, 3); // indirect zone 7
        let indirect_at = (7 * 4096) as usize;
        image[indirect_at..indirect_at + 4].copy_from_slice(&6u32.to_le_bytes());
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert_eq!(summary.errors, 0);
        assert_eq!(summary.ztype, [2, 1, 0, 0], "根数据区与文件数据区两级 0，间区块一级 1");
        assert_eq!(summary.nfreezone, 56, "58 - 数据区 6 - 间区块 7");
        // The tampered face: the indirect zone points at 999 (out of range, so the
        // recursive level-0 report fires).
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &file_inode(1, 4096, &[0, 0, 0, 0, 0, 0, 0, 7]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, ZMAP_BLOCK, 3);
        let indirect_at = (7 * 4096) as usize;
        image[indirect_at..indirect_at + 4].copy_from_slice(&999u32.to_le_bytes());
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            // The first seven direct slots are holes: pos has already jumped a
            // whole zone per hole, to 28672.
            "out-of-range zone in /f (ino = 2, zno = 999, type = DATA, pos = 28672)"
        )));
    }

    #[test]
    fn root_not_directory_is_fatal() {
        let (mut image, _plan) = base_image();
        let mut root = dir_inode(2, 128, FIRST_ZONE);
        root.mode = (crate::inode::TYPE_REGULAR | 0o644) as u16;
        write_inode(&mut image, 1, &root);
        let mut fsck = Fsck::new(&image).unwrap();
        let fatal = match fsck.run(&FsckOptions::default()) {
            Err(fatal) => fatal,
            Ok(_) => panic!("expected fatal"),
        };
        assert_eq!(
            fatal.message,
            "root inode is not a directory (ino = 1, mode = 100644)"
        );
    }

    #[test]
    fn orphan_allocated_inode_flagged_by_maps_and_ilist() {
        // A bitmap bit with no mode and no entry: the map reconciliation reports
        // missing, while the free-list check does not (a zero mode is already the
        // cleared state).
        let (mut image, _plan) = base_image();
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck
            .messages()
            .contains(&String::from("inode 2 is missing")));
        assert!(!fsck
            .messages()
            .iter()
            .any(|m| m.contains("not cleared")));
        // Now add a nonzero mode: the free-list check reports not cleared too.
        let (mut image, _plan) = base_image();
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        write_inode(&mut image, 2, &file_inode(1, 0, &[]));
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck
            .messages()
            .contains(&String::from("mode inode 2 not cleared")));
    }
}
