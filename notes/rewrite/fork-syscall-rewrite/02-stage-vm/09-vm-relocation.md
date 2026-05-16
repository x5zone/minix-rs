# 09-vm-relocation: VM 自举的终点

> **分类**: VM私有
> **源码**: [alloc.c](minix3/minix/servers/vm/alloc.c)、[pagetable.c](minix3/minix/servers/vm/pagetable.c)
> **说明**: Minix3 的初始化数据搬迁机制分析，以及 Rust 版本的搬迁设计

> **里程碑定位**：09 是 VM 自举的终点。从 04 到 09，VM 完成了从"依赖 BSS 静态分配"到"完全自主管理全部物理内存"的演进：
>
> | 文档 | VM 获得的能力 | 依赖前提 |
> |------|-------------|----------|
> | 04 | 物理内存管理（bitmap allocator） | BSS 静态数组存储元数据 |
> | 05 | 页分配（alloc_phys + vm_phys_to_virt） | bitmap allocator |
> | 06 | 初始页表（4 页 + 1GB direct map） | kernel 建立 |
> | 07 | 页表操作（双视图模型） | direct map |
> | 08 | 堆分配（GlobalAlloc 对接 HeapArena） | alloc crate 可用 |
> | **09** | **搬迁：从静态分配切换到动态分配** | **堆分配器 + 页表操作可用** |
>
> **09 之前**：页表结构和 spare page 池依赖 BSS 静态分配（Minix3），或元数据依赖 BumpBuf 连续 PA 约束（Rust 版本）
> **09 之后**：所有 VM 内部数据结构由动态分配支撑，不再依赖静态资源
>
> 09 的三个阶段对应 VM 自举的三次状态转换：
> - **Phase 1**（自举阶段）：Minix3 使用 BSS 静态数组，Rust 版本使用 BumpBuf——这是**临时方案**，有硬性约束
> - **Phase 2**（搬迁）：从静态/BumpBuf 切换到动态分配——这是**约束消除**，搬迁的核心动机
> - **Phase 3**（可选升级）：分配器策略选择（bitmap → buddy 等）——这是**策略选择**，非必需步骤
>
> 自举完成后，VM 不再依赖内核的特殊支持。

---

## 1. 基本概念

### 1.1 什么是初始化数据搬迁

在操作系统启动过程中，VM（Virtual Memory）服务器需要管理物理内存。但 VM 自身在初始化阶段也面临"先有鸡还是先有蛋"的问题：

- **物理内存管理器需要元数据**（如 bitmap）来跟踪哪些物理页是空闲的。
- **这些元数据本身也需要内存来存储**。
- **在 VM 的页表和堆分配器就绪之前**，VM 无法像普通进程一样从自己的堆中分配内存。

因此，Minix3 采用了一个**两阶段初始化**策略：

1. **自举阶段**：VM 使用 BSS 段中的静态数组（`free_pages_bitmap`、`free_page_cache`、`static_sparepages`）存储元数据。这些数组编译进 VM 的 ELF，由内核在加载时映射。此时 VM 不需要动态分配——所有元数据已在 BSS 中。
2. **运行阶段**：`pt_init()` 完成后（`pt_init_done = 1`），VM 可以通过 `alloc_mem()` + `vm_mappages()` 动态分配内存。页表结构和备用页池从静态分配切换到动态分配。

**搬迁**就是指：在 `pt_init()` 中，将页表结构和备用页池从 BSS 静态分配切换到动态分配。搬迁完成后，VM 的页表基础设施完全由动态内存支撑，不再依赖 BSS 中的静态资源。

注意：Minix3 中 `free_pages_bitmap` 和 `free_page_cache` **不需要搬迁**——它们一直在 BSS 中，大小固定但位置永久。真正需要搬迁的是**页表结构**和 **spare page 池**（Rust 版本的搬迁对象不同，见 §3.0）。

| | 自举阶段（BSS 静态数组） | 运行阶段（动态分配） |
|--|----------------------|------------------|
| **搬迁** | spare pages, 页表结构 | spare pages, 页表结构 |
| **不搬迁** | bitmap[], page_cache[] | bitmap[], page_cache[] |

**搬迁不是简单的 memcpy**。Minix3 的搬迁通过"重新分配 + 复制内容 + 替换结构"实现——分配新的动态页，复制旧内容，然后用新结构替换旧结构（详见 §2.9）。

### 1.2 为什么需要搬迁

Minix3 的搬迁动机有两个：

**1. BSS 大小固定，无法扩展**

`free_pages_bitmap` 和 `free_page_cache` 是 BSS 中的静态数组，大小在编译时确定。`free_pages_bitmap` 固定为 128KB（对应 4GB 地址空间），`free_page_cache` 固定为 10000 项。如果物理内存超过 4GB，bitmap 无法扩展；如果缓存需求超过 10000 项，缓存无法增长。

但 `free_pages_bitmap` 和 `free_page_cache` 本身不需要搬迁——它们在 BSS 中的位置是永久的。真正受固定大小限制的是**spare page 池**和**页表结构**：静态 spare page 池只有 15 页（i386），页表结构在初始化时用静态页构建，无法动态扩展。

**2. liveupdate 导致物理地址变化**

Minix3 支持 liveupdate——在系统运行时更新 VM 的代码和数据。liveupdate 会重新分配 VM 的物理内存，导致 BSS 中 `static_sparepages` 的物理地址失效。搬迁将 spare page 池从静态 BSS 切换到动态分配，确保 liveupdate 后 spare page 的物理地址仍然有效。

```
搬迁前: BSS 静态 spare pages（物理地址由内核在加载时决定，liveupdate 后失效）
搬迁后: 动态分配的 spare pages（物理地址由 VM 自行分配，liveupdate 后仍有效）
```

搬迁的核心价值：让 VM 的页表基础设施不再依赖 BSS 中的静态资源，从而支持 liveupdate 和动态扩展。

### 1.3 自举阶段依赖的静态数据

在 Minix3 中，Bootstrap 阶段依赖的静态数据主要包括：

| 数据 | 位置 | 大小 | 用途 |
|------|------|------|------|
| `free_pages_bitmap[]` | BSS | 固定（对应 4GB） | 物理页空闲位图 |
| `free_page_cache[]` | BSS | 固定（10000 项） | 单页分配缓存 |
| `static_sparepages[]` | BSS | 15 页（i386） | 初始化阶段的备用页池 |
| `sparepagedirs[]` | BSS | 1 项（arm） | 初始化阶段的备用页目录池 |
| `pagemap[]`（SANITYCHECKS） | BSS | 对应全部物理页 | 调试用的页使用追踪 |

**关键特征**：
- `free_pages_bitmap` 和 `free_page_cache` 是**扁平数组**（SoA，Structure of Arrays），没有指针、没有嵌套结构、没有链表。
- `static_sparepages` 是**物理页数组**，每个元素是一页物理内存，由内核在加载 ELF 时分配并映射到 VM 的虚拟地址空间。
- 搬迁时，`free_pages_bitmap` 和 `free_page_cache` 不需要搬迁（它们一直在 BSS 中，只是大小固定）。真正需要搬迁的是**页表结构**和**spare page 池**——从静态分配切换到动态分配。

### 1.4 搬迁在初始化时序中的位置

#### Minix3 时序

```
T0: Kernel 启动 VM 进程
    ├── 加载 VM 的 ELF，映射 BSS 段（含 static_sparepages）
    └── 传递 boot_info

T1: main() → init_vm()
    │
    ├── T2: get_mem_chunks(mem_chunks)
    │       → 从 kernel_boot_info 解析内存布局
    │
    ├── T3: mem_init(mem_chunks)
    │       → 初始化 free_pages_bitmap（BSS 中，无需分配）
    │       → 标记所有可用物理页为空闲
    │
    ├── T4: pt_init()
    │       → 创建 VM 自己的页表
    │       → 分配 spare page 池（从 static_sparepages）
    │       → pt_init_done = 1
    │       → 【隐式搬迁开始】
    │       → 用光所有静态 spare page
    │       → 重新分配页表（动态内存）
    │       → 复制页表内容
    │       → 丢弃旧页表
    │       → 【隐式搬迁结束】
    │
    └── init_vm() 返回

T5: 主循环开始，所有分配走动态路径
```

Minix3 的搬迁是"隐式的"——在 `pt_init()` 中悄悄完成，没有显式的搬迁函数。搬迁发生在 `pt_init_done = 1` 之后，此时 VM 已经可以动态分配内存，但仍然在使用 BSS 中的静态 spare page。搬迁将这些静态资源替换为动态分配的资源。

### 1.5 核心边界条件

