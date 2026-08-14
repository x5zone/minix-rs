# 20-vm-exit: 进程退出与资源释放

> **分类**: VM服务
> **源码**: `minix3/minix/servers/vm/exit.c`, `region.c`, `pb.c`, `pagetable.c`
> **说明**: fork 的反面——进程退出时如何释放内存资源，兑现 CoW 引用计数的最终承诺

---

## 1. 概述

### 1.1 本文档的定位

16-vm-fork 创建进程，14-cow-mechanism + 15-pagefault 处理运行时 CoW 和页错误，本文档是生命周期的终点——进程退出时释放所有内存资源。

进程退出看似简单（"释放所有东西"），但涉及一个核心问题：**CoW 共享页的正确释放**。fork 后父子进程共享物理页（refcount > 1），退出时不能直接释放物理内存，必须等最后一个引用者退出。这就是引用计数系统的最终兑现——每个 `add_ref()` 都必须对应一个 `release_ref()`。

### 1.2 从 fork 的叙事线说起

```
fork 创建进程
  ├── 父子共享物理页，refcount = 2
  ├── 页表标记为只读
  │
  ▼  (写入时)
CoW 执行 (14-cow-mechanism + 15-pagefault)
  ├── 分配新页，复制数据
  ├── 旧页 refcount: 2 → 1
  ├── 新页 refcount = 1
  │
  ▼  (进程退出时)
进程退出 (本文档)
  ├── 遍历所有 VirRegion
  ├── 每个 PhysRegion: unlink_from_block()
  │    ├── refcount: 1 → 0 → 释放物理页
  │    └── refcount: 2 → 1 → 物理页保留（兄弟进程仍在用）
  ├── 释放页表
  └── 清零进程结构体
```

### 1.3 与其他文档的关系

| 文档 | 关系 |
|------|------|
| 10-phys-pagestate | PhysBlock/PFN 引用计数是退出的核心机制 |
| 14-cow-mechanism | CoW 设置的 refcount 在退出时被消费 |
| 15-pagefault | 页错误处理中发现 CoW 条件，退出时释放共享页（最后一页） |
| 16-vm-fork | fork 创建的共享页在退出时被释放 |
| 01-vmproc-struct | `VmProc::clear()` 是退出的核心实现 |
| 02-vmproc-table | typestate 状态转换驱动退出流程 |

---

## 2. C 源码分析：Minix3 退出链路

### 2.1 退出流程概览

Minix3 的进程退出涉及两个阶段，由 PM 和 VM 协作完成：

```
用户调用 exit(status)
  │
  ▼
PM: do_exit()
  ├── 标记进程为 ZOMBIE
  ├── 通知 VM 进程即将退出 (VM_WILLEXIT)
  │
  ▼
VM: do_willexit()
  └── 设置 VMF_EXITING 标志
  │
  ▼
PM: 确认进程可以完全退出
  ├── 发送 VM_EXIT 给 VM
  │
  ▼
VM: do_exit()
  ├── 处理 VM_INSTANCE 计数器
  ├── free_proc(vmp)        ← 释放内存资源
  │    ├── map_free_proc()  ← 释放所有区域
  │    ├── pt_free()        ← 释放页表
  │    └── 重置统计
  └── clear_proc(vmp)       ← 清零进程结构体
       ├── region_init()    ← 重置区域树
       ├── acl_clear()      ← 清除 ACL
       └── vm_flags = 0     ← 清除 IN_USE，slot 变为空闲
```

**两阶段设计的原因**：PM 先通知 VM "进程即将退出"（willexit），VM 设置 EXITING 标志。之后 PM 才发送正式的 exit 请求。这个两阶段设计允许 VM 在 EXITING 状态下拒绝新的内存分配请求，防止即将退出的进程继续消耗内存。

**关键依赖 — vm_isokendpt 分析**：`do_willexit` 和 `do_exit` 都依赖 `vm_isokendpt()` 验证 endpoint 有效性（utility.c:84-93）。该函数按以下优先级验证：

1. endpoint→slot 映射：`ENDPOINT_P(endpoint)` 提取 proc 索引
2. 范围检查：`proc < 0 || proc >= NR_PROCS` → **EINVAL**
3. endpoint 匹配：`endpoint != vmproc[proc].vm_endpoint` → **EDEADEPT**（endpoint 过期）
4. IN_USE 检查：`!(vmproc[proc].vm_flags & VMF_INUSE)` → **EDEADEPT**（slot 已空闲）

注意 do_exit 和 do_willexit 对 `vm_isokendpt` 的所有失败返回统一映射为 **EINVAL**（不区分 EINVAL 和 EDEADEPT）。这是 Minix3 的设计选择——在退出路径中，无论 endpoint 无效还是 slot 已释放，都视为同一类"无效请求"错误。Rust 实现中 typestate 在编译时保证 slot 有效性，`table.vm_isokendpt()` 失败时统一映射为 `VmExitError::ProcessNotFound → EINVAL`。

### 2.2 do_willexit — 预通知

