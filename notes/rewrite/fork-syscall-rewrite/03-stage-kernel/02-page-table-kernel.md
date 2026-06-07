# 02-page-table-kernel: 内核运行时页表操作

> **分类**: Kernel 运行时页表管理
> **源码**: `minix3/minix/kernel/arch/i386/memory.c`(1020行), `arch_do_vmctl.c`(67行), `pg_utils.c`(317行)
> **说明**: 分页开启后，内核如何管理进程页表——跨地址空间操作（Direct Map 替代临时 PDE 映射）、VM 通过 SYS_VMCTL 管理页表、内核与 VM 的地址空间协作机制。自举阶段的页表建立见 01-multiboot-bootstrap。

---

## 1. 概述

### 1.1 内核页表操作解决什么问题

Minix3 是微内核架构：内核不管理页表，**VM 进程**才是页表的真正管理者。但内核有时必须访问进程的地址空间——比如执行 `sys_copy` 在两个进程之间拷贝数据、或者 `sys_memset` 清零一段内存。问题是：内核运行在 VM 的地址空间中，而目标进程的地址空间对内核不可见。

内核页表操作的核心机制就是**临时映射**：在当前页目录中借用若干空闲 PDE 槽位，将目标进程的页目录条目（或物理内存的大页条目）临时写入这些槽位，使得内核可以通过这些"窗口"访问原本不可见的 4MB 地址范围。操作完成后，清除这些临时映射。

> 一句话总结：**内核借用当前页目录的空闲 PDE，临时映射目标进程的 4MB 地址窗口，实现跨地址空间的数据访问。**

### 1.2 与 Minix3 的对应关系

| Minix3 组件 | 文件 | 职责 |
|------------|------|------|
| `createpde()` | `memory.c:69` | 核心映射函数：在当前页目录中创建临时 PDE 条目 |
| `lin_lin_copy()` | `memory.c:149` | 跨地址空间的线性地址到线性地址拷贝 |
| `vm_memset()` | `memory.c:526` | 跨地址空间的内存清零 |
| `virtual_copy_f()` | `memory.c:592` | 通用虚拟拷贝，支持 VMSUSPEND 挂起 |
| `vm_lookup()` | `memory.c:325` | 查询进程虚拟地址对应的物理地址 |
| `vm_lookup_range()` | `memory.c:377` | 查询连续物理内存的范围 |
| `vm_check_range()` | `memory.c:427` | 委托 VM 检查地址范围合法性 |
| `arch_do_vmctl()` | `arch_do_vmctl.c:38` | VM 通过 SYS_VMCTL 设置进程 CR3/刷新 TLB |
| `memory_init()` | `memory.c:707` | 初始化 freepdes 槽位 |
| `arch_enable_paging()` | `memory.c:940` | VM 进程分页使能后的地址切换 |
| `arch_phys_map()` | `memory.c:746` | 声明内核需要的物理内存映射（APIC/video 等） |
| `arch_phys_map_reply()` | `memory.c:847` | VM 完成映射后回传虚拟地址 |

### 1.3 关键状态与机制

#### 1.3.1 freepdes：内核的临时 PDE 槽位

内核在当前页目录中保留 **2 个**空闲 PDE 条目（`freepdes[0]` 和 `freepdes[1]`），用于临时映射。这两个槽位在 `memory_init()` 中从 `kinfo.freepde_start` 分配，而 `freepde_start` 由 `pg_mapkernel()` 返回——即内核高地址映射之后的第一个空闲 PDE。

```c
// memory.c:29-31
static int nfreepdes = 0;
#define MAXFREEPDES 2
static int freepdes[MAXFREEPDES];
```

- `freepdes[0]`：用于源地址映射（`createpde` 的 `free_pde_idx=0`）
- `freepdes[1]`：用于目标地址映射（`createpde` 的 `free_pde_idx=1`）

这样 `lin_lin_copy` 可以同时映射源和目标两个 4MB 窗口。

#### 1.3.2 ptproc：当前页目录所属进程

`ptproc` 是 per-CPU 变量，指向**当前 CR3 所对应的进程**。内核运行时，`ptproc` 通常是 VM 进程（因为内核借用 VM 的地址空间）。`createpde()` 操作的就是 `ptproc->p_seg.p_cr3_v` 指向的页目录。

```c
// protect.c:375-376 — arch_post_init() 中设置
get_cpulocal_var(ptproc) = vm;
pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);
```

#### 1.3.3 segframe：进程的页表描述

