//! x86-64 trap entry stubs — S-8 (asm trap stub + SYSCALL entry).
//!
//! # Design (one paragraph)
//!
//! Two hardware entry paths are modeled separately (smp_todo.md §3.7), C
//! ground truth mpx.S: **A. IDT/trap-gate path** — exceptions (#DE/#PF/#GP…),
//! external IRQs (PIT → IOAPIC → LAPIC), IPIs and the user-visible soft-int
//! gates (vectors 32-35) all arrive through an IDT gate; a per-vector stub
//! normalizes the error-code slot (the CPU pushes it only for some
//! exceptions — vectors 8, 10-14, 17), pushes the vector number, and jumps
//! to a common stub that saves every GPR into a [`TrapFrame`] and calls the
//! kernel-side dispatcher `x86_trap_dispatch`. **B. SYSCALL/SYSRET path** —
//! not an IDT vector: the LSTAR MSR points at [`syscall_entry_va`], which
//! swaps GS, switches to the per-CPU kernel stack, synthesizes the same
//! [`TrapFrame`] layout, and calls `x86_syscall_dispatch`; the return leg
//! restores the frame and executes `sysretq`.
//!
//! # C parity (mpx.S hwint_master / TEST_INT_IN_KERNEL)
//!
//! C distinguishes an interrupt delivered while running a *process* (full
//! process frame + `switch_to_user`) from one delivered *inside the kernel*
//! (light frame: pusha → handle → EOI → popa → `iret`). This module
//! implements the unified frame for both: every entry builds the same
//! [`TrapFrame`], and the assembly exits with `iretq`/`sysretq` after the
//! Rust dispatcher returns. The *rescheduling* exit (a trap that switches to
//! another process instead of returning) is the `switch_to_user` wiring and
//! arrives with the per-CPU scheduler steps (S-6/S-7) — until then the
//! kernel dispatcher treats user-origin traps as unreachable (no CPL3 code
//! can exist before the scheduler hands out user contexts).
//!
//! # Stage invariant (smp_todo §3.6, v8 #1)
//!
//! After the BSP calls [`install_idt_handlers`] + `TrapEntryArch::load()`
//! there is no empty-vector / empty-handler gate left, and every gate is an
//! interrupt gate (IF auto-cleared on entry), so the kernel runs with IF=0
//! except at explicitly controlled `sti` points. NMI/DF keep their C IST
//! slots only after S-4 programs the per-CPU TSS IST table; `set_handler`
//! writes IST=0, so until S-4 those two run on the interrupted stack (they
//! are never delivered during bring-up: NMI only via QEMU monitor, #DF only
//! after an already-fatal first fault).
//!
//! # GS contract (B path; S-4 programs the MSRs)
//!
//! `x86_syscall_entry` reads the per-CPU kernel stack top from `gs:0x0` and
//! parks the user RSP at `gs:0x8`. S-4 (init_ap / BSP per-CPU init) writes
//! IA32_GS_BASE / IA32_KERNEL_GS_BASE before any entry path can run, and
//! the entry is only reachable via SYSCALL from CPL3, which no code can
//! execute before S-6/S-7 — the dependency order is safe by construction.

use crate::trap_entry::{InterruptVector, TrapEntryArch};
use crate::x86_64::trap_entry::X86_64TrapEntry;
use core::mem::offset_of;
use core::sync::atomic::{AtomicPtr, Ordering};
use minix_types::VirBytes;

// ── Dispatcher registration gate ──
//
// The asm stubs call `x86_trap_dispatch` / `x86_syscall_dispatch`, which are
// defined HERE so the arch crate stays independently linkable (the S-8
// inventory: a direct `call` of a kernel symbol would break every hosted
// test link). The thunks forward to the kernel-registered bodies — the
// policy lives with the BKL/IrqManager owner (kernel crate), the entry
// mechanics live here. Registration happens in the kernel's
// `init_protection`, strictly before `TrapEntryArch::load()`; an unregistered
// dispatch panics instead of silently returning (the S-3d lesson: silent
// misrouting costs ten debugging rounds).
type DispatchFn = unsafe extern "C" fn(&mut TrapFrame);
static TRAP_DISPATCH: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
static SYSCALL_DISPATCH: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register the kernel-side dispatch bodies (policy layer) with the entry
/// stubs (mechanics layer). Must be called before `TrapEntryArch::load()`.
pub fn register_dispatchers(
    trap: DispatchFn,
    syscall: DispatchFn,
) {
    TRAP_DISPATCH.store(trap as *mut (), Ordering::Release);
    SYSCALL_DISPATCH.store(syscall as *mut (), Ordering::Release);
}

/// # Safety
///
/// Forwarded from the asm stub: `frame` must point at a live TrapFrame on
/// the interrupted stack.
#[unsafe(no_mangle)]
unsafe extern "C" fn x86_trap_dispatch(frame: &mut TrapFrame) {
    let f = TRAP_DISPATCH.load(Ordering::Acquire);
    assert!(!f.is_null(), "trap dispatched before register_dispatchers()");
    unsafe { (core::mem::transmute::<*mut (), DispatchFn>(f))(frame) }
}