```c
/* minix3/minix/servers/vm/exit.c:100 */
int do_willexit(message *msg)
{
    int proc;
    struct vmproc *vmp;

    if(vm_isokendpt(msg->VMWE_ENDPOINT, &proc) != OK) {
        printf("VM: bogus endpoint VM_EXITING %d\n",
            msg->VMWE_ENDPOINT);
        return EINVAL;
    }
    vmp = &vmproc[proc];

    vmp->vm_flags |= VMF_EXITING;

    return OK;
}
```

**极简实现**：只设置 `VMF_EXITING` 标志。但这个标志有深远影响——后续的内存分配请求（brk、mmap）会检查此标志并拒绝。

### 2.3 do_exit — 正式退出

```c
/* minix3/minix/servers/vm/exit.c:60 */
int do_exit(message *msg)
{
    int proc;
    struct vmproc *vmp;

    if(vm_isokendpt(msg->VME_ENDPOINT, &proc) != OK) {
        printf("VM: bogus endpoint VM_EXIT %d\n", msg->VME_ENDPOINT);
        return EINVAL;
    }
    vmp = &vmproc[proc];

    if(!(vmp->vm_flags & VMF_EXITING)) {
        printf("VM: unannounced VM_EXIT %d\n", msg->VME_ENDPOINT);
        return EINVAL;
    }

    /* 处理 VM_INSTANCE 计数器 */
    if(vmp->vm_flags & VMF_VM_INSTANCE) {
        vmp->vm_flags &= ~VMF_VM_INSTANCE;
        num_vm_instances--;
    }

    /* 释放内存资源 */
    free_proc(vmp);

    /* 清零进程结构体 */
    clear_proc(vmp);

    return OK;
}
```

**关键检查**：`VMF_EXITING` 必须已设置。如果进程没有先调用 willexit 就直接 exit，VM 会报错。这是防御性编程——确保退出流程的完整性。

**VM_INSTANCE 处理**：VM 自身也是一个进程，如果退出的是 VM 实例，需要递减全局计数器 `num_vm_instances`。这在 Minix3 中用于跟踪 VM 服务实例的数量。

### 2.4 free_proc — 释放内存资源

```c
/* minix3/minix/servers/vm/exit.c:33 */
void free_proc(struct vmproc *vmp)
{
    map_free_proc(vmp);           /* 1. 释放所有虚拟区域 */
    pt_free(&vmp->vm_pt);         /* 2. 释放页表 */
    region_init(&vmp->vm_regions_avl); /* 3. 重置区域 AVL 树 */
#if VMSTATS
    vmp->vm_bytecopies = 0;
#endif
    vmp->vm_region_top = 0;
    reset_vm_rusage(vmp);         /* 4. 重置统计 */
}
```

**执行顺序很重要**：
1. **先释放区域**（`map_free_proc`）：遍历所有 VirRegion，对每个 PhysRegion 执行 `pb_unreferenced()`，减少 PhysBlock 引用计数，refcount==0 时释放物理页
2. **再释放页表**（`pt_free`）：释放页表本身占用的物理页（PDPT/PD/PT 页）
3. **最后重置**：清零统计和区域树

为什么先释放区域再释放页表？因为释放区域时可能需要访问页表（例如 `ev_unreference` 回调可能需要更新页表映射）。但实际上 Minix3 的 `ev_unreference` 只释放物理内存，不操作页表，所以顺序理论上可以互换。但保持"先释放内容，再释放容器"的逻辑更清晰。

**reset_vm_rusage 辅助函数**：该 static 函数（exit.c:25-31）被 `free_proc` 和 `clear_proc` 共同调用，清零以下统计字段：`vm_total`（总分配量）、`vm_total_max`（峰值分配量）、`vm_minor_page_fault`（轻微页错误次数）、`vm_major_page_fault`（严重页错误次数）。在 Rust 中此逻辑被合并到 `VmProc::clear()` 方法中。

### 2.5 map_free_proc — 释放所有区域

```c
/* minix3/minix/servers/vm/region.c:589 */
int map_free_proc(struct vmproc *vmp)
{
    struct vir_region *r;

    while((r = region_search_root(&vmp->vm_regions_avl))) {
        region_remove(&vmp->vm_regions_avl, r->vaddr);
        map_free(r);
    }

    region_init(&vmp->vm_regions_avl);
    return OK;
}
```

**循环释放**：不断从 AVL 树中取出根节点，调用 `map_free()` 释放。`region_search_root()` 返回树的根节点，`region_remove()` 从树中移除。每次移除根节点后树会重新平衡，直到树为空。

### 2.6 map_free — 释放单个区域

```c
/* minix3/minix/servers/vm/region.c:568 */
int map_free(struct vir_region *region)
{
    int r;

    /* 1. 释放所有 PhysRegion */
    if((r = map_subfree(region, 0, region->length)) != OK) {
        return r;
    }

    /* 2. 调用 memtype 的 ev_delete 回调 */
    if(region->def_memtype->ev_delete)
        region->def_memtype->ev_delete(region);

    /* 3. 释放 physblocks 数组 */
    free(region->physblocks);
    region->physblocks = NULL;

    /* 4. 释放 VirRegion 本身 */
    SLABFREE(region);

    return OK;
}
```

