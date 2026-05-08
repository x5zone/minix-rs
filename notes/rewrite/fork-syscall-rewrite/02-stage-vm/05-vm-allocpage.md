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
VM 需要分配物理页 → 需要页表来映射 → 页表本身也需要物理页 → 谁来分配？
```

更具体地说，`vm_allocpage` 需要同时返回两个地址：

| 返回值 | 用途 | 消费者 |
|--------|------|--------|
| 虚拟地址（VA） | CPU 执行代码、读写数据 | VM 代码 |
| 物理地址（PA） | MMU 硬件做地址翻译 | 页表 entry、CR3 |

物理地址可以从物理内存分配器（`alloc_mem`）获取——纯 bitmap 操作，不涉及页表。但虚拟地址的获取需要经过 `vm_mappages`（详见 [07-pagetable-ops.md §2.3.3](07-pagetable-ops.md#2333-vm_mappages---分配虚拟地址并建立映射)），而 `vm_mappages` 在映射之前必须确保目标虚拟地址对应的页表已存在。如果页表不存在，就需要分配一个新的页表页——这就回到了 `vm_allocpage`，形成递归。

### 1.3 接口定义

```c
void *vm_allocpages(phys_bytes *phys, int reason, int pages);
void *vm_allocpage(phys_bytes *phys, int reason);
```

| 参数 | 含义 |
|------|------|
| `phys` | [出参] 物理地址，供硬件使用（如加载到 CR3） |
| `reason` | 用途分类：`VMP_SPARE`(0) / `VMP_SLAB` / `VMP_PAGEDIR` / `VMP_PAGETABLE` |
| `pages` | 页数（`vm_allocpage` 固定为 1） |
| 返回值 | 虚拟地址，供 VM 代码访问 |

**关键设计**：一次调用同时返回虚拟地址和物理地址。这是递归问题的根源——两个地址的获取路径不同，其中一个可能触发另一个的再次调用。

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
    static int level = 0;
    void *ret;
    u32_t mem_flags = 0;

    assert(reason >= 0 && reason < VMP_CATEGORIES);
    assert(pages > 0);

    level++;
    assert(level >= 1 && level <= 2);

    // 阶段 1：初始化阶段 或 递归情况
    if ((level > 1) || !pt_init_done) {
        void *s;
        if (pages == 1) s = vm_getsparepage(phys);
        else if (pages == 4) s = vm_getsparepagedir(phys);
        else panic("%d pages", pages);

        level--;
        if (!s) {
            util_stacktrace();
            printf("VM: warning: out of spare pages\n");
        }
        if (!is_staticaddr(s)) vm_self_pages++;
        return s;
    }

    // 阶段 2：正常运行阶段
#if defined(__arm__)
    if (reason == VMP_PAGEDIR) {
        mem_flags |= PAF_ALIGN16K;
    }
#endif

    if ((newpage = alloc_mem(pages, mem_flags)) == NO_MEM) {
        level--;
        printf("VM: vm_allocpage: alloc_mem failed\n");
        return NULL;
    }

    *phys = CLICK2ABS(newpage);

    if (!(ret = vm_mappages(*phys, pages))) {
        level--;
        printf("VM: vm_allocpage: vm_mappages failed\n");
        return NULL;
    }

    level--;
    vm_self_pages++;
    return ret;
}
```

两条路径：

| 条件 | 路径 | 说明 |
|------|------|------|
| `!pt_init_done` | `vm_getsparepage()` | 初始化阶段，页表系统未就绪 |
| `level > 1` | `vm_getsparepage()` | 递归调用，避免在 `alloc_mem` 内部再次触发 `alloc_mem` |
| `pt_init_done && level == 1` | `alloc_mem()` + `vm_mappages()` | 正常运行 |

### 2.2 阶段 1：静态备用页（BSS）

初始化阶段，页表系统尚未就绪，无法调用 `vm_mappages()` 做动态映射。Minix3 的解法是编译时在 BSS 段预留静态备用页，内核加载 VM 时已将这些页映射到 VM 的地址空间。

**备用页数量**（条件编译）：

| 构建配置 | `SPAREPAGES` | `STATIC_SPAREPAGES` |
|---------|-------------|---------------------|
| SANITYCHECKS | 200 | 190 |
| ARM 生产构建 | 150 | 140 |
| x86 生产构建 | 20 | 15 |

> 文档中分析以 SANITYCHECKS 值（190 静态页）为例，生产构建（x86 仅 15 页）的递归保护机制相同，但池容量更小。

```c
static char static_sparepages[VM_PAGE_SIZE * STATIC_SPAREPAGES]
    __aligned(VM_PAGE_SIZE);
```

