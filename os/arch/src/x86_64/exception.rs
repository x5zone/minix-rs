//! x86-64 exception frame and ExceptionArch implementation
//!
//! Implements `ExceptionArch` for x86-64, providing exception frame parsing
//! and fault recovery point manipulation.
//!
//! # 64-bit long mode changes (see 14-exception-interrupt.md §3.7)
//!
//! - Exception frame fields are 64-bit (RIP, RFLAGS, RSP vs EIP, EFLAGS, ESP)
//! - CPU always pushes SS/RSP on privilege level change (even in 64-bit mode)
//! - CR2 is 64-bit

use crate::exception::ExceptionArch;
use crate::protection::InterruptVector;
use minix_types::VirBytes;

/// x86-64 exception frame.
///
/// Pushed by CPU (SS, RSP, RFLAGS, CS, RIP, Error Code) and
/// assembly entry (vector number).
///
/// C: exception_frame — arch_proto.h:72-80 (32-bit version)
/// 64-bit: RIP/RFLAGS/RSP are 64-bit; SS/CS are 16-bit but
/// stored in 64-bit slots for alignment.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct X86_64ExceptionFrame {
    pub vector: u64,
    pub errcode: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl ExceptionArch for X86_64ExceptionFrame {
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
        VirBytes::new(frame.rip)
    }

    #[inline]
    fn is_user_mode(frame: &Self::Frame) -> bool {
        (frame.cs & 3) == 3
    }

    fn page_fault_address() -> VirBytes {
        let cr2: u64;
        // SAFETY: Reading CR2 is a privileged but side-effect-free operation.
        // It only reads the page fault linear address written by the CPU on
        // a #PF exception. The `nomem` option correctly indicates no memory
        // is read or written; `nostack` indicates no stack changes;
        // `preserves_flags` indicates no flag register changes.
        unsafe {
            core::arch::asm!(
                "mov {}, cr2",
                out(reg) cr2,
                options(nomem, nostack, preserves_flags)
            );
        }
        VirBytes::new(cr2)
    }

    #[inline]
    fn is_write_fault(frame: &Self::Frame) -> bool {
        (frame.errcode & 2) != 0
    }

    #[inline]
    fn set_instruction_pointer(frame: &mut Self::Frame, ip: VirBytes) {
        frame.rip = ip.get();
    }

    fn set_return_value(frame: &mut Self::Frame, value: u64) {
        // C: pr->p_reg.retreg = pagefaultcr2 — exception.c:72
        //
        // Design limitation (x86-64): RAX is NOT in the CPU-pushed exception
        // frame (which contains only vector, errcode, rip, cs, rflags, rsp,
        // ss). RAX is saved by the assembly entry point into the process
        // register save area (`p_reg.retreg`), which is NOT accessible via
        // `&mut Self::Frame`.
        //
        // ARM64/RISC-V can implement this because their frames include the
        // return register (x0 / a0) saved by assembly. On x86-64, the caller
        // (exception handler) must set `proc.p_reg.retreg = value` directly
        // after calling this method — matching C's `pr->p_reg.retreg` write.
        //
        // This method is intentionally a no-op on x86-64. It exists to
        // satisfy the trait contract; the actual return-value write happens
        // at the call site via `&mut KProcess`.
        let _ = (frame, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_frame_size() {
        assert_eq!(
            core::mem::size_of::<X86_64ExceptionFrame>(),
            56,
            "7 x u64 = 56 bytes"
        );
    }

    #[test]
    fn vector_extraction() {
        let frame = X86_64ExceptionFrame {
            vector: 14,
            errcode: 0,
            rip: 0x1000,
            cs: 0x08,
            rflags: 0x202,
            rsp: 0x7FFF_0000,
            ss: 0x10,
        };
        assert_eq!(X86_64ExceptionFrame::vector(&frame).get(), 14);
    }

    #[test]
    fn is_user_mode_kernel() {
        let frame = X86_64ExceptionFrame {
            vector: 13,
            errcode: 0,
            rip: 0,
            cs: 0x08,
            rflags: 0,
            rsp: 0,
            ss: 0,
        };
        assert!(!X86_64ExceptionFrame::is_user_mode(&frame));
    }

    #[test]
    fn is_user_mode_user() {
        let frame = X86_64ExceptionFrame {
            vector: 14,
            errcode: 0,
            rip: 0,
            cs: 0x1B,
            rflags: 0,
            rsp: 0,
            ss: 0,
        };
        assert!(X86_64ExceptionFrame::is_user_mode(&frame));
    }

    #[test]
    fn is_write_fault_read() {
        let frame = X86_64ExceptionFrame {
            vector: 14,
            errcode: 0,
            rip: 0,
            cs: 0,
            rflags: 0,
            rsp: 0,
            ss: 0,
        };
        assert!(!X86_64ExceptionFrame::is_write_fault(&frame));
    }

    #[test]
    fn is_write_fault_write() {
        let frame = X86_64ExceptionFrame {
            vector: 14,
            errcode: 2,
            rip: 0,
            cs: 0,
            rflags: 0,
            rsp: 0,
            ss: 0,
        };
        assert!(X86_64ExceptionFrame::is_write_fault(&frame));
    }

    #[test]
    fn set_instruction_pointer() {
        let mut frame = X86_64ExceptionFrame {
            vector: 14,
            errcode: 0,
            rip: 0x1000,
            cs: 0,
            rflags: 0,
            rsp: 0,
            ss: 0,
        };
        X86_64ExceptionFrame::set_instruction_pointer(
            &mut frame,
            VirBytes::new(0x2000),
        );
        assert_eq!(frame.rip, 0x2000);
    }
}
