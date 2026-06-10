# 10-async-ipc: 异步 IPC

> **分类**: Kernel IPC
> **源码**: `minix3/minix/kernel/proc.c`: mini_senda(1331), try_async(1348), cancel_async(1510), has_pending 系列
> **说明**: ASYNC 消息传递——发送方不阻塞、通过异步表传递通知的性能优化路径

---

## 1. 概述

### 1.1 概念定义/作用

**异步 IPC（Asynchronous IPC）** 是 Minix3 同步 IPC 的补充机制，允许发送方在不阻塞的情况下将消息投递给目标进程。与同步 IPC（SEND/RECEIVE/SENDREC）的"握手式"通信不同，异步发送方提交消息后立即返回，不等待接收方就绪。

异步 IPC 的核心数据结构是**异步消息表（asynmsg_t table）**——发送方在自身地址空间中维护一个消息数组，调用 `SENDA` 系统调用时将表地址和大小传递给内核。内核扫描表中的每个条目，尝试投递所有有效消息：

- 若目标进程正在接收且匹配，消息立即投递
- 若目标未就绪，消息标记为"待处理"（设置 `s_asyn_pending` 位图），下次目标接收时自动投递

异步 IPC 仅限**系统进程**使用（`s_flags & SYS_PROC`），用户进程不能使用。这是因为异步消息表存储在发送方地址空间中，内核需要跨地址空间访问它，而系统进程的地址空间映射在内核中始终可用。

### 1.2 与 Minix3 的对应关系

异步 IPC 的实现分布在以下源码中：

| 功能 | 函数 | 位置 |
|------|------|------|
| SENDA 系统调用入口 | `mini_senda()` | proc.c:1331 |
| 扫描表并尝试投递 | `try_deliver_senda()` | proc.c:1200 |
| 接收时尝试异步投递（ANY） | `try_async()` | proc.c:1348 |
| 接收时尝试异步投递（指定源） | `try_one()` | proc.c:1390 |
| 取消异步消息 | `cancel_async()` | proc.c:1510 |
| 检查待处理通知 | `has_pending_notify()` | proc.c:843 |
| 检查待处理异步消息 | `has_pending_asend()` | proc.c:852 |
| 通用待处理检查 | `has_pending()` | proc.c:773 |

异步消息表结构定义在 `minix3/minix/include/minix/ipc.h:2745-2751`，标志位定义在 `ipc.h:2754-2762`。

### 1.3 关键状态/机制说明

**异步消息表（asynmsg_t table）**：发送方在自身地址空间中维护的 `asynmsg_t` 数组，每个条目包含：

- `flags`：消息状态标志（AMF_VALID/AMF_DONE/AMF_NOTIFY 等）
- `dst`：目标进程 endpoint
- `result`：内核处理结果
- `msg`：消息体

发送方填充 `flags`（含 `AMF_VALID`）、`dst`、`msg` 后调用 SENDA。内核处理完成后设置 `AMF_DONE` 并写入 `result`。发送方通过轮询 `AMF_DONE` 位判断消息是否已处理。

**待处理位图（s_asyn_pending）**：当目标进程未就绪时，内核在目标的 `s_asyn_pending` 位图中设置发送方特权 ID 对应的位。目标进程下次调用 RECEIVE 时，`mini_receive()` 通过 `has_pending_asend()` 检查位图，发现待处理消息后调用 `try_async()` 或 `try_one()` 重新扫描发送方的异步消息表完成投递。

**ASYNCM 伪进程**：当异步消息需要通知发送方处理结果时（`AMF_NOTIFY` 或 `AMF_NOTIFY_ERR`），内核通过 `mini_notify(ASYNCM, ...)` 发送通知。ASYNCM（endpoint=-5）是一个专用的伪进程，仅用于异步消息完成通知。

### 1.4 行为规则

