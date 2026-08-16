# 12-memtype: 内存类型系统——多态内存语义分发

> **分类**: 阶段 5 — 地址空间数据结构（内存语义面）
> **源码**: `minix3/minix/servers/vm/memtype.h`（`mem_type_t` 15 字段）+ `minix3/minix/servers/vm/mem_anon.c`（151 行，`mem_type_anon` :34）+ `minix3/minix/servers/vm/mem_directphys.c`（79 行，`mem_type_directphys` :28 / `phys_setphys` :69）+ `minix3/minix/servers/vm/mem_shared.c`（211 行，`mem_type_shared` :28 / `shared_setsource` :167）+ `minix3/minix/servers/vm/mem_anon_contig.c`（132 行，`mem_type_anon_contig` :24）+ `minix3/minix/servers/vm/mem_cache.c`（324 行，`mem_type_cache` :39）+ `minix3/minix/servers/vm/mem_file.c`（287 行，`mem_type_mappedfile` :30）
> **Rust 模块**: `os/servers/vm/src/memtype.rs`（1366 行：`trait MemType` :10 / `AnonymousMemory` :165 / `DirectPhysical` :296 / `SharedMemory` :391 / `ContiguousAnonymous` :566 / `CacheMemory` :754 / `MappedFile` :871 / `MemTypeError` :125 / `PagefaultResult` :152）+ `os/servers/vm/src/region/page_state.rs`（`PageSlot.memtype` 挂载）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/05-physical-memory.md`（`alloc_mem` 供页分配）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/11-phys-pagestate.md`（phys_block/phys_region 生命周期 + refcount）
> **说明**: 内存类型系统语义模块：**Minix3 的 `mem_type_t` 函数指针表（name + 14 回调）× 6 类内存实例 → minix-rs 的 `trait MemType`（15 方法 + 默认实现）× 6 个 unit struct**。**不覆盖**：区域框架层调用点（13）、页错误状态机消费（16）、CoW 分裂细节（17）、文件/缓存语义详述（23/24）。

---

## 1. 概念：多态内存语义分发

### 1.0 章节引言

一个地址空间里的内存来源各异：堆/栈是按需分配的匿名页，mmap 可能是文件内容、设备寄存器或共享区域。它们的**行为**不同——何时分配物理页、写入时是否要 CoW、释放时要不要还物理页、要不要走缓存。VM 用**多态回调表**（策略模式）统一管理：区域层框架负责"何时调用"，内存类型负责"做什么"。本文档回答三个问题：

1. **为什么需要类型系统**——不同来源内存的行为差异（§1.1）。
2. **mem_type_t 是什么**——15 字段函数指针表，框架如何调用（§1.2-§1.3）。
3. **6 类内存各自的行为契约**——匿名/直接物理/共享/连续匿名/缓存/文件映射（§1.4-§1.9）。

它在地址空间数据结构阶段的位置：

```
11（物理页状态：refcount/标志）→ ★12（内存语义：类型回调）→ 13（区域映射：框架调用点）
→ 14（区域查找）→ 15（主循环）→ 16/17（页错误 + CoW 消费回调）
```

### 1.1 为什么需要内存类型系统

| 内存类型 | 来源 | 分配时机 | 释放时机 | 特殊行为 |
|---------|------|---------|---------|---------|
| 匿名内存 | 按需 | 页错误时 | refcount 归零 | CoW、按需清零 |
| 直接物理映射 | 设备寄存器 | 映射时（页恒存在） | 不解映射 | 不归 VM 管、uncached |
| 共享内存 | remap 创建 | 递归源区域缺页 | 所有映射解除 | 多进程可见、源跟随 |
| 连续匿名 | 按需 | ev_new 预分配 | refcount 归零 | 物理连续（DMA） |
| 缓存 | 磁盘缓存 | 缓存索引命中 | 缓存回收 | 与页缓存协同 |
| 文件映射 | mmap 文件 | VFS 异步 IO | munmap | 磁盘同步 |

统一接口的价值：**区域层代码只写一遍**（map_pf/复制/释放），行为差异全部封在回调里——新增内存类型不需要改框架。

### 1.2 mem_type_t：15 字段函数指针表

`mem_type_t`（memtype.h:12-30）是 **name + 14 回调 = 15 字段**：

