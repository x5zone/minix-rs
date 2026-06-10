# 09-priv-struct - 特权结构体

> 本文档分析 `minix3/minix/kernel/priv.h` 第 1-60 行，讲解特权结构体的定义。

---

## 1. 概述

特权结构体（`struct priv`）是 Minix3 内核中用于管理进程特权的核心数据结构。每个系统进程拥有自己独立的特权结构体，而所有用户进程共享同一个特权结构体。这种设计实现了特权的最小化原则，确保进程只能访问其被授权的资源。

### 1.1 特权管理设计

Minix3 采用**基于能力的特权管理模型**（Capability-based Privilege Management），其核心设计原则包括：

**1. 特权分离（Privilege Separation）**

系统进程和用户进程拥有完全不同的特权集合。系统进程（如 VM、PM、VFS 等）拥有特定的内核调用权限和 IPC 通信权限，而用户进程仅有最基本的权限。

**2. 最小权限原则（Principle of Least Privilege）**

每个进程只被授予完成其任务所必需的最小权限集合。例如：
- 文件系统服务器只需要与磁盘 I/O 相关的权限
- 进程管理器只需要与进程控制相关的权限
- 普通用户进程几乎没有特殊权限

**3. 特权表结构（Privilege Table Structure）**

```
┌─────────────────────────────────────────────────────────────┐
│                    Privilege Table                          │
├──────────────┬────────────────────────────────────────────┤
│ Static Privs │ 预定义系统进程的特权（VM, PM, VFS, etc.）    │
│ (0-15)       │ 在编译时确定，不可更改                       │
├──────────────┼────────────────────────────────────────────┤
│ Dynamic Privs│ 运行时动态分配的特权                         │
│ (16-31)      │ 用于用户进程和动态创建的系统进程              │
└──────────────┴────────────────────────────────────────────┘
```

**4. 特权标识（Privilege Identification）**

每个特权结构体通过 `s_id` 字段标识其在特权表中的索引，通过 `s_proc_nr` 字段关联到具体的进程。

### 1.2 系统进程与用户进程

Minix3 区分两种类型的进程，它们在特权管理上有本质区别：

**系统进程（System Processes）**

| 特性 | 说明 |
|------|------|
| 特权结构 | 每个系统进程拥有独立的 `struct priv` |
| 权限范围 | 拥有特定的内核调用权限和 IPC 权限 |
| 标识 | `s_flags & SYS_PROC` 标志设置 |
| 示例 | VM（虚拟内存）、PM（进程管理）、VFS（虚拟文件系统）、RS（重启动服务器）等 |

系统进程特权结构包含：
- **系统调用掩码**（`s_trap_mask`）：允许的系统调用陷阱号
- **IPC 目标映射**（`s_ipc_to`）：允许通信的目标进程
- **内核调用掩码**（`s_k_call_mask`）：允许的内核调用号
- **I/O 端口/内存范围**（`s_io_tab`, `s_mem_tab`）：允许访问的硬件资源

**用户进程（User Processes）**

| 特性 | 说明 |
|------|------|
| 特权结构 | 所有用户进程共享同一个 `struct priv` |
| 权限范围 | 仅有最基本的权限，无特殊内核调用权限 |
| 标识 | `s_flags & SYS_PROC` 标志未设置 |
| 共享 ID | `USER_PRIV_ID`（通常是特权表中的最后一个条目） |

用户进程特权限制：
- 只能进行基本的 IPC 通信
- 无硬件 I/O 访问权限
- 无特殊内核调用权限
- 受 VM 管理的内存访问控制

**进程类型对比表**

```
┌─────────────────┬──────────────────────┬──────────────────────┐
│     特性        │      系统进程         │      用户进程         │
├─────────────────┼──────────────────────┼──────────────────────┤
│ 特权结构体      │ 每个进程独立          │ 所有进程共享          │
│ s_id 分配       │ 静态分配 (0-15)       │ 动态分配 (16+)        │
│ SYS_PROC 标志   │ 设置                  │ 未设置                │
│ 内核调用权限    │ 有特定权限            │ 无                    │
│ I/O 端口访问    │ 有特定范围            │ 无                    │
│ IPC 权限        │ 受限的目标集          │ 基本权限              │
│ fork 子进程     │ 降级为用户进程        │ 保持用户进程          │
└─────────────────┴──────────────────────┴──────────────────────┘
```

### 1.3 与 fork 的关系

特权结构体在 `do_fork()` 系统调用中扮演着关键角色，其处理逻辑直接影响系统的安全性和隔离性。

**Fork 时的特权处理流程**

```
父进程 (系统进程)                     子进程 (新创建)
     │                                     │
     │  do_fork()                         │
     │──────────────────────────────────>│
     │                                     │
     │  检查父进程类型                      │
     │  priv(rpp)->s_flags & SYS_PROC    │
     │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
     │         │                           │
     │         │ 是                        │
     │         v                           │
     │  降级处理:                          │
     │  rpc->p_priv =                     │
     │  priv_addr(USER_PRIV_ID)           │
     │                                     │
     │  RTS_SET(rpc, RTS_NO_PRIV)         │
     │  (设置禁止运行标志)                  │
     │                                     │
     │<──────────────────────────────────│
     │         子进程创建完成                │
     │         等待 RS 授权                 │
```

**特权继承规则**

1. **用户进程 fork**
   - 子进程继承父进程的 `p_priv` 指针（指向共享的用户特权结构）
   - 子进程保持为用户进程，无特殊权限
   - 无需特权降级处理

2. **系统进程 fork**
   - 子进程的 `p_priv` 被显式设置为 `priv_addr(USER_PRIV_ID)`
   - 子进程从系统进程降级为普通用户进程
   - `RTS_NO_PRIV` 标志被设置，阻止子进程运行
   - 子进程必须通过 RS（重启动服务器）的授权才能恢复运行

**安全意义**

```
┌─────────────────────────────────────────────────────────────────┐
│                    Fork 特权处理的安全目标                       │
├─────────────────────────────────────────────────────────────────┤
│ 1. 防止特权扩散                                                  │
│    - 系统进程 fork 时，子进程不自动继承特权                        │
│    - 必须通过 RS 服务显式授权                                      │
├─────────────────────────────────────────────────────────────────┤
│ 2. 强制审计点                                                    │
│    - RTS_NO_PRIV 标志强制子进程进入"禁止运行"状态                   │
│    - RS 服务必须通过 sys_privctl() 授权                          │
├─────────────────────────────────────────────────────────────────┤
│ 3. 资源隔离                                                      │
│    - 子进程使用共享的用户特权结构                                  │
│    - 限制子进程的 IPC 能力和内核调用权限                            │
└─────────────────────────────────────────────────────────────────┘
```

**关键代码示例（do_fork.c）**

```c
// 系统进程 fork 时的特权降级处理
if (priv(rpp)->s_flags & SYS_PROC) {
    // 父进程是系统进程，子进程降级为用户进程
    rpc->p_priv = priv_addr(USER_PRIV_ID);   // 使用共享用户特权
    rpc->p_rts_flags |= RTS_NO_PRIV;          // 设置禁止运行标志
}
```

这种设计确保了系统进程可以创建子进程，但子进程默认没有特权，必须通过特权管理器（RS）的审核才能运行，从而实现了特权的安全控制和审计。

---

## 2. C 源码分析

本节详细分析 `minix3/minix/kernel/priv.h` 中定义的 `struct priv` 特权结构体。该结构体定义了系统进程的特权属性，包括标志位、系统调用掩码、IPC 权限、资源访问权限等。

### 2.1 结构体定义开头

`struct priv` 定义在 `priv.h` 第 21-66 行，结构体头部包含进程关联字段和标志字段：

```c
struct priv {
  proc_nr_t s_proc_nr;		/* number of associated process */
  sys_id_t s_id;		/* index of this system structure */
  short s_flags;		/* PREEMTIBLE, BILLABLE, etc. */
  int s_init_flags;             /* initialization flags given to the process. */
  // ... 后续字段
};
```

**设计意图**：

1. **分离关注**：将进程属性分为通用字段（`struct proc`）和特权字段（`struct priv`），实现清晰的职责分离
2. **空间效率**：用户进程共享同一个特权结构体，避免重复存储相同的权限信息
3. **安全隔离**：系统进程的特权字段独立，防止特权泄露

### 2.2 进程关联字段

进程关联字段用于建立特权结构体与进程之间的双向关联。

#### 2.2.1 s_proc_nr 字段

**字段定义**（`priv.h` 第 22 行）：

```c
proc_nr_t s_proc_nr;		/* number of associated process */
```

**作用说明**：

`s_proc_nr` 字段存储关联进程的进程号（process number）。这是特权结构体与进程之间的**反向关联指针**：

1. **进程→特权**：通过 `proc.p_priv` 指针从进程找到其特权结构体
2. **特权→进程**：通过 `priv.s_proc_nr` 进程号从特权结构体找到关联进程

**使用场景**：

```c
// 通过特权结构体查找关联的进程
struct priv *sp = &priv[i];
struct proc *rp = proc_addr(sp->s_proc_nr);  // 获取关联进程指针

// 检查特权结构体是否已关联进程
if (sp->s_proc_nr != NONE) {
    // 该特权结构体已分配给某个进程
}
```

**特殊值**：

- `NONE`（通常为 -1）：表示该特权结构体未关联任何进程（空闲状态）

#### 2.2.2 s_id 字段

**字段定义**（`priv.h` 第 23 行）：

```c
sys_id_t s_id;		/* index of this system structure */
```

**作用说明**：

`s_id` 字段存储特权结构体在特权表中的**索引**（ID）。该 ID 是特权结构体的唯一标识符，用于在特权表中快速定位。

**ID 分配策略**：

```
┌─────────────────────────────────────────────────────────┐
│                     特权表 ID 分配                       │
├─────────────────────────────────────────────────────────┤
│ ID 0-15    │ 静态特权（预定义系统进程）                   │
│            │ VM_PROC_NR, PM_PROC_NR, VFS_PROC_NR, etc.  │
├─────────────────────────────────────────────────────────┤
│ ID 16-31   │ 动态特权（运行时分配）                       │
│            │ 用户进程、动态创建的系统进程                  │
└─────────────────────────────────────────────────────────┘
```

**宏定义**（`priv.h` 第 72-77 行）：

```c
#define BEG_PRIV_ADDR              (&priv[0])
#define END_PRIV_ADDR              (&priv[NR_SYS_PROCS])
#define BEG_STATIC_PRIV_ADDR       BEG_PRIV_ADDR
#define END_STATIC_PRIV_ADDR       (BEG_STATIC_PRIV_ADDR + NR_STATIC_PRIV_IDS)
#define BEG_DYN_PRIV_ADDR          END_STATIC_PRIV_ADDR
#define END_DYN_PRIV_ADDR          END_PRIV_ADDR
```

**使用场景**：

