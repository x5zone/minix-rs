# 07-pagetable-struct: 页表结构——从双视图结构到 Direct Map 常量偏移

> **分类**: 阶段 3 — 页与页表（结构锚点）
> **源码**: `minix3/minix/servers/vm/pt.h:11-25`（`pt_t`）；`minix3/minix/servers/vm/arch/i386/pagetable.h`（`ARCH_VM_*` 宏族）；`minix3/minix/servers/vm/arch/earm/pagetable.h`（earm 变体）；`minix3/minix/include/arch/i386/include/vm.h:7-64`（PTE 位布局）；`minix3/minix/servers/vm/pagetable.c:1088-1349`（`pt_init` 结构面）+ `:112`（`static_sparepagedirs`）+ `:1358-1435`（`pt_bind` 结构语义）+ `:1442-1489`（`pt_mapkernel` 结构语义）
> **Rust 模块**: `os/servers/vm/src/pagetable/mod.rs`（`PageTable` 别名 + `page_align`）+ `os/servers/vm/src/direct_map.rs`（VM 侧 Direct Map 双向转换）+ `os/servers/vm/src/pagetable/vm_self_map.rs`（VM 自身页表接口）+ `os/arch/src/arch/direct_map.rs`（`DirectMapArch` trait + 三架构实现）+ `os/arch/src/arch/paging.rs`（`Paging` trait + `PageFlags` + `paging_init`）+ `os/arch/src/x86_64/paging.rs`（4 级 walk）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/05-physical-memory.md`（物理分配器）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-page-allocator.md`（页分配 + Direct Map 概念首次引入）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`（`init_vm` 调用点）
> **说明**: 页表**结构**语义模块：**`pt_t` 结构、`ARCH_VM_*` 宏族、Direct Map 双视图（[ARCH: A-1]）、VM 自映射页表（[ARCH: A-9]）、页表层级（[ARCH: A-2]）、地址空间宽度（[ARCH: A-6]）、多架构 trait（[ARCH: A-10]）、`pt_init` 结构面**。**不覆盖**：pt 操作（`pt_new`/`pt_bind`/`pt_copy`/`pt_mapkernel`/`pt_writemap` 逐函数语义 → 08）、页分配（06）、物理分配器（05）。

---

## 1. 概念：页表结构——MMU 的翻译数据与 VM 的访问难题

### 1.0 章节引言

本文档回答 01 文档启动链上的一个问题：**VM 作为"所有进程页表的管理者"，如何表示一个页表，又如何访问页表本身**。它在 `init_vm()` 中的位置是：

```
init_vm() ──► init_proc(VM_PROC_NR) + pt_init()（main.c:474-475，VM 自身槽 + 页表建立）
              │
              └─ pt_init 结构面（pagetable.c:1088-1349）
                     ├─ 从内核继承初始映射（拷贝内核页目录 + 登记 kernel 映射）
                     ├─ 自举资源（static_sparepages / static_sparepagedirs）
                     └─ 动态重建（alloc_cycle 换血 + pt_copy 重建，06 §1.4 已述）
```

06 文档解决了"VM 给自己分配页"（`vm_allocpage` 族）；本文档解决"**页表长什么样、VM 怎么读写它**"——`pt_t` 结构、层级、Direct Map 双视图。08 文档承接"页表怎么建/改/删/绑"（`pt_new`/`pt_writemap` 等）。三篇合起来回答启动链上"页与页表"阶段。

### 1.1 页表是 MMU 的翻译数据

页表（page table）是 CPU 的 MMU 硬件用来把**虚拟地址**翻译成**物理地址**的表格数据。它既不是设备寄存器，也不是内核私有账本——它是**每个进程地址空间的结构骨架**：

| 维度 | 32 位（Minix3 i386） | 64 位（minix-rs） |
|------|---------------------|-------------------|
| 级数 | 2 级：PD → PT | 4 级：PML4 → PDPT → PD → PT |
| 每级项数 | 1024（PD）/ 1024（PT） | 512 × 4 |
| PTE 宽度 | u32 | u64 |
| 虚拟地址切分 | 10 + 10 + 12 | 9 + 9 + 9 + 9 + 12 |
| 覆盖地址空间 | 4GB | 256TB（48 位） |

**层级数不是设计选择，是位宽演进的必然**：32 位地址用 10+10+12 切分刚好两级；64 位地址若每级仍 10 位需要 5 级多、若每级 9 位需要 4 级——所有主流 64 位架构（x86-64/arm64/riscv64）都收敛到 4 级 + 9 位索引。这就是 [ARCH: A-2] 的硬件面：**Minix3 的两级结构在 64 位上不存在**，minix-rs 必须用 4 级。

页表项（PTE）的布局也遵循同一模式：**高比特存物理页框地址，低比特存属性标志**（PRESENT/WRITE/USER/缓存策略等）。x86-64 的 PTE 是 52 位物理地址 + 12 位标志；Minix3 i386 是 20 位物理地址 + 12 位标志（`I386_VM_ADDR_MASK 0xFFFFF000`，include/arch/i386/include/vm.h:18）。

### 1.2 双视图问题：MMU 用 PA，VM 用 VA

页表有一个任何内存管理器都必须面对的**视角分裂**：

- **MMU 用物理地址（PA）读页表**——CR3/TTBR0/satp 装的是页表根的物理地址，翻译过程每一步都是 PA。
- **VM 用虚拟地址（VA）读写页表**——VM 自己是用户态进程，分页开启后它的每一次访存都经过 MMU；它要往 PTE 里写东西，必须先有一个可访问的 VA。

于是同一个页表页，必须同时存在两个地址视图：**PA（给 MMU）和 VA（给 VM）**。这是理解 C 版 `pt_t` 结构的一把钥匙——它的每一个字段都是这个"双视图"的直接产物。

注意这个问题的边界：它只涉及"**访问页表页本身**"，不涉及"物理页的分配"。物理页从分配器拿（05/06），页表页拿到后**怎么写 PTE**才是本文档的主题。

### 1.3 C 的解法：pt_t 双视图结构

Minix3 的 `pt_t`（pt.h:11-25）用四个字段解决双视图问题：

```c
typedef struct {
	u32_t *pt_dir;		/* 页目录的 VA（VM 访问用） */
	u32_t pt_dir_phys;	/* 页目录的 PA（CR3 用） */
	u32_t *pt_pt[ARCH_VM_DIR_ENTRIES];	/* 1024 个页表的 VA 缓存 */
	u32_t pt_virtop;	/* VA 分配提示（未使用） */
} pt_t;
```

同一物理页的两个视角：

```
          ┌──────────────────────────┐
   VA 视角 │  pt_dir / pt_pt[pde]     │ ← VM 写 PTE 用（VM 地址空间内可访问）
          │  （VA 缓存）              │
          └─────────────┬────────────┘
                        │ 同一物理页
          ┌─────────────┴────────────┐
   PA 视角 │  pt_dir_phys / pt_dir[pde] │ ← MMU 翻译用（CR3/PDE 里是物理地址）
          │  （物理地址）               │
          └──────────────────────────┘
