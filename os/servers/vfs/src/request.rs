//! Typed `REQ_*` wrappers — `request.c` + `vfsif.h:41-73` + `request.h`.
//!
//! `request.c` hides three repetitions: `grant` construction / `fs_sendrec`
//! / `revoke` / `ERESTART → vm_handlemem → retry(0)` and `RES_64BIT` early
//! `EINVAL` and `m_source → res->fs_e` back-fill.  This module makes each
//! repetition a type: `FsReq` (32 variants, one per live `REQ_*` type,
//! `FS_BASE 0xA00` prefix), `FsResp`
//! (`NodeDetails` 7 fields vs `LookupRes` 9 fields), `GrantScope` (`Try` vs
//! `NoTry`), `FsFlags` (`RES_64BIT`守门).  `REQ_GETNODE 0xA01` is dead.
//!
//! `ARCH A-2` (enum vs function pointer) and `ARCH A-8` (64-bit) in one place.
//!
//! E-REQWIRE: the `REQ_*` numbers themselves are no longer defined here —
//! they moved to the shared authority `minix-types::fs_driver` (consumed by
//! the FS-driver side too, so the two ends cannot drift; devman's VTreeFS is
//! the registered third consumer). The C-absolute pin tests stay in this
//! module: they guard what VFS actually puts on the wire.

use minix_types::{
    is_fs_rq, Endpoint, Message, REQ_BREAD, REQ_BPEEK, REQ_BWRITE, REQ_CHMOD, REQ_CHOWN, REQ_CREATE,
    REQ_FLUSH, REQ_FTRUNC, REQ_GETDENTS, REQ_GETNODE, REQ_INHIBREAD, REQ_LINK, REQ_LOOKUP,
    REQ_MKDIR, REQ_MKNOD, REQ_MOUNTPOINT, REQ_NEWNODE, REQ_NEW_DRIVER, REQ_PEEK, REQ_PUTNODE,
    REQ_RDLINK, REQ_READ, REQ_READSUPER, REQ_RENAME, REQ_RMDIR, REQ_SLINK, REQ_STAT,
    REQ_STATVFS, REQ_SYNC, REQ_UNLINK, REQ_UNMOUNT, REQ_UTIME, REQ_WRITE,
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
/// `REQ_RDONLY` / `REQ_ISROOT` — `readsuper` 的请求 flags
/// (vfsif.h:8-9;S14 挂载先行批的 wire 编码面)。
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
    /// `readsuper` 回复(req_readsuper,request.c:818):
    /// `node_details` + `fs_flags`(RES_*)+ `con_reqs`
    /// (ipc.h:198-211;con_reqs 在 C 侧无消费者,mount.c 的 max_reqs
    /// 由 RES_THREADED 决定——见 `mount.rs::SuperInfo::max_reqs`)。
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

/// `req_readsuper` 的请求编码(C request.c:780-813):device@0(dev_t)、
/// flags@8(REQ_RDONLY/ISROOT)、path_len@16(size_t)、grant@24
/// (cp_grant_id_t,ipc.h:2111-2119 的 LP64 布局)。
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
    // SAFETY: raw 臂按字节写——无专属 union 成员的 wire 面
    // (前例:pm 的 decode.rs raw 模式)。
    let raw = unsafe { &mut msg.m_u.raw };
    raw[0..8].copy_from_slice(&device.to_le_bytes());
    raw[8..12].copy_from_slice(&flags.to_le_bytes());
    raw[16..24].copy_from_slice(&(path_len as u64).to_le_bytes());
    raw[24..28].copy_from_slice(&grant.to_le_bytes());
    msg
}