### 2.7 map_subfree — 释放区域内的所有 PhysRegion

```c
/* minix3/minix/servers/vm/region.c:527 */
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
        pb_unreferenced(region, pr, 1);  /* 释放引用 */
        SLABFREE(pr);                     /* 释放 PhysRegion */
    }

    return OK;
}
```

**逐页释放**：遍历区域内的每个页面，对有 PhysRegion 的页面执行 `pb_unreferenced()` 减少引用计数。

### 2.8 pb_unreferenced — 引用计数释放的核心

```c
/* minix3/minix/servers/vm/pb.c:96 */
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb;

    pb = pr->ph;
    assert(pb->refcount > 0);
    pb->refcount--;

    /* 从 PhysBlock 的链表中移除 PhysRegion */
    if(pb->firstregion == pr) {
        pb->firstregion = pr->next_ph_list;
    } else {
        struct phys_region *others;
        for(others = pb->firstregion; others;
            others = others->next_ph_list) {
            if(others->next_ph_list == pr) {
                others->next_ph_list = pr->next_ph_list;
                break;
            }
        }
    }

    /* 引用计数归零 → 释放物理页 */
    if(pb->refcount == 0) {
        assert(!pb->firstregion);
        int r;
        if((r = pr->memtype->ev_unreference(pr)) != OK)
            panic("unref failed, %d", r);
        SLABFREE(pb);
    }

    pr->ph = NULL;

    if(rm) physblock_set(region, pr->offset, NULL);
}
```

**这是退出的核心函数**。它做了三件事：
1. **减少引用计数**：`pb->refcount--`
2. **从链表中移除**：将 PhysRegion 从 PhysBlock 的链表中摘除
3. **条件释放物理页**：如果 refcount 降到 0，调用 `ev_unreference()` 释放物理内存

**CoW 场景下的行为**：

```
假设 fork 后，物理页 P 被 parent 和 child 共享，refcount = 2

情况 1: child 先退出
  ├── child 的 pr unlink: P.refcount 2 → 1
  ├── P.refcount > 0 → 不释放物理页
  └── parent 仍可正常使用 P（此时 refcount=1，可写）

情况 2: parent 先退出
  ├── parent 的 pr unlink: P.refcount 2 → 1
  ├── P.refcount > 0 → 不释放物理页
  └── child 仍可正常使用 P

情况 3: 两者都退出
  ├── 第一个退出: P.refcount 2 → 1 → 不释放
  └── 第二个退出: P.refcount 1 → 0 → ev_unreference() → free_mem() → 物理页回收
```

### 2.9 anon_unreference — 匿名内存的物理页释放

```c
/* minix3/minix/servers/vm/mem_anon.c:56 */
static int anon_unreference(struct phys_region *pr)
{
    assert(pr->ph->refcount == 0);
    if(pr->ph->phys != MAP_NONE)
        free_mem(ABS2CLICK(pr->ph->phys), 1);  /* 归还物理页给分配器 */
    return OK;
}
```

**极简实现**：refcount 归零时，将物理页归还给 `free_mem()` 分配器。`MAP_NONE` 表示该 PhysBlock 没有分配物理页（延迟分配场景），不需要释放。

### 2.10 pt_free — 释放页表

```c
/* minix3/minix/servers/vm/pagetable.c:1427 */
void pt_free(pt_t *pt)
{
    int i;

    for(i = 0; i < ARCH_VM_DIR_ENTRIES; i++)
        if(pt->pt_pt[i])
            vm_freepages((vir_bytes) pt->pt_pt[i], 1);

    return;
}
```

**释放页表页**：遍历页目录的所有条目，释放已分配的中间页表页（PDPT/PD/PT）。页目录本身也在其中。

**注意**：`vm_freepages()` 释放的是 VM 的内核虚拟地址空间中的页面，这些页面是 VM 通过 `_brk()` 分配的。释放后这些虚拟地址空间可以被重用。

### 2.11 clear_proc — 清零进程结构体

```c
/* minix3/minix/servers/vm/exit.c:45 */
void clear_proc(struct vmproc *vmp)
{
    region_init(&vmp->vm_regions_avl);
    acl_clear(vmp);
    vmp->vm_flags = 0;      /* 清除 IN_USE，slot 变为空闲 */
#if VMSTATS
    vmp->vm_bytecopies = 0;
#endif
    vmp->vm_region_top = 0;
    reset_vm_rusage(vmp);
}
```

**与 free_proc 的区别**：
- `free_proc()`：释放**资源**（内存区域、页表、物理页）
- `clear_proc()`：重置**元数据**（标志位、ACL、统计），使 slot 可重用

`clear_proc()` 最关键的操作是 `vm_flags = 0`，这清除了 `VMF_INUSE` 标志，使该 slot 可以被新进程使用。

### 2.12 do_procctl — 进程控制操作

