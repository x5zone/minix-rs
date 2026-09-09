//! VM Server main structure.
//!
//! Contains the VmServer struct with initialization phases
//! and main loop. Corresponds to Minix3's init_vm() + main loop.
//!
//! # Initialization Phases
//!
//! 1. **Phase 1 - Memory detection**: Initialize global state with total page count
//! 2. **Phase 2 - Process table**: Set up boot processes from kernel boot image
//! 3. **Phase 3 - Page tables**: Initialize kernel page tables and direct map (reserved)
//!
//! # Main Loop
//!
//! The main loop receives IPC messages and dispatches them:
//! - VM requests (fork, brk, munmap, exit, etc.) → `MessageDispatcher`
//! - VFS replies → `VfsRequestQueue::handle_reply`
//! - Page faults → `cow_exec_pf::handle_pagefault`

use minix_types::{Endpoint, UserSlot, BootImage, NR_BOOT_PROCS, VmPagefaultIn, VmProcctlIn, Message, VmReply, VmError, VM_RQ_BASE};
use crate::ipc::encode::{encode_reply_data, reply_to_errno, VmReplyForIpc};
#[cfg(test)]
use minix_types::VM_PROCCTL;
#[cfg(test)]
use minix_types::{VmForkIn, VmBrkIn, VmExitIn};
use crate::vmproc::VmProcTable;
use crate::alloc_page::VmPageAllocator;
use crate::phys_mem::{PhysAlloc, PhysAllocType, BitmapAllocator, PhysAllocator, BootMemRegion, AlignedPhysBytes, bytes_to_clicks, CLICK_SIZE};
#[cfg(feature = "buddy_alloc")]
use crate::phys_mem::BuddyAllocator;
#[cfg(all(feature = "buddy_alloc", not(feature = "segment_tree_alloc")))]
use crate::phys_mem::BUDDY_THRESHOLD_PAGES;
#[cfg(feature = "segment_tree_alloc")]
use crate::phys_mem::SegmentTreeAllocator;
use crate::boot::{BootParams, KernelAllocated, VM_PROC_NR};
use crate::page_cache::PageCache;
use crate::vfs_queue::VfsRequestQueue;
use crate::ipc::dispatcher::MessageDispatcher;
use crate::ipc::transport::IpcStatus;
#[cfg(not(test))]
use crate::pagetable::vm_self_map::init_vm_self_pt;
use crate::direct_map::vm_phys_to_virt;
use crate::region::PageFrames;
use minix_types::PhysBytes;
#[cfg(test)]
use minix_types::VirBytes;

/// The memory subsystem state the server drives: the physical-page
/// allocator, the frame table, the page cache, and the pending VFS request
/// queue. These four live and die with the server and are needed together
/// at every dispatch site.
///
/// V9-P1-3 step 1 (02-stage-vm todo): grouping them makes that invariant
/// structural — callers destructure `&mut VmContext` into disjoint `&mut`
/// fields (the borrow checker enforces the split) — and retires the
/// former `parts_mut()` 4-tuple, whose existence was a borrow-checker
/// workaround rather than a design statement. Later steps extend this
/// type toward the full server context (proc table, boot parameters).
///
/// C has no counterpart: the four roles are globals in C (`vm_pagetable`
/// / `phys_blocks` / `cache_list` / VFS request state, glo.h + region.h);
/// minix-rs groups them so ownership is explicit in one place.
pub(crate) struct VmContext {
    pub(crate) proc_table: &'static VmProcTable,
    pub(crate) page_alloc: VmPageAllocator,
    pub(crate) page_frames: Option<PageFrames>,
    pub(crate) page_cache: PageCache,
    pub(crate) vfs_queue: VfsRequestQueue,
    /// Kernel-call gateway (V11/T9 step 2): the single exit point for
    /// kernel syscalls (sys_fork today; safecopy/update/exec/diag join
    /// with their consumers). `Rc<RefCell<…>>` follows the transport
    /// pattern (V10-P0-2): tests keep a mock behind the same shape the
    /// production `TrapKernelGateway` uses.
    pub(crate) gateway: alloc::rc::Rc<
        core::cell::RefCell<alloc::boxed::Box<dyn crate::kernel_gateway::KernelGateway>>,
    >,
    /// C: `kernel_boot_info.kernel_allocated_bytes(_dynamic)` (glo.h) —
    /// consumed by the kernel usage query (region.c:1357-1364).
    pub(crate) kernel_allocated: KernelAllocated,
    /// C: `kernel_boot_info.vm_allocated_bytes` — consumed by the VM-self
    /// usage query (region.c:1366-1373).
    pub(crate) vm_allocated_bytes: u64,
    /// Failed pagefault handlings ([ARCH: A-15], saturating).
    pub(crate) pagefault_errors: u64,
    /// Main-loop dropped-message count ([ARCH: A-14], saturating).
    pub(crate) dropped_messages: u64,
    /// V11/T16 (`sanity_checks` feature): dispatches since the last
    /// refcount verification. C: SANITYCHECKS 周期校验 (alloc.c)。
    #[cfg(feature = "sanity_checks")]
    pub(crate) sanity_ticks: u32,
}

impl VmContext {
    fn new(
        page_alloc: VmPageAllocator,
        kernel_allocated: KernelAllocated,
        vm_allocated_bytes: u64,
    ) -> Self {
        let gateway: alloc::rc::Rc<
            core::cell::RefCell<alloc::boxed::Box<dyn crate::kernel_gateway::KernelGateway>>,
        > = alloc::rc::Rc::new(core::cell::RefCell::new(
            alloc::boxed::Box::new(crate::kernel_gateway::TrapKernelGateway {
                transport: minix_sys::syscall::DirectKernelCallTransport,
            }),
        ));
        // V11/T15: audit records route through this gateway (SYS_DIAGCTL).
        #[cfg(all(not(test), feature = "vm_acl_audit"))]
        crate::audit::register_gateway(gateway.clone());

        Self {
            proc_table: VmProcTable::get_global(),
            gateway,
            page_alloc,
            page_frames: None,
            page_cache: PageCache::new(),
            vfs_queue: VfsRequestQueue::new(),
            // Field semantics are documented on the struct definition;
            // counters start saturated-proof at zero.
            kernel_allocated,
            vm_allocated_bytes,
            pagefault_errors: 0,
            dropped_messages: 0,
            #[cfg(feature = "sanity_checks")]
            sanity_ticks: 0u32,
        }
    }

    pub(crate) fn usage_sources(&self) -> crate::query::UsageSources {
        crate::query::UsageSources {
            kernel_bytes: self
                .kernel_allocated
                .static_bytes
                .saturating_add(self.kernel_allocated.dynamic_bytes),
            vm_self_bytes: self.vm_allocated_bytes
                .saturating_add(
                    (self.page_alloc.self_page_count() as u64)
                        * crate::region::page_state::PAGE_SIZE,
                ),
        }
    }
}

pub struct VmServer {
    pub(crate) ctx: VmContext,
    initialized: bool,
    /// Count of kernel pagefault messages whose handling failed.
    ///
    /// V9-P1-1 (todo): the main loop previously dropped the
    /// `dispatch_pagefault` result (`let _ =`), so CoW / allocation /
    /// region-lookup failures were unobservable in release builds. Each
    /// `VmReply::Error` from a pagefault is now counted here and surfaced
    /// through the feature-gated audit channel (same `vm_acl_audit` feature
    /// as ACL denials). Saturating so a pathological fault storm cannot
    /// wrap the counter.
    ///
    /// C has no direct equivalent: Minix3's pagefault path (pagefaults.c)
    /// does not audit failures either; this is a minix-rs observability
    /// extension ([ARCH: A-15]).
    /// Boot process images, copied at construction.
    ///
    /// C: `kernel_boot_info.boot_procs[]` (main.c:497-520). Copied from
    /// [`BootParams`] so `init()` does not borrow external memory; the
    /// kernel→VM boot protocol is a one-shot hand-off.
    boot_procs: [BootImage; NR_BOOT_PROCS],
    /// Kernel layout from the boot handoff (V11/E3): `None` = pre-E3
    /// handoff/test shape → `init_global_state` keeps the mock constants.
    kernel_layout: Option<minix_types::KernelLayout>,
    /// Pages to charge to the global page total during init.
    ///
    /// C: `mem_add_total_pages()` call points (main.c:485-495).
    boot_extra_pages: usize,
    /// IPC transport for the main loop.
    ///
    /// `Rc<RefCell<...>>` (V10-P0-2, V9-P1-2): the previous process-global
    /// `AtomicPtr` + `Box::into_raw` slot always constructed a
    /// `KernelIpcTransport` (even in tests), never marked it initialized,
    /// and leaked the allocation — the main loop was untestable and would
    /// busy-loop on `Err(Unimplemented)` in production. The shared handle
    /// lets tests keep a clone to drive and inspect a mock transport while
    /// the server owns the sole production instance; VM is single-threaded
    /// (lib.rs), so `Rc`/`RefCell` is sound.
    transport: alloc::rc::Rc<
        core::cell::RefCell<alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>>,
    >,
}

impl VmServer {
    pub fn new(total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
        Self::new_with_boot_params(BootParams::simple(total_pages, free_regions))
    }

    /// Production constructor — takes the full kernel→VM boot contract.
    ///
    /// C: `main()` (main.c:93-104) — the boot info is retrieved by
    /// `sys_getkinfo()` and consumed by `init_vm()`. `BootParams::validate()`
    /// mirrors the two `init_vm()` asserts (main.c:451-452).
    pub fn new_with_boot_params(params: BootParams<'_>) -> Self {
        Self::new_inner(params, Self::kernel_transport())
    }

    /// Test constructor: inject a mock transport so the main loop can be
    /// driven end-to-end on the host (V10-P0-2). The boot contract is the
    /// simplified `BootParams::simple` form used by `new()`.
    #[cfg(test)]
    fn new_for_test(
        total_pages: usize,
        free_regions: &[BootMemRegion],
        transport: alloc::rc::Rc<
            core::cell::RefCell<alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>>,
        >,
    ) -> Self {
        Self::new_inner(BootParams::simple(total_pages, free_regions), transport)
    }

