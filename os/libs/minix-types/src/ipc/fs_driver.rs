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
/// Note that it tests the **call number with the transid already stripped**:
/// the on-wire `m_type` is `TRNS_ADD_ID(call_nr, transid)` (see below), so
/// testing the wire value directly yields `0xAxx << 16` rather than `0xAxx`.
pub const fn is_fs_rq(raw: i32) -> bool {
    (raw & !0xff) == FS_BASE
}

/// C: `TRNS_ADD_ID(t, id) (((t) << 16) | ((id) & 0xFFFF))` — vfsif.h:80.
/// Combines the call number and the transid into the on-wire `m_type`: VFS
/// sending a request and FS sending a reply both use this one expression (for
/// a reply `t` is the **result value**).
///
/// Relation to `minix-fs::protocol::TransactionId`: the latter is a **typed
/// wrapper** around the same C macros (`decode`/`encode_reply`/
/// `encode_request`), and the FS-side server dispatches through it; VFS's
/// `fs_comm::TransId` additionally carries worker-slot semantics
/// (`VFS_TRANSID + slot`). The formula in this module is authoritative for all
/// three (the three vfsif.h macro lines land here line by line); changing one
/// means updating the tests of the other two.
pub const fn trns_add_id(t: i32, id: u32) -> i32 {
    ((t as u32) << 16 | (id & 0xFFFF)) as i32
}

/// C: `TRNS_GET_ID(t) ((t) & 0xFFFF)` — vfsif.h:79 (the low 16 bits of the
/// wire value are the transid; `VFS_TRANSID + worker_tid`, com.h:911).
pub const fn trns_get_id(raw: i32) -> u32 {
    (raw as u32) & 0xFFFF
}

/// C: `TRNS_DEL_ID(t) ((short)((t) >> 16))` — vfsif.h:81.
///
/// Note that the `(short)` truncation is **deliberate**: the high 16 bits hold
/// either the call number (positive, e.g. `REQ_READ = 0xA13`) or the reply's
/// result value (which may be a negative errno), and both are read back as
/// 16-bit signed.
pub const fn trns_del_id(raw: i32) -> i32 {
    ((raw >> 16) as i16) as i32
}

/// LP64 field offsets of the `REQ_LOOKUP` request payload (C field order plus
/// 64-bit repacking).
///
/// C: `mess_vfs_fs_lookup` (ipc.h:2011-2035) has the field order
/// `dir_ino / root_ino / flags / path_len / path_size / ucred_size /
/// grant_path / grant_ucred`; on i386 each takes 4 bytes, on LP64 pointers and
/// `size_t` stretch to 8 while `uint32_t` stays 4 — **both sides (the VFS
/// encoder and the FS decoder) must use this table**, otherwise the name shifts
/// (the same family of accident as a request/response wire mismatch).
pub mod lookup_req_off {
    /// `ino_t dir_ino` (where the lookup starts).
    pub const DIR_INO: usize = 0;
    /// `ino_t root_ino` (the root for path resolution).
    pub const ROOT_INO: usize = 8;
    /// `uint32_t flags` (`LookupFlags`, see `minix-fs::protocol`).
    pub const FLAGS: usize = 16;
    /// `size_t path_len` (name length including the NUL).
    pub const PATH_LEN: usize = 24;
    /// `size_t path_size` (the grant window size).
    pub const PATH_SIZE: usize = 32;
    /// `size_t ucred_size` (credential area size; 0 = none attached).
    pub const UCRED_SIZE: usize = 40;
    /// `cp_grant_id_t grant_path` (the grant holding the name).
    pub const GRANT_PATH: usize = 48;
    /// `cp_grant_id_t grant_ucred` (the grant holding the credentials).
    pub const GRANT_UCRED: usize = 56;
}

/// LP64 field offsets of the `REQ_READ` / `REQ_GETDENTS` request payloads
/// (same prefix).
///
/// C: `mess_vfs_fs_readwrite` (ipc.h:2121-2131) and `mess_vfs_fs_getdents`
/// (ipc.h:1992-2002) have the same field order.
pub mod transfer_req_off {
    /// `ino_t inode`.
    pub const INODE: usize = 0;
    /// `off_t seek_pos`.
    pub const SEEK_POS: usize = 8;
    /// `cp_grant_id_t grant` (the data buffer lives in the caller).
    pub const GRANT: usize = 16;
    /// `size_t nbytes` (bytes to read) / `mem_size` (the getdents window).
    pub const BYTES: usize = 24;
}

/// C: `PATH_RET_SYMLINK (010)` — vfsif.h:12 (a `flags` bit of `REQ_LOOKUP`:
/// when the last component is a symbolic link, do **not** resolve it — return
/// the link itself).
///
/// The bit's semantics are implemented on the FS side
/// (`libfsdriver/lookup.c:249-251`); VFS only forwards it unchanged.
pub const PATH_RET_SYMLINK: u32 = 0o10;

/// C: `PATH_GET_UCRED (020)` — vfsif.h:16 (the request carries a credential
/// grant that FS uses for permission checks). The Rust credential face is not
/// wired yet; see the assertion in `encode_lookup`.
pub const PATH_GET_UCRED: u32 = 0o20;

/// C: `EENTERMOUNT (-301)` — vfsif.h:26 (the FS→VFS "entering a mount point"
/// special code, carried in the reply's `m_type`; it is not an error).
pub const EENTERMOUNT: i32 = -301;
/// C: `ELEAVEMOUNT (-302)` — vfsif.h:27 ("leaving a mount point").
pub const ELEAVEMOUNT: i32 = -302;
/// C: `ESYMLINK (-303)` — vfsif.h:28 ("this is a symbolic link").
pub const ESYMLINK: i32 = -303;