```c
/* minix3/minix/servers/vm/exit.c:117 */
int do_procctl(message *msg, int transid)
{
    endpoint_t proc;
    struct vmproc *vmp;

    if(vm_isokendpt(msg->VMPCTL_WHO, &proc) != OK)
        return EINVAL;
    vmp = &vmproc[proc];

    switch(msg->VMPCTL_PARAM) {
    case VMPPARAM_CLEAR:
        /* 只有 RS 和 VFS 可以调用 */
        if(msg->m_source != RS_PROC_NR
            && msg->m_source != VFS_PROC_NR)
            return EPERM;
        free_proc(vmp);               /* 释放内存资源 */
        if(pt_new(&vmp->vm_pt) != OK) /* 创建新的空页表 */
            panic("VMPPARAM_CLEAR: pt_new failed");
        pt_bind(&vmp->vm_pt, vmp);    /* 绑定到进程 */
        return OK;

    case VMPPARAM_HANDLEMEM:
        /* VFS 专用：处理内存映射 */
        if(msg->m_source != VFS_PROC_NR)
            return EPERM;
        handle_memory_start(vmp, msg->VMPCTL_M1,
            msg->VMPCTL_LEN, msg->VMPCTL_FLAGS,
            VFS_PROC_NR, VFS_PROC_NR, transid, 1);
        return SUSPEND;

    default:
        return EINVAL;
    }
}
```

**VMPPARAM_CLEAR 的特殊之处**：只释放内存资源（`free_proc`），但**不清零进程结构体**（不调用 `clear_proc`）。之后立即创建新页表并绑定。这是 RS（Reincarnation Server）用于服务重启的场景——同一个进程 slot，新的内存空间。

### 2.13 完整退出链路总结

```
进程退出完整链路

PM → VM_WILLEXIT:
  └── vm_flags |= VMF_EXITING

PM → VM_EXIT:
  ├── 验证 endpoint + VMF_EXITING
  ├── 处理 VM_INSTANCE 计数器
  │
  ├── free_proc():
  │    ├── map_free_proc():
  │    │    └── 循环: region_search_root() → map_free():
  │    │         ├── map_subfree():
  │    │         │    └── 循环: pb_unreferenced(region, pr, rm=1):
  │    │         │         ├── pb.refcount--
  │    │         │         ├── 从 pb.firstregion 链表移除 pr
  │    │         │         ├── if refcount == 0:
  │    │         │         │    └── pr->memtype->ev_unreference(pr):
  │    │         │         │         └── free_mem(pb.phys)  [匿名内存]
  │    │         │         └── SLABFREE(pr)
  │    │         ├── def_memtype->ev_delete(region)
  │    │         ├── free(region->physblocks)
  │    │         └── SLABFREE(region)
  │    │
  │    ├── pt_free():
  │    │    └── 循环: vm_freepages(pt.pt_pt[i])
  │    │
  │    ├── region_init()
  │    └── reset_vm_rusage()
  │
  └── clear_proc():
       ├── region_init()
       ├── acl_clear()
       └── vm_flags = 0  ← slot 变为空闲
```

---

## 3. Rust 设计决策

> **本章基于 Ch1&Ch2 分析，确定 Rust 实现的设计决策。每节标注了 Ch2 依据。**

### 3.1 Typestate 驱动退出流程

**决策**：编译时 typestate 替代 Minix3 的运行时 `VMF_EXITING` 标志位检查。

**依据**（Ch2§2.2-§2.3）：

```
Minix3 两阶段退出:                     Rust typestate:
  do_willexit → VMF_EXITING              ActiveProc → mark_exiting() → ExitingProc
  do_exit → check VMF_EXITING            table.get_exiting(slot) → ExitingProc
  free_proc + clear_proc                 exiting.reap() → EmptySlot
  slot 变为空闲                           slot 可被新进程使用
```

**设计对比**：

| 状态 | Minix3（运行时 flags） | Rust（编译时 typestate） |
|------|----------------------|------------------------|
| 空闲 | `vm_flags = 0` | `EmptySlot` — 无 endpoint，只能 `activate()` |
| 正常 | `VMF_INUSE` | `ActiveProc` — 有 endpoint+page_table+regions |
| 预通知 | `VMF_INUSE \| VMF_EXITING` | `ExitingProc` — 只能 `reap()`，不能分配内存 |
| 释放后 | `vm_flags = 0` | `EmptySlot`（由 `reap()` 返回） |

**关键**：`mark_exiting()` 消费 `ActiveProc`（拿走 `&mut VmProc` 引用），返回 `ExitingProc`。之后 `handle_vm_exit` 通过 `table.get_exiting(slot)` 获取 `ExitingProc`，在类型层面保证"先 willexit 再 exit"——如果 `get_exiting()` 返回 `None`（即进程不在 EXITING 状态），直接返回错误。

**不采用**：
- 单一 `enum VmState { Active, Exiting }` — 失去编译时安全，运行时仍需 assert
- 继续使用 Minix3 的 flags 方式 — C 式运行时检查，不是 Rewrite 目标

### 3.2 PFN 模型简化物理页释放

**决策**：不逐个显式 "unlink"，利用 PageFrames 全局引用计数 + `free_range()` 统一释放。

**依据**（Ch2§2.5-§2.9）：

