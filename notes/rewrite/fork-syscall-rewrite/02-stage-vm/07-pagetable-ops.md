# 07-pagetable-ops: 页表操作

> **分类**: VM库  
> **源码**: [pagetable.c](minix3/minix/servers/vm/pagetable.c)  
> **说明**: 页表的生命周期管理，包括创建、绑定、映射等操作

> ⚠️ **设计目标标注**: 本文档第 4-5 章中的 Rust API 为架构设计方向，供后续实现参考。当前实现使用 `VmProc` + `Paging` trait + `VmPagingExt` trait。

---

## 1. 概述

### 1.1 页表操作的作用

页表操作负责管理进程地址空间的生命周期，是虚拟内存管理的核心功能：

**核心操作**:
- **创建页表**: 为新进程分配页目录和初始页表
- **建立映射**: 将虚拟地址映射到物理页
- **绑定进程**: 将页表与进程关联，激活地址空间
- **释放页表**: 进程退出时释放资源

**使用场景**:
| 场景 | 操作 |
|------|------|
| fork | 创建新页表，复制父进程映射（共享物理页，CoW） |
| exec | 清除旧地址空间，创建新页表，映射新程序内存 |
| exit | 释放内存区域、物理页引用、页表 |
| mmap | 建立新的虚拟地址映射 |
| boot | VM 启动时为 boot 进程创建页表 |

### 1.2 生命周期管理

**页表生命周期**:

```
fork/boot 生命周期:
  创建(pt_new) ──► 映射(map_proc_copy) ──► 绑定(pt_bind) ──► [运行中]

exec 生命周期:
  [运行中] ──► 释放(free_proc) ──► 创建(pt_new) ──► 绑定(pt_bind) ──► 映射(handle_memory) ──► [运行中]

exit 生命周期:
  [运行中] ──► 释放(free_proc)
```

> **注意**: fork 和 exec 的页表操作顺序不同。fork 中映射先于绑定（先复制父进程映射，再注册到 `pagedir_mappings`）；exec 中绑定先于映射（先清除旧空间、创建新页表并绑定，再由 VFS 驱动逐步映射新程序内存）。

**各阶段说明**:

| 阶段 | Minix3 函数 | 调用场景 |
|------|-------------|----------|
| 创建 | `pt_new()` | fork、boot、exec（`VMPPARAM_CLEAR`） |
| 映射 | `pt_writemap()` | fork（通过 `map_proc_copy`）、mmap、exec（`VMPPARAM_HANDLEMEM`） |
| 绑定 | `pt_bind()` | fork、boot、VM 热更新、exec（`VMPPARAM_CLEAR`） |
| 释放 | `pt_free()` | 进程退出（`free_proc`）、exec（`VMPPARAM_CLEAR`） |

**注意**: 
- `pt_bind` 的作用是将页目录物理地址登记到 `pagedir_mappings`，使 VM 和内核都能通过虚拟地址访问该进程的页目录，并通知内核更新进程的地址空间。
- exec 由 PM、VFS、VM 三方协作完成：PM 负责进程元数据和协调，VFS 负责加载 ELF 文件，VM 通过 `VMPPARAM_CLEAR`（清除旧地址空间、创建新页表并绑定）和 `VMPPARAM_HANDLEMEM`（映射新程序内存）参与地址空间的重建。

**关键概念**:
- **页目录 (Page Directory)**: 一级页表，存储页表物理地址
- **页表 (Page Table)**: 二级页表，存储页框物理地址
- **页框 (Page Frame)**: 物理内存页，4KB 对齐
- **映射 (Mapping)**: 虚拟地址到物理地址的对应关系

### 1.3 核心架构问题：内核如何访问任意进程的页目录

> ⚠️ **阅读提示**: 本节描述的 `pagedir_mappings` 机制是 Minix3 在 32 位 x86 地址空间约束下的产物。minix-rs 目标为 x86-64，采用直接映射区方案（详见 3.0 节），不再使用此机制。本节粗略阅读即可，无需深究实现细节。

理解 Minix3 页表操作的前提是理解一个架构层面的根本矛盾：

x86/ARM/RISC-V 一旦开启分页，所有内存访问都必须经过 MMU，CPU 没有任何机制可以绕过页表直接用物理地址读写。无论运行在最高特权级还是用户态，CPU 访问内存时，MMU 都会将地址当作虚拟地址，通过当前页表翻译成物理地址。

而Minix3 内核在运行过程中需要读写其他进程的页目录（需要页目录的虚拟地址）：

- **跨进程内存拷贝**：`sys_datacopy` 在两个进程之间复制数据时，内核需要访问目标进程的页表来建立临时映射窗口。
- **虚拟地址翻译**：内核函数 `vm_lookup` 将目标进程的虚拟地址翻译为物理地址时，需要读取目标进程的页目录和页表。

