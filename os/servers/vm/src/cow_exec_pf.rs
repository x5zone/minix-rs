//! Copy-on-Write and page fault handling (PFN index model).
//!
//! Uses PageFrames + PageSlot for CoW resolution and page fault dispatch.

use minix_types::{Endpoint, VirBytes};
use crate::region::{VirRegion, PageFrames, PageSlot, PfnAllocator, PAGE_SIZE};
use crate::memtype::{MemType, PagefaultResult, MemTypeError, MEM_TYPE_ANON};
use crate::vmproc::VmProcTable;
use crate::page_cache::PageCache;
use crate::pagetable::{PageFlags, PageTable, PageTableError, Paging};
use crate::vfs_queue::{VfsQueueError, VfsReply, VfsRequest, VfsRequestQueue, VfsRequestState, VfsRequestType};
use crate::fdref::FdRefTable;
use crate::region::VrParam;
use crate::phys_mem::PageAllocFlags;
#[cfg(not(test))]
use crate::direct_map::vm_phys_to_virt;
#[cfg(not(test))]
use crate::phys_mem::AlignedPhysBytes;

/// VM page fault handler entry point.
///
/// Dispatches to the region's `MemType::ev_pagefault`, then acts on the
/// returned `PagefaultResult`: allocate a new page, resolve CoW, or report
/// an access violation.
#[allow(clippy::too_many_arguments)] // V10-P2-1 (DEFERRED): fold into a PagefaultCtx struct
pub(crate) fn handle_pagefault(
    proc_endpoint: Endpoint,
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    fault_addr: VirBytes,
    write: bool,
    table: &VmProcTable,
    cache: &mut PageCache,
    vfs_queue: &mut VfsRequestQueue,
    pt: &mut PageTable,
) -> Result<PagefaultAction, CowError> {
    let offset = VirBytes(fault_addr.0 - region.vaddr.0);

    // NK4-C 第 32 轮取证探针（task1-close 裁决删除）：RS（ep2）缺页
    // 进入时打印 VM 句柄里该进程页表根的物理地址。第 31 轮真机对照：
    // 内核侧 48 条 RS 故障的层级 dump 全部 lvl1=0（RS 自己的根里 PTE
    // 不在），而 VM 写入回读全程静默（VM 认为写成功）。本轮专打
    // asynsend 首指令页（0x203bf0，refault 主角）——vmpt2 全量版被
    // 启动早期 32 次填充耗尽额度，后段 refault 没采到（c32a 教训）。
    // 内核侧对照锚点：sa0-0x2 root=0x35fd000。
    #[cfg(not(test))]
    if proc_endpoint.0 == 2 && fault_addr.0 == 0x203bf0 {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static P2N: AtomicUsize = AtomicUsize::new(0);
        if P2N.fetch_add(1, AtomicOrd::Relaxed) < 48 {
            use minix_arch::paging::Paging as _;
            crate::bootmark::mark(&alloc::format!(
                "nk4a: vmpt2bf off={:#x} ptroot={:#x}\n",
                offset.0,
                pt.root_paddr().0
            ));
        }
    }

    let memtype = region.def_memtype
        .ok_or(CowError::NoMemType)?;

    let result = memtype.ev_pagefault(proc_endpoint, region, frames, offset, write, table, alloc, cache)?;

    match result {
        PagefaultResult::Handled => {
            // Memtype resolved the fault itself (e.g. a cache hit linked the
            // PFN into the slot) — make the hardware PTE agree before the
            // kernel resumes the faulting instruction.
            sync_slot_pte(region, frames, offset, pt).map_err(CowError::from)?;
            Ok(PagefaultAction::Handled)
        }
        PagefaultResult::NeedNewPage => {
            alloc_and_map(region, frames, alloc, offset, memtype, pt)?;
            Ok(PagefaultAction::MappedNewPage)
        }
        PagefaultResult::NeedCow => {
            cow_resolve(region, frames, alloc, offset, pt)?;
            Ok(PagefaultAction::CowResolved)
        }
        PagefaultResult::NeedVfsIo => {
            enqueue_fdio(proc_endpoint, region, offset, write, vfs_queue)
        }
        PagefaultResult::AccessViolation => {
            Ok(PagefaultAction::AccessViolation)
        }
    }
}

