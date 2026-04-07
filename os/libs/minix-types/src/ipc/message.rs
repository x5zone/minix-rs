//! IPC 消息结构定义
//!
//! Minix3 使用固定大小的消息进行进程间通信

use crate::types::Endpoint;

/// 消息大小（字节）
pub const MESSAGE_SIZE: usize = 56;

/// IPC 消息
///
/// Minix3 中所有进程间通信都通过此消息结构
///
/// # 内存布局
/// ```text
/// | 字段      | 大小    | 偏移 |
/// |-----------|---------|------|
/// | m_source  | 4 bytes | 0    |
/// | m_type    | 4 bytes | 4    |
/// | m_u       | 48 bytes| 8    |
/// | 总计      | 56 bytes|      |
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Message {
    /// 消息发送者端点
    pub m_source: Endpoint,
    /// 消息类型（正数=请求，负数=响应/错误）
    pub m_type: i32,
    /// 消息负载
    pub m_u: MessageUnion,
}

/// 消息负载联合体
///
/// 包含多种消息格式，根据 `m_type` 选择合适的格式
#[derive(Clone, Copy)]
#[repr(C)]
pub union MessageUnion {
    /// 格式 1：混合类型（int + pointer）
    pub m_m1: MessageM1,
    /// 格式 2：混合类型（int + long）
    pub m_m2: MessageM2,
    /// 格式 3：混合类型（int + char array）
    pub m_m3: MessageM3,
    /// 格式 4：纯 long 类型
    pub m_m4: MessageM4,
    /// 格式 5：混合类型（char + int + long）
    pub m_m5: MessageM5,
    /// 原始字节
    pub raw: [u8; 48],
}

impl Default for MessageUnion {
    fn default() -> Self {
        Self { raw: [0u8; 48] }
    }
}

impl core::fmt::Debug for MessageUnion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "MessageUnion {{ ... }}")
    }
}

/// 消息格式 1：混合类型
///
/// 用于需要传递指针的系统调用（如 read/write）
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM1 {
    /// 整数参数 1
    pub m1i1: i32,
    /// 整数参数 2
    pub m1i2: i32,
    /// 整数参数 3
    pub m1i3: i32,
    /// 指针参数 1（64 位）
    pub m1p1: u64,
    /// 指针参数 2（64 位）
    pub m1p2: u64,
    /// 指针参数 3（64 位）
    pub m1p3: u64,
}

/// 消息格式 2：混合类型
///
/// 用于需要传递 long 类型参数的系统调用
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM2 {
    /// 整数参数 1
    pub m2i1: i32,
    /// 整数参数 2
    pub m2i2: i32,
    /// 整数参数 3
    pub m2i3: i32,
    /// long 参数 1
    pub m2l1: i64,
    /// long 参数 2
    pub m2l2: i64,
}

/// 消息格式 3：混合类型
///
/// 用于需要传递字符串/路径的系统调用（如 open）
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM3 {
    /// 整数参数 1
    pub m3i1: i32,
    /// 整数参数 2
    pub m3i2: i32,
    /// 整数参数 3
    pub m3i3: i32,
    /// 字符数组（路径名等）
    pub m3ca1: [u8; 24],
}

/// 消息格式 4：纯 long 类型
///
/// 用于只需要传递 long 类型参数的系统调用
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM4 {
    /// long 参数 1
    pub m4l1: i64,
    /// long 参数 2
    pub m4l2: i64,
    /// long 参数 3
    pub m4l3: i64,
    /// long 参数 4
    pub m4l4: i64,
    /// long 参数 5
    pub m4l5: i64,
}

/// 消息格式 5：混合类型
///
/// 用于需要传递多个类型参数的系统调用
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM5 {
    /// 字符数组
    pub m5c1: [u8; 8],
    /// 整数参数 1
    pub m5i1: i32,
    /// 整数参数 2
    pub m5i2: i32,
    /// 整数参数 3
    pub m5i3: i32,
    /// 整数参数 4
    pub m5i4: i32,
    /// long 参数 1
    pub m5l1: i64,
}