```c
// 通过 ID 获取特权结构体指针
struct priv *sp = priv_addr(s_id);

// 通过进程号获取特权 ID
sys_id_t id = nr_to_id(proc_nr);

// 检查 ID 是否在静态特权范围内
if (id >= 0 && id < NR_STATIC_PRIV_IDS) {
    // 这是预定义的系统进程特权
}
```

### 2.3 标志字段

标志字段定义了特权结构体的基本属性和进程类型。

#### 2.3.1 s_flags 字段

**字段定义**（`priv.h` 第 24 行）：

```c
short s_flags;		/* PREEMTIBLE, BILLABLE, etc. */
```

**作用说明**：

`s_flags` 字段存储特权结构体的**标志位**，用于标识进程的类型和属性。这些标志位决定了进程的行为和权限。

**主要标志位定义**（`minix/priv.h`）：

| 标志 | 值 | 含义 |
|------|-----|------|
| `SYS_PROC` | 0x001 | 系统进程标志 |
| `BILLABLE` | 0x002 | 可计费进程 |
| `PREEMPTIBLE` | 0x004 | 可抢占进程 |
| `KERNEL` | 0x008 | 内核任务 |
| `VM_F` | 0x010 | VM 相关标志 |
| `SENDREC_F` | 0x020 | 允许 sendrec |
| `DRIVER_F` | 0x040 | 驱动程序 |
| `SERVER_F` | 0x080 | 服务器进程 |

##### 2.3.1.1 SYS_PROC 标志

**标志定义**：

```c
#define SYS_PROC    0x001    /* system process flag */
```

**作用说明**：

`SYS_PROC` 标志是 `s_flags` 字段中最重要的标志位，用于**标识进程是否为系统进程**：

- **设置**（`s_flags & SYS_PROC != 0`）：该进程是系统进程，拥有独立的特权结构体和特殊权限
- **未设置**（`s_flags & SYS_PROC == 0`）：该进程是普通用户进程，共享用户特权结构体

**使用场景**：

```c
// 检查进程是否为系统进程
if (priv(rp)->s_flags & SYS_PROC) {
    // 这是系统进程，拥有特殊权限
    // 可以执行特权操作
} else {
    // 这是普通用户进程
    // 只能执行受限操作
}

// 获取特权结构体的方式
struct priv *p = priv(rp);  // 系统进程：独立的特权结构体
                           // 用户进程：共享的用户特权结构体
```

##### 2.3.1.2 fork 时的检查

**fork 中的 SYS_PROC 检查**：

在 `do_fork()` 系统调用中，`SYS_PROC` 标志用于确定如何处理子进程的特权：

```c
// do_fork.c 第 104-108 行
if (priv(rpp)->s_flags & SYS_PROC) {
    // 父进程是系统进程
    rpc->p_priv = priv_addr(USER_PRIV_ID);  // 降级为用户特权
    rpc->p_rts_flags |= RTS_NO_PRIV;        // 设置禁止运行标志
}
```

**检查逻辑**：

1. **检查父进程的 `SYS_PROC` 标志**：
   - 使用 `priv(rpp)` 宏获取父进程的特权结构体指针
   - 检查 `s_flags & SYS_PROC` 是否非零

2. **如果是系统进程**：
   - **特权降级**：将子进程的 `p_priv` 指针指向共享的用户特权结构体（`USER_PRIV_ID`）
   - **设置禁止标志**：设置 `RTS_NO_PRIV` 标志，阻止子进程运行

3. **如果不是系统进程**（普通用户进程）：
   - 子进程继承父进程的 `p_priv` 指针（已经是用户特权结构体）
   - 不设置 `RTS_NO_PRIV` 标志（子进程可以正常运行）

**安全意义**：

这种检查机制确保了：
1. **特权不扩散**：系统进程创建的子进程不会自动成为系统进程
2. **强制降级**：系统进程的子进程必须降级为普通用户进程
3. **审核控制**：通过 `RTS_NO_PRIV` 强制子进程必须经过 RS 服务的授权才能运行

#### 2.3.2 s_init_flags 字段

**字段定义**（`priv.h` 第 25 行）：

```c
int s_init_flags;             /* initialization flags given to the process. */
```

**作用说明**：

`s_init_flags` 字段存储进程的**初始化标志**，这些标志在进程启动时由父进程或系统设置，用于控制进程的初始行为和属性。与 `s_flags` 不同，`s_init_flags` 主要用于初始化阶段的配置，而不是运行时状态。

**主要初始化标志**（`minix/priv.h`）：

| 标志 | 值 | 含义 |
|------|-----|------|
| `INTERCEPT_F` | 0x001 | 允许系统调用拦截 |
| `ORPHAN_F` | 0x002 | 孤儿进程标志 |
| `SIGINT_F` | 0x004 | 允许 SIGINT |
| `SIGQUIT_F` | 0x008 | 允许 SIGQUIT |
| `SIGILL_F` | 0x010 | 允许 SIGILL |
| `SIGTRAP_F` | 0x020 | 允许 SIGTRAP |
| `SIGABRT_F` | 0x040 | 允许 SIGABRT |
| `SIGBUS_F` | 0x080 | 允许 SIGBUS |
| `SIGFPE_F` | 0x100 | 允许 SIGFPE |
| `SIGUSR1_F` | 0x200 | 允许 SIGUSR1 |
| `SIGSEGV_F` | 0x400 | 允许 SIGSEGV |
| `SIGUSR2_F` | 0x800 | 允许 SIGUSR2 |

**使用场景**：

```c
// 在进程启动时设置初始化标志
struct proc *rp = ...;
rp->p_priv->s_init_flags = SIGINT_F | SIGQUIT_F | SIGILL_F;

// 检查进程是否允许接收特定信号
if (rp->p_priv->s_init_flags & SIGINT_F) {
    // 进程允许接收 SIGINT 信号
    send_signal(rp, SIGINT);
}

// 在 fork 时继承初始化标志（可选）
rpc->p_priv->s_init_flags = rpp->p_priv->s_init_flags;
```

**与 fork 的关系**：

在 `do_fork()` 操作中，初始化标志的处理取决于具体需求：

1. **默认行为**：子进程不继承父进程的初始化标志，或者继承后由系统重置
2. **信号继承**：某些初始化标志（如允许的信号）可以被子进程继承
3. **重置标志**：子进程的某些初始化标志可能被显式清除，以确保干净的初始状态

**设计意义**：

`s_init_flags` 字段提供了一种灵活的配置机制，允许在进程启动时设置特定的行为和权限，而不需要在运行时动态检查。这种静态配置的方式提高了系统的安全性和可预测性。

### 2.4 异步发送字段

异步发送字段用于支持 Minix3 的**异步消息传递机制**，允许进程在不阻塞的情况下发送消息。这是提高系统并发性能的重要特性。

#### 2.4.1 s_asyntab 字段

**字段定义**（`priv.h` 第 28 行）：

```c
vir_bytes s_asyntab;		/* addr. of table in process' address space */
```

**作用说明**：

`s_asyntab` 字段存储异步发送表在用户进程**虚拟地址空间中的地址**。异步发送表是一个由用户进程管理的环形缓冲区，用于存储待发送的异步消息。

**使用场景**：

```c
// 用户进程设置异步发送表
struct asyn_message asyn_table[ASYN_TABLE_SIZE];
struct priv *sp = priv(rp);
sp->s_asyntab = (vir_bytes)asyn_table;
sp->s_asynsize = ASYN_TABLE_SIZE;
```

**初始值**：

- `0` 或 `NULL`：表示该进程不使用异步发送机制

#### 2.4.2 s_asynsize 字段

**字段定义**（`priv.h` 第 29-31 行）：

```c
size_t s_asynsize;		/* number of elements in table. 0 when not in use */
```

**作用说明**：

`s_asynsize` 字段存储异步发送表的**元素数量**。值为 `0` 时表示该进程不使用异步发送机制。

**与 s_asyntab 的关系**：

| s_asyntab | s_asynsize | 状态 |
|-----------|------------|------|
| 0 | 0 | 异步发送未启用 |
| 有效地址 | >0 | 异步发送已启用 |

#### 2.4.3 s_asynendpoint 字段

**字段定义**（`priv.h` 第 32 行）：

```c
endpoint_t s_asynendpoint;    /* the endpoint the asyn table belongs to. */
```

**作用说明**：

`s_asynendpoint` 字段存储**拥有该异步发送表的进程的端点**。这个字段用于验证和跟踪，确保异步发送表与正确的进程关联。

**安全意义**：

该字段防止了恶意进程试图访问或修改其他进程的异步发送表，是权限验证的重要组成部分。

### 2.5 系统调用控制字段

系统调用控制字段定义了进程可以执行哪些系统调用、可以与哪些进程进行 IPC 通信。这是特权管理的核心机制。

#### 2.5.1 s_trap_mask 字段

**字段定义**（`priv.h` 第 34 行）：

```c
short s_trap_mask;		/* allowed system call traps */
```

**作用说明**：

`s_trap_mask` 字段是一个**位掩码**，定义了进程被允许使用的系统调用陷阱号（system call trap numbers）。当进程执行 `int` 指令（或系统调用指令）进入内核时，内核使用此掩码检查该进程是否有权限执行该调用。

**掩码位定义**（部分）：

| 位 | 系统调用 | 说明 |
|----|---------|------|
| 0x01 | `send` | 发送消息 |
| 0x02 | `receive` | 接收消息 |
| 0x04 | `sendrec` | 发送并接收 |
| 0x08 | `notify` | 发送通知 |
| 0x10 | `senda` | 异步发送 |
| 0x20 | `receivea` | 异步接收 |

**使用场景**：

```c
// 检查进程是否允许执行 send 系统调用
if (priv(rp)->s_trap_mask & (1 << TRAP_SEND)) {
    // 允许执行 send()
    result = do_send(rp, msg);
} else {
    // 权限不足
    return EPERM;
}

// 设置系统调用掩码（初始化时）
priv(rp)->s_trap_mask = TRAP_SEND | TRAP_RECEIVE | TRAP_NOTIFY;
```

#### 2.5.2 s_ipc_to 字段

**字段定义**（`priv.h` 第 35 行）：

```c
sys_map_t s_ipc_to;		/* allowed destination processes */
```

**作用说明**：

`s_ipc_to` 字段是一个**系统映射**（`sys_map_t` 通常是位图），定义了进程被允许与之进行 IPC 通信的**目标进程集合**。即使进程有 `send` 权限，也只能向 `s_ipc_to` 中指定的进程发送消息。

**sys_map_t 设计**：

```c
// sys_map_t 通常是 32 位或 64 位整数，每一位代表一个特权 ID
typedef unsigned long sys_map_t;

// 第 i 位为 1 表示可以与特权 ID 为 i 的进程通信
```

**权限检查**：

```c
// may_send_to 宏定义（priv.h 第 86 行）
#define may_send_to(rp, nr) (get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr)))

// 检查进程 rp 是否可以向进程 nr 发送消息
if (!may_send_to(rp, target_nr)) {
    return EPERM;  // 没有 IPC 权限
}
```

**使用场景**：

