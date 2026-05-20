# 14-cow-mechanism: 写时复制 (CoW)

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/pb.c`, `mem_anon.c`  
> **说明**: 基于 phys_block 引用计数的写时复制机制

---

## 1. 概述

**CoW 原理**

写时复制（Copy-on-Write，CoW）是一种内存优化技术，允许多个进程共享同一物理内存页，直到某个进程需要修改时才创建私有副本。

传统方式（fork 时立即复制）：fork() 时将父进程内存完整复制到子进程，问题在于复制大量内存耗时、内存使用翻倍、fork() 延迟高。

CoW 方式（延迟复制）：fork() 时父进程和子进程共享同一物理页，页表项标记为只读，refcount = 2。写入时触发页面保护异常，分配新物理页，复制数据到新页，更新页表映射，原块 refcount-- / 新块 refcount=1。

**CoW 的优势**

1. **节省内存**: 传统 fork 父子进程各需 100MB = 200MB；CoW fork 共享 100MB，只有被修改的页面才需要额外内存
2. **加速 fork**: 传统 fork 需要 O(n) 页面复制；CoW fork 只需设置页表为只读，fork() 从秒级降到毫秒级
3. **按需分配**: 只有实际写入的页面才会被复制；如果子进程立即 exec()，大部分页面无需复制（典型的 shell 工作负载：fork → exec）
4. **语义正确**: 父子进程内存隔离，互不影响；写操作只影响自己的副本

**与 Minix3 的对应关系**

| Minix3 组件 | 作用 | CoW 相关 |
|------------|------|---------|
| `phys_block.refcount` | 引用计数 | refcount > 1 时需要 CoW |
| `mem_cow()` | 执行复制 | 分配新页、复制数据、更新映射 |
| `anon_pagefault()` | 页错误处理 | 检测 CoW 条件并调用 mem_cow() |
| `map_writept()` / `map_ph_writept()` | 页表操作 | fork 后重写页表，共享页自动只读 |
| `pr_writable()` | 可写判断 | 结合 VR_WRITABLE 和 memtype->writable |
| `VR_WRITABLE` | 区域标志 | 标记区域是否可写 |

**CoW 触发流程**

1. **fork 系统调用**: `map_proc_copy_range()` 复制父进程区域，`pb_reference()` 增加引用计数（refcount 1→2），然后 `map_writept()` 重写页表
2. **设置页表只读**: `map_ph_writept()` 通过 `pr_writable()` → `anon_writable()` 检测共享页（refcount>1）返回不可写，页表项不含 `PTF_WRITE`
3. **进程尝试写入**: CPU 触发页面保护异常（#PF），内核将异常传递给 VM
4. **VM 页错误处理**: `anon_pagefault()` 检测 `refcount >= 2 && write`，调用 `mem_cow()`
5. **执行 CoW**: `mem_cow()` 分配新物理页 → `sys_abscopy()` 复制数据 → `pb_unreferenced()` 取消旧引用 → `pb_link()` 链接新块 → `ph->memtype = &mem_type_anon`
6. **更新页表**: `map_pf()` 返回后调用 `map_ph_writept()` 更新页表项，页面变为可写（refcount=1）
7. **继续执行**: 进程拥有私有物理页，写入成功完成

---

## 2. C 源码分析

### 2.1 CoW 触发条件

**两个必要条件**

CoW 需要同时满足两个条件才会触发：

1. **写操作** (`write = 1`): 页错误必须由写操作触发。读操作不会触发 CoW，即使 refcount > 1，多个进程可以安全地读取共享页
2. **引用计数 > 1** (`refcount > 1`): phys_block 被多个 phys_region 引用。refcount = 1 时为私有页无需 CoW，refcount > 1 时为共享页需要 CoW

触发条件 = `write && (refcount > 1)`

**Minix3 源码分析**

```c
/* mem_anon.c: anon_pagefault() */
static int anon_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb, void *state,
    int len, int *io)
{
    phys_bytes new_page, new_page_cl;
    u32_t allocflags;

    allocflags = vrallocflags(region->flags);

    assert(ph->ph->refcount > 0);

    /* 预分配新页（可能用于 CoW 或首次分配） */
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {
        printf("anon_pagefault: out of memory\n");
        return ENOMEM;                    /* ← ENOMEM: 物理内存不足 */
    }
    new_page = CLICK2ABS(new_page_cl);

    /* 情况 1: 延迟分配，首次分配物理页 */
    if(ph->ph->phys == MAP_NONE) {
        ph->ph->phys = new_page;          /* ← 直接使用预分配页 */
        assert(ph->ph->phys != MAP_NONE);
        return OK;
    }

    /* 情况 2: CoW 判断 */
    if(ph->ph->refcount < 2 || !write) {
        /* 不需要 CoW:
         * - refcount < 2: 私有页
         * - !write: 读操作
         */
        /* 注意: 预分配的 new_page 未释放，源码中直接 return OK */
        return OK;                        /* ← ⚠️ 内存泄漏：预分配页未释放 */
    }

    /* 情况 3: 触发 CoW */
    assert(region->flags & VR_WRITABLE);

    return mem_cow(region, ph, new_page_cl, new_page);
}
```

**条件判断详解**

1. `ph->ph->phys == MAP_NONE` → 延迟分配，设置 phys 并返回
2. `ph->ph->refcount < 2 || !write` → 无需 CoW（私有页或读操作），返回 OK
3. 否则 → 触发 CoW，调用 `mem_cow()`

**写保护页机制**

**写保护页机制**（x86-32 Minix3）

fork 时通过 `map_writept()` → `map_ph_writept()` → `pr_writable()` 设置页表只读：

- `pr_writable()` 调用 `memtype->writable()`，对于共享页 `anon_writable()` 返回 0
- `map_ph_writept()` 设置 `PTF_READ` 而非 `PTF_WRITE`，页面变为只读
- 页面仍然存在（`PTF_PRESENT` 始终设置），只是不可写
- 写操作时 CPU 检测到页表项无写权限，触发页面保护异常（#PF, error code bit 1 = 1）
- 内核将异常传递给 VM 处理

> **x86-32 vs x86-64**: x86-32 的 PTE 使用 bit 0 (P), bit 1 (R/W), bit 2 (U/S)；x86-64 的 PTE 结构相同但位宽为 64 位，且增加了 NX 位等。Minix3 的 `PTF_WRITE = I386_VM_WRITE` 对应 x86-32 的 bit 1。

**x86 页错误错误码**（x86-32/x86-64 通用）:

| Bit | 名称 | 含义 |
|-----|------|------|
| 0 | P | 1 = 页面存在，0 = 页面不存在 |
| 1 | W/R | 1 = 写操作触发，0 = 读操作触发 |
| 2 | U/S | 0 = 内核模式，1 = 用户模式 |
| 3 | RSVD | 1 = 保留位设置 |
| 4 | I/D | 1 = 取指触发（x86-64） |

CoW 场景典型错误码: `0x07` (P=1, W/R=1, U/S=1) — 用户态写已存在但只读的页面。

**refcount 状态与 CoW**

| refcount | 状态 | CoW 行为 |
|----------|------|---------|
| 0 | 新创建的 phys_block，尚未被引用（`pb_new()` 初始化为 0） | 不应触发页错误（无映射） |
| 1 | 私有页，只有一个引用者 | 写操作直接执行，无需 CoW |
| ≥2 | 共享页，多个引用者（fork 后） | 写操作触发 CoW |

**特殊情况处理**

```c
/* anon_writable() - 检查区域是否可写 */
static int anon_writable(struct phys_region *pr)
{
    assert(pr->ph->refcount > 0);
    
    /* 延迟分配的页面不可写（尚未分配物理页） */
    if(pr->ph->phys == MAP_NONE)
        return 0;
    
    /* 有 remaps 表示正在 CoW 过程中 */
    if(pr->parent->remaps > 0)
        return 1;
    
    /* 只有私有页才可写 */
    return pr->ph->refcount == 1;
}
```

---

### 2.2 CoW 核心函数

#### 2.2.1 mem_cow - 执行写时复制

**函数签名**

```c
int mem_cow(struct vir_region *region,
    struct phys_region *ph, phys_bytes new_page_cl, phys_bytes new_page);
```

**Minix3 源码实现**

```c
int mem_cow(struct vir_region *region,
        struct phys_region *ph, phys_bytes new_page_cl, phys_bytes new_page)
{
        struct phys_block *pb;

        /* 如果没有预分配新页，现在分配 */
        if(new_page == MAP_NONE) {
                u32_t allocflags;
                allocflags = vrallocflags(region->flags);

                if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM)
                        return ENOMEM;

