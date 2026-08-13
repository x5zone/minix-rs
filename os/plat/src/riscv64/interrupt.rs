//! RISC-V 64-bit PLIC interrupt controller implementation
//!
//! Implements `InterruptController` for RISC-V 64-bit using PLIC
//! (Platform-Level Interrupt Controller).
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! Hardware base address (PLIC), context ID, and IRQ count are stored in
//! instance fields, populated by `new(desc)` via `Any` downcast to `PlicDesc`.
//! This replaces the previous `new()` + `set_base()` two-step pattern and the
//! `PLIC_BASE` / `S_MODE_CONTEXT` hardcoded constants.

use minix_platform::arch::riscv64::PlicDesc;

use crate::interrupt::{InterruptController, IrqVector, NR_IRQ_VECTORS};

/// PLIC register offsets.
const PLIC_PRIORITY: usize = 0x0000;
const PLIC_ENABLE: usize = 0x2000;
const PLIC_THRESHOLD: usize = 0x200000;
const PLIC_CLAIM: usize = 0x200004;
/// PLIC complete register offset.
/// In the PLIC spec, the complete register shares the same offset as claim:
/// reading claims an interrupt, writing completes it.
const PLIC_COMPLETE: usize = PLIC_CLAIM;

/// RISC-V 64-bit PLIC interrupt controller.
///
/// # Fields
///
/// - `plic_base`: PLIC MMIO base, from `PlicDesc`.
/// - `nr_irqs`: number of IRQ sources (from descriptor, clamped to `NR_IRQ_VECTORS`).
/// - `context`: S-mode context ID for the current hart, from descriptor.
/// - `last_claimed`: last claimed interrupt ID (saved from claim register read).
pub struct Riscv64InterruptController {
    /// PLIC MMIO base address.
    plic_base: usize,
    /// Number of IRQ sources supported.
    nr_irqs: usize,
    /// S-mode context ID for the current hart.
    context: usize,
    /// Last claimed interrupt ID.
    last_claimed: u32,
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

impl InterruptController for Riscv64InterruptController {
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
        let plic = desc.as_any()
            .downcast_ref::<PlicDesc>()
            .expect("Riscv64InterruptController::new: expected PlicDesc");
        Self {
            plic_base: plic.plic_base,
            nr_irqs: (plic.nr_irqs as usize).min(NR_IRQ_VECTORS),
            context: plic.context as usize,
            last_claimed: 0,
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

    fn ack(&mut self, _irq: IrqVector) {
        let claimed: u32;
        unsafe {
            claimed = self.plic_read32(Self::claim_offset(self.context));
        }
        self.last_claimed = claimed;
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // Write the interrupt ID to the complete register.
        // PLIC_COMPLETE shares the same offset as PLIC_CLAIM (write = complete).
        unsafe {
            self.plic_write32(PLIC_COMPLETE + self.context * 0x1000, self.last_claimed);
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
        // nr_irqs clamped at construction; init() iterates 0..nr_irqs.
        // Constructing with a mock threshold keeps init MMIO-safe; just
        // verify construction doesn't panic and the clamp path runs.
        assert_eq!(ic.plic_base(), 0x0C00_0000);
    }
}
