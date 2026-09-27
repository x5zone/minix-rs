//! Early boot console — PL011 UART (aarch64).
//!
//! Provides minimal serial output for boot-stage diagnostics.

use crate::early_console::{tx_wait_then_send, EarlyConsole};
use core::sync::atomic::{AtomicBool, Ordering};
use minix_platform::arch::aarch64::mmio_translate;

/// PL011 UART0 物理基址 on QEMU `virt`（与 `os/qemu-tests/` 的 aarch64 载体一致）。
const PL011_PA_BASE: u64 = 0x0900_0000;
/// 内核高半（TTBR1）MMIO 窗口是否已生效。
///
/// §1.114：设备 MMIO 只在会被 `switch_address_space` 切走的 TTBR0 低半身份
/// 映射里可达，首真切根后 `tlbi alle1is` 清掉缓存的身份条目，之后再摸
/// `PL011_PA_BASE` 即同步异常。修法是把 MMIO 窗口另映射进 bootstrap 根的高半
/// （TTBR1，永不随进程切换），运行时改走高半别名。本标志区分两个阶段：
/// - `false`：`enable()` 之前（boot-shim EBS + arch_boot Step 1/2）——此时
///   跑在固件页表下，只有 `VA==PA` 身份可达，高半别名尚未建立。
/// - `true`：`enable()` 之后——高半窗口已随 bootstrap 根装入 TTBR1，切根
///   持久，控制台改走高半别名。
///
/// 由内核在 `arch_boot` 装好高半 MMIO 窗口并 `enable()` 后调用
/// [`use_high_mmio_window`] 置位。默认 `false` 保证极早期输出不丢。
///
/// Ordering：当前置位/读取全在 bootstrap 核顺序执行（AP bring-up 尚为
/// 骨架），`Relaxed` 无可见性缺口。**S-4 经 PSCI `CPU_ON` 放行次级核时重新
/// 评估**：须保证置位先于任何 `CPU_ON`（否则次级核入口指令流可能被重排到
/// 看到 `HIGH_MMIO_LIVE==true` 之前，落回已被切根清除的身份路径），届时改用
/// `Release`/`Acquire`。
static HIGH_MMIO_LIVE: AtomicBool = AtomicBool::new(false);

/// 将早期控制台切到 TTBR1 常驻的高半 MMIO 别名（幂等，仅置标志）。
pub fn use_high_mmio_window() {
    HIGH_MMIO_LIVE.store(true, Ordering::Relaxed);
}

/// 当前生效的 PL011 访问基址：高半窗口生效后走 TTBR1 别名，否则走身份 `VA==PA`。
#[inline]
fn pl011_base() -> u64 {
    if HIGH_MMIO_LIVE.load(Ordering::Relaxed) {
        mmio_translate(PL011_PA_BASE)
    } else {
        PL011_PA_BASE
    }
}
/// UARTFR（Flag Register）偏移 0x18。
const PL011_FR_OFFSET: u64 = 0x18;
/// UARTFR bit5 = TXFF：发送 FIFO 满（PL011 的 FIFO 深度 16 字节）。
const PL011_FR_TXFF: u8 = 1 << 5;
/// 发送前的有界轮询上限。数量级跟 x86 侧对位实现（`os/boot-shim/src/lib.rs`
/// 的 `emit_byte`，~10 万次即放弃）一致：设备状态位异常时宁丢字节。
const PL011_TX_POLL_LIMIT: u32 = 100_000;

/// 读 UARTFR 的 TXFF 位：发送 FIFO 是否已满。
fn tx_full() -> bool {
    // SAFETY: PL011 UARTFR（BASE+0x18）在 ARM v8-A 平台上是固定 MMIO 只读
    // 寄存器；early console 在 boot 阶段由当前核独占，无并发写者。单字节读
    // 与下方单字节 DR 写走同一设备解码路径（aarch64 载体串口日志已实证该
    // 访问宽度在本项目用的 QEMU virt + PL011 上可用）。
    let fr = unsafe { core::ptr::read_volatile((pl011_base() + PL011_FR_OFFSET) as *const u8) };
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
            core::ptr::write_volatile(pl011_base() as *mut u8, b);
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
