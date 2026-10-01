//! ARM64 FPU implementation using FPSIMD save/restore.
//!
//! Implements `FpuArch` for ARM64. Uses the FPSIMD save/restore
//! instructions (`stp`/`ldp` of Q registers + FPSR/FPCR).
//!
//! # FPU state buffer
//!
//! The FPSIMD state consists of:
//! - 32 × 128-bit general registers (Q0-Q31): 512 bytes
//! - FPSR (4 bytes) + FPCR (4 bytes): 8 bytes
//! Total: 520 bytes (padded to 528 for 16-byte alignment)
//!
//! # Lazy FPU on ARM64
//!
//! ARM64 uses CPACR_EL1.FPEN as the lazy-FPU gate (the analogue of x86's
//! CR0.TS):
//! - FPEN=0b00: trap EL0 and EL1 FPSIMD accesses
//! - FPEN=0b01: trap EL0 FPSIMD accesses (EL1 unaffected — kernel NEON
//!   keeps working while user FP traps)
//! - FPEN=0b11: no trap (full access)
//!
//! The lazy model mirrors C `copr_not_available_handler` (proc.c:1923-1962):
//! a context switch to a non-owner disables user FP ([`FpuArch::disable`]
//! = FPEN=0b01), the trapping process's first FP instruction takes the
//! EC=0x07 trap, and the kernel saves the outgoing owner's state, restores
//! the new owner's, and re-enables ([`FpuArch::enable`] = FPEN=0b11).

use crate::fpu_arch::FpuArch;

/// FPSIMD state size: 32 × 16 bytes (Q0-Q31) + 4 (FPSR) + 4 (FPCR) = 524.
/// Padded to 528 for 16-byte alignment.
const FPSIMD_SIZE: usize = 528;

/// ARM64 FPSIMD state buffer.
///
/// Contains 32 × 128-bit Q registers + FPSR + FPCR. The layout matches
/// the `struct fpsimd_state` in Linux's `arch/arm64/include/uapi/asm/ptrace.h`.
#[derive(Clone, Copy)]
#[repr(C, align(16))]
pub struct AArch64FpuState {
    /// 32 × 128-bit Q registers (Q0-Q31).
    /// Stored as 32 × [u64; 2] for easier manipulation.
    regs: [[u64; 2]; 32],
    /// FPSR (Floating-point Status Register).
    fpsr: u32,
    /// FPCR (Floating-point Control Register).
    fpcr: u32,
    /// Padding to 528 bytes (524 + 4 = 528).
    _pad: u32,
}

impl core::fmt::Debug for AArch64FpuState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AArch64FpuState")
            .field("fpsr", &self.fpsr)
            .field("fpcr", &self.fpcr)
            .finish()
    }
}

impl Default for AArch64FpuState {
    fn default() -> Self {
        Self {
            regs: [[0u64; 2]; 32],
            fpsr: 0,
            fpcr: 0,
            _pad: 0,
        }
    }
}

impl AArch64FpuState {
    /// Const-constructible zeroed state (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            regs: [[0u64; 2]; 32],
            fpsr: 0,
            fpcr: 0,
            _pad: 0,
        }
    }
}

/// ARM64 FPU implementation using FPSIMD instructions.
#[derive(Debug, Default, Clone, Copy)]
pub struct AArch64FpuArch;

