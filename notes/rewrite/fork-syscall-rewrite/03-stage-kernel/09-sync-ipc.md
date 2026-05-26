# 09-sync-ipc: 同步 IPC（SEND / RECEIVE / BOTH / NOTIFY）

> **分类**: Kernel IPC
> **源码**: `minix3/minix/kernel/proc.c: do_ipc()`(L599-773, ~175行), `proc.c` 相关辅助函数
> **说明**: Minix3 微内核通信的四个原语——进程间消息传递的核心协议

---

## 1. 概述

### 1.1 概念定义/作用

**同步 IPC（Inter-Process Communication）** 是 Minix3 微内核的核心通信机制。在微内核架构中，操作系统服务（PM、VFS、VM 等）运行在用户态，它们之间以及与用户进程之间的所有交互都通过 IPC 消息传递完成。Minix3 提供四个同步 IPC 原语：

1. **SEND**：阻塞式发送。调用者将消息发送给目标进程，若目标未准备好接收则阻塞等待
2. **RECEIVE**：阻塞式接收。调用者等待来自指定源（或 ANY）的消息，若无消息则阻塞等待
3. **SENDREC**：组合原语。先 SEND 再 RECEIVE，等价于 RPC（远程过程调用）——发送请求后等待回复
4. **NOTIFY**：非阻塞通知。发送一个轻量级通知消息，若目标未准备好接收则标记为待处理，发送方永不阻塞

此外还有两个辅助原语：
- **SENDNB**：非阻塞式发送。若目标未准备好接收，立即返回 `ENOTREADY` 而非阻塞
- **SENDA**：异步批量发送（在 [10-async-ipc.md](10-async-ipc.md) 中详述）

Minix3 的 IPC 是**同步的**（SEND/RECEIVE/SENDREC 会阻塞），消息传递的语义是"握手式"的——发送方和接收方必须同时就绪才能完成消息传递。这与异步 IPC（发送方不等待接收方）形成对比。

### 1.2 与 Minix3 的对应关系

同步 IPC 的实现分布在以下源码中：

| 功能 | 函数 | 位置 |
|------|------|------|
| IPC 系统调用入口 | `do_ipc()` | proc.c:599 |
| 同步 IPC 分发 | `do_sync_ipc()` | proc.c:479 |
| 阻塞式发送 | `mini_send()` | proc.c:870 |
| 阻塞式接收 | `mini_receive()` | proc.c:967 |
| 通知发送 | `mini_notify()` | proc.c:1122 |
| 死锁检测 | `deadlock()` | proc.c:703 |
| 待处理消息检查 | `has_pending()` | proc.c:773 |
| 待处理通知检查 | `has_pending_notify()` | proc.c:843 |
| 待处理异步检查 | `has_pending_asend()` | proc.c:852 |
| 消息投递 | `delivermsg()` | proc.c:263 |

IPC 常量定义在 `minix3/minix/include/minix/ipcconst.h` 中，IPC 权限检查宏在 `minix3/minix/kernel/ipc.h` 中。

### 1.3 关键状态/机制说明

**消息传递模型**：Minix3 的 IPC 是**直接消息传递**——消息从发送方地址空间直接复制到接收方地址空间，内核作为可信中介执行复制。消息大小固定为 56 字节（`sizeof(message)`）。

**阻塞语义**：
- SEND：若目标未在接收（`!WILLRECEIVE`），发送方阻塞，加入目标的发送等待队列 `p_caller_q`
- RECEIVE：若无消息可用（无待处理通知、无待处理异步消息、无发送等待者），接收方阻塞，设置 `RTS_RECEIVING`
- SENDREC：先执行 SEND，成功后自动执行 RECEIVE，整体是原子的（从调用者角度）
- NOTIFY：永不阻塞，若目标未在接收则设置待处理位

**消息投递机制**：消息不是在 IPC 调用时直接写入接收方地址空间，而是先存入 `p_delivermsg`，设置 `MF_DELIVERMSG` 标志，由 `switch_to_user()` 中的 `delivermsg()` 在进程恢复执行前完成实际复制。这种延迟投递设计简化了内核代码路径。

