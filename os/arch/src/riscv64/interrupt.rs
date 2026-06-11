//! RISC-V 64-bit PLIC interrupt controller implementation
//!
//! Implements `InterruptController` for RISC-V 64-bit using PLIC
//! (Platform-Level Interrupt Controller). PLIC manages external
//! interrupts (UART, disk, network, etc.), while CLINT handles
//! timer and inter-processor interrupts.
//!
//! # PLIC architecture
//!
//! PLIC has three main register regions:
//!
//! - **Priority**: Per-interrupt priority threshold (0 = disabled)
//! - **Enable**: Per-context interrupt enable bits
//! - **Threshold/Claim/Complete**: Per-context control registers
//!
//! A "context" is a privilege-mode/hart combination. For S-mode on
//! hart 0, the context ID is typically 1 (0 = M-mode, 1 = S-mode).
//!
//! # QEMU virt PLIC mapping
//!
//! | Register | Offset | Description |
//! |----------|--------|-------------|
//! | Priority | 0x0000 | 4 bytes per IRQ |
//! | Pending | 0x1000 | Bitmask per 32 IRQs |
//! | Enable | 0x2000 | Bitmask per 32 IRQs per context |
//! | Threshold | 0x200000 | 4 bytes per context |
//! | Claim | 0x200004 | 4 bytes per context (read) |
//! | Complete | 0x200004 | 4 bytes per context (write) |
//!
//! C: No Minix3 equivalent (Minix3 has no RISC-V port).

use crate::interrupt::{InterruptController, IrqVector, NR_IRQ_VECTORS};

/// PLIC base address for QEMU virt machine.
/// TODO: Should be discovered from device tree.
const PLIC_BASE: usize = 0x0C00_0000;

/// PLIC register offsets.
const PLIC_PRIORITY: usize = 0x0000;
const PLIC_PENDING: usize = 0x1000;
const PLIC_ENABLE: usize = 0x2000;
const PLIC_THRESHOLD: usize = 0x200000;
const PLIC_CLAIM: usize = 0x200004;
const PLIC_COMPLETE: usize = 0x200004;

/// S-mode context offset for hart 0.
/// Context 0 = M-mode, Context 1 = S-mode (QEMU virt).
const S_MODE_CONTEXT: usize = 1;

/// RISC-V 64-bit PLIC interrupt controller.
///
/// Manages external interrupts via the PLIC MMIO interface.
/// Timer interrupts are handled by CLINT (separate hardware).
///
/// C: No Minix3 equivalent (Minix3 has no RISC-V port).
pub struct Riscv64InterruptController {
    /// PLIC MMIO base address.
    plic_base: usize,
    /// Number of IRQ sources supported.
    nr_irqs: usize,
    /// S-mode context ID for the current hart.
    context: usize,
    /// Last claimed interrupt ID (saved from claim register read).
    /// Must be written to complete register in eoi().
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

    /// Calculate the enable register offset for a given context and IRQ.
    fn enable_offset(context: usize, irq: u8) -> usize {
        PLIC_ENABLE + context * 0x80 + (irq as usize / 32) * 4
    }

    /// Calculate the threshold register offset for a given context.
    fn threshold_offset(context: usize) -> usize {
        PLIC_THRESHOLD + context * 0x1000
    }

    /// Calculate the claim/complete register offset for a given context.
    fn claim_offset(context: usize) -> usize {
        PLIC_CLAIM + context * 0x1000
    }

    /// Read a 32-bit PLIC register.
    ///
    /// SAFETY: Caller must ensure plic_base + offset is a valid MMIO address.
    unsafe fn plic_read32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.plic_base + offset) as *const u32)
    }

    /// Write a 32-bit PLIC register.
    ///
    /// SAFETY: Caller must ensure plic_base + offset is a valid MMIO address.
    unsafe fn plic_write32(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.plic_base + offset) as *mut u32, value);
    }
}

impl InterruptController for Riscv64InterruptController {
    fn init(&mut self) {
        // PLIC initialization sequence:
        // 1. Set all interrupt priorities to 1 (minimum active)
        // 2. Disable all interrupts (enable = 0)
        // 3. Set threshold to 0 (accept all priorities)

        unsafe {
            // Set all priorities to 1 (minimum active priority)
            for irq in 1..self.nr_irqs {
                self.plic_write32(PLIC_PRIORITY + irq * 4, 1);
            }

            // Disable all interrupts for this context
            for word in 0..(self.nr_irqs + 31) / 32 {
                self.plic_write32(PLIC_ENABLE + self.context * 0x80 + word * 4, 0);
            }

            // Set priority threshold to 0 (accept all)
            self.plic_write32(Self::threshold_offset(self.context), 0);
        }
    }

    fn mask(&mut self, irq: IrqVector) {
        // Clear the enable bit for this IRQ in the S-mode context.
        let irq_num = irq.get() as usize;
        if irq_num == 0 {
            return; // IRQ 0 does not exist in PLIC
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
        // Set the enable bit for this IRQ in the S-mode context.
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
        // Read the claim register to acknowledge the highest-priority
        // pending interrupt. This returns the interrupt ID and
        // automatically clears the pending bit.
        // Save the claimed ID for use in eoi().
        let claimed: u32;
        unsafe {
            claimed = self.plic_read32(Self::claim_offset(self.context));
        }
        self.last_claimed = claimed;
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // Write the interrupt ID to the complete register to signal
        // end-of-interrupt. PLIC requires writing the same ID that
        // was read from the claim register.
        unsafe {
            self.plic_write32(Self::claim_offset(self.context), self.last_claimed);
        }
    }

    fn mask_all(&mut self) {
        // Disable all interrupts for this context.
        for word in 0..(self.nr_irqs + 31) / 32 {
            unsafe {
                self.plic_write32(PLIC_ENABLE + self.context * 0x80 + word * 4, 0);
            }
        }
    }
}
