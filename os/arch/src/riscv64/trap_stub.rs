//! riscv64 trap entry stubs — the production S-mode trap legs (E-3ARCHTRAP).
//!
//! # Design (one paragraph)
//!
//! stvec Direct mode gives ONE vector address, but a production leg must
//! distinguish the interrupted privilege to pick its stack strategy: a
//! U-origin trap must swap to the per-CPU kernel stack via `sscratch`
//! (the user sp travels through `sscratch`), while an S-origin trap must
//! frame *below the interrupted kernel sp* — swapping there would stack a
//! second frame at the same kstack top and clobber the outer frame. Both
//! strategies also need a scratch register *before* any user register can
//! be saved (the origin test and the CSR reads all clobber something), and
//! on RISC-V there is no hardware-pushed frame to park state in. The leg
//! therefore follows the xv6-riscv model: **two vectors, selected by
//! switching `stvec`** — `load()` installs [`kernel_trap_vector`] (the
//! production default: every trap is a kernel trap until user mode exists),
//! and the U-return path (`TrapReturnArch::restore_to_user`) re-points
//! `stvec` at [`user_trap_vector`] right before `sret`, so `stvec` always
//! describes the mode the CPU is about to run in. The user leg flips
//! `stvec` back to the kernel leg as soon as its frame is saved, so a
//! kernel fault raised *inside* the dispatcher nests cleanly on the kernel
//! leg instead of recursing through the swap.
//!
//! # Mechanics / policy split (S-8, mirrored from x86_64::trap_stub)
//!
//! This module owns the *mechanics*: full GPR file + `sepc`/`sstatus`
//! frame ([`Riscv64TrapFrame`]), the stack strategies above, and the two
//! Rust thunks ([`riscv64_kernel_trap_dispatch`] /
//! [`riscv64_user_trap_dispatch`]) that forward to the kernel-registered
//! bodies (the *policy*: timer tick, kernel calls, fault diagnostics —
//! kernel `trap_dispatch`). Registration happens in the kernel's
//! `init_protection` strictly before `TrapEntryArch::load()`; an
//! unregistered dispatch panics instead of silently returning (the S-3d
//! lesson: silent misrouting costs ten debugging rounds).
//!
//! # Frame layout (the build↔run contract)
//!
//! 34 × 8 bytes: `gpr[i]` = architectural register `x_i` (indices 0–31 —
//! x0 has a slot even though it is hardwired zero, so slot index equals
//! register number; the interrupted sp travels in slot 2 for both
//! origins), `sepc` at slot 32, `sstatus` at slot 33. Pinned by
//! [`test_frame_layout_frozen`] plus the leg-shape contract in
//! `arch/tests/riscv64_trap_leg_shape.rs`. On the way out the two legs
//! differ: the kernel leg pops back to the interrupted `sp` (slot 2 is the
//! only copy of it, since `sscratch` keeps holding the kernel-stack top),
//! while the user leg takes the parked user `sp` back out of `sscratch`
//! with the same exchange it entered with — slot 2 is that same value
//! (written from `sscratch` at entry) and is not re-read on exit.
//!
//! # `ecall` PC semantics (minix-sys arch_trap contract)
//!
//! `ecall` does NOT advance `sepc` (unlike x86 `int` or arm64 `svc`): the
//! kernel syscall body must step the saved PC past the 4-byte instruction
//! before `sret`, or the call re-traps forever. Documented on the user-side
//! half in `minix-sys/src/arch_trap.rs`; the kernel body owns the step.
//!
//! # Park decision ABI (NK4-C 续-75, the riscv twin of aarch64 §1.113)
//!
//! The user-leg dispatch body returns a `u64` park decision: `PARK_NONE`
//! restores the frame and `sret`s normally; `PARK_RESCHEDULE` means the
//! body already saved the process context (a blocked IPC receiver) and
//! requests switch-after-pop — the epilogue unwinds the 34-slot frame
//! back to the clean kernel-stack top and branches to the registered
//! resched entry instead of returning to the parked process. Without the
//! unwind the kernel stack would ratchet down one frame per blocked
//! receive (the aarch64 §1.112 CodeReview Critical-1 shape: riscv runs
//! the EL1h-like single-kernel-stack model, no TSS.sp0 reload exists).
//!
//! # `stvec` describes the mode the CPU is about to run in
//!
//! Both epilogues re-point `stvec` AFTER writing the frame's `sstatus` (the
//! latch: the hardware cleared `SIE` on entry, so restoring the frame value
//! leaves the tail masked) and BEFORE any GPR reload (the vector address has
//! to travel through a register, and after the reload every register belongs
//! to the returning context) — so the invariant `load()` established, the
//! vector matches the upcoming privilege, survives every return. A kernel-leg
//! return goes back to S-mode, where an open `sstatus.SIE` would otherwise
//! deliver the next timer tick through the user leg's sscratch swap (the
//! kernel never owns the user-stack register) — the tick would build its
//! frame at `sscratch` − 272 and corrupt memory. The re-anchor plus the latch
//! ordering close that hole (both orders are pinned by the host test
//! `arch/tests/riscv64_trap_leg_shape.rs`, rule 5).
//!
//! Everything above is stated for the single-hart configuration this port runs
//! in today (`-smp 1`). When APs are brought up, `KERNEL_TRAP_STACK_BASE` and
//! the `sscratch` invariants in this module become per-CPU concerns (see the
//! note in `protection.rs`) and this paragraph must be re-read per hart.

