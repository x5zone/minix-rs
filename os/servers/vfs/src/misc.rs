//! `misc` — the edges: asking, syncing, borrowing, answering, knobs, clocks.
//!
//! Corresponds to Minix3's `do_getsysinfo`/`do_sync`/`do_fsync`/`dupvm`/
//! `do_vm_call`/`do_svrctl` (`misc.c:52-112,276-498,797-898`), `do_utimens`
//! (`time.c:26-155`), `do_gcov_flush` (`gcov.c:10-73`), `do_getrusage`/
//! `panic_hook` (`misc.c:989-1006`).
//!
//! Design decisions (see 31-misc-queries.md §3):
//! - `VfsCallNum` reuse types the eight calls (no second enum, 09 owns it)
//! - `SysinfoWhat` + root/len gates + `light_task` type the asking door
//! - `MntSync` + `sync_targets`/`fsync_targets` type the double sweep
//! - `dupvm_gate` + `LookupAnswer` type the borrowing door
//! - `VmVfsReq` + source gate + `Suspend` type the VM word
//! - `SvrctlCmd`/`SvrctlKey` + `HostAction` type the knobs
//! - `UtimensStyle` + flag/owner gates + `NsecSpec` type the clock
//! - `GcovTarget` + label/endpt/grant gates type the probe
//! - `MiscFs` types the FS dialogue (`ScriptedMisc` vs `RefusingMisc`)
//!
//! Scope note: PM-side execution (`pm_reboot`/`free_proc`, 10) and DS
//! dispatch (`ds_event`, 09+19) stay out; table locks (`lock_vmnt`,
//! 06), fd allocation (`get_fd`, 14), path walking (`eat_path`, 13),
//! permission verdicts (`forbidden`/`read_only`, 29), FS envelopes
//! (`req_sync`/`req_utime`, 12), and message packing (09/ipc) stay out.
//! This module only decides: gates, sweeps, answers, and dialogue.
//!
//! Linux models the same core as `sync_filesystems` (broadcast over
//! superblocks), `do_sysinfo`-style table copies, `utimensat` (the same
//! NOW/OMIT/explicit triple), and debugfs-flavoured introspection;
//! Redox models it as per-handle `sync` plus scheme reads for status.
//! Here the sweep is the core and [`MiscFs`] is the per-filesystem answer.

extern crate alloc;

use alloc::vec::Vec;

use minix_types::Endpoint;

use crate::device_map::{IOC_IN, IOC_OUT};
use crate::open::FileType;
use crate::protect::readonly_gate;

/// `VFS_BASE` offsets of the eight calls (`callnr.h:88-120`, 09 owns them).
///
/// Listed here so the call surface reads in one place; the enum itself is
/// [`crate::call_table::VfsCallNum`] and is not redefined (one truth).
pub const VFS_SYNC_OFF: u32 = 16;
/// `VFS_FSYNC` offset (`callnr.h:104`).
pub const VFS_FSYNC_OFF: u32 = 32;
/// `VFS_UTIMENS` offset (`callnr.h:109`).
pub const VFS_UTIMENS_OFF: u32 = 37;
/// `VFS_VMCALL` offset (`callnr.h:110`).
pub const VFS_VMCALL_OFF: u32 = 38;
/// `VFS_GETRUSAGE` offset (`callnr.h:114`, obsolete).
pub const VFS_GETRUSAGE_OFF: u32 = 42;
/// `VFS_SVRCTL` offset (`callnr.h:115`).
pub const VFS_SVRCTL_OFF: u32 = 43;
/// `VFS_GCOV_FLUSH` offset (`callnr.h:116`).
pub const VFS_GCOV_FLUSH_OFF: u32 = 44;
/// `VFS_GETSYSINFO` offset (`callnr.h:120`).
pub const VFS_GETSYSINFO_OFF: u32 = 48;

/// `SI_PROC_TAB` (`minix3/minix/include/minix/sysinfo.h:11`).
pub const SI_PROC_TAB: u32 = 2;
/// `SI_DMAP_TAB` (`sysinfo.h:12`).
pub const SI_DMAP_TAB: u32 = 3;
/// `SI_CALL_STATS` (`sysinfo.h:14`, `ENABLE_SYSCALL_STATS` gated, no table here).
pub const SI_CALL_STATS: u32 = 9;
/// `SI_PROCLIGHT_TAB` (`sysinfo.h:17`).
pub const SI_PROCLIGHT_TAB: u32 = 13;

/// Asking surface (`do_getsysinfo:72-103`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysinfoWhat {
    /// `SI_PROC_TAB`: whole process table copy.
    ProcTab,
    /// `SI_DMAP_TAB`: device-driver mappings.
    DmapTab,
    /// `SI_PROCLIGHT_TAB`: light table, filled on request.
    ProcLightTab,
}

impl SysinfoWhat {
    /// Decode the wire `what`; statistics (`9`) and wild values refuse
    /// (`default:104-105`; `9` has no table outside the stats build).
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            SI_PROC_TAB => Some(Self::ProcTab),
            SI_DMAP_TAB => Some(Self::DmapTab),
            SI_PROCLIGHT_TAB => Some(Self::ProcLightTab),
            _ => None,
        }
    }
}

/// Root door (`do_getsysinfo:70`): asking leaks tables, root only.
pub fn sysinfo_root_gate(is_root: bool) -> Result<(), MiscError> {
    if !is_root {
        return Err(MiscError::Perm);
    }
    Ok(())
}

