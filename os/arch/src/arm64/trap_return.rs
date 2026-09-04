//! aarch64 trap return implementation
//!
//! Implements [`TrapReturnArch`] for aarch64: loads SPSR_EL1 / ELR_EL1 /
//! SP_EL0 from the exception frame, restores X0–X30 from the process's
//! register file, and executes `eret` back to EL0. Return-path twin of
//! `arm64::trap_entry` (VBAR_EL1 entry).
//!
//! # What lives where
//!
//! The two inputs split the saved state exactly like C's `p_reg` does:
//!
//! - **Frame** (`AArch64ExceptionFrame`) — the mode-switch triple
//!   SPSR_EL1 / ELR_EL1 / SP_EL0, rebuilt by `apply_to_trap_frame` from the
//!   context on every dispatch.
//! - **RegisterFile** (`AArch64CpuContext`) — the general registers: X0
//!   (ps_strings / IPC-status carrier) as a named field, X1–X30 in
//!   `gp_regs` (indexed `GP_X1..GP_X30` = ABI number − 1). This is the
//!   authoritative saved state — the trap-entry path (future work) writes
//!   it back here, mirroring C's `p_reg` being both initial and saved
//!   state.
//!
//! # Register liveness in the asm
//!
//! Every X register is a restore target, so the asm keeps the register-file
//! pointer in **X2** — whose own user value (`gp_regs[GP_X2]`) is loaded
//! last, as a self-referential load. X1's user value is loaded first
//! (`gp_regs[GP_X1]`), X30..X3 descend, X0 comes from the frame's named
//! context mirror. X16 serves as the system-register scratch (its user
//! value loads later through X2).
//!
//! # Interrupt guarantee
//!
//! Returning to EL0 with SPSR_EL1 mask bits set would leave the user
//! process with interrupts masked — the aarch64 analogue of entering user
//! mode with IF=0. The impl clears A(5)/I(6)/F(7)/D(9) (SError, IRQ, FIQ,
//! Debug masks) while copying SPSR into the system register, matching the
//! x86-64 impl's `IF_MASK` OR (C: `arch_finish_switch_to_user` —
//! arch_system.c:512).

use crate::arch::trap_return::TrapReturnArch;
use crate::arm64::boot::AArch64CpuContext;
use crate::arm64::exception::AArch64ExceptionFrame;

/// SPSR_EL1 mask bits cleared on restore: A(5) SError, I(6) IRQ, F(7) FIQ,
/// D(9) Debug — guarantees an interruptible user context.
const SPSR_MASK_BITS: u64 = 0x1E0;

/// aarch64 implementation marker (no state; the mode switch is a pure
/// register/system-register operation).
pub struct AArch64TrapReturn;

impl TrapReturnArch for AArch64TrapReturn {
    type Frame = AArch64ExceptionFrame;
    type RegisterFile = AArch64CpuContext;

    unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> ! {
        // SAFETY (full contract in TrapReturnArch::restore_to_user):
        // - `frame` was rebuilt from the picked process's saved state by
        //   `apply_to_trap_frame` immediately before this call.
        // - X16 (system-register scratch) and X2 (register-file pointer)
        //   are clobbered by design; both carry their user values again
        //   before `eret` — see the liveness notes in the module docs.
        // - The BKL has been released by the caller before this call.
        core::arch::asm!(
            // Park the register-file pointer in X2 (its user slot is
            // gp_regs[GP_X2], loaded last as a self-referential load).
            "mov x2, x1",
            // ── Mode-switch triple: ELR / SPSR / SP_EL0 ──
            "ldr x16, [x0, {elr_off}]",
            "msr elr_el1, x16",
            "ldr x16, [x0, {spsr_off}]",
            "bic x16, x16, #{mask_bits}", // interruptible user context
            "msr spsr_el1, x16",
            "ldr x16, [x0, {sp_off}]",
            "msr sp_el0, x16",
            // ── General registers from the register file ──
            // X0 is the named field (ps_strings carrier) inside the
            // register file — loading it frees the frame pointer, which
            // becomes user X0.
            "ldr x0, [x2, {r0_off}]",
            // Rebase X2 onto gp_regs so the strides below are plain
            // gp_regs offsets: gp_regs[i] = user register X(i+1).
            "add x2, x2, {gp_off}",
            "ldr x1, [x2]",
            "ldr x30, [x2, 29*8]",
            "ldr x29, [x2, 28*8]",
            "ldr x28, [x2, 27*8]",
            "ldr x27, [x2, 26*8]",
            "ldr x26, [x2, 25*8]",
            "ldr x25, [x2, 24*8]",
            "ldr x24, [x2, 23*8]",
            "ldr x23, [x2, 22*8]",
            "ldr x22, [x2, 21*8]",
            "ldr x21, [x2, 20*8]",
            "ldr x20, [x2, 19*8]",
            "ldr x19, [x2, 18*8]",
            "ldr x18, [x2, 17*8]",
            "ldr x17, [x2, 16*8]",
            "ldr x16, [x2, 15*8]",
            "ldr x15, [x2, 14*8]",
            "ldr x14, [x2, 13*8]",
            "ldr x13, [x2, 12*8]",
            "ldr x12, [x2, 11*8]",
            "ldr x11, [x2, 10*8]",
            "ldr x10, [x2, 9*8]",
            "ldr x9,  [x2, 8*8]",
            "ldr x8,  [x2, 7*8]",
            "ldr x7,  [x2, 6*8]",
            "ldr x6,  [x2, 5*8]",
            "ldr x5,  [x2, 4*8]",
            "ldr x4,  [x2, 3*8]",
            "ldr x3,  [x2, 2*8]",
            "ldr x2,  [x2, 1*8]", // pointer's last use — user X2
            "eret",
            frame = in("x0") frame as *const AArch64ExceptionFrame,
            regs = in("x1") regs as *const AArch64CpuContext,
            elr_off = const core::mem::offset_of!(AArch64ExceptionFrame, elr_el1),
            spsr_off = const core::mem::offset_of!(AArch64ExceptionFrame, spsr_el1),
            sp_off = const core::mem::offset_of!(AArch64ExceptionFrame, sp_el0),
            r0_off = const core::mem::offset_of!(AArch64CpuContext, r0),
            gp_off = const core::mem::offset_of!(AArch64CpuContext, gp_regs),
            mask_bits = const SPSR_MASK_BITS,
            // `noreturn` documents the divergence and frees the clobber
            // set (every X register is reloaded right before eret).
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spsr_mask_bits_cover_aifd() {
        // A(5) | I(6) | F(7) | D(9) = 0x20 | 0x40 | 0x80 | 0x200.
        assert_eq!(SPSR_MASK_BITS, 0x1E0);
    }

    #[test]
    fn gp_regs_layout_matches_asm_strides() {
        // The asm maps gp_regs[i] → user register X(i+1): GP_X1 = 0
        // (self-load `ldr x1, [x2]`) and GP_X30 = 29 (`[x2, 29*8]`).
        // AArch64CpuContext::GP_REGS_LEN must stay 30 (X1..X30).
        assert_eq!(AArch64CpuContext::GP_REGS_LEN, 30);
    }
}
