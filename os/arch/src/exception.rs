//! Exception architecture abstraction
//!
//! Defines the trait interface for exception frame parsing and fault recovery.
//!
//! # Design decisions (see 05-exception-interrupt.md §3.3, §3.6)
//!
//! - **Associated type Frame** (§3.3): Each architecture has its own exception
//!   frame layout. A common struct would waste fields on most architectures.
//! - **page_fault_address() is static** (§3.3): On x86-64, the fault address
//!   is in CR2 (not in the frame). ARM64 uses FAR_EL1, RISC-V uses stval.
//! - **set_instruction_pointer / set_return_value** (§3.3): Needed for nested
//!   exception fault recovery — redirecting execution to a recovery point.

use crate::protection::InterruptVector;
use minix_types::VirBytes;

/// Architecture abstraction for exception frame parsing and fault recovery.
///
/// Provides methods to extract information from the CPU-pushed exception
/// frame and to modify it for fault recovery (e.g., redirecting execution
/// to a recovery point after a nested page fault in phys_copy).
///
/// # Architecture mapping
///
/// | Method                  | x86-64                    | ARM64              | RISC-V          |
/// |------------------------|---------------------------|--------------------|-----------------|
/// | `vector()`             | frame->vector             | ESR_EL1.EC         | scause.code      |
/// | `error_code()`         | frame->errcode            | ESR_EL1.ISS        | stval            |
/// | `instruction_pointer()`| frame->rip                | ELR_EL1            | sepc             |
/// | `is_user_mode()`       | frame->cs & 3             | SPSR_EL1.M[3:0]    | sstatus.SPP      |
/// | `page_fault_address()` | read CR2                  | read FAR_EL1       | read stval       |
/// | `set_instruction_      | frame->rip = addr         | ELR_EL1 = addr     | sepc = addr      |
/// |  pointer()`            |                           |                    |                  |
/// | `set_return_value()`   | p_reg.retreg = value      | x0 = value         | a0 = value       |
///
/// C: exception_handler() — exception.c:180-283
pub trait ExceptionArch {
    /// Architecture-specific exception frame type.
    ///
    /// x86-64: struct with vector, errcode, rip, cs, rflags, rsp, ss
    /// ARM64:  struct with esr, far, elr, spsr, sp
    /// RISC-V: struct with scause, stval, sepc, sstatus, sp
    type Frame;

    /// Get the interrupt/exception vector number from the frame.
    ///
    /// C: frame->vector — exception.c:182
    fn vector(frame: &Self::Frame) -> InterruptVector;

    /// Get the error code pushed by the CPU (or 0 if none).
    ///
    /// C: frame->errcode — exception.c:183
    fn error_code(frame: &Self::Frame) -> u64;

    /// Get the instruction pointer where the exception occurred.
    ///
    /// C: frame->eip — exception.c:186
    fn instruction_pointer(frame: &Self::Frame) -> VirBytes;

    /// Check whether the exception occurred in user mode.
    ///
    /// C: (frame->cs & 3) == USER_PRIVILEGE — exception.c:191
    fn is_user_mode(frame: &Self::Frame) -> bool;

    /// Read the page fault address from the CPU's fault address register.
    ///
    /// On x86-64, this reads CR2. On ARM64, this reads FAR_EL1.
    /// On RISC-V, this reads stval.
    ///
    /// C: read_cr2() — exception.c:59
    fn page_fault_address() -> VirBytes;

    /// Check if the page fault was caused by a write access.
    ///
    /// On x86-64, checks bit 1 of the error code.
    /// On ARM64, checks the WnR bit of ESR_EL1.ISS.
    ///
    /// C: (frame->errcode & 2) — interpreted by VM
    fn is_write_fault(frame: &Self::Frame) -> bool;

    /// Modify the instruction pointer in the frame for fault recovery.
    ///
    /// Used by nested exception handlers to redirect execution to
    /// a recovery point (e.g., phys_copy_fault_in_kernel).
    ///
    /// C: frame->eip = (reg_t) phys_copy_fault_in_kernel — exception.c:68
    fn set_instruction_pointer(frame: &mut Self::Frame, ip: VirBytes);

    /// Set a return value in the frame (architecture-specific register).
    ///
    /// On x86-64, this sets the return register (eax/rax).
    /// Used to pass the fault address back to the recovery point.
    ///
    /// C: pr->p_reg.retreg = pagefaultcr2 — exception.c:72
    fn set_return_value(frame: &mut Self::Frame, value: u64);
}

/// Fault recovery context for nested exceptions.
///
/// Replaces C's address-range comparison (`frame->eip > phys_copy && ...`)
/// with an explicit context enum. Set before entering recoverable
/// operations, cleared on exit.
///
/// C: catch_pagefaults + address range comparison — exception.c:59-73
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultContext {
    Normal,
    PhysCopy,
    Memset,
    UserCopyMsg,
    FpuRestore,
}

/// Fault recovery points for nested exception handling.
///
/// C: phys_copy_fault_in_kernel, memset_fault_in_kernel,
///    __user_copy_msg_pointer_failure, __frstor_failure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPoint {
    PhysCopyFaultInKernel,
    MemsetFaultInKernel,
    UserCopyMsgFailure,
    FpuRestoreFailure,
}

impl FaultContext {
    /// Get the recovery point for this fault context.
    ///
    /// Maps each fault context to its corresponding recovery point.
    /// Used by the exception handler to determine where to redirect
    /// execution when a nested page fault occurs.
    ///
    /// C: exception.c:59-73 (address range comparison)
    pub fn recovery_point(self) -> Option<RecoveryPoint> {
        match self {
            FaultContext::PhysCopy => Some(RecoveryPoint::PhysCopyFaultInKernel),
            FaultContext::Memset => Some(RecoveryPoint::MemsetFaultInKernel),
            FaultContext::UserCopyMsg => Some(RecoveryPoint::UserCopyMsgFailure),
            FaultContext::FpuRestore => Some(RecoveryPoint::FpuRestoreFailure),
            FaultContext::Normal => None,
        }
    }
}

/// Per-CPU fault context tracker.
///
/// Replaces C's `catch_pagefaults` global flag and address-range
/// comparison. Before entering a recoverable operation (phys_copy,
/// memset, etc.), the kernel sets the current `FaultContext`. On
/// exit, it clears it back to `Normal`. If a nested page fault
/// occurs, the exception handler reads the current context and
/// uses `recovery_point()` to determine where to redirect execution.
///
/// # BKL safety
///
/// All modifications happen under BKL in the syscall path. The
/// exception handler also runs under BKL (or with interrupts disabled
/// on the current CPU). No additional synchronization is needed.
///
/// C: catch_pagefaults — exception.c:42
pub struct FaultContextTracker {
    context: FaultContext,
}

impl FaultContextTracker {
    pub const fn new() -> Self {
        Self {
            context: FaultContext::Normal,
        }
    }

    pub fn current(&self) -> FaultContext {
        self.context
    }

    pub fn enter(&mut self, ctx: FaultContext) {
        self.context = ctx;
    }

    pub fn leave(&mut self) {
        self.context = FaultContext::Normal;
    }
}
