//! arm64 trap entry stubs — the production EL1 exception legs (E-3ARCHTRAP).
//!
//! # Design (one paragraph)
//!
//! The ARM64 exception vector hardware-discriminates the interrupted
//! context into 16 slots (4 exception classes × 4 origin groups), so —
//! unlike riscv64's single Direct-mode `stvec` — no software origin test
//! is needed and each slot carries the stack strategy its group demands.
//! Lower-EL (EL0) entries frame on the EL1 entry stack (`SP_EL1`, which
//! the exception entry selected automatically) and swap the interrupted
//! user stack pointer in from `SP_EL0` — that save/restore pair is the
//! SP_EL0 exchange: EL0 state parks in the frame, EL1 runs on its own
//! stack, and `eret` puts both back. Current-EL (kernel) entries frame
//! below the interrupted kernel `sp` — no stack switch at all, so a
//! dispatcher fault nests cleanly one frame deeper. Slots that are
//! architecturally unreachable for this kernel (current-EL-with-SP0:
//! the kernel never runs with SPSel=0; lower-EL AArch32: no 32-bit
//! guests) and the non-deliverable classes (FIQ: Group 0/secure only;
//! SError: no handler contract yet) keep the print-and-halt diagnostic —
//! "observable, never silent".
//!
//! # Mechanics / policy split (S-8, mirrored from x86_64::trap_stub)
//!
//! This module owns the *mechanics*: full register file + `elr`/`spsr`
//! frame ([`AArch64TrapFrame`]), the per-group stack strategies above,
//! and the two Rust thunks ([`aarch64_kernel_trap_dispatch`] /
//! [`aarch64_user_trap_dispatch`]) that forward to the kernel-registered
//! bodies (the *policy*: GIC claim/route, timer tick, SVC kernel calls,
//! fault diagnostics — kernel `trap_dispatch`). Registration happens in
//! the kernel's `init_protection` strictly before `TrapEntryArch::load()`;
//! an unregistered dispatch panics instead of silently returning (the
//! S-3d lesson).
//!
//! # Frame layout (the build↔run contract)
//!
//! 34 × 8 bytes: `gpr[i]` = `x_i` (indices 0–30), interrupted sp at 31
//! (user sp from `SP_EL0` on lower-EL entries, kernel sp on current-EL
//! entries), `elr` at 32, `spsr` at 33. Pinned by
//! [`test_frame_layout_frozen`].
//!
//! # Register liveness in the asm
//!
//! Every entry needs a scratch (x9) for `SP_EL0`/`ELR`/`SPSR` reads, and
//! every GPR holds interrupted state. Current-EL legs order their stores
//! so x9 is saved into its frame slot *before* the first scratch use.
//! Lower-EL legs cannot frame before capturing `SP_EL0`, so they park the
//! two registers the early sequence needs (x9, x10) in a 16-byte pocket
//! just above the frame, then fold them into the frame once the CSR pair
//! is captured — restoring them last, from the same pocket.
//!
//! # `svc` PC semantics (minix-sys arch_trap contract)
//!
//! `svc` DOES advance the return PC (`ELR_EL1` = next instruction, ARM
//! exception semantics) — the kernel body must NOT step the saved PC,
//! unlike the riscv64 `ecall` leg. Documented on the user-side half in
//! `minix-sys/src/arch_trap.rs`.

use core::mem::offset_of;
use core::sync::atomic::{AtomicPtr, Ordering};
use minix_types::VirBytes;

/// Full register file saved by the production legs.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AArch64TrapFrame {
    /// x0–x30 (31 slots; SP is not a GPR on ARM64 and travels in
    /// [`AArch64TrapFrame::sp`]).
    pub gpr: [u64; 31],
    /// Interrupted stack pointer: the EL0 sp (from `SP_EL0`) on
    /// lower-EL entries, the interrupted kernel sp on current-EL entries.
    pub sp: u64,
    /// Value of `ELR_EL1` at entry.
    pub elr: u64,
    /// Value of `SPSR_EL1` at entry (M field bits [4:0] = interrupted
    /// mode; 0b00000 = EL0).
    pub spsr: u64,
}

impl AArch64TrapFrame {
    /// SPSR.M — the interrupted exception level, bits [4:0].
    const SPSR_M_MASK: u64 = 0x1F;
    /// SPSR.M value for EL0 (AArch64).
    const SPSR_M_EL0: u64 = 0;