/// LP64 field offsets of the `REQ_LOOKUP` **reply** payload (FS→VFS).
///
/// C: `mess_fs_vfs_lookup` (ipc.h:163-176) has the field order
/// `offset / file_size / device / inode / mode / uid / gid / symloop`.
/// For the three special codes ([`EENTERMOUNT`]/[`ELEAVEMOUNT`]/[`ESYMLINK`])
/// only `offset` (plus `inode` and `symloop`) is meaningful; on `Ok` the
/// remaining fields are filled from `node_details`.
pub mod lookup_reply_off {
    /// `off_t offset` (bytes FS already consumed / remaining path offset).
    pub const OFFSET: usize = 0;
    /// `off_t file_size`.
    pub const FILE_SIZE: usize = 8;
    /// `dev_t device`.
    pub const DEVICE: usize = 16;
    /// `ino_t inode` (8-aligned, so 20..24 is padding).
    pub const INODE: usize = 24;
    /// `mode_t mode`.
    pub const MODE: usize = 32;
    /// `uid_t uid`.
    pub const UID: usize = 36;
    /// `gid_t gid`.
    pub const GID: usize = 40;
    /// `uint16_t symloop`.
    pub const SYMLOOP: usize = 44;
}

/// LP64 field offsets of the `REQ_FTRUNC` request payload.
///
/// C: `mess_vfs_fs_ftrunc { ino_t inode; off_t trc_start; off_t trc_end; }`
/// (`req_ftrunc` at `request.c:269-272`). Semantics: `trc_end == 0` means
/// "truncate at `trc_start`" (mfs `fs_trunc:439-443`); non-zero frees
/// `[start,end)`.
pub mod ftrunc_req_off {
    /// `ino_t inode`.
    pub const INODE: usize = 0;
    /// `off_t trc_start`.
    pub const TRC_START: usize = 8;
    /// `off_t trc_end`.
    pub const TRC_END: usize = 16;
}

/// LP64 field offsets of the `REQ_CREATE` request payload.
///
/// C: `mess_vfs_fs_create { ino_t inode; mode_t mode; uid_t uid; gid_t gid;
/// cp_grant_id_t grant; size_t path_len; }` (`req_create` at
/// `request.c:189-196`; `grant` points at the **last component name**).
pub mod create_req_off {
    /// `ino_t inode` (the parent directory).
    pub const INODE: usize = 0;
    /// `mode_t mode` (omode = mode bits, **without** `O_*`).
    pub const MODE: usize = 8;
    /// `uid_t uid`.
    pub const UID: usize = 12;
    /// `gid_t gid`.
    pub const GID: usize = 16;
    /// `cp_grant_id_t grant`.
    pub const GRANT: usize = 20;
    /// `size_t path_len` (including the NUL).
    pub const PATH_LEN: usize = 24;
}

/// LP64 field offsets of the `REQ_CREATE` **reply** payload (FS→VFS).
///
/// C: `mess_fs_vfs_create { off_t file_size; ino_t inode; mode_t mode;
/// uid_t uid; gid_t gid; }` (ipc.h:141-150) — a different structure from the
/// request.
pub mod create_reply_off {
    /// `off_t file_size`.
    pub const FILE_SIZE: usize = 0;
    /// `ino_t inode`.
    pub const INODE: usize = 8;
    /// `mode_t mode`.
    pub const MODE: usize = 16;
    /// `uid_t uid`.
    pub const UID: usize = 20;
    /// `gid_t gid`.
    pub const GID: usize = 24;
}

/// LP64 field offsets of the `REQ_MKDIR` request payload.
///
/// C: `mess_vfs_fs_mkdir { ino_t inode; mode_t mode; uid_t uid; gid_t gid;
/// cp_grant_id_t grant; }` (`req_mkdir` at `request.c:548-555`; `grant` points
/// at the **last component name**).
pub mod mkdir_req_off {
    /// `ino_t inode` (the parent directory).
    pub const INODE: usize = 0;
    /// `mode_t mode` (including `I_DIRECTORY` and the umask-applied
    /// permission bits).
    pub const MODE: usize = 8;
    /// `uid_t uid`.
    pub const UID: usize = 12;
    /// `gid_t gid`.
    pub const GID: usize = 16;
    /// `cp_grant_id_t grant`.
    pub const GRANT: usize = 20;
}

/// LP64 field offsets of the `REQ_STAT` request payload.
///
/// C: `mess_vfs_fs_stat { ino_t inode; cp_grant_id_t grant; }`
/// (ipc.h:…, filled by `req_stat_actual` at `request.c:1087-1096`).
pub mod stat_req_off {
    /// `ino_t inode`.
    pub const INODE: usize = 0;
    /// `cp_grant_id_t grant` (the magic grant FS writes the user
    /// `struct stat` into).
    pub const GRANT: usize = 8;
}

/// LP64 field offsets of the `REQ_READ`/`REQ_WRITE` **reply** payload (FS→VFS).
///
/// C: `mess_fs_vfs_readwrite { off_t seek_pos; size_t nbytes; }`
/// (ipc.h:214-220) — a different structure from the request payload; do not
/// mix them up (the request one is [`transfer_req_off`]).
pub mod transfer_reply_off {
    /// `off_t seek_pos` (the new position after the transfer).
    pub const SEEK_POS: usize = 0;
    /// `size_t nbytes` (the number of bytes actually transferred).
    pub const NBYTES: usize = 8;
}

