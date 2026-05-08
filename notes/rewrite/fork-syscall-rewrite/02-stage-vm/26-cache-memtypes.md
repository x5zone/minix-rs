# 26-cache-memtypes: 缓存、共享内存、连续内存

> **分类**: MemType 补全
> **源码**: `minix3/minix/servers/vm/cache.c`, `mem_cache.c`, `mem_shared.c`, `mem_anon_contig.c`
> **Rust 对应**: `memtype.rs` 补全 + `cache.rs` 新增
> **说明**: 补全四种特殊 memtype：Cache、Shared、AnonContig、MappedFile（已在24中设计）

---

## 1. 概述

### 1.1 六种 MemType 的完整图景

| MemType | 名称 | 用途 | 缺页行为 | CoW |
|---------|------|------|---------|-----|
| `mem_type_anon` | 匿名内存 | 堆、栈、普通 mmap | 分配新物理页 | ✅ 支持 |
| `mem_type_anon_contig` | 连续匿名内存 | DMA 缓冲区 | ❌ 不可能（预分配） | ❌ 禁止 |
| `mem_type_direct` | 直接物理映射 | 设备寄存器映射 | 计算物理地址 | ❌ 禁止 |
| `mem_type_cache` | 缓存内存 | 文件系统页缓存 | 从缓存链接物理块 | ❌ 禁止 |
| `mem_type_shared` | 共享内存 | 进程间共享 | 从源区域链接物理块 | ❌ 禁止 |
| `mem_type_mappedfile` | 文件映射 | mmap 文件 | VFS 读取 + 缓存 | ✅ 写入时 CoW |

### 1.2 本文档覆盖范围

```
┌─────────────────────────────────────────────┐
│           MemType 继承/依赖关系               │
│                                             │
│  mem_type_anon (基础)                        │
│    ├── mem_type_anon_contig (预分配版)        │
│    │     └── on_new: alloc_mem(pages)        │
│    │     └── pagefault: panic!               │
│    │     └── reference: ENOMEM (禁止 fork)    │
│    │                                         │
│    ├── mem_type_cache (缓存版)               │
│    │     └── pagefault: 从 pb_cache 链接      │
│    │     └── 依赖 cache.c 数据结构            │
│    │                                         │
│    └── mem_type_shared (共享版)              │
│          └── pagefault: 从源区域链接           │
│          └── 依赖 getsrc() 查找源进程          │
│                                             │
│  mem_type_direct (独立)                      │
│  mem_type_mappedfile (已在24中设计)           │
└─────────────────────────────────────────────┘
```

---

## 2. C 源码分析：cache.c — 页缓存数据结构

### 2.1 cached_page 结构

```c
/* cache.h */
struct cached_page {
    dev_t dev;              /* 设备号（必须有效，不能是 NO_DEV） */
    u64_t dev_offset;       /* 设备内偏移 */
    ino_t ino;              /* inode 号（可能未知 = VMC_NO_INODE） */
    u64_t ino_offset;       /* inode 内偏移 */
    int flags;              /* VMSF_ONCE 或 0 */
    struct phys_block *page; /* 指向物理块 */
    struct cached_page *older; /* LRU 链表：更老 */
    struct cached_page *newer; /* LRU 链表：更新 */
    struct cached_page *hash_next_dev;  /* 设备哈希链 */
    struct cached_page *hash_next_ino;  /* inode 哈希链 */
};
```

**关键设计**：
- **双哈希索引**：按 `(dev, dev_offset)` 和 `(ino, ino_offset)` 双重索引
- **LRU 淘汰**：双向链表维护访问顺序，最老的在最前面
- **引用计数集成**：缓存页引用 `phys_block`，`refcount++`；淘汰时 `refcount--`

### 2.2 双哈希表

```c
#define HASHSIZE 65536

static struct cached_page *cache_hash_bydev[HASHSIZE];  /* 按 (dev, dev_offset) */
static struct cached_page *cache_hash_byino[HASHSIZE];  /* 按 (ino, ino_offset) */
```

**查找逻辑**：