| # | 字段 | 语义分类 | C 签名 |
|---|------|---------|--------|
| 1 | `name` | 元数据 | `const char *name` |
| 2 | `ev_new` | 生命周期 | `int (*)(vir_region*)` |
| 3 | `ev_delete` | 生命周期 | `void (*)(vir_region*)` |
| 4 | `ev_reference` | 生命周期 | `int (*)(phys_region*, phys_region*)` |
| 5 | `ev_unreference` | 生命周期 | `int (*)(phys_region*)` |
| 6 | `ev_pagefault` | 页错误 | `int (*)(vmp, region, ph, write, cb, state, len, io)` |
| 7 | `ev_resize` | 调整 | `int (*)(vmp, vr, len)` |
| 8 | `ev_split` | 调整 | `void (*)(vmp, vr, r1, r2)` |
| 9 | `ev_lowshrink` | 调整 | `int (*)(vr, len)` |
| 10 | `ev_sanitycheck` | 调试 | `int (*)(pr, file, line)` |
| 11 | `writable` | 查询 | `int (*)(pr)` |
| 12 | `ev_copy` | 生命周期 | `int (*)(vr, newvr)` |
| 13 | `regionid` | 查询 | `u32_t (*)(vr)` |
| 14 | `refcount` | 查询 | `int (*)(vr)` |
| 15 | `pt_flags` | 查询 | `int (*)(vr)` |

**NULL 回调的语义**：memtype 实例可以留 NULL 表示"框架默认行为"——框架层用 `if (mt->ev_xxx) mt->ev_xxx(...)` 判空调用（例如 region.c:1096 ev_split NULL → EINVAL、region.c:1164 ev_lowshrink NULL → EINVAL）。

### 1.3 框架与策略的分离

以页错误为例（11 的 map_pf 是框架，12 的 ev_pagefault 是策略）：

```
map_pf()                              ← 13 文档：框架层（何时调用）
  ├── pb_new(MAP_NONE)                ← 创建空 phys_block
  ├── pb_reference(pb, ...)           ← 创建 phys_region，refcount++
  └── ph->memtype->ev_pagefault(...)  ← ★本文档：策略层（做什么）
       ├── anon_pagefault()           ← 匿名：分配物理页 / CoW
       ├── dp_pagefault()             ← 直接物理：算 PA 映射
       ├── shared_pagefault()         ← 共享：递归源区域
       └── ...
```

同样：`map_copy_region`（fork）→ `ev_reference`/`ev_copy`；`pb_unreferenced` refcount 归零 → `ev_unreference`；`map_free` → `ev_delete`。**框架管流程，类型管行为**。

### 1.4 匿名内存（mem_type_anon，mem_anon.c:34）

行为契约：

- **按需分配**：`anon_pagefault`（:64）——`alloc_mem(1, allocflags)` 分配物理页；`ph->phys == MAP_NONE`（全新块）直接挂载；`refcount < 2 || !write`（无共享或只读）直接返回 OK（页已就绪）；否则（共享 + 写）→ `mem_cow`（CoW 分裂，17 详述）。
- **释放**：`anon_unreference`（:56）——`ph->phys != MAP_NONE` 时 `free_mem(ABS2CLICK(phys), 1)` 还物理页。
- **可写判定**：`anon_writable`（:105）——`phys == MAP_NONE` → 0（未映射不可写）；`parent->remaps > 0` → 1（共享源可写）；否则 `refcount == 1`（独占才可写，CoW 语义）。
- **调整**：`anon_resize`（:115）——只允许增长（`l > vr->length` 时更新 length），收缩静默忽略（brk 场景 OK）；`anon_split`（:147）空实现（区域可自由分割）；`anon_lowshrink`（:137）返回 OK。
- **查询**：`anon_regionid`（:132）返回 `region->id`；`anon_refcount`（:142）返回 `1 + vr->remaps`；`anon_pt_flags`（:48）arm 上返回 CACHED，其他 0。

### 1.5 直接物理映射（mem_type_directphys，mem_directphys.c:28）

设备内存语义：

