//! x86-64 architecture module.
//!
//! Provides:
//! - `X86_64HigherHalf`: Higher-half transition via `mov rsp + call kmain`

pub mod higher_half;