Minix3 的退出物理页释放链路是：`map_free_proc → map_free → map_subfree → pb_unreferenced(rm=1) → refcount-- → if 0 then ev_unreference → free_mem → SLABFREE(pr) + SLABFREE(pb)`。这一连串操作的核心是两件事：**减少引用计数** 和 **条件释放物理页**。

在 PFN 模型中，`PageSlot` 是 `Copy` 类型（pfn + memtype），物理页引用计数集中在 `PageFrames.states[pfn].refcount` 全局数组中，不再由 `PhysBlock` 侵入式链表管理。退出释放步骤：

```
1. 遍历 region.physblocks 中所有 mapped PageSlot（pfn != PFN_NONE）：
   a. 递减 PageFrames refcount（state.refcount--，等价于 Minix3 的 pb.refcount--）
   b. slot.memtype.ev_unreference(frames, slot.pfn) — MemType 回调
      （注意：anon/direct 的 ev_unreference 在 PFN 模型中为空操作，
       物理页的实际释放由 caller 通过 free_pfn 执行，而不是在回调中归还。
       这是 PFN 模型的职责分离设计——PageFrames 管理引用计数，
       PfnAllocator 管理物理页分配/归还。）
   c. 若 refcount == 0 且非缓存页：page_alloc.free_pfn(slot.pfn) — 归还物理页
2. RegionMap::clear() — 释放 BTreeMap → VirRegion → physblocks Vec
   （Vec 的 drop 自动回收堆内存，PageSlot 是 Copy 无需手动释放）
```

**对比**：

| 操作 | Minix3 | Rust (PFN) |
|------|--------|------------|
| 释放入口 | `map_free_proc()` | `free_region_pages()` + `RegionMap::clear()` |
| 引用计数 | `pb.refcount--`（侵入式链表） | `refcount--`（PageFrames 全局数组） |
| 物理页归还 | `free_mem(ABS2CLICK(...), 1)` | `page_alloc.free_pfn(pfn)` |
| PhysRegion 释放 | `SLABFREE(pr)` | `Vec<Option<PageSlot>>` drop 自动回收 |
| PhysBlock 释放 | `SLABFREE(pb)` | 不需要（PFN 模型没有 PhysBlock 堆分配） |

**不采用**：为 `PageSlot` 添加 `Drop` — `PageSlot` 是 Copy，且物理页生命周期由 `PageFrames` 统一管理，不应在 drop 中隐式释放。

### 3.3 错误处理

**决策**：`VmExitError` 独立枚举，错误语义与 Minix3 exit.c 严格对齐。

**依据**（Ch2§2.2, §2.3, §2.12）：

| VmExitError 变体 | errno | Minix3 来源 | 触发条件 |
|-----------------|-------|------------|---------|
| `ProcessNotFound` | EINVAL | `vm_isokendpt()` 失败 | endpoint 无效或进程不存在 |
| `NotExiting` | EINVAL | do_exit 检查 `!(VMF_EXITING)` | 未先 willexit 就 exit |

**与 Minix3 的差异说明**：Minix3 的 `vm_isokendpt` 对无效 endpoint 可返回 EINVAL（proc 越界）或 EDEADEPT（endpoint 过期/slot 已空闲）。do_exit/do_willexit 将两者统一映射为 EINVAL。Rust 中 typestate 在编译时保证 slot 有效性，`table.vm_isokendpt()` 失败时映射为 `ProcessNotFound → EINVAL`，不再区分 EDEADEPT 场景。

Minix3 的 do_exit 在 `!(VMF_EXITING)` 时返回 EINVAL，语义是"未预告退出"（unannounced exit），不是"已退出"。Rust 中 typestate 在编译时保证顺序，但 `get_exiting()` 仍然可能返回 `None`（并发场景或 bug），对应 `NotExiting`。

### 3.4 IPC 类型

**决策**：复用已有的 minix-types `VmExitIn`/`VmWillexitIn`，无需新增类型。

| 类型 | 对应 C 宏 | 方向 | 字段 |
|------|----------|------|------|
| `VmExitIn` | `VME_ENDPOINT` (m1_i1) | PM→VM | `endpoint: Endpoint` |
| `VmWillexitIn` | `VMWE_ENDPOINT` (m1_i1) | PM→VM | `endpoint: Endpoint` |

Minix3 的 `VM_PROCCTL`（Ch2§2.12）支持 `VMPPARAM_CLEAR`（释放内存但保留 slot）和 `VMPPARAM_HANDLEMEM`（VFS 内存处理），但不是退出核心路径。`VMPPARAM_HANDLEMEM` 涉及的 VFS 交互（fdref 释放）见 [23-vfs-interaction.md](23-vfs-interaction.md) §4.5，标记为 **Phase N**。

### 3.5 与已有服务的模式一致性

**决策**：退出服务遵循 fork/brk/munmap 建立的模式。