**分配失败**：
- Minix3 Bootstrap 阶段：如果静态 spare page 池耗尽（如 `static_sparepages` 的 15 页用完），`vm_allocpage()` 返回 NULL，可能导致 panic。
- Minix3 Normal 阶段：如果堆分配失败（`alloc_mem()` 返回 `NO_MEM`），系统无法继续运行，通常 panic。

**内存对齐**：
- `alloc_mem()` 支持 `PAF_ALIGN64K` 和 `PAF_ALIGN16K` 标志。对齐分配时，先分配比请求更大的块，然后释放头部未对齐的部分。这要求空闲位图能够正确标记部分释放的页。

**并发安全**：
- Minix3 的 VM 是单线程的（一个进程，一个消息循环），因此分配器不需要考虑并发竞争。所有操作都是同步的、不可中断的。

**物理内存限制**：
- `PAF_LOWER16MB` 和 `PAF_LOWER1MB` 限制分配只能在低地址区域。这在 DMA 设备需要物理连续内存时使用。

### 1.6 BSS 静态分配的约束

Minix3 的自举阶段依赖 BSS 静态数组存储元数据。这种方案有两个根本性约束：

**1. 大小固定，无法扩展**

BSS 数组在编译时确定大小，运行时无法增长。`free_pages_bitmap` 固定 128KB（对应 4GB 地址空间），`free_page_cache` 固定 10000 项，`static_sparepages` 固定 15 页。如果物理内存超过 4GB，bitmap 无法扩展；如果缓存需求超过 10000 项，缓存无法增长。

**2. 物理地址不可控**

BSS 段的虚拟地址由链接器决定，物理地址由内核在加载 VM 的 ELF 时决定。VM 无法选择 BSS 数据的物理位置。这对 liveupdate 构成问题——liveupdate 会重新分配 VM 的物理内存，导致 BSS 中数据的物理地址失效。

这两个约束是搬迁的根本动机（详见 §1.2）。Rust 版本的自举方案不同（使用 BumpBuf 而非 BSS），但面临类似的约束——详见 §3.0 的对比分析。

---

## 2. Minix3 C 源码分析

### 2.1 物理内存管理器的核心数据结构

Minix3 的物理内存管理器位于 `minix3/minix/servers/vm/alloc.c`。核心数据结构（`free_pages_bitmap[]`、`free_page_cache[]`、位图操作宏）的完整定义和逐行解析见 [04-physical-memory.md §2.0-2.0.3](04-physical-memory.md)。

**搬迁关联**：`free_pages_bitmap` 和 `free_page_cache` 是 BSS 中的静态数组，在 Minix3 中**不需要搬迁**（大小固定但位置永久）。Rust 版本中它们来自 BumpBuf 分配，需要搬迁（见 §3.0）。

### 2.2 初始化流程：`mem_init()`

`mem_init()` 从 `kernel_boot_info` 解析内存布局，将可用物理页标记为空闲。完整代码和逐行解析见 [04-physical-memory.md §3.1.5](04-physical-memory.md)。

**搬迁关联**：`mem_init()` 初始化的 `free_pages_bitmap` 在 Minix3 中是 BSS 静态数组，不涉及搬迁。Rust 版本的 `BitmapAllocator::init()` 从 BumpBuf 分配 bitmap 内存，搬迁时需要迁移。

### 2.3 分配流程：`alloc_mem()`

`alloc_mem()` 从空闲位图中分配指定大小的连续物理页块。完整代码和逐行解析见 [04-physical-memory.md §2.1.1](04-physical-memory.md)。

**搬迁关联**：搬迁完成后，`alloc_mem()` 仍使用同一个 `free_pages_bitmap`（只是 bitmap 的内存位置从 BumpBuf 变为 HeapArena）。搬迁不影响 `alloc_mem()` 的逻辑。

### 2.4 底层分配：`alloc_pages()`

`alloc_pages()` 从位图中分配指定数量的连续页，支持对齐标志。完整代码和逐行解析见 [04-physical-memory.md §2.1.2](04-physical-memory.md)。

**搬迁关联**：同 `alloc_mem()`，搬迁不影响 `alloc_pages()` 的逻辑。

### 2.5 位图扫描：`findbit()`

`findbit()` 在位图中从指定位置向前扫描，找到第一个满足条件的连续空闲块。完整代码和逐行解析见 [04-physical-memory.md §2.1.2](04-physical-memory.md)。

**搬迁关联**：无直接关联。

### 2.6 释放流程：`free_mem()` 和 `free_pages()`

`free_mem()` 将连续物理页块标记为空闲，`free_pages()` 释放单页。完整代码和逐行解析见 [04-physical-memory.md §2.2.1](04-physical-memory.md)。

**搬迁关联**：搬迁完成后，`relocate()` 调用 `free_mem()` 释放旧 PA 页（BumpBuf 占据的连续物理页）。这是搬迁的核心收益——释放稀缺的连续 PA 资源。

### 2.7 备用页池：`spare_pagequeue` 与 `reservedqueue`

Minix3 的页表操作（如 `pt_ptalloc()`）需要分配物理页来存储页表。但 `vm_allocpage()` 在初始化阶段（`pt_init_done == 0`）不能调用 `alloc_mem()`，因为物理内存管理器可能还没完全就绪。为了解决这个递归依赖，Minix3 引入了**备用页池（spare page pool）**。

```c
// [pagetable.c:59-109](minix3/minix/servers/vm/pagetable.c#L59-L109)

#if SANITYCHECKS
#define SPAREPAGES 200
#define STATIC_SPAREPAGES 190
#else
#ifdef __arm__
# define SPAREPAGES 150
# define STATIC_SPAREPAGES 140
#else
# define SPAREPAGES 20
# define STATIC_SPAREPAGES 15
#endif
#endif

static void *spare_pagequeue;
static char static_sparepages[VM_PAGE_SIZE*STATIC_SPAREPAGES]
	__aligned(VM_PAGE_SIZE);
```

- **`SPAREPAGES`**：备用页池的总容量。i386 上为 20，arm 上为 150，开启 SANITYCHECKS 时为 200。
- **`STATIC_SPAREPAGES`**：静态备用页的数量。i386 上为 15，arm 上为 140。这些页在 BSS 段中，由内核预先映射。
- **`spare_pagequeue`**：指向 `reservedqueue` 结构的指针，管理备用页池。
- **`static_sparepages[]`**：静态数组，存储备用页的虚拟地址。`__aligned(VM_PAGE_SIZE)` 确保数组页对齐。

**`reservedqueue` 结构**：

```c
// [alloc.c:60-72](minix3/minix/servers/vm/alloc.c#L60-L72)

static struct reserved_pages {
	struct reserved_pages *next;	/* next in use */
	int max_available;	/* queue depth use, 0 if not in use at all */
	int npages;		/* number of consecutive pages */
	int mappedin;		/* must reserved pages also be mapped? */
	int n_available;	/* number of queue entries */
	int allocflags;		/* allocflags for alloc_mem */
	struct reserved_pageslot {
		phys_bytes	phys;
		void		*vir;
	} slots[MAXRESERVEDPAGES];
	u32_t magic;
} reservedqueues[MAXRESERVEDQUEUES], *first_reserved_inuse = NULL;
```

- **`reservedqueues[]`**：静态数组，最多 15 个队列。每个队列管理一类备用页。
- **`max_available`**：队列的最大容量。
- **`npages`**：每个 slot 占用的连续页数（spare page 为 1）。
- **`mappedin`**：是否需要虚拟地址映射（spare page 需要，因为 VM 需要访问它们）。
- **`n_available`**：当前可用的 slot 数。
- **`slots[]`**：每个 slot 记录物理地址和虚拟地址。
- **`magic`**：魔数 `0x6e4c74d5`，用于 sanity check。

**reservedqueue 操作函数**：

```c
// [alloc.c:74](minix3/minix/servers/vm/alloc.c#L74)

int missing_spares = 0;
```

`missing_spares` 是全局计数器，记录所有 reservedqueue 中尚未填充的 slot 总数。每次 `reservedqueue_new()` 时增加 `max_available`，每次 `reservedqueue_fillslot()` 时递减，每次 `reservedqueue_alloc()` 时递增。`alloc_cycle()` 根据 `missing_spares > 0` 决定是否需要填充。

```c
// [alloc.c:136-146](minix3/minix/servers/vm/alloc.c#L136-L146)

static void
reservedqueue_fillslot(struct reserved_pages *rq,
	struct reserved_pageslot *rps, phys_bytes ph, void *vir)
{
	rps->phys = ph;
	rps->vir = vir;
	assert(missing_spares > 0);
	if(rq->mappedin) assert(vir);
	missing_spares--;
	rq->n_available++;
}
```

`reservedqueue_fillslot()` 是底层填充函数，将一个物理地址+虚拟地址对写入 slot，递减 `missing_spares`，递增 `n_available`。如果队列要求映射（`mappedin`），断言虚拟地址非空。

