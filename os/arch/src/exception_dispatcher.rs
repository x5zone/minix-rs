//! Architecture-independent exception dispatcher
//!
//! Dispatches exceptions based on vector number and fault context.
//! Page faults are forwarded to VM; user-mode exceptions generate
//! signals; kernel-mode exceptions in recoverable contexts redirect
//! execution; all other kernel exceptions panic.
//!
//! # Design decisions (see 05-exception-interrupt.md §3.5, §3.6)
//!
//! - **Returns ExceptionOutcome enum** (§3.5): Separates "decide what to do"
//!   (dispatch) from "do it" (signal/send/panic). C's exception_handler()
//!   directly calls cause_sig()/mini_send()/inkernel_disaster().
//! - **FaultContext enum** (§3.6): Replaces C's address-range comparison
//!   with explicit context tracking.
//! - **ExceptionClass enum** (§3.5): Replaces C's ex_data[] static array
//!   with a typed classification.

use crate::exception::{ExceptionArch, FaultContext, RecoveryPoint};
use crate::protection::InterruptVector;
use minix_types::ipc::VmPagefaultIn;
use minix_types::VirBytes;

/// Exception classification for dispatch.
///
/// C: ex_data[] — exception.c:19-39
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionClass {
    NonMaskableInterrupt,
    PageFault,
    Debug,
    Signal(ExceptionSignal),
}

/// Signal type derived from exception classification.
///
/// Subset of POSIX signals that x86 exceptions map to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionSignal {
    Fpe,
    Ill,
    Segv,
    Bus,
    Emt,
    Trap,
}

/// Outcome of exception dispatch.
///
/// Replaces C's mix of returns, panics, and direct function calls
/// with a structured enum that the caller can pattern-match on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionOutcome {
    SpuriousNmi,
    Signal(ExceptionSignal),
    ForwardToVm(VmPagefaultIn),
    RedirectToRecovery(RecoveryPoint),
    PhysCopyFault { fault_addr: VirBytes },
    VmPageFault,
    KernelPanic(InterruptVector),
    ClearTrapFlag,
}

/// Kernel trap style — how the process entered the kernel.
///
/// C: KTS_* — archconst.h:167-173
/// 64-bit: SYSENTER is removed; only KTS_NONE, KTS_SYSCALL, and
/// KTS_INT_HARD remain relevant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernTrapStyle {
    None,
    IntHard,
    Syscall,
    Other,
}

/// Architecture-independent exception dispatcher.
///
/// Dispatches exceptions based on vector number and fault context.
/// Page faults are forwarded to VM; user-mode exceptions generate
/// signals; kernel-mode exceptions in recoverable contexts redirect
/// execution; all other kernel exceptions panic.
///
/// C: exception_handler() — exception.c:180-283
pub struct ExceptionDispatcher<EA: ExceptionArch> {
    _phantom: core::marker::PhantomData<EA>,
}

impl<EA: ExceptionArch> ExceptionDispatcher<EA> {
    /// Main exception dispatch entry point.
    ///
    /// C: exception_handler(is_nested, frame) — exception.c:180
    ///
    /// `is_traced` indicates whether the current process has the
    /// trace flag (TF/TRACEBIT) set in its saved PSW.
    /// `kern_trap_style` indicates how the process entered the kernel.
    /// Both are needed for the nested debug exception check:
    /// C: exception.c:231-234
    pub fn handle(
        frame: &mut EA::Frame,
        is_nested: bool,
        is_vm: bool,
        fault_ctx: FaultContext,
        is_traced: bool,
        kern_trap_style: KernTrapStyle,
    ) -> ExceptionOutcome {
        let vector = EA::vector(frame);
        let is_user = EA::is_user_mode(frame);

        if vector.get() == 2 {
            return ExceptionOutcome::SpuriousNmi;
        }

        if is_nested {
            return Self::handle_nested(frame, vector, fault_ctx, is_traced, kern_trap_style);
        }

        if vector.get() == 14 {
            return Self::handle_page_fault(frame, is_nested, is_vm, fault_ctx);
        }

        if is_user {
            return Self::classify_signal(vector);
        }

        ExceptionOutcome::KernelPanic(vector)
    }