| 方面 | fork | brk | munmap | exit |
|------|------|-----|--------|------|
| 模块文件 | fork.rs | brk.rs | munmap.rs | exit.rs |
| 入口函数 | `do_fork()` | `handle_brk()` | `handle_munmap()` | `handle_vm_exit()` / `handle_vm_willexit()` |
| 错误类型 | `ForkError` | `BrkError` | `MunmapError` | `VmExitError` |
| Typestate | Empty→Active | 从 table 取 Active | 从 table 取 Active | Active→Exiting→Empty |
| PageFrames | refcount++ | 分配新页 | refcount-- + unmap | refcount-- + free_pfn |
| VFS 交互 | 无 | 无 | 无 | 无 |
| IPC 响应 | `VmReply::Fork` | `VmReply::Brk` | `VmReply::Munmap` | `VmReply::Exit` / `VmReply::Willexit` |

退出是唯一涉及 typestate "销毁"转换（Active→Exiting→Empty）的服务——fork/brk/munmap 只持有 ActiveProc 的可变引用，不改变其生命周期。

**架构演进 — 页表层级**：Minix3（32位）使用 2 级页表（PD→PT），`pt_free` 释放 `pt_pt[ARCH_VM_DIR_ENTRIES]`（1024 个条目）。minix-rs（64位）使用 4 级页表（PML4→PDPT→PD→PT），`PageTable::destroy()` 递归释放所有 4 级页表页。这一差异不影响退出流程的正确性，因为两个实现都是"遍历→释放页表页"的同构操作，仅遍历深度不同。

---

## 4. 实现详解

> **本章描述实际 Rust 代码结构。每个 § 对应一个源文件或关键流程，标注了与 Ch2 C 源码的对应关系。**

### 4.1 模块结构

```
os/servers/vm/src/
├── exit.rs                  # handle_vm_exit() + handle_vm_willexit() + VmExitError
├── vmproc/
│   ├── vmproc.rs            # VmProc + clear()（对应 free_proc + clear_proc）
│   └── vmproc_handle.rs     # ActiveProc / ExitingProc / EmptySlot typestate views
├── region/
│   ├── mod.rs               # free_region_pages()（对应 map_free/map_subfree）
│   ├── vir_region.rs        # VirRegion + free_range()
│   └── page_state.rs        # PageFrames + refcount 管理
├── ipc/
│   └── dispatcher.rs        # dispatch_exit / dispatch_willexit
└── vm_server.rs             # handle_exit 入口（handle_willexit 直接通过 dispatcher）

os/libs/minix-types/src/ipc/
└── vm.rs                    # VmExitIn / VmWillexitIn / VmReply::{Exit, Willexit}
```

### 4.2 exit.rs — handle_vm_willexit

**对应 C 源码**：`minix3/minix/servers/vm/exit.c` 的 `do_willexit()` (L100-115)

```
handle_vm_willexit(table, endpoint) → Result<(), VmExitError>
```

| 步骤 | Rust | Minix3 C |
|------|------|----------|
| 1. 验证端点 | `table.vm_isokendpt(endpoint)` | `vm_isokendpt(msg->VMWE_ENDPOINT, &proc)` |
| 2. 获取进程 | `table.get_active(slot)` | `vmp = &vmproc[proc]` |
| 3. 设置标志 | `active.mark_exiting()` | `vmp->vm_flags \|= VMF_EXITING` |
| 4. 返回 | drop(ExitingProc) 释放 &mut VmProc | `return OK` |

> **设计决策**：§3.1（typestate 驱动）。`mark_exiting()` 消费 `ActiveProc`，返回 `ExitingProc`。drop `ExitingProc` 释放 `&mut VmProc` 引用但 VmProc 内部 flags 已包含 EXITING——后续 `get_exiting()` 可以获取此进程。

```rust
pub(crate) fn handle_vm_willexit(
    table: &VmProcTable,
    endpoint: Endpoint,
) -> Result<(), VmExitError> {
    let slot = table.vm_isokendpt(endpoint)
        .map_err(|_| VmExitError::ProcessNotFound)?;

    let active = table.get_active(slot)
        .ok_or(VmExitError::ProcessNotFound)?;

    let _exiting = active.mark_exiting();

    Ok(())
}
```

### 4.3 exit.rs — handle_vm_exit

**对应 C 源码**：`minix3/minix/servers/vm/exit.c` 的 `do_exit()` (L60-94)

```
handle_vm_exit(table, page_alloc, frames, endpoint) → Result<(), VmExitError>
```

| 步骤 | Rust | Minix3 C | 在哪个函数 |
|------|------|----------|-----------|
| 1. 验证端点 | `table.vm_isokendpt(endpoint)` | `vm_isokendpt(msg->VME_ENDPOINT, &proc)` | `handle_vm_exit` |
| 2. 获取 ExitingProc | `table.get_exiting(slot)` | 检查 `!(vmp->vm_flags & VMF_EXITING)` | `handle_vm_exit` |
| 3. 释放物理页 | `free_process_phys` → ev_unreference + free_pfn | `free_proc → map_free_proc → map_subfree → pb_unreferenced → free_mem` | `handle_vm_exit` |
| 4. 释放区域树 | `RegionMap::clear()` | `region_init()` | `reap()` → `VmProc::clear()` |
| 5. 释放页表 | `PageTable::destroy()` | `pt_free(&vmp->vm_pt)` | `reap()` → `VmProc::clear()` |
| 6. 重置结构 | `vm_flags=empty()` 等 | `clear_proc(vmp)` → `vm_flags = 0` | `reap()` → `VmProc::clear()` |

