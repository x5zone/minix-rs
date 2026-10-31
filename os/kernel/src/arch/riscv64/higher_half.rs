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
        // Jump-target fix under the medany code model: under the default
        // `riscv64gc-unknown-none-elf` medany code model, `la` expands to a
        // PC-relative `auipc + addi`. Which runtime address it resolves to
        // depends on where this code is *executing from*, and the two v1
        // boot legs differ exactly there:
        //
        // - **Direct leg** (`arch_boot_impl`): the kernel enabled its own
        //   root and still executes below `kern_virt_base` (identity
        //   window), so `la` resolved to kmain's LOW runtime address. The
        //   constant image link↔load shift
        //   `delta = kern_virt_base - kern_phys_base` (the whole image is
        //   loaded at PA = VMA - delta, so kmain_VMA = kmain_runtime_low +
        //   delta) must be added, landing PC at kmain's true high-half VMA —
        //   the same invariant x86_64/aarch64 rely on, making
        //   `map_kernel`'s high-half proc-root mapping correct. Verified on
        //   real hardware: PC now reads 0xffffffc0... (was 0x8020...).
        // - **Resume leg** (`arch_boot_resume_v1`, D-73): the boot-shim's
        //   `enter_kernel_image_v1` already jumped to the receiver's
        //   high-half `_start`, so `la` resolved to kmain's linked VMA
        //   directly. Adding delta again corrupted the target
        //   (0xffffffc00000477a + 0xffffffbf7ffe0000 → instruction page
        //   fault at 0xffffff7f7fe0477a).
        //
        // One jump for both legs: probe the current PC (`auipc t3, 0`) and
        // apply delta only when executing below the linked VMA. The
        // unsigned compare is safe because the higher-half invariant
        // (`kern_virt_base ≫ kern_phys_base`, checked below) puts every
        // identity-window PC strictly below `kern_virt_base`.
        //   mv sp, stack_top        // switch stack to high address
        //   and sp, sp, -16         // align to 16 bytes
        //   li s0, 0                // zero frame pointer
        //   fence.i                 // synchronize I-cache with writes
        //   auipc t3, 0             // t3 = current PC
        //   la t1, {kmain}          // PC-relative -> kmain's runtime addr
        //   bltu t3, t4, 3f         // below linked VMA → identity leg
        //   jalr x0, t1, 0          // already at linked VMA → jump as-is
        //   3: add t1, t1, t2       // t2 = delta = (virt_base - phys_base)
        //   jalr x0, t1, 0          // PC <- t1 (high-half VMA; rd=x0 discards)
        //
        // IMPORTANT: Do NOT use a0/a7/a6 inside this asm block!
        // a0 holds kinfo (passed via in("a0") constraint), and must
        // be preserved until kmain is reached. SBI ecall clobbers a0.
        // Code-review note: `checked_sub` makes the
        // `kern_virt_base >= kern_phys_base` invariant explicit — a violated
        // higher-half layout panics here at boot instead of wrapping to a bogus
        // delta that would mis-target the jump (under the release build
        // `overflow-checks` is off, so a bare subtraction would silently wrap
        // rather than fault — `checked_sub` decouples the guard from the build
        // profile). The invariant always holds for the higher-half image
        // (virt ≫ phys), so this is behavior-preserving.
        // (Declaring the asm's register writes (`sp`, `t0`, `t1`, `t2`, `t3`,
        // and the intentionally-zeroed frame pointer `s0`) as outputs — is not
        // expressible: rustc rejects "asm outputs are not allowed with the
        // `noreturn` option". `options(noreturn)` (jump to kmain, never return
        // to the caller) is itself the correctness argument that those writes
        // are unobservable downstream, so they need no explicit clobber list.)
        let delta = kinfo
            .kern_virt_base()
            .0
            .checked_sub(kinfo.kern_phys_base().0)
            .expect("higher_half: kern_virt_base must be >= kern_phys_base");
        let kvirt = kinfo.kern_virt_base().0;
        unsafe {
            core::arch::asm!(
                "mv sp, {stktop}",
                "li t0, -16",
                "and sp, sp, t0",
                "li s0, 0",
                "fence.i",
                "auipc t3, 0",
                "la t1, {kmain}",
                // t4 carries `kvirt` via the explicit-register operand below
                // (explicit-register operands are not template-referenceable).
                "bltu t3, t4, 3f",
                "jalr x0, t1, 0",
                "3:",
                "add t1, t1, t2",
                "jalr x0, t1, 0",
                stktop = in(reg) stack_top.0,
                in("t2") delta,
                in("t4") kvirt,
                kmain = sym kmain,
                in("a0") kinfo,
                options(noreturn)
            );
        }
    }
}
