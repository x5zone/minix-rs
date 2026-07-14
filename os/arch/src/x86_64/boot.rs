//! x86-64 `CpuContextArch` implementation.
//!
//! `X86_64CpuContext` is the arch-private "process's initial CPU state"
//! — kernel-visible only as `Copy + Debug + Default` opaque value. It
//! contains the PSW (RFLAGS), all six segment selectors, the entry
//! RIP/RSP/RBX, plus an arch-internal `X86FpuInitPolicy` flag that the
//! kernel layer never reads.
//!
//! # FPU
//!
//! Modern x86-64 uses XSAVE (lazy) — there is no `fnsave`/`fxrstor`
//! memcpy of a 576-byte area. The previous `fpu_needs_zero: bool` field
//! that "leaked" through `InitialRegState` is gone; FPU init policy is
//! a `CpuContext` field that lives entirely inside the arch layer.

use minix_types::VirBytes;

use crate::arch::boot::{
    CpuContextArch, EntrySpec, ProcKind, ProcNr,
};
use super::exception::X86_64ExceptionFrame;

/// x86-64 initial PSW (RFLAGS) for kernel tasks.
///
/// In Minix3 32-bit this was `0x1200`; in 64-bit the reserved bit 1
/// must also be set, so we use `0x1202` (IOPL=1, IF=1, bit1=1).
const INIT_TASK_PSW: u64 = 0x1202;

/// x86-64 initial PSW (RFLAGS) for user processes.
///
/// Minix3 32-bit `0x0200`; 64-bit `0x0202` (IOPL=0, IF=1, bit1=1).
const INIT_PSW: u64 = 0x0202;

/// User CS (Ring 3). GDT index 3, RPL=3.
const USER_CS_SELECTOR: u64 = 0x1B;

/// User DS (Ring 3). GDT index 4, RPL=3.
const USER_DS_SELECTOR: u64 = 0x23;

/// x86-64 FPU init policy (arch-internal; kernel layer never reads).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum X86FpuInitPolicy {
    /// Kernel task: shares the kernel FPU context, no per-process init.
    KernelTask,
    /// User process: XSAVE area is allocated lazily on first FP
    /// instruction (the modern XSAVE model — NOT Minix3's
    /// `memset 0` fnsave model).
    LazyUserInit,
}

impl Default for X86FpuInitPolicy {
    fn default() -> Self {
        Self::KernelTask
    }
}

impl X86FpuInitPolicy {
    /// Const-constructible default (for `const fn` CpuContext init).
    pub const fn default_const() -> Self {
        Self::KernelTask
    }
}

/// x86-64 `CpuContext`.
///
/// All fields are arch-internal. The kernel stores this by value in
/// `KProcess` and never reads it.
#[derive(Debug, Clone, Copy, Default)]
pub struct X86_64CpuContext {
    /// RFLAGS initial value.
    pub(super) psw: u64,
    pub(super) cs: u64,
    pub(super) ds: u64,
    pub(super) ss: u64,
    pub(super) es: u64,
    pub(super) fs: u64,
    pub(super) gs: u64,
    /// Initial RIP.
    pub(super) rip: u64,
    /// Initial RSP.
    pub(super) rsp: u64,
    /// Initial RBX (carries ps_strings address).
    pub(super) rbx: u64,
    /// FPU init strategy (kernel layer never reads this).
    fpu_policy: X86FpuInitPolicy,
}

impl X86_64CpuContext {
    /// Const-constructible zeroed context (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            psw: 0, cs: 0, ds: 0, ss: 0, es: 0, fs: 0, gs: 0,
            rip: 0, rsp: 0, rbx: 0,
            fpu_policy: X86FpuInitPolicy::default_const(),
        }
    }
}

/// x86-64 implementation marker (no state; trait is stateless).
pub struct X86_64CpuContextArch;

impl CpuContextArch for X86_64CpuContextArch {
    type CpuContext = X86_64CpuContext;
    type TrapFrame = X86_64ExceptionFrame;

    fn build_cpu_context(kind: ProcKind, _proc_nr: ProcNr, entry: EntrySpec) -> Self::CpuContext {
        let (psw, fpu_policy) = match kind {
            ProcKind::KernelTask => (INIT_TASK_PSW, X86FpuInitPolicy::KernelTask),
            _ => (INIT_PSW, X86FpuInitPolicy::LazyUserInit),
        };
        X86_64CpuContext {
            psw,
            cs: USER_CS_SELECTOR,
            ds: USER_DS_SELECTOR,
            ss: USER_DS_SELECTOR,
            es: USER_DS_SELECTOR,
            fs: USER_DS_SELECTOR,
            gs: USER_DS_SELECTOR,
            rip: entry.pc.map(|v| v.0).unwrap_or(0),
            rsp: entry.sp.map(|v| v.0).unwrap_or(0),
            rbx: entry.ps_strings.map(|v| v.0).unwrap_or(0),
            fpu_policy,
        }
    }