/// # Safety
///
/// Forwarded from the LSTAR entry: `frame` must point at a live TrapFrame
/// on the per-CPU kernel stack.
#[unsafe(no_mangle)]
unsafe extern "C" fn x86_syscall_dispatch(frame: &mut TrapFrame) {
    let f = SYSCALL_DISPATCH.load(Ordering::Acquire);
    assert!(!f.is_null(), "syscall dispatched before register_dispatchers()");
    unsafe { (core::mem::transmute::<*mut (), DispatchFn>(f))(frame) }
}

/// Full register file saved by both entry paths.
///
/// Memory layout is the build↔run contract with the `global_asm!` below:
/// after the common stub's pushes, RSP points at `rax` and the CPU-pushed
/// tail (`rip`..`ss`) sits above the vector/error-code pair. Pinned by
/// [`test_trap_frame_layout_frozen`].
///
/// `rsp`/`ss` are valid only when the trap came from user mode
/// (`cs & 3 == 3`); for a same-ring (kernel) trap the CPU did not push them
/// and the fields read stack garbage — the same contract as C's frame
/// (doc 14 §1.3: "嵌套异常时无效").
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapFrame {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    /// Vector number pushed by the per-vector stub.
    pub vector: u64,
    /// CPU error code, or the stub's normalized 0.
    pub errcode: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    /// Interrupted stack pointer (valid only for user-origin traps).
    pub rsp: u64,
    /// Interrupted stack segment (valid only for user-origin traps).
    pub ss: u64,
}

// ── Per-CPU GS slot layout (B path; S-4 programs GS_BASE to this struct) ──

/// GS-relative slot holding the per-CPU kernel stack top (S-4 writes it).
pub const GS_SLOT_KERNEL_STACK: u64 = 0x0;
/// GS-relative slot where the entry parks the user RSP.
pub const GS_SLOT_USER_RSP: u64 = 0x8;

// ── Vector → stub table (the frozen build↔run contract) ──

/// Every vector that gets a stub: `(vector, cpu_pushes_error_code)`.
///
/// Coverage rule (asserted by [`test_stub_coverage_matches_idt_gates`]):
/// exactly the gates `X86_64TrapEntry::init()` marks present — exceptions
/// 0-19 minus reserved 9/15, soft-int 32-35 (C: interrupt.h:31-35), the PIC
/// range 0x50-0x57 / 0x70-0x77 (C: VECTOR(irq)), and the LAPIC spurious
/// vector 0xFF (C: apic.c registers a spurious handler).
///
/// Error-code vectors per Intel SDM Vol. 3 §6.3.1 within this set:
/// 8 (#DF), 10 (#TS), 11 (#NP), 12 (#SS), 13 (#GP), 14 (#PF), 17 (#AC).
const STUB_VECTORS: &[(u8, bool)] = &[
    (0, false),
    (1, false),
    (2, false),
    (3, false),
    (4, false),
    (5, false),
    (6, false),
    (7, false),
    (8, true),
    (10, true),
    (11, true),
    (12, true),
    (13, true),
    (14, true),
    (16, false),
    (17, true),
    (18, false),
    (19, false),
    (32, false),
    (33, false),
    (34, false),
    (35, false),
    (0x50, false),
    (0x51, false),
    (0x52, false),
    (0x53, false),
    (0x54, false),
    (0x55, false),
    (0x56, false),
    (0x57, false),
    (0x70, false),
    (0x71, false),
    (0x72, false),
    (0x73, false),
    (0x74, false),
    (0x75, false),
    (0x76, false),
    (0x77, false),
    // IPI: scheduler (C arch_smp.c SCHED_IPI_VECTOR = 0xF0).
    (0xF0, false),
    (0xFF, false),
];

/// Whether the CPU pushes an error code for this exception vector.
pub const fn has_errcode(vector: u8) -> bool {
    match vector {
        8 | 10 | 11 | 12 | 13 | 14 | 17 => true,
        _ => false,
    }
}

/// Whether a stub exists for this vector.
pub const fn has_stub(vector: u8) -> bool {
    let mut i = 0;
    while i < STUB_VECTORS.len() {
        if STUB_VECTORS[i].0 == vector {
            return true;
        }
        i += 1;
    }
    false
}

/// Vectors user mode may raise through a gate (DPL=3): breakpoint, overflow,
/// and the C soft-int syscall/IPC vectors (interrupt.h:31-35).
const fn user_accessible(vector: u8) -> bool {
    matches!(vector, 3 | 4 | 32 | 33 | 34 | 35)
}