```

关键观察：

1. **根（页目录）双视图**：`pt_dir`（VA）与 `pt_dir_phys`（PA）指向同一物理页，VM 写目录用 VA，CPU 翻译用 PA。
2. **二级页表双视图**：`pt_dir[pde]` 存第 `pde` 个页表的 **PA**（给 CPU），`pt_pt[pde]` 缓存同一页表的 **VA**（给 VM 写 PTE 用）。两个数组按 PDE 索引对齐——`pt_pt[pde]` 与 `pt_dir[pde]` 是同一物理页的两副眼镜。
3. **VA 缓存从哪来**：VM 用 `vm_mappages` 把页表页映射进自己的地址空间（findhole 找 VA，06 文档），或自举期从 BSS 静态池拿（VA 编译期固定）。
4. **`pt_virtop` 是死字段**：设计意图是"查找空闲虚拟地址的起始提示"，但 `findhole` 用的是静态变量 `lastv`（pagetable.c:155），rg 实证 `pt_virtop` 全库仅 pagetable.c:1019 一次写入、零读取。

**这套结构的代价**：VM 每拿到一页（无论是页表页还是数据页），都要先解决"它在我的地址空间里有没有 VA"——有则缓存（`pt_pt[]`），没有则动态映射。C 的双视图不是免费的：它需要 1024 项指针缓存 + findhole 映射机制 + 递归保护（06 §1.2）。

### 1.4 minix-rs 的解法：Direct Map 常量偏移（[ARCH: A-1]）

minix-rs 引入 **Direct Map**（[ARCH: A-1]）：内核在 VM 的地址空间预映射一段固定窗口，使

```
VA = VM_DIRECT_MAP_BASE + PA        （direct_map.rs:21，常量偏移）
```

于是"拿到一个物理页"和"获得一个可访问的 VA"是**同一件事**。对页表页尤其如此：

| 维度 | Minix3 `pt_t` | minix-rs `PageTable` |
|------|---------------|----------------------|
| 页表根 | `pt_dir`（VA）+ `pt_dir_phys`（PA）两个字段 | 单一 `root_paddr`（PA），VA 用 `phys_to_ptr_dm` 即时派生 |
| 二级页表 | `pt_pt[1024]` VA 缓存 + `pt_dir[pde]` PA | 中间表 PA 存于上一级表项，VA 经 Direct Map 派生 |
| VA 获取 | findhole + `vm_mappages`（可能递归，06 §1.2） | 常量加法，不可能递归 |
| 死字段 | `pt_virtop` | 无 |
| 数组规模 | 固定 1024 项（i386）/ 4096 项（earm） | 按需 `walk_alloc` 分配中间表（A-2） |

**这不是翻译，是结构消除**：C 的"双视图结构"（两个字段/两套数组表达同一物理页）在 minix-rs 中被"单一物理地址 + 常量偏移派生"取代。外部行为（VM 总能读写页表）不变，内部结构从"显式双视图"变成"一个 PA，两个视角即时换算"。

为什么现代 OS 都这么做？对照 Redox 与 Linux：Redox 内核有 kernel direct map，物理帧分配器（FrameAllocator）与映射子系统解耦；Linux 内核线性映射区（`page_offset_base` 起）让"物理页 → 内核 VA"是常量加法；Windows 的 `MmGetVirtualForPhysical` 同理。**Direct Map 是 64 位地址空间富余后的通用 OS 实践**（Redox/Linux/Windows 均如此）；Minix3 的 VM 服务因 32 位地址空间稀缺而采用动态映射——这正是 A-1 演进的外部对照。双窗口（用户区 U/S=1 + 内核区 U/S=0）的必要性在 `01-stage-kernel/07-cross-space-init.md §4` 已论证，不重复。

### 1.5 VM 自身的页表（[ARCH: A-9]）

VM 作为所有进程页表的管理者，**自己也得有一个页表**（内核启动时建立，VM 进程的代码/数据就在其中）。"VM 自映射"指的是 VM 对自己页表的访问接口：

- **C**：自举期用 BSS 静态资源——`static_sparepages`（pagetable.c:108，单页池）与 `static_sparepagedirs`（pagetable.c:112，**仅 ARM**，16KB 对齐的页目录框）——在 VM 页表可用前支撑自举；`pt_init` 末尾用动态页整体替换（06 §1.4）。
- **minix-rs**：`vm_self_map` 模块——VM 自身 `PageTable` 存于模块级静态（`Option<PageTable>`，初始化前为 `None`），提供 `vm_self_mappages`/`vm_self_unmap`/`vm_self_query`/`vm_self_unmappages` 四个自由函数。页表页的读写经 Direct Map，不需要 BSS 备用资源。

[ARCH: A-9] 的语义是**资源形态演进**：C 的"静态 BSS 备用数组 + 保留队列"演进为"模块级静态 `Option<PageTable>` + 自由函数接口"。注意不是一一对应——`static_sparepagedirs` 仅 ARM 存在（x86 页目录 4KB，用单页池即可），而 `vm_self_map` 对所有架构统一。

### 1.6 地址空间宽度与多架构（[ARCH: A-6] / [ARCH: A-10]）

**地址空间宽度（A-6）**：Minix3 的 VM 在 32 位地址空间里精打细算——`VM_DATATOP`/`VM_STACKTOP` 由内核参数决定（vm.h:65-67），VM 自身堆从 `VM_OWN_HEAPBASE` 起、mmap 区挤在 `VM_OWN_MMAPBASE` 的 100MB 窗口（vm.h:83-86）。minix-rs 的 64 位地址空间不再稀缺：`MMAP_BASE 0x1_0000_0000` / `MMAP_TOP 0x200_0000_0000`（mmap.rs:157-158）——1TB 的 mmap 区，Direct Map 窗口（1GB）只是用户区的一小段。A-6 是 Direct Map 可行的前提：**没有 64 位余量，就没有常量偏移映射的奢侈**。

**多架构（A-10）**：Minix3 用两套宏（`arch/i386/pagetable.h` 与 `arch/earm/pagetable.h`）表达架构差异——同一语义（`ARCH_VM_DIR_ENTRIES`/`PTF_*`/`PFERR_*`）在两个头文件里各写一遍，调用点用 `#if defined(__i386__)`/`#elif defined(__arm__)` 切换。minix-rs 用 trait 静态分派：`Paging`（页表操作）、`DirectMapArch`（地址空间布局）、`HugePages`（大页能力）三 trait，x86-64/arm64/riscv64 各实现一份，VM 侧只依赖 trait 接口。这是"宏族 → trait"的机制级演进（Ch2 §2.2/2.3 给出 C 侧证据，Ch3 §3.4 给出 Rust 侧设计）。

### 1.7 与 06/08 的分工边界

| 文档 | 管辖 | 关键接口 |
|------|------|---------|
| 05-physical-memory | 物理内存布局与分配器 | `alloc_mem/free_mem` |
| 06-page-allocator | VM 自用页分配（VA+PA 一次拿） | `vm_allocpage` 族、`vm_pt_alloc` |
| **07（本文档）** | **页表结构**：`pt_t`、层级、Direct Map 双视图、VM 自映射、`pt_init` 结构面 | `PageTable`、`DirectMapArch`、`vm_self_map` |
| 08-pagetable-ops | 页表操作：建/改/删/绑/拷 | `pt_new`、`pt_writemap`、`pt_bind` 逐函数 |

边界以本文档头部『说明』块函数清单为准绳。**"结构面 vs 操作面"切割**：`pt_init` 是 01 的调用点、07 的结构叙事、08 的操作素材——本文档只讲"`pt_init` 之后 VM 页表长什么样、结构如何继承自内核、自举如何完成"，`pt_new`/`pt_bind`/`pt_copy`/`pt_mapkernel` 的逐函数语义移交 08。

### 1.8 本章小结

- 页表是 MMU 的翻译数据；层级数（2 级 vs 4 级）是位宽演进的必然（A-2）。
- **双视图问题**（MMU 用 PA、VM 用 VA）是 C `pt_t` 结构的成因；minix-rs 用 Direct Map 常量偏移（A-1）从结构上消除它。
- VM 自映射从"BSS 静态备用资源"演进为"`vm_self_map` 模块"（A-9）。
- 64 位地址空间（A-6）与多架构 trait（A-10）是 Direct Map 可行的前提与表达方式。
- 下一篇（08）承接"页表操作"：建/改/删/绑/拷的逐函数语义。