    /// Production transport handle: a single shared `KernelIpcTransport`.
    fn kernel_transport() -> alloc::rc::Rc<
        core::cell::RefCell<alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>>,
    > {
        alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(
            crate::ipc::transport::KernelIpcTransport::new(),
        )))
    }

    fn new_inner(
        params: BootParams<'_>,
        transport: alloc::rc::Rc<
            core::cell::RefCell<alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>>,
        >,
    ) -> Self {
        params.validate();

        let phys_alloc = Self::create_default_allocator(params.total_pages, params.free_regions);
        let mut page_alloc = VmPageAllocator::new(phys_alloc);
        crate::global::register_page_alloc(&mut page_alloc);

        // Register the page-table-page allocator before any `Paging::new()` /
        // `map()` call. C: `pt_ptalloc` draws page-table pages from
        // `vm_allocpage` (pagetable.c:515); the VM-side hook
        // (`alloc_page::vm_pt_alloc`) supplies them from the page allocator
        // via the Direct Map ([ARCH: A-1], 06-page-allocator.md §3.2).
        // The `is_registered()` guard mirrors os/kernel/src/lib.rs:178 —
        // production never has a prior registration in the VM address space,
        // but repeated `VmServer::new()` in tests must not panic.
        if !minix_arch::pt_alloc::is_registered() {
            minix_arch::pt_alloc::register(crate::alloc_page::vm_pt_alloc);
        }

        // Skip init_vm_self_pt() in test builds — tests use MockPaging
        // (in-memory mapping table, no page table to initialize), while the
        // production path must establish VM's own page table before any heap
        // allocation (HeapArena::grow → vm_self_mappages).
        // A1 adoption: the root comes from the boot handoff
        // (`BootParams::root_paddr`, read from the handoff page in `main`)
        // — VM adopts the bootstrap root the kernel built, it does not
        // create a fresh one.
        #[cfg(not(test))]
        init_vm_self_pt(params.root_paddr);

        // Copy the boot process list (C: kernel_boot_info.boot_procs[]).
        let mut boot_procs = [BootImage::empty(); NR_BOOT_PROCS];
        for (i, img) in params.boot_procs.iter().take(NR_BOOT_PROCS).enumerate() {
            boot_procs[i] = *img;
        }
        let kernel_layout = params.kernel_layout;

        Self {
            ctx: VmContext::new(page_alloc, params.kernel_allocated, params.vm_allocated_bytes),
            initialized: false,
            boot_procs,
            boot_extra_pages: params.extra_pages(),
            kernel_layout,
            transport,
        }
    }

    fn create_default_allocator(total_pages: usize, free_regions: &[BootMemRegion]) -> PhysAlloc {
        // [ARCH: A-5] — bootstrap backend is always the bitmap; the
        // strategy switch (bitmap/buddy/segment-tree) happens later in
        // relocate() (see plan.md §4 A-5, 05-physical-memory.md §3.3).
        let meta_size = BitmapAllocator::metadata_size(total_pages);
        let meta_pages = bytes_to_clicks(meta_size);

        let meta_region = free_regions.iter()
            .find(|r| {
                (r.base as u64) < crate::direct_map::VM_DIRECT_MAP_SIZE
                && r.size >= meta_pages * CLICK_SIZE
            })
            .expect("no free region in Direct Map range large enough for allocator metadata");

        let meta_phys_base = meta_region.base;
        // V11/T26: one path for both profiles — `vm_phys_to_virt` is
        // window-aware in test builds too (per-thread window base), so the
        // former cfg split (arch global read vs funnel) is unnecessary.
        let meta_va = vm_phys_to_virt(AlignedPhysBytes::new(meta_phys_base as u64));
        // SAFETY: meta_va points to a valid direct-mapped physical region
        // of meta_size bytes; no aliasing references exist.
        let metadata = unsafe {
            core::slice::from_raw_parts_mut(meta_va.0 as *mut u8, meta_size)
        };

        let adjusted_base = meta_phys_base + meta_pages * CLICK_SIZE;
        let adjusted_size = meta_region.size.saturating_sub(meta_pages * CLICK_SIZE);
        let adjusted_regions = [BootMemRegion { base: adjusted_base, size: adjusted_size }];

        PhysAlloc::Bitmap(BitmapAllocator::init(metadata, total_pages, &adjusted_regions, meta_phys_base as u64, meta_pages))
    }

    /// Select the boot-time allocator backend ([ARCH: A-5]).
    ///
    /// This function is the **single precedence authority** (the contradictory
    /// `DefaultAllocator` alias in `phys_mem/mod.rs` was removed in V11-P1-3):
    ///
    /// - `segment_tree_alloc` selects the segment-tree backend outright,
    ///   including when the `buddy_alloc` feature is also enabled (V10-P0-1
    ///   wired the previously-never-selected backend; V11-P1-3 pinned the
    ///   combined-feature semantics with tests).
    /// - `buddy_alloc` (without segment-tree) keeps the documented adaptive
    ///   threshold — at or below `BUDDY_THRESHOLD_PAGES` (1M pages ≈ 4GB) the
    ///   compact bitmap is used even when the feature is enabled
    ///   (05-physical-memory.md §3.3).
    /// - with no backend feature, the bitmap backend is used.
    ///
    /// The `cfg` blocks are structured so every feature combination compiles
    /// to a body without unreachable code: the segment-tree check comes first
    /// and the buddy check is compiled only when segment-tree is absent.
    /// `total_pages` is consumed only in the buddy-threshold branch, hence
    /// the mirrored `allow` condition.
    #[cfg_attr(
        not(all(feature = "buddy_alloc", not(feature = "segment_tree_alloc"))),
        allow(unused_variables)
    )]
    fn choose_allocator_type(total_pages: usize) -> PhysAllocType {
        #[cfg(feature = "segment_tree_alloc")]
        {
            PhysAllocType::SegmentTree
        }
        #[cfg(not(feature = "segment_tree_alloc"))]
        {
            #[cfg(feature = "buddy_alloc")]
            {
                if total_pages > BUDDY_THRESHOLD_PAGES {
                    return PhysAllocType::Buddy;
                }
            }
            PhysAllocType::Bitmap
        }
    }

    fn relocate(&mut self) {
        let (total_pages, old_pa_base, old_pa_pages) = {
            let phys_alloc = self.ctx.page_alloc.phys_alloc();
            let bitmap = phys_alloc.as_bitmap().expect("relocate: bootstrap allocator must be Bitmap");
            let (pa_base, pa_pages) = bitmap.metadata_pa_range();
            assert!(pa_pages > 0, "relocate: no BumpBuf metadata to relocate (already relocated?)");
            (bitmap.total_count(), pa_base, pa_pages)
        };

        let alloc_type = Self::choose_allocator_type(total_pages);

        let meta_size = alloc_type.metadata_size(total_pages);
        let pages = bytes_to_clicks(meta_size);
        let new_va = crate::global::heap_arena_grow(pages, &mut self.ctx.page_alloc)
            .expect("relocate: failed to allocate new metadata via HeapArena");
        // SAFETY: new_va points to a valid heap-arena region of meta_size bytes;
        // no aliasing references exist.
        let new_metadata = unsafe {
            core::slice::from_raw_parts_mut(new_va as *mut u8, meta_size)
        };

        let mut free_regions: alloc::vec::Vec<BootMemRegion> = alloc::vec![];
        {
            let phys_alloc = self.ctx.page_alloc.phys_alloc();
            phys_alloc.available_regions(&mut |base_page, num_pages| {
                free_regions.push(BootMemRegion {
                    base: base_page * CLICK_SIZE,
                    size: num_pages * CLICK_SIZE,
                });
            });
        }

        // Each backend is cfg-gated on its feature; a requested backend
        // whose feature is disabled falls back to Bitmap (the bootstrap
        // allocator), so `PhysAllocType` stays feature-independent while
        // `PhysAlloc` construction matches the compiled-in backend.
        //
        // V12-P1-1: under the current `choose_allocator_type` the fallback
        // arms are UNREACHABLE — it only names a backend whose feature is
        // on, so each `#[cfg(not(...))]` fallback never runs in any feature
        // combination. They keep the match exhaustive without making
        // `PhysAllocType` cfg-dependent; do not read them as live paths.
        let new_alloc = match alloc_type {
            PhysAllocType::Bitmap => {
                PhysAlloc::Bitmap(BitmapAllocator::init(new_metadata, total_pages, &free_regions, 0, 0))
            }
            PhysAllocType::Buddy => {
                #[cfg(feature = "buddy_alloc")]
                {
                    PhysAlloc::Buddy(BuddyAllocator::init(new_metadata, total_pages, &free_regions))
                }
                #[cfg(not(feature = "buddy_alloc"))]
                {
                    PhysAlloc::Bitmap(BitmapAllocator::init(new_metadata, total_pages, &free_regions, 0, 0))
                }
            }
            PhysAllocType::SegmentTree => {
                #[cfg(feature = "segment_tree_alloc")]
                {
                    PhysAlloc::SegmentTree(SegmentTreeAllocator::init(new_metadata, total_pages, &free_regions))
                }
                #[cfg(not(feature = "segment_tree_alloc"))]
                {
                    PhysAlloc::Bitmap(BitmapAllocator::init(new_metadata, total_pages, &free_regions, 0, 0))
                }
            }
        };

        // V12-P1-1: the relocation's backend decision is otherwise
        // unobservable in production — one audit line makes "asked for X,
        // got Y" diagnosable after the fact.
        audit_log!(
            "[VM alloc] relocate: {} backend over {} pages (metadata {} bytes)",
            alloc_type.name(),
            total_pages,
            meta_size
        );

        {
            let phys_alloc = self.ctx.page_alloc.phys_alloc_mut();
            *phys_alloc = new_alloc;
            let old_pa = AlignedPhysBytes::new(old_pa_base);
            phys_alloc.free_mem(old_pa, old_pa_pages);
        }
    }

    pub fn init(&mut self) {
        // C: init_vm() — main.c:428-584. Step order mirrors C exactly:
        //
        //   main.c:442      sys_getkinfo; main.c:451-452 asserts → BootParams::validate() (new_with_boot_params)
        //   main.c:455      get_mem_chunks; main.c:471 mem_init → VmServer::new (allocator construction)
        //   main.c:457-462  memset(vmproc) + vm_slot    → compile-time vacant slots + get_empty()
        //   main.c:465      acl_init()                  → AclState::Uninitialized (compile-time default)
        //   main.c:468      map_region_init()           → RegionMap::new() lazily per process
        //   main.c:474-475  init_proc(VM_PROC_NR)+pt_init → init_vm_slot() + init_vm_self_pt() (new)
        //   main.c:480      __minix_init()              → mark_initialized() (V10-P0-2; real trap wiring → E1)
        //   main.c:485-495  mem_add_total_pages()       → account_boot_memory()
        //   main.c:497-520  boot procs (exec_bootproc)  → init_boot_procs() + exec_bootproc (V11/T14; stack frame → E-BOOTFRAME)
        //   main.c:522-572  CALLMAP                     → compile-time match (MessageDispatcher)
        //   main.c:577-579  VM instance mark            → mark_vm_instance()
        //
        // Relocation requires real page tables (vm_self_mappages); skipped in tests.
        #[cfg(not(test))]
        self.relocate();

        // Phase 1: Memory detection — initialize global state with total page count.
        self.init_global_state();

        // Phase 2a: init_proc(VM_PROC_NR) — main.c:474.
        self.init_vm_slot();

        // Phase 2b: mem_add_total_pages() call points — main.c:485-495.
        self.account_boot_memory();

        // Phase 2c: boot process slots — main.c:497-520 (exec_bootproc
        // landed in V11/T14; the initial stack frame ABI waits on edge
        // E-BOOTFRAME).
        self.init_boot_procs();

        // Phase 2d: VM instance mark — main.c:577-579.
        self.mark_vm_instance();

        // Phase 3: PageFrames after total_pages is known.
        let total_phys = PhysBytes(self.ctx.page_alloc.total_pages() as u64 * crate::region::PAGE_SIZE);
        self.ctx.page_frames = Some(PageFrames::new(total_phys));

        // V11/T30: the reclaim-retry funnel needs kernel-visible access to
        // the cache/frames pair (C alloc_mem reads the global cache). Same
        // registration lifetime as the audit gateway and page allocator.
        if let Some(frames) = self.ctx.page_frames.as_mut() {
            crate::global::register_reclaim(&mut self.ctx.page_cache, frames);
        }

        // C: __minix_init() (main.c:480) — SEF startup makes the IPC
        // channel ready before the main loop. V10-P0-2: without this, a
        // `KernelIpcTransport` stays uninitialized and `run()` would
        // busy-loop on `Err(Unimplemented)` instead of blocking on
        // `sef_receive_status`.
        self.transport.borrow_mut().mark_initialized();

        self.initialized = true;
    }

    fn init_global_state(&mut self) {
        // SAFETY: init() must be called exactly once during VM startup.
        unsafe {
            crate::global::init(self.ctx.page_alloc.total_pages());
        }

        // Initialize the kernel memory layout used by `init_page_table()`.
        //
        // Previously (hardcoded kernel layout fix), these constants were hardcoded in
        // `vmproc_handle.rs::init_page_table()` and guarded by a
        // `hardcoded_kernel_layout` feature flag with a `compile_error!`.
        // That approach acknowledged the values were wrong for real hardware
        // but provided no way to supply correct values. Now the layout is
        // set here, once, during `VmServer::init()`.
        //
        // TODO (boot-info integration): Replace these mock values with real
        // values parsed from the multiboot2 / stivale2 boot headers or linker
        // symbols. The kernel text physical base, text/data sizes, and direct
        // map size all come from boot-time information. Until then, these mock
        // values match the previous hardcoded constants so behavior is
        // unchanged, but the mechanism is now in place to supply correct
        // values without touching `init_page_table()`.
        //
        // V11/E3: a version ≥ 3 handoff carries the real kernel layout —
        // consume it. `None` (pre-E3 handoffs, host tests via
        // `BootParams::simple`) keeps the historical mock constants.
        let layout = self.kernel_layout.unwrap_or_else(|| {
            audit_log!("[VM BOOT] handoff lacks kernel layout (version < 3) — mock constants in use");
            minix_types::KernelLayout::new(
                0xFFFF_FFFF_8000_0000, // kernel_text_vbase (mock)
                0x100_0000,            // kernel_text_pbase (16 MiB, mock)
                8,                     // kernel_text_pages (mock)
                8,                     // kernel_data_pages (mock)
                0xFFFF_8000_0000_0000, // dm_vbase (KERNEL_DIRECT_MAP_BASE)
                4,                     // dm_pages (sentinel only)
            )
        });
        // SAFETY: Called before any `init_page_table()` call (process table
        // setup happens in `init_proc_table()` after this). Single-threaded
        // VM ensures no concurrent reader of `KERNEL_LAYOUT`.
        unsafe {
            crate::global::set_kernel_layout(layout);
        }
    }

    /// init_proc(VM_PROC_NR) — main.c:262-283, called at main.c:474.
    fn init_vm_slot(&self) {
        let table = self.ctx.proc_table; // V12-P2-5: ctx 字段即全局别名，语法统一
        if let Some(ip) = self.boot_procs.iter().find(|ip| ip.proc_nr == VM_PROC_NR) {
            Self::init_proc(table, *ip);
        }
    }

    /// Boot process slots — main.c:497-520.
    ///
    /// C also runs `exec_bootproc()` + `free_mem()` per boot process here;
    /// both are implemented (V11/T14 — ELF segment loading through the
    /// Direct Map + `Gateway::sys_exec`, blob freed back to the allocator).
    /// The minimal initial stack frame is edge E-BOOTFRAME (VM↔libc↔kernel
    /// shared ABI), so `stack`/`ps_str` report 0 until it lands; real
    /// kernel traffic for `sys_exec` waits on edge E2.
    fn init_boot_procs(&mut self) {
        let table = self.ctx.proc_table; // V12-P2-5: 语法统一（同对象）
        // Own a copy: exec_bootproc needs &mut self while iterating.
        let boot_procs = self.boot_procs;
        for ip in &boot_procs {
            // C: main.c:502 — skip kernel tasks (negative proc_nr).
            // Rust additionally skips padding entries: `boot_procs` is copied
            // into a fixed `[BootImage; NR_BOOT_PROCS]` array, so empty slots
            // have `endpoint == NONE` and must not be treated as processes
            // (the C array is exactly filled, minix-types' is not).
            if ip.proc_nr < 0 || ip.proc_nr == VM_PROC_NR || ip.endpoint.is_none() {
                continue;
            }
            // C: main.c:504 — assert(ip->start_addr) for non-VM boot procs.
            assert!(
                ip.start_addr != 0,
                "init_boot_procs: boot proc {} has no start_addr",
                ip.name()
            );
            Self::init_proc(table, *ip);

            // C: main.c:509 — exec_bootproc(vmp, ip) per user boot proc.
            #[cfg(not(test))]
            self.exec_bootproc(ip)
                .unwrap_or_else(|e| panic!("exec_bootproc: {} failed: {e}", ip.name()));

            // C: main.c:513-516 — the boot blob is consumed; free it back
            // to the allocator (page-aligned, length rounded up).
            #[cfg(not(test))]
            {
                let pages = ip.len.div_ceil(crate::region::PAGE_SIZE) as usize;
                self.ctx.page_alloc.free_pages(
                    crate::phys_mem::AlignedPhysBytes::new(ip.start_addr),
                    pages,
                );
            }
        }
    }

    /// V11/T14: `exec_bootproc` (C: main.c:331-426) — load a boot image
    /// process's ELF segments into its fresh address space and hand the
    /// entry point to the kernel.
    ///
    /// # Scope (honest split)
    ///
    /// **Implemented here**: `pt_new`/`pt_bind` equivalent (`init_page_table`,
    /// already run by `init_proc`), image read through the VM Direct Map
    /// (`start_addr` is physical), PT_LOAD segment regions with eagerly
    /// materialized pages carrying the segment bytes, and the `sys_exec`
    /// notification (`Gateway::sys_exec`; kernel `dispatch_exec` is real).
    ///
    /// **Edge E-BOOTFRAME** (edge_todo.md): the minimal initial stack frame
    /// (C `minix_stack_params`/`minix_stack_fill` — argv/envp/ps_strings
    /// byte-exact ABI, consumed by minix3 libc crt0 and the kernel's
    /// `arch_proc_init`). That frame is a VM↔libc↔kernel shared contract;
    /// until it lands, `stack`/`ps_str` are reported as 0 and the boot
    /// proc's user start is gated on it.
    ///
    /// # C reference
    ///
    /// ```c
    /// static void exec_bootproc(struct vmproc *vmp, struct boot_image *ip)
    /// {
    ///     ...libexec_load_elf(execi) with physcopy allocators...  // segments
    ///     minix_stack_params/fill(...)                             // frame
    ///     sys_exec(endpoint, vsp, progname, execi->pc, ps_str)     // kernel
    /// }
    /// ```
    #[cfg(not(test))]
    fn exec_bootproc(&mut self, ip: &minix_types::BootImage) -> Result<(), &'static str> {
        const PS: usize = crate::region::PAGE_SIZE as usize;
        use crate::region::page_state::PfnAllocator as _;

        let table = self.ctx.proc_table; // V12-P2-5: 语法统一（同对象）
        let slot = table
            .vm_isokendpt(ip.endpoint)
            .map_err(|_| "boot proc endpoint not registered")?;

        // C: sys_physcopy(NONE, ip->start_addr, SELF, hdr, ...) — the image
        // is physical memory owned by the boot handoff; the Direct Map
        // window makes it directly readable (no copy needed, unlike C).
        let image_len = ip.len as usize;
        let image_va = crate::direct_map::vm_phys_to_virt(
            crate::phys_mem::AlignedPhysBytes::new(ip.start_addr),
        );
        // SAFETY: [image_va, image_va+image_len) is the boot module blob the
        // kernel handed over; VM owns it exclusively (C frees it right after
        // exec_bootproc, main.c:514-516). The Direct Map window covers all
        // physical memory; the handoff guarantees page alignment.
        let image: &[u8] = unsafe {
            core::slice::from_raw_parts(image_va.0 as *const u8, image_len)
        };

        // Parse + walk PT_LOAD segments (C: libexec_load_elf → elf_exec_hdr).
        let entry = minix_elf::entry_point(image)
            .map_err(|_| "boot image is not a valid ELF")?;
        // SegmentIter yields PT_LOAD segments only (p_type filter inside).
        let segments: alloc::vec::Vec<minix_elf::LoadSegment> =
            minix_elf::segment_iter(image)
                .map_err(|_| "boot image phdrs unreadable")?
                .collect();

        let frames = self
            .ctx
            .page_frames
            .as_mut()
            .ok_or("page_frames not initialized")?;

        let mut proc = table
            .get_active(slot)
            .ok_or("boot proc slot not active")?;

        for seg in segments.iter() {
            let seg_len = seg.memsz;
            if seg_len == 0 {
                continue;
            }
            let pages = (seg_len as usize).div_ceil(PS);
            let vaddr = minix_types::VirBytes(seg.vaddr);

            // Region per segment (C: libexec_alloc_vm_prealloc → map_page_region).
            let region = crate::region::VirRegion::with_memtype(
                vaddr,
                minix_types::VirBytes((pages * PS) as u64),
                crate::region::VrFlags::ANON,
                &crate::memtype::MEM_TYPE_ANON,
            );
            proc.regions_mut()
                .insert(region)
                .map_err(|_| "boot segment region overlap")?;

            // Materialize pages eagerly and copy segment bytes through the
            // Direct Map (C: libexec_copy_physcopy per page).
            let pfn_alloc = &mut self.ctx.page_alloc;
            let seg_vr = proc.regions_mut().find_mut(vaddr).unwrap();
            for i in 0..pages {
                let offset = minix_types::VirBytes((i * PS) as u64);
                let pfn = pfn_alloc
                    .alloc_pfn()
                    .map_err(|_| "boot segment page allocation failed")?;
                seg_vr.map_page(
                    frames,
                    offset,
                    pfn,
                    &crate::memtype::MEM_TYPE_ANON,
                );

                // Copy file bytes into the fresh physical page.
                let dst_phys = pfn as u64 * PS as u64;
                let dst_va = crate::direct_map::vm_phys_to_virt(
                    crate::phys_mem::AlignedPhysBytes::new(dst_phys),
                );
                let file_off = i as u64 * PS as u64;
                let copy_len = core::cmp::min(
                    PS as u64,
                    seg.filesz.saturating_sub(file_off),
                ) as usize;
                if copy_len > 0 {
                    let src_off = (seg.offset + file_off) as usize;
                    // SAFETY: destination is the freshly allocated page
                    // through the Direct Map; source is the boot image
                    // slice; both bounds-checked above.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            image.as_ptr().add(src_off),
                            dst_va.0 as *mut u8,
                            copy_len,
                        );
                    }
                    // BSS remainder (memsz > filesz): zero-fill (allocator
                    // zero-fills new pages in this codebase, but be explicit
                    // for partial pages whose file part ends mid-page).
                    let tail = PS - copy_len;
                    if tail > 0 {
                        unsafe {
                            core::ptr::write_bytes(
                                (dst_va.0 as *mut u8).add(copy_len),
                                0,
                                tail,
                            );
                        }
                    }
                }
            }
        }

        // E-BOOTFRAME: stack/ps_str reported as 0 until the initial-stack
        // ABI lands; the kernel treats them as "no ps_strings" (matches
        // the boot-gated state — these procs are not user-runnable yet).
        let mut gateway = self.ctx.gateway.borrow_mut();
        gateway
            .sys_exec(ip.endpoint, entry, 0, 0, 0)
            .map_err(|_| "sys_exec rejected by kernel")?;
        Ok(())
    }

    /// C: init_proc() — main.c:262-283.
    fn init_proc(table: &'static VmProcTable, ip: BootImage) {
        // C: main.c:272-273 — proc_nr range check (panics like C).
        let slot = UserSlot(ip.proc_nr as usize);
        let empty = table
            .get_empty(slot)
            .expect("init_proc: slot already in use");
        // C: clear_proc() is compile-time in Rust (vacant slot); activate()
        // sets VMF_INUSE + vm_endpoint (main.c:277-280).
        let mut proc = empty.activate(ip.endpoint);
        proc.set_boot(ip);
    }

    /// mem_add_total_pages() call points — main.c:485-495.
    fn account_boot_memory(&self) {
        if self.boot_extra_pages == 0 {
            return;
        }
        // SAFETY: init() runs exactly once, single-threaded, before any
        // concurrent reader of the global total (see global::add_total_pages).
        unsafe {
            crate::global::add_total_pages(self.boot_extra_pages);
        }
    }

    /// Mark the VM slot as a VM instance — main.c:577-579.
    fn mark_vm_instance(&self) {
        let table = VmProcTable::get_global();
        if let Some(mut proc) = table.get_active(UserSlot(VM_PROC_NR as usize)) {
            proc.mark_vm_instance();
        }
    }

    /// Returns the count of failed kernel pagefault handlings (V9-P1-1).
    ///
    /// Every `VmReply::Error` produced while dispatching a `VM_PAGEFAULT`
    /// message increments this counter. Tests assert on it; the audit
    /// channel (`vm_acl_audit` feature) prints each failure.
    pub fn pagefault_errors(&self) -> u64 {
        self.ctx.pagefault_errors
    }

    /// Returns the count of IPC messages dropped at the main-loop boundary
    /// (V9-P0-1, [ARCH: A-14]): receive failures + invalid callers.
    pub fn dropped_messages(&self) -> u64 {
        self.ctx.dropped_messages
    }

    /// C: `alloc_cycle()` (alloc.c:227-237) — main-loop replenishment hook,
    /// invoked whenever the pressure counter is non-zero (main.c:118-119).
    ///
    /// C iterates the in-use reserve queues and calls `reservedqueue_fill()`,
    /// which allocates pages from `alloc_mem()` (alloc.c:149-174); on
    /// exhaustion `alloc_mem` itself retries after `cache_freepages()`
    /// (alloc.c:242-279, cache.c:288).
    ///
    /// Rust design: the reserve queues are eliminated by the Direct Map
    /// (`[ARCH: A-1]`, 06-page-allocator.md §3.3). The cache-reclaim half is
    /// implemented below as a bounded batch (`page_cache.free_pages`);
    /// allocation-time reclaim-retry (the C `alloc_mem` half) lives in
    /// `alloc_page::alloc_pfn_reclaiming` since V11/T30. The counter is
    /// cleared here so the next failure re-arms the hook — the loop always
    /// gets a fresh replenishment attempt per pressure episode.
    /// C: `SIGKMEM` (minix3/sys/sys/signal.h:271) — kernel memory request
    /// pending.
    pub(crate) const SIGKMEM: i32 = 71;

    /// Kernel-signal dispatch — the body of C's `sef_cb_signal_handler`
    /// (main.c:733-749).
    ///
    /// C registers the handler with SEF at startup; real signal delivery
    /// needs the trap layer (edge E1) and only then becomes reachable in
    /// production. The dispatch body itself is stage-internal so its
    /// behavior is testable and E1 only has to call this entry.
    ///
    /// C tail (main.c:744-748): after handling, a pending spare-page
    /// deficit triggers `alloc_cycle()`; `pt_clearmapcache()` has no
    /// counterpart (map cache eliminated, [ARCH: A-1]).
    #[cfg_attr(not(test), allow(dead_code))] // entry wired at edge E1
    pub(crate) fn handle_signal(&mut self, signo: i32) {
        // C: "Check for known kernel signals, ignore anything else."
        if signo == Self::SIGKMEM {
            self.do_memory();
        }
        // V12-P2-4: C's tail here also ran `alloc_cycle()` on a pending
        // `missing_spares` deficit (main.c:118-119) — that chain is deleted
        // (no producer; the spare-pool mechanism it served is structurally
        // eliminated by the Direct Map, [ARCH: A-1], and allocation-time
        // reclaim lives in `alloc_pfn_reclaiming` since V11/T30).
    }

    /// C: `do_memory()` (pagefaults.c:294-339) — drain the kernel's pending
    /// memory requests. Each request is a VMPTYPE_CHECK: confirm the range
    /// is mapped (and writable when requested), resolving CoW along the
    /// way, then report OK/EFAULT back to the kernel.
    ///
    /// Kernel protocol: SYS_VMCTL `VMCTL_MEMREQ_GET` (fetch; ENOENT when
    /// the queue is empty) then `VMCTL_MEMREQ_REPLY` (verdict) — both via
    /// the gateway, so pre-E2 the trap stub's -EIO fail-closes the loop
    /// with an audit line instead of spinning.
    #[cfg_attr(not(test), allow(dead_code))] // reached via handle_signal (E1)
    pub(crate) fn do_memory(&mut self) {
        // Defensive bound: the kernel's request queue is depth-bounded by
        // the process count, so a correct kernel always reaches ENOENT far
        // below this. A transport that keeps answering "valid request"
        // forever (e.g. a misbehaving script) would otherwise hang the VM —
        // the only memory manager — so the drain stops at the bound and
        // audits ([ARCH: A-14] input-trust posture).
        const MAX_MEMREQ_BATCH: usize = 1024;
        for serviced in 0..MAX_MEMREQ_BATCH {
            let req = match self.ctx.gateway.borrow_mut().sys_vmctl_memreq_get() {
                Ok(Some(req)) => req,
                Ok(None) => return,
                Err(e) => {
                    let _ = &e; // audit_log! compiles args away without features
                    audit_log!("[VM SIGKMEM] memreq_get failed: {e:?}");
                    return;
                }
            };

            let ok = self.handle_kernel_memreq(&req);
            if let Err(e) = self.ctx.gateway.borrow_mut().sys_vmctl_memreq_reply(req.target, ok) {
                let _ = &e; // audit_log! compiles args away without features
                audit_log!("[VM SIGKMEM] memreq_reply failed: {e:?}");
                return;
            }
            let _ = serviced;
        }
        audit_log!("[VM SIGKMEM] drain bound {} reached — kernel re-signals", MAX_MEMREQ_BATCH);
    }

    /// C: `VMPTYPE_CHECK` arm of do_memory (pagefaults.c:311-330) —
    /// `handle_memory_start(vmp, mem, len, wrflag, KERNEL, ...)`. The
    /// mapping work itself is `handle_memory_once` (fork.rs), the same
    /// machinery fork uses: walk the range, resolve CoW, allocate fresh
    /// anonymous pages on demand.
    ///
    /// C panics on a bad target endpoint (`do_memory: bad endpoint`);
    /// minix-rs fails closed instead ([ARCH: A-14] — the VM is the only
    /// memory manager, an audit line plus an EFAULT verdict beats a
    /// whole-system halt).
    #[cfg_attr(not(test), allow(dead_code))] // reached via handle_signal (E1)
    fn handle_kernel_memreq(&mut self, req: &crate::kernel_gateway::KernelMemReq) -> bool {
        let table = VmProcTable::get_global();
        let slot = match table.vm_isokendpt(req.target) {
            Ok(s) => s,
            Err(_) => {
                audit_log!("[VM SIGKMEM] bad target endpoint {}", req.target.0);
                return false;
            }
        };

        let VmContext { page_alloc, page_frames, .. } = &mut self.ctx;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        let mut proc = match table.get_active(slot) {
            Some(p) => p,
            None => return false,
        };
        // G-V12-8: the CHECK verdict tells the kernel the range is mapped —
        // the CoW resolution inside must also write the PTEs, or the process
        // re-faults on resume (C: handle_memory_start → pt_writemap).
        let (regions, pt) = proc.mem_parts_mut();
        crate::fork::handle_memory_once(
            regions,
            frames,
            page_alloc,
            minix_types::VirBytes(req.start),
            minix_types::VirBytes(req.length),
            req.write,
            pt,
        )
        .is_ok()
    }

    /// Main event loop. Never returns (C: main.c:113-193).
    ///
    /// Per-iteration work lives in [`Self::run_once`] so tests can drive a
    /// single dispatch→reply round without spawning the infinite loop
    /// (V10-P0-2). The loop owns the receive-failure bound that prevents a
    /// busy-spin when the transport is broken. (C's per-iteration
    /// `if(missing_spares > 0) alloc_cycle()` replenishment hook is deleted
    /// with its chain — V12-P2-4.)
    pub fn run(&mut self) -> ! {
        assert!(self.initialized, "VmServer::run() called before init()");

        let mut consecutive_recv_failures: u32 = 0;
        loop {
            match self.run_once() {
                RunStep::Handled => consecutive_recv_failures = 0,
                RunStep::ReceiveFailed => {
                    // C: sef_receive_status blocks until a message arrives,
                    // so a receive `Err` means the transport itself is
                    // broken, not "no message". Busy-spinning at 100% CPU
                    // would hide the failure (V10-P0-2) — fail fast instead.
                    consecutive_recv_failures = consecutive_recv_failures.saturating_add(1);
                    if consecutive_recv_failures >= MAX_CONSECUTIVE_RECV_FAILURES {
                        panic!(
                            "IPC transport permanently broken: {} consecutive receive failures",
                            consecutive_recv_failures
                        );
                    }
                }
            }
        }
    }

    /// Process exactly one IPC message (or one receive failure).
    ///
    /// Mirrors one iteration of C main.c:113-193. Extracted from `run()`
    /// so tests can drive the main loop one round at a time with a mock
    /// transport (V10-P0-2); `run()` supplies the infinite loop, the
    /// pressure hook, and the receive-failure bound.
    fn run_once(&mut self) -> RunStep {
        // C: sef_receive_status(ANY, &msg, &rcv_sts)
        let (msg, rcv_sts) = match self.transport.borrow_mut().receive() {
            Ok(v) => v,
            // [ARCH: A-14] V9-P0-1: C panics (main.c:122-123); a
            // user-space server must survive bad IPC — drop + audit.
            Err(_) => {
                self.ctx.dropped_messages = self.ctx.dropped_messages.saturating_add(1);
                audit_log!("[VM IPC] ipc_receive() failed — message dropped");
                return RunStep::ReceiveFailed;
            }
        };

        // C: if(is_ipc_notify(rcv_sts)) { continue; } (main.c:126-129).
        // Notifications are async signals, not requests; they are skipped
        // before endpoint validation (V10-P1-1).
        if rcv_sts.is_notify() {
            return RunStep::Handled;
        }

        // C: who_e = msg.m_source; vm_isokendpt(who_e, &caller_slot);
        let who_e = msg.m_source;
        let caller_slot = match VmProcTable::get_global().vm_isokendpt(who_e) {
            Ok(slot) => slot,
            // [ARCH: A-14] V9-P0-1: C panics (main.c:131-132); the
            // caller cannot be serviced either way, but VM must not
            // die with it — drop + audit.
            Err(_) => {
                self.ctx.dropped_messages = self.ctx.dropped_messages.saturating_add(1);
                audit_log!("[VM IPC] invalid caller {:?} — message dropped", who_e);
                return RunStep::Handled;
            }
        };

        let action = self.dispatch_on_msg(&msg, &rcv_sts, caller_slot);

        // C: if(result != SUSPEND) { ipc_send(who_e, &msg); }
        //
        // `VmReplyForIpc` *statically* excludes `VmReply::Suspend`
        // (replacing the earlier `unreachable!("Suspend filtered before
        // reply_to_errno")` panic). `DispatchAction` encodes the three C
        // outcomes (SUSPEND / no-reply / reply-with-payload); at this call
        // site `DispatchAction::Reply(reply)` — where `reply` is *any*
        // `VmReply` — is funneled through `VmReplyForIpc::new(reply)`,
        // which returns `None` for `VmReply::Suspend`. Hitting that case
        // would be a logic bug (a Suspend escaping the dispatch boundary),
        // so we panic with a remediation hint instead of sending a
        // corrupt reply.
        match action {
            DispatchAction::Reply(reply) => {
                let reply_for_ipc = VmReplyForIpc::new(reply)
                    .expect("DispatchAction::Reply carries VmReply::Suspend; \
                             dispatch_on_msg should translate to DispatchAction::Suspend");
                // Clone once (VmReply is `Clone`) instead of cloning twice
                // as the original code did; the wrapper owns the reply.
                let code = reply_to_errno(reply_for_ipc.payload());
                let mut reply_msg = msg;
                reply_msg.m_type = code;
                encode_reply_data(reply_for_ipc.into_payload(), &mut reply_msg);
                self.transport
                    .borrow_mut()
                    .send(who_e, &reply_msg)
                    .unwrap_or_else(|_| panic!("ipc_send() failed"));
            }
            // C: main.c:191 — SUSPEND means "no reply now, resume later".
            // The only producer is the RS_INIT handshake branch (Priority 2
            // above). rs_update never routes here: C's do_rs_update returns
            // SUSPEND merely to suppress the main-loop's *second* reply
            // (it already ipc_send'd OK to the external requester inside
            // the handler, rs.c:201-208); minix-rs sends that one reply
            // through DispatchAction::Reply instead, so this arm stays
            // empty.
            DispatchAction::Suspend => {}
            DispatchAction::NoReply => {}
        }

        // V11/T16 ([ARCH: A-16] family): C runs SANITYCHECKS refcount
        // verification periodically (alloc.c `#if SANITYCHECKS`). Feature-
        // gated so release builds pay nothing; a mismatch is counted and
        // audited, never a panic (diagnostics, not control flow).
        #[cfg(feature = "sanity_checks")]
        {
            self.ctx.sanity_ticks = self.ctx.sanity_ticks.wrapping_add(1);
            if self.ctx.sanity_ticks.is_multiple_of(64)
                && let Some(frames) = self.ctx.page_frames.as_ref()
                && let Err(mismatches) = crate::sanity::verify_refcounts(
                    frames,
                    VmProcTable::get_global(),
                )
            {
                self.ctx.pagefault_errors = self.ctx.pagefault_errors.saturating_add(1);
                audit_log!("[VM SANITY] {} refcount mismatches detected", mismatches.len());
                // audit_log! compiles out without vm_acl_audit; keep the
                // mismatch list alive in every build.
                let _ = &mismatches;
            }
        }
        RunStep::Handled
    }
}

