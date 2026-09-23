# SENDREC 的原子性问题：代码验证与设计分析

## 背景注释

> 以下注释来自 Minix3 内核源码 `ipc.h`：
>
> ```c
> /*
>  * XXX: the following check is used to set the status code only on RECEIVE.
>  * SENDREC is not currently atomic for user processes. A process can return
>  * from SENDREC in a different context than the original when a Posix signal
>  * handler gets executed. For this reason, it is not safe to manipulate
>  * the context (i.e. registers) when a process is blocked on a SENDREC.
>  * Unfortunately, avoiding setting the status code for SENDREC doesn't solve
>  * the problem entirely because in rare situations it is still necessary to
>  * override retreg dynamically (and possibly in a different context).
>  * A possible reliable solution is to improve our Posix signal handling
>  * implementation and guarantee SENDREC atomicity w.r.t. the process context.
>  */
> ```
>
> **翻译**：
> 
> > XXX：以下检查用于仅在 RECEIVE 时设置状态码。
> > SENDREC 对于用户进程目前不是原子的。当 Posix 信号处理程序执行时，
> > 进程可能在一个与原始调用不同的上下文中从 SENDREC 返回。
> > 因此，在进程被阻塞在 SENDREC 上时，操作上下文（即寄存器）是不安全的。
> > 不幸的是，避免为 SENDREC 设置状态码并不能完全解决问题，因为在某些
> > 罕见情况下，仍然需要动态覆盖返回寄存器（可能在一个不同的上下文中）。
> > 一个可能的可靠解决方案是改进我们的 Posix 信号处理实现，保证 SENDREC
> > 相对于进程上下文的原子性。

---

## 一、核心问题：两个"原子性"概念

这段注释涉及两个容易混淆的概念：

| 概念 | 含义 | 是否保证 |
|------|------|----------|
| **IPC 语义原子性** | 消息传递过程不可被其他 IPC 打断 | ✅ 保证 |
| **进程上下文原子性** | 执行上下文（寄存器）在调用期间不变 | ❌ 不保证 |

**注释警告的是第二个**，而非第一个。

---

## 二、IPC 语义原子性：代码验证

### 2.1 SENDREC 的实现流程

```c
// proc.c:569-582
case SENDREC:
    /* A flag is set so that notifications cannot interrupt SENDREC. */
    caller_ptr->p_misc_flags |= MF_REPLY_PEND;
    /* fall through */
case SEND:			
    result = mini_send(caller_ptr, src_dst_e, m_ptr, 0);
    if (call_nr == SEND || result != OK)
        break;				/* done, or SEND failed */
    /* fall through for SENDREC */
case RECEIVE:			
    if (call_nr == RECEIVE) {
        caller_ptr->p_misc_flags &= ~MF_REPLY_PEND;
        IPC_STATUS_CLEAR(caller_ptr);
    }
    result = mini_receive(caller_ptr, src_dst_e, m_ptr, 0);
    break;
```

### 2.2 关键发现

#### 发现 1：`MF_REPLY_PEND` 禁止通知中断

```c
// proc.c:1002-1004
/* Check if there are pending notifications, except for SENDREC. */
if (! (caller_ptr->p_misc_flags & MF_REPLY_PEND)) {
    // 只有非 SENDREC 才处理通知
}
```

**结论**：SENDREC 期间，通知消息被跳过。

#### 发现 2：接收只检查发送者 endpoint

```c
// ipc.h:21-23
#define CANRECEIVE(receive_e,src_e,dst_ptr,m_src_v,m_src_p) \
    (((receive_e) == ANY || (receive_e) == (src_e)) && \
    (priv(dst_ptr)->s_ipcf == NULL || \
    allow_ipc_filtered_msg(dst_ptr,src_e,m_src_v,m_src_p)))
```

**结论**：只检查发送者 endpoint 是否匹配，不检查消息类型。

#### 发现 3：没有 REPLY 标志

搜索整个内核代码，没有发现 `REPLY_FLAG` 或类似定义。

**结论**：SENDREC 不区分"回复"和"普通消息"，只区分发送者。

### 2.3 IPC 语义原子性的保证

```
A sendrec(B, &msg)
    │
    ├─► 发送阶段：消息投递给 B
    │
    ├─► 接收阶段：只接收来自 B 的消息
    │
    └─► 返回

中间不会被其他 IPC 打断，因为：
1. MF_REPLY_PEND 禁止通知
2. p_getfrom_e 锁定接收源
```

**但是**：这只保证 IPC 语义，不保证进程上下文。

---

## 三、进程上下文原子性：不保证

