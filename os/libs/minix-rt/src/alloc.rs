//! Memory allocator: break management plus a slab allocator over pages.
//!
//! A C program's heap starts right after the program image: the linker
//! provides the symbol `_end`, an assembly stub publishes it as the initial
//! `_brksize` (`minix3/minix/lib/libc/arch/i386/sys/brksize.S`), and two
//! functions manage the boundary — `brk` moves it to an absolute address
//! through the virtual memory server, `sbrk` moves it relative to the current
//! position (`minix3/minix/lib/libc/sys/brk.c` and `sbrk.c`). The NetBSD
//! allocator on top mixes both strategies: small objects come from a
//! page-described arena grown with `sbrk`, and the memory mapping path
//! serves the allocator's page directory
//! (`minix3/lib/libc/stdlib/malloc.c:317-388` is the `sbrk` growth loop;
//! the `MMAP` requests live at `malloc.c:448` and `malloc.c:559`).
//!
//! This module keeps the two-level shape but replaces both levels with owned
//! Rust types:
//!
//! 1. Break management becomes pure arithmetic ([`request_new_break`]): the
//!    overflow rule and the "only call the kernel when the address actually
//!    changed" rule, testable without any server.
//! 2. The arena becomes [`SlabAllocator`]: fixed size classes served from
//!    single pages with intrusive free lists, large objects served as whole
//!    page runs. Pages arrive through the [`PageSupplier`] trait, so tests
//!    serve them from plain buffers while real binaries chain a static pool
//!    and, later, virtual memory mapping.
//!
//! # Execution model
//!
//! The owned [`SlabAllocator`] needs no synchronization: tests and callers
//! own their instance. The single global instance behind [`global_alloc`]
//! and [`global_free`] assumes a single-threaded user process — the same
//! assumption the rest of this crate's startup path makes. Thread support
//! will revisit this contract when it lands.

use minix_types::Errno;

/// One page holds this many bytes.
///
/// The NetBSD allocator rounds its arena growth to page multiples (see
/// `malloc.c:383`), and the virtual memory server maps whole pages. Four
/// kibibytes is the page granularity on 64-bit Intel hardware.
pub const PAGE_BYTES: usize = 4096;

/// Small-object size classes in bytes.
///
/// Every class is a multiple of eight, so every slot is eight-byte aligned.
/// Objects up to the largest class come from slabs; anything bigger becomes
/// a whole page run. Nine classes keep internal waste below a factor of two
/// for every size while keeping the class search trivial.
pub const SIZE_CLASSES: [usize; 9] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

/// Largest object still served from a slab.
pub const MAX_SLAB_OBJECT_BYTES: usize = 2048;

/// Maximum slabs one allocator tracks (one page per slab).
///
/// Must cover every pool page: a slab dedicates one page, so if this is
/// smaller than the pool's page count the fixed `slabs` record table fills
/// while pool bytes are still free, and a request fails with "out of memory"
/// despite an 87%-empty pool. Real-machine NK4-C 1.5c (`nk4c: OOM-RT
/// slabs=64/64 … px=65/512`, 2026-09-23): `MAX_SLABS=64` capped slab objects
/// at ~256 KiB of a 2 MiB pool because the pool was grown to 512 pages
/// (第12轮扰动实验) without raising this table. Tying it to the pool page
/// count keeps tracking and byte capacity consistent.
pub const MAX_SLABS: usize = GLOBAL_POOL_BYTES / PAGE_BYTES;