/// Length door (`do_getsysinfo:108-109`): the copy must fit exactly.
pub fn sysinfo_len_gate(len: u64, buf_size: u64) -> Result<(), MiscError> {
    if len != buf_size {
        return Err(MiscError::Inval);
    }
    Ok(())
}

/// How a process is blocked, as far as the light table cares
/// (`do_getsysinfo:87-93`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// `FP_BLOCKED_ON_CDEV`: task reads straight off the record.
    Cdev,
    /// `FP_BLOCKED_ON_SDEV`: task resolves through the socket map.
    Sdev,
    /// Anything else: no task.
    Other,
}

/// Light-table task cell (`fpl_task`, `do_getsysinfo:87-94`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightTask {
    /// Some driver owns the wait.
    Driver(Endpoint),
    /// `NONE`: nobody does.
    NoDriver,
}

/// Derive the task cell (`do_getsysinfo:87-94`).
///
/// The socket-map lookup itself runs caller-side (19 owns smap); its
/// outcome arrives as `smap_endpt` (`None` = lookup missed, `90`).
pub fn light_task(
    kind: BlockKind,
    cdev_endpt: Endpoint,
    smap_endpt: Option<Endpoint>,
) -> LightTask {
    match kind {
        BlockKind::Cdev => LightTask::Driver(cdev_endpt),
        BlockKind::Sdev => match smap_endpt {
            Some(ep) => LightTask::Driver(ep),
            None => LightTask::NoDriver,
        },
        BlockKind::Other => LightTask::NoDriver,
    }
}

/// One mount's sync-relevant facts (`do_sync:284-285`, `do_fsync:317-318`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MntSync {
    /// `m_dev != NO_DEV`.
    pub has_dev: bool,
    /// `m_fs_e != NONE`.
    pub has_fs: bool,
    /// `m_root_node != NULL`.
    pub has_root: bool,
    /// Hosting FS (`m_fs_e`).
    pub fs: Endpoint,
    /// Device (`m_dev`, the fsync filter key).
    pub dev: u64,
}

/// Whom `do_sync` flushes (`do_sync:281-291`).
///
/// Every mounted, served, rooted volume — the sweep is pure selection;
/// locking (`lock_vmnt`, 06) and the downcall (`req_sync`, 12) run
/// caller-side. A lock failure aborts the C loop (`282-283`); that error
/// rides the caller, not this selector.
pub fn sync_targets(mnts: &[MntSync]) -> Vec<Endpoint> {
    mnts.iter()
        .filter(|m| m.has_dev && m.has_fs && m.has_root)
        .map(|m| m.fs)
        .collect()
}

/// Whom `do_fsync` flushes (`do_fsync:313-323`): the same triple, narrowed
/// to the fd's own device (`314`).
pub fn fsync_targets(dev: u64, mnts: &[MntSync]) -> Vec<Endpoint> {
    mnts.iter()
        .filter(|m| m.dev == dev && m.has_dev && m.has_fs && m.has_root)
        .map(|m| m.fs)
        .collect()
}

/// Borrowed-fd door (`dupvm:336-339`).
///
/// An fd that resolves nowhere refuses with `EBADF` ("get_filp2 failed");
/// the lookup itself runs caller-side (14 owns the table).
pub fn dupvm_fd_gate(fd_ok: bool) -> Result<(), MiscError> {
    if !fd_ok {
        return Err(MiscError::BadF);
    }
    Ok(())
}

/// Borrow door (`dupvm:341-357`).
///
/// The FS must offer peek (`RES_HASPEEK`, `vfsif.h:22`) and the file must
/// be regular or block; the VM-side fd and the shared count stay
/// caller-side (14 owns the table).
pub fn dupvm_gate(fs_has_peek: bool, file_type: FileType) -> Result<(), MiscError> {
    if !fs_has_peek {
        return Err(MiscError::Inval);
    }
    if !matches!(file_type, FileType::Regular | FileType::Block) {
        return Err(MiscError::Inval);
    }
    Ok(())
}

/// Page size for the lookup answer (4096; `roundup(..., PAGE_SIZE)`).
pub const PAGE_SIZE: u64 = 4096;
/// `VMC_NO_INODE` (`minix3/minix/include/minix/vm.h:90`): block lookups
/// name no file.
pub const VMC_NO_INODE: u64 = 0;
/// Unbounded block pages (`do_vm_call:438`).
///
/// C stores 32-bit `LONG_MAX`; the wire value pins here (ARCH A-8: the
/// 64-bit `LONG_MAX` differs, the VM wire keeps the C value).
pub const BLK_PAGES_UNBOUNDED: u64 = 0x7FFF_FFFF;

/// What an `FDLOOKUP` reports (`do_vm_call:434-447`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupAnswer {
    /// Device (or socket device for blocks).
    pub dev: u64,
    /// Inode (`VMC_NO_INODE` for blocks).
    pub ino: u64,
    /// File pages, rounded up (unbounded for blocks).
    pub size_pages: u64,
}

/// Byte size to page count (`roundup(size, PAGE_SIZE)/PAGE_SIZE`, `442-444`).
///
/// `div_ceil` cannot overflow, unlike the C add-then-divide.
pub fn size_pages(size: u64) -> u64 {
    size.div_ceil(PAGE_SIZE)
}

