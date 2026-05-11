# 05-vm-allocpage: 页分配器

> **分类**: VM库
> **源码**: [pagetable.c](minix3/minix/servers/vm/pagetable.c)
> **说明**: VM 的页分配器，同时返回虚拟地址和物理地址

---

## 1. 概述

### 1.1 定位与依赖关系

`vm_allocpage` 是 VM 内存自举的关键卡点。在它之前，VM 只能使用内核预留的静态内存；在它之后，VM 可以动态分配物理页并映射到自己的虚拟地址空间。

```
04-physical-memory.md  ───→  05-vm-allocpage.md  ───→  06-pagetable-struct.md
  (物理内存池)                  (页分配器)                (页表结构)
  alloc_mem / free_mem         vm_allocpage              pt_t / pt_new
```

**为什么放在 06/07 之前？** 页表操作（`pt_new`）必须调用 `vm_allocpage` 来分配页目录和页表——这是硬依赖。反过来，`vm_allocpage` 对 `pt_init_done` 的依赖是软的（只是一个布尔标志），可以在本章先声明、在 07 再详述。

### 1.2 核心问题：VM 如何给自己分配内存

VM 是系统内存管理服务器，但它自己也是一个用户态进程。这产生了一个循环依赖：

```
VM 需要映射物理页 → 需要页表来建立映射 → 分配页表页又需要映射 → 谁来映射？
```

更具体地说，`vm_allocpage` 需要同时返回两个地址：

| 返回值 | 用途 | 消费者 |
|--------|------|--------|
| 虚拟地址（VA） | CPU 执行代码、读写数据 | VM 代码 |
| 物理地址（PA） | MMU 硬件做地址翻译 | 页表 entry、CR3 |

> **术语说明**：一旦 CPU 启动分页机制（CR0.PG=1），所有内存访问都通过 MMU 进行地址转换，CPU 只能使用虚拟地址（Virtual Address）。

