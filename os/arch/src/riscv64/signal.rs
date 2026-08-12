//! riscv64 `SignalContext` implementation.
//!
//! Implements the `SignalContext` trait for RISC-V 64-bit, providing
//! sigcontext construction and restoration for POSIX-style signal delivery.
//!
//! # C Source Mapping
//!
//! Minix3 C source does NOT implement riscv64 signal handling (the
//! `#if defined(__arm__)` block in `do_sigsend.c` is the last arch
//! branch). This implementation follows the RISC-V ELF psABI and
//! Linux signal handling conventions, adapted for minix-rs.
//!
//! # RISC-V signal handling design
//!
//! - `sepc` = PC (return address after `sret`)
//! - `sstatus` = processor state
//! - `sp` = stack pointer (x2)
//! - `ra` (x1) = return address register (used for sigreturn trampoline)
//! - `a0` (x10) = first argument (signal number)
//! - `a1` (x11) = second argument (sigcontext pointer)
//!
//! Signal handler is called with:
//! - `a0` = signal number
//! - `a1` = pointer to sigcontext
//! - `ra` = sigreturn trampoline address
//! - `sp` = adjusted to sigframe

use crate::signal_context::{SignalContext, SignalInfo};
use crate::arch::signal_context::SC_MAGIC;

use super::boot::Riscv64CpuContext;

// ── GP register indices in `gp_regs` array ──
//
// gp_regs layout (30 entries):
// [0]  = X1  (ra)
// [1]  = X3  (gp)
// [2]  = X4  (tp)
// [3]  = X5  (t0)
// [4]  = X6  (t1)
// [5]  = X7  (t2)
// [6]  = X8  (s0 / fp)
// [7]  = X9  (s1)
// [8]  = X11 (a1)
// [9]  = X12 (a2)
// [10] = X13 (a3)
// [11] = X14 (a4)
// [12] = X15 (a5)
// [13] = X16 (a6)
// [14] = X17 (a7)
// [15] = X18 (s2)
// [16] = X19 (s3)
// [17] = X20 (s4)
// [18] = X21 (s5)
// [19] = X22 (s6)
// [20] = X23 (s7)
// [21] = X24 (s8)
// [22] = X25 (s9)
// [23] = X26 (s10)
// [24] = X27 (s11)
// [25] = X28 (t3)
// [26] = X29 (t4)
// [27] = X30 (t5)
// [28] = X31 (t6)
// [29] = reserved (padding for alignment)

/// Index of X1 (ra) in `gp_regs`.
pub(super) const GP_RA:  usize = 0;
/// Index of X3 (gp) in `gp_regs`.
pub(super) const GP_GP:  usize = 1;
/// Index of X4 (tp) in `gp_regs`.
pub(super) const GP_TP:  usize = 2;
/// Index of X5 (t0) in `gp_regs`.
pub(super) const GP_T0:  usize = 3;
/// Index of X6 (t1) in `gp_regs`.
pub(super) const GP_T1:  usize = 4;
/// Index of X7 (t2) in `gp_regs`.
pub(super) const GP_T2:  usize = 5;
/// Index of X8 (s0/fp) in `gp_regs`.
pub(super) const GP_S0:  usize = 6;
/// Index of X9 (s1) in `gp_regs`.
pub(super) const GP_S1:  usize = 7;
/// Index of X11 (a1) in `gp_regs`.
pub(super) const GP_A1:  usize = 8;
/// Index of X12 (a2) in `gp_regs`.
pub(super) const GP_A2:  usize = 9;
/// Index of X13 (a3) in `gp_regs`.
pub(super) const GP_A3:  usize = 10;
/// Index of X14 (a4) in `gp_regs`.
pub(super) const GP_A4:  usize = 11;
/// Index of X15 (a5) in `gp_regs`.
pub(super) const GP_A5:  usize = 12;
/// Index of X16 (a6) in `gp_regs`.
pub(super) const GP_A6:  usize = 13;
/// Index of X17 (a7) in `gp_regs`.
pub(super) const GP_A7:  usize = 14;
/// Index of X18 (s2) in `gp_regs`.
pub(super) const GP_S2:  usize = 15;
/// Index of X19 (s3) in `gp_regs`.
pub(super) const GP_S3:  usize = 16;
/// Index of X20 (s4) in `gp_regs`.
pub(super) const GP_S4:  usize = 17;
/// Index of X21 (s5) in `gp_regs`.
pub(super) const GP_S5:  usize = 18;
/// Index of X22 (s6) in `gp_regs`.
pub(super) const GP_S6:  usize = 19;
/// Index of X23 (s7) in `gp_regs`.
pub(super) const GP_S7:  usize = 20;
/// Index of X24 (s8) in `gp_regs`.
pub(super) const GP_S8:  usize = 21;
/// Index of X25 (s9) in `gp_regs`.
pub(super) const GP_S9:  usize = 22;
/// Index of X26 (s10) in `gp_regs`.
pub(super) const GP_S10: usize = 23;
/// Index of X27 (s11) in `gp_regs`.
pub(super) const GP_S11: usize = 24;
/// Index of X28 (t3) in `gp_regs`.
pub(super) const GP_T3:  usize = 25;
/// Index of X29 (t4) in `gp_regs`.
pub(super) const GP_T4:  usize = 26;
/// Index of X30 (t5) in `gp_regs`.
pub(super) const GP_T5:  usize = 27;
/// Index of X31 (t6) in `gp_regs`.
pub(super) const GP_T6:  usize = 28;

