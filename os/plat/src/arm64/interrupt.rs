//! ARM64 (aarch64) GICv3 interrupt controller implementation
//!
//! Implements `InterruptController` for ARM64 using GICv3 (Generic Interrupt
//! Controller version 3).
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! Hardware base addresses (GICD, GICR) are stored in instance fields,
//! populated by `new(desc)` via `Any` downcast to `Gicv3Desc`. This
//! replaces the previous `new()` + `set_base()` two-step pattern.

use minix_platform::arch::aarch64::Gicv3Desc;

use crate::interrupt::{InterruptController, IrqVector, NR_IRQ_VECTORS};

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
/// GICR_WAKER.ChildrenAsleep bit (read-only).
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 0x4;

/// ARM64 GICv3 interrupt controller.
///
/// # Fields
///
/// - `gicd_base`: GIC Distributor MMIO base, from `Gicv3Desc`.
/// - `gicr_base`: GIC Redistributor MMIO base, from `Gicv3Desc`.
/// - `nr_irqs`: number of IRQ vectors (from descriptor, clamped to `NR_IRQ_VECTORS`).
/// - `last_iar`: last acknowledged interrupt ID (saved from ICC_IAR1_EL1 read).
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
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
        let gicv3 = desc.as_any()
            .downcast_ref::<Gicv3Desc>()
            .expect("AArch64InterruptController::new: expected Gicv3Desc");
        Self {
            gicd_base: gicv3.gicd_base,
            gicr_base: gicv3.gicr_base,
            nr_irqs: (gicv3.nr_irqs as usize).min(NR_IRQ_VECTORS),
            last_iar: 0,
        }
    }

    fn init(&mut self) {
        assert!(self.gicd_base != 0, "AArch64InterruptController: gicd_base is zero (descriptor did not provide GICD base)");
        assert!(self.gicr_base != 0, "AArch64InterruptController: gicr_base is zero (descriptor did not provide GICR base)");
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
    fn test_new_from_gicv3_descriptor() {
        let desc = Gicv3Desc {
            gicd_base: 0x0800_0000,
            gicr_base: 0x080A_0000,
            gicr_stride: 0x2_0000,
            nr_irqs: 64,
        };
        let ic = AArch64InterruptController::new(&desc);
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
