#![cfg_attr(not(test), no_std)]

//! System information and Minix specific tools core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/20-minix-system.md`:
//! version display (`minix3/minix/commands/version/version.sh`, prints the
//! contents of the version file), hardware clock handling
//! (`minix3/minix/commands/readclock/readclock.c`, read with
//! `RTCDEV_GET_TIME` near line 73, write with `RTCDEV_SET_TIME` near line 122,
//! complementary metal oxide semiconductor register access with
//! `RTCDEV_CMOSREG` near line 55, year 2000 workaround with `RTCDEV_Y2KBUG`
//! near line 59, usage `readclock [-nqwW2]` near line 164), interrupt scoped
//! execution (`minix3/minix/commands/intr/intr.c`, background mode flag near
//! line 51, log device `/dev/log` near line 19, alarm arming near line 140),
//! root device discovery (`minix3/minix/commands/printroot/printroot.c`,
//! device directory `/dev/` near line 25, unknown fallback
//! `/dev/unknown` near line 27, root comparison with `stat` near line 45),
//! system parameters (`minix3/sbin/sysctl/sysctl.c`, node type
//! `CTLTYPE_NODE` near line 520), static archive inspection
//! (`minix3/usr.bin/ldd/ldd.c`, executable and linkable format handling near
//! line 93), profiling (`minix3/minix/commands/profile/profile.c`,
//! `minix3/minix/commands/sprofalyze/sprofalyze.c`), and time zones
//! (`minix3/usr.sbin/zic/`, `minix3/usr.sbin/zdump/`).
//!
//! # Design
//!
//! These commands observe the system; they never steer it except through
//! narrow, auditable actions (setting the clock, running one command with a
//! deadline). What is pure here lives in this crate, what reads drivers,
//! walks the device directory, or queries the kernel stays with the execution
//! layer:
//!
//! - [`clock`]: hardware clock option parsing (preview, write to hardware,
//!   register access, year 2000 workaround, quiet), direction decision (read
//!   hardware into system versus write system into hardware), and retry
//!   policy (at most ten reads until a valid time arrives).
//! - [`intr`]: interrupt scoped execution options (background mode, deadline
//!   in seconds), usage parsing, and the foreground versus background setup
//!   decision.
//! - [`rootdev`]: root device discovery over the [`rootdev::DeviceDir`]
//!   trait (a slice backend plus an empty backend so tests run without a
//!   device directory).
//! - [`sysctl`]: system parameter name parsing (dotted names, `name=value`
//!   assignments) over the [`sysctl::SysctlTable`] trait.
//! - [`ldd`]: static archive listing (no dynamic linker exists, so the tool
//!   lists archive members instead of shared objects).
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout. Driver calls, directory walks, and kernel queries
//! stay with the execution layer behind [`rootdev::DeviceDir`] and
//! [`sysctl::SysctlTable`].

pub mod clock;
pub mod intr;
pub mod ldd;
pub mod rootdev;
pub mod sysctl;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): unknown options, bad names, bad
/// values. 2 marks a missing entry (`ENOENT`): a device, parameter, or member
/// with no record behind it. 1 marks a denied operation (`EPERM`, the same
/// number clock setting reports when the caller lacks permission).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysinfoError {
    /// Malformed input.
    InvalidArgument,
    /// No such device, parameter, or member.
    NotFound,
    /// Operation not permitted.
    Denied,
}

impl SysinfoError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            SysinfoError::InvalidArgument => 22,
            SysinfoError::NotFound => 2,
            SysinfoError::Denied => 1,
        }
    }
}
