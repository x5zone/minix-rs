//! x86-64 `SignalContext` implementation.
//!
//! Implements the `SignalContext` trait for x86-64, providing sigcontext
//! construction and restoration for POSIX-style signal delivery.
//!
//! # C Source Mapping
//!
//! - `do_sigsend.c:53-89` — build sigcontext (segment + GP regs + FPU)
//! - `do_sigsend.c:134-138,154` — setup handler entry (SP/PC/FP)
//! - `do_sigreturn.c:40-58` — restore registers (merge RFLAGS user bits)
//!
//! # 64-bit adaptation
//!
//! C's `sigcontext` uses 32-bit `int` fields (sc_edi, sc_esi, etc.) because
//! Minix3 was 32-bit. minix-rs is 64-bit, so all register fields are `u64`.
//! The `sc_cs`/`sc_ss` segment selectors are also `u64` for alignment,
//! matching what FXSAVE/RXRSTOR expect.

use crate::signal_context::{SignalContext, SignalInfo};
use crate::arch::signal_context::{
    SC_MAGIC, X86_FLAGS_USER,
    KTS_NONE,
};

use super::boot::X86_64CpuContext;

// ── GP register indices in `gp_regs` array ──

/// Index into `X86_64CpuContext::gp_regs`.
///
/// The array holds 14 GP registers that are NOT named fields in
/// `X86_64CpuContext` (RIP, RSP, RBX are named fields).
pub(super) const GP_RAX: usize = 0;
pub(super) const GP_RCX: usize = 1;
pub(super) const GP_RDX: usize = 2;
pub(super) const GP_RSI: usize = 3;
pub(super) const GP_RDI: usize = 4;
pub(super) const GP_RBP: usize = 5;
pub(super) const GP_R8:  usize = 6;
pub(super) const GP_R9:  usize = 7;
pub(super) const GP_R10: usize = 8;
pub(super) const GP_R11: usize = 9;
pub(super) const GP_R12: usize = 10;
pub(super) const GP_R13: usize = 11;
pub(super) const GP_R14: usize = 12;
pub(super) const GP_R15: usize = 13;

// ── sigcontext struct ──

/// x86-64 saved register state for signal delivery.
///
/// C: `struct sigcontext` — sys/arch/i386/include/signal.h:84-117
///
/// # 64-bit adaptation
///
/// All fields are `u64` (C uses `int` for 32-bit). The layout is NOT
/// binary-compatible with the C struct — minix-rs uses its own layout
/// because the sigframe is only exchanged between the kernel and the
/// minix-rs user-space signal library (no C ABI compatibility required).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct X86_64SigContext {
    /// GS selector. C: `sc_gs`
    pub sc_gs: u64,
    /// FS selector. C: `sc_fs`
    pub sc_fs: u64,
    /// ES selector. C: `sc_es`
    pub sc_es: u64,
    /// DS selector. C: `sc_ds`
    pub sc_ds: u64,
    /// RDI. C: `sc_edi`
    pub sc_rdi: u64,
    /// RSI. C: `sc_esi`
    pub sc_rsi: u64,
    /// RBP. C: `sc_ebp`
    pub sc_rbp: u64,
    /// RBX. C: `sc_ebx`
    pub sc_rbx: u64,
    /// RDX. C: `sc_edx`
    pub sc_rdx: u64,
    /// RCX. C: `sc_ecx`
    pub sc_rcx: u64,
    /// RAX. C: `sc_eax`
    pub sc_rax: u64,
    /// RIP. C: `sc_eip`
    pub sc_rip: u64,
    /// CS selector. C: `sc_cs`
    pub sc_cs: u64,
    /// RFLAGS. C: `sc_eflags`
    pub sc_rflags: u64,
    /// RSP. C: `sc_esp`
    pub sc_rsp: u64,
    /// SS selector. C: `sc_ss`
    pub sc_ss: u64,
    /// Signal mask to restore. C: `sc_mask`
    pub sc_mask: u64,
    /// FPU state valid flag (MF_FPU_INITIALIZED). C: `sc_flags`
    pub sc_flags: i32,
    /// Trap style (KTS_*). C: `trap_style`
    pub trap_style: i32,
    /// Integrity magic (SC_MAGIC). C: `sc_magic`
    pub sc_magic: i32,
    /// R8-R15 (64-bit extension; C 32-bit has no equivalent).
    pub sc_r8:  u64,
    pub sc_r9:  u64,
    pub sc_r10: u64,
    pub sc_r11: u64,
    pub sc_r12: u64,
    pub sc_r13: u64,
    pub sc_r14: u64,
    pub sc_r15: u64,
}

