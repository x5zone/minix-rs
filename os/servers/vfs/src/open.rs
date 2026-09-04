//! `open` — path-to-fd binding: `open/creat/mknod/mkdir/lseek/close`.
//!
//! Corresponds to Minix3's `open.c:1-727` (`do_open/do_creat/common_open`,
//! `new_node/pipe_open`, `do_mknod/do_mkdir`, `actual_lseek/do_lseek`,
//! `do_close/close_fd`) with the `close_filp` tail in `filedes.c:411-523`.
//!
//! Design decisions (see 15-open-close.md §3):
//! - `AccessMode` enum replaces the `mode_map[4]` table (`open.c:29`)
//! - `OpenFlags` bitflags replace the bare `int oflags`
//! - `FsNodeFactory` trait isolates the FS round-trip (`req_create`)
//! - `decide_pipe_open` is a pure function over the pairing matrix
//! - `Whence` + `checked_add` replace the hand-written overflow compare
//! - `FileType` six-way dispatch yields a pure `OpenOutcome`
//! - `special_close` tabulates the demolition shunt (`close_filp`)

use minix_types::Mode;

/// `R_BIT/W_BIT/X_BIT` (`minix3/minix/include/minix/const.h:117-119`).
pub const R_BIT: Mode = 0o4;
/// See [`R_BIT`].
pub const W_BIT: Mode = 0o2;
/// See [`R_BIT`].
pub const X_BIT: Mode = 0o1;

/// `S_IFMT` mask and file-type values (POSIX `sys/stat.h`, as used by
/// `open.c:148` `switch (vp->v_mode & S_IFMT)`).
pub const S_IFMT: Mode = 0o170000;
/// See [`S_IFMT`].
pub const S_IFREG: Mode = 0o100000;
/// See [`S_IFMT`].
pub const S_IFDIR: Mode = 0o040000;
/// See [`S_IFMT`].
pub const S_IFCHR: Mode = 0o020000;
/// See [`S_IFMT`].
pub const S_IFBLK: Mode = 0o060000;
/// See [`S_IFMT`].
pub const S_IFIFO: Mode = 0o010000;
/// See [`S_IFMT`].
pub const S_IFSOCK: Mode = 0o140000;

/// `O_ACCMODE` mask (`minix3/sys/sys/fcntl.h:67`): bottom two bits.
pub const O_ACCMODE: u32 = 0x00000003;
/// `O_RDONLY` (`fcntl.h:64`): zero, i.e. absence of write intent.
pub const O_RDONLY: u32 = 0x00000000;
/// `O_WRONLY` (`fcntl.h:65`).
pub const O_WRONLY: u32 = 0x00000001;
/// `O_RDWR` (`fcntl.h:66`).
pub const O_RDWR: u32 = 0x00000002;

/// Access intent decoded from `oflags & O_ACCMODE`.
///
/// Replaces `mode_map[oflags & O_ACCMODE]` (`open.c:29,98`): the fourth
/// value (`3`) is unrepresentable here, so the `if (!bits) return EINVAL`
/// guard (`open.c:99`) becomes a conversion failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    /// `O_RDONLY` → read intent.
    ReadOnly,
    /// `O_WRONLY` → write intent.
    WriteOnly,
    /// `O_RDWR` → read + write intent.
    ReadWrite,
}

impl TryFrom<u32> for AccessMode {
    type Error = OpenError;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v & O_ACCMODE {
            O_RDONLY => Ok(Self::ReadOnly),
            O_WRONLY => Ok(Self::WriteOnly),
            O_RDWR => Ok(Self::ReadWrite),
            _ => Err(OpenError::Inval),
        }
    }
}

impl AccessMode {
    /// Permission bits this intent requires (`mode_map` row as a value).
    pub fn bits(self) -> Mode {
        match self {
            Self::ReadOnly => R_BIT,
            Self::WriteOnly => W_BIT,
            Self::ReadWrite => R_BIT | W_BIT,
        }
    }
}

