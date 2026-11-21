//! x86-64 trap entry implementation
//!
//! Implements `TrapEntryArch` for x86-64, managing the IDT (Interrupt
//! Descriptor Table) and SYSCALL MSR configuration.
//!
//! # x86-64 specific: IDT + SYSCALL MSR
//!
//! On x86-64, the trap entry path uses **two** distinct mechanisms that
//! are INTENTIONALLY hidden from OS code via the `TrapEntryArch` trait:
//!
//! 1. **IDT** (Interrupt Descriptor Table) — handles CPU exceptions and
//!    hardware interrupts; each IDT gate encodes (vector, handler, DPL, IST)
//! 2. **SYSCALL MSR** (LSTAR MSR) — handles user→kernel system calls; the
//!    MSR holds the handler address; the CPU jumps there on `syscall`
//!
//! This split exists because x86-64 ISA evolved SYSCALL as a separate
//! fast path from interrupt-based trap. The OS calls `set_handler()` for
//! exception/interrupt vectors and `configure_syscall()` for syscall
//! entry — it never writes IDT entries or MSRs directly.
//!
//! See `rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
//! §1.2 (the three CPU questions) and §4.6 (the cross-arch entry flow) for
//! the OS-level trap flow
//! these mechanisms implement.
//!
//! # 64-bit long mode specifics
//!
//! - IDT gate descriptors are 16 bytes (vs 8 bytes in 32-bit mode); the
//!   extra space holds the upper 32 bits of the handler address and the
//!   IST field
//! - IST (Interrupt Stack Table) field added to gate descriptors;
//!   allows specifying a separate stack per interrupt class
//! - SYSENTER removed — only SYSCALL/SYSRET in 64-bit mode
//!
//! # How the OS-level concerns map to x86-64 mechanisms
//!
//! | OS concern (trait method) | x86-64 mechanism (this file's impl) |
//! |---------------------------|--------------------------------------|
//! | `init` (install trap entry) | Fill IDT with gate_table_exceptions[] and gate_table_pic[] (handler addresses, DPL, IST) |
//! | `configure_syscall` (syscall entry) | Write LSTAR MSR + STAR MSR + SFMASK; enable EFER.SCE |
//! | `load` (make trap entry effective) | `lidt` (IDTR) |
//! | `load_ap` (AP trap entry) | Per-CPU IDT reload (each AP needs its own IDTR if IDT layout differs) |
//! | `set_handler` (install handler) | Write one IDT gate entry (handler addr + DPL + IST) |

use crate::trap_entry::{TrapEntryArch, InterruptVector};
use crate::x86_64::protection::{KERN_CS_SELECTOR, USER_STAR_ANCHOR_SELECTOR};
use minix_types::VirBytes;

// IDT gate type encoding (p_dpl_type field bits 0-3)
// Intel SDM Vol. 3A §6.14.1: IDT Gate Descriptors
pub(crate) const GATE_TYPE_INTERRUPT: u8 = 0xE; // Interrupt gate (IF auto-cleared)
const GATE_PRESENT: u8 = 0x80;       // Gate descriptor present bit

// MSR addresses for SYSCALL/SYSRET configuration
// AMD64 Architecture Programmer's Manual Vol. 2 §4.1.5
pub(crate) const MSR_STAR: u32 = 0xC0000081;    // SYSRET/SYSCALL CS and SS
pub(crate) const MSR_LSTAR: u32 = 0xC0000082;   // SYSCALL entry point (RIP)
pub(crate) const MSR_SFMASK: u32 = 0xC0000084;  // SYSCALL RFLAGS mask
pub(crate) const MSR_EFER: u32 = 0xC0000080;    // Extended Feature Enable Register

// EFER bit definitions
pub(crate) const EFER_SCE: u64 = 0x1;           // System Call Enable (bit 0)