/// Build the lookup answer (`do_vm_call:434-445`).
pub fn lookup_answer(is_blk: bool, sdev: u64, dev: u64, ino: u64, size: u64) -> LookupAnswer {
    if is_blk {
        LookupAnswer {
            dev: sdev,
            ino: VMC_NO_INODE,
            size_pages: BLK_PAGES_UNBOUNDED,
        }
    } else {
        LookupAnswer {
            dev,
            ino,
            size_pages: size_pages(size),
        }
    }
}

/// VM's three words (`do_vm_call:414-477`, `com.h:702-704`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmVfsReq {
    /// `VMVFSREQ_FDLOOKUP` (101): borrow an fd into VM's table.
    FdLookup,
    /// `VMVFSREQ_FDCLOSE` (102): close a borrowed fd.
    FdClose,
    /// `VMVFSREQ_FDIO` (103): seek + peek for paging I/O.
    FdIo,
}

impl VmVfsReq {
    /// Decode the wire request; anything else panics in C (`475`), so it
    /// refuses here (`None`, the caller answers the error).
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            101 => Some(Self::FdLookup),
            102 => Some(Self::FdClose),
            103 => Some(Self::FdIo),
            _ => None,
        }
    }
}

/// Word must come from VM (`do_vm_call:402-403`).
pub fn vm_call_source_ok(is_vm: bool) -> Result<(), MiscError> {
    if !is_vm {
        return Err(MiscError::NoSys);
    }
    Ok(())
}

/// Lookup hit door (`do_vm_call:421-425`).
///
/// A named endpoint that resolves nowhere refuses with `ESRCH`
/// ("why isn't ep here"); the slot lookup itself runs caller-side
/// (02 owns the table).
pub fn vm_lookup_gate(found: bool) -> Result<(), MiscError> {
    if !found {
        return Err(MiscError::Srch);
    }
    Ok(())
}

/// `VM_VFS_REPLY` (`com.h:707`): `VM_RQ_BASE 0xC00 + 30`.
pub const VM_VFS_REPLY: u32 = 0xC1E;

/// `svrctl` group (`svrctl.h:23-24`, `_IOW/_IOWR('F', ...)`).
pub const SVRCTL_GROUP: u8 = b'F';
/// `sizeof(struct sysgetenv)` on 64-bit (2 pointers + 2 `size_t`).
///
/// The C 32-bit build hashes 16 here instead, so the full wire values
/// differ across bitness (ARCH A-8); only the group byte is stable.
pub const SYSGETENV_LEN: u64 = 32;
/// Compose an `svrctl` wire value (`_IOC(dir, 'F', num, len)`).
const fn svrctl_req(dir: u64, num: u64) -> u64 {
    dir | (SYSGETENV_LEN << 16) | ((SVRCTL_GROUP as u64) << 8) | num
}
/// `VFSSETPARAM`, composed (`_IOC(IN, 'F', 1, len)`, `svrctl.h:24`).
pub const VFSSETPARAM: u64 = svrctl_req(IOC_IN, 1);
/// `VFSGETPARAM`, composed (`_IOC(INOUT, 'F', 0, len)`, `svrctl.h:23`).
pub const VFSGETPARAM: u64 = svrctl_req(IOC_IN | IOC_OUT, 0);

/// Read the group byte (`IOCGROUP`, `ioccom.h:68`).
pub fn svrctl_group(req: u64) -> u8 {
    ((req >> 8) & 0xff) as u8
}

/// Group door (`do_svrctl:805`): only the `F` family enters.
pub fn svrctl_group_ok(req: u64) -> Result<(), MiscError> {
    if svrctl_group(req) != SVRCTL_GROUP {
        return Err(MiscError::Inval);
    }
    Ok(())
}

/// The two knobs (`do_svrctl:808-809`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvrctlCmd {
    /// `VFSSETPARAM`: set a parameter.
    SetParam,
    /// `VFSGETPARAM`: get a parameter.
    GetParam,
}

impl SvrctlCmd {
    /// Decode the wire request; the `default` branch refuses (`895-896`).
    pub fn from_raw(req: u64) -> Option<Self> {
        match req {
            VFSSETPARAM => Some(Self::SetParam),
            VFSGETPARAM => Some(Self::GetParam),
            _ => None,
        }
    }
}

/// Known keys (`do_svrctl:840,860,865,870`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvrctlKey {
    /// `verbose`: the only settable key.
    Verbose,
    /// `print_traces`: dump thread stacks (host runs it, A-1).
    PrintTraces,
    /// `print_select`: dump select state (host runs it, 23 owns it).
    PrintSelect,
    /// `active_threads`: report `NR_WTHREADS - available`.
    ActiveThreads,
}

impl SvrctlKey {
    /// Name the key; unknown names refuse (`ESRCH`, `854/859`).
    ///
    /// The caller copies and NUL-terminates the name first (`832-836`).
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "verbose" => Some(Self::Verbose),
            "print_traces" => Some(Self::PrintTraces),
            "print_select" => Some(Self::PrintSelect),
            "active_threads" => Some(Self::ActiveThreads),
            _ => None,
        }
    }

    /// Only `verbose` sets (`839-855`); the rest are get-only.
    pub fn settable(self) -> bool {
        matches!(self, Self::Verbose)
    }
}

