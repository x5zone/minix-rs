#![cfg_attr(not(test), no_std)]

//! Process and session information core for Minix-RS commands.
//!
//! Covers `rewrite-notes/18-stage-commands/12-process-tools.md`:
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
//! borrows from the input and uses fixed size buffers except the [`env`]
//! assignment merge (which materialises the resulting environment): `no_std`
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
//! - [`dates`]: epoch to broken-down UTC time and the `strftime` subset
//!   behind `date` (reuses [`stamp`]'s calendar inverse).
//! - [`printenv`]: environment-variable lookup behind `printenv`.
//! - [`env`]: option scan, operand split and `putenv` merge behind `env`.
//! - [`sleep`]: operand parsing behind `sleep` (integer vs decimal path).
//! - [`from`]: the `From ` record scan and the BSD sender matcher behind
//!   `from`.

// The `env` assignment merge materialises the resulting environment as an
// owned list; every other module in this crate stays allocation free.
extern crate alloc;

pub mod dates;
pub mod env;
pub mod from;
pub mod printenv;
pub mod ptime;
pub mod signal;
pub mod sleep;
pub mod stable;
pub mod stamp;
pub mod users;
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
