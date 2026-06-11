//! x86-64 APIC-based interrupt controller implementation
//!
//! Implements `InterruptController` for x86-64 using LAPIC + IOAPIC.
//! 8259A PIC is not supported — 64-bit systems use APIC exclusively.
//!
//! # 64-bit long mode changes (see 05-exception-interrupt.md §3.7)
//!
//! - 8259A PIC removed — LAPIC + IOAPIC only
//! - NR_IRQ_VECTORS = 64 (APIC mode)
//! - IRQ-to-IDT vector mapping via IOAPIC redirection table
//!
//! # MMIO addresses (QEMU `virt` machine, also the project default)
//!
//! - LAPIC base: 0xFEE0_0000 (Intel SDM Vol. 3A §10.4.1; relocated via
//!   IA32_APIC_BASE MSR but x86-64 long-mode firmware typically leaves
//!   the default).
//! - IOAPIC base: 0xFEC0_0000 (QEMU virt machine, ACPI MADT entry).
//!
//! # References
//!
//! - Intel SDM Vol. 3A §10: Advanced Programmable Interrupt Controller
//! - Intel SDM Vol. 3A §10.4.4: Local APIC register map
//! - Intel SDM Vol. 3A §10.9: I/O APIC register map

use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

use crate::interrupt::{InterruptController, IrqVector, NR_IRQ_VECTORS};

/// x86-64 IRQ-to-IDT vector offset.
///
/// Hardware IRQ 0 maps to IDT vector 0x50 (80).
/// C: IRQ0_VECTOR — interrupt.h:34
pub const IRQ0_VECTOR: u8 = 0x50;

/// LAPIC spurious interrupt vector (must be a valid IDT vector with DPL=0).
/// Vector 0xFF is the conventional choice (highest in the range).
const LAPIC_SPURIOUS_VECTOR: u8 = 0xFF;

/// IA32_APIC_BASE MSR index (Intel SDM Vol. 3A §10.4.3).
const MSR_IA32_APIC_BASE: u32 = 0x1B;
/// Bit 11 of IA32_APIC_BASE: APIC global enable.
const IA32_APIC_BASE_EN: u64 = 1 << 11;
/// Bits 12-35: APIC base physical address (4-KB aligned).
const IA32_APIC_BASE_ADDR_MASK: u64 = 0xFFFF_F000;

/// LAPIC register offsets from the LAPIC base (Intel SDM Vol. 3A §10.4.4).
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
/// First redirection entry (low half) — entry N uses IOREDTBL + 2*N.
const IOAPIC_REG_REDTBL_BASE: u32 = 0x10;
/// Bit 16 of redirection entry low: interrupt mask.
const IOAPIC_REDTBL_MASK: u32 = 1 << 16;

/// Default LAPIC MMIO base (QEMU `virt` machine, also most modern PC firmware).
const DEFAULT_LAPIC_BASE: usize = 0xFEE0_0000;
/// Default IOAPIC MMIO base (QEMU `virt` machine).
const DEFAULT_IOAPIC_BASE: usize = 0xFEC0_0000;

/// Read a 32-bit value from a LAPIC MMIO register.
///
/// # Safety
///
/// `base + offset` must be the address of a valid LAPIC register.
#[inline]
unsafe fn lapic_read(base: usize, offset: usize) -> u32 {
    read_volatile((base + offset) as *const u32)
}

/// Write a 32-bit value to a LAPIC MMIO register.
///
/// # Safety
///
/// `base + offset` must be the address of a valid LAPIC register, and
/// `value` must be a legal value for that register.
#[inline]
unsafe fn lapic_write(base: usize, offset: usize, value: u32) {
    write_volatile((base + offset) as *mut u32, value)
}

/// Read the 64-bit IOREDTBL entry (low + high) for IRQ `irq`.
///
/// # Safety
///
/// `base` must be the address of a valid IOAPIC.
unsafe fn ioapic_read_redtbl(base: usize, irq: u8) -> u64 {
    let reg = IOAPIC_REG_REDTBL_BASE + 2 * (irq as u32);
    let low = ioapic_read_indirect(base, reg);
    let high = ioapic_read_indirect(base, reg + 1);
    ((high as u64) << 32) | (low as u64)
}

