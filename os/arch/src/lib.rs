//! 硬件抽象层 (Hardware Abstraction Layer)
//!
//! 提供跨架构的硬件机制抽象，定义trait接口。
//! 具体实现由各架构模块提供（mock, x86_64, arm64, riscv64）。
//!
//! # 设计原则
//!
//! 1. **分散定义**: 各功能模块定义自己的trait（如页表、中断、时钟）
//! 2. **集中实现**: 所有trait在arch crate中统一实现
//! 3. **架构无关**: OS代码只依赖trait，不依赖具体硬件
//!
//! # 当前支持
//!
//! - `mock`: Mock硬件实现，用于用户态测试
//! - `x86_64`: x86-64架构（待实现）
//! - `arm64`: ARM64架构（待实现）
//! - `riscv64`: RISC-V 64位架构（待实现）

#![cfg_attr(not(feature = "mock"), no_std)]

extern crate alloc;

pub mod paging;

// 根据特性选择实现
#[cfg(feature = "mock")]
pub use paging::mock::MockPaging;

/// 当前架构的页表实现类型
#[cfg(feature = "mock")]
pub type CurrentPaging = MockPaging;