use core::mem::offset_of;
use core::sync::atomic::{AtomicPtr, Ordering};
use minix_types::VirBytes;

/// Full register file saved by both trap legs.
///
/// Memory layout is the build↔run contract with the `global_asm!` below;
/// `gpr[i]` is register `x_i` (slot 0 = x0 pad, slot 2 = interrupted sp,
/// slot 10 = a0 …), followed by the mode-switch pair. Pinned by
/// [`test_frame_layout_frozen`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Riscv64TrapFrame {
    /// x0–x31; x0 is a pad (hardwired zero), slot 2 carries the
    /// interrupted stack pointer for both origins.
    pub gpr: [u64; 32],
    /// Value of `sepc` at entry — the interrupted instruction address
    /// (not advanced past `ecall`; that is the kernel body's job).
    pub sepc: u64,
    /// Value of `sstatus` at entry (SPP bit 8 = interrupted mode).
    pub sstatus: u64,
}

impl Riscv64TrapFrame {
    /// sstatus.SPP — the interrupted privilege mode (1 = S-mode).
    const SSTATUS_SPP: u64 = 1 << 8;

    /// Whether the trap interrupted S-mode (kernel) code.
    pub fn from_kernel(&self) -> bool {
        self.sstatus & Self::SSTATUS_SPP != 0
    }

    /// The interrupted stack pointer (frame slot 2).
    pub fn sp(&self) -> u64 {
        self.gpr[2]
    }
}

// ── Dispatcher registration gate (x86_64::trap_stub shape) ──────────────
//
// Per-arch slot semantics: the KERNEL leg (S-origin interrupts, kernel
// faults) takes the `trap` slot; the USER leg (ecall kernel calls, user
// faults) takes the `syscall` slot — the x86 split (exceptions/IRQs vs
// SYSCALL) is privilege-based here because RISC-V routes both call types
// through one `ecall` boundary.
//
// The user leg returns the park decision (§1.113 twin, see module docs);
// the kernel leg never parks (a blocked IPC is a U-origin event) and its
// epilogue ignores returns, so it stays unit-shaped.
type KernelDispatchFn = unsafe extern "C" fn(&mut Riscv64TrapFrame);
/// The `u64` return is the park decision (NK4-C 续-75, the riscv mirror
/// of aarch64 §1.113): [`PARK_NONE`] restores and `sret`s;
/// [`PARK_RESCHEDULE`] unwinds the frame and enters the resched thunk.
type DispatchFn = unsafe extern "C" fn(&mut Riscv64TrapFrame) -> u64;
/// Park decision: return to the interrupted context normally (`sret`).
pub const PARK_NONE: u64 = 0;
/// Park decision: switch-after-pop into the scheduler (blocked IPC).
pub const PARK_RESCHEDULE: u64 = 1;
static KERNEL_DISPATCH: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
static USER_DISPATCH: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// The park-and-reschedule entry (the riscv twin of aarch64 §1.113).
/// Registered by the kernel with a diverging thunk (re-acquire BKL +
/// `scheduler_loop`); the user leg branches here after unwinding a
/// blocked-receiver frame.
type ReschedFn = unsafe extern "C" fn() -> !;
static RESCHED_ENTRY: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register the kernel-side dispatch bodies (policy layer) with the entry
/// stubs (mechanics layer). `kernel` = kernel leg, `user` = user leg (the
/// one carrying the park decision). Must be called before
/// `TrapEntryArch::load()`.
pub fn register_dispatchers(kernel: KernelDispatchFn, user: DispatchFn) {
    KERNEL_DISPATCH.store(kernel as *mut (), Ordering::Release);
    USER_DISPATCH.store(user as *mut (), Ordering::Release);
}

/// Register the diverging resched thunk the park branch jumps to. Must run
/// before `TrapEntryArch::load()` alongside [`register_dispatchers`].
pub fn register_resched_entry(f: ReschedFn) {
    RESCHED_ENTRY.store(f as *mut (), Ordering::Release);
}

