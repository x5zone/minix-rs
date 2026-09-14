//! QEMU test shutdown backend — sifive_test finisher (S-11 §3.8).
//!
//! QEMU `virt` maps the `sifive_test` device at 0x100000: a 32-bit write of
//! `FINISHER_PASS` (0x5555) exits with code 0; `FINISHER_FAIL` (0x3333)
//! exits with code `(value >> 16) & 1`... (low bits carry a code). Real-
//! hardware backend (SBI SRST) is a separate later lane (§3.8).

const SIFIVE_TEST_BASE: usize = 0x100000;
/// FINISHER_PASS — exit QEMU with code 0.
const FINISHER_PASS: u32 = 0x5555;
/// FINISHER_FAIL — exit QEMU with a failing code.
const FINISHER_FAIL: u32 = 0x3333;

/// Terminate QEMU via the sifive_test finisher (exit code 0 on pass).
pub fn qemu_exit(status: u32) -> ! {
    let word = if status == 0 { FINISHER_PASS } else { FINISHER_FAIL | ((status & 0x7FFF) << 16) };
    // SAFETY: write to the sifive_test finisher; terminates the test VM by
    // design (QEMU virt maps the device unconditionally).
    unsafe {
        core::ptr::write_volatile(SIFIVE_TEST_BASE as *mut u32, word);
    }
    loop {
        core::hint::spin_loop();
    }
}
