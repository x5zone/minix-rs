# 07-pagetable-ops: 页表操作

> **分类**: VM库  
> **源码**: [pagetable.c](minix3/minix/servers/vm/pagetable.c)  
> **说明**: 页表的生命周期管理，包括创建、绑定、映射等操作

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

### 1.2.1 页表子系统初始化

上述生命周期描述的是**单个进程的页表**如何创建、使用、销毁。但在这些操作之前，必须先建立**整个页表子系统的基础设施**——这是 `pt_init()` 的职责。

`pt_init()` 在任何进程页表操作之前执行一次，完成以下初始化：

1. **备用页池建立**：将 BSS 段中静态分配的备用页注册到 `spare_pagequeue`，供 `vm_allocpage` 在初始化阶段使用（详见 [05-vm-allocpage.md](05-vm-allocpage.md) §2.2）
2. **CPU 特性检测**：x86-32 上检测 Global 位（PGE）和 4MB 大页（PSE）支持，决定 `pt_mapkernel` 是否设置 Global 位
3. **`kern_mappings` 初始化**：从内核获取预留映射信息（`sys_vmctl_get_mapping`），记录物理地址、长度、权限标志，分配 PDE 编号，供 `pt_mapkernel` 在每个进程页表中复制内核映射
4. **`pagedir_mappings` 初始化**：调用 `pt_allocate_kernel_mapped_pagetables()` 建立内核访问进程页目录的登记册基础设施（详见 §1.3）
5. **VM 自身页表建立**：调用 `pt_new` 创建 VM 进程的页表，从内核获取当前页目录内容，为每个已存在的 PDE 分配二级页表并复制内容（`pt_ptalloc` + `sys_abscopy`），然后调用 `pt_bind` 绑定到 VM 进程
6. **动态化重建**：`pt_init_done = 1` 后，将所有备用页池替换为动态分配，重新执行 `pt_allocate_kernel_mapped_pagetables`、`pt_bind`、`pt_mapkernel`，并用 `pt_copy` 将 VM 页表重建为纯动态内存版本

**`pt_copy(dst, src)`**：复制 `src` 用户空间（`pde < kern_start_pde`）的所有 PTE 到 `dst`。对每个已存在的 PDE，调用 `pt_ptalloc` 分配目标二级页表，然后 `memcpy` 整个页表内容。语义上相当于 `pt_map_in_range` 的全量版本。

> `pt_init` 从备用页到动态分配的切换机制、递归问题等，详见 [05-vm-allocpage.md](05-vm-allocpage.md) §2.2-2.4。本节侧重初始化建立的页表操作语义。

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

### 2.0 页表子系统初始化

#### 2.0.1 pt_init - 页表子系统初始化

**源码位置**: `minix/servers/vm/pagetable.c:1088`

**作用**: 在任何进程页表操作之前执行一次，建立整个页表子系统的基础设施。这是 VM 启动过程中的关键步骤——在此之前，VM 使用内核预设的页表，无法自主管理内存。

**执行流程**（6 个阶段）：

**阶段 1：备用页池建立**

将 BSS 段中静态分配的 `static_sparepages` 注册到 `spare_pagequeue`，通过 `sys_umap` 获取每个备用页的物理地址。这些备用页是 `vm_allocpage` 在初始化阶段唯一的内存来源。

**阶段 2：CPU 特性检测**（x86-32）

```c
global_bit_ok = _cpufeature(_CPUF_I386_PGE);  // Global 位支持
bigpage_ok = _cpufeature(_CPUF_I386_PSE);      // 4MB 大页支持
if(global_bit_ok) global_bit = I386_VM_GLOBAL;
```

Global 位使内核页表项在 CR3 切换时不被刷出 TLB，对性能至关重要。

**阶段 3：`kern_mappings` 初始化**

从内核获取预留映射信息（`sys_vmctl_get_mapping`），记录到 `kern_mappings[]` 数组。每个映射包含物理地址、长度、权限标志，并分配连续的 PDE 编号（通过 `freepde()`）。这些映射将在 `pt_mapkernel` 中写入每个进程的页表。`freepde()` 从内核预留的 PDE 范围中递增分配编号。

**阶段 4：`pagedir_mappings` 初始化**

调用 `pt_allocate_kernel_mapped_pagetables()` 建立 `pagedir_mappings` 登记册基础设施（详见 §1.3）。

**阶段 5：VM 自身页表建立**

```c
newpt = &vmprocess->vm_pt;
pt_new(newpt);                                    // 创建空页表
sys_vmctl_get_pdbr(SELF, &mypdbr);                // 获取当前页目录物理地址
sys_vircopy(NONE, mypdbr, SELF, currentpagedir);  // 复制当前页目录内容
for(p = 0; p < ARCH_VM_DIR_ENTRIES; p++) {
    if(!(entry & ARCH_VM_PDE_PRESENT)) continue;
    if(entry & ARCH_VM_BIGPAGE) continue;         // 跳过大页（内核/identity）
    pt_ptalloc(newpt, p, 0);                       // 分配二级页表
    sys_abscopy(ptaddr_kern, ptaddr_us);           // 从内核页表复制 PTE 内容
}
pt_bind(newpt, &vmproc[VM_PROC_NR]);              // 注册到 pagedir_mappings
pt_init_done = 1;                                  // 标记初始化完成
```

关键点：VM 不能直接读取内核建立的页表（因为没有 direct map），必须通过 `sys_vircopy`（内核 IPC）获取当前页目录内容，再逐 PDE 用 `sys_abscopy` 复制二级页表。

**阶段 6：动态化重建**

```c
alloc_cycle();                              // 确保动态分配可用
while(vm_getsparepage(&phys)) ;             // 耗尽静态备用页
alloc_cycle();                              // 用动态页重新填充备用池
pt_allocate_kernel_mapped_pagetables();     // 用动态内存重建登记册
pt_bind(newpt, &vmproc[VM_PROC_NR]);        // 重新绑定
pt_mapkernel(newpt);                        // 重建内核映射

// 用 pt_copy 将 VM 页表重建为纯动态版本
pt_new(&newpt_dyn);
pt_copy(&newpt_dyn, newpt);                 // 复制用户空间 PTE
memcpy(newpt, &newpt_dyn, sizeof(*newpt));  // 替换

pt_bind(newpt, &vmproc[VM_PROC_NR]);        // 最终绑定
pt_mapkernel(newpt);                        // 最终内核映射
```

为什么要重建？源码注释（`pagetable.c:1316-1318`）解释：静态备用页的物理地址在 live update 时会变化，因此必须替换为动态分配的内存。`pt_copy` 复制用户空间 PTE，`memcpy` 替换整个 `pt_t` 结构。

#### 2.0.2 pt_copy - 复制用户空间页表项

