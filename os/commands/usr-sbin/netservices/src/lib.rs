#![cfg_attr(not(test), no_std)]

//! Network services and daemons core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/19-network-services.md`:
//! the superserver (`minix3/usr.sbin/inetd/inetd.c`, at most `OPEN_MAX 64`
//! served sockets near line 276, at most `MAXARGV 20` server arguments near
//! line 306, built-in echo, discard, daytime, and character generation
//! handlers near lines 339 to 348), the system logger
//! (`minix3/usr.sbin/syslogd/syslogd.c`, action kinds `F_FILE` through
//! `F_FIFO` near lines 130 to 137, facility mask `LOG_FACMASK` and priority
//! mask `LOG_PRIMASK` near line 1488, default user priority `DEFUPRI` and
//! default kernel priority `DEFSPRI` near lines 60 to 61), file fetching
//! (`minix3/minix/commands/fetch/fetch.c`, usage with `-o` output file and
//! `-T` timeout near line 859), file transfer and terminal daemons
//! (`minix3/libexec/ftpd/` with the command table in `cmds.c`, terminal
//! handling in `minix3/libexec/telnetd/`), and mail and print queues
//! (`minix3/minix/commands/mail`, `minix3/minix/commands/lp`,
//! `minix3/minix/commands/lpd`).
//!
//! # Design
//!
//! Daemons wait, clients ask, the logger sorts. What is pure here lives in
//! this crate, what listens on sockets, forks, drops privileges, or writes
//! log files stays with the execution layer:
//!
//! - [`inetd`]: service table rows (name, socket kind, protocol, wait mode,
//!   user, server path, arguments), built-in services (echo, discard,
//!   daytime, character generation), and the service table trait.
//! - [`syslog`]: priority decoding (facility plus severity, default user and
//!   kernel priorities), selector parsing (`facility.priority`), action kinds
//!   (file, terminal, console, remote forward, user list, wall broadcast,
//!   pipe, first-in-first-out queue), and the log sink trait.
//! - [`fetch`]: uniform resource locator parsing (scheme, host, port, path),
//!   scheme selection (hypertext versus file transfer), and output selection.
//! - [`session`]: daemon session setup shared by file transfer, terminal, and
//!   remote shell daemons (banner, authentication outcome, change-root
//!   directory, login accounting).
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout. Socket listening, process creation, privilege changes,
//! and file writes stay with the execution layer behind
//! [`inetd::ServiceTable`] and [`syslog::LogSink`].

pub mod fetch;
pub mod inetd;
pub mod session;
pub mod syslog;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): unknown services, bad selectors, bad
/// locators. 2 marks a missing entry (`ENOENT`): a service or user with no row
/// behind it. 13 marks a denied session (`EACCES`, the same number the file
/// transfer daemon reports for a denied login).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceError {
    /// Malformed input.
    InvalidArgument,
    /// No such service, user, or entry.
    NotFound,
    /// Session denied.
    Denied,
}

impl ServiceError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            ServiceError::InvalidArgument => 22,
            ServiceError::NotFound => 2,
            ServiceError::Denied => 13,
        }
    }
}
