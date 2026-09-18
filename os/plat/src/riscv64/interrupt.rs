//! RISC-V 64-bit PLIC interrupt controller implementation
//!
//! Implements `InterruptController` for RISC-V 64-bit using PLIC
//! (Platform-Level Interrupt Controller).
//!
//! Minix3 has no RISC-V port — architectural evolution `[ARCH: K-2]`
//! (05-clock-interrupt-init.md §3.8).
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! Hardware base address (PLIC), context ID, and IRQ count are stored in
//! instance fields, populated by `new(desc)` via `Any` downcast to `PlicDesc`.
//! This replaces the previous `new()` + `set_base()` two-step pattern and the
//! `PLIC_BASE` / `S_MODE_CONTEXT` hardcoded constants.

use minix_platform::arch::riscv64::PlicDesc;

use crate::interrupt::{InterruptRouter, PerCpuInterruptUnit, IrqVector, NR_IRQ_VECTORS};

/// PLIC register offsets.
const PLIC_PRIORITY: usize = 0x0000;
const PLIC_ENABLE: usize = 0x2000;
const PLIC_THRESHOLD: usize = 0x200000;
const PLIC_CLAIM: usize = 0x200004;
/// PLIC complete register offset.
/// In the PLIC spec, the complete register shares the same offset as claim:
/// reading claims an interrupt, writing completes it.
const PLIC_COMPLETE: usize = PLIC_CLAIM;

/// The boot clock source's dispatch identity: pseudo-vector 0.
///
/// The S-mode timer is a CPU-local interrupt — it fires when `mtime`
/// crosses `mtimecmp` and is gated by `sie.STIE`; it never crosses the
/// PLIC, so it has no controller source number. Vector 0 is reserved as
/// the timer's dispatch identity (the future trap entry maps
/// `scause == SupervisorTimer` to it), and the PLIC mask/unmask paths
/// already treat vector 0 as "no controller line" — a reserved pseudo
/// vector, not an accident. The module-local gate lives in
/// `TimerIrqGate` (`sie.STIE`).
pub const TIMER_IRQ: IrqVector = IrqVector::new(0);

/// The statistical-profiling clock's dispatch identity: pseudo-vector 0.
///
/// There is no dedicated profiling clock on riscv64 — the arch
/// implementation returns `ProfileClockError::Unsupported` (the CLINT
/// `mtimecmp` is already owned by the scheduler; no second S-mode
/// comparator is standardized), so no hook is ever registered on this
/// identity and no delivery path can produce it. It exists so the
/// kernel-side profile wiring compiles per-architecture the same way
/// `TIMER_IRQ` does.
pub const PROFILE_CLOCK_IRQ: IrqVector = IrqVector::new(0);

/// RISC-V 64-bit PLIC interrupt controller.
///
/// # Fields
///
/// - `plic_base`: PLIC MMIO base, from `PlicDesc`.
/// - `nr_irqs`: number of IRQ sources (from descriptor, clamped to `NR_IRQ_VECTORS`).
/// - `context`: S-mode context ID for the current hart, from descriptor.
pub struct Riscv64InterruptController {
    /// PLIC MMIO base address.
    plic_base: usize,
    /// Number of IRQ sources supported.
    nr_irqs: usize,
    /// S-mode context ID for the current hart.
    context: usize,
}

impl Riscv64InterruptController {
    fn threshold_offset(context: usize) -> usize {
        PLIC_THRESHOLD + context * 0x1000
    }

    fn claim_offset(context: usize) -> usize {
        PLIC_CLAIM + context * 0x1000
    }

    /// PLIC MMIO base (read-only accessor).
    pub fn plic_base(&self) -> usize {
        self.plic_base
    }

    unsafe fn plic_read32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.plic_base + offset) as *const u32)
    }

    unsafe fn plic_write32(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.plic_base + offset) as *mut u32, value);
    }
}