/// Map an IDT vector back to its hardware IRQ line number (C: VECTOR(irq)
/// inverted): 0x50+i for i<8, 0x70+(i-8) for 8≤i<16; anything else (all
/// non-IRQ vectors, including LAPIC spurious 0xFF) is not a hardware line.
pub const fn irq_of_vector(vector: u8) -> Option<u8> {
    match vector {
        0x50..=0x57 => Some(vector - 0x50),
        0x70..=0x77 => Some(vector - 0x70 + 8),
        _ => None,
    }
}

core::arch::global_asm!(
    ".section .text.trap_stub, \"ax\"",
    // Per-vector stub: normalize the error-code slot (the CPU pushes it only
    // for vectors 8,10-14,17 — S-8 §3.7) so the common stub always finds the
    // same layout, then push the vector number and jump to the common tail.
    // Byte contract pinned by test_stub_bytes_errcode_normalization.
    ".macro TRAPSTUB vec err",
    "  .globl x86_trap_stub_\\vec",
    "x86_trap_stub_\\vec:",
    "  .if \\err == 0",
    "    push 0",
    "  .endif",
    "  push \\vec",
    "  jmp x86_trap_common",
    ".endm",
    // Exceptions (vectors 9/15 are reserved in long mode — no gate, no stub).
    "TRAPSTUB 0 0",
    "TRAPSTUB 1 0",
    "TRAPSTUB 2 0",
    "TRAPSTUB 3 0",
    "TRAPSTUB 4 0",
    "TRAPSTUB 5 0",
    "TRAPSTUB 6 0",
    "TRAPSTUB 7 0",
    "TRAPSTUB 8 1",
    "TRAPSTUB 10 1",
    "TRAPSTUB 11 1",
    "TRAPSTUB 12 1",
    "TRAPSTUB 13 1",
    "TRAPSTUB 14 1",
    "TRAPSTUB 16 0",
    "TRAPSTUB 17 1",
    "TRAPSTUB 18 0",
    "TRAPSTUB 19 0",
    // Soft-int syscall / IPC gates (C: KERN_CALL_VECTOR_ORIG=32 …).
    "TRAPSTUB 32 0",
    "TRAPSTUB 33 0",
    "TRAPSTUB 34 0",
    "TRAPSTUB 35 0",
    // 8259A-style PIC vector range, reused as the IOAPIC delivery range
    // (C: VECTOR(irq) = 0x50+irq / 0x70+irq-8).
    "TRAPSTUB 80 0",
    "TRAPSTUB 81 0",
    "TRAPSTUB 82 0",
    "TRAPSTUB 83 0",
    "TRAPSTUB 84 0",
    "TRAPSTUB 85 0",
    "TRAPSTUB 86 0",
    "TRAPSTUB 87 0",
    "TRAPSTUB 112 0",
    "TRAPSTUB 113 0",
    "TRAPSTUB 114 0",
    "TRAPSTUB 115 0",
    "TRAPSTUB 116 0",
    "TRAPSTUB 117 0",
    "TRAPSTUB 118 0",
    "TRAPSTUB 119 0",
    // LAPIC spurious interrupt (C: apic.c apic_spurious_interrupt).
    "TRAPSTUB 255 0",
    // Inter-processor interrupt: scheduler (S-10; C: SMP_SCHED_IPI_VECTOR).
    "TRAPSTUB 240 0",
    // Common tail: save every GPR, hand the frame to the kernel dispatcher,
    // restore, drop the (vector, errcode) pair and return with iretq —
    // which pops exactly (rip, cs, rflags[, rsp, ss]) as the CPU frame
    // implies for same-ring and user-origin traps alike.
    ".globl x86_trap_common",
    "x86_trap_common:",
    "  push r15",
    "  push r14",
    "  push r13",
    "  push r12",
    "  push r11",
    "  push r10",
    "  push r9",
    "  push r8",
    "  push rbp",
    "  push rdi",
    "  push rsi",
    "  push rdx",
    "  push rcx",
    "  push rbx",
    "  push rax",
    // Hand the frame to the dispatcher in BOTH first-argument registers:
    // SysV (hosted target) reads RDI, Win64 (x86_64-unknown-uefi!) reads
    // RCX — the codebase builds for both (cf. kmain's naked_asm register
    // rearrangement in kernel/src/lib.rs). RDI/RCX are saved GPRs here;
    // the pops restore them after the call.
    "  mov rdi, rsp",
    "  mov rcx, rsp",
    "  mov rbp, rsp", // keep frame base across the ABI alignment dance
    "  and rsp, -16",
    "  sub rsp, 32",  // Win64 shadow space (SysV callees ignore it)
    "  call x86_trap_dispatch",
    "  mov rsp, rbp",
    "  pop rax",
    "  pop rbx",
    "  pop rcx",
    "  pop rdx",
    "  pop rsi",
    "  pop rdi",
    "  pop rbp",
    "  pop r8",
    "  pop r9",
    "  pop r10",
    "  pop r11",
    "  pop r12",
    "  pop r13",
    "  pop r14",
    "  pop r15",
    "  add rsp, 16", // vector + errcode slots
    "  iretq",
    // B path: LSTAR target. Entry state: CPL0, IF cleared (SFMASK), rcx =
    // user rip, r11 = user rflags. Builds the same TrapFrame on the per-CPU
    // kernel stack, calls x86_syscall_dispatch, and returns with sysretq.
    // rcx/r11 carry rip/rflags back out — the SYSCALL ABI does not preserve
    // them for user code, so reusing them is contract-correct.
    ".globl x86_syscall_entry",
    "x86_syscall_entry:",
    "  swapgs",
    "  mov qword ptr gs:{gs_user_rsp}, rsp",  // park user RSP
    "  mov rsp, gs:{gs_kern_stack}",          // per-CPU kernel stack top (S-4)
    "  push qword ptr gs:{gs_user_rsp}",      // frame.rsp (user)
    "  push {user_ss}",                       // frame.ss (flat user data)
    "  push r11",                             // frame.rflags
    "  push {user_cs}",                       // frame.cs
    "  push rcx",                             // frame.rip
    "  push 0",                               // frame.errcode (normalized)
    "  push 32",                              // frame.vector (KERN_CALL_VECTOR)
    "  push r15",
    "  push r14",
    "  push r13",
    "  push r12",
    "  push r11",
    "  push r10",
    "  push r9",
    "  push r8",
    "  push rbp",
    "  push rdi",
    "  push rsi",
    "  push rdx",
    "  push rcx",
    "  push rbx",
    "  push rax",
    "  mov rdi, rsp",
    "  mov rcx, rsp", // Win64 first argument (see the trap-path comment)
    "  mov rbp, rsp",
    "  and rsp, -16",
    "  sub rsp, 32",  // Win64 shadow space
    "  call x86_syscall_dispatch",
    "  mov rsp, rbp",
    "  pop rax", // syscall return value written by the dispatcher
    "  pop rbx",
    "  pop rcx",
    "  pop rdx",
    "  pop rsi",
    "  pop rdi",
    "  pop rbp",
    "  pop r8",
    "  pop r9",
    "  pop r10",
    "  pop r11",
    "  pop r12",
    "  pop r13",
    "  pop r14",
    "  pop r15",
    "  add rsp, 16", // vector + errcode
    "  pop rcx",     // rip for sysret (user rcx is not preserved — ABI)
    "  add rsp, 8",  // cs slot (sysret takes CS from STAR)
    "  pop r11",     // rflags for sysret (user r11 is not preserved — ABI)
    "  pop rsp",     // back on the user stack
    "  swapgs",
    "  sysretq",
    gs_kern_stack = const GS_SLOT_KERNEL_STACK,
    gs_user_rsp = const GS_SLOT_USER_RSP,
    user_cs = const crate::x86_64::protection::USER_CS_SELECTOR,
    user_ss = const crate::x86_64::protection::USER_DS_SELECTOR,
);