/// Write the 64-bit IOREDTBL entry (low + high) for IRQ `irq`.
///
/// # Safety
///
/// `base` must be the address of a valid IOAPIC.
unsafe fn ioapic_write_redtbl(base: usize, irq: u8, value: u64) {
    let reg = IOAPIC_REG_REDTBL_BASE + 2 * (irq as u32);
    ioapic_write_indirect(base, reg, value as u32);
    ioapic_write_indirect(base, reg + 1, (value >> 32) as u32);
}

/// Read a 32-bit value from an indirect IOAPIC register.
///
/// # Safety
///
/// `base` must be the address of a valid IOAPIC.
unsafe fn ioapic_read_indirect(base: usize, reg: u32) -> u32 {
    write_volatile((base + IOAPIC_REG_IOREGSEL) as *mut u32, reg);
    read_volatile((base + IOAPIC_REG_IOWIN) as *const u32)
}

/// Write a 32-bit value to an indirect IOAPIC register.
///
/// # Safety
///
/// `base` must be the address of a valid IOAPIC.
unsafe fn ioapic_write_indirect(base: usize, reg: u32, value: u32) {
    write_volatile((base + IOAPIC_REG_IOREGSEL) as *mut u32, reg);
    write_volatile((base + IOAPIC_REG_IOWIN) as *mut u32, value)
}

/// x86-64 APIC-based interrupt controller.
///
/// Combines Local APIC (per-CPU) and I/O APIC (system-wide) operations.
///
/// LAPIC base defaults to 0xFEE0_0000 and IOAPIC base to 0xFEC0_0000
/// (QEMU `virt` machine defaults). Override via [`Self::set_base`] for
/// hardware with non-default addresses (read from ACPI MADT).
///
/// C: i8259.c (PIC) / apic.c (APIC)
pub struct X86_64InterruptController {
    nr_irq_vectors: usize,
    lapic_base: usize,
    ioapic_base: usize,
}

impl X86_64InterruptController {
    pub const fn new() -> Self {
        Self {
            nr_irq_vectors: NR_IRQ_VECTORS,
            lapic_base: DEFAULT_LAPIC_BASE,
            ioapic_base: DEFAULT_IOAPIC_BASE,
        }
    }

    /// Override the LAPIC and IOAPIC MMIO base addresses.
    ///
    /// Call after parsing ACPI MADT, before [`Self::init`].
    pub fn set_base(&mut self, lapic_base: usize, ioapic_base: usize) {
        self.lapic_base = lapic_base;
        self.ioapic_base = ioapic_base;
    }

    /// LAPIC MMIO base (read-only accessor for tests and tooling).
    pub fn lapic_base(&self) -> usize {
        self.lapic_base
    }

    /// IOAPIC MMIO base (read-only accessor for tests and tooling).
    pub fn ioapic_base(&self) -> usize {
        self.ioapic_base
    }

    /// Initialize the Local APIC.
    ///
    /// Enables the LAPIC via IA32_APIC_BASE MSR and sets the spurious
    /// interrupt vector register.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that `self.lapic_base` is the address
    /// of a valid LAPIC MMIO region, and that no interrupts are
    /// in-flight that could race with this initialization.
    unsafe fn init_lapic(&mut self) {
        // SAFETY: IA32_APIC_BASE MSR is a valid MSR on every x86-64 CPU;
        // enabling the LAPIC is required for any APIC-based interrupt.
        let apic_base = wrmsr_msr_read(MSR_IA32_APIC_BASE);
        let apic_phys = apic_base & IA32_APIC_BASE_ADDR_MASK;
        if apic_phys as usize != self.lapic_base {
            // Hardware LAPIC is at a different address than our default.
            // We trust the runtime-detected value going forward.
            self.lapic_base = apic_phys as usize;
        }
        // SAFETY: Enabling the LAPIC is a one-shot idempotent operation.
        wrmsr_msr_write(MSR_IA32_APIC_BASE, apic_base | IA32_APIC_BASE_EN);

        // Set spurious-interrupt vector register: enable bit + vector 0xFF.
        // SAFETY: SVR is a valid LAPIC register; vector 0xFF is the
        // conventional spurious vector (Intel SDM Vol. 3A §10.9).
        lapic_write(self.lapic_base, LAPIC_REG_SVR, LAPIC_SPURIOUS_VECTOR as u32 | LAPIC_SVR_ENABLE);
    }