/// Outcome of one [`VmServer::run_once`] iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunStep {
    /// A message was received and handled (dispatched, skipped as a
    /// notification, or dropped as invalid input).
    Handled,
    /// The transport reported a receive error; the message was dropped
    /// and counted. `run()` uses this to bound consecutive failures.
    ReceiveFailed,
}

/// Consecutive receive failures before `run()` treats the transport as
/// permanently broken and panics (V10-P0-2). C's `sef_receive_status`
/// blocks, so errors cannot be transient "no message" conditions.
const MAX_CONSECUTIVE_RECV_FAILURES: u32 = 64;

/// Three reply actions — maps to C main.c:178-191.
///
/// `#[allow(clippy::large_enum_variant)]`: the largest variant is
/// `Reply(VmReply)`, and `VmReply::InfoRegion` carries an inline
/// `[VmRegionInfo; 64]` (~1.5 KiB, see `minix-types/src/ipc/vm.rs`
/// for the rationale). The enum is held by value inside the
/// single-threaded dispatcher hot path, and `VmReply` is `Copy` —
/// boxing the variant would only add an `alloc` dependency for no
/// measurable benefit.
#[allow(clippy::large_enum_variant)]
enum DispatchAction {
    Reply(VmReply),
    Suspend,
    NoReply,
}



