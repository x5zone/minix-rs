# 07-cross-space-init: 跨地址空间初始化——从临时窗口到 direct_map

> **分类**: 全局基建（内核启动阶段 D）
> **源码**: `minix3/minix/kernel/arch/i386/protect.c` · `minix3/minix/kernel/arch/i386/pg_utils.c` · `minix3/minix/kernel/arch/i386/memory.c`
> **说明**: 内核如何获得"看"别的进程地址空间的能力——Minix3 用 32 位临时窗口（freepdes/ptproc），minix-rs 用 64 位 direct_map 重新表达。
> **Redesign 依据**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-pagetable-struct.md` §3.6（direct_map 设计）、`02-stage-vm/07-pagetable-ops.md` §3.0.7（map_kernel 职责简化）

---

## 1. 概述

### 1.0 本章讲什么

kmain 的启动流程分为六个阶段：

| 阶段 | 名称 | 核心动作 | 文档 |
|------|------|---------|------|
| A | 入口 | 固件交接、栈建立 | 01-04 |
| B | cstart | 早期硬件初始化、解析启动信息 | 05 |
| C | 进程表初始化 | 清空进程表、加载 VM ELF | 06 |
| **D** | **跨空间初始化** | **内核获得跨地址空间访问能力** | **07（本文）** |
| E | 系统初始化 | 系统调用注册、子系统启动 | 08 |
| F | 启动完成 | 调度开始、bsp_finish_booting | 08 |

阶段 D 要回答一个核心矛盾：**内核运行在自己的地址空间里，却要读写用户进程的内存**。IPC 消息拷贝（`lin_lin_copy`）、`vm_memset` 等内核服务都需要穿越进程隔离——目标内存不在当前页表的用户空间映射里。这就是跨地址空间访问问题。

这个问题的解法随虚拟地址空间位宽演进：

- **32 位时代**：虚拟地址空间只有 4GB，内核和用户共享，没有余量建立全物理内存的固定映射。解法是**临时窗口**——在当前页目录里预留 2 个槽位（freepdes），需要访问目标进程内存时，把目标页目录项（PDE）临时写入槽位，用完清掉。这是 Minix3 的方案。
- **64 位时代**：虚拟地址空间 256TB+，有余量建立"全物理内存 → 固定虚拟地址区间"的线性映射（`va = pa + BASE`）。内核要访问任意物理内存时，直接算出 VA，MMU 按 direct map 的 PTE 解释——不需要临时窗口、不污染任何页目录、不需要清理。这是 Linux/Windows/BSD 主流内核的通用模式，也是 minix-rs 的选择。

**本章立场**：minix-rs 选择 direct_map，废弃 32 位的临时窗口机制。阶段 D 从"分配临时窗口"简化为"确认 direct_map 就绪"。

**目标读者**：已理解 Minix3 微内核基本结构、了解 x86 分页机制、读过 [06-proc-init-boot-proc-new.md](06-proc-init-boot-proc-new.md)（阶段 C）的开发者。

**本章不讲什么**：

- direct_map 的完整建立过程（VM server [06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §3.6 已详述，本文仅引用）
- `createpde()` 的运行时使用（后续 24-cross-space-runtime.md）
- `map_kernel()` 的完整实现（VM server [07-pagetable-ops.md](../02-stage-vm/07-pagetable-ops.md) §3.0.7，本文仅讲协作关系）

### 1.1 核心矛盾：内核如何"看"别人的地址空间

每个进程有独立页表——CR3 装的是当前进程的页表根，MMU 按 CR3 指向的页表解释虚拟地址。这是进程隔离的基础，也是 OS 核心机制。

内核映射在每个进程的地址空间里。中断或系统调用进入 ring 0 时，CPU 必须能立即执行内核代码，所以每个进程的页表都映射了内核段（高地址区）。进程切换时，CR3 换成新进程的页表，但内核段映射始终可见——这是为什么内核能在任何进程的上下文里运行。

但内核要访问**别的**进程的用户态内存时，问题出现了：目标内存的 VA 在当前页表里没有映射（它映射在目标进程的页表里）。CPU 当前的页表解释不了这个 VA。

> **灵魂本质**：跨地址空间访问 = "CPU 当前页表解释不了目标 VA，如何临时让它解释得了"。

这个能力是内核运行时服务的基石：IPC 消息要在进程间拷贝、`fork` 要复制父进程地址空间、`exec` 要加载新镜像——都依赖内核能读写目标进程的内存。

### 1.2 32 位解法：临时窗口（历史包袱）

> **架构范围**：x86-32 特有设计（4MB 大页、1024 项页目录）。64 位不沿用。

32 位虚拟地址空间只有 4GB，内核和用户共享，没有余量建立全物理内存的固定映射。解法是**借当前页目录开临时后门**：

1. 在页目录里预留 2 个槽位（freepdes），需要访问目标进程内存时，把目标 PDE 临时写入槽位
2. "借谁的页目录"——借当前 CPU 正在用的页目录（`ptproc` 指向它），因为 MMU 只按 CR3 装的页目录解释 VA
3. 用完清空槽位（`mem_clear_mapcache`），防残留

这个机制有三个丑陋之处：

- **污染当前页目录视图**：临时写入的 PDE 改变了当前进程的地址空间视图，用完必须清理
- **TLB 反复 flush**：每次写入/清理 PDE 都要 invalidate TLB，性能损耗
- **4MB 粒度限制**：PDE 对应 4MB 区间，即使只访问 1 字节也要映射整段

> **灵魂本质**：临时窗口 = 在别人的页目录上开两个临时后门，用完堵上。

这是地址空间受限的妥协。Minix3 的 `freepdes`/`ptproc`/`createpde`/`mem_clear_mapcache` 都是这套机制的组成部分。

### 1.3 64 位解法：direct_map（现代方案）

> **架构范围**：三架构统一抽象（DirectMapArch trait），BASE 值各架构不同。

64 位虚拟地址空间 256TB+，有余量建立"全物理内存 → 固定虚拟地址区间"的线性映射。这就是 direct_map：

```
va = pa + BASE
```

内核要访问任意物理内存时，直接 `kernel_phys_to_virt(pa)` 得到 VA，MMU 按 direct map 的 PTE 解释——不需要临时窗口、不污染任何页目录、不需要清理。

**双视图地址空间**（硬件特权级要求）：同一物理内存需要两个窗口——

| 窗口 | U/S 位 | 使用者 | 建立者 | 建立时机 |
|------|--------|--------|--------|---------|
| VM direct map | U/S=1 | VM 用户态 | kernel | 阶段 C（VM 启动前） |
| Kernel direct map | U/S=0, G=1 | 内核态 | VM（`map_kernel`） | VM 启动后 |

为什么需要两个窗口？x86-64 的 U/S 位不能同时 0 和 1。如果只有一个 direct map，要么 VM 用户态访问不了（U/S=0），要么内核安全降级（U/S=1）。两个窗口是硬件特权级的必然要求，不是冗余。

#### 1.3.1 双视图的硬件根源（ISA U/S 位语义）

> **新增**（2026-07-16，TODO-07-6）：把"为什么必须两个视图"独立成子节，便于读者抓住 ISA 必然性。

x86-64 的页表项（PTE）有一个二元 U/S 位（User/Supervisor bit），定义如下：
- **U/S=1**：PTE 被 ring 3（用户态）访问时，TLB/MMU 允许
- **U/S=0**：PTE 仅被 ring 0（内核态）访问时，TLB/MMU 允许；用户态访问触发 #PF（page fault）

**问题**：同一物理地址需要同时被 VM 用户态访问（VM 业务逻辑要读写自己的页表）和内核态访问（kernel 的 IPC 拷贝、vm_memset 等）。一个 PTE 只能设一个 U/S 位——若全部设 1，内核能访问但失去特权隔离（VM 用户态可读写内核 PTE）；若全部设 0，VM 用户态无法访问自己的页表（VM 业务依赖用户态 VA）。

**解法**：同一物理内存**建立两个 PTE**，分别在两个虚拟地址区间：
- **VM direct map**（U/S=1）：VM 进程页表里加一段 [VM_DIRECT_MAP_BASE, VM_DIRECT_MAP_BASE + physmem_size) 的 PTE 数组，U/S=1 让 VM 用户态访问
- **Kernel direct map**（U/S=0, G=1）：所有进程页表里加一段 [KERNEL_DIRECT_MAP_BASE, + physmem_size) 的 PTE 数组，U/S=0 隔离用户态，G=1 让 CR3 切换不刷 TLB

**架构差异**：
- **x86-64**：U/S 是 PTE 第 2 位，由 CPU 在 TLB lookup 时强制检查；同物理页的两个 PTE 共享同一个内存页
- **aarch64**：等价于 PTE 的 AP[2:1] 位（Access Permissions）；UXN/PXN 控制执行权限
- **riscv64**：等价于 PTE 的 X/W/R 位与 SUM 位（S-mode 能访问 U-mode 内存的开关）

三个 ISA 机制名不同，但**问题域相同**——必须有两个视图。直接 map 的 BASE 值三架构不同（见 §4.1），但双视图的**必然性是 ISA 规范**，不是 minix-rs 的设计选择。

> **灵魂本质**：双视图 = ISA 强制（U/S 不能同时 0 和 1）+ 性能优化（G=1 跨 CR3 切换 TLB 不刷新）。

> **灵魂本质**：direct_map = 给所有物理内存一个永久的虚拟地址，临时窗口的"借/还"整个消失。

Minix3 全线没有 direct map（`pmap.h` 的 `PMAP_DIRECT_MAP` 宏受 `#ifdef __HAVE_DIRECT_MAP` 保护且从未定义，见 [VM 06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §3.6）。direct_map 是 minix-rs 全新引入的架构演进。

