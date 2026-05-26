# 11-privilege: struct priv 特权结构体

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/priv.h`(80行), `system.c` 特权操作(918-973)
> **说明**: priv 结构体的每个字段、s_k_call_mask 位图、priv_add_irq/io/mem——谁有权做什么的权限矩阵

---

## 1. 概述

### 1.1 概念定义/作用

**特权结构体（struct priv）** 是 Minix3 内核中管理进程权限的核心数据结构。它定义了一个进程"被允许做什么"——能调用哪些系统调用、能向谁发送 IPC 消息、能访问哪些 I/O 端口、能使用哪些 IRQ 线、能映射哪些内存区域。

Minix3 将进程分为两类，每类有不同的特权管理方式：

1. **系统进程**（`s_flags & SYS_PROC`）：如 PM、VFS、VM、RS 等服务进程，每个拥有独立的 `struct priv` 实例，包含完整的权限信息
2. **用户进程**：所有用户进程共享一个默认的 `struct priv` 实例（`USER_PRIV_ID`），权限受限

这种分离设计是 Minix3 安全模型的基础：系统进程拥有细粒度的权限控制，用户进程则被限制在最小权限集合内。内核在每次 IPC 消息传递和系统调用时检查特权结构，确保进程不会越权操作。

### 1.2 与 Minix3 的对应关系

特权结构体的核心实现分布在以下文件：

| 功能 | 文件 | 关键符号 |
|------|------|---------|
| priv 结构体定义 | `minix3/minix/kernel/priv.h:21-66` | `struct priv` |
| 特权标志定义 | `minix3/minix/include/minix/priv.h` | `TSK_F/SRV_F/USR_F` 等 |
| 特权表全局数组 | `minix3/minix/kernel/priv.h:94-95` | `priv[NR_SYS_PROCS]`, `ppriv_addr[]` |
| 特权操作函数 | `minix3/minix/kernel/system.c:918-996` | `priv_add_irq/io/mem` |
| 特权控制入口 | `minix3/minix/kernel/system.c` | `do_privctl()` |
| 权限检查宏 | `minix3/minix/kernel/priv.h:86-87` | `may_send_to`, `may_asynsend_to` |
| IPC 过滤器 | `minix3/minix/kernel/ipc_filter.h` | `ipc_filter_t` |

### 1.3 关键状态/机制说明

**特权表（priv table）**：全局静态数组 `priv[NR_SYS_PROCS]`，每个系统进程占用一个槽位。`ppriv_addr[id]` 是快速索引数组，通过特权 ID 直接定位到对应的 `struct priv` 实例，避免乘法计算偏移。

**特权 ID 分配**：特权 ID 分为静态和动态两部分：
- **静态 ID**（0 ~ NR_BOOT_PROCS-1）：启动时分配给 boot image 中的进程，永不释放
- **动态 ID**（NR_BOOT_PROCS ~ NR_SYS_PROCS-1）：运行时分配给动态启动的服务进程（如驱动），可释放和重用

**权限检查流程**：每次 IPC 发送时，内核调用 `may_send_to(rp, nr)` 检查发送方的 `s_ipc_to` 位图是否允许向目标进程发送消息。每次系统调用时，内核检查 `s_k_call_mask` 位图是否允许该调用号。

**进程类型特权模板**：Minix3 为不同类型的进程定义了特权模板（`TSK_F/SRV_F/USR_F` 等），包含标志、陷阱掩码、IPC 目标、内核调用掩码、信号管理器、调度器、优先级、时间片等全套配置。新进程创建时从对应模板初始化特权结构。

### 1.4 行为规则

1. **系统进程独立特权**：每个系统进程有独立的 `struct priv`，互不影响
2. **用户进程共享特权**：所有用户进程共享 `USER_PRIV_ID` 指向的同一个 `struct priv`，权限相同且受限
3. **IPC 权限双向检查**：发送方检查 `s_ipc_to`（允许发给谁），接收方检查 IPC 过滤器（允许接收谁的消息）
4. **内核调用掩码精确控制**：`s_k_call_mask` 位图中每一位对应一个内核调用号，逐调用控制权限
5. **I/O 端口 / IRQ / 内存范围**：系统进程通过 `priv_add_io/irq/mem` 逐条添加权限，有上限（NR_IO_RANGE=64, NR_IRQ=16, NR_MEM_RANGE=20）
6. **特权 ID 回收**：动态服务进程退出时释放特权 ID，重启时重新分配

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 特权表规模常量

| 常量 | 值 | 定义位置 | 含义 |
|------|-----|---------|------|
| `NR_SYS_PROCS` | 64 | `minix3/minix/include/minix/sys_config.h` | 特权结构总数（系统进程上限） |
| `NR_BOOT_PROCS` | ~15 | `minix3/minix/include/minix/com.h` | 启动映像中的进程数 |
| `NR_STATIC_PRIV_IDS` | `NR_BOOT_PROCS` | `minix3/minix/include/minix/priv.h:10` | 静态特权 ID 数 |
| `NR_IO_RANGE` | 64 | `minix3/minix/include/minix/config.h:52` | 每个 priv 最大 I/O 端口范围数 |
| `NR_MEM_RANGE` | 20 | `minix3/minix/include/minix/config.h:55` | 每个 priv 最大内存范围数 |
| `NR_IRQ` | 16 | `minix3/minix/include/minix/config.h:58` | 每个 priv 最大 IRQ 线数 |
| `SYS_CALL_MASK_SIZE` | `BITMAP_CHUNKS(NR_SYS_CALLS)` | `minix3/minix/include/minix/com.h:272` | 内核调用掩码位数组大小 |

#### 2.1.2 特权标志（s_flags）

| 标志 | 值 | 含义 |
|------|-----|------|
| `PREEMPTIBLE` | 0x002 | 进程可被抢占 |
| `BILLABLE` | 0x004 | 进程可被计费（时钟中断统计 CPU 时间） |
| `DYN_PRIV_ID` | 0x008 | 特权 ID 动态分配（可释放和重用） |
| `SYS_PROC` | 0x010 | 系统进程（拥有独立 priv 结构） |
| `CHECK_IO_PORT` | 0x020 | 检查 I/O 端口权限 |
| `CHECK_IRQ` | 0x040 | 检查 IRQ 权限 |
| `CHECK_MEM` | 0x080 | 检查内存映射权限 |
| `ROOT_SYS_PROC` | 0x100 | 根系统进程实例（如 RS） |
| `VM_SYS_PROC` | 0x200 | VM 系统进程实例 |
| `LU_SYS_PROC` | 0x400 | 热更新系统进程实例 |
| `RST_SYS_PROC` | 0x800 | 重启的系统进程实例 |

#### 2.1.3 进程类型特权模板

| 模板 | 标志 | 陷阱掩码 | IPC 目标 | 内核调用 | 信号管理器 |
|------|------|---------|---------|---------|-----------|
| `IDL_F` | `SYS_PROC \| BILLABLE` | — | — | `NO_C` | — |
| `TSK_F` | `SYS_PROC` | `0` | `NO_M` | `NO_C` | — |
| `SRV_F` | `SYS_PROC \| PREEMPTIBLE` | `~0`（全部） | `ALL_M` | `ALL_C` | `ROOT_SYS_PROC_NR` |
| `DSRV_F` | `SRV_F \| DYN_PRIV_ID` | `~0` | `ALL_M` | `ALL_C` | `ROOT_SYS_PROC_NR` |
| `RSYS_F` | `SRV_F \| ROOT_SYS_PROC` | `~0` | `ALL_M` | `ALL_C` | — |
| `VM_F` | `SYS_PROC \| VM_SYS_PROC` | — | — | — | — |
| `USR_F` | `BILLABLE \| PREEMPTIBLE` | `1<<SENDREC` | `ALL_M` | `NO_C` | `PM_PROC_NR` |
| `IMM_F` | `ROOT_SYS_PROC \| VM_SYS_PROC \| PREEMPTIBLE` | — | — | — | — |

**关键对比**：
- **内核任务**（TSK_F）：无 IPC 目标限制（`NO_M`），无内核调用权限（`NO_C`），无陷阱掩码（只能通过 SENDREC 回复）
- **系统服务**（SRV_F）：全部 IPC 目标（`ALL_M`），全部内核调用（`ALL_C`），全部陷阱
- **用户进程**（USR_F）：全部 IPC 目标，无内核调用，仅 SENDREC 陷阱，PM 作为信号管理器

#### 2.1.4 IPC 目标掩码常量

| 常量 | 值 | 含义 |
|------|-----|------|
| `NO_M` | -1 | 不允许向任何进程发送 IPC |
| `ALL_M` | -2 | 允许向所有进程发送 IPC |

#### 2.1.5 内核调用掩码常量

| 常量 | 值 | 含义 |
|------|-----|------|
| `NO_C` | -1 | 不允许任何内核调用 |
| `ALL_C` | -2 | 允许所有内核调用 |
| `NULL_C` | -3 | 空调用条目 |

### 2.2 核心数据结构

#### 2.2.1 struct priv（特权结构体）

定义于 `minix3/minix/kernel/priv.h:21-66`，按功能分组：

**进程关联**

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_proc_nr` | `proc_nr_t` | 关联的进程号，`NONE` 表示槽位空闲 |
| `s_id` | `sys_id_t` | 特权结构索引（0 ~ NR_SYS_PROCS-1） |
| `s_flags` | `short` | 特权标志（PREEMPTIBLE/BILLABLE/SYS_PROC 等） |
| `s_init_flags` | `int` | 初始化标志（传递给进程的初始权限信息） |

