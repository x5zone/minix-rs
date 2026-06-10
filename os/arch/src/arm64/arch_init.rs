//! ARM64 (aarch64) architecture-specific initialization
//!
//! Implements `ArchInit` for ARM64, performing PMU cycle counter
//! enablement and board-specific initialization.
//!
//! C: arch_init() — earm/arch_system.c:101-132

use crate::arch_init::ArchInit;

/// ARM64 architecture-specific initialization.
///
/// Performs hardware-specific setup after protection structures and
/// interrupt controller are initialized:
///
/// 1. Enable PMU cycle counter for user mode access
/// 2. Board-specific initialization (bsp_init)
///
/// C: arch_init() — earm/arch_system.c:101-132
pub struct AArch64ArchInit;

impl ArchInit for AArch64ArchInit {
    fn init() {
        // C: arch_init() — earm/arch_system.c:101-132

        // 1. Enable PMU cycle counter for user mode access
        // C: PMU_PMCR_E + PMU_PMCNTENSET_C + PMU_PMUSERENR_EN
        unsafe {
            // Enable PMU counter hardware and reset counters
            // PMCR: E (enable) + C (reset event counters) + P (reset cycle counter)
            let pmcr: u64 = 0x7; // E + C + P bits
            core::arch::asm!("msr pmcr_el0, {}", in(reg) pmcr);

            // Enable cycle counter (PMCCNTR_EL0)
            // PMCNTENSET: C bit (bit 31) enables cycle counter
            core::arch::asm!("msr pmcntenset_el0, {}", in(reg) 0x8000_0000u64);

            // Allow EL0 (user mode) access to cycle counter
            // PMUSERENR: EN bit (bit 0)
            core::arch::asm!("msr pmuserenr_el0, {}", in(reg) 0x1u64);
        }

        // 2. Board-specific initialization
        // C: bsp_init()
        // TODO: platform-specific setup (e.g., GIC base address discovery)
    }
}
