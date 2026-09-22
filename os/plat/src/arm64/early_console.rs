//! Early boot console — PL011 UART (aarch64).
//!
//! Provides minimal serial output for boot-stage diagnostics.

use crate::early_console::{tx_wait_then_send, EarlyConsole};

/// PL011 UART0 base on QEMU `virt`（与 `os/qemu-tests/` 的 aarch64 载体一致）。
const PL011_BASE: u64 = 0x0900_0000;
/// UARTFR（Flag Register）偏移 0x18。
const PL011_FR_OFFSET: u64 = 0x18;
/// UARTFR bit5 = TXFF：发送 FIFO 满（PL011 的 FIFO 深度 16 字节）。
const PL011_FR_TXFF: u8 = 1 << 5;
/// 发送前的有界轮询上限。数量级跟 x86 侧对位实现（`os/boot-shim/src/main.rs`
/// 的 `raw_serial_line`，~10 万次即放弃）一致：设备状态位异常时宁丢字节。
const PL011_TX_POLL_LIMIT: u32 = 100_000;

/// 读 UARTFR 的 TXFF 位：发送 FIFO 是否已满。
fn tx_full() -> bool {
    // SAFETY: PL011 UARTFR（BASE+0x18）在 ARM v8-A 平台上是固定 MMIO 只读
    // 寄存器；early console 在 boot 阶段由当前核独占，无并发写者。单字节读
    // 与下方单字节 DR 写走同一设备解码路径（aarch64 载体串口日志已实证该
    // 访问宽度在本项目用的 QEMU virt + PL011 上可用）。
    let fr = unsafe { core::ptr::read_volatile((PL011_BASE + PL011_FR_OFFSET) as *const u8) };
    fr & PL011_FR_TXFF != 0
}

/// Write a single byte to PL011 UART.
///
/// 先等发送 FIFO 腾空再写（有界，见 [`tx_wait_then_send`]）。原实现是无条件
/// `write_volatile`：连写超过 16 字节就会撞满 FIFO 丢字（x86 侧同位置原实现
/// 同构踩过，已在 NK4-A 取证中修正）。QEMU 下不显现（串口同步排空，M3.1 归档
/// 日志里 64 字节的长行完整），但交棒后的诊断通道不能把正确性押在“模拟器排空
/// 够快”上。
pub fn write_byte(byte: u8) {
    tx_wait_then_send(
        byte,
        tx_full,
        |b| unsafe {
            // SAFETY: DR（BASE+0x00）同上：固定 MMIO，写一个字节即入发送
            // FIFO。已先等过 TXFF（或到达上限后放行）。
            core::ptr::write_volatile(PL011_BASE as *mut u8, b);
        },
        PL011_TX_POLL_LIMIT,
    );
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

/// Zero-sized type implementing [`EarlyConsole`] for aarch64.
pub struct AArch64EarlyConsole;

impl EarlyConsole for AArch64EarlyConsole {
    fn write_byte(byte: u8) {
        write_byte(byte);
    }
}