- **页恒存在**：`dp_pagefault`（:50）——`phys = vr->param.phys + offset` 直接算物理地址，`pt_writemap` 映射（不需要分配、不需要页错误等待）。
- **不释放**：`phys_unreference`（:45）——no-op（物理页不归 VM 分配器管）。
- **uncached**：`phys_pt_flags`——返回 NO_CACHE（MMIO 必须直达硬件，绕过 CPU 缓存）。
- **不可分割**：`ev_split` NULL → 框架 EINVAL（设备映射区间是原子整体）。
- **参数设置**：`phys_setphys`（:69）——`vr->param.phys = phys` 设置区域基址（mmap MAP_PHYS 路径调用，mmap.c:356）。
- **复制**：`phys_copy`（:74）——复制 `param.phys`（fork 时子进程继承设备映射参数）。

### 1.6 共享内存（mem_type_shared，mem_shared.c:28）

remap 创建的多进程共享：

- **递归缺页**：`shared_pagefault`（:122）——从 `vr->param.shared`（ep/vaddr/id）定位源进程区域，递归调用源区域的 ev_pagefault（共享语义 = 源页的别名）；源不可达返回 EINVAL。
- **源设置**：`shared_setsource`（:167）——设置 `param.shared.{ep, vaddr, id}`；零 ep/vaddr/id 忽略（防御）。
- **引用管理**：`shared_unreference`（:49）——源区域的 remaps 递减；`shared_delete`（:110）——区域删除时对源 `shared_unreference`；`shared_refcount`（:207）——`1 + remaps`。
- **复制**：`shared_copy`（:194）——复制 param.shared + `shared_setsource(newvr, ...)` 重新登记。

### 1.7 连续匿名内存（mem_type_anon_contig，mem_anon_contig.c:24）

物理连续匿名页（DMA 等需要连续物理内存）：

- **预分配**：`anon_contig_new`（:52）——`ev_new` 时按区域长度预分配**连续**物理页（`alloc_mem` + PAF_ALIGN64K 等标志）。
- **拒绝 fork**：`anon_contig_reference`（:103）——返回错误（连续区域无法按页 CoW 共享，fork 直接失败）。
- **增长**：`anon_contig_resize`（:97）——仅增长，预分配新连续段。
- **缺页**：`anon_contig_pagefault`（:45）——从预分配区取页（不重新分配）。

### 1.8 缓存 / 文件映射（mem_type_cache / mem_type_mappedfile）

- **mem_type_cache**（mem_cache.c:39）：页缓存持有页——`cache_pagefault` 从缓存双哈希索引取 pfn（24-page-cache 详述）。
- **mem_type_mappedfile**（mem_file.c:30）：文件映射——`mappedfile_pagefault` 查缓存，未命中 → VFS 异步请求（`vfs_request`），`cow_block`（mem_file.c:59）处理文件页 CoW 分裂（23-vfs-interaction 详述）。

### 1.9 Rust：trait MemType 与 PagefaultResult

Rust 用 `trait MemType`（memtype.rs:10）替代函数指针表：

```rust
pub(crate) trait MemType: Send + Sync {
    fn name(&self) -> &'static str;
    fn ev_new(...) -> Result<(), MemTypeError> { Ok(()) }        /* C NULL → 跳过 */
    fn ev_delete(&self, _region: &mut VirRegion) {}              /* C NULL → 跳过 */
    fn ev_reference(...) -> Result<(), MemTypeError> { Ok(()) }
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}
    fn ev_pagefault(...) -> Result<PagefaultResult, MemTypeError> { Ok(PagefaultResult::Handled) }
    fn ev_resize(...) -> Result<(), MemTypeError> { Ok(()) }
    fn ev_split(...) -> Result<(), MemTypeError> { Err(MemTypeError::NotSupported) }  /* C NULL → EINVAL */
    fn ev_low_shrink(...) -> Result<(), MemTypeError> { Err(MemTypeError::NotSupported) }
    fn ev_sanitycheck(...) -> Result<(), MemTypeError> { Ok(()) }
    fn writable(...) -> bool { false }
    fn ev_copy(...) -> Result<(), MemTypeError> { Ok(()) }
    fn region_id(&self, _region: &VirRegion) -> u32 { 0 }
    fn ref_count(&self, _region: &VirRegion) -> i32 { 0 }
    fn pt_flags(&self, _region: &VirRegion) -> PageFlags { PageFlags::empty() }
}
```

**设计要点**：

