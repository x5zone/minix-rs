# 05-physical-memory: 物理内存分配——VM 的"地盘账本"

> **分类**: 阶段 2 — 访问控制与物理内存（物理内存锚点）
> **源码**: `minix3/minix/servers/vm/alloc.c`（548 行）；`minix3/minix/servers/vm/utility.c:44-79`（`get_mem_chunks`）；`minix3/minix/servers/vm/main.c:428-520`（`init_vm` 调用点）；`minix3/minix/include/minix/type.h:157-160`（`struct memory`）；`minix3/minix/include/minix/param.h:13-19`（`MAXMEMMAP`/`kinfo.memmap`）；`minix3/minix/servers/vm/vm.h:22-27,62`（`PAF_*`/`NO_MEM`）；`minix3/minix/include/minix/const.h:84-101`（click 宏）
> **Rust 模块**: `os/servers/vm/src/phys_mem/`（`mod.rs`/`types.rs`/`alloc_trait.rs`/`bitmap_alloc.rs`/`buddy_alloc.rs`/`segment_tree_alloc.rs`/`stats.rs`/`allocator_tests.rs`）+ `os/servers/vm/src/boot.rs` + `os/servers/vm/src/global.rs` + `os/servers/vm/src/vm_server.rs:230-350,505-600` + `os/servers/vm/src/query.rs:296-305`
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/00-vm-overview.md`（启动主线）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`（`init_vm` 调用点）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md`（Direct Map `A-1` 概念，`PAF_CLEAR` 清零机制的前置）
> **说明**: 物理内存分配的语义模块：**内存清单获取 / 分配器初始化 / 任意大小连续块分配与释放 / 记账与诊断 / 保留队列机制**。**不覆盖**：`vm_allocpage` 页分配与保留页池消费（`06-page-allocator`）、元数据搬迁 `relocate`（`10-vm-relocation`）、块缓存回收 `cache_freepages`（`24-page-cache`）。

---

## 1. 概念：物理内存分配——VM 的"地盘账本"

### 1.0 章节引言

本文档回答 01 文档启动链上的一个问题：**VM 如何从内核手里接过物理内存，并把它变成可分配的页池**。它在 `init_vm()` 中的位置是：

```
init_vm() ──► sys_getkinfo()（main.c:442，取 kernel_boot_info）
           ──► get_mem_chunks()（main.c:455，内存清单 → click 取整）
           ──► mem_init(mem_chunks)（main.c:471，分配器接管）
           ──► mem_add_total_pages()（main.c:489/495，总页数校准）
```

这条链把**物理内存**从"内核 boot 协议里的一个字节区间数组"变成"VM 可以按页借出、收回的账本"。与页表文档（07/08）的分工是：**页表管"怎么映射"（虚拟地址 ↔ 物理页），本文档管"谁拥有哪段物理内存"（分配与回收）**。分配器是页表操作的物理侧供给者——07/08 每次建立映射之前，都要先从这里借一页物理内存。

### 1.1 内存来源链：VM 不探测，只消费

VM 不是硬件探测者。物理内存清单的完整链路是：

```
bootloader（multiboot 内存图）
  → kernel pre_init 收集到 kinfo.memmap[]（param.h:18，最多 MAXMEMMAP=40 块）
  → kernel 已扣除自身占用（内核 text/data/bss 不在 memmap 中）
  → sys_getkinfo IPC 传给 VM（main.c:442）
  → get_mem_chunks() 把字节区间按 click 取整（utility.c:44-79）
  → mem_init() 把取整后的区间标记进分配器（alloc.c:306-335）
