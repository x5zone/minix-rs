# 10-phys-pagestate: 物理页状态 (PageState)

> **分类**: VM私有
> **源码**: `minix3/minix/servers/vm/pb.c`, `region.h`
> **说明**: 全局物理页状态表，管理物理页的引用计数、标志位和生命周期

---

## 1. 概述

### 1.1 全局物理页状态表的作用

物理页状态表是 VM 中追踪物理内存页语义的核心数据结构。它回答的问题是：**物理页 X 现在是什么状态？被引用几次？被谁引用？是否在缓存中？**

在 Minix3 中，这一职责由 `phys_block` 结构体承担——每个 `phys_block` 代表一个物理内存页（4KB），通过引用计数支持多个虚拟区域共享同一物理页。本文档分析 `phys_block` 的 C 源码实现（第2章），然后讨论 Rust 重写的设计方案（第3-4章）。

### 1.2 两层物理页管理：裸物理页 vs 映射物理页

> ⚠️ 阅读前文（04-physical-memory、05-vm-allocpage）的读者可能会困惑：前面那些物理页分配（`alloc_mem`、`vm_allocpages`）怎么没提到 `phys_block`？这不是遗漏，而是 VM 确实存在**两层**物理页管理，它们服务于不同场景。

VM 中的物理页管理分为两层：

| 层次 | 数据结构 | 分配接口 | 管理场景 | 文档 |
|------|---------|---------|---------|------|
| **第 1 层：裸物理页** | 无（仅 `phys_bytes` 地址） | `alloc_mem()` / `free_mem()` | VM 基础设施：页表页、spare pages、VM 自身内存 | [04-physical-memory.md](04-physical-memory.md)、[05-vm-allocpage.md](05-vm-allocpage.md) |
| **第 2 层：映射物理页** | `phys_block` | `pb_new()` + `pb_reference()` / `pb_unreferenced()` | 进程内存：堆、栈、mmap、CoW 共享页 | 本文档 |

**为什么第 1 层不需要 `phys_block`？**

第 1 层的物理页是**VM 基础设施**——页目录、页表页、spare pages、slab 分配器自身的内存。这些页有以下特点：
- **独占使用**：页表页只被 VM 自己使用，不存在共享，不需要引用计数
- **生命周期简单**：创建时分配，销毁时释放，没有 CoW、fork 等复杂场景
- **分配路径更早**：`vm_allocpages()` 在 VM 初始化阶段就被调用（`pt_init_done` 之前），此时 `phys_block` 基础设施尚未就绪