---

## 2. C 源码分析

### 2.0 本章定位

本章覆盖本文档语义范围内的全部 C 符号：`pt_t`（pt.h）、`ARCH_VM_*`/`PTF_*`/`PFERR_*` 宏族（两个 arch 的 pagetable.h + include/arch/i386/include/vm.h）、`pt_init` 结构面（pagetable.c:1088-1349）、`static_sparepagedirs`（pagetable.c:112）、`pt_bind`/`pt_mapkernel` 的结构语义（pagetable.c:1358-1435/1442-1489）。所有行号经 `nl -ba` 实证。

### 2.1 pt_t 结构（pt.h:11-25）

```c
typedef struct {
	u32_t *pt_dir;		/* 页目录 VA，页对齐（ARCH_VM_DIR_ENTRIES） */
	u32_t pt_dir_phys;	/* 页目录 PA */
	u32_t *pt_pt[ARCH_VM_DIR_ENTRIES];	/* 页表 VA 缓存数组 */
	u32_t pt_virtop;	/* VA 分配提示（死字段） */
} pt_t;
```

- **`pt_dir` / `pt_dir_phys`**（pt.h:13-14）：页目录的双视图。`pt_new`（pagetable.c:990-1026）用 `vm_allocpages(&pt->pt_dir_phys, VMP_PAGEDIR, ARCH_PAGEDIR_SIZE/VM_PAGE_SIZE)` 一次拿到 VA+PA（06 §2.1），然后清零 1024 项目录（pagetable.c:1004-1009）。注意 `ARCH_PAGEDIR_SIZE/VM_PAGE_SIZE`：i386 页目录 = 1 页（4KB），earm 页目录 = 4 页（16KB，`ARM_PAGEDIR_SIZE`）。
- **`pt_pt[ARCH_VM_DIR_ENTRIES]`**（pt.h:17）：页表 VA 缓存。`pt_ptalloc`（pagetable.c:494-528）按需分配页表页并登记：`pt->pt_pt[pde] = p`（VA）、`pt->pt_dir[pde] = (pt_phys & ARCH_VM_ADDR_MASK) | flags`（PA+标志）。两个数组按 PDE 索引对齐。
- **`pt_virtop`**（pt.h:24）：**未使用**。rg 实证：`rg "pt_virtop" minix3/minix/servers/vm/ -n` 仅返回定义（pt.h:24）与一次写入（pagetable.c:1019 `pt->pt_virtop = 0;`），零读取。`findhole` 用静态 `lastv`（pagetable.c:155-157）。
- **`CLICKSPERPAGE`**（pt.h:27）：`VM_PAGE_SIZE/CLICK_SIZE`——要求 click 等于页（pagetable.c:97-101 有编译期断言 `#error CLICK_SIZE must be page size.`）。

### 2.2 ARCH_VM_* 宏族（arch/i386/pagetable.h:10-45）

i386 版把硬件细节**归一**成 VM 可用的抽象宏：

```c
/* Mapping flags（pagetable.h:11-16）：PTF_* 直接映射到 I386_VM_* 位 */
#define PTF_WRITE	I386_VM_WRITE
#define PTF_READ	I386_VM_READ
#define PTF_PRESENT	I386_VM_PRESENT
#define PTF_USER	I386_VM_USER
#define PTF_GLOBAL	I386_VM_GLOBAL
#define PTF_NOCACHE	(I386_VM_PWT | I386_VM_PCD)

/* 结构常量（pagetable.h:18-29） */
#define ARCH_VM_DIR_ENTRIES	I386_VM_DIR_ENTRIES	/* 1024 */
#define ARCH_BIG_PAGE_SIZE	I386_BIG_PAGE_SIZE	/* 4MB */
#define ARCH_VM_ADDR_MASK	I386_VM_ADDR_MASK	/* 0xFFFFF000 */
#define ARCH_PAGEDIR_SIZE	I386_PAGE_SIZE		/* 4KB */
#define ARCH_VM_PT_ENTRIES	I386_VM_PT_ENTRIES	/* 1024 */
#define ARCH_VM_BIGPAGE		I386_VM_BIGPAGE		/* 4MB 大页位 */
/* ... ARCH_VM_PDE_PRESENT/ARCH_VM_PTE_PRESENT/ARCH_VM_PTE_USER/ARCH_VM_PTE_RW */

/* 页错误解码（pagetable.h:36-39） */
#define PFERR_NOPAGE(e)	(!((e) & I386_VM_PFE_P))
#define PFERR_PROT(e)	(((e) & I386_VM_PFE_P))
#define PFERR_WRITE(e)	((e) & I386_VM_PFE_W)

/* VA → 表索引（pagetable.h:44-45） */
#define ARCH_VM_PTE(v) I386_VM_PTE(v)	/* (v>>12) & 0x3FF */
#define ARCH_VM_PDE(v) I386_VM_PDE(v)	/* v>>22 */
```

**`ARCH_VM_*` 是 C 版的"硬件抽象层"**：VM 核心代码只写 `ARCH_VM_PDE(v)`，不写 `I386_VM_PDE(v)`，从而让 earm 版通过**重定义同一批宏**接入。它证明了"抽象"是 Minix3 的需求，只是表达方式是宏而不是 trait——这为 [ARCH: A-10] 提供了"trait 是宏族的类型化继承者"的论据。

**页错误解码宏（`PFERR_*`）**：虽然文件在本文档范围，但消费方是页错误处理（16-pagefault），本文档只登记不展开。i386 的语义：错误码 bit0 = 页面不存在（否则是保护违规），bit1 = 写操作（否则读操作）。

### 2.3 earm 变体（arch/earm/pagetable.h:10-49）——A-10 的 C 侧证据

| 宏 | i386 | earm | 差异含义 |
|----|------|------|---------|
| `ARCH_VM_DIR_ENTRIES` | 1024 | 4096（`ARM_VM_DIR_ENTRIES`） | ARM 一级目录 16KB |
| `ARCH_VM_PT_ENTRIES` | 1024 | 256（`ARM_VM_PT_ENTRIES`） | ARM 二级页表 1KB |
| `ARCH_BIG_PAGE_SIZE` | 4MB | 1MB（`ARM_SECTION_SIZE`） | section 映射粒度 |
| `ARCH_PAGEDIR_SIZE` | 4KB | 16KB（`ARM_PAGEDIR_SIZE`） | 页目录物理页数 |
| `PTF_*` 标志面 | READ/WRITE/PRESENT/USER/GLOBAL/NOCACHE | +SUPER/CACHEWB/CACHEWT/SHARE；NOCACHE=DEVICE | 缓存策略与特权模型差异 |
| `PFERR_PROT` | 按 P 位 | 按 L1/L2 permission fault（`ARM_VM_PFE_L1PERM/L2PERM`） | 页错误编码差异 |

同一语义（"目录项数""大页大小""页错误解码"）在两个头文件里**各写一遍**，调用点用条件编译切换——这正是 A-10 要演进掉的重复：minix-rs 的 trait 把"语义"（trait 方法/常量）与"实现"（每架构 impl）分离，VM 核心不再出现 `#if defined(__i386__)`。

### 2.4 PTE 位布局（include/arch/i386/include/vm.h:7-64）

