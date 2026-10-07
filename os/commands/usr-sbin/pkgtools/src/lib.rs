#![cfg_attr(not(test), no_std)]

//! Package management and build helper core for Minix-RS commands.
//!
//! Covers `rewrite-notes/18-stage-commands/21-package-tools.md`:
//! package set installation (`minix3/minix/commands/pkgin_sets/pkgin_sets.sh`
//! with the core, development, and extras sets, `minix3/minix/commands/pkgin_cd/pkgin_cd.sh`
//! with the compact disc repository path `packages/{release}/{arch}/All` and the
//! summary file `pkg_summary.bz2`, `minix3/minix/commands/pkgin_all/pkgin_all.sh`),
//! boot block installation (`minix3/usr.sbin/installboot/installboot.c` with the
//! file system type dispatch near line 61, the 32 sector room requirement near
//! line 276, and the stage file selection near line 246), post installation
//! checks (`minix3/usr.sbin/postinstall/postinstall`), coverage data transfer
//! (`minix3/minix/commands/gcov-pull/gcov-pull.c` with the 4 megabyte buffer
//! `BUFF_SZ` and the single label argument usage), dependency generation
//! (`minix3/usr.bin/mkdep/mkdep.c` with the default output file `.depend`),
//! perfect hash generation (`minix3/usr.bin/nbperf/nbperf.c` with the hashing
//! methods `chm`, `chm3`, and `bdz` near lines 139 to 144), assembler symbol
//! extraction (`minix3/usr.bin/genassym/genassym.sh` with the `-c` and `-f`
//! mode flags), and the build configuration file (`minix3/etc/mk.conf`).
//!
//! # Design
//!
//! These tools prepare software; they never run it. What is pure here lives in
//! this crate, what touches disks, the network, or the compiler stays with the
//! execution layer:
//!
//! - [`sets`]: package set vocabulary (set selection, package name checks,
//!   and the package database trait).
//! - [`cdrepo`]: compact disc repository location (release plus architecture
//!   path building, summary file lookup order).
//! - [`bootinst`]: boot block installation vocabulary (file system type
//!   parsing, stage file choice, room check).
//! - [`depfile`]: dependency file vocabulary (default file name, `target:`
//!   line parsing, backslash continuation joining).
//! - [`symgen`]: symbol extraction vocabulary (mode flags, `name value`
//!   definition lines, and the symbol table trait).
//! - [`mkconf`]: build configuration vocabulary (`name?= value` assignments).
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout. Disk writes, network transfers, and compiler calls
//! stay with the execution layer behind [`sets::PackageDb`] and
//! [`symgen::SymbolTable`].

pub mod bootinst;
pub mod cdrepo;
pub mod depfile;
pub mod mkconf;
pub mod sets;
pub mod symgen;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): unknown sets, bad package names, bad
/// dependency lines. 2 marks a missing entry (`ENOENT`): a set, symbol, or
/// file with no record behind it. 28 marks a full volume (`ENOSPC`, the same
/// number boot block installation reports when the 32 reserved sectors do not
/// fit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgError {
    /// Malformed input.
    InvalidArgument,
    /// No such entry.
    NotFound,
    /// Volume full.
    NoSpace,
}

impl PkgError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            PkgError::InvalidArgument => 22,
            PkgError::NotFound => 2,
            PkgError::NoSpace => 28,
        }
    }
}