### 1.4 阶段 D 的位置与简化

阶段 D 在 C 里是两行代码：`arch_post_init`（设 ptproc + `pg_info`）+ `memory_init`（分配 freepdes）。

direct_map 下这两行的语义变化：

| C 代码 | 32 位语义 | direct_map 下 |
|--------|----------|--------------|
| `ptproc = VM` | 记录"当前页目录是 VM 的"，freepdes 借此页目录 | 废弃——kernel 有 Kernel direct map，不借页目录 |
| `pg_info()` | 记录 bootstrap 页表的物理/虚拟地址 | 废弃——kernel 用 `kernel_phys_to_virt` 直接访问 |
| `memory_init` 分配 freepdes | 领取 2 个临时窗口槽位 | 废弃——direct map 是永久映射 |

阶段 D 简化为：**确认 direct_map 已就绪**。VM direct map 由 kernel 在阶段 C 建立 VM 进程时建好（详见 [VM 06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §3.6 的 4 页初始页表结构）；阶段 D 只需确认它已就绪，无需分配任何东西。

> **灵魂本质**：阶段 D 从"分配临时窗口"降级为"确认永久窗口已开"。

注意：Kernel direct map 不在阶段 D 范围——它由 VM 启动后通过 `map_kernel` 建立（见 §3.4）。阶段 D 确认的是 VM direct map 就绪。

> **实现状态**（2026-07-16，TODO-07-1N）：当前 `os/kernel/src/lib.rs` 的 `init_post_and_memory` 函数**仍翻译 C 旧逻辑**——
> - [lib.rs:1045](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs) 调用 `CurrentPostInitArch::set_ptproc`（应废弃——记录 arch 内部状态 `virt_root` 供 createpde 用，但 createpde 已被 Direct Map 取代）
> - [lib.rs:1057](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs) 调用 `set_current_ptproc_nr(VM_PROC_NR)`（**P9-4 新增，不废弃**——这是 kernel 级 ptproc 跟踪，使 `dispatch_vmctl(SetAddrSpace)` 能决定是否 reload CR3；详见 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md)）
> - 行 1087 调用 `CurrentMemoryInitArch::allocate_free_pdes`（应废弃）
> - 行 1098 写入 `FREE_PDE_SLOTS` 全局可变状态（应废弃）
>
> **两层 ptproc 跟踪的区分**（P9-4 澄清）：arch 层 `PostInitArch::set_ptproc` 记录 arch 内部状态（`virt_root` 供 createpde 借页目录用——但 createpde 已被 Direct Map 取代，故应废弃）；kernel 层 `set_current_ptproc_nr`（[lib.rs:2075](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）记录 VM 的 proc-nr，使 `SetAddrSpace` 的 Step 3（`if current_ptproc_nr() == Some(target.p_nr)`）能判断是否需立即 reload CR3。后者是 `setcr3()` 语义的直接对应（C: `if (p == get_cpulocal_var(ptproc))`），**不是** createpde 临时窗口机制的一部分，因此 direct_map 下仍需保留。
>
> 这与本节"阶段 D 简化为确认就绪"的设计承诺直接矛盾。**修复方向**：将 `init_post_and_memory` 重构为"VM direct map base != 0 + vm_proc.page_table_root.is_valid()"两条断言，删除 set_ptproc/allocate_free_pdes/FREE_PDE_SLOTS 三处调用（但**保留** `set_current_ptproc_nr`）。DirectMapArch（vm.rs:21）已接入，但 `init_post_and_memory` 自身需要清理。此修复是 **Action Item #1**（非本 session 范围，留作代码任务）。

### 1.5 本章小结

- 跨地址空间访问是内核核心能力，解法随位宽演进
- 32 位临时窗口（freepdes/ptproc）是地址空间受限的妥协；64 位 direct_map 是地址空间充裕的自然解
- minix-rs 选择 direct_map，阶段 D 简化为确认 VM direct map 就绪
- 关键不变量：direct_map 建立后只读不变（VM 不再修改 Kernel direct map 的 PTE，G=1 保证 CR3 切换不刷新 TLB）

---

## 2. C 源码分析

> 本章讲 Minix3 的 32 位实现，作为"被替代的历史方案"分析。所有引用均标注 `file:line`。

### 2.1 arch_post_init()：设置 ptproc + 记录页表地址

`arch_post_init` 是阶段 D 的第一步（`minix3/minix/kernel/arch/i386/protect.c:370-377`）：

```c
void arch_post_init(void)
{
  struct proc *vm;
  vm = proc_addr(VM_PROC_NR);          /* 找到 VM 进程 */
  get_cpulocal_var(ptproc) = vm;       /* ptproc = VM */
  pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);  /* 记录页表地址 */
}
```

三步动作：

1. `proc_addr(VM_PROC_NR)`：获取 VM 进程的 `struct proc`
2. `get_cpulocal_var(ptproc) = vm`：把 VM 设为当前 CPU 的 ptproc
3. `pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v)`：记录页表地址

**为什么是 VM**：VM 是第一个拥有完整页表的进程。从此刻到 VM 通过 `VMCTL_SETADDRSPACE` 切换到自己页表前，内核和 VM 共享 bootstrap 页表。

**ptproc 是什么**：per-CPU 变量（`minix3/minix/kernel/cpulocals.h:55`），记录"当前 CR3 装的是谁"。ptproc 在 C 中有两个用途：
1. **freepdes 临时窗口**：createpde 借 ptproc 的页目录放临时映射——因为 MMU 只按 CR3 装的页目录解释 VA，临时映射必须写入这个页目录才有效。direct_map 下此用途废弃（kernel 有 Kernel direct map，不借页目录）。
2. **SetAddrSpace 的 CR3-reload 决策**：`setcr3()` 用 `if (p == get_cpulocal_var(ptproc))` 判断目标进程是否是当前页表进程，若是则立即 `write_cr3`（arch_do_vmctl.c:31）。direct_map 下此用途**保留**——VM 仍通过 `VMCTL_SETADDRSPACE` 切换到自己的页表，内核需知道是否该立即 reload CR3。Rust 用 kernel 全局 `CURRENT_PTPROC_NR: AtomicI32` + `set_current_ptproc_nr(VM_PROC_NR)` 跟踪（P9-4，详见 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md)）。

