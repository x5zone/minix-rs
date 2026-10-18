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
// SD-19 静默吞错家族机械化门（本 crate 接手）：句柄关闭等 `Result` 返回被
// `let _ =` 掉空时，这个 restriction lint 让它在 `cargo clippy` 当场现形。
// 本 crate 已定性的站点按同族惯例写 `//[SD19-gate] <理由>` + 语句级
// `#[allow]`；今后新写一处无理由的 `let _ =` 会多报一条警告。
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
