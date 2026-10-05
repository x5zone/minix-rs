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
///
/// Private to this module: external callers go through the [`EarlyConsole`] trait
/// method `write_byte` (or via `write_str` / `write_hex`). Naming this helper
/// `com1_write_byte` (rather than `write_byte`) avoids the name-collision that
/// would otherwise force Rust's name resolver to disambiguate between the trait
/// method and a same-named free function inside the `impl` block.
fn com1_write_byte(byte: u8) {
    // SD-5 / P-X86-02（2026-10-06）：原实现是无界 `while (LSR.THRE==0) {}`
    //——UART 异常（或被模拟器禁写）时内核首条输出即永久挂死。改用共享
    // `tx_wait_then_send`（aarch64 同范式）：有界等 LSR bit5（THRE，0x3FD），
    // 到限放行宁丢字节（与 boot-shim `emit_byte` 的 NK4-A 取证语义一致）。
    const COM1_LSR: u16 = COM1_BASE + 5; // Line Status Register
    const COM1_LSR_THRE: u8 = 0x20; // Transmit Holding Register Empty
    const COM1_TX_POLL_LIMIT: u32 = 100_000;
    crate::early_console::tx_wait_then_send(
        byte,
        || unsafe { inb(COM1_LSR) & COM1_LSR_THRE == 0 },
        |b| unsafe { outb(COM1_BASE, b) },
        COM1_TX_POLL_LIMIT,
    );
}

/// Write a string to COM1, translating `\n` to `\r\n`.
pub fn write_str(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            com1_write_byte(b'\r');
        }
        com1_write_byte(b);
    }
}

/// Write a u64 value in hexadecimal format (0x-prefixed, 16 digits).
pub fn write_hex(val: u64) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    write_str("0x");
    for i in (0..16).rev() {
        com1_write_byte(HEX[((val >> (i * 4)) & 0xf) as usize]);
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
        // Forward to the module-private COM1 helper. The previous implementation
        // called a same-named free function `write_byte`, which compiled only
        // because Rust's name resolver disambiguated in favor of the free
        // function — fragile and surprising. Naming the helper `com1_write_byte`
        // makes the call site explicit and removes the dependency on resolver
        // tie-breaking.
        com1_write_byte(byte);
    }
}

// Note: X86_64EarlyConsole::init() performs real COM1 I/O port writes,
// which segfault in a host `cargo test` process without I/O privileges.
// Constant values (COM1_BASE / COM1_DIVISOR_115200 / COM1_LCR_DLAB /
// COM1_LCR_8N1 / COM1_FCR_ENABLE / COM1_MCR_DTR_RTS_OUT2) are validated
// by their literal declarations above — tautological asserts were
// intentionally removed (Pattern #38: trivial tests). Runtime correctness
// is covered by QEMU/target integration tests.
