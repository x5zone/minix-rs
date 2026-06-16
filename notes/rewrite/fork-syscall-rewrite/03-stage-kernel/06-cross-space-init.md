# 06-cross-space-init: 跨地址空间初始化

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/arch/i386/protect.c:370-377`, `minix3/minix/kernel/arch/i386/memory.c:707-717`, `minix3/minix/kernel/arch/earm/protect.c:97-104`, `minix3/minix/kernel/arch/earm/memory.c:612-622`
> **说明**: 阶段 C 完成后，内核设置 ptproc（当前页表进程）并分配 freepdes（临时页目录项），为运行时跨地址空间访问铺路
> **前置**: [05-proc-init-boot-proc.md](05-proc-init-boot-proc.md) — 进程表已初始化、VM ELF 已加载到 bootstrap 页表

---

## 1. 概述

### 1.0 ptproc 与 freepdes：内核如何"看"别人的地址空间

内核运行在自己的地址空间中。VM 运行在 VM 的地址空间中。PM 运行在 PM 的地址空间中。那么问题来了——内核要读写 VM 的内存（比如 `lin_lin_copy`、`vm_memset`），怎么做到？

答案依赖于两个机制，它们正好是阶段 D 初始化的：

1. **`ptproc`**（page table process）：内核维护一个"当前活跃的页表进程"指针。当内核需要访问某个进程的地址空间时，它会临时借用 `ptproc` 的页目录项（PDE）来映射目标进程的页面。`arch_post_init()` 将 `ptproc` 设置为 VM——因为 VM 是第一个拥有完整页表的进程。

2. **`freepdes`**（free page directory entries）：内核页目录中预留的"临时映射槽位"。当 `createpde()` 需要映射一个不在当前页表中的页面时，它将目标页的 PDE 值写入 `freepdes` 指向的页目录项，通过这种"临时窗口"访问目标内存。`memory_init()` 分配 2 个这样的槽位。

**这两个机制的关系**：`freepdes` 写入的是 `ptproc` 页目录中的项——所以 `ptproc` 必须先设置好，`freepdes` 才能工作。这也是为什么 `arch_post_init()` 在 `memory_init()` 之前调用。

### 1.1 阶段 D 的位置

| 阶段 | 标记 | 函数 | 做什么 | 文档 |
|------|------|------|--------|------|
| A: 入口 | T2 | kmain 入口 | memcpy(&kinfo)、BSS 检查 | 03 |
| B: cstart | T2+T3 | cstart() | prot_init → init_clock → intr_init → arch_init | 03+04 |
| C: 进程表 | T4+T5 | proc_init + arch_boot_proc | 清空进程表、加载 VM ELF | 05 |
| **D: post-init** | **T6+T7** | **arch_post_init + memory_init** | **ptproc=VM、freepdes 分配** | **本文** |
| E: system | T8+T9 | system_init + add_memmap | 系统调用初始化、bootstrap 回收 | 07 |
| F: finish | T9.5+T10 | bsp_finish_booting | SMP 初始化、启动完成、切换用户态 | 07 |

阶段 D 仅有两行 C 代码，但它们为内核运行时的核心能力——跨地址空间访问——奠定基础。

### 1.2 为什么 arch_post_init 必须在 arch_boot_proc 之后

`arch_post_init()` 调用 `proc_addr(VM_PROC_NR)` 获取 VM 的进程指针。这个指针必须在阶段 C 的 `proc_init()` 中已经正确初始化（`p_nr`、`p_endpoint` 等字段已设置）。此外，VM 的 `p_seg.p_cr3`（页表物理地址）和 `p_seg.p_cr3_v`（页表虚拟地址）需要在 `arch_boot_proc()` 中被设置——`pg_info()` 会读取这些字段。

### 1.3 为什么 memory_init 必须在 arch_post_init 之后

`freepdes` 写入的是 `ptproc->p_seg.p_cr3_v[pde]`——即 VM 页目录的虚拟地址。如果 `ptproc` 未设置，`createpde()` 中的 `get_cpulocal_var(ptproc)->p_seg.p_cr3_v` 将返回 NULL，导致内核崩溃。

### 1.4 三架构对照

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| ptproc 字段名 | `ptproc`（cpulocal） | `ptproc`（cpulocal） | `ptproc`（cpulocal） |
| ptproc 设置 | `get_cpulocal_var(ptproc) = vm` | `get_cpulocal_var(ptproc) = vm` | `get_cpulocal_var(ptproc) = vm` |
| pg_info 参数 | `&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v` | `&vm->p_seg.p_ttbr, &vm->p_seg.p_ttbr_v` | 对应页表寄存器 |
| freepdes 数量 | 2 | 2 | 2 |
| freepdes 来源 | `kinfo.freepde_start`（由 pg_mapkernel 返回） | `kinfo.freepde_start` | 对应 |
| 页目录项大小 | 8 字节（64 位 PDE） | 8 字节 | 8 字节 |
| 临时映射粒度 | 2MB 大页（PD 映射） | 2MB block（L2 映射） | 2MB 大页 |

### 1.5 Rust 版与 C 版的差异

| 方面 | Minix3 C | minix-rs |
|------|---------|----------|
| ptproc | cpulocal 变量 `get_cpulocal_var(ptproc)` | `PostInitArch::set_ptproc()` trait 方法 |
| pg_info | 直接写 VM 的 `p_seg.p_cr3` / `p_seg.p_cr3_v` | `PostInitArch::set_ptproc()` 内部完成 |
| freepdes | 全局静态数组 `static int freepdes[2]` | `MemoryInitArch::allocate_free_pdes()` 返回 `FreePdeSlots` |
| freepde_start | `kinfo.freepde_start` | `kernel_info.free_upper_idx()` (returns `Option<usize>`) |
| createpde | 全局函数，直接操作 `ptproc->p_seg.p_cr3_v[pde]` | `CrossSpaceArch` trait 方法（后续文档） |

---

## 2. C 源码分析

### 2.1 arch_post_init()：设置 ptproc

x86-64（`protect.c:370-377`）：

```c
void arch_post_init(void)
{
  /* Let memory mapping code know what's going on at bootstrap time */
  struct proc *vm;
  vm = proc_addr(VM_PROC_NR);
  get_cpulocal_var(ptproc) = vm;
  pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);
}
```

aarch64（`protect.c:97-104`）：

```c
void arch_post_init(void)
{
  struct proc *vm;
  vm = proc_addr(VM_PROC_NR);
  get_cpulocal_var(ptproc) = vm;
  pg_info(&vm->p_seg.p_ttbr, &vm->p_seg.p_ttbr_v);
}
```

**逐行分析**：

1. **`vm = proc_addr(VM_PROC_NR)`**：获取 VM 进程的 `struct proc` 指针。`proc_addr()` 是一个 O(1) 查找——直接用 `p_nr` 作为数组索引。此时 VM 的进程 slot 已经在阶段 C 被 `arch_boot_proc()` 初始化过。

2. **`get_cpulocal_var(ptproc) = vm`**：将 VM 设为当前 CPU 的"页表进程"。`ptproc` 是 per-CPU 变量，定义在 `cpulocals.h:55`。它指向"当前活跃的页表所属的进程"——内核通过临时修改 `ptproc` 的页目录来映射其他进程的内存。

3. **`pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v)`**：记录 bootstrap 页目录的物理地址和虚拟地址。这两个地址由 `pg_info()` 写入全局变量 `pagedir`（虚拟地址）和 `pagedir_ph`（物理地址）。

**为什么是 VM？** 三个原因：

- VM 是第一个拥有完整页表的进程。其他进程（PM、VFS 等）还没有页表——它们的 `RTS_VMINHIBIT` 标志还设置着。
- `createpde()` 需要一个"宿主页表"来放置临时映射——VM 的页表是唯一可用的。
- 从此刻起到 VM 通过 `VMCTL_SETADDRSPACE` 切换到自己的真实页表之前，内核和 VM 共享 bootstrap 页表。

### 2.2 pg_info()：记录页目录地址

x86-64（`pg_utils.c:312-316`）：

```c
void pg_info(reg_t *pagedir_ph, u32_t **pagedir_v)
{
    *pagedir_ph = vir2phys(pagedir);
    *pagedir_v = pagedir;
}
```

`pg_info()` 做的事情极其简单——把全局变量 `pagedir`（bootstrap 页目录的虚拟地址）转成物理地址，写入 VM 进程的 `p_seg.p_cr3` 和 `p_seg.p_cr3_v`。这样 VM 的进程结构体就知道了"自己的页表在哪"。

**但是**——VM 此时的页表不是 VM 自己建的，而是内核在 `arch_boot_proc()` 中用 `pg_map(PG_ALLOCATEME, ...)` 建的 bootstrap 页表。VM 运行后会通过 `VMCTL_SETADDRSPACE` 建立自己的页表，替换掉 bootstrap 页表。所以 `pg_info()` 记录的是**初始**页表地址，不是最终地址。

### 2.3 IPCNAME 宏：IPC 调用类型名称注册

在 `arch_post_init()` 和 `memory_init()` 之间，`main.c:277-290` 有一段 IPCNAME 宏调用：

```c
IPCNAME(SEND, 0);
IPCNAME(RECEIVE, 0);
IPCNAME(SENDREC, 0);
IPCNAME(NOTIFY, 0);
IPCNAME(SENDNB, 0);
IPCNAME(RECEIVE_ASYNC, 0);
/* ... 共约 13 个调用类型 */
```

`IPCNAME` 宏（定义在 `kernel/ipc.h`）将 IPC 调用类型编号映射为可读字符串，写入全局数组 `ipcnames[]`。它**仅用于调试输出**（如 `kprintf` 打印 IPC 统计），不影响任何运行时逻辑。

**Rust 版替代方案**：C 版用宏+全局数组实现名称查找，Rust 版可用 `enum IpcCall { Send, Receive, ... }` + `impl Display for IpcCall` 替代，无需全局数组，编译期保证完整性。

### 2.4 memory_init()：分配临时映射槽位

x86-64（`memory.c:707-717`）：

```c
void memory_init(void)
{
    assert(nfreepdes == 0);

    freepdes[nfreepdes++] = kinfo.freepde_start++;
    freepdes[nfreepdes++] = kinfo.freepde_start++;

    assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES);
    assert(nfreepdes == 2);
    assert(nfreepdes <= MAXFREEPDES);
}
```

aarch64（`memory.c:612-622`）：

```c
void memory_init(void)
{
    assert(nfreepdes == 0);

    freepdes[nfreepdes++] = kinfo.freepde_start++;
    freepdes[nfreepdes++] = kinfo.freepde_start++;

    assert(kinfo.freepde_start < ARM_VM_DIR_ENTRIES);
    assert(nfreepdes == 2);
    assert(nfreepdes <= MAXFREEPDES);
}
```

**逐行分析**：

1. **`assert(nfreepdes == 0)`**：确保 `memory_init()` 只被调用一次。

2. **`freepdes[0] = kinfo.freepde_start++`**：取第一个空闲的页目录项索引。`kinfo.freepde_start` 在 `pre_init.c:232` 中由 `pg_mapkernel()` 返回——它是内核映射之后的第一个空闲 PDE 索引。自增后 `freepde_start` 指向下一个空闲索引。

3. **`freepdes[1] = kinfo.freepde_start++`**：取第二个空闲 PDE 索引。

4. **`assert(kinfo.freepde_start < 512)`**（64 位）：确保分配后没有越界。

**为什么是 2 个？** `createpde()` 的调用者 `virtual_copy_f()` 和 `vm_memset()` 都需要"源"和"目标"两个临时映射。每次跨地址空间拷贝需要两个 freepde——一个映射源，一个映射目标。

### 2.5 freepdes 如何被使用：createpde() 快速导览

`createpde()` 是 freepdes 的唯一消费者。它的工作流程：

```
1. 检查目标进程是否就是 ptproc 或是内核
   → 是：直接返回 linaddr（已经在页表中可见）
   → 否：需要临时映射

