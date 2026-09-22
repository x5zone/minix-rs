//! Early console trait abstraction.
//!
//! Provides a cross-architecture interface for minimal serial output during
//! boot-stage diagnostics. Each architecture implements `write_byte`; the
//! higher-level `write_str` and `write_hex` are provided as default methods.

/// 发送一个字节的控制流：先「有界等待发送侧腾空」，再写。
///
/// 为什么这个纯控制流住在跨架构共享模块里、且把寄存器访问做成闭包参数：
/// MMIO 型 early console 的判断力全在「等到什么条件才放行、上限是多少、
/// 上限到了怎么办」这几行上，而它本体在宿主 `cargo test` 里根本不存在
/// （`lib.rs` 的 `arm64` 模块带 `#[cfg(target_arch = "aarch64")]`，宿主
/// 编译单元不含它）。把决策抽出来、把「读状态」与「写数据」注入进来，
/// 宿主测试才能把这条循环跑满——而不是对着常量做同义反复断言。
///
/// 语义（与 x86 boot-shim `lib.rs::emit_byte` 的有界轮询同构）：
/// - `tx_full()` 为假 → 立即写；
/// - `tx_full()` 为真 → 最多再轮询 `limit` 次（即状态读取至多 `limit + 1` 次）；
/// - 达到上限仍为真 → **照写**。
///
/// 最后一条是刻意的：early console 是 panic / 取证的最后一条通道，设备
/// 状态位异常时丢掉个别字节，也远好过把整条诊断消息连同死循环一起吞掉。
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

    /// 假 UART：把「读状态 / 写数据」两个注入点共享的可变状态做成 `Cell`
    /// 内部可变，以便两个闭包都以**不可变**借用拿到它（`&mut` 会被第一个
    /// 闭包占住，宿主编译不过）。
    ///
    /// 关键是 `reads_at_first_write`：它把「先等后写」这个时序契约变成可断言
    /// 的数——只看读/写次数抓不出「先写后等」，而真机上「先写后等」就是丢字。
    struct FakeUart {
        /// 已做的状态读取次数
        reads: core::cell::Cell<u32>,
        /// 已写入的字节数
        writes: core::cell::Cell<u32>,
        /// 第一次写入时已完成的读取次数（时序证据）
        reads_at_first_write: core::cell::Cell<u32>,
        /// 前多少次读取报「发送 FIFO 满」（`u32::MAX` = 永远满）
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

    /// 判别断言（上限生效 + 不提前放弃 + 到限仍写）：设备永远报「满」时，
    /// 状态读取必须恰好 `limit + 1` 次，然后**照写**。多一次 = 上限没生效
    /// （真机表现为 early console 死循环）；少一次 = 提前放弃（表现为丢字）；
    /// 没写 = 整条诊断消息被吞。
    #[test]
    fn bounded_wait_gives_up_and_still_sends() {
        let uart = FakeUart::new(u32::MAX);
        tx_wait_then_send(b'x', || uart.tx_full(), |b| uart.send(b), 3);
        assert_eq!(uart.reads.get(), 4, "上限 3 次重读 = 共 4 次状态读取");
        assert_eq!(uart.writes.get(), 1, "到限必须放行，否则本字节连同后续整条消息丢失");
        assert_eq!(uart.reads_at_first_write.get(), 4, "写入发生在等满上限之后");
    }

    /// 判别断言（不白等也不抢写）：FIFO 在第 3 次读取时腾空（前 2 次报满），
    /// 则只读 3 次、写 1 次，且写入紧跟在腾空那一次读取之后。多读 = 每字节
    /// 白转（慢设备上按字节数放大）；抢在腾空前写 = 丢字。
    #[test]
    fn waits_exactly_until_tx_drains() {
        let uart = FakeUart::new(2);
        tx_wait_then_send(b'x', || uart.tx_full(), |b| uart.send(b), 100);
        assert_eq!(uart.reads.get(), 3, "腾空当次即放行，不多等一次");
        assert_eq!(uart.reads_at_first_write.get(), 3, "写必须在腾空之后");
        assert_eq!(uart.writes.get(), 1);
    }

    /// 判别断言（空闲快路径）：发送侧本来空时，只读一次状态、写一次数据，
    /// 不该进等待循环。
    #[test]
    fn idle_device_sends_immediately() {
        let uart = FakeUart::new(0);
        tx_wait_then_send(b'x', || uart.tx_full(), |b| uart.send(b), 100);
        assert_eq!(uart.reads.get(), 1, "空闲设备不该多转");
        assert_eq!(uart.reads_at_first_write.get(), 1);
        assert_eq!(uart.writes.get(), 1);
    }
}