    /// Whether the trap interrupted EL0 (user) code.
    pub fn from_user(&self) -> bool {
        self.spsr & Self::SPSR_M_MASK == Self::SPSR_M_EL0
    }
}

/// Persist an interrupted aarch64 user register file into the per-process
/// saved context — the EL1 mirror of the x86-64 `save_frame_to_context`
/// (design decision 3: `CpuContext` is the single user-state truth; the
/// scheduler restore asm in [`crate::arm64::trap_return`] reads
/// `r0`/`gp_regs`/`pc`/`psr`/`sp` back out of it before `eret`).
///
/// [`AArch64CpuContext`] stores X0 separately (`r0`) and X1..X30 in
/// `gp_regs[0..30]` (so `gp_regs[i]` = X(i+1)), matching
/// [`crate::arm64::boot`]’s `write_user_register` offset convention. SP is
/// not a GPR on ARM64: the interrupted EL0 sp travels in
/// [`AArch64TrapFrame::sp`] and lands in `ctx.sp` (SP_EL0).
pub fn save_frame_to_context(frame: &AArch64TrapFrame, ctx: &mut super::boot::AArch64CpuContext) {
    ctx.r0 = frame.gpr[0];
    let mut i = 0;
    while i < super::boot::AArch64CpuContext::GP_REGS_LEN {
        ctx.gp_regs[i] = frame.gpr[i + 1];
        i += 1;
    }
    ctx.sp = frame.sp;
    ctx.pc = frame.elr;
    ctx.psr = frame.spsr;
}

/// Pull the IPC status register from a saved context into the outgoing
/// trap frame so the `eret` leg returns the up-to-date value — the EL1
/// mirror of [`crate::x86_64::trap_stub::sync_status_register_to_frame`].
///
/// The aarch64 status lane is X1 (`gp_regs[GP_X1]`, the analogue of x86's
/// R10): the delivery path ORs `IpcCall` status bits into it via
/// `AArch64CpuContextArch::or_ipc_status_reg`, and the plain-RECEIVE
/// prologue clears it. On the receiving process's own trap return the
/// entry save happened before that OR, so the frame's X1 is stale and must
/// be refreshed from the context.
pub fn sync_status_register_to_frame(
    ctx: &super::boot::AArch64CpuContext,
    frame: &mut AArch64TrapFrame,
) {
    frame.gpr[1] = ctx.gp_regs[crate::arm64::signal::GP_X1];
}

// ── Dispatcher registration gate (x86_64::trap_stub shape) ──────────────
//
// Per-arch slot semantics: the KERNEL leg (current-EL entries: kernel
// interrupts and faults) takes the `trap` slot; the USER leg (lower-EL
// entries: SVC kernel calls, user faults, interrupts arriving from EL0)
// takes the `syscall` slot — the x86 split (exceptions/IRQs vs SYSCALL)
// is origin-group-based here because the vector table makes the split in
// hardware.
//
// The second argument is the exception class the asm slot ran (0 =
// synchronous, 1 = IRQ — the E-3ARCHTRAP convention, the analogue of the
// x86 vector number in the frame): the sync and IRQ slots of a group
// share one thunk/body pair, and ARM64 has no IRQ syndrome register, so
// the class travels as an explicit operand instead.
/// The `u64` return is the park decision (NK4-C §1.113): `0` means
/// "restore this frame and `eret` normally"; nonzero means the dispatcher
/// already saved the process context and requests a switch-after-pop (a
/// blocked IPC receiver) — `EL0BODY` then unwinds the frame without
/// restoring and branches to the registered resched entry instead of
/// `eret`-ing back. Kernel-origin legs always return `0`.
type DispatchFn = unsafe extern "C" fn(&mut AArch64TrapFrame, u64) -> u64;
/// Park decision: return to the interrupted context normally (`eret`).
pub const PARK_NONE: u64 = 0;
/// Park decision: switch-after-pop into the scheduler (blocked IPC).
pub const PARK_RESCHEDULE: u64 = 1;
/// Exception class operand: a synchronous exception (syndrome in
/// ESR_EL1).
pub const TRAP_CLASS_SYNC: u64 = 0;
/// Exception class operand: an interrupt (no syndrome).
pub const TRAP_CLASS_IRQ: u64 = 1;
static KERNEL_DISPATCH: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
static USER_DISPATCH: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// The park-and-reschedule entry (NK4-C §1.113). Registered by the kernel
/// with a diverging thunk (re-acquire BKL + `scheduler_loop`); `EL0BODY`
/// branches here after unwinding a blocked-receiver frame instead of
/// `eret`-ing back to the parked process.
type ReschedFn = unsafe extern "C" fn() -> !;
static RESCHED_ENTRY: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register the kernel-side dispatch bodies (policy layer) with the entry
/// stubs (mechanics layer). `trap` = current-EL leg, `syscall` = lower-EL
/// leg. Must be called before `TrapEntryArch::load()`.
pub fn register_dispatchers(kernel: DispatchFn, user: DispatchFn) {
    KERNEL_DISPATCH.store(kernel as *mut (), Ordering::Release);
    USER_DISPATCH.store(user as *mut (), Ordering::Release);
}

