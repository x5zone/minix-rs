# 22-vm-munmap: 取消映射与物理内存映射

> **分类**: VM服务
> **源码**: `minix3/minix/servers/vm/mmap.c`, `region.c:map_unmap_region/range`, `mem_directphys.c`
> **说明**: munmap 取消虚拟地址映射、释放物理页；map_phys 将物理地址直接映射到进程地址空间

---

## 1. 概述

### 1.1 本文档覆盖什么

munmap 和 map_phys 是一对互补操作：

| 操作 | 方向 | 效果 |
|------|------|------|
| `map_phys` | 物理地址 → 虚拟地址 | 将设备内存/物理内存映射到进程地址空间 |
| `munmap` | 虚拟地址 → 取消映射 | 释放虚拟区域，归还物理页（如果有） |

两者共享底层基础设施：
- `map_unmap_region()` — 取消单个区域（或区域的一部分）的映射
- `map_unmap_range()` — 取消一段地址范围内的所有映射
- `split_region()` — 将一个区域一分为二（munmap 中间部分时需要）
- `map_subfree()` — 释放区域内的物理页引用

### 1.2 与其他文档的关系

| 文档 | 关系 |
|------|------|
| 21-vm-brk-complete | brk 收缩使用 `free_range()`，与 munmap 的 `map_subfree` 逻辑类似 |
| 20-vm-exit | `map_free_proc()` 释放所有区域，是 munmap 的"全部取消"特例 |
| 14-phys-region | `pb_unreferenced()` 是 munmap 释放物理页的核心 |
| 13-region-avl | AVL 树搜索、插入、删除操作 |
| 12-vir-region | VirRegion 结构和 split 操作 |

### 1.3 三个取消映射的入口

Minix3 有三个取消映射的消息类型，都由 `do_munmap()` 处理：

| 消息类型 | 调用者 | 说明 |
|---------|--------|------|
| `VM_MUNMAP` | 用户进程 | 标准 munmap 系统调用 |
| `VM_UNMAP_PHYS` | 内核/驱动 | 取消 map_phys 的映射 |
| `VM_SHM_UNMAP` | 用户进程 | 取消共享内存映射 |

---

## 2. C 源码分析

### 2.1 do_munmap — 多入口统一处理

```c
/* mmap.c:512 */
int do_munmap(message *m)
{
    int r, n;
    struct vmproc *vmp;
    struct vir_region *vr;
    vir_bytes addr, len;
    endpoint_t target = SELF;

    /* 1. 确定目标进程 */
    if(m->m_type == VM_UNMAP_PHYS)
        target = m->m_lsys_vm_unmap_phys.ep;
    else if(m->m_type == VM_SHM_UNMAP)
        target = m->m_lc_vm_shm_unmap.forwhom;

    if(target == SELF)
        target = m->m_source;

    if((r=vm_isokendpt(target, &n)) != OK)
        panic("do_mmap: message from strange source");

    vmp = &vmproc[n];

    /* 2. VM 自身取消映射的特殊处理 */
    if(m->m_source == VM_PROC_NR) {
        if(!region_search_root(&vmp->vm_regions_avl)) {
            munmap_vm_lin(addr, m->VMUM_LEN);
        }
        else if((vr = map_lookup(vmp, addr, NULL))) {
            if(map_unmap_region(vmp, vr, 0, m->VMUM_LEN) != OK) {
                printf("VM: self map_unmap_region failed\n");
            }
        }
        return SUSPEND;
    }

    /* 3. 获取地址和长度 */
    if(m->m_type == VM_UNMAP_PHYS)
        addr = (vir_bytes) m->m_lsys_vm_unmap_phys.vaddr;
    else if(m->m_type == VM_SHM_UNMAP)
        addr = (vir_bytes) m->m_lc_vm_shm_unmap.addr;
    else
        addr = (vir_bytes) m->VMUM_ADDR;

    if(addr % VM_PAGE_SIZE) return EFAULT;

    /* 4. UNMAP_PHYS/SHM_UNMAP: 取消整个区域 */
    if(m->m_type == VM_UNMAP_PHYS || m->m_type == VM_SHM_UNMAP) {
        if(!(vr = map_lookup(vmp, addr, NULL))) {
            printf("VM: unmap: address not found\n");
            return EFAULT;
        }
        len = vr->length;   /* 使用区域长度，忽略消息中的 len */
    } else {
        len = roundup(m->VMUM_LEN, VM_PAGE_SIZE);  /* 标准munmap: 页对齐 */
    }

    return map_unmap_range(vmp, addr, len);
}
```

**关键设计**：
- `VM_UNMAP_PHYS` 和 `VM_SHM_UNMAP` 按区域整体取消映射（`len = vr->length`）
- `VM_MUNMAP` 按指定长度取消映射（`len = roundup(msg_len)`）
- VM 自身取消映射是特殊情况，因为 VM 进程的地址空间管理与其他进程不同

### 2.2 map_unmap_range — 范围取消映射

