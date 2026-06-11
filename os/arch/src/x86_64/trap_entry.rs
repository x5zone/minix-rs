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

use crate::trap_entry::{TrapEntryArch, InterruptVector};
use crate::x86_64::protection::{KERN_CS_SELECTOR, USER_CS_SELECTOR};
use minix_types::VirBytes;

// IDT gate type encoding (p_dpl_type field bits 0-3)
// Intel SDM Vol. 3A §6.14.1: IDT Gate Descriptors
const GATE_TYPE_INTERRUPT: u8 = 0xE; // Interrupt gate (IF auto-cleared)
const GATE_TYPE_TRAP: u8 = 0xF;      // Trap gate (IF not modified)
const GATE_PRESENT: u8 = 0x80;       // Gate descriptor present bit

// MSR addresses for SYSCALL/SYSRET configuration
// AMD64 Architecture Programmer's Manual Vol. 2 §4.1.5
const MSR_STAR: u32 = 0xC0000081;    // SYSRET/SYSCALL CS and SS
const MSR_LSTAR: u32 = 0xC0000082;   // SYSCALL entry point (RIP)
const MSR_SFMASK: u32 = 0xC0000084;  // SYSCALL RFLAGS mask
const MSR_EFER: u32 = 0xC0000080;    // Extended Feature Enable Register

// EFER bit definitions
const EFER_SCE: u64 = 0x1;           // System Call Enable (bit 0)