/// Register the diverging resched thunk the park branch jumps to. Must run
/// before `TrapEntryArch::load()` alongside [`register_dispatchers`].
pub fn register_resched_entry(f: ReschedFn) {
    RESCHED_ENTRY.store(f as *mut (), Ordering::Release);
}

/// Current-EL (kernel) dispatch thunk.
///
/// # Safety
///
/// Forwarded from the asm stub: `frame` points at the live frame this leg
/// built below the interrupted kernel stack pointer.
#[unsafe(no_mangle)]
unsafe extern "C" fn aarch64_kernel_trap_dispatch(frame: &mut AArch64TrapFrame, class: u64) -> u64 {
    let f = KERNEL_DISPATCH.load(Ordering::Acquire);
    assert!(
        !f.is_null(),
        "aarch64 kernel trap dispatched before register_dispatchers()"
    );
    // Kernel-origin legs never park; the returned code is ignored by the
    // `EL1BODY` epilogue. Forward it unchanged for ABI symmetry.
    unsafe { (core::mem::transmute::<*mut (), DispatchFn>(f))(frame, class) }
}

/// Lower-EL (user) dispatch thunk.
///
/// # Safety
///
/// Forwarded from the asm stub: `frame` points at the live frame this leg
/// built on the EL1 entry stack (interrupted EL0 sp in `frame.sp`). The
/// body's park decision (x0) is forwarded to `EL0BODY` (§1.113).
#[unsafe(no_mangle)]
unsafe extern "C" fn aarch64_user_trap_dispatch(frame: &mut AArch64TrapFrame, class: u64) -> u64 {
    let f = USER_DISPATCH.load(Ordering::Acquire);
    assert!(
        !f.is_null(),
        "aarch64 user trap dispatched before register_dispatchers()"
    );
    unsafe { (core::mem::transmute::<*mut (), DispatchFn>(f))(frame, class) }
}

/// The park branch's destination (NK4-C §1.113). `EL0BODY` jumps here
/// (tail, `b`) after unwinding a blocked-receiver frame; it forwards to
/// the kernel-registered diverging thunk. `-> !`, so the never-returning
/// `b` in the asm is well-formed.
///
/// # Safety
///
/// Called only from `EL0BODY`'s park branch with SP_EL1 restored to the
/// clean entry base; requires `register_resched_entry()` to have run.
#[unsafe(no_mangle)]
unsafe extern "C" fn aarch64_resched_entry() -> ! {
    let f = RESCHED_ENTRY.load(Ordering::Acquire);
    assert!(
        !f.is_null(),
        "aarch64 park before register_resched_entry() — wiring bug"
    );
    unsafe { (core::mem::transmute::<*mut (), ReschedFn>(f))() }
}

