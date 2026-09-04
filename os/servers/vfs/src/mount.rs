//! `mount` — grafting file systems onto one tree: devices, slots, teardown.
//!
//! Corresponds to Minix3's `mount.c:1-653` (`update_bspec`, `do_mount`,
//! `mount_fs`, `mount_pfs`, `do_umount`, `unmount`, `unmount_all`,
//! `name_to_dev`, `is_nonedev`, `find_free_nonedev`) with device-number
//! codecs from `minix3/sys/sys/types.h:290-295` and table sizes from
//! `minix3/minix/servers/vfs/const.h:7-12`.
//!
//! Design decisions (see 18-mount.md §3):
//! - `DevCodec` pure functions replace the major/minor/makedev macros
//! - `NonedevBitmap(u16)` makes the pseudo-device pool observable
//! - `SuperblockReader` trait isolates the FS round-trip (test doubles)
//! - glue/busy/type/thread decisions are pure predicates
//! - `RootStage` enum replaces the bare `have_root` counter
//! - teardown is a plan struct with the PFS exception explicit
//! - `verify_empty` types the force-shutdown assertion
//!
//! Scope note: `vmnt` slot storage itself belongs to 06-vmnt-table.md.
//! This module only decides *which* slot transitions happen, never how
//! slots are stored. (`NR_MNTS` below is intentionally *not* redefined:
//! the pseudo-device pool cites `NR_NONEDEVS` directly.)

/// `NR_NONEDEVS` (`const.h:12` = `NR_MNTS` = 16): pseudo-device pool size.
pub const NR_NONEDEVS: usize = 16;

/// `NONE_MAJOR` (`minix3/minix/include/minix/dmap.h:21`): major number of
/// the `none` pseudo devices.
pub const NONE_MAJOR: u32 = 0;

/// `NO_DEV` (`minix3/minix/include/minix/const.h:132`): absence of a device.
pub const NO_DEV: u64 = 0;

/// `NR_WTHREADS` (`const.h:9`): worker count a threaded FS may use.
pub const NR_WTHREADS: u32 = 9;

/// Device-number codec (`major/minor/makedev`, `types.h:290-295`).
pub struct DevCodec;

impl DevCodec {
    /// `major(x) = (x & 0xfff00) >> 8`.
    pub fn major(dev: u64) -> u32 {
        ((dev & 0x000f_ff00) >> 8) as u32
    }

    /// `minor(x) = ((x & 0xfff00000) >> 12) | (x & 0xff)`.
    pub fn minor(dev: u64) -> u32 {
        (((dev & 0xfff0_0000) >> 12) | (dev & 0x0000_00ff)) as u32
    }

    /// `makedev(major, minor)` inverse of the two above.
    pub fn make(major: u32, minor: u32) -> u64 {
        ((((major as u64) << 8) & 0x000f_ff00)
            | (((minor as u64) << 12) & 0xfff0_0000)
            | ((minor as u64) & 0x0000_00ff)) as u64
    }

    /// Whether `dev` is a `none` pseudo device (`is_nonedev`,
    /// `mount.c:633-634`): major zero with a nonzero in-range minor.
    pub fn is_nonedev(dev: u64) -> bool {
        let major = Self::major(dev);
        let minor = Self::minor(dev);
        major == NONE_MAJOR && minor > 0 && (minor as usize) <= NR_NONEDEVS
    }
}

/// Pseudo-device allocation pool (`nonedev` bitmap, `mount.c:33-37`).
///
/// Sixteen bits exactly cover `NR_NONEDEVS`; bit `i` stands for minor
/// `i + 1` (minors start at 1, `dmap.h:17-18`). A set bit means in use.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NonedevBitmap(pub u16);

impl NonedevBitmap {
    /// Empty pool.
    pub fn new() -> Self {
        Self(0)
    }

