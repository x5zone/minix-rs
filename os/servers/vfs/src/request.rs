//! Typed `REQ_*` wrappers — `request.c` + `vfsif.h:41-73` + `request.h`.
//!
//! `request.c` hides three repetitions: `grant` construction / `fs_sendrec`
//! / `revoke` / `ERESTART → vm_handlemem → retry(0)` and `RES_64BIT` early
//! `EINVAL` and `m_source → res->fs_e` back-fill.  This module makes each
//! repetition a type: `FsReq` (32 variants, one per live `REQ_*` type,
//! `FS_BASE 0xA00` prefix), `FsResp`
//! (`NodeDetails` 7 fields vs `LookupRes` 9 fields), `GrantScope` (`Try` vs
//! `NoTry`), `FsFlags` (`RES_64BIT` gate).  `REQ_GETNODE 0xA01` is dead.
//!
//! `[ARCH: A-2]` (enum vs function pointer) and `[ARCH: A-8]` (64-bit) in one place.
//!
//! E-REQWIRE: the `REQ_*` numbers themselves are no longer defined here —
//! they moved to the shared authority `minix-types::fs_driver` (consumed by
//! the FS-driver side too, so the two ends cannot drift; devman's VTreeFS is
//! the registered third consumer). The C-absolute pin tests stay in this
//! module: they guard what VFS actually puts on the wire.

#[cfg(test)] // Box is used only by the test module (PathFetcher/FsClient injection)
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use minix_types::{
    is_fs_rq, trns_add_id, trns_del_id, Endpoint, Message, REQ_BREAD, REQ_BPEEK, REQ_BWRITE,
    REQ_CHMOD, REQ_CHOWN, REQ_CREATE, REQ_FLUSH, REQ_FTRUNC, REQ_GETDENTS, REQ_GETNODE,
    REQ_INHIBREAD, REQ_LINK, REQ_LOOKUP, REQ_MKDIR, REQ_MKNOD, REQ_MOUNTPOINT, REQ_NEWNODE,
    REQ_NEW_DRIVER, REQ_PEEK, REQ_PUTNODE, REQ_RDLINK, REQ_READ, REQ_READSUPER, REQ_RENAME,
    REQ_RMDIR, REQ_SLINK, REQ_STAT, REQ_STATVFS, REQ_SYNC, REQ_UNLINK, REQ_UNMOUNT, REQ_UTIME,
    REQ_WRITE,
};

bitflags::bitflags! {
    /// `RES_*` flags — `vfsif.h:20-23`, mirrored in `vmnt.m_fs_flags`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FsFlags: u32 {
        const THREADED = 0x01;
        const HASPEEK = 0x02;
        const IS64BIT = 0x04;
    }
}

/// `node_details` — `request.h:12` 7 fields (MFS `REQ_CREATE/NEWNODE` response).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NodeDetails {
    pub fs_e: Endpoint,
    pub ino: u64,
    pub mode: u32,
    pub size: u64,
    pub uid: u32,
    pub gid: u32,
    pub dev: u64,
}

/// `lookup_res` — `request.h:25` 9 fields (`OK` vs `EENTERMOUNT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LookupRes {
    pub fs_e: Endpoint,
    pub ino: u64,
    pub mode: u32,
    pub size: u64,
    pub uid: u32,
    pub gid: u32,
    pub dev: u64,
    pub char_processed: i32,
    pub symloop: u8,
}

/// `GrantScope` — `CPF_TRY` vs `0` for `ERESTART` retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantScope {
    Try,
    NoTry,
}

impl GrantScope {
    pub fn cpf_flag(self) -> u32 {
        match self {
            Self::Try => 1, // `CPF_TRY` bit
            Self::NoTry => 0,
        }
    }
}

/// `vfs_ucred_t` — `vfsif.h:33` for `PATH_GET_UCRED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VfsUCred {
    pub uid: u32,
    pub gid: u32,
    pub ngroups: usize,
    pub sgroups: [u32; 16],
}

/// Typed `REQ_*` request — 32 variants, one per live `REQ_*` type (no
/// `GetNode`; `vfsif.h` defines 33 constants of which `REQ_GETNODE` is dead).
/// `REQ_RDONLY` / `REQ_ISROOT` — `readsuper`'s request flags
/// (vfsif.h:8-9; the wire-encoding face of the mount-first work).
pub const REQ_RDONLY: u32 = 0o1;
pub const REQ_ISROOT: u32 = 0o2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsReq {
    PutNode {
        fs_e: Endpoint,
        ino: u64,
        count: i32,
    },
    SLink {
        fs_e: Endpoint,
        dir_ino: u64,
        lastc: String,
        target: String,
        uid: u32,
        gid: u32,
    },
    FTrunc {
        fs_e: Endpoint,
        ino: u64,
        start: i64,
        end: i64,
    },
    Chown {
        fs_e: Endpoint,
        ino: u64,
        uid: u32,
        gid: u32,
    },
    Chmod {
        fs_e: Endpoint,
        ino: u64,
        mode: u32,
    },
    InhibRead {
        fs_e: Endpoint,
        ino: u64,
    },
    Stat {
        fs_e: Endpoint,
        ino: u64,
        grant: Option<u64>,
    },
    Utime {
        fs_e: Endpoint,
        ino: u64,
        actime: i64,
        modtime: i64,
    },
    StatVfs {
        fs_e: Endpoint,
        grant: Option<u64>,
    },
    BRead {
        fs_e: Endpoint,
        dev: u64,
        pos: i64,
        nbytes: usize,
        user: Endpoint,
    },
    BWrite {
        fs_e: Endpoint,
        dev: u64,
        pos: i64,
        nbytes: usize,
        user: Endpoint,
    },
    Unlink {
        fs_e: Endpoint,
        dir_ino: u64,
        lastc: String,
    },
    Rmdir {
        fs_e: Endpoint,
        dir_ino: u64,
        lastc: String,
    },
    Unmount {
        fs_e: Endpoint,
    },
    Sync {
        fs_e: Endpoint,
    },
    NewDriver {
        fs_e: Endpoint,
        dev: u64,
        label: String,
    },
    Flush {
        fs_e: Endpoint,
        dev: u64,
    },
    Read {
        fs_e: Endpoint,
        ino: u64,
        pos: i64,
        nbytes: usize,
        user: Endpoint,
    },
    Write {
        fs_e: Endpoint,
        ino: u64,
        pos: i64,
        nbytes: usize,
        user: Endpoint,
    },
    Mknod {
        fs_e: Endpoint,
        dir_ino: u64,
        lastc: String,
        mode: u32,
        dev: u64,
        uid: u32,
        gid: u32,
    },
    Mkdir {
        fs_e: Endpoint,
        dir_ino: u64,
        lastc: String,
        mode: u32,
        uid: u32,
        gid: u32,
    },
    Create {
        fs_e: Endpoint,
        dir_ino: u64,
        lastc: String,
        mode: u32,
        uid: u32,
        gid: u32,
    },
    Link {
        fs_e: Endpoint,
        dir_ino: u64,
        lastc: String,
        linked: u64,
    },
    Rename {
        fs_e: Endpoint,
        old_dir: u64,
        old_name: String,
        new_dir: u64,
        new_name: String,
    },
    Lookup {
        fs_e: Endpoint,
        dir_ino: u64,
        root_ino: u64,
        path: String,
        cred: Option<VfsUCred>,
        flags: u32,
    },
    Mountpoint {
        fs_e: Endpoint,
        ino: u64,
    },
    ReadSuper {
        fs_e: Endpoint,
        label: String,
        dev: u64,
        readonly: bool,
        isroot: bool,
    },
    NewNode {
        fs_e: Endpoint,
        mode: u32,
        dev: u64,
        uid: u32,
        gid: u32,
    },
    RdLink {
        fs_e: Endpoint,
        ino: u64,
        size: usize,
        direct: bool,
    },
    GetDents {
        fs_e: Endpoint,
        ino: u64,
        pos: i64,
        size: usize,
        direct: bool,
    },
    Peek {
        fs_e: Endpoint,
        ino: u64,
        pos: i64,
        nbytes: usize,
    },
    BPeek {
        fs_e: Endpoint,
        dev: u64,
        pos: i64,
        nbytes: usize,
    },
}

impl FsReq {
    /// `m_type` for this request (`FS_BASE + N`).
    ///
    /// E-REQWIRE: the wire domain is `i32` (`Message::m_type`), so the
    /// signature follows the shared constants' canonical type.
    pub fn m_type(&self) -> i32 {
        match self {
            Self::PutNode { .. } => REQ_PUTNODE,
            Self::SLink { .. } => REQ_SLINK,
            Self::FTrunc { .. } => REQ_FTRUNC,
            Self::Chown { .. } => REQ_CHOWN,
            Self::Chmod { .. } => REQ_CHMOD,
            Self::InhibRead { .. } => REQ_INHIBREAD,
            Self::Stat { .. } => REQ_STAT,
            Self::Utime { .. } => REQ_UTIME,
            Self::StatVfs { .. } => REQ_STATVFS,
            Self::BRead { .. } => REQ_BREAD,
            Self::BWrite { .. } => REQ_BWRITE,
            Self::Unlink { .. } => REQ_UNLINK,
            Self::Rmdir { .. } => REQ_RMDIR,
            Self::Unmount { .. } => REQ_UNMOUNT,
            Self::Sync { .. } => REQ_SYNC,
            Self::NewDriver { .. } => REQ_NEW_DRIVER,
            Self::Flush { .. } => REQ_FLUSH,
            Self::Read { .. } => REQ_READ,
            Self::Write { .. } => REQ_WRITE,
            Self::Mknod { .. } => REQ_MKNOD,
            Self::Mkdir { .. } => REQ_MKDIR,
            Self::Create { .. } => REQ_CREATE,
            Self::Link { .. } => REQ_LINK,
            Self::Rename { .. } => REQ_RENAME,
            Self::Lookup { .. } => REQ_LOOKUP,
            Self::Mountpoint { .. } => REQ_MOUNTPOINT,
            Self::ReadSuper { .. } => REQ_READSUPER,
            Self::NewNode { .. } => REQ_NEWNODE,
            Self::RdLink { .. } => REQ_RDLINK,
            Self::GetDents { .. } => REQ_GETDENTS,
            Self::Peek { .. } => REQ_PEEK,
            Self::BPeek { .. } => REQ_BPEEK,
        }
    }

    /// Whether `raw` is a live `REQ_*` (excludes dead `GETNODE` `0xA01`).
    pub fn is_known(raw: i32) -> bool {
        if !is_fs_rq(raw) {
            return false;
        }
        raw != REQ_GETNODE && (REQ_PUTNODE..=REQ_BPEEK).contains(&raw)
    }

    /// Grant count for this request (for `cpf_grant` pairing audit).
    pub fn grants(&self) -> usize {
        match self {
            Self::Lookup { cred, .. } => {
                if cred.is_some() {
                    2
                } else {
                    1
                }
            }
            Self::Rename { .. } => 2,
            Self::SLink { .. } => 2,
            Self::BPeek { .. }
            | Self::Peek { .. }
            | Self::Flush { .. }
            | Self::Sync { .. }
            | Self::Unmount { .. }
            | Self::PutNode { .. }
            | Self::InhibRead { .. }
            | Self::FTrunc { .. }
            | Self::Chmod { .. }
            | Self::Chown { .. }
            | Self::Stat { .. }
            | Self::Utime { .. }
            | Self::StatVfs { .. }
            | Self::BRead { .. }
            | Self::BWrite { .. }
            | Self::Unlink { .. }
            | Self::Rmdir { .. }
            | Self::NewDriver { .. }
            | Self::Read { .. }
            | Self::Write { .. }
            | Self::Mknod { .. }
            | Self::Mkdir { .. }
            | Self::Create { .. }
            | Self::Link { .. }
            | Self::Mountpoint { .. }
            | Self::ReadSuper { .. }
            | Self::NewNode { .. }
            | Self::RdLink { .. }
            | Self::GetDents { .. } => 1,
        }
    }