// ── The exception vector table (E-3ARCHTRAP: all 16 slots wired) ────────
//
// 2 KiB table, `.align 11` (VBAR_EL1 mandates bits [10:0] zero), sixteen
// 128-byte slots in hardware order: exception class × origin group. Slot
// offsets are address bits [9:7]; the lower-EL AArch64 synchronous slot
// (VBAR + 0x400) is the SVC entry.
//
// No `.size` directive: LLVM's integrated assembler rejects the GNU
// `.size` pseudo-op in inline asm on this target (live finding, K9); the
// table's extent is fixed by construction — `.align 11` plus sixteen
// 128-byte slots.
core::arch::global_asm! {
    ".section .text.trap_stub, \"ax\"",
    ".align 11",
    ".globl exc_vector_table",
    "exc_vector_table:",
    // One 128-byte slot branching to its class handler.
    ".macro VEC name",
    "b  \\name",
    ".space 124",
    ".endm",

    // ── Group 0 — current EL with SP0 (kernel must never run with SPSel=0).
    "VEC exc_bad_mode",
    "VEC exc_bad_mode",
    "VEC exc_bad_mode",
    "VEC exc_bad_mode",

    // ── Group 1 — current EL with SPx (kernel-origin; the live slots).
    "VEC el1_sync",
    "VEC el1_irq",
    "VEC el1_fiq",
    "VEC el1_serror",

    // ── Group 2 — lower EL with AArch64 (user-origin; the EL0 bridge).
    "VEC el0_sync",
    "VEC el0_irq",
    "VEC el0_fiq",
    "VEC el0_serror",

    // ── Group 3 — lower EL with AArch32 (never: no 32-bit guests).
    "VEC exc_bad_mode",
    "VEC exc_bad_mode",
    "VEC exc_bad_mode",
    "VEC exc_bad_mode",

    // Current-EL (kernel) flavor: frame below the interrupted kernel sp;
    // x9 is saved into its slot before the first scratch use, the
    // interrupted sp is recovered as frame_base + frame_size, and the
    // exception class rides x1 (the dispatch ABI's second operand).
    ".macro EL1BODY handler class",
    "sub sp, sp, #(34*8)",
    "stp x0, x1, [sp, #0]",
    "stp x2, x3, [sp, #16]",
    "stp x4, x5, [sp, #32]",
    "stp x6, x7, [sp, #48]",
    "stp x8, x9, [sp, #64]",
    "stp x10, x11, [sp, #80]",
    "stp x12, x13, [sp, #96]",
    "stp x14, x15, [sp, #112]",
    "stp x16, x17, [sp, #128]",
    "stp x18, x19, [sp, #144]",
    "stp x20, x21, [sp, #160]",
    "stp x22, x23, [sp, #176]",
    "stp x24, x25, [sp, #192]",
    "stp x26, x27, [sp, #208]",
    "stp x28, x29, [sp, #224]",
    "mov x9, sp",
    "add x9, x9, #(34*8)",      // interrupted kernel sp
    "stp x30, x9, [sp, #240]",
    "mrs x9, elr_el1",
    "str x9, [sp, #256]",
    "mrs x9, spsr_el1",
    "str x9, [sp, #264]",
    "mov x0, sp",
    "mov x1, #\\class",
    "bl  \\handler",
    "ldr x9, [sp, #264]",
    "msr spsr_el1, x9",
    "ldr x9, [sp, #256]",
    "msr elr_el1, x9",
    "ldp x0, x1, [sp, #0]",
    "ldp x2, x3, [sp, #16]",
    "ldp x4, x5, [sp, #32]",
    "ldp x6, x7, [sp, #48]",
    "ldp x8, x9, [sp, #64]",
    "ldp x10, x11, [sp, #80]",
    "ldp x12, x13, [sp, #96]",
    "ldp x14, x15, [sp, #112]",
    "ldp x16, x17, [sp, #128]",
    "ldp x18, x19, [sp, #144]",
    "ldp x20, x21, [sp, #160]",
    "ldp x22, x23, [sp, #176]",
    "ldp x24, x25, [sp, #192]",
    "ldp x26, x27, [sp, #208]",
    "ldp x28, x29, [sp, #224]",
    "ldp x30, x9, [sp, #240]",
    "add sp, sp, #(34*8)",      // sp = interrupted kernel sp
    "eret",
    ".endm",

    // Lower-EL (user) flavor: the EL0 sp must be captured from SP_EL0
    // before anything else can be framed, so x9/x10 first park in a
    // 16-byte pocket above the frame; the EL0 sp rides x9 into slot 31,
    // the CSR pair follows, and the parked registers fold into slots 9/10
    // last. The restore mirrors it, SP_EL0 exchange included.
    ".macro EL0BODY handler class",
    "stp x9, x10, [sp, #-16]!",  // pocket above the frame
    "mrs x9, sp_el0",            // interrupted EL0 sp
    "sub sp, sp, #(34*8)",
    "stp x0, x1, [sp, #0]",
    "stp x2, x3, [sp, #16]",
    "stp x4, x5, [sp, #32]",
    "stp x6, x7, [sp, #48]",
    "str x8, [sp, #64]",
    "str x9, [sp, #248]",        // slot 31: the EL0 sp (x9 still holds it)
    "stp x11, x12, [sp, #88]",
    "stp x13, x14, [sp, #104]",
    "stp x15, x16, [sp, #120]",
    "stp x17, x18, [sp, #136]",
    "stp x19, x20, [sp, #152]",
    "stp x21, x22, [sp, #168]",
    "stp x23, x24, [sp, #184]",
    "stp x25, x26, [sp, #200]",
    "stp x27, x28, [sp, #216]",
    "stp x29, x30, [sp, #232]",
    "mrs x9, elr_el1",
    "str x9, [sp, #256]",
    "mrs x9, spsr_el1",
    "str x9, [sp, #264]",
    "ldp x9, x10, [sp, #(34*8)]", // recover parked user x9/x10
    "str x9, [sp, #72]",           // slot 9
    "str x10, [sp, #80]",          // slot 10
    "mov x0, sp",
    "mov x1, #\\class",
    "bl  \\handler",
    // NK4-C §1.113 switch-after-pop: a nonzero return (x0) from the
    // handler is a park-and-reschedule request (blocked IPC receiver).
    // The dispatcher already saved this process's register state into its
    // cpu_context, so discard the EL0 frame WITHOUT restoring registers
    // and WITHOUT eret. Unwinding SP_EL1 back to its clean entry base (the
    // 34*8 frame + the 16-byte x9/x10 pocket) is mandatory: unlike x86,
    // whose every later trap reloads RSP from TSS.sp0, the EL1h model runs
    // on a single kernel stack loaded once by jump_to_kmain, so leaving
    // the frame in place would ratchet SP_EL1 down into .bss one blocked
    // receive at a time. The parked process resumes later through
    // finish_and_restore when a delivery wakes it.
    "cbz x0, 9f",
    "add sp, sp, #(34*8)",
    "add sp, sp, #16",
    "b   aarch64_resched_entry",    // -> ! (never returns here)
    "9:",
    "ldr x9, [sp, #264]",
    "msr spsr_el1, x9",
    "ldr x9, [sp, #256]",
    "msr elr_el1, x9",
    "ldp x0, x1, [sp, #0]",
    "ldp x2, x3, [sp, #16]",
    "ldp x4, x5, [sp, #32]",
    "ldp x6, x7, [sp, #48]",
    "ldr x8, [sp, #64]",
    "ldp x11, x12, [sp, #88]",
    "ldp x13, x14, [sp, #104]",
    "ldp x15, x16, [sp, #120]",
    "ldp x17, x18, [sp, #136]",
    "ldp x19, x20, [sp, #152]",
    "ldp x21, x22, [sp, #168]",
    "ldp x23, x24, [sp, #184]",
    "ldp x25, x26, [sp, #200]",
    "ldp x27, x28, [sp, #216]",
    "ldp x29, x30, [sp, #232]",
    "ldr x9, [sp, #248]",
    "msr sp_el0, x9",             // SP_EL0 exchange back
    "add sp, sp, #(34*8)",
    "ldp x9, x10, [sp], #16",     // restore user x9/x10 from the pocket
    "eret",
    ".endm",

    // Diagnostic flavor for the never-legal / non-deliverable slots:
    // capture the architectural state and hand it to the Rust reporter,
    // which never returns.
    ".macro DIAGBODY class",
    "mrs x0, esr_el1",
    "mrs x1, far_el1",
    "mrs x2, elr_el1",
    "mrs x3, spsr_el1",
    "mov x4, #\\class",
    "bl  aarch64_trap_diag",
    "1: wfe",
    "b   1b",
    ".endm",

    "el1_sync:",
    "EL1BODY aarch64_kernel_trap_dispatch, 0",
    "el1_irq:",
    "EL1BODY aarch64_kernel_trap_dispatch, 1",
    "el1_fiq:",
    "DIAGBODY 2",
    "el1_serror:",
    "DIAGBODY 3",

    "el0_sync:",
    "EL0BODY aarch64_user_trap_dispatch, 0",
    "el0_irq:",
    "EL0BODY aarch64_user_trap_dispatch, 1",
    "el0_fiq:",
    "DIAGBODY 2",
    "el0_serror:",
    "DIAGBODY 3",

    // exc_bad_mode reports via a synthetic ESR (class field 0xF = unknown)
    // so the printed line distinguishes "wrong mode" from a real exception.
    "exc_bad_mode:",
    "mov x0, #0x00F00000",
    "mov x1, xzr",
    "mov x2, xzr",
    "mov x3, xzr",
    "mov x4, #4",
    "bl  aarch64_trap_diag",
    "1: wfe",
    "b   1b",
}

