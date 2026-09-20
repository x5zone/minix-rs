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
//! returns: the handler's `ret` jumps to whatever the frame carried.
//! init was the first consumer to plant the real address in the `ret`
//! slot (it fills `minix_rt::signals::__sigreturn` — NS11); the
//! [`sigaction`] face below now supplies it for every caller.
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

use minix_sys::Errno;
use minix_sys::ipc::{DirectTrapTransport, IpcTransport};
use minix_sys::pm::{SigActionWire, sigaction_via, sigsuspend_via};

/// Installs or queries a signal disposition over an explicit transport —
/// C `sigaction` (`minix3/minix/lib/libc/sys/sigaction.c:11-22`): the
/// one-call face C programs get because their libc holds both the
/// wrapper and `__sigreturn`. This module is the minix-rs counterpart of
/// that unit, so the `ret` slot is filled here, with this module's
/// trampoline address (NS11's init did the same by hand; this face is
/// the shared form). The trampoline only exists on x86_64 — on the other
/// architectures there is no correct `ret` value until their delivery
/// legs land, so this face is x86_64-only with the module gate.
pub fn sigaction_with<T: IpcTransport>(
    transport: &T,
    sig: i32,
    act: Option<&SigActionWire>,
    oact: Option<&mut SigActionWire>,
) -> Result<(), Errno> {
    sigaction_via(transport, sig, act, oact, __sigreturn as *const () as u64)
}

/// Installs or queries a signal disposition (C: `sigaction`) over the
/// direct transport. See [`sigaction_with`] for the wire contract.
pub fn sigaction(
    sig: i32,
    act: Option<&SigActionWire>,
    oact: Option<&mut SigActionWire>,
) -> Result<(), Errno> {
    sigaction_with(&DirectTrapTransport, sig, act, oact)
}

/// Waits for a signal with a temporary mask over an explicit transport —
/// C `sigsuspend` (`minix3/minix/lib/libc/sys/sigsuspend.c:11-17`): the
/// mask is all the wire carries; the handler return path was fixed by
/// the earlier [`sigaction`] call, so no restorer rides this one.
pub fn sigsuspend_with<T: IpcTransport>(transport: &T, set: &[u32; 4]) -> Result<(), Errno> {
    sigsuspend_via(transport, set)
}

/// Waits for a signal with a temporary mask (C: `sigsuspend`) over the
/// direct transport. See [`sigsuspend_with`] for the wire contract.
pub fn sigsuspend(set: &[u32; 4]) -> Result<(), Errno> {
    sigsuspend_with(&DirectTrapTransport, set)
}

#[cfg(test)]
mod tests {
    use super::{__sigreturn, sigaction_with, sigsuspend_with};
    use minix_sys::ipc::CannedTransport;
    use minix_sys::pm::PM_CALL_SIGACTION;
    use minix_types::Message;

    /// Address pin: the trampoline must have a real, nonzero address — the
    /// sigaction consumer hands it to PM as the handler return address, and
    /// a zero slot is exactly the gap this module closes.
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

    /// The `sigaction` face rides the canned transport with the
    /// trampoline's own address in the `ret` lane — the value PM plants
    /// as the handler return address (C sigaction.c:19 `ret` 域).
    #[test]
    fn sigaction_face_carries_trampoline_address() {
        let mut transport = CannedTransport::new();
        let mut reply = Message::zeroed();
        reply.m_type = 0;
        transport.reply_sendrec(Ok(reply));
        let act = minix_sys::pm::SigActionWire {
            sa_handler: 0x2000,
            sa_mask: [0; 4],
            sa_flags: 0,
            _pad: [0; 4],
        };
        assert_eq!(sigaction_with(&transport, 15, Some(&act), None), Ok(()));
        let (destination, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(destination, minix_sys::pm::pm_endpoint());
        assert_eq!(sent.m_type, PM_CALL_SIGACTION);
        // SAFETY: byte-level read of the union lanes (nr@4, act@8, ret@24).
        let raw = unsafe { &sent.m_u.raw };
        let nr = i32::from_ne_bytes(raw[4..8].try_into().unwrap());
        let act_addr = u64::from_ne_bytes(raw[8..16].try_into().unwrap());
        let ret = u64::from_ne_bytes(raw[24..32].try_into().unwrap());
        assert_eq!(nr, 15);
        assert_eq!(act_addr, &act as *const _ as u64);
        assert_eq!(ret, __sigreturn as *const () as u64);
    }

    /// The `sigsuspend` face rides the canned transport mask-only —
    /// C sigsuspend.c:11-17 sends `.set` and nothing else.
    #[test]
    fn sigsuspend_face_carries_mask_only() {
        let mut transport = CannedTransport::new();
        let mut reply = Message::zeroed();
        reply.m_type = 0;
        transport.reply_sendrec(Ok(reply));
        assert_eq!(sigsuspend_with(&transport, &[0, 1, 0, 0]), Ok(()));
        let (destination, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(destination, minix_sys::pm::pm_endpoint());
        assert_eq!(sent.m_type, minix_sys::pm::PM_CALL_SIGSUSPEND);
        // SAFETY: byte-level read of the ctx lane (kept at zero).
        let ctx = unsafe { u64::from_ne_bytes(sent.m_u.raw[8..16].try_into().unwrap()) };
        assert_eq!(ctx, 0);
    }
}
