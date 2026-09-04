# Minix3 VM Server → Rust 实现覆盖检查表

> 生成日期: 2026-06-12
> 目标: 100% 覆盖 Minix3 VM server (`minix3/minix/servers/vm/`) 所有逻辑
> 范围: 24 个 .c 文件 + 多个 .h + arch/ 子目录
> 验证依据: `minix3/minix/servers/vm/` (C 源) ↔ `os/servers/vm/src/` (Rust 实现)

## 0. 覆盖度总览

| 分类 | 总数 | 已实现 | 未实现 | 实现率 |
|------|------|--------|--------|--------|
| 宏 (#define) | 88 | 28 | 60 | 32% |
| 全局变量 | 25 | 18 | 7 | 72% |
| 结构体 (struct/typedef) | 22 | 14 | 8 | 64% |
| 函数 (non-static) | 137 | 93 | 44 | 68% |
| 函数 (static) | ~38 | 6 | 32 | 16% |
| 6 类 mem_type 回调 | 6×15=90 | 70 | 20 | 78% |
| IPC handler | 30 | 17 | 13 | 57% |
| **总体** | **~430** | **252** | **178** | **~59%** |

> **注意**: "未实现" 不等于 "bug" — 许多未实现项是**有意省略** (如 `SANITYCHECKS`/`CACHE_SANITY` 调试宏)、**设计差异** (BTreeMap 替代 AVL 树)、或**阶段性未完成** (详见每项 Reason 列)。

---

## 1. 宏 (#define) 覆盖

### 1.1 vm.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-001 | `SANITYCHECKS` | vm.h:8 | 健全检查开关 | 改 `#[cfg(feature = "sanity_checks")]` 替代 | 未实现 (改 cfg) |
| M-002 | `CACHE_SANITY` | vm.h:9 | 缓存检查开关 | `#[cfg(feature = "cache_sanity")]` | 未实现 |
| M-003 | `VMSTATS` | vm.h:10 | 统计开关 | 改 `#[cfg(feature = "vm_stats")]` | 未实现 |
| M-004 | `MEMPROTECT` | vm.h:13 | slab 内存保护 | `slab` 模块未实现 | 跳过 (依赖 slab) |
| M-005 | `JUNKFREE` | vm.h:14 | 释放填充垃圾 | 未实现 | 设计决策 (Rust Drop 自动) |
| M-006 | `PAF_CLEAR` | vm.h:22 | 清零标志 | `PageAllocFlags::ZERO` | 已实现 |
| M-007 | `PAF_CONTIG` | vm.h:23 | 物理连续 | `PageAllocFlags::CONTIGUOUS` | 已实现 |
| M-008 | `PAF_ALIGN64K` | vm.h:24 | 64K 对齐 | `PageAllocFlags::ALIGN_64K` | 已实现 |
| M-009 | `PAF_LOWER16MB` | vm.h:25 | 16M 以下 | `PageAllocFlags::LOW_16M` | 已实现 |
| M-010 | `PAF_LOWER1MB` | vm.h:26 | 1M 以下 | `PageAllocFlags::LOW_1M` | 已实现 |
| M-011 | `PAF_ALIGN16K` | vm.h:27 | 16K 对齐 | `PageAllocFlags::ALIGN_16K` | 已实现 |
| M-012 | `MARK` | vm.h:29 | 调试标记 | 不需要 (Rust dbg!) | 设计差异 |
| M-013 | `AM_AUTO` | vm.h:32 | 自动映射 | `MmapFlags::AUTO` | 已实现 |
| M-014 | `VERBOSE` | vm.h:35 | 详细输出 | 不需要 | 设计差异 |
| M-015 | `LU_DEBUG` | vm.h:36 | Live Update 调试 | 不需要 | 设计差异 |
| M-016 | `MINSTACKREGION` | vm.h:39 | 最小栈区 | `MIN_STACK_SIZE` | 已实现 (mmap.rs) |
| M-017 | `SCL_NONE/SCL_FS/SCL_*)` | vm.h:42-46 | 健全检查等级 | 不需要 (cfg 切换) | 设计差异 |
| M-018 | `VMP_SPARE/VMP_PAGETABLE/VMP_PAGEDIR/VMP_SLAB` | vm.h:49-53 | 页面分配类别 | `VmPageAllocReason` enum | 已实现 (alloc_page.rs) |
| M-019 | `WMF_OVERWRITE/WMF_WRITEFLAGSONLY/WMF_FREE/WMF_VERIFY` | vm.h:56-59 | writemap 标志 | `WriteMapFlags` | 已实现 (pagetable) |
| M-020 | `MAP_NONE` | vm.h:61 | 无映射值 | `PhysBytes::NONE` / `Option<PhysBytes>` | 已实现 |
| M-021 | `NO_MEM` | vm.h:62 | 分配失败值 | `Result<_, AllocError>` | 设计差异 (Result) |
| M-022 | `VM_DATATOP` | vm.h:65 | 数据顶端 | `VM_HEAP_BASE` + heap 配置 | 已实现 (直接映射) |
| M-023 | `VM_STACKTOP` | vm.h:67 | 栈顶 | `STACK_TOP` | 已实现 |
| M-024 | `VM_MMAP_MIN/VM_MMAP_MAX` | vm.h:75-79 | mmap 范围 | `MMAP_BASE/MMAP_TOP` | 已实现 (mmap.rs) |
| M-025 | `VM_OWN_*` | vm.h:83-86 | VM 自有范围 | `VM_HEAP_*` | 已实现 (direct_map.rs) |

### 1.2 region.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-026 | `PBF_INCACHE` | region.h:35 | 缓存块标志 | `PageFlags::IN_CACHE` | 已实现 (page_state.rs) |
| M-027 | `VR_WRITABLE/VR_PHYS64K/VR_LOWER16MB/VR_LOWER1MB/VR_SHARED/VR_UNINITIALIZED/VR_ANON/VR_DIRECT/VR_PREALLOC` | region.h:69-79 | 虚拟区标志 | `VrFlags` (bitflags!) | 已实现 (vir_region.rs:16-27) |
| M-028 | `MF_PREALLOC` | region.h:82 | 预分配标志 | `MmapFlags::PREALLOC` | 已实现 |

### 1.3 vmproc.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-029 | `VMF_INUSE` | vmproc.h:35 | 进程槽位使用中 | `VmFlags::IN_USE` | 已实现 (flags.rs) |
| M-030 | `VMF_EXITING` | vmproc.h:36 | 进程退出中 | `VmFlags::EXITING` | 已实现 |
| M-031 | `VMF_VM_INSTANCE` | vmproc.h:37 | VM 实例槽位 | `VmFlags::VM_INSTANCE` | 已实现 |

### 1.4 alloc.c (分配器相关)

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-032 | `NUMBER_PHYSICAL_PAGES` | alloc.c:33 | 物理页数 | `TOTAL_PAGES` | 已实现 (global.rs) |
| M-033 | `PAGE_BITMAP_CHUNKS` | alloc.c:34 | 位图块数 | `BitmapAllocator::chunk_count` | 已实现 (bitmap_alloc.rs) |
| M-034 | `PAGE_CACHE_MAX` | alloc.c:36 | 页缓存上限 | 未建模（C 编译期上限；Rust 由内存压力驱动 `free_pages`，见 24-page-cache） | 设计差异 |
| M-035 | `page_isfree(p)` | alloc.c:54 | 测页空闲 | `BitmapAllocator::is_free(p)` | 已实现 |
| M-036 | `RESERVEDMAGIC/MAXRESERVED*` | alloc.c:56-58 | 保留队列 | 改 `ReservedQueue` 类型 | 已实现 (reserved_pages) |

### 1.5 acl.c

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-037 | `NO_ACL` | acl.c:10 | 无 ACL 标记 | `AclState::Uninitialized` | 已实现 (acl.rs) |
| M-038 | `USER_ACL` | acl.c:11 | 用户 ACL | `AclState::Default` | 已实现 |
| M-039 | `FIRST_SYS_ACL` | acl.c:12 | 系统 ACL 起点 | `AclState::System(_)` | 已实现 |

### 1.6 break.c

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-040 | `DATA_CHANGED/STACK_CHANGED` | break.c:38-39 | 段改变标志 | `BrkFlags` | 已实现 (brk.rs) |

### 1.7 cache.c

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-041 | `HASHSIZE` | cache.c:21 | 哈希表大小 | `PageCache::by_dev`（BTreeMap 主键 + `by_ino` 辅索引，`[ARCH: A-4]`，无固定哈希桶） | 已实现 (page_cache.rs) |

### 1.8 main.c

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-042 | `CALLNUMBER(req)` | main.c:57 | 调用号转换 | `VmCallNr` enum | 已实现 (ipc/dispatcher.rs) |
| M-043 | `CALLMAP` | main.c:523 | 注册调用 | `MessageDispatcher::dispatch_by_number` | 已实现 |

### 1.9 pagefaults.c

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-044 | `VALID` | pagefaults.c:48 | 状态合法值 | `PageFaultState::Valid` | 已实现 (cow_exec_pf.rs) |

### 1.10 proto.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-045 | `usedpages_add(...)` | proto.h:40 | 健全包装宏 | `alloc_stats.rs::record_alloc` | 已实现 (简化) |
| M-046 | `SLABALLOC/SLABFREE` | proto.h:133-134 | slab 包装 | `slab` 未实现 | 跳过 |

### 1.11 pt.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-047 | `CLICKSPERPAGE` | pt.h:27 | 每页 click 数 | `CLICK_SIZE` (4KB 隐含 1) | 已实现 |

### 1.12 regionavl_defs.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-048-M-055 | `AVL_*` | regionavl_defs.h:3-17 | AVL 树宏实例 | `BTreeMap<VirBytes, VirRegion>` | 设计差异 (BTreeMap 替代) |

### 1.13 region.c

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-056 | `ALLREGIONS` | region.c:175 | 全区迭代 | `RegionMap::iter()` | 已实现 (region_map.rs:217) |
| M-057 | `MYSLABSANE` | region.c:196 | 健全检查 | 跳过 (slab 未实现) | 跳过 |
| M-058 | `SLOT_FAIL` | region.c:297 | 槽位查找失败 | `Result<_, RegionError>` | 设计差异 (Result) |
| M-059-M-062 | `FREEVRANGE*` | region.c:341-350 | 区间搜索 | `find_slot` / `find_all_overlaps` | 已实现 (region_map.rs) |

### 1.14 sanitycheck.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-063-M-067 | `SANITYCHECK/MYASSERT/USE/SLABSANE/PT_SANE` | sanitycheck.h:10-65 | 健全检查宏族 | 改 `#[cfg(feature = ...)]` + 单元测试 | 设计差异 |

### 1.15 slaballoc.c

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-068-M-083 | `SLABSIZES/ITEMSPERPAGE/ELBITS/BITPAT/GETBIT/SETBIT/CLEARBIT/OBJALIGN/MINSIZE/MAXSIZE/USEELEMENTS/BITS_FULL` | slaballoc.c:29-82 | slab 分配宏 | `slab` 模块未实现 | **未实现 (整模块空)** |
| M-084-M-087 | `SLABDATA*` | slaballoc.c:42-65 | 内存保护宏 | 跳过 | 跳过 |
| M-088-M-092 | `WRITABLE_*/DATABYTES/MAGIC1/2/JUNK/NOJUNK` | slaballoc.c:93-116 | slab 哨兵 | 跳过 | 跳过 |
| M-093-M-098 | `GETSLAB/ADDHEAD/UNLINKNODE/OBJSTATSCHECK` | slaballoc.c:130-355 | slab 链表操作 | 跳过 | 跳过 |

### 1.16 vfs.c

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-099 | `STATELEN` | vfs.c:31 | 状态长度 | `VfsRequest::STATE_LEN` | 已实现 (vfs_queue.rs) |
| M-100 | `ID_MAX` | vfs.c:55 | 请求 ID 上限 | `VfsRequest::MAX_ID` | 已实现 |

### 1.17 util.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-101 | `ELEMENTS(x)` | util.h:8 | 数组元素数 | `x.len()` | 设计差异 (内建方法) |

### 1.18 cavl_if.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-102-M-105 | `AVL_IMPL_*` | cavl_if.h:194-211 | AVL 实现位 | `BTreeMap` 替代 | 设计差异 |

### 1.19 arch/i386/pagetable.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-106-M-115 | `PTF_*/ARCH_VM_*/ARCH_PAGEDIR_SIZE/PFERR_*` | arch/i386/pagetable.h:11-46 | i386 页表宏 | `minix_arch::paging::*` | 已实现 (通过 trait 抽象) |

### 1.20 arch/earm/pagetable.h

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-116-M-125 | `PTF_*/ARCH_VM_*/ARCH_PAGEDIR_SIZE/PFERR_*` | arch/earm/pagetable.h:11-49 | ARM 页表宏 | `minix_arch::paging::*` | 已实现 (通过 trait 抽象) |

### 1.21 pagetable.c

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| M-126 | `MAX_PAGEDIR_PDES` | pagetable.c:37 | 页目录 PDE 数 | `PageTable::MAX_PDES` | 已实现 (arch crate) |
| M-127-M-130 | `SPAREPAGES/STATIC_SPAREPAGES/SPAREPAGEDIRS/STATIC_SPAREPAGEDIRS` | pagetable.c:60-77 | 备用页 | 无（Direct Map `[ARCH: A-1]` 结构消除） | 结构消除（06-page-allocator.md §3.3） |
| M-131 | `is_staticaddr(va)` | pagetable.c:85 | 静态地址判定 | 无（[ARCH: A-1] 结构消除，无 BSS 静态页概念） | 结构消除（07-pagetable-struct.md §1.5） |
| M-132 | `MAX_KERNMAPPINGS` | pagetable.c:87 | 内核映射数 | `MAX_KERNEL_MAPPINGS` | 已实现 (pagetable) |
| M-133 | `FLAG` | pagetable.c:589 | 页表项标志打印宏 | `PageFlags::Display` impl | 已实现 (minix_arch::paging::PageFlags) |

**宏覆盖小结**: 32 个核心宏已实现 (主要在 phys_mem/pagetable/memtype/vmproc 模块), 56 个未实现 (主要是 sanity check、slab 依赖、AVL 树、调试宏等)。

---

## 2. 全局变量覆盖

### 2.1 EXTERN 声明 (glo.h)

| # | C 变量 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| G-001 | `vmproc[]` | glo.h:20 | 进程表 (1024 槽位) | `VM_PROC_TABLE: [AssumeSyncCell<VmProc>; VM_PROC_COUNT]` | 已实现 (vmproc/table.rs) |
| G-002 | `enable_filemap` | glo.h:22 | 文件映射开关 | `GLOBAL.enable_filemap` | 已实现 (global.rs) |
| G-003 | `kernel_boot_info` | glo.h:25 | 启动信息 | `VmServer.boot_procs`（`BootParams`，全局 `BOOT_INFO` 已删，V9-P3-1） | 已实现 (vm_server.rs) |
| G-004 | `nocheck/incheck/sc_lastline` | glo.h:28-30 | 健全检查控制 | 跳过 (cfg 控制) | 跳过 |
| G-005 | `sc_lastfile` | glo.h:31 | 健全检查位置 | 跳过 | 跳过 |
| G-006 | `mem_type_anon` | glo.h:37 | 匿名内存类型 | `MEM_TYPE_ANON` | 已实现 (memtype.rs:692) |
| G-007 | `mem_type_directphys` | glo.h:38 | 直映射 | `MEM_TYPE_DIRECT` | 已实现 (memtype.rs:693) |
| G-008 | `mem_type_anon_contig` | glo.h:39 | 连续物理 | `MEM_TYPE_CONTIG_ANON` | 已实现 (memtype.rs:695) |
| G-009 | `mem_type_cache` | glo.h:40 | 缓存类型 | `MEM_TYPE_CACHE` | 已实现 (memtype.rs:696) |
| G-010 | `mem_type_mappedfile` | glo.h:41 | 文件映射 | `MEM_TYPE_MAPPED_FILE` | 已实现 (memtype.rs:697) |
| G-011 | `mem_type_shared` | glo.h:42 | 共享类型 | `MEM_TYPE_SHARED` | 已实现 (memtype.rs:694) |
| G-012 | `total_pages` | glo.h:45 | 总页数 | `TOTAL_PAGES` | 已实现 (global.rs) |
| G-013 | `num_vm_instances` | glo.h:46 | VM 实例数 | `VM_INSTANCE_COUNT` | 已实现 (global.rs) |

### 2.2 main.c

| # | C 变量 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| G-014 | `__vm_init_fresh` | main.c:64 | 首次启动标志 | `VmServer::init_fresh` | 已实现 (vm_server.rs) |
| G-015 | `rprocpub` | main.c:63 | RS 进程表 (static) | 集成在 `vm_server.rs::VmServer` | 已实现 + **TODO `ipc_call_rs_init()` 3 阶段合约文档**: 1) `mess_rs_init` send→RS; 2) receive `mess_rs_init_reply`; 3) `sys_safecopyfrom` 拷出 rproctab. 3 条 DEFERRED 依赖 (IpcTransport/kernel IPC core, sys_safecopyfrom/safecopy, endpoint→proc_nr/proc-table lookup). 当前返 `Ok(RprocTab::empty())` 保持 rs_handshake 非 panic |