### 2.2 pg_info()：记录 bootstrap 页表地址

`pg_info` 记录 bootstrap 页表的物理/虚拟地址（`minix3/minix/kernel/arch/i386/pg_utils.c:312-316`）：

```c
void pg_info(phys_bytes *pagedir_ph, vir_bytes *pagedir_v)
{
  *pagedir_ph = vir2phys(pagedir);   /* 物理地址 */
  *pagedir_v = pagedir;              /* 虚拟地址 */
}
```

把全局 `pagedir`（bootstrap 页目录）的物理/虚拟地址写入 VM 的 `p_seg.p_cr3`/`p_seg.p_cr3_v`。

VM 运行后会通过 `VMCTL_SETADDRSPACE` 替换为自己的页表，所以 `pg_info` 记录的是**初始**页表地址。后续 `createpde` 借 ptproc 的页目录时，读的就是这个地址。

### 2.3 pg_mapkernel()：建立内核映射 + 返回 freepde_start

> **阶段归属**：`pg_mapkernel` 在 pre_init 阶段调用（`minix3/minix/kernel/arch/i386/pre_init.c:232`），比阶段 D 更早。本节讲它是为了说明 `freepde_start` 的来源——阶段 D 的 `memory_init` 要用它。

`pg_mapkernel` 用 4MB 大页映射内核代码/数据段（`minix3/minix/kernel/arch/i386/pg_utils.c:186-206`）：

```c
int pg_mapkernel(void)
{
  int pde;
  /* 用 4MB 大页映射内核段：kern_vir_start → kern_phys_start，长度 kern_kernlen */
  for (pde = ...; pde < ...; pde++) {
    pagedir[pde] = ... | BIG_PAGE | ...;  /* 4MB 大页 */
  }
  return pde;  /* 返回映射之后的第一个空闲 PDE 索引 = freepde_start */
}
```

`pre_init.c:232` 记录返回值：

```c
kinfo.freepde_start = pg_mapkernel();
```

这是"内核映射在每个进程地址空间"的 C 实现：每个进程页表的高位 PDE 都指向内核段。`pg_mapkernel` 返回的 `freepde_start` 是内核映射之后的第一个空闲 PDE 索引——`memory_init` 从这里分配 freepdes 槽位。

direct_map 下，内核映射变成 Kernel direct map（1GB huge page），`freepde_start` 不再需要。

### 2.4 memory_init()：分配 freepdes