1. **默认实现替代 NULL 判空**——C 的 `if (mt->ev_xxx)` 检查变成 trait 默认方法（框架调用 `region.memtype?.ev_xxx(...)`，不需要判空分支）。NULL 语义保留：`Option<&'static dyn MemType>`（PageSlot.memtype）表示"无类型"。
2. **`PagefaultResult` 枚举**（:152）——`Handled`/`NeedNewPage`/`NeedCow`/`NeedVfsIo`/`AccessViolation` 显式化页错误结果，16 页错误状态机消费。
3. **`MemTypeError` 映射 errno**（:125）——`NoMemory`→ENOMEM、`InvalidParam`/`InvalidProcess`/`InvalidAddress`/`NotSupported`→EINVAL、`IoError`→EIO。
4. **PFN 模型适配**——`ev_unreference(frames, pfn)` 收 PFN（无 phys_region 对象）；`ev_pagefault` 收 `&VmProcTable`（共享内存递归源查找）+ `&mut dyn PfnAllocator`（类型内分配）。
5. **6 个 unit struct** 实现 trait，通过 `&'static dyn MemType` 引用挂载（`PageSlot.memtype`）。

### 1.10 对照 Redox / Linux

**Linux**：`struct vm_operations_struct`（fault/open/close/mmap 回调）是同一策略模式——每个 VMA 挂一组操作回调，页错误经 `handle_mm_fault` 分发到 `vma->vm_ops->fault()`。minix-rs 的 MemType trait 与它结构同构（区域 = VMA、memtype = vm_ops）。Linux 的 `VM_IO`/`VM_PFNMAP` 标志对应 DirectPhysical 语义（设备映射不经 page cache）。

**Redox**：早期无 per-region 类型系统——页表直接管理映射，文件映射由内核 `physmap` 面处理；其 `mm` 的 region 抽象没有多态回调。minix-rs 保留 Minix3 的类型系统（因为 fork/CoW/共享语义需要），但在表达上向 Linux 的 vm_ops 靠拢。

### 1.11 本章小结

- `mem_type_t` = name + 14 回调；框架判空调用，类型实现行为。
- 6 类内存：anon（按需/CoW）、directphys（恒存在/uncached）、shared（递归源）、anon_contig（预分配连续）、cache（缓存索引）、mappedfile（VFS 异步）。
- Rust trait MemType：15 方法带默认实现；PagefaultResult/MemTypeError 显式化；PFN 模型适配。

---

## 2. C 源码分析

### 2.0 本章定位

本章验证 6 个 mem_type 实例的初始化表与代表回调。所有行号以 `sed -n` 实证为准（2026-08-16）。

### 2.1 mem_type_anon 初始化表（mem_anon.c:34-46）

```c
struct mem_type mem_type_anon = {
    .name = "anonymous memory",
    .ev_unreference = anon_unreference,      /* :56 */
    .ev_pagefault = anon_pagefault,          /* :64 */
    .ev_resize = anon_resize,                /* :115 */
    .ev_sanitycheck = anon_sanitycheck,      /* :99 */
    .ev_lowshrink = anon_lowshrink,          /* :137 */
    .ev_split = anon_split,                  /* :147 */
    .regionid = anon_regionid,               /* :132 */
    .writable = anon_writable,               /* :105 */
    .refcount = anon_refcount,               /* :142 */
    .pt_flags = anon_pt_flags,               /* :48 */
};
```

注意：**没有 `.ev_new`/`.ev_delete`/`.ev_reference`/`.ev_copy`**——匿名内存用框架默认（NULL 跳过）。

### 2.2 anon_pagefault（mem_anon.c:64-97）

```c
static int anon_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb, void *state,
    int len, int *io)
{
    phys_bytes new_page, new_page_cl;
    u32_t allocflags;
    allocflags = vrallocflags(region->flags);          /* :74 区域标志 → 分配标志（13） */
    assert(ph->ph->refcount > 0);
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM)   /* :79 分配一物理页 */
        return ENOMEM;
    new_page = CLICK2ABS(new_page_cl);
    if(ph->ph->phys == MAP_NONE) {                     /* :85 全新块：直接挂载 */
        ph->ph->phys = new_page;
        return OK;
    }
    if(ph->ph->refcount < 2 || !write) {               /* :91 无共享/只读：页已就绪 */
        return OK;
    }
    assert(region->flags & VR_WRITABLE);
    return mem_cow(region, ph, new_page_cl, new_page); /* :96 CoW 分裂（17） */
}
```

三态决策：**新块挂载 / 直用 / CoW**——这是匿名内存页错误的核心逻辑，Rust 的 `AnonymousMemory::ev_pagefault` 用 `NeedNewPage`/`NeedCow` 返回（§4.1）。

