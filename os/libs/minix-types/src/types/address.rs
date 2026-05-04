//! Memory address type definitions.
//!
//! 64-bit address types for virtual and physical memory.

/// Virtual address/byte count (64-bit unsigned integer).
///
/// On 64-bit systems, pointers and `size_t` are both 8 bytes.
/// Shared by PM, VM, VFS, Kernel.
// TODO(XZHAO): Add overflow checking for address arithmetic on 64-bit systems.
// Current implementations use wrapping arithmetic which may silently overflow.
// This should be addressed when focusing on memory/address related modules.
//
// TECH DEBT: `pub u64` field allows constructing sentinel values like
// `VirBytes(0)` / `PhysBytes(0)` to mean "invalid address" (C-style MAP_NONE).
// Rust should use `Option<PhysBytes>` instead. Mid-term goal: change to
// `pub(crate) u64` with `new()` constructor + `as_u64()` accessor.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VirBytes(pub u64);

impl VirBytes {
    /// Creates a new virtual byte count.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Gets the value.
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

/// Physical address (64-bit unsigned integer).
///
/// Used by VM, Kernel.
///
/// TECH DEBT: `pub u64` field allows constructing sentinel values like
/// `PhysBytes(0)` to mean "no mapping" (C-style MAP_NONE). Rust should
/// use `Option<PhysBytes>` instead. Mid-term goal: change to `pub(crate) u64`
/// with `new()` constructor + `as_u64()` accessor.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysBytes(pub u64);

impl PhysBytes {
    /// Creates a new physical byte count.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Gets the value.
    pub const fn get(self) -> u64 {
        self.0
    }
}