**源码位置**: `minix/servers/vm/pagetable.c:1069`

**作用**: 将 `src` 页表的用户空间 PTE 完整复制到 `dst` 页表。

```c
static void pt_copy(pt_t *dst, pt_t *src)
{
    for(pde=0; pde < kern_start_pde; pde++) {
        if(!(src->pt_dir[pde] & ARCH_VM_PDE_PRESENT)) continue;
        pt_ptalloc(dst, pde, 0);                         // 分配目标二级页表
        memcpy(dst->pt_pt[pde], src->pt_pt[pde],         // 复制全部 PTE
            ARCH_VM_PT_ENTRIES * sizeof(*dst->pt_pt[pde]));
    }
}
```

语义上相当于 `pt_map_in_range` 的全量版本——遍历用户空间所有 PDE，逐页表 `memcpy`。与 `pt_map_in_range` 的区别：`pt_copy` 用 `memcpy` 批量复制（更快，但要求 `dst` 的二级页表可写），`pt_map_in_range` 逐 PTE 赋值（更灵活，可指定范围）。

仅在 `pt_init` 阶段 6 中使用，将 VM 页表从静态版本重建为动态版本。

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

> **minix-rs 实现**：`map_kernel()` 执行两段映射：
> 1. **内核代码/数据段**: 与 Minix3 相同，必须映射，否则中断/系统调用进入 ring 0 时无法执行。
> 2. **Kernel direct map**: 1GB huge pages，U/S=0（仅内核可访问），G=1（Global，CR3 切换不刷新 TLB）。
>
> Minix3 的第 2 段（`pagedir_mappings` 登记册）和第 3 段（`kern_mappings` 特殊映射）不再需要——kernel direct map 替代了 `pagedir_mappings`，设备内存映射通过 VM_MAP_PHYS 处理。
>
> **Kernel direct map 只读不变量**：`map_kernel()` 建立后，VM 不再修改 kernel direct map 的 PTE/PDE/PDPT 表项。这个约束不是限制，而是简化——不变的东西不需要管理。Global 位的 TLB 条目不需要额外刷新策略（CR3 切换不刷新，且内容不变）。如果未来需要动态修改（如内存热插拔），需要设计显式的 TLB 刷新协议（跨核 shootdown），但当前设计中不需要。

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

- `pdeno`: 固定的 PDE 索引号，在所有进程的页目录中占据此位置。由 `freepde()` 分配——`freepde()` 从内核预留的 PDE 范围（`freepde_start`）中递增分配编号，确保不会与用户空间 PDE 冲突。
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

> **Direct Map 标注**：`pagedir_mappings` 是 x86-32 内核未采用 direct map 时的间接访问机制。x86-32 理论上可以划出地址空间做 direct map（Linux x86-32 用 ~896MB 线性映射区），但 Minix3 选择了更简单的方案——不建 direct map，内核需要访问进程页目录时，只能通过 VM 在每个进程页目录中注入的固定窗口（`p_cr3_v`）来间接访问。x86-64 下 kernel 通过 direct map 直接访问进程页目录——`kernel_phys_to_virt(cr3_phys)` 一步到位，`pagedir_mappings` 登记册、`page_directories` 页表、`p_cr3_v` 窗口全部不再需要。
>
> 读者可对比 `pagedir_mappings` 与 `createpde`（§A.1-A.2）的定位差异：`pagedir_mappings` 是**持久化**的间接访问机制（VM 初始化时建立，进程生命周期内有效），`createpde` 是**临时**的间接访问机制（每次使用时建立，用完立即清除）。两者都是 x86-32 没有 direct map 的产物，只是生命周期不同。

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

> **Direct Map 标注**：minix-rs 的 `bind_to_process()` 仅对应 Minix3 `pt_bind` 的第 5 步——调用 `sys_vmctl_set_addrspace()` 通知内核。步骤 1-4（定位登记册槽位、写入物理地址、计算虚拟访问地址）全部不再需要，因为 x86-64 使用 direct map，内核可直接通过 `kernel_phys_to_virt(cr3_phys)` 访问任何进程的页目录，不需要 `pagedir_mappings` 登记册。
>
> `pt_bind` 从 5 步简化为 1 步，这不是"优化了绑定流程"，而是 **"绑定的语义被重新定义"**——Minix3 的"绑定"包含"让内核能看到页目录"（步骤 1-4）和"通知内核切换地址空间"（步骤 5），Direct Map 的"绑定"仅包含后者，因为内核已经能看到所有页目录。

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

#### 2.3.2 pt_ptalloc / pt_ptalloc_in_range - 分配二级页表

**源码位置**: `minix/servers/vm/pagetable.c:494` / `pagetable.c:545`

**作用**: 为页目录中尚未分配二级页表的 PDE 槽位分配物理页、建立映射、写入 PDE。`pt_writemap` 在写入 PTE 之前必须确保目标 PDE 对应的二级页表已存在——`pt_ptalloc` 就是完成这个前置步骤的函数。

**`pt_ptalloc(pt, pde, flags)`**：

1. 断言 PDE 不存在（`pt->pt_dir[pde]` 无 `PRESENT` 位，`pt->pt_pt[pde]` 为 `NULL`）
2. 调用 `vm_allocpage(&pt_phys, VMP_PAGETABLE)` 分配物理页，获得虚拟地址 `p` 和物理地址 `pt_phys`
3. **递归副作用检查**：`vm_allocpage` 可能递归触发 `vm_mappages` → `pt_writemap` → `pt_ptalloc`，内层 `pt_ptalloc` 可能已设置了 `pt->pt_pt[pde]`。此时释放刚分配的页，直接返回 `OK`
4. 清零页表：`pt->pt_pt[pde]` 的所有 PTE 置 0
5. 写入 PDE：`pt->pt_dir[pde] = pt_phys | flags | PRESENT | USER | RW`

**`pt_ptalloc_in_range(pt, start, end, flags, verify)`**：

遍历 `[first_pde, last_pde]`，对每个不存在二级页表的 PDE 调用 `pt_ptalloc`。`verify` 模式下发现缺失 PDE 直接返回 `EFAULT`，不分配。

**调用场景**：

| 场景 | 调用方 | 说明 |
|------|--------|------|
| 写入 PTE 前 | `pt_writemap` 步骤 2 | 确保目标地址范围的二级页表已存在 |
| 初始化阶段 | `pt_init` | 建立 VM 自身页表结构 |
| 复制映射前 | `pt_map_in_range` | 确保目标页表的二级页表已存在 |

**递归问题**：`vm_allocpage` → `vm_mappages` → `pt_writemap` → `pt_ptalloc` → `vm_allocpage` 的递归链，以及步骤 3 的副作用处理，详见 [05-vm-allocpage.md](05-vm-allocpage.md) §2.4。