impl VmServer {
    /// Five-priority dispatch. C: main.c:137-176.
    fn dispatch_on_msg(
        &mut self,
        msg: &Message,
        rcv_sts: &IpcStatus,
        caller_slot: UserSlot,
    ) -> DispatchAction {
        let m_type = msg.m_type as u32;
        let source = msg.m_source;

        // Priority 1: VFS transid (main.c:143-148)
        if source == VFS_PROC_NR && is_vfs_fs_transid(m_type) {
            let transid = transid_extract(m_type);
            let clean_type = transid_strip(m_type);
            let result = self.handle_vfs_transid(clean_type, transid, msg);
            return DispatchAction::Reply(result);
        }

        // Priority 2: RS_INIT (main.c:149-152). C's do_sef_init_request
        // unpacks mess_rs_init (sef_init.c:193-215); the grant inside it
        // feeds the rproctab copy (main.c:246).
        if m_type == RS_INIT && source == RS_PROC_NR {
            let init = minix_types::RsInit::decode_message(msg);
            match self.rs_handshake(&init) {
                Ok(()) => return DispatchAction::Suspend,
                Err(e) => {
                    // C panics on init failure (main.c:151 "do_sef_init_request
                    // failed!"); minix-rs fails closed at the IPC boundary
                    // ([ARCH: A-14] / V9-P0-1): drop + count + audit, no reply —
                    // RS times out exactly as it would against a dead VM,
                    // minus the whole-system outage.
                    self.ctx.dropped_messages = self.ctx.dropped_messages.saturating_add(1);
                    audit_log!(
                        "[VM RS] handshake failed (gid={}): {:?} — RS_INIT dropped",
                        init.rproctab_gid, e
                    );
                    // audit_log! compiles out without the feature; keep `e`
                    // alive in every build (the variant is part of the drop
                    // contract, surfaced again when a log channel lands).
                    let _ = e;
                    return DispatchAction::NoReply;
                }
            }
        }

        // Priority 3: VM_PAGEFAULT (main.c:153-164)
        if m_type == VM_PAGEFAULT {
            // V13-P3-1: C logs a faked pagefault in release too (printf,
            // main.c:154-157) and then handles it regardless — both sides
            // tolerate the forgery, C just stays observable. `debug_assert!`
            // alone compiles the whole check away in release; evaluating the
            // flag unconditionally keeps the audit line alive there without
            // changing the (tolerant) behavior.
            if !rcv_sts.is_from_kernel() {
                debug_assert!(false, "faked VM_PAGEFAULT from {:?}", source);
                audit_log!("[VM PF] faked pagefault source: {:?}", source);
            }
            let reply = self.dispatch_pagefault(msg);
            // V9-P1-1: never silently drop a pagefault failure — the faulting
            // process stays suspended and would otherwise re-fault forever
            // with no observable signal. Count + audit instead (the counter is
            // also surfaced by the `pagefault_errors()` accessor in tests).
            if let VmReply::Error(e) = reply {
                self.ctx.pagefault_errors = self.ctx.pagefault_errors.saturating_add(1);
                audit_log!(
                    "[VM PF] pagefault failed: err={:?} endpoint={:?} vaddr={:?}",
                    e, source, minix_types::VmPagefaultIn::decode_message(msg).vaddr
                );
                let _ = e;
            }
            return DispatchAction::NoReply;
        }

        // Priority 4: Normal VM calls (main.c:165-176)
        if let Some(c) = callnr(m_type) {
            // C: acl_check(&vmproc[caller_slot], c)
            let table = VmProcTable::get_global();
            // V13-P2-3: `get_active` returns None once the caller is EXITING
            // (or otherwise not active). The old `if let Some(..) && check`
            // shape made that state skip the gate entirely — fail-open, at
            // odds with the default-deny policy this module otherwise
            // enforces ([ARCH: A-11]). An unresolvable caller is now denied
            // exactly like a checked-and-refused one: the gate has no skip
            // lane.
            let acl_denied = match table.get_active(caller_slot) {
                Some(proc) => proc.acl_check(c as u32).is_err(),
                None => true,
            };
            if acl_denied {
                    // FIX (VMA-1): Previously `let _ = (c, source);` silently
                    // dropped the ACL denial event, making production
                    // misbehaviour unobservable. Now we record the denial
                    // through a feature-gated audit channel:
                    //
                    //   * `cargo test`           → eprintln! to test stderr
                    //   * `--features vm_acl_audit` → no_std sink
                    //     (`audit::emit`; formats + drops, output pending
                    //     VM ↔ syslog IPC, see 15-ipc-dispatch.md §3.7)
                    //   * release (no feature)   → compiled out entirely
                    //
                    // The audit feature is intentionally off by default:
                    // VM is `no_std`-only outside test cfg, so the audit
                    // channel is a no_std-compatible sink until IPC-grade
                    // logging lands (V10-P0-1).
                    audit_log!(
                        "[VM ACL] denied: call=0x{:x} source={:?} caller_slot={:?}",
                        c, source, caller_slot
                    );
                    let _ = (c, source);
                    // C reply semantics: main.c:145 initializes `result = ENOSYS`
                    // ("Out of range or restricted calls return this.") and the
                    // ACL-denied path never overwrites it — so the caller sees
                    // ENOSYS, not the internal EPERM that `acl_check` returned.
                    // We mirror that exactly: `AclState::acl_check` still
                    // returns `Err(PermissionDenied)` (EPERM, = C's acl_check),
                    // but the *reply* errno is ENOSYS (NotImplemented).
                    return DispatchAction::Reply(
                        VmReply::Error(VmError::NotImplemented)
                    );
                }
            // C: result = vm_calls[c].vmc_func(&msg);
            let result = MessageDispatcher::dispatch_by_number(c, msg, self);
            // Execute deferred VFS callback if present (C: do_vfs_reply
            // invokes req_callback inline, but Rust's borrow checker
            // requires us to defer it until after the vfs_queue borrow
            // is released).
            if let Some((callback, reply, state)) = result.vfs_callback {
                let _ = callback(self, &reply, &state);
            }
            return match result.reply {
                VmReply::Suspend => DispatchAction::Suspend,
                other => DispatchAction::Reply(other),
            };
        }

        // Priority 5: Invalid request → ENOSYS (main.c:165-166)
        DispatchAction::Reply(VmReply::Error(VmError::NotImplemented))
    }

    /// RS handshake — replaces C's sef_startup() + sef_cb_init_fresh().
    /// C (main.c:237-250): sys_safecopyfrom → map_service for each rprocpub entry.
    fn rs_handshake(&mut self, init: &minix_types::RsInit) -> Result<(), VmError> {
        let table = VmProcTable::get_global();

        // 1. Fetch the rproctab through the RS grant carried by RS_INIT.
        // C (main.c:246): sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, 0,
        // rprocpub, sizeof(rprocpub)) — RS is the granter, not SELF.
        let rproctab = ipc_call_rs_init(init.rproctab_gid)?;

        // 2. Register ACL for each boot service
        // C: for(i=0; i<NR_BOOT_PROCS; i++) if(rprocpub[i].in_use) map_service(&rprocpub[i]);
        for entry in rproctab.iter() {
            if !entry.in_use { continue; }
            let slot = table.vm_isokendpt(entry.endpoint)
                .map_err(|_| VmError::InvalidProcess)?;
            let mut proc = table.get_active(slot)
                .ok_or(VmError::InvalidProcess)?;
            let is_sys = !entry.is_user;
            let mask = Some(crate::acl::AclMask::from_bits_truncate(entry.call_mask as u64));
            // C: acl_set(&vmproc[proc_nr], rpub->vm_call_mask, !IS_RPUB_BOOT_USR(rpub))
            proc.set_acl(crate::acl::AclState::acl_set(is_sys, mask));
        }
        Ok(())
    }

    /// VFS transid dispatch — the resume path for VFS's asynchronous
    /// VM_PROCCTL (HANDLEMEM) conversation. C (main.c:141-148):
    ///
    /// ```c
    /// transid = TRNS_GET_ID(msg.m_type);
    /// if(msg.m_source == VFS_PROC_NR && IS_VFS_FS_TRANSID(transid)) {
    ///     msg.m_type = TRNS_DEL_ID(msg.m_type);
    ///     result = do_procctl(&msg, transid);
    /// }
    /// ```
    ///
    /// Ground-truth wire shape (vfsif.h:79-81, com.h:909-912): the arriving
    /// message's `m_type` IS the transid (`0xB00 | seq`), so `TRNS_DEL_ID`
    /// yields **0** — the original call number never travels in `m_type`;
    /// do_procctl re-reads the procctl parameters from the message body
    /// (the m9 overlay). The `transid` argument itself identifies which
    /// suspended operation to resume — in minix-rs it has no consumer:
    /// VMPPARAM_HANDLEMEM completes synchronously (documented deviation,
    /// 22-vm-exit.md), so this path routes to the same single-shot
    /// dispatch_procctl a fresh request would take.
    fn handle_vfs_transid(
        &mut self,
        _clean_type: u32,
        _transid: i32,
        msg: &Message,
    ) -> VmReply {
        // Decode the procctl request from the message body.
        // C: do_procctl reads VMPCTL_PARAM, VMPCTL_WHO, VMPCTL_M1,
        //    VMPCTL_LEN, VMPCTL_FLAGS from the m9 overlay (param@16/
        //    who@20/m1@24/len@28/flags@32).
        let request = VmProcctlIn::decode_message(msg);

        // The caller is always VFS in this path (P1 gate).
        // C: main.c:142 — msg.m_source == VFS_PROC_NR is the gate.
        let caller = VFS_PROC_NR;

        // Pre-init defense (unreachable past run()): preserve the old
        // graceful InternalError instead of the handler's expect() panic.
        if self.ctx.page_frames.is_none() {
            return VmReply::Error(VmError::InternalError);
        }

        MessageDispatcher::dispatch_procctl(&mut self.ctx, caller, request)
    }