源码证据：
- [pagetable.c:375](minix3/minix/servers/vm/pagetable.c#L375)：`vm_allocpages()` 调用 `alloc_mem()` 分配页表页，**完全不经 `phys_block`**
- [alloc.c](minix3/minix/servers/vm/alloc.c)：整个文件零引用 `pb_new`、`pb_reference`、`phys_block`
- `pb_new()` 仅在 [region.c:691](minix3/minix/servers/vm/region.c#L691)（进程缺页时创建新物理块）、[pb.c:158](minix3/minix/servers/vm/pb.c#L158)（`mem_cow` CoW 复制）、[mem_anon_contig.c:65](minix3/minix/servers/vm/mem_anon_contig.c#L65)（连续匿名内存）中被调用——全部是**进程内存**场景

**为什么第 2 层需要 `phys_block`？**

第 2 层的物理页是**进程内存**——映射到进程地址空间的堆、栈、mmap 页。这些页需要引用计数的原因：
- **共享需求**：fork 后父子进程共享同一物理页，需要引用计数追踪
- **CoW 支持**：写入共享页时需要判断 `refcount > 1` 来触发写时复制
- **生命周期复杂**：一个物理页可能被多个映射引用，必须等所有引用都释放后才能回收

**两层的关系**：第 2 层在第 1 层之上。`phys_block.phys` 字段存储的物理地址，最终来源于 `alloc_mem()` 分配的裸物理页——但 `phys_block` 在裸物理页之上增加了引用计数和共享管理。第 1 层不知道第 2 层的存在，第 2 层依赖第 1 层提供物理页。

```
进程内存（第 2 层）:
  vir_region → phys_region → phys_block (refcount=2)
                                    │
                                    │ phys = 0x1234000
                                    │
                              ┌─────┘
                              ▼
裸物理页分配（第 1 层）:
  alloc_mem() → bitmap 分配 → 返回 phys_clicks
                              ↓
  free_mem()  ← bitmap 回收 ← 释放 phys_clicks

VM 基础设施（第 1 层，无 phys_block）:
  vm_allocpages() → alloc_mem() → 页表页、spare pages
                                   ↑
                                   直接使用 phys_bytes，
                                   不经过 phys_block
```

> **架构差异**：Minix3 运行在 x86-32，`phys_bytes` 为 4 字节，指针为 4 字节，`phys_block` 结构体大小约 12 字节（不含 seencount）。minix-rs 目标为 x86-64，相应类型为 8 字节，结构体大小增至 24 字节。此差异影响所有相关结构体的布局。

### 1.3 统一的三层系统：phys_block + phys_region + vir_region

`phys_block` 与 `phys_region` 和 `vir_region` 是紧密耦合的整体，理解其中任何一个都需要了解另外两个。

Minix3 使用三个独立结构体共同管理「虚拟地址→物理页」的映射：

```
进程的 AVL 树（按 vaddr 组织）
  │
  └── vir_region (虚拟区域, N 个)
        │ vaddr, length, flags, def_memtype, param
        │
        └── physblocks[0..N]  (phys_region 指针数组, 每页一个槽位)
              │
              ├── [0] = phys_region_A
              │          │ ph ──────────────┐
              │          │ parent → vr      │
              │          │ offset = 0       │
              │          │ memtype = anon   │
              │          │ next_ph_list ───┐│
              │          └────────────────┘││
              │                            ▼▼
              │              phys_block (物理页 0x1234000)
              │                phys = 0x1234000
              │                refcount = 2
              │                flags = 0
              │                firstregion ──→ phys_region_A
              │                                │
              │                                └─→ phys_region_B
              │                                     (另一个 vir_region
              │                                      或 fork 子进程)
              │
              ├── [1] = NULL   (未分配物理页的槽位)
              │
              └── [2] = phys_region_C (另一页)
```

| 结构体 | 回答的问题 | 关键字段 |
|--------|-----------|---------|
| `phys_block` | 物理页 X 现在是什么状态？谁在引用它？ | `phys`, `refcount`, `flags`, `firstregion` |
| `phys_region` | 虚拟页 Y 映射到哪个物理页？以什么类型？ | `ph`, `offset`, `memtype`, `next_ph_list` |
| `vir_region` | 虚拟地址范围 [A, B) 有什么属性？ | `vaddr`, `length`, `physblocks[]`, `flags`, `param` |

**为什么需要 `phys_region` 这个中间层？**

直觉上似乎可以让 `vir_region` 直接指向 `phys_block`。但中间层解决四个问题：

| 场景 | 没有 phys_region 的问题 | phys_region 的解决方式 |
|------|------------------------|----------------------|
| **共享/CoW** | 一个物理页被多个进程映射，无法区分 | 每个映射有独立 `phys_region`，共享同一 `phys_block` |
| **稀疏映射** | 大区域只有部分页面有物理页 | 每页一个槽位，未映射为 NULL |
| **内存类型差异** | 同一区域不同页面可能有不同 memtype | `phys_region.memtype` 可独立于区域的 `def_memtype` |
| **偏移记录** | 需要知道物理页在区域中的偏移 | `phys_region.offset` 记录偏移 |

**三层在关键场景中的协作**：

fork 是最能体现三层协作的场景。以 4KB 匿名内存区域的 fork 为例：

```
fork 前（父进程）：
  vir_region (vaddr=0x1000, length=0x1000)
      └── physblocks[0] = phys_region_A
              └── phys_block (phys=0x8000, refcount=1)
  页表: 0x1000 → 0x8000 (Writable)

fork 后（父+子各持一份）：
  父 vir_region                  子 vir_region (vaddr=0x1000, length=0x1000)
      └── physblocks[0] = A          └── physblocks[0] = phys_region_B
                                             (parent=子vr, memtype=anon)
                                    A ──→ phys_block (refcount=2) ←── B
  父页表: 0x1000 → 0x8000 (ReadOnly)    子页表: 0x1000 → 0x8000 (ReadOnly)

写入时（CoW 触发）：
  map_lookup()  → vir_region 确认地址在区域内
  physblock_get() → 找到 phys_region_B
  mem_cow() → refcount==2，触发复制
    → pb_unreferenced(B, rm=0): refcount-- → 1
    → alloc_mem() 分配新页 0xA000
    → pb_new(0xA000): refcount=0
    → pb_link(B, 新pb): refcount++ → 1, B→新pb
    → B.memtype = mem_type_anon
  子页表: 0x1000 → 0xA000 (Writable)
```

这个流程说明：**三层缺一不可**——vir_region 定位区域，phys_region 定位映射，phys_block 判断是否需要 CoW。

> 三层结构的操作细节和 Rust 重设计见第 3 章。`phys_region` 和 `vir_region` 的 C 源码分析见 [11-region-mapping.md](11-region-mapping.md) 第 2 章。

### 1.4 五个子问题

三层结构共同解决一个核心问题：**如何管理进程虚拟地址空间到物理内存页的映射关系，同时支持共享（CoW/fork/shm）和多种内存类型？**

分解为五个子问题：

| # | 子问题 | Minix3 解决方式 | 涉及结构 |
|---|--------|----------------|---------|
| P1 | 虚拟地址空间分段 | `vir_region` + AVL 树 | `vir_region` |
| P2 | 虚拟页→物理页映射 | `physblocks[]` + `phys_region` | `vir_region`, `phys_region` |
| P3 | 物理页共享与引用计数 | `phys_block.refcount` + 侵入式链表 | `phys_block`, `phys_region` |
| P4 | 多种内存类型的行为差异 | `mem_type_t` 函数指针表 | `phys_region.memtype` |
| P5 | 延迟分配与按需填充 | `MAP_NONE` + pagefault 回调 | `phys_block.phys`, `memtype.ev_pagefault` |

P1 相对独立（见 [11-region-mapping.md](11-region-mapping.md)）。P3 是本文档的核心——物理页共享与引用计数。P2、P4、P5 在 [11-region-mapping.md](11-region-mapping.md) 和 [12-memtype.md](12-memtype.md) 中详述。

---

## 2. C 源码分析

### 2.1 phys_block 结构体定义与字段语义

**结构体定义**

Minix3 中 `phys_block` 的定义位于 `region.h`：

```c
struct phys_block {
#if SANITYCHECKS
    u32_t       seencount;      /* 调试用：遍历检查 */
#endif
    phys_bytes  phys;           /* 物理内存地址 */
    
    /* 引用此块的 phys_region 链表头 */
    struct phys_region *firstregion;    
    u8_t        refcount;       /* 引用计数 */
    u8_t        flags;          /* 标志位 */
};

/* phys_block 标志位 */
#define PBF_INCACHE     0x01    /* 此块在页面缓存中 */
```

**字段详解**

| 字段 | 类型 | 说明 |
|------|------|------|
| `seencount` | `u32_t` | 调试用（仅 `SANITYCHECKS` 构建）。一致性检查流程见 §2.2 溢出风险处 |
| `phys` | `phys_bytes` | 物理内存地址。`MAP_NONE`（`0xFFFFFFFE`）表示未分配物理内存（延迟分配） |
| `firstregion` | `*phys_region` | 引用链表头，通过 `phys_region.next_ph_list` 形成链表 |
| `refcount` | `u8_t` | 引用计数 |
| `flags` | `u8_t` | 标志位，目前只有 `PBF_INCACHE` |

**phys 字段**

- 存储物理页的起始地址
- 必须是页对齐（4KB 对齐）
- 值为 `MAP_NONE`（`0xFFFFFFFE`）表示未分配物理内存（延迟分配）

> **注意**：`MAP_NONE` 在页表操作中另有"取消映射"语义（如 `pt_writemap()` 传入 `MAP_NONE` 表示清除页表项），参见 [07-pagetable-ops.md](07-pagetable-ops.md)。

**firstregion 字段**

- 指向第一个引用此块的 `phys_region`
- 通过 `phys_region.next_ph_list` 形成链表
- 用于遍历所有引用此块的虚拟区域
- 链表长度始终等于 `refcount`（非缓存页时）

### 2.2 refcount 语义

**基本不变式**：

```
非 INCACHE 页：  refcount == count(firstregion 链表中的 phys_region 数量)
INCACHE 页：     refcount == count(firstregion 链表中的 phys_region 数量) + 1
```

`+1` 的来源：页缓存中的 `cached_page` 结构体持有该物理页的引用，但这个引用**不在 `firstregion` 链表中**。证据：
- [cache.c:243](minix3/minix/servers/vm/cache.c)：`hb->page->refcount++` — `addcache()` 直接对 `phys_block.refcount` 加 1
- [cache.c:274-275](minix3/minix/servers/vm/cache.c)：`rmcache()` 中 `cp->page->refcount--`

**refcount 的生命周期**：

```
refcount = 0  ── pb_link() ──→ refcount = 1 ── pb_link() ──→ refcount = 2 ── ...
    │                              │                              │
    │ 初始状态                      │ 独占                        │ 共享
    │ (pb_new 刚创建)              │ (或 INCACHE 时              │ (fork 后)
    │                              │  cache 持有计数)            │
    │                              │                              │
    └── pb_unreferenced ───────────┴── pb_unreferenced ──────────┘
        → ev_unreference              → refcount > 0，继续存在
        → SLABFREE(pb)
```

**类型选择**：`u8_t`（0-255）。MINIX 3 是微内核教学系统，典型部署只有几个到几十个进程，即使所有进程映射同一页，refcount = 1（缓存）+ 几十（进程），远小于 255——`u8` 在目标场景下足够。

**典型 refcount 值**：

| 场景 | refcount | 构成 |
|------|----------|------|
| 私有匿名页 | 1 | 1 个 phys_region |
| fork 后共享页 | 2 | 父 + 子各 1 个 phys_region |
| 10 进程共享 shm | 10 | 10 个 phys_region |
| 10 进程映射同一缓存页 | 11 | 1（缓存）+ 10（进程）|

**溢出风险**：`u8_t refcount++` 溢出后会回绕到 0，导致物理页被错误释放 → **严重 bug**。生产构建无防御，仅在 `SANITYCHECKS` 构建中通过一致性检查检测。

**一致性检查**（`#if SANITYCHECKS`，[region.c:220-247](minix3/minix/servers/vm/region.c#L220-L247)）：

`map_sanitycheck` 使用 `seencount` 字段执行双层验证，确保 `refcount` 与链表中的实际 `phys_region` 数量和缓存引用数一致：

1. **seencount 快速计数**：`ALLREGIONS` 宏遍历所有进程的所有 `phys_region`，每次遇到某个 `phys_block` 就对 `seencount++`（INCACHE 块额外 +1）。如果 `refcount != seencount`，立即打印警告。

2. **链表遍历精确验证**：遍历 `firstregion` 链表计算 `n_others`（链表长度），对 INCACHE 块 `n_others++`，然后断言 `refcount == n_others`。

### 2.3 PBF_INCACHE 标志

```c
#define PBF_INCACHE  0x01  /* 此块在页面缓存中 */
```

- `PBF_INCACHE`: 表示此物理块在页面缓存中
  - 由文件映射或磁盘缓存使用
  - 缓存引用使 `refcount` 额外 +1（不在 `firstregion` 链表中）
  - `cache_freepages()` 在 `refcount==1`（仅缓存引用，无进程引用）时回收此页

```
flags 使用场景:

1. 匿名内存:
   flags = 0x00
   - 普通进程堆/栈内存
   - 释放时直接归还空闲列表

2. 文件映射:
   flags = PBF_INCACHE
   - mmap 文件内容
   - 释放时更新缓存状态

3. 磁盘缓存:
   flags = PBF_INCACHE
   - 页面缓存中的块
   - 可能被多个文件共享
```

### 2.4 侵入式链表：firstregion + next_ph_list

`phys_block.firstregion` 侵入式链表是当前设计的核心争议点。基于 C 源码逐一验证：

| 用途 | 源码位置 | 说明 |
|------|---------|------|
| `pb_link` 头插法插入 | [pb.c:63-66](minix3/minix/servers/vm/pb.c) | 链表维护 |
| `pb_unreferenced` 从链表移除节点 | [pb.c:100-115](minix3/minix/servers/vm/pb.c) | O(n) 查找前驱 |
| sanity check 验证 refcount 一致性 | [region.c:234-248](minix3/minix/servers/vm/region.c) | 仅调试构建 |
| CoW 时遍历所有引用者设为只读 | — | **不存在**。`map_ph_writept` 只操作单个 `phys_region` |
| fork 后遍历所有引用者 | — | **不存在**。`map_writept` 逐页独立操作 |

**结论**：在 Minix3 的**生产代码**中，`firstregion` 链表的遍历仅用于链表维护本身（`pb_unreferenced` 摘除节点）和调试检查。CoW 和 fork 都不遍历链表。换言之，**"谁在引用这个物理页"这个信息在生产代码中从未被使用**——链表只服务于自身的维护和调试诊断。侵入式链表的实际价值有限，保留它带来的复杂度远大于它提供的便利。

链表操作时间复杂度：

| 操作 | 时间复杂度 | 说明 |
|------|-----------|------|
| 头部插入 | O(1) | `pb_link()` |
| 头部删除 | O(1) | `pb_unreferenced()`（pr 是链表头时） |
| 中间删除 | O(n) | `pb_unreferenced()`（需遍历查找） |
| 遍历链表 | O(n) | 调试/诊断 |

n = refcount，通常很小（1-5），所以 O(n) 可接受。

### 2.5 pb_new / pb_free / pb_link / pb_unreferenced

#### pb_new - 创建物理块

```c
struct phys_block *pb_new(phys_bytes phys)
{
    struct phys_block *newpb;

    /* 从 slab 分配器分配 phys_block 结构体 */
    if(!SLABALLOC(newpb)) {
        printf("vm: pb_new: couldn't allocate phys block\n");
        return NULL;
    }

    /* 若已分配物理页，断言地址必须页对齐 */
    if(phys != MAP_NONE)
        assert(!(phys % VM_PAGE_SIZE));
    
    /* USE 宏：若启用 MEMPROTECT，先解锁 slab 页再写入，写完后重新上锁 */
    USE(newpb,
    newpb->phys = phys;         /* 物理页地址；MAP_NONE 表示延迟分配 */
    newpb->refcount = 0;        /* 初始引用计数为 0，由 pb_link() 递增 */
    newpb->firstregion = NULL;  /* 引用链表初始为空 */
    newpb->flags = 0;           /* 标志位初始清零 */
    );

    return newpb;
}
```

- `phys = MAP_NONE`：延迟分配，不立即分配物理页
- `phys = 有效地址`：已分配的物理页地址，必须页对齐
- `refcount` 初始化为 0（分离创建和引用，`pb_link()` 建立引用关系时 `refcount++`）

#### pb_free - 释放物理块

```c
void pb_free(struct phys_block *pb)
{
    /* 若已分配物理页，先归还物理内存（ABS2CLICK 将字节地址转为 click 单位） */
    if(pb->phys != MAP_NONE)
        free_mem(ABS2CLICK(pb->phys), 1);
    /* 将 phys_block 结构体归还 slab 分配器 */
    SLABFREE(pb);
}
```

⚠️ `pb_free` 只能在 `refcount == 0` 时调用。两条释放路径：

| 方面 | 直接调用 `pb_free()` | 通过 `pb_unreferenced()` |
|------|----------------------|--------------------------|
| 适用场景 | 创建后未使用的块、错误处理路径 | 正常的引用释放（munmap、exit、CoW） |
| 前提 | 确保无 `phys_region` 引用此块 | `pr` 必须已链接到 `pb` |
| 物理页释放 | `free_mem(ABS2CLICK(pb->phys), 1)` | 由 `memtype->ev_unreference(pr)` 回调负责 |
| 结构体释放 | `SLABFREE(pb)` | `SLABFREE(pb)`（在 `refcount==0` 时） |

#### pb_reference - 增加引用

```c
struct phys_region *pb_reference(
    struct phys_block *newpb,
    vir_bytes offset,
    struct vir_region *region,
    mem_type_t *memtype
)
{
    struct phys_region *newphysr;

    /* 从 slab 分配器分配 phys_region 结构体 */
    if(!SLABALLOC(newphysr)) {
        printf("vm: pb_reference: couldn't allocate phys region\n");
        return NULL;
    }

    /* 设置内存类型，然后建立与 phys_block 的链接（refcount++） */
    newphysr->memtype = memtype;
    pb_link(newphysr, newpb, offset, region);
    /* 将新 phys_region 挂入 vir_region->physblocks[] 数组对应槽位 */
    physblock_set(region, offset, newphysr);

    return newphysr;
}
```

`pb_reference` 是高层接口，组合了三步：分配 `phys_region` → `pb_link` 建立链接 → `physblock_set` 更新 `vir_region` 数组。

#### pb_link - 链表插入

```c
void pb_link(
    struct phys_region *newphysr,
    struct phys_block *newpb,
    vir_bytes offset,
    struct vir_region *parent
)
{
    USE(newphysr,
    newphysr->offset = offset;      /* 记录在 vir_region 中的字节偏移 */
    newphysr->ph = newpb;           /* 指向所属的 phys_block */
    newphysr->parent = parent;      /* 指向所属的 vir_region */

    /* 头插法：将 newphysr 插入 phys_block 的 firstregion 链表头部 */
    newphysr->next_ph_list = newpb->firstregion;
    newpb->firstregion = newphysr;);

    newpb->refcount++;              /* 引用计数递增 */
}
```

头插法插入链表 + `refcount++`。

#### pb_unreferenced - 减少引用

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb;

    pb = pr->ph;                        /* 获取 pr 所属的 phys_block */
    assert(pb->refcount > 0);           /* 断言：引用计数必须大于 0 */
    USE(pb, pb->refcount--;);           /* 引用计数递减 */

    /* 从 firstregion 链表中摘除 pr */
    if(pb->firstregion == pr) {
        /* pr 是链表头，直接摘除 */
        USE(pb, pb->firstregion = pr->next_ph_list;);
    } else {
        /* pr 不在链表头，遍历查找前驱节点 */
        struct phys_region *others;
        for(others = pb->firstregion; others;
            others = others->next_ph_list) {
            assert(others->ph == pb);   /* 断言：链表上所有节点都指向同一 phys_block */
            if(others->next_ph_list == pr) {
                USE(others, others->next_ph_list = pr->next_ph_list;);
                break;
            }
        }
        assert(others);                 /* 断言：pr 必须在链表上，否则是 bug */
    }

    /* 若引用计数归零，释放物理页和 phys_block */
    if(pb->refcount == 0) {
        assert(!pb->firstregion);       /* 断言：链表必须已空 */
        int r;
        /* 调用 memtype 的 ev_unreference 回调释放物理页（如 free_mem） */
        if((r = pr->memtype->ev_unreference(pr)) != OK)
            panic("unref failed, %d", r);
        SLABFREE(pb);                   /* 归还 phys_block 到 slab 分配器 */
    }

    pr->ph = NULL;                      /* 断开 pr 与 phys_block 的关联 */
    /* rm=1：从 vir_region->physblocks[] 中清除该槽位（munmap/exit 场景）
     * rm=0：保留槽位，供 CoW 重新链接新 phys_block 使用 */
    if(rm) physblock_set(region, pr->offset, NULL);
}
```

关键步骤：
1. `refcount--`
2. 从链表中移除 `phys_region`（O(n) 查找前驱）
3. `refcount == 0` 时调用 `memtype->ev_unreference(pr)` 释放物理页，然后 `SLABFREE(pb)`
4. `rm` 参数：`rm=1`（munmap/exit）从 `vir_region` 移除；`rm=0`（CoW 重新链接）不移除

**memtype->ev_unreference 回调**：

`ev_unreference` 在 `refcount` 降为 0 时被 `pb_unreferenced()` 调用，负责释放物理页。不同 memtype 的释放策略不同：

| memtype | 用途 | ev_unreference 行为 | 源码 |
|---------|------|---------------------|------|
| `mem_type_anon` | 匿名页（堆、栈、匿名 mmap） | 断言 refcount==0；若物理页已分配则 `free_mem` 归还裸物理页 | [mem_anon.c](minix3/minix/servers/vm/mem_anon.c) |
| `mem_type_mappedfile` | 文件映射页（mmap 文件） | 断言 refcount==0；若物理页已分配则 `free_mem` 归还裸物理页（不负责磁盘回写） | [mem_file.c](minix3/minix/servers/vm/mem_file.c) |
| `mem_type_shared` | 共享内存（POSIX shm） | 委托 anon——底层物理页由 `alloc_mem` 分配，释放逻辑与匿名页相同 | [mem_shared.c](minix3/minix/servers/vm/mem_shared.c) |
| `mem_type_cache` | 页缓存（文件系统缓存页） | 委托 anon——cache 持有独立 refcount 引用，`ev_unreference` 仅在缓存引用已释放后触发（见下方注释） | [mem_cache.c](minix3/minix/servers/vm/mem_cache.c) |
| `mem_type_directphys` | 设备物理内存映射（mmap 设备寄存器） | 直接返回 OK——物理页不属于 VM 管理，不释放 | [mem_directphys.c](minix3/minix/servers/vm/mem_directphys.c) |
| `mem_type_anon_contig` | 连续匿名页（DMA 缓冲区等需物理连续） | 委托 anon——逐页 `free_mem(1)` 归还，与匿名页相同 | [mem_anon_contig.c](minix3/minix/servers/vm/mem_anon_contig.c) |

> **cache 的 refcount 引用机制**：`addcache()` 对 `phys_block.refcount++`，使缓存持有一个独立引用。`rmcache()` 对 `refcount--`，若降为 0 则**直接** `free_mem` + `SLABFREE`（绕过 `ev_unreference`）。因此 `cache_unreference` 仅在以下场景触发：缓存已通过 `rmcache` 释放引用（refcount--），但进程仍持有映射引用，随后进程释放时 refcount 才降为 0——此时按匿名页释放。

> 注意：`mappedfile_unreference` 直接调用 `free_mem` 释放物理页，**不负责磁盘回写**。磁盘回写是 VFS/文件系统的职责，不是 VM `ev_unreference` 的事。

### 2.6 mem_cow 函数

`mem_cow()` 是 CoW 的核心实现函数：当共享页（refcount≥2）发生写缺页时，分配新物理页并复制旧页内容，然后将 phys_region 重新链接到新 phys_block 并将 memtype 改为 anon。位于 [pb.c:134-168](minix3/minix/servers/vm/pb.c)，被 `anon_pagefault()` 和 `file_pagefault()` 调用：

```c
int mem_cow(struct vir_region *region,
        struct phys_region *ph, phys_bytes new_page_cl, phys_bytes new_page)
{
        struct phys_block *pb;

        /* 若调用者未提供新页，则分配一个 */
        if(new_page == MAP_NONE) {
                u32_t allocflags;
                allocflags = vrallocflags(region->flags);

                if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM)
                        return ENOMEM;

                new_page = CLICK2ABS(new_page_cl);
        }

        assert(ph->ph->phys != MAP_NONE);

        /* 将旧页内容复制到新页 */
        if(sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE) != OK) {
                panic("VM: abscopy failed\n");
                return EFAULT;
        }

        /* 为新页创建 phys_block */
        if(!(pb = pb_new(new_page))) {
                free_mem(new_page_cl, 1);
                return ENOMEM;
        }

        /* 解除旧页引用（refcount--，rm=0 保留 phys_region 槽位） */
        pb_unreferenced(region, ph, 0);
        /* 将 phys_region 链接到新 phys_block */
        pb_link(ph, pb, ph->offset, region);
        /* CoW 后 memtype 变为 anon——不再是文件映射或缓存页 */
        ph->memtype = &mem_type_anon;

        return OK;
}
```

与 refcount 相关的关键操作：
- `pb_unreferenced(region, ph, 0)`：旧块 `refcount--`
- `pb_link(ph, pb, ...)`：新块 `refcount++`（从 0 到 1）
- CoW 后旧块和新块各自 `refcount=1`

> 完整 CoW 流程分析见 [14-cow-mechanism.md](14-cow-mechanism.md)。

### 2.6.1 pb_new 的调用者：缺页时的 phys_block 创建

`pb_new` 的主要调用路径在 `map_pf()` ([region.c:689-695](minix3/minix/servers/vm/region.c#L689-L695)) —— 缺页时，若 `physblock_get` 返回 NULL（该虚拟页尚未映射物理页），则：

1. `pb_new(MAP_NONE)` 创建延迟分配的 phys_block（refcount=0，phys=MAP_NONE）
2. `pb_reference` 分配 phys_region + 建立链入（refcount++）
3. `pb_reference` 失败时 `pb_free` 释放 phys_block 并返回 ENOMEM

源码：

```c
if(!(pb = pb_new(MAP_NONE))) {
    printf("map_pf: pb_new failed\n");
    return ENOMEM;
}
if(!(ph = pb_reference(pb, offset, region, region->def_memtype))) {
    printf("map_pf: pb_reference failed\n");
    pb_free(pb);
    return ENOMEM;
}
```

> **注意**：`map_pf` 创建的 phys_block.phys = MAP_NONE，实际的物理页分配由 memtype 的 `ev_pagefault` 回调负责（如 `anon_pagefault` 调用 `alloc_mem()`）。

### 2.7 错误路径

**pb_new 分配失败**：

```c
if(!SLABALLOC(newpb)) {
    printf("vm: pb_new: couldn't allocate phys block\n");
    return NULL;
}
```

`SLABALLOC` 失败返回 `NULL`，调用者必须检查返回值。在 Minix3 中，`pb_new` 的调用者（`region.c:691`）在失败时返回 `ENOMEM`。

**refcount 溢出**：

`u8_t refcount++` 溢出后回绕到 0，导致物理页被错误释放。Minix3 仅在 `SANITYCHECKS` 构建中通过遍历链表验证一致性，生产构建无保护。

**ev_unreference 失败**：

```c
if((r = pr->memtype->ev_unreference(pr)) != OK)
    panic("unref failed, %d", r);
