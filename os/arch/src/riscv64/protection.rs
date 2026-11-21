//! RISC-V 64-bit (Sv39) protection mechanism implementation
//!
//! Implements `ProtectionArch` for RISC-V 64-bit. RISC-V uses a simpler
//! privilege model than x86 — there are no segment descriptors (GDT) or
//! descriptor tables (IDT). Privilege levels are managed by the CPU's
//! mode bits (S-mode/U-mode) and the sscratch CSR.
//!
//! # RISC-V privilege model
//!
//! - U-mode: User mode (unprivileged)
//! - S-mode: Supervisor mode (kernel)
//! - M-mode: Machine mode (not used by Minix-RS after boot)
//!
//! On trap entry (ecall/external interrupt/exception), the CPU:
//! 1. Sets pc = stvec (supervisor trap vector base)
//! 2. Saves pc to sepc (supervisor exception program counter)
//! 3. Saves privilege mode to sstatus.SPP
//! 4. Sets privilege mode to S-mode
//! 5. Disables interrupts (sstatus.SPIE = sstatus.SIE, SIE = 0)
//!
//! The kernel is responsible for swapping sp with sscratch on entry
//! and restoring it on exit (sret). This is done in the trap handler
//! assembly code, not by hardware.
//!
//! Note: Minix3 does not have a RISC-V port. The semantics are derived
//! from the RISC-V Privileged Specification and the same higher-half
//! principle used by x86/ARM.
//!
//! # OS-level concerns → RISC-V mechanism mapping
//!
//! The `ProtectionArch` trait exposes OS-level concerns; this file maps
//! each to its concrete RISC-V implementation:
//!
//! | OS concern (trait method) | RISC-V mechanism (this file's impl) |
//! |---------------------------|--------------------------------------|
//! | `init` (establish protection) | `csrw sscratch, ...` (kernel stack swap register) — no other setup needed (sstatus.SUM is set later) |
//! | `set_kernel_stack` (switch kernel stack) | Same `csrw sscratch, ...` — single register write |
//! | `load` (make protection effective) | **No-op** — CSRs take effect immediately on write (no separate load step like x86 lgdt) |
//! | `init_ap` (AP startup) | Same `csrw sscratch, ...` — AP-specific sscratch value (each CPU has its own) |
//!
//! # Why RISC-V uses sscratch (software stack switch) vs x86 TSS (hardware)
//!
//! | Approach | x86-64 (TSS) | RISC-V (sscratch) |
//! |----------|---------------|---------------------|
//! | Stack switch trigger | Hardware (CPU reads TSS.sp0 on ring transition) | Software (handler's first instruction: `csrrw sp, sscratch, sp`) |
//! | Overhead per trap | 0 cycles (hardware) | 2-3 cycles (CSR swap) |
//! | Flexibility | Fixed by ISA (sp0 is the only slot) | Programmable (handler can choose to swap or not) |
//!
//! RISC-V chose software swap for ISA simplicity and flexibility. The
//! `csrrw sp, sscratch, sp` instruction is the canonical first
//! instruction of any RISC-V trap handler.

use crate::protection::ProtectionArch;
use minix_types::VirBytes;
use core::arch::asm;

/// Per-CPU kernel-stack base for the trap entry legs (the riscv counterpart
/// of aarch64's `TPIDR_EL1` entry-base scheme).
///
/// The state lives **only** in the per-hart `sscratch` CSR: every hart's
/// `init`/`init_ap` writes its own value, and the scheduler refreshes it
/// through [`TrapReturnArch::prepare_user_return`] before each user return
/// (the park/resched path leaves `sscratch` holding the parked user sp, so a
/// refresh is required). There is deliberately no shared cell: a scalar
/// shared by all harts let an AP's `init_ap` overwrite the BSP's entry stack,
/// and the stack-based `current_cpu_id()` then misattributed the BSP's first
/// kernel call to the AP's idle process.
///
/// [`TrapReturnArch::prepare_user_return`]: crate::arch::trap_return::TrapReturnArch::prepare_user_return
pub(crate) fn set_entry_stack_base(base: VirBytes) {
    debug_assert_eq!(base.get() % 8, 0, "kernel stack top must be 8-byte aligned");
    // SAFETY: CSR write to sscratch in S-mode with a valid kernel VA; each
    // hart writes only its own CSR, so no cross-hart alias exists.
    unsafe {
        asm!("csrw sscratch, {}", in(reg) base.get(), options(nomem, preserves_flags));
    }
}

/// RISC-V 64-bit protection state.
///
/// On RISC-V, the primary protection configuration is:
/// - sscratch: Holds the kernel stack pointer, swapped with sp on trap entry
/// - sstatus: Controls interrupt enable and tracks previous privilege mode
/// - stvec: Trap vector base address (set by TrapEntryArch)
///
/// Unlike x86-64, there is no GDT, IDT, or TSS. The CPU handles
/// privilege transitions based on the trap mechanism defined in the
/// RISC-V Privileged Specification.
pub struct Riscv64Protection;

impl Riscv64Protection {
    /// The post-`cpu_count` protection state is a ZST; this constructor lets
    /// the non-x86 `with_protection` accessor return one `'static` instance
    /// instead of fabricating a throwaway `init(0, 0)`.
    pub const fn new() -> Self {
        Self
    }
}

impl ProtectionArch for Riscv64Protection {
    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self {
        // Per-hart `sscratch` only; there is no shared cell to keep in
        // sync. `cpu_id` is implicit in the CSR each hart writes.
        //
        // Precondition: `kernel_stack_top` is a real stack top, never 0.
        // Writing 0 would poison the live sscratch; the first user trap would
        // swap sp := 0, decrement to 0xFFFF_FFFF_FFFF_FEEF (the frame size),
        // and fault on a non-canonical VA inside the trap entry. The former
        // throwaway caller (`with_protection`'s `init(0, 0)`) is gone.
        assert_ne!(kernel_stack_top.0, 0, "init(0) would poison sscratch");
        set_entry_stack_base(kernel_stack_top);

        Self
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, stack_top: VirBytes) {
        // A 0 argument is never a stack — this entry has no throwaway
        // caller, so a 0 here is a wiring bug (loud stop instead of silently
        // poisoning sscratch, in release too).
        assert_ne!(stack_top.0, 0, "set_kernel_stack(0) would poison sscratch");
        // Contract: per-CPU entry state only. `_cpu_id` is implicit because
        // the write targets the calling hart's own `sscratch`; an
        // implementation must not introduce a shared scalar here.
        set_entry_stack_base(stack_top);
    }

    fn load(&self) {
        // On RISC-V, sscratch is already set by init().
        // stvec is set by TrapEntryArch::load().
        // There is no separate "load" operation for protection
        // structures — CSRs take effect immediately on write.
    }

    fn init_ap(&self, _cpu_id: u32, kernel_stack_top: VirBytes) {
        // Park this AP's own kernel-stack top in its `sscratch` (per-hart
        // CSR — `init_ap` runs on the AP). No shared cell: the calling hart
        // is the index.
        assert_ne!(kernel_stack_top.0, 0, "init_ap(0) would poison sscratch");
        set_entry_stack_base(kernel_stack_top);
    }
}
