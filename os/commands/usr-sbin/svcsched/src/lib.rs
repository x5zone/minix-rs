#![cfg_attr(not(test), no_std)]

//! Service management and scheduling core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/02-service-scheduler.md`:
//! the client side of service control (`service` shell script in
//! `minix3/usr.sbin/service/service`, the `svrctl` low level interface in
//! `minix3/minix/commands/svrctl/svrctl.c`) and the time based schedulers
//! (`cron` in `minix3/minix/commands/cron/`, one shot `at` in
//! `minix3/minix/commands/at/at.c`, the `update` sync daemon).
//!
//! # Design
//!
//! The crate keeps only pure decision logic: argument shapes, timetable
//! matching, and schedule data structures. Every effect (sending a request
//! to the reincarnation server, forking a job, writing the clock) stays
//! outside, behind the caller. This split mirrors how Redox organises its
//! user programs (parse and decide in a plain library, perform effects in a
//! thin program layer) and keeps the whole crate testable without an
//! operating system underneath.
//!
//! All parsers are zero copy: they borrow from the input text instead of
//! allocating, so the crate needs no heap and compiles under `no_std`.
//!
//! # Modules
//!
//! - [`service`]: the `service name action` command line model.
//! - [`cron`]: crontab line parsing and timetable matching.
//! - [`scheduler`]: the [`scheduler::ScheduleMatcher`] trait with one
//!   implementation per scheduler flavour.

pub mod cron;
pub mod scheduler;
pub mod service;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// The mapping reuses the well known `errno` values so that the future
/// program layer can exit with the same status a NetBSD style system would
/// report: 22 for a malformed argument (`EINVAL`), 2 for an unknown name
/// (`ENOENT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedError {
    /// Malformed input: bad field, empty command, unknown action.
    InvalidArgument,
    /// A name (service, user, table entry) that does not exist.
    NotFound,
}

impl SchedError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            SchedError::InvalidArgument => 22,
            SchedError::NotFound => 2,
        }
    }
}
