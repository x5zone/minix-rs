# 12-ipc-core: IPC 核心机制

> **分类**: Kernel IPC 核心
> **源码**: `minix3/minix/kernel/proc.c:263-294, 479-597, 599-698, 703-768, 870-962, 967-1117, 1122-1167, 1200-1326, 1331-1346`
> **前置**: 06（struct proc 字段语义）、10（switch_to_user 调用 delivermsg）、11（RTS_SENDING/RECEIVING 状态机 + rts_set 联动）
> **说明**: 内核如何在无共享内存的前提下完成进程间消息中转、阻塞唤醒、死锁检测与通知投递

---

## 1. 概述

### 1.1 核心问题：无共享内存的进程间通信

Minix3 的微内核架构中，进程间通信是系统的基石。内核不提供"服务"，只提供"通信"——所有服务请求都通过 IPC 消息传递完成。用户进程请求 PM fork、请求 VM 映射内存、驱动向 VFS 注册设备，本质都是发送一条消息并等待回复。

内核的 IPC 职责有四项：

1. **消息中转**：将发送方的消息拷贝到接收方（不暴露共享内存）
2. **阻塞管理**：发送/接收不匹配时阻塞进程，匹配时唤醒
3. **死锁检测**：防止进程间形成循环等待（A 等 B，B 等 A）
4. **通知机制**：轻量级单边通知，永不阻塞、永不丢失

这四项职责共同保证了"消息即服务"模型：进程只需 send/receive 即可完成所有系统交互，无需关心底层同步与状态机。

### 1.2 六原语语义模型：按阻塞行为分类

Minix3 提供六个 IPC 原语（`ipcconst.h:7-13`），按"阻塞行为"分为三类：

| 类别 | 原语 | 阻塞条件 | 唤醒机制 | RTS 标志变化 |
|------|------|---------|---------|-------------|
| 同步阻塞 | SEND | 目标未在 RECEIVE | 目标调用 RECEIVE | RTS_SENDING set/clear |
| 同步阻塞 | RECEIVE | 无匹配消息可用 | 有消息到达 | RTS_RECEIVING set/clear |
| 原子同步 | SENDREC | SEND 或 RECEIVE 阻塞 | 两步都完成 | RTS_SENDING → RTS_RECEIVING |
| 异步非阻塞 | NOTIFY | **永不阻塞** | 位图记录，下次 RECEIVE 检查 | 无（写 `s_notify_pending`） |
| 非阻塞尝试 | SENDNB | 目标未就绪返回 ENOTREADY | 不等待 | 无 |
| 批量异步 | SENDA | 不阻塞（扫描表投递） | 失败的标记 pending | 无 |

**SENDREC 的原子性**是关键设计：它表示"请求-回复"模式，发送方发出请求后必须收到回复，中间不能被其他消息打断。C 通过 `MF_REPLY_PEND` 标志实现——RECEIVE 阶段检查到此标志时跳过通知检查，保证回复只能来自被调用方。

**NOTIFY 的"永不阻塞"**通过 `s_notify_pending` 位图实现：目标不在 RECEIVE 时，记录位图位，下次 RECEIVE 时优先检查位图，保证通知不丢失。

### 1.3 消息投递的延迟拷贝设计

IPC 调用时，消息**不直接写入接收方用户空间**，而是分两阶段投递：

1. **匹配阶段**（IPC 调用路径）：消息存入内核缓冲区 `p_delivermsg`，设置 `MF_DELIVERMSG`
2. **投递阶段**（`switch_to_user()`）：调用 `delivermsg()` 将 `p_delivermsg` 拷贝到用户空间 `p_delivermsg_vir`
3. **页错误处理**：拷贝若触发页错误，进程进入 VMSUSPEND 状态请求 VM 处理；连续两次失败触发 SIGSEGV

**为什么延迟拷贝？** 完整的因果链如下：

- 拷贝到用户空间可能触发**页错误**（用户缓冲区未映射）
- 页错误处理需要**阻塞**（请求 VM 映射页面，VM 是独立进程）
- IPC 匹配路径**不能阻塞**——必须先完成匹配逻辑，否则会破坏状态一致性（如发送者已入队但目标未唤醒）
- 因此延迟到 `switch_to_user()` 的**安全点**处理，此时 IPC 状态已稳定

### 1.4 RECEIVE 消息来源优先级

`mini_receive()` 检查三个消息来源的顺序（`proc.c:1000-1112`）：

1. **待处理通知**（`s_notify_pending` 位图）→ 构建通知消息投递
   - 跳过条件：`MF_REPLY_PEND` 置位（SENDREC 的 RECEIVE 阶段）
2. **待处理异步消息**（`s_asyn_pending` 位图）→ `try_async()` 投递
3. **同步发送者队列**（`p_caller_q` 链表）→ 遍历找匹配 `src_e` 投递 + 唤醒发送者
4. **都没有** → `RTS_RECEIVING` 阻塞

**为什么这个顺序？**
- 通知优先：通知是轻量级的，不应被同步消息阻塞
- 异步次之：异步消息已经"到达"，应优先于未来的同步发送者
- 同步最后：同步发送者会阻塞等待，可以延后处理

### 1.5 死锁检测：跟随 P_BLOCKEDON 动态链

SEND 时如何检测循环等待？跟随 `P_BLOCKEDON` 宏定义的阻塞链：

```
SEND(A → B) 时：
  B 阻塞在谁身上？(P_BLOCKEDON(B))
    若 B 在 SENDING → B 等的是 p_sendto_e 指向的 C
    若 B 在 RECEIVING → B 等的是 p_getfrom_e 指向的 C
  C 阻塞在谁身上？...
  如果链回到 A → 死锁
```

**关键宏 `P_BLOCKEDON`**（`proc.h:187-194`）动态选字段：

```c
#define P_BLOCKEDON(p) \
    (RTS_ISSET(p, RTS_SENDING) ? (p)->p_sendto_e : \
     RTS_ISSET(p, RTS_RECEIVING) ? (p)->p_getfrom_e : NONE)
```

**动态字段选择**是关键——每步跟随链时，根据当前进程的 RTS 状态**动态选择**字段，而非由调用方传入的 function 参数固定。这样能检测混合链死锁：A send→B, B receive←C, C send→A 是死锁，但固定 SEND 字段会在 B 处断链（B 在 RECEIVING 不是 SENDING）。

**2-cycle 特例**：A→send→B, B→receive←A 不是死锁（这是请求-回复模式）。C 利用 `RTS_SENDING = 0x04 = 1<<2` 的位编码，通过 `(xp->p_rts_flags ^ (function << 2)) & RTS_SENDING` 判定 SEND↔RECEIVE 模式（详见 §2.7）。

### 1.6 与上下游文档的关系

| 文档 | 提供的概念 | 12 的依赖点 |
|------|-----------|------------|
| 06-proc-init-boot-proc | IPC 状态组字段分组导航 / `p_rts_flags` 16 位全集 | IPC 字段分组与 RTS 不变量（不重复定义） |
| 10-switch-to-user | `switch_to_user` 主循环 / misc 标志处理 | `delivermsg()` 调用点（10 调用本节 §4.6） |
| 11-scheduling-primitives | RTS 状态机 / `rts_set`/`rts_unset` 联动 | SENDING/RECEIVING 对调度的影响 |
| 22-privilege | `struct priv` / `s_ipc_to` / `s_trap_mask` | 权限检查机制（22 提供位图语义） |
| 13-syscall-dispatch | `do_ipc` 系统调用入口 | 13 调用本节 §4.7 的 `do_ipc` |
| 14-exception-interrupt | 系统调用陷入入口 | 14 的陷入路径最终进入 `do_ipc` |
| 23-ipc-filter | IPC 过滤详过滤机制 | 23 详 `s_ipcf` 过滤器，本节只调用 |

**本文档边界**：只讲六原语核心机制（send/receive/notify/deadlock/delivermsg/senda），不讲权限详过滤（见 23）、syscall 陷入（见 14）、delivermsg 调用方（见 10）。

---

## 2. C 源码分析

### 2.1 数据结构：进程与特权中的 IPC 字段

C 在 `struct proc` 和 `struct priv` 中组织 IPC 相关状态，按职责分四组：

**组 1：队列链表**（FIFO 发送者等待）

| 字段 | 源码位置 | 语义 |
|------|---------|------|
| `p_caller_q` | proc.h:73 | 发送者等待队列头（链表），目标进程的"待处理发送者"链 |
| `p_q_link` | proc.h | 队列中下一个发送者（链表 next），内嵌在进程结构体 |

**组 2：阻塞端点**（死锁检测跟随字段）

| 字段 | 源码位置 | 语义 |
|------|---------|------|
| `p_getfrom_e` | proc.h:75 | RECEIVING 时等待的源端点（可 ANY） |
| `p_sendto_e` | proc.h:76 | SENDING 时等待的目标端点 |

