# 01-vmproc-struct: VM 进程结构体 (vmproc)

> **分类**: VM私有
> **源码**: `minix3/minix/servers/vm/vmproc.h`
> **说明**: 定义 VM 进程控制块结构体，是 VM 最核心的数据结构

***

## 目录

1. [概述](#1-概述)
   - [1.1 设计目标](#11-设计目标)
2. [背景与约束](#2-背景与约束)
   - [2.1 vmproc 在 VM 中的位置](#21-vmproc-在-vm-中的位置)
   - [2.2 vmproc 特有的约束](#22-vmproc-特有的约束)
3. [C 源码分析](#3-c-源码分析)
   - [3.1 vmproc 结构体定义](#31-vmproc-结构体定义)
   - [3.2 关键字段详解](#32-关键字段详解)
4. [Rust 设计决策](#4-rust-设计决策)
   - [4.1 类型系统设计](#41-类型系统设计)
   - [4.2 状态管理](#42-状态管理)
   - [4.3 内存布局](#43-内存布局)
5. [实现详解](#5-实现详解)
   - [5.1 VmProc 结构体](#51-vmproc-结构体)
   - [5.2 关联类型](#52-关联类型)
   - [5.3 辅助方法](#53-辅助方法)
6. [生命周期与状态机](#6-生命周期与状态机)
   - [6.1 进程状态流转](#61-进程状态流转)
   - [6.2 fork 流程中的状态一致性](#62-fork-流程中的状态一致性)
   - [6.3 错误处理与 panic 安全](#63-错误处理与-panic-安全)
7. [测试与验证](#7-测试与验证)
   - [7.1 测试基础设施](#71-测试基础设施)
   - [7.2 结构体构造与初始状态](#72-结构体构造与初始状态)
   - [7.3 字段访问与一致性](#73-字段访问与一致性)
   - [7.4 状态标志（VmFlags）](#74-状态标志vmflags)
   - [7.5 Drop 行为](#75-drop-行为)
   - [7.6 clear() 安全性](#76-clear-安全性)
   - [7.7 与 Minix3 行为对比](#77-与-minix3-行为对比)
   - [7.8 测试维度总结](#78-测试维度总结)
8. [参见](#8-参见)

***

## 1. 概述

本文档详细说明 VM (Virtual Memory) 服务器的进程结构体 `vmproc` 的设计。

`vmproc` 是 VM 模块最核心的数据结构，每个进程在 VM 中都有一个对应的 `vmproc` 条目，用于管理该进程的内存相关信息。它存储了进程的页表、虚拟内存区域、访问控制权限等关键信息，是 VM 实现内存管理功能的基础。

### 1.1 设计目标

- **生命周期管理**: 精确跟踪进程从创建到退出的整个生命周期，支持 fork、exit 等操作
- **资源隔离**: 每个进程有独立的虚拟地址空间，通过 `vmproc` 隔离管理
- **权限控制**: 通过 ACL 机制控制进程可以调用的 VM 系统调用

***

## 2. 背景与约束

### 2.1 vmproc 在 VM 中的位置

`vmproc` 是 VM 模块最核心的数据结构，与其他组件的关系：

```
vmproc (进程控制块)
    ├── vm_pt → PageTable (页表)
    ├── vm_regions_avl → AVL Tree → vir_region (虚拟区域)
    │                                      ↓
    │                               phys_region (物理区域)
    │                                      ↓
    │                               phys_block (物理块)
    │
    ├── vm_acl → ACL Mask (权限控制)
    └── vm_slot / vm_endpoint (进程标识)
```

**核心作用**:

- 每个进程在 VM 中有且仅有一个 `vmproc` 条目
- 通过 `vmproc` 可以访问该进程的所有内存相关信息
- `vmproc` 的 slot 号与 PM 中的进程槽位一一对应

> **注意**: VM 整体架构和与其他服务的关系，详见 [00-vm-overview.md](00-vm-overview.md)。

### 2.2 vmproc 特有的约束

#### 2.2.1 与 Minix3 语义兼容

根据语义冻结原则（外部语义不变，内部表达可以改变）：

**必须保持的语义**:

- **行为一致**: Rust 实现的 `vmproc` 必须表现出与 C 版本相同的外部行为
- **状态等价**: `vm_flags` 的各种组合在 Rust 中必须有相同的语义
- **接口兼容**: 与 PM、内核的交互必须遵循相同的协议

**允许的内部优化**:

- flag → enum（如 `VmFlags` 用 bitflags 实现）
- int → newtype（如 `UserSlot`、`Endpoint`）
- struct 拆分（如将相关字段组织为子结构）

> **注意**: 不要求内存布局与 C 完全一致，只要求外部可观察的行为一致。

#### 2.2.2 字段访问模式

- **高频访问**: `vm_flags`、`vm_endpoint`、`vm_pt`（每次 IPC 都可能访问）
  - 通过 `vm_isokendpt(endpoint, &slot)` 直接获取 slot，无需额外索引
- **中频访问**: `vm_regions_avl`（内存操作时使用）
- **低频访问**: `vm_boot`、`vm_bytecopies`（仅特定场景）

#### 2.2.3 初始化要求

- `vm_slot` 在 slot 被首次使用时设置，之后不变
  - Minix3: `vmproc[i].vm_slot = i` 在 [main.c:461](minix3/minix/servers/vm/main.c#L461) 初始化时设置
  - Rust 实现差异见 [§4.3](#43-内存布局) 和 [§5.1.2](#512-初始化策略)
- `vm_flags` 初始为 0，通过 `VMF_INUSE` 标记激活
- `vm_endpoint` 在 `sys_fork` 后由内核分配

***

## 3. C 源码分析

### 3.1 vmproc 结构体定义

**文件**: `minix3/minix/servers/vm/vmproc.h`

```c
struct vmproc {
  int        vm_flags;           /* 进程标志位 */
  endpoint_t vm_endpoint;        /* 进程端点标识符 */
  pt_t       vm_pt;              /* 页表数据 */
  struct boot_image *vm_boot;    /* 启动时进程的引导映像指针 */
  region_avl vm_regions_avl;     /* 虚拟地址空间中的区域 AVL 树 */
  vir_bytes  vm_region_top;      /* 最后插入的最高虚拟地址 */
  int        vm_acl;             /* ACL 访问控制列表索引 */
  int        vm_slot;            /* 进程表槽位号 */
#if VMSTATS
  int        vm_bytecopies;      /* 字节复制计数（调试用） */
#endif
  vir_bytes  vm_total;           /* 总虚拟内存大小 */
  vir_bytes  vm_total_max;       /* 最大虚拟内存大小 */
  u64_t      vm_minor_page_fault;/* 次缺页中断计数 */
  u64_t      vm_major_page_fault;/* 主缺页中断计数 */
};
```

**字段分类**:

- **进程标识**: `vm_flags`, `vm_endpoint`, `vm_slot`
- **内存管理**: `vm_pt`, `vm_regions_avl`, `vm_region_top`
- **访问控制**: `vm_acl`
- **启动信息**: `vm_boot`
- **统计信息**: `vm_total`, `vm_total_max`, `vm_bytecopies`, `vm_minor_page_fault`, `vm_major_page_fault`

### 3.2 关键字段详解

#### 3.2.1 vm_flags - 进程状态标志

**定义**:

```c
#define VMF_INUSE       0x001    /* 槽位包含一个活跃进程 */
#define VMF_EXITING     0x002    /* PM 正在清理此进程 */
#define VMF_VM_INSTANCE 0x010    /* 这是一个 VM 进程实例 */
```

**语义说明**:

| 标志                | 值     | 说明      | 典型组合                         |
| ----------------- | ----- | ------- | ---------------------------- |
| `VMF_INUSE`       | 0x001 | 槽位被占用   | 所有活跃进程必须有                    |
| `VMF_EXITING`     | 0x002 | 进程正在退出  | `INUSE \| EXITING`：正在清理      |
| `VMF_VM_INSTANCE` | 0x010 | VM 自身进程 | `INUSE \| VM_INSTANCE`：VM 进程 |

**使用场景**:

- **进程分配**: PM 通过 `VM_FORK` 请求分配新槽位时，VM 设置 `VMF_INUSE` 标记该槽位已被占用
- **进程退出**: PM 调用 `VM_WILLEXIT` 时，VM 设置 `VMF_EXITING` 防止新的内存操作（[exit.c:112](minix3/minix/servers/vm/exit.c#L112) `do_willexit()`），之后 PM 调用 `VM_EXIT` 时 VM 清理资源。`do_exit()` 会检查 `VMF_EXITING` 标志——未经过 `VM_WILLEXIT` 的退出请求会被拒绝（[exit.c:73](minix3/minix/servers/vm/exit.c#L73)）
- **VM 自识别**: VM 进程自身带有 `VMF_VM_INSTANCE`，用于特殊处理（如避免递归调用，[main.c:579](minix3/minix/servers/vm/main.c#L579)）
- **退出处理**: `do_exit()` 检查 `VMF_VM_INSTANCE`，如果设置则递减全局计数器 `num_vm_instances`（用于 RS 重启机制）。Rust 代码中 `clear()` 自动处理此逻辑

**标志位组合**:

这三个标志位**不是互斥的**，可以同时设置：

- `VMF_INUSE \| VMF_EXITING`: 进程正在退出但槽位仍占用
- `VMF_INUSE \| VMF_VM_INSTANCE`: VM 进程自身

**fork 时的处理**:

```c
// fork.c:83
vmc->vm_flags &= VMF_INUSE;  // 只保留 INUSE，清除其他标志
```

- 子进程只保留 `VMF_INUSE`，清除 `VMF_EXITING` 和 `VMF_VM_INSTANCE`
- 防止继承父进程的退出状态或特殊身份
- 确保子进程以"干净"的状态开始生命周期

#### 3.2.2 vm_endpoint - 进程端点标识符

> **注意**: `endpoint_t` 是全局概念，定义在 `minix/endpoint.h`。详见 [Endpoint 协议](../../concepts/endpoint.md)。

**在 vmproc 中的作用**:

- **IPC 路由**: 当 PM 发送 `VM_FORK` 消息时，使用父进程的 endpoint 来标识"为谁创建子进程"
- **内核回调**: VM 调用 `sys_fork()` 后，内核分配新的 endpoint 并通过参数返回，VM 将其存入子进程的 `vm_endpoint`

**fork 时的处理**:

1. **fork 前验证**: VM 收到 `VM_FORK` 消息后，验证父进程 endpoint 是否有效
   ```c
   // fork.c:41-44
   if (vm_isokendpt(msg->VMF_ENDPOINT, &proc_nr) != OK) {
       return EINVAL;  // 父进程不存在或已退出
   }
   ```
   > 验证机制详见 [02-vmproc-table.md](02-vmproc-table.md) 的 `vm_isokendpt` 说明。
2. **fork 中**: VM 调用 `sys_fork()`，内核生成新 endpoint
   ```c
   // 内核分配新的 endpoint，通过 child_ep 参数返回
   sys_fork(parent_ep, child_slot, &child_ep, ...);
   ```
3. **fork 后**: 将内核返回的 endpoint 存入子进程
   ```c
   vmc->vm_endpoint = child_ep;
   ```

> **注意**：`vm_isokendpt` 验证的是 endpoint **有效性**（是否存在、是否活跃），不是**权限**。权限验证有两层：
>
> 1. **身份验证**：IPC 机制确保 `msg->m_source` 不可伪造 → 确认调用者身份
> 2. **ACL 检查**：`acl_check()` 检查调用者是否有权执行该系统调用
>
> 第 1 层由 IPC 机制隐式完成，第 2 层由 ACL 显式检查。详见 [03-acl.md](03-acl.md)。

#### 3.2.3 vm\_pt - 页表数据

**类型**: `pt_t`（页表结构）

**作用**: 存储虚拟地址到物理地址的映射关系，是 MMU 地址转换的依据。

**fork 时的处理**:

1. 创建新的页表（`pt_new`）
2. 复制父进程页表映射（共享物理页，只读标记）
3. 绑定页表到子进程（`pt_bind`）

> **详见**: [06-pagetable-struct.md](06-pagetable-struct.md)

#### 3.2.4 vm\_regions\_avl - 虚拟区域 AVL 树

**类型**: `region_avl`

**作用**: 管理进程的虚拟地址空间布局，记录代码段、数据段、堆、栈等区域。

**fork 时的处理**:

1. 初始化空的 AVL 树
2. 遍历父进程区域，复制到子进程
3. 增加物理页引用计数（CoW 机制）

> **详见**: [11-region-mapping.md](11-region-mapping.md) 和 [13-region-avl.md](13-region-avl.md)

#### 3.2.5 vm\_acl - ACL 访问控制列表索引

**作用**: 控制进程可以调用哪些 VM 系统调用。
**取值**:

- `NO_ACL` (-1): **临时状态**，进程尚未设置 ACL，暂时允许所有调用（会打印警告）
- `USER_ACL` (0): 普通用户进程共享的 ACL，有明确的权限限制
- `1~31`: 系统进程独立的 ACL 索引

**fork 时的处理**:

- 父进程为 `USER_ACL`（普通用户进程），子进程继承 `USER_ACL`
- 父进程为系统进程（有独立 ACL），子进程获得 `NO_ACL`，需要 RS 重新设置权限

> **详见**: [03-acl.md](03-acl.md)

#### 3.2.6 其他字段

##### vm\_slot - 进程表槽位号

**类型**: `int`

**作用**: `vmproc` 数组中的索引（0\~NR\_PROCS-1），O(1) 定位进程结构。

**slot 与 endpoint 的关系**

```
endpoint = (slot << 8) | generation
```

- **endpoint**: 全局唯一标识，用于 IPC 通信
- **slot**: 数组索引，用于 O(1) 快速访问
- **generation**: 防止 slot 重用时的混淆

**为什么需要额外的 vm\_slot 字段？**

虽然 `vm_slot` 是冗余的（可通过 `_ENDPOINT_P(vm_endpoint)` 提取），但保留它有两个重要原因：

1. **处理中间状态**

   在 fork 过程中，存在 slot 已知但 endpoint 还未生成的阶段：
   ```
   T0: PM 分配 child_slot，发送 VM_FORK 消息给 VM
   T1: VM 使用 child_slot 初始化 vmproc[child_slot]
   T2: VM 调用 sys_fork()，内核生成 child_endpoint
   T3: VM 将 child_endpoint 存入 vmproc[child_slot].vm_endpoint
   ```
   在 T1-T2 阶段，endpoint 还不存在，但 VM 需要知道正在操作哪个 slot。
2. **一致性检查**
   ```c
   assert(p->vm_slot == _ENDPOINT_P(p->vm_endpoint));
   ```
   这个断言可以在 debug 时快速发现 invariant 被破坏的情况。

**本质**: 冗余存储换工程可控性。允许短暂的不一致（slot 已知但 endpoint 未生成），但让代码更容易写对。

**使用场景**

- **数组访问**: `vmproc[slot]` 直接获取进程结构，O(1) 时间复杂度
- **PM 通信**: PM 通过 `VM_FORK` 消息传递子进程的 slot 号，VM 直接使用该 slot 初始化子进程
- **资源限制**: `NR_PROCS` 定义了系统最大进程数，slot 号超出范围是非法的

**fork 时的处理**

1. PM 调用 `get_free_slot()` 找到空闲 slot
2. PM 将 `child_slot` 通过 `VM_FORK` 消息发送给 VM
3. VM 验证 slot 有效后，存入 `vmproc[child_slot].vm_slot`
4. VM 调用 `sys_fork()` 获取 endpoint，存入 `vmproc[child_slot].vm_endpoint`

##### vm\_boot - 引导映像指针

**类型**: `struct boot_image *`

**作用**: 指向内核启动时加载的进程映像信息，仅对系统服务（如 PM、VFS、VM 自身）有效。

**使用场景**:

- **启动初始化**：系统启动时，内核加载各个服务器进程，VM 记录每个进程的引导信息
- **重启恢复**：如果某个系统服务崩溃重启，可以使用 `vm_boot` 重新初始化其内存空间

**Minix3** **`init_proc()`** **初始化路径**: `vm_boot` 唯一被设置的地方是 `init_proc()`（[main.c:262](minix3/minix/servers/vm/main.c#L262)）：

```c
static struct vmproc *init_proc(endpoint_t ep_nr)
{
    struct boot_image *ip;
    for (ip = &kernel_boot_info.boot_procs[0];
            ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
        struct vmproc *vmp;
        if(ip->proc_nr != ep_nr) continue;
        if(ip->proc_nr >= _NR_PROCS || ip->proc_nr < 0)
            panic("proc: %d", ip->proc_nr);
        vmp = &vmproc[ip->proc_nr];
        assert(!(vmp->vm_flags & VMF_INUSE));  /* no double procs */
        clear_proc(vmp);
        vmp->vm_flags = VMF_INUSE;
        vmp->vm_endpoint = ip->endpoint;
        vmp->vm_boot = ip;       /* ← vm_boot 唯一被设置的地方 */
        return vmp;
    }
    panic("no init_proc");
}
```

> **Rust 设计差异**：Rust 使用 `Option<BootImage>` 值语义而非 C 的 `struct boot_image *` 指针。详见 [§4.3 内存布局](#43-内存布局) 和 [§5.1 VmProc 结构体](#51-vmproc-结构体)。

**fork 时的处理**: 普通用户进程的 `vm_boot` 为 `NULL`，fork 时直接复制（`NULL` 复制后还是 `NULL`）。

##### vm\_region\_top - 最高虚拟地址

**类型**: `vir_bytes`

**作用**: 记录已分配区域中最高的结束地址，用于快速分配新区域。

**使用场景**:

- **堆增长**：`brk()` 系统调用增加堆大小时，通常从 `vm_region_top` 向上扩展
- **内存映射**：`mmap()` 没有指定地址时，默认从 `vm_region_top` 开始分配

**fork 时的处理**: 直接复制父进程的值，子进程的地址空间布局与父进程一致。

> **详见**: [11-region-mapping.md](11-region-mapping.md) 的区域管理。

##### vm\_total / vm\_total\_max - 虚拟内存大小

**类型**: `vir_bytes`

**作用**:

- `vm_total`: 当前进程已分配的虚拟内存总量
- `vm_total_max`: 虚拟内存历史峰值（非硬限制）

> **`total_max`** **不是硬限制**：`add_total()` 在累加 `vm_total` 后，如果 `vm_total > vm_total_max`，会自动更新 `vm_total_max`。因此 `total_max` 是"历史峰值"而非"不允许超过的上限"。真正的资源限制由 PM 侧管理。

**使用场景**:

- **资源统计**：跟踪进程的内存使用量和历史峰值
- **OOM 预防**：分配内存前检查 `vm_total + 新分配大小 <= vm_total_max`（软限制）

**fork 时的处理**: 子进程继承父进程的 `vm_total` 和 `vm_total_max`，因为子进程初始时与父进程占用相同的虚拟内存。

> **详见**: [04-physical-memory.md](04-physical-memory.md) 的物理内存管理。

##### vm\_bytecopies - 字节复制计数

**类型**: `int`（仅在 `VMSTATS` 启用时存在）

**作用**: 统计 VM 执行的字节复制操作次数，用于性能分析和调试。

**使用场景**:

- **性能监控**：统计 CoW 触发时的实际物理页复制次数
- **优化分析**：评估 CoW 机制的效果（复制次数越少，CoW 效果越好）

**fork 时的处理**: 初始化为 0，在 CoW 触发实际复制时递增。

**条件编译说明**:

```c
// Minix3 C 源码
#if VMSTATS
  int vm_bytecopies;
#endif
```

- 该字段仅在启用 `VMSTATS` 时存在
- 生产环境通常禁用，避免运行时开销
- 调试/性能分析时启用，收集统计信息

> **Rust 实现差异**：Rust 使用 `u64`（64-bit）而非 C 的 `int`（32-bit），并通过 `#[cfg(feature = "vmstats")]` 条件编译。详见 [§5.1 VmProc 结构体](#51-vmproc-结构体)。

> **详见**: [14-cow-mechanism.md](14-cow-mechanism.md) 的写时复制机制。

##### vm\_minor\_page\_fault / vm\_major\_page\_fault - 缺页统计

**类型**: `u64_t`

**作用**:

- **Minor Page Fault**: 页表项存在但权限不足（如 CoW 写保护触发），无需磁盘 I/O
- **Major Page Fault**: 需要磁盘 I/O（如加载可执行文件的代码段、交换区换入）

**使用场景**:

- **性能分析**：Major fault 过多表示内存压力大，可能需要增加物理内存
- **CoW 监控**：Minor fault 增加表示 CoW 机制正在工作

**fork 时的处理**: 子进程初始化为 0，开始独立统计自己的缺页情况。

> **详见**: [15-pagefault.md](15-pagefault.md) 的页错误处理。

***

## 4. Rust 设计决策

### 4.1 类型系统设计

#### 4.1.1 公共类型

VM 使用以下公共类型（定义在 `minix_types` crate）：

| 类型          | 说明      |
| ----------- | ------- |
| `UserSlot`  | 进程表槽位索引 |
| `Endpoint`  | 进程端点标识符 |
| `VirBytes`  | 虚拟地址字节数 |
| `PhysBytes` | 物理地址字节数 |

> **详见**: [系统核心概念](../../concepts/README.md) 中的相关文档。

#### 4.1.2 VM 私有类型

以下类型是 VM 私有的，定义在 VM crate 内：

```rust
/// ACL 状态，表达进程的权限配置。
///
/// 对应 Minix3 的三种状态：
/// - `Uninitialized` → `NO_ACL (-1)`
/// - `Default` → `USER_ACL (0)`
/// - `System(mask)` → 系统进程 ACL 槽位
pub(crate) enum AclState {
    Uninitialized,
    Default,
    System(AclMask),
}
```

**设计原则**:

- 系统级概念（如 slot、endpoint）使用公共类型
- VM 特有概念（如 ACL 状态）使用 VM 私有类型
- 详见 [03-acl.md](03-acl.md) 中 ACL 模块的完整设计

### 4.2 状态管理

#### 4.2.1 为什么用 flags 不用 enum？

`VmFlags` 使用 `bitflags` crate 实现：

```rust
bitflags! {
    pub struct VmFlags: u32 {
        const IN_USE      = 0x001;  // 槽位包含一个进程
        const EXITING     = 0x002;  // PM 正在清理此进程
        const VM_INSTANCE = 0x010;  // 这是 VM 进程实例
    }
}
```

VM 的进程状态是**多维正交**的，多个状态可以同时存在：

```rust
// 典型状态组合
const ACTIVE: VmFlags = VmFlags::IN_USE;
const EXITING: VmFlags = VmFlags::IN_USE.union(VmFlags::EXITING);
const VM_PROC: VmFlags = VmFlags::IN_USE.union(VmFlags::VM_INSTANCE);
```

如果用 enum，会变成：

```rust
// enum 强制互斥，需要嵌套 bool
enum VmState {
    Free,
    Active { exiting: bool, is_vm: bool },
}
// 实际上退化成 struct，不如 bitflags 清晰
```

**bitflags 的优势**:

- 自然表达正交状态组合
- 与 C 的位标志兼容
- 高效的位运算操作

#### 4.2.2 为什么 endpoint 不用 Option？

`VmProc` 中的 `endpoint` 字段是**非 Option** 的：

```rust
pub struct VmProc {
    pub endpoint: Endpoint,  // 不是 Option<Endpoint>
    pub flags: VmFlags,
    // ...
}
```

**设计结论**: endpoint 不用 `Option`，原因有三：

1. **fork 时的中间态极短暂**
   - T0: PM 分配 slot，发送 VM_FORK 消息
   - T1: VM 初始化 vmproc（slot 已知，endpoint 未生成）
   - T2: VM 调用 sys_fork()，获得 endpoint
   - T3: VM 存储 endpoint，设置 IN_USE 标志

   T1-T2 阶段只占进程生命周期的 0.001%，不应为此惩罚 100% 的代码。
2. **Option 的代价** — 所有访问都需要处理 `None`，但 90% 的代码走 `Some` 分支
3. **flags 更合适** — 用 `VmFlags::IN_USE` 判断有效性，与 Minix3 的 `vm_flags & VMF_INUSE` 一致

> **详细分析**: 关于 `Option` vs `MaybeUninit+flag` 的对比，见 5.1.2 节"初始化策略"。

### 4.3 内存布局

`VmProc` 结构体本身的内存布局是常规的 Rust struct，不涉及特殊的地址稳定性要求。

进程表（`VmProcTable`）层面的内存布局设计（静态数组、`AssumeSyncCell`、地址稳定性分析等）详见 [02-vmproc-table.md](02-vmproc-table.md) 的第 2 节和第 4 节。

***

## 5. 实现详解

### 5.1 VmProc 结构体

#### 5.1.1 字段分层设计

```rust
/// VM process structure.
///
/// Design principles:
/// - All fields always exist (no Option)
/// - State expressed via flags
/// - Allows temporary inconsistency (e.g., during fork)
/// - `vm_pt` and `vm_regions` are MaybeUninit - only valid when IN_USE
///
/// Corresponds to Minix3's `struct vmproc`.
///
/// # Visibility Design
/// `VmProc` is NOT exported from the `vmproc` module. External code must use
/// typestate views (`EmptySlot`, `ActiveProc`, `ExitingProc`) to access
/// process data. Fields are `pub(crate)` for internal use within the vmproc
/// module tree.
#[derive(Debug)]
pub struct VmProc {
    // === 标识层 ===
    pub(crate) vm_slot: UserSlot,
    pub(crate) vm_endpoint: Endpoint,
    pub(crate) vm_flags: VmFlags,
    pub(crate) vm_acl: AclState,
    /// Boot image info (only valid for boot-time processes).
    pub(crate) vm_boot: Option<BootImage>,

    // === 内存层 ===
    /// Page table - uninitialized until `init_page_table()` is called.
    /// TODO: Evaluate replacing MaybeUninit+bool with a custom InPlaceOption<T>
    /// that provides safe in-place initialization/cleanup without move-out.
    pub(crate) vm_pt: MaybeUninit<PageTable>,
    /// Virtual memory regions map - uninitialized until `init_regions()` is called.
    /// TODO: Evaluate replacing MaybeUninit+bool with a custom InPlaceOption<T>
    /// that provides safe in-place initialization/cleanup without move-out.
    pub(crate) vm_regions: MaybeUninit<RegionMap>,
    /// Whether vm_pt has been initialized (must check before assume_init).
    pub(crate) vm_pt_initialized: bool,
    /// Whether vm_regions has been initialized (must check before assume_init).
    pub(crate) vm_regions_initialized: bool,
    pub(crate) vm_region_top: VirBytes,

    // === 资源限制 ===
    pub(crate) vm_total: VirBytes,
    pub(crate) vm_total_max: VirBytes,

    // === 统计数据 ===
    pub(crate) vm_minor_page_fault: u64,
    pub(crate) vm_major_page_fault: u64,

    // === 调试统计（条件编译）===
    /// Byte copy count (only when vmstats feature is enabled).
    #[cfg(feature = "vmstats")]
    pub(crate) vm_bytecopies: u64,
}
```

**命名约定**: 保留 C 源码的 `vm_` 前缀（如 `vm_flags`、`vm_endpoint`），与 Minix3 的 `struct vmproc` 字段名一一对应，方便对照 C 源码。

**可见性设计**: 所有字段为 `pub(crate)`，且 `VmProc` 本身不导出。外部代码必须通过 typestate view（`EmptySlot`、`ActiveProc`、`ExitingProc`）访问进程数据。这通过 Rust 模块系统的可见性控制，在编译期强制外部代码使用 typestate API。

#### 5.1.2 初始化策略

**构造函数**: `vacant()` 和 `vacant_with_slot()`

```rust
impl VmProc {
    /// Creates a vacant (unoccupied) process slot.
    ///
    /// Corresponds to Minix3's `memset(vmproc, 0, sizeof(vmproc))`.
    /// `vm_pt` and `vm_regions` are uninitialized - only access when IN_USE.
    pub const fn vacant() -> Self {
        Self {
            vm_slot: UserSlot(0),
            vm_endpoint: Endpoint::NONE,
            vm_flags: VmFlags::empty(),
            vm_acl: AclState::Uninitialized,
            vm_boot: None,
            vm_pt: MaybeUninit::uninit(),
            vm_regions: MaybeUninit::uninit(),
            vm_pt_initialized: false,
            vm_regions_initialized: false,
            vm_region_top: VirBytes::new(0),
            vm_total: VirBytes::new(0),
            vm_total_max: VirBytes::new(0),
            vm_minor_page_fault: 0,
            vm_major_page_fault: 0,
            #[cfg(feature = "vmstats")]
            vm_bytecopies: 0,
        }
    }

    /// Creates a vacant slot with the given slot number.
    pub const fn vacant_with_slot(vm_slot: UserSlot) -> Self {
        let mut proc = Self::vacant();
        proc.vm_slot = vm_slot;
        proc
    }
}
```

**Minix3 初始化序列对应**:

Minix3 的 vmproc 初始化是三步：

| 步骤            | Minix3                                         | Rust 对应                                          | 说明                                                   |
| ------------- | ---------------------------------------------- | ------------------------------------------------ | ---------------------------------------------------- |
| 1. BSS 零初始化   | `memset(vmproc, 0, sizeof(vmproc))`            | `VmProc::vacant()`                               | `vm_acl = AclState::Uninitialized`, `vm_slot = UserSlot(0)` |
| 2. 设置 slot 号  | `vmproc[i].vm_slot = i` (main.c:461)           | `get_empty()`/`alloc_empty_slot()` 中设置 `vm_slot` | 实际 slot 号在使用时设置                                      |
| 3. 修正 ACL 初始值 | `acl_init()`: `vmproc[i].vm_acl = NO_ACL` (-1) | `vacant()` 中 `AclState::Uninitialized`          | 直接使用 `Uninitialized`，无需后续修正                            |

**注意**: Rust 的 `vacant()` 直接初始化为 `NO_ACL`，而 Minix3 是 BSS 零初始化后再由 `acl_init()` 修正。两者最终语义一致，但 Rust 更直接。

**MaybeUninit+flag 模式**: `vm_pt` 和 `vm_regions` 使用 `MaybeUninit` + `bool` flag 跟踪初始化状态，而非 `Option`：

```rust
// ❌ Option 的代价：占用额外 tag 空间，且 move-out 语义不适合 in-place 场景
pub struct VmProc {
    pub vm_pt: Option<PageTable>,    // 额外 tag + move-out 语义
}

// ✅ MaybeUninit+flag：零额外空间开销，适合 in-place 初始化
pub struct VmProc {
    pub vm_pt: MaybeUninit<PageTable>,  // 无额外 tag
    pub vm_pt_initialized: bool,        // 独立 flag
}
```

**为什么不用 Option**:

1. **零开销**: `MaybeUninit` 无额外 tag，与原始内存布局一致
2. **避免隐式 Drop**: `Option::take()` 会 move out 值并触发 Drop。对于进程表这种"先初始化后清理"的场景，我们需要的是 in-place 初始化（`write()`）和 in-place 清理（`assume_init_drop()`），而不是 move-out + drop
3. **避免栈开销**: Minix3 的 `pt_t` 在 i386 上约 8KB（1024 个指针），在 ARM 上约 32KB（4096 个指针）。`Option::take()` 会将整个结构 move 到栈上，造成显著的栈空间压力。`MaybeUninit::write()` 是 in-place 写入，无此开销

> **关键区别**: `Option::take()` = move out 到栈 + 留下 `None` + 触发 Drop；`MaybeUninit::write()` + `assume_init_drop()` = in-place 写入/清理（无 move，无栈开销）。对于进程表这种"数据必须留在原地"的场景，后者是唯一正确的选择。

**endpoint 暂时不一致的处理**（通过 typestate API）:

```rust
// fork 过程中，通过 EmptySlot::activate_relaxed() 允许中间态
let empty = table.get_empty(child_slot)?;
let mut active = empty.activate_relaxed(Endpoint::NONE); // fork 时 endpoint 暂时为 NONE

// sys_fork 后更新
let child_ep = sys_fork(parent.endpoint)?;
active.set_endpoint(child_ep); // 设置真实 endpoint

// 初始化页表和区域（fork 场景用 init_page_table + CoW，exec 场景用 init_page_table）
```

### 5.2 关联类型

`VmProc` 的字段引用了多个外部类型，这些类型定义在 vmproc 模块之外，各有专属文档：

| 类型          | VmProc 字段                                | 定义位置                                  | 说明                                                                                                           | 专属文档                                             |
| ----------- | ---------------------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------ |
| `BootImage` | `vm_boot: Option<BootImage>`             | `minix-types` crate (`types/boot.rs`) | 启动时进程的引导映像信息，跨服务共享类型                                                                                         | [00-vm-overview.md](00-vm-overview.md)           |
| `PageTable` | `vm_pt: MaybeUninit<PageTable>`          | `vm/src/pagetable/mod.rs`             | 进程页表，`minix_arch::CurrentPaging` 的类型别名                                                                       | [06-pagetable-struct.md](06-pagetable-struct.md) |
| `RegionMap` | `vm_regions: MaybeUninit<RegionMap>` | `vm/src/region/region_map.rs`           | 虚拟内存区域映射表，使用 BTreeMap 按地址排序管理进程区域                                                                                     | [11-region-mapping.md](11-region-mapping.md)             |
| `AclState`  | `vm_acl: AclState`                       | `vm/src/acl.rs`                       | ACL 状态，控制进程对 VM 系统调用的访问。三态 enum：`Uninitialized`/`Default`/`System(AclMask)` | [03-acl.md](03-acl.md)       |

**与 VmProc 的关系**: 这些类型通过 `VmProc` 的字段被组合使用，但各自有独立的生命周期管理和 API。在 vmproc 模块中，通过 typestate view（`ActiveProc`）的方法访问它们，例如 `ActiveProc::page_table()`、`ActiveProc::regions()`、`ActiveProc::init_page_table()` 等。

### 5.3 辅助方法

```rust
impl VmProc {
    /// Checks if the process is in use.
    #[inline]
    pub(crate) fn is_in_use(&self) -> bool {
        self.vm_flags.contains(VmFlags::IN_USE)
    }

    /// Checks if the process is exiting.
    #[inline]
    pub(crate) fn is_exiting(&self) -> bool {
        self.vm_flags.contains(VmFlags::EXITING)
    }

    /// Checks if this is a VM instance.
    #[inline]
    pub(crate) fn is_vm_instance(&self) -> bool {
        self.vm_flags.contains(VmFlags::VM_INSTANCE)
    }

    /// Debug invariant check (runtime assertion).
    #[cfg(debug_assertions)]
    pub(crate) fn check(&self) {
        if self.vm_flags.contains(VmFlags::IN_USE) {
            debug_assert!(!self.vm_endpoint.is_none(), "IN_USE but vm_endpoint is NONE");
        }
    }

    /// Explicitly clears process resources.
    ///
    /// Core of explicit resource management - called from typestate transitions
    /// (`ExitingProc::reap()`, `ActiveProc::force_clear()`).
    /// Does not rely on Drop.
    ///
    /// Only clears `vm_pt` and `vm_regions` if they were previously initialized
    /// (tracked by `vm_pt_initialized` / `vm_regions_initialized` flags). This makes it
    /// safe to call on slots that were activated but never had vm_pt/vm_regions
    /// initialized (e.g., fork intermediate state).
    ///
    /// Corresponds to Minix3's `free_proc()` + `clear_proc()`, with differences
    /// (see design notes below).
    ///
    /// # Safety
    /// Caller must ensure this process's page table is no longer in use by hardware.
    /// Caller must also call `AclManager::clear()` before this method to release ACL slot.
    pub(crate) unsafe fn clear(&mut self) {
        if self.vm_regions_initialized {
            unsafe { self.vm_regions.assume_init_mut().clear(); }
        }
        if self.vm_pt_initialized {
            unsafe { self.vm_pt.assume_init_mut().destroy(); }
        }

        if self.vm_flags.contains(VmFlags::VM_INSTANCE) {
            crate::global::dec_vm_instance();
        }

        self.vm_flags = VmFlags::empty();
        self.vm_endpoint = Endpoint::NONE;
        self.vm_boot = None;
        self.vm_acl = AclState::Uninitialized;
        self.vm_pt_initialized = false;
        self.vm_regions_initialized = false;

        self.vm_region_top = VirBytes::new(0);
        self.vm_total = VirBytes::default();
        self.vm_total_max = VirBytes::default();
        self.vm_minor_page_fault = 0;
        self.vm_major_page_fault = 0;

        #[cfg(feature = "vmstats")]
        {
            self.vm_bytecopies = 0;
        }
    }
}

impl Default for VmProc {
    fn default() -> Self {
        Self::vacant()
    }
}
```

> **已确认**: Rust 的 `clear()` 内部直接将 `vm_acl` 重置为 `AclState::Uninitialized`（等价于 Minix3 的 `acl_clear()` 将 `vm_acl` 设为 `NO_ACL`），无需外部单独调用 `acl_clear()`。这合并了 Minix3 的 `acl_clear()` + `clear_proc()` 为一个操作。

**Drop 实现**（调试断言）:

```rust
/// VmProc is always in-place in the process table and must never be dropped.
///
/// # Design Note
/// In production, the process table is a static variable that never gets dropped
/// (program exits without calling destructors). Any Drop call indicates a bug
/// in process table management (e.g., moving a VmProc out of its slot).
///
/// In tests, vacant slots may be dropped during cleanup, which is acceptable.
/// Only dropping an IN_USE slot is a bug.
impl Drop for VmProc {
    fn drop(&mut self) {
        #[cfg(not(test))]
        {
            panic!(
                "VmProc should never be dropped in production — use in-place cleanup via clear()"
            );
        }

        #[cfg(test)]
        if self.vm_flags.contains(VmFlags::IN_USE) {
            panic!(
                "VmProc dropped while IN_USE — process table management bug. \
                 Use in-place cleanup via VmProc::clear() or typestate transitions."
            );
        }
    }
}
```

> **TODO**: 后续应实现内核专属的 `panic()` 函数（类似 Minix3 的 `panic()`），支持栈展开以打印触发 drop 的文件名和行号。当前 Rust 标准库的 `panic!` 在隐式 drop 场景下无法直接获取调用位置，需要依赖 panic 运行时的栈展开能力。

**设计说明**:

| 方法                 | 用途           | 说明                                                |
| ------------------ | ------------ | ------------------------------------------------- |
| `is_in_use()`      | 检查进程是否活跃     | 封装 `vm_flags.contains(IN_USE)`                    |
| `is_exiting()`     | 检查进程是否正在退出   | 封装 `vm_flags.contains(EXITING)`                   |
| `is_vm_instance()` | 检查是否为 VM 实例  | 封装 `vm_flags.contains(VM_INSTANCE)`               |
| `check()`          | Debug 时检查不变量 | 验证 IN\_USE 时 endpoint 非 NONE                      |
| `clear()`          | 显式资源清理       | 对应 Minix3 的 `free_proc()` + `clear_proc()`，见下方对比表 |

**`clear()`** **与 Minix3** **`free_proc()`** **/** **`clear_proc()`** **对比**:

Minix3 的进程退出分两步：`free_proc()` 释放页表/物理页/区域/统计，`clear_proc()` 重置区域/ACL/标志位/统计。Rust 的 `clear()` 合并了两步：

| 字段                   | Minix3 `free_proc()` | Minix3 `clear_proc()`       | Rust `clear()`           | 一致?         |
| -------------------- | -------------------- | --------------------------- | ------------------------ | ----------- |
| 映射页释放               | `map_free_proc()`    | —                           | `vm_regions.clear()` | ✅           |
| 页表释放                 | `pt_free()`          | —                           | `vm_pt.destroy()`        | ✅           |
| 区域重置                 | `region_init()`      | `region_init()`             | `vm_regions.clear()` | ✅           |
| ACL 清理               | —                    | `acl_clear()` (释放+设NO\_ACL) | `AclState::Uninitialized` | ✅           |
| vm\_flags            | —                    | `= 0`                       | `= empty()`              | ✅           |
| vm\_endpoint         | —                    | 不重置                         | `= NONE`                 | ⚠️ Rust 更彻底 |
| vm\_boot             | —                    | 不重置                         | `= None`                 | ⚠️ Rust 更彻底 |
| vm\_region\_top      | `= 0`                | `= 0`                       | `= VirBytes::new(0)`     | ✅           |
| vm\_total/max/faults | `reset_vm_rusage()`  | `reset_vm_rusage()`         | 重置                       | ✅           |
| vm\_bytecopies       | `= 0`                | `= 0`                       | `= 0` (cfg)              | ✅           |

**关键差异**:

1. **ACL 重置**: `clear()` 内部直接将 `vm_acl` 重置为 `AclState::Uninitialized`，等价于 Minix3 的 `acl_clear()` 将 `vm_acl` 设为 `NO_ACL`。由于 Rust 的 `AclState::System(AclMask)` 权限数据内联于 enum 中（无全局 `acl_mask[][]` 数组），无需额外的槽位释放操作
2. **endpoint/boot 重置**: Rust 比 Minix3 更彻底，清除了 Minix3 不重置的字段。Minix3 不重置是因为 C 代码依赖 `vm_flags = 0`（清除 `VMF_INUSE`）来标记 slot 为空闲，后续 `*vmc = *vmp` 会无条件覆盖所有字段，旧值不会被观察到。Rust 的 typestate 体系下，`EmptySlot` 可能被多次读取（如 `check()` 断言），残留的 endpoint/boot 值可能导致误判，因此必须清除
3. **VM\_INSTANCE 计数器**: `clear()` 内部处理 `VM_INSTANCE` 标志的计数器递减（对应 Minix3 的 `do_exit()` 在 `free_proc()` + `clear_proc()` 之前检查 `VMF_VM_INSTANCE` 并递减 `num_vm_instances`）

**VmProc 的双层安全设计**:

`VmProc` 自身用 flags + runtime checks 保持灵活（支持 fork 中间态等暂时不一致），而 typestate view 在上层提供编译期保证（`EmptySlot`/`ActiveProc`/`ExitingProc` 只能执行对应状态允许的操作）。两者互补：typestate view 是编译期的"粗粒度"状态约束，`VmProc` 的 `check()` 是运行时的"细粒度"不变量断言（`debug_assertions` 模式，生产环境零开销）。

***

## 6. 生命周期与状态机

### 6.1 进程状态流转

#### 6.1.1 各阶段说明

| 阶段      | Minix3 标志                          | Rust typestate                                 | 说明                       | 触发条件                     |
| ------- | --------------------------------- | ---------------------------------------------- | ------------------------ | ------------------------ |
| **空闲**  | `vm_flags=0`                      | `EmptySlot`                                    | 槽位未使用                    | 初始状态或回收后                 |
| **半初始化** | `VMF_INUSE` + endpoint=NONE        | `ActiveProc`（endpoint=NONE，`activate_relaxed`） | PM 已分配 slot，等待 sys\_fork | PM 发送 VM\_FORK 消息        |
| **运行**  | `VMF_INUSE`（endpoint 有效）           | `ActiveProc`（endpoint 非 NONE）                  | 进程正常运行                   | sys\_fork 成功，获得 endpoint |
| **退出中** | `VMF_INUSE \| VMF_EXITING` | `ExitingProc`                                  | PM 正在清理                  | 进程调用 exit 或被信号终止         |

#### 6.1.2 Typestate View 设计

**设计目标**: 在编译期保证状态转换的合法性，避免运行时检查。

**EmptySlot**: 空闲槽位的 view

```rust
pub(crate) struct EmptySlot<'a> {
    inner: &'a mut VmProc,
}

impl<'a> EmptySlot<'a> {
    /// 严格模式：验证 endpoint.slot() 匹配 self.slot()
    ///
    /// **注意**：使用 `debug_assert_eq!`，仅在 debug 构建时检查。
    /// Release 模式下跳过验证，依赖调用者保证一致性。
    pub(crate) fn activate(self, endpoint: Endpoint) -> ActiveProc<'a> {
        debug_assert_eq!(endpoint.slot() as usize, self.slot().get());
        self.activate_relaxed(endpoint)   // 严格模式内部调用宽松模式
    }

    /// 宽松模式：不验证 endpoint/slot 一致性
    pub(crate) fn activate_relaxed(self, endpoint: Endpoint) -> ActiveProc<'a> {
        self.inner.vm_flags = VmFlags::IN_USE;
        self.inner.vm_endpoint = endpoint;
        ActiveProc::new(self.inner)
    }
}
```

**`activate()`** **vs** **`activate_relaxed()`** **设计权衡**：

| <br /> | `activate()`                                     | `activate_relaxed()`          |
| ------ | ------------------------------------------------ | ----------------------------- |
| 检查     | `debug_assert_eq!(endpoint.slot(), self.slot())` | 无检查                           |
| 实现方式   | 内部调用 `activate_relaxed()`                        | 直接设置 flags + endpoint         |
| 检查时机   | 仅 `debug` 构建                                     | —                             |
| 适用场景   | 正常进程创建                                           | fork / exec temp slot / tests |

**设计说明**：

- **`activate()` 验证 slot 一致性**：确保 `endpoint.slot()` 与目标槽位匹配，捕获调用者传入错误 endpoint 的 bug。仅 `debug_assert` 因为这是编程错误检查，非安全不变量。
- **`activate_relaxed()` 跳过验证**：用于 fork（endpoint 暂时为 NONE）、exec temp slot、测试等特殊场景。
- **与 Minix3 的差异**：Minix3 没有"激活"操作，也没有此类验证。Minix3 的 `vm_isokendpt()` 检查完整 endpoint 一致性（用于系统调用验证），Rust 的 `VmProcTable::vm_isokendpt()` 同样实现了完整验证。

**ActiveProc**: 活跃进程的 view

```rust
pub(crate) struct ActiveProc<'a> {
    inner: &'a mut VmProc,
}

impl<'a> ActiveProc<'a> {
    /// 状态转换：Active → Exiting
    pub(crate) fn mark_exiting(self) -> ExitingProc<'a> {
        self.inner.vm_flags.insert(VmFlags::EXITING);
        ExitingProc::new(self.inner)
    }

    /// 强制清理：Active → Empty（异常终止场景）
    pub(crate) unsafe fn force_clear(self) -> EmptySlot<'a> {
        unsafe { self.inner.clear(); }
        EmptySlot::new(self.inner)
    }

    /// 字段访问
    pub(crate) fn endpoint(&self) -> Endpoint { self.inner.vm_endpoint }
    pub(crate) fn set_endpoint(&mut self, ep: Endpoint) { self.inner.vm_endpoint = ep; }
    pub(crate) fn page_table(&self) -> &PageTable { ... }
    pub(crate) fn page_table_mut(&mut self) -> &mut PageTable { ... }
    pub(crate) fn regions(&self) -> &RegionMap { ... }
    pub(crate) fn regions_mut(&mut self) -> &mut RegionMap { ... }

    /// 内存统计
    pub(crate) fn total(&self) -> VirBytes { self.inner.vm_total }
    pub(crate) fn total_max(&self) -> VirBytes { self.inner.vm_total_max }
    pub(crate) fn set_total_max(&mut self, value: VirBytes) { self.inner.vm_total_max = value; }
    pub(crate) fn add_total(&mut self, value: VirBytes) {
        self.inner.vm_total.0 += value.0;
        if self.inner.vm_total > self.inner.vm_total_max {
            self.inner.vm_total_max = self.inner.vm_total;  // 自动更新历史峰值
        }
    }
    pub(crate) fn sub_total(&mut self, value: VirBytes) {
        self.inner.vm_total.0 = self.inner.vm_total.0.saturating_sub(value.0);  // 饱和减法
    }
    pub(crate) fn set_total(&mut self, value: VirBytes) { self.inner.vm_total = value; }

    /// 初始化方法
    pub(crate) fn init_page_table(&mut self) -> Result<(), PageTableError> {
        let mut pt = <PageTable as Paging>::new()?;
        pt.map_kernel()?;
        self.inner.vm_pt.write(pt);
        self.inner.vm_pt_initialized = true;
        Ok(())
    }
    pub(crate) fn bind_page_table(&self) -> Result<(), PageTableError> {
        self.page_table().bind_to_process(self.endpoint())
    }
    pub(crate) fn init_regions(&mut self) {
        self.inner.vm_regions.write(RegionMap::new());
        self.inner.vm_regions_initialized = true;
    }
    pub(crate) fn init_from_fork(&mut self, endpoint: Endpoint, total: VirBytes, total_max: VirBytes, region_top: VirBytes) {
        self.inner.vm_flags = VmFlags::IN_USE;   // 清除其他标志，只保留 IN_USE
        self.inner.vm_endpoint = endpoint;
        self.inner.vm_total = total;
        self.inner.vm_total_max = total_max;
        self.inner.vm_region_top = region_top;
    }
    pub(crate) fn copy_acl_from(&mut self, parent: &ActiveProc<'_>) {
        // 对应 Minix3 的 acl_fork()
        // 父进程有 USER_ACL → 子进程也获得 USER_ACL，否则 NO_ACL
    }

    /// CoW 与页表映射
    pub(crate) unsafe fn setup_cow_for_all_regions(&mut self) {
        // 遍历所有 region，设置 WRITABLE 标志
        // 对每个 physblock 调用 add_ref()（引用计数 +1）
        // 调用 region.prepare_cow()
    }
    pub(crate) unsafe fn write_page_table_mappings(&mut self) {
        // 对应 Minix3 的 map_writept()
        // 遍历所有 region 和 physblock，收集 (vaddr, paddr, flags)
        // 写入页表映射
    }
}
```

**ActiveProc 方法分类**：

| 类别       | 方法                                               | 说明                                                   |
| -------- | ------------------------------------------------ | ---------------------------------------------------- |
| 状态转换     | `mark_exiting()`, `force_clear()`                | 消费 self，返回下一个 view                                   |
| 字段访问     | `endpoint()`, `total()`, `flags()`, `acl()` 等    | 只读 getter                                            |
| 字段修改     | `set_endpoint()`, `add_total()`, `sub_total()` 等 | 修改单个字段                                               |
| 页表初始化    | `init_page_table()`                              | 创建空页表 + 映射内核，对应 Minix3 `pt_new()` + `pt_mapkernel()` |
| 页表绑定     | `bind_page_table()`                              | 将页表绑定到进程，对应 Minix3 `pt_bind()`                       |
| 区域初始化    | `init_regions()`                                 | 创建空区域树                                               |
| fork 初始化 | `init_from_fork()`, `copy_acl_from()`            | 设置 endpoint/内存统计/ACL                                 |
| CoW      | `setup_cow_for_all_regions()`                    | fork 后设置写时复制，增加 physblock 引用计数                       |
| 页表映射     | `write_page_table_mappings()`                    | 将物理映射写入页表，对应 Minix3 `map_writept()`                  |

**初始化顺序**：不同场景需要不同的初始化路径：

- **新进程（exec）**：`init_page_table()` → `init_regions()` → `bind_page_table()`
- **fork**：`init_from_fork()` → `copy_acl_from()` → `setup_cow_for_all_regions()` → `write_page_table_mappings()` → `bind_page_table()`

**ExitingProc**: 退出中进程的 view

```rust
pub(crate) struct ExitingProc<'a> {
    inner: &'a mut VmProc,
}

impl<'a> ExitingProc<'a> {
    /// 回收资源，槽位回到空闲
    pub(crate) unsafe fn reap(self) -> EmptySlot<'a> {
        unsafe { self.inner.clear(); }
        EmptySlot::new(self.inner)
    }
}
```

**设计意义**:

- **编译期保证**: `ActiveProc` 才能访问 `page_table()`/`regions()`，`EmptySlot` 不能
- **状态转换**: `activate()`/`mark_exiting()`/`reap()` 消费当前 view，返回下一个 view
- **资源安全**: `reap()`/`force_clear()` 需要 `unsafe`，强调调用者需确保页表不再被硬件使用
- **宽松模式**: `activate_relaxed()` 为 fork 等中间态场景提供灵活入口

**Typestate View vs 传统 Typestate**

传统 typestate 模式通过 **move 语义** 实现状态转换——消费 `self`，返回新类型，所有权随之转移：

```rust
// 传统 typestate：VmProc 被 move，地址改变 ❌
struct Uninitialized { inner: VmProc }
struct Initialized { inner: VmProc }

impl Uninitialized {
    fn init(self) -> Initialized {
        Initialized { inner: self.inner }  // VmProc 被 move！
    }
}
```

但 `VmProc` 必须在进程表中 **in-place**，地址不能变。move 语义会把 `VmProc` 从固定地址移走，违反地址稳定性要求。

真实代码采用 **typestate view**——通过 `&mut VmProc` 借用，而非拥有：

```rust
// Typestate view：VmProc 不动，只有借用权转移 ✅
pub(crate) struct EmptySlot<'a> {
    inner: &'a mut VmProc,   // 借用，不是拥有
}
pub(crate) struct ActiveProc<'a> {
    inner: &'a mut VmProc,   // 借用
}

impl<'a> EmptySlot<'a> {
    pub(crate) fn activate(self, endpoint: Endpoint) -> ActiveProc<'a> {
        self.inner.vm_flags = VmFlags::IN_USE;    // in-place 修改
        self.inner.vm_endpoint = endpoint;          // in-place 修改
        ActiveProc::new(self.inner)                 // 借用权转移，VmProc 不动
    }
}
```

**核心区别**：move 的是 view wrapper（`EmptySlot` → `ActiveProc`），不是 `VmProc` 本身。`VmProc` 始终待在进程表的固定地址上，只有 `&mut` 借用权在不同 view 之间传递。这就是为什么叫 **typestate view**——它是对 in-place 数据的"视角"，不是数据本身。

#### 6.1.3 状态转换示例

**fork 流程**（通过 typestate API，与实际 `fork.rs` 一致）:

```rust
// T0: 获取父进程信息
let parent_total;
let parent_total_max;
let parent_region_top;
{
    let parent = table.get_active(parent_slot)?;
    parent_total = parent.total();
    parent_total_max = parent.total_max();
    parent_region_top = parent.region_top();
}

// T1: 获取空闲槽位并激活（使用真实 child endpoint）
let empty = table.get_empty(child_slot)?;
let mut child = empty.activate(child_endpoint);

// T2: 初始化页表和区域
child.init_page_table()?;
child.init_regions();

// T3: 从父进程继承状态
child.init_from_fork(child_endpoint, parent_total, parent_total_max, parent_region_top);

// T4: 复制 ACL
let parent = table.get_active(parent_slot)?;
child.copy_acl_from(&parent);
```

**与 Minix3 的差异**:

| 方面       | Minix3                                          | Rust                                  |
| ---------- | ----------------------------------------------- | ------------------------------------- |
| endpoint 处理 | `*vmc = *vmp` 复制后显式设为 `NONE`，`sys_fork()` 后内核填充 | `activate(Endpoint::NONE)` 显式设为无效值 |
| 字段复制方式 | `*vmc = *vmp` 整体复制后逐字段修正                   | 逐字段通过 `init_from_fork()` 显式复制    |
| 页表初始化  | `pt_new()` 创建新页表                             | `init_page_table()` 创建新页表          |
| ACL 复制   | `acl_fork()`                                    | `copy_acl_from()`                     |

> **注意**: Minix3 使用 `*vmc = *vmp` 整体复制后逐字段修正，Rust 使用逐字段显式复制。Rust 的方式更安全（避免 origpt 保存/恢复问题），但需要确保所有必要字段都被复制。详见 [16-vm-fork.md](16-vm-fork.md) 的 fork 实现对比。

**exit 流程**（通过 typestate API）:

```rust
// T1: 标记为退出中
let exiting = active.mark_exiting();

// T2: 回收资源，槽位回到空闲
let empty = unsafe { exiting.reap() };
```

**Minix3 两步退出协议**:

Minix3 的退出是两个独立消息：

1. PM 发 `VM_WILLEXIT` → VM 设 `VMF_EXITING`（`do_willexit()`）
2. PM 再发 `VM_EXIT` → VM 清理资源（`do_exit()` → `free_proc()` + `clear_proc()`）

且 `do_exit()` 会检查 `VMF_EXITING` 标志——未经过 `VM_WILLEXIT` 的退出请求会被拒绝：

```c
// exit.c:73
if(!(vmp->vm_flags & VMF_EXITING)) {
    printf("VM: unannounced VM_EXIT %d\n", msg->VME_ENDPOINT);
    return EINVAL;
}
```

**Rust typestate 对应**:

| Minix3                                     | Rust typestate                                 | 说明             |
| ------------------------------------------ | ---------------------------------------------- | -------------- |
| `VM_WILLEXIT` → 设 `VMF_EXITING`            | `ActiveProc::mark_exiting()` → `ExitingProc`   | 运行时检查 → 编译期保证  |
| `VM_EXIT` → `free_proc()` + `clear_proc()` | `ExitingProc::reap()` → `EmptySlot`            | 资源释放 + 状态重置    |
| `do_exit()` 检查 `VMF_EXITING`               | `get_exiting()` 只返回 `IN_USE && EXITING` 的 slot | 运行时检查 → 类型系统保证 |

Rust typestate 将 Minix3 的运行时检查提升为编译期保证：你无法对 `ActiveProc` 调用 `reap()`，只有 `ExitingProc` 才有 `reap()` 方法。这等价于 Minix3 的 `VMF_EXITING` 检查，但在编译期就阻止了非法操作。

#### 6.1.4 状态检查

`VmProc` 的状态检查分为两层：

**运行时检查**（`VmProc` 自身，`debug_assertions` 模式）:

```rust
impl VmProc {
    pub(crate) fn is_in_use(&self) -> bool {
        self.vm_flags.contains(VmFlags::IN_USE)
    }
    pub(crate) fn is_exiting(&self) -> bool {
        self.vm_flags.contains(VmFlags::EXITING)
    }
    #[cfg(debug_assertions)]
    pub(crate) fn check(&self) {
        if self.vm_flags.contains(VmFlags::IN_USE) {
            debug_assert!(!self.vm_endpoint.is_none(), "IN_USE but vm_endpoint is NONE");
        }
    }
}
```

**编译期保证**（typestate view 层）:

| 检查       | Typestate View 保证                          | 说明          |
| -------- | ------------------------------------------ | ----------- |
| 只能操作活跃进程 | `ActiveProc` 才有 `page_table()`/`regions()` | 编译期阻止对空槽位调用 |
| 只能从活跃→退出 | `mark_exiting()` 消费 `ActiveProc`           | 编译期阻止非法状态跳转 |
| 退出后必须清理  | `ExitingProc` 只能 `reap()`                  | 编译期阻止忘记清理   |


#### 6.1.5 可见性设计：不导出 `VmProc`

Typestate view 能在编译期保证状态安全，前提是**外部代码无法绕过 view 直接操作** **`VmProc`**。这通过 Rust 模块系统的可见性控制实现。

**`VmProc` 的可见性**：

| 元素 | 可见性 | 说明 |
|------|--------|------|
| `VmProc` 类型 | 不导出 | vmproc 模块外部无法写出 `VmProc` 这个类型标识符（编译器禁止） |
| `VmProc` 字段 | `pub(crate)` | vmproc 模块树内部可直接访问 |
| `get_slot_mut()` | `pub(super)` | 仅 vmproc 模块树内部，用于测试和 view 构造 |

**`get_slot_mut()` 的可见性**：

`get_slot_mut()` 是 `pub(super)` 可见性（仅 vmproc 模块树内部可访问），返回 `&mut VmProc`。用于：
- `VmProcTable` 内部实现（`alloc_empty_slot`、`get_active` 等）
- typestate view 的构造（`EmptySlot::new`、`ActiveProc::new` 等）
- 模块内部测试

**Typestate view 的可见性**：

| 元素 | 可见性 | 说明 |
|------|--------|------|
| `EmptySlot`/`ActiveProc`/`ExitingProc` | `pub(crate)` | vm crate 内部通过 view 访问进程数据 |
| view 上的方法 | `pub(crate)` | 与 struct 可见性一致 |
| `VmFlags` | `pub(crate)` | view 方法返回值需要，vm crate 内部使用 |

**设计意图**：`VmProc` 不导出，外部代码（vm crate 的其他模块）只能通过 typestate view 操作进程。view 消费 self 实现状态转换，编译期阻止非法状态跳转。

**与 Minix3 的对比**：Minix3 中 `vmproc[]` 是全局数组，任何 VM 代码都可以直接 `vmproc[i].vm_flags = ...`，没有编译期保护。Rust 通过"不导出 `VmProc`"将 C 的隐式约定变成了编译期强制。

> **TODO**: 当 VM crate 稳定后，再次 review vmproc 模块对 vm crate 暴露的可见性。理论上 vmproc mod 应仅暴露 `VmProcTable` 的 get_handle 方法（如 `get_empty`/`get_active`/`get_exiting`）和 typestate view handle，其余均不暴露。当前 `VmFlags`、`VM_PROC_COUNT` 等的重导出可能过于宽松。

### 6.2 fork 流程中的状态一致性

#### 6.2.1 中间状态的风险

fork 流程中存在**中间状态**（slot 已分配，但 endpoint 尚未生成）：

```
PM 分配 slot=5          VM 初始化 vmproc        sys_fork 成功
    │                        │                       │
    ▼                        ▼                       ▼
┌─────────┐            ┌─────────────┐         ┌─────────────┐
│ slot=5  │ ────────→  │ slot=5      │ ──────→ │ slot=5      │
│ 空闲    │   VM_FORK  │ endpoint=NONE│         │ endpoint=OK │
└─────────┘            │ flags=EMPTY │         │ flags=IN_USE│
                       └─────────────┘         └─────────────┘
                              │
                              ▼
                       【中间状态窗口】
                       持续时间: ~μs
```

**风险**: 如果 VM 在此窗口崩溃，slot=5 处于"半初始化"状态。

#### 6.2.2 TOCTOU 防护

**问题场景**（Time-of-Check to Time-of-Use）:

```
T0: PM 检查父进程活跃（OK）
T1: PM 发送 VM_FORK 消息给 VM
T2: 【竞争窗口】父进程崩溃，slot 被释放
T3: 新进程重用 slot，生成新 endpoint
T4: VM 收到消息，使用旧 endpoint 操作
```

**后果**: VM 拿着旧 endpoint 错误操作新进程的内存！

**核心防护:** **`vm_isokendpt()`** **的三重检查**：

Minix3 和 Rust 代码使用相同的防护策略——在操作前验证 endpoint 的一致性：

```rust
// os/servers/vm/src/vmproc/table.rs
pub(crate) fn vm_isokendpt(&self, endpoint: Endpoint) -> Option<UserSlot> {
    let vm_slot = endpoint.slot();
    if vm_slot < 0 || vm_slot as usize >= VM_PROC_COUNT {
        return None;                          // 检查 1: slot 范围
    }
    let proc = /* get proc at slot */;

    if proc.vm_endpoint != endpoint {
        return None;                          // 检查 2: endpoint 匹配（关键！）
    }

    if !proc.vm_flags.contains(VmFlags::IN_USE) {
        return None;                          // 检查 3: 进程活跃
    }

    Some(slot_idx)
}
```

**检查 2 是 TOCTOU 防护的关键**：如果父进程已退出、slot 被新进程重用，`vm_endpoint` 一定不同（endpoint 含 generation 字段），验证失败。

**fork 入口处的使用**：

```rust
// os/servers/vm/src/fork.rs
pub(crate) fn handle_fork(table: &VmProcTable, request: &VmForkRequest, ...) -> Result<...> {
    let parent_slot = table.vm_isokendpt(request.parent_endpoint)
        .ok_or(VmForkError::ParentNotFound)?;  // TOCTOU 检查点
    ...
}
```

**为什么单线程下仍需防护？** VM 虽然是单线程的，但 PM → VM 是跨进程消息。PM 在 T0 检查父进程时和 T4 VM 处理消息时，中间父进程可能已经退出。**这不是并发竞争问题，而是分布式一致性问题**——VM 与 PM/Kernel 构成分布式系统，消息传递存在"检查与使用之间的时间窗口"。`vm_isokendpt()` 的 endpoint 匹配检查是分布式系统中的"乐观并发控制"：假设操作可能过期，在执行前重新验证。

> **详见**: [00-vm-overview.md](00-vm-overview.md) 的 TOCTOU 与分布式一致性分析。

**typestate 对 TOCTOU 的额外保护**：

| 防护层              | 机制                                | 保护范围         |
| ---------------- | --------------------------------- | ------------ |
| `vm_isokendpt()` | endpoint 一致性验证                    | 防止操作错误进程     |
| `get_active()`   | typestate 检查 `IN_USE && !EXITING` | 防止操作非活跃进程    |
| `get_empty()`    | typestate 检查 `!IN_USE`            | 防止重用非空闲 slot |

#### 6.2.3 失败安全

如果 fork 在中间状态失败，必须清理资源。

**Minix3 的 fork 失败路径存在 slot 泄漏**：

```c
// minix3/minix/servers/vm/fork.c
*vmc = *vmp;                    // 此时 vmc->vm_flags 已包含 VMF_INUSE
vmc->vm_endpoint = NONE;

if(pt_new(&vmc->vm_pt) != OK) {
    return ENOMEM;              // ← 没有调用 clear_proc(vmc)！
}

if(map_proc_copy(vmc, vmp) != OK) {
    pt_free(&vmc->vm_pt);
    return ENOMEM;              // ← 同样没有调用 clear_proc(vmc)！
}
```

fork 失败后，child slot 在 VM 看来仍然是 `VMF_INUSE`，但实际上是个"脏"状态——`INUSE` 置位但进程不可用。Minix3 依赖 **PM 的隐式协议**保证安全：PM 知道 fork 失败了，不会再使用这个 slot；下次 fork 时 `*vmc = *vmp` 无条件覆盖，脏数据不会被观察到。

**为什么 Minix3 不直接** **`clear_proc(vmc)`？** 完全可以——失败时 child 的状态完全可以安全清理。Minix3 没有这样做，纯粹是依赖单线程 + PM 隐式协议的"够用就行"设计。详见 [16-vm-fork.md](16-vm-fork.md) 的 fork 失败处理分析。

**Rust typestate 体系下，`force_clear()`** **是必要操作**：

```rust
// Rust 的 get_empty() 会检查 !IN_USE：
pub(crate) fn get_empty(&self, slot: UserSlot) -> Option<EmptySlot<'_>> {
    let vmp = self.get_proc(slot);
    if vmp.vm_flags.contains(VmFlags::IN_USE) {
        return None;  // ← 脏 slot 永远拿不到 EmptySlot！
    }
    Some(EmptySlot::new(vmp))
}
```

如果 fork 失败后不清理，`IN_USE` 仍然置位，`get_empty()` 永远返回 `None`——**slot 永久泄漏**。没有 C 那种"无条件 `*vmc = *vmp` 覆盖"的逃生通道。

**正确的回滚路径**：`ActiveProc → force_clear() → EmptySlot`

```rust
let empty = table.get_empty(child_slot)?;
let mut child = empty.activate(child_endpoint);

if let Err(_) = child.init_page_table() {
    unsafe { child.force_clear() };  // 回滚到 EmptySlot
    return Err(VmForkError::OutOfMemory);
}
```

**为什么不用** **`ExitingProc`？** 语义不匹配：

- `ExitingProc` 表示"进程曾运行，现在退出"（`mark_exiting` → `reap`）
- fork 失败是"进程从未运行，撤销激活"（`force_clear`）

**为什么** **`force_clear`** **是** **`unsafe`？** 因为调用者必须保证页表不再被硬件使用。但在 fork 失败场景下，`pt_bind()` 还没调用，页表从未绑定到硬件，所以条件天然成立。`VmProc::clear()` 也正确处理了"页表未初始化"的情况：

```rust
pub unsafe fn clear(&mut self) {
    if self.vm_regions_initialized {   // 只在已初始化时才清理
        unsafe { self.vm_regions.assume_init_mut().clear(); }
    }
    if self.vm_pt_initialized {            // 只在已初始化时才清理
        unsafe { self.vm_pt.assume_init_mut().destroy(); }
    }
    self.vm_flags = VmFlags::empty();      // 清除所有标志
    ...
}
```

**对比总结**：

| <br />  | Minix3                    | Rust typestate + `force_clear`         |
| ------- | ------------------------- | -------------------------------------- |
| fork 失败 | 不清理，依赖 PM 隐式协议            | `force_clear()` 显式回滚                   |
| 状态一致性   | 脏 slot（`INUSE` 但不可用）      | 干净的 `EmptySlot`                        |
| 错误可发现性  | 脏 slot 只在遍历 `INUSE` 进程时暴露 | 不存在脏状态，不可能误用                           |
| 回滚路径    | 无（依赖下次 fork 覆盖）           | `ActiveProc → force_clear → EmptySlot` |

***

## 7. 测试与验证

本章列出为保证 `VmProc` 结构体设计正确性应覆盖的测试维度。每个维度说明**测什么、为什么测**，辅以少量关键断言示例，不罗列完整测试代码。

### 7.1 测试基础设施

测试使用全局进程表 `VmProcTable::get_global()`，每个用例需先调用 `reset_slot()` 清理 slot：

```rust
#[cfg(test)]
pub unsafe fn reset_slot(&self, slot: UserSlot) {
    if let Some(proc) = self.get_slot_mut(slot) {
        if proc.vm_flags.contains(VmFlags::IN_USE) { proc.clear(); }
        core::ptr::write(proc, VmProc::vacant_with_slot(slot));
    }
}
```

**两步清理**：先 `clear()` 清除 `IN_USE`（避免 Drop panic），再 `ptr::write` 覆盖（跳过 Drop）。此函数标记 `#[cfg(test)]`——生产代码通过 typestate API 转换状态，不应绕过。

**测试中的生命周期扩展**：`vmproc_handle.rs` 的测试辅助函数使用 `transmute` 将 `ActiveProc<'_>` 转为 `ActiveProc<'static>`：

```rust
fn get_active_vmproc(slot: UserSlot) -> ActiveProc<'static> {
    let table = VmProcTable::get_global();
    unsafe { table.reset_slot(slot); }
    let empty = table.get_empty(slot).unwrap();
    let active = empty.activate(Endpoint::from_generation_slot(1, slot.get() as i32));
    // SAFETY: 全局表生命周期 = 程序运行期，等价于 'static
    unsafe { core::mem::transmute::<ActiveProc<'_>, ActiveProc<'static>>(active) }
}
```

**为什么需要** **`transmute`？** `VmProcTable::get_global()` 返回 `&'static VmProcTable`，但 `get_empty()` 返回的 `EmptySlot<'_>` 的生命周期被推导为 `'_`（匿名生命周期），而非 `'static`。这是因为 Rust 的类型推导在 `&self` 方法中不会自动将返回值的生命周期提升到 `'static`。测试需要返回 `ActiveProc<'static>` 以便在多个断言间自由使用（不受局部借用限制）。

**为什么安全？** 全局表 `VM_PROC_TABLE` 是 `static` 变量，生命周期等于程序运行期，与 `'static` 语义一致。`transmute` 只是将编译器无法自动推导的事实显式声明。

### 7.2 结构体构造与初始状态

| 测试点                   | 验证目标                    | 关键断言                                                                    |
| --------------------- | ----------------------- | ----------------------------------------------------------------------- |
| `vacant()` 初始状态       | 新构造的 VmProc 所有字段处于安全默认值 | `endpoint == NONE`, `flags.is_empty()`, `!is_in_use()`, `!is_exiting()` |
| `vacant_with_slot(n)` | slot 编号正确设置             | `vm_slot == UserSlot(n)`, 其余同 vacant                                    |
| MaybeUninit 字段        | 未初始化字段不可访问              | `vm_pt_initialized == false`, `vm_regions_initialized == false`     |

### 7.3 字段访问与一致性

| 测试点                 | 验证目标                     | 关键断言                                                                        |
| ------------------- | ------------------------ | --------------------------------------------------------------------------- |
| endpoint 设置/读取      | getter/setter 正确传递       | `set_endpoint(ep); assert_eq!(endpoint(), ep)`                              |
| slot 与 endpoint 一致性 | `UserSlot::matches()` 语义 | `UserSlot(5).matches(Endpoint::from_generation_slot(1, 5)) == true`         |
| slot 与 endpoint 不一致 | 检测不匹配                    | `UserSlot(5).matches(Endpoint::from_generation_slot(1, 3)) == false`        |
| 内存统计：`add_total`    | 累加 + total\_max 自动更新     | `add_total(1024); assert_eq!(total(), 1024); assert_eq!(total_max(), 1024)` |
| 内存统计：`sub_total`    | 饱和减法                     | `sub_total(500); total() 不会下溢`                                              |
| 页错误计数器              | 递增正确                     | `inc_minor_fault(); assert_eq!(minor_fault(), 1)`                           |

### 7.4 状态标志（VmFlags）

| 测试点      | 验证目标                   | 关键断言                                                    |
| -------- | ---------------------- | ------------------------------------------------------- |
| 基本标志操作   | insert/remove/contains | `flags.insert(IN_USE); assert!(flags.contains(IN_USE))` |
| 标志组合     | 多标志共存                  | `IN_USE \| EXITING` 同时包含两者                              |
| 默认值      | 空 flags                | `VmFlags::default().is_empty() == true`                 |
| 位值与 C 一致 | bitflags 值匹配 C 头文件     | `IN_USE.bits() == 0x001`, `EXITING.bits() == 0x002`     |

### 7.5 Drop 行为

| 测试点            | 验证目标               | 关键断言                                               |
| -------------- | ------------------ | -------------------------------------------------- |
| 空 slot Drop    | 无 IN\_USE 时安全 Drop | `let proc = VmProc::vacant(); drop(proc);` 不 panic |
| IN\_USE 时 Drop | 检测资源泄漏             | `proc.vm_flags \|= IN_USE; drop(proc);` 应 panic    |

> Drop 在 `IN_USE` 时 panic 是**设计意图**：捕获"活跃进程被意外 Drop"的资源泄漏。生产代码通过 `clear()` 或 `reap()` 先清除标志再释放。

### 7.6 clear() 安全性

| 测试点                | 验证目标                                                                     |
| ------------------ | ------------------------------------------------------------------------ |
| clear() 重置所有字段     | 调用后 `flags.is_empty()`, `endpoint == NONE`, `vm_pt_initialized == false` |
| clear() 后 slot 可重用 | clear 后能再次 activate                                                      |

### 7.7 与 Minix3 行为对比

| 测试点                         | 验证目标                          |
| --------------------------- | ----------------------------- |
| 字段映射完整性                     | C 的每个 `vmproc` 字段在 Rust 中都有对应 |
| `vm_isokendpt` 语义一致         | 相同输入下 Rust 与 C 返回相同结果         |
| `clear_proc()` vs `clear()` | 两者都重置为空闲状态                    |

### 7.8 测试维度总结

```
VmProc 测试覆盖
├── 构造：vacant / vacant_with_slot
├── 字段访问：endpoint / slot / total / stats
├── 一致性：slot ↔ endpoint 匹配
├── 状态标志：VmFlags 操作 + 位值与 C 一致
├── 生命周期：Drop 行为（IN_USE 时 panic）
├── 清理：clear() 完整重置
└── 兼容性：与 Minix3 C 实现行为一致
```

## 8. 参见

- [00-vm-overview.md](00-vm-overview.md) - VM 整体架构（地址稳定性等全局原则）
- [02-vmproc-table.md](02-vmproc-table.md) - 进程表管理
- [03-acl.md](03-acl.md) - 访问控制
- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构
- [11-region-mapping.md](11-region-mapping.md) - 虚拟区域与页映射

***

*分类: VM私有 | 全局概念: endpoint\_t (minix/endpoint.h)*
