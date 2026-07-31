//! Architecture-specific platform descriptor implementations.
//!
//! Brand-name structs (`ApicDesc`, `Gicv3Desc`, `PlicDesc`, `PitDesc`,
//! `ArmGenericTimerDesc`, `ClintDesc`, `IsaSerialDesc`, `MmioSerialDesc`,
//! `SbiConsoleDesc`) live in the per-arch submodules. Upper layers see only
//! the traits (`InterruptControllerDesc`, `TimerDesc`, `ConsoleDesc`) defined
//! in `minix-boot::platform` — brand names never leak across arch boundaries.
//!
//! # Open-closed principle
//!
//! Adding support for a new interrupt controller (e.g., x2APIC) requires only:
//! 1. Define `X2ApicDesc` in the relevant arch submodule.
//! 2. Implement `InterruptControllerDesc` for it.
//! 3. Use it in the parser (`AcpiDesc` / `DeviceTreeDesc`).
//!
//! No changes to `minix-boot`, upper-layer traits, or `PlatformDescEnum`.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "riscv64")]
pub mod riscv64;