/// LP64 field offsets of the `REQ_RENAME` request payload.
///
/// C: `mess_vfs_fs_rename { ino_t dir_old; ino_t dir_new; size_t len_old;
/// size_t len_new; cp_grant_id_t grant_old; cp_grant_id_t grant_new; }`
/// (ipc.h:2133-2143, `req_rename` at `request.c:927-955`). **Both grants are
/// direct** (the old name is a component name VFS kept, the new one comes from
/// the last component of name2, and both live in VFS memory); both name lengths
/// include the trailing NUL.
pub mod rename_req_off {
    /// `ino_t dir_old` (the parent directory of the old name).
    pub const DIR_OLD: usize = 0;
    /// `ino_t dir_new` (the parent directory of the new name).
    pub const DIR_NEW: usize = 8;
    /// `size_t len_old`.
    pub const LEN_OLD: usize = 16;
    /// `size_t len_new`.
    pub const LEN_NEW: usize = 24;
    /// `cp_grant_id_t grant_old`.
    pub const GRANT_OLD: usize = 32;
    /// `cp_grant_id_t grant_new`.
    pub const GRANT_NEW: usize = 36;
}

/// LP64 field offsets of the `REQ_SLINK` request payload.
///
/// C: `mess_vfs_fs_slink { ino_t inode; size_t path_len; size_t mem_size;
/// cp_grant_id_t grant_path; cp_grant_id_t grant_target; uid_t uid; gid_t
/// gid; }` (ipc.h:2146-2157, `req_slink_actual` at `request.c:990-1046`).
/// **Two grants**: `grant_path` is a component name in VFS memory (direct),
/// `grant_target` is the link target string in **user memory** (magic) — FS
/// reads the target from user space, the only two-grant request in this
/// family.
pub mod slink_req_off {
    /// `ino_t inode` (the parent directory).
    pub const INODE: usize = 0;
    /// `size_t path_len` (component name length, including the trailing NUL).
    pub const PATH_LEN: usize = 8;
    /// `size_t mem_size` (target string length, **excluding** the trailing
    /// NUL).
    pub const MEM_SIZE: usize = 16;
    /// `cp_grant_id_t grant_path`.
    pub const GRANT_PATH: usize = 24;
    /// `cp_grant_id_t grant_target`.
    pub const GRANT_TARGET: usize = 28;
    /// `uid_t uid` (owner of the new link = the caller's effective uid).
    pub const UID: usize = 32;
    /// `gid_t gid`.
    pub const GID: usize = 36;
}

/// LP64 field offsets of the `REQ_LINK` request payload.
///
/// C: `mess_vfs_fs_link { ino_t inode; ino_t dir_ino; cp_grant_id_t grant;
/// size_t path_len; }` (ipc.h:2010-2018, `req_link` at `request.c:390-418`).
/// `inode` is the **file being linked** and `dir_ino` is the new name's parent
/// directory — do not swap the two inos (in the struct the file comes first,
/// the directory second).
pub mod link_req_off {
    /// `ino_t inode` (the file being linked).
    pub const INODE: usize = 0;
    /// `ino_t dir_ino` (the parent directory of the new name).
    pub const DIR_INO: usize = 8;
    /// `cp_grant_id_t grant`.
    pub const GRANT: usize = 16;
    /// `size_t path_len` (new name length, including the trailing NUL).
    pub const PATH_LEN: usize = 24;
}

/// LP64 field offsets of the `REQ_UTIME` request payload.
///
/// C: `mess_vfs_fs_utime { ino_t inode; time_t actime; time_t modtime;
/// uint32_t acnsec; uint32_t modnsec; }` (ipc.h:2186-2195,
/// `request.c:1180-1199` (`req_utime`). `acnsec`/`modnsec` may be
/// `UTIME_OMIT` ("leave this one alone"), and VFS has already carried it down
/// unchanged.
pub mod utime_req_off {
    /// `ino_t inode`.
    pub const INODE: usize = 0;
    /// `time_t actime`.
    pub const ACTIME: usize = 8;
    /// `time_t modtime`.
    pub const MODTIME: usize = 16;
    /// `uint32_t acnsec`.
    pub const ACNSEC: usize = 24;
    /// `uint32_t modnsec`.
    pub const MODNSEC: usize = 28;
}

/// LP64 field offsets of the `REQ_MKNOD` request payload.
///
/// C: `mess_vfs_fs_mknod { dev_t device; ino_t inode; mode_t mode; uid_t uid;
/// gid_t gid; cp_grant_id_t grant; size_t path_len; }` (ipc.h:2051-2062,
/// `req_mknod` at `request.c:567-601`). `inode` is the **parent directory**;
/// `grant` points at a component name in VFS memory (a `CPF_READ` direct
/// grant); the reply carries **status only** (libfsdriver's `fsdriver_mknod`
/// marks `m_out` as `__unused`).
pub mod mknod_req_off {
    /// `dev_t device` (device number for char/block devices; 0 for FIFOs and
    /// regular files).
    pub const DEVICE: usize = 0;
    /// `ino_t inode` (the parent directory).
    pub const INODE: usize = 8;
    /// `mode_t mode` (the full mode already narrowed by umask, including the
    /// type bits).
    pub const MODE: usize = 16;
    /// `uid_t uid` (owner of the new node = the caller's effective uid).
    pub const UID: usize = 20;
    /// `gid_t gid`.
    pub const GID: usize = 24;
    /// `cp_grant_id_t grant`.
    pub const GRANT: usize = 28;
    /// `size_t path_len` (including the trailing NUL).
    pub const PATH_LEN: usize = 32;
}

/// LP64 field offsets of the `REQ_CHOWN` request payload.
///
/// C: `mess_vfs_fs_chown { ino_t inode; uid_t uid; gid_t gid; }`
/// (ipc.h:1953-1960, `req_chown` at `request.c:136-158`). The `-1` semantics
/// ("do not change this one") are resolved by the caller into the current value
/// first; see `protect::keep_id`.
pub mod chown_req_off {
    /// `ino_t inode`.
    pub const INODE: usize = 0;
    /// `uid_t uid`.
    pub const UID: usize = 8;
    /// `gid_t gid`.
    pub const GID: usize = 12;
}

