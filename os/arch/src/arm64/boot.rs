//! aarch64 `CpuContextArch` implementation.
//!
//! # FPU
//!
//! aarch64 has no per-process FPU save area — FPCR / FPSR are saved
//! and restored on context switch. The previous design's "no FPU field
//! at all" was correct about the data layout but **wrong** about the
//! per-process distinction: a kernel task runs at EL1 with FP
//! unconditionally trapped, while a user process runs at EL0 with FP
//! enabled. Per the §12.1 review, `fpu_enable_el0` must be a per-process
//! field that the trait's `apply_to_trap_frame` consults.

use crate::arch::boot::{
    CpuContextArch, EntrySpec, ProcKind, ProcNr,
};
use super::exception::AArch64ExceptionFrame;

/// aarch64 initial PSR (SPSR_EL1) for kernel tasks: EL1h, all
/// exceptions masked (F/I/A/D).
///
/// C: INIT_TASK_PSR — earm/include/archconst.h:12
const INIT_TASK_PSR: u64 = 0x0000_03C5;

/// aarch64 initial PSR (SPSR_EL1) for user processes: EL0t, no masking.
///
/// C: INIT_PSR — earm/include/archconst.h:11
const INIT_PSR: u64 = 0x0000_0000;

#[derive(Debug, Clone, Copy, Default)]
pub struct AArch64CpuContext {
    /// SPSR_EL1 initial value.
    pub(super) psr: u64,
    /// ELR_EL1 (entry PC).
    pub(super) pc: u64,
    /// SP_EL0 (initial user SP).
    pub(super) sp: u64,
    /// X0 (carries ps_strings address).
    pub(super) r0: u64,
    /// `true` ⇔ CPACR_EL1.FPEN = 0b01 (EL0 FP enable, EL1 trap).
    ///
    /// Kernel tasks have this `false`; user processes have this
    /// `true`. Arch-internal: the kernel layer never reads this
    /// (see `06-design-final.md` §12.1).
    fpu_enable_el0: bool,
}

impl AArch64CpuContext {
    /// Const-constructible zeroed context (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            psr: 0, pc: 0, sp: 0, r0: 0, fpu_enable_el0: false,
        }
    }
}

pub struct AArch64CpuContextArch;

impl CpuContextArch for AArch64CpuContextArch {
    type CpuContext = AArch64CpuContext;
    type TrapFrame = AArch64ExceptionFrame;

    fn build_cpu_context(kind: ProcKind, _proc_nr: ProcNr, entry: EntrySpec) -> Self::CpuContext {
        let (psr, fpu_enable_el0) = match kind {
            ProcKind::KernelTask => (INIT_TASK_PSR, false),
            _ => (INIT_PSR, true),
        };
        AArch64CpuContext {
            psr,
            pc: entry.pc.map(|v| v.0).unwrap_or(0),
            sp: entry.sp.map(|v| v.0).unwrap_or(0),
            r0: entry.ps_strings.map(|v| v.0).unwrap_or(0),
            fpu_enable_el0,
        }
    }

    fn apply_to_trap_frame(ctx: &Self::CpuContext, frame: &mut Self::TrapFrame) {
        frame.spsr_el1 = ctx.psr;
        frame.elr_el1 = ctx.pc;
        frame.sp_el0 = ctx.sp;
        frame.regs[0] = ctx.r0;

        // CPACR_EL1.FPEN: arch-internal write of the per-process FP
        // policy. Default impl in `boot.rs` is a no-op; aarch64
        // actually needs this to flip between kernel-task (EL1 trap)
        // and user-process (EL0 enable) modes.
        // SAFETY: writing CPACR_EL1 is a per-CPU system register.
        // The caller (`apply_to_trap_frame`) is invoked under BKL
        // during first dispatch, so no race on the system register.
        if ctx.fpu_enable_el0 {
            unsafe { enable_cpacr_el1_fpen_user() };
        } else {
            unsafe { enable_cpacr_el1_fpen_kernel() };
        }
    }

    // enable_user_io: default no-op (aarch64 has no IOPL concept).
    // aarch64 equivalent is PSTATE.PAN, set per-process via SPSR
    // rather than via a global flag.

