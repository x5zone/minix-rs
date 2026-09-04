//! `device_map` — the driver phone books: number-to-driver tables.
//!
//! Corresponds to Minix3's `dmap.c:1-328` (device↔driver table),
//! `smap.c:1-273` (socket-driver table), `device.c:1-95` (type-independent
//! ioctl dispatch and grant decoding), and `dmap.h:1-27` (table layout).
//!
//! Design decisions (see 19-device-map.md §3):
//! - `DmapEntry/SmapEntry` are fixed-size values (`None` replaces `NONE`)
//! - `EndpointDirectory` trait isolates DS label resolution (test doubles)
//! - registration is a pure plan: reuse/claim/validate/side-effects
//! - `recover_step` types the driver-restart state machine
//! - `ioctl_access/ioctl_size` decode grant parameters as pure functions
//! - `ioctl_route` reuses 15's `FileType` instead of redefining dispatch
//! - dmap locking reuses 07's borrow model (no second lock abstraction)

use crate::open::FileType;

/// `NR_DEVICES` (`minix3/minix/include/minix/dmap.h:82`): major table size.
pub const NR_DEVICES: usize = 135;

/// `NR_SOCKDEVS` (`const.h:10`): socket-driver table size.
pub const NR_SOCKDEVS: usize = 8;

/// `PF_MAX` (`minix3/sys/sys/socket.h:333` = `AF_MAX` = 35): domain map size.
pub const PF_MAX: usize = 35;

/// `PF_UNSPEC` (`socket.h:290` = 0): unusable domain value.
pub const PF_UNSPEC: i32 = 0;

/// `NR_DOMAIN` (`minix3/minix/include/minix/config.h:61`): registration cap.
pub const NR_DOMAIN: usize = 8;

/// `LABEL_MAX` (`const.h:34`): label size including the NUL.
pub const LABEL_MAX: usize = 16;

/// `CTTY_MAJOR` (`dmap.h:26`): `/dev/tty` is handled by VFS itself.
pub const CTTY_MAJOR: u32 = 5;

/// `CTTY_ENDPT` = `VFS_PROC_NR` (`const.h:52`, `com.h:60` = 1).
pub const CTTY_ENDPT: i32 = 1;

/// `RS_PROC_NR` (`com.h:61` = 2): only RS may map drivers.
pub const RS_PROC_NR: i32 = 2;

/// `IOC_OUT/IOC_IN` (`minix3/sys/sys/ioccom.h:74,76`): direction bits.
pub const IOC_OUT: u64 = 0x4000_0000;
/// See [`IOC_OUT`].
pub const IOC_IN: u64 = 0x8000_0000;
/// `IOC_BIG` (`ioccom.h:58`): wide-size flag.
pub const IOC_BIG: u64 = 0x1000_0000;
/// `IOCPARM_SHIFT/MASK` (`ioccom.h:64,59`): 12-bit size field.
pub const IOCPARM_SHIFT: u32 = 16;
/// See [`IOCPARM_SHIFT`].
pub const IOCPARM_MASK: u64 = 0xfff;
/// `IOCPARM_SHIFT_BIG/MASK_BIG` (`ioccom.h:61,60`): 20-bit size field.
pub const IOCPARM_SHIFT_BIG: u32 = 8;
/// See [`IOCPARM_SHIFT_BIG`].
pub const IOCPARM_MASK_BIG: u64 = 0xF_FFFF;
/// `CPF_READ/CPF_WRITE` (`minix3/minix/include/minix/safecopies.h:64-65`).
pub const CPF_READ: u32 = 0x0000_01;
/// See [`CPF_READ`].
pub const CPF_WRITE: u32 = 0x0000_02;

/// One device↔driver row (`struct dmap`, `dmap.h:16-25`).
///
/// Only routing knowledge lives here: per-driver locks and select state
/// belong to 07/20/21/23 and stay out of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmapEntry {
    /// Owning driver endpoint (`None` = `NONE`, unmapped).
    pub driver: Option<i32>,
    /// Driver label, NUL-padded (`LABEL_MAX` bytes).
    pub label: [u8; LABEL_MAX],
    /// Recovery in progress (`dmap_recovering`).
    pub recovering: bool,
    /// A worker is currently servicing this driver.
    pub servicing: bool,
}

impl DmapEntry {
    /// Unmapped row.
    pub fn empty() -> Self {
        Self {
            driver: None,
            label: [0; LABEL_MAX],
            recovering: false,
            servicing: false,
        }
    }

