//! LP64 wire layout of `struct statvfs` (C `sys/sys/statvfs.h:66-99`).
//!
//! This struct differs from the other `*Snap` types: it is not a Rust-defined
//! snapshot of the in-use field subset but **the C struct that the FS fills
//! back through a grant** — `req_statvfs` (request.c:232-247) makes the address
//! of VFS's buffer a direct grant and hands it to the FS, which writes the whole
//! block in C layout; VFS then patches a few local fields and copies the block
//! out to the user. The offsets here must therefore be **byte-for-byte**
//! identical to the C-compiled layout, and field order and types are copied
//! verbatim.
//!
//! Basis of the arithmetic (x86-64 LP64): `unsigned long` 8, `__uint64_t` 8
//! (`fsblkcnt_t`/`fsfilcnt_t` see `sys/sys/ansi.h:47-48`), `fsid_t` = `struct
//! fsid { int32_t __fsid_val[2]; }` 8, `uid_t` 4, `uint32_t f_spare[4]` 16,
//! `char f_*name[_VFS_NAMELEN]` 32 each (`statvfs.h:42`). Total field length
//! 268, struct alignment 8 → `sizeof = 272`.
extern crate alloc;


/// `_VFS_NAMELEN` (`statvfs.h:42`): bytes occupied by each of the three name
/// fields (including the trailing NUL).
pub const VFS_NAMELEN: usize = 32;

/// `sizeof(struct statvfs)` — used both as the grant length and as the length
/// copied out to the user.
pub const STATVFS_SIZE: usize = 272;

/// Byte offsets of the `struct statvfs` fields (LP64).
///
/// C: `sys/sys/statvfs.h:66-99`. **Order is layout**: after the 16 counter
/// fields come `f_fsidx`/`f_fsid`/`f_namemax`/`f_owner`/`f_spare[4]`, and last
/// the three names.
pub mod statvfs_off {
    /// `unsigned long f_flag`.
    pub const FLAG: usize = 0;
    /// `unsigned long f_bsize`.
    pub const BSIZE: usize = 8;
    /// `unsigned long f_frsize`.
    pub const FRSIZE: usize = 16;
    /// `unsigned long f_iosize`.
    pub const IOSIZE: usize = 24;
    /// `fsblkcnt_t f_blocks`.
    pub const BLOCKS: usize = 32;
    /// `fsblkcnt_t f_bfree`.
    pub const BFREE: usize = 40;
    /// `fsblkcnt_t f_bavail`.
    pub const BAVAIL: usize = 48;
    /// `fsblkcnt_t f_bresvd`.
    pub const BRESVD: usize = 56;
    /// `fsfilcnt_t f_files`.
    pub const FILES: usize = 64;
    /// `fsfilcnt_t f_ffree`.
    pub const FFREE: usize = 72;
    /// `fsfilcnt_t f_favail`.
    pub const FFAVAIL: usize = 80;
    /// `fsfilcnt_t f_fresvd`.
    pub const FRESVD: usize = 88;
    /// `uint64_t f_syncreads`.
    pub const SYNCREADS: usize = 96;
    /// `uint64_t f_syncwrites`.
    pub const SYNCWRITES: usize = 104;
    /// `uint64_t f_asyncreads`.
    pub const ASYNCREADS: usize = 112;
    /// `uint64_t f_asyncwrites`.
    pub const ASYNCWRITES: usize = 120;
    /// `fsid_t f_fsidx` (`int32_t __fsid_val[2]`, 8 bytes).
    pub const FSIDX: usize = 128;
    /// `unsigned long f_fsid`.
    pub const FSID: usize = 136;
    /// `unsigned long f_namemax`.
    pub const NAMEMAX: usize = 144;
    /// `uid_t f_owner` (4 bytes).
    pub const OWNER: usize = 152;
    /// `uint32_t f_spare[4]` (16 bytes).
    pub const SPARE: usize = 156;
    /// `char f_fstypename[_VFS_NAMELEN]`.
    pub const FSTYPENAME: usize = 172;
    /// `char f_mntonname[_VFS_NAMELEN]`.
    pub const MNTONNAME: usize = 204;
    /// `char f_mntfromname[_VFS_NAMELEN]`.
    pub const MNTFROMNAME: usize = 236;
}

