//! Trap entry architecture abstraction
//!
//! Defines the trait interface for trap entry configuration:
//! - Interrupt/exception/system call entry point setup
//! - IDT / exception vector table / trap vector management
//! - SYSCALL/SVC/ecall entry mechanism configuration
//!
//! # Design decisions (see 04-protection.md §3)
//!
//! - **Two-trait split** (§3.1): `TrapEntryArch` is separate from
//!   `ProtectionArch` because "how to enter the kernel" (entry mechanism)
//!   is a different concern from "who can access what" (access control).
//! - **SYSCALL in TrapEntryArch** (§3.1): Although SYSCALL MSR configuration
//!   is x86-64 specific, its semantics ("configure system call entry point")
//!   belongs to the entry mechanism concern. ARM64/RISC-V implement
//!   `configure_syscall()` as no-op since SVC/ecall use the same exception
//!   vector as other traps.
//! - **64-bit only** (§3.4): SYSENTER is not supported in 64-bit mode.
//!   Only SYSCALL/SYSRET is used on x86-64.

use minix_types::VirBytes;
use crate::protection::InterruptVector;

/// Architecture abstraction for trap entry configuration.
///
/// Manages how the CPU enters the kernel in response to interrupts,
/// exceptions, and system calls. On x86-64, this includes the IDT
/// (Interrupt Descriptor Table) and SYSCALL MSR configuration.
/// On ARM64/RISC-V, the exception/trap vector serves all three purposes.
///
/// # Initialization order
///
/// `ProtectionArch::load()` must be called before `TrapEntryArch::init()`,
/// because trap entry descriptors (e.g., IDT gate descriptors on x86-64)
/// reference segment selectors that must be valid in the GDT.
///
/// C: idt_init() — protect.c:260
pub trait TrapEntryArch: Sized {
    /// Initialize the trap entry table with architecture-specific handlers.
    ///
    /// Fills the table with handler addresses for CPU exceptions,
    /// hardware interrupts, and system call vectors.
    ///
    /// C: idt_init() — protect.c:260
    ///    (fills IDT with gate_table_exceptions[] and gate_table_pic[])
    fn init() -> Self;

    /// Configure the system call entry mechanism.
    ///
    /// x86-64: Enable SYSCALL/SYSRET via MSR — sets STAR, LSTAR, SFMASK,
    ///         and enables EFER.SCE. Called after `init()` and before `load()`.
    /// ARM64:  No-op — SVC instruction uses the exception vector set by `init()`.
    /// RISC-V: No-op — ecall instruction uses the trap vector set by `init()`.
    ///
    /// C: tss_init() lines 189-205 — SYSCALL MSR setup
    fn configure_syscall(&mut self, entry_point: VirBytes);

    /// Load the trap entry table into hardware.
    ///
    /// After this call, the CPU will route interrupts, exceptions, and
    /// system calls through the configured entry points.
    ///
    /// C: idt_reload() — protect.c:268
    ///    (x86_lidt(&idt_desc))
    fn load(&self);

    /// Load the trap entry table on an AP.
    ///
    /// On x86-64, the IDT is shared across CPUs, so this just reloads
    /// the IDTR. On ARM64/RISC-V, each CPU has its own VBAR_EL1/stvec.
    fn load_ap(&self);

    /// Register a handler for a specific interrupt vector.
    ///
    /// Used for dynamic handler registration (e.g., device drivers
    /// registering IRQ handlers). The DPL (Descriptor Privilege Level)
    /// controls whether the vector can be triggered from user mode.
    ///
    /// C: gate_table_pic[] / gate_table_exceptions[] — protect.c:260
    fn set_handler(
        &mut self,
        vector: InterruptVector,
        handler: VirBytes,
        user_accessible: bool,
    );
}
