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
// VmContext.user_sp + install_boot_stack use VirBytes unconditionally
// (E-BOOTFRAME); before that only test code needed it.
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
    /// Initial user stack top (E-BOOTFRAME). C: `kernel_boot_info.user_sp`
    /// (glo.h; `kinfo.user_sp = USR_STACKTOP`, pre_init.c:156) —
    /// exec_bootproc builds boot-proc initial stacks downward from it
    /// (main.c:346-411).
    pub(crate) user_sp: VirBytes,
}

impl VmContext {
    fn new(
        page_alloc: VmPageAllocator,
        kernel_allocated: KernelAllocated,
        vm_allocated_bytes: u64,
        user_sp: VirBytes,
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
            user_sp,
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
        let page_alloc = VmPageAllocator::new(phys_alloc);
        // fix21 (P0-code-bug): the global registration used to happen HERE,
        // against the local `page_alloc` — which is then MOVED into
        // `VmContext` and out through the return value, leaving
        // PAGE_ALLOC_PTR dangling into this dead stack frame. Everything
        // reaching `page_alloc_mut()` afterwards (the `vm_pt_alloc` hook,
        // `refill_arena`, `alloc_oversize`) read a stale corpse: the first
        // boot procs "worked" off leftover bits (double-allocating pages
        // the post-relocate allocator still counted free), and `rs` died
        // with `AllocationFailed free=0 … self_pages=0` once the frame was
        // recycled (forensics 2026-09-21, serial_fix21.log). The
        // registration now targets the FINAL address, at the top of
        // `init()`, where `self` is the caller's stable local
        // (`main.rs:57`) and never moves again. Heap requests in the
        // construction window ride the early pool (global.rs EARLY_POOL).

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
        // E4 余件:配对的归还钩子。注册后 `destroy()` 的
        // free_child_tables + 根页回收才激活(未注册时退化为只清零根,
        // 中间页表页随进程退出泄漏——C pt_free pagetable.c:1427-1437
        // 的 Rust 对应物至此闭环)。
        if !minix_arch::pt_alloc::is_free_registered() {
            minix_arch::pt_alloc::register_free(crate::alloc_page::vm_pt_free);
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
            ctx: VmContext::new(
            page_alloc,
            params.kernel_allocated,
            params.vm_allocated_bytes,
            params.user_sp,
        ),
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
        // The metadata donor is carved IN PLACE; every other survivor region
        // must reach the allocator too (C: `mem_init` seeds the hole list from
        // ALL chunks, alloc.c:306-331). Passing only the donor — what this
        // line used to do — left the PMM with the donor's remainder only
        // (~133 pages on first light, so the `PageFrames` oversize grow hit
        // PhysicalAllocFailed; fix19 forensics 2026-09-21, the vm_handoff
        // serial landmark showing `free n=10` vs a 24-page pool).
        let mut adjusted_regions =
            alloc::vec::Vec::with_capacity(free_regions.len());
        for r in free_regions {
            if r.base == meta_region.base && r.size == meta_region.size {
                adjusted_regions.push(BootMemRegion {
                    base: adjusted_base,
                    size: adjusted_size,
                });
            } else {
                adjusted_regions.push(*r);
            }
        }

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
        // fix21: register the global PMM pointer against the FINAL address
        // of the allocator. `self` here is `main.rs:57`'s `let mut server`
        // local — it never moves again after this point, so PAGE_ALLOC_PTR
        // (read by the `vm_pt_alloc` hook, `refill_arena`, `alloc_oversize`)
        // stays valid for the server's whole lifetime. Registering earlier
        // (in `new_inner`, against a pre-move local) dangled the pointer
        // into a dead stack frame — see the fix21 comment there.
        // `relocate()` below replaces the allocator's `phys_alloc` field
        // IN PLACE, so this registration survives it.
        crate::global::register_page_alloc(&mut self.ctx.page_alloc);

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
        // NK4-A fix22 路标（bootmark，task1-close 裁决去留）
        crate::bootmark::mark("nk4a: relocate ok");

        // Phase 1: Memory detection — initialize global state with total page count.
        self.init_global_state();

        // Phase 2a: init_proc(VM_PROC_NR) — main.c:474.
        self.init_vm_slot();
        crate::bootmark::mark("nk4a: vm slot ok");

        // Phase 2b: mem_add_total_pages() call points — main.c:485-495.
        self.account_boot_memory();

        // PageFrames after total_pages is known, BEFORE the boot-proc loop:
        // C orders `mem_init()` (main.c:471 — the all-physical bookkeeping
        // this mirrors) ahead of the `exec_bootproc` pass (main.c:498-520),
        // and exec_bootproc consumes page_frames for segment pages. The
        // former placement after init_boot_procs() was an unreachable-order
        // latent bug the fix16 forensics exposed ("page_frames not
        // initialized" panic from exec_bootproc, 2026-09-21).
        let total_phys = PhysBytes(self.ctx.page_alloc.total_pages() as u64 * crate::region::PAGE_SIZE);
        self.ctx.page_frames = Some(PageFrames::new(total_phys));

        // V11/T30: the reclaim-retry funnel needs kernel-visible access to
        // the cache/frames pair (C alloc_mem reads the global cache). Same
        // registration lifetime as the audit gateway and page allocator.
        if let Some(frames) = self.ctx.page_frames.as_mut() {
            crate::global::register_reclaim(&mut self.ctx.page_cache, frames);
        }

        // Phase 2c: boot process slots — main.c:497-520 (exec_bootproc
        // landed in V11/T14; the initial stack frame ABI waits on edge
        // E-BOOTFRAME).
        self.init_boot_procs();

        // Phase 2d: VM instance mark — main.c:577-579.
        self.mark_vm_instance();

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
                0xFFFF_8000_0000_0000, // dm_vbase (legacy sentinel — host-test shape only; a real handoff is v5 and carries the arch's true KERNEL_DIRECT_MAP_BASE, fix26)
                4,                     // dm_pages (legacy sentinel — see fix26)
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
            // NK4-A fix22 路标：boot proc 出生逐个过点（task1-close 裁决去留）
            crate::bootmark::mark(
                alloc::format!("nk4a: exec {} ok", ip.name()).as_str(),
            );

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

        // C: pt_bind(&vmp->vm_pt, vmp) — main.c:355, whose substance is
        // `sys_vmctl_set_addrspace(endpoint, pt_dir_phys, pdes)`
        // (pagetable.c:1421). The kernel stores the root and clears
        // RTS_VMINHIBIT (C setcr3, arch_do_vmctl.c:19-33) — this is the
        // ONLY boot-path VMINHIBIT clear leg (the kernel sets
        // VMINHIBIT|BOOTINHIBIT on every non-VM boot proc,
        // kernel/src/lib.rs:1666); without it the exec'd processes stay
        // parked even after BOOTINHIBIT lifts (NK4-A fix25 forensics).
        // `pdes` (the kernel-visible PDE alias) has no meaning under the
        // Direct Map — 0 travels as `virt_root = None` (documented
        // deviation, exit.rs:242-247).
        let ptroot_phys =
            <crate::pagetable::PageTable as crate::pagetable::Paging>::root_paddr(
                proc.page_table_mut(),
            )
            .0;
        // NK4-C 第 33 轮取证探针（task1-close 裁决删除）：SetAddrSpace
        // 发送值。c33a 实锤矛盾：VM ptalloc 首批 64 个 PT 页全无
        // 0x35fd0（内核持有的 RS root），而内核 SetAddrSpace 忠实写
        // p_seg——故 0x35fd000 必是 VM 发的。本探针直接打印发送值，
        // 裁决「VM 发错 root」vs「内核存错 root」。
        #[cfg(not(test))]
        crate::bootmark::mark(&alloc::format!(
            "nk4a: sas-send ep={} root={:#x}\n",
            ip.endpoint.0,
            ptroot_phys
        ));
        self.ctx
            .gateway
            .borrow_mut()
            .sys_vmctl_set_addrspace(ip.endpoint, ptroot_phys, 0)
            .map_err(|_| "VMCTL_SETADDRSPACE failed")?;

        for seg in segments.iter() {
            let seg_len = seg.memsz;
            if seg_len == 0 {
                continue;
            }
            // NK4-A C-3 迭代5（2026-09-22）：段基页对齐向下取整。RS 的入口
            // 段 vaddr=0x2246c0 非页对齐——旧实现以未对齐段基为 region 零点
            // 铺页，硬件 PTE 却按 4KiB 页界安装，页内内容整体错位
            // (seg.vaddr & 0xfff)——RS 首指令在错位字节上 #GP(0)。
            // C libexec 对位：p_offset ≡ p_vaddr (mod page) 的 ELF 约定下，
            // 对齐页的文件源 = seg.offset - (seg.vaddr & 0xfff)。
            let va_base = seg.vaddr & !(PS as u64 - 1);
            let va_end = seg.vaddr + seg_len;
            let pages = (va_end - va_base).div_ceil(PS as u64) as usize;
            let vaddr = minix_types::VirBytes(va_base);

            // Region per segment (C: libexec_alloc_vm_prealloc → map_page_region).
            // Writable flag 来自 ELF 段旗标（PF_W=2）：C 的 map_page_region
            // 对 PT_LOAD RW 段建可写 region。此前全部段只给 ANON——数据段
            // region 不可写 → is_page_writable 恒 false → sync_slot_pte 把
            // .data 页映射成只读，用户态首次写即 PF(err=7) → VM 判违例 →
            // SIGSEGV（真机 NK4-A C-3 c10a 轮：RS ensure_global_allocator
            // 的分配器旗标 xchg 于 0x229708 二连故障，2026-09-22）。
            let mut seg_flags = crate::region::VrFlags::ANON;
            if seg.flags & minix_elf::PF_W != 0 {
                seg_flags |= crate::region::VrFlags::WRITABLE;
            }
            let region = crate::region::VirRegion::with_memtype(
                vaddr,
                minix_types::VirBytes((pages * PS) as u64),
                seg_flags,
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

                // Copy file bytes into the fresh physical page. Page i 覆盖
                // [va_base+i*PS, +PS)，与 [seg.vaddr, seg.vaddr+filesz) 的
                // 交集才有效：ELF 对齐约定下文件源 = seg.offset +
                // (页内起点 - seg.vaddr)，未对齐首部的尾部零填。
                let dst_phys = pfn as u64 * PS as u64;
                let dst_va = crate::direct_map::vm_phys_to_virt(
                    crate::phys_mem::AlignedPhysBytes::new(dst_phys),
                );
                let page_va = va_base + i as u64 * PS as u64;
                let lo = page_va.max(seg.vaddr);
                let hi = (page_va + PS as u64).min(seg.vaddr + seg.filesz);
                if hi > lo {
                    let src_off = (seg.offset + (lo - seg.vaddr)) as usize;
                    let dst_off = (lo - page_va) as usize;
                    let copy_len = (hi - lo) as usize;
                    // SAFETY: destination is the freshly allocated page
                    // through the Direct Map; source is the boot image
                    // slice; both bounds-checked above.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            image.as_ptr().add(src_off),
                            (dst_va.0 as *mut u8).add(dst_off),
                            copy_len,
                        );
                    }
                }
            }
        }

