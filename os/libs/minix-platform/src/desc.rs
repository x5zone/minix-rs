//! Platform descriptor trait and sub-trait re-exports.
//!
//! The canonical definitions live in [`minix_boot::platform`]. This module
//! re-exports them so that downstream code referencing
//! `minix_platform::PlatformDesc` / `minix_platform::InterruptControllerDesc`
//! / etc. continues to work without changes.
//!
//! # Design (TODO-01-2 fix, 2026-07-16)
//!
//! Previously this file defined `InterruptControllerDesc`, `TimerDesc`, and
//! `ConsoleDesc` as enums with brand-name variants (`Apic` / `Gicv3` / `Plic`,
//! `Pit` / `ArmGenericTimer` / `Clint`, `IsaSerial` / `MmioSerial` /
//! `SbiConsole`). Those variants leaked hardware brand names into the
//! `KernelInfo` public API surface.
//!
//! The fix replaces the enums with traits (defined in `minix-boot::platform`).
//! Brand-name structs (`ApicDesc`, `Gicv3Desc`, `PlicDesc`, ...) now live in
//! [`crate::arch`] submodules and implement the traits. Upper layers see only
//! `&dyn InterruptControllerDesc` / `&dyn TimerDesc` / `&dyn ConsoleDesc`;
//! arch-layer consumers downcast via `Any` to access architecture-specific
//! fields.

// Re-export the trait abstractions and supporting types from `minix-boot`.
// These are the canonical definitions; this re-export preserves backward
// compatibility for code that writes `minix_platform::PlatformDesc` etc.
pub use minix_boot::platform::{
    ArchMiscDesc, ConsoleDesc, CpuInfo, CpuTopology, InterruptControllerDesc, PlatformDesc,
    PlatformSource, TimerDesc, MAX_CPUS,
};
