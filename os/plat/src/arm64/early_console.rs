//! Early boot console — PL011 UART (aarch64).
//!
//! Provides minimal serial output for boot-stage diagnostics.

use crate::early_console::{tx_wait_then_send, EarlyConsole};
use core::sync::atomic::{AtomicBool, Ordering};
use minix_platform::arch::aarch64::mmio_translate;

/// PL011 UART0 physical base on QEMU `virt` (matching the aarch64 carriers
/// under `os/qemu-tests/`).
const PL011_PA_BASE: u64 = 0x0900_0000;
/// Whether the higher-half (TTBR1) MMIO window is live.
///
/// Device MMIO is reachable only through the TTBR0 low-half identity mapping,
/// which is switched away by `switch_address_space`; after the first real
/// root switch, `tlbi alle1is` drops the cached identity entries, and
/// touching `PL011_PA_BASE` afterwards raises a synchronous exception. The
/// fix maps the MMIO window separately into the bootstrap root's higher half
/// (TTBR1, never switched with processes), and the runtime uses the
/// higher-half alias. This flag distinguishes the two phases:
/// - `false`: before `enable()` (boot-shim EBS + arch_boot steps 1/2) — the
///   code runs under firmware page tables where only the identity `VA==PA`
///   alias is reachable and the higher-half alias does not exist yet.
/// - `true`: after `enable()` — the higher-half window has been installed in
///   TTBR1 together with the bootstrap root, persists across root switches,
///   and the console uses the higher-half alias.
///
/// Set by the kernel after `arch_boot` installs the higher-half MMIO window
/// and calls `enable()`, via [`use_high_mmio_window`]. The default `false`
/// guarantees the very earliest output is not lost.
///
/// Ordering: for now the set and the read both run sequentially on the
/// bootstrap core (AP bring-up is still skeletal), so `Relaxed` leaves no
/// visibility gap. **Re-evaluate when the bootstrap core releases secondary
/// cores via PSCI `CPU_ON`**: the set must be ordered before any `CPU_ON`
/// (otherwise the secondary core's entry instruction stream could be
/// reordered to see `HIGH_MMIO_LIVE == true` too early and fall back to the
/// identity path already cleared by the root switch); use
/// `Release`/`Acquire` then.
static HIGH_MMIO_LIVE: AtomicBool = AtomicBool::new(false);

/// Switch the early console to the TTBR1-resident higher-half MMIO alias
/// (idempotent; only sets the flag).
pub fn use_high_mmio_window() {
    HIGH_MMIO_LIVE.store(true, Ordering::Relaxed);
}

/// The PL011 access base currently in effect: the TTBR1 alias once the
/// higher-half window is live, otherwise the identity `VA==PA` address.
#[inline]
fn pl011_base() -> u64 {
    if HIGH_MMIO_LIVE.load(Ordering::Relaxed) {
        mmio_translate(PL011_PA_BASE)
    } else {
        PL011_PA_BASE
    }
}
/// UARTFR (Flag Register) offset 0x18.
const PL011_FR_OFFSET: u64 = 0x18;
/// UARTFR bit5 = TXFF: transmit FIFO full (the PL011 FIFO is 16 bytes deep).
const PL011_FR_TXFF: u8 = 1 << 5;
/// Bounded polling limit before sending. The magnitude matches the x86
/// counterpart (`emit_byte` in `os/boot-shim/src/lib.rs`, which gives up
/// after ~100k iterations): drop bytes rather than spin forever if a device
/// status bit misbehaves.
const PL011_TX_POLL_LIMIT: u32 = 100_000;

/// Read the UARTFR TXFF bit: whether the transmit FIFO is full.
fn tx_full() -> bool {
    // SAFETY: PL011 UARTFR (BASE+0x18) is a fixed read-only MMIO register on
    // the ARM v8-A platform; the early console is owned by the current core
    // during boot and has no concurrent writer. The single-byte read shares
    // the device decode path with the single-byte DR write below (the
    // aarch64 carrier's serial log already demonstrates this access width
    // works on the QEMU virt + PL011 combination used by this project).
    let fr = unsafe { core::ptr::read_volatile((pl011_base() + PL011_FR_OFFSET) as *const u8) };
    fr & PL011_FR_TXFF != 0
}

/// Write a single byte to PL011 UART.
///
/// Wait for the transmit FIFO to drain before writing (bounded; see
/// [`tx_wait_then_send`]). The original implementation called
/// `write_volatile` unconditionally: writing more than 16 bytes back to back
/// filled the FIFO and dropped characters (the x86 counterpart had the same
/// shape and was fixed during forensics). It does not show under QEMU (the
/// serial port drains synchronously, and the 64-byte long lines in the
/// archived log come out complete), but the diagnostic channel after
/// hand-off must not stake correctness on "the emulator drains fast enough".
pub fn write_byte(byte: u8) {
    tx_wait_then_send(
        byte,
        tx_full,
        |b| unsafe {
            // SAFETY: DR (BASE+0x00) is the same fixed MMIO register: writing
            // one byte enqueues it into the transmit FIFO. TXFF has already
            // been waited on (or the limit was reached and the write is let
            // through).
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