**组 3：消息缓冲**（延迟拷贝核心）

| 字段 | 源码位置 | 语义 |
|------|---------|------|
| `p_sendmsg` | proc.h | 阻塞发送时缓存的消息（发送方持有） |
| `p_delivermsg` | proc.h | 待投递消息（接收方持有，延迟拷贝源） |
| `p_delivermsg_vir` | proc.h | 用户空间消息缓冲区地址（延迟拷贝目标） |

**组 4：标志位与权限**（状态机 + 权限）

| 字段 | 源码位置 | 语义 |
|------|---------|------|
| `p_rts_flags` | proc.h | RTS_SENDING/RECEIVING/NO_ENDPOINT 等状态位 |
| `p_misc_flags` | proc.h | MF_DELIVERMSG/REPLY_PEND/MSGFAILED/SENDING_FROM_KERNEL |
| `s_notify_pending` | priv | 通知待处理位图（`sys_map_t`，位索引=发送者 priv_id） |
| `s_asyn_pending` | priv | 异步消息待处理位图 |
| `s_ipc_to` | priv | IPC 目标白名单位图 |
| `s_trap_mask` | priv | 系统调用陷阱掩码 |
| `asynmsg_t` | ipc.h | 异步消息表条目（SENDA 用） |
| `message` | ipc.h | IPC 消息（56 字节） |

**为什么这样组织？** 链表因 FIFO 语义；位图因快速查找（O(1) 位测试）；缓冲因延迟拷贝（必须在内核持有而非用户空间）；标志位因状态机原子切换（一组位图位表达完整 IPC 状态）。

### 2.2 do_ipc() — 系统调用级 IPC 入口

> **入口路径**（详见 [13-syscall-dispatch §1.2-§1.3](13-syscall-dispatch.md)）：用户态执行 IPC 原语时，三架构通过**独立 trap vector** 进入内核，绕过 `kernel_call_dispatch_inner`，直接路由到 Rust 的 `dispatch_ipc_entry`：
> - **x86-64**：IDT vector 33（`IPC_VECTOR`，DPL=3 trap gate），由 `TrapEntryArch::configure_ipc_entry` 配置。C: protect.c:147
> - **aarch64**：SVC vector + 运行时读 `r3 == IPCVEC_INTR` 软件分流。C: earm/mpx.S:181-184
> - **riscv64**：ecall vector + 运行时读 `a7 < 17` 软件分流（IPC call_nr 1..=16）
>
> `dispatch_ipc_entry` 从 `msg.m_type` 解码 `IpcCall`（不经 `KERNEL_CALL` 偏移），acquire BKL 后调用本节的 `do_ipc` 等价物 `dispatch_ipc`。

**源码**: `proc.c:599-698`

```c
int do_ipc(reg_t r1, reg_t r2, reg_t r3) {
    caller_ptr = get_cpulocal_var(proc_ptr);
    int call_nr = (int) r1;
    // ptrace 处理：MF_SC_TRACE/MF_SC_DEFER
    // 权限检查在 do_sync_ipc() 中完成（proc.c:479-597）

    switch(call_nr) {
    case SENDREC:
        caller_ptr->p_misc_flags |= MF_REPLY_PEND;  // 标记原子两阶段
        // fall through 到 SEND
    case SEND:
        result = mini_send(caller_ptr, src_dst_e, m_ptr, 0);
        if (call_nr == SEND || result != OK) break;
        // SENDREC 且 SEND 成功 → fall through 到 RECEIVE
    case RECEIVE:
        result = mini_receive(caller_ptr, src_dst_e, m_ptr, 0);
        break;
    case NOTIFY:
        result = mini_notify(caller_ptr, src_dst_e);
        break;
    case SENDNB:
        result = mini_send(caller_ptr, src_dst_e, m_ptr, NON_BLOCKING);
        break;
    default:
        result = EBADCALL;
    }
    return result;
}
```

**关键语义**：
- **SENDREC 的 fall-through**：先 SEND，成功后自动 RECEIVE。C 用 fall-through 表达 SENDREC = SEND + RECEIVE 的原子组合
- **MF_REPLY_PEND 的作用**：阻止 RECEIVE 阶段被通知打断（SENDREC 语义要求回复必须来自被调用方）
- **SENDNB 的实现**：复用 `mini_send` 但传 `NON_BLOCKING` 标志，目标未就绪时返回 ENOTREADY
- **权限检查位置**：在 `do_sync_ipc()` 中完成（§2.3），`do_ipc` 只负责分派

### 2.3 do_sync_ipc() — 权限检查与原语分派

**源码**: `proc.c:479-597`

`do_sync_ipc` 执行三层权限检查，每层失败返回不同 errno：

| 层 | 检查 | 失败 errno | 源码位置 |
|---|------|-----------|---------|
| 1 | 端点有效性 `isokendpt` | EDEADSRCDST | proc.c:521-527 |
| 2 | IPC 目标白名单 `may_send_to` / `s_ipc_to` | ECALLDENIED | proc.c:536-544 |
| 3 | 陷阱掩码 `s_trap_mask & (1 << call_nr)` | ETRAPDENIED | proc.c:552-558 |
| 4 | 内核任务限制：`call_nr != SENDREC && call_nr != RECEIVE && iskerneln(src_dst_p)` | ETRAPDENIED | proc.c:560-566 |

**对内核任务的调用只能 SENDREC 或 RECEIVE**（proc.c:560-566）的设计动机：
- C 检查的是**目标端点**（`iskerneln(src_dst_p)`，proc.h:276 判定目标进程号 < 0 = 内核任务），**不是调用方**——调用方是谁不受此限制
- 内核任务（CLOCK/SYSTEM 等）总是回复消息：收到请求即处理即回复，从不主动发起 IPC
- 若调用方只 SEND 不 RECEIVE，内核任务的回复无人接收，任务会永远阻塞在发送上
- 因此 `do_sync_ipc` 对内核任务目标的调用只允许 SENDREC（发后即收）或 RECEIVE（等任务回复），拒绝单独的 SEND

### 2.4 mini_send() — 同步发送

**源码**: `proc.c:870-962`

`mini_send` 有两条路径，由 `WILLRECEIVE` 宏判定：

**路径 1：目标正在等待**（WILLRECEIVE 为真，`proc.c:895-923`）

```c
if (WILLRECEIVE(caller_ptr->p_endpoint, dst_ptr, m_ptr, NULL)) {
    if (!(flags & FROM_KERNEL)) {
        if (copy_msg_from_user(m_ptr, &dst_ptr->p_delivermsg))  // 用户态拷贝
            return EFAULT;
    } else {
        dst_ptr->p_delivermsg = *m_ptr;  // 内核消息直接赋值
        IPC_STATUS_ADD_FLAGS(dst_ptr, IPC_FLG_MSG_FROM_KERNEL);
    }
    dst_ptr->p_delivermsg.m_source = caller_ptr->p_endpoint;
    dst_ptr->p_misc_flags |= MF_DELIVERMSG;
    RTS_UNSET(dst_ptr, RTS_RECEIVING);  // 唤醒目标
}
```

**路径 2：目标未在等待**（阻塞发送方，`proc.c:924-960`）

```c
else {
    if (flags & NON_BLOCKING) return ENOTREADY;  // SENDNB 路径
    if (deadlock(SEND, caller_ptr, dst_e)) return ELOCKED;  // 死锁检测
    if (!(flags & FROM_KERNEL)) {
        if (copy_msg_from_user(m_ptr, &caller_ptr->p_sendmsg))  // 缓存消息
            return EFAULT;
    } else {
        caller_ptr->p_sendmsg = *m_ptr;
        caller_ptr->p_misc_flags |= MF_SENDING_FROM_KERNEL;
    }
    RTS_SET(caller_ptr, RTS_SENDING);       // 阻塞发送方
    caller_ptr->p_sendto_e = dst_e;
    // 加入目标的 p_caller_q 队尾
    xpp = &dst_ptr->p_caller_q;
    while (*xpp) xpp = &(*xpp)->p_q_link;
    *xpp = caller_ptr;
}
```

**关键语义**：
- `WILLRECEIVE` 判定目标是否愿意接收（含 SENDREC 中 RECEIVE 阶段判定，详见 §2.10）
- 延迟拷贝体现在"写入 `p_delivermsg` 而非用户空间"——实际拷贝由 `delivermsg()` 完成（§2.8）
- `FROM_KERNEL` 标志区分内核消息（直接赋值）与用户消息（需 `copy_msg_from_user`）
- 阻塞路径加入 `p_caller_q` 队尾，保证 FIFO 顺序

### 2.5 mini_receive() — 同步接收（三级检查）

**源码**: `proc.c:967-1117`

`mini_receive` 按优先级检查三个消息来源：

**第 1 级：待处理通知**（`proc.c:1000-1039`）

