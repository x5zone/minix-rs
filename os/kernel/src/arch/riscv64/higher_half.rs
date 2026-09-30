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
        // The inline asm performs the higher-half transition.
        //
        // NK4-C 续-101/102 fix (root cause §1.120续-100): under the default
        // `riscv64gc-unknown-none-elf` medany code model, `la` expands to a
        // PC-relative `auipc + addi`. Because this boot code still executes at
        // the identity (low) load address, `la t0, {kmain}` resolved to kmain's
        // LOW physical address — so the kernel never relocated to its high-half
        // VMA, while `map_kernel` maps the kernel into per-process roots only at
        // the high-half VMA. Switching satp to a process root then faulted the
        // next low-PC kernel fetch (riscv RS birth-chain death).
        //
        // Fix: add the constant image link↔load shift
        // `delta = kern_virt_base - kern_phys_base` (the whole image is loaded
        // at PA = VMA - delta, so kmain_VMA = kmain_runtime_low + delta) to the
        // `la`-derived address, landing PC at kmain's true high-half VMA — the
        // same invariant x86_64/aarch64 rely on, making `map_kernel`'s high-half
        // proc-root mapping correct. Verified on real hardware: PC now reads
        // 0xffffffc0... (was 0x8020...).
        //   mv sp, stack_top        // switch stack to high address
        //   and sp, sp, -16         // align to 16 bytes
        //   li s0, 0                // zero frame pointer
        //   fence.i                 // synchronize I-cache with writes
        //   la t1, {kmain}          // PC-relative -> kmain's low runtime addr
        //   add t1, t1, t2          // t2 = delta = (virt_base - phys_base)
        //   jalr x0, t1, 0          // PC <- t1 (high-half VMA; rd=x0 discards)
        //
        // IMPORTANT: Do NOT use a0/a7/a6 inside this asm block!
        // a0 holds kinfo (passed via in("a0") constraint), and must
        // be preserved until kmain is reached. SBI ecall clobbers a0.
        let delta = kinfo.kern_virt_base().0 - kinfo.kern_phys_base().0;
        unsafe {
            core::arch::asm!(
                "mv sp, {stktop}",
                "li t0, -16",
                "and sp, sp, t0",
                "li s0, 0",
                "fence.i",
                "la t1, {kmain}",
                "add t1, t1, t2",
                "jalr x0, t1, 0",
                stktop = in(reg) stack_top.0,
                in("t2") delta,
                kmain = sym kmain,
                in("a0") kinfo,
                options(noreturn)
            );
        }
    }
}
