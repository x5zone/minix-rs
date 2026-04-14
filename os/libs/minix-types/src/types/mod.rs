//! 类型定义模块
//!
//! 提供 Minix3 核心类型定义，包括：
//! - `com`: 系统级常量（MAX_NR_TASKS, NR_PROCS 等）
//! - `pid`: 进程 ID、进程索引
//! - `endpoint`: 端点标识（IPC 核心概念）
//! - `id`: 用户 ID、组 ID
//! - `clock`: 时钟滴答、虚拟地址
//! - `bitmap`: 泛型位图

mod com;
mod pid;
mod endpoint;
mod id;
mod clock;
mod bitmap;

pub use com::*;
pub use pid::*;
pub use endpoint::*;
pub use id::*;
pub use clock::*;
pub use bitmap::*;