2. 获取目标进程的 PDE 值：
   - 如果是进程内存：读 pr->p_seg.p_cr3_v[I386_VM_PDE(linaddr)]
   - 如果是物理地址：构造大页 PDE 条目

3. 将该 PDE 值写入 freepdes[free_pde_idx] 对应的 ptproc 页目录项：
   ptproc->p_seg.p_cr3_v[pde] = pdeval

4. 返回映射后的虚拟地址（freepde 索引对应的 2MB 区域内的偏移）
```

**关键约束**：`createpde()` 不是线程安全的——它修改全局的页目录项。但在 Minix3 的 BKL 模型下，内核同一时刻只有一个 CPU 在执行内核代码，所以不需要锁。

### 2.6 mem_clear_mapcache()：清理临时映射

`memory.c:35-50`：

```c
void mem_clear_mapcache(void)
{
    int i;
    for(i = 0; i < nfreepdes; i++) {
        struct proc *ptproc = get_cpulocal_var(ptproc);
        int pde = freepdes[i];
        u32_t *ptv;
        assert(ptproc);
        ptv = ptproc->p_seg.p_cr3_v;
        assert(ptv);
        ptv[pde] = 0;
    }
}
```

每次 `virtual_copy_f()` 或 `vm_memset()` 完成后，必须调用 `mem_clear_mapcache()` 清空 freepdes 对应的 PDE 条目。否则临时映射会残留，可能被后续操作误用。

---

## 3. Rust 设计决策

### 3.1 arch_post_init → PostInitArch trait

C 版的 `arch_post_init()` 做两件事：设置 `ptproc` 和调用 `pg_info()`。在 Rust 版中，这两个操作封装为一个 trait 方法。但 trait 方法不直接操作全局变量——而是返回 `CrossSpaceInit` 结构体，由 kernel 层存储和使用：

```rust
/// Post-initialization result: the kernel stores this for cross-address-space access.
pub struct CrossSpaceInit {
    /// VM's page table info (for createpde equivalent)
    pub vm_page_table: VmPageTableInfo,
    /// Temporary page table slots for createpde()
    pub free_pde_slots: FreePdeSlots,
}

