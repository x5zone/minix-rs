//! Shared-folder filesystem framework: host-backed naming, handles,
//! and verification (`libsffs`, fifteen sources).
//!
//! A shared folder exposes host files inside the guest: the host owns
//! the bytes, the guest only names them. The framework therefore keeps
//! a guest-side tree of names plus host handles, and re-checks every
//! cached answer against the host before trusting it (the host may
//! rename behind our back at any moment). Host round-trips stay behind
//! a narrow table of operations the guest fills per hypervisor, so the
//! same framework serves both guests from one codebase.
//!
//! Module map: [`params`] owns mount options and permission masks,
//! [`path`] builds host paths from guest nodes, [`name`] compares
//! names with optional case folding, [`verify`] decides staleness,
//! [`handles`] opens lazily and closes silently.

#![no_std]

extern crate alloc;

pub mod handles;
pub mod name;
pub mod params;
pub mod path;
pub mod verify;
