#![cfg_attr(not(test), no_std)]

//! Process and session information core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/12-process-tools.md`:
//! the process tools (`minix3/bin/ps/ps.c` with its keyword table in
//! `keyword.c`, `minix3/bin/kill/kill.c` with `signame_to_signum` at line
//! 188 over the system signal names) and the session face (`utmp` records
//! as in `minix3/lib/libc/compat/include/utmp.h`: terminal line, user
//! name, host, time).
//!
//! # Design
//!
//! These commands report and steer; they never compute much. What is pure
//! here: signal name/number conversion (the `-l` list and `-9`/`-TERM`
//! spellings), duration formatting (`time`, `ps` elapsed columns), login
//! record parsing (`who`, `w`, `last`), and process table listing over the
//! [`stable::ProcessTable`] trait. Process inspection itself (kernel
//! tables, terminal checks) stays with the execution layer. Everything
//! borrows from the input and uses fixed size buffers: no heap, `no_std`
//! throughout.
//!
//! # Modules
//!
//! - [`signal`]: signal name/number table (exact NetBSD/Minix numbers from
//!   `minix3/sys/sys/signal.h` lines 52 to 84).
//! - [`ptime`]: duration formatting (`[[dd-]hh:]mm:ss`).
//! - [`utmp`]: login record parsing.
//! - [`stable`]: the [`stable::ProcessTable`] trait with an empty and a
//!   slice implementation.

pub mod ptime;
pub mod signal;
pub mod stable;
pub mod utmp;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): unknown signal names, bad
/// durations, short records. A name with no process or session behind it
/// is 3 (`ESRCH`, the same number `kill` reports for a missing process).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcError {
    /// Malformed input.
    InvalidArgument,
    /// No such process, session, or record.
    NotFound,
}

impl ProcError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            ProcError::InvalidArgument => 22,
            ProcError::NotFound => 3,
        }
    }
}
