//! 时间和地址类型定义
//!
//! 64 位系统专用类型映射

/// 时钟滴答数（64 位有符号整数）
///
/// 在 64 位系统中，`clock_t` 是 8 字节
pub type Clock = i64;

/// 虚拟地址/字节数（64 位无符号整数）
///
/// 在 64 位系统中，指针和 `size_t` 都是 8 字节
pub type VirBytes = u64;

/// 物理地址（64 位无符号整数）
pub type PhysBytes = u64;

/// 时间戳（64 位有符号整数）
pub type Time = i64;

/// 文件偏移（64 位有符号整数）
pub type Off = i64;