bitflags::bitflags! {
    /// Permission bits `R/W/X` as a set (`const.h:117-119`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AccessBits: u32 {
        const R = 0o4;
        const W = 0o2;
        const X = 0o1;
    }
}

impl From<AccessMode> for AccessBits {
    fn from(m: AccessMode) -> Self {
        match m {
            AccessMode::ReadOnly => Self::R,
            AccessMode::WriteOnly => Self::W,
            AccessMode::ReadWrite => Self::R | Self::W,
        }
    }
}

bitflags::bitflags! {
    /// `O_*` open flags (`minix3/sys/sys/fcntl.h:64-136`).
    ///
    /// Only the flags `common_open` inspects are modelled; the rest ride
    /// along untouched in `filp_flags` (`open.c:136`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OpenFlags: u32 {
        const WRONLY   = 0x00000001;
        const RDWR     = 0x00000002;
        const NONBLOCK = 0x00000004;
        const APPEND   = 0x00000008;
        const CREAT    = 0x00000200;
        const TRUNC    = 0x00000400;
        const EXCL     = 0x00000800;
        const NOCTTY   = 0x00008000;
        const CLOEXEC  = 0x00400000;
    }
}

impl OpenFlags {
    /// Intent encoded in the bottom two bits (`O_ACCMODE`).
    pub fn access(self) -> Result<AccessMode, OpenError> {
        AccessMode::try_from(self.bits())
    }
}

/// Arguments shared by the `open` / `creat` entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenArgs {
    /// Raw `oflags` from the caller.
    pub oflags: OpenFlags,
    /// Creation mode (`omode`), meaningful only with `CREAT`.
    pub mode: Mode,
}

impl OpenArgs {
    /// `do_open` gate (`open.c:46`): `O_CREAT` must be absent.
    pub fn validate_for_open(self) -> Result<AccessMode, OpenError> {
        if self.oflags.contains(OpenFlags::CREAT) {
            return Err(OpenError::Inval);
        }
        self.oflags.access()
    }

    /// `do_creat` gate (`open.c:71`): `O_CREAT` must be present.
    pub fn validate_for_creat(self) -> Result<AccessMode, OpenError> {
        if !self.oflags.contains(OpenFlags::CREAT) {
            return Err(OpenError::Inval);
        }
        self.oflags.access()
    }
}

/// File type from `v_mode & S_IFMT` (`open.c:148` six-way `switch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// `S_IFREG`: regular file (truncation candidate).
    Regular,
    /// `S_IFDIR`: directory (writable open rejected).
    Directory,
    /// `S_IFCHR`: character device (driver open).
    Char,
    /// `S_IFBLK`: block device (driver open + mount scan).
    Block,
    /// `S_IFIFO`: named pipe (PFS mapping + pairing).
    Fifo,
    /// `S_IFSOCK`: socket (`EOPNOTSUPP` here; sockets live in 22/24).
    Socket,
    /// Anything else: `EIO` with a diagnostic (`open.c:270-273`).
    Unknown(Mode),
}

impl From<Mode> for FileType {
    fn from(mode: Mode) -> Self {
        match mode & S_IFMT {
            S_IFREG => Self::Regular,
            S_IFDIR => Self::Directory,
            S_IFCHR => Self::Char,
            S_IFBLK => Self::Block,
            S_IFIFO => Self::Fifo,
            S_IFSOCK => Self::Socket,
            other => Self::Unknown(other),
        }
    }
}

/// Which driver family an open delegates to (handled in 19-22, not here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    /// `cdev_open` (`open.c:166`).
    Char,
    /// `bdev_open` (`open.c:176`).
    Block,
}

/// Pure dispatch verdict for an existing vnode (`open.c:148-274`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    /// Regular path: permission check already conceptually passed.
    Proceed,
    /// Regular file with `O_TRUNC`: needs a write check + truncate.
    NeedTruncate,
    /// Open must fail with this error.
    Reject(OpenError),
    /// FIFO with no peer and blocking mode: suspend until one arrives.
    Suspend,
    /// Character / block device: hand to the driver layer (19-22).
    Delegate(DeviceClass),
}