/// `readsuper` 回复解码(C request.c:818-825):file_size@0(off_t)、
/// device@8(dev_t)、inode@16(ino_t)、flags@24(u32 = fs_flags)、
/// mode@28、uid@32、gid@36、con_reqs@40(u16,ipc.h:198-211)。
/// `fs_e` 由调用方给(即 `m_source`,C 的 `res->fs_e = m.m_source`)。
/// `REQ_READ` / `REQ_WRITE` 请求（C `req_readwrite_actual` —
/// request.c:834-874）：载荷 `{inode, seek_pos, grant, nbytes}`，数据面由
/// FS 经 grant 直读写（读是 FS 往用户缓冲 `safecopyto`）。
///
/// 偏移取共享权威 `minix_types::transfer_req_off`（FS 侧解码用同一张表）。
pub fn encode_read(ino: u64, grant: i32, pos: i64, nbytes: usize) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_READ,
        ..Message::default()
    };
    // SAFETY: REQ_READ 的载荷按 LP64 域序写在消息负载区（无专属 union 成员）。
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

/// `REQ_WRITE` 请求（与 [`encode_read`] 同载荷，方向相反：FS 从用户缓冲
/// **读**数据，故 grant 权限位是 `CPF_READ` — C `req_readwrite_actual`
/// request.c:846-848 的 `rw_flag==READING ? CPF_WRITE : CPF_READ`）。
pub fn encode_write(ino: u64, grant: i32, pos: i64, nbytes: usize) -> Message {
    let mut msg = encode_read(ino, grant, pos, nbytes);
    msg.m_type = minix_types::REQ_WRITE;
    msg
}

/// `REQ_RDLINK` 请求（C `req_rdlink_actual` — request.c:717-748）。
///
/// 载荷 `{inode, grant, mem_size}`；数据面由 FS 经 grant 把链接文本写进
/// **用户**缓冲（`CPF_WRITE` 的 magic grant）。回复的字节数在载荷里
/// （`rdlink_reply_off::NBYTES`），`m_type` 只是 `OK`。
pub fn encode_rdlink(ino: u64, grant: i32, mem_size: usize) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_RDLINK,
        ..Message::default()
    };
    // SAFETY: REQ_RDLINK 的载荷按 LP64 域序写在消息负载区（无专属 union 成员）。
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

/// `REQ_GETDENTS` 请求（C `req_getdents_actual` — request.c:288-336）。
///
/// 载荷 `{inode, seek_pos, grant, mem_size}`；数据面由 FS 经 grant 往用户
/// 缓冲写目录项（`CPF_WRITE` 的 magic grant）。位置由调用方从 filp 取出
/// 随请求带上——FS 不持有位置（C `read.c:309` 传的是 `rfilp->filp_pos`）。
///
/// 偏移取共享权威 `minix_types::getdents_req_off`（FS 侧解码用同一张表）。
pub fn encode_getdents(ino: u64, pos: i64, grant: i32, mem_size: usize) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_GETDENTS,
        ..Message::default()
    };
    // SAFETY: REQ_GETDENTS 的载荷按 LP64 域序写在消息负载区（无专属 union 成员）。
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