/// LP64 field offsets of the `REQ_CHOWN` **reply** payload (FS→VFS).
///
/// C: `mess_fs_vfs_chown { mode_t mode; }` (ipc.h:134-138) — it returns the
/// **new mode** (FS may have cleared the setuid/setgid bits), not uid/gid:
/// uid/gid are written into the vnode by VFS itself (`do_chown:159-163`).
pub mod chown_reply_off {
    /// `mode_t mode`.
    pub const MODE: usize = 0;
}

/// LP64 field offsets of the `REQ_UNLINK` / `REQ_RMDIR` request payloads.
///
/// C: `mess_vfs_fs_unlink { ino_t inode; cp_grant_id_t grant; size_t
/// path_len; }` (ipc.h:2176-2183) — `req_unlink` (request.c:1149-1175) and
/// `req_rmdir` (request.c:966-989) **share this one structure** and differ only
/// in `m_type`. `grant` is a direct grant of a component name in VFS's own
/// memory (`CPF_READ`, not a magic grant: the name lives on the VFS side, not
/// the user side).
pub mod unlink_req_off {
    /// `ino_t inode` (the **parent directory**'s node number).
    pub const INODE: usize = 0;
    /// `cp_grant_id_t grant`.
    pub const GRANT: usize = 8;
    /// `size_t path_len` (including the trailing NUL).
    pub const PATH_LEN: usize = 16;
}

/// LP64 field offsets of the `REQ_CHMOD` request payload.
///
/// C: `mess_vfs_fs_chmod { ino_t inode; mode_t mode; }` (ipc.h:1944-1950,
/// `req_chmod` at `request.c:108-130`).
pub mod chmod_req_off {
    /// `ino_t inode`.
    pub const INODE: usize = 0;
    /// `mode_t mode` (4 bytes, naturally aligned after the 8-byte field).
    pub const MODE: usize = 8;
}

/// LP64 field offsets of the `REQ_CHMOD` **reply** payload (FS→VFS).
///
/// C: `mess_fs_vfs_chmod { mode_t mode; }` (ipc.h:127-131) — FS returns the
/// **mode actually in effect** (`request.c:127`'s
/// `*new_modep = m.m_fs_vfs_chmod.mode`, possibly narrowed by FS), and VFS
/// writes it back into the vnode cache.
pub mod chmod_reply_off {
    /// `mode_t mode`.
    pub const MODE: usize = 0;
}

/// LP64 field offsets of the `REQ_RDLINK` request payload.
///
/// C: `mess_vfs_fs_rdlink { ino_t inode; cp_grant_id_t grant; size_t
/// mem_size; }` (ipc.h:2101-2108, `req_rdlink_actual` at `request.c:717-748`).
/// `grant` is the magic grant FS writes the link text into the **user** buffer
/// with (`CPF_WRITE`); `mem_size` is the window the user supplied.
pub mod rdlink_req_off {
    /// `ino_t inode` (the symbolic link node).
    pub const INODE: usize = 0;
    /// `cp_grant_id_t grant`.
    pub const GRANT: usize = 8;
    /// `size_t mem_size`.
    pub const MEM_SIZE: usize = 16;
}

/// LP64 field offsets of the `REQ_RDLINK` **reply** payload (FS→VFS).
///
/// C: `mess_fs_vfs_rdlink { size_t nbytes; }` (ipc.h:191-195) — the link
/// text's byte count lives in the **payload**, and `m_type` is just `OK`
/// (`request.c:745`: `if (r == OK) r = m.m_fs_vfs_rdlink.nbytes;`).
pub mod rdlink_reply_off {
    /// `size_t nbytes`.
    pub const NBYTES: usize = 0;
}

/// LP64 field offsets of the `REQ_GETDENTS` request payload.
///
/// C: `mess_vfs_fs_getdents { ino_t inode; off_t seek_pos; cp_grant_id_t
/// grant; size_t mem_size; }` (ipc.h:1990-2000, `req_getdents_actual` at
/// `request.c:308-315`). `mem_size` is the size of the directory-entry window;
/// FS writes the data through `grant` straight into the caller's buffer, and
/// VFS takes the position out of the filp and sends it with the request.
pub mod getdents_req_off {
    /// `ino_t inode` (the directory's node number).
    pub const INODE: usize = 0;
    /// `off_t seek_pos` (which directory offset this pass starts at).
    pub const SEEK_POS: usize = 8;
    /// `cp_grant_id_t grant` (the magic grant FS writes into the user buffer).
    pub const GRANT: usize = 16;
    /// `size_t mem_size` (window size in bytes).
    pub const MEM_SIZE: usize = 24;
}

/// LP64 field offsets of the `REQ_GETDENTS` **reply** payload (FS→VFS).
///
/// C: `mess_fs_vfs_getdents { off_t seek_pos; size_t nbytes; }`
/// (ipc.h:153-159) — a different structure from the request payload. The field
/// order matches [`transfer_reply_off`], but in C they are two separate structs,
/// so they are separate tables here too (a paired edit need not guess the other
/// request number's layout).
pub mod getdents_reply_off {
    /// `off_t seek_pos` (the directory offset for the next pass).
    pub const SEEK_POS: usize = 0;
    /// `size_t nbytes` (bytes actually written by this call).
    pub const NBYTES: usize = 8;
}

