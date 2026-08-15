# 06-page-allocator: VM 自身页分配——双地址问题的 Direct Map 解

> **分类**: 阶段 3 — 页与页表（页分配锚点）
> **源码**: `minix3/minix/servers/vm/pagetable.c:34-489`（`vm_*` 自用页函数族 + `findhole`）；`minix3/minix/servers/vm/alloc.c:56-237`（`reservedqueue_*` + `alloc_cycle` + `missing_spares`）；`minix3/minix/servers/vm/main.c:112-119,745-746`（主循环调用点）；`minix3/minix/servers/vm/pagetable.c:1088-1161,1311-1345`（`pt_init` 备用页建立与替换）
> **Rust 模块**: `os/servers/vm/src/alloc_page.rs`（`VmPageAllocator` + `vm_pt_alloc`）+ `os/servers/vm/src/global.rs:356-380`（`page_alloc_mut`）+ `os/servers/vm/src/vm_server.rs:54-70,102-107,416-428`（压力计数 + 注册 + 主循环钩子）+ `os/servers/vm/src/direct_map.rs`（A-1 基础）+ `os/servers/vm/src/heap_arena.rs:88-177`（消费方）+ `os/servers/vm/src/pagetable/vm_self_map.rs:127-135`（`vm_self_query`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/05-physical-memory.md`（物理分配器 `alloc_mem/free_mem`）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`（`init_vm` 调用点）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md`（Direct Map `[ARCH: A-1]` 的页表侧承接）
> **说明**: VM 自身页分配语义模块：**`vm_allocpage/vm_allocpages/vm_mappages/vm_freepages`（自用页分配/映射/释放）、`vm_pagelock/vm_addrok`（写保护/校验）、备用页池消费方（`vm_getsparepage` 路径、`alloc_cycle` 主循环接线、`missing_spares` 记账）、`get_vm_self_pages`（自用页计数）**。**不覆盖**：物理分配器本体（05）、`pt_t` 结构与 Direct Map 双视图（07）、pt 操作 `pt_writemap` 等（08）、slab/HeapArena 消费流程（09）、页缓存回收 `cache_freepages`（24）。

---

## 1. 概念：VM 如何给自己分配页

### 1.0 章节引言

本文档回答 01 文档启动链上的一个问题：**VM 作为内存管理服务器，如何为自己分配可用的页**。它在 `init_vm()` 中的位置是：

```
init_vm() ──► mem_init(mem_chunks)（main.c:471，分配器接管）
           ──► init_proc(VM_PROC_NR) + pt_init()（main.c:474-475，VM 自身槽 + 页表）
           │      └─ pt_init 备用页建立（pagetable.c:1151-1161）
           ──► __minix_init()（main.c:480，堆可用分界线）
           │      └─ 首次堆分配 → vm_allocpage（slaballoc.c:166）
           ──► 主循环（main.c:112-119）
                  └─ if(missing_spares > 0) alloc_cycle()
```

05 文档把物理内存变成了"按页借出、收回的账本"（`alloc_mem/free_mem`）；本文档把"任意大小的连续块"升级为 **"VM 自用页"**——一次调用同时拿到**虚拟地址（VA，CPU 执行用）和物理地址（PA，MMU/页表项用）**。页表结构文档（07/08）消费本文档的页分配结果来建立映射；slab/堆文档（09）消费它来获得 VM 自己的内存。

### 1.1 双地址问题：自用页必须同时拿到 VA 和 PA

VM 自己也是一个进程。它分配的任何自用页都有两个消费者：

| 地址 | 谁用 | 用途 |
|------|------|------|
| VA（虚拟地址） | CPU | 执行代码、读写数据（分页开启后所有访存都经 MMU） |
| PA（物理地址） | MMU / 硬件 | 页表 entry、CR3、DMA |

物理地址可以从 `alloc_mem` 直接拿——纯 bitmap 操作，不涉及页表，**不递归**。但虚拟地址不行：把一页映射进 VM 自己的地址空间（`vm_mappages`）需要写页表，而写页表可能需要分配新的页表页——这就回到了 `vm_allocpage`。**递归的根源是 VA 的获取，不是 PA 的获取**。这是理解本文档全部机制的一条主线。

### 1.2 递归链：映射页表页需要页表

完整递归链（pagetable.c:333-393 → 494-528）：

```
vm_allocpages()                          [level = 1]
  │
  ├── alloc_mem(pages, flags)            [PA：bitmap 扫描，不递归]
  │
  └── vm_mappages(phys, pages)           [VA：findhole + pt_writemap]
        │
        └── pt_writemap → pt_ptalloc     [页表页缺失时]
              │
              └── vm_allocpage(...)      [level = 2，递归重入！]
