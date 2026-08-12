//! Signal context architecture abstraction.
//!
//! Defines the `SignalContext` trait for architecture-specific signal
//! frame (sigcontext / sigframe) construction and restoration.
//!
//! # Why a separate trait?
//!
//! Minix3 C source has `#if defined(__i386__)` / `#if defined(__arm__)`
//! blocks in `do_sigsend.c` and `do_sigreturn.c` for register save and
//! restore. The Rust rewrite abstracts those operations through
//! `SignalContext` so the kernel dispatch layer has no
//! `#[cfg(target_arch)]`.
//!
//! # Design (19-syscall-signal.md §3 D6)
//!
//! - **Trait + associated types + static dispatch**: Each architecture
//!   defines its own `SigContext` / `SigFrame` / `CpuContext` types.
//!   The kernel uses `CurrentSignalContext` type alias for compile-time
//!   monomorphization — zero virtual overhead, `no_std`-friendly.
//! - **SignalInfo**: Arch-layer equivalent of the kernel's `SigMsg`. Uses
//!   `u64` for the signal mask (instead of the kernel's `SigSet` newtype)
//!   to avoid a dependency on the kernel crate.
//! - **CpuContext as register source**: The trait reads/writes the
//!   process's saved register state through `&Self::CpuContext` /
//!   `&mut Self::CpuContext`. This is the arch-private type stored in
//!   `KProcess.cpu_context`.
//!
//! # Architecture mapping
//!
//! | Method | x86-64 | ARM64 | RISC-V |
//! |--------|--------|-------|--------|
//! | `build_sigcontext` | Save GS/FS/ES/DS + GP regs + RIP/RFLAGS/RSP/SS | Save SPSR + X0-X30 + SP + LR + PC | Save sstatus + X0-X31 + sepc |
//! | `build_sigframe` | Fill `sigframe_sigcontext` with ret addr + signum + scp + sigcontext | Fill `sigframe_sigcontext` with scp + sigcontext | Fill sigframe with sigreturn trampoline + sigcontext |
//! | `setup_handler_entry` | SP=sigframe, PC=sighandler, FP=new_fp | LR=sigreturn, X0=signo, X2=sigctx | RA=sigreturn, A0=signo, A1=sigctx |
//! | `restore_sigcontext` | Merge RFLAGS user bits + restore GP regs | Restore full SPSR + GP regs | Restore sstatus + GP regs |
//! | `get_sp` | `ctx.rsp` | `ctx.sp` | `ctx.sp` |
//! | `sigframe_size` | `sizeof(sigframe_sigcontext)` | same | same |
//!
//! # C Source Mapping
//!
//! - `do_sigsend.c:50-115` — build sigcontext from process registers
//! - `do_sigsend.c:117-118` — fill sigframe
//! - `do_sigsend.c:130-145` — modify registers to enter handler
//! - `do_sigreturn.c:42-80` — restore registers from sigcontext
//! - `do_sigreturn.c:82-84` — `arch_proc_setcontext()`
//!
//! # Idempotency constraint (D3, 19-syscall-signal.md §1.3)
//!
//! `build_sigcontext` and `build_sigframe` are **idempotent** — they only
//! read `CpuContext` and produce new values. `setup_handler_entry` is
//! **NOT idempotent** — it mutates `CpuContext`. It MUST be called only
//! after the last `data_copy_vmcheck` succeeds (which may VMSUSPEND).

// ── Constants ──

/// Magic value written to `sigcontext.sc_magic` for integrity check.
///
/// C: `SC_MAGIC` — arch/i386/include/signal.h:115 (0xc0ffee1),
/// arch/arm/include/signal.h:117 (0xc0ffee2).
///
/// minix-rs uses a single value across architectures to simplify testing.
/// The x86-64 value (0xc0ffee1) is chosen as the canonical one.
pub const SC_MAGIC: i32 = 0xc0ffee1;

/// Flag: FPU state in sigcontext is valid.
/// C: `MF_FPU_INITIALIZED` — proc.h:248 (0x1000)
pub const MF_FPU_INITIALIZED: i32 = 0x1000;

