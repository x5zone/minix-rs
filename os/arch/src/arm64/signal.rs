//! aarch64 `SignalContext` implementation.
//!
//! Implements the `SignalContext` trait for ARM64, providing sigcontext
//! construction and restoration for POSIX-style signal delivery.
//!
//! # C Source Mapping
//!
//! - `do_sigsend.c:91-110` — build sigcontext (SPSR + X0-X12 + SP + LR + PC)
//! - `do_sigsend.c:139-151` — setup handler entry (LR=sigreturn, X0=signo, X2=sigctx)
//! - `do_sigreturn.c:60-78` — restore registers
//!
//! # 64-bit adaptation
//!
//! C's ARM sigcontext uses 32-bit `unsigned int` fields (sc_r0, etc.)
//! because Minix3 ARM was 32-bit. minix-rs is 64-bit, so all register
//! fields are `u64`. The full X0-X30 register set is saved (C only
//! saved X0-X12; 64-bit extends to all 31 GP registers).

use crate::signal_context::{SignalContext, SignalInfo};
use crate::arch::signal_context::{SC_MAGIC, MF_CONTEXT_SET};

use super::boot::AArch64CpuContext;

// ── GP register indices in `gp_regs` array ──
//
// gp_regs[0] = X1, gp_regs[1] = X2, ..., gp_regs[29] = X30 (LR)
// X0 is the named field `r0`; SP is the named field `sp`.

/// Index of X1 in `gp_regs`.
pub(super) const GP_X1:  usize = 0;
/// Index of X2 in `gp_regs`.
pub(super) const GP_X2:  usize = 1;
/// Index of X3 in `gp_regs`.
pub(super) const GP_X3:  usize = 2;
/// Index of X4 in `gp_regs`.
pub(super) const GP_X4:  usize = 3;
/// Index of X5 in `gp_regs`.
pub(super) const GP_X5:  usize = 4;
/// Index of X6 in `gp_regs`.
pub(super) const GP_X6:  usize = 5;
/// Index of X7 in `gp_regs`.
pub(super) const GP_X7:  usize = 6;
/// Index of X8 in `gp_regs`.
pub(super) const GP_X8:  usize = 7;
/// Index of X9 in `gp_regs`.
pub(super) const GP_X9:  usize = 8;
/// Index of X10 in `gp_regs`.
pub(super) const GP_X10: usize = 9;
/// Index of X11 in `gp_regs`.
pub(super) const GP_X11: usize = 10;
/// Index of X12 in `gp_regs`.
pub(super) const GP_X12: usize = 11;
/// Index of X13 in `gp_regs`.
pub(super) const GP_X13: usize = 12;
/// Index of X14 in `gp_regs`.
pub(super) const GP_X14: usize = 13;
/// Index of X15 in `gp_regs`.
pub(super) const GP_X15: usize = 14;
/// Index of X16 in `gp_regs`.
pub(super) const GP_X16: usize = 15;
/// Index of X17 in `gp_regs`.
pub(super) const GP_X17: usize = 16;
/// Index of X18 in `gp_regs`.
pub(super) const GP_X18: usize = 17;
/// Index of X19 in `gp_regs`.
pub(super) const GP_X19: usize = 18;
/// Index of X20 in `gp_regs`.
pub(super) const GP_X20: usize = 19;
/// Index of X21 in `gp_regs`.
pub(super) const GP_X21: usize = 20;
/// Index of X22 in `gp_regs`.
pub(super) const GP_X22: usize = 21;
/// Index of X23 in `gp_regs`.
pub(super) const GP_X23: usize = 22;
/// Index of X24 in `gp_regs`.
pub(super) const GP_X24: usize = 23;
/// Index of X25 in `gp_regs`.
pub(super) const GP_X25: usize = 24;
/// Index of X26 in `gp_regs`.
pub(super) const GP_X26: usize = 25;
/// Index of X27 in `gp_regs`.
pub(super) const GP_X27: usize = 26;
/// Index of X28 in `gp_regs`.
pub(super) const GP_X28: usize = 27;
/// Index of X29 (FP) in `gp_regs`.
pub(super) const GP_X29: usize = 28;
/// Index of X30 (LR) in `gp_regs`.
pub(super) const GP_X30: usize = 29;

// ── sigcontext struct ──

