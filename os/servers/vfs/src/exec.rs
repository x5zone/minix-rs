//! `exec` — process rebirth: check the volume, break the soul, land, rename.
//!
//! Corresponds to Minix3's `exec.c:1-763` (`get_read_vp`, `vfs_memmap`,
//! `pm_exec`, `stack_prepare_elf`, `is_script`, `patch_stack`,
//! `insert_arg`, `read_seg`, `clo_exec`, `map_header`).
//!
//! Design decisions (see 25-exec.md §3):
//! - `ExecPhase` types the eleven pipeline stages; `FAILCHECK` is `?`
//! - `check_identity`/`honor_suid` type the three opening checks
//! - `is_script`/`PatchPlan`/`calc_insert` type the script door and its arithmetic
//! - `classify_interpreter`/`loader_base`/switch flags type the dyn branch
//! - `check_seg`/`mmap_flags`/`ImageLoader` type the two landing roads
//! - `plan_aux`/`HeaderReader` type the aux note list and header read
//! - `settle_ids`/`plan_cleanup` type the closing checklist
//!
//! Scope note: path lookup (`eat_path`/`fetch_name`) stays with
//! 13-path-lookup.md; permission checks (`forbidden`) stay with
//! 29-protect.md; FS reads (`req_stat`/`req_readwrite`) stay with
//! 12-request-wrappers.md; fd-table execution stays with 14-filedes.md;
//! `minix_vfs_mmap`/`libexec_*` execute VM-side (02-stage-vm/20);
//! `libexec_pm_newexec` runs PM-side (04-stage-pm/17); waiting and replies
//! stay with 08/09. This module only decides: phase, gate, switch,
//! compensate, note, and settle.
//!
//! Linux models the same flow as `binfmt` chains (`binfmt_script` sniffs
//! `#!`, `binfmt_elf` loads, `flush_old_exec` settles); Redox models it as
//! an `exec` scheme that validates, maps, and renames. Here `ExecPhase`
//! is the chain and [`ImageLoader`] is the per-format answer.

/// `ARG_MAX` (`minix3/sys/sys/syslimits.h:49`): max exec stack bytes.
pub const ARG_MAX: usize = 256 * 1024;
// `PATH_MAX`: authoritative at `crate::path::PATH_MAX` (single source).
pub use crate::path::PATH_MAX;
/// `DEFAULT_STACK_LIMIT` (`minix3/minix/include/minix/sys_config.h:25`): 4MB.
pub const DEFAULT_STACK_LIMIT: u64 = 4 * 1024 * 1024;
// `PROC_NAME_LEN`: authoritative at `crate::fproc::PROC_NAME_LEN` (single source).
pub use crate::fproc::PROC_NAME_LEN;
/// `PROT_WRITE` (`minix3/sys/sys/mman.h:64`).
pub const PROT_WRITE: u32 = 0x02;
/// `MVM_WRITABLE` (`minix3/minix/include/minix/vm.h:34`).
pub const MVM_WRITABLE: u16 = 0x8000;
/// Loader reservation below the stack (`exec.c:302`, 10MB for ld.so).
pub const LOADER_RESERVE: u64 = 0xa0_0000;
/// Pointer size in argv/envp (`exec.c:67`, 64-bit).
pub const PTRSIZE: u64 = 8;
/// `LONG_MAX` bound for segment arithmetic (`exec.c:701`, 64-bit).
pub const LONG_MAX_U64: u64 = i64::MAX as u64;

/// The eleven pipeline stages (header contract, `exec.c:1-11`).
///
/// `FAILCHECK` (`exec.c:156`) is early return (`?`); the `pm_execfinal`
/// cleanup (`386-401`) is [`CleanupPlan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecPhase {
    /// Fetch the user stack before killing the old image (`226-234`).
    FetchStack,
    /// Open the executable volume (`254`, `get_read_vp`).
    OpenExec,
    /// `#!` branch: repatch the stack, open the interpreter (`260-271`).
    ScriptBranch,
    /// Dynamic branch: open the main program, switch volumes (`283-314`).
    DynBranch,
    /// Borrow a VM fd for `mmap` landing (`316-335`).
    BorrowVmFd,
    /// Run the loader table (`350-356`).
    LoadImage,
    /// Tell PM about the new image (`359`).
    NotifyPm,
    /// Loader stack setup hook (`365`).
    SetupStack,
    /// Copy the stack into the new image (`368-369`).
    CopyStack,
    /// Close CLOEXEC, apply setuid, rename (`374-384`).
    Settle,
    /// Returned with `*pc`/`*newsp` set.
    Done,
}

