//! First interrupt-source bring-up: QEMU virt UART RX (PLIC IRQ 10) — the
//! first PLIC device source.
//!
//! Device = NS16550A at `0x1000_0000` (QEMU virt DTB
//! `/soc/serial@10000000`), PLIC line 10, S-mode context. **Loopback
//! self-proof**: the MCR LOOP bit closes an internal THR→RDR ring, so the
//! whole SEI chain (UART device → PLIC → claim → hook chain → complete) can
//! be lit on real hardware without any external serial input.
//!
//! Enable sequence ([`enable_uart_rx_selftest`], caller holds the BKL — the
//! carrier boot holds it by convention):
//! ① `register_hook` (first registration auto-unmasks = PLIC enable bit, S context)
//! ② PLIC priority[10]=1 (QEMU reset value 0 means never delivered,
//!    `set_irq_priority`)
//! ③ `sie.SEIE=1`
//! ④ UART `IER.ERBFI=1` + `MCR.LOOP=1` + THR write of the expected byte
//! The hook reads RHR, verifies the expected character, and prints the
//! first-source marker (capped at 4, the same house rule as sei-ack).

use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering as AtomicOrd};
use minix_plat::{IrqAction, IrqNotifyId, IrqPolicy, IrqVector};
use crate::irq_manager::IrqHookContext;

/// QEMU virt NS16550A base address (DTB `/soc/serial@10000000`; inside the
/// identity-mapped window).
pub const UART_BASE: u64 = 0x1000_0000;
/// QEMU virt PLIC line number (`VIRT_UART0_IRQ`).
pub const UART_IRQ_NUM: u8 = 10;

static EXPECTED: AtomicU8 = AtomicU8::new(0);
static FIRED: AtomicUsize = AtomicUsize::new(0);
static MATCHED: AtomicUsize = AtomicUsize::new(0);

fn uart_read(off: u64) -> u8 {
    // SAFETY: device MMIO inside the identity-mapped window [0,4 GiB); reads
    // of RHR/LSR carry no side-effect ordering constraints.
    unsafe { ((UART_BASE + off) as *const u8).read_volatile() }
}

fn uart_write(off: u64, v: u8) {
    // SAFETY: same as above; writes IER/MCR/THR.
    unsafe { ((UART_BASE + off) as *mut u8).write_volatile(v) }
}

/// UART RX hook (first source): drain RDR, verify the expected character,
/// print the first-source marker (capped at 4 — same house rule as sei-ack,
/// serial writes are the bottleneck). Returns Completed (the level source ends
/// once RDR is drained).
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

/// Enable the UART RX self-test (caller holds the BKL — the carrier boot
/// holds it by convention). `expected`: the loopback byte the hook checks.
/// Writing THR triggers it.
pub fn enable_uart_rx_selftest(expected: u8) {
    EXPECTED.store(expected, AtomicOrd::Release);
    use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
    Console::write_str("  uart: stage0 enter\n");
    let section = unsafe { crate::smp::BklSection::assume_held() };
    let irq = IrqVector::new(UART_IRQ_NUM);
    // ① First registration auto-unmasks: the PLIC enable bit (S context).
    crate::irq_manager_with(&section).register_hook(
        irq,
        uart_rx_hook,
        minix_types::Endpoint::from_generation_slot(0, crate::proc::proc_nr::CLOCK.0),
        IrqNotifyId(0),
        IrqPolicy::REENABLE,
    )
    .expect("uart rx hook: no free slots in IRQ_MANAGER");
    Console::write_str("  uart: stageA hook+unmask\n");
    // ② PLIC priority[10] = 1 (reset value 0 means never delivered).
    crate::irq_manager_with(&section).set_irq_priority(irq, 1);
    Console::write_str("  uart: stageB priority\n");
    // ③ sie.SEIE = 1 (bit 9) — the master gate for external interrupts into S-mode.
    unsafe {
        core::arch::asm!("csrs sie, {}", in(reg) 1u64 << 9, options(nomem, nostack));
    }
    Console::write_str("  uart: stageC sie\n");
    // ④ UART: MCR.LOOP=1 (internal THR→RDR ring) + IER.ERBFI=1 (RX interrupt enable).
    let mcr = uart_read(4);
    uart_write(4, mcr | 0x10);
    uart_write(1, 0x01);
    Console::write_str("  uart: stageD uart regs\n");
    // ⑤ THR write of the expected byte: loopback turns it into RX and lights
    // the whole SEI chain. **Pulsed loopback**: clear LOOP immediately once
    // the byte lands in RDR — a lingering LOOP makes the console deaf to the
    // internal ring of the same UART (which OpenSBI putchar uses): later
    // markers never reach the serial port, and console output bytes become
    // spurious RX sources for the hook (third-layer lesson from the first
    // bring-up).
    uart_write(0, expected);
    uart_write(4, mcr & !0x10);
    uart_write(0, expected);
}

/// First-source readings (used by the script and accounting): (fire count,
/// match count).
pub fn readings() -> (usize, usize) {
    (
        FIRED.load(AtomicOrd::Relaxed),
        MATCHED.load(AtomicOrd::Relaxed),
    )
}
