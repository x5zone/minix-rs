//! `stadir` — whereabouts and worth: anchors, attributes, mountain tallies.
//!
//! Corresponds to Minix3's `stadir.c:1-446` (`do_fchdir`, `do_chdir`,
//! `do_chroot`, `change_into`, `do_stat`, `do_fstat`, `update_statvfs`,
//! `fill_statvfs`, `do_statvfs`, `do_fstatvfs`, `do_getvfsstat`, `do_lstat`).
//!
//! Design decisions (see 28-stadir.md §3):
//! - `StadirCall` types the nine calls (fchdir is real despite the header)
//! - `change_dir` types the three anchor vows purely
//! - `chroot_gate` types the Son-of-Heaven door
//! - `StatSrc` types the two asking roads (symlink tail differs)
//! - `fill_plan`/`apply_readonly_overlay`/`FsIds` type the mountain form
//! - `walk_plan`/`need_lock` type the tally walk purely
//! - `StatFs` trait types the FS dialogue (test doubles)
//!
//! Scope note: road execution (`eat_path`/`get_filp`) stays with
//! 13-path-lookup.md/14-filedes.md; verdicts (`forbidden`) stay with
//! 29-protect.md; FS requests (`req_stat`/`req_statvfs`) execute FS-side
//! (12-request-wrappers.md describes the envelopes); locking executes
//! with 06-vmnt.md; copies stay kernel-side; anchors live with
//! 02-fproc-struct.md. This module only decides: call, anchor, road,
//! form, walk, and dialogue.
//!
//! Linux models the same split as `vfs_statfs` (fresh vs cached,
//! read-only overlay) over per-filesystem `statfs`, and `getvfsstat`-like
//! walks in `sys_getvfsstat`/`statfs` helpers; Redox models it as `stat`
//! schemes with cached attributes. Here `StadirCall` is the surface and
//! [`StatFs`] is the per-filesystem answer.

/// `ST_RDONLY` (`minix3/sys/sys/fstypes.h:88` via `statvfs.h:108`).
pub const ST_RDONLY: u64 = 0x0000_0001;
/// `ST_NOWAIT` (`fstypes.h:283` via `statvfs.h:139`): don't wait for I/O.
pub const ST_NOWAIT: u64 = 2;
/// `VMNT_READONLY` (`minix3/minix/servers/vfs/vmnt.h:24`, octal).
pub const VMNT_READONLY: u32 = 0o01;
/// `VMNT_CANSTAT` (`vmnt.h:28`, octal): counted in the tally.
pub const VMNT_CANSTAT: u32 = 0o20;
/// `NR_MNTS` (`minix3/minix/servers/vfs/const.h:7`): mount-table slots.
pub const NR_MNTS: usize = 16;

/// The nine directory/status calls (header lists eight; `do_fchdir`'s
/// body at `stadir.c:32` makes nine — the enumeration follows the
/// implementation, not the comment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StadirCall {
    /// `FCHDIR`: change directory by fd.
    Fchdir,
    /// `CHDIR`: change directory by path.
    Chdir,
    /// `CHROOT`: change root (root only).
    Chroot,
    /// `STAT`: attributes by path.
    Stat,
    /// `FSTAT`: attributes by fd.
    Fstat,
    /// `LSTAT`: attributes by path, symlink tail kept.
    Lstat,
    /// `STATVFS1`: mountain tally by path.
    Statvfs,
    /// `FSTATVFS1`: mountain tally by fd.
    Fstatvfs,
    /// `GETVFSSTAT`: tally all mountains.
    Getvfsstat,
}

impl StadirCall {
    /// Calls holding an fd already (vs calls walking a road).
    pub fn by_fd(self) -> bool {
        matches!(self, Self::Fchdir | Self::Fstat | Self::Fstatvfs)
    }

    /// Calls keeping the final symlink unexpanded (`PATH_RET_SYMLINK`).
    pub fn keeps_symlink(self) -> bool {
        matches!(self, Self::Lstat)
    }
}

/// Anchor change outcome (`change_into:130-134`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorOut {
    /// Already there: nothing to do (`121`).
    Keep,
    /// Released the old anchor, holding the new one (`131-133`).
    Switch,
}