impl InterruptRouter for Riscv64InterruptController {
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
        let plic = desc.as_any()
            .downcast_ref::<PlicDesc>()
            .expect("Riscv64InterruptController::new: expected PlicDesc");
        Self {
            plic_base: plic.plic_base,
            nr_irqs: (plic.nr_irqs as usize).min(NR_IRQ_VECTORS),
            context: plic.context as usize,
        }
    }

    fn init(&mut self) {
        unsafe {
            for irq in 1..self.nr_irqs {
                self.plic_write32(PLIC_PRIORITY + irq * 4, 1);
            }
            for word in 0..(self.nr_irqs + 31) / 32 {
                self.plic_write32(PLIC_ENABLE + self.context * 0x80 + word * 4, 0);
            }
            self.plic_write32(Self::threshold_offset(self.context), 0);
        }
    }

    fn mask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        if irq_num == 0 {
            return;
        }
        let word = irq_num / 32;
        let bit = irq_num % 32;
        unsafe {
            let offset = PLIC_ENABLE + self.context * 0x80 + word * 4;
            let mut val = self.plic_read32(offset);
            val &= !(1u32 << bit);
            self.plic_write32(offset, val);
        }
    }

    fn unmask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        if irq_num == 0 {
            return;
        }
        let word = irq_num / 32;
        let bit = irq_num % 32;
        unsafe {
            let offset = PLIC_ENABLE + self.context * 0x80 + word * 4;
            let mut val = self.plic_read32(offset);
            val |= 1u32 << bit;
            self.plic_write32(offset, val);
        }
    }


    fn mask_all(&mut self) {
        for word in 0..(self.nr_irqs + 31) / 32 {
            unsafe {
                self.plic_write32(PLIC_ENABLE + self.context * 0x80 + word * 4, 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_platform::InterruptControllerDesc;

    #[test]
    fn test_new_from_plic_descriptor() {
        let desc = PlicDesc {
            plic_base: 0x0C00_0000,
            nr_irqs: 53,
            context: 1,
        };
        let ic = Riscv64InterruptController::new(&desc);
        assert_eq!(ic.plic_base(), 0x0C00_0000);
    }

    #[test]
    fn test_new_clamps_nr_irqs_to_max() {
        let desc = PlicDesc {
            plic_base: 0x0C00_0000,
            nr_irqs: 128, // exceeds NR_IRQ_VECTORS (64)
            context: 1,
        };
        let ic = Riscv64InterruptController::new(&desc);
        // nr_irqs clamped at construction; init() iterates 1..nr_irqs so the
        // clamp bounds MMIO access to the supported IRQ source count.
        assert_eq!(ic.nr_irqs, NR_IRQ_VECTORS);
        assert_eq!(ic.plic_base(), 0x0C00_0000);
    }

    #[test]
    fn test_timer_irq_is_the_reserved_pseudo_vector() {
        // D-59: the S-mode timer never crosses the PLIC (it is a CPU-local
        // interrupt gated by sie.STIE), so its dispatch identity is the
        // reserved pseudo-vector 0 — the same value the PLIC mask/unmask
        // paths special-case as "no controller line". If the constant ever
        // moves off 0, the trap-entry mapping (scause SupervisorTimer →
        // vector) and the PLIC special case must move with it.
        assert_eq!(TIMER_IRQ.get(), 0);
    }

    #[test]
    fn test_profile_clock_irq_is_unreachable_pseudo() {
        // init_profile_clock on riscv64 reports Unsupported (no second
        // S-mode comparator), so no hook is ever registered on this
        // identity and the trap entry's line check can never match.
        assert_eq!(PROFILE_CLOCK_IRQ.get(), 0);
    }
}

impl PerCpuInterruptUnit for Riscv64InterruptController {
    fn claim(&mut self) -> Option<u32> {
        // SAFETY: PLIC claim-register read on the interrupting CPU; the
        // read claims the interrupt and returns its ID (0 = nothing
        // pending → nothing to handle or complete).
        unsafe {
            let id = self.plic_read32(Self::claim_offset(self.context));
            (id != 0).then_some(id)
        }
    }

    fn complete(&mut self, claimed: Option<u32>) {
        let Some(id) = claimed else {
            return; // nothing claimed: nothing to complete
        };
        // SAFETY: write the claimed ID to the complete register (the
        // complete register shares the claim offset; write = complete).
        // The ID is exactly what claim returned — the pairing is the
        // protocol (D-61).
        unsafe {
            self.plic_write32(PLIC_COMPLETE + self.context * 0x1000, id);
        }
    }
}