    /// Whether this row names a live driver.
    pub fn is_mapped(self) -> bool {
        self.driver.is_some()
    }
}

/// Device↔driver table (`dmap[NR_DEVICES]`, `dmap.c:22`).
#[derive(Debug, Clone)]
pub struct DmapTable {
    entries: [DmapEntry; NR_DEVICES],
}

impl DmapTable {
    /// All rows unmapped (`init_dmap` zeroing, `dmap.c:235-242`).
    pub fn new() -> Self {
        Self {
            entries: [DmapEntry::empty(); NR_DEVICES],
        }
    }

    /// Initialize exactly like `init_dmap`: clear all, then self-map the
    /// controlling terminal (`dmap.c:244-246`).
    pub fn init() -> Self {
        let mut table = Self::new();
        let mut ctty = DmapEntry::empty();
        ctty.driver = Some(CTTY_ENDPT);
        ctty.label[0] = b'v';
        ctty.label[1] = b'f';
        ctty.label[2] = b's';
        table.entries[CTTY_MAJOR as usize] = ctty;
        table
    }

    /// Row count.
    pub fn len(&self) -> usize {
        NR_DEVICES
    }

    /// Read one row (`None` when out of range).
    pub fn get(&self, major: u32) -> Option<&DmapEntry> {
        self.entries.get(major as usize)
    }

    /// Write one row (`false` when out of range → `ENODEV` at call sites).
    pub fn set(&mut self, major: u32, entry: DmapEntry) -> bool {
        if (major as usize) < NR_DEVICES {
            self.entries[major as usize] = entry;
            true
        } else {
            false
        }
    }
}

impl Default for DmapTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `proc` drives `major` (`dmap_driver_match`, `dmap.c:252-259`).
pub fn driver_match(table: &DmapTable, proc: i32, major: u32) -> bool {
    match table.get(major) {
        Some(row) => row.driver == Some(proc),
        None => false,
    }
}

/// Live row by major (`get_dmap_by_major`, `dmap.c:264-270`):
/// out-of-range or unmapped rows report absent.
pub fn get_by_major(table: &DmapTable, major: u32) -> Option<&DmapEntry> {
    let row = table.get(major)?;
    if row.is_mapped() { Some(row) } else { None }
}

/// First row driven by `proc` (`get_dmap_by_endpt`, `dmap.c:317-328`).
///
/// The linear scan is honest: `smap.c:249` notes the same O(n) wish for
/// the socket twin, and 135 pointer-free rows are a cache line or two.
pub fn get_by_endpt(table: &DmapTable, proc: i32) -> Option<u32> {
    for (i, row) in table.entries.iter().enumerate() {
        if row.driver == Some(proc) {
            return Some(i as u32);
        }
    }
    None
}

/// Validate a driver label fits (`map_driver` length gate, `dmap.c:89-94`).
pub fn validate_label(label: &[u8]) -> Result<(), MapError> {
    if label.len() + 1 > LABEL_MAX {
        return Err(MapError::Inval);
    }
    Ok(())
}

/// Store or clear one mapping (`map_driver`, `dmap.c:61-101`).
///
/// - `endpoint = None` unmaps (the char-filp invalidation rides with the
///   caller, 14-filedes.md).
/// - Out-of-range majors report `ENODEV`; overlong labels `EINVAL`.
pub fn map_driver(
    table: &mut DmapTable,
    label: Option<&[u8]>,
    major: u32,
    endpoint: Option<i32>,
) -> Result<(), MapError> {
    if (major as usize) >= NR_DEVICES {
        return Err(MapError::NoDev);
    }
    if endpoint.is_none() {
        table.entries[major as usize] = DmapEntry::empty();
        return Ok(());
    }
    if let Some(name) = label {
        validate_label(name)?;
        let mut row = DmapEntry::empty();
        let n = name.len().min(LABEL_MAX - 1);
        row.label[..n].copy_from_slice(&name[..n]);
        row.driver = endpoint;
        table.entries[major as usize] = row;
    } else {
        table.entries[major as usize].driver = endpoint;
    }
    Ok(())
}

