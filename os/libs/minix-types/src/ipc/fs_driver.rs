//! VFS→FS driver request numbers — the single Rust authority for C's
//! `REQ_*` family (`minix3/minix/include/minix/vfsif.h:41-73`, base
//! `FS_BASE` at `com.h:589`).
//!
//! Consumers: VFS builds requests with these numbers (`servers/vfs/
//! src/request.rs`), every file-server driver dispatches on them
//! (`minix-fs` `protocol.rs`, consumed by mfs/pfs), and devman's inline
//! VTreeFS is the registered third consumer (edge E-DMWIRE/E-REQWIRE).
//! One authority here means the sides cannot drift — the pre-convergence
//! incident had VFS at base `0x600` against the FS side's `0xA00`, with
//! offset alignment hiding the split until a wire break (05-stage-vfs
//! R2-P0-1).
//!
//! Domain note: the canonical type is `i32`, matching `Message::m_type`.
//! `IS_FS_RQ` (`vfsif.h:77`) and C's driver-side decoders bit-mask the
//! raw `int`; the bit pattern of `0xA00..=0xAFF` is identical in both
//! signednesses for the values in use.

/// Base of the request-number space ("Requests sent by VFS to filesystem").
/// C: `FS_BASE` — com.h:589.
pub const FS_BASE: i32 = 0xA00;

/// Dispatch-table slot count, including the unused slot zero.
/// C: `NREQS` — vfsif.h:75 (includes dead `GETNODE`).
pub const NREQS: usize = 34;

/// C: `REQ_GETNODE (FS_BASE + 1)` — vfsif.h:42. Marked "Should be
/// removed" upstream: no dispatch-table entry answers it. The number is
/// pinned so the dead slot stays named, not forgotten.
pub const REQ_GETNODE: i32 = FS_BASE + 1;

/// C: `REQ_PUTNODE (FS_BASE + 2)` — vfsif.h:43.
pub const REQ_PUTNODE: i32 = FS_BASE + 2;

/// C: `REQ_SLINK (FS_BASE + 3)` — vfsif.h:44.
pub const REQ_SLINK: i32 = FS_BASE + 3;

/// C: `REQ_FTRUNC (FS_BASE + 4)` — vfsif.h:45.
pub const REQ_FTRUNC: i32 = FS_BASE + 4;

/// C: `REQ_CHOWN (FS_BASE + 5)` — vfsif.h:46.
pub const REQ_CHOWN: i32 = FS_BASE + 5;

/// C: `REQ_CHMOD (FS_BASE + 6)` — vfsif.h:47.
pub const REQ_CHMOD: i32 = FS_BASE + 6;

/// C: `REQ_INHIBREAD (FS_BASE + 7)` — vfsif.h:48.
pub const REQ_INHIBREAD: i32 = FS_BASE + 7;

/// C: `REQ_STAT (FS_BASE + 8)` — vfsif.h:49.
pub const REQ_STAT: i32 = FS_BASE + 8;

/// C: `REQ_UTIME (FS_BASE + 9)` — vfsif.h:50.
pub const REQ_UTIME: i32 = FS_BASE + 9;

/// C: `REQ_STATVFS (FS_BASE + 10)` — vfsif.h:51.
pub const REQ_STATVFS: i32 = FS_BASE + 10;

/// C: `REQ_BREAD (FS_BASE + 11)` — vfsif.h:52.
pub const REQ_BREAD: i32 = FS_BASE + 11;

/// C: `REQ_BWRITE (FS_BASE + 12)` — vfsif.h:53.
pub const REQ_BWRITE: i32 = FS_BASE + 12;

/// C: `REQ_UNLINK (FS_BASE + 13)` — vfsif.h:54.
pub const REQ_UNLINK: i32 = FS_BASE + 13;

/// C: `REQ_RMDIR (FS_BASE + 14)` — vfsif.h:55.
pub const REQ_RMDIR: i32 = FS_BASE + 14;

/// C: `REQ_UNMOUNT (FS_BASE + 15)` — vfsif.h:56.
pub const REQ_UNMOUNT: i32 = FS_BASE + 15;

/// C: `REQ_SYNC (FS_BASE + 16)` — vfsif.h:57.
pub const REQ_SYNC: i32 = FS_BASE + 16;

/// C: `REQ_NEW_DRIVER (FS_BASE + 17)` — vfsif.h:58.
pub const REQ_NEW_DRIVER: i32 = FS_BASE + 17;

/// C: `REQ_FLUSH (FS_BASE + 18)` — vfsif.h:59.
pub const REQ_FLUSH: i32 = FS_BASE + 18;

