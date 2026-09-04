//! `link` — names and meat: hard links, removals, renames, truncations.
//!
//! Corresponds to Minix3's `link.c:1-508` (`do_link`, `do_unlink`,
//! `do_rename`, `do_truncate`, `do_ftruncate`, `truncate_vnode`,
//! `do_slink`, `rdlink_direct`, `do_rdlink`).
//!
//! Design decisions (see 27-link.md §3):
//! - `LinkCall` types the eight calls (unlink doubles as rmdir)
//! - `LinkLookup` + `lookup_spec` table the road specs (testable)
//! - `sticky_check` types the sticky gate purely
//! - length/skip/type/mode gates type the truncate doors
//! - `check_slink_len` types the string doors (returns req length)
//! - `RdlinkSide` types the inner/outer read split
//! - `same_device`/`old_name_fits` + `FsLink` type the device/perm doors
//!
//! Scope note: road execution (`eat_path`/`last_dir`/`advance`,
//! `fetch_name`/`copy_path`) stays with 13-path-lookup.md; permission
//! checks (`forbidden`) stay with 29-protect.md; FS requests (`req_*`)
//! execute FS-side (12-request-wrappers.md describes the envelopes);
//! fd-table execution stays with 14-filedes.md; locking executes with
//! 05/06/07. This module only decides: call, road, gate, and door.
//!
//! Linux models the same split as `vfs_link`/`vfs_unlink`/`vfs_rename`
//! (permission + sticky + device gates) over per-filesystem `i_op`
//! (the actual directory surgery); Redox models it as namespace renames
//! with owner-or-root sticky rules. Here `LinkCall` is the surface and
//! [`FsLink`] is the per-filesystem answer.

use crate::path::PATH_MAX;
use crate::socket::SSIZE_MAX_U64;

/// `_POSIX_SYMLINK_MAX` (`minix3/include/limits.h:60`): 255.
pub const POSIX_SYMLINK_MAX: usize = 255;
/// `SU_UID` (`minix3/minix/servers/vfs/const.h:15`): root's uid.
pub const SU_UID: u32 = 0;

/// The eight renaming calls (header contract, `link.c:1-12`).
///
/// `do_unlink` serves both unlink and rmdir, split by `job_call_nr`
/// (`156-159`); the split is a call-surface fact, so two variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkCall {
    /// `LINK`: new name for an old file.
    Link,
    /// `UNLINK`: remove a name.
    Unlink,
    /// `RMDIR`: remove an (empty) directory; same body as unlink.
    Rmdir,
    /// `RENAME`: move a name across (same-device) directories.
    Rename,
    /// `TRUNCATE`: resize by path.
    Truncate,
    /// `FTRUNCATE`: resize by fd.
    Ftruncate,
    /// `SYMLINK`: plant a symbolic link.
    Slink,
    /// `RDLNK`: read a symbolic link.
    Rdlink,
}

impl LinkCall {
    /// Calls whose roads end in directories (last_dir family).
    pub fn ends_in_dir(self) -> bool {
        matches!(
            self,
            Self::Link | Self::Unlink | Self::Rmdir | Self::Rename | Self::Slink
        )
    }
}

/// Which road a lookup walks (`eat_path`/`last_dir`/`advance`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalKind {
    /// Walk the whole road (`eat_path`).
    EatPath,
    /// Stop at the last directory (`last_dir`).
    LastDir,
    /// Step from a held directory (`advance`, sticky re-checks).
    Advance,
}

/// Lock intent requested by a lookup (`l_vmnt_lock`/`l_vnode_lock`).
///
/// Intent, not state: lock *states* live with 05-vnode.md/06-vmnt.md;
/// this layer only records which intent each road asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockIntent {
    /// Shared (`VMNT_READ`/`VNODE_READ`).
    Read,
    /// Exclusive (`VMNT_WRITE`/`VNODE_WRITE`).
    Write,
}