/// Kernel-leg dispatch thunk: reached from `riscv64_kernel_trap_vector`.
///
/// # Safety
///
/// Forwarded from the asm stub: `frame` points at the live frame this leg
/// built on the interrupted kernel stack.
#[unsafe(no_mangle)]
unsafe extern "C" fn riscv64_kernel_trap_dispatch(frame: &mut Riscv64TrapFrame) {
    let f = KERNEL_DISPATCH.load(Ordering::Acquire);
    assert!(
        !f.is_null(),
        "riscv64 kernel trap dispatched before register_dispatchers()"
    );
    unsafe { (core::mem::transmute::<*mut (), KernelDispatchFn>(f))(frame) }
}

/// User-leg dispatch thunk.
///
/// # Safety
///
/// Forwarded from the asm stub: `frame` points at the live frame this leg
/// built at the kernel-stack top (parked user sp in slot 2). The body's
/// park decision (a0) is forwarded to the user-leg epilogue (续-75).
#[unsafe(no_mangle)]
unsafe extern "C" fn riscv64_user_trap_dispatch(frame: &mut Riscv64TrapFrame) -> u64 {
    let f = USER_DISPATCH.load(Ordering::Acquire);
    assert!(
        !f.is_null(),
        "riscv64 user trap dispatched before register_dispatchers()"
    );
    unsafe { (core::mem::transmute::<*mut (), DispatchFn>(f))(frame) }
}

/// The park branch's destination (the riscv twin of aarch64 §1.113).
/// Reached by the user leg after unwinding a blocked-receiver frame; it
/// forwards to the kernel-registered diverging thunk. `-> !`, so the
/// never-returning `b` in the asm is well-formed.
///
/// # Safety
///
/// Called only from the user leg's park branch with sp at the clean
/// kernel-stack base and sscratch holding the parked user sp (the
/// invariant `restore_to_user` re-anchors before any new U-mode trap);
/// requires `register_resched_entry()` to have run.
#[unsafe(no_mangle)]
unsafe extern "C" fn riscv64_resched_entry() -> ! {
    let f = RESCHED_ENTRY.load(Ordering::Acquire);
    assert!(
        !f.is_null(),
        "riscv64 park before register_resched_entry() — wiring bug"
    );
    unsafe { (core::mem::transmute::<*mut (), ReschedFn>(f))() }
}

