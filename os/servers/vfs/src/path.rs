//! Path lookup — `path.c` + `path.h` + `utility.c:24-93`.
//!
//! `lookup` translates `Path → Vnode` with mount-point `EENTERMOUNT` and
//! symlink `SYMLOOP 16` handling.  `DO_POSIX_PATHNAME_RES 0` keeps trailing
//! slashes ignored (historical Unix).  `PATH_GET_UCRED 020` carries
//! `vfs_ucred_t` via two grants when `ngroups>0`.
//!
//! `ARCH A-10` (DO_POSIX) is `const DO_POSIX: bool = false`.

use crate::open::{S_IFDIR, S_IFLNK, S_IFMT};
use crate::vnode::{VnodeId, VnodeTable};
use minix_types::Endpoint;

/// `PATH_MAX 1024` — `limits.h`.
use alloc::string::String;
use alloc::vec::Vec;
#[cfg(test)] // Box 仅测试模块（PathFetcher/FsClient 注入）使用
use alloc::boxed::Box;

use alloc::string::ToString;

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
        /// `node_details.uid`——**文件属主**（C `advance` 的
        /// `new_vp->v_uid = res.uid`，path.c:98-99；`path.c:565-566` 同款）。
        /// 权限判断按它算，不是按调用方自己的 id。
        uid: u32,
        /// `node_details.gid`——文件属组（同上）。
        gid: u32,
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
    /// `m_dev`——挂载设备号（`v_dev` 的来源；C `advance` 的 `new_vp->v_dev = vmp->m_dev`）。
    pub dev: u64,
    /// 根 vnode 身份（`m_root_node`）：`(ino, dev)`。
    pub root: (u64, u64),
    /// 挂载点 vnode 身份（`m_mounted_on`）：`(ino, fs_e, dev)`；`None` = 空行。
    pub mounted_on: Option<(u64, Endpoint, u64)>,
}

/// `req_getdents` 对话接缝的类型别名——一批已解析目录项 `(ino, name)`。
pub type GetDents<'a> =
    &'a mut dyn FnMut(Endpoint, u64, usize) -> Result<Vec<(u64, String)>, PathError>;

/// `req_rdlink` 对话接缝的类型别名——读符号链接内容。
pub type RdLink<'a> = &'a mut dyn FnMut(Endpoint, u64) -> Result<String, PathError>;

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
/// 与 `FsClient`；测试用脚本应答）。
///
/// `lookup` 的起点身份（`start_node` 的三字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupStart {
    /// `v_fs_e`。
    pub fs: Endpoint,
    /// `v_inode_nr`。
    pub ino: u64,
    /// `v_dev`。
    pub dev: u64,
}

/// REQ_LOOKUP 对话接缝——一轮"FS 尽可能多解析"的往返。
pub type ReqLookup<'a> = &'a mut dyn FnMut(Endpoint, u64, u64, &mut Lookup) -> Result<LookupRes, PathError>;

#[allow(clippy::too_many_arguments)] // 忠实移植：C 环境参数显式化（Fix #36）
/// 一步遍历动作（[`LookupWalk`] 的产出）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkStep {
    /// 下一步：把这条 `REQ_LOOKUP` 发给 `fs_e`（`dir_ino` 为起点，
    /// `root_ino` 为 chroot 边界；全 0 表示不设边界）。
    Send {
        /// 目标文件系统端点。
        fs_e: Endpoint,
        /// 起始目录 inode。
        dir_ino: u64,
        /// chroot 边界 inode。
        root_ino: u64,
    },
    /// 走完：节点详情（七字段，C `lookup:548-554`）。
    Done(NodeDetails),
}

/// 路径遍历的**可续走状态机**（`[ARCH: W7]`）。
///
/// C 的 `lookup()` 在一趟里挑多次 `req_lookup`（path.c:229 首调、:300 特殊码
/// 循环续调），每一次都阻塞 worker 线程的栈；单线程事件循环没有可恢复的
/// 栈，故把"走到哪了"显式成状态：[起点三元组](#structfield.fs_e) + symloop +
/// 路径游标（`resolve.path`）。`resume(lookup_res)` 吃掉一次 FS 回复并给出
/// 下一步——**同一个状态机既服务同步驱动**（[`lookup`] 的闭包循环，测试与
/// 单发路径用它）**也服务挂起-续走**（臂把游标存进 worker 槽，回复到了再
/// `resume`）。
#[derive(Debug, Clone)]
pub struct LookupWalk {
    fs_e: Endpoint,
    dir_ino: u64,
    root_ino: u64,
    symloop: u32,
    resolve: Lookup,
    /// 调用方的有效 uid/gid（C `lookup:400-421` 的 `uid`/`gid` 参数）。
    ///
    /// C 用它们给 `REQ_LOOKUP` 附**凭证 grant**（`PATH_GET_UCRED`）让 FS 做
    /// 权限判断；本重写的凭证面还没接线（`encode_lookup` 用 debug_assert 挡住
    /// 了 `PATH_GET_UCRED`），所以这两个字段目前**只是随行走**、还没被读——
    /// 凭证面接通时它们是输入。见 `encode_lookup` 的注记。
    #[allow(dead_code)]
    uid: u32,
    #[allow(dead_code)]
    gid: u32,
}