/// One road specification: terminal + lock pair + symlink tail flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupSpec {
    /// Where the road ends.
    pub terminal: TerminalKind,
    /// Mount-table lock intent.
    pub vmnt: LockIntent,
    /// Vnode lock intent.
    pub vnode: LockIntent,
    /// `PATH_RET_SYMLINK`: keep the final symlink unexpanded.
    pub retain_symlink: bool,
}

/// The nine roads walked (`link.c` per-call lookup blocks).
///
/// Link and rename each walk twice (source + destination rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkLookup {
    /// `do_link` name1: whole road, mount-write/node-read (`45-47`).
    LinkSrc,
    /// `do_link` name2: last dir, mount-read/node-write (`54-56`).
    LinkDst,
    /// `do_unlink`/`rmdir`: last dir, write/write, keep symlink (`107-109`).
    UnlinkTarget,
    /// `do_rename` name1: last dir, write/write, keep symlink (`186-189`).
    RenameSrc,
    /// `do_rename` name2: last dir, read/write, keep symlink (`229-231`).
    RenameDst,
    /// `do_truncate`: whole road, read/write (`295-297`).
    TruncateTarget,
    /// `do_slink` name2's dir: last dir, write/write (`398-400`).
    SlinkDir,
    /// `do_rdlink`/`rdlink_direct`: whole road, read/read, keep symlink.
    RdlinkTarget,
}

/// Road table: one spec per road (`link.c` lookup blocks).
pub fn lookup_spec(road: LinkLookup) -> LookupSpec {
    use LinkLookup as R;
    use LockIntent as L;
    use TerminalKind as T;
    match road {
        R::LinkSrc => LookupSpec {
            terminal: T::EatPath,
            vmnt: L::Write,
            vnode: L::Read,
            retain_symlink: false,
        },
        R::LinkDst => LookupSpec {
            terminal: T::LastDir,
            vmnt: L::Read,
            vnode: L::Write,
            retain_symlink: false,
        },
        R::UnlinkTarget => LookupSpec {
            terminal: T::LastDir,
            vmnt: L::Write,
            vnode: L::Write,
            retain_symlink: true,
        },
        R::RenameSrc => LookupSpec {
            terminal: T::LastDir,
            vmnt: L::Write,
            vnode: L::Write,
            retain_symlink: true,
        },
        R::RenameDst => LookupSpec {
            terminal: T::LastDir,
            vmnt: L::Read,
            vnode: L::Write,
            retain_symlink: true,
        },
        R::TruncateTarget => LookupSpec {
            terminal: T::EatPath,
            vmnt: L::Read,
            vnode: L::Write,
            retain_symlink: false,
        },
        R::SlinkDir => LookupSpec {
            terminal: T::LastDir,
            vmnt: L::Write,
            vnode: L::Write,
            retain_symlink: false,
        },
        R::RdlinkTarget => LookupSpec {
            terminal: T::EatPath,
            vmnt: L::Read,
            vnode: L::Read,
            retain_symlink: true,
        },
    }
}

/// Sticky gate (`do_unlink:130-152`, `do_rename:195-217`, same body).
///
/// In a sticky (`S_ISVTX`) directory only the victim's owner or a
/// privileged user may unlink/rename (`140,205`); `SU_UID` passes.
/// A missing victim surfaces the lookup error upstream (caller-side).
pub fn sticky_check(sticky_set: bool, victim_uid: u32, effuid: u32) -> Result<(), LinkError> {
    if !sticky_set {
        return Ok(());
    }
    if victim_uid == effuid || effuid == SU_UID {
        return Ok(());
    }
    Err(LinkError::Perm)
}

/// Negative-length door (`do_truncate:300`, `do_ftruncate:338`).
pub fn check_length(len: i64) -> Result<u64, LinkError> {
    if len < 0 {
        return Err(LinkError::Inval);
    }
    Ok(len as u64)
}

/// Same-size skip (`do_truncate:312-313`, `do_ftruncate:348-353`).
///
/// POSIX keeps file times when the size does not change, so a same-size
/// regular file skips the FS call entirely (`r = OK`).
pub fn same_size_skip(is_reg: bool, cur_size: u64, new_size: u64) -> bool {
    is_reg && cur_size == new_size
}

