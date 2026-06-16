//! Early boot console — SBI ecall (riscv64).
//!
//! Provides minimal serial output for boot-stage diagnostics.
//! Uses OpenSBI's console_putchar ecall.

use core::arch::asm;
use crate::early_console::EarlyConsole;

const SBI_CONSOLE_PUTCHAR: usize = 1;

fn sbi_ecall(eid: usize, fid: usize, arg0: usize) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            inlateout("a0") arg0 => ret,
            out("a1") _,
            out("a2") _,
            out("a3") _,
            out("a4") _,
            out("a5") _,
            options(nostack),
        );
    }
    ret
}

/// Write a single byte via SBI console_putchar.
pub fn write_byte(byte: u8) {
    sbi_ecall(SBI_CONSOLE_PUTCHAR, 0, byte as usize);
}

/// Write a string via SBI, translating `\n` to `\r\n`.
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

/// Zero-sized type implementing [`EarlyConsole`] for riscv64.
pub struct Riscv64EarlyConsole;

impl EarlyConsole for Riscv64EarlyConsole {
    fn write_byte(byte: u8) {
        write_byte(byte);
    }
}
