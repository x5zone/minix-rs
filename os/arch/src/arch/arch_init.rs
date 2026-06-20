//! Architecture-specific initialization trait
//!
//! Defines the trait interface for architecture-specific initialization
//! that must happen after protection structures and interrupt controller
//! are initialized.
//!
//! # Design decisions (see 04-clock-interrupt-init.md §3.5, §3.6)
//!
//! - **ArchInit trait** (§3.5): Replaces C's `arch_init()` function
//!   which was scattered with `#ifdef` conditionals. Each architecture
//!   implements its own initialization without conditional compilation.
//! - **No memory cutting** (§3.6): Unlike C's `arch_init()` which calls
//!   `cut_memmap()`, Rust version does not cut memory regions because
//!   `KernelInfo.memmap` already excludes reserved regions.
//! - **Instance-based design** (see `plat-design.md` §5.1): `new(desc)`
//!   stores architecture-misc parameters (ACPI tables pointer, PMU enable)
//!   from `ArchMiscDesc` into instance fields.

/// Architecture-specific initialization performed after interrupt controller init.
///
/// This trait encapsulates the `arch_init()` function from Minix3 C,
/// which performs hardware-specific setup that must happen after
/// protection structures and interrupt controller are initialized.
///
/// # Instance-based design
///
/// `ArchInit` is **instance-based**: `new(desc)` stores the
/// `ArchMiscDesc` (ACPI tables pointer, PMU enable flag) in instance
/// fields. Upper layers obtain the descriptor from
/// `minix_platform::platform_desc().arch_misc()`.
///
/// # Architecture mapping
///
/// | Method | x86-64 | ARM64 | RISC-V |
/// |--------|--------|-------|--------|
/// | `new()` | store ACPI tables ptr from `ArchMiscDesc` | store PMU flag | no-op (no misc) |
/// | `init()` | serial (COM1) + ACPI | PMU cycle counter + bsp_init | PMP + SIE |
///
/// Note: `ArchInit` is a *phase* trait, not a *functional* trait. It gathers
/// architecture-specific leftovers that do not have a cross-architecture
/// abstraction (like `ClockArch` or `InterruptController`). TSS and APIC
/// setup on x86-64 live in the protection and interrupt-controller modules,
/// respectively, not here.
///
/// C: arch_init() — arch/i386/arch_system.c:246 / earm/arch_system.c:101
pub trait ArchInit: Sized + Send + Sync {
    /// Create an instance from an architecture-misc descriptor.
    ///
    /// Stores the architecture-misc parameters (ACPI tables pointer, PMU
    /// enable flag) from the descriptor into instance fields. Called once
    /// during `init_clock_and_interrupts()` after `PlatformContext` is
    /// initialized.
    fn new(desc: &minix_platform::ArchMiscDesc) -> Self;

    /// Perform architecture-specific initialization.
    ///
    /// Called once during `init_clock_and_interrupts()`, after `init_clock()` and
    /// `intr_init()` have completed.
    ///
    /// C: arch_init() — arch_system.c:246 (x86) / arch_system.c:101 (ARM)
    fn init(&mut self);
}
