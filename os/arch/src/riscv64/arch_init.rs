//! RISC-V 64-bit architecture-specific initialization
//!
//! Implements `ArchInit` for RISC-V 64-bit, performing PMP configuration
//! and S-mode interrupt enablement.
//!
//! C: No Minix3 equivalent (Minix3 has no RISC-V port).

use crate::arch_init::ArchInit;

/// RISC-V 64-bit architecture-specific initialization.
///
/// Performs hardware-specific setup after protection structures and
/// interrupt controller are initialized:
///
/// 1. Configure PMP (Physical Memory Protection) to allow all access
/// 2. Enable S-mode interrupts (SIE register)
///
/// C: No Minix3 equivalent (Minix3 has no RISC-V port).
pub struct Riscv64ArchInit;

impl ArchInit for Riscv64ArchInit {
    fn init() {
        // 1. Configure PMP (Physical Memory Protection)
        // OpenSBI may have already configured PMP entries.
        // We add a default entry that allows all access.
        unsafe {
            // pmpaddr0 = 0xFFFFFFFFFFFFFFFF (match all addresses)
            core::arch::asm!("csrw pmpaddr0, {}", in(reg) u64::MAX);
            // pmpcfg0 = A=NAPOT (0x18) + X+R+W (0x7) = 0x1F
            // This allows all access to all memory regions.
            core::arch::asm!("csrw pmpcfg0, {}", in(reg) 0x1Fu64);
        }

        // 2. Enable S-mode external and timer interrupts
        // SIE: SEIE (bit 9) + STIE (bit 5) + SSIE (bit 1)
        unsafe {
            core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x22u64);
        }
    }
}