/// Verbose range (`do_svrctl:848-851`): five stops, `0-4`.
/// The string-to-int conversion runs caller-side (`847`).
pub fn verbose_gate(v: i32) -> Result<u8, MiscError> {
    if !(0..=4).contains(&v) {
        return Err(MiscError::Inval);
    }
    Ok(v as u8)
}

/// Key/value length doors (`do_svrctl:822-829`).
///
/// Same range `[1, 63]`, asymmetric operators (`> 63` vs `>= 64`):
/// the shape differs, the verdict matches.
pub fn sysgetenv_len_gate(keylen: usize, vallen: usize) -> Result<(), MiscError> {
    if keylen == 0 || keylen > 63 {
        return Err(MiscError::Inval);
    }
    if vallen == 0 || vallen >= 64 {
        return Err(MiscError::Inval);
    }
    Ok(())
}

/// What the host must run (`do_svrctl:851,860-876`).
///
/// Trace/select dumps execute host-side (no mthreads under A-1;
/// `select_dump` lives unimplemented in 23); the module only names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAction {
    /// Store the verbosity (the one action that lands here).
    SetVerbose(u8),
    /// Dump thread stacks, host-side.
    RunTraces,
    /// Dump select state, host-side.
    RunSelectDump,
    /// Report the active count, host-side.
    ReportActive,
}

/// Which clock style (`do_utimens:59-85`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtimensStyle {
    /// Name given: walk the road (`UTIMENS_STYLE`).
    ByName,
    /// No name: stamp an open fd (`FUTIMENS_STYLE`).
    ByFd,
}

/// Flag doors (`do_utimens:61-62,80-81`).
///
/// Named walks take only `AT_SYMLINK_NOFOLLOW` (`fcntl.h:299`);
/// fd stamps take no flags at all.
pub fn utimens_flag_gate(named: bool, flags: u32) -> Result<UtimensStyle, MiscError> {
    /// `AT_SYMLINK_NOFOLLOW` (`minix3/sys/sys/fcntl.h:299`).
    const AT_SYMLINK_NOFOLLOW: u32 = 0x200;
    if named {
        if flags & !AT_SYMLINK_NOFOLLOW != 0 {
            return Err(MiscError::Inval);
        }
        Ok(UtimensStyle::ByName)
    } else {
        if flags != 0 {
            return Err(MiscError::Inval);
        }
        Ok(UtimensStyle::ByFd)
    }
}

/// Owner door, two acts (`do_utimens:89-92`).
///
/// Owners and root pass outright; strangers pass only when both stamps
/// say NOW *and* the write verdict allows — the second chance in `92`.
pub fn utimens_owner_gate(
    is_owner: bool,
    is_root: bool,
    both_now: bool,
    write_ok: bool,
) -> Result<(), MiscError> {
    if is_owner || is_root {
        return Ok(());
    }
    if both_now && write_ok {
        return Ok(());
    }
    Err(MiscError::Perm)
}

/// `UTIME_NOW` (`minix3/sys/sys/stat.h:235`): stamp it now.
pub const UTIME_NOW: i64 = (1 << 30) - 1;
/// `UTIME_OMIT` (`stat.h:236`): leave it alone.
pub const UTIME_OMIT: i64 = (1 << 30) - 2;
/// A billion nanoseconds: explicit stamps must sit below it (`117/134`).
pub const NSEC_MAX: i64 = 1_000_000_000;

/// One clock reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timespec {
    /// Seconds.
    pub sec: i64,
    /// Nanoseconds (or a `UTIME_*` sentinel on the wire).
    pub nsec: i64,
}

/// Requested stamp, three states (`do_utimens:105-138`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NsecSpec {
    /// `UTIME_NOW`: stamp it now.
    Now,
    /// `UTIME_OMIT`: leave it alone (seconds ride along for old FS).
    Omit,
    /// Explicit nanoseconds.
    At(i64),
}

impl NsecSpec {
    /// Read the wire sentinel.
    pub fn from_raw(v: i64) -> Self {
        match v {
            UTIME_NOW => Self::Now,
            UTIME_OMIT => Self::Omit,
            other => Self::At(other),
        }
    }
}

/// Resolve one stamp (`do_utimens:105-138`).
///
/// `NOW` takes the clock; `OMIT` keeps the sentinel with the clock's
/// seconds (`110-115`, "be nice with old FS"); explicit values must sit
/// in `[0, 1e9)` — the C `(unsigned)` cast rejects negatives too
/// (`117/134`), spelled as a plain bound here.
pub fn resolve_nsec(spec: NsecSpec, now: Timespec) -> Result<Timespec, MiscError> {
    match spec {
        NsecSpec::Now => Ok(now),
        NsecSpec::Omit => Ok(Timespec {
            sec: now.sec,
            nsec: UTIME_OMIT,
        }),
        NsecSpec::At(v) => {
            if !(0..NSEC_MAX).contains(&v) {
                return Err(MiscError::Inval);
            }
            Ok(Timespec { sec: 0, nsec: v })
        }
    }
}

/// Read-only terminus (`do_utimens:93`).
///
/// Shape-matches 29's door (`protect.rs:267`); the error maps below.
pub fn utimens_readonly_gate(has_vmnt: bool, readonly_flag: bool) -> Result<(), MiscError> {
    readonly_gate(has_vmnt, readonly_flag).map_err(|_| MiscError::RoFs)
}

/// `LABEL_MAX` (`minix3/minix/servers/vfs/const.h:34`): 16 with the NUL.
pub const LABEL_MAX: usize = 16;