> **关键**：`handle_vm_exit` 负责步骤 1-3（验证 + 物理页释放），`exiting.reap()` 负责步骤 4-6（数据结构释放 + 元数据重置）。物理页释放必须在 `reap()` **之前**完成——因为 `RegionMap::clear()` 只释放数据结构，不处理 PageFrames 引用计数。

```rust
pub(crate) fn handle_vm_exit(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    endpoint: Endpoint,
) -> Result<(), VmExitError> {
    let slot = table.vm_isokendpt(endpoint)
        .map_err(|_| VmExitError::ProcessNotFound)?;

    // Typestate guarantees: only ExitingProc can be reaped
    let exiting = table.get_exiting(slot)
        .ok_or(VmExitError::NotExiting)?;

    // 1. Free physical pages (PFN refcounts)
    free_process_phys(exiting.regions(), frames, page_alloc);

    // 2. Release all resources + reset fields → EmptySlot
    unsafe { exiting.reap(); }

    Ok(())
}
```

### 4.4 物理页释放流程（PFN 模型）

**对应 C 源码**：`map_subfree` (region.c:527-563) → `pb_unreferenced` (pb.c:96-133) → `anon_unreference` (mem_anon.c:56-62)

```rust
/// Release physical pages for all regions in the exiting process.
///
/// Corresponds to Minix3's map_free_proc() → map_free() → map_subfree() chain.
/// For each PageSlot: decrements PageFrames refcount, calls ev_unreference,
/// and conditionally frees physical pages when refcount reaches 0.
/// RegionMap::clear() (inside reap()) handles releasing the data structures.
fn free_process_phys(
    regions: &RegionMap,
    frames: &mut PageFrames,
    page_alloc: &mut VmPageAllocator,
) {
    for region in regions.iter() {
        for slot_opt in &region.physblocks {
            if let Some(slot) = slot_opt {
                if slot.pfn == PFN_NONE {
                    continue;
                }
                if let Some(mt) = slot.memtype {
                    mt.ev_unreference(frames, slot.pfn);
                }
                let should_free = if let Some(state) = frames.get_mut(slot.pfn) {
                    if state.refcount > 0 {
                        state.refcount -= 1;
                    }
                    state.refcount == 0 && !state.flags.contains(PageFlags::IN_CACHE)
                } else {
                    false
                };
                if should_free {
                    page_alloc.free_pfn(slot.pfn);
                }
            }
        }
    }
}
```

**与 Minix3 的关键差异**（详细宏观对比见 §3.2，此处聚焦实现层面的差异）：

| 步骤 | Minix3 | Rust (PFN) |
|------|--------|------------|
| 遍历区域 | `region_search_root` 循环 | `regions.iter()` |
| 查找 PhysRegion | `physblock_get(region, voffset)` | `physblocks[i]`（Vec 索引） |
| 释放 physblocks 数组 | `free(region->physblocks)` | Vec drop 自动 |
| 释放 VirRegion | `SLABFREE(region)` | Box drop 自动 |

### 4.5 Typestate 转换链

**对应 Minix3 do_willexit → do_exit 的两阶段协议**：

```
PM 发送 VM_WILLEXIT
  └── dispatcher → handle_vm_willexit(table, endpoint):
        ├── table.vm_isokendpt(endpoint) → slot
        ├── table.get_active(slot) → ActiveProc
        └── active.mark_exiting() → ExitingProc
             └── drop(ExitingProc) → 释放 &mut VmProc（flags 保留 EXITING）

PM 发送 VM_EXIT
  └── dispatcher → handle_vm_exit(table, page_alloc, frames, endpoint):
        ├── table.vm_isokendpt(endpoint) → slot
        ├── table.get_exiting(slot) → ExitingProc  ← typestate 编译时保证
        ├── free_process_phys(exiting.regions(), frames, page_alloc)
        └── exiting.reap():
             └── VmProc::clear():
                  ├── RegionMap::clear()      ← 释放 BTreeMap + VirRegion 节点
                  ├── PageTable::destroy()    ← 释放页表物理页
                  ├── if VM_INSTANCE: dec_vm_instance()
                  ├── vm_flags = empty()      ← 清除 IN_USE/EXITING/VM_INSTANCE
                  ├── vm_endpoint = NONE      ← 重置 endpoint
                  ├── vm_boot = None          ← 清除 boot 信息
                  ├── vm_acl = Uninitialized  ← ACL 回到未初始化状态
                  ├── vm_pt_initialized = false
                  ├── vm_regions_initialized = false
                  ├── vm_total = 0, vm_total_max = 0  ← 统计清零
                  └── vm_minor_fault = 0, vm_major_fault = 0
                  → EmptySlot → slot 可被新进程使用
```

**对比 Minix3**：

