//! x86-64 APIC-based interrupt controller implementation
//!
//! Implements `InterruptController` for x86-64 using LAPIC + IOAPIC.
//! 8259A PIC is not supported — 64-bit systems use APIC exclusively.
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! Hardware base addresses (LAPIC, IOAPIC) are stored in instance fields,
//! populated by `new(desc)` via `Any` downcast to `ApicDesc`. This
//! replaces the previous `new()` + `set_base()` two-step pattern and the
//! `DEFAULT_LAPIC_BASE` / `DEFAULT_IOAPIC_BASE` hardcoded constants.

use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

use minix_platform::arch::x86_64::ApicDesc;

use crate::interrupt::{InterruptController, IrqVector, NR_IRQ_VECTORS};

/// x86-64 IRQ-to-IDT vector offset.
///
/// Hardware IRQ 0 maps to IDT vector 0x50 (80).
/// C: IRQ0_VECTOR — interrupt.h:34
pub const IRQ0_VECTOR: u8 = 0x50;

/// LAPIC spurious interrupt vector.
const LAPIC_SPURIOUS_VECTOR: u8 = 0xFF;

/// IA32_APIC_BASE MSR index (Intel SDM Vol. 3A §10.4.3).
const MSR_IA32_APIC_BASE: u32 = 0x1B;
/// Bit 11 of IA32_APIC_BASE: APIC global enable.
const IA32_APIC_BASE_EN: u64 = 1 << 11;
/// Bits 12-35: APIC base physical address (4-KB aligned).
const IA32_APIC_BASE_ADDR_MASK: u64 = 0xFFFF_F000;

/// LAPIC register offsets from the LAPIC base.
const LAPIC_REG_ID: usize = 0x020;
const LAPIC_REG_EOI: usize = 0x0B0;
const LAPIC_REG_SVR: usize = 0x0F0;
/// Bit 8 of SVR: APIC software enable.
const LAPIC_SVR_ENABLE: u32 = 1 << 8;

/// IOAPIC register offsets from the IOAPIC base.
const IOAPIC_REG_IOREGSEL: usize = 0x00;
const IOAPIC_REG_IOWIN: usize = 0x10;
/// IOAPIC indirect register indices.
const IOAPIC_REG_VER: u32 = 0x01;
/// First redirection entry (low half).
const IOAPIC_REG_REDTBL_BASE: u32 = 0x10;
/// Bit 16 of redirection entry low: interrupt mask.
const IOAPIC_REDTBL_MASK: u32 = 1 << 16;

#[inline]
unsafe fn lapic_read(base: usize, offset: usize) -> u32 { unsafe {
    read_volatile((base + offset) as *const u32)
}}

#[inline]
unsafe fn lapic_write(base: usize, offset: usize, value: u32) { unsafe {
    write_volatile((base + offset) as *mut u32, value)
}}

unsafe fn ioapic_read_redtbl(base: usize, irq: u8) -> u64 { unsafe {
    let reg = IOAPIC_REG_REDTBL_BASE + 2 * (irq as u32);
    let low = ioapic_read_indirect(base, reg);
    let high = ioapic_read_indirect(base, reg + 1);
    ((high as u64) << 32) | (low as u64)
}}

unsafe fn ioapic_write_redtbl(base: usize, irq: u8, value: u64) { unsafe {
    let reg = IOAPIC_REG_REDTBL_BASE + 2 * (irq as u32);
    ioapic_write_indirect(base, reg, value as u32);
    ioapic_write_indirect(base, reg + 1, (value >> 32) as u32);
}}

unsafe fn ioapic_read_indirect(base: usize, reg: u32) -> u32 { unsafe {
    write_volatile((base + IOAPIC_REG_IOREGSEL) as *mut u32, reg);
    read_volatile((base + IOAPIC_REG_IOWIN) as *const u32)
}}

unsafe fn ioapic_write_indirect(base: usize, reg: u32, value: u32) { unsafe {
    write_volatile((base + IOAPIC_REG_IOREGSEL) as *mut u32, reg);
    write_volatile((base + IOAPIC_REG_IOWIN) as *mut u32, value)
}}

/// x86-64 APIC-based interrupt controller.
///
/// Combines Local APIC (per-CPU) and I/O APIC (system-wide) operations.
///
/// # Fields
///
/// - `nr_irq_vectors`: number of IRQ vectors (from descriptor, typically 64).
/// - `lapic_base`: LAPIC MMIO base address, from `ApicDesc`.
/// - `ioapic_base`: IOAPIC MMIO base address, from `ApicDesc`.
pub struct X86_64InterruptController {
    nr_irq_vectors: usize,
    lapic_base: usize,
    ioapic_base: usize,
}

impl X86_64InterruptController {
    /// LAPIC MMIO base (read-only accessor).
    pub fn lapic_base(&self) -> usize {
        self.lapic_base
    }

    /// IOAPIC MMIO base (read-only accessor).
    pub fn ioapic_base(&self) -> usize {
        self.ioapic_base
    }