/// C: `REQ_READ (FS_BASE + 19)` — vfsif.h:60.
pub const REQ_READ: i32 = FS_BASE + 19;

/// C: `REQ_WRITE (FS_BASE + 20)` — vfsif.h:61.
pub const REQ_WRITE: i32 = FS_BASE + 20;

/// C: `REQ_MKNOD (FS_BASE + 21)` — vfsif.h:62.
pub const REQ_MKNOD: i32 = FS_BASE + 21;

/// C: `REQ_MKDIR (FS_BASE + 22)` — vfsif.h:63.
pub const REQ_MKDIR: i32 = FS_BASE + 22;

/// C: `REQ_CREATE (FS_BASE + 23)` — vfsif.h:64.
pub const REQ_CREATE: i32 = FS_BASE + 23;

/// C: `REQ_LINK (FS_BASE + 24)` — vfsif.h:65.
pub const REQ_LINK: i32 = FS_BASE + 24;

/// C: `REQ_RENAME (FS_BASE + 25)` — vfsif.h:66.
pub const REQ_RENAME: i32 = FS_BASE + 25;

/// C: `REQ_LOOKUP (FS_BASE + 26)` — vfsif.h:67.
pub const REQ_LOOKUP: i32 = FS_BASE + 26;

/// C: `REQ_MOUNTPOINT (FS_BASE + 27)` — vfsif.h:68.
pub const REQ_MOUNTPOINT: i32 = FS_BASE + 27;

/// C: `REQ_READSUPER (FS_BASE + 28)` — vfsif.h:69.
pub const REQ_READSUPER: i32 = FS_BASE + 28;

/// C: `REQ_NEWNODE (FS_BASE + 29)` — vfsif.h:70.
pub const REQ_NEWNODE: i32 = FS_BASE + 29;

/// C: `REQ_RDLINK (FS_BASE + 30)` — vfsif.h:71.
pub const REQ_RDLINK: i32 = FS_BASE + 30;

/// C: `REQ_GETDENTS (FS_BASE + 31)` — vfsif.h:72.
pub const REQ_GETDENTS: i32 = FS_BASE + 31;

/// C: `REQ_PEEK (FS_BASE + 32)` — vfsif.h:73.
pub const REQ_PEEK: i32 = FS_BASE + 32;

/// C: `REQ_BPEEK (FS_BASE + 33)` — vfsif.h:73 (shares the line with
/// `REQ_PEEK` upstream).
pub const REQ_BPEEK: i32 = FS_BASE + 33;

/// C: `IS_FS_RQ(type) ((type & ~0xff) == FS_BASE)` — vfsif.h:77.
///
/// 注意它判的是**已剥掉 transid 的调用号**：线上 `m_type` 是
/// `TRNS_ADD_ID(call_nr, transid)`（见下），直接拿线上值判会看到
/// `0xAxx << 16` 而不是 `0xAxx`。
pub const fn is_fs_rq(raw: i32) -> bool {
    (raw & !0xff) == FS_BASE
}

/// C: `TRNS_ADD_ID(t, id) (((t) << 16) | ((id) & 0xFFFF))` —
/// vfsif.h:80。把调用号与 transid 合成线上的 `m_type`：
/// VFS 发请求、FS 发回复都用这一个式子（回复时 `t` 是**结果值**）。
///
/// 与 `minix-fs::protocol::TransactionId` 的关系：后者是同一套 C 宏的
/// **类型化包装**（`decode`/`encode_reply`/`encode_request` 三方法），
/// FS 侧服务器按它写分派；VFS 侧的 `fs_comm::TransId` 另带 worker 槽
/// 语义（`VFS_TRANSID + slot`）。三处的算式以本模块为准（vfsif.h 的
/// 三行宏在此逐行落地），改一处须同步另两处的测试。
pub const fn trns_add_id(t: i32, id: u32) -> i32 {
    ((t as u32) << 16 | (id & 0xFFFF)) as i32
}

/// C: `TRNS_GET_ID(t) ((t) & 0xFFFF)` — vfsif.h:79（线上值的低 16 位
/// 即 transid；`VFS_TRANSID + worker_tid`，com.h:911）。
pub const fn trns_get_id(raw: i32) -> u32 {
    (raw as u32) & 0xFFFF
}