// ── The two legs (asm) ──────────────────────────────────────────────────
//
// Save order note: every register is stored from its architectural name
// BEFORE any scratch use, so the legs never clobber interrupted state —
// the scratch (t0) is only used after its own slot is saved. The frame
// restore mirrors the saves exactly; the CSR trio (sstatus/sepc/stvec) is
// written first on the way out because all three need a scratch register.
//
// This ordering is LOAD-BEARING, not style (NK4C 续-370). The kernel leg
// recovers the interrupted `sp` by arithmetically rebasing `t0`, so the
// moment `t0` is used as scratch before its own `sd` the interrupted
// context's `t0` is gone: slot 5 would hold a stack-shaped value and the
// `ld t0, 5*8(sp)` on the way out would hand the resumed S-mode
// instruction a bogus `t0` — an asynchronous, per-tick register theft
// that looks like nothing (no fault, no log). The aarch64 twin documents
// the same rule for its scratch (`arm64::trap_stub`'s EL1BODY: "x9 is
// saved into its slot before the first scratch use"); the shape is pinned
// for BOTH legs by the host integration test
// `arch/tests/riscv64_trap_leg_shape.rs::legs_save_scratch_before_using_it`.
core::arch::global_asm! {
    ".section .text.trap_stub, \"ax\"",

    // Kernel leg: frame below the interrupted kernel sp; no sscratch use
    // (it keeps holding the kernel-stack top for the user leg).
    ".align 2",
    ".globl riscv64_kernel_trap_vector",
    "riscv64_kernel_trap_vector:",
    "    addi sp, sp, -(34*8)",
    "    sd ra, 1*8(sp)",
    "    sd t0, 5*8(sp)",       // scratch first — the rebasing below owns t0
    "    addi t0, sp, 34*8",    // interrupted sp (pre-decrement value)
    "    sd t0, 2*8(sp)",
    "    sd gp, 3*8(sp)",
    "    sd tp, 4*8(sp)",
    "    sd t1, 6*8(sp)",
    "    sd t2, 7*8(sp)",
    "    sd s0, 8*8(sp)",
    "    sd s1, 9*8(sp)",
    "    sd a0, 10*8(sp)",
    "    sd a1, 11*8(sp)",
    "    sd a2, 12*8(sp)",
    "    sd a3, 13*8(sp)",
    "    sd a4, 14*8(sp)",
    "    sd a5, 15*8(sp)",
    "    sd a6, 16*8(sp)",
    "    sd a7, 17*8(sp)",
    "    sd s2, 18*8(sp)",
    "    sd s3, 19*8(sp)",
    "    sd s4, 20*8(sp)",
    "    sd s5, 21*8(sp)",
    "    sd s6, 22*8(sp)",
    "    sd s7, 23*8(sp)",
    "    sd s8, 24*8(sp)",
    "    sd s9, 25*8(sp)",
    "    sd s10, 26*8(sp)",
    "    sd s11, 27*8(sp)",
    "    sd t3, 28*8(sp)",
    "    sd t4, 29*8(sp)",
    "    sd t5, 30*8(sp)",
    "    sd t6, 31*8(sp)",
    "    csrr t0, sepc",
    "    sd t0, 32*8(sp)",
    "    csrr t0, sstatus",
    "    sd t0, 33*8(sp)",
    "    mv a0, sp",
    "    call riscv64_kernel_trap_dispatch",
    // Restore: CSR pair first (through t0), then the GPRs; sp (slot 2)
    // loads last and IS the frame-pop.
    //
    // stvec re-anchor (续-75): `load()` points stvec at this leg while
    // the kernel runs, but a user-leg entry flipped it here for the
    // dispatcher — any S-origin trap nested below a user frame returns
    // through THIS epilogue to S-mode, and leaving stvec on the user leg
    // would route the next S timer tick (sstatus.SIE stays open under
    // SBI) through the sscratch swap: sp := sscratch - 272, the frame
    // written over whatever user stack that points at. Pick the vector
    // from the saved SPP instead — the kernel leg for an S-return, the
    // user leg for a U-return (the latter mirrors the old unconditional
    // `la` below; an SPP==1 return still on the kernel leg is exactly
    // the invariant this closes). SPP is sstatus bit 8; the test branches
    // rather than arithmetically selects — the two legs are `.align 2`
    // siblings in one section but NOT adjacent, so their distance is a
    // layout artifact the epilogue must not depend on.
    "    ld t0, 33*8(sp)",
    "    csrw sstatus, t0",
    "    srli t0, t0, 8",
    "    andi t0, t0, 1",
    "    beqz t0, 8f",
    "    la t2, riscv64_kernel_trap_vector",
    "    j 9f",
    "8:",
    "    la t2, riscv64_user_trap_vector",
    "9:",
    "    csrw stvec, t2",
    "    ld t0, 32*8(sp)",
    "    csrw sepc, t0",
    "    ld ra, 1*8(sp)",
    "    ld gp, 3*8(sp)",
    "    ld tp, 4*8(sp)",
    "    ld t0, 5*8(sp)",
    "    ld t1, 6*8(sp)",
    "    ld t2, 7*8(sp)",
    "    ld s0, 8*8(sp)",
    "    ld s1, 9*8(sp)",
    "    ld a0, 10*8(sp)",
    "    ld a1, 11*8(sp)",
    "    ld a2, 12*8(sp)",
    "    ld a3, 13*8(sp)",
    "    ld a4, 14*8(sp)",
    "    ld a5, 15*8(sp)",
    "    ld a6, 16*8(sp)",
    "    ld a7, 17*8(sp)",
    "    ld s2, 18*8(sp)",
    "    ld s3, 19*8(sp)",
    "    ld s4, 20*8(sp)",
    "    ld s5, 21*8(sp)",
    "    ld s6, 22*8(sp)",
    "    ld s7, 23*8(sp)",
    "    ld s8, 24*8(sp)",
    "    ld s9, 25*8(sp)",
    "    ld s10, 26*8(sp)",
    "    ld s11, 27*8(sp)",
    "    ld t3, 28*8(sp)",
    "    ld t4, 29*8(sp)",
    "    ld t5, 30*8(sp)",
    "    ld t6, 31*8(sp)",
    "    ld sp, 2*8(sp)",
    "    sret",

    // User leg: swap to the kernel stack, frame at its top, park the user
    // sp from sscratch into slot 2, flip stvec to the kernel leg so a
    // dispatcher fault nests cleanly, dispatch, then mirror everything.
    ".align 2",
    ".globl riscv64_user_trap_vector",
    "riscv64_user_trap_vector:",
    "    csrrw sp, sscratch, sp",  // sp ← kstack top; sscratch ← user sp
    "    addi sp, sp, -(34*8)",
    "    sd ra, 1*8(sp)",
    "    sd gp, 3*8(sp)",
    "    sd tp, 4*8(sp)",
    "    sd t0, 5*8(sp)",
    "    sd t1, 6*8(sp)",
    "    sd t2, 7*8(sp)",
    "    sd s0, 8*8(sp)",
    "    sd s1, 9*8(sp)",
    "    sd a0, 10*8(sp)",
    "    sd a1, 11*8(sp)",
    "    sd a2, 12*8(sp)",
    "    sd a3, 13*8(sp)",
    "    sd a4, 14*8(sp)",
    "    sd a5, 15*8(sp)",
    "    sd a6, 16*8(sp)",
    "    sd a7, 17*8(sp)",
    "    sd s2, 18*8(sp)",
    "    sd s3, 19*8(sp)",
    "    sd s4, 20*8(sp)",
    "    sd s5, 21*8(sp)",
    "    sd s6, 22*8(sp)",
    "    sd s7, 23*8(sp)",
    "    sd s8, 24*8(sp)",
    "    sd s9, 25*8(sp)",
    "    sd s10, 26*8(sp)",
    "    sd s11, 27*8(sp)",
    "    sd t3, 28*8(sp)",
    "    sd t4, 29*8(sp)",
    "    sd t5, 30*8(sp)",
    "    sd t6, 31*8(sp)",
    "    csrr t0, sscratch",
    "    sd t0, 2*8(sp)",        // interrupted user sp (from sscratch)
    "    csrr t0, sepc",
    "    sd t0, 32*8(sp)",
    "    csrr t0, sstatus",
    "    sd t0, 33*8(sp)",
    // Kernel execution from here runs on the kernel leg: a fault in the
    // dispatcher must not re-enter the swap (it would stack a second frame
    // at the same kstack top over this one).
    "    la t0, riscv64_kernel_trap_vector",
    "    csrw stvec, t0",
    "    mv a0, sp",
    "    call riscv64_user_trap_dispatch",
    // Park decision (续-75, riscv mirror of aarch64 §1.113): a nonzero
    // return means the body saved this process's context and parked it
    // (blocked IPC receiver) — unwind the frame to the clean kernel-stack
    // base and branch to the resched entry instead of restoring. Skipping
    // the unwind ratchets the single kernel stack down one frame per
    // blocked receive (no TSS.sp0 reload exists on this arch). The kernel
    // leg stays installed in stvec here — the scheduler runs in S-mode,
    // and `restore_to_user` re-points it for the next U entry.
    "    beqz a0, 1f",
    "    addi sp, sp, 34*8",
    // `la`+`jr`, not `j`: the thunk lives in its own `.text.riscv64_
    // resched_entry` section (Rust per-function sectioning), and `j`
    // would emit a pc-relative R_RISCV_JAL (±1 MiB) whose reach the
    // linker layout does not guarantee (CodeReview 续-75 S3). t0 is
    // dead here (reloaded from slot 5 only on the restore path) and
    // the target is `-> !`, so no return address is needed.
    "    la t0, riscv64_resched_entry",
    "    jr t0",                      // -> ! (never returns here)
    "1:",
    // Restore. CSR pair first (they read through t0), then stvec — the
    // vector write ALSO goes through t0 and MUST stay ahead of the GPR
    // reload (NK4C 续-371): the user leg returns to U-mode, so every
    // register it touches after that register's own `ld` is a user value
    // it destroys. Doing the `stvec` write here is free precisely because
    // t0's user value is still in memory, waiting at slot 5.
    //
    // The position is load-bearing in BOTH directions, and rule 5 of
    // `arch/tests/riscv64_trap_leg_shape.rs` keeps it there: moving this
    // write BELOW the latch (`csrw sstatus`) would re-open the window where
    // S-mode code with `SIE` still set — `idle_halt` enables it — re-enters
    // THIS leg, whose frame would then be built at `sscratch` − 272, i.e. on
    // top of live kernel stack; moving it AFTER the reloads would steal a
    // restored user register (the 续-371 bug exactly).
    //
    // The user `sp` is NOT fetched from slot 2 on this path: the parked
    // value lives in `sscratch` (the leg's own entry exchange put it there
    // and nothing between then and here writes `sscratch` — the kernel leg
    // documentedly never touches it, and `restore_to_user`, which does
    // re-anchor it, never returns here). 续-371 removed a `ld t0, 2*8(sp)`
    // that this exchange never consumed: it was dead AND it left the
    // returning user context with `t0` = its own stack pointer.
    "    ld t0, 33*8(sp)",
    "    csrw sstatus, t0",
    "    ld t0, 32*8(sp)",
    "    csrw sepc, t0",
    "    la t0, riscv64_user_trap_vector",
    "    csrw stvec, t0",
    "    ld ra, 1*8(sp)",
    "    ld gp, 3*8(sp)",
    "    ld tp, 4*8(sp)",
    "    ld t0, 5*8(sp)",
    "    ld t1, 6*8(sp)",
    "    ld t2, 7*8(sp)",
    "    ld s0, 8*8(sp)",
    "    ld s1, 9*8(sp)",
    "    ld a0, 10*8(sp)",
    "    ld a1, 11*8(sp)",
    "    ld a2, 12*8(sp)",
    "    ld a3, 13*8(sp)",
    "    ld a4, 14*8(sp)",
    "    ld a5, 15*8(sp)",
    "    ld a6, 16*8(sp)",
    "    ld a7, 17*8(sp)",
    "    ld s2, 18*8(sp)",
    "    ld s3, 19*8(sp)",
    "    ld s4, 20*8(sp)",
    "    ld s5, 21*8(sp)",
    "    ld s6, 22*8(sp)",
    "    ld s7, 23*8(sp)",
    "    ld s8, 24*8(sp)",
    "    ld s9, 25*8(sp)",
    "    ld s10, 26*8(sp)",
    "    ld s11, 27*8(sp)",
    "    ld t3, 28*8(sp)",
    "    ld t4, 29*8(sp)",
    "    ld t5, 30*8(sp)",
    "    ld t6, 31*8(sp)",
    "    addi sp, sp, 34*8",     // pop the frame (sp = kstack top again)
    "    csrrw sp, sscratch, sp", // sp ← user sp (parked in sscratch); sscratch ← kstack top
    "    sret",

    // stvec Direct mode requires 4-byte alignment of the base; both legs
    // carry `.align 2` individually.
    ".size riscv64_kernel_trap_vector, .-riscv64_kernel_trap_vector",
}

