//! x86-64 higher-half transition implementation.
//!
//! Uses inline assembly to:
//! 1. Load `stack_top` (high address) into RSP
//! 2. Align stack to 16 bytes (System V ABI)
//! 3. Call `kmain` at its high virtual address
//!
//! The `kinfo` pointer is passed in RDI (System V ABI first argument).
//!
//! Corresponds to Minix3 head.S:78-87.
//! See 02-higher-half-kernel.md §4.3 for design rationale.

use crate::boot::higher_half::HigherHalf;
use crate::kmain;
use minix_boot::KernelInfo;
use minix_types::VirBytes;

/// x86-64 higher-half transition.
///
/// Switches RSP to the high-address kernel stack and calls kmain.
/// The kinfo pointer is already in RDI from the calling convention.
pub struct X86_64HigherHalf;

impl HigherHalf for X86_64HigherHalf {
    unsafe fn jump_to_kmain(kinfo: &KernelInfo) -> ! {
        // The stack top is derived from the same `kinfo` the jump hands to
        // `kmain`; keeping it out of the signature removes the second source
        // that could disagree with the image span.
        let stack_top = kinfo.kern_stack_top();
        // Production-path ABI check (R02-T04): the qemu_test carrier checks
        // the entry SP after the jump; this is the same invariant on the
        // production path, before the asm runs. stack_top is 16-aligned and
        // the asm's `and`/`push`/`sub` sequence makes entry RSP 8 mod 16.
        assert_eq!(
            stack_top.0 & 0xF,
            0,
            "jump_to_kmain: kern_stack_top must be 16-byte aligned"
        );
        assert_eq!(
            stack_top.0.wrapping_sub(24) & 0xF,
            8,
            "jump_to_kmain: x86-64 kmain entry RSP must be 8 mod 16"
        );
        // SAFETY: Caller guarantees paging is enabled with both identity
        // and kernel high mappings, and that `kinfo` is valid and accessible
        // at a high address. The stack top derived from `kinfo` is asserted
        // 16-byte-aligned by boot validation. This is called exactly once
        // from the boot CPU.
        //
        // The inline asm performs the same sequence as Minix3 head.S, plus
        // the x86-64 psABI adjustment:
        //   mov rsp, stack_top     // switch stack to high address
        //   and rsp, -16           // align to 16 bytes
        //   xor rbp, rbp           // zero frame pointer
        //   push 0                 // terminate the initial stack
        //   sub rsp, 8             // psABI: call must see RSP 16-aligned
        //   call kmain             // entry RSP = stack_top-24 (8 mod 16)
        //
        // Production `kmain` is an ordinary Rust function (the naked entry
        // exists only under `qemu_test`), so the System V entry rule
        // `(RSP + 8) % 16 == 0` must hold at the call target.
        unsafe {
            core::arch::asm!(
                "mov rsp, {stktop}",
                "and rsp, -16",
                "xor rbp, rbp",
                "push 0",
                "sub rsp, 8",
                "call {kmain}",
                stktop = in(reg) stack_top.0,
                kmain = sym kmain,
                in("rdi") kinfo,
                options(noreturn)
            );
        }
    }
}