```c
if (!(caller_ptr->p_misc_flags & MF_REPLY_PEND)) {  // SENDREC 的 RECEIVE 阶段跳过
    if (has_pending_notify(caller_ptr, src_e)) {
        // 构建通知消息 → 写 p_delivermsg + MF_DELIVERMSG → 清位图位
    }
}
```

**第 2 级：待处理异步消息**（`proc.c:1040-1053`）

```c
if (has_pending_asend(caller_ptr, src_e)) {
    try_async(caller_ptr, src_e);  // 投递异步消息
}
```

**第 3 级：同步发送者队列**（`proc.c:1054-1099`）

```c
// 遍历 p_caller_q 找匹配 src_e 的发送者
xpp = &caller_ptr->p_caller_q;
while (*xpp) {
    if (CANRECEIVE(src_e, (*xpp)->p_endpoint, ...)) {
        // 投递：写 p_delivermsg + MF_DELIVERMSG
        // 唤醒发送者：RTS_UNSET(sender, RTS_SENDING) + 出队
        break;
    }
    xpp = &(*xpp)->p_q_link;
}
```

**第 4 级：阻塞**（`proc.c:1100-1112`）

```c
// 都没有 → 阻塞
RTS_SET(caller_ptr, RTS_RECEIVING);
caller_ptr->p_getfrom_e = src_e;
```

**投递语义三种来源统一**：消息写入 `p_delivermsg`，设置 `MF_DELIVERMSG`；唤醒发送者时清除 `RTS_SENDING` 并从 `p_caller_q` 移除；通知路径清除 `s_notify_pending` 位图位。

**MF_REPLY_PEND 跳过通知**保证 SENDREC 原子性：SENDREC 的 RECEIVE 阶段必须等待被调用方的回复，不能被其他通知打断。

### 2.6 mini_notify() — 异步通知

**源码**: `proc.c:1122-1167`

```c
int mini_notify(const struct proc *caller_ptr, endpoint_t dst_e) {
    if (WILLRECEIVE(caller_ptr->p_endpoint, dst_ptr, NULL, NULL)) {
        // 直接投递通知消息
        BuildNotifyMessage(&dst_ptr->p_delivermsg, src_proc_nr, caller_ptr);
        dst_ptr->p_misc_flags |= MF_DELIVERMSG;
        RTS_UNSET(dst_ptr, RTS_RECEIVING);
    } else {
        // 标记待处理位（位图索引 = 发送者 priv_id，不是端点号）
        priv(dst_ptr)->s_notify_pending |= (1 << priv_id(caller_ptr));
    }
    return OK;
}
```

**关键语义**：
- **永不阻塞，永不失败**（总是返回 OK）
- 目标在 RECEIVE → 直接投递（与 send 路径 1 类似，但消息由内核构建）
- 目标不在 RECEIVE → 设置位图，下次 RECEIVE 时检查
- **位图索引是 `priv_id(caller_ptr)`**（发送者的特权 ID），不是端点号或进程号——这是性能优化，`priv_id` 是稳定的小整数（0-63），适合位图

### 2.7 deadlock() — 死锁检测算法

**源码**: `proc.c:703-768`

```c
int deadlock(int function, register struct proc *cp, endpoint_t src_dst_e) {
    register struct proc *xp;
    int group_size = 1;

    while (src_dst_e != ANY) {
        int src_dst_slot;
        okendpt(src_dst_e, &src_dst_slot);
        xp = proc_addr(src_dst_slot);      // 跟随链到下一个进程
        group_size++;

        // P_BLOCKEDON 动态选字段，返回 NONE 表示无依赖
        if ((src_dst_e = P_BLOCKEDON(xp)) == NONE)
            return 0;  // 无环

        // 回到起点 → 可能死锁
        if (src_dst_e == cp->p_endpoint) {
            if (group_size == 2) {
                // 2-cycle 特例：SEND↔RECEIVE 不是死锁
                if ((xp->p_rts_flags ^ (function << 2)) & RTS_SENDING)
                    return 0;  // 请求-回复模式
            }
            return group_size;  // 死锁
        }
    }
    return 0;
}
```

**P_BLOCKEDON 宏**（`proc.h:187-194`）动态选字段：

```c
#define P_BLOCKEDON(p) \
    (RTS_ISSET(p, RTS_SENDING) ? (p)->p_sendto_e : \
     RTS_ISSET(p, RTS_RECEIVING) ? (p)->p_getfrom_e : NONE)
```

每步跟随链时，根据当前进程的 RTS 状态**动态选择**字段：
- 进程在 SENDING → 跟随 `p_sendto_e`
- 进程在 RECEIVING → 跟随 `p_getfrom_e`
- 都不在 → NONE（无依赖，无环）

**2-cycle 特例的位运算**（`proc.c:746`）：

```c
if ((xp->p_rts_flags ^ (function << 2)) & RTS_SENDING)
    return 0;  // 不是死锁
```

- `RTS_SENDING = 0x04 = 1 << 2`
- `function << 2`：SEND=1→0x04，RECEIVE=2→0x08
- SEND 调用 + xp 在 SENDING：`(0x04 ^ 0x04) & 0x04 = 0` → 是死锁（SEND↔SEND）
- SEND 调用 + xp 在 RECEIVING：`(0x08 ^ 0x04) & 0x04 = 0x04` → 不是死锁（SEND↔RECEIVE 请求-回复）

利用 `RTS_SENDING = 1<<2` 的位编码，将 function 左移 2 位后异或，巧妙判定方向是否相反。

### 2.8 delivermsg() — 延迟消息实际投递

**源码**: `proc.c:263-294`

```c
static void delivermsg(struct proc *p) {
    assert(p->p_misc_flags & MF_DELIVERMSG);
    // 将 p_delivermsg 拷贝到用户空间 p_delivermsg_vir
    if (copy_msg_to_user(&p->p_delivermsg, (message *) p->p_delivermsg_vir) != OK) {
        // 第 1 次失败 → vm_suspend 请求 VM 处理 + MF_MSGFAILED
        // 第 2 次连续失败 → cause_sig(SIGSEGV) 终止进程
    }
    p->p_misc_flags &= ~MF_DELIVERMSG;
}
```

**两次失败判定逻辑**：
- **第 1 次页错误是正常的**（VM 未映射用户缓冲区页）：`vm_suspend` 让 VM 处理映射后重试
- **第 2 次连续失败说明用户指针错误**（非法地址，VM 也无法映射）：`cause_sig(SIGSEGV)` 终止进程
- `MF_MSGFAILED` 标志位记录"上次失败"，下次 `delivermsg` 调用时检查此位判定是否连续失败

### 2.9 mini_senda() / try_deliver_senda() — 批量异步发送

**源码**: `proc.c:1200-1346`

- **`mini_senda(table, size)`**（proc.c:1331-1346）：权限检查 + 委托 `try_deliver_senda`
- **`try_deliver_senda(caller, table, size)`**（proc.c:1200-1326）：扫描 `asynmsg_t` 表，逐个尝试投递
  - 成功投递的标记 DONE
  - 失败的（目标未就绪）标记 pending，等下次 RECEIVE 重试
  - 全部完成后通知 ASYNCM（异步消息完成通知进程）

**SENDA 的"批量注册 + 延迟投递"**设计：
- 注册时不阻塞（扫描表是 O(n)，n 是表大小）
- 投递时按目标状态决定成功或 pending
- 失败的条目不丢失，必须等下次 RECEIVE 重试（保证消息可靠性）

### 2.10 关键宏 WILLRECEIVE / CANRECEIVE / P_BLOCKEDON

**`WILLRECEIVE(src_e, dst, m_sv, mp)`**（`ipc.h:14`）：

```c
#define WILLRECEIVE(src_e, dst, m_sv, mp) \
    (RTS_ISSET(dst, RTS_RECEIVING) && !RTS_ISSET(dst, RTS_SENDING) && \
     ((dst)->p_getfrom_e == ANY || (dst)->p_getfrom_e == (src_e)))
```

判定目标是否愿意接收来自 `src_e` 的消息：
- 目标在 RECEIVING 且不在 SENDING（SENDREC 的 RECEIVE 阶段 SENDING 已清）
- 源匹配：`p_getfrom_e == ANY` 或 `p_getfrom_e == src_e`

**`CANRECEIVE`**（`ipc.h:19`）：在 WILLRECEIVE 基础上增加 IPC 过滤器检查（`s_ipcf`），详见 23-ipc-filter。

**`P_BLOCKEDON(p)`**：见 §2.7。

这些宏是 C 实现"参数化多态"的方式——同一宏用于 send/notify 的 WILLRECEIVE 检查，deadlock 的 P_BLOCKEDON 跟随。Rust 中用方法 + enum match 替代（详见 §3.7）。

---

## 3. Rust 设计决策

> 本章聚焦"为什么这样设计"，可追溯 Ch1&2。每个决策含候选对比与 anti-translate 理由。

### 3.1 IPC 结果模型：IpcOutcome 枚举替代 Result + Err(NotReady) hack

