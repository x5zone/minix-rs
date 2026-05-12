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
pub mod direct_map;

#[cfg(feature = "x86_64")]
pub mod x86_64;

pub use paging_ext::{PagingWithId, HugePages, VmPagingExt};
pub use direct_map::DirectMapArch;

#[cfg(feature = "mock")]
pub use paging::mock::MockPaging;

#[cfg(feature = "mock")]
pub use paging::mock::MockAsid;

#[cfg(feature = "mock")]
pub type CurrentPaging = MockPaging;

#[cfg(feature = "x86_64")]
pub type CurrentPaging = crate::x86_64::paging::X86_64Paging;

#[cfg(feature = "mock")]
pub use direct_map::MockDirectMap;

#[cfg(feature = "x86_64")]
pub use direct_map::X86_64DirectMap;

#[cfg(feature = "mock")]
pub type CurrentDirectMap = MockDirectMap;

#[cfg(feature = "x86_64")]
pub type CurrentDirectMap = X86_64DirectMap;
