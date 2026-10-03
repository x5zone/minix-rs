//! User-side file status layout (C: `struct stat`).
//!
//! The C definition lives in `minix3/sys/sys/stat.h:59-97` (NetBSD style,
//! four `timespec` timestamps). Field widths come from the C tree's fixed
//! typedefs — `dev_t`/`ino_t` are 64-bit (`sys/sys/types.h:187/:197`),
//! `mode_t`/`uid_t`/`gid_t`/`nlink_t` are 32-bit (`sys/sys/ansi.h:41/:46/:38`,
//! `types.h:205`), `off_t`/`blkcnt_t`/`time_t` are 64-bit
//! (`ansi.h:42`, `types.h:159`, i386 `ansi.h:54`) — and none of them is a
//! pointer, so the layout is identical for i386 and LP64: 148 bytes of
//! fields, tail-padded to 152 for 8-byte alignment.
//!
//! This is the buffer the VFS stat family asks the file-system server to
//! fill through a magic grant (`stadir.c:140-176` routes `req_stat` with
//! `sizeof(struct stat)`); the kernel never inspects individual fields, so
//! byte-for-byte fidelity is the whole contract.

/// One file timestamp. C: `struct timespec` (`sys/sys/timespec.h`) —
/// whole seconds plus a nanosecond fraction, both 64-bit here.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatTimeSpec {
    /// Whole seconds. C: `tv_sec`.
    pub tv_sec: i64,
    /// Nanosecond fraction. C: `tv_nsec`.
    pub tv_nsec: i64,
}

/// File status, byte-compatible with C's `struct stat`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    /// Device hosting the file. C: `st_dev`.
    pub st_dev: u64,
    /// File type and permission bits. C: `st_mode` (`mode_t`).
    pub st_mode: u32,
    _padding_after_mode: [u8; 4],
    /// File serial number. C: `st_ino`.
    pub st_ino: u64,
    /// Hard link count. C: `st_nlink`.
    pub st_nlink: u32,
    /// Owner user id. C: `st_uid`.
    pub st_uid: u32,
    /// Owner group id. C: `st_gid`.
    pub st_gid: u32,
    _padding_after_gid: [u8; 4],
    /// Device id for device files. C: `st_rdev`.
    pub st_rdev: u64,
    /// Last access. C: `st_atim` (the `st_atime` spelling is the same
    /// storage; the C header provides both names).
    pub st_atim: StatTimeSpec,
    /// Last data modification. C: `st_mtim`.
    pub st_mtim: StatTimeSpec,
    /// Last status change. C: `st_ctim`.
    pub st_ctim: StatTimeSpec,
    /// Creation time. C: `st_birthtim`.
    pub st_birthtim: StatTimeSpec,
    /// File size in bytes. C: `st_size` (`off_t`).
    pub st_size: i64,
    /// Blocks of 512 bytes actually stored. C: `st_blocks`.
    pub st_blocks: i64,
    /// Preferred I/O block size. C: `st_blksize`.
    pub st_blksize: i32,
    /// User-settable flags. C: `st_flags`.
    pub st_flags: u32,
    /// File generation number. C: `st_gen`.
    pub st_gen: u32,
    /// Reserved for future use. C: `st_spare[2]`.
    pub st_spare: [u32; 2],
}

#[cfg(test)]
mod tests {
    use super::Stat;
    use core::mem::{offset_of, size_of};

    /// The whole point of this type: byte-for-byte agreement with the C
    /// layout the file-system server writes. Field offsets follow from the
    /// C widths (no pointer fields, so i386 and LP64 agree).
    #[test]
    fn test_stat_layout_matches_c_struct() {
        assert_eq!(size_of::<Stat>(), 152);
        assert_eq!(offset_of!(Stat, st_dev), 0);
        assert_eq!(offset_of!(Stat, st_mode), 8);
        assert_eq!(offset_of!(Stat, st_ino), 16);
        assert_eq!(offset_of!(Stat, st_nlink), 24);
        assert_eq!(offset_of!(Stat, st_uid), 28);
        assert_eq!(offset_of!(Stat, st_gid), 32);
        assert_eq!(offset_of!(Stat, st_rdev), 40);
        assert_eq!(offset_of!(Stat, st_atim), 48);
        assert_eq!(offset_of!(Stat, st_mtim), 64);
        assert_eq!(offset_of!(Stat, st_ctim), 80);
        assert_eq!(offset_of!(Stat, st_birthtim), 96);
        assert_eq!(offset_of!(Stat, st_size), 112);
        assert_eq!(offset_of!(Stat, st_blocks), 120);
        assert_eq!(offset_of!(Stat, st_blksize), 128);
        assert_eq!(offset_of!(Stat, st_flags), 132);
        assert_eq!(offset_of!(Stat, st_gen), 136);
        assert_eq!(offset_of!(Stat, st_spare), 140);
    }

    /// NK4-C 续-277c（CodeReview P1-1）：目标③ C 桥（tools/atf-c-compat/
    /// posix-stubs.c 的 lstat 翻译层）把本布局的字节偏移手抄成了
    /// `FS_ST_*` 宏——本测试读那份 C 源逐条对账：任何一侧改动（字段序/
    /// 宽度/宏值）都会让这里变红，而不是让 C 侧静默读垃圾。
    /// 路径从 crate 位置推导（os/libs/minix-types 上溯三级 = 仓根），仅宿主
    /// 测试跑（#[cfg(test)] 天然不进生产构建）。
    #[test]
    fn test_c_bridge_fs_stat_offsets_match_rust_layout() {
        use std::format;   // no_std 预lude 不带 format!，宿主测试显式导入
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../tools/atf-c-compat/posix-stubs.c");
        let src = std::fs::read_to_string(path).expect("posix-stubs.c readable from crate dir");
        let want: &[(&str, usize)] = &[
            ("FS_ST_DEV", offset_of!(Stat, st_dev)),
            ("FS_ST_MODE", offset_of!(Stat, st_mode)),
            ("FS_ST_INO", offset_of!(Stat, st_ino)),
            ("FS_ST_NLINK", offset_of!(Stat, st_nlink)),
            ("FS_ST_UID", offset_of!(Stat, st_uid)),
            ("FS_ST_GID", offset_of!(Stat, st_gid)),
            ("FS_ST_RDEV", offset_of!(Stat, st_rdev)),
            ("FS_ST_ATIM", offset_of!(Stat, st_atim)),
            ("FS_ST_MTIM", offset_of!(Stat, st_mtim)),
            ("FS_ST_CTIM", offset_of!(Stat, st_ctim)),
            ("FS_ST_SIZE", offset_of!(Stat, st_size)),
            ("FS_ST_BLOCKS", offset_of!(Stat, st_blocks)),
            ("FS_ST_BLKSIZE", offset_of!(Stat, st_blksize)),
            ("FS_STAT_SIZE", size_of::<Stat>()),
        ];
        for (name, offset) in want {
            let needle = format!("#define {name}");
            let line = src
                .lines()
                .find(|l| l.trim_start().starts_with(needle.as_str()))
                .unwrap_or_else(|| panic!("{name} define missing in posix-stubs.c"));
            let value: usize = line
                .split_whitespace()
                .nth(2)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| panic!("{name} value unparsable: {line}"));
            assert_eq!(value, *offset, "{name} drifts from Rust Stat layout");
        }
    }
}