**权限控制**：每个进程的 `s_trap_mask` 控制允许使用的 IPC 原语，`s_ipc_to` 位图控制允许发送的目标进程。内核任务只能通过 SENDREC 通信（因为任务总是回复且不能阻塞在发送上）。

### 1.4 行为规则

1. **SEND 阻塞规则**：发送方在目标未准备好接收时阻塞，加入目标的 `p_caller_q` 队列尾部
2. **RECEIVE 优先级**：接收时按 通知 → 异步消息 → 同步发送者 的顺序检查消息来源
3. **SENDREC 原子性**：SENDREC 的 SEND 和 RECEIVE 作为一个整体执行，通知不能在 SEND 和 RECEIVE 之间插入（`MF_REPLY_PEND` 标志保证）
4. **NOTIFY 不丢失**：若目标未在接收，通知被标记在 `s_notify_pending` 位图中，下次接收时投递
5. **死锁检测**：SEND 和 RECEIVE 阻塞前都检测死锁，发现循环依赖返回 `ELOCKED`
6. **SENDNB 不阻塞**：目标未准备好接收时立即返回 `ENOTREADY`
7. **ANY 只用于 RECEIVE**：只有 RECEIVE 可以使用 `ANY` 作为源，其他原语必须指定有效 endpoint
8. **内核任务限制**：向内核任务发送只能用 SENDREC（因为任务总是回复）

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 IPC 调用号

定义于 `minix3/minix/include/minix/ipcconst.h:7-14`：

| 常量 | 值 | 含义 |
|------|-----|------|
| `SEND` | 1 | 阻塞式发送 |
| `RECEIVE` | 2 | 阻塞式接收 |
| `SENDREC` | 3 | 发送+接收（RPC） |
| `NOTIFY` | 4 | 非阻塞通知 |
| `SENDNB` | 5 | 非阻塞发送 |
| `MINIX_KERNINFO` | 6 | 请求内核信息结构 |
| `SENDA` | 16 | 异步批量发送 |
| `IPCNO_HIGHEST` | 16 (= SENDA) | 最大 IPC 调用号 |

#### 2.1.2 IPC 标志

定义于 `minix3/minix/kernel/ipc.h:11-12`：

| 常量 | 值 | 含义 |
|------|-----|------|
| `NON_BLOCKING` | 0x0080 | 非阻塞模式（SENDNB 使用） |
| `FROM_KERNEL` | 0x0100 | 消息来自内核（代表进程发送） |

#### 2.1.3 IPC 状态码宏

定义于 `minix3/minix/include/minix/ipcconst.h:20-35`：

| 宏 | 含义 |
|-----|------|
| `IPC_STATUS_CALL(status)` | 从状态码提取 IPC 调用类型 |
| `IPC_STATUS_CALL_TO(call)` | 将调用类型编码到状态码 |
| `IPC_FLG_MSG_FROM_KERNEL` | 标记消息来自内核 |
| `IPC_STATUS_FLAGS(flgs)` | 将标志编码到状态码高位 |
| `IPC_STATUS_FLAGS_TEST(status, flgs)` | 从状态码测试标志 |

#### 2.1.4 IPC 权限检查宏

定义于 `minix3/minix/kernel/ipc.h:14-22`：

| 宏 | 含义 |
|-----|------|
| `WILLRECEIVE(src_e, dst_ptr, m_src_v, m_src_p)` | 目标进程是否愿意接收来自 src_e 的消息 |
| `CANRECEIVE(receive_e, src_e, dst_ptr, m_src_v, m_src_p)` | 接收条件是否满足（endpoint 匹配 + IPC 过滤器） |

`WILLRECEIVE` 条件：目标正在接收（`RTS_RECEIVING` 置位且 `RTS_SENDING` 未置位）且 `CANRECEIVE` 为真。