**C 模式**：errno 返回（OK/ELOCKED/ENOTREADY/EDEADSRCDST 等）。C 用同一个 `OK` 表达"已投递"和"已阻塞"——靠副作用（RTS_SENDING 是否置位）区分。

**当前 Rust 直译陷阱**：`Result<(), IpcError>` + `Err(NotReady)` 表达"已阻塞"——语义混淆，把正常阻塞塞进 `Err`，调用方无法区分"已投递可继续"与"已阻塞需调度切换"。

**设计决策**：引入 `IpcOutcome` 三变体枚举：

```rust
pub enum IpcOutcome {
    Delivered,           // 消息已投递（或通知已记位图）
    Blocked,             // 调用方已阻塞（RTS_SENDING/RECEIVING 已置位）
    Error(IpcError),     // IPC 失败（错误码）
}
```

**理由**：阻塞是 IPC 的正常语义，不是错误。`IpcOutcome` 显式区分三种状态，调用方 match 处理。借鉴 02-stage-vm/draft/24-vm-ipc-dispatch 的 `VmReply::Suspend` 模式（区分"完成"与"挂起"）。

### 3.2 发送者队列：caller_q 索引式侵入 FIFO

> **改判说明**：v1 原设计 `SenderQueue(VecDeque<ProcNr>)` 为运行期堆结构，违反内核零堆纪律（C kernel 无 malloc，`p_caller_q` 链是进程表内嵌的侵入链）。改判为 C 同构的**索引式侵入 FIFO**。C 侧本就不是堆结构——本次改判是存储形态对 C 收敛（Refactor，非行为演进）：FIFO 语义/阻塞唤醒语义不变。旧"Rust 直译陷阱"段所指的 unsafe 问题在索引方案下不存在：链接是 `Option<ProcNr>` 槽索引（值语义，无别名借用），自由函数取 `&mut [KProcess]` 整表独占借用，无 unsafe。

**C 模式**：`struct proc *p_caller_q`（链头，目标进程槽，proc.h:73）+ `p_q_link`（链后继，发送方进程槽，proc.h:74）内嵌侵入链，pointer-pointer 遍历。

**设计决策**：同构保留侵入链布局，指针换槽索引——自由函数操作：

```rust
// 链分布（C 同构）：
//   目标槽:  caller_q_head / caller_q_tail  — C: p_caller_q（队头）
//   发送方槽: send_q_link                    — C: p_q_link（后继）
pub(crate) fn caller_q_push(procs: &mut [KProcess], dst_idx: usize, caller_idx: usize);
pub(crate) fn caller_q_find(procs: &[KProcess], dst_idx: usize, src_endpoint: Endpoint) -> Option<usize>;
pub(crate) fn caller_q_remove(procs: &mut [KProcess], dst_idx: usize, sender_idx: usize) -> bool;
pub(crate) fn caller_q_remove_by_nr(procs: &mut [KProcess], dst_idx: usize, target_nr: ProcNr) -> bool;
```

**理由**：
1. **零堆（否决 VecDeque 的根因）**：`VecDeque` 运行期堆分配，kernel 生产构建（无 `global_allocator`）中分配在链接期失败；C 的队列存储就在进程表内（侵入链），索引方案同构
2. 链接是 `Option<ProcNr>` 槽索引——值语义，无借用别名，无 unsafe；入队 = 两次索引写（C: proc.c:960-964 两次指针写），不可失败
3. `caller_q_tail` 是 O(1) 尾插扩展（C 遍历 O(n) 到尾）；FIFO 顺序与 C 逐行为一致
4. 槽位身份字段在 `sys_update` 槽交换时保留（C: do_update.c:241-258 `rp->p_caller_q = from_rp->p_caller_q`；`send_q_link` 随内容交换）——与 C 的 slot-identity/content 二分完全对齐

### 3.3 IpcEngine 形态：持有 &mut 借用的真实封装

**C 模式**：自由函数 `mini_send(caller_ptr, ...)` 每次传 `struct proc *`。

**Rust 直译陷阱**：`IpcEngine` 作为 ZST 仅作命名空间，每个方法签名 `fn send(procs: &mut [KProcess], caller: ProcNr, ...)` 重复传 procs——这是"为了 Rust 而 Rust"的过度设计，丢失了封装性。

**设计决策**：`IpcEngine<'a>` 持有 `&mut [KProcess]` + `&mut PrivTable` + `&dyn UserCopy` 三个借用，方法用 `&mut self`：

```rust
pub struct IpcEngine<'a> {
    procs: &'a mut [KProcess],
    priv_table: &'a mut PrivTable,
    user_copy: &'a dyn UserCopy,
}
```

**理由**：
1. `&mut [KProcess]` 是 BKL 的类型代理——Rust 借用检查器在编译期保证同一时刻只有一个可变引用，等价于 C 的 `big_kernel_lock` spinlock 语义。
2. 消除每个方法重复传 `procs`/`priv_table` 参数，API 更简洁。
3. `'a` 生命周期将 engine 绑定到借用域——engine 是短生命周期对象（每次 syscall dispatch 构造，返回时 drop），不会跨 syscall 存活。
4. `user_copy: &dyn UserCopy` 注入 arch 层实现，测试用 `KernelUserCopy` stub。

### 3.4 SendFlags：bitflags! 宏替代裸 u32 + 修正常量值

**C 模式**：`NON_BLOCKING=0x0080` / `FROM_KERNEL=0x0100`（`ipc.h:11-12`）

**当前 Rust 问题**：`SendFlags(u32)` 裸整数 + 手动常量，**值还错了**（`NON_BLOCKING=0x01`、`FROM_KERNEL=0x02`）——这是 P0 错误。

**设计决策**：改用 `bitflags!` 宏 + 修正常量值：

```rust
bitflags::bitflags! {
    pub struct SendFlags: u32 {
        const NON_BLOCKING = 0x0080;  // C: ipc.h:11
        const FROM_KERNEL = 0x0100;  // C: ipc.h:12
    }
}
```

**理由**：bitflags! 编译期检查位掩码合法性；`.contains()`/`.insert()`/`.remove()` 自文档化；**常量值必须与 C 严格对齐**。

### 3.5 IpcError 类型分层：保留 kernel/minix-types 分层

**C 模式**：errno 是全局整数。

**当前 Rust**：kernel `IpcError`（Deadlock/DeadSrcDst/NotReady/...）与 minix-types `IpcError`（InvalidEndpoint/WouldBlock/Interrupted/NoPerm）语义不同。

**设计决策**：**保留分层**——kernel IpcError 是权威定义（含内核专有错误码如 Deadlock）；minix-types IpcError 是用户态镜像（简化为用户可见错误）。

**理由**：用户态不应看到 Deadlock/DeadSrcDst 等内核内部错误码（这些在用户态被转为 EAGAIN/EINVAL）。强行统一会污染用户态 API。因此保留分层，文档说明映射关系。

### 3.6 死锁检测：blocked_on() 方法动态选字段

**C 模式**：`P_BLOCKEDON(p)` 宏动态选字段（RTS_SENDING→`p_sendto_e`，RTS_RECEIVING→`p_getfrom_e`）

**当前 Rust 问题**：`detect_deadlock` 由 function 参数固定选字段——无法检测混合链死锁（A send→B, B receive←C, C send→A）。

**设计决策**：引入 `blocked_on(nr)` 方法动态选字段，对齐 C 宏语义：

```rust
fn blocked_on(&self, procs: &[KProcess], nr: ProcNr) -> Option<Endpoint> {
    let p = &procs[nr_to_idx(nr)?];
    if p.p_rts_flags.is_set(RtsFlagsBits::SENDING) { Some(p.p_sendto_e) }
    else if p.p_rts_flags.is_set(RtsFlagsBits::RECEIVING) { Some(p.p_getfrom_e) }
    else { None }
}
```

**理由**：动态选字段是 C 宏的核心语义——固定字段无法检测混合链。这是 P0 修复。

### 3.7 WILLRECEIVE 宏替代：is_willing_to_receive 方法

**C 模式**：`WILLRECEIVE(src_e, dst, m_sv, mp)` 宏内联展开。

**设计决策**：用辅助方法 `is_willing_to_receive(dst, src) -> bool` 替代宏：

```rust
fn is_willing_to_receive(dst: &KProcess, src: Endpoint) -> bool {
    !dst.p_rts_flags.is_set(RtsFlagsBits::SENDING)
        && dst.p_rts_flags.is_set(RtsFlagsBits::RECEIVING)
        && (dst.p_getfrom_e == Endpoint::ANY || dst.p_getfrom_e == src)
}
```

**理由**：Rust 无宏污染，方法封装更清晰，编译器可内联优化。`is_willing_to_receive` 表达"目标是否愿意接收"的语义，非"宏展开"。

### 3.8 延迟拷贝保留：delivermsg 自由函数 + DeliverResult

