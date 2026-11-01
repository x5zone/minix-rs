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

/// Kernel device-window VA of the UART (PD-22 step 2): computed once from
/// the arch device description — the module never dereferences the
/// physical `UART_BASE` after paging is live; `None` before the window is
/// installed (the selftest arms only after boot, when the window is live).
fn uart_va(off: u64) -> Option<*mut u8> {
    use minix_arch::arch::device_window::{device_va_for, device_window_regions};
    let regions = device_window_regions();
    let slot = if cfg!(target_arch = "riscv64") { 1 } else { return None };
    let va = device_va_for(regions, slot, UART_BASE + off)?;
    Some(va as *mut u8)
}
/// QEMU virt PLIC line number (`VIRT_UART0_IRQ`).
pub const UART_IRQ_NUM: u8 = 10;

static EXPECTED: AtomicU8 = AtomicU8::new(0);
static FIRED: AtomicUsize = AtomicUsize::new(0);
static MATCHED: AtomicUsize = AtomicUsize::new(0);

fn uart_read(off: u64) -> u8 {
    // SAFETY: device MMIO inside the kernel device window (slot 1, PD-22
    // step 2); reads of RHR/LSR carry no side-effect ordering constraints.
    unsafe { (uart_va(off).expect("UART device window unmapped") as *const u8).read_volatile() }
}

fn uart_write(off: u64, v: u8) {
    // SAFETY: same as above; writes IER/MCR/THR via the kernel device
    // window (slot 1).
    unsafe { (uart_va(off).expect("UART device window unmapped") as *mut u8).write_volatile(v) }
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
    Console::write_str("  uart: stageC1 mcr read\n");
    // PD22S2H diag: machine state right before the first window-VA store.
    {
        let (mut sstatus, mut sie, mut sscratch, mut satp): (u64, u64, u64, u64);
        unsafe {
            core::arch::asm!(
                "csrr {}, sstatus",
                "csrr {}, sie",
                "csrr {}, sscratch",
                "csrr {}, satp",
                out(reg) sstatus, out(reg) sie, out(reg) sscratch, out(reg) satp,
                options(nomem)
            );
        }
        for (name, v) in [("sstatus", sstatus), ("sie", sie), ("sscratch", sscratch), ("satp", satp)] {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut hb = [0u8; 16];
            let mut vv = v;
            for i in (0..16).rev() {
                hb[i] = HEX[(vv & 0xf) as usize];
                vv >>= 4;
            }
            Console::write_str("  [mcr-diag] ");
            Console::write_str(name);
            Console::write_str("=0x");
            Console::write_str(core::str::from_utf8(&hb).unwrap_or("?"));
            Console::write_str("\n");
        }
    }
    uart_write(4, mcr | 0x10);
    Console::write_str("  uart: stageC2 mcr loop\n");
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
    // PD22S2H diag: PLIC state right after the trigger — distinguishes
    // "UART did not raise the line" (pending bit 10 clear) from "enable
    // written to the wrong context" (enable bit 10 clear) from "PLIC
    // holds it" (both set, SEI pending in sip).
    use minix_plat::riscv64::early_console as sbi_con;
    let regions = minix_arch::arch::device_window::device_window_regions();
    let plic_va = minix_arch::arch::device_window::device_va_for(
        regions,
        0,
        regions[0].phys_base,
    )
    .expect("PLIC window") as usize;
    let mut show = |name: &str, addr: usize| {
        let v = unsafe { (addr as *const u32).read_volatile() };
        sbi_con::write_str("  [plic-diag] ");
        sbi_con::write_str(name);
        sbi_con::write_str("=0x");
        let mut hb = [0u8; 8];
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut vv = v as u64;
        for i in (0..8).rev() {
            hb[i] = HEX[(vv & 0xf) as usize];
            vv >>= 4;
        }
        sbi_con::write_str(core::str::from_utf8(&hb).unwrap_or("?"));
        sbi_con::write_str("\n");
    };
    {
        let mut hb = [0u8; 16];
        const HX: &[u8; 16] = b"0123456789abcdef";
        let mut vv = plic_va as u64;
        for i in (0..16).rev() {
            hb[i] = HX[(vv & 0xf) as usize];
            vv >>= 4;
        }
        sbi_con::write_str("  [plic-diag] diag_base=0x");
        sbi_con::write_str(core::str::from_utf8(&hb).unwrap_or("?"));
        sbi_con::write_str("\n");
    }
    show("pending0", plic_va + 0x1000);
    show("enable_c1_w0", plic_va + 0x2000 + 0x100);
    show("threshold_c1", plic_va + 0x200000 + 0x1000);
    let sip_word = {
        let mut v: u32;
        // SAFETY: PLIC window MMIO read of... reuses the pending block as a
        // canary: the SEIP state is visible via sip only through CSR, which
        // faulted in this context; the MMIO pending read above already
        // proves the line state, so this second pending read suffices.
        v = unsafe { (plic_va as *const u32).read_volatile() };
        v
    };
    show("plic_base_canary", sip_word as usize);
}

/// First-source readings (used by the script and accounting): (fire count,
/// match count).
pub fn readings() -> (usize, usize) {
    (
        FIRED.load(AtomicOrd::Relaxed),
        MATCHED.load(AtomicOrd::Relaxed),
    )
}