#### 2.3.3 pt_checkrange - 检查地址范围

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

#### 2.3.4 vm_mappages - 分配虚拟地址并建立映射

> **注意**: `vm_mappages` 在 Minix3 源码中位于 `pagetable.c:295`，但语义上属于"页表映射操作"。它由 `vm_allocpage` 调用，负责为物理页分配虚拟地址并建立映射。

**源码位置**: `minix3/minix/servers/vm/pagetable.c:295`

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

#### 2.3.5 vm_freepages - 释放 VM 映射的页

**源码位置**: `minix/servers/vm/pagetable.c:235`

**作用**: `vm_mappages` 的逆操作——解除 VA→PA 映射并释放物理页。

```c
void vm_freepages(vir_bytes vir, int pages)
{
    assert(!(vir % VM_PAGE_SIZE));
    if(is_staticaddr(vir)) {        // 不释放 BSS 静态备用页
        printf("VM: not freeing static page\n");
        return;
    }
    pt_writemap(vmprocess, &vmprocess->vm_pt, vir,
        MAP_NONE, pages*VM_PAGE_SIZE, 0,
        WMF_OVERWRITE | WMF_FREE);  // 清除 PTE + free_mem 释放物理页
    vm_self_pages--;
}
```

**与 `vm_mappages` 的对称性**：

| | `vm_mappages` | `vm_freepages` |
|---|---|---|
| 语义 | 分配 VA + 建立 VA→PA 映射 | 清除 VA→PA 映射 + 释放物理页 |
| 物理页 | 由调用方提供（已知 PA） | 由 `pt_writemap(WMF_FREE)` 从 PTE 中提取后调用 `free_mem` 释放 |
| VA 来源 | `findhole` 在 VM 地址空间中查找 | 由调用方提供 |
| `pt_writemap` flags | `WMF_OVERWRITE` | `WMF_OVERWRITE | WMF_FREE` |
| 统计 | `vm_self_pages++` | `vm_self_pages--` |
| TLB 刷新 | 自动（`VMCTL_FLUSHTLB`） | `SANITYCHECKS` 模式下刷新 |

**静态页保护**：`is_staticaddr(vir)` 检查地址是否属于 BSS 段的静态备用页，如果是则拒绝释放。初始化阶段的备用页由 `pt_init` 统一管理，不允许单独释放。

**调用场景**：

| 场景 | 源码位置 | 说明 |
|------|----------|------|
| `pt_free` 释放二级页表 | `pagetable.c:1433` | 释放页表页的 VA 映射 |
| `pt_ptalloc` 递归副作用 | `pagetable.c:517` | 递归已分配的页表页被内层提前完成，释放外层多余的页 |
| VM 映射区域释放 | `utility.c:378` | `vm_freememory` 释放 VM 映射的内存区域 |
| slab 分配器释放 | `slaballoc.c:449` | 释放 slab 页 |

#### 2.3.6 vm_pagelock - VM 自身页权限锁定

**源码位置**: `minix/servers/vm/pagetable.c:403`

**作用**: 将 VM 自身分配的页标记为只读（`lockflag=1`）或恢复可写（`lockflag=0`）。用于 slab 分配器的写保护——空闲 slab 页设为只读，写入前解锁。

```c
void vm_pagelock(void *vir, int lockflag)
{
    u32_t flags = ARCH_VM_PTE_PRESENT | ARCH_VM_PTE_USER;
    if(!lockflag)
        flags |= ARCH_VM_PTE_RW;    // 解锁：恢复可写
    // lockflag=1 时不设 RW 位 → 只读

    pt_writemap(vmprocess, &vmprocess->vm_pt, m, 0, VM_PAGE_SIZE,
        flags, WMF_OVERWRITE | WMF_WRITEFLAGSONLY);  // 只改权限位，保留物理地址
    sys_vmctl(SELF, VMCTL_FLUSHTLB, 0);               // 刷新 TLB
}
```

**实现要点**：

- 使用 `pt_writemap` 的 `WMF_WRITEFLAGSONLY` 模式，保留原 PTE 中的物理地址，只修改权限标志位
- `physaddr` 参数传 0（被 `WMF_WRITEFLAGSONLY` 忽略）
- 操作的是 VM 自身页表（`vmprocess->vm_pt`），不影响其他进程
- 修改后必须刷新 TLB，否则 CPU 可能使用缓存的旧权限

**调用场景**：

| 场景 | 源码位置 | 说明 |
|------|----------|------|
| slab 页解锁（写入前） | `slaballoc.c:45` | `vm_pagelock(data, 0)` 恢复可写 |
| slab 页锁定（空闲时） | `slaballoc.c:52` | `vm_pagelock(data, 1)` 设为只读 |

slab 分配器通过写保护检测悬空指针：空闲 slab 页标记为只读，意外写入触发 page fault。

#### 2.3.7 vm_addrok - 检查 VM 自身地址有效性

**源码位置**: `minix/servers/vm/pagetable.c:440`

**作用**: 检查 VM 自身地址空间中，给定虚拟地址是否在页表中有有效映射，以及是否可写。

```c
int vm_addrok(void *vir, int writeflag)
{
    pt_t *pt = &vmprocess->vm_pt;
    int pde, pte;
    vir_bytes v = (vir_bytes) vir;

    pde = ARCH_VM_PDE(v);
    pte = ARCH_VM_PTE(v);

    /* 页目录项不存在 */
    if(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT)) {
        printf("addr not ok: missing pde %d\n", pde);
        return 0;
    }

    /* PDE 写权限检查（架构相关） */
#if defined(__i386__)
    if(writeflag && !(pt->pt_dir[pde] & ARCH_VM_PTE_RW)) {
        printf("addr not ok: pde %d present but pde unwritable\n", pde);
        return 0;
    }
#elif defined(__arm__)
    if(writeflag && (pt->pt_dir[pde] & ARCH_VM_PTE_RO)) {
        printf("addr not ok: pde %d present but pde unwritable\n", pde);
        return 0;
    }
#endif

    /* 页表项不存在 */
    if(!(pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT)) {
        printf("addr not ok: missing pde %d / pte %d\n", pde, pte);
        return 0;
    }

    /* PTE 写权限检查（架构相关） */
#if defined(__i386__)
    if(writeflag && !(pt->pt_pt[pde][pte] & ARCH_VM_PTE_RW)) {
        printf("addr not ok: pde %d / pte %d present but unwritable\n", pde, pte);
#elif defined(__arm__)
    if(writeflag && (pt->pt_pt[pde][pte] & ARCH_VM_PTE_RO)) {
        printf("addr not ok: pde %d / pte %d present but unwritable\n", pde, pte);
#endif
        return 0;
    }

    return 1;
}
```