**C 模式**：`delivermsg(p)` 由 `switch_to_user` → `process_misc_flags` 调用，完成用户空间拷贝。

**设计决策**：保留 C 的延迟拷贝设计，提取为 **自由函数** `ipc::delivermsg` 而非 `IpcEngine` 方法（FIX-20, Phase 1B）：

```rust
/// C: delivermsg(&p) — proc.c:263-294
pub fn delivermsg(
    proc: &mut KProcess,
    user_copy: &dyn UserCopy,
) -> DeliverResult;

pub enum DeliverResult {
    Delivered,    // 拷贝成功
    PageFault,    // 第 1 次页错误，需 vm_suspend
    Segfault,     // 第 2 次连续失败，需 cause_sig(SIGSEGV)
}
```

**为何是自由函数而非 `IpcEngine` 方法**：
- `process_misc_flags` 由 `ProcessTable` 调用，已持有 `procs` 切片；若调 `IpcEngine::deliver_message` 需构造完整 `IpcEngine`（需 `priv_table` + `procs` 切片），冗余且违反"最小依赖"
- `delivermsg` 只需 `&mut KProcess` + `&dyn UserCopy`，自由函数签名更精确表达真实依赖
- `IpcEngine::deliver_message` 保留为薄 wrapper，委托给自由函数（兼容已有 IpcEngine API）

**理由**：延迟拷贝是 IPC 核心机制不能改；`DeliverResult` 三态让调用方（`process_misc_flags` → `switch_to_user`）根据结果决定 `vm_suspend` 或 `cause_sig`。用户空间拷贝委托 `UserCopy` trait，arch 层实现见 §3.9。

### 3.9 用户空间拷贝：UserCopy trait 抽象

**C 模式**：`copy_msg_from_user`/`copy_msg_to_user` 直接调 `virtual_copy`。

**设计决策**：定义 `UserCopy` trait 抽象用户空间拷贝，arch 层实现：

```rust
pub trait UserCopy {
    fn copy_from_user<T>(&self, src: UserPtr<T>) -> Result<T, CopyError>;
    fn copy_to_user<T>(&self, dst: UserPtr<T>, val: T) -> Result<(), CopyError>;
}
```

**理由**：硬件抽象原则——用户空间拷贝涉及页表权限检查，必须 trait 化。当前 IPC 代码保留 `copy_msg_from_user` 调用点但标记 TODO，完整 trait 实现见后续 follow-up。

### 3.10 SENDA 批量：用户表直读

> **改判说明**：v1 原选 A（Vec-owned `AsyncMessageTable`，每 SENDA 一次堆分配）违反内核零堆纪律。改判为 C 同构的**用户表直读**（v1 选项 B 的 trait 化形态）：内核不拷贝表、不长期持有用户指针——每次扫描逐条 `A_RETR` 读用户表/`A_INSRT` 写回结果（proc.c:1231-1326 `mini_senda` 逐条扫描；重试路径 `try_one`/`deliver_async` 每次重读，proc.c:1425-1427——用户态可在重试间修改条目，C 语义如此）。"用户指针不长期持有"原则的落地方式：内核仅在发送方 priv 缓存**表指针三元组**（`s_asyntab`/`s_asynsize`/`s_asynendpoint`，C: priv.h:28，proc.c:1320-1323），重投递时用它重新走 `UserCopy` trait 校验读取，与 C 持久持有 `s_asyntab` 指针的行为一致（指针本身是用户地址值，非内核引用语义）。

**C 模式**：`asynmsg_t` 表（在调用方用户地址空间）+ `mini_senda` 逐条 `A_RETR`/`A_INSRT`（proc.c:1231-1326）+ priv 缓存表指针（proc.c:1320-1323）+ `try_one`/`deliver_async` 重试重读（proc.c:1390-1497）。

**设计决策**：`IpcEngine::senda(caller_nr, table: VirBytes, size: usize)` 逐条扫描用户表：
- 每条目经 `UserCopy::read_senda_entry` 读取（含用户指针校验），投递成功/失败经 `UserCopy::write_senda_result` 写回（C: `A_RETR`/`A_INSRT`）
- 目标不可达（`iskerneln` task 区）→ `ECALLDENIED`（proc.c:1266）；`may_send_to` 检查 caller 的 `s_ipc_to` 位图
- 未完成条目：目标 priv 的 `s_asyn_pending` 位图置位（C: proc.c:1328-1331 `setasynpending`）+ 发送方 priv 缓存表三元组——下次目标 `receive` 时 `deliver_async` **重读用户表**重试（INV-8）
- SENDA 从不阻塞调用方（与 C 一致：失败条目留 pending 位图，调用方继续运行）

**设计选项（多方案列举 + 选优）**：

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. Vec-owned `AsyncMessageTable` | `Vec<AsyncMessageEntry>` 内核副本 | 类型安全（v1 原选） | **堆分配（每 SENDA 一次，违反零堆）**；缓存副本偏离 C 语义（用户态改表后内核看不到） |
| **B. 用户表直读（trait 化）** | `UserCopy` 逐条 `A_RETR`/`A_INSRT`，priv 只缓存表指针三元组 | **零堆（C 同构）**；用户态可变语义保真；重试自然重读 | 每次扫描多次 `UserCopy` 调用（SENDA 低频，可接受） |
| C. 固定大小数组 | `[AsyncMessageEntry; N]` 内核副本 | 无堆 | N 难定（C 上限 16*PROC_TABLE_SIZE）；仍是副本语义，偏离 C |

**选定 B**。理由：C ground truth 就是用户表直读（`mini_senda` 全程无内核表副本，proc.c:1231-1326）；零堆；INV-8 的"用户表是权威状态"语义精确成立（用户态改条目、下次重试生效）。

**实现位置**：`os/kernel/src/ipc.rs`（`IpcEngine::senda` + `try_one`/`deliver_async` 重试路径 + `UserCopy::read_senda_entry`/`write_senda_result`）+ `os/kernel/src/kpriv.rs`（`s_asyntab`/`s_asynsize`/`s_asynendpoint`/`s_asyn_pending` 字段）。

### 3.11 SENDREC 两阶段：保留 MF_REPLY_PEND 标志

**C 模式**：SENDREC 用 `MF_REPLY_PEND` 标志 + fall-through 表达两阶段。

**设计决策**：保留 `MF_REPLY_PEND` 标志，不引入 typestate（`SendRec<Sending> → SendRec<Receiving>`）。

**理由**：typestate 增加复杂度但收益有限——`MF_REPLY_PEND` 已足够清晰表达 SENDREC 状态。权衡后选择保留标志位，标 TODO 评估未来 typestate 演进。

---

## 4. 实现要点

### 4.1 IpcCall / IpcError / SendFlags 定义

```rust
/// IPC 调用类型。C: call_nr in do_ipc() — proc.c:599
/// 值对齐 ipcconst.h:7-13
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpcCall {
    Send,       // C: SEND=1
    Receive,    // C: RECEIVE=2
    SendRec,    // C: SENDREC=3
    Notify,     // C: NOTIFY=4
    SendNb,     // C: SENDNB=5
    SendA,      // C: SENDA=16
}

/// IPC 错误码。C: errno from mini_* functions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    Deadlock,      // C: ELOCKED — proc.c:931
    DeadSrcDst,    // C: EDEADSRCDST — proc.c:889
    NotReady,      // C: ENOTREADY — proc.c:926
    BadCall,       // C: EBADCALL — proc.c:696
    Fault,         // C: EFAULT — proc.c:902,937
    CallDenied,    // C: ECALLDENIED — do_sync_ipc
    TrapDenied,    // C: ETRAPDENIED — do_sync_ipc
}

/// IPC 结果。区分"已投递"/"已阻塞"/"错误"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcOutcome {
    Delivered,
    Blocked,
    Error(IpcError),
}

bitflags::bitflags! {
    pub struct SendFlags: u32 {
        const NON_BLOCKING = 0x0080;  // C: ipc.h:11
        const FROM_KERNEL = 0x0100;   // C: ipc.h:12
        const SENDA = 0x0001;         // Rust 内部标志：senda 复用 send 路径时区分 IPC status call 类型
    }
}

/// 死锁环描述符。C: deadlock() — proc.c:703-768
/// direction 记录产生环的 IPC call 类型，调用方据此应用 2-cycle SEND↔RECEIVE 特例。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockCycle {
    pub chain: [ProcNr; PROC_TABLE_SIZE],  // 环中进程（遍历顺序）
    pub chain_len: usize,                   // 有效条目数
    pub direction: DeadlockDirection,       // 环方向（SEND 或 RECEIVE）
    pub group_size: usize,                  // 环中进程数；2 触发 SEND↔RECEIVE 检查
}

/// 死锁方向。与 IpcCall 分离，避免死锁检测器与 IPC 分派器语义耦合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockDirection {
    Send,       // 发送方向环（caller 在 SENDING 等待）
    Receive,    // 接收方向环（caller 在 RECEIVING 等待）
}
```

