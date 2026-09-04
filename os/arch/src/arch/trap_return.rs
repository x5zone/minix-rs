//! Trap return abstraction: OS-level concern of "how the kernel re-enters user code"
//!
//! # Design principle
//!
//! This is the **return-path twin** of [`TrapEntryArch`](crate::trap_entry::TrapEntryArch)
//! (doc 10-switch-to-user.md §3, decision row `restore_user_context`). Trap entry
//! answers "how does the CPU reach the kernel"; this trait answers the mirror
//! question: "how does the kernel hand the CPU back to a user process".
//!
//! - **OS layer cares about WHAT**: the scheduler's last act is "restore this
//!   frame + register file and transfer control to user mode, never returning".
//!   The OS never writes `iretq`, `eret`, or `sret`, and never names
//!   segment selectors or system registers.
//! - **Architecture layer handles HOW**: each impl assembles the CPU-level
//!   mode switch (x86-64 `iretq`, aarch64 `eret`, riscv64 `sret`) from the
//!   two inputs and guarantees the caller-visible contract below.
//!
//! # The two inputs
//!
//! Minix3 keeps a process's whole saved CPU state in one struct (`p_reg`,
//! the `stackframe_s` on the process kernel stack). minix-rs splits it
//! along the same seam the exception layer already uses:
//!
//! - **`Frame`** — the CPU/architecture-assembled part that the mode-switch
//!   instruction consumes: on x86-64 the `iretq` payload
//!   (RIP/CS/RFLAGS/RSP/SS); on aarch64 SPSR_EL1/ELR_EL1/SP_EL0; on
//!   riscv64 sstatus/sepc. The scheduler builds it from the process's saved
//!   state via `CpuContextArch::apply_to_trap_frame` before calling
//!   [`restore_to_user`](TrapReturnArch::restore_to_user) (first dispatch and
//!   every re-dispatch alike — the `cpu_context` plays the role of C's
//!   `p_reg`, which is simultaneously initial state and saved state).
//! - **`RegisterFile`** — everything *beyond* the frame that the mode switch
//!   needs: on x86-64 the general-purpose registers plus the data segment
//!   selectors (they live outside the CPU-pushed frame); on aarch64/riscv64
//!   the frames are self-contained, but the register file is still passed so
//!   that OS code has a single, architecture-uniform calling convention.
//!
//! # Caller-visible contract
//!
//! [`restore_to_user`](TrapReturnArch::restore_to_user) is the kernel's last
//! statement. Its contract matches what C guarantees in
//! `arch_finish_switch_to_user()` (arch_system.c:495-513) plus
//! `restore_user_context()`:
//!
//! 1. **Divergence**: the call never returns to the caller — expressed in
//!    the type system as `-> !` (C: `NOT_REACHABLE`, proc.c:473).
//! 2. **Interrupts enabled in user mode**: the restored context runs with
//!    interrupts unmasked (C: `p->p_reg.psw |= IF_MASK` —
//!    arch_system.c:512). Without this guarantee a first dispatch would
//!    enter user mode with IF=0 and the machine would silently go deaf.
//! 3. **Kernel stack abandoned**: the impl may push its mode-switch payload
//!    on the current kernel stack and never pop it. This is safe because a
//!    privilege-level switch reloads the kernel stack pointer from the
//!    per-CPU TSS/entry state, so the next kernel entry starts from a clean
//!    stack top — the same invariant C's `restore_user_context` relies on.
//!
//! # Relationship to the design doc
//!
//! 10-switch-to-user.md §3 proposed `trait TrapReturnArch: ExceptionArch`
//! borrowing `Frame` from the exception layer. The landed trait keeps its
//! own `Frame` associated type instead: borrowing would force a mock
//! implementation to re-implement (or forward) all eight `ExceptionArch`
//! methods it does not care about, coupling two orthogonal concerns
//! (fault parsing vs. mode switch). The x86-64 impl simply reuses
//! `X86_64ExceptionFrame` as its `Frame`, so nothing is duplicated in
//! practice.