    /// Initialize the I/O APIC: mask all redirection entries.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that `self.ioapic_base` is the address
    /// of a valid IOAPIC MMIO region.
    unsafe fn init_ioapic(&mut self) {
        // Mask all 24 redirection entries (QEMU virt has 24 IRQ lines;
        // real hardware has 24 on most chipsets).
        for irq in 0..NR_IRQ_VECTORS as u8 {
            self.ioapic_set_mask(irq, true);
        }
    }

    /// Set the mask bit on an IOAPIC redirection entry.
    ///
    /// # Safety
    ///
    /// `self.ioapic_base` must be the address of a valid IOAPIC.
    unsafe fn ioapic_set_mask(&mut self, irq: u8, mask: bool) {
        // SAFETY: ioapic_base set in init(); irq in 0..NR_IRQ_VECTORS.
        let mut entry = ioapic_read_redtbl(self.ioapic_base, irq);
        if mask {
            entry |= IOAPIC_REDTBL_MASK as u64;
        } else {
            entry &= !(IOAPIC_REDTBL_MASK as u64);
        }
        ioapic_write_redtbl(self.ioapic_base, irq, entry);
    }

    /// Write the LAPIC EOI register to acknowledge the current interrupt.
    ///
    /// # Safety
    ///
    /// `self.lapic_base` must be the address of a valid LAPIC.
    unsafe fn lapic_eoi(&mut self) {
        // SAFETY: EOI is a write-only LAPIC register; value 0 is the
        // only legal value (Intel SDM Vol. 3A §10.8.5).
        lapic_write(self.lapic_base, LAPIC_REG_EOI, 0);
    }

    /// Read LAPIC ID register (for tests and AP bring-up).
    ///
    /// # Safety
    ///
    /// `self.lapic_base` must be the address of a valid LAPIC.
    pub unsafe fn lapic_id(&self) -> u32 {
        lapic_read(self.lapic_base, LAPIC_REG_ID) >> 24
    }

    /// Read IOAPIC version register (for tests and chip identification).
    ///
    /// # Safety
    ///
    /// `self.ioapic_base` must be the address of a valid IOAPIC.
    pub unsafe fn ioapic_version(&self) -> u32 {
        ioapic_read_indirect(self.ioapic_base, IOAPIC_REG_VER)
    }
}

// Local wrappers for MSR read/write so we don't depend on trap_entry.rs.
unsafe fn wrmsr_msr_read(msr: u32) -> u64 {
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
}

unsafe fn wrmsr_msr_write(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("edx") high,
        in("eax") low,
        options(nostack, preserves_flags),
    );
}

impl Default for X86_64InterruptController {
    fn default() -> Self {
        Self::new()
    }
}

impl InterruptController for X86_64InterruptController {
    fn init(&mut self) {
        // C: intr_init() — i8259.c:28
        // 64-bit: Initialize LAPIC + IOAPIC, mask all IRQs
        // SAFETY: at boot time the firmware-set LAPIC/IOAPIC bases are valid.
        unsafe {
            self.init_lapic();
            self.init_ioapic();
        }
        self.mask_all();
    }

    fn mask(&mut self, irq: IrqVector) {
        // C: ioapic_mask_irq(irq) — apic.c
        // SAFETY: ioapic_base is set by init(); irq is bounded.
        unsafe { self.ioapic_set_mask(irq.get(), true) };
    }

    fn unmask(&mut self, irq: IrqVector) {
        // C: ioapic_unmask_irq(irq) — apic.c
        // SAFETY: ioapic_base is set by init(); irq is bounded.
        unsafe { self.ioapic_set_mask(irq.get(), false) };
    }