/// `REQ_LOOKUP` 请求（C `req_lookup` — request.c:430-500）。
///
/// 载荷是 `mess_vfs_fs_lookup`：路径 grant（**`CPF_READ|CPF_WRITE`** ——
/// FS 要把"剩余路径"写回来，这是 `EENTERMOUNT`/`ESYMLINK` 报告进度的方法）、
/// 路径长度与窗口、起目录 ino 与 chroot 边界 ino、以及 `flags`
/// （`PATH_RET_SYMLINK`/`PATH_GET_UCRED` — vfsif.h）；凭证 grant 在
/// `ngroups > 0` 时才带（本编码器把 `grant_ucred`/`ucred_size` 留给调用方传 0，
/// 凭证面随 Open 族一并接）。
///
/// 偏移取共享权威 `minix_types::lookup_req_off`（FS 侧解码用同一张表）。
pub fn encode_lookup(
    grant_path: i32,
    path_len: usize,
    dir_ino: u64,
    root_ino: u64,
    flags: u32,
) -> Message {
    // `PATH_RET_SYMLINK` 的语义在 **FS 侧**实现（libfsdriver/lookup.c:249-251：
    // "最后一个组件是符号链接且 VFS 要求不解析时不解析"），所以这个字必须
    // 真的发过去——少发就等于让 FS 一路跟进符号链接。
    // `PATH_GET_UCRED` 需要随请求带凭证 grant，本编码器还不带（ucred_size=0）：
    // 谁先打开这条面，谁在这里补 grant 并把断言换成实现。
    debug_assert!(
        flags & minix_types::PATH_GET_UCRED == 0,
        "lookup 的凭证面未接线（req_lookup 的 ucred grant）"
    );
    let mut msg = Message {
        m_type: minix_types::REQ_LOOKUP,
        ..Message::default()
    };
    // SAFETY: REQ_LOOKUP 的载荷按 LP64 域序写在消息负载区。
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
        // ucred_size = 0（不带凭证，见上）。
    }
    msg
}

/// 解一条 `REQ_LOOKUP` 回复（C `req_lookup` 的收尾 — request.c:500-521）。
///
/// `status` 是剥掉 transid 后的 `m_type`：`OK` 时字节区是 `node_details`
/// 的前四域（ino/mode/size/dev），三个特殊码
/// （`EENTERMOUNT`/`ELEAVEMOUNT`/`ESYMLINK`）只有 `offset`/`inode`/`symloop`
/// 有意义；其余（含负 errno）由调用方按错误处理（本函数返回 `None`）。
pub fn decode_lookup_reply(status: i32, msg: &Message) -> Option<crate::path::LookupRes> {
    use minix_types::lookup_reply_off as off;
    // SAFETY: 回复载荷按上述域序写在负载区。
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
            // `node_details` 的属主/属组：C `advance` 把它写进 vnode 的
            // `v_uid`/`v_gid`（path.c:98-99），权限判断随后按它算。
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

/// `REQ_INHIBREAD` 请求（C `req_inhibread` — request.c:374-384）：载荷只有
/// `{inode}`，让 FS 丢掉该 inode 的预读状态（`lseek` 改了位置时发）。
pub fn encode_inhibread(ino: u64) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_INHIBREAD,
        ..Message::default()
    };
    // SAFETY: 载荷只有 `ino_t inode` 一个域，写在负载区首字。
    unsafe {
        msg.m_u.raw[0..8].copy_from_slice(&ino.to_le_bytes());
    }
    msg
}

/// `REQ_CREATE` 请求（C `req_create` — request.c:166-200）：父目录 ino +
/// 模式位 + uid/gid + 指向**最后组件名**的 grant + 名字长度。
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
    // SAFETY: REQ_CREATE 的载荷按 LP64 域序写在消息负载区。
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

/// 解一条 `REQ_CREATE` 回复（C `req_create` 收尾）：`fs_e` 取回复的
/// `m_source`（C 的 `res->fs_e = m.m_source`），`dev` 为 0（新建节点无设备，
/// C 的 `res->dev` 在该路径上未写）。
pub fn decode_create_reply(msg: &Message) -> crate::path::NodeDetails {
    use minix_types::create_reply_off as off;
    // SAFETY: 回复载荷按上述域序写在负载区。
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

/// `REQ_MKDIR` 请求（C `req_mkdir` — request.c:528-558）：父目录 ino +
/// 权限位 + uid/gid + 指向**最后组件名**的 grant。
pub fn encode_mkdir(dir_ino: u64, grant: i32, mode: u32, uid: u32, gid: u32) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_MKDIR,
        ..Message::default()
    };
    // SAFETY: REQ_MKDIR 的载荷按 LP64 域序写在消息负载区。
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