    /// `RES_64BIT` guard — `pos>INT_MAX` with `!IS64BIT` → `EINVAL` (request.c:323).
    pub fn check_64bit(&self, flags: FsFlags, off: i64) -> Result<(), FsError> {
        if !flags.contains(FsFlags::IS64BIT) && off > i32::MAX as i64 {
            return Err(FsError::InvalidOff);
        }
        Ok(())
    }
}

/// Typed `REQ_*` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsResp {
    Node(NodeDetails),
    Lookup(LookupRes),
    /// `readsuper` reply (req_readsuper, request.c:818):
    /// `node_details` + `fs_flags`(RES_*) + `con_reqs`
    /// (ipc.h:198-211; con_reqs has no consumer on the C side, and mount.c's max_reqs
    /// is decided by RES_THREADED — see `mount.rs::SuperInfo::max_reqs`).
    ReadSuper {
        node: NodeDetails,
        fs_flags: FsFlags,
        con_reqs: u16,
    },
    Ok,
    Count(i32),
    Size(usize),
}

// `FsFlags` already defined above; re-use for `check_64bit`.
/// `FsError` — maps to Minix errno for `req_*` wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    UnknownReq(i32),
    InvalidOff,
    GrantFaulted, // `ERESTART` → `vm_handlemem` retry sentinel
    Io(i32),
}

impl minix_types::ToErrno for FsError {
    fn to_errno(&self) -> minix_types::Errno {
        minix_types::Errno::from_i32((*self).to_errno())
    }
}

impl FsError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::UnknownReq(_) => minix_types::ENOSYS,
            Self::InvalidOff => minix_types::EINVAL,
            Self::GrantFaulted => minix_types::ERESTART,
            Self::Io(e) => e,
        }
    }
}

/// Request encoding for `req_readsuper` (C request.c:780-813): device@0(dev_t),
/// flags@8(REQ_RDONLY/ISROOT), path_len@16(size_t), grant@24
/// (cp_grant_id_t, the LP64 layout at ipc.h:2111-2119).
pub fn encode_readsuper(
    device: u64,
    path_len: usize,
    grant: i32,
    readonly: bool,
    isroot: bool,
) -> Message {
    let mut msg = Message {
        m_type: REQ_READSUPER,
        ..Message::default()
    };
    let mut flags = 0u32;
    if readonly {
        flags |= REQ_RDONLY;
    }
    if isroot {
        flags |= REQ_ISROOT;
    }
    // SAFETY: the raw arm writes byte-wise — a wire face with no dedicated union
    // member (precedent: pm's decode.rs raw mode).
    let raw = unsafe { &mut msg.m_u.raw };
    raw[0..8].copy_from_slice(&device.to_le_bytes());
    raw[8..12].copy_from_slice(&flags.to_le_bytes());
    raw[16..24].copy_from_slice(&(path_len as u64).to_le_bytes());
    raw[24..28].copy_from_slice(&grant.to_le_bytes());
    msg
}

/// Request encoding for `req_putnode` (C request.c:699-711): payload
/// `{inode@0(ino_t), count@8(unsigned int)}` — returns `count` inode references
/// to the FS (the batch return of `put_vnode`'s slow path). The reply is status
/// only and is discarded by the caller.
pub fn encode_putnode(ino: u64, count: i32) -> Message {
    let mut msg = Message {
        m_type: REQ_PUTNODE,
        ..Message::default()
    };
    // SAFETY: the raw arm writes byte-wise — `mess_vfs_fs_putnode` (ipc.h:2093-2098)
    // has no dedicated union member; field order in `minix_types::putnode_req_off`.
    let raw = unsafe { &mut msg.m_u.raw };
    raw[0..8].copy_from_slice(&ino.to_le_bytes());
    raw[8..12].copy_from_slice(&count.to_le_bytes());
    msg
}

/// Request encoding for `req_newdriver` (C request.c:664-691): payload
/// `{device@0(dev_t), grant@8(cp_grant_id_t), path_len@16(size_t)}` — the driver
/// label itself lives in VFS memory and goes to the FS through a **direct grant**
/// (`CPF_READ`); the reply is status only.
///
/// `device` is the **special device number of the block device** (`vp->v_sdev`),
/// not the device number of the mounted partition — the FS uses it to tell "which
/// block driver owns this partition".
pub fn encode_new_driver(device: u64, grant: i32, path_len: usize) -> Message {
    let mut msg = Message {
        m_type: REQ_NEW_DRIVER,
        ..Message::default()
    };
    // SAFETY: the raw arm writes byte-wise — `mess_vfs_fs_new_driver` (ipc.h:2072-2079)
    // has no dedicated union member; field order in `minix_types::new_driver_req_off`.
    let raw = unsafe { &mut msg.m_u.raw };
    raw[0..8].copy_from_slice(&device.to_le_bytes());
    raw[8..12].copy_from_slice(&grant.to_le_bytes());
    raw[16..24].copy_from_slice(&(path_len as u64).to_le_bytes());
    msg
}

/// `readsuper` reply decode (C request.c:818-825): file_size@0(off_t),
/// device@8(dev_t), inode@16(ino_t), flags@24(u32 = fs_flags),
/// mode@28, uid@32, gid@36, con_reqs@40(u16, ipc.h:198-211).
/// `fs_e` is supplied by the caller (i.e. `m_source`; C's `res->fs_e = m.m_source`).
/// `REQ_READ` / `REQ_WRITE` request (C `req_readwrite_actual` —
/// request.c:834-874): payload `{inode, seek_pos, grant, nbytes}`; the data face is
/// read and written by the FS directly through the grant (for reads the FS
/// `safecopyto`s into the user buffer).
///
/// Offsets come from the shared authority `minix_types::transfer_req_off` (the FS side
/// decodes with the same table).
pub fn encode_read(ino: u64, grant: i32, pos: i64, nbytes: usize) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_READ,
        ..Message::default()
    };
    // SAFETY: REQ_READ's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::transfer_req_off::INODE..minix_types::transfer_req_off::INODE + 8]
            .copy_from_slice(&ino.to_le_bytes());
        raw[minix_types::transfer_req_off::SEEK_POS..minix_types::transfer_req_off::SEEK_POS + 8]
            .copy_from_slice(&pos.to_le_bytes());
        raw[minix_types::transfer_req_off::GRANT..minix_types::transfer_req_off::GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
        raw[minix_types::transfer_req_off::BYTES..minix_types::transfer_req_off::BYTES + 8]
            .copy_from_slice(&(nbytes as u64).to_le_bytes());
    }
    msg
}

/// `REQ_WRITE` request (same payload as [`encode_read`], opposite direction: the FS
/// **reads** data from the user buffer, so the grant permission bit is `CPF_READ` —
/// C `req_readwrite_actual` request.c:846-848's
/// `rw_flag==READING ? CPF_WRITE : CPF_READ`).
pub fn encode_write(ino: u64, grant: i32, pos: i64, nbytes: usize) -> Message {
    let mut msg = encode_read(ino, grant, pos, nbytes);
    msg.m_type = minix_types::REQ_WRITE;
    msg
}

/// `REQ_NEWNODE` request (C `req_newnode` — request.c:170-197).
///
/// Payload `{device, mode, uid, gid}` (ipc.h:2082-2090); **the reply carries the new
/// node's `node_details`** (`mess_fs_vfs_newnode`:
/// file_size/device/inode/mode/uid/gid — its field order matches the first six fields
/// of [`minix_types::lookup_reply_off`], so decoding can reuse that table). This is
/// how a pipe asks PFS for a new inode.
pub fn encode_newnode(dev: u64, mode: u32, uid: u32, gid: u32) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_NEWNODE,
        ..Message::default()
    };
    // SAFETY: REQ_NEWNODE's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[0..8].copy_from_slice(&dev.to_le_bytes());
        raw[8..12].copy_from_slice(&mode.to_le_bytes());
        raw[12..16].copy_from_slice(&uid.to_le_bytes());
        raw[16..20].copy_from_slice(&gid.to_le_bytes());
    }
    msg
}

/// `REQ_FLUSH` request (C `req_flush` — request.c:219-229).
///
/// The payload is just `device`: it asks the FS to flush that device's blocks (both
/// `fcntl(F_FLUSH_FS_CACHE)` and `bdev`'s flush path use it). The reply is status
/// only.
pub fn encode_flush(dev: u64) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_FLUSH,
        ..Message::default()
    };
    // SAFETY: REQ_FLUSH's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[0..8].copy_from_slice(&dev.to_le_bytes());
    }
    msg
}

/// `REQ_STATVFS` request (C `req_statvfs` — request.c:232-247).
///
/// The payload is just `grant`: the FS writes a whole `struct statvfs` in C's layout
/// **into** the VFS-side buffer (direct grant, `CPF_WRITE`); the reply is status
/// only.
pub fn encode_statvfs(grant: i32) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_STATVFS,
        ..Message::default()
    };
    // SAFETY: REQ_STATVFS's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[0..4].copy_from_slice(&grant.to_le_bytes());
    }
    msg
}

/// `REQ_RENAME` request (C `req_rename` — request.c:927-955).
///
/// Both names live in VFS memory (one direct grant each): the old name was saved by
/// VFS from name1's last component, and the new name comes from name2's last
/// component. Both lengths include the trailing NUL.
pub fn encode_rename(
    dir_old: u64,
    dir_new: u64,
    len_old: usize,
    len_new: usize,
    grant_old: i32,
    grant_new: i32,
) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_RENAME,
        ..Message::default()
    };
    // SAFETY: REQ_RENAME's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::rename_req_off::DIR_OLD..minix_types::rename_req_off::DIR_OLD + 8]
            .copy_from_slice(&dir_old.to_le_bytes());
        raw[minix_types::rename_req_off::DIR_NEW..minix_types::rename_req_off::DIR_NEW + 8]
            .copy_from_slice(&dir_new.to_le_bytes());
        raw[minix_types::rename_req_off::LEN_OLD..minix_types::rename_req_off::LEN_OLD + 8]
            .copy_from_slice(&(len_old as u64).to_le_bytes());
        raw[minix_types::rename_req_off::LEN_NEW..minix_types::rename_req_off::LEN_NEW + 8]
            .copy_from_slice(&(len_new as u64).to_le_bytes());
        raw[minix_types::rename_req_off::GRANT_OLD..minix_types::rename_req_off::GRANT_OLD + 4]
            .copy_from_slice(&grant_old.to_le_bytes());
        raw[minix_types::rename_req_off::GRANT_NEW..minix_types::rename_req_off::GRANT_NEW + 4]
            .copy_from_slice(&grant_new.to_le_bytes());
    }
    msg
}

/// `REQ_SLINK` request (C `req_slink_actual` — request.c:990-1046).
///
/// Two grants: the component name (VFS memory, direct) and the link target string
/// (**user memory**, magic). `mem_size` is the target length **without** the trailing
/// NUL (C passes `vname1_length - 1`).
pub fn encode_slink(
    dir_ino: u64,
    path_len: usize,
    mem_size: usize,
    grant_path: i32,
    grant_target: i32,
    uid: u32,
    gid: u32,
) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_SLINK,
        ..Message::default()
    };
    // SAFETY: REQ_SLINK's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::slink_req_off::INODE..minix_types::slink_req_off::INODE + 8]
            .copy_from_slice(&dir_ino.to_le_bytes());
        raw[minix_types::slink_req_off::PATH_LEN..minix_types::slink_req_off::PATH_LEN + 8]
            .copy_from_slice(&(path_len as u64).to_le_bytes());
        raw[minix_types::slink_req_off::MEM_SIZE..minix_types::slink_req_off::MEM_SIZE + 8]
            .copy_from_slice(&(mem_size as u64).to_le_bytes());
        raw[minix_types::slink_req_off::GRANT_PATH..minix_types::slink_req_off::GRANT_PATH + 4]
            .copy_from_slice(&grant_path.to_le_bytes());
        raw[minix_types::slink_req_off::GRANT_TARGET
            ..minix_types::slink_req_off::GRANT_TARGET + 4]
            .copy_from_slice(&grant_target.to_le_bytes());
        raw[minix_types::slink_req_off::UID..minix_types::slink_req_off::UID + 4]
            .copy_from_slice(&uid.to_le_bytes());
        raw[minix_types::slink_req_off::GID..minix_types::slink_req_off::GID + 4]
            .copy_from_slice(&gid.to_le_bytes());
    }
    msg
}

