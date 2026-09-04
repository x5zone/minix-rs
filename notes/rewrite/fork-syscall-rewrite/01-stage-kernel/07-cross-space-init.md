# 07-cross-space-init: 跨地址空间初始化——从临时窗口到 direct_map

> **分类**: 全局基建（内核启动阶段 D）
> **源码**: `minix3/minix/kernel/arch/i386/protect.c` · `minix3/minix/kernel/arch/i386/pg_utils.c` · `minix3/minix/kernel/arch/i386/memory.c`
> **说明**: 内核如何获得"看"别的进程地址空间的能力——Minix3 用 32 位临时窗口（freepdes/ptproc），minix-rs 用 64 位 direct_map 重新表达。
> **Redesign 依据**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md` §3.2（direct_map 设计）、`02-stage-vm/08-pagetable-ops.md` §3.4（map_kernel 职责简化）

---

## 1. 概述

### 1.0 本章讲什么

kmain 的启动流程分为六个阶段：

| 阶段 | 名称 | 核心动作 | 文档 |
|------|------|---------|------|
| A | 入口 | 固件交接、栈建立、高半转换（含 DM 双窗口建立，§1.3.2） | 01-04 |
| B | cstart | 早期硬件初始化、解析启动信息 | 05 |
| C | 进程表初始化 | 清空进程表、加载 VM ELF | 06 |
| **D** | **跨空间初始化** | **内核获得跨地址空间访问能力** | **07（本文）** |
| E | 系统初始化 | 系统调用注册、子系统启动 | 08 |
| F | 启动完成 | 调度开始、bsp_finish_booting | 08 |

阶段 D 要回答一个核心矛盾：**Minix3 内核没有自己独立的页表——它运行在"被借用"的进程页表里（boot 期借 VM 的 bootstrap 页表，运行期借当前进程的页表），却要访问"别的"进程的内存**。当前 CR3 装的页表只映射了"当前进程"的用户态 VA，**目标进程**的用户态 VA 在当前页表里没有映射——CPU 无法解释这个 VA。IPC 消息拷贝（`lin_lin_copy`）、`vm_memset` 等内核服务都需要穿越进程隔离。这就是跨地址空间访问问题。

> **重要澄清**：Minix3 不是"内核态 vs 用户态"二元地址空间模型。每个进程的页表都映射了内核段（高地址区），所以 CPU 在任何进程的上下文里都能直接跑内核代码。真正的"跨空间"问题是**"借来的页表"vs"目标进程的页表"**——内核当前跑在 A 进程页表里（= 只看得见 A 的用户态），要访问 B 进程的用户态内存时，需要让 CPU 暂时能解释 B 的 VA。

这个问题的解法随虚拟地址空间位宽演进：

- **32 位时代**：虚拟地址空间只有 4GB，内核和用户共享，没有余量建立全物理内存的固定映射。解法是**临时窗口**——在当前页目录里预留 2 个槽位（freepdes），需要访问目标进程内存时，把目标页目录项（PDE）临时写入槽位，用完清掉。这是 Minix3 的方案。
- **64 位时代**：虚拟地址空间 256TB+，有余量建立"全物理内存 → 固定虚拟地址区间"的线性映射（`va = pa + BASE`）。内核要访问任意物理内存时，直接算出 VA，MMU 按 direct map 的 PTE 解释——不需要临时窗口、不污染任何页目录、不需要清理。这是 Linux/Windows/BSD 主流内核的通用模式，也是 minix-rs 的选择。

**本章立场**：minix-rs 选择 direct_map，废弃 32 位的临时窗口机制。阶段 D 从"分配临时窗口"简化为"确认 direct_map 就绪"。

**目标读者**：已理解 Minix3 微内核基本结构、了解 x86 分页机制、读过 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md)（阶段 C）的开发者。

**本章不讲什么**：

- direct_map 的 VA 布局常量与 `DirectMapArch` trait 设计（VM server [07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.2；本文 §1.3.2 讲建立链路、§4.4 讲建立实现）
- C 端 `createpde()` 临时 PDE 映射机制（Rust 端 0 命中，仅作 C 历史参照系出现在 24-cross-space-runtime.md §1.1 / §3 D1 / §4.1；本文不引，避免与 24 重复）
- `map_kernel()` 的完整实现（VM server [08-pagetable-ops.md](../02-stage-vm/08-pagetable-ops.md) §3.4，本文仅讲协作关系）

### 1.1 核心矛盾：内核如何"看"别人的地址空间

每个进程有独立页表——CR3 装的是当前进程的页表根，MMU 按 CR3 指向的页表解释虚拟地址。这是进程隔离的基础，也是 OS 核心机制。

内核映射在每个进程的地址空间里。中断或系统调用进入 ring 0 时，CPU 必须能立即执行内核代码，所以每个进程的页表都映射了内核段（高地址区）。进程切换时，CR3 换成新进程的页表，但内核段映射始终可见——这是为什么内核能在任何进程的上下文里运行。

但内核要访问**别的**进程的用户态内存时，问题出现了：目标内存的 VA 在当前页表里没有映射（它映射在目标进程的页表里）。CPU 当前的页表解释不了这个 VA。

> **灵魂本质**：跨地址空间访问 = "CPU 当前页表解释不了目标 VA，如何临时让它解释得了"。

这个能力是内核运行时服务的基石：IPC 消息要在进程间拷贝、`fork` 要复制父进程地址空间、`exec` 要加载新镜像——都依赖内核能读写目标进程的内存。

下一节给出两种回答：32 位的"临时 PDE 窗口"与 64 位的"direct map 绕开"。

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

**跨进程访问的实现路径**——给"目标进程的某个 VA"取内核可读 KV：

```
目标进程VA ──┐
            ├─► [进程表查 CR3] ──► 目标 CR3（物理地址）
            ├─► [PTE walk，从 CR3 出发] ──► 目标 PA
            └─► [kernel_phys_to_virt(PA)] ──► 内核可访问 KV
                                      ▼
                                   [memcpy KV→KV 完成]