// ── sigframe struct ──

/// x86-64 signal frame written to user stack.
///
/// C: `struct sigframe_sigcontext` — sys/arch/i386/include/frame.h:151-168
///
/// Layout (matching C):
/// ```text
/// sf_ra_sigreturn  — return address for sigreturn trampoline
/// sf_signum        — signal number argument for handler
/// sf_code          — code argument (always 0 in minix-rs)
/// sf_scp           — pointer to sigcontext (for handler's 3rd arg)
/// sf_fp            — saved frame pointer
/// sf_ra            — actual return address (copy of handler return)
/// sf_scpcopy       — minix scp copy (pointer to sf_sc)
/// sf_sc            — the sigcontext itself
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct X86_64SigFrame {
    /// Return address for sigreturn trampoline. C: `sf_ra_sigreturn`
    pub sf_ra_sigreturn: u64,
    /// Signal number for handler. C: `sf_signum`
    pub sf_signum: u32,
    /// Code argument (0). C: `sf_code`
    pub sf_code: u32,
    /// Pointer to sigcontext. C: `sf_scp`
    pub sf_scp: u64,
    /// Saved frame pointer. C: `sf_fp`
    pub sf_fp: u64,
    /// Actual return address. C: `sf_ra`
    pub sf_ra: u64,
    /// Minix scp copy. C: `sf_scpcopy`
    pub sf_scpcopy: u64,
    /// The sigcontext. C: `sf_sc`
    pub sf_sc: X86_64SigContext,
}

// ── mcontext struct ──

/// x86-64 mcontext_t (C ABI compatible).
///
/// # Layout
///
/// Follows the NetBSD/minix mcontext_t layout adapted for 64-bit:
/// - gregs: 21 × u64 general registers (168 bytes)
/// - fpregs: 640-byte FPU state area (FXSAVE 512 + padding)
/// - tlsbase: thread-local storage base (8 bytes)
/// - mc_magic: integrity magic (4 bytes)
/// - mc_flags: context flags (4 bytes)
///
/// Total: 824 bytes.
///
/// C: `mcontext_t` — sys/arch/i386/include/mcontext.h (32-bit reference);
/// minix-rs defines its own 64-bit layout.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct X86_64Mcontext {
    /// General register set. C: `__gregset_t` (21 registers).
    pub gregs: [u64; 21],
    /// FPU register state. C: `__fpregset_t` (512-byte FXSAVE + padding).
    pub fpregs: [u8; 640],
    /// Thread-local storage base. C: `_mc_tlsbase`.
    pub tlsbase: u64,
    /// Magic value for integrity check. C: `mc_magic` (MCF_MAGIC = 0xc0ffee).
    pub mc_magic: i32,
    /// Context flags. C: `mc_flags` (_MC_FPU_SAVED = 0x001).
    pub mc_flags: i32,
}

impl Default for X86_64Mcontext {
    fn default() -> Self {
        Self {
            gregs: [0u64; 21],
            fpregs: [0u8; 640],
            tlsbase: 0,
            mc_magic: 0xc0ffee,
            mc_flags: 0,
        }
    }
}

impl core::fmt::Debug for X86_64Mcontext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("X86_64Mcontext")
            .field("mc_magic", &self.mc_magic)
            .field("mc_flags", &self.mc_flags)
            .finish()
    }
}

// ── SignalContext implementation ──

/// x86-64 `SignalContext` implementation marker.
pub struct X86_64SignalContext;

impl SignalContext for X86_64SignalContext {
    type CpuContext = X86_64CpuContext;
    type SigContext = X86_64SigContext;
    type SigFrame = X86_64SigFrame;
    type Mcontext = X86_64Mcontext;