`memory_init` 是阶段 D 的第二步（`minix3/minix/kernel/arch/i386/memory.c:707-717`）：

```c
static int freepdes[MAXFREEPDES];   /* memory.c:32 */
static int nfreepdes;               /* memory.c:29 */

void memory_init(void)
{
  freepdes[nfreepdes++] = kinfo.freepde_start++;  /* 领取槽位 0 */
  freepdes[nfreepdes++] = kinfo.freepde_start++;  /* 领取槽位 1 */
  assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES);  /* 越界检查 */
}
```

**为什么是 2 个**：`createpde` 的调用者 `virtual_copy_f`/`vm_memset` 需要源和目标两个临时映射——一次跨空间拷贝可能涉及两个不同的目标进程。

`freepdes[]` 是全局静态数组（`memory.c:32`），`nfreepdes` 是已分配计数。BKL 下单 CPU 执行，无需加锁。

**为什么必须在 arch_post_init 之后**：`createpde` 把临时映射写入 `get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde]`——即**当前 ptproc（VM）的页目录**（§2.5）。若 `arch_post_init` 未先执行，ptproc 未设置（cpulocals 零初始化，值 NULL），`get_cpulocal_var(ptproc)->p_seg.p_cr3_v` 解引用 NULL 崩溃。这是**因果依赖**而非惯例：freepdes 槽位只是索引，真正的映射载体是 ptproc 的页目录——MMU 只按 CR3 装的页目录解释 VA，临时映射必须写进它才有效。阶段 D 三步顺序（arch_post_init → pg_info → memory_init）由此强制。direct_map 下此依赖消失——内核用自己的 direct map 解释 VA，不借任何进程页目录。

direct_map 下，`memory_init` 整体废弃——direct map 是永久映射，不需要运行时分配槽位。

### 2.5 createpde() + mem_clear_mapcache()：临时窗口的使用与清理

`createpde` 是 freepdes 的唯一消费者（`minix3/minix/kernel/arch/i386/memory.c:69-145`）：

```c
static phys_bytes createpde(struct proc *target, vir_bytes v)
{
  if (target == get_cpulocal_var(ptproc) || iskernelproc(target)) {
    return v;  /* 目标就是当前页目录或内核：直接返回 VA */
  }
  /* 否则：读目标 PDE，写入 freepdes 槽位，返回临时窗口 VA */
  pde = ...;  /* 选一个空闲 freepde 槽位 */
  get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde] = target->p_seg.p_cr3_v[...];
  return <临时窗口 VA>;
}
```

`mem_clear_mapcache` 用完清空 freepdes 槽位（`memory.c:35-50`），防残留污染下次访问。

**BKL 下安全性**：`createpde` 修改全局页目录项，但 BKL 保证单 CPU 执行，无并发问题。

direct_map 下，`createpde` 退化为 `kernel_phys_to_virt(pa)` 一行加法——不需要借页目录、不需要临时窗口、不需要清理。完整实现见后续 24-cross-space-runtime.md。

### 2.6 IPCNAME 调试宏（阶段 D 中间夹）

`main.c:277-290` 定义了 IPC 调用类型编号→字符串的映射宏，仅调试用（`proc.c:497` 打印 IPC 统计时引用）：

```c
#define IPCNAME(n) { \
	assert((n) >= 0 && (n) <= IPCNO_HIGHEST); \
	assert(!ipc_call_names[n]);	\
	ipc_call_names[n] = #n; \
}
```

这与跨空间访问主题无关，仅因时序位置在阶段 D 中间被提及。Rust 替代：`enum IpcCall` + `impl Display`，无需全局数组。

---

## 3. Rust 设计决策

> 每节用"如果 X 设计，会有 Y 问题，所以用 Z"的假设性推理。

### 3.1 本质：direct_map 替代 freepdes（rewrite not translate）

**本质**：跨地址空间访问的解法随位宽演进，64 位下 direct_map 是自然解。

**约束驱动**：64 位虚拟地址空间充裕（256TB+），可以建立全物理内存的固定映射；`no_std` 下应避免全局可变状态（`freepdes[]` 是 `static int[]`，`FREE_PDE_SLOTS` 已迁移为 `SyncUnsafeCell`，非 `static mut`）。

**假设性推理**：如果翻译 Minix3 的 freepdes/ptproc，会泄漏 32 位临时窗口模型到 64 位 OS 层——污染页目录视图、需要清理、TLB 反复 flush、4MB 粒度限制全部继承，且 64 位地址空间本可避免这些。更糟的是，64 位页表是 4 级（PML4+PDPT+PD+PT），"借页目录"的语义从 PDE 变成 PML4E，临时窗口的粒度和复杂度都上升，而 direct_map 可以让这一切消失。

**决策**：废弃 freepdes/ptproc/`memory_init` 整套，用 direct_map 重新表达。这是 rewrite not translate——不是改 freepdes 的实现，是换整个机制。

### 3.2 双视图地址空间：VM direct map + Kernel direct map

**本质**：同一物理内存需要两个窗口，因为 x86-64 的 U/S 位不能同时 0 和 1。

| 窗口 | U/S 位 | 使用者 | 建立者 | 建立时机 |
|------|--------|--------|--------|---------|
| VM direct map | U/S=1 | VM 用户态 | kernel | 阶段 C（VM 启动前） |
| Kernel direct map | U/S=0, G=1 | 内核态 | VM（`map_kernel`） | VM 启动后 |

**假设性推理**：如果只有一个 direct map，要么 VM 访问不了（U/S=0，用户态访问触发 fault），要么内核安全降级（U/S=1，用户态能访问内核内存）。硬件特权级要求两个窗口——这不是设计冗余，是 ISA 规范的必然。

**关键不变量**：Kernel direct map 建立后只读不变。

**G=1（Global 位）的 TLB 行为细节**（TODO-07-5，2026-07-16 补充）：

PTE 的 Global 位（bit 8）告诉 CPU："这条 PTE 的翻译对所有进程地址空间有效，CR3 切换时不要 invalidate 对应的 TLB 条目"。这有 3 个前提：