/// Make the hardware PTE for `offset` agree with the VM bookkeeping slot.
///
/// C 对应: `map_pf` 尾部的 `pt_writemap`（region.c/pagetable.c:784，CoW 写位
/// 翻转走 WMF_WRITEFLAGSONLY）与 `map_ph_writept`——Minix3 的缺页路径由 VM
/// 自己写进程页表。G-V12-8: 本函数之前缺失，`handle_pagefault` 链只更新
/// PageSlot/PageFrames 记账，通电后指令重执行会二次故障（活锁）。
///
/// 三路分派：同帧不同位 → `update_flags`（WMF_WRITEFLAGSONLY）；换帧 →
/// `remap`（WMF_OVERWRITE，单操作替换、无"无映射"窗口）；未映射 → `map`。
/// 写权限判定用 `is_page_writable`（C `pr_writable`，region.c:130-133）。
pub(crate) fn sync_slot_pte(
    region: &VirRegion,
    frames: &PageFrames,
    offset: VirBytes,
    pt: &mut PageTable,
) -> Result<(), CowCoreError> {
    let pfn = region
        .get_slot(offset)
        .and_then(PageSlot::pfn)
        .ok_or(CowCoreError::PageNotMapped)?;
    // C map_pf 页对齐后再 pt_writemap——fault_addr 是指令给出的字节地址，
    // PTE 只吃页界。未对齐地址此前让 pt.map 报 InvalidAddress，缺页填充
    // 整体失败（NK4-A C-3 真机：RS 首指令 0x2246c0 → vm-pf err
    // PageTable(InvalidAddress)，2026-09-22）。
    let vaddr = VirBytes(
        (region.vaddr.0 + offset.0) & !(crate::region::PAGE_SIZE as u64 - 1),
    );
    let paddr = frames.pfn_to_phys(pfn);
    // 续-141 探针（用后即滚）：sync 时 slot pfn 带位 26 的现形。
    #[cfg(not(feature = "mock"))]
    if pfn >= 0xA0000 {
        crate::bootmark::mark(&alloc::format!(
            "nk4a: sync-big pfn={:#x} va={:#x}\n",
            pfn,
            vaddr.0
        ));
    }
    // C i386 的 P|U 天然可执行（无 NX）；x86-64 NXE 下 EXECUTABLE 必须显式
    // 给出，否则用户 text 首次取指即 #PF(err=0x15)（NK4-A C-3 真机
    // 2026-09-22：RS 入口页 PTE=NX，walk=0x5，fetch 拒绝）。B41（§1.74/
    // §1.75）修正：可执行性不再从“非可写”间接推断（那会把 exec 装载的
    // PROT_RWX 文本因“可写”误判 NX → 取指恒 #PF 活锁），而是直接取 region
    // 承接的 PROT_EXEC（`is_executable`）。语义＝“caller 请求 EXEC 才可执
    // 行”：未请求 EXEC 的映射（如裸 mmap 数据/栈）保持 NX；但 exec 装载腿
    // （`exec_worker.rs` 每段 + 栈均 PROT_RWX，C 忠实）会产出 RWX，与
    // i386 C 无-NX 等价，**非严格用户页 W^X**（fix27e [ARCH] 的 W^X 作
    // 用于内核 identity window，不约束此 C 忠实路径）；若要恢复严格用户
    // 页 W^X，需专开一单元在 exec_worker 按 ELF p_flags/PT_GNU_STACK 分派 prot。
    let mut flags = if region.is_page_writable(frames, offset) {
        PageFlags::read_write()
    } else {
        PageFlags::read_only()
    };
    if region.is_executable() {
        flags |= PageFlags::EXECUTABLE;
    }
    let result = match pt.query(vaddr) {
        Some((cur_paddr, _)) if cur_paddr == paddr => pt.update_flags(vaddr, flags),
        Some(_) => pt.remap(vaddr, paddr, flags).map(|_| ()),
        None => pt.map(vaddr, paddr, flags),
    };
    // NK4-C 第 20 轮取证探针（task1-close 裁决删除）：写入回读验证。
    // map/remap 报 Ok 但目标 PT 页不在 VM DM 窗口覆盖内时，写会静默
    // 丢失（下次 walk 又见 PTE=0 → 同 VA refault 循环）。回读裁决
    // 「写入未落地」vs「落地后被第三方清写」两个分支。
    #[cfg(not(test))]
    if result.is_ok() {
        let readback = pt.query(vaddr).map(|(pa, _)| pa.0);
        if readback != Some(paddr.0) {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static WBFAIL: AtomicUsize = AtomicUsize::new(0);
            if WBFAIL.fetch_add(1, AtomicOrd::Relaxed) < 16 {
                crate::bootmark::mark(&alloc::format!(
                    "nk4a: pte-wb-FAIL va={:#x} pa={:#x} read={:#?}\n",
                    vaddr.0,
                    paddr.0,
                    readback
                ));
            }
        }
        // 续-165 判别探针（用后即滚）：填页句柄的 root——对账 sas-clear
        // 的 rebind 值（0x9dc38000）；若此处印 0x9dc39000 族=双句柄实锤。
        if vaddr.0 > 0x7fff_0000_0000 {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static FR_N: AtomicUsize = AtomicUsize::new(0);
            if FR_N.fetch_add(1, AtomicOrd::Relaxed) < 3 {
                use minix_arch::paging::Paging as _;
                crate::bootmark::mark(&alloc::format!(
                    "nk4a: fill-root va={:#x} ptroot={:#x} pte_pa={:#x}\n",
                    vaddr.0,
                    pt.root_paddr().0,
                    paddr.0
                ));
            }
        }
    }
    result.map_err(CowCoreError::PageTable)
}

/// Enqueue a `FdIo` VFS request for a file-backed page fault.
///
/// C `mappedfile_pagefault` (mem_file.c:146-153): on a cache miss, issue
/// `vfs_request(VMVFSREQ_FDIO, procfd, vmp, referenced_offset,
/// VM_PAGE_SIZE, cb, NULL, state, statelen)` and return `SUSPEND` with
/// `*io = 1`. The `procfd` is the fd recorded in the region's fdref
/// entry; `referenced_offset` is the file offset of the faulting page.
pub(crate) fn enqueue_fdio(
    proc_endpoint: Endpoint,
    region: &VirRegion,
    offset: VirBytes,
    write: bool,
    vfs_queue: &mut VfsRequestQueue,
) -> Result<PagefaultAction, CowError> {
    // C: procfd = region->param.file.fdref->fd (mem_file.c:93)
    let VrParam::File { fdref_id, offset: file_offset, .. } = &region.param else {
        return Ok(PagefaultAction::AccessViolation);
    };
    let Some(fdref_id) = fdref_id else {
        return Ok(PagefaultAction::AccessViolation);
    };
    let Some(fdref) = FdRefTable::get_global().get(*fdref_id) else {
        return Ok(PagefaultAction::AccessViolation);
    };

    let req = VfsRequest {
        request_type: VfsRequestType::FdIo,
        req_id: 0, // assigned by VfsRequestQueue::request
        caller_endpoint: proc_endpoint,
        fd: fdref.fd,
        offset: file_offset + offset.0,
        length: PAGE_SIZE,
        callback: Some(mappedfile_pf_cont),
        state: Some(VfsRequestState::FdIo {
            region_vaddr: region.vaddr,
            page_offset: offset,
            write,
            caller_endpoint: proc_endpoint,
        }),
        sent: false,
    };
    // C: vfs_request failure → ENOMEM (mem_file.c:151)
    vfs_queue.request(req).map_err(|_| CowError::NoMemory)?;
    Ok(PagefaultAction::Suspended)
}

