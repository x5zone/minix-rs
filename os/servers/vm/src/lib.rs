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
pub(crate) mod heap_arena;
pub(crate) mod page_cache;
pub(crate) mod vfs_queue;
pub(crate) mod exit;
pub(crate) mod brk;
pub(crate) mod munmap;
pub(crate) mod cow_exec_pf;

pub use vm_server::VmServer;

pub(crate) use global::*;
pub(crate) use vmproc::*;
pub(crate) use acl::{AclState, AclMask};
pub(crate) use fork::{ForkError, fork_region, fork_regions, cow_copy_page};
pub(crate) use alloc_stats::VmAllocStats;
pub(crate) use critical_pool::CriticalPool;
pub use phys_mem::BootMemRegion;

pub(crate) use phys_mem::{
    PhysAlloc, PhysAllocator, PhysMemStats, PageAllocFlags, AlignedPhysBytes, AllocError,
};
pub(crate) use region::{VirRegion, VrFlags, PageState, PageFrames, PageSlot, PageFlags as PhysPageFlags, PFN_NONE, PfnAllocator, PfnAllocError, RegionMap};
pub(crate) use pagetable::{PageTable, PageFlags, PageTableError, vm_self_mappages, vm_self_unmappages, vm_self_unmap, vm_self_query, init_vm_self_pt};
pub(crate) use memtype::{
    MemType, MemTypeError, PagefaultResult,
    AnonymousMemory, DirectPhysical, SharedMemory,
    ContiguousAnonymous, CacheMemory, MappedFile,
    MEM_TYPE_ANON, MEM_TYPE_DIRECT, MEM_TYPE_SHARED,
    MEM_TYPE_CONTIG_ANON, MEM_TYPE_CACHE, MEM_TYPE_MAPPED_FILE,
};
pub(crate) use ipc::*;
pub(crate) use direct_map::{VM_DIRECT_MAP_BASE, KERNEL_DIRECT_MAP_BASE, VM_HEAP_BASE, VM_HEAP_SIZE, VM_HEAP_LIMIT, vm_phys_to_virt, kernel_phys_to_virt, virt_to_phys};
pub(crate) use alloc_page::VmPageAllocator;
pub(crate) use page_cache::PageCache;
pub(crate) use vfs_queue::VfsRequestQueue;
pub(crate) use exit::{VmExitError, handle_vm_exit, handle_vm_willexit};
pub(crate) use brk::{BrkError, BrkRequest, BrkResponse, handle_brk};
pub(crate) use munmap::{MunmapError, MunmapRequest, handle_munmap};
pub(crate) use cow_exec_pf::{
    PagefaultAction, CowError,
    handle_pagefault, alloc_and_map, cow_resolve, cow_resolve_region,
};

mod vm_server;