```c
// 初始化时设置 IPC 目标映射
// 允许向 VM、PM、VFS 发送消息
priv(rp)->s_ipc_to = (1 << VM_PRIV_ID) | (1 << PM_PRIV_ID) | (1 << VFS_PRIV_ID);

// 尝试发送消息前检查权限
if (!may_send_to(rp, target_nr)) {
    return EDEADSRCDST;  // 目标不允许
}
```

#### 2.5.3 s_k_call_mask 字段

**字段定义**（`priv.h` 第 38 行）：

```c
bitchunk_t s_k_call_mask[SYS_CALL_MASK_SIZE];
```

**作用说明**：

`s_k_call_mask` 字段是一个**位数组**，定义了进程被允许执行的**内核调用**（kernel calls）。内核调用是系统进程通过 `sys_*()` 函数向内核发出的请求（如 `sys_fork`, `sys_exec`, `sys_privctl` 等）。

**bitchunk_t 设计**：

```c
// bitchunk_t 通常是一个整数类型（如 unsigned long）
// 每个 bitchunk_t 包含多个位，每个位代表一个内核调用
typedef unsigned long bitchunk_t;
#define BITCHUNK_BITS (sizeof(bitchunk_t) * 8)

// SYS_CALL_MASK_SIZE 计算所需数组大小
#define SYS_CALL_MASK_SIZE (NR_SYS_CALLS / BITCHUNK_BITS + 1)
```

**内核调用编号示例**（部分）：

| 编号 | 内核调用 | 说明 |
|------|---------|------|
| 0 | `SYS_FORK` | 创建子进程 |
| 1 | `SYS_EXEC` | 执行新程序 |
| 2 | `SYS_EXIT` | 进程退出 |
| 3 | `SYS_PRIVCTL` | 特权控制 |
| 4 | `SYS_TRACE` | 调试跟踪 |
| 5 | `SYS_KILL` | 发送信号 |
| ... | ... | ... |

**掩码检查**：

```c
// 检查是否允许执行特定内核调用
#define get_kcall_mask(sp, call_nr) \
    ((sp)->s_k_call_mask[(call_nr) / BITCHUNK_BITS] & \
     (1 << ((call_nr) % BITCHUNK_BITS)))

// 在内核调用处理中检查权限
if (!get_kcall_mask(priv(rp), call_nr)) {
    return ECALLDENIED;  // 内核调用被拒绝
}
```

**使用场景**：

```c
// PM 进程的内核调用掩码（允许 fork/exec/exit/privctl）
priv(rp)->s_k_call_mask[0] = (1 << SYS_FORK) | (1 << SYS_EXEC) | 
                              (1 << SYS_EXIT) | (1 << SYS_PRIVCTL);

// 普通用户进程无内核调用权限
memset(priv(rp)->s_k_call_mask, 0, sizeof(s_k_call_mask));
```

**与 fork 的关系**：

在 `do_fork()` 执行期间，内核调用权限通过 `s_k_call_mask` 进行间接控制：

**父进程权限检查**：

父进程必须拥有 `SYS_FORK` 权限才能成功执行 `sys_fork()` 系统调用：

```c
// system.c 中的系统调用分发
if (!get_kcall_mask(priv(rp), call_nr)) {
    return ECALLDENIED;
}
// 调用处理函数
do_fork(caller, m_ptr);
```

**子进程权限**：

```c
// do_fork() 中的处理
if (priv(rpp)->s_flags & SYS_PROC) {
    // 系统进程 fork，子进程降级
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    // 子进程现在使用共享用户特权，s_k_call_mask 全为 0
}
```

**权限降级效果**：

| 进程类型 | s_k_call_mask | 效果 |
|----------|--------------|------|
| 系统进程 | 特定权限 | 可以执行内核调用 |
| 用户进程 | 全 0 | 无法执行内核调用 |

### 2.6 信号管理字段

信号管理字段定义了进程的信号处理器和信号管理策略，用于实现可靠信号机制。

#### 2.6.1 s_sig_mgr 字段

**字段定义**（`priv.h` 第 40 行）：

```c
endpoint_t s_sig_mgr;		/* signal manager for system signals */
```

**作用说明**：

`s_sig_mgr` 字段存储进程的**信号管理器**（signal manager）的端点。当进程收到系统信号时，信号会被转发给指定的信号管理器处理，而不是由内核直接处理。

**信号管理流程**：

```
信号到达
    │
    v
┌─────────────────┐
│ 查找信号管理器  │
│ s_sig_mgr       │
└─────────────────┘
    │
    v
┌─────────────────┐
│ 转发信号给      │
│ 信号管理器      │
└─────────────────┘
    │
    v
┌─────────────────┐
│ 信号管理器处理  │
│ 并返回结果      │
└─────────────────┘
```

**使用场景**：

```c
// 设置 PM 为信号管理器
priv(rp)->s_sig_mgr = PM_ENDPOINT;

// 信号到达时转发给信号管理器
if (priv(rp)->s_sig_mgr != NONE) {
    send_signal_to_manager(rp, sig_nr, priv(rp)->s_sig_mgr);
} else {
    // 没有信号管理器，使用默认处理
    default_signal_handler(rp, sig_nr);
}
```

#### 2.6.2 s_bak_sig_mgr 字段

**字段定义**（`priv.h` 第 41 行）：

```c
endpoint_t s_bak_sig_mgr;	/* backup signal manager for system signals */
```

**作用说明**：

`s_bak_sig_mgr` 字段存储进程的**备用信号管理器**（backup signal manager）的端点。当主信号管理器（`s_sig_mgr`）不可用时（例如崩溃或被阻塞），信号会被转发给备用信号管理器处理。

**双保险机制**：

```
信号到达
    │
    v
┌─────────────────┐
│ 主信号管理器    │
│ s_sig_mgr       │
│ 可用？          │
└─────────────────┘
    │
   是├───────────┐
    │           │否
    v           v
┌───────┐  ┌─────────────────┐
│转发给 │  │ 备用信号管理器  │
│主管理器│  │ s_bak_sig_mgr   │
│       │  │ 可用？            │
└───────┘  └─────────────────┘
                │
               是├───────────┐
                │           │否
                v           v
           ┌───────┐   ┌───────────┐
           │转发给 │   │ 默认处理  │
           │备管理器│   │ (可能丢失)│
           └───────┘   └───────────┘
```

**使用场景**：

```c
// 设置主信号管理器为 PM，备用为 RS
priv(rp)->s_sig_mgr = PM_ENDPOINT;
priv(rp)->s_bak_sig_mgr = RS_ENDPOINT;

// 发送信号时检查可用性
if (is_process_available(priv(rp)->s_sig_mgr)) {
    send_signal_to(rp, sig_nr, priv(rp)->s_sig_mgr);
} else if (is_process_available(priv(rp)->s_bak_sig_mgr)) {
    send_signal_to(rp, sig_nr, priv(rp)->s_bak_sig_mgr);
} else {
    // 无可用信号管理器，使用默认处理或记录错误
    handle_signal_no_manager(rp, sig_nr);
}
```

**高可用性设计**：

备用信号管理器机制确保了即使在主信号管理器故障的情况下，信号仍然能够被处理，提高了系统的可靠性和可用性。这对于关键系统进程（如 PM、RS）尤其重要。

### 2.7 待处理事件字段

待处理事件字段用于记录该进程需要处理的各种异步事件，包括通知、异步消息、中断和信号。

#### 2.7.1 s_notify_pending 字段

**字段定义**（`priv.h` 第 42 行）：

```c
sys_map_t s_notify_pending;  	/* bit map with pending notifications */
```

**作用说明**：

`s_notify_pending` 字段是一个**位图**，记录有哪些发送者向该进程发送了**通知**（notification）但尚未被接收。每个位对应一个特权 ID，如果第 `i` 位为 1，表示特权 ID 为 `i` 的进程有待发送的通知。

**通知机制**：

```c
// 发送通知给目标进程
void send_notification(endpoint_t src, endpoint_t dst) {
    struct priv *sp = priv(proc_addr(dst));
    int src_id = nr_to_id(src);
    
    // 设置对应位，表示有待处理通知
    set_sys_bit(&sp->s_notify_pending, src_id);
    
    // 唤醒目标进程（如果正在等待接收）
    if (RTS_ISSET(proc_addr(dst), RTS_RECEIVING)) {
        // 唤醒处理
    }
}
```

**接收处理**：

```c
// 进程接收通知时清除对应位
void receive_notification(struct proc *rp, endpoint_t *src_out) {
    // 查找待处理的通知
    int src_id = get_next_bit(rp->p_priv->s_notify_pending);
    if (src_id >= 0) {
        clear_sys_bit(&rp->p_priv->s_notify_pending, src_id);
        *src_out = id_to_nr(src_id);
    }
}
```

#### 2.7.2 s_asyn_pending 字段

**字段定义**（`priv.h` 第 43 行）：

```c
sys_map_t s_asyn_pending;	/* bit map with pending asyn messages */
```

**作用说明**：

`s_asyn_pending` 字段是一个**位图**，记录有哪些发送者向该进程发送了**异步消息**但尚未被处理。与 `s_notify_pending` 类似，但用于异步消息而非通知。

**使用场景**：

```c
// 异步消息到达
void deliver_async_message(endpoint_t src, endpoint_t dst) {
    struct priv *sp = priv(proc_addr(dst));
    int src_id = nr_to_id(src);
    
    // 设置异步消息待处理位
    set_sys_bit(&sp->s_asyn_pending, src_id);
}

// 检查并处理异步消息
void check_async_messages(struct proc *rp) {
    if (rp->p_priv->s_asyn_pending != 0) {
        // 有待处理的异步消息
        process_async_messages(rp);
    }
}
```

#### 2.7.3 s_int_pending 字段

**字段定义**（`priv.h` 第 44 行）：

```c
irq_id_t s_int_pending;	/* pending hardware interrupts */
```

**作用说明**：

`s_int_pending` 字段记录有哪些**硬件中断**正在等待该进程处理。`irq_id_t` 通常是一个位掩码类型，每个位对应一个 IRQ 号。

**中断处理流程**：

```
硬件中断触发
    │
    v
┌─────────────────┐
│ 中断处理程序    │
└─────────────────┘
    │
    v
┌─────────────────┐
│ 查找中断处理进程│
│ 根据 irq_table  │
└─────────────────┘
    │
    v
┌─────────────────┐
│ s_int_pending   │
│ 设置对应 IRQ 位 │
└─────────────────┘
    │
    v
┌─────────────────┐
│ 唤醒/通知进程   │
└─────────────────┘
```

**使用场景**：

```c
// 中断到达时
void handle_interrupt(int irq) {
    struct proc *rp = irq_table[irq];  // 查找处理进程
    if (rp != NULL) {
        // 设置待处理中断位
        rp->p_priv->s_int_pending |= (1 << irq);
        
        // 通知进程（如果进程在等待）
        notify_process(rp, INTERRUPT_NOTIFICATION);
    }
}

// 进程处理中断
void process_interrupts(struct proc *rp) {
    irq_id_t pending = rp->p_priv->s_int_pending;
    
    while (pending != 0) {
        int irq = find_first_bit(pending);
        // 处理该 IRQ
        handle_specific_irq(rp, irq);
        // 清除已处理的位
        rp->p_priv->s_int_pending &= ~(1 << irq);
        pending &= ~(1 << irq);
    }
}
```