**异步 IPC**

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_asyntab` | `vir_bytes` | 异步消息表地址（发送方地址空间内），-1 表示无活跃表 |
| `s_asynsize` | `size_t` | 异步消息表元素数，0 表示无活跃表 |
| `s_asynendpoint` | `endpoint_t` | 异步表所属的 endpoint（验证表所有者） |

**IPC 权限**

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_trap_mask` | `short` | 允许的 IPC 陷阱掩码（按位对应 SEND/RECEIVE/SENDREC 等） |
| `s_ipc_to` | `sys_map_t` | 允许的 IPC 目标进程位图 |
| `s_k_call_mask[SYS_CALL_MASK_SIZE]` | `bitchunk_t[]` | 允许的内核调用掩码（每一位对应一个内核调用号） |

**信号与通知**

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_sig_mgr` | `endpoint_t` | 信号管理器 endpoint |
| `s_bak_sig_mgr` | `endpoint_t` | 备份信号管理器 endpoint |
| `s_notify_pending` | `sys_map_t` | 待处理通知位图 |
| `s_asyn_pending` | `sys_map_t` | 待处理异步消息位图 |
| `s_int_pending` | `irq_id_t` | 待处理硬件中断 |
| `s_sig_pending` | `sigset_t` | 待处理信号 |
| `s_ipcf` | `ipc_filter_t *` | IPC 过滤器（NULL 表示无过滤） |

**定时器与调试**

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_alarm_timer` | `minix_timer_t` | 同步闹钟定时器 |
| `s_stack_guard` | `reg_t *` | 内核任务栈保护字指针 |
| `s_diag_sig` | `char` | 诊断到达时是否发 SIGKMESS |