初始化流程（`pt_init()`）：

```c
void pt_init(void)
{
    int s, r;
    vir_bytes sparepages_mem;

    sparepages_mem = (vir_bytes) static_sparepages;
    assert(!(sparepages_mem % VM_PAGE_SIZE));

    spare_pagequeue = reservedqueue_new(SPAREPAGES, 1, 1, 0);

    for (s = 0; s < STATIC_SPAREPAGES; s++) {
        void *v = (void *)(sparepages_mem + s * VM_PAGE_SIZE);
        phys_bytes ph;
        if ((r = sys_umap(SELF, VM_D, (vir_bytes)v,
                VM_PAGE_SIZE * SPAREPAGES, &ph)) != OK)
            panic("pt_init: sys_umap failed: %d", r);
        reservedqueue_add(spare_pagequeue, v, ph);
    }

    pt_init_done = 1;  // 切换标志
}
```

`pt_init_done = 1` 是分水岭。在此之前所有 `vm_allocpage` 调用走备用页，在此之后走动态分配。

### 2.3 阶段 2：动态分配

条件：`pt_init_done == 1` 且 `level == 1`（非递归）。

```
vm_allocpage()
    ├── alloc_mem(pages, flags)    // 从物理内存池分配（纯 bitmap，不递归）
    └── vm_mappages(phys, pages)   // 映射到 VM 虚拟地址空间
```

### 2.4 递归链分析