    /// Whether minor `m` (1-based) is in use.
    pub fn contains(self, minor: u32) -> bool {
        if minor == 0 || (minor as usize) > NR_NONEDEVS {
            return false;
        }
        self.0 & (1 << (minor - 1)) != 0
    }

    /// Mark minor `m` in use (`alloc_nonedev`, `mount.c:36`).
    pub fn alloc_minor(&mut self, minor: u32) {
        debug_assert!((1..=(NR_NONEDEVS as u32)).contains(&minor));
        if (1..=(NR_NONEDEVS as u32)).contains(&minor) {
            self.0 |= 1 << (minor - 1);
        }
    }

    /// Release minor `m` (`free_nonedev`, `mount.c:37`).
    pub fn free_minor(&mut self, minor: u32) {
        debug_assert!((1..=(NR_NONEDEVS as u32)).contains(&minor));
        if (1..=(NR_NONEDEVS as u32)).contains(&minor) {
            self.0 &= !(1 << (minor - 1));
        }
    }

    /// First free minor, if any (`find_free_nonedev`, `mount.c:641-653`).
    pub fn find_free(&self) -> Option<u32> {
        for i in 0..NR_NONEDEVS as u32 {
            if self.0 & (1 << i) == 0 {
                return Some(i + 1);
            }
        }
        None
    }

    /// Whether the pool is exhausted.
    pub fn is_full(self) -> bool {
        self.find_free().is_none()
    }
}

/// Allocate a free pseudo device (`find_free_nonedev` + `alloc_nonedev`).
///
/// Exhaustion reports `EMFILE` (`mount.c:651` sets `err_code`), matching C.
pub fn alloc_nonedev(pool: &mut NonedevBitmap) -> Result<u64, MountError> {
    match pool.find_free() {
        Some(minor) => {
            pool.alloc_minor(minor);
            Ok(DevCodec::make(NONE_MAJOR, minor))
        }
        None => Err(MountError::NoSpace),
    }
}

/// Release a pseudo device back to the pool (`free_nonedev` at call sites).
pub fn free_nonedev(pool: &mut NonedevBitmap, dev: u64) {
    if DevCodec::is_nonedev(dev) {
        pool.free_minor(DevCodec::minor(dev));
    }
}

/// Superblock read result (the `node_details` fields `mount_fs` keeps).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuperInfo {
    /// File-server endpoint that answered.
    pub fs_e: i32,
    /// Whether the FS is multithreaded (`RES_THREADED`, `vfsif.h:21`).
    pub threaded: bool,
}

/// File-server hook for superblock reads (`req_readsuper`, `mount.c:272`).
///
/// Isolates the FS round-trip so the commit sequence is unit-testable
/// without a live file server.
pub trait SuperblockReader {
    /// Read the superblock of `dev`; map FS errors to [`MountError`].
    fn read_super(&self, dev: u64, readonly: bool, is_root: bool) -> Result<SuperInfo, MountError>;
}

/// Reader whose superblock read always succeeds (test double).
#[derive(Debug, Default, Clone, Copy)]
pub struct MemSuperblock {
    /// Reported threading flag.
    pub threaded: bool,
}

impl SuperblockReader for MemSuperblock {
    fn read_super(
        &self,
        _dev: u64,
        _readonly: bool,
        _is_root: bool,
    ) -> Result<SuperInfo, MountError> {
        Ok(SuperInfo {
            fs_e: 3,
            threaded: self.threaded,
        })
    }
}

/// Reader whose superblock read always fails with `EIO` (test double).
///
/// Behaves differently from [`MemSuperblock`] (success vs refusal),
/// satisfying the "two behaviorally different impls" rule for traits.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailSuperblock;

impl SuperblockReader for FailSuperblock {
    fn read_super(
        &self,
        _dev: u64,
        _readonly: bool,
        _is_root: bool,
    ) -> Result<SuperInfo, MountError> {
        Err(MountError::Io)
    }
}