                new_page = CLICK2ABS(new_page_cl);
        }

        /* 确保原页有物理内存 */
        assert(ph->ph->phys != MAP_NONE);

        /* 复制原页内容到新页 */
        if(sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE) != OK) {
                panic("VM: abscopy failed\n");
                return EFAULT;
        }

        /* 创建新的 phys_block */
        if(!(pb = pb_new(new_page))) {
                free_mem(new_page_cl, 1);
                return ENOMEM;
        }

        /* 取消对旧 phys_block 的引用 */
        pb_unreferenced(region, ph, 0);

        /* 链接到新的 phys_block */
        pb_link(ph, pb, ph->offset, region);

        /* 设置内存类型为匿名 */
        ph->memtype = &mem_type_anon;

        return OK;
}
```

#### 方案 A（PageState + PageSlot）实现

> **方案 A 标注**：方案 A 下，`sys_abscopy` 被 `vm_phys_to_virt() + copy_nonoverlapping()` 替代（Direct Map），`PhysBlock`/`PhysRegion` 被 `PageFrames`/`PageSlot` 替代。CoW 不再需要侵入式链表操作（`pb_link`/`pb_unreferenced`），而是通过 `PageSlot` 的 Copy 语义和 `PageFrames` 的 refcount 操作完成。

```rust
fn mem_cow(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
) -> Result<u32, CowCoreError> {
    let slot = region.get_slot(offset)
        .ok_or(CowCoreError::PageNotMapped)?;

    if !slot.is_mapped() {
        return Err(CowCoreError::PageNotMapped);
    }

    let old_pfn = slot.pfn;
    let refcount = frames.get(old_pfn)
        .map(|s| s.refcount)
        .unwrap_or(0);

    // 防御性检查：refcount <= 1 时无需 CoW
    if refcount <= 1 {
        return Ok(old_pfn);
    }

    // 1. 分配新物理页
    let new_pfn = alloc.alloc_pfn()
        .map_err(|_| CowCoreError::NoMemory)?;

    // 2. 复制数据：通过 Direct Map 访问物理页
    copy_page_content(frames, old_pfn, new_pfn);

    // 3. 取消旧引用：unmap_page 减少 refcount
    let pending = region.unmap_page(frames, offset);

    // 4. 映射新页：CoW 后总是匿名内存（对应 C 中 ph->memtype = &mem_type_anon）
    region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON);

    // 5. 如果旧页 refcount 归零，通知内存类型并释放
    if let Some((pfn, mt)) = pending {
        mt.ev_unreference(frames, pfn);
        alloc.free_pfn(pfn);
    }

    Ok(new_pfn)
}
```

**关键区别**：Minix3 的 `sys_abscopy` 是内核系统调用——VM 无法直接访问物理内存，必须请求内核代劳。方案 A 中 VM 拥有 direct map，`vm_phys_to_virt()` 使物理页直接可操作，复制变成一行 `copy_nonoverlapping`。CoW 逻辑的简化不是"去掉了系统调用开销"，而是 **"VM 不再需要内核作为物理内存访问的中介"**——这与 07 的双视图模型、08 的 GlobalAlloc 自包含是同一个叙事。同时，方案 A 消除了 `pb_link`/`pb_unreferenced` 的侵入式链表操作，CoW 只需 `unmap_page`（递减旧 refcount）+ `map_page`（递增新 refcount）两步。

**执行流程**

1. **确保新页已分配**: `if (new_page == MAP_NONE)` 则调用 `alloc_mem(1, flags)` 分配。如果调用者已预分配（如 `anon_pagefault`），跳过此步骤
2. **复制页面内容**: `sys_abscopy(old_phys, new_phys, VM_PAGE_SIZE)` — 内核系统调用，高效复制 4KB 数据
3. **创建新 phys_block**: `pb = pb_new(new_page)` — 初始化 `pb->phys = new_page`, `pb->refcount = 0`, `pb->flags = 0`, `pb->firstregion = NULL`
4. **取消旧引用**: `pb_unreferenced(region, ph, 0)` — `rm=0` 不从 vir_region 移除 phys_region；效果：`old_pb->refcount--`，从 `old_pb->firstregion` 链表移除 ph，`ph->ph = NULL`（临时）
5. **链接新 phys_block**: `pb_link(ph, pb, ph->offset, region)` — 效果：`ph->ph = pb`, `pb->refcount = 1`，ph 加入 `pb->firstregion` 链表
6. **设置内存类型**: `ph->memtype = &mem_type_anon` — CoW 后总是变成匿名内存，即使原来是文件映射

**状态变化**

| | mem_cow 前 | mem_cow 后 |
|---|---|---|
| phys_region (ph) | ph → old_pb, memtype: mem_type_anon | ph → new_pb, memtype: mem_type_anon |
| old_pb | phys: 0x1234000, refcount: 2, firstregion → parent_pr → child_pr | phys: 0x1234000, refcount: 1, firstregion → other_pr（如果 refcount > 0）或已释放（refcount = 0） |
| new_pb | 不存在 | phys: 0x5678000（新页）, refcount: 1, firstregion → ph |

**关键点说明**

1. **`pb_unreferenced(region, ph, 0)` 的 `rm=0` 参数**: rm=0 表示不从 vir_region 移除 phys_region，因为 phys_region 还要继续使用，只是换一个 pb。如果 rm=1，physblock_set 会清除引用
2. **memtype 变为匿名**: CoW 后总是 `mem_type_anon`，因为私有副本不再与文件关联，修改不会写回原文件，后续页错误由 `anon_pagefault` 处理
3. **`sys_abscopy` 系统调用**：内核提供的物理内存复制接口，直接操作物理地址，无需映射到虚拟地址空间，内核可使用优化的复制例程。**方案四中不再需要**：VM 拥有 direct map，`vm_phys_to_virt()` 使物理页直接可操作，`sys_abscopy` 的角色被 `copy_nonoverlapping` 替代。`sys_abscopy` 存在的根本原因是 VM 无法直接访问物理内存——direct map 消除了这个限制
4. **错误处理**: ENOMEM 无法分配新页；EFAULT 复制失败（不应该发生）。失败时需要释放已分配的资源

---

#### 2.2.2 pb_reference - 共享时增加引用

**fork 时的共享机制**

fork 系统调用创建子进程时，需要让子进程共享父进程的物理页。这是通过 `pb_reference` 实现的。

**函数签名**

```c
struct phys_region *pb_reference(
    struct phys_block *newpb,
    vir_bytes offset,
    struct vir_region *region,
    mem_type_t *memtype
);
```

**Minix3 源码实现**

```c
struct phys_region *pb_reference(struct phys_block *newpb,
    vir_bytes offset, struct vir_region *region, mem_type_t *memtype)
{
    struct phys_region *newphysr;

    /* 分配 phys_region 结构体 */
    if(!SLABALLOC(newphysr)) {
        printf("vm: pb_reference: couldn't allocate phys region\n");
        return NULL;
    }

    /* 设置内存类型 */
    newphysr->memtype = memtype;

    /* 链接到 phys_block */
    pb_link(newphysr, newpb, offset, region);

    /* 记录在 vir_region 的 physblocks 数组中 */
    physblock_set(region, offset, newphysr);

    return newphysr;
}

void pb_link(struct phys_region *newphysr, struct phys_block *newpb,
    vir_bytes offset, struct vir_region *parent)
{
    /* 设置 phys_region 字段 */
    newphysr->offset = offset;
    newphysr->ph = newpb;
    newphysr->parent = parent;

    /* 头插法加入链表 */
    newphysr->next_ph_list = newpb->firstregion;
    newpb->firstregion = newphysr;

    /* 增加引用计数 */
    newpb->refcount++;
}
```

**fork 时的调用流程**

1. **父进程状态**: 父进程 vir_region 的 physblocks[0] → phys_region_p → phys_block (phys: 0x1234000, refcount: 1)
2. **fork 创建子进程区域**: `map_copy_region()` 对每个 phys_region 调用 `pb_reference(ph->ph, offset, child_vr, memtype)`
3. **共享后的状态**: 父子进程的 phys_region 都指向同一 phys_block (phys: 0x1234000, refcount: 2, firstregion → child_pr → parent_pr)

**关键点**

1. **共享而非复制**: 子进程创建新的 phys_region 但指向同一个 phys_block，物理内存不复制，只增加引用计数
2. **memtype 继承**: 子进程的 phys_region 使用相同的 memtype（如父进程是匿名内存，子进程也是匿名内存）
3. **offset 保持一致**: 子进程的 `phys_region.offset` 与父进程相同，因为虚拟地址空间布局相同
4. **链表顺序**: 头插法，新引用在链表头部（firstregion → child_pr → parent_pr）

**与 CoW 的关系**

`pb_reference` 建立共享，为 CoW 创造条件：

1. fork 时调用 `pb_reference` → refcount 从 1 变为 2 → 页表设置为只读
2. 写入时触发 CoW → 检测 `refcount > 1` → 执行 `mem_cow`
3. CoW 后 → 原块 refcount 减为 1 → 新块 refcount = 1 → 各自独立
```

#### 2.2.3 pb_unreferenced - 解除共享

**函数作用**

`pb_unreferenced` 用于解除 phys_region 对 phys_block 的引用。在 CoW 场景中，它用于减少原物理块的引用计数。