每个进程的 `p_seg`（类型 `segframe_t`）包含两个关键字段：

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_cr3` | `reg_t`（即 `u32_t`） | 页目录的物理地址，写入 CR3 寄存器 |
| `p_cr3_v` | `u32_t *` | 页目录的虚拟地址，内核通过它读写页目录内容 |

`p_cr3` 和 `p_cr3_v` 的关系：`p_cr3` 是物理地址（硬件使用），`p_cr3_v` 是对应的内核虚拟地址（软件读写）。在 Minix3 的 1:1 恒等映射下，两者数值相同；但在 minix-rs 的 64 位高半核内核中，两者不同。

#### 1.3.4 HASPT：进程是否有页表

```c
// memory.c:28
#define HASPT(procptr) ((procptr)->p_seg.p_cr3 != 0)
```

内核进程（如 IDLE、CLOCK）没有独立页表（`p_cr3 == 0`），它们共享内核的地址空间。用户进程和系统服务进程（如 VM、PM）有自己的页表。

#### 1.3.5 VMSUSPEND：缺页挂起机制

当 `lin_lin_copy` 或 `vm_memset` 在访问进程内存时发生缺页，内核不能自行处理——它必须挂起当前操作，请求 VM 进程处理缺页，等 VM 回复后再继续。这个挂起状态用 `VMSUSPEND`（-996）返回值表示。

挂起信息保存在 `caller->p_vmrequest` 中，包含目标进程、缺页地址、操作类型等。VM 处理完毕后，内核通过 `check_resumed_caller()` 检查恢复结果。

### 1.4 行为规则

1. **临时映射是 4MB 粒度**：每次 `createpde` 映射一个 4MB 大页窗口（32 位 x86 的 PDE 大页模式）
2. **映射前先检查快捷路径**：如果目标进程就是 `ptproc`（当前页目录所属进程）或者是内核进程，则无需映射，直接使用线性地址
3. **映射后必须刷新 TLB**：修改 PDE 后调用 `reload_cr3()` 刷新 TLB，确保新映射生效
4. **操作完成后清除映射**：`mem_clear_mapcache()` 清零所有 freepdes 对应的 PDE 条目
5. **VM 是页表的唯一管理者**：内核不主动修改进程页表，只通过临时映射读取；修改进程页表必须通过 `SYS_VMCTL` 让 VM 执行
6. **缺页时挂起而非崩溃**：内核访问进程内存时发生缺页，不会 panic，而是通过 `vm_suspend()` 挂起操作，等 VM 处理

---

## 2. C 源码分析

### 2.1 相关定义

#### 2.1.1 页表常量（`arch/i386/include/vm.h`）

| 常量 | 值 | 含义 |
|------|-----|------|
| `I386_PAGE_SIZE` | 4096 | 普通 4KB 页大小 |
| `I386_BIG_PAGE_SIZE` | 4MB (`4096 × 1024`) | 大页大小，PDE 大页模式 |
| `I386_VM_DIR_ENTRIES` | 1024 | 页目录条目数 |
| `I386_VM_PT_ENTRIES` | 1024 | 页表条目数 |
| `I386_VM_DIR_ENT_SHIFT` | 22 | 页目录索引移位 |
| `I386_VM_PT_ENT_SHIFT` | 12 | 页表索引移位 |
| `I386_VM_PT_ENT_MASK` | 0x3FF | 页表索引掩码 |

#### 2.1.2 PDE/PTE 标志位（`arch/i386/include/vm.h`）

| 标志 | 值 | 含义 | 适用 |
|------|-----|------|------|
| `I386_VM_PRESENT` | 0x001 | 页存在 | PDE + PTE |
| `I386_VM_WRITE` | 0x002 | 可写 | PDE + PTE |
| `I386_VM_USER` | 0x004 | 用户可访问 | PDE + PTE |
| `I386_VM_PWT` | 0x008 | 写穿透 | PDE + PTE |
| `I386_VM_PCD` | 0x010 | 禁用缓存 | PDE + PTE |
| `I386_VM_ACC` | 0x020 | 已访问 | PDE + PTE |
| `I386_VM_BIGPAGE` | 0x080 | 4MB 大页 | PDE only |
| `I386_VM_DIRTY` | bit 6 | 已修改 | PTE only |
| `I386_VM_GLOBAL` | bit 8 | 全局页 | PTE only |

#### 2.1.3 地址掩码

| 掩码 | 值 | 用途 |
|------|-----|------|
| `I386_VM_ADDR_MASK` | 0xFFFFF000 | 4KB 页物理地址掩码 |
| `I386_VM_ADDR_MASK_4MB` | 0xFFC00000 | 4MB 大页物理地址掩码 |
| `I386_VM_OFFSET_MASK_4MB` | 0x003FFFFF | 4MB 大页页内偏移掩码 |

#### 2.1.4 地址分解宏

```c
#define I386_VM_PDE(v)  ((v) >> I386_VM_DIR_ENT_SHIFT)    // 取 PDE 索引
#define I386_VM_PTE(v)  (((v) >> I386_VM_PT_ENT_SHIFT) & I386_VM_PT_ENT_MASK) // 取 PTE 索引
#define I386_VM_PFA(e)  ((e) & I386_VM_ADDR_MASK)         // 取页帧地址
```

#### 2.1.5 VMCTL 命令（`minix/com.h`）

| 命令 | 值 | 含义 |
|------|-----|------|
| `VMCTL_GET_PDBR` | 13 | 获取进程 CR3（页目录基址） |
| `VMCTL_SETADDRSPACE` | 29 | 设置进程地址空间（CR3 + 虚拟地址） |
| `VMCTL_FLUSHTLB` | 26 | 刷新整个 TLB（重载 CR3） |
| `VMCTL_I386_INVLPG` | 25 | 刷新单个页的 TLB 条目 |

#### 2.1.6 映射标志（`minix/com.h`）

| 标志 | 值 | 含义 |
|------|-----|------|
| `VMMF_UNCACHED` | bit 0 | 不缓存（MMIO） |
| `VMMF_USER` | bit 1 | 用户可访问 |
| `VMMF_WRITE` | bit 2 | 可写 |
| `VMMF_GLO` | bit 3 | 全局页 |

#### 2.1.7 VM 挂起类型（`kernel/proc.h`）

| 类型 | 值 | 含义 |
|------|-----|------|
| `VMSTYPE_SYS_NONE` | 0 | 无挂起 |
| `VMSTYPE_KERNELCALL` | 1 | 内核调用中挂起 |
| `VMSTYPE_DELIVERMSG` | 2 | 消息投递中挂起 |
| `VMSTYPE_MAP` | 3 | 映射操作中挂起 |

### 2.2 核心数据结构

#### 2.2.1 segframe_t — 进程页表描述（`arch/i386/include/archtypes.h:32`）

```c
typedef struct segframe {
    reg_t   p_cr3;          // 页目录物理地址（写入 CR3）
    u32_t  *p_cr3_v;        // 页目录虚拟地址（内核读写）
    char   *fpu_state;      // FPU 状态保存区
    int     p_kern_trap_style; // 内核陷阱风格
} segframe_t;
```

**字段说明**：
- `p_cr3`：页目录的物理地址。当进程被调度时，此值写入 CR3 寄存器。`p_cr3 == 0` 表示该进程没有独立页表（`HASPT` 宏据此判断）
- `p_cr3_v`：页目录的虚拟地址。内核通过此指针直接读写页目录内容。在 `arch_do_vmctl` 的 `VMCTL_SETADDRSPACE` 中设置
- `fpu_state`：FPU/SSE 状态保存区，与页表操作无关
- `p_kern_trap_style`：内核陷阱入口风格，与页表操作无关

**ARM 对比**：ARM 架构的 `segframe_t` 使用 `p_ttbr`/`p_ttbr_v`（对应 TTBR0/TTBR1 寄存器），无 `p_kern_trap_style` 字段。

**字段覆盖说明**：`segframe_t` 的 4 个字段中，`p_cr3`/`p_cr3_v` 是页表核心字段（§3.4 详细映射），`fpu_state` 和 `p_kern_trap_style` 与页表操作无关，在 minix-rs 中分别移到 `KProcess` 的其他字段中（见 03-vm-request.md §3.2 字段映射表）。

#### 2.2.2 vir_addr — 虚拟地址描述（`minix/type.h:27`）

```c
struct vir_addr {
    endpoint_t proc_nr_e;   // NONE 表示物理地址，否则为进程端点号
    vir_bytes  offset;       // 线性地址偏移
};
```

这是 `virtual_copy_f()` 和 `data_copy()` 的参数类型。`proc_nr_e == NONE` 时 `offset` 被解释为物理地址。

#### 2.2.3 p_vmrequest — VM 挂起请求（`kernel/proc.h:87-124`）

```c
struct {
    struct proc  *nextrestart;   // vmrestart 链表下一个
    struct proc  *nextrequestor; // vmrequest 链表下一个
    int           type;          // VMSTYPE_KERNELCALL 等
    union ixfer_saved {
        message reqmsg;          // 保存的请求消息
    } saved;
    int           req_type;      // 请求类型
    endpoint_t    target;        // 目标进程
    union ixfer_params {
        struct {
            vir_bytes start, length; // 内存范围
            u8_t      writeflag;    // 非零表示写操作
        } check;
    } params;
    int           vmresult;      // VM 处理结果
} p_vmrequest;
```

当 `lin_lin_copy` 因缺页返回 `EFAULT_SRC`/`EFAULT_DST` 时，`virtual_copy_f` 将缺页信息填入 `caller->p_vmrequest`，然后调用 `vm_suspend()` 挂起调用者。VM 处理完毕后，`vmresult` 被设置为 VM 的返回值，内核通过 `check_resumed_caller()` 读取。

#### 2.2.4 pagedir — 内核启动页目录（`pg_utils.c:19`）

```c
static u32_t pagedir[1024] __aligned(4096);
```

这是启动阶段使用的静态页目录。`pg_identity()` 和 `pg_mapkernel()` 直接写入此数组。启动完成后，`pg_info()` 将其物理地址和虚拟地址传给 VM 进程，VM 接管此页目录作为自己的地址空间。

#### 2.2.5 freepdes — 临时 PDE 槽位数组（`memory.c:29-31`）

```c
static int nfreepdes = 0;
#define MAXFREEPDES 2
static int freepdes[MAXFREEPDES];
```

存储 2 个空闲 PDE 索引号。`memory_init()` 从 `kinfo.freepde_start` 分配，`createpde()` 使用，`mem_clear_mapcache()` 清除。

### 2.3 关键函数分析

#### 2.3.1 createpde() — 临时 PDE 映射（`memory.c:69`）

这是内核页表操作的核心函数。它将目标进程的某个 4MB 地址窗口映射到当前页目录的一个空闲 PDE 中。

```c
static phys_bytes createpde(
    const struct proc *pr,       // 目标进程，NULL 表示物理地址
    const phys_bytes linaddr,    // 目标线性地址
    phys_bytes *bytes,           // [in/out] 请求字节数，可能被截断
    int free_pde_idx,            // 使用哪个 freepde 槽位（0 或 1）
    int *changed                 // [out] 是否修改了 PDE
)
```

**执行流程**：

1. **快捷路径**：如果 `pr` 是 `ptproc`（当前页目录进程）或内核进程，直接返回 `linaddr`——无需映射
2. **进程映射**：如果 `pr` 非空，从 `pr->p_seg.p_cr3_v[I386_VM_PDE(linaddr)]` 读取目标 PDE 值
3. **物理映射**：如果 `pr` 为 NULL，构造大页 PDE 值：`(linaddr & ADDR_MASK_4MB) | BIGPAGE | PRESENT | WRITE | USER`
4. **写入临时 PDE**：将 PDE 值写入 `ptproc->p_seg.p_cr3_v[pde]`（`pde = freepdes[free_pde_idx]`）
5. **截断字节数**：`*bytes = MIN(*bytes, I386_BIG_PAGE_SIZE - offset)`，确保不超过 4MB 窗口
6. **返回映射地址**：`I386_BIG_PAGE_SIZE * pde + offset`

**关键细节**：
- 步骤 2 中读取的是目标进程的 PDE（可能指向一个页表），写入当前页目录后，当前地址空间就能通过该 PDE 访问目标进程的 4MB 范围
- 步骤 3 中物理地址映射使用大页模式，因为物理内存不需要二级页表
- 如果 PDE 值未变化（`ptproc->p_seg.p_cr3_v[pde] == pdeval`），不设置 `*changed`，避免不必要的 TLB 刷新

#### 2.3.2 lin_lin_copy() — 跨地址空间拷贝（`memory.c:149`）

```c
static int lin_lin_copy(
    struct proc *srcproc,      // 源进程，NULL 表示物理地址
    vir_bytes srclinaddr,      // 源线性地址
    struct proc *dstproc,      // 目标进程，NULL 表示物理地址
    vir_bytes dstlinaddr,      // 目标线性地址
    vir_bytes bytes            // 拷贝字节数
)
```

**执行流程**：

1. 断言 `ptproc` 存在且 CR3 一致
2. 循环处理，每次迭代：
   a. 调用 `createpde(srcproc, srclinaddr, &chunk, 0, &changed)` 映射源 4MB 窗口
   b. 调用 `createpde(dstproc, dstlinaddr, &chunk, 1, &changed)` 映射目标 4MB 窗口
   c. 如果 `changed`，调用 `reload_cr3()` 刷新 TLB
   d. 执行 `PHYS_COPY_CATCH` 物理拷贝（带缺页捕获）
   e. 如果捕获到缺页，返回 `EFAULT_SRC` 或 `EFAULT_DST`
   f. 更新地址和剩余字节数
3. 全部拷贝完成返回 `OK`

**关键细节**：
- `chunk` 被 `createpde` 截断为 4MB 窗口内剩余字节数，所以大块拷贝会分多次迭代
- `PHYS_COPY_CATCH` 是内联汇编宏，在页错误时将故障地址写入 `addr` 变量而非 panic
- SMP 模式下，检查 `p_stale_tlb` 位图，如果目标进程在其他 CPU 上有陈旧 TLB 条目，强制刷新

#### 2.3.3 vm_lookup() — 虚拟地址到物理地址查询（`memory.c:325`）

```c
int vm_lookup(
    const struct proc *proc,   // 目标进程
    const vir_bytes virtual,   // 虚拟地址
    phys_bytes *physical,      // [out] 物理地址
    u32_t *ptent               // [out] PTE 值（可选）
)
```

**执行流程**：

1. 断言进程有页表（`HASPT`）
2. 读取 PDE：`pde_v = phys_get32(root + pde)`——注意这里通过 `phys_get32`（即 `lin_lin_copy(NULL, addr, SYSTEM, &v, 4)`）间接读取，因为页目录可能不在当前地址空间
3. 如果 PDE 不存在（`!(pde_v & PRESENT)`），返回 `EFAULT`
4. 如果是大页 PDE（`pde_v & BIGPAGE`），物理地址 = `(pde_v & ADDR_MASK_4MB) + (virtual & OFFSET_MASK_4MB)`
5. 否则读取 PTE：`pte_v = phys_get32(pt + pte)`，物理地址 = `(pte_v & ADDR_MASK) + (virtual % PAGE_SIZE)`

**关键细节**：
- `phys_get32` 使用 `lin_lin_copy` 读取物理内存——这是一个递归依赖：`vm_lookup` → `phys_get32` → `lin_lin_copy` → `createpde`。但不会死循环，因为 `phys_get32` 传 `NULL`（物理地址）给 `lin_lin_copy`，而 `createpde` 对物理地址直接构造大页 PDE，不再调用 `vm_lookup`

#### 2.3.4 vm_lookup_range() — 连续物理内存范围查询（`memory.c:377`）

```c
size_t vm_lookup_range(
    const struct proc *proc,    // 目标进程
    vir_bytes vir_addr,         // 起始虚拟地址
    phys_bytes *phys_addr,      // [out] 起始物理地址
    size_t bytes                // 查询长度
)
```

逐页调用 `vm_lookup`，检查物理地址是否连续。返回连续的长度（可能小于 `bytes`）。`umap_virtual()` 使用此函数验证拷贝范围的物理连续性。

#### 2.3.5 vm_memset() — 跨地址空间清零（`memory.c:526`）

```c
int vm_memset(
    struct proc *caller,        // 调用者进程
    endpoint_t who,             // 目标进程端点，NONE 表示物理地址
    phys_bytes ph,              // 目标起始地址
    int c,                      // 填充字节
    phys_bytes count            // 字节数
)
```

与 `lin_lin_copy` 类似，但只映射一个方向（`free_pde_idx=0`）。使用 `phys_memset` 执行实际清零（带缺页捕获）。缺页时通过 `vm_suspend` 挂起。

#### 2.3.6 virtual_copy_f() — 通用虚拟拷贝（`memory.c:592`）

```c
int virtual_copy_f(
    struct proc *caller,        // 调用者（用于 VMSUSPEND）
    struct vir_addr *src_addr,  // 源虚拟地址
    struct vir_addr *dst_addr,  // 目标虚拟地址
    vir_bytes bytes,            // 字节数
    int vmcheck                 // 非零表示允许 VMSUSPEND
)
```

这是 `sys_vircopy` 等系统调用的底层实现。它将 `vir_addr` 解析为 `proc` 指针，然后调用 `lin_lin_copy`。如果 `lin_lin_copy` 返回缺页错误且 `vmcheck` 为真，则调用 `vm_suspend` 挂起调用者。

**关键路径**：
- `data_copy()` → `virtual_copy()` → `virtual_copy_f(vmcheck=0)` — 不允许挂起
- `data_copy_vmcheck()` → `virtual_copy_vmcheck()` → `virtual_copy_f(vmcheck=1)` — 允许挂起

#### 2.3.7 memory_init() — 初始化 freepdes（`memory.c:707`）

```c
void memory_init(void)
{
    freepdes[nfreepdes++] = kinfo.freepde_start++;  // slot 0
    freepdes[nfreepdes++] = kinfo.freepde_start++;  // slot 1
    assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES);
    assert(nfreepdes == 2);
}
```

从 `kinfo.freepde_start`（`pg_mapkernel()` 返回值）分配 2 个 PDE 索引。此后 `freepdes[0]` 和 `freepdes[1]` 分别用于源和目标映射。

#### 2.3.8 mem_clear_mapcache() — 清除临时映射（`memory.c:35`）

```c
void mem_clear_mapcache(void)
{
    for(i = 0; i < nfreepdes; i++) {
        int pde = freepdes[i];
        ptproc->p_seg.p_cr3_v[pde] = 0;  // 清零 PDE
    }
}
```

在进程切换前调用，清除所有临时 PDE 映射，防止下一个进程看到残留映射。

#### 2.3.8a umap_virtual() — 虚拟地址到物理地址映射（`memory.c:282`）

`umap_virtual` 是 `vm_lookup` + `vm_lookup_range` 的上层封装，验证目标虚拟地址范围的物理连续性：

```c
phys_bytes umap_virtual(struct proc *rp, int seg, vir_bytes vir_addr, vir_bytes bytes)
{
    phys_bytes phys = 0;
    if(vm_lookup(rp, vir_addr, &phys, NULL) != OK) return 0;
    if(phys == 0) panic("vm_lookup returned phys: 0");
    if(bytes > 0 && vm_lookup_range(rp, vir_addr, NULL, bytes) != bytes) return 0;
    return phys;
}
```

**流程**：先调用 `vm_lookup` 查询单个虚拟地址的物理地址，再调用 `vm_lookup_range` 验证整个范围的物理连续性。如果地址不连续（跨不连续物理页），返回 0 表示失败。

**与 minix-rs 的关系**：`umap_virtual` 的功能被 `lookup_in_table`（§4.4）替代——`lookup_in_table` 返回单个页的物理地址，跨页连续性由调用者（如 `cross_space_copy`）按页循环处理。minix-rs 不需要"物理连续性验证"这一步，因为 Direct Map 下每页独立映射，不需要连续物理内存。

#### 2.3.8b vm_check_range() — 委托 VM 检查地址范围（`memory.c:427`）

`vm_check_range` 是 VMSUSPEND 机制的内核入口，供内核调用使用。它代表调用者进程（`caller`）请求 VM 检查目标进程（`target`）的地址范围合法性：

```c
int vm_check_range(struct proc *caller, struct proc *target,
    vir_bytes vir_addr, size_t bytes, int writeflag)
{
    int r;
    if ((caller->p_misc_flags & MF_KCALL_RESUME) &&
            (r = caller->p_vmrequest.vmresult) != OK)
        return r;
    vm_suspend(caller, target, vir_addr, bytes, VMSTYPE_KERNELCALL, writeflag);
    return VMSUSPEND;
}
```

**流程**：
1. 如果调用者带有 `MF_KCALL_RESUME` 标志（表示从 VMSUSPEND 恢复），直接返回上次 VM 的检查结果
2. 否则调用 `vm_suspend` 挂起调用者，请求 VM 检查目标进程的地址范围
3. 返回 `VMSUSPEND`，调用者被挂起直到 VM 回复

**与 minix-rs 的关系**：`vm_check_range` 的语义由 `CrossSpaceResult::Suspended`（§4.3）+ `VmSuspendState`（03-vm-request.md §3.3）替代。当 `cross_space_copy` 发现缺页时，返回 `CrossSpaceResult::Suspended(VmFaultType)`，调用者（系统调用处理）根据 `vmcheck` 标志决定是否挂起。Minix3 的 `vm_suspend` 直接在内核中挂起调用者；minix-rs 将"发现缺页"和"挂起调用者"分离为两步，更清晰。注意：Minix3 的 `VMPTYPE_NONE`(0) 虽然在 `include/minix/vm.h:37` 中定义，但从未被使用——`vm_suspend()` 中硬编码 `req_type = VMPTYPE_CHECK`（见 03-vm-request.md §2.5.6），因此 minix-rs 不需要对应类型。

#### 2.3.9 arch_do_vmctl() — VM 页表管理命令（`arch_do_vmctl.c:38`）

VM 进程通过 `SYS_VMCTL` 系统调用向内核发送页表管理命令：

| 命令 | 操作 |
|------|------|
| `VMCTL_GET_PDBR` | 读取进程 `p_cr3`，返回页目录物理地址 |
| `VMCTL_SETADDRSPACE` | 设置进程 `p_cr3` 和 `p_cr3_v`，如果目标是 `ptproc` 则立即 `write_cr3`；如果目标是 VM 进程则调用 `arch_enable_paging` |
| `VMCTL_FLUSHTLB` | 调用 `reload_cr3()` 刷新整个 TLB |
| `VMCTL_I386_INVLPG` | 调用 `i386_invlpg()` 刷新单个页的 TLB 条目 |

`VMCTL_SETADDRSPACE` 的关键逻辑（`setcr3` 函数）：

1. 设置 `p->p_seg.p_cr3 = cr3` 和 `p->p_seg.p_cr3_v = v`
2. 如果 `p == ptproc`（当前地址空间进程），立即 `write_cr3(cr3)` 刷新
3. 如果 `p->p_nr == VM_PROC_NR`，调用 `arch_enable_paging(p)` 完成分页使能后的地址切换
4. 清除 `RTS_VMINHIBIT` 标志，使进程可被调度

#### 2.3.10 arch_enable_paging() — VM 分页使能（`memory.c:940`）

```c
int arch_enable_paging(struct proc *caller)
{
    switch_address_space(caller);  // 切换到 VM 的地址空间
    video_mem = (char *) video_mem_vaddr;  // 使用虚拟地址访问 video 内存
    // APIC 地址从物理切换到虚拟
    // 启动 watchdog（如果需要）
    return OK;
}
```

VM 进程第一次通过 `VMCTL_SETADDRSPACE` 设置自己的页表时调用。此时内核从启动阶段的 1:1 恒等映射切换到 VM 的真实地址空间，所有物理地址引用（video_mem、APIC）必须切换为虚拟地址。

#### 2.3.11 arch_phys_map() / arch_phys_map_reply() — 物理内存映射协商

`arch_phys_map()` 是内核向 VM 声明自己需要映射的物理内存区域（video 内存、APIC、usermapped 区域等）。VM 遍历所有索引，为每个区域创建映射，然后通过 `arch_phys_map_reply()` 回传虚拟地址。

**映射区域**：

| 索引 | 区域 | 标志 |
|------|------|------|
| `video_mem_mapping_index` | VGA 文本缓冲区 | `VMMF_WRITE` |
| `usermapped_glo_index` | 全局用户映射区（IPC 向量等） | `VMMF_USER \| VMMF_GLO` |
| `usermapped_index` | 非全局用户映射区 | `VMMF_USER` |
| `lapic_mapping_index` | Local APIC 寄存器 | `VMMF_UNCACHED \| VMMF_WRITE` |
| `ioapic_first_index..last_index` | I/O APIC 寄存器 | `VMMF_UNCACHED \| VMMF_WRITE` |
| `oxpcie_mapping_index` | Oxford PCIe 串口 | `VMMF_UNCACHED \| VMMF_WRITE` |

`arch_phys_map_reply()` 中最复杂的部分是 `first_um_idx`（usermapped 区域）的处理：它将内核数据结构（`kinfo`、`machine`、IPC 向量等）的指针从物理地址修正为虚拟地址，并设置 `minix_kerninfo` 供用户进程访问。

#### 2.3.12 release_address_space() — 释放地址空间（`memory.c:986`）

```c
void release_address_space(struct proc *pr)
{
    pr->p_seg.p_cr3_v = NULL;
}
```

进程终止时调用。仅清空 `p_cr3_v`，不清空 `p_cr3`（物理地址由 VM 负责释放）。

### 2.4 调用关系/调用点分析

#### 2.4.1 启动阶段

```
pre_init()
  → pg_clear()              // 清零 pagedir
  → pg_identity(&kinfo)     // 恒等映射所有 4MB 区域
  → pg_mapkernel()          // 内核高地址映射，返回 freepde_start
  → pg_load()               // write_cr3(pagedir 物理地址)
  → vm_enable_paging()      // CR0.PG=1, CR4.PSE=1, CR4.PGE=1

