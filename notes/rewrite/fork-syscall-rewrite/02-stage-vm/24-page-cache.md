# 24-page-cache: 页缓存 —— 磁盘块的缓存目录、LRU 淘汰与四个缓存 IPC

> **分类**: 阶段 8 — 跨服务协作（页缓存：所有文件系统共享的磁盘块缓存中介）
> **源码**: `minix3/minix/servers/vm/cache.c`（全 332 行：`lru_rm` :29-50 / `lru_add` :52-68 / `cache_lru_touch` :70-74 / `makehash` :76-84 / `cache_sanitycheck_internal` :86-143 / `rmhash_byino/bydev` :144-153 / `addcache_byino` :155-161 / `update_inohash` :163-174 / `find_cached_page_bydev` :177-196 / `find_cached_page_byino` :198-214 / `addcache` :216-257 / `rmcache` :259-286 / `cache_freepages` :288-307 / `clear_cache_bydev` :312-325 / `get_stats_info` :328-331）+ `minix3/minix/servers/vm/cache.h`（`struct cached_page` :2-21）+ `minix3/minix/servers/vm/mem_cache.c`（全 324 行：`mem_type_cache` :39-49 / `cache_pt_flags` :51-57 / `cache_reference` :60-63 / `cache_unreference` :65-68 / `cache_sanitycheck` :70-74 / `cache_writable` :76-81 / `cache_resize` :83-87 / `cache_lowshrink` :89-92 / `do_mapcache` :95-179 / `cache_pagefault` :181-193 / `do_setcache` :196-277 / `do_forgetcache` :283-309 / `do_clearcache` :315-324）+ 调用面（`mem_file.c`：`mappedfile_pagefault` 缓存命中/ONCE 分流 :104-138、`mappedfile_setfile` 预填 :210-240；`main.c`：CALLMAP :569-572；`alloc.c`：`alloc_mem` 耗尽后 `cache_freepages(clicks)` 重试 :260-262；`libminixfs/cache.c`：`ONE_SHOT` → `VMSF_ONCE` :565/:704）
> **Rust 模块**: `os/servers/vm/src/page_cache.rs`（`VMC_NO_INODE` :42 / `VMSF_ONCE` :48 / `CachedPageRef` :56 / `CacheError` :65 / `CachedPage` :76 / `LruNode` :96 / `LruList` :102 / `PageCache` :201 / `addcache` :232 / `rmcache` :281 / `find_by_dev` :317 / `find_by_ino` :353 / `free_pages` :375 / `clear_by_dev` :406 / `total_cached` :421）+ `os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_mapcache` :482 / `unmap_region_pages` :573 / `dispatch_setcache` :607 / `dispatch_forgetcache` :714 / `dispatch_clearcache` :740）+ `os/servers/vm/src/memtype.rs`（`CacheMemory` :795 / `MappedFile` :919）+ `os/servers/vm/src/vm_server.rs`（`FREE_CACHE_BATCH` :41 / `alloc_cycle` :424 / 4 个 `handle_*cache` :1257-1278 / `MapCache` 回复编码 :1123）+ `os/servers/vm/src/region/page_state.rs`（`PageFlags::IN_CACHE` :30 / `PageFrames::addcache` :166 / `rmcache` :175）+ `os/libs/minix-types/src/ipc/message.rs`（`MessVmmcp` :1948 / `MessVmmcpReply` :2001 / 联合体成员 :154-157）+ `os/libs/minix-types/src/ipc/vm.rs`（`VmCacheIn::decode_message` :1053）
> **前置**: `12-memtype.md`（memtype 回调体系）、`23-vfs-interaction.md`（VFS 异步对话、mappedfile 消费缓存命中）
> **说明**: 本文档管 **VM 侧磁盘块页缓存**——`(dev, dev_offset)` 主键目录 + `(dev, ino, ino_offset)` 辅索引、精确 LRU、refcount 淘汰，以及四个缓存 IPC handler（mapcache/setcache/forgetcache/clearcache）。**不覆盖**：VFS 异步请求队列（23）、memtype 回调体系本体（12）、物理分配器（05/06）、主循环分发框架（15）。

---

## 1. 概念：VM 作为所有文件系统的磁盘块缓存中介

### 1.0 章节引言

**目标读者**：已读完 12（memtype）、23（VFS 异步对话）、15（主循环分发）的读者。本文档回答三个问题：为什么磁盘块缓存必须住在 VM？缓存块如何被查找（双键模型）？内存压力下如何淘汰（LRU + refcount）？

**本章不讲什么**：VFS 请求队列的串行激活与 fdref（23）、memtype 六类回调（12）、物理页分配器（05/06）——这里只讲"缓存目录"这一侧：索引、LRU、淘汰、四个 IPC 服务。

### 1.1 为什么页缓存放 VM

Minix3 的文件系统（MFS/ISOFS/PFS 等）运行在用户态，通过 VFS 挂到系统里。它们读写磁盘块时，块内容必须落到物理页——而**物理页与页表只有 VM 拥有**。于是磁盘块缓存的"目录"（哪个磁盘块对应哪个物理页）只能住在 VM：

```
FS 进程（用户态，经 VFS）
  │  vm_set_cacheblock / vm_map_cacheblock / vm_forget_cacheblock / vm_clear_cache
  ▼
VM（拥有物理页 + 页表）
  ├─ PageCache 目录： (dev, dev_offset) → 物理页
  ├─ LRU 淘汰：只淘汰无人映射的页（refcount == 1）
  └─ CacheMemory memtype：把缓存块映射进 FS 地址空间
```

把缓存放 VM 的收益：多个 FS 共享同一块缓存（同一磁盘块只读一次进内存）、块内容与物理页生命周期统一管理、FS 崩溃不影响已缓存的页。代价：FS 每次访问块都要过一次 IPC（`vm_map_cacheblock`，libsys/vm_cache.c:47-54）。

### 1.2 双键模型：磁盘块地址是主键，inode 偏移是元信息

每个缓存块**恰好一个** `cached_page` 节点（cache.h:2-21）。节点的主身份是**磁盘块地址** `(dev, dev_offset)`——`dev` 是设备号，`dev_offset` 是设备内偏移。`(ino, ino_offset)` 是**同一节点上的元信息**：它描述"这个磁盘块属于哪个文件的哪个逻辑偏移"，可能缺失（`VMC_NO_INODE = 0`，原始设备块没有文件归属）。