**⚠️ 重要：单核也能发生！**

这不是 SMP 特有的问题。关键在于：
1. **消息传递是延迟的**：B 发送回复时，只设置 `MF_DELIVERMSG` 标志
2. **返回值设置发生在进程切换时**：在 `switch_to_user()` 中调用 `delivermsg()`
3. **信号处理函数可能在消息投递之前执行**

---

### 3.1 完整时间线

```
T1: A 进程执行 sendrec(B)
    └─► 内核：设置 MF_REPLY_PEND, RTS_RECEIVING
    └─► 内核：A 阻塞，切换到 B 进程

T2: B 进程执行
    └─► B 处理请求...

T3: B 进程执行 send(A, reply)
    └─► 内核：设置 A 的 p_delivermsg
    └─► 内核：设置 MF_DELIVERMSG 标志
    └─► 内核：RTS_UNSET(A, RTS_RECEIVING)
    └─► **注意：此时还没有设置 p_reg.retreg！**

T4: 信号到达 A 进程
    └─► PM 设置信号处理函数 (SYS_SIGSEND)
        ① 保存原始 p_reg 到用户栈 (sigcontext)
        ② 修改 p_reg 为信号处理函数入口

T5: 调度器选择 A 进程运行
    └─► switch_to_user()
    └─► 检查 MF_DELIVERMSG 标志
    └─► 调用 delivermsg(A)
        └─► copy_msg_to_user() 拷贝消息
        └─► ⚠️ 设置 p_reg.retreg = OK
            此时 p_reg 是信号处理函数的上下文！

T6: A 执行信号处理函数
    └─► 使用 p_reg（信号处理函数的上下文）

T7: sigreturn (SYS_SIGRETURN)
    └─► 从用户栈恢复原始 p_reg
    └─► ⚠️ 覆盖 T5 设置的返回值！
```

---

### 3.2 问题的本质

**不是"上下文被修改"，而是"返回值被覆盖"！**

```
┌─────────────────────────────────────────────────────────────┐
│                    用户栈 (sigcontext)                       │
├─────────────────────────────────────────────────────────────┤
│  T4 保存的"快照"：                                           │
│  ├─ sc_eax = 原始 retreg（sendrec 调用前的值）               │
│  ├─ sc_eip = 原始 pc（sendrec 调用点）                       │
│  └─ ... 其他寄存器                                          │
│                                                             │
│  ⚠️ 这个快照在 T4 之后不会更新！                             │
│     T5 设置的返回值不会反映到这里！                          │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    内核 p_reg                                │
├─────────────────────────────────────────────────────────────┤
│  T4-T6: 信号处理函数的上下文                                 │
│  T5: delivermsg 设置 p_reg.retreg = OK                      │
│  T7: sigreturn 用用户栈快照覆盖 p_reg                        │
│      ⚠️ T5 的设置丢失！返回值错误！                         │
└─────────────────────────────────────────────────────────────┘
```

---

### 3.3 关键代码验证

**谁修改了 p_reg？**

```c
// T3: B 发送回复 - proc.c:908-911
// 这是内核代 B 进程执行的代码
dst_ptr->p_delivermsg.m_source = caller_ptr->p_endpoint;
dst_ptr->p_misc_flags |= MF_DELIVERMSG;  // 设置标志，延迟设置返回值

// T5: 进程切换时设置返回值 - proc.c:283-291
// 这是内核代 A 进程执行的代码
static void delivermsg(struct proc *rp)
{
    // ... 拷贝消息到用户空间 ...
    
    if(!(rp->p_misc_flags & MF_CONTEXT_SET)) {
        rp->p_reg.retreg = OK;  // ⚠️ 设置返回值！
    }
}

// T5: 进程切换时调用 - proc.c:360-365
void switch_to_user(void)
{
    // ...
    while (p->p_misc_flags & MF_DELIVERMSG) {
        delivermsg(p);  // ⚠️ 在这里设置返回值！
    }
    // ...
}

// T7: sigreturn 恢复原始上下文 - do_sigreturn.c:45-55
rp->p_reg.retreg = sc.sc_eax;  // 从用户栈恢复
// ⚠️ 覆盖了 delivermsg 设置的返回值！
```

**执行者总结**：

| 阶段 | 代码位置 | 执行者 | 修改内容 |
|------|----------|--------|----------|
| T3: B 发送回复 | `proc.c:908` | 内核（代 B 进程） | 设置 `p_delivermsg`，设置 `MF_DELIVERMSG` |
| T5: 进程切换 | `proc.c:289` | 内核（代 A 进程） | 设置 `p_reg.retreg = OK` |
| T7: sigreturn | `do_sigreturn.c:45` | 内核（代 A 进程） | 恢复 `p_reg.retreg`，覆盖 T5 的设置 |

