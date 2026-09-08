//! Timer IRQ hardware gate.
//!
//! Architecture-specific enable/disable of timer IRQ delivery.
//!
//! See 05-clock-interrupt-init.md §3.7 (the deleted
//! `ArchBoot::register_timer_handler`) and §4.7.2 (why handler
//! registration is not part of this trait).
//!
//! # C source path
//!
//! The timer IRQ enable/disable is the hardware half of `boot_cpu_init_timer`
//! (clock.c:294) / `register_local_timer_handler` (clock.c:177):
//!
//! ```c
//! // clock.c:294 — boot_cpu_init_timer
//! int boot_cpu_init_timer(unsigned freq) {
//!     if (init_local_timer(freq))            // ClockArch::init_timer in Rust
//!         return -1;
//!     if (register_local_timer_handler(
//!             (irq_handler_t) timer_int_handler))
//!         return -1;
//!     return 0;
//! }
//! ```
//!
//! `register_local_timer_handler` itself is arch-specific:
//! - x86-64: `arch_clock.c:177` — with APIC the LAPIC timer is configured in
//!   `apic_idt_init()`; the LVT Timer mask bit (offset 0x320, bit 16) is
//!   `APIC_LVTT_MASK` (`apic.c:44`), set at `apic.c:475-477`.
//! - aarch64: `earm/arch_clock.c:182` — delegates to `bsp_register_timer_handler`
//!   (`bsp/ti/omap_timer.c:136`). The Rust rewrite targets ARMv8-A Generic
//!   Timer (CNTP_CTL_EL0) instead (Minix3 C is 32-bit ARM; architectural
//!   evolution, see 05-clock-interrupt-init.md §2.5).
//! - riscv64: no Minix3 port ([ARCH: K-2]); RISC-V Privileged Spec 1.12 §4.1.3
//!   (Supervisor Interrupt Registers, `sie.STIE` bit 5).
//!
//! # Three-architecture coverage (FIX-22, Phase 2 → TimerIrqGate; semantics
//! refined by D-59, 2026-09-09 `[ARCH: gate-semantics]`)
//!
//! `enable_timer_irq` / `disable_timer_irq` open and close the *module-local*
//! gate of the current boot clock source. The gate genuinely differs across
//! the three architectures, so they stay in a trait — this is the
//! architecture capability boundary:
//!
//! | Boot clock source | Module-local gate | `enable_timer_irq` |
//! |--------|--------|-------|
//! | x86-64: 8254 PIT | **none** — the delivery gate is the IOAPIC IRQ 0 line, unmasked by `IrqManager`'s first-handler rule (interrupt.c:65) | no-op (the LAPIC LVT Timer gate belongs to the *future* LAPIC timer source and must not be opened while the PIT is the source) |
//! | aarch64: CNTP | `CNTP_CTL_EL0` Enable/IMASK + `isb` | Enable=1, IMASK=0 + `isb` |
//! | riscv64: CLINT mtimecmp | `sie.STIE` (bit 5) | `csrs sie` STIE |
//!
//! `disable_timer_irq` mirrors each enable. The controller-side delivery
//! gate (IOAPIC redtbl / GICR_ISENABLER0 bit 30 / — none for the RISC-V
//! local timer) is *not* this trait's job: it lives behind
//! `IrqManager::register_hook`'s C-parity unmask, driven by
//! `minix_plat::TIMER_IRQ`.
//!
//! Timer handler *registration* is intentionally NOT part of this trait:
//! the old `ArchBoot::register_timer_handler` was a mock placeholder with no
//! readers (trap entry reads nothing from it; real dispatch goes through
//! `IrqManager::register_hook`). See 05-clock-interrupt-init.md §4.7.2.

/// Open/close the timer IRQ delivery gate: the paired hardware operations.
///
/// # Why associated functions (no `&self`)
///
/// `TimerIrqGate` is stateless at the trait level — the hardware parameter
/// (LAPIC MMIO base / system registers / sie CSR) is reached via per-arch
/// mechanisms, matching `TrapEntryArch` / `ProtectionArch` (static-method
/// traits). Hence the bound is only `Sized`.
pub trait TimerIrqGate: Sized {
    /// Unmask the timer IRQ so timer interrupts are delivered.
    ///
    /// # Invariant (call timing)
    ///
    /// Must be called after the boot clock source is programmed
    /// (`ClockArch::init_timer`, bsp_finish_booting Step 6) AND after the
    /// timer hook is registered (`IrqManager::register_hook` under
    /// `minix_plat::TIMER_IRQ`) — the C order inside
    /// `boot_cpu_init_timer` (clock.c:294: `init_local_timer` then
    /// `register_local_timer_handler`, whose `put_irq_handler` caller rule
    /// enables the line). Opening the gate before the handler exists
    /// leaves a live interrupt source with nothing to serve it (the
    /// pre-D-59 defect). See 05-clock-interrupt-init.md §3.7.
    fn enable_timer_irq();

    /// Mask the timer IRQ.
    ///
    /// Used during critical sections where timer delivery is undesirable.
    fn disable_timer_irq();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_timer_irq_gate_compiles() {
        // Compile-time check: CurrentTimerIrqGate selects one of the three
        // real per-arch ZST impls via the lib.rs cfg alias (no cfg leaks
        // into capability consumers).
        fn _check<T: TimerIrqGate>() {}
        _check::<crate::CurrentTimerIrqGate>();
    }

    // FIX-22 (Phase 2) heritage: three-architecture compile-time coverage.
    // Each architecture's TimerIrqGate impl must exist and satisfy the
    // trait bound. Real hardware programming (LAPIC/CNTP/sie) is verified
    // via QEMU integration tests, not unit tests.

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn test_x86_64_timer_irq_gate_compiles() {
        fn _check<T: TimerIrqGate>() {}
        _check::<crate::x86_64::timer_irq_gate::X86_64TimerIrqGate>();
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn test_aarch64_timer_irq_gate_compiles() {
        fn _check<T: TimerIrqGate>() {}
        _check::<crate::arm64::timer_irq_gate::AArch64TimerIrqGate>();
    }

    #[cfg(target_arch = "riscv64")]
    #[test]
    fn test_riscv64_timer_irq_gate_compiles() {
        fn _check<T: TimerIrqGate>() {}
        _check::<crate::riscv64::timer_irq_gate::Riscv64TimerIrqGate>();
    }
}