1. **仅系统进程可用**：`mini_senda()` 首先检查 `s_flags & SYS_PROC`，非系统进程返回 `EPERM`
2. **发送方不阻塞**：SENDA 系统调用始终立即返回 OK，消息投递是尽力而为的
3. **表大小限制**：异步消息表大小不能超过 `16 * (NR_TASKS + NR_PROCS)` = 4160 个条目
4. **AMF_DONE 语义**：内核处理完一个条目后设置 `AMF_DONE` 并写入 `result`，发送方据此判断消息是否已投递
5. **AMF_NOTIFY 通知**：若条目设置了 `AMF_NOTIFY`，内核在设置 `AMF_DONE` 后通过 ASYNCM 发送通知；若设置了 `AMF_NOTIFY_ERR`，仅在投递失败时通知
6. **AMF_NOREPLY 语义**：设置了 `AMF_NOREPLY` 的消息不会满足 SENDREC 的接收部分，避免异步消息意外匹配 RPC 回复
7. **不允许发送给内核任务**：异步消息的目标不能是内核任务（`iskerneln()` 检查），返回 `ECALLDENIED`
8. **VM 干预时跳过**：SMP 下若发送方地址空间正在被 VM 修改（`RTS_VMINHIBIT`），跳过该发送方的异步消息，设置 `MF_SENDA_VM_MISS` 标志
9. **权限检查**：`may_asynsend_to()` 检查发送权限，比同步 IPC 的 `may_send_to()` 更宽松——允许发送给自己

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 异步消息标志位

定义于 `minix3/minix/include/minix/ipc.h:2754-2762`：

| 标志 | 值 | 含义 |
|------|-----|------|
| `AMF_EMPTY` | 000 | 槽位未使用 |
| `AMF_VALID` | 001 | 槽位包含有效消息 |
| `AMF_DONE` | 002 | 内核已处理此消息，结果在 `result` 字段 |
| `AMF_NOTIFY` | 004 | 内核处理完成后发送通知 |
| `AMF_NOREPLY` | 010 | 此消息不是 SENDREC 的回复，不应匹配 SENDREC 的接收部分 |
| `AMF_NOTIFY_ERR` | 020 | 仅在投递失败时发送通知 |

#### 2.1.2 异步消息表大小限制

| 常量 | 计算 | 含义 |
|------|------|------|
| 最大表大小 | `16 * (NR_TASKS + NR_PROCS)` = 4160 | 防止恶意或错误的超大表消耗内核时间 |

#### 2.1.3 ASYNCM 伪进程

| 常量 | 值 | 含义 |
|------|-----|------|
| `ASYNCM` | -5 | 异步消息通知伪进程的 endpoint |

#### 2.1.4 相关 MISC_FLAGS

| 标志 | 值 | 异步 IPC 含义 |
|------|-----|-------------|
| `MF_SENDA_VM_MISS` | 0x20000 | 异步发送因 VM 修改地址空间而失败（SMP） |

### 2.2 核心数据结构

#### 2.2.1 asynmsg_t（异步消息条目）

定义于 `minix3/minix/include/minix/ipc.h:2745-2751`：

| 字段 | 类型 | 含义 |
|------|------|------|
| `flags` | `unsigned` | 消息状态标志（AMF_VALID/AMF_DONE 等） |
| `dst` | `endpoint_t` | 目标进程 endpoint |
| `result` | `int` | 内核处理结果（OK/EDEADSRCDST/ECALLDENIED 等） |
| `msg` | `message` | 消息体（56 字节） |

**生命周期**：
1. 发送方设置 `flags = AMF_VALID`，填写 `dst` 和 `msg`
2. 调用 SENDA 后内核扫描表
3. 内核处理完成后设置 `flags |= AMF_DONE`，写入 `result`
4. 发送方检查 `AMF_DONE` 后读取 `result`，清空 `flags` 复用槽位

