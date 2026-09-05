#![cfg_attr(not(test), no_std)]

//! Login chain and password database core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/03-login-passwd.md`:
//! the terminal login chain (`getty` in `minix3/libexec/getty/main.c`,
//! `login` in `minix3/usr.bin/login/login.c`), the terminal line table
//! (`minix3/etc/ttys`), the terminal capability table
//! (`minix3/etc/gettytab`), and the password database face
//! (`minix3/etc/master.passwd`, `pwd_mkdb`, `vipw`, `chpass`).
//!
//! # Design
//!
//! As with the sibling scheduler crate, only pure decision logic lives here:
//! line formats, table lookups, and the shape of a login attempt. Password
//! hash verification and session setup (setting user identifiers, opening
//! the terminal, starting the shell) stay outside, behind the caller,
//! because they need operating system services this crate must not assume.
//! The split follows the Redox convention of keeping user programs' parsing
//! in plain testable libraries.
//!
//! All parsers borrow from the input text (zero copy) and use no heap, so
//! the crate compiles under `no_std`.
//!
//! # Modules
//!
//! - [`passwd`]: password file line format.
//! - [`ttys`]: terminal line table format.
//! - [`gettytab`]: terminal capability table format.
//! - [`userdb`]: the [`userdb::UserDatabase`] trait with one implementation
//!   per deployment stage.

pub mod gettytab;
pub mod passwd;
pub mod ttys;
pub mod userdb;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`), 2 marks an unknown name (`ENOENT`),
/// 13 marks a refused login (`EACCES`). Reusing these numbers keeps the
/// future program layer's exit statuses identical to the ones a NetBSD
/// style system reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginError {
    /// Malformed input: bad line format, bad number, empty name.
    InvalidArgument,
    /// A user, terminal, or table entry that does not exist.
    NotFound,
    /// A login attempt the policy refuses.
    PermissionDenied,
}

impl LoginError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            LoginError::InvalidArgument => 22,
            LoginError::NotFound => 2,
            LoginError::PermissionDenied => 13,
        }
    }
}