```

页表页需要两样东西：物理地址（写入 PDE 给 MMU）和虚拟地址（VM 往页表里写 PTE）。物理地址可以从 `alloc_mem` 拿（不递归）；虚拟地址如果走 `findhole + vm_mappages`，就回到了 `vm_allocpage`——必须有个东西在递归重入时兜底。

### 1.3 C 的两阶段方案：备用页池（自举）→ 动态分配（稳态）

Minix3 用 `pt_init_done` 标志（pagetable.c:328/1311）划分两个阶段：

| 条件 | 路径 | 说明 |
|------|------|------|
| `!pt_init_done` | `vm_getsparepage()` | 初始化期：VM 页表未就绪，不能 `vm_mappages` |
| `level > 1` | `vm_getsparepage()` | 递归重入：避免 `vm_mappages → pt_ptalloc → vm_allocpage` 无限递归 |
| `pt_init_done && level == 1` | `alloc_mem()` + `vm_mappages()` | 稳态：正常动态分配 |

备用页的 VA 来自 BSS 段静态数组 `static_sparepages`（pagetable.c:108），内核加载 VM 时已映射——**VA 编译期确定，不需要 findhole**。这是它打破递归的原因。

### 1.4 备用页池的本质：自举循环依赖的打破（非稳态供应）

pagetable.c:55-57 的注释明言备用页的用途：

> "Spare memory, ready to go after initialization, **to avoid a circular dependency on allocating memory and writing it into VM's page table**."

即：备用页池解决的是**自举循环依赖**（页表建立需要映射，映射需要页表）。它不是稳态供应——`pt_init` 末尾（pagetable.c:1311-1345）在 `pt_init_done = 1` 之后，立刻执行：

```c
alloc_cycle();                          /* Make sure allocating works */
while(vm_getsparepage(&phys)) ;         /* Use up all static pages */
alloc_cycle();                          /* Refill spares with dynamic */
```

把静态备用页整体用光、再以动态页重新填充（原因：liveupdate 后 BSS 静态页的物理地址会变化，pagetable.c:1316-1318 注释）。**备用页池是启动期的脚手架，不是运行时的页源**。

### 1.5 Direct Map：把 VA 变成常量偏移（[ARCH: A-1]）

minix-rs 的解法是 **Direct Map**（`[ARCH: A-1]`，详见 07-pagetable-struct.md）：内核在 VM 地址空间预映射一段固定窗口，使

```
VA = VM_DIRECT_MAP_BASE + PA        （常量偏移，direct_map.rs:12/21）
```

于是"拿到物理页"和"获得可访问的虚拟地址"是**同一件事**——`alloc_page()` 一次返回 `(VA, PA)`，不再需要 findhole，不再需要写 VM 页表来获得 VA。递归从结构上消失：

| 维度 | Minix3 `vm_allocpages` | minix-rs `VmPageAllocator` |
|------|------------------------|----------------------------|
| VA 来源 | `findhole` + `vm_mappages`（可能递归） | `vm_phys_to_virt(phys)`（常量加法） |
| 阶段切换 | `pt_init_done` + `level` 计数器 | 无（单路径从初始化起可用） |
| 递归保护 | `level` 计数 + 备用页池 | 不需要（递归不可能发生） |
| 页表页供给 | 备用页池 → `alloc_mem` | `vm_pt_alloc` 直取分配器（§3.5） |

这是 ARCH（架构演进）而非翻译：C 的"两阶段 + 备用页池"机制被"常量偏移映射"取代，外部行为（VM 总能拿到可用自用页）保持不变。

### 1.6 释放 / 锁定 / 校验 / 记账

四个配套语义：

- **释放**（`vm_freepages`）：解映射 + 物理释放一体（`WMF_FREE`）；BSS 静态页（`is_staticaddr`）拒绝释放。
- **锁定**（`vm_pagelock`）：改写 VM 自身页表 PTE 的 RW 标志，用于 MEMPROTECT 下 slab 数据页的写保护（slaballoc.c:45/52）。
- **校验**（`vm_addrok`）：调试辅助，逐级检查 PDE/PTE 的 PRESENT 与可写性。
- **记账**（`vm_self_pages` / `get_vm_self_pages`）：跟踪 VM 自身占用的页数，供 `get_usage_info_vm`（region.c:1370）在 `VM_GETRUSAGE` 查询中汇报。

### 1.7 与 05/07 的分工边界

| 文档 | 管辖 | 关键接口 |
|------|------|---------|
| 05-physical-memory | 任意大小连续块的物理分配、保留队列**机制**（alloc.c 侧） | `alloc_mem/free_mem`、`reservedqueue_*` 数据结构 |
| **06（本文档）** | **VM 自用页**：页分配、VA 映射、释放、锁定、校验、备用页池**消费方**、主循环补充接线 | `vm_allocpage` 族、`alloc_cycle` 调用点 |
| 07/08-pagetable | 页表结构与操作、Direct Map 双视图 | `pt_t`、`pt_writemap`、`vm_self_map` |
| 09-slab-allocator | 小对象堆、HeapArena 消费流程、MEMPROTECT | `slaballoc`、`HeapArena::grow/shrink` |

边界以本文档头部『说明』块函数清单为准绳：本文档覆盖 `vm_allocpage/vm_allocpages/vm_mappages/vm_freepages/vm_pagelock/vm_addrok/get_vm_self_pages`（7 个函数）+ 备用页池消费语义（`vm_getsparepage` 路径 + `alloc_cycle` 主循环接线）。

### 1.8 本章小结

- 自用页分配的本质是**双地址问题**：VA 获取递归，PA 获取不递归。
- C 用"备用页池（自举）→ 动态分配（稳态）"两阶段打破循环依赖；备用页池是脚手架，不是稳态页源。
- minix-rs 用 Direct Map 把 VA 变成常量偏移（[ARCH: A-1]），递归与阶段切换一起消失。
- 配套语义（释放/锁定/校验/记账）在 Rust 中各有归属（§3.4/§3.6）。

---

## 2. C 源码分析

### 2.1 vm_allocpages / vm_allocpage：两阶段分配（pagetable.c:333-397）

```c
// [pagetable.c:333] — 核心分配函数；vm_allocpage（L395-397）是 pages=1 特化
void *vm_allocpages(phys_bytes *phys, int reason, int pages)
{
    phys_bytes newpage;
    static int level = 0;              // L335：递归深度计数器
    ...
    level++;                           // L344：进入 +1
    assert(level >= 1 && level <= 2);  // L347：最多递归一层

    // L350：阶段 1（自举/递归）——从备用页池取页
    if ((level > 1) || !pt_init_done) {
        void *s;
        if (pages == 1) s = vm_getsparepage(phys);        // L353
        else if (pages == 4) s = vm_getsparepagedir(phys);// L354
        ...
        if (!is_staticaddr(s)) vm_self_pages++;           // L362：仅动态页计数
        return s;
    }

    // L368：阶段 2（稳态）——先取物理页
    if ((newpage = alloc_mem(pages, mem_flags)) == NO_MEM) { ... return NULL; }
    *phys = CLICK2ABS(newpage);       // L375：页号 → 物理地址

    // L379：再建立 VA 映射（可能递归进入 vm_allocpages，level=2）
    if (!(ret = vm_mappages(*phys, pages))) { ... return NULL; }

    vm_self_pages++;                  // L390：计入 VM 自身占用
    return ret;
}
```

两个关键点：

1. **`level` 是静态递归计数器**（pagetable.c:335），`vm_mappages` 内部递归进入 `vm_allocpages` 时 `level == 2`，走备用页池终止递归。它是运行时递归检测，不是并发保护——VM 单线程。
2. **`vm_self_pages` 只统计动态页**（pagetable.c:362 的 `!is_staticaddr(s)` 判断）：BSS 备用页是编译期预留的，不消耗物理内存池，不计入 VM 自身占用。

### 2.2 vm_mappages：findhole + pt_writemap + FLUSHTLB（pagetable.c:295-325）

```c
void *vm_mappages(phys_bytes p, int pages)
{
    vir_bytes loc;
    pt_t *pt = &vmprocess->vm_pt;

    loc = findhole(pages);                    // L302：在 VM 地址空间找空位
    if(loc == NO_MEM) { ... return NULL; }

    if((r=pt_writemap(vmprocess, pt, loc, p, VM_PAGE_SIZE*pages,
        ARCH_VM_PTE_PRESENT | ARCH_VM_PTE_USER | ARCH_VM_PTE_RW, 0)) != OK) { ... return NULL; }

    if((r=sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK)  // L319：刷 TLB
        panic("VMCTL_FLUSHTLB failed: %d", r);
    ...
}
```

`findhole`（pagetable.c:155-231，static）从 `lastv` 提示位起扫描 VM 的页目录，找一段连续空闲虚拟区间。`sys_vmctl(VMCTL_FLUSHTLB)` 是内核 IPC——`pt_writemap` 可能修改多条 PTE，Minix3 用整表刷新（`reload_cr3`）而非逐条 `invlpg`。

### 2.3 递归链分析：pt_ptalloc 的 side effect

`vm_mappages` 硬编码操作 VM 自己的页表（`&vmprocess->vm_pt`），所以内层 `pt_ptalloc`（pagetable.c:494-528）永远操作同一个页表。当外层 `pt_ptalloc` 也在处理同一页表时（如 `pt_init` 中），`findhole` 返回的 VA 可能落在外层正在处理的 PDE 范围——内层递归**先于外层**设置了 `pt->pt_pt[pde]` 和 `pt->pt_dir[pde]`；递归返回后外层发现 `pt->pt_pt[pde]` 已非空（pagetable.c:515-521）：

```c
if (!(p = vm_allocpage(&pt_phys, VMP_PAGETABLE)))
    return ENOMEM;
if (pt->pt_pt[pde]) {
    vm_freepages((vir_bytes) p, 1);   // 内层已代劳，释放自己刚拿的页
    assert(pt->pt_pt[pde]);
    return OK;
}
```

这一"释放已分配页、直接返回 OK"的路径是递归机制正确性的关键——**分配允许失败回退，但绝不允许死锁**。

### 2.4 vm_freepages：解映射 + 物理释放一体（pagetable.c:235-258）

```c
void vm_freepages(vir_bytes vir, int pages)
{
    assert(!(vir % VM_PAGE_SIZE));

    if(is_staticaddr(vir)) {          // L239：BSS 静态页拒绝释放
        printf("VM: not freeing static page\n");
        return;
    }

    if(pt_writemap(vmprocess, &vmprocess->vm_pt, vir,
        MAP_NONE, pages*VM_PAGE_SIZE, 0,
        WMF_OVERWRITE | WMF_FREE) != OK)          // L243-247：解映射 + 释放一体
        panic("vm_freepages: pt_writemap failed");

    vm_self_pages--;                  // L249：计数递减
    // SANITYCHECKS 下额外刷 TLB（L251-257）
}
```

`WMF_FREE` 使 `pt_writemap` 在取消映射的同时把物理页释放回分配器（内部调 `free_mem`）。`is_staticaddr`（pagetable.c:85：`(vir_bytes)(v) < VM_OWN_HEAPSTART`）判定 BSS 静态地址——静态页由系统回收，VM 不释放。

### 2.5 vm_getsparepage / vm_getsparepagedir（pagetable.c:264-294）

```c
static void *vm_getsparepage(phys_bytes *phys)
{
    void *ptr;
    if(reservedqueue_alloc(spare_pagequeue, phys, &ptr) != OK)
        return NULL;                  // L267-269：从保留队列取单页
    assert(ptr);
    return ptr;
}
```

`vm_getsparepagedir`（L277-294）从 `sparepagedirs[SPAREPAGEDIRS]` 数组取页目录（ARM 16KB 对齐场景），用 `missing_sparedirs` 记账。两者都是**消费保留队列**的入口——队列的生产在 `pt_init`（§2.10）与主循环 `alloc_cycle`（§2.9）。

### 2.6 vm_pagelock：MEMPROTECT 写保护（pagetable.c:403-437）

```c
void vm_pagelock(void *vir, int lockflag)
{
/* Mark a page allocated by vm_allocpage() unwritable, i.e. only for VM. */
    ...
    u32_t flags = ARCH_VM_PTE_PRESENT | ARCH_VM_PTE_USER;   // L408
    if(!lockflag) flags |= ARCH_VM_PTE_RW;                  // L415-416：解锁加 RW
    ...
    if((r=pt_writemap(vmprocess, pt, m, 0, VM_PAGE_SIZE,
        flags, WMF_OVERWRITE | WMF_WRITEFLAGSONLY)) != OK)  // L426-431
        panic("vm_lockpage: pt_writemap failed");
    ...
}
```

`WMF_WRITEFLAGSONLY` 表示只改标志不改映射。消费方是 slaballoc.c:45/52 的 `SLABDATAWRITABLE/SLABDATAUNWRITABLE` 宏（MEMPROTECT 下）：slab 数据页在用前解锁、用后重新写保护，把"内存损坏只发生在使用窗口内"的调试性质变成结构性约束。

### 2.7 vm_addrok：映射校验（pagetable.c:440-489）

`vm_addrok(vir, writeflag)` 逐级检查 VM 自身页表：PDE 的 PRESENT（L449-452）、writeflag 时 PDE 的可写性（L454-459，i386 分支）、PTE 的 PRESENT（L468-472）、writeflag 时 PTE 的可写性（L474-486）。任一失败打印诊断并返回 0。它是**调试/断言辅助**——正常运行路径不调用（`rg vm_addrok minix3/minix/servers/vm/` 仅定义处 + proto 声明）。

### 2.8 保留队列：reservedqueue_* 机制（alloc.c:56-237）

数据结构（alloc.c:60-74）：

```c
#define RESERVEDMAGIC     0x6e4c74d5
#define MAXRESERVEDPAGES  300
#define MAXRESERVEDQUEUES 15

static struct reserved_pages {
    struct reserved_pages *next;   /* 链入 first_reserved_inuse */
    int max_available;             /* 队列深度上限（0 = 未使用） */
    int npages;                    /* 每槽连续页数 */
    int mappedin;                  /* 槽是否需要预映射 VA */
    int n_available;               /* 当前可用槽数 */
    int allocflags;                /* 传给 alloc_mem 的标志 */
    struct reserved_pageslot {
        phys_bytes phys;
        void *vir;
    } slots[MAXRESERVEDPAGES];
    u32_t magic;
} reservedqueues[MAXRESERVEDQUEUES], *first_reserved_inuse = NULL;

int missing_spares = 0;            /* L74：缺口记账 */
```

API 面（全部 grep 实证）：

| 函数 | 行 | 语义 |
|------|----|------|
| `reservedqueue_new(max, npages, mapped, flags)` | 100-133 | 找空闲队列初始化，`missing_spares += max_available`（L131） |
| `reservedqueue_fillslot(rq, rps, ph, vir)` | 137-147 | 填入一个槽：`missing_spares--`（L144）+ `n_available++`（L145） |
| `reservedqueue_addslot(rq)` | 149-175 | 从 `alloc_mem` 取物理页，`mappedin` 时 `vm_mappages` 预映射，失败 `free_mem` 回滚 |
| `reservedqueue_add(rq, vir, ph)` | 179-188 | 静态页入队（pt_init 用，不分配） |
| `reservedqueue_fill(rq)` | 191-203 | 循环 addslot 补满到 `max_available` |
| `reservedqueue_alloc(rq, ph, vir)` | 206-224 | 出队一个槽，`missing_spares++`（L216） |
| `alloc_cycle()` | 227-237 | 遍历 `first_reserved_inuse` 队列，`missing_spares > 0` 时 fill |

**`missing_spares` 是"队列缺口"记账，不是分配失败计数**：出队 +1（L216）、入队 -1（L144）、新建队列预加（L131）。它回答的问题是"还有多少槽没填满"，而不是"最近失败了几次"——这是文档 06 与历史代码注释（曾误述为失败计数）的关键区分点。

### 2.9 alloc_cycle 与主循环接线（alloc.c:227-237 + main.c:112-119/745-746）

主循环两处调用 `alloc_cycle`：

```c
// main.c:118-119 —— 收消息前
if(missing_spares > 0) {
    alloc_cycle();      /* mem alloc code wants to be called */
}
// main.c:745-746 —— SIGKMEM 处理后（"pagetable code wants to be called"）
if(missing_spares > 0) {
    alloc_cycle();
}
```

`alloc_cycle` 的补充链是 `alloc_cycle → reservedqueue_fill → addslot → alloc_mem`；`alloc_mem` 自身在 NO_MEM 时先调 `cache_freepages`（cache.c:288，页缓存 LRU 回收）再重试（alloc.c:242-279）。所以**"主循环定期补充备用页"最终依赖页缓存回收**——这个跨文档依赖在 24-page-cache 收口。

### 2.10 pt_init 中的备用页建立与替换（pagetable.c:1088-1161/1311-1345）

**建立**（pt_init，pagetable.c:1151-1161）：

```c
if(!(spare_pagequeue = reservedqueue_new(SPAREPAGES, 1, 1, 0)))
    panic("reservedqueue_new for single pages failed");

for(s = 0; s < STATIC_SPAREPAGES; s++) {
    void *v = (void *) (sparepages_mem + s*VM_PAGE_SIZE);
    phys_bytes ph;
    // sys_umap：向内核查询 BSS 静态页的物理地址（VM 自己不知道）
    if((r=sys_umap(SELF, VM_D, (vir_bytes) v, VM_PAGE_SIZE*SPAREPAGES, &ph)) != OK)
        panic("pt_init: sys_umap failed: %d", r);
    reservedqueue_add(spare_pagequeue, v, ph);
}
```

`sys_umap` 是内核 IPC：VM 进程知道 BSS 静态页的 VA，但不知道 PA，必须向内核查询。数量（pagetable.c:60-68）：SANITYCHECKS 200/190、ARM 150/140、x86 生产 20/15（`SPAREPAGES`/`STATIC_SPAREPAGES`，差值为动态补充槽）。

**替换**（pt_init 末尾，pagetable.c:1311-1345）：`pt_init_done = 1` 后执行 §1.4 的三步（alloc_cycle → 用光静态页 → alloc_cycle），再用纯动态分配重建整个 VM 页表（`pt_new` + `pt_copy` + `memcpy`，L1338-1341）。原因（L1316-1318 注释）：**liveupdate 后 BSS 静态页的物理地址会变化**，动态页不受影响。

### 2.11 消费方全景：谁调用 vm_allocpage 族

`rg vm_allocpage|vm_allocpages|vm_freepages|vm_pagelock minix3/minix/servers/vm/` 实证（排除 pagetable.c 自身定义）：

| 消费方 | 位置 | 用途 |
|--------|------|------|
| `pt_ptalloc` | pagetable.c:515 | 页表页（`VMP_PAGETABLE`）——07/08 文档 |
| `pt_new` | pagetable.c:1005-1009 | 页目录分配（`VMP_PAGEDIR`，x86 1 页 / ARM 4 页 → `vm_getsparepagedir`）——07/08 文档 |
| `pt_allocate_kernel_mapped_pagetables` | pagetable.c:1051 | 内核共享页表页（`VMP_PAGETABLE`）——07/08 文档 |
| `pt_free` | pagetable.c:1434 | 释放页表页（`vm_freepages`）——07/08 文档 |
| `newslabdata` | slaballoc.c:166 | slab 数据页（`VMP_SLAB`）——09 文档 |
| `slabfree` | slaballoc.c:449 | 释放 slab 数据页——09 文档 |
| `SLABDATAWRITABLE/UNWRITABLE` | slaballoc.c:45/52 | `vm_pagelock` MEMPROTECT——09 文档 |
| `mmap`/`munmap`（VM 自身 libc） | utility.c:369/378 | VM 自身 libc 兼容接口，无 servers/vm 内部调用者（01 边界声明） |

即：**C 侧 `vm_allocpage` 的真实消费者只有页表代码（pt_ptalloc/pt_new/pt_allocate_kernel_mapped_pagetables/pt_free）与 slab 代码（newslabdata）**。Rust 侧对应：页表页 → `vm_pt_alloc`（§3.5），slab/堆页 → `HeapArena::grow` 经 `alloc_phys`（09 文档）。

---

## 3. Rust 设计决策

### 3.1 D1: `VmPageAllocator`——单路径 (VA, PA) 分配（合并 C 两步）

- **C**: `vm_allocpages` = `alloc_mem`（PA）+ `vm_mappages`（VA：findhole + pt_writemap + FLUSHTLB）两步，且初始化期走备用页池。
- **Rust**: `VmPageAllocator`（alloc_page.rs:47）包装物理分配器，`alloc_page()/alloc_pages()`（alloc_page.rs:71/77）一次返回 `(VirBytes, AlignedPhysBytes)`——VA 由 `vm_phys_to_virt` 常量偏移给出，两步合并为一步，初始化期与稳态无差别。
- **为什么**：Direct Map（D2）使 VA 不再是"稀缺资源"（无需 findhole），两步合并是自然结果；返回类型 `(VirBytes, AlignedPhysBytes)` 让"双地址"在类型层面显式化，杜绝只拿一个地址的用法。
- **行为契约**：`alloc_page(flags)` 返回的 `(v, p)` 恒满足 `virt_to_phys(v) == p`（direct_map.rs:31 双向转换）；多页分配 VA 连续（`v + i*CLICK` ↔ `p + i*CLICK`）；`free_pages(phys, clicks)` 释放后同一 PA 可再次分配（bitmap 单页缓存 LIFO 保证）。

### 3.2 D2: Direct Map——递归结构性消除（[ARCH: A-1]）

- **C**: VA 获取递归 + `level` 计数器 + `pt_init_done` 两阶段 + BSS 备用页池 + `sys_vmctl(FLUSHTLB)`。
- **Rust**: `vm_phys_to_virt(phys) = VM_DIRECT_MAP_BASE + phys`（direct_map.rs:12/21）——VA 是常量偏移，页表页分配（`vm_pt_alloc`）、堆页分配从初始化起单路径可用。TLB 维护归 `Paging` 实现内部（x86_64/paging.rs `write_pte_dm` 逐条 `invlpg`，不再需要整表刷新 IPC）。
- **对照 Redox / 主流 OS**：这是"**物理帧分配器不依赖映射子系统**"原则的具体化——Redox 内核早期以恒等映射 + `FrameAllocator` 打破同类循环；Linux 以 bootmem/memblock（物理地址直接操作）+ 固定映射区（fixmap）打破。minix-rs 的 Direct Map 与二者同构：分配器工作在 PA 空间，VA 由预映射窗口保证。
- **行为契约**：任何初始化阶段调用 `alloc_page` 都返回可立即访问的 (VA, PA) 对——C 的"初始化期只能取备用页"限制被结构性移除（`[ARCH: A-1]` 三处一致标注：本 doc §3.2 + design.v1 D2 + alloc_page.rs:26-30 代码注释）。

### 3.3 D3: 保留页池的归宿——结构消除 + 压力计数（[ARCH: A-1]）

- **C**: `reservedqueue_*`（alloc.c:56-237）+ `missing_spares` 缺口记账 + 主循环 `alloc_cycle` 补充。
- **Rust**: **通用 `CriticalPool<T>`（critical_pool.rs）删除**——理由三层：
  1. C 池是**自举机制**而非稳态供应（§1.4 实证：pagetable.c:55-57 注释 + pt_init 末尾整体替换）；
  2. 自举循环依赖已被 Direct Map 结构性打破（D2），池无生产消费方（`rg CriticalPool os/servers/vm/src/` 全量 0 hits——文件已随结构消除删除）；
  3. Redox / Linux 均无 VM 侧备用页池——同类问题都以"物理分配不依赖映射"解决。
- **保留的语义**：`missing_spares` 在 `VmServer` 中保留（vm_server.rs:54-70），重解释为**分配压力计数**——`mark_alloc_failure()`（饱和计数，`&mut self` 独占）记录 `alloc_*` 失败；主循环 `if(missing_spares > 0) alloc_cycle()`（vm_server.rs:425-428）与 C main.c:118-119 **同一主循环位置**（C 第二调用点 main.c:745-746 在 SIGKMEM 处理后，随 SIGKMEM 事件处理落地——Rust 主循环当前仅镜像 main.c:118-119 一处，见 01 §5）；`alloc_cycle()` 方法（vm_server.rs:416-419）是补充钩子，**补充体（页缓存回收 + 重试）DEFERRED 归 24-page-cache**（C 的 `alloc_mem → cache_freepages` 链在 Rust 侧的落点）。
- **行为契约**：`missing_spares > 0` → 主循环下一轮执行补充钩子（与 C 外可观测行为收敛）；C 的"缺口精确值"与 Rust 的"压力信号"在契约层面等价（都是 `> 0` 触发补充机会）。
- **决策同步**：plan.md §7.3 + checklist.md M-127-M-130/F-012/F-158/F-159 行已同步（三处一致）。

### 3.4 D4: `vm_self_pages` → `VmAllocStats`——类型化记账

- **C**: `static int vm_self_pages`（pagetable.c:34），`vm_allocpages` 成功 +1 / `vm_freepages` -1；`get_vm_self_pages()`（pagetable.c:1500）供 `get_usage_info_vm`（region.c:1370）汇报。
- **Rust**: `VmAllocStats`（alloc_stats.rs）——`record_alloc/record_dealloc/record_failure` 对称记账，`active_allocations()/active_pages()` 派生活跃值，`check_leak()` 检测泄漏（alloc 数 > dealloc 数即活动页泄漏）。`VmPageAllocator::self_alloc_count()/self_page_count()`（alloc_page.rs:98/102）暴露。
- **为什么**：C 的手工 `++/--` 在分配失败路径容易漏减；`record_alloc` 挂在 `alloc_phys` 成功分支、`record_dealloc` 挂在 `free_pages`，**记账与分配/释放在同一函数内配对**，结构性消除漏减。
- **行为契约**：`self_page_count()` 等价 `get_vm_self_pages()`；语义差异诚实标注：C 排除 BSS 静态备用页（pagetable.c:362），Rust 全动态无排除（Direct Map 下所有自用页都经分配器）。26-vm-queries 经 `self_page_count` 汇报 VM 自身占用。

### 3.5 D5: `vm_pt_alloc`——页表页供给链注册

- **C**: `pt_ptalloc → vm_allocpage(&pt_phys, VMP_PAGETABLE)`（pagetable.c:515）——页表页来自 VM 页分配器；初始化/递归期走备用页池。
- **Rust**: `alloc_page::vm_pt_alloc()`（alloc_page.rs:32-46）——签名 `fn() -> Result<(PhysBytes, VirBytes), PageTableError>`，注册进 `minix_arch::pt_alloc`（vm_server.rs:102-107，`is_registered()` 守卫防重复注册，mirror os/kernel/src/lib.rs:178 惯例）：

```rust
// alloc_page.rs:32-46（关键路径，节选）
pub(crate) fn vm_pt_alloc() -> Result<(minix_types::PhysBytes, VirBytes), PageTableError> {
    let phys = crate::global::page_alloc_mut()
        .alloc_phys(1, PageAllocFlags::empty())
        .map_err(|_| PageTableError::AllocationFailed)?;
    let virt = vm_phys_to_virt(phys);
    // Zero-fill via the Direct Map. `Paging::walk_alloc` (x86_64/paging.rs)
    // reads PRESENT bits of freshly allocated tables and must observe zeros.
    // SAFETY: `virt` is a page-aligned Direct Map VA of a freshly allocated,
    // exclusively owned physical page; no aliasing reference exists.
    unsafe {
        core::ptr::write_bytes(virt.0 as *mut u8, 0, CLICK_SIZE);
    }
    Ok((minix_types::PhysBytes(phys.as_u64()), virt))
}

```

- **为什么 fn 指针 + 全局访问**：`minix_arch::pt_alloc` 的注册接口是 `fn()`（无状态签名），VM 侧通过 `global::page_alloc_mut()`（global.rs:356-380，从 `PAGE_ALLOC_PTR` 派生 `&'static mut VmPageAllocator`，单线程 + outlive 论证）访问分配器——与 boot 侧 `boot_pt_alloc`（恒等映射）共用同一注册机制。
- **清零是硬需求**：`Paging::walk_alloc`（x86_64/paging.rs:277-330）读取新分配页表页的 PRESENT 位判断是否分配下一级——**必须观察到 0**，否则垃圾位被当作有效页表项。
- **行为契约**：任何 `Paging::new()/map()` 调用前必须已注册（否则 `uninit_alloc` 返回 `AllocationFailed`）；注册顺序 = `register_page_alloc` → `pt_alloc::register` → `init_vm_self_pt`（vm_server.rs:87-109）；新页表页恒零填充、页对齐、`virt = VM_DIRECT_MAP_BASE + phys`。

### 3.6 D6: `vm_pagelock` / `vm_addrok`——语义移交（07/08/09）

| C 函数 | C 位置 | Rust 语义承接 | 归属 |
|--------|--------|--------------|------|
| `vm_pagelock` | pagetable.c:403-437 | 页标志改写 → `Paging` `PageFlags` + `vm_self_mappages`（OVERWRITE 语义）；MEMPROTECT 写保护硬化 → HeapArena 页策略 | 07/08-pagetable、09-slab |
| `vm_addrok` | pagetable.c:440-489 | 映射校验 → `vm_self_query`（vm_self_map.rs:127-135，已存在） | 07/08-pagetable |

06 不提供 `pagelock`/`addrok` API；doc 06 §2 完整覆盖 C 函数语义并声明移交。MEMPROTECT（slab 数据页用后写保护）在 Rust 侧**未实现**（HeapArena 页恒可写）——诚实标注为硬化项（可归 09 后续），不是当前行为缺口。

### 3.7 语义差异清单（C ↔ Rust 诚实标注）

| # | C 行为 | Rust 行为 | 差异类型 |
|---|--------|----------|---------|
| S-1 | `vm_allocpages` 两阶段 + `level`/`pt_init_done` | 单路径 `alloc_page` | 结构性替代（[ARCH: A-1]，D2） |
| S-2 | VA 来自 findhole（动态扫描） | VA 来自 Direct Map 常量偏移 | 结构性替代（[ARCH: A-1]，D2） |
| S-3 | 备用页池（BSS 静态 + 动态补充） | 无池；`missing_spares` 压力计数 + 补充钩子 | 结构消除（[ARCH: A-1]，D3） |
| S-4 | `vm_freepages` 解映射 + 释放一体（WMF_FREE） | `free_pages`（物理释放）+ 消费方 `vm_self_unmap`（解映射）两步 | 职责拆分（D1，09 消费） |
| S-5 | `vm_self_pages` 排除 BSS 静态页 | `self_page_count` 全动态 | 计数口径差异（D4，诚实标注） |
| S-6 | `sys_vmctl(VMCTL_FLUSHTLB)` 整表刷新 | `Paging` 实现内逐条 `invlpg` | 机制差异（TLB 维护归 arch，D2） |
| S-7 | `alloc_mem` NO_MEM 时 `cache_freepages` 重试 | 无缓存回收重试（DEFERRED） | 覆盖缺口（D3，归 24） |
| S-8 | `sys_umap` 查询 BSS 静态页 PA | 无（无静态页） | 机制消失（D2） |

---

## 4. 实现详解

### 4.1 `alloc_page.rs`：`VmPageAllocator` 与 `vm_pt_alloc`

- `VmPageAllocator`（alloc_page.rs:47-117）：`alloc_phys(clicks, flags)`（L60，物理分配 + 记账）、`alloc_page/alloc_pages`（L71/77，返回 `(VirBytes, AlignedPhysBytes)`）、`free_page/free_pages`（L85/89，释放 + 记账）、`total_pages/self_alloc_count/self_page_count/stats`（L94-106）。
- `PfnAllocator` impl（alloc_page.rs:119-133）：PFN 索引模型适配（`PageFrames` 集成），`alloc_pfn/free_pfn` 经 `PAGE_SIZE` 换算——06 范围外（region 文档）但接口落点在此。
- `vm_pt_alloc`（alloc_page.rs:32-46）：§3.5，页表页供给链。
- **模块注释**（alloc_page.rs:1-7）：声明"wraps PhysAlloc + Direct Map VA↔PA translation"，与 05 的 `PhysAllocator` 分层明确。

### 4.2 `global.rs`：`page_alloc_mut` 全局访问

- `PAGE_ALLOC_PTR: AtomicPtr<VmPageAllocator>`（global.rs:303）——`register_page_alloc`（L307，compare_exchange 防覆盖）/ `unregister_page_alloc`（L343，Drop 时清空）。
- `page_alloc_mut()`（global.rs:356-380）：新增访问器，供无状态 fn 指针（`vm_pt_alloc`）获取 `&'static mut VmPageAllocator`。SAFETY 论证三条：单线程事件循环无并发访问；outlive（VmServer 持有分配器，指针生命周期受 `register/unregister` 约束）；无别名（调用点在事件循环不持有其他 `&mut` 借用时）。
- 既有 `refill_arena`（global.rs:384-406）经同一指针访问分配器——两个消费方共享单一注册点。

### 4.3 `vm_server.rs`：注册顺序与主循环接线

构造顺序（vm_server.rs:87-109）——**这是"页分配器 → 页表"供给链的落地顺序**：

```
create_default_allocator()          // 物理分配器（bitmap，A-5）
  → VmPageAllocator::new()          // 页级包装
  → register_page_alloc()           // 全局指针注册
  → pt_alloc::register(vm_pt_alloc) // 页表页供给链注册（新增）
  → init_vm_self_pt()               // VM 自身页表（07/08）
```

主循环（vm_server.rs:425-428）：`if missing_spares > 0 { self.alloc_cycle(); }`——与 C main.c:118-119 同位置；`alloc_cycle`（L416-419）清计数，补充体 DEFERRED 归 24。`missing_spares` 字段（L54-70）文档化 C↔Rust 语义映射（§3.3）。

### 4.4 消费方：`heap_arena.rs` / `vm_self_map.rs` / `direct_map.rs`

- `HeapArena::grow`（heap_arena.rs:88-136）：逐页 `alloc_phys(1)` + `vm_self_mappages`（写 VM 自身页表）建立连续 VA；失败回滚（已映射页 `vm_self_unmap` + `free_page`）。`shrink`（L138-177）反向：`vm_self_unmap` + `free_page`——这是 `vm_freepages` 语义的 Rust 消费方（09 文档详述）。
- `vm_self_map.rs`：`init_vm_self_pt`（L82-99）、`vm_self_mappages`（L100-114）、`vm_self_unmap`（L116-125）、`vm_self_query`（L127-135，`vm_addrok` 语义对应）。
- `direct_map.rs`：`VM_DIRECT_MAP_BASE`（L12）、`vm_phys_to_virt`（L21）、`virt_to_phys`（L31）、`is_direct_map_virt`（L37）——A-1 基础，07 详述双视图。

---

## 5. 测试要点

### 5.1 单元测试清单（`os/servers/vm/src/`）

| 模块 | 测试数 | 覆盖 |
|------|-------|------|
| `alloc_page.rs` | 7 | 单页分配（VA=DM+PA + roundtrip）、多页分配（VA 连续 + 多页 roundtrip）、分配-释放回收、`vm_pt_alloc`（清零/对齐/去重/全局访问）、记账对称（self_alloc_count/self_page_count）、total_pages |
| `direct_map.rs` | 5 | `vm_phys_to_virt`/`virt_to_phys` roundtrip、`is_direct_map_virt`、mock base 机制 |
| `global.rs` | 5 | kernel layout、VM instance 计数、boot image（`TOTAL_PAGES` 读写/`add_total_pages` 由 vm_server 记账测试间接覆盖） |
| `vm_server.rs` | 22 | 构造/init 流程、`missing_spares` 压力计数 + `alloc_cycle` 钩子（`test_missing_spares_pressure_counter`）、VFS transid 等 |
| `heap_arena.rs` | 1 | 常量/边界测试；grow/shrink/失败回滚测试随 09 落地 |

### 5.2 覆盖维度

- **C 行为契约**：单页/多页分配、VA↔PA 双向转换、释放后回收、记账对称性、页表页清零（walk_alloc PRESENT 依赖）；
- **架构演进**：`[ARCH: A-1]` 三处一致（doc §3.2 / design.v1 D2 / alloc_page.rs:26-30 注释）；`missing_spares` 压力计数语义（vm_server.rs 字段文档）；
- **供给链**：`vm_pt_alloc` 注册顺序（构造测试）、全局访问器 `page_alloc_mut`（vm_server 测试间接覆盖）。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 |
|------|------|
| `vm_pagelock`（MEMPROTECT 写保护硬化） | 未实现（语义移交 07/08/09，§3.6） |
| `vm_addrok` 调试校验 | 语义对应 `vm_self_query`（07/08 接线，无独立实现） |
| `alloc_cycle` 补充体（`cache_freepages` 回收重试） | DEFERRED，归 24-page-cache（§3.3） |
| `vm_pt_alloc` 生产路径（真实 4 级页表中间表分配） | QEMU 集成验证；`test_vm_pt_alloc` 用 mock 分配器验证逻辑 |
| C 的 `level` 计数器 / `pt_init_done` / BSS 静态页 / `sys_umap` | 结构性消失（[ARCH: A-1]），无对应测试对象 |

### 5.4 测试统计（截至 2026-08-15）

- `cargo test -p minix-vm --lib`：**347 passed / 1 failed**
- 本次 06 修复：`alloc_page` 2 个 pre-existing 失败（PE-1/PE-2，plan.md §3.5 基线）→ 已修复闭环；新增 `test_vm_pt_alloc`（alloc_page.rs）+ `test_missing_spares_pressure_counter`（vm_server.rs）2 个
- 剩余 1 个失败：`region::vir_region::tests::test_map_lazy`（13 范围，pre-existing，不属本文档）
- 本文档相关模块（alloc_page/direct_map/global/vm_server/heap_arena）默认 feature 构建下全部通过

---

## 6. 过渡

本文档在启动时序中的位置：`init_vm()` 的 `init_proc(VM_PROC_NR) + pt_init` 链（main.c:474-475）与主循环的 `alloc_cycle` 钩子（main.c:118-119）之间——是 05（物理分配器）之后、07/08（页表）之前的"VM 自用页"桥梁。ACL（04）决定"谁可以调服务"，物理分配器（05）决定"谁拥有哪段物理内存"，本文档决定"VM 自己怎么拿到可用的 (VA, PA) 对"。

**下一篇入口（07-pagetable-struct）**：`vm_pt_alloc` 供给的页表页被 `PageTable::new()` 与 `Paging::map` 消费；07 建立 `pt_t` 结构与 Direct Map 双视图（A-1 的页表侧），08 实现 `pt_new/writemap/...` 操作——06 的"页分配"与 07/08 的"页映射"在此闭环。09（slab/HeapArena）则消费 06 的 `alloc_page` 建立 VM 自身的堆。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/00-vm-overview.md` — 启动主线图与文档导航
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` — `init_vm` 调用点、VM 自身 libc 接口边界
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/05-physical-memory.md` — 物理分配器、保留队列机制（C 侧）、`alloc_cycle` DEFERRED 移交
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md` — Direct Map 双视图（A-1）、`vm_self_map`
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/08-pagetable-ops.md` — `pt_writemap` 等操作（`vm_pagelock/vm_addrok` 语义承接）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/09-slab-allocator.md` — HeapArena 消费 `alloc_page`、MEMPROTECT（D6 移交）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/24-page-cache.md` — `cache_freepages` 回收（`alloc_cycle` 补充体 DEFERRED 落点）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/26-vm-queries.md` — `VM_GETRUSAGE`（`self_page_count` 接线）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/plan.md` — §2 阶段总览、§3.4 边界表、§4 A-1、§7.3 保留页池决策
- `minix3/minix/servers/vm/pagetable.c`、`minix3/minix/servers/vm/alloc.c`、`minix3/minix/servers/vm/main.c` — C 源码（ground truth）
- `os/servers/vm/src/alloc_page.rs`、`os/servers/vm/src/global.rs`、`os/servers/vm/src/vm_server.rs` — Rust 实现
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/05-vm-allocpage.md` — 旧主线素材（素材，非正式引用源）