#### 2.2.2 priv 结构中的异步 IPC 字段

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_asyntab` | `vir_bytes` | 异步消息表地址（发送方地址空间内），-1 表示无活跃表 |
| `s_asynsize` | `size_t` | 异步消息表元素数，0 表示无活跃表 |
| `s_asynendpoint` | `endpoint_t` | 异步表所属的 endpoint（用于验证表所有者） |
| `s_asyn_pending` | `sys_map_t` | 待处理异步消息位图（接收方侧） |

#### 2.2.3 待处理位图

`s_asyn_pending` 是一个 `sys_map_t` 位图，每一位对应一个特权结构 ID（`sys_id_t`）。当发送方有消息无法立即投递时，内核在接收方的 `s_asyn_pending` 中设置发送方特权 ID 对应的位。接收方调用 RECEIVE 时，`has_pending_asend()` 扫描此位图找到有待处理消息的发送方。

### 2.3 关键函数分析

#### 2.3.1 mini_senda()——SENDA 系统调用入口

`minix3/minix/kernel/proc.c:1331-1342`

```c
static int mini_senda(struct proc *caller_ptr, asynmsg_t *table, size_t size)
```

**功能**：SENDA 系统调用的入口函数。

**行为**：
1. 检查调用者是否为系统进程（`s_flags & SYS_PROC`），非系统进程返回 `EPERM`
2. 调用 `try_deliver_senda()` 尝试投递表中的所有消息

**注意**：`mini_senda()` 本身几乎不做工作，所有逻辑都在 `try_deliver_senda()` 中。

#### 2.3.2 try_deliver_senda()——扫描表并尝试投递

`minix3/minix/kernel/proc.c:1200-1326`

```c
int try_deliver_senda(struct proc *caller_ptr, asynmsg_t *table, size_t size)
```

**功能**：扫描发送方的异步消息表，尝试投递每个有效消息。

**行为**：
1. 清除发送方的活跃表信息：`s_asyntab = -1`, `s_asynsize = 0`，设置 `s_asynendpoint`
2. 若 `size == 0`，直接返回 OK
3. 若 `size > 16*(NR_TASKS+NR_PROCS)`，返回 `EDOM`
4. 遍历表中每个条目（`for i = 0; i < size; i++`）：
   a. `A_RETR(i)`：从发送方地址空间复制条目到内核栈上的 `tabent`
   b. 跳过空条目（`flags == 0`）和已完成条目（`flags & AMF_DONE`）
   c. 验证 flags 合法性（仅允许 AMF_VALID/DONE/NOTIFY/NOREPLY/NOTIFY_ERR）
   d. 验证目标 endpoint（`isokendpt()`）、非内核任务（`!iskerneln()`）、发送权限（`may_asynsend_to()`）
   e. 若目标正在接收且匹配（`WILLRECEIVE`），且非 AMF_NOREPLY 匹配 SENDREC：
      - 消息复制到 `dst.p_delivermsg`
      - 设置 `MF_DELIVERMSG`
      - `RTS_UNSET(RECEIVING)` 解除接收方阻塞
   f. 若目标未就绪：
      - 在 `s_asyn_pending` 中设置发送方特权 ID 对应的位
      - `done = FALSE`（表示还有未投递的消息）
      - 继续处理下一个条目
   g. `A_INSRT(i)`：将处理结果复制回发送方地址空间
5. 若有条目设置了 `AMF_NOTIFY`，通过 `mini_notify(ASYNCM, ...)` 通知发送方
6. 若 `!done`（仍有未投递消息），恢复活跃表信息：`s_asyntab = table`, `s_asynsize = size`

**A_RETR / A_INSRT 宏**：这两个宏封装了跨地址空间的数据复制操作。`A_RETR` 从发送方地址空间复制条目到内核，`A_INSRT` 将处理结果复制回发送方。两者都使用 `data_copy()` 系统调用完成跨地址空间复制。

#### 2.3.3 try_async()——接收时尝试异步投递（ANY 源）

`minix3/minix/kernel/proc.c:1348-1384`

```c
static int try_async(struct proc *caller_ptr)
```

**功能**：当接收方使用 ANY 作为源时，扫描所有有待处理异步消息的发送方，尝试投递第一个匹配的消息。

**行为**：
1. 获取接收方的 `s_asyn_pending` 位图
2. 遍历所有特权结构（`BEG_PRIV_ADDR` ~ `END_PRIV_ADDR`）：
   a. 跳过空闲特权结构（`s_proc_nr == NONE`）
   b. 检查该特权 ID 在位图中是否置位（`get_sys_bit`）
   c. SMP 下：若发送方被 VM 干预（`RTS_VMINHIBIT`），设置 `MF_SENDA_VM_MISS` 并跳过
   d. 调用 `try_one(ANY, src_ptr, caller_ptr)` 尝试投递
   e. 若成功（返回 OK），立即返回
3. 若所有发送方都无匹配消息，返回 `ESRCH`

**调用时机**：`mini_receive()` 中检查 `has_pending_asend()` 后调用。

#### 2.3.4 try_one()——接收时尝试异步投递（指定源）

`minix3/minix/kernel/proc.c:1390-1505`

```c
static int try_one(endpoint_t receive_e, struct proc *src_ptr,
    struct proc *dst_ptr)