```c
// [alloc.c:148-177](minix3/minix/servers/vm/alloc.c#L148-L177)

static int
reservedqueue_addslot(struct reserved_pages *rq)
{
	phys_bytes cl, cl_addr;
	void *vir;
	struct reserved_pageslot *rps;

	sanitycheck_rq(rq);

	if((cl = alloc_mem(rq->npages, rq->allocflags)) == NO_MEM)
		return ENOMEM;

	cl_addr = CLICK2ABS(cl);

	vir = NULL;

	if(rq->mappedin) {
		if(!(vir = vm_mappages(cl_addr, rq->npages))) {
			free_mem(cl, rq->npages);
			printf("reservedqueue_addslot: vm_mappages failed\n");
			return ENOMEM;
		}
	}

	rps = &rq->slots[rq->n_available];

	reservedqueue_fillslot(rq, rps, cl_addr, vir);

	return OK;
}
```

`reservedqueue_addslot()` 动态分配一个 slot：调用 `alloc_mem()` 分配物理页，如果需要映射则调用 `vm_mappages()` 分配虚拟地址，最后通过 `reservedqueue_fillslot()` 填入队列。如果 `vm_mappages()` 失败，会先释放已分配的物理页再返回错误。

```c
// [alloc.c:191-203](minix3/minix/servers/vm/alloc.c#L191-L203)

static int reservedqueue_fill(void *rq_v)
{
	struct reserved_pages *rq = rq_v;
	int r;

	sanitycheck_rq(rq);

	while(rq->n_available < rq->max_available)
		if((r=reservedqueue_addslot(rq)) != OK)
			return r;

	return OK;
}
```

`reservedqueue_fill()` 循环调用 `reservedqueue_addslot()` 直到队列填满或分配失败。

```c
// [alloc.c:206-225](minix3/minix/servers/vm/alloc.c#L206-L225)

int
reservedqueue_alloc(void *rq_v, phys_bytes *ph, void **vir)
{
	struct reserved_pages *rq = rq_v;
	struct reserved_pageslot *rps;

	sanitycheck_rq(rq);

	if(rq->n_available < 1) return ENOMEM;

	rq->n_available--;
	missing_spares++;
	rps = &rq->slots[rq->n_available];

	*ph = rps->phys;
	*vir = rps->vir;

	sanitycheck_rq(rq);

	return OK;
}
```

`reservedqueue_alloc()` 从队列尾部取出一个 slot：递减 `n_available`，递增 `missing_spares`（因为该 slot 变为空缺），返回物理地址和虚拟地址。LIFO 顺序——最后填入的 slot 最先被取出。

```c
// [alloc.c:227-237](minix3/minix/servers/vm/alloc.c#L227-L237)

void alloc_cycle(void)
{
	struct reserved_pages *rq;
	sanitycheck_queues();
	for(rq = first_reserved_inuse; rq && missing_spares > 0; rq = rq->next) {
		sanitycheck_rq(rq);
		reservedqueue_fill(rq);
		sanitycheck_rq(rq);
	}
	sanitycheck_queues();
}
```

`alloc_cycle()` 遍历所有活跃的 reservedqueue，对每个有空缺的队列调用 `reservedqueue_fill()` 填充。这是 `pt_init()` 搬迁阶段的核心函数——先用光静态页（`missing_spares` 增加），再调用 `alloc_cycle()` 用动态页填充空缺。

**`pt_init()` 中的备用页池初始化**：

```c
// [pagetable.c:1116-1162](minix3/minix/servers/vm/pagetable.c#L1116-L1162)

/* Get ourselves spare pages. */
sparepages_mem = (vir_bytes) static_sparepages;
assert(!(sparepages_mem % VM_PAGE_SIZE));

if(!(spare_pagequeue = reservedqueue_new(SPAREPAGES, 1, 1, 0)))
	panic("reservedqueue_new for single pages failed");

assert(STATIC_SPAREPAGES < SPAREPAGES);
for(s = 0; s < STATIC_SPAREPAGES; s++) {
	void *v = (void *) (sparepages_mem + s*VM_PAGE_SIZE);
	phys_bytes ph;
	if((r=sys_umap(SELF, VM_D, (vir_bytes) v,
                VM_PAGE_SIZE*SPAREPAGES, &ph)) != OK)
			panic("pt_init: sys_umap failed: %d", r);
	reservedqueue_add(spare_pagequeue, v, ph);
}
```

**逐行解析**：

- **`sparepages_mem = (vir_bytes) static_sparepages`**：获取静态备用页数组的虚拟地址。
- **`reservedqueue_new(SPAREPAGES, 1, 1, 0)`**：创建一个备用页队列，容量 20，每 slot 1 页，需要映射，无特殊分配标志。
- **`for(s = 0; s < STATIC_SPAREPAGES; s++)`**：遍历静态备用页。
- **`sys_umap(SELF, VM_D, (vir_bytes) v, VM_PAGE_SIZE*SPAREPAGES, &ph)`**：通过内核系统调用，查询虚拟地址 `v` 对应的物理地址 `ph`。`VM_D` 表示数据段。
- **`reservedqueue_add(spare_pagequeue, v, ph)`**：将虚拟地址和物理地址加入队列。

**注意**：这里只初始化了 `STATIC_SPAREPAGES`（15 页），而队列容量是 `SPAREPAGES`（20 页）。剩余的 5 页在 `alloc_cycle()` 中通过 `alloc_mem()` 动态分配并加入队列。

### 2.8 阶段切换：`pt_init_done`

```c
// [pagetable.c:328](minix3/minix/servers/vm/pagetable.c#L328)

static int pt_init_done;
```

`pt_init_done` 是一个静态整型标志，初始值为 0（C 语言静态变量默认初始化）。详细定义见 [05-vm-allocpage.md](05-vm-allocpage.md)。

```c
// [pagetable.c:333-393](minix3/minix/servers/vm/pagetable.c#L333-L393)

void *vm_allocpages(phys_bytes *phys, int reason, int pages)
{
	phys_bytes newpage;
	static int level = 0;
	void *ret;
	u32_t mem_flags = 0;

	assert(reason >= 0 && reason < VMP_CATEGORIES);
	assert(pages > 0);

	level++;

	assert(level >= 1);
	assert(level <= 2);

	if((level > 1) || !pt_init_done) {
		void *s;

		if(pages == 1) s=vm_getsparepage(phys);
		else if(pages == 4) s=vm_getsparepagedir(phys);
		else panic("%d pages", pages);

		level--;
		if(!s) {
			util_stacktrace();
			printf("VM: warning: out of spare pages\n");
		}
		if(!is_staticaddr(s)) vm_self_pages++;
		return s;
	}
	...
}
```

**逐行解析**：

- **`static int level = 0`**：递归深度计数器。`vm_allocpage()` 可能递归调用自身（例如 `pt_ptalloc()` 调用 `vm_allocpage()` 分配页表，而 `vm_allocpage()` 又需要映射页，映射页又需要页表...）。
- **`level++` 和 `assert(level <= 2)`**：限制递归深度最多为 2。如果超过，panic。
- **`if((level > 1) || !pt_init_done)`**：如果递归深度大于 1，或者 `pt_init_done == 0`，使用备用页池。
  - **`level > 1`**：表示当前处于递归路径。递归路径上的分配不能走正常路径（因为正常路径可能再次触发递归）。
  - **`!pt_init_done`**：表示初始化尚未完成。此时 VM 的页表和堆分配器还没就绪，不能调用 `alloc_mem()`。
- **`vm_getsparepage(phys)`**：从备用页队列取一个页。返回虚拟地址，并通过指针参数返回物理地址。
- **`is_staticaddr(s)`**：检查地址是否是静态地址（低于 `VM_OWN_HEAPSTART`）。如果是静态地址，不需要追踪（因为静态页不会被释放）；如果是动态地址，增加 `vm_self_pages` 计数。

**`pt_init_done = 1` 的位置**：

```c
// [pagetable.c:1311](minix3/minix/servers/vm/pagetable.c#L1311)

pt_init_done = 1;
```

这行代码位于 `pt_init()` 的"显式搬迁"之前。具体来说，`pt_init()` 的执行顺序是：

1. 初始化 spare page 池（静态页）。
2. 创建 VM 自己的页表（使用静态 spare page）。
3. 绑定页表到内核（`pt_bind`）。
4. `pt_init_done = 1`。
5. **显式搬迁**：用光静态页，重新分配动态页，复制页表。

这意味着 `pt_init_done = 1` 之后，VM 立即执行搬迁，将静态页替换为动态分配的页。

### 2.9 显式搬迁：`pt_init()` 的后半段

**关键函数说明**：

