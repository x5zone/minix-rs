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

/// The IRQ vector that carries the boot clock source's interrupt, on the
/// current architecture's interrupt-controller numbering.
///
/// The boot clock source is registered with `IrqManager::register_hook`
/// under this vector (`bsp_finish_booting` Step 6, the C
/// `boot_cpu_init_timer` position, clock.c:294); the controller-side
/// delivery gate (the IRQ line) is unmasked there by the C-parity
/// "unmask on first handler" rule (interrupt.c:65).
///
/// Per-architecture values and their hardware reasons live next to each
/// architecture's interrupt controller driver; the cfg selection here is
/// the sanctioned "define current" pattern (CLAUDE.md hardware-abstraction
/// rule): it picks a constant, it does not pick behavior.
///
/// - x86_64: `0` — the 8254 PIT output is IOAPIC input 0
///   (C: CLOCK_IRQ = 0, arch/i386).
/// - aarch64: `30` — the EL1 non-secure physical timer (CNTP, the bank
///   that `CNTP_CTL_EL0` controls at EL1) is PPI INTID 30 per the GIC PPI
///   assignment (CNTPNSIRQ); QEMU virt maps it as device-tree PPI 14 with
///   PPI base 16.
/// - riscv64: `0` — a pseudo-vector. The S-mode timer is a CPU-local
///   interrupt (gated by `sie.STIE`, no PLIC source exists); vector 0 is
///   reserved as its dispatch identity (the future trap entry maps
///   `scause == SupervisorTimer` here), and the PLIC driver's mask/unmask
///   treat 0 as "no controller line" by design.
#[cfg(target_arch = "x86_64")]
pub const TIMER_IRQ: IrqVector = crate::x86_64::interrupt::TIMER_IRQ;
#[cfg(target_arch = "aarch64")]
pub const TIMER_IRQ: IrqVector = crate::arm64::interrupt::TIMER_IRQ;
#[cfg(target_arch = "riscv64")]
pub const TIMER_IRQ: IrqVector = crate::riscv64::interrupt::TIMER_IRQ;
#[cfg(all(feature = "mock", not(target_arch = "x86_64"), not(target_arch = "aarch64"), not(target_arch = "riscv64")))]
pub const TIMER_IRQ: IrqVector = IrqVector::new(0);

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

// ── S-11: QEMU test shutdown backends (§3.8) ──
//
// `shutdown_qemu(status)` terminates the test VM: x86-64 isa-debug-exit
// (exit code (status<<1)|1 — always odd; run_qemu's shutdown test special-
// cases rc==1 by its serial marker), aarch64 semihosting SYS_EXIT (exit 0),
// riscv64 sifive_test FINISHER_PASS (exit 0). Requires the corresponding
// QEMU option/device (isa-debug-exit device / -semihosting / none for
// sifive_test). The real-hardware backends (ACPI S5 / PSCI SYSTEM_OFF /
// SBI SRST) are separate later lanes — §3.8's two-layer rule.
#[cfg(target_arch = "x86_64")]
pub fn shutdown_qemu(status: u32) -> ! {
    crate::x86_64::shutdown::qemu_exit(status)
}
#[cfg(target_arch = "aarch64")]
pub fn shutdown_qemu(status: u32) -> ! {
    crate::arm64::shutdown::qemu_exit(status)
}
#[cfg(target_arch = "riscv64")]
pub fn shutdown_qemu(status: u32) -> ! {
    crate::riscv64::shutdown::qemu_exit(status)
}