/// Maximum simultaneous whole-page allocations one allocator tracks.
///
/// A premature "out of memory" is confirmed on real machine NK4-C 1.58
/// (**B34**): after B33b let `/bin/sh` run, a cross-space `kdst copy` of a
/// 4 KiB run hit `nk4c: OOM-RT size=001000 … big=20/20 px=094/400`（十六
/// 进制：`big=0x20/0x20`=32/32 记录表满，而 `px=0x94/0x400`=148/1024
/// 空闲页仍在）——即早期登记的「big-block-heavy server 到来时 revisit」
/// 同型缺陷，与已修的 `MAX_SLABS`（见上）一类：每个 big block ≥ 1 页，
/// 所以原则上的上限应是 [`GLOBAL_POOL_PAGES`]。
///
/// 但未直接绑到 `GLOBAL_POOL_PAGES`：那会把分配器静态实例（`GlobalAllocator`
/// 里的 `big_blocks` 数组）撑大——每个 `Option<BigBlock>` 槽在 x86_64 上是
/// 24 B（`*mut u8` 无 niche，`Option` 需独立 discriminant），从 32 槽到
/// 1024 槽 ≈ +24 KB，平移每个用户服务器的 .bss/用户栈基址；而 boot 对服务
/// 镜像是 demand-fill-on-first-touch（未 eager 物化），实测确定性地让 VM 在
/// 启动初期自缺页递归 panic（从 30090 行崩回 124 行，§1.59 对照实验
/// bn22ctl 坐实）。那属反复被 defer 的结构债（boot eager 物化 / VM-backed
/// heap supplier）。因此本轮采用与 B29/B30 同类的“保守容量 round”：抬到
/// 覆盖真机观测峰值（big 峰值 47）的 64——(64−32)×24 B = +768 B <
/// 一页，不跨未物化页。原则解 = 待 boot eager 物化落地后改绑
/// `GLOBAL_POOL_PAGES`。
///
/// r7 实测把「boot eager 物化」的确切缺口钉死（单核重建·同镜像双跑）：本
/// 常量抬到 `GLOBAL_POOL_PAGES`=1024 → `OOM-RT`=0（premature-OOM 消失），
/// 但 boot 于 ~116 行确定性 `pagefault in VM`（rip=0x23581b、
/// cr2=rsp=0x7ffffffeef88、err=0x6 not-present）；抬到 128 同样崩（同一现
/// 场）、改回 64 全新重建复现 29685 行至 premature-OOM（`OOM-RT`=1）＝
/// 排除构建产物敏感性、坐实常量本身是回归因。根因＝`install_boot_stack`
/// 只为每个 boot 服务 eager 映**一个** frame 页、4 MiB 栈区其余页按需缺页，
/// 而 VM 无法服务自身启动缺页；`exec_bootproc` 的 eager `memsz` 只覆盖
/// PT_LOAD/.bss，**不覆盖初始栈 runway**（§1.91 猜「memsz eager 已化解 §1.59」
/// 因此被推翻）。⇒ 原则解的真前置是「boot 初始栈 eager runway 物化」（对位
/// C `handle_memory_once`/`MAP_PREALLOC`，§1.55 分歧），非纯抬本常量。
/// 故本轮仍留 64（非回退、是唯一不崩的已知值），把常量抬升与 eager-runway
/// 修复绑为一个单元（r7b）。
///
/// r7b 已落地（本轮）：在正确落点 `os/arch/src/arch/boot.rs` 把 VM 自身初
/// 始栈 eager runway `VM_STACK_SIZE` 64 KiB→256 KiB（覆盖 §1.92 实测 ~65 KiB
/// 启动栈下降）。§1.94 纠正：真落点是 kernel `load_vm_elf`（`install_boot_stack`
/// 对 VM 从不调用），非 memtype `ev_new`。前置既解除 → 本常量直绑
/// `GLOBAL_POOL_PAGES` 根除 premature-OOM（下表为其旧保守 round 的历史叙述）。
pub const MAX_BIG_BLOCKS: usize = GLOBAL_POOL_PAGES;

/// §1.60 护栏退役：旧编译期断言要求 `(MAX_BIG_BLOCKS - 32)` 的数组增长小于
/// 一页，以规避一个启动 panic。但 §1.92-§1.94 实证该 panic 源于 VM 自身初
/// 始栈 runway 不足（已在 `boot.rs` `VM_STACK_SIZE` 修），**非**本数组增长跨
/// 未物化页——VM 的 `.bss` 由 `load_vm_elf` 段循环按 memsz 整段 eager 物化。
/// 绑 `GLOBAL_POOL_PAGES`（每池页一条记录）才是原则天花板，一页护栏遂删除。

/// Initial heap pool for the global allocator, in bytes (2048 pages).
///
/// The pool is `.bss` storage, so its size charges each binary's image
/// segment. The bootproc leg materializes every segment page eagerly
/// (VM `exec_bootproc` copies the ELF image through freshly allocated
/// PFNs), and C additionally preallocates at mmap time for anything
/// carrying `MAP_PREALLOC` (C's image memmap leg is
/// `MAP_ANON|MAP_PREALLOC|MAP_UNINITIALIZED|MAP_FIXED`,
/// `minix3/minix/lib/libexec/exec_general.c:25`; `region.c:492-499`
/// takes the frames right there) — although this rewrite's VM currently
/// only records the PREALLOC bit and demand-fills on first touch
/// (`mmap.rs` `PREALLOC_MAP`, `memtype.rs` `AnonymousMemory` has no
/// `ev_new`), a divergence registered with NK4-C 1.55. Either way the
/// bootproc leg prices the pool in real pages at exec time, which caps
/// how far it may grow until the VM-backed supplier lands (C's RS asks
/// for an 8 MiB prealloc map outright — `rs/const.h:83
/// RS_VM_DEFAULT_MAP_PREALLOC_LEN`).
/// The former 16-page pool OOM'd real servers —
/// RS's boot allocates a 256512-byte table and aborted before its main
/// loop (real machine NK4-A C-3 c12a: "memory allocation of 256512 bytes
/// failed", 2026-09-22). C servers grow a real heap through VM `brk`; a
/// VM-backed supplier is the follow-up (alloc.rs §module docs, "later,
/// virtual memory mapping").
///
/// 512 pages died on the real machine NK4-C 1.55 (bn7/bn8: VFS panic
/// `memory allocation of 40960 bytes failed`, raw hex diag
/// `size=00a000 slabs=009/200 big=06/20 px=200/200 fp=0aa/200`): VFS's
/// own tables alone consume ~327 pages — `FProc` is
/// 4368 B (`filps: [Option<usize>; 255]` costs 16 B per slot where C's
/// `file_desc` pointer costs 8 B) × 256 slots, plus `filp[1024]` and
/// `vnode[1024]` — and the freed exec buffers cannot be reused for the
/// next multi-page `hdr_buf` run because the free stack only serves
/// single pages (see `supply_pages`). A legal working set, not a leak:
/// C keeps those tables in BSS and never caps its heap. 1024 pages
/// doubles the runway; the faithful fix is the VM-backed supplier.
///
/// NK4-C §续-279c (real machine gh140/141, ATF suite): the 1024-page pool
/// hit the same wall again from a new consumer — MFS's block cache pool
/// (`DEFAULT_POOL_BUFFERS` 1024 × 4 KiB lazily-filled `Buffer` pages, the C
/// `DEFAULT_NR_BUFS` faithful value) legally fills the whole pool while the
/// 16 MiB imgrd (ATF suite seeding, §续-278) grows the working set, leaving
/// zero runway for transient allocations: `oomrt caller=0xa` (MFS_PROC_NR),
/// `big=3b6/400 px=400/400 fp=0`. Budget identity for alloc-global servers
/// with a C-sized cache: cache pages + transient headroom ≤ pool pages —
/// 2048 doubles the runway again (same 扰动实验 posture as B29 above; VM is
/// unaffected: it does not take `alloc-global`, `os/servers/vm/Cargo.toml:
/// 47`, so the B34 .bss-self-paging landmine does not apply here). The
/// faithful fix stays the VM-backed supplier (module docs).
#[cfg(not(target_arch = "x86_64"))]
pub const GLOBAL_POOL_BYTES: usize = 2048 * PAGE_BYTES; // NK4-C §续-279c MFS cache 预算轮