```c
#define I386_PAGE_SIZE		4096
#define I386_VM_PRESENT	0x001	/* Page is present */
#define I386_VM_WRITE	0x002	/* Read/write access allowed */
#define I386_VM_USER	0x004	/* User access allowed */
#define I386_VM_PWT	0x008	/* Write through */
#define I386_VM_PCD	0x010	/* Cache disable */
#define I386_VM_ADDR_MASK 0xFFFFF000 /* physical address */
#define I386_VM_BIGPAGE	0x080	/* 4MB page */
#define I386_VM_GLOBAL   (1L<< 8)	/* Global. */
#define I386_VM_DIR_ENTRIES	1024
#define I386_VM_DIR_ENT_SHIFT	22
#define I386_VM_PT_ENT_SHIFT	12
#define I386_VM_PT_ENT_MASK	0x3FF
```

要点：
- **物理地址掩码 `0xFFFFF000`**（include/arch/i386/include/vm.h:18）：20 位页框地址 + 低 12 位标志。x86-64 对应 `ADDR_MASK 0x000F_FFFF_FFFF_F000`（52 位物理地址，x86_64/paging.rs:41）。
- **索引宏**（include/arch/i386/include/vm.h:61-62）：`I386_VM_PDE(v) = v >> 22`、`I386_VM_PTE(v) = (v >> 12) & 0x3FF`——10 位目录索引 + 10 位页表索引 + 12 位页内偏移。
- **标志位**：PRESENT/WRITE/USER/PWT/PCD/ACC（include/arch/i386/include/vm.h:11-17）、BIGPAGE（:23，PDE 专属 4MB 页）、GLOBAL（:28，CR4.PGE 开启时 TLB 不刷）。

这些是 A-2 的 C 侧基线：32 位两级结构的具体数值。minix-rs 的 x86-64 对应：`PML4_SHIFT 39`/`PDPT_SHIFT 30`/`PD_SHIFT 21`（x86_64/paging.rs:38-41）+ `flags_to_pte`/`pte_to_flags` 翻译函数（:80-113，NX 反相）。**位布局语义封在 arch crate，VM 侧只接触 `PageFlags`**（Ch3 §3.4）。

### 2.5 pt_init 结构面（pagetable.c:1088-1349）

`pt_init` 是"VM 页表从零到可用"的完整过程。本文档只讲**结构面**（形状/继承关系/自举），按执行顺序分六步：

**① 定位内核模块**（pagetable.c:1104-1113）：

```c
kern_mb_mod = &kernel_boot_info.module_list[m];	/* m = kern_mod */
kern_size = kern_mb_mod->mod_end - kern_mb_mod->mod_start;
kern_start_pde = kernel_boot_info.vir_kern_start / ARCH_BIG_PAGE_SIZE;
```

结构意义：VM 页表必须**包含内核映射**（每个进程页表都映射内核区，`pt_mapkernel` 结构语义见 §2.7），所以先要知道内核在物理/虚拟空间的什么位置。`kern_start_pde` 是内核虚拟地址对应的 PDE 编号。

**② 自举资源**（pagetable.c:1116-1136）：`static_sparepages`（BSS 数组）入保留队列，`sys_umap` 向内核查询其物理地址（VM 不知道 BSS 页的 PA）。ARM 额外处理 `static_sparepagedirs`（§2.6）。结构意义：**VM 页表可用前，页表页/数据页从 BSS 静态池拿**——VA 编译期固定，不需要 findhole（06 §1.3）。

**③ CPU 特性**（pagetable.c:1138-1146）：`_cpufeature(_CPUF_I386_PGE)` → `global_bit`；`_cpufeature(_CPUF_I386_PSE)` → `bigpage_ok`。结构意义：是否能用 4MB 大页映射内核区、PTE 是否置 G 位，取决于硬件。

**④ 内核映射登记**（pagetable.c:1148-1241）：`freepde()` 从 `kernel_boot_info.freepde_start` 逐号分配 PDE；`sys_vmctl_get_mapping` 遍历内核自有映射（`kern_mappings[]`），VM 为每个映射分配 VA 并 `sys_vmctl_reply_mapping` 回告内核。结构意义：**VM 页表的 PDE 空间有一块专供内核映射**（`MAX_KERNMAPPINGS 10`，pagetable.c:90）。

**⑤ 页目录页表（pagedir_mappings）**（pagetable.c:1243 + 1035-1067）：`pt_allocate_kernel_mapped_pagetables` 为 `pagedir_mappings[MAX_PAGEDIR_PDES]`（pagetable.c:44-49）分配页表页——**内核借这些 PDE 查看所有进程的页目录**（`pt_bind` 用它登记每个进程的目录，§2.7）。结构意义：内核需要一个稳定的 VA 视图访问任意进程的 CR3 目标页。

**⑥ 拷贝内核初始映射 + 绑定 + 动态重建**（pagetable.c:1254-1349）：

```c
newpt = &vmprocess->vm_pt;
if(pt_new(newpt) != OK) panic("vm pt_new failed");	/* ① 分配页目录 + pt_mapkernel */

sys_vmctl_get_pdbr(SELF, &mypdbr);			/* ② 读当前 CR3（内核的） */
sys_vircopy(NONE, mypdbr, SELF, currentpagedir, ...);	/* ③ 拷贝内核页目录到本地 */

for(p = 0; p < ARCH_VM_DIR_ENTRIES; p++) {		/* ④ 逐项继承 */
    if(!(entry & ARCH_VM_PDE_PRESENT)) continue;	/* 跳过空项 */
    if((entry & ARCH_VM_BIGPAGE)) continue;		/* 跳过 4MB 项（内核区/恒等映射） */
    pt_ptalloc(newpt, p, 0);				/* 分配页表页 */
    sys_abscopy(ptaddr_kern, ptaddr_us, VM_PAGE_SIZE);	/* 拷贝整页 PTE */
}

pt_bind(newpt, &vmproc[VM_PROC_NR]);			/* ⑤ 通知内核切换 */
pt_init_done = 1;					/* ⑥ 稳态开始 */
/* ⑦ 动态重建：alloc_cycle 换血 + pt_copy 重建（06 §1.4 已述，L1314-1349） */
```

结构意义：**VM 的页表不是从零画的，而是从内核的初始页表"继承"而来**——内核为 VM 进程建立的映射（代码/数据/BSS + 内核区）被逐项拷贝进 VM 自己的 `pt_t`。这正是 minix-rs `paging_init`（Ch3 §3.4）对应"结构继承"的地方。

### 2.6 static_sparepagedirs 与自举资源（pagetable.c:112）

```c
#if defined(__arm__)
static char static_sparepagedirs[ARCH_PAGEDIR_SIZE*STATIC_SPAREPAGEDIRS + ARCH_PAGEDIR_SIZE] __aligned(ARCH_PAGEDIR_SIZE);
#endif
```

- **仅 ARM**：earm 页目录 16KB（`ARCH_PAGEDIR_SIZE`），自举期需要预对齐的页目录框；i386 页目录 4KB，`static_sparepages` 单页池即可兼作。
- **消费方式**（pagetable.c:1133-1144）：`sys_umap` 查询物理地址，前 `STATIC_SPAREPAGEDIRS` 个入 `sparepagedirs[]` 数组（`missing_sparedirs` 记账），其余标记 NULL——与 06 文档的备用页池同构（自举脚手架，非稳态供应）。

A-9 的 C 侧就是这些 BSS 静态资源：**VM 用自己的"静态自有页"支撑自己的页表建立**。minix-rs 的 `vm_self_map` 模块（Ch3 §3.3）是这一语义的类型化继承者。

### 2.7 pt_bind / pt_mapkernel 的结构语义

**`pt_bind`**（pagetable.c:1358-1435）：把进程页表登记给内核并切换。

```c
pdm = &pagedir_mappings[procslot/slots_per_pde];	/* 内核看所有页目录的窗口 */
pdm->page_directories[pdeslot] = phys | ARCH_VM_PDE_PRESENT|ARCH_VM_PTE_RW;
pdes = (void *)(pagedir_pde*ARCH_BIG_PAGE_SIZE + pdeslot*VM_PAGE_SIZE);
return sys_vmctl_set_addrspace(who->vm_endpoint, pt->pt_dir_phys, pdes);
```