/// LP64 field offsets of the `REQ_READSUPER` request payload.
///
/// C: `mess_vfs_fs_readsuper` (ipc.h:2112-2119): `dev_t device`,
/// `uint32_t flags` (`REQ_RDONLY`/`REQ_ISROOT` — vfsif.h:8-9),
/// `size_t path_len`, `cp_grant_id_t grant` (the driver label).
pub mod readsuper_req_off {
    /// `dev_t device`.
    pub const DEVICE: usize = 0;
    /// `uint32_t flags`.
    pub const FLAGS: usize = 8;
    /// `size_t path_len` (driver label length).
    pub const PATH_LEN: usize = 16;
    /// `cp_grant_id_t grant` (the grant holding the label).
    pub const GRANT: usize = 24;
}

/// LP64 field offsets of the `REQ_PUTNODE` request payload.
///
/// C: `mess_vfs_fs_putnode { ino_t inode; unsigned int count; }`
/// (ipc.h:2093-2098; `req_putnode` at `request.c:699-711` fills both cells) —
/// asks FS to drop `count` inode references (VFS's `fs_count` bulk return).
pub mod putnode_req_off {
    /// `ino_t inode`.
    pub const INODE: usize = 0;
    /// `unsigned int count`.
    pub const COUNT: usize = 8;
}

/// LP64 field offsets of the `REQ_NEW_DRIVER` request payload.
///
/// C: `mess_vfs_fs_new_driver { dev_t device; cp_grant_id_t grant;
/// size_t path_len; }` (ipc.h:2072-2079; `req_newdriver` at `request.c:664-691`
/// fills these three cells). It looks like [`readsuper_req_off`] but is **not**
/// the same structure: the second cell here is the grant, with no `flags`
/// cell.
pub mod new_driver_req_off {
    /// `dev_t device` (the block device's `v_sdev`, not the mounted
    /// partition's device number).
    pub const DEVICE: usize = 0;
    /// `cp_grant_id_t grant` (the grant holding the label, `CPF_READ`).
    pub const GRANT: usize = 8;
    /// `size_t path_len` (driver label length, including the trailing NUL).
    pub const PATH_LEN: usize = 16;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `StatVfs::write_user_statvfs` must land fields at the C `struct statvfs`
    /// offsets, not the old 10×u64 custom layout (which put `blocks` into
    /// `f_flag`@0). The mapping follows C `fs_statvfs`
    /// (`minix3/minix/fs/mfs/stadir.c:92-102`).
    #[test]
    fn test_statvfs_wire_matches_c_layout() {
        use crate::types::statvfs::statvfs_off as off;
        let v = StatVfs {
            blocks: 100,
            blocks_free: 90,
            blocks_available: 80,
            block_size: 4096,
            fragment_size: 1024,
            io_size: 1024,
            files: 50,
            files_free: 40,
            files_available: 40,
            name_max: 60,
        };
        let buf = v.write_user_statvfs();
        assert_eq!(buf.get_u64(off::BSIZE), 4096);
        assert_eq!(buf.get_u64(off::FRSIZE), 1024);
        assert_eq!(buf.get_u64(off::IOSIZE), 1024);
        assert_eq!(buf.get_u64(off::BLOCKS), 100);
        assert_eq!(buf.get_u64(off::BFREE), 90);
        assert_eq!(buf.get_u64(off::BAVAIL), 80);
        assert_eq!(buf.get_u64(off::FILES), 50);
        assert_eq!(buf.get_u64(off::FFREE), 40);
        assert_eq!(buf.get_u64(off::FFAVAIL), 40);
        assert_eq!(buf.get_u64(off::NAMEMAX), 60);
        // Fields FS does not fill stay zero (VFS patches them later): the
        // f_flag bits must be 0, not 100 pushed out by blocks (the signature of
        // the old wrong layout).
        assert_eq!(buf.get_u64(off::FLAG), 0);
    }