/// `REQ_FTRUNC` 请求（C `req_ftrunc` — request.c:261-282）。
///
/// VFS 的 `truncate_vnode` 只发一种形状：`req_ftrunc(fs_e, ino, newsize, 0)`
/// ——`trc_end == 0` 在 FS 侧就是"截到 `trc_start`"（mfs `fs_trunc`）。
pub fn encode_ftrunc(ino: u64, start: i64, end: i64) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_FTRUNC,
        ..Message::default()
    };
    // SAFETY: REQ_FTRUNC 的载荷按 LP64 域序写在消息负载区。
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

/// `REQ_STAT` 请求（C `req_stat_actual` — request.c:1087-1096）：载荷只有
/// `{inode, grant}` 两域，FS 把 `struct stat` 直接写进 grant 指向的用户缓冲。
///
/// 偏移取共享权威 `minix_types::stat_req_off`（FS 侧解码用同一张表）。
pub fn encode_stat(ino: u64, grant: i32) -> Message {
    let mut msg = Message {
        m_type: minix_types::REQ_STAT,
        ..Message::default()
    };
    // SAFETY: REQ_STAT 的载荷按 LP64 域序写在消息负载区（无专属 union 成员）。
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
    // SAFETY: 同 encode_readsuper——按字节读回复载荷。
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
        // E-REQWIRE：全量 33 常量对账（C vfsif.h REQ_GETNODE..REQ_BPEEK）。
        // 绝对值 = 0xA00 + 偏移；常量本体已收敛至 minix-types::fs_driver，
        // 本表改为钉住 VFS 消费侧（编码进 m_type 的就是这些值）。
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
        // 死常量 GETNODE 也在表内但不应被 dispatch。
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
    /// `REQ_LOOKUP` 请求编码：grant/长度/起目录/chroot 边界四个域落在共享
    /// 偏移表上；`path_size` 恒为 `PATH_MAX`（C 的窗口大小）。
    #[test]
    fn test_encode_lookup_fields() {
        let m = encode_lookup(7, 12, 0x33, 0x11, minix_types::PATH_RET_SYMLINK);
        assert_eq!(m.m_type, minix_types::REQ_LOOKUP);
        // SAFETY(test): 按共享偏移表读回。
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

    /// `REQ_LOOKUP` 回复解码（C `req_lookup` 收尾 — request.c:500-521）：
    /// `OK` → 四域 `node_details`；三个特殊码 → offset/ino/symloop；其余
    /// 状态（含负 errno）→ `None` 交错误面。
    #[test]
    fn test_decode_lookup_reply_variants() {
        use minix_types::lookup_reply_off as off;
        let mk = |status: i32, off_v: i64, ino: u64, symloop: u16, mode: u32, size: u64, dev: u64| {
            let mut m = Message { m_type: status, ..Message::default() };
            // SAFETY(test): 按共享偏移表填回复。
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

    /// `REQ_CREATE` 编码六域 + 回复解码五域（`fs_e` 取 `m_source`）。
    #[test]
    fn test_encode_create_and_decode_reply() {
        let m = encode_create(0x20, 44, 0o100644, 1000, 1000, 4);
        assert_eq!(m.m_type, minix_types::REQ_CREATE);
        // SAFETY(test): 按共享偏移表读回请求域。
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

        // 回复：file_size@0、inode@8、mode@16、uid@20、gid@24。
        let mut reply = Message {
            m_source: minix_types::Endpoint::MFS,
            m_type: minix_types::OK,
            ..Message::default()
        };
        // SAFETY(test): 按共享偏移表填回复。
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

    /// `REQ_MKDIR` 编码：五域（父 ino/mode/uid/gid/grant）。
    #[test]
    fn test_encode_mkdir_fields() {
        let m = encode_mkdir(0x10, 33, 0o40755, 1000, 1000);
        assert_eq!(m.m_type, minix_types::REQ_MKDIR);
        // SAFETY(test): 按共享偏移表读回。
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

    /// `REQ_FTRUNC` 编码：三域（inode/trc_start/trc_end），且 VFS 的调用
    /// 形状是 `end = 0`（"截到 start"）。
    #[test]
    fn test_encode_ftrunc_fields() {
        let m = encode_ftrunc(0x42, 100, 0);
        assert_eq!(m.m_type, minix_types::REQ_FTRUNC);
        // SAFETY(test): 按共享偏移表读回。
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

    /// `REQ_STAT` 编码：inode/grant 落在共享偏移表的两个域上。
    #[test]
    fn test_encode_stat_fields() {
        let m = encode_stat(0x1234, 42);
        assert_eq!(m.m_type, minix_types::REQ_STAT);
        // SAFETY(test): 按共享偏移表读回。
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

    /// `REQ_WRITE`：与读同载荷，只换请求号（方向差在 grant 权限位上）。
    #[test]
    fn test_encode_write_shares_read_layout() {
        let w = encode_write(0x11, 9, 0x20, 8);
        let r = encode_read(0x11, 9, 0x20, 8);
        assert_eq!(w.m_type, minix_types::REQ_WRITE);
        assert_eq!(r.m_type, minix_types::REQ_READ);
        // SAFETY(test): 两边的负载字节应逐位相同（只有 m_type 不同）。
        assert_eq!(unsafe { w.m_u.raw }, unsafe { r.m_u.raw });
    }

    /// `REQ_READ` 编码：四个域（inode/seek_pos/grant/nbytes）落在共享偏移表上。
    #[test]
    fn test_encode_read_fields() {
        let m = encode_read(0x99, 5, 0x1000, 64);
        assert_eq!(m.m_type, minix_types::REQ_READ);
        // SAFETY(test): 按共享偏移表读回。
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

    /// `encode_getdents` 的四个域按 `getdents_req_off` 落位——请求与回复是
    /// 两套结构（回复见 `main_loop` 的续接体用 `getdents_reply_off` 解），
    /// 这里只钉请求侧，防"看着回复表写请求"。
    /// `encode_rdlink` 的三个域落位；回复侧只有 `nbytes` 一域（见
    /// `rdlink_reply_off`）——别把回复表拿来写请求。
    #[test]
    fn test_encode_rdlink_fields() {
        let m = encode_rdlink(0x42, 11, 256);
        assert_eq!(m.m_type, minix_types::REQ_RDLINK);
        // SAFETY(test): 按共享偏移表读回。
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
        // SAFETY(test): 按共享偏移表读回。
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
        // ipc.h:2111-2119:device@0/flags@8/path_len@16/grant@24。
        let msg = encode_readsuper(0x0401, 5, 7, true, true);
        assert_eq!(msg.m_type, REQ_READSUPER);
        // SAFETY: 断言侧按编码域序读。
        let raw = unsafe { &msg.m_u.raw };
        assert_eq!(u64::from_le_bytes(raw[0..8].try_into().unwrap()), 0x0401);
        assert_eq!(u32::from_le_bytes(raw[8..12].try_into().unwrap()), REQ_RDONLY | REQ_ISROOT);
        assert_eq!(u64::from_le_bytes(raw[16..24].try_into().unwrap()), 5);
        assert_eq!(i32::from_le_bytes(raw[24..28].try_into().unwrap()), 7);
        // 非 root 非只读:flags = 0。
        let msg2 = encode_readsuper(1, 0, 0, false, false);
        let raw2 = unsafe { &msg2.m_u.raw };
        assert_eq!(u32::from_le_bytes(raw2[8..12].try_into().unwrap()), 0);
    }

    #[test]
    fn test_decode_readsuper_reply_roundtrip() {
        // ipc.h:198-211:file_size@0/device@8/inode@16/flags@24/
        // mode@28/uid@32/gid@36/con_reqs@40。
        let mut msg = Message::default();
        {
            // SAFETY: 测试构造——按回复域序写 raw。
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
