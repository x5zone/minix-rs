# 12-ipc-core: IPC 核心机制

> **分类**: Kernel IPC 核心
> **源码**: `minix3/minix/kernel/proc.c:599-1590`
> **前置**: 10（进程状态机，RTS_SENDING/RECEIVING 语义）
> **C 总行数**: ~1000 行（内核最复杂的部分）

---

## 1. 概述

### 1.1 核心问题

Minix3 的微内核架构中，进程间通信是系统的基石。内核不提供"服务"，只提供"通信"——所有服务请求都通过 IPC 消息传递完成。内核的 IPC 职责是：

1. **消息中转**：将发送方的消息拷贝到接收方
2. **阻塞管理**：发送/接收不匹配时阻塞进程
3. **死锁检测**：防止循环等待
4. **通知机制**：轻量级单边通知，不丢失

### 1.2 六个 IPC 原语

| 原语 | 语义 | 阻塞条件 | 唤醒条件 | RTS 标志变化 |
|------|------|---------|---------|-------------|
| SEND | 发送消息 | 目标未在 RECEIVE | 目标调用 RECEIVE | RTS_SENDING set/clear |
| RECEIVE | 接收消息 | 无匹配消息可用 | 有消息到达 | RTS_RECEIVING set/clear |
| SENDREC | 先 SEND 再 RECEIVE（原子） | SEND 或 RECEIVE 阻塞 | 两步都完成 | RTS_SENDING → RTS_RECEIVING |
| NOTIFY | 发送轻量通知 | **永不阻塞** | — | 无（写入 s_notify_pending 位图） |
| SENDNB | 非阻塞发送 | 目标未就绪返回 ENOTREADY | — | 无 |
| SENDA | 异步批量发送 | 不阻塞（扫描表逐个投递） | — | 无 |

### 1.3 消息投递的延迟拷贝设计

IPC 调用时，消息不直接写入接收方用户空间：
1. 存入内核缓冲区 `p_delivermsg`，设置 `MF_DELIVERMSG`
2. `switch_to_user()` 在恢复进程前调用 `delivermsg()` 完成实际拷贝
3. 如果拷贝触发页错误，进程进入 VMSUSPEND 状态

**为什么延迟拷贝？**：因为拷贝到用户空间可能触发页错误，而 IPC 处理路径不能直接阻塞——需要先完成 IPC 匹配逻辑，再在安全点处理页错误。

### 1.4 RECEIVE 的消息来源优先级

```
RECEIVE(src_e) 检查顺序：
1. 待处理通知（s_notify_pending 位图）
2. 待处理异步消息（s_asyn_pending 位图）
3. 同步发送者队列（p_caller_q 中的进程）
4. 都没有 → RTS_RECEIVING 阻塞
```

### 1.5 死锁检测

```
SEND(A → B) 时：
  B 是否在等 C？(RTS_SENDING, p_sendto_e = C)
  C 是否在等 D？...
  如果形成环 → ELOCKED
```

---

## 2. C 源码分析

### 2.1 do_ipc() — 系统调用级 IPC 入口

**源码**: `proc.c:599-698`

