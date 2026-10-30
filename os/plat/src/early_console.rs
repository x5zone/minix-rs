//! Early console trait abstraction.
//!
//! Provides a cross-architecture interface for minimal serial output during
//! boot-stage diagnostics. Each architecture implements `write_byte`; the
//! higher-level `write_str` and `write_hex` are provided as default methods.

/// Control flow for sending a single byte: wait (bounded) for the transmit
/// side to drain, then write.
///
/// Why this pure control flow lives in a cross-architecture shared module
/// and takes its register accesses as closure parameters: the judgement of
/// an MMIO early console lies entirely in these few lines — which condition
/// releases the write, what the bound is, and what to do when the bound is
/// reached — yet the console itself does not exist in a host `cargo test`
/// (`lib.rs`'s `arm64` module is gated by `#[cfg(target_arch = "aarch64")]`,
/// so the host compilation unit does not include it). Extracting the
/// decision and injecting "read status" and "write data" is what lets the
/// host tests run the loop to completion, instead of reciting tautological
/// assertions against constants.
///
/// Semantics (isomorphic to the bounded polling in the x86 boot-shim
/// `lib.rs::emit_byte`):
/// - `tx_full()` false → write immediately;
/// - `tx_full()` true → poll at most `limit` more times (i.e. at most
///   `limit + 1` status reads in total);
/// - still true at the bound → **write anyway**.
///
/// The last rule is deliberate: the early console is the last channel for
/// panics and evidence collection, so dropping the occasional byte when a
/// device status bit misbehaves is far better than swallowing the whole
/// diagnostic message together with an endless loop.
pub fn tx_wait_then_send(
    byte: u8,
    mut tx_full: impl FnMut() -> bool,
    mut send: impl FnMut(u8),
    limit: u32,
) {
    let mut spins: u32 = 0;
    while tx_full() {
        if spins >= limit {
            break;
        }
        spins += 1;
    }
    send(byte);
}

/// Minimal early-boot console interface.
///
/// All architecture-specific output mechanisms (x86-64 COM1, ARM64 PL011,
/// RISC-V SBI ecall) are abstracted behind a single `write_byte` method.
/// `write_str` and `write_hex` are trait default methods shared across all
/// architectures — they handle newline translation (`\n` → `\r\n`) and
/// hexadecimal formatting.
pub trait EarlyConsole {
    /// One-time hardware initialization for the early console.
    ///
    /// Called once on the BSP before any output. Architectures whose console
    /// is already usable at boot (e.g. aarch64 PL011, riscv64 SBI) use the
    /// default no-op implementation.
    fn init() {}

    /// Write a single raw byte to the console.
    fn write_byte(byte: u8);

    /// Write a string, translating `\n` to `\r\n`.
    fn write_str(s: &str) {
        for b in s.bytes() {
            if b == b'\n' {
                Self::write_byte(b'\r');
            }
            Self::write_byte(b);
        }
    }

    /// Write a `u64` in hexadecimal (0x-prefixed, 16 digits).
    fn write_hex(val: u64) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        Self::write_str("0x");
        for i in (0..16).rev() {
            Self::write_byte(HEX[((val >> (i * 4)) & 0xf) as usize]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake UART: shares the mutable state of the two injection points
    /// ("read status" / "write data") through a `Cell` with interior
    /// mutability, so both closures can hold it by **immutable** borrow (a
    /// `&mut` borrow would be occupied by the first closure and the host
    /// build would not compile).
    ///
    /// The key is `reads_at_first_write`: it turns the "wait first, then
    /// write" ordering contract into an assertable number — counting
    /// reads/writes alone cannot catch "write first, then wait", and on
    /// hardware "write first, then wait" means dropped bytes.
    struct FakeUart {
        /// Number of status reads performed so far
        reads: core::cell::Cell<u32>,
        /// Number of bytes written so far
        writes: core::cell::Cell<u32>,
        /// Number of reads completed at the time of the first write
        /// (ordering evidence)
        reads_at_first_write: core::cell::Cell<u32>,
        /// How many leading reads report "transmit FIFO full"
        /// (`u32::MAX` = always full)
        full_reads: u32,
    }

    impl FakeUart {
        fn new(full_reads: u32) -> Self {
            Self {
                reads: core::cell::Cell::new(0),
                writes: core::cell::Cell::new(0),
                reads_at_first_write: core::cell::Cell::new(0),
                full_reads,
            }
        }

        fn tx_full(&self) -> bool {
            let n = self.reads.get() + 1;
            self.reads.set(n);
            n <= self.full_reads
        }

        fn send(&self, _byte: u8) {
            if self.writes.get() == 0 {
                self.reads_at_first_write.set(self.reads.get());
            }
            self.writes.set(self.writes.get() + 1);
        }
    }

    /// Discriminating assertion (bound takes effect + no early giving up +
    /// still writes at the bound): when the device always reports "full",
    /// the status must be read exactly `limit + 1` times, and then the byte
    /// must be **written anyway**. One extra read = the bound did not take
    /// effect (on hardware: an early-console endless loop); one read less =
    /// giving up early (dropped bytes); no write at all = the whole
    /// diagnostic message is swallowed.
    #[test]
    fn bounded_wait_gives_up_and_still_sends() {
        let uart = FakeUart::new(u32::MAX);
        tx_wait_then_send(b'x', || uart.tx_full(), |b| uart.send(b), 3);
        assert_eq!(uart.reads.get(), 4, "上限 3 次重读 = 共 4 次状态读取");
        assert_eq!(uart.writes.get(), 1, "到限必须放行，否则本字节连同后续整条消息丢失");
        assert_eq!(uart.reads_at_first_write.get(), 4, "写入发生在等满上限之后");
    }

    /// Discriminating assertion (no wasted waiting and no writing before the
    /// queue drains): when the FIFO drains on the third read (the first two
    /// report full), only 3 reads and 1 write happen, with the write
    /// immediately after the read that saw the drain. Extra reads = one
    /// wasted status poll per byte (multiplied by the byte count on slow
    /// devices); writing before the drain = dropped bytes.
    #[test]
    fn waits_exactly_until_tx_drains() {
        let uart = FakeUart::new(2);
        tx_wait_then_send(b'x', || uart.tx_full(), |b| uart.send(b), 100);
        assert_eq!(uart.reads.get(), 3, "腾空当次即放行，不多等一次");
        assert_eq!(uart.reads_at_first_write.get(), 3, "写必须在腾空之后");
        assert_eq!(uart.writes.get(), 1);
    }

    /// Discriminating assertion (idle fast path): when the transmit side is
    /// already empty, exactly one status read and one data write happen; the
    /// wait loop must not run.
    #[test]
    fn idle_device_sends_immediately() {
        let uart = FakeUart::new(0);
        tx_wait_then_send(b'x', || uart.tx_full(), |b| uart.send(b), 100);
        assert_eq!(uart.reads.get(), 1, "空闲设备不该多转");
        assert_eq!(uart.reads_at_first_write.get(), 1);
        assert_eq!(uart.writes.get(), 1);
    }
}
