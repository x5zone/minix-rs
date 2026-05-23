# 20-vm-exit: 进程退出与资源释放

> **分类**: VM服务
> **源码**: `minix3/minix/servers/vm/exit.c`, `region.c`, `pb.c`, `pagetable.c`
> **说明**: fork 的反面——进程退出时如何释放内存资源，兑现 CoW 引用计数的最终承诺

---

## 1. 概述

### 1.1 本文档的定位

16-vm-fork 创建进程，19-cow-exec-pagefault 处理运行时 CoW，本文档是生命周期的终点——进程退出时释放所有内存资源。

进程退出看似简单（"释放所有东西"），但涉及一个核心问题：**CoW 共享页的正确释放**。fork 后父子进程共享物理页（refcount > 1），退出时不能直接释放物理内存，必须等最后一个引用者退出。这就是引用计数系统的最终兑现——每个 `add_ref()` 都必须对应一个 `release_ref()`。

### 1.2 从 fork 的叙事线说起

```
fork 创建进程
  ├── 父子共享物理页，refcount = 2
  ├── 页表标记为只读
  │
  ▼  (写入时)
CoW 执行 (19-cow-exec-pagefault)
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
| 10-phys-block | PhysBlock 引用计数是退出的核心机制 |
| 14-phys-region | `unlink_from_block()` 是退出的核心操作 |
| 14-cow-mechanism | CoW 设置的 refcount 在退出时被消费 |
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

### 2.2 do_willexit — 预通知

```c
/* minix3/minix/servers/vm/exit.c:112 */
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
/* minix3/minix/servers/vm/exit.c:68 */
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
/* minix3/minix/servers/vm/exit.c:28 */
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
/* minix3/minix/servers/vm/exit.c:42 */
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
/* minix3/minix/servers/vm/exit.c:131 */
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
        pt_new(&vmp->vm_pt);          /* 创建新的空页表 */
        pt_bind(&vmp->vm_pt, vmp);    /* 绑定到进程 */
        return OK;

    case VMPPARAM_HANDLEMEM:
        /* VFS 专用：处理内存映射 */
        if(msg->m_source != VFS_PROC_NR)
            return EPERM;
        handle_memory_start(vmp, ...);
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

### 3.1 现有代码状态

| 组件 | 现有状态 | 退出需要的操作 |
|------|---------|---------------|
| `VmProc::clear()` | ✅ 已实现 | 核心：释放区域 + 页表 + 重置字段 |
| `ExitingProc::reap()` | ✅ 已实现 | 调用 `VmProc::clear()`，返回 `EmptySlot` |
| `ActiveProc::force_clear()` | ✅ 已实现 | 异常终止：调用 `VmProc::clear()` |
| `RegionMap::clear()` | ✅ 已实现 | 释放所有 VirRegion 节点 |
| `PageTable::destroy()` | ✅ 已实现（trait） | 释放页表资源 |
| `PhysRegion::unlink_from_block()` | ✅ 已实现 | 从 PhysBlock 链表移除，减少引用计数 |
| `PhysRegion::unbind_block()` | ✅ 已实现 | 简化版引用释放（无链表管理） |
| `AnonymousMemory::on_unreference()` | ✅ 已实现 | refcount==0 时返回 `Ok(true)` |
| `VirRegion::free_range()` | ✅ 已实现 | 释放范围内物理页 |
| `MemType::on_delete()` | ✅ 已实现（默认空） | 区域删除回调 |
| `do_exit()` | ❌ 不存在 | VM_EXIT IPC 处理 |
| `do_willexit()` | ❌ 不存在 | VM_WILLEXIT IPC 处理 |
| `do_procctl()` | ❌ 不存在 | VM_PROCCTL IPC 处理 |
| 物理页归还分配器 | ❌ 缺失 | `ev_unreference` 返回 true 后无释放逻辑 |

### 3.2 设计原则

**原则 1：typestate 驱动退出流程**

Minix3 用标志位（`VMF_EXITING`）控制状态，Rust 用 typestate 编译时保证：

```
Minix3:                              Rust:
Active + VMF_EXITING=0               ActiveProc
Active + VMF_EXITING=1               ExitingProc
!VMF_INUSE                           EmptySlot
```