```c
// [pagetable.c:264-272](minix3/minix/servers/vm/pagetable.c#L264-L272)

static void *vm_getsparepage(phys_bytes *phys)
{
	void *ptr;
	if(reservedqueue_alloc(spare_pagequeue, phys, &ptr) != OK) {
		return NULL;
	}
	assert(ptr);
	return ptr;
}
```

`vm_getsparepage()` 从备用页队列取出一个页：调用 `reservedqueue_alloc()` 从 `spare_pagequeue` 尾部取出一个 slot，返回虚拟地址，通过指针参数返回物理地址。如果队列为空，返回 NULL。

```c
// [pagetable.c:235-259](minix3/minix/servers/vm/pagetable.c#L235-L259)

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

`vm_freepages()` 释放 VM 自身使用的页：先通过 `is_staticaddr()` 检查是否是静态地址（低于 `VM_OWN_HEAPSTART`），如果是则跳过释放（静态页不属于动态分配，无法回收）；否则调用 `pt_writemap()` 解除映射并释放物理页，递减 `vm_self_pages` 计数。

```c
// [pagetable.c:1313-1352](minix3/minix/servers/vm/pagetable.c#L1313-L1352)

/* VM is now fully functional in that it can dynamically allocate memory
 * for itself.
 *
 * We don't want to keep using the bootstrap statically allocated spare
 * pages though, as the physical addresses will change on liveupdate. So we
 * re-do part of the initialization now with purely dynamically allocated
 * memory. First throw out the static pool.
 *
 * Then allocate the kernel-shared-pagetables and VM pagetables with dynamic
 * memory.
 */

alloc_cycle();                          /* Make sure allocating works */
while(vm_getsparepage(&phys)) ;		/* Use up all static pages */
alloc_cycle();                          /* Refill spares with dynamic */
pt_allocate_kernel_mapped_pagetables(); /* Reallocate in-kernel pages */
pt_bind(newpt, &vmproc[VM_PROC_NR]);    /* Recalculate */
pt_mapkernel(newpt);                    /* Rewrite pagetable info */

