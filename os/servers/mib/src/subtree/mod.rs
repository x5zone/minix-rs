//! MIB subsystem subtrees: kern, vm/hw, minix.
//!
//! 13 owns `kern`; 14 owns `vm` + `hw`; 15 owns `minix`. The four wiring
//! calls in 04 plug these tables into the top slots.
//!
//! 13-mib-subtree-kern.md + companions.

pub mod hw;
pub mod kern;
pub mod minix;
pub mod vm;

pub use minix::{
    ABSENT_LWIP, MIB_STAT_IDS, MINIX_SLOT_IDS, MINIX_TEST_SUBTREE, MibStat, MinixSlot,
    PROC_DOOR_IDS, ProcDoor, SECRET_ENTRY, SecretEntry, TEST_ENTRIES, TestEntry, TestKind,
};

pub use hw::{
    HW_ENTRIES, HW_UNIMPLEMENTED, HwEntry, HwFunc, HwKind, MACH, MACHINE_ARCH, clamp_u32,
    find_entry as find_hw_entry, is_narrow_door, physmem_bytes, usermem_minus_kernel,
};

pub use kern::{
    CpTimeMode, FORKFSLEEP_MAX_MS, KERN_ENTRIES, KERN_UNIMPLEMENTED, KernEntry, KernFunc, KernKind,
    KernVerify, MAXSLP, clock_tick_us, cp_time_mode, cp_time_namelen_ok,
    find_entry as find_kern_entry, forkfsleep_ok, ipc_info_namelen_ok, pty_alias_needed,
    securelvl_ok, truncate_len,
};
pub use vm::{
    LOAD_FSCALE, LOAD_HISTORY, LOAD_MINUTES, LOAD_UNIT_SECS, VM_ENTRIES, VM_UNIMPLEMENTED, VmEntry,
    VmFunc, VmKind, avg_x100, find_entry as find_vm_entry, page_shift, slots_for, ticks_per_slot,
    unfilled_ticks,
};