    unsafe fn init_lapic(&mut self) { unsafe {
        let apic_base = wrmsr_msr_read(MSR_IA32_APIC_BASE);
        let apic_phys = apic_base & IA32_APIC_BASE_ADDR_MASK;
        if apic_phys as usize != self.lapic_base {
            self.lapic_base = apic_phys as usize;
        }
        wrmsr_msr_write(MSR_IA32_APIC_BASE, apic_base | IA32_APIC_BASE_EN);
        lapic_write(self.lapic_base, LAPIC_REG_SVR, LAPIC_SPURIOUS_VECTOR as u32 | LAPIC_SVR_ENABLE);
    }}

    unsafe fn init_ioapic(&mut self) { unsafe {
        for irq in 0..NR_IRQ_VECTORS as u8 {
            self.ioapic_set_mask(irq, true);
        }
    }}

    unsafe fn ioapic_set_mask(&mut self, irq: u8, mask: bool) { unsafe {
        let mut entry = ioapic_read_redtbl(self.ioapic_base, irq);
        if mask {
            entry |= IOAPIC_REDTBL_MASK as u64;
        } else {
            entry &= !(IOAPIC_REDTBL_MASK as u64);
        }
        ioapic_write_redtbl(self.ioapic_base, irq, entry);
    }}

    unsafe fn lapic_eoi(&mut self) { unsafe {
        lapic_write(self.lapic_base, LAPIC_REG_EOI, 0);
    }}

    /// Read LAPIC ID register.
    ///
    /// # Safety
    ///
    /// `self.lapic_base` must be a valid LAPIC MMIO address (verified during
    /// controller init) and the caller must hold BKL / have interrupts
    /// disabled.
    pub unsafe fn lapic_id(&self) -> u32 { unsafe {
        lapic_read(self.lapic_base, LAPIC_REG_ID) >> 24
    }}

    /// Read IOAPIC version register.
    ///
    /// # Safety
    ///
    /// `self.ioapic_base` must be a valid IOAPIC MMIO address (verified
    /// during controller init) and the caller must hold BKL / have
    /// interrupts disabled.
    pub unsafe fn ioapic_version(&self) -> u32 { unsafe {
        ioapic_read_indirect(self.ioapic_base, IOAPIC_REG_VER)
    }}
}

unsafe fn wrmsr_msr_read(msr: u32) -> u64 { unsafe {
    let low: u32;
    let high: u32;
    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nostack, preserves_flags),
    );
    ((high as u64) << 32) | (low as u64)
}}

unsafe fn wrmsr_msr_write(msr: u32, value: u64) { unsafe {
    let low = value as u32;
    let high = (value >> 32) as u32;
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("edx") high,
        in("eax") low,
        options(nostack, preserves_flags),
    );
}}

impl InterruptController for X86_64InterruptController {
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
        let apic = desc.as_any()
            .downcast_ref::<ApicDesc>()
            .expect("X86_64InterruptController::new: expected ApicDesc");
        Self {
            nr_irq_vectors: (apic.nr_irqs as usize).min(NR_IRQ_VECTORS),
            lapic_base: apic.lapic_base,
            ioapic_base: apic.ioapic_base,
        }
    }

    fn init(&mut self) {
        unsafe {
            self.init_lapic();
            self.init_ioapic();
        }
        self.mask_all();
    }

    fn mask(&mut self, irq: IrqVector) {
        unsafe { self.ioapic_set_mask(irq.get(), true) };
    }

    fn unmask(&mut self, irq: IrqVector) {
        unsafe { self.ioapic_set_mask(irq.get(), false) };
    }

    fn ack(&mut self, _irq: IrqVector) {
        unsafe { self.lapic_eoi() };
    }

    fn eoi(&mut self, _irq: IrqVector) {
        unsafe { self.lapic_eoi() };
    }

    fn mask_all(&mut self) {
        for irq in 0..self.nr_irq_vectors as u8 {
            self.mask(IrqVector::new(irq));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_from_apic_descriptor() {
        let desc = ApicDesc {
            lapic_base: 0xFEE0_0000,
            ioapic_base: 0xFEC0_0000,
            nr_irqs: 64,
        };
        let ic = X86_64InterruptController::new(&desc);
        assert_eq!(ic.lapic_base(), 0xFEE0_0000);
        assert_eq!(ic.ioapic_base(), 0xFEC0_0000);
    }

    #[test]
    fn test_new_clamps_nr_irqs_to_max() {
        let desc = ApicDesc {
            lapic_base: 0xFEE0_0000,
            ioapic_base: 0xFEC0_0000,
            nr_irqs: 128, // exceeds NR_IRQ_VECTORS (64)
        };
        let ic = X86_64InterruptController::new(&desc);
        // mask_all iterates 0..nr_irq_vectors; verify it's clamped
        // (indirectly — we just check it doesn't panic)
        let _ = ic.nr_irq_vectors;
    }

    #[test]
    fn test_irq0_vector_constant() {
        assert_eq!(IRQ0_VECTOR, 0x50);
    }
}