结构语义：**"页目录的页目录"**——`pagedir_mappings` 的 PDE 让内核在自己的地址空间里能看到任意进程的页目录页（`page_directories` 页表），`sys_vmctl_set_addrspace` 通知内核把目标进程的 CR3 换成 `pt_dir_phys`。minix-rs 对应 `bind_to_process`（paging.rs:464）。

**`pt_mapkernel`**（pagetable.c:1442-1489）：每个页表必须映射内核区——三段：

1. 内核代码/数据：`kern_start_pde` 起逐个大页（`pt->pt_dir[kern_pde] = addr | PRESENT|BIGPAGE|RW|global_bit`，i386）；
2. `pagedir_mappings` 的 PDE（内核看页目录的窗口）；
3. `kern_mappings[]`（内核自有映射，§2.5 ④登记）。

结构语义：**"页表的结构完整性" = 用户区 + 内核区 + 内核窗口三段**。任何进程页表缺了内核区，系统调用返回用户态就会立刻故障。minix-rs 对应 `map_kernel`（paging.rs:500）。两个函数的**逐函数细节（返回值/错误路径/标志翻译）移交 08**，本文档只取其结构角色。

### 2.8 结构面函数清单

| C 符号 | 位置 | 本文档角色 | 归属 |
|--------|------|-----------|------|
| `pt_t` 结构 | pt.h:11-25 | §2.1 全解析 | 07（本文档） |
| `ARCH_VM_*`/`PTF_*`/`PFERR_*` 宏族 | i386/pagetable.h:10-45、earm/pagetable.h:10-49 | §2.2/2.3 全解析 | 07 |
| `I386_VM_*` 位布局 | include/arch/i386/include/vm.h:7-64 | §2.4 全解析 | 07 |
| `pt_init`（结构面） | pagetable.c:1088-1349 | §2.5 结构叙事 | 07（操作细节归 08/06） |
| `static_sparepages`/`static_sparepagedirs` | pagetable.c:108/112 | §2.6（消费语义归 06） | 07（A-9 结构） |
| `pt_bind`（结构语义） | pagetable.c:1358-1435 | §2.7 | 08（本文档只取结构） |
| `pt_mapkernel`（结构语义） | pagetable.c:1442-1489 | §2.7 | 08（本文档只取结构） |
| `pt_new`/`pt_copy`/`pt_allocate_kernel_mapped_pagetables` | pagetable.c:990-1085 | §2.5 上下文引用 | 08 |
| `findhole`/`vm_mappages` | pagetable.c:155-325 | 06 已述 | 06 |

---

## 3. Rust 设计决策

> 本节是正式文档的设计决策论述；设计契约快照属中间产物，决策依据以 C 源码与 Rust 实现为准（ground truth 链）。

### 3.1 D1: `PageTable` 类型别名——pt_t 结构消除（[ARCH: A-2]）

**C 的问题**：`pt_t` 把"页表"建模为"数据 + 指针缓存"结构体——`pt_dir`/`pt_pt[]` 是 VA 缓存，`pt_dir_phys`/PDE 是 PA，双视图要靠**两套字段保持同步**；1024 项固定数组在 64 位上不存在（A-2）。

**Rust 决策**：

```rust
// pagetable/mod.rs:22
pub(crate) type PageTable = minix_arch::CurrentPaging;
```

`CurrentPaging` 按 feature/target_arch 编译期选择实现（`MockPaging`/`X86_64Paging`/`AArch64Paging`/`Riscv64Paging`，os/arch/src/lib.rs:100-115）。页表结构从此是 trait 对象内部的事：

- **根 = 单一物理页**（`root_paddr`），不再有 `pt_dir`/`pt_dir_phys` 双字段——PA 是唯一真相，VA 经 Direct Map 派生（D2）；
- **中间级按需分配**：`walk_alloc`（x86_64/paging.rs:277-330）在 map 时逐级分配 PDPT/PD/PT 页，不再有 `pt_pt[1024]` 固定缓存——稀疏地址空间（64 位必然稀疏）不用预先分配全部 512 项 × 4 级；
- **`pt_virtop` 死字段消失**：VA 分配由 region 机制（13/14）管理，页表结构不关心。

**为什么不是别的方案**：备选是保留"固定 4 级数组"结构体（`[Pml4Entry; 512]` 嵌套），与 C 的固定数组同构——但 64 位稀疏地址空间会让每个进程页表背上 4KB×4 的固定开销，且把"硬件层级"泄漏进 VM 层。trait 对象 + 按需分配把层级封在 arch crate，VM 层只见"映射/解映射/查询"。

### 3.2 D2: `DirectMapArch`——双视图 → 常量偏移（[ARCH: A-1]）

**C 的问题**：页表页/自用页的 VA 要么缓存（`pt_pt[]`），要么动态映射（findhole），要么内核中介拷贝（`sys_abscopy`）——三个机制维护同一份"PA→VA"关系。

**Rust 决策**：`DirectMapArch` trait（os/arch/src/arch/direct_map.rs:26-55）把"PA→VA"变成**常量算术**：

```rust
pub trait DirectMapArch {
    const VM_DIRECT_MAP_BASE: u64;
    const KERNEL_DIRECT_MAP_BASE: u64;
    const VM_HEAP_BASE: u64;
    const VM_HEAP_SIZE: u64;

    fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes { /* phys + VM_DIRECT_MAP_BASE */ }
    fn kernel_phys_to_virt(phys: PhysBytes) -> VirBytes { /* phys + KERNEL_DIRECT_MAP_BASE */ }
    fn virt_to_phys(virt: VirBytes) -> PhysBytes { /* 高半 → kernel，低半 → VM */ }
}
```

VM 侧 `direct_map.rs` 包装为 `vm_phys_to_virt`/`kernel_phys_to_virt`/`virt_to_phys`/`is_direct_map_virt`（+ `VM_DIRECT_MAP_SIZE`/`VM_HEAP_*` 常量）。**双窗口（用户区 U/S=1 + 内核区 U/S=0）是特权级必然**，不是冗余（01-stage-kernel/07-cross-space-init.md §4 已论证）。

三架构常量：

| 架构 | `VM_DIRECT_MAP_BASE` | `KERNEL_DIRECT_MAP_BASE` | 依据 |
|------|---------------------|--------------------------|------|
| x86-64 | `0x0000_0000_8000_0000`（2GB） | `0xFFFF_8000_0000_0000` | 用户区 2GB 起，内核高半 |
| arm64 | `0x0000_1000_0000_0000` | `0xFFFF_8000_0000_0000` | TTBR0 用户区 |
| riscv64 | `0x0000_0010_0000_0000` | `0xFFFF_FC00_0000_0000` | Sv39 用户低半 / 内核高半 |
| Mock | 可配置（`set_mock_vm_base`） | `0xFFFF_8000_0000_0000` | 无 QEMU 单元测试 |

**为什么是 trait 而不是常量**：`VM_DIRECT_MAP_BASE` 若做成 VM crate 的裸常量，`virt_to_phys` 的双窗口判断就得用 `#[cfg(target_arch)]` 硬编码（A-10 禁止）；trait 让"布局"成为架构的**能力**，`CurrentDirectMap` 编译期选中实现，VM 侧零条件编译。**大页参数（1GB/2MB）不在此 trait**——那是 MMU 能力（`HugePages`），不是地址空间布局（direct_map.rs 头注释明言）。

### 3.3 D3: `vm_self_map`——VM 自映射模块（[ARCH: A-9]）