```

两个值得注意的事实：

1. **VM 拿到的是"可用内存"而非"全部内存"**：`memmap[]` 里的区间是 kernel 已经减去自身占用后的空闲物理内存。kernel 自己占了多少，VM 只能事后通过 `kernel_allocated_bytes` 字段"知道"（§2.7 的 `mem_add_total_pages` 校准）。
2. **接口是一次性的**：`sys_getkinfo` 只在 `init_vm()` 里调用一次，之后 VM 再没有第二条途径获知物理内存布局。因此 Rust 把这次交接建模为显式的 `BootParams`（§3.5），而不是藏在全局变量里。

### 1.2 Click 单位系统：页号是"元单位"

Minix3 的内存分配单位是 **click**，即 4KB 页（`const.h:84-85`：`CLICK_SIZE 4096`/`CLICK_SHIFT 12`）。转换宏（`const.h:91-101`）：

```
CLICK2ABS(v)   = v << 12   页号 → 字节地址
ABS2CLICK(a)   = a >> 12   字节地址 → 页号
CLICK_FLOOR(n) = (n / 4096) * 4096        向下取整
CLICK_CEIL(n)  = ((n + 4095) / 4096) * 4096  向上取整
```

`alloc_mem` 返回的是**页号**（`phys_clicks`），不是字节地址——调用方需要 `CLICK2ABS` 才能得到物理地址。`get_mem_chunks` 的取整规则正是这两个宏：**起点向上取整（`CLICK_CEIL`）、终点向下取整（`CLICK_FLOOR`）**，保证每个区间天然页对齐、不越界。

### 1.3 连续区间分配问题：分配的本质

物理内存分配是一个**连续区间分配问题**：

```
输入：空闲区间集合（来自 mem_init）+ 请求（大小 clicks + 约束 flags）
输出：起始页号（满足对齐/低端/清零约束）或失败（NO_MEM）
约束：连续（bitmap 天然满足）、对齐（64K/16K）、低端（16MB/1MB）
```

Minix3 用一个**每页 1 bit 的位图**表达"谁空闲"：bit=1 空闲、bit=0 已用（§2.1）。分配 = 找到一段连续空闲位并清零；释放 = 置回 1。位图的好处是**任何大小、任何对齐的连续块都只需 O(1) 记账**（改几个 bit），代价是**分配扫描 O(n)**。

### 1.4 自举循环依赖：分配器不能给自己分配内存

分配器的元数据（位图、buddy 数组、线段树节点）本身需要内存。这里有一个自指问题：**在分配器就绪之前，没有任何东西可以为它分配内存**。三种解法：

| 方案 | 做法 | 代表 |
|------|------|------|
| 静态预留 | 按地址空间上限固定元数据大小，编译期放进 BSS | Minix3（32 位 4GB 上限 → 位图固定 128KB，alloc.c:33-35） |
| 先导分配器 | 用一个极简 bump 分配器先接管内存，主分配器初始化后退役 | Linux bootmem/memblock；Redox RMM 的 `BumpAllocator` |
| 预映射内存 | 从已映射的物理内存切一块做元数据，之后可搬迁 | minix-rs（Direct Map 区 + `BumpBuf`，§3.2） |

Minix3 的静态方案在 32 位下成立，是因为 `NUMBER_PHYSICAL_PAGES` 直接按 4GB 上限算（alloc.c:33），无论实际内存是 512MB 还是 4GB，位图都是 128KB。64 位下物理内存没有硬上限，元数据必须随 `total_pages` 动态确定大小——这就是 Rust 必须引入"预映射内存 + BumpBuf"自举路径的根本原因（§3.2）。

### 1.5 约束维度：对齐 / 低端 / 清零 / 连续

`alloc_mem` 的 `memflags`（`vm.h:22-27`）表达四类约束：

| 约束 | flag | 用途 |
|------|------|------|
| 64KB 对齐 | `PAF_ALIGN64K` (0x04) | DMA 描述符等硬件对齐要求 |
| 16KB 对齐 | `PAF_ALIGN16K` (0x40) | 较弱的对齐要求 |
| 16MB 以下 | `PAF_LOWER16MB` (0x08) | ISA DMA 只能访问低端内存 |
| 1MB 以下 | `PAF_LOWER1MB` (0x10) | 更严格的低端限制 |
| 清零 | `PAF_CLEAR` (0x01) | 分配返回前物理页必须全零 |
| 物理连续 | `PAF_CONTIG` (0x02) | **定义但从未使用**（位图分配天然连续，region 层的连续/非连续区分在 12-memtype） |

低端约束是"搜索范围"约束（`alloc_pages` 里把 `maxpage` 限到边界），对齐约束是"预加大 + 裁剪"（§2.4），清零约束是"分配后清"（§2.5）。

### 1.6 分配器三选一：策略模式（`[ARCH: A-5]`）

Minix3 只有一个位图分配器（`alloc.c`）。minix-rs 把它演进为**三个可互换后端 + 一个 trait**（`[ARCH: A-5]`，见 plan.md §4）：

| 后端 | 分配复杂度 | 元数据 | 适合 |
|------|-----------|--------|------|
| bitmap | O(n) 扫描 + O(1) 记账 | 1 bit/页（最省） | 中小内存（默认） |
| buddy | O(log n) 拆分/合并 | 页数 × (4B+4B+1B) | 大内存（≥1M 页 ≈ 4GB） |
| segment-tree | O(log n) 任意连续度 | 2×2^k×32B（最贵） | 实验性（feature 门控） |

这个演进与主流 OS 的路径一致：Redox 的 RMM 从 boot 期 `BumpAllocator` 切换到主 `BuddyAllocator`（`FrameAllocator` trait，11 个 order），Linux 用 buddy（页粒度）+ slab（小对象）双层。minix-rs 的差异是**不做固定演进、而是运行时按内存规模选后端**（§3.3）：VM 是用户态服务器，内存规模跨度大，单一后端无法同时兼顾位图的内存效率与 buddy 的分配延迟。

### 1.7 记账：总量 vs 空闲量

两个容易混淆的数字：

- **`total_pages`**（`glo.h:45`）：系统物理内存总页数。`mem_init` 从 chunks 累加（alloc.c:331），`mem_add_total_pages` 事后校准（§2.7）。**初始化后基本不变**，只在启动早期追加。
- **`memstats`**（alloc.c:348-367）：分配器当前**空闲**量——空闲块数（nodes）、空闲页数（pages）、最大连续块（largest）。"总内存 4GB"和"当前还剩 512MB"是两个不同的问题。

Rust 侧把前者放进 `BootParams.total_pages` + 全局 `TOTAL_PAGES`，后者建模为 `PhysMemStats` 结构体（§3.6），并接线到 `VM_INFO` 查询（query.rs:297）。

### 1.8 与 06 的分工边界：块分配 vs 页分配

物理内存分配在 Minix3 里分两层：

| 层 | 入口 | 语义 | 文档 |
|----|------|------|------|
| 任意大小连续块 | `alloc_mem(clicks, flags)` | 按 click 数借出连续物理内存 | 本文档（05） |
| VM 自身页分配 | `vm_allocpage`/`vm_allocpages` | 单页/多页 + Direct Map 映射，供页表/内核数据结构用 | 06-page-allocator |

`vm_allocpage` 是 `alloc_mem` 的**消费方**（06 文档覆盖），它把"借到一页物理内存"变成"借到一页已映射的虚拟内存"。本文档只负责底层账本。同样，**保留队列的消费方 `alloc_cycle` 主体归 06**——本文档只覆盖 `reservedqueue_*` 的队列机制与 `missing_spares` 记账（§2.9）。

### 1.9 本章小结

物理内存分配是 VM 的"地盘账本"：

- **来源**：VM 不探测内存，只消费 kernel 一次性传来的 `memmap[]` 清单（`get_mem_chunks` → `mem_init`）；
- **单位**：click（4KB），返回页号而非字节地址；
- **问题**：连续区间分配，Minix3 用位图 + 首次适配反向扫描；
- **自举**：分配器元数据自身需要内存——Minix3 静态 BSS，minix-rs 预映射内存 + `BumpBuf`；
- **约束**：对齐/低端/清零/连续四类，映射到 `PAF_*` 标志；
- **演进**：单一位图 → bitmap/buddy/segment-tree 三后端策略模式（`[ARCH: A-5]`）；
- **记账**：`total_pages`（总量）与 `memstats`（空闲量）是两个不同的问题。

后续章节依次回答：C 如何用位图实现（§2）→ Rust 如何用 trait + 三后端重表达（§3）→ 代码如何落地（§4）→ 测试如何证明（§5）。

---

## 2. C 源码分析

### 2.1 数据结构（alloc.c:33-72）

alloc.c 顶部定义了三组数据结构：

```c
// alloc.c:33-38 —— 位图 + 单页缓存
#define NUMBER_PHYSICAL_PAGES (int)(0x100000000ULL/VM_PAGE_SIZE)  // 1M 页（32 位上限）
#define PAGE_BITMAP_CHUNKS BITMAP_CHUNKS(NUMBER_PHYSICAL_PAGES)
static bitchunk_t free_pages_bitmap[PAGE_BITMAP_CHUNKS];           // 128KB
#define PAGE_CACHE_MAX 10000
static int free_page_cache[PAGE_CACHE_MAX];                        // 单页 LIFO 缓存
static int free_page_cache_size = 0;

// alloc.c:41 —— 地址范围（sanity check 用）
static phys_bytes mem_low, mem_high;

// alloc.c:47-51 —— SANITYCHECKS 双分配检测表
struct { int used; const char *file; int line; } pagemap[NUMBER_PHYSICAL_PAGES];

