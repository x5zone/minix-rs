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
/// 被 PM、VM、VFS、Kernel 共用
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VirBytes(pub u64);

impl VirBytes {
    /// 创建新虚拟字节数
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 获取值
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl core::ops::Add for VirBytes {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl core::ops::Sub for VirBytes {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl core::ops::Div for VirBytes {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self(self.0 / rhs.0)
    }
}

impl core::ops::Rem for VirBytes {
    type Output = Self;
    fn rem(self, rhs: Self) -> Self::Output {
        Self(self.0 % rhs.0)
    }
}

impl core::ops::Add<u64> for VirBytes {
    type Output = Self;
    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl core::ops::Sub<u64> for VirBytes {
    type Output = Self;
    fn sub(self, rhs: u64) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl PartialOrd<u64> for VirBytes {
    fn partial_cmp(&self, other: &u64) -> Option<core::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialEq<u64> for VirBytes {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

/// 物理地址（64 位无符号整数）
///
/// 被 VM、Kernel 使用
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysBytes(pub u64);

impl PhysBytes {
    /// 创建新物理字节数
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 获取值
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 时间戳（64 位有符号整数）
pub type Time = i64;

/// 文件偏移（64 位有符号整数）
pub type Off = i64;
