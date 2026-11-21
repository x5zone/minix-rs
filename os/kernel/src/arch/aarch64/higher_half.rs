//! AArch64 higher-half transition implementation.
//!
//! Uses inline assembly to:
//! 1. Load `stack_top` (high address) into SP
//! 2. Align stack to 16 bytes (AAPCS64)
//! 3. Branch to `kmain` at its high virtual address
//!
//! The `kinfo` pointer is passed in X0 (AAPCS64 first argument).
//!
//! Corresponds to Minix3 earm/head.S:34-40.
//! See 02-higher-half-kernel.md §4.3 for design rationale.

use crate::boot::higher_half::HigherHalf;
use crate::kmain;
use minix_boot::KernelInfo;
use minix_types::VirBytes;

/// AArch64 higher-half transition.
///
/// Switches SP to the high-address kernel stack and branches to kmain.
/// The kinfo pointer is already in X0 from the calling convention.
pub struct AArch64HigherHalf;

impl HigherHalf for AArch64HigherHalf {
    unsafe fn jump_to_kmain(kinfo: &KernelInfo) -> ! {
        // The stack top is derived from the same `kinfo` the jump hands to
        // `kmain`; keeping it out of the signature removes the second source
        // that could disagree with the image span.
        let stack_top = kinfo.kern_stack_top();
        // Production-path ABI check (R02-T04): AAPCS64 requires SP to be
        // 16-byte aligned at kmain entry; the asm's `and` keeps it aligned,
        // and this asserts the value handed in.
        assert_eq!(
            stack_top.0 & 0xF,
            0,
            "jump_to_kmain: aarch64 kern_stack_top must be 16-byte aligned"
        );
        // SAFETY: Caller guarantees paging is enabled with both identity
        // and kernel high mappings, and that `kinfo` is valid and accessible
        // at a high address. The stack top derived from `kinfo` is asserted
        // 16-byte-aligned by boot validation. This is called exactly once
        // from the boot CPU.
        //
        // The inline asm performs the same sequence as Minix3 head.S:
        //   mov sp, stack_top       // switch stack to high address
        //   and sp, sp, #-16        // align to 16 bytes (AAPCS64)
        //   mov x29, #0             // zero frame pointer
        //   br kmain                // branch to high address
        //
        // Note: AArch64 does not allow SP as a destination for AND,
        // so we use X2 as a temporary register.
        unsafe {
            core::arch::asm!(
                "mov sp, {stktop}",
                "mov x1, #-16",
                "mov x2, sp",
                "and x2, x2, x1",
                "mov sp, x2",
                "mov x29, #0",
                "isb",                      // defensive barrier; enable() already synchronized TTBR/SCTLR
                "ldr x1, ={kmain}",
                "br x1",
                stktop = in(reg) stack_top.0,
                kmain = sym kmain,
                in("x0") kinfo,
                options(noreturn)
            );
        }
    }
}
