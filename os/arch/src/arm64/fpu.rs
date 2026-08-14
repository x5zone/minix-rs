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
//! ARM64 does not use CR0.TS-style lazy disable. Instead, CPACR_EL1.FPEN
//! controls access:
//! - FPEN=0b00: trap EL0 and EL1 FPSIMD accesses
//! - FPEN=0b01: trap EL0 FPSIMD accesses
//! - FPEN=0b11: no trap (full access)
//!
//! The kernel sets FPEN=0b11 at init and uses explicit save/restore
//! during context switch (no lazy trap path).
//!
//! C: earm arch_system.c:30-90 — fpu 族为空实现（lazy 模型在 ARM 未启用，无陷阱路径）

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
        // C: earm arch_system.c:save_local_fpu 为空实现；Rust 侧 stp q0..q31 显式保存
        //
        // SAFETY: `dst` is 16-byte aligned. Caller must hold BKL.
        unsafe {
            // Save Q0-Q31 using stp with post-increment
            let ptr = dst.regs.as_mut_ptr() as *mut u64;
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
                "mrs {fpsr}, fpsr_el1",
                "mrs {fpcr}, fpcr_el1",
                ptr = in(reg) ptr,
                fpsr = out(reg) dst.fpsr,
                fpcr = out(reg) dst.fpcr,
                options(nostack),
            );
        }
    }

    fn restore(&self, src: &Self::State) {
        // Restore FPSIMD state: 32 Q registers + FPSR + FPCR.
        // C: earm arch_system.c:restore_fpu 为空实现；Rust 侧 ldp q0..q31 显式恢复
        //
        // SAFETY: `src` is 16-byte aligned and contains valid FPSIMD data.
        // Caller must hold BKL.
        unsafe {
            let ptr = src.regs.as_ptr() as *const u64;
            let fpsr = src.fpsr;
            let fpcr = src.fpcr;
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
                "msr fpsr_el1, {fpsr}",
                "msr fpcr_el1, {fpcr}",
                ptr = in(reg) ptr,
                fpsr = in(reg) fpsr,
                fpcr = in(reg) fpcr,
                options(nostack),
            );
        }
    }

    fn enable(&self) {
        // ARM64 does not use lazy FPU disable. FPEN is set to 0b11 at init
        // and never changed. This is a no-op.
        //
        // C: enable_fpu() — no-op on ARM64
    }

    fn disable(&self) {
        // ARM64 does not use lazy FPU disable. Context switch uses
        // explicit save/restore instead of trap-on-use.
        //
        // C: disable_fpu() — no-op on ARM64
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