    /// Inherit FPU enable policy from parent on fork (06-design-final.md §15.5).
    ///
    /// aarch64: the child inherits `fpu_enable_el0` so that a forked user
    /// process keeps EL0 FP access (CPACR_EL1.FPEN=0b01) rather than
    /// silently dropping to the kernel-task trap mode.
    fn inherit_fpu_state(child: &mut Self::CpuContext, parent: &Self::CpuContext) {
        child.fpu_enable_el0 = parent.fpu_enable_el0;
    }
}

#[cfg(target_arch = "aarch64")]
mod cpacr {
    /// Enable FP at EL0, trap at EL1.
    pub(super) unsafe fn enable_cpacr_el1_fpen_user() {
        core::arch::asm!(
            "mrs {tmp}, CPACR_EL1",
            "orr {tmp}, {tmp}, #(0x3 << 20)", // FPEN = 0b01
            "msr CPACR_EL1, {tmp}",
            "isb",
            tmp = out(reg) _,
        );
    }
    /// Disable FP at EL0 (kernel-task mode).
    pub(super) unsafe fn enable_cpacr_el1_fpen_kernel() {
        core::arch::asm!(
            "mrs {tmp}, CPACR_EL1",
            "bic {tmp}, {tmp}, #(0x3 << 20)", // FPEN = 0b00
            "msr CPACR_EL1, {tmp}",
            "isb",
            tmp = out(reg) _,
        );
    }
}

#[cfg(not(target_arch = "aarch64"))]
mod cpacr {
    /// Stub for non-aarch64 builds (tests, mock).
    pub(super) unsafe fn enable_cpacr_el1_fpen_user() {}
    pub(super) unsafe fn enable_cpacr_el1_fpen_kernel() {}
}

use cpacr::*;

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::VirBytes;

    #[test]
    fn kernel_task_uses_init_task_psr() {
        let ctx = AArch64CpuContextArch::build_cpu_context(
            ProcKind::KernelTask,
            -1,
            EntrySpec::KERNEL_TASK,
        );
        assert_eq!(ctx.psr, INIT_TASK_PSR);
        assert!(!ctx.fpu_enable_el0, "kernel task must keep FPEN=0");
    }

    #[test]
    fn user_process_uses_init_psr_and_fpen_user() {
        let ctx = AArch64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::loaded(
                VirBytes(0x1000),
                VirBytes(0x4000_0000),
                VirBytes(0x3fff_ffe0),
            ),
        );
        assert_eq!(ctx.psr, INIT_PSR);
        assert_eq!(ctx.pc, 0x1000);
        assert_eq!(ctx.sp, 0x4000_0000);
        assert_eq!(ctx.r0, 0x3fff_ffe0);
        assert!(ctx.fpu_enable_el0, "user process must have FPEN=0b01");
    }

    #[test]
    fn all_user_kinds_get_fpen_user() {
        // Vm, RootService, UserService, UserProcess all map to user mode.
        for kind in [ProcKind::Vm, ProcKind::RootService, ProcKind::UserService, ProcKind::UserProcess] {
            let ctx = AArch64CpuContextArch::build_cpu_context(kind, 0, EntrySpec::DEFERRED);
            assert!(ctx.fpu_enable_el0, "{:?} must enable FP at EL0", kind);
        }
    }

    #[test]
    fn default_ctx_is_zeroed() {
        let ctx = AArch64CpuContext::default();
        assert_eq!(ctx.psr, 0);
        assert!(!ctx.fpu_enable_el0);
    }

    #[test]
    fn inherit_fpu_state_propagates_fpu_enable_el0() {
        let parent = AArch64CpuContextArch::build_cpu_context(
            ProcKind::Vm, 8, EntrySpec::DEFERRED,
        );
        let mut child = AArch64CpuContextArch::build_cpu_context(
            ProcKind::KernelTask, -1, EntrySpec::KERNEL_TASK,
        );
        assert!(parent.fpu_enable_el0, "precondition: parent has FP enabled");
        assert!(!child.fpu_enable_el0, "precondition: child (kernel task) has FP disabled");
        AArch64CpuContextArch::inherit_fpu_state(&mut child, &parent);
        assert!(child.fpu_enable_el0, "child must inherit parent FP enable");
    }
}