// Foreign symbols emitted by the TRAPSTUB instantiations above. The
// instantiation table mirrors STUB_VECTORS exactly; the coverage test
// fails if the two ever drift (macro_rules cannot paste identifiers, so
// the full symbol names are listed here).
macro_rules! stub_symbols {
    ($($name:ident),+) => {
        unsafe extern "C" {
            $(pub static $name: u8;)+
        }
    };
}

stub_symbols!(
    x86_trap_stub_0, x86_trap_stub_1, x86_trap_stub_2, x86_trap_stub_3,
    x86_trap_stub_4, x86_trap_stub_5, x86_trap_stub_6, x86_trap_stub_7,
    x86_trap_stub_8, x86_trap_stub_10, x86_trap_stub_11, x86_trap_stub_12,
    x86_trap_stub_13, x86_trap_stub_14, x86_trap_stub_16, x86_trap_stub_17,
    x86_trap_stub_18, x86_trap_stub_19, x86_trap_stub_32, x86_trap_stub_33,
    x86_trap_stub_34, x86_trap_stub_35, x86_trap_stub_80, x86_trap_stub_81,
    x86_trap_stub_82, x86_trap_stub_83, x86_trap_stub_84, x86_trap_stub_85,
    x86_trap_stub_86, x86_trap_stub_87, x86_trap_stub_112, x86_trap_stub_113,
    x86_trap_stub_114, x86_trap_stub_115, x86_trap_stub_116, x86_trap_stub_117,
    x86_trap_stub_118, x86_trap_stub_119, x86_trap_stub_240, x86_trap_stub_255
);

unsafe extern "C" {
    pub static x86_trap_common: u8;
    pub static x86_syscall_entry: u8;
}

