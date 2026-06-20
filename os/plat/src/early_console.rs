//! Early console trait abstraction.
//!
//! Provides a cross-architecture interface for minimal serial output during
//! boot-stage diagnostics. Each architecture implements `write_byte`; the
//! higher-level `write_str` and `write_hex` are provided as default methods.

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