/// C: `TRNS_DEL_ID(t) ((short)((t) >> 16))` — vfsif.h:81。
///
/// 注意 `(short)` 截断是**有意的**：高 16 位装的是调用号（正值，如
/// `REQ_READ = 0xA13`）或回复的结果值（含负 errno），一律按 16 位有
/// 符号读回。
pub const fn trns_del_id(raw: i32) -> i32 {
    ((raw >> 16) as i16) as i32
}

/// `REQ_LOOKUP` 请求载荷的 LP64 域偏移（C 字段序 + 64 位重排）。
///
/// C: `mess_vfs_fs_lookup`（ipc.h:2011-2035）的字段序
/// `dir_ino / root_ino / flags / path_len / path_size / ucred_size /
/// grant_path / grant_ucred`；i386 下各占 4 字节，LP64 下指针与 `size_t`
/// 拉到 8 字节、`uint32_t` 保持 4——**两侧（VFS 编码器与 FS 解码器）必须
/// 用这张表**，否则名字会错位（E-REQWIRE 同族事故）。
pub mod lookup_req_off {
    /// `ino_t dir_ino`（lookup 起点）。
    pub const DIR_INO: usize = 0;
    /// `ino_t root_ino`（路径解析的根）。
    pub const ROOT_INO: usize = 8;
    /// `uint32_t flags`（`LookupFlags`，见 `minix-fs::protocol`）。
    pub const FLAGS: usize = 16;
    /// `size_t path_len`（含 NUL 的名字长度）。
    pub const PATH_LEN: usize = 24;
    /// `size_t path_size`（grant 窗口大小）。
    pub const PATH_SIZE: usize = 32;
    /// `size_t ucred_size`（凭证区大小，0 = 不带）。
    pub const UCRED_SIZE: usize = 40;
    /// `cp_grant_id_t grant_path`（名字所在 grant）。
    pub const GRANT_PATH: usize = 48;
    /// `cp_grant_id_t grant_ucred`（凭证所在 grant）。
    pub const GRANT_UCRED: usize = 56;
}

/// `REQ_READ` / `REQ_GETDENTS` 请求载荷的 LP64 域偏移（同前缀）。
///
/// C: `mess_vfs_fs_readwrite`（ipc.h:2121-2131）与
/// `mess_vfs_fs_getdents`（ipc.h:1992-2002）的字段序相同。
pub mod transfer_req_off {
    /// `ino_t inode`。
    pub const INODE: usize = 0;
    /// `off_t seek_pos`。
    pub const SEEK_POS: usize = 8;
    /// `cp_grant_id_t grant`（数据缓冲在调用方）。
    pub const GRANT: usize = 16;
    /// `size_t nbytes`（read 的字节数）/ `mem_size`（getdents 的窗口）。
    pub const BYTES: usize = 24;
}

/// C: `PATH_RET_SYMLINK (010)` — vfsif.h:12（`REQ_LOOKUP` 的 `flags` 位：
/// 最后一个组件是符号链接时**不要**解析它，把链接本身带回）。
///
/// 这个位的语义在 FS 侧实现（`libfsdriver/lookup.c:249-251`），VFS 只负责
/// 原样发过去。
pub const PATH_RET_SYMLINK: u32 = 0o10;

/// C: `PATH_GET_UCRED (020)` — vfsif.h:16（请求里带凭证 grant，FS 用它做
/// 权限判断）。Rust 侧凭证面还没接线，见 `encode_lookup` 的断言。
pub const PATH_GET_UCRED: u32 = 0o20;

/// C: `EENTERMOUNT (-301)` — vfsif.h:26（FS→VFS 的"进入挂载点"特殊码，
/// 走回复的 `m_type`，不是错误）。
pub const EENTERMOUNT: i32 = -301;
/// C: `ELEAVEMOUNT (-302)` — vfsif.h:27（"离开挂载点"）。
pub const ELEAVEMOUNT: i32 = -302;
/// C: `ESYMLINK (-303)` — vfsif.h:28（"这是符号链接"）。
pub const ESYMLINK: i32 = -303;

/// `REQ_LOOKUP` 的**回复**载荷 LP64 域偏移（FS→VFS 方向）。
///
/// C: `mess_fs_vfs_lookup`（ipc.h:163-176）字段序
/// `offset / file_size / device / inode / mode / uid / gid / symloop`。
/// 三个特殊码（[`EENTERMOUNT`]/[`ELEAVEMOUNT`]/[`ESYMLINK`]）时只有
/// `offset`（与 `inode`、`symloop`）有意义；`Ok` 时其余字段填 `node_details`。
pub mod lookup_reply_off {
    /// `off_t offset`（FS 已消费的字节数 / 剩余路径偏移）。
    pub const OFFSET: usize = 0;
    /// `off_t file_size`。
    pub const FILE_SIZE: usize = 8;
    /// `dev_t device`。
    pub const DEVICE: usize = 16;
    /// `ino_t inode`（8 对齐，故 20..24 是垫）。
    pub const INODE: usize = 24;
    /// `mode_t mode`。
    pub const MODE: usize = 32;
    /// `uid_t uid`。
    pub const UID: usize = 36;
    /// `gid_t gid`。
    pub const GID: usize = 40;
    /// `uint16_t symloop`。
    pub const SYMLOOP: usize = 44;
}

