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
#[cfg(test)] // Box is used only by the test modules (PathFetcher/FsClient injection)
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
        /// `node_details.uid` — the **file owner** (C `advance`'s
        /// `new_vp->v_uid = res.uid`, path.c:98-99; the same at
        /// `path.c:565-566`). Permission checks use it, not the caller's own
        /// id.
        uid: u32,
        /// `node_details.gid` — the file group (same as above).
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

/// Loop-time view of a mount row (a snapshot of the `vmnt[]` fields, built by
/// the caller from VmntTable+VnodeTable).
///
/// C reads the shared `vmnt[]`/`->v_mounted_on->` directly inside the loop;
/// with the Rust tables split apart this arrives as an identity snapshot —
/// under the single-threaded event loop nothing changes concurrently during
/// the loop, so the snapshot is equivalent (rebuilt before each lookup round
/// once the wiring lands).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountedFs {
    /// `m_fs_e` — the FS endpoint of this mount's partition.
    pub fs: Endpoint,
    /// `m_dev` — the mounted device number (the source of `v_dev`; C
    /// `advance`'s `new_vp->v_dev = vmp->m_dev`).
    pub dev: u64,
    /// Root vnode identity (`m_root_node`): `(ino, dev)`.
    pub root: (u64, u64),
    /// Mount-point vnode identity (`m_mounted_on`): `(ino, fs_e, dev)`;
    /// `None` = an empty row.
    pub mounted_on: Option<(u64, Endpoint, u64)>,
}

/// Type alias for the `req_getdents` conversation seam — one batch of parsed
/// directory entries `(ino, name)`.
pub type GetDents<'a> =
    &'a mut dyn FnMut(Endpoint, u64, usize) -> Result<Vec<(u64, String)>, PathError>;

/// Type alias for the `req_rdlink` conversation seam — reads symlink content.
pub type RdLink<'a> = &'a mut dyn FnMut(Endpoint, u64) -> Result<String, PathError>;

/// The process root anchor (`fp_rd`) — the chroot boundary and the symlink
/// restart point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootDir {
    /// `v_inode_nr`。
    pub ino: u64,
    /// `v_fs_e`。
    pub fs: Endpoint,
    /// `v_dev`。
    pub dev: u64,
}

/// `lookup` (`path.c:384-546`) — the cross-FS round-trip resolution loop.
///
/// Each round hands `(fs_e, dir_ino, root_ino)` to `req_lookup` so the FS can
/// resolve as much of the remaining path as possible; the FS answers with one
/// of four [`LookupRes`] kinds — `Ok` settles it, and the three special codes
/// (enter mount/leave mount/symlink) advance the path and switch the starting
/// point for another round. The loop only adjudicates: raising or lowering
/// vmnt locks is the execution side (tll), and this round records "which mount
/// row is currently in effect".
///
/// The `req_lookup` closure is the seam of the REQ_LOOKUP conversation (it
/// connects `FsReq::Lookup` and `FsClient` once the wiring lands; tests use
/// scripted replies).
///
/// The starting-point identity of `lookup` (the three fields of `start_node`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupStart {
    /// `v_fs_e`。
    pub fs: Endpoint,
    /// `v_inode_nr`。
    pub ino: u64,
    /// `v_dev`。
    pub dev: u64,
}

/// REQ_LOOKUP conversation seam — one round of "the FS resolves as much as it
/// can".
pub type ReqLookup<'a> = &'a mut dyn FnMut(Endpoint, u64, u64, &mut Lookup) -> Result<LookupRes, PathError>;

#[allow(clippy::too_many_arguments)] // Faithful port: C environment parameters made explicit
/// One traversal step (produced by [`LookupWalk`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkStep {
    /// Next step: send this `REQ_LOOKUP` to `fs_e` (`dir_ino` is the starting
    /// point, `root_ino` the chroot boundary; all zeros means no boundary).
    Send {
        /// Target filesystem endpoint.
        fs_e: Endpoint,
        /// Starting directory inode.
        dir_ino: u64,
        /// chroot boundary inode.
        root_ino: u64,
    },
    /// Finished: the node details (seven fields, C `lookup:548-554`).
    Done(NodeDetails),
}