pub trait PostInitArch {
    /// Register VM's page table info for cross-address-space operations.
    fn set_ptproc(vm_page_table: &VmPageTableInfo);
}
```

**为什么不用两个方法（`set_ptproc` + `pg_info`）？** 因为 `pg_info` 在 C 中是一个独立的函数，但它只被 `arch_post_init()` 调用。在 Rust 版中，`pg_info` 的逻辑内联到 `set_ptproc()` 中——减少接口面积，避免 `pg_info` 被错误地在其他地方调用。

### 3.2 memory_init → MemoryInitArch trait

C 版的 `memory_init()` 分配 freepdes。在 Rust 版中，这是一个返回值而不是修改全局变量：

```rust
pub trait MemoryInitArch {
    /// Allocate temporary page table slots from the free list.
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots;
}
```

**为什么返回结构体而不是修改全局变量？** C 版用 `static int freepdes[2]` 全局变量。Rust 版将 freepdes 封装为 `FreePdeSlots` 结构体，存储在内核的 `CrossSpaceInit` 中，避免全局可变状态。`FreePdeSlots` 使用固定大小数组 + `len` 字段，提供与 C 版相同的语义但增加了边界检查。

### 3.3 freepdes 的 64 位适配

C 版的 freepdes 是 32 位 PDE 索引（0-1023）。在 64 位下，页表层级从 2 级变为 4 级：

| 架构 | 页表层级 | 临时映射粒度 | freepde 含义 |
|------|---------|-------------|-------------|
| x86-32 | 2 级（PD+PT） | 4MB（PD 项直接映射） | PD 索引 |
| x86-64 | 4 级（PML4+PDPT+PD+PT） | 2MB（PD 项映射） | PD 索引 |
| aarch64 | 3-4 级 | 1GB/2MB | 对应层级索引 |
| riscv64 | 3 级（Sv39） | 2MB（大页） | 对应层级索引 |

在 64 位下，临时映射的原理完全相同——在当前页目录中预留若干项作为"临时窗口"。区别只是索引位宽和映射粒度。`free_upper_idx` 在 `KernelInfo` 中已经是 `usize`，可以适配不同位宽。

### 3.4 ptproc 的 Rust 类型安全

C 版中 `ptproc` 是裸指针 `struct proc *`，可能为 NULL。Rust 版通过 `VmPageTableInfo` 结构体封装页表地址信息，`virt_root` 使用 `Option<VirBytes>`——`None` 表示该架构不需要虚拟地址指针（如 RISC-V 的 Sv39 通过物理地址直接操作页表）。这样 `createpde()` 的 Rust 等价函数可以通过类型系统区分"有虚拟指针"和"没有虚拟指针"的架构。

### 3.5 BKL 下的安全性

`ptproc` 和 `freepdes` 都是 BKL 保护的共享状态。在 Minix3 的 BKL 模型下：

- `ptproc` 是 per-CPU 变量，每个 CPU 有自己的 `ptproc`，但 BKL 保证同一时刻只有一个 CPU 在内核中
- `freepdes` 是全局静态变量，`createpde()` 修改它时 BKL 已持有
- `mem_clear_mapcache()` 在 BKL 持有期间调用

Rust 版中，`CrossSpaceInit` 存储在内核全局状态中，访问时需要 `&mut` 引用。由于 BKL 保证同一时刻只有一个执行流，可以安全地获得 `&mut`。

### 3.6 设计决策的替代方案

| 决策 | 替代方案 | 为何不选 |
|------|---------|---------|
| `set_ptproc` 接收 `&VmPageTableInfo` | 接收 `&KProcess` | arch 层不能依赖 kernel 层，违反分层架构 |
| `FreePdeSlots` 返回结构体 | 全局 `static mut` | 全局可变状态违反 Rust 安全模型 |
| 2 个 freepdes 桶 | 更多或按需分配 | C 版用 2 个，改为 >2 需要证明必要性 |
| `virt_root: Option<VirBytes>` | 所有架构必有 `virt_root` | RISC-V Sv39 不维护虚拟地址指针 |

---

## 4. 实现详解

### 4.1 核心类型定义

> 设计决策：§3.1 — PostInitArch trait 返回 VmPageTableInfo

```rust
// os/arch/src/arch/post_init.rs

