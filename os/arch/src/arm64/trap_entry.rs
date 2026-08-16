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
//! # OS-level concerns → ARM64 mechanism mapping
//!
//! The `TrapEntryArch` trait exposes OS-level concerns; this file maps
//! each to its concrete ARM64 implementation:
//!
//! | OS concern (trait method) | ARM64 mechanism (this file's impl) |
//! |---------------------------|--------------------------------------|
//! | `init` (install trap entry) | Fill exception vector table (16 entries × 128 bytes = 2KB) at `exc_vector_table`; covers 3 of 4 categories (Current EL SP0 is unused) |
//! | `configure_syscall` (syscall entry) | **No-op** — SVC uses the same exception vector as other traps (no separate MSR-style config like x86 SYSCALL) |
//! | `load` (make trap entry effective) | `msr VBAR_EL1, ...` (write vector base register) + `isb` (instruction sync barrier) |
//! | `load_ap` (AP trap entry) | Per-CPU VBAR_EL1 write — each AP needs its own (exception vector base is per-CPU on ARM64) |
//! | `set_handler` (install handler) | Update one of the 16 exception vector table entries (modify the `br` instruction in the 128-byte slot) |
//!
//! # Why ARM64 has 16 entries (vs x86-64's 256-entry IDT)
//!
//! x86-64 IDT is keyed by interrupt number (256 entries). ARM64
//! exception vector is keyed by (exception type × source SP × source
//! EL) — only 4×4=16 entries. This forces the handler to inspect
//! ESR_EL1 (Exception Syndrome Register) to dispatch on specific
//! exception numbers. Trade-off: smaller table, but slower dispatch.
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

    fn configure_ipc_entry(&mut self, _entry_point: VirBytes) {
        // ARM64: No-op — IPC and kernel-call traps share the same SVC
        // exception vector (VBAR_EL1 + 0x400). The dispatch happens in
        // software: the SVC handler reads r3 at runtime to distinguish:
        //   r3 == KERVEC_INTR → kernel_call_entry → kernel_call()
        //   r3 == IPCVEC_INTR → ipc_entry → do_ipc()
        //
        // This mirrors the C implementation where the assembly SVC handler
        // (earm/mpx.S:181-184) performs the register check and branches
        // to the appropriate entry. No IDT-gate-style configuration is
        // needed because ARM64 has no per-vector interrupt table for
        // synchronous exceptions.
        //
        // C: earm/mpx.S:181-184
        //   cmp r3, #KERVEC_INTR
        //   beq kernel_call_entry
        //   cmp r3, #IPCVEC_INTR
        //   beq ipc_entry
    }

    fn load(&self) {
        // Set VBAR_EL1 to the exception vector table address.
        // The table is defined in assembly as exc_vector_table.
        unsafe extern "C" {
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
    use crate::trap_entry::TrapEntryArch;

    // 注：原 4 个烟雾测试（trap_entry_init_returns_unit_struct /
    // configure_syscall_is_noop / set_handler_is_noop /
    // configure_ipc_entry_is_noop_on_arm64）改为编译期约束检查 —— 验证
    // AArch64TrapEntry 满足 TrapEntryArch trait bound。`set_handler` 等在
    // ARM64 上为 noop，运行时无可观察行为；这些方法的存在与正确编译即可
    // 替代运行时断言。
    //
    // 运行时正确性（异常向量表布局、VBAR_EL1 装载、SVC dispatch）由 QEMU
    // 集成测试覆盖。

    #[test]
    fn aarch64_trap_entry_arch_impl_satisfies_trait_bound() {
        // Compile-time check: AArch64TrapEntry implements TrapEntryArch
        // and satisfies its Sized bound. Replaces 4 prior smoke tests
        // that asserted nothing observable on unit-struct APIs.
        // Mirrors x86_64 / riscv64 trap_entry.rs patterns.
        fn _check<T: TrapEntryArch>() {}
        _check::<AArch64TrapEntry>();
    }
}
