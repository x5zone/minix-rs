# 25-page-cache: 页缓存数据结构与缓存请求处理

> **分类**: VM私有
> **源码**: `minix3/minix/servers/vm/cache.c`, `cache.h`, `mem_cache.c`
> **Rust 对应**: `os/servers/vm/src/page_cache.rs`, `os/servers/vm/src/ipc/dispatcher.rs`
> **说明**: 页缓存的双哈希索引、LRU 淘汰、缓存 IPC 请求处理（mapcache/setcache/forgetcache/clearcache）
> **前置**: [12-memtype.md](12-memtype.md)（CacheMemory memtype 定义）、[24-vm-ipc-dispatch.md](24-vm-ipc-dispatch.md)（IPC 分发框架）

---

## 1. 概述

### 1.1 页缓存的作用

页缓存（Page Cache）是 VM 管理的磁盘块缓存，用于加速文件系统 I/O。当文件系统需要读写磁盘块时，先在页缓存中查找；命中则直接使用，未命中则从磁盘读取并加入缓存。

页缓存与 MemType 系统的关系：`CacheMemory` memtype（[12-memtype.md](12-memtype.md) §4.6）负责缓存区域的缺页行为——将预加载的缓存物理块链接到 `PhysRegion`。而**本文档**聚焦缓存数据的索引、查找、淘汰和 IPC 请求处理。

### 1.2 两个视角

| 视角 | 负责模块 | 关注点 |
|------|---------|--------|
| **MemType 视角** | `CacheMemory`（12-memtype.md） | 缺页时如何链接缓存块、CoW 行为、resize 限制 |
| **数据结构视角** | `PageCache`（本文档） | 缓存块如何索引、查找、淘汰、IPC 请求如何操作缓存 |

### 1.3 缓存操作的完整生命周期

```
文件系统分配匿名内存 → do_setcache → 匿名内存转为缓存类型 + 加入 PageCache 索引
文件系统需要缓存   → do_mapcache → 查找 PageCache → 映射到文件系统地址空间
缓存失效           → do_forgetcache → 从 PageCache 移除指定范围
设备卸载           → do_clearcache → 从 PageCache 移除该设备的所有缓存
内存不足           → cache_freepages → LRU 淘汰 refcount==1 的缓存页
```

---

## 2. C 源码分析：cache.c — 页缓存数据结构

### 2.1 cached_page 结构

**源码位置**: [`cache.h`](../../../minix3/minix/servers/vm/cache.h)

```c
struct cached_page {
    dev_t dev;                      /* 设备号（必须有效，不能是 NO_DEV） */
    u64_t dev_offset;               /* 设备内偏移 */
    ino_t ino;                      /* inode 号（可能未知 = VMC_NO_INODE） */
    u64_t ino_offset;               /* inode 内偏移 */
    int flags;                      /* VMSF_ONCE 或 0 */
    struct phys_block *page;        /* 指向物理块 */
    struct cached_page *older;      /* LRU 链表：更老 */
    struct cached_page *newer;      /* LRU 链表：更新 */
    struct cached_page *hash_next_dev;  /* 设备哈希链 */
    struct cached_page *hash_next_ino;  /* inode 哈希链 */
};
```

**字段语义**：

| 字段 | 语义 | 约束 |
|------|------|------|
| `dev` + `dev_offset` | **主键**：唯一标识一个缓存块 | `dev != NO_DEV`，`(dev, dev_offset)` 全局唯一 |
| `ino` + `ino_offset` | **辅助索引**：inode 信息，可能缺失 | `ino == VMC_NO_INODE` 表示未知 |
| `flags` | `VMSF_ONCE`：一次性缓存，映射后立即失效 | `do_mapcache` 遇到 `VMSF_ONCE` 返回 `ENOENT` |
| `page` | 指向 `phys_block`，缓存引用使 `refcount++` | 移除时 `refcount--` |
| `older`/`newer` | LRU 双向链表指针 | `lru_newest->newer == NULL`，`lru_oldest->older == NULL` |
| `hash_next_dev`/`hash_next_ino` | 开链法哈希链 | 分别属于 `cache_hash_bydev` 和 `cache_hash_byino` |

### 2.2 双哈希表

