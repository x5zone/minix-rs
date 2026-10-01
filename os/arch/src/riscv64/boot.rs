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
    CpuContextArch, EntrySpec, ProcKind, ProcNr, WriteUserRegError,
};
use crate::arch::stacktrace::StacktraceArch;
use super::exception::Riscv64ExceptionFrame;

/// Initial sstatus for kernel tasks: SPP=1 (return to S-mode on sret).
const INIT_TASK_SSTATUS: u64 = 0x0000_0100;

/// Initial sstatus for user processes: SPP=0, SPIE=1 (interrupts on).
///
/// Named `INIT_USER_SSTATUS` per 06-proc-init-boot-proc.md §3.4 to distinguish
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
    /// GP register save area for signal handling (X1, X3-X9, X11-X31).
    ///
    /// Indexed by `Riscv64GpReg` constants. X0 is hardwired zero,
    /// X2 (sp) and X10 (a0) are named fields.
    /// Updated by trap entry path (future) and read/written by `SignalContext`.
    pub(super) gp_regs: [u64; Riscv64CpuContext::GP_REGS_LEN],
}

impl Riscv64CpuContext {
    /// NK4-C 续-133：patch sstatus.FS 字段（懒 FPU 归属策略的 riscv 载
    /// 体——FS 在 sstatus/帧内，kernel finish_and_restore 按归属写
    /// Dirty(owner，不陷阱活寄存器延续)/Initial(非 owner，首条 FP 陷
    /// 阱轮转)；`fs` 传已移位值（0b01/0b10 << 13）。
    pub fn set_fs_field(&mut self, fs: u64) {
        self.sstatus = (self.sstatus & !(0b11 << 13)) | fs;
    }
}

impl Riscv64CpuContext {
    /// Number of GP registers in `gp_regs` (30: X1, X3-X9, X11-X31).
    pub const GP_REGS_LEN: usize = 30;

    /// Const-constructible zeroed context (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            sstatus: 0, sepc: 0, sp: 0, a0: 0,
            gp_regs: [0; Self::GP_REGS_LEN],
        }
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
            gp_regs: [0; Riscv64CpuContext::GP_REGS_LEN],
        }
    }

    fn apply_to_trap_frame(ctx: &Self::CpuContext, frame: &mut Self::TrapFrame) {
        frame.sstatus = ctx.sstatus;
        frame.sepc = ctx.sepc;
        // RISC-V ABI register indices: x2 = sp, x10 = a0 (see
        // Riscv64ExceptionFrame::set_return_value, exception.rs).
        frame.regs[2] = ctx.sp;
        frame.regs[10] = ctx.a0;

        // NK4-C 续-133（CodeReview B-1 同源裁决）：本函数的无条件
        // FS=Initial 写已移除——riscv 的 FS 位住在 sstatus/帧内（ctx 是
        // 唯一载体），per-dispatch 覆写会摧毁 finish_and_restore 的归属
        // 策略（owner==picked → ctx FS=Dirty 不陷阱、活寄存器延续；
        // 非 owner → FS=Initial 首条 FP 陷阱轮转）。策略唯一所有者＝
        // kernel finish_and_restore 的 ctx-patch riscv 臂 + scause=2
        // 懒陷阱腿。首次 dispatch 的 ctx 由 build_cpu_context 构造（初
        // 始 FS=Off，首条 FP 同样陷阱取得归属）。
    }

    // enable_user_io: default no-op (riscv64 has no IOPL concept).
    // riscv64 equivalent is sstatus.SUM, set per-process via the
    // sstatus field rather than via a global flag.

    /// Inherit FPU state field from parent on fork (06-proc-init-boot-proc.md §3.14).
    ///
    /// riscv64: the child inherits the parent's `sstatus` value so that
    /// the FS field (Off/Initial/Clean/Dirty) is preserved — a forked
    /// user process keeps `FS=Initial` (lazy trap on first FP insn)
    /// rather than resetting to `FS=Off`.
    fn inherit_fpu_state(child: &mut Self::CpuContext, parent: &Self::CpuContext) {
        child.sstatus = parent.sstatus;
    }

    /// T_SETUSER register write.
    ///
    /// Offset convention (8-byte aligned, matching `Riscv64CpuContext`
    /// field order):
    /// ```text
    /// 0: sstatus
    /// 8: sepc   (PC)
    /// 16: sp     (x2)
    /// 24: a0     (x10, carries ps_strings)
    /// 32..272: gp_regs[0..30]  (x1, x3-x9, x11-x31)
    /// ```
    fn write_user_register(
        ctx: &mut Self::CpuContext,
        offset: usize,
        value: u64,
    ) -> Result<(), WriteUserRegError> {
        if offset % 8 != 0 {
            return Err(WriteUserRegError::BadAddress);
        }
        match offset {
            0 => { ctx.sstatus = value; Ok(()) }
            8 => { ctx.sepc = value; Ok(()) }
            16 => { ctx.sp = value; Ok(()) }
            24 => { ctx.a0 = value; Ok(()) }
            32..=271 => {
                let idx = (offset - 32) / 8;
                if idx < Riscv64CpuContext::GP_REGS_LEN {
                    ctx.gp_regs[idx] = value;
                    Ok(())
                } else {
                    Err(WriteUserRegError::BadAddress)
                }
            }
            _ => Err(WriteUserRegError::BadAddress),
        }
    }

    fn or_ipc_status_reg(ctx: &mut Self::CpuContext, value: u64) {
        // No C original for riscv64. By analogy to ARM's R1 (separate from
        // R0/retreg), we use A1/X11 — separate from A0 (retreg / ps_strings).
        // Accessed via gp_regs[GP_A1] where GP_A1 = 8.
        ctx.gp_regs[crate::riscv64::signal::GP_A1] |= value;
    }

    fn clear_ipc_status_reg(ctx: &mut Self::CpuContext) {
        // C: `IPC_STATUS_CLEAR(p)` — `p_reg.A1 = 0` (ipc.h:45; plain
        // RECEIVE prologue, proc.c:581).
        ctx.gp_regs[crate::riscv64::signal::GP_A1] = 0;
    }

    fn set_secondary_ipc_return(ctx: &mut Self::CpuContext, value: u64) {
        // C: `arch_set_secondary_ipc_return` (arch_system.c:184-186, i386
        // body `p->p_reg.bx = val`) — the MINIX_KERNINFO arm hands the
        // caller the user-mapped info-page address through the secondary
        // lane (whole-value assignment, not an OR-merge). riscv's
        // secondary lane = A1 (`gp_regs[GP_A1]`), the same register
        // `ipc_trap` reads back as its second return value and
        // `sync_status_register_to_frame` pulls into the outgoing frame
        // (续-75, mirroring the aarch64 §1.112 CodeReview W2 fix: the
        // trait default no-op would drop the page address and make
        // `query_kerninfo_page` falsely return Ok(0)).
        ctx.gp_regs[crate::riscv64::signal::GP_A1] = value;
    }

    fn set_ipc_return_reg(ctx: &mut Self::CpuContext, value: u64) {
        // C: `p->p_reg.retreg = val` (proc.c:969/:1097 wake sites). On
        // riscv64 the return lane is A0, stored separately as `ctx.a0`
        // (write_user_register offset 24) — NOT in `gp_regs`.
        ctx.a0 = value;
    }

    fn ipc_return_reg(ctx: &Self::CpuContext) -> u64 {
        ctx.a0
    }
}