**Minix3 源码实现**

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb;

    pb = pr->ph;
    assert(pb->refcount > 0);
    
    /* 减少引用计数 */
    pb->refcount--;

    /* 从链表中移除 phys_region */
    if(pb->firstregion == pr) {
        /* pr 是链表头 */
        pb->firstregion = pr->next_ph_list;
    } else {
        /* pr 在链表中间，需要遍历查找 */
        struct phys_region *others;

        for(others = pb->firstregion; others;
            others = others->next_ph_list) {
            assert(others->ph == pb);
            if(others->next_ph_list == pr) {
                others->next_ph_list = pr->next_ph_list;
                break;
            }
        }

        assert(others); /* 否则说明 pr 不在链表中 */
    }

    /* 如果引用计数为 0，释放 phys_block */
    if(pb->refcount == 0) {
        assert(!pb->firstregion);
        int r;
        if((r = pr->memtype->ev_unreference(pr)) != OK)
            panic("unref failed, %d", r);

        SLABFREE(pb);
    }

    pr->ph = NULL;

    /* 如果 rm 为真，从 vir_region 中移除 */
    if(rm) physblock_set(region, pr->offset, NULL);
}
```

**rm 参数在 CoW 中的作用**

| rm 值 | 场景 | 调用位置 | 效果 |
|-------|------|---------|------|
| 0 | CoW | `mem_cow()` 中 `pb_unreferenced(region, ph, 0)` | 减少 refcount、从链表移除 ph、`ph->ph = NULL`，但 `physblock_set` 不被调用（phys_region 还要继续使用，后续会 `pb_link` 到新的 phys_block） |
| 1 | 释放 | munmap/exit 中 `pb_unreferenced(region, ph, 1)` | 减少 refcount、从链表移除 ph、`ph->ph = NULL`，且 `physblock_set(region, offset, NULL)` 被调用（phys_region 不再使用，需要从 vir_region 中清除引用） |

**CoW 中的调用序列**

```c
/* mem_cow() 中的调用 */
int mem_cow(struct vir_region *region,
    struct phys_region *ph, phys_bytes new_page_cl, phys_bytes new_page)
{
    struct phys_block *pb;
    
    /* ... 分配新页、复制数据 ... */
    
    /* 1. 取消对旧块的引用 (rm=0) */
    pb_unreferenced(region, ph, 0);
    /* 此时:
     *   - old_pb->refcount--
     *   - ph->ph = NULL
     *   - 但 vir_region->physblocks[offset] 仍然指向 ph
     */
    
    /* 2. 链接到新块 */
    pb_link(ph, pb, ph->offset, region);
    /* 此时:
     *   - ph->ph = new_pb
     *   - new_pb->refcount = 1
     *   - vir_region->physblocks[offset] 仍然指向 ph
     */
    
    return OK;
}
```

**链表操作详解**

**链表移除操作**

情况 1: pr 是链表头
- 移除前: `pb->firstregion → pr → pr2 → pr3 → NULL`
- 移除后: `pb->firstregion → pr2 → pr3 → NULL`
- 代码: `pb->firstregion = pr->next_ph_list;`

情况 2: pr 在链表中间
- 移除前: `pb->firstregion → pr1 → pr → pr3 → NULL`
- 移除后: `pb->firstregion → pr1 → pr3 → NULL`
- 代码: 遍历链表找到 pr 的前驱，修改 `others->next_ph_list = pr->next_ph_list`

**refcount 归零处理**

```c
if(pb->refcount == 0) {
    assert(!pb->firstregion);  /* 链表应为空 */
    
    /* 调用 memtype 的释放回调 */
    int r;
    if((r = pr->memtype->ev_unreference(pr)) != OK)
        panic("unref failed, %d", r);

    /* 释放 phys_block 结构体 */
    SLABFREE(pb);
}
```

**ev_unreference 回调**

```c
/* mem_anon.c: 匿名内存的释放回调 */
static int anon_unreference(struct phys_region *pr)
{
    assert(pr->ph->refcount == 0);
    
    /* 释放物理页 */
    if(pr->ph->phys != MAP_NONE)
        free_mem(ABS2CLICK(pr->ph->phys), 1);
    
    return OK;
}
```

**CoW 完整流程中的 pb_unreferenced**

| 阶段 | phys_block 状态 | refcount | firstregion |
|------|----------------|----------|-------------|
| 初始（fork 后） | 共享块 | 2 | child_pr → parent_pr → NULL |
| `pb_unreferenced(child_region, child_pr, 0)` 后 | 原块 | 1 | parent_pr → NULL |
| `pb_link(child_pr, new_pb, ...)` 后 | 新块 | 1 | child_pr → NULL |

CoW 后：原块变为父进程私有（refcount=1），新块为子进程私有（refcount=1）。

---

### 2.3 页表操作

#### 2.3.1 设置只读

**fork 时的只读设置**

fork 创建子进程后，需要将共享页的页表项设置为只读，以便写入时触发 CoW。

**Minix3 实现**

Minix3 中有两个层次的函数：

1. `map_writept(vmp)` — 遍历进程的所有 vir_region 和 phys_region，对每个调用 `map_ph_writept`
2. `map_ph_writept(vmp, vr, pr)` — 对单个 phys_region 写入页表项

```c
/* region.c: map_writept() — 遍历所有区域 (region.c:906) */
int map_writept(struct vmproc *vmp)
{
    struct vir_region *vr;
    struct phys_region *ph;
    int r;
    region_iter v_iter;
    region_start_iter_least(&vmp->vm_regions_avl, &v_iter);

    while((vr = region_get_iter(&v_iter))) {
        vir_bytes p;
        for(p = 0; p < vr->length; p += VM_PAGE_SIZE) {
            if(!(ph = physblock_get(vr, p))) continue;

            if((r=map_ph_writept(vmp, vr, ph)) != OK) {
                printf("VM: map_writept: failed\n");
                return r;
            }
        }
        region_incr_iter(&v_iter);
    }

    return OK;
}

/* region.c: map_ph_writept() — 单个 phys_region 的页表写入 (region.c:257) */
int map_ph_writept(struct vmproc *vmp, struct vir_region *vr,
    struct phys_region *pr)
{
    int flags = PTF_PRESENT | PTF_USER;
    struct phys_block *pb = pr->ph;

    assert(vr);
    assert(pr);
    assert(pb);

    assert(!(vr->vaddr % VM_PAGE_SIZE));
    assert(!(pr->offset % VM_PAGE_SIZE));
    assert(pb->refcount > 0);

    /* 通过 pr_writable() 判断是否可写 */
    if(pr_writable(vr, pr))
        flags |= PTF_WRITE;
    else
        flags |= PTF_READ;

    if(vr->def_memtype->pt_flags)
        flags |= vr->def_memtype->pt_flags(vr);

    if(pt_writemap(vmp, &vmp->vm_pt, vr->vaddr + pr->offset,
            pb->phys, VM_PAGE_SIZE, flags,
            WMF_OVERWRITE) != OK) {
        printf("VM: map_writept: pt_writemap failed\n");
        return ENOMEM;
    }

    return OK;
}