| 状态 | Minix3 | Rust |
|------|--------|------|
| 两阶段顺序保证 | 运行时 `if(!(vmp->vm_flags & VMF_EXITING))` | 编译时 typestate（只有 `get_exiting()` 能获取 ExitingProc） |
| 内存释放 | `free_proc()` 显式调用 | `free_process_phys()` + `reap().clear()` |
| 结构清零 | `clear_proc()` 显式调用 | `VmProc::clear()` 在 `reap()` 中 |
| Slot 复用 | `vm_flags = 0` 后 `get_empty()` 可用 | `reap()` 返回 `EmptySlot` |

### 4.6 Dispatcher 和 VmServer 入口

**dispatcher.rs** — 错误映射：

```rust
fn exit_error_to_vm_error(e: exit::VmExitError) -> VmError {
    match e {
        exit::VmExitError::ProcessNotFound => VmError::InvalidEndpoint,
        exit::VmExitError::NotExiting => VmError::InvalidEndpoint,
    }
}
```

**vm_server.rs** — 传入 PageFrames：

```rust
pub fn handle_exit(&mut self, req: VmExitIn) -> VmReply {
    let table = VmProcTable::get_global();
    let frames = self.page_frames.as_mut().expect("page_frames not initialized");
    MessageDispatcher::dispatch_exit(table, &mut self.page_alloc, frames, req)
}
```

> **注意**：`handle_willexit` 在 vm_server.rs 中尚未实现（Phase N TODO），当前 willexit 请求通过 dispatcher 直接调用 `exit::handle_vm_willexit`。
```

### 4.7 C-Rust 对应关系

| Minix3 函数 | 源码位置 | Rust 对应 | 代码位置 |
|------------|---------|----------|---------|
| `do_willexit()` | exit.c:100 | `handle_vm_willexit()` | exit.rs |
| `do_exit()` | exit.c:60 | `handle_vm_exit()` | exit.rs |
| `free_proc()` | exit.c:33 | `free_process_phys()` + `VmProc::clear()` | exit.rs + vmproc.rs |
| `clear_proc()` | exit.c:45 | `VmProc::clear()`（在 reap 中） | vmproc.rs:152 |
| `map_free_proc()` | region.c:589 | `free_process_phys()` 的遍历逻辑 | exit.rs |
| `map_free()` | region.c:568 | `free_process_phys()` 遍历单区域 | exit.rs |
| `map_subfree()` | region.c:527 | `free_process_phys()` 遍历 physblocks | exit.rs |
| `pb_unreferenced()` | pb.c:96 | `ev_unreference` + `free_pfn` | exit.rs |
| `anon_unreference()` | mem_anon.c:56 | `MemType::ev_unreference` | memtype.rs |
| `pt_free()` | pagetable.c:1427 | `PageTable::destroy()` | pagetable.rs |
| `VMF_EXITING` | vmproc.h:36 | `VmFlags::EXITING` (∈ `ExitingProc` typestate) | vmproc_handle.rs |

---

## 5. 测试要点

> **本节基于 Ch2 错误场景和 Ch3 设计决策，列出测试覆盖要点。**

### 5.1 退出流程测试

| 测试场景 | 验证点 |
|---------|--------|
| 正常两阶段退出 | willexit → exit → slot 变为 EmptySlot |
| 未 willexit 直接 exit | `table.get_exiting()` 返回 None → `NotExiting` |
| 重复 willexit | `mark_exiting()` 在 EXITING 标志已设置时的行为 |
| exit 后 slot 可复用 | `EmptySlot.activate()` 创建新进程 |

### 5.2 物理页释放测试

| 测试场景 | 验证点 |
|---------|--------|
| 独立进程退出 | 所有物理页 refcount 归零，归还 allocator |
| CoW 共享页，一个进程退出 | 共享页 refcount 从 2 降到 1，页保留 |
| CoW 共享页，两个进程都退出 | 共享页 refcount 从 2→1→0，最终释放 |
| CoW 已执行，各自拥有独立页 | 各进程释放自己的私有页 |
| 三进程 fork 树，逐个退出 | refcount 3→2→1→0 正确递减 |

### 5.3 资源完整性测试

| 测试场景 | 验证点 |
|---------|--------|
| 区域树清空 | exit 后 `regions.len() == 0` |
| 页表释放 | exit 后 `vm_pt_initialized == false` |
| VM_INSTANCE 计数器 | VM 实例退出后 `dec_vm_instance()` 被调用 |
| 统计归零 | `vm_total = 0`, `vm_minor_page_fault = 0` 等 |
| 标志清零 | `vm_flags = empty()` |

---

## 6. 参见

- [01-vmproc-struct.md](01-vmproc-struct.md) — VmProc 结构体定义
- [02-vmproc-table.md](02-vmproc-table.md) — 进程表 + typestate 视图
- [10-phys-pagestate.md](10-phys-pagestate.md) — PhysBlock 引用计数（C 侧）
- [14-cow-mechanism.md](14-cow-mechanism.md) — CoW 机制，退出时 refcount 递减的"正向"操作
- [15-pagefault.md](15-pagefault.md) — 页错误处理，退出时触发 CoW 条件的"前置"操作
- [16-vm-fork.md](16-vm-fork.md) — fork 创建进程，退出是 fork 的生命周期终点