```c
/* region.c:1222 */
int map_unmap_range(struct vmproc *vmp, vir_bytes unmap_start, vir_bytes length)
{
    vir_bytes o = unmap_start % VM_PAGE_SIZE, unmap_limit;
    region_iter v_iter;
    struct vir_region *vr, *nextvr;

    /* 1. 页对齐 */
    unmap_start -= o;
    length += o;
    length = roundup(length, VM_PAGE_SIZE);
    unmap_limit = length + unmap_start;

    if(length < VM_PAGE_SIZE) return EINVAL;
    if(unmap_limit <= unmap_start) return EINVAL;

    /* 2. 找到第一个重叠区域 */
    region_start_iter(&vmp->vm_regions_avl, &v_iter,
        unmap_start, AVL_LESS_EQUAL);

    if(!(vr = region_get_iter(&v_iter))) {
        region_start_iter(&vmp->vm_regions_avl, &v_iter,
            unmap_start, AVL_GREATER);
        if(!(vr = region_get_iter(&v_iter))) {
            return OK;   /* 没有区域可取消映射 */
        }
    }

    /* 3. 遍历所有重叠区域 */
    for(; vr && vr->vaddr < unmap_limit; vr = nextvr) {
        vir_bytes thislimit = vr->vaddr + vr->length;
        vir_bytes this_unmap_start, this_unmap_limit;
        vir_bytes remainlen;
        int r;

        region_incr_iter(&v_iter);
        nextvr = region_get_iter(&v_iter);

        this_unmap_start = MAX(unmap_start, vr->vaddr);
        this_unmap_limit = MIN(unmap_limit, thislimit);

        if(this_unmap_start >= this_unmap_limit) continue;

        /* 4. 中间部分：需要先 split */
        if(this_unmap_start > vr->vaddr && this_unmap_limit < thislimit) {
            struct vir_region *vr1, *vr2;
            vir_bytes split_len = this_unmap_limit - vr->vaddr;
            if((r=split_region(vmp, vr, &vr1, &vr2, split_len)) != OK)
                return r;
            vr = vr1;
            thislimit = vr->vaddr + vr->length;
        }

        /* 5. 取消映射 */
        r = map_unmap_region(vmp, vr,
            this_unmap_start - vr->vaddr,
            this_unmap_limit - this_unmap_start);

        if(r != OK) return r;

        /* 6. 重新定位迭代器 */
        if(nextvr) {
            region_start_iter(&vmp->vm_regions_avl, &v_iter,
                nextvr->vaddr, AVL_EQUAL);
        }
    }

    return OK;
}
```

**核心难点**：munmap 可以取消区域的一部分，产生三种情况：

```
情况 1: 取消整个区域
  [AAAAAA]          →  (空)
  |munmap|

情况 2: 从头部取消
  [AAAAAA]          →  [BBBB]
  |munmap|              ^vaddr 前移

情况 3: 从尾部取消
  [AAAAAA]          →  [AAAA]
       |munmap|          ^length 缩减

情况 4: 从中间取消（需要 split）
  [AAAAAA]          →  [AA][CC]
     |munmap|           split 后取消中间部分
```

### 2.3 map_unmap_region — 单区域取消映射

```c
/* region.c:1065 */
int map_unmap_region(struct vmproc *vmp, struct vir_region *r,
    vir_bytes offset, vir_bytes len)
{
    vir_bytes regionstart;
    int freeslots = phys_slot(len);

    if(offset+len > r->length || (len % VM_PAGE_SIZE))
        return EINVAL;

    regionstart = r->vaddr + offset;

    /* 1. 释放物理页引用 */
    map_subfree(r, offset, len);

    /* 2. 根据位置分三种情况 */
    if(r->length == len) {
        /* 情况 1: 整个区域取消 → 从 AVL 树移除并释放 */
        region_remove(&vmp->vm_regions_avl, r->vaddr);
        map_free(r);

    } else if(offset == 0) {
        /* 情况 2: 从头部取消 → vaddr 前移，length 缩减 */

        /* 2a. 调用 ev_lowshrink 回调 */
        if(!r->def_memtype->ev_lowshrink)
            return EINVAL;
        if(r->def_memtype->ev_lowshrink(r, len) != OK)
            return EINVAL;

        /* 2b. 从 AVL 树移除，修改 vaddr，重新插入 */
        region_remove(&vmp->vm_regions_avl, r->vaddr);
        r->vaddr += len;

        /* 2c. 调整 physblocks 数组 */
        int remslots = phys_slot(r->length);
        for(voffset = len; voffset < r->length; voffset += VM_PAGE_SIZE) {
            if(!(pr = physblock_get(r, voffset))) continue;
            pr->offset -= len;    /* offset 前移 */
        }
        memmove(r->physblocks, r->physblocks + freeslots,
            remslots * sizeof(struct phys_region *));
        r->length -= len;

        region_insert(&vmp->vm_regions_avl, r);

    } else if(offset + len == r->length) {
        /* 情况 3: 从尾部取消 → length 缩减 */
        r->length -= len;
    }

    /* 3. 更新页表：取消映射 */
    if(pt_writemap(vmp, &vmp->vm_pt, regionstart,
        MAP_NONE, len, 0, WMF_OVERWRITE) != OK) {
        return ENOMEM;
    }

    return OK;
}
```

**三种情况的处理对比**：