```
cached_page { dev, dev_offset, ino, ino_offset, flags, page, LRU 指针, 哈希链 }
              ├── 主索引（bydev 哈希）：(dev, dev_offset) → 节点
              └── 辅索引（byino 哈希）：(ino, ino_offset) → 同一节点（若 ino 已知）
```

**两个索引挂同一个节点**——这不是两份缓存，而是同一份缓存的两个查询入口：

- **bydev**：文件系统读写磁盘块时用（`do_mapcache`/`do_setcache`/`do_forgetcache` 全走 bydev）。
- **byino**：文件映射缺页时用（`mappedfile_pagefault` 按 `(dev, ino, ino_offset)` 找，mem_file.c:104-110）。

**延迟 ino 更新**（`update_inohash`，cache.c:163-174）：`find_cached_page_bydev` 命中后，如果调用方提供了 ino 信息且与节点不符，就把节点的 ino 元信息更新并**挪到新的 ino 桶**。这是"两阶段登记"的桥接：FS 可以先只按磁盘块地址登记（读块时还不知道文件归属），之后文件映射按 ino 查到同一个块。

### 1.3 LRU + refcount 淘汰

淘汰语义由两个机制合成：

1. **精确 LRU**：所有缓存节点挂在一条双向链表上，`lru_add` 插最新端、`lru_rm` 摘除、`cache_lru_touch` 移到最新端（cache.c:29-75）。淘汰从**最老端**开始（`cache_freepages`，cache.c:288-305）。
2. **refcount 条件**：物理块被 `addcache` 时 `refcount++` + 置 `PBF_INCACHE`（cache.c:243-244，拒绝重复登记 :222-226）；被某个地址空间映射时再 `refcount++`。淘汰条件 `refcount == 1` = **只有缓存引用、没有任何地址空间映射**（cache.c:296-302）。`rmcache` 后 refcount 归零 → `free_mem` 把物理页还给分配器（cache.c:280-284）。

```
内存压力（alloc_mem 耗尽）
  └─ cache_freepages(clicks)（alloc.c:260-262，clicks=请求页数）
       └─ 从 LRU 最老端扫描：refcount==1 → rmcache（物理页归还）；refcount>1 → 跳过
```

### 1.4 VMSF_ONCE：一次性块

`VMSF_ONCE`（minix/include/minix/vm.h:93）是**缓存节点的 flag**，由 FS 经 libminixfs 的 `ONE_SHOT` 标志设置（libminixfs/cache.c:565/:704）——用于"用完即弃"的块（如元数据块，FS 不希望 VM 保留旧副本）。三处消费语义：

| 场景 | C 行为 | 位置 |
|------|--------|------|
| `do_mapcache` 映射 | 遇 ONCE 条目返回 `ENOENT`（不消费） | mem_cache.c:147-158 |
| `do_setcache` 重登记 | 旧条目是 ONCE → `rmcache` 重建（同页非 ONCE 才跳过） | mem_cache.c:236-245 |
| 文件缺页命中 | 强制走 VFS 往返（"for one-time use pages, no caching is performed"） | mem_file.c:120-131 |

### 1.5 四个缓存 IPC 服务

| 请求码 | handler | 功能 | 错误码 |
|--------|---------|------|--------|
| `VM_MAPCACHEPAGE`（com.h:682） | `do_mapcache`（mem_cache.c:95-179） | 把缓存块映射进调用者（FS）地址空间，返回 vaddr | EFAULT/EINVAL/ENOENT/ENOMEM |
| `VM_SETCACHEPAGE`（com.h:685） | `do_setcache`（mem_cache.c:196-277） | 把 FS 自己的匿名页登记为缓存块 | EFAULT/EINVAL |
| `VM_FORGETCACHEPAGE`（com.h:688） | `do_forgetcache`（mem_cache.c:283-309） | 使指定设备偏移范围的缓存失效 | EFAULT/EINVAL |
| `VM_CLEARCACHE`（com.h:691） | `do_clearcache`（mem_cache.c:315-324） | 使指定设备的全部缓存失效（卸载） | 无 |

### 1.6 对照：Redox 与 Linux

- **Linux**：page cache 在内核，`address_space` 的 XArray（原 radix tree）按 `(inode, offset)` 索引，LRU 用 per-cpu `list_lru`（近似 LRU），回收由 `shrink_slab`/`try_to_free_pages` 挂在"分配失败 → 回收 → 重试"路径。索引键是 `(inode, offset)`——因为 Linux 的块设备与文件系统统一走 address_space，磁盘块地址（`(dev, block)`）不是用户可见身份。
- **Redox**：内核 memory manager 只做映射，**没有集中式页缓存**——文件数据缓存由用户态 scheme（redoxfs 等）自己做，内核经 `fmap`/`mmap` scheme 调用按需取页。分配失败直接返回错误，无内核侧回收。
- **Minix3 的位置**：介于两者之间——像 Linux 一样有集中式页缓存，但住在**用户态 VM 服务器**（与 FS 同层，经 IPC 对话）；索引保留 `(dev, dev_offset)` 主键（因为 FS 是独立进程，设备块是它们之间最自然的共享身份），辅以 `(ino, ino_offset)` 服务文件映射。
- **对照要点**：三家的共同语义是"**以缓存为中转、映射与读文件分离、内存压力下先回收缓存**"；Minix3 的独特之处是**精确 LRU + 只淘汰未映射页（refcount==1）**——VM 无法像 Linux 那样回收"已映射但未脏"的页（那需要 TLB shootdown 与页表遍历），所以淘汰面被限定在无人映射的缓存块，这也是缓存引用计数的根本原因。

### 1.7 小结

页缓存 = **双键目录**（`(dev,dev_offset)` 主键 + `(dev,ino,ino_offset)` 辅索引，同一节点两个入口）+ **精确 LRU**（O(1) touch/remove）+ **refcount 淘汰**（只淘汰未映射页，归零还页）+ **VMSF_ONCE 一次性语义** + **四个 IPC 服务**。Rust 侧对应：`PageCache`（目录 + LRU）、`dispatch_*cache`（四个 handler）、`CacheMemory`（映射 memtype）、`alloc_cycle`（回收接线）。

