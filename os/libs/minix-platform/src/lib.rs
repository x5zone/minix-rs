#![no_std]

//! Platform hardware discovery abstraction.
//!
//! Provides the `PlatformDesc` trait — a unified, architecture-agnostic
//! interface for querying hardware parameters (interrupt controller base
//! addresses, timer frequencies, CPU topology) that were previously
//! hardcoded per-architecture.
//!
//! # Design (see `plat-design.md`)
//!
//! - **Root trait + enum sub-descriptors**: `PlatformDesc` is a trait
//!   (supports `&dyn` and mock); sub-descriptors (`InterruptControllerDesc`,
//!   `TimerDesc`, ...) are enums (type-safe, pattern-matchable).
//! - **Three sources unified**: `DeviceTreeDesc` (ARM/RISC-V),
//!   `AcpiDesc` (x86), `QemuVirtDesc` (fallback) all implement the same
//!   trait. Upper layers (`ClockArch`/`InterruptController`) are source-agnostic.
//! - **Instance-based hardware traits**: `ClockArch::new(desc) -> Self`
//!   stores parsed addresses in instance fields — zero indirection on the
//!   hot path.
//! - **Global `PlatformContext`**: written once at T2.5 (single-threaded,
//!   BKL held), read-only thereafter. SMP-safe without locks.

pub mod acpi;
pub mod desc;
pub mod device_tree;
pub mod global;
pub mod qemu_virt;

pub use acpi::{AcpiDesc, AcpiParseError};
pub use desc::*;
pub use device_tree::{DeviceTreeDesc, DtParseError};
pub use global::{init, init_from_kinfo, platform_desc, PlatformContext, PlatformDescEnum};
pub use qemu_virt::QemuVirtDesc;