/// Rust-side diagnostic reporter for the never-legal slots.
///
/// Prints the captured state over the early console and halts the CPU.
/// These slots are architecturally unreachable for this kernel (wrong SP
/// selection or wrong execution state); halting is correct there — there
/// is no frame, no scheduler state, and no correct resume target.
#[unsafe(no_mangle)]
extern "C" fn aarch64_trap_diag(class: u64, esr: u64, far: u64, elr: u64, spsr: u64) -> ! {
    use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole};
    Console::write_str("aarch64 trap class=");
    Console::write_hex(class);
    Console::write_str(" esr=");
    Console::write_hex(esr);
    Console::write_str(" far=");
    Console::write_hex(far);
    Console::write_str(" elr=");
    Console::write_hex(elr);
    Console::write_str(" spsr=");
    Console::write_hex(spsr);
    Console::write_str(" — halted\n");
    loop {
        // SAFETY: privileged halt instruction; the diag reporter never
        // returns by design.
        unsafe {
            core::arch::asm!("wfe");
        }
    }
}

unsafe extern "C" {
    /// The exception vector table — installed by `TrapEntryArch::load()`
    /// via `msr VBAR_EL1`.
    pub static exc_vector_table: u8;
}

/// Table base address (the `load()` operand).
pub fn vector_table_va() -> VirBytes {
    // SAFETY: address-only symbol read; the symbol is defined by the
    // `global_asm!` above (K9 pattern — definition in the same module).
    VirBytes::new(unsafe { &exc_vector_table as *const u8 as usize } as u64)
}