`CANRECEIVE` 条件：接收方指定的源匹配（`receive_e == ANY || receive_e == src_e`）且 IPC 过滤器允许（若设置了过滤器）。

#### 2.1.5 IPC 错误码

| 错误码 | 含义 |
|--------|------|
| `EDEADSRCDST` | 无效的源/目标 endpoint |
| `ECALLDENIED` | IPC 权限被拒绝（s_ipc_to 位图不允许） |
| `ETRAPDENIED` | IPC 陷阱被拒绝（s_trap_mask 不允许） |
| `ELOCKED` | 死锁检测发现循环依赖 |
| `ENOTREADY` | 非阻塞操作目标未就绪 |
| `EBADCALL` | 非法 IPC 调用号 |
| `EFAULT` | 消息复制失败（用户空间地址无效） |

### 2.2 核心数据结构

#### 2.2.1 消息（message）

Minix3 的消息是固定大小的结构体（56 字节），定义在 `minix3/minix/include/minix/ipc.h` 中。消息包含 `m_source`（发送方 endpoint）和 `m_type`（消息类型），其余字段为联合体，根据消息类型不同而解释不同。

#### 2.2.2 IPC 相关的 proc 字段

| 字段 | 类型 | IPC 含义 |
|------|------|---------|
| `p_sendmsg` | `message` | 发送方暂存的消息（SENDING 时有效） |
| `p_delivermsg` | `message` | 待投递给此进程的消息（MF_DELIVERMSG 时有效） |
| `p_delivermsg_vir` | `vir_bytes` | 消息投递目标用户空间地址 |
| `p_caller_q` | `struct proc *` | 向此进程发送消息的等待队列头 |
| `p_q_link` | `struct proc *` | 发送等待队列中的链接指针 |
| `p_getfrom_e` | `endpoint_t` | 想从谁接收（RECEIVING 时有效） |
| `p_sendto_e` | `endpoint_t` | 想向谁发送（SENDING 时有效） |

#### 2.2.3 IPC 相关的 priv 字段

| 字段 | 类型 | IPC 含义 |
|------|------|---------|
| `s_trap_mask` | `short` | 允许的 IPC 陷阱掩码 |
| `s_ipc_to` | `sys_map_t` | 允许的 IPC 目标进程位图 |
| `s_notify_pending` | `sys_map_t` | 待处理通知位图 |
| `s_asyn_pending` | `sys_map_t` | 待处理异步消息位图 |
| `s_ipcf` | `ipc_filter_t *` | IPC 过滤器（NULL 表示无过滤） |

### 2.3 关键函数分析

#### 2.3.1 do_ipc()——IPC 系统调用入口

`minix3/minix/kernel/proc.c:599-698`

```c
int do_ipc(reg_t r1, reg_t r2, reg_t r3)
```

**功能**：IPC 系统调用的内核入口，由体系结构相关的系统调用处理代码调用。

**行为**：
1. 获取当前进程指针 `caller_ptr`
2. 处理系统调用追踪（`MF_SC_TRACE` / `MF_SC_DEFER`）——若进程被追踪，延迟 IPC 调用
3. 检查 `MF_DELIVERMSG`（不应在 IPC 入口时设置）
4. 根据 `call_nr` 分发：
   - `SEND/RECEIVE/SENDREC/NOTIFY/SENDNB`：调用 `do_sync_ipc()`
   - `SENDA`：调用 `mini_senda()`（异步 IPC）
   - `MINIX_KERNINFO`：返回内核信息结构地址
   - 其他：返回 `EBADCALL`
5. 更新 IPC 统计 `p_accounting.ipc_sync++`

#### 2.3.2 do_sync_ipc()——同步 IPC 分发

`minix3/minix/kernel/proc.c:479-597`

```c
static int do_sync_ipc(struct proc *caller_ptr, int call_nr,
    endpoint_t src_dst_e, message *m_ptr)
```

**功能**：验证参数并分发同步 IPC 调用。

