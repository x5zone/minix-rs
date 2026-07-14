//! riscv64 `CpuContextArch` implementation.
//!
//! # FPU
//!
//! riscv64 has no per-process FPU save area either; the `sstatus.FS`
//! field is a state machine (Off/Initial/Clean/Dirty). Setting it to
//! `Initial` causes the first FP instruction to trap, at which point
//! the kernel can allocate the FPU context lazily. The kernel layer
//! never reads `sstatus` — it lives in the `CpuContext` only.

use crate::arch::boot::{
    CpuContextArch, EntrySpec, ProcKind, ProcNr,
};
use super::exception::Riscv64ExceptionFrame;

/// Initial sstatus for kernel tasks: SPP=1 (return to S-mode on sret).
const INIT_TASK_SSTATUS: u64 = 0x0000_0100;

/// Initial sstatus for user processes: SPP=0, SPIE=1 (interrupts on).
///
/// Named `INIT_USER_SSTATUS` per 06-design-final.md §3.3 to distinguish
/// the user-process variant from the kernel-task variant.
const INIT_USER_SSTATUS: u64 = 0x0000_0020;

#[derive(Debug, Clone, Copy, Default)]
pub struct Riscv64CpuContext {
    /// Initial sstatus.
    pub(super) sstatus: u64,
    /// Initial sepc (PC).
    pub(super) sepc: u64,
    /// Initial sp (x2).
    pub(super) sp: u64,
    /// Initial a0 (x10, carries ps_strings address).
    pub(super) a0: u64,
}

impl Riscv64CpuContext {
    /// Const-constructible zeroed context (for `const fn` table init).
    pub const fn new() -> Self {
        Self { sstatus: 0, sepc: 0, sp: 0, a0: 0 }
    }
}

pub struct Riscv64CpuContextArch;

impl CpuContextArch for Riscv64CpuContextArch {
    type CpuContext = Riscv64CpuContext;
    type TrapFrame = Riscv64ExceptionFrame;

    fn build_cpu_context(kind: ProcKind, _proc_nr: ProcNr, entry: EntrySpec) -> Self::CpuContext {
        let sstatus = match kind {
            ProcKind::KernelTask => INIT_TASK_SSTATUS,
            _ => INIT_USER_SSTATUS,
        };
        Riscv64CpuContext {
            sstatus,
            sepc: entry.pc.map(|v| v.0).unwrap_or(0),
            sp: entry.sp.map(|v| v.0).unwrap_or(0),
            a0: entry.ps_strings.map(|v| v.0).unwrap_or(0),
        }
    }

    fn apply_to_trap_frame(ctx: &Self::CpuContext, frame: &mut Self::TrapFrame) {
        frame.sstatus = ctx.sstatus;
        frame.sepc = ctx.sepc;
        frame.sp = ctx.sp;
        frame.a0 = ctx.a0;

        // sstatus.FS = Initial so the first FP instruction traps and
        // the kernel can do lazy FPU allocation. This is the
        // modern RISC-V equivalent of Minix3's memset-FPU-state, but
        // it does not require a per-process FPU save area in
        // `KProcess`.
        // SAFETY: writing sstatus is a per-CPU system register; the
        // caller holds BKL during first dispatch.
        unsafe { set_sstatus_fs_initial() };
    }

    // enable_user_io: default no-op (riscv64 has no IOPL concept).
    // riscv64 equivalent is sstatus.SUM, set per-process via the
    // sstatus field rather than via a global flag.

    /// Inherit FPU state field from parent on fork (06-design-final.md §15.5).
    ///
    /// riscv64: the child inherits the parent's `sstatus` value so that
    /// the FS field (Off/Initial/Clean/Dirty) is preserved — a forked
    /// user process keeps `FS=Initial` (lazy trap on first FP insn)
    /// rather than resetting to `FS=Off`.
    fn inherit_fpu_state(child: &mut Self::CpuContext, parent: &Self::CpuContext) {
        child.sstatus = parent.sstatus;
    }
}

#[cfg(target_arch = "riscv64")]
mod sstatus {
    /// Set sstatus.FS = Initial (0b01 << 13).
    pub(super) unsafe fn set_sstatus_fs_initial() {
        core::arch::asm!(
            "li {tmp}, 0x4000", // FS=Initial = 0b01 << 13
            "csrs sstatus, {tmp}",
            tmp = out(reg) _,
        );
    }
}

#[cfg(not(target_arch = "riscv64"))]
mod sstatus {
    pub(super) unsafe fn set_sstatus_fs_initial() {}
}

use sstatus::*;

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::VirBytes;

    #[test]
    fn kernel_task_uses_init_task_sstatus() {
        let ctx = Riscv64CpuContextArch::build_cpu_context(
            ProcKind::KernelTask,
            -1,
            EntrySpec::KERNEL_TASK,
        );
        assert_eq!(ctx.sstatus, INIT_TASK_SSTATUS);
        assert_eq!(ctx.sstatus & 0x100, 0x100, "SPP must be 1 for kernel task");
    }

    #[test]
    fn user_process_uses_init_sstatus() {
        let ctx = Riscv64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::loaded(
                VirBytes(0x1000),
                VirBytes(0x4000_0000),
                VirBytes(0x3fff_ffe0),
            ),
        );
        assert_eq!(ctx.sstatus, INIT_USER_SSTATUS);
        assert_eq!(ctx.sstatus & 0x100, 0, "SPP must be 0 for user process");
        assert_eq!(ctx.sstatus & 0x20, 0x20, "SPIE must be 1 for user process");
        assert_eq!(ctx.sepc, 0x1000);
        assert_eq!(ctx.sp, 0x4000_0000);
        assert_eq!(ctx.a0, 0x3fff_ffe0);
    }

    #[test]
    fn default_ctx_is_zeroed() {
        let ctx = Riscv64CpuContext::default();
        assert_eq!(ctx.sstatus, 0);
    }

    #[test]
    fn inherit_fpu_state_copies_sstatus() {
        let parent = Riscv64CpuContextArch::build_cpu_context(
            ProcKind::Vm, 8, EntrySpec::DEFERRED,
        );
        let mut child = Riscv64CpuContextArch::build_cpu_context(
            ProcKind::KernelTask, -1, EntrySpec::KERNEL_TASK,
        );
        assert_ne!(child.sstatus, parent.sstatus, "precondition: differ");
        Riscv64CpuContextArch::inherit_fpu_state(&mut child, &parent);
        assert_eq!(child.sstatus, parent.sstatus, "child must inherit parent sstatus");
    }
}