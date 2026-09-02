# 11-phys-pagestate: 物理页状态——从引用计数对象到 PFN 索引模型

> **分类**: 阶段 5 — 地址空间数据结构（物理页状态面）
> **源码**: `minix3/minix/servers/vm/pb.c`（168 行：`pb_new` :32 / `pb_free` :54 / `pb_link` :61 / `pb_reference` :73 / `pb_unreferenced` :96 / `mem_cow` :136）+ `minix3/minix/servers/vm/region.h`（`struct phys_block` :23 / `PBF_INCACHE` :35）+ `minix3/minix/servers/vm/phys_region.h`（`struct phys_region` :9-24）+ `minix3/minix/servers/vm/region.c`（`physblock_get` :60 / `physblock_set` :72 / `map_sanitycheck` :168-250）
> **Rust 模块**: `os/servers/vm/src/region/page_state.rs`（`PageFrames`/`PageState`/`PageSlot`/`PageFlags`/`PfnAllocator`）+ `os/servers/vm/src/region/mod.rs`（re-export）+ `os/servers/vm/src/sanity.rs`（`verify_refcounts` :54）+ `os/servers/vm/src/page_cache.rs`（`addcache`/`rmcache` 消费）+ `os/servers/vm/src/fork.rs`/`cow_exec_pf.rs`（CoW 消费）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/05-physical-memory.md`（裸物理页分配）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/10-vm-relocation.md`（自举终点，元数据稳态化）
> **说明**: 物理页状态语义模块：**Minix3 的 `phys_block`/`phys_region` 两层对象（引用计数 + 反向引用链表 + PBF_* 标志）→ minix-rs 的 PFN 索引全局数组（`PageFrames`）**。**不覆盖**：CoW 分裂机制细节（17）、页缓存持有面（24）、vir_region 槽位挂接（13）、memtype 回调族（12）。

---

## 1. 概念：物理页引用计数与生命周期

### 1.0 章节引言

地址空间数据结构阶段（11-14）回答一个问题：**进程的虚拟地址如何落到物理页上，物理页又由谁负责、被谁共享**。本文档聚焦物理页这一侧：VM 必须追踪每个物理页的状态——被引用几次、被哪些映射引用、是否在页缓存中。Minix3 用 `phys_block` + `phys_region` 两个堆对象表达，minix-rs 用全局 **PFN 索引数组**（`PageFrames`）表达。

### 1.1 两层物理页管理

VM 中存在**两层**物理页管理（draft 素材 §1.2 归纳，grep 实证）：

| 层次 | 数据结构 | 分配接口 | 场景 | 文档 |
|------|---------|---------|------|------|
| 第 1 层：裸物理页 | 无（仅 `phys_bytes`） | `alloc_mem()`/`free_mem()` | 页表页、spare pages、VM 自身内存 | 05/06 |
| 第 2 层：映射物理页 | `phys_block`/`phys_region` | `pb_new()`+`pb_reference()` | 进程内存（堆/栈/mmap/CoW 共享页） | **本文档** |

分界依据（grep 实证）：`alloc.c` 全文件零引用 `pb_new`/`phys_block`（`rg "pb_new" minix3/minix/servers/vm/alloc.c` → 0 hits）；`pb_new` 调用点全部在进程内存路径——region.c:691（缺页）、pb.c:136（mem_cow）、mem_anon_contig.c:65（连续匿名内存）。第 1 层页独占、无共享、生命周期简单，不需要引用计数；第 2 层页需要 refcount（fork 共享）、CoW（写时复制）、缓存（页缓存持有）三类语义。

### 1.2 三层结构系统：vir_region → phys_region → phys_block

C 侧用三个结构体协作管理"虚拟地址 → 物理页"映射：

```
进程 AVL 树（按 vaddr 组织）
  └── vir_region（虚拟区域，region.h:38）
        ├── vaddr/length/flags/parent/def_memtype
        └── physblocks[]（每页一个槽位，phys_region 指针）
              ├── [0] = phys_region_A { ph→phys_block, offset, memtype, next_ph_list→B }
              ├── [1] = NULL（未分配）
              └── [2] = phys_region_C
phys_block（region.h:23）{ phys, refcount, firstregion, flags }
  └── firstregion → phys_region_A → next_ph_list → phys_region_B（共享同一物理页）
```