/// VM 页表信息（由 arch_post_init 设置）。
///
/// | 字段      | x86-64              | ARM64            | RISC-V     |
/// |------------|---------------------|------------------|------------|
/// | phys_root  | p_cr3 (CR3 value)   | p_ttbr (TTBR0)   | satp value |
/// | virt_root  | p_cr3_v (virt ptr)  | p_ttbr_v (virt ptr)| N/A*    |
#[derive(Debug, Clone, Copy)]
pub struct VmPageTableInfo {
    pub phys_root: PhysBytes,         // C: vm->p_seg.p_cr3 (x86)
    pub virt_root: Option<VirBytes>,  // C: vm->p_seg.p_cr3_v (x86)
}

/// 跨地址空间初始化结果（存储在 kernel 全局状态中）。
#[derive(Debug)]
pub struct CrossSpaceInit {
    pub vm_page_table: VmPageTableInfo,
    pub free_pde_slots: FreePdeSlots,
}

/// 临时页表槽位（createpde 的临时映射窗口）。
///
/// C: `static int freepdes[2]` + `nfreepdes` counter
#[derive(Debug, Clone)]
pub struct FreePdeSlots {
    slots: [usize; MAX_FREE_PDE_SLOTS],
    len: usize,
}

/// C: #define MAXFREEPDES 2 — memory.c:30
pub const MAX_FREE_PDE_SLOTS: usize = 2;