    fn build_sigcontext(ctx: &Self::CpuContext, info: &mut SignalInfo) -> Self::SigContext {
        // C: do_sigsend.c:46 — smsg.sm_stkptr = arch_get_sp(rp)
        info.stkptr = ctx.rsp;

        // C: do_sigsend.c:50 — memset(&fr, 0, sizeof(fr))
        // (Default::default() gives zero-init)
        let mut sctx = X86_64SigContext::default();

        // C: do_sigsend.c:54-69 — fill segment + GP registers
        sctx.sc_gs = ctx.gs;
        sctx.sc_fs = ctx.fs;
        sctx.sc_es = ctx.es;
        sctx.sc_ds = ctx.ds;
        sctx.sc_rdi = ctx.gp_regs[GP_RDI];
        sctx.sc_rsi = ctx.gp_regs[GP_RSI];
        sctx.sc_rbp = ctx.gp_regs[GP_RBP];
        sctx.sc_rbx = ctx.rbx;
        sctx.sc_rdx = ctx.gp_regs[GP_RDX];
        sctx.sc_rcx = ctx.gp_regs[GP_RCX];
        sctx.sc_rax = ctx.gp_regs[GP_RAX];
        sctx.sc_rip = ctx.rip;
        sctx.sc_cs = ctx.cs;
        sctx.sc_rflags = ctx.psw;
        sctx.sc_rsp = ctx.rsp;
        sctx.sc_ss = ctx.ss;

        // 64-bit extension registers
        sctx.sc_r8  = ctx.gp_regs[GP_R8];
        sctx.sc_r9  = ctx.gp_regs[GP_R9];
        sctx.sc_r10 = ctx.gp_regs[GP_R10];
        sctx.sc_r11 = ctx.gp_regs[GP_R11];
        sctx.sc_r12 = ctx.gp_regs[GP_R12];
        sctx.sc_r13 = ctx.gp_regs[GP_R13];
        sctx.sc_r14 = ctx.gp_regs[GP_R14];
        sctx.sc_r15 = ctx.gp_regs[GP_R15];

        // C: do_sigsend.c:113-115 — sc_mask, sc_flags, sc_magic
        sctx.sc_mask = info.mask;
        sctx.sc_flags = 0; // MF_FPU_INITIALIZED handled by caller
        sctx.sc_magic = SC_MAGIC;

        // C: do_sigsend.c:77 — trap_style from p_seg.p_kern_trap_style
        // Default to KTS_NONE; caller sets if needed.
        sctx.trap_style = KTS_NONE;

        sctx
    }

    fn build_sigframe(
        ctx: &Self::CpuContext,
        sctx: &Self::SigContext,
        info: &SignalInfo,
        frame_addr: u64,
    ) -> Self::SigFrame {
        // C: do_sigsend.c:51 — fr.sf_scp = &frp->sf_sc
        // frame_addr is the address of the sigframe on the user stack.
        // sf_sc is at offset `offsetof(sf_sc)` within the frame.
        let sf_sc_addr = frame_addr + core::mem::offset_of!(X86_64SigFrame, sf_sc) as u64;

        // C: do_sigsend.c:71 — sf_signum = smsg.sm_signo
        // C: do_sigsend.c:72 — new_fp = (reg_t) &frp->sf_fp
        let sf_fp_addr = frame_addr + core::mem::offset_of!(X86_64SigFrame, sf_fp) as u64;

        // C: do_sigsend.c:73 — fr.sf_scpcopy = fr.sf_scp
        // C: do_sigsend.c:74 — fr.sf_ra_sigreturn = smsg.sm_sigreturn
        // C: do_sigsend.c:75 — fr.sf_ra = rp->p_reg.pc
        X86_64SigFrame {
            sf_ra_sigreturn: info.sigreturn,
            sf_signum: info.signo,
            sf_code: 0,
            sf_scp: sf_sc_addr,
            sf_fp: sf_fp_addr, // C: new_fp = &frp->sf_fp
            sf_ra: ctx.rip,
            sf_scpcopy: sf_sc_addr,
            sf_sc: *sctx,
        }
    }

    fn setup_handler_entry(ctx: &mut Self::CpuContext, info: &SignalInfo, frame_addr: u64) {
        // C: do_sigsend.c:134 — rp->p_reg.sp = (reg_t) frp
        // frp = (struct sigframe_sigcontext *) smsg.sm_stkptr - 1
        // frame_addr is already the computed address of the sigframe.
        ctx.rsp = frame_addr;

        // C: do_sigsend.c:135 — rp->p_reg.pc = (reg_t) smsg.sm_sighandler
        ctx.rip = info.sighandler;

        // C: do_sigsend.c:138 — rp->p_reg.fp = new_fp
        // new_fp = &frp->sf_fp (computed in build_sigframe)
        let sf_fp_addr = frame_addr + core::mem::offset_of!(X86_64SigFrame, sf_fp) as u64;
        ctx.gp_regs[GP_RBP] = sf_fp_addr;

        // C: do_sigsend.c:154 — rp->p_misc_flags &= ~MF_FPU_INITIALIZED
        // (kernel layer handles p_misc_flags; arch layer doesn't track it)
    }

