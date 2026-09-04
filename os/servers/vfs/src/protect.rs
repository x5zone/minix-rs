//! `protect` — asking leave: owners, groups, masks, and the verdict core.
//!
//! Corresponds to Minix3's `protect.c:1-302` (`do_chmod`, `do_chown`,
//! `do_umask`, `do_access`, `forbidden`, `read_only`) and `in_group`
//! (`utility.c:128-141`).
//!
//! Design decisions (see 29-protect.md §3):
//! - `ProtectCall` types the six calls (road/finger split by call number)
//! - `chmod_gate`/`chown_gate` type the owner doors (order differs)
//! - `strip_setgid`/`keep_id`/`check_id_bounds` type the small change
//! - `umask_swap` types the complement tango purely
//! - `AccessBits` + `check_access_mode` type the asking-mode door
//! - `forbidden_decision` types the five-act verdict core
//! - `readonly_gate` + `ProtectFs` type the mountain door and FS dialogue
//!
//! Scope note: road execution (`eat_path`/`get_filp`) stays with
//! 13-path-lookup.md/14-filedes.md; FS requests (`req_chmod`/`req_chown`)
//! execute FS-side (12-request-wrappers.md describes the envelopes);
//! credential fields live with 02-fproc-struct.md, their setting with
//! 10-pm-protocol.md. This module only decides: gate, change, mask,
//! verdict, and dialogue.
//!
//! Linux models the same core as `inode_permission` (owner/group/other
//! shifts over nine bits, root exception, read-only remount check) with
//! `posix_acl` aside; Redox models it as capability checks over handles.
//! Here `forbidden_decision` is the core and [`ProtectFs`] is the
//! per-filesystem answer.

use crate::link::SU_UID;

/// `R_BIT` (`minix3/minix/include/minix/const.h:117`): read protection bit.
pub const R_BIT: u8 = 0o4;
/// `W_BIT` (`const.h:118`): write protection bit.
pub const W_BIT: u8 = 0o2;
/// `X_BIT` (`const.h:119`): execute protection bit.
pub const X_BIT: u8 = 0o1;
/// All three protection bits.
pub const RWX_BITS: u8 = R_BIT | W_BIT | X_BIT;

/// `R_OK` (`minix3/sys/sys/unistd.h:171`): test for read permission.
///
/// Same value as [`R_BIT`], different domain: asking bits (`R_OK`)
/// versus stored bits (`R_BIT`). The C code relies on the coincidence
/// (`forbidden(fp, vp, access)` compares them directly, `277`).
pub const R_OK: u8 = 0x04;
/// `W_OK` (`unistd.h:170`).
pub const W_OK: u8 = 0x02;
/// `X_OK` (`unistd.h:169`).
pub const X_OK: u8 = 0x01;
/// `F_OK` (`unistd.h:168`): existence only (the empty set).
pub const F_OK: u8 = 0x00;

/// `I_SET_UID_BIT` (`minix3/minix/include/minix/const.h:112`).
pub const I_SET_UID_BIT: u32 = 0o4000;
/// `I_SET_GID_BIT` (`const.h:113`).
pub const I_SET_GID_BIT: u32 = 0o2000;
/// `RWX_MODES` (`const.h:116`): the low nine mode bits.
pub const RWX_MODES: u32 = 0o777;
/// `UID_MAX`/`GID_MAX` (`minix3/sys/sys/syslimits.h:53,60`): 2^31-2.
pub const ID_MAX: u32 = 2147483647;
/// Expired id (`(uid_t)-1`/`(gid_t)-1`, `forbidden:251`).
pub const ID_EXPIRED: u32 = u32::MAX;

