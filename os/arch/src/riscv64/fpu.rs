//! RISC-V 64-bit FPU implementation using F/D extension registers.
//!
//! Implements `FpuArch` for RISC-V 64. Uses the F/D extension registers
//! (f0-f31) and the FCSR (Floating-point Control and Status Register).
//!
//! # FPU state buffer
//!
//! The F/D extension state consists of:
//! - 32 × 64-bit floating-point registers (f0-f31): 256 bytes
//! - FCSR (4 bytes)
//! Total: 260 bytes (padded to 264 for 8-byte alignment)
//!
//! # Lazy FPU on RISC-V
//!
//! RISC-V controls FPU access via `sstatus.FS` (bits 13-14):
//! - 0b00 (Off): FPU instructions trap
//! - 0b01 (Initial): FPU available, registers have initial values
//! - 0b10 (Clean): FPU available, registers unchanged since last save
//! - 0b11 (Dirty): FPU available, registers modified (must save on switch)
//!
//! The kernel sets FS=Initial at boot and uses explicit save/restore
//! during context switch.
//!
//! C: No Minix3 equivalent (Minix3 has no RISC-V port). Based on the
//! RISC-V Privileged ISA Specification v1.11.

use crate::fpu_arch::FpuArch;

/// F/D extension state size: 32 × 8 bytes (f0-f31) + 4 (FCSR) + 4 (pad) = 264.
const FPU_SIZE: usize = 264;

/// RISC-V F/D extension state buffer.
///
/// Contains 32 × 64-bit floating-point registers + FCSR.
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct Riscv64FpuState {
    /// 32 × 64-bit floating-point registers (f0-f31).
    regs: [u64; 32],
    /// FCSR (Floating-point Control and Status Register).
    fcsr: u32,
    /// Padding to 264 bytes (256 + 4 + 4 = 264).
    _pad: u32,
}

impl core::fmt::Debug for Riscv64FpuState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Riscv64FpuState")
            .field("fcsr", &self.fcsr)
            .finish()
    }
}

impl Default for Riscv64FpuState {
    fn default() -> Self {
        Self {
            regs: [0u64; 32],
            fcsr: 0,
            _pad: 0,
        }
    }
}

impl Riscv64FpuState {
    /// Const-constructible zeroed state (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            regs: [0u64; 32],
            fcsr: 0,
            _pad: 0,
        }
    }
}

/// RISC-V 64-bit FPU implementation using F/D extension instructions.
#[derive(Debug, Default, Clone, Copy)]
pub struct Riscv64FpuArch;

impl FpuArch for Riscv64FpuArch {
    type State = Riscv64FpuState;

    fn init(&self) {
        // Enable FPU by setting sstatus.FS = Initial (0b01).
        // C: No Minix3 equivalent; based on RISC-V Privileged ISA v1.11
        unsafe {
            // sstatus.FS is bits 13:14. Set to 0b01 (Initial).
            // Read sstatus, clear FS bits, set to 0b01.
            let mut sstatus: u64;
            core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
            sstatus &= !(0b11 << 13); // Clear FS
            sstatus |= 0b01 << 13;     // Set FS = Initial
            core::arch::asm!("csrw sstatus, {}", in(reg) sstatus);
        }
    }

    fn save(&self, dst: &mut Self::State) {
        // Save FPU state: 32 f registers + FCSR.
        // RISC-V uses `fsd` (store double) for each register and `frcsr` for FCSR.
        //
        // SAFETY: `dst` is 8-byte aligned. Caller must hold BKL.
        unsafe {
            let ptr = dst.regs.as_mut_ptr() as *mut u64;
            // Save f0-f31 using fsd with offset addressing
            core::arch::asm!(
                "fsd f0, 0({ptr})",
                "fsd f1, 8({ptr})",
                "fsd f2, 16({ptr})",
                "fsd f3, 24({ptr})",
                "fsd f4, 32({ptr})",
                "fsd f5, 40({ptr})",
                "fsd f6, 48({ptr})",
                "fsd f7, 56({ptr})",
                "fsd f8, 64({ptr})",
                "fsd f9, 72({ptr})",
                "fsd f10, 80({ptr})",
                "fsd f11, 88({ptr})",
                "fsd f12, 96({ptr})",
                "fsd f13, 104({ptr})",
                "fsd f14, 112({ptr})",
                "fsd f15, 120({ptr})",
                "fsd f16, 128({ptr})",
                "fsd f17, 136({ptr})",
                "fsd f18, 144({ptr})",
                "fsd f19, 152({ptr})",
                "fsd f20, 160({ptr})",
                "fsd f21, 168({ptr})",
                "fsd f22, 176({ptr})",
                "fsd f23, 184({ptr})",
                "fsd f24, 192({ptr})",
                "fsd f25, 200({ptr})",
                "fsd f26, 208({ptr})",
                "fsd f27, 216({ptr})",
                "fsd f28, 224({ptr})",
                "fsd f29, 232({ptr})",
                "fsd f30, 240({ptr})",
                "fsd f31, 248({ptr})",
                "frcsr {fcsr}",
                ptr = in(reg) ptr,
                fcsr = out(reg) dst.fcsr,
                options(nostack),
            );
        }
    }