/// SVC/syscall entry address: the lower-EL AArch64 synchronous slot
/// (VBAR_EL1 + 0x400). `TrapEntryArch::configure_syscall` is a documented
/// no-op on ARM64 (SVC shares the exception table), so this is facade
/// metadata, not an MSR target.
pub fn syscall_entry_va() -> VirBytes {
    VirBytes::new(vector_table_va().get() + 0x400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_layout_frozen() {
        // The asm stores gpr slot i at i*8, sp at 31*8, elr at 32*8,
        // spsr at 33*8 (34-slot frame, the build↔run contract).
        let base = core::mem::size_of::<u64>();
        assert_eq!(offset_of!(AArch64TrapFrame, gpr), 0);
        assert_eq!(offset_of!(AArch64TrapFrame, sp), 31 * base);
        assert_eq!(offset_of!(AArch64TrapFrame, elr), 32 * base);
        assert_eq!(offset_of!(AArch64TrapFrame, spsr), 33 * base);
        assert_eq!(core::mem::size_of::<AArch64TrapFrame>(), 34 * base);
    }

    #[test]
    fn test_spsr_m_reads_interrupted_el() {
        // SPSR_EL1.M bits [4:0]: 0b00000 = EL0 (user), 0b00101 =
        // EL1h (kernel). The kernel bodies classify origin with this.
        let mut f = AArch64TrapFrame {
            gpr: [0; 31],
            sp: 0,
            elr: 0,
            spsr: 0,
        };
        assert!(f.from_user());
        f.spsr = 0x5; // EL1h
        assert!(!f.from_user());
    }

    #[test]
    fn test_svc_entry_is_lower_el_sync_slot() {
        // The SVC entry is the lower-EL AArch64 synchronous slot at
        // VBAR + 0x400 (ARMv8-A vector layout — offsets are origin
        // bits, not encoded fields).
        let table = vector_table_va();
        let svc = syscall_entry_va();
        assert_eq!(svc.get() - table.get(), 0x400);
    }
}