/// The six protection calls (header contract, `protect.c:1-9`).
///
/// `chmod`/`chown` each double as their `f` variant, split by call
/// number (`46-60`, `118-138`); the split is a call-surface fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectCall {
    /// `VFS_CHMOD`: change mode by path.
    Chmod,
    /// `VFS_FCHMOD`: change mode by fd.
    Fchmod,
    /// `VFS_CHOWN`: change owner by path.
    Chown,
    /// `VFS_FCHOWN`: change owner by fd.
    Fchown,
    /// `VFS_UMASK`: set the creation mask.
    Umask,
    /// `VFS_ACCESS`: test access by path.
    Access,
}

impl ProtectCall {
    /// Calls that walk a road (vs calls holding an fd already).
    pub fn by_path(self) -> bool {
        matches!(self, Self::Chmod | Self::Chown | Self::Access)
    }
}

/// `do_chmod` owner door (`protect.c:64-70`).
///
/// Only the owner or root may change a mode; nobody changes modes on a
/// read-only mount. Owner first, mountain second.
pub fn chmod_gate(is_owner: bool, is_root: bool, readonly_ok: bool) -> Result<(), ProtectError> {
    if !is_owner && !is_root {
        return Err(ProtectError::Perm);
    }
    if !readonly_ok {
        return Err(ProtectError::RoFs);
    }
    Ok(())
}

/// `do_chown` owner rules (`protect.c:140-151`).
///
/// Mount first, then (for regular users): must own the file, must not
/// give it away (new uid equals the file's), new group must be one's
/// own. Root skips the three vows.
pub fn chown_gate(
    is_root: bool,
    owns_file: bool,
    same_uid: bool,
    same_gid: bool,
    readonly_ok: bool,
) -> Result<(), ProtectError> {
    if !readonly_ok {
        return Err(ProtectError::RoFs);
    }
    if is_root {
        return Ok(());
    }
    if !owns_file {
        return Err(ProtectError::Perm);
    }
    if !same_uid {
        return Err(ProtectError::Perm);
    }
    if !same_gid {
        return Err(ProtectError::Perm);
    }
    Ok(())
}

/// Drop the setgid bit for outsiders (`do_chmod:75-76`).
///
/// Non-root callers outside the file's group lose setgid on chmod.
pub fn strip_setgid(is_root: bool, file_gid: u32, eff_gid: u32, mode: u32) -> u32 {
    if !is_root && file_gid != eff_gid {
        mode & !I_SET_GID_BIT
    } else {
        mode
    }
}

/// `-1` keeps the id (`do_chown:154-156`).
///
/// The C wire value `-1` means "don't touch"; the rewrite says it with
/// `None` (no magic numbers at decision points).
pub fn keep_id(new_id: Option<u32>, current: u32) -> u32 {
    new_id.unwrap_or(current)
}

/// Id bounds (`do_chown:158-159`): past 2^31-2 is `EINVAL`.
pub fn check_id_bounds(uid: u32, gid: u32) -> Result<(), ProtectError> {
    if uid > ID_MAX || gid > ID_MAX {
        return Err(ProtectError::Inval);
    }
    Ok(())
}

/// The complement tango (`do_umask:189-191`).
///
/// Masks store inverted and return the old complement: returns
/// `(stored, returned)` so the pairing is testable as one fact.
pub fn umask_swap(old_mask: u32, new_mask: u32) -> (u32, u32) {
    (!(new_mask & RWX_MODES), !old_mask)
}

/// Asking-mode door (`do_access:217-218`).
///
/// Anything outside R/W/X is `EINVAL` — except `F_OK` (0), which asks
/// existence only and always passes the door.
pub fn check_access_mode(mode: u8) -> Result<(), ProtectError> {
    if mode == F_OK {
        return Ok(());
    }
    if mode & !RWX_BITS != 0 {
        return Err(ProtectError::Inval);
    }
    Ok(())
}

/// Supplementary-group membership (`in_group`, `utility.c:128-141`).
///
/// The C loop returns `OK`/`EINVAL`; the rewrite says it with a bool
/// (found or not — the odd `EINVAL`-for-"no" stays caller-side).
pub fn in_supplementary(sgroups: &[u32], grp: u32) -> bool {
    sgroups.contains(&grp)
}