/// The three anchor vows (`change_into:117-135`).
///
/// Same anchor → keep; must be a directory (`ENOTDIR`); must be
/// searchable (the X verdict, `EACCES` upstream from 29). Release-then-
/// welcome order is the decision: reversed, the old anchor leaks.
pub fn change_dir(same: bool, is_dir: bool, searchable: bool) -> Result<AnchorOut, StadirError> {
    if same {
        return Ok(AnchorOut::Keep);
    }
    if !is_dir {
        return Err(StadirError::NotDir);
    }
    if !searchable {
        return Err(StadirError::Acces);
    }
    Ok(AnchorOut::Switch)
}

/// The Son-of-Heaven door (`do_chroot:94`): only root changes roots.
pub fn chroot_gate(is_root: bool) -> Result<(), StadirError> {
    if is_root {
        return Ok(());
    }
    Err(StadirError::Perm)
}

/// Asking road for attributes (`do_stat`/`do_fstat`/`do_lstat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatSrc {
    /// Walk a road; lstat keeps the symlink tail (`433` vs `155`).
    ByPath {
        /// `PATH_RET_SYMLINK` for lstat, plain for stat.
        retain_symlink: bool,
    },
    /// Read off a held fd (`do_fstat:184-187`).
    ByFd,
}

/// Mountain freshness (`fill_statvfs:244-274`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatvfsFresh {
    /// Ask the mountain now; failure is `EIO` (`246-247`).
    Fresh,
    /// Copy the cached tally, zero-filled first (`250`).
    Cached,
}

/// Fresh-vs-cached split on the flag word (`fill_statvfs:244`).
pub fn fill_plan(flags: u64) -> StatvfsFresh {
    if flags & ST_NOWAIT != 0 {
        StatvfsFresh::Cached
    } else {
        StatvfsFresh::Fresh
    }
}

/// Read-only overlay (`fill_statvfs:276-277`): OR in `ST_RDONLY`,
/// never overwrite — the mountain's own flags survive underneath.
pub fn apply_readonly_overlay(f_flag: u64, readonly: bool) -> u64 {
    if readonly { f_flag | ST_RDONLY } else { f_flag }
}

/// Filesystem identity block (`fill_statvfs:279-281`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsIds {
    /// `f_fsid`: the device, POSIX-style.
    pub fsid: u64,
    /// `f_fsidx[0]`: the device again, NetBSD-style.
    pub idx0: u64,
    /// `f_fsidx[1]`: zero (type-name numbering lives elsewhere).
    pub idx1: u64,
}

/// Identity block from the device number (`279-281`).
pub fn fs_identity(dev: u64) -> FsIds {
    FsIds {
        fsid: dev,
        idx0: dev,
        idx1: 0,
    }
}

/// One mount's tally eligibility (`do_getvfsstat:388,407`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MountView {
    /// Device installed (`m_dev != NO_DEV`).
    pub in_use: bool,
    /// Reportable (`m_flags & VMNT_CANSTAT`); (un)mounting mounts wait.
    pub canstat: bool,
}

/// Tally-walk outcome (`do_getvfsstat:351-413`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkOut {
    /// Mountains counted.
    pub count: usize,
    /// Volumes filled (zero when counting only).
    pub fills: usize,
}

/// Whether the walk locks (`do_getvfsstat:372`).
///
/// Locking follows querying: no NOWAIT, no queries, no locks — which is
/// why procfs (a filesystem itself) can call lock-free (`366-371`).
pub fn need_lock(flags: u64) -> bool {
    flags & ST_NOWAIT == 0
}

/// Walk the mount table (`do_getvfsstat:374-410`).
///
/// - `has_buf == false`: count only, no fills, no locks (`404-410`).
/// - Otherwise: stop when `buf_slots` runs out (`376-377`); skip idle
///   or unreportable mounts (`388`); a fill failure aborts with the
///   error (`389-394`, `fill_fails_at` naming the failing fill index).
///
/// Locking itself stays execution-side (06); the walk only decides.
pub fn walk_plan(
    mounts: &[MountView],
    has_buf: bool,
    buf_slots: usize,
    fill_fails_at: Option<usize>,
) -> Result<WalkOut, StadirError> {
    let mut count = 0;
    let mut fills = 0;
    if !has_buf {
        for m in mounts {
            if m.in_use && m.canstat {
                count += 1;
            }
        }
        return Ok(WalkOut { count, fills: 0 });
    }
    for m in mounts {
        if fills >= buf_slots {
            break;
        }
        if !(m.in_use && m.canstat) {
            continue;
        }
        if fill_fails_at == Some(fills) {
            return Err(StadirError::Io);
        }
        count += 1;
        fills += 1;
    }
    Ok(WalkOut { count, fills })
}