/// Commit stages of `mount_fs` (`mount.c:156-385`).
///
/// Later stages unwind more; each failure returns its stage so the caller
/// releases exactly what was reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MountPhase {
    /// Driver label lookup (`mount.c:176-188`).
    DriverTag,
    /// Busy scan + free slot claim (`mount.c:190-200`).
    SlotClaim,
    /// Mountpoint glue (`mount.c:211-238`).
    Glue,
    /// Superblock read + statvfs (`mount.c:240-291`).
    Superblock,
    /// Final graft (`mount.c:318-384`).
    Commit,
}

/// Whether `dev` may be mounted: reject already-mounted devices with
/// `EBUSY` (`mount.c:191-196`).
pub fn check_dev_free(mounted: &[u64], dev: u64) -> Result<(), MountError> {
    if mounted.contains(&dev) {
        return Err(MountError::Busy);
    }
    Ok(())
}

/// Mountpoint glue verdict (`mount.c:218-222`): the mountpoint must be
/// referenced exactly once — by us — or someone else is using it.
pub fn glue_decision(mountpoint_refs: u32) -> Result<(), MountError> {
    if mountpoint_refs == 1 {
        Ok(())
    } else {
        Err(MountError::Busy)
    }
}

/// Mounted-type vs root-type clash (`mount.c:352-353`): a non-directory
/// mountpoint cannot receive a directory root.
pub fn type_clash(mountpoint_is_dir: bool, root_is_dir: bool) -> Result<(), MountError> {
    if !mountpoint_is_dir && root_is_dir {
        return Err(MountError::IsDir);
    }
    Ok(())
}

/// Concurrent request allowance from the threading flag (`mount.c:309-313`).
pub fn thread_allowance(threaded: bool) -> u32 {
    if threaded { NR_WTHREADS } else { 1 }
}

/// Root-mount state (`have_root`, `mount.c:31`).
///
/// The root may be mounted twice — first the ramdisk, then the boot disk
/// (`mount.c:206-209`) — after which further `/` mounts are ordinary ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RootStage {
    /// No root yet (`have_root == 0`).
    #[default]
    Absent,
    /// Ramdisk root live (`have_root == 1`).
    Ramdisk,
    /// Boot-disk root live (`have_root == 2`, saturated).
    Bootdisk,
}

impl RootStage {
    /// Advance after a successful root mount (saturates at `Bootdisk`).
    pub fn advance(self) -> Self {
        match self {
            Self::Absent => Self::Ramdisk,
            Self::Ramdisk | Self::Bootdisk => Self::Bootdisk,
        }
    }

    /// Whether mounting `is_root_path` takes the root path
    /// (`mount_root = isroot && have_root < 2`, `mount.c:206`).
    pub fn mount_takes_root_path(self, is_root_path: bool) -> bool {
        is_root_path && self != Self::Bootdisk
    }
}

/// Block-routing update for one device (`update_bspec`, `mount.c:46-80`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BspecAction {
    /// Retarget block vnodes, no driver message.
    SetOnly,
    /// Retarget and forward the driver label (`send_drv_e`).
    SetAndSend,
    /// Major out of range: leave this vnode alone (`mount.c:61-64`).
    SkipOutOfRange,
    /// Driver vanished: leave this vnode alone (`mount.c:66-70`).
    SkipVanished,
}

/// Pure per-vnode routing decision.
///
/// - `major_valid`: major number in range.
/// - `driver_present`: `dmap[major].dmap_driver != NONE`.
/// - `send`: caller passes `send_drv_e`.
pub fn bspec_update(major_valid: bool, driver_present: bool, send: bool) -> BspecAction {
    if !major_valid {
        return BspecAction::SkipOutOfRange;
    }
    if send && !driver_present {
        return BspecAction::SkipVanished;
    }
    if send {
        BspecAction::SetAndSend
    } else {
        BspecAction::SetOnly
    }
}