/// VFS callback for page-fault-initiated `FdIo` requests.
///
/// C `handle_memory_continue` (pagefaults.c:170-190): when the VFS reply
/// carries `VMV_RESULT == OK`, retry the page fault (`handle_memory_step(
/// TRUE /*retry*/)`). The VFS side loaded the page into the VM page cache
/// (`actual_read_write_peek` with PEEKING → `lmfs_get_block_ino` +
/// `vm_map_cacheblock`), so the retry hits the cache and links the page
/// instead of issuing another FDIO. On error, the faulting process is
/// unblocked with the errno.
pub(crate) fn mappedfile_pf_cont(
    server: &mut crate::vm_server::VmServer,
    reply: &VfsReply,
    state: &VfsRequestState,
) -> Result<(), VfsQueueError> {
    let VfsRequestState::FdIo { region_vaddr, page_offset, write, caller_endpoint } = state else {
        return Err(VfsQueueError::NoCallbackState);
    };

    // C: if(m->VMV_RESULT != OK) { handle_memory_final(state, m->VMV_RESULT); return; }
    if reply.result != 0 {
        // Transport note: delivering the errno to the faulting process
        // (handle_memory_final → sys_vmctl / asynsend3) requires the
        // kernel IPC transport, which is not yet wired.
        return Ok(());
    }

    // C: r = handle_memory_step(TRUE) — retry the fault for this page.
    let table = VmProcTable::get_global();
    let slot = table.vm_isokendpt(*caller_endpoint).map_err(|_| VfsQueueError::InvalidFd)?;
    let mut proc = table.get_active(slot).ok_or(VfsQueueError::InvalidFd)?;
    // V9-P1-3 step 1: disjoint &mut fields via VmContext destructuring.
    let crate::vm_server::VmContext { page_alloc, page_frames, page_cache: cache, vfs_queue, .. } =
        &mut server.ctx;
    let frames = page_frames.as_mut().expect("page_frames not initialized");
    // G-V12-8: the retry resolves the fault and must write the PTE too.
    let (regions, pt) = proc.mem_parts_mut();
    let region = regions.find_mut(*region_vaddr)
        .ok_or(VfsQueueError::InvalidFd)?;

    match handle_pagefault(
        *caller_endpoint,
        region,
        frames,
        page_alloc,
        VirBytes(region_vaddr.0 + page_offset.0),
        *write,
        table,
        cache,
        vfs_queue,
        pt,
    ) {
        Ok(PagefaultAction::Suspended) => {
            // Another FDIO was enqueued (repeated miss); the process stays
            // suspended until the next VFS reply.
            Ok(())
        }
        Ok(_) => {
            // Fault resolved (cache hit linked the page, or CoW ran).
            // Unblocking the faulting process (C: handle_memory_final →
            // sys_vmctl(VMCTL_CLEAR_PAGEFAULT)) is transport-gated.
            Ok(())
        }
        Err(_) => {
            // C: handle_memory_final(state, r) with a negative errno —
            // transport-gated reply.
            Ok(())
        }
    }
}

pub(crate) fn alloc_and_map(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
    memtype: &'static dyn MemType,
    pt: &mut PageTable,
) -> Result<u32, CowError> {
    // V11/T30: C alloc_mem semantics — reclaim-retry at the funnel.
    let pfn = crate::alloc_page::alloc_pfn_reclaiming(alloc)
        .map_err(|_| CowError::NoMemory)?;
    // 续-141 探针（用后即滚）：分配值带位 26 的现形（栈页 slot pfn 污染源
    // 候选一＝分配器本身）。
    #[cfg(not(feature = "mock"))]
    if pfn >= 0xA0000 {
        crate::bootmark::mark(&alloc::format!(
            "nk4a: alloc-big pfn={:#x}\n",
            pfn
        ));
    }

    // C `vrallocflags` (region.c:646-658) → `PAF_CLEAR` (alloc.c:452): every
    // region not flagged `VR_UNINITIALIZED` allocates its demand pages with
    // the CLEAR bit, and `alloc_mem` `sys_memset`s the frame to zero. So a
    // fresh anon page must read back as zero — never the residue of a
    // previously-freed physical frame (which can carry a stale stack address,
    // the aarch64 mode① signature). `to_alloc_flags()` is the VrFlags→
    // PageAllocFlags mapping mirroring C `vrallocflags`; we key the clear on
    // its CLEAR bit rather than a hardcoded value.
    if region.flags.to_alloc_flags().contains(PageAllocFlags::CLEAR) {
        clear_phys_page(frames, pfn);
    }

    // NOTE: If map_page could fail in the future, we would need to roll back:
    //   alloc.free_pfn(pfn);
    // Currently map_page is infallible (just sets slot + increments refcount).
    region.map_page(frames, offset, pfn, memtype);
    sync_slot_pte(region, frames, offset, pt).map_err(CowError::from)?;

    Ok(pfn)
}

