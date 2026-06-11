//! RISC-V 64-bit (Sv39) trap entry implementation
//!
//! Implements `TrapEntryArch` for RISC-V 64-bit, managing the trap
//! vector (stvec CSR).
//!
//! # RISC-V trap vector layout
//!
//! stvec has two fields:
//! - BASE[63:2]: Trap vector base address, must be 4-byte aligned
//! - MODE[1:0]: Vector mode
//!   - 0: Direct mode — all traps set pc = BASE
//!   - 1: Vectored mode — synchronous traps set pc = BASE,
//!     interrupts set pc = BASE + 4 × cause
//!
//! Minix-RS uses Direct mode (MODE=0), same as most RISC-V OS kernels.
//! All traps go to a single entry point, and the handler dispatches
//! based on scause (supervisor cause register).
//!
//! Note: Minix3 does not have a RISC-V port. The semantics are derived
//! from the RISC-V Privileged Specification.

use crate::trap_entry::{TrapEntryArch, InterruptVector};
use minix_types::VirBytes;
use core::arch::asm;

/// RISC-V 64-bit trap entry state.
///
/// On RISC-V, the trap entry mechanism is the stvec CSR, which points
/// to the trap handler. The handler itself is defined in assembly
/// (trap_vector), and this struct manages loading it into the CSR.
pub struct Riscv64TrapEntry;

impl TrapEntryArch for Riscv64TrapEntry {
    fn init() -> Self {
        // On RISC-V, the trap handler is defined in assembly.
        // We don't need to fill any descriptor table — we just need
        // to set stvec to the handler's address in load().
        Self
    }

    fn configure_syscall(&mut self, _entry_point: VirBytes) {
        // RISC-V uses the ecall instruction for system calls, which
        // traps through the same stvec vector. There is no separate
        // SYSCALL MSR configuration like x86-64.
        //
        // The ecall from U-mode sets scause = Environment Call from
        // U-mode (cause = 8), which the trap handler dispatches to
        // the syscall handler.
    }

    fn load(&self) {
        // Set stvec to the trap vector address in Direct mode (MODE=0).
        // The trap vector is defined in assembly as trap_vector.
        extern "C" {
            static trap_vector: u8;
        }
        // SAFETY: CSR write to stvec is safe because:
        // - We are in S-mode (supervisor), required for CSR access.
        // - trap_vector is a valid symbol defined in assembly,
        //   4-byte aligned as required by Direct mode.
        // - Direct mode (MODE=0) is the simplest and most common mode.
        unsafe {
            let stvec_addr = &trap_vector as *const u8 as usize;
            // stvec = BASE (aligned to 4) | MODE (0 = Direct)
            // The address must be 4-byte aligned for Direct mode.
            asm!("csrw stvec, {}", in(reg) stvec_addr);
        }
    }

    fn load_ap(&self) {
        // Each AP (hart) needs its own stvec pointing to the same
        // trap vector. The handler code is shared across harts.
        self.load();
    }

    fn set_handler(
        &mut self,
        _vector: InterruptVector,
        _handler: VirBytes,
        _user_accessible: bool,
    ) {
        // RISC-V uses Direct mode (all traps go to stvec BASE).
        // Dynamic handler registration is done in software by the
        // trap dispatcher based on scause, not by modifying stvec.
        // This method is a no-op on RISC-V.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trap_entry_init_returns_unit_struct() {
        // Riscv64TrapEntry is a unit struct — no state to verify.
        let _entry = Riscv64TrapEntry::init();
    }

    #[test]
    fn configure_syscall_is_noop() {
        // RISC-V uses ecall for syscalls — no CSR configuration needed.
        // configure_syscall should not panic.
        let mut entry = Riscv64TrapEntry::init();
        entry.configure_syscall(VirBytes::new(0xDEAD));
    }

    #[test]
    fn set_handler_is_noop() {
        // RISC-V uses Direct mode — set_handler is no-op.
        // Should not panic.
        let mut entry = Riscv64TrapEntry::init();
        entry.set_handler(InterruptVector::new(14), VirBytes::new(0xBEEF), true);
    }
}