// ── sigcontext struct ──

/// riscv64 saved register state for signal delivery.
///
/// # Design
///
/// Minix3 C source does not implement riscv64 signal handling.
/// This struct follows the RISC-V ELF psABI `mcontext_t` layout,
/// adapted for minix-rs.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Riscv64SigContext {
    /// sstatus. C: N/A (riscv64 not in C source)
    pub sc_sstatus: u64,
    /// sepc (PC). C: N/A
    pub sc_sepc: u64,
    /// X0 (always 0; included for completeness).
    pub sc_x0: u64,
    /// X1 (ra).
    pub sc_ra: u64,
    /// X2 (sp).
    pub sc_sp: u64,
    /// X3 (gp).
    pub sc_gp: u64,
    /// X4 (tp).
    pub sc_tp: u64,
    /// X5 (t0).
    pub sc_t0: u64,
    /// X6 (t1).
    pub sc_t1: u64,
    /// X7 (t2).
    pub sc_t2: u64,
    /// X8 (s0/fp).
    pub sc_s0: u64,
    /// X9 (s1).
    pub sc_s1: u64,
    /// X10 (a0).
    pub sc_a0: u64,
    /// X11 (a1).
    pub sc_a1: u64,
    /// X12 (a2).
    pub sc_a2: u64,
    /// X13 (a3).
    pub sc_a3: u64,
    /// X14 (a4).
    pub sc_a4: u64,
    /// X15 (a5).
    pub sc_a5: u64,
    /// X16 (a6).
    pub sc_a6: u64,
    /// X17 (a7).
    pub sc_a7: u64,
    /// X18 (s2).
    pub sc_s2: u64,
    /// X19 (s3).
    pub sc_s3: u64,
    /// X20 (s4).
    pub sc_s4: u64,
    /// X21 (s5).
    pub sc_s5: u64,
    /// X22 (s6).
    pub sc_s6: u64,
    /// X23 (s7).
    pub sc_s7: u64,
    /// X24 (s8).
    pub sc_s8: u64,
    /// X25 (s9).
    pub sc_s9: u64,
    /// X26 (s10).
    pub sc_s10: u64,
    /// X27 (s11).
    pub sc_s11: u64,
    /// X28 (t3).
    pub sc_t3: u64,
    /// X29 (t4).
    pub sc_t4: u64,
    /// X30 (t5).
    pub sc_t5: u64,
    /// X31 (t6).
    pub sc_t6: u64,
    /// Signal mask.
    pub sc_mask: u64,
    /// Flags.
    pub sc_flags: i32,
    /// Trap style.
    pub trap_style: i32,
    /// Magic.
    pub sc_magic: i32,
}

// ── sigframe struct ──