/* region.c: pr_writable() — 判断 phys_region 是否可写 (region.c:130) */
static int pr_writable(struct vir_region *vr, struct phys_region *pr)
{
    assert(pr->memtype->writable);
    return ((vr->flags & VR_WRITABLE) && pr->memtype->writable(pr));
}
```

> **注意**: Minix3 使用 `PTF_PRESENT`/`PTF_WRITE`/`PTF_READ`/`PTF_USER` 标志（定义在 `arch/i386/pagetable.h`），而非 `ARCH_VM_PTE_*` 宏。`ARCH_VM_PTE_*` 是更底层的硬件标志。对于 x86-32，`PTF_WRITE = I386_VM_WRITE`，`PTF_READ = I386_VM_READ`。CoW 只读机制不是直接检查 `refcount > 1`，而是通过 `pr_writable()` → `memtype->writable()` 间接实现：`anon_writable()` 在 `refcount > 1` 时返回 0，从而使页表项不含 `PTF_WRITE`。

**页表标志说明**（x86-32 Minix3）

| 标志 | 含义 | CoW 中的作用 |
|------|------|-------------|
| `PTF_PRESENT` | 页面存在 | 始终设置，CoW 不影响此位 |
| `PTF_USER` | 用户可访问 | 用户态页面始终设置 |
| `PTF_WRITE` | 可写 | 共享页(refcount>1)时清除，触发写保护异常 |
| `PTF_READ` | 只读 | 共享页时设置此位（x86-32 上 PTF_READ = I386_VM_READ） |

> **x86-32 vs x86-64 差异**: Minix3 原始代码运行在 x86-32 上，PTE 为 32 位，使用 2 级页表（PD+PT）。minix-rs 运行在 x86-64 上，PTE 为 64 位，使用 4 级页表（PML4+PDPT+PD+PT）。PTE 标志位的位置和含义在 x86-64 上有所不同（如 bit 1 仍为 R/W，但 bit 位宽和保留位不同）。在 x86-64 上，`PTF_READ`/`PTF_WRITE` 的映射需要重新定义。

**只读设置流程**

1. `map_proc_copy_range()` 完成区域复制后，调用 `map_writept(src)` 和 `map_writept(dst)`（region.c:995-996）
2. `map_writept()` 遍历进程所有 vir_region 和 phys_region，对每个调用 `map_ph_writept()`
3. `map_ph_writept()` 通过 `pr_writable()` 判断是否可写：
   - `pr_writable()` = `(vr->flags & VR_WRITABLE) && pr->memtype->writable(pr)`
   - 对于共享页，`anon_writable()` 返回 0（因为 `refcount > 1`），所以 `pr_writable()` 返回 0
   - 页表项设置 `PTF_READ` 而非 `PTF_WRITE`，页面变为只读
4. 两个进程都指向同一物理页，写入任一进程都会触发页面保护异常

#### 2.3.2 更新映射

**CoW 后更新页表**

CoW 完成后，需要更新页表映射，将虚拟地址指向新的物理页。

**Minix3 实现**

`mem_cow()` 本身不更新页表。页表更新由页错误处理的调用链完成：

1. `pagefaults.c` 中的 `handle_pagefault()` 是顶层页错误分发函数，负责：验证地址合法性（`map_lookup` 查找区域）、检查区域可写性（`VR_WRITABLE`）、计算 offset、调用 `map_pf()` 处理页错误、处理 SUSPEND（VFS 异步回调）和错误（SIGSEGV）
2. `map_pf()` 先检查 `physblock_get`：如果不存在则 `pb_new + pb_reference` 创建新块；然后检查 `!write || !ph->memtype->writable(ph)` 决定是否调用 `ev_pagefault`；最后调用 `map_ph_writept()` 更新页表
3. `anon_pagefault()` 在 CoW 场景下调用 `mem_cow()` 并返回 OK
4. `map_pf()` 返回后，调用者会调用 `map_ph_writept()` 更新该页的页表项

```c
/* pagefaults.c: 页错误处理调用链（简化） */
int map_pf(struct vmproc *vmp, struct vir_region *region,
    vir_bytes v, int write, ...)
{
    struct phys_region *ph;
    /* ... 查找 phys_region ... */

    /* 调用内存类型的页错误处理 */
    if((r = region->def_memtype->ev_pagefault(vmp, region, ph,
            write, cb, state, len, io)) != OK) {
        return r;
    }

    /* 更新页表映射 */
    if((r = map_ph_writept(vmp, region, ph)) != OK)
        return r;

    return OK;
}
```

**pt_writemap 函数**

```c
/* pagetable.c */
int pt_writemap(struct vmproc *vmp, pt_t *pt, vir_bytes v,
    phys_bytes physaddr, size_t bytes, u32_t flags, u32_t writemapflags)
{
    /* 写入页表项 */
    /* v: 虚拟地址 */
    /* physaddr: 物理地址 */
    /* flags: 页表标志 */
    /* writemapflags: 写入模式 */
    
    /* WMF_OVERWRITE: 允许覆盖已有映射 */
    /* WMF_FREE: 释放被覆盖的物理页 */
    /* WMF_VERIFY: 验证页表内容 */
    
    /* ... */
}
```

**页表更新流程**

| 阶段 | 父进程页表 | 子进程页表 |
|------|-----------|-----------|
| CoW 前 | vaddr 0x400000 → phys 0x1234000 (只读) | vaddr 0x400000 → phys 0x1234000 (只读) |
| CoW 执行 | 不变 | 分配新页 0x5678000，`sys_abscopy` 复制数据，更新 phys_block 引用 |
| CoW 后 | vaddr 0x400000 → phys 0x1234000 (refcount=1，`map_ph_writept` 后可能变为可写) | vaddr 0x400000 → phys 0x5678000 (可写，refcount=1，私有页) |

**TLB 刷新**

CoW 后页表映射改变，需要刷新 TLB。Minix3 中 `pt_writemap()` 使用 `WMF_OVERWRITE` 标志，内部会调用 `pt_flush()` 刷新对应进程的 TLB 条目。

TLB 刷新方式：
1. **单进程刷新**: `invlpg(vaddr)` — 刷新单个页的 TLB 条目，只刷新当前进程的 TLB 条目
2. **全局刷新**: `reload_cr3()` — 重新加载页表基址，刷新所有 TLB 条目

CoW 场景：只需要刷新触发 CoW 的进程的 TLB，其他共享进程的 TLB 不受影响（它们仍然指向原物理页，只读）。

---

### 2.4 mem_anon 实现

#### 2.4.1 anon_cow - 匿名内存 CoW

**mem_type_anon 结构体**

Minix3 使用函数指针表实现多态，`mem_type_anon` 是匿名内存的类型定义：

```c
/* mem_anon.c */
struct mem_type mem_type_anon = {
    .name = "anonymous memory",
    .ev_unreference = anon_unreference,
    .ev_pagefault = anon_pagefault,
    .ev_resize = anon_resize,
    .ev_sanitycheck = anon_sanitycheck,
    .ev_lowshrink = anon_lowshrink,
    .ev_split = anon_split,
    .regionid = anon_regionid,
    .writable = anon_writable,
    .refcount = anon_refcount,
    .pt_flags = anon_pt_flags,
};
```

**anon_pagefault - 页错误处理**

```c
static int anon_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb, void *state,
    int len, int *io)
{
    phys_bytes new_page, new_page_cl;
    u32_t allocflags;

    allocflags = vrallocflags(region->flags);

    assert(ph->ph->refcount > 0);

    /* 预分配新页（可能用于 CoW 或首次分配） */
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {
        printf("anon_pagefault: out of memory\n");
        return ENOMEM;
    }
    new_page = CLICK2ABS(new_page_cl);

    /* 情况 1: 延迟分配，首次分配物理页 */
    if(ph->ph->phys == MAP_NONE) {
        ph->ph->phys = new_page;
        assert(ph->ph->phys != MAP_NONE);
        return OK;
    }

    /* 情况 2: 不需要 CoW */
    if(ph->ph->refcount < 2 || !write) {
        /* 私有页或读操作，无需复制 */
        /* 注意: 预分配的 new_page 未释放，源码中直接 return OK */
        return OK;
    }

    /* 情况 3: 触发 CoW */
    assert(region->flags & VR_WRITABLE);

    return mem_cow(region, ph, new_page_cl, new_page);
}
```

**CoW 判断逻辑**

1. **预分配新页**: `new_page_cl = alloc_mem(1, flags)` — 预先分配，避免 CoW 时内存不足
2. **检查是否延迟分配**: `if (ph->ph->phys == MAP_NONE)` → 首次分配，直接使用预分配的页，return OK
3. **检查是否需要 CoW**: `if (ph->ph->refcount < 2 || !write)` → 私有页或读操作，无需 CoW，return OK（注意：源码中预分配的页未释放）
4. **执行 CoW**: `return mem_cow(region, ph, new_page_cl, new_page)` — 使用预分配的页进行复制

**anon_writable - 可写检查**

```c
static int anon_writable(struct phys_region *pr)
{
    assert(pr->ph->refcount > 0);
    
    /* 延迟分配的页面不可写 */
    if(pr->ph->phys == MAP_NONE)
        return 0;
    
    /* 正在 CoW 过程中，可写 */
    if(pr->parent->remaps > 0)
        return 1;
    
    /* 只有私有页才可写 */
    return pr->ph->refcount == 1;
}
```

**anon_unreference - 释放回调**

```c
static int anon_unreference(struct phys_region *pr)
{
    assert(pr->ph->refcount == 0);
    
    /* 释放物理页 */
    if(pr->ph->phys != MAP_NONE)
        free_mem(ABS2CLICK(pr->ph->phys), 1);
    
    return OK;
}
```

#### 2.4.2 其他类型的 CoW 处理

**文件映射的 CoW**

Minix3 中文件映射由 `mem_type_mappedfile`（mem_file.c）处理，而非 `mem_type_cache`。`mem_type_cache` 是内核文件缓存，其 `cache_pagefault` 逻辑完全不同。

```c
/* mem_file.c: 文件映射内存类型 (mem_file.c:30) */
struct mem_type mem_type_mappedfile = {
    .name = "file-mapped memory",
    .ev_unreference = mappedfile_unreference,
    .ev_pagefault = mappedfile_pagefault,
    .ev_sanitycheck = mappedfile_sanitycheck,
    .ev_copy = mappedfile_copy,
    .writable = mappedfile_writable,
    .ev_split = mappedfile_split,
    .ev_lowshrink = mappedfile_lowshrink,
    .ev_delete = mappedfile_delete,
    .pt_flags = mappedfile_pt_flags,
};

/* mem_file.c: mappedfile_writable() (mem_file.c:173) */
static int mappedfile_writable(struct phys_region *pr)
{
    /* 文件映射页始终不可写，任何写入都触发页错误 */
    return 0;
}

/* mem_file.c: cow_block() (mem_file.c:59) */
static int cow_block(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, u16_t clearend)
{
    int r;

    if((r=mem_cow(region, ph, MAP_NONE, MAP_NONE)) != OK) {
        printf("mappedfile_pagefault: COW failed\n");
        return r;
    }

    /* CoW 后变成匿名内存，修改不再写回文件 */
    ph->memtype = &mem_type_anon;

    /* ... clearend 处理 ... */

    return OK;
}

