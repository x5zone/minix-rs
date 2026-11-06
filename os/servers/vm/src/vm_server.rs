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

use minix_types::AssumeSyncCell;

use crate::alloc_page::VmPageAllocator;
use crate::boot::{BootParams, KernelAllocated, VM_PROC_NR};
use crate::direct_map::vm_phys_to_virt;
use crate::ipc::dispatcher::MessageDispatcher;
use crate::ipc::encode::{VmReplyForIpc, encode_reply_data, reply_to_errno};
use crate::ipc::transport::IpcStatus;
use crate::page_cache::PageCache;
#[cfg(not(test))]
use crate::pagetable::vm_self_map::init_vm_self_pt;
#[cfg(all(feature = "buddy_alloc", not(feature = "segment_tree_alloc")))]
use crate::phys_mem::BUDDY_THRESHOLD_PAGES;
#[cfg(feature = "buddy_alloc")]
use crate::phys_mem::BuddyAllocator;
#[cfg(feature = "segment_tree_alloc")]
use crate::phys_mem::SegmentTreeAllocator;
use crate::phys_mem::{
    AlignedPhysBytes, BitmapAllocator, BootMemRegion, CLICK_SIZE, PhysAlloc, PhysAllocType,
    PhysAllocator, bytes_to_clicks,
};
use crate::region::PageFrames;
use crate::vfs_queue::VfsRequestQueue;
use crate::vmproc::VmProcTable;
use minix_types::PhysBytes;
#[cfg(test)]
use minix_types::VM_PROCCTL;
use minix_types::{
    BootImage, Endpoint, Message, NR_BOOT_PROCS, UserSlot, VM_RQ_BASE, VmError, VmPagefaultIn,
    VmProcctlIn, VmReply, reply_status,
};
#[cfg(test)]
use minix_types::{VmBrkIn, VmExitIn, VmForkIn};
// VmContext.user_sp + install_boot_stack use VirBytes unconditionally
// (E-BOOTFRAME); before that only test code needed it.
use minix_types::VirBytes;

/// The memory subsystem state the server drives: the physical-page
/// allocator, the frame table, the page cache, and the pending VFS request
/// queue. These four live and die with the server and are needed together
/// at every dispatch site.
///
/// Grouping them makes that invariant
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
    /// Kernel-call gateway: the single exit point for
    /// kernel syscalls (sys_fork today; safecopy/update/exec/diag join
    /// with their consumers). `Rc<RefCell<…>>` follows the transport
    /// pattern: tests keep a mock behind the same shape the
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
    /// Dispatches since the last refcount verification, counted only under
    /// the `sanity_checks` feature. C: the periodic SANITYCHECKS pass
    /// (alloc.c).
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
        > = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(
            crate::kernel_gateway::TrapKernelGateway {
                transport: minix_sys::syscall::DirectKernelCallTransport,
            },
        )));
        // Audit records route through this gateway (SYS_DIAGCTL).
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
            vm_self_bytes: self.vm_allocated_bytes.saturating_add(
                (self.page_alloc.self_page_count() as u64) * crate::region::page_state::PAGE_SIZE,
            ),
        }
    }
}

pub struct VmServer {
    pub(crate) ctx: VmContext,
    initialized: bool,
    /// Count of kernel pagefault messages whose handling failed.
    ///
    /// The main loop previously dropped the
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
    /// Kernel layout from the boot handoff: `None` = an older-than-v3
    /// handoff/test shape → `init_global_state` keeps the mock constants.
    kernel_layout: Option<minix_types::KernelLayout>,
    /// Kernel-information page physical address (handoff v7).
    /// Every new root maps it read-only at `minix_types::KERNINFO_USER_VA`.
    kerninfo_pa: minix_types::PhysBytes,
    /// Pages to charge to the global page total during init.
    ///
    /// C: `mem_add_total_pages()` call points (main.c:485-495).
    boot_extra_pages: usize,
    /// Real usable physical page count = Σ of the free-region page counts.
    ///
    /// This is C's accounting `total_pages` (`mem_init()` —
    /// `total_pages += chunks[i].c_pages`, alloc.c:319), the value behind
    /// `vsi_total` (utility.c:118). It is deliberately kept separate from
    /// `BootParams::total_pages`, which is the bitmap *capacity* (the
    /// highest absolute page index) and must span address space with a
    /// non-zero RAM base (aarch64 @ PA 0x40000000). Using the capacity for
    /// the accounting total would inflate `vsi_total` by every reserved hole
    /// below the highest free page (CodeReview P1).
    boot_usable_pages: usize,
    /// IPC transport for the main loop.
    ///
    /// `Rc<RefCell<...>>`: the previous process-global
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

/// The image-resident server slot ([ARCH: V20-P1-1]): C carries VM state in
/// the image's data segment (main.c's BSS globals survive the warm start),
/// so the Rust server lives at one address-stable static the cold boot
/// fills exactly once. VM is a single-threaded event loop (crate root),
/// which is the single-access guard this unsync cell needs. A warm start
/// in the same process finds the slot full and skips `init()`; a re-exec'd
/// process finds it empty — that shape has no C counterpart (the evidence
/// entry in 02-stage-vm/todo.md, case ③), and the boot router answers it
/// with the loud rejection.
static VM_SERVER: AssumeSyncCell<Option<VmServer>> = AssumeSyncCell::new(None);

/// Which boot path `main` takes, from the kernel flag and the image slot
/// ([ARCH: V20-P1-1]; the kernel owns the flag — the 2.4 evidence entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootRoute {
    /// Fresh boot: build the server, run `init()` once.
    ColdInit,
    /// Warm start with the image slot carrying the server: skip `init()`,
    /// enter the loop directly (case ①, in-process).
    WarmRun,
    /// Warm start with an empty slot: a re-exec'd process whose state is
    /// gone. C has no usable semantics here (case ③, evidence entry), so
    /// the boot answers with the loud rejection.
    WarmNoState,
    /// A cold boot while the slot is already full: `init()` must run
    /// exactly once, so this is a contract breach.
    DoubleInit,
}

/// Route the boot ([ARCH: V20-P1-1]). `is_first_time` is the kernel-owned
/// `RTS_BOOTINHIBIT` verdict delivered through the boot handoff; the slot
/// state is the image-resident half.
pub fn boot_route(is_first_time: bool) -> BootRoute {
    let installed = server_installed();
    match (is_first_time, installed) {
        (true, false) => BootRoute::ColdInit,
        (false, true) => BootRoute::WarmRun,
        (false, false) => BootRoute::WarmNoState,
        (true, true) => BootRoute::DoubleInit,
    }
}

/// Whether the image slot carries an initialized-or-building server.
pub fn server_installed() -> bool {
    // SAFETY: single-threaded VM (crate root); the slot is only touched
    // through this module's accessors.
    unsafe { (*VM_SERVER.get()).is_some() }
}

/// Cold boot: place the freshly built server into the image slot.
///
/// Installing twice breaches the "init exactly once" contract, so the
/// second server comes back as `Err` untouched.
pub fn install_server(server: VmServer) -> Result<(), VmServer> {
    // SAFETY: single-threaded VM (crate root); see [`VM_SERVER`].
    let slot = unsafe { &mut *VM_SERVER.get() };
    if slot.is_some() {
        return Err(server);
    }
    *slot = Some(server);
    Ok(())
}

/// Take the server out of the image slot (tests, and a future orderly
/// shutdown path).
pub fn take_server() -> Option<VmServer> {
    // SAFETY: single-threaded VM (crate root); see [`VM_SERVER`].
    let slot = unsafe { &mut *VM_SERVER.get() };
    slot.take()
}

/// Run `f` with the installed server. Panics when the slot is empty — the
/// caller routes through [`boot_route`] first, so an empty slot here is a
/// boot-order bug, not a runtime condition.
pub fn with_installed_server<R>(f: impl FnOnce(&mut VmServer) -> R) -> R {
    // SAFETY: single-threaded VM (crate root); see [`VM_SERVER`].
    let slot = unsafe { &mut *VM_SERVER.get() };
    let server = slot
        .as_mut()
        .expect("with_installed_server: the image slot is empty (cold boot must install first)");
    f(server)
}

/// Enter the scheduling loop on the installed server (`main`'s tail; both
/// boot routes converge here).
pub fn run_installed() -> ! {
    with_installed_server(|server| server.run())
}