| 方面 | 整体取消 | 头部取消 | 尾部取消 |
|------|---------|---------|---------|
| AVL 树 | remove + free | remove + 修改 vaddr + insert | 无需修改 |
| physblocks | 全部释放 | memmove 前移 | 截断（length 缩减即可） |
| vaddr | 不变（区域被释放） | += len | 不变 |
| length | 不变（区域被释放） | -= len | -= len |
| ev_lowshrink | 不需要 | **必须调用** | 不需要 |
| 页表 | pt_writemap(MAP_NONE) | pt_writemap(MAP_NONE) | pt_writemap(MAP_NONE) |

**为什么头部取消需要 ev_lowshrink？** 因为头部取消后 vaddr 前移，physblocks 数组需要 memmove，PhysRegion 的 offset 需要调整。不同 memtype 可能有额外状态需要更新（如共享内存的 source 指针）。

### 2.4 map_subfree — 释放物理页引用

```c
/* region.c:527 */
static int map_subfree(struct vir_region *region,
    vir_bytes start, vir_bytes len)
{
    struct phys_region *pr;
    vir_bytes end = start + len;
    vir_bytes voffset;

    for(voffset = start; voffset < end; voffset += VM_PAGE_SIZE) {
        if(!(pr = physblock_get(region, voffset)))
            continue;
        assert(pr->offset >= start);
        assert(pr->offset < end);
        pb_unreferenced(region, pr, 1);  /* 减少引用计数，可能释放物理页 */
        SLABFREE(pr);                     /* 释放 PhysRegion 本身 */
    }

    return OK;
}
```

**与 map_free 的关系**：
- `map_subfree(region, 0, region->length)` = 释放区域内所有物理页
- `map_free(region)` = `map_subfree(0, length)` + `ev_delete` + 释放 physblocks 数组 + 释放 VirRegion 本身
- `map_free_proc(vmp)` = 对所有区域调用 `map_free`

```
map_free_proc  →  map_free  →  map_subfree(0, length)
                                    ↓
                              pb_unreferenced → refcount--
                                    ↓
                              refcount == 0? → free_mem + ev_unreference
```

### 2.5 split_region — 区域分裂

```c
/* region.c:1150 */
static int split_region(struct vmproc *vmp, struct vir_region *vr,
    struct vir_region **vr1, struct vir_region **vr2, vir_bytes split_len)
{
    struct vir_region *r1 = NULL, *r2 = NULL;
    vir_bytes rem_len = vr->length - split_len;

    /* 1. 检查 memtype 支持 split */
    if(!vr->def_memtype->ev_split) {
        printf("VM: split not implemented for %s\n", vr->def_memtype->name);
        return EINVAL;
    }

    /* 2. 创建两个新区域 */
    r1 = region_new(vmp, vr->vaddr, split_len, vr->flags, vr->def_memtype);
    r2 = region_new(vmp, vr->vaddr+split_len, rem_len, vr->flags, vr->def_memtype);

    /* 3. 转移 PhysRegion 引用 */
    for(voffset = 0; voffset < r1->length; voffset += VM_PAGE_SIZE) {
        if(!(ph = physblock_get(vr, voffset))) continue;
        phn = pb_reference(ph->ph, voffset, r1, ph->memtype);  /* refcount++ */
    }

    for(voffset = 0; voffset < r2->length; voffset += VM_PAGE_SIZE) {
        if(!(ph = physblock_get(vr, split_len + voffset))) continue;
        phn = pb_reference(ph->ph, voffset, r2, ph->memtype);  /* refcount++ */
    }

    /* 4. 通知 memtype，替换 AVL 节点 */
    vr->def_memtype->ev_split(vmp, vr, r1, r2);
    region_remove(&vmp->vm_regions_avl, vr->vaddr);
    map_free(vr);   /* 释放原区域（refcount-- for each PhysBlock） */
    region_insert(&vmp->vm_regions_avl, r1);
    region_insert(&vmp->vm_regions_avl, r2);

    *vr1 = r1;
    *vr2 = r2;
    return OK;
}
```

**split 的引用计数变化**：

```
split 前:  PhysBlock refcount = 1 (被 vr 引用)
split 后:  PhysBlock refcount = 2 (被 r1 和 r2 引用)
map_free(vr) 后: PhysBlock refcount = 1 (只被 r1 或 r2 引用)
```

`pb_reference()` 将 refcount 从 1 增加到 2，`map_free(vr)` 通过 `pb_unreferenced()` 将 refcount 从 2 减回 1。最终每个 PhysBlock 只被一个子区域引用。

### 2.6 do_map_phys — 物理内存映射

```c
/* mmap.c:310 */
int do_map_phys(message *m)
{
    int r, n;
    struct vmproc *vmp;
    endpoint_t target;
    struct vir_region *vr;
    vir_bytes len;
    phys_bytes startaddr;
    size_t offset;

    target = m->m_lsys_vm_map_phys.ep;
    len = m->m_lsys_vm_map_phys.len;

    if (len <= 0) return EINVAL;

    if(target == SELF) target = m->m_source;

    if((r=vm_isokendpt(target, &n)) != OK) return EINVAL;

    startaddr = (vir_bytes)m->m_lsys_vm_map_phys.phaddr;

    /* 1. 权限检查 */
    if(map_perm_check(m->m_source, target, startaddr, len) != OK)
        return EPERM;

    vmp = &vmproc[n];

    /* 2. 页对齐 */
    offset = startaddr % VM_PAGE_SIZE;
    len += offset;
    startaddr -= offset;
    if(len % VM_PAGE_SIZE)
        len += VM_PAGE_SIZE - (len % VM_PAGE_SIZE);

    /* 3. 创建 directphys 区域 */
    if(!(vr = map_page_region(vmp, VM_MMAPBASE, VM_MMAPTOP, len,
        VR_DIRECT | VR_WRITABLE, 0, &mem_type_directphys)))
        return ENOMEM;

    /* 4. 设置物理基地址 */
    phys_setphys(vr, startaddr);

    /* 5. 返回映射后的虚拟地址 */
    m->m_lsys_vm_map_phys.reply = (void *) (vr->vaddr + offset);

    return OK;
}
```