**硬件资源权限**

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_nr_io_range` | `int` | 允许的 I/O 端口范围数 |
| `s_io_tab[NR_IO_RANGE]` | `struct io_range[]` | I/O 端口范围表 |
| `s_nr_mem_range` | `int` | 允许的内存范围数 |
| `s_mem_tab[NR_MEM_RANGE]` | `struct minix_mem_range[]` | 内存范围表 |
| `s_nr_irq` | `int` | 允许的 IRQ 线数 |
| `s_irq_tab[NR_IRQ]` | `int[]` | IRQ 表 |

**Grant 与状态表**

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_grant_table` | `vir_bytes` | grant 表地址（进程地址空间内），0 表示无 |
| `s_grant_entries` | `int` | grant 表条目数，0 表示无 |
| `s_grant_endpoint` | `endpoint_t` | grant 表所属 endpoint |
| `s_state_table` | `vir_bytes` | 状态表地址（进程地址空间内），0 表示无 |
| `s_state_entries` | `int` | 状态表条目数，0 表示无 |

#### 2.2.2 struct io_range（I/O 端口范围）

定义于 `minix3/minix/include/minix/type.h:133-137`：

| 字段 | 类型 | 含义 |
|------|------|------|
| `ior_base` | `unsigned` | 范围内最低 I/O 端口号 |
| `ior_limit` | `unsigned` | 范围内最高 I/O 端口号 |

#### 2.2.3 struct minix_mem_range（内存范围）