**检查流程**：PDE Present → PDE 可写（仅 `writeflag=1`）→ PTE Present → PTE 可写（仅 `writeflag=1`）。任一环节失败则返回 0 并打印诊断信息。

**与 `pt_checkrange` 的对比**：

| | `pt_checkrange` | `vm_addrok` |
|---|---|---|
| 检查对象 | 任意进程的用户空间地址范围 | VM 自身地址空间中的单个地址 |
| 检查内容 | PDE/PTE Present + 写权限 | PDE/PTE Present + 写权限 |
| 操作粒度 | 地址范围（循环遍历） | 单个地址 |
| 返回值 | `OK` / `EFAULT` | `1` / `0` |
| 失败诊断 | 静默返回 | 打印具体缺失的 PDE/PTE 编号 |
| 使用场景 | `SANITYCHECKS` 条件编译下的调试断言 | 调试/断言用途 |

**调用场景**：`vm_addrok` 有声明（`proto.h:119`）和定义，但在 Minix3 整个 VM 源码中**没有任何调用点**，属于死代码。其设计意图是供 `vm_allocpage` 分配后验证映射有效性，但实际未启用。

> **minix-rs**：当前无 swap 机制，`vm_addrok` 的语义已被 `Paging` trait 的 `query()` 方法覆盖——`query(vaddr).is_some()` 等价于 PDE/PTE Present 检查，`flags.contains(WRITABLE)` 等价于写权限检查。无需新增 `is_mapped()` 方法，避免与 `query()` 功能重叠。

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

#### 2.4.2 pt_ptmap - 转移页表结构映射

**源码位置**: `minix/servers/vm/pagetable.c:685`

**作用**: 将源进程的页目录和二级页表的 VA→PA 映射转移到目标进程的地址空间中。让目标进程能通过虚拟地址访问源进程的页表结构（页目录本身 + 各二级页表页）。

**与 `pt_map_in_range` 的本质区别**：

| | `pt_map_in_range` | `pt_ptmap` |
|---|---|---|
| 复制对象 | PTE 值（用户空间映射） | 页表结构本身的 VA→PA 映射 |
| 操作方式 | 直接拷贝 `pt_pt[pde][pte]` 值 | 调用 `pt_writemap` 建立新映射 |
| 操作粒度 | 单个 PTE | 页目录 + 所有二级页表 |
| 目的 | 让 dst 拥有与 src 相同的用户空间映射 | 让 dst 能**访问** src 的页表结构 |

**执行流程**：

1. **转移页目录映射**：将 `src` 的页目录物理地址（`pt->pt_dir_phys`）映射到 `dst` 的虚拟地址（`pt->pt_dir` 的 VA），调用 `pt_writemap` 在 `dst` 页表中建立映射
2. **转移二级页表映射**：遍历 `src` 用户空间（`pde < kern_start_pde`）的每个已存在 PDE，将对应的二级页表物理地址（从 PDE 中提取）映射到 `dst` 的虚拟地址（`pt->pt_pt[pde]` 的 VA），同样通过 `pt_writemap` 建立映射

**调用场景**：

| 场景 | 源码位置 | 说明 |
|------|----------|------|
| RS 服务重启 | `rs.c:263` | `pt_ptmap(old_vm, new_vm)` — 让新 VM 能访问旧 VM 的页表 |
| RS 服务重启 | `rs.c:267` | `pt_ptmap(new_vm, new_vm)` — 让新 VM 能访问自己的页表（自映射） |

RS 服务重启流程中的两次调用：第一次让新 VM 实例"看到"旧 VM 的页表结构，用于读取旧 VM 的映射信息；第二次让新 VM 实例"看到"自己的页表结构，建立自映射（VM 进程的页表页映射在 VM 自身的地址空间中）。

#### 2.4.3 pt_writable - 查询页是否可写

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

#### 2.4.4 pt_clearmapcache - 清除内核映射缓存

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

### 3.0 架构决策：取消 `pagedir_mappings`，采用双视图直接映射区

Minix3 的 `pagedir_mappings` 机制是 32 位地址空间约束下的产物。minix-rs 目标为 x86-64，虚拟地址空间充裕（48 位，256TB），应采用直接映射区方案。但 x86-64 的特权级硬件要求 kernel（ring 0）和 VM（ring 3）使用不同的页表映射来访问同一物理内存——这就是**双视图模型**。

#### 3.0.1 双视图模型

Kernel 和 VM 都需要访问物理内存，但运行在不同的特权级。x86-64 硬件规定：U/S=0 的页面只能在 ring 0 访问，U/S=1 的页面可以在 ring 3 访问。因此，同一物理内存需要两套映射：

| | Kernel direct map | VM direct map |
|---|---|---|
| U/S 位 | 0（特权级） | 1（用户态） |
| 存在于 | 所有进程页表 | 仅 VM 进程页表 |
| 用途 | 内核执行代码时访问物理内存 | VM 编辑页表数据时访问物理内存 |
| 权限 | R/W，Global 位 | R/W + NX，无 Global 位 |
| 建立者 | VM 在 `map_kernel()` 中建立 | VM 在初始化时建立 |
| 生命周期 | 建立后只读不变量 | 随物理内存扩展而扩展 |
| VA 基址 | `KERNEL_DIRECT_MAP_BASE`（高位内核空间） | `VM_DIRECT_MAP_BASE`（用户空间高位） |

两者映射**同一物理内存**，只是 VA 窗口和权限不同。这不是冗余，而是 x86-64 特权级硬件的必然要求——不同执行上下文需要不同的映射来访问同一物理内存。

**`phys_to_virt()` 的拆分**：

```rust
fn kernel_phys_to_virt(phys: PhysBytes) -> VirBytes {
    VirBytes(KERNEL_DIRECT_MAP_BASE + phys.as_u64())
}

fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes {
    VirBytes(VM_DIRECT_MAP_BASE + phys.as_u64())
}
```

Kernel 代码使用 `kernel_phys_to_virt()`，VM 代码使用 `vm_phys_to_virt()`。两者做的是同一件事——`phys → stable VA`——只是 VA 窗口不同。

**`virt_to_phys()` 的前置条件**：

```rust
fn virt_to_phys(virt: VirBytes) -> PhysBytes {
    if virt.get() >= KERNEL_DIRECT_MAP_BASE {
        PhysBytes::new(virt.get() - KERNEL_DIRECT_MAP_BASE)
    } else {
        PhysBytes::new(virt.get() - VM_DIRECT_MAP_BASE)
    }
}
```

