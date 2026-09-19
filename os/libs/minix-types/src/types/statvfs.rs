//! `struct statvfs` 的 LP64 wire 布局（C `sys/sys/statvfs.h:66-99`）。
//!
//! 这个结构体与其它 `*Snap` 类型不同：它不是"Rust 定义的已用字段子集快照"
//! （A-4 裁决），而是**FS 直接经 grant 回填的 C 结构体**——`req_statvfs`
//! （request.c:232-247）把 VFS 侧缓冲的地址做成 direct grant 交给 FS，FS 按
//! C 布局整块写进去，VFS 再补几个本地字段后整块拷给用户。所以这里的偏移必须
//! 与 C 编译出的布局**逐字节一致**，字段顺序与类型都照抄。
//!
//! 计算依据（x86-64 LP64）：`unsigned long` 8、`__uint64_t` 8（`fsblkcnt_t`/
//! `fsfilcnt_t` 见 `sys/sys/ansi.h:47-48`）、`fsid_t` = `struct fsid { int32_t
//! __fsid_val[2]; }` 8、`uid_t` 4、`uint32_t f_spare[4]` 16、
//! `char f_*name[_VFS_NAMELEN]` 各 32（`statvfs.h:42`）。字段总长 268，结构体
//! 对齐 8 → `sizeof = 272`。
extern crate alloc;


/// `_VFS_NAMELEN` (`statvfs.h:42`)：三个名字字段各占这么多字节（含结尾 NUL）。
pub const VFS_NAMELEN: usize = 32;

/// `sizeof(struct statvfs)`——grant 的长度与拷给用户的长度都用它。
pub const STATVFS_SIZE: usize = 272;

/// `struct statvfs` 各字段的字节偏移（LP64）。
///
/// C: `sys/sys/statvfs.h:66-99`。**顺序即布局**：16 个计数字段之后是
/// `f_fsidx`/`f_fsid`/`f_namemax`/`f_owner`/`f_spare[4]`，最后是三个名字。
pub mod statvfs_off {
    /// `unsigned long f_flag`。
    pub const FLAG: usize = 0;
    /// `unsigned long f_bsize`。
    pub const BSIZE: usize = 8;
    /// `unsigned long f_frsize`。
    pub const FRSIZE: usize = 16;
    /// `unsigned long f_iosize`。
    pub const IOSIZE: usize = 24;
    /// `fsblkcnt_t f_blocks`。
    pub const BLOCKS: usize = 32;
    /// `fsblkcnt_t f_bfree`。
    pub const BFREE: usize = 40;
    /// `fsblkcnt_t f_bavail`。
    pub const BAVAIL: usize = 48;
    /// `fsblkcnt_t f_bresvd`。
    pub const BRESVD: usize = 56;
    /// `fsfilcnt_t f_files`。
    pub const FILES: usize = 64;
    /// `fsfilcnt_t f_ffree`。
    pub const FFREE: usize = 72;
    /// `fsfilcnt_t f_favail`。
    pub const FFAVAIL: usize = 80;
    /// `fsfilcnt_t f_fresvd`。
    pub const FRESVD: usize = 88;
    /// `uint64_t f_syncreads`。
    pub const SYNCREADS: usize = 96;
    /// `uint64_t f_syncwrites`。
    pub const SYNCWRITES: usize = 104;
    /// `uint64_t f_asyncreads`。
    pub const ASYNCREADS: usize = 112;
    /// `uint64_t f_asyncwrites`。
    pub const ASYNCWRITES: usize = 120;
    /// `fsid_t f_fsidx`（`int32_t __fsid_val[2]`，8 字节）。
    pub const FSIDX: usize = 128;
    /// `unsigned long f_fsid`。
    pub const FSID: usize = 136;
    /// `unsigned long f_namemax`。
    pub const NAMEMAX: usize = 144;
    /// `uid_t f_owner`（4 字节）。
    pub const OWNER: usize = 152;
    /// `uint32_t f_spare[4]`（16 字节）。
    pub const SPARE: usize = 156;
    /// `char f_fstypename[_VFS_NAMELEN]`。
    pub const FSTYPENAME: usize = 172;
    /// `char f_mntonname[_VFS_NAMELEN]`。
    pub const MNTONNAME: usize = 204;
    /// `char f_mntfromname[_VFS_NAMELEN]`。
    pub const MNTFROMNAME: usize = 236;
}

/// `ST_RDONLY` (`statvfs.h:108` = `MNT_RDONLY` `fstypes.h:88` `0x00000001`)——
/// 只读挂载位，`fill_statvfs` 按 `VMNT_READONLY` 或进 `f_flag`。
pub const ST_RDONLY: u64 = 0x00000001;

/// `ST_NOWAIT` (`statvfs.h:139` = `MNT_NOWAIT` `fstypes.h:283` `2`)——只读缓存、
/// 不向 FS 要新数据。
pub const ST_NOWAIT: i32 = 2;

