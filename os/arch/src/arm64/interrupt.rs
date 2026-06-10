//! ARM64 (aarch64) GICv3 interrupt controller implementation
//!
//! Implements `InterruptController` for ARM64 using GICv3 (Generic Interrupt
//! Controller version 3). This is the standard interrupt controller for
//! ARMv8-A systems, used by QEMU virt and most ARM64 SoCs.
//!
//! # GICv3 architecture
//!
//! GICv3 has three main components:
//!
//! - **Distributor (GICD)**: System-wide, manages Shared Peripheral Interrupts
//!   (SPI). Controls routing, priority, and enable/disable state.
//! - **Redistributor (GICR)**: Per-CPU, manages Private Peripheral Interrupts
//!   (PPI) and Software Generated Interrupts (SGI).
//! - **CPU Interface (ICC_*_EL1)**: Per-CPU system registers for interrupt
//!   acknowledge, priority masking, and end-of-interrupt.
//!
//! # Interrupt types
//!
//! | Type | Range | Source |
//! |------|-------|--------|
//! | SGI | 0-15 | Software generated (inter-processor) |
//! | PPI | 16-31 | Per-CPU peripherals (timer, PMU, etc.) |
//! | SPI | 32-1019 | System peripherals (UART, block dev, etc.) |
//! | LPI | 8192+ | Message-based (ITS), not used by Minix-RS |
//!
//! C: omap_intr.c (OMAP INTC, 32-bit ARM) — minix-rs uses GICv3 instead

use crate::interrupt::{InterruptController, IrqVector, NR_IRQ_VECTORS};

/// GICv3 Distributor base address offset (from GIC base).
/// QEMU virt: GICD at 0x08000000
const GICD_OFFSET: usize = 0x0000_0000;

/// GICv3 Redistributor base address offset (from GIC base).
/// QEMU virt: GICR at 0x080A0000
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

/// GICR_WAKER: Redistributor Wake Register.
const GICR_WAKER: usize = 0x0014;
/// GICR_WAKER.ProcessorSleep bit.
const GICR_WAKER_PROCESSOR_SLEEP: u32 = 0x2;
/// GICR_WAKER.ChildrenAsleep bit (read-only).
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 0x4;

/// ARM64 GICv3 interrupt controller.
///
/// Combines Distributor (system-wide SPI management) and Redistributor
/// (per-CPU PPI/SGI management) operations. CPU Interface uses system
/// registers (ICC_*_EL1) accessed via MSR/MRS instructions.
///
/// C: omap_intr.c (OMAP INTC, 32-bit ARM)
/// minix-rs uses GICv3 (64-bit ARM + QEMU virt)
pub struct AArch64InterruptController {
    /// GIC Distributor MMIO base address.
    gicd_base: usize,
    /// GIC Redistributor MMIO base address.
    gicr_base: usize,
    /// Number of IRQ vectors supported.
    nr_irqs: usize,
}

impl AArch64InterruptController {
    pub const fn new() -> Self {
        Self {
            gicd_base: 0,
            gicr_base: 0,
            nr_irqs: NR_IRQ_VECTORS,
        }
    }

    /// Set GIC base addresses from device tree / platform discovery.
    pub fn set_base(&mut self, gicd_base: usize, gicr_base: usize) {
        self.gicd_base = gicd_base;
        self.gicr_base = gicr_base;
    }