/* Flush TLB just in case any of those mappings have been touched */
if((sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK) {
	panic("VMCTL_FLUSHTLB failed");
}

/* Recreate VM page table with dynamic-only allocations */
memset(&newpt_dyn, 0, sizeof(newpt_dyn));
pt_new(&newpt_dyn);
pt_copy(&newpt_dyn, newpt);
memcpy(newpt, &newpt_dyn, sizeof(*newpt));

pt_bind(newpt, &vmproc[VM_PROC_NR]);    /* Recalculate */
pt_mapkernel(newpt);                    /* Rewrite pagetable info */

/* Flush TLB just in case any of those mappings have been touched */
if((sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK) {
	panic("VMCTL_FLUSHTLB failed");
}
```

**逐行解析**：

- **`alloc_cycle()`**：触发备用页队列的填充。`alloc_cycle()` 遍历所有 `reservedqueue`，如果某个队列的可用 slot 数小于最大容量，调用 `reservedqueue_fill()` 动态分配新页并加入队列。这里的作用是确保备用页池中有动态分配的页。
- **`while(vm_getsparepage(&phys))`**：循环取出所有备用页，直到队列为空。这会耗尽静态 spare page（因为静态页在队列前面，动态页在后面）。但注意：`vm_getsparepage()` 只是取出页，并不释放它们。静态页无法被 `vm_freepages()` 释放（`is_staticaddr` 检查会跳过），所以它们只能被"用 up"——从队列中取出后不再归还。
- **`alloc_cycle()`**：再次触发填充。此时队列已空，`alloc_cycle()` 会通过 `alloc_mem()` 动态分配新页加入队列。这些新页是动态分配的，物理地址不会随 liveupdate 改变。
- **`pt_allocate_kernel_mapped_pagetables()`**：重新分配内核映射的页表。这些页表之前可能是用静态页分配的，现在用动态页重新分配。
- **`pt_bind(newpt, &vmproc[VM_PROC_NR])`**：将新的页表绑定到 VM 进程。`pt_bind()` 更新内核中的页目录映射，让内核知道 VM 的新页表位置。
- **`pt_mapkernel(newpt)`**：重新映射内核地址空间到新页表。
- **`sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)`**：刷新 TLB，确保新的页表映射生效。
- **`memset(&newpt_dyn, 0, sizeof(newpt_dyn))`**：清零一个新的页表结构。
- **`pt_new(&newpt_dyn)`**：用动态分配的内存创建一个新的页表根（页目录）。`pt_new()` 会调用 `vm_allocpage()` 分配页目录和页表，此时 `pt_init_done == 1`，所以走正常分配路径（`alloc_mem()` + `vm_mappages()`）。
- **`pt_copy(&newpt_dyn, newpt)`**：将旧页表的内容复制到新页表。`pt_copy()` 遍历所有 PDE，如果旧页表中有页表，在新页表中分配新的页表并复制内容。
- **`memcpy(newpt, &newpt_dyn, sizeof(*newpt))`**：用新页表结构覆盖旧页表结构。此时 `newpt` 指向的页表完全由动态分配的内存组成。
- **`pt_bind()` 和 `pt_mapkernel()` 再次执行**：更新内核的页表绑定和内核映射。
- **再次刷新 TLB**。

**搬迁的本质**：

Minix3 的搬迁不是显式的 `memcpy` + 更新指针，而是通过**重新分配 + 复制内容 + 替换结构**来实现的。具体来说：

1. **页目录**：`pt_new()` 分配新的页目录（动态页），`memcpy` 替换旧的页目录指针。
2. **页表**：`pt_copy()` 为每个 PDE 分配新的页表（动态页），复制旧页表的内容。
3. **spare page 池**：用光静态页，重新填充动态页。

这种设计的优点是**不需要更新所有指向旧结构的引用**——因为 `newpt` 是一个局部结构，`memcpy` 直接替换了它的内容，所有通过 `newpt` 访问的代码自动使用新结构。

**关键澄清**：这里的搬迁对象是**页表结构本身**（页目录和页表页），而不是 `free_pages_bitmap` 或 `free_page_cache`。`free_pages_bitmap` 和 `free_page_cache` 是 BSS 中的静态数组，它们在 VM 的整个生命周期中一直留在原地，不会被搬迁到堆上。文档 §1.3 中"搬迁涉及的数据"表格提到的这些数组，实际上在 Minix3 源码中并没有被搬迁。

### 2.10 虚拟地址分配：`findhole()`

`findhole()` 的功能概述见 [07-pagetable-ops.md](07-pagetable-ops.md)，本文分析其与搬迁相关的细节。

```c
// [pagetable.c:155-230](minix3/minix/servers/vm/pagetable.c#L155-L230)

static u32_t findhole(int pages)
{
	u32_t curv;
	int pde = 0, try_restart;
	static void *lastv = 0;
	pt_t *pt = &vmprocess->vm_pt;
	vir_bytes vmin, vmax;
	u32_t holev = NO_MEM;
	int holesize = -1;

	vmin = VM_OWN_MMAPBASE;
	vmax = VM_OWN_MMAPTOP;

	/* Input sanity check. */
	assert(vmin + VM_PAGE_SIZE >= vmin);
	assert(vmax >= vmin + VM_PAGE_SIZE);
	assert((vmin % VM_PAGE_SIZE) == 0);
	assert((vmax % VM_PAGE_SIZE) == 0);
	assert(pages > 0);

	curv = (u32_t) lastv;
	if(curv < vmin || curv >= vmax)
		curv = vmin;

	try_restart = 1;

	/* Start looking for a free page starting at vmin. */
	while(curv < vmax) {
		int pte;

		assert(curv >= vmin);
		assert(curv < vmax);

		pde = ARCH_VM_PDE(curv);
		pte = ARCH_VM_PTE(curv);

		if((pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT) &&
		   (pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT)) {
			/* there is a page here - so keep looking for holes */
			holev = NO_MEM;
			holesize = 0;
		} else {
			/* there is no page here - so we have a hole, a bigger
			 * one if we already had one
			 */
			if(holev == NO_MEM) {
				holev = curv;
				holesize = 1;
			} else holesize++;

			assert(holesize > 0);
			assert(holesize <= pages);

			/* if it's big enough, return it */
			if(holesize == pages) {
				lastv = (void*) (curv + VM_PAGE_SIZE);
				return holev;
			}
		}

		curv+=VM_PAGE_SIZE;

		/* if we reached the limit, start scanning from the beginning if
		 * we haven't looked there yet
		 */
		if(curv >= vmax && try_restart) {
			try_restart = 0;
			curv = vmin;
		}
	}

	printf("VM: out of virtual address space in vm\n");

	return NO_MEM;
}
```

**逐行解析**：

- **`vmin = VM_OWN_MMAPBASE`**：VM 自己的 mmap 基地址。定义为 `VM_OWN_HEAPBASE + 1024*1024*1024`（即堆基地址 + 1GB）。
- **`vmax = VM_OWN_MMAPTOP`**：VM 自己的 mmap 顶地址。定义为 `VM_OWN_MMAPBASE + 100 * 1024 * 1024`（即 100MB 的 mmap 区域）。
- **`assert(vmin + VM_PAGE_SIZE >= vmin)`**：检查溢出。如果 `vmin` 接近 0xFFFFFFFF，加 `VM_PAGE_SIZE` 会溢出。
- **`curv = (u32_t) lastv`**：从上次分配的地址之后开始扫描。`lastv` 是静态变量，初始为 0。
- **`try_restart = 1`**：允许从头开始扫描一次。
- **`pde = ARCH_VM_PDE(curv)`**：计算虚拟地址对应的页目录项索引。在 i386 上，`ARCH_VM_PDE` 是 `(addr >> 22)`，即取高 10 位。

> **架构演进**：以上为 Minix3 x86-32 设计。minix-rs 使用 x86-64，页表从 2 级变为 4 级（PML4+PDPT+PD+PT），地址划分从 10+10+12 变为 9+9+9+9+12，页表项从 u32 变为 u64，`pt_pt[1024]` 固定数组改为动态分配。详见 [06-pagetable-struct.md](06-pagetable-struct.md)。

- **`pte = ARCH_VM_PTE(curv)`**：计算页表项索引。在 i386 上，`ARCH_VM_PTE` 是 `((addr >> 12) & 0x3FF)`，即取中间 10 位。
- **`if((pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT) && (pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT))`**：检查该虚拟地址是否已有映射。
  - **`pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT`**：页目录项是否存在。
  - **`pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT`**：页表项是否存在。
  - 如果两者都存在，表示该地址已被占用，重置 `holev` 和 `holesize`。
- **`else { ... }`**：该地址未被占用，扩展空洞。
  - **`if(holev == NO_MEM)`**：新空洞的开始。
  - **`holesize == pages`**：空洞足够大，返回起始地址。
- **`curv += VM_PAGE_SIZE`**：检查下一个页。
- **`if(curv >= vmax && try_restart)`**：如果扫描到顶且未找到，从头开始再扫描一次。
- **`return NO_MEM`**：没有找到足够大的空洞，虚拟地址空间耗尽。

**为什么 mmap 区域只有 100MB？**

Minix3 的 VM 是一个用户态进程，它自己的地址空间也需要管理。`VM_OWN_MMAPBASE` 到 `VM_OWN_MMAPTOP` 是 VM 用于映射物理页的区域（如页表、缓存页等）。100MB 对于 VM 的内部使用来说足够，但如果 VM 需要管理大量缓存页，可能会耗尽。

### 2.11 页表映射：`vm_mappages()`

> **注意**: `vm_mappages` 的完整文档已移至 [07-pagetable-ops.md](07-pagetable-ops.md) §2.3.3。本文档仅引用其功能。

**核心功能**: 为物理页分配虚拟地址并建立页表映射。

**调用链**:
```
vm_allocpage()
  └── vm_mappages(phys, pages)
        ├── findhole(pages)        // 分配虚拟地址
        └── pt_writemap(...)      // 建立页表映射
```

**与递归问题的关系**: `vm_mappages` → `pt_writemap` → `pt_ptalloc()` → `vm_allocpage()` → `vm_mappages()` 形成递归。这是 05-vm-allocpage.md 解决的核心问题。

### 2.12 页表分配：`pt_ptalloc()`

`pt_ptalloc()` 的详细分析见 [07-pagetable-ops.md](07-pagetable-ops.md)，本文分析其与搬迁相关的防御性检查。

```c
// [pagetable.c:494-540](minix3/minix/servers/vm/pagetable.c#L494-L540)

static int pt_ptalloc(pt_t *pt, int pde, u32_t flags)
{
	int i;
	phys_bytes pt_phys;
	u32_t *p;

	/* Argument must make sense. */
	assert(pde >= 0 && pde < ARCH_VM_DIR_ENTRIES);
	assert(!(flags & ~(PTF_ALLFLAGS)));

	/* We don't expect to overwrite page directory entry, nor
	 * storage for the page table.
	 */
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

	for(i = 0; i < ARCH_VM_PT_ENTRIES; i++)
		pt->pt_pt[pde][i] = 0;	/* Empty entry. */

	/* Make page directory entry.
	 * The PDE is always 'present,' 'writable,' and 'user accessible,'
	 * relying on the PTE for protection.
	 */
#if defined(__i386__)
	pt->pt_dir[pde] = (pt_phys & ARCH_VM_ADDR_MASK) | flags
		| ARCH_VM_PDE_PRESENT | ARCH_VM_PTE_USER | ARCH_VM_PTE_RW;
#elif defined(__arm__)
	pt->pt_dir[pde] = (pt_phys & ARCH_VM_PDE_MASK)
		| ARCH_VM_PDE_PRESENT | ARM_VM_PDE_DOMAIN;
#endif

	return OK;
}
```

**逐行解析**：

- **`assert(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT))`**：断言该 PDE 尚未存在。`pt_ptalloc()` 不应该被调用来覆盖已有的页目录项。
- **`assert(!pt->pt_pt[pde])`**：断言该 PDE 对应的页表指针为空。
- **`vm_allocpage(&pt_phys, VMP_PAGETABLE)`**：分配一页物理内存用于存储页表。`VMP_PAGETABLE` 表示分配原因是页表。
- **`if (pt->pt_pt[pde])`**：这是一个**防御性检查**。`vm_allocpage()` 在内部可能调用 `pt_writemap()`，而 `pt_writemap()` 可能递归调用 `pt_ptalloc()`。如果递归路径上已经分配了该 PDE 的页表，那么 `pt->pt_pt[pde]` 会被设置。此时释放刚分配的页（避免泄漏），直接返回。
- **`pt->pt_pt[pde] = p`**：记录页表的虚拟地址。
- **`pt->pt_pt[pde][i] = 0`**：初始化所有页表项为 0（无效）。
- **`pt->pt_dir[pde] = ...`**：设置页目录项，指向页表的物理地址，并标记为存在、可写、用户态可访问。

**递归问题的根源**：

`pt_ptalloc()` → `vm_allocpage()` → `vm_mappages()` → `pt_writemap()` → `pt_ptalloc()`

这是一个潜在的无限递归。Minix3 通过以下机制解决：
1. **`level` 计数器**：`vm_allocpage()` 中的 `level` 限制递归深度为 2。
2. **`pt_init_done` 标志**：初始化阶段使用备用页池，不走 `alloc_mem()` + `vm_mappages()` 路径。
3. **`pt->pt_pt[pde]` 检查**：如果递归路径上已经分配了页表，直接复用。

### 2.13 内存统计：`memstats()`

`memstats()` 返回物理内存的使用统计信息。完整代码和逐行解析见 [04-physical-memory.md](04-physical-memory.md)。

**搬迁关联**：`memstats()` 在 64 位系统上不可行（`NUMBER_PHYSICAL_PAGES` 溢出），这是 Rust 版本使用动态分配 bitmap 的原因之一。

---

## 3. Rust 设计决策

### 3.0 Rust 版本的搬迁动机与 Minix3 的差异

§1.2 分析了 Minix3 的搬迁动机，§1.6 归纳了 BSS 静态分配的两个根本约束（大小固定 + 物理地址不可控）。Rust 版本的自举方案使用 BumpBuf 而非 BSS，但面临对应的约束——核心驱动力是**消除 BumpBuf 的连续 PA 约束**（对应 Minix3 的"物理地址不可控"约束）。

**与 Minix3 搬迁动机的对比**：

| 维度 | Minix3 | Rust 版本 |
|------|--------|----------|
| 搬迁对象 | 页表结构（spare page 池 + 页目录/页表页） | 分配器元数据（bitmap + page_cache） |
| 搬迁原因 | BSS 大小固定 + liveupdate 物理地址变化 | BumpBuf 连续 PA 约束 |
| 搬迁后效果 | 页表使用动态内存，spare page 池可扩展 | 连续 PA 页释放，元数据通过 HeapArena 访问 |
| 搬迁方式 | 重新分配 + 复制内容 + 替换结构 | HeapArena::grow + memcpy + update_ptrs + free_mem |

**Rust 版本的搬迁对象**：与 Minix3 不同，Rust 版本中**bitmap 和 page_cache 确实需要搬迁**。原因：Rust 版本没有 BSS 静态数组——bitmap 和 page_cache 的内存来自 BumpBuf（`free_regions[0]` 的 Direct Map 区域），这是自举阶段的临时分配。搬迁将它们从 BumpBuf（连续 PA 约束）迁移到 HeapArena（碎片化 PA + 连续 VA），释放连续 PA 页。Minix3 不需要搬迁 bitmap/page_cache 是因为它们在 BSS 中（大小固定但位置永久），而 Rust 版本的 BumpBuf 分配是临时的。

**BumpBuf 的连续 PA 约束**（BumpBuf 定义见 [04-physical-memory.md](04-physical-memory.md)，HeapArena 定义见 [08-slab-allocator.md](08-slab-allocator.md)）：自举阶段，元数据（bitmap + page_cache）从 `free_regions[0]` 的 Direct Map 区域分配。由于 `VA = PA + BASE`，VA 的连续性跟随 PA 的连续性——BumpBuf **强制要求连续物理页**。HeapArena 就位之前，VM 没有任何机制将碎片化的物理页缝合为连续 VA。

搬迁后，元数据迁移到 HeapArena（碎片化 PA + 连续 VA），原来的连续 PA 页被释放回分配器。这些连续 PA 页对 DMA 等需要连续物理内存的场景非常有价值——搬迁不仅消除了约束，还回收了稀缺资源。

**Direct Map 视角：搬迁被大幅简化**：在 Direct Map + HeapArena 方案下，搬迁的概念被大幅简化。早期使用 PtRegion 的搬迁逻辑是：分配 VA → 建立映射 → 复制 → 更新指针。Direct Map + HeapArena 简化为：`HeapArena::grow() → memcpy → update_relocated_arrays() → free_mem()`。关键区别：PtRegion 方案需要从 PtRegion 分配 VA 并建立映射（可能触发 `pt_ptalloc_in_range`），而 HeapArena 提供连续 VA（逐页映射碎片化 PA），物理页的 VA 分配步骤消失了。

**Rust 版本的初始化时序**：

```
T0: Kernel 启动 VM 进程
    ├── 建立初始页表（4 页 + 1GB direct map）
    ├── 映射 VM 代码/数据段
    └── 传递 boot_info（含物理内存范围 + 可用页列表）