`virt_to_phys()` 假定输入虚拟地址位于某个 direct map 区域内。若传入的地址低于 `VM_DIRECT_MAP_BASE`（不在任何 direct map 区域），`else` 分支会计算 `virt - VM_DIRECT_MAP_BASE`，导致 u64 整数下溢。当前设计下这是可接受的——所有调用者（VM、内核）只对已知位于 direct map 区域内的地址调用此函数。若未来需要更健壮的接口，可改为返回 `Option<PhysBytes>`，对非法地址返回 `None`。

#### 3.0.2 安全边界论证

VM 有了 U/S=1 的 direct map，是否扩大了权限？

**没有。** VM 是 trusted pager，原本就控制所有进程的页表，拥有**事实上**的全物理内存访问能力。Minix3 的 `createpde` 只是一个 **capability façade**——它没有真正限制 VM 的物理内存访问权。VM 可以通过修改任何进程的页表来读写任意物理内存，只是多了一步间接操作。

Direct map 是"**显式拥有**"而非"新增权限"。安全边界没有变化，只是从 createpde 的"事实上拥有"变为 direct map 的"显式拥有"。

VM direct map 设置 NX 位，阻止代码执行，提供深度防御——即使攻击者获得了 VM 进程的执行控制权，也无法通过 direct map 执行物理内存中的代码。

#### 3.0.3 Direct map 的归属

Direct map 是 CPU/MMU architecture mechanism，不是 VM policy——它不表达"哪个进程该拥有什么内存"，只表达"如何稳定访问 physical memory"。但"谁使用 ≠ 谁构造"：

- **Kernel direct map**：由 VM 在 `map_kernel()` 中建立（Minix3 的 VM 本来就负责 `map_kernel()`），但建立后视为只读不变量，VM 不再修改
- **VM direct map**：由 VM 在初始化时建立，仅存在于 VM 进程页表

这比"kernel 建立 direct map"更符合微内核精神——VM 是 address-space architect，kernel 只是消费者。

#### 3.0.4 问题一：内核如何访问其他进程的页表

Minix3 内核运行时使用当前进程的页表，没有物理内存的直接映射。`pagedir_mappings` 为每个进程分配一个固定的虚拟地址窗口（`p_cr3_v`），内核通过该窗口读写进程的页目录。

minix-rs 采用 kernel direct map：内核启动时在高位虚拟地址建立所有物理内存的线性映射，关系为 `va = pa + KERNEL_DIRECT_MAP_BASE`。内核拿到任意物理地址后，通过 `kernel_phys_to_virt()` 一行加法即可得到可用的虚拟地址。

#### 3.0.5 问题二：跨进程内存拷贝

Minix3 的 `sys_datacopy` 需要将目标进程的 PDE 写入当前进程页目录的 `freepdes` 空闲槽位，建立 4MB 临时映射窗口，拷贝完成后清除映射（详见附录 A）。

minix-rs 直接映射区方案下，内核将源/目标虚拟地址翻译为物理地址，再通过 `kernel_phys_to_virt()` 得到内核虚拟地址，直接 `memcpy` 即可，无需修改页表。

> **Direct Map 深化**：`sys_datacopy` 的消失不是"优化了跨进程复制"，而是 **"跨进程复制这个概念本身被重新定义"**。
>
> 在 Minix3 中，"跨进程复制"是一个特殊的操作——需要内核介入，修改页表，建立临时映射窗口。这是因为内核无法直接看到其他进程的物理内存。
>
> 在 direct map 方案下，"跨进程复制"退化为"普通 memcpy"——内核已经能看到所有物理内存，源和目标只是两个不同的物理地址。`sys_datacopy` 这个系统调用不再需要，因为它的存在前提（"内核无法直接看到物理内存"）被消除了。
>
> VM 侧同理：VM 通过 `vm_phys_to_virt()` 可以直接看到所有物理内存，跨进程复制也是普通 memcpy。VM 不再需要 `sys_datacopy` 系统调用——它自己就是内存的来源，也是内存的观察者。

#### 3.0.6 对比总结

| 场景 | Minix3（32 位） | minix-rs（64 位 + 双视图直接映射区） |
|------|-----------------|-------------------------------|
| 访问进程页目录 | `p_cr3_v`（固定窗口） | `kernel_phys_to_virt(paddr)` |
| 跨进程内存拷贝 | `freepdes` 临时映射 → 拷贝 → 清除 | 翻译虚拟地址 → `kernel_phys_to_virt()` → 直接 memcpy |
| 页表修改 | 需要（临时注入 PDE） | 不需要 |
| 并发风险 | 有（临时借用页表） | 无 |
| VM 访问物理内存 | `createpde` 临时映射 | `vm_phys_to_virt()` 永久映射 |
| VM 安全边界 | 事实上拥有（capability façade） | 显式拥有（direct map + NX） |

#### 3.0.7 `map_kernel` 职责简化

每个用户进程的页表中必须映射内核代码段和数据段，否则中断/系统调用进入 ring 0 时无法执行。直接映射区消除了 `page_directories` 登记册和 `freepdes` 空闲槽位的映射需求，但增加了 kernel direct map 的建立：

| 映射内容 | Minix3 `pt_mapkernel` | minix-rs `map_kernel` |
|----------|----------------------|----------------------|
| 内核代码/数据段 | ✅ 必须 | ✅ 必须 |
| Kernel direct map（1GB huge pages, U/S=0, G=1） | ❌ 不存在 | ✅ 必须 |
| `page_directories` 登记册 | ✅ 必须 | ❌ 不需要 |
| `freepdes` 空闲槽位 | ✅ 必须预留 | ❌ 不需要 |

Kernel direct map 设 Global 位（G=1），CR3 切换时不刷新这部分 TLB 条目。这是安全的，因为 kernel direct map 是"建立后只读不变量"——`map_kernel()` 建立后 VM 不再修改 kernel direct map 的 PTE/PDE/PDPT 表项。如果未来需要动态修改（如内存热插拔），需要设计显式的 TLB 刷新协议（跨核 shootdown）。但当前设计中，不变的东西不需要管理。

### 3.1 Minix3 函数映射

| Minix3 函数 | Rust 对应 | 层级 | 说明 |
|-------------|----------|------|------|
| `pt_new()` | `Paging::new()` | 硬件机制 | 创建页表 |
| `pt_bind()` | `VmPagingExt::bind_to_process()` | VM 策略 | 绑定页表到进程（通知内核） |
| `pt_free()` | `Paging::destroy()` | 硬件机制 | 销毁页表 |
| `pt_writemap()` | `Paging::map()` | 硬件机制 | 建立映射 |
| `pt_checkrange()` | `Paging::query()` | 硬件机制 | 检查映射 |
| `pt_copy()` | `query()` + `map()` 组合 | 跨页表操作 | 全量 PTE 复制（fork），见 §3.2 |
| `pt_map_in_range()` | `query()` + `map()` 组合 | 跨页表操作 | 范围 PTE 复制（VM 热更新），见 §3.2 |