/// `REQ_FTRUNC` 请求载荷的 LP64 域偏移。
///
/// C: `mess_vfs_fs_ftrunc { ino_t inode; off_t trc_start; off_t trc_end; }`
/// （`request.c:269-272` 的 `req_ftrunc`）。语义：`trc_end == 0` 表示
/// "截到 `trc_start`"（mfs `fs_trunc:439-443`），非零则释放 `[start,end)`。
pub mod ftrunc_req_off {
    /// `ino_t inode`。
    pub const INODE: usize = 0;
    /// `off_t trc_start`。
    pub const TRC_START: usize = 8;
    /// `off_t trc_end`。
    pub const TRC_END: usize = 16;
}

/// `REQ_CREATE` 请求载荷的 LP64 域偏移。
///
/// C: `mess_vfs_fs_create { ino_t inode; mode_t mode; uid_t uid; gid_t gid;
/// cp_grant_id_t grant; size_t path_len; }`（`request.c:189-196` 的
/// `req_create`；`grant` 指向**最后组件名**）。
pub mod create_req_off {
    /// `ino_t inode`（父目录）。
    pub const INODE: usize = 0;
    /// `mode_t mode`（omode = 模式位，**不含** `O_*`）。
    pub const MODE: usize = 8;
    /// `uid_t uid`。
    pub const UID: usize = 12;
    /// `gid_t gid`。
    pub const GID: usize = 16;
    /// `cp_grant_id_t grant`。
    pub const GRANT: usize = 20;
    /// `size_t path_len`（含 NUL）。
    pub const PATH_LEN: usize = 24;
}

/// `REQ_CREATE` 的**回复**载荷 LP64 域偏移（FS→VFS 方向）。
///
/// C: `mess_fs_vfs_create { off_t file_size; ino_t inode; mode_t mode;
/// uid_t uid; gid_t gid; }`（ipc.h:141-150）——与请求是两套结构。
pub mod create_reply_off {
    /// `off_t file_size`。
    pub const FILE_SIZE: usize = 0;
    /// `ino_t inode`。
    pub const INODE: usize = 8;
    /// `mode_t mode`。
    pub const MODE: usize = 16;
    /// `uid_t uid`。
    pub const UID: usize = 20;
    /// `gid_t gid`。
    pub const GID: usize = 24;
}

/// `REQ_MKDIR` 请求载荷的 LP64 域偏移。
///
/// C: `mess_vfs_fs_mkdir { ino_t inode; mode_t mode; uid_t uid; gid_t gid;
/// cp_grant_id_t grant; }`（`request.c:548-555` 的 `req_mkdir`；`grant`
/// 指向**最后组件名**）。
pub mod mkdir_req_off {
    /// `ino_t inode`（父目录）。
    pub const INODE: usize = 0;
    /// `mode_t mode`（含 `I_DIRECTORY` 与 umask 后的权限位）。
    pub const MODE: usize = 8;
    /// `uid_t uid`。
    pub const UID: usize = 12;
    /// `gid_t gid`。
    pub const GID: usize = 16;
    /// `cp_grant_id_t grant`。
    pub const GRANT: usize = 20;
}

/// `REQ_STAT` 请求载荷的 LP64 域偏移。
///
/// C: `mess_vfs_fs_stat { ino_t inode; cp_grant_id_t grant; }`
/// (ipc.h:…… 由 `request.c:1087-1096` 的 `req_stat_actual` 填充)。
pub mod stat_req_off {
    /// `ino_t inode`。
    pub const INODE: usize = 0;
    /// `cp_grant_id_t grant`（FS 往用户 `struct stat` 写的 magic grant）。
    pub const GRANT: usize = 8;
}