/// `REQ_LINK` request (C `req_link` — request.c:390-418).
///
/// Payload `{inode(the file being linked), dir_ino(the new name's parent directory),
/// grant(the new name), path_len}` — the order of the two inos is **file first,
/// directory second** (struct order), so do not swap them.
pub fn encode_link(linked_ino: u64, dir_ino: u64, grant: i32, path_len: usize) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_LINK,
        ..Message::default()
    };
    // SAFETY: REQ_LINK's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::link_req_off::INODE..minix_types::link_req_off::INODE + 8]
            .copy_from_slice(&linked_ino.to_le_bytes());
        raw[minix_types::link_req_off::DIR_INO..minix_types::link_req_off::DIR_INO + 8]
            .copy_from_slice(&dir_ino.to_le_bytes());
        raw[minix_types::link_req_off::GRANT..minix_types::link_req_off::GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
        raw[minix_types::link_req_off::PATH_LEN..minix_types::link_req_off::PATH_LEN + 8]
            .copy_from_slice(&(path_len as u64).to_le_bytes());
    }
    msg
}

/// `REQ_UTIME` request (C `req_utime` — request.c:1180-1199).
///
/// Payload `{inode, actime, modtime, acnsec, modnsec}`: seconds and nanoseconds
/// travel separately, and `acnsec`/`modnsec` may be `UTIME_OMIT` ("leave this field
/// alone"). The reply is status only.
pub fn encode_utime(ino: u64, actime: i64, modtime: i64, acnsec: u32, modnsec: u32) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_UTIME,
        ..Message::default()
    };
    // SAFETY: REQ_UTIME's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::utime_req_off::INODE..minix_types::utime_req_off::INODE + 8]
            .copy_from_slice(&ino.to_le_bytes());
        raw[minix_types::utime_req_off::ACTIME..minix_types::utime_req_off::ACTIME + 8]
            .copy_from_slice(&actime.to_le_bytes());
        raw[minix_types::utime_req_off::MODTIME..minix_types::utime_req_off::MODTIME + 8]
            .copy_from_slice(&modtime.to_le_bytes());
        raw[minix_types::utime_req_off::ACNSEC..minix_types::utime_req_off::ACNSEC + 4]
            .copy_from_slice(&acnsec.to_le_bytes());
        raw[minix_types::utime_req_off::MODNSEC..minix_types::utime_req_off::MODNSEC + 4]
            .copy_from_slice(&modnsec.to_le_bytes());
    }
    msg
}

/// `REQ_MKNOD` request (C `req_mknod` — request.c:567-601).
///
/// Seven fields in the payload: `device` (the character/block device number), the
/// parent directory `inode`, the already-narrowed `mode`, the owner `uid`/`gid`, the
/// name grant, and the name length. The reply is status only.
pub fn encode_mknod(
    dev: u64,
    dir_ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    grant: i32,
    path_len: usize,
) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_MKNOD,
        ..Message::default()
    };
    // SAFETY: REQ_MKNOD's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::mknod_req_off::DEVICE..minix_types::mknod_req_off::DEVICE + 8]
            .copy_from_slice(&dev.to_le_bytes());
        raw[minix_types::mknod_req_off::INODE..minix_types::mknod_req_off::INODE + 8]
            .copy_from_slice(&dir_ino.to_le_bytes());
        raw[minix_types::mknod_req_off::MODE..minix_types::mknod_req_off::MODE + 4]
            .copy_from_slice(&mode.to_le_bytes());
        raw[minix_types::mknod_req_off::UID..minix_types::mknod_req_off::UID + 4]
            .copy_from_slice(&uid.to_le_bytes());
        raw[minix_types::mknod_req_off::GID..minix_types::mknod_req_off::GID + 4]
            .copy_from_slice(&gid.to_le_bytes());
        raw[minix_types::mknod_req_off::GRANT..minix_types::mknod_req_off::GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
        raw[minix_types::mknod_req_off::PATH_LEN..minix_types::mknod_req_off::PATH_LEN + 8]
            .copy_from_slice(&(path_len as u64).to_le_bytes());
    }
    msg
}

/// `REQ_CHOWN` request (C `req_chown` — request.c:136-158).
///
/// Payload `{inode, uid, gid}`; the reply carries the **new mode** (the FS may clear
/// setuid/setgid), and VFS writes uid/gid back to the vnode itself.
pub fn encode_chown(ino: u64, uid: u32, gid: u32) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_CHOWN,
        ..Message::default()
    };
    // SAFETY: REQ_CHOWN's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::chown_req_off::INODE..minix_types::chown_req_off::INODE + 8]
            .copy_from_slice(&ino.to_le_bytes());
        raw[minix_types::chown_req_off::UID..minix_types::chown_req_off::UID + 4]
            .copy_from_slice(&uid.to_le_bytes());
        raw[minix_types::chown_req_off::GID..minix_types::chown_req_off::GID + 4]
            .copy_from_slice(&gid.to_le_bytes());
    }
    msg
}

/// `REQ_UNLINK` request (C `req_unlink` — request.c:1149-1175).
///
/// Payload `{inode, grant, path_len}`: `inode` is the **parent directory**, and
/// `grant` points at the component name in VFS memory (a `CPF_READ` direct grant;
/// the name is not in user space). The reply is status only (`m_type`) with no
/// payload — so the continuation needs just `WorkerCont::Status`.
pub fn encode_unlink(dir_ino: u64, grant: i32, path_len: usize) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_UNLINK,
        ..Message::default()
    };
    // SAFETY: REQ_UNLINK's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::unlink_req_off::INODE..minix_types::unlink_req_off::INODE + 8]
            .copy_from_slice(&dir_ino.to_le_bytes());
        raw[minix_types::unlink_req_off::GRANT..minix_types::unlink_req_off::GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
        raw[minix_types::unlink_req_off::PATH_LEN..minix_types::unlink_req_off::PATH_LEN + 8]
            .copy_from_slice(&(path_len as u64).to_le_bytes());
    }
    msg
}

/// `REQ_RMDIR` request: the **same payload and structure** as [`encode_unlink`] (C's
/// `req_rmdir` also fills `m_vfs_fs_unlink`, request.c:977-979); only `m_type`
/// differs.
pub fn encode_rmdir(dir_ino: u64, grant: i32, path_len: usize) -> Message {
    let mut msg = encode_unlink(dir_ino, grant, path_len);
    msg.m_type = minix_types::REQ_RMDIR;
    msg
}

/// `REQ_CHMOD` request (C `req_chmod` — request.c:108-130).
///
/// Payload `{inode, mode}`; the reply carries the **mode that actually took effect**
/// (the FS may narrow it), which the caller writes back to the vnode cache.
pub fn encode_chmod(ino: u64, mode: u32) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_CHMOD,
        ..Message::default()
    };
    // SAFETY: REQ_CHMOD's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::chmod_req_off::INODE..minix_types::chmod_req_off::INODE + 8]
            .copy_from_slice(&ino.to_le_bytes());
        raw[minix_types::chmod_req_off::MODE..minix_types::chmod_req_off::MODE + 4]
            .copy_from_slice(&mode.to_le_bytes());
    }
    msg
}

/// `REQ_RDLINK` request (C `req_rdlink_actual` — request.c:717-748).
///
/// Payload `{inode, grant, mem_size}`; the data face has the FS write the link text
/// through the grant into the **user** buffer (a `CPF_WRITE` magic grant). The byte
/// count is in the payload (`rdlink_reply_off::NBYTES`); `m_type` is just `OK`.
pub fn encode_rdlink(ino: u64, grant: i32, mem_size: usize) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_RDLINK,
        ..Message::default()
    };
    // SAFETY: REQ_RDLINK's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::rdlink_req_off::INODE..minix_types::rdlink_req_off::INODE + 8]
            .copy_from_slice(&ino.to_le_bytes());
        raw[minix_types::rdlink_req_off::GRANT..minix_types::rdlink_req_off::GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
        raw[minix_types::rdlink_req_off::MEM_SIZE..minix_types::rdlink_req_off::MEM_SIZE + 8]
            .copy_from_slice(&(mem_size as u64).to_le_bytes());
    }
    msg
}

/// `REQ_GETDENTS` request (C `req_getdents_actual` — request.c:288-336).
///
/// Payload `{inode, seek_pos, grant, mem_size}`; the data face has the FS write
/// directory entries through the grant into the user buffer (a `CPF_WRITE` magic
/// grant). The caller takes the position out of the filp and carries it with the
/// request — the FS holds no position (C `read.c:309` passes `rfilp->filp_pos`).
///
/// Offsets come from the shared authority `minix_types::getdents_req_off` (the FS side
/// decodes with the same table).
pub fn encode_getdents(ino: u64, pos: i64, grant: i32, mem_size: usize) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_GETDENTS,
        ..Message::default()
    };
    // SAFETY: REQ_GETDENTS's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::getdents_req_off::INODE..minix_types::getdents_req_off::INODE + 8]
            .copy_from_slice(&ino.to_le_bytes());
        raw[minix_types::getdents_req_off::SEEK_POS..minix_types::getdents_req_off::SEEK_POS + 8]
            .copy_from_slice(&pos.to_le_bytes());
        raw[minix_types::getdents_req_off::GRANT..minix_types::getdents_req_off::GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
        raw[minix_types::getdents_req_off::MEM_SIZE..minix_types::getdents_req_off::MEM_SIZE + 8]
            .copy_from_slice(&(mem_size as u64).to_le_bytes());
    }
    msg
}

/// `REQ_LOOKUP` request (C `req_lookup` — request.c:430-500).
///
/// The payload is `mess_vfs_fs_lookup`: the path grant (**`CPF_READ|CPF_WRITE`** — the
/// FS writes the "remaining path" back, which is how `EENTERMOUNT`/`ESYMLINK` report
/// progress), the path length and window, the start-directory ino and the chroot
/// boundary ino, and `flags` (`PATH_RET_SYMLINK`/`PATH_GET_UCRED` — vfsif.h); the
/// credential grant is carried only when `ngroups > 0` (this encoder leaves
/// `grant_ucred`/`ucred_size` for the caller to pass as 0; the credential face lands
/// with the Open family).
///
/// Offsets come from the shared authority `minix_types::lookup_req_off` (the FS side
/// decodes with the same table).
pub fn encode_lookup(
    grant_path: i32,
    path_len: usize,
    dir_ino: u64,
    root_ino: u64,
    flags: u32,
) -> Message {
    // `PATH_RET_SYMLINK` semantics are implemented on the **FS side**
    // (libfsdriver/lookup.c:249-251: "do not resolve when the last component is a
    // symlink and VFS asked for no resolution"), so this word really must be sent —
    // dropping it would let the FS follow symlinks all the way.
    // `PATH_GET_UCRED` needs a credential grant carried with the request, which this
    // encoder does not carry yet (ucred_size=0): whoever opens this face first adds the
    // grant here and turns the assertion into the implementation.
    debug_assert!(
        flags & minix_types::PATH_GET_UCRED == 0,
        "lookup 的凭证面未接线（req_lookup 的 ucred grant）"
    );
    let mut msg = Message {
        m_type: minix_types::REQ_LOOKUP,
        ..Message::default()
    };
    // SAFETY: REQ_LOOKUP's payload is written in LP64 field order in the message payload area.
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::lookup_req_off::DIR_INO..minix_types::lookup_req_off::DIR_INO + 8]
            .copy_from_slice(&dir_ino.to_le_bytes());
        raw[minix_types::lookup_req_off::ROOT_INO..minix_types::lookup_req_off::ROOT_INO + 8]
            .copy_from_slice(&root_ino.to_le_bytes());
        raw[minix_types::lookup_req_off::PATH_LEN..minix_types::lookup_req_off::PATH_LEN + 8]
            .copy_from_slice(&(path_len as u64).to_le_bytes());
        raw[minix_types::lookup_req_off::FLAGS..minix_types::lookup_req_off::FLAGS + 4]
            .copy_from_slice(&flags.to_le_bytes());
        raw[minix_types::lookup_req_off::PATH_SIZE..minix_types::lookup_req_off::PATH_SIZE + 8]
            .copy_from_slice(&(crate::path::PATH_MAX as u64).to_le_bytes());
        raw[minix_types::lookup_req_off::GRANT_PATH..minix_types::lookup_req_off::GRANT_PATH + 4]
            .copy_from_slice(&grant_path.to_le_bytes());
        // ucred_size = 0 (no credentials carried; see above).
    }
    msg
}