/// The FS dialogue behind a trait.
///
/// `req_stat`/`req_statvfs` execute FS-side; the FS is the only
/// untestable point, so only the dialogue is abstracted.
pub trait StatFs {
    /// `req_stat`: fetch attributes.
    fn stat(&mut self) -> Result<(), StadirError>;
    /// `req_statvfs`: fetch a mountain tally.
    fn statvfs(&mut self) -> Result<(), StadirError>;
}

/// Scripted FS (test double with programmed answers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedStat {
    /// Programmed `stat` answer.
    pub stat_result: Result<(), StadirError>,
    /// Programmed `statvfs` answer.
    pub statvfs_result: Result<(), StadirError>,
    /// Downcalls made (observable dialogue).
    pub ncalls: u32,
}

impl Default for ScriptedStat {
    fn default() -> Self {
        Self {
            stat_result: Ok(()),
            statvfs_result: Ok(()),
            ncalls: 0,
        }
    }
}

impl StatFs for ScriptedStat {
    fn stat(&mut self) -> Result<(), StadirError> {
        self.ncalls += 1;
        self.stat_result
    }
    fn statvfs(&mut self) -> Result<(), StadirError> {
        self.ncalls += 1;
        self.statvfs_result
    }
}

/// Refusing FS (test double: every downcall fails with `EIO`).
///
/// Behaves differently from [`ScriptedStat`] (blanket refusal vs
/// programmed answers), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefusingStat;

impl StatFs for RefusingStat {
    fn stat(&mut self) -> Result<(), StadirError> {
        Err(StadirError::Io)
    }
    fn statvfs(&mut self) -> Result<(), StadirError> {
        Err(StadirError::Io)
    }
}

/// L2 contract probe: stat then statvfs through any FS (generic bound use).
pub fn stat_then_statvfs<S: StatFs>(fs: &mut S) -> Result<(), StadirError> {
    fs.stat()?;
    fs.statvfs()
}

/// What the call tells the main loop (ARCH A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StadirVerdict {
    /// Reply now (status carried separately).
    Done,
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// Road failures ride `err_code` upstream (caller-side inputs), so they
/// are not variants here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StadirError {
    /// `EPERM`: non-root chroot.
    Perm,
    /// `ENOTDIR`: the anchor is no directory.
    NotDir,
    /// `EACCES`: unsearchable anchor (29's verdict, surfaced here).
    Acces,
    /// `EIO`: fresh mountain queries refused (FS-side or fill).
    Io,
}