/// `ST_RDONLY` (`statvfs.h:108` = `MNT_RDONLY` `fstypes.h:88` `0x00000001`) —
/// the read-only mount bit; `fill_statvfs` ORs `VMNT_READONLY` into `f_flag`.
pub const ST_RDONLY: u64 = 0x00000001;

/// `ST_NOWAIT` (`statvfs.h:139` = `MNT_NOWAIT` `fstypes.h:283` `2`) — read the
/// cache only, do not ask the FS for fresh data.
pub const ST_NOWAIT: i32 = 2;

/// `ST_WAIT` (`statvfs.h:138` = `MNT_WAIT` `fstypes.h:282` `1`) — ask the FS
/// synchronously for fresh data. C's `statvfs()` calls `statvfs1` with
/// `ST_WAIT`.
pub const ST_WAIT: i32 = 1;

/// VFS's `struct statvfs` buffer (the FS fills it back whole through a grant,
/// then VFS patches the local fields).
///
/// A byte array rather than a `#[repr(C)]` struct: the offset table is the
/// **single authority** and every field read/write goes through it, so if the C
/// layout ever disagrees, the offset table's pin test and the access sites both
/// fail visibly instead of hiding in the compiler's implicit padding.
#[derive(Clone, Copy)]
pub struct StatvfsBuf {
    bytes: [u8; STATVFS_SIZE],
}

impl Default for StatvfsBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl StatvfsBuf {
    /// All-zero buffer (`fill_statvfs`'s `ST_NOWAIT` branch is exactly
    /// `memset(&buf, 0, ...)` followed by filling the cached fields one by
    /// one).
    pub fn new() -> Self {
        Self {
            bytes: [0u8; STATVFS_SIZE],
        }
    }

    /// Read-only view (used as a whole block when copying out to the user).
    pub fn as_bytes(&self) -> &[u8; STATVFS_SIZE] {
        &self.bytes
    }