**关键区别**:
- Minix3 直接操作硬件页表结构
- Rust 通过 `Paging` trait 抽象，支持 Mock 测试
- 两者都使用显式生命周期管理（`pt_free()` / `destroy()`），内核代码偏好显式控制而非隐式析构

### 3.2 操作分层

页表操作在 Rust 中分为三个层级，每层有不同的抽象粒度和职责：

| 层级 | 抽象 | 操作对象 | 示例 |
|------|------|---------|------|
| **硬件机制层** | `Paging` trait | 单个页表 | `map()`、`unmap()`、`query()`、`switch()` |
| **VM 策略层** | `VmPagingExt` trait | 单个页表 + 内核协作 | `bind_to_process()`、`map_kernel()` |
| **跨页表操作层** | 基于 `Paging` 的组合函数 | 两个页表之间 | `pt_copy` → `query(src)` + `map(dst)` 循环 |

**跨页表操作的本质**：`pt_copy` 和 `pt_map_in_range` 都不是硬件机制——它们不操作单个 PTE 的硬件位编码，而是在两个页表之间**搬运已有的映射关系**。因此它们不是 `Paging` trait 的方法，而是基于 `Paging` 的组合操作：

```
pt_copy(src, dst) ≡ clone_range(src, dst, 0, VM_USER_TOP)
pt_map_in_range(src, dst, start, end) ≡ clone_range(src, dst, start, end)
```

```rust
// 统一的跨页表 PTE 复制函数
pub fn clone_range<P: Paging>(
    src: &P, dst: &mut P,
    start: VirBytes, end: VirBytes,
) -> Result<(), PageTableError>
```

**Minix3 的 `memcpy` vs Rust 的 `query()`+`map()`**：Minix3 的 `pt_copy` 直接 `memcpy` 整个二级页表内容（4KB，1024 个 PTE），这是 32 位特有的优化——PTE 是 `u32`，页表恰好占一页。x86-64 下 PTE 是 `u64`，页表结构不同，且 `query()`+`map()` 更符合抽象层级。如果性能是关键，arch 实现可以提供 `clone_range()` 的批量优化（如 x86-64 直接复制整个 PT 页），但这属于实现优化，不是 trait 接口。

**`pt_copy` vs `pt_map_in_range`**：语义上 `pt_copy` 是 `pt_map_in_range` 的全量版本（复制所有用户空间 PDE），见 §2.0.2 的分析。Rust 统一为 `clone_range()` 函数——`pt_copy` 对应 `clone_range(src, dst, 0, VM_USER_TOP)`，`pt_map_in_range` 对应带具体 `start`/`end` 的调用。

### 3.3 安全封装

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
  (2) 批量映射需要与 Minix3 对齐的 all-or-nothing 语义，拆分后可独立设计验证阶段；
  (3) `map_range()` 提供默认实现（逐页调用 `map()`），arch 实现可覆盖以利用硬件优化（如 x86-64 批量映射后单次 CR3 reload）
- `map()` 是单页映射的底层 API，对应硬件页表项操作

**`map_range()` 的部分失败语义**：

Minix3 的 `pt_writemap` 采用"预验证+执行"两阶段模式：
1. `pt_ptalloc_in_range` 确保覆盖该范围的所有二级页表已分配
2. 逐页写入 PTE

若预验证失败，不写入任何 PTE——all-or-nothing 语义，不会留下部分映射。

Rust `map_range()` 采用相同策略：先 `query()` 遍历整个范围确认无冲突，再逐页 `map()`。若验证阶段发现任何地址已映射（返回 `AlreadyMapped`）或地址无效（返回 `InvalidAddress`），立即返回错误，不做任何写入。这与 Minix3 的语义完全对齐。

arch 实现可覆盖默认实现，将验证与写入合并为单次遍历（如 x86-64 在遍历 PTE 时同时检查和写入），但必须保证"验证失败时不留部分映射"的语义不变。

### 3.4 错误处理

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

### 3.5 页表子系统初始化

> Minix3 的 `pt_init()` 有 6 个阶段（§2.0.1），本节分析 Rust 版本中哪些保留、哪些消除、新增什么。

#### 3.5.1 Minix3 → Rust 阶段对照

| Minix3 阶段 | Rust 状态 | 理由 |
|---|---|---|
| 1. 备用页池建立 | **消除** | Direct Map 下 VA 不是需要"分配"的资源（05 §3.3），不存在 `pt_init_done` 前后的阶段切换（05 §3.3 "不存在页表系统未就绪的阶段"） |
| 2. CPU 特性检测 | **保留但方式改变** | 不再是 `pt_init` 中的运行时检测 + 全局变量。改为 `HugePages` trait 的方法调用——`supports_1gb_page()` 由 arch 实现（06 §3.4），`DirectMapArch` 通过 `HugePages` 获取大页参数（06 §3.6.3 "大页参数从 `HugePages` 获取"） |
| 3. `kern_mappings` 初始化 | **消除** | Direct Map 替代了 `kern_mappings` 的所有用途——内核不再需要从 VM 获取预留映射信息 |
| 4. `pagedir_mappings` 初始化 | **消除** | §3.0 核心决策 |
| 5. VM 自身页表建立 | **保留但大幅简化** | 仍需 `Paging::new()` + `map_kernel()` + `bind_to_process()`，但：①不需要 `sys_vircopy`/`sys_abscopy`——VM 通过 `vm_phys_to_virt()` 直接读写；②不需要 `pt_bind` 写 `pagedir_mappings`——`bind_to_process()` 仅调用 `sys_vmctl_set_addrspace()`；③初始页表由 kernel 建立（06 §3.6.4），VM 启动时已有基础映射，只需扩展 |
| 6. 动态化重建 | **消除** | 没有静态备用页，VM 页表从创建起就是纯动态的 |

#### 3.5.2 Rust 新增阶段

| 阶段 | 说明 | 来源 |
|---|---|---|
| 大页能力确认 | `HugePages::supports_1gb_page()` 决定 direct map 用 1GB 还是 2MB 大页，影响扩展 direct map 时的映射策略 | 06 §3.6.3 "回退逻辑在初始页表构建阶段完成" |
| VM direct map 扩展 | 初始页表由 kernel 建立了前 1GB 的 direct map（06 §3.6.4），但 VM 可能需要扩展 direct map 覆盖超过 1GB 的物理内存 | 06 §3.6.4 "VM 启动后可以自行扩展 direct map" |

#### 3.5.3 Rust `paging_init` 流程