/// VFS 侧的 `struct statvfs` 缓冲（FS 经 grant 整块回填，VFS 再补本地字段）。
///
/// 用字节数组而不是 `#[repr(C)]` 结构体：偏移表是**唯一权威**，字段读写都
/// 过它；这样 C 侧布局一旦对不上，偏移表的 pin 测试与读写点会同时暴露，而不是
/// 藏在编译器的隐式填充里。
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
    /// 全零缓冲（`fill_statvfs` 的 `ST_NOWAIT` 分支就是 `memset(&buf, 0, ...)`
    /// 之后逐字段填缓存）。
    pub fn new() -> Self {
        Self {
            bytes: [0u8; STATVFS_SIZE],
        }
    }

    /// 只读视图（拷给用户时按整块用）。
    pub fn as_bytes(&self) -> &[u8; STATVFS_SIZE] {
        &self.bytes
    }

    /// 读一个 8 字节字段。
    pub fn get_u64(&self, off: usize) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.bytes[off..off + 8]);
        u64::from_le_bytes(b)
    }

    /// 写一个 8 字节字段。
    pub fn set_u64(&mut self, off: usize, v: u64) {
        self.bytes[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }

    /// 读一个 4 字节字段。
    pub fn get_u32(&self, off: usize) -> u32 {
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.bytes[off..off + 4]);
        u32::from_le_bytes(b)
    }

    /// 写一个 4 字节字段。
    pub fn set_u32(&mut self, off: usize, v: u32) {
        self.bytes[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// 写一个名字字段（`strlcpy` 语义：截断到 `VFS_NAMELEN - 1` 并补 NUL）。
    pub fn set_name(&mut self, off: usize, name: &str) {
        let bytes = name.as_bytes();
        let n = bytes.len().min(VFS_NAMELEN - 1);
        self.bytes[off..off + n].copy_from_slice(&bytes[..n]);
        for b in &mut self.bytes[off + n..off + VFS_NAMELEN] {
            *b = 0;
        }
    }

    /// 读一个名字字段（到第一个 NUL 为止，按 UTF-8 有损解码）。
    pub fn get_name(&self, off: usize) -> alloc::string::String {
        let raw = &self.bytes[off..off + VFS_NAMELEN];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        alloc::string::String::from_utf8_lossy(&raw[..end]).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 偏移表的 pin：字段序照 `sys/sys/statvfs.h:66-99` 逐条核（16 个 8 字节
    /// 计数 + fsidx/fsid/namemax/owner/spare + 三个 32 字节名字）。
    /// 结构体总长 268、按 8 对齐 → 272。
    #[test]
    fn test_statvfs_lp64_layout_matches_c() {
        assert_eq!(VFS_NAMELEN, 32);
        // 前 16 个字段：四个一组、每组 32 字节。
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
        // 尾部：fsid 系 8 字节对齐、owner 4 字节、spare 16 字节。
        assert_eq!(statvfs_off::FSIDX, 128);
        assert_eq!(statvfs_off::FSID, 136);
        assert_eq!(statvfs_off::NAMEMAX, 144);
        assert_eq!(statvfs_off::OWNER, 152);
        assert_eq!(statvfs_off::SPARE, 156);
        // 三个名字：172 起，各 32 字节，末字段结束于 268。
        assert_eq!(statvfs_off::FSTYPENAME, 172);
        assert_eq!(statvfs_off::MNTONNAME, 204);
        assert_eq!(statvfs_off::MNTFROMNAME, 236);
        assert_eq!(statvfs_off::MNTFROMNAME + VFS_NAMELEN, 268);
        // 结构体对齐 8 → 272。
        assert_eq!(STATVFS_SIZE, 272);
    }

    /// 读写走偏移表；名字按 `strlcpy` 截断（超长截到 31 字节 + NUL）。
    #[test]
    fn test_statvfs_buf_accessors() {
        let mut b = StatvfsBuf::new();
        b.set_u64(statvfs_off::BLOCKS, 0x1234);
        b.set_u64(statvfs_off::NAMEMAX, 60);
        b.set_u32(statvfs_off::OWNER, 1000);
        assert_eq!(b.get_u64(statvfs_off::BLOCKS), 0x1234);
        assert_eq!(b.get_u64(statvfs_off::NAMEMAX), 60);
        assert_eq!(b.get_u32(statvfs_off::OWNER), 1000);
        // 名字：正常写入与读回。
        b.set_name(statvfs_off::FSTYPENAME, "mfs");
        assert_eq!(b.get_name(statvfs_off::FSTYPENAME), "mfs");
        // 超长：截到 31 字节（`strlcpy` 语义），尾部是 NUL。
        let long = "x".repeat(64);
        b.set_name(statvfs_off::MNTONNAME, &long);
        assert_eq!(b.get_name(statvfs_off::MNTONNAME).len(), VFS_NAMELEN - 1);
        assert_eq!(
            b.as_bytes()[statvfs_off::MNTONNAME + VFS_NAMELEN - 1],
            0,
            "末尾必须是 NUL"
        );
        // 覆盖写要清掉旧尾巴（不是只覆盖前几个字节）。
        b.set_name(statvfs_off::MNTONNAME, "s");
        assert_eq!(b.get_name(statvfs_off::MNTONNAME), "s");
        assert_eq!(b.as_bytes()[statvfs_off::MNTONNAME + 1], 0);
    }
}