**C 的问题**：VM 自己的页表靠 BSS 静态资源（`static_sparepages`/`static_sparepagedirs`）+ 保留队列自举（06 已消），且"VM 自身页表"散落在全局 `vmprocess->vm_pt` + 各种 `pt_*` 函数里。

**Rust 决策**：`pagetable/vm_self_map.rs` 把"VM 自身的页表"收敛为一个模块：

```rust
static VM_SELF_PT_STORAGE: AssumeSyncCell<Option<PageTable>> = AssumeSyncCell::new(None);
// init_vm_self_pt()（一次性初始化，vm_server.rs:111 调用）
// vm_self_mappages(va, phys, flags) / vm_self_unmap(va) → PhysBytes
// vm_self_query(va) → Option<(PhysBytes, PageFlags)> / vm_self_unmappages(va, pages)
```

设计要点：

1. **`Option<PageTable>` 而非空指针**：C 的"未初始化 = NULL 指针"用 `Option` 表达（`None` = 未初始化），`get_pt_mut()` 在未初始化时 `expect` panic——Rust 类型系统把"可能未初始化"变成可检查状态，消除 C 式哨兵值。
2. **`AssumeSyncCell` 而非裸 `static mut`**：单线程事件循环模型（lib.rs 头注释）下，`&mut` 唯一性由"单线程 + 方法调用顺序"保证；`AssumeSyncCell` 只为满足 `static` 的 `Sync` 约束，无并发语义（04-acl 文档同款论证）。
3. **为什么自由函数而非 `VmServer` 方法**：页表必须存放在**稳定地址**（`VmServer` 可能被移动，`Option<PageTable>` 存于模块级静态保证地址稳定）；且全局分配器（`GlobalAlloc`）只有 `&self`，无法拿 `&mut VmServer`——自由函数从静态取 `&mut PageTable` 是唯一自洽路径（vm_self_map.rs 头注释"# Why not just use Paging::map() directly?"）。
4. **消费方**：HeapArena（09）经 `vm_self_mappages` 映射堆页、`vm_self_unmap` 回滚；munmap（21）经 `vm_self_unmappages` 批量解映射。**没有递归风险**：页表页本身经 Direct Map 访问（`phys_to_ptr_dm`），不经过 HeapArena（vm_self_map.rs 头注释"# No recursion risk"）。

### 3.4 D4: `Paging` trait——层级与操作的统一抽象（[ARCH: A-2/A-10]）

**C 的问题**：操作与结构耦合——`pt_*` 函数族直接操作 `pt_t` 的字段，`#if defined(__i386__)` 散落各处（pagetable.c 的 `#if` 至少 8 处）。

**Rust 决策**：`Paging` trait（os/arch/src/arch/paging.rs:152-425）定义页表操作的统一契约：

```rust
pub trait Paging {
    const PAGE_SIZE: usize;                       // 4096
    fn new() -> Result<Self, PageTableError>;
    fn new_from_page(root_page: PhysBytes) -> Self;  // 自举：预分配根页
    fn from_active_root(root_phys: PhysBytes) -> Self; // 包装已活动页表
    unsafe fn enable(&self) -> PhysBytes;         // 加载根 + 开分页
    unsafe fn destroy(&mut self);
    fn map(&mut self, vaddr, paddr, flags) -> Result<(), PageTableError>;
    fn remap(&mut self, vaddr, paddr, flags) -> Result<Option<(PhysBytes, PageFlags)>, _>;
    fn unmap(&mut self, vaddr) -> Result<PhysBytes, PageTableError>;
    fn update_flags(&mut self, vaddr, flags) -> Result<(), PageTableError>;
    fn query(&self, vaddr) -> Option<(PhysBytes, PageFlags)>;
    fn root_paddr(&self) -> PhysBytes;
    unsafe fn switch(&self);
    unsafe fn flush_tlb(&self);
    unsafe fn flush_tlb_addr(&self, vaddr);
    fn map_range(&mut self, ...); fn unmap_range(&mut self, ...);
}
```

配套设计：

1. **`PageFlags`（u16 位标志）与硬件编码分离**：OS 语义（`PRESENT`/`WRITABLE`/`USER_ACCESSIBLE`/`EXECUTABLE`/`GLOBAL`/`NO_CACHE`…）与架构位布局解耦；x86-64 的 NX 反相、arm64 的 nG 反相等翻译封在 `flags_to_pte`/`pte_to_flags`（x86_64/paging.rs:80-113）。VM 层**永不接触 PTE 位域**（review-code-skill §2 硬件抽象强制）。
2. **`PageTableError` 6 变体**：`InvalidAddress`/`AlreadyMapped`/`NotMapped`/`AllocationFailed`/`PermissionDenied`/`NotSupported`——映射到页表操作的失败面；错误码对齐在 IPC 边界（15）完成，不在此 trait 内自创 errno。
3. **独立函数而非 trait 方法**：`clone_range`（paging.rs:426，对应 C `pt_copy`/`pt_map_in_range`）、`bind_to_process`（:464，对应 `pt_bind`）、`map_kernel`（:500，对应 `pt_mapkernel`）、`paging_init`（:569，对应 `pt_init` 结构继承）——这些是**跨页表组合操作**，不是单个架构的硬件机制，做成泛型函数复用 trait 方法（review-code-skill §2.5 机制 vs 策略分离）。
4. **`paging_init` 的结构继承**（paging.rs:569-650）：kernel 已为 VM 建好 1GB DM（`VM_DIRECT_MAP_BASE` 起）+ 内核映射；`paging_init` 在其上 `map_huge` 扩展 >1GB 物理内存 + `bind_to_process`——对应 C `pt_init` 的"从内核继承 + 扩展"结构。**注意差异**：C 拷贝内核页表内容（`sys_vircopy`/`sys_abscopy`），minix-rs 的 kernel 直接把 DM/内核映射建进 VM 页表，VM 只扩展 >1GB 部分（`MOCK_KERNEL_*` 常量 FIXME：真实 boot_info 接线归 01/10）。

### 3.5 语义差异清单（C ↔ Rust 诚实标注）

| # | C 行为 | Rust 行为 | 类型 |
|---|--------|----------|------|
| 1 | `pt_t` 四字段（VA 缓存 + PA 条目双视图） | `PageTable` 单一根 PA，VA 经 DM 派生 | [ARCH: A-1/A-2] 结构消除 |
| 2 | 固定 1024/4096 项数组，页表页预先分配 | 中间级按需 `walk_alloc` | [ARCH: A-2] 层级演进 |
| 3 | `pt_virtop` 死字段（仅写入无读取） | 无对应 | 结构消除 |
| 4 | `ARCH_VM_*` 宏族两套 + `#if` 切换 | trait 静态分派，VM 侧零条件编译 | [ARCH: A-10] |
| 5 | `pt_init` 拷贝内核页表内容 | `paging_init` 建立在 kernel 已建 DM 上，仅扩展 >1GB | [ARCH: A-1] 结构继承方式演进 |
| 6 | 自举用 BSS 静态备用资源 | 无（`vm_self_map` + Direct Map + `vm_pt_alloc`） | [ARCH: A-9/A-1]（06 D3/D5） |
| 7 | `PTF_*`/`I386_VM_*` 位标志直接暴露 | `PageFlags` OS 语义 + arch 内翻译 | 类型安全演进 |
| 8 | `PFERR_*` 页错误解码宏 | 16-pagefault 消费（cow_exec_pf.rs） | 移交（16） |

---

## 4. 实现详解

### 4.1 `pagetable/mod.rs`：模块入口与类型别名

