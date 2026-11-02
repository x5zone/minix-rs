//! x86-64 `TimerIrqGate` implementation: the boot clock source has no
//! module-local gate.
//!
//! # Semantics — `[ARCH: gate-semantics]` (settled 2026-09-09)
//!
//! `TimerIrqGate::enable_timer_irq` / `disable_timer_irq` open and close
//! the *module-local* gate of the current boot clock source. On x86-64 the
//! boot clock source is the 8254 PIT (`X86_64ClockArch::init_timer`
//! programs it), and the PIT has no module-local gate: its output enters
//! the 8259A at IRQ0, whose INT is routed to IOAPIC input 2; that input's
//! mask bit *is* the delivery gate.
//! That gate is opened by the C-parity "unmask on first handler" rule in
//! `IrqManager::register_hook` (interrupt.c:65) when the timer hook is
//! installed at `bsp_finish_booting` Step 6 — so both gate methods are
//! intentionally no-ops here.
//!
//! C: `register_local_timer_handler` (arch_clock.c:177-196) — the PIC
//! path installs the hook on `CLOCK_IRQ` (IRQ 0) and `put_irq_handler`'s
//! caller rule enables the line; there is no separate timer-module
//! register to program. `[ARCH: gate-semantics]` is recorded in
//! 05-clock-interrupt-init.md §3.7.
//!
//! # Why clearing the LAPIC LVT Timer mask here was a trap
//!
//! An earlier revision cleared the LAPIC LVT Timer Mask (offset 0x320,
//! bit 16) and set the SVR enable. That gate belongs to the *LAPIC
//! timer*, which is not the boot clock source (and is not programmed):
//! unmasking it would have let an unconfigured timer deliver interrupts
//! with no handler. The LVT gate comes back with the LAPIC timer clock
//! source itself (see 05-clock-interrupt-init.md §3.7; the mask-write
//! idiom survives in `X86_64ClockArch::stop_local_timer`).
//!
//! # Call timing invariant
//!
//! Still must be called after `InterruptController::init` if a future
//! implementation touches the LAPIC again; today's no-op has no hardware
//! precondition.

use crate::arch::timer_irq_gate::TimerIrqGate;

/// x86-64 timer IRQ gate: no module-local gate for the PIT clock source.
pub struct X86_64TimerIrqGate;

impl TimerIrqGate for X86_64TimerIrqGate {
    fn enable_timer_irq() {
        // The boot clock source (8254 PIT) has no module-local gate; its
        // delivery gate is IOAPIC input 2 (PIT→8259A IRQ0→8259A INT→pin 2),
        // unmasked by `IrqManager::register_hook`'s first-handler rule. See
        // the module documentation for why this is deliberately not the
        // LAPIC LVT Timer mask.
    }

    fn disable_timer_irq() {
        // Mirror of `enable_timer_irq`: nothing to close on the PIT path.
        // (Masking an unprogrammed LVT timer here would silently break the
        // future LAPIC-timer source's assumptions instead of helping.)
    }
}