/// Address of the stub for `vector`, or `None` if no stub exists.
pub fn stub_va(vector: u8) -> Option<VirBytes> {
    // SAFETY: address-only symbol reads; each symbol is emitted by the
    // TRAPSTUB instantiation table above.
    let va = match vector {
        0 => unsafe { &x86_trap_stub_0 as *const u8 as usize },
        1 => unsafe { &x86_trap_stub_1 as *const u8 as usize },
        2 => unsafe { &x86_trap_stub_2 as *const u8 as usize },
        3 => unsafe { &x86_trap_stub_3 as *const u8 as usize },
        4 => unsafe { &x86_trap_stub_4 as *const u8 as usize },
        5 => unsafe { &x86_trap_stub_5 as *const u8 as usize },
        6 => unsafe { &x86_trap_stub_6 as *const u8 as usize },
        7 => unsafe { &x86_trap_stub_7 as *const u8 as usize },
        8 => unsafe { &x86_trap_stub_8 as *const u8 as usize },
        10 => unsafe { &x86_trap_stub_10 as *const u8 as usize },
        11 => unsafe { &x86_trap_stub_11 as *const u8 as usize },
        12 => unsafe { &x86_trap_stub_12 as *const u8 as usize },
        13 => unsafe { &x86_trap_stub_13 as *const u8 as usize },
        14 => unsafe { &x86_trap_stub_14 as *const u8 as usize },
        16 => unsafe { &x86_trap_stub_16 as *const u8 as usize },
        17 => unsafe { &x86_trap_stub_17 as *const u8 as usize },
        18 => unsafe { &x86_trap_stub_18 as *const u8 as usize },
        19 => unsafe { &x86_trap_stub_19 as *const u8 as usize },
        32 => unsafe { &x86_trap_stub_32 as *const u8 as usize },
        33 => unsafe { &x86_trap_stub_33 as *const u8 as usize },
        34 => unsafe { &x86_trap_stub_34 as *const u8 as usize },
        35 => unsafe { &x86_trap_stub_35 as *const u8 as usize },
        0x50 => unsafe { &x86_trap_stub_80 as *const u8 as usize },
        0x51 => unsafe { &x86_trap_stub_81 as *const u8 as usize },
        0x52 => unsafe { &x86_trap_stub_82 as *const u8 as usize },
        0x53 => unsafe { &x86_trap_stub_83 as *const u8 as usize },
        0x54 => unsafe { &x86_trap_stub_84 as *const u8 as usize },
        0x55 => unsafe { &x86_trap_stub_85 as *const u8 as usize },
        0x56 => unsafe { &x86_trap_stub_86 as *const u8 as usize },
        0x57 => unsafe { &x86_trap_stub_87 as *const u8 as usize },
        0x70 => unsafe { &x86_trap_stub_112 as *const u8 as usize },
        0x71 => unsafe { &x86_trap_stub_113 as *const u8 as usize },
        0x72 => unsafe { &x86_trap_stub_114 as *const u8 as usize },
        0x73 => unsafe { &x86_trap_stub_115 as *const u8 as usize },
        0x74 => unsafe { &x86_trap_stub_116 as *const u8 as usize },
        0x75 => unsafe { &x86_trap_stub_117 as *const u8 as usize },
        0x76 => unsafe { &x86_trap_stub_118 as *const u8 as usize },
        0x77 => unsafe { &x86_trap_stub_119 as *const u8 as usize },
        0xF0 => unsafe { &x86_trap_stub_240 as *const u8 as usize },
        0xFF => unsafe { &x86_trap_stub_255 as *const u8 as usize },
        _ => return None,
    };
    Some(VirBytes::new(va as u64))
}

/// Address of the LSTAR SYSCALL entry (B path).
/// Persist an interrupted user register file into the per-process saved
/// context (E1 trap bridge, design decision 3: `CpuContext` is the single
/// user-state truth; the scheduler restore path reads it back).
///
/// GP index order matches the trap-return restore sequence (gp[0]=rax …
/// gp[13]=r15). Segment registers ds/es/fs/gs are not in the TrapFrame —
/// every user process runs the same flat selectors in this kernel, so the
/// saved context keeps its boot values (documented simplification; a future
/// TLS-style per-thread selector would extend the stub, not this function).
pub fn save_frame_to_context(frame: &TrapFrame, ctx: &mut super::boot::X86_64CpuContext) {
    ctx.gp_regs[0] = frame.rax;
    ctx.gp_regs[1] = frame.rcx;
    ctx.gp_regs[2] = frame.rdx;
    ctx.gp_regs[3] = frame.rsi;
    ctx.gp_regs[4] = frame.rdi;
    ctx.gp_regs[5] = frame.rbp;
    ctx.gp_regs[6] = frame.r8;
    ctx.gp_regs[7] = frame.r9;
    ctx.gp_regs[8] = frame.r10;
    ctx.gp_regs[9] = frame.r11;
    ctx.gp_regs[10] = frame.r12;
    ctx.gp_regs[11] = frame.r13;
    ctx.gp_regs[12] = frame.r14;
    ctx.gp_regs[13] = frame.r15;
    ctx.rbx = frame.rbx;
    ctx.rip = frame.rip;
    ctx.rsp = frame.rsp;
    ctx.psw = frame.rflags;
    ctx.cs = frame.cs;
    ctx.ss = frame.ss;
}

