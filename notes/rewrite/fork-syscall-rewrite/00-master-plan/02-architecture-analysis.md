# 五层架构深度分析

## 1. 架构概述

## 2. 数据流分析

### Minix3 多进程表架构

> **重要**: Minix3 采用分布式进程表设计，共有 **5 份进程表**，分别由不同组件管理：

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                        Minix3 进程表分布                                            │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐         │
│  │  Kernel  │  │    PM    │  │    VM    │  │   VFS    │  │  SCHED   │         │
│  │ proc[]   │  │ mproc[]  │  │vmproc[]  │  │ fproc[]  │  │schedproc[]│        │
│  └─────┬────┘  └─────┬────┘  └─────┬────┘  └─────┬────┘  └─────┬────┘         │
│        │             │             │             │             │               │
│        │  endpoint   │  endpoint   │  endpoint   │  endpoint   │  endpoint     │
│        │  proc_nr    │  mp_pid     │             │  fp_pid     │  parent       │
│        ▼             ▼             ▼             ▼             ▼               │
│  ┌──────────────────────────────────────────────────────────────────────────────┐│
│  │                    通过 endpoint / proc_nr 关联                                ││
│  └──────────────────────────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────────────────────┘
```

### 各进程表文件对应关系

| 组件 | C 文件 | Rust 文件 | 进程表名 | 说明 |
|------|--------|-----------|---------|------|
| **Kernel** | `minix/kernel/proc.h` | `os/kernel/src/proc.rs` | `proc[NR_TASKS + NR_PROCS]` | 内核进程表，包含任务和用户进程 |
| **PM** | `minix/servers/pm/mproc.h` | `os/servers/pm/src/mproc/mproc.rs` | `mproc[NR_PROCS]` | PM 进程表，进程管理信息 |
| **VM** | `minix/servers/vm/vmproc.h` | `os/servers/vm/src/vmproc.rs` | `vmproc[NR_PROCS]` | VM 进程表，虚拟内存信息 |

**PM mproc 子模块**:
- `os/servers/pm/src/mproc/mproc.rs` - Process 结构体
- `os/servers/pm/src/mproc/lifecycle.rs` - 生命周期
- `os/servers/pm/src/mproc/block.rs` - 阻塞状态
- `os/servers/pm/src/mproc/wait.rs` - 等待状态
- `os/servers/pm/src/mproc/guardianship.rs` - 监护关系
- `os/servers/pm/src/mproc/trace.rs` - 追踪状态
- `os/servers/pm/src/mproc/signal.rs` - 信号状态
- `os/servers/pm/src/mproc/credentials.rs` - 权限凭证
- `os/servers/pm/src/mproc/table.rs` - 进程表管理
- `os/servers/pm/src/mproc/context.rs` - PmContext
- `os/servers/pm/src/mproc/fork.rs` - fork 实现
- `os/servers/pm/src/mproc/pid_gen.rs` - PID 生成器
| **VFS** | `minix/servers/vfs/fproc.h` | `os/servers/vfs/src/fproc.rs` | `fproc[NR_PROCS]` | VFS 进程表，文件系统信息 |
| **SCHED** | `minix/servers/sched/schedproc.h` | (待实现) | `schedproc[NR_PROCS]` | SCHED 进程表，调度信息 |

### 各进程表关键字段对比

| 字段类型 | Kernel (proc) | PM (mproc) | VM (vmproc) | VFS (fproc) | SCHED (schedproc) |
|---------|---------------|------------|-------------|-------------|-------------------|
| **标识** | `p_endpoint` | `mp_endpoint`, `mp_pid` | `vm_endpoint` | `fp_endpoint`, `fp_pid` | `endpoint` |
| **索引** | `p_nr` | 数组索引 | `vm_slot` | 数组索引 | 数组索引 |
| **状态** | `p_rts_flags` | `mp_flags` | `vm_flags` | `fp_flags` | `flags` |
| **父进程** | - | `mp_parent` | - | - | `parent` |
| **权限** | `p_priv` | `mp_realuid`, `mp_effuid` | - | `fp_realuid`, `fp_effuid` | - |
| **内存** | - | - | `vm_pt`, `vm_regions_avl` | - | - |
| **文件** | - | - | - | `fp_filp[]`, `fp_wd`, `fp_rd` | - |
| **调度** | `p_priority` | - | - | - | `priority`, `time_slice`, `cpu` |

### 各进程表核心职责

| 进程表 | 核心职责 | 关键字段 |
|--------|----------|----------|
| **kernel/proc** | 调度、IPC、寄存器 | `p_reg`, `p_rts_flags`, `p_priority`, `p_endpoint` |
| **PM/mproc** | 进程管理、信号、权限 | `mp_pid`, `mp_parent`, `mp_flags`, `mp_realuid` |
| **VM/vmproc** | 虚拟内存、页表 | `vm_pt`, `vm_regions_avl`, `vm_total` |
| **VFS/fproc** | 文件描述符、目录 | `fp_filp`, `fp_wd`, `fp_rd`, `fp_tty` |
| **SCHED/schedproc** | 调度参数、CPU 选择 | `priority`, `time_slice`, `cpu`, `max_priority` |

## 3. Endpoint 与 Generation

### Endpoint 格式详解

Minix3 的 Endpoint 格式（源码：`minix/include/minix/endpoint.h`）：

```c
#define _ENDPOINT_GENERATION_SHIFT  15
#define _ENDPOINT(g, p) ((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
```

```
endpoint = (generation << 15) + proc_nr