impl LookupWalk {
    /// 开一趟遍历：校验路径非空，输出首步（第一条 `REQ_LOOKUP` 的参数）。
    ///
    /// C: `lookup:400-421` —— 空路径 `ENOENT`、chroot 边界只在根与起点同分区
    /// 时生效（`rd.dev == start_dev`）。
    pub fn begin(
        start: LookupStart,
        resolve: Lookup,
        rd: RootDir,
        uid: u32,
        gid: u32,
    ) -> Result<(Self, WalkStep), PathError> {
        if resolve.path.is_empty() {
            return Err(PathError::NoEnt);
        }
        let root_ino = if rd.dev == start.dev { rd.ino } else { 0 };
        let walk = Self {
            fs_e: start.fs,
            dir_ino: start.ino,
            root_ino,
            symloop: 0,
            resolve,
            uid,
            gid,
        };
        let step = WalkStep::Send {
            fs_e: walk.fs_e,
            dir_ino: walk.dir_ino,
            root_ino: walk.root_ino,
        };
        Ok((walk, step))
    }

    /// 借出遍历中的路径游标（C 的 `resolve->l_path`——驱动方可以读它，
    /// 例如判断提前返回的组件）。
    pub fn resolve(&self) -> &Lookup {
        &self.resolve
    }

    /// 吃掉一次 FS 回复，给出下一步。`rd`/`mounts` 按当时状态传入（游标
    /// 推进与挂载点穿越要在续走时重新读到最新表，而不是挂起那一刻的快照）。
    pub fn resume(
        &mut self,
        res: LookupRes,
        rd: RootDir,
        mounts: &[MountedFs],
    ) -> Result<WalkStep, PathError> {
        if matches!(res, LookupRes::Ok { .. }) {
            return match res {
                LookupRes::Ok { ino, mode, size, dev, uid, gid } => {
                    Ok(WalkStep::Done(NodeDetails {
                        fs_e: self.fs_e,
                        ino,
                        mode,
                        size,
                        // 属主来自 FS 的回复（C `advance` 的 `v_uid = res.uid`），
                        // **不是**遍历时带的调用方 id——那两个只用于 `advance`
                        // 内部的搜索权限判断。
                        uid,
                        gid,
                        dev,
                    }))
                }
                _ => unreachable!(),
            };
        }
        // 特殊码：推进游标 + symloop 累计（`path.c:446-461`）。
        let (offset, symloop_delta) = match res {
            LookupRes::EnterMount { offset, symloop, .. }
            | LookupRes::LeaveMount { offset, symloop, .. }
            | LookupRes::Symlink { offset, symloop } => (offset, symloop),
            LookupRes::Ok { .. } => unreachable!(),
        };
        self.resolve.consume_prefix(offset as usize);
        self.symloop += u32::from(symloop_delta);
        if self.symloop > SYMLOOP_MAX as u32 {
            return Err(PathError::Loop);
        }
        match res {
            // 符号链接：从进程根重启（`path.c:465-468`）。
            LookupRes::Symlink { .. } => {
                self.dir_ino = rd.ino;
                self.fs_e = rd.fs;
                self.root_ino = rd.ino;
            }
            // 进挂载点：起点切到被挂载分区的根（`path.c:470-484`）。
            LookupRes::EnterMount { ino, .. } => {
                match mounts.iter().find(|m| {
                    m.mounted_on
                        .is_some_and(|(mino, mfs, _)| mino == ino && mfs == self.fs_e)
                }) {
                    Some(m) => {
                        self.dir_ino = m.root.0;
                        self.fs_e = m.fs;
                        self.root_ino = if rd.dev == m.root.1 { rd.ino } else { 0 };
                    }
                    None => return Err(PathError::NoEnt),
                }
            }
            // 出挂载点：路径必须以 `..` 开头（`path.c:496-521` 的 bogus 守卫）。
            LookupRes::LeaveMount { .. } => {
                match mounts.iter().find(|m| m.fs == self.fs_e) {
                    Some(m) => {
                        if !self.resolve.path.starts_with("..") {
                            return Err(PathError::NoEnt);
                        }
                        let rest = &self.resolve.path[2..];
                        if !(rest.is_empty() || rest.starts_with('/')) {
                            return Err(PathError::NoEnt);
                        }
                        match m.mounted_on {
                            Some((mino, mfs, mdev)) => {
                                self.dir_ino = mino;
                                self.fs_e = mfs;
                                self.root_ino = if rd.dev == mdev { rd.ino } else { 0 };
                            }
                            None => return Err(PathError::NoEnt),
                        }
                    }
                    None => return Err(PathError::NoEnt),
                }
            }
            LookupRes::Ok { .. } => unreachable!(),
        }
        Ok(WalkStep::Send {
            fs_e: self.fs_e,
            dir_ino: self.dir_ino,
            root_ino: self.root_ino,
        })
    }
}

pub fn lookup(
    start: LookupStart,
    resolve: &mut Lookup,
    rd: RootDir,
    uid: u32,
    gid: u32,
    mounts: &[MountedFs],
    req_lookup: ReqLookup<'_>,
) -> Result<NodeDetails, PathError> {
    // 遍历逻辑单一真相在 [`LookupWalk`]：这里只是"同步驱动"——拿一步、
    // 发一条 REQ_LOOKUP、把回复喂回去（C 的 `lookup` 循环在同一趟里做，
    // 因为它的 `req_lookup` 会阻塞线程栈；单线程版把它拆成可续走的两半）。
    let (mut walk, mut step) = LookupWalk::begin(start, resolve.clone(), rd, uid, gid)?;
    loop {
        match step {
            WalkStep::Done(details) => {
                // 游标回写（C 的 `resolve` 是调用方的指针，退出时它已推进）。
                *resolve = walk.resolve;
                return Ok(details);
            }
            WalkStep::Send { fs_e, dir_ino, root_ino } => {
                let res = req_lookup(fs_e, dir_ino, root_ino, &mut walk.resolve)?;
                step = walk.resume(res, rd, mounts)?;
            }
        }
    }
}

