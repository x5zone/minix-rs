//! riscv64 trap return implementation
//!
//! Implements [`TrapReturnArch`] for riscv64: loads sepc / sstatus from the
//! exception frame, restores the integer registers from the process's
//! register file, and executes `sret` back to U-mode. Return-path twin of
//! `riscv64::trap_entry` (stvec entry).
//!
//! # What lives where
//!
//! - **Frame** (`Riscv64ExceptionFrame`) — the mode-switch pair sstatus /
//!   sepc, rebuilt by `apply_to_trap_frame` from the context on every
//!   dispatch.
//! - **RegisterFile** (`Riscv64CpuContext`) — the integer registers: x2
//!   (sp) and x10 (a0, the ps_strings carrier) as named fields, everything
//!   else in `gp_regs` (indexed by the `GP_*` ABI constants: RA, GP, TP,
//!   T0–T2, S0–S1, A1–A7, S2–S11, T3–T6; X0 is hardwired zero and has no
//!   slot).
//!
//! # Register liveness in the asm
//!
//! The register-file pointer parks in **t6 (x31)** — the one target whose
//! `gp_regs` slot (GP_T6) is loaded last, self-referentially. t0 (x5)
//! serves as the sepc/sstatus scratch before its own slot loads; the named
//! fields sp/a0 load while the struct-relative pointer is still intact,
//! then the pointer rebases onto `gp_regs` for the plain-stride descent.
//!
//! # Interrupt guarantee
//!
//! Setting `SPIE` here is the riscv64 analogue of the x86-64 `IF_MASK` OR
//! (C: `arch_finish_switch_to_user` — arch_system.c:512): `sret` re-enables
//! interrupts in U-mode iff `SPIE` is set (with `SPP` clear for a U-mode
//! return). This impl clears SPP and re-sets SPIE while preparing sstatus.
//!
//! Before that, the leg opens by CLEARING `sstatus.SIE` and keeps it clear
//! until `sret` re-establishes the enable from `SPIE` — the riscv64 twin of
//! aarch64's first instruction `msr daifset, #0xf` (`arm64::trap_return`,
//! whose note records the symptom it prevents). The difference that makes
//! self-masking necessary here: i386 reloads RSP from TSS.sp0 on every
//! CPL3→0 entry, so an interrupt during a handoff cannot land on the
//! handoff's own stack, while riscv64 (like aarch64's EL1h model) runs on one
//! kernel stack chosen by software — and `stvec`/`sscratch` ARE that choice.
//!
//! Why the mask is load-bearing (NK4C 续-373/续-374, arm condition stated
//! exactly): the window is armed on chains that run in S-mode with `SIE` ON.
//! That happens after `idle_halt` enables SIE (`csrs sstatus, SIE; wfi` in
//! `riscv64::smp`) and the tick that wakes it returns to S-mode through the
//! kernel leg, whose `sret` copies `SPIE`(=1) back into `SIE` — from then on,
//! while the scheduler loop runs in S-mode, kernel code is interruptible. On
//! the other main chain — U-origin trap → user leg → park → resched → this
//! leg — `SIE` is 0 the whole way (entry cleared it and no S-mode `sret` has
//! re-set it), and the window there is empty. The mask covers both shapes
//! unconditionally, which is why it is placed here rather than argued per
//! chain.
//!
//! When armed, this leg installs the USER trap vector at `stvec` about 50
//! instructions before `sret`, so a tick landing in that stretch takes the
//! user leg while still in S-mode: that leg swaps `sp` with `sscratch` —
//! which this leg has just re-anchored to the kernel-stack TOP — and writes
//! its 34-slot frame into `[top-272, top)`, the band the scheduler's own live
//! frames occupy at this depth. The tick restores the registers it saved, but
//! the kernel locals and intermediates that were sitting in that band are
//! gone: the shape that lets a later cross-space copy read a bogus length or
//! destination out of the kernel's own stack.
//!
//! Masking here costs nothing in delivery semantics: a tick that fires while
//! masked stays pending (`sip.STIP` set by `stimecmp`) and is taken normally
//! once `sret` restores `SIE` from `SPIE` — nothing is lost, only deferred
//! past the handoff.
//!
//! The §续-368 `NK4C_CLI` experiment flag does NOT belong to this guarantee:
//! it only ever skipped the `csrs` that SETS `SPIE`, and hardware already
//! left `SPIE`=1 (entry stores the pre-trap enable; `sret` sets it to 1 per
//! the privileged spec) — so that arm suppressed nothing and its "interrupts
//! off" reading is void (CodeReview on 续-373; 续-374 now clears `SPIE`
//! explicitly before the conditional re-set, which is what makes the flag
//! mean what its name says).