**map_phys 的特殊性**：

| 方面 | 普通匿名内存 | directphys 映射 |
|------|------------|----------------|
| 物理页分配 | 延迟分配（缺页时） | **不分配**，直接映射已有物理地址 |
| PhysBlock.phys | 初始 MAP_NONE，缺页时分配 | 初始 MAP_NONE，缺页时计算 `param.phys + offset` |
| 释放物理页 | refcount==0 时 free_mem | **不释放**（ev_unreference 返回 OK，不做任何事） |
| 缺页处理 | alloc_phys + zero fill | `ph->ph->phys = param.phys + ph->offset` |
| VR_DIRECT 标志 | 无 | 有 |
| 映射范围 | 任意 | VM_MMAPBASE ~ VM_MMAPTOP |

### 2.7 mem_type_directphys — 直接物理映射的 memtype

```c
/* mem_directphys.c */
struct mem_type mem_type_directphys = {
    .name = "physical memory mapping",
    .ev_copy = phys_copy,
    .ev_unreference = phys_unreference,   /* 空操作，不释放物理页 */
    .writable = phys_writable,
    .ev_pagefault = phys_pagefault,       /* 计算物理地址，不分配新页 */
    .pt_flags = phys_pt_flags
};

static int phys_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, ...)
{
    phys_bytes arg = region->param.phys, phmem;
    assert(arg != MAP_NONE);
    assert(ph->ph->phys == MAP_NONE);
    phmem = arg + ph->offset;        /* 物理地址 = 基地址 + 页内偏移 */
    assert(phmem != MAP_NONE);
    ph->ph->phys = phmem;            /* 直接设置，不分配新页 */
    return OK;
}

static int phys_unreference(struct phys_region *pr)
{
    return OK;    /* 不释放物理页！设备内存不属于 VM 管理 */
}

static int phys_copy(struct vir_region *vr, struct vir_region *newvr)
{
    newvr->param.phys = vr->param.phys;   /* fork 时复制物理基地址 */
    return OK;
}
```

**关键洞察**：directphys 区域的物理页不是 VM 分配的，而是设备/驱动提供的。VM 只负责建立虚拟→物理的映射，不负责分配和释放物理内存。这就是 `ev_unreference` 为空操作的原因。

### 2.8 map_lookup — 查找地址所在区域

```c
/* region.c:616 */
struct vir_region *map_lookup(struct vmproc *vmp,
    vir_bytes offset, struct phys_region **physr)
{
    struct vir_region *r;

    if((r = region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL))) {
        if(offset >= r->vaddr && offset < r->vaddr + r->length) {
            if(physr) {
                *physr = physblock_get(r, offset - r->vaddr);
            }
            return r;
        }
    }

    return NULL;
}
```

**搜索策略**：`AVL_LESS_EQUAL` 找到 `vaddr <= offset` 的最大区域，然后检查 offset 是否在该区域范围内。这比遍历所有区域高效得多——O(log n) 而非 O(n)。

---

## 3. Rust 设计决策

### 3.1 现有代码状态

| 组件 | 现有状态 | munmap 需要的操作 |
|------|---------|-----------------|
| `RegionMap::find()` | ✅ 已实现 | 对应 `region_search(AVL_LESS_EQUAL)` |
| `RegionMap::search(SearchType::GREATER)` | ✅ 已实现 | 找下一个区域 |
| `RegionMap::remove()` | ✅ 已实现 | 移除区域 |
| `RegionMap::insert()` | ✅ 已实现 | 重新插入修改后的区域 |
| `VirRegion::split()` | ✅ 已实现 | 区域分裂 |
| `VirRegion::free_range()` | ✅ 已实现 | 释放范围内物理页 |
| `DirectPhysical` memtype | ✅ 已实现 | directphys 缺页和 unreference |
| `MemType::on_low_shrink()` | ✅ 默认空实现 | 头部取消映射时需要 |
| `MemType::on_split()` | ✅ 默认空实现 | split_region 时需要 |
| `do_munmap()` | ❌ 不存在 | IPC 处理 |
| `unmap_region()` | ❌ 不存在 | 单区域取消映射 |
| `unmap_range()` | ❌ 不存在 | 范围取消映射 |
| `do_map_phys()` | ❌ 不存在 | 物理内存映射 |
| `map_lookup()` | ❌ 不存在 | 地址查找区域 |
| `map_perm_check()` | ❌ 不存在 | 权限检查 |

### 3.2 设计原则

**原则 1：统一三个入口**

与 Minix3 一致，`VM_MUNMAP`、`VM_UNMAP_PHYS`、`VM_SHM_UNMAP` 统一由 `do_munmap()` 处理。差异通过消息类型区分。