T1: main() → VmServer::new()
    │
    ├── T2: Phase 1 — Bootstrap（临时方案，有硬性约束）
    │       → vm_phys_to_virt() 可用（前 1GB）
    │       → 读取 boot_info，初始化 bitmap allocator
    │       → bitmap 元数据从 free_regions[0] 的 BumpBuf 分配（连续 PA 约束）
    │       → alloc_phys() + vm_phys_to_virt() 可用
    │
    ├── T3: register_page_alloc() + init_vm_self_pt()
    │       → GlobalAlloc 可用（VmAllocator → HeapArena::grow）
    │       → vm_self_mappages() 可用
    │
    └── VmServer::new() 返回

T4: VmServer::relocate() — Phase 2 搬迁（约束消除）
    │
    ├── 1. 收集旧元数据信息（reloc_array_count + reloc_array_info）
    │      → 栈上固定大小数组，不依赖堆分配
    │
    ├── 2. HeapArena::grow() 分配新 VA 空间
    │      → 逐页分配物理页（可碎片化）+ vm_self_mappages() 映射到连续 VA
    │
    ├── 3. memcpy 旧数据到新位置
    │      → copy_nonoverlapping(old_ptr, new_va, bytes)
    │
    ├── 4. update_relocated_arrays() 更新 PhysAllocator 内部指针
    │      → bitmap 和 page_cache slice 指向 HeapArena VA
    │
    └── 5. free_mem() 释放旧 PA 页
           → 通过 metadata_pa_range() 获取旧 PA 范围
           → 连续 PA 页释放回分配器（可被 DMA 等场景使用）

T5: VmServer::init() — Phase 3（可选升级）
    → 如果策略决定：bitmap → buddy
    → buddy 元数据通过 HeapArena 分配
    → 无需再次搬迁——HeapArena 已提供连续 VA

T6: 主循环开始
    → VM 完全自主管理所有物理内存
    → 无代码通过 Direct Map VA 访问旧元数据位置
```

**关键区别**：Minix3 的搬迁是"隐式的"——在 `pt_init()` 中悄悄完成，没有显式的搬迁函数。Rust 版本的搬迁是"显式的"——`VmServer::relocate()` 作为独立方法，在 `new()` 之后、`init()` 之前调用。

**Rust 版本的边界条件**（详见 §3.2）：

- 自举阶段分配失败：如果 `free_regions[0]` 不足以容纳元数据，`BitmapAllocator::init()` 会 panic（debug_assert）。
- 搬迁失败：`relocate()` 静默返回，系统继续使用 BumpBuf 元数据（降级策略）。

### 3.1 搬迁策略选择

搬迁的核心问题是：**如何把数据从旧位置移到新位置，同时保证系统在搬迁期间的一致性**。有三种策略：

#### 策略一：原地升级（In-place Upgrade）

**思路**：不搬迁。直接把 BumpBuf 的内存"标记"为堆的一部分，PhysAllocator 的元数据永远留在原地。

```
搬迁前:  [ BumpBuf Region | ... ]
搬迁后:  [ BumpBuf Region | ... ]  ← 同一块内存，只是"身份"变了
          ↑
          └── 现在属于 VM Heap
```

**实现**：在 `relocate()` 时，不释放 BumpBuf 区域，而是将其虚拟地址范围注册到 HeapArena 中，标记为"已占用"。后续堆分配从 BumpBuf 区域之后开始。

**优点**：
- 零拷贝，最快
- 不需要更新任何引用
- 实现最简单

**缺点**：
- **连续 PA 约束无法消除**：BumpBuf 区域占据 free_regions[0] 开头的连续 PA 页，这些页无法释放回分配器
- BumpBuf 区域的位置由内核决定，可能不在 VM 期望的堆区域
- BumpBuf 区域大小有限（通常几 MB），堆的起始位置被"钉"在这里
- 如果 BumpBuf 区域在虚拟地址空间的中间，会造成地址空间碎片
- 语义不干净：BumpBuf 的物理页是内核分配的，VM 堆的物理页是 VM 自己分配的，混在一起管理复杂

#### 策略二：复制搬迁（Copy Relocation）

**思路**：在 HeapArena 上分配新内存，把数据复制过去，更新引用，释放旧内存。

```
搬迁前:
  BumpBuf (Direct Map):  [bitmap][page_cache]  ← 连续 PA，永久占用
  HeapArena:             [.......................................]

搬迁后:
  BumpBuf (Direct Map):  [free    ][free      ]  ← 连续 PA 释放回分配器
  HeapArena:             [bitmap][page_cache][...]
```

**实现步骤**：
1. 通过 HeapArena::grow() 分配新 VA 空间（逐页映射碎片化 PA）
2. memcpy 数据
3. 更新 PhysAllocator 内部指针指向新数组
4. 通过 metadata_pa_range() 获取旧 PA 范围，调用 free_mem() 释放

**优点**：
- **释放连续 PA 页**：BumpBuf 占据的连续 PA 页被释放回分配器，可被 DMA 等场景使用
- 堆的布局完全由 VM 控制
- BumpBuf 区域可以完全释放
- 语义清晰：Bootstrap 数据是"临时"的，Normal 数据是"永久"的

**缺点**：
- 需要一次完整拷贝
- 需要更新引用（但 SoA 结构使这很简单——只需更新几个指针）
- 搬迁期间 PhysAllocator 处于不一致状态（需要短暂暂停分配）

#### 策略三：双缓冲（Double Buffering）

**思路**：分配新数组，复制数据，然后**原子地**切换 PhysAllocator 的内部指针。搬迁期间，旧数组仍然可用。

```
搬迁前:
  PhysAllocator.bitmap ──► [Old Bitmap in Reserved Region]
  PhysAllocator.new_bitmap ──► NULL

搬迁中:
  PhysAllocator.bitmap ──► [Old Bitmap in Reserved Region]  ← 仍可用
  PhysAllocator.new_bitmap ──► [New Bitmap in Heap]          ← 已复制

切换后:
  PhysAllocator.bitmap ──► [New Bitmap in Heap]
  PhysAllocator.new_bitmap ──► NULL
  [Old Bitmap] 释放