#### 2.7.4 s_sig_pending 字段

**字段定义**（`priv.h` 第 45 行）：

```c
sigset_t s_sig_pending;	/* pending signals */
```

**作用说明**：

`s_sig_pending` 字段是一个**信号集**（signal set），记录哪些信号正在等待该进程处理。`sigset_t` 是一个位图，每个位对应一个信号号（如 `SIGINT`, `SIGTERM` 等）。

**信号处理**：

```c
// 发送信号给进程
void deliver_signal(struct proc *rp, int sig_nr) {
    // 添加到待处理信号集
    sigaddset(&rp->p_priv->s_sig_pending, sig_nr);
    
    // 设置信号标志
    RTS_SET(rp, RTS_SIGNALED);
}

// 进程处理信号
void process_signals(struct proc *rp) {
    sigset_t pending = rp->p_priv->s_sig_pending;
    
    for (int sig = 1; sig < NSIG; sig++) {
        if (sigismember(&pending, sig)) {
            // 处理该信号
            handle_signal(rp, sig);
            
            // 清除已处理的信号
            sigdelset(&rp->p_priv->s_sig_pending, sig);
        }
    }
    
    // 如果没有更多待处理信号，清除 RTS_SIGNALED
    if (rp->p_priv->s_sig_pending == 0) {
        RTS_UNSET(rp, RTS_SIGNALED);
    }
}
```

**与 fork 的关系**：

在 `do_fork()` 中，子进程的待处理事件字段会被清空，确保子进程从一个干净的状态开始：

```c
// do_fork.c 第 122 行
*rpc = *rpp;

// 清空子进程的待处理信号
sigemptyset(&rpc->p_priv->s_sig_pending);

// 清空其他待处理事件（在内核初始化时处理）
rpc->p_priv->s_notify_pending = 0;
rpc->p_priv->s_asyn_pending = 0;
rpc->p_priv->s_int_pending = 0;
```

### 2.8 其他字段

其他字段包含了一些辅助功能字段，如 IPC 过滤器、定时器、栈保护和诊断标志等。

#### 2.8.1 s_ipcf 字段

**字段定义**（`priv.h` 第 46 行）：

```c
ipc_filter_t *s_ipcf;         /* ipc filter (NULL when no filter is set) */
```

**作用说明**：

`s_ipcf` 字段是一个指向**IPC 过滤器**（IPC Filter）的指针。IPC 过滤器用于在消息传递过程中对消息进行过滤和验证，可以实现细粒度的访问控制策略。

**IPC 过滤器用途**：

1. **消息过滤**：根据消息内容、类型、大小等条件决定是否允许消息通过
2. **审计日志**：记录所有 IPC 通信用于安全审计
3. **访问控制**：实现基于消息的访问控制策略
4. **隔离策略**：实现沙箱隔离，限制进程通信范围

**使用场景**：

```c
// 设置 IPC 过滤器
ipc_filter_t *filter = create_ipc_filter();
filter->allow_mask = IPCF_TYPE_MASK;
filter->allowed_types = MSG_TYPE_DATA | MSG_TYPE_CONTROL;
priv(rp)->s_ipcf = filter;

// 检查消息时应用过滤器
int check_ipc_permission(struct proc *rp, message *msg) {
    if (priv(rp)->s_ipcf != NULL) {
        return apply_ipc_filter(priv(rp)->s_ipcf, msg);
    }
    return OK;  // 无过滤器，允许通过
}
```

**初始值**：

- `NULL`：表示该进程没有设置 IPC 过滤器

#### 2.8.2 s_alarm_timer 字段

**字段定义**（`priv.h` 第 48 行）：

```c
minix_timer_t s_alarm_timer;	/* synchronous alarm timer */
```

**作用说明**：

`s_alarm_timer` 字段存储进程的**同步闹钟定时器**（synchronous alarm timer）。该定时器用于实现 `alarm()` 系统调用，允许进程在指定时间后收到 `SIGALRM` 信号。

**定时器机制**：

```
进程设置闹钟
    │
    v
┌─────────────────┐
│ s_alarm_timer   │
│ 设置到期时间    │
└─────────────────┘
    │
    v
时钟中断处理
    │
    v
┌─────────────────┐
│ 检查到期时间    │
│ 是否到达？      │
└─────────────────┘
    │
   是
    v
┌─────────────────┐
│ 发送 SIGALRM    │
│ 给进程          │
└─────────────────┘
```

**使用场景**：

```c
// 设置闹钟（秒数）
void sys_alarm(struct proc *rp, unsigned int seconds) {
    if (seconds == 0) {
        // 取消闹钟
        cancel_timer(&priv(rp)->s_alarm_timer);
    } else {
        // 设置闹钟到期时间
        priv(rp)->s_alarm_timer.expire_time = get_uptime() + seconds * HZ;
        priv(rp)->s_alarm_timer.proc_nr = rp->p_nr;
        insert_timer(&priv(rp)->s_alarm_timer);
    }
}

// 定时器到期处理
void handle_alarm_timer(struct proc *rp) {
    deliver_signal(rp, SIGALRM);
}
```

#### 2.8.3 s_stack_guard 字段

**字段定义**（`priv.h` 第 49 行）：

```c
reg_t *s_stack_guard;		/* stack guard word for kernel tasks */
```

**作用说明**：

`s_stack_guard` 字段存储**栈保护字**（stack guard word）的地址。该字段主要用于内核任务（kernel tasks）的栈溢出检测，通过定期检查栈底附近的特殊值（哨兵值）是否被覆盖来判断是否发生栈溢出。

**栈溢出检测机制**：

```
栈生长方向 (从高地址向低地址)
    │
    ▼
┌─────────────────┐ 高地址
│   有效栈数据    │
│   ...           │
│   局部变量      │
│   返回地址      │
│   ...           │
├─────────────────┤
│   STACK_GUARD   │  <-- s_stack_guard 指向这里
│   (0xDEADBEEF)  │      哨兵值
└─────────────────┘ 低地址

栈溢出时:
    如果栈数据覆盖到 STACK_GUARD 位置
    哨兵值被修改为其他值
    检测到溢出，触发错误或杀死进程
```

**使用场景**：

```c
// 初始化栈保护字
void init_stack_guard(struct proc *rp, reg_t *stack_bottom) {
    // s_stack_guard 指向栈底
    priv(rp)->s_stack_guard = stack_bottom;
    // 写入哨兵值
    *priv(rp)->s_stack_guard = STACK_GUARD;
}

// 定期检查栈溢出
void check_stack_overflow(struct proc *rp) {
    if (priv(rp)->s_stack_guard != NULL) {
        if (*priv(rp)->s_stack_guard != STACK_GUARD) {
            // 栈溢出检测到！
            panic("Stack overflow detected in process %d", rp->p_nr);
            // 或杀死进程
            kill_process(rp, SIGSEGV);
        }
    }
}

// 在时钟中断中定期检查
void clock_handler() {
    for (each kernel task) {
        check_stack_overflow(rp);
    }
}
```

**重要说明**：

栈保护机制主要用于**内核任务**（`NR_TASKS` 范围内的进程），因为它们运行在内核态且栈空间有限。用户进程的栈由 VM 管理，使用不同的保护机制（如 guard page）。

#### 2.8.4 s_diag_sig 字段

**字段定义**（`priv.h` 第 51 行）：

```c
char s_diag_sig;		/* send a SIGKMESS when diagnostics arrive? */
```

**作用说明**：

`s_diag_sig` 字段是一个标志位，用于控制当**诊断消息**（diagnostics）到达时是否向进程发送 `SIGKMESS` 信号。这允许进程选择性地接收内核诊断通知。

**诊断消息来源**：

1. **内核日志**（kernel log）：内核产生的日志消息
2. **系统事件**：重要的系统状态变化
3. **错误报告**：内核检测到的错误条件
4. **调试信息**：调试模式下产生的信息

**使用场景**：

```c
// 进程注册接收诊断消息
void enable_diag_notifications(struct proc *rp) {
    priv(rp)->s_diag_sig = 1;  // 启用 SIGKMESS
}

// 禁用诊断通知
void disable_diag_notifications(struct proc *rp) {
    priv(rp)->s_diag_sig = 0;  // 禁用 SIGKMESS
}

// 内核产生诊断消息时
void kernel_diagnostic_message(const char *msg) {
    // 记录到内核日志
    log_kernel_message(msg);
    
    // 通知所有注册了诊断通知的进程
    for (each process rp) {
        if (priv(rp)->s_diag_sig) {
            deliver_signal(rp, SIGKMESS);
        }
    }
}

// 进程的信号处理程序
void sigkmess_handler(int sig) {
    // 读取最新的诊断消息
    char diag_msg[256];
    read_latest_diag_message(diag_msg, sizeof(diag_msg));
    
    // 处理诊断信息
    process_diagnostic_message(diag_msg);
}
```

**典型应用**：

1. **日志监控进程**：专门的进程监控系统日志并产生报告
2. **系统监控工具**：如 `top`、`vmstat` 等需要实时了解内核状态的工具
3. **调试工具**：调试器需要捕获内核产生的调试信息
4. **系统管理守护进程**：如 `syslogd` 等需要收集内核消息的守护进程

**设计优点**：

- **选择性通知**：只有注册了 `s_diag_sig` 的进程才会收到通知，避免了不必要的信号处理开销
- **异步通知**：使用信号机制实现异步通知，进程可以在方便的时候处理诊断信息
- **可扩展性**：可以轻松添加更多类型的诊断通知机制

### 2.9 资源访问字段

资源访问字段定义了进程对系统资源（I/O 端口、物理内存、IRQ 线、Grant 表、State 表等）的访问权限。这些字段是实现设备驱动程序隔离和系统安全的关键机制。

本章将详细分析以下资源访问相关字段：

- **I/O 端口范围** (`s_nr_io_range`, `s_io_tab`)：定义进程可访问的 I/O 端口地址空间
- **内存范围** (`s_nr_mem_range`, `s_mem_tab`)：定义进程可访问的物理内存地址空间
- **IRQ 线** (`s_nr_irq`, `s_irq_tab`)：定义进程可处理的中断请求线
- **Grant 表** (`s_grant_table`, `s_grant_entries`, `s_grant_endpoint`)：支持跨进程安全内存共享
- **State 表** (`s_state_table`, `s_state_entries`)：支持 RS 管理进程状态和重启恢复

#### 2.9.1 I/O 端口范围

I/O 端口范围字段定义了进程被允许访问的 I/O 端口地址空间范围。这是实现设备驱动程序隔离的关键机制，防止驱动程序访问不属于它们的硬件资源。