| 结构体 | 回答的问题 | 关键字段 |
|--------|-----------|---------|
| `phys_block` | 物理页 X 是什么状态？谁引用它？ | `phys`、`refcount`、`firstregion`、`flags` |
| `phys_region` | 虚拟页 Y 映射到哪个物理页？什么类型？ | `ph`、`offset`、`memtype`、`parent`、`next_ph_list` |
| `vir_region` | 虚拟范围 [A,B) 有什么属性？ | `vaddr`、`length`、`physblocks[]`、`flags`、`param` |

**为什么需要 phys_region 中间层**：vir_region 直接指向 phys_block 会丢失四类信息——(1) **共享/CoW**：一个物理页被多个进程映射时无法区分每个映射；(2) **部分映射**：区域可能只有部分页映射；(3) **类型区分**：同一区域各页可属不同 memtype（12）；(4) **归属**：`parent` 指向所属 vir_region。

### 1.3 引用计数语义：reference / unreferenced / free

物理页生命周期三操作（pb.c）：

1. **`pb_reference(pb, offset, region, memtype)`**（:73）——创建新 phys_region 挂到物理页：`pb_link`（:61）设置 offset/ph/parent、把 phys_region 头插到 `pb->firstregion`、`refcount++`；`physblock_set(region, offset, newphysr)` 写回 vir_region 槽位。
2. **`pb_unreferenced(region, pr, rm)`**（:96）——解除一个映射：`refcount--` → 从 firstregion 链表摘除（头节点或遍历查找）→ `refcount == 0` 时调用 `pr->memtype->ev_unreference(pr)`（12 的回调面）释放页语义 + `SLABFREE(pb)` → `pr->ph = NULL` → `rm` 时 `physblock_set(region, offset, NULL)` 清槽。
3. **`pb_free(pb)`**（:54）——物理页回收：`phys != MAP_NONE` 时 `free_mem(ABS2CLICK(phys), 1)` 还裸页 + `SLABFREE(pb)`。

**引用计数不变量**：`refcount == 0` ⟺ `firstregion == NULL`（pb.c:103 `assert(!pb->firstregion)`）——共享链为空时物理页无人持有，可安全回收。

### 1.4 反向引用链表：谁在引用这个物理页

`firstregion`/`next_ph_list` 把引用同一物理页的所有 phys_region 串成链表。三个消费者：

- **CoW 判断**（17）：fork 后写共享页时，`refcount > 1` 决定是否需要分裂复制。
- **页缓存失效**（24）：缓存页被进程引用时 refcount 计数区分"进程引用"与"缓存持有"（PBF_INCACHE）。
- **sanity 检查**（region.c:168-250 `map_sanitycheck`）：遍历全进程区域核对 refcount 与实际引用一致。

### 1.5 PBF_INCACHE 标志

`PBF_INCACHE`（region.h:35）标记"该物理页被页缓存持有"——与进程映射的引用正交：页可以同时在缓存里（可复用）和映射中（进程在用）。`addcache`/`rmcache`（cache.c，24 覆盖）维护这个标志 + 关联 refcount。

### 1.6 PFN 索引模型（minix-rs 的 ARCH 演进）

minix-rs 用全局数组替代分散对象：

```
PageFrames { states: Vec<PageState>, total_pages: u32 }
  └── states[pfn] = PageState { refcount: u32, flags: PageFlags }
PageSlot { pfn: u32, offset: VirBytes, memtype: Option<&'static dyn MemType> }  ← 替代 phys_region
```

设计动机：

