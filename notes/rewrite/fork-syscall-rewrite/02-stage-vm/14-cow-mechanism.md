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
| `phys_block.refcount` | 引用计数 | refcount > 1 表示页面共享，写操作时触发 CoW |
| `mem_cow()` | 执行复制 | 分配新页、复制数据、切换 phys_block 引用 |
| `anon_pagefault()` | 页错误处理 | 检测 CoW 条件（write && refcount > 1）并调用 mem_cow() |
| `map_writept()` / `map_ph_writept()` | 页表操作 | fork 后重写页表，共享页通过 pr_writable() 判为只读 |
| `pr_writable()` | 可写判断 | 结合 VR_WRITABLE 和 memtype->writable |
| `VR_WRITABLE` | 区域标志 | 标记区域是否可写 |

**CoW 触发流程**

1. **fork 系统调用**: `map_proc_copy_range()` 复制父进程区域，`pb_reference()` 增加引用计数（refcount 1→2）；所有区域复制完成后，`map_writept()` 重写父子进程页表
2. **设置页表只读**: `map_ph_writept()` 通过 `pr_writable()` → `anon_writable()` 检测共享页（refcount>1）返回不可写，页表项不含 `PTF_WRITE`
3. **进程尝试写入**: CPU 触发页面保护异常（#PF），内核将异常传递给 VM
4. **VM 页错误处理**: `anon_pagefault()` 检测 `refcount >= 2 && write`，调用 `mem_cow()`
5. **执行 CoW**: `mem_cow()` 分配新物理页 → `sys_abscopy()` 复制数据 → `pb_unreferenced()` 取消旧引用 → `pb_link()` 链接新块 → `ph->memtype = &mem_type_anon`
6. **更新页表**: `map_pf()` 返回后调用 `map_ph_writept()` 更新页表项，页面变为可写（refcount=1）
7. **继续执行**: 进程拥有私有物理页，写入成功完成

**首次写入与多次 fork**

CoW 的"谁先写谁触发"特性使得 fork 后的物理页复制完全按需进行：

- **父进程先写入**: 父进程获得私有页（refcount=1），子进程仍共享原页（refcount=1）
- **子进程先写入**: 子进程获得私有页（refcount=1），父进程仍共享原页（refcount=1）
- **都不写入**: 如果子进程立即 exec()，不需要复制任何页面，这是 CoW 的主要优化场景

多次 fork 时 refcount 递增，每次写入触发 CoW 后递减：

| 阶段 | PageState refcount | 说明 |
|------|-------------------|------|
| 初始 | 1 | 父进程私有 |
| 第一次 fork | 2 | 父 + 子1 共享 |
| 第二次 fork | 3 | 父 + 子1 + 子2 共享 |
| 父进程写入 CoW 后 | 原=2(子1+子2), 新=1(父私有) | 父进程获得私有页 |
| 子1 写入 CoW 后 | 原=1(子2), 新=1(子1私有) | 子1 获得私有页 |
| 最终 | 各=1 | 每个进程都有私有页 |

---

## 2. C 源码分析

### 2.1 CoW 触发条件

**触发前提：页表只读**

CoW 的前提是共享页的页表被设为只读。fork 时 `map_writept()` → `map_ph_writept()` → `pr_writable()` 遍历所有 phys_region 写页表，`anon_writable()` 对共享页（refcount > 1）返回 0，页表项不含 `PTF_WRITE`，页面变为只读。可写判断的完整逻辑见 §2.4.1 `anon_writable`。

写入只读页时，CPU 触发页面保护异常（#PF），错误码 bit 1 (W/R) = 1 表示写操作触发，bit 0 (P) = 1 表示页面存在。CoW 场景典型错误码: `0x07` (P=1, W/R=1, U/S=1) — 用户态写已存在但只读的页面。

**两个必要条件**

页错误到达 VM 后，CoW 需要同时满足两个条件才会触发：

1. **写操作** (`write = 1`): 页错误必须由写操作触发。读操作不会触发 CoW，即使 refcount > 1，多个进程可以安全地读取共享页
2. **引用计数 > 1** (`refcount > 1`): phys_block 被多个 phys_region 引用。refcount = 1 时为私有页无需 CoW，refcount > 1 时为共享页需要 CoW

触发条件 = `write && (refcount > 1)`

**Minix3 源码分析**

CoW 条件判断发生在页错误处理链中。调用链为：`handle_pagefault()` → `map_pf()` → `ph->memtype->ev_pagefault()` → `anon_pagefault()`。其中 `map_pf()` 通过 `!write || !ph->memtype->writable(ph)` 判断是否需要调用 `ev_pagefault`（详见 §2.3.2），CoW 的具体条件判断在 `anon_pagefault()` 中完成（完整源码分析见 §2.4.1）：