### 4.2 IpcEngine 核心 trait/方法签名

**位置**: os/kernel/src/ipc.rs:643-660

```rust
pub struct IpcEngine<'a> {
    procs: &'a mut [KProcess],
    priv_table: &'a mut PrivTable,
    user_copy: &'a dyn UserCopy,
}

impl<'a> IpcEngine<'a> {
    /// 同步发送。C: mini_send — proc.c:870-962
    pub fn send(&mut self, caller_nr: ProcNr, dst_endpoint: Endpoint, msg: &Message, flags: SendFlags) -> IpcOutcome;

    /// 同步接收。C: mini_receive — proc.c:967-1117
    pub fn receive(&mut self, caller_nr: ProcNr, src_endpoint: Endpoint) -> IpcOutcome;

    /// 原子 SEND + RECEIVE。C: do_ipc SENDREC 分支 — proc.c:655-666
    /// send 成功后 receive(ANY)；send 阻塞时设 MF_REPLY_PEND。
    pub fn sendrec(&mut self, caller_nr: ProcNr, dst_endpoint: Endpoint, msg: &Message) -> IpcOutcome;

    /// 异步通知。C: mini_notify — proc.c:1122-1167
    pub fn notify(&mut self, caller_nr: ProcNr, dst_endpoint: Endpoint) -> IpcOutcome;

    /// 死锁检测。C: deadlock — proc.c:703-768
    pub fn detect_deadlock(&mut self, function: IpcCall, caller_nr: ProcNr, dst_endpoint: Endpoint) -> Option<DeadlockCycle>;

    /// 动态选字段。C: P_BLOCKEDON 宏 — proc.h:187-194
    fn blocked_on(proc_: &KProcess) -> Option<Endpoint>;

    /// 延迟消息投递。C: delivermsg — proc.c:263-294
    pub fn deliver_message(&mut self, nr: ProcNr) -> DeliverResult;

    /// 检查目标是否愿意接收。C: WILLRECEIVE 宏 — ipc.h:14
    fn is_willing_to_receive(dst: &KProcess, src: Endpoint) -> bool;

    /// 批量异步发送。C: mini_senda — proc.c:1331-1346
    /// 逐条扫描用户表（A_RETR/A_INSRT 直读直写，零拷贝缓存），始终返回
    /// Delivered（SENDA 不阻塞）；未投递条目留在用户表等待重试。
    pub fn senda(&mut self, caller_nr: ProcNr, table: VirBytes, size: usize) -> IpcOutcome;

    /// IPC 权限检查。C: do_sync_ipc 权限层 — proc.c:479-597
    pub fn check_ipc_permission(&self, caller_nr: ProcNr, dst_endpoint: Endpoint, call: IpcCall) -> Result<(), IpcError>;

    /// IPC 入口分派。C: do_ipc — proc.c:599-698
    pub fn do_ipc(&mut self, caller_nr: ProcNr, call: IpcCall, dst_endpoint: Endpoint, msg: &Message, flags: SendFlags, senda_table: Option<(VirBytes, usize)>) -> IpcOutcome;
}
```

### 4.3 send 方法阶段划分（对齐 C 控制流）

```rust
pub fn send(&mut self, caller_nr: ProcNr, dst_endpoint: Endpoint, msg: &Message, flags: SendFlags) -> IpcOutcome {
    // Phase 1: 端点检查。C: proc.c:887-890
    if self.procs[dst_idx].p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) {
        return IpcOutcome::Error(IpcError::DeadSrcDst);
    }

    // Phase 2: WILLRECEIVE 检查。C: proc.c:895
    if Self::is_willing_to_receive(&self.procs[dst_idx], caller_endpoint) {
        // 路径 A：直接投递。C: proc.c:895-923
        // copy_msg_from_user (或 FROM_KERNEL 直接赋值)
        // 写 p_delivermsg + MF_DELIVERMSG + RTS_UNSET(RECEIVING)
        return IpcOutcome::Delivered;
    }

    // 路径 B：阻塞发送方。C: proc.c:924-960
    if flags.contains(SendFlags::NON_BLOCKING) {
        return IpcOutcome::Error(IpcError::NotReady);
    }
    if self.detect_deadlock(IpcCall::Send, caller_nr, dst_endpoint).is_some() {
        return IpcOutcome::Error(IpcError::Deadlock);
    }
    // 写 p_sendmsg + RTS_SET(SENDING) + p_sendto_e + caller_q.push_back
    return IpcOutcome::Blocked;
}
```

### 4.4 receive 方法三级检查（对齐 C 控制流）

```rust
pub fn receive(&mut self, caller_nr: ProcNr, src_endpoint: Endpoint) -> IpcOutcome {
    // Phase 1: 通知检查（MF_REPLY_PEND 跳过）。C: proc.c:1000-1039
    if !reply_pend {
        if self.take_pending_notify(caller_nr, src_endpoint).is_some() {
            // 构建通知消息 + 投递 + 清位图位
            return IpcOutcome::Delivered;
        }
    }

    // Phase 2: 异步消息检查。C: proc.c:1040-1053
    if let Some(async_src) = self.take_pending_async(caller_nr, src_endpoint) {
        // deliver_async 投递
        return IpcOutcome::Delivered;
    }

    // Phase 3: 同步发送者队列。C: proc.c:1054-1099
    // 遍历 caller_q 侵入链（caller_q_head → send_q_link → ...）找
    // p_endpoint 匹配 src 的发送者；Endpoint::ANY 匹配链头。
    if let Some(sender_idx) = caller_q_find(self.procs, caller_idx, src_endpoint) {
        // 先摘链（前驱链接 + 链头/链尾修正），再投递。
        caller_q_remove(self.procs, caller_idx, sender_idx);
        // 投递 sender.p_sendmsg + MF_DELIVERMSG
        // 唤醒 sender: RTS_UNSET(SENDING) + 清 SENDING_FROM_KERNEL
        // C: proc.c:1082-1083 — sender 若置 MF_SIG_DELAY（PM 延迟停止），
        // 在此记录到 sig_delay_sender；由 ProcessTable 级 dispatch_ipc
        // 在 do_ipc 返回后统一 sig_delay_done（原因见 19-syscall-signal.md §4.8）。
        return IpcOutcome::Delivered;
    }

    // Phase 4: 阻塞。C: proc.c:1100-1112
    // RTS_SET(RECEIVING) + p_getfrom_e = src
    return IpcOutcome::Blocked;
}
```

### 4.5 detect_deadlock 与 blocked_on 实现

**位置**: os/kernel/src/ipc.rs:685-783

```rust
/// 动态选字段。C: P_BLOCKEDON 宏 — proc.h:187-194
fn blocked_on(proc_: &KProcess) -> Option<Endpoint> {
    if proc_.p_rts_flags.is_set(RtsFlagsBits::SENDING) {
        Some(proc_.p_sendto_e)
    } else if proc_.p_rts_flags.is_set(RtsFlagsBits::RECEIVING) {
        Some(proc_.p_getfrom_e)
    } else {
        None
    }
}

pub fn detect_deadlock(&mut self, function: IpcCall, caller_nr: ProcNr, dst_endpoint: Endpoint)
    -> Option<DeadlockCycle>
{
    let caller_idx = self.idx_of(caller_nr)?;
    let caller_endpoint = self.procs[caller_idx].p_endpoint;
    let mut current_ep = dst_endpoint;
    let mut group_size = 1;

    loop {
        if current_ep == Endpoint::ANY { return None; }
        let target_idx = self.idx_by_endpoint(current_ep)?;
        group_size += 1;

        // 动态选字段跟随
        let next_ep = match Self::blocked_on(&self.procs[target_idx]) {
            Some(ep) => ep,
            None => return None,  // 无依赖，无环
        };

        // 回到起点 → 可能死锁
        if next_ep == caller_endpoint {
            if group_size == 2 {
                // 2-cycle 特例：SEND↔RECEIVE 不是死锁
                // C: (xp->p_rts_flags ^ (function << 2)) & RTS_SENDING — proc.c:746
                let xp_rts = self.procs[target_idx].p_rts_flags.get().bits();
                let function_shifted = (function as u32) << 2;
                if (xp_rts ^ function_shifted) & RtsFlagsBits::SENDING.bits() != 0 {
                    return None;  // 请求-回复模式，不是死锁
                }
            }
            return Some(DeadlockCycle { /* chain, direction, group_size */ });
        }
        current_ep = next_ep;
    }
}
```

### 4.6 delivermsg 自由函数实现（FIX-20, Phase 1B）

**位置**: os/kernel/src/ipc.rs:363

**调用方**: `ProcessTable::process_misc_flags`（os/kernel/src/proc_table.rs:851-879），当 `MF_DELIVERMSG` 置位时调用。

