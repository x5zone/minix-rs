#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub(crate) mod global;
pub(crate) mod vmproc;
pub(crate) mod acl;
pub(crate) mod fork;
pub(crate) mod alloc_stats;
pub(crate) mod critical_pool;
pub(crate) mod phys_mem;
pub(crate) mod region;
pub(crate) mod pagetable;
pub(crate) mod memtype;
pub(crate) mod ipc;
pub(crate) mod direct_map;
pub(crate) mod alloc_page;

pub(crate) use global::*;
pub(crate) use vmproc::*;
pub(crate) use acl::{AclState, AclMask};
pub(crate) use fork::{VmForkRequest, VmForkResponse, VmForkError, ForkContext, handle_fork};
pub(crate) use alloc_stats::VmAllocStats;
pub(crate) use critical_pool::CriticalPool;
pub(crate) use phys_mem::{
    PhysAllocator, PhysAllocatorStats, PhysMemStats, PageAllocFlags, PhysBytes, AllocError,
};
pub(crate) use region::{VirRegion, VrFlags, PhysRegion, PhysBlock, RegionAvl};
pub(crate) use pagetable::{PageTable, PageFlags, PageTableError};
pub(crate) use memtype::{
    MemType, MemTypeError, PagefaultResult,
    AnonymousMemory, DirectPhysical, SharedMemory,
    MEM_TYPE_ANON, MEM_TYPE_DIRECT, MEM_TYPE_SHARED,
};
pub(crate) use ipc::*;
pub(crate) use direct_map::{DIRECT_MAP_BASE, vm_phys_to_virt, kernel_phys_to_virt, virt_to_phys};
pub(crate) use alloc_page::{VmPageAllocator, ReservedRegion};
