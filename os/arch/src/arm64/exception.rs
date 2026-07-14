//! aarch64 exception frame.
//!
//! On aarch64, the CPU pushes a fixed set of registers on exception
//! entry (EL1h mode), and the assembly trap entry saves the general-
//! purpose registers into the kernel stack's save area.
//!
//! This struct mirrors the C `frame` struct used by `arch_proc_reset`
//! / `arch_proc_init` / `arch_boot_proc` and gives `CpuContextArch`
//! something concrete to write into.

use minix_types::VirBytes;
use crate::exception::{ExceptionArch, FaultContext, RecoveryPoint};
use crate::protection::InterruptVector;

/// Number of general-purpose registers saved on trap entry.
///
/// GPRs x0..x30 + sp_el0 — total 32. (x31 is the stack pointer mirror
/// in some contexts; we don't store it here separately.)
const NUM_GPRS: usize = 32;

/// aarch64 exception frame.
///
/// Layout (simplified):
/// - `regs[0]`  = return value / ps_strings (x0)
/// - `regs[1..30]` = scratch / argument registers
/// - `regs[30]` = LR (x30)
/// - `regs[31]` = SP_EL0 (user stack pointer)
/// - `spsr_el1` = saved program status register
/// - `elr_el1` = saved program counter (return address)
/// - `sp_el0`  = user stack pointer (separate slot)
/// - `vector`  = exception vector number
/// - `errcode` = syndrome / error code from the CPU
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct AArch64ExceptionFrame {
    pub vector: u64,
    pub errcode: u64,
    pub spsr_el1: u64,
    pub elr_el1: u64,
    pub sp_el0: u64,
    pub regs: [u64; NUM_GPRS],
}

impl ExceptionArch for AArch64ExceptionFrame {
    type Frame = Self;

    #[inline]
    fn vector(frame: &Self::Frame) -> InterruptVector {
        InterruptVector::new(frame.vector as u8)
    }

    #[inline]
    fn error_code(frame: &Self::Frame) -> u64 {
        frame.errcode
    }

    #[inline]
    fn instruction_pointer(frame: &Self::Frame) -> VirBytes {
        VirBytes::new(frame.elr_el1)
    }

    #[inline]
    fn is_user_mode(frame: &Self::Frame) -> bool {
        // SPSR_EL1.M[3:0] = 0b0000 (User) on exception entry from EL0.
        (frame.spsr_el1 & 0xF) == 0
    }

    fn page_fault_address() -> VirBytes {
        // FAR_EL1 holds the faulting virtual address on aarch64.
        let far: u64;
        // SAFETY: reading FAR_EL1 is a privileged but side-effect-free
        // operation. `nomem` / `nostack` / `preserves_flags` describe
        // the asm invocation correctly.
        unsafe {
            core::arch::asm!("mrs {}, FAR_EL1", out(reg) far, options(nomem, nostack, preserves_flags));
        }
        VirBytes::new(far)
    }

    #[inline]
    fn is_write_fault(frame: &Self::Frame) -> bool {
        // ESR_EL1.WnR is bit 6 of the instruction-specific syndrome.
        (frame.errcode & (1 << 6)) != 0
    }

    #[inline]
    fn set_instruction_pointer(frame: &mut Self::Frame, ip: VirBytes) {
        frame.elr_el1 = ip.get();
    }

    fn set_return_value(frame: &mut Self::Frame, value: u64) {
        frame.regs[0] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_frame_size_is_documented() {
        // 5 * u64 (header) + 32 * u64 (regs) = 37 * 8 = 296 bytes.
        assert_eq!(core::mem::size_of::<AArch64ExceptionFrame>(), 296);
    }

    #[test]
    fn is_user_mode_el0() {
        let mut f = AArch64ExceptionFrame::default();
        f.spsr_el1 = 0; // M=EL0t
        assert!(AArch64ExceptionFrame::is_user_mode(&f));
        f.spsr_el1 = 5; // M=EL1h
        assert!(!AArch64ExceptionFrame::is_user_mode(&f));
    }

    #[test]
    fn set_instruction_pointer_writes_elr_el1() {
        let mut f = AArch64ExceptionFrame::default();
        AArch64ExceptionFrame::set_instruction_pointer(&mut f, VirBytes::new(0xCAFE));
        assert_eq!(f.elr_el1, 0xCAFE);
    }

    #[test]
    fn set_return_value_writes_x0() {
        let mut f = AArch64ExceptionFrame::default();
        AArch64ExceptionFrame::set_return_value(&mut f, 0x1234);
        assert_eq!(f.regs[0], 0x1234);
    }
}