物理地址（Physical Address）可以从物理内存分配器（`alloc_mem`）获取——纯 bitmap 操作，不涉及页表。但虚拟地址的获取需要经过 `vm_mappages`（详见 [07-pagetable-ops.md §2.3.3](07-pagetable-ops.md#2333-vm_mappages---分配虚拟地址并建立映射)），而 `vm_mappages` 在映射之前必须确保目标虚拟地址对应的页表已存在。如果页表不存在，就需要分配一个新的页表页——这就回到了 `vm_allocpage`，形成递归。

### 1.3 接口定义

```c
void *vm_allocpages(phys_bytes *phys, int reason, int pages);
void *vm_allocpage(phys_bytes *phys, int reason);
```

| 参数 | 含义 |
|------|------|
| `phys` | [出参] 物理地址，供硬件使用（如加载到 CR3） |
| `reason` | 用途分类：`VMP_SPARE`(0) / `VMP_PAGETABLE`(1) / `VMP_PAGEDIR`(2) / `VMP_SLAB`(3) |
| `pages` | 页数（`vm_allocpage` 固定为 1） |
| 返回值 | 虚拟地址，供 VM 代码访问 |

`vm_allocpage` 一次调用同时返回虚拟地址和物理地址。**递归的根源在于 VA 的获取**——建立页表映射可能需要分配新的页表页，而分配页表页又需要建立映射，形成循环。PA 的获取（从空闲页池取页）不会递归，VA 的获取才会。

---

## 2. Minix3 C 源码分析

### 2.1 两阶段分配机制

Minix3 用两个条件控制分配路径：

```c
// [pagetable.c:333](minix3/minix/servers/vm/pagetable.c#L333)

static int pt_init_done;  // [pagetable.c:328](minix3/minix/servers/vm/pagetable.c#L328) 页表系统初始化完成标志

void *vm_allocpages(phys_bytes *phys, int reason, int pages)
{
    phys_bytes newpage;
    static int level = 0;   // 递归深度计数器：1=正常调用，2=递归重入
    void *ret;
    u32_t mem_flags = 0;

    assert(reason >= 0 && reason < VMP_CATEGORIES);
    assert(pages > 0);

    level++;                 // 进入时 +1，退出时 -1，保证配对
    assert(level >= 1 && level <= 2);  // 最多递归一层

    // 阶段 1：初始化阶段 或 递归情况
    //   level > 1：说明是从 vm_mappages 递归回来的，不能再走正常路径
    //   !pt_init_done：VM 页表尚未初始化，无法调用 vm_mappages
    if ((level > 1) || !pt_init_done) {
        void *s;
        if (pages == 1) s = vm_getsparepage(phys);       // 从备用页池取单页
        else if (pages == 4) s = vm_getsparepagedir(phys); // 从备用页池取页目录（ARM）
        else panic("%d pages", pages);

        level--;
        if (!s) {
            util_stacktrace();
            printf("VM: warning: out of spare pages\n");
        }
        // 只有动态分配的页才计入 VM 自身用量统计：
        // - 静态页（BSS 备用页）：编译期预留，内核加载时已映射，不占用物理内存池，不计数
        // - 动态页：运行时从 alloc_mem 分配，消耗物理内存池，需要计数跟踪
        if (!is_staticaddr(s)) vm_self_pages++;
        return s;
    }

    // 阶段 2：正常运行阶段（level == 1 且 pt_init_done）
#if defined(__arm__)
    if (reason == VMP_PAGEDIR) {
        mem_flags |= PAF_ALIGN16K;  // ARM 页目录需要 16KB 对齐
    }
#endif

    // 步骤 1：分配物理页
    if ((newpage = alloc_mem(pages, mem_flags)) == NO_MEM) {
        level--;
        printf("VM: vm_allocpage: alloc_mem failed\n");
        return NULL;
    }

    *phys = CLICK2ABS(newpage);  // 页号 → 物理地址

    // 步骤 2：建立 VA 映射（⚠️ 这里可能递归：vm_mappages → pt_writemap → vm_allocpages）
    if (!(ret = vm_mappages(*phys, pages))) {
        level--;
        printf("VM: vm_allocpage: vm_mappages failed\n");
        return NULL;
    }

    level--;
    vm_self_pages++;  // 计入 VM 自身占用的页数
    return ret;
}
```

两条路径：

| 条件 | 路径 | 说明 |
|------|------|------|
| `!pt_init_done` | `vm_getsparepage()` | 初始化阶段，页表系统未就绪 |
| `level > 1` | `vm_getsparepage()` | 递归重入，避免 `vm_mappages` → `pt_ptalloc` → `vm_allocpage` 无限递归 |
| `pt_init_done && level == 1` | `alloc_mem()` + `vm_mappages()` | 正常运行 |

### 2.2 阶段 1：静态备用页（BSS）

初始化阶段，页表系统尚未就绪，无法调用 `vm_mappages()` 做动态映射。Minix3 的解法是编译时在 BSS 段预留静态备用页，内核加载 VM 时已将这些页映射到 VM 的地址空间。

**备用页数量**（条件编译）：

| 构建配置 | `SPAREPAGES` | `STATIC_SPAREPAGES` |
|---------|-------------|---------------------|
| SANITYCHECKS | 200 | 190 |
| ARM 生产构建 | 150 | 140 |
| x86 生产构建 | 20 | 15 |

**字段说明**：
- `SPAREPAGES`：备用页池**总容量**（静态页 + 动态填充槽位）
- `STATIC_SPAREPAGES`：BSS 段**静态预留**页数，内核加载时已映射
- **差额**（10 页）：运行时由 `alloc_cycle()` 动态分配填充，用于平滑过渡——先用光静态页，再用动态页重新填充备用池

> 文档中分析以 SANITYCHECKS 值（190 静态页）为例，生产构建（x86 仅 15 页）的递归保护机制相同，但池容量更小。

```c
static char static_sparepages[VM_PAGE_SIZE * STATIC_SPAREPAGES]
    __aligned(VM_PAGE_SIZE);
```

初始化流程（`pt_init()`）：

```c
// [pagetable.c:1088](minix3/minix/servers/vm/pagetable.c#L1088) — 简化，省略中间步骤

void pt_init(void)
{
    // ... 变量声明 ...

    // 步骤 1：初始化备用页池
    // static_sparepages 是 BSS 段数组，编译期已分配 VA，VM 进程可直接访问
    sparepages_mem = (vir_bytes) static_sparepages;
    assert(!(sparepages_mem % VM_PAGE_SIZE));

    // 创建备用页队列，容量 SPAREPAGES 页，每槽 1 页
    // mappedin=1：队列将来动态补充页时（reservedqueue_addslot），
    //             需要调用 vm_mappages 建立 VA 映射；
    //             同时要求通过 reservedqueue_add 加入的静态页也必须提供有效 VA
    spare_pagequeue = reservedqueue_new(SPAREPAGES, 1, 1, 0);

    // 将 STATIC_SPAREPAGES 个静态页加入队列
    for (s = 0; s < STATIC_SPAREPAGES; s++) {
        // v = 第 s 个静态页的虚拟地址（BSS 段内偏移）
        void *v = (void *)(sparepages_mem + s * VM_PAGE_SIZE);
        phys_bytes ph;

        // sys_umap：内核调用，将 VM 进程的虚拟地址 v 翻译为物理地址 ph
        //   SELF  = VM 自身进程端点
        //   VM_D  = LOCAL_VM_SEG | VIR_ADDR，表示查询 VM 自身虚拟地址空间
        //   第 4 个参数传整个数组大小，内核校验整个区间是否有效映射
        // 为什么需要 sys_umap？因为 reservedqueue 需要同时记录 VA 和 PA：
        //   - VA 供 VM 代码访问页内容（CPU 需要 VA）
        //   - PA 供页表 entry 使用（MMU 需要 PA）
        // BSS 段在内核加载时已建立映射，但 VM 进程不知道物理地址，必须向内核查询
        if ((r = sys_umap(SELF, VM_D, (vir_bytes)v,
                VM_PAGE_SIZE * SPAREPAGES, &ph)) != OK)
            panic("pt_init: sys_umap failed: %d", r);

        // 将 (VA, PA) 对加入备用页队列，后续 vm_getsparepage 直接取出使用
        reservedqueue_add(spare_pagequeue, v, ph);
    }

    // ... 中间省略：页目录分配、内核映射建立等（约 150 行）...

    // 步骤 N：切换标志（[pagetable.c:1311](minix3/minix/servers/vm/pagetable.c#L1311)）
    pt_init_done = 1;
}
```

`pt_init_done = 1` 是分水岭。在此之前所有 `vm_allocpage` 调用走备用页，在此之后走动态分配。

### 2.3 阶段 2：动态分配

条件：`pt_init_done == 1` 且 `level == 1`（非递归）。

```
vm_allocpage()
    ├── alloc_mem(pages, flags)    // 从物理内存池分配（纯 bitmap，不递归）
    └── vm_mappages(*phys, pages)   // 映射到 VM 虚拟地址空间
```

### 2.4 递归链分析

`vm_allocpage` 的递归不是来自 `alloc_mem`（纯 bitmap 操作），而是来自 `vm_mappages`（详见 [07-pagetable-ops.md §2.3.3](07-pagetable-ops.md#2333-vm_mappages---分配虚拟地址并建立映射)）。完整递归链（[pagetable.c:333-393](minix3/minix/servers/vm/pagetable.c#L333-L393) → [pagetable.c:494-540](minix3/minix/servers/vm/pagetable.c#L494-L540)）：

```
vm_allocpages()                          [level = 1]
  │
  ├── alloc_mem(pages, flags)            [bitmap 扫描，不递归]
  │
  └── vm_mappages(phys, pages)
        │
        ├── findhole(pages)              [在 VM 虚拟地址空间中找空洞]
        │
        └── pt_writemap()
              │
              └── pt_ptalloc_in_range()
                    │
                    └── pt_ptalloc(pde)  [pde 由目标虚拟地址决定]
                          │
                          └── vm_allocpage(VMP_PAGETABLE)
                                │         [level = 2，递归！]
                                │
                                └── level > 1 → vm_getsparepage()
```

**关键代码** — `pt_ptalloc`（[pagetable.c:494-523](minix3/minix/servers/vm/pagetable.c#L494-L523)）：

```c
static int pt_ptalloc(pt_t *pt, int pde, u32_t flags)
{
    phys_bytes pt_phys;
    u32_t *p;

    assert(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT));
    assert(!pt->pt_pt[pde]);

    /* Get storage for the page table. The allocation call may in fact
     * recursively create the directory entry as a side effect. In that
     * case, we free the newly allocated page and do nothing else.
     */
    if (!(p = vm_allocpage(&pt_phys, VMP_PAGETABLE)))
        return ENOMEM;
    if (pt->pt_pt[pde]) {
        vm_freepages((vir_bytes) p, 1);
        assert(pt->pt_pt[pde]);
        return OK;
    }
    pt->pt_pt[pde] = p;
    // ... 设置 pt->pt_dir[pde] ...
}
```

**注释中提到的 "side effect"**：`vm_allocpage`（level=1）调用 `vm_mappages` 时，`vm_mappages` → `pt_writemap` → `pt_ptalloc_in_range` 可能递归进入另一个 `pt_ptalloc(pde)`。

关键点在于 `vm_mappages` 硬编码操作 VM 自己的页表（`&vmprocess->vm_pt`），所以内层 `pt_ptalloc` 永远操作 `vmprocess->vm_pt`。如果外层 `pt_ptalloc` 操作的也是 `vmprocess->vm_pt`（如 `pt_init` 中），那么外层和内层操作同一个页表结构。此时 `findhole` 返回的 VA 可能落在外层正在处理的 PDE 范围内——内层 `pt_ptalloc` 先于外层设置了 `pt->pt_pt[pde]` 和 `pt->pt_dir[pde]`。当递归返回、外层 `pt_ptalloc` 继续执行时，发现 `pt->pt_pt[pde]` 已经非空——说明内层递归已经替它完成了页表分配，于是释放自己刚拿到的页，直接返回 OK。

**递归的根源**：页表页需要两样东西——物理地址（写入 PDE 给 MMU）和虚拟地址（VM 往页表里写 PTE）。物理地址可以从 `alloc_mem` 拿（不递归），但虚拟地址如果走 `find_hole + vm_mappages`，就回到了 `vm_allocpage`。

**Minix3 的解法**：用备用页池（`spare_pagequeue`）承接递归。备用页的虚拟地址是已知的（BSS 段），不需要走 `find_hole`。

关键证据在 `pt_init()` 的结尾（[pagetable.c:1088](minix3/minix/servers/vm/pagetable.c#L1088) 定义，[1311-1327](minix3/minix/servers/vm/pagetable.c#L1311-L1327) 关键逻辑）：

```c
pt_init_done = 1;

/* We don't want to keep using the bootstrap statically allocated spare
 * pages though. So we re-do part of the initialization now with purely
 * dynamically allocated memory. */

alloc_cycle();                          /* Make sure allocating works */
while(vm_getsparepage(&phys)) ;         /* Use up all static pages */
alloc_cycle();                          /* Refill spares with dynamic */
```

`pt_init_done = 1` 标志着 VM 页表初始化完成，可以走正常分配路径。但 Minix3 并不就此停止——它还要替换掉初始化阶段使用的静态备用页。源码注释（[pagetable.c:1316-1318](minix3/minix/servers/vm/pagetable.c#L1316-L1318)）解释了原因：

```c
/* We don't want to keep using the bootstrap statically allocated spare
 * pages though, as the physical addresses will change on liveupdate. So we
 * re-do part of the initialization now with purely dynamically allocated
 * memory. */
```

替换静态页的根本原因是 **liveupdate**——静态页的物理地址在 liveupdate 时会变化，而动态分配的页不受影响。替换过程分三步：

1. `alloc_cycle()`：确保动态分配可用
2. `while(vm_getsparepage(&phys))`：用光所有静态备用页
3. `alloc_cycle()`：用动态分配的页重新填充备用池

随后（[pagetable.c:1338-1341](minix3/minix/servers/vm/pagetable.c#L1338-L1341)），Minix3 还用动态分配重建了整个 VM 页表：

```c
/* Recreate VM page table with dynamic-only allocations */
memset(&newpt_dyn, 0, sizeof(newpt_dyn));
pt_new(&newpt_dyn);
pt_copy(&newpt_dyn, newpt);
memcpy(newpt, &newpt_dyn, sizeof(*newpt));
```

至此，VM 彻底脱离对静态内存的依赖，所有页表结构都使用动态分配的内存。

### 2.5 释放机制

```c
// [pagetable.c:235](minix3/minix/servers/vm/pagetable.c#L235)
void vm_freepages(vir_bytes vir, int pages)
{
	assert(!(vir % VM_PAGE_SIZE));

	if(is_staticaddr(vir)) {
		printf("VM: not freeing static page\n");
		return;
	}

	if(pt_writemap(vmprocess, &vmprocess->vm_pt, vir,
		MAP_NONE, pages*VM_PAGE_SIZE, 0,
		WMF_OVERWRITE | WMF_FREE) != OK)
		panic("vm_freepages: pt_writemap failed");

	vm_self_pages--;

#if SANITYCHECKS
	if((sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK) {
		panic("VMCTL_FLUSHTLB failed");
	}
#endif
}
```

**步骤说明**：
1. **静态地址检查**：`is_staticaddr(vir)` 判断是否为 BSS 段静态备用页，静态页不释放（由系统回收）
2. **解映射并释放**：`pt_writemap(..., WMF_OVERWRITE | WMF_FREE)` 一次性完成取消映射和物理页释放。`WMF_FREE` 标志使 `pt_writemap` 内部调用 `free_mem`，无需单独调用
3. **计数递减**：`vm_self_pages--` 跟踪 VM 自身分配的页数
4. **TLB 刷新**：仅在 `SANITYCHECKS` 构建时刷新 TLB，确保访问已释放页会触发页错误（便于调试）
    > 正常构建下页表 entry 已清除（PRESENT=0），TLB 的 stale entry 会随运行被自然替换，不刷新是性能与风险的权衡。x86 从 486 起就支持 `invlpg` 指令刷新单条 TLB entry，但 Minix3 使用 `reload_cr3()`（重新加载 CR3 刷新整个 TLB），因为 `pt_writemap` 可能修改多条 entry，批量刷新更简单。

---

## 3. Rust 设计决策

本章围绕两个核心问题展开：**VA 从哪里来**和**递归如何消除**。这两个问题本质上是同一个问题的两面——递归的根源就是 VA 的获取需要页表映射，而页表映射本身又需要 VA。三阶段演进从"递归兜底"到"结构性消除"到"前提消失"，每一步都在重新回答"VA 从哪里来"这个问题。

### 3.1 VA 来源的三阶段演进

#### 3.1.1 spare_pagequeue：递归兜底（Minix3 原方案）

Minix3 的 `spare_pagequeue` 是递归兜底方案。当 `vm_allocpage` 递归重入（`level > 1`）时，不走 `vm_mappages` 路径，而是从备用页池取页。备用页的 VA 来自 BSS 段静态分配——编译期已确定，不需要运行时分配。

```
VA 来源: BSS 段静态预留（编译期确定）
递归处理: 递归发生时用备用页兜底
本质: "递归还是会发生，只是有缓冲"
```

**问题**：备用页池是固定大小的。x86 生产构建仅 15 页，如果递归频繁发生（每次 `vm_mappages` 落入新的 2MB 区域），池会耗尽。这个方案没有消除递归，只是提供了缓冲。

> 详细分析见 §2.2–§2.4。

#### 3.1.2 PtRegion：结构性消除递归

**核心思路**：在 VM 虚拟地址空间中预留一段**连续的、预先映射好的区域**，专供页表页使用。页表页需要 VA 时，从这段区域按序切分（bump allocator），不需要 `find_hole`，也不需要 `vm_mappages`。

**关键前提**：这段区域本身必须在 VM 启动时就映射好——由 kernel 在加载 VM 时建立初始页表映射。

**PtRegion 的结构**（假设 x86-64 四级页表）：

```
VM 虚拟地址空间（x86-64）:
  0x0000_7F00_0000_0000  ┬── PtRegion 专用区域 (初始 2MB)
                         │   ├── PDPT page (4KB)  ← 页目录指针表
                         │   ├── PD page   (4KB)  ← 页目录
                         │   ├── PT[0]     (4KB)  ← 页表，512 个 PTE slot
                         │   ├── slot 0: 已用 (映射 PDPT)
                         │   ├── slot 1: 已用 (映射 PD)
                         │   ├── slot 2: 已用 (映射 PT[0])
                         │   ├── slot 3: 空闲 ← 下一个可分配的页表页
                         │   ├── ...
                         │   ├── slot 511: 空闲
  0x0000_7F00_0020_0000  ┴── 区域结束
```

**术语说明**：
- **PDPT**（Page Directory Pointer Table）：x86-64 四级页表的第 3 级，每个 entry 指向一个 PD
- **PD**（Page Directory）：第 2 级，每个 entry 指向一个 PT
- **PT**（Page Table）：第 1 级，每个 entry（PTE）指向一个 4KB 物理页
- **slot**：PT 中的 PTE 索引（0-511），每个 slot 对应一个 4KB 虚拟页
- **current_pt**：当前活跃的 PT 页，提供 slot 供 bump allocator 分配

**容量分析**：1 个 PT 页有 512 个 slot，每个 slot 映射 4KB。当 512 个 slot 用完时，需要分配一个新的 PT 页。1 个 PD 页可以指向 512 个 PT 页，覆盖 512 × 2MB = 1GB 的虚拟地址空间。VM 自身页表需求远小于 1GB，所以容量充足。

**页表页的分配过程**（从 PtRegion 取一个 slot 给新的页表页）：

```
1. 从物理内存分配器取一页: pt_phys = alloc_phys(1)
   ← 纯 bitmap 操作，不涉及页表，不递归

2. 从 bump allocator 取一个空闲 slot: slot_idx = next_free_slot++
   ← 纯整数递增，不递归

3. 计算该 slot 对应的虚拟地址:
   pt_virt = PTREGION_BASE + slot_idx * 4KB
   ← 简单算术，不递归

4. 写 PTE 建立映射:
   current_pt[slot_idx] = pt_phys | PRESENT | RW
   ← current_pt 本身已映射在 PtRegion 中，直接写内存，不递归

5. 清零新页表页:
   memset(pt_virt, 0, 4KB)
   ← 通过步骤 4 建立的映射访问，不递归
```

**全部操作都不需要 `find_hole` 或 `vm_mappages`，递归从结构上被消除。**

**PT 页用完时的扩展**（current_pt 的 512 slot 耗尽）：

```
1. 按上述过程分配一个新的 PT 页: new_pt_phys, new_pt_virt
2. 在 PD 中写入 PDE，让 PD 指向 new_pt_phys:
   pd[pd_idx] = new_pt_phys | PRESENT | RW
   ← PD 本身也映射在 PtRegion 中，直接写内存，不递归
3. 更新 current_pt = new_pt_virt
4. next_free_slot = 0（新 PT 页从 slot 0 开始分配）
```

**递归检查清单**：

| 操作 | 需要什么 | 从哪来 | 触发 vm_mappages？ |
|------|----------|--------|-------------------|
| `alloc_phys` | 物理页 | PhysAllocator bitmap | ❌ |
| `alloc_pt_page` | 虚拟地址 | bump allocator（整数递增） | ❌ |
| 写 PTE | 写已映射页表 | `current_pt`（已映射） | ❌ |
| 写 PDE | 写已映射页表 | `pd`（已映射） | ❌ |

**全部 ❌。递归从结构上被消除。**

**设计反思：PtRegion 的本质**

PtRegion 的核心职责是 `alloc_pt_page() → VirBytes`：给物理页分配一个 stable VA。这和 direct map 的 `vm_phys_to_virt(phys) → VirBytes` 做的是同一件事——**phys → stable VA**。区别只是范围：

| | PtRegion | Direct Map |
|---|---------|------------|
| VA 计算方式 | `PTREGION_BASE + slot_idx * 4KB` | `DIRECT_MAP_BASE + phys` |
| 适用范围 | 仅页表页 | 所有物理页 |
| 自举复杂度 | 需要 kernel 预先映射 PtRegion 区域 | 需要 kernel 预先映射整个 direct map |

PtRegion 是在 VM 里"重新发明了一套 mini direct-map"——用 bump allocator 替代简单加法，用有限区域替代全局映射。

#### 3.1.3 Direct Map：VA 分配步骤消失（最终方案）

Direct Map 使"VA 分配"这个步骤本身消失了。物理页天然拥有 stable VA：`vm_phys_to_virt(phys) = DIRECT_MAP_BASE + phys`。

```
VA 来源: vm_phys_to_virt(phys) = DIRECT_MAP_BASE + phys（简单加法）
递归处理: 递归的前提条件消失——不存在"需要分配 VA"这个步骤
本质: "递归的根源消失了"
```

**核心变化**：

```rust
// PtRegion 方案
fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
    let phys = self.phys_alloc.alloc_mem(1, ...)?;
    let virt = self.pt_region.alloc_pt_page()?;  // 需要 bump allocator
    Some((virt, phys))
}

// Direct Map 方案（当前实现）
fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
    let phys = self.phys_alloc.alloc_mem(1, ...)?;
    let virt = vm_phys_to_virt(phys);  // 简单加法
    Some((virt, phys))
}
```

**递归链消失**：回顾 §1.2 的递归链 `vm_allocpage → vm_mappages → pt_ptalloc_in_range → pt_ptalloc → vm_allocpage`，在 Direct Map 下这条链根本不存在——`vm_allocpage` 不调用 `vm_mappages`，因为映射已经存在于 direct map 中。

**页表页不是特殊对象**：Direct map 出现前，隐含的模型是"普通物理页 ≠ 页表页"——页表页需要 PtRegion 这样的特殊 VA 管理。Direct map 出现后，"所有 physical pages are equally accessible"——页表页只是物理页的一种用途，不需要特殊 VA 管理。概念从"页表页需要独立 VA 分配路径"进化为"所有物理页统一通过 `vm_phys_to_virt()` 访问"。

**Rust 所有权与物理内存 Aliasing**：Direct Map 使每个物理页在 VM 地址空间中都有一个稳定的 VA，但同一个物理页可能同时被映射到进程的地址空间——两个 VA 指向同一个 PA，形成 aliasing。

```
进程页表:  VA 0x4000_0000 ──→ PA 0x2000 ←── 进程通过这个 VA 读写自己的数据
VM Direct Map: VA 0x8000_2000 ──→ PA 0x2000 ←── VM 通过 vm_phys_to_virt(0x2000) 访问同一页
```

这意味着 VM 通过 Direct Map 拿到的 `&mut [u8]` 并非真正独占——进程可能随时通过自己的 VA 修改同一物理页。Rust 编译器基于 `&mut` 的独占承诺做优化（如缓存值到寄存器、省略重复读取），在 aliasing 下这些优化会产出错误结果。

**解法**：不向编译器承诺独占访问，改用 `read_volatile` / `write_volatile` 强制每次都真正读写内存，或封装 `PhysPtr` 统一处理。这不是设计缺陷，而是 Rust 安全保证在 OS 内核开发中需要显式处理的固有约束。

### 3.2 递归消除三阶段递进

| 方案 | 页表页 VA 来源 | 普通页 VA 来源 | 页表页处理 | 递归处理 | 类比 |
|------|---------------|---------------|-----------|---------|------|
| Minix3 | `spare_pagequeue` 取静态页 | `find_hole + vm_mappages` | 需要特殊处理（静态备用页） | 备用页池兜底 | 治标——递归还是会发生，只是有缓冲 |
| PtRegion | `PTREGION_BASE + slot_idx * 4KB` | `find_hole + vm_mappages` | 需要特殊 VA 管理（PtRegion） | 结构性消除 | 治本——让递归不可能发生 |
| Direct Map | `DIRECT_MAP_BASE + phys` | `DIRECT_MAP_BASE + phys` | 与普通物理页无区别 | 前提消失 | 不需要药——递归的根源消失了 |

这是从"治标"到"治本"到"不需要药"的跃迁。spare_pagequeue 是运行时保护（递归发生但有缓冲），PtRegion 是设计层面消除（递归从结构上不可能），Direct Map 是概念层面消除（"需要分配 VA"这个前提本身不存在了）。

这不是"换了一种 VA 分配方式"，而是 **"VA 分配这个步骤本身消失了"**。

### 3.3 init 阶段管理

Minix3 用 `pt_init_done` 全局标志区分初始化阶段和正常运行阶段。在 Direct Map 方案下，VM 从第一条指令起就能通过 `vm_phys_to_virt()` 访问物理内存——不存在"页表系统未就绪"的阶段，因此不需要阶段标志。

**不需要 ReservedRegion**：在 Minix3 中，BSS 静态备用页同时提供 VA 和 PA，用于 init 阶段的物理页预留。在 Direct Map 方案下，VA 由 `vm_phys_to_virt()` 统一提供，物理页预留由 `PhysAllocator.reserve_pages()` 完成——ReservedRegion 的两个职责都被更简单的机制替代，因此不再需要。

### 3.4 从拆分到统一

在 Direct Map 出现前，VM 需要区分"分配物理页"和"分配虚拟地址"两个步骤：
- 物理页来自物理内存分配器（`alloc_phys`）
- 虚拟地址需要 `find_hole` 搜索空闲区域，或通过 PtRegion 等专用机制分配

Direct Map 使这两个步骤统一为一步：`vm_phys_to_virt(phys)`。物理页一旦分配，天然拥有 stable VA。

```rust
fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
    let phys = self.alloc_phys(1, PageAllocFlags::empty()).ok()?;
    let virt = vm_phys_to_virt(phys);  // 不再需要单独的虚拟地址分配
    Some((virt, phys))
}
```

**关键简化**：`alloc_virt` 这个步骤消失了。不是被优化了，而是概念上不再需要——VA 不再是需要"分配"的资源，而是物理地址的简单函数。

---

## 4. 实现详解

### 4.1 VmPageAllocator

Direct Map 方案下，`VmPageAllocator` 极其简洁：

```rust
pub(crate) struct VmPageAllocator {
    phys_alloc: PhysAlloc,
}
```

**完整实现**（[alloc_page.rs](os/servers/vm/src/alloc_page.rs)）：

```rust
impl VmPageAllocator {
    pub(crate) fn new(phys_alloc: PhysAlloc) -> Self {
        Self { phys_alloc }
    }

    pub(crate) fn alloc_phys(
        &mut self, clicks: usize, flags: PageAllocFlags,
    ) -> Result<PhysBytes, AllocError> {
        self.phys_alloc.alloc_mem(clicks, flags)
    }

    pub(crate) fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
        let phys = self.alloc_phys(1, PageAllocFlags::empty()).ok()?;
        let virt = vm_phys_to_virt(phys);
        Some((virt, phys))
    }

    pub(crate) fn free_page(&mut self, phys: PhysBytes) {
        self.phys_alloc.free_mem(phys, 1);
    }

    pub(crate) fn total_pages(&self) -> usize {
        self.phys_alloc.total_count()
    }
}
```

**与 Minix3 的对比**：

| 维度 | Minix3 `vm_allocpages` | minix-rs `VmPageAllocator` |
|------|----------------------|---------------------------|
| 结构体字段 | 无（全局函数 + 静态变量） | `phys_alloc: PhysAlloc` |
| 阶段区分 | `pt_init_done` + `level` | 无（Direct Map 消除阶段） |
| 递归保护 | `level` 计数器 + 备用页池 | 不需要（递归不可能发生） |
| VA 获取 | `vm_mappages`（可能递归） | `vm_phys_to_virt(phys)`（简单加法） |
| 代码行数 | ~60 行 | ~15 行 |

### 4.2 Direct Map

```rust
pub(crate) const VM_DIRECT_MAP_BASE: u64 = 0x0000_0000_8000_0000;
pub(crate) const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

pub(crate) fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes {
    VirBytes(phys.as_u64() + VM_DIRECT_MAP_BASE)
}

pub(crate) fn virt_to_phys(virt: VirBytes) -> PhysBytes {
    if virt.0 >= KERNEL_DIRECT_MAP_BASE {
        PhysBytes::new(virt.0 - KERNEL_DIRECT_MAP_BASE)
    } else {
        PhysBytes::new(virt.0 - VM_DIRECT_MAP_BASE)
    }
}
```

**地址空间布局**：

```
0x0000_0000_0000_0000  ─── 用户空间低地址
        ...
0x0000_0000_8000_0000  ─── VM Direct Map 基址（1GB 物理内存映射）
        ...
0x0000_7FFF_FFFF_FFFF  ─── 用户空间结束
0xFFFF_8000_0000_0000  ─── Kernel Direct Map 基址
        ...
0xFFFF_FFFF_FFFF_FFFF  ─── 内核空间结束
```

### 4.4 初始化时序

```
T0: Kernel 启动 VM 进程
    ├── 创建初始页表（映射 .text, .rodata, .data, .bss）
    ├── 建立 Direct Map（1GB 物理内存映射）
    ├── 映射 reserved_region 到 VM 地址空间
    └── 传递 boot_info（含 reserved_region 描述）

T1: main() → init_vm()
    │
    ├── T2: PhysAllocator::init(mem_chunks)
    │       → 初始化物理页 bitmap
    │       → 此时 alloc_mem() 可用
    │
    ├── T3: VmPageAllocator::new(phys_alloc)
    │       → 此时 alloc_page() 可用
    │       → alloc_page() = alloc_phys() + vm_phys_to_virt()
    │
    ├── T4: pt_init()
    │       → 创建页表系统
    │
    ├── T5: relocate_to_heap()  [独立文档详述]
    │       → 搬迁预留区域数据到堆
    │       → 释放预留区域物理页回 PhysAllocator
    │
    └── init_vm() 返回
            → VM 堆完全可用

T6: 主循环开始
    → VM 正常运行
```

**与 Minix3 时序的对比**：Minix3 需要 `pt_init_done` 标志和 `alloc_cycle()` 来切换阶段；Direct Map 方案下，`alloc_page()` 从 T3 起就使用同一条代码路径，不需要阶段切换。

---

## 5. 测试

### 5.1 测试策略概述

| 维度 | 测试重点 | 关键场景 |
|------|----------|----------|
| VmPageAllocator | alloc_page 返回 VA = vm_phys_to_virt(phys) | 分配、释放、地址正确性 |
| Direct Map | phys ↔ virt 转换 | vm_phys_to_virt、virt_to_phys、roundtrip |

### 5.2 VmPageAllocator 测试

**alloc_page 地址正确性**：验证 `alloc_page()` 返回的 VA 等于 `vm_phys_to_virt(phys)`。

```rust
#[test]
fn test_alloc_page() {
    let phys_alloc = make_test_phys_alloc(256);
    let mut alloc = VmPageAllocator::new(phys_alloc);

    let (v1, p1) = alloc.alloc_page().unwrap();
    assert_eq!(v1.0 - p1.as_u64(), mock_map::offset());

    let (v2, p2) = alloc.alloc_page().unwrap();
    assert_ne!(p1.as_u64(), p2.as_u64());
    assert_eq!(v2.0 - p2.as_u64(), mock_map::offset());
}
```

**alloc_phys 与 free_page**：验证物理页分配和释放的正确性。

```rust
#[test]
fn test_alloc_phys_and_free() {
    let phys_alloc = make_test_phys_alloc(256);
    let mut alloc = VmPageAllocator::new(phys_alloc);

    let p1 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
    let p2 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
    assert_ne!(p1, p2);

    alloc.free_page(p1);
    let p3 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
    assert_ne!(p3, p2);
}
```

### 5.3 Direct Map 测试

**phys → virt → phys roundtrip**：验证地址转换的正确性。

```rust
#[test]
fn test_virt_to_phys_roundtrip() {
    let phys = PhysBytes::new(0x2000);
    assert_eq!(virt_to_phys(vm_phys_to_virt(phys)), phys);
    assert_eq!(virt_to_phys(kernel_phys_to_virt(phys)), phys);
}
```

**is_direct_map_virt**：验证 direct map 地址范围判断。

```rust
#[test]
fn test_is_direct_map_virt() {
    assert!(is_direct_map_virt(VirBytes(VM_DIRECT_MAP_BASE)));
    assert!(is_direct_map_virt(VirBytes(KERNEL_DIRECT_MAP_BASE)));
    assert!(!is_direct_map_virt(VirBytes(0x7000_0000)));
}
```

---

## 6. 执行模型与线程安全

### 6.1 单线程假设

VM 进程采用**单线程事件循环**模型（参见项目级 review 规范）。所有内存管理操作都在主线程中顺序执行，不存在并发访问。

**影响**：
- `VmPageAllocator` 及其内部组件不需要 `Sync` 或 `Send` trait
- 不需要互斥锁、原子操作或内存屏障
- Minix3 中用于检测递归的 `level` 变量在 Rust 版本中不再需要——Direct Map 从结构上消除了递归

### 6.2 与 Minix3 的对比

| 维度 | Minix3 | minix-rs |
|------|--------|----------|
| 执行模型 | 单线程事件循环 | 单线程事件循环（保持） |
| 递归检测 | 运行时 `level` 计数器 | 不需要（Direct Map 消除递归） |
| 并发保护 | 无（单线程） | 无（单线程，保持） |
| 可重入性 | 通过 `level` 限制 | 不需要（递归不可能发生） |

---

## 7. 参见

- [04-physical-memory.md](04-physical-memory.md) - 物理页分配器（`alloc_mem` / `free_mem`）
- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构（`pt_t`）
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作（`pt_init` / `pt_new`）
- [08-slab-allocator.md](08-slab-allocator.md) - Slab 分配器（全局 allocator）
- [09-vm-relocation.md](09-vm-relocation.md) - 数据搬迁（预留区域 → 堆）

---

*分类: VM库 | 使用范围: 仅 VM 内部*