**字段定义**（`priv.h` 第 53-54 行）：

```c
int s_nr_io_range;		/* allowed I/O ports */
struct io_range s_io_tab[NR_IO_RANGE];
```

**结构体定义**（`minix/type.h`）：

```c
struct io_range {
    unsigned ior_base;	/* Lowest I/O port in range */
    unsigned ior_limit;	/* Highest I/O port in range */
};
```

**字段说明**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `s_nr_io_range` | `int` | 当前进程拥有的 I/O 端口范围数量（0 表示无 I/O 权限） |
| `s_io_tab` | `struct io_range[]` | I/O 端口范围表，每个条目定义一个连续的端口范围 |

**常量定义**：

```c
#define NR_IO_RANGE  64  /* 每个进程最多拥有的 I/O 端口范围数 */
```

**典型使用场景**：

```c
// 为串口驱动分配 I/O 端口范围 (COM1: 0x3F8-0x3FF)
struct priv *sp = priv(rp);
sp->s_io_tab[0].ior_base = 0x3F8;
sp->s_io_tab[0].ior_limit = 0x3FF;
sp->s_nr_io_range = 1;

// 为 PCI 配置空间分配 I/O 范围
sp->s_io_tab[0].ior_base = 0xCF8;   // PCI 配置地址端口
sp->s_io_tab[0].ior_limit = 0xCFB;
sp->s_io_tab[1].ior_base = 0xCFC;   // PCI 配置数据端口
sp->s_io_tab[1].ior_limit = 0xCFF;
sp->s_nr_io_range = 2;
```

**与 fork 的关系**：

在 `do_fork()` 中，子进程继承父进程的 I/O 端口范围配置。但对于系统进程 fork，子进程会被降级为用户进程，其 I/O 权限需要由 RS 重新配置：

```c
// 系统进程 fork 时，子进程降级为用户进程
if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;
    // 子进程的 s_nr_io_range 继承自父进程，但由于降级为用户进程
    // 实际上 I/O 权限被 RS 重新配置
}
```

**I/O 端口访问控制机制**：

```
┌─────────────────────────────────────────────────────────────┐
│                    I/O 端口访问控制流程                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  驱动进程请求访问 I/O 端口 0x3F8 (COM1)                       │
│                     │                                       │
│                     ▼                                       │
│     ┌───────────────────────────┐                           │
│     │  检查 s_nr_io_range > 0   │                           │
│     └───────────────────────────┘                           │
│                     │                                       │
│         否          │           是                          │
│          ┌──────────┴──────────┐                            │
│          ▼                     ▼                            │
│     ┌─────────┐    ┌─────────────────────┐                  │
│     │ EPERM   │    │ 遍历 s_io_tab 检查   │                  │
│     │ 拒绝访问 │    │ 0x3F8 是否在范围内   │                  │
│     └─────────┘    └─────────────────────┘                  │
│                               │                             │
│                   否          │           是                  │
│                    ┌──────────┴──────────┐                  │
│                    ▼                     ▼                  │
│               ┌─────────┐          ┌─────────┐              │
│               │ EPERM   │          │ 允许访问 │              │
│               │ 拒绝访问 │          │ I/O 端口 │              │
│               └─────────┘          └─────────┘              │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Rust 设计考虑**：

```rust
/// I/O 端口范围
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoRange {
    /// I/O 端口基地址
    pub base: u16,
    /// I/O 端口上限（包含）
    pub limit: u16,
}

impl IoRange {
    /// 创建新的 I/O 端口范围
    pub fn new(base: u16, limit: u16) -> Self {
        assert!(base <= limit, "base must be <= limit");
        Self { base, limit }
    }

    /// 检查端口是否在范围内
    pub fn contains(&self, port: u16) -> bool {
        self.base <= port && port <= self.limit
    }
}

/// I/O 端口访问控制 trait（OS 抽象）
pub trait IoPortAccess {
    /// 检查进程是否有权限访问指定 I/O 端口
    fn check_io_permission(&self, port: u16) -> bool;

    /// 添加 I/O 端口范围
    fn add_io_range(&mut self, range: IoRange) -> Result<(), PrivError>;

    /// 移除 I/O 端口范围
    fn remove_io_range(&mut self, range: IoRange) -> Result<(), PrivError>;
}

/// Mock 硬件实现
#[derive(Debug, Default)]
pub struct MockIoPort;

impl IoPortAccess for MockIoPort {
    fn check_io_permission(&self, _port: u16) -> bool {
        // Mock 实现：总是允许访问
        true
    }

    fn add_io_range(
        &mut self,
        _range: IoRange,
    ) -> Result<(), PrivError> {
        Ok(())
    }

    fn remove_io_range(
        &mut self,
        _range: IoRange,
    ) -> Result<(), PrivError> {
        Ok(())
    }
}
```

#### 2.9.2 内存范围

内存范围字段定义了进程被允许访问的物理内存地址范围。这对于需要直接访问物理内存的设备驱动程序（如帧缓冲驱动、DMA 控制器驱动）至关重要。

**字段定义**（`priv.h` 第 56-57 行）：

```c
int s_nr_mem_range;		/* allowed memory ranges */
struct minix_mem_range s_mem_tab[NR_MEM_RANGE];
```

**结构体定义**（`minix/type.h`）：

```c
struct minix_mem_range {
    phys_bytes mr_base;	  /* Lowest memory address in range */
    phys_bytes mr_limit;  /* Highest memory address in range */
};
```

**字段说明**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `s_nr_mem_range` | `int` | 当前进程拥有的内存范围数量（0 表示无物理内存访问权限） |
| `s_mem_tab` | `struct minix_mem_range[]` | 内存范围表，每个条目定义一个连续的物理内存范围 |

**常量定义**：

```c
#define NR_MEM_RANGE  20  /* 每个进程最多拥有的内存范围数 */
```

**典型使用场景**：

```c
// 为帧缓冲驱动分配内存范围 (0xA0000-0xBFFFF)
struct priv *sp = priv(rp);
sp->s_mem_tab[0].mr_base = 0xA0000;
sp->s_mem_tab[0].mr_limit = 0xBFFFF;
sp->s_nr_mem_range = 1;

// 为 DMA 控制器分配多个内存范围
sp->s_mem_tab[0].mr_base = 0x00000;   // 低内存区域
sp->s_mem_tab[0].mr_limit = 0x9FFFF;
sp->s_mem_tab[1].mr_base = 0x100000;  // 扩展内存区域
sp->s_mem_tab[1].mr_limit = 0xFFFFFFFF;
sp->s_nr_mem_range = 2;
```

**与 fork 的关系**：

在 `do_fork()` 中，子进程不继承父进程的物理内存访问权限：

```c
// 系统进程 fork 时，子进程降级为用户进程
if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;
    // 子进程 s_nr_mem_range 被重置为 0
    // 用户进程默认没有物理内存访问权限
}
```

**Rust 设计考虑**：

```rust
/// 物理内存范围
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemRange {
    /// 物理内存基地址
    pub base: PhysAddr,
    /// 物理内存上限（包含）
    pub limit: PhysAddr,
}

impl MemRange {
    /// 创建新的物理内存范围
    pub fn new(base: PhysAddr, limit: PhysAddr) -> Self {
        assert!(base <= limit, "base must be <= limit");
        Self { base, limit }
    }

    /// 检查物理地址是否在范围内
    pub fn contains(&self, addr: PhysAddr) -> bool {
        self.base <= addr && addr <= self.limit
    }
}

/// 物理内存访问控制 trait（OS 抽象）
pub trait PhysMemAccess {
    /// 检查进程是否有权限访问指定物理地址
    fn check_mem_permission(&self, addr: PhysAddr) -> bool;

    /// 添加物理内存范围
    fn add_mem_range(&mut self, range: MemRange) -> Result<(), PrivError>;

    /// 移除物理内存范围
    fn remove_mem_range(&mut self, range: MemRange) -> Result<(), PrivError>;
}

/// Mock 硬件实现
#[derive(Debug, Default)]
pub struct MockPhysMem;

impl PhysMemAccess for MockPhysMem {
    fn check_mem_permission(&self, _addr: PhysAddr) -> bool {
        // Mock 实现：总是允许访问
        true
    }

    fn add_mem_range(&mut self, _range: MemRange) -> Result<(), PrivError> {
        Ok(())
    }

    fn remove_mem_range(&mut self, _range: MemRange) -> Result<(), PrivError> {
        Ok(())
    }
}
```

#### 2.9.3 IRQ 线

IRQ（中断请求）线字段定义了进程被允许处理的中断线号。这是设备驱动程序与硬件交互的关键机制，允许驱动程序注册中断处理程序来响应硬件事件。

**字段定义**（`priv.h` 第 59-60 行）：

```c
int s_nr_irq;			/* allowed IRQ lines */
int s_irq_tab[NR_IRQ];
```

**字段说明**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `s_nr_irq` | `int` | 当前进程拥有的 IRQ 线数量（0 表示无中断处理权限） |
| `s_irq_tab` | `int[]` | IRQ 线表，每个条目存储一个 IRQ 线号 |

**常量定义**：

```c
#define NR_IRQ  16  /* 每个进程最多拥有的 IRQ 线数（x86 架构） */
```

**IRQ 号分配（x86 架构）**：

| IRQ 号 | 典型用途 | 说明 |
|--------|---------|------|
| 0 | 系统定时器（PIT） | 系统时钟中断 |
| 1 | 键盘控制器 | 键盘输入中断 |
| 2 | 级联（8259A 从片）| 用于连接第二个 8259A |
| 3 | COM2 | 串口 2 中断 |
| 4 | COM1 | 串口 1 中断 |
| 5 | LPT2 / 可用 | 并口 2 或通用 |
| 6 | 软盘控制器 | 软盘驱动器中断 |
| 7 | LPT1 | 并口 1 中断 |
| 8 | RTC | 实时时钟中断 |
| 9-15 | 可用 / PCI 设备 | 通用或 PCI 设备 |

**典型使用场景**：

```c
// 为串口驱动分配 IRQ（COM1: IRQ 4）
struct priv *sp = priv(rp);
sp->s_irq_tab[0] = 4;  // IRQ 4
sp->s_nr_irq = 1;

// 为网卡驱动分配多个 IRQ（主 IRQ 和 MSI-X）
sp->s_irq_tab[0] = 11;  // PCI IRQ A
sp->s_irq_tab[1] = 12;  // PCI IRQ B (MSI-X)
sp->s_nr_irq = 2;
```

**与 fork 的关系**：

在 `do_fork()` 中，子进程不继承父进程的 IRQ 权限：

```c
// 系统进程 fork 时，子进程降级为用户进程
if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;
    // 子进程 s_nr_irq 被重置为 0
    // 用户进程默认没有 IRQ 处理权限
}
```

**Rust 设计考虑**：

```rust
/// IRQ 号类型
pub type IrqNumber = u8;

/// IRQ 表
#[derive(Debug, Clone)]
pub struct IrqTable {
    /// IRQ 线数量
    pub count: usize,
    /// IRQ 线表
    pub irqs: [Option<IrqNumber>; NR_IRQ],
}