（以上场景的详细实现机制见[附录 A](#附录-a-内核访问页目录的实现细节)）

因此，内核要读写某个进程的页目录，必须有一条虚拟地址到该页目录物理地址的映射。内核虽然持有页目录的物理地址（`p_seg.p_cr3`），但物理地址只能用于加载 CR3 寄存器——这是硬件操作，CPU 直接将物理地址写入 CR3，不经过 MMU。内核要**读写**页目录的内容，仍需虚拟地址。

**Minix3 内核运行时使用当前进程的页表，且没有建立物理内存的直接映射。** 当进程 A 正在运行时，内核看到的地址空间是进程 A 的页表，VM 地址空间中的 `pt->pt_dir` 虚拟地址在进程 A 的页表中可能指向完全不同的物理页，或者根本不存在。

VM 自身不存在这个问题。`pt_t` 结构中的 `pt_dir` 和 `pt_pt[i]` 分别是页目录和二级页表在 VM 地址空间中的虚拟地址，VM 可以直接通过它们读写任意进程的页表，就像操作普通数组一样：

```c
pt->pt_dir[pde] = ...;     // VM 直接写页目录项
pt->pt_pt[pde][pte] = ...; // VM 直接写页表项
```

Minix3 的解决方案是引入一个**全局页目录登记册**——一个特殊的页表（`page_directories`），其中存储所有进程页目录的物理地址。这个页表通过 `pt_mapkernel` 被映射到每个进程的页目录中，使得无论当前哪个进程在运行，内核都能通过一致的虚拟地址窗口访问任意进程的页目录。

**机制概述**:

1. VM 维护一组特殊的页表（`page_directories`），这些页表的内容不是普通的虚拟地址到物理地址的映射，而是**所有进程页目录的物理地址**。
2. 在 `pt_mapkernel` 阶段，这些 `page_directories` 页表被映射到**每个进程的页目录**中（包括 VM 自身）。这意味着，无论是 VM 还是内核，只要通过当前进程的页表，就能访问到这份全局登记册。
3. 当新进程的页表创建后，`pt_bind` 将新进程页目录的物理地址写入登记册的对应槽位。此后，内核可以通过计算出的虚拟地址 `p_cr3_v`，像访问普通内存一样读取该进程的页目录。

**归属关系**: `pagedir_mappings` 数组和 `page_directories` 页表的创建、初始化、更新逻辑全部在 VM 中。VM 是这个数据结构的管理者。但从映射角度看，`page_directories` 页表被映射到每个进程的页目录中，就像内核代码段一样——语义上它出现在每个进程的页表中，但本质上是内核与 VM 共享的数据结构，其虚拟地址在内核运行时有效。

**地址空间关系**:

| 虚拟地址区域 | VM 地址空间 | 进程 A 地址空间 | 进程 B 地址空间 | 物理页 |
|-------------|------------|----------------|----------------|--------|
| 高地址区 | 内核代码（大页） | 内核代码（大页） | 内核代码（大页） | 同一份 |
| 中地址区 | `page_directories`（全局登记册） | `page_directories`（全局登记册） | `page_directories`（全局登记册） | 同一份 |
| 低地址区 | VM 自身内存 | 进程 A 用户内存 | 进程 B 用户内存 | 各自独立 |

`page_directories` 页表在所有进程的地址空间中都映射到相同的物理页。无论当前哪个进程在运行，内核都能通过当前页表中的该区域读出任意进程的页目录物理地址，进而访问目标进程的页目录。

**两套虚拟地址，同一份物理数据**: 页目录的物理内存只有一份，但有两个虚拟地址入口：

| 入口 | 虚拟地址 | 使用者 | 用途 |
|------|---------|--------|------|
| `pt->pt_dir` | VM 地址空间中的虚拟地址 | VM | VM 直接读写页目录和页表 |
| `p_cr3_v` | 通过 `pagedir_mappings` 计算出的虚拟地址 | 内核 | 内核通过窗口访问页目录 |

VM 不需要 `pagedir_mappings` 就能操作页表（它有 `pt->pt_dir`），但内核没有 VM 地址空间中的映射，所以必须依赖 `pagedir_mappings` 提供的窗口。`pt_bind` 的核心作用就是将页目录物理地址写入登记册，使内核获得访问能力。

**与 Linux 的关键差异**:

在 Linux 等单内核系统中，内核在启动时建立所有物理内存的直接映射区（`PAGE_OFFSET` 开始的区域），可以随时随地通过 `phys_to_virt()` 访问任意物理页，因此内核代码可以直接修改任意进程的页表。Minix3 的微内核架构将内存管理策略移至用户态的 VM 服务器，内核仅提供页表切换、TLB 刷新等底层机制，且不建立物理内存直接映射。`pagedir_mappings` 正是弥合这一架构鸿沟的关键设计。

> **注意**: Minix3 不使用 x86 的递归页目录（self-mapping）技巧。递归页目录通过在页目录的最后一个条目指向页目录自身，使得可以通过固定的虚拟地址范围访问页目录和页表。Minix3 选择了 `pagedir_mappings` 方案，将所有进程的页目录集中管理在一个独立的页表中。

### 1.4 Minix3 源码位置

| 文件 | 说明 |
|------|------|
| `minix/servers/vm/pagetable.c` | 页表操作实现 |
| `minix/servers/vm/pt.h` | 页表结构定义 |
| `minix/servers/vm/arch/i386/pagetable.h` | x86 架构相关宏定义 |
| `minix/servers/vm/arch/earm/pagetable.h` | ARM 架构相关宏定义 |

---

## 2. Minix3 源码分析

```c
typedef struct {
    u32_t *pt_dir;                      /* 页目录在 VM 地址空间中的虚拟地址 */
    u32_t pt_dir_phys;                  /* 页目录物理地址 */
    u32_t *pt_pt[ARCH_VM_DIR_ENTRIES];  /* 二级页表在 VM 地址空间中的虚拟地址 */
    u32_t pt_virtop;                    /* 虚拟地址空间空洞查找的起始位置 */
} pt_t;
```

> **页表结构 `pt_t`** 详见 [06-pagetable-struct.md](06-pagetable-struct.md#21-页表结构体-pt_t)。

### 2.1 页表创建与销毁

#### 2.1.1 pt_new - 创建新页表

**源码位置**: `minix/servers/vm/pagetable.c:990`

**解决的问题**: 当 fork 或 boot 需要创建一个新进程时，首先需要一张空白的页表。`pt_new` 完成页目录的分配和初始化，并建立必要的基础映射。

**执行步骤**:

1. **分配页目录物理页**: 调用 `vm_allocpages` 获取一个页对齐的物理页作为页目录。VM 同时获得该页的物理地址（`pt_dir_phys`，供 CPU 的 CR3 使用）和虚拟地址（`pt_dir`，供 VM 代码访问）。
2. **初始化页目录**: 将所有页目录项清零（PRESENT 位为 0，表示尚未建立映射），所有页表指针置为 NULL。
3. **映射内核空间**: 调用 `pt_mapkernel`，将内核代码段、`pagedir_mappings` 登记册、以及内核特殊映射写入新页表。这一步确保新进程能够执行系统调用，且内核能够通过登记册访问该进程的页目录。

**源码**:

```c
int pt_new(pt_t *pt)
{
    int i, r;

    /* 分配页目录（页对齐），不重复分配 */
    if(!pt->pt_dir &&
      !(pt->pt_dir = vm_allocpages((phys_bytes *)&pt->pt_dir_phys,
        VMP_PAGEDIR, ARCH_PAGEDIR_SIZE/VM_PAGE_SIZE))) {
        return ENOMEM;
    }

    /* 检查页目录物理地址是否页对齐 */
    assert(!((u32_t)pt->pt_dir_phys % ARCH_PAGEDIR_SIZE));

    /* 初始化页目录项为 0（PRESENT bit = 0） */
    for(i = 0; i < ARCH_VM_DIR_ENTRIES; i++) {
        pt->pt_dir[i] = 0;
        pt->pt_pt[i] = NULL;
    }

    /* 冗余字段，初始化后从未被读取 */
    pt->pt_virtop = 0;

    /* 映射内核空间 */
    if((r=pt_mapkernel(pt)) != OK)
        return r;

    return OK;
}
```

**设计要点**:

- **页目录不重新分配**: 一旦某个进程槽位的页目录被分配，就不会再释放或重新分配。源码注释指出两个原因：(1) 略微提高性能，避免重复分配；(2) 避免更新内核页表中指向页目录的映射（`page_directories` 数据）。这意味着页目录的生命周期与进程槽位绑定，而非与进程绑定。
- **每个页表必须映射内核**: Minix3 内核在系统调用、中断等场景下运行时，不切换页表，而是直接使用当前进程的页表。因此每个进程的页目录中必须包含内核映射，否则内核代码无法执行。内核映射使用 `PTF_GLOBAL` 标志，在进程切换时对应的 TLB 条目不会被刷新。
- **`pt_virtop` 为冗余字段**: 被初始化为 0，但实际未被使用。详见 [06-pagetable-struct.md](06-pagetable-struct.md#22-pt_virtop---冗余字段)。

> **pt_mapkernel 详情**: `pt_mapkernel`（[pagetable.c:1442](minix3/minix/servers/vm/pagetable.c#L1442)）执行三段映射：
> 1. **内核代码段**: 从 `kern_mb_mod->mod_start` 开始，以 4MB 大页（x86）或 1MB section（ARM）映射 `kern_size` 字节。x86 使用 `ARCH_VM_BIGPAGE` 标志，无需二级页表。
> 2. **页目录登记册**: 遍历 `pagedir_mappings` 数组，将每个 `pdm` 的 PDE 写入页目录。这些 PDE 指向 `page_directories` 页表，使内核能通过该窗口访问所有进程的页目录。
> 3. **内核特殊映射**: 遍历 `kern_mappings` 数组，通过 `pt_writemap` 建立映射。这些映射由内核在启动时通过 `sys_vmctl_get_mapping` 提供，包括视频内存、APIC、用户态可访问的内核代码段（`usermapped`）等。

#### 2.1.2 pt_free - 释放页表

**源码位置**: `minix/servers/vm/pagetable.c:1427`

**解决的问题**: 进程退出时，释放页表占用的内存资源。注意 `pt_free` 仅释放二级页表，不释放页目录和物理页框。

```c
void pt_free(pt_t *pt)
{
    int i;

    /* 释放所有已分配的页表 */
    for(i = 0; i < ARCH_VM_DIR_ENTRIES; i++)
        if(pt->pt_pt[i])
            vm_freepages((vir_bytes) pt->pt_pt[i], 1);

    return;
}
```

**关键细节**:
- 只释放二级页表（`pt->pt_pt[i]`），不释放页目录。页目录与进程槽位绑定，可能被后续占用同一槽位的进程复用。
- 不释放物理页框：进程实际使用的物理内存（代码段、数据段、堆、栈等）已在 `map_free_proc` 中释放（`exit.c:35`），`pt_free` 在其后调用（`exit.c:36`）。
- `pagedir_mappings` 中的旧条目不会被显式清除。当进程槽位被新进程复用时，`pt_bind` 会直接覆盖旧条目。

#### 2.1.3 全局结构：pagedir_mappings 数组

**源码位置**: `minix/servers/vm/pagetable.c:38`

`pagedir_mappings` 是贯穿多个页表操作函数的全局数据结构，在阅读 `pt_bind` 之前需要先理解其定义和初始化。

> **术语说明**: PDE（Page Directory Entry）是一级页表项，存储二级页表的物理地址；PTE（Page Table Entry）是二级页表项，存储物理页框地址。x86 两级页表结构为：页目录（PDE 数组）→ 页表（PTE 数组）→ 物理页。

```c
#define MAX_PAGEDIR_PDES 5
static struct pdm {
    int       pdeno;              // 该页表映射在页目录的哪个 PDE 位置
    u32_t     val;                // PDE 值（物理地址 | 标志位）
    phys_bytes phys;              // page_directories 页表的物理地址
    u32_t     *page_directories;  // page_directories 页表的虚拟地址
} pagedir_mappings[MAX_PAGEDIR_PDES];
```

**各字段含义**:

- `pdeno`: 固定的 PDE 索引号，在所有进程的页目录中占据此位置。由 `freepde()` 从内核预留的 PDE 范围分配，避免与用户空间冲突。
- `val`: 写入页目录的 PDE 值，由 `phys` 加上权限标志（PRESENT、RW 等）构造而成。
- `phys`: `page_directories` 页表的物理地址。
- `page_directories`: `page_directories` 页表的虚拟地址，VM 通过此指针读写页表内容。该页表的每个条目存储一个进程页目录的物理地址。

**初始化过程**（[pagetable.c:1035](minix3/minix/servers/vm/pagetable.c#L1035) `pt_allocate_kernel_mapped_pagetables`）:

1. 为每个 `pdm` 分配一个 PDE 编号（通过 `freepde()`）。
2. 分配一个物理页作为 `page_directories` 页表，内容清零。此函数仅在 VM 初始化时调用一次，liveupdate 时会重新调用一次以切换到动态内存。
3. 构造 PDE 值 `val`，包含物理地址和权限标志。

**槽位计算规则**:

每个 `page_directories` 页表的大小为 `VM_PAGE_SIZE`（4KB），每个条目 4 字节，因此一个页表可容纳 `ARCH_VM_PT_ENTRIES` 个条目。但一个进程的页目录可能占用多个条目（取决于 `ARCH_PAGEDIR_SIZE`）：

- **x86**: `ARCH_PAGEDIR_SIZE = 4KB`，`pages_per_pagedir = 1`，`slots_per_pde = 1024 / 1 = 1024`。每个 `pdm` 可管理 1024 个进程。
- **ARM**: `ARCH_PAGEDIR_SIZE = 16KB`，`pages_per_pagedir = 4`，`slots_per_pde = 256 / 4 = 64`。每个 `pdm` 可管理 64 个进程。

进程槽位 `procslot` 到 `pdm` 索引和槽内偏移的映射：
- `pdm_index = procslot / slots_per_pde`
- `pdeslot = procslot % slots_per_pde`

`MAX_PAGEDIR_PDES = 5`，因此系统最多支持 5 × 1024 = 5120 个进程（x86）或 5 × 64 = 320 个进程（ARM）。

**访问示例**:

假设进程槽位 `procslot = 100`（x86），访问该进程页目录：

```c
// VM 视角：通过 pt_dir 直接访问
pt_t *pt = &vmproc[procslot].vm_pt;
u32_t *pagedir = pt->pt_dir;           // VM 的虚拟地址
pagedir[0] = new_pde_value;            // 修改页目录项

// 内核视角：通过 pagedir_mappings 访问
int slots_per_pde = 1024;              // x86
int pdm_index = 100 / 1024;            // = 0
int pdeslot = 100 % 1024;              // = 100
struct pdm *pdm = &pagedir_mappings[0];
int pdeno = pdm->pdeno;                // 例如 768

// 内核虚拟地址 = PDE索引 * 4MB + 槽内偏移 * 4KB
u32_t *kernel_vaddr = (u32_t *)(pdeno * 4 * 1024 * 1024 + pdeslot * 4 * 1024);
kernel_vaddr[0] = new_pde_value;       // 内核修改页目录项
```

VM 通过 `pt->pt_dir` 直接访问页目录（VM 的虚拟地址）。内核通过 `pagedir_mappings` 建立的映射窗口访问，虚拟地址由 PDE 索引和槽内偏移计算得出。

### 2.2 页表绑定

#### 2.2.1 pt_bind - 绑定到进程

**源码位置**: `minix/servers/vm/pagetable.c:1358`

**解决的问题**: 页表通过 `pt_new` 创建后，内核仍然无法访问它——新页目录的物理地址尚未登记到全局登记册中。`pt_bind` 完成两件事：(1) 将新页目录的物理地址写入 `pagedir_mappings` 登记册，使内核能够通过虚拟地址访问该页目录；(2) 通知内核记录该地址空间信息，并在必要时切换 CR3。

**执行步骤**:

1. **定位登记册槽位**: 根据进程的槽位号 `vm_slot`，计算出该进程在 `pagedir_mappings` 中的位置——属于哪个 `pdm`、在 `page_directories` 页表中的偏移（详见 2.1.3 节的槽位计算规则）。
2. **写入物理地址**: 取出页目录物理地址，屏蔽低位标志位后，加上 PRESENT 和 RW 权限标志，写入 `page_directories` 页表的对应槽位。对于 ARM，由于页目录为 16KB，需要循环填充 4 个连续的页表条目。
3. **计算访问用的虚拟地址**: 利用登记册所在的 PDE 编号和槽内偏移，计算出内核用来访问该进程页目录的虚拟地址 `pdes`。此地址可根据进程 slot 反算，VM 传递给内核是为了避免重复计算。
4. **通知内核**: 调用 `sys_vmctl_set_addrspace`，将页目录物理地址和虚拟地址 `pdes` 传递给内核。内核将其记录在进程控制块的 `p_seg.p_cr3` 和 `p_seg.p_cr3_v` 中。如果该进程正在当前 CPU 上运行，内核立即通过 `write_cr3` 加载新页表。

**关键源码**:

```c
int pt_bind(pt_t *pt, struct vmproc *who)
{
    int procslot, pdeslot;
    u32_t phys;
    void *pdes;
    int pagedir_pde;
    int slots_per_pde;
    int pages_per_pagedir = ARCH_PAGEDIR_SIZE/VM_PAGE_SIZE;
    struct pdm *pdm;

    slots_per_pde = ARCH_VM_PT_ENTRIES / pages_per_pagedir;

    assert(who);
    assert(who->vm_flags & VMF_INUSE);
    assert(pt);

    /* Step 1: 定位登记册槽位 */
    procslot = who->vm_slot;
    pdm = &pagedir_mappings[procslot/slots_per_pde];
    pdeslot = procslot%slots_per_pde;
    pagedir_pde = pdm->pdeno;

    /* Step 2: 提取物理地址 */
#if defined(__i386__)
    phys = pt->pt_dir_phys & ARCH_VM_ADDR_MASK;
#elif defined(__arm__)
    phys = pt->pt_dir_phys & ARM_VM_PTE_MASK;
#endif

    /* Step 3: 写入登记册 */
#if defined(__i386__)
    pdm->page_directories[pdeslot] =
        phys | ARCH_VM_PDE_PRESENT|ARCH_VM_PTE_RW;
#elif defined(__arm__)
    for (i = 0; i < pages_per_pagedir; i++) {
        pdm->page_directories[pdeslot*pages_per_pagedir+i] =
            (phys+i*VM_PAGE_SIZE)
            | ARCH_VM_PTE_PRESENT
            | ARCH_VM_PTE_RW
            | ARM_VM_PTE_CACHED
            | ARCH_VM_PTE_USER;
    }
#endif

    /* Step 4: 计算虚拟访问地址 */
    pdes = (void *) (pagedir_pde*ARCH_BIG_PAGE_SIZE + 
#if defined(__i386__)
            pdeslot * VM_PAGE_SIZE);
#elif defined(__arm__)
            pdeslot * ARCH_PAGEDIR_SIZE);
#endif

    /* Step 5: 通知内核 */
    return sys_vmctl_set_addrspace(who->vm_endpoint, pt->pt_dir_phys, pdes);
}
```

**地址视角总结**:

页目录的物理地址 `pt_dir_phys` 在不同实体中有不同的使用方式：

| 实体 | 访问页目录的方式 | 使用的地址 |
|------|-----------------|-----------|
| CPU 硬件 (MMU) | 从 CR3 寄存器加载，自动进行页表遍历 | `pt_dir_phys`（物理地址） |
| VM 服务器 | 通过 `pt->pt_dir` 直接访问 | `pt->pt_dir`（VM 地址空间中的虚拟地址） |
| 内核 | 通过 `pagedir_mappings` 窗口访问 | `p_cr3_v`（内核地址空间中的虚拟地址） |

**调用场景**:
| 场景 | 源码位置 | 说明 |
|------|----------|------|
| fork | `fork.c:94` | 子进程页表创建后，绑定到子进程 |
| boot | `main.c:355` | VM 启动时为 boot 进程绑定页表 |
| VM 初始化 | `main.c:211` | VM 初始化自身页表 |
| VM 热更新 | `main.c:717-718` | live update 交换新旧 VM 进程槽位后，分别重新绑定 |
| exec (VMPPARAM_CLEAR) | `exit.c:137` | 清除旧地址空间后创建新页表，绑定到进程 |
| VM 切换动态内存 | `pagetable.c:1329,1343` | `page_directories` 页表重新分配并清零，需重新填充进程页目录物理地址 |

### 2.3 页表映射操作

#### 2.3.1 pt_writemap - 建立映射

**源码位置**: `minix/servers/vm/pagetable.c:784`

**作用**: 在页表中写入映射（虚拟地址 → 物理地址），或修改/清除已有映射。这是 VM 最核心的页表操作函数。

**源码**:

```c
int pt_writemap(struct vmproc * vmp,
            pt_t *pt,
            vir_bytes v,
            phys_bytes physaddr,
            size_t bytes,
            u32_t flags,
            u32_t writemapflags)
{
    int p, pages;
    int verify = 0;
    int ret = OK;

    /* WMF_VERIFY: 仅验证页表项，不写入 */
    if(writemapflags & WMF_VERIFY)
        verify = 1;

    assert(!(bytes % VM_PAGE_SIZE));
    assert(!(flags & ~(PTF_ALLFLAGS)));

    pages = bytes / VM_PAGE_SIZE;

    /* MAP_NONE 表示清除映射（PRESENT 位为 0） */
    assert(physaddr == MAP_NONE || (flags & ARCH_VM_PTE_PRESENT));
    assert(physaddr != MAP_NONE || !flags);

    /* 预先分配所需的所有二级页表，避免中途失败后难以回滚 */
    ret = pt_ptalloc_in_range(pt, v, v + VM_PAGE_SIZE*pages, flags, verify);
    if(ret != OK) {
        printf("VM: writemap: pt_ptalloc_in_range failed\n");
        goto resume_exit;
    }

    /* 遍历每一页，写入页表项 */
    for(p = 0; p < pages; p++) {
        u32_t entry;
        int pde = ARCH_VM_PDE(v);  /* 页目录索引 */
        int pte = ARCH_VM_PTE(v);  /* 页表索引 */

        /* 确保二级页表已分配且不是大页 */
        assert(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT);
        assert(!(pt->pt_dir[pde] & ARCH_VM_BIGPAGE));
        assert(pt->pt_pt[pde]);

        /* WMF_WRITEFLAGSONLY/WMF_FREE: 仅修改标志位或释放，保留原物理地址 */
        if(writemapflags & (WMF_WRITEFLAGSONLY|WMF_FREE)) {
            physaddr = pt->pt_pt[pde][pte] & ARCH_VM_ADDR_MASK;
        }

        /* WMF_FREE: 释放物理页 */
        if(writemapflags & WMF_FREE) {
            free_mem(ABS2CLICK(physaddr), 1);
        }

        /* 构造页表项：物理地址 | 权限标志 */
        entry = (physaddr & ARCH_VM_ADDR_MASK) | flags;

        if(verify) {
            /* 验证模式：比较页表项是否匹配 */
            u32_t maskedentry = pt->pt_pt[pde][pte];
            // ... 比较逻辑，不匹配则返回 EFAULT
        } else {
            /* 写入页表项 */
            pt->pt_pt[pde][pte] = entry;
        }

        physaddr += VM_PAGE_SIZE;
        v += VM_PAGE_SIZE;
    }

    return OK;
}
```

**参数说明**:
| 参数 | 说明 |
|------|------|
| `vmp` | 进程指针（SMP 时用于 VMINHIBIT，可为 NULL） |
| `pt` | 目标页表 |
| `v` | 虚拟地址（页对齐） |
| `physaddr` | 物理地址，或 `MAP_NONE`（取消映射） |
| `bytes` | 映射大小（页大小的倍数） |
| `flags` | 页表项标志（`PTF_PRESENT`、`PTF_WRITE`、`PTF_USER` 等） |
| `writemapflags` | 操作模式（见下表） |

**writemapflags 标志**（[vm.h:56-59](minix3/minix/servers/vm/vm.h#L56-L59)）:
| 标志 | 值 | 说明 |
|------|-----|------|
| `WMF_OVERWRITE` | 0x01 | 允许覆盖已有映射 |
| `WMF_WRITEFLAGSONLY` | 0x02 | 只更新标志位，保留原物理地址 |
| `WMF_FREE` | 0x04 | 取消映射时释放物理页 |
| `WMF_VERIFY` | 0x08 | 验证页表项是否匹配预期值 |

**执行流程**:

1. **参数校验**: 大小页对齐、标志合法、`MAP_NONE` 时 flags 必须为 0
2. **分配页表**: `pt_ptalloc_in_range` 确保覆盖该地址范围的所有二级页表已分配
3. **写入页表项**: 逐页构造 `entry = physaddr | flags`，写入 `pt->pt_pt[pde][pte]`
4. **特殊模式**:
   - `WMF_WRITEFLAGSONLY`: 从现有 PTE 中取出物理地址，只更新标志位
   - `WMF_FREE`: 取出物理地址后调用 `free_mem` 释放物理页
   - `WMF_VERIFY`: 不写入，只比较现有 PTE 是否匹配预期值

**调用场景**:
| 场景 | 源码位置 | flags | writemapflags |
|------|----------|-------|---------------|
| 建立映射 | `region.c:280` | `PTF_PRESENT\|PTF_USER\|rw` | `WMF_OVERWRITE` |
| 取消映射 | `region.c:1139` | 0 | `WMF_OVERWRITE` |
| 验证映射 | `region.c:153` | `PTF_PRESENT\|PTF_USER\|rw` | `WMF_VERIFY` |
| 修改权限 | `pagetable.c:425` | 新 flags | `WMF_OVERWRITE\|WMF_WRITEFLAGSONLY` |
| 释放映射 | `mmap.c:500` | 0 | `WMF_OVERWRITE\|WMF_FREE` |

#### 2.3.2 pt_checkrange - 检查地址范围

**源码位置**: `minix/servers/vm/pagetable.c:943`

**作用**: 检查一段虚拟地址范围内的所有页表项是否有效（存在且有权限）。

```c
int pt_checkrange(pt_t *pt, vir_bytes v, size_t bytes, int write)
{
    int p, pages;

    assert(!(bytes % VM_PAGE_SIZE));
    pages = bytes / VM_PAGE_SIZE;

    for(p = 0; p < pages; p++) {
        int pde = ARCH_VM_PDE(v);
        int pte = ARCH_VM_PTE(v);

        /* 页目录项不存在 → EFAULT */
        if(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT))
            return EFAULT;

        /* 页表项不存在 → EFAULT */
        if(!(pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT))
            return EFAULT;

        /* 写权限检查 */
#if defined(__i386__)
        if(write && !(pt->pt_pt[pde][pte] & ARCH_VM_PTE_RW))
#elif defined(__arm__)
        if(write && (pt->pt_pt[pde][pte] & ARCH_VM_PTE_RO))
#endif
            return EFAULT;

        v += VM_PAGE_SIZE;
    }

    return OK;
}
```

**检查内容**:
1. 页目录项是否存在（PDE PRESENT 位）
2. 页表项是否存在（PTE PRESENT 位）
3. 写权限检查（x86 检查 RW 位，ARM 检查 RO 位）

**使用场景**: 仅在 `#if SANITYCHECKS` 条件编译下使用（`region.c:747`），属于调试断言。在 page fault 处理后验证映射是否正确建立。

#### 2.3.3 vm_mappages - 分配虚拟地址并建立映射

> **注意**: `vm_mappages` 在 Minix3 源码中位于 `pagetable.c:295`，但语义上属于"页表映射操作"。它由 `vm_allocpage` 调用，负责为物理页分配虚拟地址并建立映射。

**源码位置**: `minix3/minix/servers/vm/pagetable.c:295-320`

```c
void *vm_mappages(phys_bytes p, int pages)
{
	vir_bytes loc;
	int r;
	pt_t *pt = &vmprocess->vm_pt;

	/* Where in our virtual address space can we put it? */
	loc = findhole(pages);
	if(loc == NO_MEM) {
		printf("vm_mappages: findhole failed\n");
		return NULL;
	}

	/* Map this page into our address space. */
	if((r=pt_writemap(vmprocess, pt, loc, p, VM_PAGE_SIZE*pages,
		ARCH_VM_PTE_PRESENT | ARCH_VM_PTE_USER | ARCH_VM_PTE_RW
#if defined(__arm__)
		| ARM_VM_PTE_CACHED
#endif
		, 0)) != OK) {
		printf("vm_mappages writemap failed\n");
		return NULL;
	}

	if((r=sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK) {
		panic("VMCTL_FLUSHTLB failed: %d", r);
	}

	return (void *) loc;
}
```

**逐行解析**:

- **`findhole(pages)`**：在 VM 的虚拟地址空间中找到一个足够大的空洞。
- **`pt_writemap(vmprocess, pt, loc, p, VM_PAGE_SIZE*pages, flags, 0)`**：将物理地址 `p` 开始的 `pages` 页，映射到虚拟地址 `loc`。
  - **`ARCH_VM_PTE_PRESENT`**：页存在。
  - **`ARCH_VM_PTE_USER`**：用户态可访问。
  - **`ARCH_VM_PTE_RW`**：可读写。
  - **`ARM_VM_PTE_CACHED`**：ARM 架构的缓存标志。
- **`sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)`**：刷新 TLB。因为页表被修改了，CPU 的 TLB 中可能还有旧的映射，需要刷新。
- **`return (void *) loc`**：返回虚拟地址。

**与 `pt_writemap` 的关系**:

`vm_mappages` 是对 `pt_writemap` 的高级封装：

| 步骤 | `vm_mappages` | `pt_writemap` |
|------|---------------|---------------|
| 1 | 调用 `findhole` 分配虚拟地址 | 调用方负责分配虚拟地址 |
| 2 | 调用 `pt_writemap` 建立映射 | 直接建立映射 |
| 3 | 刷新 TLB | 调用方决定是否刷新 |

**TLB 刷新说明**:

`vm_mappages` 在修改页表后调用 `sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)` 刷新 TLB，这是必要的，因为：

1. **CPU 缓存旧映射**：x86/ARM 处理器的 TLB（Translation Lookaside Buffer）缓存了最近使用的虚拟地址到物理地址的映射。当页表被修改后，TLB 中可能仍保留旧的映射（或该虚拟地址之前未映射时的"无映射"状态）。
2. **VM 使用自身页表**：`vm_mappages` 将物理页映射到 VM 自身的地址空间（`vmprocess->vm_pt`），VM 随后会通过返回的虚拟地址访问这些页。如果 TLB 未刷新，CPU 可能使用缓存的旧映射，导致访问错误。
3. **与 `pt_writemap` 的区别**：`pt_writemap` 不自动刷新 TLB，因为调用场景多样——有时批量修改多个页表项后才需要一次刷新，有时修改的是其他进程的页表（当前未运行，无需立即刷新）。`vm_mappages` 作为高级封装，明确知道"映射已建立，即将使用"，因此负责刷新。

**注意**: 在 SMP 系统中，TLB 刷新需要广播到其他 CPU。Minix3 的 `VMCTL_FLUSHTLB` 会触发内核的跨核 TLB shootdown 机制。

**递归问题**: `vm_mappages` 调用 `pt_writemap`，后者在目标页表不存在时调用 `pt_ptalloc()` → `vm_allocpage()` → `vm_mappages()`，形成递归。这是 05-vm-allocpage.md 中分析的核心问题。

### 2.4 页表遍历

#### 2.4.1 pt_map_in_range - 范围内转移映射

**源码位置**: `minix/servers/vm/pagetable.c:631`

**作用**: 将源进程页表中指定范围的映射，复制到目标进程的页表中。只复制二级页表项（PTE），不涉及页目录。

```c
int pt_map_in_range(struct vmproc *src_vmp, struct vmproc *dst_vmp,
    vir_bytes start, vir_bytes end)
{
    int pde, pte;
    vir_bytes viraddr;
    pt_t *pt, *dst_pt;

    pt = &src_vmp->vm_pt;       /* 源进程页表 */
    dst_pt = &dst_vmp->vm_pt;   /* 目标进程页表 */

    end = end ? end : VM_DATATOP;  /* 默认扫描到用户空间顶部 */

    /* 遍历地址范围内的每一页 */
    for(viraddr = start; viraddr <= end; viraddr += VM_PAGE_SIZE) {
        pde = ARCH_VM_PDE(viraddr);

        /* 跳过不存在的页目录项 */
        if(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT)) {
            if(viraddr == VM_DATATOP) break;
            continue;
        }

        pte = ARCH_VM_PTE(viraddr);

        /* 跳过不存在的页表项 */
        if(!(pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT)) {
            if(viraddr == VM_DATATOP) break;
            continue;
        }

        /* 复制页表项到目标进程 */
        dst_pt->pt_pt[pde][pte] = pt->pt_pt[pde][pte];
        assert(dst_pt->pt_pt[pde]);

        if(viraddr == VM_DATATOP) break;
    }

    return OK;
}
```

**关键细节**:
- 只复制 PTE（二级页表项），不分配新的物理页
- 源进程的 PDE/PTE 不存在时会被跳过
- **前置条件**：目标进程的二级页表（`dst_pt->pt_pt[pde]`）必须已分配。代码中 `assert(dst_pt->pt_pt[pde])` 在写入之后检查，位置不当，但实际运行时因调用场景保证前置条件成立，不会出问题。

**调用场景**:

| 场景 | 源码位置 | 说明 |
|------|----------|------|
| VM 热更新（堆/mmap） | `utility.c:326` | 转移 VM 的堆和 mmap 区域映射 |
| VM 热更新（栈） | `utility.c:331` | 转移 VM 的栈区域映射 |

> **注意**: `pt_map_in_range` 与 `pt_ptmap` 的区别：
> - `pt_map_in_range`：复制指定虚拟地址范围内的**页表项（PTE）**，用于复制用户空间映射
> - `pt_ptmap`：复制页目录和二级页表本身的映射（即让 dst 进程能访问 src 进程的页表结构），用于 RS 服务重启时恢复 VM 的页表自映射

#### 2.4.2 pt_writable - 查询页是否可写

**源码位置**: `minix/servers/vm/pagetable.c:761`

**作用**: 查询指定虚拟地址对应的页表项是否设置了写权限。

```c
int pt_writable(struct vmproc *vmp, vir_bytes v)
{
	u32_t entry;
	pt_t *pt = &vmp->vm_pt;
	assert(!(v % VM_PAGE_SIZE));
	int pde = ARCH_VM_PDE(v);
	int pte = ARCH_VM_PTE(v);

	assert(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT);
	assert(pt->pt_pt[pde]);

	entry = pt->pt_pt[pde][pte];

#if defined(__i386__)
	return((entry & PTF_WRITE) ? 1 : 0);
#elif defined(__arm__)
	return((entry & ARCH_VM_PTE_RO) ? 0 : 1);
#endif
}
```

**架构差异**:
- **x86**: 检查 `PTF_WRITE`（RW 位），置位返回 1
- **ARM**: 检查 `ARCH_VM_PTE_RO`（只读位），置位返回 0（取反逻辑）

**调用场景**: 仅在 `#if SANITYCHECKS` 条件编译下使用（`region.c:55`），用于调试输出时标记物理页是只读（R）还是可写（W）。

#### 2.4.3 pt_clearmapcache - 清除内核映射缓存

**源码位置**: `minix/servers/vm/pagetable.c:751`

**作用**: 通知内核清除其内部的页表映射缓存（mapcache），确保内核在使用当前页表建立新映射前使 TLB 失效。

```c
void pt_clearmapcache(void)
{
	/* Make sure kernel will invalidate tlb when using current
	 * pagetable (i.e. vm's) to make new mappings before new cr3
	 * is loaded.
	 */
	if(sys_vmctl(SELF, VMCTL_CLEARMAPCACHE, 0) != OK)
		panic("VMCTL_CLEARMAPCACHE failed");
}
```

**背景**: Minix3 内核在 `data_copy` 等操作中会缓存页目录项（PDE）到 `freepdes` 槽位（详见[附录 A](#附录-a-内核访问页目录的实现细节)）。当 VM 修改了页表（如 `pt_bind` 重新绑定、live update 交换进程槽位后），这些缓存的 PDE 可能指向旧的页表。`pt_clearmapcache` 通过 `VMCTL_CLEARMAPCACHE` 通知内核丢弃缓存。

**调用场景**:

| 场景 | 源码位置 | 说明 |
|------|----------|------|
| VM 初始化完成 | `main.c:212` | VM 初始化自身页表并 `pt_bind` 后清除缓存 |
| VM 热更新 | `main.c:719` | live update 交换新旧 VM 进程槽位并重新 `pt_bind` 后 |
| VM 主循环结束 | `main.c:749` | 每次 `alloc_cycle` 后（处理完所有请求后） |
| page fault 处理完成 | `pagefaults.c:153` | 处理完 page fault、修改页表后 |
| `pt_assert` 调试 | `pagetable.c:115` | 验证页表前确保内核缓存同步 |

---

## 3. Rust 设计决策

> 以下 Rust 设计基于 64 位假设（x86-64 四级页表、48 位虚拟地址、52 位物理地址），不适用于 32 位架构。

### 3.0 架构决策：取消 `pagedir_mappings`，采用直接映射区

Minix3 的 `pagedir_mappings` 机制是 32 位地址空间约束下的产物。minix-rs 目标为 x86-64，虚拟地址空间充裕（48 位，256TB），应采用直接映射区方案。

**问题一：内核如何访问其他进程的页表**

Minix3 内核运行时使用当前进程的页表，没有物理内存的直接映射。`pagedir_mappings` 为每个进程分配一个固定的虚拟地址窗口（`p_cr3_v`），内核通过该窗口读写进程的页目录。

minix-rs 采用直接映射区：内核启动时在高位虚拟地址（如 `0xffff8880_00000000`）建立所有物理内存的线性映射，关系为 `va = pa + DIRECT_MAP_BASE`。内核拿到任意物理地址后，通过 `phys_to_virt()` 一行加法即可得到可用的虚拟地址。

**问题二：跨进程内存拷贝**

Minix3 的 `sys_datacopy` 需要将目标进程的 PDE 写入当前进程页目录的 `freepdes` 空闲槽位，建立 4MB 临时映射窗口，拷贝完成后清除映射（详见附录 A）。

minix-rs 直接映射区方案下，内核将源/目标虚拟地址翻译为物理地址，再通过 `phys_to_virt()` 得到内核虚拟地址，直接 `memcpy` 即可，无需修改页表。

**对比总结**：

| 场景 | Minix3（32 位） | minix-rs（64 位 + 直接映射区） |
|------|-----------------|-------------------------------|
| 访问进程页目录 | `p_cr3_v`（固定窗口） | `phys_to_virt(paddr)` |
| 跨进程内存拷贝 | `freepdes` 临时映射 → 拷贝 → 清除 | 翻译虚拟地址 → `phys_to_virt()` → 直接 memcpy |
| 页表修改 | 需要（临时注入 PDE） | 不需要 |
| 并发风险 | 有（临时借用页表） | 无 |

**`map_kernel` 职责简化**：

每个用户进程的页表中必须映射内核代码段和数据段，否则中断/系统调用进入 ring 0 时无法执行。直接映射区消除了 `page_directories` 登记册和 `freepdes` 空闲槽位的映射需求：

| 映射内容 | Minix3 `pt_mapkernel` | minix-rs `map_kernel` |
|----------|----------------------|----------------------|
| 内核代码/数据段 | ✅ 必须 | ✅ 必须 |
| `page_directories` 登记册 | ✅ 必须 | ❌ 不需要 |
| `freepdes` 空闲槽位 | ✅ 必须预留 | ❌ 不需要 |

### 3.1 Minix3 函数映射

| Minix3 函数 | Rust trait 方法 | 说明 |
|-------------|-----------------|------|
| `pt_new()` | `Paging::new()` | 创建页表 |
| `pt_bind()` | `VmPagingExt::bind_to_process()` | 绑定页表到进程（通知内核） |
| `pt_free()` | `Paging::destroy()` | 销毁页表 |
| `pt_writemap()` | `Paging::map()` | 建立映射 |
| `pt_checkrange()` | `Paging::query()` | 检查映射 |

**关键区别**:
- Minix3 直接操作硬件页表结构
- Rust 通过 `Paging` trait 抽象，支持 Mock 测试
- 两者都使用显式生命周期管理（`pt_free()` / `destroy()`），内核代码偏好显式控制而非隐式析构

### 3.2 安全封装

> 本节聚焦安全封装的**动机**（为什么用 newtype、为什么 unsafe）。
> 各操作与 Minix3 的语义对应关系见 §4。

**问题**: Minix3 中页表操作大量使用裸指针和手动内存管理。

**Rust 解决方案**:

```rust
/// 类型安全的地址类型
///
/// 防止虚拟地址和物理地址混淆。
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VirBytes(pub u64);

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysBytes(pub u64);

/// 编译时防止类型混淆
fn map(vaddr: VirBytes, paddr: PhysBytes) {
    // 编译器会阻止传入错误类型
    // map(paddr, vaddr)  // 编译错误！
}
```

**unsafe 边界**:

```rust
/// Paging trait 定义（已实现）
pub trait Paging {
    const PAGE_SIZE: usize;

    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized;

    /// 单页映射（底层 API）
    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<(), PageTableError>;

    /// 原子覆盖映射（对应 WMF_OVERWRITE）
    fn remap(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<Option<(PhysBytes, PageFlags)>, PageTableError>;

    /// 取消映射，返回原物理地址
    fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError>;

    /// 更新标志位，保留物理地址（对应 WMF_WRITEFLAGSONLY）
    fn update_flags(&mut self, vaddr: VirBytes, flags: PageFlags)
        -> Result<(), PageTableError>;

    /// 查询映射
    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)>;

    /// 获取页表根物理地址
    fn root_paddr(&self) -> PhysBytes;

    /// 切换页表（unsafe：修改全局状态）
    unsafe fn switch(&self);

    /// 刷新整个 TLB
    unsafe fn flush_tlb(&self);

    /// 刷新单个虚拟地址的 TLB 条目
    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes);

    /// 批量映射（默认实现：逐页调用 map）
    fn map_range(&mut self, vaddr_start: VirBytes, paddr_start: PhysBytes,
        pages: usize, flags: PageFlags) -> Result<(), PageTableError> {
        for i in 0..pages {
            let v = VirBytes(vaddr_start.0 + (i * Self::PAGE_SIZE) as u64);
            let p = PhysBytes(paddr_start.0 + (i * Self::PAGE_SIZE) as u64);
            self.map(v, p, flags)?;
        }
        Ok(())
    }

    /// 批量取消映射（默认实现：逐页调用 unmap）
    fn unmap_range(&mut self, vaddr_start: VirBytes, pages: usize)
        -> Result<(), PageTableError> {
        for i in 0..pages {
            let v = VirBytes(vaddr_start.0 + (i * Self::PAGE_SIZE) as u64);
            self.unmap(v)?;
        }
        Ok(())
    }

    /// 销毁页表（unsafe：需确保未激活）
    unsafe fn destroy(&mut self);
}
```

```rust
/// unsafe 操作封装示例
impl Paging for MockPaging {
    const PAGE_SIZE: usize = 4096;

    fn new() -> Result<Self, PageTableError> {
        Self::new_mock()
    }

    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<(), PageTableError>
    {
        let v = vaddr.0;
        let p = paddr.0;
        if v % Self::PAGE_SIZE as u64 != 0 || p % Self::PAGE_SIZE as u64 != 0 {
            return Err(PageTableError::InvalidAddress);
        }
        if self.mappings.contains_key(&v) {
            return Err(PageTableError::AlreadyMapped);
        }
        self.mappings.insert(v, (p, flags));
        Ok(())
    }

    unsafe fn switch(&self) {
        ACTIVE_MOCK_TABLE.store(self.id, Ordering::SeqCst);
    }

    unsafe fn destroy(&mut self) {
        self.mappings.clear();
        if ACTIVE_MOCK_TABLE.load(Ordering::SeqCst) == self.id {
            ACTIVE_MOCK_TABLE.store(NO_ACTIVE_TABLE, Ordering::SeqCst);
        }
    }

    // ... 其他方法省略
}
```

**API 设计说明**:
- Minix3 的 `pt_writemap()` 通过 `bytes` 参数同时支持单页和批量映射。
  Rust 将其拆分为 `map()`（单页）和 `map_range()`（批量），理由：
  (1) 单页操作是最常见路径，独立签名更清晰；
  (2) 批量映射的错误处理语义不同（部分成功 vs 全部回滚），拆分后可分别设计；
  (3) `map_range()` 提供默认实现（逐页调用 `map()`），arch 实现可覆盖以利用硬件优化（如 x86-64 批量映射后单次 CR3 reload）
- `map()` 是单页映射的底层 API，对应硬件页表项操作

### 3.3 错误处理

> **定义位置**: `minix_arch::paging`，6 个变体。

**问题**: Minix3 使用整数错误码，缺乏上下文信息。

**Rust Result 类型**:

```rust
/// 页表错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableError {
    /// 地址未页对齐或超出有效范围
    InvalidAddress,
    /// 虚拟地址已存在映射
    AlreadyMapped,
    /// 虚拟地址不存在映射
    NotMapped,
    /// 物理页分配失败（中间页表或目标页）
    AllocationFailed,
    /// 权限不足（如对只读页执行写操作）
    PermissionDenied,
    /// 操作不被当前架构支持
    NotSupported,
}
```

**Minix3 错误码对比**:

| Minix3 errno | 触发场景 | Rust 对应 |
|---|---|---|
| `ENOMEM` | `pt_new()` 分配页目录失败、`pt_ptalloc()` 分配页表失败 | `AllocationFailed` |
| `EFAULT` | `pt_ptalloc_in_range()` verify 失败、`pt_writemap()` verify 不匹配 | `InvalidAddress` |
| `OK` | `pt_writemap()` 遇到已映射地址（`WMF_OVERWRITE` 隐式覆盖） | `AlreadyMapped`（Rust 拒绝覆盖，需显式 `unmap` + `map`） |
| `assert` | `pt_writemap()` 地址未对齐（编程错误，不可恢复） | `InvalidAddress`（Rust 作为可恢复错误返回） |
| (无) | `pt_bind()` 内核 IPC 失败 | `NotSupported` / `PermissionDenied` |

> **设计说明**：Minix3 的对齐错误用 `assert` 处理（编程错误，直接 panic），
> Rust 将其作为 `InvalidAddress` 可恢复错误返回，允许调用者做参数校验。
> `AlreadyMapped`/`NotMapped` 是 Rust 新增的，Minix3 用 `WMF_OVERWRITE`
> 隐式处理这些情况。`VmError` 等上层错误类型不属于 pagetable 模块，
> 将在 VM 系统错误处理的整体设计中定义。

---

## 4. 实现详解

> `Paging` trait 的完整定义、`MockPaging` 实现、`PageFlags`/`PageTableError` 类型详见 [06-pagetable-struct.md](06-pagetable-struct.md)。本章聚焦于各操作的语义和 Minix3 对应关系。

> **TODO**：当前仅有 `MockPaging` 实现（用于用户态单元测试），各架构的具体实现（如 `X86_64Paging`）尚未完成。待架构实现就绪后，补充各操作在硬件层面的实现细节（如中间页表按需分配、TLB 刷新策略、大页映射路径等）。

### 4.1 页表创建与销毁

| 操作 | Minix3 | Rust |
|------|--------|------|
| 创建 | `pt_new()` | `Paging::new()` |
| 销毁 | `pt_free()` | `unsafe fn destroy()` |
| 获取根地址 | `pt_dir_phys` | `root_paddr()` |

**创建流程**：`Paging::new()` 分配页表所需的所有物理页（x86-64 为 PML4 + 按需分配的中间页表），返回初始化完成的页表实例。Minix3 的 `pt_new()` 同时分配页目录和 `pt_dir`/`pt_pt[]` 缓存，Rust 版本将缓存逻辑封装在实现内部，不暴露给调用者。

**销毁流程**：`destroy()` 释放页表占用的所有物理页。标记为 `unsafe`，调用者需确保页表未激活且未绑定到任何进程。Minix3 的 `pt_free()` 不释放页目录（与槽位绑定复用），Rust 版本无此限制。

### 4.2 映射操作

| 操作 | Minix3 | Rust |
|------|--------|------|
| 单页映射（新建） | `pt_writemap()` (无 `WMF_OVERWRITE`) | `Paging::map()` |
| 单页映射（覆盖） | `pt_writemap()` + `WMF_OVERWRITE` | `Paging::remap()` |
| 批量映射 | `pt_writemap()` | `Paging::map_range()` |
| 取消映射 | `pt_writemap()` (flags=0) | `Paging::unmap()` |
| 取消映射+释放物理页 | `pt_writemap()` + `WMF_FREE` | `unmap()` + `phys_block` 引用计数递减¹ |
| 查询映射 | `pt_checkrange()` | `Paging::query()` |
| 更新标志 | `pt_writemap()` + `WMF_WRITEFLAGSONLY` | `Paging::update_flags()` |

¹ Minix3 将"取消映射"和"释放物理页"耦合在 `pt_writemap` 的 `WMF_FREE` 标志中。Rust 版本将两者解耦：`unmap()` 仅取消映射并返回原物理地址，物理页的释放由 `phys_block` 引用计数管理——当引用计数归零时自动释放。这是职责分离的设计决策：页表层只管映射，物理页生命周期由独立的内存管理模块负责。

**`map()` vs `remap()`**：Minix3 的 `pt_writemap()` 几乎总是带 `WMF_OVERWRITE`（region.c:285, pagetable.c:713,743），即"覆盖映射"是常态。Rust 将其拆分为两个方法：
- `map()`：严格语义，遇到已映射地址返回 `AlreadyMapped`，用于"此处不应有映射"的场景
- `remap()`：原子覆盖语义，对应 `WMF_OVERWRITE`，避免 `unmap()` + `map()` 之间的无映射窗口

**`map()` vs `map_range()`**：`map()` 是单页映射的底层 API。`map_range()` 是批量映射的上层封装，提供默认实现（逐页调用 `map()`），arch 实现可覆盖以利用硬件优化（如 x86-64 批量映射后单次 CR3 reload 替代多次 INVLPG）。

**`query()` vs `pt_checkrange()`**：Minix3 的 `pt_checkrange()` 在全源码中仅有一处调用且被 `#if SANITYCHECKS` 包裹，属于 debug-only 断言。Rust 版本使用 `query()` 返回单页映射信息，调用者可自行循环实现范围检查。

### 4.3 进程绑定

| 操作 | Minix3 | Rust |
|------|--------|------|
| 绑定页表 | `pt_bind()` | `VmPagingExt::bind_to_process()` |
| 激活页表 | `setcr3()` (内核) | `unsafe fn switch()` |
| 映射内核 | `pt_mapkernel()` | `VmPagingExt::map_kernel()` |

**绑定流程**：`bind_to_process()` 将页表与进程关联，通知内核该进程的地址空间根地址。对应 Minix3 的 `pt_bind()` → `sys_vmctl_set_addrspace()` → 内核 `setcr3()`。

> **语义差异**：Minix3 的 `pt_bind()` 实际做两件事：(1) 将页目录物理地址写入 `pagedir_mappings` 登记册（内核通过此登记册间接访问进程页目录）；(2) 调用 `sys_vmctl_set_addrspace()` 通知内核。Rust 的 `bind_to_process()` 仅对应第 (2) 步，因为 x86-64 使用直接映射区（direct-map），内核可直接通过物理地址访问任何进程的页目录，不需要 `pagedir_mappings` 登记册。第 (1) 步在 64 位架构下不需要。

**`map_kernel()`**：每个用户进程的页表中必须映射内核代码段和数据段，否则中断/系统调用进入 ring 0 时无法执行。当前内核部分尚未就绪，`map_kernel()` 的实现暂为 TODO。详见 [3.0 节](#30-架构决策取消-pagedir_mappings采用直接映射区)中 `map_kernel` 职责简化的讨论。

### 4.4 fork 相关操作 【设计目标】

> fork 时的页表复制、CoW 标记、`phys_block` 引用计数等内容详见 [10-phys-block.md](10-phys-block.md) 和 [17-vm-fork.md](17-vm-fork.md)。

---

## 5. 测试与验证

> 实际测试代码位于 `os/arch/src/paging.rs` 的 `mock::tests` 模块，
> 使用 `#[cfg(test)]` + `#[cfg(feature = "mock")]` 条件编译。
> 以下仅列出测试设计意图，不重复实际代码。

### 5.1 测试覆盖矩阵

| 测试场景 | 验证点 | 对应 Minix3 行为 |
|----------|--------|-----------------|
| `new()` + `destroy()` | 页表创建/销毁生命周期 | `pt_new()` / `pt_free()` |
| `map()` + `query()` | 映射后可查询 | `pt_writemap()` + `pt_checkrange()` |
| `map()` 重复映射 → `AlreadyMapped` | 不允许静默覆盖 | `pt_writemap()` 无 `WMF_OVERWRITE` 时行为 |
| `remap()` 覆盖映射 | 原子替换旧映射 | `pt_writemap()` + `WMF_OVERWRITE` |
| `unmap()` → `query()` 返回 None | 取消映射后不可查询 | `pt_writemap(MAP_NONE, 0)` |
| `unmap()` 未映射地址 → `NotMapped` | 取消不存在的映射报错 | Minix3 无此检查（静默忽略） |
| `update_flags()` 保留物理地址 | 只改标志不改映射目标 | `pt_writemap()` + `WMF_WRITEFLAGSONLY` |
| 未对齐地址 → `InvalidAddress` | 地址必须页对齐 | Minix3 隐式依赖硬件检查 |

### 5.2 运行测试

```bash
cargo test -p minix-arch --features mock
```

---

## 6. 参见

- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构
- [10-phys-block.md](10-phys-block.md) - 物理块引用计数
- [17-vm-fork.md](17-vm-fork.md) - fork 时的页表操作

---

## 附录 A: 内核访问页目录的实现细节

> 本附录详细说明 Minix3 内核在哪些场景下需要访问其他进程的页目录，以及 `pagedir_mappings` 和 `freepdes` 机制在这些场景中的具体工作方式。这些场景均发生在内核代码中，不属于 VM 服务器的实现范围。

### A.1 跨进程内存拷贝：`data_copy`

**源码位置**: `minix/kernel/arch/i386/memory.c`

**调用链**：用户态 `sys_datacopy`（宏，定义在 `syslib.h`）→ 内核 `do_copy` → `virtual_copy` → `lin_lin_copy` → `createpde`。

`data_copy` 是内核中跨进程数据拷贝的核心操作，用于 IPC 等场景。内核执行此操作时，需要访问源进程和目标进程的页表来建立临时映射窗口。

**流程**（`lin_lin_copy` → `createpde`）：

1. `createpde` 判断源/目标进程是否已在当前页表中可见（当前进程或内核进程），若可见则直接返回线性地址
2. 若不可见且 `pr != NULL`（进程地址），通过 `p_cr3_v`（`pagedir_mappings` 提供的虚拟地址）读取该进程的页目录项，取出 PDE 值（包含二级页表的物理地址 + 标志位）
3. 若 `pr == NULL`（物理地址），直接构造 4MB 大页 PDE：`pdeval = (addr & ADDR_MASK_4MB) | BIGPAGE | PRESENT | WRITE | USER`。这是自映射大页：PDE 的物理地址部分取自 `linaddr` 的高 10 位（4MB 对齐），使得虚拟地址 `pde_slot * 4MB + offset` 直接映射到物理地址 `(linaddr & ADDR_MASK_4MB) + offset`，无需经过二级页表
4. 将 PDE 值写入当前进程页目录的空闲槽位（`freepdes`，最多 `MAXFREEPDES = 2` 个，源和目标各占一个）
5. 当前进程的地址空间中出现 4MB 的临时映射窗口
6. 内核通过该窗口执行 `PHYS_COPY_CATCH` 完成数据拷贝
7. 拷贝完成后，`mem_clear_mapcache` 将 `freepdes` 对应的 PDE 清零，临时映射立即失效

**两种机制的分工**：
- `pagedir_mappings`（`p_cr3_v`）：提供进程页目录的虚拟地址，用于读取进程的 PDE 值
- `freepdes`：提供当前页目录中的空闲 PDE 槽位，用于写入临时映射

**4MB 粒度的原因**：Minix3 选择在 PDE 层面做借用，粒度为 4MB（一个二级页表覆盖 1024 × 4KB）。若在 PTE 层面做借用（通过 scratch 页表），粒度可降至 4KB，但需要额外的页表管理开销。Minix3 选择了更简单的 PDE 级借用。

**临时借用的本质**：映射期间，当前进程和目标进程的页目录指向同一份物理页表（二级页表）。这不是共享内存——数据是复制过去的，映射是临时的，用完即销毁。

**与 Linux 的对比**：Linux 内核通过直接映射区（`va = pa + PAGE_OFFSET`）即可访问任意物理页，无需修改页目录。Minix3 内核没有直接映射区，必须通过 PDE 注入建立临时窗口。

### A.2 虚拟地址翻译：`vm_lookup`

**源码位置**: `minix/kernel/arch/i386/memory.c:325-372`

`vm_lookup` 是内核函数（非 VM 服务器函数），用于将目标进程的虚拟地址翻译为物理地址。被内核的 `do_umap_remote`、`umap_virtual` 等调用。

**流程**：

1. 从进程控制块获取页目录物理地址（`p_seg.p_cr3`）
2. 通过 `phys_get32` 读取页目录项：`pde_v = phys_get32(root + pde_index * 4)`
3. 从 PDE 中提取二级页表物理地址
4. 通过 `phys_get32` 读取页表项：`pte_v = phys_get32(pt_addr + pte_index * 4)`
5. 从 PTE 中提取物理页地址，加上页内偏移得到最终物理地址

**关键点**：`phys_get32` 内部调用 `lin_lin_copy(NULL, addr, ...)`，源进程为 NULL（物理地址），走 `createpde` 的 `pr==NULL` 路径——直接构造 4MB 大页 PDE 写入 `freepdes` 槽位。因此 `vm_lookup` 依赖的是 `freepdes`（临时 PDE 注入），**不依赖** `pagedir_mappings`（`p_cr3_v`）。这是两级索引直接定位（非遍历），没有 for 循环。

---

*分类: VM库 | 可被其他服务使用*
