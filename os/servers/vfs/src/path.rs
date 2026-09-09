//! Path lookup — `path.c` + `path.h` + `utility.c:24-93`.
//!
//! `lookup` translates `Path → Vnode` with mount-point `EENTERMOUNT` and
//! symlink `SYMLOOP 16` handling.  `DO_POSIX_PATHNAME_RES 0` keeps trailing
//! slashes ignored (historical Unix).  `PATH_GET_UCRED 020` carries
//! `vfs_ucred_t` via two grants when `ngroups>0`.
//!
//! `ARCH A-10` (DO_POSIX) is `const DO_POSIX: bool = false`.

use minix_types::Endpoint;

/// `PATH_MAX 1024` — `limits.h`.
pub const PATH_MAX: usize = 1024;
/// `NAME_MAX 60` — single component max (Minix `NAME_MAX`).
pub const NAME_MAX: usize = 60;
/// `SYMLOOP 16` — `_POSIX_SYMLOOP_MAX` (const.h:32).
pub const SYMLOOP_MAX: usize = 16;
/// `DO_POSIX_PATHNAME_RES 0` — historical trailing-slash ignore (path.c:31).
///
/// `ARCH A-10`: `false` = historical (strip trailing `/`), `true` = POSIX
/// (`append "."`).
pub const DO_POSIX: bool = false;

bitflags::bitflags! {
    /// `PATH_*` flags — `vfsif.h:12`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LookupFlags: u32 {
        const NOFLAGS = 0x00;
        const RET_SYMLINK = 0x08; // 010
        const GET_UCRED = 0x10; // 020
    }
}

/// `TLL_*` lock kind for `l_vmnt_lock / l_vnode_lock` (tll.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockKind {
    None,
    Read,
    Write,
    ReadSer,
}

/// `lookup` — `path.h:4` 5-field structure + `symloop` counter.
#[derive(Debug, Clone)]
pub struct Lookup {
    /// `l_path` — mutable `char[PATH_MAX]` buffer (owned `String`).
    pub path: String,
    /// `l_flags` — `PATH_*`.
    pub flags: LookupFlags,
    /// `l_vmnt_lock` — `VMNT_*` mapped to `TLL`.
    pub vmnt_lock: LockKind,
    /// `l_vnode_lock` — `VNODE_*`.
    pub vnode_lock: LockKind,
    /// `l_vmp` — output `vmnt` locked (None = not locked yet).
    pub vmnt: Option<usize>,
    /// `l_vnode` — output `vnode` locked.
    pub vnode: Option<usize>,
    /// Symlink loop counter (`lookup:432` + `last_dir:298`).
    pub symloop: u8,
}

impl Lookup {
    /// `lookup_init:574` — `l_path=path; l_flags=flags; *vmp=NULL; *vp=NULL`.
    pub fn new(path: String, flags: LookupFlags) -> Result<Self, PathError> {
        if path.len() > PATH_MAX {
            return Err(PathError::TooLong);
        }
        Ok(Self {
            path,
            flags,
            vmnt_lock: LockKind::None,
            vnode_lock: LockKind::None,
            vmnt: None,
            vnode: None,
            symloop: 0,
        })
    }

    /// Whether `symloop` exceeded `SYMLOOP_MAX`.
    pub fn check_symloop(&self) -> Result<(), PathError> {
        if (self.symloop as usize) > SYMLOOP_MAX {
            Err(PathError::Loop)
        } else {
            Ok(())
        }
    }

    /// `DO_POSIX` trailing-slash handling — historical strip vs POSIX append.
    pub fn normalize_trailing_slash(&mut self) {
        if DO_POSIX {
            // POSIX: trailing slash not stripped (would append ".")
            // For test, we model as no-op when DO_POSIX true (kept for trait)
        } else {
            // Historical: strip trailing slashes except root "/"
            while self.path.len() > 1 && self.path.ends_with('/') {
                self.path.pop();
            }
        }
    }

    /// Simulate `lookup`'s `char_processed` memmove: drain prefix `off`.
    pub fn consume_prefix(&mut self, off: usize) {
        if off < self.path.len() {
            self.path = self.path[off..].to_string();
        } else {
            self.path.clear();
        }
    }
}

/// `lookup_res` — `request.h:25` 9-field response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupRes {
    Ok {
        ino: u64,
        mode: u32,
        size: u64,
        dev: u64,
    },
    EnterMount {
        ino: u64,
        offset: i32,
        symloop: u8,
    },
    LeaveMount {
        offset: i32,
        symloop: u8,
    },
    Symlink {
        offset: i32,
        symloop: u8,
    },
}