/// `truncate_vnode` type gate (`link.c:371-372`): regulars and fifos only.
pub fn truncate_type_ok(is_reg: bool, is_fifo: bool) -> Result<(), LinkError> {
    if is_reg || is_fifo {
        return Ok(());
    }
    Err(LinkError::Inval)
}

/// `do_ftruncate` mode gate (`link.c:346-347`): unwritable fd is `EBADF`.
pub fn write_mode_ok(mode_write: bool) -> Result<(), LinkError> {
    if mode_write {
        return Ok(());
    }
    Err(LinkError::BadF)
}

/// Symlink string doors (`do_slink:407-408`) + request length (`414-416`).
///
/// Empty-or-short names are `ENOENT`; over-long names are
/// `ENAMETOOLONG`; the FS gets `len - 1` (the NUL stays home).
pub fn check_slink_len(len: usize) -> Result<usize, LinkError> {
    if len <= 1 {
        return Err(LinkError::NoEnt);
    }
    if len >= POSIX_SYMLINK_MAX {
        return Err(LinkError::NameLong);
    }
    Ok(len - 1)
}

/// `do_rdlink` buffer door (`link.c:487`): past-`SSIZE_MAX` is `EINVAL`.
pub fn check_rdlink_buf(bufsize: u64) -> Result<(), LinkError> {
    if bufsize > SSIZE_MAX_U64 {
        return Err(LinkError::Inval);
    }
    Ok(())
}

/// Who reads the link: inside VFS or the syscall reply path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RdlinkSide {
    /// `rdlink_direct` (`430-466`): path traversal's helper.
    Internal,
    /// `do_rdlink` (`471-508`): the syscall itself.
    Syscall,
}

/// Read channel per side (`456` vs `501`).
///
/// The endpoint and flag travel as a pair: inside reads use `NONE` with
/// flag 1, syscalls use the caller with flag 0. They cannot be mixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RdlinkChannel {
    /// Use the caller's endpoint (`true`) or `NONE` (`false`).
    pub use_caller: bool,
    /// Request flag (1 inside, 0 outside).
    pub flag: u8,
}

/// Channel pairing per side (`link.c:456,501`).
pub fn rdlink_channel(side: RdlinkSide) -> RdlinkChannel {
    match side {
        RdlinkSide::Internal => RdlinkChannel {
            use_caller: false,
            flag: 1,
        },
        RdlinkSide::Syscall => RdlinkChannel {
            use_caller: true,
            flag: 0,
        },
    }
}

/// NUL-terminate rule (`rdlink_direct:459`): terminate when `r > 0`.
pub fn terminate_ok(r: i64) -> bool {
    r > 0
}

/// Cross-device door (`do_link:70-71`, `do_rename:250`): meat never
/// crosses devices.
pub fn same_device(a: u64, b: u64) -> Result<(), LinkError> {
    if a == b {
        return Ok(());
    }
    Err(LinkError::CrossDev)
}

/// Saved-old-name bound (`do_rename:220-225`): past-`PATH_MAX` is
/// `ENAMETOOLONG` (the `>=` counts the NUL).
pub fn old_name_fits(len: usize) -> Result<(), LinkError> {
    if len >= PATH_MAX {
        return Err(LinkError::NameLong);
    }
    Ok(())
}