// SFMASK: bits set in this mask are cleared in RFLAGS on SYSCALL.
// 0x200 = bit 9 = IF (Interrupt Flag) — SYSCALL clears IF by default.
const SFMASK_CLEAR_IF: u64 = 0x200;

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
        let type_bits = if is_trap { GATE_TYPE_TRAP } else { GATE_TYPE_INTERRUPT };
        Self {
            offset_low: (handler & 0xFFFF) as u16,
            selector,
            ist: ist & 0x7,
            p_dpl_type: GATE_PRESENT | ((dpl & 0x3) << 5) | type_bits,
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
        // SAFETY: MSR writes are safe because:
        // - MSR_STAR/LSTAR/SFMASK/EFER are model-specific registers
        //   defined by AMD64 Architecture for SYSCALL configuration.
        // - We are in kernel mode (CPL=0), which is required for WRMSR.
        // - The values written are architecturally correct:
        //   STAR: valid segment selectors in the GDT.
        //   LSTAR: entry point provided by caller (kernel syscall handler).
        //   SFMASK: only bit 9 (IF) is set, matching C behavior.
        //   EFER: only SCE bit is set, enabling SYSCALL/SYSRET.
        unsafe {
            // STAR[32:47] = SYSCALL CS (KERN_CS_SELECTOR)
            // STAR[48:63] = SYSRET CS (USER_CS_SELECTOR)
            // C: AMD_MSR_STAR — archconst.h:170
            //   64-bit: KERN_CS_SELECTOR << 32 | USER_CS_SELECTOR << 48
            let star = (KERN_CS_SELECTOR as u64) << 32
                     | (USER_CS_SELECTOR as u64) << 48;
            wrmsr(MSR_STAR, star);

            // LSTAR = SYSCALL entry point (RIP on SYSCALL)
            wrmsr(MSR_LSTAR, entry_point.get());

            // SFMASK: bits set here are cleared in RFLAGS on SYSCALL.
            // SFMASK_CLEAR_IF (bit 9) clears IF — SYSCALL disables
            // interrupts by default, matching C behavior.
            wrmsr(MSR_SFMASK, SFMASK_CLEAR_IF);

            // Enable SYSCALL/SYSRET by setting EFER.SCE (bit 0).
            // C: msr_lo |= AMD_EFER_SCE — protect.c:325
            let efer = rdmsr(MSR_EFER);
            wrmsr(MSR_EFER, efer | EFER_SCE);
        }
    }

    fn load(&self) {
        let idtr = IdtPtr {
            limit: (core::mem::size_of_val(&self.idt) - 1) as u16,
            base: self.idt.as_ptr() as u64,
        };

        // SAFETY: LIDT is safe because:
        // - idtr points to a valid IdtPtr on the stack with correct
        //   limit (IDT size - 1) and base (IDT array address).
        // - self.idt contains a valid IDT initialized by init().
        // - We are in kernel mode (CPL=0), which is required for LIDT.
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
    use crate::trap_entry::{PAGE_FAULT, DOUBLE_FAULT};
    use crate::x86_64::protection::{KERN_DS_SELECTOR, USER_DS_SELECTOR};

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

    #[test]
    fn set_handler_sets_dpl_correctly() {
        let mut entry = X86_64TrapEntry::init();

        // Set a handler with user_accessible=true → DPL=3
        entry.set_handler(
            PAGE_FAULT,
            VirBytes::new(0xDEAD),
            true,
        );
        let pf_idx = PAGE_FAULT.get() as usize;
        let dpl_user = (entry.idt[pf_idx].p_dpl_type >> 5) & 0x3;
        assert_eq!(dpl_user, 3, "User-accessible handler must have DPL=3");

        // Set a handler with user_accessible=false → DPL=0
        entry.set_handler(
            DOUBLE_FAULT,
            VirBytes::new(0xBEEF),
            false,
        );
        let df_idx = DOUBLE_FAULT.get() as usize;
        let dpl_kernel = (entry.idt[df_idx].p_dpl_type >> 5) & 0x3;
        assert_eq!(dpl_kernel, 0, "Kernel-only handler must have DPL=0");
    }

    #[test]
    fn set_handler_writes_handler_address() {
        let mut entry = X86_64TrapEntry::init();
        let handler_addr: u64 = 0xFFFF_8000_0000_1000;
        entry.set_handler(
            PAGE_FAULT,
            VirBytes::new(handler_addr),
            false,
        );
        let pf_idx = PAGE_FAULT.get() as usize;
        let reconstructed = (entry.idt[pf_idx].offset_low as u64)
            | ((entry.idt[pf_idx].offset_mid as u64) << 16)
            | ((entry.idt[pf_idx].offset_high as u64) << 32);
        assert_eq!(reconstructed, handler_addr, "Handler address must be correctly split across IDT entry fields");
    }

    #[test]
    fn gate_type_constants_correct() {
        assert_eq!(GATE_TYPE_INTERRUPT, 0xE, "Interrupt gate type = 0xE");
        assert_eq!(GATE_TYPE_TRAP, 0xF, "Trap gate type = 0xF");
        assert_eq!(GATE_PRESENT, 0x80, "Present bit = 0x80");
    }

    #[test]
    fn msr_constants_correct() {
        assert_eq!(MSR_STAR, 0xC0000081, "STAR MSR address");
        assert_eq!(MSR_LSTAR, 0xC0000082, "LSTAR MSR address");
        assert_eq!(MSR_SFMASK, 0xC0000084, "SFMASK MSR address");
        assert_eq!(MSR_EFER, 0xC0000080, "EFER MSR address");
        assert_eq!(EFER_SCE, 0x1, "EFER.SCE bit");
        assert_eq!(SFMASK_CLEAR_IF, 0x200, "SFMASK IF bit (bit 9)");
    }

    #[test]
    fn star_register_layout() {
        // STAR[32:47] = SYSCALL CS (KERN_CS_SELECTOR = 0x08)
        // STAR[48:63] = SYSRET CS (USER_CS_SELECTOR = 0x1B)
        let star = (KERN_CS_SELECTOR as u64) << 32
                 | (USER_CS_SELECTOR as u64) << 48;
        // SYSCALL: CS = STAR[32:47] + 0, SS = STAR[32:47] + 8
        let syscall_cs = ((star >> 32) & 0xFFFF) as u16;
        let syscall_ss = (syscall_cs + 8) as u16;
        assert_eq!(syscall_cs, KERN_CS_SELECTOR, "SYSCALL CS = KERN_CS_SELECTOR");
        assert_eq!(syscall_ss, KERN_DS_SELECTOR, "SYSCALL SS = KERN_DS_SELECTOR");
        // SYSRET: CS = STAR[48:63] + 0, SS = STAR[48:63] + 8
        let sysret_cs = ((star >> 48) & 0xFFFF) as u16;
        let sysret_ss = (sysret_cs + 8) as u16;
        assert_eq!(sysret_cs, USER_CS_SELECTOR, "SYSRET CS = USER_CS_SELECTOR");
        assert_eq!(sysret_ss, USER_DS_SELECTOR, "SYSRET SS = USER_DS_SELECTOR");
    }

    #[test]
    fn sfmask_clears_if_on_syscall() {
        // SFMASK_CLEAR_IF = 0x200 = bit 9 (IF)
        // On SYSCALL, RFLAGS &= ~SFMASK, so IF is cleared.
        assert_eq!(SFMASK_CLEAR_IF & (1 << 9), 1 << 9, "SFMASK must clear IF (bit 9)");
        // Only IF bit is set in our SFMASK — no other flags are cleared.
        assert_eq!(SFMASK_CLEAR_IF, 1 << 9, "SFMASK should only clear IF");
    }

    #[test]
    fn idt_init_sets_exception_gates() {
        let entry = X86_64TrapEntry::init();
        // Verify that exception vectors are present (p_dpl_type has present bit)
        for &vec in &[0u8, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14] {
            let p = entry.idt[vec as usize].p_dpl_type;
            assert_ne!(p & GATE_PRESENT, 0, "Vector {} must be present", vec);
        }
        // Verify IRQ vectors 32 and 33 are present
        for &vec in &[32u8, 33] {
            let p = entry.idt[vec as usize].p_dpl_type;
            assert_ne!(p & GATE_PRESENT, 0, "Vector {} must be present", vec);
        }
    }

    #[test]
    fn idt_init_breakpoint_has_dpl3() {
        // Breakpoint (INT3, vector 3) must be DPL=3 so user mode can trigger it.
        let entry = X86_64TrapEntry::init();
        let dpl = (entry.idt[3].p_dpl_type >> 5) & 0x3;
        assert_eq!(dpl, 3, "Breakpoint (vector 3) must have DPL=3");
    }

    #[test]
    fn idt_init_overflow_has_dpl3() {
        // Overflow (INTO, vector 4) must be DPL=3 so user mode can trigger it.
        let entry = X86_64TrapEntry::init();
        let dpl = (entry.idt[4].p_dpl_type >> 5) & 0x3;
        assert_eq!(dpl, 3, "Overflow (vector 4) must have DPL=3");
    }

    #[test]
    fn idt_init_double_fault_uses_ist2() {
        // Double fault (vector 8) uses IST=2 per C gate_table_exceptions[].
        let entry = X86_64TrapEntry::init();
        assert_eq!(entry.idt[8].ist, 2, "Double fault must use IST2");
    }

    #[test]
    fn idt_init_nmi_uses_ist1() {
        // NMI (vector 2) uses IST=1 per C gate_table_exceptions[].
        let entry = X86_64TrapEntry::init();
        assert_eq!(entry.idt[2].ist, 1, "NMI must use IST1");
    }

    #[test]
    fn idt_init_kernel_exceptions_have_dpl0() {
        // Most exceptions (0,1,5,6,7,8,10-14) must have DPL=0.
        let entry = X86_64TrapEntry::init();
        for &vec in &[0u8, 1, 5, 6, 7, 10, 11, 12, 13, 14] {
            let dpl = (entry.idt[vec as usize].p_dpl_type >> 5) & 0x3;
            assert_eq!(dpl, 0, "Vector {} must have DPL=0", vec);
        }
    }
}