// alloc.c:56-72 —— 保留队列
#define RESERVEDMAGIC       0x6e4c74d5
#define MAXRESERVEDPAGES    300
#define MAXRESERVEDQUEUES    15
static struct reserved_pages {
    struct reserved_pages *next;
    int max_available;      /* 队列深度，0 = 未使用 */
    int npages;             /* 每槽连续页数 */
    int mappedin;           /* 是否要求映射 */
    int n_available;        /* 当前可用槽数 */
    int allocflags;         /* 分配标志 */
    struct reserved_pageslot { phys_bytes phys; void *vir; } slots[MAXRESERVEDPAGES];
    u32_t magic;
} reservedqueues[MAXRESERVEDQUEUES], *first_reserved_inuse = NULL;
```

三个要点：

1. **位图语义是"1 = 空闲"**：`page_isfree(i)` 宏（alloc.c:54）用 `GET_BIT` 判断。初始化时 `memset` 全 0（全已用），再对每个空闲 chunk 调用 `free_mem` 置 1（§2.3）。
2. **单页缓存是 LIFO 加速**：`free_pages` 释放时压栈（上限 10000），`alloc_pages` 单页分配时先弹栈验证（§2.5）——避免频繁扫描位图。
3. **保留队列是"预分配 + 延迟补充"**：为页表、内核 spare 页等关键场景**预先**分配好页并持有（alloc.c:56-72，机制见 §2.9）。它打破"分配失败时无页可用"的递归困境，而不是保证无限供应。

### 2.2 get_mem_chunks：内存清单 → click 取整（utility.c:44-79）

```c
void get_mem_chunks(struct memory *mem_chunks)
{
  memset(mem_chunks, 0, NR_MEMS*sizeof(*mem_chunks));      // utility.c:55
  /* XXX Any memory chunk in excess of NR_MEMS is silently ignored. */
  for(i = 0; i < MIN(MAXMEMMAP, NR_MEMS); i++) {           // utility.c:59
    mem_chunks[i].base = kernel_boot_info.memmap[i].mm_base_addr;
    mem_chunks[i].size = kernel_boot_info.memmap[i].mm_length;
  }
  /* Round physical memory to clicks. Round start up, round end down. */
  for (i = 0; i < NR_MEMS; i++) {                          // utility.c:65
    base = mem_chunks[i].base;
    limit = base + mem_chunks[i].size;
    base = CLICK_CEIL(base);
    limit = CLICK_FLOOR(limit);
    if (limit <= base) { memp->base = memp->size = 0; }
    else { memp->base = base >> CLICK_SHIFT;               // 页号
           memp->size = (limit - base) >> CLICK_SHIFT; }
  }
}
```

关键语义：

- **拷贝上限是 `MIN(MAXMEMMAP=40, NR_MEMS=16)`**（utility.c:59）：`kernel_boot_info.memmap[]` 最多 40 块，但 `mem_chunks[]` 只有 16 槽。`XXX` 注释承认**超出的块被静默丢弃**——这是 32 位时代的遗留上限，64 位下大内存图块数可能超过 16。
- **取整规则**：起点 `CLICK_CEIL`（向上）、终点 `CLICK_FLOOR`（向下）；`limit <= base` 的碎片区间直接置 0（utility.c:72-76）。
- **输出是页号**：`base`/`size` 都右移 12，进入 `struct memory`（`type.h:157-160`：`phys_bytes base; phys_bytes size;`），`mem_init` 直接消费。

### 2.3 mem_init：分配器接管（alloc.c:306-335）

```c
void mem_init(struct memory *chunks)
{
  int i, first = 0;
  total_pages = 0;                              // alloc.c:319
  memset(free_pages_bitmap, 0, sizeof(free_pages_bitmap));  // alloc.c:321
  for (i=NR_MEMS-1; i>=0; i--) {                // alloc.c:324 —— 逆序扫描
    if (chunks[i].size > 0) {
      from = CLICK2ABS(chunks[i].base);
      to = CLICK2ABS(chunks[i].base+chunks[i].size)-1;
      if(first || from < mem_low) mem_low = from;   // 记录低端
      if(first || to > mem_high) mem_high = to;     // 记录高端
      free_mem(chunks[i].base, chunks[i].size);     // 置空闲位
      total_pages += chunks[i].size;                // 累加总页数
      first = 0;
    }
  }
}
```

语义要点：

- **逆序扫描**（alloc.c:324）：从高地址 chunk 到低地址，配合 `free_pages` 的缓存压栈，使缓存里最先出现的是**低地址页**（单页分配优先拿到低地址，利于 DMA 场景）。
- **`mem_low`/`mem_high` 是"已登记内存"的边界**：alloc_mem 的 `lastscan` 初始化为 `mem_high`，对齐裁剪也用它做上界（见 §2.4 的 C 语义说明）。注意 Rust 侧没有移植 `lastscan`（§3.8），`mem_low/mem_high` 只在 sanity 路径使用。
- **`total_pages` 在此清零并累加**（alloc.c:319,331）：分配器的"总页数"从 chunks 推导，之后由 `mem_add_total_pages` 校准（§2.7）。

### 2.4 alloc_mem：对齐预加大 + 失败重试（alloc.c:242-279）

```c
phys_clicks alloc_mem(phys_clicks clicks, u32_t memflags)
{
  phys_clicks mem = NO_MEM, align_clicks = 0;
  if(memflags & PAF_ALIGN64K) { align_clicks = 16; clicks += align_clicks; }   // alloc.c:252-253
  else if(memflags & PAF_ALIGN16K) { align_clicks = 4; clicks += align_clicks; } // alloc.c:255-256

  do {
    mem = alloc_pages(clicks, memflags);                  // alloc.c:261
  } while(mem == NO_MEM && cache_freepages(clicks) > 0);  // alloc.c:262 —— 失败后回收块缓存重试
  if(mem == NO_MEM) return mem;

  if(align_clicks) {                                      // alloc.c:267
    o = mem % align_clicks;
    if(o > 0) { e = align_clicks - o; free_mem(mem, e); mem += e; }  // 仅释放前缀
  }
  return mem;
}
```

语义要点：

1. **对齐 = 预加大 + 裁剪**：先多要 `align_clicks` 页（64KB 对齐 = 16 clicks，16KB 对齐 = 4 clicks），分配成功后若起点未对齐，释放前缀、前移起点。**注意 C 只释放前缀**：当起点已经对齐（`o == 0`）时什么都不释放，`align_clicks` 页的尾部保留在分配器账上（外部不可见但减少空闲量）。Rust 侧回收了这部分（§3.8 语义差异表）。
2. **失败重试依赖 `cache_freepages`**（cache.c:288-320）：分配失败时遍历 VM 块缓存 LRU、`rmcache` + `free_mem` 回收物理页后重试。这是"内存耗尽时块缓存可续命"机制（Rust 侧 DEFERRED，归 24-page-cache）。
3. **返回值是页号**：`NO_MEM` 定义在 `vm.h:62`（`((phys_clicks) MAP_NONE)`）。

### 2.5 alloc_pages / findbit：位图扫描核心（alloc.c:404-460 / 369-399）

`alloc_pages`（static，alloc.c:404-460）：

```c
static phys_bytes alloc_pages(int pages, int memflags)
{
  phys_bytes boundary16 = 16 * 1024 * 1024 / VM_PAGE_SIZE;   // alloc.c:406
  phys_bytes boundary1  =  1 * 1024 * 1024 / VM_PAGE_SIZE;   // alloc.c:407
  int maxpage = NUMBER_PHYSICAL_PAGES - 1;
  static int lastscan = -1;                                  // alloc.c:410 —— 分配位置提示

  if(memflags & PAF_LOWER16MB) maxpage = boundary16 - 1;     // alloc.c:414
  else if(memflags & PAF_LOWER1MB) maxpage = boundary1 - 1;  // alloc.c:416
  else if(pages == 1) {                                      // alloc.c:418 —— 单页走缓存
    while(free_page_cache_size > 0) {
      i = free_page_cache[free_page_cache_size-1];
      if(page_isfree(i)) { free_page_cache_size--; mem = i; run_length = 1; break; }
      free_page_cache_size--;                                // 失效条目也弹出
    }
  }

  if(lastscan < maxpage && lastscan >= 0) startscan = lastscan;  // alloc.c:434
  else startscan = maxpage;
  if(mem == NO_MEM) mem = findbit(0, startscan, pages, memflags, &run_length);  // alloc.c:438
  if(mem == NO_MEM) mem = findbit(0, maxpage, pages, memflags, &run_length);    // alloc.c:440 双扫描
  if(mem == NO_MEM) return NO_MEM;
  lastscan = mem;                                            // alloc.c:446
  for(i = mem; i < mem + pages; i++) UNSET_BIT(free_pages_bitmap, i);  // alloc.c:448-450
  if(memflags & PAF_CLEAR)                                   // alloc.c:452
    sys_memset(NONE, 0, CLICK_SIZE*mem, VM_PAGE_SIZE*pages); // alloc.c:454 —— 委托内核清零
  return mem;
}
```

`findbit`（static，alloc.c:369-399）从 `startscan` **向低地址**扫描：

- 遇空闲页则累积 `run_length`，凑满 `pages` 即返回块起点（首次适配，alloc.c:388-395）；
- 遇已用页则重置计数，并做 **chunk-skip**：若整个 32 位 chunk 全为 0（全已用），直接跳到前一个有 1 位的 chunk 末尾（alloc.c:376-385），避免逐位检查；
- **双扫描**（alloc.c:438-440）：先从 `lastscan` 位置扫（顺序局部性），失败再从 `maxpage` 扫（保证兜底）。

`lastscan` 是**分配位置提示（性能优化）**，不是正确性依赖——Rust 侧未移植它（每次从 `maxpage` 起扫，语义等价、性能略差，§3.8 诚实标注）。

### 2.6 free_mem / free_pages：释放与缓存回填（alloc.c:289-301 / 465-481）

```c
void free_mem(phys_clicks base, phys_clicks clicks)  // alloc.c:289
{
  if (clicks == 0) return;                           // alloc.c:296 早退
  assert(CLICK_SIZE == VM_PAGE_SIZE);                // alloc.c:298
  free_pages(base, clicks);
}