/// Flag: context has been set by sigsend, do not clobber on next dispatch.
/// C: `MF_CONTEXT_SET` — proc.h:251 (0x4000)
pub const MF_CONTEXT_SET: i32 = 0x4000;

/// x86 user-modifiable RFLAGS bits (status + control flags).
/// C: `X86_FLAGS_USER` — sys/arch/i386/include/cpu.h:98
/// = PSL_C | PSL_PF | PSL_AF | PSL_Z | PSL_N | PSL_D | PSL_V
/// = 0x00000CD7
pub const X86_FLAGS_USER: u64 = 0x0000_0CD7;

// ── Trap style constants (C: archconst.h:167-172) ──

/// Trap style: invalid / not yet saved.
pub const KTS_NONE: i32 = 1;

/// Trap style: exception or hard interrupt.
pub const KTS_INT_HARD: i32 = 2;

/// Trap style: soft interrupt from libc.
pub const KTS_INT_ORIG: i32 = 3;

/// Trap style: soft interrupt from usermapped code.
pub const KTS_INT_UM: i32 = 4;

/// Trap style: must restore full context.
pub const KTS_FULLCONTEXT: i32 = 5;

/// Trap style: SYSENTER instruction.
pub const KTS_SYSENTER: i32 = 6;

// ── SignalInfo ──

/// Signal delivery parameters (arch-layer equivalent of kernel's `SigMsg`).
///
/// The kernel converts its `SigMsg` to this struct when calling
/// `SignalContext` trait methods. Uses `u64` for the mask (instead of
/// the kernel's `SigSet` newtype) to avoid a dependency on the kernel
/// crate.
///
/// C: `struct sigmsg` — sigcontext.h
#[derive(Debug, Clone, Copy, Default)]
pub struct SignalInfo {
    /// Signal handler address. C: `sm_sighandler`
    pub sighandler: u64,
    /// Signal mask to block during handler. C: `sm_mask` (raw u64)
    pub mask: u64,
    /// Signal number. C: `sm_signo`
    pub signo: u32,
    /// Return address for sigreturn. C: `sm_sigreturn`
    pub sigreturn: u64,
    /// User stack pointer at signal time. C: `sm_stkptr`
    /// Filled by `build_sigcontext` (overwrites caller's value with
    /// `get_sp(proc)`).
    pub stkptr: u64,
}

// ── SignalContext trait ──

/// Architecture abstraction for signal context save/restore.
///
/// Each architecture implements this trait to:
/// 1. Build a `SigContext` (saved register snapshot) from `CpuContext`
/// 2. Build a `SigFrame` (user-stack signal frame) from `SigContext`
/// 3. Modify `CpuContext` to enter the signal handler
/// 4. Restore `CpuContext` from a `SigContext` (sigreturn)
///
/// # Associated Types
///
/// - `CpuContext`: The arch-private CPU context stored in `KProcess`.
///   Must be `Copy + Default` (same bound as `CpuContextArch`).
/// - `SigContext`: The saved-register struct (`struct sigcontext` in C).
///   Must be `Copy + Default` so the kernel can zero-init it before
///   `build_sigcontext` fills it.
/// - `SigFrame`: The user-stack frame (`struct sigframe_sigcontext` in C).
///   Must be `Copy + Default` for the same reason.
///
/// # Safety: `setup_handler_entry` timing
///
/// `setup_handler_entry` mutates `CpuContext` (sets SP, PC, FP/LR).
/// It MUST be called only after the last `data_copy_vmcheck` succeeds.
/// If called before the copy, a VMSUSPEND recovery would re-execute the
/// syscall and call `setup_handler_entry` again, corrupting the register
/// state. The kernel dispatch layer enforces this ordering.
///
/// C: `do_sigsend.c:126-131` (WARNING comment about this constraint)
pub trait SignalContext: Sized + Send + Sync {
    /// Arch-private CPU context (same type as `CpuContextArch::CpuContext`).
    type CpuContext: Copy + core::fmt::Debug + Default + Send + Sync;
    /// Saved register state (`struct sigcontext` in C).
    type SigContext: Copy + core::fmt::Debug + Default + Send + Sync;
    /// User-stack signal frame (`struct sigframe_sigcontext` in C).
    type SigFrame: Copy + core::fmt::Debug + Default + Send + Sync;