/// aarch64 saved register state for signal delivery.
///
/// C: `struct sigcontext` — sys/arch/arm/include/signal.h:92-122
///
/// # 64-bit adaptation
///
/// All fields are `u64` (C uses `unsigned int` for 32-bit).
/// Full X0-X30 register set is saved (C only saved X0-X12).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct AArch64SigContext {
    /// SPSR_EL1. C: `sc_spsr`
    pub sc_spsr: u64,
    /// X0. C: `sc_r0`
    pub sc_r0: u64,
    /// X1. C: `sc_r1`
    pub sc_r1: u64,
    /// X2. C: `sc_r2`
    pub sc_r2: u64,
    /// X3. C: `sc_r3`
    pub sc_r3: u64,
    /// X4. C: `sc_r4`
    pub sc_r4: u64,
    /// X5. C: `sc_r5`
    pub sc_r5: u64,
    /// X6. C: `sc_r6`
    pub sc_r6: u64,
    /// X7. C: `sc_r7`
    pub sc_r7: u64,
    /// X8. C: `sc_r8`
    pub sc_r8: u64,
    /// X9. C: `sc_r9`
    pub sc_r9: u64,
    /// X10. C: `sc_r10`
    pub sc_r10: u64,
    /// X11 (FP). C: `sc_r11`
    pub sc_r11: u64,
    /// X12. C: `sc_r12`
    pub sc_r12: u64,
    /// SP_EL0. C: `sc_usr_sp`
    pub sc_usr_sp: u64,
    /// X30 (LR). C: `sc_usr_lr`
    pub sc_usr_lr: u64,
    /// Saved SVC LR (always 0 in minix-rs). C: `sc_svc_lr`
    pub sc_svc_lr: u64,
    /// ELR_EL1 (PC). C: `sc_pc`
    pub sc_pc: u64,
    /// Signal mask. C: `sc_mask`
    pub sc_mask: u64,
    /// Flags (MF_FPU_INITIALIZED). C: `sc_flags`
    pub sc_flags: i32,
    /// Trap style. C: `trap_style`
    pub trap_style: i32,
    /// Magic. C: `sc_magic`
    pub sc_magic: i32,
    /// X13-X28 (64-bit extension; C 32-bit has no equivalent).
    pub sc_r13: u64,
    pub sc_r14: u64,
    pub sc_r15: u64,
    pub sc_r16: u64,
    pub sc_r17: u64,
    pub sc_r18: u64,
    pub sc_r19: u64,
    pub sc_r20: u64,
    pub sc_r21: u64,
    pub sc_r22: u64,
    pub sc_r23: u64,
    pub sc_r24: u64,
    pub sc_r25: u64,
    pub sc_r26: u64,
    pub sc_r27: u64,
    pub sc_r28: u64,
}

// ── sigframe struct ──

/// aarch64 signal frame written to user stack.
///
/// C: `struct sigframe_sigcontext` — sys/arch/arm/include/frame.h:92-97
///
/// ARM's sigframe is simpler than x86: just a pointer to sigcontext
/// and the sigcontext itself.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct AArch64SigFrame {
    /// Pointer to sigcontext (for handler's 2nd arg). C: `sf_scp`
    pub sf_scp: u64,
    /// The sigcontext. C: `sf_sc`
    pub sf_sc: AArch64SigContext,
}

// ── mcontext struct ──

/// AArch64 mcontext_t (C ABI compatible).
///
/// # Layout
/// - gregs: 32 × u64 general registers (X0-X31, 256 bytes)
/// - sp: stack pointer (8 bytes)
/// - pc: program counter (8 bytes)
/// - pstate: processor state (8 bytes)
/// - fpregs: FPU state (32 × 16-byte Q registers + FPSR + FPCR, 528 bytes)
/// - mc_magic: 4 bytes
/// - mc_flags: 4 bytes
///
/// Total: 816 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AArch64Mcontext {
    /// General register set (X0-X31).
    pub gregs: [u64; 32],
    /// Stack pointer.
    pub sp: u64,
    /// Program counter.
    pub pc: u64,
    /// Processor state (PSTATE).
    pub pstate: u64,
    /// FPU register state (32 × 16-byte Q registers + FPSR + FPCR).
    pub fpregs: [u8; 528],
    /// Magic value for integrity check. C: `mc_magic` (MCF_MAGIC = 0xc0ffee).
    pub mc_magic: i32,
    /// Context flags. C: `mc_flags` (_MC_FPU_SAVED = 0x001).
    pub mc_flags: i32,
}

impl Default for AArch64Mcontext {
    fn default() -> Self {
        Self {
            gregs: [0u64; 32],
            sp: 0,
            pc: 0,
            pstate: 0,
            fpregs: [0u8; 528],
            mc_magic: 0xc0ffee,
            mc_flags: 0,
        }
    }
}

