#![cfg_attr(not(test), no_std)]

//! Backup and maintenance core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/17-backup-maintenance.md`:
//! directory backup (`minix3/minix/commands/backup/backup.c`, flags documented
//! in the header comment, copy buffer `COPY_SIZE 4096`, at most `MAX_ENTRIES 512`
//! entries per directory, at most `MAX_PATH 256` characters per path),
//! remote and tree synchronization (`minix3/minix/commands/remsync/remsync.c`
//! with the `pathname` record near line 101, the `namelist` record near line 177,
//! the per-file `entry` record near line 285, and the usage function near line 1472;
//! `minix3/minix/commands/synctree/synctree.c` with `CHUNK 4096` transfer blocks
//! and the `entry` record near line 200), temporary directory cleaning
//! (`minix3/minix/commands/cleantmp/cleantmp.c` with `SEC_DAY` day length and
//! `DOTDAYS 14` extra retention for hidden names, midnight rounding in
//! `days2time`), progress display (`minix3/minix/commands/progressbar/progressbar.c`
//! with `WIDTH 77` bar width), log rotation (`minix3/minix/commands/rotate/rotate.sh`),
//! difference-list repair (`minix3/minix/commands/fix/fix.c` with `LINELEN 1024`
//! input lines), magnetic tape control (`minix3/minix/commands/mt/mt.c` with the
//! `tape_operation_t` table, `MTWEOF`/`MTFSF`/`MTFSR`/`MTBSF`/`MTBSR`/`MTEOM`/
//! `MTREW`/`MTOFFL`/`MTRETEN`/`MTERASE`/`MTSETDNSTY`/`MTSETBSIZ` operations,
//! `Usage: mt [-f device] command [count]`), and boot image refresh scripts
//! (`minix3/minix/commands/updateboot/updateboot.sh`,
//! `minix3/minix/commands/update_asr/update_asr.sh`).
//!
//! # Design
//!
//! These commands move bytes that already exist; they never invent content.
//! What is pure here lives in this crate, what touches disks, clocks, tapes,
//! or the screen stays with the execution layer:
//!
//! - [`backup`]: option parsing (only directories, skip junk, ask for another
//!   volume, only loose files, skip object files, restore direction, skip
//!   assembler files, keep creation date, verbose, compress), junk name
//!   filtering (`*.Z`, `*.bak`, `*.log`, `a.out`, `core`, `*.o`, `*.s`),
//!   and the copy decision (target missing, or source newer than target).
//! - [`cleantmp`]: retention arithmetic (day count to midnight timestamps,
//!   longer retention for hidden names), and the removal decision (forced
//!   cleaning removes everything, otherwise only entries older than the
//!   matching timestamp).
//! - [`progress`]: progress bar rendering (remaining file count plus a fixed
//!   width bar of `=` fill and `-` empty cells) and total count parsing.
//! - [`tape`]: magnetic tape command table (end-of-file marks, forward and
//!   backward spacing over files and records, end of media, rewind, offline,
//!   status query, retension, erase, density and block size selection) with
//!   per-command count rules, plus device status decoding.
//! - [`rotate`]: log rotation planning (oldest generation dropped, middle
//!   generations shifted up by one, current log compressed into generation one,
//!   current log truncated).
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout. File system walks, clock reads, tape input and output
//! control calls, and screen updates stay with the execution layer behind the
//! [`backup::MetaSource`] and [`tape::TapeBackend`] traits (each ships a memory
//! backend plus an empty backend so tests run without hardware).

pub mod backup;
pub mod cleantmp;
pub mod progress;
pub mod rotate;
pub mod tape;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): unknown flags, bad counts, unknown tape
/// commands. 2 marks a missing entry (`ENOENT`): a lookup with no record behind
/// it. 28 marks a full volume (`ENOSPC`, the same number `backup` reports when
/// the target volume runs out of space). 5 marks a device failure (`EIO`, the
/// same number tape status decoding reports for a failing drive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintError {
    /// Malformed input.
    InvalidArgument,
    /// No such entry.
    NotFound,
    /// Volume full.
    NoSpace,
    /// Device failure.
    DeviceError,
}

impl MaintError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            MaintError::InvalidArgument => 22,
            MaintError::NotFound => 2,
            MaintError::NoSpace => 28,
            MaintError::DeviceError => 5,
        }
    }
}