```rust
/// C: delivermsg(&p) — proc.c:263-294
pub fn delivermsg(
    proc: &mut KProcess,
    user_copy: &dyn UserCopy,
) -> DeliverResult {
    debug_assert!(proc.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));

    let msg = proc.p_delivermsg.clone();
    let user_addr = proc.p_delivermsg_vir;

    match user_copy.copy_msg_to_user(user_addr, &msg) {
        Ok(()) => {
            proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
            proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
            DeliverResult::Delivered
        }
        Err(CopyError::PageFault) => {
            if proc.p_misc_flags.is_set(MiscFlagsBits::MSGFAILED) {
                // 第 2 次连续失败 → SIGSEGV。C: proc.c:278
                proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
                proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
                DeliverResult::Segfault
            } else {
                // 第 1 次失败 → vm_suspend。C: proc.c:282-283
                proc.p_misc_flags.set(MiscFlagsBits::MSGFAILED);
                DeliverResult::PageFault
            }
        }
        Err(CopyError::OutOfBounds) => {
            proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
            proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
            DeliverResult::Segfault
        }
    }
}
```

**与 C 对齐**：
- 成功 → 清 `MF_DELIVERMSG` + `MF_MSGFAILED`（C: `p->p_misc_flags &= ~(MF_DELIVERMSG | MF_MSGFAILED)`）
- 第 1 次 `PageFault` → 置 `MF_MSGFAILED`，返回 `PageFault`（caller 路由 `vm_suspend(VMS_PAGEFAULT)`）
- 第 2 次连续 `PageFault`（`MF_MSGFAILED` 已置）→ 清标志，返回 `Segfault`（caller 路由 `cause_sig(SIGSEGV)`）
- `OutOfBounds` → 直接 `Segfault`（非法地址，VM 无法映射）

**`IpcEngine::deliver_message` 薄 wrapper**：保留以兼容已有 API，内部委托给自由函数：

```rust
impl IpcEngine {
    pub fn deliver_message(&mut self, nr: ProcNr) -> DeliverResult {
        // 委托给自由函数（FIX-20）。self.user_copy 是 IpcEngine 构造时
        // 注入的 &dyn UserCopy。
        let proc = &mut self.procs[nr_to_idx(nr).expect("valid proc nr")];
        delivermsg(proc, self.user_copy)
    }
}
```

### 4.7 check_ipc_permission 四层检查

**位置**: os/kernel/src/ipc.rs:1270-1318

```rust
pub fn check_ipc_permission(&self, caller_nr: ProcNr, dst_endpoint: Endpoint, call: IpcCall) -> Result<(), IpcError> {
    // 层 1: 端点有效性。C: proc.c:521-527
    let dst_idx = self.idx_by_endpoint(dst_endpoint);
    match dst_idx {
        None => return Err(IpcError::DeadSrcDst),
        Some(i) if self.procs[i].p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) => {
            return Err(IpcError::DeadSrcDst);
        }
        _ => {}
    }

    // 层 2: IPC 白名单。C: may_send_to — ipc.h
    if let Some(i) = dst_idx {
        if let Some(dst_pid) = self.procs[i].priv_id {
            if !caller_priv.may_send_to(dst_pid) {
                return Err(IpcError::CallDenied);
            }
        }
    }

    // 层 3: 陷阱掩码。C: s_trap_mask & (1 << call_nr) — proc.c:552
    // C 的 short s_trap_mask 符号扩展为 int，SRV_T = ~0 允许 SENDA(call_nr=16)
    let mask_extended = (caller_priv.ipc.s_trap_mask as i16) as u32;
    let call_bit = 1u32 << (call as u32);
    if (mask_extended & call_bit) == 0 {
        return Err(IpcError::TrapDenied);
    }

    // 层 4: 内核任务限制。C: proc.c:560-566。
    // 对内核任务目标（p_nr < 0）的调用只能 SENDREC 或 RECEIVE——
    // 内核任务总是回复，若调用方不接收，任务可能永远阻塞。
    // C 检查 TARGET（`iskerneln(src_dst_p)`），不是调用方：
    // `call_nr != SENDREC && call_nr != RECEIVE && iskerneln(src_dst_p)`。
    if call != IpcCall::SendRec
        && call != IpcCall::Receive
        && let Some(i) = dst_idx
        && self.procs[i].is_kernel_task()
    {
        return Err(IpcError::TrapDenied);
    }

    Ok(())
}
```

### 4.8 do_ipc 分派

**位置**: os/kernel/src/ipc.rs:1339-1388

> **入口前置**：`dispatch_ipc_entry`（syscall.rs:523-550，详见 [13-syscall-dispatch §4.8](13-syscall-dispatch.md)）从 IPC trap 入口接收控制流，解码 `IpcCall::from_raw(msg.m_type)`，acquire BKL 后调用本节的 `do_ipc`。本节描述的是 BKL 已持有后的分派逻辑。

> **SENDA 表传递**：C 通过 trap-frame 寄存器 `r3`（表指针）和 `r2`（表大小）传递 SENDA 表（proc.c:673, 683），不走消息字段。Rust API 对齐：`senda_table: Option<(VirBytes, usize)>` 是独立参数，仅在 `call == SendA` 时使用。

```rust
pub fn do_ipc(
    &mut self,
    caller_nr: ProcNr,
    call: IpcCall,
    dst_endpoint: Endpoint,
    msg: &Message,
    flags: SendFlags,
    senda_table: Option<(VirBytes, usize)>,
) -> IpcOutcome {
    // 权限检查（FIX-9）
    if let Err(e) = self.check_ipc_permission(caller_nr, dst_endpoint, call) {
        return IpcOutcome::Error(e);
    }

    match call {
        IpcCall::Send | IpcCall::SendNb => self.send(
            caller_nr, dst_endpoint, msg,
            if call == IpcCall::SendNb { flags | SendFlags::NON_BLOCKING } else { flags },
        ),
        IpcCall::Receive => self.receive(caller_nr, dst_endpoint),
        IpcCall::SendRec => self.sendrec(caller_nr, dst_endpoint, msg),
        IpcCall::Notify => self.notify(caller_nr, dst_endpoint),
        IpcCall::SendA => {
            // C: `size_t msg_size = (size_t) r2;` (proc.c:673)
            //    `return mini_senda(caller_ptr, (asynmsg_t *) r3, msg_size);` (proc.c:683)
            let (table_ptr, count) = match senda_table {
                Some(tc) => tc,
                None => return IpcOutcome::Error(IpcError::BadCall),
            };
            // C: 上限 16*(NR_TASKS + NR_PROCS) — proc.c:681
            let max_count = 16 * PROC_TABLE_SIZE;
            if count > max_count {
                return IpcOutcome::Error(IpcError::BadCall);
            }
            // 不预拷贝整表：`senda` 逐条读用户表（C: A_RETR），
            // 内核零堆分配，且重试语义与 C 同构（重试时重读表）。
            self.senda(caller_nr, table_ptr, count)
        }
    }
}
```

### 4.9 caller_q 队列操作（索引式侵入链自由函数）

**位置**: os/kernel/src/ipc.rs:502-607

caller_q 是**索引式侵入 FIFO**：链表节点内嵌在进程槽位中，指针退化为 `Option<ProcNr>` 槽位索引。链的存储分布与 C 同构（`proc.h:73-74`）：

```
目标槽（dst_idx）                     发送方槽
┌──────────────────┐                ┌──────────────────┐
│ caller_q_head ───┼──→ ProcNr(A) ─│ send_q_link ─────┼──→ ProcNr(B) ...
│ caller_q_tail ───┼──→ ProcNr(尾)  │                  │
└──────────────────┘                └──────────────────┘
```

- **链头/链尾** `caller_q_head`/`caller_q_tail` 在**目标槽**（C: 目标的 `p_caller_q` + `mini_send` 尾插时缓存的链尾）
- **后继链接** `send_q_link` 在**发送方槽**（C: 发送方的 `p_q_link`）

```rust
/// 尾插。C: while (*xpp) xpp = &(*xpp)->p_q_link; *xpp = caller;
/// — proc.c:1077-1105 的入队路径
pub(crate) fn caller_q_push(procs: &mut [KProcess], dst_idx: usize, caller_idx: usize);

/// 沿链查找 p_endpoint 匹配 src 的发送者槽位索引（不移除）。
/// C: while (*xpp) { if (CANRECEIVE(...)) break; } — proc.c:1077-1105
/// Endpoint::ANY 匹配链头（C: 队首即最长等待者）。
pub(crate) fn caller_q_find(
    procs: &[KProcess],
    dst_idx: usize,
    src_endpoint: Endpoint,
) -> Option<usize>;

/// 摘链：前驱 send_q_link 改接 + 链头/链尾修正。
/// C: *xpp = (*xpp)->p_q_link — proc.c:1084（查找+摘链一次遍历）；
/// 同形态遍历见 clear_ipc（system.c:520-531）。
pub(crate) fn caller_q_remove(procs: &mut [KProcess], dst_idx: usize, sender_idx: usize) -> bool;

/// 按 ProcNr 摘链（内部解析槽位索引）。
/// C: clear_ipc / abort_proc_ipc_send 循环 — system.c:520-531
pub(crate) fn caller_q_remove_by_nr(procs: &mut [KProcess], dst_idx: usize, target_nr: ProcNr) -> bool;

/// 队列判空（链头为 None）。
pub(crate) fn caller_q_is_empty(procs: &[KProcess], dst_idx: usize) -> bool;
```

