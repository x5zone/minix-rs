//! 进程管理模块
//!
//! 提供进程状态机相关类型定义，包括：
//! - `lifecycle`: 进程生命周期状态
//! - `block`: 进程阻塞状态
//! - `wait`: 父进程等待状态
//! - `guardianship`: 监护关系（父进程/追踪者）
//! - `credentials`: 权限凭证
//! - `signal`: 信号处理状态
//! - `trace`: 追踪状态
//! - `process`: 进程结构体

mod lifecycle;
mod block;
mod wait;
mod guardianship;
mod credentials;
mod signal;
mod trace;
mod process;

pub use lifecycle::*;
pub use block::*;
pub use wait::*;
pub use guardianship::*;
pub use credentials::*;
pub use signal::*;
pub use trace::*;
pub use process::*;
