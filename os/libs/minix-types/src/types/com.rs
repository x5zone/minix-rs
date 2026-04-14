//! 系统级常量定义
//!
//! 对应 Minix3 的 `<minix/com.h>`
//!
//! # 说明
//!
//! 这些常量是系统级的全局配置，被多个模块共享：
//! - `MAX_NR_TASKS`: 最大任务数（内核任务）
//! - `NR_PROCS`: 最大进程数
//! - `LAST_FEW`: 保留给 root 的槽位数
//!
//! 这些常量放在独立的 `com` 模块中，因为它们：
//! 1. 是系统级配置，不属于某个特定类型
//! 2. 被多个模块共享（pid, endpoint 等）
//! 3. 对应 Minix3 的 `com.h` 头文件

/// 最大任务数（内核任务）
///
/// 对应 Minix3 的 `MAX_NR_TASKS` (在 `com.h` 中定义)
///
/// # 说明
///
/// 这是系统支持的最大任务数（内核任务），值为 1023。
/// 用户进程数由 `NR_PROCS` 定义。
pub const MAX_NR_TASKS: usize = 1023;

/// 最大进程数
///
/// 对应 Minix3 的 `NR_PROCS` (在 `config.h` 中定义)
///
/// # 说明
///
/// 这是系统支持的最大用户进程数。
/// 注意：实际可用的进程槽位还受 `MAX_NR_PROCS` 限制
/// （由端点生成机制决定）。
pub const NR_PROCS: usize = 256;

/// 保留给 root 的槽位数
///
/// 对应 Minix3 的 `LAST_FEW`
///
/// # 说明
///
/// 最后几个进程槽位保留给 root 用户，
/// 防止普通用户耗尽所有进程槽位。
pub const LAST_FEW: usize = 5;

/// 实际任务数（引导时初始化的任务）
///
/// 对应 Minix3 的 `NR_TASKS`
pub const NR_TASKS: usize = 5;

/// 最后一个特殊进程号
///
/// 对应 Minix3 的 `LAST_SPECIAL_PROC_NR` (init 进程)
pub const LAST_SPECIAL_PROC_NR: usize = 11;

/// 引导模块数
///
/// 对应 Minix3 的 `NR_BOOT_MODULES` = `INIT_PROC_NR + 1`
pub const NR_BOOT_MODULES: usize = LAST_SPECIAL_PROC_NR + 1;