/// Count rows driven by `proc`, clearing each (`dmap_unmap_by_endpt`,
/// `dmap.c:180-195`). Per-row failures are logged-and-continued in C;
/// here every clear succeeds, so the count is exact.
pub fn unmap_by_endpt(table: &mut DmapTable, proc: i32) -> u32 {
    let mut cleared = 0;
    for row in table.entries.iter_mut() {
        if row.driver == Some(proc) {
            *row = DmapEntry::empty();
            cleared += 1;
        }
    }
    cleared
}

/// Directory of living drivers: DS label resolution seam
/// (`ds_retrieve_label_endpt`, `dmap.c:148-152`).
pub trait EndpointDirectory {
    /// Resolve `label` to an endpoint (`None` = unknown label).
    fn lookup(&self, label: &str) -> Option<i32>;
}

/// Fixed directory (test double with real answers).
#[derive(Debug, Clone, Copy)]
pub struct StaticDir {
    /// Sorted or unsorted pairs; linear scan is fine for tests.
    pub pairs: &'static [(&'static str, i32)],
}

impl EndpointDirectory for StaticDir {
    fn lookup(&self, label: &str) -> Option<i32> {
        self.pairs
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, e)| *e)
    }
}

/// Empty directory (test double that knows nobody).
///
/// Behaves differently from [`StaticDir`] (answers vs refusal),
/// satisfying the "two behaviorally different impls" rule for traits.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyDir;

impl EndpointDirectory for EmptyDir {
    fn lookup(&self, _label: &str) -> Option<i32> {
        None
    }
}

/// Resolve a driver label through an abstract directory.
///
/// Unknown labels become `EINVAL`, mirroring `do_mapdriver` (`dmap.c:149`).
pub fn resolve_driver<D: EndpointDirectory>(dir: &D, label: &str) -> Result<i32, MapError> {
    dir.lookup(label).ok_or(MapError::Inval)
}

/// Whether the caller may map drivers: RS only (`dmap.c:123`).
pub fn check_mapper(caller: i32) -> Result<(), MapError> {
    if caller != RS_PROC_NR {
        return Err(MapError::Perm);
    }
    Ok(())
}

/// Service classification for `map_service` (`dmap.c:200-225`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceMap {
    /// Boot-time user process: nothing to do (`IS_RPUB_BOOT_USR`).
    SkipBootUser,
    /// No device: mark service, stop here (`dev_nr == NO_DEV`).
    ServiceOnly,
    /// Map the driver's device through `map_driver`.
    MapDriver,
}

/// Pure `map_service` head: boot-user skip, then device presence.
pub fn classify_service(is_boot_user: bool, has_device: bool) -> ServiceMap {
    if is_boot_user {
        ServiceMap::SkipBootUser
    } else if !has_device {
        ServiceMap::ServiceOnly
    } else {
        ServiceMap::MapDriver
    }
}

/// One socket-driver row (`struct smap`, `type.h:41-47`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmapEntry {
    /// One-based slot number (never zero, so no socket is `NO_DEV`).
    pub num: u32,
    /// Owning driver endpoint (`None` = free).
    pub endpt: Option<i32>,
    /// Driver label, NUL-padded.
    pub label: [u8; LABEL_MAX],
}

impl SmapEntry {
    /// Free row with its fixed one-based number (`smap.c:32-33`).
    pub fn free(num: u32) -> Self {
        Self {
            num,
            endpt: None,
            label: [0; LABEL_MAX],
        }
    }
}

/// Socket-driver tables (`smap[8]` + `pfmap[35]`, `smap.c:15-16`).
#[derive(Debug, Clone)]
pub struct SmapTable {
    /// Driver rows, one-based numbers fixed at construction.
    pub entries: [SmapEntry; NR_SOCKDEVS],
    /// Domain → row index (`None` = unclaimed).
    pub pfmap: [Option<u8>; PF_MAX],
}

impl SmapTable {
    /// `init_smap` (`smap.c:22-37`): numbered rows, no owners, no domains.
    pub fn new() -> Self {
        let mut entries = [SmapEntry::free(1); NR_SOCKDEVS];
        for (i, row) in entries.iter_mut().enumerate() {
            *row = SmapEntry::free((i + 1) as u32);
        }
        Self {
            entries,
            pfmap: [None; PF_MAX],
        }
    }

    /// Row count.
    pub fn len(&self) -> usize {
        NR_SOCKDEVS
    }
}