/// Frame-pointer chain walk for riscv64 (s0/fp-linked frames).
///
/// RISC-V ABI frame layout (same 8-byte slot convention as x86-64/aarch64):
/// ```text
/// [s0+0]  saved_fp  (caller's s0/fp)
/// [s0+8]  return_addr (saved ra)
/// ```
///
/// Frame pointers are optional in the RISC-V ABI (like x86-64, unlike
/// aarch64's mandatory FP), so the walk is best-effort for code compiled
/// without frame-pointer omission.
impl StacktraceArch for Riscv64CpuContextArch {
    fn frame_pointer(cpu_context: &Riscv64CpuContext) -> u64 {
        // C: whichproc->p_reg.fp — riscv64 stores s0/fp (X8) in gp_regs.
        // gp_regs layout: [0]=X1, [1]=X3, ..., [6]=X8 (s0/fp), ...
        // GP_S0 (X8 = s0/fp) → index 6 (signal.rs:80).
        cpu_context
            .gp_regs
            .get(crate::riscv64::signal::GP_S0)
            .copied()
            .unwrap_or(0)
    }

    fn program_counter(cpu_context: &Riscv64CpuContext) -> u64 {
        // C: whichproc->p_reg.pc — riscv64 stores PC as sepc (named field).
        cpu_context.sepc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::VirBytes;

    #[test]
    fn kernel_task_uses_init_task_sstatus() {
        let ctx = Riscv64CpuContextArch::build_cpu_context(
            ProcKind::KernelTask,
            ProcNr(-1),
            EntrySpec::KERNEL_TASK,
        );
        assert_eq!(ctx.sstatus, INIT_TASK_SSTATUS);
        assert_eq!(ctx.sstatus & 0x100, 0x100, "SPP must be 1 for kernel task");
    }

    #[test]
    fn user_process_uses_init_sstatus() {
        let ctx = Riscv64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            ProcNr(8),
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
            ProcKind::Vm, ProcNr(8), EntrySpec::DEFERRED,
        );
        let mut child = Riscv64CpuContextArch::build_cpu_context(
            ProcKind::KernelTask, ProcNr(-1), EntrySpec::KERNEL_TASK,
        );
        assert_ne!(child.sstatus, parent.sstatus, "precondition: differ");
        Riscv64CpuContextArch::inherit_fpu_state(&mut child, &parent);
        assert_eq!(child.sstatus, parent.sstatus, "child must inherit parent sstatus");
    }
}