impl IrqTable {
    /// 创建空的 IRQ 表
    pub fn new() -> Self {
        Self {
            count: 0,
            irqs: [None; NR_IRQ],
        }
    }

    /// 添加 IRQ
    pub fn add_irq(&mut self, irq: IrqNumber) -> Result<(), PrivError> {
        if self.count >= NR_IRQ {
            return Err(PrivError::IrqTableFull);
        }
        if self.irqs.iter().any(|&i| i == Some(irq)) {
            return Err(PrivError::IrqAlreadyExists);
        }
        self.irqs[self.count] = Some(irq);
        self.count += 1;
        Ok(())
    }

    /// 检查是否允许使用指定 IRQ
    pub fn contains(&self, irq: IrqNumber) -> bool {
        self.irqs.iter().any(|&i| i == Some(irq))
    }
}
```

#### 2.9.4 grant 表

Grant 表（Grant Table）是 Minix3 内核中用于实现跨进程安全内存共享的关键机制。它允许一个进程（授权者）将自身地址空间的一部分"授权"给另一个进程（被授权者）访问，而无需复制数据。

**字段定义**（`priv.h` 第 61-63 行）：

```c
vir_bytes s_grant_table;	/* grant table address of process, or 0 */
int s_grant_entries;		/* no. of entries, or 0 */
endpoint_t s_grant_endpoint;  /* the endpoint the grant table belongs to */
```

**字段说明**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `s_grant_table` | `vir_bytes` | Grant 表在用户进程虚拟地址空间中的地址，0 表示未启用 |
| `s_grant_entries` | `int` | Grant 表中的条目数量 |
| `s_grant_endpoint` | `endpoint_t` | Grant 表所属进程的端点（用于验证） |

**Grant 表条目结构**：

```c
/* cpuf-flags 定义 */
#define GF_SRC_START  0x01   /* 源地址是起始地址 */
#define GF_DST_START  0x02   /* 目标地址是起始地址 */

/* 访问权限标志 */
#define CPF_READ      0x01   /* 允许读取 */
#define CPF_WRITE     0x02   /* 允许写入 */

/* Grant 表条目（简化版） */
struct grant {
    endpoint_t g_endpoint;   /* 被授权者端点 */
    int g_flags;             /* 访问权限标志（CPF_READ/CPF_WRITE） */
    vir_bytes g_addr;        /* 授权内存的起始虚拟地址 */
    size_t g_size;           /* 授权内存的大小（字节） */
};
```

**典型使用场景**：

```c
// 设备驱动准备 Grant 表供其他进程访问其缓冲区
struct priv *sp = priv(rp);

// 1. 分配 Grant 表（通常由 VM 或驱动自己管理）
struct grant *grant_table = alloc_grant_table(GRANT_TABLE_SIZE);
sp->s_grant_table = (vir_bytes)grant_table;
sp->s_grant_entries = GRANT_TABLE_SIZE;
sp->s_grant_endpoint = rp->p_endpoint;

// 2. 创建 Grant 条目，允许 PM 读取驱动状态
grant_table[0].g_endpoint = PM_ENDPOINT;
grant_table[0].g_flags = CPF_READ;
grant_table[0].g_addr = (vir_bytes)&driver_status;
grant_table[0].g_size = sizeof(driver_status);

// 3. PM 使用 sys_safecopyfrom() 安全读取
// 内核验证 grant[0] 的权限后允许访问
```

**与 fork 的关系**：

在 `do_fork()` 中，子进程不继承父进程的 Grant 表：

```c
// 系统进程 fork 时，子进程降级为用户进程
if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;
    // 子进程的 Grant 表被清空
    rpc->p_priv->s_grant_table = 0;
    rpc->p_priv->s_grant_entries = 0;
    rpc->p_priv->s_grant_endpoint = NONE;
}
```

**Rust 设计考虑**：

```rust
/// Grant 条目访问权限标志
bitflags! {
    pub struct GrantFlags: u32 {
        const READ = 0x01;    /* 允许读取 */
        const WRITE = 0x02;   /* 允许写入 */
    }
}

/// Grant 条目
#[derive(Debug, Clone)]
pub struct Grant {
    /// 被授权者端点
    pub endpoint: Endpoint,
    /// 访问权限标志
    pub flags: GrantFlags,
    /// 授权内存的起始虚拟地址
    pub addr: VirAddr,
    /// 授权内存的大小（字节）
    pub size: usize,
}

/// Grant 表
#[derive(Debug)]
pub struct GrantTable {
    /// Grant 表在用户空间中的地址
    pub table_addr: VirAddr,
    /// Grant 条目数量
    pub entries: usize,
    /// Grant 表所属进程的端点
    pub owner: Endpoint,
    /// 预分配的 Grant 条目（可选，用于 Mock）
    grants: Vec<Grant>,
}

impl GrantTable {
    /// 创建新的 Grant 表
    pub fn new(owner: Endpoint, entries: usize) -> Self {
        Self {
            table_addr: VirAddr(0),
            entries,
            owner,
            grants: Vec::with_capacity(entries),
        }
    }

    /// 添加 Grant 条目
    pub fn add_grant(&mut self, grant: Grant) -> Result<usize, GrantError> {
        if self.grants.len() >= self.entries {
            return Err(GrantError::TableFull);
        }
        let index = self.grants.len();
        self.grants.push(grant);
        Ok(index)
    }

    /// 获取 Grant 条目
    pub fn get_grant(&self, index: usize) -> Option<&Grant> {
        self.grants.get(index)
    }

    /// 验证 Grant 访问权限
    pub fn verify_access(
        &self,
        index: usize,
        accessor: Endpoint,
        required_flags: GrantFlags,
    ) -> Result<(), GrantError> {
        let grant = self.get_grant(index)
            .ok_or(GrantError::InvalidIndex)?;

        if grant.endpoint != accessor {
            return Err(GrantError::UnauthorizedAccessor);
        }

        if !grant.flags.contains(required_flags) {
            return Err(GrantError::InsufficientPermissions);
        }

        Ok(())
    }
}

/// Grant 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantError {
    TableFull,
    InvalidIndex,
    UnauthorizedAccessor,
    InsufficientPermissions,
}
```

#### 2.9.5 state 表

State 表（状态表）是 Minix3 内核中用于 RS（重启动服务器）管理进程状态的机制。它允许 RS 在进程启动时预分配和配置进程状态，支持进程的快速重启和状态恢复。

**字段定义**（`priv.h` 第 64-65 行）：

```c
vir_bytes s_state_table;	/* state table address of process, or 0 */
int s_state_entries;		/* no. of entries, or 0 */
```

**字段说明**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `s_state_table` | `vir_bytes` | State 表在用户进程虚拟地址空间中的地址，0 表示未启用 |
| `s_state_entries` | `int` | State 表中的条目数量 |

**State 表结构**：

```c
/* State 表条目结构（RS 使用） */
struct state_entry {
    int st_type;             /* 状态条目类型 */
    int st_flags;            /* 状态标志 */
    vir_bytes st_addr;       /* 状态数据地址 */
    size_t st_size;          /* 状态数据大小 */
    int st_state;            /* 运行时状态值 */
};

/* State 条目类型 */
#define ST_TYPE_PRIV       1   /* 特权信息 */
#define ST_TYPE_ID         2   /* 进程标识 */
#define ST_TYPE_SIGNAL     3   /* 信号状态 */
#define ST_TYPE_MEM        4   /* 内存状态 */
#define ST_TYPE_TIMERS     5   /* 定时器状态 */
```

**State 表的作用**：

```
┌─────────────────────────────────────────────────────────────┐
│                    State 表的作用                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. 进程启动时预配置                                        │
│     - RS 在启动进程前配置 state 表                          │
│     - 定义进程需要的初始状态                                │
│     - 指定恢复时需要的资源                                   │
│                                                             │
│  2. 进程重启时状态恢复                                      │
│     - RS 读取 state 表获取进程状态                          │
│     - 根据 state 表重建进程环境                             │
│     - 恢复信号状态、定时器等                                │
│                                                             │
│  3. 支持高可用性                                            │
│     - 服务崩溃时快速重启                                    │
│     - 保持服务状态一致性                                    │
│     - 最小化服务中断时间                                    │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**典型使用场景**：

```c
// RS 在启动 PM 前配置 state 表
struct priv *sp = priv(pm_rp);

// 分配 state 表
struct state_entry *state_table = alloc_state_table(STATE_TABLE_SIZE);
sp->s_state_table = (vir_bytes)state_table;
sp->s_state_entries = STATE_TABLE_SIZE;

// 配置特权信息状态条目
state_table[0].st_type = ST_TYPE_PRIV;
state_table[0].st_flags = ST_FLAG_PRESERVE;
state_table[0].st_addr = 0;  // 由 RS 填充
state_table[0].st_size = sizeof(struct priv);
state_table[0].st_state = 0;

// 配置进程标识状态条目
state_table[1].st_type = ST_TYPE_ID;
state_table[1].st_flags = ST_FLAG_PRESERVE;
state_table[1].st_addr = 0;
state_table[1].st_size = sizeof(proc_nr_t);
state_table[1].st_state = PM_PROC_NR;

// PM 崩溃重启时，RS 使用 state 表恢复 PM 状态
// 读取 state 表，重建 PM 的特权结构、进程号等
```

**与 fork 的关系**：

在 `do_fork()` 中，子进程不继承父进程的 State 表：

```c
// 系统进程 fork 时，子进程降级为用户进程
if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;
    // 子进程的 State 表被清空
    rpc->p_priv->s_state_table = 0;
    rpc->p_priv->s_state_entries = 0;
    // 只有 RS 可以为进程配置 State 表
}
```

**与 RS 的关系**：

State 表是 RS 实现进程重启的关键数据结构：

```
┌─────────────────────────────────────────────────────────────┐
│                    RS 使用 State 表流程                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. 服务启动阶段                                            │
│     │                                                       │
│     ▼                                                       │
│  ┌─────────────────┐                                        │
│  │ 读取配置文件     │                                        │
│  │ 获取服务参数     │                                        │
│  └─────────────────┘                                        │
│     │                                                       │
│     ▼                                                       │
│  ┌─────────────────┐                                        │
│  │ 配置 State 表    │                                        │
│  │ s_state_table    │                                        │
│  │ s_state_entries  │                                        │
│  └─────────────────┘                                        │
│     │                                                       │
│     ▼                                                       │
│  ┌─────────────────┐                                        │
│  │ 启动服务进程    │                                        │
│  │ exec() 加载    │                                        │
│  └─────────────────┘                                        │
│                                                             │
│  2. 服务崩溃检测                                            │
│     │                                                       │
│     ▼                                                       │
│  ┌─────────────────┐                                        │
│  │ 接收 SIGCHLD   │                                        │
│  │ 或看门狗超时   │                                        │
│  └─────────────────┘                                        │
│     │                                                       │
│     ▼                                                       │
│  ┌─────────────────┐                                        │
│  │ 读取 State 表    │                                        │
│  │ 获取恢复信息     │                                        │
│  └─────────────────┘                                        │
│     │                                                       │
│     ▼                                                       │
│  ┌─────────────────┐                                        │
│  │ 重启服务进程    │                                        │
│  │ 恢复之前状态    │                                        │
│  └─────────────────┘                                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Rust 设计考虑**：

```rust
/// State 条目类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StateType {
    Priv = 1,      /* 特权信息 */
    Id = 2,        /* 进程标识 */
    Signal = 3,    /* 信号状态 */
    Memory = 4,    /* 内存状态 */
    Timers = 5,    /* 定时器状态 */
}