/* mem_file.c: mappedfile_pagefault() (mem_file.c:84) */
static int mappedfile_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb,
    void *state, int statelen, int *io)
{
    /* ... 缓存查找和 VFS 请求 ... */

    if(!write) {
        return OK;
    }

    /* 写操作触发 CoW */
    return cow_block(vmp, region, ph, 0);
}
```

> **注意**: `mem_type_cache`（mem_cache.c）是 VM 内部的文件缓存机制，其 `cache_pagefault` 逻辑是链接预分配的缓存页，与 CoW 无关。文档之前的版本错误地将 `mem_type_cache` 描述为文件映射的 CoW 处理。

**共享内存的 CoW**

```c
/* mem_shared.c: 共享内存类型 (mem_shared.c:28) */
struct mem_type mem_type_shared = {
    .name = "shared memory",
    .ev_copy = shared_copy,
    .ev_unreference = shared_unreference,
    .ev_pagefault = shared_pagefault,
    .ev_sanitycheck = shared_sanitycheck,
    .ev_delete = shared_delete,
    .regionid = shared_regionid,
    .refcount = shared_refcount,
    .writable = shared_writable,
    .pt_flags = shared_pt_flags,
};

/* mem_shared.c: shared_pagefault() (mem_shared.c:122) */
static int shared_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb,
    void *state, int statelen, int *io)
{
    struct vir_region *src_region;
    struct vmproc *src_vmp;
    struct phys_region *pr;

    if(getsrc(region, &src_vmp, &src_region) != OK) {
        return EINVAL;
    }

    if(ph->ph->phys != MAP_NONE) {
        /* 物理页已存在，无需处理 */
        return OK;
    }

    /* 物理页不存在，从源进程获取 */
    pb_free(ph->ph);

    if(!(pr = physblock_get(src_region, ph->offset))) {
        /* 源进程也没有，触发源进程的页错误 */
        int r;
        if((r=map_pf(src_vmp, src_region, ph->offset, write,
            NULL, NULL, 0, io)) != OK)
            return r;
        if(!(pr = physblock_get(src_region, ph->offset))) {
            panic("missing region after pagefault handling");
        }
    }

    /* 链接到源进程的 phys_block，共享物理页 */
    pb_link(ph, pr->ph, ph->offset, region);

    return OK;
}

/* mem_shared.c: shared_writable() (mem_shared.c:161) */
static int shared_writable(struct phys_region *pr)
{
    assert(pr->ph->refcount > 0);
    /* 共享内存只要物理页存在就可写 */
    return pr->ph->phys != MAP_NONE;
}
```

> **关键区别**: 共享内存的 `shared_pagefault` 不执行 CoW，而是通过 `pb_link` 直接链接到源进程的 `phys_block`，实现真正的共享（所有修改可见）。共享内存的 `writable` 只要物理页存在就返回可写，不受 `refcount` 限制。

**内存类型对比**

| 内存类型 | CoW 支持 | CoW 后类型 | writable 逻辑 | 说明 |
|---------|---------|-----------|--------------|------|
| 匿名内存 (`mem_type_anon`) | 是 | 匿名内存 | `refcount == 1` 时可写 | 标准行为 |
| 文件映射 (`mem_type_mappedfile`) | 是 | 匿名内存 | 始终返回 0（不可写） | 写入必触发 CoW |
| 共享内存 (`mem_type_shared`) | 否 | 共享内存 | `phys != MAP_NONE` 即可写 | 所有修改可见 |
| 缓存内存 (`mem_type_cache`) | 否 | 缓存内存 | `phys != MAP_NONE` 即可写 | VM 内部缓存 |

---

## 3. Rust 设计决策

### 3.1 CoW 安全实现

**安全挑战**

CoW 涉及多个进程共享同一物理页，需要安全地处理并发访问：

**CoW 安全挑战**

1. **并发写入**: 多个进程可能同时尝试写入同一共享页，需要确保只有一个进程执行 CoW，其他进程等待或重试
2. **引用计数一致性**: `PageState.refcount` 必须与实际引用数一致，增减操作必须成对出现，不会出现悬垂指针
3. **页表同步**: 页表更新需要与 CoW 操作同步，页表更新在 CoW 完成后，TLB 刷新及时

**方案 A 下的 Rust 解决方案**

方案 A 使用全局 `PageFrames`（`Vec<PageState>`）按 PFN 索引管理物理页状态，`VirRegion.physblocks: Vec<Option<PageSlot>>` 管理映射。CoW 操作不再需要侵入式链表（`pb_link`/`pb_unreferenced`），而是通过 `PageSlot` 的 Copy 语义和 `PageFrames` 的 refcount 操作完成。

```rust
/// CoW 状态机（设计参考，未实现）
///
/// 当前实现通过 PageFrames.refcount 隐式判断 CoW 状态：
/// refcount == 1 → Private, refcount > 1 → Shared。
/// CowState 可在后续迭代中用于显式状态跟踪。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CowState {
    /// 私有页，可直接写入
    Private,
    /// 共享页，需要 CoW
    Shared,
    /// 正在执行 CoW
    Copying,
}

