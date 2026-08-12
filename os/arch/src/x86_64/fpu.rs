//! x86-64 FPU implementation using FXSAVE/FXRSTOR.
//!
//! Implements `FpuArch` for x86-64. Uses the legacy FXSAVE/FXRSTOR
//! instructions (512-byte state) rather than XSAVE (variable-size).
//! This matches Minix3's `fpu.c` which uses `fxsave`/`fxrstor`.
//!
//! # FPU state buffer
//!
//! The FXSAVE area is 512 bytes and must be 16-byte aligned. The layout
//! is defined by the Intel SDM Vol 3A, Table 13-1 (FXSAVE Area Format).
//! The kernel does not inspect individual fields; it treats the buffer
//! as an opaque `Copy` blob.
//!
//! # CR0 / CR4 control
//!
//! - CR0.MP (bit 1): Monitor co-processor (set = 1)
//! - CR0.EM (bit 2): Emulate (set = 0, use hardware FPU)
//! - CR0.TS (bit 3): Task Switched (set = 1 to trap on FPU instructions)
//! - CR0.NE (bit 5): Numeric Error (set = 1, use native x87 error reporting)
//! - CR4.OSFXSR (bit 9): OS supports FXSAVE/FXRSTOR (set = 1)
//! - CR4.OSXMMEXCPT (bit 10): OS supports #XM exception (set = 1)
//!
//! C: `fpu.c` — `fpu_init()`, `save_local_fpu()`, `restore_fpu()`,
//! `enable_fpu()`, `disable_fpu()`, `disable_fpu_exception()`

use crate::fpu_arch::FpuArch;

/// FXSAVE area size in bytes (Intel SDM: 512 bytes for legacy FXSAVE).
const FXSAVE_SIZE: usize = 512;

/// x86-64 FPU state buffer (FXSAVE area).
///
/// 512 bytes, 16-byte aligned. The kernel stores this inside `KProcess`
/// (via `CpuContext`) and never reads its fields directly.
#[derive(Clone, Copy)]
#[repr(C, align(16))]
pub struct X86_64FpuState {
    data: [u8; FXSAVE_SIZE],
}

impl core::fmt::Debug for X86_64FpuState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("X86_64FpuState")
            .field("size", &self.data.len())
            .finish()
    }
}

impl Default for X86_64FpuState {
    fn default() -> Self {
        Self { data: [0u8; FXSAVE_SIZE] }
    }
}

impl X86_64FpuState {
    /// Const-constructible zeroed state (for `const fn` table init).
    pub const fn new() -> Self {
        Self { data: [0u8; FXSAVE_SIZE] }
    }
}

/// x86-64 FPU implementation using FXSAVE/FXRSTOR.
///
/// Stateless ZST — all state lives in the `X86_64FpuState` buffer passed
/// to `save`/`restore`.
#[derive(Debug, Default, Clone, Copy)]
pub struct X86_64FpuArch;

impl FpuArch for X86_64FpuArch {
    type State = X86_64FpuState;

    fn init(&self) {
        // Initialize CR0 and CR4 for hardware FPU support.
        // C: fpu.c:fpu_init()
        unsafe {
            // Read CR0
            let mut cr0: u64;
            core::arch::asm!("mov {}, cr0", out(reg) cr0);
            // Set MP (bit 1), NE (bit 5); clear EM (bit 2), TS (bit 3)
            cr0 |= (1 << 1) | (1 << 5);   // MP | NE
            cr0 &= !((1 << 2) | (1 << 3)); // ~EM | ~TS
            core::arch::asm!("mov cr0, {}", in(reg) cr0);

            // Read CR4
            let mut cr4: u64;
            core::arch::asm!("mov {}, cr4", out(reg) cr4);
            // Set OSFXSR (bit 9), OSXMMEXCPT (bit 10)
            cr4 |= (1 << 9) | (1 << 10);
            core::arch::asm!("mov cr4, {}", in(reg) cr4);
        }
    }

    fn save(&self, dst: &mut Self::State) {
        // FXSAVE saves the x87 FPU, MMX, and SSE state to a 512-byte area.
        // Unlike FNSAVE, FXSAVE does NOT initialize the FPU after saving.
        //
        // C: fpu.c:save_local_fpu() — `fxsave [dst]`
        //
        // SAFETY: `dst` is 16-byte aligned (repr(C, align(16))).
        // Caller must hold BKL (interrupts disabled).
        unsafe {
            let ptr = dst.data.as_mut_ptr() as *mut u8;
            core::arch::asm!(
                "fxsave [{ptr}]",
                ptr = in(reg) ptr,
                options(nostack, preserves_flags),
            );
        }
    }

    fn restore(&self, src: &Self::State) {
        // FXRSTOR restores x87 FPU, MMX, and SSE state from a 512-byte area.
        //
        // C: fpu.c:restore_fpu() — `fxrstor [src]`
        //
        // SAFETY: `src` is 16-byte aligned and contains valid FXSAVE data
        // (previously saved or zero-initialized).
        // Caller must hold BKL.
        unsafe {
            let ptr = src.data.as_ptr() as *const u8;
            core::arch::asm!(
                "fxrstor [{ptr}]",
                ptr = in(reg) ptr,
                options(nostack),
            );
        }
    }

    fn enable(&self) {
        // CLTS clears CR0.TS, allowing FPU instructions without trapping.
        //
        // C: enable_fpu() — `clts`
        unsafe {
            core::arch::asm!("clts", options(nostack, preserves_flags));
        }
    }

    fn disable(&self) {
        // Set CR0.TS to trap on next FPU instruction (#NM).
        //
        // C: disable_fpu() — `mov %cr0, %eax; or $8, %eax; mov %eax, %cr0`
        unsafe {
            let mut cr0: u64;
            core::arch::asm!("mov {}, cr0", out(reg) cr0);
            cr0 |= 1 << 3; // TS
            core::arch::asm!("mov cr0, {}", in(reg) cr0);
        }
    }

    fn disable_exception(&self) {
        // Clear CR4.OSXMMEXCPT to suppress #XM (SSE exception) during save.
        // Also CLTS to ensure FXSAVE doesn't trap.
        //
        // C: disable_fpu_exception() — `clts; and $~0x400, %cr4`
        unsafe {
            core::arch::asm!("clts", options(nostack, preserves_flags));
            let mut cr4: u64;
            core::arch::asm!("mov {}, cr4", out(reg) cr4);
            cr4 &= !(1 << 10); // ~OSXMMEXCPT
            core::arch::asm!("mov cr4, {}", in(reg) cr4);
        }
    }

    fn is_present(&self) -> bool {
        // On x86-64, FPU presence is indicated by CPUID.1:EDX[0] (FPU bit).
        // All x86-64 CPUs have an FPU, so this always returns true.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fpu_state_size() {
        assert_eq!(core::mem::size_of::<X86_64FpuState>(), FXSAVE_SIZE);
    }

    #[test]
    fn test_fpu_state_alignment() {
        assert_eq!(core::mem::align_of::<X86_64FpuState>(), 16);
    }

    #[test]
    fn test_fpu_state_default_is_zeroed() {
        let state = X86_64FpuState::default();
        assert!(state.data.iter().all(|&b| b == 0));
    }
}
