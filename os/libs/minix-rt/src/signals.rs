//! Signal return trampoline — the C `__sigreturn` counterpart.
//!
//! C anchors: `minix3/minix/lib/libc/arch/i386/sys/__sigreturn.S:7-10`
//! (the trampoline entry repositions the stack and jumps to the `sigreturn`
//! wrapper) and `minix3/minix/lib/libc/sys/sigreturn.c:13-31` (block every
//! signal, then hand the sigcontext address to PM through PM_SIGRETURN).
//!
//! # Why a trampoline exists at all
//!
//! When PM delivers a signal it plants a frame on the user stack whose
//! handler return address is this module's exported address — the `ret`
//! slot of the sigaction request (`m_lc_pm_sig.ret`; PM stores it per
//! process in `sigreturn_addr`, `os/servers/pm/src/mproc/signal.rs:92`).
//! A handler is a plain `extern "C" fn(i32)`: its final `ret` pops that
//! planted address and control lands here. At that moment the interrupted
//! register state lives only in the sigcontext inside the signal frame, so
//! the trampoline is the one piece of code that can hand the context
//! pointer back to PM for restoration. Without it a delivered signal never
//! returns: the handler's `ret` jumps to whatever the frame carried, and
//! the frame consumer that needs the address finds none (the current
//! consumer fills the slot with 0 —
//! `os/commands/sbin/init/src/host.rs:294-295` self-declares that gap).
//!
//! # Frame contract (what PM's delivery arm must plant)
//!
//! Ascending addresses on the user stack at handler entry:
//!
//! | slot                    | content                |
//! |-------------------------|------------------------|
//! | handler return address  | [`__sigreturn`]        |
//! | next slot above it      | `&sigcontext`          |
//!
//! The handler's `ret` consumes the first slot, which leaves the
//! sigcontext pointer at `[rsp]` — exactly where the trampoline reads it.
//! C i386 runs the same handoff with `addl $16, %esp` repositioning the
//! cdecl argument slot (`__sigreturn.S:8`); the x86-64 SysV variant
//! passes arguments in registers, so the trampoline loads `[rsp]` into
//! `rdi` and tail-jumps to the Rust shim instead.
//!
//! The syscall half ([`minix_sys::sigreturn`]) already blocks every signal
//! before the round trip (C sigreturn.c:24-26) and never returns: a
//! successful restoration swaps the saved context in from under the call.
//!
//! The x86-64 leg is the one in place; the aarch64/riscv64 legs land with
//! their delivery arms (C arm counterpart: `__sigreturn.S:7-9`,
//! `b sigreturn`), so the contract above stays single-sourced here.

#![cfg(target_arch = "x86_64")]

/// The handler-return stub whose address goes into the sigaction request's
/// `ret` slot (C: `__sigreturn`, the address libc advertises to PM).
///
/// Naked on purpose: the frame contract (module doc) fixes `[rsp]` at
/// entry, so the stub must not touch the stack before reading it.
#[unsafe(naked)]
pub extern "C" fn __sigreturn() -> ! {
    core::arch::naked_asm!(
        "mov rdi, [rsp]",
        "jmp {body}",
        body = sym __sigreturn_body,
    );
}

/// Rust-side tail of the trampoline: the context pointer arrives in the
/// first argument slot; everything from here on is ordinary code.
extern "C" fn __sigreturn_body(ctx: usize) -> ! {
    minix_sys::sigreturn(ctx as u64)
}

#[cfg(test)]
mod tests {
    use super::__sigreturn;

    /// Address pin: the trampoline must have a real, nonzero address — the
    /// sigaction consumer hands it to PM as the handler return address, and
    /// a zero slot is exactly the gap this module closes (init fills the
    /// `ret` slot with 0 until it adopts this symbol).
    #[test]
    fn trampoline_address_is_nonzero() {
        assert_ne!(__sigreturn as usize, 0);
    }

    /// Body pin: the naked stub opens with real instructions, not a
    /// collapsed or zero-filled symbol — a zero first word would execute
    /// as an unintended memory write the moment a handler returned.
    #[test]
    fn trampoline_entry_holds_code() {
        let first_word = unsafe { core::ptr::read(__sigreturn as usize as *const u32) };
        assert_ne!(first_word, 0);
    }
}