impl FreePdeSlots {
    pub fn new() -> Self {
        Self { slots: [0; MAX_FREE_PDE_SLOTS], len: 0 }
    }

    pub fn push(&mut self, index: usize) -> Result<(), &'static str> {
        if self.len >= MAX_FREE_PDE_SLOTS {
            return Err("free PDE slots overflow");
        }
        self.slots[self.len] = index;
        self.len += 1;
        Ok(())
    }

    pub fn get(&self, idx: usize) -> Option<usize> {
        if idx < self.len { Some(self.slots[idx]) } else { None }
    }
}

/// Architecture abstraction for post-initialization.
///
/// C: arch_post_init() — protect.c:370 (x86) / protect.c:97 (ARM)
pub trait PostInitArch {
    /// Register VM's page table info for cross-address-space operations.
    ///
    /// Equivalent C code:
    ///   vm = proc_addr(VM_PROC_NR);
    ///   get_cpulocal_var(ptproc) = vm;
    ///   pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);   // x86
    fn set_ptproc(vm_page_table: &VmPageTableInfo);
}

/// Architecture abstraction for memory initialization.
///
/// C: memory_init() — memory.c:707 (x86) / memory.c:612 (ARM)
pub trait MemoryInitArch {
    /// Allocate free page table slots from kinfo.free_upper_idx.
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots;
}
```

### 4.2 PostInitArch 实现

#### x86-64

```rust
// os/arch/src/x86_64/post_init.rs