impl Default for SmapTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Domain verdict for one registration candidate (`smap.c:75-83`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainCheck {
    /// All domains usable.
    Ok,
    /// Out of range (`domain < 0 || >= PF_MAX`).
    BadRange,
    /// `PF_UNSPEC` is never mappable.
    Unspec,
    /// Reserved by a different driver instance.
    Busy,
}

/// Check one domain against the table.
pub fn check_domain(table: &SmapTable, domain: i32, self_idx: Option<u8>) -> DomainCheck {
    if domain < 0 || (domain as usize) >= PF_MAX {
        return DomainCheck::BadRange;
    }
    if domain == PF_UNSPEC {
        return DomainCheck::Unspec;
    }
    match table.pfmap[domain as usize] {
        Some(owner) if Some(owner) != self_idx => DomainCheck::Busy,
        _ => DomainCheck::Ok,
    }
}

/// Side effects owed when a restarted driver changes endpoint
/// (`smap.c:108-120`): wake the old endpoint's sleepers and invalidate
/// its sockets — but only when the endpoint actually changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplaceSideEffects {
    /// Old endpoint whose sleepers to scatter (`unsuspend_by_endpt`).
    pub unsuspend: Option<i32>,
    /// Smap number whose sockets to invalidate.
    pub invalidate_num: Option<u32>,
}

/// Registration plan (`smap_map` core, `smap.c:47-141`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterPlan {
    /// Slot index to (re)write.
    pub slot: u8,
    /// Restart path (reuse by label) vs fresh claim.
    pub reuse: bool,
    /// Side effects when a changed endpoint replaces an old one.
    pub effects: ReplaceSideEffects,
}

/// Pure registration decision.
///
/// - `existing`: slot already filed under `label` (restart), if any.
/// - `free`: first free slot, if any (`None` → `ENOMEM` on fresh claims).
/// - `domains_ok`: precomputed per-domain verdicts (first failure wins,
///   in domain order, `smap.c:75-83`).
/// - `old_endpt`/`new_endpt`: endpoints for the change detector.
pub fn register_plan(
    existing: Option<u8>,
    free: Option<u8>,
    domains_ok: &[DomainCheck],
    old_endpt: Option<i32>,
    new_endpt: i32,
) -> Result<RegisterPlan, MapError> {
    for check in domains_ok {
        match check {
            DomainCheck::Ok => {}
            DomainCheck::BadRange | DomainCheck::Unspec => return Err(MapError::Inval),
            DomainCheck::Busy => return Err(MapError::Busy),
        }
    }
    let (slot, reuse) = match (existing, free) {
        (Some(s), _) => (s, true),
        (None, Some(f)) => (f, false),
        (None, None) => return Err(MapError::NoMem),
    };
    let effects = if reuse && old_endpt != Some(new_endpt) {
        ReplaceSideEffects {
            unsuspend: old_endpt,
            invalidate_num: None,
        }
    } else {
        ReplaceSideEffects::default()
    };
    Ok(RegisterPlan {
        slot,
        reuse,
        effects,
    })
}

/// Find the slot filed under `label` with a live owner (restart reuse,
/// `smap.c:62-69`).
pub fn find_slot_by_label(table: &SmapTable, label: &[u8]) -> Option<u8> {
    for (i, row) in table.entries.iter().enumerate() {
        if row.endpt.is_some() && row.label.starts_with(label) && label.len() <= LABEL_MAX {
            // Exact match required: stored tail must be NUL padding.
            if row.label[label.len()..].iter().all(|b| *b == 0) {
                return Some(i as u8);
            }
        }
    }
    None
}

/// First free slot, if any (`smap.c:89-95`).
pub fn find_free_slot(table: &SmapTable) -> Option<u8> {
    table
        .entries
        .iter()
        .position(|row| row.endpt.is_none())
        .map(|i| i as u8)
}

/// Compose a socket device number (`make_smap_dev`, `smap.c:200-208`):
/// entry number up top, per-driver id below. Namespaces differ from
/// major/minor on purpose (file type must be tested first).
pub fn make_smap_dev(num: u32, sockid: u32) -> u64 {
    ((num as u64) << 32) | (sockid as u64)
}

/// Split a socket device number (`get_smap_by_dev`, `smap.c:216-237`).
///
/// Yields `(num, sockid)` only for in-range numbers; liveness (`endpt`
/// set) stays with the caller, which owns the table.
pub fn split_smap_dev(dev: u64) -> Option<(u32, i32)> {
    let num = (dev >> 32) as u32;
    let id = (dev & 0xffff_ffff) as u32 as i32;
    if num == 0 || (num as usize) > NR_SOCKDEVS || id < 0 {
        return None;
    }
    Some((num, id))
}