    /// Handle nested (kernel-mode) exception.
    ///
    /// C: exception.c:189-249
    fn handle_nested(
        frame: &mut EA::Frame,
        vector: InterruptVector,
        fault_ctx: FaultContext,
        is_traced: bool,
        kern_trap_style: KernTrapStyle,
    ) -> ExceptionOutcome {
        if fault_ctx == FaultContext::UserCopyMsg {
            let is_pf_or_gpf = vector.get() == 14 || vector.get() == 13;
            if is_pf_or_gpf {
                return ExceptionOutcome::RedirectToRecovery(
                    RecoveryPoint::UserCopyMsgFailure,
                );
            }
        }

        if fault_ctx == FaultContext::FpuRestore {
            return ExceptionOutcome::RedirectToRecovery(
                RecoveryPoint::FpuRestoreFailure,
            );
        }

        // C: exception.c:231-234 — debug exception in kernel is legitimate
        // only if the process is traced and entered via SYSENTER/SYSCALL
        // (trap style is still KTS_NONE at the first kernel entry).
        if vector.get() == 1 && is_traced && kern_trap_style == KernTrapStyle::None {
            return ExceptionOutcome::ClearTrapFlag;
        }

        if vector.get() == 14 {
            return Self::handle_page_fault(frame, true, false, fault_ctx);
        }

        ExceptionOutcome::KernelPanic(vector)
    }

    /// Handle page fault.
    ///
    /// C: pagefault() — exception.c:49-130
    fn handle_page_fault(
        frame: &mut EA::Frame,
        is_nested: bool,
        is_vm: bool,
        fault_ctx: FaultContext,
    ) -> ExceptionOutcome {
        let fault_addr = EA::page_fault_address();
        let is_write = EA::is_write_fault(frame);

        // C also checks `catch_pagefaults` counter before recovering;
        // omitted here because FaultContext enum precisely tracks the
        // recoverable context (see §3.6 in 05-exception-interrupt.md).
        // C: catch_pagefaults && (in_physcopy || in_memset) — exception.c:62-73
        if fault_ctx == FaultContext::PhysCopy || fault_ctx == FaultContext::Memset {
            if is_nested {
                let recovery = if fault_ctx == FaultContext::PhysCopy {
                    RecoveryPoint::PhysCopyFaultInKernel
                } else {
                    RecoveryPoint::MemsetFaultInKernel
                };
                return ExceptionOutcome::RedirectToRecovery(recovery);
            } else {
                return ExceptionOutcome::PhysCopyFault { fault_addr };
            }
        }

        if is_nested {
            return ExceptionOutcome::KernelPanic(InterruptVector::new(14));
        }

        if is_vm {
            return ExceptionOutcome::VmPageFault;
        }

        ExceptionOutcome::ForwardToVm(VmPagefaultIn {
            endpoint: minix_types::Endpoint::NONE,
            vaddr: fault_addr,
            write: is_write,
        })
    }

    /// Classify an exception vector into a signal.
    ///
    /// C: ex_data[] — exception.c:19-39
    fn classify_signal(vector: InterruptVector) -> ExceptionOutcome {
        use ExceptionSignal::*;
        let sig = match vector.get() {
            0 => Fpe,
            1 => Trap,
            3 => Emt,
            4 => Fpe,
            5 => Fpe,
            6 => Ill,
            7 => Fpe,
            8 => Bus,
            9 => Segv,
            10 => Segv,
            11 => Segv,
            12 => Segv,
            13 => Segv,
            15 => Ill,
            16 => Fpe,
            17 => Bus,
            18 => Bus,
            19 => Fpe,
            _ => Segv,
        };
        ExceptionOutcome::Signal(sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exception::ExceptionArch;
    use crate::protection::InterruptVector;
    use minix_types::VirBytes;

    struct MockFrame {
        vector: u8,
        errcode: u64,
        rip: u64,
        cs: u64,
    }

    impl ExceptionArch for MockFrame {
        type Frame = Self;

        fn vector(frame: &Self) -> InterruptVector {
            InterruptVector::new(frame.vector)
        }
        fn error_code(frame: &Self) -> u64 {
            frame.errcode
        }
        fn instruction_pointer(frame: &Self) -> VirBytes {
            VirBytes::new(frame.rip)
        }
        fn is_user_mode(frame: &Self) -> bool {
            (frame.cs & 3) == 3
        }
        fn page_fault_address() -> VirBytes {
            VirBytes::new(0xDEAD_0000)
        }
        fn is_write_fault(frame: &Self) -> bool {
            (frame.errcode & 2) != 0
        }
        fn set_instruction_pointer(_frame: &mut Self, _ip: VirBytes) {}
        fn set_return_value(_frame: &mut Self, _value: u64) {}
    }

    #[test]
    fn spurious_nmi() {
        let mut frame = MockFrame { vector: 2, errcode: 0, rip: 0, cs: 0 };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, false, false, FaultContext::Normal, false, KernTrapStyle::None,
        );
        assert_eq!(outcome, ExceptionOutcome::SpuriousNmi);
    }

    #[test]
    fn user_divide_error() {
        let mut frame = MockFrame { vector: 0, errcode: 0, rip: 0, cs: 0x1B };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, false, false, FaultContext::Normal, false, KernTrapStyle::None,
        );
        assert!(matches!(outcome, ExceptionOutcome::Signal(ExceptionSignal::Fpe)));
    }