static void free_pages(phys_bytes pageno, int npages)  // alloc.c:465
{
  for(i = pageno; i <= pageno + npages - 1; i++) {
    SET_BIT(free_pages_bitmap, i);                   // alloc.c:476
    if(free_page_cache_size < PAGE_CACHE_MAX)
      free_page_cache[free_page_cache_size++] = i;   // alloc.c:477-478 压栈
  }
}
```

语义要点：`clicks == 0` 早退（幂等释放）；每页置空闲位 + 压入单页缓存（缓存满则只置位不入缓存）。`JUNKFREE` 宏（alloc.c:469-473，默认关闭）可在释放时用 `0xa5a5a5a5` 填充内存用于调试。

### 2.7 mem_add_total_pages：总页数校准（alloc.c:281-284 + main.c:485-495）

```c
void mem_add_total_pages(int pages) { total_pages += pages; }   // alloc.c:281-284
```

调用点在 `init_vm()`（main.c:485-495）：

```c
/* The kernel's freelist does not include boot-time modules; let
 * the allocator know that the total memory is bigger. */
for (mod = &kernel_boot_info.module_list[0];
     mod < &kernel_boot_info.module_list[kernel_boot_info.mods_with_kernel-1]; mod++) {
  len = roundup(mod->mod_end-mod->mod_start+1, VM_PAGE_SIZE);
  mem_add_total_pages(len/VM_PAGE_SIZE);            // main.c:489
}
kern_dyn = kernel_boot_info.kernel_allocated_bytes_dynamic;   // main.c:492
kern_static = kernel_boot_info.kernel_allocated_bytes;        // main.c:493
kern_static = roundup(kern_static, VM_PAGE_SIZE);
mem_add_total_pages((kern_dyn + kern_static)/VM_PAGE_SIZE);   // main.c:495
```

语义要点：

- **boot 模块循环的上界是 `mods_with_kernel-1`**（main.c:486）：最后一个模块是 VM 自身的 ELF，被**刻意排除**（VM 的二进制已在 `mem_init` 之前被映射，不属于"需要记账的模块"）。
- **kernel 占用分两笔**：`kernel_allocated_bytes`（静态，向上取整到页）+ `kernel_allocated_bytes_dynamic`（动态，kernel 侧已取整）。
- **为什么需要校准**：`memmap[]` 里没有 kernel 和模块占用的内存，`total_pages` 若只算 chunks 会小于真实物理内存。校准让"总量"反映真实内存，供资源统计（`VM_GETRUSAGE` 等）使用。

### 2.8 memstats / printmemstats：空闲量统计（alloc.c:348-367 / 486-493）

```c
void memstats(int *nodes, int *pages, int *largest)  // alloc.c:348
{
  *nodes = *pages = *largest = 0;
  for(i = 0; i < NUMBER_PHYSICAL_PAGES; i++) {
    int size = 0;
    while(i < NUMBER_PHYSICAL_PAGES && page_isfree(i)) { size++; i++; }  // 连续空闲计数
    if(size == 0) continue;
    (*nodes)++;            // 空闲块数
    (*pages) += size;      // 空闲总页数
    if(size > *largest) *largest = size;   // 最大连续块
  }
}