---

### 3.4 受影响的寄存器

```c
// i386 架构
#define IPC_STATUS_REG    bx    // ebx 寄存器
#define retreg            ax    // eax 寄存器（系统调用返回值）
```

| 寄存器 | 用途 | 是否受影响 | 原因 |
|--------|------|------------|------|
| `retreg` (eax) | 系统调用返回值 | ✅ **受影响** | `delivermsg()` 设置，`sigreturn` 覆盖 |
| `IPC_STATUS_REG` (ebx) | IPC 状态码 | ✅ **受影响** | `IPC_STATUS_ADD()` 设置，`sigreturn` 覆盖 |
| `pc` (eip) | 程序计数器 | ❌ 不受影响 | `sigreturn` 恢复正确的返回地址 |
| `sp` (esp) | 栈指针 | ❌ 不受影响 | `sigreturn` 恢复正确的栈 |

**真正的问题**：`retreg`（返回值）丢失！这会导致 sendrec 返回错误的结果。

---

### 3.5 RTS_SIG_PENDING 的作用

```c
// system.c:444 - 信号触发时设置
RTS_SET(rp, RTS_SIGNALED | RTS_SIG_PENDING);

// proc.h:147 - 含义
#define RTS_SIG_PENDING	0x20	/* unready while signal being processed */

// proc.h:169-170 - 只有 p_rts_flags == 0 才可运行
#define rts_f_is_runnable(flg)	((flg) == 0)
```

| 阶段 | RTS_SIG_PENDING | 进程状态 |
|------|-----------------|----------|
| 信号触发 | 设置 | 不可调度 |
| PM 获取信号 | 保持 | 不可调度 |
| PM 设置处理函数 | 保持 | 不可调度 |
| PM 完成设置 | 清除 | 可调度 |

**关键点**：用户态信号处理函数执行时，`RTS_SIG_PENDING` 已经被清除，进程可以被调度！

---

### 3.6 解决方案

```c
// 当前方案：在 SENDREC 期间不修改 p_reg
if (!(p->p_misc_flags & MF_REPLY_PEND)) {
    p->p_reg.IPC_STATUS_REG |= m;
}

// 更好的方案：将 IPC 状态保存在内核
struct ipc_state {
    int return_value;      // 保存在内核，不会被 sigreturn 覆盖
    endpoint_t reply_from; // 回复来源
    // ...
};
```

---

## 四、与 L4 的对比

### 4.1 L4 的 IPC 设计

L4 只有一个 syscall：

```
IPC(dest, send_msg, recv_from)
```

**关键区别**：

| 特性 | MINIX | L4 |
|------|-------|-----|
| IPC 原语 | SEND, RECEIVE, SENDREC | 单一 IPC |
| 执行方式 | 阻塞 + 调度器 | 原子上下文切换 |
| 调度器参与 | 是 | 否（direct switch） |
| 死锁检测 | 需要 | 不需要 |
| 信号安全 | 不保证 | 天然保证 |

### 4.2 L4 的优势

```
L4 IPC 流程：

A call B
    │
    ├─► trap kernel
    ├─► copy message
    ├─► switch to B (direct, no scheduler)
    │
B run
    │
    ├─► process request
    ├─► reply
    │
    └─► switch to A (direct)

全程原子，信号无法插入！
```

### 4.3 MINIX 的局限

```
MINIX IPC 流程：

A sendrec B
    │
    ├─► trap kernel
    ├─► mini_send
    ├─► A block
    ├─► scheduler run  ← 信号可能在这里插入！
    ├─► B run
    │
B process
    │
    ├─► reply
    ├─► A wakeup
    │
    └─► A resume

中间存在调度器介入，信号可以插入！
```

### 4.4 SENDREC 与网络 RPC（如 Thrift）的对照

本篇 §5.1 把改进方向命名为"原子 RPC"，容易让人追问：SENDREC 和 RPC 到底是什么关系？一句话——**SENDREC 就是微内核版的同步 RPC 调用**，两者是同一个"请求-回复"语义，只是实现栈完全不同。

Thrift 那类网络 RPC 走"序列化 → 传输 → 反序列化"三段：把调用参数编码成字节流、经 TCP 送到对端、再解码回参数；返回时反向再来一遍。之所以需要编解码和传输层，是因为两端异构、跨网络（IPC 为什么没有这两层的通用解释见 `fork-syscall-rewrite/01-stage-kernel/12-ipc-core.md` §1.1）。

