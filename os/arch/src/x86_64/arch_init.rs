//! x86-64 architecture-specific initialization
//!
//! Implements `ArchInit` for x86-64, performing ACPI table parsing and
//! other architecture-specific setup that is not covered by functional
//! traits such as `InterruptController` or `EarlyConsole`.
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! `new(desc)` stores the ACPI tables physical address (if provided) from
//! `ArchMiscDesc` into an instance field. This replaces the previous
//! no-parameter static `init()`.
//!
//! C: arch_init() — arch/i386/arch_system.c:246-288

use minix_platform::ArchMiscDesc;

use crate::arch_init::ArchInit;

/// x86-64 architecture-specific initialization.
///
/// Performs hardware-specific setup after protection structures and
/// interrupt controller are initialized:
///
/// 1. Per-CPU kernel stack allocation (handled by linker script)
/// 2. ACPI table parsing for hardware topology discovery
///
/// Note: serial port (COM1) initialization is handled by
/// `X86_64EarlyConsole::init()` in `minix-plat`; APIC initialization
/// is handled by `X86_64InterruptController::init()`.
///
/// C: arch_init() — arch_system.c:246-288
pub struct X86_64ArchInit {
    /// ACPI tables physical address, if provided by the platform descriptor.
    /// `None` means no ACPI tables were discovered (QEMU virt fallback).
    #[allow(dead_code)]
    acpi_tables: Option<usize>,
}

impl ArchInit for X86_64ArchInit {
    fn new(desc: &ArchMiscDesc) -> Self {
        Self {
            acpi_tables: desc.acpi_tables,
        }
    }

    fn init(&mut self) {
        // C: arch_init() — arch_system.c:246-288

        // 1. Per-CPU kernel stacks
        // C: k_stacks = &k_stacks_start
        // Already handled by linker script in Rust version

        // 2. ACPI table parsing
        // C: acpi_init()
        // QEMU virt boot does not depend on ACPI; physical machines need
        // RSDP search + table parsing. When `acpi_tables` is `Some`,
        // Phase 4 will parse the tables starting from that address.

        // APIC initialization (C: apic_single_cpu_init()) is now handled by
        // X86_64InterruptController::init() (called from
        // init_clock_and_interrupts before arch_init).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // C: arch_init() takes the ACPI tables pointer from the platform
    // descriptor (arch_system.c:246-288, acpi_init path).
    #[test]
    fn new_stores_acpi_tables_address() {
        let desc = ArchMiscDesc {
            acpi_tables: Some(0x1_0000),
            ..Default::default()
        };
        let init = X86_64ArchInit::new(&desc);
        assert_eq!(init.acpi_tables, Some(0x1_0000));
    }

    #[test]
    fn new_defaults_to_no_acpi_tables() {
        // QEMU virt fallback: no ACPI tables discovered.
        let desc = ArchMiscDesc::default();
        let init = X86_64ArchInit::new(&desc);
        assert_eq!(init.acpi_tables, None);
    }

    #[test]
    fn init_is_repeatable_noop() {
        let desc = ArchMiscDesc {
            acpi_tables: Some(0x2_0000),
            ..Default::default()
        };
        let mut init = X86_64ArchInit::new(&desc);
        init.init();
        init.init();
        assert_eq!(init.acpi_tables, Some(0x2_0000));
    }
}