`vm_allocpage` 的递归不是来自 `alloc_mem`（纯 bitmap 操作），而是来自 `vm_mappages`（详见 [07-pagetable-ops.md §2.3.3](07-pagetable-ops.md#2333-vm_mappages---分配虚拟地址并建立映射)）。完整递归链（[pagetable.c:333-392](minix3/minix/servers/vm/pagetable.c#L333-L392) → [pagetable.c:494-523](minix3/minix/servers/vm/pagetable.c#L494-L523)）：

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

**注释中提到的 "side effect"**：递归调用 `vm_allocpage`（level=2）走 `vm_getsparepage()` 拿到一个备用页，然后 `pt_ptalloc` 用这个备用页设置了 `pt->pt_pt[pde]` 和 `pt->pt_dir[pde]`。当递归返回、外层 `pt_ptalloc` 继续执行时，发现 `pt->pt_pt[pde]` 已经非空——说明递归调用已经替它完成了页表分配，于是释放自己刚拿到的备用页，直接返回 OK。

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

`pt_init_done = 1` 之后递归仍然可能发生。Minix3 的做法是：先用光 BSS 静态备用页，再调用 `alloc_cycle()` 用**动态分配**的页重新填充备用页池。这样递归保护在初始化后依然有效。

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

---

## 3. Rust 设计决策

本章展示三种初始化方案，从 Minix3 原方案出发，逐步演进到最终采用的 Typestate 模式。三种方案的对比本身就是一次设计思维的训练。

### 3.1 方案一：BSS 静态分配（Minix3 原方案）

**思路**：编译期在 BSS 段预留固定大小的备用页数组，内核加载时已映射。

```rust
// 方案一：直接翻译 Minix3 的 BSS 方案
#[link_section = ".bss.reserved"]
static mut STATIC_SPARE_PAGES: [u8; STATIC_SPARE_SIZE] = [0; STATIC_SPARE_SIZE];
```

**优点**：
- 实现简单，Minix3 已验证
- 虚拟地址编译期确定，无运行时开销

**缺点**：

| 问题 | Minix3 (32-bit) | 64-bit |
|------|-----------------|--------|
| bitmap 大小 | 128KB（1M 页） | 无法静态分配完整 bitmap |
| 备用页数量 | 190 页（760KB） | 编译期固定，不灵活 |
| 扩展性 | 编译期固定 | 需要运行时动态 |
| 地址空间 | 32 位，BSS 够用 | 64 位，但 BSS 方案本身不利用 64 位优势 |

**结论**：BSS 方案在 64 位下没有根本性问题（备用页本身不需要覆盖全部地址空间），但缺乏灵活性。更重要的是，它把"初始化"和"正常运行"两个阶段的区分交给了运行时的 `pt_init_done` 标志——这个标志是一个全局可变状态，任何函数都可以读取它，编译器无法验证阶段切换的正确性。

### 3.2 方案二：内核预留区域 + 运行时标志

**思路**：kernel 在启动 VM 时预留一段物理内存区域，通过启动信息传递给 VM。VM 内部仍用 `pt_init_done` 标志区分阶段。

```
┌─────────────────────────────────────────────────────────────┐
│              内核预留内存方案                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Kernel 启动 VM 时:                                         │
│  ┌──────────────────────────────────────┐                   │
│  │  boot_info.reserved_region           │                   │
│  │  ├── phys_start: 0x1000000           │                   │
│  │  ├── phys_end:   0x1100000           │                   │
│  │  ├── virt_start: 0x7f0000000000      │                   │
│  │  └── size:       0x100000 (1MB)      │                   │
│  └──────────────────────────────────────┘                   │
│         │                                                   │
│         ▼                                                   │
│  VM 初始化阶段:                                              │
│  ┌──────────────────────────────────────┐                   │
│  │  1. 从 reserved_region 分配备用页     │                   │
│  │  2. 初始化 PhysAllocator (bitmap)    │                   │
│  │  3. 初始化页表系统                    │                   │
│  │  4. 搬迁已初始化数据到堆              │                   │
│  │  5. 释放 reserved_region 回物理池    │                   │
│  └──────────────────────────────────────┘                   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

```rust
// 方案二：内核预留区域 + 运行时标志
pub(crate) struct VmPageAllocator {
    stage: AllocStage,
    reserved: ReservedRegion,
    phys_alloc: PhysAllocator,
    mapper: PageMapper,
}

enum AllocStage {
    Bootstrap,
    Normal,
}

impl VmPageAllocator {
    pub(crate) fn alloc_page(
        &mut self, reason: PageCategory,
    ) -> Option<(VirtAddr, PhysAddr)> {
        match self.stage {
            AllocStage::Bootstrap => self.reserved.alloc_page(reason),
            AllocStage::Normal => self.dynamic_alloc(reason),
        }
    }

    pub(crate) fn switch_to_normal(&mut self) {
        self.stage = AllocStage::Normal;
    }
}
```

**优点**：
- 解决了 BSS 方案在 64 位下的灵活性问题
- 预留区域大小由 kernel 决定，可运行时调整
- 初始化完成后预留区域归还物理池，不浪费内存

**缺点**：
- `stage` 仍然是运行时状态，编译器无法验证
- `switch_to_normal()` 之后，`reserved` 字段仍然存在（虽然不再使用），占用结构体空间
- 如果某段代码在 Normal 阶段错误地访问了 `reserved`，编译器不会报错

### 3.3 方案三：Typestate 模式（最终选择）

**思路**：将"初始化"和"正常运行"编码为两个不同的类型，阶段切换通过消耗旧类型、产生新类型来完成。编译器在编译期保证：Normal 阶段不可能访问 `reserved`，Bootstrap 阶段不可能调用 `dynamic_alloc`。

```rust
pub(crate) struct Bootstrap;
pub(crate) struct Normal;

/// 泛型参数 S 标记当前阶段，O 抽象页表操作（便于测试注入）
pub(crate) struct VmPageAllocator<S, O: PtOps = RealPtOps> {
    reserved: ReservedRegion,
    phys_alloc: Option<Box<dyn PhysAllocator>>,
    pt_region: Option<PtRegion<O>>,
    pt_ops: Option<O>,
    _stage: PhantomData<S>,
}
```

**Bootstrap 阶段**：

```rust
impl<O: PtOps> VmPageAllocator<Bootstrap, O> {
    /// 从预留区域分配，直接返回 (VA, PA)。
    /// 预留区域已被内核映射，不需要 vm_mappages。
    pub(crate) fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
        self.reserved.alloc_page()
    }
}

impl VmPageAllocator<Bootstrap, RealPtOps> {
    pub(crate) fn new(
        reserved: ReservedRegion,
        phys_alloc: Box<dyn PhysAllocator>,
    ) -> Self { ... }

    /// 消耗 Bootstrap，产生 Normal。
    /// 从 reserved 切出 3 页初始化 PtRegion，其余物理页后续由搬迁逻辑归还。
    /// 
    /// 注意：into_normal 仅在 RealPtOps 上实现，因为 PtRegion::from_reserved
    /// 需要真实的页表操作。测试代码使用 into_normal_for_test() 配合 MockPtOps。
    pub(crate) fn into_normal(mut self) -> VmPageAllocator<Normal, RealPtOps> {
        let pt_region = PtRegion::from_reserved_with_ops(
            &mut self.reserved,
            self.phys_alloc.unwrap(),
            self.pt_ops.unwrap(),
        );
        VmPageAllocator {
            reserved: self.reserved,
            phys_alloc: None,
            pt_region: Some(pt_region),
            pt_ops: None,
            _stage: PhantomData,
        }
    }
}
```

**Normal 阶段**：

```rust
impl<O: PtOps> VmPageAllocator<Normal, O> {
    pub(crate) fn alloc_phys(&mut self, clicks: usize, flags: PageAllocFlags)
        -> Result<PhysBytes, AllocError> { ... }