**源码位置**: [`cache.c:21-24`](../../../minix3/minix/servers/vm/cache.c#L21)

```c
#define HASHSIZE 65536

static struct cached_page *cache_hash_bydev[HASHSIZE];  /* 按 (dev, dev_offset) */
static struct cached_page *cache_hash_byino[HASHSIZE];  /* 按 (ino, ino_offset) */
```

**为什么需要双哈希表？**

Minix3 的缓存块有两种查找路径：
- **按设备查找**（`find_cached_page_bydev`）：文件系统通过 `(dev, dev_offset)` 查找缓存块。这是**主查找路径**，所有缓存操作都使用。
- **按 inode 查找**（`find_cached_page_byino`）：`MappedFile` memtype 通过 `(dev, ino, ino_offset)` 查找缓存块（[12-memtype.md](12-memtype.md) §2.2.5 `mappedfile_pagefault`）。这是**辅助路径**，仅文件映射使用。

**哈希函数**（[`cache.c:76-83`](../../../minix3/minix/servers/vm/cache.c#L76)）：

```c
static __inline u32_t makehash(u32_t p1, u64_t p2)
{
    u32_t offlo = ex64lo(p2), offhi = ex64hi(p2), v = 0x12345678;
    hash_mix(p1, offlo, offhi);
    hash_final(offlo, offhi, v);
    return v % HASHSIZE;
}
```

使用 Minix3 的 `hash_mix`/`hash_final` 宏计算哈希值，输入为两个键（`dev`+`dev_offset` 或 `ino`+`ino_offset`）。

### 2.3 LRU 淘汰机制

**源码位置**: [`cache.c:29-71`](../../../minix3/minix/servers/vm/cache.c#L29)

```c
static struct cached_page *lru_oldest = NULL, *lru_newest = NULL;
static u32_t cached_pages = 0;
```

**核心操作**：

| 操作 | 函数 | 行为 |
|------|------|------|
| 添加 | `lru_add(hb)` | 插入 `lru_newest` 端，`cached_pages++` |
| 移除 | `lru_rm(hb)` | 从双向链表中摘除，`cached_pages--` |
| 访问 | `cache_lru_touch(hb)` | `lru_rm` + `lru_add`（移到最新端） |

**淘汰条件**（[`cache.c:288-305`](../../../minix3/minix/servers/vm/cache.c#L288)）：

```c
int cache_freepages(int pages)
{
    struct cached_page *cp, *newercp;
    int freed = 0;

    for(cp = lru_oldest; cp && freed < pages; cp = newercp) {
        newercp = cp->newer;
        if(cp->page->refcount == 1) {  /* 只被缓存引用，无人映射 */
            rmcache(cp);
            freed++;
        }
    }
    return freed;
}
```

**`refcount == 1` 的含义**：缓存索引本身持有一个引用（`addcache` 时 `refcount++`），如果 `refcount == 1` 说明没有进程映射此页，可以安全回收。

### 2.4 addcache / rmcache

**addcache**（[`cache.c:232-262`](../../../minix3/minix/servers/vm/cache.c#L232)）：

```c
int addcache(dev_t dev, u64_t dev_off, ino_t ino, u64_t ino_off,
    int flags, struct phys_block *pb)
{
    if(pb->flags & PBF_INCACHE) return EINVAL;  /* 防止重复添加 */

    hb = SLABALLOC(hb);  /* 从 slab 分配 cached_page */

    hb->dev = dev;
    hb->dev_offset = dev_off;
    hb->ino = ino;
    hb->ino_offset = ino_off;
    hb->flags = flags & VMSF_ONCE;
    hb->page = pb;
    hb->page->refcount++;       /* 缓存引用 +1 */
    hb->page->flags |= PBF_INCACHE;

    /* 加入设备哈希（头插法） */
    hb->hash_next_dev = cache_hash_bydev[hv_dev];
    cache_hash_bydev[hv_dev] = hb;

    /* 如果有 inode 信息，加入 inode 哈希 */
    if(hb->ino != VMC_NO_INODE) addcache_byino(hb);

    lru_add(hb);
    return OK;
}
```

**rmcache**（[`cache.c:259-286`](../../../minix3/minix/servers/vm/cache.c#L259)）：

```c
void rmcache(struct cached_page *cp)
{
    cp->page->flags &= ~PBF_INCACHE;

    /* 从两个哈希表中移除 */
    rmhash_bydev(cp, &cache_hash_bydev[makehash(cp->dev, cp->dev_offset)]);
    if(cp->ino != VMC_NO_INODE)
        rmhash_byino(cp, &cache_hash_byino[makehash(cp->ino, cp->ino_offset)]);

    cp->page->refcount--;       /* 缓存引用 -1 */

    lru_rm(cp);

    /* 如果物理块无人引用，释放物理页 */
    if(pb->refcount == 0) {
        free_mem(ABS2CLICK(pb->phys), 1);
        SLABFREE(pb);
    }

    SLABFREE(cp);
}
```

**rmcache 的调用者**：`do_forgetcache`、`do_clearcache`、`cache_freepages`、`do_setcache`（过时缓存项）。

### 2.5 查找函数

**find_cached_page_bydev**（[`cache.c:177-195`](../../../minix3/minix/servers/vm/cache.c#L177)）：

```c
struct cached_page *
find_cached_page_bydev(dev_t dev, u64_t dev_off, ino_t ino, u64_t ino_off, int touchlru)
{
    for(hb = cache_hash_bydev[makehash(dev, dev_off)]; hb; hb=hb->hash_next_dev) {
        if(hb->dev == dev && hb->dev_offset == dev_off) {
            /* 如果传入了 inode 信息，更新哈希（延迟更新） */
            if(ino != VMC_NO_INODE) {
                if(hb->ino != ino || hb->ino_offset != ino_off)
                    update_inohash(hb, ino, ino_off);
            }
            if(touchlru) cache_lru_touch(hb);
            return hb;
        }
    }
    return NULL;
}
```

**`touchlru` 参数**：`1` 表示访问后更新 LRU 位置（正常查找），`0` 表示仅查找不更新（`do_forgetcache` 中使用，因为即将移除）。

**find_cached_page_byino**（[`cache.c:198-215`](../../../minix3/minix/servers/vm/cache.c#L198)）：

```c
struct cached_page *find_cached_page_byino(dev_t dev, ino_t ino, u64_t ino_off, int touchlru)
{
    assert(ino != VMC_NO_INODE);
    assert(dev != NO_DEV);

    for(hb = cache_hash_byino[makehash(ino, ino_off)]; hb; hb=hb->hash_next_ino) {
        if(hb->dev == dev && hb->ino == ino && hb->ino_offset == ino_off) {
            if(touchlru) cache_lru_touch(hb);
            return hb;
        }
    }
    return NULL;
}
```

**inode 查找需要额外验证 `dev`**：因为不同设备上可能有相同的 inode 号。

### 2.6 clear_cache_bydev

**源码位置**: [`cache.c:313-325`](../../../minix3/minix/servers/vm/cache.c#L313)

```c
void clear_cache_bydev(dev_t dev)
{
    struct cached_page *cp, *ncp;
    int h;

    for (h = 0; h < HASHSIZE; h++) {
        for (cp = cache_hash_bydev[h]; cp != NULL; cp = ncp) {
            ncp = cp->hash_next_dev;
            if (cp->dev == dev) rmcache(cp);
        }
    }
}
```

遍历整个设备哈希表，移除指定设备的所有缓存页。`ncp` 预保存下一个指针，因为 `rmcache` 会释放当前节点。

---

## 3. C 源码分析：mem_cache.c — 缓存 IPC 请求处理

### 3.1 四个缓存 IPC 请求

| 请求码 | 处理函数 | 功能 | 错误码 |
|--------|---------|------|--------|
| `VM_MAPCACHEPAGE` | `do_mapcache` | 将缓存块映射到调用者地址空间 | `ENOENT`/`ENOMEM`/`EINVAL`/`EFAULT` |
| `VM_SETCACHEPAGE` | `do_setcache` | 将匿名内存标记为缓存 | `EFAULT`/`EINVAL`/`ENOMEM` |
| `VM_FORGETCACHEPAGE` | `do_forgetcache` | 使指定范围的缓存失效 | `EINVAL`/`EFAULT` |
| `VM_CLEARCACHE` | `do_clearcache` | 使指定设备的全部缓存失效 | 无 |

### 3.2 do_mapcache — 映射缓存块

**源码位置**: [`mem_cache.c:95-194`](../../../minix3/minix/servers/vm/mem_cache.c#L95)

**调用者**：文件系统（VFS），需要将缓存块映射到自己的地址空间进行读写。

**处理流程**：

```
1. 验证参数：dev_off 和 ino_offset 必须页对齐，bytes >= PAGE_SIZE
2. 在调用者地址空间分配 cache memtype 区域（map_page_region）
3. 逐页查找缓存并映射：
   a. find_cached_page_bydev(dev, dev_off + offset, ino, ino_off + offset, 1)
   b. 未找到或 VMSF_ONCE → 回滚已映射区域，返回 ENOENT
   c. 设置 vr->param.pb_cache = hb->page（预加载指针）
   d. map_pf() 触发缺页 → cache_pagefault 从 pb_cache 链接
   e. 验证 pb_cache 已被清除
4. 返回映射的虚拟地址
```

**ENOENT 的两种情况**：
- 缓存未命中：`find_cached_page_bydev` 返回 `NULL`
- 一次性缓存：`hb->flags & VMSF_ONCE`（映射一次后自动失效）

### 3.3 do_setcache — 注册缓存块

**源码位置**: [`mem_cache.c:196-281`](../../../minix3/minix/servers/vm/mem_cache.c#L196)

**调用者**：文件系统，将已分配的匿名内存页注册为缓存。

**处理流程**：

```
1. 验证参数：bytes >= PAGE_SIZE，dev_off 和 ino_offset 页对齐
2. 逐页处理：
   a. map_lookup 查找调用者地址空间中的物理区域
   b. find_cached_page_bydev 检查是否已有缓存项
      - 已有且有效（同一 phys_block 且非 VMSF_ONCE）→ 跳过
      - 已有但过时（不同 phys_block 或 VMSF_ONCE）→ rmcache 移除旧项
   c. 验证 memtype 必须是 anon 或 anon_contig
   d. 验证 refcount == 1（只有调用者持有）
   e. phys_region->memtype = &mem_type_cache（就地切换 memtype）
   f. addcache 注册到缓存索引
```

**memtype 就地切换**：`phys_region->memtype = &mem_type_cache` 是 Minix3 的关键模式——同一个 `PhysRegion` 的 memtype 在运行时改变。Rust 实现需要支持这种动态切换（[12-memtype.md](12-memtype.md) §3.5）。

### 3.4 do_forgetcache — 使指定范围缓存失效

**源码位置**: [`mem_cache.c:283-313`](../../../minix3/minix/servers/vm/mem_cache.c#L283)

```c
int do_forgetcache(message *msg)
{
    dev_t dev = msg->m_vmmcp.dev;
    uint64_t dev_off = msg->m_vmmcp.dev_offset;
    phys_bytes bytes = msg->m_vmmcp.pages * VM_PAGE_SIZE;

    for (offset = 0; offset < bytes; offset += VM_PAGE_SIZE) {
        if ((hb = find_cached_page_bydev(dev, dev_off + offset,
            VMC_NO_INODE, 0, 0)) != NULL)
            rmcache(hb);
    }
    return OK;
}
```

**注意**：`touchlru = 0`——因为即将移除，无需更新 LRU 位置。`ino = VMC_NO_INODE`——forget 操作只按设备偏移查找，不关心 inode 信息。

### 3.5 do_clearcache — 使指定设备的全部缓存失效

**源码位置**: [`mem_cache.c:315-324`](../../../minix3/minix/servers/vm/mem_cache.c#L315)

```c
int do_clearcache(message *msg)
{
    dev_t dev = msg->m_vmmcp.dev;
    clear_cache_bydev(dev);
    return OK;
}
```

设备卸载时调用，清除该设备的所有缓存页。

---

## 4. Rust 设计决策

### 4.1 从开链法哈希到 BTreeMap

Minix3 使用开链法静态数组（`HASHSIZE=65536`）实现哈希表。Rust 实现选择 `BTreeMap`：

| 方面 | Minix3 | Rust |
|------|--------|------|
| 数据结构 | `cached_page*` 数组 + 开链法 | `BTreeMap<CacheKey, PageCacheEntry>` |
| 查找复杂度 | O(1) 平均，O(n) 最坏 | O(log n) |
| 内存开销 | 预分配 65536 指针 | 按需分配 |
| 排序 | 无 | 有序遍历 |
| no_std | N/A | `alloc::collections::BTreeMap` 可用 |

**选择 BTreeMap 而非 HashMap 的原因**：
1. `alloc` 中无 `HashMap`（需要 `hashbrown` 依赖）
2. 有序遍历对 `clear_by_dev` 有利（可范围查询）
3. VM 缓存操作不在热路径上，O(log n) 可接受
4. 避免引入额外依赖

### 4.2 CacheKey 的枚举设计

Minix3 的双哈希表在 Rust 中合并为单一 `BTreeMap`，通过枚举键区分两种查找路径：

```rust
pub(crate) enum CacheKey {
    ByInode { dev: u64, ino: u64, offset: u64 },
    ByDevice { dev: u64, offset: u64 },
}
```

**为什么不用两个 BTreeMap？** Minix3 中同一个 `cached_page` 同时存在于两个哈希表中。在 Rust 中，如果用两个 map，同一数据需要两个引用，引入同步复杂度。单一 map + 枚举键避免了这个问题——每个缓存页只有一个入口，通过键类型区分查找路径。

**ByDevice 键的语义**：对应 Minix3 中 `ino == VMC_NO_INODE` 的缓存页。`find_cached_page_bydev` 在 C 中按 `(dev, dev_offset)` 查找后还会检查 inode 信息；Rust 中 `ByDevice` 键直接表示"无 inode 信息的缓存页"。

### 4.3 LRU 淘汰策略

Minix3 使用双向链表实现精确 LRU。Rust 实现分两阶段：

**当前阶段**：简化 LRU——`Vec<CacheKey>` 维护访问顺序，淘汰时从头扫描。`Vec::retain` 的时间复杂度为 O(n)，但 VM 缓存操作频率低，可接受。

**未来优化**：如果缓存性能成为瓶颈，可替换为 `alloc::collections::VecDeque` 或引入 `hashlink` crate 实现 O(1) LRU。

### 4.4 PFN 模型下的引用计数

Minix3 的 `phys_block.refcount` 手动管理（`addcache` 时 `++`，`rmcache` 时 `--`）。PFN 模型下，引用计数由 `PageFrames` 管理：

| Minix3 操作 | Rust 操作 |
|------------|----------|
| `addcache`: `pb->refcount++` | `PageCache::insert`: `frames.addcache(pfn)` |
| `rmcache`: `pb->refcount--` | `PageCache::remove`: `frames.rmcache(pfn)` |
| `cache_freepages`: 检查 `refcount == 1` | `PageCache::free_pages`: 检查 `frames.get(pfn).refcount == 1` |

`frames.addcache(pfn)` 和 `frames.rmcache(pfn)` 在 `PageFrames` 中增减引用计数，语义等价于 C 的 `refcount++`/`refcount--`。

### 4.5 缓存 IPC 请求的错误码对齐

| 请求 | C 错误码 | Rust VmError | errno |
|------|---------|-------------|-------|
| `do_mapcache` 未命中 | `ENOENT` | `InvalidAddress` | `EFAULT` |
| `do_mapcache` 映射失败 | `ENOMEM` | `OutOfMemory` | `ENOMEM` |
| `do_mapcache` 参数错误 | `EINVAL`/`EFAULT` | `InvalidAddress` | `EFAULT` |
| `do_setcache` 类型错误 | `EFAULT` | `AccessViolation` | `EACCES` |
| `do_setcache` addcache 失败 | `ENOMEM`/`EINVAL` | `OutOfMemory` | `ENOMEM` |
| `do_forgetcache` 参数错误 | `EINVAL`/`EFAULT` | `InvalidAddress` | `EFAULT` |

**注意**：C 中 `do_mapcache` 未命中返回 `ENOENT`，但 `ENOENT` 在 VM 错误码中没有直接对应。Rust 中映射为 `InvalidAddress`（`EFAULT`），因为从调用者视角看，请求的缓存地址无效。这是 C→Rust 错误码映射中少数不对齐的情况，需在代码注释中说明。

---

## 5. Rust 实现

### 5.1 PageCache 数据结构

> 设计决策：§4.1（BTreeMap）、§4.2（枚举键）、§4.3（简化 LRU）、§4.4（PFN 引用计数）

```rust
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CacheKey {
    ByInode { dev: u64, ino: u64, offset: u64 },
    ByDevice { dev: u64, offset: u64 },
}

#[derive(Debug)]
pub(crate) struct PageCacheEntry {
    pub pfn: u32,
    pub refcount: u16,
}

pub(crate) struct PageCache {
    entries: BTreeMap<CacheKey, PageCacheEntry>,
    lru: Vec<CacheKey>,
    total_cached: u64,
}
```

**与 Minix3 的对应**：

| Minix3 | Rust | 说明 |
|--------|------|------|
| `cache_hash_bydev[HASHSIZE]` + `cache_hash_byino[HASHSIZE]` | `entries: BTreeMap<CacheKey, ...>` | 双哈希表合并为单一有序 map |
| `cached_page.page` | `PageCacheEntry.pfn` | 物理块指针 → PFN 索引 |
| `cached_page.older`/`newer` | `lru: Vec<CacheKey>` | 双向链表 → Vec（简化版） |
| `cached_pages` | `total_cached: u64` | 缓存页计数 |

### 5.2 核心操作

```rust
impl PageCache {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            lru: Vec::new(),
            total_cached: 0,
        }
    }

    /// 对应 Minix3 的 find_cached_page_byino。
    /// 按 (dev, ino, ino_offset) 查找缓存页。
    pub fn find_by_inode(&self, dev: u64, ino: u64, offset: u64) -> Option<&PageCacheEntry> {
        self.entries.get(&CacheKey::ByInode { dev, ino, offset })
    }

    /// 对应 Minix3 的 find_cached_page_bydev。
    /// 按 (dev, dev_offset) 查找缓存页（无 inode 信息的缓存页）。
    pub fn find_by_device(&self, dev: u64, offset: u64) -> Option<&PageCacheEntry> {
        self.entries.get(&CacheKey::ByDevice { dev, offset })
    }

    /// 对应 Minix3 的 addcache。
    /// 将缓存页加入索引，同时增加 PageFrames 中的引用计数。
    pub fn insert(&mut self, key: CacheKey, pfn: u32, frames: &mut PageFrames) {
        frames.addcache(pfn);
        self.entries.insert(key.clone(), PageCacheEntry { pfn, refcount: 1 });
        self.lru.push(key);
        self.total_cached += 1;
    }

    /// 对应 Minix3 的 rmcache。
    /// 从索引中移除缓存页，同时减少 PageFrames 中的引用计数。
    pub fn remove(&mut self, key: &CacheKey, frames: &mut PageFrames) -> Option<u32> {
        if let Some(entry) = self.entries.remove(key) {
            frames.rmcache(entry.pfn);
            self.lru.retain(|k| k != key);
            self.total_cached = self.total_cached.saturating_sub(1);
            Some(entry.pfn)
        } else {
            None
        }
    }

    /// 对应 Minix3 的 cache_freepages。
    /// 内存不足时从 LRU 最老端回收缓存页。
    /// 淘汰条件：refcount == 1（只被缓存引用，无人映射）。
    pub fn free_pages(&mut self, needed: usize, frames: &mut PageFrames) -> usize {
        let mut freed = 0;
        let mut keys_to_remove = Vec::new();

        for key in &self.lru {
            if freed >= needed { break; }
            if let Some(entry) = self.entries.get(key) {
                if frames.get(entry.pfn)
                    .map(|s| s.refcount == 1)
                    .unwrap_or(false)
                {
                    keys_to_remove.push(key.clone());
                    freed += 1;
                }
            }
        }

        for key in keys_to_remove {
            self.remove(&key, frames);
        }

        freed
    }

    /// 对应 Minix3 的 clear_cache_bydev。
    /// 移除指定设备的所有缓存页。
    pub fn clear_by_dev(&mut self, dev: u64, frames: &mut PageFrames) {
        let keys: Vec<CacheKey> = self.entries.keys()
            .filter(|k| match k {
                CacheKey::ByInode { dev: d, .. } => *d == dev,
                CacheKey::ByDevice { dev: d, .. } => *d == dev,
            })
            .cloned()
            .collect();
        for key in keys {
            self.remove(&key, frames);
        }
    }
}
```

### 5.3 缓存 IPC 请求的 In/Out 类型

> 请求码已在 `minix-types/src/ipc/vm.rs` 中定义：
> `VM_MAPCACHEPAGE`、`VM_SETCACHEPAGE`、`VM_FORGETCACHEPAGE`、`VM_CLEARCACHE`

```rust
/// VM_MAPCACHEPAGE / VM_SETCACHEPAGE / VM_FORGETCACHEPAGE 共享的请求格式。
/// 对应 Minix3 的 m_vmmcp 消息字段。
pub struct VmCacheIn {
    pub dev: u64,
    pub dev_offset: u64,
    pub ino: u64,
    pub ino_offset: u64,
    pub pages: u32,
    pub flags: u32,
    pub block: u64,     // do_setcache 专用：调用者地址空间中的块地址
}

/// VM_MAPCACHEPAGE 的回复：映射后的虚拟地址。
pub struct VmCacheMapOut {
    pub addr: VirBytes,
}

/// VM_SETCACHEPAGE / VM_FORGETCACHEPAGE / VM_CLEARCACHE 的回复：仅成功/失败。
/// 使用 VmReply::Ok 或 VmReply::Error。
```

### 5.4 Dispatcher 中的缓存请求处理

```rust
impl MessageDispatcher {
    /// 对应 Minix3 do_mapcache() (mem_cache.c:95)。
    /// 将缓存块映射到调用者地址空间。
    pub(crate) fn dispatch_mapcache(
        table: &VmProcTable,
        cache: &mut PageCache,
        frames: &mut PageFrames,
        request: VmCacheIn,
    ) -> VmReply {
        // 1. 验证参数对齐和大小
        // 2. 在调用者地址空间分配 cache memtype 区域
        // 3. 逐页查找缓存并映射
        // 4. 返回映射地址
        todo!("implement dispatch_mapcache")
    }

    /// 对应 Minix3 do_setcache() (mem_cache.c:196)。
    /// 将匿名内存标记为缓存并加入 PageCache 索引。
    pub(crate) fn dispatch_setcache(
        table: &VmProcTable,
        cache: &mut PageCache,
        frames: &mut PageFrames,
        request: VmCacheIn,
    ) -> VmReply {
        // 1. 验证参数
        // 2. 逐页：查找已有缓存项 → 验证 memtype → 切换 memtype → addcache
        todo!("implement dispatch_setcache")
    }

    /// 对应 Minix3 do_forgetcache() (mem_cache.c:283)。
    /// 使指定范围的缓存失效。
    pub(crate) fn dispatch_forgetcache(
        cache: &mut PageCache,
        frames: &mut PageFrames,
        request: VmCacheIn,
    ) -> VmReply {
        let bytes = request.pages as u64 * 4096;
        for offset in (0..bytes).step_by(4096) {
            let key = CacheKey::ByDevice { dev: request.dev, offset: request.dev_offset + offset };
            cache.remove(&key, frames);
        }
        VmReply::Ok
    }

    /// 对应 Minix3 do_clearcache() (mem_cache.c:315)。
    /// 使指定设备的全部缓存失效。
    pub(crate) fn dispatch_clearcache(
        cache: &mut PageCache,
        frames: &mut PageFrames,
        request: VmCacheIn,
    ) -> VmReply {
        cache.clear_by_dev(request.dev, frames);
        VmReply::Ok
    }
}
```

---

## 6. 测试要点

| 测试 | 描述 | 覆盖设计 |
|------|------|---------|
| `test_page_cache_insert_remove_by_inode` | 按 inode 添加和移除 | §5.2 insert/remove |
| `test_page_cache_by_device` | 按设备查找 | §5.2 find_by_device |
| `test_page_cache_refcount` | 引用计数增减 | §5.2 increase/decrease_refcount |
| `test_page_cache_find_by_pfn` | 反向查找 | §5.2 find_by_pfn |
| `test_page_cache_flush_all` | 全部清除 | §5.2 flush_all |
| `test_page_cache_free_pages_lru` | LRU 淘汰 refcount==1 的页 | §5.2 free_pages |
| `test_page_cache_clear_by_dev` | 按设备清除 | §5.2 clear_by_dev |
| `test_dispatch_forgetcache` | forgetcache 请求处理 | §5.4 dispatch_forgetcache |
| `test_dispatch_clearcache` | clearcache 请求处理 | §5.4 dispatch_clearcache |

---

## 7. 参见

| 文档 | 关系 |
|------|------|
| [12-memtype.md](12-memtype.md) | CacheMemory memtype 定义（§4.6）、memtype 就地切换（§3.5） |
| [23-vfs-interaction.md](23-vfs-interaction.md) | MappedFile 使用 PageCache 查找缓存页 |
| [24-vm-ipc-dispatch.md](24-vm-ipc-dispatch.md) | IPC 分发框架，缓存请求的分发入口 |