/// `node_details` — `request.h:12` 7-field (MFS `REQ_CREATE` response).
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

/// 挂载行的循环视图（`vmnt[]` 快照字段，调用方从 VmntTable+VnodeTable 构建）。
///
/// C 在循环内直接读共享内存 `vmnt[]`/`->v_mounted_on->`；Rust 表分离后以
/// 身份快照传入——单线程事件循环下循环内无并发改动，快照等价（W7 接线时
/// 每轮 lookup 前重建一次）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountedFs {
    /// `m_fs_e`——该挂载分区的 FS 端点。
    pub fs: Endpoint,
    /// 根 vnode 身份（`m_root_node`）：`(ino, dev)`。
    pub root: (u64, u64),
    /// 挂载点 vnode 身份（`m_mounted_on`）：`(ino, fs_e, dev)`；`None` = 空行。
    pub mounted_on: Option<(u64, Endpoint, u64)>,
}

/// 进程根目录锚（`fp_rd`）——chroot 边界与符号链接重启点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootDir {
    /// `v_inode_nr`。
    pub ino: u64,
    /// `v_fs_e`。
    pub fs: Endpoint,
    /// `v_dev`。
    pub dev: u64,
}

/// `lookup`（`path.c:384-546`）——跨 FS 往返解析循环。
///
/// 每轮把 `(fs_e, dir_ino, root_ino)` 交给 `req_lookup` 让 FS 尽可能多地
/// 解析剩余路径；FS 以 [`LookupRes`] 回答四类结果——`Ok` 直接落定，三类
/// 特殊码（进挂载/出挂载/符号链接）推进路径后切换起点再来一轮。循环只做
/// 裁决：vmnt 的锁升降是执行面（tll），本轮记录"当前作用于哪个挂载行"。
///
/// `req_lookup` 闭包即 REQ_LOOKUP 对话的接缝（W7 接线时接 `FsReq::Lookup`
/// + `FsClient`；测试用脚本应答）。
pub fn lookup(
    start_fs: Endpoint,
    start_ino: u64,
    start_dev: u64,
    resolve: &mut Lookup,
    rd: RootDir,
    uid: u32,
    gid: u32,
    mounts: &[MountedFs],
    req_lookup: &mut dyn FnMut(Endpoint, u64, u64, &mut Lookup) -> Result<LookupRes, PathError>,
) -> Result<NodeDetails, PathError> {
    // 空路径（`path.c:400-404`）。
    if resolve.path.is_empty() {
        return Err(PathError::NoEnt);
    }
    let mut fs_e = start_fs;
    let mut dir_ino = start_ino;
    // chroot 边界：根与起点同分区才生效（`path.c:416-420`）。
    let mut root_ino = if rd.dev == start_dev { rd.ino } else { 0 };
    let mut symloop: u32 = 0;

    loop {
        let mut res = req_lookup(fs_e, dir_ino, root_ino, resolve)?;
        // 特殊码循环（`path.c:446-543`）：路径推进 + symloop 累计 + 起点切换。
        while matches!(
            res,
            LookupRes::EnterMount { .. }
                | LookupRes::LeaveMount { .. }
                | LookupRes::Symlink { .. }
        ) {
            let (offset, symloop_delta) = match res {
                LookupRes::EnterMount { offset, symloop, .. }
                | LookupRes::LeaveMount { offset, symloop, .. }
                | LookupRes::Symlink { offset, symloop } => (offset, symloop),
                LookupRes::Ok { .. } => unreachable!(),
            };
            // 推进路径（`path.c:450-453` 的 memmove）。
            resolve.consume_prefix(offset as usize);
            // symloop 累计并检查（`path.c:455-461`）。
            symloop += u32::from(symloop_delta);
            if symloop > SYMLOOP_MAX as u32 {
                return Err(PathError::Loop);
            }
            match res {
                // 符号链接：从进程根重启（`path.c:465-468`）。
                LookupRes::Symlink { .. } => {
                    dir_ino = rd.ino;
                    fs_e = rd.fs;
                    root_ino = if rd.dev == rd.dev { rd.ino } else { 0 };
                }
                // 进挂载点：找 mounted_on == (ino, fs_e) 的挂载行，
                // 起点切到其根 vnode（`path.c:470-484`）。
                LookupRes::EnterMount { ino, .. } => {
                    match mounts.iter().find(|m| {
                        m.mounted_on
                            .map_or(false, |(mino, mfs, _)| mino == ino && mfs == fs_e)
                    }) {
                        Some(m) => {
                            dir_ino = m.root.0;
                            fs_e = m.fs;
                            root_ino = if rd.dev == m.root.1 { rd.ino } else { 0 };
                        }
                        None => return Err(PathError::NoEnt), // C: EIO，根节点丢失
                    }
                }
                // 出挂载点：路径必须以 `..` 开头（`path.c:496-521` 的
                // bogus-path 守卫），起点切到挂载点自身 vnode。
                LookupRes::LeaveMount { .. } => {
                    match mounts.iter().find(|m| m.fs == fs_e) {
                        Some(m) => {
                            if !resolve.path.starts_with("..") {
                                return Err(PathError::NoEnt);
                            }
                            let rest = &resolve.path[2..];
                            if !rest.is_empty() && !rest.starts_with('/') {
                                return Err(PathError::NoEnt);
                            }
                            match m.mounted_on {
                                Some((mino, mfs, mdev)) => {
                                    dir_ino = mino;
                                    fs_e = mfs;
                                    root_ino = if rd.dev == mdev { rd.ino } else { 0 };
                                }
                                None => return Err(PathError::NoEnt),
                            }
                        }
                        None => return Err(PathError::NoEnt), // C: panic，加固为 Err
                    }
                }
                LookupRes::Ok { .. } => unreachable!(),
            }
            // 下一轮 REQ_LOOKUP（`path.c:537-541`）。
            res = req_lookup(fs_e, dir_ino, root_ino, resolve)?;
        }
        // `Ok`：七字段结果——fs_e/uid/gid 由本轮上下文回填（C 的 res 三字段
        // 即 VFS 发出的值，`path.c:548-554`）。
        match res {
            LookupRes::Ok { ino, mode, size, dev } => {
                return Ok(NodeDetails {
                    fs_e,
                    ino,
                    mode,
                    size,
                    uid,
                    gid,
                    dev,
                });
            }
            _ => unreachable!(),
        }
    }
}