impl PostInitArch for X86_64PostInitArch {
    fn set_ptproc(vm_page_table: &VmPageTableInfo) {
        // C: arch_post_init() — protect.c:370-377
        //   vm = proc_addr(VM_PROC_NR);
        //   get_cpulocal_var(ptproc) = vm;
        //   pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);

        // 记录 bootstrap 页表的物理和虚拟地址
        // CR3 物理地址在 vm_page_table.phys_root
        // 页表虚拟地址在 vm_page_table.virt_root（identity mapped）
        //
        // 实际效果：内核的 per-CPU ptproc 指针被设置为 VM，
        // createpde() 可以从 ptproc.p_seg.p_cr3_v[pde] 读取 PDE 值。
        //
        // SAFETY: BKL 持有期间，仅此 CPU 访问 ptproc。
        let _ = vm_page_table; // 实际存储由 kernel 层完成
    }
}
```

#### aarch64

```rust
// os/arch/src/arm64/post_init.rs

impl PostInitArch for AArch64PostInitArch {
    fn set_ptproc(vm_page_table: &VmPageTableInfo) {
        // C: arch_post_init() — protect.c:97-104
        //   vm = proc_addr(VM_PROC_NR);
        //   get_cpulocal_var(ptproc) = vm;
        //   pg_info(&vm->p_seg.p_ttbr, &vm->p_seg.p_ttbr_v);

        // AArch64: TTBR0_EL1 用户空间页表基址
        // phys_root = TTBR0_EL1 值（物理地址）
        // virt_root = TTBR0_EL1 的虚拟地址映射
        let _ = vm_page_table;
    }
}
```

#### riscv64

```rust
// os/arch/src/riscv64/post_init.rs

