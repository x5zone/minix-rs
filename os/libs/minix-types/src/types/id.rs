//! 用户和组 ID 类型定义
//!
//! 提供 UID/GID 及其三元组（real/effective/saved）类型

use core::fmt;

/// 用户 ID（32 位无符号整数）
pub type Uid = u32;

/// 组 ID（32 位无符号整数）
pub type Gid = u32;

/// ID 三元组（real / effective / saved）
///
/// 用于存储 UID 或 GID 的三种状态：
/// - `real`: 真实 ID，标识进程的实际所有者
/// - `effective`: 有效 ID，用于权限检查
/// - `saved`: 保存的 ID，用于 setuid/setgid 恢复
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct IdSet<T> {
    pub real: T,
    pub effective: T,
    pub saved: T,
}

impl<T: Default> Default for IdSet<T> {
    fn default() -> Self {
        Self {
            real: T::default(),
            effective: T::default(),
            saved: T::default(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for IdSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IdSet {{ real: {}, effective: {}, saved: {} }}",
            self.real, self.effective, self.saved
        )
    }
}

/// 用户 ID 三元组
pub type UidSet = IdSet<Uid>;

/// 组 ID 三元组
pub type GidSet = IdSet<Gid>;