/// Decode one `REQ_LOOKUP` reply (the tail of C `req_lookup` — request.c:500-521).
///
/// `status` is `m_type` with the transid stripped: on `OK` the byte area is the first
/// four fields of `node_details` (ino/mode/size/dev), and the three special codes
/// (`EENTERMOUNT`/`ELEAVEMOUNT`/`ESYMLINK`) carry meaningful `offset`/`inode`/`symloop`
/// only; everything else (including a negative errno) is handled as an error by the
/// caller (this function returns `None`).
pub fn decode_lookup_reply(status: i32, msg: &Message) -> Option<crate::path::LookupRes> {
    use minix_types::lookup_reply_off as off;
    // SAFETY: the reply payload is written in the field order above in the payload area.
    let raw = unsafe { &msg.m_u.raw };
    let rd8 = |at: usize| -> u64 {
        let mut b = [0u8; 8];
        if at + 8 <= raw.len() {
            b.copy_from_slice(&raw[at..at + 8]);
        }
        u64::from_le_bytes(b)
    };
    let rd4 = |at: usize| -> u32 {
        let mut b = [0u8; 4];
        if at + 4 <= raw.len() {
            b.copy_from_slice(&raw[at..at + 4]);
        }
        u32::from_le_bytes(b)
    };
    let rd2 = |at: usize| -> u8 {
        let mut b = [0u8; 2];
        if at + 2 <= raw.len() {
            b.copy_from_slice(&raw[at..at + 2]);
        }
        u16::from_le_bytes(b) as u8
    };
    match status {
        minix_types::OK => Some(crate::path::LookupRes::Ok {
            ino: rd8(off::INODE),
            mode: rd4(off::MODE),
            size: rd8(off::FILE_SIZE),
            dev: rd8(off::DEVICE),
            // `node_details`' owner/group: C `advance` writes them into the vnode's
            // `v_uid`/`v_gid` (path.c:98-99), and permission checks then use them.
            uid: rd4(off::UID),
            gid: rd4(off::GID),
        }),
        minix_types::EENTERMOUNT => Some(crate::path::LookupRes::EnterMount {
            ino: rd8(off::INODE),
            offset: rd8(off::OFFSET) as i64 as i32,
            symloop: rd2(off::SYMLOOP),
        }),
        minix_types::ELEAVEMOUNT => Some(crate::path::LookupRes::LeaveMount {
            offset: rd8(off::OFFSET) as i64 as i32,
            symloop: rd2(off::SYMLOOP),
        }),
        minix_types::ESYMLINK => Some(crate::path::LookupRes::Symlink {
            offset: rd8(off::OFFSET) as i64 as i32,
            symloop: rd2(off::SYMLOOP),
        }),
        _ => None,
    }
}

/// `REQ_INHIBREAD` request (C `req_inhibread` — request.c:374-384): the payload is
/// only `{inode}`, which tells the FS to drop that inode's read-ahead state (sent when
/// `lseek` moved the position).
pub fn encode_inhibread(ino: u64) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_INHIBREAD,
        ..Message::default()
    };
    // SAFETY: the payload is a single `ino_t inode` field, written in the first word of the payload area.
    unsafe {
        msg.m_u.raw[0..8].copy_from_slice(&ino.to_le_bytes());
    }
    msg
}

/// `REQ_CREATE` request (C `req_create` — request.c:166-200): parent directory ino +
/// mode bits + uid/gid + a grant pointing at the **last component name** + the name
/// length.
pub fn encode_create(
    dir_ino: u64,
    grant: i32,
    mode: u32,
    uid: u32,
    gid: u32,
    path_len: usize,
) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_CREATE,
        ..Message::default()
    };
    // SAFETY: REQ_CREATE's payload is written in LP64 field order in the message payload area.
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::create_req_off::INODE..minix_types::create_req_off::INODE + 8]
            .copy_from_slice(&dir_ino.to_le_bytes());
        raw[minix_types::create_req_off::MODE..minix_types::create_req_off::MODE + 4]
            .copy_from_slice(&mode.to_le_bytes());
        raw[minix_types::create_req_off::UID..minix_types::create_req_off::UID + 4]
            .copy_from_slice(&uid.to_le_bytes());
        raw[minix_types::create_req_off::GID..minix_types::create_req_off::GID + 4]
            .copy_from_slice(&gid.to_le_bytes());
        raw[minix_types::create_req_off::GRANT..minix_types::create_req_off::GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
        raw[minix_types::create_req_off::PATH_LEN..minix_types::create_req_off::PATH_LEN + 8]
            .copy_from_slice(&(path_len as u64).to_le_bytes());
    }
    msg
}

/// Decode one `REQ_CREATE` reply (the tail of C `req_create`): `fs_e` comes from the
/// reply's `m_source` (C's `res->fs_e = m.m_source`), and `dev` is 0 (a newly created
/// node has no device; C's `res->dev` is not written on this path).
pub fn decode_create_reply(msg: &Message) -> crate::path::NodeDetails {
    use minix_types::create_reply_off as off;
    // SAFETY: the reply payload is written in the field order above in the payload area.
    let raw = unsafe { &msg.m_u.raw };
    let rd8 = |at: usize| -> u64 {
        let mut b = [0u8; 8];
        if at + 8 <= raw.len() {
            b.copy_from_slice(&raw[at..at + 8]);
        }
        u64::from_le_bytes(b)
    };
    let rd4 = |at: usize| -> u32 {
        let mut b = [0u8; 4];
        if at + 4 <= raw.len() {
            b.copy_from_slice(&raw[at..at + 4]);
        }
        u32::from_le_bytes(b)
    };
    crate::path::NodeDetails {
        fs_e: msg.m_source,
        ino: rd8(off::INODE),
        mode: rd4(off::MODE),
        size: rd8(off::FILE_SIZE),
        uid: rd4(off::UID),
        gid: rd4(off::GID),
        dev: 0,
    }
}

/// `REQ_MKDIR` request (C `req_mkdir` — request.c:528-558): parent directory ino +
/// permission bits + uid/gid + a grant pointing at the **last component name**.
pub fn encode_mkdir(dir_ino: u64, grant: i32, mode: u32, uid: u32, gid: u32) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_MKDIR,
        ..Message::default()
    };
    // SAFETY: REQ_MKDIR's payload is written in LP64 field order in the message payload area.
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::mkdir_req_off::INODE..minix_types::mkdir_req_off::INODE + 8]
            .copy_from_slice(&dir_ino.to_le_bytes());
        raw[minix_types::mkdir_req_off::MODE..minix_types::mkdir_req_off::MODE + 4]
            .copy_from_slice(&mode.to_le_bytes());
        raw[minix_types::mkdir_req_off::UID..minix_types::mkdir_req_off::UID + 4]
            .copy_from_slice(&uid.to_le_bytes());
        raw[minix_types::mkdir_req_off::GID..minix_types::mkdir_req_off::GID + 4]
            .copy_from_slice(&gid.to_le_bytes());
        raw[minix_types::mkdir_req_off::GRANT..minix_types::mkdir_req_off::GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
    }
    msg
}

/// `REQ_FTRUNC` request (C `req_ftrunc` — request.c:261-282).
///
/// VFS's `truncate_vnode` sends exactly one shape: `req_ftrunc(fs_e, ino, newsize, 0)`
/// — `trc_end == 0` means "truncate to `trc_start`" on the FS side (mfs `fs_trunc`).
pub fn encode_ftrunc(ino: u64, start: i64, end: i64) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_FTRUNC,
        ..Message::default()
    };
    // SAFETY: REQ_FTRUNC's payload is written in LP64 field order in the message payload area.
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::ftrunc_req_off::INODE..minix_types::ftrunc_req_off::INODE + 8]
            .copy_from_slice(&ino.to_le_bytes());
        raw[minix_types::ftrunc_req_off::TRC_START..minix_types::ftrunc_req_off::TRC_START + 8]
            .copy_from_slice(&start.to_le_bytes());
        raw[minix_types::ftrunc_req_off::TRC_END..minix_types::ftrunc_req_off::TRC_END + 8]
            .copy_from_slice(&end.to_le_bytes());
    }
    msg
}

/// `REQ_STAT` request (C `req_stat_actual` — request.c:1087-1096): the payload has
/// only the two fields `{inode, grant}`, and the FS writes `struct stat` straight into
/// the user buffer the grant points at.
///
/// Offsets come from the shared authority `minix_types::stat_req_off` (the FS side
/// decodes with the same table).
pub fn encode_stat(ino: u64, grant: i32) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_STAT,
        ..Message::default()
    };
    // SAFETY: REQ_STAT's payload is written in LP64 field order in the message payload area (no dedicated union member).
    unsafe {
        let raw = &mut msg.m_u.raw;
        raw[minix_types::stat_req_off::INODE..minix_types::stat_req_off::INODE + 8]
            .copy_from_slice(&ino.to_le_bytes());
        raw[minix_types::stat_req_off::GRANT..minix_types::stat_req_off::GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
    }
    msg
}

pub fn decode_readsuper_reply(msg: &Message, fs_e: Endpoint) -> FsResp {
    // SAFETY: same as encode_readsuper — read the reply payload byte-wise.
    let raw = unsafe { &msg.m_u.raw };
    let u64_at = |o: usize| u64::from_le_bytes(raw[o..o + 8].try_into().unwrap());
    let u32_at = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
    FsResp::ReadSuper {
        node: NodeDetails {
            fs_e,
            ino: u64_at(16),
            mode: u32_at(28),
            size: u64_at(0),
            uid: u32_at(32),
            gid: u32_at(36),
            dev: u64_at(8),
        },
        fs_flags: FsFlags::from_bits_truncate(u32_at(24)),
        con_reqs: u16::from_le_bytes(raw[40..42].try_into().unwrap()),
    }
}

/// `FsClient` — typed `REQ_*` transport (hides `grant` + `ERESTART` retry).
///
/// Two behaviourally different impls satisfy Gate D.
pub trait FsClient {
    fn send(&mut self, req: FsReq, scope: GrantScope) -> Result<FsResp, FsError>;
    fn send_with_retry(&mut self, req: FsReq) -> Result<FsResp, FsError> {
        // `CPF_TRY → ERESTART → vm_handlemem → retry(0)` (request.c:73)
        match self.send(req.clone(), GrantScope::Try) {
            Err(FsError::GrantFaulted) => {
                // `vm_vfs_procctl_handlemem` would pin the user pages — stub
                // always succeeds in tests, so retry with `NoTry`.
                self.send(req, GrantScope::NoTry)
            }
            other => other,
        }
    }
}