```

**优点**：
- 搬迁期间 PhysAllocator 始终可用（旧指针仍有效）
- 切换是原子的（单条赋值语句）
- 如果搬迁失败，可以回滚（保留旧数组）

**缺点**：
- 短暂的双倍内存占用
- 实现复杂度最高
- 对于 SoA 扁平数组来说，过度设计

#### 选择：复制搬迁

对于当前场景，**复制搬迁**是最佳选择：

1. **SoA 结构使引用更新极简**：只需更新 2 个 slice 指针（bitmap + page_cache），不需要遍历对象图。
2. **搬迁窗口极短**：memcpy 几个数组只需要微秒级时间。搬迁期间暂停分配是可接受的（初始化阶段没有并发请求）。
3. **双缓冲过度设计**：双缓冲的价值在于"搬迁期间系统仍可用"，但初始化阶段没有其他线程竞争 PhysAllocator。
4. **原地升级无法消除连续 PA 约束**：BumpBuf 占据的连续 PA 页无法释放，对 DMA 等需要连续物理内存的场景是永久损失。
5. **搬迁后连续 PA 页可被 DMA 等场景使用**：释放的连续 PA 页是稀缺资源，搬迁回收了这些资源。

### 3.2 搬迁的边界条件

**搬迁期间 PhysAllocator 的一致性**：

搬迁期间，PhysAllocator 的内部指针指向旧数组。搬迁完成后，指针切换到新数组。在切换瞬间，PhysAllocator 处于不一致状态。解决方案：

- 搬迁在 `init_vm()` 中执行，此时还没有其他线程
- 搬迁期间不调用 `alloc_mem()` / `free_mem()`
- 搬迁是同步的、不可中断的

**搬迁失败的处理**：

如果堆分配失败（内存不足），搬迁无法完成。当前实现中 `relocate()` 在失败时静默返回（`Err(_) => return`），系统继续使用 BumpBuf 中的元数据——连续 PA 约束未被消除，但系统仍可正常运行。这是一种**降级策略**：搬迁是优化（释放连续 PA），不是必需（BumpBuf 元数据仍然可用）。

如果未来需要更严格的语义，可以改为 panic（初始化阶段，没有恢复的必要）或回退到原地升级（保留 BumpBuf 区域，标记为堆的一部分）。

**`HeapArena::grow()` 部分失败的资源安全**：

`relocate()` Phase 2 调用 `heap_arena_grow(pages, &mut self.page_alloc)`。如果 `HeapArena::grow()` 在分配部分物理页后失败（已映射 N 页但请求 M > N 页），已分配的物理页是否泄漏？

当前实现依赖 `HeapArena::grow()` 的**原子性保证**：要么全部成功（映射所有 M 页），要么不分配任何页（回滚已映射的 N 页）。如果 `HeapArena::grow()` 不提供此保证，`relocate()` 中的 `Err(_) => return` 会导致已映射的物理页永久泄漏——这些页既不在 HeapArena 的有效区域中，也没有被释放回分配器。

确保 `HeapArena::grow()` 的原子性是搬迁资源安全的前提。如果未来 `HeapArena::grow()` 的实现变为部分成功，`relocate()` 需要添加回滚逻辑：在 `Err` 分支中调用 `HeapArena` 的 shrink/rollback 接口释放已映射的页。

### 3.3 搬迁的调用时机

搬迁是 `VmServer::new()` 之后的独立步骤。`new()` 中已完成 `register_page_alloc()` + `init_vm_self_pt()`，HeapArena 和 vm_self_mappages() 均已可用：

```rust
pub fn new(...) -> Self { ... }     // 创建分配器 + 注册 GlobalAlloc + init_vm_self_pt
pub fn relocate(&mut self) { ... }  // 搬迁元数据：BumpBuf → HeapArena
pub fn init(&mut self) { ... }      // 正常初始化（搬迁后的状态）
```

`relocate()` 是独立方法，与 `init()` 分离。搬迁后 `init()` 中不再有连续 PA 约束。

---

## 4. 实现详解

### 4.1 搬迁接口

搬迁接口使用分步查询设计（`reloc_array_count()` + `reloc_array_info()` + `update_relocated_arrays()`），避免在 trait 方法中使用 Vec（保持 trait 不依赖 alloc crate）：

```rust
// os/servers/vm/src/phys_mem/alloc_trait.rs

pub trait PhysAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<AlignedPhysBytes, AllocError>;
    fn free_mem(&mut self, base: AlignedPhysBytes, clicks: usize);
    fn total_count(&self) -> usize;
    fn reserve_pages(&mut self, base_page: usize, count: usize);

    fn reloc_array_count(&self) -> usize { 0 }

    fn reloc_array_info(&self, _index: usize) -> (*const u8, usize, usize) {
        (core::ptr::null(), 0, 0)
    }

    fn update_relocated_arrays(&mut self, _new_ptrs: &[*mut u8]) {}
}
```

默认实现返回空——不支持搬迁的分配器（如 BuddyAllocator、测试 mock）无需实现。

### 4.2 BitmapAllocator 的搬迁实现

```rust
// os/servers/vm/src/phys_mem/bitmap_alloc.rs

impl BitmapAllocator {
    pub fn reloc_array_count(&self) -> usize { 2 }

    pub fn reloc_array_info(&self, index: usize) -> (*const u8, usize, usize) {
        match index {
            0 => (self.bitmap.as_ptr() as *const u8, self.bitmap.len(), core::mem::size_of::<u64>()),
            1 => (self.page_cache.as_ptr() as *const u8, self.page_cache.len(), core::mem::size_of::<usize>()),
            _ => (core::ptr::null(), 0, 0),
        }
    }

    pub fn update_relocated_arrays(&mut self, new_ptrs: &[*mut u8]) {
        if new_ptrs.len() >= 1 && !new_ptrs[0].is_null() {
            // SAFETY: new_ptrs[0] points to a valid memory region of at least
            // self.bitmap.len() * size_of::<u64>() bytes, allocated by HeapArena::grow()
            // during relocation. The region is valid for 'static because HeapArena
            // never shrinks below its current top.
            self.bitmap = unsafe {
                core::slice::from_raw_parts_mut(new_ptrs[0] as *mut u64, self.bitmap.len())
            };
        }
        if new_ptrs.len() >= 2 && !new_ptrs[1].is_null() {
            // SAFETY: new_ptrs[1] points to a valid memory region of at least
            // self.page_cache.len() * size_of::<usize>() bytes, allocated by
            // HeapArena::grow() during relocation. Same 'static validity as above.
            self.page_cache = unsafe {
                core::slice::from_raw_parts_mut(new_ptrs[1] as *mut usize, self.page_cache.len())
            };
        }
    }

    pub fn metadata_pa_range(&self) -> (u64, usize) {
        (self.meta_phys_base, self.meta_pages)
    }
}
```

`metadata_pa_range()` 返回旧元数据占据的 PA 基地址和页数。`create_default_allocator()` 中 `meta_phys_base` 和 `meta_pages` 原本是局部变量，现在保存到 `BitmapAllocator` 字段中，供搬迁时释放旧 PA 页。

### 4.3 PhysAlloc 枚举转发

所有 `PhysAllocator` trait 方法都通过 `PhysAlloc` 枚举的 match 转发。新增的三个搬迁方法同样需要转发：

```rust
// os/servers/vm/src/phys_mem/mod.rs

impl PhysAllocator for PhysAlloc {
    // ... existing methods ...

    fn reloc_array_count(&self) -> usize {
        match self {
            PhysAlloc::Bitmap(b) => b.reloc_array_count(),
            #[cfg(feature = "buddy_alloc")]
            PhysAlloc::Buddy(b) => b.reloc_array_count(),
            #[cfg(feature = "segment_tree_alloc")]
            PhysAlloc::SegmentTree(s) => s.reloc_array_count(),
        }
    }
    // ... reloc_array_info, update_relocated_arrays 同理 ...
}
```

### 4.4 VmServer::relocate() 实现

```rust
// os/servers/vm/src/vm_server.rs