**行为**：
1. **调用号验证**：`call_nr` 必须在 [0, 32) 范围内且是已知的 IPC 调用
2. **Endpoint 验证**：
   - RECEIVE 允许 `ANY` 作为源
   - 其他调用必须指定有效 endpoint（`isokendpt()` 验证）
3. **发送权限验证**：非 RECEIVE 调用检查 `may_send_to()` 权限
4. **陷阱掩码验证**：检查 `s_trap_mask` 是否允许此 IPC 调用
5. **内核任务限制**：非 SENDREC/RECEIVE 调用不允许以内核任务为目标
6. **分发执行**：
   - `SENDREC`：设置 `MF_REPLY_PEND`，先 `mini_send()`，成功后 `mini_receive()`
   - `SEND`：调用 `mini_send()`
   - `RECEIVE`：清除 `MF_REPLY_PEND`，调用 `mini_receive()`
   - `NOTIFY`：调用 `mini_notify()`
   - `SENDNB`：调用 `mini_send()` 带 `NON_BLOCKING` 标志

#### 2.3.3 mini_send()——阻塞式发送

`minix3/minix/kernel/proc.c:870-962`

```c
int mini_send(register struct proc *caller_ptr, endpoint_t dst_e,
    message *m_ptr, const int flags)
```

**功能**：将消息从调用者发送给目标进程。

**行为**（两条路径）：

**路径 A——目标正在等待接收**（`WILLRECEIVE` 为真）：
1. 将消息复制到 `dst_ptr->p_delivermsg`
2. 设置 `m_source` 为发送方 endpoint
3. 设置 `MF_DELIVERMSG` 标志
4. 清除 `MF_REPLY_PEND`（若是 SENDREC 的回复）
5. `RTS_UNSET(dst_ptr, RTS_RECEIVING)` 解除接收方阻塞

**路径 B——目标未在接收**：
1. 若 `NON_BLOCKING` 标志，返回 `ENOTREADY`
2. 调用 `deadlock()` 检测死锁
3. 将消息复制到 `caller_ptr->p_sendmsg`
4. `RTS_SET(caller_ptr, RTS_SENDING)` 阻塞发送方
5. 设置 `p_sendto_e = dst_e`
6. 将发送方加入目标的 `p_caller_q` 队列尾部

**FROM_KERNEL 标志**：内核代表进程发送消息时设置，消息直接赋值（无需从用户空间复制），并标记 `MF_SENDING_FROM_KERNEL`。

#### 2.3.4 mini_receive()——阻塞式接收

`minix3/minix/kernel/proc.c:967-1117`

```c
static int mini_receive(struct proc *caller_ptr, endpoint_t src_e,
    message *m_buff_usr, const int flags)
```

**功能**：接收来自指定源的消息。

**行为**（按优先级检查消息来源）：

1. **记录目标地址**：`p_delivermsg_vir = m_buff_usr`
2. **若调用者已在 SENDING**（SENDREC 的发送阶段阻塞）：跳过所有检查，直接阻塞在接收
3. **检查待处理通知**（`has_pending_notify`）：
   - 若找到匹配的通知源，构造通知消息，设置 `MF_DELIVERMSG`，跳到 `receive_done`
   - SENDREC 期间（`MF_REPLY_PEND`）跳过通知检查
4. **检查待处理异步消息**（`has_pending_asend`）：
   - 若指定源：调用 `try_one()` 尝试投递
   - 若 ANY：调用 `try_async()` 尝试投递
5. **检查同步发送等待队列**（`p_caller_q`）：
   - 遍历等待队列，找到第一个匹配源的发送者
   - 复制消息到 `p_delivermsg`，解除发送者阻塞
6. **无消息可用**：
   - 若 `NON_BLOCKING`：返回 `ENOTREADY`
   - 否则：`deadlock()` 检测 → 设置 `p_getfrom_e` → `RTS_SET(RTS_RECEIVING)` 阻塞

**receive_done**：清除 `MF_REPLY_PEND`，返回 OK。

#### 2.3.5 mini_notify()——非阻塞通知

`minix3/minix/kernel/proc.c:1122-1167`