1. **CR4.PGE 位必须启用**：x86-64 通过 CR4 第 7 位（PGE, Page Global Enable）开启 Global 位语义。Minix3 在 `pg_utils.c:243` 的 `vm_enable_paging()` 中设置 CR4.PGE（`cr4 |= I386_CR4_PGE`）。**minix-rs 当前未设置 CR4.PGE**——x86-64 的 CR4 初始化仅见于 `os/arch/src/x86_64/fpu.rs`（L88 `cr4 |= (1 << 9) | (1 << 10)`，即 OSFXSR/OSXMMEXCPT），G=1 位尚未启用，待 VM `map_kernel` 建立 Kernel direct map 时随 PTE 的 G 位一并落地。未启用 PGE 时设 G=1 = **未定义行为**（CPU 忽略 G 位但保留为未来兼容性）
2. **PTE 必须有 G=1 + 有效 P（Present）位**：纯 G=1 但 P=0 的 PTE 仍会被 invalidate（无效条目不缓存）
3. **TLB shootdown 影响**：即使 G=1，CPU 显式 `invlpg`（x86-64）/ `tlbi`（aarch64/riscv64）单条 invalidate 仍生效；G=1 只豁免**全局 CR3 切换**的 flush

**三架构等价语义**：
| 架构 | G 位等价 | 全局 TLB 不刷开关 | 单条 invalidate |
|------|---------|------------------|-----------------|
| x86-64 | PTE bit 8 | CR4.PGE | `invlpg va` |
| aarch64 | (无显式 G 位；MAIR 索引 + contiguous 提示) | TCR.EPD0/1（禁用整个 TTBR0/1 walk cache）| `tlbi vaae1is` |
| riscv64 | PTE bit 5 (G) | `sfence.vma` 配对 `sum` 位 | `hfence.vma va` / `sfence.vma va` |

aarch64 没有真正的 G 位，但通过 `TCR.EPD0=1`（disable TTBR0 walks）+ 在 TTBR1 放 Kernel direct map 实现等价效果；CPU 实现上仍 cache 全局翻译条目。

**为什么 G=1 对 direct_map 至关重要**：每次进程切换 `mov CR3, new_pdir`（x86-64）会 flush 整个非全局 TLB；若 Kernel direct map 的 PTE 没有 G=1，进程切换后内核第一次访问任何内核 VA 都触发 page walk → 2-3 级表查找 → 数十个 CPU 周期开销；G=1 让 Kernel direct map 的 TLB 条目**永远命中**，跨进程访问内核内存零开销。这是 Linux/BSD/Windows 通用做法，minix-rs 沿用。

### 3.3 对接 DirectMapArch trait（不自造 PostInitArch/MemoryInitArch）

**本质**：direct_map 的核心是地址空间布局（`va = pa + BASE`），跨架构仅 BASE 不同。