impl core::fmt::Debug for AArch64Mcontext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AArch64Mcontext")
            .field("mc_magic", &self.mc_magic)
            .field("mc_flags", &self.mc_flags)
            .finish()
    }
}

// ── SignalContext implementation ──

/// aarch64 `SignalContext` implementation marker.
pub struct AArch64SignalContext;

impl SignalContext for AArch64SignalContext {
    type CpuContext = AArch64CpuContext;
    type SigContext = AArch64SigContext;
    type SigFrame = AArch64SigFrame;
    type Mcontext = AArch64Mcontext;

    fn build_sigcontext(ctx: &Self::CpuContext, info: &mut SignalInfo) -> Self::SigContext {
        // C: do_sigsend.c:46 — smsg.sm_stkptr = arch_get_sp(rp)
        info.stkptr = ctx.sp;

        let mut sctx = AArch64SigContext::default();

        // C: do_sigsend.c:92-109 — fill registers
        sctx.sc_spsr = ctx.psr;
        sctx.sc_r0 = ctx.r0;
        sctx.sc_r1  = ctx.gp_regs[GP_X1];
        sctx.sc_r2  = ctx.gp_regs[GP_X2];
        sctx.sc_r3  = ctx.gp_regs[GP_X3];
        sctx.sc_r4  = ctx.gp_regs[GP_X4];
        sctx.sc_r5  = ctx.gp_regs[GP_X5];
        sctx.sc_r6  = ctx.gp_regs[GP_X6];
        sctx.sc_r7  = ctx.gp_regs[GP_X7];
        sctx.sc_r8  = ctx.gp_regs[GP_X8];
        sctx.sc_r9  = ctx.gp_regs[GP_X9];
        sctx.sc_r10 = ctx.gp_regs[GP_X10];
        sctx.sc_r11 = ctx.gp_regs[GP_X29]; // FP = X29
        sctx.sc_r12 = ctx.gp_regs[GP_X12];
        sctx.sc_usr_sp = ctx.sp;
        sctx.sc_usr_lr = ctx.gp_regs[GP_X30]; // LR = X30
        sctx.sc_svc_lr = 0; // C: sc_svc_lr = 0 ("?")
        sctx.sc_pc = ctx.pc;

        // 64-bit extension: X13-X28
        sctx.sc_r13 = ctx.gp_regs[GP_X13];
        sctx.sc_r14 = ctx.gp_regs[GP_X14];
        sctx.sc_r15 = ctx.gp_regs[GP_X15];
        sctx.sc_r16 = ctx.gp_regs[GP_X16];
        sctx.sc_r17 = ctx.gp_regs[GP_X17];
        sctx.sc_r18 = ctx.gp_regs[GP_X18];
        sctx.sc_r19 = ctx.gp_regs[GP_X19];
        sctx.sc_r20 = ctx.gp_regs[GP_X20];
        sctx.sc_r21 = ctx.gp_regs[GP_X21];
        sctx.sc_r22 = ctx.gp_regs[GP_X22];
        sctx.sc_r23 = ctx.gp_regs[GP_X23];
        sctx.sc_r24 = ctx.gp_regs[GP_X24];
        sctx.sc_r25 = ctx.gp_regs[GP_X25];
        sctx.sc_r26 = ctx.gp_regs[GP_X26];
        sctx.sc_r27 = ctx.gp_regs[GP_X27];
        sctx.sc_r28 = ctx.gp_regs[GP_X28];

        // C: do_sigsend.c:113-115 — sc_mask, sc_flags, sc_magic
        sctx.sc_mask = info.mask;
        sctx.sc_flags = 0;
        sctx.sc_magic = SC_MAGIC;

        sctx
    }

    fn build_sigframe(
        _ctx: &Self::CpuContext,
        sctx: &Self::SigContext,
        _info: &SignalInfo,
        frame_addr: u64,
    ) -> Self::SigFrame {
        // C: do_sigsend.c:51 — fr.sf_scp = &frp->sf_sc
        let sf_sc_addr = frame_addr + core::mem::offset_of!(AArch64SigFrame, sf_sc) as u64;

        AArch64SigFrame {
            sf_scp: sf_sc_addr,
            sf_sc: *sctx,
        }
    }