```
阶段 1: 大页能力确认
  - HugePages::supports_1gb_page()   // Paging trait 的可选方法
  - 决定 direct map 用 1GB 还是 2MB 大页

阶段 2: VM 自身页表建立（基于 kernel 提供的初始页表）
  - Paging::new()                    // 可选：如果需要替换初始页表
  - map_kernel()                     // ⚠️ TODO（当前仅有 Mock 实现）
  - VM direct map 扩展               // ⚠️ TODO（物理内存 > 1GB 时需要）
  - bind_to_process()                // ✅ 已定义（Mock 中为 no-op）

阶段 3: （未来）SMP 其他 CPU 的页表同步
```

**与 27-vm-init-main.md 的对应**：27 中 `init_phase2()` 已有 `vm_proc.init_page_table()` 调用，`paging_init()` 是其内部逻辑：
- 大页能力确认属于 `paging_init()` 内部第一阶段
- VM direct map 扩展在物理内存 > 1GB 时需要
- `map_kernel()` 具体内容属于 #13 的范畴

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

**实现要点**：

- `Paging::new()` 仅分配根页表（x86-64 为 PML4），中间页表（PDPT/PD/PT）在 `map()` 时按需分配，对调用者透明
- `destroy()` 需递归遍历所有已分配的中间页表并释放物理页，x86-64 实现需遍历 PML4 → PDPT → PD → PT 四级结构
- `root_paddr()` 返回根页表的物理地址，供 `bind_to_process()` 传递给内核

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

**实现要点**：

- **`map()`**：中间页表（PDPT/PD/PT）按需分配，对调用者透明。地址对齐检查在 trait 实现内部完成，未对齐返回 `InvalidAddress`。x86-64 实现中，PTE 写入为 8 字节自然对齐的原子操作（Intel SDM Vol3 §4.10.4）
- **`remap()`**：原子覆盖语义，避免 `unmap()` + `map()` 之间的无映射窗口。在单线程事件循环模型下逻辑原子（无中间状态可见），硬件层面 PTE 写入也是原子的。返回旧映射的 `(PhysBytes, PageFlags)`，供调用者决定是否释放物理页
- **`map_range()`**：默认实现为两阶段——Phase 1 预验证（逐页 `query()` 确认无冲突），Phase 2 执行（逐页 `map()`）。arch 实现可覆盖为单遍扫描。使用 `checked_mul`/`checked_add` 防止地址溢出。保证 all-or-nothing 语义：失败时无部分映射残留
- **`unmap()`**：仅取消映射并返回原物理地址，不释放物理页。物理页释放由 `phys_block` 引用计数管理
- **`clone_range()`**：跨页表组合函数（非 trait 方法），通过 `src.query()` + `dst.map()` 实现映射复制。跳过源页表中无映射的地址（对应 Minix3 的 absent PDE/PTE 静默跳过行为）。`start=0, end=VM_USER_TOP` 时等价于 Minix3 的 `pt_copy()`

### 4.3 进程绑定

| 操作 | Minix3 | Rust |
|------|--------|------|
| 绑定页表 | `pt_bind()` | `VmPagingExt::bind_to_process()` |
| 激活页表 | `setcr3()` (内核) | `unsafe fn switch()` |
| 映射内核 | `pt_mapkernel()` | `VmPagingExt::map_kernel()` |

**绑定流程**：`bind_to_process()` 将页表与进程关联，通知内核该进程的地址空间根地址。对应 Minix3 的 `pt_bind()` → `sys_vmctl_set_addrspace()` → 内核 `setcr3()`。

> **语义差异**：Minix3 的 `pt_bind()` 实际做两件事：(1) 将页目录物理地址写入 `pagedir_mappings` 登记册（内核通过此登记册间接访问进程页目录）；(2) 调用 `sys_vmctl_set_addrspace()` 通知内核。Rust 的 `bind_to_process()` 仅对应第 (2) 步，因为 x86-64 使用直接映射区（direct-map），内核可直接通过物理地址访问任何进程的页目录，不需要 `pagedir_mappings` 登记册。第 (1) 步在 64 位架构下不需要。

