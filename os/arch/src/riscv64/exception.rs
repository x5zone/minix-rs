//! riscv64 exception frame.
//!
//! On riscv64, the CPU saves `sepc` and `sstatus` on trap entry, and
//! the assembly trap entry saves all 32 general-purpose registers
//! (`x0..x31`) into the kernel stack's save area.
//!
//! This struct gives `CpuContextArch` something concrete to write into
//! at first dispatch.

use minix_types::VirBytes;
use crate::exception::{ExceptionArch, FaultContext, RecoveryPoint};
use crate::protection::InterruptVector;

/// Number of general-purpose registers (x0..x31).
const NUM_GPRS: usize = 32;

/// riscv64 exception frame.
///
/// Note: `x0` is hardwired to zero and not stored; we still allocate
/// the slot for layout simplicity.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Riscv64ExceptionFrame {
    pub vector: u64,
    pub errcode: u64,
    pub sepc: u64,
    pub sstatus: u64,
    pub stval: u64,
    pub regs: [u64; NUM_GPRS],
}

impl ExceptionArch for Riscv64ExceptionFrame {
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
        VirBytes::new(frame.sepc)
    }

    #[inline]
    fn is_user_mode(frame: &Self::Frame) -> bool {
        // sstatus.SPP bit (bit 8): 0 = U-mode, 1 = S-mode.
        (frame.sstatus & (1 << 8)) == 0
    }

    fn page_fault_address() -> VirBytes {
        // stval holds the faulting virtual address on a page fault.
        let stval: u64;
        // SAFETY: reading stval is side-effect-free.
        unsafe {
            core::arch::asm!("csrr {}, stval", out(reg) stval, options(nomem, nostack, preserves_flags));
        }
        VirBytes::new(stval)
    }

    #[inline]
    fn is_write_fault(frame: &Self::Frame) -> bool {
        // scause bit — for a page fault, scause = 15 (store page fault).
        // The full scause is in `errcode`; bit 0 (read vs write) is
        // implementation-defined on RISC-V, so we conservatively use
        // the lower bit of errcode.
        (frame.errcode & 1) != 0
    }

    #[inline]
    fn set_instruction_pointer(frame: &mut Self::Frame, ip: VirBytes) {
        frame.sepc = ip.get();
    }

    fn set_return_value(frame: &mut Self::Frame, value: u64) {
        // RISC-V return value register is a0 (x10).
        frame.regs[10] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_frame_size_is_documented() {
        // 5 * u64 (header) + 32 * u64 (regs) = 37 * 8 = 296 bytes.
        assert_eq!(core::mem::size_of::<Riscv64ExceptionFrame>(), 296);
    }

    #[test]
    fn is_user_mode_uses_spp_bit() {
        let mut f = Riscv64ExceptionFrame::default();
        f.sstatus = 0; // SPP=0 → U-mode
        assert!(Riscv64ExceptionFrame::is_user_mode(&f));
        f.sstatus = 1 << 8; // SPP=1 → S-mode
        assert!(!Riscv64ExceptionFrame::is_user_mode(&f));
    }

    #[test]
    fn set_instruction_pointer_writes_sepc() {
        let mut f = Riscv64ExceptionFrame::default();
        Riscv64ExceptionFrame::set_instruction_pointer(&mut f, VirBytes::new(0xCAFE));
        assert_eq!(f.sepc, 0xCAFE);
    }

    #[test]
    fn set_return_value_writes_a0() {
        let mut f = Riscv64ExceptionFrame::default();
        Riscv64ExceptionFrame::set_return_value(&mut f, 0x1234);
        assert_eq!(f.regs[10], 0x1234);
    }
}