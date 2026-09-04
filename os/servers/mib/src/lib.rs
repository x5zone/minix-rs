#![cfg_attr(not(test), no_std)]

//! Minix-RS management information base (MIB): the sysctl(2) server.
//!
//! The user-space side of Minix3's sysctl tree: userland names a node,
//! MIB resolves it to data. This crate opens with the startup skeleton
//! documented in
//! `notes/rewrite/fork-syscall-rewrite/10-stage-mib/01-mib-init-main.md`:
//!
//! - [`dispatch`] — the three letters, the two refusals, the reply rule,
//!   and the sysctl decode verdicts (which path, what the reply carries).
//! - [`sef`] — the two init names and what each promises about state.
//! - [`tree`] — the node vocabulary and shape invariants (what a node is).
//! - [`io`] — copy and relay verdicts (what moves, who may touch it).
//! - [`auth`] — the cached superuser verdict and the permission gates.
//! - [`data`] — read and write verdicts for plain leaves.
//! - [`query`] — enumeration verdicts (versions, flag strip, sizes).
//! - [`describe`] — description verdicts (lengths, packing, set-guards).
//! - [`remote`] — endpoint table and relay verdicts (slots, replies).
//! - [`subtree`] — subsystem subtrees (kern first, vm/hw/minix follow).
//! - [`proc`] — process-table snapshots (pulls first, formats after).
//!
//! Everything else (handlers, client contracts) lands in 02~22 beside it.
//!
//! Single-threaded event loop: pure verdict functions, no shared state.

pub mod auth;
pub mod data;
pub mod describe;
pub mod dispatch;
pub mod io;
pub mod proc;
pub mod query;
pub mod remote;
pub mod sef;
pub mod subtree;
pub mod tree;

/// Published entry point kept for the binary shell (`main.rs`).
pub fn init() {}