        // C-3 F0 续修取证（task1-close 裁决删除）：RS 入口页 not-present
        // （err=0x14，cr2=entry=0x2246c0）——段循环后立即读回入口 PTE，
        // 分辨"map 静默失败/写错表"（query None）还是"表在内存正确但内核
        // 装载的 root 不符"（query Some）。
        {
            let mut proc = table
                .get_active(slot)
                .ok_or("boot proc slot not active (vmq probe)")?;
            use crate::pagetable::Paging as _;
            match proc.page_table_mut().query(minix_types::VirBytes(entry)) {
                Some((pa, flags)) => {
                    crate::bootmark::mark("nk4a: vmq entry mapped\n");
                    let _ = (pa, flags);
                }
                None => {
                    crate::bootmark::mark("nk4a: vmq entry MISSING\n");
                }
            }
        }

        self.install_boot_stack(ip.endpoint, ip.name(), entry)
    }

    /// E-BOOTFRAME: build and install the initial stack frame for a boot
    /// process, then `sys_exec` with the real `stack`/`ps_str` values.
    ///
    /// C: main.c:346-411 — `minix_stack_params` sizes the frame,
    /// `minix_stack_fill` lays out argc/argv/envp/strings/ps_strings
    /// (byte-exact; the pure builders live in `minix_sys::stack`),
    /// `handle_memory_once` maps the stack range (main.c:400), and the
    /// frame bytes are copied to `vsp` (main.c:402-404, sys_datacopy;
    /// here a Direct Map write). `ps_str = vsp + (psp - frame)` rides
    /// `sys_exec`'s last argument (main.c:409-411); the kernel parks it
    /// in the process's saved RBX (the ps_strings register convention).
    fn install_boot_stack(
        &mut self,
        endpoint: Endpoint,
        name: &str,
        entry: u64,
    ) -> Result<(), &'static str> {
        const PS: usize = crate::region::page_state::PAGE_SIZE as usize;
        use crate::region::page_state::PfnAllocator as _;
        // C: char *argv[] = {ip->proc_name, NULL}; char *envp[] = {NULL}
        // (main.c:347-348).
        let argv = [name];

        // C: minix_stack_params + the frame_size > sizeof(frame) panic
        // (main.c:392-397) — our frame buffer is one page, same budget.
        let params = minix_sys::stack::stack_params(&argv, &[]);
        if params.frame_size > PS {
            return Err("initial stack frame exceeds one page");
        }
        let mut frame = [0u8; PS];
        let filled = minix_sys::stack::stack_fill(
            &argv,
            &[],
            params.frame_size,
            self.ctx.user_sp.0,
            &mut frame[..params.frame_size],
        )
        .map_err(|_| "stack_fill refused the frame")?;
        let vsp = filled.vsp;

        // Map the stack region: C execi->stack_size = DEFAULT_STACK_LIMIT
        // (sys_config.h:25, 4 MiB) below user_sp — the region covers the
        // whole limit and pages materialize on demand; only the frame page
        // is eagerly materialized below. The former one-page region made
        // any call-chain descent past the frame page leave the region —
        // pf find_mut → InvalidAddress → SIGSEGV (real machine NK4-A C-3
        // c11a: RS main's stack read at 0x7fffffffd9c8, one page below the
        // frame page, 2026-09-22).
        const DEFAULT_STACK_LIMIT: u64 = 4 * 1024 * 1024;
        // 下溢防护（NK4 回归评审 P2-2）：user_sp 低于限额时裸减回绕会把
        // region 基址推到地址空间顶端。生产 user_sp=0x7ffffffff000 远高于
        // 限额；宿主测试的 mock user_sp 同样须满足该不变量。
        debug_assert!(
            self.ctx.user_sp.0 > DEFAULT_STACK_LIMIT,
            "user_sp {:#x} leaves no room for the {}-byte stack window",
            self.ctx.user_sp.0,
            DEFAULT_STACK_LIMIT,
        );
        let region_base =
            VirBytes((self.ctx.user_sp.0.saturating_sub(DEFAULT_STACK_LIMIT)) & !(PS as u64 - 1));
        let region = crate::region::VirRegion::with_memtype(
            region_base,
            VirBytes(self.ctx.user_sp.0 - region_base.0),
            crate::region::VrFlags::ANON | crate::region::VrFlags::WRITABLE,
            &crate::memtype::MEM_TYPE_ANON,
        );
        let table = self.ctx.proc_table;
        let slot = table
            .vm_isokendpt(endpoint)
            .map_err(|_| "boot proc endpoint not registered")?;
        let mut proc = table
            .get_active(slot)
            .ok_or("boot proc slot not active")?;
        proc.regions_mut()
            .insert(region)
            .map_err(|_| "boot stack region overlap")?;

        // Materialize the stack page (C: the VR_UNINITIALIZED half of
        // boot_alloc/handle_memory_start) — the pfn stays in hand for the
        // frame-byte write below.
        let frames = self
            .ctx
            .page_frames
            .as_mut()
            .ok_or("page_frames not initialized")?;
        let pfn = self.ctx.page_alloc.alloc_pfn()
            .map_err(|_| "boot stack page allocation failed")?;
        {
            // 帧页在 region 内的偏移：region 基址是 4MiB 窗口底，帧页是
            // vsp 所在页（非 region 首页）。
            let frame_page = VirBytes(vsp & !(PS as u64 - 1));
            let vr = proc
                .regions_mut()
                .find_mut(frame_page)
                .ok_or("stack region vanished")?;
            vr.map_page(
                frames,
                VirBytes(frame_page.0 - region_base.0),
                pfn,
                &crate::memtype::MEM_TYPE_ANON,
            );
        }

        // C: handle_memory_once(vmp, vsp, frame_size, 1) — main.c:400.
        // The fresh page has no CoW to resolve; the call is the
        // C-isomorphic gate (range mapped + writable) before the copy.
        {
            let (regions, pt) = proc.mem_parts_mut();
            crate::fork::handle_memory_once(
                regions,
                frames,
                &mut self.ctx.page_alloc,
                VirBytes(vsp),
                VirBytes(params.frame_size as u64),
                true,
                pt,
            )
            .map_err(|_| "stack range failed the map check")?;
        }

        // Write the frame bytes through the Direct Map (C:
        // sys_datacopy(SELF, frame, endpoint, vsp, ...), main.c:402-404 —
        // the image sits on our stack buffer; the target page is the one
        // just mapped, reachable through the DM window).
        let dst_phys = pfn as u64 * PS as u64;
        let dst_va = crate::direct_map::vm_phys_to_virt(
            crate::phys_mem::AlignedPhysBytes::new(dst_phys),
        );
        // 页内偏移（vsp 所在页的页基 = vsp & !PS-1），region 基址是 4MiB
        // 窗口底——用它会把帧写到页外物理内存（宿主测试实证）。
        let write_off = (vsp & (PS as u64 - 1)) as usize;
        // The frame's byte 0 lands at `vsp`, which sits `write_off` bytes
        // into the mapped page — not at the page start.
        // SAFETY: the DM window maps all of physical memory; the page was
        // just allocated to this process's stack region with refcount 1
        // (no CoW sharing), so the bytes are exclusively ours to write.
        // `write_off + frame_size <= PS` holds because the frame fits one
        // page and vsp + frame_size == user_sp.
        unsafe {
            core::ptr::copy_nonoverlapping(
                frame.as_ptr(),
                (dst_va.0 + write_off as u64) as *mut u8,
                params.frame_size,
            );
        }

        // C: sys_exec(endpoint, vsp, progname, pc, ps_str) — main.c:409-411.
        // `name` stays 0 (kernel-side name semantics are a separate edge);
        // stack/ps_str are now the real ABI values.
        // fix20 probe: name the rejecting errno in the message (kernel
        // answers negative errno; EINVAL = dead endpoint, EFAULT = the
        // name-copy arm — C do_exec.c:37-40 treats that one as NON-fatal
        // "<unset>", so an EFAULT here pinpoints the deviation).
        let mut gateway = self.ctx.gateway.borrow_mut();
        gateway
            .sys_exec(endpoint, entry, vsp, 0, filled.ps_str)
            .map_err(|e| match e {
                crate::kernel_gateway::GatewayError::Kernel(c)
                    if c == -minix_types::EINVAL =>
                {
                    "sys_exec rejected by kernel: EINVAL"
                }
                crate::kernel_gateway::GatewayError::Kernel(c)
                    if c == -minix_types::EFAULT =>
                {
                    "sys_exec rejected by kernel: EFAULT"
                }
                crate::kernel_gateway::GatewayError::Kernel(c)
                    if c == -minix_types::ECALLDENIED =>
                {
                    "sys_exec rejected by kernel: ECALLDENIED"
                }
                crate::kernel_gateway::GatewayError::Kernel(c)
                    if c == -minix_types::EBADREQUEST =>
                {
                    "sys_exec rejected by kernel: EBADREQUEST"
                }
                crate::kernel_gateway::GatewayError::Kernel(c)
                    if c == -minix_types::EIO =>
                {
                    "sys_exec rejected by kernel: EIO(stub)"
                }
                _ => "sys_exec rejected by kernel",
            })?;
        // C: main.c:414-416 — "make it runnable": the boot process was
        // forked with VMINHIBIT|BOOTINHIBIT (kernel/src/lib.rs:1666) and
        // stays parked after exec unless VM clears the inhibit. Without
        // this leg every `exec X ok` booted a permanently stopped
        // process (NK4-A fix23b forensics 2026-09-21: 11 boot procs
        // exec'd, none ever reached `_start`).
        gateway
            .sys_vmctl_boot_inhibit_clear(endpoint)
            .map_err(|_| "VMCTL_BOOTINHIBIT_CLEAR failed")?;
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
        // C: exec_bootproc's pt_new + pt_bind (main.c:344-347) give the
        // boot process a fresh page table, and map_region_init()
        // (main.c:468) its empty region map — exec_bootproc's segment and
        // stack inserts depend on both. Rust: explicit init on the handle
        // (SimPaging stands in for the arch table in test builds).
        // fix21 取证路标（NK4-A 真机挂点定位，task1-close 裁决去留）：
        // panic 消息带上进程名与 PMM 实态（free 页数/最大连续段/VM 自持
        // 页数），把"AllocationFailed 到底是真空闲还是状态被踩"一次问清。
        proc.init_page_table().unwrap_or_else(|e| {
            let alloc = crate::global::page_alloc_mut();
            let stats = alloc.phys_alloc().memstats();
            panic!(
                "init_proc: {} pt failed {:?} free={} largest={} nodes={} self_pages={}",
                ip.name(),
                e,
                stats.free_pages,
                stats.largest_free,
                stats.free_nodes,
                alloc.self_page_count(),
            );
        });
        proc.init_regions();
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
    /// pending. Rust 侧 SYSTEM notify 即其唤醒半(sigset 半为 no-op,
    /// 见 os/kernel/src/proc_table.rs vm_enqueue_and_notify_vm 契约)。
    pub(crate) const SIGKMEM: i32 = 71;

    /// Kernel-signal dispatch — the body of C's `sef_cb_signal_handler`
    /// (main.c:733-749).
    ///
    /// C registers the handler with SEF at startup; since V14-P2-1 the
    /// arrival path is the shared `minix_sef::sef_receive_status`
    /// classification (run_once), which dispatches per set bit of the
    /// SYSTEM notify's sigset — the E1 trap layer carries the notify to
    /// this loop.
    ///
    /// C tail (main.c:744-748): after handling, a pending spare-page
    /// deficit triggers `alloc_cycle()`; `pt_clearmapcache()` has no
    /// counterpart (map cache eliminated, [ARCH: A-1]).
    pub(crate) fn handle_signal(&mut self, signo: i32) {
        // C: "Check for known kernel signals, ignore anything else."
        if signo == Self::SIGKMEM {
            crate::bootmark::mark("nk4a: do-memory enter\n");
            self.do_memory();
            crate::bootmark::mark("nk4a: do-memory done\n");
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
            // C-3 迭代8 取证：memreq 服务结果与目标范围。
            crate::bootmark::mark(&alloc::format!(
                "nk4a: memreq target={} start={:#x} len={:#x} ok={}\n",
                req.target.0, req.start, req.length, ok as u8
            ));
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

        let VmContext { page_alloc, page_frames, page_cache, vfs_queue, .. } = &mut self.ctx;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        let mut proc = match table.get_active(slot) {
            Some(p) => p,
            None => return false,
        };
        // C: VMPTYPE_CHECK 的 handle_memory_start（pagefaults.c:311-330）
        // 逐页调 map_handle_memory——读故障（wrflag=false）同样保证页面
        // 已映射进目标页表，wrflag 只决定 CoW 是否破解。此前经由
        // handle_memory_once 的读路径只确认 region 存在就返回 Ok、不写
        // 任何 PTE——内核重派 data_copy_vmcheck 仍 walk=NP，挂起-服务-
        // 重派死循环（真机 NK4-A C-3 c9a 轮：memreq ok=1 后 DIAGCTL 重派
        // 仍 susp-again，2026-09-22）。此处走缺页路径同一核心
        // handle_pagefault（G-V12-8：映射必须落 PTE，记账与硬件表一致）。
        let (regions, pt) = proc.mem_parts_mut();
        let page_mask = crate::region::PAGE_SIZE as u64 - 1;
        let mut va = req.start & !page_mask;
        let end = req.start.saturating_add(req.length);
        while va < end {
            let region = match regions.find_mut(minix_types::VirBytes(va)) {
                Some(r) => r,
                None => return false,
            };
            match crate::cow_exec_pf::handle_pagefault(
                req.target,
                region,
                frames,
                page_alloc,
                minix_types::VirBytes(va),
                req.write,
                table,
                page_cache,
                vfs_queue,
                pt,
            ) {
                Ok(crate::cow_exec_pf::PagefaultAction::Handled)
                | Ok(crate::cow_exec_pf::PagefaultAction::MappedNewPage)
                | Ok(crate::cow_exec_pf::PagefaultAction::CowResolved) => {}
                // VFS 后备页需异步 I/O：C 在此处挂请求等 VFS 完成后再答；
                // 本轮 fail closed（ok=0 → 内核按 Fault 处理），登记不假完成。
                Ok(crate::cow_exec_pf::PagefaultAction::Suspended)
                | Ok(crate::cow_exec_pf::PagefaultAction::AccessViolation) => return false,
                Err(_) => return false,
            }
            // 单页步进；region 边界由下一轮 find_mut 重查（C
            // map_handle_memory 逐页遍历同形）。
            va += crate::region::PAGE_SIZE as u64;
        }
        true
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
        // V14-P2-1 (plan A): the receive half goes through the shared
        // `minix_sef::sef_receive_status` — VM is minix-sef's fourth
        // consumer. SYSTEM notifies surface as signals, RS pings are
        // ponged and swallowed inside the library, everything else falls
        // through to the loop below (C sef.c:149-260 wrapping main.c:113).
        let mut raw_msg = minix_types::Message::default();
        let mut pending_signo: Option<i32> = None;
        let rcv_sts_raw = {
            let mut guard = self.transport.borrow_mut();
            let mut sef = crate::ipc::transport::SefAdapter(guard.as_mut());
            let rcv = minix_sef::sef_receive_status(
                &mut sef,
                minix_types::Endpoint::ANY,
                &mut raw_msg,
                &mut |signo| pending_signo = Some(signo),
            );
            match rcv {
                Ok(r) => {
                    // C-3 迭代8 取证（task1-close 裁决删除）：receive 返回
                    // 的 source/type（限 16 次）——定位 VM 的接收自旋回路的
                    // 内容。
                    #[cfg(not(feature = "mock"))]
                    {
                        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                        static RCV_LOG: AtomicUsize = AtomicUsize::new(0);
                        let rn = RCV_LOG.fetch_add(1, AtomicOrd::Relaxed);
                        if rn < 16 {
                            crate::bootmark::mark(&alloc::format!(
                                "nk4a: rcv{} src={} type={:#x}\n",
                                rn, r.source.0, r.message.m_type
                            ));
                        }
                    }
                    r
                }
                // [ARCH: A-14] V9-P0-1: C panics (main.c:122-123); a
                // user-space server must survive bad IPC — drop + audit.
                Err(e) => {
                    drop(guard);
                    self.ctx.dropped_messages = self.ctx.dropped_messages.saturating_add(1);
                    // C-3 迭代10 取证：receive Err 的错误码（限 4 次）——
                    // 判别 VM 接收自旋的 Err 来源（EIO/EAGAIN/…）。
                    #[cfg(not(feature = "mock"))]
                    crate::bootmark::mark(&alloc::format!(
                        "nk4a: rcv-err {e:?}\n"
                    ));
                    audit_log!("[VM IPC] ipc_receive() failed — message dropped");
                    return RunStep::ReceiveFailed;
                }
            }
        };

        // 信号抵达裁决(V14-P2-1 影响②,按 kernel 侧既定契约):SYSTEM
        // notify 是 `send_sig(VM, SIGKMEM)` 的唤醒半——sigset 半是文档化
        // no-op(SIGKMEM=71 超出 64 位 SigSet,os/kernel/src/proc_table.rs
        // vm_enqueue_and_notify_vm 契约),VM 经 MEMREQ_GET 探测细节而不
        // 读信号号;SIGKSIG 信号管理器家族 VM 不消费。
        if pending_signo.is_some() {
            self.handle_signal(VmServer::SIGKMEM);
            return RunStep::Handled;
        }

        let msg = raw_msg;
        let rcv_sts = crate::ipc::transport::IpcStatus { flags: rcv_sts_raw.status as u32 };

        // C: if(is_ipc_notify(rcv_sts)) { continue; } (main.c:126-129).
        // SEF already took the SYSTEM/RS-ping notifies; what reaches here
        // is a leftover notification from any other source — still an
        // async signal, not a request; skipped before endpoint validation
        // (V10-P1-1). PingInvalid (RS notify that failed the ping test)
        // lands here too, matching C's sef.c:208-214 fall-through.
        // C: if(is_ipc_notify(rcv_sts)) { continue; } (main.c:126-129).
        // SEF already took the SYSTEM/RS-ping notifies; what reaches here
        // is a leftover notification from any other source — still an
        // async signal, not a request; skipped before endpoint validation
        // (V10-P1-1). PingInvalid (RS notify that failed the ping test)
        // lands here too, matching C's sef.c:208-214 fall-through.
        //
        // NK4-A C-3 迭代8：落入臂排空 memreq——SIGKMEM=71 超出 64 位
        // SigSet，内核 D-20 唤醒通知的 sigset 为空，sef 提不出 signo 而
        // 于此落入；D-20 契约（VM 经 MEMREQ_GET 探测而非读信号号）要求
        // 此处排空，否则 RS 的挂起请求永不被服务（真机：sys-susp 后
        // VM 静默死等，2026-09-22）。队列为空时 memreq_get 立即返回，
        // 开销可忽略。
        if rcv_sts.is_notify() {
            self.do_memory();
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

        // E-VFSWIRE drain step: if the queue holds an unsent active VFS
        // request, build its VFS_VMCALL wire and send it. A failed send
        // (pre-E1: the transport always refuses) clears the sent mark so
        // the next round retries — behaviorally identical to today's
        // "never sent" state, no regression.
        if let Some(call_msg) = self.ctx.vfs_queue.take_pending_vfs_call()
            && self
                .transport
                .borrow_mut()
                .send(minix_types::Endpoint::VFS, &call_msg)
                .is_err()
        {
            self.ctx.vfs_queue.mark_send_failed();
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
                Ok(()) => {
                    // E-BIRTHFACE（NS1）出生应答半：C 的 do_sef_init_request
                    // 内部 process_init 尾部先回 RS_INIT+result
                    //（sef_init.c:113-117），主循环的 SUSPEND
                    //（vm/main.c:150-152 "do not reply to RS"）压的是
                    // 第二回复。本臂此前只学 SUSPEND 半，RS boot step3
                    // 等不到应答。应答失败 = RS 已不在，同 Reply 臂
                    // fail-fast（vm/main.c:195-197 panic 同款）。
                    self.transport
                        .borrow_mut()
                        .send(
                            RS_PROC_NR,
                            &minix_sef::sef_init_reply(minix_types::OK),
                        )
                        .unwrap_or_else(|_| {
                            panic!("ipc_send() failed (RS_INIT birth report)")
                        });
                    return DispatchAction::Suspend;
                }
                Err(e) => {
                    // NS2/E-RPROCTAB: C's process_init replies RS_INIT+result
                    // to RS *unconditionally* — success or failure
                    // (sef_init.c:110-119; VM registers the async-response
                    // variant, main.c:229 "avoid a boot-time deadlock") — and
                    // only then does the main loop treat the failure as fatal
                    // (main.c:151). Reply with the mapped errno first so RS's
                    // catch_boot_init_ready fails visibly on the non-OK
                    // result (main.c:805-807) instead of blocking forever.
                    self.transport
                        .borrow_mut()
                        .send(RS_PROC_NR, &minix_sef::sef_init_reply(e.to_errno()))
                        .unwrap_or_else(|_| panic!("ipc_send() failed (RS_INIT birth report)"));
                    // [A-14] fail-closed continues after the reply: C panics
                    // here (main.c:151); minix-rs drops + counts + audits —
                    // no second reply, VM stays alive minus the whole-system
                    // outage ([ARCH: A-14] / V9-P0-1).
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
        let rproctab = ipc_call_rs_init(self, init.rproctab_gid)?;

        // 2. Register ACL for each boot service
        // C: for(i=0; i<NR_BOOT_PROCS; i++) if(rprocpub[i].in_use) map_service(&rprocpub[i]);
        for entry in rproctab.iter() {
            if !entry.in_use { continue; }
            let slot = table.vm_isokendpt(entry.endpoint)
                .map_err(|_| VmError::InvalidProcess)?;
            let mut proc = table.get_active(slot)
                .ok_or(VmError::InvalidProcess)?;
            let is_sys = !entry.is_user;
            let mask = Some(crate::acl::AclMask::from_bits_truncate(entry.call_mask));
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

    /// C pagefaults.c `handle_pagefault` 的"不可服务"终局收口
    /// （unknown region :99-104、ro-map 写 :112-116、map_pf 失败
    /// :146-151 三处共享的同一段尾巴）：`sys_kill(SIGSEGV)` 投递给
    /// 故障进程 + `sys_vmctl(VMCTL_CLEAR_PAGEFAULT)` 清其挂起位。
    /// 两条缺一不可——C 对两者都 `panic` on error；漏清挂起位则故障
    /// 进程永停 RTS_PAGEFAULT（NK4-A Task A 真机 c17a：cr2=0 的
    /// noaddr 出口只回 Error 不清挂起 → 全系统静默死锁，2026-09-22）。
    /// 失败仅审计不 panic（[ARCH: A-14] fail-closed 姿态，同 wro 臂旧实现）。
    fn pf_fail_segv(&mut self, proc_endpoint: Endpoint) {
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
    }

    /// Pagefault dispatch — decodes VmPagefaultIn from Message, delegates to cow_exec_pf.
    /// C (main.c:147-156): do_pagefaults(&msg); continue;
    // NK4-A Task A 探针（task1-close 裁决删除）：mock 门在本 crate 未声明
    // feature（与既有 rcv 探针同款），函数级放行以零新增编译警告。
    #[allow(unexpected_cfgs)] // 外属性：放行函数体内 mock 门 cfg
    fn dispatch_pagefault(&mut self, msg: &Message) -> VmReply {
        // NK4-A Task A 取证（task1-close 裁决删除）：入口解出 fault 地址，
        // 供各静默出口打印 cr2（停滞判据 = vm-pf recv 后无 bytes，需在每
        // 个跳过 bytes 的出口定性）。
        #[cfg(not(feature = "mock"))]
        let pf_cr2 = minix_types::VmPagefaultIn::decode_message(msg).vaddr.0;
        macro_rules! pf_exit {
            ($tag:literal) => {
                #[cfg(not(feature = "mock"))]
                {
                    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                    static PF_EXIT_LOG: AtomicUsize = AtomicUsize::new(0);
                    if PF_EXIT_LOG.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                        crate::bootmark::mark(&alloc::format!(
                            concat!("nk4a: pf-exit ", $tag, " cr2={:#x}\n"),
                            pf_cr2
                        ));
                    }
                }
            };
        }
        // minix-rs: the kernel packs vpf_addr/vpf_flags in the dedicated
        // m_vm_pagefault union member (os/kernel/src/page_fault.rs:142-166);
        // the faulting endpoint is m_source (C: pagefaults.c:242).
        let request = VmPagefaultIn::decode_message(msg);
        let table = VmProcTable::get_global();
        let slot = match table.vm_isokendpt(request.endpoint) {
            Ok(s) => s,
            Err(_) => {
                pf_exit!("badendpt");
                return VmReply::Error(VmError::InvalidProcess);
            }
        };
        let mut proc = match table.get_active(slot) {
            Some(p) => p,
            None => {
                pf_exit!("inactive");
                return VmReply::Error(VmError::InvalidProcess);
            }
        };
        let proc_endpoint = proc.endpoint();
        let fault_addr = request.vaddr;
        // C-3 F0 续修取证（task1-close 裁决删除）：VM 收到转发 PF 的现场。
        crate::bootmark::mark("nk4a: vm-pf recv\n");
        // C pagefaults.c:109-119 — a write to a read-only region is not a
        // servable fault: deliver SIGSEGV to the faulting process, clear its
        // kernel pagefault suspension (RTS_PAGEFAULT), and stop. Pre-E2 the
        // trap stub answers -EIO for both calls; the failure is audited and
        // the error reply still counts the episode (G-V12-6).
        // Read-only borrow — ends before the mem_parts_mut split below.
        if request.write
            && !proc.regions().find(fault_addr).is_some_and(|r| r.is_writable())
        {
            pf_exit!("wro");
            // C: pagefaults.c:112-116 — SIGSEGV + CLEAR_PAGEFAULT 收口
            // （原内联两段与 pf_fail_segv 同体，提取共用不改语义）。
            self.pf_fail_segv(proc_endpoint);
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
            None => {
                // NK4-A Task A 修复（C pagefaults.c:89-105 对位）：
                // unknown region 的 fault 不可服务——SIGSEGV +
                // CLEAR_PAGEFAULT 收口后才返回。旧实现只回
                // Error(InvalidAddress)，挂起位无人清，故障进程永停
                // RTS_PAGEFAULT（真机 c17a：cr2=0 → 全系统静默死锁）。
                pf_exit!("noaddr");
                self.pf_fail_segv(proc_endpoint);
                return VmReply::Error(VmError::InvalidAddress);
            }
        };
        let service_outcome = crate::cow_exec_pf::handle_pagefault(
            proc_endpoint, region, frames, page_alloc,
            fault_addr, request.write, table, page_cache, vfs_queue, pt,
        );
        // C-3 F0 续修取证（task1-close 裁决删除）：填充后直读叶子物理页
        // 首 8 字节——对照 ELF 字节（RS: 48 83 e4 f0），分辨"内容没拷上/
        // 拷错页"与"内容对但 CPU 视图不同"。须在 proc 记账前用 pt（借用
        // 顺序），随后才轮到 proc 计数。
        let probe_ok = matches!(
            service_outcome,
            Ok(
                crate::cow_exec_pf::PagefaultAction::Handled
                    | crate::cow_exec_pf::PagefaultAction::MappedNewPage
                    | crate::cow_exec_pf::PagefaultAction::CowResolved
            )
        );
        if probe_ok {
            let aligned = minix_types::VirBytes(
                fault_addr.0 & !(crate::region::PAGE_SIZE as u64 - 1),
            );
            use crate::pagetable::Paging as _;
            if let Some((pa, _fl)) = pt.query(aligned) {
                let dv = crate::direct_map::vm_phys_to_virt(
                    crate::phys_mem::AlignedPhysBytes::new(pa.0),
                );
                let bytes =
                    unsafe { core::slice::from_raw_parts(dv.0 as *const u8, 8) };
                let mut hex = alloc::format!("nk4a: vm-pf bytes ");
                for b in bytes {
                    hex.push_str(&alloc::format!("{:02x}", b));
                }
                // NK4-C 第 12 轮（task1-close 裁决删除）：PA 别名检测——
                // 若分配器把同一物理帧发给两个 VA，后一个零填充会清掉前一个
                // VA 已写内容（「栈槽自零」机制）。记录 pa→首 VA，重复时
                // 打印别名对。
                {
                    use core::sync::atomic::{
                        AtomicU64, AtomicUsize, Ordering as AtomicOrd,
                    };
                    static SEEN_PA: AtomicUsize = AtomicUsize::new(0);
                    static SEEN: [AtomicU64; 96] = [const { AtomicU64::new(0) }; 96];
                    static SEEN_VA: [AtomicU64; 96] = [const { AtomicU64::new(0) }; 96];
                    let n = SEEN_PA.load(AtomicOrd::Relaxed);
                    let mut i = 0;
                    let mut alias_va: u64 = 0;
                    while i < n && i < 96 {
                        if SEEN[i].load(AtomicOrd::Relaxed) == pa.0 {
                            alias_va = SEEN_VA[i].load(AtomicOrd::Relaxed);
                            break;
                        }
                        i += 1;
                    }
                    if alias_va != 0 {
                        hex.push_str(&alloc::format!(
                            "  PA-ALIAS pa={:#x} first_va={:#x} this_va={:#x}",
                            pa.0,
                            alias_va,
                            aligned.0
                        ));
                    } else if n < 96 {
                        SEEN[n].store(pa.0, AtomicOrd::Relaxed);
                        SEEN_VA[n].store(aligned.0, AtomicOrd::Relaxed);
                        SEEN_PA.store(n + 1, AtomicOrd::Relaxed);
                    }
                }
                // NK4-C 第 10 轮取证（task1-close 裁决删除）：**故障地址处**
                // 的 8 字节（非页首）——页首全零可能是 ELF gap 的合法形状，
                // 故障地址处的零才是「零填充错页」的直接证据。
                let off = (fault_addr.0 - aligned.0) as usize;
                let fbytes = unsafe {
                    core::slice::from_raw_parts(
                        (dv.0 + off as u64) as *const u8,
                        8,
                    )
                };
                hex.push_str(&alloc::format!("  fa={:#x} fa8=", fault_addr.0));
                for b in fbytes {
                    hex.push_str(&alloc::format!("{:02x}", b));
                }
                hex.push('\n');
                crate::bootmark::mark(&hex);
            } else {
                // 成功出口但页表读不到 PTE——bytes 静默缺失的另一候选
                // （task1-close 裁决删除）。
                pf_exit!("ok-nopte");
            }
        }
        match service_outcome {
            Ok(action) => {
                // V11/T31: fault accounting. Minix3's VM has no fault
                // counters — the fields are a minix-rs extension following
                // Linux getrusage semantics: minor = satisfied without
                // block I/O (fresh zero page, CoW copy, in-place handled);
                // major = the fault needed VFS I/O, counted at enqueue
                // (Linux counts major when I/O is required, not at
                // completion). Access violations are not faults served.
                match action {
                    crate::cow_exec_pf::PagefaultAction::Suspended => {
                        pf_exit!("susp");
                        proc.inc_major_fault()
                    }
                    crate::cow_exec_pf::PagefaultAction::AccessViolation => {
                        // C map_pf 返回 EFAULT → pagefaults.c:144-151
                        // "pagefault not handled"：SIGSEGV + 清挂起。
                        // 旧实现只计数返回 Ok，挂起位永不清（同 noaddr
                        // 缺陷类的未观测分支）。
                        pf_exit!("accvio");
                        self.pf_fail_segv(proc_endpoint);
                    }
                    crate::cow_exec_pf::PagefaultAction::Handled
                    | crate::cow_exec_pf::PagefaultAction::MappedNewPage
                    | crate::cow_exec_pf::PagefaultAction::CowResolved => proc.inc_minor_fault(),
                }
                // C do_memory tail: memreq_reply → kernel ClearPageFault
                // re-enqueues the parked process (do_vmctl.c:35). The
                // message-shape arm (kernel VM_PAGEFAULT mini_send,
                // exception.c:112-129) must unpark the same way — without
                // it the faulting process stays RTS_PAGEFAULT forever and
                // the system idles after the first user fault (NK4-A C-3
                // 真机：fwd Delivered 后全静默的另一半，2026-09-22).
                if matches!(
                    action,
                    crate::cow_exec_pf::PagefaultAction::Handled
                        | crate::cow_exec_pf::PagefaultAction::MappedNewPage
                        | crate::cow_exec_pf::PagefaultAction::CowResolved
                ) {
                    if let Err(e) = self
                        .ctx
                        .gateway
                        .borrow_mut()
                        .sys_vmctl_clear_pagefault(proc_endpoint)
                    {
                        let _ = &e;
                        audit_log!("[VM PF] clear_pagefault failed: {e:?}");
                        pf_exit!("clrpf");
                    }
                }
                VmReply::Ok
            }
            Err(e) => {
                // C-3 F0 续修取证（task1-close 裁决删除）。
                crate::bootmark::mark(&alloc::format!(
                    "nk4a: vm-pf err {:?}\n", e
                ));
                // C pagefaults.c:144-151 — 服务失败同属"不可服务"终局：
                // SIGSEGV + 清挂起（旧实现只回 Error，挂起位不清）。
                self.pf_fail_segv(proc_endpoint);
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

fn ipc_call_rs_init(server: &mut VmServer, rproctab_gid: i32) -> Result<RprocTab, VmError> {
    // C contract (ground truth: main.c:137-155 + main.c:237-260 + sef_init.c:193):
    //   1. RS *sends* RS_INIT to VM — main.c:149 gates on
    //      `msg.m_source == RS_PROC_NR`; VM never sends RS_INIT itself.
    //   2. The message carries mess_rs_init.rproctab_gid — a grant RS holds
    //      on its public process table (m_rs_init, ipc.h:1858-1867).
    //   3. VM's init callback copies the whole table in one safecopy —
    //      sys_safecopyfrom(RS_PROC_NR, gid, 0, rprocpub, sizeof(rprocpub))
    //      — the granter is RS_PROC_NR, **not SELF** (main.c:244-247), and
    //      `rprocpub` is `struct rprocpub[NR_BOOT_PROCS]`.
    //   4. map_service(&rprocpub[i]) per in_use entry — the ACL loop in
    //      `rs_handshake` (main.c:249-255).
    const ENTRIES: usize = minix_types::NR_BOOT_PROCS;
    // 行宽是共享快照 `RprocpubSnap`（`[ARCH: A-4]` 单一权威）：RS 的公共表
    // 按 `rproctab_gid` 授权读出的就是这一行列，C 的 `struct rprocpub`
    // 字节镜像（`rprocpub_off`）随对齐退役。
    const ENTRY_SIZE: usize = core::mem::size_of::<minix_types::RprocpubSnap>();
    let mut buf = alloc::vec![0u8; ENTRIES * ENTRY_SIZE];
    {
        let mut gateway = server.ctx.gateway.borrow_mut();
        let res = gateway.sys_safecopyfrom(Endpoint::RS, rproctab_gid, 0, &mut buf);
        // NK4-C B4 取证（task1-close 裁决删除）：真 errno 被下游
        // `map_err(|_| InvalidEndpoint)` 吞掉——VM 无论内核回什么都
        // 统一上报 ESRCH(3)，RS 侧只看到“init 失败”看不到根因。
        // 这里把内核原始返回码 + gid + 请求字节数落串口，分辨
        // 坏 grant / 越界拷贝 / endpoint 不存在。不改判定链。
        #[cfg(not(feature = "mock"))]
        if let Err(crate::kernel_gateway::GatewayError::Kernel(code)) = res {
            crate::bootmark::mark(&alloc::format!(
                "nk4a: vm-rswire gid={} len={} err={}\n",
                rproctab_gid,
                buf.len(),
                code,
            ));
        }
        res.map_err(|_| VmError::InvalidEndpoint)?;
    }
    let mut tab = RprocTab::EMPTY;
    for (i, entry) in tab.entries.iter_mut().enumerate() {
        // SAFETY(读取): 快照行是 repr(C) POD，从授权捞回的字节位逐行读；
        // `read_unaligned` 不假设缓冲对齐（内核 safecopy 写的是字节流）。
        let w: minix_types::RprocpubSnap =
            unsafe { core::ptr::read_unaligned(buf[i * ENTRY_SIZE..].as_ptr().cast()) };
        let endpoint = Endpoint(w.endpoint);
        // C rs.h:188 — IS_RPUB_BOOT_USR(rpub) is (endpoint == INIT_PROC_NR).
        *entry = RprocEntry {
            in_use: w.in_use != 0,
            endpoint,
            call_mask: w.vm_call_mask,
            is_user: endpoint == Endpoint::INIT,
        };
    }
    Ok(tab)
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
    /// V13a: full wire width — the rprocpub wire carries 2×u32 chunks
    /// combined into u64 (rprocpub.rs), and truncating to u32 would drop
    /// the authorization bits for VM calls +32..+48.
    call_mask: u64,
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

/// C: `struct rprocpub rprocpub[NR_BOOT_PROCS]` (glo.h) — the table the
/// RS_INIT grant carries and `sef_cb_init_fresh` walks (main.c:249-255).
struct RprocTab {
    entries: [RprocEntry; minix_types::NR_BOOT_PROCS],
}

impl RprocTab {
    const EMPTY: Self = Self {
        entries: [RprocEntry::EMPTY; minix_types::NR_BOOT_PROCS],
    };
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

    /// V14-P2-1 映射:SYSTEM notify 上浮为 `SefEvent::Signal` → 唤醒半
    /// 契约(见 run_once 注释与 os/kernel/src/proc_table.rs
    /// vm_enqueue_and_notify_vm)→ `handle_signal(SIGKMEM)` → do_memory
    /// 探测。观测量:memreq 脚本被消费(GET/REPLY/终止 GET 三次内核调用,
    /// verdict OK);V14 前该 notify 会被旧循环当普通 notify 吞掉。
    #[test]
    fn sef_signal_notify_dispatches_sigset_bits() {
        with_test_mock_base(|| {
            reset_boot_slots();
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
                // memtype 必须在位（C 的每个 VM region 都有类型；
                // handle_pagefault 经 memtype 分派，None 即 NoMemType）。
                proc.regions_mut()
                    .insert(crate::region::VirRegion::with_memtype(
                        VirBytes(0x3000_0000),
                        VirBytes(0x4000),
                        crate::region::VrFlags::WRITABLE | crate::region::VrFlags::ANON,
                        &crate::memtype::MEM_TYPE_ANON,
                    ))
                    .unwrap();
            }

            // Scripted kernel: CHECK over the region, then queue empty(与
            // test_handle_signal_routes_sigkmem_only 同脚本)。
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
            canned.reply_message(check);
            canned.reply(0);
            canned.reply(minix_types::ENOENT);
            let canned = alloc::rc::Rc::new(canned);
            install_canned_gateway(&mut server, alloc::rc::Rc::clone(&canned));

            // SYSTEM notify(kernel send_sig 的唤醒半;sigset 半为 no-op,
            // 不携带位)。
            let mut notify = Message::default();
            notify.m_source = Endpoint::SYSTEM;
            notify.m_type = minix_types::NOTIFY_MESSAGE;
            handle.queue_receive(notify, IpcStatus { flags: 4 /* NOTIFY */ });

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);
            assert_eq!(server.dropped_messages(), 0, "signal is handled, not dropped");
            assert!(handle.sent().is_empty(), "signal must not be replied to");

            // 内核可观测:排空循环真的跑了(GET/REPLY/终止 GET)。
            let sent = canned.sent.borrow();
            assert_eq!(sent.len(), 3, "GET, REPLY, second GET");
            {
                // SAFETY: reply wire inspection (SYS_VMCTL M1 fields).
                let m1 = unsafe { &sent[1].m_u.m_m1 };
                assert_eq!(m1.m1i1, ep.0, "reply targets the fetched request");
                assert_eq!(m1.m1i2, 15, "VMCTL_MEMREQ_REPLY");
                assert_eq!(m1.m1i3, 0, "valid writable range → OK verdict");
            }

            reset_boot_slots();
        });
    }

    /// V14-P2-1 映射:RS ping 在 minix-sef 内被 pong 并吞掉(不上浮、不
    /// 入分派);后续消息正常继续接收。
    #[test]
    fn sef_ping_ponged_and_swallowed() {
        with_test_mock_base(|| {
            reset_boot_slots();
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

            // RS ping:RS 源 + NOTIFY_MESSAGE + notify 状态位。
            let mut ping = Message::default();
            ping.m_source = Endpoint::RS;
            ping.m_type = minix_types::NOTIFY_MESSAGE;
            handle.queue_receive(ping, IpcStatus { flags: 4 /* NOTIFY */ });
            // 后续:普通 notify(NONE 源)——sef 放行为 Call,主循环按
            // main.c:126-129 吞掉。
            let mut stray = Message::default();
            stray.m_source = Endpoint::NONE;
            stray.m_type = 0;
            handle.queue_receive(stray, IpcStatus { flags: 4 /* NOTIFY */ });

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);
            assert_eq!(server.dropped_messages(), 0);
            assert_eq!(handle.notified(), alloc::vec![Endpoint::RS], "ping → pong");
            assert!(handle.sent().is_empty(), "ping must not produce a reply");

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
    fn test_pagefault_unknown_region_sigsegv_and_clears_park() {
        // NK4-A Task A 防回归（C pagefaults.c:89-105 对位）：不属于任何
        // region 的 fault 必须 SIGSEGV + CLEAR_PAGEFAULT 收口。修复前
        // noaddr 出口只回 Error(InvalidAddress)——下面两条 mock 网关断言
        // 皆为空，故障进程滞留 RTS_PAGEFAULT（真机 c17a：cr2=0 全系统
        // 静默死锁）。既有 wro 测试只覆盖"写只读 region"路径，无断言
        // 触及 unknown region 出口——判别性在此。
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
                    VirBytes(0x4000_0000),
                    VirBytes(0x4000),
                    VrFlags::ANON | VrFlags::WRITABLE,
                );
                region.def_memtype = Some(&crate::memtype::MEM_TYPE_ANON);
                proc.regions_mut().insert(region).unwrap();
            }

            let mock = alloc::rc::Rc::new(core::cell::RefCell::new(
                crate::kernel_gateway::MockGateway::new(),
            ));
            server.ctx.gateway = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(SharedMockGateway(alloc::rc::Rc::clone(&mock)))
                    as alloc::boxed::Box<dyn crate::kernel_gateway::KernelGateway>,
            ));

            // 读故障 @0x0：不落任何 region（真机 c17a 的 cr2=0 形态）。
            let mut msg = Message::default();
            msg.m_source = ep;
            msg.m_type = minix_types::VM_PAGEFAULT as i32;
            let mut pf = minix_types::ipc::MessVmPagefault::default();
            pf.vpf_addr = 0x0;
            pf.vpf_flags = 0;
            unsafe {
                msg.m_u.m_vm_pagefault = pf;
            }
            let kernel_status = IpcStatus { flags: 1 << 16 };
            let action = server.dispatch_on_msg(&msg, &kernel_status, UserSlot::new(0));
            assert!(matches!(action, DispatchAction::NoReply));
            {
                let gw = mock.borrow();
                assert_eq!(
                    gw.kills.borrow().as_slice(),
                    &[(ep, minix_types::SIGNAL_SEGMENT_VIOLATION)],
                    "unknown-region fault must SIGSEGV the faulting process \
                     (C pagefaults.c:99-101)"
                );
                assert_eq!(
                    gw.clear_pagefaults.borrow().as_slice(),
                    &[ep],
                    "unknown-region fault must clear the RTS_PAGEFAULT park \
                     (C pagefaults.c:102-104)"
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
            user_sp: VirBytes(0x7fff_ffff_f000),
        }
    }

    /// E-BOOTFRAME: `install_boot_stack` builds the initial frame with the
    /// `minix_sys::stack` builders, maps one stack page below `user_sp`,
    /// writes the frame bytes there, and hands `sys_exec` the real
    /// `stack`/`ps_str` (C: main.c:346-411). MockGateway captures the
    /// wire values; the written bytes are read back through the Direct Map
    /// and compared against an independently built frame.
    /// E-VFSWIRE slice 3: the run_once drain step builds the VFS_VMCALL
    /// wire from the active queue entry and sends it to VFS over the
    /// transport (C vfs.c:83-90 field mapping), verified end to end through
    /// TestIpcTransport's send log.
    #[test]
    fn test_vfs_call_drain_sends_wire_to_vfs() {
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

            // Queue an FdClose via the enqueue half (V11/T10 seam).
            server.ctx.vfs_queue.request(crate::vfs_queue::VfsRequest {
                request_type: crate::vfs_queue::VfsRequestType::FdClose,
                req_id: 0,
                caller_endpoint: Endpoint::PFS,
                fd: 5,
                offset: 0x1234_0000,
                length: 0x2000,
                callback: None,
                state: None,
                sent: false,
            });

            // Drain: build + send to VFS over the transport.
            let call_msg = server.ctx.vfs_queue.take_pending_vfs_call().expect("wire built");
            server
                .transport
                .borrow_mut()
                .send(minix_types::Endpoint::VFS, &call_msg)
                .expect("test transport accepts");

            let sends = handle.sent();
            assert_eq!(sends.len(), 1, "exactly one VFS_VMCALL sent");
            let (dest, sent_msg) = &sends[0];
            assert_eq!(*dest, minix_types::Endpoint::VFS);
            assert_eq!(sent_msg.m_type, minix_types::VFS_VMCALL);
            // SAFETY: m10 arm is what the builder wrote.
            let call = unsafe { &sent_msg.m_u.m_vm_vfs_call };
            assert_eq!(call.req, minix_types::VMVFSREQ_FDCLOSE);
            assert_eq!(call.fd, 5);
            assert_eq!(call.endpoint, Endpoint::PFS.0);
            assert_eq!(call.offset, 0x1234_0000);
            assert_eq!(call.length, 0x2000);
            assert_eq!(call.req_id, 1, "first allocated req_id");

            // Sent mark set: a second take returns nothing until failure
            // marks it again for retry.
            assert!(server.ctx.vfs_queue.take_pending_vfs_call().is_none());
            server.ctx.vfs_queue.mark_send_failed();
            assert!(server.ctx.vfs_queue.take_pending_vfs_call().is_some());
        });
    }

    #[test]
    fn test_install_boot_stack_writes_frame_and_exec_values() {
        with_test_mock_base(|| {
            reset_boot_slots();

            // Boot proc in slot 9 (the init test's PFS stand-in shape).
            let mut img = BootImage::empty();
            img.proc_nr = 9;
            img.endpoint = Endpoint::PFS;
            img.start_addr = 0x100_0000;
            img.proc_name[0] = b'p';
            img.proc_name[1] = b'f';
            img.proc_name[2] = b's';
            let boot_procs = [vm_boot_image(), img];
            let regions = test_free_regions();
            let mut server =
                VmServer::new_with_boot_params(boot_params(&regions, &boot_procs, &[]));
            server.init();

            // Recording gateway (shared-handle delegate pattern).
            let mock = alloc::rc::Rc::new(core::cell::RefCell::new(
                crate::kernel_gateway::MockGateway::new(),
            ));
            server.ctx.gateway = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(SharedMockGateway(alloc::rc::Rc::clone(&mock)))
                    as alloc::boxed::Box<dyn crate::kernel_gateway::KernelGateway>,
            ));

            const PS: usize = crate::region::page_state::PAGE_SIZE as usize;
            let entry = 0x40_1000u64;
            server
                .install_boot_stack(Endpoint::PFS, "pfs", entry)
                .expect("boot stack install");

            // Expected placement from the same pure builders the
            // production path uses.
            let params = minix_sys::stack::stack_params(&["pfs"], &[]);
            assert!(params.frame_size <= PS);
            let user_sp = server.ctx.user_sp.0;
            let vsp = user_sp - params.frame_size as u64;
            let mut expect = alloc::vec![0u8; params.frame_size];
            let placement = minix_sys::stack::stack_fill(
                &["pfs"],
                &[],
                params.frame_size,
                user_sp,
                &mut expect,
            )
            .expect("reference frame builds");

            // sys_exec wire: (endpoint, ip=entry, stack=vsp, ps_str).
            let (ep, ip_val, stack_val, ps_val) = mock
                .borrow()
                .last_exec
                .get()
                .expect("sys_exec recorded by mock");
            assert_eq!(ep, Endpoint::PFS);
            assert_eq!(ip_val, entry);
            assert_eq!(stack_val, vsp, "stack = vsp (main.c:410)");
            assert_eq!(ps_val, placement.ps_str, "ps_str absolute (main.c:411)");
            assert!(ps_val < user_sp);

            // Frame bytes readable at vsp through the Direct Map.
            let table = VmProcTable::get_global();
            let proc = table.get_active(UserSlot(9)).expect("slot 9 active");
            let frame_page = vsp & !(PS as u64 - 1);
            let region = proc
                .regions()
                .find(minix_types::VirBytes(frame_page))
                .expect("stack region registered");
            // 栈 region 是 user_sp 下的 4MiB 窗口（C DEFAULT_STACK_LIMIT），
            // 帧页在其中偏移 (frame_page - region.vaddr) 处。
            let frame_off = frame_page - region.vaddr.0;
            let pfn = region
                .get_slot(minix_types::VirBytes(frame_off))
                .and_then(|s| s.pfn())
                .expect("stack page materialized");
            let off_in_page = (vsp - frame_page) as usize;
            let phys_page = pfn as u64 * PS as u64;
            let va = crate::direct_map::vm_phys_to_virt(
                crate::phys_mem::AlignedPhysBytes::new(phys_page),
            );
            // SAFETY: DM window covers physical memory; the bytes were just
            // written by install_boot_stack and nothing reuses the page.
            // The read starts mid-page at `off_in_page` (frame byte 0 = vsp).
            let written = unsafe {
                core::slice::from_raw_parts(
                    (va.0 + off_in_page as u64) as *const u8,
                    params.frame_size,
                )
            };
            assert_eq!(written, &expect[..], "frame bytes land at vsp");
        });
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
    /// E-RSWIRE flip: an RS_INIT from RS now runs the real handshake —
    /// the rproctab grant is copied through `sys_safecopyfrom`, decoded,
    /// and in_use services get their ACLs. This test seeds the mock
    /// gateway with one genuine VFS rprocpub image and asserts the full
    /// path (C: sef_cb_init_fresh, main.c:237-260).
    #[test]
    fn test_run_once_rs_init_handshake_copies_and_maps_service() {
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

            // Register RS as a live caller (slot 2 ↔ RS_PROC_NR; run_once
            // drops messages from unregistered senders before the
            // handshake runs). C: RS is a boot-image process, so its slot
            // is active by construction (main.c:497-520).
            let rs_table = VmProcTable::get_global();
            let rs_slot = UserSlot::new(2);
            unsafe { rs_table.reset_slot(rs_slot); }
            let rs_empty = rs_table.get_empty(rs_slot).unwrap();
            let mut rs_proc = rs_empty.activate(Endpoint::RS);
            rs_proc.init_page_table().expect("rs pt");
            rs_proc.init_regions();

            // Recording gateway seeded with a real VFS entry image.
            let mock = alloc::rc::Rc::new(core::cell::RefCell::new(
                crate::kernel_gateway::MockGateway::new(),
            ));
            server.ctx.gateway = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(SharedMockGateway(alloc::rc::Rc::clone(&mock)))
                    as alloc::boxed::Box<dyn crate::kernel_gateway::KernelGateway>,
            ));
            // rs_handshake looks services up by endpoint — VFS's slot must
            // be active before the RS_INIT arrives (C: VFS is a boot-image
            // process, main.c:497-520).
            let vfs_slot = UserSlot::new(Endpoint::VFS.0 as usize);
            unsafe { rs_table.reset_slot(vfs_slot); }
            let vfs_empty = rs_table.get_empty(vfs_slot).unwrap();
            let mut vfs_pre = vfs_empty.activate(Endpoint::VFS);
            vfs_pre.init_page_table().expect("vfs pt");
            vfs_pre.init_regions();

            // 授权捞回的是 `NR_BOOT_PROCS` 行共享快照（A-4 行宽）。
            let entry_size = core::mem::size_of::<minix_types::RprocpubSnap>();
            let table_bytes = minix_types::NR_BOOT_PROCS * entry_size;
            *mock.borrow_mut().safecopy_payload.borrow_mut() =
                vfs_rprocpub_entry_image();

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

            // One whole-table safecopy from RS, grant 7, offset 0.
            let sc = mock.borrow().last_safecopy.get().expect("safecopy fired");
            assert_eq!(sc, (Endpoint::RS.0, 7, 0, table_bytes));

            // The decoded entry took effect: VFS's slot has an ACL.
            let table = VmProcTable::get_global();
            let vfs = table
                .get_active(
                    table
                        .vm_isokendpt(Endpoint::VFS)
                        .expect("VFS endpoint registered"),
                )
                .expect("VFS active");
            let acl = vfs.acl();
            assert!(
                matches!(acl, crate::acl::AclState::System(_)),
                "map_service equivalent gave VFS a System ACL, got {acl:?}"
            );

            // The in_use entry's 64-bit call mask survives the decode
            // (V13a: chunks beyond bit 32 are authorization, not noise).
            assert_eq!(vfs_rprocpub_call_mask(), 0x0000_0300_0000_00ff);

            // E-BIRTHFACE（NS1）：成功握手的同轮必须回出生报告
            //（process_init 尾部 sef_init.c:113-117）——RS boot step3
            // 等的就是它；SUSPEND 只压主循环的第二回复。
            let sent = handle.sent();
            assert_eq!(sent.len(), 1, "恰好一条出生应答");
            assert_eq!(sent[0].0, Endpoint::RS);
            assert_eq!(sent[0].1.m_type, RS_INIT as i32);
            assert_eq!(sent[0].1.rs_init_result(), Some(minix_types::OK));

            reset_boot_slots();
        });
    }

    /// E-RSWIRE fail-closed half: when the kernel rejects the safecopy
    /// (grant not mounted yet), the handshake errors — RS still gets the
    /// RS_INIT+result failure report (sef_init.c:110-119), then the message
    /// is dropped and counted ([A-14]: no second reply, no panic).
    #[test]
    fn test_run_once_rs_init_fails_closed_on_safecopy_error() {
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

            // Register RS as a live caller — without this the pre-dispatch
            // sender check drops the RS_INIT before the handshake ever runs
            // (the failure reply below must be reachable; main.c:497-520 —
            // RS is a boot-image process, its slot is active by construction).
            let rs_table = VmProcTable::get_global();
            let rs_slot = UserSlot::new(2);
            unsafe { rs_table.reset_slot(rs_slot); }
            let rs_empty = rs_table.get_empty(rs_slot).unwrap();
            let mut rs_proc = rs_empty.activate(Endpoint::RS);
            rs_proc.init_page_table().expect("rs pt");
            rs_proc.init_regions();

            // Mock with NO payload and a failing safecopy: swap in a
            // gateway whose safecopy errors — reuse SharedMockGateway over
            // a Mock with fork_reply poisoned? Simplest: a fresh mock whose
            // safecopy returns Err via the dedicated failure flag.
            struct FailingSafecopy;
            impl crate::kernel_gateway::KernelGateway for FailingSafecopy {
                fn sys_datacopy_from(
                    &mut self,
                    _: Endpoint,
                    _: u64,
                    _: &mut [u8],
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Err(crate::kernel_gateway::GatewayError::Kernel(-minix_types::EIO))
                }
                fn sys_safecopyfrom(
                    &mut self,
                    _granter: Endpoint,
                    _grant_id: i32,
                    _offset: u64,
                    _buf: &mut [u8],
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Err(crate::kernel_gateway::GatewayError::Kernel(-minix_types::EIO))
                }
                fn sys_fork(&mut self, _: Endpoint, _: UserSlot)
                    -> Result<(Endpoint, Option<u64>), crate::kernel_gateway::GatewayError>
                { Err(crate::kernel_gateway::GatewayError::Kernel(-minix_types::EIO)) }
                fn sys_exec(&mut self, _: Endpoint, _: u64, _: u64, _: u64, _: u64)
                    -> Result<(), crate::kernel_gateway::GatewayError>
                { Err(crate::kernel_gateway::GatewayError::Kernel(-minix_types::EIO)) }
                fn sys_update(&mut self, _: Endpoint, _: Endpoint, _: u32)
                    -> Result<(), crate::kernel_gateway::GatewayError>
                { Err(crate::kernel_gateway::GatewayError::Kernel(-minix_types::EIO)) }
                fn sys_kill(&mut self, _: Endpoint, _: i32)
                    -> Result<(), crate::kernel_gateway::GatewayError>
                { Err(crate::kernel_gateway::GatewayError::Kernel(-minix_types::EIO)) }
                fn sys_vmctl_memreq_get(&mut self)
                    -> Result<Option<crate::kernel_gateway::KernelMemReq>, crate::kernel_gateway::GatewayError>
                { Ok(None) }
                fn sys_vmctl_memreq_reply(&mut self, _: Endpoint, _: bool)
                    -> Result<(), crate::kernel_gateway::GatewayError>
                { Ok(()) }
                fn sys_vmctl_clear_pagefault(&mut self, _: Endpoint)
                    -> Result<(), crate::kernel_gateway::GatewayError>
                { Ok(()) }
                fn sys_vmctl_boot_inhibit_clear(&mut self, _: Endpoint)
                    -> Result<(), crate::kernel_gateway::GatewayError>
                { Ok(()) }
                fn sys_vmctl_set_addrspace(&mut self, _: Endpoint, _: u64, _: u64)
                    -> Result<(), crate::kernel_gateway::GatewayError>
                { Ok(()) }
                fn diag_write(&mut self, _text: &str)
                    -> Result<(), crate::kernel_gateway::GatewayError>
                { Ok(()) }
            }
            server.ctx.gateway = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(FailingSafecopy)
                    as alloc::boxed::Box<dyn crate::kernel_gateway::KernelGateway>,
            ));

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
            // NS2/E-RPROCTAB: the failure still replies RS_INIT+result
            // (process_init replies unconditionally — sef_init.c:110-119),
            // then the fail-closed drop stands ([A-14]: no second reply).
            let sent = handle.sent();
            assert_eq!(sent.len(), 1, "exactly the failure result reaches RS");
            assert_eq!(sent[0].0, Endpoint::RS);
            assert_eq!(sent[0].1.m_type, RS_INIT as i32);
            assert_eq!(
                sent[0].1.rs_init_result(),
                Some(minix_types::ESRCH),
                "safecopy failure maps to InvalidEndpoint → ESRCH (ipc/vm.rs)"
            );
            assert_eq!(server.dropped_messages(), 1, "failed handshake must be counted");

            reset_boot_slots();
        });
    }

    #[test]
    fn test_rproctab_shape_matches_c_table() {
        // C: `struct rprocpub rprocpub[NR_BOOT_PROCS]` (glo.h) — the table
        // the RS_INIT grant carries. The stub's 32-slot shape is gone; the
        // real table right-sizes to the same entry count the safecopy
        // copies (main.c:244-247), and the RprocEntry::EMPTY initializer
        // yields all-not-in-use rows.
        let tab = RprocTab::EMPTY;
        assert_eq!(minix_types::NR_BOOT_PROCS, 17);
        assert_eq!(tab.entries.len(), minix_types::NR_BOOT_PROCS);
        assert!(tab.entries.iter().all(|e| !e.in_use));
        assert_eq!(tab.entries.iter().filter(|e| e.endpoint != Endpoint::NONE).count(), 0);
    }

    #[test]
    fn test_rproctab_empty_entry_defaults() {
        let e = RprocEntry::EMPTY;
        assert!(!e.in_use);
        assert_eq!(e.endpoint, Endpoint::NONE);
        assert_eq!(e.call_mask, 0);
        assert!(!e.is_user);
    }

    /// Builds one genuine VFS rprocpub snapshot row (in_use, endpoint=VFS,
    /// 64-bit call_mask 0x300_0000_00ff) for the handshake test — the row
    /// kept is the shared `RprocpubSnap` layout (A-4).
    fn vfs_rprocpub_entry_image() -> alloc::vec::Vec<u8> {
        let mut label = [0u8; minix_types::RS_MAX_LABEL_LEN];
        label[..4].copy_from_slice(b"vfs\0");
        let row = minix_types::RprocpubSnap {
            in_use: 1,
            sys_flags: 0,
            endpoint: Endpoint::VFS.0,
            dev_nr: 0,
            label,
            vm_call_mask: 0x0000_0300_0000_00ff,
        };
        // SAFETY(test): 行是 repr(C) POD，取其字节视图拼进缓冲。
        unsafe {
            core::slice::from_raw_parts(
                (&row as *const minix_types::RprocpubSnap).cast::<u8>(),
                core::mem::size_of::<minix_types::RprocpubSnap>(),
            )
            .to_vec()
        }
    }

    fn vfs_rprocpub_call_mask() -> u64 {
        let img = vfs_rprocpub_entry_image();
        // SAFETY(test): 行字节位按同一结构读回。
        unsafe { core::ptr::read_unaligned(img.as_ptr().cast::<minix_types::RprocpubSnap>()) }
            .vm_call_mask
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
                // memtype 必须在位（C 的每个 VM region 都有类型；
                // handle_pagefault 经 memtype 分派，None 即 NoMemType）。
                proc.regions_mut()
                    .insert(crate::region::VirRegion::with_memtype(
                        VirBytes(0x3000_0000),
                        VirBytes(0x4000),
                        crate::region::VrFlags::WRITABLE | crate::region::VrFlags::ANON,
                        &crate::memtype::MEM_TYPE_ANON,
                    ))
                    .unwrap();
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
        fn sys_datacopy_from(
            &mut self,
            src: Endpoint,
            src_addr: u64,
            buf: &mut [u8],
        ) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().sys_datacopy_from(src, src_addr, buf)
        }
        fn sys_safecopyfrom(
            &mut self,
            granter: Endpoint,
            grant_id: i32,
            offset: u64,
            buf: &mut [u8],
        ) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0
                .borrow_mut()
                .sys_safecopyfrom(granter, grant_id, offset, buf)
        }
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
        fn sys_vmctl_boot_inhibit_clear(&mut self, endpoint: Endpoint)
            -> Result<(), crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_vmctl_boot_inhibit_clear(endpoint)
        }
        fn sys_vmctl_set_addrspace(&mut self, endpoint: Endpoint, ptroot_phys: u64, ptroot_virt: u64)
            -> Result<(), crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_vmctl_set_addrspace(endpoint, ptroot_phys, ptroot_virt)
        }
        fn diag_write(&mut self, text: &str) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().diag_write(text)
        }
    }
}