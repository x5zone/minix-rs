//! Architecture-specific initialization trait
//!
//! Defines the trait interface for architecture-specific initialization
//! that must happen after protection structures and interrupt controller
//! are initialized.
//!
//! # Design decisions (see 04-clock-interrupt-init.md §3.4, §3.5)
//!
//! - **ArchInit trait** (§3.4): Replaces C's `arch_init()` function
//!   which was scattered with `#ifdef` conditionals. Each architecture
//!   implements its own initialization without conditional compilation.
//! - **No memory cutting** (§3.5): Unlike C's `arch_init()` which calls
//!   `cut_memmap()`, Rust version does not cut memory regions because
//!   `KernelInfo.memmap` already excludes reserved regions.

/// Architecture-specific initialization performed after interrupt controller init.
///
/// This trait encapsulates the `arch_init()` function from Minix3 C,
/// which performs hardware-specific setup that must happen after
/// protection structures and interrupt controller are initialized.
///
/// # Architecture mapping
///
/// | Method | x86-64 | ARM64 | RISC-V |
/// |--------|--------|-------|--------|
/// | `init()` | TSS + serial + ACPI + APIC | PMU cycle counter + bsp_init | PMP + SIE |
///
/// C: arch_init() — arch/i386/arch_system.c:246 / earm/arch_system.c:101
pub trait ArchInit {
    /// Perform architecture-specific initialization.
    ///
    /// Called once during `init_clock_and_interrupts()`, after `init_clock()` and
    /// `intr_init()` have completed.
    ///
    /// C: arch_init() — arch_system.c:246 (x86) / arch_system.c:101 (ARM)
    fn init();
}