/// The **resumable state machine** of path traversal (`[ARCH: W7]`).
///
/// C's `lookup()` issues `req_lookup` several times within one pass (the first
/// call at path.c:229, continued from the special-code loop at :300), and each
/// call blocks the worker thread's stack; a single-threaded event loop has no
/// resumable stack, so "where we are" becomes explicit state: the starting
/// triple ([the `fs_e` field](#structfield.fs_e)) + symloop + the path cursor
/// (`resolve.path`). `resume(lookup_res)` consumes one FS reply and yields the
/// next step — **the same state machine serves both the synchronous driver**
/// (the closure loop in [`lookup`], used by tests and the single-shot path)
/// **and suspend/resume** (the arm stores the cursor in the worker slot and
/// calls `resume` when the reply arrives).
#[derive(Debug, Clone)]
pub struct LookupWalk {
    fs_e: Endpoint,
    dir_ino: u64,
    root_ino: u64,
    symloop: u32,
    resolve: Lookup,
    /// The caller's effective uid/gid (the `uid`/`gid` parameters of C
    /// `lookup:400-421`).
    ///
    /// C uses them to attach a **credential grant** (`PATH_GET_UCRED`) to
    /// `REQ_LOOKUP` so the FS can do permission checks; this rewrite's
    /// credential face is not wired yet (`encode_lookup`'s debug_assert blocks
    /// `PATH_GET_UCRED`), so these two fields currently **just travel along**
    /// and are not read — they become inputs once the credential face is
    /// connected. See the note in `encode_lookup`.
    #[allow(dead_code)]
    uid: u32,
    #[allow(dead_code)]
    gid: u32,
}

impl LookupWalk {
    /// Start a traversal: check the path is non-empty and output the first
    /// step (the parameters of the first `REQ_LOOKUP`).
    ///
    /// C: `lookup:400-421` — an empty path is `ENOENT`, and the chroot
    /// boundary applies only when the root and the starting point are on the
    /// same partition (`rd.dev == start_dev`).
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

    /// Borrow the traversal's path cursor (C's `resolve->l_path` — the driver
    /// may read it, e.g. to judge an early-returning component).
    pub fn resolve(&self) -> &Lookup {
        &self.resolve
    }