```c
/* mem_anon.c: anon_pagefault() — 简化版，完整源码见 §2.4.1 */
static int anon_pagefault(...) {
    /* 预分配新页（可能用于 CoW 或首次分配） */
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM)
        return ENOMEM;

    /* 情况 1: 延迟分配，首次分配物理页 */
    if(ph->ph->phys == MAP_NONE) { /* ... */ return OK; }

    /* 情况 2: 不需要 CoW */
    if(ph->ph->refcount < 2 || !write) {
        return OK;    /* ← 注意: 内存泄漏：预分配页未释放 */
    }

    /* 情况 3: 触发 CoW */
    assert(region->flags & VR_WRITABLE);
    return mem_cow(region, ph, new_page_cl, new_page);
}
```

> **内存泄漏**：`anon_pagefault()` 在函数入口预分配 `new_page_cl = alloc_mem(1, ...)`，但当 `refcount < 2 || !write` 时直接 `return OK`，未调用 `free_mem(new_page_cl, 1)` 释放。`alloc_mem()` 从物理内存空闲链表分配（alloc.c:242），必须用 `free_mem()` 归还。整个 `mem_anon.c` 中 `free_mem` 仅在 `anon_unreference()` 中出现，此分支无任何释放路径。Rust 实现中避免了此问题：先判断是否需要 CoW，需要时才分配（见 §3 `ev_pagefault` 返回 `PagefaultResult` 的设计）。

**条件判断详解**

1. `ph->ph->phys == MAP_NONE` → 延迟分配，设置 phys 并返回
2. `ph->ph->refcount < 2 || !write` → 无需 CoW（私有页或读操作），返回 OK
3. 否则 → 触发 CoW，调用 `mem_cow()`

**refcount 状态与 CoW**

| refcount | 状态 | CoW 行为 |
|----------|------|---------|
| 0 | 新创建的 phys_block，尚未被引用（`pb_new()` 初始化为 0） | 不应触发页错误（无映射） |
| 1 | 私有页，只有一个引用者 | 写操作直接执行，无需 CoW |
| ≥2 | 共享页，多个引用者（fork 后） | 写操作触发 CoW |

---

### 2.2 CoW 核心函数

#### 2.2.1 mem_cow - 执行写时复制

`mem_cow()` 是 CoW 的核心执行函数，负责分配新物理页、复制数据、切换 phys_block 引用。调用者为 `anon_pagefault()`（匿名内存 CoW）和 `cow_block()`（文件映射 CoW），它们会预分配新页传入；如果未预分配（`new_page == MAP_NONE`），`mem_cow()` 自行分配。

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
3. **`sys_abscopy` 系统调用**：内核提供的物理内存复制接口，直接操作物理地址，无需映射到虚拟地址空间。Rust 设计中 VM 拥有 direct map，此系统调用被 `copy_nonoverlapping` 替代（见 §3）
4. **错误处理**: ENOMEM 无法分配新页；EFAULT 复制失败（不应该发生）。失败时需要释放已分配的资源

---

#### 2.2.2 pb_reference - 共享时增加引用

fork 时子进程需要共享父进程的物理页，`pb_reference()` 完成这个操作：分配新的 `phys_region`，通过 `pb_link()` 链入 `phys_block` 的引用链表（refcount++），并设置 `vir_region.physblocks[]` 数组槽位。完整源码分析见 [10-phys-pagestate.md](10-phys-pagestate.md) §2.5 和 [11-region-mapping.md](11-region-mapping.md) §2.3。

**与 CoW 的关系**

`pb_reference` 建立共享，为 CoW 创造前提条件：

1. fork 时 `map_copy_region()` 对每个 phys_region 调用 `pb_reference(ph->ph, offset, child_vr, memtype)` → refcount 从 1 变为 2
2. 所有区域复制完成后，`map_writept()` 重写页表，共享页（refcount > 1）通过 `pr_writable()` → `anon_writable()` 判为只读
3. 写入时触发 CoW → 检测 `refcount > 1` → 执行 `mem_cow`
4. CoW 后 → 原块 refcount 减为 1 → 新块 refcount = 1 → 各自独立


#### 2.2.3 pb_unreferenced - 解除共享

`pb_unreferenced()` 解除 phys_region 对 phys_block 的引用：refcount--、从侵入式链表移除节点、refcount 归零时调用 `ev_unreference` 释放物理页。完整源码分析见 [10-phys-pagestate.md](10-phys-pagestate.md) §2.5。