/// `REQ_READ`/`REQ_WRITE` 的**回复**载荷 LP64 域偏移（FS→VFS 方向）。
///
/// C: `mess_fs_vfs_readwrite { off_t seek_pos; size_t nbytes; }`
/// (ipc.h:214-220) —— 与请求载荷是两套结构，别混用（请求是
/// [`transfer_req_off`]）。
pub mod transfer_reply_off {
    /// `off_t seek_pos`（传输后的新位置）。
    pub const SEEK_POS: usize = 0;
    /// `size_t nbytes`（实际传输的字节数）。
    pub const NBYTES: usize = 8;
}

/// `REQ_GETDENTS` 请求载荷的 LP64 域偏移。
///
/// C: `mess_vfs_fs_getdents { ino_t inode; off_t seek_pos; cp_grant_id_t
/// grant; size_t mem_size; }`（ipc.h:1990-2000，`request.c:308-315` 的
/// `req_getdents_actual`）。`mem_size` 是目录项窗口的大小，数据经 `grant`
/// 由 FS 直接写进调用方缓冲；位置由 VFS 从 filp 取出随请求带上。
pub mod getdents_req_off {
    /// `ino_t inode`（目录的节点号）。
    pub const INODE: usize = 0;
    /// `off_t seek_pos`（本趟从目录的哪个偏移开始）。
    pub const SEEK_POS: usize = 8;
    /// `cp_grant_id_t grant`（FS 往用户缓冲写的 magic grant）。
    pub const GRANT: usize = 16;
    /// `size_t mem_size`（窗口字节数）。
    pub const MEM_SIZE: usize = 24;
}

/// `REQ_GETDENTS` 的**回复**载荷 LP64 域偏移（FS→VFS 方向）。
///
/// C: `mess_fs_vfs_getdents { off_t seek_pos; size_t nbytes; }`
/// （ipc.h:153-159）——与请求载荷是两套结构。字段序与
/// [`transfer_reply_off`] 相同，但 C 里是两个独立结构体，这里也各自成表
/// （对照修改时不必去猜另一个请求号的布局）。
pub mod getdents_reply_off {
    /// `off_t seek_pos`（下一趟的目录偏移）。
    pub const SEEK_POS: usize = 0;
    /// `size_t nbytes`（本次实际写出的字节数）。
    pub const NBYTES: usize = 8;
}

/// `REQ_READSUPER` 请求载荷的 LP64 域偏移。
///
/// C: `mess_vfs_fs_readsuper`（ipc.h:2112-2119）：`dev_t device`、
/// `uint32_t flags`（`REQ_RDONLY`/`REQ_ISROOT` — vfsif.h:8-9）、
/// `size_t path_len`、`cp_grant_id_t grant`（驱动标签）。
pub mod readsuper_req_off {
    /// `dev_t device`。
    pub const DEVICE: usize = 0;
    /// `uint32_t flags`。
    pub const FLAGS: usize = 8;
    /// `size_t path_len`（驱动标签长度）。
    pub const PATH_LEN: usize = 16;
    /// `cp_grant_id_t grant`（标签所在 grant）。
    pub const GRANT: usize = 24;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C 绝对值全量 pin:33 个请求号逐一对照 vfsif.h:42-73 的字面偏移。
    /// 基址错位(0x600 事故)或偏移错位都会在这里爆——单测偏移是不够的,
    /// FS 侧按 0xA00 派生的绝对值分派。
    #[test]
    fn test_req_family_matches_c_absolute_values() {
        assert_eq!(FS_BASE, 0xA00); // com.h:589
        let expected: [(i32, i32); 33] = [
            (1, 0xA01),
            (2, 0xA02),
            (3, 0xA03),
            (4, 0xA04),
            (5, 0xA05),
            (6, 0xA06),
            (7, 0xA07),
            (8, 0xA08),
            (9, 0xA09),
            (10, 0xA0A),
            (11, 0xA0B),
            (12, 0xA0C),
            (13, 0xA0D),
            (14, 0xA0E),
            (15, 0xA0F),
            (16, 0xA10),
            (17, 0xA11),
            (18, 0xA12),
            (19, 0xA13),
            (20, 0xA14),
            (21, 0xA15),
            (22, 0xA16),
            (23, 0xA17),
            (24, 0xA18),
            (25, 0xA19),
            (26, 0xA1A),
            (27, 0xA1B),
            (28, 0xA1C),
            (29, 0xA1D),
            (30, 0xA1E),
            (31, 0xA1F),
            (32, 0xA20),
            (33, 0xA21),
        ];
        let family = [
            REQ_GETNODE, REQ_PUTNODE, REQ_SLINK, REQ_FTRUNC, REQ_CHOWN, REQ_CHMOD,
            REQ_INHIBREAD, REQ_STAT, REQ_UTIME, REQ_STATVFS, REQ_BREAD, REQ_BWRITE, REQ_UNLINK,
            REQ_RMDIR, REQ_UNMOUNT, REQ_SYNC, REQ_NEW_DRIVER, REQ_FLUSH, REQ_READ, REQ_WRITE,
            REQ_MKNOD, REQ_MKDIR, REQ_CREATE, REQ_LINK, REQ_RENAME, REQ_LOOKUP, REQ_MOUNTPOINT,
            REQ_READSUPER, REQ_NEWNODE, REQ_RDLINK, REQ_GETDENTS, REQ_PEEK, REQ_BPEEK,
        ];
        assert_eq!(family.len(), expected.len());
        for (req, (index, absolute)) in family.iter().zip(expected.iter()) {
            assert_eq!(*req, *absolute, "REQ 偏移 {index} 的绝对值漂移");
            assert_eq!(req - FS_BASE, *index);
        }
    }

