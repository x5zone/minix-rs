//! ARM64 (aarch64) GICv3 interrupt controller implementation
//!
//! Implements `InterruptController` for ARM64 using GICv3 (Generic Interrupt
//! Controller version 3).

use crate::interrupt::{InterruptController, IrqVector, NR_IRQ_VECTORS};

/// GICv3 Distributor base address offset (from GIC base).
const GICD_OFFSET: usize = 0x0000_0000;

/// GICv3 Redistributor base address offset (from GIC base).
const GICR_OFFSET: usize = 0x000A_0000;

/// GICD_CTLR: Distributor Control Register.
const GICD_CTLR: usize = 0x0000;
/// GICD_CTLR.EnableGrp1NS bit.
const GICD_CTLR_ENABLE_GRP1NS: u32 = 0x2;

/// GICD_ISENABLER<n>: Interrupt Set-Enable Register.
const GICD_ISENABLER: usize = 0x0100;

/// GICD_ICENABLER<n>: Interrupt Clear-Enable Register.
const GICD_ICENABLER: usize = 0x0180;

/// GICD_IGROUPR<n>: Interrupt Group Register.
const GICD_IGROUPR: usize = 0x0080;

/// GICR_ISENABLER0: Redistributor Interrupt Set-Enable Register (SGI+PPI, INTID 0-31).
const GICR_ISENABLER0: usize = 0x0100;

/// GICR_ICENABLER0: Redistributor Interrupt Clear-Enable Register (SGI+PPI, INTID 0-31).
const GICR_ICENABLER0: usize = 0x0180;

/// GICR_WAKER: Redistributor Wake Register.
const GICR_WAKER: usize = 0x0014;
/// GICR_WAKER.ProcessorSleep bit.
const GICR_WAKER_PROCESSOR_SLEEP: u32 = 0x2;
/// GICR_WAKER.ChildrenAsleep bit (read-only).
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 0x4;

/// ARM64 GICv3 interrupt controller.
pub struct AArch64InterruptController {
    /// GIC Distributor MMIO base address.
    gicd_base: usize,
    /// GIC Redistributor MMIO base address.
    gicr_base: usize,
    /// Number of IRQ vectors supported.
    nr_irqs: usize,
    /// Last acknowledged interrupt ID (saved from ICC_IAR1_EL1 read).
    last_iar: u32,
}

impl AArch64InterruptController {
    pub const fn new() -> Self {
        Self {
            gicd_base: 0,
            gicr_base: 0,
            nr_irqs: NR_IRQ_VECTORS,
            last_iar: 0,
        }
    }

    /// Set GIC base addresses from device tree / platform discovery.
    pub fn set_base(&mut self, gicd_base: usize, gicr_base: usize) {
        self.gicd_base = gicd_base;
        self.gicr_base = gicr_base;
    }

    unsafe fn gicd_read32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.gicd_base + offset) as *const u32)
    }

    unsafe fn gicd_write32(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.gicd_base + offset) as *mut u32, value);
    }

    unsafe fn gicr_read32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.gicr_base + offset) as *const u32)
    }

    unsafe fn gicr_write32(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.gicr_base + offset) as *mut u32, value);
    }

    fn init_distributor(&mut self) {
        unsafe {
            for irq in (32..self.nr_irqs).step_by(32) {
                let reg = (irq / 32) as usize;
                self.gicd_write32(GICD_IGROUPR + reg * 4, 0xFFFF_FFFF);
            }
            for irq in (32..self.nr_irqs).step_by(32) {
                let reg = (irq / 32) as usize;
                self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF);
            }
            self.gicd_write32(GICD_CTLR, GICD_CTLR_ENABLE_GRP1NS);
        }
    }

    fn init_redistributor(&mut self) {
        unsafe {
            self.gicr_write32(GICR_WAKER, 0);
            while self.gicr_read32(GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP != 0 {
                core::hint::spin_loop();
            }
        }
    }

    fn init_cpu_interface(&mut self) {
        unsafe {
            let mut sre: u64;
            core::arch::asm!("mrs {}, icc_sre_el1", out(reg) sre);
            sre |= 0x7;
            core::arch::asm!("msr icc_sre_el1, {}", in(reg) sre);
            core::arch::asm!("msr icc_pmr_el1, {}", in(reg) 0xFFu64);
            core::arch::asm!("msr icc_igrpen1_el1, {}", in(reg) 0x1u64);
        }
    }
}

