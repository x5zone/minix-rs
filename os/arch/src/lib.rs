//! Hardware Abstraction Layer
//!
//! Provides cross-architecture hardware mechanism abstractions and trait interfaces.
//! Concrete implementations are provided by each architecture module (mock, x86_64, arm64, riscv64).
//!
//! # Design principles
//!
//! 1. **Distributed definition**: Each feature module defines its own traits (e.g., paging, interrupts, timers)
//! 2. **Centralized implementation**: All traits are implemented within the arch crate
//! 3. **Architecture-independent**: OS code depends only on traits, not on specific hardware
//!
//! # Current support
//!
//! - `mock`: Mock hardware implementation for user-space testing
//! - `x86_64`: x86-64 architecture (not yet implemented)
//! - `arm64`: ARM64 architecture (not yet implemented)
//! - `riscv64`: RISC-V 64-bit architecture (not yet implemented)

#![cfg_attr(not(feature = "mock"), no_std)]

extern crate alloc;

pub mod paging;
pub mod paging_ext;

#[cfg(feature = "x86_64")]
pub mod x86_64;

pub use paging_ext::{PagingWithId, HugePages, VmPagingExt};

#[cfg(feature = "mock")]
pub use paging::mock::MockPaging;

#[cfg(feature = "mock")]
pub use paging::mock::MockAsid;

/// Page table implementation type for the current architecture
#[cfg(feature = "mock")]
pub type CurrentPaging = MockPaging;