/// Row by endpoint (`get_smap_by_endpt`, `smap.c:244-259`).
/// The O(n) scan is inherited honestly (cf. `smap.c:249`).
pub fn smap_by_endpt(table: &SmapTable, endpt: i32) -> Option<u8> {
    table
        .entries
        .iter()
        .position(|row| row.endpt == Some(endpt))
        .map(|i| i as u8)
}

/// Row by domain (`get_smap_by_domain`, `smap.c:265-273`).
pub fn smap_by_domain(table: &SmapTable, domain: i32) -> Option<u8> {
    if domain < 0 || (domain as usize) >= PF_MAX {
        return None;
    }
    table.pfmap[domain as usize]
}

/// Driver-restart verdict per entry (`dmap_endpt_up`, `dmap.c:275-312`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoverVerdict {
    /// Recovery already in flight and broken again: stop the worker.
    FailoverStop,
    /// Fresh failure: mark recovering and run the kind-specific path.
    BeginRecover,
    /// No work for this entry.
    Steady,
}

/// Pure recovery step: `recovering` flag × servicing worker present.
pub fn recover_step(recovering: bool, servicing: bool, is_blk: bool) -> RecoverVerdict {
    if !is_blk {
        // Character path always stops the worker / clears the table;
        // modelled as steady here, execution stays with 21/14.
        return RecoverVerdict::Steady;
    }
    match (recovering, servicing) {
        (true, _) => RecoverVerdict::FailoverStop,
        (false, _) => RecoverVerdict::BeginRecover,
    }
}

/// Which restart notices a vanished driver owes (`dmap_endpt_up` char arm
/// vs `smap_unmap_by_endpt`/`smap_endpt_up`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VanishNotice {
    /// Char driver gone: invalidate its filps (14-filedes.md executes).
    InvalidateChar,
    /// Socket driver gone: invalidate its sockets (14 executes).
    InvalidateSock,
    /// Block driver gone: `bdev_up` path (20-bdev.md executes).
    RecoverBlock,
}

/// Classify one vanished-driver notice by family.
pub fn classify_vanish(is_blk: bool, is_sock: bool) -> VanishNotice {
    if is_sock {
        VanishNotice::InvalidateSock
    } else if is_blk {
        VanishNotice::RecoverBlock
    } else {
        VanishNotice::InvalidateChar
    }
}

/// ioctl dispatch target (`do_ioctl` switch, `device.c:34-54`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoctlTarget {
    /// Block device: `bdev_ioctl` under the `filp_ioctl_fp` guard.
    Block,
    /// Character device: `cdev_io(CDEV_IOCTL, …)`.
    Char,
    /// Socket: `sdev_ioctl`.
    Sock,
}

/// Whether the block arm needs the `filp_ioctl_fp` guard
/// (`device.c:36-40` set → call → clear).
pub const BLOCK_NEEDS_GUARD: bool = true;

/// Pure ioctl dispatch over file type; anything else is `ENOTTY`.
pub fn ioctl_route(ft: FileType) -> Result<IoctlTarget, MapError> {
    match ft {
        FileType::Block => Ok(IoctlTarget::Block),
        FileType::Char => Ok(IoctlTarget::Char),
        FileType::Socket => Ok(IoctlTarget::Sock),
        FileType::Regular | FileType::Directory | FileType::Fifo | FileType::Unknown(_) => {
            Err(MapError::NotTty)
        }
    }
}

/// Decode grant access from an ioctl request (`device.c:76-78`).
///
/// Note the cross: `_IOR` (read *out* to the user) grants the driver
/// *write* access, and `_IOW` (write *in* from the user) grants *read*.
pub fn ioctl_access(request: u64) -> u32 {
    let mut access = 0;
    if request & IOC_OUT != 0 {
        access |= CPF_WRITE;
    }
    if request & IOC_IN != 0 {
        access |= CPF_READ;
    }
    access
}