    pub(crate) fn alloc_virt(&mut self, phys: PhysBytes, clicks: usize)
        -> Option<VirBytes> { ... }

    pub(crate) fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
        let phys = self.alloc_phys(1, PageAllocFlags::empty()).ok()?;
        let virt = self.alloc_virt(phys, 1)?;
        Some((virt, phys))
    }
}
```

**关键设计点**：

- `phys_alloc` 用 `Option` 包裹：Bootstrap 阶段持有 `Some`，`into_normal()` 将其所有权转移给 `PtRegion`，Normal 阶段变为 `None`。编译器保证 Normal 阶段不可能再通过 `phys_alloc` 直接分配。
- `pt_ops` 同样用 `Option` 包裹：Bootstrap 阶段持有 `Some(RealPtOps)`，`into_normal()` 转移给 `PtRegion`。
- `PtOps` trait 抽象了页表读写操作，使得测试可以用 `MockPtOps` 替代 `RealPtOps`，避免测试中访问非法内存地址。

**三种方案对比**：

| 维度 | BSS 静态分配 | 预留区域 + 标志 | Typestate |
|------|-------------|----------------|-----------|
| 内存来源 | 编译期 BSS | kernel 预留 | kernel 预留 |
| 阶段区分 | 运行时 `pt_init_done` | 运行时 `stage` 枚举 | 编译期类型参数 |
| Normal 阶段能否访问 reserved | 能（BSS 永远存在） | 能（字段仍存在） | **不能**（所有权已转移） |
| 阶段切换后旧状态 | 仍可访问 | 仍可访问 | **被消耗，不可访问** |
| 编译器保证 | 无 | 无 | **有** |
| 代码复杂度 | 低 | 中 | 中 |

**32/64 位架构差异**：

| 维度 | Minix3 (32-bit x86) | minix-rs (64-bit x86_64) |
|------|---------------------|--------------------------|
| 页表层级 | 2 级（PD → PT） | 4 级（PML4 → PDPT → PD → PT） |
| 虚拟地址宽度 | 32 位（4GB） | 48 位（256TB） |
| 页表项大小 | 4 字节 | 8 字节 |
| 单页页表项数 | 1024 | 512 |
| 单 PT 覆盖空间 | 4MB | 2MB |
| 单 PD 覆盖空间 | 4GB | 1GB |
| 备用页方案 | BSS 静态分配 190 页 | 内核预留区域 + Typestate |
| 递归保护 | `level` 计数器 + 备用页池 | PtRegion 消除递归根源 |
| 地址空间压力 | 高（4GB 需精心规划） | 低（256TB 充足） |

**为什么选择 Typestate？**

1. **编译期安全**：Normal 阶段不可能访问 `reserved` 和独立的 `phys_alloc`，编译器阻止了整类 bug。
2. **自文档化**：函数签名 `fn foo(alloc: &VmPageAllocator<Normal>)` 明确表达了"此函数仅在 Normal 阶段调用"。
3. **零运行时开销**：`PhantomData` 是零大小类型，编译后完全消除。
4. **符合 Rust 惯例**：`Builder` 模式、状态机等场景广泛使用 Typestate。

### 3.4 递归的结构性消除：PtRegion

Typestate 解决了阶段管理问题，但没有解决递归问题。回顾 §2.4 的分析：递归的根源是页表页的虚拟地址需要走 `find_hole + vm_mappages`。

**Minix3 的 spare_pagequeue 方案的问题**：备用页池是固定大小的。如果递归频繁发生（每次 `vm_mappages` 落入新的 2MB 区域），池会耗尽。

**更根本的解法**：为页表页预留一段**专用的虚拟地址空间**，用 bump allocator 按序分配。页表页的虚拟地址不再走 `find_hole + vm_mappages`，递归从结构上被消除。

```
VM 虚拟地址空间:
  0x7F0000000000  ┬── 页表页专用区域 (初始 2MB, 可增长)
                  │   ┌── PDPT page (4KB)     ← 占用 1 个 slot
                  │   ├── PD page   (4KB)     ← 占用 1 个 slot
                  │   ├── PT[0]     (4KB)     ← 占用 1 个 slot, 提供 512 slots
                  │   ├── slot 3: 可用
                  │   ├── ...
                  │   ├── slot 511: 可用
                  │   │
                  │   │  快用完时扩展:
                  │   ├── PT[1]     (4KB)     ← 占用 1 个 slot, 提供 512 slots
                  │   ├── ...
  0x7F0000200000  ┴── 区域结束
  0x7F0000200000  ─── 通用映射区域 (find_hole 从这里开始)
