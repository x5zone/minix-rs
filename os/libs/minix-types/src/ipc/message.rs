//! IPC message structure definitions.
//!
//! Minix3 uses fixed-size messages for inter-process communication.

use crate::types::Endpoint;

/// Message size (bytes).
pub const MESSAGE_SIZE: usize = 56;

/// IPC message.
///
/// All inter-process communication in Minix3 uses this message structure.
///
/// # Memory Layout
/// ```text
/// | Field     | Size    | Offset |
/// |-----------|---------|--------|
/// | m_source  | 4 bytes | 0      |
/// | m_type    | 4 bytes | 4      |
/// | m_u       | 48 bytes| 8      |
/// | Total     | 56 bytes|        |
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Message {
    /// Message sender endpoint.
    pub m_source: Endpoint,
    /// Message type (positive=request, negative=response/error).
    pub m_type: i32,
    /// Message payload.
    pub m_u: MessageUnion,
}

/// Message payload union.
///
/// Contains multiple message formats, select the appropriate format based on `m_type`.
#[derive(Clone, Copy)]
#[repr(C)]
pub union MessageUnion {
    /// Format 1: Mixed types (int + pointer).
    pub m_m1: MessageM1,
    /// Format 2: Mixed types (int + long).
    pub m_m2: MessageM2,
    /// Format 3: Mixed types (int + char array).
    pub m_m3: MessageM3,
    /// Format 4: Pure long types.
    pub m_m4: MessageM4,
    /// Format 5: Mixed types (char + int + long).
    pub m_m5: MessageM5,
    /// Raw bytes.
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

/// Message format 1: Mixed types.
///
/// Used for syscalls that need to pass pointers (e.g. read/write).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM1 {
    /// Integer argument 1.
    pub m1i1: i32,
    /// Integer argument 2.
    pub m1i2: i32,
    /// Integer argument 3.
    pub m1i3: i32,
    /// Pointer argument 1 (64-bit).
    pub m1p1: u64,
    /// Pointer argument 2 (64-bit).
    pub m1p2: u64,
    /// Pointer argument 3 (64-bit).
    pub m1p3: u64,
}

/// Message format 2: Mixed types.
///
/// Used for syscalls that need to pass long type arguments.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM2 {
    /// Integer argument 1.
    pub m2i1: i32,
    /// Integer argument 2.
    pub m2i2: i32,
    /// Integer argument 3.
    pub m2i3: i32,
    /// Long argument 1.
    pub m2l1: i64,
    /// Long argument 2.
    pub m2l2: i64,
}

/// Message format 3: Mixed types.
///
/// Used for syscalls that need to pass strings/paths (e.g. open).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM3 {
    /// Integer argument 1.
    pub m3i1: i32,
    /// Integer argument 2.
    pub m3i2: i32,
    /// Integer argument 3.
    pub m3i3: i32,
    /// Character array (pathname, etc.).
    pub m3ca1: [u8; 24],
}

/// Message format 4: Pure long types.
///
/// Used for syscalls that only need to pass long type arguments.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM4 {
    /// Long argument 1.
    pub m4l1: i64,
    /// Long argument 2.
    pub m4l2: i64,
    /// Long argument 3.
    pub m4l3: i64,
    /// Long argument 4.
    pub m4l4: i64,
    /// Long argument 5.
    pub m4l5: i64,
}

/// Message format 5: Mixed types.
///
/// Used for syscalls that need to pass multiple type arguments.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MessageM5 {
    /// Character array.
    pub m5c1: [u8; 8],
    /// Integer argument 1.
    pub m5i1: i32,
    /// Integer argument 2.
    pub m5i2: i32,
    /// Integer argument 3.
    pub m5i3: i32,
    /// Integer argument 4.
    pub m5i4: i32,
    /// Long argument 1.
    pub m5l1: i64,
}