/// Pure six-way dispatch (`open.c:148-274`, driver calls left to 19-22).
pub fn dispatch_open(ft: FileType, bits: AccessBits, oflags: OpenFlags) -> OpenOutcome {
    match ft {
        FileType::Regular => {
            if oflags.contains(OpenFlags::TRUNC) {
                OpenOutcome::NeedTruncate
            } else {
                OpenOutcome::Proceed
            }
        }
        FileType::Directory => {
            if bits.contains(AccessBits::W) {
                OpenOutcome::Reject(OpenError::IsDir)
            } else {
                OpenOutcome::Proceed
            }
        }
        FileType::Char => OpenOutcome::Delegate(DeviceClass::Char),
        FileType::Block => OpenOutcome::Delegate(DeviceClass::Block),
        FileType::Fifo => OpenOutcome::Proceed,
        FileType::Socket => OpenOutcome::Reject(OpenError::NotSup),
        FileType::Unknown(_) => OpenOutcome::Reject(OpenError::Io),
    }
}

/// Verdict of the pipe-pairing check (`pipe_open`, `open.c:483-508`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeOpenDecision {
    /// A peer is present (or none is needed): open now.
    OpenNow,
    /// No peer and blocking mode: suspend (`FP_BLOCKED_ON_POPEN`).
    Suspend,
    /// Peers were already suspended: wake them (`release`, `open.c:505`).
    RevivePeers(u32),
    /// Open must fail with this error.
    Reject(OpenError),
}

/// Pure pipe-pairing decision (`open.c:491-506`).
///
/// - `bits`: this opener's intent (`R` and/or `W`).
/// - `peer`: intent of an already-open filp on the same vnode, if any.
/// - `nonblock`: `O_NONBLOCK` present.
/// - `suspended`: `susp_count` waiters hanging on the pipe.
pub fn decide_pipe_open(
    bits: AccessBits,
    peer: Option<AccessBits>,
    nonblock: bool,
    suspended: u32,
) -> PipeOpenDecision {
    // Read+write on one fd has no peer semantics (`open.c:491`).
    if bits.contains(AccessBits::R) && bits.contains(AccessBits::W) {
        return PipeOpenDecision::Reject(OpenError::Nxio);
    }
    match peer {
        None => {
            if nonblock {
                // A blocking writer with no reader fails; a blocking
                // reader with no writer succeeds (`open.c:496-497` only
                // rejects the `W_BIT` case).
                if bits.contains(AccessBits::W) {
                    PipeOpenDecision::Reject(OpenError::Nxio)
                } else {
                    PipeOpenDecision::OpenNow
                }
            } else {
                PipeOpenDecision::Suspend
            }
        }
        Some(_) => {
            if suspended > 0 {
                PipeOpenDecision::RevivePeers(suspended)
            } else {
                PipeOpenDecision::OpenNow
            }
        }
    }
}

/// Directory context for node creation (`last_dir` result).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirCtx {
    /// File-server endpoint owning the parent directory.
    pub fs_e: i32,
    /// Inode number of the parent directory.
    pub inode_nr: u64,
}

/// Newly created node details (`node_details`, `request.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeDetails {
    /// File-server endpoint owning the node.
    pub fs_e: i32,
    /// Inode number assigned by the file server.
    pub inode_nr: u64,
    /// Full mode including type bits.
    pub mode: Mode,
    /// Initial size (zero for fresh nodes).
    pub size: i64,
    /// Owner / group assigned at creation.
    pub uid: u32,
    /// See [`NodeDetails::uid`].
    pub gid: u32,
    /// Device id for device nodes.
    pub dev: u64,
}

/// Outcome of the create-or-find step (`new_node`, `open.c:299-477`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationOutcome {
    /// `ENOENT` was observed and `req_create` succeeded (`open.c:349-455`).
    Created(NodeDetails),
    /// The final component exists (`EEXIST`, `open.c:459`).
    Exists,
    /// Resolution failed for another reason (`err_code`, `open.c:462`).
    Absent(OpenError),
}