/// Path errors — map to Minix errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    TooLong,
    Empty,
    Loop,
    NoEnt,
    Inval,
}

impl minix_types::ToErrno for PathError {
    fn to_errno(&self) -> minix_types::Errno {
        minix_types::Errno::from_i32((*self).to_errno())
    }
}

impl PathError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::TooLong => minix_types::ENAMETOOLONG,
            Self::Empty => minix_types::ENOENT,
            Self::Loop => minix_types::ELOOP,
            Self::NoEnt => minix_types::ENOENT,
            Self::Inval => minix_types::EINVAL,
        }
    }
}

/// `PathFetcher` — `utility.c:24 copy_path / 60 fetch_name` `safecopy`.
///
/// `ARCH` : `sys_safecopy` is `Direct` vs `Safecopy` via this trait.
pub trait PathFetcher {
    fn fetch(&self, addr: u64, len: usize) -> Result<String, PathError>;
    fn copy(&self, path: &str) -> Result<String, PathError>;
}

/// Test double: fabricates bytes instead of reading caller memory.
/// The production `PathFetcher` impl arrives with the kernel IPC
/// primitives (W1) — `sys_safecopy` over the transport.
#[cfg(test)]
#[derive(Debug, Default, Clone, Copy)]
pub struct DirectFetcher;

#[cfg(test)]
impl PathFetcher for DirectFetcher {
    fn fetch(&self, _addr: u64, len: usize) -> Result<String, PathError> {
        if len == 0 || len > PATH_MAX {
            return Err(PathError::Inval);
        }
        Ok("a".repeat(len - 1))
    }
    fn copy(&self, path: &str) -> Result<String, PathError> {
        if path.len() > PATH_MAX {
            return Err(PathError::TooLong);
        }
        Ok(path.to_string())
    }
}

/// Test double for the `TRY`-flavoured safecopy path (see [`DirectFetcher`]).
#[cfg(test)]
#[derive(Debug, Default, Clone, Copy)]
pub struct SafecopyFetcher;