**为什么是自由函数而非 `KProcess` 方法/独立容器**：

1. **零堆纪律**：`VecDeque<ProcNr>`（v1 设计）是运行期堆结构——C 内核无 malloc，链表节点内嵌于 `proc[]` 静态数组。侵入链对 C 同构，链操作只是槽位字段读写，无任何分配。
2. **链跨槽分布**：链头在目标槽、后继在发送方槽，任何单槽方法都无法在不拿整表的情况下完成链操作。自由函数以 `&mut [KProcess]`（或 `&[KProcess]`）为第一参数，借用形状与数据分布一致。
3. **O(1) 尾插**：C 的 `mini_send` 入队需从头遍历到尾（`while (*xpp) xpp = &(*xpp)->p_q_link`）；Rust 缓存 `caller_q_tail` 后尾插 O(1)。这是实现优化，FIFO 语义不变（语义等价，非 ARCH 演进）。
4. **slot-identity 与 content 二分**（`sys_update` 槽交换语义，见 [06 §3.4](06-proc-init-boot-proc.md)）：链身份 = 槽位 ProcNr；槽交换后新进程继承旧槽的队列位置——`caller_q_remove_by_nr` 按槽位摘链正确处理该语义，与 `do_update.c:241-258` 一致。

**find + remove 分离**：`caller_q_find`（不可变借用）与 `caller_q_remove`（可变借用）分离，借用按序结束/开始，借用检查器接受。旧 v1 `SenderQueue` 因队列容器内聚于单槽子对象、需 find/remove 两阶段借用规避——侵入链后链接散布在发送方槽，借用退化为普通顺序槽访问，不再有该约束。

---

## 5. 测试

### 5.1 单元测试覆盖矩阵

| 测试名 | 覆盖路径 | C 行为对齐 | 优先级 |
|--------|---------|-----------|--------|
| `test_send_when_target_receiving` | send 路径 A（WILLRECEIVE 为真） | proc.c:895-923 | P0 |
| `test_send_when_target_not_receiving` | send 路径 B（阻塞入队） | proc.c:924-960 | P0 |
| `test_send_non_blocking_returns_not_ready` | send + NON_BLOCKING | proc.c:925-927 | P0 |
| `test_send_detects_deadlock` | send + 死锁检测 | proc.c:930-932 | P0 |
| `test_receive_picks_notify_first` | receive Phase 1（通知优先） | proc.c:1000-1039 | P0 |
| `test_receive_skips_notify_when_reply_pend` | receive + MF_REPLY_PEND | proc.c:1000-1005 | P0 |
| `test_receive_picks_async_second` | receive Phase 2（async 次之） | proc.c:1040-1053 | P0 |
| `test_receive_picks_caller_q_last` | receive Phase 3（caller_q 最后） | proc.c:1054-1099 | P0 |
| `test_receive_sig_delay_sender_records_pending_delay` | receive Phase 3 + `MF_SIG_DELAY` sender（记录 one-shot） | proc.c:1082-1083 | P0 |
| `test_receive_plain_sender_has_no_pending_delay` | receive Phase 3 无 `MF_SIG_DELAY` 不记录 | proc.c:1082-1083 | P0 |
| `test_receive_blocks_when_no_match` | receive Phase 4（阻塞） | proc.c:1100-1112 | P0 |
| `test_notify_delivers_when_target_receiving` | notify 路径 A | proc.c:1122-1150 | P0 |
| `test_notify_records_bitmap_when_not_receiving` | notify 路径 B（位图） | proc.c:1151-1167 | P0 |
| `test_notify_never_blocks` | notify 永不阻塞 | proc.c:1122 | P0 |
| `test_deadlock_no_cycle_empty_table` | 无环链（空表） | proc.c:736-737 | P0 |
| `test_deadlock_no_cycle_when_target_runnable` | 无环链（目标可运行） | proc.c:736-737 | P0 |
| `test_deadlock_three_proc_send_cycle` | 三进程单环死锁 | proc.c:743-764 | P0 |
| `test_deadlock_send_receive_two_cycle_not_deadlock` | 2-cycle SEND↔RECEIVE 特例 | proc.c:744-749 | P0 |
| `test_deadlock_send_send_two_cycle_is_deadlock` | 2-cycle SEND↔SEND 死锁 | proc.c:744-749 | P0 |
| `test_deadlock_receive_receive_two_cycle_is_deadlock` | 2-cycle RECV↔RECV 死锁 | proc.c:744-749 | P0 |
| `test_deadlock_mixed_chain_cycle` | 混合链死锁（防固定字段回归） | P0 FIX-3 验证 | P0 |
| `test_deadlock_send_state_mismatch` | blocked_on 动态选字段验证 | proc.h:187-194 | P0 |
| `test_deliver_message_success` | delivermsg 成功 | proc.c:263-294 | P0 |
| `test_deliver_message_first_page_fault` | 第 1 次页错误 → PageFault | proc.c:282-283 | P0 |
| `test_deliver_message_second_consecutive_fault` | 连续两次 → Segfault | proc.c:278 | P0 |
| `test_process_misc_flags_clears_delivermsg` | DELIVERMSG 经 `ipc::delivermsg` 拷贝成功后清除（FIX-20） | proc.c:263-294 + 351-415 | P0 |
| `test_check_ipc_permission_target_kernel_task_restriction` | 对内核任务目标仅允许 SENDREC/RECEIVE（层 4，TARGET 方向） | proc.c:560-566 | P0 |
| `test_senda_all_delivered` | SENDA 全部投递成功 | proc.c:1331-1346 | P1 |
| `test_dispatch_ipc_entry_routes_send_to_ipc_engine` | IPC trap 入口 → dispatch_ipc 路由 SEND | proc.c:599-697 | P0 |
| `test_dispatch_ipc_entry_bad_call_nr_returns_ebadcall` | 无效 call_nr（0/17/255）→ EBADCALL | proc.c:695-696 | P0 |
| `test_dispatch_ipc_entry_acquires_bkl` | 入口 acquire BKL（mem::forget guard） | mpx.S:ipc_entry BKL_LOCK | P1 |

### 5.2 混合链死锁测试（P0 防回归）

```rust
#[test]
fn test_deadlock_mixed_chain_cycle() {
    // 设置：A send→B, B receive←C, C send→A
    // 这是固定字段检测会漏的死锁（B 在 RECEIVING 不是 SENDING，固定 SEND 字段会断链）
    let mut procs = create_test_procs(3);
    set_rts(&mut procs[0], RtsFlagsBits::SENDING);
    procs[0].p_sendto_e = Endpoint(2);  // A send→B
    set_rts(&mut procs[1], RtsFlagsBits::RECEIVING);
    procs[1].p_getfrom_e = Endpoint(3);  // B receive←C
    set_rts(&mut procs[2], RtsFlagsBits::SENDING);
    procs[2].p_sendto_e = Endpoint(1);  // C send→A

    let mut engine = IpcEngine::new(&mut procs, &mut priv_table, &user_copy);
    let cycle = engine.detect_deadlock(IpcCall::Send, ProcNr(1), Endpoint(2));
    assert!(cycle.is_some(), "mixed-chain deadlock must be detected");
    assert_eq!(cycle.unwrap().chain_len, 3);
}
```

---

## 6. 参见

- [06-proc-init-boot-proc](06-proc-init-boot-proc.md) — IPC 状态组字段分组导航 / `p_rts_flags` 16 位全集 / RTS 不变量
- [10-switch-to-user](10-switch-to-user.md) — `switch_to_user` 调用 `delivermsg` / misc 标志处理
- [11-scheduling-primitives](11-scheduling-primitives.md) — `RTS_SENDING`/`RECEIVING` 状态机 / `rts_set` 联动
- [22-privilege](22-privilege.md) — `struct priv` / `s_ipc_to` / `s_trap_mask` 权限位图
- [13-syscall-dispatch](13-syscall-dispatch.md) — `do_ipc` 系统调用入口分派
- [14-exception-interrupt](14-exception-interrupt.md) — 系统调用陷入入口
- [23-ipc-filter](23-ipc-filter.md) — IPC 过滤详过滤机制（`s_ipcf`）
- [02-stage-vm/24-vm-ipc-dispatch](../02-stage-vm/draft/24-vm-ipc-dispatch.md) — VM 服务端 IPC 分派（`VmReply::Suspend` anti-translate 设计参考）