unsafe extern "C" {
    /// Kernel-leg entry — installed by `TrapEntryArch::load()` (and
    /// `load_ap`): until a `sret` targets U-mode, every trap is a kernel
    /// trap.
    pub static riscv64_kernel_trap_vector: u8;
    /// User-leg entry — installed by the U-return path
    /// (`TrapReturnArch::restore_to_user`) right before `sret`.
    pub static riscv64_user_trap_vector: u8;
}

/// Address of the kernel-leg entry (the `load()` default).
pub fn kernel_trap_vector_va() -> VirBytes {
    // SAFETY: address-only symbol read; the symbol is defined by the
    // `global_asm!` above (K9 pattern: the symbol and its definition live
    // in the same module, so the old "declared but never defined" link
    // landmine cannot reappear).
    VirBytes::new(unsafe { &riscv64_kernel_trap_vector as *const u8 as usize } as u64)
}

/// Address of the user-leg entry (for the U-return path's `stvec` switch).
pub fn user_trap_vector_va() -> VirBytes {
    // SAFETY: address-only symbol read; see `kernel_trap_vector_va`.
    VirBytes::new(unsafe { &riscv64_user_trap_vector as *const u8 as usize } as u64)
}

// ── Frame ↔ saved-context translation (the riscv twins of x86/aarch64
// `save_frame_to_context` / `sync_status_register_to_frame`, NK4-C
// §1.112/§1.113 — wired for the 续-75 IPC bridge) ────────────────────────