arch_post_init()
  → ptproc = VM 进程
  → pg_info(&vm->p_cr3, &vm->p_cr3_v)  // VM 接管 pagedir

memory_init()
  → freepdes[0] = kinfo.freepde_start++
  → freepdes[1] = kinfo.freepde_start++
```

#### 2.4.2 运行时跨地址空间访问

```
sys_vircopy / sys_physcopy
  → virtual_copy_f()
    → lin_lin_copy(srcproc, dstproc, ...)
      → createpde(srcproc, ..., 0, &changed)   // 映射源
      → createpde(dstproc, ..., 1, &changed)   // 映射目标
      → reload_cr3()                            // 刷新 TLB
      → PHYS_COPY_CATCH(...)                    // 带缺页捕获的拷贝
      → [缺页] → return EFAULT_SRC/DST
    → [缺页] → vm_suspend() → return VMSUSPEND

sys_memset
  → vm_memset()
    → createpde(whoptr, ..., 0, &new_cr3)
    → reload_cr3()
    → phys_memset(...)
    → [缺页] → vm_suspend() → return VMSUSPEND
```

#### 2.4.3 VM 页表管理

```
VM 进程 → SYS_VMCTL(VMCTL_SETADDRSPACE)
  → arch_do_vmctl()
    → setcr3(p, cr3, v)
      → p->p_seg.p_cr3 = cr3
      → p->p_seg.p_cr3_v = v
      → [p == ptproc] → write_cr3(cr3)
      → [p == VM] → arch_enable_paging(p)
      → RTS_UNSET(p, RTS_VMINHIBIT)