/// Identity-check failure (`get_read_vp:126-134`, in gate order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdCheck {
    /// Not a regular file → `ENOEXEC` (`126-127`).
    NotRegular,
    /// Execute forbidden → caller errno, usually `EACCES` (`128-129`).
    NoExecPerm,
    /// `req_stat` failed → I/O path error (`131-134`).
    StatFailed,
}

/// Run the three opening checks in order (`exec.c:126-134`).
pub fn check_identity(is_reg: bool, x_ok: bool, stat_ok: bool) -> Result<(), ExecError> {
    if !is_reg {
        return Err(ExecError::NoExec);
    }
    if !x_ok {
        return Err(ExecError::Acces);
    }
    if !stat_ok {
        return Err(ExecError::Io);
    }
    Ok(())
}

/// Setuid/setgid honoring (`get_read_vp:137-147`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuidHonor {
    /// Proposed effective uid.
    pub new_uid: u32,
    /// Proposed effective gid.
    pub new_gid: u32,
    /// Whether the loader may apply them at settle time.
    pub allow: bool,
}

/// Honor mode bits when the caller asks (`sugid`, `exec.c:137-147`).
///
/// With `sugid` clear the defaults pass through and `allow` stays false;
/// with it set, each raised bit adopts the file owner/group and arms
/// `allow` (`allow_setuid` covers both, `141,145`).
pub fn honor_suid(
    sugid: bool,
    suid_bit: bool,
    sgid_bit: bool,
    file_uid: u32,
    file_gid: u32,
    cur_uid: u32,
    cur_gid: u32,
) -> SuidHonor {
    if !sugid {
        return SuidHonor {
            new_uid: cur_uid,
            new_gid: cur_gid,
            allow: false,
        };
    }
    SuidHonor {
        new_uid: if suid_bit { file_uid } else { cur_uid },
        new_gid: if sgid_bit { file_gid } else { cur_gid },
        allow: suid_bit || sgid_bit,
    }
}

/// `is_script` (`exec.c:522-529`): `#!` in the first two header bytes.
pub fn is_script(hdr: &[u8]) -> bool {
    hdr.len() >= 2 && hdr[0] == b'#' && hdr[1] == b'!'
}

/// Script handling plan (`pm_exec:260-271`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchPlan {
    /// Plain binary: load as-is.
    Direct,
    /// Script: repatch the stack, then open the interpreter volume
    /// with `copyprogname` kept but `sugid` dropped (`270`: 1, 0).
    ViaInterpreter,
}

/// Choose the script plan (`exec.c:260`).
pub fn plan_patch(script: bool) -> PatchPlan {
    if script {
        PatchPlan::ViaInterpreter
    } else {
        PatchPlan::Direct
    }
}

/// `insert_arg` arithmetic outcome (`exec.c:607-683`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetCalc {
    /// Stack growth (positive) or shrink (negative) in bytes, word-aligned.
    pub offset: i64,
    /// New stack byte count.
    pub new_len: usize,
    /// New virtual stack pointer.
    pub new_vsp: u64,
}

/// Pure `insert_arg` arithmetic (`exec.c:626-652`).
///
/// - `old_len`: current `*stk_bytes`; `a0`: `argv[0]` offset in frame
///   (negative or past-the-end fails the bounds check, `629-632`).
/// - `arg_len`: `strlen(arg)+1`; `old_arg_len`: current `argv[0]` length
///   (replace mode only); `replace`: swap vs prepend mode.
/// - Word-aligns the delta (`645`), refuses past-`ARG_MAX` growth
///   (`648-652`, surfacing as `ENOMEM` upstream, `553,589-592`).
pub fn calc_insert(
    old_len: usize,
    a0: i64,
    arg_len: usize,
    old_arg_len: usize,
    replace: bool,
    vsp: u64,
) -> Result<OffsetCalc, ExecError> {
    if a0 < 0 || a0 as u64 >= old_len as u64 {
        return Err(ExecError::NoMem);
    }
    // Prepend adds one pointer plus the string; replace adds the length
    // difference of the two strings (`636-642`). Shrink is negative.
    let delta: i64 = if replace {
        arg_len as i64 - old_arg_len as i64
    } else {
        arg_len as i64 + PTRSIZE as i64
    };
    // Same remainder semantics as C (`%` keeps the dividend sign).
    let aligned = delta + (PTRSIZE as i64 - ((PTRSIZE as i64 + delta) % PTRSIZE as i64));
    let new_len = old_len as i64 + aligned;
    if new_len < 0 {
        return Err(ExecError::Inval);
    }
    if new_len as u64 > ARG_MAX as u64 {
        return Err(ExecError::NoMem);
    }
    Ok(OffsetCalc {
        offset: aligned,
        new_len: new_len as usize,
        new_vsp: vsp.wrapping_sub(aligned as u64),
    })
}