```
find_cached_page_bydev(dev, dev_offset, ino, ino_offset, touchlru)
  │
  ├── 在 cache_hash_bydev[hash(dev, dev_offset)] 中线性搜索
  │   匹配条件: cp->dev == dev && cp->dev_offset == dev_offset
  │
  ├── 如果找到且 ino != VMC_NO_INODE:
  │   └── 更新 inode 信息 (update_inohash)
  │
  └── 如果 touchlru: cache_lru_touch(cp)  /* 移到 LRU 最新端 */

find_cached_page_byino(dev, ino, ino_offset, touchlru)
  │
  ├── 在 cache_hash_byino[hash(ino, ino_offset)] 中线性搜索
  │   匹配条件: cp->dev == dev && cp->ino == ino && cp->ino_offset == ino_offset
  │
  └── 如果 touchlru: cache_lru_touch(cp)
```

### 2.3 LRU 淘汰

```c
static struct cached_page *lru_oldest = NULL, *lru_newest = NULL;

/* 添加到 LRU 最新端 */
static void lru_add(struct cached_page *hb) {
    hb->older = lru_newest;
    hb->newer = NULL;
    if(lru_newest) lru_newest->newer = hb;
    else lru_oldest = hb;
    lru_newest = hb;
    cached_pages++;
}

/* 从 LRU 中移除 */
static void lru_rm(struct cached_page *hb) {
    // 双向链表删除...
    cached_pages--;
}

/* 内存不足时回收缓存页 */
int cache_freepages(int pages) {
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

**淘汰条件**：`refcount == 1`（只被缓存自身引用，没有进程映射此页）

### 2.4 addcache / rmcache

```c
int addcache(dev_t dev, u64_t dev_off, ino_t ino, u64_t ino_off,
    int flags, struct phys_block *pb)
{
    struct cached_page *hb = SLABALLOC(hb);  /* 从 slab 分配 */

    hb->dev = dev;
    hb->dev_offset = dev_off;
    hb->ino = ino;
    hb->ino_offset = ino_off;
    hb->flags = flags & VMSF_ONCE;
    hb->page = pb;
    hb->page->refcount++;       /* 缓存引用 +1 */
    hb->page->flags |= PBF_INCACHE;

    /* 加入设备哈希 */
    int hv_dev = makehash(dev, dev_off);
    hb->hash_next_dev = cache_hash_bydev[hv_dev];
    cache_hash_bydev[hv_dev] = hb;

    /* 如果有 inode 信息，加入 inode 哈希 */
    if(hb->ino != VMC_NO_INODE)
        addcache_byino(hb);

    lru_add(hb);
    return OK;
}