    /// Read a 32-bit GIC Distributor register.
    ///
    /// SAFETY: Caller must ensure gicd_base is a valid MMIO address.
    unsafe fn gicd_read32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.gicd_base + offset) as *const u32)
    }

    /// Write a 32-bit GIC Distributor register.
    ///
    /// SAFETY: Caller must ensure gicd_base is a valid MMIO address.
    unsafe fn gicd_write32(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.gicd_base + offset) as *mut u32, value);
    }

    /// Read a 32-bit GIC Redistributor register.
    ///
    /// SAFETY: Caller must ensure gicr_base is a valid MMIO address.
    unsafe fn gicr_read32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.gicr_base + offset) as *const u32)
    }

    /// Write a 32-bit GIC Redistributor register.
    ///
    /// SAFETY: Caller must ensure gicr_base is a valid MMIO address.
    unsafe fn gicr_write32(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.gicr_base + offset) as *mut u32, value);
    }

    /// Initialize the GICv3 Distributor.
    fn init_distributor(&mut self) {
        // C: omap_intr.c:24-44 — map INTC base address
        // GICv3 equivalent: enable Distributor, configure routing

        unsafe {
            // 1. Assign all SPIs to Group 1 (Non-secure)
            for irq in (32..self.nr_irqs).step_by(32) {
                let reg = (irq / 32) as usize;
                self.gicd_write32(GICD_IGROUPR + reg * 4, 0xFFFF_FFFF);
            }

            // 2. Disable all SPIs
            for irq in (32..self.nr_irqs).step_by(32) {
                let reg = (irq / 32) as usize;
                self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF);
            }

            // 3. Enable Distributor (Group 1 Non-secure)
            self.gicd_write32(GICD_CTLR, GICD_CTLR_ENABLE_GRP1NS);
        }
    }

    /// Initialize the GICv3 Redistributor for the current CPU.
    fn init_redistributor(&mut self) {
        unsafe {
            // Wake up the Redistributor
            self.gicr_write32(GICR_WAKER, 0);

            // Wait until ChildrenAsleep is cleared
            while self.gicr_read32(GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP != 0 {
                core::hint::spin_loop();
            }
        }
    }

    /// Initialize the GICv3 CPU Interface using system registers.
    fn init_cpu_interface(&mut self) {
        unsafe {
            // Enable System Register Interface (ICC_SRE_EL1)
            let mut sre: u64;
            core::arch::asm!("mrs {}, icc_sre_el1", out(reg) sre);
            sre |= 0x7; // SRE, Enable
            core::arch::asm!("msr icc_sre_el1, {}", in(reg) sre);

            // Set priority mask to lowest (accept all interrupts)
            // ICC_PMR_EL1: priority 0xFF = lowest priority = all accepted
            core::arch::asm!("msr icc_pmr_el1, {}", in(reg) 0xFFu64);

            // Enable Group 1 Non-secure interrupts
            // ICC_IGRPEN1_EL1: EnableGrp1NS bit
            core::arch::asm!("msr icc_igrpen1_el1, {}", in(reg) 0x1u64);
        }
    }
}

impl InterruptController for AArch64InterruptController {
    fn init(&mut self) {
        // C: intr_init() — omap_intr.c:24 / GICv3 equivalent
        self.init_distributor();
        self.init_redistributor();
        self.init_cpu_interface();
        // All SPIs already disabled by init_distributor()
    }

    fn mask(&mut self, irq: IrqVector) {
        // C: bsp_irq_mask(irq) — omap_intr.c:63
        let irq_num = irq.get() as usize;
        if irq_num < 32 {
            // PPI/SGI: handled by Redistributor (GICR_ICENABLER0)
            // TODO: implement PPI masking
        } else {
            // SPI: handled by Distributor (GICD_ICENABLER<n>)
            let reg = (irq_num / 32) as usize;
            let bit = 1u32 << (irq_num % 32);
            unsafe {
                self.gicd_write32(GICD_ICENABLER + reg * 4, bit);
            }
        }
    }

    fn unmask(&mut self, irq: IrqVector) {
        // C: bsp_irq_unmask(irq) — omap_intr.c:57
        let irq_num = irq.get() as usize;
        if irq_num < 32 {
            // PPI/SGI: handled by Redistributor (GICR_ISENABLER0)
            // TODO: implement PPI unmasking
        } else {
            // SPI: handled by Distributor (GICD_ISENABLER<n>)
            let reg = (irq_num / 32) as usize;
            let bit = 1u32 << (irq_num % 32);
            unsafe {
                self.gicd_write32(GICD_ISENABLER + reg * 4, bit);
            }
        }
    }

    fn ack(&mut self, _irq: IrqVector) {
        // Read ICC_IAR1_EL1 to acknowledge the highest-priority pending interrupt.
        // This returns the interrupt ID and marks it as "in service".
        let _iar: u64;
        unsafe {
            core::arch::asm!("mrs {}, icc_iar1_el1", out(reg) _iar);
        }
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // Write ICC_EOIR1_EL1 to signal end of interrupt processing.
        // The interrupt ID should be written, but we can use the value
        // saved from ack() — for now, just write 0 as placeholder.
        // TODO: save IAR value from ack() and write it here.
        unsafe {
            core::arch::asm!("msr icc_eoir1_el1, {}", in(reg) 0u64);
        }
    }

    fn mask_all(&mut self) {
        // Disable all SPIs in the Distributor.
        // PPIs are per-CPU and handled separately.
        for irq in (32..self.nr_irqs).step_by(32) {
            let reg = (irq / 32) as usize;
            unsafe {
                self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF);
            }
        }
    }
}