┌────────────────────────────────┬───────────────────────┐
│         高 17 位               │       低 15 位         │
│        generation              │      proc_nr          │
│    （代数，防止过时消息）        │   （进程槽位号）        │
└────────────────────────────────┴───────────────────────┘
```

**Generation 的维护**：
- 嵌入在 `endpoint` 中，**不需要单独存储**
- 每次槽位释放时 generation +1
- 作用：防止"过时的消息发给新进程"

### Rust 实现

```rust
/// 生成 Endpoint
/// 
/// # 参数
/// - generation: 代数（每次槽位重用时递增）
/// - proc_nr: 进程槽位号（低15位）
/// 
/// # 返回值
/// 组合后的 endpoint 值
pub fn make_endpoint(generation: u16, proc_nr: u16) -> Endpoint {
    Endpoint(((generation as u32) << 15) | (proc_nr as u32))
}

/// 从 endpoint 提取槽位号
pub fn get_proc_nr(endpoint: Endpoint) -> u16 {
    (endpoint.0 & 0x7FFF) as u16  // 低15位
}

/// 从 endpoint 提取 generation
pub fn get_generation(endpoint: Endpoint) -> u16 {
    (endpoint.0 >> 15) as u16  // 高17位
}
```

## 4. 索引强对齐约束

> ⚠️ **关键约束**: Minix3 的五份进程表之间存在**物理级的索引对齐**关系。

这种"强约束"是 Minix3 设计的核心特征，也是现代开发者感到不适的根源。

### 什么是索引强对齐？

在 Minix3 中，PM 的 `mproc[5]`、VM 的 `vmproc[5]`、VFS 的 `fproc[5]`、Kernel 的 `proc[5]`、SCHED 的 `schedproc[5]`
描述的是**同一个进程**的不同侧面。这个索引（槽位号）是跨服务的"物理标识符"。

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        索引强对齐示意                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  进程 A (index=5)                                                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │
│  │ mproc[5]    │  │ vmproc[5]   │  │ fproc[5]    │  │ proc[5]     │       │
│  │ PID, 信号   │  │ 页表, 内存  │  │ 文件描述符  │  │ 调度状态    │       │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘       │
│        ▲                ▲                ▲                ▲                │
│        │                │                │                │                │
│        └────────────────┴────────────────┴────────────────┘                │
│                          同一个索引 = 同一个进程                              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 这种设计的优势

| 优势 | 说明 |
|------|------|
| **Endpoint 瞬间转换** | 内核收到消息后，只需 `index = endpoint & mask` 即可定位，无需 Hash 表、无需搜索、无需锁 |
| **同步逻辑简化** | PM 告诉 VFS "5 号槽位 fork 了，新进程在 8 号槽位"，VFS 直接去 8 号槽位初始化即可 |
| **隐式共识** | 各服务共享同一套"物理地址空间（槽位号）"，成为跨服务的隐式协议 |

### 这种设计的劣势

| 劣势 | 说明 |
|------|------|
| **扩展性极差** | 若 PM 支持 1024 进程，但 VFS 只编译了 256，整个系统崩溃。所有服务必须为 `NR_PROCS` 重新编译 |
| **资源浪费** | 纯计算进程不打开文件，仍需在 VFS 的 `fproc` 表占坑位 |
| **容错风险** | 若 index 对齐出错（PM 释放 8 号槽位但 VFS 未收到通知），会发生严重"身份错乱" |

### Rust 重写中的处理策略

面对这种"物理级耦合"，在 Rust 重写中有两条路：

| 策略 | 说明 | 适用场景 |
|------|------|----------|
| **复刻派** | 继续使用 `NR_PROCS` 数组，接受 index 对齐 | 忠实复刻 Minix3，保持 IPC 协议兼容 |
| **进化派** | 各服务自管槽位，只有 `Pid`/`Endpoint` 唯一 | 现代分布式设计，但需要实现内核 Hash 表 |

**当前阶段采用"复刻派"**，理由：
1. 我们复刻的是 Minix3，index 对齐是其 IPC 协议的一部分
2. 在 `no_std` 且无分配器的内核早期，`HashMap<Pid, SlotIndex>` 会引入巨大复杂度

### 类型系统提醒

> **建议**: 将 `SlotIndex` 封装为独立类型（而非 `usize`），通过类型系统提醒：
> 这个 Index 不是普通数字，而是跨服务的"物理标识符"。

```rust
/// 进程槽位索引（跨服务强对齐）
/// 
/// # 安全性
/// 
/// 此索引必须在 PM、VM、VFS、Kernel、SCHED 五份进程表中保持一致。
/// 任何不同步都会导致严重的"身份错乱"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotIndex(pub usize);

