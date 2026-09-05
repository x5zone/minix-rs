//! Process-information filesystem: static files plus per-process
//! directories, served on the virtual-tree framework.
//!
//! C correspondence: `minix3/minix/fs/procfs/` (eight sources, about one
//! thousand seven hundred lines): `main.c` (hooks table and startup),
//! `root.c` (static root files), `tree.c` (dynamic process directories
//! and hook dispatch), `pid.c` (per-process files), `service.c` (service
//! subdirectory), `cpuinfo.c` (processor file), `buf.c` (output staging),
//! `util.c` (load-average arithmetic).
//!
//! Like every file server here, ProcFS is a single-threaded event loop:
//! one message at a time, no shared mutable state across threads. Kernel
//! queries stay behind narrow input structures so every byte of output is
//! reproducible in tests.
//!
//! Module map: [`buf`] stages output with read-offset skipping, [`pid`]
//! reconciles process directories in two passes, [`content`] renders file
//! bytes from plain inputs.

#![no_std]

extern crate alloc;

pub mod buf;
pub mod content;
pub mod pid;

pub use minix_vtreefs as framework;

/// Inode budget (`NR_INODES`, `const.h:28`): four times the slot count,
/// covering static files, deleted-but-open files, and one full listing
/// pass of process directories. The comment in the source derives the
/// same bound from three independent worst cases.
pub const fn inode_budget(task_slots: usize, process_slots: usize) -> usize {
    (task_slots + process_slots) * 4
}

/// Staging buffer size (`BUF_SIZE`, `const.h:36`): four kilobytes plus
/// one byte of terminator margin (see [`buf`]).
pub const STAGING_SIZE: usize = 4097;

/// World-readable regular file (`REG_ALL_MODE`, `const.h:32`).
pub const MODE_STATIC_FILE: u16 = 0o100444;
/// World-accessible directory (`DIR_ALL_MODE`, `const.h:33`).
pub const MODE_STATIC_DIR: u16 = 0o040555;

/// Static root files (`root_files`, `root.c:22-38`, without the
/// x86-only entries, which the server layer adds per target).
pub const STATIC_FILES: [&str; 7] =
    ["hz", "uptime", "loadavg", "kinfo", "meminfo", "dmap", "mounts"];

/// Per-process files (`pid_files`, `pid.c:18-24`).
pub const PROCESS_FILES: [&str; 4] = ["psinfo", "cmdline", "environ", "map"];

/// Service initialization entry (kept for the server binary).
pub fn init() {}