```c
int mini_notify(const struct proc *caller_ptr, endpoint_t dst_e)
```

**功能**：发送轻量级通知消息，永不阻塞。

**行为**（两条路径）：

**路径 A——目标正在等待接收**（`WILLRECEIVE` 且非 `MF_REPLY_PEND`）：
1. 构造通知消息（`BuildNotifyMessage`）
2. 设置 `MF_DELIVERMSG`
3. `RTS_UNSET(dst_ptr, RTS_RECEIVING)` 解除接收方阻塞

**路径 B——目标未在接收**：
1. 获取发送方的特权 ID `src_id = priv(caller_ptr)->s_id`
2. 在目标的 `s_notify_pending` 位图中设置对应位
3. 返回 OK（通知被标记为待处理，下次接收时投递）

**通知消息的特殊性**：通知消息不携带用户数据，仅包含发送方 endpoint 和系统状态信息（如待处理信号、定时器到期等）。消息体由 `BuildNotifyMessage` 宏构造。

#### 2.3.6 deadlock()——死锁检测

`minix3/minix/kernel/proc.c:703-768`

```c
static int deadlock(int function, register struct proc *cp, endpoint_t src_dst_e)
```

**功能**：检测 IPC 调用是否会导致死锁。

**行为**：
1. 从调用者开始，沿 `P_BLOCKEDON()` 链追踪阻塞依赖
2. 每追踪一个进程，`group_size` 递增
3. 若追踪到调用者自身（形成环），检测到潜在死锁
4. **两进程特例**：若环中只有两个进程，且一个是 SEND(REC) 另一个是 RECEIVE，则不是死锁——这是正常的请求-回复模式
5. 更大的环或其他组合则报告死锁，返回 `group_size`

**两进程特例的判断**：`(xp->p_rts_flags ^ (function << 2)) & RTS_SENDING`。这个表达式利用了 `RTS_SENDING = 0x04 = 1 << 2` 的特性，将 function（SEND=1 或 RECEIVE=2）左移 2 位后与目标进程的 RTS_SENDING 位做异或，若结果非零则表示一个是发送一个是接收，不是死锁。

### 2.4 调用关系/调用点分析

#### 2.4.1 同步 IPC 调用链

```
用户进程执行 sys_call(call_nr, src_dst, msg)
  └─ 体系结构相关入口（中断/陷阱）
       └─ do_ipc(r1, r2, r3)
            ├─ [SEND/RECEIVE/SENDREC/NOTIFY/SENDNB]
            │    └─ do_sync_ipc(caller, call_nr, src_dst, msg)
            │         ├─ 参数验证（endpoint/权限/陷阱掩码）
            │         ├─ SENDREC: mini_send() → mini_receive()
            │         ├─ SEND:    mini_send()
            │         ├─ RECEIVE: mini_receive()
            │         ├─ NOTIFY:  mini_notify()
            │         └─ SENDNB:  mini_send(NON_BLOCKING)
            └─ [SENDA]
                 └─ mini_senda()
```

#### 2.4.2 mini_send 消息投递路径

```
mini_send(caller, dst_e, msg, flags)
  ├─ [WILLRECEIVE?]
  │    ├─ YES: msg → dst.p_delivermsg
  │    │       MF_DELIVERMSG 置位
  │    │       RTS_UNSET(RECEIVING) 解除阻塞
  │    │       → switch_to_user() 中 delivermsg() 完成实际复制
  │    └─ NO:  [NON_BLOCKING?] → ENOTREADY
  │            deadlock() 检测
  │            msg → caller.p_sendmsg
  │            RTS_SET(SENDING) 阻塞
  │            caller → dst.p_caller_q 队列
```

#### 2.4.3 mini_receive 消息检查优先级

```
mini_receive(caller, src_e, msg, flags)
  ├─ 1. has_pending_notify()    → 通知消息（最高优先级）
  ├─ 2. has_pending_asend()     → 异步消息
  │    ├─ try_one()             → 指定源的异步消息
  │    └─ try_async()           → 任意源的异步消息
  ├─ 3. p_caller_q 遍历        → 同步发送者
  └─ 4. 无消息 → RTS_SET(RECEIVING) 阻塞
```

