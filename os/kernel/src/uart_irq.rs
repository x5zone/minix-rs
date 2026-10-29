//! SD-41/C3 首源触发：QEMU virt UART RX（PLIC IRQ 10）——首个 PLIC 设备源。
//!
//! B 板「首个 PLIC 设备驱动」由 C 板代领做（2026-10-20 授权，B 板线程已收）。
//! 设备=NS16550A @ `0x1000_0000`（QEMU virt DTB `/soc/serial@10000000`），
//! PLIC 线 10，S-mode context。**回环自证**：MCR LOOP 位使 THR→RDR 内环，
//! 无需外部串口输入即真机点亮 SEI 全链（UART 设备→PLIC→claim→钩子链→complete）。
//!
//! 使能序（[`enable_uart_rx_selftest`]，调用方持 BKL——载体 boot 持锁惯例）：
//! ① `register_hook`（首挂自动 unmask=PLIC enable 位，S context）
//! ② PLIC priority[10]=1（QEMU 复位值 0=永不投递，`set_irq_priority`）
//! ③ `sie.SEIE=1`
//! ④ UART `IER.ERBFI=1` + `MCR.LOOP=1` + THR 写期望字节
//! 钩子读 RHR 校验期望字符并打首源标记（封顶 4，sei-ack 同款房规）。

use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering as AtomicOrd};
use minix_plat::{IrqAction, IrqNotifyId, IrqPolicy, IrqVector};
use crate::irq_manager::IrqHookContext;

/// QEMU virt NS16550A 基址（DTB `/soc/serial@10000000`；恒等映射窗内）。
pub const UART_BASE: u64 = 0x1000_0000;
/// QEMU virt PLIC 线号（`VIRT_UART0_IRQ`）。
pub const UART_IRQ_NUM: u8 = 10;

static EXPECTED: AtomicU8 = AtomicU8::new(0);
static FIRED: AtomicUsize = AtomicUsize::new(0);
static MATCHED: AtomicUsize = AtomicUsize::new(0);

fn uart_read(off: u64) -> u8 {
    // SAFETY: 恒等映射窗 [0,4 GiB) 内的设备 MMIO；读 RHR/LSR 无副作用序列约束。
    unsafe { ((UART_BASE + off) as *const u8).read_volatile() }
}

fn uart_write(off: u64, v: u8) {
    // SAFETY: 同上；写 IER/MCR/THR。
    unsafe { ((UART_BASE + off) as *mut u8).write_volatile(v) }
}

/// UART RX 钩子（SD-41/C3 首源）：排空 RDR、校验期望字符、打首源标记
/// （封顶 4——sei-ack 同款房规，串口写是瓶颈）。返回 Completed（电平
/// 源已随 RDR 排空终结）。
fn uart_rx_hook(_ctx: &mut IrqHookContext) -> IrqAction {
    let mut got: Option<u8> = None;
    while uart_read(5) & 1 == 1 {
        got = Some(uart_read(0));
    }
    let n = FIRED.fetch_add(1, AtomicOrd::AcqRel);
    let exp = EXPECTED.load(AtomicOrd::Acquire);
    let matched = exp != 0 && got == Some(exp);
    if matched {
        MATCHED.fetch_add(1, AtomicOrd::AcqRel);
    }
    if n < 4 {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        Console::write_str("nk4c: uart-rx irq10 fired=#");
        let mut hb = [0u8; 8];
        let mut v = n as u64;
        for byte in hb.iter_mut().rev() {
            *byte = b"0123456789abcdef"[(v & 0xf) as usize];
            v >>= 4;
        }
        Console::write_str(core::str::from_utf8(&hb).unwrap_or("?"));
        Console::write_str(" ch=");
        match got {
            Some(ch) => {
                let mut b2 = [0u8; 2];
                b2[0] = b'\'';
                b2[1] = ch;
                Console::write_str(core::str::from_utf8(&b2).unwrap_or("?"));
            }
            None => Console::write_str("none"),
        }
        Console::write_str(" match=");
        Console::write_str(if matched { "1" } else { "0" });
        Console::write_str("\n");
    }
    IrqAction::Completed
}

/// 使能 UART RX 首源自证（调用方持 BKL——载体 boot 持锁惯例）。
/// `expected`：回环期待字节（钩子校验用）。写 THR 即触发。
pub fn enable_uart_rx_selftest(expected: u8) {
    EXPECTED.store(expected, AtomicOrd::Release);
    use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
    Console::write_str("  uart: stage0 enter\n");
    let section = unsafe { crate::smp::BklSection::assume_held() };
    let irq = IrqVector::new(UART_IRQ_NUM);
    // ① 首挂自动 unmask：PLIC enable 位（S context）。
    crate::irq_manager_with(&section).register_hook(
        irq,
        uart_rx_hook,
        minix_types::Endpoint::from_generation_slot(0, crate::proc::proc_nr::CLOCK.0),
        IrqNotifyId(0),
        IrqPolicy::REENABLE,
    )
    .expect("uart rx hook: no free slots in IRQ_MANAGER");
    Console::write_str("  uart: stageA hook+unmask\n");
    // ② PLIC priority[10] = 1（复位值 0=永不投递）。
    crate::irq_manager_with(&section).set_irq_priority(irq, 1);
    Console::write_str("  uart: stageB priority\n");
    // ③ sie.SEIE = 1（bit 9）——外部中断到 S-mode 的总闸。
    unsafe {
        core::arch::asm!("csrs sie, {}", in(reg) 1u64 << 9, options(nomem, nostack));
    }
    Console::write_str("  uart: stageC sie\n");
    // ④ UART：MCR.LOOP=1（THR→RDR 内环）+ IER.ERBFI=1（RX 中断使能）。
    let mcr = uart_read(4);
    uart_write(4, mcr | 0x10);
    uart_write(1, 0x01);
    Console::write_str("  uart: stageD uart regs\n");
    // ⑤ THR 写期望字节：回环即 RX，SEI 全链点亮。**脉冲式回环**：字节落
    // RDR 后立即清 LOOP——LOOP 悬置会让控制台（同一 UART 经 OpenSBI
    // putchar）内环失聪：后续 marker 无法出串口、控制台输出字节反成钩子
    // 的垃圾 RX 源（首点火实测的第三层教训）。
    uart_write(0, expected);
    uart_write(4, mcr & !0x10);
    uart_write(0, expected);
}

/// 首源读数（脚本/板账判据用）：（触发次数, 匹配次数）。
pub fn readings() -> (usize, usize) {
    (
        FIRED.load(AtomicOrd::Relaxed),
        MATCHED.load(AtomicOrd::Relaxed),
    )
}
