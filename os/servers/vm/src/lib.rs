#![cfg_attr(not(test), no_std)]

//! Minix VM (Virtual Memory Manager)
//!
//! Standalone VM service process. All internal types are `pub(crate)` —
//! VM is an independent user-space process, not a library for other crates.
//! External processes (PM, VFS, RS) interact with VM via IPC messages,
//! not by linking against this crate.
//!
//! # Architecture
//!
//! ```text
//! PM ──IPC──> VM dispatcher ──> fork / exit / brk / mmap ...
//! VFS ──IPC──>                ──> VmProcTable ──> typestate views
//! RS ──IPC──>                 ──> PageTable / Region / PhysMem
//! ```
//!
//! # State Machine
//!
//! ```text
//! EmptySlot ──[activate]──> ActiveProc ──[mark_exiting]──> ExitingProc ──[reap]──> EmptySlot
//!                           ActiveProc ──[force_clear]──────────────────────────> EmptySlot
//! ```

extern crate alloc;

pub(crate) mod global;
pub(crate) mod vmproc;
pub(crate) mod acl;
pub(crate) mod fork;
pub(crate) mod slab;
pub(crate) mod phys_mem;
pub(crate) mod region;
pub(crate) mod pagetable;
pub(crate) mod memtype;
pub(crate) mod ipc;

pub(crate) use global::*;
pub(crate) use vmproc::*;
pub(crate) use acl::{AclState, AclMask};
pub(crate) use fork::{VmForkRequest, VmForkResponse, VmForkError, ForkContext, handle_fork};
pub(crate) use slab::{SlabCache, SlabStats, LeakReport, MockPageAllocator};
pub(crate) use phys_mem::{PhysMemAllocator, AllocFlags, PhysAddr, MemStats, CLICK_SIZE, CLICK_SHIFT};
pub(crate) use region::{VirRegion, VrFlags, PhysRegion, PhysBlock, RegionAvl};
pub(crate) use pagetable::{PageTable, PtFlags, PageTableError, PAGE_SIZE, PT_ENTRIES, PD_ENTRIES};
pub(crate) use memtype::{
    MemType, MemTypeError, PagefaultResult,
    AnonymousMemory, DirectPhysical, SharedMemory,
    MEM_TYPE_ANON, MEM_TYPE_DIRECT, MEM_TYPE_SHARED,
};
pub(crate) use ipc::*;
