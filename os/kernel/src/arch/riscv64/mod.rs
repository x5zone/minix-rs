//! RISC-V 64-bit (Sv39) architecture module.
//!
//! Provides:
//! - `Riscv64HigherHalf`: Higher-half transition via `la sp + jalr kmain`

pub mod higher_half;