/// Zero a freshly allocated physical frame before it is mapped into a region
/// that requires cleared pages (C `alloc_mem` + `PAF_CLEAR`, alloc.c:452).
///
/// # SAFETY (target builds)
/// `pfn` refers to a frame that has just been handed out by the allocator and
/// is not yet mapped anywhere. `pfn_to_phys` returns a page-aligned physical
/// address, `vm_phys_to_virt` maps the whole direct region, so `[ptr,
/// ptr + PAGE_SIZE)` is valid for a write of `PAGE_SIZE` bytes and exclusive
/// to this frame.
#[cfg(not(test))]
fn clear_phys_page(frames: &PageFrames, pfn: u32) {
    let phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(pfn).0);
    let ptr = vm_phys_to_virt(phys).0 as *mut u8;
    // SAFETY: see the function-level note — a valid, exclusive direct-map
    // window of PAGE_SIZE bytes over a freshly allocated frame.
    unsafe {
        core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
    }
}

/// Host-test stub: memory content is not observable without a real direct
/// map, so the zeroing is a no-op here (same split as `copy_page_content`).
/// The CLEAR gate in `alloc_and_map` is still exercised; byte-level clearing
/// verifies on target builds only.
#[cfg(test)]
fn clear_phys_page(_frames: &PageFrames, _pfn: u32) {}

pub(crate) fn cow_resolve(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
    pt: &mut PageTable,
) -> Result<u32, CowError> {
    cow_resolve_core(region, frames, alloc, offset, pt).map_err(Into::into)
}

/// Core CoW resolution: allocate a new physical page, copy content from the
/// shared page, unmap the old slot and map the new one as `MEM_TYPE_ANON`.
///
/// If `refcount <= 1` the page is already private and no copy is needed.
pub(crate) fn cow_resolve_core(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
    pt: &mut PageTable,
) -> Result<u32, CowCoreError> {
    let Some(old_pfn) = region.get_slot(offset).and_then(PageSlot::pfn) else {
        return Err(CowCoreError::PageNotMapped);
    };
    let refcount = frames.get(old_pfn)
        .map(|s| s.refcount)
        .unwrap_or(0);

    // V13-P1-1: exclusive ownership alone does not license the write-bit
    // flip — the reuse shortcut is only valid for memtypes where a
    // privately-held page is writable (the anon family). For MappedFile,
    // whose `writable()` is constantly false, C has no refcount shortcut at
    // all: `mappedfile_pagefault` always `cow_block`s, and `cow_block`
    // re-types the page to anon ("After COW we are a normal piece of
    // anonymous memory", mem_file.c:70-71). Taking the shortcut there wrote
    // a read-only PTE and reported success, so the resumed instruction
    // re-faulted on the same address forever. Linux draws the same line in
    // `do_wp_page`: only exclusive PageAnon pages reuse (wp_page_reuse);
    // file-backed private pages always wp_page_copy. Gate the shortcut on
    // `is_page_writable` (C `pr_writable`) and let sole-held file pages fall
    // through to the copy path below.
    if refcount <= 1 && region.is_page_writable(frames, offset) {
        // Already private and the memtype says a private page is writable:
        // C's wp_page_reuse analogue — only the PTE write bit needs flipping
        // (WMF_WRITEFLAGSONLY), the frame stays.
        sync_slot_pte(region, frames, offset, pt)?;
        return Ok(old_pfn);
    }

    // V11/T30: reclaim-retry funnel (C alloc_mem).
    let new_pfn = crate::alloc_page::alloc_pfn_reclaiming(alloc)
        .map_err(|_| CowCoreError::NoMemory)?;

    copy_page_content(frames, old_pfn, new_pfn);

    let pending = region.unmap_page(frames, offset);
    region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON);
    // G-V12-8: point the hardware PTE at the private copy (C pt_writemap
    // WMF_OVERWRITE) — without this the resumed instruction re-faults.
    sync_slot_pte(region, frames, offset, pt)?;

    // If the old page's refcount dropped to 0 and it's not cached, unmap_page
    // returns (pfn, memtype) so the caller can notify the memtype (ev_unreference)
    // and free the physical page. This matches Minix3's pb_unreferenced() path
    // where a shared page's last reference is released.
    if let Some((pfn, mt)) = pending {
        mt.ev_unreference(frames, pfn);
        alloc.free_pfn(pfn);
    }

    #[cfg(debug_assertions)]
    verify_cow_consistency(frames, old_pfn, new_pfn, region, offset);

    Ok(new_pfn)
}