use crate::arch::trap_return::TrapReturnArch;
use crate::riscv64::boot::Riscv64CpuContext;
use crate::riscv64::exception::Riscv64ExceptionFrame;

/// sstatus.SPP — previous privilege mode. Cleared so `sret` returns to
/// U-mode.
const SSTATUS_SPP: u64 = 1 << 8;

/// sstatus.SPIE — previous interrupt-enable. Set so `sret` re-enables
/// interrupts in U-mode.
const SSTATUS_SPIE: u64 = 1 << 5;

/// sstatus.SIE — the LIVE supervisor interrupt enable (bit 1, NOT bit 5,
/// which is the saved `SPIE` consumed by `sret`). Cleared on entry to this
/// leg so the stack/register-file handoff cannot be interrupted through the
/// user trap vector (`riscv64::trap_stub`'s user leg), whose frame would land
/// on the scheduler's live kernel frames. Same bit `smp::idle_halt` sets
/// directly for the same reason, read off in the opposite direction.
///
/// The bit values and the instruction order that makes this mask effective
/// are pinned by the HOST test `arch/tests/riscv64_return_leg_pin.rs` —
/// a `#[test]` in this module would never run, because `pub mod riscv64` is
/// `#[cfg(target_arch = "riscv64")]` (NK4C 续-370 登记的同一陷阱).
const SSTATUS_SIE: u64 = 1 << 1;

/// riscv64 implementation marker (no state; the mode switch is a pure
/// register/CSR operation).
pub struct Riscv64TrapReturn;

/// §续-368 实验丁旗标（riscv64 真机诊断，默认 0）：1 = 本 hart 的下一次
/// 及后续用户返回**跳过 SPIE 置位**（sret 后 U 态中断关）。由内核
/// `dispatch_diagctl` 的魔术串置/清；`set_active_root_tracked`（每次地址
/// 空间切换）自动清零——粘滞窗不越过任何调度点，测试进程 abort 也安全。
pub static NK4C_CLI: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

impl TrapReturnArch for Riscv64TrapReturn {
    type Frame = Riscv64ExceptionFrame;
    type RegisterFile = Riscv64CpuContext;

    unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> ! {
        // SAFETY (full contract in TrapReturnArch::restore_to_user):
        // - `frame` was rebuilt from the picked process's saved state by
        //   `apply_to_trap_frame` immediately before this call.
        // - t0 (x5) and t6 (x31) are clobbered by design; both carry their
        //   user values again before `sret` — see the liveness notes in
        //   the module docs.
        // - The BKL has been released by the caller before this call.
        core::arch::asm!(
            // NK4C 续-373: mask supervisor interrupts for the whole leg, mirroring
            // aarch64's opening `msr daifset, #0xf`. `t1` is free here (the only
            // bound inputs are a0/a1/t0) and its user value is reloaded from the
            // context later. See the module docs for why the window is real.
            "li t1, {sie}",
            "csrc sstatus, t1",
            // Park the register-file pointer in t6 (its user slot is
            // gp_regs[GP_T6], loaded last as a self-referential load).
            "mv t6, a1",
            // sscratch re-anchor (续-75, the riscv form of the aarch64
            // §1.113 SP rebase before `eret`): after a park-and-resched
            // switch the register holds the PARKED process's user sp, not
            // a kernel-stack top — and this leg's own `sp` is about to
            // become a user sp, so nothing here else heals it. Load the
            // boot-recorded per-CPU base and restore the `load()`
            // invariant (`sscratch` = this CPU's kernel-stack top) BEFORE
            // anything reads a user value into a4. The symbol is the
            // `no_mangle` cell in `protection` (written by `init`/
            // `init_ap`, never reachable before the first user return).
            "la a4, KERNEL_TRAP_STACK_BASE",
            "ld a4, 0(a4)",
            "csrw sscratch, a4",
            // Point stvec at the USER leg: the CPU is about to run U-mode
            // code, so its next trap must enter through the swap leg
            // (`riscv64_user_trap_vector` — the kernel/user legs are
            // selected by this switch, not by an origin test in a scratch
            // register the interrupted context owns). t0 is explicitly
            // bound for the write and reloaded from the context right
            // after, before its user value matters.
            "csrw stvec, t0",
            // ── Mode-switch pair: sepc + sstatus ──
            "ld t0, {sepc_off}(a0)",
            "csrw sepc, t0",
            // SPP=0 (return to U-mode) + SPIE=1 (interrupts enabled after
            // sret). `csrci`/`csrsi` take 5-bit immediates, so SPP (0x100)
            // goes through t0.
            //
            // SPIE must be CLEARED first and only then re-set when the
            // experiment flag is off (NK4C 续-374, CodeReview on 续-373):
            // hardware already leaves SPIE=1 — trap entry stores the pre-trap
            // enable into it, and `sret` sets it to 1 per the privileged spec
            // — so merely SKIPPING the `csrs`, which is all §续-368's arm did,
            // is a no-op: `sret` would hand U-mode an enabled SIE anyway. The
            // 续-368 "interrupts off" reading is void (those three boots are
            // ordinary samples, not an arm); this branch is what makes the
            // flag actually suppress U-mode delivery.
            "li t0, {spp}",
            "csrc sstatus, t0",
            "li t0, {spie}",
            "csrc sstatus, t0",
            "la t1, {cli_sym}",
            "ld t1, 0(t1)",
            "bnez t1, 2f",
            "csrs sstatus, t0",
            "2:",
            // ── Named register-file fields (struct-relative offsets) ──
            "ld sp, {sp_off}(t6)",   // x2 — user stack pointer
            "ld a0, {a0_off}(t6)",   // x10 — ps_strings / IPC-status carrier
            // ── gp_regs descent ──
            // Rebase t6 onto gp_regs; strides are plain slot offsets.
            "add t6, t6, {gp_off}",
            "ld t5, 27*8(t6)",   // x30 ← GP_T5
            "ld t4, 26*8(t6)",   // x29 ← GP_T4
            "ld t3, 25*8(t6)",   // x28 ← GP_T3
            "ld s11, 24*8(t6)",  // x27
            "ld s10, 23*8(t6)",  // x26
            "ld s9,  22*8(t6)",  // x25
            "ld s8,  21*8(t6)",  // x24
            "ld s7,  20*8(t6)",  // x23
            "ld s6,  19*8(t6)",  // x22
            "ld s5,  18*8(t6)",  // x21
            "ld s4,  17*8(t6)",  // x20
            "ld s3,  16*8(t6)",  // x19
            "ld s2,  15*8(t6)",  // x18
            "ld a7,  14*8(t6)",  // x17
            "ld a6,  13*8(t6)",  // x16
            "ld a5,  12*8(t6)",  // x15
            "ld a4,  11*8(t6)",  // x14
            "ld a3,  10*8(t6)",  // x13
            "ld a2,  9*8(t6)",   // x12
            "ld a1,  8*8(t6)",   // x11 ← GP_A1
            "ld s1,  7*8(t6)",   // x9
            "ld s0,  6*8(t6)",   // x8
            "ld t2,  5*8(t6)",   // x7
            "ld t1,  4*8(t6)",   // x6
            "ld t0,  3*8(t6)",   // x5 — scratch's user value
            "ld gp,  1*8(t6)",   // x3 ← GP_GP
            "ld ra,  0*8(t6)",   // x1 ← GP_RA
            "ld tp,  2*8(t6)",   // x4 ← GP_TP
            "ld t6,  28*8(t6)",  // x31 ← GP_T6 — pointer's last use
            "sret",
            in("a0") frame as *const Riscv64ExceptionFrame,
            in("a1") regs as *const Riscv64CpuContext,
            in("t0") super::trap_stub::user_trap_vector_va().get(),
            sepc_off = const core::mem::offset_of!(Riscv64ExceptionFrame, sepc),
            sp_off = const core::mem::offset_of!(Riscv64CpuContext, sp),
            a0_off = const core::mem::offset_of!(Riscv64CpuContext, a0),
            gp_off = const core::mem::offset_of!(Riscv64CpuContext, gp_regs),
            spp = const SSTATUS_SPP,
            spie = const SSTATUS_SPIE,
            sie = const SSTATUS_SIE,
            cli_sym = sym NK4C_CLI,
            // `noreturn` documents the divergence and frees the clobber
            // set (every integer register is reloaded right before sret).
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sstatus_bits_are_spp_and_spie() {
        // NOTE (NK4C 续-370 登记的同一陷阱): 宿主 `cargo test` 不编译本模块
        // （`pub mod riscv64` 带 `#[cfg(target_arch = "riscv64")]`），所以这两条
        // 断言只在交叉编译测试时才会跑。权威断言在宿主集成测试
        // `arch/tests/riscv64_return_leg_pin.rs`（它额外覆盖 SSTATUS_SIE 与本腿
        // 的指令次序），本测试只当交叉编译路径的重复保险。
        // SPP = bit 8 (previous privilege), SPIE = bit 5 (previous
        // interrupt enable) — RISC-V privileged spec, sstatus register.
        assert_eq!(SSTATUS_SPP, 0x100);
        assert_eq!(SSTATUS_SPIE, 0x020);
    }

    #[test]
    fn gp_regs_layout_matches_asm_strides() {
        // gp_regs covers X1, X3–X9, X11–X31 (30 slots, GP_RA = 0 ..
        // GP_T6 = 28). x0 is hardwired zero; x2/x10 are named fields.
        assert_eq!(Riscv64CpuContext::GP_REGS_LEN, 30);
    }
}