```c
int do_ipc(reg_t r1, reg_t r2, reg_t r3) {
    caller_ptr = get_cpulocal_var(proc_ptr);
    int call_nr = (int) r1;

    // ptrace 处理：MF_SC_TRACE/MF_SC_DEFER
    // 权限检查：s_trap_mask & (1 << call_nr)
    // 内核任务只允许 SENDREC

    switch(call_nr) {
    case SENDREC:
        caller_ptr->p_misc_flags |= MF_REPLY_PEND;
        // fall through
    case SEND:
        result = mini_send(caller_ptr, src_dst_e, m_ptr, 0);
        if (call_nr == SEND || result != OK) break;
        // fall through for SENDREC
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
- SENDREC 的 fall-through：先 SEND，成功后自动 RECEIVE
- MF_REPLY_PEND：阻止通知中断 SENDREC 的 RECEIVE 阶段
- 权限检查在 do_sync_ipc() 中完成（proc.c:479-598）

### 2.2 mini_send() — 同步发送

**源码**: `proc.c:870-965`

**路径 1：目标正在等待**（WILLRECEIVE 为真）

```c
if (WILLRECEIVE(caller_ptr->p_endpoint, dst_ptr, m_ptr, NULL)) {
    copy_msg_from_user(m_ptr, &dst_ptr->p_delivermsg);
    dst_ptr->p_delivermsg.m_source = caller_ptr->p_endpoint;
    dst_ptr->p_misc_flags |= MF_DELIVERMSG;
    RTS_UNSET(dst_ptr, RTS_RECEIVING);  // 唤醒目标
}
```

**路径 2：目标未在等待**（阻塞发送方）

```c
else {
    if (flags & NON_BLOCKING) return ENOTREADY;
    if (deadlock(SEND, caller_ptr, dst_e)) return ELOCKED;
    copy_msg_from_user(m_ptr, &caller_ptr->p_sendmsg);
    RTS_SET(caller_ptr, RTS_SENDING);       // 阻塞发送方
    caller_ptr->p_sendto_e = dst_e;
    // 加入目标的 p_caller_q 队尾
    xpp = &dst_ptr->p_caller_q;
    while (*xpp) xpp = &(*xpp)->p_q_link;
    *xpp = caller_ptr;
}
```

### 2.3 mini_receive() — 同步接收

**源码**: `proc.c:967-1120`

**检查顺序**：

1. **待处理通知**：`has_pending_notify()` → 构建 NOTIFY 消息 → 投递
2. **待处理异步消息**：`has_pending_asend()` → `try_async()` → 投递
3. **同步发送者队列**：遍历 `p_caller_q`，找到匹配 `src_e` 的发送者 → 投递
4. **阻塞**：`RTS_SET(caller_ptr, RTS_RECEIVING)`, `p_getfrom_e = src_e`

**投递语义**（三种来源统一）：
- 消息写入 `p_delivermsg`，设置 `MF_DELIVERMSG`
- 同步发送者：清除 `RTS_SENDING`，从 `p_caller_q` 移除
- 通知：清除 `s_notify_pending` 位

### 2.4 mini_notify() — 异步通知

**源码**: `proc.c:1122-1196`

```c
int mini_notify(const struct proc *caller_ptr, endpoint_t dst_e) {
    // 目标正在 RECEIVE？
    if (WILLRECEIVE(caller_ptr->p_endpoint, dst_ptr, NULL, NULL)) {
        // 直接投递通知消息
        BuildNotifyMessage(&dst_ptr->p_delivermsg, src_proc_nr, caller_ptr);
        dst_ptr->p_misc_flags |= MF_DELIVERMSG;
        RTS_UNSET(dst_ptr, RTS_RECEIVING);
    } else {
        // 标记待处理位
        priv(dst_ptr)->s_notify_pending |= (1 << src_id);
    }
    return OK;
}
```

**关键语义**：
- 永不阻塞，永不失败
- 目标在 RECEIVE → 直接投递
- 目标不在 RECEIVE → 设置位图，下次 RECEIVE 时检查

### 2.5 delivermsg() — 延迟消息投递

**源码**: `proc.c:263-297`

```c
static void delivermsg(struct proc *p) {
    assert(p->p_misc_flags & MF_DELIVERMSG);
    // 将 p_delivermsg 拷贝到用户空间 p_delivermsg_vir
    if (copy_msg_to_user(&p->p_delivermsg,
            (message *) p->p_delivermsg_vir) != OK) {
        // 页错误 → VMSUSPEND
    }
    p->p_misc_flags &= ~MF_DELIVERMSG;
}
```

### 2.6 deadlock() — 死锁检测

**源码**: `proc.c:703-770`

```c
int deadlock(int function, struct proc *cp, endpoint_t dst_e) {
    struct proc *xp;
    int group_size = 1;
    while (src_dst_e != ANY) {
        // xp = proc_addr(src_dst_slot)
        // group_size++
        // P_BLOCKEDON(xp): if NONE → no cycle; if cp's endpoint → possible
        //                   group_size==2 + SEND↔RECEIVE not fatal
        // for SEND: src_dst_e = P_SENDTO_E(xp)
        // for RECEIVE: src_dst_e = proc_nr(caller_q(xp))  /* walk callers */
    }
    return 0;  // no deadlock
}
```

**Rust 实现**: `IpcEngine::detect_deadlock()` — `kernel/src/ipc.rs:216-269`

- SEND/SENDREC/SENDNB: walk `p_sendto_e` 链，要求每步在 `RTS_SENDING`
- **RECEIVE** (P1-19 2026-06-13 修复): walk `p_getfrom_e` 链，要求每步在 `RTS_RECEIVING`
- NOTIFY/SENDA: 永不阻塞，无需死锁检测（返回 `None`）
- 2-cycle (`group_size == 2`) 的 SEND↔RECEIVE 由调用方（`IpcCall` 状态位）判定合法性，详见 §3 D5

### 2.7 try_deliver_senda() / mini_senda() — 异步批量发送

**源码**: `proc.c:1200-1346`

- `mini_senda()`: 注册异步消息表（`asynmsg_t *table, size_t size`）
- `try_deliver_senda()`: 扫描表，逐个尝试投递
- 成功投递的标记为 DONE，失败的重试
- 全部完成后通知 ASYNCM

---

## 3. Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| p_caller_q 链表 | `*mut Proc` 链表 vs `SenderQueue<ProcNr>` | **SenderQueue 封装** | 类型安全，避免裸指针 |
| deadlock() | 返回 i32 vs Option | **`detect_deadlock() -> Option<Cycle>`** | 显式表达"有环/无环" |
| IPC 入口 | 单函数 vs IpcEngine | **IpcEngine 结构** | 封装 IPC 状态机，避免全局状态 |
| 消息拷贝 | 返回 errno vs Result | **`Result<(), PageFault>`** | 统一错误处理 |
| NOTIFY 位图 | u32 vs bitflags | **`NotifyBitmap: u32`** | 位索引 = priv_id，简单映射 |
| asynmsg_t 表 | 裸指针 vs AsyncMessageTable | **AsyncMessageTable 结构** | 类型安全 |
| IPC 错误码 | i32 vs enum | **`IpcError` 枚举** | ELOCKED/EDEADSRCDST/ENOTREADY 等 |

---

## 4. 实现要点

### 4.1 IpcCall 枚举

```rust
/// IPC 调用类型。
/// C: call_nr in do_ipc() — proc.c:599
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcCall {
    Send,
    Receive,
    SendRec,
    Notify,
    SendNb,
    SendA,
}
```

### 4.2 IpcError 枚举

```rust
/// IPC 错误码。
/// C: ELOCKED/EDEADSRCDST/ENOTREADY/EBADCALL/EFAULT/ECALLDENIED/ETRAPDENIED
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    /// 死锁检测发现循环等待。C: ELOCKED
    Deadlock,
    /// 源或目标端点无效。C: EDEADSRCDST
    DeadSrcDst,
    /// 非阻塞发送目标未就绪。C: ENOTREADY
    NotReady,
    /// 无效的 IPC 调用号。C: EBADCALL
    BadCall,
    /// 消息拷贝失败（页错误）。C: EFAULT
    Fault,
    /// IPC 权限被拒绝。C: ECALLDENIED
    CallDenied,
    /// 系统调用陷阱权限被拒绝。C: ETRAPDENIED
    TrapDenied,
}
```

### 4.3 SenderQueue

```rust
/// 发送者等待队列。
/// C: p_caller_q 链表 — proc.h:120, proc.c:960-964
///
/// 使用 ProcNr 索引替代 C 的指针链表。
/// 每个进程有 p_q_link 字段指向下一个发送者。
pub struct SenderQueue;