impl VmServer {
    pub fn relocate(&mut self) {
        // Phase 1: collect old metadata info (stack-allocated, no heap dependency)
        let (count, old_ptrs, old_pa_base, old_pa_pages) = {
            let phys_alloc = self.page_alloc.phys_alloc_mut();
            let count = phys_alloc.reloc_array_count();
            if count == 0 { return; }

            let mut old_ptrs: [(*const u8, usize); 4] = [(core::ptr::null(), 0); 4];
            let mut total_bytes = 0usize;
            for i in 0..count {
                let (ptr, elem_count, elem_size) = phys_alloc.reloc_array_info(i);
                let bytes = elem_count * elem_size;
                old_ptrs[i] = (ptr, bytes);
                total_bytes += bytes;
            }

            let (old_pa_base, old_pa_pages) = if let Some(bmp) = phys_alloc.as_bitmap_mut() {
                bmp.metadata_pa_range()
            } else {
                return;
            };
            if old_pa_pages == 0 { return; }

            (count, old_ptrs, old_pa_base, old_pa_pages)
        };

        // Phase 2: allocate new VA space via HeapArena
        let total_bytes: usize = old_ptrs[..count].iter().map(|(_, b)| *b).sum();
        let pages = (total_bytes + CLICK_SIZE - 1) / CLICK_SIZE;
        let new_va = match crate::global::heap_arena_grow(pages, &mut self.page_alloc) {
            Ok(va) => va,
            Err(_) => return,
        };

        // Phase 3: memcpy old data to new location
        let mut offset = 0usize;
        let mut new_ptrs: [*mut u8; 4] = [core::ptr::null_mut(); 4];
        for i in 0..count {
            let (old_ptr, bytes) = old_ptrs[i];
            // SAFETY: new_va is a valid heap VA from HeapArena::grow() with at least
            // `total_bytes` bytes of contiguous writable memory starting at new_va.
            // offset is bounded by total_bytes which fits within the allocated region.
            // old_ptr is a valid readable pointer from reloc_array_info() pointing to
            // the old metadata buffer in the BumpBuf region.
            let dst = unsafe { (new_va as *mut u8).add(offset) };
            unsafe { core::ptr::copy_nonoverlapping(old_ptr, dst, bytes); }
            new_ptrs[i] = dst;
            offset += bytes;
        }

        // Phase 4: update PhysAllocator internal pointers
        // Phase 5: free old PA pages
        {
            let phys_alloc = self.page_alloc.phys_alloc_mut();
            phys_alloc.update_relocated_arrays(&new_ptrs[..count]);
            let old_pa = AlignedPhysBytes::new(old_pa_base);
            phys_alloc.free_mem(old_pa, old_pa_pages);
        }
    }
}
```

**关键设计约束**：

1. **搬迁不依赖 GlobalAlloc 分配元数据**。新元数据空间通过 `HeapArena::grow()` 分配（逐页映射物理页），不通过 `Box::new()` 或 `Vec::new()`。
2. **搬迁后无代码通过 Direct Map VA 访问旧元数据位置**。`BitmapAllocator` 通过 `self.bitmap` 和 `self.page_cache` slice 访问元数据，搬迁只需更新这两个 slice 指针。
3. **`metadata_pa_range()` 将旧 VA 转回 PA**。搬迁后释放旧 PA 页时，通过 `metadata_pa_range()` 获取旧 metadata buffer 的物理地址范围。
4. **借用检查器友好**：分阶段释放 `phys_alloc` 的可变借用，避免同时持有 `&mut PhysAlloc` 和 `&mut VmPageAllocator`。

### 4.5 搬迁时序图

```
VmServer::new()                  VmServer::relocate()              VmServer::init()
│                                │                                 │
├─ create_default_allocator()    │                                 │
│  ├─ BumpBuf 分配元数据          │                                 │
│  │  (连续 PA, Direct Map VA)    │                                 │
│  └─ BitmapAllocator::init()    │                                 │
│     (meta_phys_base, meta_pages)                                 │
│                                │                                 │
├─ register_page_alloc()         │                                 │
├─ init_vm_self_pt()             │                                 │
│                                │                                 │
└─────────────────────────────────┤                                 │
                                 │                                 │
                                 ├─ reloc_array_count/info()       │
                                 │  → 收集旧元数据信息               │
                                 │  → 栈上固定大小数组               │
                                 │                                 │
                                 ├─ HeapArena::grow()              │
                                 │  → alloc_phys() × N             │
                                 │  → vm_self_mappages() × N       │
                                 │  → 返回连续 VA                   │
                                 │                                 │
                                 ├─ copy_nonoverlapping()          │
                                 │  → memcpy 旧→新                  │
                                 │                                 │
                                 ├─ update_relocated_arrays()      │
                                 │  → bitmap slice → HeapArena VA  │
                                 │  → page_cache slice → HeapArena │
                                 │                                 │
                                 ├─ free_mem(old_pa, meta_pages)   │
                                 │  → 连续 PA 页释放回分配器         │
                                 │                                 │
                                 └─────────────────────────────────┤
                                                                   │
                                                                   ├─ init_phase1..4()
                                                                   └─ initialized = true
```

### 4.6 搬迁的完整调用链

```
VmServer::relocate()
  │
  ├── VmPageAllocator::phys_alloc_mut()     → 获取 &mut PhysAlloc
  │
  ├── PhysAlloc::reloc_array_count()        → 2 (bitmap + page_cache)
  │
  ├── PhysAlloc::reloc_array_info(0)        → (bitmap_ptr, len, sizeof(u64))
  ├── PhysAlloc::reloc_array_info(1)        → (cache_ptr, len, sizeof(usize))
  │
  ├── BitmapAllocator::metadata_pa_range()  → (meta_phys_base, meta_pages)
  │
  ├── global::heap_arena_grow(pages, alloc)
  │   └── HeapArena::grow(pages, page_alloc)
  │       ├── page_alloc.alloc_phys(1) × N   → 碎片化 PA
  │       └── vm_self_mappages(va, phys) × N  → 连续 VA
  │
  ├── copy_nonoverlapping(old, new, bytes)  → memcpy
  │
  ├── PhysAlloc::update_relocated_arrays(&new_ptrs)
  │   └── BitmapAllocator::update_relocated_arrays()
  │       ├── self.bitmap = from_raw_parts_mut(new_ptrs[0])
  │       └── self.page_cache = from_raw_parts_mut(new_ptrs[1])
  │
  └── PhysAlloc::free_mem(old_pa, meta_pages)
      └── BitmapAllocator::free_mem()
          └── free_pages_internal()          → 连续 PA 页标记为空闲
```

---

## 5. 测试

### 5.1 搬迁接口测试

```rust
// os/servers/vm/src/phys_mem/bitmap_alloc.rs

#[test]
fn test_reloc_array_count() {
    let alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);
    assert_eq!(alloc.reloc_array_count(), 2);
}

#[test]
fn test_reloc_array_info_bitmap_and_page_cache() {
    let alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);
    let (ptr0, len0, size0) = alloc.reloc_array_info(0);
    assert!(!ptr0.is_null());
    assert_eq!(size0, core::mem::size_of::<u64>());
    // ...
}

#[test]
fn test_metadata_pa_range() {
    let alloc = BitmapAllocator::init(metadata, tp, &regions, 0x100000, 5);
    let (pa_base, pa_pages) = alloc.metadata_pa_range();
    assert_eq!(pa_base, 0x100000);
    assert_eq!(pa_pages, 5);
}
```

### 5.2 搬迁后数据一致性

```rust
#[test]
fn test_update_relocated_arrays() {
    let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);
    // allocate and free some pages
    let addr1 = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
    alloc.free_mem(addr1, 1);
    let free_before = alloc.free_pages;

    // simulate relocation: copy data to new buffer, update pointers
    let (old_bitmap_ptr, bitmap_len, _) = alloc.reloc_array_info(0);
    let (old_cache_ptr, cache_len, _) = alloc.reloc_array_info(1);
    // ... allocate new buffer, memcpy, update_relocated_arrays ...

    assert_eq!(alloc.free_pages, free_before);
    // allocation still works after relocation
    let addr2 = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
    alloc.free_mem(addr2, 1);
}
```

### 5.3 搬迁后可正常分配和释放

搬迁后 `BitmapAllocator` 的 `self.bitmap` 和 `self.page_cache` slice 指向 HeapArena VA 范围，alloc/free 行为与搬迁前一致。

### 5.4 搬迁失败降级测试

如果 `HeapArena::grow()` 失败，`relocate()` 静默返回，系统继续使用 BumpBuf 元数据。测试应验证：

- 搬迁失败后 `alloc_mem()` / `free_mem()` 仍可正常工作
- `reloc_array_count()` 仍返回 2（元数据仍在 BumpBuf 中）
- `metadata_pa_range()` 仍返回原始 PA 范围

### 5.5 旧 PA 页释放验证

搬迁完成后，`relocate()` 调用 `free_mem()` 释放旧 PA 页。测试应验证：

- `metadata_pa_range()` 返回的 PA 页在 `free_mem()` 后被标记为空闲
- 释放的 PA 页可被后续 `alloc_mem()` 重新分配

### 5.6 PhysAlloc 枚举转发测试

`PhysAlloc` 枚举的 `reloc_array_count()` / `reloc_array_info()` / `update_relocated_arrays()` 方法应正确转发到内部变体。测试应验证：

- `PhysAlloc::Bitmap(b)` 的转发结果与直接调用 `b.reloc_array_count()` 一致
- `PhysAlloc::Buddy(b)` 的转发结果与直接调用一致（需 `buddy_alloc` feature）

### 5.7 端到端搬迁测试

完整执行 `VmServer::relocate()`，验证搬迁前后 `alloc_mem()` / `free_mem()` 行为一致。这是集成测试，需要完整的 VmServer 初始化环境。

## 6. 参见

- [04-physical-memory.md](04-physical-memory.md) - 物理页分配器（搬迁的主要对象）
- [05-vm-allocpage.md](05-vm-allocpage.md) - 页分配器（BumpBuf → HeapArena 的初始化时序）
- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构体（pt_t 定义，架构差异说明）
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作（搬迁依赖的映射能力）
- [08-slab-allocator.md](08-slab-allocator.md) - Slab 分配器（搬迁后堆的主要使用者）

---

*分类: VM私有 | 使用范围: 仅 VM 内部，初始化阶段*
