//! x86-64 trap entry implementation
//!
//! Implements `TrapEntryArch` for x86-64, managing the IDT (Interrupt
//! Descriptor Table) and SYSCALL MSR configuration.
//!
//! # 64-bit long mode changes from 32-bit (see 04-protection.md §3.4)
//!
//! - IDT gate descriptors are 16 bytes (vs 8 bytes in 32-bit mode)
//! - IST field added to gate descriptors (Interrupt Stack Table)
//! - SYSENTER removed — only SYSCALL/SYSRET in 64-bit mode

use crate::protection::InterruptVector;
use crate::trap_entry::TrapEntryArch;
use crate::x86_64::protection::{KERN_CS_SELECTOR, USER_CS_SELECTOR};
use minix_types::VirBytes;

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct IdtEntry64 {
    offset_low: u16,
    selector: u16,
    ist: u8,
    p_dpl_type: u8,
    offset_mid: u16,
    offset_high: u32,
    _reserved: u32,
}

impl IdtEntry64 {
    const fn empty() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            p_dpl_type: 0,
            offset_mid: 0,
            offset_high: 0,
            _reserved: 0,
        }
    }

    fn new(handler: u64, selector: u16, dpl: u8, ist: u8, is_trap: bool) -> Self {
        let type_bits = if is_trap { 0xF } else { 0xE };
        Self {
            offset_low: (handler & 0xFFFF) as u16,
            selector,
            ist: ist & 0x7,
            p_dpl_type: 0x80 | ((dpl & 0x3) << 5) | type_bits,
            offset_mid: ((handler >> 16) & 0xFFFF) as u16,
            offset_high: ((handler >> 32) & 0xFFFFFFFF) as u32,
            _reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct IdtPtr {
    limit: u16,
    base: u64,
}

const IDT_ENTRIES: usize = 256;

pub struct X86_64TrapEntry {
    idt: [IdtEntry64; IDT_ENTRIES],
}

impl X86_64TrapEntry {
    fn set_gate(
        &mut self,
        vector: u8,
        handler: u64,
        dpl: u8,
        ist: u8,
        is_trap: bool,
    ) {
        self.idt[vector as usize] = IdtEntry64::new(
            handler,
            KERN_CS_SELECTOR,
            dpl,
            ist,
            is_trap,
        );
    }
}

/// Write a 64-bit value to a Model-Specific Register.
///
/// # Safety
///
/// - `msr` must be a valid MSR index for the current CPU.
/// - Writing to certain MSRs can change CPU behavior (e.g., enabling
///   features, changing entry points). The caller must ensure the
///   write is appropriate for the current CPU state.
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("edx") high,
        in("eax") low,
        options(nostack, preserves_flags),
    );
}

/// Read a 64-bit value from a Model-Specific Register.
///
/// # Safety
///
/// - `msr` must be a valid MSR index for the current CPU.
///   Reading an invalid MSR raises a #GP exception.
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nostack, preserves_flags),
    );
    ((high as u64) << 32) | (low as u64)
}

impl TrapEntryArch for X86_64TrapEntry {
    fn init() -> Self {
        let mut entry = Self {
            idt: [IdtEntry64::empty(); IDT_ENTRIES],
        };

        // TODO: handler addresses are placeholder 0 — must be set to actual
        // trap handler entry points before load() is called. The DPL and IST
        // values here match the C gate_table_exceptions[] / gate_table_pic[]
        // configuration (protect.c:107-152).
        entry.set_gate(0, 0, 0, 0, true);
        entry.set_gate(1, 0, 0, 0, true);
        entry.set_gate(2, 0, 0, 1, false);
        entry.set_gate(3, 0, 3, 0, true);
        entry.set_gate(4, 0, 3, 0, true);
        entry.set_gate(5, 0, 0, 0, false);
        entry.set_gate(6, 0, 0, 0, false);
        entry.set_gate(7, 0, 0, 0, false);
        entry.set_gate(8, 0, 0, 2, false);
        entry.set_gate(10, 0, 0, 0, false);
        entry.set_gate(11, 0, 0, 0, false);
        entry.set_gate(12, 0, 0, 0, false);
        entry.set_gate(13, 0, 0, 0, false);
        entry.set_gate(14, 0, 0, 0, false);

        entry.set_gate(32, 0, 3, 0, true);
        entry.set_gate(33, 0, 3, 0, true);

        entry
    }

    fn configure_syscall(&mut self, entry_point: VirBytes) {
        unsafe {
            // STAR[32:47] = SYSCALL CS (KERN_CS_SELECTOR)
            // STAR[48:63] = SYSRET CS (USER_CS_SELECTOR)
            // C: AMD_MSR_STAR — archconst.h:170
            //   32-bit: ((USER_CS_SELECTOR << 16) | KERN_CS_SELECTOR) in low 32 bits
            //   64-bit: KERN_CS_SELECTOR << 32 | USER_CS_SELECTOR << 48
            let star = (KERN_CS_SELECTOR as u64) << 32
                     | (USER_CS_SELECTOR as u64) << 48;
            wrmsr(0xC0000081, star);

            wrmsr(0xC0000082, entry_point.get());

            wrmsr(0xC0000084, 0x200);

            let efer = rdmsr(0xC0000080);
            wrmsr(0xC0000080, efer | 1);
        }
    }

    fn load(&self) {
        let idtr = IdtPtr {
            limit: (core::mem::size_of_val(&self.idt) - 1) as u16,
            base: self.idt.as_ptr() as u64,
        };

        unsafe {
            core::arch::asm!(
                "lidt [{idtr}]",
                idtr = in(reg) &idtr,
                options(readonly, nostack, preserves_flags)
            );
        }
    }

    fn load_ap(&self) {
        self.load();
    }

    fn set_handler(
        &mut self,
        vector: InterruptVector,
        handler: VirBytes,
        user_accessible: bool,
    ) {
        let dpl = if user_accessible { 3 } else { 0 };
        self.set_gate(vector.get(), handler.get(), dpl, 0, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idt_entry64_size_is_16_bytes() {
        assert_eq!(
            core::mem::size_of::<IdtEntry64>(),
            16,
            "IdtEntry64 must be 16 bytes per Intel SDM Vol. 3A §6.14.1"
        );
    }

    #[test]
    fn idt_ptr_size_is_10_bytes() {
        assert_eq!(
            core::mem::size_of::<IdtPtr>(),
            10,
            "IdtPtr must be 10 bytes: 2-byte limit + 8-byte base"
        );
    }

    #[test]
    fn star_register_value_correct() {
        let star = (KERN_CS_SELECTOR as u64) << 32
                 | (USER_CS_SELECTOR as u64) << 48;
        assert_eq!(star & 0xFFFF_0000_0000_0000, 0x001B_0000_0000_0000, "STAR[48:63] = USER_CS_SELECTOR");
        assert_eq!(star & 0x0000_FFFF_0000_0000, 0x0000_0008_0000_0000, "STAR[32:47] = KERN_CS_SELECTOR");
    }
}