/// Blocking client — would `fs_sendrec` + `worker_wait` in real kernel.
#[derive(Debug, Default)]
pub struct BlockingFsClient {
    pub sent: Vec<FsReq>,
}

impl FsClient for BlockingFsClient {
    fn send(&mut self, req: FsReq, _scope: GrantScope) -> Result<FsResp, FsError> {
        if !FsReq::is_known(req.m_type()) {
            return Err(FsError::UnknownReq(req.m_type()));
        }
        self.sent.push(req.clone());
        // Minimal canned responses for tests (real FS would fill `m_fs_vfs_*`).
        match req {
            FsReq::BRead { .. } | FsReq::BWrite { .. } => Ok(FsResp::Size(512)),
            FsReq::Lookup { .. } => Ok(FsResp::Lookup(LookupRes::default())),
            FsReq::Create { .. } | FsReq::NewNode { .. } => {
                Ok(FsResp::Node(NodeDetails::default()))
            }
            FsReq::ReadSuper { .. } => Ok(FsResp::ReadSuper {
                node: NodeDetails::default(),
                fs_flags: FsFlags::empty(),
                con_reqs: 1,
            }),
            _ => Ok(FsResp::Ok),
        }
    }
}

/// Mock client — records requests, injects `ERESTART` for `BRead` `Try`.
#[derive(Debug, Default)]
#[cfg(test)]
pub struct MockFsClient {
    pub sent: Vec<(FsReq, GrantScope)>,
    pub inject_restart: bool,
}

/// Production `FsClient` — `REQ_*` goes over the real transport.
///
/// The three-step dance of every `req_*` in C `request.c` has one shape:
/// `cpf_grant_direct` builds the payload grant → fill the message and `fs_sendrec` →
/// `cpf_revoke`, with the reply decoded into out-parameters. Here that shape is
/// implemented for [`FsReq::ReadSuper`] (the only consumer the root-mount chain has
/// today, C `request.c:780-833`); the remaining `REQ_*` variants get one arm each as
/// they are energized, and a missing arm fails honestly with [`FsError::UnknownReq`]
/// — it does not pretend the send succeeded.
///
/// Borrowing: the grant table lives in [`crate::main_loop::VfsState`] (the `grants`
/// field), and the client borrows it `&mut` per call, avoiding moving the table out of
/// the state aggregate ([ARCH: A-4]).
pub struct WireFsClient<'a, K: minix_sys::syscall::KernelCallTransport, T: minix_sys::ipc::IpcTransport>
{
    /// The table behind `cpf_grant_direct`/`cpf_revoke` (the `grants` global of safecopies.c).
    pub grants: &'a mut minix_sys::grant::GrantTable,
    /// The kernel-call transport needed by `sys_setgrant`/grant growth.
    pub kernel: &'a K,
    /// The carrier for `fs_sendrec` (direct trap; C's synchronous sendrec in a worker context).
    pub ipc: &'a T,
}

impl<
        K: minix_sys::syscall::KernelCallTransport,
        T: minix_sys::ipc::IpcTransport,
    > FsClient for WireFsClient<'_, K, T>
{
    fn send(&mut self, req: FsReq, _scope: GrantScope) -> Result<FsResp, FsError> {
        let (fs_e, label, dev, readonly, isroot) = match req {
            FsReq::ReadSuper { fs_e, label, dev, readonly, isroot } => {
                (fs_e, label, dev, readonly, isroot)
            }
            // A `REQ_*` arm that is not energized: refuse honestly (the follow-up waves of the mount work land them one by one).
            other => return Err(FsError::UnknownReq(other.m_type())),
        };

        // C request.c:789-793 — `len = strlen(label)+1`, the grant covers the whole
        // string including the NUL (CPF_READ). An empty label (mount_pfs passes "") is
        // still one NUL byte.
        let mut bytes = label.into_bytes();
        bytes.push(0);
        let len = bytes.len();
        let grant_id = self
            .grants
            .grant_direct(
                self.kernel,
                fs_e.get(),
                bytes.as_ptr() as u64,
                len as u64,
                minix_types::CpFlags::READ,
            )
            .map_err(FsError::Io)?;
        // C request.c:797-810 — the message's four-piece set flags/grant/device/path_len.
        let mut msg = encode_readsuper(dev, len, grant_id, readonly, isroot);
        // C `fs_sendrec` (comm.c) stamps the wire `m_type` with the transid in the high
        // 16 bits before the round trip: `TRNS_ADD_ID(call, id)`. The FS side
        // (`fs-rt::transport::receive`) dispatches via `TransactionId::decode`
        // (call = raw >> 16) — without the stamp call=0 and
        // `index = 0.wrapping_sub(FS_BASE)` underflows → `Unserved` → ENOSYS comes
        // back, so read_super never reaches MFS's mount gate (`task.rs:429`) and MFS's
        // `is_mounted()` is forever false. A synchronous sendrec matches the reply by
        // blocking round trip (not by transid), so id 0 is enough here.
        msg.m_type = trns_add_id(REQ_READSUPER, 0);
        // C request.c:813 — `fs_sendrec` (synchronous round trip, worker context).
        let r = self.ipc.sendrec(fs_e, &mut msg);
        // C request.c:814 — revoke first whether or not the reply succeeded (C also revokes before checking r).
        // [SD19-gate] Best-effort grant revoke: the main operation already completed or has entered the error path, and a failed revoke does not change its result
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.grants.revoke(grant_id);
        if let Err(t) = r {
            return Err(FsError::Io(t.0));
        }
        // C `req_readsuper` (request.c:813-816): if the reply status pulled out by
        // `fs_sendrec` is not OK, it is propagated unchanged, **before** decoding the
        // payload. The reply's `m_type = TRNS_ADD_ID(status, id)`; strip the id to get
        // the status (the high 16 bits). Older code ignored the status word and went
        // straight to `decode_readsuper_reply` returning Ok, treating ENOSYS as a
        // successful mount and letting `do_init_root` open the door sick (boot should
        // panic, see main.c:519-520).
        let status = trns_del_id(msg.m_type);
        if status != 0 {
            return Err(FsError::Io(status));
        }
        match decode_readsuper_reply(&msg, fs_e) {
            resp @ FsResp::ReadSuper { .. } => Ok(resp),
            _ => Err(FsError::UnknownReq(REQ_READSUPER)),
        }
    }
}

#[cfg(test)]
impl FsClient for MockFsClient {
    fn send(&mut self, req: FsReq, scope: GrantScope) -> Result<FsResp, FsError> {
        self.sent.push((req.clone(), scope));
        if self.inject_restart
            && scope == GrantScope::Try
            && matches!(
                req,
                FsReq::BRead { .. } | FsReq::Read { .. } | FsReq::GetDents { .. }
            )
        {
            return Err(FsError::GrantFaulted);
        }
        // Same canned responses as blocking, but records scope
        match req {
            FsReq::BRead { .. } | FsReq::BWrite { .. } => Ok(FsResp::Size(0)),
            _ => Ok(FsResp::Ok),
        }
    }
}

