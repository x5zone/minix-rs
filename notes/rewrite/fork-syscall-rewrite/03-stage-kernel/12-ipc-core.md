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

`mini_receive()` 检查三个消息来源的顺序（`proc.c:1000-1095`）：

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
| 06-proc-init-boot-proc | `struct proc` / `p_rts_flags` / `p_misc_flags` 字段 | 进程结构体字段语义（不重复定义） |
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
| 1 | 端点有效性 `isokendpt` | EDEADSRCDST | proc.c:487-495 |
| 2 | IPC 目标白名单 `may_send_to` / `s_ipc_to` | ECALLDENIED | proc.c:500-520 |
| 3 | 陷阱掩码 `s_trap_mask & (1 << call_nr)` | ETRAPDENIED | proc.c:520-540 |
| 4 | 内核任务限制：`call_nr != SENDREC && iskerneln` | ETRAPDENIED | proc.c:560-566 |

**内核任务只能 SENDREC**（proc.c:560-566）的设计动机：
- 内核任务（CLOCK/SYSTEM 等）总是回复消息，单独 SEND 会让任务无法回复（任务设计为收到请求即处理即回复）
- 内核任务不能阻塞在发送上——如果调用方只 SEND 不 RECEIVE，任务会永远阻塞
- 因此 `do_sync_ipc` 显式拒绝内核任务的 SEND/RECEIVE 单独调用

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

**第 1 级：待处理通知**（`proc.c:1000-1030`）

```c
if (!(caller_ptr->p_misc_flags & MF_REPLY_PEND)) {  // SENDREC 的 RECEIVE 阶段跳过
    if (has_pending_notify(caller_ptr, src_e)) {
        // 构建通知消息 → 写 p_delivermsg + MF_DELIVERMSG → 清位图位
    }
}
```

**第 2 级：待处理异步消息**（`proc.c:1031-1070`）

```c
if (has_pending_asend(caller_ptr, src_e)) {
    try_async(caller_ptr, src_e);  // 投递异步消息
}
```

**第 3 级：同步发送者队列**（`proc.c:1071-1095`）

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

**第 4 级：阻塞**（`proc.c:1096-1110`）

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

**理由**：阻塞是 IPC 的正常语义，不是错误。`IpcOutcome` 显式区分三种状态，调用方 match 处理。借鉴 02-stage-vm/24-vm-ipc-dispatch 的 `VmReply::Suspend` 模式（区分"完成"与"挂起"）。

### 3.2 发送者队列：SenderQueue 封装 + 索引替代指针链表

**C 模式**：`struct proc *p_caller_q` + `p_q_link` 内嵌链表，pointer-pointer 遍历。

**当前 Rust 直译陷阱**：用 `AtomicI32` 索引模拟 C 链表，仍需 unsafe 访问进程表，所有权混乱。

**设计决策**：保留 `SenderQueue` 封装，内部用 `AtomicI32` 索引（与 KProcess 字段 `p_caller_q`/`p_q_link` 对齐），但提供类型安全方法 `enqueue`/`remove`/`find_matching`。

**理由**：完全替换为 `VecDeque<ProcNr>` 需重构 KProcess 字段（移除 `p_q_link`），影响 06 文档的字段定义。当前封装已提供类型安全，且与 C 字段布局对齐便于对照。未来如需完全 Rust 化可演进。

### 3.3 IpcEngine 形态：当前 ZST + 未来演进

**C 模式**：自由函数 `mini_send(caller_ptr, ...)` 每次传 `struct proc *`。

**当前 Rust**：`IpcEngine` 是 ZST 仅作命名空间，方法签名 `fn send(procs: &mut [KProcess], caller: ProcNr, ...)` 重复传 procs。

**设计决策**：保留 ZST 形态，但方法签名集中表达 IPC 状态机。未来如需持有状态（如 async_tables）可演化为 `IpcEngine<'a> { procs: &'a mut [KProcess] }`。

**理由**：当前 ZST 已能正确表达 IPC 语义，过早引入生命周期参数会增加调用方复杂度。`&mut [KProcess]` 是 BKL 的类型代理，已保证单 CPU 互斥。

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

### 3.8 延迟拷贝保留：deliver_message 方法