/// Label length door (`do_gcov_flush:39-44`).
///
/// Below 16 the copy fits and the terminator lands inside (`44`).
/// Zero refuses too: C would write `label[len - 1]` out of bounds on an
/// empty label.
///
/// MINIX3 BUG (`gcov.c:39-44`): the length check admits `labellen == 0`,
/// and then `label[labellen - 1] = '\0'` writes one byte before the stack
/// buffer. Rust fix: the door demands a non-empty label.
pub fn gcov_label_gate(len: usize) -> Result<(), MiscError> {
    if len == 0 || len >= LABEL_MAX {
        return Err(MiscError::Inval);
    }
    Ok(())
}

/// Init exclusion (`do_gcov_flush:49-51`).
///
/// Init is the only non-system process with a label, and coverage stays
/// a system affair: label resolves to init refuses with `ENOENT`.
pub fn gcov_endpt_ok(is_init: bool) -> Result<(), MiscError> {
    if is_init {
        return Err(MiscError::NoEnt);
    }
    Ok(())
}

/// Grant outcome (`do_gcov_flush:54-57`): a failed grant refuses
/// with `ENOMEM` ("grant failed").
pub fn gcov_grant_outcome(granted: bool) -> Result<(), MiscError> {
    if !granted {
        return Err(MiscError::NoMem);
    }
    Ok(())
}

/// Self or other (`do_gcov_flush:59-68`): VFS answers for itself,
/// otherwise the request travels by taskcall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcovTarget {
    /// Ours: flush locally.
    SelfVfs,
    /// Theirs: send `COMMON_REQ_GCOV_DATA`.
    Other,
}

/// Split the target (`do_gcov_flush:59`).
pub fn gcov_target(is_self: bool) -> GcovTarget {
    if is_self {
        GcovTarget::SelfVfs
    } else {
        GcovTarget::Other
    }
}

/// The obsolete verdict (`do_getrusage:998-1006`).
///
/// PM owns rusage now; VFS answers `OK` until the call is removed
/// (the C TODO says so, `1003`). The contract pins the `OK`, not the TODO.
pub fn getrusage_verdict() -> i32 {
    minix_types::OK
}

/// The FS dialogue behind a trait.
///
/// `req_sync` fires and forgets (C ignores its `void`, `286/320`);
/// `req_utime` asks and answers (`time.c:143`). Only the dialogue is
/// abstracted (same-source convention as 29-D7).
pub trait MiscFs {
    /// `req_sync`: flush a filesystem, no answer expected.
    fn sync_fs(&mut self, fs: Endpoint);
    /// `req_utime`: stamp `(ino)` on `fs`.
    fn utime(
        &mut self,
        fs: Endpoint,
        ino: u64,
        actime: Timespec,
        modtime: Timespec,
    ) -> Result<(), MiscError>;
}

/// Scripted FS (test double with programmed answers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedMisc {
    /// Programmed `utime` answer.
    pub utime_out: Result<(), MiscError>,
    /// Filesystems synced, in order (observable dialogue).
    pub synced: Vec<Endpoint>,
    /// Downcalls made (observable dialogue).
    pub ncalls: u32,
}

impl Default for ScriptedMisc {
    fn default() -> Self {
        Self {
            utime_out: Ok(()),
            synced: Vec::new(),
            ncalls: 0,
        }
    }
}

impl MiscFs for ScriptedMisc {
    fn sync_fs(&mut self, fs: Endpoint) {
        self.synced.push(fs);
        self.ncalls += 1;
    }
    fn utime(
        &mut self,
        _fs: Endpoint,
        _ino: u64,
        _actime: Timespec,
        _modtime: Timespec,
    ) -> Result<(), MiscError> {
        self.ncalls += 1;
        self.utime_out
    }
}

/// Refusing FS (test double: `utime` fails with `EIO`, syncs vanish).
///
/// Behaves differently from [`ScriptedMisc`] (blanket refusal vs
/// programmed answers), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefusingMisc;

impl MiscFs for RefusingMisc {
    fn sync_fs(&mut self, _fs: Endpoint) {}
    fn utime(
        &mut self,
        _fs: Endpoint,
        _ino: u64,
        _actime: Timespec,
        _modtime: Timespec,
    ) -> Result<(), MiscError> {
        Err(MiscError::Io)
    }
}

/// What the call tells the main loop (ARCH A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiscVerdict {
    /// Reply now (status carried separately).
    Done,
    /// C `SUSPEND`: the VM word answers asynchronously (`do_vm_call:497`,
    /// 09 `ReplyIntent::ReplyLater`).
    Suspend,
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// Lookup failures ride `err_code` upstream (caller-side inputs), so
/// they are not variants here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiscError {
    /// `EPERM`: strangers ask, touch, or flush caches.
    Perm,
    /// `EINVAL`: wild selectors, lengths, flags, stamps, labels.
    Inval,
    /// `ESRCH`: unknown knob names; missing endpoints.
    Srch,
    /// `ENOSYS`: words from anyone but VM.
    NoSys,
    /// `ENOMEM`: a failed grant refuses.
    NoMem,
    /// `ENOENT`: init has no coverage to give.
    NoEnt,
    /// `EBADF`: borrowed fd resolves nowhere.
    BadF,
    /// `EROFS`: not even root touches read-only mounts.
    RoFs,
    /// `EIO`: FS-side refusal.
    Io,
}