**原则 2：split 沿用现有实现**

Rust 已有 `VirRegion::split()`，直接使用。但需要补全 `MemType::on_split()` 的具体实现（`AnonymousMemory::on_split` 和 `DirectPhysical::on_split`）。

**原则 3：头部取消的 physblocks 调整**

Minix3 使用 `memmove` 调整 physblocks 数组。Rust 中 `Vec` 的 `drain()` 和 `splice()` 可以实现相同效果，更安全。

**原则 4：map_phys 不分配物理页**

与 Minix3 一致，`DirectPhysical` 的 `ev_pagefault` 只计算物理地址，不分配新页。`ev_unreference` 不释放物理页。

### 3.3 与 Minix3 的关键差异

| 方面 | Minix3 | minix-rs |
|------|--------|----------|
| physblocks 调整 | `memmove` + 手动 offset 调整 | `Vec::drain()` + 自动 offset |
| 区域迭代 | `region_start_iter` + `region_incr_iter` | `RegionMap::iter()` |
| 物理页释放 | `pb_unreferenced()` + `SLABFREE(pr)` | `PhysRegion::unlink_from_block()` + `Vec::take()` |
| 权限检查 | `map_perm_check()` | 需要实现 ACL 检查 |
| split 后引用计数 | `pb_reference()` (refcount++) + `map_free()` (refcount--) | `PhysBlock::reference()` + `VirRegion::free_range()` |
| VM 自身取消映射 | 特殊路径 `munmap_vm_lin()` | 简化处理 |

---

## 4. Rust 实现详解

### 4.1 do_munmap — IPC 处理入口

```rust
/// 处理 VM_MUNMAP / VM_UNMAP_PHYS / VM_SHM_UNMAP 请求
pub fn do_munmap(
    msg: &MunmapMessage,
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), MunmapError> {
    let (target, addr, len, unmap_whole) = match msg.msg_type {
        VmMsgType::VM_UNMAP_PHYS => {
            let target = msg.unmap_phys.ep;
            let addr = msg.unmap_phys.vaddr;
            (target, addr, VirBytes(0), true)
        }
        VmMsgType::VM_SHM_UNMAP => {
            let target = msg.shm_unmap.forwhom;
            let addr = msg.shm_unmap.addr;
            (target, addr, VirBytes(0), true)
        }
        _ => {
            let addr = msg.munmap.addr;
            let len = page_align_up(VirBytes(msg.munmap.len));
            (msg.source, addr, len, false)
        }
    };

    if addr.0 % PAGE_SIZE != 0 {
        return Err(MunmapError::NotAligned);
    }

    let slot = table.vm_isokendpt(target)
        .map_err(|_| MunmapError::InvalidEndpoint)?;

    let mut vmp = table.get_active(slot)
        .ok_or(MunmapError::ProcessNotActive)?;

    if unmap_whole {
        let region = vmp.regions().find(addr)
            .ok_or(MunmapError::AddressNotFound)?;
        let region_len = region.length;
        unmap_range(&mut vmp, addr, region_len, page_alloc)
    } else {
        unmap_range(&mut vmp, addr, len, page_alloc)
    }
}
```

### 4.2 unmap_range — 范围取消映射

```rust
/// 取消指定地址范围内的映射
///
/// 对应 Minix3 的 map_unmap_range()。
fn unmap_range(
    vmp: &mut ActiveProc<'_>,
    unmap_start: VirBytes,
    length: VirBytes,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), MunmapError> {
    let unmap_start = page_align_down(unmap_start);
    let length = page_align_up(length);
    let unmap_limit = VirBytes(unmap_start.0.checked_add(length.0)
        .ok_or(MunmapError::InvalidRange)?);

    if length.0 < PAGE_SIZE || unmap_limit.0 <= unmap_start.0 {
        return Err(MunmapError::InvalidRange);
    }

    // 收集所有重叠区域的 vaddr
    let overlap_vaddrs: Vec<VirBytes> = vmp.regions()
        .find_all_overlaps(unmap_start, unmap_limit)
        .map(|r| r.vaddr)
        .collect();

    for vr_vaddr in overlap_vaddrs {
        // 重新获取区域信息（因为前一次操作可能修改了树结构）
        let (vr_start, vr_end) = {
            let region = vmp.regions().find(vr_vaddr)
                .ok_or(MunmapError::InternalError)?;
            (region.vaddr, region.end_addr())
        };

        let this_start = VirBytes(unmap_start.0.max(vr_start.0));
        let this_end = VirBytes(unmap_limit.0.min(vr_end.0));

        if this_start.0 >= this_end.0 {
            continue;
        }

        let offset = VirBytes(this_start.0 - vr_start.0);
        let len = VirBytes(this_end.0 - this_start.0);

        // 中间部分：需要先 split
        if this_start.0 > vr_start.0 && this_end.0 < vr_end.0 {
            let split_len = VirBytes(this_end.0 - vr_start.0);
            let region = vmp.regions_mut().remove(vr_vaddr)
                .ok_or(MunmapError::InternalError)?;

            let (left, right) = region.split(split_len)
                .map_err(|_| MunmapError::SplitFailed)?;

            vmp.regions_mut().insert(left);
            vmp.regions_mut().insert(right);

            // 现在取消 left 的尾部
            unmap_region(vmp, vr_vaddr,
                VirBytes(split_len.0 - len.0), len, page_alloc)?;
        } else {
            unmap_region(vmp, vr_vaddr, offset, len, page_alloc)?;
        }
    }

    Ok(())
}
```