### 2.3 alloc.c

| # | C 变量 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| G-016 | `missing_spares` | alloc.c:74 | 缺失备用页数 | `alloc_stats.missing` | 已实现 (alloc_stats.rs) |
| G-017 | `free_pages_bitmap` | alloc.c:35 | 空闲页位图 | `BitmapAllocator::bitmap` | 已实现 (bitmap_alloc.rs) |
| G-018 | `free_page_cache/free_page_cache_size` | alloc.c:37-38 | 空闲页缓存 | `PageCache::cache` | 已实现 (page_cache.rs) |

### 2.4 pagetable.c

| # | C 变量 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| G-019 | `missing_sparedirs` | pagetable.c:79 | 缺失备用页目录 | `alloc_stats.missing_dirs` | 已实现 (alloc_stats.rs) |
| G-020 | `kernmappings` | pagetable.c:94 | 内核映射数 | `pagetable::KERNMAPPINGS` | 已实现 |
| G-021 | `vm_self_pages` | pagetable.c:34 | VM 自有页数 (static) | `VmAllocStats`（alloc_stats.rs，06 D4） | 已实现（06-page-allocator.md §3.4） |
| G-022 | `kern_size/kern_start_pde/bigpage_ok` | pagetable.c:46-50 | 内核大小/起点/大页 (static) | `boot_info.kernel_*` | 已实现 (global.rs) |
| G-023 | `vmprocess` | pagetable.c:53 | VM 自身进程槽指针 | `VmProcTable::vm_self()` | 已实现 |
| G-024 | `global_bit` | pagetable.c:73 | 全局位 (static) | `PageFlags::GLOBAL` | 已实现 |

### 2.5 其他

| # | C 变量 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| G-025 | `acl_mask/acl_inuse` | acl.c:14-15 | ACL 表 (static) | `AclState` enum (per-process) | 已实现 (acl.rs) |
| G-026 | `fdrefs` | fdref.c:35 | fdref 链表头 (static) | `FdRefTable::entries` | 已实现 (fdref.rs) |
| G-027 | `cache_hash_*` | cache.c:23-26 | 缓存哈希与 LRU (static) | `PageCache::by_dev`/`by_ino` 双索引 + `LruList` | 已实现 (page_cache.rs) |
| G-028 | `cached_pages` | cache.c:27 | 缓存页计数 (static) | `PageCache::total_cached` | 已实现 |
| G-029 | `pages` | slaballoc.c:79 | slab 页数 (static) | `slab` 未实现 | 跳过 |
| G-030 | `vfs_request_node/first_queued/active` | vfs.c:33-41 | VFS 请求节点 (static) | `VfsRequestQueue` | 已实现 (vfs_queue.rs) |

**全局变量覆盖小结**: 18/25 = 72% 已实现。7 个未实现主要是 sanity check 控制 (依赖 cfg 切换) 和 slab 依赖项。

---

## 3. 结构体覆盖

| # | C 结构体 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|----------|---------|------|-----------|------|
| S-001 | `struct cached_page` | cache.h:2 | 缓存页描述 | `CachedPage`（`ino: Option<u64>`/`once: bool`/`pfn`/`lru_node`） | 已实现 (page_cache.rs) |
| S-002 | `avl_search_type` (enum) | cavl_if.h:24 | AVL 搜索类型 | `SearchType` (enum, 互斥方向) | 已实现 (region_map.rs) — **已从 bitflags 改为 enum** |
| S-003 | `avl` (typedef) | cavl_if.h:67 | AVL 树 | `BTreeMap<VirBytes, VirRegion>` | 设计差异 |
| S-004 | `iter` (typedef) | cavl_if.h:158 | AVL 迭代器 | `RegionMap::iter()` | 已实现 (region_map.rs:217) |
| S-005 | `struct fdref` | fdref.h:18 | fd 引用 | `FdRefEntry` | 已实现 (fdref.rs) |
| S-006 | `vm_calls[]` (匿名) | main.c:48 | 调用表 | `MessageDispatcher::dispatch_by_number` | 已实现 (ipc/dispatcher.rs) |
| S-007 | `struct vm_exec_info` | main.c:288 | 启动进程执行 | `VmExecInfo` | 已实现 (vm_server.rs) |
| S-008 | `struct memlist` | memlist.h:5 | 内存链表 | `BootMemRegion` | 已实现 (phys_mem/types.rs) |
| S-009 | `struct mem_type` (typedef mem_type_t) | memtype.h:12 | 内存类型表 | `MemType` trait + 6 impls | 已实现 (memtype.rs) |
| S-010 | `struct pf_state` | pagefaults.c:33 | 缺页状态 | `PageFaultState` | 已实现 (cow_exec_pf.rs) |
| S-011 | `struct hm_state` | pagefaults.c:39 | 内存处理状态 | `MemoryHandleState` | 已实现 (cow_exec_pf.rs) |
| S-012 | `struct pdm` | pagetable.c:38 | 页目录映射 | `PageDirectoryMapping` (在 arch crate) | 已实现 (arch crate) |
| S-013 | `struct phys_region` (typedef phys_region_t) | phys_region.h:8 | 物理区 | — | **已删除** (phys_region.rs 已移除) |
| S-014 | `pt_t` (typedef) | pt.h:11 | 页表 | `PageTable` type alias | 已实现 (pagetable/mod.rs) |
| S-015 | `struct phys_block` | region.h:23 | 物理块 | — | **已删除** (phys_region.rs 已移除) |
| S-016 | `struct vir_region` (typedef region_t) | region.h:37 | 虚拟区 | `VirRegion` | 已实现 (region/vir_region.rs) |
| S-017 | `ALLREGIONS` macro code | region.c:175 | 区域迭代模板 | `RegionMap::iter()` | 已实现 (region_map.rs:217) |
| S-018 | `struct sdh` | slaballoc.c:96 | slab 数据头 | `slab` 未实现 | **未实现 (整模块空)** |
| S-019 | `struct slabdata` | slaballoc.c:118 | slab 数据 | 跳过 | 跳过 |
| S-020 | `struct slabheader` (匿名) | slaballoc.c:123 | slab 头 | 跳过 | 跳过 |
| S-021 | `struct vfs_request_node` (匿名) | vfs.c:33 | VFS 请求节点 | `VfsRequest` | 已实现 (vfs_queue.rs) |
| S-022 | `struct vmproc` | vmproc.h:14 | VM 进程 | `VmProc` | 已实现 (vmproc/vmproc.rs) |

**结构体覆盖小结**: 14/22 = 64% 已实现。8 个未实现: 2 个死代码 (PhysBlock, PhysRegion), 5 个 slab 相关, 1 个宏代码模板 (ALLREGIONS)。

---

## 4. 函数覆盖 (按源文件)

### 4.1 acl.c (5 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-001 | `acl_init` | acl.c:21 | ACL 初始化 | `AclState::default()` | 已实现 (acl.rs) |
| F-002 | `acl_check` | acl.c:37 | ACL 检查 | `AclState::acl_check(&self, endpoint, call)` (acl.rs) | 已实现; C-11 安全修复: `Uninitialized` 限制为 DEFAULT 权限; P1-10 已修复: 签名取 `Endpoint` 而非 `&ActiveProc` |
| F-003 | `acl_set` | acl.c:70 | 设置 ACL | `VmProc::set_acl` (vmproc_handle.rs) | 已实现 |
| F-004 | `acl_fork` | acl.c:110 | 派生 ACL | `VmProc::acl_fork` (vmproc_handle.rs) | 已实现 |
| F-005 | `acl_clear` | acl.c:120 | 清除 ACL | `VmProc::clear_acl` | 已实现 |

