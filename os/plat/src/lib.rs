//! Board-level platform abstraction (minix-plat)
//!
//! Provides hardware-agnostic interfaces for board-level devices:
//! - **Early console**: Minimal serial output for boot-stage diagnostics
//! - **Interrupt controller**: IRQ mask/unmask/ack/eoi operations
//!
//! This crate is separate from `minix-arch` (CPU ISA abstractions) because:
//! - Board-level devices (UART, interrupt controllers) vary independently of CPU ISA
//! - A single CPU architecture (e.g., ARM64) can have different interrupt controllers (GICv2, GICv3, GICv4)
//! - Servers that only need CPU ISA traits don't need to pull in board-level device code
//!
//! # Architecture mapping
//!
//! | Platform | Early Console | Interrupt Controller | Port I/O       |
//! |----------|--------------|---------------------|----------------|
//! | mock     | log::debug   | log::debug          | log::debug     |
//! | x86-64   | COM1 (0x3F8) | LAPIC + IOAPIC      | in/out insns   |
//! | ARM64    | PL011 UART   | GICv3               | no-op (no I/O) |
//! | RISC-V   | SBI ecall    | PLIC                | no-op (no I/O) |

#![no_std]

pub mod early_console;
pub mod interrupt;
pub mod port_io;

#[cfg(feature = "mock")]
pub mod mock;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "aarch64")]
pub mod arm64;
#[cfg(target_arch = "riscv64")]
pub mod riscv64;

// ── Re-exports ──
pub use early_console::EarlyConsole;
pub use interrupt::{
    InterruptController, IrqVector, IrqId, IrqNotifyId, IrqPolicy, IrqAction,
    NR_IRQ_VECTORS, NR_IRQ_HOOKS,
};
pub use port_io::PortIo;

#[cfg(feature = "mock")]
pub use mock::{MockInterruptController, MockEarlyConsole, MockPortIo};

// ── CurrentInterruptController type alias ──
#[cfg(all(feature = "mock", not(target_arch = "x86_64"), not(target_arch = "aarch64"), not(target_arch = "riscv64")))]
pub type CurrentInterruptController = MockInterruptController;
#[cfg(target_arch = "x86_64")]
pub type CurrentInterruptController = crate::x86_64::interrupt::X86_64InterruptController;
#[cfg(target_arch = "aarch64")]
pub type CurrentInterruptController = crate::arm64::interrupt::AArch64InterruptController;
#[cfg(target_arch = "riscv64")]
pub type CurrentInterruptController = crate::riscv64::interrupt::Riscv64InterruptController;

// ── CurrentEarlyConsole type alias ──
#[cfg(all(feature = "mock", not(target_arch = "x86_64"), not(target_arch = "aarch64"), not(target_arch = "riscv64")))]
pub type CurrentEarlyConsole = MockEarlyConsole;
#[cfg(target_arch = "x86_64")]
pub type CurrentEarlyConsole = crate::x86_64::early_console::X86_64EarlyConsole;
#[cfg(target_arch = "aarch64")]
pub type CurrentEarlyConsole = crate::arm64::early_console::AArch64EarlyConsole;
#[cfg(target_arch = "riscv64")]
pub type CurrentEarlyConsole = crate::riscv64::early_console::Riscv64EarlyConsole;

// ── CurrentPortIo type alias ──
#[cfg(all(feature = "mock", not(target_arch = "x86_64")))]
pub type CurrentPortIo = MockPortIo;
#[cfg(target_arch = "x86_64")]
pub type CurrentPortIo = crate::x86_64::port_io::X86_64PortIo;