/// The FS dialogue behind a trait.
///
/// The seven `req_*` downcalls (`req_link/unlink/rmdir/rename/slink/
/// rdlink/ftrunc`) execute FS-side; the FS is the only untestable
/// point, so only the dialogue is abstracted.
pub trait FsLink {
    /// `req_link`: plant a new name on an inode.
    fn link(&mut self, fs: u64, dir: u64, file: u64) -> Result<(), LinkError>;
    /// `req_unlink`: drop a name.
    fn unlink(&mut self, fs: u64, dir: u64) -> Result<(), LinkError>;
    /// `req_rmdir`: drop an empty directory name.
    fn rmdir(&mut self, fs: u64, dir: u64) -> Result<(), LinkError>;
    /// `req_rename`: move a name across same-device directories.
    fn rename(&mut self, fs: u64, old_dir: u64, new_dir: u64) -> Result<(), LinkError>;
    /// `req_slink`: plant a symbolic link of `len` bytes.
    fn slink(&mut self, fs: u64, dir: u64, len: usize) -> Result<(), LinkError>;
    /// `req_rdlink`: read a symbolic link (returns byte count).
    fn rdlink(&mut self, fs: u64, file: u64) -> Result<u64, LinkError>;
    /// `req_ftrunc`: resize an inode.
    fn ftrunc(&mut self, fs: u64, file: u64, size: u64) -> Result<(), LinkError>;
}

/// Scripted FS (test double with programmed answers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedLink {
    /// Programmed `link` answer.
    pub link_result: Result<(), LinkError>,
    /// Programmed `rdlink` answer (byte count).
    pub rdlink_result: Result<u64, LinkError>,
    /// Downcalls made (observable dialogue).
    pub ncalls: u32,
}

impl Default for ScriptedLink {
    fn default() -> Self {
        Self {
            link_result: Ok(()),
            rdlink_result: Ok(0),
            ncalls: 0,
        }
    }
}

impl FsLink for ScriptedLink {
    fn link(&mut self, _fs: u64, _dir: u64, _file: u64) -> Result<(), LinkError> {
        self.ncalls += 1;
        self.link_result
    }
    fn unlink(&mut self, _fs: u64, _dir: u64) -> Result<(), LinkError> {
        self.ncalls += 1;
        Ok(())
    }
    fn rmdir(&mut self, _fs: u64, _dir: u64) -> Result<(), LinkError> {
        self.ncalls += 1;
        Ok(())
    }
    fn rename(&mut self, _fs: u64, _old_dir: u64, _new_dir: u64) -> Result<(), LinkError> {
        self.ncalls += 1;
        Ok(())
    }
    fn slink(&mut self, _fs: u64, _dir: u64, _len: usize) -> Result<(), LinkError> {
        self.ncalls += 1;
        Ok(())
    }
    fn rdlink(&mut self, _fs: u64, _file: u64) -> Result<u64, LinkError> {
        self.ncalls += 1;
        self.rdlink_result
    }
    fn ftrunc(&mut self, _fs: u64, _file: u64, _size: u64) -> Result<(), LinkError> {
        self.ncalls += 1;
        Ok(())
    }
}

/// Refusing FS (test double: every downcall fails with `EIO`).
///
/// Behaves differently from [`ScriptedLink`] (blanket refusal vs
/// programmed answers), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefusingLink;

impl FsLink for RefusingLink {
    fn link(&mut self, _fs: u64, _dir: u64, _file: u64) -> Result<(), LinkError> {
        Err(LinkError::Io)
    }
    fn unlink(&mut self, _fs: u64, _dir: u64) -> Result<(), LinkError> {
        Err(LinkError::Io)
    }
    fn rmdir(&mut self, _fs: u64, _dir: u64) -> Result<(), LinkError> {
        Err(LinkError::Io)
    }
    fn rename(&mut self, _fs: u64, _old_dir: u64, _new_dir: u64) -> Result<(), LinkError> {
        Err(LinkError::Io)
    }
    fn slink(&mut self, _fs: u64, _dir: u64, _len: usize) -> Result<(), LinkError> {
        Err(LinkError::Io)
    }
    fn rdlink(&mut self, _fs: u64, _file: u64) -> Result<u64, LinkError> {
        Err(LinkError::Io)
    }
    fn ftrunc(&mut self, _fs: u64, _file: u64, _size: u64) -> Result<(), LinkError> {
        Err(LinkError::Io)
    }
}

/// L2 contract probe: plant then read through any FS (generic bound use).
pub fn link_then_read<L: FsLink>(fs: &mut L) -> Result<u64, LinkError> {
    fs.link(1, 2, 3)?;
    fs.rdlink(1, 3)
}