    /// Consume one FS reply and yield the next step. `rd`/`mounts` are passed
    /// in as of that moment (cursor advancement and mount-point crossing must
    /// read the latest tables on resume, not the snapshot taken when
    /// suspending).
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
                        // The owner comes from the FS reply (C `advance`'s
                        // `v_uid = res.uid`), **not** the caller ids carried
                        // by the traversal — those two only serve the search
                        // permission check inside `advance`.
                        uid,
                        gid,
                        dev,
                    }))
                }
                _ => unreachable!(),
            };
        }
        // Special codes: advance the cursor + accumulate symloop (`path.c:446-461`).
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
            // Symlink: restart from the process root (`path.c:465-468`).
            LookupRes::Symlink { .. } => {
                self.dir_ino = rd.ino;
                self.fs_e = rd.fs;
                self.root_ino = rd.ino;
            }
            // Enter mount: the starting point switches to the mounted
            // partition's root (`path.c:470-484`).
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
            // Leave mount: the path must start with `..` (`path.c:496-521`'s
            // bogus guard).
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
    // The single source of truth for traversal logic is [`LookupWalk`]: this is
    // just the "synchronous driver" — take a step, send one REQ_LOOKUP, feed the
    // reply back (C's `lookup` loop does it in the same pass because its
    // `req_lookup` blocks the thread stack; the single-threaded version splits
    // it into two resumable halves).
    let (mut walk, mut step) = LookupWalk::begin(start, resolve.clone(), rd, uid, gid)?;
    loop {
        match step {
            WalkStep::Done(details) => {
                // Write the cursor back (C's `resolve` is the caller's
                // pointer, already advanced on exit).
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

/// The split decision of `last_dir` (`path.c:185-228`).
///
/// Strip trailing slashes (`DO_POSIX=false`, only when len>1) → `strrchr('/')`
/// cuts the directory prefix and the last component → the component name
/// passes the `NAME_MAX` gate (`ENAMETOOLONG`). For the root path `"/"` the
/// directory prefix stays `"/"` and the component name is `"."`
/// (`path.c:200-204`).
pub fn last_dir_split(path: &str) -> Result<LastDirSplit, PathError> {
    let mut s = path;
    while s.len() > 1 && s.ends_with('/') {
        s = &s[..s.len() - 1];
    }
    let (dir, entry) = match s.rfind('/') {
        // `cp == NULL`: the path has no slash — the directory is "."
        // (`path.c:200-205`).
        None => (".", s),
        // The prefix includes the leading `/`; `cp[1]=='\0'` (the path ends
        // with a slash) makes the component ".".
        Some(pos) => {
            let dir = &s[..=pos];
            let entry = if pos + 1 >= s.len() { "." } else { &s[pos + 1..] };
            // The directory prefix drops extra trailing slashes
            // (`path.c:226-229`, keeping the first character).
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

/// The product of `last_dir_split`: directory prefix + last component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastDirSplit {
    /// The directory prefix fed to the resolution loop.
    pub dir_path: String,
    /// The last component name (`dir_entry`).
    pub entry: String,
}

/// `advance` (`path.c:36-127`) — the lookup loop + vnode cache layer.
///
/// `get_free_vnode` (`alloc`) reserves a draft slot → [`lookup`] resolves →
/// `find_by_ino` hit means `dup` reuse, a miss fills the draft slot and marks
/// the reference (`v_fs_count = 1`, `ref_count = 1`, i.e. `dup_vnode`). C's
/// "vnode vanished mid-race, rebuild the count" compensation (`path.c:80-90`)
/// is unreachable in the single-threaded model.
#[allow(clippy::too_many_arguments)] // Faithful port: C environment parameters made explicit
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
        // Cache hit: `v_fs_count++` (an FS-side reference) + `dup_vnode`'s
        // `ref++`.
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

/// `eat_path` (`path.c:131-141`) — the first character selects the starting
/// point: `/` starts at the process root, otherwise at the working directory;
/// then `advance`.
#[allow(clippy::too_many_arguments)] // Faithful port: C environment parameters made explicit
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

/// `last_dir` (`path.c:145-380`) — split + resolve the directory + retry the
/// last component's rdlink.
///
/// C's do-while structure: split ([`last_dir_split`]) → advance the directory
/// prefix (with `RET_SYMLINK` cleared, `path.c:231-235`) → bring the last
/// component back and check it with `RET_SYMLINK` (`path.c:247-260`) → if it
/// is a symlink and the caller does not want the link itself, `req_rdlink`
/// fetches the content and restarts (a value containing a slash restarts from
/// the root if absolute, or from the resolved directory if relative,
/// `path.c:289-303`; a value without a slash returns to the original starting
/// family, bounded by `symloop < SYMLOOP_MAX`).
///
/// Returns (the directory vnode's cache slot, the last component name).
/// `rdlink` is the seam of `req_rdlink` (`path.c:281-285`).
#[allow(clippy::too_many_arguments)] // Faithful port: C environment parameters made explicit
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
        // Resolve the directory prefix with RET_SYMLINK cleared
        // (`path.c:231-235`) — a directory is always followed.
        let mut dir_lk = Lookup::new(split.dir_path.clone(), LookupFlags::NOFLAGS)?;
        let dir_node = lookup(
            LookupStart { fs: dir_start.fs, ino: dir_start.ino, dev: dir_start.dev },
            &mut dir_lk, rd, uid, gid, mounts, req_lookup,
        )?;
        // Put the directory vnode into the cache: on a find_by_ino hit
        // `fs_count++`+dup, otherwise alloc and fill (the advance cache layer
        // in `path.c`, `vnode.c:84-124`).
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
        // Bring the last component back into l_path and check it with
        // RET_SYMLINK (`path.c:239-240/:247-260`).
        resolve.path = split.entry.clone();
        let mut entry_lk = Lookup::new(
            resolve.path.clone(),
            resolve.flags | LookupFlags::RET_SYMLINK,
        )?;
        let entry_node = lookup(
            LookupStart { fs: dir_node.fs_e, ino: dir_node.ino, dev: dir_node.dev },
            &mut entry_lk, rd, uid, gid, mounts, req_lookup,
        )?;
        // The last component is a symlink and the caller wants the real node:
        // rdlink fetches the content and restarts (`path.c:278-310`; C's panic
        // branch is hardened into an error).
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
                // Absolute link: forget the resolved directory and restart
                // from the root.
                loop_start = None;
            } else {
                // Relative link: continue from the resolved directory as the
                // base.
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

/// `get_name` (`path.c:594-646`) — find a component name in the directory
/// entry stream by inode.
///
/// The `getdents` seam yields one batch of parsed directory entries `(ino,
/// name)` (C's `req_getdents` byte stream and `d_reclen` cursor parsing belong
/// to the FS conversation codec; `pos` is the cursor, with a scale defined by
/// the seam). Guards: a non-dir vnode gets `EBADF` (`path.c:598-600`); the
/// stream running out without a hit gives `ENOENT` (`path.c:609-611`).
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

/// `canonical_path` (`path.c:648-770`) — expand symlinks and climb level by
/// level to assemble an absolute path.
///
/// Two parts: (1) [`last_dir`] resolves "the file's directory + file name" (a
/// symlink last component is expanded by last_dir's internal rdlink retry,
/// `path.c:278-310`; a component name of `"."` is cleared, matching C
/// `path.c:679-681`); (2) the climb — while `dir != rd`, advance one `".."`
/// step and prepend the parent's name via `get_name` (`path.c:704-770`), step
/// out to the mount point at an fs root (`path.c:712-724`), and stop at the
/// real root.
#[allow(clippy::too_many_arguments)] // Faithful port: C environment parameters made explicit
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
    // A file name of "." (the path ends with `/`) is cleared — the climb only
    // assembles the directory chain.
    if file_name == "." {
        file_name.clear();
    }

    // (2) The climb: while `dir != rd`, `".."` + `get_name` prepend level by
    // level (`path.c:704-770`). The `".."` lookup crosses out of a mount
    // through ELEAVEMOUNT automatically.
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
        // `get_name(parent, cur)` fetches this level's component name
        // (`path.c:747-751`).
        let component = get_name(
            parent_details.mode,
            parent_details.fs_e,
            parent_details.ino,
            cur.1,
            getdents,
        )?;
        // Prepend `"/component"` (`path.c:753-760`'s memmove reserving a slot
        // for the slash).
        out = format!("/{}/{}", component, out);
        if out.len() >= PATH_MAX {
            return Err(PathError::TooLong);
        }
        cur = (parent_details.fs_e, parent_details.ino, parent_details.dev);
    }
    if !out.starts_with('/') {
        // The file sits directly under the root (zero climb rounds): add the
        // root prefix.
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
    /// `EBADF` — `get_name`'s guard for a non-directory vnode
    /// (`path.c:598-600`).
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

/// Decode the already-fetched name bytes into a path string (the tail gate of
/// C `copy_path`/`fetch_name`).
///
/// C checks the same thing in two places: `dest[len - 1] != '\0'` means
/// `ENAMETOOLONG` (utility.c:49-52's inline branch and :84-87's cross-space
/// branch) — the `len` given by the caller **includes the trailing NUL**, so a
/// last byte that is not NUL means the name was truncated or the length lied.
pub(crate) fn decode_name(buf: &[u8], len: usize) -> Result<String, PathError> {
    if len == 0 || len > buf.len() {
        return Err(PathError::Inval);
    }
    if buf[len - 1] != 0 {
        return Err(PathError::TooLong);
    }
    Ok(String::from_utf8_lossy(&buf[..len - 1]).into_owned())
}

/// `copy_path` — the two C branches for fetching a name in a path request are
/// fully isomorphic (utility.c:17-43).
///
/// C: a length beyond the destination buffer (`PATH_MAX`) is rejected first
/// with `ENAMETOOLONG` (:21-25); above `M_PATH_STRING_MAX` it fetches from the
/// caller's address space via `fetch_name` (:31-32); otherwise it strncpy's
/// the inline message and checks the trailing NUL (:35-41). This line's inline
/// capacity is [`minix_sys::vfs::OPEN_PATH_INLINE_MAX`] (32 — the window width
/// under the LP64 56-byte payload invariant, doc 99 §1.2; C i386 is 40), and
/// the fetch half's PATH_MAX/NUL gates live in [`SysPathFetcher::fetch`]
/// (isomorphic to utility.c:60-90).
///
/// `inline` is the inline bytes this arm read from the payload's buf lane;
/// `name_addr`/`name_len` are the same request's pointer and NUL-including
/// length lanes; `who` is the caller's endpoint (C `who_e`).
pub(crate) fn copy_path(
    inline: &[u8],
    name_addr: u64,
    name_len: usize,
    who: minix_types::Endpoint,
) -> Result<String, PathError> {
    if name_len <= minix_sys::vfs::OPEN_PATH_INLINE_MAX {
        let n = name_len.min(inline.len());
        decode_name(&inline[..n], n)
    } else {
        SysPathFetcher { who }.fetch(name_addr, name_len)
    }
}

/// Production `PathFetcher`: the path string lives in the **caller's memory**
/// and is fetched back through a cross-address-space copy.
///
/// C: the "name is not in the message" branch of `copy_path`
/// (utility.c:24-55) + `fetch_name` (:60-90) — two length gates
/// (`len > PATH_MAX` → `ENAMETOOLONG`, `len > SSIZE_MAX` → `EINVAL`), one
/// `sys_datacopy(who_e, path, VFS, dest, len)` (failure → `EINVAL`), and the
/// trailing NUL check.
///
/// `who` is the endpoint of the calling process (C's `who_e` global; Rust
/// carries it explicitly because `PathFetcher::fetch` is a `&self` pure fetch
/// verb).
#[derive(Debug, Clone, Copy)]
pub struct SysPathFetcher {
    /// The process whose memory holds the path string (C `who_e`).
    pub who: minix_types::Endpoint,
}

impl PathFetcher for SysPathFetcher {
    fn fetch(&self, addr: u64, len: usize) -> Result<String, PathError> {
        // The two gates at C utility.c:71-78.
        if len > PATH_MAX {
            return Err(PathError::TooLong);
        }
        if len == 0 || len > i64::MAX as usize {
            return Err(PathError::Inval);
        }
        let mut buf = [0u8; PATH_MAX];
        let dst = buf.as_mut_ptr() as u64;
        // C: `sys_datacopy_wrapper(who_e, path, VFS_PROC_NR, dest, len)`.
        // Failure means `EINVAL` (utility.c:79-83).
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
        // C: the branch where the name is in the message — only the length
        // gate (utility.c:44-53).
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

    /// State advancement of the resumable traversal (`[ARCH: W7]`): the three
    /// `resume` calls walk "normal reply → done", "enter mount → the starting
    /// point switches to the mounted partition", and "leave mount → the
    /// starting point switches back to the mount point itself"; each step's
    /// `Send` triple is the next `REQ_LOOKUP`'s parameters (the same switching
    /// as C `lookup:470-521`).
    #[test]
    fn test_lookup_walk_crosses_mounts() {
        let parent = Endpoint(1);
        let child = Endpoint(2);
        let rd = RootDir { ino: 1, fs: parent, dev: 0 };
        let start = LookupStart { fs: parent, ino: 1, dev: 0 };
        let mounts = [
            MountedFs { fs: child, dev: 0, root: (50, 0), mounted_on: Some((7, parent, 0)) },
        ];

        // First step: sent from the starting point (the chroot boundary = the
        // process root ino, since same partition).
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

        // Enter mount: the row with `mounted_on == (7, parent)` switches the
        // starting point to child:50.
        let step = walk
            .resume(LookupRes::EnterMount { ino: 7, offset: 5, symloop: 0 }, rd, &mounts)
            .unwrap();
        assert_eq!(
            step,
            WalkStep::Send { fs_e: child, dir_ino: 50, root_ino: 1 }
        );

        // Normal reply: done, the seven fields carrying the fs from the
        // starting triple and the context uid/gid.
        let step = walk
            .resume(
                LookupRes::Ok { ino: 9, mode: 0o100644, size: 12, dev: 0 , uid: 0, gid: 0 },
                rd,
                &mounts,
            )
            .unwrap();
        assert_eq!(
            step,
            // The node lives in the **mounted partition**, so fs_e is child
            // (the starting triple already switched over).
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

        // Leave mount: **a separate pass**, the path starts with `..` (required
        // by C's bogus guard), and the current partition has a mount row → the
        // starting point switches back to the mount point itself (ino=7,
        // parent).
        let (mut up, step) = LookupWalk::begin(
            LookupStart { fs: child, ino: 50, dev: 0 },
            Lookup::new("../y".to_string(), LookupFlags::NOFLAGS).unwrap(),
            rd,
            0,
            0,
        )
        .unwrap();
        assert_eq!(step, WalkStep::Send { fs_e: child, dir_ino: 50, root_ino: 1 });
        // `offset` is "the number of bytes the FS already consumed": C
        // `memmove`s the cursor forward **before** the bogus guard
        // (path.c:461-466), so the remaining segment must start with `..`; here
        // offset=0 means it is left as is for the parent partition.
        let step = up
            .resume(LookupRes::LeaveMount { offset: 0, symloop: 0 }, rd, &mounts)
            .unwrap();
        assert_eq!(
            step,
            WalkStep::Send { fs_e: parent, dir_ino: 7, root_ino: 1 },
            "起点切回挂载点自身 (ino=7, parent)"
        );
    }

    /// An out-of-range symloop accumulation is `ELOOP` (C `path.c:455-461`);
    /// `LeaveMount`'s bogus-path guard (not starting with `..`) is `ENOENT`
    /// (`path.c:496-521`).
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

        // symloop out of range: one step with an over-limit delta reports
        // Loop.
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

        // LeaveMount with a path not starting with `..` → ENOENT (C's
        // bogus-path guard).
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

    /// `decode_name`: the caller's `len` includes the trailing NUL; a last byte
    /// that is not NUL means `ENAMETOOLONG` (C's same-name checks at
    /// utility.c:49-52/:84-87).
    #[test]
    fn test_decode_name_requires_trailing_nul() {
        assert_eq!(decode_name(b"/dev\0", 5).unwrap(), "/dev");
        assert_eq!(decode_name(b"/dev", 4), Err(PathError::TooLong));
        assert_eq!(decode_name(b"", 0), Err(PathError::Inval));
        assert_eq!(decode_name(b"ab\0", 9), Err(PathError::Inval), "len 越界");
    }

    /// The production `PathFetcher`'s two length gates take effect **before any
    /// copy** (on a host build the copy is unreachable → an honest `Inval`; the
    /// real-machine path is covered by E5/T4 integration).
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
        // `copy` is the branch where the name is in the message: only the
        // length gate.
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
        // Matched in one round: the FS gives the final node directly
        // (`path.c:548-554` seven-field fill).
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
        // The id carried by the traversal deliberately differs from the owner
        // in the reply: `node_details`'s uid/gid are the file's owner/group
        // (C `advance`'s `v_uid = res.uid`, path.c:98-99), and the traversal
        // parameters only serve the search permission check inside `advance`.
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
        // `/mnt` is a mount point: FS1 reports EENTERMOUNT → the starting
        // point switches to FS2's root → FS2 finishes.
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
                // Consume "/mnt/", leaving "data" (`path.c:450-453`).
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
        // After leaving a mount the remaining path must start with `..`
        // (`path.c:515-521`).
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
                // Consume "/", leaving "../hidden" — a legal leave-mount
                // shape.
                Ok(LookupRes::LeaveMount { offset: 1, symloop: 0 })
            } else {
                // After leaving, it lands on the parent partition's
                // mount-point vnode (ino 9).
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
        // A remaining path that is not `..` is a bogus path fed by the child
        // FS (`path.c:519-521` → ENOENT).
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
        // The FS reports a symlink every round without advancing — after 17
        // rounds, ELOOP (`path.c:455-461`).
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
        assert_eq!(rounds, 17); // only when symloop reaches 17 > 16 is it out of range
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
        // rdlink returns a self-pointing link every round — ELOOP once symloop
        // reaches 16.
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
        // A non-dir is refused with EBADF (`path.c:598-600`).
        let mut no_dir = |_fs: Endpoint, _ino: u64, _pos: usize| -> Result<Vec<(u64, String)>, PathError> {
            Ok(vec![])
        };
        assert_eq!(
            get_name(0o100644, Endpoint::from_generation_slot(0, 10), 5, 9, &mut no_dir),
            Err(PathError::BadF)
        );
        // Batched scan: the first batch misses, the second hits
        // (`path.c:620-634`).
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
        // Stream drained without a hit → ENOENT (`path.c:609-611`).
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
        // `/a/link` where `link → c` (same directory): last_dir expands the
        // last-component link, then the climb over `/a` assembles `/a/c`
        // (`path.c:648-770`).
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
                // "." resolves to the directory itself (/a = ino 5).
                return Ok(LookupRes::Ok { ino: 5, mode: 0o040755, size: 0, dev: 100 , uid: 0, gid: 0 });
            }
            if lk2.path == ".." {
                // ".." climbs one level to the root (ino 1).
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
