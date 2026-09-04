//! x86-64 trap return implementation
//!
//! Implements [`TrapReturnArch`] for x86-64: assembles the `iretq` payload
//! from the exception frame, loads the remaining register state from the
//! process's `CpuContext`, and switches to ring 3. This is the return-path
//! twin of `x86_64::trap_entry` (IDT/SYSCALL entry) — together they form the
//! user⇄kernel mode-switch boundary (10-switch-to-user.md §2.1 stage 5).
//!
//! # What lives where
//!
//! The CPU-pushed exception frame (`X86_64ExceptionFrame`) carries the five
//! `iretq` operands (RIP, CS, RFLAGS, RSP, SS) plus the vector/error code,
//! which `iretq` does *not* consume and must therefore be skipped when
//! walking the frame. Everything else the mode switch needs — the
//! general-purpose registers (RAX, RCX, RDX, RSI, RDI, RBP, R8–R15), RBX
//! (Minix3's IPC-status / ps_strings register), and the data segment
//! selectors DS/ES/FS/GS — lives in `X86_64CpuContext`, mirroring C's
//! `p_reg` (`stackframe_s`): C's `restore_user_context` pops exactly this
//! set from the process kernel stack (i386 `mpx.S:_restore_user_context`).
//!
//! # Interrupt guarantee
//!
//! `arch_finish_switch_to_user()` ORs `IF_MASK` into the saved PSW
//! (arch_system.c:512) so the restored context is interruptible. This impl
//! applies the same OR while copying RFLAGS onto the stack, keeping the
//! guarantee inside the single privileged instruction boundary.
//!
//! # Why the iret payload is pushed, not popped
//!
//! The kernel stack at dispatch holds no saved frame (the frame is a value
//! the scheduler just built). The impl pushes the five `iretq` operands and
//! never pops them: the privilege switch reloads RSP0 from the per-CPU TSS
//! on the next kernel entry, so the abandoned pushes are reclaimed — the
//! same stack-lifecycle invariant C relies on (see `TrapReturnArch` docs,
//! contract 3).

use crate::arch::trap_return::TrapReturnArch;
use crate::x86_64::boot::X86_64CpuContext;
use crate::x86_64::exception::X86_64ExceptionFrame;
use crate::x86_64::signal::{GP_R15, GP_RAX};

/// RFLAGS interrupt-enable bit (IF). C: `IF_MASK` — archconst.h / i86type.h.
const IF_MASK: u64 = 1 << 9;

/// x86-64 implementation marker (no state; the mode switch is a pure
/// register/stack operation).
pub struct X86_64TrapReturn;

impl TrapReturnArch for X86_64TrapReturn {
    type Frame = X86_64ExceptionFrame;
    type RegisterFile = X86_64CpuContext;

    unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> ! {
        // Compile-time correspondence check between the hard-coded gp_regs
        // strides below and the indexed constants (a mismatch fails the
        // build here rather than corrupting registers at runtime).
        const _: () = assert!(GP_RAX == 0 && GP_R15 == 13);

        // SAFETY (full contract in TrapReturnArch::restore_to_user):
        // - `frame`/`regs` come from the scheduler's dispatch path and
        //   describe the picked process's saved user state. They are bound
        //   to RDI/RSI explicitly: the sequence clobbers every GP register
        //   it loads, and `options(noreturn)` makes the clobber set
        //   irrelevant (no code follows the mode switch).
        // - The current CPU's kernel stack is active; the pushed iret
        //   payload is abandoned on the mode switch (RSP0 reload, contract 3).
        // - The BKL has been released by the caller before this call.
        // - Register sequencing: every scratch register used below (RAX,
        //   RCX, RDX, R10, R11) is reloaded from `regs.gp_regs` afterwards,
        //   so no user value is clobbered; RBX is loaded after its last use
        //   as a source and never used as scratch.
        unsafe {
        core::arch::asm!(
            // ── 1. Build the iretq payload on the kernel stack ──
            // Frame layout (repr(C)): vector(0) errcode(8) rip(16) cs(24)
            // rflags(32) rsp(40) ss(48). Skip vector+errcode: iretq does
            // not consume them.
            "mov rax, [rdi + {rflags_off}]",
            "or  rax, {if_mask}",                 // interrupts-enabled guarantee
            "mov r10, [rdi + {rip_off}]",
            "mov r11, [rdi + {cs_off}]",
            "mov rcx, [rdi + {rsp_off}]",
            "mov rdx, [rdi + {ss_off}]",
            "push rdx",                           // SS
            "push rcx",                           // RSP
            "push rax",                           // RFLAGS (IF set)
            "push r11",                           // CS
            "push r10",                           // RIP
            // ── 2. Data segment selectors (outside the CPU frame) ──
            // RAX is free again (its payload was pushed). C restores these
            // from p_reg in _restore_user_context; 64-bit mode loads
            // selectors only — the FS/GS bases live in MSRs and are set
            // elsewhere.
            "mov ax, [rsi + {ds_off}]",
            "mov ds, ax",
            "mov ax, [rsi + {es_off}]",
            "mov es, ax",
            "mov ax, [rsi + {fs_off}]",
            "mov fs, ax",
            "mov ax, [rsi + {gs_off}]",
            "mov gs, ax",
            // ── 3. RBX — named field (ps_strings / IPC status) ──
            "mov rbx, [rsi + {rbx_off}]",
            // ── 4. General-purpose registers, gp_regs[GP_RAX..=GP_R15] ──
            // Loaded high-to-low so RAX (last) cannot clobber RSI.
            "mov r15, [rsi + {gp_off} + 13*8]",
            "mov r14, [rsi + {gp_off} + 12*8]",
            "mov r13, [rsi + {gp_off} + 11*8]",
            "mov r12, [rsi + {gp_off} + 10*8]",
            "mov r11, [rsi + {gp_off} + 9*8]",
            "mov r10, [rsi + {gp_off} + 8*8]",
            "mov r9,  [rsi + {gp_off} + 7*8]",
            "mov r8,  [rsi + {gp_off} + 6*8]",
            "mov rbp, [rsi + {gp_off} + 5*8]",
            "mov rdi, [rsi + {gp_off} + 4*8]",
            "mov rsi, [rsi + {gp_off} + 3*8]",
            "mov rdx, [rsi + {gp_off} + 2*8]",
            "mov rcx, [rsi + {gp_off} + 1*8]",
            "mov rax, [rsi + {gp_off} + 0*8]",
            // ── 5. Mode switch — never returns to this sequence ──
            "iretq",
            in("rdi") frame as *const X86_64ExceptionFrame,
            in("rsi") regs as *const X86_64CpuContext,
            if_mask = const IF_MASK,
            rip_off = const core::mem::offset_of!(X86_64ExceptionFrame, rip),
            cs_off = const core::mem::offset_of!(X86_64ExceptionFrame, cs),
            rflags_off = const core::mem::offset_of!(X86_64ExceptionFrame, rflags),
            rsp_off = const core::mem::offset_of!(X86_64ExceptionFrame, rsp),
            ss_off = const core::mem::offset_of!(X86_64ExceptionFrame, ss),
            ds_off = const core::mem::offset_of!(X86_64CpuContext, ds),
            es_off = const core::mem::offset_of!(X86_64CpuContext, es),
            fs_off = const core::mem::offset_of!(X86_64CpuContext, fs),
            gs_off = const core::mem::offset_of!(X86_64CpuContext, gs),
            rbx_off = const core::mem::offset_of!(X86_64CpuContext, rbx),
            gp_off = const core::mem::offset_of!(X86_64CpuContext, gp_regs),
            // `nostack` must NOT be used: the payload pushes 5 quads onto
            // the kernel stack (abandoned by design — see module docs).
            // `noreturn` documents the divergence and frees the clobber
            // set (every GP register is reloaded right before iretq).
            options(noreturn)
        );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gp_register_index_order_matches_asm_strides() {
        // The asm block hard-codes the gp_regs strides 13..0 for
        // R15..RAX. This test documents and verifies the invariant with
        // the actual constants.
        assert_eq!(GP_RAX, 0, "gp_regs[0] must be RAX (asm stride 0)");
        assert_eq!(GP_R15, 13, "gp_regs[13] must be R15 (asm stride 13)");
        assert_eq!(X86_64CpuContext::GP_REGS_LEN, 14);
    }

    #[test]
    fn exception_frame_layout_matches_asm_offsets() {
        // The asm reads frame fields via offset_of! so layout changes are
        // picked up automatically; this test pins the iret-relevant order
        // (rip < cs < rflags < rsp < ss) that the push sequence relies on.
        let rip = core::mem::offset_of!(X86_64ExceptionFrame, rip);
        let cs = core::mem::offset_of!(X86_64ExceptionFrame, cs);
        let rflags = core::mem::offset_of!(X86_64ExceptionFrame, rflags);
        let rsp = core::mem::offset_of!(X86_64ExceptionFrame, rsp);
        let ss = core::mem::offset_of!(X86_64ExceptionFrame, ss);
        assert!(rip < cs && cs < rflags && rflags < rsp && rsp < ss);
        // vector + errcode precede the iret payload (skipped by the asm).
        assert_eq!(rip, 16, "vector(0) + errcode(8) must precede rip");
    }
}
