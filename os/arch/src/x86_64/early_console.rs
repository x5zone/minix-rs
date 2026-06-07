//! Early boot console — COM1 serial port (x86_64).
//!
//! Provides minimal serial output for boot-stage diagnostics.
//! Used by both the real kernel and test kernels.

use core::arch::asm;

const COM1: u16 = 0x3F8;

/// Write a single byte to COM1, waiting for the transmit buffer to be ready.
pub fn write_byte(byte: u8) {
    unsafe {
        while (inb(COM1 + 5) & 0x20) == 0 {}
        outb(COM1, byte);
    }
}

/// Write a string to COM1, translating `\n` to `\r\n`.
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

unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val);
}

unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    asm!("in al, dx", out("al") val, in("dx") port);
    val
}