/// What the call tells the main loop (ARCH A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkVerdict {
    /// Reply now (status carried separately).
    Done,
}

/// Errors of this module, each mapping to one Minix3 errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkError {
    /// `EXDEV`: meat across devices.
    CrossDev,
    /// `EPERM`: sticky denial.
    Perm,
    /// `ENOTDIR`: road ends outside a directory.
    NotDir,
    /// `ENAMETOOLONG`: over-long symlink or saved old name.
    NameLong,
    /// `ENOENT`: missing name or stubby symlink string.
    NoEnt,
    /// `EINVAL`: negative lengths, wrong vnode kinds, wild buffers.
    Inval,
    /// `EBADF`: unwritable fd on the ftruncate road.
    BadF,
    /// `EIO`: FS-side refusal.
    Io,
}

impl LinkError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::CrossDev => minix_types::EXDEV,
            Self::Perm => minix_types::EPERM,
            Self::NotDir => minix_types::ENOTDIR,
            Self::NameLong => minix_types::ENAMETOOLONG,
            Self::NoEnt => minix_types::ENOENT,
            Self::Inval => minix_types::EINVAL,
            Self::BadF => minix_types::EBADF,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calls_and_roads() {
        // Eight surface calls (`link.c:1-12`, unlink doubling as rmdir).
        let all = [
            LinkCall::Link,
            LinkCall::Unlink,
            LinkCall::Rmdir,
            LinkCall::Rename,
            LinkCall::Truncate,
            LinkCall::Ftruncate,
            LinkCall::Slink,
            LinkCall::Rdlink,
        ];
        assert_eq!(all.len(), 8);
        assert!(LinkCall::Rename.ends_in_dir());
        assert!(!LinkCall::Truncate.ends_in_dir());
        assert!(!LinkCall::Rdlink.ends_in_dir());
        // Road table: terminals, lock pairs, symlink tails.
        let spec = lookup_spec(LinkLookup::LinkSrc);
        assert_eq!(
            spec,
            LookupSpec {
                terminal: TerminalKind::EatPath,
                vmnt: LockIntent::Write,
                vnode: LockIntent::Read,
                retain_symlink: false
            }
        );
        let spec = lookup_spec(LinkLookup::UnlinkTarget);
        assert_eq!(
            (spec.vmnt, spec.vnode, spec.retain_symlink),
            (LockIntent::Write, LockIntent::Write, true)
        );
        // Rename's destination reads the mount but writes the node.
        let spec = lookup_spec(LinkLookup::RenameDst);
        assert_eq!(
            (spec.vmnt, spec.vnode),
            (LockIntent::Read, LockIntent::Write)
        );
        // Reads never ask exclusive node locks.
        let spec = lookup_spec(LinkLookup::RdlinkTarget);
        assert_eq!(
            (spec.vmnt, spec.vnode, spec.retain_symlink),
            (LockIntent::Read, LockIntent::Read, true)
        );
        // Truncate walks whole, slink stops at the dir.
        assert_eq!(
            lookup_spec(LinkLookup::TruncateTarget).terminal,
            TerminalKind::EatPath
        );
        assert_eq!(
            lookup_spec(LinkLookup::SlinkDir).terminal,
            TerminalKind::LastDir
        );
    }

    #[test]
    fn test_sticky_gate() {
        // No sticky bit: free passage (`132`/`197` gate off).
        assert!(sticky_check(false, 100, 200).is_ok());
        // Owner passes, root passes, strangers fail (`140`/`205`).
        assert!(sticky_check(true, 100, 100).is_ok());
        assert!(sticky_check(true, 100, SU_UID).is_ok());
        assert_eq!(sticky_check(true, 100, 200), Err(LinkError::Perm));
        assert_eq!(LinkError::Perm.to_errno(), minix_types::EPERM);
    }

    #[test]
    fn test_truncate_doors() {
        // Negative lengths refuse (`300`/`338`).
        assert_eq!(check_length(-1), Err(LinkError::Inval));
        assert_eq!(check_length(0), Ok(0));
        // Same-size regulars skip the call, keeping times (`312`/`348`).
        assert!(same_size_skip(true, 100, 100));
        assert!(!same_size_skip(true, 100, 200));
        assert!(!same_size_skip(false, 100, 100));
        // Only regulars and fifos truncate (`372`).
        assert!(truncate_type_ok(true, false).is_ok());
        assert!(truncate_type_ok(false, true).is_ok());
        assert_eq!(truncate_type_ok(false, false), Err(LinkError::Inval));
        // Unwritable fds are EBADF, not EINVAL (`346-347`).
        assert!(write_mode_ok(true).is_ok());
        assert_eq!(write_mode_ok(false), Err(LinkError::BadF));
    }

    #[test]
    fn test_slink_and_rdlink_doors() {
        // Stubby strings are ENOENT, long ones ENAMETOOLONG (`407-408`).
        assert_eq!(check_slink_len(0), Err(LinkError::NoEnt));
        assert_eq!(check_slink_len(1), Err(LinkError::NoEnt));
        assert_eq!(check_slink_len(POSIX_SYMLINK_MAX), Err(LinkError::NameLong));
        // The FS gets len - 1: the NUL stays home (`414-416`).
        assert_eq!(check_slink_len(6), Ok(5));
        // Wild buffers refuse (`487`).
        assert!(check_rdlink_buf(100).is_ok());
        assert_eq!(check_rdlink_buf(SSIZE_MAX_U64 + 1), Err(LinkError::Inval));
        // Inside vs outside travel as pairs (`456` vs `501`).
        assert_eq!(
            rdlink_channel(RdlinkSide::Internal),
            RdlinkChannel {
                use_caller: false,
                flag: 1
            }
        );
        assert_eq!(
            rdlink_channel(RdlinkSide::Syscall),
            RdlinkChannel {
                use_caller: true,
                flag: 0
            }
        );
        // Terminate only on positive reads (`459`).
        assert!(terminate_ok(5));
        assert!(!terminate_ok(0));
        assert!(!terminate_ok(-5));
    }

    #[test]
    fn test_device_perm_and_fs() {
        // Meat never crosses devices (`70`/`250`).
        assert!(same_device(3, 3).is_ok());
        assert_eq!(same_device(3, 4), Err(LinkError::CrossDev));
        // Saved old names respect PATH_MAX (`220-225`).
        assert!(old_name_fits(PATH_MAX - 1).is_ok());
        assert_eq!(old_name_fits(PATH_MAX), Err(LinkError::NameLong));
        // L2 contract through any FS: plant then read.
        let mut scripted = ScriptedLink {
            link_result: Ok(()),
            rdlink_result: Ok(11),
            ..Default::default()
        };
        assert_eq!(link_then_read(&mut scripted), Ok(11));
        assert_eq!(scripted.ncalls, 2);
        // Link failures propagate before any read.
        let mut failing = ScriptedLink {
            link_result: Err(LinkError::Io),
            rdlink_result: Ok(11),
            ..Default::default()
        };
        assert_eq!(link_then_read(&mut failing), Err(LinkError::Io));
        assert_eq!(failing.ncalls, 1);
        // Blanket refusal (second impl).
        let mut refusing = RefusingLink;
        assert_eq!(link_then_read(&mut refusing), Err(LinkError::Io));
    }

    #[test]
    fn test_errno_map_covers_link_c() {
        for (err, errno) in [
            (LinkError::CrossDev, minix_types::EXDEV),
            (LinkError::Perm, minix_types::EPERM),
            (LinkError::NotDir, minix_types::ENOTDIR),
            (LinkError::NameLong, minix_types::ENAMETOOLONG),
            (LinkError::NoEnt, minix_types::ENOENT),
            (LinkError::Inval, minix_types::EINVAL),
            (LinkError::BadF, minix_types::EBADF),
            (LinkError::Io, minix_types::EIO),
        ] {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
        // Symlink ceiling and root identity hold.
        assert_eq!(POSIX_SYMLINK_MAX, 255);
        assert_eq!(SU_UID, 0);
    }
}
