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

use minix_types::{Endpoint, UserSlot, BootImage, NR_BOOT_PROCS, VmPagefaultIn, VmProcctlIn, Message, VmReply, VmError, VM_RQ_BASE, VM_PROCCTL, EncodeToM1};
#[cfg(test)]
use minix_types::{VmForkIn, VmBrkIn, VmExitIn, VmMmapIn, VmMapPhysIn, VmCacheIn};
use crate::vmproc::VmProcTable;
use crate::alloc_page::VmPageAllocator;
use crate::phys_mem::{PhysAlloc, PhysAllocType, BitmapAllocator, PhysAllocator, BootMemRegion, AlignedPhysBytes, bytes_to_clicks, CLICK_SIZE};
#[cfg(feature = "buddy_alloc")]
use crate::phys_mem::{BuddyAllocator, BUDDY_THRESHOLD_PAGES};
#[cfg(feature = "segment_tree_alloc")]
use crate::phys_mem::SegmentTreeAllocator;
use crate::boot::{BootParams, KernelAllocated, VM_PROC_NR};
use crate::page_cache::PageCache;
use crate::vfs_queue::VfsRequestQueue;
use crate::ipc::dispatcher::MessageDispatcher;
use crate::ipc::transport::IpcStatus;
#[cfg(not(test))]
use crate::pagetable::vm_self_map::init_vm_self_pt;
#[cfg(not(test))]
use crate::direct_map::vm_phys_to_virt;
#[cfg(test)]
use minix_types::VirBytes;
use crate::region::PageFrames;
use minix_types::PhysBytes;

/// Cache-reclaim batch size for the main-loop `alloc_cycle` hook.
///
/// C: `alloc_mem` retries after `cache_freepages(1024)` on exhaustion
/// (alloc.c:242-279); the same batch is used here.
const FREE_CACHE_BATCH: usize = 1024;

pub struct VmServer {
    page_alloc: VmPageAllocator,
    page_cache: PageCache,
    page_frames: Option<PageFrames>,
    vfs_queue: VfsRequestQueue,
    initialized: bool,
    /// Allocation-pressure counter surfaced to the main loop.
    ///
    /// C: `missing_spares` (alloc.c:74) is the *reserve-queue deficit* —
    /// incremented by `reservedqueue_alloc()` (alloc.c:216) when a spare page
    /// is drawn and decremented by `reservedqueue_fillslot()` (alloc.c:142)
    /// when a slot is refilled; the main loop calls `alloc_cycle()`
    /// (alloc.c:227-237, main.c:118-119) to top the queues back up.
    ///
    /// Rust design: the Direct Map (`[ARCH: A-1]`) structurally eliminates the
    /// spare queues (see 06-page-allocator.md §3.3), so this counter is
    /// re-interpreted as *allocation-pressure accounting* — `mark_alloc_failure()`
    /// records a page-allocation failure and the main loop's `alloc_cycle()`
    /// hook is the replenishment opportunity (page-cache reclaim plugs in
    /// there, DEFERRED to 24-page-cache). The C "deficit" semantics and the
    /// Rust "pressure" semantics converge on the same observable contract:
    /// `> 0` → the loop re-attempts memory replenishment on its next pass.
    ///
    /// A plain `u32` because the VM event loop is single-threaded (no
    /// concurrent increments possible). `mark_alloc_failure()` /
    /// `alloc_cycle()` are the only mutating access points, both
    /// `&mut self`-only.
    missing_spares: u32,
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
    pagefault_errors: u64,
    /// Count of IPC messages dropped at the main-loop boundary.
    ///
    /// V9-P0-1 (todo): the C main loop panics on receive failure
    /// (main.c:122-123) and on invalid callers (main.c:131-132). A
    /// user-space server must treat IPC as untrusted input: VM is the
    /// system's only memory manager, and a panic would halt all memory
    /// management with unrecoverable page/refcount/region state. We drop
    /// the offending message and count it instead — the caller (if any)
    /// times out, which is the same observable outcome as C for that
    /// caller, minus the whole-server outage ([ARCH: A-14]).
    ///
    /// Saturating so a hostile fault storm cannot wrap the counter.
    dropped_messages: u64,
    /// Boot process images, copied at construction.
    ///
    /// C: `kernel_boot_info.boot_procs[]` (main.c:497-520). Copied from
    /// [`BootParams`] so `init()` does not borrow external memory; the
    /// kernel→VM boot protocol is a one-shot hand-off.
    boot_procs: [BootImage; NR_BOOT_PROCS],
    /// Pages to charge to the global page total during init.
    ///
    /// C: `mem_add_total_pages()` call points (main.c:485-495).
    boot_extra_pages: usize,
    /// Kernel's own memory footprint, kept for the kernel usage query.
    ///
    /// C: `kernel_boot_info.kernel_allocated_bytes(_dynamic)` — consumed by
    /// `get_usage_info_kernel` (region.c:1357-1364).
    kernel_allocated: KernelAllocated,
    /// Bytes the kernel allocated to load VM, kept for the VM-self usage query.
    ///
    /// C: `kernel_boot_info.vm_allocated_bytes` — consumed by
    /// `get_usage_info_vm` (region.c:1366-1373).
    vm_allocated_bytes: u64,
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