### 2.5 设计要点/特殊处理

#### 2.5.1 SENDREC 的原子性保证

SENDREC 组合了 SEND 和 RECEIVE，从调用者角度看是原子的——发送请求后必然等待回复，中间不会被其他消息插入。实现机制：

1. `MF_REPLY_PEND` 标志：SENDREC 开始时设置，RECEIVE 完成时清除
2. 通知检查时跳过有 `MF_REPLY_PEND` 的进程——确保通知不会在 SEND 和 RECEIVE 之间被投递
3. SEND 阶段若阻塞，RECEIVE 阶段自动执行（`do_sync_ipc` 的 fall-through 设计）

#### 2.5.2 延迟消息投递（MF_DELIVERMSG）

消息不是在 IPC 调用时直接写入接收方的用户空间地址，而是先存入 `p_delivermsg`，设置 `MF_DELIVERMSG`，由 `switch_to_user()` 中的 `delivermsg()` 在进程恢复执行前完成实际复制。

这种设计的原因：
1. **简化内核路径**：IPC 函数只需设置标志，无需处理用户空间地址映射可能触发的页缺失
2. **统一投递点**：所有消息（同步/异步/通知）都通过同一个 `delivermsg()` 投递
3. **页缺失处理**：若投递时触发页缺失，可以挂起操作请求 VM 处理，完成后恢复

#### 2.5.3 通知的位图机制

通知使用 `s_notify_pending` 位图而非消息队列，这是关键的性能优化：

1. **O(1) 发送**：设置位图中的一个位即可，无需分配内存或入队
2. **自动合并**：同一源的多次通知只保留一次（位图天然去重）
3. **O(N) 接收**：接收时扫描位图找到第一个待处理源

代价是通知不携带用户数据，仅包含系统状态信息。需要传递数据时必须使用 SEND/RECEIVE。

#### 2.5.4 发送等待队列的指针指针模式

`mini_send()` 和 `mini_receive()` 中操作 `p_caller_q` 队列时使用指针指针模式：

```c
xpp = &dst_ptr->p_caller_q;
while (*xpp) xpp = &(*xpp)->p_q_link;
*xpp = caller_ptr;
```

这与调度队列的 `dequeue()` 使用相同的模式，避免对队首/队尾的特殊处理。

#### 2.5.5 死锁检测的两进程特例

两进程之间的 SEND+RECEIVE 不是死锁，而是正常的请求-回复模式：

- 进程 A SENDREC 到进程 B：A 阻塞在 SEND（等待 B 接收）
- 进程 B RECEIVE from ANY：B 阻塞在 RECEIVE（等待消息）
- A 的 SEND 会被 B 的 RECEIVE 匹配，双方解除阻塞

`deadlock()` 函数通过检查阻塞链中两个进程的状态组合来区分这种情况。

#### 2.5.6 内核任务通信限制

内核任务（IDLE、CLOCK、SYSTEM 等）只能通过 SENDREC 与其他进程通信，不能单独使用 SEND 或 RECEIVE。原因：

1. 内核任务总是回复消息，单独 SEND 会导致任务无法回复
2. 内核任务不能阻塞在发送上——如果调用者只 SEND 不 RECEIVE，任务会永远阻塞
3. `do_sync_ipc()` 中显式检查：`call_nr != SENDREC && call_nr != RECEIVE && iskerneln(src_dst_p)` 时返回 `ETRAPDENIED`

#### 2.5.7 IPC 过滤器（s_ipcf）

`CANRECEIVE` 宏中检查 `s_ipcf`：若进程设置了 IPC 过滤器，则消息必须通过过滤器才能被接收。过滤器可以按 `m_source` 和 `m_type` 匹配，支持白名单和黑名单模式。这是 Minix3 安全模型的一部分，限制进程能接收的消息范围。