impl MiscError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Perm => minix_types::EPERM,
            Self::Inval => minix_types::EINVAL,
            Self::Srch => minix_types::ESRCH,
            Self::NoSys => minix_types::ENOSYS,
            Self::NoMem => minix_types::ENOMEM,
            Self::NoEnt => minix_types::ENOENT,
            Self::BadF => minix_types::EBADF,
            Self::RoFs => minix_types::EROFS,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_table::VfsCallNum;

    fn mnt(has_dev: bool, has_fs: bool, has_root: bool, fs: i32, dev: u64) -> MntSync {
        MntSync {
            has_dev,
            has_fs,
            has_root,
            fs: Endpoint(fs),
            dev,
        }
    }

    #[test]
    fn test_call_surface_and_sysinfo_doors() {
        // Eight calls ride VfsCallNum (09 owns the enum, D1 reuses it).
        assert_eq!(
            VfsCallNum::Sync as u32,
            VFS_SYNC_OFF + VfsCallNum::Read as u32
        );
        assert_eq!(VFS_SYNC_OFF, 16);
        assert_eq!(VFS_FSYNC_OFF, 32);
        assert_eq!(VFS_UTIMENS_OFF, 37);
        assert_eq!(VFS_VMCALL_OFF, 38);
        assert_eq!(VFS_GETRUSAGE_OFF, 42);
        assert_eq!(VFS_SVRCTL_OFF, 43);
        assert_eq!(VFS_GCOV_FLUSH_OFF, 44);
        assert_eq!(VFS_GETSYSINFO_OFF, 48);
        // Three tables decode (`sysinfo.h:11-17`); stats (9) has no table.
        assert_eq!(SysinfoWhat::from_raw(2), Some(SysinfoWhat::ProcTab));
        assert_eq!(SysinfoWhat::from_raw(3), Some(SysinfoWhat::DmapTab));
        assert_eq!(SysinfoWhat::from_raw(13), Some(SysinfoWhat::ProcLightTab));
        assert_eq!(SysinfoWhat::from_raw(9), None);
        assert_eq!(SysinfoWhat::from_raw(0), None);
        assert_eq!(SysinfoWhat::from_raw(99), None);
        // Root first (`70`).
        assert!(sysinfo_root_gate(true).is_ok());
        assert_eq!(sysinfo_root_gate(false), Err(MiscError::Perm));
        // Length must match exactly (`108-109`).
        assert!(sysinfo_len_gate(100, 100).is_ok());
        assert_eq!(sysinfo_len_gate(99, 100), Err(MiscError::Inval));
        assert_eq!(sysinfo_len_gate(101, 100), Err(MiscError::Inval));
        // Light task cells (`87-94`).
        assert_eq!(
            light_task(BlockKind::Cdev, Endpoint(5), None),
            LightTask::Driver(Endpoint(5))
        );
        assert_eq!(
            light_task(BlockKind::Sdev, Endpoint(5), Some(Endpoint(7))),
            LightTask::Driver(Endpoint(7))
        );
        assert_eq!(
            light_task(BlockKind::Sdev, Endpoint(5), None),
            LightTask::NoDriver
        );
        assert_eq!(
            light_task(BlockKind::Other, Endpoint(5), Some(Endpoint(7))),
            LightTask::NoDriver
        );
    }

    #[test]
    fn test_sync_sweeps() {
        let mnts = [
            mnt(true, true, true, 1, 10),  // served + rooted: flush
            mnt(false, true, true, 2, 10), // no dev: skip
            mnt(true, false, true, 3, 20), // no server: skip
            mnt(true, true, false, 4, 20), // no root: skip
            mnt(true, true, true, 5, 20),  // served + rooted: flush
        ];
        // Broadcast takes every served, rooted volume (`281-291`).
        assert_eq!(sync_targets(&mnts), alloc::vec![Endpoint(1), Endpoint(5)]);
        // Narrowed to the fd's device (`313-323`).
        assert_eq!(fsync_targets(10, &mnts), alloc::vec![Endpoint(1)]);
        assert_eq!(fsync_targets(20, &mnts), alloc::vec![Endpoint(5)]);
        assert!(fsync_targets(99, &mnts).is_empty());
        assert!(sync_targets(&[]).is_empty());
    }

    #[test]
    fn test_dupvm_gate_and_lookup() {
        // Dead fds refuse (`336-339`); the lookup runs caller-side.
        assert!(dupvm_fd_gate(true).is_ok());
        assert_eq!(dupvm_fd_gate(false), Err(MiscError::BadF));
        // Peek first (`341`), then shape (`352`).
        assert!(dupvm_gate(true, FileType::Regular).is_ok());
        assert!(dupvm_gate(true, FileType::Block).is_ok());
        assert_eq!(dupvm_gate(false, FileType::Regular), Err(MiscError::Inval));
        assert_eq!(dupvm_gate(true, FileType::Directory), Err(MiscError::Inval));
        assert_eq!(dupvm_gate(true, FileType::Fifo), Err(MiscError::Inval));
        // Pages round up (`442-444`).
        assert_eq!(size_pages(0), 0);
        assert_eq!(size_pages(1), 1);
        assert_eq!(size_pages(4096), 1);
        assert_eq!(size_pages(4097), 2);
        // Blocks name no file and run unbounded (`434-438`).
        assert_eq!(
            lookup_answer(true, 11, 22, 33, 9999),
            LookupAnswer {
                dev: 11,
                ino: VMC_NO_INODE,
                size_pages: BLK_PAGES_UNBOUNDED
            }
        );
        assert_eq!(VMC_NO_INODE, 0);
        assert_eq!(BLK_PAGES_UNBOUNDED, 0x7FFF_FFFF);
        // Files report themselves (`440-444`).
        assert_eq!(
            lookup_answer(false, 11, 22, 33, 5000),
            LookupAnswer {
                dev: 22,
                ino: 33,
                size_pages: 2
            }
        );
    }

    #[test]
    fn test_vm_word() {
        // Three words decode (`com.h:702-704`); wild words refuse (C panics, `475`).
        assert_eq!(VmVfsReq::from_raw(101), Some(VmVfsReq::FdLookup));
        assert_eq!(VmVfsReq::from_raw(102), Some(VmVfsReq::FdClose));
        assert_eq!(VmVfsReq::from_raw(103), Some(VmVfsReq::FdIo));
        assert_eq!(VmVfsReq::from_raw(0), None);
        assert_eq!(VmVfsReq::from_raw(104), None);
        // Words come from VM only (`402-403`).
        assert!(vm_call_source_ok(true).is_ok());
        assert_eq!(vm_call_source_ok(false), Err(MiscError::NoSys));
        // Named endpoints that resolve nowhere refuse (`421-425`).
        assert!(vm_lookup_gate(true).is_ok());
        assert_eq!(vm_lookup_gate(false), Err(MiscError::Srch));
        // The reply travels async under a fixed type (`492-497`).
        assert_eq!(VM_VFS_REPLY, 0xC1E);
        assert_eq!(MiscVerdict::Suspend, MiscVerdict::Suspend);
        assert_ne!(MiscVerdict::Done, MiscVerdict::Suspend);
    }

    #[test]
    fn test_svrctl_knobs() {
        // Only the F family enters (`805`); the group byte is stable (A-8).
        assert_eq!(svrctl_group(VFSSETPARAM), SVRCTL_GROUP);
        assert_eq!(svrctl_group(VFSGETPARAM), SVRCTL_GROUP);
        assert_eq!(SVRCTL_GROUP, b'F');
        assert!(svrctl_group_ok(VFSSETPARAM).is_ok());
        assert!(svrctl_group_ok(VFSGETPARAM).is_ok());
        assert_eq!(svrctl_group_ok(0x1234), Err(MiscError::Inval));
        // Two knobs decode; the default refuses (`807-809,895-896`).
        assert_eq!(SvrctlCmd::from_raw(VFSSETPARAM), Some(SvrctlCmd::SetParam));
        assert_eq!(SvrctlCmd::from_raw(VFSGETPARAM), Some(SvrctlCmd::GetParam));
        assert_eq!(SvrctlCmd::from_raw(0), None);
        assert_ne!(VFSSETPARAM, VFSGETPARAM);
        // Four keys known (`840,860,865,870`); strangers get ESRCH.
        assert_eq!(SvrctlKey::from_key("verbose"), Some(SvrctlKey::Verbose));
        assert_eq!(
            SvrctlKey::from_key("print_traces"),
            Some(SvrctlKey::PrintTraces)
        );
        assert_eq!(
            SvrctlKey::from_key("print_select"),
            Some(SvrctlKey::PrintSelect)
        );
        assert_eq!(
            SvrctlKey::from_key("active_threads"),
            Some(SvrctlKey::ActiveThreads)
        );
        assert_eq!(SvrctlKey::from_key("nope"), None);
        assert!(SvrctlKey::Verbose.settable());
        assert!(!SvrctlKey::PrintTraces.settable());
        // Verbose stops at five (`848-851`).
        assert_eq!(verbose_gate(0), Ok(0));
        assert_eq!(verbose_gate(4), Ok(4));
        assert_eq!(verbose_gate(-1), Err(MiscError::Inval));
        assert_eq!(verbose_gate(5), Err(MiscError::Inval));
        // Lengths share the range, not the operators (`822-829`).
        assert!(sysgetenv_len_gate(1, 1).is_ok());
        assert!(sysgetenv_len_gate(63, 63).is_ok());
        assert_eq!(sysgetenv_len_gate(0, 1), Err(MiscError::Inval));
        assert_eq!(sysgetenv_len_gate(64, 1), Err(MiscError::Inval));
        assert_eq!(sysgetenv_len_gate(1, 0), Err(MiscError::Inval));
        assert_eq!(sysgetenv_len_gate(1, 64), Err(MiscError::Inval));
        // Host actions name their runners.
        assert_eq!(HostAction::SetVerbose(2), HostAction::SetVerbose(2));
        assert_ne!(HostAction::RunTraces, HostAction::RunSelectDump);
    }

    #[test]
    fn test_clock() {
        // Named walks take only NOFOLLOW; fd stamps take nothing (`61-62,80-81`).
        assert_eq!(utimens_flag_gate(true, 0x200), Ok(UtimensStyle::ByName));
        assert_eq!(utimens_flag_gate(true, 0), Ok(UtimensStyle::ByName));
        assert_eq!(utimens_flag_gate(true, 0x201), Err(MiscError::Inval));
        assert_eq!(utimens_flag_gate(false, 0), Ok(UtimensStyle::ByFd));
        assert_eq!(utimens_flag_gate(false, 1), Err(MiscError::Inval));
        // Owners and root pass; strangers get a second chance only when
        // both stamps say NOW and writing allows (`89-92`).
        assert!(utimens_owner_gate(true, false, false, false).is_ok());
        assert!(utimens_owner_gate(false, true, false, false).is_ok());
        assert!(utimens_owner_gate(false, false, true, true).is_ok());
        assert_eq!(
            utimens_owner_gate(false, false, true, false),
            Err(MiscError::Perm)
        );
        assert_eq!(
            utimens_owner_gate(false, false, false, true),
            Err(MiscError::Perm)
        );
        assert_eq!(
            utimens_owner_gate(false, false, false, false),
            Err(MiscError::Perm)
        );
        // Sentinels read (`stat.h:235-236`).
        assert_eq!(NsecSpec::from_raw(UTIME_NOW), NsecSpec::Now);
        assert_eq!(NsecSpec::from_raw(UTIME_OMIT), NsecSpec::Omit);
        assert_eq!(NsecSpec::from_raw(5), NsecSpec::At(5));
        assert_eq!(UTIME_NOW, (1 << 30) - 1);
        assert_eq!(UTIME_OMIT, (1 << 30) - 2);
        // NOW takes the clock; OMIT keeps the sentinel with clock seconds.
        let now = Timespec {
            sec: 100,
            nsec: 200,
        };
        assert_eq!(resolve_nsec(NsecSpec::Now, now), Ok(now));
        assert_eq!(
            resolve_nsec(NsecSpec::Omit, now),
            Ok(Timespec {
                sec: 100,
                nsec: UTIME_OMIT
            })
        );
        assert_eq!(
            resolve_nsec(NsecSpec::At(500), now),
            Ok(Timespec { sec: 0, nsec: 500 })
        );
        // At or past a billion refuses — negatives too, like the C cast.
        assert_eq!(
            resolve_nsec(NsecSpec::At(1_000_000_000), now),
            Err(MiscError::Inval)
        );
        assert_eq!(resolve_nsec(NsecSpec::At(-1), now), Err(MiscError::Inval));
        // Read-only ends the walk, even for root (`93`, 29's door reused).
        assert!(utimens_readonly_gate(false, true).is_ok());
        assert!(utimens_readonly_gate(true, false).is_ok());
        assert_eq!(utimens_readonly_gate(true, true), Err(MiscError::RoFs));
    }

    #[test]
    fn test_probe_and_obsolete() {
        // Labels fit below 16 (`gcov.c:39`); empty refuses (MINIX3 BUG pin).
        assert_eq!(LABEL_MAX, 16);
        assert!(gcov_label_gate(1).is_ok());
        assert!(gcov_label_gate(15).is_ok());
        assert_eq!(gcov_label_gate(0), Err(MiscError::Inval));
        assert_eq!(gcov_label_gate(16), Err(MiscError::Inval));
        assert_eq!(gcov_label_gate(100), Err(MiscError::Inval));
        // Init keeps no coverage (`50-51`).
        assert!(gcov_endpt_ok(false).is_ok());
        assert_eq!(gcov_endpt_ok(true), Err(MiscError::NoEnt));
        // Failed grants refuse (`54-57`).
        assert!(gcov_grant_outcome(true).is_ok());
        assert_eq!(gcov_grant_outcome(false), Err(MiscError::NoMem));
        // Self flushes, others travel (`59-68`).
        assert_eq!(gcov_target(true), GcovTarget::SelfVfs);
        assert_eq!(gcov_target(false), GcovTarget::Other);
        // The obsolete verdict stays OK (`1005`).
        assert_eq!(getrusage_verdict(), minix_types::OK);
        assert_eq!(getrusage_verdict(), 0);
    }

    #[test]
    fn test_fs_dialogue_and_errno() {
        // Fire-and-forget syncs record their targets; answers stay unasked.
        let mut scripted = ScriptedMisc::default();
        scripted.sync_fs(Endpoint(1));
        scripted.sync_fs(Endpoint(2));
        assert_eq!(scripted.synced, alloc::vec![Endpoint(1), Endpoint(2)]);
        assert_eq!(scripted.ncalls, 2);
        let now = Timespec { sec: 1, nsec: 2 };
        assert!(scripted.utime(Endpoint(1), 7, now, now).is_ok());
        assert_eq!(scripted.ncalls, 3);
        // Scripted failure propagates.
        let mut failing = ScriptedMisc {
            utime_out: Err(MiscError::Io),
            ..Default::default()
        };
        assert_eq!(failing.utime(Endpoint(1), 7, now, now), Err(MiscError::Io));
        // Blanket refusal (second impl).
        let mut refusing = RefusingMisc;
        assert_eq!(refusing.utime(Endpoint(1), 7, now, now), Err(MiscError::Io));
        // Nine variants, nine errnos, no inventions.
        for (err, errno) in [
            (MiscError::Perm, minix_types::EPERM),
            (MiscError::Inval, minix_types::EINVAL),
            (MiscError::Srch, minix_types::ESRCH),
            (MiscError::NoSys, minix_types::ENOSYS),
            (MiscError::NoMem, minix_types::ENOMEM),
            (MiscError::NoEnt, minix_types::ENOENT),
            (MiscError::BadF, minix_types::EBADF),
            (MiscError::RoFs, minix_types::EROFS),
            (MiscError::Io, minix_types::EIO),
        ] {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
