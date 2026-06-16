//! RISC-V 64-bit PLIC interrupt controller implementation
//!
//! Implements `InterruptController` for RISC-V 64-bit using PLIC
//! (Platform-Level Interrupt Controller).

use crate::interrupt::{InterruptController, IrqVector, NR_IRQ_VECTORS};

/// PLIC base address for QEMU virt machine.
const PLIC_BASE: usize = 0x0C00_0000;

/// PLIC register offsets.
const PLIC_PRIORITY: usize = 0x0000;
const PLIC_ENABLE: usize = 0x2000;
const PLIC_THRESHOLD: usize = 0x200000;
const PLIC_CLAIM: usize = 0x200004;

/// S-mode context offset for hart 0.
const S_MODE_CONTEXT: usize = 1;

/// RISC-V 64-bit PLIC interrupt controller.
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
    pub const fn new() -> Self {
        Self {
            plic_base: PLIC_BASE,
            nr_irqs: NR_IRQ_VECTORS,
            context: S_MODE_CONTEXT,
            last_claimed: 0,
        }
    }

    /// Set PLIC base address from device tree / platform discovery.
    pub fn set_base(&mut self, plic_base: usize) {
        self.plic_base = plic_base;
    }

    fn threshold_offset(context: usize) -> usize {
        PLIC_THRESHOLD + context * 0x1000
    }

    fn claim_offset(context: usize) -> usize {
        PLIC_CLAIM + context * 0x1000
    }

    unsafe fn plic_read32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.plic_base + offset) as *const u32)
    }

    unsafe fn plic_write32(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.plic_base + offset) as *mut u32, value);
    }
}

impl InterruptController for Riscv64InterruptController {
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
        unsafe {
            self.plic_write32(Self::claim_offset(self.context), self.last_claimed);
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