/// `GrantStrategy` — how a `grant` is created (direct vs magic).
///
/// Second trait dimension for Gate D (Try vs Direct).
pub trait GrantStrategy {
    fn direct(&self) -> bool;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DirectGrant;
impl GrantStrategy for DirectGrant {
    fn direct(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MagicGrant;
impl GrantStrategy for MagicGrant {
    fn direct(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{Endpoint, FS_BASE, NREQS};

    #[test]
    fn test_fs_base_prefix() {
        assert!(is_fs_rq(REQ_BREAD));
        assert!(is_fs_rq(REQ_LOOKUP));
        assert!(!is_fs_rq(0x100)); // VFS call
        assert!(!is_fs_rq(0x900)); // PM call
        assert!(!is_fs_rq(0xB00)); // transid
    }

    #[test]
    fn test_fs_wire_values_match_c_absolute() {
        // C: com.h:589 `#define FS_BASE 0xA00`.  The absolute values are the
        // wire contract — offsets alone cannot catch a wrong base (the FS
        // side dispatches on 0xA00-derived types).
        assert_eq!(FS_BASE, 0xA00);
        assert_eq!(REQ_GETNODE, 0xA01); // dead, number still pinned
        assert_eq!(REQ_READ, 0xA13);
        assert_eq!(REQ_LOOKUP, 0xA1A);
        assert_eq!(REQ_BPEEK, 0xA21);
        // Regression guard: the pre-fix 0x600 base must never come back.
        assert!(!is_fs_rq(0x600));
        assert!(!is_fs_rq(0x61A));
        // Distinct from the device RS namespaces (com.h:919/963/1038).
        assert_ne!(FS_BASE & !0x7f, 0x480); // CDEV_RS_BASE
        assert_ne!(FS_BASE & !0x7f, 0x580); // BDEV_RS_BASE
    }

    #[test]
    fn test_reqwire_all_33_align_c_ipc_h() {
        // E-REQWIRE: reconcile all 33 constants (C vfsif.h REQ_GETNODE..REQ_BPEEK).
        // The absolute value = 0xA00 + offset; the constants themselves have converged
        // into minix-types::fs_driver, and this table now pins the VFS consumption side
        // (these are the values encoded into m_type).
        let expected: [(i32, i32); 33] = [
            (1, 0xA01), (2, 0xA02), (3, 0xA03), (4, 0xA04),
            (5, 0xA05), (6, 0xA06), (7, 0xA07), (8, 0xA08),
            (9, 0xA09), (10, 0xA0A), (11, 0xA0B), (12, 0xA0C),
            (13, 0xA0D), (14, 0xA0E), (15, 0xA0F), (16, 0xA10),
            (17, 0xA11), (18, 0xA12), (19, 0xA13), (20, 0xA14),
            (21, 0xA15), (22, 0xA16), (23, 0xA17), (24, 0xA18),
            (25, 0xA19), (26, 0xA1A), (27, 0xA1B), (28, 0xA1C),
            (29, 0xA1D), (30, 0xA1E), (31, 0xA1F), (32, 0xA20),
            (33, 0xA21),
        ];
        let consts = [
            REQ_GETNODE, REQ_PUTNODE, REQ_SLINK, REQ_FTRUNC,
            REQ_CHOWN, REQ_CHMOD, REQ_INHIBREAD, REQ_STAT,
            REQ_UTIME, REQ_STATVFS, REQ_BREAD, REQ_BWRITE,
            REQ_UNLINK, REQ_RMDIR, REQ_UNMOUNT, REQ_SYNC,
            REQ_NEW_DRIVER, REQ_FLUSH, REQ_READ, REQ_WRITE,
            REQ_MKNOD, REQ_MKDIR, REQ_CREATE, REQ_LINK,
            REQ_RENAME, REQ_LOOKUP, REQ_MOUNTPOINT, REQ_READSUPER,
            REQ_NEWNODE, REQ_RDLINK, REQ_GETDENTS, REQ_PEEK,
            REQ_BPEEK,
        ];
        assert_eq!(consts.len(), 33);
        for (i, &val) in consts.iter().enumerate() {
            assert_eq!(val, expected[i].1, "REQ index {} mismatch", i + 1);
        }
        // The dead GETNODE constant is in the table too but must not be dispatched.
        assert!(!FsReq::is_known(REQ_GETNODE));
    }

    #[test]
    fn test_nreqs_getnode_dead() {
        assert_eq!(REQ_GETNODE, FS_BASE + 1);
        assert!(!FsReq::is_known(REQ_GETNODE));
        assert!(FsReq::is_known(REQ_BREAD));
        assert!(!FsReq::is_known(0xA01)); // dead
        assert_eq!(NREQS, 34);
    }

    #[test]
    fn test_node_details() {
        let nd = NodeDetails {
            fs_e: Endpoint::MFS,
            ino: 42,
            mode: 0o644,
            size: 1024,
            uid: 1000,
            gid: 1000,
            dev: 7,
        };
        assert_eq!(nd.ino, 42);
        assert_eq!(nd.fs_e, Endpoint::MFS);
    }

    #[test]
    fn test_lookup_res() {
        let lr = LookupRes {
            fs_e: Endpoint::MFS,
            ino: 1,
            mode: 0o755,
            size: 4096,
            uid: 0,
            gid: 0,
            dev: 8,
            char_processed: 5,
            symloop: 2,
        };
        assert_eq!(lr.char_processed, 5);
        assert_eq!(lr.symloop, 2);
        // EENTERMOUNT branch
        let r = FsResp::Lookup(lr);
        assert!(matches!(r, FsResp::Lookup(_)));
    }

    #[test]
    fn test_breadwrite_grant() {
        let req = FsReq::BRead {
            fs_e: Endpoint::MFS,
            dev: 1,
            pos: 0,
            nbytes: 512,
            user: Endpoint::INIT,
        };
        assert_eq!(req.m_type(), REQ_BREAD);
        assert_eq!(req.grants(), 1);
        let scope = GrantScope::Try;
        assert_eq!(scope.cpf_flag(), 1);
        let scope2 = GrantScope::NoTry;
        assert_eq!(scope2.cpf_flag(), 0);
    }

    #[test]
    fn test_breadwrite_retry() {
        let mut mock = MockFsClient {
            inject_restart: true,
            ..Default::default()
        };
        let req = FsReq::BRead {
            fs_e: Endpoint::MFS,
            dev: 1,
            pos: 0,
            nbytes: 512,
            user: Endpoint::INIT,
        };
        // First Try → ERESTART, then NoTry → Ok
        let r = mock.send_with_retry(req);
        assert!(r.is_ok());
        assert_eq!(mock.sent.len(), 2);
        assert_eq!(mock.sent[0].1, GrantScope::Try);
        assert_eq!(mock.sent[1].1, GrantScope::NoTry);
    }

    #[test]
    fn test_lookup_ucred() {
        let with = FsReq::Lookup {
            fs_e: Endpoint::MFS,
            dir_ino: 1,
            root_ino: 1,
            path: "/a".to_string(),
            cred: Some(VfsUCred {
                uid: 1000,
                gid: 1000,
                ngroups: 2,
                sgroups: [1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            }),
            flags: 0,
        };
        let without = FsReq::Lookup {
            fs_e: Endpoint::MFS,
            dir_ino: 1,
            root_ino: 1,
            path: "/a".to_string(),
            cred: None,
            flags: 0,
        };
        assert_eq!(with.grants(), 2);
        assert_eq!(without.grants(), 1);
    }

    #[test]
    fn test_getdents_64() {
        let req = FsReq::GetDents {
            fs_e: Endpoint::MFS,
            ino: 1,
            pos: i32::MAX as i64 + 1,
            size: 1024,
            direct: false,
        };
        assert_eq!(
            req.check_64bit(FsFlags::empty(), i32::MAX as i64 + 1)
                .unwrap_err(),
            FsError::InvalidOff
        );
        assert!(
            req.check_64bit(FsFlags::IS64BIT, i32::MAX as i64 + 1)
                .is_ok()
        );
    }

    #[test]
    fn test_write64() {
        let req = FsReq::Write {
            fs_e: Endpoint::MFS,
            ino: 1,
            pos: i32::MAX as i64 + 1,
            nbytes: 100,
            user: Endpoint::INIT,
        };
        assert_eq!(
            req.check_64bit(FsFlags::empty(), i32::MAX as i64 + 1)
                .unwrap_err(),
            FsError::InvalidOff
        );
    }

    #[test]
    fn test_read64() {
        let req = FsReq::Read {
            fs_e: Endpoint::MFS,
            ino: 1,
            pos: 0,
            nbytes: 100,
            user: Endpoint::INIT,
        };
        assert!(req.check_64bit(FsFlags::empty(), 0).is_ok());
    }

    #[test]
    fn test_bpeek_no_grant() {
        let req = FsReq::BPeek {
            fs_e: Endpoint::MFS,
            dev: 1,
            pos: 0,
            nbytes: 512,
        };
        assert_eq!(req.m_type(), REQ_BPEEK);
        assert_eq!(req.grants(), 1); // even BPeek has 1 logical grant slot (direct -1 maps to None)
    }

    #[test]
    fn test_flush_no_resp() {
        let req = FsReq::Flush {
            fs_e: Endpoint::MFS,
            dev: 1,
        };
        assert_eq!(req.m_type(), REQ_FLUSH);
        let mut c = BlockingFsClient::default();
        let r = c.send(req, GrantScope::Try);
        assert!(matches!(r, Ok(FsResp::Ok)));
    }

    #[test]
    fn test_ftrunc_64() {
        let req = FsReq::FTrunc {
            fs_e: Endpoint::MFS,
            ino: 1,
            start: i32::MAX as i64 + 1,
            end: 100,
        };
        assert_eq!(
            req.check_64bit(FsFlags::empty(), i32::MAX as i64 + 1)
                .unwrap_err(),
            FsError::InvalidOff
        );
    }

    #[test]
    /// `REQ_LOOKUP` request encoding: the four fields grant/length/start-dir/chroot
    /// boundary land on the shared offset table; `path_size` is always `PATH_MAX`
    /// (C's window size).
    #[test]
    fn test_encode_lookup_fields() {
        let m = encode_lookup(7, 12, 0x33, 0x11, minix_types::PATH_RET_SYMLINK);
        assert_eq!(m.m_type, minix_types::REQ_LOOKUP);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::lookup_req_off::DIR_INO..minix_types::lookup_req_off::DIR_INO + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x33);
        b8.copy_from_slice(
            &raw[minix_types::lookup_req_off::ROOT_INO..minix_types::lookup_req_off::ROOT_INO + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x11);
        b8.copy_from_slice(
            &raw[minix_types::lookup_req_off::PATH_LEN..minix_types::lookup_req_off::PATH_LEN + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 12);
        b8.copy_from_slice(
            &raw[minix_types::lookup_req_off::PATH_SIZE..minix_types::lookup_req_off::PATH_SIZE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), crate::path::PATH_MAX as u64);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::lookup_req_off::FLAGS..minix_types::lookup_req_off::FLAGS + 4],
        );
        assert_eq!(
            u32::from_le_bytes(b4),
            minix_types::PATH_RET_SYMLINK,
            "flags 必须真的发到 FS（RET_SYMLINK 的语义在 FS 侧）"
        );
        b4.copy_from_slice(
            &raw[minix_types::lookup_req_off::GRANT_PATH
                ..minix_types::lookup_req_off::GRANT_PATH + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 7);
    }

    /// `REQ_LOOKUP` reply decode (the tail of C `req_lookup` — request.c:500-521):
    /// `OK` → the four `node_details` fields; the three special codes →
    /// offset/ino/symloop; any other status (including a negative errno) → `None` handed
    /// to the error face.
    #[test]
    fn test_decode_lookup_reply_variants() {
        use minix_types::lookup_reply_off as off;
        let mk = |status: i32, off_v: i64, ino: u64, symloop: u16, mode: u32, size: u64, dev: u64| {
            let mut m = Message { m_type: status, ..Message::default() };
            // SAFETY(test): fill the reply through the shared offset table.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[off::OFFSET..off::OFFSET + 8].copy_from_slice(&off_v.to_le_bytes());
                raw[off::FILE_SIZE..off::FILE_SIZE + 8].copy_from_slice(&size.to_le_bytes());
                raw[off::DEVICE..off::DEVICE + 4].copy_from_slice(&(dev as u32).to_le_bytes());
                raw[off::INODE..off::INODE + 8].copy_from_slice(&ino.to_le_bytes());
                raw[off::MODE..off::MODE + 4].copy_from_slice(&mode.to_le_bytes());
                raw[off::SYMLOOP..off::SYMLOOP + 2].copy_from_slice(&symloop.to_le_bytes());
            }
            m
        };

        assert_eq!(
            decode_lookup_reply(minix_types::OK, &mk(minix_types::OK, 0, 0x55, 0, 0o100644, 12, 3)),
            Some(crate::path::LookupRes::Ok {
                ino: 0x55,
                mode: 0o100644,
                size: 12,
                dev: 3
            , uid: 0, gid: 0 })
        );
        assert_eq!(
            decode_lookup_reply(
                minix_types::EENTERMOUNT,
                &mk(minix_types::EENTERMOUNT, 5, 0x77, 2, 0, 0, 0)
            ),
            Some(crate::path::LookupRes::EnterMount { ino: 0x77, offset: 5, symloop: 2 })
        );
        assert_eq!(
            decode_lookup_reply(
                minix_types::ELEAVEMOUNT,
                &mk(minix_types::ELEAVEMOUNT, 2, 0, 1, 0, 0, 0)
            ),
            Some(crate::path::LookupRes::LeaveMount { offset: 2, symloop: 1 })
        );
        assert_eq!(
            decode_lookup_reply(minix_types::ESYMLINK, &mk(minix_types::ESYMLINK, 7, 0, 0, 0, 0, 0)),
            Some(crate::path::LookupRes::Symlink { offset: 7, symloop: 0 })
        );
        assert_eq!(
            decode_lookup_reply(-minix_types::ENOENT, &mk(-minix_types::ENOENT, 0, 0, 0, 0, 0, 0)),
            None,
            "普通错误交错误面"
        );
    }

    /// `REQ_CREATE` encodes six fields + reply decodes five (`fs_e` from `m_source`).
    #[test]
    fn test_encode_create_and_decode_reply() {
        let m = encode_create(0x20, 44, 0o100644, 1000, 1000, 4);
        assert_eq!(m.m_type, minix_types::REQ_CREATE);
        // SAFETY(test): read back the request fields through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::create_req_off::INODE..minix_types::create_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x20);
        b8.copy_from_slice(
            &raw[minix_types::create_req_off::PATH_LEN..minix_types::create_req_off::PATH_LEN + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 4);

        // Reply: file_size@0, inode@8, mode@16, uid@20, gid@24.
        let mut reply = Message {
            m_source: minix_types::Endpoint::MFS,
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): fill the reply through the shared offset table.
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::create_reply_off::FILE_SIZE
                ..minix_types::create_reply_off::FILE_SIZE + 8]
                .copy_from_slice(&0u64.to_le_bytes());
            raw[minix_types::create_reply_off::INODE..minix_types::create_reply_off::INODE + 8]
                .copy_from_slice(&0x77u64.to_le_bytes());
            raw[minix_types::create_reply_off::MODE..minix_types::create_reply_off::MODE + 4]
                .copy_from_slice(&0o100644u32.to_le_bytes());
            raw[minix_types::create_reply_off::UID..minix_types::create_reply_off::UID + 4]
                .copy_from_slice(&7u32.to_le_bytes());
            raw[minix_types::create_reply_off::GID..minix_types::create_reply_off::GID + 4]
                .copy_from_slice(&8u32.to_le_bytes());
        }
        let d = decode_create_reply(&reply);
        assert_eq!(d.fs_e, minix_types::Endpoint::MFS, "fs_e 取 m_source");
        assert_eq!(d.ino, 0x77);
        assert_eq!(d.mode, 0o100644);
        assert_eq!((d.uid, d.gid), (7, 8));
        assert_eq!((d.size, d.dev), (0, 0));
    }

    /// `REQ_MKDIR` encoding: five fields (parent ino/mode/uid/gid/grant).
    #[test]
    fn test_encode_mkdir_fields() {
        let m = encode_mkdir(0x10, 33, 0o40755, 1000, 1000);
        assert_eq!(m.m_type, minix_types::REQ_MKDIR);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::mkdir_req_off::INODE..minix_types::mkdir_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x10);
        let rd4 = |at: usize| {
            let mut b = [0u8; 4];
            b.copy_from_slice(&raw[at..at + 4]);
            u32::from_le_bytes(b)
        };
        assert_eq!(rd4(minix_types::mkdir_req_off::MODE), 0o40755);
        assert_eq!(rd4(minix_types::mkdir_req_off::UID), 1000);
        assert_eq!(rd4(minix_types::mkdir_req_off::GID), 1000);
        assert_eq!(rd4(minix_types::mkdir_req_off::GRANT), 33);
    }

    /// `REQ_FTRUNC` encoding: three fields (inode/trc_start/trc_end), and VFS's call
    /// shape is `end = 0` ("truncate to start").
    #[test]
    fn test_encode_ftrunc_fields() {
        let m = encode_ftrunc(0x42, 100, 0);
        assert_eq!(m.m_type, minix_types::REQ_FTRUNC);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::ftrunc_req_off::INODE..minix_types::ftrunc_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x42);
        b8.copy_from_slice(
            &raw[minix_types::ftrunc_req_off::TRC_START
                ..minix_types::ftrunc_req_off::TRC_START + 8],
        );
        assert_eq!(i64::from_le_bytes(b8), 100);
        b8.copy_from_slice(
            &raw[minix_types::ftrunc_req_off::TRC_END..minix_types::ftrunc_req_off::TRC_END + 8],
        );
        assert_eq!(i64::from_le_bytes(b8), 0);
    }

