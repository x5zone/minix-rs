//! Memory address type definitions.
//!
//! 64-bit address types for virtual and physical memory.

/// Virtual address/byte count (64-bit unsigned integer).
///
/// On 64-bit systems, pointers and `size_t` are both 8 bytes.
/// Shared by PM, VM, VFS, Kernel.
///
/// TECH DEBT: `pub u64` field allows constructing sentinel values like
/// `VirBytes(0)` / `PhysBytes(0)` to mean "invalid address" (C-style MAP_NONE).
/// Rust should use `Option<PhysBytes>` instead. Mid-term goal: change to
/// `pub(crate) u64` with `new()` constructor + `as_u64()` accessor.
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

    /// Checked addition. Returns `None` on overflow.
    ///
    /// Use for safety-critical address arithmetic where overflow
    /// would indicate a logic error (e.g., region size calculation).
    /// Basic `Add` trait impls remain for ergonomic use in contexts
    /// where overflow is impossible by construction (e.g., adding
    /// page offsets within the 48-bit address space).
    pub const fn checked_add(self, rhs: u64) -> Option<Self> {
        match self.0.checked_add(rhs) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked subtraction. Returns `None` on underflow.
    pub const fn checked_sub(self, rhs: u64) -> Option<Self> {
        match self.0.checked_sub(rhs) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }
}

/// A page-sized physical memory frame — a 4 KiB-aligned physical
/// address denoting one page of physical memory.
///
/// *Different from an address*: a frame is a unit of physical memory
/// that an allocator hands out (bootstrap: `VmBootAllocator`; runtime:
/// VM PMM). The owner is determined by context, not by the type — do
/// not encode "owned by PMM" here.
///
/// C: nothing — Minix3 has no such type. This is minix-rs's type-level
/// semantic for "one page of physical memory".
///
/// NOTE (`PhysFrame::SIZE`, 4 KiB): `minix-types` is an
/// architecture-independent crate. All currently supported architectures
/// (x86-64 / AArch64 / RISC-V64) use 4 KiB base pages, so the constant
/// is safe for now. If a different base page size is ever supported,
/// introduce a `PageSize` trait — not before (over-engineering).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysFrame {
    start: PhysBytes,
}

impl PhysFrame {
    /// Frame size in bytes — the base-page size of all supported
    /// minix-rs architectures (4 KiB). See type-level NOTE.
    pub const SIZE: u64 = 0x1000;

    /// Creates a frame from a physical address, assuming 4 KiB alignment.
    /// Callers must guarantee the alignment invariant.
    pub const fn new(start: PhysBytes) -> Self {
        Self { start }
    }

    /// Starting physical address of this frame.
    pub const fn start(self) -> PhysBytes {
        self.start
    }

    /// Whether `addr` lies within this frame's `[start, start+SIZE)`.
    pub const fn contains(self, addr: PhysBytes) -> bool {
        addr.0 >= self.start.0 && addr.0 < self.start.0 + Self::SIZE
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

    /// Checked addition. Returns `None` on overflow.
    pub const fn checked_add(self, rhs: u64) -> Option<Self> {
        match self.0.checked_add(rhs) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked subtraction. Returns `None` on underflow.
    pub const fn checked_sub(self, rhs: u64) -> Option<Self> {
        match self.0.checked_sub(rhs) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }
}