    /// Pins every C absolute value: all 33 request numbers against the literal
    /// offsets of vfsif.h:42-73. A wrong base (the 0x600 accident) or a wrong
    /// offset blows up here — testing offsets alone is not enough, since the FS
    /// side dispatches on absolute values derived from 0xA00.
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
            REQ_GETNODE,
            REQ_PUTNODE,
            REQ_SLINK,
            REQ_FTRUNC,
            REQ_CHOWN,
            REQ_CHMOD,
            REQ_INHIBREAD,
            REQ_STAT,
            REQ_UTIME,
            REQ_STATVFS,
            REQ_BREAD,
            REQ_BWRITE,
            REQ_UNLINK,
            REQ_RMDIR,
            REQ_UNMOUNT,
            REQ_SYNC,
            REQ_NEW_DRIVER,
            REQ_FLUSH,
            REQ_READ,
            REQ_WRITE,
            REQ_MKNOD,
            REQ_MKDIR,
            REQ_CREATE,
            REQ_LINK,
            REQ_RENAME,
            REQ_LOOKUP,
            REQ_MOUNTPOINT,
            REQ_READSUPER,
            REQ_NEWNODE,
            REQ_RDLINK,
            REQ_GETDENTS,
            REQ_PEEK,
            REQ_BPEEK,
        ];
        assert_eq!(family.len(), expected.len());
        for (req, (index, absolute)) in family.iter().zip(expected.iter()) {
            assert_eq!(*req, *absolute, "REQ 偏移 {index} 的绝对值漂移");
            assert_eq!(req - FS_BASE, *index);
        }
    }

    /// The slot count and the `IS_FS_RQ` gate (vfsif.h:75/:77): in-band is
    /// recognized, out-of-band is rejected, including 0x600 (the old accident
    /// base) and 0xB00 (the transid band).
    #[test]
    /// Pin of the request payload's LP64 field offsets: the VFS encoder and the
    /// FS decoder share this one table, so offset drift must be visible at
    /// compile time (the module's constants plus this test).
    #[test]
    fn test_request_payload_offsets() {
        // lookup: two adjacent 8-byte fields, the 32-bit flags at 16, the rest
        // 8-byte aligned.
        assert_eq!(lookup_req_off::DIR_INO, 0);
        assert_eq!(lookup_req_off::ROOT_INO, 8);
        assert_eq!(lookup_req_off::FLAGS, 16);
        assert_eq!(lookup_req_off::PATH_LEN, 24);
        assert_eq!(lookup_req_off::PATH_SIZE, 32);
        assert_eq!(lookup_req_off::UCRED_SIZE, 40);
        assert_eq!(lookup_req_off::GRANT_PATH, 48);
        assert_eq!(lookup_req_off::GRANT_UCRED, 56);
        // The transfer family: four 8-byte fields inode/seek_pos/grant/bytes.
        assert_eq!(transfer_req_off::INODE, 0);
        assert_eq!(transfer_req_off::SEEK_POS, 8);
        assert_eq!(transfer_req_off::GRANT, 16);
        assert_eq!(transfer_req_off::BYTES, 24);
        // The read reply: two fields seek_pos/nbytes (separate from the
        // request's transfer_req_off).
        assert_eq!(transfer_reply_off::SEEK_POS, 0);
        assert_eq!(transfer_reply_off::NBYTES, 8);
        // The lookup reply: eight fields (the special codes only look at
        // offset/inode/symloop).
        assert_eq!(lookup_reply_off::OFFSET, 0);
        assert_eq!(lookup_reply_off::FILE_SIZE, 8);
        assert_eq!(lookup_reply_off::DEVICE, 16);
        assert_eq!(lookup_reply_off::INODE, 24);
        assert_eq!(lookup_reply_off::MODE, 32);
        assert_eq!(lookup_reply_off::UID, 36);
        assert_eq!(lookup_reply_off::GID, 40);
        assert_eq!(lookup_reply_off::SYMLOOP, 44);
        // create: six request fields + five reply fields (two structures).
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
        // mkdir: five fields parent inode/mode/uid/gid/grant.
        assert_eq!(mkdir_req_off::INODE, 0);
        assert_eq!(mkdir_req_off::MODE, 8);
        assert_eq!(mkdir_req_off::UID, 12);
        assert_eq!(mkdir_req_off::GID, 16);
        assert_eq!(mkdir_req_off::GRANT, 20);
        // ftrunc: three fields inode/trc_start/trc_end.
        assert_eq!(ftrunc_req_off::INODE, 0);
        assert_eq!(ftrunc_req_off::TRC_START, 8);
        assert_eq!(ftrunc_req_off::TRC_END, 16);
        // stat: two fields inode/grant.
        assert_eq!(stat_req_off::INODE, 0);
        assert_eq!(stat_req_off::GRANT, 8);
        // rename: six request fields (two direct grants: old name + new name).
        assert_eq!(rename_req_off::DIR_OLD, 0);
        assert_eq!(rename_req_off::DIR_NEW, 8);
        assert_eq!(rename_req_off::LEN_OLD, 16);
        assert_eq!(rename_req_off::LEN_NEW, 24);
        assert_eq!(rename_req_off::GRANT_OLD, 32);
        assert_eq!(rename_req_off::GRANT_NEW, 36);
        // slink: seven request fields (two grants: name direct + target magic).
        assert_eq!(slink_req_off::INODE, 0);
        assert_eq!(slink_req_off::PATH_LEN, 8);
        assert_eq!(slink_req_off::MEM_SIZE, 16);
        assert_eq!(slink_req_off::GRANT_PATH, 24);
        assert_eq!(slink_req_off::GRANT_TARGET, 28);
        assert_eq!(slink_req_off::UID, 32);
        assert_eq!(slink_req_off::GID, 36);
        // link: four request fields (the file ino first, the parent directory
        // ino after).
        assert_eq!(link_req_off::INODE, 0);
        assert_eq!(link_req_off::DIR_INO, 8);
        assert_eq!(link_req_off::GRANT, 16);
        assert_eq!(link_req_off::PATH_LEN, 24);
        // utime: five request fields (inode/actime/modtime/acnsec/modnsec).
        assert_eq!(utime_req_off::INODE, 0);
        assert_eq!(utime_req_off::ACTIME, 8);
        assert_eq!(utime_req_off::MODTIME, 16);
        assert_eq!(utime_req_off::ACNSEC, 24);
        assert_eq!(utime_req_off::MODNSEC, 28);
        // mknod: seven request fields (device/inode/mode/uid/gid/grant/path_len).
        assert_eq!(mknod_req_off::DEVICE, 0);
        assert_eq!(mknod_req_off::INODE, 8);
        assert_eq!(mknod_req_off::MODE, 16);
        assert_eq!(mknod_req_off::UID, 20);
        assert_eq!(mknod_req_off::GID, 24);
        assert_eq!(mknod_req_off::GRANT, 28);
        assert_eq!(mknod_req_off::PATH_LEN, 32);
        // chown: three request fields + one reply field (the new mode).
        assert_eq!(chown_req_off::INODE, 0);
        assert_eq!(chown_req_off::UID, 8);
        assert_eq!(chown_req_off::GID, 12);
        assert_eq!(chown_reply_off::MODE, 0);
        // unlink/rmdir: three request fields (a shared structure, only m_type
        // differs), no payload reply.
        assert_eq!(unlink_req_off::INODE, 0);
        assert_eq!(unlink_req_off::GRANT, 8);
        assert_eq!(unlink_req_off::PATH_LEN, 16);
        // chmod: two request fields + one reply field (the mode actually in
        // effect).
        assert_eq!(chmod_req_off::INODE, 0);
        assert_eq!(chmod_req_off::MODE, 8);
        assert_eq!(chmod_reply_off::MODE, 0);
        // rdlink: three request fields + one reply field (the byte count is in
        // the payload, m_type is just OK).
        assert_eq!(rdlink_req_off::INODE, 0);
        assert_eq!(rdlink_req_off::GRANT, 8);
        assert_eq!(rdlink_req_off::MEM_SIZE, 16);
        assert_eq!(rdlink_reply_off::NBYTES, 0);
        // getdents: four request fields (inode/seek_pos/grant/mem_size) + two
        // reply fields.
        assert_eq!(getdents_req_off::INODE, 0);
        assert_eq!(getdents_req_off::SEEK_POS, 8);
        assert_eq!(getdents_req_off::GRANT, 16);
        assert_eq!(getdents_req_off::MEM_SIZE, 24);
        assert_eq!(getdents_reply_off::SEEK_POS, 0);
        assert_eq!(getdents_reply_off::NBYTES, 8);
        // readsuper: device/flags/path_len/grant.
        assert_eq!(readsuper_req_off::DEVICE, 0);
        assert_eq!(readsuper_req_off::FLAGS, 8);
        assert_eq!(readsuper_req_off::PATH_LEN, 16);
        assert_eq!(readsuper_req_off::GRANT, 24);
        // new_driver: device/grant/path_len (no flags cell — a different
        // structure from readsuper; mixing them up sends the grant as flags).
        assert_eq!(new_driver_req_off::DEVICE, 0);
        assert_eq!(new_driver_req_off::GRANT, 8);
        assert_eq!(new_driver_req_off::PATH_LEN, 16);
        // putnode: two cells inode/count (the bulk fs_count return).
        assert_eq!(putnode_req_off::INODE, 0);
        assert_eq!(putnode_req_off::COUNT, 8);
    }

    /// Pins the two `flags` bits of lookup (vfsif.h:12,16) — the FS side tests
    /// `PATH_RET_SYMLINK` against the same table, so sending the wrong word
    /// means FS follows symbolic links all the way.
    #[test]
    fn test_lookup_flag_bits() {
        assert_eq!(PATH_RET_SYMLINK, 0o10);
        assert_eq!(PATH_GET_UCRED, 0o20);
    }

    /// Pins the values of the three special codes (vfsif.h:26-28; negative
    /// values ride the reply's m_type).
    #[test]
    fn test_special_lookup_codes() {
        assert_eq!(EENTERMOUNT, -301);
        assert_eq!(ELEAVEMOUNT, -302);
        assert_eq!(ESYMLINK, -303);
    }

    /// The three transid formulas are mutual inverses (vfsif.h:79-81):
    /// combine/fetch/strip, including negative result values (an errno in a
    /// reply rides the high 16 bits).
    #[test]
    fn test_transid_round_trip() {
        let id = 0xB01u32 + 7; // VFS_TRANSID + worker slot
        let wire = trns_add_id(REQ_READ, id);
        assert_eq!(trns_get_id(wire), id);
        assert_eq!(trns_del_id(wire), REQ_READ);
        // The reply arm: the result value sits in the high 16 bits, and a
        // negative one (errno) is read back as 16-bit signed.
        assert_eq!(trns_del_id(trns_add_id(-22, id)), -22);
        assert_eq!(trns_get_id(trns_add_id(-22, id)), id);
    }

    /// `is_fs_rq` tests the call number after stripping the transid (strip the
    /// wire value with `trns_del_id` first).
    #[test]
    fn test_is_fs_rq_needs_transid_stripped() {
        let wire = trns_add_id(REQ_LOOKUP, 0xB01);
        assert!(!is_fs_rq(wire), "线上值高位是调用号，须先剥");
        assert!(is_fs_rq(trns_del_id(wire)));
    }

    // Registers the `#[test]` that was previously missing (the "named but
    // never registered" test never ran).
    #[test]
    fn test_nreqs_and_is_fs_rq_gate() {
        assert_eq!(NREQS, 34); // vfsif.h:75
        assert!(is_fs_rq(FS_BASE));
        assert!(is_fs_rq(REQ_BPEEK));
        assert!(!is_fs_rq(FS_BASE - 1));
        assert!(!is_fs_rq(FS_BASE + 0x100));
        assert!(!is_fs_rq(0x600)); // old accident base, never again
        assert!(!is_fs_rq(0x61A));
        assert!(!is_fs_rq(0xB00)); // transid band
        // Stays distinct from the device RS namespace (com.h:919/963/1038).
        assert_ne!(FS_BASE & !0x7f, 0x480); // CDEV_RS_BASE
        assert_ne!(FS_BASE & !0x7f, 0x580); // BDEV_RS_BASE
    }
}

