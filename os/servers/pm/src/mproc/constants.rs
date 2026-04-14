//! PM 私有常量定义
//!
//! 这些常量是 PM 服务私有的，不应放在 minix-types 中。
//!
//! # 为什么放在 PM crate？
//!
//! 1. **职责隔离**: PID 范围是 PM 的私有逻辑
//! 2. **微内核原则**: 其他服务不需要了解 PM 的 PID 生成规则
//!
//! # Minix3 源码映射
//!
//! ```c
//! // minix3/minix/servers/pm/const.h
//! #define NR_PIDS    30000    // PID 最大值
//! #define INIT_PID   1        // init 进程的 PID
//! #define NO_PID     0        // 无效 PID
//! #define NO_TRACER  0        // 无追踪者（进程表索引 0 是 INIT，不会被追踪）
//! ```

use minix_types::Pid;

/// PID 最大值
///
/// Minix3 定义：`#define NR_PIDS 30000`
///
/// PID 范围：2 ~ 30000（INIT_PID+1 到 NR_PIDS）
pub const NR_PIDS: Pid = 30000;

/// init 进程的 PID
///
/// Minix3 定义：`#define INIT_PID 1`
///
/// PID 1 是 init 进程，不会被重新分配
pub const INIT_PID: Pid = 1;

/// 无效 PID
///
/// Minix3 定义：`#define NO_PID 0`
///
/// 用于表示无效或未设置的 PID
pub const NO_PID: Pid = 0;

/// 无追踪者索引
///
/// Minix3 定义：`#define NO_TRACER 0`
///
/// 注意：Minix3 中 NO_TRACER = 0，因为进程表索引 0 是 INIT 进程，
/// INIT 进程永远不会被追踪（它是系统第一个进程）。
///
/// 这与 minix-types 中的 NO_TRACER = UserSlot(usize::MAX) 不同，
/// 但语义一致：表示"没有追踪者"。
pub const NO_TRACER_INDEX: usize = 0;