void printmemstats(void)   // alloc.c:486 —— 诊断打印
{
  memstats(&nodes, &pages, &largest);
  printf("%d blocks, %d pages (%lukB) free, largest %d pages (%lukB)\n", ...);
}
```

语义要点：`nodes` = 空闲块数（连续空闲段数），`pages` = 空闲总页数，`largest` = 最大连续块——"碎片化程度"的三种观察。`printmemstats` 只是格式化打印，数据面与 `memstats` 相同。Rust 侧用 `PhysMemStats` 结构体承载同一数据（§3.6），`printmemstats` 无直接对应（诊断打印，数据面经 `VM_INFO` 覆盖）。

### 2.9 保留队列：预分配 + 延迟补充（alloc.c:56-237）

保留队列（`reservedqueue_*`）解决一个递归困境：**页表建立需要页，而页表自身的页也要从分配器拿**——若分配器已耗尽，VM 连"建立回收页表的页表"都做不到。解法是启动时预先分配并持有一些页（spare pages），需要时直接取出：

| 函数 | 位置 | 语义 |
|------|------|------|
| `reservedqueue_new(max_available, npages, mapped, allocflags)` | alloc.c:100-133 | 登记一个新队列，`missing_spares += max_available`（alloc.c:131） |
| `reservedqueue_fillslot`（static） | alloc.c:137-146 | 填一个槽，`missing_spares--`、`n_available++` |
| `reservedqueue_addslot`（static） | alloc.c:149-177 | `alloc_mem` 预分配 + （可选）`vm_mappages` 映射后填槽 |
| `reservedqueue_add` | alloc.c:179-189 | 外部（pt_init 等）直接提供已分配的页填槽 |
| `reservedqueue_fill`（static） | alloc.c:191-203 | 循环 `addslot` 补满队列 |
| `reservedqueue_alloc` | alloc.c:206-226 | 取出一个槽（`n_available--`、`missing_spares++`） |
| `alloc_cycle` | alloc.c:227-237 | 遍历 in-use 队列补满（`missing_spares > 0` 时），**主循环调用方** |

记账不变量：`missing_spares` = Σ(每个队列 `max_available - n_available`)（alloc.c:76-89 `sanitycheck_queues`）。`main.c:118` 和 `main.c:745` 两处 `if(missing_spares > 0) alloc_cycle()` 是消费方（主循环 + 信号接收卡住时的兜底）。

**边界声明**：本文档只覆盖 alloc.c 侧的队列机制与记账（数据结构、API 语义、`missing_spares` 不变量）。**Rust 侧的保留页池（`critical_pool.rs`）与 `alloc_cycle` 完整补充逻辑归 06-page-allocator**——06 文档负责"spare 页池如何初始化、何时补充"的完整语义。

### 2.10 usedpages_* / mem_sanitycheck：双分配检测（alloc.c:338-346 / 501-545）

`SANITYCHECKS` 编译宏（默认关闭）下：

- `pagemap[NUMBER_PHYSICAL_PAGES]`（alloc.c:47-51）记录每页被哪个 `file:line` 占用；
- `usedpages_reset()`（alloc.c:501-507）清零全表；
- `usedpages_add_f()`（alloc.c:509-545）在每次分配时登记，发现**同一页被二次使用**（`pagemap[pagestart].used` 已置位）则告警 + 打印栈 + 返回 `EFAULT`；
- `mem_sanitycheck()`（alloc.c:338-346）从位图反推：对每个空闲页调 `usedpages_add`，与 `pb.c` 的已用页表对照，验证位图与页状态一致。

这是编译期开启的**一致性审计机制**（不是运行时路径）。Rust 以 `cfg(test)` + `debug_assert` 双分配检测替代（A-7，§3.8 语义差异表）。

### 2.11 调用点全景：init_vm 链（main.c:428-520）

```
main.c:442  sys_getkinfo(&kernel_boot_info)      ← 内存清单来源
main.c:455  get_mem_chunks(mem_chunks)           ← 05 §2.2
main.c:465  acl_init()                           ← 04
main.c:468  map_region_init()                    ← 13
main.c:471  mem_init(mem_chunks)                 ← 05 §2.3
main.c:474  init_proc(VM_PROC_NR)                ← 02/03
main.c:475  pt_init()                            ← 07/08
main.c:480  __minix_init()                       ← 09
main.c:485-495  mem_add_total_pages()            ← 05 §2.7（模块 + kernel 占用）
main.c:498-520  boot 进程 exec_bootproc + free_mem ← 01/06
```

注意 boot 进程循环里还有一处 `free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len))`（main.c:519）：每个 boot 进程的 ELF 二进制在 `exec_bootproc` 完成后被释放回分配器——这是 `free_mem` 在启动期的真实消费方（Rust 侧 exec 流程 DEFERRED，归 01/06）。

---

## 3. Rust 设计决策

### 3.1 D1: `PhysAllocator` trait + `PhysAlloc` 枚举——策略模式静态分发（`[ARCH: A-5]`）

- **C**: 单一位图实现，`alloc_mem/free_mem` 直接操作全局 `free_pages_bitmap`（alloc.c:35）。
- **Rust**: `trait PhysAllocator`（`alloc_trait.rs:34-44`：`alloc_mem/free_mem/total_count/reserve_pages/available_regions`）+ `enum PhysAlloc`（`mod.rs:127`，三变体 `Bitmap/Buddy/SegmentTree`，变体按 feature 门控）。Cargo feature 启动期决定编译进哪个后端，`PhysAlloc` 是唯一静态分发点——**无 `dyn` 开销**。
- **为什么**：分配器是 VM 最热路径之一，vtable 间接不可接受；trait 使消费方（`alloc_page.rs` 的 `VmPageAllocator`）与后端解耦，跨后端 parity 由 `allocator_tests.rs` 验证（§5.1）。
- **行为契约**：三后端对同一请求序列产生可观察等价结果（对齐语义差异见 §3.8）；`PhysAlloc` 把 trait 方法 `match` 到具体后端。

### 3.2 D2: 元数据放置——预映射内存 + `BumpBuf`

- **C**: 位图是静态 BSS 数组，大小按 32 位地址空间上限固定（128KB），"先于分配器存在"。
- **Rust**: 64 位物理内存无硬上限 → 元数据大小随 `total_pages` 增长（`PhysAllocType::metadata_size`，`mod.rs:249`）。自举期 GlobalAlloc 依赖分配器（循环依赖），故从 Direct Map 预映射区切一段**连续物理内存**做元数据（`vm_server.rs:create_default_allocator`，L230-285：找 `base < VM_DIRECT_MAP_SIZE && size >= meta_pages*CLICK` 的 free region，元数据 PA 从分配器视野中扣除——等价于 C 的"分配器占用不进入空闲池"）。`VM_DIRECT_MAP_SIZE` 的来源是 `DirectMapArch::VM_DIRECT_MAP_SIZE`（`os/arch/src/arch/direct_map.rs`，按架构窗口给出：x86_64 1 GiB、aarch64 2 GiB、riscv64 16 GiB——窗口容量必须覆盖目标平台的 RAM base，QEMU virt 的 arm64 RAM base 为 1 GiB，1 GiB 窗口会使全部 RAM 落在 DM 可表达范围之外，故 aarch64 取 2 GiB；窗口容量与资格过滤的推导见 07-pagetable-struct.md §3.4），VM 侧不再硬编码。`BumpBuf`（`mod.rs:49`）把裸字节按对齐切出 `&'static mut [T]` slice。
- **生命周期**：元数据 slice 的 `'static` 生命周期由"VM 进程存活期"保证（SAFETY 注释见 `mod.rs:81-85`）；`metadata_pa_range()`（`bitmap_alloc.rs:110-112`）暴露元数据 PA 范围，供 `relocate` 搬迁后 `free_mem` 回收（归 10-vm-relocation）。
- **行为契约**：`adjusted_regions`（`vm_server.rs:266-268`）扣除元数据页后作为初始空闲区间；`validate()`（`boot.rs:152`）断言页对齐。

### 3.3 D3: 三后端权衡——bitmap / buddy / segment-tree

| 后端 | 分配 | 元数据/页 | 选择条件 |
|------|------|----------|---------|
| bitmap（默认） | O(n) 扫描 + 单页缓存 O(1) | 1 bit/页 + 10000×usize 缓存（64 位下 8B/槽） | 默认 feature |
| buddy | O(log n) 拆分/合并 | 5B/页（4B next + 1B order）+ (max_order+1)×4B 头部 | `buddy_alloc` feature 且 `total_pages > BUDDY_THRESHOLD_PAGES`（`mod.rs:288`，1M 页 ≈ 4GB） |
| segment-tree | O(log n) 任意连续度 | 2×2^k×32B（最贵） | `segment_tree_alloc` feature 直接选中（V10-P0-1 修复前永远选不上，见 §3.3 注） |

**与 Redox / 主流 OS 的对比**（用户要求）：

- **Redox RMM**：boot 期 `BumpAllocator` → 主分配器 `BuddyAllocator`（11 orders），`FrameAllocator` trait 抽象、`RecycleAllocator` 做回收复用——是"先导分配器 → 主分配器"的固定演进路径。
- **Linux**：buddy（页粒度，`__alloc_pages`）+ slab（小对象）双层，boot 期 bootmem/memblock 先导。
- **minix-rs 的选择**：三后端 trait 可选、按内存规模选择，而不是固定演进。理由：VM 是用户态服务器，内存规模跨度大（256MB-2TB+），位图的内存效率（1 bit/页）在中小内存下最优，buddy 的 O(log n) 延迟在大内存下才值得；segment-tree 保留 O(log n) 任意连续度能力作为实验后端。这与"单进程内分配器策略可配置"的定位一致——分配策略在**用户态服务器层**决定（微内核分工），而不是像 Linux 一样固化在内核。