VM 进程 → SYS_VMCTL(VMCTL_GET_PDBR)
  → 返回 p->p_seg.p_cr3

VM 进程 → SYS_VMCTL(VMCTL_FLUSHTLB)
  → reload_cr3()

VM 进程 → SYS_VMCTL(VMCTL_I386_INVLPG)
  → i386_invlpg(addr)
```

#### 2.4.4 物理内存映射协商

```
VM 启动时遍历 arch_phys_map() 索引
  → arch_phys_map(index, &addr, &len, &flags)  // 内核声明需求
  → VM 为该区域创建映射
  → arch_phys_map_reply(index, vaddr)           // VM 回传虚拟地址
    → [first_um_idx] → 修正 minix_kerninfo 指针
    → [lapic] → lapic_addr = lapic_addr_vaddr
    → [video] → video_mem_vaddr = addr
```

#### 2.4.5 进程切换时的映射清除

```
进程切换前
  → mem_clear_mapcache()
    → ptproc->p_seg.p_cr3_v[freepdes[0]] = 0
    → ptproc->p_seg.p_cr3_v[freepdes[1]] = 0
```

### 2.5 设计要点/特殊处理

#### 2.5.1 为什么用 4MB 大页而非 4KB 普通页做临时映射

临时映射使用 PDE 大页模式（`I386_VM_BIGPAGE`），而非分配一个页表再映射 4KB 页。原因：

1. **性能**：大页只需写一个 PDE 条目，无需分配页表、写 1024 个 PTE
2. **简单**：物理地址映射直接构造 PDE 值，无需二级查找
3. **足够**：4MB 窗口对大多数内核操作（拷贝消息、读写寄存器）已经足够；超出 4MB 的操作通过循环多次映射完成

#### 2.5.2 为什么需要 2 个 freepdes 而非 1 个

`lin_lin_copy` 需要同时映射源和目标两个地址窗口。如果只有 1 个 freepde，就无法同时访问源和目标，拷贝操作无法完成。

#### 2.5.3 phys_get32 的递归安全

`vm_lookup` 通过 `phys_get32` 读取页目录/页表内容，而 `phys_get32` 内部调用 `lin_lin_copy`，`lin_lin_copy` 又调用 `createpde`。这形成了一个调用链：`vm_lookup` → `phys_get32` → `lin_lin_copy` → `createpde`。不会死循环的原因：

- `phys_get32` 传 `NULL`（物理地址）给 `lin_lin_copy`
- `createpde` 对物理地址（`pr == NULL`）直接构造大页 PDE，不调用 `vm_lookup`
- `lin_lin_copy` 对物理地址的 `createpde` 不依赖任何页表查询

#### 2.5.4 缺页挂起的必要性

内核访问进程内存时，该内存可能不存在（未映射、被换出、写时复制页）。内核不能自行处理缺页——因为页表管理是 VM 的职责。所以内核必须：

1. 捕获缺页（`PHYS_COPY_CATCH` / `phys_memset` 的缺页捕获机制）
2. 识别缺页方向（源还是目标）
3. 通过 `vm_suspend()` 挂起调用者，将控制权交给 VM
4. VM 处理缺页后，内核恢复调用者，重试操作

`check_resumed_caller()` 检查 `MF_KCALL_RESUME` 标志和 `p_vmrequest.vmresult`，判断 VM 是否已处理完毕。

#### 2.5.5 ptproc 与地址空间切换

`ptproc` 是内核运行时的"当前地址空间锚点"。内核不拥有自己的地址空间——它借用 VM 的地址空间运行。当 VM 通过 `VMCTL_SETADDRSPACE` 修改自己的页表时，如果 VM 就是 `ptproc`，则立即 `write_cr3` 刷新。

`switch_address_space()` 在进程切换时更新 `ptproc` 和 CR3。`mem_clear_mapcache()` 在切换前清除临时映射，防止残留映射泄露到下一个进程的视角。

#### 2.5.6 usermapped 区域的特殊处理

`arch_phys_map_reply()` 对 `first_um_idx` 的处理是整个文件最复杂的部分。它将内核的 `minix_kerninfo` 结构体中的所有指针从物理地址修正为虚拟地址（`FIXEDPTR`/`FIXPTR` 宏），并选择 IPC 向量集（SYSENTER/SYSCALL/softint）。这些数据最终通过 `minix_kerninfo_user` 暴露给用户进程，是用户态与内核通信的关键通道。

#### 2.5.7 VMCTL_SETADDRSPACE 与 RTS_VMINHIBIT

进程创建时 `RTS_VMINHIBIT` 标志被设置，表示进程还没有有效的地址空间，不能被调度。当 VM 通过 `VMCTL_SETADDRSPACE` 为进程设置页表后，`setcr3()` 清除 `RTS_VMINHIBIT`，进程才可以被调度运行。这是内核与 VM 之间的同步机制：内核保证在 VM 设置好页表之前不会调度该进程。

---

## 3. Rust 设计决策

> 本章解释"为什么这样设计"，每个决策追溯 Ch1&Ch2 的依据。

### 3.1 Direct Map 消除临时 PDE 映射机制

**决策**：minix-rs 不实现 `createpde`/`freepdes`/`mem_clear_mapcache`/`ptproc` 这套临时 PDE 映射机制。内核通过 Direct Map（`DirectMapArch::kernel_phys_to_virt()`）直接访问任意物理内存。

**依据**：§1.1 分析了 Minix3 临时 PDE 映射的根本原因——内核运行在 VM 的地址空间中，无法直接访问其他进程的物理内存。§1.3.1-1.3.2 分析了 `freepdes` 和 `ptproc` 都是为这个限制服务的。64 位架构的 Direct Map 从根本上消除了这个限制：`kernel_phys_to_virt(phys)` 将任意物理地址映射到内核虚拟地址，无需借用 PDE 槽位。

**为什么不是"优化"而是"消除"**：这不是用 Direct Map 替代 createpde 实现同样的"临时映射"语义，而是**临时映射这个概念本身不存在了**。Minix3 的 createpde 做的是"在当前页目录中写入一个临时 PDE 条目，映射目标进程的 4MB 窗口"；minix-rs 的 Direct Map 做的是"通过固定的偏移量，将物理地址转换为虚拟地址"。前者需要管理槽位、刷新 TLB、操作完成后清除；后者是纯算术运算，无副作用。

**替代方案**：保留 createpde 语义，用 `Paging::map()` + `Paging::unmap()` 实现临时映射。❌ 拒绝理由：这等于在 64 位架构上重新实现 32 位的限制，违背 Rewrite 原则（Don't Over-Emulate C：如果设计只因 C 语言/32 位限制而存在，用现代 Rust 表达）。

### 3.2 VM 建设偏移映射，内核只建脚手架

**决策**：内核在启动阶段只建 Identity Mapping（VA=PA）和高半核映射作为临时脚手架。Direct Map（`KERNEL_DIRECT_MAP_BASE`/`VM_DIRECT_MAP_BASE`）由 VM 进程在 `paging_init()` 中建立。

**依据**：§1.4 规则 5 明确"VM 是页表的唯一管理者"。§2.3.9 分析了 `VMCTL_SETADDRSPACE` 的 `setcr3()` 逻辑——内核通过 VMCTL 请求 VM 操作页表，不直接写。§2.3.10 分析了 `arch_enable_paging()` 的切换流程——VM 建好页表后通知内核切换。

**推理过程**：

考虑两种方案——

| 维度 | 内核建 Direct Map | VM 建 Direct Map |
|------|------------------|-----------------|
| 分工一致性 | 内核越权写页表，违反"VM 唯一管理者" | VM 统一管理所有页表操作 |
| 地址布局决策 | 内核需要知道 VM 的 Direct Map 基地址和堆布局 | VM 自己决定地址空间布局 |
| 启动复杂度 | 内核需要知道物理内存总量 | 内核只建最小脚手架 |
| Minix3 一致性 | Minix3 的 `pt_init()` 由 VM 重建页表 | 与 `pt_init()` 流程一致 |

选择 VM 建的理由：内核建 Direct Map 需要内核知道 VM 的地址空间布局（`VM_DIRECT_MAP_BASE`、`VM_HEAP_BASE`），造成内核和 VM 之间的隐式耦合。而 Minix3 的 `pt_init()` 已经证明了"VM 建一切"是可行的——VM 从内核获取 CR3 值和物理区域声明，然后自己创建新页表、拷贝映射、通知内核切换。

**时序约束**：`kernel_phys_to_virt()` 只在运行阶段（VM 设置好 Direct Map 之后）才能使用。启动阶段用 Identity Mapping（VA=PA），不需要 `kernel_phys_to_virt()`。不存在"Direct Map 还没建立但需要访问其他进程地址空间"的窗口期——因为那个窗口期根本没有其他进程。

### 3.3 先查后操作替代操作时捕获缺页

**决策**：跨地址空间操作（拷贝/清零）采用"先查询物理地址，再通过 Direct Map 操作"的两阶段模式，而非 Minix3 的"操作时通过 PHYS_COPY_CATCH 捕获缺页"模式。

**依据**：§1.3.5 分析了 VMSUSPEND 缺页挂起机制——`lin_lin_copy` 在拷贝过程中通过 `PHYS_COPY_CATCH` 捕获缺页，返回 `EFAULT_SRC`/`EFAULT_DST`。§2.3.2 分析了 `lin_lin_copy` 的完整流程——缺页发生在 `PHYS_COPY_CATCH` 执行期间。

**Minix3 流程 vs minix-rs 流程**：

```
Minix3:  拷贝 → 缺页捕获 → 返回 EFAULT_SRC/DST → vm_suspend → VM 处理 → 重试
minix-rs: 查询物理地址 → 页不存在 → 返回 PageFault → vm_suspend → VM 处理 → 重试
          查询物理地址 → 页存在 → Direct Map 拷贝（不会缺页）