**C 模式**：`delivermsg(p)` 由 `switch_to_user` 调用，完成用户空间拷贝。

**设计决策**：保留 C 的延迟拷贝设计，提供 `IpcEngine::deliver_message(procs, nr) -> DeliverResult` 方法：

```rust
pub enum DeliverResult {
    Delivered,    // 拷贝成功
    PageFault,    // 第 1 次页错误，需 vm_suspend
    Segfault,     // 第 2 次连续失败，需 cause_sig(SIGSEGV)
}
```

**理由**：延迟拷贝是 IPC 核心机制不能改；`DeliverResult` 让调用方（switch_to_user）根据结果决定 vm_suspend 或 cause_sig。当前实现是骨架（用户空间拷贝委托 UserCopy trait，trait 完整实现见后续 follow-up）。

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

### 3.10 SENDA 批量：AsyncMessageTable + try_deliver_all

**C 模式**：`asynmsg_t` 表 + `try_deliver_senda` 扫描（proc.c:1200-1326, 1331-1346）。

**设计决策**：`AsyncMessageTable` 持有 `Vec<AsyncMessageEntry>`（Vec-owned，非裸指针 + size 对），每条目跟踪 `AsyncEntryState`（Pending/Done/NotReady）。`try_deliver_all` 遍历 Pending 条目，经 `engine.send`（`FROM_KERNEL` 标志——异步消息是内核缓存副本，非用户指针）逐个投递；`Blocked` 结果记为 `NotReady` 待下次 RECEIVE 重试（INV-8）。`IpcEngine::senda` 调用 `table.try_deliver_all(self, caller_nr)` 后**始终返回 `Delivered`**——SENDA 从不阻塞调用方（与 C 一致：失败条目留 pending，调用方继续运行）。

**设计选项（多方案列举 + 选优）**：

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. Vec-owned `AsyncMessageTable`** | `Vec<AsyncMessageEntry>` + 状态字段 + `try_deliver_all` | 类型安全；状态显式；可在 `no_std` 用 `alloc::Vec` | 堆分配（每 SENDA 一次） |
| **B. 裸指针 + size（C 1:1）** | `*const asynmsg_t` + `usize`，遍历时读用户空间 | 无堆分配 | 反复 `copy_from_user`；无状态跟踪；违反"用户指针不长期持有"原则 |
| **C. 固定大小数组** | `[AsyncMessageEntry; N]` | 无堆分配 | N 难定（C 上限 16*PROC_TABLE_SIZE）；浪费空间 |

**选定 A**：Vec-owned。理由：SENDA 调用频率低（仅 ASYNCM 进程），堆分配开销可接受；状态跟踪是正确性必需（INV-8 重试要求区分 Pending/Done/NotReady）；`alloc::Vec` 在 `no_std` 可用（`extern crate alloc`）。C: proc.c:1200-1326 `try_deliver_senda` 内部也缓存了用户表副本。

**实现位置**：`os/kernel/src/ipc.rs:300-381`（`AsyncMessageEntry`/`AsyncEntryState`/`AsyncMessageTable`/`try_deliver_all`）+ `os/kernel/src/ipc.rs:1059-1064`（`IpcEngine::senda`）。

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
    BadCall,       // C: EBADCALL — proc.c:95
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
    }
}
```

### 4.2 IpcEngine 核心 trait/方法签名

> **结构演进说明**：下方签名展示**设计意图**（方法名 + C 对应 + 参数语义）。实际代码中 `IpcEngine` 已从 unit struct 演进为 `IpcEngine<'a>` 生命周期结构体（3 字段：`procs: &'a mut [KProcess]` + `priv_table` + `user_copy`），方法从关联函数（`fn send(procs, ...)`）改为 `&mut self` 方法（`fn send(&mut self, caller_nr, ...)`）。以代码 `os/kernel/src/ipc.rs:528-535` 为源真相；本节签名作为 API 概览，参数细节以代码为准。

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

    /// 异步通知。C: mini_notify — proc.c:1122-1167
    pub fn notify(&mut self, caller_nr: ProcNr, dst_endpoint: Endpoint) -> IpcOutcome;

    /// 死锁检测。C: deadlock — proc.c:703-768
    pub fn detect_deadlock(&self, function: IpcCall, caller_nr: ProcNr, dst_endpoint: Endpoint) -> Option<DeadlockCycle>;

    /// 动态选字段。C: P_BLOCKEDON 宏 — proc.h:187-194
    fn blocked_on(&self, nr: ProcNr) -> Option<Endpoint>;

    /// 延迟消息投递。C: delivermsg — proc.c:263-294
    pub fn deliver_message(&mut self, nr: ProcNr) -> DeliverResult;

    /// 检查目标是否愿意接收。C: WILLRECEIVE 宏 — ipc.h:14
    fn is_willing_to_receive(dst: &KProcess, src: Endpoint) -> bool;

    /// 批量异步发送。C: mini_senda — proc.c:1331-1346
    /// 调用 `table.try_deliver_all(self, caller_nr)` 后始终返回 Delivered（SENDA 不阻塞）。
    pub fn senda(&mut self, caller_nr: ProcNr, table: &mut AsyncMessageTable) -> IpcOutcome;
}
```