```

`ev_unreference` 返回非 `OK` 时直接 `panic`。在 Minix3 的实现中，所有 `ev_unreference` 都只返回 `OK`，所以这条路径实际上不会触发。

**pb_reference 中 SLABALLOC 失败**：

```c
if(!SLABALLOC(newphysr)) {
    printf("vm: pb_reference: couldn't allocate phys region\n");
    return NULL;
}
```

分配 `phys_region` 失败返回 `NULL`，调用者必须处理。

**SANITYCHECKS 相关**：

`seencount` 字段（仅调试构建）在遍历检查中用于防止重复访问。`map_sanitycheck`（[region.c:220-247](minix3/minix/servers/vm/region.c)）遍历所有 `phys_region` 链表验证 `refcount` 一致性。

---

## 3. Rust 设计方案

三层系统需要解决的五个子问题已在 [§1.4](#14-五个子问题) 中叙述。本章聚焦 P3（物理页共享与引用计数）的 Rust 设计方案，并说明最终方案如何同时影响 P2（虚拟页→物理页映射）、P4（多种内存类型的行为差异）和 P5（延迟分配与按需填充）的解决方式。

以下按递进顺序列出三个方案：从最接近 Minix3 直译的方案开始，逐步演进到最终方案。

### 3.1 方案一：直译保留

保留 Minix3 的三结构体设计（`PhysBlock`、`PhysRegion`、`VirRegion`），1:1 直译 C 源码。

```rust
pub(crate) struct PhysBlock {
    phys: PhysBytes,
    refcount: u16,
    flags: PhysBlockFlags,
    firstregion: Option<NonNull<PhysRegion>>,
}