impl VmServer {
    /// Test-facing constructor over [`BootParams::simple`].
    ///
    /// The production entry point, gated the same way, is
    /// [`Self::new_with_boot_params`] (the real boot contract); `new()` serves
    /// host tests only, behind a compile-time gate that prevents production from
    /// using mock-shaped boot parameters.
    #[cfg(test)]
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
    /// driven end-to-end on the host. The boot contract is the
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
        // The global registration used to happen HERE,
        // against the local `page_alloc` — which is then MOVED into
        // `VmContext` and out through the return value, leaving
        // PAGE_ALLOC_PTR dangling into this dead stack frame. Everything
        // reaching `page_alloc_mut()` afterwards (the `vm_pt_alloc` hook,
        // `refill_arena`, `alloc_oversize`) read a stale corpse: the first
        // boot procs "worked" off leftover bits (double-allocating pages
        // the post-relocate allocator still counted free), and `rs` died
        // with `AllocationFailed free=0 … self_pages=0` once the frame was
        // recycled (seen in the boot serial log). The
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
        // The paired release hook. Only after registration does `destroy()`
        // activate free_child_tables and root-page reclaim (without it the
        // behavior degrades to clearing the root alone, and intermediate page
        // table pages leak on process exit — the Rust counterpart of C pt_free
        // pagetable.c:1427-1437 closes here).
        if !minix_arch::pt_alloc::is_free_registered() {
            minix_arch::pt_alloc::register_free(crate::alloc_page::vm_pt_free);
        }
        // The kernel-side read bypass: walk_read's VmDm slot reads go through a
        // kernel call so the kernel's KDM reads them directly (a riscv-only
        // production mechanism, placed with vmdm_bridge when the T8 probe was
        // removed). It is timed with pt_alloc::register — before any Paging
        // walk.
        #[cfg(all(target_arch = "riscv64", not(test)))]
        crate::vmdm_bridge::register_vmdm_read();

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
            // C accounting total (Σ usable pages) — distinct from the
            // bitmap capacity in `params.total_pages` (see field doc).
            boot_usable_pages: params
                .free_regions
                .iter()
                .map(|r| r.size / CLICK_SIZE)
                .sum(),
            kernel_layout,
            kerninfo_pa: params.kerninfo_pa,
            transport,
        }
    }

    fn create_default_allocator(total_pages: usize, free_regions: &[BootMemRegion]) -> PhysAlloc {
        // [ARCH: A-5] — bootstrap backend is always the bitmap; the
        // strategy switch (bitmap/buddy/segment-tree) happens later in
        // relocate() (see plan.md §4 A-5, 05-physical-memory.md §3.3).
        let meta_size = BitmapAllocator::metadata_size(total_pages);
        let meta_pages = bytes_to_clicks(meta_size);

        let meta_region = free_regions
            .iter()
            .find(|r| {
                (r.base as u64) < crate::direct_map::VM_DIRECT_MAP_SIZE
                    && r.size >= meta_pages * CLICK_SIZE
            })
            .expect("no free region in Direct Map range large enough for allocator metadata");

        let meta_phys_base = meta_region.base;
        // One path for both profiles — `vm_phys_to_virt` is
        // window-aware in test builds too (per-thread window base), so the
        // former cfg split (arch global read vs funnel) is unnecessary.
        let meta_va = vm_phys_to_virt(AlignedPhysBytes::new(meta_phys_base as u64));
        // SAFETY: meta_va points to a valid direct-mapped physical region
        // of meta_size bytes; no aliasing references exist.
        let metadata = unsafe { core::slice::from_raw_parts_mut(meta_va.0 as *mut u8, meta_size) };

        let adjusted_base = meta_phys_base + meta_pages * CLICK_SIZE;
        let adjusted_size = meta_region.size.saturating_sub(meta_pages * CLICK_SIZE);
        // The metadata donor is carved IN PLACE; every other survivor region
        // must reach the allocator too (C: `mem_init` seeds the hole list from
        // ALL chunks, alloc.c:306-331). Passing only the donor — what this
        // line used to do — left the PMM with the donor's remainder only
        // (~133 pages on first light, so the `PageFrames` oversize grow hit
        // PhysicalAllocFailed; the vm_handoff
        // serial landmark showing `free n=10` vs a 24-page pool).
        let mut adjusted_regions = alloc::vec::Vec::with_capacity(free_regions.len());
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

        PhysAlloc::Bitmap(BitmapAllocator::init(
            metadata,
            total_pages,
            &adjusted_regions,
            meta_phys_base as u64,
            meta_pages,
        ))
    }

    /// Select the boot-time allocator backend ([ARCH: A-5]).
    ///
    /// This function is the **single precedence authority** (the contradictory
    /// `DefaultAllocator` alias in `phys_mem/mod.rs` was removed):
    ///
    /// - `segment_tree_alloc` selects the segment-tree backend outright,
    ///   including when the `buddy_alloc` feature is also enabled (that wiring
    ///   selected the previously-never-selected backend, and the
    ///   combined-feature semantics are pinned by tests).
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
            let bitmap = phys_alloc
                .as_bitmap()
                .expect("relocate: bootstrap allocator must be Bitmap");
            let (pa_base, pa_pages) = bitmap.metadata_pa_range();
            assert!(
                pa_pages > 0,
                "relocate: no BumpBuf metadata to relocate (already relocated?)"
            );
            (bitmap.total_count(), pa_base, pa_pages)
        };

        let alloc_type = Self::choose_allocator_type(total_pages);

        let meta_size = alloc_type.metadata_size(total_pages);
        let pages = bytes_to_clicks(meta_size);
        let new_va = crate::global::heap_arena_grow(pages, &mut self.ctx.page_alloc)
            .expect("relocate: failed to allocate new metadata via HeapArena");
        // SAFETY: new_va points to a valid heap-arena region of meta_size bytes;
        // no aliasing references exist.
        let new_metadata = unsafe { core::slice::from_raw_parts_mut(new_va as *mut u8, meta_size) };

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
        // Under the current `choose_allocator_type` the fallback
        // arms are UNREACHABLE — it only names a backend whose feature is
        // on, so each `#[cfg(not(...))]` fallback never runs in any feature
        // combination. They keep the match exhaustive without making
        // `PhysAllocType` cfg-dependent; do not read them as live paths.
        let new_alloc = match alloc_type {
            PhysAllocType::Bitmap => PhysAlloc::Bitmap(BitmapAllocator::init(
                new_metadata,
                total_pages,
                &free_regions,
                0,
                0,
            )),
            PhysAllocType::Buddy => {
                #[cfg(feature = "buddy_alloc")]
                {
                    PhysAlloc::Buddy(BuddyAllocator::init(
                        new_metadata,
                        total_pages,
                        &free_regions,
                    ))
                }
                #[cfg(not(feature = "buddy_alloc"))]
                {
                    PhysAlloc::Bitmap(BitmapAllocator::init(
                        new_metadata,
                        total_pages,
                        &free_regions,
                        0,
                        0,
                    ))
                }
            }
            PhysAllocType::SegmentTree => {
                #[cfg(feature = "segment_tree_alloc")]
                {
                    PhysAlloc::SegmentTree(SegmentTreeAllocator::init(
                        new_metadata,
                        total_pages,
                        &free_regions,
                    ))
                }
                #[cfg(not(feature = "segment_tree_alloc"))]
                {
                    PhysAlloc::Bitmap(BitmapAllocator::init(
                        new_metadata,
                        total_pages,
                        &free_regions,
                        0,
                        0,
                    ))
                }
            }
        };

        // The relocation's backend decision is otherwise
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
        // Register the global PMM pointer against the FINAL address
        // of the allocator. `self` here is the server as stored in the
        // image slot ([ARCH: V20-P1-1]) — an address-stable static, so
        // PAGE_ALLOC_PTR (read by the `vm_pt_alloc` hook, `refill_arena`,
        // `alloc_oversize`) stays valid for the server's whole lifetime.
        // Registering earlier (in `new_inner`, against a pre-move local)
        // dangled the pointer into a dead stack frame — see the comment
        // there. `relocate()` below replaces the allocator's `phys_alloc`
        // field IN PLACE, so this registration survives it.
        crate::global::register_page_alloc(&mut self.ctx.page_alloc);

        // C: init_vm() — main.c:428-584. Step order mirrors C exactly:
        //
        //   main.c:442      sys_getkinfo; main.c:451-452 asserts → BootParams::validate() (new_with_boot_params)
        //   main.c:455      get_mem_chunks; main.c:471 mem_init → VmServer::new (allocator construction)
        //   main.c:457-462  memset(vmproc) + vm_slot    → compile-time vacant slots + get_empty()
        //   main.c:465      acl_init()                  → AclState::Uninitialized (compile-time default)
        //   main.c:468      map_region_init()           → RegionMap::new() lazily per process
        //   main.c:474-475  init_proc(VM_PROC_NR)+pt_init → init_vm_slot() + init_vm_self_pt() (new)
        //   main.c:480      __minix_init()              → mark_initialized() (real trap wiring → E1)
        //   main.c:485-495  mem_add_total_pages()       → account_boot_memory()
        //   main.c:497-520  boot procs (exec_bootproc)  → init_boot_procs() + exec_bootproc (stack frame → E-BOOTFRAME)
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

        // PageFrames after total_pages is known, BEFORE the boot-proc loop:
        // C orders `mem_init()` (main.c:471 — the all-physical bookkeeping
        // this mirrors) ahead of the `exec_bootproc` pass (main.c:498-520),
        // and exec_bootproc consumes page_frames for segment pages. The
        // former placement after init_boot_procs() was an unreachable-order
        // latent bug that forensics exposed ("page_frames not
        // initialized" panic from exec_bootproc, 2026-09-21).
        let total_phys =
            PhysBytes(self.ctx.page_alloc.total_pages() as u64 * crate::region::PAGE_SIZE);
        self.ctx.page_frames = Some(PageFrames::new(total_phys));

        // The reclaim-retry funnel needs kernel-visible access to
        // the cache/frames pair (C alloc_mem reads the global cache). Same
        // registration lifetime as the audit gateway and page allocator.
        if let Some(frames) = self.ctx.page_frames.as_mut() {
            crate::global::register_reclaim(&mut self.ctx.page_cache, frames);
        }

        // Phase 2c: boot process slots — main.c:497-520 (exec_bootproc
        // landed; the initial stack frame ABI waits on edge
        // E-BOOTFRAME).
        self.init_boot_procs();

        // Phase 2d: VM instance mark — main.c:577-579.
        self.mark_vm_instance();

        // C: __minix_init() (main.c:480) — SEF startup makes the IPC
        // channel ready before the main loop. Without this, a
        // `KernelIpcTransport` stays uninitialized and `run()` would
        // busy-loop on `Err(Unimplemented)` instead of blocking on
        // `sef_receive_status`.
        self.transport.borrow_mut().mark_initialized();

        self.initialized = true;
    }

    fn init_global_state(&mut self) {
        // SAFETY: init() must be called exactly once during VM startup.
        // Seed the global total with C's accounting figure (Σ usable pages,
        // `boot_usable_pages`), NOT the bitmap capacity
        // (`page_alloc.total_pages()`). The latter spans address space to
        // cover absolute page indices and would overstate `vsi_total` by
        // every reserved hole below the top free page. `account_boot_memory`
        // then adds the boot-module extras (C `mem_add_total_pages`).
        unsafe {
            crate::global::init(self.boot_usable_pages);
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
        // Open item (boot-info integration): replace these mock values with real
        // values parsed from the multiboot2 / stivale2 boot headers or linker
        // symbols. The kernel text physical base, text/data sizes, and direct
        // map size all come from boot-time information. Until then, these mock
        // values match the previous hardcoded constants so behavior is
        // unchanged, but the mechanism is now in place to supply correct
        // values without touching `init_page_table()`.
        //
        // A version ≥ 3 handoff carries the real kernel layout —
        // consume it.
        //
        // A6 (C17 remainder, external-review ruling): a missing
        // kernel_layout means an incomplete input (a real handoff is v5
        // and always carries it) — production rejects it uniformly on all
        // three architectures. There is no silent fallback-to-mock
        // production path anymore (the old shape: riscv panicked while
        // x86/aarch64 silently adopted the x86 mock constants). The mock
        // layout is injected explicitly by the test fixture:
        // `BootParams::simple` carries a mock `KernelLayout` (fixture
        // semantics — using the simple constructor means "I want the mock
        // layout"), so host tests are unaffected; the reject-on-None
        // behavior is pinned by
        // `test_init_rejects_missing_kernel_layout`.
        let layout = self.kernel_layout.expect(
            "handoff lacks kernel layout (version < 5) — input rejected (A6): \
             a real handoff is v5 and carries the kernel layout",
        );
        // SAFETY: Called before any `init_page_table()` call (process table
        // setup happens in `init_proc_table()` after this). Single-threaded
        // VM ensures no concurrent reader of `KERNEL_LAYOUT`.
        unsafe {
            crate::global::set_kernel_layout(layout);
        }

        // The kernel-information page's physical address. Every new
        // root maps it read-only at `minix_types::KERNINFO_USER_VA`; the
        // page itself stays kernel-owned.
        // SAFETY: same single-threaded, pre-page-table window as above.
        unsafe {
            crate::global::set_kerninfo_pa(self.kerninfo_pa.0);
        }
    }

    /// init_proc(VM_PROC_NR) — main.c:262-283, called at main.c:474.
    fn init_vm_slot(&self) {
        let table = self.ctx.proc_table; // the ctx field is the global alias; one spelling
        if let Some(ip) = self.boot_procs.iter().find(|ip| ip.proc_nr == VM_PROC_NR) {
            Self::init_proc(table, *ip);
        }
    }

    /// Boot process slots — main.c:497-520.
    ///
    /// C also runs `exec_bootproc()` + `free_mem()` per boot process here;
    /// both are implemented (ELF segment loading through the
    /// Direct Map + `Gateway::sys_exec`, blob freed back to the allocator).
    /// The minimal initial stack frame is edge E-BOOTFRAME (VM↔libc↔kernel
    /// shared ABI), so `stack`/`ps_str` report 0 until it lands; real
    /// kernel traffic for `sys_exec` waits on edge E2.
    fn init_boot_procs(&mut self) {
        let table = self.ctx.proc_table; // one spelling (the same object)
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
                self.ctx
                    .page_alloc
                    .free_pages(crate::phys_mem::AlignedPhysBytes::new(ip.start_addr), pages);
            }
        }
    }

    /// `exec_bootproc` (C: main.c:331-426) — load a boot image
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

        let table = self.ctx.proc_table; // one spelling (the same object)
        let slot = table
            .vm_isokendpt(ip.endpoint)
            .map_err(|_| "boot proc endpoint not registered")?;

        // C: sys_physcopy(NONE, ip->start_addr, SELF, hdr, ...) — the image
        // is physical memory owned by the boot handoff; the Direct Map
        // window makes it directly readable (no copy needed, unlike C).
        let image_len = ip.len as usize;
        let image_va = crate::direct_map::vm_phys_to_virt(crate::phys_mem::AlignedPhysBytes::new(
            ip.start_addr,
        ));
        // SAFETY: [image_va, image_va+image_len) is the boot module blob the
        // kernel handed over; VM owns it exclusively (C frees it right after
        // exec_bootproc, main.c:514-516). The Direct Map window covers all
        // physical memory; the handoff guarantees page alignment.
        let image: &[u8] =
            unsafe { core::slice::from_raw_parts(image_va.0 as *const u8, image_len) };

        // Parse + walk PT_LOAD segments (C: libexec_load_elf → elf_exec_hdr).
        let entry = minix_elf::entry_point(image).map_err(|_| "boot image is not a valid ELF")?;
        // SegmentIter yields PT_LOAD segments only (p_type filter inside).
        let segments: alloc::vec::Vec<minix_elf::LoadSegment> = minix_elf::segment_iter(image)
            .map_err(|_| "boot image phdrs unreadable")?
            .collect();

        let frames = self
            .ctx
            .page_frames
            .as_mut()
            .ok_or("page_frames not initialized")?;

        let mut proc = table.get_active(slot).ok_or("boot proc slot not active")?;

        // C: pt_bind(&vmp->vm_pt, vmp) — main.c:355, whose substance is
        // `sys_vmctl_set_addrspace(endpoint, pt_dir_phys, pdes)`
        // (pagetable.c:1421). The kernel stores the root and clears
        // RTS_VMINHIBIT (C setcr3, arch_do_vmctl.c:19-33) — this is the
        // ONLY boot-path VMINHIBIT clear leg (the kernel sets
        // VMINHIBIT|BOOTINHIBIT on every non-VM boot proc,
        // kernel/src/lib.rs:1666); without it the exec'd processes stay
        // parked even after BOOTINHIBIT lifts (confirmed on the real machine).
        // `pdes` (the kernel-visible PDE alias) has no meaning under the
        // Direct Map — 0 travels as `virt_root = None` (documented
        // deviation, exit.rs `handle_procctl_clear` Step-4 note).
        let ptroot_phys = <crate::pagetable::PageTable as crate::pagetable::Paging>::root_paddr(
            proc.page_table_mut(),
        )
        .0;
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
            // Round the segment base down to a page boundary. RS's entry
            // segment has vaddr=0x2246c0, which is not page-aligned — the old
            // implementation laid pages from the unaligned segment base as the
            // region origin while hardware PTEs install on 4KiB boundaries, so
            // page contents shifted wholesale by (seg.vaddr & 0xfff) and RS's
            // first instruction faulted with #GP(0) on a displaced byte.
            // C libexec correspondence: under the ELF convention
            // p_offset ≡ p_vaddr (mod page), the file source for an aligned
            // page is seg.offset - (seg.vaddr & 0xfff).
            let va_base = seg.vaddr & !(PS as u64 - 1);
            let va_end = seg.vaddr + seg_len;
            let pages = (va_end - va_base).div_ceil(PS as u64) as usize;
            // The root cause of the boot OOM: the region covers the whole
            // memsz (including the heap/BSS reserve tail — the last elf segment
            // reads FileSiz=64K / MemSiz=4M), but only file pages need eager
            // materialization; the page-aligned tail between memsz and filesz
            // (uninitialized .bss plus heap) is left to demand paging (C libexec
            // correspondence: exec_image physcopies only file pages and leaves
            // the rest demand-faulted). The old implementation ran alloc_pfn +
            // map_page over all of 0..pages, so twelve boot servers eagerly laid
            // out ~4MB each → 12×1041 pages exhausted the 14319-frame pool at
            // the eighth (mib) with "boot segment page allocation failed" (exec
            // got only to 7). The in-page .bss holes of the first and last pages
            // are explicitly zeroed by the write_bytes below (matching C
            // clearmem head/tail); zero fill of a page-aligned whole-page tail
            // relies on ANON demand-fault handing out a zero page — during boot
            // there is no page cache to reclaim and alloc_pfn_reclaiming
            // degrades to fresh frames, so behavior is unchanged. Wiring
            // demand-fault explicitly to PAF_CLEAR (matching C VR_UNINITIALIZED)
            // is recorded as a later hardening item.
            let file_pages = if seg.filesz == 0 {
                0
            } else {
                ((seg.vaddr + seg.filesz - va_base) as usize)
                    .div_ceil(PS)
                    // CodeReview SHOULD: minix-elf's SegmentIter does not check
                    // filesz<=memsz, so a malformed last segment would push
                    // file_pages past the region length `pages` and make
                    // map_page's bare index go out of bounds (a panic here means
                    // VM cannot start and the whole system stalls). Clamping to
                    // `pages` keeps the old tolerant semantics of silently
                    // dropping excess file bytes.
                    .min(pages)
            };
            let vaddr = minix_types::VirBytes(va_base);

            // Region per segment (C: libexec_alloc_vm_prealloc → map_page_region).
            // The writable flag comes from the ELF segment flags (PF_W=2): C's
            // map_page_region builds a writable region for a PT_LOAD RW segment.
            // Previously every segment got only ANON — the data segment's region
            // was not writable → is_page_writable was always false →
            // sync_slot_pte mapped .data pages read-only, and the first user
            // write faulted (err=7) → VM judged a violation → SIGSEGV (on the
            // real machine: RS's ensure_global_allocator allocator-flag xchg at
            // 0x229708 failed twice in a row).
            //
            // Executability must likewise be inherited from the ELF segment
            // flags (PF_X=1) — C on i386 has no NX, so boot text is executable by
            // nature; under x86-64 NXE, if the boot segment's region does not set
            // EXECUTABLE, then sync_slot_pte (cow_exec_pf.rs:129-135) groups the
            // text pages read_only without EXECUTABLE → the PTE is present but
            // NX → the boot server's instruction fetch faults forever and
            // livelocks (on the real machine: RS entry text at 0x227980 was hit
            // 71287 times). That is the same present-but-NX defect as in the
            // dynamic path, moved to the boot leg; only the
            // exec_worker→to_vr_flags leg had been fixed. A data segment without
            // PF_X stays NX (correct W^X), and a text segment with PF_X becomes
            // EXECUTABLE, equivalent to C.
            let mut seg_flags = crate::region::VrFlags::ANON;
            if seg.flags & minix_elf::PF_W != 0 {
                seg_flags |= crate::region::VrFlags::WRITABLE;
            }
            if seg.flags & minix_elf::PF_X != 0 {
                seg_flags |= crate::region::VrFlags::EXECUTABLE;
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
            // C: boot_alloc → map_page_region (main.c:308-317) always runs
            // region_find_slot_range, whose common leg records
            // `vm_region_top = startv + length` (region.c:391). Mirror it per
            // segment; segments are walked in ascending phdr order, so after
            // the loop the hint sits at the data end (C:402-415 slot-packing
            // semantics — a hint, not a break value).
            proc.set_region_top(VirBytes(vaddr.0 + (pages * PS) as u64));

            // Materialize pages eagerly and copy segment bytes through the
            // Direct Map (C: libexec_copy_physcopy per page).
            let pfn_alloc = &mut self.ctx.page_alloc;
            let seg_vr = proc.regions_mut().find_mut(vaddr).unwrap();
            for i in 0..file_pages {
                let offset = minix_types::VirBytes((i * PS) as u64);
                let pfn = pfn_alloc
                    .alloc_pfn()
                    .map_err(|_| "boot segment page allocation failed")?;
                seg_vr.map_page(frames, offset, pfn, &crate::memtype::MEM_TYPE_ANON);

                // Copy file bytes into the fresh physical page. Page i covers
                // [va_base+i*PS, +PS), and only its intersection with
                // [seg.vaddr, seg.vaddr+filesz) is valid: under the ELF
                // convention the file source is seg.offset + (in-page start -
                // seg.vaddr), with the tail of an unaligned head zero-filled.
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
                // CodeReview MUST-1(b): zero the in-page .bss holes (matching C
                // libexec clearmem head/tail). The prefix [page_va,lo) and
                // suffix [hi,page_va+PS) that the file does not cover must be
                // written as zeros explicitly — the page is already present via
                // map_page, so after the in-page .bss of the first/last pages a
                // fault is **never** raised again, and without this clearing it
                // would read the previous physical frame's stale data. For a
                // middle page lo==page_va && hi==page_va+PS gives
                // head_len=0/tail_len=0, a no-op, so one path needs no branch.
                {
                    let base_ptr = dst_va.0 as *mut u8;
                    let head_len = (lo - page_va) as usize;
                    let tail_start = (hi - page_va) as usize;
                    // SAFETY: inside the Direct Map window [0, PS) of the same
                    // freshly allocated page, head_len <= tail_start <= PS, so
                    // the two ranges neither overlap nor cross the page
                    // boundary.
                    unsafe {
                        core::ptr::write_bytes(base_ptr, 0, head_len);
                        core::ptr::write_bytes(base_ptr.add(tail_start), 0, PS - tail_start);
                    }
                }

                // SD-20: the page's bytes are final; if this segment is
                // executable, the CPU will fetch them as instructions — the
                // arch sequence must run before the image ever runs
                // (aarch64/riscv64; x86_64 and host builds compile the call
                // out). One kernel call per executable page, boot-time only.
                if seg.flags & minix_elf::PF_X != 0 {
                    crate::icache_bridge::sync_written_page(dst_phys);
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
        // pf find_mut → InvalidAddress → SIGSEGV (real machine
        // c11a: RS main's stack read at 0x7fffffffd9c8, one page below the
        // frame page, 2026-09-22).
        const DEFAULT_STACK_LIMIT: u64 = 4 * 1024 * 1024;
        // Underflow guard: a bare subtraction with user_sp below the limit
        // would wrap and push the region base to the top of the address space.
        // Production user_sp varies by architecture (0x7ffffffff000 on x86-64,
        // the 0x3fe0000000 family on riscv64; the authority is
        // `minix_types::USER_STACK_TOP`), but all are far above the limit; the
        // mock user_sp in host tests must satisfy the same invariant.
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
        let mut proc = table.get_active(slot).ok_or("boot proc slot not active")?;
        proc.regions_mut()
            .insert(region)
            .map_err(|_| "boot stack region overlap")?;
        // C: libexec's stack leg is `allocmem_ondemand(stacklow, size)`
        // (exec_elf.c:306-309 → boot_alloc → map_page_region), so the hint
        // leg (region.c:391) fires here too — hint = stacklow + length =
        // user_sp (stack_high is page-aligned on both sides), exactly as C's
        // insert order lands.
        proc.set_region_top(self.ctx.user_sp);

        // Materialize the stack page (C: the VR_UNINITIALIZED half of
        // boot_alloc/handle_memory_start) — the pfn stays in hand for the
        // frame-byte write below.
        let frames = self
            .ctx
            .page_frames
            .as_mut()
            .ok_or("page_frames not initialized")?;
        let pfn = self
            .ctx
            .page_alloc
            .alloc_pfn()
            .map_err(|_| "boot stack page allocation failed")?;
        {
            // The frame page's offset inside the region: the region base is
            // the bottom of the 4MiB window, and the frame page is the page
            // holding vsp (not the region's first page).
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
        let dst_va =
            crate::direct_map::vm_phys_to_virt(crate::phys_mem::AlignedPhysBytes::new(dst_phys));
        // The in-page offset (the page base holding vsp is vsp & !PS-1); the
        // region base is the bottom of the 4MiB window, and using it would
        // write the frame outside the page in physical memory (demonstrated by
        // host tests).
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
        // Name the rejecting errno in the message (the kernel
        // answers negative errno; EINVAL = dead endpoint, EFAULT = the
        // name-copy arm — C do_exec.c:37-40 treats that one as NON-fatal
        // "<unset>", so an EFAULT here pinpoints the deviation).
        let mut gateway = self.ctx.gateway.borrow_mut();
        gateway
            .sys_exec(endpoint, entry, vsp, 0, filled.ps_str)
            .map_err(|e| match e {
                crate::kernel_gateway::GatewayError::Kernel(c) if c == -minix_types::EINVAL => {
                    "sys_exec rejected by kernel: EINVAL"
                }
                crate::kernel_gateway::GatewayError::Kernel(c) if c == -minix_types::EFAULT => {
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
                crate::kernel_gateway::GatewayError::Kernel(c) if c == -minix_types::EIO => {
                    "sys_exec rejected by kernel: EIO(stub)"
                }
                _ => "sys_exec rejected by kernel",
            })?;
        // C: main.c:414-416 — "make it runnable": the boot process was
        // forked with VMINHIBIT|BOOTINHIBIT (kernel/src/lib.rs:1666) and
        // stays parked after exec unless VM clears the inhibit. Without
        // this leg every `exec X ok` booted a permanently stopped
        // process (forensics on the real machine: 11 boot procs
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

    /// Returns the count of failed kernel pagefault handlings.
    ///
    /// Every `VmReply::Error` produced while dispatching a `VM_PAGEFAULT`
    /// message increments this counter. Tests assert on it; the audit
    /// channel (`vm_acl_audit` feature) prints each failure.
    pub fn pagefault_errors(&self) -> u64 {
        self.ctx.pagefault_errors
    }

    /// Returns the count of IPC messages dropped at the main-loop boundary
    /// ([ARCH: A-14]): receive failures + invalid callers.
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
    /// `alloc_page::alloc_pfn_reclaiming`. The counter is
    /// cleared here so the next failure re-arms the hook — the loop always
    /// gets a fresh replenishment attempt per pressure episode.
    /// Kernel-signal dispatch — the body of C's `sef_cb_signal_handler`
    /// (main.c:731-749).
    ///
    /// The C arm recognizes exactly one number: `switch (signo) { case SIGKMEM: do_memory(); }`
    /// (`minix3/minix/servers/vm/main.c:733-739`; SIGKMEM=71,
    /// `minix3/sys/sys/signal.h:271`, with the single authority for the number
    /// living in `minix-types`). Every other number is ignored. In this tree the
    /// SYSTEM notification is the wakeup half of C's `send_sig(VM, SIGKMEM)`
    /// (the sigset half is a no-op because of the kernel `SigSet` width; see
    /// the `os/kernel/src/proc_table.rs:vm_enqueue_and_notify_vm` contract), so
    /// the number that arrives here is first translated by
    /// [`VmServer::effective_kernel_signal`].
    ///
    /// C tail (main.c:744-748): after handling, a pending spare-page
    /// deficit triggers `alloc_cycle()`; `pt_clearmapcache()` has no
    /// counterpart (map cache eliminated, [ARCH: A-1]).
    pub(crate) fn handle_signal(&mut self, signo: i32) {
        // C: "Check for known kernel signals, ignore anything else."
        if signo == minix_types::SIGNAL_KERNEL_MEMORY {
            self.do_memory();
        }
        // C's tail here also ran `alloc_cycle()` on a pending
        // `missing_spares` deficit (main.c:118-119) — that chain is deleted
        // (no producer; the spare-pool mechanism it served is structurally
        // eliminated by the Direct Map, [ARCH: A-1], and allocation-time
        // reclaim lives in `alloc_pfn_reclaiming`).
    }

    /// Turns the number `sef_receive_status` hands to the callback into "which
    /// kernel signal this should be handled as".
    ///
    /// Both shapes must be recognized: C's notify arm expands bits and passes
    /// the real signal number (71..=74), while this tree's kernel `SigSet` is
    /// still 64 bits wide and can never emit those four, so an empty bitmap
    /// takes `minix-sef`'s wakeup fallback and passes
    /// `SEF_SIGNAL_REQUEST_TYPE` (see "The wake-up fallback" section of
    /// `os/libs/minix-sef/src/lib.rs`). The fallback value says only "something
    /// happened": a `SYSTEM` notification VM can receive can only come from
    /// `vm_enqueue_and_notify_vm` (that is, C's `send_sig(VM, SIGKMEM)`), so it
    /// is handled as SIGKMEM; a real signal number is always dispatched by that
    /// number (treating SIGKSIG=74 as SIGKMEM is exactly the step that today's
    /// shape would get wrong once the bit width widens).
    pub(crate) fn effective_kernel_signal(arrived: Option<i32>) -> Option<i32> {
        match arrived {
            // The wakeup fallback: no number is available, so handle it by
            // its only possible source.
            Some(minix_sef::SEF_SIGNAL_REQUEST_TYPE) => Some(minix_types::SIGNAL_KERNEL_MEMORY),
            // A real signal number (or nothing delivered by the library):
            // hand it through unchanged.
            other => other,
        }
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
            // The reply's WHO must be the **requestor** (the process that was
            // suspended and holds the VMREQUEST context), not the space the
            // faulting page belongs to. C: pagefaults.c:206 —
            // `sys_vmctl(state->requestor, VMCTL_MEMREQ_REPLY, result)`; the
            // kernel's do_vmctl.c MEMREQ_REPLY arm reads the suspended context
            // by WHO's slot and clears RTS_VMREQUEST. The earlier code replied
            // to req.target by mistake: whenever target != requestor (a
            // cross-process copy suspension, e.g. PM writing a reply into INIT's
            // message buffer) the reply landed on a slot with no suspended
            // context → EINVAL → this loop returned and abandoned the drain →
            // the real requestor stayed stuck in VMREQUEST = a system-wide
            // deadlock (the terminal state on the real machine).
            if let Err(e) = self
                .ctx
                .gateway
                .borrow_mut()
                .sys_vmctl_memreq_reply(req.requestor, ok)
            {
                let _ = &e; // audit_log! compiles args away without features
                audit_log!("[VM SIGKMEM] memreq_reply failed: {e:?}");
                return;
            }
            let _ = serviced;
        }
        audit_log!(
            "[VM SIGKMEM] drain bound {} reached — kernel re-signals",
            MAX_MEMREQ_BATCH
        );
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
        // target == VM itself: VM's own slot is not an active row of this table
        // (VM runs in its own address space and never registers an active view
        // in VmProcTable), so get_active always returns None → false →
        // kernel_call_resume Fault → SIGSEGV against itself (the whole family).
        // The self-fill path: demand-fill VM's own page (BSS/IPC buffers)
        // through vm_self_map, and the kernel's retried copy succeeds. C
        // correspondence: VM treats its own pages through map_pf as well (the
        // vmp in pagefaults.c is VM).
        if req.target == minix_types::Endpoint::VM {
            let page_mask = crate::region::PAGE_SIZE as u64 - 1;
            let mut va = req.start & !page_mask;
            let end = req.start.saturating_add(req.length);
            while va < end {
                let v = minix_types::VirBytes(va);
                let present = crate::pagetable::vm_self_map::vm_self_query(v);
                match present {
                    // Already mapped: a self page has no CoW case, so RW is
                    // enough (BSS and heap pages are always RW).
                    Some(_) => {}
                    None => {
                        match self
                            .ctx
                            .page_alloc
                            .alloc_phys(1, crate::phys_mem::PageAllocFlags::empty())
                        {
                            Ok(phys) => {
                                if crate::pagetable::vm_self_map::vm_self_mappages(
                                    v,
                                    minix_types::PhysBytes(phys.as_u64()),
                                    crate::pagetable::PageFlags::WRITABLE
                                        | crate::pagetable::PageFlags::USER_ACCESSIBLE
                                        | crate::pagetable::PageFlags::PRESENT,
                                )
                                .is_err()
                                {
                                    return false;
                                }
                            }
                            Err(_) => return false,
                        }
                    }
                }
                va += crate::region::PAGE_SIZE as u64;
            }
            return true;
        }
        let table = VmProcTable::get_global();
        let slot = match table.vm_isokendpt(req.target) {
            Ok(s) => s,
            Err(_) => {
                audit_log!("[VM SIGKMEM] bad target endpoint {}", req.target.0);
                return false;
            }
        };

        let VmContext {
            page_alloc,
            page_frames,
            page_cache,
            vfs_queue,
            ..
        } = &mut self.ctx;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        let mut proc = match table.get_active(slot) {
            Some(p) => p,
            None => return false,
        };
        // C: VMPTYPE_CHECK's handle_memory_start (pagefaults.c:311-330) calls
        // map_handle_memory per page — a read fault (wrflag=false) likewise
        // guarantees the page is mapped into the target page table, and wrflag
        // only decides whether CoW is broken. The earlier read path through
        // handle_memory_once merely confirmed the region existed and returned
        // Ok without writing any PTE — the kernel re-issued
        // data_copy_vmcheck and still walked to NP, giving a
        // suspend/service/re-issue loop (on the real machine: after memreq ok=1
        // the DIAGCTL re-issue still reported susp-again). Here the fault path
        // runs the same core handle_pagefault (the mapping must land in the
        // PTE, and accounting must agree with the hardware table).
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
                // A VFS-backed page needs asynchronous I/O: C suspends the
                // request here and answers after VFS completes. This round fails
                // closed (ok=0 → the kernel treats it as a Fault); it is
                // recorded rather than falsely reported complete.
                Ok(crate::cow_exec_pf::PagefaultAction::Suspended) => return false,
                Ok(crate::cow_exec_pf::PagefaultAction::AccessViolation) => return false,
                Err(_) => return false,
            }
            // Single-page step; the next find_mut re-checks the region
            // boundary (the same shape as C map_handle_memory's per-page
            // walk).
            va += crate::region::PAGE_SIZE as u64;
        }
        true
    }

    /// Main event loop. Never returns (C: main.c:113-193).
    ///
    /// Per-iteration work lives in [`Self::run_once`] so tests can drive a
    /// single dispatch→reply round without spawning the infinite loop. The
    /// loop owns the receive-failure bound that prevents a
    /// busy-spin when the transport is broken. (C's per-iteration
    /// `if(missing_spares > 0) alloc_cycle()` replenishment hook is deleted
    /// with its chain.)
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
                    // would hide the failure — fail fast instead.
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
    /// transport; `run()` supplies the infinite loop, the pressure hook, and
    /// the receive-failure bound.
    fn run_once(&mut self) -> RunStep {
        // The receive half goes through the shared
        // `minix_sef::sef_receive_status` — VM is minix-sef's fourth
        // consumer. SYSTEM notifies surface as signals, RS pings are
        // ponged and swallowed inside the library, everything else falls
        // through to the loop below (C sef.c:149-260 wrapping main.c:113).
        let mut raw_msg = minix_types::Message::default();
        let mut pending_signo: Option<i32> = None;
        // VM's signal face is a SIGKMEM wakeup (it does not exit), so the
        // escape-hatch token only satisfies the library contract — nobody ever
        // cancels it.
        let cancel = minix_sef::SefCancel::new();
        let rcv_sts_raw = {
            let mut guard = self.transport.borrow_mut();
            let mut sef = crate::ipc::transport::SefAdapter(guard.as_mut());
            let rcv = minix_sef::sef_receive_status(
                &mut sef,
                minix_types::Endpoint::ANY,
                &mut raw_msg,
                &mut |signo| pending_signo = Some(signo),
                &cancel,
            );
            match rcv {
                Ok(r) => r,
                // [ARCH: A-14]: C panics (main.c:122-123); a
                // user-space server must survive bad IPC — drop + audit.
                Err(e) => {
                    drop(guard);
                    self.ctx.dropped_messages = self.ctx.dropped_messages.saturating_add(1);
                    // The errno of a receive Err (up to four of them) tells
                    // apart the sources of VM's receive spin (EIO/EAGAIN/…).
                    #[cfg(not(feature = "mock"))]
                    audit_log!("[VM IPC] ipc_receive() failed — message dropped");
                    return RunStep::ReceiveFailed;
                }
            }
        };

        // Signal-arrival arbitration (per the kernel side's established
        // contract): a SYSTEM notify is the wakeup half of
        // `send_sig(VM, SIGKMEM)`, and the sigset half is a documented no-op
        // (SIGKMEM=71 exceeds the 64-bit SigSet; see the
        // os/kernel/src/proc_table.rs vm_enqueue_and_notify_vm contract). VM
        // probes the details through MEMREQ_GET instead of reading a signal
        // number; VM does not consume the SIGKSIG signal-manager family.
        // The number conversion lives in exactly one place
        // (effective_kernel_signal): everything delivered today is the wakeup
        // fallback value, handled as SIGKMEM; once the bit width widens, real
        // numbers are dispatched by number directly.
        if let Some(signo) = Self::effective_kernel_signal(pending_signo) {
            self.handle_signal(signo);
            return RunStep::Handled;
        }

        let msg = raw_msg;
        let rcv_sts = crate::ipc::transport::IpcStatus {
            flags: rcv_sts_raw.status as u32,
        };

        // C: if(is_ipc_notify(rcv_sts)) { continue; } (main.c:126-129).
        // SEF already took the SYSTEM/RS-ping notifies; what reaches here
        // is a leftover notification from any other source — still an
        // async signal, not a request; skipped before endpoint validation
        // PingInvalid (RS notify that failed the ping test)
        // lands here too, matching C's sef.c:208-214 fall-through.
        // C: if(is_ipc_notify(rcv_sts)) { continue; } (main.c:126-129).
        // SEF already took the SYSTEM/RS-ping notifies; what reaches here
        // is a leftover notification from any other source — still an
        // async signal, not a request; skipped before endpoint validation
        // PingInvalid (RS notify that failed the ping test)
        // lands here too, matching C's sef.c:208-214 fall-through.
        //
        // This arm drains memreq: SIGKMEM=71 exceeds the 64-bit SigSet, so
        // the kernel's wakeup notification carries an empty sigset, sef cannot
        // extract a signo, and the message lands here; the contract (VM probes
        // through MEMREQ_GET rather than reading a signal number) requires
        // draining here, or RS's suspended request is never served (on the real
        // machine VM waited silently after sys-susp). When the queue is empty
        // memreq_get returns immediately, so the cost is negligible.
        if rcv_sts.is_notify() {
            self.do_memory();
            return RunStep::Handled;
        }

        // C: who_e = msg.m_source; vm_isokendpt(who_e, &caller_slot);
        let who_e = msg.m_source;
        let caller_slot = match VmProcTable::get_global().vm_isokendpt(who_e) {
            Ok(slot) => slot,
            // [ARCH: A-14]: C panics (main.c:131-132); the
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
                let reply_for_ipc = VmReplyForIpc::new(reply).expect(
                    "DispatchAction::Reply carries VmReply::Suspend; \
                             dispatch_on_msg should translate to DispatchAction::Suspend",
                );
                // Clone once (VmReply is `Clone`) instead of cloning twice
                // as the original code did; the wrapper owns the reply.
                //
                // F10 sign fold: C VM server is built with `_SYSTEM` (vm.h:5),
                // so handler errnos are already negative on the wire. Our Rust
                // tree uses positive errno constants; `reply_status` folds once
                // at the boundary (0 and already-negative values pass through).
                let code = reply_to_errno(reply_for_ipc.payload());
                let mut reply_msg = msg;
                reply_msg.m_type = reply_status(code);
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
                && let Err(mismatches) =
                    crate::sanity::verify_refcounts(frames, VmProcTable::get_global())
            {
                self.ctx.pagefault_errors = self.ctx.pagefault_errors.saturating_add(1);
                audit_log!(
                    "[VM SANITY] {} refcount mismatches detected",
                    mismatches.len()
                );
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
/// permanently broken and panics. C's `sef_receive_status`
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
                    // The birth answering half: C's process_init tail replies
                    // RS_INIT+result to RS **exactly once** (sef_init.c:110-119).
                    // VM is special — to avoid a boot deadlock this **first**
                    // reply takes the asynchronous leg `asynsend3(AMF_NOREPLY)`:
                    // VM registers sef_cb_init_response_rs_asyn_once at
                    // vm/main.c:225-229 (sef_init.c:471-483 = asynsend3, after
                    // which the default synchronous leg is restored). At this
                    // moment RS is still in PAGEFAULT|SENDING and has not entered
                    // catch_boot_init_ready's receive, so a blocking send would
                    // be judged a 2-cycle EDEADLOCK by the kernel (the root
                    // cause). The async leg hands the message to the kernel's
                    // senda queue and returns at once; VM then goes back to
                    // receive to resolve RS's page fault, and the kernel delivers
                    // that message when RS enters its receive. Returning Suspend
                    // suppresses the main loop's second reply (vm/main.c:150).
                    // The async leg does not normally fail; only in extremes such
                    // as a full senda table does it degrade, per asynsend.c:76-79,
                    // to a count plus an audit line — never a panic (C likewise
                    // would not panic VM over a boot deadlock).
                    if let Err(e) = self
                        .transport
                        .borrow_mut()
                        .asynsend(RS_PROC_NR, &minix_sef::sef_init_reply(minix_types::OK))
                    {
                        self.ctx.dropped_messages = self.ctx.dropped_messages.saturating_add(1);
                        audit_log!("[VM RS] birth report asynsend failed: {:?}", e);
                    }
                    return DispatchAction::Suspend;
                }
                Err(e) => {
                    // NS2/E-RPROCTAB: C's process_init replies RS_INIT+result
                    // to RS *unconditionally* — success or failure
                    // (sef_init.c:110-119). For VM the **first** such reply
                    // uses the async leg `asynsend3(AMF_NOREPLY)`
                    // (vm/main.c:225-229 registers sef_cb_init_response_rs_asyn_once,
                    // sef_init.c:471-483) precisely to "avoid a boot-time
                    // deadlock" — the same 2-cycle EDEADLOCK as the Ok arm
                    // Reply with the mapped errno first (async)
                    // so RS's catch_boot_init_ready fails visibly on the
                    // non-OK result (main.c:805-807) without blocking.
                    if let Err(se) = self
                        .transport
                        .borrow_mut()
                        .asynsend(RS_PROC_NR, &minix_sef::sef_init_reply(e.to_errno()))
                    {
                        // A failed birth report is a second, independent drop
                        // event (distinct from the handshake failure counted
                        // below): count it symmetrically with the Ok arm so
                        // the two legs share one accounting rule.
                        self.ctx.dropped_messages = self.ctx.dropped_messages.saturating_add(1);
                        audit_log!("[VM RS] failure birth report asynsend failed: {:?}", se);
                    }
                    // [A-14] fail-closed continues after the reply: C panics
                    // here (main.c:151); minix-rs drops + counts + audits —
                    // no second reply, VM stays alive minus the whole-system
                    // outage ([ARCH: A-14] / V9-P0-1).
                    self.ctx.dropped_messages = self.ctx.dropped_messages.saturating_add(1);
                    audit_log!(
                        "[VM RS] handshake failed (gid={}): {:?} — RS_INIT dropped",
                        init.rproctab_gid,
                        e
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
            // C logs a faked pagefault in release too (printf,
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
            // Never silently drop a pagefault failure — the faulting
            // process stays suspended and would otherwise re-fault forever
            // with no observable signal. Count + audit instead (the counter is
            // also surfaced by the `pagefault_errors()` accessor in tests).
            if let VmReply::Error(e) = reply {
                self.ctx.pagefault_errors = self.ctx.pagefault_errors.saturating_add(1);
                audit_log!(
                    "[VM PF] pagefault failed: err={:?} endpoint={:?} vaddr={:?}",
                    e,
                    source,
                    minix_types::VmPagefaultIn::decode_message(msg).vaddr
                );
                let _ = e;
            }
            return DispatchAction::NoReply;
        }

        // Priority 4: Normal VM calls (main.c:165-176)
        if let Some(c) = callnr(m_type) {
            // C: acl_check(&vmproc[caller_slot], c)
            let table = VmProcTable::get_global();
            // `get_active` returns None once the caller is EXITING
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
                // logging lands).
                audit_log!(
                    "[VM ACL] denied: call=0x{:x} source={:?} caller_slot={:?}",
                    c,
                    source,
                    caller_slot
                );
                let _ = (c, source);
                // C reply semantics: main.c:145 initializes `result = ENOSYS`
                // ("Out of range or restricted calls return this.") and the
                // ACL-denied path never overwrites it — so the caller sees
                // ENOSYS, not the internal EPERM that `acl_check` returned.
                // We mirror that exactly: `AclState::acl_check` still
                // returns `Err(PermissionDenied)` (EPERM, = C's acl_check),
                // but the *reply* errno is ENOSYS (NotImplemented).
                return DispatchAction::Reply(VmReply::Error(VmError::NotImplemented));
            }
            // C: result = vm_calls[c].vmc_func(&msg);
            let result = MessageDispatcher::dispatch_by_number(c, msg, self);
            // Execute deferred VFS callback if present (C: do_vfs_reply
            // invokes req_callback inline, but Rust's borrow checker
            // requires us to defer it until after the vfs_queue borrow
            // is released).
            if let Some((callback, reply, state)) = result.vfs_callback {
                //[SD19-gate] isomorphic to C dropping req_callback's result
                // inline: Rust splits the call out only for the borrow checker,
                // and the failure surface is the same inline drop, with no
                // separate re-attachment
                #[allow(clippy::let_underscore_must_use)]
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
            if !entry.in_use {
                continue;
            }
            let slot = table
                .vm_isokendpt(entry.endpoint)
                .map_err(|_| VmError::InvalidProcess)?;
            let mut proc = table.get_active(slot).ok_or(VmError::InvalidProcess)?;
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
    fn handle_vfs_transid(&mut self, _clean_type: u32, _transid: i32, msg: &Message) -> VmReply {
        // Decode the procctl request from the message body.
        // C: do_procctl reads VMPCTL_PARAM, VMPCTL_WHO, the third M-word,
        //    VMPCTL_LEN and VMPCTL_FLAGS from the m9 overlay (param@16/
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

    /// The terminal "cannot be served" close of C pagefaults.c
    /// `handle_pagefault` (the one tail shared by three exits: unknown region
    /// :99-104, write to a ro-map :112-116, map_pf failure :146-151):
    /// `sys_kill(SIGSEGV)` delivered to the faulting process plus
    /// `sys_vmctl(VMCTL_CLEAR_PAGEFAULT)` to clear its pending bit. Both are
    /// required — C panics on an error from either; if the pending bit is left
    /// set the faulting process sits in RTS_PAGEFAULT forever (on the real
    /// machine: the cr2=0 noaddr exit returned Error without clearing the bit →
    /// the whole system deadlocked silently). A failure here is only audited,
    /// never a panic (the [ARCH: A-14] fail-closed posture, as in the old wro
    /// arm).
    fn pf_fail_segv(&mut self, proc_endpoint: Endpoint) {
        if let Err(e) = self
            .ctx
            .gateway
            .borrow_mut()
            .sys_kill(proc_endpoint, minix_types::SIGNAL_SEGMENT_VIOLATION)
        {
            let _ = &e;
            audit_log!("[VM PF] SIGSEGV delivery failed: {e:?}");
        }
        if let Err(e) = self
            .ctx
            .gateway
            .borrow_mut()
            .sys_vmctl_clear_pagefault(proc_endpoint)
        {
            let _ = &e;
            audit_log!("[VM PF] clear_pagefault failed: {e:?}");
        }
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
        // C: vm_isokendpt only validates in-use; for an IN_USE+EXITING slot (a
        // process that is exiting and has passed VM_WILLEXT) get_active finds
        // nothing → refuse to serve. When the exit chain can run to completion
        // sys_clear releases the whole slot, so nothing needs to be unlocked
        // here (the settled ruling: unlocking must be done together with the
        // termination semantics, so clearing a bit here would be a half fix).
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
        // the error reply still counts the episode.
        let seg_writable = proc
            .regions()
            .find(fault_addr)
            .is_some_and(|r| r.is_writable());
        if request.write && !seg_writable {
            // C: pagefaults.c:112-116 — the SIGSEGV + CLEAR_PAGEFAULT close
            // (originally two inline blocks identical to pf_fail_segv;
            // extracted for sharing without a semantic change).
            self.pf_fail_segv(proc_endpoint);
            return VmReply::Error(VmError::AccessViolation);
        }

        // Destructure the memory context into disjoint
        // &mut fields instead of the former parts_mut() 4-tuple.
        let VmContext {
            page_alloc,
            page_frames,
            page_cache,
            vfs_queue,
            ..
        } = &mut self.ctx;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        // The fault path owns the process page table and must keep
        // it in sync with the bookkeeping slot (C: map_pf → pt_writemap).
        let (regions, pt) = proc.mem_parts_mut();
        let region = match regions.find_mut(fault_addr) {
            Some(r) => r,
            None => {
                // Fix (matching C pagefaults.c:89-105): a fault on an unknown
                // region cannot be served — it returns only after the SIGSEGV +
                // CLEAR_PAGEFAULT close. The old implementation returned
                // Error(InvalidAddress) alone, nobody cleared the pending bit,
                // and the faulting process sat in RTS_PAGEFAULT forever (on the
                // real machine: cr2=0 → the whole system deadlocked silently).
                self.pf_fail_segv(proc_endpoint);
                return VmReply::Error(VmError::InvalidAddress);
            }
        };
        // A DIRECT (device passthrough) region skips the content probe: an
        // MMIO page is (a) outside VM's DM window coverage (reading it is a
        // fatal self-fault for VM) and (b) has read side effects (reading
        // PL011's DR pops the receive FIFO and swallows user input). The probe
        // exists only to validate demand-paged content (file/anon) and means
        // nothing for a passthrough page.
        // CodeReview S1: decide by VrFlags::DIRECT (the only production setter
        // is map_phys.rs), not by matches!(param, Direct) — the default param
        // of an anonymous/file region is Direct{phys:0}, which would kill every
        // anonymous probe.
        let is_direct_region = region.is_direct();
        let service_outcome = crate::cow_exec_pf::handle_pagefault(
            proc_endpoint,
            region,
            frames,
            page_alloc,
            fault_addr,
            request.write,
            table,
            page_cache,
            vfs_queue,
            pt,
        );
        match service_outcome {
            Ok(action) => {
                // Fault accounting. Minix3's VM has no fault
                // counters — the fields are a minix-rs extension following
                // Linux getrusage semantics: minor = satisfied without
                // block I/O (fresh zero page, CoW copy, in-place handled);
                // major = the fault needed VFS I/O, counted at enqueue
                // (Linux counts major when I/O is required, not at
                // completion). Access violations are not faults served.
                match action {
                    crate::cow_exec_pf::PagefaultAction::Suspended => proc.inc_major_fault(),
                    crate::cow_exec_pf::PagefaultAction::AccessViolation => {
                        // C map_pf returns EFAULT → pagefaults.c:144-151
                        // "pagefault not handled": SIGSEGV plus clear the
                        // pending bit. The old implementation only counted and
                        // returned Ok, never clearing the bit (an unobserved
                        // branch of the same defect class as noaddr).
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
                // the system idles after the first user fault (on the real
                // machine: the other half of going silent after fwd
                // Delivered).
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
                    }
                }
                VmReply::Ok
            }
            Err(e) => {
                // C pagefaults.c:144-151 — a service failure is the same
                // "cannot be served" terminal case: SIGSEGV plus clearing the
                // pending bit (the old implementation returned Error and left
                // the bit set).
                self.pf_fail_segv(proc_endpoint);
                VmReply::Error(VmError::AccessViolation)
            }
        }
    }

    /// Returns mutable references to page_alloc, page_frames, page_cache, and vfs_queue simultaneously.
    /// This avoids double mutable borrow when dispatching VM calls that need multiple components.
    #[cfg_attr(not(test), allow(dead_code))] // test-only accessor
    pub(crate) fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Drop for VmServer {
    fn drop(&mut self) {
        // The audit gateway sink follows the server lifetime
        // (same test-isolation contract as unregister_page_alloc below).
        #[cfg(all(not(test), feature = "vm_acl_audit"))]
        crate::audit::clear_gateway();

        // Clear the reclaim sink with the same lifetime contract.
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
/// Note: for a genuine transid message (`m_type == 0xB00 | seq`)
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
    // The row width is that of the shared snapshot `RprocpubSnap` (the
    // `[ARCH: A-4]` single authority): what RS's public table reads out under
    // the `rproctab_gid` grant is exactly this row, and C's `struct rprocpub`
    // byte mirror (`rprocpub_off`) is retired along with the alignment.
    const ENTRY_SIZE: usize = core::mem::size_of::<minix_types::RprocpubSnap>();
    let mut buf = alloc::vec![0u8; ENTRIES * ENTRY_SIZE];
    {
        let mut gateway = server.ctx.gateway.borrow_mut();
        let res = gateway.sys_safecopyfrom(Endpoint::RS, rproctab_gid, 0, &mut buf);
        res.map_err(|_| VmError::InvalidEndpoint)?;
    }
    let mut tab = RprocTab::EMPTY;
    for (i, entry) in tab.entries.iter_mut().enumerate() {
        // SAFETY(read): a snapshot row is a repr(C) POD and is read row by row
        // from the bytes fetched through the grant; `read_unaligned` assumes no
        // buffer alignment (the kernel's safecopy writes a byte stream).
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
// minix-types constant so the two cannot drift).
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
    // Unreachable until E-RSWIRE (the fake-success caller
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
    // The `handle_*` wrappers were superseded by
    // `dispatch_on_msg` → `MessageDispatcher::dispatch_*` (which routes
    // directly); they had zero callers and are removed.

    pub fn has_pending_vfs_requests(&self) -> bool {
        !self.ctx.vfs_queue.is_empty()
    }

    #[cfg_attr(not(test), allow(dead_code))] // test-only accessors
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
    // Test-only constant: the production face of this file does not
    // reference it, so the import lives here instead of the top-level use
    // list (where `clippy --workspace`, which builds without `--tests`,
    // would flag it unused).
    use crate::boot::{BootModule, KernelAllocated};
    use minix_types::USER_STACK_TOP;

    // Pin the backend-selection semantics for every feature
    // combination — this function is the single precedence authority since
    // the contradictory `DefaultAllocator` alias was removed.
    #[cfg(not(any(feature = "buddy_alloc", feature = "segment_tree_alloc")))]
    #[test]
    fn test_choose_allocator_no_backend_feature_selects_bitmap() {
        assert_eq!(
            VmServer::choose_allocator_type(TEST_TOTAL_PAGES),
            PhysAllocType::Bitmap
        );
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
        assert_eq!(
            VmServer::choose_allocator_type(TEST_TOTAL_PAGES),
            PhysAllocType::SegmentTree
        );
    }

    const TEST_TOTAL_PAGES: usize = 256;

    /// Run a test with the direct-map window pointed at a fresh per-thread
    /// leaked buffer (the former process-global mock base +
    /// mutex serialization is replaced by thread-local windows).
    fn with_test_mock_base<F: FnOnce()>(f: F) {
        crate::direct_map::with_test_window(TEST_TOTAL_PAGES, f);
    }

    fn test_free_regions() -> [BootMemRegion; 1] {
        [BootMemRegion {
            base: 0,
            size: TEST_TOTAL_PAGES * CLICK_SIZE,
        }]
    }

    fn make_test_vm_server() -> VmServer {
        VmServer::new(TEST_TOTAL_PAGES, &test_free_regions())
    }

    /// A9 (C17 remainder) source-contract, fixture seam: `BootParams::
    /// simple` takes its initial stack top from the architecture-authority
    /// constant `USER_STACK_TOP` (resolved per host target, zero
    /// literals). `ctx.user_sp` semantically means the stack-top INPUT at
    /// frame-construction time (C `kinfo.user_sp = USR_STACKTOP`,
    /// pre_init.c:156), not the entry SP; the entry-SP alignment assertion
    /// belongs to the C8 fix (embedded in the `test-rt-birth*` gates) and
    /// the two are kept separate.
    #[test]
    fn test_user_sp_source_authority_fixture() {
        let regions = test_free_regions();
        let params = crate::boot::BootParams::simple(TEST_TOTAL_PAGES, &regions);
        assert_eq!(params.user_sp.0, minix_types::USER_STACK_TOP);
    }

    /// A9 (C17 remainder) source-contract, pass-through seam: the
    /// constructor lands params.user_sp verbatim into
    /// `VmContext.user_sp` — no conversion or literal substitution is
    /// allowed anywhere on the source chain. The kernel-side origin (the
    /// user_sp produced by `build_kernel_info`) is already pinned by the
    /// boot-shim host tests (the two `info.user_sp == USER_STACK_TOP`
    /// assertions in opensbi_helpers); with this VM-side seam the source
    /// chain is closed end to end.
    #[test]
    fn test_user_sp_source_authority_ctx_passthrough() {
        with_test_mock_base(|| {
            let server = make_test_vm_server();
            assert_eq!(server.ctx.user_sp.0, minix_types::USER_STACK_TOP);
        });
    }

    /// A6 (C17 remainder) negative control: a handoff missing
    /// kernel_layout (the real v<5 shape) is rejected by `init`
    /// unconditionally — the silent fallback-to-mock production path is
    /// gone, and the mock layout can only arrive via the explicit
    /// `BootParams::simple` fixture.
    #[test]
    #[should_panic(expected = "input rejected (A6)")]
    fn test_init_rejects_missing_kernel_layout() {
        with_test_mock_base(|| {
            let regions = test_free_regions();
            // Field-for-field identical to BootParams::simple except
            // kernel_layout=None — the real v<5 handoff shape.
            let params = crate::boot::BootParams {
                root_paddr: minix_types::PhysBytes(0x900_000),
                kerninfo_pa: minix_types::PhysBytes(0x901_000),
                total_pages: TEST_TOTAL_PAGES,
                free_regions: &regions,
                boot_procs: &[],
                modules: &[],
                kernel_allocated: KernelAllocated::ZERO,
                vm_allocated_bytes: 0,
                is_first_time: true,
                kernel_layout: None,
                user_sp: minix_types::VirBytes(minix_types::USER_STACK_TOP),
            };
            let t = crate::ipc::transport::TestIpcTransport::new();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server = VmServer::new_inner(params, shared);
            server.init();
        });
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
            assert!(matches!(
                result.reply,
                VmReply::Error(VmError::InvalidProcess)
            ));
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
            unsafe {
                table.reset_slot(slot);
            }
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

    /// [ARCH: V20-P1-1] The image-slot router covers the four boot shapes,
    /// `init()` runs exactly once (a second install is refused, and a cold
    /// route over a full slot is the `DoubleInit` breach), and the warm
    /// route carries the slot without re-entering `init`.
    #[test]
    fn the_image_slot_routes_the_four_boot_shapes_and_inits_exactly_once() {
        with_test_mock_base(|| {
            // Empty slot: a fresh boot installs; a re-exec'd warm start has
            // no state and is rejected (case ③, evidence entry).
            let _ = take_server();
            assert_eq!(boot_route(true), BootRoute::ColdInit);
            assert_eq!(boot_route(false), BootRoute::WarmNoState);

            // Installed: exactly once — the second install comes back
            // untouched, and a cold route over a full slot is the breach.
            if install_server(make_test_vm_server()).is_err() {
                panic!("first install refused — the slot must start empty");
            }
            assert!(
                install_server(make_test_vm_server()).is_err(),
                "init must run exactly once"
            );
            assert_eq!(boot_route(true), BootRoute::DoubleInit);

            // One init pass through the slot; the address the PMM pointer
            // registered against is the slot's (stable for the lifetime).
            with_installed_server(|server| server.init());
            with_installed_server(|server| assert!(server.is_initialized()));

            // Warm start in the same process: the route skips init and the
            // initialized flag carries.
            assert_eq!(boot_route(false), BootRoute::WarmRun);
            with_installed_server(|server| assert!(server.is_initialized()));

            // Restore the empty slot for the parallel suite.
            drop(take_server());
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

    /// End-to-end main-loop round. A `TestIpcTransport` drives
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
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            // Register a valid caller at slot 70 (endpoint encodes slot 70).
            let table = VmProcTable::get_global();
            let caller_slot = UserSlot::new(70);
            unsafe {
                table.reset_slot(caller_slot);
            }
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
            unsafe {
                table.reset_slot(caller_slot);
            }
        });
    }

    /// An EXITING caller is refused at the ACL gate with ENOSYS.
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
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server = VmServer::new_for_test(
                TEST_TOTAL_PAGES,
                &test_free_regions(),
                alloc::rc::Rc::clone(&shared),
            );
            server.init();

            // Caller at slot 66 already past VM_WILLEXIT: IN_USE + EXITING,
            // so `get_active` yields None at the gate.
            let table = VmProcTable::get_global();
            let caller_slot = UserSlot::new(66);
            unsafe {
                table.reset_slot(caller_slot);
            }
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
            assert_eq!(
                sent.len(),
                1,
                "denied caller still gets the C-shaped ENOSYS reply"
            );
            assert_eq!(sent[0].0, caller_ep, "reply goes to the caller");
            assert_eq!(
                sent[0].1.m_type,
                -(minix_types::ENOSYS as i32),
                "EXITING caller must be denied with -ENOSYS on the wire (F10: C `_SYSTEM` convention)"
            );

            reset_boot_slots();
            unsafe {
                table.reset_slot(caller_slot);
            }
        });
    }

    /// A notification (IPC_STATUS_CALL == NOTIFY == 4) is skipped
    /// before endpoint validation — it must not be dropped nor answered.
    #[test]
    fn test_run_once_notify_skipped_before_dispatch() {
        with_test_mock_base(|| {
            reset_boot_slots();

            let boot_procs = [vm_boot_image()];
            let regions = test_free_regions();
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            // Source NONE: if the notify were treated as a request it would
            // be dropped as an invalid caller — the skip must happen first.
            let mut msg = Message::default();
            msg.m_source = Endpoint::NONE;
            msg.m_type = 0;
            handle.queue_receive(
                msg,
                IpcStatus {
                    flags: 4, /* NOTIFY */
                },
            );

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);
            assert_eq!(
                server.dropped_messages(),
                0,
                "notify must not be counted as dropped"
            );
            assert!(handle.sent().is_empty(), "notify must not produce a reply");

            reset_boot_slots();
        });
    }

    /// The mapping: a SYSTEM notify surfaces as `SefEvent::Signal` → the
    /// wakeup-half contract (see the run_once comment and
    /// os/kernel/src/proc_table.rs
    /// vm_enqueue_and_notify_vm)→ `handle_signal(SIGKMEM)` → do_memory
    /// probing. Observation: the memreq script is consumed (three kernel calls:
    /// GET/REPLY/terminal GET, verdict OK); before this mapping the old loop
    /// swallowed that notify as an ordinary one.
    #[test]
    fn sef_signal_notify_dispatches_sigset_bits() {
        with_test_mock_base(|| {
            reset_boot_slots();
            let regions = test_free_regions();
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(71);
            unsafe {
                table.reset_slot(slot);
            }
            let empty = table.get_empty(slot).unwrap();
            let ep = Endpoint::from_generation_slot(1, 71);
            let mut active = empty.activate(ep);
            active.init_page_table().unwrap();
            active.init_regions();
            drop(active);
            {
                let mut proc = table.get_active(slot).unwrap();
                // memtype must be present (every C VM region has a type;
                // handle_pagefault dispatches on memtype, and None means
                // NoMemType).
                proc.regions_mut()
                    .insert(crate::region::VirRegion::with_memtype(
                        VirBytes(0x3000_0000),
                        VirBytes(0x4000),
                        crate::region::VrFlags::WRITABLE | crate::region::VrFlags::ANON,
                        &crate::memtype::MEM_TYPE_ANON,
                    ))
                    .unwrap();
            }

            // Scripted kernel: CHECK over the region, then the queue is empty
            // (the same script as test_handle_signal_routes_sigkmem_only).
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
                // Regression: requestor (the suspended process) is
                // deliberately != target (the faulting space), and REPLY's WHO
                // must follow the requestor (C pagefaults.c:206).
                m1.m1p3 = 0x100;
            }
            canned.reply_message(check);
            canned.reply(0);
            canned.reply(minix_types::ENOENT);
            let canned = alloc::rc::Rc::new(canned);
            install_canned_gateway(&mut server, alloc::rc::Rc::clone(&canned));

            // SYSTEM notify (the wakeup half of the kernel's send_sig; the
            // sigset half is a no-op and carries no bits).
            let mut notify = Message::default();
            notify.m_source = Endpoint::SYSTEM;
            notify.m_type = minix_types::NOTIFY_MESSAGE;
            handle.queue_receive(
                notify,
                IpcStatus {
                    flags: 4, /* NOTIFY */
                },
            );

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);
            assert_eq!(
                server.dropped_messages(),
                0,
                "signal is handled, not dropped"
            );
            assert!(handle.sent().is_empty(), "signal must not be replied to");

            // Kernel-observable: the drain loop really ran (GET/REPLY/terminal
            // GET).
            let sent = canned.sent.borrow();
            assert_eq!(sent.len(), 3, "GET, REPLY, second GET");
            {
                // SAFETY: reply wire inspection (SYS_VMCTL parameter words).
                let m1 = unsafe { &sent[1].m_u.m_m1 };
                assert_eq!(
                    m1.m1i1, 0x100,
                    "reply WHO carries the requestor, not the fault target (C pagefaults.c:206)"
                );
                assert_eq!(m1.m1i2, 15, "VMCTL_MEMREQ_REPLY");
                assert_eq!(m1.m1i3, 0, "valid writable range → OK verdict");
            }

            reset_boot_slots();
        });
    }

    /// The mapping: an RS ping is ponged and swallowed inside minix-sef (it
    /// never surfaces and never enters dispatch); later messages keep being
    /// received normally.
    #[test]
    fn sef_ping_ponged_and_swallowed() {
        with_test_mock_base(|| {
            reset_boot_slots();
            let regions = test_free_regions();
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            // RS ping: RS source + NOTIFY_MESSAGE + the notify status bit.
            let mut ping = Message::default();
            ping.m_source = Endpoint::RS;
            ping.m_type = minix_types::NOTIFY_MESSAGE;
            handle.queue_receive(
                ping,
                IpcStatus {
                    flags: 4, /* NOTIFY */
                },
            );
            // Afterwards: an ordinary notify (NONE source) — sef lets it
            // through as a Call and the main loop swallows it per
            // main.c:126-129.
            let mut stray = Message::default();
            stray.m_source = Endpoint::NONE;
            stray.m_type = 0;
            handle.queue_receive(
                stray,
                IpcStatus {
                    flags: 4, /* NOTIFY */
                },
            );

            let step = server.run_once();
            assert_eq!(step, RunStep::Handled);
            assert_eq!(server.dropped_messages(), 0);
            assert_eq!(handle.notified(), alloc::vec![Endpoint::RS], "ping → pong");
            assert!(handle.sent().is_empty(), "ping must not produce a reply");

            reset_boot_slots();
        });
    }

    /// A receive failure is dropped + counted, and reported as
    /// `RunStep::ReceiveFailed` so `run()` can bound consecutive failures.
    #[test]
    fn test_run_once_receive_failure_counts() {
        with_test_mock_base(|| {
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server = VmServer::new_for_test(
                TEST_TOTAL_PAGES,
                &test_free_regions(),
                alloc::rc::Rc::clone(&shared),
            );
            server.init();
            handle.set_should_fail(true);

            let step = server.run_once();
            assert_eq!(step, RunStep::ReceiveFailed);
            assert_eq!(server.dropped_messages(), 1);
        });
    }

    /// The main-loop counters are observable via `InfoStats` —
    /// after N dropped receives, a VM_INFO (VMIW_STATS) query reports them.
    #[test]
    fn test_dropped_messages_observable_via_info_stats() {
        with_test_mock_base(|| {
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server = VmServer::new_for_test(
                TEST_TOTAL_PAGES,
                &test_free_regions(),
                alloc::rc::Rc::clone(&shared),
            );
            server.init();

            // Register a valid caller at slot 70 (endpoint encodes slot 70).
            let table = VmProcTable::get_global();
            let caller_slot = UserSlot::new(70);
            unsafe {
                table.reset_slot(caller_slot);
            }
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
                VmReply::InfoStats {
                    dropped_messages,
                    pagefault_errors,
                    ..
                } => {
                    assert_eq!(
                        dropped_messages, 3,
                        "dropped counter must surface via InfoStats"
                    );
                    assert_eq!(pagefault_errors, 0);
                }
                other => panic!("VMIW_STATS must decode to InfoStats: {:?}", other),
            }

            unsafe {
                table.reset_slot(caller_slot);
            }
        });
    }

    /// A permanently broken transport must not busy-spin — after
    /// `MAX_CONSECUTIVE_RECV_FAILURES` consecutive failures `run()` panics
    /// instead of burning 100% CPU (C's `sef_receive_status` blocks).
    #[test]
    fn test_run_busy_loop_protection() {
        with_test_mock_base(|| {
            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server = VmServer::new_for_test(
                TEST_TOTAL_PAGES,
                &test_free_regions(),
                alloc::rc::Rc::clone(&shared),
            );
            server.init();
            handle.set_should_fail(true);

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                server.run();
            }));
            assert!(
                result.is_err(),
                "broken transport must panic, not busy-loop"
            );
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
            unsafe {
                table.reset_slot(slot);
            }
            let empty = table.get_empty(slot).unwrap();
            let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
            let mut active = empty.activate(ep);
            active.init_regions();

            let mmap_req = minix_types::VmMmapIn {
                caller: ep,
                forwhom: Endpoint::NONE,
                addr: VirBytes(0),
                length: VirBytes(0x1000),
                prot: 1,       // PROT_READ
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
            let state = crate::vfs_queue::VfsRequestState::FdLookup { mmap: mmap_req };

            crate::mmap::mmap_file_cont(&mut server, &reply, &state)
                .expect("callback must succeed");

            let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
            let region = active
                .regions()
                .find_overlap(
                    VirBytes(0x0000_0001_0000_0000),
                    VirBytes(crate::mmap::MMAP_TOP),
                )
                .expect("file region must exist after mmap_file_cont");
            assert!(!region.flags.contains(crate::region::VrFlags::ANON));
            assert!(matches!(
                region.param,
                crate::region::VrParam::File {
                    inited: true,
                    offset: 0,
                    ..
                }
            ));
            // read-only mapping (PROT_READ) → not writable
            assert!(!region.flags.contains(crate::region::VrFlags::WRITABLE));

            unsafe {
                table.reset_slot(slot);
            }
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
        // A pagefault whose handling fails (here: endpoint not in
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
            assert_eq!(
                server.pagefault_errors(),
                1,
                "failed pagefault must be counted"
            );
        });
    }

    #[test]
    fn test_pagefault_accounting_minor_and_violation() {
        // A successfully served fault bumps minor (fresh anon
        // page); a write to a read-only region is an access violation and
        // counts nothing (Linux getrusage semantics — minix3 VM has no
        // counters, the fields are a minix-rs extension).
        use crate::region::{VirRegion, VrFlags};
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(72);
            unsafe {
                table.reset_slot(slot);
            }
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

            // A write to the read-only region is
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
        // Regression guard (matching C pagefaults.c:89-105): a fault outside
        // every region must close with SIGSEGV + CLEAR_PAGEFAULT. Before the
        // fix the noaddr exit returned Error(InvalidAddress) alone — the two
        // mock-gateway assertions below were both empty and the faulting
        // process stayed in RTS_PAGEFAULT (on the real machine, cr2=0 made the
        // whole system deadlock silently). The existing wro test covers only
        // the "write to a read-only region" path, and no assertion reached the
        // unknown-region exit — that is what makes this test discriminating.
        use crate::region::{VirRegion, VrFlags};
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(73);
            unsafe {
                table.reset_slot(slot);
            }
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

            // A read fault @0x0 belongs to no region (the cr2=0 shape seen on
            // the real machine).
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
        // A genuine VFS transid message carries m_type = 0xB00|seq
        // (the transid IS the type; TRNS_DEL_ID yields 0) and the procctl
        // parameters in the m9 body. P1 must route it to VM_PROCCTL: CLEAR
        // from VFS executes and empties the target's address space.
        use crate::region::{VirRegion, VrFlags};
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(73);
            unsafe {
                table.reset_slot(slot);
            }
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

            // Swap in a Mock gateway: CLEAR now ends with C's `pt_bind`
            // leg (`sys_vmctl_set_addrspace`), which the host
            // build's default `DirectKernelCallTransport` cannot service.
            // The shared handle lets the test assert the rebind fired.
            let mock = alloc::rc::Rc::new(core::cell::RefCell::new(
                crate::kernel_gateway::MockGateway::new(),
            ));
            server.ctx.gateway = alloc::rc::Rc::new(core::cell::RefCell::new(
                alloc::boxed::Box::new(SharedMockGateway(alloc::rc::Rc::clone(&mock)))
                    as alloc::boxed::Box<dyn crate::kernel_gateway::KernelGateway>,
            ));

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
            // pt_bind leg: the fresh page-table root is rebound into the
            // kernel for the target endpoint (virt alias 0 under Direct Map).
            {
                let sets = mock.borrow().addrspace_sets.borrow().clone();
                assert_eq!(sets.len(), 1, "CLEAR must rebind the address space");
                assert_eq!(sets[0].0, ep, "rebind targets the cleared process");
                assert_ne!(sets[0].1, 0, "CLEAR must send a real root paddr");
                assert_eq!(sets[0].2, 0, "Direct Map sends virt_root = None (0)");
            }
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
        assert!(is_vfs_fs_transid(0xB00)); // base itself
        assert!(is_vfs_fs_transid(0xB01)); // base + 1
        assert!(is_vfs_fs_transid(0xBFF)); // base + 0xFF
        assert!(is_vfs_fs_transid(0xB45)); // arbitrary within range
    }

    #[test]
    fn test_is_vfs_fs_transid_invalid() {
        assert!(!is_vfs_fs_transid(0x000)); // zero
        assert!(!is_vfs_fs_transid(0xA00)); // wrong base
        assert!(!is_vfs_fs_transid(0xC00)); // VM_RQ_BASE
        assert!(!is_vfs_fs_transid(0x600)); // VFS call number
        assert!(!is_vfs_fs_transid(0xB100)); // base shifted left
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

            // C parity: TRNS_DEL_ID on a genuine transid message
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
            // The former dedicated transid==0 gate is gone —
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
            msg.m_u.m_lc_vm_procctl.who = 0; // VMPCTL_WHO = 0 (invalid)
            msg.m_u.m_lc_vm_procctl.m1 = 0; // the VMPCTL third M-word (m9_l3)
            msg.m_u.m_lc_vm_procctl.len = 0; // VMPCTL_LEN
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
            kerninfo_pa: PhysBytes(0x901_000),
            total_pages: TEST_TOTAL_PAGES,
            free_regions,
            boot_procs,
            modules,
            kernel_allocated: KernelAllocated::ZERO,
            vm_allocated_bytes: 0,
            is_first_time: true,
            // A6: tests that run init must inject the mock layout fixture
            // explicitly (a missing layout is rejected).
            kernel_layout: Some(minix_types::KernelLayout::new(
                0xFFFF_FFFF_8000_0000,
                0x100_0000,
                8,
                8,
                0xFFFF_8000_0000_0000,
                4,
            )),
            user_sp: VirBytes(USER_STACK_TOP as u64),
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
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server = VmServer::new_for_test(
                TEST_TOTAL_PAGES,
                &test_free_regions(),
                alloc::rc::Rc::clone(&shared),
            );
            server.init();

            // Queue an FdClose via the enqueue half.
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
            let call_msg = server
                .ctx
                .vfs_queue
                .take_pending_vfs_call()
                .expect("wire built");
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
            // The stack region is a 4MiB window below user_sp (C
            // DEFAULT_STACK_LIMIT), and the frame page sits at offset
            // (frame_page - region.vaddr) inside it.
            let frame_off = frame_page - region.vaddr.0;
            let pfn = region
                .get_slot(minix_types::VirBytes(frame_off))
                .and_then(|s| s.pfn())
                .expect("stack page materialized");
            let off_in_page = (vsp - frame_page) as usize;
            let phys_page = pfn as u64 * PS as u64;
            let va = crate::direct_map::vm_phys_to_virt(crate::phys_mem::AlignedPhysBytes::new(
                phys_page,
            ));
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
                BootModule {
                    start_addr: 0x2000,
                    len: CLICK_SIZE as u64,
                }, // 1 page
                BootModule {
                    start_addr: 0x3000,
                    len: 1,
                }, // excluded (last)
            ];
            let regions = test_free_regions();
            let mut server = VmServer::new_with_boot_params(boot_params(&regions, &[], &modules));
            server.init();
            // global::init() reset the total to the allocator total; the
            // single charged module adds exactly 1 page.
            assert_eq!(crate::global::total_pages(), TEST_TOTAL_PAGES + 1);
        });
    }

    /// The invalid-caller drop branch (run_once
    /// :713-723) — an unregistered endpoint's message is dropped with a
    /// counter bump, no reply, no panic. Mirrors [ARCH: A-14].
    #[test]
    fn test_run_once_invalid_caller_dropped() {
        with_test_mock_base(|| {
            reset_boot_slots();

            let t = crate::ipc::transport::TestIpcTransport::new();
            let handle = t.handle();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server = VmServer::new_for_test(
                TEST_TOTAL_PAGES,
                &test_free_regions(),
                alloc::rc::Rc::clone(&shared),
            );
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

    /// Pins the reply-encode failure invariant that
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
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server = VmServer::new_for_test(
                TEST_TOTAL_PAGES,
                &test_free_regions(),
                alloc::rc::Rc::clone(&shared),
            );
            server.init();

            // Register RS as a live caller (slot 2 ↔ RS_PROC_NR; run_once
            // drops messages from unregistered senders before the
            // handshake runs). C: RS is a boot-image process, so its slot
            // is active by construction (main.c:497-520).
            let rs_table = VmProcTable::get_global();
            let rs_slot = UserSlot::new(2);
            unsafe {
                rs_table.reset_slot(rs_slot);
            }
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
            unsafe {
                rs_table.reset_slot(vfs_slot);
            }
            let vfs_empty = rs_table.get_empty(vfs_slot).unwrap();
            let mut vfs_pre = vfs_empty.activate(Endpoint::VFS);
            vfs_pre.init_page_table().expect("vfs pt");
            vfs_pre.init_regions();

            // The grant fetches the shared snapshot of `NR_BOOT_PROCS` rows (the
            // A-4 row width).
            let entry_size = core::mem::size_of::<minix_types::RprocpubSnap>();
            let table_bytes = minix_types::NR_BOOT_PROCS * entry_size;
            *mock.borrow_mut().safecopy_payload.borrow_mut() = vfs_rprocpub_entry_image();

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

            // In the same round as a successful handshake a birth report must go
            // back (process_init's tail, sef_init.c:110-119) — RS boot step3
            // waits for exactly that; SUSPEND only suppresses the main loop's
            // second reply.
            let sent = handle.sent();
            assert_eq!(sent.len(), 1, "恰好一条出生应答");
            assert_eq!(sent[0].0, Endpoint::RS);
            assert_eq!(sent[0].1.m_type, RS_INIT as i32);
            assert_eq!(sent[0].1.rs_init_result(), Some(minix_types::OK));
            // The birth report must take the **asynchronous leg**
            // (asynsend3/AMF_NOREPLY), not a blocking send — otherwise a
            // 2-cycle EDEADLOCK is judged while RS sits in
            // PAGEFAULT|SENDING.
            assert_eq!(
                handle.async_sends(),
                1,
                "birth report must go out via the async (SENDA) leg, not a blocking send"
            );

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
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server = VmServer::new_for_test(
                TEST_TOTAL_PAGES,
                &test_free_regions(),
                alloc::rc::Rc::clone(&shared),
            );
            server.init();

            // Register RS as a live caller — without this the pre-dispatch
            // sender check drops the RS_INIT before the handshake ever runs
            // (the failure reply below must be reachable; main.c:497-520 —
            // RS is a boot-image process, its slot is active by construction).
            let rs_table = VmProcTable::get_global();
            let rs_slot = UserSlot::new(2);
            unsafe {
                rs_table.reset_slot(rs_slot);
            }
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
                    Err(crate::kernel_gateway::GatewayError::Kernel(
                        -minix_types::EIO,
                    ))
                }
                fn sys_safecopyfrom(
                    &mut self,
                    _granter: Endpoint,
                    _grant_id: i32,
                    _offset: u64,
                    _buf: &mut [u8],
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Err(crate::kernel_gateway::GatewayError::Kernel(
                        -minix_types::EIO,
                    ))
                }
                fn sys_fork(
                    &mut self,
                    _: Endpoint,
                    _: UserSlot,
                    _: u32,
                ) -> Result<(Endpoint, Option<u64>), crate::kernel_gateway::GatewayError>
                {
                    Err(crate::kernel_gateway::GatewayError::Kernel(
                        -minix_types::EIO,
                    ))
                }
                fn sys_exec(
                    &mut self,
                    _: Endpoint,
                    _: u64,
                    _: u64,
                    _: u64,
                    _: u64,
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Err(crate::kernel_gateway::GatewayError::Kernel(
                        -minix_types::EIO,
                    ))
                }
                fn sys_update(
                    &mut self,
                    _: Endpoint,
                    _: Endpoint,
                    _: u32,
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Err(crate::kernel_gateway::GatewayError::Kernel(
                        -minix_types::EIO,
                    ))
                }
                fn sys_kill(
                    &mut self,
                    _: Endpoint,
                    _: i32,
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Err(crate::kernel_gateway::GatewayError::Kernel(
                        -minix_types::EIO,
                    ))
                }
                fn sys_vmctl_memreq_get(
                    &mut self,
                ) -> Result<
                    Option<crate::kernel_gateway::KernelMemReq>,
                    crate::kernel_gateway::GatewayError,
                > {
                    Ok(None)
                }
                fn sys_vmctl_memreq_reply(
                    &mut self,
                    _: Endpoint,
                    _: bool,
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Ok(())
                }
                fn sys_vmctl_clear_pagefault(
                    &mut self,
                    _: Endpoint,
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Ok(())
                }
                fn sys_vmctl_boot_inhibit_clear(
                    &mut self,
                    _: Endpoint,
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Ok(())
                }
                fn sys_vmctl_set_addrspace(
                    &mut self,
                    _: Endpoint,
                    _: u64,
                    _: u64,
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Ok(())
                }
                fn diag_write(
                    &mut self,
                    _text: &str,
                ) -> Result<(), crate::kernel_gateway::GatewayError> {
                    Ok(())
                }
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
            assert_eq!(
                server.dropped_messages(),
                1,
                "failed handshake must be counted"
            );
            // The failure arm's birth report likewise takes the asynchronous
            // leg (the same as C's first reply).
            assert_eq!(
                handle.async_sends(),
                1,
                "failure birth report must use the async leg"
            );

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
        assert_eq!(
            tab.entries
                .iter()
                .filter(|e| e.endpoint != Endpoint::NONE)
                .count(),
            0
        );
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
        // SAFETY(test): a row is a repr(C) POD; take its byte view and splice
        // it into the buffer.
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
        // SAFETY(test): read the row's bytes back as the same structure.
        unsafe { core::ptr::read_unaligned(img.as_ptr().cast::<minix_types::RprocpubSnap>()) }
            .vm_call_mask
    }
    // ── SIGKMEM signal seam + do_memory drain loop ──────────────────

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
        server.ctx.gateway = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(
            crate::kernel_gateway::TrapKernelGateway {
                transport: SharedCanned(canned),
            },
        )));
    }

    #[test]
    fn test_do_memory_services_kernel_check_request() {
        with_test_mock_base(|| {
            reset_boot_slots();
            let boot_procs = [vm_boot_image()];
            let regions = test_free_regions();
            let t = crate::ipc::transport::TestIpcTransport::new();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            // Target process with a writable anonymous region.
            let table = VmProcTable::get_global();
            let slot = UserSlot::new(71);
            unsafe {
                table.reset_slot(slot);
            }
            let empty = table.get_empty(slot).unwrap();
            let ep = Endpoint::from_generation_slot(1, 71);
            let mut active = empty.activate(ep);
            active.init_page_table().unwrap();
            active.init_regions();
            drop(active);
            {
                let mut proc = table.get_active(slot).unwrap();
                // memtype must be present (every C VM region has a type;
                // handle_pagefault dispatches on memtype, and None means
                // NoMemType).
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
                // Regression: requestor != target (the cross-process
                // suspension shape).
                m1.m1p3 = 0x100;
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
                // SAFETY: reply wire inspection (SYS_VMCTL parameter words).
                let m1 = unsafe { &sent[1].m_u.m_m1 };
                assert_eq!(
                    m1.m1i1, 0x100,
                    "reply WHO carries the requestor, not the fault target (C pagefaults.c:206)"
                );
                assert_ne!(
                    m1.m1i1, ep.0,
                    "cross-process script: requestor differs from target"
                );
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
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            let table = VmProcTable::get_global();
            let slot = UserSlot::new(71);
            unsafe {
                table.reset_slot(slot);
            }
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
                m1.m1p3 = 0x100; // requestor != target (cross-process shape)
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
    fn test_effective_kernel_signal_maps_the_wakeup_fallback() {
        // The fork between the two delivery shapes (the fallback, and the real
        // number once the bit width widens): a fallback value can only be
        // handled by its "only possible source" = SIGKMEM, while a real number
        // is always handed through as is and no longer taken for SIGKMEM (that
        // wrong fork is the reason this function exists).
        use minix_sef::SEF_SIGNAL_REQUEST_TYPE;
        use minix_types::{SIGNAL_KERNEL_MEMORY, SIGNAL_KERNEL_PENDING};
        assert_eq!(
            VmServer::effective_kernel_signal(Some(SEF_SIGNAL_REQUEST_TYPE)),
            Some(SIGNAL_KERNEL_MEMORY),
            "今天的生产路径：空位图→唤醒兜底值→按 SIGKMEM 处理"
        );
        assert_eq!(
            VmServer::effective_kernel_signal(Some(SIGNAL_KERNEL_MEMORY)),
            Some(SIGNAL_KERNEL_MEMORY)
        );
        assert_eq!(
            VmServer::effective_kernel_signal(Some(SIGNAL_KERNEL_PENDING)),
            Some(SIGNAL_KERNEL_PENDING),
            "真号按号分派：不得把 74 当 71 用"
        );
        assert_eq!(VmServer::effective_kernel_signal(None), None);
    }

    #[test]
    fn test_handle_signal_routes_sigkmem_only() {
        with_test_mock_base(|| {
            reset_boot_slots();
            let boot_procs = [vm_boot_image()];
            let regions = test_free_regions();
            let t = crate::ipc::transport::TestIpcTransport::new();
            let shared = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(t)
                as alloc::boxed::Box<dyn crate::ipc::transport::IpcTransport>));
            let mut server =
                VmServer::new_for_test(TEST_TOTAL_PAGES, &regions, alloc::rc::Rc::clone(&shared));
            server.init();

            let mut canned = minix_sys::syscall::CannedKernelCallTransport::new();
            // SIGKMEM routes into the drain loop: GET (raw CHECK reply; no
            // payload, so the request's own parameter words read back) → REPLY →
            // GET → ENOENT stops the loop. Every call scripted explicitly.
            canned.reply(1); // VMPTYPE_CHECK
            canned.reply(0); // REPLY acknowledged OK
            canned.reply(minix_types::ENOENT);
            let canned = alloc::rc::Rc::new(canned);
            install_canned_gateway(&mut server, alloc::rc::Rc::clone(&canned));

            // Unknown signal → no kernel interaction (C ignores the rest).
            server.handle_signal(0);
            assert_eq!(canned.calls.get(), 0);
            // Another real kernel signal number (SIGKSIG=74) must not wrongly
            // enter the drain ring either — that is exactly the distinction the
            // numbered dispatch newly provides.
            server.handle_signal(minix_types::SIGNAL_KERNEL_PENDING);
            assert_eq!(
                canned.calls.get(),
                0,
                "non-SIGKMEM kernel signal is ignored"
            );
            server.handle_signal(minix_types::SIGNAL_KERNEL_MEMORY);
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
    /// handle point at the same concrete mock.
    struct SharedMockGateway(
        alloc::rc::Rc<core::cell::RefCell<crate::kernel_gateway::MockGateway>>,
    );

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
        fn sys_fork(
            &mut self,
            parent: Endpoint,
            child_slot: UserSlot,
            flags: u32,
        ) -> Result<(Endpoint, Option<u64>), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().sys_fork(parent, child_slot, flags)
        }
        fn sys_exec(
            &mut self,
            endpt: Endpoint,
            ip: u64,
            stack: u64,
            name_ptr: u64,
            ps_str: u64,
        ) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0
                .borrow_mut()
                .sys_exec(endpt, ip, stack, name_ptr, ps_str)
        }
        fn sys_update(
            &mut self,
            src: Endpoint,
            dst: Endpoint,
            flags: u32,
        ) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().sys_update(src, dst, flags)
        }
        fn sys_vmctl_memreq_get(
            &mut self,
        ) -> Result<Option<crate::kernel_gateway::KernelMemReq>, crate::kernel_gateway::GatewayError>
        {
            self.0.borrow_mut().sys_vmctl_memreq_get()
        }
        fn sys_vmctl_memreq_reply(
            &mut self,
            target: Endpoint,
            ok: bool,
        ) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().sys_vmctl_memreq_reply(target, ok)
        }
        fn sys_kill(
            &mut self,
            endpoint: Endpoint,
            signal: i32,
        ) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().sys_kill(endpoint, signal)
        }
        fn sys_vmctl_clear_pagefault(
            &mut self,
            endpoint: Endpoint,
        ) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().sys_vmctl_clear_pagefault(endpoint)
        }
        fn sys_vmctl_boot_inhibit_clear(
            &mut self,
            endpoint: Endpoint,
        ) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().sys_vmctl_boot_inhibit_clear(endpoint)
        }
        fn sys_vmctl_set_addrspace(
            &mut self,
            endpoint: Endpoint,
            ptroot_phys: u64,
            ptroot_virt: u64,
        ) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0
                .borrow_mut()
                .sys_vmctl_set_addrspace(endpoint, ptroot_phys, ptroot_virt)
        }
        fn diag_write(&mut self, text: &str) -> Result<(), crate::kernel_gateway::GatewayError> {
            self.0.borrow_mut().diag_write(text)
        }
    }
}