        Self {
            page_alloc,
            page_cache: PageCache::new(),
            page_frames: None,
            vfs_queue: VfsRequestQueue::new(),
            initialized: false,
            missing_spares: 0,
            pagefault_errors: 0,
            dropped_messages: 0,
            boot_procs,
            boot_extra_pages: params.extra_pages(),
            kernel_allocated: params.kernel_allocated,
            vm_allocated_bytes: params.vm_allocated_bytes,
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
        // In test builds on x86_64, vm_phys_to_virt uses a constant base (0x80000000)
        // which is not valid heap memory. Use mock_vm_base() directly instead.
        let meta_va = {
            #[cfg(test)]
            {
                let base = minix_arch::direct_map::mock_vm_base();
                VirBytes(base + meta_phys_base as u64)
            }
            #[cfg(not(test))]
            {
                vm_phys_to_virt(AlignedPhysBytes::new(meta_phys_base as u64))
            }
        };
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
    /// Enabling a backend Cargo feature selects that backend, mirroring
    /// `DefaultAllocator` precedence in `phys_mem/mod.rs` (buddy >
    /// segment-tree > bitmap):
    ///
    /// - `buddy_alloc` keeps the documented adaptive threshold — below
    ///   `BUDDY_THRESHOLD_PAGES` (1M pages ≈ 4GB) the compact bitmap is
    ///   used even when the feature is enabled (05-physical-memory.md §3.3).
    /// - `segment_tree_alloc` selects the segment-tree backend outright
    ///   (was previously compiled but never selected — V10-P0-1).
    #[cfg_attr(
        not(any(feature = "buddy_alloc", feature = "segment_tree_alloc")),
        allow(unused_variables)
    )]
    fn choose_allocator_type(total_pages: usize) -> PhysAllocType {
        #[cfg(feature = "buddy_alloc")]
        {
            if total_pages > BUDDY_THRESHOLD_PAGES {
                return PhysAllocType::Buddy;
            }
        }
        #[cfg(feature = "segment_tree_alloc")]
        {
            return PhysAllocType::SegmentTree;
        }
        PhysAllocType::Bitmap
    }

    fn relocate(&mut self) {
        let (total_pages, old_pa_base, old_pa_pages) = {
            let phys_alloc = self.page_alloc.phys_alloc();
            let bitmap = phys_alloc.as_bitmap().expect("relocate: bootstrap allocator must be Bitmap");
            let (pa_base, pa_pages) = bitmap.metadata_pa_range();
            assert!(pa_pages > 0, "relocate: no BumpBuf metadata to relocate (already relocated?)");
            (bitmap.total_count(), pa_base, pa_pages)
        };

        let alloc_type = Self::choose_allocator_type(total_pages);

        let meta_size = alloc_type.metadata_size(total_pages);
        let pages = bytes_to_clicks(meta_size);
        let new_va = crate::global::heap_arena_grow(pages, &mut self.page_alloc)
            .expect("relocate: failed to allocate new metadata via HeapArena");
        // SAFETY: new_va points to a valid heap-arena region of meta_size bytes;
        // no aliasing references exist.
        let new_metadata = unsafe {
            core::slice::from_raw_parts_mut(new_va as *mut u8, meta_size)
        };

        let mut free_regions: alloc::vec::Vec<BootMemRegion> = alloc::vec![];
        {
            let phys_alloc = self.page_alloc.phys_alloc();
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

        {
            let phys_alloc = self.page_alloc.phys_alloc_mut();
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
        //   main.c:480      __minix_init()              → DEFERRED (kernel IPC vectors, minix-sys)
        //   main.c:485-495  mem_add_total_pages()       → account_boot_memory()
        //   main.c:497-520  boot procs (exec_bootproc)  → init_boot_procs() (exec DEFERRED)
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

        // Phase 2c: boot process slots — main.c:497-520 (exec_bootproc DEFERRED).
        self.init_boot_procs();

        // Phase 2d: VM instance mark — main.c:577-579.
        self.mark_vm_instance();

        // Phase 3: PageFrames after total_pages is known.
        let total_phys = PhysBytes(self.page_alloc.total_pages() as u64 * crate::region::PAGE_SIZE);
        self.page_frames = Some(PageFrames::new(total_phys));

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
            crate::global::init(self.page_alloc.total_pages());
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
        // SAFETY: Called before any `init_page_table()` call (process table
        // setup happens in `init_proc_table()` after this). Single-threaded
        // VM ensures no concurrent reader of `KERNEL_LAYOUT`.
        unsafe {
            crate::global::set_kernel_layout(minix_types::KernelLayout::new(
                0xFFFF_FFFF_8000_0000, // kernel_text_vbase
                0x100_0000,            // kernel_text_pbase (16 MiB)
                8,                     // kernel_text_pages
                8,                     // kernel_data_pages
                0xFFFF_8000_0000_0000, // dm_vbase (KERNEL_DIRECT_MAP_BASE)
                4,                     // dm_pages (sentinel only)
            ));
        }
    }

    /// init_proc(VM_PROC_NR) — main.c:262-283, called at main.c:474.
    fn init_vm_slot(&self) {
        let table = VmProcTable::get_global();
        if let Some(ip) = self.boot_procs.iter().find(|ip| ip.proc_nr == VM_PROC_NR) {
            Self::init_proc(table, *ip);
        }
    }

    /// Boot process slots — main.c:497-520.
    ///
    /// C also runs `exec_bootproc()` + `free_mem()` per boot process here;
    /// both are DEFERRED (ELF loading / pagetable bind / sys_exec depend on
    /// the kernel IPC core, minix-sys). Slot population happens now so the
    /// process table reflects the boot image before the main loop starts.
    fn init_boot_procs(&self) {
        let table = VmProcTable::get_global();
        for &ip in &self.boot_procs {
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
            Self::init_proc(table, ip);
        }
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

    /// Records one page-allocation failure (pressure accounting, see the
    /// `missing_spares` field docs for the C↔Rust mapping).
    ///
    /// Callers must invoke this when `VmPageAllocator::alloc_*()` returns
    /// `None` so the main loop knows to schedule an `alloc_cycle` on its
    /// next pass. Saturates at `u32::MAX` to avoid wraparound (the loop
    /// only checks `> 0` and clears to 0, so saturation is safe).
    ///
    /// VM is single-threaded (`pub` is fine; no `&mut self` contention).
    pub fn mark_alloc_failure(&mut self) {
        self.missing_spares = self.missing_spares.saturating_add(1);
    }

    /// Returns the current allocation-pressure count (for tests/observability).
    pub fn missing_spares(&self) -> u32 {
        self.missing_spares
    }

    /// Returns the count of failed kernel pagefault handlings (V9-P1-1).
    ///
    /// Every `VmReply::Error` produced while dispatching a `VM_PAGEFAULT`
    /// message increments this counter. Tests assert on it; the audit
    /// channel (`vm_acl_audit` feature) prints each failure.
    pub fn pagefault_errors(&self) -> u64 {
        self.pagefault_errors
    }

    /// Returns the count of IPC messages dropped at the main-loop boundary
    /// (V9-P0-1, [ARCH: A-14]): receive failures + invalid callers.
    pub fn dropped_messages(&self) -> u64 {
        self.dropped_messages
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
    /// (`[ARCH: A-1]`, 06-page-allocator.md §3.3), so the replenishment body
    /// is DEFERRED to 24-page-cache (cache reclaim + retry). Until then the
    /// counter is cleared so the next failure re-arms the hook — the loop
    /// always gets a fresh replenishment attempt per pressure episode.
    fn alloc_cycle(&mut self) {
        debug_assert!(self.missing_spares > 0);
        // C: alloc_mem → cache_freepages(1024) 重试（alloc.c:242-279，main.c:118-119）。
        // plan.md §7.3：补充体 DEFERRED 归 24-page-cache —— 回收页缓存后再清压力计数；
        // 若回收后压力仍在，下一次分配失败会重新武装计数（每压力片段一次回收机会）。
        if let Some(frames) = self.page_frames.as_mut() {
            let _freed = self.page_cache.free_pages(FREE_CACHE_BATCH, frames, &mut self.page_alloc);
        }
        self.missing_spares = 0;
    }

    /// Main event loop. Never returns (C: main.c:113-193).
    ///
    /// Per-iteration work lives in [`Self::run_once`] so tests can drive a
    /// single dispatch→reply round without spawning the infinite loop
    /// (V10-P0-2). The loop owns the two things `run_once` cannot:
    /// the allocation-pressure replenishment hook and the receive-failure
    /// bound that prevents a busy-spin when the transport is broken.
    pub fn run(&mut self) -> ! {
        assert!(self.initialized, "VmServer::run() called before init()");

        let mut consecutive_recv_failures: u32 = 0;
        loop {
            // C: if(missing_spares > 0) alloc_cycle();
            if self.missing_spares > 0 {
                self.alloc_cycle();
            }

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
                self.dropped_messages = self.dropped_messages.saturating_add(1);
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
                self.dropped_messages = self.dropped_messages.saturating_add(1);
                audit_log!("[VM IPC] invalid caller {:?} — message dropped", who_e);
                return RunStep::Handled;
            }
        };

        let action = self.dispatch_on_msg(&msg, &rcv_sts, caller_slot);

        // C: if(result != SUSPEND) { ipc_send(who_e, &msg); }
        //
        // DEFERRED: use `VmReplyForIpc` wrapper that
        // *statically* excludes `VmReply::Suspend`, replacing the prior
        // `unreachable!("Suspend filtered before reply_to_errno")` panic.
        // The `DispatchAction` enum already encodes the three C outcomes
        // (SUSPEND / no-reply / reply-with-payload); at this call site
        // we map `DispatchAction::Reply(reply)` (where `reply` is *any*
        // `VmReply`) into `VmReplyForIpc::new(reply)`. The wrapper
        // constructor returns `None` for `VmReply::Suspend`, which would
        // be a logic bug (we forgot to convert Suspend at dispatch
        // boundary) — we explicitly check that case and panic with a
        // useful error message rather than the previous cryptic
        // `unreachable!()` panic from deep inside `reply_to_errno`.
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
            // V10-P1-2: live-update scaffolding. C: main.c:191 — SUSPEND
            // means "no reply now, resume later" (RS_INIT handshake and
            // rs_update). The only current producer is the RS_INIT
            // handshake (Priority 2 above); the rs_update Suspend path is
            // unreachable until kernel `sys_update` lands — the empty arm
            // is intentional and pinned by
            // `dispatcher::tests::test_dispatch_rs_update_pins_not_implemented`.
            DispatchAction::Suspend => {}
            DispatchAction::NoReply => {}
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

/// A statically-filtered view of [`VmReply`] that excludes [`VmReply::Suspend`].
///
/// `reply_to_errno` and `encode_reply_data` both require a `VmReply` that
/// will actually be encoded into an IPC reply message — i.e. *not* a
/// `Suspend` (which means "no reply now, resume later"). By constructing
/// `VmReplyForIpc` at the boundary, we make this requirement a compile-time
/// property: `VmReplyForIpc::new(VmReply::Suspend)` returns `None`, and the
/// only way to obtain a `VmReplyForIpc` is via `new()` which statically
/// rejects `Suspend`.
///
/// # Why not just `match` everywhere?
///
/// Before this wrapper, `reply_to_errno` carried an `unreachable!()` arm for
/// `VmReply::Suspend`. If a future refactor accidentally routed a
/// `VmReply::Suspend` through `DispatchAction::Reply(...)` (e.g. by adding
/// a new call site in `dispatch_on_msg`), the kernel would panic deep in
/// the reply path with the cryptic message "Suspend filtered before
/// reply_to_errno". The wrapper moves the check to the *only* place where
/// the boundary is crossed, and produces a much more useful diagnostic
/// ("DispatchAction::Reply carries VmReply::Suspend; dispatch_on_msg should
/// translate to DispatchAction::Suspend") with a clear remediation hint.
///
/// # C-Rust parity
///
/// This is purely a Rust-side type-system improvement; the C code uses a
/// `result` integer (SUSPEND vs others) without a typed wrapper. The
/// wrapper exists only to encode the Rust type-level invariant.
#[derive(Debug, Clone)]
struct VmReplyForIpc {
    inner: VmReply,
}

impl VmReplyForIpc {
    /// Construct a `VmReplyForIpc` from a `VmReply`. Returns `None` if
    /// `reply` is `VmReply::Suspend` — caller must use `DispatchAction::Suspend`
    /// for that case instead of `DispatchAction::Reply(...)`.
    fn new(reply: VmReply) -> Option<Self> {
        match reply {
            VmReply::Suspend => None,
            inner => Some(Self { inner }),
        }
    }

    /// Returns a clone of the underlying `VmReply`. By construction this
    /// is never `VmReply::Suspend`. Use this when the caller still needs
    /// to inspect the reply after encoding (e.g. for logging).
    fn payload(&self) -> VmReply {
        self.inner
    }

    /// Consumes the wrapper and returns the owned `VmReply`. By construction
    /// this is never `VmReply::Suspend`.
    fn into_payload(self) -> VmReply {
        self.inner
    }
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

        // Priority 2: RS_INIT (main.c:149-152)
        if m_type == RS_INIT && source == RS_PROC_NR {
            self.rs_handshake()
                .expect("rs_handshake failed");
            return DispatchAction::Suspend;
        }

        // Priority 3: VM_PAGEFAULT (main.c:153-164)
        if m_type == VM_PAGEFAULT {
            debug_assert!(
                rcv_sts.is_from_kernel(),
                "faked VM_PAGEFAULT from {:?}", source
            );
            let reply = self.dispatch_pagefault(msg);
            // V9-P1-1: never silently drop a pagefault failure — the faulting
            // process stays suspended and would otherwise re-fault forever
            // with no observable signal. Count + audit instead (the counter is
            // also surfaced by the `pagefault_errors()` accessor in tests).
            if let VmReply::Error(e) = reply {
                self.pagefault_errors = self.pagefault_errors.saturating_add(1);
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
            if let Some(proc) = table.get_active(caller_slot)
                && proc.acl_check(c as u32).is_err() {
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
    fn rs_handshake(&mut self) -> Result<(), VmError> {
        let table = VmProcTable::get_global();

        // 1. Send RS_INIT, receive rproctab
        // C: sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, 0, rprocpub, ...)
        let rproctab = ipc_call_rs_init()
            .map_err(|_| VmError::InternalError)?;

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

    /// VFS transid dispatch — extracts transid, strips it, calls procctl.
    /// C (main.c:131-141): TRNS_GET_ID → TRNS_DEL_ID → do_procctl
    ///
    /// # Current state (2026-06-14 procctl protocol partial fix)
    ///
    /// The full C path is `TRNS_GET_ID → TRNS_DEL_ID → do_procctl`. `do_procctl`
    /// itself is a multi-call dispatch (process control IPC), which is currently
    /// DEFERRED on the procctl protocol design (follow-up).
    ///
    /// What this function does today:
    /// 1. Validate parameters (`clean_type` in known range, `transid` non-zero)
    /// 2. Record the transid in a tracing log so the call site can be observed
    /// 3. Return a structured `VmError::NotImplemented` so callers get a
    ///    meaningful error code (not a panic or `Ok(())` silent no-op)
    ///
    /// This is strictly an improvement over the previous stub which silently
    /// discarded all parameters with `let _ = (...)`.
    fn handle_vfs_transid(
        &mut self,
        clean_type: u32,
        transid: i32,
        msg: &Message,
    ) -> VmReply {
        // C: main.c:141-148
        //   transid = TRNS_GET_ID(msg.m_type);
        //   if(msg.m_source == VFS_PROC_NR && IS_VFS_FS_TRANSID(transid)) {
        //       msg.m_type = TRNS_DEL_ID(msg.m_type);
        //       result = do_procctl(&msg, transid);
        //   }
        //
        // The clean_type (after TRNS_DEL_ID) is the actual VM request number.
        // In Minix3, only VM_PROCCTL is routed through the VFS transid path.
        // We validate clean_type and transid, then delegate to dispatch_procctl.

        // Validate clean_type: must be VM_PROCCTL.
        // C: only do_procctl is called in the VFS transid branch.
        if clean_type != VM_PROCCTL {
            return VmReply::Error(VmError::InternalError);
        }

        // A zero transid means no transaction ID was attached.
        // C: TRNS_GET_ID returns 0 when no transid; the assert
        // `!IS_VFS_FS_TRANSID(transid)` in main.c:135 guarantees
        // that a valid transid is non-zero in this branch.
        if transid == 0 {
            return VmReply::Error(VmError::InvalidProcess);
        }

        // Decode the procctl request from the message.
        // C: do_procctl reads VMPCTL_PARAM, VMPCTL_WHO, VMPCTL_M1,
        //    VMPCTL_LEN, VMPCTL_FLAGS from the message.
        // Wire layout is the C m9 layout (param@16/who@20/m1@24/len@28/
        // flags@32); `decode_message` reads the dedicated overlay.
        let request = VmProcctlIn::decode_message(msg);

        // The caller is always VFS in this path.
        // C: main.c:143 — msg.m_source == VFS_PROC_NR is the gate.
        let caller = VFS_PROC_NR;

        let table = VmProcTable::get_global();
        let frames = match self.page_frames.as_mut() {
            Some(f) => f,
            None => return VmReply::Error(VmError::InternalError),
        };

        MessageDispatcher::dispatch_procctl(
            table, &mut self.page_alloc, frames, caller, request,
        )
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
        let region = match proc.regions_mut().find_mut(fault_addr) {
            Some(r) => r,
            None => return VmReply::Error(VmError::InvalidAddress),
        };
        let (page_alloc, frames, cache, vfs_queue) = self.parts_mut();
        match crate::cow_exec_pf::handle_pagefault(
            proc_endpoint, region, frames, page_alloc,
            fault_addr, request.write, table, cache, vfs_queue,
        ) {
            Ok(_action) => VmReply::Ok,
            Err(_e) => {
                VmReply::Error(VmError::AccessViolation)
            }
        }
    }

    /// Returns mutable references to page_alloc, page_frames, page_cache, and vfs_queue simultaneously.
    /// This avoids double mutable borrow when dispatching VM calls that need multiple components.
    /// Boot-time byte totals for the kernel / VM-self usage queries.
    ///
    /// C: `do_info` VMIW_USAGE with `ep < 0` → `get_usage_info_kernel()`
    /// (region.c:1357-1364): `kernel_allocated_bytes + _dynamic`;
    /// `ep == VM_PROC_NR` → `get_usage_info_vm()` (region.c:1366-1373):
    /// `vm_allocated_bytes + get_vm_self_pages() * VM_PAGE_SIZE`.
    ///
    /// `get_vm_self_pages()` (pagetable.c:1500) is carried by
    /// `VmPageAllocator::self_page_count()` in minix-rs: the Direct Map
    /// ([ARCH: A-1], 06-page-allocator.md §3.3) structurally eliminates
    /// VM's separate self-mapping page accounting, so the allocator's
    /// live allocation count is the direct analog.
    pub(crate) fn usage_sources(&self) -> crate::query::UsageSources {
        crate::query::UsageSources {
            kernel_bytes: self.kernel_allocated.static_bytes
                .saturating_add(self.kernel_allocated.dynamic_bytes),
            vm_self_bytes: self.vm_allocated_bytes
                .saturating_add(
                    (self.page_alloc.self_page_count() as u64)
                        * crate::region::page_state::PAGE_SIZE,
                ),
        }
    }

    pub(crate) fn parts_mut(
        &mut self,
    ) -> (&mut VmPageAllocator, &mut PageFrames, &mut PageCache, &mut VfsRequestQueue) {
        let page_alloc = &mut self.page_alloc;
        let page_frames = self.page_frames.as_mut().expect("page_frames not initialized");
        let page_cache = &mut self.page_cache;
        let vfs_queue = &mut self.vfs_queue;
        (page_alloc, page_frames, page_cache, vfs_queue)
    }

    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only accessor
    pub(crate) fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Drop for VmServer {
    fn drop(&mut self) {
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

/// Strip the transaction ID from a VFS transid-encoded message type,
/// returning the underlying call number.
///
/// C: `TRNS_DEL_ID(t)` in `minix/vfsif.h:81`.
/// ```c
/// #define TRNS_DEL_ID(t)  ((short)((t) >> 16))
/// ```
fn transid_strip(m_type: u32) -> u32 {
    // C casts to short (i16) which sign-extends. The result is the
    // actual VM request number (e.g. VM_PROCCTL).
    ((m_type >> 16) as i16) as u32
}

/// VFS transaction base constant.
/// C: `minix/com.h:909` — `#define VFS_TRANSACTION_BASE 0xB00`
const VFS_TRANSACTION_BASE: u32 = 0xB00;

fn ipc_call_rs_init() -> Result<RprocTab, ()> {
    // TODO: expand the stub into a documented
    // three-stage contract so the deferred path has explicit semantics.
    //
    // The C source (proto.h + table.c) does this in three steps:
    //
    //   1. Build a `mess_rs_init` message (request type RS_INIT=0x714,
    //      sender = VM endpoint, no payload) and send it to RS.
    //   2. Receive a `mess_rs_init_reply` from RS that contains a
    //      pointer to a `struct rprocinfo` describing the live
    //      replicated services and the grant table metadata.
    //   3. Copy that table out of RS's address space via
    //      `sys_safecopyfrom(SELF, ...)` and decode it into our
    //      `RprocTab`.
    //
    // The Rust rewrite is blocked on three DEFERRED dependencies:
    //
    //   - IpcTransport (this crate, `ipc/transport.rs:150`/`:164` is still
    //     `unimplemented!()` for KernelIpcTransport; this depends on
    //     kernel IPC primitive).
    //   - sys_safecopyfrom syscall shim (depends on the kernel-side
    //     SYS_SAFECOPYFROM dispatch and the SAFECOPY grant table;
    //     see kernel `dispatch_safecopy` — DEFERRED).
    //   - Endpoint ↔ ProcNr conversion on the kernel side (depends
    //     on `ProcessTable::endpoint_to_nr()` being globally
    //     available; partial fix in place, full wiring pending).
    //
    // Until all three land, returning `Ok(RprocTab::empty())` keeps
    // `rs_handshake()` non-panicking and lets VM continue to service
    // requests from PM/SYS without an RS handshake. The `Err(())`
    // variant is reserved for "RS replied but the reply was malformed" —
    // currently unreachable but kept for symmetry with the future
    // implementation.
    Ok(RprocTab::empty())
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
// (VM_RQ_BASE + 48), so relative indices are 0..=48.
const NR_VM_CALLS: usize = 49;

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

/// Convert VmReply to raw errno for IPC reply. C: result != SUSPEND branch.
///
/// All variants are explicitly listed — adding a new `VmReply` variant
/// will produce a compile error here, forcing the author to decide the
/// correct errno value.
///
/// # DEFERRED
///
/// Previously contained an `unreachable!("Suspend filtered before
/// reply_to_errno")` arm reachable from any code path that called
/// `reply_to_errno` with `VmReply::Suspend`. The fix introduces
/// [`VmReplyForIpc`], a typed wrapper whose `new()` constructor statically
/// rejects `VmReply::Suspend` (returns `None`), so this function never
/// receives `Suspend` *through the wrapper path*. The `unreachable!()`
/// arm is retained as a defense-in-depth runtime assertion: if a future
/// refactor calls this function directly (bypassing the wrapper) with
/// `VmReply::Suspend`, we panic immediately rather than silently emitting
/// a bogus errno. This is the Rust idiom "make illegal states
/// unrepresentable where possible, but keep assertions as a backstop".
fn reply_to_errno(reply: VmReply) -> i32 {
    match reply {
        VmReply::Ok => 0,
        VmReply::Error(e) => e.to_errno(),
        // All success replies return 0 (OK) to the caller.
        VmReply::Fork(_) => 0,
        VmReply::Brk(_) => 0,
        VmReply::Mmap(_) => 0,
        VmReply::MapPhys(_) => 0,
        VmReply::Exit => 0,
        VmReply::Willexit => 0,
        VmReply::Munmap => 0,
        VmReply::ExecNewmem(_) => 0,
        VmReply::MapCache { .. } => 0,
        VmReply::VfsMmap(_) => 0,
        VmReply::GetPhys { .. } => 0,
        VmReply::GetRefcount { .. } => 0,
        VmReply::InfoStats { .. } => 0,
        VmReply::InfoUsage { .. } => 0,
        VmReply::InfoRegion { .. } => 0,
        VmReply::Getrusage { .. } => 0,
        VmReply::RsMemctlAddrLen { .. } => 0,
        // Compile-time guarantee: VmReplyForIpc::new() returns None for
        // VmReply::Suspend, so this function never receives it via the
        // wrapper path. If you see this panic, someone bypassed the
        // wrapper — fix the caller, do not silence this assertion.
        VmReply::Suspend => unreachable!(
            "VmReply::Suspend reached reply_to_errno; \
             caller must wrap in VmReplyForIpc or use DispatchAction::Suspend"
        ),
    }
}

/// Encode VmReply per-service output data into the reply message fields.
///
/// All variants are explicitly listed — adding a new `VmReply` variant
/// will produce a compile error here, forcing the author to encode the
/// output data correctly.
///
/// # Defense-in-depth
///
/// The `VmReply::Suspend` arm is unreachable in practice because the
/// [`VmReplyForIpc`] wrapper statically filters it out at the call site
/// (the only call site passes `reply_for_ipc.into_payload()`). However,
/// this function accepts `VmReply` directly, so a future refactor that
/// bypasses the wrapper could pass `Suspend` here. The `unreachable!()`
/// assertion catches that bug immediately, consistent with `reply_to_errno`.
fn encode_reply_data(reply: VmReply, msg: &mut Message) {
    // SAFETY: All VM replies use the M1 message format.
    let m1 = unsafe { &mut msg.m_u.m_m1 };
    match reply {
        VmReply::Fork(out) => out.encode(m1),
        VmReply::Brk(out) => out.encode(m1),
        VmReply::Mmap(out) => out.encode(m1),
        VmReply::MapPhys(out) => out.encode(m1),
        VmReply::ExecNewmem(out) => out.encode(m1),
        VmReply::MapCache { addr } => {
            // C: msg->m_vmmcp_reply.addr = vr->vaddr (mem_cache.c:170);
            // libminixfs reads it back in vm_map_cacheblock (libsys/vm_cache.c:47-54).
            // SAFETY: cache replies use the m_vmmcp_reply format.
            let reply = unsafe { &mut msg.m_u.m_vmmcp_reply };
            reply.addr = addr.0 as u32;
        }
        VmReply::VfsMmap(out) => out.encode(m1),
        VmReply::GetPhys { phys_addr } => { m1.m1p1 = phys_addr.0; }
        VmReply::GetRefcount { count } => { m1.m1i1 = count as i32; }
        VmReply::InfoStats { page_size, total_pages, free_pages, largest_contiguous, cached_pages, .. } => {
            // C: struct vm_stats_info (vm.h:39-44) — pagesize/total/free/
            // largest/cached. M1 slots: p1=pagesize, i1=total, i2=free,
            // i3=largest, p2=cached (u64 page count; no integer slots left).
            // `dropped_messages`/`pagefault_errors` (V10-P2-4) are a
            // minix-rs extension with no C wire slot — dropped here, like
            // InfoUsage's minflt/majflt (see below).
            m1.m1p1 = page_size;
            m1.m1i1 = total_pages as i32;
            m1.m1i2 = free_pages as i32;
            m1.m1i3 = largest_contiguous as i32;
            m1.m1p2 = cached_pages;
        }
        VmReply::InfoUsage { total, common, shared, virtual_total, mvirtual, max_rss_kb, minor_faults, major_faults } => {
            // Minix3 C uses sys_datacopy to copy a `struct vm_usage_info`
            // (5 VirBytes fields + 3 u64 fields) into the caller's address
            // space (utility.c — do_info → get_usage_info).
            // Rust M1 layout has 3 pointer slots (m1p1..m1p3) and 3 integer
            // slots (m1i1..m1i3). Encode the 5 VirBytes fields: 3 in pointer
            // slots, 2 as page counts (saturated i32) in integer slots, and
            // vui_maxrss (KB) in the last integer slot.
            //
            // Field mapping (aligned with C's struct vm_usage_info):
            //   m1p1 = vui_total, m1p2 = vui_common, m1p3 = vui_shared
            //   m1i1 = vui_virtual (page count), m1i2 = vui_mvirtual (page count)
            //   m1i3 = vui_maxrss (KB, saturated)
            // vui_minflt / vui_majflt have no remaining M1 slot — DEFERRED
            // to the sys_datacopy path (VMI-3 follow-up; MIB gets them via
            // the full reply once transport lands).
            m1.m1p1 = total.0;
            m1.m1p2 = common.0;
            m1.m1p3 = shared.0;
            // SAFETY: `as i32` is a truncating cast. Region sizes in bytes fit
            // in u32 when divided by PAGE_SIZE (PAGE_SIZE = 4096 ⇒ 1 TiB of
            // memory ≈ 2^28 pages, well within i32::MAX = 2^31). Documented
            // for reviewer (see review-patterns-skill §模式19 — `as` 截断
            // 必须有 SAFETY 注释).
            let pages = |bytes: u64| -> i32 {
                (bytes / crate::region::page_state::PAGE_SIZE).min(i32::MAX as u64) as i32
            };
            m1.m1i1 = pages(virtual_total.0);
            m1.m1i2 = pages(mvirtual.0);
            // SAFETY: `as i32` saturates — maxrss in KB is bounded by
            // total physical memory / 1024, far below i32::MAX.
            m1.m1i3 = max_rss_kb.min(i32::MAX as u64) as i32;
            let _ = minor_faults;
            let _ = major_faults;
        }
        VmReply::RsMemctlAddrLen { addr, len } => {
            // C message layout (com.h:738-741): VM_RS_CTL_ADDR == m2_p1,
            // VM_RS_CTL_LEN == m2_i3. In C's `mess` union, m2_p1 is at
            // offset 40 while m1_p1/m2_l1 are at offset 24 — the minix-rs
            // MessageM1/M2 layouts are offset-shifted vs C (see
            // minix-types message.rs), so within this model addr→m1p1 and
            // len→m1i3 alias the slots the request decode reads back
            // (m2l1/m2i3). The C wire offsets differ and need a dedicated
            // minix-types overlay when a real C RS is on the wire (A-8:
            // transport DEFERRED). (FIX 25-R2: len was written to m1i1,
            // i.e. the request's endpoint slot — vm_memctl would read a
            // stale len.)
            m1.m1p1 = addr.0;
            m1.m1i3 = len as i32;
        }
        VmReply::InfoRegion { regions, count, next } => {
            // Minix3 C uses `sys_datacopy(VM_PROC_NR, regions_addr,
            // caller, call_addr, count*sizeof(vm_region_info))` to copy
            // the region array (utility.c). M1 layout has only 1 pointer
            // and 3 integer slots — insufficient to ship the array inline.
            //
            // FIX (VMI-2): Previously `let _ = regions;` discarded the
            // entire region list, leaving PM unable to enumerate regions
            // (m1.m1i1=count, m1.m1i2=next but no array payload). Encoding
            // is unchanged for now (count + next in integer slots), but we
            // expose the source length in m1.m1p1 so the caller can detect
            // "VM stub returned N regions but no sys_datacopy happened" vs
            // "VM really has 0 regions". When `IpcTransport::send` lands,
            // replace this with a real sys_datacopy call.
            let len_u32 = u32::try_from(regions.len()).unwrap_or(u32::MAX);
            m1.m1p1 = u64::from(len_u32); // sentinel: source-side length
            m1.m1i1 = count as i32;
            // SAFETY: `as i32` truncates the vaddr cursor. Region addresses
            // live in the low 4 GiB user range (VM_MMAPTOP = 0x80000000),
            // so the cursor fits — documented per §模式19.
            m1.m1i2 = next.0 as i32;
            // m1.m1i3 deliberately left as 0 — reserved for caller-side
            // buffer capacity once sys_datacopy is wired.
        }
        VmReply::Getrusage { max_rss_kb, minor_faults, major_faults } => {
            // Minix3 C uses sys_datacopy to copy a `struct rusage`
            // (≥15 fields: utime, stime, maxrss, ixrss, idrss, isrss,
            // minflt, majflt, nswap, inblock, oublock, msgsnd, msgrcv,
            // nvcsw, nivcsw) into the caller's address space (utility.c).
            // M1 layout has 3 pointer slots + 3 integer slots — pick the
            // 4 most diagnostic fields. The remaining 11 are DEFERRED to
            // the sys_datacopy path (VMI-3 follow-up).
            //
            // FIX (VMI-3): Previously only 3 fields were encoded
            // (max_rss/min_flt/maj_flt) with `m1p2..m1p3`/`m1i3` unused.
            // Encoding now uses remaining slots: m1p2=minor_faults, m1p3=
            // major_faults (both fit in u64 for any realistic process),
            // freeing m1i1/m1i2 for future in_use_time fields.
            m1.m1p1 = max_rss_kb;
            // SAFETY: `as i32` truncates fault counters. i32::MAX = 2^31 ≈
            // 2.1B faults per measurement window; saturation at i32::MAX
            // signals "very high fault rate" rather than overflow in
            // practice. Documented per review-patterns-skill §模式19.
            let faults_to_i32 = |n: u64| -> i32 { n.min(i32::MAX as u64) as i32 };
            m1.m1i1 = faults_to_i32(minor_faults);
            m1.m1i2 = faults_to_i32(major_faults);
            // m1.m1i3 reserved for future ru_inblock (block-input ops).
        }
        // Variants with no output data to encode
        VmReply::Ok | VmReply::Error(_)
        | VmReply::Exit | VmReply::Willexit | VmReply::Munmap => {}
        // Defense-in-depth: Suspend is statically excluded by VmReplyForIpc
        // at the only call site. If this fires, someone bypassed the wrapper.
        VmReply::Suspend => unreachable!(
            "VmReply::Suspend reached encode_reply_data; \
             caller must wrap in VmReplyForIpc or use DispatchAction::Suspend"
        ),
    }
}

impl VmServer {
    // V10-P2-1: the `handle_*` wrappers were superseded by
    // `dispatch_on_msg` → `MessageDispatcher::dispatch_*` (which routes
    // directly); they had zero callers and are removed.

    pub fn has_pending_vfs_requests(&self) -> bool {
        !self.vfs_queue.is_empty()
    }

    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only accessors
    pub(crate) fn page_cache(&self) -> &PageCache {
        &self.page_cache
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn vfs_queue(&self) -> &VfsRequestQueue {
        &self.vfs_queue
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn vfs_queue_mut(&mut self) -> &mut VfsRequestQueue {
        &mut self.vfs_queue
    }
}

// V10-P2-1: `handle_fork`/`handle_brk`/`handle_exit` are thin test
// wrappers over `MessageDispatcher` (the main loop routes via
// `dispatch_on_msg` → `dispatch_by_number` directly). They are kept under
// `cfg(test)` for the server-level dispatch tests.
#[cfg(test)]
impl VmServer {
    pub(crate) fn handle_fork(&mut self, req: VmForkIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_fork(table, &mut self.page_alloc, frames, req)
    }

    pub(crate) fn handle_brk(&mut self, req: VmBrkIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_brk(table, &mut self.page_alloc, frames, req)
    }

    pub(crate) fn handle_exit(&mut self, req: VmExitIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_exit(table, &mut self.page_alloc, frames, req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direct_map::tests::with_custom_mock_base;
    use crate::boot::{BootModule, KernelAllocated};

    const TEST_TOTAL_PAGES: usize = 256;

    /// One-time mock physical memory setup for all VM server tests.
    /// Uses a leaked static buffer to avoid parallel test races on mock_vm_base.
    static TEST_PHYS_INIT: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    static TEST_MOCK_BASE: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

    fn ensure_mock_phys_init() {
        use core::sync::atomic::Ordering;
        if TEST_PHYS_INIT.swap(true, Ordering::SeqCst) {
            return; // already initialized
        }
        let mock_phys_size = TEST_TOTAL_PAGES * CLICK_SIZE + CLICK_SIZE;
        let mock_phys: alloc::vec::Vec<u8> = alloc::vec![0u8; mock_phys_size];
        let mock_phys_leaked = alloc::boxed::Box::leak(mock_phys.into_boxed_slice());

        let raw_base = mock_phys_leaked.as_ptr() as usize;
        let aligned_base = (raw_base + CLICK_SIZE - 1) & !(CLICK_SIZE - 1);
        minix_arch::direct_map::set_mock_vm_base(aligned_base as u64);
        TEST_MOCK_BASE.store(aligned_base as u64, Ordering::SeqCst);
    }

    /// Run a test with the vm_server mock base set correctly.
    /// Uses the global MOCK_BASE_MUTEX to prevent parallel test interference.
    fn with_test_mock_base<F: FnOnce()>(f: F) {
        // Ensure mock physical memory is initialized before reading TEST_MOCK_BASE.
        ensure_mock_phys_init();
        let base = TEST_MOCK_BASE.load(core::sync::atomic::Ordering::SeqCst);
        with_custom_mock_base(base, f);
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
    fn test_missing_spares_pressure_counter() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            assert_eq!(server.missing_spares(), 0);

            // Allocation failures arm the pressure counter; it saturates
            // (no wraparound) and drives the main-loop replenishment hook.
            server.mark_alloc_failure();
            server.mark_alloc_failure();
            assert_eq!(server.missing_spares(), 2);

            // alloc_cycle() clears the counter so the next failure re-arms
            // the hook — observable contract: > 0 → next loop pass
            // re-attempts replenishment (body DEFERRED to 24-page-cache).
            server.alloc_cycle();
            assert_eq!(server.missing_spares(), 0);
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

            let reply = server.handle_fork(request);
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

            let reply = server.handle_fork(request);
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

            let reply = server.handle_brk(request);
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

            let reply = server.handle_exit(request);
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

            // clean_type != VM_PROCCTL → InternalError
            let msg = Message::default();
            let reply = server.handle_vfs_transid(0x600, 1, &msg);
            assert!(matches!(reply, VmReply::Error(VmError::InternalError)));
        });
    }

    #[test]
    fn test_handle_vfs_transid_zero_transid() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            // transid == 0 → InvalidProcess
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

    #[test]
    fn test_encode_reply_rs_memctl_addr_len_slots() {
        // 25-R2 regression: len must be written to m1i3 (the VM_RS_CTL_LEN
        // slot), not m1i1 (the VM_RS_CTL_ENDPT slot). C: com.h:746-747 —
        // VM_RS_CTL_ADDR=m2_p1, VM_RS_CTL_LEN=m2_i3; vm_memctl reads both
        // back after the call.
        let mut msg = Message::default();
        encode_reply_data(
            VmReply::RsMemctlAddrLen {
                addr: VirBytes(0x1_2345_6000),
                len: 0x3000,
            },
            &mut msg,
        );
        let m1 = unsafe { &msg.m_u.m_m1 };
        assert_eq!(m1.m1p1, 0x1_2345_6000);
        assert_eq!(m1.m1i3, 0x3000);
        // The endpoint slot must not be clobbered by the len write (25-R2).
        assert_eq!(m1.m1i1, 0);
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
}