    /// 槽位数与 `IS_FS_RQ` 门(vfsif.h:75/:77):带内识别、带外拒绝,
    /// 包括 0x600(旧事故基址)与 0xB00(transid 带)。
    #[test]
    /// 请求载荷的 LP64 域偏移 pin：VFS 编码器与 FS 解码器共用这一张表，
    /// 偏移漂移必须在编译期可见（同一模块的常量 + 本测试）。
    #[test]
    fn test_request_payload_offsets() {
        // lookup：8 字节域两两相邻，32 位 flags 在 16，其余 8 字节对齐。
        assert_eq!(lookup_req_off::DIR_INO, 0);
        assert_eq!(lookup_req_off::ROOT_INO, 8);
        assert_eq!(lookup_req_off::FLAGS, 16);
        assert_eq!(lookup_req_off::PATH_LEN, 24);
        assert_eq!(lookup_req_off::PATH_SIZE, 32);
        assert_eq!(lookup_req_off::UCRED_SIZE, 40);
        assert_eq!(lookup_req_off::GRANT_PATH, 48);
        assert_eq!(lookup_req_off::GRANT_UCRED, 56);
        // 传输族：inode/seek_pos/grant/bytes 四个 8 字节域。
        assert_eq!(transfer_req_off::INODE, 0);
        assert_eq!(transfer_req_off::SEEK_POS, 8);
        assert_eq!(transfer_req_off::GRANT, 16);
        assert_eq!(transfer_req_off::BYTES, 24);
        // read 回复：seek_pos/nbytes 两个域（与请求的 transfer_req_off 分开）。
        assert_eq!(transfer_reply_off::SEEK_POS, 0);
        assert_eq!(transfer_reply_off::NBYTES, 8);
        // lookup 回复：八个域（特殊码只看 offset/inode/symloop）。
        assert_eq!(lookup_reply_off::OFFSET, 0);
        assert_eq!(lookup_reply_off::FILE_SIZE, 8);
        assert_eq!(lookup_reply_off::DEVICE, 16);
        assert_eq!(lookup_reply_off::INODE, 24);
        assert_eq!(lookup_reply_off::MODE, 32);
        assert_eq!(lookup_reply_off::UID, 36);
        assert_eq!(lookup_reply_off::GID, 40);
        assert_eq!(lookup_reply_off::SYMLOOP, 44);
        // create：请求六域 + 回复五域（两套结构）。
        assert_eq!(create_req_off::INODE, 0);
        assert_eq!(create_req_off::MODE, 8);
        assert_eq!(create_req_off::UID, 12);
        assert_eq!(create_req_off::GID, 16);
        assert_eq!(create_req_off::GRANT, 20);
        assert_eq!(create_req_off::PATH_LEN, 24);
        assert_eq!(create_reply_off::FILE_SIZE, 0);
        assert_eq!(create_reply_off::INODE, 8);
        assert_eq!(create_reply_off::MODE, 16);
        assert_eq!(create_reply_off::UID, 20);
        assert_eq!(create_reply_off::GID, 24);
        // mkdir：父 inode/mode/uid/gid/grant 五个域。
        assert_eq!(mkdir_req_off::INODE, 0);
        assert_eq!(mkdir_req_off::MODE, 8);
        assert_eq!(mkdir_req_off::UID, 12);
        assert_eq!(mkdir_req_off::GID, 16);
        assert_eq!(mkdir_req_off::GRANT, 20);
        // ftrunc：inode/trc_start/trc_end 三个域。
        assert_eq!(ftrunc_req_off::INODE, 0);
        assert_eq!(ftrunc_req_off::TRC_START, 8);
        assert_eq!(ftrunc_req_off::TRC_END, 16);
        // stat：inode/grant 两个域。
        assert_eq!(stat_req_off::INODE, 0);
        assert_eq!(stat_req_off::GRANT, 8);
        // getdents：请求四域（inode/seek_pos/grant/mem_size）+ 回复两域。
        assert_eq!(getdents_req_off::INODE, 0);
        assert_eq!(getdents_req_off::SEEK_POS, 8);
        assert_eq!(getdents_req_off::GRANT, 16);
        assert_eq!(getdents_req_off::MEM_SIZE, 24);
        assert_eq!(getdents_reply_off::SEEK_POS, 0);
        assert_eq!(getdents_reply_off::NBYTES, 8);
        // reads超級：device/flags/path_len/grant。
        assert_eq!(readsuper_req_off::DEVICE, 0);
        assert_eq!(readsuper_req_off::FLAGS, 8);
        assert_eq!(readsuper_req_off::PATH_LEN, 16);
        assert_eq!(readsuper_req_off::GRANT, 24);
    }