/// Frame `gpr` slot → `Riscv64CpuContext::gp_regs` index, one entry per
/// architectural register x0..x31; `None` for the three registers the
/// context stores as named fields instead (x0 hardwired zero — no slot —
/// plus x2 = sp and x10 = a0), and for x0 itself.
///
/// The context's `gp_regs` is the 30-slot compact file (X1, X3–X9,
/// X11–X31 — see `signal::GP_*`); the frame is the full 32-slot file
/// where slot index == register number. This table is the single source
/// of truth for the hole-skipping stride, pinned against the `GP_*`
/// constants by `test_frame_slot_map_matches_gp_constants`.
const fn frame_slot_to_gp_reg(slot: usize) -> Option<usize> {
    match slot {
        1 => Some(crate::riscv64::signal::GP_RA),
        3 => Some(crate::riscv64::signal::GP_GP),
        4 => Some(crate::riscv64::signal::GP_TP),
        5 => Some(crate::riscv64::signal::GP_T0),
        6 => Some(crate::riscv64::signal::GP_T1),
        7 => Some(crate::riscv64::signal::GP_T2),
        8 => Some(crate::riscv64::signal::GP_S0),
        9 => Some(crate::riscv64::signal::GP_S1),
        11 => Some(crate::riscv64::signal::GP_A1),
        12 => Some(crate::riscv64::signal::GP_A2),
        13 => Some(crate::riscv64::signal::GP_A3),
        14 => Some(crate::riscv64::signal::GP_A4),
        15 => Some(crate::riscv64::signal::GP_A5),
        16 => Some(crate::riscv64::signal::GP_A6),
        17 => Some(crate::riscv64::signal::GP_A7),
        18 => Some(crate::riscv64::signal::GP_S2),
        19 => Some(crate::riscv64::signal::GP_S3),
        20 => Some(crate::riscv64::signal::GP_S4),
        21 => Some(crate::riscv64::signal::GP_S5),
        22 => Some(crate::riscv64::signal::GP_S6),
        23 => Some(crate::riscv64::signal::GP_S7),
        24 => Some(crate::riscv64::signal::GP_S8),
        25 => Some(crate::riscv64::signal::GP_S9),
        26 => Some(crate::riscv64::signal::GP_S10),
        27 => Some(crate::riscv64::signal::GP_S11),
        28 => Some(crate::riscv64::signal::GP_T3),
        29 => Some(crate::riscv64::signal::GP_T4),
        30 => Some(crate::riscv64::signal::GP_T5),
        31 => Some(crate::riscv64::signal::GP_T6),
        _ => None,
    }
}

