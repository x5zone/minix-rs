//! QEMU test shutdown backend — isa-debug-exit (S-11, §3.8).
//!
//! `isa-debug-exit` is a QEMU ISA device: an I/O write to its base port
//! (0x501 by default) terminates QEMU with exit code `(value << 1) | 1`.
//! This is the x86-64 QEMU **test** backend — the real-hardware backend
//! (ACPI S5 sleep) is a separate, later lane (§3.8 two-layer rule: the
//! shutdown SEMANTIC and the QEMU exit MECHANISM must not be conflated).
//!
//! C: there is no C counterpart — C Minix3's `minix_shutdown` prints and
//! halts; the QEMU-exit mechanism is this port's bring-up instrumentation.

/// isa-debug-exit base port (QEMU default).
const ISA_DEBUG_EXIT_PORT: u16 = 0x501;

/// Terminate QEMU with the mapped exit code for `status`.
///
/// Exit code = `(status << 1) | 1` (isa-debug-exit encoding — always odd).
/// `status == 0` (success) maps to exit code 1; run_qemu special-cases the
/// shutdown test by its serial marker.
pub fn qemu_exit(status: u32) -> ! {
    let value = (status & 0xFF) as u8;
    // SAFETY: ISA port write to the isa-debug-exit device; terminates the
    // test VM by design.
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") ISA_DEBUG_EXIT_PORT,
            in("al") value,
            options(nostack, nomem)
        );
    }
    // QEMU terminates inside the OUT; unreachable — but keep a defined tail
    // for the compiler (the port write never returns control on QEMU).
    loop {
        core::hint::spin_loop();
    }
}
