//! Process number type definitions.
//!
//! Provides the process number type (`ProcNr`) — the kernel process table
//! slot numbering used by the kernel, the boot protocol, and the arch layer.
//!
//! # C Correspondence
//!
//! - Type: `proc_nr_t` — `typedef int proc_nr_t` (`minix3/minix/kernel/type.h:9`,
//!   "process table entry number"); stored as the `p_nr` field
//!   (`minix3/minix/kernel/proc.h:25`).
//! - Constants: the boot-time process numbers are shared system-wide in
//!   `minix3/minix/include/minix/com.h:59-78` (PM_PROC_NR=0 … INIT_PROC_NR=11).
//!
//! # Value Range
//!
//! Kernel tasks use negative numbers (ASYNCM=-5, IDLE=-4, CLOCK=-3,
//! SYSTEM=-2, KERNEL=-1); user processes are >= 0. These are slot numbers,
//! not dynamic IDs — the process *identity* seen by user space is the
//! [`Endpoint`] (slot + generation).
//!
//! # Relation to the Slot Family
//!
//! - [`Endpoint`]: wire identity for IPC addressing (slot + generation).
//! - [`UserSlot`]: index into server-local tables (mproc/fproc/vmproc).
//! - [`KernelSlot`]: index into the kernel process table array.
//! - `ProcNr`: the C `p_nr` slot *number* (kernel tasks negative). It is the
//!   shared single source for the kernel (`crate::proc`) and the arch layer's
//!   `CpuContextArch::build_cpu_context` parameter — previously each crate
//!   defined its own (kernel newtype vs arch `pub type ProcNr = i32` alias),
//!   which defeated the newtype protection at the kernel→arch boundary.

use core::fmt;

/// Process number type (corresponds to C's `proc_nr_t`).
///
/// Newtype wrapper providing type safety — prevents accidental mixing of
/// process numbers with raw `i32` values. The inner `i32` is accessible via
/// `.0` for `AtomicI32` interop (`p_nextready`) and array indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ProcNr(pub i32);

impl ProcNr {
    /// Create a process number from a raw `i32`.
    pub const fn new(val: i32) -> Self { ProcNr(val) }
}

impl From<i32> for ProcNr {
    fn from(val: i32) -> Self { ProcNr(val) }
}
impl From<ProcNr> for i32 {
    fn from(nr: ProcNr) -> Self { nr.0 }
}

impl core::ops::Neg for ProcNr {
    type Output = ProcNr;
    fn neg(self) -> ProcNr { ProcNr(-self.0) }
}
impl core::ops::Add for ProcNr {
    type Output = ProcNr;
    fn add(self, rhs: ProcNr) -> ProcNr { ProcNr(self.0 + rhs.0) }
}
impl core::ops::Sub for ProcNr {
    type Output = ProcNr;
    fn sub(self, rhs: ProcNr) -> ProcNr { ProcNr(self.0 - rhs.0) }
}

impl fmt::Display for ProcNr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::format;

    #[test]
    fn test_proc_nr_newtype_is_transparent_4byte() {
        // repr(transparent) + i32 payload: same size/alignment as the C
        // `typedef int proc_nr_t` (kernel/type.h:9), so struct layouts that
        // embed process numbers stay wire-compatible.
        assert_eq!(core::mem::size_of::<ProcNr>(), 4);
        assert_eq!(core::mem::align_of::<ProcNr>(), 4);
    }

    #[test]
    fn test_proc_nr_construction_and_conversion_roundtrip() {
        // ProcNr(x) / .0 / From both directions must agree.
        let nr = ProcNr(8);
        assert_eq!(nr.0, 8);
        assert_eq!(ProcNr::new(8), nr);
        assert_eq!(ProcNr::from(8), nr);
        assert_eq!(i32::from(nr), 8);
    }

    #[test]
    fn test_proc_nr_negative_kernel_task_numbers() {
        // Kernel tasks have negative slot numbers (CLOCK=-3, SYSTEM=-2,
        // KERNEL=-1); ordering must follow the raw i32 order.
        assert!(ProcNr(-3) < ProcNr(-2));
        assert!(ProcNr(-1) < ProcNr(0));
        assert_eq!((-ProcNr(3)).0, -3);
    }

    #[test]
    fn test_proc_nr_arithmetic() {
        // Add/Sub operate on the slot numbers and return a ProcNr, matching
        // the kernel's slot-index arithmetic (e.g. MIN_TASK_NR offsets).
        assert_eq!(ProcNr(2) + ProcNr(6), ProcNr(8));
        assert_eq!(ProcNr(8) - ProcNr(6), ProcNr(2));
    }

    #[test]
    fn test_proc_nr_display() {
        assert_eq!(format!("{}", ProcNr(8)), "8");
        assert_eq!(format!("{}", ProcNr(-3)), "-3");
    }
}