1. **O(1) 随机访问**：`PageFrames::get(pfn)`/`get_mut(pfn)` 直接索引——CoW 判断（fork.rs:74 `region.needs_cow(frames, offset)`）、页缓存命中、sanity 核对全部需要按物理页快速查状态。C 的 phys_block 是堆对象，查状态必须经 phys_region → ph 指针间接访问。
2. **无堆分配/释放**：PageFrames 在 `VmServer::init()` 一次性分配（vm_server.rs:276），运行期无 `SLABALLOC`/`SLABFREE` 每页元数据分配——消除 per-page 分配失败路径。
3. **类型安全**：`refcount: u32` + `saturating_add` 防溢出（C `u8_t` 上限 255，64 位系统长 fork 链可超）；标志位用 `bitflags` 类型化。
4. **显式 CoW**：`PageFlags::COW` 显式标记替代 C 的"PTE 只读推断"——页错误处理直接查标志（16/17），不依赖页表内容。

### 1.7 对照 Redox / Linux

**Linux**：`struct page`（`include/linux/mm_types.h`）是 PFN → page 全局数组（`mem_map`），每页有 `_refcount`/`_mapcount`/`flags`/LRU 链表——与 minix-rs `PageFrames` **同构**（全局数组 + 每页 refcount + 标志位）。Linux 的 `_mapcount`（映射计数）对应 C 的 refcount，`PageActive`/`PageLRU` 对应 flags 的缓存/活跃位。

**Redox**：早期内核无页级引用计数——进程地址空间独占物理帧，fork 由内核页表复制实现；Redox 的 `PageTable` 直接管理映射。其 `mm::paging` 的帧分配器不维护 per-page 共享计数。Minix3 的 phys_block 是"per-physical-page 元数据"的 32 位朴素版（无 per-CPU 分配、无 page 池）；minix-rs 的 PageFrames 在结构上更接近 Linux `mem_map` 而非 Minix3 原版——这是 ARCH 演进的方向性判断。

### 1.8 本章小结

- 两层物理页管理：裸页（05）vs 映射页（本文档）——按"是否需要共享/CoW/缓存语义"分界。
- C 三层结构：vir_region → phys_region（映射详情 + 链表节点）→ phys_block（页状态 + refcount）。
- 引用计数生命周期：reference（挂链 + 计数）→ unreferenced（摘链 + 归零释放）→ free（还裸页）。
- minix-rs：PFN 索引数组 + PageSlot，O(1) 状态访问，refcount u32，CoW 显式标志。

---

## 2. C 源码分析

### 2.0 本章定位

本章逐行分析 pb.c 全量 + 两个头文件的结构定义 + region.c 的槽位访问函数。所有行号以 `sed -n` 实证为准（2026-08-15）。

### 2.1 结构定义（region.h:23-35 / phys_region.h:9-24）

```c
struct phys_block {                          /* region.h:23 */
#if SANITYCHECKS
    u32_t seencount;                         /* :25 */
#endif
    phys_bytes phys;                         /* :27 physical memory */
    struct phys_region *firstregion;         /* :30 first in list of phys_regions */
    u8_t refcount;                           /* :31 Refcount of these pages */
    u8_t flags;                              /* :32 */
};
#define PBF_INCACHE  0x01                     /* :35 */
```

```c
typedef struct phys_region {                 /* phys_region.h:9 */
    struct phys_block *ph;                   /* :10 */
    struct vir_region *parent;               /* :11 vir_region or NULL if yielded */
    vir_bytes offset;                        /* :12 offset from start of vir region */
#if SANITYCHECKS
    int written;                             /* :15 written to pagetable */
#endif
    mem_type_t *memtype;                     /* :18 what kind of memory is it? */
    struct phys_region *next_ph_list;        /* :21 list of phys_regions referencing same block */
} phys_region_t;
```

要点：`refcount` 是 **u8**（32 位系统共享链很少超 255）；`firstregion`/`next_ph_list` 是**反向引用链表**；`parent` 在"yielded"（区域让渡）时为 NULL；`written`/`seencount` 是 SANITYCHECKS 调试字段。

### 2.2 pb_new（pb.c:32-51）