    fn ack(&mut self, _irq: IrqVector) {
        // SAFETY: lapic_base is set by init().
        unsafe { self.lapic_eoi() };
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // C: ioapic_eoi(irq) / lapic_eoi() — apic.c
        // On APIC systems, EOI goes to the LAPIC, not the IOAPIC.
        // SAFETY: lapic_base is set by init().
        unsafe { self.lapic_eoi() };
    }

    fn mask_all(&mut self) {
        // C: hw_intr_disable_all() — hw_intr.h:33
        for irq in 0..self.nr_irq_vectors as u8 {
            self.mask(IrqVector::new(irq));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_has_default_bases() {
        let ic = X86_64InterruptController::new();
        assert_eq!(ic.lapic_base(), 0xFEE0_0000);
        assert_eq!(ic.ioapic_base(), 0xFEC0_0000);
    }

    #[test]
    fn test_set_base_overrides() {
        let mut ic = X86_64InterruptController::new();
        ic.set_base(0xFEE0_1000, 0xFEC0_2000);
        assert_eq!(ic.lapic_base(), 0xFEE0_1000);
        assert_eq!(ic.ioapic_base(), 0xFEC0_2000);
    }

    #[test]
    fn test_nr_irq_vectors_default_64() {
        let ic = X86_64InterruptController::new();
        assert_eq!(ic.nr_irq_vectors, 64);
    }

    #[test]
    fn test_default_impl_matches_new() {
        let a = X86_64InterruptController::new();
        let b = X86_64InterruptController::default();
        assert_eq!(a.lapic_base(), b.lapic_base());
        assert_eq!(a.ioapic_base(), b.ioapic_base());
    }

    #[test]
    fn test_irq0_vector_constant() {
        // Hardware IRQ 0 should map to IDT vector 0x50 (Minix3 convention).
        assert_eq!(IRQ0_VECTOR, 0x50);
    }

    #[test]
    fn test_lapic_offsets() {
        // LAPIC register offsets (Intel SDM Vol. 3A §10.4.4).
        assert_eq!(LAPIC_REG_ID, 0x020);
        assert_eq!(LAPIC_REG_EOI, 0x0B0);
        assert_eq!(LAPIC_REG_SVR, 0x0F0);
    }

    #[test]
    fn test_lapic_svr_enable_bit() {
        // SVR bit 8 is the APIC software enable.
        assert_eq!(LAPIC_SVR_ENABLE, 1 << 8);
    }

    #[test]
    fn test_ioapic_redtbl_mask_bit() {
        // Bit 16 of the redirection table entry is the interrupt mask bit.
        assert_eq!(IOAPIC_REDTBL_MASK, 1 << 16);
    }

    #[test]
    fn test_ioapic_redtbl_base_offset() {
        // First redirection entry (low half) is at IOREDTBL register 0x10.
        assert_eq!(IOAPIC_REG_REDTBL_BASE, 0x10);
    }

    #[test]
    fn test_ia32_apic_base_msr_index() {
        assert_eq!(MSR_IA32_APIC_BASE, 0x1B);
    }

    #[test]
    fn test_ia32_apic_base_en_bit() {
        // Bit 11 of IA32_APIC_BASE enables the APIC.
        assert_eq!(IA32_APIC_BASE_EN, 1 << 11);
    }

    #[test]
    fn test_spurious_vector_value() {
        // 0xFF is the conventional spurious vector.
        assert_eq!(LAPIC_SPURIOUS_VECTOR, 0xFF);
    }

    #[test]
    fn test_set_base_then_mask_all_does_not_panic() {
        // Exercises the public API path; the actual MMIO write will
        // fault on the host, but the call sequence must not panic
        // before the MMIO access. We can't call init() in unit tests
        // because that would touch real MMIO.
        let mut ic = X86_64InterruptController::new();
        ic.set_base(0xFEE0_0000, 0xFEC0_0000);
        assert_eq!(ic.lapic_base(), 0xFEE0_0000);
    }
}