```

**为什么更安全**：Minix3 的缺页发生在拷贝过程中，存在"拷贝到一半缺页需要回滚"的复杂状态。minix-rs 的缺页发生在查询阶段，不存在半完成状态——要么查询成功然后完整拷贝，要么查询失败直接挂起。

**外部行为不变**：调用者收到的仍然是"操作因缺页挂起，等 VM 处理后重试"——VMSUSPEND 语义保留。

### 3.4 PageTableRef 替代 segframe_t

**决策**：用 `PageTableRef` 结构体替代 Minix3 的 `segframe_t`，`cr3` 字段使用 `Option<PhysBytes>` 替代 `p_cr3 != 0` 的哨兵值判断。

**依据**：§1.3.3 分析了 `segframe_t` 的 `p_cr3` 和 `p_cr3_v` 两个字段。§1.3.4 分析了 `HASPT` 宏——`p_cr3 == 0` 表示进程没有独立页表。

**命名理由**：此结构体不是"地址空间"的完整描述（不包含虚拟地址布局、映射关系），而是对进程页表的一个引用——"这个进程有没有自己的页表？如果有，页目录在物理内存的哪里？"。命名为 `PageTableRef` 精确表达语义，且避免与 `PagingWithId::AddressSpaceId`（PCID/ASID，TLB 硬件标签）混淆。

**字段映射**：

| segframe_t 字段 | PageTableRef 字段 | 变化说明 |
|----------------|-------------------|---------|
| `p_cr3` (reg_t) | `cr3: Option<PhysBytes>` | `Some(cr3)` = 有页表，`None` = 内核进程共享内核地址空间。消除 `HASPT` 宏 |
| `p_cr3_v` (u32_t*) | 消除 | 通过 `DirectMapArch::kernel_phys_to_virt(cr3)` 按需计算，无需存储 |
| `fpu_state` | 移到 KProcess | 不属于页表概念 |
| `p_kern_trap_style` | 移到 KProcess | 不属于页表概念 |

**为什么消除 p_cr3_v**：Minix3 存储 `p_cr3_v` 是因为内核需要通过虚拟地址读写页目录内容，而 32 位架构下 `p_cr3` 是物理地址，不能直接访问。64 位 Direct Map 下，`kernel_phys_to_virt(cr3)` 将物理地址转换为虚拟地址，是纯算术运算，无需存储。

### 3.5 AddressRef 枚举替代 vir_addr 结构体

**决策**：用 `AddressRef` 枚举替代 Minix3 的 `vir_addr` 结构体，用类型系统区分"进程虚拟地址"和"物理地址"。

**依据**：§2.2.2 分析了 `vir_addr` 结构体——`proc_nr_e == NONE` 时 `offset` 被解释为物理地址。这是典型的 C 式哨兵值模式。

**Minix3 vs Rust**：

```c
// Minix3: 用 NONE 哨兵值区分两种语义
struct vir_addr {
    endpoint_t proc_nr_e;   // NONE = 物理地址
    vir_bytes  offset;
};
```

```rust
// minix-rs: 用枚举在类型层面区分
enum AddressRef {
    Process { endpoint: Endpoint, offset: VirBytes },
    Physical(PhysBytes),
}
```

**为什么用枚举**：`vir_addr` 的 `proc_nr_e == NONE` 是运行时检查，编译器无法保证调用者不会传错。`AddressRef` 枚举在编译时强制区分两种地址类型——`Process` 变体必须提供 `endpoint`，`Physical` 变体直接是物理地址。非法状态不可表达。

### 3.6 lookup_in_table 为自由函数而非 Paging trait 方法

**决策**：`lookup_in_table()` 是依赖 `DirectMapArch` trait 的自由函数，不是 `Paging` trait 的方法。

**依据**：§2.3.3 分析了 `vm_lookup()` 的功能——查询**任意指定进程**的虚拟地址对应的物理地址。§2.3.1 分析了 `createpde()` 的核心——操作**当前 CR3 的页目录**。

**推理过程**：

`Paging::query()` 查询的是**当前 CR3 加载的页表**——硬件 TLB 辅助，需要页表是"活跃的"。`lookup_in_table()` 查询的是**任意指定的页表**——通过 Direct Map 软件遍历页表结构，不需要切换 CR3。两者语义不同：

| 操作 | 前提 | 实现方式 |
|------|------|---------|
| `Paging::query()` | 页表已加载到 CR3 | 硬件页表遍历（TLB 缓存） |
| `lookup_in_table()` | 只需知道页表的物理地址 | Direct Map 软件遍历 |

如果将 `lookup_in_table` 放入 `Paging` trait，调用者需要先 `Paging::new_empty(root_page)` 创建一个 Paging 实例才能查询——这是不必要的对象创建。自由函数只需要 `root_paddr: PhysBytes` 参数，更简洁。

**依赖 `DirectMapArch` 而非具体硬件**：`lookup_in_table` 通过 `D::kernel_phys_to_virt()` 读取页表内容，不直接操作 CR3/PTE 等硬件寄存器。不同架构的页表结构差异（x86-64 四级 vs RISC-V Sv39 三级）通过泛型参数或架构特定实现处理，上层代码只调用 `lookup_in_table()`。

### 3.7 VMSUSPEND 语义保留，实现重新表达

**决策**：VMSUSPEND 的外部行为（操作因缺页挂起，等 VM 处理后重试）保留不变，但内部实现从"拷贝时捕获缺页"变为"查询时发现缺页"。

**依据**：§1.3.5 分析了 VMSUSPEND 机制——`lin_lin_copy` 返回 `EFAULT_SRC`/`EFAULT_DST`，`virtual_copy_f` 调用 `vm_suspend`。§2.3.6 分析了 `virtual_copy_f` 的 `vmcheck` 参数——`vmcheck=1` 时允许 VMSUSPEND。

**错误码映射**：

| Minix3 返回值 | 含义 | Rust 等价 |
|-------------|------|----------|
| `VMSUSPEND` (-996) | 操作因缺页挂起 | `CrossSpaceResult::Suspended(VmFaultType)` |
| `EFAULT_SRC` (-995) | 源地址缺页 | `VmCopyError::SrcPageFault` |
| `EFAULT_DST` (-994) | 目标地址缺页 | `VmCopyError::DstPageFault` |
| `EFAULT` (14) | 地址错误（非缺页） | `VmCopyError::InvalidAddress` |
| `OK` (0) | 成功 | `Ok(())` |

**VmCopyContext 结构体**：替代 Minix3 的 `p_vmrequest`（§2.2.3）中与拷贝相关的字段。`vmresult` 三态哨兵值由 03-vm-request.md 的 `VmSuspendState` 枚举替代。

### 3.8 BKL 保护下的并发安全

**决策**：当前跨地址空间操作设计假设 BKL（Big Kernel Lock）保护，不引入额外的同步机制。

**依据**：Minix3 内核支持 SMP，使用 BKL 自旋锁确保同一时刻只有一个 CPU 执行内核代码（`smp.h:48`）。虽然 BKL 在时钟中断、APIC 中断、IPI 同步等待等路径中会被释放（`smp.c:44,86-94`，`arch_clock.c:92,107,118`），但 BKL 释放窗口内不会执行 `cross_space_copy`/`cross_space_memset`——这些函数只在系统调用处理路径中被调用，而系统调用处理全程持有 BKL。

**推理过程**：

| 维度 | 分析 |
|------|------|
| BKL 保护范围 | 系统调用处理全程持有 BKL，`cross_space_copy` 在此范围内执行 |
| BKL 释放窗口 | 时钟/APIC/IPI 路径释放 BKL，但这些路径不执行跨地址空间操作 |
| 跨 CPU TLB 一致性 | `VMCTL_SETADDRSPACE` 需要通知其他 CPU 刷新目标进程的 TLB（Minix3 通过 `smp_schedule_vminhibit()` 实现） |
| `Rc`/`RefCell` 安全性 | 当前 `vm.rs` 未使用，若将来使用需确保不在 BKL 释放窗口内访问 |

**与 Minix3 的对照**：

| 方面 | Minix3 | minix-rs |
|------|--------|----------|
| 并发模型 | BKL + IPI | BKL + IPI（同） |
| 跨 CPU TLB 刷新 | `smp_schedule_vminhibit()` | SMP IPI 机制见 18-smp.md |
| `p_stale_tlb` 位图 | 跟踪其他 CPU 上的陈旧 TLB | 设计见 18-smp.md |
| `RTS_VMINHIBIT` 标志 | 阻止进程在其他 CPU 上调度 | 保留（见 §4.7） |

**限制**：当前设计不处理以下场景（留待 SMP 完整实现）：
- 多 CPU 同时修改同一进程的地址空间描述
- Direct Map 区域的并发访问（BKL 已保护，但需显式说明）
- `p_stale_tlb` 位图的跨 CPU 同步

> 完整的 SMP 设计见 [18-smp.md](18-smp.md)。

---

## 4. 实现详解

> 本章解释"如何实现"，每个实现对应 Ch3 的设计决策。

### 4.1 PageTableRef — 进程页表引用

> 设计决策：§3.4（PageTableRef 替代 segframe_t）

`PageTableRef` 是内核对进程页表的引用。它只包含一个字段——页目录的物理地址。内核进程（IDLE、CLOCK）没有独立页表，`cr3` 为 `None`，共享内核地址空间。

```rust
pub struct PageTableRef {
    cr3: Option<PhysBytes>,
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 (segframe_t) | minix-rs (PageTableRef) |
|------|---------------------|------------------------|
| 判断有无页表 | `HASPT(proc) = (proc->p_seg.p_cr3 != 0)` | `pt_ref.cr3().is_some()` |
| 获取 CR3 值 | `proc->p_seg.p_cr3` | `pt_ref.cr3().unwrap()` |
| 获取页目录虚拟地址 | `proc->p_seg.p_cr3_v` | `D::kernel_phys_to_virt(cr3)` |
| 设置页表 | `p->p_seg.p_cr3 = cr3; p->p_seg.p_cr3_v = v` | `pt_ref.set_cr3(cr3)` |
| 清除页表 | `p->p_seg.p_cr3 = 0` | `pt_ref.clear_cr3()` |

**关键方法**：

- `cr3()` → `Option<PhysBytes>`：读取 CR3 物理地址。`None` 表示无独立页表
- `set_cr3(cr3: PhysBytes)`：设置页表。对应 `VMCTL_SETADDRSPACE` 的 `setcr3()` 逻辑
- `page_table_vaddr<D: DirectMapArch>()` → `Option<VirBytes>`：通过 Direct Map 计算页目录的虚拟地址。对应 Minix3 的 `p_cr3_v`，但按需计算而非存储

### 4.2 AddressRef — 地址引用枚举

> 设计决策：§3.5（AddressRef 枚举替代 vir_addr 结构体）

`AddressRef` 在类型层面区分"进程虚拟地址"和"物理地址"两种语义，消除 `proc_nr_e == NONE` 的哨兵值。

```rust
pub enum AddressRef {
    Process { endpoint: Endpoint, offset: VirBytes },
    Physical(PhysBytes),
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 (vir_addr) | minix-rs (AddressRef) |
|------|-------------------|----------------------|
| 进程虚拟地址 | `{ .proc_nr_e = PM_PROC_NR, .offset = 0x1000 }` | `AddressRef::Process { endpoint: Endpoint::PM, offset: VirBytes(0x1000) }` |
| 物理地址 | `{ .proc_nr_e = NONE, .offset = 0x100000 }` | `AddressRef::Physical(PhysBytes(0x100000)) }` |
| 判断类型 | `if (va.proc_nr_e == NONE)` | `match addr_ref { Process => ..., Physical => ... }` |

### 4.3 VmCopyError — 跨地址空间操作错误

> 设计决策：§3.7（VMSUSPEND 语义保留，实现重新表达）

`VmCopyError` 枚举覆盖跨地址空间操作的所有错误场景。错误码与 Minix3 严格对应，不自行创造。

```rust
pub enum VmCopyError {
    SrcPageFault,       // EFAULT_SRC (-995): 源地址缺页
    DstPageFault,       // EFAULT_DST (-994): 目标地址缺页
    InvalidAddress,     // EFAULT (14): 地址无效（非缺页类错误）
    Suspended,          // VMSUSPEND (-996): 操作因缺页挂起
    PermissionDenied,   // EPERM (1): 权限不足
    UnknownEndpoint,    // ESRCH (3): 进程不存在
}
```

**错误码对齐**：每个变体对应 Minix3 的一个 errno 或内核内部返回值。`SrcPageFault`/`DstPageFault` 对应 `EFAULT_SRC`/`EFAULT_DST`（内核内部值，不暴露给用户态）；`Suspended` 对应 `VMSUSPEND`（内核内部值）；`InvalidAddress` 对应 `EFAULT`（POSIX errno）。

### 4.3a VmFaultType — 缺页方向

> 设计决策：§3.7（VMSUSPEND 语义保留）

`VmFaultType` 区分缺页发生在源端还是目标端，用于 `VmCopyContext` 记录挂起原因。

```rust
pub enum VmFaultType {
    Src,
    Dst,
}
```

对应 Minix3 `p_vmrequest` 中隐含的缺页方向信息——Minix3 通过 `vmresult` 的值（`EFAULT_SRC`/`EFAULT_DST`）判断方向，minix-rs 用独立枚举显式表达。

### 4.3b VmCopyContext — 跨地址空间操作挂起请求

> 设计决策：§3.7（VMSUSPEND 语义保留）

`VmCopyContext`（原 `VmRequest`，已重命名以避免与 03-vm-request.md 的 `VmSuspendContext` 混淆）替代 Minix3 的 `p_vmrequest` 中与跨地址空间拷贝相关的字段，记录拷贝操作因缺页挂起时的上下文信息，以便 VM 处理缺页后恢复操作。

> **注意**：`p_vmrequest` 中的 `vmresult` 三态哨兵值（0/VMSUSPEND/errno）已由 03-vm-request.md §3.3 的 `VmSuspendState` 枚举替代。`VmCopyContext` 仅记录拷贝方向信息，不再承担 `vmresult` 的职责。

```rust
pub struct VmCopyContext {
    pub src: AddressRef,
    pub dst: AddressRef,
    pub bytes: usize,
    pub fault_type: VmFaultType,
}
```

**与 Minix3 p_vmrequest 的对照**：

| p_vmrequest 字段 | VmCopyContext 字段 | 变化说明 |
|-----------------|-------------------|---------|
| `src` (vir_addr) | `src: AddressRef` | 枚举替代哨兵值（§3.5） |
| `dst` (vir_addr) | `dst: AddressRef` | 同上 |
| `length` (vir_bytes) | `bytes: usize` | 类型更精确 |
| `vmresult` (int) | ~~`fault_type: Option<VmFaultType>`~~ → `VmSuspendState` | `vmresult` 的三态语义已迁移至 03-vm-request.md 的 `VmSuspendState` 枚举（Pending/Fetched/Completed）。`VmCopyContext.fault_type` 仅记录缺页方向（Src/Dst） |

`VmCopyContext.fault_type: VmFaultType`（非 `Option`）记录缺页发生在源端还是目标端。`vmresult` 的三态哨兵值（0=未处理、VMSUSPEND=已获取、其他=已完成）已由 `VmSuspendState` 枚举替代（见 03-vm-request.md §3.3）。

### 4.3c resolve_physical — 地址引用解析（内部辅助）

`resolve_physical` 是 `cross_space_copy`/`cross_space_memset` 的内部辅助函数，将 `AddressRef` 解析为物理地址。

```rust
fn resolve_physical<D: DirectMapArch>(
    addr: &AddressRef,
    proc_cr3: impl Fn(Endpoint) -> Option<PhysBytes>,
) -> Option<PhysBytes>
```

**逻辑**：
- `AddressRef::Physical(paddr)` → 直接返回 `Some(paddr)`
- `AddressRef::Process { endpoint, offset }` → 通过 `proc_cr3(endpoint)` 获取 CR3，再调用 `lookup_in_table::<D>(cr3, offset)` 查询物理地址

**设计选择**：返回 `Option<PhysBytes>` 而非 `Result<PhysBytes, VmCopyError>`，因为"解析失败"的语义（缺页/进程不存在）应由调用者根据上下文映射为 `SrcPageFault`/`DstPageFault`/`UnknownEndpoint`。函数本身不知道自己用于源端还是目标端。

### 4.3d copy_page_table_ref — fork 页表引用复制

`copy_page_table_ref` 在 `sys_fork` 时为新进程创建页表引用。当前实现返回空的 `PageTableRef`（`cr3 = None`），因为新进程的页表由 VM 分配（通过 `VMCTL_SETADDRSPACE` 设置），内核不复制父进程的 CR3。

```rust
pub fn copy_page_table_ref(_as: &PageTableRef) -> PageTableRef {
    PageTableRef::new()
}
```

**与 Minix3 的对照**：Minix3 的 `do_fork` 也不复制 `p_cr3`——新进程的页表由 VM 通过 `vm_fork` 创建，再通过 `VMCTL_SETADDRSPACE` 通知内核。`copy_page_table_ref` 的存在是为了在 fork 流程中提供统一的页表引用处理入口，即使当前实现是空操作。

### 4.4 lookup_in_table — 页表软件遍历

> 设计决策：§3.6（lookup_in_table 为自由函数）

`lookup_in_table()` 通过 Direct Map 软件遍历指定页表，查询虚拟地址对应的物理地址。不依赖当前 CR3 状态，不需要切换地址空间。

```rust
pub fn lookup_in_table<D: DirectMapArch>(
    root_paddr: PhysBytes,
    vaddr: VirBytes,
) -> Option<(PhysBytes, PageFlags)>
```

**执行流程**（以 x86-64 四级页表为例）：

1. 通过 `D::kernel_phys_to_virt(root_paddr)` 获取 PML4 的虚拟地址
2. 从 `vaddr` 提取 PML4 索引（bit 47-39），读取 PML4 条目
3. 如果条目不存在（`!PRESENT`），返回 `None`
4. 从条目提取 PDPT 物理地址，通过 Direct Map 读取 PDPT 条目
5. 如果 PDPT 条目是 1GB 大页（`PS` 位），直接计算物理地址并返回
6. 否则继续遍历 PD → PT，处理 2MB 大页和 4KB 页

**与 Minix3 vm_lookup 的关键差异**：

| 方面 | Minix3 vm_lookup | minix-rs lookup_in_table |
|------|-----------------|-------------------------|
| 读取页表方式 | `phys_get32` → `lin_lin_copy` → `createpde` | Direct Map 直接读取 |
| 递归依赖 | vm_lookup → phys_get32 → lin_lin_copy → createpde | 无递归，Direct Map 打断依赖链 |
| 大页处理 | 检查 `BIGPAGE` 标志 | 检查 `PS` 标志，支持 1GB/2MB/4KB |
| TLB 影响 | 无（软件遍历不触发 TLB） | 无（同） |

**架构差异处理**：x86-64 四级页表、aarch64 三级页表、RISC-V Sv39/Sv48 的遍历逻辑不同。当前实现为 x86-64；其他架构需要各自的遍历实现。可以通过泛型参数或条件编译选择，但上层代码统一调用 `lookup_in_table()`。

### 4.5 cross_space_copy — 跨地址空间拷贝

> 设计决策：§3.3（先查后操作替代操作时捕获缺页）

`cross_space_copy()` 实现跨地址空间的数据拷贝。采用"先查询物理地址，再通过 Direct Map 拷贝"的两阶段模式。

```rust
pub fn cross_space_copy<D: DirectMapArch>(
    src: &AddressRef,
    dst: &AddressRef,
    bytes: usize,
    proc_cr3: impl Fn(Endpoint) -> Option<PhysBytes>,
) -> Result<(), VmCopyError>
```

**`proc_cr3` 闭包参数**：当 `AddressRef::Process` 变体需要查询进程页表时，`cross_space_copy` 需要知道目标进程的 CR3 物理地址。由于内核进程表（`KProcess`）的完整定义尚未稳定，`proc_cr3` 以闭包形式注入，避免 `vm.rs` 依赖具体的进程表结构。调用者传入类似 `|ep| proc_table.get(ep).and_then(|p| p.pt_ref.cr3())` 的闭包。

**执行流程**：

1. **解析源地址**：`match src` → `Process` 则从进程页表查询物理地址，`Physical` 则直接使用
2. **解析目标地址**：同上
3. **查询物理地址**：调用 `lookup_in_table::<D>(cr3, offset)` → `None` 则返回 `SrcPageFault`/`DstPageFault`
4. **Direct Map 拷贝**：`kernel_phys_to_virt(src_phys)` → `kernel_phys_to_virt(dst_phys)` → `copy_nonoverlapping`
5. **返回结果**：`Ok(())` 或 `VmCopyError`

**与 Minix3 lin_lin_copy 的关键差异**：

| 方面 | Minix3 lin_lin_copy | minix-rs cross_space_copy |
|------|--------------------|--------------------------|
| 映射方式 | createpde 临时映射 4MB 窗口 | Direct Map 直接访问 |
| 缺页时机 | 拷贝过程中 PHYS_COPY_CATCH 捕获 | 查询阶段 lookup_in_table 发现 |
| TLB 刷新 | createpde 后 reload_cr3 | 无需（Direct Map 是固定映射） |
| 窗口限制 | 每次 4MB，大块需循环 | 无窗口限制（Direct Map 覆盖全部物理内存） |
| 临时映射清除 | mem_clear_mapcache | 无需 |

**TODO**：当前实现是单页拷贝（一次 `lookup_in_table` 查一页）。跨页边界的拷贝需要循环处理，每页独立查询物理地址。Minix3 的 `lin_lin_copy` 通过 `createpde` 的 `bytes` 截断实现分页；minix-rs 需要在 `cross_space_copy` 内部按页循环。

### 4.6 cross_space_memset — 跨地址空间清零

> 设计决策：§3.3（先查后操作）

与 `cross_space_copy` 相同模式，但只映射一个方向（目标地址），使用 `core::ptr::write_bytes` 执行清零。

```rust
pub fn cross_space_memset<D: DirectMapArch>(
    dst: &AddressRef,
    value: u8,
    count: usize,
    proc_cr3: impl Fn(Endpoint) -> Option<PhysBytes>,
) -> Result<(), VmCopyError>
```

`proc_cr3` 闭包参数的设计理由同 `cross_space_copy`（见上文）。

对应 Minix3 的 `vm_memset()`（§2.3.5）。

### 4.7 VMCTL 命令处理

> 设计决策：§3.2（VM 建设偏移映射）

VM 通过 `SYS_VMCTL` 系统调用向内核发送页表管理命令。minix-rs 的处理逻辑与 Minix3 的 `arch_do_vmctl()`（§2.3.9）语义对齐：

| VMCTL 命令 | Minix3 操作 | minix-rs 操作 |
|-----------|------------|--------------|
| `VMCTL_GET_PDBR` | 读取 `p->p_seg.p_cr3` | `pt_ref.cr3()` |
| `VMCTL_SETADDRSPACE` | 设置 `p_cr3`/`p_cr3_v`，条件 `write_cr3`，清除 `RTS_VMINHIBIT` | `pt_ref.set_cr3(cr3)`，条件 `Paging::switch()`，清除 `rts::VMINHIBIT` |
| `VMCTL_FLUSHTLB` | `reload_cr3()` | `Paging::flush_tlb()` |
| `VMCTL_I386_INVLPG` | `i386_invlpg()` | `Paging::flush_tlb_addr()` |

`VMCTL_SETADDRSPACE` 的关键逻辑保留：如果目标是当前进程，立即切换地址空间；清除 `RTS_VMINHIBIT` 使进程可被调度。对应 Minix3 `setcr3()` 的步骤 2-4（§2.3.9）。

**SMP 注意事项**：Minix3 在 `VMCTL_SETADDRSPACE` 处理中调用 `smp_schedule_vminhibit(p)`（`do_vmctl.c:127-133`），通知目标进程当前所在 CPU 刷新 TLB 并停止该进程。如果目标进程在其他 CPU 上运行，必须通过 IPI 使其停止后才能安全修改地址空间。minix-rs 的 SMP IPI 机制见 [18-smp.md](18-smp.md)，但 `RTS_VMINHIBIT` 标志已保留——设置此标志后，进程不会被调度到任何 CPU，等效于 Minix3 的 `smp_schedule_vminhibit` 效果。

### 4.8 arch_enable_paging — VM 分页使能

> 设计决策：§3.2（VM 建设偏移映射）

VM 进程首次通过 `VMCTL_SETADDRSPACE` 设置自己的页表时，内核从启动阶段的 Identity Mapping 切换到 VM 的真实地址空间。对应 Minix3 的 `arch_enable_paging()`（§2.3.10）。

**Minix3 vs minix-rs**：

| 方面 | Minix3 | minix-rs |
|------|--------|----------|
| 地址切换 | `switch_address_space(vm)` | `Paging::switch()` |
| MMIO 地址修正 | `video_mem = video_mem_vaddr` | MMIO 引用通过 Direct Map 计算 |
| APIC 地址 | `lapic_addr = lapic_addr_vaddr` | 同上 |

**时序**：此函数在 VM 第一次 `VMCTL_SETADDRSPACE` 时调用。调用后，Identity Mapping 消失，所有物理地址引用必须通过 Direct Map。

---

## 5. 测试要点

### 5.1 PageTableRef 测试

- `cr3 = None` 时 `cr3().is_some()` 返回 `false`（对应 `HASPT == 0`）
- `set_cr3()` 后 `cr3()` 返回正确值
- `clear_cr3()` 后 `cr3()` 返回 `None`
- `page_table_vaddr()` 通过 Direct Map 计算的虚拟地址与物理地址偏移正确

### 5.2 AddressRef 测试

- `Process` 变体携带正确的 endpoint 和 offset
- `Physical` 变体直接是物理地址
- 模式匹配覆盖所有变体，编译器保证穷尽

### 5.3 lookup_in_table 测试

- 已映射地址返回正确的 `(PhysBytes, PageFlags)`
- 未映射地址返回 `None`
- 大页条目（2MB/1GB）正确处理
- 跨页边界地址查询正确

### 5.4 cross_space_copy 测试

- 进程→进程拷贝：数据正确
- 物理→进程拷贝：数据正确
- 源缺页：返回 `SrcPageFault`
- 目标缺页：返回 `DstPageFault`
- 跨页边界拷贝：数据完整

### 5.5 VMCTL 命令测试

- `VMCTL_SETADDRSPACE`：CR3 正确设置，`RTS_VMINHIBIT` 正确清除
- 当前进程设置地址空间：立即 `switch()`
- 非当前进程设置地址空间：不立即 `switch()`

---

## 6. 参见

- [01-multiboot-bootstrap.md](01-multiboot-bootstrap.md) — 启动阶段分页设置（Identity Mapping + 高半核映射）
- [02-stage-vm/06-pagetable-struct.md](../../02-stage-vm/06-pagetable-struct.md) — VM 页表数据结构（pt_t）
- [02-stage-vm/07-pagetable-ops.md](../../02-stage-vm/07-pagetable-ops.md) — VM 页表操作（pt_mapkernel, pt_bind）
- `os/arch/src/paging.rs` — Paging trait 定义
- `os/arch/src/paging_ext.rs` — HugePages trait 定义
- `os/arch/src/direct_map.rs` — DirectMapArch trait 定义
- `os/kernel/src/vm.rs` — 内核 VM 模块实现