/// VirRegion 的 CoW 支持
impl VirRegion {
    /// 检查指定偏移处的页是否需要 CoW
    pub(crate) fn needs_cow(&self, frames: &PageFrames, offset: VirBytes) -> bool {
        match self.get_slot(offset) {
            Some(slot) if slot.is_mapped() => {
                frames.get(slot.pfn)
                    .map(|s| s.refcount > 1)
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// 执行 CoW：分配新物理页，复制数据，替换 PageSlot
    ///
    /// 实际实现为独立函数 `cow_resolve_core`，此处保留为 VirRegion 方法
    /// 的设计参考。实际调用链：handle_pagefault → cow_resolve → cow_resolve_core。
    ///
    /// # Errors
    ///
    /// - `CowCoreError::NoMemory`: 物理页分配失败
    /// - `CowCoreError::PageNotMapped`: 页面未映射
    pub(crate) fn cow_resolve_core(
        region: &mut VirRegion,
        frames: &mut PageFrames,
        alloc: &mut dyn PfnAllocator,
        offset: VirBytes,
    ) -> Result<u32, CowCoreError> {
        let slot = region.get_slot(offset)
            .ok_or(CowCoreError::PageNotMapped)?;

        if !slot.is_mapped() {
            return Err(CowCoreError::PageNotMapped);
        }

        let old_pfn = slot.pfn;
        let refcount = frames.get(old_pfn)
            .map(|s| s.refcount)
            .unwrap_or(0);

        // 防御性检查：refcount <= 1 时无需 CoW
        if refcount <= 1 {
            return Ok(old_pfn);
        }

        // 1. 分配新物理页
        let new_pfn = alloc.alloc_pfn()
            .map_err(|_| CowCoreError::NoMemory)?;

        // 2. 复制数据：通过 Direct Map 访问物理页
        copy_page_content(frames, old_pfn, new_pfn);

        // 3. 取消旧引用：unmap_page 减少 refcount
        let pending = region.unmap_page(frames, offset);
        // 对于 CoW，旧页 refcount > 0（其他进程仍在引用），所以 pending 通常为 None

        // 4. 映射新页：CoW 后总是匿名内存（对应 C 中 ph->memtype = &mem_type_anon）
        region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON);

        // 5. 如果旧页 refcount 归零，通知内存类型并释放
        if let Some((pfn, mt)) = pending {
            mt.ev_unreference(frames, pfn);
            alloc.free_pfn(pfn);
        }

        Ok(new_pfn)
    }
}
```

**MemType trait 集成**

```rust
/// 内存类型 trait（详见 12-memtype.md）
///
/// 对应 Minix3: `struct mem_type`
pub trait MemType: Send + Sync {
    fn name(&self) -> &'static str;

    fn ev_pagefault(
        &self,
        proc: &ActiveProc<'_>,
        region: &mut VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError>;

    fn ev_unreference(&self, frames: &mut PageFrames, pfn: u32);

    fn ev_reference(&self, frames: &mut PageFrames, slot: PageSlot) -> Result<(), MemTypeError>;

    fn writable(&self, frames: &PageFrames, slot: PageSlot, region: &VirRegion) -> bool;
}

/// 匿名内存类型
pub struct AnonymousMemory;

impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        let slot = region.get_slot(offset);

        match slot {
            None | Some(s) if !s.is_mapped() => {
                return Ok(PagefaultResult::NeedNewPage);
            }
            _ => {}
        }

        let slot = slot.unwrap();
        let refcount = frames.get(slot.pfn)
            .map(|s| s.refcount)
            .unwrap_or(0);

        if refcount < 2 || !write {
            return Ok(PagefaultResult::Handled);
        }

        if !region.is_writable() {
            return Ok(PagefaultResult::AccessViolation);
        }

        Ok(PagefaultResult::NeedCow)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {
        // Physical page freeing is the caller's responsibility via PfnAllocator::free_pfn().
    }

    fn writable(&self, frames: &PageFrames, slot: PageSlot, region: &VirRegion) -> bool {
        if !slot.is_mapped() {
            return false;
        }
        if region.remaps > 0 {
            return true;
        }
        frames.get(slot.pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }
}
```

> **设计说明**：`ev_pagefault` 返回 `PagefaultResult` 而非直接执行 CoW，实现了**判断与执行的分离**。C 中 `anon_pagefault` 预分配新页再判断是否需要 CoW，Rust 中先判断再由 `handle_pagefault` 统一分配——避免了 C 中预分配但不需要 CoW 时内存泄漏的问题。`writable` 包含 `region.remaps > 0` 检查，对应 C 中 `anon_writable` 的 `pr->parent->remaps > 0` 分支。

---

## 4. 实现详解

### 4.1 CoW 流程

**完整流程**

1. **页错误发生**: CPU 写入只读页 → 页面保护异常 (#PF) → 内核捕获异常 → 传递给 VM
2. **VM 处理**: `map_pf(vmp, vaddr, write)` → 查找 VirRegion（AVL 树查找）→ 计算 offset → 查找 PageSlot（Vec 索引）
3. **检查 CoW 条件**: `if (write && frames.get(slot.pfn).refcount > 1)` → 需要 CoW，否则不需要
4. **执行 CoW**: `region.mem_cow(frames, offset, new_pfn)` → 分配新物理页 → 复制数据 (4KB) → unmap_page 减少旧页 refcount → map_page 增加新页 refcount
5. **更新页表**: `map_writept(vmp, region, frames, offset)` → 更新 PTE 指向新物理页，设置可写 → 刷新 TLB
6. **恢复执行**: 返回用户态，重新执行写入指令。此时页面已私有，写入成功

**Rust 实现**

```rust
/// VM 页错误处理入口
pub(crate) fn handle_pagefault(
    proc: &ActiveProc<'_>,
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    fault_addr: VirBytes,
    write: bool,
) -> Result<PagefaultAction, CowError> {
    let offset = VirBytes(fault_addr.0 - region.vaddr.0);

    let memtype = region.def_memtype
        .ok_or(CowError::NoMemType)?;

    let result = memtype.ev_pagefault(proc, region, frames, offset, write)?;

    match result {
        PagefaultResult::Handled => Ok(PagefaultAction::Handled),
        PagefaultResult::NeedNewPage => {
            alloc_and_map(region, frames, alloc, offset, memtype)?;
            Ok(PagefaultAction::MappedNewPage)
        }
        PagefaultResult::NeedCow => {
            cow_resolve(region, frames, alloc, offset)?;
            Ok(PagefaultAction::CowResolved)
        }
        PagefaultResult::AccessViolation => {
            Ok(PagefaultAction::AccessViolation)
        }
    }
}
```

> **与 C 的差异**：C 中 `map_pf()` 直接调用 `ev_pagefault`（内含分配和 CoW 逻辑），然后调用 `map_ph_writept()` 更新页表。Rust 中 `handle_pagefault` 将 `ev_pagefault` 的结果分为四种动作，由上层统一调度——`NeedNewPage` 调用 `alloc_and_map`，`NeedCow` 调用 `cow_resolve`。页表更新由调用者在 `handle_pagefault` 返回后执行。

**批量 CoW 解析**

`cow_resolve_region` 遍历区域的所有页槽位，对需要 CoW 的页执行 `cow_resolve`，返回已解析的页数。用于 exec 等需要一次性解析整个区域的场景。

```rust
pub(crate) fn cow_resolve_region(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
) -> Result<usize, CowError> {
    let num_pages = region.physblocks.len();
    let mut resolved = 0;
    for i in 0..num_pages {
        let offset = VirBytes((i as u64) * PAGE_SIZE);
        if region.needs_cow(frames, offset) {
            cow_resolve(region, frames, alloc, offset)?;
            resolved += 1;
        }
    }
    Ok(resolved)
}
```

**fork 辅助函数**

`cow_copy_page` 是 fork 后 CoW 解析的辅助函数，封装 `cow_resolve_core` 并将 `CowCoreError` 映射为 `ForkError`：

```rust
pub(crate) fn cow_copy_page(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
) -> Result<(), ForkError> {
    cow_resolve_core(region, frames, alloc, offset)
        .map(|_| ())
        .map_err(|e| match e {
            CowCoreError::NoMemory => ForkError::NoMemory,
            CowCoreError::PageNotMapped => ForkError::PageNotMapped,
        })
}
```

### 4.2 物理页复制

**复制操作**

```rust
/// 复制物理页内容
///
/// # Safety
///
/// - src 和 dst 必须是有效的物理地址
/// - 两个地址都必须页对齐
pub unsafe fn copy_phys_page(src: PhysBytes, dst: PhysBytes) -> Result<(), i32> {
    // 通过 direct map 直接复制（VM 拥有物理内存的完整映射）
    let src_ptr = vm_phys_to_virt(src) as *const u8;
    let dst_ptr = vm_phys_to_virt(dst) as *mut u8;
    core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, PAGE_SIZE as usize);
    
    Ok(())
}
```

**优化可能性**

1. **使用 SIMD 指令**: AVX/SSE 指令可以一次复制 32/16 字节。4KB 页面：普通 memcpy ~128 次循环，AVX-256 ~16 次循环
2. **非临时存储 (Non-temporal Store)**: 绕过缓存，避免污染 L1/L2。适用于数据不会被立即使用、避免缓存污染的场景
3. **预取 (Prefetch)**: 预取下一页到缓存，如果连续触发多个 CoW 可以预取
4. **延迟复制**: 如果页面全为零，直接分配零页；如果页面未修改，可以共享

### 4.3 映射更新

**页表更新**

> **设计参考，待实现**：以下 `map_writept` 伪代码展示了页表更新的设计意图。实际 Rust 实现中，页表操作由 `minix_arch::paging` 模块提供，`map_writept` 的具体签名和实现待页表模块完成后确定。当前 CoW 流程中，页表更新在 `handle_pagefault` 返回后由调用者执行。

```rust
/// 更新页表映射
pub fn map_writept(
    vmp: &mut VmProc,
    region: &VirRegion,
    frames: &PageFrames,
    offset: VirBytes,
) -> Result<(), i32> {
    let page_idx = (offset.get() / PAGE_SIZE) as usize;
    let slot = region.physblocks[page_idx]
        .ok_or(EFAULT)?;

    // 确定页表标志
    let mut flags = PteFlags::PRESENT | PteFlags::USER;

    // 通过 MemType::writable 判断可写性（包含 refcount 和 remaps 检查）
    let memtype = slot.memtype.unwrap();
    if memtype.writable(frames, slot, region) && region.flags.contains(VrFlags::WRITABLE) {
        flags |= PteFlags::WRITABLE;
    }

    // 计算虚拟地址
    let vaddr = VirBytes(region.vaddr.0 + offset.0);

    // 写入页表
    let phys = frames.pfn_to_phys(slot.pfn);
    vmp.page_table.map_page(
        vaddr,
        phys,
        flags,
        MapFlags::OVERWRITE,
    )?;

    // 刷新 TLB
    vmp.page_table.flush_tlb(vaddr);

    Ok(())
}
```

**TLB 刷新策略**

1. **单页刷新 (INVLPG)**: 只刷新单个页的 TLB 条目，优点是开销小，适用于单个 CoW 操作
2. **全局刷新 (CR3 重载)**: 重新加载页表基址寄存器，优点是简单，缺点是刷新所有 TLB 开销大，适用于大量映射变更
3. **进程间 TLB**: CoW 只影响当前进程的 TLB，其他共享进程的 TLB 不受影响（它们仍然指向原物理页，页表项仍然是只读，写入时也会触发 CoW）

### 4.4 引用计数维护

**引用计数变化**

| 阶段 | old_pfn (PageState) | new_pfn (PageState) |
|------|---------------------|---------------------|
| CoW 前 | refcount: 2 | 不存在 |
| `unmap_page(frames, offset)` 后 | refcount: 1 | 不存在 |
| `alloc_phys_page()` 后 | 同上 | refcount: 0 |
| `map_page(frames, offset, new_pfn, &MEM_TYPE_ANON)` 后 | 同上 | refcount: 1 |

CoW 后：old_pfn 变为父进程私有（refcount=1），new_pfn 为子进程私有（refcount=1）。

**引用计数一致性保证**

```rust
/// CoW 引用计数一致性验证
///
/// # Arguments
///
/// - `frames`: 全局物理页状态表
/// - `old_pfn`: CoW 前的物理页帧号
/// - `new_pfn`: CoW 后新分配的物理页帧号
/// - `parent_slot`: 父进程中该页的 PageSlot（应指向 old_pfn）
/// - `child_slot`: 子进程中该页的 PageSlot（应指向 new_pfn）
#[cfg(debug_assertions)]
fn verify_cow_consistency(
    frames: &PageFrames,
    old_pfn: u32,
    new_pfn: u32,
    parent_slot: &PageSlot,
    child_slot: &PageSlot,
) {
    // 原页应该只有父进程引用
    let old_state = frames.get(old_pfn).unwrap();
    assert_eq!(old_state.refcount, 1);

    // 新页应该只有子进程引用
    let new_state = frames.get(new_pfn).unwrap();
    assert_eq!(new_state.refcount, 1);

    // 父进程 PageSlot 应指向原页
    assert_eq!(parent_slot.pfn, old_pfn);

    // 子进程 PageSlot 应指向新页
    assert_eq!(child_slot.pfn, new_pfn);
}
```

---

## 5. fork 与 CoW

### 5.1 fork 时的 CoW 准备

**fork 流程中的 CoW 设置**

```c
/* fork.c: do_fork() (fork.c:32) — CoW 相关部分 */
int do_fork(message *msg)
{
  int r, proc, childproc;
  struct vmproc *vmp, *vmc;
  pt_t origpt;

  /* ... 参数验证 ... */

  vmp = &vmproc[proc];        /* parent */
  vmc = &vmproc[childproc];   /* child */

  /* 子进程基本初始化 */
  origpt = vmc->vm_pt;
  *vmc = *vmp;
  vmc->vm_slot = childproc;
  region_init(&vmc->vm_regions_avl);
  vmc->vm_endpoint = NONE;
  vmc->vm_pt = origpt;

#if VMSTATS
  vmc->vm_bytecopies = 0;
#endif

  if(pt_new(&vmc->vm_pt) != OK) {
    return ENOMEM;
  }

  /* 复制父进程的地址空间（含 CoW 设置） */
  if(map_proc_copy(vmc, vmp) != OK) {
    printf("VM: fork: map_proc_copy failed\n");
    pt_free(&vmc->vm_pt);
    return(ENOMEM);
  }

  /* ... ACL、sys_fork、pt_bind 等 ... */

  return OK;
}

/* region.c: map_proc_copy() (region.c:933) */
int map_proc_copy(struct vmproc *dst, struct vmproc *src)
{
    region_init(&dst->vm_regions_avl);
    return map_proc_copy_range(dst, src, NULL, NULL);
}

/* region.c: map_proc_copy_range() (region.c:944) */
int map_proc_copy_range(struct vmproc *dst, struct vmproc *src,
    struct vir_region *start_src_vr, struct vir_region *end_src_vr)
{
    struct vir_region *vr;
    region_iter v_iter;

    /* ... 初始化迭代器 ... */

    while((vr = region_get_iter(&v_iter))) {
        struct vir_region *newvr;
        if(!(newvr = map_copy_region(dst, vr))) {
            map_free_proc(dst);
            return ENOMEM;
        }
        region_insert(&dst->vm_regions_avl, newvr);

        if(vr == end_src_vr) break;
        region_incr_iter(&v_iter);
    }

    /* 重写父子进程的页表，共享页自动变为只读 */
    map_writept(src);
    map_writept(dst);

    return OK;
}

/* region.c: map_copy_region() (region.c:802) */
struct vir_region *map_copy_region(struct vmproc *vmp, struct vir_region *vr)
{
    struct vir_region *newvr;
    vir_bytes p;

    if(!(newvr = region_new(vr->parent, vr->vaddr, vr->length,
            vr->flags, vr->def_memtype)))
        return NULL;

    USE(newvr, newvr->parent = vmp;);

    if(vr->def_memtype->ev_copy &&
       (r=vr->def_memtype->ev_copy(vr, newvr)) != OK) {
        map_free(newvr);
        return NULL;
    }

    for(p = 0; p < phys_slot(vr->length); p++) {
        struct phys_region *newph;
        struct phys_region *ph;

        if(!(ph = physblock_get(vr, p*VM_PAGE_SIZE))) continue;
        newph = pb_reference(ph->ph, ph->offset, newvr,
            vr->def_memtype);

        if(!newph) { map_free(newvr); return NULL; }

        if(ph->memtype->ev_reference)
            ph->memtype->ev_reference(ph, newph);
    }

    return newvr;
}
```

**CoW 准备流程**

1. **创建子进程结构**: 分配 vmproc 结构体，初始化基本字段
2. **复制虚拟区域**: `map_proc_copy_range()` 对每个 vir_region 调用 `map_copy_region()`，对每个 phys_region 调用 `pb_reference()` → refcount++，然后调用 `ph->memtype->ev_reference()` 通知内存类型（如 `cache_reference` 返回 OK、`anon_contig_reference` 返回 ENOMEM 阻止 fork）。Rust 实现中 `ev_reference` 返回 `Result<(), MemTypeError>`，`ContiguousAnonymous::ev_reference` 返回 `Err(NotSupported)` 阻止 fork
3. **设置页表只读**: `map_writept(parent)` 和 `map_writept(child)` 重写页表，共享页（refcount > 1）通过 `pr_writable()` → `anon_writable()` 返回不可写，PTE 不含 `PTF_WRITE`
4. **返回用户态**: 父进程返回子进程 PID，子进程返回 0。两个进程共享所有物理页，写入时触发 CoW

**共享状态示意**

fork 后父子进程共享状态：

- 父进程 VirRegion A (vaddr: 0x400000): physblocks[0] → PageSlot(pfn=10), physblocks[1] → PageSlot(pfn=11), physblocks[2] → PageSlot(pfn=12)
- 子进程 VirRegion A (vaddr: 0x400000): physblocks[0] → PageSlot(pfn=10), physblocks[1] → PageSlot(pfn=11), physblocks[2] → PageSlot(pfn=12)
- PageFrames[10]: refcount=2
- PageFrames[11]: refcount=2
- PageFrames[12]: refcount=2
- 所有页表项都是只读（不含 PTF_WRITE）

### 5.2 首次写入触发

**谁先写谁触发 CoW**

- **场景 1: 父进程先写入**: 父进程写入共享页 → 页面保护异常 → VM 检测 refcount=2 → 执行 CoW → 父进程获得私有页（refcount=1），子进程仍共享原页（refcount=1）。如果子进程也写入，也会触发 CoW
- **场景 2: 子进程先写入**: 子进程写入共享页 → 页面保护异常 → VM 检测 refcount=2 → 执行 CoW → 子进程获得私有页（refcount=1），父进程仍共享原页（refcount=1）。如果父进程也写入，也会触发 CoW
- **场景 3: 都不写入**: 如果子进程立即 exec()，不需要复制任何页面，fork 效率最高。这是 CoW 的主要优化场景

**多次 fork 的 CoW**

| 阶段 | PageState refcount | 说明 |
|------|-------------------|------|
| 初始 | 1 | 父进程私有 |
| 第一次 fork | 2 | 父 + 子1 共享 |
| 第二次 fork | 3 | 父 + 子1 + 子2 共享 |
| 父进程写入 CoW 后 | 原=2(子1+子2), 新=1(父私有) | 父进程获得私有页 |
| 子1 写入 CoW 后 | 原=1(子2), 新=1(子1私有) | 子1 获得私有页 |
| 最终 | 各=1 | 每个进程都有私有页 |

---

## 6. 测试与验证

### 6.1 CoW 触发测试

**延迟复制验证**

```rust
#[cfg(test)]
mod cow_trigger_tests {
    use super::*;

    #[test]
    fn test_cow_deferred() {
        let mut frames = PageFrames::new(PhysBytes(128 * 1024 * 1024));
        let mut parent = VmProc::new(1);
        let vaddr = VirBytes(0x400000);

        // 分配一个页面
        let region = parent.alloc_region(vaddr, VirBytes(0x1000), VrFlags::WRITABLE).unwrap();

        // 写入数据
        let data: [u8; 4096] = [0xAA; 4096];
        parent.write_memory(vaddr, &data).unwrap();

        // 记录原始 PFN
        let slot = region.physblocks[0].unwrap();
        let original_pfn = slot.pfn;

        // fork
        let child = parent.fork().unwrap();

        // 验证共享：PFN 相同
        let child_region = child.find_region(vaddr).unwrap();
        let child_slot = child_region.physblocks[0].unwrap();
        assert_eq!(original_pfn, child_slot.pfn, "fork 后应该共享物理页");

        // 验证引用计数
        assert_eq!(frames.get(original_pfn).unwrap().refcount, 2);

        // 验证页表只读
        assert!(!parent.is_writable(vaddr));
        assert!(!child.is_writable(vaddr));
    }

    #[test]
    fn test_cow_on_write() {
        let mut frames = PageFrames::new(PhysBytes(128 * 1024 * 1024));
        let mut parent = VmProc::new(1);
        let vaddr = VirBytes(0x400000);

        // 分配并写入
        parent.alloc_region(vaddr, VirBytes(0x1000), VrFlags::WRITABLE).unwrap();
        let data: [u8; 4096] = [0xAA; 4096];
        parent.write_memory(vaddr, &data).unwrap();

        // fork
        let mut child = parent.fork().unwrap();

        // 子进程写入触发 CoW
        let new_data: [u8; 4096] = [0xBB; 4096];
        child.write_memory(vaddr, &new_data).unwrap();

        // 验证 PFN 不同
        let parent_region = parent.find_region(vaddr).unwrap();
        let child_region = child.find_region(vaddr).unwrap();

        let parent_pfn = parent_region.physblocks[0].unwrap().pfn;
        let child_pfn = child_region.physblocks[0].unwrap().pfn;

        assert_ne!(parent_pfn, child_pfn, "CoW 后物理页应该不同");

        // 验证数据隔离
        let mut parent_buf = [0u8; 4096];
        let mut child_buf = [0u8; 4096];
        parent.read_memory(vaddr, &mut parent_buf).unwrap();
        child.read_memory(vaddr, &mut child_buf).unwrap();

        assert_eq!(parent_buf, [0xAA; 4096]);
        assert_eq!(child_buf, [0xBB; 4096]);
    }

    #[test]
    fn test_cow_refcount_decrement() {
        let mut frames = PageFrames::new(PhysBytes(128 * 1024 * 1024));
        let mut parent = VmProc::new(1);
        let vaddr = VirBytes(0x400000);

        parent.alloc_region(vaddr, VirBytes(0x1000), VrFlags::WRITABLE).unwrap();
        parent.write_memory(vaddr, &[0xAA; 4096]).unwrap();

        let child = parent.fork().unwrap();

        // 验证 refcount = 2
        let region = parent.find_region(vaddr).unwrap();
        let pfn = region.physblocks[0].unwrap().pfn;
        assert_eq!(frames.get(pfn).unwrap().refcount, 2);

        // 子进程释放
        drop(child);

        // 验证 refcount = 1
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);
    }
}
```

### 6.2 内存节省测试

**共享页统计**

```rust
#[cfg(test)]
mod memory_saving_tests {
    use super::*;