/// Debug-only CoW consistency verification.
///
/// After CoW resolution, asserts that refcounts and slot mappings are correct:
/// - old_pfn: refcount should be decremented (was shared, now private to other owner)
/// - new_pfn: refcount should be 1 (newly allocated, owned by this region)
/// - region's slot at `offset` should point to new_pfn
#[cfg(debug_assertions)]
fn verify_cow_consistency(
    frames: &PageFrames,
    _old_pfn: u32,
    new_pfn: u32,
    region: &VirRegion,
    offset: VirBytes,
) {
    // V13-P1-1: the old page's post-CoW refcount has two legal outcomes and
    // which one applies is unknown here — 0 when this CoW displaced the last
    // reference (sole-held file page, now freed via the unref funnel) or ≥1
    // when other sharers remain (fork shape). The former became reachable
    // with the is_page_writable gate; the old `refcount >= 1` assertion only
    // held for the fork shape and has been dropped. The sharp invariants are
    // the new frame's refcount and the slot's identity below.
    if let Some(new_state) = frames.get(new_pfn) {
        assert_eq!(
            new_state.refcount, 1,
            "new_pfn {} refcount should be 1 after CoW, got {}",
            new_pfn, new_state.refcount
        );
    }

    if let Some(slot) = region.get_slot(offset) {
        assert!(
            slot.is_mapped(),
            "slot at offset {:?} should be mapped after CoW",
            offset
        );
        assert_eq!(
            slot.pfn(), Some(new_pfn),
            "slot at offset {:?} should point to new_pfn {}, got {:?}",
            offset, new_pfn, slot.pfn()
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CowCoreError {
    NoMemory,
    PageNotMapped,
    PageTable(PageTableError),
}

impl From<CowCoreError> for CowError {
    fn from(e: CowCoreError) -> Self {
        match e {
            CowCoreError::NoMemory => CowError::NoMemory,
            CowCoreError::PageNotMapped => CowError::PageNotMapped,
            CowCoreError::PageTable(e) => CowError::PageTable(e),
        }
    }
}

/// Copy `src_pfn` into `dst_pfn` and zero the last `zero_len` bytes of the
/// destination page — the Rust `cow_block(…, clearend)` tail (mem_file.c:
/// 73-79, G-V12-11): a file's final partial page must never expose the
/// cache's stale bytes past EOF, so the private copy is created first and
/// the tail is scrubbed second.
///
/// # SAFETY (target builds)
/// See `copy_page_content`; the zeroing writes `[dst + PAGE_SIZE -
/// zero_len, dst + PAGE_SIZE)` via the direct map. Callers guarantee
/// `zero_len <= PAGE_SIZE`; a zero `zero_len` skips the memset entirely.
#[cfg(not(test))]
pub(crate) fn copy_page_and_zero_tail(
    frames: &PageFrames,
    src_pfn: u32,
    dst_pfn: u32,
    zero_len: u64,
) {
    copy_page_content(frames, src_pfn, dst_pfn);
    if zero_len == 0 {
        return;
    }
    let zero_len = zero_len.min(PAGE_SIZE) as usize;
    let dst_phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(dst_pfn).0);
    let tail_ptr = unsafe { (vm_phys_to_virt(dst_phys).0 as *mut u8).add(PAGE_SIZE as usize - zero_len) };
    // SAFETY: the tail range lies inside the destination page (zero_len is
    // clamped to PAGE_SIZE above) and is exclusive to this fresh private
    // copy — no other reference exists.
    unsafe {
        core::ptr::write_bytes(tail_ptr, 0, zero_len);
    }
}

/// Host-test stub: memory content is not observable without a real Direct
/// Map, so the copy/zero body is a no-op here (same split as
/// `copy_page_content`). Structural outcomes (private PFN, ANON retype)
/// remain assertable; byte-level zeroing verifies on target builds only.
#[cfg(test)]
pub(crate) fn copy_page_and_zero_tail(
    _frames: &PageFrames,
    _src_pfn: u32,
    _dst_pfn: u32,
    _zero_len: u64,
) {
}

#[cfg(not(test))]
fn copy_page_content(frames: &PageFrames, src_pfn: u32, dst_pfn: u32) {
    debug_assert_ne!(src_pfn, dst_pfn, "copy_page_content: src and dst PFN must differ");
    // SAFETY: pfn_to_phys returns a page-aligned physical address (multiple of PAGE_SIZE).
    // AlignedPhysBytes::new_unchecked requires its argument to be page-aligned, which is
    // guaranteed by the PageFrames invariant that all PFNs map to page-aligned addresses.
    let src_phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(src_pfn).0);
    let dst_phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(dst_pfn).0);
    let src_ptr = vm_phys_to_virt(src_phys).0 as *const u8;
    let dst_ptr = vm_phys_to_virt(dst_phys).0 as *mut u8;
    // SAFETY: src_ptr and dst_ptr are valid for reads/writes of PAGE_SIZE bytes.
    // They point to distinct physical pages (src_pfn != dst_pfn guaranteed by caller),
    // so the regions do not overlap. Both pages are mapped and accessible via the
    // direct-mapped region (vm_phys_to_virt).
    unsafe {
        core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, PAGE_SIZE as usize);
    }
}

#[cfg(test)]
fn copy_page_content(_frames: &PageFrames, _src_pfn: u32, _dst_pfn: u32) {
}