定义于 `minix3/minix/include/minix/type.h:139-143`：

| 字段 | 类型 | 含义 |
|------|------|------|
| `mr_base` | `phys_bytes` | 范围内最低物理内存地址 |
| `mr_limit` | `phys_bytes` | 范围内最高物理内存地址 |

### 2.3 关键函数分析

#### 2.3.1 priv_add_irq()——添加 IRQ 权限

`minix3/minix/kernel/system.c:918-940`

```c
int priv_add_irq(struct proc *rp, int irq)
```

**功能**：为进程添加一条 IRQ 线的访问权限。

**行为**：
1. 设置 `CHECK_IRQ` 标志，启用 IRQ 权限检查
2. 检查是否已有此 IRQ 权限（去重）
3. 若 `s_nr_irq >= NR_IRQ(16)`，返回 `ENOMEM`
4. 将 IRQ 号追加到 `s_irq_tab`，`s_nr_irq++`

#### 2.3.2 priv_add_io()——添加 I/O 端口范围权限

`minix3/minix/kernel/system.c:945-968`

```c
int priv_add_io(struct proc *rp, struct io_range *ior)
```

**功能**：为进程添加一个 I/O 端口范围的访问权限。

**行为**：
1. 设置 `CHECK_IO_PORT` 标志，启用 I/O 端口权限检查
2. 检查是否已有相同范围（base 和 limit 都匹配才去重）
3. 若 `s_nr_io_range >= NR_IO_RANGE(64)`，返回 `ENOMEM`
4. 将 `io_range` 追加到 `s_io_tab`，`s_nr_io_range++`

#### 2.3.3 priv_add_mem()——添加内存范围权限

`minix3/minix/kernel/system.c:973-996`

```c
int priv_add_mem(struct proc *rp, struct minix_mem_range *memr)
```

**功能**：为进程添加一个内存范围的访问权限。

**行为**：
1. 设置 `CHECK_MEM` 标志，启用内存映射权限检查
2. 检查是否已有相同范围（base 和 limit 都匹配才去重）
3. 若 `s_nr_mem_range >= NR_MEM_RANGE(20)`，返回 `ENOMEM`
4. 将 `minix_mem_range` 追加到 `s_mem_tab`，`s_nr_mem_range++`

#### 2.3.4 权限检查宏

| 宏 | 定义 | 功能 |
|-----|------|------|
| `priv(rp)` | `(rp)->p_priv` | 取进程的特权结构指针 |
| `priv_id(rp)` | `(rp)->p_priv->s_id` | 取进程的特权 ID |
| `priv_addr(i)` | `ppriv_addr[(i)]` | 按特权 ID 取 priv 指针 |
| `id_to_nr(id)` | `priv_addr(id)->s_proc_nr` | 特权 ID → 进程号 |
| `nr_to_id(nr)` | `priv(proc_addr(nr))->s_id` | 进程号 → 特权 ID |
| `may_send_to(rp, nr)` | `get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr))` | 检查 IPC 发送权限 |
| `may_asynsend_to(rp, nr)` | `may_send_to(rp, nr) \|\| (rp)->p_nr == nr` | 检查异步发送权限（允许自发自收） |
| `is_static_priv_id(id)` | `id >= 0 && id < NR_STATIC_PRIV_IDS` | 判断是否为静态特权 ID |

#### 2.3.5 allow_ipc_filtered_msg()——IPC 过滤器消息检查

`minix3/minix/kernel/system.c:790-874`

```c
int allow_ipc_filtered_msg(struct proc *rp, endpoint_t src_e,
    vir_bytes m_src_v, message *m_src_p)
```

**功能**：检查消息是否通过接收方的 IPC 过滤器。

**行为**：
1. 若接收方无过滤器（`s_ipcf == NULL`），允许所有消息
2. 若过滤器需要匹配 `m_type`，从发送方地址空间复制 `m_type` 字段
3. 遍历过滤器链表，对每个过滤器段：
   - 白名单（`IPCF_WHITELIST`）：匹配任一元素则允许
   - 黑名单（`IPCF_BLACKLIST`）：匹配任一元素则拒绝
4. 返回是否允许

### 2.4 调用关系/调用点分析

#### 2.4.1 特权操作调用链