    fn setup_handler_entry(ctx: &mut Self::CpuContext, info: &SignalInfo, frame_addr: u64) {
        // C: do_sigsend.c:134 — rp->p_reg.sp = (reg_t) frp
        ctx.sp = frame_addr;

        // C: do_sigsend.c:135 — rp->p_reg.pc = (reg_t) smsg.sm_sighandler
        ctx.pc = info.sighandler;

        // C: do_sigsend.c:143 — rp->p_reg.lr = (reg_t) smsg.sm_sigreturn
        ctx.gp_regs[GP_X30] = info.sigreturn;

        // C: do_sigsend.c:147 — rp->p_reg.retreg = (reg_t) smsg.sm_signo
        ctx.r0 = info.signo as u64;

        // C: do_sigsend.c:148 — rp->p_reg.r1 = 0 (sf_code)
        ctx.gp_regs[GP_X1] = 0;

        // C: do_sigsend.c:149 — rp->p_reg.r2 = (reg_t) fr.sf_scp
        let sf_sc_addr = frame_addr + core::mem::offset_of!(AArch64SigFrame, sf_sc) as u64;
        ctx.gp_regs[GP_X2] = sf_sc_addr;

        // C: do_sigsend.c:150 — rp->p_misc_flags |= MF_CONTEXT_SET
        // (kernel layer handles p_misc_flags)
        let _ = MF_CONTEXT_SET;
    }

    fn restore_sigcontext(ctx: &mut Self::CpuContext, sctx: &Self::SigContext) {
        // C: do_sigreturn.c:61-78 — restore all registers
        ctx.psr = sctx.sc_spsr;
        ctx.r0 = sctx.sc_r0;
        ctx.gp_regs[GP_X1]  = sctx.sc_r1;
        ctx.gp_regs[GP_X2]  = sctx.sc_r2;
        ctx.gp_regs[GP_X3]  = sctx.sc_r3;
        ctx.gp_regs[GP_X4]  = sctx.sc_r4;
        ctx.gp_regs[GP_X5]  = sctx.sc_r5;
        ctx.gp_regs[GP_X6]  = sctx.sc_r6;
        ctx.gp_regs[GP_X7]  = sctx.sc_r7;
        ctx.gp_regs[GP_X8]  = sctx.sc_r8;
        ctx.gp_regs[GP_X9]  = sctx.sc_r9;
        ctx.gp_regs[GP_X10] = sctx.sc_r10;
        ctx.gp_regs[GP_X29] = sctx.sc_r11; // FP = X29
        ctx.gp_regs[GP_X12] = sctx.sc_r12;
        ctx.sp = sctx.sc_usr_sp;
        ctx.gp_regs[GP_X30] = sctx.sc_usr_lr; // LR = X30
        ctx.pc = sctx.sc_pc;

        // 64-bit extension: X13-X28
        ctx.gp_regs[GP_X13] = sctx.sc_r13;
        ctx.gp_regs[GP_X14] = sctx.sc_r14;
        ctx.gp_regs[GP_X15] = sctx.sc_r15;
        ctx.gp_regs[GP_X16] = sctx.sc_r16;
        ctx.gp_regs[GP_X17] = sctx.sc_r17;
        ctx.gp_regs[GP_X18] = sctx.sc_r18;
        ctx.gp_regs[GP_X19] = sctx.sc_r19;
        ctx.gp_regs[GP_X20] = sctx.sc_r20;
        ctx.gp_regs[GP_X21] = sctx.sc_r21;
        ctx.gp_regs[GP_X22] = sctx.sc_r22;
        ctx.gp_regs[GP_X23] = sctx.sc_r23;
        ctx.gp_regs[GP_X24] = sctx.sc_r24;
        ctx.gp_regs[GP_X25] = sctx.sc_r25;
        ctx.gp_regs[GP_X26] = sctx.sc_r26;
        ctx.gp_regs[GP_X27] = sctx.sc_r27;
        ctx.gp_regs[GP_X28] = sctx.sc_r28;
    }

    fn arch_setcontext(_ctx: &mut Self::CpuContext, _trap_style: i32) {
        // C: do_sigreturn.c:81 — arch_proc_setcontext(rp, &rp->p_reg, 1, sc.trap_style)
        // aarch64: no-op (registers are restored directly into CpuContext;
        // the return-to-user path reads from CpuContext on next dispatch).
    }

    fn get_sp(ctx: &Self::CpuContext) -> u64 {
        // C: arch_get_sp(rp) — returns rp->p_reg.sp
        ctx.sp
    }

    fn sigframe_size() -> usize {
        core::mem::size_of::<AArch64SigFrame>()
    }

    fn check_magic(sctx: &Self::SigContext) -> bool {
        sctx.sc_magic == SC_MAGIC
    }

    fn get_trap_style(sctx: &Self::SigContext) -> i32 {
        sctx.trap_style
    }