// NOTE: This index MUST be synchronized with VFS/VM/Kernel due to Minix3's design.
```

## 5. 消息序列图

### Fork 调用流程（跨服务器协调）

```
用户进程 fork()
    │
    ▼ 系统调用
┌─────────────────────────────────────────┐
│  PM (Process Manager)                   │
│  ┌─────────────┐  ┌─────────────┐      │
│  │ do_fork()   │  │ get_free_pid│      │
│  │ mproc 表管理│  │ PID生成器   │      │
│  └──────┬──────┘  └─────────────┘      │
│         │                              │
│         │ ① 分配 mproc 槽位            │
│         │ ② 生成新 PID                 │
│         │ ③ 复制父进程 mproc           │
│         │                              │
└─────────┼───────────────────────────────┘
          │ IPC: vm_fork()
          ▼
┌─────────────────────────────────────────┐
│  VM (Virtual Memory)                    │
│  ┌─────────────┐                        │
│  │ vm_fork()   │                        │
│  │ vmproc 表管理│                       │
│  └──────┬──────┘                        │
│         │                              │
│         │ ④ 分配 vmproc 槽位            │
│         │ ⑤ 复制父进程地址空间          │
│         │ ⑥ 生成新 endpoint             │
│         │                              │
└─────────┼───────────────────────────────┘
          │ IPC: VFS_PM_FORK
          ▼