> **选择路径（V10-P0-1 修复，2026-08-17；V11-P1-3 澄清组合语义，2026-09-06）**：`choose_allocator_type`（`vm_server.rs:294-309`）在 `buddy_alloc` feature 下保留自适应阈值（`total_pages > BUDDY_THRESHOLD_PAGES` 才切 buddy，否则回落 bitmap），在 `segment_tree_alloc` feature 下直接选中 `SegmentTree`——**两个 feature 同时开启时 segment-tree 优先**（该函数是后端选择的唯一权威；`phys_mem/mod.rs` 里声称 "buddy > segment-tree" 的 `DefaultAllocator` 死别名已删除，组合语义由三个 feature 组合测试逐一定格）；`relocate()`（`vm_server.rs:311-376` 附近）按 feature 分别构造 `PhysAlloc` 分支，未启用 feature 的分支回落 Bitmap（bootstrap 分配器）。三个 feature 组合此前**根本无法构建**（缺 import / `_total_pages` 引错 / no_std 下 `eprintln!`），已修复并纳入构建矩阵验证。

### 3.4 D4: 类型化错误与标志——消除魔法值

- **C**: `NO_MEM`（vm.h:62）是裸 `phys_clicks` 哨兵；`memflags` 是裸 `u32_t`；低端分配失败与普通失败**不可区分**。
- **Rust**:
  - `AlignedPhysBytes(u64)` newtype（`types.rs:24-78`）：`new()` 断言页对齐，`new_unchecked()` 只 debug_assert——**对齐不变量由类型保证**；
  - `PageAllocFlags` bitflags（`types.rs:80-90`），值面与 C `PAF_*` 一致；
  - `AllocError { OutOfMemory, LowMemoryExhausted }`（`types.rs:98-102`）：保留"低端耗尽"与"普通耗尽"的区分用于遥测；两者在 `VmError::to_errno()` 均折叠为 `ENOMEM`（`os/libs/minix-types/src/ipc/vm.rs:601`）——对外 errno 语义与 C 一致。
- **行为契约**：`LOWER16MB/LOWER1MB` 失败 → `LowMemoryExhausted`（`mod.rs:336-344 oom_error`）；其余失败 → `OutOfMemory`。

### 3.5 D5: boot 契约显式化——`BootParams` / `validate` / `extra_pages`

- **C**: `kernel_boot_info`（glo.h）是隐藏全局；`init_vm()` 的断言与调用点散落在 main.c:442-495。
- **Rust**: `BootParams`（`boot.rs:61`）把 kernel→VM 交接建模为**构造输入**，生产构造经 boot handoff 页读取（`read_boot_params`，`boot.rs:242`）：`root_paddr` 是 A1 地址空间身份交接（VM 初始页表 = bootstrap root，不新建不拷贝）；`free_regions`/`deducted` 是 kernel 侧 **A2 post-bootstrap classification** 的产物——`VM PMM eligible = conventional ∩ DM-representable − LiveBootstrap`（kernel 在分类时点按 LiveBootstrap 记录从全量 memmap 扣除，对账契约见下文 `reconcile`）。C 的 `mem_chunks[]` 对应的是"分类后的幸存区间"而非全量内存图。`validate()`（`boot.rs:132`）镜像 main.c:451-452 断言（`mmap_size > 0` → 分类后 `free_regions` 非空），另断言 root PA 非零且页对齐（A1 契约）与 `total_pages == Σ region pages`（C 无此断言，Rust 把 mem_init 的累加不变量前移到构造期）；`extra_pages()`（`boot.rs:197`）精确复刻 `mem_add_total_pages` 调用点（main.c:485-495：模块循环排除最后一个 + kernel static 向上取整 + dynamic 原样）。
- **为什么**：boot 协议是一次性 one-shot 交接，把依赖显式化在构造函数使启动链可审计、可单测（`boot.rs` 12 个测试：validate 5 + extra_pages 2 + reconcile 5，§5.1）。kernel 写侧与 VM 读侧各自带对账：kernel 随 free 清单移交扣除记录 `deducted`，VM 的 `reconcile`（`boot.rs:322`）用**独立可枚举的事实**复核记录——adopted root 页、保留模块 blob、kernel 动态分配都必须落在记录内，free 与 deducted 不得相交。交接面从"信任一个隐藏全局"变为"验证一份记录"。
- **行为契约**：`validate()`/`reconcile()` 失败即 panic（fail-fast，与 C `assert` 同构）；`extra_pages()` 对 modules 最后一项（VM 自身）用 `saturating_sub(1)` 排除；总页数 = `global::init(total_pages)`（`vm_server.rs:425`）+ `account_boot_memory()`（`vm_server.rs:509`）追加 `extra_pages()`。

**A2 清单的定格语义：linearizable，而非 atomic**。设计把"一页退出 bootstrap 记账、进入 VM free set"定义为一个对分配路径**不可观察的单一状态转移**，即"合法回收（reclaim）相对 VM PMM 分配操作 linearizable"。当前系统里三类 LiveBootstrap 成员（self 页表层级、ELF backing、用户栈帧）都没有回收路径，这个契约是防御性的：它约束的是**未来可能出现**的回收机制。为什么术语用 linearizable 而不是 atomic——linearizable 是**可观察性契约**，不预设实现机制：VM 是单线程事件循环，顺序代码里"先退出记账、后进入 free"两步连续提交即构成单一转移，不需要 CAS/锁/原子 CPU 指令；说 atomic 会让读者误以为必须用原子指令实现。契约锚定的是"中间态不得暴露给分配路径"这个性质本身，未来回收改为批量/多阶段时依然适用。

**capacity validation 验证的是"最低运行需求"，不是"最低自举需求"**。`validate()` 断言分类后的资源非空、足以支撑 VM 正常运行（分配器元数据 + 最低堆页数等）。为什么判据是运行需求而非自举需求——这是防时序错位：bootstrap（页表层级、ELF、栈）在 eligible 清单建成**之前**就已完成，且已从清单中扣除；eligible 承载的是"排除后"的运行资源，自举不再向它索要任何东西。若把 validation 语义错置为"够不够自举"，等于在时序上追问一个已经过去的问题（自举成败在分类前已成定局），而真正的检查对象是 VM 进入服务循环后的 PMM 容量下限。

### 3.6 D6: 统计结构体——取代 C 三指针 out-param

- **C**: `memstats(int *nodes, int *pages, int *largest)`（alloc.c:348）三指针输出；`printmemstats` 直接 printf。
- **Rust**: `PhysMemStats { free_nodes, free_pages, largest_free }`（`alloc_trait.rs:28-32`）；`PhysAlloc::memstats()` 统一分发到三后端（bitmap 全量扫描 / buddy `largest_free()` + `free_pages` / segment-tree 同 buddy）；接线 `query.rs:296-305`：`InfoQuery::Stats` → `InfoResult::Stats`。
- **诚实标注**：buddy/segment-tree 的 `free_nodes` 暂为 0（buddy 无现成块计数，`nodes` 仅诊断用途，`VM_INFO` 路径只用 `free_pages`/`largest_free`）；`printmemstats` 无 Rust 直接对应（诊断打印，数据面已被 INFO 查询覆盖）。

### 3.7 D7: `PAF_CLEAR` 机制演进——`sys_memset` IPC → Direct Map 直接清零

