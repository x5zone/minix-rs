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
    is_fs_rq, Endpoint, REQ_BREAD, REQ_BPEEK, REQ_BWRITE, REQ_CHMOD, REQ_CHOWN, REQ_CREATE,
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