    fn apply_to_trap_frame(ctx: &Self::CpuContext, frame: &mut Self::TrapFrame) {
        // The CPU-pushed exception frame carries CS, SS, RFLAGS, RIP,
        // RSP. ES/FS/GS/RBX are not part of the CPU-pushed frame —
        // they live in the broader register save area managed by the
        // trap entry path. The scheduler restores them on first
        // dispatch using values stored alongside `CpuContext`.
        frame.rflags = ctx.psw;
        frame.cs = ctx.cs;
        frame.ss = ctx.ss;
        frame.rip = ctx.rip;
        frame.rsp = ctx.rsp;

        // FPU init policy: see the doc comment on `X86FpuInitPolicy`.
        // For `KernelTask` we do nothing; for `LazyUserInit` the
        // XSAVE area is allocated by the first FP-instruction trap
        // (#NM) inside the kernel — no work to do here.
        let _ = ctx.fpu_policy;
    }

    /// Enable user I/O: set RFLAGS.IOPL = 3 (bits 12-13).
    fn enable_user_io(ctx: &mut Self::CpuContext) {
        ctx.psw |= 0x3000;
    }

    /// Inherit FPU init policy from parent on fork (06-design-final.md §15.5).
    ///
    /// x86-64: the child inherits the parent's `X86FpuInitPolicy` so that
    /// a forked user process keeps `LazyUserInit` (rather than silently
    /// dropping back to `KernelTask`). C: `memcpy(rpc->p_seg.fpu_state,
    /// rpp->p_seg.fpu_state, FPU_XFP_SIZE)` under `proc_used_fpu(rpp)`.
    fn inherit_fpu_state(child: &mut Self::CpuContext, parent: &Self::CpuContext) {
        child.fpu_policy = parent.fpu_policy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_task_uses_init_task_psw() {
        let ctx = X86_64CpuContextArch::build_cpu_context(
            ProcKind::KernelTask,
            -1,
            EntrySpec::KERNEL_TASK,
        );
        assert_eq!(ctx.psw, INIT_TASK_PSW);
        assert_eq!(ctx.cs, USER_CS_SELECTOR);
        assert_eq!(ctx.ds, USER_DS_SELECTOR);
        // Kernel task has no entry.
        assert_eq!(ctx.rip, 0);
        assert_eq!(ctx.rsp, 0);
    }

    #[test]
    fn user_process_uses_init_psw() {
        let ctx = X86_64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::loaded(
                VirBytes(0x1000),
                VirBytes(0x7fff_0000),
                VirBytes(0x7ffe_ffe0),
            ),
        );
        assert_eq!(ctx.psw, INIT_PSW);
        assert_eq!(ctx.rip, 0x1000);
        assert_eq!(ctx.rsp, 0x7fff_0000);
        assert_eq!(ctx.rbx, 0x7ffe_ffe0);
    }

    #[test]
    fn enable_user_io_sets_iopl() {
        let mut ctx = X86_64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::KERNEL_TASK,
        );
        assert_eq!(ctx.psw & 0x3000, 0, "IOPL starts at 0");
        X86_64CpuContextArch::enable_user_io(&mut ctx);
        assert_eq!(ctx.psw & 0x3000, 0x3000, "IOPL set to 3");
    }

    #[test]
    fn default_ctx_is_kernel_task_shape() {
        let ctx = X86_64CpuContext::default();
        // All-zero default; meaningful values come from build_cpu_context.
        assert_eq!(ctx.psw, 0);
        assert_eq!(ctx.rip, 0);
    }

    #[test]
    fn apply_to_trap_frame_copies_registers() {
        let ctx = X86_64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::loaded(
                VirBytes(0xdead_beef),
                VirBytes(0xcafe_f00d),
                VirBytes(0x1234_5678),
            ),
        );
        let mut frame = X86_64ExceptionFrame::default();
        X86_64CpuContextArch::apply_to_trap_frame(&ctx, &mut frame);
        assert_eq!(frame.rflags, INIT_PSW);
        assert_eq!(frame.cs, USER_CS_SELECTOR);
        assert_eq!(frame.rip, 0xdead_beef);
        assert_eq!(frame.rsp, 0xcafe_f00d);
    }

    #[test]
    fn inherit_fpu_state_propagates_lazy_user_policy() {
        let parent = X86_64CpuContextArch::build_cpu_context(
            ProcKind::Vm, 8, EntrySpec::DEFERRED,
        );
        let mut child = X86_64CpuContextArch::build_cpu_context(
            ProcKind::KernelTask, -1, EntrySpec::KERNEL_TASK,
        );
        // Precondition: child starts as KernelTask policy, parent is LazyUserInit.
        assert_ne!(child.fpu_policy, parent.fpu_policy, "precondition: differ");
        X86_64CpuContextArch::inherit_fpu_state(&mut child, &parent);
        assert_eq!(child.fpu_policy, parent.fpu_policy,
            "child must inherit parent FPU policy");
        assert_eq!(child.fpu_policy, X86FpuInitPolicy::LazyUserInit);
    }
}