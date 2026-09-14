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
pub fn qemu_exit(_status: u32) -> ! {
    let code = ADP_STOPPED_APPLICATION_EXIT;
    // SAFETY: semihosting HLT call — terminates the test VM by design; only
    // valid when QEMU runs with semihosting enabled.
    unsafe {
        core::arch::asm!(
            "ldr x1, ={code}",
            "str x1, [sp, #-16]!",
            "ldr x0, ={op}",
            "hlt #0xF000",
            code = const SEMIHOST_SYS_EXIT,
            op = in(reg) ADP_STOPPED_APPLICATION_EXIT,
            inout("x1") code,
            options(noreturn, nostack)
        );
    }
}
