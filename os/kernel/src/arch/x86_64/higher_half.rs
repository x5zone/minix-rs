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
    unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> ! {
        // SAFETY: Caller guarantees paging is enabled with both identity
        // and kernel high mappings. kinfo is valid and accessible at high
        // address. stack_top is a valid, 16-byte-aligned high virtual
        // address. This is called exactly once from the boot CPU.
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