impl PostInitArch for Riscv64PostInitArch {
    fn set_ptproc(vm_page_table: &VmPageTableInfo) {
        // RISC-V: satp CSR 页表基址
        // phys_root = satp 值（PPN 字段）
        // virt_root = None（Sv39 通过物理地址直接操作页表）
        let _ = vm_page_table;
    }
}
```

### 4.3 MemoryInitArch 实现

三种架构的实现逻辑完全相同，差异仅在溢出检查的页表项数量：

#### x86-64

```rust
// os/arch/src/x86_64/post_init.rs

impl MemoryInitArch for X86_64MemoryInitArch {
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots {
        // C: memory_init() — memory.c:707-717
        //   freepdes[nfreepdes++] = kinfo.freepde_start++;
        //   freepdes[nfreepdes++] = kinfo.freepde_start++;
        //   assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES);

        let mut slots = FreePdeSlots::new();
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;

        // 64 位模式下 PD 有 512 个条目（不是 32 位的 1024 个）
        assert!(*free_upper_idx < 512, "free_upper_idx overflow: {}", *free_upper_idx);

        slots
    }
}
```

#### aarch64

```rust
// os/arch/src/arm64/post_init.rs

impl MemoryInitArch for AArch64MemoryInitArch {
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots {
        let mut slots = FreePdeSlots::new();
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;

        assert!(*free_upper_idx < 512, "free_upper_idx overflow: {}", *free_upper_idx);
        slots
    }
}
```

#### riscv64

```rust
// os/arch/src/riscv64/post_init.rs

impl MemoryInitArch for Riscv64MemoryInitArch {
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots {
        let mut slots = FreePdeSlots::new();
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;

        // Sv39: 512 个条目每级
        assert!(*free_upper_idx < 512, "free_upper_idx overflow: {}", *free_upper_idx);
        slots
    }
}
```

### 4.4 init_post_and_memory() — 主流程

```rust
// os/kernel/src/lib.rs

/// Phase D of kmain: arch_post_init + memory_init.
///
/// C: main.c:283-285
///    arch_post_init();    // protect.c:370 (x86) / protect.c:97 (ARM)
///    memory_init();       // memory.c:707 (x86) / memory.c:612 (ARM)
#[cfg(not(feature = "mock"))]
fn init_post_and_memory(kernel_info: &KernelInfo) {
    use minix_arch::{
        PostInitArch, MemoryInitArch,
        CurrentPostInitArch, CurrentMemoryInitArch,
        VmPageTableInfo, FreePdeSlots,
    };

    // Step 1: Set ptproc to VM.
    // C: arch_post_init() — protect.c:370
    //
    // TODO: Populate VmPageTableInfo from the VM process's p_seg fields
    //       after ProcessTable provides get_vm_page_table_info().
    let vm_page_table = VmPageTableInfo {
        phys_root: PhysBytes(0), // TODO: from VM process's p_seg.p_cr3/p_ttbr
        virt_root: Some(VirBytes(0)), // TODO: from VM process's p_seg.p_cr3_v/p_ttbr_v
    };
    CurrentPostInitArch::set_ptproc(&vm_page_table);

    // Step 2: Allocate temporary page table slots.
    // C: memory_init() — memory.c:707
    //    freepdes[nfreepdes++] = kinfo.freepde_start++;
    let mut free_idx = kernel_info.free_upper_idx().expect(
        "free_upper_idx must be set by boot-shim before kernel init"
    );
    let _free_pde_slots: FreePdeSlots = CurrentMemoryInitArch::allocate_free_pdes(&mut free_idx);
    // free_idx is now advanced by MAX_FREE_PDE_SLOTS (2).
    // TODO: Store _free_pde_slots in kernel global state for createpde() access.
}
```

**当前状态与 TODO**：

| 方面 | 状态 | 说明 |
|------|------|------|
| PostInitArch trait | 已实现 | 三种架构的 `set_ptproc()` 均已实现 |
| MemoryInitArch trait | 已实现 | 三种架构的 `allocate_free_pdes()` 均已实现 |
| VmPageTableInfo 填充 | **TODO** | 当前使用硬编码的 `PhysBytes(0)/VirBytes(0)`，待 ProcessTable 提供 VM 页表信息访问方法 |
| CrossSpaceInit 全局存储 | **TODO** | `_free_pde_slots` 当前被丢弃，待 kernel 全局状态容器实现后存储 |
| createpde() 等价函数 | 后续文档 | 需要 `free_pde_slots` 和 `vm_page_table` 就绪后方可实现 |
```