```rust
// pagetable/mod.rs
pub(crate) mod vm_self_map;                                     // L18
pub(crate) type PageTable = minix_arch::CurrentPaging;         // L22
pub(crate) use minix_arch::paging::PageFlags;      // L24
pub(crate) use minix_arch::paging::PageTableError; // L25
pub(crate) use minix_arch::paging::Paging;         // L26
pub(crate) use vm_self_map::{vm_self_mappages, vm_self_unmappages, vm_self_unmap}; // L27

pub(crate) fn page_align(addr: VirBytes) -> VirBytes { ... }   // L29-32
pub(crate) fn page_align_down(addr: VirBytes) -> VirBytes { ... } // L34-37
```

- `PageTable` 别名是 VM 侧唯一入口：所有消费方（vmproc 的 `vm_pt`、`vm_self_map`、未来 fork/exit）都经它接触页表，不直接依赖 `minix_arch` 的具体类型。
- `page_align`/`page_align_down` 用 `<PageTable as Paging>::PAGE_SIZE` 取页大小（trait 常量），避免硬编码 4096。
- 模块头注释明言设计边界：**不暴露 PDE/PTE 位域与表索引**，硬件细节归 `minix_arch` 实现。

### 4.2 `direct_map.rs`：VM 侧 Direct Map 包装

```rust
// direct_map.rs
pub(crate) const VM_DIRECT_MAP_BASE: u64 = CurrentDirectMap::VM_DIRECT_MAP_BASE;   // L12
pub(crate) const KERNEL_DIRECT_MAP_BASE: u64 = CurrentDirectMap::KERNEL_DIRECT_MAP_BASE; // L13
pub(crate) const VM_DIRECT_MAP_SIZE: u64 = 1 << 30;                                // L14
pub(crate) const VM_HEAP_BASE: u64 = CurrentDirectMap::VM_HEAP_BASE;               // L16
pub(crate) const VM_HEAP_SIZE: u64 = CurrentDirectMap::VM_HEAP_SIZE;               // L17
pub(crate) const VM_HEAP_LIMIT: u64 = VM_HEAP_BASE + VM_HEAP_SIZE;                 // L18

pub(crate) fn vm_phys_to_virt(phys: AlignedPhysBytes) -> VirBytes { ... }          // L21-23
pub(crate) fn kernel_phys_to_virt(phys: AlignedPhysBytes) -> VirBytes { ... }      // L26-28
pub(crate) fn virt_to_phys(virt: VirBytes) -> AlignedPhysBytes { ... }             // L31-34
pub(crate) fn is_direct_map_virt(virt: VirBytes) -> bool { ... }                   // L37-39
```

- **输入用 `AlignedPhysBytes`**（物理页对齐新类型）：构造时强制页对齐（`AlignedPhysBytes::new` 断言、`new_unchecked` debug 断言），未对齐在构造点即暴露——C 里 `vm_phys_to_virt` 接受任意 `phys_bytes`，越界/未对齐在更晚的访问点才暴露。
- `is_direct_map_virt` 的双窗口判定（高半 kernel / 低半 VM）是 `virt_to_phys` 的方向选择器，也是后续 09/13 判断"这个 VA 是不是 DM 页"的哨兵。
- `VM_DIRECT_MAP_SIZE`（1GB）是 kernel 初始建好的 DM 窗口大小；>1GB 由 `paging_init` 扩展（§3.4）。

### 4.3 `vm_self_map.rs`：VM 自身页表接口

```rust
static VM_SELF_PT_STORAGE: AssumeSyncCell<Option<PageTable>> = AssumeSyncCell::new(None); // L55

fn get_pt_mut() -> &'static mut PageTable {                       // L63-71
    // SAFETY: 单线程事件循环，无并发 &mut（lib.rs 头注释）
    let opt = unsafe { &mut *VM_SELF_PT_STORAGE.get() };
    opt.as_mut().expect("vm_self_pt: not initialized — call init_vm_self_pt() first")
}

pub(crate) fn init_vm_self_pt() { ... }                           // L82-90（重复调用 panic）
pub(crate) fn vm_self_mappages(va: VirBytes, phys: PhysBytes, flags: PageFlags) -> Result<(), PageTableError> { ... } // L100-106
pub(crate) fn vm_self_unmap(va: VirBytes) -> Result<PhysBytes, PageTableError> { ... }  // L116-118
pub(crate) fn vm_self_query(va: VirBytes) -> Option<(PhysBytes, PageFlags)> { ... }    // L127-129
pub(crate) fn vm_self_unmappages(va_start: VirBytes, pages: usize) -> Result<(), PageTableError> { ... } // L138-143
```

- **初始化时机**：`vm_server.rs:new_with_boot_params` 在 `#[cfg(not(test))]` 下调用（L110-111）——测试构建用 `MockPaging`，不需要真实页表；生产构建在 `pt_alloc` 注册（06 D5）之后、`VmServer::init()` 之前。
- **`vm_self_unmap` 返回被解映射的 PA**：调用者可据此释放物理页（HeapArena::shrink 用）——比 C 的 `pt_writemap(MAP_NONE, WMF_FREE)` 一体化解映射+释放更显式（06 D1 已述拆分）。
- **`vm_self_query` 是 `vm_addrok` 语义的 Rust 对应**（06 D6 移交）：返回映射的 (PA, flags)，供校验/查询。

### 4.4 arch 侧：`DirectMapArch` / `Paging` / `X86_64Paging` 4 级 walk

**`DirectMapArch` 三实现**（os/arch/src/arch/direct_map.rs:61-146）：`X86_64DirectMap`/`AArch64DirectMap`/`Riscv64DirectMap` 各定义 BASE 常量，转换函数用 trait 默认实现（§3.2 表）；`MockDirectMap` 用 `AtomicU64` 存可配置 base（测试隔离，`set_mock_vm_base`）。

**`X86_64Paging` 的 4 级结构**（os/arch/src/x86_64/paging.rs）：

```rust
const PML4_SHIFT: u32 = 39;   // L38
const PDPT_SHIFT: u32 = 30;   // L39
const PD_SHIFT: u32 = 21;     // L40
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;  // L41（52 位物理地址）

fn pml4_index(vaddr: u64) -> usize { ((vaddr >> PML4_SHIFT) & 0x1FF) as usize }  // L43-45
fn pdpt_index(vaddr: u64) -> usize { ((vaddr >> PDPT_SHIFT) & 0x1FF) as usize }  // L47-49
fn pd_index(vaddr: u64) -> usize { ((vaddr >> PD_SHIFT) & 0x1FF) as usize }      // L51-53
```

- **`walk_alloc`**（L277-330）：从 PML4 逐级 walk，遇到未分配的中间表用 `crate::pt_alloc::alloc_pt_page()`（06 D5 注册的 VM 侧供给）分配 + Direct Map 清零，最后返回叶子 PTE 的物理地址。**这是 A-2 的运行时载体**：4 级表按需生长。
- **`phys_to_ptr_dm`**（L120-123）：`KERNEL_DIRECT_MAP_BASE + phys` 得到内核 VA 指针——PTE 读写统一经它（`write_pte_dm`/`read_pte_dm`），**页表页的"VM 视角"由 DM 派生，不再有 `pt_pt[]` 缓存**。
- **arm64/riscv64 同构**：各自的 `walk_alloc` + `phys_to_ptr_dm` 变体（arm64/paging.rs:160、riscv64/paging.rs:195）——trait 契约相同，索引/位域不同，全部封在 arch crate。

**`paging_init` 的结构继承**（os/arch/src/arch/paging.rs:569-650）：`map_kernel`（内核文本 + kernel DM 哨兵页）→ 若 `total_phys_bytes > 1GB` 则 `map_huge` 扩展 VM DM → `bind_to_process` 通知内核。对应 C `pt_init` 的"继承内核映射 + 登记 + 绑定"（§2.5 ⑥），差异在 §3.5 #5 已注。