/// File-server hook for node creation (`req_create`, `open.c:363`).
///
/// Isolates the FS round-trip so the creation machine is unit-testable
/// without MFS/PFS (cf. Redox `Scheme::open` isolating the scheme).
pub trait FsNodeFactory {
    /// Create `name` under `dir` with `mode`; map FS errors to [`OpenError`].
    fn create(&self, dir: &DirCtx, name: &str, mode: Mode) -> Result<NodeDetails, OpenError>;
}

/// In-memory factory: creation always succeeds (test double).
#[derive(Debug, Default, Clone, Copy)]
pub struct MemFs;

impl FsNodeFactory for MemFs {
    fn create(&self, dir: &DirCtx, _name: &str, mode: Mode) -> Result<NodeDetails, OpenError> {
        Ok(NodeDetails {
            fs_e: dir.fs_e,
            inode_nr: 1,
            mode,
            size: 0,
            uid: 0,
            gid: 0,
            dev: 0,
        })
    }
}

/// Read-only factory: creation always fails with `EACCES` (test double).
///
/// Behaves differently from [`MemFs`] (success vs refusal), satisfying the
/// "two behaviorally different impls" rule for traits.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReadOnlyFs;

impl FsNodeFactory for ReadOnlyFs {
    fn create(&self, _dir: &DirCtx, _name: &str, _mode: Mode) -> Result<NodeDetails, OpenError> {
        Err(OpenError::Acces)
    }
}

/// Create-or-find step over an abstract factory (`new_node` core).
///
/// - `last_exists`: whether `advance` found the final component.
/// - `last_err`: the `err_code` left by `advance` when it did not.
/// - `excl`: `O_EXCL` present (existing name is an error).
pub fn resolve_or_create<F: FsNodeFactory>(
    factory: &F,
    dir: &DirCtx,
    name: &str,
    mode: Mode,
    last_exists: bool,
    last_err: OpenError,
    excl: bool,
) -> CreationOutcome {
    if last_exists {
        // `open.c:459`: the name exists; `O_EXCL` turns that into `EEXIST`.
        let _ = excl;
        return CreationOutcome::Exists;
    }
    if last_err != OpenError::NoEnt {
        // Some failure other than "absent" (`open.c:462`).
        return CreationOutcome::Absent(last_err);
    }
    match factory.create(dir, name, mode) {
        Ok(details) => CreationOutcome::Created(details),
        Err(OpenError::Exist) => CreationOutcome::Exists,
        Err(e) => CreationOutcome::Absent(e),
    }
}

/// `lseek` origin (`SEEK_SET/SEEK_CUR/SEEK_END`, `open.c:623-625`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whence {
    /// `SEEK_SET`: from the start of the file.
    Set,
    /// `SEEK_CUR`: from the current position.
    Cur,
    /// `SEEK_END`: from the end of the file.
    End,
}

impl TryFrom<i32> for Whence {
    type Error = OpenError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        // libc values: `SEEK_SET 0 / SEEK_CUR 1 / SEEK_END 2`.
        match v {
            0 => Ok(Self::Set),
            1 => Ok(Self::Cur),
            2 => Ok(Self::End),
            _ => Err(OpenError::Inval),
        }
    }
}

/// Pure offset computation (`actual_lseek`, `open.c:622-646`).
///
/// Returns the new position. A computed position equal to the current one
/// skips the read-ahead inhibit (`open.c:639`), reported via the flag.
pub fn seek_pos(
    whence: Whence,
    cur: i64,
    size: i64,
    offset: i64,
    is_fifo: bool,
) -> Result<(i64, bool), OpenError> {
    // No `lseek` on pipes (`open.c:616-619`).
    if is_fifo {
        return Err(OpenError::Pipe);
    }
    let base = match whence {
        Whence::Set => 0,
        Whence::Cur => cur,
        Whence::End => size,
    };
    // Replaces the hand-written two-sided compare (`open.c:632-635`).
    let newpos = base.checked_add(offset).ok_or(OpenError::Overflow)?;
    // `checked_add` already rules out wrap; `i64` cannot overflow here.
    Ok((newpos, newpos != cur))
}