/// The typed structure of a file status reply (the `fs_stat` payload, with the
/// same field set as C `struct stat`). This is the **VFS↔FS driver-side** field
/// carrier; when delivering to the calling process it must be written out with
/// the user `struct stat` layout ([`crate::types::stat::Stat`], this project's
/// byte mirror of C `struct stat`, `st_mode@8`); see
/// [`Stat::write_user_stat`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    /// Device number (`st_dev`).
    pub device: u64,
    /// Inode number (`st_ino`).
    pub inode: u64,
    /// Mode bits (`st_mode`).
    pub mode: u32,
    /// Hard link count (`st_nlink`).
    pub nlinks: u32,
    /// Owner (`st_uid`).
    pub owner: u32,
    /// Group (`st_gid`).
    pub group: u32,
    /// Device number of the device node (`st_rdev`).
    pub special: u64,
    /// File length (`st_size`).
    pub size: i64,
    /// Access time (`st_atime`).
    pub accessed: i64,
    /// Modification time (`st_mtime`).
    pub modified: i64,
    /// Status change time (`st_ctime`).
    pub changed: i64,
    /// Preferred I/O block size (`st_blksize`).
    pub block_size: u64,
    /// Number of 512-byte blocks occupied (`st_blocks`).
    pub blocks: u64,
}

