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

#[cfg(test)]
use minix_types::VirBytes;

use crate::arch::boot::{
    CpuContextArch, EntrySpec, ProcKind, ProcNr, WriteUserRegError,
};
use crate::arch::stacktrace::StacktraceArch;
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
#[derive(Default)]
enum X86FpuInitPolicy {
    /// Kernel task: shares the kernel FPU context, no per-process init.
    #[default]
    KernelTask,
    /// User process: XSAVE area is allocated lazily on first FP
    /// instruction (the modern XSAVE model — NOT Minix3's
    /// `memset 0` fnsave model).
    LazyUserInit,
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
///
/// # GP register save area
///
/// `gp_regs` holds the 14 general-purpose registers that are NOT
/// already named fields (RAX, RCX, RDX, RSI, RDI, RBP, R8-R15).
/// Named fields (RIP, RSP, RBX) serve dual purpose: initial state
/// at boot and saved state at trap entry. `gp_regs` is updated by
/// the trap entry path (future) and read/written by `SignalContext`.
///
/// Until the trap entry assembly is updated to save GP registers
/// into `gp_regs`, the array defaults to all-zeros. This is safe
/// — signal delivery will save/restore zeros, which is incorrect
/// for production use but structurally sound.
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
    /// GP register save area for signal handling.
    ///
    /// Indexed by `X86_64GpReg` constants. Updated by trap entry
    /// path and read/written by `SignalContext`.
    pub(super) gp_regs: [u64; X86_64CpuContext::GP_REGS_LEN],
}

impl X86_64CpuContext {
    /// Number of GP registers in `gp_regs` (14: RAX, RCX, RDX, RSI,
    /// RDI, RBP, R8-R15).
    pub const GP_REGS_LEN: usize = 14;

    /// Const-constructible zeroed context (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            psw: 0, cs: 0, ds: 0, ss: 0, es: 0, fs: 0, gs: 0,
            rip: 0, rsp: 0, rbx: 0,
            fpu_policy: X86FpuInitPolicy::default_const(),
            gp_regs: [0; Self::GP_REGS_LEN],
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
            gp_regs: [0; X86_64CpuContext::GP_REGS_LEN],
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

    /// Inherit FPU init policy from parent on fork (06-proc-init-boot-proc.md §3.14).
    ///
    /// x86-64: the child inherits the parent's `X86FpuInitPolicy` so that
    /// a forked user process keeps `LazyUserInit` (rather than silently
    /// dropping back to `KernelTask`). C: `memcpy(rpc->p_seg.fpu_state,
    /// rpp->p_seg.fpu_state, FPU_XFP_SIZE)` under `proc_used_fpu(rpp)`.
    fn inherit_fpu_state(child: &mut Self::CpuContext, parent: &Self::CpuContext) {
        child.fpu_policy = parent.fpu_policy;
    }