pub(crate) struct PhysRegion {
    ph: Option<NonNull<PhysBlock>>,
    parent: *mut VirRegion,
    next_ph_list: Option<NonNull<PhysRegion>>,
    memtype: Option<&'static dyn MemType>,
    offset: VirBytes,
}
```

**与 Minix3 的对应**：

| Minix3 | 方案一 | 说明 |
|--------|--------|------|
| `SLABALLOC(pb)` | `Box::<PhysBlock>::new_uninit().assume_init()` | Rust 无自研 slab，用 `Box` 堆分配替代 |
| `SLABFREE(pb)` | `Box::from_raw(pb)` + drop | 释放堆分配的 PhysBlock |
| `SLABALLOC(pr)` | `Box::<PhysRegion>::new(...)` | PhysRegion 同样 Box 堆分配 |
| `pb->firstregion` | `PhysBlock.firstregion: Option<NonNull<PhysRegion>>` | 侵入式链表头 |
| `pr->next_ph_list` | `PhysRegion.next_ph_list: Option<NonNull<PhysRegion>>` | 链表节点 |
| `pr->ph` | `PhysRegion.ph: Option<NonNull<PhysBlock>>` | 裸指针引用 |

**核心特征**：

- `PhysBlock`：`Box` 堆分配的独立结构体（Minix3 用 slab，Rust 用 `alloc` crate），含 `phys`、`refcount`、`flags`、`firstregion` 侵入式链表头
- `PhysRegion`：`Box` 堆分配的独立结构体（同上），含 `NonNull<PhysBlock>` 裸指针、`next_ph_list` 链表节点
- `pb_new()` / `pb_free()`：`Box` 堆分配/释放 `PhysBlock`（对应 Minix3 的 `SLABALLOC` / `SLABFREE`）
- `pb_reference()` / `pb_unreferenced()`：链表插入/摘除 + refcount 增减

**问题**：

| 问题 | 说明 |
|------|------|
| 大量 `unsafe` | `NonNull<PhysBlock>` 裸指针操作需要 `unsafe` 块，丧失安全保证 |
| 侵入式链表维护复杂 | `pb_unreferenced()` O(n) 摘除，容易出错且性能差 |
| `PhysBlock` 独立堆分配 | 每个物理页一个 `Box<PhysBlock>`，分配/释放开销大 |
| `PhysRegion` 独立堆分配 | 每页一次 `Box` 堆分配，开销大 |

**结论**：直译方案是起点，但不是终点。其核心问题（裸指针、侵入式链表、独立堆分配开销）在 Rust 中都有更好的解决方案。

### 3.2 方案二：最小改动

保留三结构体设计，但消除 `unsafe` 和侵入式链表，用 Arena 索引替代裸指针。

**改动清单**：

1. `PhysRegion` 退化为 `PageSlot`：消除独立堆分配，内嵌在 `VirRegion` 中；同时移除 `parent`（指向所属 VirRegion 的裸指针）和 `next_ph_list`（链表节点），只保留映射所需的核心字段
2. 用 Arena 索引替代 `NonNull<PhysBlock>`：`PhysRegion.ph` 从 `Option<NonNull<PhysBlock>>` 改为 `Option<u32>`（Arena 中的下标）
3. 消除侵入式链表：移除 `PhysBlock.first_region` 和 `PhysRegion.next_ph_list`
4. 保留 `PhysBlock` 结构体：但通过全局 Arena 管理

`PageSlot` 是 `PhysRegion` 的退化形态：Minix3 的 `PhysRegion` 是 slab 分配的独立结构体，含裸指针（`ph`、`parent`）和链表节点（`next_ph_list`）；`PageSlot` 消除了这些，只保留"一个虚拟页映射到哪个物理页、属于什么内存类型"这三个核心信息，内嵌在 `VirRegion` 中，无需独立分配。

```rust
pub(crate) struct PhysBlock {
    phys: PhysBytes,
    refcount: u16,
    flags: PhysBlockFlags,
}

pub(crate) struct PhysBlockArena {
    blocks: Vec<PhysBlock>,
    free_list: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PageSlot {
    block_idx: Option<u32>,
    offset: VirBytes,
    memtype: Option<&'static dyn MemType>,
}
```

**方案一 → 方案二的改动**：

| 改动 | 方案一 | 方案二 |
|------|--------|--------|
| PhysRegion 分配 | `Box<PhysRegion>` 独立分配 | 消除，退化为 `PageSlot` 内嵌在 VirRegion 中 |
| PhysBlock 引用 | `NonNull<PhysBlock>` 裸指针 | `Option<u32>` Arena 索引 |
| 侵入式链表 | `firstregion` + `next_ph_list` | 消除 |
| PhysBlock 分配 | `Box` 堆分配 | Arena 分配 |
| unsafe 代码量 | 高 | 中（Arena 索引安全，但 PhysBlock 仍需间接访问） |

**问题**：

1. **`PhysBlock` 被削薄为"鸡肋"**：消除了侵入式链表和独立分配后，`PhysBlock` 只剩下 `phys`、`refcount`、`flags` 三个字段。它不再是 Minix3 中那个"拥有链表头、管理引用者集合"的核心实体，而退化为一个薄数据容器。
2. **Arena 索引与物理页号本质相同**：`PageSlot.block_idx` 存储 Arena 索引，Arena 索引指向 `PhysBlock`，`PhysBlock.phys` 存储物理地址。但物理地址本身可以用物理页号表示——Arena 索引和物理页号都是"一个整数标识一个物理页"，维护两套索引增加了复杂度。

**结论**：方案二消除了方案一的 unsafe 和链表问题，但引入了新的尴尬——`PhysBlock` 被削薄后变得可有可无，Arena 索引与物理页号又存在冗余。这说明"保留三结构体"的框架本身可能就是问题所在。

**方案二的唯一价值**：作为迁移的中间态。从方案一出发，先做方案二的改动（消除 unsafe、消除链表），验证功能正确性，再过渡到下一步。

### 3.3 方案三：PFN（Page Frame Number）索引模型（最终方案）

**核心思想**：物理页是全局资源，用全局数组管理其状态；虚拟映射是进程局部资源，内嵌在 VirRegion 中。

- **物理页状态**：全局 `PageState[]` 数组，以 PFN 为索引
- **消除 PhysBlock 和 PhysRegion 独立结构体**
- **消除侵入式链表**：refcount 是唯一的状态追踪机制

> **信息丢失说明**：Minix3 通过 `firstregion → next_ph_list → phys_region.parent` 可以枚举"谁在引用这个物理页"。方案三只保留 `refcount` 数字，丢失了"引用者列表"信息。但如 §2.4 分析，生产代码从未遍历链表查询引用者——CoW 和 fork 都逐页独立操作，不依赖此信息。链表遍历仅在调试检查（`#if SANITYCHECKS`）和链表维护自身（摘除节点）中使用，调试检查可由 `verify_refcounts`（遍历所有进程，见 §3.3.5）替代，链表维护则完全不需要。

#### 3.3.1 Minix3 → 方案三映射

| Minix3 结构 | 作用 | 方案三对应 |
|------------|------|-----------|
| `phys_block` | 物理内存块，含引用计数和引用者链表 | `PageState`（全局数组元素） |
| `phys_block.phys` | 物理地址 | `PageFrames::pfn_to_phys(pfn)` |
| `phys_block.refcount` | 引用计数 | `PageState.refcount` |
| `phys_block.flags` | 标志位 | `PageState.flags` |
| `phys_block.firstregion` | 侵入式链表头 | 消除（refcount only） |
| `pb_new()` | 创建物理块（refcount=0） | `buddy.alloc_page()` + `map_page()`（refcount 直接从 0→1） |
| `pb_free()` | 释放物理块 | `ev_unreference` + `buddy.free_page()` |
| `pb_link()` | 将 phys_region 链接到 phys_block（refcount++） | `map_page()`（设置 PageSlot + refcount++） |
| `pb_unreferenced()` | 从 phys_block 解链 phys_region（refcount--） | `unmap_page()`（清除 PageSlot + refcount--） |
| `PBF_INCACHE` | 缓存标志 | `PageFlags::IN_CACHE` |
| `MAP_NONE` | 延迟分配标记 | `PFN_NONE`（`u32::MAX`） |

#### 3.3.2 PageState 与 PageFlags

```rust
#[repr(C)]
pub(crate) struct PageState {
    refcount: u16,
    flags: PageFlags,
    _padding: u8,  // 显式填充，确保 4 字节对齐，消除未初始化 padding
}

bitflags::bitflags! {
    pub(crate) struct PageFlags: u8 {
        const IN_CACHE   = 0x01;  // 物理页在页缓存中
        const PENDING_IO = 0x02;  // 物理页正在等待异步 I/O（文件映射缺页）
    }
}
```

**设计要点**：

1. **PFN 索引**：PFN = `phys_addr / PAGE_SIZE`。O(1) 查找，无需分配独立结构体。与 Minix3 的 `phys_block`（slab 分配的独立结构体，通过裸指针引用）相比，PFN 索引模型从根本上消除了指针安全问题。

2. **refcount 使用 u16 而非 u8**：`u16` 提供足够的安全裕量（Minix3 用 `u8_t`，最大 255），对齐更自然，避免 padding。debug 构建加 `saturating_add` 断言检测溢出。

3. **无 ALLOCATED 标志**：物理页的"已分配/空闲"状态由 buddy/bitmap 分配器独立追踪（见 [04-physical-memory.md](04-physical-memory.md)），PageFrames 不需要重复记录。`refcount` 足以描述 PageFrames 关心的状态：`refcount > 0` 表示有引用（映射或缓存），`refcount == 0` 表示无引用。buddy 和 PageFrames 是两个不同的关注点——buddy 关心"哪些页可用"，PageFrames 关心"哪些页被引用"——不需要在 PageFrames 中重复 buddy 的信息。CoW 流程中 `buddy.alloc_page()` 与 `map_page()` 之间存在短暂窗口（refcount=0 但 buddy 认为页已分配），但 VM 是微内核用户态单线程服务器，通过 IPC 串行处理请求，此窗口不会被并发访问观察到。