impl InterruptController for AArch64InterruptController {
    fn init(&mut self) {
        assert!(self.gicd_base != 0, "AArch64InterruptController: gicd_base not set; call set_base() before init()");
        assert!(self.gicr_base != 0, "AArch64InterruptController: gicr_base not set; call set_base() before init()");
        self.init_distributor();
        self.init_redistributor();
        self.init_cpu_interface();
    }

    fn mask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        let bit = 1u32 << (irq_num % 32);
        if irq_num >= 32 {
            // SPI: use GICD_ICENABLER<n>
            let reg = (irq_num / 32) as usize;
            unsafe {
                self.gicd_write32(GICD_ICENABLER + reg * 4, bit);
            }
        } else {
            // SGI (0-15) / PPI (16-31): use GICR_ICENABLER0
            // GICv3 spec: GICR_ICENABLER0 bit N controls INTID N for this CPU.
            // SGI masking is technically supported but rarely used — the
            // register layout is the same as for PPI.
            unsafe {
                self.gicr_write32(GICR_ICENABLER0, bit);
            }
        }
    }

    fn unmask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        let bit = 1u32 << (irq_num % 32);
        if irq_num >= 32 {
            // SPI: use GICD_ISENABLER<n>
            let reg = (irq_num / 32) as usize;
            unsafe {
                self.gicd_write32(GICD_ISENABLER + reg * 4, bit);
            }
        } else {
            // SGI (0-15) / PPI (16-31): use GICR_ISENABLER0
            // GICv3 spec: GICR_ISENABLER0 bit N controls INTID N for this CPU.
            // SGI unmasking is technically supported but rarely used — the
            // register layout is the same as for PPI.
            unsafe {
                self.gicr_write32(GICR_ISENABLER0, bit);
            }
        }
    }

    fn ack(&mut self, _irq: IrqVector) {
        let iar: u64;
        unsafe {
            core::arch::asm!("mrs {}, icc_iar1_el1", out(reg) iar);
        }
        self.last_iar = (iar as u32) & 0x00FF_FFFF;
    }

    fn eoi(&mut self, _irq: IrqVector) {
        unsafe {
            core::arch::asm!("msr icc_eoir1_el1, {}", in(reg) self.last_iar as u64);
        }
    }

    fn mask_all(&mut self) {
        for irq in (32..self.nr_irqs).step_by(32) {
            let reg = (irq / 32) as usize;
            unsafe {
                self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_has_zero_bases() {
        let ic = AArch64InterruptController::new();
        assert_eq!(ic.gicd_base, 0);
        assert_eq!(ic.gicr_base, 0);
    }

    #[test]
    fn test_set_base_overrides() {
        let mut ic = AArch64InterruptController::new();
        ic.set_base(0x0800_0000, 0x080A_0000);
        assert_eq!(ic.gicd_base, 0x0800_0000);
        assert_eq!(ic.gicr_base, 0x080A_0000);
    }

    #[test]
    fn test_gic_register_offsets() {
        assert_eq!(GICD_CTLR, 0x0000);
        assert_eq!(GICD_IGROUPR, 0x0080);
        assert_eq!(GICD_ISENABLER, 0x0100);
        assert_eq!(GICD_ICENABLER, 0x0180);
        // GICv3 Redistributor SGI+PPI enable registers
        assert_eq!(GICR_ISENABLER0, 0x0100);
        assert_eq!(GICR_ICENABLER0, 0x0180);
    }

    #[test]
    fn test_ppi_bit_calculation() {
        // PPI 16 (timer): bit 16
        assert_eq!(1u32 << (16 % 32), 0x1_0000);
        // PPI 25 (virt timer on some platforms): bit 25
        assert_eq!(1u32 << (25 % 32), 0x0200_0000);
        // SGI 0: bit 0
        assert_eq!(1u32 << (0 % 32), 0x1);
    }
}