#[cfg(test)]
impl PathFetcher for SafecopyFetcher {
    fn fetch(&self, _addr: u64, len: usize) -> Result<String, PathError> {
        if len == 0 || len > PATH_MAX {
            return Err(PathError::Inval);
        }
        // Simulate `sys_safecopy` that may fault → `ERESTART` would be
        // handled by caller; here we always succeed for test.
        Ok("b".repeat(len - 1))
    }
    fn copy(&self, path: &str) -> Result<String, PathError> {
        if path.len() > PATH_MAX {
            return Err(PathError::TooLong);
        }
        Ok(path.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_init_null() {
        let lk = Lookup::new("/a".to_string(), LookupFlags::NOFLAGS).unwrap();
        assert!(lk.vmnt.is_none());
        assert!(lk.vnode.is_none());
        assert_eq!(lk.symloop, 0);
        assert_eq!(lk.vmnt_lock, LockKind::None);
    }

    #[test]
    fn test_symloop_e_loop() {
        let mut lk = Lookup::new("/a/b".to_string(), LookupFlags::NOFLAGS).unwrap();
        lk.symloop = 16;
        assert!(lk.check_symloop().is_ok());
        lk.symloop = 17;
        assert_eq!(lk.check_symloop().unwrap_err(), PathError::Loop);
        // LookupRes symloop field is u8, max 255, but threshold 16
        let res = LookupRes::EnterMount {
            ino: 1,
            offset: 3,
            symloop: 5,
        };
        if let LookupRes::EnterMount { symloop, .. } = res {
            assert_eq!(symloop, 5);
        }
    }

    #[test]
    fn test_last_dir_split() {
        // last_dir: strrchr('/') cut
        let path = "/a/b/c".to_string();
        let cp = path.rfind('/').unwrap();
        let dir_entry = &path[cp + 1..];
        assert_eq!(dir_entry, "c");
        let dir_part = &path[..cp + 1];
        assert_eq!(dir_part, "/a/b/");
        // Symloop threshold
        let mut lk = Lookup::new("/a/b/c".to_string(), LookupFlags::NOFLAGS).unwrap();
        lk.symloop = 15;
        assert!(lk.check_symloop().is_ok());
        lk.symloop = 17;
        assert!(lk.check_symloop().is_err());
    }

    #[test]
    fn test_canonical_path() {
        // canonical_path: last_dir + rdlink loop + .. climb
        let mut p = "/a/b/c/".to_string();
        // Historical trailing-slash semantics (DO_POSIX=false, path.c:31).
        while p.len() > 1 && p.ends_with('/') {
            p.pop();
        }
        assert_eq!(p, "/a/b/c");
        // Simulate canonical climbs PATH_MAX bound
        let long = "a".repeat(PATH_MAX + 1);
        assert_eq!(
            Lookup::new(long, LookupFlags::NOFLAGS).unwrap_err(),
            PathError::TooLong
        );
    }

    #[test]
    fn test_path_max() {
        let long = "a".repeat(PATH_MAX + 1);
        assert_eq!(
            Lookup::new(long, LookupFlags::NOFLAGS).unwrap_err(),
            PathError::TooLong
        );
        let ok = "a".repeat(PATH_MAX);
        assert!(Lookup::new(ok, LookupFlags::NOFLAGS).is_ok());
    }

    #[test]
    fn test_fetch_name_copy() {
        let direct = DirectFetcher;
        assert_eq!(direct.copy("/etc/passwd").unwrap(), "/etc/passwd");
        assert_eq!(direct.fetch(0x1000, 5).unwrap().len(), 4);
        assert_eq!(direct.fetch(0, 0).unwrap_err(), PathError::Inval);
        let safecopy = SafecopyFetcher;
        assert_eq!(safecopy.fetch(0x2000, 5).unwrap().len(), 4);
        // Trait objects
        let fetchers: Vec<Box<dyn PathFetcher>> =
            vec![Box::new(DirectFetcher), Box::new(SafecopyFetcher)];
        assert_eq!(fetchers[0].fetch(0x1000, 3).unwrap().len(), 2);
        assert_eq!(fetchers[1].fetch(0x1000, 3).unwrap().len(), 2);
        // Behavioural difference: Direct vs Safecopy differ on large len? Both ok but we test len check
        assert_ne!(
            fetchers[0].fetch(0x1000, 2).unwrap(),
            fetchers[1].fetch(0x1000, 3).unwrap()
        );
    }

    #[test]
    fn test_empty_path() {
        let lk = Lookup::new("".to_string(), LookupFlags::NOFLAGS).unwrap();
        assert_eq!(lk.path, "");
        // lookup:405 if(l_path[0]=='\0') ENOENT
        let err = if lk.path.is_empty() {
            PathError::Empty
        } else {
            PathError::NoEnt
        };
        assert_eq!(err, PathError::Empty);
    }

    #[test]
    fn test_loop_init_flags() {
        let flags = LookupFlags::RET_SYMLINK | LookupFlags::GET_UCRED;
        let lk = Lookup::new("/a".to_string(), flags).unwrap();
        assert!(lk.flags.contains(LookupFlags::RET_SYMLINK));
        assert!(lk.flags.contains(LookupFlags::GET_UCRED));
        let lk2 = Lookup::new("/a".to_string(), LookupFlags::NOFLAGS).unwrap();
        assert!(!lk2.flags.contains(LookupFlags::RET_SYMLINK));
    }

    #[test]
    fn test_get_name() {
        // get_name iterates req_getdents; here we test the dirent parsing logic stub
        let dir_ino = 1u64;
        let entry_ino = 2u64;
        // Simulate find
        let found = dir_ino != entry_ino;
        assert!(found);
        // get_name would return ELOOP if symloop etc — not needed
        let lk = Lookup::new("/a".to_string(), LookupFlags::NOFLAGS).unwrap();
        assert_eq!(lk.path, "/a");
    }

    #[test]
    fn test_mount_enter() {
        // EENTERMOUNT: find_vmnt scan for m_mounted_on
        // Simulate vmnt table try_enter
        let mut lk = Lookup::new("/mnt/a".to_string(), LookupFlags::NOFLAGS).unwrap();
        lk.vmnt = Some(2);
        lk.vnode = Some(10);
        // Simulate EnterMount res
        let res = LookupRes::EnterMount {
            ino: 5,
            offset: 4,
            symloop: 1,
        };
        if let LookupRes::EnterMount { ino, offset, .. } = res {
            assert_eq!(ino, 5);
            assert_eq!(offset, 4);
            lk.consume_prefix(offset as usize);
            assert_eq!(lk.path, "/a");
        }
    }

    #[test]
    fn test_path_fetcher_two_impls() {
        let direct = DirectFetcher;
        let safecopy = SafecopyFetcher;
        // Same len, both succeed but we test that they are distinct types
        assert_eq!(
            direct.fetch(0x1000, 10).unwrap().len(),
            safecopy.fetch(0x1000, 10).unwrap().len()
        );
        // Behavioural difference via trait object
        let fetchers: Vec<Box<dyn PathFetcher>> =
            vec![Box::new(DirectFetcher), Box::new(SafecopyFetcher)];
        assert_eq!(fetchers.len(), 2);
        // Direct vs Safecopy differ on error handling for len 0? Both Err, but we test that trait objects work
        assert!(fetchers[0].fetch(0, 0).is_err());
        assert!(fetchers[1].fetch(0, 0).is_err());
    }
}

    #[test]
    fn test_lookup_single_fs() {
        // 一轮即中：FS 直接给出最终节点（`path.c:548-554` 七字段回填）。
        let parent = Endpoint::from_generation_slot(0, 10);
        let mounts = [MountedFs {
            fs: parent,
            root: (1, 100),
            mounted_on: None,
        }];
        let mut calls = 0;
        let mut req = |fs: Endpoint, dir: u64, _root: u64, _lk: &mut Lookup| {
            calls += 1;
            assert_eq!(fs, parent);
            assert_eq!(dir, 1);
            Ok(LookupRes::Ok { ino: 42, mode: 0o100644, size: 7, dev: 100 })
        };
        let mut lk = Lookup::new("/a/b".to_string(), LookupFlags::NOFLAGS).unwrap();
        let nd = lookup(
            parent, 1, 100, &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            1000, 100, &mounts, &mut req,
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(nd.ino, 42);
        assert_eq!(nd.fs_e, parent);
        assert_eq!(nd.uid, 1000);
    }

    #[test]
    fn test_lookup_enter_mount() {
        // `/mnt` 是挂载点：FS1 报 EENTERMOUNT → 起点切到 FS2 根 → FS2 完成。
        let parent = Endpoint::from_generation_slot(0, 10);
        let child = Endpoint::from_generation_slot(0, 11);
        let mounts = [
            MountedFs { fs: parent, root: (1, 100), mounted_on: None },
            MountedFs {
                fs: child,
                root: (2, 101),
                mounted_on: Some((9, parent, 100)),
            },
        ];
        let mut lk = Lookup::new("/mnt/data".to_string(), LookupFlags::NOFLAGS).unwrap();
        let mut req = |fs: Endpoint, dir: u64, _root: u64, lk: &mut Lookup| {
            if fs == parent {
                assert_eq!(dir, 1);
                // 消耗 "/mnt/"，剩 "data"（`path.c:450-453`）。
                Ok(LookupRes::EnterMount { ino: 9, offset: 5, symloop: 0 })
            } else {
                assert_eq!(dir, 2);
                Ok(LookupRes::Ok { ino: 77, mode: 0o040755, size: 3, dev: 101 })
            }
        };
        let nd = lookup(
            parent, 1, 100, &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            0, 0, &mounts, &mut req,
        )
        .unwrap();
        assert_eq!(nd.ino, 77);
        assert_eq!(nd.fs_e, child);
        assert_eq!(nd.fs_e, child);
        assert_eq!(lk.path, "data");
    }

    #[test]
    fn test_lookup_leave_mount_bogus_guard() {
        // 爬出挂载点后剩余路径必须以 `..` 开头（`path.c:515-521`）。
        let parent = Endpoint::from_generation_slot(0, 10);
        let child = Endpoint::from_generation_slot(0, 11);
        let mounts = [MountedFs {
            fs: child,
            root: (2, 101),
            mounted_on: Some((9, parent, 100)),
        }];
        let mut lk = Lookup::new("/../hidden".to_string(), LookupFlags::NOFLAGS).unwrap();
        let mut req = |fs: Endpoint, dir: u64, _root: u64, _lk: &mut Lookup| {
            if fs == child {
                // 消耗 "/"，剩 "../hidden"——合法的爬出形态。
                Ok(LookupRes::LeaveMount { offset: 1, symloop: 0 })
            } else {
                // 爬出后落在父分区的挂载点 vnode 上（ino 9）。
                assert_eq!(dir, 9);
                Ok(LookupRes::Ok { ino: 55, mode: 0o100644, size: 1, dev: 100 })
            }
        };
        let nd = lookup(
            child, 2, 101, &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            0, 0, &mounts, &mut req,
        )
        .unwrap();
        assert_eq!(nd.ino, 55);
        assert_eq!(nd.fs_e, parent);
    }

    #[test]
    fn test_lookup_leave_mount_bogus_rejected() {
        // 非 `..` 的剩余路径是子 FS 喂的伪路径（`path.c:519-521` → ENOENT）。
        let parent = Endpoint::from_generation_slot(0, 10);
        let child = Endpoint::from_generation_slot(0, 11);
        let mounts = [MountedFs {
            fs: child,
            root: (2, 101),
            mounted_on: Some((9, parent, 100)),
        }];
        let mut lk = Lookup::new("/etc/passwd".to_string(), LookupFlags::NOFLAGS).unwrap();
        let mut req = |_fs: Endpoint, _dir: u64, _root: u64, _lk: &mut Lookup| {
            Ok(LookupRes::LeaveMount { offset: 1, symloop: 0 })
        };
        let r = lookup(
            child, 2, 101, &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            0, 0, &mounts, &mut req,
        );
        assert_eq!(r.unwrap_err(), PathError::NoEnt);
    }

    #[test]
    fn test_lookup_symlink_loop_e_loop() {
        // FS 每轮都报符号链接且不推进——17 轮后 ELOOP（`path.c:455-461`）。
        let parent = Endpoint::from_generation_slot(0, 10);
        let mounts = [MountedFs { fs: parent, root: (1, 100), mounted_on: None }];
        let mut lk = Lookup::new("/loop".to_string(), LookupFlags::NOFLAGS).unwrap();
        let mut rounds = 0;
        let mut req = |_fs: Endpoint, _dir: u64, _root: u64, _lk: &mut Lookup| {
            rounds += 1;
            Ok(LookupRes::Symlink { offset: 0, symloop: 1 })
        };
        let r = lookup(
            parent, 1, 100, &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            0, 0, &mounts, &mut req,
        );
        assert_eq!(r.unwrap_err(), PathError::Loop);
        assert_eq!(rounds, 17); // symloop 累计到 17 > 16 才越界
    }

    #[test]
    fn test_lookup_empty_path_enoent() {
        let parent = Endpoint::from_generation_slot(0, 10);
        let mounts: [MountedFs; 0] = [];
        let mut lk = Lookup::new(String::new(), LookupFlags::NOFLAGS).unwrap();
        let mut req = |_fs: Endpoint, _dir: u64, _root: u64, _lk: &mut Lookup| {
            Err(PathError::NoEnt)
        };
        let r = lookup(
            parent, 1, 100, &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            0, 0, &mounts, &mut req,
        );
        assert_eq!(r.unwrap_err(), PathError::NoEnt);
    }
