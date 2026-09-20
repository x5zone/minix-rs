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
//! [`test_frame_layout_frozen`]. The user leg restores `sp` from slot 2
//! and exchanges it with `sscratch` on the way out, which leaves
//! `sscratch` holding the kernel-stack top again — the invariant
//! `ProtectionArch::init` established.
//!
//! # `ecall` PC semantics (minix-sys arch_trap contract)
//!
//! `ecall` does NOT advance `sepc` (unlike x86 `int` or arm64 `svc`): the
//! kernel syscall body must step the saved PC past the 4-byte instruction
//! before `sret`, or the call re-traps forever. Documented on the user-side
//! half in `minix-sys/src/arch_trap.rs`; the kernel body owns the step.

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
type DispatchFn = unsafe extern "C" fn(&mut Riscv64TrapFrame);
static KERNEL_DISPATCH: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
static USER_DISPATCH: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register the kernel-side dispatch bodies (policy layer) with the entry
/// stubs (mechanics layer). `trap` = kernel leg, `syscall` = user leg.
/// Must be called before `TrapEntryArch::load()`.
pub fn register_dispatchers(kernel: DispatchFn, user: DispatchFn) {
    KERNEL_DISPATCH.store(kernel as *mut (), Ordering::Release);
    USER_DISPATCH.store(user as *mut (), Ordering::Release);
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
    unsafe { (core::mem::transmute::<*mut (), DispatchFn>(f))(frame) }
}

/// User-leg dispatch thunk: reached from `riscv64_user_trap_vector`.
///
/// # Safety
///
/// Forwarded from the asm stub: `frame` points at the live frame this leg
/// built at the kernel-stack top (parked user sp in slot 2).
#[unsafe(no_mangle)]
unsafe extern "C" fn riscv64_user_trap_dispatch(frame: &mut Riscv64TrapFrame) {
    let f = USER_DISPATCH.load(Ordering::Acquire);
    assert!(
        !f.is_null(),
        "riscv64 user trap dispatched before register_dispatchers()"
    );
    unsafe { (core::mem::transmute::<*mut (), DispatchFn>(f))(frame) }
}

// ── The two legs (asm) ──────────────────────────────────────────────────
//
// Save order note: every register is stored from its architectural name
// BEFORE any scratch use, so the legs never clobber interrupted state —
// the scratch (t0) is only used after its own slot is saved. The frame
// restore mirrors the saves exactly; CSR writes (sstatus/sepc) come first
// on the way out because they read through t0.
core::arch::global_asm! {
    ".section .text.trap_stub, \"ax\"",

    // Kernel leg: frame below the interrupted kernel sp; no sscratch use
    // (it keeps holding the kernel-stack top for the user leg).
    ".align 2",
    ".globl riscv64_kernel_trap_vector",
    "riscv64_kernel_trap_vector:",
    "    addi sp, sp, -(34*8)",
    "    sd ra, 1*8(sp)",
    "    addi t0, sp, 34*8",     // interrupted sp (pre-decrement value)
    "    sd t0, 2*8(sp)",
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
    "    csrr t0, sepc",
    "    sd t0, 32*8(sp)",
    "    csrr t0, sstatus",
    "    sd t0, 33*8(sp)",
    "    mv a0, sp",
    "    call riscv64_kernel_trap_dispatch",
    // Restore: CSR pair first (through t0), then the GPRs; sp (slot 2)
    // loads last and IS the frame-pop.
    "    ld t0, 33*8(sp)",
    "    csrw sstatus, t0",
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
    // Restore. The user sp (slot 2) travels out through t0 for the final
    // sscratch exchange, so it loads after every plain GPR restore.
    "    ld t0, 33*8(sp)",
    "    csrw sstatus, t0",
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
    // stvec back to the user leg BEFORE the GPR restore clobbers the
    // scratch: the next trap from U-mode must land here again.
    "    la t0, riscv64_user_trap_vector",
    "    csrw stvec, t0",
    "    ld t0, 2*8(sp)",        // user sp
    "    addi sp, sp, 34*8",     // pop the frame (sp = kstack top again)
    "    csrrw sp, sscratch, sp", // sp ← user sp; sscratch ← kstack top
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
}