/// Architecture abstraction for returning from the kernel to user mode.
///
/// See the [module documentation](self) for the design rationale and the
/// caller-visible contract.
pub trait TrapReturnArch {
    /// The arch-assembled mode-switch frame (the `iretq`/`eret`/`sret`
    /// payload).
    ///
    /// Same `Copy + Debug + Default` bounds as `CpuContextArch::TrapFrame`:
    /// the scheduler zero-builds it and lets `apply_to_trap_frame` fill the
    /// user-visible fields.
    type Frame: Copy + core::fmt::Debug + Default;

    /// Saved register state beyond the frame.
    ///
    /// Always the process's `CpuContext` value (the kernel stores it in
    /// `KProcess::cpu_context` and never inspects its fields). Architectures
    /// whose frame is self-contained (aarch64, riscv64) still receive it so
    /// OS call sites stay architecture-uniform.
    type RegisterFile: Copy + core::fmt::Debug + Default;

    /// Transfer control to user mode with the given saved state. Never
    /// returns.
    ///
    /// # Contract (enforced by every impl)
    ///
    /// - Loads the mode-switch state from `frame` and the remaining register
    ///   state from `regs`, then executes the privilege-level switch.
    /// - The restored user context has interrupts enabled (C:
    ///   `arch_finish_switch_to_user` — arch_system.c:512).
    ///
    /// # Safety
    ///
    /// - `frame`/`regs` must describe a valid user-mode context for the
    ///   process that owns the currently installed address space; executing
    ///   arbitrary register state is inherently privileged.
    /// - Must run on the current CPU's kernel stack with paging enabled and
    ///   the user address space already active (the scheduler switches the
    ///   root via `TlbArch::set_active_root` before dispatching).
    /// - The caller must have released the BKL — this is the last kernel
    ///   statement before user mode (C: BKL unlock inside
    ///   `context_stop(KERNEL)`, arch_clock.c:226-233).
    unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> !;
}

// ── Mock implementation for host tests ──────────────────────────────────────

/// Mock trap return — records that a restore was attempted, then panics.
///
/// `restore_to_user` diverges (`-> !`), so a mock cannot "return" a result.
/// The panic message carries the frame, which makes an accidental dispatch
/// on the host loud and diagnosable instead of silently corrupting the test
/// process. Host tests never trigger it: they exercise the scheduling
/// *stages* (pick, requeue, address-space switch, finish) individually.
#[cfg(feature = "mock")]
pub struct MockTrapReturn;

#[cfg(feature = "mock")]
impl TrapReturnArch for MockTrapReturn {
    // The mock build has no per-arch frame of its own; reuse the x86-64
    // frame/context types, which are plain data and compile on any host
    // (the same choice `CurrentCpuContext` makes — there is no mock
    // variant of the CPU context).
    type Frame = crate::x86_64::exception::X86_64ExceptionFrame;
    type RegisterFile = crate::x86_64::boot::X86_64CpuContext;

    unsafe fn restore_to_user(frame: &Self::Frame, _regs: &Self::RegisterFile) -> ! {
        panic!(
            "MockTrapReturn::restore_to_user: real dispatch attempted on host \
             (frame = {frame:?})"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The trait must be usable as a generic bound (contract-test pattern
    /// from tlb_arch.rs — keeps the trait object-safe-free but bound-able).
    #[test]
    fn test_trap_return_arch_as_generic_constraint() {
        fn assert_restore<D: TrapReturnArch>(_frame: &D::Frame, _regs: &D::RegisterFile) {}
        // Only a compile-time check: calling it would diverge.
        fn _typecheck() {
            fn probe<D: TrapReturnArch>() {
                let _ = core::any::type_name::<D::Frame>();
                let _ = core::any::type_name::<D::RegisterFile>();
            }
            let _ = assert_restore::<MockTrapReturn> as fn(
                &<MockTrapReturn as TrapReturnArch>::Frame,
                &<MockTrapReturn as TrapReturnArch>::RegisterFile,
            );
            probe::<MockTrapReturn>();
        }
        _typecheck();
    }

    #[test]
    #[should_panic(expected = "MockTrapReturn::restore_to_user")]
    fn test_mock_restore_panics_loudly() {
        let frame = <MockTrapReturn as TrapReturnArch>::Frame::default();
        let regs = <MockTrapReturn as TrapReturnArch>::RegisterFile::default();
        // SAFETY: mock — panics instead of touching hardware.
        unsafe { MockTrapReturn::restore_to_user(&frame, &regs) };
    }
}