    /// lookup 的 `flags` 两位 pin（vfsif.h:12,16）——FS 侧按同一张表判
    /// `PATH_RET_SYMLINK`，发错字就等于让 FS 一路跟进符号链接。
    #[test]
    fn test_lookup_flag_bits() {
        assert_eq!(PATH_RET_SYMLINK, 0o10);
        assert_eq!(PATH_GET_UCRED, 0o20);
    }

    /// 三个特殊码的值 pin（vfsif.h:26-28，负值走回复的 m_type）。
    #[test]
    fn test_special_lookup_codes() {
        assert_eq!(EENTERMOUNT, -301);
        assert_eq!(ELEAVEMOUNT, -302);
        assert_eq!(ESYMLINK, -303);
    }

    /// transid 三式互为逆（vfsif.h:79-81）：合成/取回/剥离，含负结果值
    /// （回复里 errno 走高 16 位）。
    #[test]
    fn test_transid_round_trip() {
        let id = 0xB01u32 + 7; // VFS_TRANSID + worker slot
        let wire = trns_add_id(REQ_READ, id);
        assert_eq!(trns_get_id(wire), id);
        assert_eq!(trns_del_id(wire), REQ_READ);
        // 回复臂：结果值在高 16 位，负数（errno）按 16 位有符号读回。
        assert_eq!(trns_del_id(trns_add_id(-22, id)), -22);
        assert_eq!(trns_get_id(trns_add_id(-22, id)), id);
    }

    /// `is_fs_rq` 判的是剥掉 transid 后的调用号（线上值先 `trns_del_id`）。
    #[test]
    fn test_is_fs_rq_needs_transid_stripped() {
        let wire = trns_add_id(REQ_LOOKUP, 0xB01);
        assert!(!is_fs_rq(wire), "线上值高位是调用号，须先剥");
        assert!(is_fs_rq(trns_del_id(wire)));
    }

    fn test_nreqs_and_is_fs_rq_gate() {
        assert_eq!(NREQS, 34); // vfsif.h:75
        assert!(is_fs_rq(FS_BASE));
        assert!(is_fs_rq(REQ_BPEEK));
        assert!(!is_fs_rq(FS_BASE - 1));
        assert!(!is_fs_rq(FS_BASE + 0x100));
        assert!(!is_fs_rq(0x600)); // 旧事故基址,永不再来
        assert!(!is_fs_rq(0x61A));
        assert!(!is_fs_rq(0xB00)); // transid 带
        // 与设备 RS 命名空间(com.h:919/963/1038)保持区分。
        assert_ne!(FS_BASE & !0x7f, 0x480); // CDEV_RS_BASE
        assert_ne!(FS_BASE & !0x7f, 0x580); // BDEV_RS_BASE
    }
}

/// 文件状态回复的类型化结构（`fs_stat` 的载荷，字段集与 C `struct stat`
/// 一致；字节布局为稳定小端序，由 [`Stat::write_to`] 写出，虚拟文件系统
/// 服务按同一布局解码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    /// 设备号（`st_dev`）。
    pub device: u64,
    /// 索引节点号（`st_ino`）。
    pub inode: u64,
    /// 模式位（`st_mode`）。
    pub mode: u32,
    /// 硬链接数（`st_nlink`）。
    pub nlinks: u32,
    /// 属主（`st_uid`）。
    pub owner: u32,
    /// 属组（`st_gid`）。
    pub group: u32,
    /// 设备节点的设备号（`st_rdev`）。
    pub special: u64,
    /// 文件长度（`st_size`）。
    pub size: i64,
    /// 访问时间（`st_atime`）。
    pub accessed: i64,
    /// 修改时间（`st_mtime`）。
    pub modified: i64,
    /// 状态变化时间（`st_ctime`）。
    pub changed: i64,
    /// 首选输入输出块尺寸（`st_blksize`）。
    pub block_size: u64,
    /// 占用的五百一十二字节块数（`st_blocks`）。
    pub blocks: u64,
}