### 4.3 unmap_region — 单区域取消映射

```rust
/// 取消单个区域（或区域的一部分）的映射
///
/// 对应 Minix3 的 map_unmap_region()。
fn unmap_region(
    vmp: &mut ActiveProc<'_>,
    vr_vaddr: VirBytes,
    offset: VirBytes,
    len: VirBytes,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), MunmapError> {
    const PAGE_SIZE: u64 = 4096;

    if offset.0 + len.0 > {
        let r = vmp.regions().find(vr_vaddr).ok_or(MunmapError::InternalError)?;
        r.length.0
    } {
        return Err(MunmapError::InvalidRange);
    }
    if len.0 % PAGE_SIZE != 0 {
        return Err(MunmapError::NotAligned);
    }

    let region_start = VirBytes(vr_vaddr.0 + offset.0);

    // 1. 释放物理页引用
    {
        let region = vmp.regions_mut().find_mut(vr_vaddr)
            .ok_or(MunmapError::InternalError)?;
        region.free_range(offset, len);
    }

    // 2. 根据位置分三种情况
    let region_length = {
        let region = vmp.regions().find(vr_vaddr)
            .ok_or(MunmapError::InternalError)?;
        region.length
    };

    if len.0 == region_length.0 {
        // 情况 1: 整个区域取消
        let region = vmp.regions_mut().remove(vr_vaddr)
            .ok_or(MunmapError::InternalError)?;

        if let Some(memtype) = region.def_memtype {
            memtype.ev_delete(&mut region.clone());
        }
        // region 被 drop，PhysRegion 的 Drop 会处理引用计数

    } else if offset.0 == 0 {
        // 情况 2: 从头部取消
        let region = vmp.regions_mut().remove(vr_vaddr)
            .ok_or(MunmapError::InternalError)?;

        // 调用 on_low_shrink
        if let Some(memtype) = region.def_memtype {
            memtype.ev_low_shrink(&mut region.clone(), len)
                .map_err(|_| MunmapError::LowShrinkFailed)?;
        }

        // 调整 physblocks：移除前 freeslots 个元素
        let freeslots = (len.0 / PAGE_SIZE) as usize;
        let remslots = ((region.length.0 - len.0) / PAGE_SIZE) as usize;

        // 调整 PhysRegion offset
        for i in freeslots..(freeslots + remslots) {
            if let Some(pr) = &mut region.physblocks[i - freeslots] {
                // offset 已在 split 中调整
            }
        }

        region.physblocks.drain(0..freeslots);
        region.vaddr = VirBytes(region.vaddr.0 + len.0);
        region.length = VirBytes(region.length.0 - len.0);

        vmp.regions_mut().insert(region);

    } else if offset.0 + len.0 == region_length.0 {
        // 情况 3: 从尾部取消
        let region = vmp.regions_mut().find_mut(vr_vaddr)
            .ok_or(MunmapError::InternalError)?;

        let removed_pages = (len.0 / PAGE_SIZE) as usize;
        let total_pages = (region.length.0 / PAGE_SIZE) as usize;
        region.physblocks.truncate(total_pages - removed_pages);
        region.length = VirBytes(region.length.0 - len.0);

    } else {
        return Err(MunmapError::InternalError);
    }

    // 3. 更新页表
    {
        let pt = vmp.page_table_mut();
        let mut addr = region_start.0;
        while addr < region_start.0 + len.0 {
            let _ = pt.unmap(VirBytes(addr));
            addr += PAGE_SIZE;
        }
    }

    // 4. 刷新 TLB
    unsafe { vmp.page_table_mut().flush_tlb(); }

    // 5. 更新内存统计
    vmp.sub_total(len);

    Ok(())
}
```

### 4.4 do_map_phys — 物理内存映射

```rust
/// 处理 VM_MAP_PHYS 请求
///
/// 对应 Minix3 的 do_map_phys()。
pub fn do_map_phys(
    msg: &MapPhysMessage,
    table: &VmProcTable,
) -> Result<VirBytes, MapPhysError> {
    let target = if msg.ep == SELF { msg.source } else { msg.ep };
    let len = msg.len;

    if len == 0 {
        return Err(MapPhysError::InvalidLength);
    }

    let slot = table.vm_isokendpt(target)
        .map_err(|_| MapPhysError::InvalidEndpoint)?;

    let mut vmp = table.get_active(slot)
        .ok_or(MapPhysError::ProcessNotActive)?;

    let startaddr = msg.phaddr;

    // 1. 权限检查
    check_phys_permission(msg.source, target, startaddr, len)
        .map_err(|_| MapPhysError::PermissionDenied)?;

    // 2. 页对齐
    let offset = (startaddr.0 % PAGE_SIZE) as u64;
    let aligned_len = page_align_up(VirBytes(len + offset as u64));
    let aligned_start = PhysBytes(startaddr.0 - offset as u64);

    // 3. 创建 directphys 区域
    let start_v = vmp.regions().find_slot(
        VirBytes(VM_MMAPBASE),
        VirBytes(VM_MMAPTOP),
        aligned_len,
    ).ok_or(MapPhysError::NoVirtualSpace)?;

    let mut region = VirRegion::with_memtype(
        start_v,
        aligned_len,
        VrFlags(VrFlags::DIRECT | VrFlags::WRITABLE),
        &MEM_TYPE_DIRECTPHYS as &'static dyn MemType,
    );
    region.param = VrParam::Direct { phys: aligned_start };

    // 4. 调用 on_new
    if let Some(memtype) = region.def_memtype {
        memtype.ev_new(&mut region)
            .map_err(|_| MapPhysError::NewRegionFailed)?;
    }

    vmp.regions_mut().insert(region);
    vmp.add_total(aligned_len);

    // 5. 返回映射后的虚拟地址
    Ok(VirBytes(start_v.0 + offset))
}
```