/// x86_64 分档——§续-279c 容量轮的 x86 回归修复（真机 gh142 后补验）：
/// 同轮全量 boot 在 x86 上 init runcom 回落 SingleUser（aarch64/riscv 同池健
/// 康），x86 侧模块 exec/加载腿对大 .bss 段的预算约束与另两架构不同
/// （aarch64/riscv 的 boot exec 已 §1.101 demand-fill，.bss 不占 eager 窗口；
/// x86 同配置实测破窗口）。x86 暂无 ATF 套件需求（xtask 播种门控
/// ATF_BOOT_LEG_READY=[aarch64]），1024 页即 §续-279c 前的基线值——x86 产物
/// 逐字节回到旧态，回归零风险；aarch64/riscv 保留 2048（套件预算）。
/// 原则解（demand-zero 运行时段或 VM-backed heap，三架构统一 2048+）在
/// module docs 挂账，落地后删除本分档。
#[cfg(target_arch = "x86_64")]
pub const GLOBAL_POOL_BYTES: usize = 1024 * PAGE_BYTES;

/// Number of whole pages the global pool holds (`GLOBAL_POOL_BYTES` /
/// [`PAGE_BYTES`]). Used to size the free-page stack and slab record table
/// so neither caps before the pool bytes are exhausted.
pub const GLOBAL_POOL_PAGES: usize = GLOBAL_POOL_BYTES / PAGE_BYTES;

/// Why an allocation or break adjustment failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    /// No memory left: the supplier has no more pages, or every tracking
    /// slot is in use.
    OutOfMemory,
    /// The requested adjustment is malformed: a negative break move that
    /// wraps around, or a zero-sized allocation.
    InvalidRequest,
}

impl AllocError {
    /// Maps the failure to the closest Minix3 error number.
    ///
    /// Exhaustion is `ENOMEM` (value 12), the same error the C library
    /// reports when the virtual memory server refuses to grow the heap. A
    /// malformed request is `EINVAL` (value 22), the error the C library
    /// uses for malformed argument vectors. Both come from
    /// `minix3/sys/sys/errno.h`, so no new error code is invented.
    pub const fn to_errno(self) -> Errno {
        match self {
            AllocError::OutOfMemory => Errno::from_i32(minix_types::ENOMEM),
            AllocError::InvalidRequest => Errno::from_i32(minix_types::EINVAL),
        }
    }
}

/// Computes the new break address for a relative adjustment.
///
/// This is the pure half of `sbrk` (`minix3/minix/lib/libc/sys/sbrk.c:13-26`):
/// add the signed increment to the current break and reject wrap-around in
/// both directions (growing past the top or shrinking below the bottom, see
/// `sbrk.c:20-21`). Pointer arithmetic cannot be unit tested without mapped
/// memory; integer arithmetic over addresses can.
pub const fn request_new_break(current_break: u64, increment: i64) -> Result<u64, AllocError> {
    let candidate = current_break.wrapping_add(increment as u64);
    if increment > 0 && candidate < current_break {
        return Err(AllocError::InvalidRequest);
    }
    if increment < 0 && candidate > current_break {
        return Err(AllocError::InvalidRequest);
    }
    Ok(candidate)
}

/// Reports whether the kernel must be told about a new break address.
///
/// C: `brk` skips the server call entirely when the requested address equals
/// the cached `_brksize` (`minix3/minix/lib/libc/sys/brk.c:27-32`). The
/// server round trip is the expensive part of moving the break; this
/// predicate keeps the "did anything change" decision in one tested place.
pub const fn needs_kernel_update(cached_break: u64, requested_break: u64) -> bool {
    cached_break != requested_break
}