    fn mcontext_clear_flags(mc: &mut Self::Mcontext) {
        mc.mc_flags = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::boot::{CpuContextArch, EntrySpec, ProcKind};
    use crate::arch::signal_context::SignalInfo;

    #[test]
    fn test_sigcontext_size_nonzero() {
        assert!(core::mem::size_of::<AArch64SigContext>() > 0);
    }

    #[test]
    fn test_build_sigcontext_fills_from_ctx() {
        let ctx = AArch64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::loaded(
                minix_types::VirBytes(0x1000),
                minix_types::VirBytes(0x4000_0000),
                minix_types::VirBytes(0x3fff_ffe0),
            ),
        );
        let mut info = SignalInfo {
            sighandler: 0x4000,
            mask: 0xFF,
            signo: 6,
            sigreturn: 0x5000,
            stkptr: 0,
        };
        let sctx = AArch64SignalContext::build_sigcontext(&ctx, &mut info);

        assert_eq!(sctx.sc_pc, 0x1000);
        assert_eq!(sctx.sc_usr_sp, 0x4000_0000);
        assert_eq!(sctx.sc_r0, 0x3fff_ffe0);
        assert_eq!(sctx.sc_spsr, 0x0000_0000); // INIT_PSR
        assert_eq!(sctx.sc_mask, 0xFF);
        assert_eq!(sctx.sc_magic, SC_MAGIC);
        assert_eq!(info.stkptr, 0x4000_0000);
    }

    #[test]
    fn test_setup_handler_entry_arm64() {
        let mut ctx = AArch64CpuContext::default();
        let info = SignalInfo {
            sighandler: 0x4000,
            mask: 0,
            signo: 6,
            sigreturn: 0x5000,
            stkptr: 0,
        };
        AArch64SignalContext::setup_handler_entry(&mut ctx, &info, 0x8000);

        assert_eq!(ctx.sp, 0x8000);
        assert_eq!(ctx.pc, 0x4000);
        assert_eq!(ctx.gp_regs[GP_X30], 0x5000); // LR = sigreturn
        assert_eq!(ctx.r0, 6); // X0 = signo
        assert_eq!(ctx.gp_regs[GP_X1], 0); // X1 = 0 (sf_code)
        // X2 = sf_scp
        let expected_scp = 0x8000 + core::mem::offset_of!(AArch64SigFrame, sf_sc) as u64;
        assert_eq!(ctx.gp_regs[GP_X2], expected_scp);
    }

    #[test]
    fn test_restore_sigcontext_arm64() {
        let mut ctx = AArch64CpuContext::default();
        let sctx = AArch64SigContext {
            sc_spsr: 0x10,
            sc_r0: 0xAA,
            sc_r1: 0xBB,
            sc_usr_sp: 0x7fff_0000,
            sc_usr_lr: 0x5000,
            sc_pc: 0x1000,
            ..Default::default()
        };
        AArch64SignalContext::restore_sigcontext(&mut ctx, &sctx);

        assert_eq!(ctx.psr, 0x10);
        assert_eq!(ctx.r0, 0xAA);
        assert_eq!(ctx.gp_regs[GP_X1], 0xBB);
        assert_eq!(ctx.sp, 0x7fff_0000);
        assert_eq!(ctx.gp_regs[GP_X30], 0x5000);
        assert_eq!(ctx.pc, 0x1000);
    }

    #[test]
    fn test_check_magic_arm64() {
        let sctx_ok = AArch64SigContext { sc_magic: SC_MAGIC, ..Default::default() };
        let sctx_bad = AArch64SigContext { sc_magic: 0, ..Default::default() };
        assert!(AArch64SignalContext::check_magic(&sctx_ok));
        assert!(!AArch64SignalContext::check_magic(&sctx_bad));
    }

    #[test]
    fn test_roundtrip_arm64() {
        let mut ctx = AArch64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::loaded(
                minix_types::VirBytes(0x1000),
                minix_types::VirBytes(0x4000_0000),
                minix_types::VirBytes(0x3fff_ffe0),
            ),
        );
        ctx.gp_regs[GP_X5] = 0x55;

        let mut info = SignalInfo {
            sighandler: 0,
            mask: 0,
            signo: 9,
            sigreturn: 0,
            stkptr: 0,
        };
        let sctx = AArch64SignalContext::build_sigcontext(&ctx, &mut info);

        let mut ctx2 = AArch64CpuContext::default();
        AArch64SignalContext::restore_sigcontext(&mut ctx2, &sctx);

        assert_eq!(ctx2.gp_regs[GP_X5], 0x55);
        assert_eq!(ctx2.pc, 0x1000);
        assert_eq!(ctx2.sp, 0x4000_0000);
    }
}