    /// T_SETUSER register write with segment-register protection.
    ///
    /// Offset convention (8-byte aligned, matching `X86_64CpuContext`
    /// field order):
    /// ```text
    /// 0: psw  (RFLAGS — user bits only, C: SETPSW)
    /// 8: cs   (PROTECTED — writing crashes kernel at context switch)
    /// 16: ds  (PROTECTED)
    /// 24: ss  (PROTECTED)
    /// 32: es  (PROTECTED)
    /// 40: fs  (PROTECTED)
    /// 48: gs  (PROTECTED)
    /// 56: rip
    /// 64: rsp
    /// 72: rbx
    /// 80..192: gp_regs[0..14]
    /// ```
    ///
    /// C: do_trace.c:140-157 — i386 forbids cs/ds/es/fs/gs/ss writes.
    /// x86_64 C source has a gap (no write path compiled); Rust
    /// implements the correct behavior.
    fn write_user_register(
        ctx: &mut Self::CpuContext,
        offset: usize,
        value: u64,
    ) -> Result<(), WriteUserRegError> {
        // Alignment: C checks `tr_addr & (sizeof(reg_t)-1)`.
        // On 64-bit, reg_t = u64, so offset must be 8-byte aligned.
        if !offset.is_multiple_of(8) {
            return Err(WriteUserRegError::BadAddress);
        }
        match offset {
            0 => {
                // PSW (RFLAGS): only user-controllable bits changeable.
                // C: SETPSW(rp, tr_data) — preserves system bits.
                // User bits: CF, PF, AF, ZF, SF, TF, DF, OF, IF (bit 9).
                const PSW_USER_MASK: u64 = 0x0DD5;
                ctx.psw = (ctx.psw & !PSW_USER_MASK) | (value & PSW_USER_MASK);
                Ok(())
            }
            // Segment registers — protected (would crash kernel).
            8 | 16 | 24 | 32 | 40 | 48 => Err(WriteUserRegError::Protected),
            56 => { ctx.rip = value; Ok(()) }
            64 => { ctx.rsp = value; Ok(()) }
            72 => { ctx.rbx = value; Ok(()) }
            80..=191 => {
                let idx = (offset - 80) / 8;
                if idx < X86_64CpuContext::GP_REGS_LEN {
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
        // C: `p->p_reg.IPC_STATUS_REG |= value` where IPC_STATUS_REG = bx
        // (ipcconst.h:10). RBX also carries ps_strings at process startup;
        // after the first IPC delivery, it is repurposed for IPC status.
        ctx.rbx |= value;
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

/// x86-64 `StacktraceArch` implementation.
///
/// C: `proc_stacktrace()` — arch/i386/exception.c:333-373.
///
/// Walks the `rbp`-linked frame chain. Each frame has layout:
/// ```text
/// [rbp+0]  saved_rbp  (caller's frame pointer)
/// [rbp+8]  return_addr
/// ```
///
/// Frame pointer must be enabled at compile time (`-C force-frame-pointers`)
/// for the walk to produce meaningful results. Without frame pointers,
/// `rbp` is a general-purpose register and the walk stops immediately.
impl StacktraceArch for X86_64CpuContextArch {
    fn frame_pointer(cpu_context: &X86_64CpuContext) -> u64 {
        // C: whichproc->p_reg.fp — x86-64 stores RBP in gp_regs[GP_RBP].
        // GP_RBP = 5 (see signal.rs:38).
        cpu_context.gp_regs.get(5).copied().unwrap_or(0)
    }

    fn program_counter(cpu_context: &X86_64CpuContext) -> u64 {
        // C: whichproc->p_reg.pc — x86-64 stores RIP as a named field.
        cpu_context.rip
    }

    fn current_frame_pointer() -> Option<u64> {
        // C: get_bp() (libsys/stacktrace.c) — `mov eax, ebp; ret`.
        // SAFETY: reads the rbp register. rbp is callee-saved, so the
        // compiler preserves it across the asm; the kernel is built with
        // frame pointers (the same assumption the saved-context walker
        // makes). Inside this function's prologue, rbp points at the
        // current frame: [rbp] = saved rbp, [rbp+8] = return address.
        let fp: u64;
        unsafe { core::arch::asm!("mov {}, rbp", out(reg) fp) };
        Some(fp)
    }
}
#[cfg(test)]
mod stacktrace_tests {
    use super::*;
    use crate::arch::stacktrace::{StacktraceArch, MAX_STACK_FRAMES};

    /// Look up a word in a fake stack slice: `(addr, value)` pairs.
    /// Returns `None` for addresses not present (simulates an unmapped
    /// frame — C's PRCOPY failure path, exception.c:297-302).
    fn read_from(stack: &[(u64, u64)]) -> impl Fn(u64) -> Option<u64> + '_ {
        move |addr| stack.iter().find(|(a, _)| *a == addr).map(|(_, v)| *v)
    }

    /// C: proc_stacktrace_execute — chain walk emits pc + each return address.
    /// D-47: current_frame_pointer reads the host (or kernel) rbp via
    /// asm — plausible on any x86_64 with frame pointers: non-zero and
    /// 8-aligned. Safe on hosted Linux (a plain register read).
    #[test]
    fn test_current_frame_pointer_plausible() {
        let fp = X86_64CpuContextArch::current_frame_pointer().expect("x86_64 must support current_frame_pointer");
        assert_ne!(fp, 0, "rbp must be non-zero inside a live frame");
        assert_eq!(fp % 8, 0, "rbp must be 8-aligned");
    }

    /// D-47: the shared `walk_frames_from` loop (used by util_stacktrace
    /// without a saved context) walks the same [saved_fp, ret] layout.
    /// Fake chain hosted on local arrays.
    #[test]
    fn test_walk_frames_from_shared_loop() {
        // Synthetic chain (the walker only reads through `read_word`, so
        // the addresses need not be real pointers):
        //   fp 0x1000 -> [saved 0x2000, ret 0xA]; fp 0x2000 -> [0, 0xB]
        let stack = [
            (0x1000u64, 0x2000u64),
            (0x1008, 0xA),
            (0x2000, 0),
            (0x2008, 0xB),
        ];
        let read_from = |addr: u64| stack.iter().find(|(a, _)| *a == addr).map(|(_, v)| *v);
        let mut emitted: Vec<u64> = Vec::new();
        X86_64CpuContextArch::walk_frames_from(read_from, |pc| emitted.push(pc), 0x1000, 0);
        assert_eq!(emitted, vec![0xA, 0xB]);
        // Non-advancing frame (saved_fp <= fp) terminates the walk —
        // the corruption sentinel, same as the walk_frames cycle test.
        let stack2 = [(0x1000u64, 0x900u64), (0x1008, 0xA)];
        let read_from2 = |addr: u64| stack2.iter().find(|(a, _)| *a == addr).map(|(_, v)| *v);
        let mut emitted2: Vec<u64> = Vec::new();
        X86_64CpuContextArch::walk_frames_from(read_from2, |pc| emitted2.push(pc), 0x1000, 0);
        assert_eq!(emitted2, vec![0xA]);
    }

    #[test]
    fn test_stacktrace_walk_frames_emits_pc_and_chain() {
        let mut ctx = X86_64CpuContext::new();
        ctx.rip = 0x1000;
        ctx.gp_regs[5] = 0x2000; // GP_RBP (signal.rs:38)
        // Fake stack:
        //   [0x2000]=0x4000 saved_fp   [0x2008]=0x5000 return_addr
        //   [0x4000]=0x0    saved_fp   [0x4008]=0x6000 return_addr (stack bottom)
        let stack = [
            (0x2000u64, 0x4000u64), (0x2008u64, 0x5000u64),
            (0x4000u64, 0x0000u64), (0x4008u64, 0x6000u64),
        ];
        let mut emitted = [0u64; 8];
        let mut n = 0usize;
        X86_64CpuContextArch::walk_frames(&ctx, read_from(&stack), |pc| {
            emitted[n] = pc;
            n += 1;
        });
        assert_eq!(&emitted[..n], &[0x1000, 0x5000, 0x6000]);
    }

    /// C: `v_hbp <= v_bp` cycle guard (exception.c:310-312) — stop, do not loop.
    #[test]
    fn test_stacktrace_walk_frames_stops_on_cycle() {
        let mut ctx = X86_64CpuContext::new();
        ctx.rip = 0x1000;
        ctx.gp_regs[5] = 0x2000;
        // Backward jump: saved_fp (0x1000) < current fp (0x2000) → corrupt.
        let stack = [(0x2000u64, 0x1000u64), (0x2008u64, 0x5000u64)];
        let mut n = 0usize;
        X86_64CpuContextArch::walk_frames(&ctx, read_from(&stack), |_| n += 1);
        assert_eq!(n, 2, "pc + one frame, then stop on cycle");
    }

    /// C: PRCOPY failure terminates the walk (exception.c:297-305).
    #[test]
    fn test_stacktrace_walk_frames_stops_on_fault() {
        let mut ctx = X86_64CpuContext::new();
        ctx.rip = 0x1000;
        ctx.gp_regs[5] = 0x2000;
        // saved_fp readable, return_addr slot unmapped → stop.
        let stack = [(0x2000u64, 0x4000u64)];
        let mut n = 0usize;
        X86_64CpuContextArch::walk_frames(&ctx, read_from(&stack), |_| n += 1);
        assert_eq!(n, 1, "only pc emitted; fault stops before emitting");
    }

    /// C: `n > 50` truncation (exception.c:313-316) — Rust caps at 32 (D4).
    #[test]
    fn test_stacktrace_walk_frames_caps_at_max() {
        let mut ctx = X86_64CpuContext::new();
        ctx.rip = 0x1000;
        ctx.gp_regs[5] = 0x2000;
        // Computed fake stack: every frame advances fp by 0x10 forever.
        let mut emitted = 0usize;
        X86_64CpuContextArch::walk_frames(
            &ctx,
            |addr| {
                if addr % 0x10 == 0 {
                    Some(addr + 0x10) // saved_fp
                } else {
                    Some(0x3000 + addr / 0x10) // return_addr
                }
            },
            |_| emitted += 1,
        );
        // MAX_STACK_FRAMES caps the TOTAL emitted frames (pc + frames).
        assert_eq!(emitted, MAX_STACK_FRAMES, "pc + (MAX-1) walk frames");
    }

    /// C: `p_reg.fp` — x86-64 stores RBP at gp_regs[GP_RBP=5].
    #[test]
    fn test_stacktrace_frame_pointer_uses_gp_regs_5() {
        let mut ctx = X86_64CpuContext::new();
        ctx.gp_regs[5] = 0xdead_beef;
        assert_eq!(X86_64CpuContextArch::frame_pointer(&ctx), 0xdead_beef);
        assert_eq!(X86_64CpuContextArch::program_counter(&ctx), 0);
    }
}