    fn restore(&self, src: &Self::State) {
        // Restore FPU state: 32 f registers + FCSR.
        // RISC-V uses `fld` (load double) for each register and `fscsr` for FCSR.
        //
        // SAFETY: `src` is 8-byte aligned and contains valid FPU data.
        // Caller must hold BKL.
        unsafe {
            let ptr = src.regs.as_ptr() as *const u64;
            let fcsr = src.fcsr;
            core::arch::asm!(
                "fld f0, 0({ptr})",
                "fld f1, 8({ptr})",
                "fld f2, 16({ptr})",
                "fld f3, 24({ptr})",
                "fld f4, 32({ptr})",
                "fld f5, 40({ptr})",
                "fld f6, 48({ptr})",
                "fld f7, 56({ptr})",
                "fld f8, 64({ptr})",
                "fld f9, 72({ptr})",
                "fld f10, 80({ptr})",
                "fld f11, 88({ptr})",
                "fld f12, 96({ptr})",
                "fld f13, 104({ptr})",
                "fld f14, 112({ptr})",
                "fld f15, 120({ptr})",
                "fld f16, 128({ptr})",
                "fld f17, 136({ptr})",
                "fld f18, 144({ptr})",
                "fld f19, 152({ptr})",
                "fld f20, 160({ptr})",
                "fld f21, 168({ptr})",
                "fld f22, 176({ptr})",
                "fld f23, 184({ptr})",
                "fld f24, 192({ptr})",
                "fld f25, 200({ptr})",
                "fld f26, 208({ptr})",
                "fld f27, 216({ptr})",
                "fld f28, 224({ptr})",
                "fld f29, 232({ptr})",
                "fld f30, 240({ptr})",
                "fld f31, 248({ptr})",
                "fscsr {fcsr}",
                ptr = in(reg) ptr,
                fcsr = in(reg) fcsr,
                options(nostack),
            );
        }
    }

    fn enable(&self) {
        // Set sstatus.FS = Initial (0b01) to enable FPU.
        unsafe {
            let mut sstatus: u64;
            core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
            sstatus &= !(0b11 << 13);
            sstatus |= 0b01 << 13;
            core::arch::asm!("csrw sstatus, {}", in(reg) sstatus);
        }
    }

    fn disable(&self) {
        // Set sstatus.FS = Off (0b00) to trap on next FPU instruction.
        unsafe {
            let mut sstatus: u64;
            core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
            sstatus &= !(0b11 << 13); // Clear FS → Off
            core::arch::asm!("csrw sstatus, {}", in(reg) sstatus);
        }
    }

    fn disable_exception(&self) {
        // RISC-V does not have a separate FPU exception enable/disable.
        // FPU exceptions are controlled via FCSR flags, not sstatus.
        // No-op.
    }

    fn is_present(&self) -> bool {
        // Check if the F extension is present by reading misa.
        // If misa has bit 5 (F) set, the FPU is present.
        // On RISC-V 64-bit targets compiled with `riscv64gc`, the FPU
        // is always present.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fpu_state_size() {
        // 32 * 8 (regs) + 4 (fcsr) + 4 (pad) = 264
        assert_eq!(core::mem::size_of::<Riscv64FpuState>(), FPU_SIZE);
    }

    #[test]
    fn test_fpu_state_alignment() {
        assert_eq!(core::mem::align_of::<Riscv64FpuState>(), 8);
    }

    #[test]
    fn test_fpu_state_default_is_zeroed() {
        let state = Riscv64FpuState::default();
        assert_eq!(state.fcsr, 0);
        assert!(state.regs.iter().all(|&r| r == 0));
    }
}