### 2.3 anon_unreference / anon_writable（mem_anon.c:56-62 / :105-113）

```c
static int anon_unreference(struct phys_region *pr)
{
    assert(pr->ph->refcount == 0);                     /* :57 归零才调用 */
    if(pr->ph->phys != MAP_NONE)
        free_mem(ABS2CLICK(pr->ph->phys), 1);          /* :60 还物理页 */
    return OK;
}
static int anon_writable(struct phys_region *pr)
{
    assert(pr->ph->refcount > 0);
    if(pr->ph->phys == MAP_NONE) return 0;             /* :108 未映射不可写 */
    if(pr->parent->remaps > 0) return 1;               /* :110 共享源可写 */
    return pr->ph->refcount == 1;                      /* :112 独占才可写 */
}
```

`anon_writable` 是 CoW 的**静态判定**：`refcount == 1` 时才可写——多引用共享页不可写，写触发页错误 → CoW 分裂。Rust 的 `AnonymousMemory::writable`（memtype.rs:192）等价。

### 2.4 mem_type_directphys（mem_directphys.c:28-79）

```c
struct mem_type mem_type_directphys = {
    .name = "physical memory mapping",
    .ev_pagefault = phys_pagefault,          /* :50 */
    .ev_unreference = phys_unreference,      /* :45 no-op：不归 VM 管 */
    .ev_copy = phys_copy,                    /* :74 */
    ...
};
void phys_setphys(struct vir_region *vr, phys_bytes phys)   /* :69 */
{
    vr->param.phys = phys;
}
```

`dp_pagefault`（:50-72）核心：`phys = vr->param.phys + ph->offset` 算 PA → `pt_writemap` 映射（`ARCH_VM_PTE_PRESENT|ARCH_VM_PTE_USER|ARCH_VM_PTE_RW`）。设备映射无缺页等待、无分配、无 CoW。

### 2.5 mem_type_shared（mem_shared.c:28-211）

```c
struct mem_type mem_type_shared = {
    .name = "shared memory",
    .ev_unreference = shared_unreference,    /* :49 */
    .ev_pagefault = shared_pagefault,        /* :122 */
    .ev_sanitycheck = shared_sanitycheck,    /* :156 */
    .ev_writable = shared_writable,          /* :161 */
    .ev_delete = shared_delete,              /* :110 */
    .regionid = shared_regionid,             /* :99 */
    .ev_copy = shared_copy,                  /* :194 */
    .refcount = shared_refcount,             /* :207 */
    .pt_flags = shared_pt_flags,             /* :41 */
};
```

`shared_pagefault`（:122-155）：`getsrc` 定位源进程/区域 → `map_pf(src_vmp, srcvr, ...)` 递归缺页 → `physblock_set` 共享新页。`shared_setsource`（:167-193）：设置 param.shared + 零值防御 + `vr->param.shared.id = ...`。

### 2.6 mem_type_anon_contig（mem_anon_contig.c:24-132）

```c
struct mem_type mem_type_anon_contig = {
    .name = "anonymous memory, contiguous",
    .ev_new = anon_contig_new,               /* :52 预分配连续页 */
    .ev_reference = anon_contig_reference,   /* :103 拒绝 fork */
    .ev_unreference = anon_contig_unreference, /* :112 */
    .ev_pagefault = anon_contig_pagefault,   /* :45 */
    .ev_resize = anon_contig_resize,         /* :97 增长预分配 */
    ...
};
```

`anon_contig_new`（:52-96）：按 `region->length` 一次 `alloc_mem(npages, PAF_ALIGN64K|PAF_CLEAR)` 分配连续物理页。`anon_contig_reference`（:103-111）返回错误——**连续区域不可 CoW 共享**。

### 2.7 本章小结

- 6 个 mem_type 实例 = 初始化表（字段 = 回调指针），NULL = 框架默认。
- anon 三态页错误（新块/直用/CoW）是核心语义；directphys/shared/anon_contig 各有独特契约。
- cache/mappedfile 的详述在 24/23，本文档只列类型表。

---

## 3. Rust 设计决策

### 3.1 D1: trait 替代函数指针表（ARCH 主决策）