退出流程：
```rust
let active = table.get_active(slot)?;
let exiting = active.mark_exiting();  // 对应 do_willexit
let empty = unsafe { exiting.reap() }; // 对应 do_exit
```

**原则 2：VmProc::clear() 是退出的核心**

Minix3 将退出拆分为 `free_proc()` + `clear_proc()` 两个函数。Rust 的 `VmProc::clear()` 合并了两者：

```rust
pub(crate) unsafe fn clear(&mut self) {
    // === free_proc 部分 ===
    if self.vm_regions_avl_initialized {
        self.vm_regions_avl.assume_init_mut().clear(); // map_free_proc
    }
    if self.vm_pt_initialized {
        self.vm_pt.assume_init_mut().destroy();        // pt_free
    }
    if self.vm_flags.contains(VmFlags::VM_INSTANCE) {
        crate::global::dec_vm_instance();              // VM_INSTANCE 处理
    }

    // === clear_proc 部分 ===
    self.vm_flags = VmFlags::empty();                  // 清除 IN_USE
    self.vm_endpoint = Endpoint::NONE;
    self.vm_acl = AclState::Uninitialized;             // acl_clear
    self.vm_region_top = VirBytes::new(0);
    self.vm_total = VirBytes::default();               // reset_vm_rusage
    self.vm_total_max = VirBytes::default();
    self.vm_minor_page_fault = 0;
    self.vm_major_page_fault = 0;
    // ...
}
```

**合并的原因**：
1. Minix3 的 `free_proc()` 和 `clear_proc()` 总是成对调用（`do_exit` 中 `free_proc` → `clear_proc`），没有单独调用 `clear_proc` 的场景
2. `do_procctl(VMPPARAM_CLEAR)` 只调用 `free_proc` 不调用 `clear_proc`，但 Rust 中这个场景用 `force_clear()` + 重新初始化来处理
3. 合并减少了遗漏调用的风险

**原则 3：RegionMap::clear() 需要增强**

当前的 `RegionMap::clear()` 释放 BTreeMap 中的所有 VirRegion 节点，但**不处理 PhysRegion 的引用计数**：

```rust
// 当前实现 — 只释放树结构
fn clear(&mut self) {
    // BTreeMap drop → Box<VirRegion> drop → Vec<Option<Box<PhysRegion>>> drop
    // 但 PhysRegion 的 drop 不会调用 unlink_from_block()!
    self.regions.clear();
}
```

**问题**：`PhysRegion` 的 `Drop` 实现没有调用 `unlink_from_block()`，因为 `PhysRegion` 不拥有 `PhysBlock`（它只是引用）。直接 drop 会导致：
- PhysBlock 的 refcount 不减少 → 物理页泄漏
- PhysBlock 的 firstregion 链表悬空指针 → UB

**解决方案**：在 `RegionMap::clear()` 中，先遍历所有 VirRegion 的 PhysRegion，调用 `unlink_from_block()` 释放引用，再清空 BTreeMap。

**原则 4：物理页归还分配器**

`AnonymousMemory::on_unreference()` 返回 `Ok(true)` 表示物理页应该被释放，但当前代码没有消费这个返回值。需要在退出流程中：

```rust
if pr.unlink_from_block() {
    // refcount 降到 0，需要释放物理页
    if let Some(memtype) = pr.memtype {
        if memtype.ev_unreference(pr)? {
            // 归还物理页给分配器
            page_alloc.free_phys(old_phys, 1);
        }
    }
}
```

### 3.3 与 Minix3 的关键差异

| 方面 | Minix3 | minix-rs |
|------|--------|----------|
| 状态管理 | `VMF_EXITING` 标志位 | `ExitingProc` typestate |
| 退出函数 | `free_proc()` + `clear_proc()` | `VmProc::clear()` 合并 |
| 区域释放 | `map_free_proc()` + `map_free()` + `map_subfree()` | `RegionMap::clear()` + 增强 |
| 引用释放 | `pb_unreferenced()` | `PhysRegion::unlink_from_block()` |
| 物理页释放 | `ev_unreference()` → `free_mem()` | `on_unreference()` → `page_alloc.free_phys()` |
| 页表释放 | `pt_free()` → `vm_freepages()` | `PageTable::destroy()` |
| 进程控制 | `do_procctl(VMPPARAM_CLEAR)` | `ActiveProc::force_clear()` + 重新初始化 |
| 内存安全 | 运行时 assert | 编译时 typestate + 运行时 debug_assert |

