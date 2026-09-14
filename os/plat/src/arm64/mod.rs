//! ARM64 platform implementation

pub mod early_console;
pub mod interrupt;
pub mod shutdown;

pub use early_console::AArch64EarlyConsole;
pub use interrupt::AArch64InterruptController;