/// riscv64 signal frame written to user stack.
///
/// # Design
///
/// Follows the same pattern as aarch64: a pointer to sigcontext
/// and the sigcontext itself. The sigreturn trampoline address
/// is stored in `ra` (set by `setup_handler_entry`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Riscv64SigFrame {
    /// Pointer to sigcontext (for handler's 2nd arg).
    pub sf_scp: u64,
    /// The sigcontext.
    pub sf_sc: Riscv64SigContext,
}

// ── mcontext struct ──

/// RISC-V 64 mcontext_t (C ABI compatible).
///
/// # Layout
/// - gregs: 32 × u64 general registers (X0-X31, 256 bytes)
/// - sepc: program counter (8 bytes)
/// - sstatus: processor status (8 bytes)
/// - fpregs: FPU state (32 × 8-byte F registers + FCSR, 264 bytes)
/// - mc_magic: 4 bytes
/// - mc_flags: 4 bytes
///
/// Total: 544 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Riscv64Mcontext {
    /// General register set (X0-X31).
    pub gregs: [u64; 32],
    /// Program counter (SEPC).
    pub sepc: u64,
    /// Processor status (SSTATUS).
    pub sstatus: u64,
    /// FPU register state (32 × 8-byte F registers + FCSR).
    pub fpregs: [u8; 264],
    /// Magic value for integrity check. C: `mc_magic` (MCF_MAGIC = 0xc0ffee).
    pub mc_magic: i32,
    /// Context flags. C: `mc_flags` (_MC_FPU_SAVED = 0x001).
    pub mc_flags: i32,
}

impl Default for Riscv64Mcontext {
    fn default() -> Self {
        Self {
            gregs: [0u64; 32],
            sepc: 0,
            sstatus: 0,
            fpregs: [0u8; 264],
            mc_magic: 0xc0ffee,
            mc_flags: 0,
        }
    }
}

impl core::fmt::Debug for Riscv64Mcontext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Riscv64Mcontext")
            .field("mc_magic", &self.mc_magic)
            .field("mc_flags", &self.mc_flags)
            .finish()
    }
}

// ── SignalContext implementation ──

/// riscv64 `SignalContext` implementation marker.
pub struct Riscv64SignalContext;

impl SignalContext for Riscv64SignalContext {
    type CpuContext = Riscv64CpuContext;
    type SigContext = Riscv64SigContext;
    type SigFrame = Riscv64SigFrame;
    type Mcontext = Riscv64Mcontext;

    fn build_sigcontext(ctx: &Self::CpuContext, info: &mut SignalInfo) -> Self::SigContext {
        info.stkptr = ctx.sp;

        let mut sctx = Riscv64SigContext::default();

        sctx.sc_sstatus = ctx.sstatus;
        sctx.sc_sepc = ctx.sepc;
        sctx.sc_x0 = 0; // hardwired zero
        sctx.sc_ra = ctx.gp_regs[GP_RA];
        sctx.sc_sp = ctx.sp;
        sctx.sc_gp = ctx.gp_regs[GP_GP];
        sctx.sc_tp = ctx.gp_regs[GP_TP];
        sctx.sc_t0 = ctx.gp_regs[GP_T0];
        sctx.sc_t1 = ctx.gp_regs[GP_T1];
        sctx.sc_t2 = ctx.gp_regs[GP_T2];
        sctx.sc_s0 = ctx.gp_regs[GP_S0];
        sctx.sc_s1 = ctx.gp_regs[GP_S1];
        sctx.sc_a0 = ctx.a0;
        sctx.sc_a1 = ctx.gp_regs[GP_A1];
        sctx.sc_a2 = ctx.gp_regs[GP_A2];
        sctx.sc_a3 = ctx.gp_regs[GP_A3];
        sctx.sc_a4 = ctx.gp_regs[GP_A4];
        sctx.sc_a5 = ctx.gp_regs[GP_A5];
        sctx.sc_a6 = ctx.gp_regs[GP_A6];
        sctx.sc_a7 = ctx.gp_regs[GP_A7];
        sctx.sc_s2 = ctx.gp_regs[GP_S2];
        sctx.sc_s3 = ctx.gp_regs[GP_S3];
        sctx.sc_s4 = ctx.gp_regs[GP_S4];
        sctx.sc_s5 = ctx.gp_regs[GP_S5];
        sctx.sc_s6 = ctx.gp_regs[GP_S6];
        sctx.sc_s7 = ctx.gp_regs[GP_S7];
        sctx.sc_s8 = ctx.gp_regs[GP_S8];
        sctx.sc_s9 = ctx.gp_regs[GP_S9];
        sctx.sc_s10 = ctx.gp_regs[GP_S10];
        sctx.sc_s11 = ctx.gp_regs[GP_S11];
        sctx.sc_t3 = ctx.gp_regs[GP_T3];
        sctx.sc_t4 = ctx.gp_regs[GP_T4];
        sctx.sc_t5 = ctx.gp_regs[GP_T5];
        sctx.sc_t6 = ctx.gp_regs[GP_T6];

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
        let sf_sc_addr = frame_addr + core::mem::offset_of!(Riscv64SigFrame, sf_sc) as u64;

        Riscv64SigFrame {
            sf_scp: sf_sc_addr,
            sf_sc: *sctx,
        }
    }