/// Verdict input (`forbidden:238-287`, one call's worth of facts).
#[derive(Debug, Clone, Copy)]
pub struct ForbidInput<'a> {
    /// Caller's real uid/gid.
    pub real_uid: u32,
    /// Caller's real gid.
    pub real_gid: u32,
    /// Caller's effective uid/gid.
    pub eff_uid: u32,
    /// Caller's effective gid.
    pub eff_gid: u32,
    /// `VFS_ACCESS` asks with real ids, others with effective (`255-256`).
    pub is_access_call: bool,
    /// File owner's uid/gid.
    pub file_uid: u32,
    /// File owner's gid.
    pub file_gid: u32,
    /// File mode (low nine bits select thepermission tier ladder).
    pub mode: u32,
    /// Desired access (`R/W/X` bits).
    pub access: u8,
    /// Target is a directory.
    pub is_dir: bool,
    /// Supplementary groups.
    pub supp: &'a [u32],
    /// Mount is read-only.
    pub readonly_fs: bool,
}

/// The verdict core (`forbidden:238-287`).
///
/// Five acts: expired ids refuse (`251`); ACCESS calls use real ids,
/// others effective (`255-256`); root takes all-read-write plus execute
/// for directories or any-X files (`258-266`); everyone else reads
/// theirpermission tier by shift — owner 6, group or supplementary 3, other 0
/// (`268-272`); desired must sit inside allowed (`277`); writes on a
/// read-only mount refuse last (`282-284`).
pub fn forbidden_decision(input: &ForbidInput) -> Result<(), ProtectError> {
    if input.file_uid == ID_EXPIRED || input.file_gid == ID_EXPIRED {
        return Err(ProtectError::Acces);
    }
    let (uid, gid) = if input.is_access_call {
        (input.real_uid, input.real_gid)
    } else {
        (input.eff_uid, input.eff_gid)
    };
    let perm: u8 = if uid == SU_UID {
        if input.is_dir || input.mode & 0o111 != 0 {
            RWX_BITS
        } else {
            R_BIT | W_BIT
        }
    } else {
        let shift = if uid == input.file_uid {
            6
        } else if gid == input.file_gid || in_supplementary(input.supp, input.file_gid) {
            3
        } else {
            0
        };
        ((input.mode >> shift) & u32::from(RWX_BITS)) as u8
    };
    if (perm | input.access) != perm {
        return Err(ProtectError::Acces);
    }
    if input.access & W_BIT != 0 && input.readonly_fs {
        return Err(ProtectError::RoFs);
    }
    Ok(())
}

/// Mount read-only door (`read_only:292-302`).
///
/// No mount pointer means writable (`vp->v_vmnt` NULL → OK); a mounted
/// read-only flag means `EROFS`.
pub fn readonly_gate(has_vmnt: bool, readonly_flag: bool) -> Result<(), ProtectError> {
    if has_vmnt && readonly_flag {
        return Err(ProtectError::RoFs);
    }
    Ok(())
}

/// Applied ownership (`do_chown:160-164`): FS answer written back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChownApplied {
    /// Installed uid/gid.
    pub uid: u32,
    /// Installed gid.
    pub gid: u32,
    /// FS-returned mode (the FS has the last word, not us).
    pub mode: u32,
}

/// The FS dialogue behind a trait.
///
/// `req_chmod`/`req_chown` execute FS-side (including the mode the FS
/// reports back, which we store verbatim, `78-80`); the FS is the only
/// untestable point, so only the dialogue is abstracted.
pub trait ProtectFs {
    /// `req_chmod`: install a mode, report the FS-stored mode.
    fn chmod(&mut self, fs: u64, inode: u64, mode: u32) -> Result<u32, ProtectError>;
    /// `req_chown`: install owner/group, report all three back.
    fn chown(
        &mut self,
        fs: u64,
        inode: u64,
        uid: u32,
        gid: u64,
    ) -> Result<ChownApplied, ProtectError>;
}