/// State 条目标志
bitflags! {
    pub struct StateFlags: u32 {
        const PRESERVE = 0x01;    /* 保留状态 */
        const RESTART = 0x02;      /* 重启时恢复 */
        const CHECKPOINT = 0x04;   /* 检查点 */
    }
}

/// State 条目
#[derive(Debug, Clone)]
pub struct StateEntry {
    /// 状态条目类型
    pub st_type: StateType,
    /// 状态标志
    pub st_flags: StateFlags,
    /// 状态数据地址
    pub st_addr: VirAddr,
    /// 状态数据大小
    pub st_size: usize,
    /// 运行时状态值
    pub st_state: i32,
}

/// State 表
#[derive(Debug)]
pub struct StateTable {
    /// State 表在用户空间中的地址
    pub table_addr: VirAddr,
    /// State 条目数量
    pub entries: usize,
    /// 预分配的 State 条目（可选，用于 Mock）
    states: Vec<StateEntry>,
}

impl StateTable {
    /// 创建新的 State 表
    pub fn new(entries: usize) -> Self {
        Self {
            table_addr: VirAddr(0),
            entries,
            states: Vec::with_capacity(entries),
        }
    }

    /// 添加 State 条目
    pub fn add_state(&mut self, state: StateEntry) -> Result<usize, StateError> {
        if self.states.len() >= self.entries {
            return Err(StateError::TableFull);
        }
        let index = self.states.len();
        self.states.push(state);
        Ok(index)
    }

    /// 获取 State 条目
    pub fn get_state(&self, index: usize) -> Option<&StateEntry> {
        self.states.get(index)
    }
}

/// State 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateError {
    TableFull,
    InvalidIndex,
    InvalidType,
}
```

---

## 3. Rust 设计决策

本节讨论如何用 Rust 实现特权结构体，包括类型安全、内存管理和抽象设计。

### 3.1 特权结构体设计

在 Rust 中实现 `struct priv` 需要考虑以下设计决策：

**1. 类型安全**

使用强类型替代原始 C 类型，避免类型混淆：

```rust
// 不使用原始整数类型
pub type SysId = u8;        // 替代 sys_id_t
pub type ProcNr = i16;      // 替代 proc_nr_t  
pub type Endpoint = i32;    // 替代 endpoint_t

// 使用 newtype 模式提供类型安全
pub struct PrivId(pub u8);
pub struct ProcNumber(pub i16);
```

**2. 内存布局**

确保与 C 代码的 FFI 兼容性：

```rust
#[repr(C)]  // 使用 C 内存布局
pub struct Priv {
    pub s_proc_nr: ProcNr,
    pub s_id: SysId,
    pub s_flags: i16,
    // ...
}
```

**3. 资源管理**

使用 Rust 的所有权系统管理 Grant 表、State 表等资源：

```rust
pub struct Priv {
    // 基础字段
    pub s_proc_nr: ProcNr,
    pub s_id: SysId,
    
    // 使用 Option 表示可选资源
    pub grant_table: Option<Box<GrantTable>>,
    pub state_table: Option<Box<StateTable>>,
}
```

### 3.2 权限掩码

权限掩码（如 `s_k_call_mask`、`s_trap_mask`）使用位数组实现：

```rust
/// 内核调用掩码大小
pub const SYS_CALL_MASK_SIZE: usize = 16;

/// 位数组实现的权限掩码
pub struct PermissionMask {
    /// 存储位数组
    chunks: [u64; SYS_CALL_MASK_SIZE],
}

impl PermissionMask {
    /// 创建空的权限掩码
    pub fn new() -> Self {
        Self {
            chunks: [0; SYS_CALL_MASK_SIZE],
        }
    }

    /// 检查是否允许指定调用号
    pub fn is_allowed(&self, call_nr: usize) -> bool {
        let chunk_idx = call_nr / 64;
        let bit_idx = call_nr % 64;
        if chunk_idx >= SYS_CALL_MASK_SIZE {
            return false;
        }
        (self.chunks[chunk_idx] & (1 << bit_idx)) != 0
    }

    /// 设置允许指定调用号
    pub fn allow(&mut self, call_nr: usize) {
        let chunk_idx = call_nr / 64;
        let bit_idx = call_nr % 64;
        if chunk_idx < SYS_CALL_MASK_SIZE {
            self.chunks[chunk_idx] |= 1 << bit_idx;
        }
    }

    /// 清除指定调用号的权限
    pub fn deny(&mut self, call_nr: usize) {
        let chunk_idx = call_nr / 64;
        let bit_idx = call_nr % 64;
        if chunk_idx < SYS_CALL_MASK_SIZE {
            self.chunks[chunk_idx] &= !(1 << bit_idx);
        }
    }
}
```

### 3.3 资源限制

资源限制（如 `s_nr_io_range`、`s_nr_mem_range`）使用计数器和固定大小数组实现：

```rust
/// I/O 端口范围表大小
pub const NR_IO_RANGE: usize = 64;

/// 内存范围表大小
pub const NR_MEM_RANGE: usize = 20;

/// IRQ 表大小
pub const NR_IRQ: usize = 16;

/// 资源限制管理
pub struct ResourceLimits {
    /// I/O 端口范围数量
    pub nr_io_range: usize,
    /// I/O 端口范围表
    pub io_tab: [IoRange; NR_IO_RANGE],

    /// 内存范围数量
    pub nr_mem_range: usize,
    /// 内存范围表
    pub mem_tab: [MemRange; NR_MEM_RANGE],

    /// IRQ 数量
    pub nr_irq: usize,
    /// IRQ 表
    pub irq_tab: [Option<IrqNumber>; NR_IRQ],
}

impl ResourceLimits {
    /// 创建空的资源限制
    pub fn new() -> Self {
        Self {
            nr_io_range: 0,
            io_tab: [IoRange { base: 0, limit: 0 }; NR_IO_RANGE],
            nr_mem_range: 0,
            mem_tab: [MemRange { base: PhysAddr(0), limit: PhysAddr(0) }; NR_MEM_RANGE],
            nr_irq: 0,
            irq_tab: [None; NR_IRQ],
        }
    }

    /// 添加 I/O 端口范围
    pub fn add_io_range(&mut self, range: IoRange) -> Result<(), ResourceError> {
        if self.nr_io_range >= NR_IO_RANGE {
            return Err(ResourceError::IoRangeTableFull);
        }
        self.io_tab[self.nr_io_range] = range;
        self.nr_io_range += 1;
        Ok(())
    }

    /// 检查 I/O 端口是否允许访问
    pub fn check_io_permission(&self, port: u16) -> bool {
        for i in 0..self.nr_io_range {
            if self.io_tab[i].contains(port) {
                return true;
            }
        }
        false
    }
}

/// 资源错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceError {
    IoRangeTableFull,
    MemRangeTableFull,
    IrqTableFull,
}
```

---

## 4. 实现

本节给出 Rust 实现代码，包括 Priv 结构体定义、特权标志定义和单元测试。

### 4.1 Priv 结构体定义

```rust
/// 特权结构体 - Rust 实现
/// 
/// 对应 Minix3 的 struct priv
#[repr(C)]
pub struct Priv {
    // 进程关联字段
    /// 关联的进程号
    pub s_proc_nr: ProcNr,
    /// 特权表索引
    pub s_id: SysId,
    
    // 标志字段
    /// 特权标志（SYS_PROC、BILLABLE 等）
    pub s_flags: i16,
    /// 初始化标志
    pub s_init_flags: i32,
    
    // 异步发送字段
    /// 异步发送表地址
    pub s_asyntab: VirBytes,
    /// 异步发送表大小
    pub s_asynsize: usize,
    /// 异步发送端点
    pub s_asynendpoint: Endpoint,
    
    // 系统调用控制字段
    /// 系统调用陷阱掩码
    pub s_trap_mask: i16,
    /// IPC 目标映射
    pub s_ipc_to: SysMap,
    /// 内核调用掩码
    pub s_k_call_mask: [BitChunk; SYS_CALL_MASK_SIZE],
    
    // 信号管理字段
    /// 信号管理器端点
    pub s_sig_mgr: Endpoint,
    /// 备用信号管理器端点
    pub s_bak_sig_mgr: Endpoint,
    
    // 待处理事件字段
    /// 待处理通知位图
    pub s_notify_pending: SysMap,
    /// 待处理异步消息位图
    pub s_asyn_pending: SysMap,
    /// 待处理硬件中断
    pub s_int_pending: IrqId,
    /// 待处理信号
    pub s_sig_pending: SigSet,
    
    // 其他字段
    /// IPC 过滤器指针
    pub s_ipcf: Option<Box<IpcFilter>>,
    /// 同步闹钟定时器
    pub s_alarm_timer: MinixTimer,
    /// 栈保护字指针
    pub s_stack_guard: Option<RegT>,
    /// 诊断信号标志
    pub s_diag_sig: i8,
    
    // 资源访问字段
    /// I/O 端口范围数量
    pub s_nr_io_range: i32,
    /// I/O 端口范围表
    pub s_io_tab: [IoRange; NR_IO_RANGE],
    /// 内存范围数量
    pub s_nr_mem_range: i32,
    /// 内存范围表
    pub s_mem_tab: [MemRange; NR_MEM_RANGE],
    /// IRQ 数量
    pub s_nr_irq: i32,
    /// IRQ 表
    pub s_irq_tab: [IrqNumber; NR_IRQ],
    /// Grant 表地址
    pub s_grant_table: VirBytes,
    /// Grant 表条目数量
    pub s_grant_entries: i32,
    /// Grant 表所属端点
    pub s_grant_endpoint: Endpoint,
    /// State 表地址
    pub s_state_table: VirBytes,
    /// State 表条目数量
    pub s_state_entries: i32,
}