/// Source of whole pages for an allocator.
///
/// The trait separates page supply (a mechanism owned by the platform: a test
/// buffer, a static pool, or virtual memory mapping) from page management
/// (the slab policy in [`SlabAllocator`]). It has two behaviorally different
/// implementations today (always-refuse vs. fixed pool, plus a virtual
/// memory mapping supplier planned for later) and is used as a generic
/// bound, which keeps the abstraction justified under the project rule
/// for traits.
///
/// Contiguity contract: [`supply_pages`] must hand out one contiguous run;
/// single pages from [`supply_page`] carry no adjacency promise.
pub trait PageSupplier {
    /// Hands out one zeroed page, or `None` when exhausted.
    fn supply_page(&mut self) -> Option<*mut u8>;
    /// Hands out `page_count` zeroed contiguous pages, or `None`.
    fn supply_pages(&mut self, page_count: usize) -> Option<*mut u8>;
    /// Returns a previously supplied page.
    fn release_page(&mut self, page: *mut u8);
    /// NK4-C 1.5c 取证（task1-close 裁决删除）：池游标快照——(已 bump
    /// 消耗的页数, 池总页数, 空闲栈里的可复用页数)。用于真机区分
    /// 「运行期每轮真增长耗尽池」与「碎片化 / 跟踪数组上限」。
    /// 默认实现面向不暴露内部游标的 supplier（返回全零，调用方忽略）。
    fn pool_diag(&self) -> (usize, usize, usize) {
        (0, 0, 0)
    }
    /// Returns a previously supplied contiguous run.
    ///
    /// # Safety
    ///
    /// `first` must be the start of a run of exactly `page_count` pages
    /// previously handed out by [`supply_pages`] on this same supplier.
    unsafe fn release_pages(&mut self, first: *mut u8, page_count: usize) {
        let mut index = 0;
        while index < page_count {
            // SAFETY: upheld by the caller contract above.
            self.release_page(unsafe { first.add(index * PAGE_BYTES) });
            index += 1;
        }
    }
}

/// Page supplier that always refuses.
///
/// Models the state before any memory source is wired: every allocation
/// fails fast with a null pointer instead of faulting. Tests use it to cover
/// the exhaustion paths deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FailingSupplier;

impl PageSupplier for FailingSupplier {
    fn supply_page(&mut self) -> Option<*mut u8> {
        None
    }
    fn supply_pages(&mut self, _page_count: usize) -> Option<*mut u8> {
        None
    }
    fn release_page(&mut self, _page: *mut u8) {}
}

/// Page supplier carving pages out of a fixed buffer.
///
/// The buffer is split into [`PAGE_BYTES`]-byte pages handed out in order;
/// released pages go onto a free stack for reuse. The global allocator feeds
/// this supplier from its static pool; tests feed it from local arrays, which
/// keeps every test hermetic.
///
/// Alignment: the usable region starts at the first eight-byte boundary
/// inside the buffer (at most seven leading bytes are skipped), so every
/// page start — and therefore every slot — is eight-byte aligned no matter
/// how the caller aligned the buffer.
#[derive(Debug)]
pub struct FixedPoolSupplier<'a> {
    // Holds the exclusive borrow of the backing buffer for the whole
    // supplier lifetime; addressing goes through base/usable_len below.
    // The field is never read directly, which is intentional.
    #[allow(dead_code)]
    pool: &'a mut [u8],
    base: *mut u8,
    usable_len: usize,
    next_page: usize,
    free_pages: [*mut u8; GLOBAL_POOL_PAGES],
    free_count: usize,
}

impl<'a> FixedPoolSupplier<'a> {
    /// Borrows the buffer as a page pool. A trailing partial page is ignored,
    /// as are up to seven leading bytes before the first eight-byte boundary.
    pub fn new(pool: &'a mut [u8]) -> Self {
        let raw = pool.as_mut_ptr() as usize;
        let aligned = raw.next_multiple_of(8);
        let skip = aligned - raw;
        let usable_len = pool.len().saturating_sub(skip);
        FixedPoolSupplier {
            pool,
            base: aligned as *mut u8,
            usable_len,
            next_page: 0,
            free_pages: [core::ptr::null_mut(); GLOBAL_POOL_PAGES],
            free_count: 0,
        }
    }

    /// How many whole pages the buffer holds.
    pub fn total_pages(&self) -> usize {
        self.usable_len / PAGE_BYTES
    }

    fn page_at(&self, index: usize) -> *mut u8 {
        // SAFETY: index is always below total_pages, so the offset stays
        // inside the aligned usable region, which sits inside the borrowed
        // buffer that outlives the supplier.
        unsafe { self.base.add(index * PAGE_BYTES) }
    }

    /// NK4-C 第 15 轮取证（task1-close 裁决删除）：池游标轨迹——每次供页
    /// 打印（kind: S=单页 M=连续页run, 游标, 页数, 池基址）。定位
    /// 「RS 堆池重复分配/清零覆盖泄漏表」的游标回卷点。仅真机
    /// （kernel_trap，宿主 trap 返回 -EIO 不打印），cap 48。
    fn nk4a_supply_log(&self, kind: u8, index: usize, pages: usize) {
        #[cfg(not(feature = "mock"))]
        {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static N: AtomicUsize = AtomicUsize::new(0);
            if N.fetch_add(1, AtomicOrd::Relaxed) < 128 {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let mut line = *b"nk4a: sup k=S idx=0x        n=0x     base=0x                \n";
                line[9] = kind;
                {
                    let mut put = |at: usize, mut v: u64, digits: usize| {
                        let mut k = at + digits;
                        while k > at {
                            line[k - 1] = HEX[(v & 0xf) as usize];
                            v >>= 4;
                            k -= 1;
                        }
                    };
                    put(18, index as u64, 8);
                    put(29, pages as u64, 5);
                    put(45, self.base as usize as u64, 16);
                }
                let _ = minix_sys::syscall::sys_diagctl_write(
                    &minix_sys::syscall::DirectKernelCallTransport,
                    core::str::from_utf8(&line).unwrap_or(""),
                );
            }
        }
        #[cfg(feature = "mock")]
        let _ = (kind, index, pages);
    }
}

