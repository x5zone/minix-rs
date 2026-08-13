//! QEMU `virt` fallback descriptor — arch-dispatched re-export.
//!
//! The actual `QemuVirtDesc` struct and its `PlatformDesc` impl live in the
//! per-arch submodules ([`crate::arch::x86_64`], [`crate::arch::aarch64`],
//! [`crate::arch::riscv64`]). Each arch defines its own `QemuVirtDesc` with
//! concrete sub-descriptor fields (`ApicDesc` / `Gicv3Desc` / `PlicDesc`,
//! etc.) — no `#[cfg(target_arch)]` inside method bodies.
//!
//! # Design (TODO-01-2 fix, 2026-07-16)
//!
//! Previously `QemuVirtDesc` was a unit struct with `#[cfg(target_arch)]`
//! scattered across 4 method bodies. The fix moves each arch's implementation
//! into a dedicated file with file-level `#[cfg]`, satisfying the
//! "no `#[cfg(target_arch)]` for behavior selection in upper layers" rule
//! (see 04-platform-discovery.md §4.2.2).

#[cfg(target_arch = "x86_64")]
pub use crate::arch::x86_64::QemuVirtDesc;
#[cfg(target_arch = "aarch64")]
pub use crate::arch::aarch64::QemuVirtDesc;
#[cfg(target_arch = "riscv64")]
pub use crate::arch::riscv64::QemuVirtDesc;