impl Stat {
    /// The byte count delivered to the calling process: equal to the user
    /// `struct stat`'s ([`crate::types::stat::Stat`]) `size_of` (152 on LP64) —
    /// i.e. the magic-grant window C `req_stat_actual` (request.c:1087) hands to
    /// FS for direct writes.
    pub const USER_STAT_SIZE: usize = core::mem::size_of::<crate::types::stat::Stat>();

    /// All-zero state: the caller fills the fields one by one.
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

    /// Serialize into a **user-space `struct stat`**
    /// ([`crate::types::stat::Stat`]) value, so FS can write straight into the
    /// caller's buffer through a magic grant.
    ///
    /// C `req_stat_actual` (request.c:1087) grants the calling process's
    /// `struct stat` buffer to FS for direct writes, so the byte layout FS
    /// produces must strictly match C `struct stat` (`st_mode@8`,
    /// `st_size@112`, …) — that is this project's `types::stat::Stat`, pinned by
    /// `#[repr(C)]` plus layout tests, so no offsets are computed by hand.
    /// Fields the driver side does not provide (time nanoseconds, creation time
    /// `st_birthtim`, `st_flags`/`st_gen`/`st_spare`) are left zero per C
    /// convention. The earlier `ls` treated `/bin` as a regular file precisely
    /// because that leg never laid down bytes and the user buffer stayed at
    /// `st_mode=0`.
    pub fn write_user_stat(&self) -> crate::types::stat::Stat {
        // SAFETY: `types::stat::Stat` is a `#[repr(C)]` POD with no invariants,
        // so all-zero is a legal initial value; the public fields are overwritten
        // one by one below and the private padding stays zero.
        let mut s: crate::types::stat::Stat = unsafe { core::mem::zeroed() };
        s.st_dev = self.device;
        s.st_mode = self.mode;
        s.st_ino = self.inode;
        s.st_nlink = self.nlinks;
        s.st_uid = self.owner;
        s.st_gid = self.group;
        s.st_rdev = self.special;
        s.st_atim.tv_sec = self.accessed;
        s.st_mtim.tv_sec = self.modified;
        s.st_ctim.tv_sec = self.changed;
        s.st_size = self.size;
        s.st_blocks = self.blocks as i64;
        s.st_blksize = self.block_size as i32;
        s
    }
}

/// The typed structure of a volume status reply (the `fs_statvfs` payload, with
/// the same field set as C `struct statvfs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatVfs {
    /// Total number of data blocks (`f_blocks`).
    pub blocks: u64,
    /// Number of free data blocks (`f_bfree`).
    pub blocks_free: u64,
    /// Number of data blocks available to unprivileged users (`f_bavail`).
    pub blocks_available: u64,
    /// Block size (`f_frsize`/`f_bsize`).
    pub block_size: u64,
    /// Fragment size (`f_frsize`).
    pub fragment_size: u64,
    /// Preferred I/O size (`f_iosize`).
    pub io_size: u64,
    /// Total number of inodes (`f_files`).
    pub files: u64,
    /// Number of free inodes (`f_ffree`).
    pub files_free: u64,
    /// Number of inodes available to unprivileged users (`f_favail`).
    pub files_available: u64,
    /// Maximum file name length (`f_namemax`).
    pub name_max: u64,
}

impl StatVfs {
    /// `sizeof(struct statvfs)` — used for the grant window and the length
    /// copied to the user.
    pub const USER_STATVFS_SIZE: usize = crate::types::statvfs::STATVFS_SIZE;

    /// All-zero volume status: the caller fills the fields one by one.
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

    /// Serialize into a grant fill buffer in C `struct statvfs` wire layout.
    ///
    /// The field→offset mapping follows C `fs_statvfs`
    /// (`minix3/minix/fs/mfs/stadir.c:92-102`):
    /// `f_bsize`←block_size, `f_frsize`←fragment_size, `f_iosize`←io_size,
    /// `f_blocks/f_bfree/f_bavail`←blocks/blocks_free/blocks_available,
    /// `f_files/f_ffree/f_favail`←files/files_free/files_available,
    /// `f_namemax`←name_max.
    /// Fields C leaves unfilled by FS (`f_flag`/name/`f_owner`/`f_fsidx` etc.)
    /// stay zero and are patched with local values by VFS's `req_statvfs`
    /// (matching the FS/VFS split in the statvfs.rs module header). Replaces the
    /// old 10×u64 custom layout `write_to` (not the C layout, zero callers,
    /// dead).
    pub fn write_user_statvfs(&self) -> crate::types::statvfs::StatvfsBuf {
        use crate::types::statvfs::{StatvfsBuf, statvfs_off as off};
        let mut buf = StatvfsBuf::new();
        buf.set_u64(off::BSIZE, self.block_size);
        buf.set_u64(off::FRSIZE, self.fragment_size);
        buf.set_u64(off::IOSIZE, self.io_size);
        buf.set_u64(off::BLOCKS, self.blocks);
        buf.set_u64(off::BFREE, self.blocks_free);
        buf.set_u64(off::BAVAIL, self.blocks_available);
        buf.set_u64(off::FILES, self.files);
        buf.set_u64(off::FFREE, self.files_free);
        buf.set_u64(off::FFAVAIL, self.files_available);
        buf.set_u64(off::NAMEMAX, self.name_max);
        buf
    }
}
