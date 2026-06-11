//! ARM64 (aarch64) trap entry implementation
//!
//! Implements `TrapEntryArch` for ARM64, managing the exception vector
//! table (VBAR_EL1).
//!
//! # ARM64 exception vector table layout
//!
//! The exception vector table has 16 entries, each 128 bytes (0x80),
//! organized as 4 exception types × 4 SPsr combinations:
//!
//! | Offset | SError | IRQ | FIQ | SVC |
//! |--------|--------|-----|-----|-----|
//! | 0x000  | Current EL with SP0 | | | |
//! | 0x080  | Current EL with SPx | | | |
//! | 0x100  | Lower EL with AArch64 | | | |
//! | 0x180  | Lower EL with AArch32 | | | |
//!
//! Minix-RS uses only:
//! - 0x000-0x07F: Current EL SP0 (synchronous) — should not happen
//! - 0x080-0x0FF: Current EL SPx (IRQ/FIQ/SError) — kernel interrupts
//! - 0x100-0x17F: Lower EL AArch64 (SVC/IRQ/FIQ/SError) — user→kernel
//!
//! C: prot_init() — earm/protect.c:77 (write_vbar)

use crate::trap_entry::{TrapEntryArch, InterruptVector};
use minix_types::VirBytes;
use core::arch::asm;

/// ARM64 trap entry state.
///
/// On ARM64, the trap entry mechanism is the exception vector table,
/// pointed to by VBAR_EL1. The table itself is defined in assembly
/// (exc_vector_table), and this struct manages loading it into the
/// hardware register.
pub struct AArch64TrapEntry;

impl TrapEntryArch for AArch64TrapEntry {
    fn init() -> Self {
        // On ARM64, the exception vector table is defined in assembly.
        // We don't need to fill any descriptor table — we just need
        // to set VBAR_EL1 to the table's address in load().
        Self
    }

    fn configure_syscall(&mut self, _entry_point: VirBytes) {
        // ARM64 uses the SVC instruction for system calls, which
        // vectors through the same VBAR_EL1 exception table.
        // There is no separate SYSCALL MSR configuration like x86-64.
        // The SVC entry is at VBAR_EL1 + 0x400 (Lower EL, AArch64,
        // synchronous exception).
    }

    fn load(&self) {
        // Set VBAR_EL1 to the exception vector table address.
        // The table is defined in assembly as exc_vector_table.
        extern "C" {
            static exc_vector_table: u8;
        }
        // SAFETY: VBAR_EL1 write is safe because:
        // - We are at EL1 (kernel mode), required for MSR access.
        // - exc_vector_table is a valid symbol defined in assembly,
        //   aligned to 2KB per ARM Architecture Reference Manual.
        // - ISB ensures the write is visible before any exception.
        unsafe {
            let vbar = &exc_vector_table as *const u8 as u64;
            asm!("msr vbar_el1, {}", in(reg) vbar);
            // Instruction Synchronization Barrier: ensures VBAR_EL1
            // write is visible to subsequent exception handling.
            asm!("isb");
        }
    }

    fn load_ap(&self) {
        // Each AP needs its own VBAR_EL1 pointing to the same
        // exception vector table. The table is shared across CPUs.
        self.load();
    }

    fn set_handler(
        &mut self,
        _vector: InterruptVector,
        _handler: VirBytes,
        _user_accessible: bool,
    ) {
        // ARM64 uses a fixed exception vector table defined in assembly.
        // Dynamic handler registration is done in software by the
        // exception dispatcher, not by modifying the VBAR table.
        // This method is a no-op on ARM64.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trap_entry_init_returns_unit_struct() {
        // AArch64TrapEntry is a unit struct — no state to verify.
        let _entry = AArch64TrapEntry::init();
    }

    #[test]
    fn configure_syscall_is_noop() {
        // ARM64 uses SVC for syscalls — no MSR configuration needed.
        // configure_syscall should not panic.
        let mut entry = AArch64TrapEntry::init();
        entry.configure_syscall(VirBytes::new(0xDEAD));
    }

    #[test]
    fn set_handler_is_noop() {
        // ARM64 uses fixed exception vector table — set_handler is no-op.
        // Should not panic.
        let mut entry = AArch64TrapEntry::init();
        entry.set_handler(InterruptVector::new(14), VirBytes::new(0xBEEF), true);
    }
}
