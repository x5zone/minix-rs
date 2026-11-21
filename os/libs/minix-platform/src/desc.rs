//! Platform descriptor trait and sub-trait re-exports.
//!
//! The canonical definitions live in [`minix_boot::platform`]. This module
//! re-exports them so that downstream code referencing
//! `minix_platform::PlatformDesc` / `minix_platform::InterruptControllerDesc`
//! / etc. continues to work without changes.
//!
//! # Design
//!
//! This file defines `InterruptControllerDesc` and `TimerDesc` as trait
//! objects; architecture-specific fields are reached through `Any`. Brand-name
//! variants (`Apic` / `Gicv3` / `Plic`, `Pit` / `ArmGenericTimer` / `Clint`)
//! `SbiConsole`). Those variants leaked hardware brand names into the
//! `KernelInfo` public API surface.
//!
//! The fix replaces the enums with traits (defined in `minix-boot::platform`).
//! Brand-name structs (`ApicDesc`, `Gicv3Desc`, `PlicDesc`, ...) now live in
//! [`crate::arch`] submodules and implement the traits. Upper layers see only
//! are not enum variants. The `ConsoleDesc` trait has no live consumer and is
//! absent; the plat layer's early console owns its own constants.

// Re-export the trait abstractions and supporting types from `minix-boot`.
// These are the canonical definitions; this re-export preserves backward
// compatibility for code that writes `minix_platform::PlatformDesc` etc.
pub use minix_boot::platform::{
    ArchMiscDesc, CpuInfo, CpuTopology, InterruptControllerDesc, MAX_CPUS,
    PlatformDesc, PlatformSource, PsciConduit, TimerDesc,
};