impl Priv {
    /// 创建新的特权结构体
    pub fn new(proc_nr: ProcNr, id: SysId) -> Self {
        Self {
            s_proc_nr: proc_nr,
            s_id: id,
            s_flags: 0,
            s_init_flags: 0,
            s_asyntab: 0,
            s_asynsize: 0,
            s_asynendpoint: Endpoint::NONE,
            s_trap_mask: 0,
            s_ipc_to: SysMap(0),
            s_k_call_mask: [0; SYS_CALL_MASK_SIZE],
            s_sig_mgr: Endpoint::NONE,
            s_bak_sig_mgr: Endpoint::NONE,
            s_notify_pending: SysMap(0),
            s_asyn_pending: SysMap(0),
            s_int_pending: 0,
            s_sig_pending: SigSet(0),
            s_ipcf: None,
            s_alarm_timer: MinixTimer::new(),
            s_stack_guard: None,
            s_diag_sig: 0,
            s_nr_io_range: 0,
            s_io_tab: [IoRange { base: 0, limit: 0 }; NR_IO_RANGE],
            s_nr_mem_range: 0,
            s_mem_tab: [MemRange { base: PhysAddr(0), limit: PhysAddr(0) }; NR_MEM_RANGE],
            s_nr_irq: 0,
            s_irq_tab: [0; NR_IRQ],
            s_grant_table: 0,
            s_grant_entries: 0,
            s_grant_endpoint: Endpoint::NONE,
            s_state_table: 0,
            s_state_entries: 0,
        }
    }

    /// 检查是否为系统进程
    pub fn is_sys_proc(&self) -> bool {
        self.s_flags & SYS_PROC != 0
    }

    /// 设置系统进程标志
    pub fn set_sys_proc(&mut self) {
        self.s_flags |= SYS_PROC;
    }

    /// 清除系统进程标志
    pub fn clear_sys_proc(&mut self) {
        self.s_flags &= !SYS_PROC;
    }

    /// 检查是否允许指定内核调用
    pub fn check_k_call(&self, call_nr: usize) -> bool {
        let chunk_idx = call_nr / 64;
        let bit_idx = call_nr % 64;
        if chunk_idx >= SYS_CALL_MASK_SIZE {
            return false;
        }
        (self.s_k_call_mask[chunk_idx] & (1 << bit_idx)) != 0
    }

    /// 允许指定内核调用
    pub fn allow_k_call(&mut self, call_nr: usize) {
        let chunk_idx = call_nr / 64;
        let bit_idx = call_nr % 64;
        if chunk_idx < SYS_CALL_MASK_SIZE {
            self.s_k_call_mask[chunk_idx] |= 1 << bit_idx;
        }
    }
}
```

### 4.2 特权标志定义

```rust
/// 特权标志位定义
pub mod priv_flags {
    /// 系统进程标志
    pub const SYS_PROC: i16 = 0x001;
    /// 可计费进程标志
    pub const BILLABLE: i16 = 0x002;
    /// 可抢占进程标志
    pub const PREEMPTIBLE: i16 = 0x004;
    /// 内核任务标志
    pub const KERNEL: i16 = 0x008;
    /// VM 相关标志
    pub const VM_F: i16 = 0x010;
    /// 允许 sendrec 标志
    pub const SENDREC_F: i16 = 0x020;
    /// 驱动程序标志
    pub const DRIVER_F: i16 = 0x040;
    /// 服务器进程标志
    pub const SERVER_F: i16 = 0x080;
}

/// 初始化标志位定义
pub mod init_flags {
    /// 允许系统调用拦截
    pub const INTERCEPT_F: i32 = 0x001;
    /// 孤儿进程标志
    pub const ORPHAN_F: i32 = 0x002;
    /// 允许 SIGINT
    pub const SIGINT_F: i32 = 0x004;
    /// 允许 SIGQUIT
    pub const SIGQUIT_F: i32 = 0x008;
    /// 允许 SIGILL
    pub const SIGILL_F: i32 = 0x010;
    /// 允许 SIGTRAP
    pub const SIGTRAP_F: i32 = 0x020;
    /// 允许 SIGABRT
    pub const SIGABRT_F: i32 = 0x040;
    /// 允许 SIGBUS
    pub const SIGBUS_F: i32 = 0x080;
    /// 允许 SIGFPE
    pub const SIGFPE_F: i32 = 0x100;
    /// 允许 SIGUSR1
    pub const SIGUSR1_F: i32 = 0x200;
    /// 允许 SIGSEGV
    pub const SIGSEGV_F: i32 = 0x400;
    /// 允许 SIGUSR2
    pub const SIGUSR2_F: i32 = 0x800;
}

/// 陷阱号定义（部分）
pub mod trap_nr {
    pub const SEND: u8 = 0;
    pub const RECEIVE: u8 = 1;
    pub const SENDREC: u8 = 2;
    pub const NOTIFY: u8 = 3;
    pub const SENDA: u8 = 4;
    pub const RECEIVEA: u8 = 5;
}

/// 内核调用号定义（部分）
pub mod sys_call_nr {
    pub const FORK: usize = 0;
    pub const EXEC: usize = 1;
    pub const EXIT: usize = 2;
    pub const PRIVCTL: usize = 3;
    pub const TRACE: usize = 4;
    pub const KILL: usize = 5;
    pub const SEGCTL: usize = 6;
    pub const NEWMAP: usize = 7;
    pub const VREBOOT: usize = 8;
    pub const IRQCTL: usize = 9;
    pub const DEVIO: usize = 10;
    pub const SDEVIO: usize = 11;
    pub const VDEVIO: usize = 12;
    pub const SETALARM: usize = 13;
    pub const TIMES: usize = 14;
    pub const GETINFO: usize = 15;
    pub const ABORT: usize = 16;
    pub const UMAP: usize = 17;
    pub const VIRCOPY: usize = 18;
    pub const PHYSCOPY: usize = 19;
    pub const VIRVCOPY: usize = 20;
    pub const PHYSVCOPY: usize = 21;
    pub const MEMSET: usize = 22;
    pub const SAFECOPYFROM: usize = 23;
    pub const SAFECOPYTO: usize = 24;
    pub const VSAFECOPY: usize = 25;
    pub const SYS_PAD: usize = 26;
    pub const PROFILE: usize = 27;
}
```

### 4.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 Priv 结构体创建
    #[test]
    fn test_priv_new() {
        let priv_struct = Priv::new(1, 0);
        assert_eq!(priv_struct.s_proc_nr, 1);
        assert_eq!(priv_struct.s_id, 0);
        assert!(!priv_struct.is_sys_proc());
    }

    /// 测试系统进程标志
    #[test]
    fn test_sys_proc_flags() {
        let mut priv_struct = Priv::new(1, 0);
        
        // 初始状态不是系统进程
        assert!(!priv_struct.is_sys_proc());
        
        // 设置为系统进程
        priv_struct.set_sys_proc();
        assert!(priv_struct.is_sys_proc());
        
        // 清除系统进程标志
        priv_struct.clear_sys_proc();
        assert!(!priv_struct.is_sys_proc());
    }

    /// 测试内核调用权限检查
    #[test]
    fn test_k_call_permission() {
        let mut priv_struct = Priv::new(1, 0);
        
        // 初始状态不允许任何调用
        assert!(!priv_struct.check_k_call(sys_call_nr::FORK));
        
        // 允许 FORK 调用
        priv_struct.allow_k_call(sys_call_nr::FORK);
        assert!(priv_struct.check_k_call(sys_call_nr::FORK));
        
        // 其他调用仍然不允许
        assert!(!priv_struct.check_k_call(sys_call_nr::EXEC));
    }

    /// 测试 I/O 端口权限
    #[test]
    fn test_io_permission() {
        let mut priv_struct = Priv::new(1, 0);
        
        // 添加 I/O 端口范围 0x3F8-0x3FF
        let io_range = IoRange { base: 0x3F8, limit: 0x3FF };
        priv_struct.s_io_tab[0] = io_range;
        priv_struct.s_nr_io_range = 1;
        
        // 检查端口权限
        assert!(priv_struct.s_io_tab[0].contains(0x3F8));
        assert!(priv_struct.s_io_tab[0].contains(0x3FF));
        assert!(!priv_struct.s_io_tab[0].contains(0x3F7));
        assert!(!priv_struct.s_io_tab[0].contains(0x400));
    }

    /// 测试内存范围权限
    #[test]
    fn test_mem_permission() {
        let mut priv_struct = Priv::new(1, 0);
        
        // 添加内存范围 0xA0000-0xBFFFF（VGA 帧缓冲）
        let mem_range = MemRange {
            base: PhysAddr(0xA0000),
            limit: PhysAddr(0xBFFFF),
        };
        priv_struct.s_mem_tab[0] = mem_range;
        priv_struct.s_nr_mem_range = 1;
        
        // 检查内存权限
        assert!(priv_struct.s_mem_tab[0].contains(PhysAddr(0xA0000)));
        assert!(priv_struct.s_mem_tab[0].contains(PhysAddr(0xBFFFF)));
        assert!(!priv_struct.s_mem_tab[0].contains(PhysAddr(0x9FFFF)));
        assert!(!priv_struct.s_mem_tab[0].contains(PhysAddr(0xC0000)));
    }

    /// 测试 IRQ 权限
    #[test]
    fn test_irq_permission() {
        let mut priv_struct = Priv::new(1, 0);
        
        // 添加 IRQ 4（COM1）
        priv_struct.s_irq_tab[0] = 4;
        priv_struct.s_nr_irq = 1;
        
        // 检查 IRQ 权限
        assert_eq!(priv_struct.s_irq_tab[0], 4);
        assert_eq!(priv_struct.s_nr_irq, 1);
    }

    /// 测试 Grant 表操作
    #[test]
    fn test_grant_table() {
        let mut grant_table = GrantTable::new(Endpoint::from_raw(1), 10);
        
        // 创建 Grant 条目
        let grant = Grant {
            endpoint: Endpoint::from_raw(2),
            flags: GrantFlags::READ | GrantFlags::WRITE,
            addr: VirAddr(0x1000),
            size: 4096,
        };
        
        // 添加 Grant 条目
        let index = grant_table.add_grant(grant).unwrap();
        assert_eq!(index, 0);
        
        // 验证 Grant 条目
        let retrieved = grant_table.get_grant(index).unwrap();
        assert_eq!(retrieved.endpoint, Endpoint::from_raw(2));
        assert!(retrieved.flags.contains(GrantFlags::READ));
        assert!(retrieved.flags.contains(GrantFlags::WRITE));
    }

    /// 测试 State 表操作
    #[test]
    fn test_state_table() {
        let mut state_table = StateTable::new(10);
        
        // 创建 State 条目
        let state = StateEntry {
            st_type: StateType::Priv,
            st_flags: StateFlags::PRESERVE,
            st_addr: VirAddr(0),
            st_size: 100,
            st_state: 0,
        };
        
        // 添加 State 条目
        let index = state_table.add_state(state).unwrap();
        assert_eq!(index, 0);
        
        // 验证 State 条目
        let retrieved = state_table.get_state(index).unwrap();
        assert_eq!(retrieved.st_type, StateType::Priv);
        assert!(retrieved.st_flags.contains(StateFlags::PRESERVE));
    }
}
```

---

## 5. 参见

- [08-proc-macros](08-proc-macros.md) - 进程访问宏
- [10-priv-macros](10-priv-macros.md) - 特权访问宏
- [18-do-fork-priv](18-do-fork-priv.md) - fork 特权处理