- **C**: `alloc_pages` 内 `sys_memset(NONE, 0, ...)`（alloc.c:453-456）——VM 不持有物理页映射，清零必须委托内核 IPC，失败 `panic`。
- **Rust**: `vm_phys_to_virt()`（`direct_map.rs:24`）直映射后 `write_volatile` 循环清零（`bitmap_alloc.rs:412-430`，三后端同构）——**依赖 Direct Map（A-1，07 文档显式章节）**。
- **行为契约**：清零语义等价（分配返回前物理页全零）；去掉一次内核 IPC 往返与 `panic` 失败面。VM 是单线程服务器，`write_volatile` 无并发写竞争（SAFETY 注释见 `bitmap_alloc.rs:414-418`）。

### 3.8 语义差异清单（C ↔ Rust 诚实标注）

| # | C 行为 | Rust 行为 | 类型 |
|---|--------|----------|------|
| S-1 | 对齐 `o==0` 时尾部 `align_clicks` 页保留在账上（浪费） | bitmap/segment-tree 释放尾部；buddy 按 2 的幂块天然无浪费 | 良性改进（free_pages 记账更准，§2.4） |
| S-2 | `lastscan` 静态提示位（顺序局部性） | 未移植，每次从 `maxpage` 起扫 | 性能差异（语义等价），诚实标注 |
| S-3 | `cache_freepages`（cache.c:288）LRU 回收块缓存后重试 | `BitmapAllocator::cache_freepages` 返回 0（`bitmap_alloc.rs:340-348`，DEFERRED） | 语义缺口（OOM 时无续命路径），归 24-page-cache |
| S-4 | `usedpages_*` + `mem_sanitycheck`（SANITYCHECKS 编译宏） | `cfg(test)` + `debug_assert` 双分配检测（如 `free_pages_internal` 的 `debug_assert!(!page_is_free)`，`bitmap_alloc.rs:266`） | cfg 替代（A-7） |
| S-5 | `alloc_cycle` 主循环补满保留队列 | `vm_server.rs:570-581` 只维护 `missing_spares` 计数（`mark_alloc_failure`，L533），清零无补充体 | DEFERRED，归 06 |
| S-6 | `printmemstats` 诊断打印 | 无直接对应 | 数据面经 `query.rs:296-305` 覆盖 |
| S-7 | `mem_low/mem_high` 边界跟踪 | `compute_memory_bounds`（`mod.rs:310-328`）等价计算，仅测试/诊断使用 | 语义等价 |

---

## 4. 实现详解

### 4.1 `phys_mem/mod.rs`：模块根与自举助手

- `PhysAlloc` 枚举（`mod.rs:114`）+ `PhysAllocator for PhysAlloc` 静态分发（`mod.rs:122-161`）；
- `BumpBuf`（`mod.rs:49-94`）：`alloc_slice::<T>()` 按对齐切 `&'static mut [T]`，溢出 `assert`（元数据缓冲耗尽即 bug）；
- `PhysAllocType::metadata_size_exact`（`mod.rs:229-262`）：三后端元数据公式，`metadata_size` 向上取整到页（`mod.rs:219-227`）；
- 常量与助手：`CLICK_SIZE/CLICK_SHIFT`（`mod.rs:284-285`）、`BUDDY_THRESHOLD_PAGES`（`mod.rs:288`）、`bytes_to_clicks/clicks_to_bytes/click_floor/click_ceil`（`mod.rs:291-313`）；
- `BootMemRegion`（`mod.rs:290-308`）：`validate()` 断言 base/size 页对齐（等价 C 的 click 取整后不变量）；
- `compute_memory_bounds`（`mod.rs:310-328`）：计算 `(total_pages, mem_low, mem_high)`，等价 C `mem_init` 的边界跟踪（S-7）；
- `is_low_mem_flag` / `oom_error`（`mod.rs:336-344`）：错误分类（D4）。

### 4.2 `types.rs` / `alloc_trait.rs`：核心类型与契约

- `AlignedPhysBytes`（`types.rs:24-78`）、`PageAllocFlags`（`types.rs:80-90`）、`AllocError`（`types.rs:98-102`）——见 D4；
- `PhysMemStats`（`alloc_trait.rs:28-32`）、`PhysAllocator` trait（`alloc_trait.rs:34-44`）——见 D1/D6。trait 的 `available_regions`（callback 遍历空闲区间）供 `relocate` 搬迁时状态转移（`vm_server.rs:324-330`）与 VFS fd 表设置使用。

### 4.3 `bitmap_alloc.rs`：默认后端（与 C 最接近）

- `init`（`bitmap_alloc.rs:49-82`）：BumpBuf 切位图 + 缓存，对 `free_regions` 逐个 `free_pages_internal` 置空闲位（等价 C `mem_init` 的"清 0 + 标记空闲"）；
- `alloc_pages`（`bitmap_alloc.rs:145-204`）：单页走 page cache（LIFO + 失效条目跳过，等价 C alloc.c:418-429）、`max_page` 边界（LOWER16MB→4096 / LOWER1MB→256，等价 C alloc.c:406-416）、`find_bit` 单次全范围扫描（从 `max_page-1` 扫到 0，`bitmap_alloc.rs:168-171`）——C 的双扫描（lastscan 起点 + maxpage 兜底）在 Rust 因无 lastscan 提示位而合并为一次完整扫描，语义等价；
- `find_bit`（`bitmap_alloc.rs:206-257`）：反向扫描 + chunk-skip（等价 C findbit alloc.c:369-399；Rust 以 u64 chunk 实现，且跳过逻辑为 C 的严格改进）；
- `free_pages_internal`（`bitmap_alloc.rs:259-274`）：置位 + 缓存压栈（上限 10000）+ `debug_assert` 双释放检测（S-4）；
- `alloc_mem`（`bitmap_alloc.rs:349-430`）：对齐预加大 + 失败重试 `cache_freepages`（当前返回 0，S-3）+ 对齐裁剪（S-1）+ `PAF_CLEAR` Direct Map 清零（D7）；
- `free_mem`（`bitmap_alloc.rs:432-439`）：`clicks == 0` 早退（等价 C alloc.c:296）；
- `reserve_pages`（`bitmap_alloc.rs:445-462`）：把一段物理页标记为已用（等价"分配器知道哪些页被谁占用"，供 boot 期预留）；`available_regions`（`bitmap_alloc.rs:464-476`）；`memstats`（`bitmap_alloc.rs:482-489`，等价 C `memstats` 的逐段扫描）。

### 4.4 `buddy_alloc.rs` / `segment_tree_alloc.rs`：可选后端

- **buddy**（`buddy_alloc.rs`）：SoA 三数组 `free_list_heads/page_next/page_orders`；`init`（L52）把 free regions 按 2 的幂拆分插入（`add_free_region`，L123，`pos.trailing_zeros()` 保证对齐）；`alloc_mem`（L309）`order = max(size_order, align_order)` 天然满足对齐，尾部 `block_size - clicks` 释放（无浪费，S-1）；`free_mem`（L372）`try_merge` 合并 buddy（L170）；`memstats`（L431）`largest_free` + `free_pages`，`free_nodes=0`（诚实标注）。
- **segment-tree**（`segment_tree_alloc.rs`，`segment_tree_alloc` feature 门控）：每节点 `(max_free,left_free,right_free,len)`；`alloc_mem`（L242）`find_first_fit`（L190）O(log n) 找最大适配连续块；**局限**：LOWER16MB/LOWER1MB 是先分配后检查（`mem + alloc_clicks > max_page` 则 `LowMemoryExhausted`，L274-276），不是 C 的"限制搜索范围"——实验性后端；V10-P0-1 起 `segment_tree_alloc` feature 会真正选中它（§3.3 注），默认构建仍走 bitmap。