/// Decode grant size from an ioctl request (`device.c:79-82`):
/// 20-bit field when `IOC_BIG`, else the 12-bit field.
pub fn ioctl_size(request: u64) -> u64 {
    if request & IOC_BIG != 0 {
        (request >> IOCPARM_SHIFT_BIG) & IOCPARM_MASK_BIG
    } else {
        (request >> IOCPARM_SHIFT) & IOCPARM_MASK
    }
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// No invented codes: every variant names the errno it becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    /// `EPERM`: anyone but RS mapping drivers.
    Perm,
    /// `EINVAL`: overlong/unterminated/unknown label, bad endpoint, bad domain.
    Inval,
    /// `ENODEV`: major out of range.
    NoDev,
    /// `ENOMEM`: socket table full.
    NoMem,
    /// `EBUSY`: domain reserved by another driver.
    Busy,
    /// `ENOTTY`: ioctl on a non-device.
    NotTty,
    /// `EIO`: driver-vanish revival, hardening cases.
    Io,
}

impl MapError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Perm => minix_types::EPERM,
            Self::Inval => minix_types::EINVAL,
            Self::NoDev => minix_types::ENODEV,
            Self::NoMem => minix_types::ENOMEM,
            Self::Busy => minix_types::EBUSY,
            Self::NotTty => minix_types::ENOTTY,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open::FileType;

    #[test]
    fn test_dmap_init_and_ctty() {
        // `init_dmap` zeroes all, then self-maps CTTY (`dmap.c:230-247`).
        let table = DmapTable::init();
        assert_eq!(table.len(), NR_DEVICES);
        let ctty = table.get(CTTY_MAJOR).unwrap();
        assert_eq!(ctty.driver, Some(CTTY_ENDPT));
        assert_eq!(&ctty.label[..3], b"vfs");
        // Every other row is unmapped.
        assert!(table.get(0).unwrap().driver.is_none());
        assert!(table.get(4).unwrap().driver.is_none());
        assert!(table.get(134).unwrap().driver.is_none());
        assert!(table.get(135).is_none());
        // Fresh table has no mappings at all.
        let fresh = DmapTable::new();
        assert!(fresh.get(CTTY_MAJOR).unwrap().driver.is_none());
    }

    #[test]
    fn test_map_driver_lifecycle() {
        let mut table = DmapTable::new();
        // Out-of-range majors refuse (`dmap.c:71`).
        assert_eq!(
            map_driver(&mut table, None, 135, Some(7)).unwrap_err(),
            MapError::NoDev
        );
        assert_eq!(MapError::NoDev.to_errno(), minix_types::ENODEV);
        // Overlong labels refuse (`dmap.c:89-93`).
        let long = [b'x'; LABEL_MAX];
        assert_eq!(
            map_driver(&mut table, Some(&long), 4, Some(7)).unwrap_err(),
            MapError::Inval
        );
        // Store and match (`dmap_driver_match`, `dmap.c:252-259`).
        map_driver(&mut table, Some(b"tty"), 4, Some(7)).unwrap();
        assert!(driver_match(&table, 7, 4));
        assert!(!driver_match(&table, 8, 4));
        assert!(!driver_match(&table, 7, 200));
        // Live lookup by major and by endpoint.
        assert!(get_by_major(&table, 4).is_some());
        assert_eq!(get_by_endpt(&table, 7), Some(4));
        assert_eq!(get_by_endpt(&table, 9), None);
        // Unmap clears (`proc_nr_e == NONE`, `dmap.c:75-86`).
        map_driver(&mut table, None, 4, None).unwrap();
        assert!(!driver_match(&table, 7, 4));
        assert!(get_by_major(&table, 4).is_none());
        // Sweep unmaps every row owned by an endpoint.
        map_driver(&mut table, Some(b"a"), 4, Some(7)).unwrap();
        map_driver(&mut table, Some(b"b"), 5, Some(7)).unwrap();
        assert_eq!(unmap_by_endpt(&mut table, 7), 2);
        assert_eq!(get_by_endpt(&table, 7), None);
    }

    #[test]
    fn test_mapper_gate_and_directory() {
        // Only RS maps (`dmap.c:123`).
        assert!(check_mapper(RS_PROC_NR).is_ok());
        assert_eq!(check_mapper(1).unwrap_err(), MapError::Perm);
        assert_eq!(MapError::Perm.to_errno(), minix_types::EPERM);
        // Gate D: the two directories behave differently through one bound.
        static PAIRS: &[(&str, i32)] = &[("tty", 7), ("inet", 12)];
        let full = StaticDir { pairs: PAIRS };
        let empty = EmptyDir;
        fn via<D: EndpointDirectory>(d: &D) -> Result<i32, MapError> {
            resolve_driver(d, "tty")
        }
        assert_eq!(via(&full).unwrap(), 7);
        assert_eq!(via(&empty).unwrap_err(), MapError::Inval);
        assert_eq!(resolve_driver(&full, "nope").unwrap_err(), MapError::Inval);
        // Service head: boot users skip, deviceless mark only.
        assert_eq!(classify_service(true, true), ServiceMap::SkipBootUser);
        assert_eq!(classify_service(false, false), ServiceMap::ServiceOnly);
        assert_eq!(classify_service(false, true), ServiceMap::MapDriver);
        // Labels are bounded (`LABEL_MAX` includes the NUL).
        assert!(validate_label(b"tty").is_ok());
        assert_eq!(
            validate_label(&[b'x'; LABEL_MAX]).unwrap_err(),
            MapError::Inval
        );
    }

    #[test]
    fn test_smap_init() {
        // One-based numbers, no owners, no domains (`smap.c:22-37`).
        let table = SmapTable::new();
        assert_eq!(table.len(), NR_SOCKDEVS);
        for (i, row) in table.entries.iter().enumerate() {
            assert_eq!(row.num, (i + 1) as u32);
            assert!(row.endpt.is_none());
        }
        assert!(table.pfmap.iter().all(|d| d.is_none()));
    }

    #[test]
    fn test_register_plan_matrix() {
        // Fresh claim takes the free slot.
        let p = register_plan(None, Some(2), &[DomainCheck::Ok], None, 9).unwrap();
        assert_eq!(p.slot, 2);
        assert!(!p.reuse);
        assert_eq!(p.effects, ReplaceSideEffects::default());
        // Restart reuses the labelled slot, no side effects when the
        // endpoint is unchanged (stateful restart, `smap.c:108-120`).
        let p = register_plan(Some(1), Some(2), &[DomainCheck::Ok], Some(9), 9).unwrap();
        assert_eq!(p.slot, 1);
        assert!(p.reuse);
        assert_eq!(p.effects.unsuspend, None);
        // Changed endpoint owes scatter + invalidate.
        let p = register_plan(Some(1), Some(2), &[DomainCheck::Ok], Some(7), 9).unwrap();
        assert_eq!(p.effects.unsuspend, Some(7));
        // Domain failures precede slot checks, in order.
        assert_eq!(
            register_plan(
                None,
                Some(2),
                &[DomainCheck::Ok, DomainCheck::Busy],
                None,
                9
            )
            .unwrap_err(),
            MapError::Busy
        );
        assert_eq!(
            register_plan(None, Some(2), &[DomainCheck::Unspec], None, 9).unwrap_err(),
            MapError::Inval
        );
        assert_eq!(
            register_plan(None, Some(2), &[DomainCheck::BadRange], None, 9).unwrap_err(),
            MapError::Inval
        );
        assert_eq!(MapError::Busy.to_errno(), minix_types::EBUSY);
        // No slot anywhere is ENOMEM (`smap.c:94-95`).
        assert_eq!(
            register_plan(None, None, &[DomainCheck::Ok], None, 9).unwrap_err(),
            MapError::NoMem
        );
        assert_eq!(MapError::NoMem.to_errno(), minix_types::ENOMEM);
    }

    #[test]
    fn test_domain_checks() {
        let mut table = SmapTable::new();
        table.pfmap[2] = Some(0);
        // Occupied by another instance.
        assert_eq!(check_domain(&table, 2, Some(1)), DomainCheck::Busy);
        // …but fine for its own restart.
        assert_eq!(check_domain(&table, 2, Some(0)), DomainCheck::Ok);
        assert_eq!(check_domain(&table, 3, None), DomainCheck::Ok);
        // Range and unspec rejections (`smap.c:77-80`).
        assert_eq!(check_domain(&table, -1, None), DomainCheck::BadRange);
        assert_eq!(
            check_domain(&table, PF_MAX as i32, None),
            DomainCheck::BadRange
        );
        assert_eq!(check_domain(&table, PF_UNSPEC, None), DomainCheck::Unspec);
        // Slot helpers: reuse by label, first-free claim.
        assert_eq!(find_slot_by_label(&table, b"inet"), None);
        assert_eq!(find_free_slot(&table), Some(0));
    }

    #[test]
    fn test_smap_dev_codec() {
        // Compose and split round-trip (`make_smap_dev/get_smap_by_dev`).
        let dev = make_smap_dev(3, 42);
        assert_eq!(split_smap_dev(dev), Some((3, 42)));
        // Zero numbers never name a socket (`smap.c:225`).
        assert_eq!(split_smap_dev(0), None);
        assert_eq!(split_smap_dev(make_smap_dev(0, 1)), None);
        // Out-of-range numbers refuse.
        assert_eq!(
            split_smap_dev(make_smap_dev(NR_SOCKDEVS as u32 + 1, 1)),
            None
        );
        // High-bit ids are negative as `sockid_t` and refuse (`id < 0`).
        assert_eq!(split_smap_dev((1u64 << 32) | 0xFFFF_FFFF), None);
        // Endpoint and domain lookups.
        let mut table = SmapTable::new();
        table.entries[2].endpt = Some(12);
        table.pfmap[2] = Some(2);
        assert_eq!(smap_by_endpt(&table, 12), Some(2));
        assert_eq!(smap_by_endpt(&table, 13), None);
        assert_eq!(smap_by_domain(&table, 2), Some(2));
        assert_eq!(smap_by_domain(&table, 3), None);
        assert_eq!(smap_by_domain(&table, -1), None);
        assert_eq!(smap_by_domain(&table, PF_MAX as i32), None);
    }

    #[test]
    fn test_recover_step_matrix() {
        // Broken-again recovery stops the worker (`dmap.c:290-299`).
        assert_eq!(recover_step(true, true, true), RecoverVerdict::FailoverStop);
        assert_eq!(
            recover_step(true, false, true),
            RecoverVerdict::FailoverStop
        );
        // Fresh failure begins recovery (`dmap.c:300-302`).
        assert_eq!(
            recover_step(false, true, true),
            RecoverVerdict::BeginRecover
        );
        assert_eq!(
            recover_step(false, false, true),
            RecoverVerdict::BeginRecover
        );
        // Character path is steady here; execution stays with 21/14.
        assert_eq!(recover_step(false, true, false), RecoverVerdict::Steady);
        // Vanish notices fan out by family.
        assert_eq!(classify_vanish(false, true), VanishNotice::InvalidateSock);
        assert_eq!(classify_vanish(true, false), VanishNotice::RecoverBlock);
        assert_eq!(classify_vanish(false, false), VanishNotice::InvalidateChar);
    }

    #[test]
    fn test_ioctl_route_and_grant() {
        // Three live targets, everything else ENOTTY (`device.c:34-54`).
        assert_eq!(ioctl_route(FileType::Block).unwrap(), IoctlTarget::Block);
        assert_eq!(ioctl_route(FileType::Char).unwrap(), IoctlTarget::Char);
        assert_eq!(ioctl_route(FileType::Socket).unwrap(), IoctlTarget::Sock);
        assert_eq!(
            ioctl_route(FileType::Regular).unwrap_err(),
            MapError::NotTty
        );
        assert_eq!(
            ioctl_route(FileType::Directory).unwrap_err(),
            MapError::NotTty
        );
        assert_eq!(ioctl_route(FileType::Fifo).unwrap_err(), MapError::NotTty);
        assert_eq!(MapError::NotTty.to_errno(), minix_types::ENOTTY);
        assert!(BLOCK_NEEDS_GUARD);
        // Direction cross: IOR grants WRITE, IOW grants READ (`76-78`).
        assert_eq!(ioctl_access(IOC_OUT), CPF_WRITE);
        assert_eq!(ioctl_access(IOC_IN), CPF_READ);
        assert_eq!(ioctl_access(IOC_IN | IOC_OUT), CPF_READ | CPF_WRITE);
        assert_eq!(ioctl_access(0), 0);
        // Size fields: 12-bit vs 20-bit BIG (`79-82`).
        assert_eq!(ioctl_size((0x123u64) << 16), 0x123);
        assert_eq!(ioctl_size(IOC_BIG | ((0xABCDEu64) << 8)), 0xABCDE);
        assert_eq!(ioctl_size(0), 0);
    }

    #[test]
    fn test_errno_map_covers_device_c() {
        let cases = [
            (MapError::Perm, minix_types::EPERM),
            (MapError::Inval, minix_types::EINVAL),
            (MapError::NoDev, minix_types::ENODEV),
            (MapError::NoMem, minix_types::ENOMEM),
            (MapError::Busy, minix_types::EBUSY),
            (MapError::NotTty, minix_types::ENOTTY),
            (MapError::Io, minix_types::EIO),
        ];
        for (err, errno) in cases {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