---

## 4. Rust 实现详解

### 4.1 RegionMap::clear() 增强 — 释放 PhysRegion 引用

当前实现只释放树结构，需要增加引用计数释放逻辑：

```rust
impl RegionMap {
    pub(crate) fn clear(&mut self) {
        // 先释放所有 PhysRegion 的引用
        for (_, region) in self.regions.iter_mut() {
            Self::free_region_phys(region);
        }
        // 再清空 BTreeMap
        self.regions.clear();
    }
        // Box<VirRegion> drop → Vec drop → PhysRegion drop
    }
}
```

**问题**：`RegionMap::clear()` 无法访问 `VmPageAllocator` 来归还物理页。

**解决方案**：将物理页释放逻辑提升到 `VmProc::clear()` 层级，`RegionMap::clear()` 只负责释放引用计数和 BTreeMap 结构：

```rust
impl RegionMap {
    /// 释放所有区域，归还物理页给分配器
    ///
    /// 对应 Minix3 的 map_free_proc()。
    pub(crate) fn free_all(&mut self, page_alloc: &mut VmPageAllocator) {
        for (_, region) in self.regions.iter() {
            Self::free_region_phys(region, page_alloc);
        }
        self.regions.clear();
    }

    fn free_region_phys(region: &VirRegion, page_alloc: &mut VmPageAllocator) {
        for phys_opt in region.physblocks.iter() {
            if let Some(pr) = phys_opt {
                let should_free = pr.unlink_from_block();
                if should_free {
                    if let Some(memtype) = pr.memtype {
                        if let Ok(true) = memtype.ev_unreference(pr) {
                            if let Some(phys) = pr.get_phys_addr() {
                                page_alloc.free_phys(phys, 1);
                            }
                        }
                    }
                }
            }
        }
    }
}
```

**注意**：`unlink_from_block()` 需要 `&mut PhysRegion`，但 `physblocks` 是 `Vec<Option<Box<PhysRegion>>>`，遍历时可以获取可变引用。但 `on_unreference()` 也需要 `&mut PhysRegion`，而 `unlink_from_block()` 已经修改了 `pr.ph = None`。需要仔细设计调用顺序。

### 4.2 VmProc::clear() 增强 — 传入 page_alloc

当前 `VmProc::clear()` 无法访问 `VmPageAllocator`，需要修改签名：

```rust
impl VmProc {
    pub(crate) unsafe fn clear(&mut self, page_alloc: &mut VmPageAllocator) {
        // 1. 释放所有区域（包括 PhysRegion 引用和物理页归还）
        if self.vm_regions_avl_initialized {
            self.vm_regions_avl.assume_init_mut().free_all(page_alloc);
        }

        // 2. 释放页表
        if self.vm_pt_initialized {
            self.vm_pt.assume_init_mut().destroy();
        }

        // 3. VM_INSTANCE 计数器
        if self.vm_flags.contains(VmFlags::VM_INSTANCE) {
            crate::global::dec_vm_instance();
        }

        // 4. 重置所有字段
        self.vm_flags = VmFlags::empty();
        self.vm_endpoint = Endpoint::NONE;
        self.vm_boot = None;
        self.vm_acl = AclState::Uninitialized;
        self.vm_pt_initialized = false;
        self.vm_regions_avl_initialized = false;
        self.vm_region_top = VirBytes::new(0);
        self.vm_total = VirBytes::default();
        self.vm_total_max = VirBytes::default();
        self.vm_minor_page_fault = 0;
        self.vm_major_page_fault = 0;
    }
}
```

**影响**：`ExitingProc::reap()` 和 `ActiveProc::force_clear()` 也需要传入 `page_alloc`：

