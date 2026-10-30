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
// Mechanical gate for the silent-error-swallow family, taken on by this crate:
// when a `Result` return such as a handle close is dropped with `let _ =`, this
// restriction lint makes it surface immediately in `cargo clippy`.
// Sites this crate has already classified follow the family convention:
// `//[SD19-gate] <reason>` plus a statement-level `#[allow]`; any future
// unjustified `let _ =` reports one extra warning.
#![warn(clippy::let_underscore_must_use)]

extern crate alloc;

pub mod handles;
pub mod name;
pub mod params;
pub mod path;
pub mod verify;
pub mod attr;
pub mod server;
pub mod table;