### 4.5 DirectPhysical::on_pagefault — 完善实现

当前 Rust 的 `DirectPhysical::on_pagefault` 已有基本框架，但需要确保缺页时正确计算物理地址：

```rust
impl MemType for DirectPhysical {
    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        pr: &mut crate::region::PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if let crate::region::VrParam::Direct { phys: base_phys } = &region.param {
            if *base_phys == PhysBlock::MAP_NONE {
                return Err(MemTypeError::InvalidParam);
            }
            // 物理地址 = 基地址 + 页内偏移
            if pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) == PhysBlock::MAP_NONE {
                // 需要设置物理地址，但不分配新页
                // pr.set_phys_addr(PhysBytes(base_phys.0 + pr.offset.0));
                return Ok(PagefaultResult::NeedNewPage);
            }
            return Ok(PagefaultResult::Handled);
        }
        Err(MemTypeError::InvalidParam)
    }
}
```

**与 Minix3 的差异**：Minix3 的 `phys_pagefault` 直接设置 `ph->ph->phys = arg + ph->offset`，不需要返回 `NeedNewPage`。Rust 实现中，`NeedNewPage` 返回后由缺页处理代码设置物理地址。这是架构差异——Rust 的缺页处理流程更模块化。

### 4.6 map_lookup — 地址查找区域

```rust
/// 查找地址所在的区域
///
/// 对应 Minix3 的 map_lookup()。
/// 使用 BTreeMap range 搜索，然后检查地址是否在区域内。
pub(crate) fn lookup_region(
    regions: &RegionMap,
    addr: VirBytes,
) -> Option<&VirRegion> {
    regions.find(addr)
}

pub(crate) fn lookup_region_mut(
    regions: &mut RegionMap,
    addr: VirBytes,
) -> Option<&mut VirRegion> {
    regions.find_mut(addr)
}
```