**`map_kernel()`**：每个用户进程的页表中必须映射内核地址空间，否则中断/系统调用进入 ring 0 时无法执行。当前内核部分尚未就绪，`map_kernel()` 的实现暂为 TODO。详见 [3.0 节](#30-架构决策取消-pagedir_mappings采用直接映射区)中 `map_kernel` 职责简化的讨论。

#### 4.3.1 映射内容清单

`map_kernel()` 建立三部分映射，按以下顺序建立：

> **注**：代码段和数据段因权限不同（代码段可执行、数据段不可执行）而分开描述，但在逻辑上属于同一映射段（内核代码/数据段）。因此 §4.3.4 与 Minix3 的对比中按逻辑段计数为"2 段"：第 1 段 = 内核代码/数据段，第 2 段 = Kernel direct map。

| 顺序 | 映射内容 | VA 范围 | PA 来源 | 页面类型 | 权限标志 | 说明 |
|------|---------|---------|---------|---------|---------|------|
| 1 | 内核代码段 | `KERNEL_TEXT_START` ~ `KERNEL_TEXT_END` | `kern_mb_mod->mod_start` | 2MB huge pages | P + RW + G + NX⁻ (可执行) | 对应 Minix3 的 4MB 大页映射 |
| 2 | 内核数据段 | `KERNEL_DATA_START` ~ `KERNEL_DATA_END` | 紧接代码段之后 | 2MB huge pages | P + RW + G + NX (不可执行) | 代码段和数据段权限不同 |
| 3 | Kernel direct map | `KERNEL_DIRECT_MAP_BASE` ~ `KERNEL_DIRECT_MAP_BASE + total_phys` | `0` ~ `total_phys` | 1GB huge pages（回退 2MB） | P + RW + G + NX + U/S⁻ | 映射全部物理内存；U/S=0（仅内核可访问）；G=1（CR3 切换不刷新 TLB） |

**地址来源**：

- 内核代码/数据段：由 kernel 在启动时通过 boot_info 传递（`kern_mb_mod->mod_start`、`kern_size`），VM 在 `paging_init()` 中从 boot_info 读取
- Kernel direct map：基址由 `DirectMapArch::KERNEL_DIRECT_MAP_BASE` 常量定义（x86-64 = `0xFFFF_8000_0000_0000`），长度由 `total_phys_bytes` 决定（来自 boot_info 的物理内存映射）
- 1GB 大页回退：若 `HugePages::supports_1gb_page()` 返回 false，则每 1GB 段映射为一个 PD 页（512 × 2MB 条目），此时需要额外分配 PD 页

#### 4.3.2 实现顺序与依赖

```
map_kernel() 内部步骤：

1. 内核代码段映射
   - VA = KERNEL_TEXT_START（boot_info 提供）
   - PA = kern_mb_mod->mod_start（boot_info 提供）
   - 大页对齐：2MB huge pages
   - 权限：Present + ReadWrite + Global + Execute

2. 内核数据段映射
   - VA = KERNEL_DATA_START（代码段结束后，2MB 对齐上取整）
   - PA = 紧接代码段物理地址之后（2MB 对齐）
   - 大页对齐：2MB huge pages
   - 权限：Present + ReadWrite + Global + NoExecute

3. Kernel direct map 映射
   - VA = KERNEL_DIRECT_MAP_BASE（DirectMapArch 常量）
   - PA = 0
   - 大页对齐：1GB huge pages（回退 2MB）
   - 权限：Present + ReadWrite + Global + NoExecute + Supervisor-only（U/S=0）
   - 循环：for each 1GB segment from PA=0 to PA=total_phys
     - 若 supports_1gb_page：map_huge(va, pa, 1GB, flags)
     - 否则：分配 PD 页，逐项 map_huge(va, pa, 2MB, flags)

4. 刷新 TLB（仅热更新场景）
   - 新建页表时不需要刷新 TLB（尚未 switch 过，无 stale TLB 条目）
   - 如果未来支持在运行中的页表上热更新 kernel mappings（如内存热插拔），则需刷新 TLB 并设计跨核 shootdown 协议
```

#### 4.3.3 Kernel Direct Map 只读不变量

`map_kernel()` 建立后，VM **不再修改** kernel direct map 的 PTE/PDE/PDPT 表项。这个约束不是限制，而是简化——不变的东西不需要管理：

- **Global 位的安全性**：G=1 的 TLB 条目在 CR3 切换时不刷新，这是安全的因为内容不变
- **修改的场景**：如果未来需要动态修改（如内存热插拔），需设计显式的 TLB 刷新协议（跨核 shootdown），但当前不需要
- **与 VM direct map 的对比**：VM direct map 是 VM 自己的映射，可以自由扩展（如物理内存 > 1GB 时追加映射）；kernel direct map 由 `map_kernel()` 一次性建立，此后只读

#### 4.3.4 与 Minix3 的对应

| 方面 | Minix3 `pt_mapkernel` | minix-rs `map_kernel` |
|------|----------------------|----------------------|
| 映射段数 | 3 段 | 2 段（逻辑段） |
| 第 1 段 | 内核代码段（4MB 大页） | 内核代码/数据段（2MB huge pages，按权限拆为两部分） |
| 第 2 段 | `pagedir_mappings` 登记册 | Kernel direct map（1GB huge pages） |
| 第 3 段 | `kern_mappings` 特殊映射 | 不需要（设备映射走 VM_MAP_PHYS） |
| 代码/数据段权限 | 统一 P + RW + G | 分离：代码段可执行，数据段不可执行 |
| 大页回退 | 无（x86-32 仅 4MB） | 1GB → 2MB（`HugePages::supports_1gb_page()`） |

#### 4.3.5 `pt_clearmapcache` 消除与 TLB 一致性

Minix3 的 `pt_clearmapcache`（§2.4.4）通过 `VMCTL_CLEARMAPCACHE` 通知内核丢弃 `freepdes` 中缓存的 PDE。Direct Map 下此操作**完全消除**：

- **消除原因**：`freepdes` 机制不存在（§3.0.4），内核通过 kernel direct map 直接访问页目录，无需缓存
- **无等价操作**：`VMCTL_CLEARMAPCACHE` 的语义是"内核丢弃旧的 PDE 缓存"，Direct Map 下内核不需要缓存 PDE

**页表变更后的 TLB 一致性**由另一套机制承担：

| 层级 | 机制 | 当前状态 |
|------|------|---------|
| 本地 CPU | `Paging::flush_tlb()` / `flush_tlb_addr()` | ✅ 已定义在 trait 中 |
| 跨 CPU（SMP） | TLB shootdown 协议 | ❌ 未实现 |

SMP TLB shootdown 不属于 `Paging` trait 或 07 的范畴——VM 运行在 ring 3，不能直接发 IPI，需要通过 syscall 请求 kernel 执行。shootdown 协议涉及 kernel 的调度器、IPI 中断处理等，属于 kernel-VM 协议设计。当前阶段（单核）不需要此协议；SMP 支持时需要设计 `sys_tlb_shootdown()` 系统调用。

> **与 Minix3 的区别**：Minix3 的 `pt_clearmapcache` 是"通知内核丢弃缓存"的语义，而非"刷新 TLB"的语义。Minix3 的内核没有 SMP 支持，不需要 TLB shootdown。minix-rs 将两者分开——`pt_clearmapcache` 消除，TLB 一致性由 `flush_tlb()`（本地）+ 未来 shootdown 协议（跨核）承担。

> fork 时的跨页表 PTE 复制机制见 §3.2 操作分层。`pt_copy` 和 `pt_map_in_range` 是 `Paging` trait 之上的组合操作（`query()`+`map()` 循环），不是 trait 方法。
>
> fork **策略**（哪些页需要复制、CoW 标记、`phys_block` 引用计数）详见 [10-phys-block.md](10-phys-block.md) 和 [17-vm-fork.md](17-vm-fork.md)。

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
| `update_flags()` 不能用于 Direct Map slab 写保护 | Direct Map PTE 是共享的，修改会影响所有访问路径 | Minix3 的 `vm_pagelock` 在 Direct Map 下失效（详见 08 §4.1 "vm_pagelock 消除决策"） |
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
- [27-vm-init-main.md](27-vm-init-main.md) - VM 初始化主流程

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

#### A.3 Direct Map 视角：createpde/freepde 的历史意义

> **Direct Map 标注**：x86-64 下 kernel direct map 使 createpde/freepde 机制完全不再需要。

**createpde/freepde 存在的原因**：Minix3 的 x86-32 内核没有直接映射区，无法直接访问物理内存。内核只有 2 个空闲 PDE 槽位（`MAXFREEPDES = 2`），通过轮转 2 个 4MB 窗口来访问非当前进程的物理内存。这是 x86-32 地址空间极端限制下的产物——3GB 用户空间 + 1GB 内核空间，内核空间还要容纳内核代码/数据、内核栈、页表等，留给临时映射的空间只有 2 × 4MB。

**x86-64 的根本变化**：64 位地址空间使 kernel direct map 成为可能——内核可以映射所有物理内存到高位虚拟地址空间，无需临时映射窗口。`createpde` 的 2 × 4MB 轮转窗口被 `kernel_phys_to_virt()` 的永久映射替代。

**保留历史描述的价值**：理解 `createpde` 的极端限制（2 个 PDE 槽位、4MB 窗口、轮转使用）有助于读者理解为什么 x86-64 的 direct map 是"架构决定的方案选择"而非"可选的优化"——32 位地址空间根本放不下 direct map，64 位地址空间天然适合。

---

*分类: VM库 | 可被其他服务使用*