void rmcache(struct cached_page *cp) {
    struct phys_block *pb = cp->page;

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

---

## 3. C 源码分析：mem_cache.c — 缓存 MemType

### 3.1 mem_type_cache 方法表

```c
struct mem_type mem_type_cache = {
    .name = "cache memory",
    .ev_reference = cache_reference,       /* OK（不需要 CoW） */
    .ev_unreference = cache_unreference,   /* 委托给 anon */
    .ev_resize = cache_resize,             /* ENOMEM（不可调整大小） */
    .ev_lowshrink = cache_lowshrink,       /* OK（空操作） */
    .ev_sanitycheck = cache_sanitycheck,
    .ev_pagefault = cache_pagefault,       /* 从 pb_cache 链接 */
    .writable = cache_writable,
    .pt_flags = cache_pt_flags,
};
```

### 3.2 cache_pagefault — 缺页处理

```c
static int cache_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb,
    void *state, int len, int *io)
{
    vir_bytes offset = ph->offset;
    assert(ph->ph->phys == MAP_NONE);           /* 物理页尚未分配 */
    assert(region->param.pb_cache);              /* 有缓存物理块 */

    /* 解除旧的空物理块引用 */
    pb_unreferenced(region, ph, 0);
    /* 将物理区域链接到缓存物理块 */
    pb_link(ph, region->param.pb_cache, offset, region);
    /* 清除缓存指针（一次性使用） */
    region->param.pb_cache = NULL;

    return OK;
}
```

**关键**：缓存缺页不需要分配新物理页——它直接链接到已存在的缓存物理块。`param.pb_cache` 是一个"预加载"指针，在 `do_mapcache` 中设置。

### 3.3 do_mapcache — 映射缓存块

```c
int do_mapcache(message *msg)
{
    dev_t dev = msg->m_vmmcp.dev;
    uint64_t dev_off = msg->m_vmmcp.dev_offset;
    phys_bytes bytes = msg->m_vmmcp.pages * VM_PAGE_SIZE;

    /* 1. 创建虚拟区域（cache memtype） */
    vr = map_page_region(caller, VM_MMAPBASE, VM_MMAPTOP,
        bytes, VR_ANON | VR_WRITABLE, 0, &mem_type_cache);

    /* 2. 逐页查找缓存并映射 */
    for(offset = 0; offset < bytes; offset += VM_PAGE_SIZE) {
        hb = find_cached_page_bydev(dev, dev_off + offset, ...);
        if(!hb || (hb->flags & VMSF_ONCE)) {
            /* 缓存未命中或一次性标志 → 失败 */
            map_unmap_region(caller, vr, 0, bytes);
            return ENOENT;
        }

        /* 设置预加载指针 */
        vr->param.pb_cache = hb->page;

        /* 触发缺页 → cache_pagefault 从 pb_cache 链接 */
        map_pf(caller, vr, offset, 1, NULL, NULL, 0, &io);
        assert(!vr->param.pb_cache);  /* 已被清除 */
    }

    /* 3. 返回虚拟地址 */
    msg->m_vmmcp_reply.addr = (void *) vr->vaddr;
    return OK;
}
```

### 3.4 do_setcache — 注册缓存块

```c
int do_setcache(message *msg)
{
    /* 逐页处理 */
    for(offset = 0; offset < bytes; offset += VM_PAGE_SIZE) {
        vir_bytes v = (vir_bytes) msg->m_vmmcp.block + offset;

        /* 1. 查找调用者地址空间中的物理区域 */
        region = map_lookup(caller, v, &phys_region);

        /* 2. 检查是否已有缓存项 */
        hb = find_cached_page_bydev(dev, dev_off + offset, ...);
        if(hb) {
            if(hb->page != phys_region->ph || (hb->flags & VMSF_ONCE)) {
                rmcache(hb);  /* 旧缓存项已过时 */
            } else {
                continue;     /* 缓存项仍有效 */
            }
        }

        /* 3. 将物理区域的 memtype 从 anon 改为 cache */
        phys_region->memtype = &mem_type_cache;

        /* 4. 添加到缓存 */
        addcache(dev, dev_off + offset, msg->m_vmmcp.ino,
            ino_off + offset, flags, phys_region->ph);
    }
    return OK;
}
```

**核心操作**：`phys_region->memtype = &mem_type_cache` — 将匿名内存的 memtype **就地切换**为缓存类型。

---

## 4. C 源码分析：mem_shared.c — 共享内存

### 4.1 mem_type_shared 方法表

```c
struct mem_type mem_type_shared = {
    .name = "shared memory",
    .ev_copy = shared_copy,             /* 复制共享参数 */
    .ev_unreference = shared_unreference, /* 委托给 anon */
    .ev_pagefault = shared_pagefault,   /* 从源区域链接 */
    .ev_sanitycheck = shared_sanitycheck,
    .ev_delete = shared_delete,         /* 减少源 remaps */
    .regionid = shared_regionid,        /* 返回源区域 id */
    .refcount = shared_refcount,        /* 1 + remaps */
    .writable = shared_writable,
    .pt_flags = shared_pt_flags,
};
```

### 4.2 共享内存的源区域模型

```
进程 A (源)                          进程 B (共享者)
┌──────────────────┐                ┌──────────────────┐
│ VirRegion (anon)  │                │ VirRegion (shared)│
│ vaddr: 0x400000   │◄───────────────│ vaddr: 0x600000   │
│ remaps: 1         │  param.shared: │ param.shared:     │
│                    │  ep = A        │   ep = A          │
│ PhysRegion         │  vaddr=0x400000│  vaddr=0x400000   │
│ ┌──┬──┬──┬──┐     │                │ ┌──┬──┬──┬──┐     │
│ │P1│P2│P3│P4│     │                │ │P1│P2│P3│P4│     │
│ └──┴──┴──┴──┘     │                │ └──┴──┴──┴──┘     │
│   ↑ 共享 PhysBlock  ↑              │   ↑ 同一个 PhysBlock│
└──────────────────┘                └──────────────────┘
```

**关键**：共享区域的 `param.shared` 记录源进程的 `(endpoint, vaddr, id)`，缺页时通过 `getsrc()` 找到源区域的 PhysBlock，直接链接（`pb_link`），实现物理页共享。

### 4.3 shared_pagefault — 缺页处理

```c
static int shared_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb,
    void *state, int statelen, int *io)
{
    struct vir_region *src_region;
    struct vmproc *src_vmp;
    struct phys_region *pr;

    /* 1. 查找源区域 */
    if(getsrc(region, &src_vmp, &src_region) != OK)
        return EINVAL;

    /* 2. 如果物理页已存在，无需处理 */
    if(ph->ph->phys != MAP_NONE)
        return OK;

    /* 3. 释放旧的空 PhysBlock */
    pb_free(ph->ph);

    /* 4. 在源区域中查找对应偏移的 PhysRegion */
    if(!(pr = physblock_get(src_region, ph->offset))) {
        /* 源区域也缺页 → 先处理源区域的缺页 */
        map_pf(src_vmp, src_region, ph->offset, write, NULL, NULL, 0, io);
        pr = physblock_get(src_region, ph->offset);
    }

    /* 5. 链接到源区域的 PhysBlock（共享！） */
    pb_link(ph, pr->ph, ph->offset, region);

    return OK;
}
```

### 4.4 getsrc — 查找源区域

```c
static int getsrc(struct vir_region *region,
    struct vmproc **vmp, struct vir_region **r)
{
    int srcproc;

    /* 从 param.shared 获取源信息 */
    if(!region->param.shared.ep || !region->param.shared.vaddr)
        return EINVAL;

    /* 通过 endpoint 查找源进程 */
    vm_isokendpt((endpoint_t) region->param.shared.ep, &srcproc);
    *vmp = &vmproc[srcproc];

    /* 在源进程地址空间中查找源区域 */
    *r = map_lookup(*vmp, region->param.shared.vaddr, NULL);

    /* 验证源区域是匿名内存 */
    if((*r)->def_memtype != &mem_type_anon)
        return EINVAL;

    /* 验证 id 匹配 */
    if(region->param.shared.id != (*r)->id)
        return EINVAL;

    return OK;
}
```

### 4.5 shared_setsource / shared_copy

```c
void shared_setsource(struct vir_region *vr, endpoint_t ep,
    struct vir_region *src_vr)
{
    vr->param.shared.ep = ep;
    vr->param.shared.vaddr = src_vr->vaddr;
    vr->param.shared.id = src_vr->id;

    /* 验证源区域可达 */
    getsrc(vr, &vmp, &srcvr);
    assert(srcvr == src_vr);

    /* 增加源区域的 remaps 计数 */
    srcvr->remaps++;
}

static int shared_copy(struct vir_region *vr, struct vir_region *newvr)
{
    /* fork 时复制共享参数 */
    shared_setsource(newvr, vr->param.shared.ep, srcvr);
    return OK;
}
```

---

## 5. C 源码分析：mem_anon_contig.c — 连续匿名内存

### 5.1 mem_type_anon_contig 方法表

```c
struct mem_type mem_type_anon_contig = {
    .name = "anonymous memory (physically contiguous)",
    .ev_new = anon_contig_new,           /* 预分配所有物理页 */
    .ev_reference = anon_contig_reference, /* ENOMEM（禁止 fork） */
    .ev_unreference = anon_contig_unreference, /* 委托给 anon */
    .ev_pagefault = anon_contig_pagefault,  /* panic! */
    .ev_resize = anon_contig_resize,     /* ENOMEM（不可调整大小） */
    .ev_split = anon_contig_split,       /* 空操作 */
    .ev_sanitycheck = anon_contig_sanitycheck, /* 委托给 anon */
    .writable = anon_contig_writable,    /* 委托给 anon */
    .pt_flags = anon_contig_pt_flags,    /* ARM: DEVICE 标志 */
};
```

### 5.2 anon_contig_new — 预分配

```c
static int anon_contig_new(struct vir_region *region)
{
    u32_t allocflags = vrallocflags(region->flags);
    phys_bytes pages = region->length / VM_PAGE_SIZE;

    /* 1. 为每一页创建空的 PhysBlock + PhysRegion */
    for(p = 0; p < pages; p++) {
        struct phys_block *pb = pb_new(MAP_NONE);
        pr = pb_reference(pb, p * VM_PAGE_SIZE, region, &mem_type_anon_contig);
    }

    /* 2. 一次性分配连续物理内存 */
    new_page_cl = alloc_mem(pages, allocflags);  /* 连续！ */
    cur_ph = CLICK2ABS(new_page_cl);

    /* 3. 将连续物理地址分配给各 PhysBlock */
    for(p = 0; p < pages; p++) {
        pr = physblock_get(region, p * VM_PAGE_SIZE);
        pr->ph->phys = cur_ph + pr->offset;
    }

    return OK;
}
```

**与普通匿名内存的区别**：
- 普通 `anon_new`：不预分配，缺页时逐页分配
- `anon_contig_new`：立即分配，且使用 `alloc_mem(pages)` 一次性分配连续页

### 5.3 限制

| 操作 | 行为 | 原因 |
|------|------|------|
| 缺页 | `panic!` | 物理页已预分配，不应发生 |
| fork | `ENOMEM` | 连续物理内存无法 CoW |
| resize | `ENOMEM` | 连续内存无法扩展 |
| split | 空操作 | 不需要特殊处理 |

---

## 6. Rust 设计

### 6.1 现有 Rust 代码状态

| 组件 | 状态 | 说明 |
|------|------|------|
| `AnonymousMemory` | ✅ 已实现 | 基本功能完整 |
| `DirectPhysical` | ✅ 已实现 | 基本功能完整 |
| `SharedMemory` | ⚠️ 骨架 | 缺少 `on_pagefault`、`on_copy` 的实际逻辑 |
| `ContiguousAnonymous` | ❌ 不存在 | 需要新增 |
| `CacheMemory` | ❌ 不存在 | 需要新增 |
| `MappedFile` | ❌ 不存在 | 24-vfs-interaction 中已设计 |
| `PageCache` 数据结构 | ❌ 不存在 | 需要新增 |
| `VrParam::PbCache` | ✅ 已存在 | 缓存预加载指针 |

### 6.2 PageCache — 页缓存数据结构

```rust
use alloc::boxed::Box;
use alloc::vec::Vec;
use std::collections::HashMap;

const CACHE_HASH_SIZE: usize = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    pub dev: u64,
    pub dev_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct InodeKey {
    pub ino: u64,
    pub ino_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheFlags {
    None,
    Once,
}

pub(crate) struct CachedPage {
    pub dev: u64,
    pub dev_offset: u64,
    pub ino: u64,
    pub ino_offset: u64,
    pub flags: CacheFlags,
    pub page: Arc<PhysBlock>,
}

pub(crate) struct PageCache {
    by_dev: HashMap<CacheKey, Box<CachedPage>>,
    by_ino: HashMap<InodeKey, Box<CachedPage>>,
    lru: Vec<CacheKey>,
    total_cached: usize,
}

impl PageCache {
    pub(crate) fn new() -> Self {
        Self {
            by_dev: HashMap::new(),
            by_ino: HashMap::new(),
            lru: Vec::new(),
            total_cached: 0,
        }
    }

    pub(crate) fn find_by_dev(
        &mut self, dev: u64, dev_offset: u64,
    ) -> Option<&CachedPage> {
        let key = CacheKey { dev, dev_offset };
        self.by_dev.get(&key).map(|b| b.as_ref())
    }

    pub(crate) fn find_by_dev_mut(
        &mut self, dev: u64, dev_offset: u64,
    ) -> Option<&mut CachedPage> {
        let key = CacheKey { dev, dev_offset };
        self.by_dev.get_mut(&key).map(|b| b.as_mut())
    }

    pub(crate) fn add(
        &mut self, dev: u64, dev_offset: u64,
        ino: u64, ino_offset: u64,
        flags: CacheFlags, page: Arc<PhysBlock>,
    ) -> Result<(), MemTypeError> {
        let key = CacheKey { dev, dev_offset };

        if self.by_dev.contains_key(&key) {
            return Err(MemTypeError::InvalidParam);
        }

        let cp = Box::new(CachedPage {
            dev, dev_offset, ino, ino_offset, flags, page,
        });

        self.by_dev.insert(key, cp);

        self.lru.push(key);
        self.total_cached += 1;

        Ok(())
    }

    pub(crate) fn remove(&mut self, dev: u64, dev_offset: u64) -> Option<Arc<PhysBlock>> {
        let key = CacheKey { dev, dev_offset };
        let cp = self.by_dev.remove(&key)?;

        if cp.ino != VMC_NO_INODE {
            let ino_key = InodeKey { ino: cp.ino, ino_offset: cp.ino_offset };
            self.by_ino.remove(&ino_key);
        }

        self.lru.retain(|k| k != &key);
        self.total_cached -= 1;

        Some(cp.page)
    }

    /// 内存不足时回收缓存页
    pub(crate) fn free_pages(
        &mut self,
        needed: usize,
        page_alloc: &mut VmPageAllocator,
    ) -> usize {
        let mut freed = 0;
        let mut keys_to_remove = Vec::new();

        for key in &self.lru {
            if freed >= needed { break; }
            if let Some(cp) = self.by_dev.get(key) {
                if Arc::strong_count(&cp.page) == 1 {
                    keys_to_remove.push(*key);
                    freed += 1;
                }
            }
        }

        for key in keys_to_remove {
            self.remove(key.dev, key.dev_offset);
        }

        freed
    }

    pub(crate) fn clear_by_dev(&mut self, dev: u64) {
        let keys: Vec<CacheKey> = self.by_dev.keys()
            .filter(|k| k.dev == dev)
            .copied()
            .collect();
        for key in keys {
            self.remove(key.dev, key.dev_offset);
        }
    }
}
```

**与 Minix3 的差异**：

| 方面 | Minix3 | Rust |
|------|--------|------|
| 哈希表 | 开链法静态数组 | `HashMap` |
| LRU | 双向链表 | `Vec`（简化版） |
| 物理块引用 | `refcount++` 手动管理 | `Arc<PhysBlock>` 自动管理 |
| 内存分配 | `SLABALLOC` | `Box::new` |
| 线性搜索 | 哈希冲突时线性搜索 | `HashMap` 内部处理 |

### 6.3 CacheMemory — 缓存 MemType

```rust
pub(crate) struct CacheMemory;

impl MemType for CacheMemory {
    fn name(&self) -> &'static str {
        "cache memory"
    }

    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        pr: &mut crate::region::PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        match &region.param {
            VrParam::PbCache { pb: Some(cached_pb) } => {
                let cached = unsafe { &**cached_pb };
                if cached.phys == PhysBlock::MAP_NONE {
                    return Err(MemTypeError::InvalidParam);
                }

                pr.link_to(cached_pb);

                region.param = VrParam::PbCache { pb: None };

                Ok(PagefaultResult::Handled)
            }
            _ => Err(MemTypeError::InvalidParam),
        }
    }

    fn is_writable(&self, pr: &crate::region::PhysRegion) -> bool {
        pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE
    }

    fn on_unreference(&self, pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        let refcount = pr.get_refcount().unwrap_or(0);
        if refcount == 0 && pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn on_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }
}
```

### 6.4 ContiguousAnonymous — 连续匿名内存

```rust
pub(crate) struct ContiguousAnonymous;

impl MemType for ContiguousAnonymous {
    fn name(&self) -> &'static str {
        "anonymous memory (physically contiguous)"
    }

    fn on_new(
        &self,
        region: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        let pages = region.length.get() / 4096;
        if pages == 0 {
            return Err(MemTypeError::InvalidParam);
        }

        Ok(())
    }

    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _pr: &mut crate::region::PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        panic!("contiguous anonymous pagefault: impossible, pages are pre-allocated");
    }

    fn on_reference(
        &self,
        _src: &crate::region::PhysRegion,
        _dst: &mut crate::region::PhysRegion,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    fn on_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    fn on_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) {
    }

    fn is_writable(&self, pr: &crate::region::PhysRegion) -> bool {
        if let Some(parent) = pr.parent {
            unsafe {
                if (*parent.as_ptr()).remaps > 0 {
                    return true;
                }
            }
        }
        pr.get_refcount().unwrap_or(0) == 1
            && pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE
    }

    fn on_unreference(&self, pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        let refcount = pr.get_refcount().unwrap_or(0);
        if refcount == 0 && pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
```

### 6.5 SharedMemory 补全

现有 `SharedMemory` 骨架需要补全 `on_pagefault` 和 `on_copy`：

```rust
impl MemType for SharedMemory {
    fn name(&self) -> &'static str {
        "shared memory"
    }

    fn on_pagefault(
        &self,
        proc: &ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        pr: &mut crate::region::PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE {
            return Ok(PagefaultResult::Handled);
        }

        let (src_ep, src_vaddr, src_id) = match &region.param {
            VrParam::Shared { ep, vaddr, id } => (*ep, *vaddr, *id),
            _ => return Err(MemTypeError::InvalidParam),
        };

        if src_ep == 0 || src_vaddr.get() == 0 {
            return Err(MemTypeError::InvalidParam);
        }

        // 查找源进程和源区域
        // let src_proc = proc.table().find_by_endpoint(src_ep)?;
        // let src_region = src_proc.regions().find(src_vaddr)?;
        // 验证 src_region.def_memtype == AnonymousMemory
        // 验证 src_region.id == src_id

        // 在源区域中查找对应偏移的 PhysBlock
        // let src_pr = src_region.physblock_get(pr.offset)?;
        // 如果源区域也缺页 → 先处理源区域
        // 链接到源区域的 PhysBlock
        // pr.link_to(src_pr.physblock());

        Ok(PagefaultResult::Handled)
    }

    fn on_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        // 需要增加源区域的 remaps
        Ok(())
    }

    fn on_delete(&self, region: &mut crate::region::VirRegion) {
        // 需要减少源区域的 remaps
    }

    fn is_writable(&self, pr: &crate::region::PhysRegion) -> bool {
        pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE
    }

    fn on_unreference(&self, _pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }

    fn region_id(&self, region: &crate::region::VirRegion) -> u32 {
        match &region.param {
            VrParam::Shared { id, .. } => *id as u32,
            _ => 0,
        }
    }

    fn ref_count(&self, region: &crate::region::VirRegion) -> i32 {
        1 + region.remaps
    }
}
```

### 6.6 全局静态实例

```rust
pub(crate) static MEM_TYPE_ANON: AnonymousMemory = AnonymousMemory::new();
pub(crate) static MEM_TYPE_DIRECT: DirectPhysical = DirectPhysical::new();
pub(crate) static MEM_TYPE_SHARED: SharedMemory = SharedMemory::new();
pub(crate) static MEM_TYPE_CONTIG: ContiguousAnonymous = ContiguousAnonymous;
pub(crate) static MEM_TYPE_CACHE: CacheMemory = CacheMemory;
// MEM_TYPE_MAPPEDFILE 在 24-vfs-interaction 中设计
```

---

## 7. 缓存请求处理

### 7.1 do_mapcache

```rust
pub(crate) fn do_mapcache(
    msg: &Message,
    table: &mut VmProcTable,
    cache: &mut PageCache,
) -> Result<(), i32> {
    let dev = msg.m_vmmcp.dev;
    let dev_off = msg.m_vmmcp.dev_offset;
    let bytes = msg.m_vmmcp.pages * PAGE_SIZE;

    let caller = table.find_by_endpoint(msg.m_source)?;

    let mut vr = map_page_region(
        caller, MMAP_BASE, MMAP_TOP, bytes,
        VrFlags::ANON | VrFlags::WRITABLE,
        0, &MEM_TYPE_CACHE,
    ).ok_or(ENOMEM)?;

    for offset in (0..bytes).step_by(PAGE_SIZE) {
        let cp = cache.find_by_dev(dev, dev_off + offset)
            .ok_or(ENOENT)?;

        if cp.flags == CacheFlags::Once {
            unmap_region(caller, vr.vaddr, 0, bytes)?;
            return Err(ENOENT);
        }

        vr.param = VrParam::PbCache { pb: Some(Arc::clone(&cp.page)) };

        map_pf(caller, &mut vr, offset, true)?;

        match &vr.param {
            VrParam::PbCache { pb: None } => {},
            _ => return Err(ENOMEM),
        }
    }

    Ok(())
}
```

### 7.2 do_setcache

```rust
pub(crate) fn do_setcache(
    msg: &Message,
    table: &mut VmProcTable,
    cache: &mut PageCache,
) -> Result<(), i32> {
    let dev = msg.m_vmmcp.dev;
    let dev_off = msg.m_vmmcp.dev_offset;
    let bytes = msg.m_vmmcp.pages * PAGE_SIZE;

    let caller = table.find_by_endpoint(msg.m_source)?;

    for offset in (0..bytes).step_by(PAGE_SIZE) {
        let vaddr = msg.m_vmmcp.block as usize + offset;

        let (region, phys_region) = map_lookup(caller, vaddr)?;

        if let Some(cp) = cache.find_by_dev(dev, dev_off + offset) {
            if Arc::ptr_eq(&cp.page, &phys_region.physblock)
                && cp.flags != CacheFlags::Once {
                continue;
            }
            cache.remove(dev, dev_off + offset);
        }

        phys_region.memtype = &MEM_TYPE_CACHE;

        cache.add(
            dev, dev_off + offset,
            msg.m_vmmcp.ino, msg.m_vmmcp.ino_offset + offset as u64,
            CacheFlags::None,
            Arc::clone(&phys_region.physblock),
        )?;
    }

    Ok(())
}
```

---

## 8. 六种 MemType 的完整对比

| 特性 | Anon | AnonContig | Direct | Cache | Shared | MappedFile |
|------|------|-----------|--------|-------|--------|-----------|
| **on_new** | 空操作 | 预分配连续页 | 空操作 | 空操作 | 空操作 | 空操作 |
| **pagefault** | 分配新页 | panic | 计算物理地址 | 从 pb_cache 链接 | 从源区域链接 | VFS 读取 |
| **CoW** | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ (写入时) |
| **fork** | ✅ | ❌ | ✅ (复制参数) | ✅ (空操作) | ✅ (复制参数) | ✅ (复制参数) |
| **resize** | ✅ (brk) | ❌ | ❌ | ❌ | ❌ | ❌ |
| **split** | 空操作 | 空操作 | 空操作 | — | — | — |
| **writable** | refcount==1 | refcount==1 | phys!=NONE | phys!=NONE | phys!=NONE | phys!=NONE |
| **unreference** | 释放物理页 | 释放物理页 | 不释放 | 释放物理页 | 不释放 | 释放物理页 |
| **VFS 交互** | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| **缓存交互** | ❌ | ❌ | ❌ | ✅ | ❌ | ✅ |

---

## 9. 实现清单

### 9.1 需要新增的文件

| 文件 | 内容 | 优先级 |
|------|------|--------|
| `os/servers/vm/src/cache.rs` | `PageCache` 数据结构 | 🔴 P0 |

### 9.2 需要修改的文件

| 文件 | 修改内容 | 优先级 |
|------|---------|--------|
| `memtype.rs` | 新增 `CacheMemory`、`ContiguousAnonymous`；补全 `SharedMemory` | 🔴 P0 |
| `lib.rs` | 新增 `pub(crate) mod cache` | 🔴 P0 |
| `vir_region.rs` | `VrParam` 已有 `PbCache`，无需修改 | — |

### 9.3 测试计划

| 测试 | 描述 |
|------|------|
| `test_page_cache_add_remove` | 缓存添加和移除 |
| `test_page_cache_find_by_dev` | 按设备查找 |
| `test_page_cache_lru_eviction` | LRU 淘汰 |
| `test_page_cache_free_pages` | 内存不足时回收 |
| `test_page_cache_clear_by_dev` | 按设备清除 |
| `test_cache_memory_pagefault` | 缓存缺页从 pb_cache 链接 |
| `test_contig_anon_new` | 连续内存预分配 |
| `test_contig_anon_no_fork` | 禁止 fork |
| `test_shared_pagefault` | 共享缺页从源区域链接 |
| `test_shared_copy` | fork 时复制共享参数 |

---

## 10. 设计洞察

### 10.1 MemType 切换模式

Minix3 中有一个重要的模式：**memtype 就地切换**。

```c
/* do_setcache 中 */
phys_region->memtype = &mem_type_cache;  /* anon → cache */

/* mappedfile_pagefault 中（24-vfs-interaction） */
cow_block(vmp, region, ph, 0);
/* cow_block 内部: ph->memtype = &mem_type_anon;  /* file → anon */
```

这意味着同一个 `PhysRegion` 的 memtype 可以在运行时改变。Rust 实现需要支持这种动态切换——`PhysRegion.memtype` 应该是 `&'static dyn MemType`（静态分发不可变），或者 `Rc<dyn MemType>` / 函数指针（动态分发可变）。

**推荐方案**：保持 `&'static dyn MemType`，因为 memtype 切换是低频操作，可以在切换时重新赋值引用。所有 memtype 实例都是全局静态的，生命周期为 `'static`。

### 10.2 PageCache 的线程安全

Minix3 的 PageCache 是全局单线程访问的（VM 是单线程事件循环）。Rust 实现中，`PageCache` 也应该是 VM 主循环独占的，不需要 `Mutex`。

### 10.3 Arc vs 手动引用计数

Minix3 的 `phys_block.refcount` 手动管理容易出错。Rust 使用 `Arc<PhysBlock>` 自动管理引用计数：

| 操作 | Minix3 | Rust |
|------|--------|------|
| 缓存添加 | `pb->refcount++` | `Arc::clone(&pb)` |
| 缓存移除 | `pb->refcount--` | `Arc` drop |
| 检查可淘汰 | `pb->refcount == 1` | `Arc::strong_count(&pb) == 1` |
| 释放物理页 | `if(refcount==0) free_mem()` | `PhysBlock::drop()` |

**注意**：`Arc::strong_count()` 有性能开销，但在 VM 的场景下（非热路径），这是可接受的。