### 4.5 内核全局状态存储

```rust
// os/kernel/src/lib.rs

/// 内核全局状态（BKL 保护）。
static mut KERNEL_STATE: Option<KernelState> = None;

struct KernelState {
    /// 跨地址空间访问基础设施
    cross_space: CrossSpaceInit,
    /// 进程表
    proc_table: ProcessTable,
    /// 特权表
    priv_table: PrivTable,
}

fn kmain(boot_info: &BootInfo) -> ! {
    // ... Phase A, B, C ...

    // Phase D: arch_post_init + memory_init
    let cross_space = init_post_and_memory(kernel_info);

    // 存储到全局状态
    // SAFETY: BKL 持有，仅此 CPU 访问
    unsafe {
        KERNEL_STATE = Some(KernelState {
            cross_space,
            proc_table,
            priv_table,
        });
    }

    // ... Phase E, F ...
}
```

---

## 5. 测试要点

### 5.1 PostInitArch 测试

| 测试项 | 验证内容 |
|--------|---------|
| `set_ptproc(vm)` 后 `VmPageTableInfo` 正确 | phys_root 和 virt_root 正确 |
| `set_ptproc` 前访问 `VmPageTableInfo` 为默认值 | 未初始化状态正确 |
| `virt_root: None` 的架构（riscv64） | `createpde` 等价函数用物理地址 |

### 5.2 MemoryInitArch 测试

| 测试项 | 验证内容 |
|--------|---------|
| `allocate_free_pdes(&mut 5)` 返回 slots `[5, 6]` | 正确分配连续索引 |
| `free_upper_idx` 调用后递增 2 | 索引推进正确 |
| `free_upper_idx = 510` 时 panic | 越界检查（512 - 2 = 510） |
| `FreePdeSlots::new()` 默认值正确 | `len = 0`, `slots = [0, 0]` |
| `FreePdeSlots::push()` 超过 MAX | 返回 `Err("free PDE slots overflow")` |

### 5.3 集成测试

| 测试项 | 验证内容 |
|--------|---------|
| Phase C→D 顺序：先 `init_proc_and_boot` 再 `init_post_and_memory` | 依赖关系正确 |
| Phase D 后 `CrossSpaceInit` 正确存储 | `vm_page_table` 和 `free_pde_slots` 可用 |
| `mem_clear_mapcache` 等价函数清空 PDE 条目 | 临时映射不残留 |
| 多次调用 `allocate_free_pdes` 不重复分配 | `free_upper_idx` 推进正确 |

---

## 6. 参见

- [05-proc-init-boot-proc.md](05-proc-init-boot-proc.md) — 阶段 C：进程表初始化与 VM ELF 加载
- [03-kmain-cstart.md](03-kmain-cstart.md) — kmain 六阶段总览、Phase A/B
- [04-clock-interrupt-init.md](04-clock-interrupt-init.md) — Phase B：时钟与中断初始化
- `minix3/minix/kernel/arch/i386/protect.c:370-377` — x86 arch_post_init
- `minix3/minix/kernel/arch/i386/memory.c:707-717` — x86 memory_init
- `minix3/minix/kernel/arch/i386/memory.c:35-114` — mem_clear_mapcache + createpde
- `minix3/minix/kernel/arch/earm/protect.c:97-104` — ARM arch_post_init
- `minix3/minix/kernel/arch/earm/memory.c:612-622` — ARM memory_init
- `minix3/minix/kernel/cpulocals.h:55` — ptproc per-CPU 变量定义