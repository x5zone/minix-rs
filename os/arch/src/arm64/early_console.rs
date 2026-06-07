//! Early boot console — PL011 UART (aarch64).
//!
//! Provides minimal serial output for boot-stage diagnostics.
//! Used by both the real kernel and test kernels.

const PL011_BASE: u64 = 0x0900_0000;

/// Write a single byte to PL011 UART.
pub fn write_byte(byte: u8) {
    unsafe {
        core::ptr::write_volatile(PL011_BASE as *mut u8, byte);
    }
}

/// Write a string to PL011 UART, translating `\n` to `\r\n`.
pub fn write_str(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            write_byte(b'\r');
        }
        write_byte(b);
    }
}

/// Write a u64 value in hexadecimal format (0x-prefixed, 16 digits).
pub fn write_hex(val: u64) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    write_str("0x");
    for i in (0..16).rev() {
        write_byte(HEX[((val >> (i * 4)) & 0xf) as usize]);
    }
}
