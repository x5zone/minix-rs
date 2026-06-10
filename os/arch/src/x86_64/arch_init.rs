//! x86-64 architecture-specific initialization
//!
//! Implements `ArchInit` for x86-64, performing TSS setup, serial port
//! initialization, ACPI table parsing, and APIC initialization.
//!
//! C: arch_init() — arch/i386/arch_system.c:246-288

use crate::arch_init::ArchInit;

/// x86-64 architecture-specific initialization.
///
/// Performs hardware-specific setup after protection structures and
/// interrupt controller are initialized:
///
/// 1. Per-CPU kernel stack allocation (handled by linker script)
/// 2. Serial port (COM1) initialization for early debug output
/// 3. ACPI table parsing for hardware topology discovery
/// 4. APIC initialization (LAPIC + IOAPIC) if available
///
/// C: arch_init() — arch_system.c:246-288
pub struct X86_64ArchInit;

impl ArchInit for X86_64ArchInit {
    fn init() {
        // C: arch_init() — arch_system.c:246-288

        // 1. Per-CPU kernel stacks
        // C: k_stacks = &k_stacks_start
        // Already handled by linker script in Rust version

        // 2. Serial port initialization (COM1 at 0x3F8)
        // C: ser_init()
        // TODO: COM1 initialization for early debug output

        // 3. ACPI table parsing
        // C: acpi_init()
        // TODO: RSDP search + table parsing

        // 4. APIC initialization
        // C: apic_single_cpu_init()
        // TODO: LAPIC + IOAPIC MMIO initialization
    }
}
