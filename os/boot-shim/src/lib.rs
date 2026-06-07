//! Boot shim — bridges firmware (UEFI/OpenSBI/...) to the bare-metal kernel.
//!
//! This crate implements the `BootShim` trait (defined in `minix-types`)
//! for different firmware types. The kernel only depends on the trait,
//! not on any concrete implementation.
//!
//! Feature gates in `Cargo.toml` select which implementation gets compiled;
//! code uses the trait uniformly without `#[cfg]` conditionals.
//!
//! Available implementations:
//! - `uefi` feature (default): `UefiBootShim` — uses `uefi` crate
//! - `opensbi` feature: `OpenSbiBootShim` — hardcoded QEMU virt memory map

#![no_std]

#[cfg(feature = "uefi")]
extern crate alloc;

// Re-export the trait and result type from minix-types.
pub use minix_types::{BootShim, BootPrepareResult};

// ── Firmware-specific modules ──
// Each module provides a struct that implements BootShim.

#[cfg(feature = "uefi")]
pub mod uefi_helpers;

#[cfg(feature = "opensbi")]
pub mod opensbi_helpers;

// ── Convenience re-export of the active implementation ──
// This allows callers to write `boot_shim::prepare_boot(...)` without
// knowing which firmware is active. The Cargo.toml feature selection
// determines which impl is compiled.

#[cfg(feature = "uefi")]
pub use uefi_helpers::UefiBootShim;

#[cfg(all(feature = "opensbi", not(feature = "uefi")))]
pub use opensbi_helpers::OpenSbiBootShim;