    #[test]
    fn test_shared_pages_count() {
        let mut frames = PageFrames::new(PhysBytes(128 * 1024 * 1024));
        let mut parent = VmProc::new(1);
        let vaddr = VirBytes(0x400000);

        for i in 0..10 {
            let addr = VirBytes(vaddr.0 + i * 0x1000);
            parent.alloc_region(addr, VirBytes(0x1000), VrFlags::WRITABLE).unwrap();
            parent.write_memory(addr, &[i as u8; 4096]).unwrap();
        }

        let phys_before = frames.allocated_count();

        let child = parent.fork().unwrap();

        let phys_after = frames.allocated_count();
        assert_eq!(phys_after, phys_before, "fork 后物理内存应该不变");

        let shared_count = count_shared_pages(&frames, &parent);
        assert_eq!(shared_count, 10, "所有 10 页应该共享");
    }

    #[test]
    fn test_cow_gradual_allocation() {
        let mut frames = PageFrames::new(PhysBytes(128 * 1024 * 1024));
        let mut parent = VmProc::new(1);
        let vaddr = VirBytes(0x400000);

        for i in 0..10 {
            let addr = VirBytes(vaddr.0 + i * 0x1000);
            parent.alloc_region(addr, VirBytes(0x1000), VrFlags::WRITABLE).unwrap();
        }

        let mut child = parent.fork().unwrap();

        let phys_after_fork = frames.allocated_count();

        for i in 0..3 {
            let addr = VirBytes(vaddr.0 + i * 0x1000);
            child.write_memory(addr, &[i as u8; 4096]).unwrap();
        }

        let phys_after_cow = frames.allocated_count();

        assert_eq!(phys_after_cow, phys_after_fork + 3,
                   "应该只分配了被修改的 3 页");
    }