impl PageSupplier for FixedPoolSupplier<'_> {
    fn supply_page(&mut self) -> Option<*mut u8> {
        if self.free_count > 0 {
            self.free_count -= 1;
            let page = self.free_pages[self.free_count];
            self.free_pages[self.free_count] = core::ptr::null_mut();
            // Zero the reused page so callers always observe clean memory.
            // SAFETY: the page came from this pool and is page-sized.
            unsafe { core::ptr::write_bytes(page, 0, PAGE_BYTES) };
            self.nk4a_supply_log(b'F', self.free_count, 1);
            return Some(page);
        }
        if self.next_page < self.total_pages() {
            let page = self.page_at(self.next_page);
            self.next_page += 1;
            // SAFETY: fresh pool memory, page-sized by construction.
            unsafe { core::ptr::write_bytes(page, 0, PAGE_BYTES) };
            self.nk4a_supply_log(b'S', self.next_page - 1, 1);
            return Some(page);
        }
        None
    }

    fn supply_pages(&mut self, page_count: usize) -> Option<*mut u8> {
        if page_count == 0 {
            return None;
        }
        // A one-page run carries no adjacency requirement, so it is exactly
        // the "single-page request" the comment below reserves the free
        // stack for: delegate to `supply_page`, which pops a recycled page
        // first and only then advances the bump cursor. Without this a
        // single-page big-block request fails the moment the bump cursor
        // reaches the pool end even though freed pages sit on the free stack
        // (real machine NK4-C 1.60 **B35**: `size=0x1000` refused while
        // `px=400/400` bump exhausted and `fp=39c/400`=924 pages stranded).
        if page_count == 1 {
            return self.supply_page();
        }
        // Multi-page runs still need contiguity the scattered free stack
        // cannot promise, so they come only from the fresh bump region:
        // recycled single pages on the free stack are not necessarily
        // adjacent. Serving runs from a coalesced free list (or a
        // VM-backed heap) is the deferred structural fix.
        if self.next_page + page_count <= self.total_pages() {
            let first = self.page_at(self.next_page);
            self.next_page += page_count;
            self.nk4a_supply_log(b'M', self.next_page - page_count, page_count);
            // SAFETY: fresh pool memory, sized by construction.
            unsafe { core::ptr::write_bytes(first, 0, page_count * PAGE_BYTES) };
            return Some(first);
        }
        None
    }

    fn release_page(&mut self, page: *mut u8) {
        if self.free_count < self.free_pages.len() {
            self.free_pages[self.free_count] = page;
            self.free_count += 1;
        }
        // When the free stack is full the page is dropped: the pool owns it,
        // so nothing leaks outside the pool; the supplier simply forgets one
        // reusable page. The stack holds one slot per pool page (see
        // [`GLOBAL_POOL_PAGES`]), so it can never overflow before every page
        // is returned; this branch is defensive only.
    }

    fn pool_diag(&self) -> (usize, usize, usize) {
        (self.next_page, self.total_pages(), self.free_count)
    }
}

/// One slab: a single page cut into equal slots of one size class.
#[derive(Debug, Clone, Copy)]
struct Slab {
    page: *mut u8,
    class_index: u8,
    slot_bytes: u16,
    slot_count: u16,
    free_head: u16,
    free_count: u16,
}

/// Sentinel meaning "no free slot" in a free list head.
const NO_FREE_SLOT: u16 = u16::MAX;

/// One whole-page allocation record.
#[derive(Debug, Clone, Copy)]
struct BigBlock {
    page: *mut u8,
    page_count: usize,
}

/// Slab allocator over a page supplier.
///
/// Small objects (up to [`MAX_SLAB_OBJECT_BYTES`] bytes) come from slabs:
/// each slab dedicates one page to one size class and threads its free slots
/// into an intrusive list, storing the next-slot index in the first two
/// bytes of each free slot. Large objects become whole page runs tracked in
/// a fixed record table. A zero-sized request returns null: unlike some C
/// libraries, this allocator never hands out an ambiguous zero-byte block,
/// and the contract is documented on [`SlabAllocator::alloc`] so callers can
/// rely on it.
#[derive(Debug)]
pub struct SlabAllocator<S: PageSupplier> {
    supplier: S,
    slabs: [Option<Slab>; MAX_SLABS],
    big_blocks: [Option<BigBlock>; MAX_BIG_BLOCKS],
}

impl<S: PageSupplier> SlabAllocator<S> {
    /// Creates an allocator that draws pages from `supplier`.
    pub const fn new(supplier: S) -> Self {
        SlabAllocator {
            supplier,
            slabs: [None; MAX_SLABS],
            big_blocks: [None; MAX_BIG_BLOCKS],
        }
    }