```c
struct phys_block *pb_new(phys_bytes phys)
{
    struct phys_block *newpb;
    if(!SLABALLOC(newpb)) {                    /* :37 从 slab 分配（09） */
        printf("vm: pb_new: couldn't allocate phys block\n");
        return NULL;
    }
    if(phys != MAP_NONE)
        assert(!(phys % VM_PAGE_SIZE));        /* :43 物理地址页对齐 */
    USE(newpb,
        newpb->phys = phys;                    /* :47 */
        newpb->refcount = 0;                   /* :48 */
        newpb->firstregion = NULL;             /* :49 */
        newpb->flags = 0;                      /* :50 */
    );
    return newpb;
}
```

- `SLABALLOC`（09 覆盖）从 slab 分配器取对象；失败返回 NULL（调用方处理 ENOMEM）。
- `phys == MAP_NONE` 表示"尚未分配裸页"（缺页时先建 phys_block，物理页按需分配）——`pb_free` 对 MAP_NONE 跳过 free_mem。
- `USE` 宏是 SANITYCHECKS 门控的写保护临界区（09 MEMPROTECT）。

### 2.3 pb_free（pb.c:54-59）

```c
void pb_free(struct phys_block *pb)
{
    if(pb->phys != MAP_NONE)
        free_mem(ABS2CLICK(pb->phys), 1);      /* :56 裸页还回分配器（05） */
    SLABFREE(pb);                              /* :57 slab 对象还回（09） */
}
```

**语义**：物理页对象的两段回收——裸页（第 1 层）+ 元数据对象（第 2 层）。Rust 对应：`PfnAllocator::free_pfn`（裸页）+ PageFrames 数组槽位状态归零（无对象释放）。

### 2.4 pb_link（pb.c:61-70）

```c
void pb_link(struct phys_region *newphysr, struct phys_block *newpb,
    vir_bytes offset, struct vir_region *parent)
{
    USE(newphysr,
        newphysr->offset = offset;             /* :65 */
        newphysr->ph = newpb;                  /* :66 */
        newphysr->parent = parent;             /* :67 */
        newphysr->next_ph_list = newpb->firstregion;   /* :68 头插 */
        newpb->firstregion = newphysr;         /* :69 */
    );
    newpb->refcount++;                         /* :70 */
}
```

**核心语义**：物理页获得一个新引用者——phys_region 头插到 firstregion 链表 + `refcount++`。O(1) 头插（无需遍历）。

### 2.5 pb_reference（pb.c:73-90）

```c
struct phys_region *pb_reference(struct phys_block *newpb,
    vir_bytes offset, struct vir_region *region, mem_type_t *memtype)
{
    struct phys_region *newphysr;
    if(!SLABALLOC(newphysr)) {                 /* :78 */
        printf("vm: pb_reference: couldn't allocate phys region\n");
        return NULL;
    }
    newphysr->memtype = memtype;               /* :82 */
    pb_link(newphysr, newpb, offset, region);  /* :84 */
    physblock_set(region, offset, newphysr);   /* :86 写回 vir_region 槽位 */
    return newphysr;
}
```

**组合语义**：`SLABALLOC`（phys_region 对象）+ `pb_link`（挂链 + 计数）+ `physblock_set`（槽位登记）——一次完成"虚拟页 → 物理页"映射建立。调用方（region.c map_pf :690、map_proc_copy_range 等）在缺页/复制时使用。