### 4.7 错误类型

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MunmapError {
    InvalidEndpoint,
    ProcessNotActive,
    AddressNotFound,
    NotAligned,
    InvalidRange,
    SplitFailed,
    LowShrinkFailed,
    InternalError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapPhysError {
    InvalidEndpoint,
    ProcessNotActive,
    InvalidLength,
    PermissionDenied,
    NoVirtualSpace,
    NewRegionFailed,
}
```

### 4.8 完整 munmap 流程（Rust）

```
VM_MUNMAP / VM_UNMAP_PHYS / VM_SHM_UNMAP IPC 到达
  │
  ▼
do_munmap():
  ├── 确定目标进程和地址
  ├── UNMAP_PHYS/SHM: lookup_region() → 获取区域长度
  ├── MUNMAP: 页对齐 len
  └── unmap_range(vmp, addr, len):
       ├── page_align_down(start) + page_align_up(len)
       ├── find_all_overlaps() → 收集重叠区域
       │
       └── for each overlap:
            ├── 计算交集 [this_start, this_end)
            │
            ├── 中间部分? → split:
            │    ├── regions.remove(vr_vaddr)
            │    ├── region.split(split_len) → (left, right)
            │    ├── regions.insert(left)
            │    ├── regions.insert(right)
            │    └── unmap_region(left, offset, len)
            │
            └── unmap_region(vmp, vr_vaddr, offset, len):
                 ├── free_range(offset, len) → 释放物理页引用
                 │
                 ├── 整体取消: regions.remove() + drop
                 ├── 头部取消: remove + drain + vaddr+=len + insert
                 ├── 尾部取消: truncate + length-=len
                 │
                 ├── pt.unmap() 取消页表映射
                 ├── pt.flush_tlb()
                 └── sub_total(len)
```

---

## 5. munmap 与 brk/exit 的关系

### 5.1 三种"释放内存"操作对比

| 操作 | 触发 | 释放范围 | 物理页处理 | 区域处理 |
|------|------|---------|-----------|---------|
| brk 收缩 | `brk(new_addr < current)` | 堆尾部 | `free_range()` | length 缩减 |
| munmap | `munmap(addr, len)` | 任意位置 | `free_range()` + `map_subfree()` | split + remove/modify |
| exit | 进程退出 | 全部 | `free_all()` | 全部移除 |

**复杂度递增**：brk 收缩只操作堆尾部（最简单），munmap 可以操作任意位置（需要 split），exit 释放全部（不需要 split）。

### 5.2 共享的底层操作

```
brk 收缩 → free_range() ← 共享
munmap   → free_range() ← 共享
exit     → free_all() = free_range(0, total_length) ← 本质相同
```

`free_range()` 是所有释放操作的底层原语：遍历 physblocks，对每个 PhysRegion 调用 `unlink_from_block()` 减少引用计数，refcount==0 时释放物理页。

### 5.3 munmap 中间部分的 split 流程

```
munmap(0x3000, 0x2000) 在区域 [0x1000, 0x6000) 上:

1. 检测到中间部分: 0x3000 > 0x1000 且 0x5000 < 0x6000

2. split(0x4000):  [0x1000, 0x6000) → [0x1000, 0x5000) + [0x5000, 0x6000)
   (split_len = 0x5000 - 0x1000 = 0x4000)
   PhysBlock refcount: 1→2 (pb_reference) → 2→1 (map_free 原区域)

3. unmap_region(left=[0x1000,0x5000), offset=0x2000, len=0x2000):
   → 从尾部取消: length = 0x2000, physblocks truncate

4. 最终结果: [0x1000, 0x3000) + [0x5000, 0x6000)
   中间的 [0x3000, 0x5000) 被取消映射
```

---

## 6. 实现清单

### 6.1 需要修改的现有代码

| 文件 | 修改内容 | 优先级 |
|------|---------|--------|
| `memtype.rs` | `AnonymousMemory::on_low_shrink()` 实现 | 🔴 P0 |
| `memtype.rs` | `AnonymousMemory::on_split()` 实现 | 🔴 P0 |
| `memtype.rs` | `DirectPhysical::on_pagefault()` 完善物理地址设置 | 🔴 P0 |
| `region/avl.rs` | 新增 `find_less_equal_mut()` 方法 | 🟡 P1 |

### 6.2 需要新增的代码

| 文件 | 新增内容 | 优先级 |
|------|---------|--------|
| `munmap.rs` (新) | `do_munmap()`, `unmap_range()`, `unmap_region()` | 🔴 P0 |
| `munmap.rs` | `do_map_phys()`, `check_phys_permission()` | 🔴 P0 |
| `munmap.rs` | `lookup_region()`, `lookup_region_mut()` | 🔴 P0 |
| `munmap.rs` | `MunmapError`, `MapPhysError` 错误类型 | 🟡 P1 |

### 6.3 测试计划

| 测试 | 描述 |
|------|------|
| `test_munmap_whole_region` | 取消整个区域的映射 |
| `test_munmap_head` | 从头部取消映射，vaddr 前移 |
| `test_munmap_tail` | 从尾部取消映射，length 缩减 |
| `test_munmap_middle` | 从中间取消映射，需要 split |
| `test_munmap_cross_region` | 取消范围跨越多个区域 |
| `test_munmap_no_region` | 地址无区域，返回错误 |
| `test_map_phys_basic` | 基本物理内存映射 |
| `test_map_phys_page_align` | 非页对齐物理地址自动对齐 |
| `test_map_phys_permission` | 权限检查拒绝未授权映射 |
| `test_unmap_phys` | 取消物理内存映射 |
| `test_munmap_page_table_update` | 页表正确取消映射 |
| `test_munmap_refcount` | CoW 页面 munmap 后引用计数正确 |

---

## 7. munmap 的引用计数场景

### 7.1 CoW 页面的 munmap

```
fork 后:
  父进程: VirRegion A → PhysBlock (refcount=2)
  子进程: VirRegion B → PhysBlock (refcount=2, 同一个)

父进程 munmap(A 的部分):
  → free_range() → unlink_from_block() → refcount 2→1
  → 物理页不释放（refcount > 0）
  → 子进程仍可访问

子进程 exit:
  → free_all() → unlink_from_block() → refcount 1→0
  → 物理页释放
```

### 7.2 directphys 区域的 munmap

```
map_phys(设备地址 0xFE000000, 0x1000):
  → 创建 VirRegion (VR_DIRECT) → VrParam::Direct { phys: 0xFE000000 }
  → 缺页时: ph->ph->phys = 0xFE000000 + offset
  → on_unreference: 空操作（不释放设备内存）

munmap(0xFE000000):
  → free_range() → unlink_from_block() → refcount 1→0
  → on_unreference: 返回 false（不释放物理页）
  → 设备内存不受影响
```

### 7.3 共享内存的 munmap

```
shm 创建:
  进程 A: VirRegion SA → PhysBlock (refcount=2)
  进程 B: VirRegion SB → PhysBlock (refcount=2, 同一个)

进程 A munmap:
  → free_range() → unlink_from_block() → refcount 2→1
  → 物理页不释放

进程 B munmap (或 exit):
  → free_range() → unlink_from_block() → refcount 1→0
  → on_unreference: 释放物理页
```

**统一规律**：无论哪种 memtype，munmap 的物理页释放逻辑都通过 `unlink_from_block()` + `on_unreference()` 统一处理。memtype 通过 `on_unreference()` 的返回值控制是否释放物理页：
- `AnonymousMemory::on_unreference()` → 返回 true → 释放物理页
- `DirectPhysical::on_unreference()` → 返回 false → 不释放物理页
- `SharedMemory::on_unreference()` → 返回 true（最后一个引用时）→ 释放物理页

这就是 memtype 抽象的威力——munmap 不需要知道物理页的类型，只需要调用 `on_unreference()`，由 memtype 决定是否释放。