```

四步皆走内核特权：CR3 来自进程表（不读用户态）、PTE walk 用目标页表副本（不在当前 CPU 页表里改任何条目）、KV 由 direct map 的恒等映射给出（与目标进程页表无关）、最后 memcpy。**CPU 当前页表从未被改写**——进程隔离全程不被破坏。

> **注意**：跨页拷贝（VA 跨越 PTE 边界时 PTE walk 需循环）、页内偏移保留（`va = pa + BASE` 仅对页对齐时成立）、TOCTOU（解析与拷贝间页可能被换出）这三个细节由 [24-cross-space-runtime.md](24-cross-space-runtime.md) §3 D1 详述，本节聚焦"路径成立"，不展开。

> **灵魂本质**：direct_map = 给所有物理内存一个永久的虚拟地址，临时窗口的"借/还"整个消失。

**双视图地址空间**（硬件特权级要求）：同一物理内存需要两个窗口——

| 窗口 | U/S 位 | 使用者 | 建立者 | 建立时机 |
|------|--------|--------|--------|---------|
| VM direct map | U/S=1 | VM 用户态 | kernel（`establish_boot_dm`） | 内核 boot 期（arch_boot Step 4，开分页后、kmain 前） |
| Kernel direct map | U/S=0, G=1 | 内核态 | kernel（`establish_boot_dm`，同一调用） | 内核 boot 期（同上，装在同一个 bootstrap root 上） |

两个窗口装在**同一个 bootstrap root** 上：此刻该 root 正是内核运行的页表；VM 启动时经 A1 adoption 整体收编它作为初始地址空间，双窗口随收编被 VM 继承（建立过程见 §1.3.2，实现细节见 §4.4）。

为什么需要两个窗口？x86-64 的 U/S 位不能同时 0 和 1。如果只有一个 direct map，要么 VM 用户态访问不了（U/S=0），要么内核安全降级（U/S=1）。两个窗口是硬件特权级的必然要求，不是冗余。

#### 1.3.1 双视图的硬件根源（ISA U/S 位语义）

x86-64 的页表项（PTE）有一个二元 U/S 位（User/Supervisor bit），定义如下：
- **U/S=1**：PTE 被 ring 3（用户态）访问时，TLB/MMU 允许
- **U/S=0**：PTE 仅被 ring 0（内核态）访问时，TLB/MMU 允许；用户态访问触发 #PF（page fault）

**问题**：同一物理地址需要同时被 VM 用户态访问（VM 业务逻辑要读写自己的页表）和内核态访问（kernel 的 IPC 拷贝、vm_memset 等）。一个 PTE 只能设一个 U/S 位——若全部设 1，内核能访问但失去特权隔离（VM 用户态可读写内核 PTE）；若全部设 0，VM 用户态无法访问自己的页表（VM 业务依赖用户态 VA）。

**解法**：同一物理内存**建立两个 PTE**，分别在两个虚拟地址区间：
- **VM direct map**（U/S=1）：VM 进程页表里加一段 [VM_DIRECT_MAP_BASE, VM_DIRECT_MAP_BASE + physmem_size) 的 PTE 数组，U/S=1 让 VM 用户态访问
- **Kernel direct map**（U/S=0, G=1）：所有进程页表里加一段 [KERNEL_DIRECT_MAP_BASE, + physmem_size) 的 PTE 数组，U/S=0 隔离用户态，G=1 让 CR3 切换不刷 TLB

**架构差异**：
- **x86-64**：U/S 是 PTE 第 2 位，由 CPU 在 TLB lookup 时强制检查
- **aarch64**：等价于 PTE 的 AP[2:1] 位（Access Permissions）；UXN/PXN 控制执行权限
- **riscv64**：等价于 PTE 的 X/W/R 位与 SUM 位（S-mode 能访问 U-mode 内存的开关）

三架构的 ISA 机制名不同，但**都要求"双视图"**——同一页物理内存必须在两个虚拟地址区间登记两次 PTE（一次 U-allowed，一次 S-only），CPU/MMU 各自按 ISA 检查位拒绝越权。直接 map 的 BASE 值三架构不同（见 §4.1），但双视图的**必然性是 ISA 规范**，不是 minix-rs 的设计选择。

> **灵魂本质**：双视图 = ISA 强制（U/S 不能同时 0 和 1）+ 性能优化（G=1 跨 CR3 切换 TLB 不刷新）。

Minix3 全线没有 direct map（`pmap.h` 的 `PMAP_DIRECT_MAP` 宏受 `#ifdef __HAVE_DIRECT_MAP` 保护且从未定义，见 [VM 07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.2）。direct_map 是 minix-rs 全新引入的架构演进。

#### 1.3.2 direct_map 的建立：kernel boot 期一次建立双窗口

direct_map 不是运行时逐步长出来的，而是**内核 boot 期一次建立**。建立点在 `arch_boot`（高半转换路径，[02-higher-half-kernel.md](02-higher-half-kernel.md) §4.4）的 Step 4：恒等映射（Step 1）、内核高地址映射（Step 2）、开分页（Step 3）之后、kmain 之前。此时内核调用 `establish_boot_dm`（`os/kernel/src/dm_coverage.rs`），在**同一个 bootstrap root** 上依次装好两个 DM 窗口：

1. **Kernel DM 窗口先行**（supervisor RW，PA 全域）——identity 映射被 DM 语义取代后，内核对物理内存的访问通道从这里走（§4.4 解释为什么必须先行）
2. **VM DM 窗口随后**（user RW，PA 裁剪到窗口容量）——VM 用户态访问自己页表树与被管理内存的通道

为什么一次建立就能同时服务内核与 VM：bootstrap root 此刻就是内核正在用的页表；VM 启动时通过 **A1 adoption**（[VM 07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.4）整体收编这棵页表树作为自己的初始地址空间——双窗口随收编被 VM 继承，无需第二次建立。

建立要回答"映射哪些物理内存"：候选 = 常规内存（memmap conventional，VM PMM 的资源来源）∪ bootstrap 页表树（root 页 + boot bump 区——二者是固件 `LOADER_DATA` 分配，不在 conventional memmap 内，须显式登记）；再经资源包含性、窗口边界、页粒度包含性过滤。x86-64 上 VM DM 窗口与 identity 映射在 VA [2GiB, 3GiB) 重叠，建立时按资源包含性决定整页替换还是下级拆分（§4.4）。

kernel 之后为**受管进程**建根时（VM 的 `init_page_table`），经 `map_kernel` 把内核段 + supervisor 权限的 Kernel DM 窗口补进每个进程根——那是"每个进程都要看得见内核"的常规装配（§3.4），不是 DM 窗口的第一次建立。

> **灵魂本质**：双窗口 = boot 期一次成型（先内核后 VM，同一 root），A1 adoption 让 VM 白拿整套窗口；`map_kernel` 只做受管进程根的例行补齐。

### 1.4 阶段 D 的位置与简化

阶段 D 在 C 里是两行代码：`arch_post_init`（设 ptproc + `pg_info`）+ `memory_init`（分配 freepdes）。

direct_map 下这两行的语义变化：

| C 代码 | 32 位语义 | direct_map 下 |
|--------|----------|--------------|
| `ptproc = VM` | 记录"当前页目录是 VM 的"，freepdes 借此页目录 | 废弃——kernel 有 Kernel direct map，不借页目录 |
| `pg_info()` | 记录 bootstrap 页表的物理/虚拟地址 | 废弃——kernel 用 `kernel_phys_to_virt` 直接访问 |
| `memory_init` 分配 freepdes | 领取 2 个临时窗口槽位 | 废弃——direct map 是永久映射 |

阶段 D 简化为：**确认 direct_map 已就绪**。VM direct map 在内核 boot 期（arch_boot Step 4，早于阶段 C）随双窗口一起建立（§1.3.2）；阶段 C 经 A1 adoption 把含双窗口的 bootstrap root 移交给 VM；阶段 D 只需确认它已就绪，无需分配任何东西。

> **灵魂本质**：阶段 D 从"分配临时窗口"降级为"确认永久窗口已开"。

注意：Kernel direct map 同样在 boot 期随双窗口建立（`establish_boot_dm`，§1.3.2），早于阶段 D。VM 启动后的 `map_kernel` 只为**受管进程**根补齐 supervisor 权限的 Kernel DM（见 §3.4），不承担"第一次建立"。

> **阶段 D 的 Rust 对应**：`init_post_and_memory`（os/kernel/src/lib.rs:1271）由三部分组成——①断言 VM 页表 root 有效（`p_seg.phys_root != 0` + `p_seg.virt_root.is_some()`，阶段 C 建立）；②记录 VM 为 kernel 级 ptproc（`set_current_ptproc_nr`，使 `dispatch_vmctl(SetAddrSpace)` 的 Step 3 能决定是否 reload CR3，详见 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md)）；③断言 VM direct map base 已配置。arch 级 `set_ptproc`（记录 `virt_root` 供 createpde 借页目录）与 freepdes 分配在现代码中没有对应物——这些类型的取舍理由与幸存契约见 §4.3。
>
> **两层 ptproc 跟踪的区分**：arch 层 `PostInitArch::set_ptproc` 记录 arch 内部状态（`virt_root` 供 createpde 借页目录用）——createpde 被 Direct Map 取代后该层不再需要；kernel 层 `set_current_ptproc_nr`（os/kernel/src/lib.rs:2212）记录 VM 的 proc-nr，使 `SetAddrSpace` 的 Step 3（`if current_ptproc_nr() == Some(target.p_nr)`）能判断是否需立即 reload CR3。后者是 `setcr3()` 语义的直接对应（C: `if (p == get_cpulocal_var(ptproc))`，arch_do_vmctl.c:25），**不是** createpde 临时窗口机制的一部分，因此 direct_map 下仍需保留。