---

## 2. C 源码分析

### 2.1 cache.h：cached_page 结构与不变量

**源码位置**: [`cache.h:2-21`](../../../minix3/minix/servers/vm/cache.h)

```c
struct cached_page {
	dev_t dev;			/* which dev is it on */
	u64_t dev_offset;		/* offset within dev */
	ino_t ino;			/* which ino is it about */
	u64_t ino_offset;		/* offset within ino */
	int flags;			/* currently only VMSF_ONCE or 0 */
	struct phys_block *page;	/* page ptr */
	struct cached_page *older;	/* older in lru chain */
	struct cached_page *newer;	/* newer in lru chain */
	struct cached_page *hash_next_dev; /* next in hash chain (bydev) */
	struct cached_page *hash_next_ino; /* next in hash chain (byino) */
};
```

注释声明的三条不变量（cache.h:4-10）：`(dev, dev_offset)` 唯一；`dev` 必须有效（≠ `NO_DEV`）；`ino` 可能未知（`VMC_NO_INODE`）。注意 **ino 信息不是节点身份**——`(ino, ino_offset)` "duplicate do not make sense although it won't bother VM much"（cache.h:5-6），即辅索引允许重复/陈旧，主索引才是真相。

### 2.2 cache.c：双哈希与 LRU

**双哈希**（cache.c:21-27）：`HASHSIZE=65536` 的 `cache_hash_bydev[]` + `cache_hash_byino[]` 开链哈希，`makehash(p1, p2)` 用 `hash_mix`/`hash_final` 混合两个键（cache.c:77-84）。`cached_pages` 全局计数（cache.c:27）。

**LRU**（cache.c:29-75）：`lru_oldest`/`lru_newest` 双指针 + 节点 `older`/`newer`。`lru_rm` 摘链时用 `assert` 校验指针一致性（:29-49）；`cache_lru_touch = lru_rm + lru_add`（:71-75）。

**addcache**（cache.c:216-257）——登记一个新缓存块：

```c
if(pb->flags & PBF_INCACHE) { return EINVAL; }   /* 页已在缓存 → 拒绝 */
SLABALLOC(hb);                                     /* 分配节点 */
hb->dev = dev; hb->dev_offset = dev_off;
hb->ino = ino; hb->ino_offset = ino_off;
hb->flags = flags & VMSF_ONCE;                     /* 只保留 ONCE 位 */
hb->page = pb; pb->refcount++; pb->flags |= PBF_INCACHE;  /* 缓存引用 */
/* 头插进 bydev 哈希；ino 已知则同时进 byino 哈希 */
lru_add(hb);                                       /* 挂最新端 */
```

**rmcache**（cache.c:264-286）——撤销登记：

```c
pb->flags &= ~PBF_INCACHE;                 /* 清除缓存标志 */
rmhash_bydev(hb, ...);                     /* 摘 bydev 链 */
if(ino != VMC_NO_INODE) rmhash_byino(...); /* 摘 byino 链 */
pb->refcount--;                            /* 释放缓存引用 */
lru_rm(hb);                                /* 摘 LRU */
if(pb->refcount == 0) {                    /* 无任何地址空间映射 */
    free_mem(ABS2CLICK(pb->phys), 1);      /* 物理页归还分配器 */
    SLABFREE(pb);
}
SLABFREE(hb);
```

**查找**（cache.c:177-215）：`find_cached_page_bydev(dev, off, ino, ino_off, touchlru)` 按主键找；命中且传入真实 ino → `update_inohash`（挪 byino 桶）；`touchlru` → `cache_lru_touch`。`find_cached_page_byino(dev, ino, ino_off, touchlru)` 按辅键找，**必须校验 `hb->dev == dev`**（cache.c:209）——不同设备可以有相同 inode 号。

**淘汰**（cache.c:288-305）：从 `lru_oldest` 起扫描，`refcount == 1` → `rmcache`，累计 `freed` 达到目标即停；`refcount > 1` 只跳过（`skips++`）。

**清设备**（cache.c:312-325）：遍历整个 bydev 哈希，`dev` 匹配即 `rmcache`（先存 `ncp = cp->hash_next_dev` 防悬垂）。

### 2.3 mem_cache.c：CacheMemory memtype

`mem_type_cache`（mem_cache.c:39-50）的回调面：

| 回调 | 实现 | 语义 |
|------|------|------|
| `ev_reference` | 空（:60-63） | 无特殊引用处理 |
| `ev_unreference` | 转 `mem_type_anon`（:65-68） | 释放语义与匿名页一致 |
| `ev_sanitycheck` | `usedpages_add`（:70-74） | 健全检查 |
| `ev_writable` | `phys != MAP_NONE`（:76-81） | 恒真（FS 写块） |
| `ev_resize` | 拒绝（ENOMEM，:83-87） | 缓存区域不可扩展 |
| `ev_lowshrink` | 空（:89-92） | 低端收缩无特殊处理 |
| `ev_pagefault` | `pb_link` 预载块（:181-193） | 见下 |
| `ev_pt_flags` | arm 才非零（:51-57） | 缓存属性 |

**cache_pagefault**（mem_cache.c:181-190）——缺页时把区域参数 `region->param.pb_cache` 里预载的物理块链接进缺页槽：

```c
static int cache_pagefault(struct vmproc *vmp, struct vir_region *region,
	struct phys_region *ph, int write, ...)
{
	vir_bytes offset = ph->offset;
	assert(ph->ph->phys == MAP_NONE);
	assert(region->param.pb_cache);
	pb_unreferenced(region, ph, 0);
	pb_link(ph, region->param.pb_cache, offset, region);  /* 链接预载块 */
	region->param.pb_cache = NULL;                          /* 消费指针 */
	return OK;
}
```

### 2.4 do_mapcache：把缓存块映射进 FS 地址空间