/// Raw FPSIMD save: stp Q0-Q31 + read FPSR/FPCR into `(fpsr, fpcr)`.
///
/// The kernel build compiles with `-C target-feature=-neon` (the kernel
/// must never touch Q0-Q31 implicitly — see the lazy-FPU model above and
/// the kernel-image build flag in `xtask/src/image.rs`); the FPU-state
/// machine itself is the one legitimate FP user, re-enabled per function.
#[target_feature(enable = "neon")]
unsafe fn fpsimd_save_raw(ptr: *mut u64) -> (u32, u32) {
    let fpsr: u32;
    let fpcr: u32;
    core::arch::asm!(
        "stp q0, q1, [{ptr}], #32",
        "stp q2, q3, [{ptr}], #32",
        "stp q4, q5, [{ptr}], #32",
        "stp q6, q7, [{ptr}], #32",
        "stp q8, q9, [{ptr}], #32",
        "stp q10, q11, [{ptr}], #32",
        "stp q12, q13, [{ptr}], #32",
        "stp q14, q15, [{ptr}], #32",
        "stp q16, q17, [{ptr}], #32",
        "stp q18, q19, [{ptr}], #32",
        "stp q20, q21, [{ptr}], #32",
        "stp q22, q23, [{ptr}], #32",
        "stp q24, q25, [{ptr}], #32",
        "stp q26, q27, [{ptr}], #32",
        "stp q28, q29, [{ptr}], #32",
        "stp q30, q31, [{ptr}], #32",
        // FPSR/FPCR aliases: `fpsr_el1`/`fpcr_el1` require
        // Armv8.8-A; the unsuffixed aliases name the same register
        // at the current exception level (EL1 in kernel context).
        "mrs {fpsr}, fpsr",
        "mrs {fpcr}, fpcr",
        ptr = in(reg) ptr,
        fpsr = out(reg) fpsr,
        fpcr = out(reg) fpcr,
        options(nostack),
    );
    (fpsr, fpcr)
}

/// Raw FPSIMD restore: ldp Q0-Q31 + write FPSR/FPCR.
///
/// The `v0`-`v31` clobbers are load-bearing for the caller-saved lanes:
/// LLVM does not track registers named inside the asm string, so without
/// them it may assume Q-register values survive the restore and reuse
/// stale copies.
///
/// ⚠ **Intentional ABI exemption for v8-v15** (d8-d15, callee-saved low
/// 64 bits): clobbering them would make LLVM spill d8-d15 in the
/// prologue and reload them in the epilogue — AFTER the ldp sequence —
/// overwriting the just-restored user lanes with the values live at
/// entry (the previous owner's registers), resurrecting exactly the
/// corruption this module exists to fix. The exemption is safe because
/// the whole kernel is compiled with `-C target-feature=-neon,-fp-armv8`
/// (see the kernel-image build flag in `xtask/src/image.rs`): no kernel
/// code outside this module carries live V-register values across a call,
/// so the callee-saved guarantee has no user here. A dual-FP-user kernel
/// regression test must keep this invariant honest.
#[target_feature(enable = "neon")]
unsafe fn fpsimd_restore_raw(ptr: *const u64, fpsr: u32, fpcr: u32) {
    core::arch::asm!(
        "ldp q0, q1, [{ptr}], #32",
        "ldp q2, q3, [{ptr}], #32",
        "ldp q4, q5, [{ptr}], #32",
        "ldp q6, q7, [{ptr}], #32",
        "ldp q8, q9, [{ptr}], #32",
        "ldp q10, q11, [{ptr}], #32",
        "ldp q12, q13, [{ptr}], #32",
        "ldp q14, q15, [{ptr}], #32",
        "ldp q16, q17, [{ptr}], #32",
        "ldp q18, q19, [{ptr}], #32",
        "ldp q20, q21, [{ptr}], #32",
        "ldp q22, q23, [{ptr}], #32",
        "ldp q24, q25, [{ptr}], #32",
        "ldp q26, q27, [{ptr}], #32",
        "ldp q28, q29, [{ptr}], #32",
        "ldp q30, q31, [{ptr}], #32",
        "msr fpsr, {fpsr}",
        "msr fpcr, {fpcr}",
        ptr = in(reg) ptr,
        fpsr = in(reg) fpsr,
        fpcr = in(reg) fpcr,
        out("v0") _, out("v1") _, out("v2") _, out("v3") _,
        out("v4") _, out("v5") _, out("v6") _, out("v7") _,
        out("v16") _, out("v17") _, out("v18") _, out("v19") _,
        out("v20") _, out("v21") _, out("v22") _, out("v23") _,
        out("v24") _, out("v25") _, out("v26") _, out("v27") _,
        out("v28") _, out("v29") _, out("v30") _, out("v31") _,
        options(nostack),
    );
}

impl FpuArch for AArch64FpuArch {
    type State = AArch64FpuState;

    fn init(&self) {
        // Enable FPSIMD access by setting CPACR_EL1.FPEN = 0b11.
        // C: earm arch_system.c:fpu_init 为空实现；Rust 侧直接操作 CPACR_EL1（FPEN=0b11）
        unsafe {
            let mut cpacr: u64;
            core::arch::asm!("mrs {}, cpacr_el1", out(reg) cpacr);
            // Set FPEN (bits 20-21) to 0b11
            cpacr |= 0x0030_0000;
            core::arch::asm!("msr cpacr_el1, {}", in(reg) cpacr);
        }
    }