**与 CoW 的关系：rm 参数**

`pb_unreferenced` 的 `rm` 参数在 CoW 中有关键作用：

| rm 值 | 场景 | 效果 |
|-------|------|------|
| 0 | CoW（`mem_cow()` 中调用） | refcount--、从链表移除、`ph->ph = NULL`，但 `physblocks[offset]` 保留（phys_region 还要继续使用，后续 `pb_link` 到新的 phys_block） |
| 1 | 释放（munmap/exit 中调用） | refcount--、从链表移除、`ph->ph = NULL`，且 `physblocks[offset] = NULL`（phys_region 不再使用） |

**CoW 中的调用序列**

`mem_cow()` 中 `pb_unreferenced` 和 `pb_link` 必须成对出现：

```
pb_unreferenced(region, ph, 0);   // 旧块 refcount--, ph->ph = NULL
pb_link(ph, new_pb, offset, region); // 新块 refcount = 1, ph->ph = new_pb
```

rm=0 是关键：phys_region 还要继续使用，只是换一个 phys_block。如果 rm=1，`physblock_set` 会清除数组槽位，后续 `pb_link` 无法恢复。

**CoW 完整流程中的 refcount 变化**

| 阶段 | phys_block 状态 | refcount |
|------|----------------|----------|
| 初始（fork 后） | 共享块 | 2 |
| `pb_unreferenced(region, ph, 0)` 后 | 原块 | 1 |
| `pb_link(ph, new_pb, ...)` 后 | 新块 | 1 |

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

    /* 遍历进程的所有虚拟区域 */
    while((vr = region_get_iter(&v_iter))) {
        vir_bytes p;
        /* 对区域内的每个页面 */
        for(p = 0; p < vr->length; p += VM_PAGE_SIZE) {
            if(!(ph = physblock_get(vr, p))) continue;

            /* 写入页表项：共享页设只读，私有页设可写 */
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
| `PTF_READ` | 只读 | 共享页时设置此位（x86-32 上 PTF_READ = I386_VM_READ，是软件抽象，x86 硬件无独立只读位） |

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
/* region.c: map_pf() — 页错误处理调用链（简化） */
int map_pf(struct vmproc *vmp,
    struct vir_region *region,
    vir_bytes offset,
    int write,
    vfs_callback_t pf_callback,
    void *state,
    int len,
    int *io)
{
    struct phys_region *ph;
    int r = OK;

    /* ... 查找或创建 phys_region ... */

    if(!write || !ph->memtype->writable(ph)) {
        assert(ph->memtype->ev_pagefault);

        /* 调用内存类型的页错误处理（使用 ph->memtype，而非 region->def_memtype） */
        if((r = ph->memtype->ev_pagefault(vmp,
                region, ph, write, pf_callback, state, len, io)) != OK) {
            if(ph)
                pb_unreferenced(region, ph, 1);
            return r;
        }

        assert(ph->ph->phys != MAP_NONE);
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
    
    /* remaps 记录有多少个共享内存区域引用了此 vir_region。
     * 共享内存的语义是"所有修改可见"，不应该走 CoW。
     * 如果这里返回 0，map_ph_writept() 会把页设为只读，
     * 共享内存写入就会反复触发页错误，所以必须返回可写。 */
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
/* mem_file.c: 文件映射内存类型 (mem_file.c:33) */
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
    void *state, int len, int *io)
{
    /* ... 缓存查找和 VFS 请求 ... */

    /*
     * 注意: 简化版：完整源码有 3 处 cow_block 调用点：
     *   1. cache-hit + clearend (mem_file.c:127)
     *   2. cache-hit + write    (mem_file.c:130)
     *   3. 物理页已存在 + write (mem_file.c:164)
     * 此处仅展示第 3 条路径。
     */

    if(!write) {
        return OK;
    }

    /* 写操作触发 CoW */
    return cow_block(vmp, region, ph, 0);
}
```

> **注意**: `mem_type_cache`（mem_cache.c）是 VM 内部的文件缓存机制，其 `cache_pagefault` 逻辑是链接预分配的缓存页，与 CoW 无关。

> **Rust 实现现状**：`MappedFile::ev_pagefault` 已设计为返回 `NeedVfsIo`（读操作缓存未命中）或 `NeedCow`（写操作已映射页），对应 C 中的 `SUSPEND` 和 `cow_block()` 路径。CoW 后的 memtype 切换（`ph->memtype = &mem_type_anon`）已通过 `map_page(new_pfn, &MEM_TYPE_ANON)` 实现（见 23-vfs-interaction.md §3.7）。完整设计见 [23-vfs-interaction.md](23-vfs-interaction.md) §4.7。

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

> **Rust 实现现状**：当前 Rust 代码中 `SharedMemory::ev_pagefault` 已 override 但为 stub 实现（始终返回 `PagefaultResult::Handled`），**未实现** C 中 `shared_pagefault` 的源进程链接逻辑。共享内存页错误处理是待实现功能（TODO）。

**内存类型对比**

| 内存类型 | CoW 支持 | CoW 后类型 | writable 逻辑 | 说明 |
|---------|---------|-----------|--------------|------|
| 匿名内存 (`mem_type_anon`) | 是 | 匿名内存 | `refcount == 1` 时可写 | 标准行为 |
| 文件映射 (`mem_type_mappedfile`) | 是 | 匿名内存 | 始终返回 0（不可写） | 写入必触发 CoW |
| 共享内存 (`mem_type_shared`) | 否 | 共享内存 | `phys != MAP_NONE` 即可写 | 所有修改可见 |
| 缓存内存 (`mem_type_cache`) | 否 | 缓存内存 | `phys != MAP_NONE` 即可写 | VM 内部缓存 |

### 2.5 fork 时的 CoW 设置

**fork 流程中的 CoW 准备**

fork 系统调用是 CoW 的起点。fork 时并不复制物理页，而是通过 `pb_reference()` 增加引用计数并重写页表为只读，将"复制"延迟到首次写入时。

fork 的 CoW 相关调用链为：`do_fork()` → `map_proc_copy()` → `map_proc_copy_range()` → `map_copy_region()`。其中与 CoW 直接相关的关键操作只有两个：

**1. `pb_reference()` — 增加引用计数（region.c:835, map_copy_region 内）**

```c
/* 遍历父区域每个 phys_region，共享而非复制物理页 */
for(p = 0; p < phys_slot(vr->length); p++) {
    if(!(ph = physblock_get(vr, p*VM_PAGE_SIZE))) continue;
    newph = pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype);  /* refcount++ */
    if(!newph) { map_free(newvr); return NULL; }
    if(ph->memtype->ev_reference) ph->memtype->ev_reference(ph, newph); /* 通知内存类型 */
}
```

`pb_reference()` 分配新的 `phys_region`，通过 `pb_link()` 链入 `phys_block` 的引用链表（refcount++）。`ev_reference()` 通知内存类型：`cache_reference` 返回 OK，`anon_contig_reference` 返回 ENOMEM。**注意: C 源码 bug**：`ev_reference` 的返回值在 `map_copy_region()` 中被忽略（region.c:841-842，`if(ph->memtype->ev_reference) ph->memtype->ev_reference(ph, newph);`），因此 `anon_contig_reference` 返回 ENOMEM **并不能**阻止 fork——子进程仍然会获得共享的连续匿名内存区域。Rust 实现修复了此 bug：`fork_region()` 检查 `ev_reference` 返回值，失败时 rollback 已增加的 refcount（fork.rs:46-52）。

**2. `map_writept()` — 重写页表为只读（region.c:995, map_proc_copy_range 内）**

```c
/* 所有区域复制完成后，重写父子进程的页表 */
map_writept(src);   /* 共享页（refcount>1）通过 pr_writable() 判为只读 */
map_writept(dst);
```

`map_writept()` 遍历所有 phys_region，通过 `pr_writable()` → `anon_writable()` 检测共享页（refcount > 1）返回不可写，PTE 不含 `PTF_WRITE`。fork 后父子进程共享所有物理页，页表均为只读，写入时触发 CoW。

**Rust 实现的 refcount rollback**：C 源码中 `ev_reference` 返回值被忽略（见上方 bug 说明），fork 无法在 `anon_contig_reference` 返回 ENOMEM 时回滚。Rust 的 `fork_region()` 正确处理了此场景：遍历 physblocks 时逐个调用 `ev_reference`，如果某个返回错误，则对已增加 refcount 的页执行 rollback——逐个直接递减 `state.refcount -= 1`（fork.rs:38-43），然后返回 `ForkError`。

**Rust fork_region 的其他关键操作**：除 refcount 递增和 rollback 外，`fork_region()` 还执行以下操作：

1. **继承元数据字段**：子进程区域继承父进程的 `parent_slot`、`def_memtype`、`remaps`、`id`、`param`（fork.rs:18-22），并通过 `ev_copy()` 通知内存类型执行类型特定的复制（如 `MappedFile::ev_copy` 克隆 `dst.param`）
2. **设置不可写**：`dst.set_writable(false)`（fork.rs:54），确保子进程区域初始为只读，写入时触发页错误进入 CoW 路径

**共享状态示意**

fork 后父子进程共享状态：

- 父进程 VirRegion A (vaddr: 0x400000): physblocks[0] → PageSlot(pfn=10), physblocks[1] → PageSlot(pfn=11), physblocks[2] → PageSlot(pfn=12)
- 子进程 VirRegion A (vaddr: 0x400000): physblocks[0] → PageSlot(pfn=10), physblocks[1] → PageSlot(pfn=11), physblocks[2] → PageSlot(pfn=12)
- PageFrames[10]: refcount=2
- PageFrames[11]: refcount=2
- PageFrames[12]: refcount=2
- 所有页表项都是只读（不含 PTF_WRITE）

---

## 3. Rust 设计决策

### 3.1 CoW 安全实现

**安全挑战**

CoW 涉及多个进程共享同一物理页，需要安全地处理并发访问：

**CoW 安全挑战**

1. **并发写入**: 多个进程可能同时尝试写入同一共享页，需要确保只有一个进程执行 CoW，其他进程等待或重试
2. **引用计数一致性**: `PageState.refcount` 必须与实际引用数一致，增减操作必须成对出现，refcount 归零时物理页才能释放
3. **页表同步**: 页表更新需要与 CoW 操作同步，页表更新在 CoW 完成后，TLB 刷新及时

**Rust 解决方案**

Rust 使用全局 `PageFrames`（`Vec<PageState>`）按 PFN 索引管理物理页状态，`VirRegion.physblocks: Vec<Option<PageSlot>>` 管理映射。CoW 操作不再需要侵入式链表（`pb_link`/`pb_unreferenced`），而是通过 `PageSlot` 的 Copy 语义和 `PageFrames` 的 refcount 操作完成。

```rust
/// VirRegion 的 CoW 检查方法
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
}

/// CoW 解析核心函数（独立函数，非 impl VirRegion）
///
/// 实际代码位于 cow_exec_pf.rs，为独立函数而非 VirRegion 的方法。
/// 此处保留为设计参考，展示 CoW 解析的完整逻辑。
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
        // 对于 CoW，旧页 refcount > 0（其他进程仍在引用），所以 pending 通常为 None。
        // pending = Some 仅在旧页 refcount 降为 0 且非缓存页时出现，
        // 此时需要通知内存类型 (ev_unreference) 并释放物理页。

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

**CoW 状态为什么不需要显式化**

`refcount` 本身就是 CoW 状态：`== 1` 即私有页，`> 1` 即共享页——这是数学关系，不是独立标志位。如果额外引入 enum，反而造成双来源：refcount 和 enum 必须同步，一旦不一致无法判断谁对。

**MemType trait 集成**

```rust
/// 内存类型 trait（详见 12-memtype.md）
///
/// 对应 Minix3: `struct mem_type`
///
/// 注意: 此处为简化视图，仅列出与 CoW 直接相关的方法。
/// 完整的 MemType trait 包含 14 个方法（ev_new, ev_delete, ev_split,
/// ev_low_shrink, ev_sanitycheck, ev_copy, pt_flags, regionid 等），
/// 详见 12-memtype.md §3。
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
```

> **ev_unreference 各类型差异**：`AnonymousMemory::ev_unreference` 是 no-op（物理页释放由 `PfnAllocator::free_pfn` 完成）；`MappedFile::ev_unreference` 同样委托 `PfnAllocator`，但文件映射页的磁盘回写由 VFS 负责，不在 VM 层处理；`SharedMemory::ev_unreference` 减少源进程的引用计数；`CacheMemory` 的 unreference 逻辑与缓存引用计数交互。详见 12-memtype.md 各类型实现。

```rust
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

> **设计说明**：`ev_pagefault` 返回 `PagefaultResult` 而非直接执行 CoW，实现了**判断与执行的分离**。C 中 `anon_pagefault` 预分配新页再判断是否需要 CoW，Rust 中先判断再由 `handle_pagefault` 统一分配——避免了 C 中预分配但不需要 CoW 时内存泄漏的问题。`writable` 包含 `region.remaps > 0` 检查，对应 C 中 `anon_writable` 的 `pr->parent->remaps > 0` 分支。此外，`AnonymousMemory::ref_count` 返回 `1 + region.remaps`，对应 C 中 `shared_refcount` 的语义——remaps 表示 fork 后子进程的引用数，加上自身共 `1 + remaps` 个引用者。

---

## 4. 实现详解

### 4.1 CoW 流程

**完整流程**

1. **页错误发生**: CPU 写入只读页 → 页面保护异常 (#PF) → 内核捕获异常 → 传递给 VM
2. **VM 处理**: `map_pf(vmp, vaddr, write)` → 查找 VirRegion（AVL 树查找）→ 计算 offset → 查找 PageSlot（Vec 索引）
3. **检查 CoW 条件**: `if (write && frames.get(slot.pfn).refcount > 1)` → 需要 CoW，否则不需要
4. **执行 CoW**: `cow_resolve(region, frames, alloc, offset)` → 分配新物理页 → 复制数据 (4KB) → unmap_page 减少旧页 refcount → map_page 增加新页 refcount
5. **更新页表**: CoW 完成后由调用者更新页表（`handle_pagefault` 返回后执行），更新 PTE 指向新物理页，设置可写 → 刷新 TLB
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
fn copy_page_content(frames: &PageFrames, src_pfn: u32, dst_pfn: u32) {
    // 通过 direct map 直接复制（VM 拥有物理内存的完整映射）
    // 使用 AlignedPhysBytes 确保地址页对齐
    // SAFETY: pfn_to_phys 返回的地址始终页对齐（4KB），
    // 因为 PFN 本身就是页帧号，左移 PAGE_SIZE 位后对齐。
    let src_phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(src_pfn).0);
    let dst_phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(dst_pfn).0);
    let src_ptr = vm_phys_to_virt(src_phys).0 as *const u8;
    let dst_ptr = vm_phys_to_virt(dst_phys).0 as *mut u8;
    unsafe {
        core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, PAGE_SIZE as usize);
    }
}
```

**优化可能性**

1. **使用 SIMD 指令**: AVX/SSE 指令可以一次复制 32/16 字节。4KB 页面：普通 memcpy ~128 次循环，AVX-256 ~16 次循环
2. **非临时存储 (Non-temporal Store)**: 绕过缓存，避免污染 L1/L2。适用于数据不会被立即使用、避免缓存污染的场景
3. **预取 (Prefetch)**: 预取下一页到缓存，如果连续触发多个 CoW 可以预取
4. **延迟复制**: 如果页面全为零，直接分配零页；如果页面未修改，可以共享

### 4.3 映射更新

**设计原则**

CoW 流程中的页表更新遵循架构无关原则：OS 代码只通过 `Paging` trait 请求"做什么"，不关心架构层"怎么做"。`Paging` trait（定义于 `minix_arch::paging`）提供了 `map()`、`remap()` 和 `flush_tlb_addr()` 等方法，OS 代码调用这些 trait 方法即可完成映射更新，无需了解底层 PTE 格式或 TLB 刷新指令。

**映射更新流程**

CoW 解析完成后，调用者需要更新页表以反映新的物理映射和权限。更新步骤如下：

1. **确定语义权限**：通过 `MemType::writable()` 判断内存类型的可写性（包含 refcount 和 remaps 检查），结合 `VrFlags::WRITABLE` 得到最终的语义权限
2. **构造 PageFlags**：将语义权限转为架构无关的 `PageFlags`（如 `PRESENT | USER_ACCESSIBLE | WRITABLE`），由架构层翻译为具体的硬件 PTE 位
3. **调用 `Paging::map()` / `Paging::remap()`**：写入映射，对应 Minix3 的 `pt_writemap(WMF_OVERWRITE)`
4. **调用 `Paging::flush_tlb_addr()`**：刷新目标虚拟地址的 TLB 条目，确保后续访问使用新的映射

**实际实现**

页表更新有两个场景，对应两个实际函数：

**场景 1：fork 后批量写入页表** — `ActiveProc::write_page_table_mappings()`（vmproc_handle.rs:377）

fork 后子进程需要一次性写入所有页表映射。此函数遍历所有区域和 PageSlot，根据 refcount 判断可写性，构造 PageFlags，调用 `pt.map()`：

```rust
pub(crate) unsafe fn write_page_table_mappings(&mut self, frames: &PageFrames) {
    use minix_arch::paging::PageFlags;
    use minix_types::{PhysBytes, VirBytes};

    const PAGE_SIZE: u64 = <PageTable as Paging>::PAGE_SIZE as u64;
    let mut mappings: alloc::vec::Vec<(VirBytes, PhysBytes, PageFlags)> = alloc::vec::Vec::new();

    for region in self.regions_mut().iter_mut() {
        for (i, slot_opt) in region.physblocks.iter().enumerate() {
            if let Some(slot) = slot_opt {
                if slot.is_mapped() {
                    let vaddr = VirBytes(region.vaddr.0 + i as u64 * PAGE_SIZE);
                    let paddr = frames.pfn_to_phys(slot.pfn);

                    let writable = region.is_writable()
                        && frames.get(slot.pfn)
                            .map(|s| s.refcount == 1)
                            .unwrap_or(false);
                    let flags = if writable {
                        PageFlags::read_write()
                    } else {
                        PageFlags::read_only()
                    };

                    mappings.push((vaddr, paddr, flags));
                }
            }
        }
    }

    let pt = self.page_table_mut();
    for (vaddr, paddr, flags) in mappings {
        let _ = pt.map(vaddr, paddr, flags);
    }
}
```

> **注意**：此函数中可写判断直接使用 `refcount == 1`，而非 `MemType::writable()`。这是因为 fork 后所有区域的 `remaps` 尚未设置，且所有共享页的 refcount > 1，直接检查 refcount 更高效。对于页错误后的单页更新，应使用 `MemType::writable()` 以正确处理 remaps 等特殊情况。

**场景 2：CoW 后单页更新** — 由 `handle_pagefault` 的调用者在 `cow_resolve` 返回后执行

CoW 解析完成后，调用者需要对单个页执行 `remap` + `flush_tlb_addr`。此逻辑尚未在 `handle_pagefault_request` 中实现（dispatcher.rs:133 当前返回 `NotImplemented`），但设计上应如下：

```rust
// CoW 后单页更新（待实现，handle_pagefault_request 中）
let page_table = active.page_table_mut();
let vaddr = VirBytes(region.vaddr.0 + offset.0);
let paddr = frames.pfn_to_phys(new_pfn);
let writable = region.is_writable()
    && region.def_memtype.unwrap().writable(frames, slot, region);
let flags = if writable {
    PageFlags::read_write()
} else {
    PageFlags::read_only()
};
page_table.remap(vaddr, paddr, flags)?;
unsafe { page_table.flush_tlb_addr(vaddr); }
```

> **与 C 的差异**：C 中 `map_pf()` 直接调用 `map_ph_writept()` 更新页表，页表标志通过 `pt_writemap()` 的 `flags` 参数传递，其中包含 x86 特定的 `PTF_WRITE` 等硬件位。Rust 中使用架构无关的 `PageFlags`，由 `Paging` trait 实现翻译为硬件位。

**进程间 TLB 隔离**

CoW 只影响当前进程的 TLB，其他共享进程的 TLB 不受影响——它们仍然指向原物理页，页表项仍然是只读，写入时也会触发各自的 CoW。这是硬件级别的隔离：每个进程拥有独立的地址空间（不同的 CR3/PML4），TLB 条目按地址空间隔离（或通过 PCID/ASID 标记）。

### 4.4 引用计数维护

**引用计数变化**（与 §2.2.1 状态变化表互补，此处聚焦 PageState 视角）

| 阶段 | old_pfn (PageState) | new_pfn (PageState) |
|------|---------------------|---------------------|
| CoW 前 | refcount: 2 | 不存在 |
| `unmap_page(frames, offset)` 后 | refcount: 1 | 不存在 |
| `alloc.alloc_pfn()` 后 | 同上 | refcount: 0 |
| `map_page(frames, offset, new_pfn, &MEM_TYPE_ANON)` 后 | 同上 | refcount: 1 |

CoW 后：old_pfn 变为父进程私有（refcount=1），new_pfn 为子进程私有（refcount=1）。

**引用计数一致性保证**

CoW 解析完成后，`cow_resolve_core` 在 debug 构建中自动调用 `verify_cow_consistency()`（cow_exec_pf.rs）验证一致性：

```rust
/// CoW 一致性验证（仅 debug 构建）
///
/// CoW 解析完成后，断言引用计数和槽位映射的正确性：
/// - old_pfn：引用计数应已递减（原为共享页，现为其他拥有者的私有页）
/// - new_pfn：引用计数应为 1（新分配的页，由当前区域独占）
/// - 区域在 `offset` 处的槽位应指向 new_pfn
#[cfg(debug_assertions)]
fn verify_cow_consistency(
    frames: &PageFrames,
    old_pfn: u32,
    new_pfn: u32,
    region: &VirRegion,
    offset: VirBytes,
) {
    if let Some(old_state) = frames.get(old_pfn) {
        assert!(
            old_state.refcount >= 1,
            "old_pfn {} refcount should be >= 1 after CoW, got {}",
            old_pfn, old_state.refcount
        );
    }

    if let Some(new_state) = frames.get(new_pfn) {
        assert_eq!(
            new_state.refcount, 1,
            "new_pfn {} refcount should be 1 after CoW, got {}",
            new_pfn, new_state.refcount
        );
    }

    if let Some(slot) = region.get_slot(offset) {
        assert!(
            slot.is_mapped(),
            "slot at offset {:?} should be mapped after CoW",
            offset
        );
        assert_eq!(
            slot.pfn, new_pfn,
            "slot at offset {:?} should point to new_pfn {}, got {}",
            offset, new_pfn, slot.pfn
        );
    }
}
```

> 此函数仅在 `#[cfg(debug_assertions)]` 下编译，release 构建中零开销。`cow_resolve_core` 在 CoW 解析完成后自动调用此函数（cow_exec_pf.rs:119），无需手动触发。

---

## 5. 测试与验证

### 5.1 CoW 触发测试

| 测试项 | 验证目标 | 预期结果 | Rust 单元测试 |
|-------|---------|---------|-------------|
| fork 后共享验证 | fork 不复制物理页 | 父子进程同一 vaddr 对应相同 PFN，refcount=2 | Y `test_fork_region_basic` |
| fork 后页表只读 | 共享页不可写 | 父子进程页表项均不含 WRITABLE 标志 | 注意: 隐式（fork_region `set_writable(false)`） |
| 写入触发 CoW | 写共享页触发复制 | 写入后 PFN 不同，原页 refcount=1，新页 refcount=1 | Y `test_cow_copy_page` |
| 子进程释放后 refcount | 释放子进程减少引用 | drop(child) 后原页 refcount 从 2 降为 1 | N 待实现（集成测试级） |

### 5.2 内存节省测试

| 测试项 | 验证目标 | 预期结果 | Rust 单元测试 |
|-------|---------|---------|-------------|
| fork 后物理内存不变 | fork 不分配新物理页 | `frames.allocated_count()` 在 fork 前后相同 | N 待实现（集成测试级） |
| CoW 按需分配 | 只有被修改的页才分配新物理页 | fork 后修改 3 页，物理页增加 3 | N 待实现（集成测试级） |
| 共享页计数 | 所有页面初始均为共享 | fork 后所有页 refcount > 1 | N 待实现（集成测试级） |

### 5.3 正确性测试

| 测试项 | 验证目标 | 预期结果 | Rust 单元测试 |
|-------|---------|---------|-------------|
| 父子进程数据隔离 | CoW 后修改互不可见 | 父写 0xBB、子写 0xCC，各自读回自己的值 | N 待实现（集成测试级） |
| 多次 fork 隔离 | 多个子进程各自独立 | 父 + 子1 + 子2 各自修改后读回各自值 | N 待实现（集成测试级） |
| 部分写入 CoW | 单字节修改触发整页复制 | 修改 1 字节后，整页 4KB 被复制，未修改字节保持一致 | N 待实现（集成测试级） |

### 5.4 性能统计 (vm_bytecopies)

`vm_bytecopies` 是 `vmproc` 结构中的统计字段（仅在 `VMSTATS` 启用时存在，默认 `VMSTATS=0`），定义在 `vmproc.h:26`。

> **注意**: 在当前 Minix3 源码中，`vm_bytecopies` 仅在 fork 和 exit 时被清零，**未在 `mem_cow()` 中递增**。`mem_cow()` 使用 `sys_abscopy()` 而非 `memcpy()`。该字段可能曾在早期版本中使用，但在当前代码中仅保留清零操作。

> **Rust 实现说明**：`vm_bytecopies` 的统计逻辑不变（仍在 fork/exit 时清零），但 CoW 复制从 `sys_abscopy` 变为 `copy_nonoverlapping`。如果需要统计 CoW 复制量，可以在 `copy_phys_page()` 中递增 `vm_bytecopies += VM_PAGE_SIZE`——因为复制代码现在在 VM 进程内执行，直接访问 `vmp` 结构即可，不需要内核参与统计。

---

## 6. 参见

- [10-phys-pagestate.md](10-phys-pagestate.md) - 全局物理页状态（PageState.refcount，替代原 phys_block）
- [12-memtype.md](12-memtype.md) - mem_type_anon / mem_type_mappedfile / mem_type_shared 的定义与 CoW 行为
- [11-region-mapping.md](11-region-mapping.md) - VirRegion + PageSlot 页映射（替代原 vir_region + phys_region）
- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构（x86-32 vs x86-64 差异）
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作（pt_writemap, PTF_* 标志）
- [15-pagefault.md](15-pagefault.md) - 页错误处理流程（map_pf → ev_pagefault → map_ph_writept）
- [16-vm-fork.md](16-vm-fork.md) - fork 时的 CoW 设置（map_proc_copy_range → map_writept）

---

*分类: VM私有*
