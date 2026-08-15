//! x86-64 `TimerIrqGate` implementation: LAPIC LVT Timer + SVR.
//!
//! Programs the Local APIC for timer IRQ delivery:
//! - `enable_timer_irq`: clears LAPIC LVT Timer Mask bit (LVT offset 0x320,
//!   bit 16) and ensures the LAPIC SVR Enable bit (offset 0xF0, bit 8) is set
//! - `disable_timer_irq`: sets LAPIC LVT Timer Mask bit
//!
//! C: `arch_clock.c:177` (register_local_timer_handler, APIC path) +
//! `apic.c` — `APIC_LVTT_MASK` defined at apic.c:44, LVT Timer mask set at
//! apic.c:475-477; SVR enable corresponds to `lapic_enable()` (apic.c:674-700).
//!
//! # Call timing invariant
//!
//! `enable_timer_irq` must be called after `X86_64InterruptController::init`
//! (Phase B of `init_clock_and_interrupts`), which sets the IA32_APIC_BASE
//! global enable bit (see os/plat/src/x86_64/interrupt.rs `init_lapic`).
//! If the LAPIC is not yet globally enabled, this is a boot-order violation
//! and we panic instead of silently recording mock state (see
//! 05-clock-interrupt-init.md §3.7 "调用时序约束").

use crate::arch::timer_irq_gate::TimerIrqGate;

/// x86-64 timer IRQ gate: LAPIC LVT Timer mask/unmask (+ SVR enable).
pub struct X86_64TimerIrqGate;

/// Read the LAPIC MMIO base address from the IA32_APIC_BASE MSR.
///
/// Returns `Some` only when the APIC global enable bit (bit 11) is set.
///
/// C: `apic.c:lapic_base()` — reads the LAPIC base from IA32_APIC_BASE
fn lapic_base_x86_64() -> Option<*mut u32> {
    let lo: u32;
    let hi: u32;
    // SAFETY: rdmsr with a fixed MSR index (0x1B, IA32_APIC_BASE) is
    // side-effect-free and always available in 64-bit mode.
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0x1B, // IA32_APIC_BASE
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags),
        );
    }
    let base = ((hi as u64) << 32 | lo as u64) & 0xFFFFF000;
    // Check the APIC global enable bit (bit 11).
    if lo & (1 << 11) == 0 {
        return None;
    }
    Some(base as *mut u32)
}

impl TimerIrqGate for X86_64TimerIrqGate {
    fn enable_timer_irq() {
        // SAFETY: `lapic_base_x86_64` returns the LAPIC MMIO region only
        // when the APIC is globally enabled; the caller must have run
        // `InterruptController::init` first (see module docs). Volatile
        // reads/writes are required because LAPIC registers are MMIO.
        unsafe {
            let lapic_base = lapic_base_x86_64()
                .expect("X86_64TimerIrqGate::enable_timer_irq: LAPIC not enabled — must be called after InterruptController::init");
            // Clear LVT Timer Mask bit (LVT offset 0x320, bit 16).
            // C: APIC_LVTT_MASK (apic.c:44); the mask is set at apic.c:475-477.
            let lvt_timer = lapic_base.add(0x320 / 4);
            let v = core::ptr::read_volatile(lvt_timer);
            core::ptr::write_volatile(lvt_timer, v & !(1 << 16));
            // Set SVR Enable bit (offset 0xF0, bit 8).
            // C: apic.c:lapic_enable() sets the SVR enable (apic.c:674-700).
            // Note: also performed by X86_64InterruptController::init_lapic;
            // kept here per Follow-up 1 (05-clock-interrupt-init.md §4.7.1)
            // until the SVR/LVT responsibility split is decided.
            let svr = lapic_base.add(0xF0 / 4);
            let v = core::ptr::read_volatile(svr);
            core::ptr::write_volatile(svr, v | (1 << 8));
        }
    }

    fn disable_timer_irq() {
        // SAFETY: same invariant as `enable_timer_irq`.
        unsafe {
            let lapic_base = lapic_base_x86_64()
                .expect("X86_64TimerIrqGate::disable_timer_irq: LAPIC not enabled — must be called after InterruptController::init");
            // Set LVT Timer Mask bit (LVT offset 0x320, bit 16).
            // C: apic.c:475-477 — the LAPIC timer is masked during calibration.
            let lvt_timer = lapic_base.add(0x320 / 4);
            let v = core::ptr::read_volatile(lvt_timer);
            core::ptr::write_volatile(lvt_timer, v | (1 << 16));
        }
    }
}