impl SenderQueue {
    /// 将发送者加入队尾。
    /// C: `while (*xpp) xpp = &(*xpp)->p_q_link; *xpp = caller_ptr;`
    pub fn enqueue(procs: &mut [KProcess], dst_nr: ProcNr, sender_nr: ProcNr);

    /// 从队列中移除指定发送者。
    /// C: `*xpp = sender->p_q_link;`
    pub fn remove(procs: &mut [KProcess], dst_nr: ProcNr, sender_nr: ProcNr);

    /// 遍历队列，找到匹配源端点的发送者。
    /// C: `while (*xpp) { if (CANRECEIVE(...)) break; }`
    pub fn find_matching(
        procs: &[KProcess],
        dst_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> Option<ProcNr>;
}
```

### 4.4 IpcEngine

```rust
/// IPC 核心引擎。
///
/// 封装所有 IPC 操作，替代 C 的全局函数调用。
/// 所有方法需要 &mut ProcessTable（BKL 保护）。
pub struct IpcEngine;

impl IpcEngine {
    /// 同步发送。C: mini_send() — proc.c:870-965
    pub fn send(
        procs: &mut [KProcess],
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
        flags: SendFlags,
    ) -> Result<(), IpcError>;

    /// 同步接收。C: mini_receive() — proc.c:967-1120
    pub fn receive(
        procs: &mut [KProcess],
        caller_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> Result<(), IpcError>;

    /// 原子发送+接收。C: SENDREC in do_ipc() — proc.c:574-583
    pub fn sendrec(
        procs: &mut [KProcess],
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
    ) -> Result<(), IpcError>;

    /// 异步通知。C: mini_notify() — proc.c:1122-1196
    pub fn notify(
        procs: &mut [KProcess],
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
    ) -> Result<(), IpcError>;

    /// 非阻塞发送。C: SENDNB — proc.c:589
    pub fn send_nb(
        procs: &mut [KProcess],
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
    ) -> Result<(), IpcError>;

    /// 死锁检测。C: deadlock() — proc.c:703-770
    pub fn detect_deadlock(
        procs: &[KProcess],
        function: IpcCall,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
    ) -> Option<DeadlockCycle>;

    /// 延迟消息投递。C: delivermsg() — proc.c:263-297
    pub fn deliver_message(
        proc: &mut KProcess,
    ) -> Result<(), IpcError>;
}
```

---

## 5. 测试

### 5.1 单元测试

| 测试 | 验证内容 |
|------|---------|
| `test_send_target_waiting` | 目标在 RECEIVE → 直接投递，唤醒目标 |
| `test_send_target_not_waiting` | 目标不在 RECEIVE → 阻塞发送方，入 p_caller_q |
| `test_sendnb_not_ready` | 非阻塞发送目标未就绪 → ENOTREADY |
| `test_receive_pending_notify` | 有待处理通知 → 构建通知消息投递 |
| `test_receive_pending_async` | 有待处理异步消息 → 投递 |
| `test_receive_sync_sender` | p_caller_q 中有匹配发送者 → 投递，唤醒发送者 |
| `test_receive_block` | 无消息 → RTS_RECEIVING 阻塞 |
| `test_sendrec_atomic` | SENDREC 先 SEND 成功后自动 RECEIVE |
| `test_deadlock_detection` | 循环等待 → ELOCKED |
| `test_notify_bitmap` | 目标不在 RECEIVE → 设置 s_notify_pending 位 |
| `test_delivermsg_success` | MF_DELIVERMSG → 拷贝到用户空间 → 清除标志 |
| `test_delivermsg_pagefault` | 拷贝触发页错误 → VMSUSPEND |

### 5.2 集成测试

| 测试 | 验证内容 |
|------|---------|
| `test_ipc_ping_pong` | 两个进程交替 SEND/RECEIVE |
| `test_notify_wakeup` | 中断通知唤醒阻塞的驱动进程 |

---

## 6. 补充：IPC 常量与权限详细定义

> 来源：tmp-09-sync-ipc.md

### 6.1 IPC 调用号常量

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

### 6.2 IPC 标志

定义于 `minix3/minix/kernel/ipc.h:11-12`：

| 常量 | 值 | 含义 |
|------|-----|------|
| `NON_BLOCKING` | 0x0080 | 非阻塞模式（SENDNB 使用） |
| `FROM_KERNEL` | 0x0100 | 消息来自内核（代表进程发送） |

### 6.3 IPC 状态码宏

定义于 `minix3/minix/include/minix/ipcconst.h:20-35`：

| 宏 | 含义 |
|-----|------|
| `IPC_STATUS_CALL(status)` | 从状态码提取 IPC 调用类型 |
| `IPC_STATUS_CALL_TO(call)` | 将调用类型编码到状态码 |
| `IPC_FLG_MSG_FROM_KERNEL` | 标记消息来自内核 |
| `IPC_STATUS_FLAGS(flgs)` | 将标志编码到状态码高位 |
| `IPC_STATUS_FLAGS_TEST(status, flgs)` | 从状态码测试标志 |

### 6.4 IPC 权限检查宏

定义于 `minix3/minix/kernel/ipc.h:14-22`：

| 宏 | 含义 |
|-----|------|
| `WILLRECEIVE(src_e, dst_ptr, m_src_v, m_src_p)` | 目标进程是否愿意接收来自 src_e 的消息 |
| `CANRECEIVE(receive_e, src_e, dst_ptr, m_src_v, m_src_p)` | 接收条件是否满足（endpoint 匹配 + IPC 过滤器） |

`WILLRECEIVE` 条件：目标正在接收（`RTS_RECEIVING` 置位且 `RTS_SENDING` 未置位）且 `CANRECEIVE` 为真。

`CANRECEIVE` 条件：接收方指定的源匹配（`receive_e == ANY || receive_e == src_e`）且 IPC 过滤器允许（若设置了过滤器）。

### 6.5 IPC 错误码完整列表

| 错误码 | 含义 |
|--------|------|
| `EDEADSRCDST` | 无效的源/目标 endpoint |
| `ECALLDENIED` | IPC 权限被拒绝（s_ipc_to 位图不允许） |
| `ETRAPDENIED` | IPC 陷阱被拒绝（s_trap_mask 不允许） |
| `ELOCKED` | 死锁检测发现循环依赖 |
| `ENOTREADY` | 非阻塞操作目标未就绪 |
| `EBADCALL` | 非法 IPC 调用号 |
| `EFAULT` | 消息复制失败（用户空间地址无效） |

### 6.6 死锁检测两进程特例

两进程之间的 SEND+RECEIVE 不是死锁，而是正常的请求-回复模式。`deadlock()` 函数通过 `(xp->p_rts_flags ^ (function << 2)) & RTS_SENDING` 表达式判断：利用 `RTS_SENDING = 0x04 = 1 << 2` 的特性，将 function（SEND=1 或 RECEIVE=2）左移 2 位后与目标进程的 RTS_SENDING 位做异或，若结果非零则表示一个是发送一个是接收，不是死锁。

### 6.7 IPC 过滤器（s_ipcf）

`CANRECEIVE` 宏中检查 `s_ipcf`：若进程设置了 IPC 过滤器，则消息必须通过过滤器才能被接收。过滤器可以按 `m_source` 和 `m_type` 匹配，支持白名单和黑名单模式。这是 Minix3 安全模型的一部分，限制进程能接收的消息范围。

### 6.8 内核任务通信限制

内核任务（IDLE、CLOCK、SYSTEM 等）只能通过 SENDREC 与其他进程通信，不能单独使用 SEND 或 RECEIVE。原因：

1. 内核任务总是回复消息，单独 SEND 会导致任务无法回复
2. 内核任务不能阻塞在发送上——如果调用者只 SEND 不 RECEIVE，任务会永远阻塞
3. `do_sync_ipc()` 中显式检查：`call_nr != SENDREC && call_nr != RECEIVE && iskerneln(src_dst_p)` 时返回 `ETRAPDENIED`

### 6.9 延迟消息投递的设计原因

消息不是在 IPC 调用时直接写入接收方的用户空间地址，而是先存入 `p_delivermsg`，设置 `MF_DELIVERMSG`，由 `switch_to_user()` 中的 `delivermsg()` 在进程恢复执行前完成实际复制。原因：

1. **简化内核路径**：IPC 函数只需设置标志，无需处理用户空间地址映射可能触发的页缺失
2. **统一投递点**：所有消息（同步/异步/通知）都通过同一个 `delivermsg()` 投递
3. **页缺失处理**：若投递时触发页缺失，可以挂起操作请求 VM 处理，完成后恢复

---

## 7. 参见

- [11-scheduling-primitives](11-scheduling-primitives.md) — RTS_SENDING/RECEIVING 对调度的影响
- [10-switch-to-user](10-switch-to-user.md) — delivermsg 在 misc 标志处理中调用
- [13-syscall-dispatch](13-syscall-dispatch.md) — do_ipc 的分派路径
- [14-exception-interrupt](14-exception-interrupt.md) — 中断通过 mini_notify 唤醒进程
