//! Boot module — kernel bootstrap from paging-enable to kmain.
//!
//! This module contains:
//! - `higher_half`: Trait for the higher-half kernel transition
//! - Architecture-specific trampoline integration
//!
//! See 02-higher-half-kernel.md for the full design rationale.

pub mod higher_half;

pub use higher_half::HigherHalf;