### 4.3 send 方法阶段划分（对齐 C 控制流）

```rust
pub fn send(procs, caller, dst, msg, flags) -> IpcOutcome {
    // Phase 1: 端点检查。C: proc.c:887-890
    if dst.p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) {
        return IpcOutcome::Error(IpcError::DeadSrcDst);
    }

    // Phase 2: WILLRECEIVE 检查。C: proc.c:895
    if Self::is_willing_to_receive(dst, caller.p_endpoint) {
        // 路径 A：直接投递。C: proc.c:895-923
        // copy_from_user (或 FROM_KERNEL 直接赋值)
        // 写 p_delivermsg + MF_DELIVERMSG + RTS_UNSET(RECEIVING)
        return IpcOutcome::Delivered;
    }

    // 路径 B：阻塞发送方。C: proc.c:924-960
    if flags.contains(SendFlags::NON_BLOCKING) {
        return IpcOutcome::Error(IpcError::NotReady);
    }
    if Self::detect_deadlock(procs, IpcCall::Send, caller, dst).is_some() {
        return IpcOutcome::Error(IpcError::Deadlock);
    }
    // 写 p_sendmsg + RTS_SET(SENDING) + p_sendto_e + 入 p_caller_q 队尾
    return IpcOutcome::Blocked;
}
```

### 4.4 receive 方法三级检查（对齐 C 控制流）

```rust
pub fn receive(procs, caller, src) -> IpcOutcome {
    // Phase 1: 通知检查（MF_REPLY_PEND 跳过）。C: proc.c:1000-1030
    if !caller.p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND) {
        if has_pending_notify(caller, src) {
            // 构建通知消息 + 投递 + 清位图位
            return IpcOutcome::Delivered;
        }
    }

    // Phase 2: 异步消息检查。C: proc.c:1031-1070
    if has_pending_asend(caller, src) {
        // try_async 投递
        return IpcOutcome::Delivered;
    }

    // Phase 3: 同步发送者队列。C: proc.c:1071-1095
    if let Some(sender) = SenderQueue::find_matching(procs, caller, src) {
        // 投递 sender.p_sendmsg + MF_DELIVERMSG
        // 唤醒 sender: RTS_UNSET(SENDING) + 出队
        return IpcOutcome::Delivered;
    }

    // Phase 4: 阻塞。C: proc.c:1096-1110
    // RTS_SET(RECEIVING) + p_getfrom_e = src
    return IpcOutcome::Blocked;
}
```

### 4.5 detect_deadlock 与 blocked_on 实现