### 4.5 消费方接线

| 消费方 | 位置 | 使用 |
|--------|------|------|
| `vm_server.rs:new_with_boot_params` | L102-111 | `pt_alloc` 注册 + `init_vm_self_pt`（06 D5 接线） |
| `heap_arena.rs` | L83-118 | `vm_self_mappages` 映射堆页、`vm_self_unmap` 回滚（09 详述） |
| `munmap.rs` | L111 | `vm_self_unmappages` 批量解映射（21 详述） |
| `alloc_page.rs` | L30-36 | `vm_phys_to_virt` 给自用页派生 VA（06 D2） |
| `cow_exec_pf.rs` | L197-198 | `vm_phys_to_virt` 拷贝物理页内容（17 详述） |
| `phys_mem/*` | bitmap/buddy/segment_tree | `vm_phys_to_virt` 访问分配器元数据（05 详述） |
| `vmproc/vmproc.rs` | L40 | `MaybeUninit<PageTable>` 存进程页表（02 详述） |

---

## 5. 测试要点

### 5.1 单元测试清单（`os/servers/vm/src/` + `os/arch/src/`）

| 测试函数 | 位置 | 验证目标 |
|----------|------|---------|
| `test_page_align` | pagetable/mod.rs:44 | 向上/向下页对齐（0x1234→0x2000/0x1000） |
| `test_page_size_from_trait` | pagetable/mod.rs:51 | `PAGE_SIZE` 经 trait 常量取 4096 |
| `test_vm_phys_to_virt` | direct_map.rs:85 | `vm_phys_to_virt(0x1000) = VM_DIRECT_MAP_BASE+0x1000` |
| `test_vm_phys_to_virt_with_real_constant` | direct_map.rs:94 | 真实常量 + roundtrip |
| `test_kernel_phys_to_virt` | direct_map.rs:104 | `kernel_phys_to_virt` 高半窗口 |
| `test_virt_to_phys_roundtrip` | direct_map.rs:111 | 双窗口 roundtrip |
| `test_is_direct_map_virt` | direct_map.rs:120 | DM 判定（含非 DM 地址拒绝） |
| `test_vm_self_pt_not_initialized_by_default` | vm_self_map.rs:150 | 静态存储初始 `None` |
| x86_64/paging.rs 11 个（`test_flag_roundtrip_*`×3、`test_nx_*`×2、`test_address_preserved_*`、`test_huge_page_flag_roundtrip`、`test_pml4_index_high_canonical`、`test_pdpt_index_1gb_boundary`、`test_pd_index_2mb_boundary`、`test_walk_read_not_present_on_zero_root`） | os/arch/src/x86_64/paging.rs:633-721 | PTE 标志翻译（NX 反相）、4 级索引、huge page 标志、零根 walk |

### 5.2 覆盖维度

- **Direct Map 双向转换**：`vm_phys_to_virt`/`virt_to_phys` roundtrip + 双窗口判定——A-1 的核心不变量（`virt_to_phys(vm_phys_to_virt(p)) == p`）。
- **结构访问**：`page_align`/`PAGE_SIZE` trait 常量——VM 层页大小统一入口。
- **4 级页表结构**：x86_64/paging.rs 的索引函数（PML4/PDPT/PD 边界值：canonical 高半、1GB/2MB 边界）+ 零根 walk（PRESENT=0 检测）——A-2 的层级正确性。
- **VM 自映射状态**：`vm_self_map` 静态存储初始 `None`——A-9 模块的初始化状态机。
- **mock 隔离**：`with_mock_base_lock`/`with_custom_mock_base` 串行化依赖全局 mock base 的测试，防并行竞态（direct_map.rs:56-80）。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 说明 | 状态 |
|------|------|------|
| `init_vm_self_pt` 生产路径 | `#[cfg(not(test))]` 跳过；`X86_64Paging::new()` + 真实 DM 需 QEMU 集成 | 06 已注，backlog（QEMU） |
| `vm_self_map` map/unmap roundtrip | 无独立测试；映射路径经 HeapArena 测试覆盖（09 详述） | 09 承接 |
| `paging_init` 真实 boot_info | `MOCK_KERNEL_*` 常量 FIXME（paging.rs:601-604），真实内核布局接线归 01/10 | 01/10 承接 |
| `vm_self_query` 无生产消费方 | `vm_addrok` 语义对应（06 D6 移交），当前仅定义供校验/查询；页表级 sanity 检查（sanity.rs:26-28 注释的 `map_sanitycheck_pt` 延后项）是预期消费方 | 接受（预留接口，诚实标注） |
| `PagingWithId`（PCID/ASID） | 预留 trait，无生产消费方 | 接受（draft/06-pagetable-struct.md §3.3（素材）预留声明） |
| `PFERR_*` 页错误解码 | 无 Rust 对应（16-pagefault 消费方接管） | 16 承接 |

### 5.4 测试统计（截至 2026-08-15）

- 本文档范围 Rust 测试：`pagetable/mod.rs` 2 + `direct_map.rs` 5 + `vm_self_map.rs` 1 + `os/arch x86_64/paging.rs` 11 = **19 个**（含 2 个 mock helper 函数）。
- 全 crate 基线：`cargo test -p minix-vm --lib` = **347 passed / 1 failed**（`region::vir_region::tests::test_map_lazy`，13 范围 pre-existing）。`os/arch` crate 测试在 `cargo test -p minix-arch` 下运行。
- 统计规则：不引用具体文件行号（避免行号漂移传播，Pattern #66 RCPD 主动应用）。

---

## 6. 过渡

本文档在启动时序中的位置：`init_vm()` → `init_proc(VM_PROC_NR) + pt_init()`（main.c:474-475）——**VM 页表从"内核给的初始映射"变成"VM 自己管理的结构"**。它把 06 的页分配（`vm_pt_alloc` 供给页表页）接进页表结构（`PageTable` 按需 walk），并为 08 的页表操作准备了结构基础：

```
05（物理分配器）→ 06（VM 自用页分配）→ 07（页表结构：pt_t/Direct Map/自映射）→ 08（页表操作）
```

**下一篇入口**：08-pagetable-ops 承接 `pt_new`/`pt_bind`/`pt_writemap`/`pt_copy`/`pt_mapkernel` 等**操作面**逐函数语义——本文档 §2.7 只给了 `pt_bind`/`pt_mapkernel` 的"结构角色"，08 将给出它们的完整行为契约（错误路径/标志翻译/`WMF_*` 映射到 `map`/`remap`/`update_flags`）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-page-allocator.md` — 页分配 + Direct Map 概念首次引入（§1.5）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/08-pagetable-ops.md` — 页表操作（下一篇）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` — `init_vm`/`pt_init` 调用点
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/05-physical-memory.md` — 物理分配器
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/09-slab-allocator.md` — HeapArena 消费 `vm_self_mappages`（09）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/02-vmproc-struct.md` — `VmProc::vm_pt` 存储
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/07-cross-space-init.md` — Direct Map 双窗口论证（kernel 侧）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/06-pagetable-struct.md` — 旧主线素材（素材）
- `minix3/minix/servers/vm/pt.h`、`minix3/minix/servers/vm/arch/i386/pagetable.h`、`minix3/minix/servers/vm/arch/earm/pagetable.h`、`minix3/minix/servers/vm/pagetable.c` — C 源码（ground truth）
- `os/servers/vm/src/pagetable/mod.rs`、`os/servers/vm/src/direct_map.rs`、`os/servers/vm/src/pagetable/vm_self_map.rs`、`os/arch/src/arch/direct_map.rs`、`os/arch/src/arch/paging.rs`、`os/arch/src/x86_64/paging.rs` — Rust 实现
