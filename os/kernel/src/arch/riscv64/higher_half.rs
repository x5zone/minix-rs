//! RISC-V 64-bit (Sv39) higher-half transition implementation.
//!
//! Uses inline assembly to:
//! 1. Load `stack_top` (high address) into SP
//! 2. Align stack to 16 bytes (RISC-V calling convention)
//! 3. Jump to `kmain` at its high virtual address
//!
//! The `kinfo` pointer is passed in A0 (RISC-V ABI first argument).
//!
//! Note: RISC-V does not have a direct Minix3 counterpart for trampoline
//! because Minix3 only supports x86 and ARM. The semantics are derived
//! from the same higher-half principle.
//!
//! See 02-higher-half-kernel.md §4.3 for design rationale.

use crate::boot::higher_half::HigherHalf;
use crate::kmain;
use minix_boot::KernelInfo;
use minix_types::VirBytes;

/// RISC-V 64-bit higher-half transition.
///
/// Switches SP to the high-address kernel stack and jumps to kmain.
/// The kinfo pointer is already in A0 from the calling convention.
pub struct Riscv64HigherHalf;

impl HigherHalf for Riscv64HigherHalf {
    unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> ! {
        // SAFETY: Caller guarantees paging is enabled with both identity
        // and kernel high mappings. kinfo is valid and accessible at high
        // address. stack_top is a valid, 16-byte-aligned high virtual
        // address. This is called exactly once from the boot CPU.
        //
        // The inline asm performs the higher-half transition:
        //   mv sp, stack_top        // switch stack to high address
        //   and sp, sp, -16         // align to 16 bytes
        //   li s0, 0                // zero frame pointer
        //   fence.i                 // P1-11: synchronize I-cache with writes
        //                            // performed during paging setup
        //   la t0, {kmain}          // load kmain address (RISC-V `la` pseudo
        //                            // expands to `auipc + jalr` for external
        //                            // symbols, or `auipc + addi` for PIC)
        //   jalr x0, t0, 0          // PC <- t0 + 0 (rd=x0 discards link)
        //
        // IMPORTANT: Do NOT use a0/a7/a6 inside this asm block!
        // a0 holds kinfo (passed via in("a0") constraint), and must
        // be preserved until kmain is reached. SBI ecall clobbers a0.
        unsafe {
            core::arch::asm!(
                "mv sp, {stktop}",
                "li t0, -16",
                "and sp, sp, t0",
                "li s0, 0",
                "fence.i",
                "la t0, {kmain}",
                "jalr x0, t0, 0",
                stktop = in(reg) stack_top.0,
                kmain = sym kmain,
                in("a0") kinfo,
                options(noreturn)
            );
        }
    }
}