/// Dynamic-linker switch (`elf_has_interpreter` result, `exec.c:278-283`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynSwitch {
    /// No interpreter: load the volume itself.
    Static,
    /// Interpreter present: open the main program, switch volumes.
    Dynamic,
}

/// Classify the interpreter probe (`exec.c:280-283`).
///
/// Negative values are raw libexec errnos, passed through untouched
/// (the C `FAILCHECK(r)` propagates them as-is).
pub fn classify_interpreter(r: i32) -> Result<DynSwitch, i32> {
    if r < 0 {
        return Err(r);
    }
    if r > 0 {
        return Ok(DynSwitch::Dynamic);
    }
    Ok(DynSwitch::Static)
}

/// Loader base below the stack (`exec.c:301-302`): high − size − 10MB.
///
/// Traps NULL dereferences while leaving the loader relocatable.
pub fn loader_base(stack_high: u64, stack_size: u64) -> u64 {
    stack_high
        .saturating_sub(stack_size)
        .saturating_sub(LOADER_RESERVE)
}

/// Volume-switch `(copyprogname, sugid)` flags per switch point.
///
/// First volume `(1,1)` (`254`); script volume `(1,0)` (`270` — keep the
/// name, drop setuid honoring); interpreter volume `(0,0)` (`313`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VpSwitch {
    /// Remember the program basename.
    pub copyprogname: bool,
    /// Honor setuid/setgid bits.
    pub sugid: bool,
}

/// Ordered volume switches of one exec.
pub const VP_SWITCHES: [VpSwitch; 3] = [
    VpSwitch {
        copyprogname: true,
        sugid: true,
    },
    VpSwitch {
        copyprogname: true,
        sugid: false,
    },
    VpSwitch {
        copyprogname: false,
        sugid: false,
    },
];

/// `read_seg` gate (`exec.c:700-702`).
///
/// Guards the file-backed copy: wrapped-around ranges and over-long
/// ranges are `EIO` (a corrupt header must not drive the FS read).
pub fn check_seg(off: u64, seg_bytes: u64, file_size: u64) -> Result<(), ExecError> {
    let end = off.checked_add(seg_bytes).ok_or(ExecError::Io)?;
    if end > LONG_MAX_U64 || end > file_size {
        return Err(ExecError::Io);
    }
    Ok(())
}

/// `vfs_memmap` flag conversion (`exec.c:170-171`).
pub fn mmap_flags(prot_write: bool) -> u16 {
    if prot_write { MVM_WRITABLE } else { 0 }
}

/// VM-fd borrow gate (`exec.c:320-321`): FS must offer peek and the
/// volume must not be a memory device.
pub fn vmfd_gate(fs_has_peek: bool, is_mem_device: bool) -> bool {
    fs_has_peek && !is_mem_device
}

/// Loader-table outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOutcome {
    /// Image loaded; run the paired stack hook (may be none).
    Loaded {
        /// Whether a stack-setup hook follows.
        has_stack_hook: bool,
    },
}

/// One image loader: `load_object` + `setup_stack` hooks
/// (`exec_loaders[]`, `exec.c:73-81`).
pub trait ImageLoader {
    /// Load the image into the new address space.
    fn load(&mut self) -> Result<LoadOutcome, ExecError>;
    /// Prepare the stack frame (aux vectors and friends).
    fn setup_stack(&mut self) -> Result<(), ExecError>;
}