`trait MemType: Send + Sync`（memtype.rs:10）15 方法**全带默认实现**。C 的 NULL 判空调用（`if (mt->ev_xxx)`)变成"框架直接调用，默认实现处理"——**减少一处判空分支，把"无特化"表达为默认行为**。

`Send + Sync` 约束：MemType 实例是 `&'static` 单例（unit struct），无内部可变状态——满足单线程 VM 模型且不阻碍未来并发演进。

### 3.2 D2: PagefaultResult 显式化页错误结果

```rust
pub(crate) enum PagefaultResult {
    Handled,        /* 页已就绪（directphys/已映射） */
    NeedNewPage,    /* 需分配新页（anon 新块） */
    NeedCow,        /* 需 CoW 分裂（anon 共享写） */
    NeedVfsIo,      /* 需 VFS 异步 IO（mappedfile） */
    AccessViolation,/* 访问违例（共享源不可达等） */
}
```

C 的 ev_pagefault 用返回值（OK/ENOMEM/...）+"已分配"副作用表达结果；Rust 显式化"后续动作"——16 页错误状态机按变体决定下一步（分配/分裂/挂起 VFS）。

### 3.3 D3: MemTypeError 映射 errno

| 变体 | errno | C 对应 |
|------|-------|--------|
| `NoMemory` | ENOMEM | alloc_mem 失败 |
| `InvalidParam` / `NotSupported` | EINVAL | 参数错误/ev_split NULL |
| `InvalidProcess` | EINVAL | getsrc 源进程无效 |
| `InvalidAddress` | EINVAL | map_lookup 源地址不存在 |
| `IoError` | EIO | VFS IO 失败 |
| `CopyFailed` | EFAULT | 复制失败 |

转换层 `From<MemTypeError> for VmError` 保证对外错误码是 Minix3 errno（无自造错误码）。

### 3.4 D4: PFN 模型适配

- `ev_unreference(&self, frames: &mut PageFrames, pfn: u32)`——C 的 `pr` 参数降为 pfn：PFN 模型下 phys_region 不存在，释放面由框架（PageFrames refcount 归零）触发，类型只拿到 pfn 做特化（如 anon 的"无操作"——实际还页由 PfnAllocator 统一做）。
- `ev_pagefault(proc_endpoint, region, frames, offset, write, table, alloc)`——`table: &VmProcTable` 供 SharedMemory 的 `getsrc` 等价；`alloc: &mut dyn PfnAllocator` 供类型内分配（anon 新页/contig 预分配）。

### 3.5 D5: 6 类型全实现（unit struct）

| Rust 类型 | 位置 | 关键覆盖 | C 对应 |
|-----------|------|---------|--------|
| `AnonymousMemory` | :165 | ev_pagefault（NeedNewPage/NeedCow）、writable（refcount==1）、region_id、ref_count（1+remaps） | mem_type_anon |
| `DirectPhysical` | :296 | ev_pagefault（base+offset→pfn）、pt_flags=NO_CACHE、ev_split NotSupported | mem_type_directphys |
| `SharedMemory` | :391 | ev_pagefault（递归源）、ev_copy（param 复制） | mem_type_shared |
| `ContiguousAnonymous` | :566 | ev_new（预分配，TODO :696）、ev_reference（拒绝 fork）、ev_resize | mem_type_anon_contig |
| `CacheMemory` | :754 | ev_pagefault（缓存索引取 pfn） | mem_type_cache |
| `MappedFile` | :871 | ev_pagefault（NeedVfsIo）、ev_copy | mem_type_mappedfile |

### 3.6 语义差异清单（C ↔ Rust 诚实标注）

| 维度 | Minix3 | minix-rs | 标注 |
|------|--------|----------|------|
| 多态载体 | 函数指针表（memtype.h:12-30） | trait + `&'static dyn MemType` | ARCH |
| NULL 回调 | 框架判空跳过 | 默认实现（框架无判空） | 结构差异 |
| 页错误结果 | 返回值 + 副作用 | PagefaultResult 显式枚举 | 显式化 |
| 错误 | 直接 errno | MemTypeError → errno 转换 | 同语义 |
| ev_unreference 参数 | phys_region* | pfn（PFN 模型） | ARCH |
| ContiguousAnonymous 连续分配 | alloc_mem 连续段 | TODO（memtype.rs:696，依赖 alloc_contiguous） | **未完成面** |
| writable 判定 | refcount==1 + remaps | 同语义 | 一致 |

