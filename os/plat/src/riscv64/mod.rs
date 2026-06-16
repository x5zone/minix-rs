//! RISC-V 64-bit platform implementation

pub mod early_console;
pub mod interrupt;

pub use early_console::Riscv64EarlyConsole;
pub use interrupt::Riscv64InterruptController;