/// Node kind for `mknod` / `mkdir` (`open.c:514-598`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// `S_ISFIFO(mode)`: anyone may create; others need super-user.
    Fifo,
    /// Any other type bits: super-user only (`open.c:538`).
    Privileged(Mode),
}

impl From<Mode> for NodeKind {
    fn from(mode: Mode) -> Self {
        if mode & S_IFMT == S_IFIFO {
            Self::Fifo
        } else {
            Self::Privileged(mode & S_IFMT)
        }
    }
}

/// `mknod` privilege gate (`open.c:538-539`).
pub fn check_mknod_perm(kind: NodeKind, is_super: bool) -> Result<(), OpenError> {
    match kind {
        NodeKind::Fifo => Ok(()),
        NodeKind::Privileged(_) => {
            if is_super {
                Ok(())
            } else {
                Err(OpenError::Perm)
            }
        }
    }
}

/// Verdict of the special-file close shunt (`close_filp`, `filedes.c:435-493`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    /// Reference dropped, still shared: just unlock.
    Closed,
    /// Last reference dropped: release the vnode (`put_vnode` + `CLOSED`).
    ClosedLast,
    /// Socket close that may block: suspend (`sdev_close`, `filedes.c:478`).
    Suspend,
}

/// Pure close shunt by file type and reference state (`filedes.c:435-508`).
///
/// - `ft`: type of the vnode being closed.
/// - `last`: whether this is the final reference (`filp_count - 1 == 0`).
/// - `may_suspend`: caller allows suspension (only `close(2)` sets it).
/// - `nonblock`: `O_NONBLOCK` clears `may_suspend` (`filedes.c:475-476`).
pub fn special_close(ft: FileType, last: bool, may_suspend: bool, nonblock: bool) -> CloseOutcome {
    if !last {
        return CloseOutcome::Closed;
    }
    match ft {
        FileType::Socket => {
            if may_suspend && !nonblock {
                CloseOutcome::Suspend
            } else {
                CloseOutcome::ClosedLast
            }
        }
        // `S_ISCHR/S_ISBLK/S_ISFIFO/regular`: driver closes ignore errors
        // (`filedes.c:453,461`), pipes `release()` waiters, then the
        // `count-- → 0 → put_vnode` tail runs in all cases.
        _ => CloseOutcome::ClosedLast,
    }
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// No invented codes: every variant names the errno it becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// `EINVAL`: bad access bits, bad `whence`, wrong entry side.
    Inval,
    /// `EEXIST`: name exists with `O_EXCL`.
    Exist,
    /// `EISDIR`: writable open of a directory (`open.c:160`).
    IsDir,
    /// `ENOTDIR`: non-directory parent (`open.c:549,588`).
    NotDir,
    /// `EPERM`: non-super-user `mknod` (`open.c:539`).
    Perm,
    /// `ENXIO`: read+write pipe open, or blocking write with no reader.
    Nxio,
    /// `EOPNOTSUPP`: opening a socket via `open` (`open.c:267`).
    NotSup,
    /// `ESPIPE`: `lseek` on a pipe (`open.c:618`).
    Pipe,
    /// `EOVERFLOW`: `lseek` position wraps (`open.c:633,635`).
    Overflow,
    /// `EBADF`: `get_filp2` fails (`open.c:612,699`).
    BadF,
    /// `ENOENT`: final component absent and no `O_CREAT` (`open.c:349`).
    NoEnt,
    /// `EACCES`: `forbidden` refuses (`open.c:146`).
    Acces,
    /// `EIO`: unknown file type (`open.c:273`).
    Io,
}

