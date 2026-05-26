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

use crate::interrupt::{InterruptController, IrqVector, NR_IRQ_VECTORS};

/// x86-64 IRQ-to-IDT vector offset.
///
/// Hardware IRQ 0 maps to IDT vector 0x50 (80).
/// C: IRQ0_VECTOR — interrupt.h:34
pub const IRQ0_VECTOR: u8 = 0x50;

/// x86-64 APIC-based interrupt controller.
///
/// Combines Local APIC (per-CPU) and I/O APIC (system-wide) operations.
///
/// C: i8259.c (PIC) / apic.c (APIC)
pub struct X86_64InterruptController {
    nr_irq_vectors: usize,
}

impl X86_64InterruptController {
    pub const fn new() -> Self {
        Self {
            nr_irq_vectors: NR_IRQ_VECTORS,
        }
    }

    fn init_lapic(&mut self) {
        // TODO: Initialize Local APIC (set spurious interrupt vector,
        // enable LAPIC, set up timer, error vector, etc.)
        // C: lapic_enable() — apic.c
    }

    fn init_ioapic(&mut self) {
        // TODO: Initialize I/O APIC (set redirection table entries,
        // mask all pins, configure delivery mode)
        // C: ioapic_init() — apic.c
    }

    fn ioapic_set_mask(&mut self, irq: u8, mask: bool) {
        // TODO: Write IOAPIC redirection table entry mask bit
        // C: ioapic_mask_irq() / ioapic_unmask_irq() — apic.c
        let _ = (irq, mask);
    }

    fn lapic_eoi(&mut self) {
        // TODO: Write to LAPIC EOI register (0x0B)
        // C: lapic_eoi() — apic.c
    }
}

impl InterruptController for X86_64InterruptController {
    fn init(&mut self) {
        // C: intr_init() — i8259.c:28
        // 64-bit: Initialize LAPIC + IOAPIC, mask all IRQs
        self.init_lapic();
        self.init_ioapic();
        self.mask_all();
    }

    fn mask(&mut self, irq: IrqVector) {
        // C: ioapic_mask_irq(irq) — apic.c
        self.ioapic_set_mask(irq.get(), true);
    }

    fn unmask(&mut self, irq: IrqVector) {
        // C: ioapic_unmask_irq(irq) — apic.c
        self.ioapic_set_mask(irq.get(), false);
    }

    fn ack(&mut self, _irq: IrqVector) {
        self.lapic_eoi();
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // C: ioapic_eoi(irq) / lapic_eoi() — apic.c
        self.lapic_eoi();
    }

    fn mask_all(&mut self) {
        // C: hw_intr_disable_all() — hw_intr.h:33
        for irq in 0..self.nr_irq_vectors as u8 {
            self.mask(IrqVector::new(irq));
        }
    }
}