    /// Build a `SigContext` from the process's current register state.
    ///
    /// Reads segment registers, GP registers, PC, SP, and flags from
    /// `ctx`. Fills `info.stkptr` with `get_sp(ctx)` if the caller
    /// hasn't already (caller passes `info` by mut ref so this method
    /// can update `stkptr`).
    ///
    /// C: `do_sigsend.c:50-115`
    fn build_sigcontext(ctx: &Self::CpuContext, info: &mut SignalInfo) -> Self::SigContext;

    /// Build a `SigFrame` from the sigcontext, ready to copy to user stack.
    ///
    /// Computes the frame's internal pointers (return address trampoline,
    /// sigcontext pointer, signum slot) based on `frame_addr`.
    ///
    /// C: `do_sigsend.c:49-58, 117-118`
    fn build_sigframe(
        ctx: &Self::CpuContext,
        sctx: &Self::SigContext,
        info: &SignalInfo,
        frame_addr: u64,
    ) -> Self::SigFrame;

    /// Modify `CpuContext` to enter the signal handler.
    ///
    /// Sets SP to `frame_addr`, PC to `sighandler`, and the
    /// architecture-specific return-address register (FP/LR/RA) to
    /// `sigreturn`.
    ///
    /// # SAFETY (timing constraint)
    ///
    /// MUST be called only after the sigframe has been successfully
    /// copied to user space. See the trait-level doc comment.
    ///
    /// C: `do_sigsend.c:130-145`
    fn setup_handler_entry(ctx: &mut Self::CpuContext, info: &SignalInfo, frame_addr: u64);

    /// Restore `CpuContext` from a `SigContext`.
    ///
    /// x86-64: merges RFLAGS user bits (preserves system bits like IF).
    /// ARM64: restores full SPSR.
    /// RISC-V: restores sstatus.
    ///
    /// C: `do_sigreturn.c:42-80`
    fn restore_sigcontext(ctx: &mut Self::CpuContext, sctx: &Self::SigContext);

    /// Architecture-specific post-restore hook.
    ///
    /// Called after `restore_sigcontext`. Sets the trap-style flag so
    /// the return-to-user path knows how to restore registers.
    ///
    /// C: `do_sigreturn.c:81` — `arch_proc_setcontext(rp, &rp->p_reg, 1, sc.trap_style)`
    fn arch_setcontext(ctx: &mut Self::CpuContext, trap_style: i32);

    /// Get the current stack pointer of the process.
    ///
    /// C: `arch_get_sp(rp)` — do_sigsend.c:46
    fn get_sp(ctx: &Self::CpuContext) -> u64;

    /// Size of the `SigFrame` structure for stack adjustment.
    ///
    /// C: `sizeof(struct sigframe_sigcontext)` — do_sigsend.c:50
    fn sigframe_size() -> usize;

    /// Check if the sigcontext magic value is valid.
    ///
    /// C: `do_sigreturn.c:83` — `if(sc.sc_magic != SC_MAGIC)`
    fn check_magic(sctx: &Self::SigContext) -> bool;

    /// Extract the trap-style field from a sigcontext.
    ///
    /// The kernel dispatch layer needs `trap_style` to pass to
    /// `arch_setcontext`. Since `SigContext` is arch-specific, only
    /// the arch implementation can read this field.
    ///
    /// C: `do_sigreturn.c:81` — `sc.trap_style`
    fn get_trap_style(sctx: &Self::SigContext) -> i32;

    /// Architecture's mcontext_t struct (C ABI compatible).
    /// Used by SYS_GETMCONTEXT / SYS_SETMCONTEXT.
    type Mcontext: Copy + core::fmt::Debug + Default + Send + Sync;

    /// Zero the mc_flags field in an mcontext buffer.
    /// C: `mc.mc_flags = 0` — do_mcontext.c:47
    fn mcontext_clear_flags(mc: &mut Self::Mcontext);
}

