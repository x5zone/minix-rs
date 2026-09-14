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

/// The boot clock source's IRQ vector: the 8254 PIT output is IOAPIC
/// input 0, so the timer hook registers under IRQ 0.
///
/// C: CLOCK_IRQ = 0 (arch/i386); the timer hook is installed on this
/// vector by `put_irq_handler` (arch_clock.c:190) and the line is
/// unmasked by the first-handler rule (interrupt.c:65).
pub const TIMER_IRQ: IrqVector = IrqVector::new(0);

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
/// LVT (Local Vector Table) register offsets — Intel SDM Vol. 3 §10.5.
const LAPIC_REG_LVT_TIMER: usize = 0x320;
const LAPIC_REG_LVT_LINT0: usize = 0x350;
const LAPIC_REG_LVT_LINT1: usize = 0x360;
const LAPIC_REG_LVT_ERROR: usize = 0x370;
/// LVT bit 16: mask — the entry delivers nothing while set.
const LAPIC_LVT_MASK: u32 = 1 << 16;
/// Bit 8 of SVR: APIC software enable.
const LAPIC_SVR_ENABLE: u32 = 1 << 8;

/// IOAPIC register offsets from the IOAPIC base.
const IOAPIC_REG_IOREGSEL: usize = 0x00;
const IOAPIC_REG_IOWIN: usize = 0x10;
/// IOAPIC indirect register indices.
const IOAPIC_REG_VER: u32 = 0x01;
/// First redirection entry (low half).
const IOAPIC_REG_REDTBL_BASE: u32 = 0x10;
/// IOAPIC redirection-entry count on the QEMU pc machine (and the classic
/// 82093): 24 inputs, 16 ISA + 8 PCI-level.
const IOAPIC_NUM_PINS: u8 = 24;
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
        // Mask every LVT entry (C apic.c init parity: all LVTs masked until
        // their owner opens them — the LAPIC timer gate is S-3c/D-59
        // territory; the LINT lines must not inherit firmware state). OVMF
        // leaves LINT0 in ExtINT mode routing the 8259 PIC into the LAPIC,
        // so an unmasked LINT0 delivers leftover PIC vectors after `sti`.
        for lvt in [LAPIC_REG_LVT_TIMER, LAPIC_REG_LVT_LINT0, LAPIC_REG_LVT_LINT1, LAPIC_REG_LVT_ERROR] {
            lapic_write(self.lapic_base, lvt, LAPIC_LVT_MASK);
        }
    }}

    unsafe fn init_ioapic(&mut self) { unsafe {
        for pin in 0..IOAPIC_NUM_PINS {
            self.ioapic_route_and_mask(pin);
        }
    }}

    /// Program redirection entry `pin` with its C-parity delivery vector and
    /// keep the line masked.
    ///
    /// S-8 (2026-09-14): QEMU/hardware reset leaves every RTE with vector 0
    /// and the mask bit set. The first-handler unmask rule
    /// (`IrqManager::register_hook` → `unmask`) only toggles the mask bit,
    /// so a line unmasked without a programmed vector would deliver to IDT
    /// vector 0 (#DE) — the timer interrupt would hit the divide-error gate.
    /// C programs the same mapping when the APIC is initialized
    /// (apic.c RTE setup, VECTOR(irq) = 0x50+irq / 0x70+irq-8). Pins ≥ 16
    /// (QEMU's extra IOAPIC inputs) stay masked with the reset vector 0 —
    /// no handler exists for them and nothing unmasks them.
    unsafe fn ioapic_route_and_mask(&mut self, pin: u8) { unsafe {
        // Inverse of isa_irq_to_pin: which kernel IRQ line this pin carries.
        let irq = match pin {
            0 => Some(2u8), // ISA cascade enters the IOAPIC at pin 0
            2 => Some(0u8), // ISA IRQ0 (PIT) enters at pin 2
            i @ 1..=15 => Some(i),
            _ => None,      // PCI-level pins: no kernel IRQ number, masked
        };
        let vector = match irq {
            Some(i @ 0..=7) => IRQ0_VECTOR + i,
            Some(i @ 8..=15) => IRQ0_VECTOR + 0x20 + (i - 8),
            _ => 0, // masked, never delivered
        };
        let entry = (vector as u64) | (IOAPIC_REDTBL_MASK as u64);
        ioapic_write_redtbl(self.ioapic_base, pin, entry);
    }}

    /// ISA IRQ number → IOAPIC input pin — the classic ISA override present
    /// on the pc machine and real chipsets alike: the PIT (ISA IRQ0) enters
    /// the IOAPIC at pin 2, and the slave-cascade (ISA IRQ2) at pin 0;
    /// every other line is identity. The kernel addresses lines by ISA IRQ
    /// number (C: CLOCK_IRQ=0; IrqVector), the IOAPIC RTEs by pin — every
    /// RTE access must go through this map or the timer line stays masked
    /// while RTE 0 is (wrongly) opened.
    const fn isa_irq_to_pin(irq: u8) -> u8 {
        match irq {
            0 => 2,
            2 => 0,
            other => other,
        }
    }

    unsafe fn ioapic_set_mask(&mut self, irq: u8, mask: bool) { unsafe {
        let pin = Self::isa_irq_to_pin(irq);
        let mut entry = ioapic_read_redtbl(self.ioapic_base, pin);
        if mask {
            entry |= IOAPIC_REDTBL_MASK as u64;
        } else {
            entry &= !(IOAPIC_REDTBL_MASK as u64);
        }
        ioapic_write_redtbl(self.ioapic_base, pin, entry);
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
        // The x86 APIC has no claim step: unlike GIC (read ICC_IAR1_EL1)
        // or PLIC (read claim), nothing must be read before the handler
        // runs. The LAPIC EOI register is the *completion* only — writing
        // it here (the pre-D-61 implementation) would send the EOI before
        // the handler and re-open the interrupt early.
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
        // nr_irqs above the platform max must be clamped so mask_all's
        // 0..nr_irq_vectors loop stays within the IDT vector space.
        assert_eq!(ic.nr_irq_vectors, NR_IRQ_VECTORS);
    }

    #[test]
    fn test_timer_irq_is_ioapic_input_zero() {
        // D-59: the boot clock source (8254 PIT) delivers through IOAPIC
        // input 0, so the boot-end hook registration must target vector 0.
        // Pinning the value here keeps the kernel-side boot chain honest:
        // if the clock source ever moves to the LAPIC timer, this pin must
        // be revisited together with TimerIrqGate's x86 semantics.
        assert_eq!(TIMER_IRQ.get(), 0);
    }
}