/// Scripted FS (test double with programmed answers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedProtect {
    /// Programmed `chmod` answer (applied mode).
    pub chmod_mode: Result<u32, ProtectError>,
    /// Programmed `chown` answer.
    pub chown_out: Result<ChownApplied, ProtectError>,
    /// Downcalls made (observable dialogue).
    pub ncalls: u32,
}

impl Default for ScriptedProtect {
    fn default() -> Self {
        Self {
            chmod_mode: Ok(0o644),
            chown_out: Ok(ChownApplied {
                uid: 0,
                gid: 0,
                mode: 0o644,
            }),
            ncalls: 0,
        }
    }
}

impl ProtectFs for ScriptedProtect {
    fn chmod(&mut self, _fs: u64, _inode: u64, _mode: u32) -> Result<u32, ProtectError> {
        self.ncalls += 1;
        self.chmod_mode
    }
    fn chown(
        &mut self,
        _fs: u64,
        _inode: u64,
        _uid: u32,
        _gid: u64,
    ) -> Result<ChownApplied, ProtectError> {
        self.ncalls += 1;
        self.chown_out
    }
}

/// Refusing FS (test double: every downcall fails with `EIO`).
///
/// Behaves differently from [`ScriptedProtect`] (blanket refusal vs
/// programmed answers), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefusingProtect;

impl ProtectFs for RefusingProtect {
    fn chmod(&mut self, _fs: u64, _inode: u64, _mode: u32) -> Result<u32, ProtectError> {
        Err(ProtectError::Io)
    }
    fn chown(
        &mut self,
        _fs: u64,
        _inode: u64,
        _uid: u32,
        _gid: u64,
    ) -> Result<ChownApplied, ProtectError> {
        Err(ProtectError::Io)
    }
}

/// L2 contract probe: chmod through any FS, report the applied mode.
pub fn chmod_propagates<P: ProtectFs>(fs: &mut P, mode: u32) -> Result<u32, ProtectError> {
    fs.chmod(1, 2, mode)
}

/// What the call tells the main loop (ARCH A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectVerdict {
    /// Reply now (status carried separately).
    Done,
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// Lookup failures ride `err_code` upstream (caller-side inputs), so
/// they are not variants here; `EBADF` never fires in this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectError {
    /// `EPERM`: not the owner, sticky-class denials, giving away.
    Perm,
    /// `EACCES`: the verdict core says no.
    Acces,
    /// `EINVAL`: wild asking modes, over-limit ids.
    Inval,
    /// `EROFS`: writes on a read-only mount.
    RoFs,
    /// `EIO`: FS-side refusal.
    Io,
}

