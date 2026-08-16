//! ARM64 (aarch64) `TimerIrqGate` implementation: CNTP_CTL_EL0.
//!
//! Programs the ARMv8-A Generic Timer for timer IRQ delivery:
//! - `enable_timer_irq`: sets CNTP_CTL_EL0.Enable (bit 0) + clears
//!   CNTP_CTL_EL0.IMASK (bit 1), then `isb`
//! - `disable_timer_irq`: clears CNTP_CTL_EL0.Enable + sets IMASK, then `isb`
//!
//! C: Minix3's 32-bit ARM port routes the timer through the BSP
//! (`earm/arch_clock.c:182` → `bsp_register_timer_handler`, `omap_timer.c:136`;
//! `bsp_timer_stop` at `omap_timer.c:331`). The aarch64 rewrite targets the
//! ARMv8-A Generic Timer (CNTP_CTL_EL0) instead — architectural evolution
//! ([ARCH: K-1], see 05-clock-interrupt-init.md §2.5 and §3.8).
//!
//! # GIC delivery path (why no ICC_IGRPEN1_EL1 here)
//!
//! A timer PPI only reaches the CPU if the GIC delivery path is up:
//! distributor global enable (GICD_CTLR.EnableGrp1NS), redistributor wake
//! (GICR_WAKER), and CPU-interface enable (ICC_SRE_EL1 / ICC_PMR_EL1 /
//! ICC_IGRPEN1_EL1). That responsibility belongs to
//! `AArch64InterruptController::init` (os/plat/src/arm64/interrupt.rs),
//! invoked in Phase B of `init_clock_and_interrupts` — NOT to the timer
//! IRQ gate. `TimerIrqGate` only controls the per-CPU timer module
//! (CNTP_CTL_EL0). See 05-clock-interrupt-init.md §4.7.1 "aarch64 真硬件路径".

use crate::arch::timer_irq_gate::TimerIrqGate;

/// ARM64 timer IRQ gate: CNTP_CTL_EL0 enable/mask.
pub struct AArch64TimerIrqGate;

impl TimerIrqGate for AArch64TimerIrqGate {
    fn enable_timer_irq() {
        // CNTP_CTL_EL0: bit 0 = Enable, bit 1 = IMASK (0 = unmasked).
        // Set Enable=1, IMASK=0 to allow timer IRQ delivery; `isb` makes
        // the system-register write visible to subsequent exception entry.
        // SAFETY: writing CNTP_CTL_EL0 is a side-effecting system-register
        // write available at EL1+; `isb` orders it before any trap entry.
        unsafe {
            core::arch::asm!(
                "msr CNTP_CTL_EL0, {ctrl}",
                "isb",
                ctrl = in(reg) 1u64, // Enable=1, IMASK=0
                options(nostack, preserves_flags),
            );
        }
    }

    fn disable_timer_irq() {
        // CNTP_CTL_EL0: Enable=0, IMASK=1 masks the timer IRQ.
        // SAFETY: same as `enable_timer_irq`.
        unsafe {
            core::arch::asm!(
                "msr CNTP_CTL_EL0, {ctrl}",
                "isb",
                ctrl = in(reg) 2u64, // Enable=0, IMASK=1
                options(nostack, preserves_flags),
            );
        }
    }
}