```rust
fn blocked_on(procs: &[KProcess], nr: ProcNr) -> Option<Endpoint> {
    let p = &procs[nr_to_idx(nr)?];
    if p.p_rts_flags.is_set(RtsFlagsBits::SENDING) {
        Some(p.p_sendto_e)
    } else if p.p_rts_flags.is_set(RtsFlagsBits::RECEIVING) {
        Some(p.p_getfrom_e)
    } else {
        None
    }
}

pub fn detect_deadlock(procs, function, caller, initial_dst) -> Option<DeadlockCycle> {
    let mut src_dst_e = initial_dst;
    let mut group_size = 1;
    let caller_ep = procs[nr_to_idx(caller)?].p_endpoint;

    while src_dst_e != Endpoint::ANY {
        let target = procs.iter().find(|p| p.p_endpoint == src_dst_e)?;
        let target_nr = target.p_nr;
        group_size += 1;

        // 动态选字段跟随
        src_dst_e = match Self::blocked_on(procs, target_nr)? {
            None => return None,  // 无依赖，无环
            Some(e) => e,
        };

        // 回到起点 → 可能死锁
        if src_dst_e == caller_ep {
            if group_size == 2 {
                // 2-cycle 特例：SEND↔RECEIVE 不是死锁
                // C: (xp->p_rts_flags ^ (function << 2)) & RTS_SENDING — proc.c:746
                let xp_rts = target.p_rts_flags.bits();
                let function_byte = match function {
                    IpcCall::Send | IpcCall::SendRec | IpcCall::SendNb => 1u8,
                    IpcCall::Receive => 2u8,
                    _ => return None,
                };
                if (xp_rts ^ (function_byte as u32) << 2) & RtsFlagsBits::SENDING.bits() != 0 {
                    return None;  // 请求-回复模式，不是死锁
                }
            }
            return Some(DeadlockCycle { /* ... */ });
        }
    }
    None
}
```

### 4.6 deliver_message 实现

```rust
pub fn deliver_message(procs: &mut [KProcess], nr: ProcNr) -> DeliverResult {
    let idx = nr_to_idx(nr).expect("valid proc nr");
    assert!(procs[idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));

    let msg = procs[idx].p_delivermsg.clone();
    let user_addr = procs[idx].p_delivermsg_vir;

    // TODO: 完整实现需 UserCopy trait (§3.9)
    // 当前骨架：假定拷贝成功（无用户空间拷贝）
    match copy_msg_to_user(&msg, user_addr) {
        Ok(()) => {
            procs[idx].p_misc_flags.remove(MiscFlagsBits::DELIVERMSG);
            procs[idx].p_misc_flags.remove(MiscFlagsBits::MSGFAILED);
            DeliverResult::Delivered
        }
        Err(CopyError::PageFault) => {
            if procs[idx].p_misc_flags.is_set(MiscFlagsBits::MSGFAILED) {
                // 第 2 次连续失败 → SIGSEGV
                procs[idx].p_misc_flags.remove(MiscFlagsBits::DELIVERMSG);
                procs[idx].p_misc_flags.remove(MiscFlagsBits::MSGFAILED);
                DeliverResult::Segfault
            } else {
                // 第 1 次失败 → vm_suspend
                procs[idx].p_misc_flags.insert(MiscFlagsBits::MSGFAILED);
                DeliverResult::PageFault
            }
        }
        Err(CopyError::OutOfBounds) => DeliverResult::Segfault,
    }
}
```

### 4.7 check_ipc_permission 三层检查

```rust
fn check_ipc_permission(procs: &[KProcess], priv_table: &PrivTable, caller: ProcNr, dst: Endpoint, call: IpcCall) -> Result<(), IpcError> {
    // 层 1: 端点有效性。C: proc.c:487-495
    let dst_proc = procs.iter().find(|p| p.p_endpoint == dst)
        .ok_or(IpcError::DeadSrcDst)?;
    if dst_proc.p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) {
        return Err(IpcError::DeadSrcDst);
    }

    // 层 2: IPC 白名单。C: may_send_to — ipc.h
    let caller_priv = priv_table.get(procs[caller_idx].priv_id)
        .ok_or(IpcError::CallDenied)?;
    if !may_send_to(caller_priv, dst) {
        return Err(IpcError::CallDenied);
    }

    // 层 3: 陷阱掩码。C: s_trap_mask & (1 << call_nr) — proc.c:520-540
    if !caller_priv.s_trap_mask.contains_bit(call as u32) {
        return Err(IpcError::TrapDenied);
    }

    // 层 4: 内核任务限制。C: proc.c:560-566
    if is_kernel_task(caller) && call != IpcCall::SendRec {
        return Err(IpcError::TrapDenied);
    }

    Ok(())
}
```

### 4.8 do_ipc 分派