### 2.6 pb_unreferenced（pb.c:96-126）

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb;
    pb = pr->ph;
    assert(pb->refcount > 0);
    USE(pb, pb->refcount--;);                  /* :101 */

    if(pb->firstregion == pr) {                /* :103 头节点直接摘 */
        USE(pb, pb->firstregion = pr->next_ph_list;);
    } else {                                   /* :105 否则遍历查找 */
        struct phys_region *others;
        for(others = pb->firstregion; others; others = others->next_ph_list) {
            assert(others->ph == pb);
            if(others->next_ph_list == pr) {   /* :112 前驱的 next 指向 pr 的后继 */
                USE(others, others->next_ph_list = pr->next_ph_list;);
                break;
            }
        }
        assert(others);                        /* :116 不在链上 → 断言失败 */
    }

    if(pb->refcount == 0) {                    /* :119 */
        assert(!pb->firstregion);
        int r;
        if((r = pr->memtype->ev_unreference(pr)) != OK)   /* :122 类型回调（12） */
            panic("unref failed, %d", r);
        SLABFREE(pb);                          /* :124 */
    }
    pr->ph = NULL;                             /* :125 */
    if(rm) physblock_set(region, pr->offset, NULL);        /* :126 清槽 */
}
```

**语义分解**：引用计数递减 → 链表摘除（头 O(1)/非头 O(n)）→ 归零时**类型回调 `ev_unreference`**（12 定义：anon 页释放/直接物理页保留等）→ 元数据对象回收 → 可选清槽。这是物理页"最后一个引用者离开"的完整路径——CoW 分裂（17）、munmap（21）、进程退出（22）都汇聚到这里。

### 2.7 physblock_get / physblock_set（region.c:60 / :72）

```c
struct phys_region *physblock_get(struct vir_region *region, vir_bytes offset)  /* region.c:60 */
{
    struct phys_region **physblocks = region->physblocks;
    return physblocks[offset / VM_PAGE_SIZE];   /* 槽位 = offset/页大小 */
}
void physblock_set(struct vir_region *region, vir_bytes offset, struct phys_region *pr)  /* :72 */
{
    struct phys_region **physblocks = region->physblocks;
    physblocks[offset / VM_PAGE_SIZE] = pr;     /* 写回槽位 */
}
```

vir_region 的 `physblocks[]` 是每页一个槽位的指针数组——虚拟区域 ↔ 物理页的登记表。Rust 对应：vir_region 的 `PageSlot` 数组（13-region-mapping 覆盖）。

### 2.8 mem_cow（pb.c:136-168，17 详述）

签名引用（本文档不展开，17-cow-mechanism 覆盖）：

```c
int mem_cow(struct vir_region *region, struct phys_region *ph,
    phys_bytes new_page_cl, phys_bytes new_page)   /* pb.c:136 */