```

**容量分析**：

| 层级 | 条目数 | 覆盖的页表空间 | 够用吗 |
|------|--------|---------------|--------|
| 1 个 PT 页 | 512 slots | 2MB | 初始 |
| 1 个 PD 页 | 512 个 PT | 1GB | 远超需求 |
| 1 个 PDPT 页 | 512 个 PD | 512GB | 不可能用完 |

1GB 页表空间 = 262,144 个页表页。VM 自身映射只需几 MB，一个 PD 页的容量绰绰有余。

**PtRegion 起始地址选择**：

`0x7F0000000000` 的选择理由：
1. **避开内核空间**：x86_64 Linux 内核占用高地址（`0xFFFF...`），用户空间为 `0x0000_0000_0000` 到 `0x0000_7FFF_FFFF_FFFF`
2. **避开用户堆/栈**：用户堆从低地址向上增长，栈从高地址向下增长。`0x7F00...` 位于用户空间高位，与典型栈地址（`0x7FFF...`）有足够距离
3. **对齐友好**：`0x7F00_0000_0000` 是 1GB 对齐，便于页表计算
4. **可配置**：实际代码中应定义为常量 `PT_REGION_START`，便于调整

**扩展过程（关键：不递归）**：

```
扩展前: 只剩 8 个 free slots
  Region: [used.........................|free(8)]

扩展步骤:
  1. alloc_phys(1) → pt_phys           ← 纯 bitmap, 不递归
  2. 取 1 个 free slot → pt_virt       ← bump allocator, 不递归
  3. 写 PTE: current_pt[slot] = pt_phys | flags  ← current_pt 已映射, 不递归
  4. 清零新 PT 页 (通过 pt_virt)        ← 刚映射好, 不递归
  5. 写 PDE: pd[new_idx] = pt_phys | flags       ← pd 已映射, 不递归
  6. 切换 current_pt → 新 PT 页
  7. 新 PT[0] = pt_phys | flags        ← 自映射, 不递归
  8. mapped_end += 2MB

扩展后: 新 PT 页提供 511 个可用 slots
```

每一步操作的对象都是**已经映射好的页表页**，不需要 `ensure_tables`，不需要 `vm_mappages`。

**跨 2MB 边界时的处理**：当 current PT 的 512 个 slot 全部用完后，`pd_idx` 会递增（进入下一个 2MB 区域）。此时 `expand()` 不再向旧 PT 写 PTE（因为已满），而是直接写 PDE 建立新映射，并将新 PT 页设为 `current_pt`。新 PT 页的第一个 slot 自映射，保证后续的 `virt_to_phys` 能正确工作。

### 3.5 alloc_phys / alloc_virt 拆分

有了 PtRegion 消除递归后，`vm_allocpage` 可以干净地拆分为两个独立步骤：

```
alloc_phys(pages) → PhysBytes       // 纯物理页分配，不递归
alloc_virt(phys, pages) → VirBytes  // 从 PtRegion 分配虚拟地址
alloc_page() → (VirBytes, PhysBytes) // 组合上述两步
```

**递归检查清单**：

| 操作 | 需要什么 | 从哪来 | 触发 alloc_virt？ |
|------|----------|--------|-------------------|
| `alloc_phys` | 物理页 | PhysAllocator bitmap | ❌ |
| `alloc_virt` | 虚拟地址 | `pt_region.alloc_pt_page()` | ❌ bump allocator |
| `pt_region.expand()` → 新 PT 物理页 | 物理页 | `alloc_phys` | ❌ |
| `pt_region.expand()` → 写 PTE | 写已映射页表 | `current_pt`（已映射） | ❌ |
| `pt_region.expand()` → 写 PDE | 写已映射页表 | `pd_page`（已映射） | ❌ |

**全部 ❌。递归从结构上被消除。** 不需要固定大小的池、不需要担心耗尽、不需要区分"递归路径"和"正常路径"。

### 3.6 搬迁策略

初始化完成后，需要将 Bootstrap 阶段从预留区域分配的数据搬迁到堆上，然后释放预留区域。搬迁涉及指针追踪、引用更新、旧区域释放，复杂度足够独立成章。

详见 [09-vm-relocation.md](09-vm-relocation.md)。本文档仅关注页分配器本身的设计与实现。

---

## 4. 实现详解

### 4.1 PtOps：页表操作抽象

为了在测试中不访问真实内存地址，页表读写操作被抽象为 `PtOps` trait：

```rust
// os/servers/vm/src/pt_region.rs