    fn setup_handler_entry(ctx: &mut Self::CpuContext, info: &SignalInfo, frame_addr: u64) {
        // Set SP to sigframe address
        ctx.sp = frame_addr;
        // Set PC to signal handler
        ctx.sepc = info.sighandler;
        // Set RA to sigreturn trampoline
        ctx.gp_regs[GP_RA] = info.sigreturn;
        // Set A0 = signal number
        ctx.a0 = info.signo as u64;
        // Set A1 = pointer to sigcontext
        let sf_sc_addr = frame_addr + core::mem::offset_of!(Riscv64SigFrame, sf_sc) as u64;
        ctx.gp_regs[GP_A1] = sf_sc_addr;
    }

    fn restore_sigcontext(ctx: &mut Self::CpuContext, sctx: &Self::SigContext) {
        ctx.sstatus = sctx.sc_sstatus;
        ctx.sepc = sctx.sc_sepc;
        ctx.gp_regs[GP_RA] = sctx.sc_ra;
        ctx.sp = sctx.sc_sp;
        ctx.gp_regs[GP_GP] = sctx.sc_gp;
        ctx.gp_regs[GP_TP] = sctx.sc_tp;
        ctx.gp_regs[GP_T0] = sctx.sc_t0;
        ctx.gp_regs[GP_T1] = sctx.sc_t1;
        ctx.gp_regs[GP_T2] = sctx.sc_t2;
        ctx.gp_regs[GP_S0] = sctx.sc_s0;
        ctx.gp_regs[GP_S1] = sctx.sc_s1;
        ctx.a0 = sctx.sc_a0;
        ctx.gp_regs[GP_A1] = sctx.sc_a1;
        ctx.gp_regs[GP_A2] = sctx.sc_a2;
        ctx.gp_regs[GP_A3] = sctx.sc_a3;
        ctx.gp_regs[GP_A4] = sctx.sc_a4;
        ctx.gp_regs[GP_A5] = sctx.sc_a5;
        ctx.gp_regs[GP_A6] = sctx.sc_a6;
        ctx.gp_regs[GP_A7] = sctx.sc_a7;
        ctx.gp_regs[GP_S2] = sctx.sc_s2;
        ctx.gp_regs[GP_S3] = sctx.sc_s3;
        ctx.gp_regs[GP_S4] = sctx.sc_s4;
        ctx.gp_regs[GP_S5] = sctx.sc_s5;
        ctx.gp_regs[GP_S6] = sctx.sc_s6;
        ctx.gp_regs[GP_S7] = sctx.sc_s7;
        ctx.gp_regs[GP_S8] = sctx.sc_s8;
        ctx.gp_regs[GP_S9] = sctx.sc_s9;
        ctx.gp_regs[GP_S10] = sctx.sc_s10;
        ctx.gp_regs[GP_S11] = sctx.sc_s11;
        ctx.gp_regs[GP_T3] = sctx.sc_t3;
        ctx.gp_regs[GP_T4] = sctx.sc_t4;
        ctx.gp_regs[GP_T5] = sctx.sc_t5;
        ctx.gp_regs[GP_T6] = sctx.sc_t6;
    }