    /// Pagefault dispatch — decodes VmPagefaultIn from Message, delegates to cow_exec_pf.
    /// C (main.c:147-156): do_pagefaults(&msg); continue;
    fn dispatch_pagefault(&mut self, msg: &Message) -> VmReply {
        // minix-rs: the kernel packs vpf_addr/vpf_flags in the dedicated
        // m_vm_pagefault union member (os/kernel/src/page_fault.rs:142-166);
        // the faulting endpoint is m_source (C: pagefaults.c:242).
        let request = VmPagefaultIn::decode_message(msg);
        let table = VmProcTable::get_global();
        let slot = match table.vm_isokendpt(request.endpoint) {
            Ok(s) => s,
            Err(_) => return VmReply::Error(VmError::InvalidProcess),
        };
        let mut proc = match table.get_active(slot) {
            Some(p) => p,
            None => return VmReply::Error(VmError::InvalidProcess),
        };
        let proc_endpoint = proc.endpoint();
        let fault_addr = request.vaddr;
        // C pagefaults.c:109-119 — a write to a read-only region is not a
        // servable fault: deliver SIGSEGV to the faulting process, clear its
        // kernel pagefault suspension (RTS_PAGEFAULT), and stop. Pre-E2 the
        // trap stub answers -EIO for both calls; the failure is audited and
        // the error reply still counts the episode (G-V12-6).
        // Read-only borrow — ends before the mem_parts_mut split below.
        if request.write
            && !proc.regions().find(fault_addr).is_some_and(|r| r.is_writable())
        {
            if let Err(e) = self.ctx.gateway.borrow_mut()
                .sys_kill(proc_endpoint, minix_types::SIGNAL_SEGMENT_VIOLATION)
            {
                let _ = &e;
                audit_log!("[VM PF] SIGSEGV delivery failed: {e:?}");
            }
            if let Err(e) = self.ctx.gateway.borrow_mut().sys_vmctl_clear_pagefault(proc_endpoint) {
                let _ = &e;
                audit_log!("[VM PF] clear_pagefault failed: {e:?}");
            }
            return VmReply::Error(VmError::AccessViolation);
        }

        // V9-P1-3 step 1: destructure the memory context into disjoint
        // &mut fields instead of the former parts_mut() 4-tuple.
        let VmContext { page_alloc, page_frames, page_cache, vfs_queue, .. } = &mut self.ctx;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        // G-V12-8: the fault path owns the process page table and must keep
        // it in sync with the bookkeeping slot (C: map_pf → pt_writemap).
        let (regions, pt) = proc.mem_parts_mut();
        let region = match regions.find_mut(fault_addr) {
            Some(r) => r,
            None => return VmReply::Error(VmError::InvalidAddress),
        };
        match crate::cow_exec_pf::handle_pagefault(
            proc_endpoint, region, frames, page_alloc,
            fault_addr, request.write, table, page_cache, vfs_queue, pt,
        ) {
            Ok(action) => {
                // V11/T31: fault accounting. Minix3's VM has no fault
                // counters — the fields are a minix-rs extension following
                // Linux getrusage semantics: minor = satisfied without
                // block I/O (fresh zero page, CoW copy, in-place handled);
                // major = the fault needed VFS I/O, counted at enqueue
                // (Linux counts major when I/O is required, not at
                // completion). Access violations are not faults served.
                match action {
                    crate::cow_exec_pf::PagefaultAction::Suspended => proc.inc_major_fault(),
                    crate::cow_exec_pf::PagefaultAction::AccessViolation => {}
                    crate::cow_exec_pf::PagefaultAction::Handled
                    | crate::cow_exec_pf::PagefaultAction::MappedNewPage
                    | crate::cow_exec_pf::PagefaultAction::CowResolved => proc.inc_minor_fault(),
                }
                VmReply::Ok
            }
            Err(_e) => {
                VmReply::Error(VmError::AccessViolation)
            }
        }
    }

    /// Returns mutable references to page_alloc, page_frames, page_cache, and vfs_queue simultaneously.
    /// This avoids double mutable borrow when dispatching VM calls that need multiple components.
    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only accessor
    pub(crate) fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Drop for VmServer {
    fn drop(&mut self) {
        // V11/T15: the audit gateway sink follows the server lifetime
        // (same test-isolation contract as unregister_page_alloc below).
        #[cfg(all(not(test), feature = "vm_acl_audit"))]
        crate::audit::clear_gateway();

        // V11/T30: clear the reclaim sink with the same lifetime contract.
        crate::global::unregister_reclaim();

        // Clear the global page-allocator pointer so a subsequent VmServer
        // (e.g. the next unit test) can register its own allocator without
        // tripping the overwrite guard in register_page_alloc().
        //
        // This implements the contract documented in global.rs ("only
        // cleared by unregister_page_alloc() in VmServer::drop"). In
        // production the VM process lives as long as the server, so the
        // drop path only matters for tests — but the contract must hold.
        crate::global::unregister_page_alloc();
    }
}

/// Check if a message type carries a VFS filesystem transaction ID.
///
/// C: `IS_VFS_FS_TRANSID(type)` in `minix/com.h:912`.
/// ```c
/// #define VFS_TRANSACTION_BASE 0xB00
/// #define IS_VFS_FS_TRANSID(type) (((type) & ~0xff) == VFS_TRANSACTION_BASE)
/// ```
fn is_vfs_fs_transid(m_type: u32) -> bool {
    (m_type & !0xFF) == VFS_TRANSACTION_BASE
}

/// Extract the transaction ID from a VFS transid-encoded message type.
///
/// C: `TRNS_GET_ID(t)` in `minix/vfsif.h:79`.
/// ```c
/// #define TRNS_GET_ID(t)  ((t) & 0xFFFF)
/// ```
fn transid_extract(m_type: u32) -> i32 {
    (m_type & 0xFFFF) as i32
}

/// Strip the transaction ID from a VFS transid-encoded message type.
///
/// C: `TRNS_DEL_ID(t)` in `minix/vfsif.h:81`.
/// ```c
/// #define TRNS_DEL_ID(t)  ((short)((t) >> 16))
/// ```
///
/// Note (V11/T32): for a genuine transid message (`m_type == 0xB00 | seq`)
/// this yields **0** — the original call number does not travel in
/// `m_type`; do_procctl re-reads its parameters from the message body.
fn transid_strip(m_type: u32) -> u32 {
    // C casts to short (i16) which sign-extends.
    ((m_type >> 16) as i16) as u32
}

/// VFS transaction base constant.
/// C: `minix/com.h:909` — `#define VFS_TRANSACTION_BASE 0xB00`
const VFS_TRANSACTION_BASE: u32 = 0xB00;

fn ipc_call_rs_init(_rproctab_gid: i32) -> Result<RprocTab, VmError> {
    // C contract (ground truth: main.c:137-155 + main.c:237-260 + sef_init.c:193):
    //   1. RS *sends* RS_INIT to VM — main.c:149 gates on
    //      `msg.m_source == RS_PROC_NR`; VM never sends RS_INIT itself.
    //   2. The message carries mess_rs_init.rproctab_gid — a grant RS holds
    //      on its public process table (m_rs_init, ipc.h:1858-1867).
    //   3. VM's init callback copies the table with
    //      sys_safecopyfrom(RS_PROC_NR, gid, 0, rprocpub, sizeof(rprocpub))
    //      — the granter is RS_PROC_NR, **not SELF** (main.c:246).
    //   4. map_service(&rprocpub[i]) per in_use entry — the ACL loop in
    //      `rs_handshake`.
    //
    // Step 3's byte decode is **E-RSWIRE** (edge_todo.md): `struct rprocpub`'s
    // byte ABI cannot be pinned from the minix3 subtree in this repository
    // (devmajor_t / bitchunk_t / struct rs_pci are referenced but not defined
    // here), and the layout is an RS↔VM shared contract. Until E-RSWIRE
    // lands this returns NotImplemented — an honest known-unimplemented —
    // replacing the previous fabricated `Ok(RprocTab::empty())`, which made
    // every handshake silently register zero ACLs while looking successful.
    Err(VmError::NotImplemented)
}

// C: com.h:60-61 — VFS_PROC_NR = 1, RS_PROC_NR = 2.
// minix-types Endpoint constants (Endpoint::VFS / Endpoint::RS) carry the
// same values; named constants keep the C call sites greppable.
const VFS_PROC_NR: Endpoint = Endpoint::VFS;
const RS_PROC_NR: Endpoint = Endpoint::RS;
// C: com.h:478 — RS_INIT = RS_RQ_BASE + 20 = 0x700 + 20 = 0x714.
// minix-types `ipc::rs::RS_INIT` carries the same value; this local
// constant keeps the C call site greppable without a minix-types dep
// at this layer (mirrors VFS_PROC_NR/RS_PROC_NR above).
const RS_INIT: u32 = 0x714;
const VM_PAGEFAULT: u32 = 0xCFF;
// C: com.h:769 — NR_VM_CALLS 49. The highest call is VM_RS_PREPARE
// (VM_RQ_BASE + 48), so relative indices are 0..=48. Derived from the
// minix-types constant so the two cannot drift (V11-P3-1).
const NR_VM_CALLS: usize = minix_types::NR_VM_CALLS as usize;

// ==========================================================================
// Helpers
// ==========================================================================

/// C: CALLNUMBER(c) with bounds check. main.c:57-59.
fn callnr(m_type: u32) -> Option<usize> {
    let c = m_type.checked_sub(VM_RQ_BASE)?;
    if (c as usize) < NR_VM_CALLS {
        Some(c as usize)
    } else {
        None
    }
}

/// RprocTab — equivalent to C's struct rprocpub[NR_SYS_PROCS].
#[derive(Clone, Copy)]
struct RprocEntry {
    in_use: bool,
    endpoint: Endpoint,
    call_mask: u32,
    is_user: bool,
}

impl RprocEntry {
    // V11/T9 step 3: unreachable until E-RSWIRE (the fake-success caller
    // `Ok(RprocTab::empty())` died; the wire decoder will construct real
    // entries). Kept + pinned by tests as the D8 stub shape.
    #[allow(dead_code)]
    const EMPTY: Self = Self {
        in_use: false,
        endpoint: Endpoint::NONE,
        call_mask: 0,
        is_user: false,
    };
}

struct RprocTab {
    entries: [RprocEntry; 32],
}

impl RprocTab {
    // V11/T9 step 3: unreachable until E-RSWIRE — see RprocEntry::EMPTY.
    #[allow(dead_code)]
    const fn empty() -> Self {
        Self {
            entries: [RprocEntry::EMPTY; 32],
        }
    }
}

impl core::ops::Deref for RprocTab {
    type Target = [RprocEntry];
    fn deref(&self) -> &[RprocEntry] {
        &self.entries
    }
}

impl VmServer {
    // V10-P2-1: the `handle_*` wrappers were superseded by
    // `dispatch_on_msg` → `MessageDispatcher::dispatch_*` (which routes
    // directly); they had zero callers and are removed.