```rust
pub fn do_ipc(procs: &mut [KProcess], priv_table: &mut PrivTable, caller: ProcNr, call: IpcCall, dst: Endpoint, msg: &Message) -> IpcOutcome {
    // 权限检查
    if let Err(e) = Self::check_ipc_permission(procs, priv_table, caller, dst, call) {
        return IpcOutcome::Error(e);
    }

    match call {
        IpcCall::SendRec => {
            // 设 MF_REPLY_PEND 阻止通知打断 RECEIVE 阶段
            procs[caller_idx].p_misc_flags.insert(MiscFlagsBits::REPLY_PEND);
            // fall through 到 SEND
            let send_outcome = Self::send(procs, caller, dst, msg, SendFlags::NONE);
            match send_outcome {
                IpcOutcome::Delivered => {
                    // SEND 成功，继续 RECEIVE
                    Self::receive(procs, caller, Endpoint::ANY)
                }
                IpcOutcome::Blocked => IpcOutcome::Blocked,  // SEND 阻塞
                IpcOutcome::Error(e) => IpcOutcome::Error(e),
            }
        }
        IpcCall::Send => Self::send(procs, caller, dst, msg, SendFlags::NONE),
        IpcCall::Receive => Self::receive(procs, caller, dst),
        IpcCall::Notify => Self::notify(procs, priv_table, caller, dst),
        IpcCall::SendNb => Self::send(procs, caller, dst, msg, SendFlags::NON_BLOCKING),
        IpcCall::SendA => {
            // C: `size_t msg_size = (size_t) r2;` (proc.c:673)
            //    `return mini_senda(caller_ptr, (asynmsg_t *) r3, msg_size);` (proc.c:683)
            // 实际代码（ipc.rs:1241-1261）以 `&mut self` 风格实现，此处用旧风格示意
            let (table_ptr, count) = match senda_table {
                Some(tc) => tc,
                None => return IpcOutcome::Error(IpcError::BadCall),
            };
            // C: 上限 16*(NR_TASKS + NR_PROCS) — proc.c:681
            let max_count = 16 * PROC_TABLE_SIZE;
            if count > max_count {
                return IpcOutcome::Error(IpcError::BadCall);
            }
            match user_copy.copy_senda_table_from_user(table_ptr, count) {
                Ok(entries) => {
                    let mut table = AsyncMessageTable::from_raw_entries(entries);
                    Self::senda(procs, caller, &mut table)
                }
                Err(_) => IpcOutcome::Error(IpcError::Fault),
            }
        }
    }
}
```

### 4.9 SenderQueue 队列操作

```rust
pub struct SenderQueue;

impl SenderQueue {
    /// 入队尾。C: while (*xpp) xpp = &(*xpp)->p_q_link; *xpp = caller;
    pub fn enqueue(procs: &mut [KProcess], dst_nr: ProcNr, sender_nr: ProcNr);

    /// 移除指定发送者。C: *xpp = sender->p_q_link;
    pub fn remove(procs: &mut [KProcess], dst_nr: ProcNr, sender_nr: ProcNr);

    /// 找匹配源端点的发送者。C: while (*xpp) { if (CANRECEIVE(...)) break; }
    pub fn find_matching(procs: &[KProcess], dst_nr: ProcNr, src: Endpoint) -> Option<ProcNr>;
}
```

**实现说明**：用 `AtomicI32` 索引（与 KProcess 的 `p_caller_q`/`p_q_link` 字段对齐），保证 BKL 保护下的单 CPU 访问安全。

---

## 5. 测试

### 5.1 单元测试覆盖矩阵

