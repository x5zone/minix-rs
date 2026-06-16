//! x86-64 platform implementation

pub mod early_console;
pub mod interrupt;
pub mod port_io;

pub use early_console::X86_64EarlyConsole;
pub use interrupt::X86_64InterruptController;
pub use port_io::X86_64PortIo;
