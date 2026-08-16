//! RISC-V 64-bit `TimerIrqGate` implementation: sie.STIE.
//!
//! Programs the S-mode timer interrupt enable:
//! - `enable_timer_irq`: sets sie.STIE (S-mode Timer Interrupt Enable, bit 5)
//! - `disable_timer_irq`: clears sie.STIE
//!
//! The actual timer firing is controlled by mtimecmp, written directly to
//! the CLINT MMIO by `ClockArch::init_timer` (S-mode writes the comparator
//! without an SBI ecall; M-mode firmware such as OpenSBI has already
//! mapped the MMIO). `TimerIrqGate` only controls the supervisor interrupt
//! enable bit.
//!
//! C: Minix3 has no riscv64 port ([ARCH: K-2]; architectural evolution,
//! 05-clock-interrupt-init.md §3.8); the reference is the RISC-V Privileged
//! Spec 1.12 §4.1.3 (Supervisor Interrupt Registers, sie.STIE).

use crate::arch::timer_irq_gate::TimerIrqGate;

/// RISC-V timer IRQ gate: sie.STIE enable/disable.
pub struct Riscv64TimerIrqGate;

impl TimerIrqGate for Riscv64TimerIrqGate {
    fn enable_timer_irq() {
        // Set STIE (S-mode Timer Interrupt Enable) = bit 5 in the sie CSR.
        // SAFETY: `csrs sie` is a side-effecting CSR write available in
        // S-mode; STIE is bit 5 per the Privileged Spec.
        unsafe {
            core::arch::asm!(
                "csrs sie, {bits}",
                bits = in(reg) 0x20u64, // STIE = bit 5
                options(nostack, preserves_flags),
            );
        }
    }

    fn disable_timer_irq() {
        // Clear STIE (S-mode Timer Interrupt Enable) = bit 5 in sie.
        // SAFETY: same as `enable_timer_irq`.
        unsafe {
            core::arch::asm!(
                "csrc sie, {bits}",
                bits = in(reg) 0x20u64, // STIE = bit 5
                options(nostack, preserves_flags),
            );
        }
    }
}