| 测试名 | 覆盖路径 | C 行为对齐 | 优先级 |
|--------|---------|-----------|--------|
| `test_send_when_target_receiving` | send 路径 A（WILLRECEIVE 为真） | proc.c:895-923 | P0 |
| `test_send_when_target_not_receiving` | send 路径 B（阻塞入队） | proc.c:924-960 | P0 |
| `test_send_non_blocking_returns_not_ready` | send + NON_BLOCKING | proc.c:925-927 | P0 |
| `test_send_detects_deadlock` | send + 死锁检测 | proc.c:930-932 | P0 |
| `test_send_from_kernel_skips_user_copy` | send + FROM_KERNEL | proc.c:903-906 | P1 |
| `test_receive_picks_notify_first` | receive Phase 1（通知优先） | proc.c:1000-1030 | P0 |
| `test_receive_skips_notify_when_reply_pend` | receive + MF_REPLY_PEND | proc.c:1000-1005 | P0 |
| `test_receive_picks_async_second` | receive Phase 2（async 次之） | proc.c:1031-1070 | P0 |
| `test_receive_picks_caller_q_last` | receive Phase 3（caller_q 最后） | proc.c:1071-1095 | P0 |
| `test_receive_blocks_when_no_match` | receive Phase 4（阻塞） | proc.c:1096-1110 | P0 |
| `test_receive_with_any_source` | receive + Endpoint::ANY | proc.c:1023 | P1 |
| `test_notify_delivers_when_target_receiving` | notify 路径 A | proc.c:1122-1150 | P0 |
| `test_notify_records_bitmap_when_not_receiving` | notify 路径 B（位图） | proc.c:1151-1167 | P0 |
| `test_notify_never_blocks` | notify 永不阻塞 | proc.c:1122 | P0 |
| `test_detect_deadlock_no_cycle` | 无环链 | proc.c:736-737 | P0 |
| `test_detect_deadlock_single_cycle` | 单环死锁 | proc.c:743-764 | P0 |
| `test_detect_deadlock_two_cycle_send_receive_not_deadlock` | 2-cycle SEND↔RECEIVE 特例 | proc.c:744-749 | P0 |
| `test_detect_deadlock_mixed_chain` | 混合链死锁（防固定字段回归） | P0 FIX-3 验证 | P0 |
| `test_deliver_message_success` | delivermsg 成功 | proc.c:263-294 | P0 |
| `test_deliver_message_first_page_fault` | 第 1 次页错误 → PageFault | proc.c:278 | P0 |
| `test_deliver_message_second_consecutive_fault` | 连续两次 → Segfault | proc.c:283 | P0 |
| `test_check_ipc_permission_kernel_task_only_sendrec` | 内核任务限制 | proc.c:560-566 | P0 |

### 5.2 混合链死锁测试（P0 防回归）

```rust
#[test]
fn test_detect_deadlock_mixed_chain() {
    // 设置：A send→B, B receive←C, C send→A
    // 这是固定字段检测会漏的死锁（B 在 RECEIVING 不是 SENDING，固定 SEND 字段会断链）
    let mut procs = create_test_procs(3);
    set_rts(&mut procs[0], RtsFlagsBits::SENDING);
    procs[0].p_sendto_e = Endpoint(2);  // A send→B
    set_rts(&mut procs[1], RtsFlagsBits::RECEIVING);
    procs[1].p_getfrom_e = Endpoint(3);  // B receive←C
    set_rts(&mut procs[2], RtsFlagsBits::SENDING);
    procs[2].p_sendto_e = Endpoint(1);  // C send→A

    let cycle = IpcEngine::detect_deadlock(&procs, IpcCall::Send, 1, Endpoint(2));
    assert!(cycle.is_some(), "mixed-chain deadlock must be detected");
    assert_eq!(cycle.unwrap().chain.len(), 3);
}
```

---

## 6. 参见

- [06-proc-init-boot-proc](06-proc-init-boot-proc.md) — `struct proc` 字段语义 / `p_rts_flags` / `p_misc_flags`
- [10-switch-to-user](10-switch-to-user.md) — `switch_to_user` 调用 `delivermsg` / misc 标志处理
- [11-scheduling-primitives](11-scheduling-primitives.md) — `RTS_SENDING`/`RECEIVING` 状态机 / `rts_set` 联动
- [22-privilege](22-privilege.md) — `struct priv` / `s_ipc_to` / `s_trap_mask` 权限位图
- [13-syscall-dispatch](13-syscall-dispatch.md) — `do_ipc` 系统调用入口分派
- [14-exception-interrupt](14-exception-interrupt.md) — 系统调用陷入入口
- [23-ipc-filter](23-ipc-filter.md) — IPC 过滤详过滤机制（`s_ipcf`）
- [02-stage-vm/24-vm-ipc-dispatch](../02-stage-vm/24-vm-ipc-dispatch.md) — VM 服务端 IPC 分派（`VmReply::Suspend` anti-translate 设计参考）