    /// Read an 8-byte field.
    pub fn get_u64(&self, off: usize) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.bytes[off..off + 8]);
        u64::from_le_bytes(b)
    }

    /// Write an 8-byte field.
    pub fn set_u64(&mut self, off: usize, v: u64) {
        self.bytes[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }

    /// Read a 4-byte field.
    pub fn get_u32(&self, off: usize) -> u32 {
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.bytes[off..off + 4]);
        u32::from_le_bytes(b)
    }

    /// Write a 4-byte field.
    pub fn set_u32(&mut self, off: usize, v: u32) {
        self.bytes[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// Write a name field (`strlcpy` semantics: truncate to `VFS_NAMELEN - 1`
    /// and NUL-terminate).
    pub fn set_name(&mut self, off: usize, name: &str) {
        let bytes = name.as_bytes();
        let n = bytes.len().min(VFS_NAMELEN - 1);
        self.bytes[off..off + n].copy_from_slice(&bytes[..n]);
        for b in &mut self.bytes[off + n..off + VFS_NAMELEN] {
            *b = 0;
        }
    }

    /// Read a name field (up to the first NUL, decoded lossily as UTF-8).
    pub fn get_name(&self, off: usize) -> alloc::string::String {
        let raw = &self.bytes[off..off + VFS_NAMELEN];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        alloc::string::String::from_utf8_lossy(&raw[..end]).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin of the offset table: field order checked one by one against
    /// `sys/sys/statvfs.h:66-99` (16 8-byte counters +
    /// fsidx/fsid/namemax/owner/spare + three 32-byte names).
    /// Struct total length 268, aligned to 8 → 272.
    #[test]
    fn test_statvfs_lp64_layout_matches_c() {
        assert_eq!(VFS_NAMELEN, 32);
        // First 16 fields: four per group, 32 bytes per group.
        assert_eq!(statvfs_off::FLAG, 0);
        assert_eq!(statvfs_off::BSIZE, 8);
        assert_eq!(statvfs_off::FRSIZE, 16);
        assert_eq!(statvfs_off::IOSIZE, 24);
        assert_eq!(statvfs_off::BLOCKS, 32);
        assert_eq!(statvfs_off::BFREE, 40);
        assert_eq!(statvfs_off::BAVAIL, 48);
        assert_eq!(statvfs_off::BRESVD, 56);
        assert_eq!(statvfs_off::FILES, 64);
        assert_eq!(statvfs_off::FFREE, 72);
        assert_eq!(statvfs_off::FFAVAIL, 80);
        assert_eq!(statvfs_off::FRESVD, 88);
        assert_eq!(statvfs_off::SYNCREADS, 96);
        assert_eq!(statvfs_off::SYNCWRITES, 104);
        assert_eq!(statvfs_off::ASYNCREADS, 112);
        assert_eq!(statvfs_off::ASYNCWRITES, 120);
        // Tail: the fsid group is 8 bytes, owner 4 bytes, spare 16 bytes.
        assert_eq!(statvfs_off::FSIDX, 128);
        assert_eq!(statvfs_off::FSID, 136);
        assert_eq!(statvfs_off::NAMEMAX, 144);
        assert_eq!(statvfs_off::OWNER, 152);
        assert_eq!(statvfs_off::SPARE, 156);
        // Three names: from 172, 32 bytes each, the last field ends at 268.
        assert_eq!(statvfs_off::FSTYPENAME, 172);
        assert_eq!(statvfs_off::MNTONNAME, 204);
        assert_eq!(statvfs_off::MNTFROMNAME, 236);
        assert_eq!(statvfs_off::MNTFROMNAME + VFS_NAMELEN, 268);
        // Struct alignment 8 → 272.
        assert_eq!(STATVFS_SIZE, 272);
    }

    /// Reads and writes go through the offset table; names are truncated
    /// `strlcpy`-style (over-long input truncates to 31 bytes + NUL).
    #[test]
    fn test_statvfs_buf_accessors() {
        let mut b = StatvfsBuf::new();
        b.set_u64(statvfs_off::BLOCKS, 0x1234);
        b.set_u64(statvfs_off::NAMEMAX, 60);
        b.set_u32(statvfs_off::OWNER, 1000);
        assert_eq!(b.get_u64(statvfs_off::BLOCKS), 0x1234);
        assert_eq!(b.get_u64(statvfs_off::NAMEMAX), 60);
        assert_eq!(b.get_u32(statvfs_off::OWNER), 1000);
        // Name: normal write and read back.
        b.set_name(statvfs_off::FSTYPENAME, "mfs");
        assert_eq!(b.get_name(statvfs_off::FSTYPENAME), "mfs");
        // Over-long: truncated to 31 bytes (`strlcpy` semantics), tail is NUL.
        let long = "x".repeat(64);
        b.set_name(statvfs_off::MNTONNAME, &long);
        assert_eq!(b.get_name(statvfs_off::MNTONNAME).len(), VFS_NAMELEN - 1);
        assert_eq!(
            b.as_bytes()[statvfs_off::MNTONNAME + VFS_NAMELEN - 1],
            0,
            "末尾必须是 NUL"
        );
        // Overwriting must clear the old tail (not just overwrite the first bytes).
        b.set_name(statvfs_off::MNTONNAME, "s");
        assert_eq!(b.get_name(statvfs_off::MNTONNAME), "s");
        assert_eq!(b.as_bytes()[statvfs_off::MNTONNAME + 1], 0);
    }
}