```rust
impl<'a> ExitingProc<'a> {
    pub(crate) unsafe fn reap(
        self,
        page_alloc: &mut VmPageAllocator,
    ) -> EmptySlot<'a> {
        unsafe { self.inner.clear(page_alloc); }
        EmptySlot::new(self.inner)
    }
}

impl<'a> ActiveProc<'a> {
    pub(crate) unsafe fn force_clear(
        self,
        page_alloc: &mut VmPageAllocator,
    ) -> EmptySlot<'a> {
        unsafe { self.inner.clear(page_alloc); }
        EmptySlot::new(self.inner)
    }
}
```

### 4.3 do_willexit — 设置 EXITING 标志

```rust
/// 处理 VM_WILLEXIT 请求
///
/// 对应 Minix3 的 do_willexit()。
/// PM 通知 VM 进程即将退出，VM 设置 EXITING 标志。
pub fn do_willexit(
    endpoint: Endpoint,
    table: &VmProcTable,
) -> Result<(), ExitError> {
    let slot = table.vm_isokendpt(endpoint)
        .map_err(|_| ExitError::InvalidEndpoint)?;

    let active = table.get_active(slot)
        .ok_or(ExitError::ProcessNotActive)?;

    // 转换为 ExitingProc（设置 EXITING 标志）
    let _exiting = active.mark_exiting();

    Ok(())
}
```

**问题**：`mark_exiting()` 消费了 `ActiveProc`，返回 `ExitingProc`。但 `ExitingProc` 持有 `&mut VmProc` 的可变引用，如果直接 drop，引用就释放了。这没问题——typestate 转换已经完成，进程的 `vm_flags` 已经包含 `EXITING`。

### 4.4 do_exit — 正式退出

```rust
/// 处理 VM_EXIT 请求
///
/// 对应 Minix3 的 do_exit()。
/// 释放进程的所有内存资源，清零进程结构体。
pub fn do_exit(
    endpoint: Endpoint,
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), ExitError> {
    let slot = table.vm_isokendpt(endpoint)
        .map_err(|_| ExitError::InvalidEndpoint)?;

    // 必须是 EXITING 状态
    let exiting = table.get_exiting(slot)
        .ok_or(ExitError::NotExiting)?;

    // 释放资源，返回 EmptySlot
    let _empty = unsafe { exiting.reap(page_alloc) };

    Ok(())
}
```

**typestate 保证**：`get_exiting()` 只返回 `ExitingProc`（IN_USE + EXITING），确保只有经过 willexit 的进程才能被 exit。这比 Minix3 的运行时检查 `if(!(vmp->vm_flags & VMF_EXITING))` 更安全——编译时就阻止了未 willexit 的进程被 exit。

### 4.5 do_procctl — 进程控制

```rust
/// 处理 VM_PROCCTL 请求
///
/// 对应 Minix3 的 do_procctl()。
pub fn do_procctl(
    msg: &ProcctlMessage,
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), ProcctlError> {
    let slot = table.vm_isokendpt(msg.who)
        .map_err(|_| ProcctlError::InvalidEndpoint)?;

    match msg.param {
        ProcctlParam::Clear => {
            // 权限检查：只有 RS 和 VFS 可以调用
            if !is_rs_or_vfs(msg.source) {
                return Err(ProcctlError::PermissionDenied);
            }

            let active = table.get_active(slot)
                .ok_or(ProcctlError::ProcessNotActive)?;

            // 释放内存资源，但不清零进程结构体
            let empty = unsafe { active.force_clear(page_alloc) };

            // 创建新页表并绑定（服务重启场景）
            let mut active = empty.activate_relaxed(endpoint);
            active.init_page_table()?;
            active.bind_page_table()?;

            Ok(())
        }

        ProcctlParam::HandleMem => {
            // VFS 专用
            if !is_vfs(msg.source) {
                return Err(ProcctlError::PermissionDenied);
            }
            // TODO: handle_memory_start
            Err(ProcctlError::NotImplemented)
        }
    }
}
```

### 4.6 PhysRegion Drop 的安全性

当前 `PhysRegion` 没有 `Drop` 实现。在退出流程中，`unlink_from_block()` 在 `RegionMap::free_all()` 中被显式调用，之后 `PhysRegion` 被 `Vec` 的 drop 释放。这是安全的，因为：