---

## 4. 实现详解

### 4.1 `AnonymousMemory`（memtype.rs:165-294）

```rust
pub(crate) struct AnonymousMemory;                        /* :165 */
impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str { "anonymous memory" }
    fn writable(&self, frames, slot, region) -> bool {
        /* refcount == 1（remaps 由 region 侧维护） */   /* :192 */
    }
    fn ev_pagefault(&self, proc_endpoint, region, frames, offset, write, ...) {
        /* NeedNewPage（新块）/ NeedCow（共享写） */      /* :220 */
    }
    fn region_id(&self, region) -> u32 { region.id }      /* :262 */
    fn ref_count(&self, region) -> i32 { 1 + region.remaps }  /* :266 */
    ...
}
```

三态页错误与 C 的 `anon_pagefault`（§2.2）逐字对应：新块 → `NeedNewPage`（框架分配）；共享 + 写 → `NeedCow`（框架走 CoW）；否则 `Handled`。`writable` 的 `refcount == 1` 判定与 C `anon_writable`（§2.3）一致。

### 4.2 `DirectPhysical`（memtype.rs:296-389）

```rust
impl MemType for DirectPhysical {
    fn ev_pagefault(&self, ..., region, frames, offset, ...) {
        if let VrParam::Direct { phys: base_phys } = &region.param {
            if base_phys.0 == 0 { return Err(MemTypeError::InvalidParam); }
            let phys_addr = PhysBytes(base_phys.0 + offset.0);   /* base+offset 算 PA */
            let pfn = frames.phys_to_pfn(phys_addr);
            region.map_page(frames, offset, pfn, memtype);       /* 直接映射 */
            Ok(PagefaultResult::Handled)
        } else { Err(MemTypeError::InvalidParam) }
    }
    fn pt_flags(&self, _) -> PageFlags { PageFlags::NO_CACHE }   /* :375 */
    fn ev_split(&self, ...) -> Result<(), MemTypeError> { Err(MemTypeError::NotSupported) }
}
```

对应 C 的 `dp_pagefault` + `phys_pt_flags`（NO_CACHE）+ `ev_split` NULL → EINVAL。`base_phys.0 == 0` 防御：设备基址不能为 0（无效参数）。

### 4.3 `SharedMemory`（memtype.rs:391-564）

```rust
impl MemType for SharedMemory {
    fn ev_pagefault(&self, proc_endpoint, region, frames, offset, write, table, alloc) {
        /* getsrc 等价：table 查找源进程/区域 → 递归源缺页 */   /* :434 */
    }
    fn ev_copy(&self, src, dst) { dst.param = src.param.clone(); }  /* :536 */
}
```

递归源缺页（`table: &VmProcTable`）对应 C 的 `shared_pagefault`（§2.5）；`ev_copy` 复制 `param.shared` 对应 `shared_copy`。

### 4.4 `ContiguousAnonymous`（memtype.rs:566-752）

```rust
impl MemType for ContiguousAnonymous {
    fn ev_new(&self, region, frames, alloc) { ... }          /* :637 预分配连续页 */
    fn ev_reference(&self, ...) -> Result<(), MemTypeError> { Err(...) }  /* :608 拒绝 fork */
    fn ev_resize(&self, ...) { ... }                         /* :613 增长 */
    fn ev_pagefault(&self, ...) { /* 从预分配区取页 */ }      /* :720 */
}
```

**⚠️ 已知缺口**：`ev_new` 的连续页预分配依赖 `PfnAllocator::alloc_contiguous()`，当前 `PfnAllocator` trait 只有 `alloc_pfn`（单页）——memtype.rs:696 有 TODO 注释。连续分配路径未完成，**诚实标注**（§5.3），不伪装实现。

### 4.5 `CacheMemory` / `MappedFile`（memtype.rs:754-1018）

- `CacheMemory::ev_pagefault`（:768 起）：从缓存索引查 pfn 映射（24 详述缓存算法本身）。
- `MappedFile::ev_pagefault`（:885 起）：返回 `NeedVfsIo`（23 详述 VFS 异步对话）。

### 4.6 挂载点：PageSlot.memtype

```rust
pub(crate) struct PageSlot {
    pub(crate) pfn: u32,
    pub(crate) offset: VirBytes,
    pub(crate) memtype: Option<&'static dyn MemType>,   /* 11 定义，12 的类型对象挂载 */
}
```