**约束驱动**：VM server 已实现 `DirectMapArch` trait（[VM 06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §3.6.3），定义在架构层（`os/arch/`），kernel 和 VM 都引用。kernel 侧应直接对接，避免重复抽象。

**假设性推理**：如果 kernel 自造 `PostInitArch`/`MemoryInitArch` trait 来表达 freepdes/ptproc 的等价物，会与 VM 层的 `DirectMapArch` 形成两套抽象，违反"硬件抽象唯一性"原则——同一个 direct_map 机制有两个 trait 定义，维护时容易漂移。

**决策**：废弃 `PostInitArch`/`MemoryInitArch`/`FreePdeSlots`/`VmPageTableInfo`，kernel 侧通过 `DirectMapArch::kernel_phys_to_virt(pa)` 直接访问。`DirectMapArch` 的定义位置在架构层（`os/arch/`），通过 `CurrentDirectMap` 类型别名编译期选择当前架构的实现，kernel 和 VM 共用同一套抽象。

### 3.4 map_kernel 职责演进：内核映射的建立者

**本质**：每个进程页表必须映射内核——中断/系统调用进入 ring 0 时能执行内核代码。

Minix3 的 `pt_mapkernel`（`minix3/minix/servers/vm/pagetable.c:1442`）职责：

| 映射内容 | Minix3 `pt_mapkernel` | minix-rs `map_kernel` |
|----------|----------------------|----------------------|
| 内核代码/数据段 | 必须 | 必须 |
| Kernel direct map（1GB huge pages, U/S=0, G=1） | 不存在 | 必须 |
| `page_directories` 登记册 | 必须 | 不需要 |
| `freepdes` 空闲槽位 | 必须预留 | 不需要 |

**假设性推理**：如果保留 `page_directories` 登记册（记录"哪些进程页目录映射了内核"），会维护一套元数据——direct_map 下每个进程都有 Kernel direct map（`map_kernel` 建立），登记册冗余。如果保留 `freepdes` 槽位预留，会继承 32 位临时窗口的全部问题（见 §3.1）。

**建立者时序**：

- VM direct map：由 kernel 建立（阶段 C，VM 启动前）——VM 还没运行，初始页表是 VM 进程创建的前提
- Kernel direct map：由 VM 的 `map_kernel` 建立（VM 启动后）——VM 运行后为每个进程（包括自己）的页表添加 Kernel direct map

注意：`map_kernel` 是 VM server 的函数（VM 进程运行时调用），不是 kernel 函数。这与 C 的 `pg_mapkernel`（kernel 函数，pre_init 阶段）不同——`pg_mapkernel` 只映射内核段，`map_kernel` 额外建立 Kernel direct map。

### 3.5 阶段 D 简化：确认 direct_map 就绪

**本质**：direct_map 在阶段 C（kernel 建立 VM 初始页表）已就绪，阶段 D 从"分配临时窗口"降级为"确认就绪"。

**约束驱动**：direct_map 是永久映射，不需要运行时分配/清理；但"确认就绪"是必要的安全检查——避免运行时才发现 direct map 缺失，那时定位困难。

**假设性推理**：如果完全删除阶段 D，kmain 流程少一步，但失去"direct map 就绪"的显式断言点。运行时 `createpde` 等价函数（`kernel_phys_to_virt`）失败时，难定位是 direct map 没建还是访问越界。保留一个确认步骤，让 fail-fast 发生在启动阶段而非运行时。

**决策**：阶段 D 保留为"确认 VM direct_map 就绪"的验证步骤，废弃 `arch_post_init` 的 ptproc/`pg_info` 和 `memory_init` 的 freepdes 分配。

**时序边界**：阶段 D 确认的是 VM direct map（阶段 C 已建立）。Kernel direct map 由 VM 启动后的 `map_kernel` 建立，不在阶段 D 范围——阶段 D 时 VM 还没运行。

### 3.6 废弃清单与假设性推理汇总

| 废弃项 | C 对应 | 如果保留会怎样 | direct_map 替代 |
|--------|--------|--------------|----------------|
| ptproc per-CPU 变量（createpde 用途） | `cpulocals.h:55` | 泄漏 32 位"借页目录"模型，64 位下无意义 | kernel 有 Kernel direct map，不借页目录 |
| freepdes[] 数组 | `memory.c:32` | 全局可变状态 + 临时窗口全部问题 | direct map 永久映射 |
| memory_init() | `memory.c:707` | 运行时分配槽位的逻辑冗余 | 无需分配 |
| PostInitArch trait | 无 C 对应 | 与 DirectMapArch 形成两套抽象 | 对接 DirectMapArch |
| MemoryInitArch trait | 无 C 对应 | 同上 | 对接 DirectMapArch（**触发条件**：TODO-07-1 修复 lib.rs:1017-1098 之后自然废弃，详见 §1.4 实现状态块）|
| FreePdeSlots 结构体 | `freepdes[]` | 表达临时窗口槽位，direct map 不需要 | 无（同上触发条件）|
| VmPageTableInfo | `pg_info` 输出 | 记录 bootstrap 页表地址，direct map 不依赖 | kernel 用 `kernel_phys_to_virt` 直接访问 |
| FREE_PDE_SLOTS static | `freepdes[]` | 全局可变状态 | 无 |
| FREE_UPPER_IDX static | `kinfo.freepde_start` | 全局原子，临时窗口索引 | 无 |

> 废弃不是删除代码，是换机制——每个废弃项都有 direct_map 的替代或确认不需要。
>
> **ptproc 跟踪不全部废弃**（P9-4 澄清）：上表"ptproc per-CPU 变量"废弃的是其 **createpde 临时窗口用途**（借页目录放临时映射）。ptproc 的第二个用途——`setcr3()` 中 `if (p == ptproc)` 决定是否立即 reload CR3（arch_do_vmctl.c:31）——在 direct_map 下**保留**，因为 VM 仍通过 `VMCTL_SETADDRSPACE` 切换页表。Rust 用 kernel 全局 `CURRENT_PTPROC_NR: AtomicI32`（[lib.rs:2036](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）+ `set_current_ptproc_nr(VM_PROC_NR)` 跟踪此用途，与 arch 层 `PostInitArch::set_ptproc`（记录 createpde 用的 `virt_root`，应废弃）分离。详见 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md) 与 §1.4 实现状态块。

> **内核全局状态存储模式**（废弃项之上的模式层问题）：上表废弃的 `FREE_PDE_SLOTS`/`FREE_UPPER_IDX` 采用"分散全局"存储——每个全局一个 `SyncUnsafeCell`/`AtomicUsize`，与 C 源码的全局变量一一对应。不聚合为 `KernelState` 结构体的理由（机制废弃后模式仍适用）：① **可审计性**——分散 static 与 C 全局一一映射，review 可逐一核对；② **渐进式 init**——每个子系统独立初始化；聚合结构必须等所有子系统 init 后才能构造，编译期依赖复杂；③ **无跨子系统联动需求**——各子系统全局（`PROC_TABLE`/`PRIV_TABLE` 等，见 [06-proc-init-boot-proc.md §4.3](06-proc-init-boot-proc.md)）以参数形式传递，无需单一访问点。此模式是 kernel 全局的通用选择，废弃清单只删条目、不推翻模式本身；若后续出现需跨子系统原子联动的全局，再评估聚合。

---

## 4. 实现详解

> 对应 Ch3 每个决策，讲具体接口/流程。本章较瘦，因为大量实现被废弃。

### 4.1 DirectMapArch 接口（引用 VM 已实现）

`DirectMapArch` trait 定义在架构层（`os/arch/`），VM server 和 kernel 共用（[VM 06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §3.6.3）：

```rust
pub trait DirectMapArch {
    /// VM direct map 基地址（U/S=1，VM 用户态用）
    const VM_DIRECT_MAP_BASE: u64;
    /// Kernel direct map 基地址（U/S=0，内核态用）
    const KERNEL_DIRECT_MAP_BASE: u64;
    /// VM HeapArena 区域基地址（紧随 VM direct map 窗口之后）
    const VM_HEAP_BASE: u64;
    /// VM HeapArena 区域大小（字节）
    const VM_HEAP_SIZE: u64;

    /// 物理地址 → VM 用户态虚拟地址
    fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes {
        VirBytes(phys.get() + Self::VM_DIRECT_MAP_BASE)
    }
    /// 物理地址 → 内核态虚拟地址
    fn kernel_phys_to_virt(phys: PhysBytes) -> VirBytes {
        VirBytes(phys.get() + Self::KERNEL_DIRECT_MAP_BASE)
    }
    /// 虚拟地址 → 物理地址（反向转换；Kernel 高半区优先判定）
    fn virt_to_phys(virt: VirBytes) -> PhysBytes {
        if virt.get() >= Self::KERNEL_DIRECT_MAP_BASE {
            PhysBytes::new(virt.get() - Self::KERNEL_DIRECT_MAP_BASE)
        } else {
            PhysBytes::new(virt.get() - Self::VM_DIRECT_MAP_BASE)
        }
    }
}
```

三架构 BASE 值（详见 [VM 06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §3.6.3）：

| 架构 | VM_DIRECT_MAP_BASE | KERNEL_DIRECT_MAP_BASE | 说明 |
|------|-------------------|----------------------|------|
| x86-64 | `0x0000_0000_8000_0000` | `0xFFFF_8000_0000_0000` | VM 在 2GB 用户态低区，Kernel 在高地址区 |
| aarch64 | `0x0000_1000_0000_0000` | `0xFFFF_8000_0000_0000` | ARM 虚拟地址空间布局 |
| riscv64 (Sv39) | `0x0000_0010_0000_0000` | `0xFFFF_FC00_0000_0000` | Sv39 地址划分 |

kernel 侧通过 `CurrentDirectMap` 类型别名编译期选择当前架构的实现：

```rust
// os/arch/src/lib.rs
#[cfg(target_arch = "x86_64")]
pub type CurrentDirectMap = X86_64DirectMap;
#[cfg(target_arch = "aarch64")]
pub type CurrentDirectMap = AArch64DirectMap;
#[cfg(target_arch = "riscv64")]
pub type CurrentDirectMap = Riscv64DirectMap;
```

三架构的 `DirectMapArch` 实现（`X86_64DirectMap` / `AArch64DirectMap` / `Riscv64DirectMap`）均定义在 `os/arch/src/arch/direct_map.rs`，提供 `vm_phys_to_virt` / `kernel_phys_to_virt` / `virt_to_phys` 方法。arm64/riscv64 的 runtime paging 方法（`map` / `unmap` / `query` / `remap` / `update_flags` / `new` / `destroy`）通过 `kernel_phys_to_virt` 经 Direct Map 读写物理页表项，与 x86_64 实现模式一致。

1GB huge page 的 CPU 支持检查（`supports_1gb_page()`）：x86-64 查 `CPUID.80000001H:EDX.GBPAGES`，不支持时回退 2MB。详见 [VM 06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §3.6.3。

### 4.2 阶段 D 入口：确认 direct_map 就绪

> **目标实现**（Action Item #1）：当前 `init_post_and_memory` 仍为 C 翻译版（§1.4 实现状态块），以下为重构后的目标形态。

阶段 D 入口 `init_post_and_memory` 重写为确认步骤：

```rust
/// 阶段 D：确认 VM direct_map 已就绪。
///
/// VM direct map 由 kernel 在阶段 C 建立 VM 进程时建好
/// （详见 VM 06-pagetable-struct.md §3.6 的 4 页初始页表结构）。
/// 本函数只做确认，不分配任何东西。
///
/// 废弃的 C 逻辑：
/// - arch_post_init 的 ptproc=VM + pg_info（direct_map 不借页目录）
/// - memory_init 的 freepdes 分配（direct_map 是永久映射）
fn init_post_and_memory(vm_proc: &Proc) {
    // 1. 确认 VM 进程页表 root 有效
    assert!(vm_proc.page_table_root.is_valid(),
            "VM page table root must be valid after stage C");

    // 2. 确认 direct map base 已由 boot-shim 填充
    assert!(CurrentDirectMap::VM_DIRECT_MAP_BASE != 0,
            "VM direct map base must be configured");

    // 3. 确认 VM direct map 在 VM 页表中已建立
    //    （阶段 C 建立，此处只验证）
    assert!(verify_vm_direct_map_present(vm_proc),
            "VM direct map must be present in VM page table");

    // 不分配 freepdes、不设 ptproc、不记录 VmPageTableInfo
}
```

**流程**：获取 VM 进程 → 确认页表 root 有效 → 确认 direct map base 已配置 → 确认 VM direct map 已存在 → 返回。

**时序边界**：阶段 D 确认的是 VM direct map（阶段 C 建立）。Kernel direct map 由 VM 启动后的 `map_kernel` 建立，此处不验证——VM 还没运行。

### 4.3 废弃的 trait/结构体清单（迁移说明）

废弃项及其替代：

| 废弃项 | 原位置 | 替代 |
|--------|--------|------|
| `PostInitArch` trait | `os/arch/src/arch/post_init.rs` | 无（direct_map 不需要 arch 级 `set_ptproc`——记录 `virt_root` 供 createpde 用，已被 Direct Map 取代） |
| `MemoryInitArch` trait | `os/arch/src/arch/post_init.rs` | 无（不分配 freepdes） |
| `FreePdeSlots` 结构体 | `os/arch/src/arch/post_init.rs` | 无 |
| `VmPageTableInfo` 结构体 | `os/arch/src/arch/post_init.rs` | 无（kernel 用 `DirectMapArch` 直接访问） |
| `FREE_PDE_SLOTS` static | `os/kernel/src/lib.rs` | 无 |
| `FREE_UPPER_IDX` static | `os/kernel/src/lib.rs` | 无 |

> **不废弃**：kernel 级 `CURRENT_PTPROC_NR` + `set_current_ptproc_nr`（[lib.rs:2036/2075](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)，P9-4 新增）。这是 `setcr3()` 中 `if (p == ptproc)` CR3-reload 决策的 Rust 对应，与 arch 级 `set_ptproc`（createpde 用途）分离。详见 §3.6 废弃清单脚注与 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md)。

**迁移影响**：后续 24-cross-space-runtime.md 的 `createpde` 等价函数改用 `CurrentDirectMap::kernel_phys_to_virt(pa)`，不再读 freepdes 槽位。

**代码清理范围**：

- `os/arch/src/arch/post_init.rs`：删除 `PostInitArch`/`MemoryInitArch`/`FreePdeSlots`/`VmPageTableInfo`
- `os/arch/src/{x86_64,arm64,riscv64}/post_init.rs`：删除对应 impl
- `os/kernel/src/lib.rs`：删除 `FREE_PDE_SLOTS`/`FREE_UPPER_IDX`

---

## 5. 测试要点

> 覆盖 Ch3+Ch4 的每个核心决策。

### 5.1 direct_map 就绪验证

- **测试**：阶段 D 确认步骤能正确识别 direct map 已就绪（VM 进程页表 root 有效 + direct map base 已配置 + VM direct map 已存在）
- **测试**：direct map 未就绪时确认步骤 panic（fail-fast，启动阶段暴露问题而非运行时）
- **测试**：`CurrentDirectMap::kernel_phys_to_virt(pa)` 返回正确 VA（对接 `DirectMapArch` 的 mock 实现）

### 5.2 废弃路径不存在的断言

> **状态标注**（2026-08-14）：以下为 **Action Item #1 落地后**的目标测试——当前 `FreePdeSlots`/`PostInitArch`/`MemoryInitArch`/`VmPageTableInfo` 仍存在于 `os/arch/src/arch/post_init.rs:35-206`（§1.4 实现状态块：`init_post_and_memory` 仍翻译 C 旧逻辑）。Action Item #1 重构后这些类型删除，编译期保证本节断言成立。

- **测试**：freepdes 相关代码已删除（编译期保证：`FreePdeSlots`/`PostInitArch`/`MemoryInitArch` 不存在）
- **测试**：ptproc 相关代码已删除
- **测试**：`VmPageTableInfo` 不存在

废弃的验证是"不存在"而非"存在"——编译期类型系统保证。

### 5.3 DirectMapArch 对接测试

- **测试**：kernel 侧通过 `CurrentDirectMap` 正确调用 `vm_phys_to_virt`/`kernel_phys_to_virt`
- **测试**：三架构 BASE 常量正确（x86-64/arm64/riscv64）
- **测试**：1GB huge page 不支持时回退 2MB

direct_map 的测试在 VM 层已覆盖（[VM 06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §5），kernel 侧仅测对接。

### 5.4 syscall_copy stub 与 07 文档的边界（TODO-07-2 范围澄清）

07 文档承诺的"direct_map 替代临时窗口"是**设计层面的承诺**——direct_map 在概念上取代了 freepdes/ptproc 机制。但**使用 direct_map 的具体代码路径**（syscall_copy.rs 的 `cross_space_copy`/`virtual_copy_vmcheck` 等）由后续文档（[18-syscall-copy.md](18-syscall-copy.md)）负责实现。

**当前状态**（2026-07-16，TODO-07-2 验证）：
- `os/kernel/src/syscall_copy.rs` 有 6 处 stub（不是 kimi 报告的 5 处）：
  | # | 函数 | 阻塞原因 |
  |---|------|---------|
  | 1 | `safecopy_common_impl` | Direct Map PTE walk blocker |
  | 2 | `dispatch_vsafecopy` | Direct Map PTE walk blocker |
  | 3 | `dispatch_umap_remote_impl` | Direct Map PTE walk blocker |
  | 4 | `dispatch_vumap` | Direct Map PTE walk blocker |
  | 5 | `dispatch_memset` | Direct Map PTE walk blocker |
  | 6 | `dispatch_safememset` | Direct Map PTE walk blocker |
- 这些 stub **不是** 07 文档的实施缺口——07 只承诺 direct_map 抽象，stub 实现见 18 文档。
- 07 文档读者应知道：direct_map 抽象已就绪（vm.rs:348-349 / :393 已在 cross_space_copy / cross_space_memset 中调用 `DirectMapArch::kernel_phys_to_virt`），但 syscall_copy 的 stub 路径仍待 18 文档补充。

**回归报告修正**：kimi/seed 报告的 5 处（行 252/374/397/416/439）**全部错误**——这些行指向已实现的 `virtual_copy_vmcheck`。07 文档不引用 syscall_copy.rs 的具体行号，避免行号漂移传播（模式 66 RCPD）。

---

## 附录 A. 阶段 D 时序图（direct_map 版）

```
阶段 C: kernel 建立 VM 初始页表（含 VM direct map, 4页结构）
        详见 VM 06-pagetable-struct.md §3.6
  ↓
阶段 D: 确认 VM direct_map 就绪（init_post_and_memory 简化版）
        - 确认 VM 进程页表 root 有效
        - 确认 direct map base 已配置
        - 确认 VM direct map 已存在
        - 不分配 freepdes、不设 ptproc
  ↓
阶段 E-F: system_init + bsp_finish_booting（见 08）
  ↓
VM 启动后: map_kernel 建立 Kernel direct map（U/S=0, G=1）
           详见 VM 07-pagetable-ops.md §3.0.7
```

## 附录 B. Minix3 vs minix-rs 阶段 D 对照

| 方面 | Minix3 C (32位) | minix-rs (64位 direct_map) |
|------|----------------|---------------------------|
| 跨空间访问机制 | freepdes 临时窗口 | direct_map 永久映射 |
| ptproc | per-CPU 变量，记录当前页目录 | 废弃 |
| memory_init | 分配 2 个 freepdes | 废弃 |
| pg_info | 记录 bootstrap 页表地址 | 废弃（direct_map 不依赖） |
| 阶段 D 内容 | 设 ptproc + 分配 freepdes | 确认 VM direct_map 就绪 |
| 内核映射建立 | pg_mapkernel (4MB 大页, pre_init) | map_kernel (1GB huge page + Kernel direct map, VM 启动后) |
| 建立者 | kernel (pre_init) | kernel (VM direct map, 阶段 C) + VM (Kernel direct map, 启动后) |
| 全局可变状态 | freepdes[]/nfreepdes/kinfo.freepde_start | 无 |

---

## 6. 参见

- [06-proc-init-boot-proc-new.md](06-proc-init-boot-proc-new.md) — 阶段 C：进程表初始化与 VM ELF 加载（含 bootstrap 页表）
- [08-system-init-boot-finish.md](08-system-init-boot-finish.md) — 阶段 E-F：系统调用注册与启动完成
- [24-cross-space-runtime.md](24-cross-space-runtime.md) — 运行时跨空间访问（`createpde` 等价函数 = `kernel_phys_to_virt`）
- [02-stage-vm/06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §3.6 — direct_map 完整设计（双视图、4页结构、DirectMapArch trait）
- [02-stage-vm/07-pagetable-ops.md](../02-stage-vm/07-pagetable-ops.md) §3.0.7 — `map_kernel` 职责简化
- `minix3/minix/kernel/arch/i386/protect.c:370-377` — `arch_post_init`
- `minix3/minix/kernel/arch/i386/pg_utils.c:186-206` — `pg_mapkernel`
- `minix3/minix/kernel/arch/i386/pg_utils.c:312-316` — `pg_info`
- `minix3/minix/kernel/arch/i386/memory.c:707-717` — `memory_init`
- `minix3/minix/kernel/arch/i386/memory.c:35-145` — `mem_clear_mapcache` + `createpde`
- `minix3/minix/kernel/cpulocals.h:55` — `ptproc` per-CPU 变量
- `minix3/minix/kernel/arch/i386/pre_init.c:232` — `kinfo.freepde_start = pg_mapkernel()`