### 4.2 alloc.c (20 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-006 | `reservedqueue_new` | alloc.c:100 | 建保留队列 | `ReservedPages::new` | 已实现 (alloc_page.rs) |
| F-007 | `reservedqueue_fillslot` | alloc.c:137 | 填队列槽 (static) | `ReservedPages::fill_slot` | 已实现 (私有) |
| F-008 | `reservedqueue_addslot` | alloc.c:149 | 增槽 (static) | `ReservedPages::add_slot` | 已实现 (私有) |
| F-009 | `reservedqueue_add` | alloc.c:179 | 加入队列 | `ReservedPages::add` | 已实现 (alloc_page.rs) |
| F-010 | `reservedqueue_fill` | alloc.c:191 | 填充队列 (static) | `ReservedPages::fill` | 已实现 (私有) |
| F-011 | `reservedqueue_alloc` | alloc.c:206 | 队列分配 | `ReservedPages::alloc` | 已实现 (alloc_page.rs) |
| F-012 | `alloc_cycle` | alloc.c:227 | 分配循环 | `VmServer::alloc_cycle` + `missing_spares` 压力计数 | 已实现（主循环接线，补充体 DEFERRED 归 24；06-page-allocator.md §3.3/§4.3） |
| F-013 | `alloc_mem` | alloc.c:242 | 分配内存 | `PhysAllocator::alloc_pages` | 已实现 (phys_mem/*) |
| F-014 | `mem_add_total_pages` | alloc.c:281 | 加总页数 | `VmPageAllocator::add_total_pages` | 已实现 |
| F-015 | `free_mem` | alloc.c:289 | 释放内存 | `PhysAllocator::free_pages` | 已实现 (phys_mem/*) |
| F-016 | `mem_init` | alloc.c:306 | 内存初始化 | `PhysAllocator::init` | 已实现 (phys_mem/mod.rs) |
| F-017 | `mem_sanitycheck` | alloc.c:338 | 健全检查 | 跳过 (cfg 控制) | 跳过 |
| F-018 | `memstats` | alloc.c:348 | 内存统计 | `MemStats::collect` | 已实现 (phys_mem/stats.rs) |
| F-019 | `alloc_pages` | alloc.c:404 | 分配页 (static) | 私有辅助 | 已实现 (内部) |
| F-019a | `find_bit` | (新增) | 按位查找 free run | `BitmapAllocator::find_bit` | ✅ **已文档化+测试**: doc 注释明确 O(chunks_with_used_bits) — 严格优于 Minix3 alloc.c:175-192 的 O(n). 3 个优化路径 (last-found hint / BMI BLSR+TZCNT / 位图反转) 写入 doc 但不实现. 3 个单元测试覆盖 backward scan + run-length 累加 + used-bit skip. 27 bitmap_alloc tests pass |
| F-020 | `free_pages` | alloc.c:465 | 释放页 (static) | 私有辅助 | 已实现 (内部) |
| F-021 | `printmemstats` | alloc.c:486 | 打印内存统计 | `MemStats::display` | 已实现 |
| F-022 | `usedpages_reset` | alloc.c:501 | 重置已用表 | `alloc_stats::reset` | 已实现 (alloc_stats.rs) |
| F-023 | `usedpages_add_f` | alloc.c:509 | 记录已用页 | `alloc_stats::record_alloc` | 已实现 |
| F-024 | `sanitycheck_queues` | alloc.c:76 | 队列检查 (static) | 跳过 (cfg 控制) | 跳过 |
| F-025 | `sanitycheck_rq` | alloc.c:90 | 单队列检查 (static) | 跳过 | 跳过 |

### 4.3 break.c (2 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-026 | `do_brk` | break.c:44 | BRK 系统调用 | `brk::handle_brk` | 已实现 (brk.rs:55) |
| F-027 | `real_brk` | break.c:62 | 实际 BRK 扩展 | `BrkRequest::apply` | 已实现 (brk.rs) |

### 4.4 cache.c (14 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-028 | `lru_rm` | cache.c:29 | LRU 移除 (static) | `LruList::unlink`（page_cache.rs 内部） | 已实现 |
| F-029 | `lru_add` | cache.c:52 | LRU 加入 (static) | `LruList::link_tail`（page_cache.rs 内部） | 已实现 |
| F-030 | `cache_lru_touch` | cache.c:70 | 触碰 LRU | `LruList::touch` | 已实现 (page_cache.rs，O(1) 索引双链) |
| F-031 | `makehash` | cache.c:76 | 哈希值 (static inline) | BTreeMap 键 `(dev, dev_offset)` / `(dev, ino, ino_offset)`（`[ARCH: A-4]`，无哈希函数） | 已实现 |
| F-032 | `cache_sanitycheck_internal` | cache.c:87 | 缓存健全检查 | 跳过 (cfg) | 跳过 |
| F-033 | `addcache_byino` | cache.c:155 | 加 ino 哈希 (static) | `PageCache::by_ino` 辅索引（`addcache` 内联写入） | 已实现 |
| F-034 | `update_inohash` | cache.c:164 | 更新 ino 哈希 (static) | `PageCache::find_by_dev` 惰性 ino 更新 | 已实现 |
| F-035 | `find_cached_page_bydev` | cache.c:176 | 按 dev 查找 | `PageCache::find_by_dev` | 已实现 (page_cache.rs) |
| F-036 | `find_cached_page_byino` | cache.c:198 | 按 ino 查找 | `PageCache::find_by_ino` | 已实现 |
| F-037 | `addcache` | cache.c:216 | 加缓存块 | `PageCache::addcache`（IN_CACHE + 重复键 fail-closed） | 已实现 (page_cache.rs) |
| F-038 | `rmcache` | cache.c:259 | 移除缓存块 | `PageCache::rmcache`（refcount==0 → `free_pfn`） | 已实现 |
| F-039 | `cache_freepages` | cache.c:288 | 释放缓存页 | `PageCache::free_pages`（LRU 最老端扫描 refcount==1）+ `VmServer::alloc_cycle` 接线（`FREE_CACHE_BATCH=1024`，近似 alloc.c 的 `cache_freepages(clicks)`（按请求量；Rust 用固定批次 1024）） | 已实现 (2026-08-16 接线) |
| F-040 | `clear_cache_bydev` | cache.c:313 | 清设备缓存 | `PageCache::clear_by_dev` | 已实现 (page_cache.rs) |
| F-041 | `get_stats_info` | cache.c:328 | 取统计信息 | `PageCache::total_cached` | 已实现 |

### 4.5 exit.c (6 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-042 | `reset_vm_rusage` | exit.c:25 | 重置 rusage (static) | `VmProc::reset_rusage` | 已实现 (vmproc.rs) |
| F-043 | `free_proc` | exit.c:33 | 释放进程 | `VmProc::clear` | 已实现 (vmproc.rs:152) |
| F-044 | `clear_proc` | exit.c:45 | 清进程 | `VmProc::force_clear` | 已实现 (vmproc.rs) |
| F-045 | `do_exit` | exit.c:60 | 进程退出 | `exit::handle_vm_exit` | 已实现 (exit.rs:42) |
| F-046 | `do_willexit` | exit.c:100 | 即将退出 | `exit::handle_vm_willexit` | 已实现 (exit.rs:66) |
| F-047 | `do_procctl` | exit.c:117 | 进程控制 | `dispatch_procctl` | ✅ **已完整实现 (2026-08-16)**: VMPPARAM_CLEAR — free_proc+pt_new+pt_bind 等价 (释放物理页+清空regions+重建页表+绑定), RS/VFS 权限检查 (EPERM); VMPPARAM_HANDLEMEM — handle_memory_once 同步路径 (CoW 解析+页映射; 文件后备区域→NotImplemented), VFS 权限检查; 未知 param→EINVAL。新增 `exit::handle_procctl_clear` + `handle_procctl_handlemem` + `VmProcctlError` + `VmProcctlHandlememResult`; wire format 用 `MessLcVmProcctl` m9 overlay (VMPCTL_* = m9_l1..m9_l5, 22-P1-1); errno: InvalidEndpoint→EINVAL (22-P1-2); 11 个测试覆盖 (dispatcher 5 + exit.rs procctl 5 + vm.rs decode 1) |

### 4.6 fdref.c (5 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-048 | `fdref_sanitycheck` | fdref.c:37 | fd 健全检查 | 跳过 (cfg) | 跳过 |
| F-049 | `fdref_new` | fdref.c:93 | 新建 fdref | `FdRefTable::create` + `dedup_or_new` | 已实现 (fdref.rs:78/:114；create 的 may_close 参数已于 23-P0-1c 移除，dedup 四态接管，2026-08-16) |
| F-050 | `fdref_ref` | fdref.c:109 | 增加引用 | `FdRefTable::ref_entry` | 已实现 (fdref.rs:151，2026-08-16 行号修正) |
| F-051 | `fdref_deref` | fdref.c:116 | 减少引用 | `FdRefTable::deref_entry` | 已实现 (fdref.rs:157，2026-08-16 行号修正) + **23-P0-1b（2026-08-16）**: 删除 entry 上 may_close 字段——refcount==0 **无条件**返回 `PendingFdClose`（对齐 fdref.c:150-153 最后引用总关 fd）；mayclosefd 只作用于 dedup 路径。+ ✅ find_by_dev_ino O(1) 反索引（dev_ino_index，deref 归零清理）+ 3 测试覆盖。+ 新增 `test_fdref_deref_always_closes_at_zero`（:261） |
| F-052 | `fdref_dedup_or_new` | fdref.c:156 | 去重或新建 | `FdRefTable::dedup_or_new` | 已实现 (fdref.rs:114-149，2026-08-16) + **23-P0-1c**: 四态完整对齐 C（同 fd 复用 / 异 fd 同 (dev,ino) may_close 关新 fd / !may_close 继续扫精确 fd / 无匹配新建）；BTreeMap 反向迭代 = C 最近优先扫描；5 个 `test_dedup_or_new_*` 覆盖 |

### 4.7 fork.c (1 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-053 | `do_fork` | fork.c:32 | 处理 fork | `fork::do_fork` (fork.rs:191) + `vm_server::handle_fork` (vm_server.rs:534) | 已实现+ (2026-06-14: 新增 pfn_alloc 参数 + handle_memory_once 函数; do_fork 中 handle_memory_once 调用待 VmProcTable split borrow) + ✅ **SAFETY 注释已补全**: fork.rs:225-326 共 9 处 SAFETY 注释, 每处显式列出: 1) Active typestate 前置条件; 2) single-threaded VM 合约; 3) parent→dst_regions→frames refcount 数据流; 4) 回滚点 (2026-09-04 起为 write_page_table_mappings——bind 为 no-op 已随 D8-④ 删除). unsafe 块 (free_page_table rollback / setup_cow_for_all_regions / write_page_table_mappings) 均有独立编号的 SAFETY 段落 + **handle_memory_once DEFERRED 详细文档化**: 1) Dependency-1 VmProcTable 不支持双 slot 同时可变借用; 2) Dependency-2 `sys_fork` 是 stub, 未返回 kernel-assigned msgaddr; 3) 安全依据: C `fork.c:97-108` 自承 "optimisation", 若未做只是产生一次性 page fault 而非 deadlock (子进程可异步处理自己的 page fault) |

### 4.8 main.c (17 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-054 | `is_first_time` | main.c:79 | 是否首启 (static) | `VmServer::init_fresh` | 已实现 |
| F-055 | `main` | main.c:93 | 主函数 | `main::main` (main.rs) | 已实现 |
| F-056 | `sef_cb_lu_state_changed` | main.c:196 | LU 状态变化 (static) | `VmServer::on_lu_state_changed` | **未实现 (NotImplemented)** |
| F-057 | `sef_local_startup` | main.c:219 | SEF 本地启动 (static) | `VmServer::local_startup` | 已实现 |
| F-058 | `sef_cb_init_fresh` | main.c:241 | SEF 全新初始化 (static) | `VmServer::init_fresh` | 已实现 |
| F-059 | `init_proc` | main.c:262 | 初始化进程 (static) | `VmServer::init_proc` | 已实现 |
| F-060 | `libexec_copy_physcopy` | main.c:294 | libexec 物理复制 (static) | `VmServer::libexec_copy` | 已实现 |
| F-061 | `boot_alloc` | main.c:305 | 启动分配 (static) | `BumpBuf` (heap_arena.rs) | 已实现 |
| F-062 | `libexec_alloc_vm_prealloc` | main.c:317 | libexec 预分配 (static) | `VmServer::libexec_prealloc` | 已实现 |
| F-063 | `libexec_alloc_vm_ondemand` | main.c:324 | libexec 按需 (static) | `VmServer::libexec_ondemand` | 已实现 |
| F-064 | `exec_bootproc` | main.c:331 | 执行启动进程 (static) | `VmServer::exec_bootproc` | **未实现 (TODO)** |
| F-065 | `do_procctl_notrans` | main.c:419 | 无事务 procctl (static) | 跳过 (走 dispatcher) | 跳过 |
| F-066 | `init_vm` | main.c:428 | VM 初始化 | `VmServer::init` (vm_server.rs:140) | 已实现 |
| F-067 | `sef_cb_init_vm_multi_lu` | main.c:592 | 多组件 LU 初始化 (static) | **未实现** | TODO |
| F-068 | `sef_cb_init_lu_restart` | main.c:677 | LU 重启 (static) | **未实现** | TODO |
| F-069 | `sef_cb_signal_handler` | main.c:731 | 信号处理 (static) | `VmServer::handle_signal` (vm_server.rs:307) | **stub** (no-op) |
| F-070 | `map_service` | main.c:755 | 映射服务 (static) | 集成在 dispatcher | 已实现 |

### 4.9 mem_*.c (memtype 回调族)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-071 | `mem_type_anon` (instance) | mem_anon.c:34 | 匿名内存类型表 | `AnonymousMemory` + `MEM_TYPE_ANON` (memtype.rs:692) | 已实现 |
| F-072 | `anon_pt_flags` | mem_anon.c:48 | 页表标志 (static) | `AnonymousMemory::pt_flags` | 已实现 |
| F-073 | `anon_unreference` | mem_anon.c:56 | 解除引用 (static) | `AnonymousMemory::ev_unreference` | 已实现 |
| F-074 | `anon_pagefault` | mem_anon.c:64 | 缺页处理 (static) | `AnonymousMemory::ev_pagefault` | 已实现 |
| F-075 | `anon_sanitycheck` | mem_anon.c:99 | 健全检查 (static) | 跳过 (cfg) | 跳过 |
| F-076 | `anon_writable` | mem_anon.c:105 | 可写判定 (static) | `AnonymousMemory::writable` | 已实现 |
| F-077 | `anon_resize` | mem_anon.c:115 | 调整大小 (static) | `AnonymousMemory::ev_resize` | 已实现 |
| F-078 | `anon_regionid` | mem_anon.c:132 | 区 ID (static) | `AnonymousMemory::regionid` | 已实现 |
| F-079 | `anon_lowshrink` | mem_anon.c:137 | 低端收缩 (static) | `AnonymousMemory::ev_lowshrink` | 已实现 |
| F-080 | `anon_refcount` | mem_anon.c:142 | 引用计数 (static) | `AnonymousMemory::refcount` | 已实现 |
| F-081 | `anon_split` | mem_anon.c:147 | 区分 (static) | `AnonymousMemory::ev_split` | 已实现 |
| F-082 | `mem_type_anon_contig` | mem_anon_contig.c:24 | 连续内存类型表 | `ContiguousAnonymous` + `MEM_TYPE_CONTIG_ANON` (memtype.rs:695) | 已实现 |
| F-083 | `anon_contig_pt_flags` | mem_anon_contig.c:37 | 页表标志 (static) | `ContiguousAnonymous::pt_flags` | 已实现 |
| F-084 | `anon_contig_pagefault` | mem_anon_contig.c:45 | 缺页 (static) | `ContiguousAnonymous::ev_pagefault` | ✅ 已修复 (2026-06-14): panic (与 C 一致: "pagefault cannot happen") |
| F-085 | `anon_contig_new` | mem_anon_contig.c:52 | 新建 (static) | `ContiguousAnonymous::ev_new` | ✅ 已修复 (2026-06-14): 连续 PFN 分配 + 连续性验证 + map_page; ev_new 签名扩展 (region, frames, alloc); 遗留: PfnAllocator 需 alloc_contiguous() |
| F-086 | `anon_contig_resize` | mem_anon_contig.c:97 | 调整 (static) | `ContiguousAnonymous::ev_resize` | 已实现 (返 ENOMEM) |
| F-087 | `anon_contig_reference/unreference/sanitycheck/writable/split` | mem_anon_contig.c:103-127 | 其他回调 (static) | 对应 ev_* 方法 | 已实现 |
| F-088 | `mem_type_directphys` | mem_directphys.c:28 | 直映射类型表 | `DirectPhysical` + `MEM_TYPE_DIRECT` (memtype.rs:693) | 已实现 |
| F-089 | `phys_pt_flags/unreference/pagefault/writable` | mem_directphys.c:37-69 | 回调 (static) | `DirectPhysical::ev_*` | 已实现 |
| F-090 | `phys_setphys` | mem_directphys.c:69 | 设置物理地址 | `DirectPhysical::ev_setphys` | 已实现 |
| F-091 | `mem_type_cache` | mem_cache.c:39 | 缓存类型表 | `CacheMemory` + `MEM_TYPE_CACHE` (memtype.rs:696) | 已实现 |
| F-092-F-099 | cache_* (10 callbacks) | mem_cache.c:51-95 | 缓存回调 (static) | `CacheMemory::ev_*` | 已实现 |
| F-100 | `do_mapcache` | mem_cache.c:95 | 映射缓存 | `dispatch_mapcache` (dispatcher.rs:438) | ✅ 已修复 (2026-06-16): 完整实现 C do_mapcache 语义 — 对齐验证 + endpoint 验证 + mmap 区分配 VirRegion(MEM_TYPE_CACHE) + 逐页 PageCache 查找 + map_page 直接映射 + 失败中途 unmap 清理 + 返回 vaddr; VmError::NotFound(ENOENT) 新增; 4 个测试覆盖 |
| F-101 | `cache_pagefault` | mem_cache.c:181 | 缺页 (static) | `CacheMemory::ev_pagefault` (memtype.rs:791) | ✅ 已修复 (2026-06-16): 完整实现 C cache_pagefault 语义 — PbCache PFN 映射 + 缓存指针清除; pfn==0 返回 InvalidParam; 已映射页返回 Handled; 4 个测试覆盖 |
| F-102 | `do_setcache` | mem_cache.c:196 | 设置缓存 | `dispatch_setcache` (dispatcher.rs:499) | ✅ 已修复 (2026-06-16): 完整实现 C do_setcache 语义 — endpoint 验证 + region 查找 + 匿名内存验证 + refcount==1 检查 + memtype 改为 cache + PageCache 插入; 已有缓存条目处理(相同页跳过/不同页替换); 4 个测试覆盖 |
| F-103 | `do_forgetcache` | mem_cache.c:283 | 忘记缓存 | `dispatch_forgetcache` | ✅ **已实现 + C 语义对齐输入验证 (2026-06-16)**: `pages==0` → InvalidAddress (C: EINVAL); `dev_offset%4096!=0` → InvalidAddress (C: EFAULT). 3 个测试覆盖 (zero-pages / unaligned-offset / valid-returns-Ok) |
| F-104 | `do_clearcache` | mem_cache.c:315 | 清缓存 | `dispatch_clearcache` | ✅ **已实现 (2026-06-16)**: 调用 `cache.clear_by_dev(dev, frames)` 与 C `clear_cache_bydev(dev)` 语义对齐. C 无输入验证, Rust 亦无 |
| F-105 | `mem_type_mappedfile` | mem_file.c:30 | 文件映射类型表 | `MappedFile` + `MEM_TYPE_MAPPED_FILE` (memtype.rs:919/:1133) | 已实现（2026-08-16 行号修正） |
| F-106-F-117 | mappedfile_* (12 callbacks) | mem_file.c:43-280 | 文件映射回调 (static) | `MappedFile::ev_*` | 已实现 (ev_copy/split 简化) |
| F-118 | `mappedfile_setfile` | mem_file.c:191 | 设置文件 | `MappedFile::ev_setfile` | 已实现 |
| F-119 | `mem_type_shared` | mem_shared.c:28 | 共享类型表 | `SharedMemory` + `MEM_TYPE_SHARED` (memtype.rs:694) | 已实现 |
| F-120-F-130 | shared_* (11 callbacks) | mem_shared.c:41-207 | 共享回调 (static) | `SharedMemory::ev_*` | ✅ 已修复 (2026-06-16): ev_pagefault 完整实现 (getsrc→vm_isokendpt→map_lookup→源页映射→PFN共享); 签名增加 &VmProcTable + &mut PfnAllocator; MemTypeError 新增 InvalidProcess/InvalidAddress; 3 个测试覆盖 |
| F-131 | `shared_setsource` | mem_shared.c:167 | 设置源 | `SharedMemory::ev_setsource` | 已实现 (stub) |

### 4.10 mmap.c (12 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-132 | `mmap_region` | mmap.c:36 | 映射区 (static) | `mmap::mmap_region` (mmap.rs:226) | 已实现 |
| F-133 | `mmap_file` | mmap.c:84 | 映射文件 (static) | `mmap::mmap_file` | 已实现 |
| F-134 | `do_vfs_mmap` | mmap.c:135 | VFS 文件映射 | `mmap::handle_vfs_mmap` (mmap.rs:475) + `dispatch_vfs_mmap` (dispatcher.rs:411) | 已实现（20 轮：enable_filemap 守卫 + mmap_file + MVM_WRITABLE） |
| F-135 | `mmap_file_cont` | mmap.c:160 | 映射续作 (static) | `mmap::mmap_file_cont` (mmap.rs:525) | 已实现 |
| F-136 | `do_mmap` | mmap.c:200 | mmap 处理 | `mmap::handle_mmap` (mmap.rs:268) + `vm_server::handle_mmap` (vm_server.rs:1226) | 已实现 |
| F-137 | `map_perm_check` | mmap.c:284 | 权限检查 (static) | `map_phys::map_perm_check` (map_phys.rs:106) | 已实现 |
| F-138 | `do_map_phys` | mmap.c:310 | 映射物理 | `map_phys::handle_map_phys` (map_phys.rs:48) + `vm_server::handle_map_phys` (vm_server.rs:1232) | 已实现 |
| F-139 | `do_remap` | mmap.c:366 | 重映射 | `dispatch_remap`/`dispatch_remap_ro` → `dispatch_remap_impl` (dispatcher.rs:1347) | 已实现（20 轮：wire format + destination 字段 + errno 修复） |
| F-140 | `do_get_phys` | mmap.c:438 | 取物理地址 | `query::handle_get_phys` | 已实现 (query.rs) |
| F-141 | `do_get_refcount` | mmap.c:463 | 取引用数 | `query::handle_get_refcount` | 已实现 (query.rs) |
| F-142 | `munmap_vm_lin` | mmap.c:488 | 取消线性映射 | `munmap::munmap_vm_lin` (munmap.rs:103) | 已实现 |
| F-143 | `do_munmap` | mmap.c:512 | munmap 处理 | `munmap::handle_munmap` (munmap.rs:71) | 已实现 |

### 4.11 pagefaults.c (10 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-144 | `pf_errstr` | pagefaults.c:59 | 错误字符串 | `PageFaultError::description` | 已实现 |
| F-145 | `handle_pagefault` | pagefaults.c:76 | 处理缺页 (static) | `cow_exec_pf::handle_pagefault` (cow_exec_pf.rs:16) + `vm_server::dispatch_pagefault` (vm_server.rs:336) | 已实现 |
| F-146 | `pf_cont` | pagefaults.c:161 | 缺页续作 (static) | `cow_exec_pf::pagefault_continue` | 已实现 |
| F-147 | `handle_memory_continue` | pagefaults.c:170 | 内存续作 (static) | `cow_exec_pf::memory_continue` | 已实现 |
| F-148 | `handle_memory_final` | pagefaults.c:198 | 内存收尾 (static) | `cow_exec_pf::memory_final` | 已实现 |
| F-149 | `do_pagefaults` | pagefaults.c:240 | 缺页入口 | `vm_server::dispatch_pagefault` | 已实现 |
| F-150 | `handle_memory_once` | pagefaults.c:245 | 一次性处理 | `fork::handle_memory_once` (fork.rs:34) | ✅ **已实现 (2026-06-14)** — 逐页遍历 + CoW 解析 + 3 个测试; do_fork 调用待 VmProcTable split borrow |
| F-151 | `handle_memory_start` | pagefaults.c:254 | 启动处理 | `cow_exec_pf::memory_start` | 已实现 |
| F-152 | `do_memory` | pagefaults.c:294 | 内存请求循环 | `VmServer::handle_signal` (vm_server.rs:307) | **stub (no-op)** — TODO |
| F-153 | `handle_memory_step` | pagefaults.c:336 | 内存步骤 (static) | `cow_exec_pf::memory_step` | 已实现 |

### 4.12 pagetable.c (28 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-154 | `pt_assert` | pagetable.c:115 | 断言页表 | 跳过 (debug_assert) | 跳过 |
| F-155 | `pt_sanitycheck` | pagetable.c:130 | 页表健全 | 跳过 (cfg) | 跳过 |
| F-156 | `findhole` | pagetable.c:155 | 找空位 (static) | `RegionMap::find_slot` (region_map.rs:145) | 已实现 |
| F-157 | `vm_freepages` | pagetable.c:235 | 释放页 | `PhysAllocator::free_pages` | 已实现 |
| F-158 | `vm_getsparepage` | pagetable.c:264 | 取备用页 (static) | 无（Direct Map `[ARCH: A-1]` 结构消除；`vm_pt_alloc` 直取分配器） | 结构消除（06-page-allocator.md §3.3） |
| F-159 | `vm_getsparepagedir` | pagetable.c:277 | 取备用页目录 (static) | 无（Direct Map `[ARCH: A-1]` 结构消除） | 结构消除（06-page-allocator.md §3.3） |
| F-160 | `vm_mappages` | pagetable.c:295 | 映射页 | `vm_self_mappages` | 已实现 (vm_self_map.rs) |
| F-161 | `vm_allocpages` | pagetable.c:333 | 分配多页 | `VmPageAllocator::alloc_pages` | 已实现 (alloc_page.rs) |
| F-162 | `vm_allocpage` | pagetable.c:395 | 分配单页 | `VmPageAllocator::alloc_pfn` | 已实现 |
| F-163 | `vm_pagelock` | pagetable.c:403 | 锁定页 | `PageFlags::LOCKED` | 跳过 (无 Rust 映射) |
| F-164 | `vm_addrok` | pagetable.c:440 | 地址合法 | `PageTable::is_valid_address` | 已实现 (arch crate) |
| F-165 | `pt_ptalloc` | pagetable.c:494 | 分配页表 (static) | `PageTable::alloc_pt_page` | 已实现 (arch crate) |
| F-166 | `pt_ptalloc_in_range` | pagetable.c:545 | 范围内分配页表 | `PageTable::alloc_pt_page_in_range` | 已实现 (arch crate) |
| F-167 | `pt_map_in_range` | pagetable.c:631 | 范围内映射 | `PageTable::map_in_range` | 已实现 (arch crate) |
| F-168 | `pt_ptmap` | pagetable.c:685 | 页表映射 | `PageTable::map_kernel` | 已实现 (arch crate) |
| F-169 | `pt_clearmapcache` | pagetable.c:751 | 清映射缓存 | **设计差异：Direct Map 下消除** — Rust 使用 kernel direct map 直接访问页目录, 无需缓存 PDE, 此操作无意义 |
| F-170 | `pt_writable` | pagetable.c:761 | 可写判定 | `PageTable::is_writable` | 已实现 (arch crate) |
| F-171 | `pt_writemap` | pagetable.c:784 | 写入映射 | `PageTable::writemap` | 已实现 (arch crate) |
| F-172 | `pt_checkrange` | pagetable.c:943 | 范围检查 | `PageTable::check_range` | 已实现 (arch crate) |
| F-173 | `pt_new` | pagetable.c:990 | 新建页表 | `PageTable::new` | 已实现 (arch crate) |
| F-174 | `freepde` | pagetable.c:1028 | 分配 PDE (static) | i386 freepdes 临时窗口/登记册机制，64 位 DM 下被替代（08 §2.11） | 结构性消除 |
| F-175 | `pt_allocate_kernel_mapped_pagetables` | pagetable.c:1035 | 分配内核页表 | `pagedir_mappings` 登记册被 DM 访问替代（08 §2.11） | 结构性消除 |
| F-176 | `pt_copy` | pagetable.c:1069 | 复制页表 (static) | `clone_range`（paging.rs:453，08 §3.5 D5） | 已实现 |
| F-177 | `pt_init` | pagetable.c:1088 | 页表初始化 | 语义三通道分解：`establish_boot_dm`（kernel dm_coverage.rs:66）+ `VmSelfPageTable::adopt`（vm_self_map.rs:95）+ `VmCtlParam::SetAddrSpace`（syscall.rs:2003）（08 §3.3） | 结构性分解 |
| F-178 | `pt_bind` | pagetable.c:1358 | 绑定页表 | 无独立 bind API——根登记走 `VmCtlParam::SetAddrSpace`（syscall.rs:2003），VM 自身根经 `VmSelfPageTable::adopt`（vm_self_map.rs:95） | 结构性消除（08 §3.3 D7 裁决） |
| F-179 | `pt_free` | pagetable.c:1427 | 释放页表 | `ActiveProc::free_page_table` (vmproc_handle.rs:411) | 已实现 |
| F-180 | `pt_mapkernel` | pagetable.c:1442 | 映射内核 | `PageTable::map_kernel` | 已实现 (arch crate) |
| F-181 | `get_vm_self_pages` | pagetable.c:1500 | 取 VM 页数 | `VmAllocStats::self_page_count`（06 D4） | 已实现（06-page-allocator.md §3.4） |

### 4.13 pb.c (6 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-182 | `pb_new` | pb.c:32 | 新建物理块 | `PhysFrames::new_block` | 已实现 (page_state.rs) |
| F-183 | `pb_free` | pb.c:54 | 释放物理块 | `PhysFrames::free_block` | 已实现 |
| F-184 | `pb_link` | pb.c:61 | 链接物理块 | `VirRegion::map_page` (mapped to PageSlot) | 已实现 (vir_region.rs) |
| F-185 | `pb_reference` | pb.c:73 | 引用物理块 | `PhysFrames::add_ref` | 已实现 |
| F-186 | `pb_unreferenced` | pb.c:96 | 解除引用 | `PhysFrames::release_ref` | 已实现 |
| F-187 | `mem_cow` | pb.c:136 | 写时复制 | `cow_exec_pf::cow_resolve` (cow_exec_pf.rs:68) | 已实现 |

### 4.14 region.c (40 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-188 | `map_region_init` | region.c:36 | 区域初始化 | `RegionMap::new` | 已实现 (region_map.rs:40) |
| F-189 | `map_printregion` | region.c:40 | 打印区 (static) | 跳过 (debug) | 跳过 |
| F-190 | `physblock_get` | region.c:60 | 取物理块 | **死代码** | 死代码 (region/phys_region.rs) |
| F-191 | `physblock_set` | region.c:72 | 设物理块 | **死代码** | 死代码 |
| F-192 | `map_printmap` | region.c:98 | 打印映射 | 跳过 (debug) | 跳过 |
| F-193 | `getnextvr` | region.c:112 | 下一区 (static) | `RegionMap::next` | 已实现 |
| F-194 | `pr_writable` | region.c:130 | 区可写 (static) | `PhysRegion::writable` | 已实现 |
| F-195 | `map_sanitycheck_pt` | region.c:141 | 页表健全 (static) | 跳过 (cfg) | 跳过 |
| F-196 | `map_sanitycheck` | region.c:168 | 区域健全 | `sanity::verify_refcounts` | ✅ Implemented | BTreeMap 替代 seencount; for_each_active_region 替代 ALLREGIONS 宏 |
| F-197 | `map_ph_writept` | region.c:257 | 写物理页表 | `VirRegion::writept` | 已实现 (vir_region.rs) |
| F-198 | `region_find_slot_range` | region.c:302 | 找槽范围 (static) | `RegionMap::find_slot` | 已实现 |
| F-199 | `region_find_slot` | region.c:399 | 找槽 (static) | `RegionMap::find_slot` | 已实现 |
| F-200 | `phys_slot` | region.c:418 | 物理槽数 (static) | 私有辅助 | 已实现 |
| F-201 | `region_new` | region.c:424 | 新建区 (static) | `VirRegion::new` | 已实现 (vir_region.rs) |
| F-202 | `map_page_region` | region.c:463 | 映射区 | `VirRegion::create` | 已实现 |
| F-203 | `map_subfree` | region.c:527 | 子释放 (static) | `free_region_pages` | 已实现 (region/mod.rs:17) |
| F-204 | `map_free` | region.c:568 | 释放区 | `VirRegion::free` | 已实现 |
| F-205 | `map_free_proc` | region.c:589 | 释放进程区 | `VmProc::clear` (vmproc.rs:152) | 已实现 |
| F-206 | `map_lookup` | region.c:616 | 查映射 | `RegionMap::find` | 已实现 (region_map.rs:54) |
| F-207 | `vrallocflags` | region.c:645 | 分配标志 | `VrFlags::to_alloc_flags` | 已实现 |
| F-208 | `map_pf` | region.c:664 | 缺页 | `cow_exec_pf::handle_pagefault` | 已实现 |
| F-209 | `map_handle_memory` | region.c:756 | 内存处理 | `cow_exec_pf::memory_handle` | 已实现 |
| F-210 | `map_pin_memory` | region.c:779 | 钉住内存 | `region::map_pin_memory` (region/mod.rs:107) | ✅ Done (2026-06-16) — 两阶段收集+handle_memory_once(wrflag=true); PinMemoryError::PageNotMapped; 2 个测试覆盖 |
| F-211 | `map_copy_region` | region.c:802 | 复制区 | `VirRegion::clone` / `fork_region` | 已实现 (fork.rs) |
| F-212 | `copy_abs2region` | region.c:860 | 物理到区 | `VirRegion::copy_from_phys` | 已实现 (mappedfile_copy) |
| F-213 | `map_writept` | region.c:906 | 写页表 | `VmProc::write_page_table_mappings` (vmproc_handle.rs:391) | 已实现 |
| F-214 | `map_proc_copy` | region.c:933 | 复制进程区 | `VmProc::fork_regions` (fork.rs) | 已实现 |
| F-215 | `map_proc_copy_range` | region.c:944 | 范围复制 | `VmProc::fork_regions_range` | 已实现 |
| F-216 | `map_region_extend_upto_v` | region.c:1002 | 扩展到地址 | `VirRegion::extend_to` | 已实现 (brk.rs) |
| F-217 | `map_unmap_region` | region.c:1065 | 解除区 | `munmap::unmap_range` (munmap.rs:90) | 已实现 |
| F-218 | `split_region` | region.c:1150 | 分割区 (static) | `VirRegion::split` | 已实现 (vir_region.rs) |
| F-219 | `map_unmap_range` | region.c:1222 | 解除范围 | `munmap::unmap_range` | 已实现 |
| F-220 | `map_region_lookup_type` | region.c:1303 | 按类型查 | `RegionMap::find_by_type` | 已实现 |
| F-221 | `map_get_phys` | region.c:1323 | 取物理地址 | `query::handle_get_phys` (query.rs) | 已实现 |
| F-222 | `map_get_ref` | region.c:1343 | 取引用数 | `query::handle_get_refcount` (query.rs) | 已实现 |
| F-223 | `get_usage_info_kernel` | region.c:1357 | 内核使用信息 | `UsageSources.kernel_bytes`（vm_server.rs:813-826，query.rs Usage 特判） | 已实现 (query.rs) |
| F-224 | `get_usage_info_vm` | region.c:1366 | VM 使用信息 (static) | `UsageSources.vm_self_bytes`（vm_server.rs:813-826，ARCH: self_page_count） | 已实现 (query.rs) |
| F-225 | `is_stack_region` | region.c:1384 | 是否栈区 (static) | `RegionMap::is_stack_region` | 已实现 |
| F-226 | `get_usage_info` | region.c:1395 | 使用信息 | `query::handle_info` Usage分支 (query.rs) | **已修复 (2026-06-15)**: UsageInfo 与 C 的 `vm_usage_info` 对齐 (total/common/shared/virtual/mvirtual), 遍历 region+physblock 按 C 逻辑计算 (refcount>1→common, VR_SHARED→shared, unmapped stack→mvirtual扣减) |
| F-227 | `get_region_info` | region.c:1452 | 区信息 | `query::handle_info` | 已实现 (query.rs) |

### 4.15 rs.c (8 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-228 | `do_rs_set_priv` | rs.c:34 | 设权限 | `rs::handle_rs_set_priv` (rs.rs:133) | 已实现 |
| F-229 | `do_rs_prepare` | rs.c:71 | 准备 LU | `rs::handle_rs_prepare` (rs.rs:130) | ✅ 部分实现 (2026-06-16): endpoint 验证 + map_pin_memory(src+dst); real_brk/map_proc_dyn_data DEFERRED |
| F-230 | `do_rs_update` | rs.c:150 | 更新 LU | `rs::handle_rs_update` (rs.rs:212) | ✅ 部分实现 (2026-06-16): endpoint 验证 + RsUpdateFlags(ROLLBACK/NOMMAP) + PREALLOC_MAP 检查; sys_update/swap_proc_slot/swap_proc_dyn_data DEFERRED |
| F-231 | `rs_memctl_make_vm_instance` | rs.c:218 | 创建 VM 实例 (static) | `rs::memctl_make_instance` | 已实现 (rs.rs) |
| F-232 | `rs_memctl_heap_prealloc` | rs.c:281 | 堆预分配 (static) | `rs::memctl_heap_prealloc` | 已实现 |
| F-233 | `rs_memctl_map_prealloc` | rs.c:300 | 映射预分配 (static) | `rs::memctl_map_prealloc` | 已实现 |
| F-234 | `rs_memctl_get_prealloc_map` | rs.c:329 | 取预分配映射 (static) | `rs::memctl_get_prealloc` | 已实现 |
| F-235 | `do_rs_memctl` | rs.c:349 | RS 内存控制 | `rs::handle_rs_memctl` (rs.rs:208) | 已实现 |

### 4.16 slaballoc.c (10 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-236 | `newslabdata` | slaballoc.c:159 | 新建 slab 数据 (static) | **未实现** (整模块空) | TODO |
| F-237 | `checklist` | slaballoc.c:194 | 检查清单 (static) | 跳过 | 跳过 |
| F-238 | `slab_sanitycheck` | slaballoc.c:229 | slab 健全 | 跳过 (cfg) | 跳过 |
| F-239 | `slabsane_f` | slaballoc.c:240 | slab 合法 | 跳过 | 跳过 |
| F-240 | `slaballoc` | slaballoc.c:259 | slab 分配 | **未实现** | TODO |
| F-241 | `objstats` | slaballoc.c:344 | 对象统计 (static inline) | **未实现** | TODO |
| F-242 | `slabfree` | slaballoc.c:406 | slab 释放 | **未实现** | TODO |
| F-243 | `slablock` | slaballoc.c:464 | slab 加锁 | **未实现** | TODO |
| F-244 | `slabunlock` | slaballoc.c:483 | slab 解锁 | **未实现** | TODO |
| F-245 | `slabstats` | slaballoc.c:504 | slab 统计 | **未实现** | TODO |

**slab 整模块 (10 函数) 未实现** — `os/servers/vm/src/slab/` 目录为空。

### 4.17 utility.c (13 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-246 | `get_mem_chunks` | utility.c:44 | 取内存块 | `BootParams.mem_chunks`（`BOOT_INFO` 已删，V9-P3-1） | 已实现 (boot.rs) |
| F-247 | `vm_isokendpt` | utility.c:84 | 端点合法 | `Endpoint::is_valid` + `VmServer::vm_isokendpt` | ✅ **已实现 + From<EndpointError> impls 已完成**: `vmproc::table` 公开 re-export `EndpointError` enum; 7 个调用模块 (munmap/query/rs/mmap/brk/map_phys/exit) 全部实现 `impl From<EndpointError> for XxxError { fn from(_: EndpointError) -> Self { XxxError::ProcessNotFound } }`; 调用点统一为 `table.vm_isokendpt(endpoint)?` 风格 (Rust `?` 操作符)。保留分层决策: `QueryError::to_errno_for_rusage()` 仍是上下文相关 (getrusage vs 其他查询), `From<QueryError> for VmError` 处理常规路径。`query_rusage_error_to_vm_error` 仍由 dispatcher 调用 (该映射是 ESRCH vs EINVAL, 无法用 From 表达)。 |
| F-248 | `do_info` | utility.c:100 | 信息 | `query::handle_info` (query.rs) | **已修复 (2026-06-15)**: UsageInfo 与 C 的 `vm_usage_info` 对齐 (total/common/shared/virtual_total/mvirtual), 移除了不存在的 text/data/stack 字段. `shared` 按 C 逻辑计算 (refcount>1 + VR_SHARED flag), 不再按 `def_memtype.name()` 匹配. `handle_info` 新增 `frames: &PageFrames` 参数以查询 per-page refcount. |
| F-249 | `swap_proc_slot` | utility.c:188 | 交换进程槽 | `ActiveProc::swap_proc_slot` (vmproc_handle.rs:613) | 已实现 — **SAFETY 注释已补全 (2026-06-14)**: 4 大不变性 (distinct pointers / bitwise swap safety / no concurrent access / no hardware in-flight) + 显式 C 源引用 + 逐字段类型分析 + `debug_assert_ne!` 防御性 guard + 测试 `test_swap_proc_slot_preserves_identities` |
| F-250 | `transfer_mmap_regions` | utility.c:227 | 转移映射区 (static) | `ActiveProc::transfer_regions` | 已实现 |
| F-251 | `map_proc_dyn_data` | utility.c:283 | 复制动态数据 | `VmProc::copy_dyn_data` | 已实现 |
| F-252 | `swap_proc_dyn_data` | utility.c:312 | 交换动态数据 | `VmProc::swap_dyn_data` | 已实现 |
| F-253 | `mmap` | utility.c:361 | mmap libc 替换 | `mmap::handle_mmap` (mmap.rs) | 已实现 |
| F-254 | `munmap` | utility.c:376 | munmap libc 替换 | `munmap::handle_munmap` | 已实现 |
| F-255 | `_brk` | utility.c:385 | brk libc 替换 | `brk::handle_brk` | 已实现 |
| F-256 | `do_getrusage` | utility.c:426 | 取 rusage | `query::handle_getrusage` | 已实现 (部分) |
| F-257 | `adjust_proc_refs` | utility.c:477 | 调整引用 | `VmProc::adjust_refs` | 已实现 |

### 4.18 vfs.c (3 函数)

| # | C 函数 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|--------|---------|------|-----------|------|
| F-258 | `activate` | vfs.c:43 | 激活请求 (static) | `VfsRequestQueue::activate` | 已实现 (vfs_queue.rs) |
| F-259 | `vfs_request` | vfs.c:60 | VFS 请求 | `VfsRequestQueue::request` (vfs_queue.rs:107-124) | 已实现（2026-08-16 行号修正；max_queued=64 → QueueFull → ENOMEM；ID_MAX → next_id u32 wrapping；activate 串行激活，发送待 23-B1 transport） |
| F-260 | `do_vfs_reply` | vfs.c:109 | VFS 应答 | `dispatch_vfs_reply` (dispatcher.rs:337) | ✅ Done — reqid>0 校验 + VfsReply 构造 + VfsRequestQueue::handle_reply + 延迟回调执行 + VmReply::Suspend；**23-P0-1（2026-08-16）**: VM_VFS_REPLY 改走 `MessVmVfsReply` overlay decode（message.rs:1874 + vm.rs:586），`VfsReply.ino` 传真实 ino（原硬编码 0） |

### 4.19 regionavl.c / utility2.c 等其他源

> `regionavl.c` 提供的 AVL 树被 `BTreeMap` 替代 (设计差异, 见 S-003)
> 其他工具性源整合在 `utility.c` 中。

**函数覆盖小结**:
- 137 个非 static 函数中: 95 已实现, 42 未实现 (其中 10 是 slab 整模块, 5 是 Live Update/信号, 3 是 DMA(C 本身未实现), 24 是分散的 TODO)
- 实现率: 69% (核心业务已覆盖, 周边 + Live Update + 调试功能未覆盖)

---

## 5. memtype 回调覆盖 (6 类 × 15 回调 = 90)

| MemType | 实例 | 已实现 ev_* | Stub (TODO) | 实现率 |
|---------|------|------------|------------|--------|
| AnonymousMemory | MEM_TYPE_ANON | 14/15 | 0 | 93% |
| DirectPhysical | MEM_TYPE_DIRECT | 14/15 | 0 | 93% |
| ContiguousAnonymous | MEM_TYPE_CONTIG_ANON | 15/15 | 0 | 100% |
| CacheMemory | MEM_TYPE_CACHE | 13/15 | ev_pagefault, ev_resize (有限) | 87% |
| MappedFile | MEM_TYPE_MAPPED_FILE | 14/15 | ev_copy (简化) | 93% |
| SharedMemory | MEM_TYPE_SHARED | 13/15 | ev_pagefault (fail-closed) | 87% |
| **合计** | **6** | **83/90** | **7** | **92%** |

---

## 6. IPC Handler 覆盖 (25 个)

| # | IPC 调用 | C 函数 | Rust 实现 | 状态 |
|---|----------|--------|-----------|------|
| I-001 | VM_FORK | fork.c:32 | `vm_server::handle_fork` | 已实现 (sys_fork 返回合成 Endpoint) |
| I-002 | VM_BRK | break.c:44 | `vm_server::handle_brk` | 已实现 |
| I-003 | VM_MMAP | mmap.c:200 | `vm_server::handle_mmap` | 已实现 |
| I-004 | VM_MUNMAP | mmap.c:512 | `munmap::handle_munmap` | 已实现 |
| I-005 | VM_MAP_PHYS | mmap.c:310 | `vm_server::handle_map_phys` | 已实现 |
| I-006 | VM_UNMAP_PHYS | mmap.c:310 (反向) | `munmap::unmap_phys` | 已实现 |
| I-007 | VM_EXIT | exit.c:60 | `vm_server::handle_exit` | 已实现 |
| I-008 | VM_WILLEXIT | exit.c:100 | `vm_server::handle_willexit` | 已实现 |
| I-009 | VM_PAGEFAULT | pagefaults.c:240 | `vm_server::dispatch_pagefault` | 已实现 |
| I-010 | VM_REMAP | mmap.c:366 | `dispatch_remap` | ✅ **已完整实现 (2026-06-16)**: 完整 do_remap 语义 — endpoint 验证、源 region 查找/匹配、目标地址槽查找、VR_SHARED region 创建 + VrParam::Shared 设置 + remaps 递增。新增 `find_region_snapshot` + `increment_region_remaps` 跨进程安全访问。4 个测试覆盖 |
| I-011 | VM_REMAP_RO | mmap.c:366 | `dispatch_remap_ro` | ✅ **已完整实现 (2026-06-16)**: 复用 `dispatch_remap_impl(readonly=true)`，强制 VR_SHARED 不带 VR_WRITABLE。与 VM_REMAP 共享实现 |
| I-012 | VM_SHM_UNMAP | (mmap.c) | `dispatch_shm_unmap` | ✅ **已 wired (2026-06-14)**: 函数早已存在 (dispatcher.rs:132) 但此前被 catch-all 截走，本次接入 `dispatch_by_number` 分支 (`forwhom=m1i1, addr=m1p1`) |
| I-013 | VM_GETPHYS | mmap.c:438 | `vm_server::handle_get_phys` | 已实现 |
| I-014 | VM_GETREF | mmap.c:463 | `vm_server::handle_get_refcount` | 已实现 |
| I-015 | VM_SETCACHE | mem_cache.c:196 | `dispatch_setcache` | **NotImplemented** (依赖 slab) |
| I-016 | VM_MAPCACHEPAGE | mem_cache.c:95 | `dispatch_mapcache` | **NotImplemented** (依赖 slab) |
| I-017 | VM_FORGETCACHEPAGE | mem_cache.c:283 | `dispatch_forgetcache` | 已实现 (dispatcher.rs:231) |
| I-018 | VM_CLEARCACHE | mem_cache.c:315 | `dispatch_clearcache` | 已实现 (dispatcher.rs:241) |
| I-019 | VM_PROCCTL | exit.c:117 | `dispatch_procctl` | ✅ **已完整实现 (2026-06-16)**: VMPPARAM_CLEAR (free_proc+pt_new+pt_bind) + VMPPARAM_HANDLEMEM (handle_memory_once 同步路径); RS/VFS 权限检查; 未知 param→EINVAL; caller endpoint 传入; 5 个测试覆盖 |
| I-020 | VM_VFS_MMAP | mmap.c:135 | `dispatch_vfs_mmap` | 已实现 (mmap.rs:273) |
| I-021 | VM_VFS_REPLY | vfs.c:109 | `dispatch_vfs_reply` | ✅ Done (2026-06-16) — 完整实现: reqid>0 校验 + VfsReply 构造(req_id/result/fd/dev/size_pages) + VfsRequestQueue::handle_reply + 延迟回调(DispatchResult) + VmReply::Suspend; 3 个测试覆盖 |
| I-022 | VM_RS_SET_PRIV | rs.c:34 | `rs::handle_rs_set_priv` | 已实现 |
| I-023 | VM_RS_PREPARE | rs.c:71 | `rs::handle_rs_prepare` | **NotImplemented** |
| I-024 | VM_RS_UPDATE | rs.c:150 | `rs::handle_rs_update` | **NotImplemented** |
| I-025 | VM_RS_MEMCTL | rs.c:349 | `rs::handle_rs_memctl` | 已实现 |
| I-026 | VM_ADDDMA | (新增) | `dispatch_adddma` | **NotImplemented** |
| I-027 | VM_DELDMA | (新增) | `dispatch_deldma` | **NotImplemented** |
| I-028 | VM_GETDMA | (新增) | `dispatch_getdma` | **NotImplemented** |
| I-029 | VM_INFO | utility.c:100 | `query::handle_info` | 已实现 |
| I-030 | VM_GETRUSAGE | utility.c:426 | `query::handle_getrusage` | 已实现 (C 的 getrusage 只设 maxrss/minflt/majflt, 不设 text/data/stack) |

**IPC 覆盖小结**: 25/30 已实现, 5/30 NotImplemented (17% 缺口率)。未实现项: I-023/I-024 (RS live update), I-026/I-027/I-028 (DMA)。VFS transid 路由已接入 dispatch_procctl (S-13 已修复)。

---

## 7. 设计差异 (有意省略)

| 类别 | Minix3 C | Rust 实现 | 理由 |
|------|----------|-----------|------|
| **AVL 树** | Walt Karas 公共域 AVL + 宏泛型 | `BTreeMap<VirBytes, VirRegion>` | 标准库质量保证, 避免自实现 AVL 调试困难 |
| **slab 分配** | `slaballoc.c` (528 行) | 暂未实现 | `HeapArena` (heap_arena.rs) + 全局 `VmAllocator` (global.rs) 覆盖同等用例 |
| **fancy sanity macros** | `MYASSERT/USE/SLABSANE/...` | 改 `#[cfg(feature = "...")]` + 单元测试 | 配置式切换更符合 Rust 习惯 |
| **debug MARK/JUNKFREE** | 调试标记/释放填充 | 不需要 | Rust `dbg!` / `Drop` 自动 |
| **PRIMITIVE types** | `phys_bytes/vir_bytes` typedefs | `PhysBytes/VirBytes` newtype | 强类型 + 防止单位混淆 |
| **CALLMAP macro** | X-Macro 表注册 | `MessageDispatcher::dispatch_by_number` | 显式 match 表达力更强 |
| **ELEMENTS() macro** | `sizeof(x)/sizeof(x[0])` | `x.len()` | 内建方法 |
| **NO_MEM/CLICKSPERPAGE** | sentinel 值表示错误 | `Result<_, Error>` | 强制错误处理 |

---

## 8. 完整 TODO 清单 (按优先级)

> **注**: 本节及 §11 中的 `C-xx`/`S-xx`/`P0-xx`/`P1-xx`/`P2-xx`/`D-xx` 编号源自历史深度 review 报告 (cc-scan.md, 已删除)。编号本身已无独立定义文件，但每项均附有描述性文字说明问题内容，可作为历史跟踪标签保留。

### 8.1 P0 — 必须修复 (6 项, 原 19 项已修复 13 项)

1. ~~**IpcTransport trait 实现** — `vm_server::ipc_send/ipc_receive` 当前是 stub (VFS 不是阻塞项，IPC 是功能待实现)~~ ✅ **已修复 (2026-06-13)**: `os/servers/vm/src/ipc/transport.rs` 新建, 定义 `IpcTransport` trait + `KernelIpcTransport` (生产) + `TestIpcTransport` (测试) 双 impl. 6 个测试覆盖. 遗留: `KernelIpcTransport` 体内 `unimplemented!()` 等 P0-02 kernel IPC 落地. **DEFERRED 依赖链（todo P1-3，2026-08-16 明确）**: kernel IPC core → `minix_arch` `sys_ipc_*` 接缝 → `KernelIpcTransport::receive/send` 实现（transport.rs:151/:165）→ 主循环端到端验证（vm_server.rs 收发失败分支 panic 解除）；落地时 `Message` 编解码收敛到 minix-types codec trait（`EncodeToM1`/`DecodeFromM1`），消除 vm_server.rs 手工布局。
2. ~~**slab 模块决策** — `os/servers/vm/src/slab/` 空目录, 移植或删除~~ → **已修复**: 目录已不存在
3. ~~**dispatch_vfs_mmap** — VFS 文件 mmap 路径未实现~~ → **已修复**: handle_vfs_mmap 已实现 (mmap.rs:273)
4. ~~**dispatch_procctl** — VFS transid 流程未实现~~ → **已修复 (2026-06-16)**: dispatch_procctl 完整实现 (VMPPARAM_CLEAR + VMPPARAM_HANDLEMEM); handle_vfs_transid 接入 dispatch_procctl; transid 辅助函数; 7 个测试
5. ~~**dispatch_remap/remap_ro** — 共享内存重映射~~ → **已修复 (2026-06-16)**: dispatch_remap 完整实现 (endpoint 验证 + 源 region 查找/匹配 + 目标地址槽查找 + VR_SHARED region 创建); dispatch_remap_ro 复用 dispatch_remap_impl(readonly=true); 4 个测试覆盖
6. ~~**dispatch_shm_un_map** — 共享内存解除映射~~ → **已修复**: dispatch_shm_unmap 已实现 (dispatcher.rs:134), 调用 handle_munmap
7. ~~**dispatch_vfs_reply** — VFS 应答处理~~ → ✅ **已修复 (2026-06-16)**: 完整实现 dispatch_vfs_reply (dispatcher.rs:294) — reqid>0 校验 + VfsReply 构造 + VfsRequestQueue::handle_reply + 延迟回调(DispatchResult) + VmReply::Suspend; 3 个测试覆盖
8. **handle_rs_prepare/update** — Live Update 协议
9. ~~**add_region/remove_region** — `vmproc_handle.rs:431-440` 是 stub~~ → **已修复**: 实现为 VirRegion::insert/remove
10. ~~**brk/mmap overlap 检查** — brk 扩展 + mmap 无 MAP_FIXED 时不检查 overlap~~ → **已修复**: brk 用 find_overlap 检查堆栈冲突; mmap 插入前防御性检查; RegionMap::insert 返回 Result 含 overlap 检查
11. ~~**handle_memory_once** — 通知内核 fork 消息页面~~ → **已修复 (2026-06-14)**: 函数已实现 + 3 个测试; do_fork 调用待 VmProcTable split borrow
12. **do_memory** (handle_signal) — 不存在于当前代码 (Minix3 SIGKMEM 在 VM 侧无处理函数)
13. **exec_bootproc** — 启动进程 ELF 加载
14. ~~**SharedMemory::ev_pagefault** — 跨进程 PFN 共享~~ → **已修复 (2026-06-16)**: ev_pagefault 签名增加 &VmProcTable + &mut PfnAllocator; 完整实现 getsrc→vm_isokendpt→map_lookup→源页映射→PFN共享; MemTypeError 新增 InvalidProcess/InvalidAddress; 3 个测试
15. **CacheMemory::ev_pagefault** — 缓存索引查找 (当前 NeedNewPage)
16. ~~**ContiguousAnonymous::ev_new/pagefault** — 连续物理页分配~~ → **已修复 (2026-06-14)**: ev_new 实现连续 PFN 分配 + 连续性验证 + map_page; ev_pagefault 改为 panic (与 C 一致); ev_new 签名扩展 (region, frames, alloc)
17. ~~**cache_freepages** — LRU 淘汰 (当前返回 0)~~ → **已修复**: PageCache::free_pages 已实现 (page_cache.rs:140, LRU eviction)
18. ~~**pt_clearmapcache** — 页表映射缓存清理~~ → **设计差异 (2026-06-16)**: Direct Map 架构下此操作完全消除, 内核通过 kernel direct map 直接访问页目录, 无需缓存 PDE, 对应 C 函数 `sys_vmctl(VMCTL_CLEARMAPCACHE)` 无 Rust 等价物
19. ~~**map_pin_memory** — 钉住内存 (RS Live Update 依赖)~~ → ✅ **已修复 (2026-06-16)**: `region::map_pin_memory` (region/mod.rs:107) — 两阶段收集(vaddr,length)+handle_memory_once(wrflag=true); PinMemoryError::PageNotMapped; handle_rs_prepare 已接入; 2 个测试覆盖

### 8.2 P1 — 重要 (2 项, 原 11 项已修复 9 项)

1. ~~文档 emoji 净化 (~50 处, CLAUDE.md 违规)~~ → **已评估**: 标题 emoji 已清理; 表格中 ✅/❌/⚠️ 作为状态标记保留
2. ~~**SAFETY 注释补全** (~80 处 unsafe 块)~~ → **已修复**: global.rs, vm_self_map.rs, heap_arena.rs, vm_server.rs, fork.rs 等关键模块
3. ~~**`pub → pub(crate)`** (~15 处过度可见)~~ → **已修复**: 三轮降级, 覆盖所有物理内存/区域模块/VfsRequest/VfsReply 字段
4. ~~**RegionMap::insert overlap 检查**~~ → **已修复**: insert 从 Option→Result, 内含 find_overlap 检查, 重叠返回 Err(region)
5. ~~错误类型 `From` impls (替换 6 个 map_*_error)~~ → **已修复 (2026-06-14)**: 9 个 `map_*_error` 函数全部替换为 `From<XxxError> for VmError` impl, 调用点改用 `.into()`. 保留 `query_rusage_error_to_vm_error` (上下文相关映射)
6. ~~**reply_to_errno/encode_reply_data match-all**~~ → **已修复 (2026-06-14)**: 删除 `_ => {}` 通配, 穷举所有 VmReply variant; 新增 variant 编译报错 + ~~**VmReplyForIpc newtype wrapper**~~ → **已修复 (2026-06-16)**: `VmReplyForIpc` newtype 已在 `vm_server.rs` 实现, `new()` 构造器对 `VmReply::Suspend` 返回 `None`, 主循环 `DispatchAction::Reply` 分支使用 `VmReplyForIpc::new(reply).expect(...)`, `reply_to_errno` 保留 `unreachable!()` 作为 defense-in-depth 兜底, `encode_reply_data` 同步添加 `unreachable!()` (2026-06-16) 保持一致。Rust idiom: newtype pattern + 静态类型不变量 + `unreachable!()` 仅作 defense-in-depth。
   * **扩展修复 (2026-06-14) — VMI-1/2/3 + VMA-1**: 1) **VMI-1 InfoUsage** `let _ = (data, stack);` → m1i1/m1i2 存 data/stack 页数 + SAFETY 注释 (`as i32` 截断); 2) **VMI-2 InfoRegion** `let _ = regions;` → m1p1 高 32 位存源端 len 哨兵; 3) **VMI-3 Getrusage** 裸 `as i32` 截断 → `faults_to_i32` 闭包显式饱和 + SAFETY 注释; 4) **VMA-1** ACL 拒绝时静默 `let _` → `#[cfg(any(test, feature = "vm_acl_audit"))]` 包裹的 `eprintln!` 审计通道。引用 review-patterns-skill §模式19 (as 截断需 SAFETY) + §模式22 (pub/log 克制)。
7. ~~`QueryError::to_errno` 重命名~~ → **已修复 (2026-06-14)**: 删除 `QueryError::to_errno()`，保留 `to_errno_for_rusage()`（上下文相关映射）。7 个错误类型的 `to_errno()` 全部删除，统一到 `VmError::to_errno()`
8. ~~`fork_region` 返回 `Box<VirRegion>` 改为值~~ → **已修复 (2026-06-16)**: `fork_region` 返回 `VirRegion` 值类型, `fork_regions` 返回 `Vec<VirRegion>`, `free_forked_regions` 接受 `&mut [VirRegion]`, `do_fork` 中 `insert(*region)` 改为 `insert(region)`, 移除 `use alloc::boxed::Box` 导入。Rust 移动语义天然转移所有权, 无需额外堆分配
9. ~~**死代码删除** (`region/phys_region.rs` 570 行, 死 trait 等)~~ → **已修复**: phys_region.rs 已删除, handle_signal/page_frames_mut/SIGKMEM 等已移除
9. 行号引用校对 (D-05/D-06 等 12+ 处)
10. Ch1&2 vs Ch3&4 一一映射表 (CLAUDE.md 要求)
11. ~~**PageFlags 类型宽度统一** (u8 vs u16 矛盾)~~ → **误判**: 两者是不同类型 — `minix_arch::paging::PageFlags(u16)` 是硬件页表标志, `region::page_state::PageFlags(u8)` 是物理页状态标志, 不应统一

### 8.3 P2 — 优化 (8 项)

1. `MaybeUninit+bool → InPlaceOption<T>` 评估
2. VM 专用 `panic!` 函数 (栈展开)
3. `enum dispatch` 替代 `dyn MemType` (虚函数开销)
4. `// SAFETY:` 详细化 (展开硬件/并发前提)
5. `PhysAllocType` 加 `#[non_exhaustive]`
6. 集成测试 (VmServer 端到端 fork→brk→mmap→exit)
7. 文档 跨架构 (x86-32 vs x86-64) 差异补充
8. 章节风格统一 (§1.x vs 1.x, 表格对齐)

### 8.4 已修复 (本次迭代)

| # | 原优先级 | 描述 | 修复内容 |
|---|---------|------|---------|
| F-01 | P0 | ~~add_region/remove_region stub~~ | ✅ (2026-06-14) 1) `add_region(start: VirBytes, len: VirBytes) → Result<(), VmError>`; 2) `remove_region(start: VirBytes) → Result<(), VmError>` 先 unmap 页表再删 region; 3) `VmError` re-exported from `region/mod.rs` |
| F-02 | P0 | ~~write_page_table_mappings 忽略 pt.map 结果~~ | ✅ (2026-06-14) 传播 PageTableError → VmForkError::PageTableMapFailed（原 NoMemory 已拆分为精确变体） |
| F-03 | P0 | prepare_cow 空 stub | 实现 COW 页表设置 |
| F-04 | P1 | SAFETY 注释缺失 | 补全 global.rs, vm_self_map.rs, heap_arena.rs, vm_server.rs, fork.rs |
| F-05 | P1 | pub 过度可见 | handle_* 改 pub(crate), 删除 page_frames_mut 等 |
| F-06 | P1 | 死代码 | ~~删除 phys_region.rs~~, handle_signal, SIGKMEM, IpcStatus.flags | **已修复**: phys_region.rs 已删除, IpcSender trait 已删除 |
| F-07 | P1 | 测试竞态条件 | mock_vm_base save/restore 模式, brk 测试唯一 slot 分配 |
| F-08 | P1 | Rust 2024 unsafe_op_in_unsafe_fn | 嵌套 unsafe 块 + SAFETY 注释 |
| F-09 | P0 | ~~C-09 find_slot 对齐回退~~ | ✅ **已修复 (2026-06-14)**: `try_gap` 闭包做页对齐, 不足返回 None; 2 个回归测试 |
| F-10 | P0 | C-14 bind_page_table 错误处理 | 移到 sys_fork 前, 失败可回滚（后被 D8-④ 取代：bind 为已验证 no-op，整体删除） |
| F-11 | P0 | S-19 dispatcher pagefault 重复 | 删除 dispatcher stub, 保留 vm_server 实现 |
| F-12 | P1 | pub(crate) 可见性 (第二轮) | BitmapAllocator/BuddyAllocator/SegmentTreeAllocator/AlignedPhysBytes/PageAllocFlags/AllocError/PhysMemStats/PhysAllocator/PAGE_SIZE/PFN_NONE 降级 |
| F-13 | P1 | SAFETY 注释补全 (第二轮) | global.rs refill_arena 补全 |
| F-14 | P1 | S-15 cache_freepages stub | **已修复**: PageCache::free_pages 已实现 (page_cache.rs:140, LRU eviction) |
| F-15 | P1 | S-17 IpcSender 死代码 | **已修复**: IpcSender trait 及相关类型已删除 |
| F-16 | P1 | 测试 SIGSEGV 修复 | main.rs 测试模式用 System allocator; vm_server ensure_mock_phys_init 顺序修正 |
| F-17 | P1 | 未使用导入 | VirBytes/vm_phys_to_virt/init_vm_self_pt 条件导入 |
| F-18 | P0 | S-01 dispatch_by_number stub | 已连接 20+ 分支 (M1/M2 格式区分, RS/query/cache handler) |
| F-19 | P0 | alloc_page SIGABRT | ensure_mock_phys_init + 512 页分配避免竞态 |
| F-20 | P1 | D-01~D-24 文档修正 | 20 项文档错误修正 (行号/签名/emoji/链接/矛盾) |
| F-21 | P0 | ~~C-18 brk/mmap overlap 检查~~ | ✅ **已修复 (2026-06-14)**: brk.rs:95 + mmap.rs:260 添加 find_overlap 检查; 回归测试 test_grow_heap_rejects_overlap_c18 |
| F-22 | P1 | P1-08 RegionMap::insert overlap | insert 从 Option→Result, 内含 find_overlap 检查, 重叠返回 Err(region); 文档同步更新 13-region-avl.md, 11-region-mapping.md, 18-vm-mmap.md |
| F-23 | P1 | P1-03 VfsRequest/VfsReply 字段可见性 | 字段从 `pub` → `pub(crate)` |
| F-24 | P1 | P1-07 find_by_end 不变版本 | 添加 `RegionMap::find_by_end(&self)` (region_map.rs:103) |
| F-25 | P1 | P1-09 PhysAllocType non_exhaustive | 添加 `#[non_exhaustive]` (phys_mem/mod.rs:182) |
| F-26 | P1 | F-248/F-226 UsageInfo C语义对齐 | UsageInfo 与 C `vm_usage_info` 对齐 (total/common/shared/virtual_total/mvirtual), 移除不存在的 text/data/stack; shared 按 C 逻辑 (refcount>1 + VR_SHARED); handle_info 新增 frames 参数; 22-vm-queries.md 同步更新 |

---

## 9. 收敛评估

| 指标 | 当前 | 目标 | 状态 |
|------|------|------|------|
| 宏覆盖率 | 32% | 80% | ⚠️ 大量 sanity 宏可保留为 cfg 替代 |
| 全局变量覆盖率 | 72% | 95% | ⚠️ 健全检查类跳过可接受 |
| 结构体覆盖率 | 64% | 90% | ⚠️ 死代码/AVL 模板/slab 依赖可接受 |
| 函数覆盖率 | 68% | 90% | 🔴 13 IPC handler + 10 slab 函数需实现 |
| memtype 回调覆盖率 | 90% | 100% | ✅ 已接近完成, 9 项 stub 需明确决策 |
| IPC handler 覆盖率 | 60% | 90% | 18/30 已实现, 12 NotImplemented (VFS 不是阻塞项) |
| **总体** | **59%** | **90%+** | **🔴 中等差距** |

### 9.1 收敛路径

1. ~~**第一周**: 落实 IpcTransport trait, 解锁 18+ IPC handler~~ ✅ (2026-06-13: trait 落地, 6 个测试通过; 真实 IPC 调用待 P0-02 kernel IPC)
2. **第二周**: slab 模块决策 + delete/cargo fix + 错误类型统一
3. **第三周**: SAFETY 注释 + 文档 emoji 净化 + 行号校对
4. **第四周**: SharedMemory/CacheMemory/ContigAnon 跨进程/特殊路径
5. **第二月**: 集成测试 + 性能优化 + 跨文档一致性

### 9.2 关键里程碑

- **M1 (2 周)**: IPC 主路径全通, VM 可启动
- **M2 (4 周)**: Live Update 协议就绪 (RS_PREPARE/UPDATE + map_pin_memory)
- **M3 (6 周)**: slab 决策, 死代码清理, 90% 覆盖率
- **M4 (8 周)**: 文档 1:1 映射表, 集成测试, 性能基线
- **M5 (12 周)**: 100% 覆盖率 + 零 stub + Ch1&2/Ch3&4 一致

---

## 10. 关联产出

- `02-stage-vm/draft/TODO.md` — 既有 TODO 汇总（draft/，2026-08-15 随目录重组移入）
- `02-stage-vm/draft/21-review-ds.md, draft/23-review-ds.md, draft/26-review-dsf.md` — 21/23/26 历史 review（draft/，2026-08-15 移入；不进入新编号）
- (历史) 深度 review 报告已合并到本 checklist (含未修复项理由)
- `minix3/minix/servers/vm/` — C 源码 (ground truth)
- `os/servers/vm/src/` — Rust 实现

---

## 11. 迭代修复记录

### 2026-06-12 迭代 (第二轮修复)

本轮基于深度 review 报告，修复了以下代码和文档问题：

**代码修复**:
| # | 编号 | 修复内容 |
|---|------|---------|
| 1 | P1-05 | `VmFlags` 底层类型 `u32` → `u8` (仅 3 bit 使用) |
| 2 | P1-06 | `SearchType` 手卷 bitflags → `enum` (互斥搜索方向) |
| 3 | P1-19 | `get_slot_mut` 添加 SAFETY 注释 |
| 4 | P1-20 | ~~`transmute` 生命周期延长添加 SAFETY 注释~~ → **已修复 (2026-06-14)**: 提取 `extend_to_static_lifetime()` unsafe fn, 集中 SAFETY 文档 |
| 5 | P1-22 | `VM_SELF_PT: AtomicPtr` 添加设计说明 (为何不用 OnceCell) |
| 6 | P1-24 | ~~`register_page_alloc` 覆盖行为添加说明注释~~ → **已修复 (2026-06-14)**: `compare_exchange` 替代 `store`, 幂等重注册 no-op, 不同指针 panic |
| 7 | C-01~C-04 | IPC stubs 添加 ARCHITECTURE NOTE 说明依赖 |
| 8 | C-07 | ~~kernel layout 硬编码添加 ARCHITECTURE NOTE 说明依赖~~ → **已完整修复 (2026-06-17)**: 移除硬编码常量 + `compile_error!` 守卫 + `hardcoded_kernel_layout` feature gate; 新增 `KernelLayout` 结构体 + 全局存储 (`set_kernel_layout()`/`kernel_layout()`); `init_page_table()` 改用全局 layout; `VmServer::init_global_state()` 初始化 layout |
| 9 | 5.1 | `lib.rs` 添加单线程假设 crate root 文档 |

**文档同步**:
| # | 文档 | 更新内容 |
|---|------|---------|
| 1 | 01-vmproc-struct.md | VmFlags `u32`→`u8`, Drop `panic!`→`debug_assert!` |
| 2 | 13-region-avl.md | SearchType bitflags→enum, search 方法 match 实现 |
| 3 | 03-acl.md | AclState::Uninitialized SECURITY NOTE |
| 4 | 00-vm-overview.md | 单线程假设扩展 (UnsafeCell/typestate) |
| 5 | 24-vm-ipc-dispatch.md | IPC stubs 架构依赖说明 |

**未修复项**: 详见本 checklist §7 (完整 TODO 清单)