```
RS (Reincarnation Server) 管理服务进程
  └─ sys_privctl(endpoint, request, arg)
       └─ do_privctl()
            ├─ SYS_PRIV_INIT:  初始化特权结构
            ├─ SYS_PRIV_ADD_IO:   priv_add_io()
            ├─ SYS_PRIV_ADD_IRQ:  priv_add_irq()
            ├─ SYS_PRIV_ADD_MEM:  priv_add_mem()
            ├─ SYS_PRIV_SET_FLAGS: 设置 s_flags
            ├─ SYS_PRIV_SET_GRANT: 设置 grant 表
            └─ SYS_PRIV_SET_STATE: 设置状态表
```

#### 2.4.2 权限检查调用点

| 检查宏 | 调用场景 | 调用者 |
|--------|---------|--------|
| `may_send_to()` | IPC 发送权限 | `do_sync_ipc()`, `mini_send()` |
| `may_asynsend_to()` | 异步发送权限 | `try_deliver_senda()`, `try_one()` |
| `s_k_call_mask` | 内核调用权限 | `kernel_call_dispatch()` |
| `s_trap_mask` | IPC 陷阱权限 | `do_sync_ipc()` |
| `CHECK_IO_PORT` | I/O 端口权限 | `do_devio()` |
| `CHECK_IRQ` | IRQ 权限 | `do_irqctl()` |
| `CHECK_MEM` | 内存映射权限 | `do_umap()` / `do_vircopy()` |
| `s_ipcf` | IPC 消息过滤 | `CANRECEIVE` 宏 |

### 2.5 设计要点/特殊处理

#### 2.5.1 特权结构与进程结构分离

`struct priv` 与 `struct proc` 分离存储，通过 `p_priv` 指针间接访问。这种设计有三个核心优势：

1. **空间效率**：`NR_SYS_PROCS(64)` 远小于 `NR_PROCS(256)`，用户进程共享一个 priv 实例，节省大量内存
2. **安全隔离**：特权信息与基本进程信息分离，减少内核代码意外修改特权字段的风险
3. **动态管理**：系统进程重启时可以重新分配和初始化特权结构，不影响其他进程

#### 2.5.2 ppriv_addr 快速索引

`ppriv_addr[id]` 数组存储了每个特权 ID 对应的 `struct priv *` 指针。这避免了通过 `&priv[id]` 计算地址时的乘法操作（`id * sizeof(struct priv)`）。由于 `struct priv` 较大（含 `s_io_tab[64]`、`s_mem_tab[20]`、`s_irq_tab[16]`），直接数组索引比乘法偏移更高效。

#### 2.5.3 NO_M / ALL_M 与位图的关系

`s_ipc_to` 是 `sys_map_t` 位图，但初始化时使用 `NO_M(-1)` 和 `ALL_M(-2)` 作为特殊值：

- `NO_M(-1)`：初始化时将位图全部清零，不允许向任何进程发送
- `ALL_M(-2)`：初始化时将位图全部置位，允许向所有进程发送

实际权限检查通过 `get_sys_bit(s_ipc_to, id)` 逐位进行。

#### 2.5.4 STACK_GUARD 栈保护字

`s_stack_guard` 指向内核任务栈底的一个保护字，值为 `0xDEADBEEF`（32 位）或 `0xBEEF`（16 位）。内核周期性检查此保护字是否被篡改，若被修改则说明内核任务栈溢出，触发 panic。这是内核任务（运行在内核态）的栈安全机制。

#### 2.5.5 Grant 表与状态表

`s_grant_table` 和 `s_state_table` 是 Minix3 安全模型的扩展机制：

- **Grant 表**：存储进程授予其他进程访问其内存区域的凭证（grant），由 RS 初始化
- **状态表**：存储进程的状态信息，用于服务进程重启时恢复状态

两者都存储在进程的地址空间中，内核通过 `data_copy()` 跨地址空间访问。

#### 2.5.6 IPC 过滤器链

`s_ipcf` 指向一个 IPC 过滤器链表，每个节点可以是白名单或黑名单。过滤器按 `m_source` 和 `m_type` 匹配消息，支持精确控制进程能接收哪些消息。链表结构允许组合多个过滤规则，实现复杂的访问控制策略。

过滤器主要用于 VM 进程——在服务更新期间，VM 需要限制接收的消息类型，避免在地址空间不稳定时处理复杂的请求。