    fn restore_sigcontext(ctx: &mut Self::CpuContext, sctx: &Self::SigContext) {
        // C: do_sigreturn.c:40-41 — merge RFLAGS user bits
        // sc.sc_eflags = (sc.sc_eflags & X86_FLAGS_USER) |
        //                (rp->p_reg.psw & ~X86_FLAGS_USER)
        let merged_rflags = (sctx.sc_rflags & X86_FLAGS_USER) |
                            (ctx.psw & !X86_FLAGS_USER);

        // C: do_sigreturn.c:48-58 — restore GP registers
        // (segment registers are NOT restored — C comment at line 45)
        ctx.gp_regs[GP_RDI] = sctx.sc_rdi;
        ctx.gp_regs[GP_RSI] = sctx.sc_rsi;
        ctx.gp_regs[GP_RBP] = sctx.sc_rbp;
        ctx.rbx = sctx.sc_rbx;
        ctx.gp_regs[GP_RDX] = sctx.sc_rdx;
        ctx.gp_regs[GP_RCX] = sctx.sc_rcx;
        ctx.gp_regs[GP_RAX] = sctx.sc_rax;
        ctx.rip = sctx.sc_rip;
        ctx.psw = merged_rflags;
        ctx.rsp = sctx.sc_rsp;

        // 64-bit extension registers
        ctx.gp_regs[GP_R8]  = sctx.sc_r8;
        ctx.gp_regs[GP_R9]  = sctx.sc_r9;
        ctx.gp_regs[GP_R10] = sctx.sc_r10;
        ctx.gp_regs[GP_R11] = sctx.sc_r11;
        ctx.gp_regs[GP_R12] = sctx.sc_r12;
        ctx.gp_regs[GP_R13] = sctx.sc_r13;
        ctx.gp_regs[GP_R14] = sctx.sc_r14;
        ctx.gp_regs[GP_R15] = sctx.sc_r15;
    }

    fn arch_setcontext(ctx: &mut Self::CpuContext, trap_style: i32) {
        // C: do_sigreturn.c:81 — arch_proc_setcontext(rp, &rp->p_reg, 1, sc.trap_style)
        //
        // On x86-64, arch_proc_setcontext updates p_kern_trap_style and
        // potentially switches return path (iret vs sysret). The arch
        // layer stores trap_style in a CpuContext-adjacent field; for
        // now we store it in gp_regs[GP_R15] as a temporary (the trap
        // entry path will overwrite R15 on next entry anyway).
        //
        // DEFERRED: proper trap_style field in CpuContext.
        // The kernel layer's p_misc_flags / p_kern_trap_style tracking
        // is not yet wired to CpuContext.
        let _ = (ctx, trap_style);
    }

    fn get_sp(ctx: &Self::CpuContext) -> u64 {
        // C: arch_get_sp(rp) — returns rp->p_reg.sp
        ctx.rsp
    }

    fn sigframe_size() -> usize {
        core::mem::size_of::<X86_64SigFrame>()
    }

    fn check_magic(sctx: &Self::SigContext) -> bool {
        // C: do_sigreturn.c:83 — if(sc.sc_magic != SC_MAGIC)
        sctx.sc_magic == SC_MAGIC
    }

    fn get_trap_style(sctx: &Self::SigContext) -> i32 {
        // C: do_sigreturn.c:81 — sc.trap_style
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
    use crate::x86_64::boot::X86_64CpuContextArch;

    #[test]
    fn test_sigcontext_size_nonzero() {
        assert!(core::mem::size_of::<X86_64SigContext>() > 0);
    }

    #[test]
    fn test_sigframe_size_nonzero() {
        assert!(core::mem::size_of::<X86_64SigFrame>() > 0);
    }

    #[test]
    fn test_build_sigcontext_fills_from_ctx() {
        let ctx = X86_64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::loaded(
                minix_types::VirBytes(0x1000),
                minix_types::VirBytes(0x7fff_0000),
                minix_types::VirBytes(0x7ffe_ffe0),
            ),
        );
        let mut info = SignalInfo {
            sighandler: 0x4000,
            mask: 0xFF,
            signo: 6,
            sigreturn: 0x5000,
            stkptr: 0,
        };
        let sctx = X86_64SignalContext::build_sigcontext(&ctx, &mut info);

        assert_eq!(sctx.sc_rip, 0x1000);
        assert_eq!(sctx.sc_rsp, 0x7fff_0000);
        assert_eq!(sctx.sc_rbx, 0x7ffe_ffe0);
        assert_eq!(sctx.sc_rflags, 0x0202); // INIT_PSW
        assert_eq!(sctx.sc_cs, 0x1B);
        assert_eq!(sctx.sc_mask, 0xFF);
        assert_eq!(sctx.sc_magic, SC_MAGIC);
        // stkptr should be updated to ctx.rsp
        assert_eq!(info.stkptr, 0x7fff_0000);
    }