impl ProtectError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Perm => minix_types::EPERM,
            Self::Acces => minix_types::EACCES,
            Self::Inval => minix_types::EINVAL,
            Self::RoFs => minix_types::EROFS,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calls_and_owner_doors() {
        // Six surface calls (`protect.c:1-9`, road/finger split).
        let all = [
            ProtectCall::Chmod,
            ProtectCall::Fchmod,
            ProtectCall::Chown,
            ProtectCall::Fchown,
            ProtectCall::Umask,
            ProtectCall::Access,
        ];
        assert_eq!(all.len(), 6);
        assert!(ProtectCall::Chmod.by_path());
        assert!(!ProtectCall::Fchmod.by_path());
        assert!(ProtectCall::Access.by_path());
        assert!(!ProtectCall::Umask.by_path());
        // chmod: owner or root, then the mountain (`64-70`).
        assert!(chmod_gate(true, false, true).is_ok());
        assert!(chmod_gate(false, true, true).is_ok());
        assert_eq!(chmod_gate(false, false, true), Err(ProtectError::Perm));
        assert_eq!(chmod_gate(true, false, false), Err(ProtectError::RoFs));
        // chown: mountain first, then the three vows (`140-151`).
        assert_eq!(
            chown_gate(false, true, true, true, false),
            Err(ProtectError::RoFs)
        );
        assert!(chown_gate(true, false, false, false, true).is_ok());
        assert!(chown_gate(false, true, true, true, true).is_ok());
        assert_eq!(
            chown_gate(false, false, true, true, true),
            Err(ProtectError::Perm)
        );
        assert_eq!(
            chown_gate(false, true, false, true, true),
            Err(ProtectError::Perm)
        );
        assert_eq!(
            chown_gate(false, true, true, false, true),
            Err(ProtectError::Perm)
        );
    }

    #[test]
    fn test_small_change_and_mask() {
        // Outsiders lose setgid on chmod (`75-76`).
        assert_eq!(strip_setgid(false, 10, 20, 0o6777), 0o4777);
        assert_eq!(strip_setgid(false, 10, 10, 0o6777), 0o6777);
        assert_eq!(strip_setgid(true, 10, 20, 0o6777), 0o6777);
        // -1 keeps the id (`154-156`, spelled None here).
        assert_eq!(keep_id(None, 100), 100);
        assert_eq!(keep_id(Some(200), 100), 200);
        // Past 2^31-2 refuses (`158-159`).
        assert!(check_id_bounds(100, 200).is_ok());
        assert_eq!(check_id_bounds(ID_MAX + 1, 0), Err(ProtectError::Inval));
        assert_eq!(check_id_bounds(0, ID_MAX + 1), Err(ProtectError::Inval));
        // Masks store inverted, return the old complement (`189-191`).
        assert_eq!(umask_swap(0o022, 0o027), (!(0o027 & RWX_MODES), !0o022));
        let (stored, _) = umask_swap(0, 0o7777);
        assert_eq!(stored, !(0o7777 & RWX_MODES));
        assert_eq!(stored, !0o777);
    }

    #[test]
    fn test_asking_mode_door() {
        // F_OK (0) asks existence only and passes (`217-218`).
        assert!(check_access_mode(F_OK).is_ok());
        assert!(check_access_mode(R_OK).is_ok());
        assert!(check_access_mode(R_OK | W_OK | X_OK).is_ok());
        // Anything outside R/W/X refuses.
        assert_eq!(check_access_mode(0x08), Err(ProtectError::Inval));
        assert_eq!(check_access_mode(0x10 | R_OK), Err(ProtectError::Inval));
    }

    #[test]
    fn test_verdict_core() {
        let base = ForbidInput {
            real_uid: 100,
            real_gid: 100,
            eff_uid: 100,
            eff_gid: 100,
            is_access_call: false,
            file_uid: 100,
            file_gid: 100,
            mode: 0o640,
            access: R_OK,
            is_dir: false,
            supp: &[],
            readonly_fs: false,
        };
        // Owner reads own file.
        assert!(forbidden_decision(&base).is_ok());
        // Owner writes own file (mode has W for owner).
        let mut input = base;
        input.access = W_OK;
        input.mode = 0o660;
        assert!(forbidden_decision(&input).is_ok());
        // Expired ids refuse (`251`).
        let mut input = base;
        input.file_uid = ID_EXPIRED;
        assert_eq!(forbidden_decision(&input), Err(ProtectError::Acces));
        // ACCESS calls use real ids (`255-256`): real stranger, eff owner.
        let mut input = base;
        input.is_access_call = true;
        input.real_uid = 200;
        input.real_gid = 200;
        input.file_uid = 100;
        input.mode = 0o600;
        assert_eq!(forbidden_decision(&input), Err(ProtectError::Acces));
        // Root reads anything; executes only dirs or any-X files (`258-266`).
        let mut input = base;
        input.eff_uid = SU_UID;
        input.access = R_OK;
        input.mode = 0o000;
        assert!(forbidden_decision(&input).is_ok());
        let mut input = base;
        input.eff_uid = SU_UID;
        input.access = X_OK;
        input.mode = 0o644;
        assert_eq!(forbidden_decision(&input), Err(ProtectError::Acces));
        let mut input = base;
        input.eff_uid = SU_UID;
        input.access = X_OK;
        input.is_dir = true;
        input.mode = 0o000;
        assert!(forbidden_decision(&input).is_ok());
        // Grouppermission tier via primary gid (`268-269`).
        let mut input = base;
        input.eff_uid = 200;
        input.eff_gid = 100;
        input.file_uid = 100;
        input.file_gid = 100;
        input.mode = 0o640;
        input.access = R_OK;
        assert!(forbidden_decision(&input).is_ok());
        // Supplementary groups count too (`270`).
        let supp = [100u32];
        let mut input = base;
        input.eff_uid = 200;
        input.eff_gid = 200;
        input.file_gid = 100;
        input.supp = &supp;
        input.mode = 0o640;
        input.access = R_OK;
        assert!(forbidden_decision(&input).is_ok());
        assert!(in_supplementary(&supp, 100));
        assert!(!in_supplementary(&supp, 7));
        // Others with nothing refuse (`277`).
        let mut input = base;
        input.eff_uid = 200;
        input.eff_gid = 200;
        input.file_uid = 100;
        input.file_gid = 100;
        input.mode = 0o600;
        input.access = R_OK;
        assert_eq!(forbidden_decision(&input), Err(ProtectError::Acces));
        // Writes on read-only mounts refuse last (`282-284`).
        let mut input = base;
        input.access = W_OK;
        input.mode = 0o660;
        input.readonly_fs = true;
        assert_eq!(forbidden_decision(&input), Err(ProtectError::RoFs));
        // Reads on read-only mounts pass.
        let mut input = base;
        input.access = R_OK;
        input.readonly_fs = true;
        assert!(forbidden_decision(&input).is_ok());
        // No mount pointer means writable (`301`).
        assert!(readonly_gate(false, true).is_ok());
        assert_eq!(readonly_gate(true, true), Err(ProtectError::RoFs));
        assert!(readonly_gate(true, false).is_ok());
    }

    #[test]
    fn test_fs_dialogue() {
        // L2 contract through any FS: chmod reports the applied mode.
        let mut scripted = ScriptedProtect::default();
        assert_eq!(chmod_propagates(&mut scripted, 0o644), Ok(0o644));
        assert_eq!(scripted.ncalls, 1);
        // Scripted failure propagates.
        let mut failing = ScriptedProtect {
            chmod_mode: Err(ProtectError::Io),
            ..Default::default()
        };
        assert_eq!(chmod_propagates(&mut failing, 0o644), Err(ProtectError::Io));
        // Blanket refusal (second impl).
        let mut refusing = RefusingProtect;
        assert_eq!(
            chmod_propagates(&mut refusing, 0o644),
            Err(ProtectError::Io)
        );
    }

    #[test]
    fn test_errno_map_covers_protect_c() {
        for (err, errno) in [
            (ProtectError::Perm, minix_types::EPERM),
            (ProtectError::Acces, minix_types::EACCES),
            (ProtectError::Inval, minix_types::EINVAL),
            (ProtectError::RoFs, minix_types::EROFS),
            (ProtectError::Io, minix_types::EIO),
        ] {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
        // Bit domains hold: asking and stored share values, root is zero.
        assert_eq!((R_BIT, W_BIT, X_BIT), (R_OK, W_OK, X_OK));
        assert_eq!(SU_UID, 0);
        assert_eq!(
            (I_SET_UID_BIT, I_SET_GID_BIT, RWX_MODES),
            (0o4000, 0o2000, 0o777)
        );
    }
}