┌─────────────────────────────────────────┐
│  VFS (Virtual File System)              │
│  ┌─────────────┐                        │
│  │ fproc 表管理│                        │
│  └──────┬──────┘                        │
│         │                              │
│         │ ⑦ 分配 fproc 槽位             │
│         │ ⑧ 复制文件描述符表            │
│         │ ⑨ 复制工作目录                │
│         │                              │
└─────────┼───────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────┐
│  Kernel                                 │
│  ┌─────────────┐                        │
│  │ sys_fork()  │                        │
│  │ proc 表管理 │                        │
│  └─────────────┘                        │
│                                         │
│  ⑩ 分配 proc 槽位                       │
│  ⑪ 初始化调度状态                       │
└─────────────────────────────────────────┘
```

### Fork 调用跨服务器协调（详细流程）

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Fork 调用跨服务器协调                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ① PM: do_fork()                                                           │
│     ├── 检查进程表是否已满                                                   │
│     ├── 查找空闲 mproc 槽位                                                 │
│     ├── 生成新 PID                                                          │
│     └── 复制父进程 mproc                                                    │
│                                                                             │
│  ② PM → VM: vm_fork() IPC 调用                                             │
│     ├── VM 分配 vmproc 槽位                                                 │
│     ├── VM 复制父进程地址空间                                               │
│     ├── VM 生成新 endpoint                                                  │
│     └── VM 返回 endpoint 给 PM                                              │
│                                                                             │
│  ③ PM → VFS: tell_vfs(VFS_PM_FORK) IPC 调用                               │
│     ├── VFS 分配 fproc 槽位                                                 │
│     ├── VFS 复制文件描述符表                                                │
│     ├── VFS 复制工作目录                                                    │
│     └── VFS 回复 PM                                                         │
│                                                                             │
│  ④ PM: 返回 SUSPEND，等待 VFS 回复                                         │
│                                                                             │
│  ⑤ Kernel: sys_fork() (由 VM 触发)                                         │
│     ├── 分配 proc 槽位                                                      │
│     └── 初始化调度状态                                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 6. 状态转换

## 7. 关键约束

### Fork 实现的关键约束

1. **PM 调用 vm_fork 后不能失败**：VM 已经调用了 sys_fork() 创建了内核进程
2. **Endpoint generation**：每次槽位重用时 generation 递增，防止旧消息误投递
3. **SUSPEND 机制**：PM 返回 SUSPEND 给内核，等待 VFS 回复后才唤醒父进程

## 8. 关键设计决策

### Rust 重构策略

由于 Minix3 的分布式设计，我们的 Rust 重构需要：

1. **分层定义进程表结构体**：
   - `minix-pm` crate → `mproc` 表（PM 私有）
   - `minix-vm` crate → `vmproc` 表（VM 私有）
   - `minix-vfs` crate → `fproc` 表（VFS 私有）
   - `minix-kernel` crate → `proc` 表（Kernel 私有）
   - `minix-types` crate → 核心协议类型（Endpoint, Pid 等）

2. **统一索引关联**（遵循索引强对齐约束）：
   - 使用 `proc_nr`（进程索引）作为统一标识
   - 使用 `endpoint` 作为跨服务器标识
   - **注意**: 索引必须在四份进程表中保持一致（参见"进程表索引强对齐约束"）

3. **当前阶段聚焦 PM**：
   - 第一阶段只处理 `mproc` 表
   - 后续阶段逐步加入 VM、VFS 支持

### 三层架构

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                        Rust Crate 架构                                              │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐         │
│  │ minix-pm │  │ minix-vm │  │ minix-vfs│  │  kernel  │  │minix-sched│         │
│  │          │  │          │  │          │  │          │  │          │         │
│  │mproc(私) │  │vmproc(私)│  │fproc(私) │  │proc(私有)│  │schedproc │         │
│  │ Process  │  │ VmProcess│  │FProc     │  │ KProcess │  │SchedProc │         │
│  │ProcTable │  │VmProcTab │  │FProcTable│  │KProcTable│  │SchedTable│         │
│  │PmContext │  │          │  │          │  │          │  │          │         │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘         │
│       │              │              │              │              │             │
│       │  Endpoint    │  Endpoint    │  Endpoint    │  Endpoint    │  Endpoint   │
│       │  Pid         │  Pid         │  Pid         │  Pid         │  Pid        │
│       │  (来自 minix-types)                        │              │             │
│       ▼              ▼              ▼              ▼              ▼             │
│  ┌──────────────────────────────────────────────────────────────────────────────┐│
│  │                    minix-types (核心协议层)                                    ││
│  │                    Endpoint, Pid, ProcIndex, Uid, Gid...                     ││
│  └──────────────────────────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────────────────────┘
```
