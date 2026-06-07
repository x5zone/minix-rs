//! RISC-V 64-bit architecture implementation

pub mod paging;
pub mod early_console;

pub use paging::Riscv64Paging;