```

- 新页分配（`new_page == MAP_NONE` → `alloc_mem` + `vrallocflags` 换算分配标志）→ `sys_abscopy` 复制内容 → `pb_new(new_page)` → `pb_unreferenced(region, ph, 0)` 摘除旧引用 → `pb_link(ph, pb, ph->offset, region)` 挂新页 → `ph->memtype = &mem_type_anon`。
- 与 `pb_unreferenced` 的交互：`rm=0` 不清槽（ph 复用，只换物理页）。

### 2.9 本章小结

- pb.c 六函数构成物理页生命周期：创建（pb_new）、引用（pb_reference→pb_link）、解除（pb_unreferenced）、回收（pb_free）、CoW 分裂（mem_cow）。
- 反向链表 + u8 refcount 是 32 位系统的设计选择；sanity 字段（written/seencount）是调试面。

---

## 3. Rust 设计决策

### 3.1 D1: 集中数组替代分散对象（ARCH 主决策）

`PageFrames`（page_state.rs:225-258）是全局 PFN → PageState 数组：

```rust
pub(crate) struct PageFrames {
    states: Vec<PageState>,
    total_pages: u32,
}
pub(crate) struct PageState {
    pub(crate) refcount: u32,       /* u32 匹配/超过 C 的 int，64 位防溢出 */
    pub(crate) flags: PageFlags,
    _padding: u8,
}
```

**论证**：C 的 phys_block 是 slab 堆对象，访问必须"phys_region → ph 指针 → 字段"两步间接 + 可能空指针；PageFrames 是 O(1) 直接索引，且初始化一次性完成（vm_server.rs:276，`VmServer::init()` Phase 3），运行期零分配。反向链表（firstregion）的枚举需求由 sanity 审计（§4.4）替代——**运行时不需要"谁在引用"的实时链表**，只有调试审计需要，而审计可以全量扫描。

### 3.2 D2: PageSlot 替代 phys_region

```rust
pub(crate) enum PageSlot {
    Empty,                                      /* 未映射也未保留 */
    Reserved { offset: VirBytes, memtype: Option<&'static dyn MemType> },  /* lazy 占位（无后备帧） */
    Mapped   { pfn: u32, offset: VirBytes, memtype: Option<&'static dyn MemType> },  /* phys_region 挂载 */
}
```

- **三态枚举替代 PFN_NONE 哨兵**（2026-08-16，todo P0-1）：旧设计 `pfn=PFN_NONE` 同时编码"未映射"与"lazy 占位"，二者在类型层不可区分，导致 `get_slot` 过滤掉 lazy 槽。现在 `Empty`/`Reserved`/`Mapped` 显式分态：`pfn()` 仅 `Mapped` 返回 `Some`，`is_reserved()`/`is_empty()` 提供状态查询。枚举尺寸与旧结构相同（32B），仍省 `Option<PageSlot>` 判别开销。
- `mapped(pfn, offset, memtype)`/`reserved(offset, memtype)` 构造器替代 `PageSlot::new`；`set_memtype` 仅对 present 槽（Mapped/Reserved）生效。
- `parent` 字段消失：PageSlot 挂在 vir_region 的 slots 数组里，归属由容器表达（13）。
- `next_ph_list` 消失：PFN 模型下共享信息在 PageFrames 的 refcount 中，无需 per-mapping 链表。

### 3.3 D3: refcount u8→u32 + saturating_add

C `u8_t refcount`（region.h:31）上限 255——32 位 Minix3 上共享链（fork 链）很少接近，但 **64 位系统 + 深 fork 链可以超过**。Rust 用 `u32` + `saturating_add`（page_state.rs:263）防溢出。这同时消除了 C 的 `refcount == 0` 判断与 u8 回绕的隐患。

### 3.4 D4: 标志显式化（PageFlags）

```rust
pub(crate) struct PageFlags: u8 {
    const IN_CACHE   = 0x01;   /* C: PBF_INCACHE（region.h:35） */
    const PENDING_IO = 0x02;   /* 页缓存异步 IO 在途（24） */
    const COW        = 0x04;   /* CoW 页：只读映射，写触发页错误（17） */
}
```

- `addcache(pfn)`/`rmcache(pfn)`（page_state.rs:258-276）：IN_CACHE 置位/清除 + refcount 同步增减——页缓存持有引用的显式表达（24 消费）。
- COW 显式标记替代 C 的 PTE 只读推断：`prepare_cow()`（fork 路径）置位，页错误处理（16）直接查标志决定分裂。

### 3.5 D5: PfnAllocator trait

```rust
pub(crate) trait PfnAllocator {
    fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError>;
    fn free_pfn(&mut self, pfn: u32);
}
```

抽象"从物理分配器借页"——对应 C 的 `pb_new(MAP_NONE)`（拿未分配块）+ `pb_free`（还裸页）。消费方：page_cache（24）、fork（18）、memtype（12）。`PfnAllocError::OutOfMemory` 对应 ENOMEM。

### 3.6 语义差异清单（C ↔ Rust 诚实标注）

| 维度 | Minix3 | minix-rs | 标注 |
|------|--------|----------|------|
| 元数据组织 | phys_block/phys_region 堆对象 | PageFrames 全局数组（PFN 索引） | ARCH（§3.1） |
| 反向链表 | firstregion/next_ph_list | 无（verify_refcounts 审计替代） | ARCH |
| refcount 宽度 | u8 | u32 + saturating_add | 64 位适配 |
| CoW 判定 | PTE 只读推断 + refcount>1 | PageFlags::COW 显式 | 显式化 |
| ev_unreference 回调 | memtype 回调释放页语义 | PageSlot.memtype Option + memtype 模块（12） | 同语义 |
| SANITYCHECKS 字段 | written/seencount | 无 | 结构简化 |
| 槽位登记 | physblock_set（region.c:72） | vir_region slots 数组（13） | 同语义 |
| 每页对象分配 | SLABALLOC/SLABFREE | 无（数组预分配） | ARCH |

---

## 4. 实现详解

### 4.1 `PageFrames`（page_state.rs:225-258）

```rust
pub(crate) struct PageFrames {
    states: Vec<PageState>,
    total_pages: u32,
}
impl PageFrames {
    pub fn new(total_phys: PhysBytes) -> Self {
        let total_pages = (total_phys.0 / PAGE_SIZE) as u32;   /* u32 上限 4TB */
        let states = vec![PageState::new(); total_pages as usize];
        Self { states, total_pages }
    }
    pub fn get(&self, pfn: u32) -> Option<&PageState>          /* O(1) 索引 */
    pub fn get_mut(&mut self, pfn: u32) -> Option<&mut PageState>
    pub fn pfn_to_phys(&self, pfn: u32) -> PhysBytes           /* PFN → PA */
    pub fn phys_to_pfn(&self, phys: PhysBytes) -> u32          /* PA → PFN */
    pub fn addcache(&mut self, pfn: u32)                       /* IN_CACHE + refcount++ */
    pub fn rmcache(&mut self, pfn: u32)                        /* 清 IN_CACHE + refcount-- */
}
```

- 初始化于 `VmServer::init()` Phase 3（vm_server.rs:274-276）——total_pages 已知后一次性分配。
- `PAGE_SIZE = 4096`（page_state.rs:15），与 VM 页大小一致。
- `total_pages as u32` 的界：4KB 页 × 4TB = 2^32 页——u32 恰好覆盖。

### 4.2 `PageSlot`（page_state.rs:90-205）

Copy 语义（fork 复制区域时整个槽位可拷贝，fork.rs 依赖）：

```rust
#[derive(Clone, Copy)]
pub(crate) enum PageSlot {
    Empty,
    Reserved { offset: VirBytes, memtype: Option<&'static dyn MemType> },
    Mapped   { pfn: u32, offset: VirBytes, memtype: Option<&'static dyn MemType> },
}
```

- **三态**：`Empty`（未映射）→ `Reserved`（lazy 占位，`map_lazy` 写入，携带 offset+memtype）→ `Mapped`（挂载帧）。`pfn()` 返回 `Option<u32>`——类型层保证只有 `Mapped` 有帧（todo P0-1 修复）。
- `PartialEq` 同态比较（memtype 指针比较不稳定，忽略）——测试/审计用。
- `Debug` 按变体打印，memtype 输出名称（`m.name()`）而非指针值——可读性。

### 4.3 `PageFlags` 与缓存消费（page_state.rs:27-40 / :258-276）

`IN_CACHE`/`PENDING_IO`/`COW` 三标志。`addcache`/`rmcache` 由 page_cache.rs 消费（24 覆盖完整缓存语义）：

```rust
pub fn addcache(&mut self, pfn: u32) {
    if let Some(state) = self.states.get_mut(pfn as usize) {
        if !state.flags.contains(PageFlags::IN_CACHE) {
            state.flags.insert(PageFlags::IN_CACHE);
            state.refcount = state.refcount.saturating_add(1);  /* 缓存持有引用 */
        }
    }
}
```

**幂等性**：已 IN_CACHE 不再重复计数（防重复 addcache 造成 refcount 虚高）。

### 4.4 `verify_refcounts`（sanity.rs:54-126）

C `map_sanitycheck`（region.c:168-250）的 Rust 对应：

```rust
pub fn verify_refcounts(
    frames: &PageFrames,
    table: &VmProcTable,
) -> Result<(), Vec<RefcountMismatch>>    /* sanity.rs:54 */
```

- 遍历全进程 `for_each_active_region()` 的所有 mapped PageSlot，按 pfn 统计实际引用数。
- 与 `PageFrames` 各 pfn 的 refcount 对比，不匹配返回 `Vec<RefcountMismatch>`（pfn + actual + expected）。
- 测试：`test_verify_refcounts_empty`（:128）/`test_verify_refcounts_mismatch_detected`（:136）/`test_verify_refcounts_cache_only_page`（:150）/`test_verify_refcounts_cache_mismatch`（:159）——覆盖空表/失配/缓存独占页/缓存失配。

### 4.5 消费链与边界

- **上游**：05（裸页分配）、10（自举终点）。
- **下游**：13（vir_region slots 数组 = PageSlot 数组，physblock_get/set 对应）；17（CoW 分裂 = refcount>1 + COW 标志）；18（fork = PageSlot Copy + refcount++）；24（addcache/rmcache = PBF_INCACHE）。
- **边界**：不覆盖 memtype 回调族（12）；不覆盖 CoW 分裂细节（17）；不覆盖页缓存算法（24）。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-15）

| 测试 | 位置 | 验证目标 |
|------|------|---------|
| `test_page_frames_init` | page_state.rs:289 | 初始化 refcount=0、flags 空 |
| `test_pfn_to_phys` | page_state.rs:300 | PFN → PA 换算 |
| `test_phys_to_pfn` | page_state.rs:308 | PA → PFN 换算 |
| `test_page_slot` | page_state.rs:316 | 三态：Mapped/Reserved/Empty |
| `test_incache` | page_state.rs:336 | addcache/rmcache refcount 维护 |
| `test_refcount_operations` | page_state.rs:348 | refcount 增减 |
| `test_verify_refcounts_empty/mismatch/cache_only/cache_mismatch` | sanity.rs:128/:136/:150/:159 | 审计一致性 |

### 5.2 覆盖维度

- **PFN 换算**：pfn_to_phys/phys_to_pfn 互逆（0/1/256 边界）。
- **缓存引用**：addcache → refcount=1 + IN_CACHE；rmcache → 归零。
- **审计**：sanity 全量核对（空表/失配/缓存独占三种场景）。

### 5.3 覆盖缺口与诚实标注

- **ev_unreference 等价**：C 的 `pb_unreferenced` 在 refcount==0 时调 memtype 回调；Rust 的 PageFrames 不持有释放逻辑——memtype 释放面归 12-memtype，fork/munmap 路径的 refcount 归零处理归 17/21/22。PageSlot 无对应测试，记录不阻塞。
- **COW 标志测试**：`prepare_cow`/`needs_cow` 路径在 17/18 覆盖。

### 5.4 测试统计（截至 2026-08-15）

- `cargo test -p minix-vm --lib`：**434 passed / 0 failed**（2026-08-16 实测；`test_map_lazy` 已由 13 范围修复，todo P0-1）。
- 本文档直接相关：page_state.rs 6 个 + sanity.rs 4 个 = **10 个**。
- 完整测试清单：`rg "^\s*fn test_" os/servers/vm/src/`。

---

## 6. 过渡

本文档是**阶段 5（地址空间数据结构）的第一篇**。物理页状态面就绪后：

```
11（物理页状态）→ 12（memtype：页的语义类型与回调）→ 13（区域映射：vir_region 槽位挂接）
→ 14（区域查找：AVL→BTreeMap）→ 15（主循环与分发）→ 16/17（页错误 + CoW）
```

12-memtype 为 `pb_unreferenced` 的 `ev_unreference` 回调提供类型定义；13-region-mapping 消费 `PageSlot` 数组（对应 physblock_get/set）；17-cow-mechanism 消费 `refcount` + `PageFlags::COW` 判断分裂；24-page-cache 消费 `IN_CACHE`。物理页状态的引用计数语义是 CoW、fork、页缓存共同的地基。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/plan.md`（§3.4 边界、§5.3 契约）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/05-physical-memory.md`（裸物理页分配）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/10-vm-relocation.md`（自举终点）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/12-memtype.md`（memtype 回调族，下一阶段）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md`（vir_region 槽位挂接）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/17-cow-mechanism.md`（CoW 分裂）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/10-phys-pagestate.md`（素材）
- `os/servers/vm/src/region/page_state.rs`、`sanity.rs`、`page_cache.rs`、`fork.rs`
- `minix3/minix/servers/vm/pb.c`、`region.h`、`phys_region.h`、`region.c`