`Option` 表达 C 的 phys_region.memtype 指针（可能 NULL——uninitialized 槽位）。框架调用模式：`slot.memtype.unwrap_or(ANON_DEFAULT).ev_pagefault(...)`。

### 4.7 消费链与边界

- **上游**：05（alloc_mem）、11（PageFrames/PageSlot）。
- **下游**：13（map_pf/复制/释放框架调用）；16（ev_pagefault 状态机）；17（ev_unreference + mem_cow）；18（ev_copy/ev_reference）；23/24（mappedfile/cache 详述）。
- **边界**：不覆盖 13 框架、16 状态机、17 CoW、23/24 文件/缓存语义。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

| 测试 | 位置 | 验证目标 |
|------|------|---------|
| `test_anonymous_memory_name` | memtype.rs:1024 | name() 正确 |
| `test_direct_physical_name` | :1030 | name() 正确 |
| `test_shared_memory_name` | :1036 | name() 正确 |
| `test_static_instances` | :1042 | 6 类型静态实例存在 |
| `test_anon_writable` | :1052 | refcount==1 可写判定 |
| `test_mapped_file_copy` | :1083 | MappedFile ev_copy |
| `test_shared_pagefault_already_mapped` | :1099 | 共享缺页已映射 → Handled |
| `test_shared_pagefault_invalid_param` | :1136 | 无效参数 → InvalidParam |
| `test_shared_pagefault_zero_ep` | :1170 | 零 endpoint → 防御 |
| `test_cache_pagefault_maps_cached_pfn` | :1203 | 缓存缺页映射 pfn |
| `test_cache_pagefault_already_mapped` | :1259 | 已映射 → Handled |
| `test_cache_pagefault_zero_pfn` | :1299 | 零 pfn → 错误 |
| `test_cache_pagefault_wrong_param_variant` | :1334 | 错误 param 变体 → InvalidParam |

### 5.2 覆盖维度

- **类型标识**：4 个 name 测试 + 静态实例存在性。
- **匿名语义**：writable 判定（refcount 边界）。
- **共享语义**：已映射/无效参数/零 ep 三类防御。
- **缓存语义**：映射/已映射/零 pfn/错误变体四类。

### 5.3 覆盖缺口与诚实标注

- **ContiguousAnonymous 连续分配**（memtype.rs:696 TODO）：`PfnAllocator::alloc_contiguous` 未实现——`ev_new` 预分配路径未完成，**记录不伪装**。
- **DirectPhysical ev_pagefault 端到端**：依赖真实页表映射（`region.map_page`），单测层仅 name/writable。
- **MappedFile VFS 异步**：`NeedVfsIo` 返回路径在 23 覆盖。

### 5.4 测试统计（截至 2026-08-16）

- `cargo test -p minix-vm --lib`：**360 passed / 1 failed**（1 failed 为 pre-existing `test_map_lazy`，13 范围）。
- 本文档直接相关：memtype.rs 13 个测试。
- 完整测试清单：`rg "^\s*fn test_" os/servers/vm/src/`。

---

## 6. 过渡

本文档完成**阶段 5 的内存语义面**。类型回调定义就绪后，13-region-mapping 的框架层（map_pf/复制/释放）可以调用它们完成完整缺页/生命周期流程；16-pagefault 消费 `PagefaultResult` 状态机；17-cow-mechanism 消费 `ev_unreference` + `mem_cow`（CoW 分裂）；23/24 详述 mappedfile/cache 的异步面。fork（18）经 `ev_copy`/`ev_reference` 复制区域语义；brk（19）经 `ev_resize` 扩展堆。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/plan.md`（§3.4 边界、§5.3 契约）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/11-phys-pagestate.md`（phys_block/phys_region + refcount）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md`（框架调用点）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/16-pagefault.md`（页错误状态机）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/17-cow-mechanism.md`（CoW 分裂）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/23-vfs-interaction.md`（mappedfile 详述）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/24-page-cache.md`（cache 详述）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/12-memtype.md`（素材）
- `os/servers/vm/src/memtype.rs`、`region/page_state.rs`
- `minix3/minix/servers/vm/memtype.h`、`mem_anon.c`、`mem_directphys.c`、`mem_shared.c`、`mem_anon_contig.c`、`mem_cache.c`、`mem_file.c`