    fn save(&self, dst: &mut Self::State) {
        // Save FPSIMD state: 32 Q registers + FPSR + FPCR.
        // C: earm arch_system.c:save_local_fpu 对位（i386: fxsave；ARM64:
        // stp q0..q31 显式保存）。
        //
        // SAFETY: `dst` is 16-byte aligned. Caller must hold BKL.
        unsafe {
            let ptr = dst.regs.as_mut_ptr() as *mut u64;
            let (fpsr, fpcr) = fpsimd_save_raw(ptr);
            dst.fpsr = fpsr;
            dst.fpcr = fpcr;
        }
    }

    fn restore(&self, src: &Self::State) {
        // Restore FPSIMD state: 32 Q registers + FPSR + FPCR.
        // C: earm arch_system.c:restore_fpu 对位（i386: frstor；ARM64:
        // ldp q0..q31 显式恢复）。
        //
        // SAFETY: `src` is 16-byte aligned and contains valid FPSIMD data.
        // Caller must hold BKL.
        unsafe {
            let ptr = src.regs.as_ptr() as *const u64;
            fpsimd_restore_raw(ptr, src.fpsr, src.fpcr);
        }
    }

    fn enable(&self) {
        // FPEN=0b11 — full access for the FPU owner (C: `clts` after the
        // lazy restore handed the FPU to the trapping process, proc.c:1954).
        //
        // SAFETY: CPACR_EL1 is a side-effect-free EL1 system-register write.
        unsafe {
            let mut cpacr: u64;
            core::arch::asm!("mrs {}, cpacr_el1", out(reg) cpacr);
            cpacr |= 0x0030_0000; // FPEN = 0b11
            core::arch::asm!("msr cpacr_el1, {}", in(reg) cpacr);
        }
    }

    fn disable(&self) {
        // FPEN=0b01 — trap EL0 FPSIMD accesses only. EL1 (the kernel's own
        // explicit FPSIMD primitives in this module) stays enabled, so the
        // kernel never takes the SIMD/FP trap itself (an EL1 trap would
        // arrive as the same EC=0x07-class exception from current EL and
        // there is no handler wired for it). The next user FP instruction
        // traps into the EC=0x07 leg, which performs the lazy restore
        // (C: `disable_fpu_exception()` before a context switch,
        // proc.c:1930/1936).
        //
        // SAFETY: CPACR_EL1 is a side-effect-free EL1 system-register write.
        unsafe {
            let mut cpacr: u64;
            core::arch::asm!("mrs {}, cpacr_el1", out(reg) cpacr);
            // Clear FPEN (bits 20-21), then set 0b01 (bit 20 only).
            cpacr &= !0x0030_0000;
            cpacr |= 0x0010_0000; // FPEN = 0b01 (trap EL0)
            core::arch::asm!("msr cpacr_el1, {}", in(reg) cpacr);
        }
    }

    fn disable_exception(&self) {
        // ARM64 FPSIMD exceptions are managed via FPCR, not a separate
        // control register. No global exception enable/disable needed.
        //
        // C: disable_fpu_exception() — no-op on ARM64
    }

    fn is_present(&self) -> bool {
        // All ARM64 implementations have FPSIMD (it's mandatory in the
        // ARMv8-A architecture).
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fpu_state_size() {
        // 32 * 16 (regs) + 4 (fpsr) + 4 (fpcr) + 4 (pad) = 528
        assert_eq!(core::mem::size_of::<AArch64FpuState>(), FPSIMD_SIZE);
    }

    #[test]
    fn test_fpu_state_alignment() {
        assert_eq!(core::mem::align_of::<AArch64FpuState>(), 16);
    }

    #[test]
    fn test_fpu_state_default_is_zeroed() {
        let state = AArch64FpuState::default();
        assert_eq!(state.fpsr, 0);
        assert_eq!(state.fpcr, 0);
        for reg in &state.regs {
            assert_eq!(*reg, [0u64; 2]);
        }
    }
}