    pub fn has_pending_vfs_requests(&self) -> bool {
        !self.ctx.vfs_queue.is_empty()
    }

    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only accessors
    pub(crate) fn page_cache(&self) -> &PageCache {
        &self.ctx.page_cache
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn vfs_queue(&self) -> &VfsRequestQueue {
        &self.ctx.vfs_queue
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn vfs_queue_mut(&mut self) -> &mut VfsRequestQueue {
        &mut self.ctx.vfs_queue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::{BootModule, KernelAllocated};

    // V11-P1-3: pin the backend-selection semantics for every feature
    // combination — this function is the single precedence authority since
    // the contradictory `DefaultAllocator` alias was removed.
    #[cfg(not(any(feature = "buddy_alloc", feature = "segment_tree_alloc")))]
    #[test]
    fn test_choose_allocator_no_backend_feature_selects_bitmap() {
        assert_eq!(VmServer::choose_allocator_type(TEST_TOTAL_PAGES), PhysAllocType::Bitmap);
    }

    #[cfg(all(feature = "buddy_alloc", not(feature = "segment_tree_alloc")))]
    #[test]
    fn test_choose_allocator_buddy_adaptive_threshold() {
        // Strictly above the threshold → buddy; at the threshold → bitmap.
        assert_eq!(
            VmServer::choose_allocator_type(crate::phys_mem::BUDDY_THRESHOLD_PAGES + 1),
            PhysAllocType::Buddy
        );
        assert_eq!(
            VmServer::choose_allocator_type(crate::phys_mem::BUDDY_THRESHOLD_PAGES),
            PhysAllocType::Bitmap
        );
    }

    #[cfg(feature = "segment_tree_alloc")]
    #[test]
    fn test_choose_allocator_segment_tree_wins_over_buddy() {
        // Segment-tree selects outright — even when the buddy feature is also
        // enabled (this combination is what the removed alias got backwards).
        assert_eq!(
            VmServer::choose_allocator_type(crate::phys_mem::BUDDY_THRESHOLD_PAGES + 1),
            PhysAllocType::SegmentTree
        );
        assert_eq!(VmServer::choose_allocator_type(TEST_TOTAL_PAGES), PhysAllocType::SegmentTree);
    }

    const TEST_TOTAL_PAGES: usize = 256;

    /// Run a test with the direct-map window pointed at a fresh per-thread
    /// leaked buffer (V11/T26 — the former process-global mock base +
    /// mutex serialization is replaced by thread-local windows).
    fn with_test_mock_base<F: FnOnce()>(f: F) {
        crate::direct_map::with_test_window(TEST_TOTAL_PAGES, f);
    }

    fn test_free_regions() -> [BootMemRegion; 1] {
        [BootMemRegion { base: 0, size: TEST_TOTAL_PAGES * CLICK_SIZE }]
    }

    fn make_test_vm_server() -> VmServer {
        VmServer::new(TEST_TOTAL_PAGES, &test_free_regions())
    }

    #[test]
    fn test_dispatch_vm_unmap_phys_wired() {
        // 21-P1-1 regression: VM_UNMAP_PHYS previously fell through to the
        // dispatch_by_number catch-all (NotImplemented) despite
        // dispatch_unmap_phys existing. Now it routes to do_munmap
        // semantics (C: CALLMAP(VM_UNMAP_PHYS, do_munmap), main.c:540).
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();
            let mut msg = Message::default();
            msg.m_source = Endpoint::MEM;
            msg.m_type = minix_types::VM_UNMAP_PHYS as i32;
            unsafe {
                msg.m_u.m_lsys_vm_unmap_phys = minix_types::ipc::MessLsysVmUnmapPhys {
                    ep: 12345, // invalid endpoint → InvalidProcess
                    vaddr: 0x1000,
                    ..minix_types::ipc::MessLsysVmUnmapPhys::default()
                };
            }
            let result = MessageDispatcher::dispatch_by_number(
                minix_types::VM_UNMAP_PHYS as usize - VM_RQ_BASE as usize,
                &msg,
                &mut server,
            );
            assert!(
                !matches!(result.reply, VmReply::Error(VmError::NotImplemented)),
                "VM_UNMAP_PHYS must be wired, not NotImplemented: {:?}",
                result.reply
            );
            assert!(matches!(result.reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_dispatch_vm_info_what_matches_c_wire() {
        // 26-P0 regression: C wire values are VMIW_STATS=1 / VMIW_USAGE=2 /
        // VMIW_REGION=3 (com.h:732-734) — libsys vm_info_stats/usage/region
        // send exactly these (vm_info.c:15/:29/:46). The decoder must match:
        // a libsys caller sending what=1 must get Stats, not Usage.
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(60);
            unsafe { table.reset_slot(slot); }
            let empty = table.get_empty(slot).unwrap();
            let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
            empty.activate(ep).init_regions();

            let mut msg = Message::default();
            msg.m_source = ep;
            msg.m_type = minix_types::VM_INFO as i32;
            let call = minix_types::VM_INFO as usize - VM_RQ_BASE as usize;

            // what=1 (VMIW_STATS) → Stats reply.
            // SAFETY: union overlay write — MessageM2 layout matches the
            // VM_INFO decode (m2i1=what, m2i2=ep, m2i3=count, m2l2=next).
            unsafe {
                msg.m_u.m_m2 = minix_types::ipc::MessageM2 {
                    m2i1: minix_types::VMIW_STATS,
                    ..Default::default()
                };
            }
            let result = MessageDispatcher::dispatch_by_number(call, &msg, &mut server);
            assert!(
                matches!(result.reply, VmReply::InfoStats { .. }),
                "what=1 (VMIW_STATS) must decode to Stats: {:?}",
                result.reply
            );

            // what=2 (VMIW_USAGE) with invalid target → InvalidProcess.
            // SAFETY: union overlay write (MessageM2 layout as above).
            unsafe {
                msg.m_u.m_m2 = minix_types::ipc::MessageM2 {
                    m2i1: minix_types::VMIW_USAGE,
                    m2i2: 9999,
                    ..Default::default()
                };
            }
            let result = MessageDispatcher::dispatch_by_number(call, &msg, &mut server);
            assert!(
                matches!(result.reply, VmReply::Error(VmError::InvalidProcess)),
                "what=2 (VMIW_USAGE) with invalid ep must be InvalidProcess: {:?}",
                result.reply
            );

            // what=3 (VMIW_REGION) with ep=SELF → replaced by m_source
            // (C: utility.c:141-143). m_source is a valid slot, so a
            // successful empty Region reply proves the replacement.
            // SAFETY: union overlay write (MessageM2 layout as above).
            unsafe {
                msg.m_u.m_m2 = minix_types::ipc::MessageM2 {
                    m2i1: minix_types::VMIW_REGION,
                    m2i2: Endpoint::SELF.get(),
                    m2i3: 64,
                    ..Default::default()
                };
            }
            let result = MessageDispatcher::dispatch_by_number(call, &msg, &mut server);
            assert!(
                matches!(result.reply, VmReply::InfoRegion { .. }),
                "what=3 (VMIW_REGION) with ep=SELF must resolve to m_source: {:?}",
                result.reply
            );

            // Unknown what → InvalidParam (C: utility.c:163).
            // SAFETY: union overlay write (MessageM2 layout as above).
            unsafe {
                msg.m_u.m_m2 = minix_types::ipc::MessageM2 {
                    m2i1: 0,
                    ..Default::default()
                };
            }
            let result = MessageDispatcher::dispatch_by_number(call, &msg, &mut server);
            assert!(
                matches!(result.reply, VmReply::Error(VmError::InvalidParam)),
                "unknown what must be InvalidParam: {:?}",
                result.reply
            );
        });
    }

    #[test]
    fn test_vm_server_new() {
        with_test_mock_base(|| {
            let server = make_test_vm_server();
            assert!(!server.initialized);
            assert!(!server.is_initialized());
        });
    }

    #[test]
    fn test_vm_server_init() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();
            assert!(server.initialized);
            assert!(server.is_initialized());
        });
    }

    #[test]
    #[should_panic(expected = "VmServer::run() called before init()")]
    fn test_vm_server_run_without_init() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.run();
        });
    }

    /// V10-P0-2: end-to-end main-loop round. A `TestIpcTransport` drives
    /// `run_once()` through receive → caller validation → dispatch → reply,
    /// and the reply is observable via the test-side handle.
    #[test]
    fn test_run_once_dispatch_reply_round() {
        with_test_mock_base(|| {
            reset_boot_slots();

            // Boot image: VM itself (slot 8). The caller is registered at a
            // dedicated slot (70) below — the shared proc-table statics race
            // under parallel tests (P2-4), so we avoid boot slots 8/9 here.
            let boot_procs = [vm_boot_image()];
            let regions = test_free_regions();

            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            // Register a valid caller at slot 70 (endpoint encodes slot 70).
            let table = VmProcTable::get_global();
            let caller_slot = UserSlot::new(70);
            unsafe { table.reset_slot(caller_slot); }
            let empty = table.get_empty(caller_slot).unwrap();
            let caller_ep = Endpoint::from_generation_slot(1, 70);
            let mut caller = empty.activate(caller_ep);
            caller.init_regions();
            // Allow all calls: a fresh slot starts Uninitialized (DEFAULT
            // mask only), and VM_INFO is a privileged query in C's ACL
            // (acl.c). System(all) mirrors "trusted caller" in this test.
            caller.set_acl(crate::acl::AclState::System(crate::acl::AclMask::all()));

            // Queue a VM_INFO (what=1 → InfoStats) request from PFS.
            let mut msg = Message::default();
            msg.m_source = caller_ep;
            msg.m_type = minix_types::VM_INFO as i32;
            // SAFETY: union overlay write — VM_INFO decodes m2i1 as `what`.
            unsafe {
                msg.m_u.m_m2 = minix_types::ipc::MessageM2 {
                    m2i1: minix_types::VMIW_STATS,
                    ..Default::default()
                };
            }
            handle.queue_receive(msg, IpcStatus::default());

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);

            let sent = handle.sent();
            assert_eq!(sent.len(), 1, "one reply must be sent");
            assert_eq!(sent[0].0, caller_ep, "reply goes to the caller");
            // InfoStats encodes as OK → errno 0 (C: do_info VMIW_STATS → OK).
            assert_eq!(sent[0].1.m_type, 0, "InfoStats reply errno must be OK(0)");

            reset_boot_slots();
            unsafe { table.reset_slot(caller_slot); }
        });
    }

    /// V13-P2-3: an EXITING caller is refused at the ACL gate with ENOSYS.
    /// `get_active` returns None for IN_USE+EXITING, and the old
    /// `if let Some(..) && check` shape treated that as "skip the check" —
    /// fail-open, contradicting the default-deny policy ([ARCH: A-11]).
    /// The observable contract now: one ENOSYS reply, handler never runs.
    #[test]
    fn test_run_once_exiting_caller_denied_enosys() {
        with_test_mock_base(|| {
            reset_boot_slots();

            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &test_free_regions(), alloc::rc::Rc::clone(&shared));
            server.init();

            // Caller at slot 66 already past VM_WILLEXIT: IN_USE + EXITING,
            // so `get_active` yields None at the gate.
            let table = VmProcTable::get_global();
            let caller_slot = UserSlot::new(66);
            unsafe { table.reset_slot(caller_slot); }
            let empty = table.get_empty(caller_slot).unwrap();
            let caller_ep = Endpoint::from_generation_slot(1, 66);
            let exiting = empty.activate(caller_ep).mark_exiting();
            drop(exiting);

            let mut msg = Message::default();
            msg.m_source = caller_ep;
            msg.m_type = minix_types::VM_INFO as i32;
            handle.queue_receive(msg, IpcStatus::default());

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);

            let sent = handle.sent();
            assert_eq!(sent.len(), 1, "denied caller still gets the C-shaped ENOSYS reply");
            assert_eq!(sent[0].0, caller_ep, "reply goes to the caller");
            assert_eq!(sent[0].1.m_type, minix_types::ENOSYS as i32,
                "EXITING caller must be denied (ENOSYS), not waved through");

            reset_boot_slots();
            unsafe { table.reset_slot(caller_slot); }
        });
    }

    /// V10-P1-1: a notification (IPC_STATUS_CALL == NOTIFY == 4) is skipped
    /// before endpoint validation — it must not be dropped nor answered.
    #[test]
    fn test_run_once_notify_skipped_before_dispatch() {
        with_test_mock_base(|| {
            reset_boot_slots();

            let boot_procs = [vm_boot_image()];
            let regions = test_free_regions();
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            // Source NONE: if the notify were treated as a request it would
            // be dropped as an invalid caller — the skip must happen first.
            let mut msg = Message::default();
            msg.m_source = Endpoint::NONE;
            msg.m_type = 0;
            handle.queue_receive(msg, IpcStatus { flags: 4 /* NOTIFY */ });

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);
            assert_eq!(server.dropped_messages(), 0, "notify must not be counted as dropped");
            assert!(handle.sent().is_empty(), "notify must not produce a reply");

            reset_boot_slots();
        });
    }

    /// V10-P0-2: a receive failure is dropped + counted, and reported as
    /// `RunStep::ReceiveFailed` so `run()` can bound consecutive failures.
    #[test]
    fn test_run_once_receive_failure_counts() {
        with_test_mock_base(|| {
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &test_free_regions(), alloc::rc::Rc::clone(&shared));
            server.init();
            handle.set_should_fail(true);

            let step = server.run_once();
            assert_eq!(step, RunStep::ReceiveFailed);
            assert_eq!(server.dropped_messages(), 1);
        });
    }

    /// V10-P2-4: the main-loop counters are observable via `InfoStats` —
    /// after N dropped receives, a VM_INFO (VMIW_STATS) query reports them.
    #[test]
    fn test_dropped_messages_observable_via_info_stats() {
        with_test_mock_base(|| {
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &test_free_regions(), alloc::rc::Rc::clone(&shared));
            server.init();

            // Register a valid caller at slot 70 (endpoint encodes slot 70).
            let table = VmProcTable::get_global();
            let caller_slot = UserSlot::new(70);
            unsafe { table.reset_slot(caller_slot); }
            let empty = table.get_empty(caller_slot).unwrap();
            let caller_ep = Endpoint::from_generation_slot(1, 70);
            let mut caller = empty.activate(caller_ep);
            caller.init_regions();
            caller.set_acl(crate::acl::AclState::System(crate::acl::AclMask::all()));

            // Three consecutive receive failures → three dropped messages.
            handle.set_should_fail(true);
            for _ in 0..3 {
                assert_eq!(server.run_once(), RunStep::ReceiveFailed);
            }
            assert_eq!(server.dropped_messages(), 3);

            // The counters must be visible through VMIW_STATS.
            let mut msg = Message::default();
            msg.m_source = caller_ep;
            msg.m_type = minix_types::VM_INFO as i32;
            // SAFETY: union overlay write — VM_INFO decodes m2i1 as `what`.
            unsafe {
                msg.m_u.m_m2 = minix_types::ipc::MessageM2 {
                    m2i1: minix_types::VMIW_STATS,
                    ..Default::default()
                };
            }
            let result = MessageDispatcher::dispatch_by_number(
                minix_types::VM_INFO as usize - VM_RQ_BASE as usize,
                &msg,
                &mut server,
            );
            match result.reply {
                VmReply::InfoStats { dropped_messages, pagefault_errors, .. } => {
                    assert_eq!(dropped_messages, 3, "dropped counter must surface via InfoStats");
                    assert_eq!(pagefault_errors, 0);
                }
                other => panic!("VMIW_STATS must decode to InfoStats: {:?}", other),
            }

            unsafe { table.reset_slot(caller_slot); }
        });
    }

    /// V10-P0-2: a permanently broken transport must not busy-spin — after
    /// `MAX_CONSECUTIVE_RECV_FAILURES` consecutive failures `run()` panics
    /// instead of burning 100% CPU (C's `sef_receive_status` blocks).
    #[test]
    fn test_run_busy_loop_protection() {
        with_test_mock_base(|| {
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &test_free_regions(), alloc::rc::Rc::clone(&shared));
            server.init();
            handle.set_should_fail(true);

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                server.run();
            }));
            assert!(result.is_err(), "broken transport must panic, not busy-loop");
            assert_eq!(
                server.dropped_messages(),
                MAX_CONSECUTIVE_RECV_FAILURES as u64,
                "each consecutive failure must be counted before the panic"
            );
        });
    }

    #[test]
    fn test_vm_server_default() {
        with_test_mock_base(|| {
            let server = make_test_vm_server();
            assert!(!server.initialized);
        });
    }

    #[test]
    fn test_vm_server_handle_fork_not_found() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let request = VmForkIn {
                parent_endpoint: Endpoint::NONE,
                child_slot: UserSlot::new(1),
            };

            let reply = MessageDispatcher::dispatch_fork(&mut server.ctx, request);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_vm_server_handle_fork_rejects_exec_tmp_slot() {
        // C: fork.c:47-52 — child slot >= NR_PROCS (incl. the exec-rewrite
        // temp slot VM_EXEC_TMP_SLOT == NR_PROCS) must fail with EINVAL.
        // Regression for the missing upper-bound check (03-P1-1).
        with_test_mock_base(|| {
            reset_boot_slots();

            // Boot image: VM itself (slot 8) + a second boot proc (slot 9)
            // to serve as a valid fork parent.
            let mut pfs_img = BootImage::empty();
            pfs_img.proc_nr = 9;
            pfs_img.endpoint = Endpoint::PFS;
            pfs_img.start_addr = 0x100_0000;
            pfs_img.proc_name[0] = b'p';
            pfs_img.proc_name[1] = b'f';
            pfs_img.proc_name[2] = b's';
            let boot_procs = [vm_boot_image(), pfs_img];
            let regions = test_free_regions();
            let mut server =
                VmServer::new_with_boot_params(boot_params(&regions, &boot_procs, &[]));
            server.init();

            let request = VmForkIn {
                parent_endpoint: Endpoint::PFS,
                child_slot: UserSlot::new(minix_types::NR_PROCS), // exec temp slot
            };

            let reply = MessageDispatcher::dispatch_fork(&mut server.ctx, request);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));

            // Cleanup: `init()` marked the VM slot as a VM instance (bumping
            // the global count); `clear()` (via reset_slot) owns the decrement.
            reset_boot_slots();
        });
    }

    #[test]
    fn test_vm_server_handle_brk_not_found() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let request = VmBrkIn {
                endpoint: Endpoint::NONE,
                new_addr: VirBytes(0x5000_0000),
            };

            let reply = MessageDispatcher::dispatch_brk(&mut server.ctx, request);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_mmap_file_cont_creates_region() {
        // VFS FDLOOKUP reply → mmap_file_cont must create the file-backed
        // region in the target process (C mmap.c:160-190). The final
        // ipc_send resume is transport-gated; the region creation is the
        // observable part of the callback.
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(20);
            unsafe { table.reset_slot(slot); }
            let empty = table.get_empty(slot).unwrap();
            let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
            let mut active = empty.activate(ep);
            active.init_regions();

            let mmap_req = minix_types::VmMmapIn {
                caller: ep,
                forwhom: Endpoint::NONE,
                addr: VirBytes(0),
                length: VirBytes(0x1000),
                prot: 1, // PROT_READ
                flags: 0x0002, // MAP_PRIVATE
                fd: 5,
                offset: 0,
            };
            let reply = crate::vfs_queue::VfsReply {
                req_id: 1,
                result: 0,
                data_phys: None,
                fd: 5,
                dev: 0xABCD,
                ino: 42,
                size_pages: 1,
            };
            let state = crate::vfs_queue::VfsRequestState::FdLookup {
                mmap: mmap_req,
            };

            crate::mmap::mmap_file_cont(&mut server, &reply, &state).expect("callback must succeed");

            let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
            let region = active.regions().find_overlap(
                VirBytes(0x0000_0001_0000_0000),
                VirBytes(0x0000_0200_0000_0000),
            ).expect("file region must exist after mmap_file_cont");
            assert!(!region.flags.contains(crate::region::VrFlags::ANON));
            assert!(matches!(
                region.param,
                crate::region::VrParam::File { inited: true, offset: 0, .. }
            ));
            // read-only mapping (PROT_READ) → not writable
            assert!(!region.flags.contains(crate::region::VrFlags::WRITABLE));

            unsafe { table.reset_slot(slot); }
        });
    }

    #[test]
    fn test_vm_server_handle_exit_not_found() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let request = VmExitIn {
                endpoint: Endpoint::NONE,
            };

            let reply = MessageDispatcher::dispatch_exit(&mut server.ctx, request);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_vm_server_page_cache_access() {
        with_test_mock_base(|| {
            let server = make_test_vm_server();
            assert_eq!(server.page_cache().total_cached(), 0);
        });
    }

    #[test]
    fn test_pagefault_errors_counted() {
        // V9-P1-1: a pagefault whose handling fails (here: endpoint not in
        // the process table) must be counted, not silently dropped.
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();
            assert_eq!(server.pagefault_errors(), 0);

            let mut msg = Message::default();
            msg.m_source = Endpoint::MEM; // kernel-ish source, invalid proc slot
            msg.m_type = minix_types::VM_PAGEFAULT as i32;
            let mut pf = minix_types::ipc::MessVmPagefault::default();
            pf.vpf_addr = 0x1000;
            pf.vpf_flags = 0; // not a write fault (C: PFERR_WRITE bit 1)
            unsafe {
                msg.m_u.m_vm_pagefault = pf;
            }

            // Pagefaults originate in the kernel on behalf of the faulting
            // process: IPC_FLG_MSG_FROM_KERNEL (ipcconst.h:22-24, bit 16).
            let from_kernel = IpcStatus { flags: 1 << 16 };
            let action = server.dispatch_on_msg(&msg, &from_kernel, UserSlot::new(0));
            assert!(matches!(action, DispatchAction::NoReply));
            assert_eq!(server.pagefault_errors(), 1, "failed pagefault must be counted");
        });
    }

    #[test]
    fn test_pagefault_accounting_minor_and_violation() {
        // V11/T31: a successfully served fault bumps minor (fresh anon
        // page); a write to a read-only region is an access violation and
        // counts nothing (Linux getrusage semantics — minix3 VM has no
        // counters, the fields are a minix-rs extension).
        use crate::region::{VirRegion, VrFlags};
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(72);
            unsafe { table.reset_slot(slot); }
            let empty = table.get_empty(slot).unwrap();
            let ep = Endpoint::from_generation_slot(1, 72);
            let mut active = empty.activate(ep);
            active.init_page_table().unwrap();
            active.init_regions();
            drop(active);
            {
                let mut proc = table.get_active(slot).unwrap();
                let mut region = VirRegion::new(
                    VirBytes(0x3000_0000),
                    VirBytes(0x4000),
                    VrFlags::ANON, // read-only anonymous
                );
                // `VirRegion::new` leaves the memtype unset (callers choose
                // it explicitly — rs.rs/mmap.rs do the same).
                region.def_memtype = Some(&crate::memtype::MEM_TYPE_ANON);
                proc.regions_mut().insert(region).unwrap();
            }

            let kernel_status = IpcStatus { flags: 1 << 16 };
            // VmPagefaultIn::decode_message reads the faulting endpoint from
            // m_source (doc 16 [ARCH]: the kernel sends on behalf of the
            // process, packing the endpoint as the source).
            let fault_msg = |addr: u64, write: u32| {
                let mut msg = Message::default();
                msg.m_source = ep;
                msg.m_type = minix_types::VM_PAGEFAULT as i32;
                let mut pf = minix_types::ipc::MessVmPagefault::default();
                pf.vpf_addr = addr;
                pf.vpf_flags = write;
                unsafe {
                    msg.m_u.m_vm_pagefault = pf;
                }
                msg
            };

            // Read fault on the anon region → fresh page → minor.
            let msg = fault_msg(0x3000_0000, 0);
            let _ = server.dispatch_on_msg(&msg, &kernel_status, UserSlot::new(0));
            let proc = table.get_active(slot).unwrap();
            assert_eq!(proc.minor_fault(), 1, "served fault counts minor");
            assert_eq!(proc.major_fault(), 0);

            // V11/T35 (G-V12-6 closed): a write to the read-only region is
            // not servable — SIGSEGV is delivered to the faulting process
            // and its kernel pagefault suspension is cleared; no fault is
            // counted. Swap in a Mock gateway (behind a shared delegate so
            // the test can inspect the delivery record afterwards).
            let mock = alloc::rc::Rc::new(core::cell::RefCell::new(
                crate::kernel_gateway::MockGateway::new(),
            ));
            server.ctx.gateway = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(SharedMockGateway(alloc::rc::Rc::clone(&mock)))
                    as alloc::boxed::Box<dyn crate::kernel_gateway::KernelGateway>,
            ));
            let msg = fault_msg(0x3000_1000, 2); // bit 1 = write (x86 PFE_W)
            let _ = server.dispatch_on_msg(&msg, &kernel_status, UserSlot::new(0));
            let proc = table.get_active(slot).unwrap();
            assert_eq!(proc.minor_fault(), 1, "violations are not served faults");
            assert_eq!(proc.major_fault(), 0);
            {
                let gw = mock.borrow();
                assert_eq!(
                    gw.kills.borrow().as_slice(),
                    &[(ep, minix_types::SIGNAL_SEGMENT_VIOLATION)],
                    "SIGSEGV must be delivered to the faulting process"
                );
                assert_eq!(
                    gw.clear_pagefaults.borrow().as_slice(),
                    &[ep],
                    "kernel pagefault suspension must be cleared"
                );
            }
        });
    }

    #[test]
    fn test_vfs_transid_routes_to_procctl_clear() {
        // V11/T32: a genuine VFS transid message carries m_type = 0xB00|seq
        // (the transid IS the type; TRNS_DEL_ID yields 0) and the procctl
        // parameters in the m9 body. P1 must route it to VM_PROCCTL: CLEAR
        // from VFS executes and empties the target's address space.
        use crate::region::{VirRegion, VrFlags};
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(73);
            unsafe { table.reset_slot(slot); }
            let empty = table.get_empty(slot).unwrap();
            let ep = Endpoint::from_generation_slot(1, 73);
            let mut active = empty.activate(ep);
            active.init_page_table().unwrap();
            active.init_regions();
            drop(active);
            {
                let mut proc = table.get_active(slot).unwrap();
                let mut region = VirRegion::new(
                    VirBytes(0x3000_0000),
                    VirBytes(0x4000),
                    VrFlags::ANON | VrFlags::WRITABLE,
                );
                region.def_memtype = Some(&crate::memtype::MEM_TYPE_ANON);
                proc.regions_mut().insert(region).unwrap();
            }

            let mut msg = Message::default();
            msg.m_source = VFS_PROC_NR; // P1 gate: VFS only (crate-local const)
            msg.m_type = 0xB01; // VFS_TRANSACTION_BASE + seq — the transid IS the type
            {
                // SAFETY: m9 overlay — VMPCTL_PARAM=1 (CLEAR), VMPCTL_WHO=ep
                // (kernel syscall.rs / lib/minix-types m9 offsets).
                let p = unsafe { &mut msg.m_u.m_lc_vm_procctl };
                p.param = 1; // VMPPARAM_CLEAR
                p.who = ep.0;
            }
            let kernel_status = IpcStatus { flags: 1 << 16 };
            let action = server.dispatch_on_msg(&msg, &kernel_status, UserSlot::new(0));
            match action {
                DispatchAction::Reply(VmReply::Ok) => {}
                DispatchAction::Reply(other) => {
                    panic!("transid CLEAR must succeed, got {other:?}")
                }
                DispatchAction::Suspend => panic!("transid CLEAR must not suspend"),
                DispatchAction::NoReply => panic!("transid CLEAR must reply"),
            }
            let proc = table.get_active(slot).unwrap();
            assert_eq!(proc.regions().len(), 0, "CLEAR empties the address space");
        });
    }

    #[test]
    fn test_vm_server_vfs_queue_access() {
        with_test_mock_base(|| {
            let server = make_test_vm_server();
            assert!(server.vfs_queue().is_empty());
        });
    }

    #[test]
    fn test_vm_server_vfs_reply_no_pending() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            // VFS reply dispatch is now handled by dispatch_vfs_reply
            // which is tested in dispatcher tests. This test verifies
            // that an empty queue reports no pending requests.
            assert!(!server.has_pending_vfs_requests());
        });
    }

    // ── transid helper tests ──

    #[test]
    fn test_is_vfs_fs_transid_valid() {
        // VFS_TRANSACTION_BASE = 0xB00. Any m_type with (m_type & ~0xFF) == 0xB00
        // is a VFS transid message.
        assert!(is_vfs_fs_transid(0xB00));   // base itself
        assert!(is_vfs_fs_transid(0xB01));   // base + 1
        assert!(is_vfs_fs_transid(0xBFF));   // base + 0xFF
        assert!(is_vfs_fs_transid(0xB45));   // arbitrary within range
    }

    #[test]
    fn test_is_vfs_fs_transid_invalid() {
        assert!(!is_vfs_fs_transid(0x000));   // zero
        assert!(!is_vfs_fs_transid(0xA00));   // wrong base
        assert!(!is_vfs_fs_transid(0xC00));   // VM_RQ_BASE
        assert!(!is_vfs_fs_transid(0x600));   // VFS call number
        assert!(!is_vfs_fs_transid(0xB100));  // base shifted left
    }

    #[test]
    fn test_transid_extract() {
        // C: TRNS_GET_ID(t) = t & 0xFFFF
        assert_eq!(transid_extract(0x000C002B), 0x002B);
        assert_eq!(transid_extract(0x000C0045), 0x0045);
        assert_eq!(transid_extract(0xFFFF0001), 0x0001);
        assert_eq!(transid_extract(0x00000000), 0);
    }

    #[test]
    fn test_transid_strip() {
        // C: TRNS_DEL_ID(t) = (short)(t >> 16)
        // VM_PROCCTL = 0xC2D = VM_RQ_BASE(0xC00) + 45
        // Encoded: (0xC2D << 16) | transid
        let encoded = (0xC2Du32 << 16) | 0x002B;
        assert_eq!(transid_strip(encoded), 0xC2D);

        // Sign extension test: if high bits represent a negative short,
        // the result should sign-extend (though VM request numbers are positive).
        let neg_encoded = (0xFFFFu32 << 16) | 0x0001;
        assert_eq!(transid_strip(neg_encoded), 0xFFFFFFFF); // -1 sign-extended
    }

    #[test]
    fn test_handle_vfs_transid_wrong_clean_type() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            // V11/T32 (C parity): TRNS_DEL_ID on a genuine transid message
            // yields 0 — the call number does not travel in m_type — so
            // clean_type carries no routing information and is not gated;
            // the message routes to do_procctl unconditionally and the
            // zeroed body fails validation (VMPCTL_WHO = 0 → EINVAL).
            let msg = Message::default();
            let reply = server.handle_vfs_transid(0x600, 1, &msg);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_handle_vfs_transid_zero_transid() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            // A zeroed body fails validation (VMPCTL_WHO = 0 → EINVAL;
            // V11/T32: the former dedicated transid==0 gate is gone —
            // a genuine transid message always carries the 0xB marker and
            // can never extract to 0).
            let msg = Message::default();
            let reply = server.handle_vfs_transid(VM_PROCCTL, 0, &msg);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_handle_vfs_transid_invalid_endpoint() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            // Build a message with VM_PROCCTL clean_type, valid transid,
            // but VmProcctlIn.who = 0 (invalid endpoint).
            // Wire layout is the C m9 overlay (param@16/who@20/m1@24/
            // len@28/flags@32, com.h:753-757).
            let mut msg = Message::default();
            msg.m_u.m_lc_vm_procctl.param = 1; // VMPPARAM_CLEAR
            msg.m_u.m_lc_vm_procctl.who = 0;   // VMPCTL_WHO = 0 (invalid)
            msg.m_u.m_lc_vm_procctl.m1 = 0;    // VMPCTL_M1
            msg.m_u.m_lc_vm_procctl.len = 0;   // VMPCTL_LEN
            msg.m_u.m_lc_vm_procctl.flags = 0; // VMPCTL_FLAGS

            let reply = server.handle_vfs_transid(VM_PROCCTL, 42, &msg);
            // C do_procctl collapses vm_isokendpt failures to EINVAL
            // (exit.c:122-125) → InvalidProcess.
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    // ── boot / init chain tests (doc 01-vm-init-main) ──

    fn vm_boot_image() -> BootImage {
        let mut img = BootImage::empty();
        img.proc_nr = VM_PROC_NR;
        img.endpoint = Endpoint::VM;
        img
    }

    /// Resets the process-table slots used by the boot-chain tests.
    ///
    /// The table is a process-wide static shared by the whole test suite;
    /// resetting here makes test ordering irrelevant. Slots 8 (VM) and 9
    /// are not used by other test modules (they use 11-17 and 100+).
    fn reset_boot_slots() {
        let table = VmProcTable::get_global();
        // SAFETY: Test-only cleanup; no other references to these slots
        // are alive at this point (slots 8/9 are used only by these tests).
        unsafe {
            table.reset_slot(UserSlot(VM_PROC_NR as usize));
            table.reset_slot(UserSlot(9));
        }
    }

    fn boot_params<'a>(
        free_regions: &'a [BootMemRegion],
        boot_procs: &'a [BootImage],
        modules: &'a [BootModule],
    ) -> BootParams<'a> {
        BootParams {
            // Fake-but-valid root: these tests never exercise adoption
            // (init_vm_self_pt is skipped in test builds).
            root_paddr: PhysBytes(0x900_000),
            total_pages: TEST_TOTAL_PAGES,
            free_regions,
            boot_procs,
            modules,
            kernel_allocated: KernelAllocated::ZERO,
            vm_allocated_bytes: 0,
            is_first_time: true,
            kernel_layout: None,
        }
    }

    #[test]
    fn test_vm_server_init_with_boot_procs() {
        with_test_mock_base(|| {
            reset_boot_slots();

            // Boot image: VM itself (slot 8) + a second boot proc (slot 9).
            let mut pfs_img = BootImage::empty();
            pfs_img.proc_nr = 9;
            pfs_img.endpoint = Endpoint::PFS;
            pfs_img.start_addr = 0x100_0000;
            pfs_img.proc_name[0] = b'p';
            pfs_img.proc_name[1] = b'f';
            pfs_img.proc_name[2] = b's';
            let boot_procs = [vm_boot_image(), pfs_img];
            let regions = test_free_regions();

            let mut server =
                VmServer::new_with_boot_params(boot_params(&regions, &boot_procs, &[]));
            server.init();
            assert!(server.is_initialized());

            // C: init_proc(VM_PROC_NR) + VMF_VM_INSTANCE (main.c:474,579).
            let table = VmProcTable::get_global();
            let vm = table
                .get_active(UserSlot(VM_PROC_NR as usize))
                .expect("VM slot should be active after init");
            assert!(vm.is_vm_instance());

            // C: boot procs loop (main.c:497-520) — boot proc slot populated.
            let pfs = table
                .get_active(UserSlot(9))
                .expect("boot proc slot should be active after init");
            assert_eq!(pfs.endpoint(), Endpoint::PFS);
        });
    }

    #[test]
    fn test_vm_server_init_vm_instance_count() {
        with_test_mock_base(|| {
            reset_boot_slots();
            let before = crate::global::vm_instance_count();
            let boot_procs = [vm_boot_image()];
            let regions = test_free_regions();
            let mut server =
                VmServer::new_with_boot_params(boot_params(&regions, &boot_procs, &[]));
            server.init();
            // C: num_vm_instances = 1 (main.c:578).
            assert_eq!(crate::global::vm_instance_count(), before + 1);
        });
    }

    #[test]
    fn test_vm_server_init_accounts_boot_memory() {
        with_test_mock_base(|| {
            // C: main.c:485-491 — modules except the last entry are charged.
            let modules = [
                BootModule { start_addr: 0x2000, len: CLICK_SIZE as u64 }, // 1 page
                BootModule { start_addr: 0x3000, len: 1 },                  // excluded (last)
            ];
            let regions = test_free_regions();
            let mut server =
                VmServer::new_with_boot_params(boot_params(&regions, &[], &modules));
            server.init();
            // global::init() reset the total to the allocator total; the
            // single charged module adds exactly 1 page.
            assert_eq!(crate::global::total_pages(), TEST_TOTAL_PAGES + 1);
        });
    }

    /// V11/T23 (V11-P2-5): the invalid-caller drop branch (run_once
    /// :713-723) — an unregistered endpoint's message is dropped with a
    /// counter bump, no reply, no panic. Mirrors [ARCH: A-14].
    #[test]
    fn test_run_once_invalid_caller_dropped() {
        with_test_mock_base(|| {
            reset_boot_slots();

            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &test_free_regions(), alloc::rc::Rc::clone(&shared));
            server.init();

            // VM_INFO from an endpoint that is NOT registered in the
            // proc table → vm_isokendpt fails → drop branch.
            let mut msg = Message::default();
            msg.m_source = Endpoint(70);
            msg.m_type = minix_types::VM_INFO as i32;
            handle.queue_receive(msg, IpcStatus::default());

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);
            assert_eq!(handle.sent().len(), 0, "invalid caller gets no reply");
            assert_eq!(server.dropped_messages(), 1, "drop must be counted");

            // Cleanup: init() registered the VM boot instance.
            reset_boot_slots();
        });
    }

    /// V11/T23 (V11-P2-5): pins the reply-encode failure invariant that
    /// `run_once`'s Reply arm relies on (vm_server.rs:745-752) — a Reply
    /// V11/T9 step 3: an RS_INIT from RS carries the rproctab grant; the
    /// handshake fails closed until E-RSWIRE (the rproctab byte decode is
    /// pending) — no panic (the previous `.expect`), the message is dropped
    /// and counted, and nothing is sent to RS. Flip when E-RSWIRE lands.
    #[test]
    fn test_run_once_rs_init_fails_closed_until_erswire() {
        with_test_mock_base(|| {
            reset_boot_slots();

            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &test_free_regions(), alloc::rc::Rc::clone(&shared));
            server.init();

            // RS_INIT from RS_PROC_NR, carrying grant 7 (C main.c:149 shape).
            let mut msg = Message::default();
            msg.m_source = Endpoint::RS;
            msg.m_type = RS_INIT as i32;
            // SAFETY: test constructs the message with m_rs_init active.
            unsafe {
                msg.m_u.m_rs_init = minix_types::ipc::MessRsInit {
                    rproctab_gid: 7,
                    ..Default::default()
                };
            }
            handle.queue_receive(msg, IpcStatus::default());

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);
            // Fail-closed: no reply to RS (NoReply), drop counted, audit fired.
            assert_eq!(handle.sent().len(), 0, "no reply to RS on failed handshake");
            assert_eq!(server.dropped_messages(), 1, "failed handshake must be counted");

            // Cleanup: init() registered the VM boot instance (global
            // counters); the sibling tests assert on those counters.
            reset_boot_slots();
        });
    }

    #[test]
    fn test_rproctab_empty_32_slots_all_not_in_use() {
        // D8: RprocTab is a 32-slot handshake stub — deliberately different
        // from C's rprocpub[NR_SYS_PROCS]=64 and minix-rs NR_PROCS=256
        // (doc §3.9); the stub shape only feeds the future handshake decode.
        let tab = RprocTab::empty();
        assert_eq!(tab.iter().count(), 32);
        assert!(tab.iter().all(|e| !e.in_use));
        assert_eq!(tab.iter().filter(|e| e.endpoint != Endpoint::NONE).count(), 0);
    }

    #[test]
    fn test_rproctab_empty_entry_defaults() {
        let e = RprocEntry::EMPTY;
        assert!(!e.in_use);
        assert_eq!(e.endpoint, Endpoint::NONE);
        assert_eq!(e.call_mask, 0);
        assert!(!e.is_user);
    }
    // ── V11/T29: SIGKMEM signal seam + do_memory drain loop ──────────

    /// Swaps the server's gateway for a trap gateway over a scripted
    /// canned transport (kernel-observable wire assertions) and returns
    /// the shared canned handle.
    fn install_canned_gateway(
        server: &mut VmServer,
        canned: alloc::rc::Rc<minix_sys::syscall::CannedKernelCallTransport>,
    ) {
        struct SharedCanned(alloc::rc::Rc<minix_sys::syscall::CannedKernelCallTransport>);
        impl minix_sys::syscall::KernelCallTransport for SharedCanned {
            fn kernel_call(&self, message: &mut Message) -> i32 {
                self.0.kernel_call(message)
            }
        }
        server.ctx.gateway = alloc::rc::Rc::new(core::cell::RefCell::new(
            alloc::boxed::Box::new(crate::kernel_gateway::TrapKernelGateway {
                transport: SharedCanned(canned),
            }),
        ));
    }

    #[test]
    fn test_do_memory_services_kernel_check_request() {
        with_test_mock_base(|| {
            reset_boot_slots();
            let boot_procs = [vm_boot_image()];
            let regions = test_free_regions();
            let t = crate::ipc::transport::TestIpcTransport::new();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            // Target process with a writable anonymous region.
            let table = VmProcTable::get_global();
            let slot = UserSlot::new(71);
            unsafe { table.reset_slot(slot); }
            let empty = table.get_empty(slot).unwrap();
            let ep = Endpoint::from_generation_slot(1, 71);
            let mut active = empty.activate(ep);
            active.init_page_table().unwrap();
            active.init_regions();
            drop(active);
            {
                let mut proc = table.get_active(slot).unwrap();
                proc.regions_mut().insert(crate::region::VirRegion::new(
                    VirBytes(0x3000_0000),
                    VirBytes(0x4000),
                    crate::region::VrFlags::WRITABLE | crate::region::VrFlags::ANON,
                )).unwrap();
            }

            // Scripted kernel: CHECK over the region, then queue empty.
            let mut canned = minix_sys::syscall::CannedKernelCallTransport::new();
            let mut check = Message::default();
            check.m_type = 1; // VMPTYPE_CHECK
            {
                // SAFETY: SVMCTL_MRG_* reply fields (kernel syscall.rs).
                let m1 = unsafe { &mut check.m_u.m_m1 };
                m1.m1i1 = ep.0;
                m1.m1p1 = 0x3000_0000;
                m1.m1p2 = 0x2000;
                m1.m1i3 = 1;
                m1.m1p3 = 0;
            }
            // Script per kernel call: GET (payload) → REPLY (OK) → GET
            // (ENOENT terminates the drain). The canned transport's
            // exhausted-script default (0) would read as a live request,
            // so every call is scripted explicitly.
            canned.reply_message(check);
            canned.reply(0);
            canned.reply(minix_types::ENOENT);
            let canned = alloc::rc::Rc::new(canned);
            install_canned_gateway(&mut server, alloc::rc::Rc::clone(&canned));

            server.do_memory();

            // Kernel-observable verdicts: GET wire, then REPLY wire with an
            // OK verdict, then the drain stopped at ENOENT.
            let sent = canned.sent.borrow();
            assert_eq!(sent.len(), 3, "GET, REPLY, second GET");
            assert_eq!(sent[0].m_type, crate::kernel_gateway::SYS_VMCTL_CALL);
            assert_eq!(sent[1].m_type, crate::kernel_gateway::SYS_VMCTL_CALL);
            {
                // SAFETY: reply wire inspection (SYS_VMCTL M1 fields).
                let m1 = unsafe { &sent[1].m_u.m_m1 };
                assert_eq!(m1.m1i1, ep.0, "reply targets the fetched request");
                assert_eq!(m1.m1i2, 15, "VMCTL_MEMREQ_REPLY");
                assert_eq!(m1.m1i3, 0, "valid writable range → OK verdict");
            }
        });
    }

    #[test]
    fn test_do_memory_reports_fault_for_unmapped_range() {
        with_test_mock_base(|| {
            reset_boot_slots();
            let boot_procs = [vm_boot_image()];
            let regions = test_free_regions();
            let t = crate::ipc::transport::TestIpcTransport::new();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(71);
            unsafe { table.reset_slot(slot); }
            let empty = table.get_empty(slot).unwrap();
            let ep = Endpoint::from_generation_slot(1, 71);
            let mut active = empty.activate(ep);
            active.init_page_table().unwrap();
            active.init_regions();
            drop(active);

            let mut canned = minix_sys::syscall::CannedKernelCallTransport::new();
            let mut check = Message::default();
            check.m_type = 1;
            {
                // SAFETY: SVMCTL_MRG_* reply fields.
                let m1 = unsafe { &mut check.m_u.m_m1 };
                m1.m1i1 = ep.0;
                m1.m1p1 = 0x5000_0000; // no region covers this
                m1.m1p2 = 0x2000;
                m1.m1i3 = 1;
                m1.m1p3 = 0;
            }
            // GET (payload) → REPLY (OK) → GET (ENOENT); see test above
            // for why every call is scripted explicitly.
            canned.reply_message(check);
            canned.reply(0);
            canned.reply(minix_types::ENOENT);
            let canned = alloc::rc::Rc::new(canned);
            install_canned_gateway(&mut server, alloc::rc::Rc::clone(&canned));

            server.do_memory();

            let sent = canned.sent.borrow();
            assert_eq!(sent.len(), 3);
            {
                // SAFETY: reply wire inspection.
                let m1 = unsafe { &sent[1].m_u.m_m1 };
                assert_eq!(m1.m1i3, 1, "unmapped range → fault verdict");
            }
        });
    }

    #[test]
    fn test_handle_signal_routes_sigkmem_only() {
        with_test_mock_base(|| {
            reset_boot_slots();
            let boot_procs = [vm_boot_image()];
            let regions = test_free_regions();
            let t = crate::ipc::transport::TestIpcTransport::new();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(t)
                    as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>,
            ));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            let mut canned = minix_sys::syscall::CannedKernelCallTransport::new();
            // SIGKMEM routes into the drain loop: GET (raw CHECK reply; no
            // payload, so the request's own M1 fields read back) → REPLY →
            // GET → ENOENT stops the loop. Every call scripted explicitly.
            canned.reply(1);            // VMPTYPE_CHECK
            canned.reply(0);            // REPLY acknowledged OK
            canned.reply(minix_types::ENOENT);
            let canned = alloc::rc::Rc::new(canned);
            install_canned_gateway(&mut server, alloc::rc::Rc::clone(&canned));

            // Unknown signal → no kernel interaction (C ignores the rest).
            server.handle_signal(0);
            assert_eq!(canned.calls.get(), 0);
            server.handle_signal(VmServer::SIGKMEM);
            let sent = canned.sent.borrow();
            assert_eq!(sent.len(), 3, "GET, REPLY, terminating GET");
            {
                // SAFETY: request wire inspection.
                let m1 = unsafe { &sent[1].m_u.m_m1 };
                assert_eq!(m1.m1i2, 15, "second kernel call is the REPLY");
            }
        });
    }
    /// Shared-handle delegate around [`crate::kernel_gateway::MockGateway`]:
    /// the boxed trait object in `VmContext` and the test's inspection
    /// handle point at the same concrete mock (V11/T35).
    struct SharedMockGateway(alloc::rc::Rc<core::cell::RefCell<crate::kernel_gateway::MockGateway>>);

    impl crate::kernel_gateway::KernelGateway for SharedMockGateway {
        fn sys_fork(&mut self, parent: Endpoint, child_slot: UserSlot)
            -> Result<(Endpoint, Option<u64>), crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_fork(parent, child_slot)
        }
        fn sys_exec(&mut self, endpt: Endpoint, ip: u64, stack: u64, name_ptr: u64, ps_str: u64)
            -> Result<(), crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_exec(endpt, ip, stack, name_ptr, ps_str)
        }
        fn sys_update(&mut self, src: Endpoint, dst: Endpoint, flags: u32)
            -> Result<(), crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_update(src, dst, flags)
        }
        fn sys_vmctl_memreq_get(&mut self)
            -> Result<Option<crate::kernel_gateway::KernelMemReq>, crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_vmctl_memreq_get()
        }
        fn sys_vmctl_memreq_reply(&mut self, target: Endpoint, ok: bool)
            -> Result<(), crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_vmctl_memreq_reply(target, ok)
        }
        fn sys_kill(&mut self, endpoint: Endpoint, signal: i32)
            -> Result<(), crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_kill(endpoint, signal)
        }
        fn sys_vmctl_clear_pagefault(&mut self, endpoint: Endpoint)
            -> Result<(), crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_vmctl_clear_pagefault(endpoint)
        }
        fn diag_write(&mut self, text: &str) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().diag_write(text)
        }
    }
}