impl Stat {
    /// 写出所需的字节数。
    pub const SIZE: usize = 88;

    /// 全零状态：由调用方逐字段填充。
    pub const fn zeroed() -> Self {
        Self {
            device: 0,
            inode: 0,
            mode: 0,
            nlinks: 0,
            owner: 0,
            group: 0,
            special: 0,
            size: 0,
            accessed: 0,
            modified: 0,
            changed: 0,
            block_size: 0,
            blocks: 0,
        }
    }

    /// 按稳定小端序写出全部字段。缓冲区不足八十八字节报无效参数。
    pub fn write_to(&self, out: &mut [u8]) -> Result<(), crate::Errno> {
        if out.len() < Self::SIZE {
            return Err(crate::Errno::from_i32(crate::EINVAL));
        }
        out[0..2].copy_from_slice(&self.mode.to_le_bytes());
        out[2..4].copy_from_slice(&(self.nlinks as u16).to_le_bytes());
        out[4..6].copy_from_slice(&(self.owner as u16).to_le_bytes());
        out[6..8].copy_from_slice(&(self.group as u16).to_le_bytes());
        out[8..16].copy_from_slice(&self.device.to_le_bytes());
        out[16..24].copy_from_slice(&self.inode.to_le_bytes());
        out[24..32].copy_from_slice(&self.special.to_le_bytes());
        out[32..40].copy_from_slice(&(self.size as u64).to_le_bytes());
        out[40..48].copy_from_slice(&self.accessed.to_le_bytes());
        out[48..56].copy_from_slice(&self.modified.to_le_bytes());
        out[56..64].copy_from_slice(&self.changed.to_le_bytes());
        out[64..72].copy_from_slice(&self.block_size.to_le_bytes());
        out[72..80].copy_from_slice(&self.blocks.to_le_bytes());
        Ok(())
    }
}

/// 卷状态回复的类型化结构（`fs_statvfs` 的载荷，字段集与 C
/// `struct statvfs` 一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatVfs {
    /// 全部数据块数（`f_blocks`）。
    pub blocks: u64,
    /// 空闲数据块数（`f_bfree`）。
    pub blocks_free: u64,
    /// 非特权可用的数据块数（`f_bavail`）。
    pub blocks_available: u64,
    /// 块尺寸（`f_frsize`/`f_bsize`）。
    pub block_size: u64,
    /// 片尺寸（`f_frsize`）。
    pub fragment_size: u64,
    /// 首选输入输出尺寸（`f_iosize`）。
    pub io_size: u64,
    /// 全部索引节点数（`f_files`）。
    pub files: u64,
    /// 空闲索引节点数（`f_ffree`）。
    pub files_free: u64,
    /// 非特权可用的索引节点数（`f_favail`）。
    pub files_available: u64,
    /// 文件名最大长度（`f_namemax`）。
    pub name_max: u64,
}

impl StatVfs {
    /// 写出所需的字节数。
    pub const SIZE: usize = 80;

    /// 全零卷状态：由调用方逐字段填充。
    pub const fn zeroed() -> Self {
        Self {
            blocks: 0,
            blocks_free: 0,
            blocks_available: 0,
            block_size: 0,
            fragment_size: 0,
            io_size: 0,
            files: 0,
            files_free: 0,
            files_available: 0,
            name_max: 0,
        }
    }

    /// 按稳定小端序写出全部字段。
    pub fn write_to(&self, out: &mut [u8]) -> Result<(), crate::Errno> {
        if out.len() < Self::SIZE {
            return Err(crate::Errno::from_i32(crate::EINVAL));
        }
        let fields = [
            self.blocks,
            self.blocks_free,
            self.blocks_available,
            self.block_size,
            self.fragment_size,
            self.io_size,
            self.files,
            self.files_free,
            self.files_available,
            self.name_max,
        ];
        for (index, value) in fields.iter().enumerate() {
            out[index * 8..index * 8 + 8].copy_from_slice(&value.to_le_bytes());
        }
        Ok(())
    }
}