/// Where `do_mount` gets its device (`mount.c:126-138`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevSource {
    /// Named block device; resolve via `name_to_dev`.
    Real,
    /// Empty name: take a pseudo device instead.
    Pseudo,
}

/// `do_mount` head gate (`mount.c:107-138`).
///
/// - Non-super-user → `EPERM`.
/// - Overlong label → `EINVAL` (`mount.c:111-112`).
/// - Overlong type → `ENAMETOOLONG` (`mount.c:144`).
/// - Empty device name → pseudo-device path; else real-device path.
pub fn validate_mount_head(
    is_super: bool,
    label_len: usize,
    label_max: usize,
    type_len: usize,
    type_max: usize,
    dev_len: usize,
) -> Result<DevSource, MountError> {
    if !is_super {
        return Err(MountError::Perm);
    }
    if label_len > label_max {
        return Err(MountError::Inval);
    }
    if type_len > type_max {
        return Err(MountError::NameTooLong);
    }
    if dev_len == 0 {
        Ok(DevSource::Pseudo)
    } else {
        Ok(DevSource::Real)
    }
}

/// Name classification for `name_to_dev` (`mount.c:590-622`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    /// Block special file: device is its `v_sdev` (`mount.c:609-610`).
    Block(u64),
    /// Mount root of a mounted tree (only with `allow_mountpt`).
    MountRoot(u64),
    /// Neither: `ENOTBLK` (`mount.c:613-616`).
    Neither,
}

/// Pure name-to-device classification.
///
/// `lookup_ok` mirrors `eat_path` success; the lookup itself stays with
/// 13-path-lookup.md.
pub fn classify_name(
    lookup_ok: bool,
    is_blk: bool,
    sdev: u64,
    allow_mountpt: bool,
    is_mount_root: bool,
    v_dev: u64,
) -> Result<u64, MountError> {
    if !lookup_ok {
        return Err(MountError::Inval);
    }
    match NameKind::of(is_blk, sdev, allow_mountpt, is_mount_root, v_dev) {
        NameKind::Block(dev) | NameKind::MountRoot(dev) => Ok(dev),
        NameKind::Neither => Err(MountError::NotBlk),
    }
}

impl NameKind {
    /// Classify one resolved path.
    pub fn of(
        is_blk: bool,
        sdev: u64,
        allow_mountpt: bool,
        is_mount_root: bool,
        v_dev: u64,
    ) -> Self {
        if is_blk {
            Self::Block(sdev)
        } else if allow_mountpt && is_mount_root {
            Self::MountRoot(v_dev)
        } else {
            Self::Neither
        }
    }
}

/// Busy verdict for `unmount` (`mount.c:492-504`).
///
/// Exactly one vnode (the root, referenced once), at most one lock, and
/// no pending waiter on the vmnt lock — otherwise `EBUSY`.
pub fn busy_check(total_refs: u32, locked: u32, vmnt_pending: bool) -> Result<(), MountError> {
    if total_refs > 1 || locked > 1 || vmnt_pending {
        return Err(MountError::Busy);
    }
    Ok(())
}

/// Teardown plan for `unmount` (`mount.c:506-545`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnmountPlan {
    /// `is_nonedev → free_nonedev` (`mount.c:526`).
    pub free_nonedev: bool,
    /// Zero the root node — skipped for PFS, which has none (`530-535`).
    pub clear_root: bool,
    /// Re-route block I/O to the root FS with driver notice (`541-543`).
    pub send_driver: bool,
}

/// Plan one teardown. `has_root_node` is false only for PFS.
pub fn plan_unmount(is_pseudo: bool, has_root_node: bool) -> UnmountPlan {
    UnmountPlan {
        free_nonedev: is_pseudo,
        clear_root: has_root_node,
        send_driver: true,
    }
}

/// Force-shutdown residue check (`unmount_all`, `mount.c:578-585`).
///
/// Any still-mounted slot is a shutdown bug (C panics); here it is a
/// testable error.
pub fn verify_empty(mounted: &[bool]) -> Result<(), MountError> {
    if mounted.iter().any(|m| *m) {
        return Err(MountError::Busy);
    }
    Ok(())
}