/// Pull the IPC status register from a saved context into the outgoing
/// trap frame (E1: the stub's iretq must restore the up-to-date RBX).
pub fn sync_status_register_to_frame(
    ctx: &super::boot::X86_64CpuContext,
    frame: &mut TrapFrame,
) {
    frame.rbx = ctx.rbx;
}

/// Read back the saved RAX (E1 slice 2 test seam — kernel-side callers
/// cannot reach the arch-private register file).
pub fn ipc_return_code(ctx: &super::boot::X86_64CpuContext) -> u64 {
    ctx.gp_regs[0]
}

pub fn syscall_entry_va() -> VirBytes {
    // SAFETY: address-only symbol read.
    VirBytes::new(unsafe { &x86_syscall_entry as *const u8 as usize } as u64)
}

/// Install the real stub addresses into every gate the IDT carries.
///
/// Called by the kernel's `init_protection` (C parity: `idt_init()` fills
/// the table with real entry addresses, protect.c:245-268) immediately
/// before `load()`. Gates are written as interrupt gates (IF auto-cleared),
/// which realizes the stage invariant "after S-8 the kernel runs IF=0
/// except at controlled points".
pub fn install_idt_handlers(entry: &mut X86_64TrapEntry) {
    let mut i = 0;
    while i < STUB_VECTORS.len() {
        let (vector, _) = STUB_VECTORS[i];
        let handler = stub_va(vector).expect("STUB_VECTORS entry without a stub symbol");
        entry.set_handler(
            InterruptVector::new(vector),
            handler,
            user_accessible(vector),
        );
        // C gate_table_exceptions parity: NMI and #DF run on dedicated IST
        // stacks (IST1/IST2, programmed into the per-CPU TSS by S-4) — a
        // gate with IST≠0 must not have it flattened by set_handler.
        match vector {
            2 => entry.set_gate_ist(2, 1),
            8 => entry.set_gate_ist(8, 2),
            _ => {}
        }
        i += 1;
    }
}

// ── Per-CPU GS area + SYSCALL MSR reprogramming (S-4 §3.3) ──

/// Per-CPU GS area read by the entry stubs. Layout is the frozen build↔run
/// contract with `x86_syscall_entry` (offsets pinned by
/// `test_gs_area_layout_frozen`); `gs_cpu_id` extends it for per-CPU
/// identity readback (S-4 L2 acceptance).
#[repr(C, align(16))]
pub struct GsArea {
    /// Kernel stack top the SYSCALL entry switches to (GS_SLOT_KERNEL_STACK).
    pub kernel_stack_top: u64,
    /// Scratch slot where the entry parks the user RSP (GS_SLOT_USER_RSP).
    pub user_rsp: u64,
    /// This CPU's logical id (S-4 L2: AP self-identity readback).
    pub cpu_id: u64,
}

const GS_AREA_CPU_ID: u64 = 0x10;

/// One area per CPU slot (arch-owned static: fixed addresses, zero-heap —
/// §3.3 "per-CPU statics" decision). S-6's per-CPU migration may repoint
/// GS_BASE at richer per-CPU blocks; the first three fields' offsets are
/// frozen by the asm.
static mut GS_AREAS: [GsArea; 8] = [const { GsArea {
    kernel_stack_top: 0,
    user_rsp: 0,
    cpu_id: 0,
} }; 8];

/// IA32_GS_BASE / IA32_KERNEL_GS_BASE (SDM Vol. 4 §2.2 — the swapgs pair).
const MSR_IA32_GS_BASE: u32 = 0xC000_0101;
const MSR_IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

/// Program CPU `cpu_id`'s GS area (kernel stack top + identity) and write
/// BOTH GS_BASE MSR variants to its address.
///
/// Runs on the AP itself (per-CPU MSRs are per-CPU state — only this CPU's
/// MSRs are touched). IA32_KERNEL_GS_BASE gets the same pointer until user
/// mode exists (S-6/S-7): `swapgs` has nothing to swap, and the stub reads
/// the kernel view at entry.
pub fn program_gs(cpu_id: u32, kernel_stack_top: VirBytes) {
    // SAFETY: the GS area for this cpu is written before the CPU's own
    // MSRs point at it and before any entry path can run on this CPU
    // (no CPL3 code before S-6/S-7) — single-writer by ordering.
    unsafe {
        let area = &mut GS_AREAS[cpu_id as usize];
        area.kernel_stack_top = kernel_stack_top.get();
        area.user_rsp = 0;
        area.cpu_id = cpu_id as u64;
        let base = core::ptr::addr_of!(*area) as u64;
        crate::x86_64::trap_entry::wrmsr(MSR_IA32_GS_BASE, base);
        crate::x86_64::trap_entry::wrmsr(MSR_IA32_KERNEL_GS_BASE, base);
    }
}