    /// Selects the size class for `size`, or `None` for the page-run path.
    fn class_for(size: usize) -> Option<usize> {
        if size == 0 || size > MAX_SLAB_OBJECT_BYTES {
            return None;
        }
        let mut index = 0;
        while index < SIZE_CLASSES.len() {
            if SIZE_CLASSES[index] >= size {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn read_next(slot: *mut u8) -> u16 {
        // SAFETY: the slot belongs to a live slab page and stores a u16 at
        // offset zero by construction; slots are two-byte aligned because
        // every class size is a multiple of eight.
        unsafe { (slot as *const u16).read() }
    }

    fn write_next(slot: *mut u8, next: u16) {
        // SAFETY: same bounds as read_next; the write stays inside the slot.
        unsafe { (slot as *mut u16).write(next) };
    }

    fn slot_at(page: *mut u8, slot_bytes: usize, index: u16) -> *mut u8 {
        // SAFETY: index is always below the slab slot count, so the slot
        // stays inside the page.
        unsafe { page.add(index as usize * slot_bytes) }
    }

    fn find_slab_with_room(&self, class_index: usize) -> Option<usize> {
        let mut index = 0;
        while index < MAX_SLABS {
            if let Some(slab) = self.slabs[index]
                && slab.class_index as usize == class_index
                && slab.free_count > 0
            {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn add_slab(&mut self, class_index: usize) -> Option<usize> {
        let page = self.supplier.supply_page()?;
        let slot_bytes = SIZE_CLASSES[class_index];
        let slot_count = (PAGE_BYTES / slot_bytes) as u16;
        let mut slot = 0;
        while slot < slot_count {
            let next = if slot + 1 < slot_count {
                slot + 1
            } else {
                NO_FREE_SLOT
            };
            Self::write_next(Self::slot_at(page, slot_bytes, slot), next);
            slot += 1;
        }
        let mut index = 0;
        while index < MAX_SLABS {
            if self.slabs[index].is_none() {
                self.slabs[index] = Some(Slab {
                    page,
                    class_index: class_index as u8,
                    slot_bytes: slot_bytes as u16,
                    slot_count,
                    free_head: 0,
                    free_count: slot_count,
                });
                return Some(index);
            }
            index += 1;
        }
        // No slab record free: hand the page back so the supplier can reuse
        // it instead of stranding it inside the allocator.
        self.supplier.release_page(page);
        None
    }

    fn alloc_from_slab(&mut self, slab_index: usize) -> *mut u8 {
        let slab = self.slabs[slab_index].as_mut().expect("slab exists");
        let slot_index = slab.free_head;
        let slot = Self::slot_at(slab.page, slab.slot_bytes as usize, slot_index);
        slab.free_head = Self::read_next(slot);
        slab.free_count -= 1;
        slot
    }

    fn alloc_big(&mut self, size: usize) -> *mut u8 {
        let pages = size.div_ceil(PAGE_BYTES);
        let first = match self.supplier.supply_pages(pages) {
            Some(first) => first,
            None => return core::ptr::null_mut(),
        };
        let mut index = 0;
        while index < MAX_BIG_BLOCKS {
            if self.big_blocks[index].is_none() {
                self.big_blocks[index] = Some(BigBlock {
                    page: first,
                    page_count: pages,
                });
                return first;
            }
            index += 1;
        }
        // No record free: this allocation could never be freed later by
        // address lookup, so hand the run back and refuse it rather than
        // create an untracked block.
        // SAFETY: the run just came from supply_pages above.
        unsafe { self.supplier.release_pages(first, pages) };
        core::ptr::null_mut()
    }

    /// Allocates `size` bytes, returning null on any failure.
    ///
    /// The returned pointer is eight-byte aligned and points at readable,
    /// writable memory. A null return means "no memory" (supplier exhausted
    /// or tracking full) or "zero-sized request"; both are fail-fast instead
    /// of faulting later. Callers must pass the pointer back to [`free`]
    /// exactly once, or leak it deliberately with a comment.
    pub fn alloc(&mut self, size: usize) -> *mut u8 {
        // NK4-C 第 13 轮取证（task1-close 裁决删除）：大分配日志——RS 的
        // self=0 崩溃源自失控 memset(0x22e000, 0, 0x22e000)（len==dst 指针
        // 值，零化整个堆池）。此处记录大分配请求的 size/ptr 以对位。本
        // crate 零堆（无 fmt/alloc），hex 手工展开。
        #[cfg(not(feature = "mock"))]
        if size >= 0x10000 {
            let ptr = self.alloc_big(size);
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut line = *b"nk4a: rs-bigalloc size=0x        ptr=0x                \n";
            let mut put = |at: usize, mut v: u64, digits: usize| {
                let mut k = at + digits;
                while k > at {
                    line[k - 1] = HEX[(v & 0xf) as usize];
                    v >>= 4;
                    k -= 1;
                }
            };
            put(23, size as u64, 8);
            put(38, ptr as usize as u64, 16);
            let _ = minix_sys::syscall::sys_diagctl_write(
                &minix_sys::syscall::DirectKernelCallTransport,
                core::str::from_utf8(&line).unwrap_or("nk4a: rs-bigalloc\n"),
            );
            return if ptr.is_null() {
                core::ptr::null_mut()
            } else {
                ptr
            };
        }
        match Self::class_for(size) {
            Some(class_index) => {
                if let Some(slab_index) = self.find_slab_with_room(class_index) {
                    return self.alloc_from_slab(slab_index);
                }
                match self.add_slab(class_index) {
                    Some(slab_index) => self.alloc_from_slab(slab_index),
                    None => core::ptr::null_mut(),
                }
            }
            None => {
                if size == 0 {
                    return core::ptr::null_mut();
                }
                self.alloc_big(size)
            }
        }
    }

    /// Returns a block to the allocator.
    ///
    /// A null pointer is accepted and ignored, matching the C convention that
    /// freeing null is a no-op. Any other pointer must have come from
    /// [`alloc`] on this same allocator and must not have been freed before.
    /// Anything else stops the process with a panic instead of corrupting
    /// the heap silently: fail-fast beats silent corruption, and the C
    /// library answers the same situation by aborting with a heap-corruption
    /// diagnostic.
    pub fn free(&mut self, pointer: *mut u8) {
        if pointer.is_null() {
            return;
        }
        let mut index = 0;
        while index < MAX_SLABS {
            if let Some(slab) = self.slabs[index].as_mut() {
                let base = slab.page as usize;
                let address = pointer as usize;
                if address >= base && address < base + PAGE_BYTES {
                    let offset = address - base;
                    let slot_bytes = slab.slot_bytes as usize;
                    assert!(
                        offset.is_multiple_of(slot_bytes)
                            && offset / slot_bytes < slab.slot_count as usize,
                        "minix-rt: free of a pointer that is not a slot start"
                    );
                    Self::write_next(pointer, slab.free_head);
                    slab.free_head = (offset / slot_bytes) as u16;
                    slab.free_count += 1;
                    return;
                }
            }
            index += 1;
        }
        let mut big = 0;
        while big < MAX_BIG_BLOCKS {
            if let Some(block) = self.big_blocks[big]
                && block.page == pointer
            {
                // SAFETY: the record proves this run came from supply_pages.
                unsafe { self.supplier.release_pages(block.page, block.page_count) };
                self.big_blocks[big] = None;
                return;
            }
            big += 1;
        }
        panic!("minix-rt: free of a pointer this allocator never handed out");
    }

    /// NK4-C 1.5c 取证（task1-close 裁决删除）：分配器状态快照——
    /// 在用 slab 数 / 在用 big-block 数 / 池游标三元组。真机采样看
    /// `slabs_in_use` 是否随事件计数单调爬升近 `MAX_SLABS`（跟踪数组
    /// 上限/碎片化耗尽）还是 `pages_consumed` 近池总页数（字节耗尽）。
    pub fn diag(&self) -> AllocDiag {
        let mut slabs_in_use = 0;
        let mut index = 0;
        while index < MAX_SLABS {
            if self.slabs[index].is_some() {
                slabs_in_use += 1;
            }
            index += 1;
        }
        let mut big_in_use = 0;
        index = 0;
        while index < MAX_BIG_BLOCKS {
            if self.big_blocks[index].is_some() {
                big_in_use += 1;
            }
            index += 1;
        }
        let (pages_consumed, total_pages, free_pages) = self.supplier.pool_diag();
        AllocDiag {
            slabs_in_use,
            big_in_use,
            pages_consumed,
            total_pages,
            free_pages,
        }
    }
}

/// 快照输出（NK4-C 1.5c 取证，task1-close 裁决删除）。
#[derive(Debug, Clone, Copy)]
pub struct AllocDiag {
    pub slabs_in_use: usize,
    pub big_in_use: usize,
    pub pages_consumed: usize,
    pub total_pages: usize,
    pub free_pages: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool_buffer() -> [u8; 16384] {
        [0u8; 16384]
    }

    fn test_allocator(pool: &mut [u8]) -> SlabAllocator<FixedPoolSupplier<'_>> {
        SlabAllocator::new(FixedPoolSupplier::new(pool))
    }

    #[test]
    fn test_break_grows_and_shrinks() {
        assert_eq!(request_new_break(0x1000, 0x500), Ok(0x1500));
        assert_eq!(request_new_break(0x1500, -0x500), Ok(0x1000));
        assert_eq!(request_new_break(0x1000, 0), Ok(0x1000));
    }

    #[test]
    fn test_break_overflow_in_both_directions_is_rejected() {
        assert_eq!(
            request_new_break(u64::MAX, 1),
            Err(AllocError::InvalidRequest)
        );
        assert_eq!(request_new_break(0, -1), Err(AllocError::InvalidRequest));
    }

    #[test]
    fn test_unchanged_break_needs_no_kernel_call() {
        assert!(!needs_kernel_update(0x1000, 0x1000));
        assert!(needs_kernel_update(0x1000, 0x2000));
    }

    #[test]
    fn test_alloc_returns_writable_aligned_memory() {
        let mut pool = pool_buffer();
        let mut allocator = test_allocator(&mut pool);
        let pointer = allocator.alloc(13);
        assert!(!pointer.is_null());
        assert_eq!(pointer as usize % 8, 0);
        // SAFETY: the allocator just handed out 16 bytes here.
        unsafe {
            core::ptr::write_bytes(pointer, 0xAB, 13);
            assert_eq!(core::ptr::read(pointer), 0xAB);
        }
    }

    #[test]
    fn test_zero_sized_request_returns_null() {
        let mut pool = pool_buffer();
        let mut allocator = test_allocator(&mut pool);
        assert!(allocator.alloc(0).is_null());
    }

    #[test]
    fn test_free_null_is_a_no_op() {
        let mut pool = pool_buffer();
        let mut allocator = test_allocator(&mut pool);
        allocator.free(core::ptr::null_mut());
    }

    #[test]
    fn test_freed_slot_is_reused() {
        let mut pool = pool_buffer();
        let mut allocator = test_allocator(&mut pool);
        let first = allocator.alloc(8);
        allocator.free(first);
        let second = allocator.alloc(8);
        // Last-in-first-out reuse: the same slot comes back.
        assert_eq!(first, second);
    }

    /// B34 回归 pin：`big_blocks` 记录表早先硬编 32，会在池字节仍充裕时
    /// 因表满而 premature-OOM（真机 `nk4c: OOM-RT size=001000 big=20/20
    /// px=094/400`，十六进制＝32/32 表满而 148/1024 空闲页仍在）。本轮按
    /// 常量文档采用「保守容量 round」把表抬到 64（覆盖真机峰值 32/32），
    /// 原则解（绑 [`GLOBAL_POOL_PAGES`]）待 boot eager 物化落地。本测从 48
    /// 页池连续分配 40 个单页 big block（旧码 32 上限会在第 33 个返
    /// null），全部必须成功，且全释放后记录表槽可回收再分配。
    #[test]
    fn test_big_block_table_not_capped_at_32() {
        let mut pool = [0u8; 48 * PAGE_BYTES];
        let mut allocator = test_allocator(&mut pool);
        let mut ptrs = [core::ptr::null_mut::<u8>(); 40];
        for slot in ptrs.iter_mut() {
            let p = allocator.alloc(PAGE_BYTES); // 4 KiB → 单个 big block
            assert!(!p.is_null(), "big block >32 must not premature-OOM");
            *slot = p;
        }
        for p in ptrs {
            allocator.free(p);
        }
        // 全释放后仍可分配一个 big block（记录表槽回收生效）。
        assert!(!allocator.alloc(PAGE_BYTES).is_null());
    }

    /// B35 回归 pin：单页 big block 必须能复用 free-stack。旧
    /// `supply_pages` 只走 bump 游标，游标撞池尾后即使 free-stack 有
    /// 空闲页也拒绝对 1 页请求供货（真机 `size=0x1000` 而 `fp=924`
    /// 搁浅）。本测用 4 页池：先填满 bump（4 个单页 big block），释
    /// 放一个回 free-stack，再取一个必须从回收页成功。
    #[test]
    fn test_single_page_big_block_reuses_free_stack() {
        let mut pool = [0u8; 4 * PAGE_BYTES];
        let mut allocator = test_allocator(&mut pool);
        // 填满 bump 区：4 个单页 big block，游标至池尾。
        let mut held = [core::ptr::null_mut::<u8>(); 4];
        for slot in held.iter_mut() {
            let p = allocator.alloc(PAGE_BYTES); // 4 KiB → 1 页 big block
            assert!(!p.is_null(), "fill must succeed while bump has room");
            *slot = p;
        }
        // 释放一个→回 free-stack；此时 bump 已尽，新请求只能来
        // 自回收页（旧码 `supply_pages(1)` 在此必返 null）。
        allocator.free(held[0]);
        held[0] = core::ptr::null_mut();
        let reused = allocator.alloc(PAGE_BYTES);
        assert!(
            !reused.is_null(),
            "1-page big block must reuse a page from the free stack"
        );
        allocator.free(reused);
        for p in held {
            if !p.is_null() {
                allocator.free(p);
            }
        }
    }

    #[test]
    fn test_exhausted_supplier_fails_fast() {
        let mut allocator = SlabAllocator::new(FailingSupplier);
        assert!(allocator.alloc(8).is_null());
        assert!(allocator.alloc(8192).is_null());
    }

    #[test]
    fn test_big_allocation_spans_whole_pages() {
        // The pool must be page-aligned for this test: the global pool is
        // `#[repr(align(4096))]`, and a large block starts at the first
        // page the supplier hands out — so with an aligned pool the block
        // start is page-aligned too. A plain array would not mirror that
        // contract.
        #[repr(align(4096))]
        struct AlignedPool([u8; 16384]);
        let mut pool = AlignedPool([0u8; 16384]);
        let mut allocator = SlabAllocator::new(FixedPoolSupplier::new(&mut pool.0));
        let pointer = allocator.alloc(5000);
        assert!(!pointer.is_null());
        // The block starts a whole two-page run: page-aligned by the
        // supplier's page-boundary arithmetic, writable across both pages.
        assert_eq!(
            pointer as usize % PAGE_BYTES,
            0,
            "large allocation must start on a supplier page boundary"
        );
        // SAFETY: 5000 bytes across two pages were just handed out.
        unsafe {
            core::ptr::write_bytes(pointer, 0xCD, 5000);
            assert_eq!(core::ptr::read(pointer.add(4999)), 0xCD);
        }
        allocator.free(pointer);
    }

    #[test]
    fn test_size_classes_cover_every_small_size() {
        // Every size up to the slab limit maps to a class that fits it.
        let mut size = 1;
        while size <= MAX_SLAB_OBJECT_BYTES {
            let class = SlabAllocator::<FailingSupplier>::class_for(size).expect("class exists");
            assert!(SIZE_CLASSES[class] >= size);
            size += 1;
        }
        assert_eq!(
            SlabAllocator::<FailingSupplier>::class_for(MAX_SLAB_OBJECT_BYTES + 1),
            None
        );
    }

    #[test]
    fn test_alloc_errors_map_to_documented_errnos() {
        assert_eq!(
            AllocError::OutOfMemory.to_errno(),
            Errno::from_i32(minix_types::ENOMEM)
        );
        assert_eq!(
            AllocError::InvalidRequest.to_errno(),
            Errno::from_i32(minix_types::EINVAL)
        );
    }
}