### 4.5 `boot.rs` / `global.rs` / `vm_server.rs`：boot 契约与调用点

- `boot.rs`：`BootModule`（L34）、`KernelAllocated`（L46）、`BootParams`（L61）、`validate`（L144）、`extra_pages`（L201）——见 D5；
- `global.rs`：`TOTAL_PAGES`（L17，`AssumeSyncCell` 单线程模型）、`init`（L43）、`add_total_pages`（L68，`mem_add_total_pages` 等价）；
- `vm_server.rs`：`create_default_allocator`（L230-285，D2 元数据切分）、`choose_allocator_type`（L286-299，D3 选后端，V10-P0-1 修复）、`relocate`（L300-369，归 10，元数据从 Direct Map 迁到 HeapArena）、`account_boot_memory`（L505-516，`extra_pages` 接线）、`mark_alloc_failure`/`missing_spares`（L533-538，C `missing_spares++`）、主循环 `missing_spares > 0` 清零（L570-581，S-5 DEFERRED 体）。

### 4.6 `alloc_page.rs` / `query.rs`：消费方与查询接线

- `alloc_page.rs`：`VmPageAllocator`（L17）包装 `PhysAlloc` + `VmAllocStats` 记账；`alloc_phys/alloc_page/alloc_pages/free_page/free_pages`（L30-59）——**06 文档的消费方边界**；
- `query.rs:296-305`：`InfoQuery::Stats` → `PhysAlloc::memstats()` → `InfoResult::Stats`（page_size/total_pages/free_pages/largest_contiguous），对应 C `do_info` 的物理内存统计面（26-vm-queries 的接线面）。

---

## 5. 测试要点

### 5.1 单元测试清单（`os/servers/vm/src/`）

| 文件 | 测试数 | 验证目标 |
|------|--------|---------|
| `phys_mem/bitmap_alloc.rs` | 24 | alloc/free 基本路径、零页、耗尽、memstats、多区间、内存压力、page cache（单页/低端禁用/失效条目跳过）、low-mem 错误、OOM 分类、reserve_pages、available_regions、metadata_pa_range、find_bit 回归（1 页/2 页 run/起始点跳过） |
| `phys_mem/buddy_alloc.rs` | 14 | alloc/free、零页、耗尽、buddy 合并、largest_free、内部碎片、多分配、多区间、单页、合并链、low-mem 错误、OOM 分类、ALIGN64K/ALIGN16K 无泄漏 |
| `phys_mem/allocator_tests.rs` | 42 | 跨后端 parity（三后端同一请求序列可观察等价；segment-tree 相关用例以 `segment_tree_alloc` feature 门控） |
| `phys_mem/mod.rs` | 4 | `metadata_size` 公式（bitmap/buddy/页对齐/0 页边界） |
| `phys_mem/stats.rs` | 3 | `MemStats` 记账（基本/峰值/失败） |
| `alloc_stats.rs` | 4 | `VmAllocStats` 记账（基本/泄漏检测/失败跟踪/压力） |
| `boot.rs` | 12 | `BootParams::validate`（通过/空区间 panic/总数不匹配 panic/root 未对齐 panic/root 为零 panic）、`extra_pages`（模块排除最后一项/kernel 取整）、`reconcile` 对账（一致记录接受/空记录拒绝/root 不在记录拒绝/free∩deducted 相交拒绝/模块不在记录拒绝） |
| `global.rs` | 5 | `TOTAL_PAGES` 读写、`add_total_pages`、kernel layout、VM instance 计数 |

合计 108 个测试函数（`segment_tree_alloc` feature 关闭时部分 parity 用例不编译）。

### 5.2 覆盖维度

- **C 行为契约**：alloc/free 基本路径、零页早退、耗尽错误分类、对齐（64K/16K）无泄漏、低端限制、page cache 语义、find_bit 反向扫描；
- **自举路径**：`metadata_size` 公式（含 0 页边界）、`create_default_allocator` 元数据切分（`vm_server.rs` 分配器构造测试）、`BootParams::validate` 一致性断言；
- **记账**：`MemStats`/`VmAllocStats`/`PhysMemStats` 三层的分配-释放对称性。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 |
|------|------|
| `cache_freepages` 块缓存回收重试（C cache.c:288 LRU） | DEFERRED，归 24-page-cache（§3.8 S-3） |
| `lastscan` 分配位置提示 | 未移植（性能差异，非语义，§3.8 S-2） |
| `usedpages_*`/`mem_sanitycheck` 双分配检测 | `cfg`/`debug_assert` 替代（A-7，§3.8 S-4） |
| `alloc_cycle` 保留队列补充体 | DEFERRED，归 06-page-allocator（§3.8 S-5） |
| `printmemstats` 诊断打印 | 无直接对应（数据面经 `query.rs:296-305` 覆盖） |
| buddy/segment-tree `free_nodes` | 恒 0（诊断字段，`VM_INFO` 路径不使用） |
| segment-tree LOWER16MB/1MB 先分配后检查 | 实验性后端局限（§4.4） |
| `PAF_CLEAR` 端到端（真实 Direct Map 清零） | 依赖 Direct Map（07-pagetable-struct） |

### 5.4 测试统计（截至 2026-09-04）

- `cargo test -p minix-vm --lib`：**448 passed / 0 failed**——此前基线（346 passed / 3 failed）中的 3 个 pre-existing 失败已随 06/13 的推进清零
- 本文档相关模块（§5.1 八文件）在默认 feature 构建下全部通过

---

## 6. 过渡

本文档在启动时序中的位置：`init_vm()` 的 `get_mem_chunks → mem_init → mem_add_total_pages` 链（main.c:455/471/489/495），是 04（ACL）之后、06（页分配）之前的物理侧基石——ACL 决定"谁可以调服务"，本文档决定"物理内存由谁持有"。

**下一篇入口（06-page-allocator）**：`mem_init` 就绪后，06 把"任意大小连续块"升级为"VM 自身页分配"——`vm_allocpage/vm_allocpages/vm_mappages` 从本文档的分配器借页并做 Direct Map 映射，同时覆盖保留页池的完整消费语义（`reservedqueue_*` 的 Rust 侧落地、`alloc_cycle` 补充体，本文档 §2.9 的 DEFERRED 项）。页表结构（07）则消费 06 的页分配结果建立 Direct Map 双视图。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/00-vm-overview.md` — 启动主线图与文档导航
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` — `init_vm` 调用点、`mem_add_total_pages` 调用上下文
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-page-allocator.md` — `vm_allocpage` 页分配、保留页池消费、`alloc_cycle` 补充体
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md` — Direct Map（A-1）、`PAF_CLEAR` 清零的映射前提
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/10-vm-relocation.md` — 元数据搬迁 `relocate`、静态 → 动态分配转换
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/24-page-cache.md` — `cache_freepages` 块缓存回收（DEFERRED 项）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/26-vm-queries.md` — `VM_INFO` 查询（`query.rs` 接线面）
- `minix3/minix/servers/vm/alloc.c`、`minix3/minix/servers/vm/utility.c` — C 源码（ground truth）
- `os/servers/vm/src/phys_mem/`、`os/servers/vm/src/boot.rs`、`os/servers/vm/src/global.rs` — Rust 实现
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/04-physical-memory.md` — 旧主线素材（素材，非正式引用源）