// SFMASK: bits set in this mask are cleared in RFLAGS on SYSCALL.
// 0x200 = bit 9 = IF (Interrupt Flag) — SYSCALL clears IF by default.
pub(crate) const SFMASK_CLEAR_IF: u64 = 0x200;

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

    fn new(handler: u64, selector: u16, dpl: u8, ist: u8) -> Self {
        let type_bits = GATE_TYPE_INTERRUPT;
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
    fn set_gate(&mut self, vector: u8, handler: u64, dpl: u8, ist: u8) {
        self.idt[vector as usize] = IdtEntry64::new(handler, KERN_CS_SELECTOR, dpl, ist);
    }

    /// Whether `vector` carries a present gate in this table.
    ///
    /// Read back whether a gate is present.
    ///
    /// `install_idt_handlers` uses this after writing the 41 gates to assert
    /// that gate presence equals stub coverage; `load()` itself does not
    /// call it (the installer is the single writer and runs first).
    pub fn gate_present(&self, vector: u8) -> bool {
        self.idt[vector as usize].p_dpl_type & GATE_PRESENT != 0
    }

    /// Read back a gate's `(dpl, ist, type)` fields.
    ///
    /// The installer uses this to assert the single-writer postcondition:
    /// after it writes all 41 gates, every DPL comes from
    /// `user_accessible`, every IST from the NMI/#DF table, and every type
    /// is an interrupt gate. Fields are byte-sized, so reading through the
    /// packed entry is aligned.
    pub(crate) fn gate_metadata(&self, vector: u8) -> (u8, u8, u8) {
        let gate = &self.idt[vector as usize];
        ((gate.p_dpl_type >> 5) & 0x3, gate.ist & 0x7, gate.p_dpl_type & 0x0F)
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
pub(crate) unsafe fn wrmsr(msr: u32, value: u64) { unsafe {
    let low = value as u32;
    let high = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("edx") high,
        in("eax") low,
        options(nostack, preserves_flags),
    );
}}

/// Read a 64-bit value from a Model-Specific Register.
///
/// # Safety
///
/// - `msr` must be a valid MSR index for the current CPU.
///   Reading an invalid MSR raises a #GP exception.
pub(crate) unsafe fn rdmsr(msr: u32) -> u64 { unsafe {
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
}}

impl TrapEntryArch for X86_64TrapEntry {
    fn init() -> Self {
        // Placeholder table: every entry is absent (handler/dpl/ist/type all
        // zero). `install_idt_handlers` is the sole writer of the 41 real
        // gates, immediately before `load()`; `set_handler` writes all
        // metadata (DPL from `user_accessible`, IST from the NMI/#DF table,
        // type always an interrupt gate), so there is no second metadata
        // source to drift.
        Self {
            idt: [IdtEntry64::empty(); IDT_ENTRIES],
        }
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
            // STAR[32:47] = SYSCALL CS (KERN_CS_SELECTOR) — SYSCALL loads
            // CS ← STAR[47:32] and SS ← STAR[47:32] + 8 (KERN_DS).
            // STAR[48:63] = SYSRET anchor (USER_STAR_ANCHOR_SELECTOR) —
            // sysret64 loads SS ← anchor + 8 (user data, GDT[4]) and
            // CS ← anchor + 16 (user code, GDT[5]); see
            // `GDT_USER_ANCHOR_INDEX` for the +8/+16 convention.
            // C: AMD_MSR_STAR — archconst.h:170 (i386 convention did not
            // transfer; the x86-64 port derives the layout from the AMD
            // APM sysret pseudocode, Linux GDT as precedent).
            let star = (KERN_CS_SELECTOR as u64) << 32
                     | (USER_STAR_ANCHOR_SELECTOR as u64) << 48;
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

        // SAFETY / LIFETIME CONTRACT:
        // - `self` (and therefore `self.idt`) must remain alive and immovable
        //   as long as the IDT is loaded. LIDT stores a pointer to this array;
        //   if `X86_64TrapEntry` is dropped or moved after `load()`, the CPU
        //   will reference freed/relocated memory on the next interrupt.
        // - In the kernel this is enforced by keeping the trap entry in a
        //   static/global location after `load()` is called.
        // - idtr points to a valid IdtPtr on the stack with correct
        //   limit (IDT size - 1) and base (IDT array address).
        // - self.idt contains a valid IDT initialized by init() and populated
        //   with real handler addresses via set_handler().
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
        // Sole writer of every gate field: handler, DPL, IST and type.
        // The installer passes the canonical `user_accessible(vector)` and
        // this function maps NMI/#DF to IST1/IST2. `init()` writes only
        // absent placeholders.
        let dpl = if user_accessible { 3 } else { 0 };
        let ist = match vector.get() {
            2 => 1,
            8 => 2,
            _ => 0,
        };
        self.set_gate(vector.get(), handler.get(), dpl, ist);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap_entry::{PAGE_FAULT, DOUBLE_FAULT};

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
    fn idt_init_writes_only_placeholders() {
        let entry = X86_64TrapEntry::init();
        for vector in 0..=u8::MAX {
            assert!(
                !entry.gate_present(vector),
                "init() must not install a live gate (vector {vector:#04x})"
            );
        }
    }

    #[test]
    fn idt_init_reserved_vectors_not_present() {
        let entry = X86_64TrapEntry::init();
        // Vectors 9 and 15 are reserved in x86-64 long mode.
        for &vec in &[9u8, 15] {
            assert!(
                !entry.gate_present(vec),
                "Vector {} must NOT be present (reserved)",
                vec
            );
        }
    }

    fn installed() -> X86_64TrapEntry {
        let mut entry = X86_64TrapEntry::init();
        crate::x86_64::trap_stub::install_idt_handlers(&mut entry);
        entry
    }

    #[test]
    fn install_sets_syscall_ipc_vectors() {
        let entry = installed();
        // KERN_CALL_VECTOR_ORIG=32, IPC_VECTOR_ORIG=33,
        // KERN_CALL_VECTOR_UM=34, IPC_VECTOR_UM=35.
        for &vec in &[32u8, 33, 34, 35] {
            let (dpl, _ist, kind) = entry.gate_metadata(vec);
            assert!(entry.gate_present(vec), "Syscall/IPC vector {} must be present", vec);
            assert_eq!(dpl, 3, "Syscall/IPC vector {} must be user-accessible (DPL=3)", vec);
            assert_eq!(kind, GATE_TYPE_INTERRUPT, "vector {} must be an interrupt gate", vec);
        }
    }

    #[test]
    fn install_sets_pic_vectors() {
        let entry = installed();
        // C: VECTOR(irq) = (irq < 8 ? 0x50 : 0x70) + (irq & 7)
        for irq in 0..8u8 {
            for &base in &[0x50u8, 0x70] {
                let vec = base + irq;
                let (dpl, ist, kind) = entry.gate_metadata(vec);
                assert!(entry.gate_present(vec), "PIC vector {} must be present", vec);
                assert_eq!(dpl, 0, "PIC vector {} must be kernel-only (DPL=0)", vec);
                assert_eq!(ist, 0, "PIC vector {} must not use IST", vec);
                assert_eq!(kind, GATE_TYPE_INTERRUPT, "vector {} must be an interrupt gate", vec);
            }
        }
    }

    #[test]
    fn install_breakpoint_has_dpl3() {
        // Breakpoint (INT3, vector 3) must be DPL=3 so user mode can trigger it.
        let entry = installed();
        let (dpl, _ist, _kind) = entry.gate_metadata(3);
        assert_eq!(dpl, 3, "Breakpoint (vector 3) must have DPL=3");
    }

    #[test]
    fn install_overflow_has_dpl3() {
        // Overflow (INTO, vector 4) must be DPL=3 so user mode can trigger it.
        let entry = installed();
        let (dpl, _ist, _kind) = entry.gate_metadata(4);
        assert_eq!(dpl, 3, "Overflow (vector 4) must have DPL=3");
    }

    #[test]
    fn install_double_fault_uses_ist2() {
        // Double fault (vector 8) uses IST=2 per the NMI/#DF table.
        let entry = installed();
        let (_dpl, ist, _kind) = entry.gate_metadata(8);
        assert_eq!(ist, 2, "Double fault must use IST2");
    }

    #[test]
    fn install_nmi_uses_ist1() {
        // NMI (vector 2) uses IST=1 per the NMI/#DF table.
        let entry = installed();
        let (_dpl, ist, _kind) = entry.gate_metadata(2);
        assert_eq!(ist, 1, "NMI must use IST1");
    }

    #[test]
    fn install_kernel_exceptions_have_dpl0() {
        // Most exceptions (0,1,5,6,7,8,10-14,16-19) must have DPL=0.
        let entry = installed();
        for &vec in &[0u8, 1, 5, 6, 7, 8, 10, 11, 12, 13, 14, 16, 17, 18, 19] {
            let (dpl, _ist, _kind) = entry.gate_metadata(vec);
            assert_eq!(dpl, 0, "Vector {} must have DPL=0", vec);
        }
    }

}
