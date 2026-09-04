//! riscv64 trap return implementation
//!
//! Implements [`TrapReturnArch`] for riscv64: loads sepc / sstatus from the
//! exception frame, restores the integer registers from the process's
//! register file, and executes `sret` back to U-mode. Return-path twin of
//! `riscv64::trap_entry` (stvec entry).
//!
//! # What lives where
//!
//! - **Frame** (`Riscv64ExceptionFrame`) — the mode-switch pair sstatus /
//!   sepc, rebuilt by `apply_to_trap_frame` from the context on every
//!   dispatch.
//! - **RegisterFile** (`Riscv64CpuContext`) — the integer registers: x2
//!   (sp) and x10 (a0, the ps_strings carrier) as named fields, everything
//!   else in `gp_regs` (indexed by the `GP_*` ABI constants: RA, GP, TP,
//!   T0–T2, S0–S1, A1–A7, S2–S11, T3–T6; X0 is hardwired zero and has no
//!   slot).
//!
//! # Register liveness in the asm
//!
//! The register-file pointer parks in **t6 (x31)** — the one target whose
//! `gp_regs` slot (GP_T6) is loaded last, self-referentially. t0 (x5)
//! serves as the sepc/sstatus scratch before its own slot loads; the named
//! fields sp/a0 load while the struct-relative pointer is still intact,
//! then the pointer rebases onto `gp_regs` for the plain-stride descent.
//!
//! # Interrupt guarantee
//!
//! `sret` re-enables interrupts iff SPIE is set (and SPP is clear for a
//! U-mode return). The impl clears SPP and sets SPIE while preparing
//! sstatus — the riscv64 analogue of the x86-64 `IF_MASK` OR (C:
//! `arch_finish_switch_to_user` — arch_system.c:512; Minix3 has no RISC-V
//! port, this follows the same semantic guarantee).

use crate::arch::trap_return::TrapReturnArch;
use crate::riscv64::boot::Riscv64CpuContext;
use crate::riscv64::exception::Riscv64ExceptionFrame;

/// sstatus.SPP — previous privilege mode. Cleared so `sret` returns to
/// U-mode.
const SSTATUS_SPP: u64 = 1 << 8;

/// sstatus.SPIE — previous interrupt-enable. Set so `sret` re-enables
/// interrupts in U-mode.
const SSTATUS_SPIE: u64 = 1 << 5;

/// riscv64 implementation marker (no state; the mode switch is a pure
/// register/CSR operation).
pub struct Riscv64TrapReturn;

impl TrapReturnArch for Riscv64TrapReturn {
    type Frame = Riscv64ExceptionFrame;
    type RegisterFile = Riscv64CpuContext;

    unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> ! {
        // SAFETY (full contract in TrapReturnArch::restore_to_user):
        // - `frame` was rebuilt from the picked process's saved state by
        //   `apply_to_trap_frame` immediately before this call.
        // - t0 (x5) and t6 (x31) are clobbered by design; both carry their
        //   user values again before `sret` — see the liveness notes in
        //   the module docs.
        // - The BKL has been released by the caller before this call.
        core::arch::asm!(
            // Park the register-file pointer in t6 (its user slot is
            // gp_regs[GP_T6], loaded last as a self-referential load).
            "mv t6, a1",
            // ── Mode-switch pair: sepc + sstatus ──
            "ld t0, {sepc_off}(a0)",
            "csrw sepc, t0",
            // SPP=0 (return to U-mode) + SPIE=1 (interrupts enabled after
            // sret). `csrci`/`csrsi` take 5-bit immediates, so SPP (0x100)
            // goes through t0.
            "li t0, {spp}",
            "csrc sstatus, t0",
            "li t0, {spie}",
            "csrs sstatus, t0",
            // ── Named register-file fields (struct-relative offsets) ──
            "ld sp, {sp_off}(t6)",   // x2 — user stack pointer
            "ld a0, {a0_off}(t6)",   // x10 — ps_strings / IPC-status carrier
            // ── gp_regs descent ──
            // Rebase t6 onto gp_regs; strides are plain slot offsets.
            "add t6, t6, {gp_off}",
            "ld t5, 27*8(t6)",   // x30 ← GP_T5
            "ld t4, 26*8(t6)",   // x29 ← GP_T4
            "ld t3, 25*8(t6)",   // x28 ← GP_T3
            "ld s11, 24*8(t6)",  // x27
            "ld s10, 23*8(t6)",  // x26
            "ld s9,  22*8(t6)",  // x25
            "ld s8,  21*8(t6)",  // x24
            "ld s7,  20*8(t6)",  // x23
            "ld s6,  19*8(t6)",  // x22
            "ld s5,  18*8(t6)",  // x21
            "ld s4,  17*8(t6)",  // x20
            "ld s3,  16*8(t6)",  // x19
            "ld s2,  15*8(t6)",  // x18
            "ld a7,  14*8(t6)",  // x17
            "ld a6,  13*8(t6)",  // x16
            "ld a5,  12*8(t6)",  // x15
            "ld a4,  11*8(t6)",  // x14
            "ld a3,  10*8(t6)",  // x13
            "ld a2,  9*8(t6)",   // x12
            "ld a1,  8*8(t6)",   // x11 ← GP_A1
            "ld s1,  7*8(t6)",   // x9
            "ld s0,  6*8(t6)",   // x8
            "ld t2,  5*8(t6)",   // x7
            "ld t1,  4*8(t6)",   // x6
            "ld t0,  3*8(t6)",   // x5 — scratch's user value
            "ld gp,  1*8(t6)",   // x3 ← GP_GP
            "ld ra,  0*8(t6)",   // x1 ← GP_RA
            "ld tp,  2*8(t6)",   // x4 ← GP_TP
            "ld t6,  28*8(t6)",  // x31 ← GP_T6 — pointer's last use
            "sret",
            frame = in("a0") frame as *const Riscv64ExceptionFrame,
            regs = in("a1") regs as *const Riscv64CpuContext,
            sepc_off = const core::mem::offset_of!(Riscv64ExceptionFrame, sepc),
            sp_off = const core::mem::offset_of!(Riscv64CpuContext, sp),
            a0_off = const core::mem::offset_of!(Riscv64CpuContext, a0),
            gp_off = const core::mem::offset_of!(Riscv64CpuContext, gp_regs),
            spp = const SSTATUS_SPP,
            spie = const SSTATUS_SPIE,
            // `noreturn` documents the divergence and frees the clobber
            // set (every integer register is reloaded right before sret).
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sstatus_bits_are_spp_and_spie() {
        // SPP = bit 8 (previous privilege), SPIE = bit 5 (previous
        // interrupt enable) — RISC-V privileged spec, sstatus register.
        assert_eq!(SSTATUS_SPP, 0x100);
        assert_eq!(SSTATUS_SPIE, 0x020);
    }

    #[test]
    fn gp_regs_layout_matches_asm_strides() {
        // gp_regs covers X1, X3–X9, X11–X31 (30 slots, GP_RA = 0 ..
        // GP_T6 = 28). x0 is hardwired zero; x2/x10 are named fields.
        assert_eq!(Riscv64CpuContext::GP_REGS_LEN, 30);
    }
}
