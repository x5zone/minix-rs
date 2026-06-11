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
    unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> ! {
        // SAFETY: Caller guarantees paging is enabled with both identity
        // and kernel high mappings. kinfo is valid and accessible at high
        // address. stack_top is a valid, 16-byte-aligned high virtual
        // address. This is called exactly once from the boot CPU.
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
                "isb",                      // P1-11: drain write buffer before br
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