/// Read this CPU's logical id from its GS area (`gs:0x10`).
///
/// S-4 L2 acceptance helper: the AP proves its per-CPU identity by reading
/// back what `program_gs` wrote — the value travels through the CPU's own
/// GS_BASE MSR, so a match means the MSR + area wiring works on that CPU.
pub fn gs_cpu_id() -> u64 {
    let id: u64;
    // SAFETY: GS_BASE points at this CPU's GsArea after program_gs; before
    // that, GS base is 0 and gs_cpu_id is only called from the AP tail that
    // ran program_gs (documented caller contract).
    unsafe { core::arch::asm!("mov {}, gs:[{off}]", out(reg) id, off = const GS_AREA_CPU_ID, options(nomem, nostack)); }
    id
}

/// Write the four SYSCALL MSRs for the CURRENT CPU (STAR/LSTAR/SFMASK +
/// EFER.SCE) — the per-CPU reprogramming half of S-4 §3.3: values the BSP
/// wrote via `configure_syscall` do not propagate to APs, and an AP without
/// them faults on its first `syscall`.
///
/// The OS-facing entry remains `TrapEntryArch::configure_syscall` (BSP);
/// the AP wiring calls this directly.
pub fn write_syscall_msrs(entry_point: VirBytes) {
    // SAFETY: same MSR sequence as configure_syscall — architecturally
    // defined SYSCALL configuration, executed at CPL0.
    use crate::x86_64::trap_entry::{MSR_EFER, MSR_LSTAR, MSR_SFMASK, MSR_STAR, EFER_SCE, SFMASK_CLEAR_IF};
    unsafe {
        let star = (crate::x86_64::protection::KERN_CS_SELECTOR as u64) << 32
                 | (crate::x86_64::protection::USER_CS_SELECTOR as u64) << 48;
        crate::x86_64::trap_entry::wrmsr(MSR_STAR, star);
        crate::x86_64::trap_entry::wrmsr(MSR_LSTAR, entry_point.get());
        crate::x86_64::trap_entry::wrmsr(MSR_SFMASK, SFMASK_CLEAR_IF);
        let efer = crate::x86_64::trap_entry::rdmsr(MSR_EFER);
        crate::x86_64::trap_entry::wrmsr(MSR_EFER, efer | EFER_SCE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gs_area_layout_frozen() {
        // The asm reads gs:0x0 / gs:0x8; gs_cpu_id reads gs:0x10.
        assert_eq!(offset_of!(GsArea, kernel_stack_top), GS_SLOT_KERNEL_STACK as usize);
        assert_eq!(offset_of!(GsArea, user_rsp), GS_SLOT_USER_RSP as usize);
        assert_eq!(offset_of!(GsArea, cpu_id), GS_AREA_CPU_ID as usize);
    }


    #[test]
    fn test_trap_frame_layout_frozen() {
        // The asm pushes in exactly this order (S-8 build↔run contract).
        let base = core::mem::size_of::<u64>();
        assert_eq!(offset_of!(TrapFrame, rax), 0);
        assert_eq!(offset_of!(TrapFrame, r15), 14 * base);
        assert_eq!(offset_of!(TrapFrame, vector), 15 * base);
        assert_eq!(offset_of!(TrapFrame, errcode), 16 * base);
        assert_eq!(offset_of!(TrapFrame, rip), 17 * base);
        assert_eq!(offset_of!(TrapFrame, cs), 18 * base);
        assert_eq!(offset_of!(TrapFrame, rflags), 19 * base);
        assert_eq!(offset_of!(TrapFrame, rsp), 20 * base);
        assert_eq!(offset_of!(TrapFrame, ss), 21 * base);
        assert_eq!(core::mem::size_of::<TrapFrame>(), 22 * base);
    }

    #[test]
    fn test_stub_coverage_matches_idt_gates() {
        // Every present IDT gate must have a stub, and vice versa — this is
        // the "no empty vector / empty handler" stage invariant's compile-
        // time half. Reserved long-mode vectors 9/15 must have neither.
        let entry = X86_64TrapEntry::init();
        for vector in 0..=u8::MAX {
            let present = entry.gate_present(vector);
            assert_eq!(
                present,
                has_stub(vector),
                "vector {vector:#04x}: IDT gate presence and stub coverage diverge"
            );
        }
        assert!(!has_stub(9) && !has_stub(15), "reserved vectors must stay stub-free");
    }

    #[test]
    fn test_errcode_classification_matches_sdm() {
        // Intel SDM Vol. 3 §6.3.1: within the gated set, only 8/10/11/12/
        // 13/14/17 push an error code. The STUB_VECTORS table must agree
        // with has_errcode — the TRAPSTUB instantiation reads the second
        // column, so a drift here is a stack-layout corruption.
        for &(vector, err) in STUB_VECTORS {
            assert_eq!(has_errcode(vector), err, "vector {vector:#04x} errcode flag");
        }
    }

    #[test]
    fn test_stub_bytes_errcode_normalization() {
        // S-3d lesson: absolute byte-level contracts need byte pins. The
        // no-errcode stubs must open with `push 0` (6A 00) before
        // `push <vector>` (6A imm8); the error-code stubs must NOT normalize.
        let stub0 = unsafe {
            core::slice::from_raw_parts(&x86_trap_stub_0 as *const u8, 8)
        };
        assert_eq!(&stub0[..4], &[0x6A, 0x00, 0x6A, 0x00], "stub 0: push 0; push 0");
        let stub13 = unsafe {
            core::slice::from_raw_parts(&x86_trap_stub_13 as *const u8, 8)
        };
        assert_eq!(&stub13[..2], &[0x6A, 0x0D], "stub 13 (errcode): push 13 only");
        // Vectors > 127 must be pushed with the imm32 encoding (68 imm32) —
        // the imm8 form sign-extends and would corrupt frame.vector.
        let stub255 = unsafe {
            core::slice::from_raw_parts(&x86_trap_stub_255 as *const u8, 8)
        };
        assert_eq!(&stub255[..6], &[0x6A, 0x00, 0x68, 0xFF, 0x00, 0x00], "stub 255: push 0; push imm32 0xFF");
    }

    #[test]
    fn test_syscall_entry_opens_with_swapgs() {
        // v3 #11: the first swapgs in the kernel is the S-8 entry stub, and
        // it must be the first instruction at LSTAR.
        let entry = unsafe {
            core::slice::from_raw_parts(&x86_syscall_entry as *const u8, 8)
        };
        assert_eq!(&entry[..3], &[0x0F, 0x01, 0xF8], "swapgs must be first");
    }

    #[test]
    fn test_irq_of_vector_mapping() {
        // C: VECTOR(irq) = 0x50+irq (master range), 0x70+irq-8 (slave).
        assert_eq!(irq_of_vector(0x50), Some(0));
        assert_eq!(irq_of_vector(0x57), Some(7));
        assert_eq!(irq_of_vector(0x70), Some(8));
        assert_eq!(irq_of_vector(0x77), Some(15));
        assert_eq!(irq_of_vector(0), None);
        assert_eq!(irq_of_vector(14), None);
        assert_eq!(irq_of_vector(32), None);
        assert_eq!(irq_of_vector(0xFF), None, "spurious is not a hardware line");
    }

    #[test]
    fn test_user_accessible_gate_set() {
        for v in [3u8, 4, 32, 33, 34, 35] {
            assert!(user_accessible(v), "vector {v} must be user-accessible");
        }
        for v in [0u8, 8, 14, 0x50, 0x70, 0xFF] {
            assert!(!user_accessible(v), "vector {v:#04x} must be kernel-only");
        }
    }
}

#[cfg(test)]
mod save_frame_tests {
    use super::*;

    /// E1 slice 1: the TrapFrame → CpuContext persistence must keep the GP
    /// index order the trap-return restore sequence expects (gp[0]=rax …
    /// gp[13]=r15) and carry the named fields (rbx/rip/rsp/psw/cs/ss) —
    /// a swapped index here silently corrupts the wrong register on
    /// every blocked-then-resumed IPC.
    #[test]
    fn save_frame_to_context_persists_all_fields() {
        let frame = TrapFrame {
            rax: 0x1000, rbx: 0x1001, rcx: 0x1002, rdx: 0x1003,
            rsi: 0x1004, rdi: 0x1005, rbp: 0x1006,
            r8: 0x1007, r9: 0x1008, r10: 0x1009, r11: 0x100a,
            r12: 0x100b, r13: 0x100c, r14: 0x100d, r15: 0x100e,
            vector: 33,
            errcode: 0,
            rip: 0x2000,
            cs: 0x1B,
            rflags: 0x202,
            rsp: 0x7fff_0000,
            ss: 0x23,
        };
        let mut ctx = super::super::boot::X86_64CpuContext::new();
        save_frame_to_context(&frame, &mut ctx);

        assert_eq!(ctx.gp_regs[0], 0x1000); // rax
        assert_eq!(ctx.gp_regs[1], 0x1002); // rcx
        assert_eq!(ctx.gp_regs[2], 0x1003); // rdx
        assert_eq!(ctx.gp_regs[3], 0x1004); // rsi
        assert_eq!(ctx.gp_regs[4], 0x1005); // rdi
        assert_eq!(ctx.gp_regs[5], 0x1006); // rbp
        assert_eq!(ctx.gp_regs[6], 0x1007); // r8
        assert_eq!(ctx.gp_regs[13], 0x100e); // r15
        assert_eq!(ctx.rbx, 0x1001);
        assert_eq!(ctx.rip, 0x2000);
        assert_eq!(ctx.rsp, 0x7fff_0000);
        assert_eq!(ctx.psw, 0x202);
        assert_eq!(ctx.cs, 0x1B);
        assert_eq!(ctx.ss, 0x23);
    }
}