```

**功能**：尝试从指定发送方投递一个异步消息给接收方。

**行为**：
1. 获取发送方的特权结构和异步消息表信息
2. 清除接收方 `s_asyn_pending` 中发送方对应的位
3. 若表为空（`size == 0`）或 endpoint 不匹配，返回 `EAGAIN`
4. 检查发送权限（`may_asynsend_to()`）
5. 遍历发送方的异步消息表：
   a. `A_RETR(i)`：从发送方地址空间复制条目
   b. 跳过空条目和已完成条目
   c. 验证 flags 合法性
   d. 检查目标 endpoint 是否匹配接收方
   e. 检查 `CANRECEIVE` 条件（endpoint 匹配 + IPC 过滤器）
   f. 若 `AMF_NOREPLY` 且接收方有 `MF_REPLY_PEND`，跳过（不匹配 SENDREC 的接收部分）
   g. 若所有条件满足：
      - 消息复制到 `dst.p_delivermsg`
      - 设置 `MF_DELIVERMSG`
      - 标记条目为 `AMF_DONE`，`result = OK`
      - `A_INSRT(i)`：将结果复制回发送方
      - 跳出循环
6. 若有条目需要通知（`AMF_NOTIFY` 或 `AMF_NOTIFY_ERR`），通过 ASYNCM 发送通知
7. 若所有条目都已处理完毕（`done == TRUE`），清除发送方的活跃表信息
8. 否则重新设置 `s_asyn_pending` 位（仍有未投递消息）

**与 try_deliver_senda 的区别**：`try_deliver_senda` 是发送方主动调用（SENDA 系统调用），`try_one` 是接收方被动触发（RECEIVE 时检查待处理消息）。两者都扫描同一个异步消息表，但触发方向不同。

#### 2.3.5 cancel_async()——取消异步消息

`minix3/minix/kernel/proc.c:1510-1590`

```c
int cancel_async(struct proc *src_ptr, struct proc *dst_ptr)
```

**功能**：取消从 `src_ptr` 到 `dst_ptr` 的所有异步消息，通常在目标进程重启时调用。

**行为**：
1. 清除发送方的活跃表信息和接收方的待处理位
2. 遍历发送方的异步消息表：
   a. 跳过空条目和已完成条目
   b. 若目标 endpoint 不匹配接收方，标记 `done = FALSE`（还有发给其他目标的消息）
   c. 若目标匹配，设置 `result = EDEADSRCDST`，`flags |= AMF_DONE`
   d. `A_INSRT(i)`：将取消结果复制回发送方
3. 若有通知需求，通过 ASYNCM 发送通知
4. 若仍有发给其他目标的消息，恢复活跃表信息

**调用时机**：系统进程重启时（`do_clear()` / `do_restart()`），取消所有发往该进程的异步消息。

#### 2.3.6 has_pending() / has_pending_notify() / has_pending_asend()

`minix3/minix/kernel/proc.c:773-856`

```c
static int has_pending(sys_map_t *map, int src_p, int asynm)
int has_pending_notify(struct proc *caller, int src_p)
int has_pending_asend(struct proc *caller, int src_p)
```

**功能**：检查是否有来自指定源的待处理消息。

**行为**：
- 若 `src_p != ANY`：检查位图中指定源对应的位是否置位，返回其特权 ID
- 若 `src_p == ANY`：扫描位图找到第一个置位的位，返回其特权 ID
- SMP 下：若 `asynm` 为真且发送方被 VM 干预（`RTS_VMINHIBIT`），设置 `MF_SENDA_VM_MISS` 并跳过
- 无待处理消息时返回 `NULL_PRIV_ID`

`has_pending_notify` 和 `has_pending_asend` 是 `has_pending` 的包装，分别检查 `s_notify_pending` 和 `s_asyn_pending` 位图。

### 2.4 调用关系/调用点分析

#### 2.4.1 异步 IPC 发送路径

```
用户空间系统进程调用 ipc_senda(table, size)
  └─ SENDA 系统调用
       └─ do_ipc()
            └─ mini_senda(caller, table, size)
                 ├─ [!SYS_PROC?] → EPERM
                 └─ try_deliver_senda(caller, table, size)
                      ├─ 遍历表中每个条目
                      │    ├─ A_RETR(i)           // 从用户空间复制条目
                      │    ├─ 验证 flags/endpoint/权限
                      │    ├─ [WILLRECEIVE?]
                      │    │    ├─ YES: 投递消息 → MF_DELIVERMSG
                      │    │    │       RTS_UNSET(RECEIVING)
                      │    │    └─ NO:  s_asyn_pending 置位
                      │    └─ A_INSRT(i)           // 结果复制回用户空间
                      └─ [AMF_NOTIFY?] → mini_notify(ASYNCM, ...)