    fn arch_setcontext(_ctx: &mut Self::CpuContext, _trap_style: i32) {
        // riscv64: no-op (registers restored directly into CpuContext)
    }

    fn get_sp(ctx: &Self::CpuContext) -> u64 {
        ctx.sp
    }

    fn sigframe_size() -> usize {
        core::mem::size_of::<Riscv64SigFrame>()
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
        assert!(core::mem::size_of::<Riscv64SigContext>() > 0);
    }

    #[test]
    fn test_build_sigcontext_fills_from_ctx() {
        let ctx = Riscv64CpuContextArch::build_cpu_context(
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
        let sctx = Riscv64SignalContext::build_sigcontext(&ctx, &mut info);

        assert_eq!(sctx.sc_sepc, 0x1000);
        assert_eq!(sctx.sc_sp, 0x4000_0000);
        assert_eq!(sctx.sc_a0, 0x3fff_ffe0);
        assert_eq!(sctx.sc_mask, 0xFF);
        assert_eq!(sctx.sc_magic, SC_MAGIC);
        assert_eq!(info.stkptr, 0x4000_0000);
    }

    #[test]
    fn test_setup_handler_entry_riscv64() {
        let mut ctx = Riscv64CpuContext::default();
        let info = SignalInfo {
            sighandler: 0x4000,
            mask: 0,
            signo: 6,
            sigreturn: 0x5000,
            stkptr: 0,
        };
        Riscv64SignalContext::setup_handler_entry(&mut ctx, &info, 0x8000);

        assert_eq!(ctx.sp, 0x8000);
        assert_eq!(ctx.sepc, 0x4000);
        assert_eq!(ctx.gp_regs[GP_RA], 0x5000);
        assert_eq!(ctx.a0, 6);
        let expected_scp = 0x8000 + core::mem::offset_of!(Riscv64SigFrame, sf_sc) as u64;
        assert_eq!(ctx.gp_regs[GP_A1], expected_scp);
    }

    #[test]
    fn test_restore_sigcontext_riscv64() {
        let mut ctx = Riscv64CpuContext::default();
        let sctx = Riscv64SigContext {
            sc_sstatus: 0x20,
            sc_sepc: 0x1000,
            sc_sp: 0x7fff_0000,
            sc_ra: 0x5000,
            sc_a0: 0xAA,
            ..Default::default()
        };
        Riscv64SignalContext::restore_sigcontext(&mut ctx, &sctx);

        assert_eq!(ctx.sstatus, 0x20);
        assert_eq!(ctx.sepc, 0x1000);
        assert_eq!(ctx.sp, 0x7fff_0000);
        assert_eq!(ctx.gp_regs[GP_RA], 0x5000);
        assert_eq!(ctx.a0, 0xAA);
    }

    #[test]
    fn test_check_magic_riscv64() {
        let sctx_ok = Riscv64SigContext { sc_magic: SC_MAGIC, ..Default::default() };
        let sctx_bad = Riscv64SigContext { sc_magic: 0, ..Default::default() };
        assert!(Riscv64SignalContext::check_magic(&sctx_ok));
        assert!(!Riscv64SignalContext::check_magic(&sctx_bad));
    }

    #[test]
    fn test_roundtrip_riscv64() {
        let mut ctx = Riscv64CpuContextArch::build_cpu_context(
            ProcKind::Vm,
            8,
            EntrySpec::loaded(
                minix_types::VirBytes(0x1000),
                minix_types::VirBytes(0x4000_0000),
                minix_types::VirBytes(0x3fff_ffe0),
            ),
        );
        ctx.gp_regs[GP_S5] = 0x55;

        let mut info = SignalInfo {
            sighandler: 0,
            mask: 0,
            signo: 9,
            sigreturn: 0,
            stkptr: 0,
        };
        let sctx = Riscv64SignalContext::build_sigcontext(&ctx, &mut info);

        let mut ctx2 = Riscv64CpuContext::default();
        Riscv64SignalContext::restore_sigcontext(&mut ctx2, &sctx);

        assert_eq!(ctx2.gp_regs[GP_S5], 0x55);
        assert_eq!(ctx2.sepc, 0x1000);
        assert_eq!(ctx2.sp, 0x4000_0000);
    }
}