1. `unlink_from_block()` 已经将 `pr.ph = None` 和 `pr.next_ph_list = None`
2. 后续的 `Drop`（如果有）不会尝试再次释放引用

**但有一个隐患**：如果 `RegionMap::clear()` 被直接调用（不经过 `free_all()`），`PhysRegion` 的 drop 不会释放引用。解决方案：

```rust
impl Drop for PhysRegion {
    fn drop(&mut self) {
        // 防御性检查：如果还有 PhysBlock 引用，说明退出流程有 bug
        if self.ph.is_some() {
            // 在 debug 模式下 panic
            debug_assert!(
                self.ph.is_none(),
                "PhysRegion dropped while still linked to PhysBlock — \
                 use unlink_from_block() before dropping, or use RegionMap::free_all()"
            );
            // 在 release 模式下尝试释放（防御性）
            unsafe {
                self.unlink_from_block();
            }
        }
    }
}
```

### 4.7 完整退出流程（Rust）

```
PM → VM_WILLEXIT(endpoint):
  ├── table.vm_isokendpt(endpoint) → slot
  ├── table.get_active(slot) → ActiveProc
  └── active.mark_exiting() → ExitingProc (vm_flags |= EXITING)
       └── drop(ExitingProc) → 释放 &mut VmProc 引用

PM → VM_EXIT(endpoint):
  ├── table.vm_isokendpt(endpoint) → slot
  ├── table.get_exiting(slot) → ExitingProc
  └── exiting.reap(page_alloc):
       └── VmProc::clear(page_alloc):
            ├── RegionMap::free_all(page_alloc):
            │    └── 递归遍历所有 VirRegion:
            │         ├── 对每个 PhysRegion:
            │         │    ├── pr.unlink_from_block()
            │         │    │    ├── pb.refcount--
            │         │    │    ├── 从 pb.firstregion 链表移除
            │         │    │    └── 返回 should_free (refcount == 0)
            │         │    ├── if should_free:
            │         │    │    ├── memtype.ev_unreference(pr)
            │         │    │    └── if Ok(true): page_alloc.free_phys(phys, 1)
            │         │    └── (PhysRegion 随 Vec drop 释放)
            │         ├── memtype.ev_delete(region)
            │         └── (VirRegion 随 Box drop 释放)
            │
            ├── PageTable::destroy()
            │    └── 释放页表页资源
            │
            ├── if VM_INSTANCE: dec_vm_instance()
            │
            └── 重置所有字段:
                 ├── vm_flags = empty()    ← slot 变为空闲
                 ├── vm_endpoint = NONE
                 ├── vm_acl = Uninitialized
                 └── 统计归零
```

---

## 5. CoW 退出场景详解

### 5.1 场景 1：独立进程退出

```
进程 A 独占物理页 P (refcount = 1)

A 退出:
  ├── pr.unlink_from_block(): P.refcount 1 → 0
  ├── should_free = true
  ├── anon.ev_unreference(pr) → Ok(true)
  ├── page_alloc.free_phys(P.phys, 1)  ← 物理页归还
  └── P 被 SLABFREE (Rust: Box drop)
```

### 5.2 场景 2：CoW 共享页，一个进程退出

```
fork 后: 进程 A 和 B 共享物理页 P (refcount = 2)

B 退出:
  ├── B 的 pr.unlink_from_block(): P.refcount 2 → 1
  ├── should_free = false (refcount > 0)
  ├── 物理页 P 不释放
  └── A 的 pr 仍在 P.firstregion 链表中

A 继续运行:
  ├── A 的 pr.refcount = 1 → is_writable() = true
  ├── 页表可以设置为可写
  └── A 拥有 P 的独占使用权
```

### 5.3 场景 3：CoW 共享页，两个进程都退出

```
fork 后: 进程 A 和 B 共享物理页 P (refcount = 2)

B 先退出:
  ├── B 的 pr.unlink_from_block(): P.refcount 2 → 1
  ├── should_free = false
  └── 物理页 P 保留

A 再退出:
  ├── A 的 pr.unlink_from_block(): P.refcount 1 → 0
  ├── should_free = true
  ├── anon.ev_unreference(pr) → Ok(true)
  ├── page_alloc.free_phys(P.phys, 1)  ← 物理页归还
  └── P 被释放
```