// ── Mock SignalContext (for tests) ──

/// Mock sigcontext — zero-sized, no real register state.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockSigContext;

/// Mock sigframe — zero-sized.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockSigFrame;

/// Mock CPU context — zero-sized.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockSigCpuContext;

/// Mock mcontext — zero-sized.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockMcontext;

/// Mock signal context implementation — all operations are no-ops.
///
/// Used when `feature = "mock"` is enabled (test mode). The mock types
/// are zero-sized so there is no memory overhead.
pub struct MockSignalContext;

impl SignalContext for MockSignalContext {
    type CpuContext = MockSigCpuContext;
    type SigContext = MockSigContext;
    type SigFrame = MockSigFrame;
    type Mcontext = MockMcontext;

    fn build_sigcontext(_ctx: &Self::CpuContext, _info: &mut SignalInfo) -> Self::SigContext {
        MockSigContext
    }

    fn build_sigframe(
        _ctx: &Self::CpuContext,
        _sctx: &Self::SigContext,
        _info: &SignalInfo,
        _frame_addr: u64,
    ) -> Self::SigFrame {
        MockSigFrame
    }

    fn setup_handler_entry(_ctx: &mut Self::CpuContext, _info: &SignalInfo, _frame_addr: u64) {}

    fn restore_sigcontext(_ctx: &mut Self::CpuContext, _sctx: &Self::SigContext) {}

    fn arch_setcontext(_ctx: &mut Self::CpuContext, _trap_style: i32) {}

    fn get_sp(_ctx: &Self::CpuContext) -> u64 { 0 }

    fn sigframe_size() -> usize { 0 }

    fn check_magic(_sctx: &Self::SigContext) -> bool { true }

    fn get_trap_style(_sctx: &Self::SigContext) -> i32 { KTS_NONE }

    fn mcontext_clear_flags(_mc: &mut Self::Mcontext) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_info_default() {
        let info = SignalInfo::default();
        assert_eq!(info.sighandler, 0);
        assert_eq!(info.mask, 0);
        assert_eq!(info.signo, 0);
        assert_eq!(info.sigreturn, 0);
        assert_eq!(info.stkptr, 0);
    }

    #[test]
    fn test_sc_magic_value() {
        assert_eq!(SC_MAGIC, 0xc0ffee1);
    }

    #[test]
    fn test_mf_flag_values() {
        assert_eq!(MF_FPU_INITIALIZED, 0x1000);
        assert_eq!(MF_CONTEXT_SET, 0x4000);
    }

    #[test]
    fn test_kts_trap_style_values() {
        assert_eq!(KTS_NONE, 1);
        assert_eq!(KTS_INT_HARD, 2);
        assert_eq!(KTS_FULLCONTEXT, 5);
    }

    #[test]
    fn test_x86_flags_user_mask() {
        // PSL_C|PF|AF|Z|N|D|V = 0x00000CD7
        assert_eq!(X86_FLAGS_USER, 0x0000_0CD7);
    }

    #[test]
    fn test_mock_signal_context_roundtrip() {
        let ctx = MockSigCpuContext;
        let mut info = SignalInfo {
            sighandler: 0x4000,
            signo: 6,
            sigreturn: 0x5000,
            ..Default::default()
        };
        let sctx = MockSignalContext::build_sigcontext(&ctx, &mut info);
        let frame = MockSignalContext::build_sigframe(&ctx, &sctx, &info, 0x7000);
        let mut ctx2 = ctx;
        MockSignalContext::setup_handler_entry(&mut ctx2, &info, 0x7000);
        MockSignalContext::restore_sigcontext(&mut ctx2, &sctx);
        MockSignalContext::arch_setcontext(&mut ctx2, KTS_INT_HARD);
        assert_eq!(MockSignalContext::get_sp(&ctx2), 0);
        assert_eq!(MockSignalContext::sigframe_size(), 0);
        assert!(MockSignalContext::check_magic(&sctx));
        let _ = frame; // suppress unused warning
    }
}