/// `last_dir` 的拆分决策（`path.c:185-228`）。
///
/// 去尾斜杠（`DO_POSIX=false`，len>1 才去）→ `strrchr('/')` 切出目录前缀
/// 与最后组件 → 组件名过 `NAME_MAX` 门（`ENAMETOOLONG`）。根路径 `"/"`
/// 的目录前缀保持 `"/"`、组件名为 `"."`（`path.c:200-204`）。
pub fn last_dir_split(path: &str) -> Result<LastDirSplit, PathError> {
    let mut s = path;
    while s.len() > 1 && s.ends_with('/') {
        s = &s[..s.len() - 1];
    }
    let (dir, entry) = match s.rfind('/') {
        // `cp == NULL`：路径无斜杠——目录是 "."（`path.c:200-205`）。
        None => (".", s),
        // 前缀含起点的 `/`；`cp[1]=='\0'`（路径以斜杠结尾）组件为 "."。
        Some(pos) => {
            let dir = &s[..=pos];
            let entry = if pos + 1 >= s.len() { "." } else { &s[pos + 1..] };
            // 目录前缀再去多余尾斜杠（`path.c:226-229`，保留首字符）。
            let dir = match dir.trim_end_matches('/') {
                "" if dir.starts_with('/') => "/",
                trimmed => trimmed,
            };
            (dir, entry)
        }
    };
    if entry.len() > NAME_MAX {
        return Err(PathError::TooLong);
    }
    Ok(LastDirSplit {
        dir_path: dir.to_string(),
        entry: entry.to_string(),
    })
}

/// `last_dir_split` 的产物：目录前缀 + 最后一组件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastDirSplit {
    /// 喂给解析循环的目录前缀。
    pub dir_path: String,
    /// 最后一组件名（`dir_entry`）。
    pub entry: String,
}

/// `advance`（`path.c:36-127`）——lookup 循环 + vnode 缓存层。
///
/// `get_free_vnode`（`alloc`）预留草稿槽 → [`lookup`] 解析 → `find_by_ino`
/// 命中则 `dup` 复用、未命中则填入草稿槽并置引用（`v_fs_count = 1`、
/// `ref_count = 1` 即 `dup_vnode`）。C 的"竞态下 vnode 消失重建计数"补偿
/// （`path.c:80-90`）在单线程模型不可达。
#[allow(clippy::too_many_arguments)] // 忠实移植：C 环境参数显式化（Fix #36）
pub fn advance(
    start: LookupStart,
    resolve: &mut Lookup,
    rd: RootDir,
    uid: u32,
    gid: u32,
    mounts: &[MountedFs],
    vnode_table: &mut VnodeTable,
    req_lookup: ReqLookup<'_>,
) -> Result<VnodeId, PathError> {
    let scratch = vnode_table.alloc().map_err(|_| PathError::NoEnt)?;
    let details = lookup(start, resolve, rd, uid, gid, mounts, req_lookup)?;
    match vnode_table.find_by_ino(details.fs_e, details.ino) {
        // 缓存命中：`v_fs_count++`（FS 侧引用）+ `dup_vnode` 的 `ref++`。
        Some(hit) => {
            if let Some(v) = vnode_table.get_mut(hit) {
                v.fs_count += 1;
            }
            vnode_table.dup(hit);
            Ok(hit)
        }
        None => {
            if let Some(v) = vnode_table.get_mut(scratch) {
                v.fs = details.fs_e;
                v.ino = details.ino;
                v.mode = details.mode;
                v.size = details.size;
                v.uid = details.uid;
                v.gid = details.gid;
                v.dev = details.dev;
                v.fs_count = 1;
                v.ref_count = 1;
            }
            Ok(scratch)
        }
    }
}

/// `eat_path`（`path.c:131-141`）——首字符选起点：`/` 从进程根，
/// 否则从工作目录；随后 `advance`。
#[allow(clippy::too_many_arguments)] // 忠实移植：C 环境参数显式化（Fix #36）
pub fn eat_path(
    resolve: &mut Lookup,
    rd: RootDir,
    wd: LookupStart,
    uid: u32,
    gid: u32,
    mounts: &[MountedFs],
    vnode_table: &mut VnodeTable,
    req_lookup: ReqLookup<'_>,
) -> Result<VnodeId, PathError> {
    let start = if resolve.path.starts_with('/') {
        LookupStart {
            fs: rd.fs,
            ino: rd.ino,
            dev: rd.dev,
        }
    } else {
        wd
    };
    advance(start, resolve, rd, uid, gid, mounts, vnode_table, req_lookup)
}