### 5.4 场景 4：CoW 已执行，各自拥有独立页

```
fork 后共享 P (refcount = 2)
  → B 写入触发 CoW
  → B 获得 P' (refcount = 1), P.refcount 降为 1

B 退出:
  ├── B 的 pr(P').unlink_from_block(): P'.refcount 1 → 0
  ├── should_free = true
  └── page_alloc.free_phys(P'.phys, 1)  ← B 的私有页归还

A 退出:
  ├── A 的 pr(P).unlink_from_block(): P.refcount 1 → 0
  ├── should_free = true
  └── page_alloc.free_phys(P.phys, 1)  ← A 的页归还
```

### 5.5 场景 5：多进程共享（3+ 引用）

```
A fork B, B fork C → P.refcount = 3

C 退出:
  ├── P.refcount 3 → 2, should_free = false

B 退出:
  ├── P.refcount 2 → 1, should_free = false

A 退出:
  ├── P.refcount 1 → 0, should_free = true
  └── page_alloc.free_phys(P.phys, 1)  ← 最终释放
```

---

## 6. 实现清单

### 6.1 需要修改的现有代码

| 文件 | 修改内容 | 优先级 |
|------|---------|--------|
| `region/avl.rs` | 新增 `free_all()` 方法，处理 PhysRegion 引用释放 | 🔴 P0 |
| `vmproc/vmproc.rs` | `VmProc::clear()` 增加 `page_alloc` 参数 | 🔴 P0 |
| `vmproc/vmproc_handle.rs` | `reap()` 和 `force_clear()` 增加 `page_alloc` 参数 | 🔴 P0 |
| `region/phys_region.rs` | 可选：为 `PhysRegion` 添加防御性 `Drop` | 🟡 P1 |

### 6.2 需要新增的代码

| 文件 | 新增内容 | 优先级 |
|------|---------|--------|
| `exit.rs` (新) | `do_willexit()`, `do_exit()`, `do_procctl()` | 🔴 P0 |
| `exit.rs` | `ExitError`, `ProcctlError` 错误类型 | 🔴 P0 |
| `region/avl.rs` | `free_region_phys()` 辅助函数 | 🔴 P0 |

### 6.3 测试计划

| 测试 | 描述 |
|------|------|
| `test_willexit_then_exit` | 完整两阶段退出流程 |
| `test_exit_cow_shared_one_exits` | CoW 共享页，一个进程退出，物理页保留 |
| `test_exit_cow_shared_both_exit` | CoW 共享页，两个进程都退出，物理页释放 |
| `test_exit_cow_already_split` | CoW 已执行，各自退出独立页 |
| `test_exit_independent_process` | 独立进程退出，物理页直接释放 |
| `test_exit_refcount_3` | 三进程共享，逐个退出 |
| `test_procctl_clear` | VMPPARAM_CLEAR：释放内存但保留 slot |
| `test_exit_without_willexit` | 未 willexit 直接 exit 应失败 |
| `test_reap_and_reactivate` | 退出后 slot 可被新进程使用 |

---

## 7. 与 19-cow-exec-pagefault 的闭环

20 文档实现了 CoW 的"正向"流程（写入 → 分配新页 → 复制 → 更新引用），本文档实现了"反向"流程（退出 → 减少引用 → 条件释放物理页）。两者共同完成了引用计数系统的闭环：

```
引用计数生命周期:

fork:   add_ref()           → refcount: 0→1, 1→2
cow:    unlink + link        → refcount: 2→1 (旧页), 0→1 (新页)
exit:   unlink_from_block()  → refcount: 1→0 → free_phys()

每个 add_ref 都有对应的 release
每个 alloc_phys 都有对应的 free_phys
```

这就是 fork 叙事线的完整闭环：从创建（16-vm-fork）到运行时 CoW（19-cow-exec-pagefault）到退出释放（20-vm-exit），引用计数系统保证了物理内存的正确管理——不多不少，不早不晚。