**源码位置**: [`mem_cache.c:95-179`](../../../minix3/minix/servers/vm/mem_cache.c#L95)

```
1. 对齐验证：dev_off/ino_off 必须页对齐 → EFAULT（:108-111）
2. vm_isokendpt(m_source) → caller（:113-114；失败 panic "bogus source"）
3. bytes < VM_PAGE_SIZE → EINVAL（:116）
4. map_page_region(caller, VM_MMAPBASE, VM_MMAPTOP, bytes,
   VR_ANON|VR_WRITABLE, 0, &mem_type_cache)（:130-134）→ 分配 cache 区域
5. 逐页（:141-166）：
   a. find_cached_page_bydev(dev, dev_off+offset, ino, ino_off+offset, 1)
      ——未命中或 (hb->flags & VMSF_ONCE) → map_unmap_region 整区回滚 → ENOENT
   b. vr->param.pb_cache = hb->page（预载指针）
   c. map_pf(...) → cache_pagefault 把预载块链接进区域
6. msg->m_vmmcp_reply.addr = vr->vaddr（:170）→ 返回映射地址
```

要点：**总是 bydev 查找**（传入 ino 仅用于延迟更新）；**VMSF_ONCE 检查的是条目 flag**（`hb->flags`），与请求消息里的 flags 无关（do_mapcache 根本不读 `m_vmmcp.flags`）。

### 2.5 do_setcache：把 FS 匿名页登记为缓存块

**源码位置**: [`mem_cache.c:196-277`](../../../minix3/minix/servers/vm/mem_cache.c#L196)

```
1. bytes < VM_PAGE_SIZE → EINVAL（:208）；对齐 → EFAULT（:210-213）
2. vm_isokendpt → caller（:215-216）
3. 逐页（:218-270）：
   a. map_lookup(caller, block+offset, &phys_region)（:224-232）→ 无区域/无页 → EFAULT
   b. find_cached_page_bydev(dev, dev_off+offset, ino, ino_off+offset, 1)（:234-235）
      ├─ 同页 && 旧条目非 ONCE → continue（:236-249，块已在缓存）
      └─ 异页或旧条目 ONCE → rmcache(hb)（:237-245，旧条目过时）
   c. memtype 必须 anon/anon_contig → EFAULT（:252-256）
   d. ph->refcount != 1 → EFAULT（:258-261，页必须独占）
   e. phys_region->memtype = &mem_type_cache（:263）
   f. addcache(...)（:265-269）
```

### 2.6 do_forgetcache / do_clearcache

- `do_forgetcache`（mem_cache.c:283-309）：`bytes < PAGE_SIZE → EINVAL`；`dev_off % PAGE_SIZE → EFAULT`；逐页 `find_cached_page_bydev(dev, dev_off+offset, VMC_NO_INODE, 0, 0)`（**touchlru=0**——马上要删，不必碰 LRU）→ `rmcache`。
- `do_clearcache`（mem_cache.c:315-324）：只读 `dev`，调 `clear_cache_bydev(dev)`。

### 2.7 调用面：谁消费缓存

| 调用者 | 路径 | 说明 |
|--------|------|------|
| `mappedfile_pagefault`（mem_file.c:104-138） | `find_cached_page_byino/bydev` → 命中 `pb_link`；写/末页 → `cow_block`；ONCE → 强制 VFS 往返；映射后 ONCE → `rmcache` | 文件映射缺页的缓存命中路径（23 篇消费侧） |
| `mappedfile_setfile`（mem_file.c:210-240） | prefill 时逐页查缓存预填 | 区域初始化预填 |
| libminixfs（libminixfs/cache.c:565/:704） | `put_block(..., ONE_SHOT)` → `VMSF_ONCE` | FS 侧一次性块登记 |
| `alloc_mem`（alloc.c:242-279） | 耗尽 → `cache_freepages(clicks)`（clicks=请求页数，alloc.c:260-262）→ 重试 | 内存压力回收 |
| `main.c:569-572` | CALLMAP 注册 4 个 handler | 分发入口（15 篇） |

### 2.8 C 小结：符号全景

`cache.c`（15 函数：7 静态 + 8 导出）+ `mem_cache.c`（8 回调 + 4 handler）全部进入本篇覆盖契约；`PBF_INCACHE` 由 region.c sanity 使用（region.c:219/:244）；`VMC_NO_INODE`/`VMSF_ONCE` 定义在 `minix/include/minix/vm.h:90/:93`。

---

## 3. Rust 设计决策

### 3.1 D1：单键模型 —— `(dev, dev_offset)` 主键 + `(dev, ino, ino_offset)` 辅索引（替代旧双键模型）

**C**: 一个磁盘块恰好一个 `cached_page`，bydev 是主身份，byino 挂同一节点（cache.h:2-21，cache.c:23-24）。
**Rust**: `PageCache { by_dev: BTreeMap<(u64,u64), CachedPage>, by_ino: BTreeMap<(u64,u64,u64), (u64,u64)> }`（page_cache.rs:201-216）——主索引存条目，辅索引映射到主键。

**修正的旧模型缺陷**（24-P0-1 系列，见 §4.3）：旧实现用 `CacheKey::ByInode/ByDevice` 两个独立键类型，同一 PFN 可以两个键并存，产生三个语义偏离：

1. **重复登记无守卫**：C 的 `PBF_INCACHE` 检查（cache.c:222-226）拒绝已缓存的物理块；旧 `insert` 无条件插入，同一 PFN 双条目 → 双倍 refcount、淘汰语义错乱。
2. **bydev 查找漏查 ino 条目**：C 的 `do_mapcache`/`do_setcache` **总是** bydev 查找（mem_cache.c:147/:235），旧实现 `ino != 0` 时改走 `find_by_inode`——FS 先按块地址登记（ino=0）、后按 ino 映射时必然 ENOENT。
3. **延迟 ino 更新无法表达**：`update_inohash`（cache.c:163-174）要求"bydev 命中时更新节点元信息"，双键模型没有"节点上的元信息"概念。

**架构标注**: 双链哈希 → BTreeMap 双索引，`[ARCH: A-4 家族]`（与 14-region-lookup 同源：链式哈希 → 平衡树）。行为等价：O(log n) vs O(1) 摊还，缓存块量级 10^5 下不可观察。

### 3.2 D2：索引型双链 LRU（arena + free list，O(1) touch/remove）

**C**: `older`/`newer` 侵入式指针双链（cache.h:17-18），touch O(1)。
**Rust**: `LruList { nodes: Vec<LruNode>, free: Vec<u32>, head, tail }`（page_cache.rs:102-117），条目存 `lru_node: u32` 索引（page_cache.rs:94）。`push_back`（:127）从 free list 取槽或扩容；`touch`（:135）= unlink + link_tail；`remove`（:141）摘链 + 槽回收；`iter_oldest`（:179）从最老端迭代。

**为什么不用 `Vec<CacheKey>` 扫描**（旧实现）：`touch` 在文件缺页命中路径上每次 O(n)（10^5 条目 × 每次缺页），且 `remove` 的 `retain` 也是 O(n)；C 是 O(1)。索引双链在 safe Rust 下等价复刻 C 的精确 LRU，无 unsafe。**淘汰顺序是外部可观察行为**（压力下先淘汰最老未映射页），必须等价。

**free list 不缩容**：arena 峰值 = 缓存条目峰值；条目本身随 `by_dev` 删除，内存上限由缓存大小决定（注释 page_cache.rs:93-95）。

### 3.3 D3：PBF_INCACHE / refcount 复用 PageFrames，归零经 PfnAllocator 释放

**C**: `phys_block.flags & PBF_INCACHE` + `refcount`（region.h:35）；`rmcache` 归零 → `free_mem`（cache.c:280-284）。
**Rust**: 直接复用 `PageFrames` 的 `IN_CACHE` 标志与 refcount（page_state.rs:30/:166/:175）：

- `PageCache::addcache` 的重复检查 = 帧 `is_cached()` **或** 主键已存在（page_cache.rs:246-255）——后者是 fail-closed 补充（C 的链式哈希允许重复键——调用方 bug；`CACHE_SANITY` 才抓，Rust 直接拒绝）。
- `PageCache::rmcache`（page_cache.rs:281-315）：摘三处索引 → `frames.rmcache(pfn)` → **若帧 refcount == 0 → `alloc.free_pfn(pfn)`**。与 `free_region_pages`（region/mod.rs:23-77）同一模式；旧实现永不释放（内存泄漏，24-P0-1）。
- `unmap_page`（vir_region.rs:196-223）已有"refcount==0 且 IN_CACHE 则不返回待释放页"的配合——缓存页被 unmap 后由缓存侧持有，rmcache 时统一释放。**不变量**：`IN_CACHE ⟺ PageCache.by_dev 中存在该 PFN 条目`。
- **没有独立条目 refcount**：帧 refcount 是权威（addcache +1 = 缓存引用；map_page +1 = 映射引用）。淘汰条件 `refcount == 1` 直接读帧（page_cache.rs:389-393）。

### 3.4 D4：VMSF_ONCE 条目 flag（修正 dispatch 的 request.flags 误读）

**C**: 条目 `hb->flags = flags & VMSF_ONCE`（cache.c:241）；mapcache 查**条目**（mem_cache.c:149）；setcache 跳过条件 = 同页**且旧条目非 ONCE**（mem_cache.c:240-246）。
**Rust**: `CachedPage.once: bool`（page_cache.rs:85）。

- `dispatch_mapcache`（dispatcher.rs:482-572）：删掉旧代码 `request.flags & 0x01` 检查——**C 的 do_mapcache 根本不读 `m_vmmcp.flags`**；改为查 `entry.once`（dispatcher.rs:545 `Some(entry) if !entry.once`）。24-P0-1 修正。
- `dispatch_setcache`（dispatcher.rs:607-713）：跳过条件 = `entry.pfn == pfn && !entry.once`（dispatcher.rs:660-664）——旧代码 `request.flags == 0` 读**新** flag 而非**旧条目** flag；新条目 `once = request.flags & VMSF_ONCE != 0`。24-P0-1 修正。
- `MappedFile::ev_pagefault`（memtype.rs:1006-1049）：命中条目 `once == true` → `NeedVfsIo`（强制 VFS 往返，mem_file.c:120-131 的保守侧）；旧实现无此分流。差异见 §3.6。

### 3.5 D5：延迟 ino 更新（update_inohash 等价）

`find_by_dev`（page_cache.rs:317-351）命中后，若 `ino: Some` 且与条目不符 → 删旧辅索引项 → 改条目元信息 → 写新辅索引项（page_cache.rs:334-343）。**语义**：文件系统登记时可只给磁盘块地址，后续块归属变化时 ino 信息自动补全——"先 dev 后 ino"两阶段登记的桥接（C cache.c:163-174/:183-188）。传 `None`（VMC_NO_INODE）**永不**清除已有 ino 信息（C 只在传入真实 ino 时更新，cache.c:183-184）。

### 3.6 差异清单（C ↔ Rust，诚实标注）

| # | 差异 | 说明 |
|---|------|------|
| 1 | **哈希 → BTreeMap**（`[ARCH: A-4 家族]`） | 双链哈希 → 主/辅 BTreeMap 索引；无固定 65536 桶，无哈希函数（checklist M-041/F-031 同步） |
| 2 | **侵入式指针 LRU → 索引双链** | 无 unsafe；free list 复用槽；精确 LRU 语义保留（checklist F-028-F-030 同步） |
| 3 | **重复键 fail-closed** | C 链式哈希允许重复键（sanity 才抓）；Rust `addcache` 拒绝（AlreadyCached → EINVAL） |
| 4 | **mapcache 直接映射替代 pb_cache 间接路径** | C 走 `map_pf` → `cache_pagefault`（懒映射 + 页表写入）；Rust `region.map_page` 直接链接（dispatcher.rs:549-555）——同样的 refcount 效果、同样的可观察结果；`CacheMemory::ev_pagefault`（memtype.rs:809-918）保留 PbCache 语义作为 memtype 契约的兜底 |
| 5 | **mapcache bytes 检查前置** | C 先查 endpoint（bogus source → panic）；Rust 先验 bytes（EINVAL）再查 endpoint（InvalidProcess，fail-closed）——对真实调用者无可观察差异 |
| 6 | **缺页 ONCE 分流** | C 初始缺页（无回调）命中 ONCE 会链接 + `rmcache` 用后即弃（mem_file.c:120-138）；Rust 统一走 `NeedVfsIo`（强制往返）——不消费一次性页，端状态等价（页由 FS 重提供），差一次 IPC 往返；`mappedfile_pf_cont` 的 ONCE 用后即弃依赖 transport，未接线（backlog） |
| 7 | **`find_cached_page_bypfn` 移除** | 旧双键模型的 `pfn_index` 反索引是 C 没有的发明（checklist F-039a）；单键模型下无消费者 |
| 8 | **`_MINIX_MAGIC` 分支不实现** | mem_cache.c:119-128/:135-137 的插桩预留（分配 1 页洞）仅编译宏启用；Rust 不实现（注释说明） |
| 9 | **`cache_sanitycheck_internal` 跳过** | `CACHE_SANITY=0`（vm.h:9）编译宏；Rust 用单测覆盖等价不变量 |

### 3.7 D6：alloc_cycle 补充体（cache_freepages 接线）

**C**: 主循环 `missing_spares>0 → alloc_cycle()`（main.c:118-119）；`alloc_mem` 耗尽时 `do { mem = alloc_pages(...); } while(mem == NO_MEM && cache_freepages(clicks) > 0)` 按请求页数 `clicks` 回收重试（alloc.c:260-262）。
**Rust**: `VmServer::alloc_cycle`（vm_server.rs:424-432）补体落地（plan.md §7.3 DEFERRED）：`self.page_cache.free_pages(FREE_CACHE_BATCH=1024, frames, &mut self.page_alloc)`（vm_server.rs:430）——固定批次（**近似** C 的 `cache_freepages(clicks)`：C 在分配路径按请求量回收，Rust 主循环钩子与分配路径解耦（Direct Map 消除保留队列，ARCH A-1），故用固定预算 1024 近似）；回收后清压力计数，若回收不足由下一次分配失败重新武装（每压力片段一次回收机会）。**闭环**：分配失败 → `mark_alloc_failure` → 主循环 → `alloc_cycle` → 缓存回收 → 后续分配重试。对照 Linux 的"分配失败 → 回收 → 重试"路径；Redox 无回收直接返回错误。

### 3.8 D7：错误码映射与输入验证（对齐 C）

| C 条件 | C errno | Rust |
|--------|---------|------|
| `dev_off % PAGE_SIZE \|\| ino_off % PAGE_SIZE`（mapcache/setcache/forgetcache） | EFAULT | `VmError::InvalidAddress`（vm.rs:690 → EFAULT） |
| `bytes < VM_PAGE_SIZE`（mapcache/setcache/forgetcache） | EINVAL | `VmError::InvalidParam`（vm.rs:697 → EINVAL）——**修正**：旧代码 mapcache 用 InvalidProcess、forgetcache 用 InvalidAddress |
| addcache 重复登记 / `dev == NO_DEV` | EINVAL / assert | `CacheError::AlreadyCached/InvalidParam` → `InvalidParam`（fail-closed，不 panic） |
| mapcache 未命中 / ONCE 条目 | ENOENT | `VmError::NotFound`（vm.rs:712 → ENOENT） |
| map_page_region 失败 | ENOMEM | `VmError::OutOfMemory` |
| setcache 非 anon / refcount!=1 / 无区域 | EFAULT | `VmError::InvalidAddress` |
| vm_isokendpt 失败 | panic("bogus source") | `VmError::InvalidProcess`（EINVAL，fail-closed） |

---

## 4. 实现详解

### 4.1 模块结构

```
os/libs/minix-types/src/ipc/message.rs
  └─ MessVmmcp（:1948，i386 wire 布局：dev@0 u64 / dev_offset@8 i64 / ino_offset@16 i64 /
     ino@24 u64 / block@32 u32 / flags_ptr@36 u32 / pages@40 u8 / flags@41 u8）
  └─ MessVmmcpReply（:2001，addr@0 u32 / flags@4 u8）
  └─ MessageUnion.m_vmmcp / m_vmmcp_reply（:154-157）
os/libs/minix-types/src/ipc/vm.rs
  └─ VmCacheIn::decode_message（:1053，从 m_vmmcp overlay 解码）
os/servers/vm/src/page_cache.rs
  ├─ CachedPage（:76）——条目：ino Option / ino_offset / once / pfn / lru_node
  ├─ LruList（:102）——索引双链 + free list
  └─ PageCache（:201）——by_dev 主键 + by_ino 辅索引 + lru + total_cached
os/servers/vm/src/ipc/dispatcher.rs
  ├─ dispatch_mapcache（:482）——VM_MAPCACHEPAGE
  ├─ dispatch_setcache（:607）——VM_SETCACHEPAGE
  ├─ dispatch_forgetcache（:714）——VM_FORGETCACHEPAGE
  └─ dispatch_clearcache（:740）——VM_CLEARCACHE
os/servers/vm/src/memtype.rs
  ├─ CacheMemory（:795）——cache 区域 memtype（PbCache 兜底 + 不可 resize）
  └─ MappedFile::ev_pagefault（:919+）——缺页缓存命中/ONCE 分流
os/servers/vm/src/vm_server.rs
  ├─ FREE_CACHE_BATCH（:41）——1024 页回收批次
  ├─ alloc_cycle（:424）——压力回收接线
  └─ handle_mapcache/setcache/forgetcache/clearcache（:1257-1278）
```

### 4.2 关键流程伪码

**mapcache**（dispatcher.rs:482-572）：

```
1. dev_off/ino_off 页对齐？否 → InvalidAddress
2. pages == 0 → InvalidParam
3. vm_isokendpt(caller) → caller_slot；失败 → InvalidProcess
4. find_slot(MMAP_BASE, MMAP_TOP, bytes) → vaddr；无洞 → OutOfMemory
5. region = VirRegion::with_memtype(vaddr, bytes, ANON|WRITABLE, MEM_TYPE_CACHE)
6. 逐页：
   find_by_dev(dev, dev_off+off, ino(若≠0), ino_off+off, touch=true)
   ├─ Some(entry) && !entry.once → region.map_page(pfn)   // 直接链接
   └─ 其他（miss / ONCE）→ unmap_region_pages + NotFound
7. regions_mut().insert(region) → 失败 → OutOfMemory
8. VmReply::MapCache { addr: vaddr } → encode_reply_data 写 m_vmmcp_reply.addr
```

**setcache**（dispatcher.rs:607-713）：

```
1. pages == 0 → InvalidParam；对齐 → InvalidAddress
2. vm_isokendpt → caller_slot
3. 逐页：
   a. map_lookup(block+off) → 无区域/无槽 → InvalidAddress
   b. find_by_dev(dev, dev_off+off, ino(若≠0), ino_off+off, touch=true)
      ├─ Some(entry) && entry.pfn == pfn && !entry.once → continue
      └─ Some(entry)（过时）→ rmcache
   c. memtype 必须是 MEM_TYPE_ANON / MEM_TYPE_CONTIG_ANON（静态指针比较）→ 否则 InvalidAddress
   d. 帧 refcount != 1 → InvalidAddress
   e. slot.set_memtype(Some(MEM_TYPE_CACHE))（三态 PageSlot 的 set_memtype，仅 present 槽生效）
   f. addcache(dev, off, ino, ino_off, flags & VMSF_ONCE, pfn) → Err → InvalidParam
```

### 4.3 本轮修复记录（2026-08-16）

| ID | 级别 | 内容 |
|----|------|------|
| 24-P0-1 | P0 | **wire format**：`VmCacheIn::decode` 从 `MessageM1` 读（dev 取自 m1p1@16、ino/ino_offset/block 硬编码 0）→ 全部字段错位；新增 `MessVmmcp`/`MessVmmcpReply` overlay（message.rs:1948/:2001）+ `decode_message`（vm.rs:1053）+ 回复编码（vm_server.rs:1123）+ 解码测试（vm.rs `test_vm_cache_in_decode_message`）。23-P0-1 同族 |
| 24-P0-2 | P0 | **双键模型缺陷**：`CacheKey::ByInode/ByDevice` 允许同一 PFN 双条目、无 IN_CACHE 检查、bydev 查找漏查 ino 条目、无延迟 ino 更新 → 重写为单键模型（D1）+ 惰性更新（D5） |
| 24-P0-3 | P0 | **VMSF_ONCE 误读**：`dispatch_mapcache` 查 `request.flags & 0x01`（C 查条目 `hb->flags`）、`dispatch_setcache` 跳过条件读新 flag → 条目 `once` 语义（D4） |
| 24-P0-4 | P0 | **缓存页永不归还**：旧 `remove` 只 `frames.rmcache` 不释放归零页（C cache.c:280-284 `free_mem`）→ `rmcache` 归零经 `PfnAllocator::free_pfn`（D3）；旧 `increase_refcount` 与 `map_page` 双重计数 → 删除（帧 refcount 权威） |
| 24-P1-1 | P1 | **errno 映射**：mapcache `bytes<PAGE_SIZE` 用 InvalidProcess、forgetcache `pages==0` 用 InvalidAddress → 统一 `InvalidParam`（EINVAL，D7） |
| 24-P1-2 | P1 | **anon 判断**：setcache 的 memtype 检查用名字字符串（`"anonymous"` 与实际 `"anonymous memory"` 不符）→ 静态指针比较（dispatcher.rs:672-676） |
| 24-P1-3 | P1 | **alloc_cycle 补体**（plan.md §7.3 DEFERRED 落地）：`free_pages(1024)` 接线（vm_server.rs:424-432）+ `FREE_CACHE_BATCH` 常量 |
| 24-P2-1 | P2 | checklist 缓存行同步（M-034/M-041/G-027/G-028/S-001/F-028~F-041）；F-039a `find_by_pfn` 移除标注 |
| 24-P2-2 | P2 | 新增测试：page_cache 重写 10 个 + memtype ONCE 分流 1 个 + wire 解码 1 个；测试总数 406 → 407（minix-vm lib，净 +1；`test_map_lazy` pre-existing 失败保持）+ minix-types 85 → 86 |

### 4.4 与 12/15/16/23 的关系

- **12-memtype**：`CacheMemory`（memtype.rs:795）是 cache 区域的 memtype 契约；本篇实现其消费方（mapcache 直接映射 + PbCache 兜底）。
- **15-ipc-dispatch**：4 个 CALLMAP 分支（main.c:569-572）在 Rust 的分发入口（dispatcher.rs:1018-1024）；SUSPEND 不涉及缓存 handler（全同步）。
- **16-pagefault**：文件缺页的缓存命中路径由 `MappedFile::ev_pagefault` 消费（memtype.rs:1006+），ONCE 分流与 `cow_block` 的 clearend 清零在 16/17 侧。
- **23-vfs-interaction**：VFS 回复后把页写进缓存（`vm_map_cacheblock`），重试缺页命中缓存——本篇是那条链路的"缓存侧"；23 篇只消费 `find_by_*`（旧 API 已于本轮同步为新 API）。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

**page_cache.rs**（10 个）：

| 测试 | 行 | 覆盖 |
|------|-----|------|
| `test_addcache_and_rmcache` | 476 | 登记/撤销 + 缓存引用 + 归零还页 |
| `test_addcache_rejects_duplicate_block` | 496 | 帧 IN_CACHE / 重复键拒绝 |
| `test_addcache_rejects_no_device` | 519 | NO_DEV fail-closed |
| `test_find_by_dev_and_ino` | 530 | 双键查找 + dev 消歧 + 延迟 ino 更新 |
| `test_find_by_dev_updates_stale_ino` | 556 | update_inohash 语义 |
| `test_lru_touch_orders_eviction` | 580 | touch 改变淘汰顺序 |
| `test_free_pages_skips_mapped_frames` | 601 | 淘汰只碰 refcount==1 |
| `test_clear_by_dev` | 627 | 按设备清除 + 计数 |
| `test_rmcache_keeps_mapped_page_alive` | 648 | 有映射时 rmcache 不还页 |
| `test_find_miss` | 666 | 双键 miss 返回 None（未登记/错键） |

**memtype.rs**（MappedFile 缺页，4 个 + 本轮 1 个新增）：

| 测试 | 行 | 覆盖 |
|------|-----|------|
| `test_mapped_file_pagefault_uninitialized_need_new_page` | 1236 | 未初始化 → NeedNewPage |
| `test_mapped_file_pagefault_cache_miss_need_vfs_io` | 1271 | 未命中 → NeedVfsIo |
| `test_mapped_file_pagefault_cache_hit_links_page` | 1301 | 命中 → 链接 + 帧 refcount 2 |
| `test_mapped_file_pagefault_device_cache_hit` | 1342 | VMC_NO_INODE 走 bydev |
| `test_mapped_file_pagefault_one_shot_hit_forces_vfs_io` | 1377 | **本轮新增**：ONCE 命中 → NeedVfsIo |

**cow_exec_pf.rs**（1 个，缓存命中循环终止）：`test_handle_pagefault_retry_cache_hit_no_fdio_loop`（:537）——回复 → 重试 → 命中 → 无第二次 FDIO。

**ipc/dispatcher.rs**（7 个）：`test_dispatch_forgetcache_*`（:1755-1816）/ `test_dispatch_setcache_*`（:1818-1907）/ `test_dispatch_mapcache_*`（:1910-1993）——错误路径（pages==0 → InvalidParam、未对齐 → InvalidAddress、无效 caller → InvalidProcess、miss → NotFound）。

**minix-types**（1 个，本轮新增）：`test_vm_cache_in_decode_message`（vm.rs）——wire 布局锁定 + 负 dev_offset 环绕。

### 5.2 覆盖维度

- **数据结构**：登记/撤销/重复拒绝/双键查找/延迟更新/设备消歧/miss（page_cache 10 个）。
- **LRU**：touch 顺序、最老端淘汰、跳过映射页（page_cache 3 个）。
- **refcount 生命周期**：缓存引用、映射引用、归零还页、有映射不还页（page_cache 4 个）。
- **缺页分流**：命中/未命中/设备页/ONCE 强制往返（memtype 4 个 + cow_exec_pf 1 个）。
- **IPC 错误路径**：4 个 handler 的验证顺序与 errno 映射（dispatcher 7 个）。
- **wire format**：m_vmmcp 布局 + 环绕（minix-types 1 个）。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 |
|------|------|
| `dispatch_mapcache`/`dispatch_setcache` 成功路径端到端（需进程槽 + 匿名区域构造） | 未覆盖（dispatcher 测试只能到 endpoint 验证——全局进程表无活动进程）；成功路径由 page_cache + memtype 单测覆盖 |
| `cache_sanitycheck_internal`（CACHE_SANITY 宏） | 跳过（cfg 门控，vm.h:9） |
| ONCE 页缺页命中后的 `rmcache`（mem_file.c:134-138） | 未接线（Rust 统一走 NeedVfsIo，见 §3.6 差异 6；transport 缺口） |
| clearend 尾清零（cow_block） | 未覆盖（17 篇范围，backlog） |
| 真实 VFS 往返（FS 读盘 → setcache → 缺页命中） | 未覆盖（transport 缺口，23 篇同） |

### 5.4 测试统计（截至 2026-08-16）

- `cargo test -p minix-vm --lib`：**407 passed / 1 failed**（`test_map_lazy` pre-existing，13 篇 backlog；本篇无关）
- `cargo test -p minix-types`：**86 passed**
- 本轮新增：page_cache 重写 10 个（旧 10 个全部改名重写）+ memtype 新增 1 个 + wire 新增 1 个 = 12 个新测试函数；minix-vm lib 406 → 407（净 +1：page_cache 旧 10 → 新 10 净 0，memtype 新增 1）
- 完整测试清单：`rg "^\s*fn test_" os/servers/vm/src/{page_cache,memtype,cow_exec_pf}.rs os/servers/vm/src/ipc/dispatcher.rs os/libs/minix-types/src/ipc/vm.rs`

---

## 6. 过渡

### 6.1 位置可回答性

**init_vm 阶段**：页缓存无显式初始化（`PageCache::new()` 空目录，vm_server.rs:127）——缓存是**运行时填充**的结构，不参与启动自举。

**主循环阶段**（main.c:112-190）：

```
主循环
  ├─ missing_spares > 0 → alloc_cycle()      ← 本篇 §3.7：缓存回收接线
  ├─ VM_MAPCACHEPAGE → dispatch_mapcache     ← 本篇
  ├─ VM_SETCACHEPAGE → dispatch_setcache     ← 本篇
  ├─ VM_FORGETCACHEPAGE → dispatch_forgetcache ← 本篇
  ├─ VM_CLEARCACHE → dispatch_clearcache     ← 本篇
  └─ 缺页（MappedFile）→ 缓存命中/ONCE 分流   ← 本篇 §3.4/§3.6
```

**与相邻文档**：缓存命中路径被 16/23 消费；`alloc_cycle` 回收是 06 篇 §7.3 的 DEFERRED 体落地；`CacheMemory` memtype 是 12 篇的消费侧。

### 6.2 下游移交（对照 plan.md §3.4 第 24 行）

- **前置依赖**: 12（memtype）/23（VFS 异步对话、mappedfile 消费缓存命中）
- **本篇职责**: `cache.c`+`mem_cache.c` 全部、双键索引、LRU、4 个 cache IPC handler
- **不覆盖（移交）**: VFS 请求队列（23）；下一篇 **25-rs-services**（RS 握手，Live Update 的缓存一致性边界在 25 篇处理）

---

## 7. 参见

| 文档 | 关系 |
|------|------|
| `12-memtype.md` | CacheMemory memtype 契约（PbCache 兜底 / 不可 resize） |
| `15-ipc-dispatch.md` | 4 个缓存 CALLMAP 分支的分发框架 |
| `16-pagefault.md` / `17-cow-mechanism.md` | 文件缺页缓存命中消费侧 / cow_block |
| `23-vfs-interaction.md` | VFS 回复 → 缓存填充 → 重试命中的链路（23 篇 §4.4） |
| `05-physical-memory.md` / `06-page-allocator.md` | 物理分配器 / alloc_cycle 压力回收（06 §7.3 DEFERRED 落地） |
| `25-rs-services.md` | 下一篇：RS 服务（Live Update） |
| `99-global-concepts.md` | 常量表（VMC_NO_INODE / VMSF_ONCE） |