/// Scripted loader (test double with a programmed answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedLoader {
    /// Programmed load result.
    pub load_result: Result<LoadOutcome, ExecError>,
    /// Times `load` ran (observable dialogue).
    pub nloads: u32,
}

impl ImageLoader for ScriptedLoader {
    fn load(&mut self) -> Result<LoadOutcome, ExecError> {
        self.nloads += 1;
        self.load_result
    }
    fn setup_stack(&mut self) -> Result<(), ExecError> {
        Ok(())
    }
}

/// Null loader (the table terminator, `exec.c:80`).
///
/// Behaves differently from [`ScriptedLoader`] (blanket `ENOEXEC` vs
/// programmed answers): trying past the last real loader always fails,
// satisfying the "two behaviorally different impls" rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NullLoader;

impl ImageLoader for NullLoader {
    fn load(&mut self) -> Result<LoadOutcome, ExecError> {
        Err(ExecError::NoExec)
    }
    fn setup_stack(&mut self) -> Result<(), ExecError> {
        Err(ExecError::NoExec)
    }
}

/// Run the loader table: first `OK` wins (`exec.c:350-354`).
pub fn run_loaders<L: ImageLoader>(loaders: &mut [L]) -> Result<LoadOutcome, ExecError> {
    let mut last = Err(ExecError::NoExec);
    for loader in loaders.iter_mut() {
        match loader.load() {
            ok @ Ok(_) => return ok,
            err @ Err(_) => last = err,
        }
    }
    last
}

/// Aux-vector kinds `stack_prepare_elf` writes (`exec.c:480-514`).
///
/// Numeric `AT_*` values belong to the ELF ABI executed loader-side;
/// the decision layer only names the slots (plus the seal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuxKind {
    /// Interpreter load base.
    Base,
    /// Main entry point.
    Entry,
    /// Main-program fd for the loader.
    ExecFd,
    /// Effective uid/gid.
    Euid,
    /// Effective gid.
    Egid,
    /// Page size.
    PageSize,
    /// Executable name string.
    ExecName,
    /// Terminator (always last).
    Null,
}

/// Aux note plan (`stack_prepare_elf:424-514`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuxFill {
    /// Notes before the seal (six fixed + optional name).
    pub notes: [Option<AuxKind>; 7],
    /// Number of live notes.
    pub nnotes: usize,
    /// `AT_NULL` sealed at the end (`514`).
    pub sealed: bool,
}

/// Plan the aux notes.
///
/// Non-dynamic images need none (`424-425`, trivially sealed).
/// Dynamic images get six fixed notes; the name rides along only when
/// it fits (`495`), and the table always seals with `AT_NULL` — an
/// unsealed table lets the interpreter read wild vectors.
pub fn plan_aux(is_dyn: bool, name_fits: bool) -> AuxFill {
    if !is_dyn {
        return AuxFill {
            notes: [None; 7],
            nnotes: 0,
            sealed: true,
        };
    }
    let mut notes: [Option<AuxKind>; 7] = [None; 7];
    let fixed = [
        AuxKind::Base,
        AuxKind::Entry,
        AuxKind::ExecFd,
        AuxKind::Euid,
        AuxKind::Egid,
        AuxKind::PageSize,
    ];
    for (i, kind) in fixed.iter().enumerate() {
        notes[i] = Some(*kind);
    }
    let mut nnotes = fixed.len();
    if name_fits {
        notes[nnotes] = Some(AuxKind::ExecName);
        nnotes += 1;
    }
    AuxFill {
        notes,
        nnotes,
        sealed: true,
    }
}

/// Header read outcome (`map_header:751-752`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderInfo {
    /// Bytes of header available (`min(vsize, buf)`).
    pub hdr_len: u64,
}

/// The first-chunk read behind a trait.
///
/// `map_header` (`exec.c:736-763`) reads through the FS; the FS is the
/// only untestable point, so only the read is abstracted.
pub trait HeaderReader {
    /// Read the first chunk of a `file_size`-byte file.
    fn read_header(&mut self, file_size: u64) -> Result<HeaderInfo, ExecError>;
}

/// Scripted header (test double with a programmed length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedHeader(pub u64);

impl HeaderReader for ScriptedHeader {
    fn read_header(&mut self, file_size: u64) -> Result<HeaderInfo, ExecError> {
        Ok(HeaderInfo {
            hdr_len: file_size.min(self.0),
        })
    }
}