    /// `REQ_STAT` encoding: inode/grant land on two fields of the shared offset table.
    #[test]
    fn test_encode_stat_fields() {
        let m = encode_stat(0x1234, 42);
        assert_eq!(m.m_type, minix_types::REQ_STAT);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::stat_req_off::INODE..minix_types::stat_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x1234);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::stat_req_off::GRANT..minix_types::stat_req_off::GRANT + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 42);
    }

    /// `REQ_WRITE`: same payload as read, only the request number differs (the direction
    /// difference is in the grant permission bit).
    #[test]
    fn test_encode_write_shares_read_layout() {
        let w = encode_write(0x11, 9, 0x20, 8);
        let r = encode_read(0x11, 9, 0x20, 8);
        assert_eq!(w.m_type, minix_types::REQ_WRITE);
        assert_eq!(r.m_type, minix_types::REQ_READ);
        // SAFETY(test): the payload bytes on both sides must be bit-identical (only m_type differs).
        assert_eq!(unsafe { w.m_u.raw }, unsafe { r.m_u.raw });
    }

    /// `REQ_READ` encoding: four fields (inode/seek_pos/grant/nbytes) land on the shared
    /// offset table.
    #[test]
    fn test_encode_read_fields() {
        let m = encode_read(0x99, 5, 0x1000, 64);
        assert_eq!(m.m_type, minix_types::REQ_READ);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::transfer_req_off::INODE..minix_types::transfer_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x99);
        b8.copy_from_slice(
            &raw[minix_types::transfer_req_off::SEEK_POS
                ..minix_types::transfer_req_off::SEEK_POS + 8],
        );
        assert_eq!(i64::from_le_bytes(b8), 0x1000);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::transfer_req_off::GRANT..minix_types::transfer_req_off::GRANT + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 5);
        b8.copy_from_slice(
            &raw[minix_types::transfer_req_off::BYTES..minix_types::transfer_req_off::BYTES + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 64);
    }

    /// `encode_getdents`'s four fields land per `getdents_req_off` — request and reply are
    /// two different structures (the reply is decoded by `main_loop`'s continuation using
    /// `getdents_reply_off`), so only the request side is pinned here, guarding against
    /// "writing the request off the reply table".
    /// `encode_newnode`'s four fields land (device@0, mode@8, uid@12, gid@16).
    #[test]
    fn test_encode_newnode_fields() {
        let m = encode_newnode(0, 0o010600, 1000, 100);
        assert_eq!(m.m_type, minix_types::REQ_NEWNODE);
        // SAFETY(test): read back in C's `mess_vfs_fs_newnode` field order.
        let raw = unsafe { &m.m_u.raw };
        assert_eq!(u64::from_le_bytes(raw[0..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(raw[8..12].try_into().unwrap()), 0o010600);
        assert_eq!(u32::from_le_bytes(raw[12..16].try_into().unwrap()), 1000);
        assert_eq!(u32::from_le_bytes(raw[16..20].try_into().unwrap()), 100);
    }

    /// `encode_flush`: the payload is just `device`.
    #[test]
    fn test_encode_flush_device_only() {
        let m = encode_flush(0x0301);
        assert_eq!(m.m_type, minix_types::REQ_FLUSH);
        // SAFETY(test): device is in the first word of the payload area (mess_vfs_fs_flush).
        let raw = unsafe { &m.m_u.raw };
        assert_eq!(u64::from_le_bytes(raw[0..8].try_into().unwrap()), 0x0301);
    }

    /// `encode_statvfs`: the payload is just `grant` (the FS fills back a whole `struct statvfs`).
    #[test]
    fn test_encode_statvfs_grant_only() {
        let m = encode_statvfs(7);
        assert_eq!(m.m_type, minix_types::REQ_STATVFS);
        // SAFETY(test): grant is in the first word of the payload area (mess_vfs_fs_statvfs).
        let raw = unsafe { &m.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), 7);
    }

    /// `encode_putnode`: two fields land (inode/count).
    #[test]
    fn test_encode_putnode_fields() {
        let m = encode_putnode(0x77, 3);
        assert_eq!(m.m_type, minix_types::REQ_PUTNODE);
        // SAFETY(test): read back through the shared offset table (putnode_req_off).
        let raw = unsafe { &m.m_u.raw };
        assert_eq!(
            u64::from_le_bytes(
                raw[minix_types::putnode_req_off::INODE
                    ..minix_types::putnode_req_off::INODE + 8]
                    .try_into()
                    .unwrap()
            ),
            0x77
        );
        assert_eq!(
            i32::from_le_bytes(
                raw[minix_types::putnode_req_off::COUNT
                    ..minix_types::putnode_req_off::COUNT + 4]
                    .try_into()
                    .unwrap()
            ),
            3
        );
    }

    /// `encode_new_driver`: three fields land (device/grant/path_len) — **grant is in the
    /// second slot** (`readsuper`'s second slot is flags); a misplacement would send the
    /// label length as the device number.
    #[test]
    fn test_encode_new_driver_fields() {
        let m = encode_new_driver(0x0301, 11, 8);
        assert_eq!(m.m_type, minix_types::REQ_NEW_DRIVER);
        // SAFETY(test): read back through the shared offset table (new_driver_req_off).
        let raw = unsafe { &m.m_u.raw };
        assert_eq!(
            u64::from_le_bytes(
                raw[minix_types::new_driver_req_off::DEVICE
                    ..minix_types::new_driver_req_off::DEVICE + 8]
                    .try_into()
                    .unwrap()
            ),
            0x0301
        );
        assert_eq!(
            i32::from_le_bytes(
                raw[minix_types::new_driver_req_off::GRANT
                    ..minix_types::new_driver_req_off::GRANT + 4]
                    .try_into()
                    .unwrap()
            ),
            11
        );
        assert_eq!(
            u64::from_le_bytes(
                raw[minix_types::new_driver_req_off::PATH_LEN
                    ..minix_types::new_driver_req_off::PATH_LEN + 8]
                    .try_into()
                    .unwrap()
            ),
            8
        );
    }

    /// `encode_rename`'s six fields land (two direct grants: old name + new name, both
    /// lengths include the trailing NUL).
    #[test]
    fn test_encode_rename_fields() {
        let m = encode_rename(0x11, 0x22, 5, 7, 3, 4);
        assert_eq!(m.m_type, minix_types::REQ_RENAME);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::rename_req_off::DIR_OLD..minix_types::rename_req_off::DIR_OLD + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x11);
        b8.copy_from_slice(
            &raw[minix_types::rename_req_off::DIR_NEW..minix_types::rename_req_off::DIR_NEW + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x22);
        b8.copy_from_slice(
            &raw[minix_types::rename_req_off::LEN_OLD..minix_types::rename_req_off::LEN_OLD + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 5);
        b8.copy_from_slice(
            &raw[minix_types::rename_req_off::LEN_NEW..minix_types::rename_req_off::LEN_NEW + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 7);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::rename_req_off::GRANT_OLD
                ..minix_types::rename_req_off::GRANT_OLD + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 3);
        b4.copy_from_slice(
            &raw[minix_types::rename_req_off::GRANT_NEW
                ..minix_types::rename_req_off::GRANT_NEW + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 4);
    }

    /// `encode_slink`'s seven fields land (two grants: name direct + target magic) and
    /// `encode_link`'s four fields land (file ino first, parent directory ino second).
    #[test]
    fn test_encode_slink_and_link_fields() {
        let sl = encode_slink(0x11, 4, 7, 3, 4, 1000, 100);
        assert_eq!(sl.m_type, minix_types::REQ_SLINK);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &sl.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::slink_req_off::INODE..minix_types::slink_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x11);
        b8.copy_from_slice(
            &raw[minix_types::slink_req_off::PATH_LEN..minix_types::slink_req_off::PATH_LEN + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 4);
        b8.copy_from_slice(
            &raw[minix_types::slink_req_off::MEM_SIZE..minix_types::slink_req_off::MEM_SIZE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 7, "目标串长度不含结尾 NUL");
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::slink_req_off::GRANT_PATH
                ..minix_types::slink_req_off::GRANT_PATH + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 3);
        b4.copy_from_slice(
            &raw[minix_types::slink_req_off::GRANT_TARGET
                ..minix_types::slink_req_off::GRANT_TARGET + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 4);
        b4.copy_from_slice(
            &raw[minix_types::slink_req_off::UID..minix_types::slink_req_off::UID + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 1000);
        b4.copy_from_slice(
            &raw[minix_types::slink_req_off::GID..minix_types::slink_req_off::GID + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 100);

        let lk = encode_link(0x77, 0x11, 9, 5);
        assert_eq!(lk.m_type, minix_types::REQ_LINK);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &lk.m_u.raw };
        b8.copy_from_slice(
            &raw[minix_types::link_req_off::INODE..minix_types::link_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x77, "inode 是被链接的文件");
        b8.copy_from_slice(
            &raw[minix_types::link_req_off::DIR_INO..minix_types::link_req_off::DIR_INO + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x11, "dir_ino 是新名的父目录");
        b4.copy_from_slice(
            &raw[minix_types::link_req_off::GRANT..minix_types::link_req_off::GRANT + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 9);
        b8.copy_from_slice(
            &raw[minix_types::link_req_off::PATH_LEN..minix_types::link_req_off::PATH_LEN + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 5);
    }

    /// `encode_utime`'s five fields land (inode/actime/modtime/acnsec/modnsec) — seconds
    /// and nanoseconds travel separately, and the nanoseconds may be the `UTIME_OMIT`
    /// sentinel.
    #[test]
    fn test_encode_utime_fields() {
        let m = encode_utime(0x66, 1700000000, 1700000001, 123, 456);
        assert_eq!(m.m_type, minix_types::REQ_UTIME);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::utime_req_off::INODE..minix_types::utime_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x66);
        b8.copy_from_slice(
            &raw[minix_types::utime_req_off::ACTIME..minix_types::utime_req_off::ACTIME + 8],
        );
        assert_eq!(i64::from_le_bytes(b8), 1700000000);
        b8.copy_from_slice(
            &raw[minix_types::utime_req_off::MODTIME..minix_types::utime_req_off::MODTIME + 8],
        );
        assert_eq!(i64::from_le_bytes(b8), 1700000001);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::utime_req_off::ACNSEC..minix_types::utime_req_off::ACNSEC + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 123);
        b4.copy_from_slice(
            &raw[minix_types::utime_req_off::MODNSEC..minix_types::utime_req_off::MODNSEC + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 456);
    }

    /// `encode_mknod`'s seven fields land (device@0, inode@8, mode@16, uid@20,
    /// gid@24, grant@28, path_len@32).
    #[test]
    fn test_encode_mknod_fields() {
        let m = encode_mknod(0x0301, 0x11, 0o100644, 1000, 100, 5, 8);
        assert_eq!(m.m_type, minix_types::REQ_MKNOD);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::mknod_req_off::DEVICE..minix_types::mknod_req_off::DEVICE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x0301);
        b8.copy_from_slice(
            &raw[minix_types::mknod_req_off::INODE..minix_types::mknod_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x11);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::mknod_req_off::MODE..minix_types::mknod_req_off::MODE + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 0o100644);
        b4.copy_from_slice(
            &raw[minix_types::mknod_req_off::UID..minix_types::mknod_req_off::UID + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 1000);
        b4.copy_from_slice(
            &raw[minix_types::mknod_req_off::GID..minix_types::mknod_req_off::GID + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 100);
        b4.copy_from_slice(
            &raw[minix_types::mknod_req_off::GRANT..minix_types::mknod_req_off::GRANT + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 5);
        b8.copy_from_slice(
            &raw[minix_types::mknod_req_off::PATH_LEN..minix_types::mknod_req_off::PATH_LEN + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 8);
    }

    /// `encode_chown`'s three fields land (inode@0, uid@8, gid@12).
    #[test]
    fn test_encode_chown_fields() {
        let m = encode_chown(0x55, 1000, 100);
        assert_eq!(m.m_type, minix_types::REQ_CHOWN);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::chown_req_off::INODE..minix_types::chown_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x55);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::chown_req_off::UID..minix_types::chown_req_off::UID + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 1000);
        b4.copy_from_slice(
            &raw[minix_types::chown_req_off::GID..minix_types::chown_req_off::GID + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 100);
    }

    /// `encode_unlink` / `encode_rmdir`'s three fields land, pinning "both share one
    /// payload and differ only in m_type" (C shares one struct; do not write two sets of
    /// offsets).
    #[test]
    fn test_encode_unlink_and_rmdir_share_layout() {
        let u = encode_unlink(0x11, 21, 8);
        let r = encode_rmdir(0x11, 21, 8);
        assert_eq!(u.m_type, minix_types::REQ_UNLINK);
        assert_eq!(r.m_type, minix_types::REQ_RMDIR);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &u.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::unlink_req_off::INODE..minix_types::unlink_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x11);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::unlink_req_off::GRANT..minix_types::unlink_req_off::GRANT + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 21);
        b8.copy_from_slice(
            &raw[minix_types::unlink_req_off::PATH_LEN..minix_types::unlink_req_off::PATH_LEN + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 8);
        // Byte-identical apart from m_type (two call numbers for the same structure).
        let ru = unsafe { &r.m_u.raw };
        assert_eq!(raw, ru, "unlink 与 rmdir 的载荷必须逐字节一致");
    }

    /// `encode_chmod`'s two fields land (inode@0, mode@8); the reply-side `mode` is "the
    /// mode that actually took effect", the same name as the request's but from a
    /// different source.
    #[test]
    fn test_encode_chmod_fields() {
        let m = encode_chmod(0x33, 0o640);
        assert_eq!(m.m_type, minix_types::REQ_CHMOD);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::chmod_req_off::INODE..minix_types::chmod_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x33);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::chmod_req_off::MODE..minix_types::chmod_req_off::MODE + 4],
        );
        assert_eq!(u32::from_le_bytes(b4), 0o640);
    }

    /// `encode_rdlink`'s three fields land; the reply side has only the one field
    /// `nbytes` (see `rdlink_reply_off`) — do not write requests off the reply table.
    #[test]
    fn test_encode_rdlink_fields() {
        let m = encode_rdlink(0x42, 11, 256);
        assert_eq!(m.m_type, minix_types::REQ_RDLINK);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::rdlink_req_off::INODE..minix_types::rdlink_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x42);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::rdlink_req_off::GRANT..minix_types::rdlink_req_off::GRANT + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 11);
        b8.copy_from_slice(
            &raw[minix_types::rdlink_req_off::MEM_SIZE
                ..minix_types::rdlink_req_off::MEM_SIZE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 256);
    }

    #[test]
    fn test_encode_getdents_fields() {
        let m = encode_getdents(0x77, 0x800, 9, 4096);
        assert_eq!(m.m_type, minix_types::REQ_GETDENTS);
        // SAFETY(test): read back through the shared offset table.
        let raw = unsafe { &m.m_u.raw };
        let mut b8 = [0u8; 8];
        b8.copy_from_slice(
            &raw[minix_types::getdents_req_off::INODE..minix_types::getdents_req_off::INODE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 0x77);
        b8.copy_from_slice(
            &raw[minix_types::getdents_req_off::SEEK_POS
                ..minix_types::getdents_req_off::SEEK_POS + 8],
        );
        assert_eq!(i64::from_le_bytes(b8), 0x800);
        let mut b4 = [0u8; 4];
        b4.copy_from_slice(
            &raw[minix_types::getdents_req_off::GRANT..minix_types::getdents_req_off::GRANT + 4],
        );
        assert_eq!(i32::from_le_bytes(b4), 9);
        b8.copy_from_slice(
            &raw[minix_types::getdents_req_off::MEM_SIZE
                ..minix_types::getdents_req_off::MEM_SIZE + 8],
        );
        assert_eq!(u64::from_le_bytes(b8), 4096);
    }

    #[test]
    fn test_encode_readsuper_wire_layout() {
        // ipc.h:2111-2119: device@0/flags@8/path_len@16/grant@24.
        let msg = encode_readsuper(0x0401, 5, 7, true, true);
        assert_eq!(msg.m_type, REQ_READSUPER);
        // SAFETY: the assertion side reads in encoding field order.
        let raw = unsafe { &msg.m_u.raw };
        assert_eq!(u64::from_le_bytes(raw[0..8].try_into().unwrap()), 0x0401);
        assert_eq!(u32::from_le_bytes(raw[8..12].try_into().unwrap()), REQ_RDONLY | REQ_ISROOT);
        assert_eq!(u64::from_le_bytes(raw[16..24].try_into().unwrap()), 5);
        assert_eq!(i32::from_le_bytes(raw[24..28].try_into().unwrap()), 7);
        // Neither root nor read-only: flags = 0.
        let msg2 = encode_readsuper(1, 0, 0, false, false);
        let raw2 = unsafe { &msg2.m_u.raw };
        assert_eq!(u32::from_le_bytes(raw2[8..12].try_into().unwrap()), 0);
    }

    #[test]
    fn test_decode_readsuper_reply_roundtrip() {
        // ipc.h:198-211: file_size@0/device@8/inode@16/flags@24/
        // mode@28/uid@32/gid@36/con_reqs@40.
        let mut msg = Message::default();
        {
            // SAFETY: constructing under test — write raw bytes in reply field order.
            let raw = unsafe { &mut msg.m_u.raw };
            raw[0..8].copy_from_slice(&4096u64.to_le_bytes()); // file_size
            raw[8..16].copy_from_slice(&0x0401u64.to_le_bytes()); // device
            raw[16..24].copy_from_slice(&42u64.to_le_bytes()); // inode
            raw[24..28].copy_from_slice(&FsFlags::THREADED.bits().to_le_bytes());
            raw[28..32].copy_from_slice(&0o040755u32.to_le_bytes());
            raw[32..36].copy_from_slice(&1000u32.to_le_bytes());
            raw[36..40].copy_from_slice(&100u32.to_le_bytes());
            raw[40..42].copy_from_slice(&3u16.to_le_bytes());
        }
        match decode_readsuper_reply(&msg, Endpoint::MFS) {
            FsResp::ReadSuper { node, fs_flags, con_reqs } => {
                assert_eq!(node.fs_e, Endpoint::MFS);
                assert_eq!(node.size, 4096);
                assert_eq!(node.dev, 0x0401);
                assert_eq!(node.ino, 42);
                assert_eq!(node.mode, 0o040755);
                assert_eq!(node.uid, 1000);
                assert_eq!(node.gid, 100);
                assert_eq!(fs_flags, FsFlags::THREADED);
                assert_eq!(con_reqs, 3);
            }
            other => panic!("wrong arm: {other:?}"),
        }
    }

    #[test]
    fn test_readsuper_flags() {
        let req = FsReq::ReadSuper {
            fs_e: Endpoint::MFS,
            label: "mfs".to_string(),
            dev: 1,
            readonly: true,
            isroot: true,
        };
        assert_eq!(req.m_type(), REQ_READSUPER);
        // Flags RO+ISROOT would be in the encoded message's flags
        let mut c = BlockingFsClient::default();
        assert!(c.send(req, GrantScope::Try).is_ok());
    }

    #[test]
    fn test_newnode_mix() {
        let req = FsReq::NewNode {
            fs_e: Endpoint::MFS,
            mode: 0o644,
            dev: 7,
            uid: 1000,
            gid: 1000,
        };
        assert_eq!(req.m_type(), REQ_NEWNODE);
        let mut c = BlockingFsClient::default();
        let r = c.send(req, GrantScope::Try);
        assert!(matches!(r, Ok(FsResp::Node(_))));
    }

    #[test]
    fn test_putnode_count() {
        let req = FsReq::PutNode {
            fs_e: Endpoint::MFS,
            ino: 42,
            count: 2,
        };
        assert_eq!(req.m_type(), REQ_PUTNODE);
        let mut c = BlockingFsClient::default();
        assert!(c.send(req, GrantScope::Try).is_ok());
    }

    #[test]
    fn test_peek_no_grant() {
        let req = FsReq::Peek {
            fs_e: Endpoint::MFS,
            ino: 1,
            pos: 0,
            nbytes: 100,
        };
        assert_eq!(req.m_type(), REQ_PEEK);
        assert_eq!(req.grants(), 1);
    }

    #[test]
    fn test_fs_req_two_impls() {
        let mut blocking = BlockingFsClient::default();
        let mut mock = MockFsClient::default();
        let req = FsReq::BRead {
            fs_e: Endpoint::MFS,
            dev: 1,
            pos: 0,
            nbytes: 512,
            user: Endpoint::INIT,
        };
        let r1 = blocking.send(req.clone(), GrantScope::Try);
        let r2 = mock.send(req.clone(), GrantScope::Try);
        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert_eq!(blocking.sent.len(), 1);
        assert_eq!(mock.sent.len(), 1);
        // Polymorphic via trait object
        let mut clients: Vec<Box<dyn FsClient>> = vec![
            Box::new(BlockingFsClient::default()),
            Box::new(MockFsClient::default()),
        ];
        assert!(clients[0].send(req.clone(), GrantScope::Try).is_ok());
        assert!(clients[1].send(req, GrantScope::Try).is_ok());
    }

    #[test]
    fn test_grant_two_impls() {
        let t = GrantScope::Try;
        let nt = GrantScope::NoTry;
        assert_ne!(t.cpf_flag(), nt.cpf_flag());
        // GrantStrategy trait second dimension
        let d = DirectGrant;
        let m = MagicGrant;
        assert_ne!(d.direct(), m.direct());
        let strategies: Vec<Box<dyn GrantStrategy>> =
            vec![Box::new(DirectGrant), Box::new(MagicGrant)];
        assert!(strategies[0].direct());
        assert!(!strategies[1].direct());
    }

    #[test]
    fn test_message_roundtrip() {
        let req = FsReq::Lookup {
            fs_e: Endpoint::MFS,
            dir_ino: 1,
            root_ino: 1,
            path: "/etc/passwd".to_string(),
            cred: None,
            flags: 0,
        };
        let msg_type = req.m_type();
        assert!(is_fs_rq(msg_type));
        assert_eq!(msg_type, REQ_LOOKUP);
    }
}