/// Resolve CoW for all pages in a region that need it.
///
/// Iterates over every page slot; if `needs_cow` is true, performs
/// `cow_resolve` on that page. Returns the number of pages resolved.
// V10-P2-1 (DEFERRED): fork/exec production paths are not wired; kept for
// the CoW test suite and the future exec-newmem flow.
#[allow(dead_code)]
pub(crate) fn cow_resolve_region(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    pt: &mut PageTable,
) -> Result<usize, CowError> {
    let num_pages = region.physblocks.len();
    let mut resolved = 0;

    for i in 0..num_pages {
        let offset = VirBytes((i as u64) * PAGE_SIZE);
        if region.needs_cow(frames, offset) {
            cow_resolve(region, frames, alloc, offset, pt)?;
            resolved += 1;
        }
    }

    Ok(resolved)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagefaultAction {
    Handled,
    MappedNewPage,
    CowResolved,
    Suspended,
    AccessViolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CowError {
    NoMemory,
    NoMemType,
    PageNotMapped,
    MemType(MemTypeError),
    PageTable(PageTableError),
}

impl From<MemTypeError> for CowError {
    fn from(e: MemTypeError) -> Self {
        Self::MemType(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::PhysBytes;
    use crate::region::VrFlags;
    // Only used by tests; kept out of the module-level import so the
    // no_std production build stays free of unused-import warnings.
    use crate::memtype::MEM_TYPE_MAPPED_FILE;

    use crate::region::PfnAllocError;

    struct TestAlloc { next: u32 }
    impl PfnAllocator for TestAlloc {
        fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
            let pfn = self.next;
            self.next += 1;
            Ok(pfn)
        }
        fn free_pfn(&mut self, _pfn: u32) {}
    }

    fn make_frames(pages: u32) -> PageFrames {
        PageFrames::new(PhysBytes(pages as u64 * PAGE_SIZE))
    }

    #[test]
    fn test_alloc_and_map() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let pfn = alloc_and_map(&mut region, &mut frames, &mut alloc, VirBytes(0x0000), &MEM_TYPE_ANON, &mut pt).unwrap();

        let slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert!(slot.is_mapped());
        assert_eq!(slot.pfn(), Some(pfn));
    }

    #[test]
    fn test_cow_resolve() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);

        frames.get_mut(pfn).unwrap().refcount = 2;

        let new_pfn = cow_resolve(&mut region, &mut frames, &mut alloc, VirBytes(0x0000), &mut pt).unwrap();

        assert_ne!(new_pfn, pfn);
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);
        assert_eq!(frames.get(new_pfn).unwrap().refcount, 1);
    }

    #[test]
    fn test_cow_resolve_no_sharing() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        // V13-P1-1: the reuse shortcut now requires the region to be
        // writable (is_page_writable = VR_WRITABLE && memtype.writable), so
        // the no-sharing case must be modelled on a writable anon region —
        // that is the shape in which C's wp_page_reuse analogue applies.
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::WRITABLE);
        region.def_memtype = Some(&MEM_TYPE_ANON);
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);

        let result_pfn = cow_resolve(&mut region, &mut frames, &mut alloc, VirBytes(0x0000), &mut pt).unwrap();
        assert_eq!(result_pfn, pfn);
    }

    #[test]
    fn test_cow_resolve_region() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn0, &MEM_TYPE_ANON);
        region.map_page(&mut frames, VirBytes(0x1000), pfn1, &MEM_TYPE_ANON);

        frames.get_mut(pfn0).unwrap().refcount = 2;
        frames.get_mut(pfn1).unwrap().refcount = 3;

        let resolved = cow_resolve_region(&mut region, &mut frames, &mut alloc, &mut pt).unwrap();
        assert_eq!(resolved, 2);

        assert_eq!(frames.get(pfn0).unwrap().refcount, 1);
        assert_eq!(frames.get(pfn1).unwrap().refcount, 2);
    }

    #[test]
    fn test_cow_resolve_core_refcount_one_fast_path() {
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        // V13-P1-1: the reuse shortcut requires is_page_writable to hold —
        // VR_WRITABLE on the region and a memtype whose private pages are
        // writable. Anon with refcount 1 is exactly that shape.
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::WRITABLE);
        region.def_memtype = Some(&MEM_TYPE_ANON);
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);

        let result = cow_resolve_core(&mut region, &mut frames, &mut alloc, VirBytes(0x0000), &mut pt).unwrap();
        assert_eq!(result, pfn);
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);
        let slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert_eq!(slot.pfn(), Some(pfn));
    }

    // --- G-V12-8: the fault path must keep the hardware PTE in sync with
    // the bookkeeping slot (C: map_pf → pt_writemap). SimPaging makes the
    // PTE side assertable for the first time.

    /// Region with VR_WRITABLE so `is_page_writable` (C pr_writable) can
    /// return true for a private anon page.
    fn make_writable_anon_region() -> VirRegion {
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::WRITABLE);
        region.def_memtype = Some(&MEM_TYPE_ANON);
        region
    }

    #[test]
    fn test_demand_fault_maps_pte_present() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();
        let mut queue = VfsRequestQueue::new();
        let table = VmProcTable::get_global();
        let mut region = make_writable_anon_region();
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let action = handle_pagefault(
            Endpoint(100), &mut region, &mut frames, &mut alloc,
            VirBytes(0x1000), true, table, &mut cache, &mut queue, &mut pt,
        ).unwrap();
        assert_eq!(action, PagefaultAction::MappedNewPage);

        let pfn = region.get_slot(VirBytes(0x0000)).and_then(PageSlot::pfn).unwrap();
        let (paddr, flags) = pt.query(VirBytes(0x1000))
            .expect("PTE must be mapped after a demand fault");
        assert_eq!(paddr, frames.pfn_to_phys(pfn));
        assert!(flags.contains(PageFlags::PRESENT));
        assert!(flags.contains(PageFlags::WRITABLE), "private writable page must get a RW PTE");
    }

    /// B41 (§1.74/§1.75) 回归：exec 装载每段 PROT_RWX（C 忠实，
    /// `exec_general.c:23-24`），region 需同时携 WRITABLE+EXECUTABLE，
    /// `sync_slot_pte` 必须据此产出可执行 PTE——否则 PROT_RWX 文本因
    /// “可写”被旧逻辑判 NX → 取指恒 #PF 活锁（真机 0x20ef60 现场）。
    /// 同时验证未请求 EXEC 的可写区（堆/栈）仍保持 NX，不破坏 W^X。
    #[test]
    fn test_sync_slot_pte_honors_executable_region_flag() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        // Exec-loaded text: WRITABLE + EXECUTABLE → PTE 必须带 EXECUTABLE。
        let mut text = VirRegion::new(
            VirBytes(0x1000),
            VirBytes(0x4000),
            VrFlags::WRITABLE | VrFlags::EXECUTABLE,
        );
        text.def_memtype = Some(&MEM_TYPE_ANON);
        let tpfn = alloc.alloc_pfn().unwrap();
        text.map_page(&mut frames, VirBytes(0x0000), tpfn, &MEM_TYPE_ANON);
        sync_slot_pte(&text, &frames, VirBytes(0x0000), &mut pt).unwrap();
        let (_, tflags) = pt.query(VirBytes(0x1000)).expect("text PTE mapped");
        assert!(tflags.contains(PageFlags::WRITABLE), "RWX text stays writable (C-faithful)");
        assert!(tflags.contains(PageFlags::EXECUTABLE), "PROT_EXEC region must map executable (B41)");

        // Plain writable data (no EXEC) → 必须保持 NX，不注入可执行。
        let mut data = VirRegion::new(VirBytes(0x2000), VirBytes(0x4000), VrFlags::WRITABLE);
        data.def_memtype = Some(&MEM_TYPE_ANON);
        let dpfn = alloc.alloc_pfn().unwrap();
        data.map_page(&mut frames, VirBytes(0x0000), dpfn, &MEM_TYPE_ANON);
        sync_slot_pte(&data, &frames, VirBytes(0x0000), &mut pt).unwrap();
        let (_, dflags) = pt.query(VirBytes(0x2000)).expect("data PTE mapped");
        assert!(dflags.contains(PageFlags::WRITABLE));
        assert!(!dflags.contains(PageFlags::EXECUTABLE), "non-exec writable page stays NX (W^X)");
    }

    #[test]
    fn test_cow_fault_pte_repoints_to_new_frame() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();
        let mut queue = VfsRequestQueue::new();
        let table = VmProcTable::get_global();
        let mut region = make_writable_anon_region();
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);
        frames.get_mut(pfn).unwrap().refcount = 2;
        // Fork-shaped pre-state: the PTE still maps the SHARED frame read-only.
        let old_paddr = frames.pfn_to_phys(pfn);
        pt.map(VirBytes(0x1000), old_paddr, PageFlags::read_only()).unwrap();

        let action = handle_pagefault(
            Endpoint(100), &mut region, &mut frames, &mut alloc,
            VirBytes(0x1000), true, table, &mut cache, &mut queue, &mut pt,
        ).unwrap();
        assert_eq!(action, PagefaultAction::CowResolved);

        let new_pfn = region.get_slot(VirBytes(0x0000)).and_then(PageSlot::pfn).unwrap();
        assert_ne!(new_pfn, pfn, "CoW must copy to a private frame");
        let (paddr, flags) = pt.query(VirBytes(0x1000))
            .expect("PTE must be mapped after CoW resolution");
        assert_eq!(paddr, frames.pfn_to_phys(new_pfn), "PTE must point at the private copy");
        assert!(flags.contains(PageFlags::WRITABLE));
    }

    #[test]
    fn test_cow_fast_path_flips_pte_writable_without_recopy() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();
        let mut queue = VfsRequestQueue::new();
        let table = VmProcTable::get_global();
        let mut region = make_writable_anon_region();
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);
        // refcount already 1 (sharper exited): C wp_page_reuse analogue —
        // same frame, flags-only flip (WMF_WRITEFLAGSONLY).
        let paddr = frames.pfn_to_phys(pfn);
        pt.map(VirBytes(0x1000), paddr, PageFlags::read_only()).unwrap();

        // refcount==1 + write → `anon_pagefault` verdicts `Handled` ("safely
        // writable without CoW"); the PTE write-bit flip happens in the
        // Handled arm's PTE sync (G-V12-8) — before this fix the stale
        // read-only PTE would re-fault the resumed instruction.
        let action = handle_pagefault(
            Endpoint(100), &mut region, &mut frames, &mut alloc,
            VirBytes(0x1000), true, table, &mut cache, &mut queue, &mut pt,
        ).unwrap();
        assert_eq!(action, PagefaultAction::Handled);

        assert_eq!(region.get_slot(VirBytes(0x0000)).and_then(PageSlot::pfn), Some(pfn));
        let (cur_paddr, flags) = pt.query(VirBytes(0x1000)).unwrap();
        assert_eq!(cur_paddr, paddr, "fast path must keep the frame");
        assert!(flags.contains(PageFlags::WRITABLE), "fast path must flip the write bit");
    }

    #[test]
    fn test_cow_mappedfile_sole_page_copies_and_retypes_to_anon() {
        // V13-P1-1 regression: a sole-held (refcount == 1) MappedFile page in
        // a writable region must take the COPY path on a write fault. C's
        // mappedfile_pagefault has no refcount shortcut and cow_block re-types
        // the page to anon (mem_file.c:70-71, "After COW we are a normal
        // piece of anonymous memory"). The pre-fix fast path kept the frame,
        // wrote the PTE read-only (MappedFile::writable is constantly false)
        // and returned Ok — the resumed instruction re-faulted on the same
        // address forever.
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();
        let mut queue = VfsRequestQueue::new();
        let table = VmProcTable::get_global();
        let mut region = make_file_region(7, 0);
        region.flags |= VrFlags::WRITABLE;
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let old_pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), old_pfn, &MEM_TYPE_MAPPED_FILE);
        pt.map(VirBytes(0x1000), frames.pfn_to_phys(old_pfn), PageFlags::read_only()).unwrap();

        let action = handle_pagefault(
            Endpoint(100), &mut region, &mut frames, &mut alloc,
            VirBytes(0x1000), true, table, &mut cache, &mut queue, &mut pt,
        ).unwrap();
        assert_eq!(action, PagefaultAction::CowResolved);

        let slot = region.get_slot(VirBytes(0x0000)).unwrap();
        let new_pfn = slot.pfn().unwrap();
        assert_ne!(new_pfn, old_pfn, "sole-held file page must still be copied (C cow_block)");
        assert_eq!(
            slot.memtype().map(|m| m.name()),
            Some(MEM_TYPE_ANON.name()),
            "post-CoW the page must be re-typed anonymous (C cow_block)"
        );
        let (paddr, flags) = pt.query(VirBytes(0x1000)).unwrap();
        assert_eq!(paddr, frames.pfn_to_phys(new_pfn), "PTE must point at the private copy");
        assert!(
            flags.contains(PageFlags::WRITABLE),
            "PTE must end writable — the resumed write must not re-fault"
        );
        // The displaced file page left the region for good: its refcount
        // dropped to zero and the frame returned to the allocator funnel.
        assert_eq!(frames.get(old_pfn).map(|s| s.refcount), Some(0));
    }

    fn make_file_region(fdref_id: u32, file_offset: u64) -> VirRegion {
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x2000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_MAPPED_FILE);
        region.param = crate::region::VrParam::File {
            inited: true,
            fdref_id: Some(fdref_id),
            offset: file_offset,
            clearend: 0,
        };
        region
    }

    #[test]
    fn test_handle_pagefault_need_vfs_io_enqueues_fdio() {
        use crate::page_cache::PageCache;
        use crate::vfs_queue::{VfsRequestQueue, VfsRequestType};

        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();
        let mut queue = VfsRequestQueue::new();
        let table = VmProcTable::get_global();

        let fdref_id = crate::fdref::FdRefTable::get_global().create(7, 1, 100);
        let mut region = make_file_region(fdref_id, 0x5000);

        // C mappedfile_pagefault: cache miss → vfs_request(VMVFSREQ_FDIO,
        // procfd, vmp, referenced_offset, VM_PAGE_SIZE, cb, ...) → SUSPEND.
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();
        let action = handle_pagefault(
            Endpoint(100), &mut region, &mut frames, &mut alloc,
            VirBytes(0x1000), false, table, &mut cache, &mut queue, &mut pt,
        ).unwrap();
        assert_eq!(action, PagefaultAction::Suspended);

        let active = queue.test_active_request().expect("FDIO request active");
        assert_eq!(active.request_type, VfsRequestType::FdIo);
        assert_eq!(active.fd, 7);
        assert_eq!(active.offset, 0x5000, "referenced_offset = file offset + page offset");
        assert_eq!(active.length, PAGE_SIZE as u64);
        assert_eq!(
            active.callback.map(|f| f as usize),
            Some(mappedfile_pf_cont as usize)
        );
        let VfsRequestState::FdIo { region_vaddr, page_offset, write, caller_endpoint } =
            active.state.as_ref().unwrap()
        else {
            panic!("expected FdIo state");
        };
        assert_eq!(*region_vaddr, VirBytes(0x1000));
        assert_eq!(*page_offset, VirBytes(0));
        assert!(!write);
        assert_eq!(*caller_endpoint, Endpoint(100));
    }

    #[test]
    fn test_handle_pagefault_retry_cache_hit_no_fdio_loop() {
        use crate::page_cache::PageCache;
        use crate::vfs_queue::{VfsReply, VfsRequestQueue};

        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();
        let mut queue = VfsRequestQueue::new();
        let table = VmProcTable::get_global();

        let fdref_id = crate::fdref::FdRefTable::get_global().create(7, 1, 100);
        let mut region = make_file_region(fdref_id, 0x5000);

        // First fault: cache miss → FDIO request enqueued (page suspended).
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();
        let action = handle_pagefault(
            Endpoint(100), &mut region, &mut frames, &mut alloc,
            VirBytes(0x1000), false, table, &mut cache, &mut queue, &mut pt,
        ).unwrap();
        assert_eq!(action, PagefaultAction::Suspended);

        // The VFS reply consumes the active FDIO request before the retry
        // callback runs (dispatch_vfs_reply → handle_reply → callback).
        let active_id = queue.active_req_id().unwrap();
        let (callback, _reply, state) = queue.handle_reply(VfsReply {
            req_id: active_id,
            result: 0,
            data_phys: None,
            fd: 7,
            dev: 1,
            ino: 100,
            size_pages: 1,
        }).unwrap().expect("FDIO callback");
        assert_eq!(callback as usize, mappedfile_pf_cont as usize);
        assert!(matches!(state, VfsRequestState::FdIo { .. }));

        // VFS reply path populates the page cache (C: actual_read_write_peek
        // PEEKING → lmfs_get_block_ino + vm_map_cacheblock). The retry
        // (handle_memory_continue → handle_memory_step(TRUE)) must now hit
        // the cache and link the page instead of enqueueing another FDIO.
        let cached_pfn = 5;
        // C: VFS reply path populated the cache via vm_map_cacheblock
        // (lmfs_get_block_ino PEEKING + vm_map_cacheblock); the retry
        // lookup is by inode offset.
        cache.addcache(1, 0x5000, Some(100), 0x5000, false, cached_pfn, &mut frames).unwrap();

        let action = handle_pagefault(
            Endpoint(100), &mut region, &mut frames, &mut alloc,
            VirBytes(0x1000), false, table, &mut cache, &mut queue, &mut pt,
        ).unwrap();
        assert_eq!(action, PagefaultAction::Handled);
        let slot = region.get_slot(VirBytes(0)).unwrap();
        assert_eq!(slot.pfn(), Some(cached_pfn));
        assert!(queue.is_empty(), "no second FDIO may be enqueued");
    }
}