```

#### 2.4.2 异步 IPC 接收路径

```
mini_receive(caller, src_e, msg, flags)
  ├─ has_pending_notify()   → 通知优先
  ├─ has_pending_asend()    → 异步消息次之
  │    ├─ [src_e == ANY]
  │    │    └─ try_async(caller)
  │    │         └─ 遍历 s_asyn_pending 位图
  │    │              └─ try_one(ANY, src, caller)
  │    │                   └─ 扫描发送方异步消息表
  │    └─ [src_e != ANY]
  │         └─ try_one(src_e, src, caller)
  │              └─ 扫描指定发送方的异步消息表
  └─ p_caller_q             → 同步发送者最后
```

#### 2.4.3 异步 IPC 取消路径

```
系统进程重启 (do_restart / do_clear)
  └─ cancel_async(src, dst)
       ├─ 清除 s_asyntab / s_asynsize / s_asyn_pending
       ├─ 遍历发送方异步消息表
       │    ├─ 匹配目标: result = EDEADSRCDST, AMF_DONE
       │    └─ 不匹配: done = FALSE
       └─ [AMF_NOTIFY?] → mini_notify(ASYNCM, ...)
```

### 2.5 设计要点/特殊处理

#### 2.5.1 异步消息表在用户空间

异步消息表存储在发送方的地址空间中，而非内核空间。这是一个关键的设计选择：

**优势**：
1. **零内核内存**：内核不需要为异步消息分配缓冲区，消息表完全由发送方管理
2. **灵活大小**：发送方可以根据需要调整表大小，不受内核缓冲区限制
3. **自然生命周期**：发送方退出时表自动消失，无需内核清理

**代价**：
1. **跨地址空间复制**：每次扫描表都需要 `data_copy()` 从发送方地址空间复制条目到内核，开销较大
2. **仅限系统进程**：用户进程的地址空间映射可能不完整，内核无法安全访问
3. **竞态风险**：发送方可能在内核扫描表时修改表内容，内核必须在每次操作时重新验证

#### 2.5.2 AMF_NOREPLY 与 SENDREC 的交互

异步消息可能意外匹配 SENDREC 的接收部分。例如，进程 A 向进程 B 发送 SENDREC（等待回复），但进程 C 通过异步 IPC 发给 B 的消息先到达，B 的 RECEIVE 匹配了 C 的异步消息而非 A 的回复。

`AMF_NOREPLY` 标志解决此问题：设置了 `AMF_NOREPLY` 的异步消息不会匹配有 `MF_REPLY_PEND` 标志的接收方（即 SENDREC 的接收阶段）。这确保了 SENDREC 的回复不会被异步消息"截胡"。

#### 2.5.3 ASYNCM 伪进程的通知机制

异步消息完成通知使用 ASYNCM（endpoint=-5）作为发送方，而非原始发送方。原因：

1. **避免递归**：若通知使用原始发送方作为源，可能触发发送方的新一轮异步消息处理
2. **统一入口**：所有异步完成通知都来自 ASYNCM，接收方可以统一处理
3. **轻量级**：通知是 `mini_notify()`，不携带数据，仅通知"有异步消息处理完毕"

发送方收到 ASYNCM 的通知后，扫描自己的异步消息表，检查哪些条目的 `AMF_DONE` 被设置。

#### 2.5.4 SMP 下的 VM 干预处理

SMP 配置下，一个 CPU 可能正在执行 `try_async()` 扫描发送方的异步消息表，而另一个 CPU 上的 VM 正在修改发送方的地址空间（`RTS_VMINHIBIT`）。此时访问发送方地址空间是危险的——页表可能不一致。

处理方式：
1. `has_pending()` 和 `try_async()` 中检查 `RTS_VMINHIBIT`
2. 若发送方被 VM 干预，设置 `MF_SENDA_VM_MISS` 标志并跳过
3. VM 完成地址空间修改后，`MF_SENDA_VM_MISS` 会导致重新尝试异步投递

#### 2.5.5 may_asynsend_to 与 may_send_to 的差异

```c
#define may_asynsend_to(rp, nr) (may_send_to(rp, nr) || (rp)->p_nr == nr)
```

`may_asynsend_to` 比 `may_send_to` 多了一个条件：允许发送给自己（`p_nr == nr`）。这是因为系统进程经常需要向自身发送异步消息（如定时器回调），而同步 IPC 的 `may_send_to` 不允许这种操作。

#### 2.5.6 done 标志与活跃表管理

`try_deliver_senda()` 和 `try_one()` 都使用 `done` 标志跟踪表的处理状态：

- `done = TRUE`：所有非空条目都已处理完毕（`AMF_DONE` 或空），可以清除活跃表信息
- `done = FALSE`：仍有未投递的消息，需要保留活跃表信息供下次扫描

当 `done = FALSE` 时，`s_asyntab` 和 `s_asynsize` 被恢复，`s_asyn_pending` 位被重新设置。这确保了下次接收方调用 RECEIVE 时能重新尝试投递。

#### 2.5.7 A_RETR / A_INSRT 宏的错误处理

`A_RETR` 宏在 `data_copy()` 失败时设置 `r = EFAULT` 并跳转到 `asyn_error`，立即终止表扫描。这是因为无法读取发送方的消息表意味着后续条目也无法处理。

`A_INSRT` 宏在 `data_copy()` 失败时仅打印警告（`ASCOMPLAIN`），不设置 `r` 也不跳转。这是因为写入结果失败不应阻止后续消息的投递——发送方可能暂时无法接收结果，但其他消息仍可正常处理。