    #[test]
    fn test_build_sigframe_computes_pointers() {
        let ctx = X86_64CpuContext::default();
        let mut info = SignalInfo {
            sighandler: 0x4000,
            mask: 0,
            signo: 6,
            sigreturn: 0x5000,
            stkptr: 0,
        };
        let sctx = X86_64SignalContext::build_sigcontext(&ctx, &mut info);
        let frame = X86_64SignalContext::build_sigframe(&ctx, &sctx, &info, 0x8000);

        assert_eq!(frame.sf_ra_sigreturn, 0x5000);
        assert_eq!(frame.sf_signum, 6);
        // sf_scp should point to sf_sc within the frame
        let expected_scp = 0x8000 + core::mem::offset_of!(X86_64SigFrame, sf_sc) as u64;
        assert_eq!(frame.sf_scp, expected_scp);
        assert_eq!(frame.sf_scpcopy, expected_scp);
    }

    #[test]
    fn test_setup_handler_entry_sets_sp_pc_fp() {
        let mut ctx = X86_64CpuContext::default();
        let info = SignalInfo {
            sighandler: 0x4000,
            mask: 0,
            signo: 6,
            sigreturn: 0x5000,
            stkptr: 0,
        };
        X86_64SignalContext::setup_handler_entry(&mut ctx, &info, 0x8000);

        assert_eq!(ctx.rsp, 0x8000);
        assert_eq!(ctx.rip, 0x4000);
        // FP should be &frame.sf_fp
        let expected_fp = 0x8000 + core::mem::offset_of!(X86_64SigFrame, sf_fp) as u64;
        assert_eq!(ctx.gp_regs[GP_RBP], expected_fp);
    }

    #[test]
    fn test_restore_sigcontext_merges_rflags() {
        let mut ctx = X86_64CpuContext::default();
        // Set system flags (IF=1, bit 9)
        ctx.psw = 0x0200;

        let sctx = X86_64SigContext {
            sc_rflags: 0x0001, // PSL_C (user flag)
            sc_rip: 0xDEAD,
            sc_rsp: 0xBEEF,
            sc_rax: 0x1234,
            ..Default::default()
        };
        X86_64SignalContext::restore_sigcontext(&mut ctx, &sctx);

        // Merged: user bits from sctx + system bits from ctx
        assert_eq!(ctx.psw, 0x0201); // IF | CF
        assert_eq!(ctx.rip, 0xDEAD);
        assert_eq!(ctx.rsp, 0xBEEF);
        assert_eq!(ctx.gp_regs[GP_RAX], 0x1234);
    }

    #[test]
    fn test_check_magic() {
        let sctx_ok = X86_64SigContext { sc_magic: SC_MAGIC, ..Default::default() };
        let sctx_bad = X86_64SigContext { sc_magic: 0, ..Default::default() };
        assert!(X86_64SignalContext::check_magic(&sctx_ok));
        assert!(!X86_64SignalContext::check_magic(&sctx_bad));
    }

    #[test]
    fn test_get_sp_returns_rsp() {
        let mut ctx = X86_64CpuContext::default();
        ctx.rsp = 0x7fff_0000;
        assert_eq!(X86_64SignalContext::get_sp(&ctx), 0x7fff_0000);
    }

    #[test]
    fn test_sigframe_size_matches_struct() {
        assert_eq!(
            X86_64SignalContext::sigframe_size(),
            core::mem::size_of::<X86_64SigFrame>()
        );
    }

    #[test]
    fn test_roundtrip_build_then_restore() {
        let mut ctx = X86_64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::loaded(
                minix_types::VirBytes(0x1000),
                minix_types::VirBytes(0x7fff_0000),
                minix_types::VirBytes(0x7ffe_ffe0),
            ),
        );
        // Simulate trap entry filling GP regs
        ctx.gp_regs[GP_RAX] = 0xAA;
        ctx.gp_regs[GP_RDX] = 0xDD;

        let mut info = SignalInfo {
            sighandler: 0x4000,
            mask: 0,
            signo: 9,
            sigreturn: 0x5000,
            stkptr: 0,
        };
        let sctx = X86_64SignalContext::build_sigcontext(&ctx, &mut info);

        // Clear ctx to verify restore
        let mut ctx2 = X86_64CpuContext::default();
        X86_64SignalContext::restore_sigcontext(&mut ctx2, &sctx);

        assert_eq!(ctx2.gp_regs[GP_RAX], 0xAA);
        assert_eq!(ctx2.gp_regs[GP_RDX], 0xDD);
        assert_eq!(ctx2.rip, 0x1000);
        assert_eq!(ctx2.rsp, 0x7fff_0000);
        assert_eq!(ctx2.rbx, 0x7ffe_ffe0);
    }
}