pub(crate) trait PtOps {
    fn write_pte(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn write_pde(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn write_pdpte(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn read_pte(&self, table_virt: VirBytes, index: usize) -> u64;
    fn read_pde(&self, table_virt: VirBytes, index: usize) -> u64;
    fn zero_table(&mut self, table_virt: VirBytes);
}

pub(crate) struct RealPtOps;

impl PtOps for RealPtOps {
    fn write_pte(&mut self, table_virt: VirBytes, index: usize, entry: u64) {
        unsafe {
            let ptr = (table_virt.0 as *mut u64).add(index);
            ptr.write_volatile(entry);
        }
    }
    // ... 其他方法类似
}
```

生产环境使用 `RealPtOps`（直接写内存），测试使用 `MockPtOps`（写入 `BTreeMap<u64, [u64; 512]>`）。

### 4.2 ReservedRegion：内核预留区域

```rust
pub(crate) struct ReservedRegion {
    phys_start: PhysBytes,
    virt_start: VirBytes,
    total_pages: usize,
    allocated_pages: usize,
    high_watermark: usize,
    bitmap: u64,
}

impl ReservedRegion {
    pub(crate) fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
        let start = self.high_watermark;
        if start >= self.total_pages {
            return None;
        }

        let mask = !self.bitmap >> start;
        let rel_bit = mask.trailing_zeros() as usize;
        let free_bit = start + rel_bit;

        if free_bit >= self.total_pages {
            return None;
        }

        self.bitmap |= 1 << free_bit;
        self.allocated_pages += 1;

        let offset = free_bit * PAGE_SIZE;
        let virt = VirBytes(self.virt_start.0 + offset as u64);
        let phys = self.phys_start.add(offset);

        Some((virt, phys))
    }

    pub(crate) fn alloc_contig_virt(&mut self, pages: usize) -> VirBytes {
        assert!(pages <= self.total_pages - self.high_watermark);
        let offset = self.high_watermark * PAGE_SIZE;
        self.high_watermark += pages;
        VirBytes(self.virt_start.0 + offset as u64)
    }

    pub(crate) fn virt_to_phys(&self, virt: VirBytes) -> PhysBytes {
        let offset = (virt.0 - self.virt_start.0) as usize;
        self.phys_start.add(offset)
    }
}
```

`alloc_contig_virt` 用于在预留区域高水位标记之上切出连续虚拟地址（给 PtRegion 的 PDPT/PD/PT[0] 使用），它不修改 bitmap——这些页的物理内存由内核保证已映射。

**`high_watermark` 的设计意图**：

`high_watermark` 将预留区域分为两个区域：

1. **低地址区**（slot 0 ~ high_watermark-1）：由 `alloc_contig_virt` 线性切出，供 PtRegion 初始化使用。这些 slot 不经过 bitmap 分配，物理页由内核保证已映射。
2. **高地址区**（slot high_watermark ~ total_pages-1）：由 `alloc_page` 通过 bitmap 分配，供 Bootstrap 阶段的其他分配使用。

`alloc_page` 从 `high_watermark` 开始搜索空闲位（`let mask = !self.bitmap >> start`），确保 bitmap 分配不会与 `alloc_contig_virt` 的线性分配重叠。`alloc_contig_virt` 递增 `high_watermark`，将新切出的区域从 bitmap 可分配空间中排除。两个方法共享同一个 `high_watermark`，互不干扰。

### 4.3 PtRegion：页表页专用虚拟地址区域

```rust
pub(crate) struct PtRegion<O: PtOps> {
    start: VirBytes,
    mapped_end: VirBytes,
    next: VirBytes,
    pd_page: VirBytes,
    current_pt: VirBytes,
    current_pt_base: VirBytes,
    phys_alloc: Box<dyn PhysAllocator>,
    pt_ops: O,
}
```

**初始化**：从预留区域切出 3 页，建立 PDPT → PD → PT[0] 的初始映射链。

```rust
impl<O: PtOps> PtRegion<O> {
    pub(crate) fn from_reserved_with_ops(
        reserved: &mut ReservedRegion,
        phys_alloc: Box<dyn PhysAllocator>,
        pt_ops: O,
    ) -> Self {
        let start = reserved.alloc_contig_virt(3);
        let pd_page = VirBytes(start.0 + PAGE_SIZE as u64);
        let current_pt = VirBytes(start.0 + 2 * PAGE_SIZE as u64);

        let pdpt_phys = reserved.virt_to_phys(start);
        let pd_phys = reserved.virt_to_phys(pd_page);
        let pt0_phys = reserved.virt_to_phys(current_pt);

        let mut region = PtRegion {
            start,
            mapped_end: VirBytes(start.0 + 3 * PAGE_SIZE as u64),
            next: VirBytes(start.0 + 3 * PAGE_SIZE as u64),
            pd_page,
            current_pt,
            current_pt_base: start,
            phys_alloc,
            pt_ops,
        };

        region.pt_ops.write_pdpte(start, 0,
            pdpt_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64);
        region.pt_ops.write_pde(pd_page, 0,
            pd_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64);
        region.pt_ops.zero_table(current_pt);

        region
    }
}
```

**分配**：bump allocator，剩余不足 8 个 slot 时自动扩展。

```rust
pub(crate) fn alloc_pt_page(&mut self) -> Option<VirBytes> {
    if self.remaining() < 8 {
        self.expand()?;
    }
    let virt = self.next;
    self.next = VirBytes(self.next.0 + PAGE_SIZE as u64);
    Some(virt)
}
```

**扩展**：分配新物理页作为 PT 页，建立映射，切换 `current_pt`。

```rust
fn expand(&mut self) -> Option<()> {
    let pt_phys = self.phys_alloc.alloc_mem(1, PageAllocFlags::empty()).ok()?;

    let pt_virt = self.next;
    self.next = VirBytes(self.next.0 + PAGE_SIZE as u64);

    let pd_idx = (pt_virt.0 - self.start.0) as usize / (512 * PAGE_SIZE);
    let current_pd_idx = (self.current_pt.0 - self.start.0) as usize / (512 * PAGE_SIZE);

    if pd_idx == current_pd_idx {
        // 仍在同一 2MB 区域：在当前 PT 中写 PTE
        let pte_idx = (pt_virt.0 - self.current_pt.0) as usize / PAGE_SIZE;
        self.pt_ops.write_pte(self.current_pt, pte_idx,
            pt_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64);
    }

    // 写 PDE（新 2MB 区域时建立映射，同一区域时覆盖写入，幂等）
    self.pt_ops.write_pde(self.pd_page, pd_idx,
        pt_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64);

    self.pt_ops.zero_table(pt_virt);

        self.current_pt = pt_virt;
        self.current_pt_base = VirBytes(self.start.0 + (pd_idx * 512 * PAGE_SIZE) as u64);

        self.pt_ops.write_pte(pt_virt, 0,
        pt_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64);

    self.mapped_end = VirBytes(self.mapped_end.0 + 512 * PAGE_SIZE as u64);
    Some(())
}
```

### 4.4 VmPageAllocator：完整实现

完整代码见 [alloc_page.rs](os/servers/vm/src/alloc_page.rs) 和 [pt_region.rs](os/servers/vm/src/pt_region.rs)。

**初始化时序**：

```
T0: Kernel 启动 VM 进程
    ├── 创建初始页表（映射 .text, .rodata, .data, .bss）
    ├── 映射 reserved_region 到 VM 地址空间
    └── 传递 boot_info（含 reserved_region 描述）

T1: main() → init_vm()
    │
    ├── T2: PhysAllocator::init(mem_chunks)
    │       → 初始化物理页 bitmap
    │       → 此时 alloc_mem() 可用
    │       → 创建 Box<dyn PhysAllocator> 实例
    │
    ├── T3: VmPageAllocator::<Bootstrap>::new(reserved, phys_alloc)
    │       → phys_alloc 由 T2 创建，传入 new()
    │       → 此时 alloc_page() 从预留区域分配
    │
    ├── T4: pt_init()
    │       → 创建页表系统
    │
    ├── T5: allocator.into_normal()
    │       → 从 reserved 切出 PtRegion（3 页）
    │       → phys_alloc 所有权转移给 PtRegion
    │       → 此时 alloc_page() 走 alloc_phys + alloc_virt
    │
    ├── T6: relocate_to_heap()  [独立文档详述]
    │       → 搬迁预留区域数据到堆
    │       → 释放预留区域虚拟地址映射
    │
    └── init_vm() 返回
            → VM 堆完全可用

T7: 主循环开始
    → VM 正常运行
```

**时序说明**：
- T2 必须在 T3 之前，因为 `VmPageAllocator::new(reserved, phys_alloc)` 需要已初始化的 `phys_alloc`
- T5 中 `into_normal()` 将 `phys_alloc` 的所有权转移给 `PtRegion`，此后 `VmPageAllocator<Normal>` 不再直接持有 `phys_alloc`

---

## 5. 测试

### 5.1 测试策略概述

本模块采用分层测试策略，覆盖以下维度：

| 维度 | 测试重点 | 关键场景 |
|------|----------|----------|
| PtRegion 分配 | bump allocator 正确性、自动扩展 | 初始容量、跨 2MB 边界、扩展后 slot 可用 |
| Typestate 切换 | 编译期阶段隔离、所有权转移 | Bootstrap → Normal、Normal 无法访问 reserved |
| alloc_phys/alloc_virt 拆分 | 两步分配的正确组合 | 独立调用、组合调用、错误路径 |
| 预留区域耗尽 | 边界条件处理 | 分配超过 total_pages 时返回 None |
| 虚拟地址转换 | virt_to_phys 正确性 | 同一 PT 页内、跨 PT 页、自映射 slot |

### 5.2 PtRegion 分配与扩展

**测试目标**：验证 bump allocator 的基本分配和自动扩展机制。

**关键场景**：
1. 初始容量验证：`remaining()` 返回正确的可用 slot 数
2. 容量内分配：连续分配不超过初始容量，全部成功
3. 超容量分配：分配超过初始容量，触发 `expand()`，新 PT 页提供 511 个 slot
4. 跨 2MB 边界：当 `pd_idx` 递增时，`expand()` 正确建立新映射

**Mock 依赖**：`MockPtOps` 替代 `RealPtOps`，避免测试中访问非法内存地址。

### 5.3 Typestate 阶段切换

**测试目标**：验证 Bootstrap → Normal 切换的编译期安全保证。

**关键场景**：
1. Bootstrap 阶段分配：`alloc_page()` 从 `reserved` 分配，返回的虚拟地址在预留区域内
2. 切换操作：`into_normal()` 消耗 Bootstrap 实例，产生 Normal 实例
3. Normal 阶段分配：`alloc_page()` 走 `alloc_phys + alloc_virt`，返回的地址不在预留区域内
4. 编译期保证：Normal 实例无法访问 `reserved` 字段（编译错误）

**注意**：测试使用 `into_normal_for_test()` 配合 `MockPtOps`，生产代码使用 `into_normal()` 配合 `RealPtOps`。

### 5.4 alloc_phys / alloc_virt 拆分

**测试目标**：验证两步分配的正确组合。

**关键场景**：
1. 独立调用 `alloc_phys`：返回有效的物理地址
2. 独立调用 `alloc_virt`：给定物理地址，返回有效的虚拟地址
3. 组合调用 `alloc_page`：等价于 `alloc_phys + alloc_virt`
4. 错误路径：`alloc_phys` 失败时 `alloc_page` 返回 None

### 5.5 预留区域耗尽

**测试目标**：验证预留区域边界条件处理。

**关键场景**：
1. 创建只有 2 页的预留区域
2. 连续分配 2 页，全部成功
3. 第 3 次分配返回 None

**注意**：此测试使用 `RealPtOps`，因为不涉及页表操作。

### 5.6 虚拟地址转换

**测试目标**：验证 `virt_to_phys` 的正确性。

**关键场景**：
1. 同一 PT 页内的地址转换
2. 跨 PT 页的地址转换
3. 自映射 slot（PT[0]）的地址转换

**验证方法**：通过 `MockPtOps` 的内部 `BTreeMap` 验证 PTE 内容。

---

## 6. 执行模型与线程安全

### 6.1 单线程假设

VM 进程采用**单线程事件循环**模型（参见项目级 review 规范）。所有内存管理操作都在主线程中顺序执行，不存在并发访问。

**影响**：
- `VmPageAllocator` 及其内部组件（`ReservedRegion`、`PtRegion`）不需要 `Sync` 或 `Send` trait
- 不需要互斥锁、原子操作或内存屏障
- `level` 变量（Minix3 中用于检测递归）在 Rust 版本中被 Typestate 模式替代，编译期保证

### 6.2 与 Minix3 的对比

| 维度 | Minix3 | minix-rs |
|------|--------|----------|
| 执行模型 | 单线程事件循环 | 单线程事件循环（保持） |
| 递归检测 | 运行时 `level` 计数器 | 编译期 Typestate |
| 并发保护 | 无（单线程） | 无（单线程，保持） |
| 可重入性 | 通过 `level` 限制 | 通过类型系统禁止 |

---

## 7. 参见

- [04-physical-memory.md](04-physical-memory.md) - 物理页分配器（`alloc_mem` / `free_mem`）
- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构（`pt_t`）
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作（`pt_init` / `pt_new`）
- [08-slab-allocator.md](08-slab-allocator.md) - Slab 分配器（全局 allocator）
- [09-vm-relocation.md](09-vm-relocation.md) - 数据搬迁（预留区域 → 堆）

---

*分类: VM库 | 使用范围: 仅 VM 内部*
