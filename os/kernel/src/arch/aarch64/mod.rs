//! AArch64 architecture module.
//!
//! Provides:
//! - `AArch64HigherHalf`: Higher-half transition via `mov sp + br kmain`

pub mod higher_half;