impl OpenError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Inval => minix_types::EINVAL,
            Self::Exist => minix_types::EEXIST,
            Self::IsDir => minix_types::EISDIR,
            Self::NotDir => minix_types::ENOTDIR,
            Self::Perm => minix_types::EPERM,
            Self::Nxio => minix_types::ENXIO,
            Self::NotSup => minix_types::EOPNOTSUPP,
            Self::Pipe => minix_types::ESPIPE,
            Self::Overflow => minix_types::EOVERFLOW,
            Self::BadF => minix_types::EBADF,
            Self::NoEnt => minix_types::ENOENT,
            Self::Acces => minix_types::EACCES,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(flags: OpenFlags) -> OpenArgs {
        OpenArgs {
            oflags: flags,
            mode: 0o644,
        }
    }

    #[test]
    fn test_access_mode_map_four_values() {
        // `mode_map[4]` parity (`open.c:29`): 0→R 1→W 2→RW 3→EINVAL.
        assert_eq!(AccessMode::try_from(0).unwrap().bits(), R_BIT);
        assert_eq!(AccessMode::try_from(1).unwrap().bits(), W_BIT);
        assert_eq!(AccessMode::try_from(2).unwrap().bits(), R_BIT | W_BIT);
        assert_eq!(AccessMode::try_from(3).unwrap_err(), OpenError::Inval);
        assert_eq!(OpenError::Inval.to_errno(), minix_types::EINVAL);
        // Upper bits are masked by `O_ACCMODE`.
        assert_eq!(AccessMode::try_from(0o1000).unwrap(), AccessMode::ReadOnly);
    }

    #[test]
    fn test_open_creat_dual_gate() {
        // `do_open` rejects `O_CREAT` (`open.c:46`); `do_creat` requires it (`open.c:71`).
        let creat = OpenFlags::CREAT;
        assert_eq!(
            args(creat).validate_for_open().unwrap_err(),
            OpenError::Inval
        );
        assert!(args(creat).validate_for_creat().is_ok());
        assert!(args(OpenFlags::empty()).validate_for_open().is_ok());
        assert_eq!(
            args(OpenFlags::empty()).validate_for_creat().unwrap_err(),
            OpenError::Inval
        );
    }

    #[test]
    fn test_dispatch_six_branches() {
        let rw = AccessBits::R | AccessBits::W;
        // Regular: `O_TRUNC` arms the truncate path (`open.c:151`).
        assert_eq!(
            dispatch_open(FileType::Regular, rw, OpenFlags::TRUNC),
            OpenOutcome::NeedTruncate
        );
        assert_eq!(
            dispatch_open(FileType::Regular, rw, OpenFlags::empty()),
            OpenOutcome::Proceed
        );
        // Directory: write intent rejected (`open.c:160`).
        assert_eq!(
            dispatch_open(FileType::Directory, AccessBits::W, OpenFlags::empty()),
            OpenOutcome::Reject(OpenError::IsDir)
        );
        assert_eq!(OpenError::IsDir.to_errno(), minix_types::EISDIR);
        assert_eq!(
            dispatch_open(FileType::Directory, AccessBits::R, OpenFlags::empty()),
            OpenOutcome::Proceed
        );
        // Devices delegate; sockets and unknowns reject.
        assert_eq!(
            dispatch_open(FileType::Char, rw, OpenFlags::empty()),
            OpenOutcome::Delegate(DeviceClass::Char)
        );
        assert_eq!(
            dispatch_open(FileType::Block, rw, OpenFlags::empty()),
            OpenOutcome::Delegate(DeviceClass::Block)
        );
        assert_eq!(
            dispatch_open(FileType::Socket, rw, OpenFlags::empty()),
            OpenOutcome::Reject(OpenError::NotSup)
        );
        assert_eq!(
            dispatch_open(FileType::Unknown(0), rw, OpenFlags::empty()),
            OpenOutcome::Reject(OpenError::Io)
        );
    }

    #[test]
    fn test_file_type_from_mode() {
        assert_eq!(FileType::from(S_IFREG | 0o644), FileType::Regular);
        assert_eq!(FileType::from(S_IFDIR | 0o755), FileType::Directory);
        assert_eq!(FileType::from(S_IFCHR | 0o600), FileType::Char);
        assert_eq!(FileType::from(S_IFBLK | 0o600), FileType::Block);
        assert_eq!(FileType::from(S_IFIFO | 0o644), FileType::Fifo);
        assert_eq!(FileType::from(S_IFSOCK | 0o777), FileType::Socket);
    }

    #[test]
    fn test_pipe_pairing_matrix() {
        let r = AccessBits::R;
        let w = AccessBits::W;
        // Read+write on one fd: no peer semantics (`open.c:491`).
        assert_eq!(
            decide_pipe_open(r | w, None, false, 0),
            PipeOpenDecision::Reject(OpenError::Nxio)
        );
        // No peer, blocking: suspend (`open.c:498-502`).
        assert_eq!(
            decide_pipe_open(r, None, false, 0),
            PipeOpenDecision::Suspend
        );
        assert_eq!(
            decide_pipe_open(w, None, false, 0),
            PipeOpenDecision::Suspend
        );
        // No peer, nonblocking: writer fails, reader opens (`open.c:496-497`).
        assert_eq!(
            decide_pipe_open(w, None, true, 0),
            PipeOpenDecision::Reject(OpenError::Nxio)
        );
        assert_eq!(
            decide_pipe_open(r, None, true, 0),
            PipeOpenDecision::OpenNow
        );
        // Peer present: open; suspended peers revive (`open.c:504-505`).
        assert_eq!(
            decide_pipe_open(r, Some(w), false, 0),
            PipeOpenDecision::OpenNow
        );
        assert_eq!(
            decide_pipe_open(w, Some(r), false, 3),
            PipeOpenDecision::RevivePeers(3)
        );
    }

    #[test]
    fn test_create_or_find() {
        let dir = DirCtx {
            fs_e: 2,
            inode_nr: 7,
        };
        let mem = MemFs;
        let ro = ReadOnlyFs;
        // Existing name: `EEXIST` semantics regardless of factory.
        assert_eq!(
            resolve_or_create(&mem, &dir, "f", 0o100644, true, OpenError::NoEnt, true),
            CreationOutcome::Exists
        );
        // Absent + `MemFs`: created with the requested mode.
        match resolve_or_create(&mem, &dir, "f", 0o100644, false, OpenError::NoEnt, false) {
            CreationOutcome::Created(d) => {
                assert_eq!(d.fs_e, 2);
                assert_eq!(d.mode, 0o100644);
                assert_eq!(d.size, 0);
            }
            other => panic!("expected Created, got {other:?}"),
        }
        // Absent + `ReadOnlyFs`: refusal propagates.
        assert_eq!(
            resolve_or_create(&ro, &dir, "f", 0o100644, false, OpenError::NoEnt, false),
            CreationOutcome::Absent(OpenError::Acces)
        );
        // Non-ENOENT failure passes through untouched.
        assert_eq!(
            resolve_or_create(&mem, &dir, "f", 0, false, OpenError::Acces, false),
            CreationOutcome::Absent(OpenError::Acces)
        );
    }

    #[test]
    fn test_factories_differ() {
        // Gate D: the two `FsNodeFactory` impls behave differently.
        let dir = DirCtx {
            fs_e: 0,
            inode_nr: 0,
        };
        assert!(MemFs.create(&dir, "f", 0o100644).is_ok());
        assert_eq!(
            ReadOnlyFs.create(&dir, "f", 0o100644).unwrap_err(),
            OpenError::Acces
        );
        // Polymorphic use through the trait bound.
        fn via<F: FsNodeFactory>(f: &F, dir: &DirCtx) -> bool {
            f.create(dir, "f", 0o100644).is_ok()
        }
        assert!(via(&MemFs, &dir));
        assert!(!via(&ReadOnlyFs, &dir));
    }

    #[test]
    fn test_seek_origins_and_overflow() {
        // `SEEK_SET/CUR/END` bases (`open.c:623-625`).
        assert_eq!(
            seek_pos(Whence::Set, 100, 1000, 10, false).unwrap(),
            (10, true)
        );
        assert_eq!(
            seek_pos(Whence::Cur, 100, 1000, -30, false).unwrap(),
            (70, true)
        );
        assert_eq!(
            seek_pos(Whence::End, 100, 1000, 0, false).unwrap(),
            (1000, true)
        );
        // Unchanged position skips the inhibit (`open.c:639`).
        assert_eq!(
            seek_pos(Whence::Cur, 100, 1000, 0, false).unwrap(),
            (100, false)
        );
        // `TryFrom` rejects bad `whence` (`open.c:626`).
        assert_eq!(Whence::try_from(7).unwrap_err(), OpenError::Inval);
        // Both wrap directions overflow (`open.c:632-635`).
        assert_eq!(
            seek_pos(Whence::Set, 0, 0, i64::MAX, false).unwrap(),
            (i64::MAX, true)
        );
        assert_eq!(
            seek_pos(Whence::Cur, 5, 0, i64::MAX, false).unwrap_err(),
            OpenError::Overflow
        );
        assert_eq!(
            seek_pos(Whence::Cur, -5, 0, i64::MIN, false).unwrap_err(),
            OpenError::Overflow
        );
        assert_eq!(OpenError::Overflow.to_errno(), minix_types::EOVERFLOW);
        // No `lseek` on pipes (`open.c:616-619`).
        assert_eq!(
            seek_pos(Whence::Set, 0, 0, 0, true).unwrap_err(),
            OpenError::Pipe
        );
        assert_eq!(OpenError::Pipe.to_errno(), minix_types::ESPIPE);
    }

    #[test]
    fn test_mknod_privilege_gate() {
        // Non-super-user may only make FIFOs (`open.c:538-539`).
        assert!(check_mknod_perm(NodeKind::Fifo, false).is_ok());
        assert!(check_mknod_perm(NodeKind::Fifo, true).is_ok());
        let dev = NodeKind::from(S_IFCHR | 0o600);
        assert_eq!(check_mknod_perm(dev, false).unwrap_err(), OpenError::Perm);
        assert_eq!(OpenError::Perm.to_errno(), minix_types::EPERM);
        assert!(check_mknod_perm(dev, true).is_ok());
    }

    #[test]
    fn test_close_shunt() {
        // Shared reference: plain drop, no vnode release.
        assert_eq!(
            special_close(FileType::Regular, false, true, false),
            CloseOutcome::Closed
        );
        // Last reference: release in every non-socket case.
        assert_eq!(
            special_close(FileType::Regular, true, true, false),
            CloseOutcome::ClosedLast
        );
        assert_eq!(
            special_close(FileType::Fifo, true, true, false),
            CloseOutcome::ClosedLast
        );
        assert_eq!(
            special_close(FileType::Block, true, true, false),
            CloseOutcome::ClosedLast
        );
        // Sockets may suspend only when allowed and blocking.
        assert_eq!(
            special_close(FileType::Socket, true, true, false),
            CloseOutcome::Suspend
        );
        assert_eq!(
            special_close(FileType::Socket, true, false, false),
            CloseOutcome::ClosedLast
        );
        assert_eq!(
            special_close(FileType::Socket, true, true, true),
            CloseOutcome::ClosedLast
        );
    }

    #[test]
    fn test_errno_map_covers_open_c() {
        // Every error the C file can produce maps to the right errno.
        let cases = [
            (OpenError::Inval, minix_types::EINVAL),
            (OpenError::Exist, minix_types::EEXIST),
            (OpenError::IsDir, minix_types::EISDIR),
            (OpenError::NotDir, minix_types::ENOTDIR),
            (OpenError::Perm, minix_types::EPERM),
            (OpenError::Nxio, minix_types::ENXIO),
            (OpenError::NotSup, minix_types::EOPNOTSUPP),
            (OpenError::Pipe, minix_types::ESPIPE),
            (OpenError::Overflow, minix_types::EOVERFLOW),
            (OpenError::BadF, minix_types::EBADF),
            (OpenError::NoEnt, minix_types::ENOENT),
            (OpenError::Acces, minix_types::EACCES),
            (OpenError::Io, minix_types::EIO),
        ];
        for (err, errno) in cases {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