/// Zero header (test double: empty file, zero-length header).
///
/// Behaves differently from [`ScriptedHeader`] (fixed empty vs
/// programmed length), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ZeroHeader;

impl HeaderReader for ZeroHeader {
    fn read_header(&mut self, _file_size: u64) -> Result<HeaderInfo, ExecError> {
        Ok(HeaderInfo { hdr_len: 0 })
    }
}

/// Stack-frame budget (`pm_exec:226-227`): past-`ARG_MAX` stacks are `ENOMEM`.
pub fn frame_gate(frame_len: usize) -> Result<(), ExecError> {
    if frame_len > ARG_MAX {
        return Err(ExecError::NoMem);
    }
    Ok(())
}

/// Starting virtual stack pointer (`exec.c:238`).
pub fn stack_start(stack_high: u64, frame_len: u64) -> u64 {
    stack_high.wrapping_sub(frame_len)
}

/// Apply settled credentials (`pm_exec:376-381`).
///
/// Returns the ids to install: proposed ones when still allowed after
/// loading, current ones otherwise.
pub fn settle_ids(
    allow_setuid: bool,
    cur_uid: u32,
    cur_gid: u32,
    new_uid: u32,
    new_gid: u32,
) -> (u32, u32) {
    if allow_setuid {
        (new_uid, new_gid)
    } else {
        (cur_uid, cur_gid)
    }
}

/// Terminal cleanup order (`pm_execfinal:386-401`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupPlan {
    /// A VM-borrowed filp is live: unlock it (else unlock + release vp).
    pub unlock_newfilp: bool,
    /// An unused VM fd lingers: close it (`393-397`).
    pub close_vmfd: bool,
}

/// Plan terminal cleanup (`exec.c:387-397`).
pub fn plan_cleanup(has_newfilp: bool, vmfd: i32, vmfd_used: bool) -> CleanupPlan {
    CleanupPlan {
        unlock_newfilp: has_newfilp,
        close_vmfd: vmfd >= 0 && !vmfd_used,
    }
}

/// What `pm_exec` tells the main loop (ARCH A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecVerdict {
    /// Reply now with `*pc`/`*newsp` set.
    Done,
}

/// Errors of this module, each mapping to one Minix3 errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecError {
    /// `ENOEXEC`: not a regular file, bad script, no loader, bad headers.
    NoExec,
    /// `ENOMEM`: over-`ARG_MAX` stack or argument growth.
    NoMem,
    /// `EINVAL`: bad loader arithmetic inputs (kept distinct from `NoMem`
    /// bounds failures so callers can tell "bad" from "too big").
    Inval,
    /// `EIO`: short/corrupt segments, failed stat or header reads.
    Io,
    /// `EACCES`: execute forbidden.
    Acces,
    /// `EBADF`: dead fd on borrow paths.
    BadFd,
    /// `ENFILE`: vnode/fd table exhaustion on landing paths.
    NoSpace,
}