    fn count_shared_pages(frames: &PageFrames, vmp: &VmProc) -> usize {
        let mut count = 0;
        for region in vmp.regions() {
            for slot_opt in &region.physblocks {
                if let Some(slot) = slot_opt {
                    if frames.get(slot.pfn).unwrap().refcount > 1 {
                        count += 1;
                    }
                }
            }
        }
        count
    }
}
```

### 6.3 正确性测试

**父子进程数据隔离**

```rust
#[cfg(test)]
mod correctness_tests {
    use super::*;
    
    #[test]
    fn test_parent_child_isolation() {
        let mut parent = VmProc::new(1);
        let vaddr = VirBytes(0x400000);
        
        // 分配并初始化
        parent.alloc_region(vaddr, VirBytes(0x1000), VrFlags::WRITABLE).unwrap();
        parent.write_memory(vaddr, &[0xAA; 4096]).unwrap();
        
        // fork
        let mut child = parent.fork().unwrap();
        
        // 父进程修改
        parent.write_memory(vaddr, &[0xBB; 4096]).unwrap();
        
        // 验证子进程不受影响
        let mut buf = [0u8; 4096];
        child.read_memory(vaddr, &mut buf).unwrap();
        assert_eq!(buf, [0xAA; 4096], "子进程应该看到原始数据");
        
        // 子进程修改
        child.write_memory(vaddr, &[0xCC; 4096]).unwrap();
        
        // 验证父进程不受影响
        parent.read_memory(vaddr, &mut buf).unwrap();
        assert_eq!(buf, [0xBB; 4096], "父进程应该看到自己的修改");
    }
    
    #[test]
    fn test_multiple_fork_isolation() {
        let mut parent = VmProc::new(1);
        let vaddr = VirBytes(0x400000);
        
        parent.alloc_region(vaddr, VirBytes(0x1000), VrFlags::WRITABLE).unwrap();
        parent.write_memory(vaddr, &[0x00; 4096]).unwrap();
        
        // 多次 fork
        let mut child1 = parent.fork().unwrap();
        let mut child2 = parent.fork().unwrap();
        
        // 各自修改
        parent.write_memory(vaddr, &[0x11; 4096]).unwrap();
        child1.write_memory(vaddr, &[0x22; 4096]).unwrap();
        child2.write_memory(vaddr, &[0x33; 4096]).unwrap();
        
        // 验证隔离
        let mut buf = [0u8; 4096];
        
        parent.read_memory(vaddr, &mut buf).unwrap();
        assert_eq!(buf, [0x11; 4096]);
        
        child1.read_memory(vaddr, &mut buf).unwrap();
        assert_eq!(buf, [0x22; 4096]);
        
        child2.read_memory(vaddr, &mut buf).unwrap();
        assert_eq!(buf, [0x33; 4096]);
    }
    
    #[test]
    fn test_partial_page_cow() {
        let mut parent = VmProc::new(1);
        let vaddr = VirBytes(0x400000);
        
        parent.alloc_region(vaddr, VirBytes(0x1000), VrFlags::WRITABLE).unwrap();
        
        // 初始化整个页面
        let mut data = [0u8; 4096];
        for i in 0..4096 {
            data[i] = (i % 256) as u8;
        }
        parent.write_memory(vaddr, &data).unwrap();
        
        // fork
        let mut child = parent.fork().unwrap();
        
        // 子进程只修改一个字节
        child.write_memory(VirBytes(vaddr.0 + 100), &[0xFF]).unwrap();
        
        // 验证 CoW 复制了整个页面
        let mut buf = [0u8; 4096];
        child.read_memory(vaddr, &mut buf).unwrap();
        
        // 验证数据正确
        for i in 0..4096 {
            if i == 100 {
                assert_eq!(buf[i], 0xFF);
            } else {
                assert_eq!(buf[i], (i % 256) as u8);
            }
        }
    }
}
```

### 6.4 性能统计 (vm_bytecopies)

**vm_bytecopies 字段**

`vm_bytecopies` 是 `vmproc` 结构中的一个统计字段（仅在 `VMSTATS` 启用时存在，默认 `VMSTATS=0`），定义在 `vmproc.h:26`。

**源码中的实际使用**:

```c
/* vmproc.h:25-26 */
#if VMSTATS
    int vm_bytecopies;
#endif

/* fork.c:66-68 — 子进程初始化时清零 */
#if VMSTATS
  vmc->vm_bytecopies = 0;
#endif

/* exit.c:38-39, 50-51 — 进程退出时清零 */
#if VMSTATS
    vmp->vm_bytecopies = 0;
#endif
```

> **注意**: 在当前 Minix3 源码中，`vm_bytecopies` 仅在 fork 和 exit 时被清零，**未在 `mem_cow()` 中递增**。`mem_cow()` 使用 `sys_abscopy()` 而非 `memcpy()`。文档之前的版本声称 `mem_cow()` 中有 `vm_bytecopies += PAGE_SIZE` 和 `memcpy()` 调用，这是不准确的。该字段可能曾在早期版本中使用，但在当前代码中仅保留清零操作。

> **方案四标注**：方案四下 `vm_bytecopies` 的统计逻辑不变（仍在 fork/exit 时清零），但 CoW 复制从 `sys_abscopy` 变为 `copy_nonoverlapping`。如果需要统计 CoW 复制量，可以在 `copy_phys_page()` 中递增 `vm_bytecopies += VM_PAGE_SIZE`——因为复制代码现在在 VM 进程内执行，直接访问 `vmp` 结构即可，不需要内核参与统计。

**fork 时的处理**:
- 子进程的 `vm_bytecopies` 初始化为 0（fork.c:67）
- 进程退出时清零（exit.c:39, 51）
- 父进程的统计不受影响

---

## 7. 参见

- [10-phys-pagestate.md](10-phys-pagestate.md) - 全局物理页状态（PageState.refcount，替代原 phys_block）
- [12-memtype.md](12-memtype.md) - mem_type_anon / mem_type_mappedfile / mem_type_shared 的定义与 CoW 行为
- [11-region-mapping.md](11-region-mapping.md) - VirRegion + PageSlot 页映射（替代原 vir_region + phys_region）
- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构（x86-32 vs x86-64 差异）
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作（pt_writemap, PTF_* 标志）
- [15-pagefault.md](15-pagefault.md) - 页错误处理流程（map_pf → ev_pagefault → map_ph_writept）
- [16-vm-fork.md](16-vm-fork.md) - fork 时的 CoW 设置（map_proc_copy_range → map_writept）

---

*分类: VM私有*