SENDREC 做同样的"请求-回复"，却省掉了这两段：

| RPC 栈的一层 | Thrift（网络） | Minix3 SENDREC（同机） |
|---|---|---|
| 序列化 / 编码 | 参数编码成字节流 | 无——两端共用同一 `message` 结构体定义（`minix3/minix/include/minix/ipc.h:noxfer_message`） |
| 传输 | TCP 分包 / 重传 | 内核把定长消息从发送方槽位拷到接收方槽位，再调度切换（即 §4.3 流程图里的 copy message 一步） |
| 反序列化 / 解码 | 对端解码回参数 | 无——接收方按 `m_type` 读回同一字段 |

也就是说，SENDREC 相对 RPC **省掉的正是编码层和传输层**，只留下"内核中转 + 一次内存拷贝"。代价是它只能在同一台机器、同一内核下工作，换不来 RPC 的跨网络能力。而本篇真正讨论的"原子性"问题——§4.3 里调度器给信号留了一个可插入的窗口——恰恰是这条同机路径特有的：网络 RPC 两端本就异步隔离，不存在"信号在同一进程的 send 与 receive 之间插入上下文"这回事。

---

## 五、设计改进方向

### 5.1 原子 RPC

将 SEND + RECEIVE 合并为真正的原子操作：

```rust
pub fn sendrec_atomic(
    dest: Endpoint,
    msg: &mut Message,
) -> Result<(), IpcError> {
    // 1. 保存当前上下文到内核栈
    // 2. 直接切换到目标进程
    // 3. 目标进程处理并回复
    // 4. 直接切换回来
    // 5. 恢复上下文
}
```

### 5.2 Direct Process Switch

跳过调度器：

```
A send B
    │
    └─► kernel switch directly to B
        (no scheduler involved)
```

### 5.3 Reply Capability

限制回复目标：

```rust
pub struct ReplyCapability {
    target: Endpoint,
    request_id: RequestId,
}

impl ReplyCapability {
    pub fn reply(self, msg: Message) -> Result<(), Error> {
        // 只能回复给特定的目标
        // 编译期保证安全性
    }
}
```

### 5.4 Signal-Safe IPC

将 IPC 状态保存在内核：

```rust
pub struct IpcState {
    phase: IpcPhase,
    saved_context: ProcessContext,
    request_id: RequestId,
}

enum IpcPhase {
    Sending,
    Receiving,
    Completed,
}
```

信号处理函数执行时，IPC 状态被保存在内核；返回后，恢复 IPC 状态。

---

## 六、要点总结

1. **IPC 语义原子性**：SENDREC 在消息传递层面是原子的
2. **上下文原子性**：SENDREC 在进程上下文层面不是原子的
3. **单核也能发生**：因为 `delivermsg()` 和信号处理函数顺序执行
4. **问题根源**：`sigcontext` 保存在用户栈，`sigreturn` 会覆盖内核设置的返回值
5. **L4 对比**：L4 的单一 IPC 原语避免了这个问题
6. **改进方向**：原子 RPC、Direct Switch、Reply Capability

---

## 七、灾难预演

### 场景：返回值丢失

```
1. A 调用 sendrec(B, &msg)
2. B 发送回复，设置 MF_DELIVERMSG
3. 信号到达，保存原始 p_reg 到用户栈
4. 调度器选择 A 运行
5. delivermsg() 设置 p_reg.retreg = OK
   ⚠️ 此时 p_reg 是信号处理函数的上下文！
6. A 执行信号处理函数
7. sigreturn 从用户栈恢复原始 p_reg
   ⚠️ 覆盖了步骤 5 设置的返回值！
8. A 从 sendrec 返回，但返回值错误！
```

---

## 八、互动自测

**问题 1**：为什么单核也能发生这个问题？

**答案**：
- `delivermsg()` 在进程切换时设置返回值
- 信号处理函数可能在 `delivermsg()` 之后执行
- `sigreturn` 恢复原始上下文，覆盖 `delivermsg()` 设置的返回值
- 不是同时执行，而是顺序执行，但状态被覆盖

**问题 2**：为什么 L4 不需要死锁检测？

**答案**：
- L4 IPC 是原子的上下文切换
- 调用链形成自然的调用栈
- 只要最终有进程返回，整个链就会解开
- 不会形成 MINIX 那样的"阻塞环"

**问题 3**：如何在 Rust 中避免这个问题？

**答案提示**：
- 使用类型状态模式区分 IPC 阶段
- 将 IPC 状态保存在内核，而非用户寄存器
- 使用 RAII 确保状态正确恢复
