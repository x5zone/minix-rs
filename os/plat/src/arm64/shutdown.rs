//! QEMU test shutdown backend — AArch64 semihosting `SYS_EXIT` (S-11 §3.8).
//!
//! Requires QEMU started with semihosting enabled (`-semihosting`). The
//! A64 semihosting call sequence is `hlt #0xF000` with X0 = operation
//! (0x18 = SYS_EXIT) and X1 = the exit-status block. Real-hardware backend
//! (PSCI SYSTEM_OFF) is a separate later lane (§3.8 two-layer rule).

const SEMIHOST_SYS_EXIT: u64 = 0x18;
/// ADP_Stopped_ApplicationExit — the semihosting exit reason for a normal
/// application exit; QEMU maps it to exit code 0.
const ADP_STOPPED_APPLICATION_EXIT: u64 = 0x20026;

/// Terminate QEMU via semihosting SYS_EXIT (exit code 0).
///
/// A64 semihosting calling convention: X0 = operation (0x18 = SYS_EXIT),
/// X1 = pointer to the two-field parameter block { reason: u64, subcode:
/// u64 }. (The A32 convention of passing the reason directly in R1 does
/// not apply on A64 — the block is mandatory.) The block is pushed on the
/// stack; the call never returns, so nothing pops it. QEMU maps
/// ADP_Stopped_ApplicationExit to guest exit code 0.
pub fn qemu_exit(_status: u32) -> ! {
    unsafe {
        core::arch::asm!(
            "ldr x0, ={op}",
            "ldr x2, ={reason}",
            "mov x3, xzr",
            "stp x2, x3, [sp, #-16]!",
            "mov x1, sp",
            "hlt #0xF000",
            op = const SEMIHOST_SYS_EXIT,
            reason = const ADP_STOPPED_APPLICATION_EXIT,
            options(noreturn)
        );
    }
}