/// Persist the interrupted user register file into the process's saved
/// context — the riscv mirror of
/// [`crate::arm64::trap_stub::save_frame_to_context`]. The IPC delivery
/// paths OR status / set return values into this saved context (the
/// resume asm reads it back via `restore_to_user`), so the entry save
/// must land every live register first. Slot 2 (sp) and slot 10 (a0) are
/// the named context fields; `sepc`/`sstatus` map to `sepc`/`sstatus`
/// (offset convention pinned by `boot::write_user_register`'s doc).
pub fn save_frame_to_context(frame: &Riscv64TrapFrame, ctx: &mut super::boot::Riscv64CpuContext) {
    ctx.sp = frame.gpr[2];
    ctx.a0 = frame.gpr[10];
    ctx.sepc = frame.sepc;
    ctx.sstatus = frame.sstatus;
    let mut slot = 1usize;
    while slot < 32 {
        if let Some(idx) = frame_slot_to_gp_reg(slot) {
            ctx.gp_regs[idx] = frame.gpr[slot];
        }
        slot += 1;
    }
}

/// Pull the IPC status register from a saved context into the outgoing
/// trap frame so the `sret` leg returns the up-to-date value — the riscv
/// mirror of `sync_status_register_to_frame` (x86: R10, aarch64: X1,
/// riscv: **A1** per `or_ipc_status_reg`). The delivery path ORed
/// `IpcCall` bits into `gp_regs[GP_A1]` after the entry save, so the
/// frame's A1 is stale and must be refreshed before returning to user.
pub fn sync_status_register_to_frame(
    ctx: &super::boot::Riscv64CpuContext,
    frame: &mut Riscv64TrapFrame,
) {
    frame.gpr[11] = ctx.gp_regs[crate::riscv64::signal::GP_A1];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_layout_frozen() {
        // The asm stores gpr[i] at i*8, sepc at 32*8, sstatus at 33*8
        // (34-slot frame, the build↔run contract with both legs).
        let base = core::mem::size_of::<u64>();
        assert_eq!(offset_of!(Riscv64TrapFrame, gpr), 0);
        assert_eq!(offset_of!(Riscv64TrapFrame, sepc), 32 * base);
        assert_eq!(offset_of!(Riscv64TrapFrame, sstatus), 33 * base);
        assert_eq!(core::mem::size_of::<Riscv64TrapFrame>(), 34 * base);
    }

    #[test]
    fn test_spp_reads_interrupted_mode() {
        // SPP is sstatus bit 8 (RISC-V privileged spec): 1 = interrupted
        // S-mode, 0 = U-mode. The kernel bodies classify origin with this.
        let mut f = Riscv64TrapFrame {
            gpr: [0; 32],
            sepc: 0,
            sstatus: 0,
        };
        assert!(!f.from_kernel());
        f.sstatus = 1 << 8;
        assert!(f.from_kernel());
    }

    #[test]
    fn test_frame_slot_map_matches_gp_constants() {
        // The compact context file's LIVE span is 29 registers (GP_RA=0 ..
        // GP_T6=28: X1, X3–X9, X11–X31 minus the named-field holes x0/x2/
        // x10); `GP_REGS_LEN = 30` carries one reserved padding slot (29)
        // that no architectural register maps into (signal.rs's tail
        // comment documents it). The slot map must be defined on every
        // mappable register and land each on a DISTINCT live index. A
        // wrong entry here silently corrupts one register per
        // blocked-then-resumed IPC (aarch64 §1.113 CodeReview BLOCKER
        // shape — hard-coded offsets drifting from the GP_* constants).
        const LIVE: usize = crate::riscv64::signal::GP_T6 + 1;
        let mut seen = [false; super::super::boot::Riscv64CpuContext::GP_REGS_LEN];
        let mut mapped = 0usize;
        for slot in 0..32usize {
            match frame_slot_to_gp_reg(slot) {
                Some(idx) => {
                    assert!(idx < LIVE, "slot {slot} maps outside the live span");
                    assert!(!seen[idx], "slot {slot} re-maps gp_regs index {idx}");
                    seen[idx] = true;
                    mapped += 1;
                }
                None => {
                    // Exactly the three unwritten slots: x0, x2 (sp), x10 (a0).
                    assert!(
                        matches!(slot, 0 | 2 | 10),
                        "slot {slot} unexpectedly has no context home"
                    );
                }
            }
        }
        assert_eq!(mapped, LIVE, "every architectural register has a home");
        assert!(
            seen[..LIVE].iter().all(|s| *s),
            "every live index is covered"
        );
        assert!(!seen[LIVE], "the reserved padding slot stays unmapped");
    }

    #[test]
    fn test_save_frame_to_context_persists_all_fields() {
        // Discriminating mirror of x86's save test: fill every frame slot
        // with a distinct value and assert the context received each at
        // its architectural home — including the named fields (sp/a0),
        // sepc/sstatus, and the two IPC lanes (A1 status, A7 call nr).
        use super::super::boot::Riscv64CpuContext;
        let mut frame = Riscv64TrapFrame {
            gpr: [0; 32],
            sepc: 0x2000,
            sstatus: 0x200,
        };
        for (i, v) in frame.gpr.iter_mut().enumerate() {
            *v = 0x1000 + i as u64 * 8;
        }
        let mut ctx = Riscv64CpuContext::new();
        save_frame_to_context(&frame, &mut ctx);
        assert_eq!(ctx.sp, frame.gpr[2]);
        assert_eq!(ctx.a0, frame.gpr[10]);
        assert_eq!(ctx.sepc, 0x2000);
        assert_eq!(ctx.sstatus, 0x200);
        // Spot-check the compact-file stride around both holes and the
        // IPC lanes: X11 (A1) follows a0's hole, X17 (A7) is the call nr.
        assert_eq!(ctx.gp_regs[crate::riscv64::signal::GP_A1], frame.gpr[11]);
        assert_eq!(ctx.gp_regs[crate::riscv64::signal::GP_A7], frame.gpr[17]);
        assert_eq!(ctx.gp_regs[crate::riscv64::signal::GP_T6], frame.gpr[31]);
        assert_eq!(ctx.gp_regs[crate::riscv64::signal::GP_RA], frame.gpr[1]);
    }

    #[test]
    fn test_sync_status_register_pulls_a1_lane() {
        // The delivery path ORs IpcCall bits into gp_regs[GP_A1]; the
        // reply leg must pull THAT lane back into frame slot 11 — pulling
        // the wrong slot would return a stale A1 and re-trigger the x86
        // B21 family (is_ipc_notify misfire on the receiver's own return).
        use super::super::boot::Riscv64CpuContext;
        let mut ctx = Riscv64CpuContext::new();
        ctx.gp_regs[crate::riscv64::signal::GP_A1] = 0x0000_1100;
        ctx.gp_regs[crate::riscv64::signal::GP_A2] = 0xdead_beef;
        let mut frame = Riscv64TrapFrame {
            gpr: [0; 32],
            sepc: 0,
            sstatus: 0,
        };
        sync_status_register_to_frame(&ctx, &mut frame);
        assert_eq!(frame.gpr[11], 0x0000_1100);
        assert_eq!(frame.gpr[12], 0, "only the A1 lane is refreshed");
    }

    #[test]
    fn test_park_decision_values() {
        // The asm epilogue tests the return with `beqz`: PARK_NONE must
        // be exactly 0 and PARK_RESCHEDULE exactly nonzero — the same
        // values the aarch64 §1.113 legs use (one ABI across the twins).
        assert_eq!(PARK_NONE, 0);
        assert_eq!(PARK_RESCHEDULE, 1);
    }

    // 两腿 asm 的形状契约（帧覆盖 + scratch 纪律）由宿主集成测试
    // `arch/tests/riscv64_trap_leg_shape.rs` 把守：本模块带
    // `#[cfg(target_arch = "riscv64")]`，写在这里的 `#[test]` 在宿主
    // `cargo test` 下根本不编译（=假绿），故测试不住这儿（§续-370）。
}