    #[test]
    fn user_page_fault() {
        let mut frame = MockFrame { vector: 14, errcode: 2, rip: 0x1000, cs: 0x1B };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, false, false, FaultContext::Normal, false, KernTrapStyle::None,
        );
        assert!(matches!(outcome, ExceptionOutcome::ForwardToVm(_)));
    }

    #[test]
    fn vm_page_fault() {
        let mut frame = MockFrame { vector: 14, errcode: 0, rip: 0x1000, cs: 0x1B };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, false, true, FaultContext::Normal, false, KernTrapStyle::None,
        );
        assert_eq!(outcome, ExceptionOutcome::VmPageFault);
    }

    #[test]
    fn kernel_panic() {
        let mut frame = MockFrame { vector: 13, errcode: 0, rip: 0, cs: 0x08 };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, false, false, FaultContext::Normal, false, KernTrapStyle::None,
        );
        assert!(matches!(outcome, ExceptionOutcome::KernelPanic(_)));
    }

    #[test]
    fn nested_user_copy_msg() {
        let mut frame = MockFrame { vector: 14, errcode: 0, rip: 0, cs: 0x08 };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, true, false, FaultContext::UserCopyMsg, false, KernTrapStyle::None,
        );
        assert_eq!(
            outcome,
            ExceptionOutcome::RedirectToRecovery(RecoveryPoint::UserCopyMsgFailure)
        );
    }

    #[test]
    fn nested_phys_copy() {
        let mut frame = MockFrame { vector: 14, errcode: 0, rip: 0, cs: 0x08 };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, true, false, FaultContext::PhysCopy, false, KernTrapStyle::None,
        );
        assert_eq!(
            outcome,
            ExceptionOutcome::RedirectToRecovery(RecoveryPoint::PhysCopyFaultInKernel)
        );
    }

    #[test]
    fn nested_fpu_restore() {
        let mut frame = MockFrame { vector: 7, errcode: 0, rip: 0, cs: 0x08 };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, true, false, FaultContext::FpuRestore, false, KernTrapStyle::None,
        );
        assert_eq!(
            outcome,
            ExceptionOutcome::RedirectToRecovery(RecoveryPoint::FpuRestoreFailure)
        );
    }

    #[test]
    fn nested_debug_clear_tf() {
        let mut frame = MockFrame { vector: 1, errcode: 0, rip: 0, cs: 0x08 };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, true, false, FaultContext::Normal, true, KernTrapStyle::None,
        );
        assert_eq!(outcome, ExceptionOutcome::ClearTrapFlag);
    }

    #[test]
    fn nested_debug_not_traced() {
        let mut frame = MockFrame { vector: 1, errcode: 0, rip: 0, cs: 0x08 };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, true, false, FaultContext::Normal, false, KernTrapStyle::None,
        );
        assert!(matches!(outcome, ExceptionOutcome::KernelPanic(_)));
    }

    #[test]
    fn nested_debug_traced_not_kts_none() {
        let mut frame = MockFrame { vector: 1, errcode: 0, rip: 0, cs: 0x08 };
        let outcome = ExceptionDispatcher::<MockFrame>::handle(
            &mut frame, true, false, FaultContext::Normal, true, KernTrapStyle::Syscall,
        );
        assert!(matches!(outcome, ExceptionOutcome::KernelPanic(_)));
    }

    #[test]
    fn classify_all_vectors() {
        let expected: &[(u8, ExceptionSignal)] = &[
            (0, ExceptionSignal::Fpe),
            (1, ExceptionSignal::Trap),
            (3, ExceptionSignal::Emt),
            (4, ExceptionSignal::Fpe),
            (5, ExceptionSignal::Fpe),
            (6, ExceptionSignal::Ill),
            (7, ExceptionSignal::Fpe),
            (8, ExceptionSignal::Bus),
            (9, ExceptionSignal::Segv),
            (10, ExceptionSignal::Segv),
            (11, ExceptionSignal::Segv),
            (12, ExceptionSignal::Segv),
            (13, ExceptionSignal::Segv),
            (15, ExceptionSignal::Ill),
            (16, ExceptionSignal::Fpe),
            (17, ExceptionSignal::Bus),
            (18, ExceptionSignal::Bus),
            (19, ExceptionSignal::Fpe),
        ];
        for &(v, expected_sig) in expected {
            let outcome = ExceptionDispatcher::<MockFrame>::classify_signal(
                InterruptVector::new(v),
            );
            assert_eq!(outcome, ExceptionOutcome::Signal(expected_sig),
                "vector {} signal mismatch", v);
        }
    }
}