   > ⚠️ **异步 IPC 重入注意**：上述"单线程安全"结论**仅对同步操作成立**。文件映射缺页时，VM 必须向 VFS 发送异步读盘请求，在等待回复期间 VM 继续处理其他 IPC 请求。此时如果另一个进程映射同一 PFN，可能读到未初始化数据。`PENDING_IO` 标志用于保护此窗口：建立映射前检查 `PENDING_IO`，若设置则将当前请求挂起至该 PFN 的等待队列。

   > **替代方案**：若未来需要区分"buddy 已分配但无映射引用"（如诊断工具查询物理页状态），可在 `PageFlags` 中添加 `ALLOCATED` 标志，由 `alloc_pfn`/`free_pfn` 设置/清除。当前阶段无需此标志——buddy 和 PageFrames 的状态可通过交叉查询获得。

4. **无 memtype_tag**：不在 `PageState` 中存储内存类型。理由：
   - Minix3 的 `phys_block` 也不存储 memtype——memtype 存储在 `phys_region` 中
   - memtype 是映射属性，不是物理页属性。同一个物理页被不同类型的映射引用时，各映射有各自的 memtype
   - CoW 后 `ph->memtype = &mem_type_anon`——这是映射级别的变更，不是物理页级别的
   - 物理页释放逻辑（`ev_unreference`）由 `PageSlot.memtype` 决定，不需要 `PageState` 中的 tag

5. **无侵入式链表**：refcount 是唯一的状态追踪机制。

6. **flags 使用 bitflags 宏**：`bitflags!` 自动生成 `contains`、`insert`、`remove` 等方法，以及 `Debug`、`PartialEq` 等 trait 实现。

#### 3.3.3 PageFrames — 全局页帧表

```rust
pub(crate) struct PageFrames {
    states: Vec<PageState>,
    total_pages: u32,
}

impl PageFrames {
    pub fn new(total_phys: PhysBytes) -> Self {
        let total_pages = (total_phys.0 / PAGE_SIZE) as u32;
        // vec! 宏直接在堆上分配 total_pages 个 PageState
        let states = vec![PageState {
            refcount: 0,
            flags: PageFlags::empty(),
        }; total_pages as usize];
        Self { states, total_pages }
    }

    pub fn get(&self, pfn: u32) -> Option<&PageState> {
        self.states.get(pfn as usize)
    }

    pub fn get_mut(&mut self, pfn: u32) -> Option<&mut PageState> {
        self.states.get_mut(pfn as usize)
    }

    pub fn pfn_to_phys(&self, pfn: u32) -> PhysBytes {
        PhysBytes((pfn as u64) * PAGE_SIZE)
    }

    pub fn phys_to_pfn(&self, phys: PhysBytes) -> u32 {
        (phys.0 / PAGE_SIZE) as u32
    }
}
```

**生命周期**：`PageFrames` 是 VM 的全局单例，在 `VmServer::new()` 时创建，生命周期覆盖整个 VM 运行期间。所有操作通过 `&mut PageFrames` 参数访问页帧状态。

**内存开销**：4GB 物理内存 ≈ 1M 页 × 8 字节/页 = 8MB。可接受。

**PFN 语义**：`PFN_NONE = u32::MAX` 对应一个无效 PFN，替代 Minix3 的 `MAP_NONE`。`u32::MAX` 不会与任何合法 PFN 冲突——合法 PFN 范围为 `[0, total_pages-1]`。这比 `PFN_NONE = 0` 更安全（PFN 0 是合法的物理页）。如果需要更严格的区分，可用 `Option<u32>` 替代 `u32`，代价是多 4 字节/页。

> **PFN 寻址上限**：`u32` PFN × 4KB 页 = 16TB 物理内存上限。x86-64 的 MAXPHYADDR 通常为 48 位（256TB）或 52 位（4PB），但 16TB 对 minix-rs 的教学/研究目标绰绰有余。若未来需要支持超过 16TB 的物理内存，PFN 需升级为 `u64`，这将导致 `PageSlot` 从 16 字节膨胀。

**no_std 兼容性**：
- `Vec<PageState>` → 使用 `alloc` crate → ✅ 项目已在 [09-vm-relocation.md](09-vm-relocation.md) 完成自举，`VmAllocator`（基于 HeapArena）已实现 `GlobalAlloc`，`alloc` crate 可用
- `bitflags!` → `no_std` 兼容 → ✅
- `&'static dyn MemType` → trait object 需要 vtable → ✅ Rust core 语言特性，`no_std` + `alloc` 下可用

#### 3.3.4 物理页状态管理

> **职责边界**：`PageFrames` 只管理物理页的**引用计数**和**缓存标志**，不负责物理页的分配/释放。物理页分配由 buddy/bitmap 分配器完成（见 [04-physical-memory.md](04-physical-memory.md)）。

**典型调用流程**：

```
分配+映射：buddy.alloc_page() → 返回 PFN → map_page(pfn)（设置 PageSlot + refcount=1）
解映射+释放：unmap_page(pfn)（refcount--）→ refcount==0 → ev_unreference → buddy.free_page(pfn)
```

无需 `mark_allocated` / `mark_free`——buddy 和 PageFrames 是两个不同的关注点，buddy 关心"哪些页可用"，PageFrames 关心"哪些页被引用"，不需要在 PageFrames 中重复 buddy 的分配状态。

> **改进方向：依赖倒置**：当前 `unmap_page` 在 refcount 归零时直接触发 `ev_unreference` 回调，导致底层 `PageFrames` 逆向调用高层 `MemType`。更清晰的层级关系是：`PageFrames` 仅暴露 `release_ref(pfn) -> bool`（返回引用是否归零），由调用方（VirRegion）根据 memtype 驱动 `ev_unreference` 和 `buddy.free_page`。当前实现通过让 `unmap_page` 返回 `(pfn, memtype)` 给调用方来间接实现这一分离（见 §3.6.1），已缓解了依赖倒置问题。

#### 3.3.5 refcount 不变量

| 不变量 | Minix3 | 方案三 |
|--------|--------|--------|
| refcount ≥ 0 | `assert(pb->refcount > 0)` 在 `pb_unreferenced` 入口 | 同 |
| refcount = 映射数 | refcount = 链表长度（非缓存）或链表长度+1（缓存） | refcount = 引用此 PFN 的 PageSlot 数量 + (IN_CACHE ? 1 : 0) |
| refcount = 0 时可释放 | 链表为空 + ev_unreference | ev_unreference |
| 缓存页额外引用 | `PBF_INCACHE` flag + refcount 额外 +1 | `PageFlags::IN_CACHE` + refcount 额外 +1 |

**验证方法**（debug 构建）：

```rust
#[cfg(debug_assertions)]
fn verify_refcounts(frames: &PageFrames, procs: &[VmProc]) {
    let mut counts = vec![0u16; frames.total_pages as usize];
    for proc in procs {
        for vr in proc.regions() {
            for slot_opt in &vr.physblocks {
                if let Some(slot) = slot_opt {
                    if slot.is_mapped() {
                        counts[slot.pfn as usize] += 1;
                    }
                }
            }
        }
    }
    for (pfn, state) in frames.states.iter().enumerate() {
        let expected = counts[pfn]
            + if state.flags.contains(PageFlags::IN_CACHE) { 1 } else { 0 };
        assert_eq!(state.refcount, expected, "PFN {pfn} refcount mismatch");
    }
}
```

这与 Minix3 `map_sanitycheck` 的遍历方式相同（遍历所有进程的所有区域），但不再需要 `seencount` 字段和链表遍历。

> **触发时机**：`verify_refcounts` 仅在 `debug_assertions` 构建中存在，不影响 release 性能。debug 构建应以覆盖率优先——在缺页、fork、exit、munmap 等关键路径后均可触发，越密集越容易定位 refcount 不一致的根因。

#### 3.3.6 INCACHE 语义保留

Minix3 的页缓存有独立的数据结构（`cached_page` + LRU 链表 + 哈希表），与 `phys_block` 通过指针交互。方案三中，缓存交互简化为：

**addcache（缓存页加入）**：

```rust
fn addcache(pfn: u32, frames: &mut PageFrames) {
    let state = frames.get_mut(pfn).unwrap();
    state.refcount += 1;
    state.flags.insert(PageFlags::IN_CACHE);
}
```

等价于 Minix3 的 `hb->page->refcount++; hb->page->flags |= PBF_INCACHE;`

**rmcache（缓存页移除）**：

```rust
fn rmcache(pfn: u32, frames: &mut PageFrames) {
    let state = frames.get_mut(pfn).unwrap();
    state.flags.remove(PageFlags::IN_CACHE);
    state.refcount -= 1;
    if state.refcount == 0 {
        buddy.free_page(pfn);
    }
}
```

等价于 Minix3 的 `cp->page->flags &= ~PBF_INCACHE; cp->page->refcount--; if(pb->refcount==0) { free_mem(...); SLABFREE(pb); }`

**cache_freepages（缓存页回收）**：

Minix3 的 `cache_freepages()` 遍历 LRU 链表，对 `refcount == 1`（仅缓存持有）的页调用 `rmcache()`。方案三中，缓存页回收逻辑不变——仍然遍历 LRU 链表，检查 `PageState.refcount == 1 && IN_CACHE`。**不需要遍历进程的 VirRegion**，因为缓存有自己的 LRU 索引。

#### 3.3.7 与 Direct Map 的协同

> **注**：Direct Map 是 minix-rs 的架构特性，非 Minix3 C 源码原有概念。Minix3 VM 是用户空间进程，没有 Direct Map，需要 `vm_mappages` 手动修改页表来访问物理页。方案三依赖 Direct Map 实现 O(1) 物理地址→虚拟地址转换，详见 [09-vm-relocation.md](09-vm-relocation.md)。

1. **物理地址是唯一锚点**：`PageSlot.pfn` → `PageFrames.pfn_to_phys(pfn)` → `vm_phys_to_virt(phys)` 获取虚拟地址。

2. **CoW 复制简化**：Minix3 的 `sys_abscopy` 被 `vm_phys_to_virt() + copy_nonoverlapping()` 替代。VM 从"受信任的请求者"变为"物理内存的主人"。

3. **物理页内容访问**：通过 `vm_phys_to_virt(frames.pfn_to_phys(pfn))` 直接读写物理页内容，无需额外的映射窗口。

4. **VM 自身 heap 不需要 VirRegion**：minix-rs 的 `alloc` crate（基于 Direct Map）已经替代了 Minix3 的 `findhole` + `vm_mappages` 机制。详见 [09-vm-relocation.md](09-vm-relocation.md)。

> 方案三中 PageFrames 与分配器元数据存在冗余（同一物理页的分配状态被记录两次），冗余分析与未来融合方向见附录 B。

#### 3.3.8 错误路径设计

**PageFrames::new() 中 Vec 分配失败**：

VM 初始化阶段 `Vec<PageState>` 分配失败 → panic。这与 Minix3 一致——VM 初始化失败无法恢复。

**refcount 溢出**：

`u16` 上限 65535，远超实际需求（Minix3 用 `u8`，典型值 < 10）。debug 构建加 `saturating_add` 断言检测溢出。

**ev_unreference 回调中的递归借用**：

`unmap_page` 持有 `&mut PageFrames`，如果 `ev_unreference` 也需要 `&mut PageFrames`，会产生借用冲突。解决方案：`unmap_page` 返回需要释放的 `(pfn, memtype)`，由调用者在释放 `&mut PageFrames` 后再调用 `ev_unreference`。

```rust
fn unmap_page(
    &mut self,
    frames: &mut PageFrames,
    offset: VirBytes,
) -> Option<(u32, &'static dyn MemType)> {
    let page_idx = (offset.get() / PAGE_SIZE) as usize;
    let slot = self.physblocks[page_idx].take()?;
    if slot.is_mapped() {
        if let Some(state) = frames.get_mut(slot.pfn) {
            if state.refcount > 0 {
                state.refcount -= 1;
            }
            if state.refcount == 0
                && !state.flags.contains(PageFlags::IN_CACHE)
            {
                if let Some(mt) = slot.memtype {
                    return Some((slot.pfn, mt));
                }
            }
        }
    }
    None
}
```

调用者模式：

```rust
let pending = vr.unmap_page(&mut frames, offset);
if let Some((pfn, mt)) = pending {
    mt.ev_unreference(&mut frames, pfn);
    if frames.get(pfn).unwrap().refcount == 0 {
        buddy.free_page(pfn);
    }
}
```

### 3.4 方案详细对比

以上三个方案分别代表了从 Minix3 直译到重新建模的演进路径。以下从差异维度和综合维度两个层次进行对比。

**方案二 vs 方案三**（方案二与方案三的关键差异）：

| 维度 | 方案二 (PhysBlockArena) | 方案三 (PageState[pfn]) |
|------|------------------------|------------------------|
| 物理页查找 | Arena 索引 O(1) | PFN 直接索引 O(1) |
| 物理地址获取 | `arena.get(idx).phys` | `pfn * PAGE_SIZE` |
| PhysBlock 存在性 | 存在（Arena 中的元素） | 不存在（退化为 PageState） |
| 分配/释放 | Arena alloc/free | 无（PageState 是预分配的） |
| 内存开销 | 按需增长 | 固定（total_pages × 4B） |
| 间接层数 | 2 层（Arena idx → PhysBlock → phys） | 1 层（PFN → PageState） |

**三方案综合对比**：

| 维度 | 方案一（直译保留） | 方案二（最小改动） | 方案三（PFN 索引） |
|------|-------------------|-------------------|-------------------|
| unsafe 代码量 | 高 | 中 | 低（仅 Direct Map 操作） |
| 所有权清晰度 | 低 | 中 | 极高（单一全局拥有者） |
| O(1) 物理页查找 | 否（遍历链表） | 是（Arena 索引） | 是（PFN 索引） |
| 侵入式链表 | 需要 | 消除 | 不需要 |
| 独立分配 phys_block | Box 分配 | Arena 分配 | 不需要 |
| 独立分配 phys_region | Box 分配 | 消除 Box | 不需要 |
| 实现复杂度 | 中 | 中 | 低 |
| 与 Minix3 对应性 | 一致 | 保留 PhysBlock | 重新建模 |
| 内存开销 | 按需 | 按需 | 固定（~8MB/4GB RAM） |
| 迁移路径 | — | 可渐进 | 需要重写 |

> Minix3 为何选择 `phys_block` + slab 而非全局 PFN 数组？见附录 A。

---

## 4. 测试要点

- **PageFrames 初始化**：`total_pages` 计算正确性，`Vec` 长度等于 `total_pages`，所有 `PageState` 初始 `refcount=0`、`flags=empty()`
- **refcount 操作**：递增/递减/归零释放，`saturating_add` 在 debug 构建中断言溢出
- **INCACHE 语义**：缓存页 refcount 额外+1，`rmcache` 时正确递减，`refcount==1 && IN_CACHE` 时可回收
- **PFN 边界**：`pfn=u32::MAX`（PFN_NONE）、`pfn=total_pages-1`、`pfn` 越界（`get` 返回 None）
- **verify_refcounts**：多进程多区域场景下的一致性验证
- **错误路径**：refcount 溢出检测、`buddy.free_page` 时 refcount 非 0
- **与 Direct Map 的协同**：`pfn_to_phys` → `vm_phys_to_virt` 链路正确性

---

## 5. 参见

- [04-physical-memory.md](04-physical-memory.md) — 裸物理页分配（第 1 层）
- [05-vm-allocpage.md](05-vm-allocpage.md) — VM 自身内存分配
- [11-region-mapping.md](11-region-mapping.md) — PageSlot + VirRegion（使用 PageFrames API）
- [12-memtype.md](12-memtype.md) — MemType trait（ev_unreference 使用 PageFrames）
- [25-page-cache.md](25-page-cache.md) — 页缓存与 PageState 的交互

---

## 附录 A：Minix3 为何选择 phys_block + Slab 设计

> **全局 PFN 索引数组是业界主流做法**：Linux（`struct page` 数组）、FreeBSD（`vm_page_t` 数组）、Fuchsia（`pmm_node`）均采用全局数组以 PFN 为索引管理物理页状态。Minix3 的 `phys_block` + slab 分配 + 侵入式链表是异类，源于其独特的历史约束（用户空间 VM、无 Direct Map、小内存目标）。以下解释这些约束如何导致了这一非主流选择。

Minix3 源码中**没有**任何注释解释为什么选择 `phys_block` + slab 分配 + 侵入式链表这种设计。Minix3 wiki 的 [VM internals](https://wiki.minix3.org/doku.php?id=developersguide:vminternals) 页面也只描述了数据结构是什么，没有解释为什么。以下分析基于历史脉络和代码特征推断。

### A.1 VM 是后加的，不是一开始就设计的

Tanenbaum 在 2016 年的 CACM 文章 [Lessons Learned from 30 Years of MINIX](https://cacm.acm.org/magazines/2016/3/198874-lessons-learned-from-30-years-of-minix/fulltext) 中明确说：

> "My initial decision back in 1984 to have fixed-size messages throughout the system and **avoid dynamic memory allocation (such as malloc) and a heap in the kernel** has not been a problem and avoids problems that occur with dynamic storage management (such as memory leaks and buffer overruns)."

Minix3 最初（2005年发布时）**根本没有虚拟内存**，PM 用的是最简单的 hole list + first fit 分配。VM server 是后来（大约 2009 年前后，由 Ben Gras 实现）加上去的。

这意味着 VM 的设计者面临一个独特的约束：**VM 自身是一个用户空间进程，它需要管理自己的内存分配，但它不能依赖内核来分配内存**（因为它自己就是内存管理者）。所以 VM 自己实现了一个 slab allocator（`slaballoc.c`），用来分配 `phys_block`、`phys_region` 等运行时对象。

### A.2 用户空间进程没有 Direct Map

这是最根本的原因。Linux 的 `struct page` 数组之所以可行，是因为 Linux 内核有 **Direct Map**（也叫 `physmap`）——所有物理内存都线性映射到内核虚拟地址空间，内核可以直接通过 `phys_to_virt(paddr)` 访问任何物理页。

但 Minix3 的 VM 是**用户空间进程**，它没有 Direct Map。它只能通过 `vm_mappages` 手动修改自己的页表来映射物理页。在这种约束下：

- **全局 `PageState[pfn]` 数组需要预先分配一大块连续虚拟内存**，然后映射足够的物理页来存储它。这在 VM 进程启动时就需要知道总物理内存量，并且需要找到一个足够大的虚拟地址空洞。
- **`phys_block` + slab 分配是增量式的**：只在需要时分配，不需要预先预留大块地址空间。对于一个小内存系统（Minix3 最初面向 256MB~1GB 的嵌入式设备），这更简单。

### A.3 Minix3 VM 是单线程的

VM 进程是**单线程事件驱动**的（Tanenbaum说的 "event-driven model"）。这意味着：
- 不需要锁——所有数据结构访问都是串行的
- 侵入式链表不需要原子操作
- 不需要 RMAP（反向映射）——遍历 `firstregion` 链表就能找到所有引用者

在 Linux 中，`struct page` 数组 + `_mapcount` + RMAP 是为了多核并发和快速反向映射。Minix3 完全不需要这些。

### A.4 目标规模不同

Minix3 的设计目标是嵌入式/低内存系统（Tanenbaum 原文："MINIX 3 is targeted to some extent at low-end systems such as embedded systems"）。在 256MB~1GB 的系统中：
- 全局数组方案：256MB → 64K 页 × 4B = 256KB（方案三）或 64K × 64B = 4MB（Linux struct page）
- Slab 方案：按需分配，只分配实际使用的 `phys_block`

在小内存系统中，**按需分配比预分配数组更节省**。只有当物理内存大到数 GB 时，全局数组的 O(1) 查找优势才 outweigh 预分配的开销。

### A.5 "简单优先"的设计哲学

Tanenbaum 在书中明确列出了 Minix3 内存管理设计的三个出发点：

> 1. The desire to keep the system easy to understand
> 2. The architecture of the original IBM PC CPU (an Intel 8088)
> 3. The goal of making MINIX 3 easy to port to other hardware

虽然这是针对早期无分页的 PM 设计说的，但这个哲学延续到了 VM 的实现。`phys_block` + `phys_region` + `vir_region` 的三层结构虽然看起来间接，但每一层的职责很清晰：
- `vir_region`：虚拟地址空间
- `phys_region`：虚拟→物理的映射关系（含 memtype）
- `phys_block`：物理页的引用计数

这比 Linux 的 `struct page`（64 字节，十几个 union 字段，需要理解 slab/lru/compound/tail 四种复用模式）**容易理解得多**。

### A.6 对 minix-rs 的启示

Minix3 当"异类"不是有意为之，而是**历史约束的自然结果**——用户空间 VM 进程 + 无 Direct Map + 单线程 + 小内存目标 + 简单优先哲学。这些约束在 minix-rs 中大部分已经不存在了：

| 约束 | Minix3 | minix-rs |
|------|--------|----------|
| VM 运行环境 | 用户空间进程，无 Direct Map | 同样是用户空间，但 Rust `alloc` 可用 |
| 内存分配 | 自研 slab allocator | Rust `alloc` crate（全局分配器） |
| 并发模型 | 单线程事件驱动 | 可设计为多线程（Rust 的安全并发） |
| 目标内存规模 | 256MB~1GB 嵌入式 | 可能支持更大内存 |
| 安全保证 | C 语言，无保证 | Rust 所有权 + 类型系统 |

因此，方案三的全局 `PageState[pfn]` 数组是对 Minix3 `phys_block` 的合理演进——在约束条件变化后，选择业界主流做法。

---

## 附录 B：物理页的 per-page 数据结构融合

> 本附录讨论当前设计的已知冗余，以及未来 redesign 阶段可能的融合方向。当前实现仍保持 PageState 与分配器元数据独立。

### B.1 当前状态：3 类数据结构管理同一个物理页

采用方案三（全局 PageState 数组）+ 物理页分配器 + Direct Map 时，管理同一个物理页帧的数据结构有 3 类（Direct Map 实际有两份——Kernel Direct Map 和 VM Direct Map，两者映射同一物理内存但特权级不同，详见 [06-pagetable-struct.md](06-pagetable-struct.md)）：

| 数据结构 | 关注点 | 回答的问题 | 大小/页 |
|----------|--------|-----------|---------|
| Direct Map（×2） | 寻址 | "如何通过虚拟地址访问这个物理页？" | 0（硬件映射） |
| PageState[pfn] | 语义 | "被引用几次？在缓存中吗？" | 4B |
| Bitmap 或 Buddy 元数据 | 分配 | "哪些页空闲？哪里有连续 N 页？" | Bitmap: 1bit；Buddy: ~5B |

**关键前提**：Bitmap 和 Buddy 是二选一的物理页分配器，实际运行时只存在其中一个。因此 per-page 的软件数据结构只有 **PageState + 分配器元数据** 两个，而非三个。

### B.2 冗余分析：bitmap 的 1-bit 是 refcount 的退化

Bitmap 分配器的每个槽位是 1 bit：0 = 空闲，1 = 已分配。而 PageState 的 `refcount` 语义为：`refcount == 0` = 空闲，`refcount > 0` = 已分配且被引用。bitmap 的 1-bit 本质上是 refcount 的 1-bit 退化版本——将 u16 refcount 压缩为 1 bit，丢失了"被引用几次"的信息。

同理，Buddy 分配器的 `page_orders[pfn]` 高位存 `FLAG_ALLOCATED`（0x80），也是 `refcount > 0` 语义的退化。

同一个物理页的分配状态被记录了两次（分配器一次、PageState 一次），这是当前设计的主要冗余。

### B.3 融合方向：将 refcount + flags 融入分配器 per-page 元数据

> 以下为未来 redesign 阶段的探索方向，当前实现不准备改动。

核心思路：**消除独立的 PageState 数组，将 refcount 和 flags 打包进分配器的 per-page 元数据**。分配状态（0/1）自然扩展为"0/1+"（空闲 / 已分配且被引用 N 次），职责并未模糊——"这个页是否被分配"和"被引用几次"本来就是同一件事的不同精度描述。

#### Buddy 路线

当前 Buddy 的 `page_orders[pfn]` 是 u8（高 1 bit 存 `FLAG_ALLOCATED`，低 7 bit 存 order）。融合方案将其扩展为 u32：

```rust
// 融合前：Buddy + PageState 各自独立
//   page_orders[pfn]: u8  (FLAG_ALLOCATED | order)
//   PageState[pfn]:   u16 refcount + u8 flags = 4B
// 合计: 5B/页

// 融合后：一个 u32 打包
struct PfnState {          // 4B/页，比融合前更省
    order: u8,             // buddy order（空闲时有效）
    refcount: u16,         // 引用计数（0=空闲，>0=已分配）
    flags: u8,             // IN_CACHE 等标志
}
```

融合后 Buddy 仍需 `page_next[]`（空闲链表 next 指针，4B/页）和 `free_list_heads[]`（每个 order 一个链表头），但 per-page 的核心状态只需一个数组。

**内存对比**（16GB 系统，4M 页）：

| 方案 | per-page 大小 | 总计 |
|------|--------------|------|
| 当前：Buddy(5B) + PageState(4B) | 9B | 36MB |
| 融合：PfnState(4B) + page_next(4B) | 8B | 32MB |

#### Bitmap 路线

当前 Bitmap 的每个槽位是 1 bit。融合方案将 1-bit 扩展为 u16 + flags：

```rust
// 融合前：Bitmap(1bit) + PageState(4B)
// 融合后：
struct PfnState {          // 4B/页
    refcount: u16,         // 引用计数
    flags: u8,             // IN_CACHE 等标志
    _pad: u8,              // 对齐
}
```

Bitmap 的连续性查找（找连续 N 个空闲页）改为遍历 `refcount == 0` 的连续区间。性能从 O(1) bit 操作退化为 O(n) 扫描，但 Bitmap 分配器本身就是为了教学目的保留的简单实现，性能不是首要考量。

### B.4 IN_CACHE 的归属

`IN_CACHE` 标志表示"物理页同时在页缓存中"，配合 refcount 的额外 +1 使用。融合后它仍在 `PfnState.flags` 中，职责略混——它属于缓存子系统的关注点，而非物理页分配/引用的关注点。

更纯粹的方案是将 IN_CACHE 移到独立的缓存结构（如 `PageCache` 维护自己的 LRU + `HashSet<PFN>`），`PfnState` 只管 refcount + order。但这需要缓存回收时额外查缓存结构，增加间接访问。当前阶段保留 IN_CACHE 在 per-page 结构中是务实选择，未来 redesign 时可评估是否拆分。

### B.5 融合后的最终结构

| 路线 | per-page 数组 | 额外结构 | Direct Map |
|------|--------------|---------|------------|
| Buddy + Direct Map | `PfnState[pfn]`（4B: order+refcount+flags） | `page_next[]` + `free_list_heads[]` | ×2（kernel + VM） |
| Bitmap + Direct Map | `PfnState[pfn]`（4B: refcount+flags+pad） | 无 | ×2（kernel + VM） |

如果未来将 VM 移入内核态（作为内核中的独立进程运行），Kernel Direct Map 和 VM Direct Map 合二为一，最终只剩 **1 个 per-page 数组 + 1 个 Direct Map**。

### B.6 当前决策

当前实现保持 PageState 与分配器元数据独立，原因：

1. **渐进式开发**：方案三（PageState + 分配器分离）已是对 Minix3 三结构体模型的重大简化，先验证核心逻辑正确性
2. **IN_CACHE 移出涉及页缓存 redesign**：融合后 `PfnState` 只剩 order + refcount，语义清晰；但 IN_CACHE 的归属变更属于页缓存子系统的 redesign，当前为 rewrite 阶段，除必要的 redesign 外优先保持原语义
3. **多分配器对比有教学价值**：项目实现了 Bitmap、Buddy、SegmentTree 三种分配器，保留独立元数据便于对比各分配器的 per-page 开销与行为差异

融合方案留给未来 redesign 阶段评估——当前 rewrite 阶段优先保证与 Minix3 语义一致，待核心 VM 功能稳定后再考虑结构优化。

---

## 附录 C：buddy 块的统一管理与 CoW 拆分

> 当前 minix-rs VM 层以 4KB 粒度运行，大页支持不是立即需求。本附录分析当前 per-4KB 管理的痛点，以及 buddy 块天然提供的分组能力如何被 CoW 打碎，供未来 redesign 参考。

### C.1 现状：per-4KB 管理的痛点

Minix3 的整个 VM 层硬编码了 4KB 粒度：

- `phys_slot(len) = len / VM_PAGE_SIZE` — `physblocks[]` 数组每个槽位对应一个 4KB 虚拟页
- `physblock_get(region, offset)` → `i = offset / VM_PAGE_SIZE` — 按 4KB 索引
- `pb_new(phys)` 只接受一个物理地址，没有 size 参数 — 隐含 4KB
- `pb_free(pb)` → `free_mem(ABS2CLICK(pb->phys), 1)` — 固定释放 1 页

方案三继承了 per-4KB 的管理方式：`PageState[pfn]` 每个 PFN 对应一个 4KB 页，`PageSlot` 每个 slot 描述一个 4KB 映射。这在以下场景产生问题：

**痛点 1：大段映射的元数据膨胀**。一个 2MB 的只读代码段需要 512 个 `PageSlot`，每个 slot 存 `pfn` + `offset` + `memtype`。但这些 slot 的 `memtype` 完全相同、`protection` 完全相同、物理页连续——它们本质上是**一个映射单元**，却被当作 512 个独立映射管理。

**痛点 2：大页 PTE 的 TLB 优势无法利用**。512 个 4KB PTE 占一整页页表（4KB），TLB 需要 512 个条目。如果用 1 个 2MB 大页 PTE，页表只需 1 个条目，TLB 也只需 1 个。但 per-4KB 的管理方式无法表达"这 512 个页应该用大页映射"。

**痛点 3：refcount 操作逐页执行**。映射/解映射一个 2MB 区域需要 512 次 refcount 增减，每次都是独立的数组访问。如果整个块的 refcount 语义相同（同时映射、同时解映射），逐页操作是冗余的。

### C.2 buddy 天然提供块分组

buddy 分配器管理的不是"4KB 页"，而是**连续物理内存块**——最小粒度 4KB（order=0），往上 8KB、16KB、...、2MB（order=9）、1GB（order=18）。buddy 分配的块天然满足两个属性：

1. **物理连续**：order=N 的块是 2^N 个连续 4KB 页
2. **对齐**：order=N 的块起始地址是 2^N × 4KB 对齐的

这两个属性恰好是大页 PTE 的硬件要求。因此 buddy 块与页表映射存在天然对应：

| buddy 块 | 页表映射方式 |
|----------|-------------|
| order=0（4KB） | 1 个 4KB PTE |
| order=9（2MB） | 1 个 2MB 大页 PTE，或 512 个 4KB PTE |
| order=18（1GB） | 1 个 1GB 大页 PTE，或 512 个 2MB PTE，或 262144 个 4KB PTE |

**关键洞察**：buddy 块是物理分配的粒度，页表映射是虚拟映射的粒度，两者独立但可协同。一个 2MB buddy 块可以用大页 PTE 映射（高效），也可以用 4KB PTE 映射（灵活），取决于 VM 的选择。

**buddy 块作为统一管理单元**：如果 VM 向 buddy 申请 order=9（2MB），这个块在 VM 层面也应该是一个管理单元——1 个 PageSlot、1 次 refcount 操作、1 个 memtype。这比 512 个独立 PageSlot 更高效，语义也更清晰。

### C.3 CoW 打碎 buddy 块

buddy 块作为统一管理单元面临一个根本矛盾：**CoW 可以打碎它**。

```
fork() 前：父进程的 2MB 数据段 → 1 个 buddy 块（order=9）
           1 个 PageSlot，1 次 refcount

fork() 后：子进程共享同一物理块，refcount=2

父进程写某个 4KB → CoW！
  - 该 4KB 需要新物理页（从 buddy 分配 order=0）
  - 其余 511 个 4KB 仍共享
  - 管理方式变为：1 个共享块（511/512 有效）+ 1 个独立 4KB 页
```

CoW 后，原本的统一管理单元被拆分。如果 PageSlot 只描述整个块，CoW 时要么：

- **复制整个 2MB 块**（浪费 511 × 4KB = 近 2MB 内存）
- **拆分 PageSlot**（退化为 per-4KB 管理，失去统一管理的优势）

这是大页/大块管理的基本矛盾：**共享时希望粒度大（效率），CoW 时需要粒度小（节省）**。

### C.4 Linux 的解法：Compound Page

Linux 用 compound page 处理这个矛盾：

```rust
bitflags::bitflags! {
    pub(crate) struct PageFlags: u8 {
        const IN_CACHE = 0x01;
        const HEAD = 0x02;      // 大块的头页
        const TAIL = 0x04;      // 大块的尾页
    }
}

pub(crate) struct PageState {
    refcount: u16,
    flags: PageFlags,
    order: u8,              // 0=4KB, 9=2MB, 18=1GB
}
```

- **Head page**（PFN 最低的槽位）：存储 refcount、order、完整状态
- **Tail pages**（其余槽位）：`flags = TAIL`，refcount 无意义

**未 CoW 时**：整个 2MB 块用 1 个 head page 管理，页表用 1 个 2MB 大页 PTE。1 个 PageSlot 指向 head PFN。

**CoW 后**：被 CoW 的 4KB 从 compound page 中拆出，变成独立的 4KB 页（order=0，无 HEAD/TAIL 标志），分配新的物理页。其余页仍属于原 compound page。页表中该 4KB 对应的 PTE 改为指向新页，其余 PTE 不变。

**PageSlot 的变化**：CoW 前 1 个 PageSlot 描述整个 2MB 映射；CoW 后需要拆分为 2 个 PageSlot——1 个描述共享部分（可能仍是大页 PTE），1 个描述独立 4KB。极端情况下（每个 4KB 都被 CoW），退化为 512 个 PageSlot。

### C.5 回到原点：phys_block 天然支持大块

理解了 compound page 后，一个自然而然的问题是：Minix3 的 `phys_block` 处理大块反而更简单，方案三是不是走弯路了？

**phys_block 的优势**：`phys_block` 是动态分配的对象，天然与大小无关。要支持大块，只需加一个 `size` 字段：

```rust
pub(crate) struct PhysBlock {
    phys: PhysBytes,
    size: PhysBytes,    // 新增：物理块大小（4KB / 2MB / 1GB）
    refcount: u16,
    flags: PhysBlockFlags,
}
```

一个 2MB 的物理块就是 1 个 `phys_block` 对象，不需要 head/tail 标记，不需要 compound page 逻辑。CoW 拆分时，把原 `phys_block` 的 `size` 缩小、新建一个 4KB 的 `phys_block` 即可——概念上比 compound page 更直觉。

**但 phys_block 的"简单"有代价**：§3.1 已分析过——裸指针 unsafe、侵入式链表 O(n) 维护、独立堆分配开销。这些代价与大块支持无关，是 `phys_block` 作为动态对象本身的固有缺陷。compound page 虽然多了一步（head/tail 标记），但它建立在方案三的全局数组基础上，没有这些固有缺陷。

**对比总结**：

| 维度 | `phys_block`（动态对象） | `PageState[pfn]`（全局数组） |
|------|------------------------|---------------------------|
| 大块表示 | 加 `size` 字段即可 | Compound Page（head + tail） |
| 一个 2MB 块 | 1 个 `phys_block` | 1 head + 511 tail PageState |
| 内存开销 | 仅 1 个结构体 | 512 × 4B = 2KB（但 tail 页状态极简） |
| 查找效率 | 需要遍历或额外索引 | PFN 直接索引 head page |
| CoW 拆分 | 缩小 size + 新建 4KB phys_block | 拆分 compound page + 更新 PageSlot |
| 扩展难度 | 低（加字段） | 中（需实现 compound page 逻辑） |
| 固有缺陷 | unsafe + 链表 + 独立分配 | 无 |

**结论**：`phys_block` 在大块支持上确实更直觉，但这是以 §3.1 的固有缺陷为代价的。方案三需要 compound page 是全局数组模型的必然结果——Linux 的 `struct page` 数组也走了同一条路，证明这是可行的。

---

## 附录 D：跨操作系统对比

| OS | 物理页状态结构 | 索引方式 | 大小/页 | 大块支持 |
|----|-------------|---------|--------|---------|
| **Linux** | `struct page` | PFN → `mem_map[]` / `vmemmap` | 64B | Compound Page + THP |
| **FreeBSD** | `vm_page_t` | PFN → `vm_page_array[]` | ~100B | Superpage |
| **Fuchsia** | `vm_page_t` | 全局池 + VMO radix tree | ~32B | VMO 大页 |
| **Minix3** | `phys_block` | slab 分配 + 侵入式链表 | ~24B | 无 |
| **方案三** | `PageState` | PFN → `Vec<PageState>` | 4B | Compound Page（可扩展） |

**关键洞察**：全局 PFN 索引数组是主流做法（Linux、FreeBSD、Fuchsia），方案三与之同源。Minix3 的 `phys_block` 是异类（独立 slab 分配 + 侵入式链表），源于历史约束。方案三的 `PageState` 是最精简的全局数组实现（4B/页 vs Linux 的 64B/页 vs FreeBSD 的 ~100B/页）。

---

*分类: VM私有*