/// How many sweep passes `unmount_all` runs (`mount.c:563-567`).
///
/// One pass per slot suffices to peel nested mounts inside-out when each
/// pass removes at least the outermost layer.
pub fn sweep_passes(nr_slots: usize) -> usize {
    nr_slots
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// No invented codes: every variant names the errno it becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountError {
    /// `EPERM`: non-super-user mount/umount.
    Perm,
    /// `EINVAL`: overlong label, bad endpoint, unknown device.
    Inval,
    /// `ENAMETOOLONG`: overlong file-system type.
    NameTooLong,
    /// `EBUSY`: already mounted, mountpoint busy, tree busy.
    Busy,
    /// `ENOMEM`: no free vmnt slot.
    NoMem,
    /// `ENOTBLK`: name is neither block device nor mountpoint.
    NotBlk,
    /// `EMFILE`: pseudo-device pool exhausted.
    NoSpace,
    /// `EISDIR`: directory root on a non-directory mountpoint.
    IsDir,
    /// `EIO`: superblock failure, driver-vanish revival, hardening cases.
    Io,
}

impl MountError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Perm => minix_types::EPERM,
            Self::Inval => minix_types::EINVAL,
            Self::NameTooLong => minix_types::ENAMETOOLONG,
            Self::Busy => minix_types::EBUSY,
            Self::NoMem => minix_types::ENOMEM,
            Self::NotBlk => minix_types::ENOTBLK,
            Self::NoSpace => minix_types::EMFILE,
            Self::IsDir => minix_types::EISDIR,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_codec_roundtrip() {
        // `makedev` inverts `major`/`minor` (`types.h:290-295`).
        for (major, minor) in [(0u32, 1u32), (4, 0), (8, 3), (0, 16), (255, 255)] {
            let dev = DevCodec::make(major, minor);
            assert_eq!(DevCodec::major(dev), major, "{major}:{minor}");
            // Minor spans two bit fields; full byte range round-trips.
            if minor < 256 {
                assert_eq!(DevCodec::minor(dev), minor, "{major}:{minor}");
            }
        }
        // Pseudo devices: major zero, minors 1..=16.
        assert!(DevCodec::is_nonedev(DevCodec::make(0, 1)));
        assert!(DevCodec::is_nonedev(DevCodec::make(0, 16)));
        assert!(!DevCodec::is_nonedev(DevCodec::make(0, 0)));
        assert!(!DevCodec::is_nonedev(DevCodec::make(0, 17)));
        assert!(!DevCodec::is_nonedev(DevCodec::make(4, 1)));
        assert!(!DevCodec::is_nonedev(NO_DEV));
    }

    #[test]
    fn test_nonedev_pool_pairing() {
        let mut pool = NonedevBitmap::new();
        assert!(!pool.is_full());
        // Allocate the whole pool (`find_free_nonedev` order).
        let mut minors = [0u64; NR_NONEDEVS];
        for (i, slot) in minors.iter_mut().enumerate() {
            let dev = alloc_nonedev(&mut pool).unwrap();
            assert_eq!(DevCodec::minor(dev), (i + 1) as u32);
            assert_eq!(DevCodec::major(dev), NONE_MAJOR);
            *slot = dev;
        }
        assert!(pool.is_full());
        // Exhaustion reports EMFILE (`mount.c:651`).
        assert_eq!(alloc_nonedev(&mut pool).unwrap_err(), MountError::NoSpace);
        assert_eq!(MountError::NoSpace.to_errno(), minix_types::EMFILE);
        // Release and reuse (`free_nonedev`).
        free_nonedev(&mut pool, minors[0]);
        assert!(!pool.is_full());
        assert!(!pool.contains(1));
        let dev = alloc_nonedev(&mut pool).unwrap();
        assert_eq!(DevCodec::minor(dev), 1);
        // Non-pseudo devices are never pooled.
        let before = pool;
        free_nonedev(&mut pool, DevCodec::make(4, 0));
        assert_eq!(pool, before);
    }

    #[test]
    fn test_superblock_factories_differ() {
        // Gate D: the two `SuperblockReader` impls behave differently.
        let mem = MemSuperblock { threaded: true };
        let info = mem.read_super(0x801, false, true).unwrap();
        assert_eq!(info.fs_e, 3);
        assert!(info.threaded);
        assert_eq!(
            FailSuperblock.read_super(0x801, false, true).unwrap_err(),
            MountError::Io
        );
        fn via<R: SuperblockReader>(r: &R) -> bool {
            r.read_super(0, false, false).is_ok()
        }
        assert!(via(&mem));
        assert!(!via(&FailSuperblock));
        // Threading flag fans out to worker allowance (`mount.c:309-313`).
        assert_eq!(thread_allowance(true), NR_WTHREADS);
        assert_eq!(thread_allowance(false), 1);
    }

    #[test]
    fn test_commit_guards() {
        // Mounted devices reject remounts (`mount.c:191-196`).
        assert!(check_dev_free(&[0x801, 0x802], 0x803).is_ok());
        assert_eq!(
            check_dev_free(&[0x801], 0x801).unwrap_err(),
            MountError::Busy
        );
        // Glue needs exactly one reference (`mount.c:218-222`).
        assert!(glue_decision(1).is_ok());
        assert_eq!(glue_decision(2).unwrap_err(), MountError::Busy);
        assert_eq!(glue_decision(0).unwrap_err(), MountError::Busy);
        // Type clash matrix (`mount.c:352-353`).
        assert!(type_clash(true, true).is_ok());
        assert!(type_clash(true, false).is_ok());
        assert!(type_clash(false, false).is_ok());
        assert_eq!(type_clash(false, true).unwrap_err(), MountError::IsDir);
        assert_eq!(MountError::IsDir.to_errno(), minix_types::EISDIR);
        assert_eq!(MountError::Busy.to_errno(), minix_types::EBUSY);
    }

    #[test]
    fn test_root_stage_machine() {
        // `have_root < 2` admits exactly two root mounts (`mount.c:206`).
        assert!(RootStage::Absent.mount_takes_root_path(true));
        assert!(RootStage::Ramdisk.mount_takes_root_path(true));
        assert!(!RootStage::Bootdisk.mount_takes_root_path(true));
        assert!(!RootStage::Absent.mount_takes_root_path(false));
        // Advance saturates (`have_root++` past 2 stays a normal mount).
        assert_eq!(RootStage::Absent.advance(), RootStage::Ramdisk);
        assert_eq!(RootStage::Ramdisk.advance(), RootStage::Bootdisk);
        assert_eq!(RootStage::Bootdisk.advance(), RootStage::Bootdisk);
    }

    #[test]
    fn test_bspec_routing_table() {
        // Out-of-range majors and vanished drivers are skipped.
        assert_eq!(bspec_update(false, true, true), BspecAction::SkipOutOfRange);
        assert_eq!(bspec_update(true, false, true), BspecAction::SkipVanished);
        // The send flag decides between retarget-only and retarget+notify.
        assert_eq!(bspec_update(true, true, true), BspecAction::SetAndSend);
        assert_eq!(bspec_update(true, true, false), BspecAction::SetOnly);
        assert_eq!(bspec_update(true, false, false), BspecAction::SetOnly);
    }

    #[test]
    fn test_mount_head_gate() {
        // Super-user gate first (`mount.c:107-108`).
        assert_eq!(
            validate_mount_head(false, 0, 16, 0, 16, 0).unwrap_err(),
            MountError::Perm
        );
        assert_eq!(MountError::Perm.to_errno(), minix_types::EPERM);
        // Label/type bounds (`mount.c:111,144`).
        assert_eq!(
            validate_mount_head(true, 17, 16, 0, 16, 0).unwrap_err(),
            MountError::Inval
        );
        assert_eq!(
            validate_mount_head(true, 0, 16, 17, 16, 0).unwrap_err(),
            MountError::NameTooLong
        );
        assert_eq!(
            MountError::NameTooLong.to_errno(),
            minix_types::ENAMETOOLONG
        );
        // Empty device name takes the pseudo path (`mount.c:126`).
        assert_eq!(
            validate_mount_head(true, 0, 16, 0, 16, 0).unwrap(),
            DevSource::Pseudo
        );
        assert_eq!(
            validate_mount_head(true, 0, 16, 0, 16, 5).unwrap(),
            DevSource::Real
        );
    }

    #[test]
    fn test_name_classification() {
        // Block files resolve to their device (`mount.c:609-610`).
        assert_eq!(
            classify_name(true, true, 0x801, false, false, 0).unwrap(),
            0x801
        );
        // Mount roots resolve with permission (`mount.c:611-612`).
        assert_eq!(
            classify_name(true, false, 0, true, true, 0x802).unwrap(),
            0x802
        );
        assert_eq!(
            classify_name(true, false, 0, false, true, 0x802).unwrap_err(),
            MountError::NotBlk
        );
        assert_eq!(
            classify_name(true, false, 0, true, false, 0x802).unwrap_err(),
            MountError::NotBlk
        );
        assert_eq!(MountError::NotBlk.to_errno(), minix_types::ENOTBLK);
        // Failed lookups propagate (`mount.c:607`).
        assert_eq!(
            classify_name(false, true, 0x801, true, true, 0x802).unwrap_err(),
            MountError::Inval
        );
    }

    #[test]
    fn test_teardown_plan() {
        // Busy trees refuse (`mount.c:501-504`): refs, locks, pending.
        assert!(busy_check(1, 0, false).is_ok());
        assert!(busy_check(1, 1, false).is_ok());
        assert_eq!(busy_check(2, 0, false).unwrap_err(), MountError::Busy);
        assert_eq!(busy_check(1, 2, false).unwrap_err(), MountError::Busy);
        assert_eq!(busy_check(1, 0, true).unwrap_err(), MountError::Busy);
        // Ordinary pseudo FS: free the minor, clear the root, notify driver.
        assert_eq!(
            plan_unmount(true, true),
            UnmountPlan {
                free_nonedev: true,
                clear_root: true,
                send_driver: true
            }
        );
        // PFS has no root node to clear (`mount.c:530-535`).
        assert_eq!(
            plan_unmount(true, false),
            UnmountPlan {
                free_nonedev: true,
                clear_root: false,
                send_driver: true
            }
        );
        // Real devices keep their numbers.
        assert_eq!(
            plan_unmount(false, true),
            UnmountPlan {
                free_nonedev: false,
                clear_root: true,
                send_driver: true
            }
        );
        // Force-shutdown residue is an error, not a panic here.
        assert!(verify_empty(&[false, false]).is_ok());
        assert_eq!(verify_empty(&[false, true]).unwrap_err(), MountError::Busy);
        assert_eq!(sweep_passes(16), 16);
    }

    #[test]
    fn test_errno_map_covers_mount_c() {
        let cases = [
            (MountError::Perm, minix_types::EPERM),
            (MountError::Inval, minix_types::EINVAL),
            (MountError::NameTooLong, minix_types::ENAMETOOLONG),
            (MountError::Busy, minix_types::EBUSY),
            (MountError::NoMem, minix_types::ENOMEM),
            (MountError::NotBlk, minix_types::ENOTBLK),
            (MountError::NoSpace, minix_types::EMFILE),
            (MountError::IsDir, minix_types::EISDIR),
            (MountError::Io, minix_types::EIO),
        ];
        for (err, errno) in cases {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