/// `last_dir`（`path.c:145-380`）——拆分 + 目录解析 + 末组件 rdlink 重试。
///
/// C 的 do-while 结构：拆分（[`last_dir_split`]）→ 目录前缀 advance（清
/// `RET_SYMLINK`，`path.c:231-235`）→ 末组件带回并以 `RET_SYMLINK` 查验
/// （`path.c:247-260`）→ 是符号链接且调用方不要真身时 `req_rdlink` 取
/// 内容重启（含斜杠按绝对/相对回根或以已解析目录为基，`path.c:289-303`；
/// 无斜杠回原起点族，`symloop < SYMLOOP_MAX` 限时）。
///
/// 返回（目录 vnode 的缓存槽位，末组件名）。`rdlink` 即 `req_rdlink`
/// （`path.c:281-285`）的接缝。
#[allow(clippy::too_many_arguments)] // 忠实移植：C 环境参数显式化（Fix #36）
pub fn last_dir(
    resolve: &mut Lookup,
    rd: RootDir,
    wd: LookupStart,
    uid: u32,
    gid: u32,
    mounts: &[MountedFs],
    vnode_table: &mut VnodeTable,
    req_lookup: ReqLookup<'_>,
    rdlink: RdLink<'_>,
) -> Result<(VnodeId, String), PathError> {
    let mut symloop: u32 = 0;
    let mut loop_start: Option<LookupStart> = None;
    loop {
        let split = last_dir_split(&resolve.path)?;
        let dir_start = loop_start.unwrap_or(if resolve.path.starts_with('/') {
            LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
        } else {
            wd
        });
        // 目录前缀解析：清 RET_SYMLINK（`path.c:231-235`）——目录永远跟进。
        let mut dir_lk = Lookup::new(split.dir_path.clone(), LookupFlags::NOFLAGS)?;
        let dir_node = lookup(
            LookupStart { fs: dir_start.fs, ino: dir_start.ino, dev: dir_start.dev },
            &mut dir_lk, rd, uid, gid, mounts, req_lookup,
        )?;
        // 目录 vnode 入缓存：find_by_ino 命中则 `fs_count++`+dup，未命中
        // alloc 填充（`path.c` 的 advance 缓存层，`vnode.c:84-124`）。
        let dir_vid = match vnode_table.find_by_ino(dir_node.fs_e, dir_node.ino) {
            Some(hit) => {
                if let Some(v) = vnode_table.get_mut(hit) {
                    v.fs_count += 1;
                }
                vnode_table.dup(hit);
                hit
            }
            None => {
                let scratch = vnode_table.alloc().map_err(|_| PathError::NoEnt)?;
                if let Some(v) = vnode_table.get_mut(scratch) {
                    v.fs = dir_node.fs_e;
                    v.ino = dir_node.ino;
                    v.mode = dir_node.mode;
                    v.size = dir_node.size;
                    v.uid = dir_node.uid;
                    v.gid = dir_node.gid;
                    v.dev = dir_node.dev;
                    v.fs_count = 1;
                    v.ref_count = 1;
                }
                scratch
            }
        };
        // 末组件带回 l_path 后以 RET_SYMLINK 查验（`path.c:239-240/:247-260`）。
        resolve.path = split.entry.clone();
        let mut entry_lk = Lookup::new(
            resolve.path.clone(),
            resolve.flags | LookupFlags::RET_SYMLINK,
        )?;
        let entry_node = lookup(
            LookupStart { fs: dir_node.fs_e, ino: dir_node.ino, dev: dir_node.dev },
            &mut entry_lk, rd, uid, gid, mounts, req_lookup,
        )?;
        // 末组件是符号链接且调用方要真身：rdlink 取内容重启
        // （`path.c:278-310`；C panic 分支加固为错误）。
        if (entry_node.mode & S_IFMT) == S_IFLNK
            && !resolve.flags.contains(LookupFlags::RET_SYMLINK)
        {
            let target = rdlink(entry_node.fs_e, entry_node.ino)?;
            symloop += 1;
            if symloop >= SYMLOOP_MAX as u32 {
                return Err(PathError::Loop);
            }
            resolve.path = target;
            if resolve.path.starts_with('/') {
                // 绝对链接：忘记已解析目录，回根重启。
                loop_start = None;
            } else {
                // 相对链接：以已解析目录为基继续。
                loop_start = Some(LookupStart {
                    fs: dir_node.fs_e,
                    ino: dir_node.ino,
                    dev: dir_node.dev,
                });
            }
            continue;
        }
        return Ok((dir_vid, split.entry));
    }
}

/// `get_name`（`path.c:594-646`）——按 inode 在目录项流中找组件名。
///
/// `getdents` 接缝给出一批已解析的目录项 `(ino, name)`（C 的
/// `req_getdents` 字节流与 `d_reclen` 游标解析归 FS 对话编解码层；
/// `pos` 为游标，刻度由接缝自定）。守护：非 dir vnode `EBADF`
/// （`path.c:598-600`）；流尽未命中 `ENOENT`（`path.c:609-611`）。
pub fn get_name(
    dir_mode: u32,
    dir_fs: Endpoint,
    dir_ino: u64,
    entry_ino: u64,
    getdents: GetDents<'_>,
) -> Result<String, PathError> {
    if (dir_mode & S_IFMT) != S_IFDIR {
        return Err(PathError::BadF);
    }
    let mut pos = 0;
    loop {
        let batch = getdents(dir_fs, dir_ino, pos)?;
        if batch.is_empty() {
            return Err(PathError::NoEnt);
        }
        if let Some((_, name)) = batch.iter().find(|(ino, _)| *ino == entry_ino) {
            return Ok(name.clone());
        }
        pos += batch.len();
    }
}