impl StadirError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Perm => minix_types::EPERM,
            Self::NotDir => minix_types::ENOTDIR,
            Self::Acces => minix_types::EACCES,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calls_and_anchor() {
        // Nine surface calls (the header names eight; fchdir's body is real).
        let all = [
            StadirCall::Fchdir,
            StadirCall::Chdir,
            StadirCall::Chroot,
            StadirCall::Stat,
            StadirCall::Fstat,
            StadirCall::Lstat,
            StadirCall::Statvfs,
            StadirCall::Fstatvfs,
            StadirCall::Getvfsstat,
        ];
        assert_eq!(all.len(), 9);
        assert!(StadirCall::Fstat.by_fd());
        assert!(StadirCall::Fstatvfs.by_fd());
        assert!(StadirCall::Fchdir.by_fd());
        assert!(!StadirCall::Stat.by_fd());
        assert!(StadirCall::Lstat.keeps_symlink());
        assert!(!StadirCall::Stat.keeps_symlink());
        // Anchor vows in order (`121-128`): same, directory, searchable.
        assert_eq!(change_dir(true, false, false), Ok(AnchorOut::Keep));
        assert_eq!(change_dir(false, false, true), Err(StadirError::NotDir));
        assert_eq!(change_dir(false, true, false), Err(StadirError::Acces));
        assert_eq!(change_dir(false, true, true), Ok(AnchorOut::Switch));
        // Only root changes roots (`94`).
        assert!(chroot_gate(true).is_ok());
        assert_eq!(chroot_gate(false), Err(StadirError::Perm));
        assert_eq!(StadirError::Perm.to_errno(), minix_types::EPERM);
    }

    #[test]
    fn test_asking_roads_and_mountain_form() {
        // stat/fstat/lstat share the road, differing in source and tail.
        let src = StatSrc::ByPath {
            retain_symlink: false,
        };
        assert_eq!(
            src,
            StatSrc::ByPath {
                retain_symlink: false
            }
        );
        assert_ne!(
            src,
            StatSrc::ByPath {
                retain_symlink: true
            }
        );
        assert_eq!(StatSrc::ByFd, StatSrc::ByFd);
        // NOWAIT picks the cache; otherwise ask fresh (`244`).
        assert_eq!(fill_plan(ST_NOWAIT), StatvfsFresh::Cached);
        assert_eq!(fill_plan(0), StatvfsFresh::Fresh);
        assert_eq!(fill_plan(ST_RDONLY), StatvfsFresh::Fresh);
        // Overlay ORs, never overwrites (`276-277`).
        assert_eq!(apply_readonly_overlay(0x10, true), 0x10 | ST_RDONLY);
        assert_eq!(apply_readonly_overlay(0x10, false), 0x10);
        assert_eq!(apply_readonly_overlay(ST_RDONLY, true), ST_RDONLY);
        // Identity block echoes the device twice plus zero (`279-281`).
        assert_eq!(
            fs_identity(7),
            FsIds {
                fsid: 7,
                idx0: 7,
                idx1: 0
            }
        );
    }

    #[test]
    fn test_tally_walk() {
        let mounts = [
            MountView {
                in_use: true,
                canstat: true,
            },
            MountView {
                in_use: true,
                canstat: false,
            },
            MountView {
                in_use: false,
                canstat: true,
            },
            MountView {
                in_use: true,
                canstat: true,
            },
        ];
        // No pen: count only (`404-410`).
        assert_eq!(
            walk_plan(&mounts, false, 0, None),
            Ok(WalkOut { count: 2, fills: 0 })
        );
        // With pen: fill usable mounts in order.
        assert_eq!(
            walk_plan(&mounts, true, 8, None),
            Ok(WalkOut { count: 2, fills: 2 })
        );
        // Tight pens stop early (`376-377`).
        assert_eq!(
            walk_plan(&mounts, true, 1, None),
            Ok(WalkOut { count: 1, fills: 1 })
        );
        assert_eq!(
            walk_plan(&mounts, true, 0, None),
            Ok(WalkOut { count: 0, fills: 0 })
        );
        // Failed fills abort with the error (`389-394`).
        assert_eq!(walk_plan(&mounts, true, 8, Some(1)), Err(StadirError::Io));
        assert_eq!(walk_plan(&mounts, true, 8, Some(0)), Err(StadirError::Io));
        // Locking follows querying (`372`).
        assert!(need_lock(0));
        assert!(!need_lock(ST_NOWAIT));
        // Table bound holds (`const.h:7`).
        assert_eq!(NR_MNTS, 16);
    }

    #[test]
    fn test_fs_dialogue() {
        // L2 contract through any FS: stat then statvfs.
        let mut scripted = ScriptedStat::default();
        assert!(stat_then_statvfs(&mut scripted).is_ok());
        assert_eq!(scripted.ncalls, 2);
        // First failure short-circuits before the second call.
        let mut failing = ScriptedStat {
            stat_result: Err(StadirError::Io),
            ..Default::default()
        };
        assert_eq!(stat_then_statvfs(&mut failing), Err(StadirError::Io));
        assert_eq!(failing.ncalls, 1);
        // Blanket refusal (second impl).
        let mut refusing = RefusingStat;
        assert_eq!(stat_then_statvfs(&mut refusing), Err(StadirError::Io));
    }

    #[test]
    fn test_errno_map_covers_stadir_c() {
        for (err, errno) in [
            (StadirError::Perm, minix_types::EPERM),
            (StadirError::NotDir, minix_types::ENOTDIR),
            (StadirError::Acces, minix_types::EACCES),
            (StadirError::Io, minix_types::EIO),
        ] {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
        // Flag domains hold their verified values.
        assert_eq!((ST_RDONLY, ST_NOWAIT), (0x1, 2));
        assert_eq!((VMNT_READONLY, VMNT_CANSTAT), (0o01, 0o20));
    }
}
