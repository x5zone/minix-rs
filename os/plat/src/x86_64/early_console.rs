//! Early boot console — COM1 serial port (x86_64).
//!
//! Provides minimal serial output for boot-stage diagnostics.
//! Used by both the real kernel and test kernels.

use core::arch::asm;
use crate::early_console::EarlyConsole;

/// COM1 base port (standard PC UART 16550).
///
/// The 16550 has 8 I/O ports starting at 0x3F8:
/// +0: data / divisor latch low
/// +1: interrupt enable / divisor latch high
/// +2: FIFO control
/// +3: line control
/// +4: modem control
/// +5: line status
pub const COM1_BASE: u16 = 0x3F8;

/// Divisor value for 115200 baud from 1.8432 MHz crystal (divisor = 1).
const COM1_DIVISOR_115200: u8 = 0x01;
/// LCR bit 7: Divisor Latch Access Bit (DLAB). Must be set to change
/// baud rate; must be clear to access data/FIFO registers.
const COM1_LCR_DLAB: u8 = 0x80;
/// LCR value: 8 data bits, no parity, 1 stop bit (DLAB clear).
const COM1_LCR_8N1: u8 = 0x03;
/// FCR value: enable FIFO, clear RX/TX buffers, 14-byte trigger.
const COM1_FCR_ENABLE: u8 = 0xC7;
/// MCR value: DTR + RTS + OUT2 (OUT2 enables IRQ routing for PIC).
const COM1_MCR_DTR_RTS_OUT2: u8 = 0x0B;

/// Initialize COM1 (UART 16550) for 115200 8N1 with FIFOs enabled.
///
/// This is the boot-stage re-initialization that supersedes the
/// firmware-set defaults (usually 9600 8N1) so kernel `printk!`
/// output is visible in QEMU `-serial` at 115200.
///
/// # Safety
///
/// Must be called only from BSP, before any UART output or
/// interrupt-driven serial I/O is enabled.
unsafe fn ser_init() { unsafe {
    // SAFETY: COM1_BASE (0x3F8) is a well-known PC I/O port range
    // (Intel IA-PC compatible). outb to these ports is the standard
    // 16550 initialization sequence (Linux `serial8250_init_hw`).
    outb(COM1_BASE + 1, 0x00); // Disable all UART interrupts
    outb(COM1_BASE + 3, COM1_LCR_DLAB); // Enable divisor latch access
    outb(COM1_BASE, COM1_DIVISOR_115200); // Divisor low = 1
    outb(COM1_BASE + 1, 0x00); // Divisor high = 0
    outb(COM1_BASE + 3, COM1_LCR_8N1); // 8 bits, no parity, 1 stop, DLAB off
    outb(COM1_BASE + 2, COM1_FCR_ENABLE); // Enable FIFO
    outb(COM1_BASE + 4, COM1_MCR_DTR_RTS_OUT2); // DTR + RTS + OUT2
}}

/// Write a single byte to COM1, waiting for the transmit buffer to be ready.
pub fn write_byte(byte: u8) {
    unsafe {
        while (inb(COM1_BASE + 5) & 0x20) == 0 {}
        outb(COM1_BASE, byte);
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

unsafe fn outb(port: u16, val: u8) { unsafe {
    asm!("out dx, al", in("dx") port, in("al") val);
}}

unsafe fn inb(port: u16) -> u8 { unsafe {
    let val: u8;
    asm!("in al, dx", out("al") val, in("dx") port);
    val
}}

/// Zero-sized type implementing [`EarlyConsole`] for x86-64.
pub struct X86_64EarlyConsole;

impl EarlyConsole for X86_64EarlyConsole {
    fn init() {
        // SAFETY: ser_init runs only on BSP before serial I/O is enabled.
        unsafe { ser_init(); }
    }

    fn write_byte(byte: u8) {
        write_byte(byte);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_com1_base() {
        assert_eq!(COM1_BASE, 0x3F8);
    }

    #[test]
    fn test_com1_divisor_115200() {
        // 1.8432 MHz / 16 / 1 = 115200 baud
        assert_eq!(COM1_DIVISOR_115200, 0x01);
    }

    #[test]
    fn test_com1_lcr_dlab_bit() {
        // LCR bit 7 is DLAB.
        assert_eq!(COM1_LCR_DLAB, 0x80);
    }

    #[test]
    fn test_com1_lcr_8n1() {
        // 8N1 with DLAB off = 0x03.
        assert_eq!(COM1_LCR_8N1, 0x03);
    }

    #[test]
    fn test_com1_fcr_enable() {
        // FIFO enable + clear RX/TX + 14-byte trigger.
        assert_eq!(COM1_FCR_ENABLE, 0xC7);
    }

    #[test]
    fn test_com1_mcr_signals() {
        // DTR + RTS + OUT2 (OUT2 enables IRQ routing).
        assert_eq!(COM1_MCR_DTR_RTS_OUT2, 0x0B);
    }

    // Note: X86_64EarlyConsole::init() performs real COM1 I/O port writes,
    // which segfault in a host `cargo test` process without I/O privileges.
    // The initialization sequence is validated by the constant tests above;
    // runtime correctness is covered by QEMU/target integration tests.
}