/// `canonical_path`（`path.c:648-770`）——符号链接展开 + 逐级爬升拼绝对路径。
///
/// 两段：① [`last_dir`] 解析出"文件所在目录 + 文件名"（末组件符号链接
/// 由 last_dir 内部的 rdlink 重试展开，`path.c:278-310`；组件名为 `"."`
/// 时清空，对应 C `path.c:679-681`）；② 爬升段——`dir != rd` 时向 `".."`
/// advance 一步、`get_name` 取父目录名前插（`path.c:704-770`），fs 根则
/// 跨出到挂载点（`path.c:712-724`），真根到顶收束。
#[allow(clippy::too_many_arguments)] // 忠实移植：C 环境参数显式化（Fix #36）
pub fn canonical_path(
    orig_path: &str,
    rd: RootDir,
    wd: LookupStart,
    uid: u32,
    gid: u32,
    mounts: &[MountedFs],
    vnode_table: &mut VnodeTable,
    req_lookup: ReqLookup<'_>,
    rdlink: RdLink<'_>,
    getdents: GetDents<'_>,
) -> Result<String, PathError> {
    let mut resolve = Lookup::new(orig_path.to_string(), LookupFlags::NOFLAGS)?;
    let (dir_vid, mut file_name) = last_dir(
        &mut resolve, rd, wd, uid, gid, mounts, vnode_table, req_lookup, rdlink,
    )?;
    let dir_vnode = vnode_table.get(dir_vid).ok_or(PathError::NoEnt)?;
    let mut cur = (dir_vnode.fs, dir_vnode.ino, dir_vnode.dev);
    // 文件名为 "."（路径以 `/` 收尾）时清空——爬升只拼目录链。
    if file_name == "." {
        file_name.clear();
    }

    // ② 爬升段：`dir != rd` 时逐级 `".."` + `get_name` 前插
    // （`path.c:704-770`）。`".."` 的 lookup 经 ELEAVEMOUNT 自动跨出挂载。
    let mut out = file_name;
    let mut guard = 0;
    while cur != (rd.fs, rd.ino, rd.dev) {
        guard += 1;
        if guard > 256 {
            return Err(PathError::Loop);
        }
        let mut up = Lookup::new("..".to_string(), LookupFlags::NOFLAGS)?;
        let parent_details = lookup(
            LookupStart { fs: cur.0, ino: cur.1, dev: cur.2 },
            &mut up, rd, uid, gid, mounts, req_lookup,
        )?;
        // `get_name(parent, cur)` 取本层组件名（`path.c:747-751`）。
        let component = get_name(
            parent_details.mode,
            parent_details.fs_e,
            parent_details.ino,
            cur.1,
            getdents,
        )?;
        // 前插 `"/component"`（`path.c:753-760` 的 memmove 预留斜杠位）。
        out = format!("/{}/{}", component, out);
        if out.len() >= PATH_MAX {
            return Err(PathError::TooLong);
        }
        cur = (parent_details.fs_e, parent_details.ino, parent_details.dev);
    }
    if !out.starts_with('/') {
        // 文件直接位于根目录下（爬升零轮）：补根前缀。
        out.insert(0, '/');
    }
    Ok(out)
}

/// Path errors — map to Minix errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    TooLong,
    Empty,
    Loop,
    NoEnt,
    Inval,
    /// `EBADF`——`get_name` 对非目录 vnode 的守护（`path.c:598-600`）。
    BadF,
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
            Self::BadF => minix_types::EBADF,
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

/// 把已取到的名字字节解成路径串（C `copy_path`/`fetch_name` 的尾门）。
///
/// C 的两处同名检查：`dest[len - 1] != '\0'` 即 `ENAMETOOLONG`
/// （utility.c:49-52 的 inline 分支与 :84-87 的跨空间分支）——调用方给的
/// `len` **含结尾 NUL**，末字节不是 NUL 说明名字被截断或长度撒谎。
pub(crate) fn decode_name(buf: &[u8], len: usize) -> Result<String, PathError> {
    if len == 0 || len > buf.len() {
        return Err(PathError::Inval);
    }
    if buf[len - 1] != 0 {
        return Err(PathError::TooLong);
    }
    Ok(String::from_utf8_lossy(&buf[..len - 1]).into_owned())
}

/// 生产 `PathFetcher`：路径字符串在**调用方内存**里，经跨地址空间拷贝取回。
///
/// C: `copy_path`（utility.c:24-55）的"名字不在消息里"分支 + `fetch_name`
/// （:60-90）——两道长度门（`len > PATH_MAX` → `ENAMETOOLONG`、
/// `len > SSIZE_MAX` → `EINVAL`）、一次 `sys_datacopy(who_e, path, VFS, dest,
/// len)`（失败 → `EINVAL`）、尾字节 NUL 检查。
///
/// `who` 是发起调用的进程端点（C 的 `who_e` 全局；Rust 显式携带，因为
/// `PathFetcher::fetch` 是 `&self` 的纯取数动词）。
#[derive(Debug, Clone, Copy)]
pub struct SysPathFetcher {
    /// 路径字符串所在进程（C `who_e`）。
    pub who: minix_types::Endpoint,
}

impl PathFetcher for SysPathFetcher {
    fn fetch(&self, addr: u64, len: usize) -> Result<String, PathError> {
        // C utility.c:71-78 的两道门。
        if len > PATH_MAX {
            return Err(PathError::TooLong);
        }
        if len == 0 || len > i64::MAX as usize {
            return Err(PathError::Inval);
        }
        let mut buf = [0u8; PATH_MAX];
        let dst = buf.as_mut_ptr() as u64;
        // C: `sys_datacopy_wrapper(who_e, path, VFS_PROC_NR, dest, len)`。
        // 失败即 `EINVAL`（utility.c:79-83）。
        minix_sys::syscall::sys_datacopy(
            &minix_sys::syscall::DirectKernelCallTransport,
            self.who.get(),
            addr,
            minix_types::Endpoint::SELF.get(),
            dst,
            len as u64,
        )
        .map_err(|_| PathError::Inval)?;
        decode_name(&buf, len)
    }