impl ExecError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::NoExec => minix_types::ENOEXEC,
            Self::NoMem => minix_types::ENOMEM,
            Self::Inval => minix_types::EINVAL,
            Self::Io => minix_types::EIO,
            Self::Acces => minix_types::EACCES,
            Self::BadFd => minix_types::EBADF,
            Self::NoSpace => minix_types::ENFILE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_and_identity() {
        // Eleven stages (`exec.c:1-11`).
        let phases = [
            ExecPhase::FetchStack,
            ExecPhase::OpenExec,
            ExecPhase::ScriptBranch,
            ExecPhase::DynBranch,
            ExecPhase::BorrowVmFd,
            ExecPhase::LoadImage,
            ExecPhase::NotifyPm,
            ExecPhase::SetupStack,
            ExecPhase::CopyStack,
            ExecPhase::Settle,
            ExecPhase::Done,
        ];
        assert_eq!(phases.len(), 11);
        // Opening checks run in gate order (`126-134`).
        assert_eq!(check_identity(false, true, true), Err(ExecError::NoExec));
        assert_eq!(check_identity(true, false, true), Err(ExecError::Acces));
        assert_eq!(check_identity(true, true, false), Err(ExecError::Io));
        assert!(check_identity(true, true, true).is_ok());
        // Setuid honoring follows the bits (`137-147`).
        let h = honor_suid(true, true, false, 100, 200, 0, 0);
        assert_eq!((h.new_uid, h.new_gid, h.allow), (100, 0, true));
        let h = honor_suid(true, false, true, 100, 200, 0, 0);
        assert_eq!((h.new_uid, h.new_gid, h.allow), (0, 200, true));
        let h = honor_suid(true, false, false, 100, 200, 7, 8);
        assert_eq!((h.new_uid, h.new_gid, h.allow), (7, 8, false));
        let h = honor_suid(false, true, true, 100, 200, 7, 8);
        assert_eq!((h.new_uid, h.new_gid, h.allow), (7, 8, false));
    }

    #[test]
    fn test_script_door_and_insert() {
        // `#!` needs two bytes (`522-529`).
        assert!(is_script(b"#!/bin/sh"));
        assert!(!is_script(b"#"));
        assert!(!is_script(b"ELF"));
        assert!(!is_script(b""));
        // Plan branches on the sniff (`260`).
        assert_eq!(plan_patch(true), PatchPlan::ViaInterpreter);
        assert_eq!(plan_patch(false), PatchPlan::Direct);
        // Out-of-frame argv[0] refuses (`629-632`).
        assert_eq!(
            calc_insert(100, -1, 8, 4, false, 1000),
            Err(ExecError::NoMem)
        );
        assert_eq!(
            calc_insert(100, 100, 8, 4, false, 1000),
            Err(ExecError::NoMem)
        );
        // Prepend grows by string plus one pointer (`636`), aligned.
        let c = calc_insert(128, 16, 7, 0, false, 4096).unwrap();
        assert_eq!(c.offset, 16);
        assert_eq!(c.new_len, 144);
        assert_eq!(c.new_vsp, 4080);
        // Replace grows by the length difference (`641`), aligned.
        let c = calc_insert(128, 16, 12, 8, true, 4096).unwrap();
        assert_eq!(c.offset, 8);
        assert_eq!(c.new_len, 136);
        assert_eq!(c.new_vsp, 4088);
        // Replace can shrink the stack, but the C aligner rounds the
        // delta strictly upward (`645`): an already-aligned delta of -8
        // lands on 0, not -8. Mirrored exactly (harmless: the stack only
        // ever grows a word more than strictly needed).
        let c = calc_insert(128, 16, 4, 12, true, 4096).unwrap();
        assert_eq!(c.offset, 0);
        assert_eq!(c.new_len, 128);
        assert_eq!(c.new_vsp, 4096);
        // Same quirk upward: aligned +8 lands on +16.
        let c = calc_insert(128, 16, 12, 4, true, 4096).unwrap();
        assert_eq!(c.offset, 16);
        assert_eq!(c.new_len, 144);
        // Past-ARG_MAX growth refuses (`648-652`).
        assert_eq!(
            calc_insert(ARG_MAX - 4, 16, 1, 0, false, 4096),
            Err(ExecError::NoMem)
        );
    }

    #[test]
    fn test_dyn_switch() {
        // Three-valued probe: error/static/dynamic (`280-283`).
        assert_eq!(classify_interpreter(-5), Err(-5));
        assert_eq!(classify_interpreter(0), Ok(DynSwitch::Static));
        assert_eq!(classify_interpreter(1), Ok(DynSwitch::Dynamic));
        // Loader base reserves 10MB below the stack (`301-302`).
        assert_eq!(
            loader_base(0x8000_0000, DEFAULT_STACK_LIMIT),
            0x8000_0000 - DEFAULT_STACK_LIMIT - LOADER_RESERVE
        );
        assert_eq!(loader_base(100, 1_000_000), 0);
        // Switch flags march 1,1 → 1,0 → 0,0 (`254,270,313`).
        assert_eq!(
            VP_SWITCHES,
            [
                VpSwitch {
                    copyprogname: true,
                    sugid: true
                },
                VpSwitch {
                    copyprogname: true,
                    sugid: false
                },
                VpSwitch {
                    copyprogname: false,
                    sugid: false
                },
            ]
        );
    }

    #[test]
    fn test_landing_roads() {
        // Wrapped or over-long ranges are EIO (`700-702`).
        assert_eq!(check_seg(u64::MAX, 1, u64::MAX), Err(ExecError::Io));
        assert_eq!(check_seg(0, 100, 50), Err(ExecError::Io));
        assert!(check_seg(0, 50, 50).is_ok());
        assert!(check_seg(0, 0, 0).is_ok());
        // WRITE converts to WRITABLE (`170-171`).
        assert_eq!(mmap_flags(true), MVM_WRITABLE);
        assert_eq!(mmap_flags(false), 0);
        // Borrow needs peek and a non-mem device (`320-321`).
        assert!(vmfd_gate(true, false));
        assert!(!vmfd_gate(false, false));
        assert!(!vmfd_gate(true, true));
        // First OK loader wins (`350-354`).
        let mut table = [
            ScriptedLoader {
                load_result: Err(ExecError::NoExec),
                nloads: 0,
            },
            ScriptedLoader {
                load_result: Ok(LoadOutcome::Loaded {
                    has_stack_hook: true,
                }),
                nloads: 0,
            },
        ];
        let out = run_loaders(&mut table).unwrap();
        assert_eq!(
            out,
            LoadOutcome::Loaded {
                has_stack_hook: true
            }
        );
        assert_eq!(table[0].nloads, 1);
        assert_eq!(table[1].nloads, 1);
        // Terminator always fails (`80`).
        let mut null = [NullLoader, NullLoader];
        assert_eq!(run_loaders(&mut null), Err(ExecError::NoExec));
    }

    #[test]
    fn test_aux_notes_and_header() {
        // Static images need no notes (`424-425`).
        let fill = plan_aux(false, true);
        assert_eq!((fill.nnotes, fill.sealed), (0, true));
        // Dynamic images get six notes plus the name when it fits.
        let fill = plan_aux(true, true);
        assert_eq!(fill.nnotes, 7);
        assert_eq!(fill.notes[6], Some(AuxKind::ExecName));
        assert!(fill.sealed);
        // Tight rooms drop the name but keep the seal (`495,514`).
        let fill = plan_aux(true, false);
        assert_eq!(fill.nnotes, 6);
        assert!(fill.sealed);
        // Header takes the smaller of file and buffer (`751`).
        let mut hdr = ScriptedHeader(40 * 4096);
        assert_eq!(hdr.read_header(100).unwrap().hdr_len, 100);
        assert_eq!(hdr.read_header(u64::MAX).unwrap().hdr_len, 40 * 4096);
        // Empty files read zero (`ZeroHeader` second impl).
        assert_eq!(ZeroHeader.read_header(100).unwrap().hdr_len, 0);
    }

    #[test]
    fn test_frame_and_settle() {
        // Over-ARG_MAX stacks refuse (`226-227`).
        assert!(frame_gate(ARG_MAX).is_ok());
        assert_eq!(frame_gate(ARG_MAX + 1), Err(ExecError::NoMem));
        // vsp starts below the top (`238`).
        assert_eq!(stack_start(0x8000, 0x100), 0x7F00);
        // Credentials apply only while allowed (`376-381`).
        assert_eq!(settle_ids(true, 0, 0, 100, 200), (100, 200));
        assert_eq!(settle_ids(false, 7, 8, 100, 200), (7, 8));
        // Cleanup unlocks the filp xor the vnode, and closes idle vmfds.
        assert_eq!(
            plan_cleanup(true, 5, false),
            CleanupPlan {
                unlock_newfilp: true,
                close_vmfd: true
            }
        );
        assert_eq!(
            plan_cleanup(false, 5, true),
            CleanupPlan {
                unlock_newfilp: false,
                close_vmfd: false
            }
        );
        assert_eq!(
            plan_cleanup(false, -1, false),
            CleanupPlan {
                unlock_newfilp: false,
                close_vmfd: false
            }
        );
    }

    #[test]
    fn test_errno_map_covers_exec_c() {
        for (err, errno) in [
            (ExecError::NoExec, minix_types::ENOEXEC),
            (ExecError::NoMem, minix_types::ENOMEM),
            (ExecError::Inval, minix_types::EINVAL),
            (ExecError::Io, minix_types::EIO),
            (ExecError::Acces, minix_types::EACCES),
            (ExecError::BadFd, minix_types::EBADF),
            (ExecError::NoSpace, minix_types::ENFILE),
        ] {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