### 1.5 本章小结

- 跨地址空间访问是内核核心能力，解法随位宽演进
- 32 位临时窗口（freepdes/ptproc）是地址空间受限的妥协；64 位 direct_map 是地址空间充裕的自然解
- 双窗口在内核 boot 期一次建立（`establish_boot_dm`，arch_boot Step 4）：Kernel DM 先行（supervisor，PA 全域）、VM DM 随后（user，PA 裁剪到窗口），VM 经 A1 adoption 继承（§1.3.2、§4.4）
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
2. **SetAddrSpace 的 CR3-reload 决策**：`setcr3()` 用 `if (p == get_cpulocal_var(ptproc))` 判断目标进程是否是当前页表进程，若是则立即 `write_cr3`（arch_do_vmctl.c:25）。direct_map 下此用途**保留**——VM 仍通过 `VMCTL_SETADDRSPACE` 切换到自己的页表，内核需知道是否该立即 reload CR3。Rust 用 kernel 全局 `CURRENT_PTPROC_NR: AtomicI32` + `set_current_ptproc_nr(VM_PROC_NR)` 跟踪（详见 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md)）。

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
static int freepdes[MAXFREEPDES];   /* memory.c:31 */
static int nfreepdes;               /* memory.c:29 */

void memory_init(void)
{
  freepdes[nfreepdes++] = kinfo.freepde_start++;  /* 领取槽位 0 */
  freepdes[nfreepdes++] = kinfo.freepde_start++;  /* 领取槽位 1 */
  assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES);  /* 越界检查 */
}
```

**为什么是 2 个**：`createpde` 的直接调用者 `lin_lin_copy`（`memory.c:189-190`，源/目标各占一槽）和 `vm_memset`（`memory.c:556`）需要同时开两个窗口——一次跨空间拷贝的源和目标可能分属两个不同进程。

`freepdes[]` 是内核全局静态数组（`memory.c:29-31`），不属于任何进程。存的**不是页数据**，是 2 个 PDE 槽位**编号**。编号从哪来：boot 阶段 `pg_mapkernel()` 把内核镜像按 4MB 大页映射进 boot 页目录，返回**镜像占用的最后一个 PDE 的下一个槽位**（记为 N，`pre_init.c:232`、`pg_utils.c:185-201`），存进 `kinfo.freepde_start`——该字段含义是"第一个未被占用的内核区 PDE"（`include/minix/param.h:27`）。`memory_init` 从这里连领 2 个：N、N+1。

**VM 为什么领不到这两个编号**：三方时序——boot 时 `kinfo.freepde_start = N`；内核初始化执行 `memory_init`（`kernel/main.c:293`），计数器前移到 N+2，此时 VM 尚未运行；VM 启动后经 `sys_getkinfo` 取到 kinfo 副本（`servers/vm/main.c:442`），读到的起点已是 N+2，此后它需要的槽位都从 `freepde()` 取号（`servers/vm/pagetable.c:1028-1033`）。窗口槽 N、N+1 在 VM 的发号线上永远成为过去——两个编号被内核先消费，VM 从剩余部分继续。

**一套编号通用于所有页目录**：每个进程的页目录都由 VM 装配（`pt_mapkernel`，`servers/vm/pagetable.c:1442-1482`）。VM 虽是用户态进程，但它装配的是**别人的页目录**——它通过把目标物理页映射进自己的用户区来编辑页目录（ring 3 从不直接写高地址），挑的 PDE 编号却落在内核区。两个窗口槽之外的内核区槽位也由 VM 取号填充，各有用途：

- **页目录的目录**（`pagetable.c:1035-1067`）：VM 领 5 个编号（`MAX_PAGEDIR_PDES=5`），每号配一个物理页，登记所有进程页目录的物理地址（`pt_bind` 写入，`pagetable.c:1394`）；`pt_mapkernel` 再把它写进每个页目录的对应项（`pagetable.c:1475-1482`）——这个簿记页在所有地址空间的相同 VA 可见
- **kernmap_pde**（`pagetable.c:1181`）：内核永久映射（如 video memory）经 `sys_vmctl_get_mapping` 报给 VM，VM 把它们放进每个页目录的同一编号处

这些槽位必须落在内核区：内核没有自己的页表，跑在"当前进程"的地址空间里，凡要在任意上下文可达的东西，必须在每个页目录的相同编号处出现——用户区各进程互不相同，只有内核区共享同一布局。所以这两个槽位在**每一个**进程的页目录里都保持空闲。临时窗口写入的目标是 `ptproc`——**当前 CR3 所加载页目录所属的进程**：boot 期由 `arch_post_init` 设为 VM（`protect.c:370-376`），运行期每次上下文切换经 `switch_address_space` 更新为刚被调度的进程（`proc.c:349`、`arch_proto.h:157-159`），所以写入目标随调度在 VM/PM/任意进程间变化，而槽位编号不变。BKL 下单 CPU 执行，无需加锁。

**为什么必须在 arch_post_init 之后**：`createpde` 把临时映射写入 `get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde]`——即 ptproc（当前 CR3 加载的页目录所属进程，boot 期是 VM，见上段）的页目录（§2.5）。若 `arch_post_init` 未先执行，ptproc 未设置（cpulocals 零初始化，值 NULL），`get_cpulocal_var(ptproc)->p_seg.p_cr3_v` 解引用 NULL 崩溃。这是**因果依赖**而非惯例：freepdes 槽位只是索引，真正的映射载体是 ptproc 的页目录——MMU 只按 CR3 装的页目录解释 VA，临时映射必须写进它才有效。阶段 D 三步顺序（arch_post_init → pg_info → memory_init）由此强制。direct_map 下此依赖消失——内核用自己的 direct map 解释 VA，不借任何进程页目录。

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

---

## 3. Rust 设计决策

### 3.1 本质：direct_map 替代 freepdes（rewrite not translate）

**本质**：跨地址空间访问的解法随位宽演进，64 位下 direct_map 是自然解。

**约束驱动**：64 位虚拟地址空间充裕（256TB+），可以建立全物理内存的固定映射；direct map 本身是纯地址运算（`va = pa + BASE`），不引入任何全局可变状态——对比 C 方案需要 `freepdes[]`/`nfreepdes` 两个内核全局维护临时窗口槽位（§2.4）。

**假设性推理**：如果翻译 Minix3 的 freepdes/ptproc，会泄漏 32 位临时窗口模型到 64 位 OS 层——污染页目录视图、需要清理、TLB 反复 flush、4MB 粒度限制全部继承，且 64 位地址空间本可避免这些。更糟的是，64 位页表是 4 级（PML4+PDPT+PD+PT），"借页目录"的语义从 PDE 变成 PML4E，临时窗口的粒度和复杂度都上升，而 direct_map 可以让这一切消失。

**决策**：废弃 freepdes/ptproc/`memory_init` 整套，用 direct_map 重新表达。这是 rewrite not translate——不是改 freepdes 的实现，是换整个机制。

### 3.2 双视图地址空间：VM direct map + Kernel direct map

**本质**：同一物理内存需要两个窗口，因为 x86-64 的 U/S 位不能同时 0 和 1。

| 窗口 | U/S 位 | 使用者 | 建立者 | 建立时机 |
|------|--------|--------|--------|---------|
| VM direct map | U/S=1 | VM 用户态 | kernel（`establish_boot_dm`） | 内核 boot 期（arch_boot Step 4，§1.3.2） |
| Kernel direct map | U/S=0, G=1 | 内核态 | kernel（`establish_boot_dm`） | 内核 boot 期（同上）；受管进程根由 VM `map_kernel` 补齐（§3.4） |

**假设性推理**：如果只有一个 direct map，要么 VM 访问不了（U/S=0，用户态访问触发 fault），要么内核安全降级（U/S=1，用户态能访问内核内存）。硬件特权级要求两个窗口——这不是设计冗余，是 ISA 规范的必然。

**关键不变量**：Kernel direct map 建立后只读不变。

**G=1（Global 位）的 TLB 行为细节**：

PTE 的 Global 位（bit 8）告诉 CPU："这条 PTE 的翻译对所有进程地址空间有效，CR3 切换时不要 invalidate 对应的 TLB 条目"。这有 3 个前提：

1. **CR4.PGE 位必须启用**：x86-64 通过 CR4 第 7 位（PGE, Page Global Enable）开启 Global 位语义。Minix3 在 `vm_enable_paging()`（`minix3/minix/kernel/arch/i386/pg_utils.c:204`）中"先开 paging，再开 PGE"（`pg_utils.c:234` 注释、`pg_utils.c:235-242` 执行顺序），并以 CPU 特性检测为门（`pgeok = _cpufeature(_CPUF_I386_PGE)`，`pg_utils.c:209`）。minix-rs 对应在 x86-64 `Paging::enable()` 收尾处开启 CR4.PGE（`os/arch/src/x86_64/paging.rs:408-421`），同样以 CPUID.01H:EDX.PGE（bit 13）检测为门（`cpu_supports_pge`，`os/arch/src/x86_64/paging.rs:155-172`）——顺序与 C 一致：CR3/CR0.PG/CR0.WP 之后才设 PGE。CR4 的另一处写入是 FPU 初始化（`os/arch/src/x86_64/fpu.rs:88`，OSFXSR/OSXMMEXCPT），与 PGE 无关。未启用 PGE 时 CPU 忽略 G 位（Intel SDM：CR4.PGE=0 时 G flag 被忽略）——PTE 中的 G=1 无害，但没有 TLB 保留效果
2. **PTE 必须有 G=1 + 有效 P（Present）位**：纯 G=1 但 P=0 的 PTE 仍会被 invalidate（无效条目不缓存）
3. **TLB shootdown 影响**：即使 G=1，CPU 显式 `invlpg`（x86-64）/ `tlbi`（aarch64/riscv64）单条 invalidate 仍生效；G=1 只豁免**全局 CR3 切换**的 flush

**minix-rs 中携带 G=1 的映射**（统一经 `PageFlags::kernel_read_write()` = PRESENT|WRITABLE|GLOBAL，`os/arch/src/arch/paging.rs:99`）：bootstrap 的 identity 与 kernel 高地址段映射（`os/kernel/src/lib.rs:288`、`os/kernel/src/lib.rs:313`）、boot 期双 DM 窗口的 Kernel DM（`os/kernel/src/dm_coverage.rs` 的 `kernel_flags`）、VM `map_kernel` 的 kernel 代码/数据段与 Kernel direct map（`os/arch/src/arch/paging.rs:508-522`）。CR4.PGE 在 bootstrap 阶段 `enable()` 时即已开启，这些映射从建立之初就具备"CR3 切换不刷 TLB"的保留效果。

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

**约束驱动**：VM server 已实现 `DirectMapArch` trait（[VM 07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.2），定义在架构层（`os/arch/`），kernel 和 VM 都引用。kernel 侧应直接对接，避免重复抽象。

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

- 双窗口（VM DM + Kernel DM）：由 kernel 的 `establish_boot_dm` 一次建立（arch_boot Step 4，kmain 之前）——装在 bootstrap root 上，VM 经 A1 adoption 继承（§1.3.2）
- 受管进程根的 Kernel DM：由 VM 的 `map_kernel` 补齐（VM 运行后装配每个受管进程根时）——supervisor 权限的内核段 + Kernel DM 窗口（`os/servers/vm/src/vmproc/vmproc_handle.rs:388`）

注意两点：①`map_kernel` 全部映射段硬编码 supervisor 权限（`kernel_read_write()`），只能表达 supervisor DM——VM root 的 USER DM 窗口不经过它，那来自 boot 期建立 + A1 继承（权限分流是 E6 硬约束：USER DM 若经 `map_kernel` 内部 flag 分流，会把地址空间角色重新揉成一个 flag）；②`map_kernel` 的 Kernel DM 窗口沿用 boot 期既定的 VA 布局（`KERNEL_DIRECT_MAP_BASE`），不是第二次设计。

### 3.5 阶段 D 简化：确认 direct_map 就绪

**本质**：direct_map 在内核 boot 期（arch_boot Step 4，早于阶段 C 的 A1 adoption）已就绪，阶段 D 从"分配临时窗口"降级为"确认就绪"。

**约束驱动**：direct_map 是永久映射，不需要运行时分配/清理；但"确认就绪"是必要的安全检查——避免运行时才发现 direct map 缺失，那时定位困难。

**假设性推理**：如果完全删除阶段 D，kmain 流程少一步，但失去"direct map 就绪"的显式断言点。运行时 `createpde` 等价函数（`kernel_phys_to_virt`）失败时，难定位是 direct map 没建还是访问越界。保留一个确认步骤，让 fail-fast 发生在启动阶段而非运行时。

**决策**：阶段 D 保留为"确认 VM direct_map 就绪"的验证步骤，废弃 `arch_post_init` 的 ptproc/`pg_info` 和 `memory_init` 的 freepdes 分配。

**时序边界**：阶段 D 确认的双窗口都由 `establish_boot_dm` 在 arch_boot Step 4 建立（§1.3.2），早于阶段 C 的 A1 adoption，更早于阶段 D。VM 启动后的 `map_kernel` 只为受管进程根补齐 supervisor Kernel DM，不改变阶段 D 确认的 bootstrap root 状态。

### 3.6 废弃清单与假设性推理汇总

| 废弃项 | C 对应 | 如果保留会怎样 | direct_map 替代 |
|--------|--------|--------------|----------------|
| ptproc per-CPU 变量（createpde 用途） | `cpulocals.h:55` | 泄漏 32 位"借页目录"模型，64 位下无意义 | kernel 有 Kernel direct map，不借页目录 |
| freepdes[] 数组 | `memory.c:32` | 全局可变状态 + 临时窗口全部问题 | direct map 永久映射（纯地址运算） |
| memory_init() | `memory.c:707` | 运行时分配槽位的逻辑冗余 | 无需分配 |
| PostInitArch trait | 无 C 对应 | 与 DirectMapArch 形成两套抽象 | 不引入——阶段 D 降为断言，无架构差异可抽象（§3.5） |
| MemoryInitArch trait | 无 C 对应 | 同上 | 不引入（同上） |
| FreePdeSlots 结构体 | `freepdes[]` | 表达临时窗口槽位，direct map 不需要 | 无——direct map 是常量加法 |
| VmPageTableInfo | `pg_info` 输出 | 记录 bootstrap 页表地址，direct map 不依赖 | `kernel_phys_to_virt` 直接访问 |
| FREE_PDE_SLOTS static | `freepdes[]` | 全局可变状态 | 无 |
| FREE_UPPER_IDX static | `kinfo.freepde_start` | 全局原子，临时窗口索引 | 无（协议字段 `KernelInfo.free_upper_idx` 保留，见 §4.3） |

> 废弃不是删除代码，是换机制——每个废弃项都有 direct_map 的替代或确认不需要。各废弃项在现代码中的落地形态（含唯一幸存的 IPC 契约字段）见 §4.3。
>
> **ptproc 跟踪不全部废弃**：上表"ptproc per-CPU 变量"废弃的是其 **createpde 临时窗口用途**（借页目录放临时映射）。ptproc 的第二个用途——`setcr3()` 中 `if (p == ptproc)` 决定是否立即 reload CR3（arch_do_vmctl.c:25）——在 direct_map 下**保留**，因为 VM 仍通过 `VMCTL_SETADDRSPACE` 切换页表。Rust 用 kernel 全局 `CURRENT_PTPROC_NR: AtomicI32`（os/kernel/src/lib.rs:2169）+ `set_current_ptproc_nr(VM_PROC_NR)`（os/kernel/src/lib.rs:2212）跟踪此用途。逐用途判定去留的判据见 §4.3 与 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md)。

> **内核全局状态存储模式**：kernel 全局（`CURRENT_PTPROC_NR`、`PROC_TABLE`/`PRIV_TABLE` 等，见 [06-proc-init-boot-proc.md §4.3](06-proc-init-boot-proc.md)）采用"分散全局"存储——每个全局一个原子类型或固定数组，与 C 源码的全局变量一一对应，而非聚合为单一 `KernelState` 结构体。理由：① **可审计性**——分散 static 与 C 全局一一映射，review 可逐一核对；② **渐进式 init**——每个子系统独立初始化，聚合结构必须等所有子系统 init 后才能构造，编译期依赖复杂；③ **无跨子系统联动需求**——各子系统全局以参数形式传递，无需单一访问点。若后续出现需跨子系统原子联动的全局，再评估聚合。

---

## 4. 实现详解

> 对应 Ch3 每个决策，讲具体接口/流程。本章较瘦，因为大量实现被废弃。

### 4.1 DirectMapArch 接口（引用 VM 已实现）

`DirectMapArch` trait 定义在架构层（`os/arch/`），VM server 和 kernel 共用（[VM 07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.2）：

```rust
pub trait DirectMapArch {
    /// VM direct map 基地址（U/S=1，VM 用户态用）
    const VM_DIRECT_MAP_BASE: u64;
    /// VM direct map 窗口容量——能表达的 PA 上界（设计输入，见下方表格）
    const VM_DIRECT_MAP_SIZE: u64;
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

三架构 BASE 值（详见 [VM 07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.2）：

| 架构 | VM_DIRECT_MAP_BASE | VM_DIRECT_MAP_SIZE | KERNEL_DIRECT_MAP_BASE | 说明 |
|------|-------------------|--------------------|-----------------------|------|
| x86-64 | `0x0000_0000_8000_0000` | 1 GiB | `0xFFFF_8080_0000_0000` | VM 在 2GB 用户态低区；Kernel DM 占 PML4[257]——内核镜像链在 PML4[256]（`0xFFFF_8000_0000_0000`），镜像翻译 `VA = kern_virt_base + offset` 不服从 DM 语义（`VA = DM base + PA`），DM 窗口不得与镜像共享顶层槽位 |
| aarch64 | `0x0000_1000_0000_0000` | 2 GiB | `0xFFFF_8080_0000_0000` | 与 x86-64 同构：镜像在 L0[256]，DM 窗口占 L0[257]；窗口取 2GiB 是 DM-window admissibility precondition 的要求——QEMU virt RAM base = 1GiB，若窗口只有 1GiB（PA [0, 1GiB)），全部 conventional RAM 落在窗口之外 |
| riscv64 (Sv39) | `0x0000_0010_0000_0000` | 16 GiB | `0xFFFF_FFC0_4000_0000` | 镜像在 VPN[2]=256（`0xFFFF_FFC0_0000_0000`），DM 窗口占 VPN[2]=257；base 不能取非 canonical 的 `0xFFFF_FC00_0000_0000`——其 39 位载荷解码后正是镜像自己的槽位 |

> **address-layout constant 是设计输入，不是实现细节**。窗口常量若结构性不满足平台布局（假设 AArch64 用 1GiB 窗口面对 RAM base = 1GiB 的平台），失效形态不是"某个不变量被违反"——不变量形式完好——而是**架构功能性不可用**：全部 conventional RAM 无 DM 可表达性，VM PMM eligible = ∅，capacity validation 拒绝启动。这正是窗口常量与资格过滤、capacity validation 同列一条推导链的原因（覆盖、资格、分配三层由同一推导锁定），也是三架构的窗口值必须逐一对照目标平台 RAM base 核算的原因（`os/arch/src/arch/direct_map.rs` 各实现块内含平台布局核算与编译期 `VM_HEAP_BASE = VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE` 断言）。

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

1GB huge page 的 CPU 支持检查（`supports_1gb_page()`）：x86-64 查 `CPUID.80000001H:EDX.GBPAGES`，不支持时回退 2MB。详见 [VM 07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.2。

> **⚠️ DirectMapArch 职责边界（避免误用 direct map 分配内存）**——`DirectMapArch` 只做一件事：**PA ↔ kernel VA 的线性转换**。它**不负责、也永远不会负责分配物理帧**（无 `alloc_phys_page` / `free_phys_page`，见 `os/arch/src/arch/frame.rs` 模块 doc-comment）。
>
> 物理帧的**来源**由分配机制决定（`os/arch/src/arch/frame.rs` 的 VM Bootstrap Memory Handoff 注释段）：
> - VM ELF image / 用户栈帧（bootstrap）：`VmBootAllocator`（消费 `VmBootRegion::select_multi(memmap, exclusions)` 验证区）
> - 页表页（bootstrap）：`BootAlloc` / `pt_alloc`
> - VM runtime：VM 自己的 bitmap/buddy PMM
>
> `DirectMapArch`（经 `PhysAccess` 的 blanket impl）只回答"这个 PA，kernel 用什么 VA 访问"。生产中 `CurrentDirectMap` 作为 `PhysAccess` 传给 `load_vm_elf`（09-vm-boot-protocol.md §4.9.3）。**若未来代码出现"从 DirectMapArch 分配内存"的需求，那是设计错误**——分配要回到 `VmBootAllocator` / VM PMM，而不是扩 `DirectMapArch`。

### 4.2 阶段 D 入口：确认 direct_map 就绪

阶段 D 入口 `init_post_and_memory` 是确认步骤（os/kernel/src/lib.rs:1271-1315）：

```rust
pub fn init_post_and_memory(proc_table: &crate::proc_table::ProcessTable) {
    use minix_arch::{CurrentDirectMap, DirectMapArch};

    // Step 1: Assert VM's page-table root is valid.
    // The root was installed in Phase C (`init_proc_and_boot`): for the
    // non-mock path, `p_seg.phys_root` records the bootstrap root and
    // `p_seg.virt_root` its identity-mapped VA (lib.rs:960-963).
    let vm_proc = proc_table.get(crate::proc::proc_nr::VM_PROC_NR)
        .expect("VM process must be initialized before init_post_and_memory");
    assert!(
        vm_proc.p_seg.phys_root.0 != 0,
        "VM page-table root (phys) must be valid after stage C"
    );
    assert!(
        vm_proc.p_seg.virt_root.is_some(),
        "VM page-table root (virt) must be kernel-mapped after stage C"
    );

    // Step 2: Record VM as the kernel-level ptproc.
    // C: get_cpulocal_var(ptproc) = vm — protect.c:372
    // Enables `dispatch_vmctl(SetAddrSpace)` Step 3 to decide whether to
    // reload the hardware root register (setcr3 semantics).
    set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);

    // Step 3: Assert the VM Direct Map base is configured.
    assert!(
        CurrentDirectMap::VM_DIRECT_MAP_BASE != 0,
        "VM direct map base must be configured"
    );
}
```

**流程**：获取 VM 进程 → 断言页表 root 有效（phys + virt）→ 记录 VM 为 kernel 级 ptproc → 断言 direct map base 已配置 → 返回。**无任何分配或登记动作**——阶段 D 的全部产出就是两组断言与一次 ptproc 记录，临时窗口机制的分配/登记职责整体消失（取舍见 §3.5、落地形态见 §4.3）。

**为什么没有 `verify_vm_direct_map_present` 式的页表 walk**：双窗口在 arch_boot Step 4 建立时已保证其存在（`establish_boot_dm` 失败即拒绝启动，fail-fast 已在 boot 期触发，§4.4），阶段 D 的断言是"就绪确认 + 契约记录"——用两个廉价断言把失败点固定在启动期。运行时真正的 direct map 访问（`kernel_phys_to_virt`）失败时，由 24-cross-space-runtime.md 的调用方处理。

**时序边界**：阶段 D 确认的双窗口由 `establish_boot_dm` 在 arch_boot Step 4 建立（§1.3.2、§4.4），早于阶段 D 的一切动作。VM 启动后的 `map_kernel` 只为受管进程根补齐 supervisor Kernel DM，不涉及此处验证的 bootstrap root。

### 4.3 废弃机制的落地形态：抽象归零与契约保留

废弃清单（§3.6）的每一项在现代码中都有明确的落地形态——不是"删除后留白"，而是由更简单的结构接管或以另一种身份幸存。三个形态各有一条可复用的判据。

**形态一：初始化层归零**。arch 级"启动阶段初始化 trait"（C 中对应 `arch_post_init`/`memory_init` 的装配点）在现代码中没有对应物。direct map 把阶段 D 的工作降为三个断言（§4.2），而断言不需要 trait 分发——三个断言在三种架构上行为完全一致，没有任何架构差异可供抽象；为它建 trait 只会制造第二个 `DirectMapArch`（§3.3 的重复抽象问题）。判据：**能用常量与断言表达的东西，不引入抽象层**——某层"没有 trait"可以是一个经过论证的设计结论，而非实施遗漏。

**形态二：协议字段保留**。`KernelInfo.free_upper_idx`（C: `kinfo.freepde_start`）是唯一幸存的 freepdes 痕迹，但身份已变——它不再是内核内部的槽位发号器，而是 `struct kinfo` 的镜像字段（C 由 kernel `pre_init.c:232` 填充、VM `pagetable.c:1030` 递增消费，用于 VM 的页目录槽位分配）。minix-rs 中 kernel 内部与 VM 都不消费该值（VM 无页目录槽位分配器），boot-shim 填 `None`（`os/boot-shim/src/uefi_helpers.rs:235`、`opensbi_helpers.rs:439`），GET_KINFO 回复的 `m4l4` 槽位填 0（`os/kernel/src/misc.rs:775`）。字段保留的理由：`KernelInfo` 是 C `struct kinfo` 的镜像，GET_KINFO 未来对齐全量 struct 拷贝（C: `do_getinfo.c:66` data_copy 语义）时需要逐字段对应。判据：**镜像 C 协议结构的字段按 C 布局保留，语义空缺用显式的 None/0 表达**——判断一个字段该不该删，看的是它在 C 协议结构中的位置，而不是有没有内部读者。

**形态三：按用途拆分去留**。C 的 per-CPU `ptproc` 有两个用途，64 位下命运不同（§3.6 脚注）：借页目录放临时映射（createpde 用途）随临时窗口消亡；`setcr3()` 的 CR3-reload 决策保留为 kernel 全局 `CURRENT_PTPROC_NR` + `set_current_ptproc_nr`（os/kernel/src/lib.rs:2169/:2212）。判据：**名字相同的机制也要逐用途判定去留**——"ptproc 废弃了"与"ptproc 保留着"同时成立，取决于谈的是哪个用途。

### 4.4 DM 覆盖建立过程：establish_boot_dm

§1.3.2 的概念链路在代码中的落点是 `establish_boot_dm`（`os/kernel/src/dm_coverage.rs:66`），由 `arch_boot_impl` Step 4 调用（`os/kernel/src/lib.rs:348`，开分页之后、kmain 之前）。VA 布局常量与 `DirectMapArch` trait 见 [VM 07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.2/§3.4；本节讲建立算法本身。

**两源候选并集**：映射哪些物理内存不是"从 0 到最大物理地址反推连续映射"——memmap 非连续（reserved/MMIO/firmware hole 穿插其间），反推会把 hole 一并塞进映射。候选是两源的并集：

| 源 | 内容 | 为什么需要 |
|----|------|-----------|
| 源 1 | memmap conventional 区段（boot-shim 已按类型过滤） | VM PMM 管理的物理 RAM |
| 源 2 | bootstrap 页表树：root 页 + boot bump 区 | 二者是 boot-shim 的固件 `LOADER_DATA` 分配，永不出现在 conventional memmap 里——不显式登记则 self root 永远进不了覆盖，VM 页表遍历无从谈起 |

源 2 与源 1 重叠时丢弃（并集语义；fallback/测试路径的 bump 区在 conventional RAM 内，由源 1 覆盖同一范围）。来源不同、过滤规则共用：资源安全性、窗口边界、页粒度包含性。

**建立顺序——Kernel DM 先行**：同一组候选先装 Kernel DM 窗口（supervisor RW，PA 不设上界），再装 VM DM 窗口（user RW，PA 裁剪到 `VM_DIRECT_MAP_SIZE`）。先行的原因：x86-64 上 VM 窗口与 identity 重叠，identity 对 PA [2GiB, 3GiB) 的翻译被取代后，内核对该 PA 区间的访问只能走 Kernel DM——若 VM 窗口先行，存在一个内核失去 PA 访问通道的中间态。超出 VM 窗口 PA 上界的候选被裁剪：这部分物理内存不具 DM 可表达性，后续经资格过滤排除在 VM PMM 之外（覆盖、资格、分配三层由同一条推导链锁定）。

**候选范围 → 叶子映射**：每个候选范围经 `establish_dm_range`（`os/arch/src/arch/dm_coverage.rs`）裁剪到窗口 PA 上界，再按**资源包含性**选页粒度（1GiB/2MiB/4KiB）——大页仅当整个叶页落在同一候选区段内才成立，reserved hole 不可能被任何粒度的大页吞没。逐叶子装到 `va_base + pa`。

**x86-64 identity 重叠处理**（`os/arch/src/x86_64/paging.rs:747` `dm_install_leaf`）：VM DM 窗口 [2GiB, 3GiB) 与 identity 映射的 PDPTE[2] 1GiB 大页重叠——同一 VA 存在两个互斥翻译（identity：PA=VA，supervisor；DM：PA=VA−BASE，user|RW）。处理按包含性分支：

- 1GiB 叶子单元仅在 PA [0,1GiB) 整体属于允许覆盖的资源范围时可行（整页替换 PDPTE[2]，无新增页表页）。真实 x86 PC 布局低 1GiB 结构性含 legacy hole [0xA0000, 0x100000)（VGA/ROM/BIOS，非 conventional），包含性必然失败——因此拆分路径是 x86-64 的常态
- 拆分路径把 identity 的 1GiB 大页（或 2MiB 叶子表）**替换为空下级表**：identity 对 VA [2GiB, 3GiB) 的翻译被整体取代，替换后的表只保留后续允许单元重映射的内容——hole 不被吞没，也不会以 user|RW 方式暴露给 VM
- VM 窗口路径的中间层（PML4/PDPT/PD）携带 USER 位：U/S 位跨层级 AND，identity 建立的 PML4[0] 原为 supervisor-only，不提 U 则 user 叶子实际不可达

identity 的宽松（吞 hole）是内核 boot 的 supervisor 可达性保障语义；VM DM 的严格（不吞 hole）是 user|RW 资源暴露语义——同一重叠，两套信任边界，前者不构成后者的先例。

**自举写通道**：建立期所有 PTE 写经 bootstrap root 的 identity 通道（VA=PA，开分页后即可用）。x86-64 上 PTE 页分配受上界约束（PA < min(IDENTITY_MAP_END, 窗口 PA 末端)），写 VA < 1GiB 落在 PDPTE[0]，与 PDPTE[2] 的替换/拆分正交——写通道不被自身修改波及，无循环依赖。运行期 PTE 访问切换到 Kernel DM 通道（`phys_to_ptr_dm`）——PTE 操纵必须使用与执行上下文相称的 PhysAccess 能力（kernel 上下文走 Kernel DM，VM 上下文走 VM DM）。

**启动验证**：bootstrap root 页与 boot bump 区必须落在 DM 可表达上界内（identity 写通道要求 PA < IDENTITY_MAP_END；VM 页表遍历要求 root 树可经 VM DM 访问）——违反即拒绝启动。这是"双窗口可用"的构造保证，也是阶段 D 断言（§4.2）能保持廉价的原因：失败点已在 boot 期被拦截。

---

## 5. 测试要点

> 覆盖 Ch3+Ch4 的每个核心决策。

### 5.1 direct_map 就绪验证

- **测试**：阶段 D 确认步骤能正确识别 direct map 已就绪（VM 进程页表 root 有效 + direct map base 已配置）
- **测试**：direct map 未就绪时确认步骤 panic（fail-fast，启动阶段暴露问题而非运行时）——`init_post_and_memory` 的 `.expect("VM process must be initialized")` + 两条 `assert!` 构成 fail-fast 点
- **测试**：`CurrentDirectMap::kernel_phys_to_virt(pa)` 返回正确 VA（对接 `DirectMapArch` 的 mock 实现）
- **测试**：双窗口建立本身（§4.4）——`establish_boot_dm` 的两源候选并集（源 2 与源 1 重叠时丢弃）、窗口 PA 裁剪、bootstrap 树上界断言（root/bump 区越界即拒绝启动）由 `os/kernel/src/dm_coverage.rs` 单元测试覆盖（mock 特性下经 `MockDmCoverage` 注册表驱动，不触真硬件）
- **测试**：建立结果在真实启动链上成立——test-proc-init 的 P1-c 哨兵链（见下方 QEMU 覆盖）

> **QEMU 覆盖**：`os/qemu-tests/test-kernels/kernel/bootstrap/test-proc-init` 在真机启动流程中调用 `init_post_and_memory`，验证阶段 D 在真实 boot 路径中不 panic。同测试的 **P1-c 哨兵链**验证双窗口建立结果：①哨兵值经 Kernel DM 通道写入哨兵物理页（通道输入不可伪造）；②同一物理页经 VM DM 窗口读回哨兵（覆盖证明）；③把哨兵页 map 到全新 VA（5GiB，此前从未翻译、无陈旧 TLB），CPU 经活动根读回哨值（被修改的页表树 == CPU 执行的页表树，且 PTE 访问通道在真实硬件可用）。`os/arch/tests/qemu_test_x86_64_procd.sh` 用 GDB breakpoint 验证到达 `init_post_and_memory` 并完成。

### 5.2 测试清单为何变短：结构性消失的错误类别

C 的 freepdes/ptproc 临时窗口机制需要一整类测试来保护：槽位分配正确性（不重复分配）、窗口越界（临时映射是否覆盖目标 PA）、状态清理（进程退出后槽位归还）。direct map 下这些错误类别**结构上不存在**——地址转换是纯加法（`va = pa + BASE`），没有分配器、没有索引、没有槽位，也没有可失败路径，因此该层没有运行时测试可写。

这不是测试遗漏，而是设计结论：错误类别随机制消失，测试随之消失（取舍见 §4.3 形态一）。07 覆盖的是仍然存在的部分——阶段 D 断言（§5.1）与 DirectMapArch 对接（§5.3）。

### 5.3 DirectMapArch 对接测试

- **测试**：kernel 侧通过 `CurrentDirectMap` 正确调用 `vm_phys_to_virt`/`kernel_phys_to_virt`
- **测试**：三架构 BASE 常量正确（x86-64/arm64/riscv64）
- **测试**：1GB huge page 不支持时回退 2MB

direct_map 的测试在 VM 层已覆盖（[VM 07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §5），kernel 侧仅测对接。

### 5.4 测试边界：direct_map 抽象（07）与 syscall_copy 消费路径（18）

07 文档承诺的"direct_map 替代临时窗口"是设计层面的承诺——direct_map 在概念上取代了 freepdes/ptproc 机制；使用该抽象的运行时代码路径（syscall_copy 的跨空间拷贝/填充）由 [18-syscall-copy.md](18-syscall-copy.md) 实现并自带测试。两层在代码中的分工：

| 层 | 职责 | 代码位置 | 测试归属 |
|----|------|---------|---------|
| 抽象层 | 跨进程 PTE walk（`lookup_in_table`/`lookup_range_in_table`，经 `CurrentPteWalk` 三架构分发）+ `DirectMapArch::kernel_phys_to_virt` + VMSUSPEND 语义 | `os/kernel/src/vm.rs`（`cross_space_copy`/`cross_space_memset`）、`os/kernel/src/cross_space.rs`（`data_copy_vmcheck`/`memset_vmcheck`） | 07 §5.1-§5.3 |
| 消费层 | 七组系统调用（copy/safecopy/vsafecopy/umap/vumap/memset/safememset）经抽象层完成跨空间访问 | `os/kernel/src/syscall_copy.rs`（模块注释列出全部接入点） | 18 文档 |

07 的"确认 direct_map 就绪"断言（§4.2）保护的是消费层的前提；消费路径自身的行为测试（缺页挂起、grant 验证、SELF 替换）不在 07 范围。07 文档不引用 syscall_copy.rs 的具体行号，具体行为描述以 18 文档为准。

---

## 附录 A. 阶段 D 时序图（direct_map 版）

```
arch_boot (高半转换, 02-higher-half-kernel.md §4.4):
  Step 1-2: identity + 内核高地址映射
  Step 3:   开分页（bootstrap root 激活）
  Step 4:   establish_boot_dm —— 双窗口一次建立（§1.3.2、§4.4）
            - Kernel DM 先行（supervisor RW, PA 全域）
            - VM DM 随后（user RW, PA 裁剪到窗口）
            - bootstrap 树上界断言（越界拒绝启动）
  ↓
阶段 C: A1 adoption —— VM 收编 bootstrap root（含双窗口）
        详见 VM 07-pagetable-struct.md §3.4
  ↓
阶段 D: 确认双窗口就绪（init_post_and_memory）
        - 断言 VM 进程页表 root 有效（phys + virt）
        - 记录 VM 为 kernel 级 ptproc（set_current_ptproc_nr）
        - 断言 direct map base 已配置
        - 无分配动作（临时窗口已由 direct map 取代）
  ↓
阶段 E-F: system_init + bsp_finish_booting（见 08）
  ↓
VM 启动后: map_kernel 为受管进程根补齐 Kernel DM（supervisor, G=1）
           详见 VM 08-pagetable-ops.md §3.4
```

## 附录 B. Minix3 vs minix-rs 阶段 D 对照

| 方面 | Minix3 C (32位) | minix-rs (64位 direct_map) |
|------|----------------|---------------------------|
| 跨空间访问机制 | freepdes 临时窗口 | direct_map 永久映射 |
| ptproc（createpde 用途） | per-CPU 变量，记录当前页目录 | 废弃（direct_map 取代） |
| ptproc（setcr3 用途） | `if (p == ptproc)` 决定 reload CR3 | kernel 级 `CURRENT_PTPROC_NR` + `set_current_ptproc_nr`（保留） |
| memory_init | 分配 2 个 freepdes | 废弃 |
| pg_info | 记录 bootstrap 页表地址 | 废弃（direct_map 不依赖） |
| 阶段 D 内容 | 设 ptproc + 分配 freepdes | 确认双窗口就绪（断言 + ptproc 记录） |
| direct_map 建立 | 无此机制 | `establish_boot_dm`（arch_boot Step 4，双窗口一次建立，VM 经 A1 继承） |
| 内核映射建立 | pg_mapkernel (4MB 大页, pre_init) | boot 期内核高地址映射（arch_boot Step 2）+ `map_kernel` 为受管进程根补齐（含 Kernel DM） |
| 建立者 | kernel (pre_init) | kernel（arch_boot Step 4 双窗口）+ VM（受管进程根补齐） |
| 全局可变状态 | freepdes[]/nfreepdes/kinfo.freepde_start | 无 |

---

## 6. 参见

- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) — 阶段 C：进程表初始化与 VM ELF 加载（含 bootstrap 页表）
- [08-system-init-boot-finish.md](08-system-init-boot-finish.md) — 阶段 E-F：系统调用注册与启动完成
- [24-cross-space-runtime.md](24-cross-space-runtime.md) — 运行时跨空间访问（`createpde` 等价函数 = `kernel_phys_to_virt`）
- [02-stage-vm/07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.2 — direct_map 完整设计（双视图、DirectMapArch trait）
- [02-stage-vm/07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) §3.4 — A1 adoption（VM 收编 bootstrap root，双窗口继承）
- [02-stage-vm/08-pagetable-ops.md](../02-stage-vm/08-pagetable-ops.md) §3.4 — `map_kernel` 职责简化
- `os/kernel/src/dm_coverage.rs` — `establish_boot_dm`（boot 期双窗口覆盖建立 + 启动验证）
- `os/arch/src/arch/dm_coverage.rs` — `DmCoverageArch` trait + `establish_dm_range`（候选范围 → 叶子映射驱动）
- `minix3/minix/kernel/arch/i386/protect.c:370-377` — `arch_post_init`
- `minix3/minix/kernel/arch/i386/pg_utils.c:186-206` — `pg_mapkernel`
- `minix3/minix/kernel/arch/i386/pg_utils.c:312-316` — `pg_info`
- `minix3/minix/kernel/arch/i386/memory.c:707-717` — `memory_init`
- `minix3/minix/kernel/arch/i386/memory.c:35-145` — `mem_clear_mapcache` + `createpde`
- `minix3/minix/kernel/cpulocals.h:55` — `ptproc` per-CPU 变量
- `minix3/minix/kernel/arch/i386/pre_init.c:232` — `kinfo.freepde_start = pg_mapkernel()`