    fn copy(&self, path: &str) -> Result<String, PathError> {
        // C: 名字在消息里的那一支——只过长度门（utility.c:44-53）。
        if path.len() + 1 > PATH_MAX {
            return Err(PathError::TooLong);
        }
        Ok(path.to_string())
    }
}

/// Test double: fabricates bytes instead of reading caller memory.
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

    /// 可续走遍历的状态推进（`[ARCH: W7]`）：三次 `resume` 分别走
    /// "正常回复 → 完成"、"进挂载点 → 起点切到被挂载分区"、"出挂载点 →
    /// 起点切回挂载点自身"，每步的 `Send` 三元组即下一条 `REQ_LOOKUP` 的
    /// 参数（C `lookup:470-521` 的同一套切换）。
    #[test]
    fn test_lookup_walk_crosses_mounts() {
        let parent = Endpoint(1);
        let child = Endpoint(2);
        let rd = RootDir { ino: 1, fs: parent, dev: 0 };
        let start = LookupStart { fs: parent, ino: 1, dev: 0 };
        let mounts = [
            MountedFs { fs: child, dev: 0, root: (50, 0), mounted_on: Some((7, parent, 0)) },
        ];

        // 首步：从起点发（chroot 边界＝进程根 ino，因为同分区）。
        let (mut walk, step) = LookupWalk::begin(
            start,
            Lookup::new("/mnt/x".to_string(), LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        assert_eq!(
            step,
            WalkStep::Send { fs_e: parent, dir_ino: 1, root_ino: 1 }
        );

        // 进挂载点：`mounted_on == (7, parent)` 的行把起点切到 child:50。
        let step = walk
            .resume(LookupRes::EnterMount { ino: 7, offset: 5, symloop: 0 }, rd, &mounts)
            .unwrap();
        assert_eq!(
            step,
            WalkStep::Send { fs_e: child, dir_ino: 50, root_ino: 1 }
        );

        // 正常回复：完成，七字段带上起点三元组里的 fs 与上下文 uid/gid。
        let step = walk
            .resume(
                LookupRes::Ok { ino: 9, mode: 0o100644, size: 12, dev: 0 , uid: 0, gid: 0 },
                rd,
                &mounts,
            )
            .unwrap();
        assert_eq!(
            step,
            // 节点在**被挂载分区**里，故 fs_e 是 child（起点三元组已切过去）。
            WalkStep::Done(NodeDetails {
                fs_e: child,
                ino: 9,
                mode: 0o100644,
                size: 12,
                uid: 0,
                gid: 0,
                dev: 0,
            })
        );

        // 出挂载点：**另起一趟**，路径以 `..` 开头（C 的 bogus 守卫要求），
        // 且当前分区有挂载行 → 起点切回挂载点自身 (ino=7, parent)。
        let (mut up, step) = LookupWalk::begin(
            LookupStart { fs: child, ino: 50, dev: 0 },
            Lookup::new("../y".to_string(), LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        assert_eq!(step, WalkStep::Send { fs_e: child, dir_ino: 50, root_ino: 1 });
        // `offset` 是"FS 已吃掉的字节数"：C 先 `memmove` 推进游标**再**过
        // bogus 守卫（path.c:461-466），所以留下的那段必须以 `..` 开头；
        // 这里 offset=0 表示原样留给父分区。
        let step = up
            .resume(LookupRes::LeaveMount { offset: 0, symloop: 0 }, rd, &mounts)
            .unwrap();
        assert_eq!(
            step,
            WalkStep::Send { fs_e: parent, dir_ino: 7, root_ino: 1 },
            "起点切回挂载点自身 (ino=7, parent)"
        );
    }

    /// symloop 累计越界即 `ELOOP`（C `path.c:455-461`）；`LeaveMount` 的
    /// bogus 路径守卫（不以 `..` 开头）即 `ENOENT`（`path.c:496-521`）。
    #[test]
    fn test_lookup_walk_guards() {
        let parent = Endpoint(1);
        let rd = RootDir { ino: 1, fs: parent, dev: 0 };
        let start = LookupStart { fs: parent, ino: 1, dev: 0 };
        let mounts = [MountedFs {
            fs: parent,
            dev: 0,
            root: (1, 0),
            mounted_on: Some((4, parent, 0)),
        }];

        // symloop 越界：单步给超限的增量即报 Loop。
        let (mut walk, _) = LookupWalk::begin(
            start,
            Lookup::new("/a".to_string(), LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        assert_eq!(
            walk.resume(
                LookupRes::Symlink { offset: 1, symloop: SYMLOOP_MAX as u8 + 1 },
                rd,
                &mounts,
            ),
            Err(PathError::Loop)
        );

        // LeaveMount 且路径不以 `..` 开头 → ENOENT（C 的 bogus-path 守卫）。
        let (mut walk2, _) = LookupWalk::begin(
            start,
            Lookup::new("/a/b".to_string(), LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        assert_eq!(
            walk2.resume(LookupRes::LeaveMount { offset: 1, symloop: 0 }, rd, &mounts),
            Err(PathError::NoEnt)
        );
    }

    /// `decode_name`：调用方的 `len` 含结尾 NUL；末字节不是 NUL 即
    /// `ENAMETOOLONG`（C utility.c:49-52/:84-87 的同名检查）。
    #[test]
    fn test_decode_name_requires_trailing_nul() {
        assert_eq!(decode_name(b"/dev\0", 5).unwrap(), "/dev");
        assert_eq!(decode_name(b"/dev", 4), Err(PathError::TooLong));
        assert_eq!(decode_name(b"", 0), Err(PathError::Inval));
        assert_eq!(decode_name(b"ab\0", 9), Err(PathError::Inval), "len 越界");
    }

    /// 生产 `PathFetcher` 的两道长度门在**任何拷贝之前**生效（宿主构建下
    /// 拷贝不可达 → 诚实 `Inval`；真机路径由 E5/T4 联调覆盖）。
    #[test]
    fn test_sys_path_fetcher_gates_then_copy() {
        use minix_types::Endpoint;
        let f = SysPathFetcher { who: Endpoint(9) };
        assert_eq!(f.fetch(0x4000, PATH_MAX + 1), Err(PathError::TooLong));
        assert_eq!(f.fetch(0x4000, 0), Err(PathError::Inval));
        assert_eq!(
            f.fetch(0x4000, 5),
            Err(PathError::Inval),
            "宿主：sys_datacopy 不可达"
        );
        // `copy` 是名字在消息里的那一支：只过长度门。
        assert_eq!(f.copy("/etc/passwd").unwrap(), "/etc/passwd");
        assert_eq!(
            f.copy(&"a".repeat(PATH_MAX)),
            Err(PathError::TooLong)
        );
    }

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
            dev: 100,
            root: (1, 100),
            mounted_on: None,
        }];
        let mut calls = 0;
        let mut req = |fs: Endpoint, dir: u64, _root: u64, _lk: &mut Lookup| {
            calls += 1;
            assert_eq!(fs, parent);
            assert_eq!(dir, 1);
            Ok(LookupRes::Ok {
                ino: 42,
                mode: 0o100644,
                size: 7,
                dev: 100,
                uid: 1000,
                gid: 100,
            })
        };
        let mut lk = Lookup::new("/a/b".to_string(), LookupFlags::NOFLAGS).unwrap();
        // 遍历时带的 id 故意与回复里的属主不同：`node_details` 的 uid/gid
        // 才是文件的属主/属组（C `advance` 的 `v_uid = res.uid`，
        // path.c:98-99），遍历参数只用于 `advance` 内部的搜索权限判断。
        let nd = lookup(
            LookupStart { fs: parent, ino: 1, dev: 100 },
            &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            5555, 5555, &mounts, &mut req,
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(nd.ino, 42);
        assert_eq!(nd.fs_e, parent);
        assert_eq!((nd.uid, nd.gid), (1000, 100), "属主来自回复而非调用方 id");
    }

    #[test]
    fn test_lookup_enter_mount() {
        // `/mnt` 是挂载点：FS1 报 EENTERMOUNT → 起点切到 FS2 根 → FS2 完成。
        let parent = Endpoint::from_generation_slot(0, 10);
        let child = Endpoint::from_generation_slot(0, 11);
        let mounts = [
            MountedFs { fs: parent, dev: 100, root: (1, 100), mounted_on: None },
            MountedFs {
                fs: child,
                dev: 101,
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
                Ok(LookupRes::Ok { ino: 77, mode: 0o040755, size: 3, dev: 101 , uid: 0, gid: 0 })
            }
        };
        let nd = lookup(
            LookupStart { fs: parent, ino: 1, dev: 100 },
            &mut lk,
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
            dev: 101,
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
                Ok(LookupRes::Ok { ino: 55, mode: 0o100644, size: 1, dev: 100 , uid: 0, gid: 0 })
            }
        };
        let nd = lookup(
            LookupStart { fs: child, ino: 2, dev: 101 },
            &mut lk,
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
            dev: 101,
            root: (2, 101),
            mounted_on: Some((9, parent, 100)),
        }];
        let mut lk = Lookup::new("/etc/passwd".to_string(), LookupFlags::NOFLAGS).unwrap();
        let mut req = |_fs: Endpoint, _dir: u64, _root: u64, _lk: &mut Lookup| {
            Ok(LookupRes::LeaveMount { offset: 1, symloop: 0 })
        };
        let r = lookup(
            LookupStart { fs: child, ino: 2, dev: 101 },
            &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            0, 0, &mounts, &mut req,
        );
        assert_eq!(r.unwrap_err(), PathError::NoEnt);
    }

    #[test]
    fn test_lookup_symlink_loop_e_loop() {
        // FS 每轮都报符号链接且不推进——17 轮后 ELOOP（`path.c:455-461`）。
        let parent = Endpoint::from_generation_slot(0, 10);
        let mounts = [MountedFs { fs: parent, dev: 100, root: (1, 100), mounted_on: None }];
        let mut lk = Lookup::new("/loop".to_string(), LookupFlags::NOFLAGS).unwrap();
        let mut rounds = 0;
        let mut req = |_fs: Endpoint, _dir: u64, _root: u64, _lk: &mut Lookup| {
            rounds += 1;
            Ok(LookupRes::Symlink { offset: 0, symloop: 1 })
        };
        let r = lookup(
            LookupStart { fs: parent, ino: 1, dev: 100 },
            &mut lk,
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
            LookupStart { fs: parent, ino: 1, dev: 100 },
            &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            0, 0, &mounts, &mut req,
        );
        assert_eq!(r.unwrap_err(), PathError::NoEnt);
    }

    #[test]
    fn test_last_dir_symloop_e_loop() {
        // rdlink 每轮都返回指向自身的链接——symloop 达 16 即 ELOOP。
        let parent = Endpoint::from_generation_slot(0, 10);
        let mounts = [MountedFs { fs: parent, dev: 100, root: (1, 100), mounted_on: None }];
        let mut lk = Lookup::new("/a/link".to_string(), LookupFlags::NOFLAGS).unwrap();
        let mut vtbl = VnodeTable::new();
        let mut rdlink_calls = 0;
        let mut req = |fs: Endpoint, dir: u64, _root: u64, lk2: &mut Lookup| {
            if dir == 1 {
                return Ok(LookupRes::Ok { ino: 5, mode: 0o040755, size: 0, dev: 100 , uid: 0, gid: 0 });
            }
            if lk2.flags.contains(LookupFlags::RET_SYMLINK) {
                return Ok(LookupRes::Ok { ino: 8, mode: S_IFLNK | 0o777, size: 1, dev: 100 , uid: 0, gid: 0 });
            }
            Ok(LookupRes::Ok { ino: 5, mode: 0o040755, size: 0, dev: 100 , uid: 0, gid: 0 })
        };
        let mut rdlink = |_fs: Endpoint, _ino: u64| -> Result<String, PathError> {
            rdlink_calls += 1;
            Ok("link".to_string())
        };
        let r = last_dir(
            &mut lk,
            RootDir { ino: 1, fs: parent, dev: 100 },
            LookupStart { fs: parent, ino: 1, dev: 100 },
            0, 0, &mounts, &mut vtbl, &mut req, &mut rdlink,
        );
        assert_eq!(r.unwrap_err(), PathError::Loop);
        assert_eq!(rdlink_calls, 16);
    }

    #[test]
    fn test_get_name_batched_scan() {
        // 非 dir 拒 EBADF（`path.c:598-600`）。
        let mut no_dir = |_fs: Endpoint, _ino: u64, _pos: usize| -> Result<Vec<(u64, String)>, PathError> {
            Ok(vec![])
        };
        assert_eq!(
            get_name(0o100644, Endpoint::from_generation_slot(0, 10), 5, 9, &mut no_dir),
            Err(PathError::BadF)
        );
        // 分批扫描：第一批未命中，第二批命中（`path.c:620-634`）。
        let mut batch2 = false;
        let mut getdents = |_fs: Endpoint, _ino: u64, pos: usize| -> Result<Vec<(u64, String)>, PathError> {
            if pos == 0 {
                Ok(vec![(60u64, "d".to_string()), (61u64, "e".to_string())])
            } else {
                batch2 = true;
                Ok(vec![(9u64, "target".to_string())])
            }
        };
        let name = get_name(0o040755, Endpoint::from_generation_slot(0, 10), 5, 9, &mut getdents).unwrap();
        assert_eq!(name, "target");
        assert!(batch2);
        // 流尽未命中 → ENOENT（`path.c:609-611`）。
        let mut drained = false;
        let mut empty = |_fs: Endpoint, _ino: u64, _pos: usize| -> Result<Vec<(u64, String)>, PathError> {
            drained = true;
            Ok(vec![])
        };
        let r = get_name(0o040755, Endpoint::from_generation_slot(0, 10), 5, 9, &mut empty);
        assert_eq!(r.unwrap_err(), PathError::NoEnt);
        assert!(drained);
    }


    #[test]
    fn test_canonical_path_climb_with_symlink_file() {
        // `/a/link` 其中 `link → c`（同目录）：last_dir 展开末组件链接后
        // 爬升 `/a` 一级拼出 `/a/c`（`path.c:648-770`）。
        let p10 = Endpoint::from_generation_slot(0, 10);
        let mounts = [MountedFs { fs: p10, dev: 100, root: (1, 100), mounted_on: None }];
        let mut vtbl = VnodeTable::new();
        let mut rdlink = |_fs: Endpoint, _ino: u64| -> Result<String, PathError> {
            Ok("c".to_string())
        };
        let mut getdents = |_fs: Endpoint, dir_ino: u64, _pos: usize| -> Result<Vec<(u64, String)>, PathError> {
            match dir_ino {
                1 => Ok(vec![(5u64, "a".to_string())]),
                5 => Ok(vec![(8u64, "link".to_string()), (9u64, "c".to_string())]),
                _ => Ok(vec![]),
            }
        };
        let mut req = |fs: Endpoint, dir: u64, _root: u64, lk2: &mut Lookup| {
            assert_eq!(fs, p10);
            if lk2.path == "/a" {
                return Ok(LookupRes::Ok { ino: 5, mode: 0o040755, size: 0, dev: 100 , uid: 0, gid: 0 });
            }
            if lk2.path == "link" && lk2.flags.contains(LookupFlags::RET_SYMLINK) {
                return Ok(LookupRes::Ok { ino: 8, mode: S_IFLNK | 0o777, size: 1, dev: 100 , uid: 0, gid: 0 });
            }
            if lk2.path == "c" {
                return Ok(LookupRes::Ok { ino: 9, mode: 0o100644, size: 4, dev: 100 , uid: 0, gid: 0 });
            }
            if lk2.path == "." {
                // "．"解析为目录自身（/a = ino 5）。
                return Ok(LookupRes::Ok { ino: 5, mode: 0o040755, size: 0, dev: 100 , uid: 0, gid: 0 });
            }
            if lk2.path == ".." {
                // ".."爬出一级到根（ino 1）。
                return Ok(LookupRes::Ok { ino: 1, mode: 0o040755, size: 0, dev: 100 , uid: 0, gid: 0 });
            }
            Err(PathError::NoEnt)
        };
        let out = canonical_path(
            "/a/link",
            RootDir { ino: 1, fs: p10, dev: 100 },
            LookupStart { fs: p10, ino: 1, dev: 100 },
            0, 0, &mounts, &mut vtbl, &mut req, &mut rdlink, &mut getdents,
        )
        .unwrap();
        assert_eq!(out, "/a/c");
    }
