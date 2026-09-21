# Minix 3 内核不变量 (Invariants) 与高危路径深度分析

> 本文档用于 minix-rs 项目重构，识别 C 语言中隐含的高风险假设，指导 Rust 实现中的安全性设计。

---

## 一、进程状态不变量

### 1. 就绪队列排他性不变量 (Ready Queue Exclusivity)

**逻辑定义**：

```
进程可运行 ⟺ p.p_rts_flags == 0
入队时必须满足可运行条件，出队时必须满足不可运行条件
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `proc.h:168` | `#define proc_is_runnable(p) (rts_f_is_runnable((p)->p_rts_flags))` | 可运行定义为 flags == 0 |
| `proc.h:200-209` | `RTS_SET` 宏 | 设置标志时自动调用 `dequeue` |
| `proc.h:212-220` | `RTS_UNSET` 宏 | 清除标志时自动调用 `enqueue` |
| `proc.c:1595` | `enqueue()` | 断言 `proc_is_runnable(rp)` |
| `proc.c:1716` | `dequeue()` | 断言 `!proc_is_runnable(rp)` |

**高危区域与破坏路径**：

**最危险函数**：
- `RTS_SET` / `RTS_UNSET` - 宏展开，隐式调用 `enqueue`/`dequeue`
- `enqueue()` / `dequeue()` - 直接操作队列
- `pick_proc()` - 从队列选择进程

**易破坏路径**：

```
路径 1：直接修改 p_rts_flags 绕过宏
┌─────────────────────────────────────────────────────────────┐
│ 错误代码示例：                                               │
│   rp->p_rts_flags |= RTS_SENDING;  // 直接修改，绕过 RTS_SET │
│   // 后果：进程仍在就绪队列中，但标志表示阻塞！              │
│   // 调度器可能选择这个"阻塞"进程运行                        │
└─────────────────────────────────────────────────────────────┘

路径 2：中断打断 RTS_SET/UNSET
┌─────────────────────────────────────────────────────────────┐
│ T1: RTS_SET 开始执行                                        │
│     rts = rp->p_rts_flags;  // 读取旧值 (假设为 0)           │
│                                                              │
│ T2: 中断发生，CPU 切换到其他进程                             │
│                                                              │
│ T3: 其他代码修改了 rp->p_rts_flags                          │
│                                                              │
│ T4: 中断返回，继续执行                                       │
│     rp->p_rts_flags |= f;   // 基于旧值修改，丢失 T3 的修改   │
│     if(rts_f_is_runnable(rts) && !proc_is_runnable(rp))     │
│         dequeue(rp);        // 可能错误地出队                │
└─────────────────────────────────────────────────────────────┘

路径 3：SMP 跨 CPU 操作
┌─────────────────────────────────────────────────────────────┐
│ CPU 0                          CPU 1                        │
│ ┌─────────────────────┐       ┌─────────────────────┐      │
│ │ RTS_SET(p, flag1)   │       │ RTS_SET(p, flag2)   │      │
│ │ 读取 rts = 0        │       │ 读取 rts = 0        │      │
│ │ 设置 flags |= flag1 │       │ 设置 flags |= flag2 │      │
│ │ dequeue(p)          │       │ dequeue(p) ← 再次出队!│     │
│ └─────────────────────┘       └─────────────────────┘      │
│                                                              │
│ 后果：p 被出队两次，队列状态损坏                             │
└─────────────────────────────────────────────────────────────┘
```

**脆弱时间窗**：

```
时间线：
┌──────────────────────────────────────────────────────────────┐
│ RTS_SET(rp, f) 执行过程：                                     │
│                                                              │
│   rts = rp->p_rts_flags;     ← 读取                          │
│   ════════════════════════════ 脆弱窗口开始 ═════════════════│
│   rp->p_rts_flags |= f;      ← 修改                          │
│   if(rts_f_is_runnable(rts) && !proc_is_runnable(rp))        │
│       dequeue(rp);           ← 出队                          │
│   ════════════════════════════ 脆弱窗口结束 ═════════════════│
│                                                              │
│ 在脆弱窗口内：                                               │
│ - p_rts_flags 已修改，但进程可能仍在队列中                    │
│ - 另一个 CPU 可能选择这个"阻塞"进程运行                       │
│ - 中断处理程序可能看到不一致的状态                            │
└──────────────────────────────────────────────────────────────┘
```

**SMP 与并发挑战**：

```
问题：p_rts_flags 是 volatile u32_t，但操作不是原子的

#define RTS_SET(rp, f)                          \
    do {                                        \
        const int rts = (rp)->p_rts_flags;      \  // 非原子读取
        (rp)->p_rts_flags |= (f);               \  // 非原子修改
        if(rts_f_is_runnable(rts) && !proc_is_runnable(rp)) { \
            dequeue(rp);                        \  // 可能基于过时的 rts 值
        }                                       \
    } while(0)

竞态条件：
1. 读取和修改之间，其他 CPU 可能修改 flags
2. dequeue 判断基于旧值，可能做出错误决策
3. 多个 CPU 可能同时 dequeue 同一进程

说明：
- 代码中存在 BKL，但本文不将其作为 RTS_SET/UNSET 的显式不变量保护
```

**Rust 重构指导**：

```rust
// 方案 1：类型状态模式 - 编译期保证
enum NotInQueue {}
enum InQueue { queue_id: u32, priority: u8 }

struct Process<State> {
    p_rts_flags: u32,
    _state: PhantomData<State>,
}

impl Process<InQueue> {
    // 只有在队列中的进程才能出队
    fn dequeue(self) -> Process<NotInQueue> {
        Process {
            p_rts_flags: self.p_rts_flags | BLOCKING_FLAG,
            _state: PhantomData,
        }
    }
}

impl Process<NotInQueue> {
    // 只有不在队列中的进程才能入队
    fn enqueue(self) -> Process<InQueue> {
        Process {
            p_rts_flags: self.p_rts_flags & !BLOCKING_FLAG,
            _state: PhantomData,
        }
    }
}

// 方案 2：运行时检查 + 原子操作
struct RuntimeFlags {
    flags: AtomicU32,
    in_queue: AtomicBool,  // 显式跟踪队列状态
}

impl RuntimeFlags {
    fn set_blocking(&self, flag: u32) -> Result<(), InvariantError> {
        // 原子操作，避免竞态
        let old = self.flags.fetch_or(flag, Ordering::SeqCst);
        
        if old == 0 && !self.in_queue.swap(false, Ordering::SeqCst) {
            // 原来在队列中，现在应该出队
            // 但 in_queue 已经是 false，说明状态不一致
            return Err(InvariantError::QueueStateMismatch);
        }
        Ok(())
    }
}

// 方案 3：使用 Rust 的类型系统编码不变量
struct RunnableProcess {
    inner: Process,
    queue_token: QueueToken,  // 证明进程在队列中
}

struct BlockedProcess {
    inner: Process,
    block_reason: BlockReason,  // 阻塞原因
}

// 类型系统保证：RunnableProcess 一定在队列中
// BlockedProcess 一定不在队列中
// 两者之间的转换是显式的，不会遗漏队列操作
```

---

### 2. 进程状态标志互斥性不变量

**逻辑定义**：

```
某些状态标志不能同时设置：
- RTS_SENDING 和 RTS_RECEIVING 可以同时设置（SENDREC 场景）
- RTS_SLOT_FREE 与其他所有标志互斥
- RTS_NO_ENDPOINT 设置后，进程不能参与 IPC
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `proc.h:141-166` | 标志定义 | 各标志位定义 |
| `proc.h:274` | `isemptyp(p)` | 检查是否为空槽 |
| `proc.c:887-890` | `RTS_NO_ENDPOINT` 检查 | 禁止与已死进程通信 |
| `proc.c:746-750` | `RTS_SENDING` 检查 | 死锁检测中的特殊处理 |

**高危区域与破坏路径**：

**最危险函数**：
- `do_clear` - 清理进程时设置 `RTS_SLOT_FREE`
- `do_fork` - 创建进程时设置初始状态
- `mini_send` / `mini_receive` - 修改 IPC 相关标志

**易破坏路径**：

```
路径 1：进程清理不完整
┌─────────────────────────────────────────────────────────────┐
│ do_clear() 清理进程：                                        │
│   RTS_SETFLAGS(rc, RTS_SLOT_FREE);  // 设置为空槽           │
│                                                              │
│ 但如果进程还在：                                             │
│ - 发送队列中（其他进程等待它）                               │
│ - 就绪队列中（调度器可能选择它）                             │
│ - 持有内核资源（锁、内存等）                                 │
│                                                              │
│ 后果：资源泄漏，其他进程永远阻塞                             │
└─────────────────────────────────────────────────────────────┘

路径 2：SENDREC 状态混乱
┌─────────────────────────────────────────────────────────────┐
│ SENDREC 系统调用：                                           │
│ 1. 设置 RTS_SENDING                                         │
│ 2. 发送成功后设置 MF_REPLY_PEND                              │
│ 3. 然后设置 RTS_RECEIVING                                   │
│                                                              │
│ 如果在步骤 2 被中断：                                        │
│ - RTS_SENDING 仍设置                                        │
│ - MF_REPLY_PEND 设置                                        │
│ - RTS_RECEIVING 未设置                                      │
│                                                              │
│ 进程状态不一致，可能永远阻塞                                 │
└─────────────────────────────────────────────────────────────┘
```

**脆弱时间窗**：

```
SENDREC 执行过程：
┌──────────────────────────────────────────────────────────────┐
│ mini_send:                                                   │
│   RTS_SET(caller, RTS_SENDING);  ← 设置发送标志              │
│   ════════════════════════════ 脆弱窗口 1 ═══════════════════│
│   // 如果发送阻塞，进程在此等待                              │
│   // 如果发送成功，继续                                      │
│                                                              │
│ do_ipc:                                                      │
│   caller->p_misc_flags |= MF_REPLY_PEND;  ← 设置回复待处理   │
│   ════════════════════════════ 脆弱窗口 2 ═══════════════════│
│                                                              │
│ mini_receive:                                                │
│   RTS_SET(caller, RTS_RECEIVING);  ← 设置接收标志            │
│   // 现在进程同时有 SENDING 和 RECEIVING                     │
│   // 这是合法的，但必须在正确的顺序下                        │
└──────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 使用枚举明确表示合法状态组合
enum ProcessIpcState {
    Idle,
    Sending {
        target: Endpoint,
        message: Message,
    },
    Receiving {
        source: Endpoint,  // 可以是 ANY
    },
    SendReceive {
        target: Endpoint,
        message: Message,
        send_complete: bool,
    },
}

// 类型系统保证状态转换合法
impl Process {
    fn start_send(&mut self, target: Endpoint, msg: Message) -> Result<(), Error> {
        match self.ipc_state {
            ProcessIpcState::Idle => {
                self.ipc_state = ProcessIpcState::Sending { target, message: msg };
                Ok(())
            }
            _ => Err(Error::InvalidStateTransition),
        }
    }
    
    fn complete_send_start_receive(&mut self) -> Result<(), Error> {
        match &self.ipc_state {
            ProcessIpcState::Sending { target, message } => {
                let target = *target;
                let msg = message.clone();
                self.ipc_state = ProcessIpcState::SendReceive {
                    target,
                    message: msg,
                    send_complete: true,
                };
                Ok(())
            }
            _ => Err(Error::InvalidStateTransition),
        }
    }
}
```

---

### 3. 进程调度优先级不变量

**逻辑定义**：

```
1. 高优先级进程就绪时，应抢占低优先级进程
2. 同优先级进程按时间片轮转
3. 进程不能同时存在于多个优先级队列
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `proc.c:1630-1639` | 优先级检查和抢占 | 入队时检查是否需要抢占 |
| `proc.c:1670-1705` | `enqueue_head` | 抢占后放回队首 |
| `proc.c:1780-1810` | `pick_proc` | 从高到低扫描队列 |

**高危区域与破坏路径**：

**最危险函数**：
- `enqueue` - 入队时可能触发抢占
- `enqueue_head` - 抢占后放回队首
- `pick_proc` - 选择进程运行

**易破坏路径**：

```
路径 1：抢占逻辑错误
┌─────────────────────────────────────────────────────────────┐
│ enqueue() 中：                                               │
│   if((p->p_priority > rp->p_priority) &&                    │
│      (priv(p)->s_flags & PREEMPTIBLE))                      │
│       RTS_SET(p, RTS_PREEMPTED);  // 这会调用 dequeue!      │
│                                                              │
│ 问题：                                                       │
│ - 当前进程 p 被出队                                         │
│ - 但它可能正在运行!                                         │
│ - 需要确保正确的上下文保存                                   │
└─────────────────────────────────────────────────────────────┘

路径 2：优先级变化
┌─────────────────────────────────────────────────────────────┐
│ 如果进程在队列中时优先级被修改：                             │
│ - 进程在优先级 5 的队列中                                    │
│ - 优先级被改为 3                                            │
│ - 但进程仍在优先级 5 的队列中!                               │
│ - pick_proc 可能扫描不到，或扫描错误队列                     │
└─────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 优先级与队列的关联通过类型系统保证
struct RunQueue {
    queues: [VecDeque<ProcessToken>; NR_SCHED_QUEUES],
}

impl RunQueue {
    fn enqueue(&mut self, process: &mut Process, token: ProcessToken) {
        let prio = process.priority();
        // 类型系统保证：token 只能在一个队列中
        self.queues[prio].push_back(token);
    }
    
    fn change_priority(&mut self, process: &mut Process, new_prio: u8) -> Result<(), Error> {
        let old_prio = process.priority();
        if old_prio != new_prio {
            // 必须先出队再入队
            let token = self.queues[old_prio].remove(process.token())?;
            process.set_priority(new_prio);
            self.queues[new_prio].push_back(token);
        }
        Ok(())
    }
}
```

---

## 二、IPC 通讯不变量

### 4. 消息投递状态不变量

**逻辑定义**：

```
MF_DELIVERMSG 标志设置时：
- p_delivermsg 的内容必须有效（消息已准备好投递）
- p_delivermsg_vir 指向有效的用户态地址
- 进程即将返回用户态

该标志在消息投递完成（复制到用户态）后必须清除。
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `proc.c:641` | `panic("sys_call: MF_DELIVERMSG on for...")` | 入口检查 |
| `proc.c:898` | `assert(!(dst_ptr->p_misc_flags & MF_DELIVERMSG))` | 发送时断言 |
| `proc.c:909` | `dst_ptr->p_misc_flags \|= MF_DELIVERMSG` | 设置标志 |
| `proc.c:980` | `assert(!(caller_ptr->p_misc_flags & MF_DELIVERMSG))` | 接收时断言 |
| `proc.c:1031` | `caller_ptr->p_misc_flags \|= MF_DELIVERMSG` | 设置标志 |
| `proc.h:243` | `MF_DELIVERMSG` 定义 | 标志定义 |

**高危区域与破坏路径**：

**最危险函数**：
- `mini_receive` - 设置投递状态
- `mini_send` - 直接投递消息
- `restore_user_context` - 返回用户态时处理投递

**易破坏路径**：

```
路径 1：重复设置 MF_DELIVERMSG
┌─────────────────────────────────────────────────────────────┐
│ 如果进程已有 MF_DELIVERMSG，又收到新消息：                   │
│                                                              │
│ assert(!(caller_ptr->p_misc_flags & MF_DELIVERMSG));        │
│ // 断言失败！                                                │
│                                                              │
│ 正常流程下不应该发生，但 SMP 环境可能触发：                   │
│ - CPU 0 正在处理消息 A，设置了 MF_DELIVERMSG                 │
│ - CPU 1 同时投递消息 B                                       │
│ - 两个消息冲突                                               │
└─────────────────────────────────────────────────────────────┘

路径 2：投递未完成
┌─────────────────────────────────────────────────────────────┐
│ 设置 MF_DELIVERMSG 后，进程被信号中断：                       │
│                                                              │
│ 1. 设置 MF_DELIVERMSG                                       │
│ 2. 准备返回用户态                                            │
│ 3. 检测到信号，执行信号处理程序                               │
│ 4. 信号处理程序修改了进程状态                                 │
│ 5. MF_DELIVERMSG 仍设置，但 p_delivermsg 可能已过期         │
│                                                              │
│ 后果：返回时投递错误的消息                                   │
└─────────────────────────────────────────────────────────────┘
```

**脆弱时间窗**：

```
消息投递过程：
┌──────────────────────────────────────────────────────────────┐
│ 设置 MF_DELIVERMSG                                           │
│   ════════════════════════════ 脆弱窗口开始 ═════════════════│
│ 准备返回用户态                                               │
│ 复制消息到用户缓冲区                                         │
│ 清除 MF_DELIVERMSG                                           │
│   ════════════════════════════ 脆弱窗口结束 ═════════════════│
│ 返回用户态                                                   │
└──────────────────────────────────────────────────────────────┘

在脆弱窗口内：
- 进程可能在返回前被信号中断
- 或者被调度器切换到其他进程
- 如果此时其他代码修改 p_delivermsg，会导致数据损坏
```

**Rust 重构指导**：

```rust
// 使用 Option 明确表示消息投递状态
struct DeliverState {
    message: Option<Message>,
    user_buffer: Option<UserPtr<Message>>,
}

impl Process {
    fn prepare_deliver(&mut self, msg: Message, user_buf: UserPtr<Message>) -> Result<(), Error> {
        // 类型系统保证：message 是 Some 时，必须先处理完才能设置新消息
        if self.deliver_state.message.is_some() {
            return Err(Error::DeliverInProgress);
        }
        self.deliver_state.message = Some(msg);
        self.deliver_state.user_buffer = Some(user_buf);
        Ok(())
    }
    
    fn complete_deliver(&mut self) -> Result<(), Error> {
        // take() 确保 message 被消费，不会重复投递
        let msg = self.deliver_state.message.take()
            .ok_or(Error::NoMessageToDeliver)?;
        let user_buf = self.deliver_state.user_buffer.take()
            .ok_or(Error::NoUserBuffer)?;
        
        // 安全地复制到用户态
        unsafe { user_buf.write(msg)? }
        Ok(())
    }
}
```

---

### 5. IPC 消息完整性不变量

**逻辑定义**：

```
消息在传输过程中不能被损坏或丢失：
- 发送者的消息必须完整复制到接收者
- 消息内容在传输过程中不能被第三方修改
- 消息必须到达预期的接收者
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `proc.c:900-906` | `copy_msg_from_user` | 从用户态复制消息到 p_delivermsg |
| `proc.c:934-947` | 保存到 `p_sendmsg` | 发送阻塞时保存消息 |
| `proc.c:1055-1067` | 复制到 `p_delivermsg` | 投递时复制消息 |

**高危区域与破坏路径**：

**最危险函数**：
- `mini_send` - 处理消息发送
- `mini_receive` - 处理消息接收
- `copy_msg_from_user` - 跨地址空间复制

**易破坏路径**：

```
路径 1：用户态地址无效
┌─────────────────────────────────────────────────────────────┐
│ copy_msg_from_user(m_ptr, &dst_ptr->p_delivermsg)           │
│                                                              │
│ 如果 m_ptr 无效：                                            │
│ - 可能返回 EFAULT                                           │
│ - 但如果部分复制成功，p_delivermsg 可能包含垃圾数据          │
│                                                              │
│ 后果：接收者收到损坏的消息                                   │
└─────────────────────────────────────────────────────────────┘

路径 2：并发修改
┌─────────────────────────────────────────────────────────────┐
│ SMP 环境下：                                                 │
│                                                              │
│ CPU 0: 读取 sender->p_sendmsg 准备复制                      │
│ CPU 1: 同时修改 sender->p_sendmsg（例如取消发送）            │
│                                                              │
│ 后果：消息内容在复制过程中被修改                             │
└─────────────────────────────────────────────────────────────┘

路径 3：进程终止
┌─────────────────────────────────────────────────────────────┐
│ 发送者 A 发送消息给接收者 B：                                 │
│ 1. A 的消息保存到 p_sendmsg                                  │
│ 2. A 阻塞等待 B 接收                                         │
│ 3. A 被信号终止                                              │
│ 4. B 准备接收，尝试读取 A->p_sendmsg                         │
│ 5. 但 A 的进程槽可能已被重用!                                │
│                                                              │
│ 后果：B 读取到垃圾数据或崩溃                                 │
└─────────────────────────────────────────────────────────────┘
```

**脆弱时间窗**：

```
消息从发送者 p_sendmsg 复制到接收者 p_delivermsg：
┌──────────────────────────────────────────────────────────────┐
│ caller_ptr->p_delivermsg = sender->p_sendmsg;                │
│   ════════════════════════════ 脆弱窗口 ═════════════════════│
│                                                              │
│ 在这个赋值过程中：                                           │
│ - 发送者可能被取消，p_sendmsg 被清除                         │
│ - 发送者进程槽被重用                                         │
│ - SMP 环境下的并发访问                                       │
└──────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 使用所有权转移保证消息完整性
struct Message {
    data: [u8; 56],
}

// 消息发送消耗所有权
fn send(msg: Message, dst: &mut Process) -> Result<(), IpcError> {
    // msg 的所有权转移，发送者无法再访问
    // 保证消息在传输过程中不会被发送者修改
    dst.receive_message(msg);
    Ok(())
}

// 或者使用引用计数共享
struct SharedMessage {
    data: Arc<[u8; 56]>,
}

fn send_shared(msg: Arc<Message>, dst: &mut Process) -> Result<(), IpcError> {
    // 引用计数保证消息在所有引用消失前不会被释放
    dst.receive_message(SharedMessage { data: msg.clone() });
    Ok(())
}

// 对于跨地址空间复制，使用安全抽象
fn copy_from_user<'a>(
    user_ptr: UserPtr<Message>,
    kernel_buf: &'a mut MaybeUninit<Message>,
) -> Result<&'a mut Message, CopyError> {
    // 安全性由 UserPtr 类型保证：
    // - UserPtr 在创建时已验证地址有效性
    // - 类型系统保证对齐和大小正确
    // - 生命周期保证在复制期间用户内存有效
    
    unsafe {
        // 受控的 unsafe 块，安全性由类型系统保证
        kernel_buf.write(user_ptr.read()?);
        Ok(kernel_buf.assume_init_mut())
    }
}
```

---

### 6. 死锁检测回路判定不变量

**逻辑定义**：

```
deadlock() 只在等待链回到调用者 cp 时判定死锁：
- 只有 src_dst_e == cp->p_endpoint 时才进入死锁判定
- 两进程 SEND/RECEIVE 的组合被视为合法会合
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `proc.c:703-760` | `deadlock()` 函数 | 检测循环等待 |
| `proc.c:742-750` | 特殊情况处理 | SENDREC + RECEIVE 合法 |
| `proc.c:930-932` | 发送前检测 | 阻塞前检查死锁 |
| `proc.c:1102-1104` | 接收前检测 | 阻塞前检查死锁 |

**高危区域与破坏路径**：

**最危险函数**：
- `deadlock` - 死锁检测
- `mini_send` - 发送前检测
- `mini_receive` - 接收前检测

**易破坏路径**：

```
路径 1：增量检测的时序问题
┌─────────────────────────────────────────────────────────────┐
│ 死锁检测是增量的，只在进程即将阻塞时检测：                    │
│                                                              │
│ T1: A 发送给 B（B 没在等待）                                 │
│     A 阻塞，检测通过（无环）                                 │
│                                                              │
│ T2: B 发送给 C（C 没在等待）                                 │
│     B 阻塞，检测通过（无环）                                 │
│                                                              │
│ T3: C 发送给 A                                               │
│     C 阻塞，检测发现环 A→B→C→A                              │
│     返回 ELOCKED                                            │
│                                                              │
│ 但如果 T3 的检测在 T1/T2 完成前：                            │
│ - A 还没设置 RTS_SENDING                                    │
│ - 检测可能看不到完整的链                                     │
│ - 死锁未被发现!                                              │
└─────────────────────────────────────────────────────────────┘

路径 2：特殊情况判断错误
┌─────────────────────────────────────────────────────────────┐
│ 两进程 SENDREC + RECEIVE 的判断：                            │
│                                                              │
│ if ((xp->p_rts_flags ^ (function << 2)) & RTS_SENDING) {    │
│     return(0);  // 不是死锁                                  │
│ }                                                            │
│                                                              │
│ 这个位操作的含义：                                           │
│ - function = SEND (0) 或 RECEIVE (1)                        │
│ - function << 2 = 0 或 4 (RTS_SENDING)                      │
│ - 异或后检查 RTS_SENDING 位                                  │
│                                                              │
│ 如果判断错误：                                               │
│ - 合法的会合被误判为死锁 → 系统功能异常                      │
│ - 真正的死锁被漏检 → 系统永久阻塞                            │
└─────────────────────────────────────────────────────────────┘

路径 3：进程终止
┌─────────────────────────────────────────────────────────────┐
│ 进程在等待链中终止：                                         │
│                                                              │
│ A → B → C → D                                                │
│                                                              │
│ 如果 C 终止：                                                │
│ - B 的等待应该被取消                                         │
│ - 但如果清理不完整，B 可能永远等待 C                         │
│ - A 也永远等待 B                                            │
│                                                              │
│ 等待链断裂但相关进程未唤醒                                   │
└─────────────────────────────────────────────────────────────┘
```

**脆弱时间窗**：

```
deadlock 检测到设置 RTS_SENDING 之间：
┌──────────────────────────────────────────────────────────────┐
│ deadlock() 检测                                              │
│   ════════════════════════════ 脆弱窗口开始 ═════════════════│
│ 返回"无死锁"                                                 │
│ 设置 RTS_SENDING                                             │
│   ════════════════════════════ 脆弱窗口结束 ═════════════════│
│                                                              │
│ 在脆弱窗口内：                                               │
│ - 检测时进程链可能变化                                       │
│ - SMP 环境下其他 CPU 可能修改等待关系                        │
│ - 检测结果可能过时                                           │
└──────────────────────────────────────────────────────────────┘
```

**SMP 与并发挑战**：

```
多核环境下的死锁检测问题：

CPU 0                          CPU 1
┌─────────────────────┐       ┌─────────────────────┐
│ deadlock(A→B→C)     │       │ deadlock(X→Y→Z)     │
│ 检测通过            │       │ 检测通过            │
│ 设置 A RTS_SENDING  │       │ 设置 X RTS_SENDING  │
└─────────────────────┘       └─────────────────────┘

如果 A→B→C→X→Y→Z→A 形成环：
- 两个 CPU 分别检测时都看不到完整环
- 两个进程都阻塞
- 系统死锁!

解决方案：
- 需要在检测时持有 Big Kernel Lock
- 或者使用全局等待图数据结构
```

**Rust 重构指导**：

```rust
// 使用类型系统编码等待关系
struct WaitGraph {
    edges: HashMap<ProcessId, ProcessId>,  // 等待者 → 被等待者
}

impl WaitGraph {
    // 添加等待关系，返回是否形成环
    fn add_wait(&mut self, waiter: ProcessId, target: ProcessId) -> Result<bool, DeadlockError> {
        // 检查是否形成环
        if self.would_create_cycle(waiter, target) {
            // 检查是否是合法的两进程会合
            if self.is_valid_rendezvous(waiter, target) {
                return Ok(false);  // 不是死锁
            }
            return Err(DeadlockError);
        }
        
        self.edges.insert(waiter, target);
        Ok(false)
    }
    
    fn would_create_cycle(&self, waiter: ProcessId, target: ProcessId) -> bool {
        // 从 target 开始遍历，看是否能回到 waiter
        let mut current = target;
        while let Some(&next) = self.edges.get(&current) {
            if next == waiter {
                return true;  // 形成环
            }
            current = next;
        }
        false
    }
    
    fn is_valid_rendezvous(&self, waiter: ProcessId, target: ProcessId) -> bool {
        // 检查是否是两进程 SENDREC + RECEIVE
        // 这需要访问进程状态，可以用更精确的类型编码
        false
    }
}

// 进程移除时自动清理等待关系
impl Drop for Process {
    fn drop(&mut self) {
        // 从等待图中移除所有相关边
        WAIT_GRAPH.lock().unwrap().remove_process(self.id);
    }
}
```

---

## 三、中断与异常处理不变量

### 7. 内核栈完整性不变量

**逻辑定义**：

```
每个进程有独立的内核栈：
- 栈大小固定 (K_STACK_SIZE)
- 栈不能溢出
- 栈指针必须始终指向有效区域
- 栈底有保护字 (STACK_GUARD) 检测溢出
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `priv.h:49` | `s_stack_guard` | 栈保护字指针 |
| `priv.h:69` | `STACK_GUARD` 定义 | 保护字魔数 |
| `proc.c:1737` | 栈保护字检查 | `dequeue` 中检查 |

```c
// proc.c:1737
assert (!iskernelp(rp) || *priv(rp)->s_stack_guard == STACK_GUARD);
```

**高危区域与破坏路径**：

**最危险函数**：
- `dequeue` - 检查栈保护字
- 中断处理程序 - 使用内核栈
- `switch_k_stack` - 切换内核栈

**易破坏路径**：

```
路径 1：深度递归/嵌套
┌─────────────────────────────────────────────────────────────┐
│ 中断嵌套导致栈溢出：                                         │
│                                                              │
│ 用户态 → 系统调用 → 中断 → 另一个中断 → ...                  │
│                                                              │
│ 每层都使用栈空间：                                           │
│ - 保存寄存器 (~100 字节)                                     │
│ - 局部变量                                                   │
│ - 调用栈                                                     │
│                                                              │
│ 如果超过 K_STACK_SIZE：                                      │
│ - 栈保护字被覆盖                                             │
│ - 或者直接溢出到其他进程的栈                                  │
│ - 内核数据损坏                                               │
└─────────────────────────────────────────────────────────────┘

路径 2：大数组局部变量
┌─────────────────────────────────────────────────────────────┐
│ 函数中定义大数组：                                           │
│                                                              │
│ void some_function() {                                       │
│     char buffer[4096];  // 在内核栈上                        │
│     ...                                                      │
│ }                                                            │
│                                                              │
│ 如果多个函数调用链都有大数组：                                │
│ - 栈使用量快速增加                                           │
│ - 可能溢出                                                   │
└─────────────────────────────────────────────────────────────┘

路径 3：栈指针错误
┌─────────────────────────────────────────────────────────────┐
│ switch_k_stack 切换栈：                                      │
│                                                              │
│ 如果新栈指针无效：                                           │
│ - 后续函数调用会失败                                         │
│ - 无法返回                                                   │
│ - 系统崩溃                                                   │
└─────────────────────────────────────────────────────────────┘
```

**脆弱时间窗**：

```
中断处理过程中：
┌──────────────────────────────────────────────────────────────┐
│ 保存寄存器到栈                                               │
│   ════════════════════════════ 脆弱窗口开始 ═════════════════│
│ 执行中断处理程序                                             │
│ (栈使用量最大)                                               │
│ 恢复寄存器                                                   │
│   ════════════════════════════ 脆弱窗口结束 ═════════════════│
│ 返回                                                         │
└──────────────────────────────────────────────────────────────┘

在脆弱窗口内：
- 栈使用量最大
- 任何栈溢出都会破坏内核数据
- 难以恢复
```

**Rust 重构指导**：

```rust
// 使用类型系统保证栈安全
#[repr(C)]
struct KernelStack {
    data: [u8; K_STACK_SIZE],
    guard: StackGuard,
}

impl KernelStack {
    fn new() -> Self {
        Self {
            data: [0; K_STACK_SIZE],
            guard: StackGuard::MAGIC,
        }
    }
    
    fn check_guard(&self) -> Result<(), StackOverflow> {
        if self.guard != StackGuard::MAGIC {
            Err(StackOverflow)
        } else {
            Ok(())
        }
    }
}

// 使用编译期检查限制栈使用
#[inline(never)]  // 防止内联导致栈使用增加
fn interrupt_handler() {
    // 使用静态分配的缓冲区，而非栈上分配
    static mut BUFFER: [u8; 4096] = [0; 4096];
    
    // 或者使用栈帧大小属性
    #[cfg_attr(target_arch = "x86_64", stack_probes)]
    fn inner_handler() {
        // ...
    }
}

// 使用 Rust 的栈保护机制
// 编译选项: -Z stack-probes
// 运行时检查栈溢出，而非仅检查保护字
```

---

### 8. 中断上下文保存不变量

**逻辑定义**：

```
中断发生时，必须完整保存用户态上下文：
- 所有通用寄存器
- 段寄存器
- 标志寄存器
- 指令指针和栈指针

返回用户态时，必须完整恢复上下文。
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `arch/i386/mpx.S` | 中断入口/出口汇编 | 保存/恢复寄存器 |
| `arch/i386/arch_system.c:566` | `restore_user_context()` | 恢复用户上下文 |
| `arch/i386/arch_system.c:588-604` | 调用恢复函数 | 根据入口类型选择恢复方式 |

**高危区域与破坏路径**：

**最危险函数**：
- `restore_user_context` - 恢复用户上下文
- 中断入口/出口汇编代码
- `save_user_context` - 保存用户上下文

**易破坏路径**：

```
路径 1：上下文结构不匹配
┌─────────────────────────────────────────────────────────────┐
│ 如果 C 代码中的栈帧结构被修改：                               │
│                                                              │
│ struct stackframe {                                          │
│     // 如果字段顺序或大小改变                                 │
│     // 但汇编代码未同步更新                                   │
│     // 寄存器会保存/恢复到错误位置                            │
│ };                                                           │
│                                                              │
│ 后果：返回用户态时寄存器值错误，进程崩溃                      │
└─────────────────────────────────────────────────────────────┘

路径 2：信号中断
┌─────────────────────────────────────────────────────────────┐
│ 进程在内核态被信号中断：                                      │
│                                                              │
│ 1. 进程 A 在内核态执行系统调用                                │
│ 2. 信号到达，需要处理                                         │
│ 3. 保存当前内核态上下文                                       │
│ 4. 设置信号处理程序                                           │
│ 5. 返回用户态执行信号处理程序                                 │
│ 6. 信号处理程序返回                                           │
│ 7. 恢复内核态上下文                                           │
│ 8. 继续系统调用                                               │
│                                                              │
│ 如果步骤 3 或 7 出错：                                        │
│ - 内核态上下文损坏                                            │
│ - 系统调用无法正确继续                                        │
└─────────────────────────────────────────────────────────────┘

路径 3：SMP 环境下的上下文迁移
┌─────────────────────────────────────────────────────────────┐
│ 进程可能在一个 CPU 保存上下文，在另一个 CPU 恢复：             │
│                                                              │
│ CPU 0: 保存进程 A 的上下文                                    │
│ 进程 A 迁移到 CPU 1                                          │
│ CPU 1: 恢复进程 A 的上下文                                    │
│                                                              │
│ 如果两个 CPU 的上下文格式不同：                               │
│ - 恢复失败                                                   │
│ - 或者恢复错误的值                                            │
└─────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 使用类型系统保证上下文结构一致
#[repr(C)]
struct UserContext {
    // 字段顺序和大小必须与汇编代码一致
    gs: u16,
    fs: u16,
    es: u16,
    ds: u16,
    edi: u32,
    esi: u32,
    ebp: u32,
    // ... 其他寄存器
    eip: u32,
    cs: u16,
    eflags: u32,
    esp: u32,
    ss: u16,
}

// 使用静态断言确保大小正确
const _: () = assert!(std::mem::size_of::<UserContext>() == EXPECTED_CONTEXT_SIZE);

// 上下文操作必须使用 unsafe，但安全性由类型系统保证
impl UserContext {
    /// # Safety
    /// 
    /// 此函数必须只在中断上下文中调用
    /// 栈指针必须指向有效的内核栈
    unsafe fn save_from_stack(stack_ptr: *const u8) -> Self {
        // 类型系统保证对齐和大小
        std::ptr::read(stack_ptr as *const Self)
    }
    
    /// # Safety
    /// 
    /// 此函数必须只在返回用户态前调用
    /// 栈指针必须指向有效的内核栈
    unsafe fn restore_to_stack(&self, stack_ptr: *mut u8) {
        std::ptr::write(stack_ptr as *mut Self, self.clone());
    }
}

// 使用 RAII 保证上下文保存/恢复成对
struct ContextGuard<'a> {
    context: UserContext,
    process: &'a mut Process,
}

impl<'a> ContextGuard<'a> {
    unsafe fn new(process: &'a mut Process) -> Self {
        let context = UserContext::save_from_stack(process.kernel_stack_ptr);
        Self { context, process }
    }
}

impl Drop for ContextGuard<'_> {
    fn drop(&mut self) {
        unsafe {
            self.context.restore_to_stack(self.process.kernel_stack_ptr);
        }
    }
}
```

---

## 四、SMP 同步不变量

### 9. Big Kernel Lock 互斥不变量

**逻辑定义**：

```
Big Kernel Lock (BKL) 在 CONFIG_SMP 下映射为自旋锁操作，
用于在关键路径中显式互斥。
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `spinlock.h:1-43` | 自旋锁定义 | SMP 条件编译 |
| `spinlock.h:40-41` | `BKL_LOCK/UNLOCK` | 获取/释放锁宏 |
| `smp.h:48` | `big_kernel_lock` | BKL 声明 |
| `smp.c:27` | `SPINLOCK_DEFINE` | BKL 定义 |

**高危区域与破坏路径**：

**最危险函数**：
- `BKL_LOCK` / `BKL_UNLOCK` - 锁操作
- `smp_schedule` - 跨 CPU 调度
- 中断处理程序 - 可能尝试获取锁

**易破坏路径**：

```
路径 1：死锁
┌─────────────────────────────────────────────────────────────┐
│ CPU 0 持有 BKL                                               │
│ CPU 0 发送 IPI 给 CPU 1                                      │
│ CPU 1 的 IPI 处理程序尝试获取 BKL                             │
│ CPU 1 等待 BKL                                               │
│ CPU 0 等待 CPU 1 响应 IPI                                    │
│                                                              │
│ 死锁!                                                        │
└─────────────────────────────────────────────────────────────┘

路径 2：忘记释放锁
┌─────────────────────────────────────────────────────────────┐
│ BKL_LOCK();                                                  │
│                                                              │
│ if (some_condition) {                                        │
│     return ERROR;  // 忘记 BKL_UNLOCK()!                     │
│ }                                                            │
│                                                              │
│ BKL_UNLOCK();                                                │
│                                                              │
│ 后果：其他 CPU 永远等待                                       │
└─────────────────────────────────────────────────────────────┘

路径 3：递归获取
┌─────────────────────────────────────────────────────────────┐
│ BKL_LOCK();                                                  │
│                                                              │
│ some_function_that_also_locks_bkl();  // 尝试再次获取        │
│                                                              │
│ 如果锁不可递归：                                             │
│ - 自死锁                                                     │
│ - 当前 CPU 永远等待                                          │
└─────────────────────────────────────────────────────────────┘
```

**脆弱时间窗**：

```
持有 BKL 期间：
┌──────────────────────────────────────────────────────────────┐
│ BKL_LOCK()                                                   │
│   ════════════════════════════ 脆弱窗口开始 ═════════════════│
│ 执行内核代码                                                 │
│ 其他 CPU 在自旋等待                                          │
│ 如果持有时间过长：                                           │
│ - 系统响应性下降                                             │
│ - 如果发生中断，可能导致死锁                                  │
│   ════════════════════════════ 脆弱窗口结束 ═════════════════│
│ BKL_UNLOCK()                                                 │
└──────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 使用 RAII 守卫确保锁释放
struct BigKernelLock {
    inner: SpinLock,
}

impl BigKernelLock {
    fn lock(&self) -> KernelGuard<'_> {
        self.inner.lock();
        KernelGuard { lock: self }
    }
    
    fn try_lock(&self) -> Option<KernelGuard<'_>> {
        if self.inner.try_lock() {
            Some(KernelGuard { lock: self })
        } else {
            None
        }
    }
}

struct KernelGuard<'a> {
    lock: &'a BigKernelLock,
}

impl Drop for KernelGuard<'_> {
    fn drop(&mut self) {
        self.lock.inner.unlock();
    }
}

// 使用时自动释放
fn kernel_function() -> Result<(), Error> {
    let _guard = BIG_KERNEL_LOCK.lock();  // 获取锁
    
    if some_condition {
        return Err(Error::Something);  // _guard 自动释放
    }
    
    // 更多代码...
    
    Ok(())  // _guard 自动释放
}

// 编译期检查：不允许递归获取
// 通过 Guard 的生命周期保证
```

---

### 10. CPU 本地变量一致性不变量

**逻辑定义**：

```
每个 CPU 有独立的本地变量副本：
- run_q_head / run_q_tail - 运行队列
- proc_ptr - 当前进程指针
- bill_ptr - 计费进程指针
- cpu_is_idle - CPU 空闲标志

访问本地变量必须使用正确的 CPU ID。
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `cpulocals.h:1-50` | CPU 本地变量定义 | 条件编译 |
| `cpulocals.h:13` | `get_cpu_var` | 获取指定 CPU 变量 |
| `cpulocals.h:15` | `get_cpulocal_var` | 获取当前 CPU 变量 |
| `proc.c:1600-1602` | 获取队列指针 | 使用示例 |

**高危区域与破坏路径**：

**最危险函数**：
- `get_cpu_var` - 获取指定 CPU 的变量
- `get_cpulocal_var` - 获取当前 CPU 的变量
- 进程迁移函数 - 修改 `p_cpu`

**易破坏路径**：

```
路径 1：进程迁移后访问错误 CPU 的数据
┌─────────────────────────────────────────────────────────────┐
│ 进程 A 原来在 CPU 0：                                         │
│ - p_cpu = 0                                                  │
│ - 在 CPU 0 的运行队列中                                      │
│                                                              │
│ 进程 A 迁移到 CPU 1：                                         │
│ - p_cpu = 1                                                  │
│ - 但如果迁移不完整：                                          │
│   - 可能还在 CPU 0 的队列中                                   │
│   - 或者本地变量未更新                                        │
│                                                              │
│ 后果：调度错误，进程丢失或双重运行                            │
└─────────────────────────────────────────────────────────────┘

路径 2：中断处理中 CPU ID 错误
┌─────────────────────────────────────────────────────────────┐
│ 中断处理程序：                                               │
│                                                              │
│ void interrupt_handler() {                                   │
│     // 如果使用错误的 CPU ID                                 │
│     struct proc *current = get_cpu_var(wrong_cpu, proc_ptr); │
│                                                              │
│     // 操作错误的进程!                                       │
│ }                                                            │
│                                                              │
│ 后果：操作错误的进程数据，系统状态损坏                        │
└─────────────────────────────────────────────────────────────┘

路径 3：SMP 启动期间的不一致
┌─────────────────────────────────────────────────────────────┐
│ SMP 启动过程中：                                             │
│                                                              │
│ BSP (Bootstrap Processor) 初始化全局数据                     │
│ AP (Application Processor) 启动                              │
│                                                              │
│ 如果 AP 在初始化完成前访问本地变量：                          │
│ - 数据可能未初始化                                           │
│ - 指针可能为 NULL                                            │
│                                                              │
│ 后果：AP 崩溃或行为异常                                      │
└─────────────────────────────────────────────────────────────┘
```

**脆弱时间窗**：

```
进程迁移过程：
┌──────────────────────────────────────────────────────────────┐
│ 从 CPU 0 的队列移除进程                                       │
│   ════════════════════════════ 脆弱窗口开始 ═════════════════│
│ 修改进程的 p_cpu = 1                                         │
│ 添加到 CPU 1 的队列                                          │
│   ════════════════════════════ 脆弱窗口结束 ═════════════════│
│                                                              │
│ 在脆弱窗口内：                                               │
│ - 进程可能不在任何队列中                                     │
│ - 或者同时在两个队列中                                       │
│ - 调度器可能做出错误决策                                     │
└──────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 使用类型系统编码 CPU 归属
struct CpuId(u8);

struct Process {
    cpu: CpuId,
    // ...
}

// CPU 本地存储
struct CpuLocal<T> {
    data: [T; MAX_CPUS],
}

impl<T> CpuLocal<T> {
    fn get(&self, cpu: CpuId) -> &T {
        &self.data[cpu.0 as usize]
    }
    
    fn get_mut(&mut self, cpu: CpuId) -> &mut T {
        &mut self.data[cpu.0 as usize]
    }
    
    fn current(&self) -> &T {
        // 使用 CPUID 指令获取当前 CPU ID
        let cpu = unsafe { current_cpu_id() };
        self.get(cpu)
    }
}

// 进程迁移使用类型系统保证完整性
impl Process {
    fn migrate_to(&mut self, run_queues: &mut RunQueues, new_cpu: CpuId) -> Result<(), Error> {
        let old_cpu = self.cpu;
        
        // 原子操作：先出队再入队
        run_queues.remove_from_cpu(old_cpu, self)?;
        self.cpu = new_cpu;
        run_queues.add_to_cpu(new_cpu, self)?;
        
        Ok(())
    }
}

// 使用 RAII 保证迁移的原子性
struct MigrationGuard<'a> {
    process: &'a mut Process,
    old_cpu: CpuId,
    new_cpu: CpuId,
    run_queues: &'a mut RunQueues,
    completed: bool,
}

impl MigrationGuard<'_> {
    fn complete(mut self) -> Result<(), Error> {
        self.run_queues.add_to_cpu(self.new_cpu, self.process)?;
        self.completed = true;
        Ok(())
    }
}

impl Drop for MigrationGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            // 回滚：恢复到旧 CPU
            self.process.cpu = self.old_cpu;
            self.run_queues.add_to_cpu(self.old_cpu, self.process).unwrap();
        }
    }
}
```

---

## 五、内存与分页不变量

### 11. 地址空间隔离不变量

**逻辑定义**：

```
每个进程有独立的地址空间：
- 用户进程之间相互隔离
- 用户进程不能直接访问内核内存
- 内核可以访问所有内存（需要适当保护）
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `arch/i386/memory.c` | 地址空间管理 | 页表操作 |
| `system/do_vmctl.c` | VM 控制 | 地址空间切换 |
| `proc.h` | `p_seg` | 段描述符（x86） |

注意：Minix3 使用微内核架构，虚拟内存管理由用户空间的 VM 服务进程处理，内核不直接管理 `p_memmap`。

**高危区域与破坏路径**：

**最危险函数**：
- `switch_address_space` - 切换地址空间
- `copy_msg_from_user` - 跨地址空间访问

**易破坏路径**：

```
路径 1：地址空间切换失败
┌─────────────────────────────────────────────────────────────┐
│ switch_address_space(new_process)                           │
│                                                              │
│ 如果切换失败：                                               │
│ - 当前地址空间未定义                                         │
│ - 后续内存访问可能使用错误的页表                              │
│ - 可能访问其他进程的内存                                      │
│                                                              │
│ 后果：数据损坏，安全漏洞                                     │
└─────────────────────────────────────────────────────────────┘

路径 2：内核态访问用户内存
┌─────────────────────────────────────────────────────────────┐
│ 内核代码访问用户态内存：                                      │
│                                                              │
│ copy_from_user(kernel_buf, user_ptr, size)                  │
│                                                              │
│ 如果 user_ptr 无效：                                         │
│ - 可能访问内核内存                                           │
│ - 可能触发页故障                                             │
│ - 可能导致内核崩溃                                           │
│                                                              │
│ 后果：内核崩溃或安全漏洞                                     │
└─────────────────────────────────────────────────────────────┘

路径 3：TLB 一致性
┌─────────────────────────────────────────────────────────────┐
│ SMP 环境下修改页表：                                         │
│                                                              │
│ CPU 0 修改进程 A 的页表                                       │
│ CPU 1 的 TLB 中仍有旧的映射                                   │
│                                                              │
│ 如果不刷新 TLB：                                             │
│ - CPU 1 可能使用旧的映射                                     │
│ - 可能访问错误的物理页                                        │
│                                                              │
│ 后果：数据损坏                                               │
└─────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 使用类型系统区分地址空间
struct KernelSpace;
struct UserSpace;

struct VirtualAddress<Space> {
    addr: usize,
    _space: PhantomData<Space>,
}

type KernelVAddr = VirtualAddress<KernelSpace>;
type UserVAddr = VirtualAddress<UserSpace>;

// 用户态指针必须验证
struct UserPtr<T> {
    addr: UserVAddr,
    _marker: PhantomData<T>,
}

impl<T> UserPtr<T> {
    fn new(addr: usize) -> Result<Self, InvalidAddress> {
        // 验证地址在用户空间范围内
        if addr < USER_SPACE_END {
            Ok(Self {
                addr: VirtualAddress { addr, _space: PhantomData },
                _marker: PhantomData,
            })
        } else {
            Err(InvalidAddress)
        }
    }
    
    fn read(&self) -> Result<T, CopyError> {
        // 安全地从用户态复制
        // 如果地址无效，返回错误而非崩溃
        unsafe {
            // 使用 copy_from_user 等安全机制
            copy_from_user(self.addr.addr)
        }
    }
}

// 地址空间切换使用 RAII
struct AddressSpaceGuard<'a> {
    process: &'a Process,
    old_space: PageTable,
}

impl<'a> AddressSpaceGuard<'a> {
    fn new(process: &'a Process) -> Result<Self, Error> {
        let old_space = current_page_table();
        switch_to_page_table(process.page_table)?;
        Ok(Self { process, old_space })
    }
}

impl Drop for AddressSpaceGuard<'_> {
    fn drop(&mut self) {
        // 自动恢复原地址空间
        switch_to_page_table(self.old_space).unwrap();
    }
}
```

---

### 12. 物理内存分配不变量

**逻辑定义**：

```
内核物理页分配必须满足：
- 不能分配已分配的页
- 不能释放未分配的页
- 页表操作必须保持一致性

注意：Minix3 使用微内核架构，实际的内存管理（分配、映射）主要由用户空间的 VM 服务进程处理，内核只负责低级别的页表操作。
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `arch/i386/pg_utils.c:138` | `pg_alloc_page()` | 分配物理页 |
| `arch/i386/pg_utils.c:123` | `alloc_pagetable()` | 分配页表 |

**高危区域与破坏路径**：

**最危险函数**：
- `pg_alloc_page` - 分配物理页
- `alloc_pagetable` - 分配页表
- `memset` / `memcpy` - 内存操作

**易破坏路径**：

```
路径 1：页表状态不一致
┌─────────────────────────────────────────────────────────────┐
│ 修改页表后不刷新 TLB：                                      │
│                                                              │
│ 后果：                                                       │
│ - TLB 中仍有旧的映射                                         │
│ - CPU 可能访问错误的物理页                                    │
│ - 数据损坏或安全漏洞                                         │
└─────────────────────────────────────────────────────────────┘

路径 2：页表操作竞态
┌─────────────────────────────────────────────────────────────┐
│ SMP 环境下，多个 CPU 同时修改页表：                            │
│                                                              │
│ 后果：                                                       │
│ - 页表状态损坏                                               │
│ - 可能导致双重映射或未映射                                    │
└─────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 使用所有权系统防止双重释放和使用后释放
struct PhysicalPage {
    frame: FrameNumber,
}

impl PhysicalPage {
    fn alloc() -> Result<Self, OutOfMemory> {
        let frame = allocate_frame()?;
        Ok(Self { frame })
    }
}

impl Drop for PhysicalPage {
    fn drop(&mut self) {
        // 自动释放，不会忘记
        deallocate_frame(self.frame);
    }
}

// 使用生命周期防止悬垂指针
struct PhysicalMapping<'a> {
    page: &'a PhysicalPage,
    virt_addr: VirtualAddress<KernelSpace>,
}

impl<'a> PhysicalMapping<'a> {
    fn map(page: &'a PhysicalPage) -> Result<Self, Error> {
        let virt_addr = map_to_kernel_space(page.frame)?;
        Ok(Self { page, virt_addr })
    }
    
    fn as_ptr(&self) -> *const u8 {
        self.virt_addr.addr as *const u8
    }
    
    fn as_mut_ptr(&self) -> *mut u8 {
        self.virt_addr.addr as *mut u8
    }
}

impl Drop for PhysicalMapping<'_> {
    fn drop(&mut self) {
        // 自动取消映射
        unmap_from_kernel_space(self.virt_addr);
    }
}

// 使用 arena 分配器管理批量分配
struct PageArena {
    pages: Vec<PhysicalPage>,
}

impl PageArena {
    fn new(capacity: usize) -> Result<Self, OutOfMemory> {
        let mut pages = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            pages.push(PhysicalPage::alloc()?);
        }
        Ok(Self { pages })
    }
    
    fn alloc(&mut self) -> Option<PhysicalPage> {
        self.pages.pop()
    }
    
    fn free(&mut self, page: PhysicalPage) {
        self.pages.push(page);
    }
}
```

---

### 13. 发送队列链表不变量

**逻辑定义**：

```
当进程处于 RTS_SENDING 状态时：
- 进程必须存在于目标进程的 p_caller_q 链表中
- p_q_link 指向链表中的下一个发送者（或 NULL）
- 进程不能同时存在于多个发送队列中

链表完整性：
- 链表不能有环
- 链表中每个节点的 p_q_link 必须有效或为 NULL
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `proc.h:73` | `struct proc *p_caller_q` | 发送者队列头 |
| `proc.h:74` | `struct proc *p_q_link` | 链表下一个节点 |
| `proc.c:952-956` | 入队操作 | 添加到发送队列尾部 |
| `proc.c:1089-1090` | 出队操作 | 从发送队列移除 |

```c
/* proc.c:952-956 - 入队 */
assert(caller_ptr->p_q_link == NULL);
xpp = &dst_ptr->p_caller_q;		/* find end of list */
while (*xpp) xpp = &(*xpp)->p_q_link;	
*xpp = caller_ptr;			/* add caller to end */

/* proc.c:1089-1090 - 出队 */
*xpp = sender->p_q_link;		/* remove from queue */
sender->p_q_link = NULL;
```

**高危区域与破坏路径**：

**最危险函数**：
- `mini_send` - 将发送者加入队列
- `mini_receive` - 从队列移除发送者
- `do_clear` - 清理进程时需要从队列移除

**易破坏路径**：

```
路径 1：进程终止时未从发送队列移除
┌─────────────────────────────────────────────────────────────┐
│ 进程 A 正在向进程 B 发送消息（在 B 的 p_caller_q 中）         │
│ 进程 A 被信号终止                                            │
│ 清理代码未将 A 从 B 的队列中移除                              │
│                                                              │
│ 后果：                                                       │
│ - B 的发送队列包含已死进程                                   │
│ - B 接收时可能访问无效的进程结构                              │
│ - 队列遍历可能崩溃                                           │
└─────────────────────────────────────────────────────────────┘

路径 2：链表操作错误
┌─────────────────────────────────────────────────────────────┐
│ 从链表移除节点时：                                           │
│                                                              │
│ *xpp = sender->p_q_link;  // 跳过 sender                     │
│ sender->p_q_link = NULL;  // 清除 sender 的链接              │
│                                                              │
│ 如果顺序错误或遗漏：                                         │
│ - 链表断裂                                                   │
│ - 或 sender 仍指向链表中的节点                                │
│ - 多个进程可能指向同一个节点                                  │
└─────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 使用所有权编码链表关系
struct SenderQueue {
    head: Option<NonNull<Process>>,
}

struct Process {
    // 当进程在发送队列中时，next_sender 是 Some
    // 当进程不在发送队列中时，next_sender 是 None
    next_sender: Option<NonNull<Process>>,
}

impl SenderQueue {
    fn push(&mut self, process: &mut Process) -> Result<(), Error> {
        // 类型系统保证：process.next_sender 必须是 None 才能入队
        if process.next_sender.is_some() {
            return Err(Error::AlreadyInQueue);
        }
        
        // 找到尾部并添加
        let mut current = &mut self.head;
        while let Some(node) = *current {
            current = &mut unsafe { &mut *node }.next_sender;
        }
        *current = Some(NonNull::from(process));
        process.next_sender = None;  // 尾部
        
        Ok(())
    }
    
    fn pop(&mut self) -> Option<&mut Process> {
        let head = self.head.take()?;
        let process = unsafe { &mut *head.as_ptr() };
        self.head = process.next_sender.take();
        Some(process)
    }
}

// 使用 RAII 保证进程终止时从队列移除
struct QueueGuard<'a> {
    process: &'a mut Process,
    queue: &'a mut SenderQueue,
}

impl Drop for QueueGuard<'_> {
    fn drop(&mut self) {
        // 自动从队列移除
        self.queue.remove(self.process);
    }
}
```

---

### 14. IPC 权限位图约束

**逻辑定义**：

```
进程只能向 s_ipc_to 位图中允许的目标发送消息：
- 每个系统进程有 s_ipc_to 位图定义通信边界
- 用户进程的 s_ipc_to 由系统管理
- 内核在每次 mini_send 前检查权限
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `priv.h:35` | `sys_map_t s_ipc_to` | 允许的目标进程位图 |
| `priv.h:86` | `may_send_to(rp, nr)` | 权限检查宏 |
| `proc.c:536` | 权限检查 | 发送前验证 |
| `proc.c:1534` | 异步发送检查 | 同样需要权限 |

```c
/* priv.h:86 */
#define may_send_to(rp, nr) (get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr)))

/* proc.c:536 */
if (!may_send_to(caller_ptr, src_dst_p)) {
    return(ECALLDENIED);	/* call denied by ipc mask */
}
```

**高危区域与破坏路径**：

**最危险函数**：
- `mini_send` - 发送前检查权限
- `mini_notify` - 通知也需要权限
- `do_privctl` - 修改权限位图

**易破坏路径**：

```
路径 1：权限检查遗漏
┌─────────────────────────────────────────────────────────────┐
│ 如果某个 IPC 路径忘记检查 may_send_to：                       │
│                                                              │
│ - 用户进程可能向内核任务发送消息                              │
│ - 可能绕过安全策略                                           │
│ - 可能导致权限提升                                           │
└─────────────────────────────────────────────────────────────┘

路径 2：权限位图不一致
┌─────────────────────────────────────────────────────────────┐
│ 进程 A 被允许向进程 B 发送消息                                │
│ 进程 B 终止，进程槽被进程 C 重用                              │
│ 权限位图未更新                                               │
│                                                              │
│ 后果：                                                       │
│ - A 可能意外向 C 发送消息                                    │
│ - 原本给 B 的消息被 C 接收                                   │
│ - 安全漏洞                                                   │
└─────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// 使用类型系统编码权限
struct IpcPermission {
    allowed_targets: BitSet<ProcessId>,
}

impl IpcPermission {
    fn can_send_to(&self, target: ProcessId) -> bool {
        self.allowed_targets.contains(target)
    }
    
    fn grant(&mut self, target: ProcessId) {
        self.allowed_targets.insert(target);
    }
    
    fn revoke(&mut self, target: ProcessId) {
        self.allowed_targets.remove(target);
    }
}

// 发送函数强制检查权限
fn send(
    caller: &Process,
    target: &mut Process,
    message: Message,
) -> Result<(), IpcError> {
    // 编译期保证：必须检查权限
    if !caller.permissions.can_send_to(target.id) {
        return Err(IpcError::PermissionDenied);
    }
    
    // 执行发送...
    Ok(())
}

// 使用类型状态防止权限检查被绕过
struct UncheckedEndpoint(ProcessId);
struct CheckedEndpoint {
    id: ProcessId,
    _permission: (),  // 证明已检查权限
}

fn check_permission(
    caller: &Process,
    target: UncheckedEndpoint,
) -> Result<CheckedEndpoint, IpcError> {
    if !caller.permissions.can_send_to(target.0) {
        return Err(IpcError::PermissionDenied);
    }
    Ok(CheckedEndpoint { id: target.0, _permission: () })
}

// 只有 CheckedEndpoint 才能用于发送
fn send_checked(target: CheckedEndpoint, message: Message) -> Result<(), IpcError> {
    // 安全：权限已检查
    Ok(())
}
```

---

### 15. Endpoint 稳定性不变量

**逻辑定义**：

```
在一次 IPC 操作的生命周期内：
- 目标 Endpoint 必须通过 isokendpt 校验（槽位与代数匹配）
- 目标进程若被清理，会被设置 RTS_NO_ENDPOINT 并导致 IPC 失败
- 内核必须返回 EDEADSRCDST 错误
```

**源码实证**：

| 位置 | 代码 | 说明 |
|------|------|------|
| `proc.h:82` | `endpoint_t p_endpoint` | 进程端点号（带代数） |
| `proc.c:511-518` | Endpoint 验证 | `isokendpt` 检查 |
| `proc.c:896-898` | 死端点检查 | `RTS_NO_ENDPOINT` |

```c
/* proc.c:511-518 */
/* Require a valid source and/or destination process. */
if(!isokendpt(src_dst_e, &src_dst_p)) {
    return EDEADSRCDST;
}

/* proc.c:896-898 */
if (RTS_ISSET(dst_ptr, RTS_NO_ENDPOINT)) {
    return EDEADSRCDST;
}
```

**高危区域与破坏路径**：

**最危险函数**：
- `mini_send` - 发送前验证 Endpoint
- `mini_receive` - 接收前验证
- `isokendpt` - Endpoint 验证函数

**易破坏路径**：

```
路径 1：TOCTOU 竞态
┌─────────────────────────────────────────────────────────────┐
│ T1: isokendpt(endpoint) 返回 true                           │
│ T2: 目标进程终止，Endpoint 失效                               │
│ T3: 使用该 Endpoint 发送消息                                 │
│                                                              │
│ 后果：                                                       │
│ - 消息可能发送给错误的新进程                                  │
│ - 或者访问无效的进程结构                                      │
└─────────────────────────────────────────────────────────────┘

路径 2：Endpoint 代数不匹配
┌─────────────────────────────────────────────────────────────┐
│ Endpoint 包含代数信息：                                       │
│ endpoint = (generation << 15) + process_nr                  │
│                                                              │
│ 如果进程槽被重用：                                           │
│ - 新进程有不同的代数                                         │
│ - 旧的 Endpoint 应该失效                                     │
│                                                              │
│ 如果代数检查遗漏：                                           │
│ - 消息可能发送给错误的新进程                                  │
└─────────────────────────────────────────────────────────────┘
```

**Rust 重构指导**：

```rust
// Minix3 Endpoint 公式：endpoint = (generation << 15) + process_nr
// 低 15 位：process_nr，高 17 位：generation

const ENDPOINT_GENERATION_SHIFT: u32 = 15;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Endpoint(i32);

impl Endpoint {
    fn new(process_nr: usize, generation: u32) -> Self {
        Endpoint(((generation as i32) << ENDPOINT_GENERATION_SHIFT) + process_nr as i32)
    }
    
    fn process_nr(&self) -> usize {
        (self.0 & 0x7FFF) as usize
    }
    
    fn generation(&self) -> u32 {
        (self.0 >> ENDPOINT_GENERATION_SHIFT) as u32
    }
    
    fn validate(&self, process_table: &ProcessTable) -> Option<&Process> {
        let process = process_table.get(self.process_nr())?;
        if process.endpoint.generation() == self.generation() {
            Some(process)
        } else {
            None  // 代数不匹配，进程槽已被重用
        }
    }
}

// 使用生命周期保证 Endpoint 在 IPC 期间有效
struct ValidatedEndpoint<'a> {
    process: &'a Process,
    endpoint: Endpoint,
}

impl<'a> ValidatedEndpoint<'a> {
    fn new(endpoint: Endpoint, process_table: &'a ProcessTable) -> Result<Self, IpcError> {
        let process = endpoint.validate(process_table)
            .ok_or(IpcError::DeadEndpoint)?;
        Ok(Self { process, endpoint })
    }
}

// IPC 函数使用 ValidatedEndpoint
fn send_validated(
    target: ValidatedEndpoint<'_>,
    message: Message,
) -> Result<(), IpcError> {
    // 安全：Endpoint 已验证且生命周期保证在操作期间有效
    Ok(())
}
```

---

## 六、Gemini 文档中的错误说明

在对比 Gemini 生成的文档与真实 Minix3 源码后，发现以下错误：

### 错误 1：`k_reenter` 变量不存在

Gemini 文档声称存在 `k_reenter` 变量跟踪内核重入深度。**实际上 Minix3 没有这个全局变量**。

**真实实现**：Minix3 使用 `is_nested` 参数在异常处理函数间传递嵌套状态：
```c
// arch/i386/exception.c
void exception_handler(int is_nested, struct exception_frame * frame)
```

### 错误 2：`p_blocked_on` 字段不存在

Gemini 文档声称进程有 `p_blocked_on` 字段。**实际上 Minix3 没有这个字段**。

**真实实现**：Minix3 使用 `P_BLOCKEDON` 宏，根据 `RTS_SENDING` 或 `RTS_RECEIVING` 标志返回 `p_sendto_e` 或 `p_getfrom_e`：
```c
// proc.h:187
#define P_BLOCKEDON(p)                          \
    (                                           \
        ((p)->p_rts_flags & RTS_SENDING) ?      \
        (p)->p_sendto_e :                       \
        (                                       \
            (((p)->p_rts_flags & RTS_RECEIVING) ? \
            (p)->p_getfrom_e : NONE)            \
        )                                       \
    )
```

### 错误 3：栈守恒不变量描述不准确

Gemini 声称嵌套时"严禁切换栈帧"。实际上 Minix3 的栈管理更复杂，每个进程有独立的内核栈，切换进程时会切换栈。

---

## 六点五、Qwen Deep Research 文档中的错误说明

在对比 Qwen 生成的文档与真实 Minix3 源码后，发现以下错误：

### 错误 1：`p_state` 字段不存在

Qwen 文档声称进程有 `p_state` 字段，取值为 `SREADY`、`SRUNNING`、`SBLOCKED`、`SSTOPPED`。**实际上 Minix3 没有这个字段**。

**真实实现**：Minix3 使用 `p_rts_flags` 位标志来表示进程状态：
```c
// proc.h
volatile u32_t p_rts_flags;  /* process is runnable iff zero */

// 状态标志位
#define RTS_SLOT_FREE     0x01  /* process slot is free */
#define RTS_SENDING       0x04  /* process blocked trying to send */
#define RTS_RECEIVING     0x08  /* process blocked trying to receive */
#define RTS_PREEMPTED     0x4000 /* preempted by higher priority process */
#define RTS_NO_QUANTUM    0x8000 /* ran out of quantum */
```

进程可运行的条件是 `p_rts_flags == 0`，而非检查 `p_state == SREADY`。

### 错误 2：`p_messbuf` 字段不存在

Qwen 文档声称进程有 `p_messbuf` 字段指向消息缓冲区。**实际上 Minix3 没有这个字段**。

**真实实现**：Minix3 使用两个独立的消息字段：
```c
// proc.h
message p_sendmsg;       /* Message from this process if SENDING */
message p_delivermsg;    /* Message for this process if MF_DELIVERMSG */
vir_bytes p_delivermsg_vir;  /* Virtual addr this proc wants message at */
```

### 错误 3：`p_getfrom`/`p_sendto` 类型错误

Qwen 文档声称 `p_getfrom` 和 `p_sendto` 类型为 `proc_nr_t`。**实际上类型为 `endpoint_t`**，且字段名不同。

**真实实现**：
```c
// proc.h
endpoint_t p_getfrom_e;  /* from whom does process want to receive? */
endpoint_t p_sendto_e;   /* to whom does process want to send? */
```

`endpoint_t` 包含进程号和代数信息，用于检测进程槽重用：
```c
// endpoint = (generation << 15) + process_nr
// 低 15 位：process_nr，高 17 位：generation
```

### 错误 4：消息结构体定义过于简化

Qwen 文档给出了简化的消息结构体定义。**实际上 Minix3 的消息结构体更复杂**。

**真实实现**：消息固定 64 字节，包含 `m_source` 和 `m_type`：
```c
// minix/ipc.h
typedef struct noxfer_message {
    endpoint_t m_source;    /* who sent the message */
    int m_type;             /* what kind of message is it */
    union {
        mess_u8  m_u8;
        mess_u16 m_u16;
        mess_u32 m_u32;
        mess_u64 m_u64;
        mess_1   m_m1;
        mess_2   m_m2;
        mess_3   m_m3;
        // ... 更多类型特定的消息格式
        u8_t size[56];  /* message payload may have 56 bytes at most */
    };
} message __ALIGNED(16);

// 编译期断言：消息必须正好 64 字节
typedef int _ASSERT_message[sizeof(message) == 64 ? 1 : -1];
```

### 错误 5：进程号范围描述不准确

Qwen 文档声称内核进程号范围为 -1 到 -NR_TASKS。**实际范围是 -NR_TASKS 到 -1**。

**真实实现**：
```c
// proc.h
#define BEG_PROC_ADDR   (&proc[0])
#define BEG_USER_ADDR   (&proc[NR_TASKS])
#define END_PROC_ADDR   (&proc[NR_TASKS + NR_PROCS])
#define proc_addr(n)    (&(proc[NR_TASKS + (n)]))

// 进程号范围：
// 内核任务：-NR_TASKS 到 -1（如 IDLE=-4, CLOCK=-3, SYSTEM=-2, KERNEL=-1）
// 用户进程：0 到 NR_PROCS-1
```

### 错误 6：文件系统不变量不属于内核

Qwen 文档详细描述了文件系统不变量（超级块、inode 等）。**这些不属于内核不变量**，因为 Minix3 的文件系统是用户空间服务进程，不在内核中。

Minix3 微内核架构的核心原则：文件系统（VFS、MFS、PFS 等）运行在用户空间，通过 IPC 与内核通信。

---

## 六点七、Seed Deep Research 文档中的严重错误说明

在对比 Seed 生成的文档与真实 Minix3 源码后，发现大量严重错误：

### 错误 1：`p_state` 字段不存在

Seed 文档声称进程有 `p_state` 字段。**实际上 Minix3 没有这个字段**。

**真实实现**：Minix3 使用 `p_rts_flags` 位标志：
```c
// proc.h:27
volatile u32_t p_rts_flags;  /* process is runnable only if zero */

// proc.h:141-166 - 状态标志位定义
#define RTS_SLOT_FREE   0x01    /* process slot is free */
#define RTS_PROC_STOP   0x02    /* process has been stopped */
#define RTS_SENDING     0x04    /* process blocked trying to send */
#define RTS_RECEIVING   0x08    /* process blocked trying to receive */
#define RTS_SIGNALED    0x10    /* set when new kernel signal arrives */
#define RTS_SIG_PENDING 0x20    /* unready while signal being processed */
#define RTS_P_STOP      0x40    /* set when process is being traced */
#define RTS_NO_PRIV     0x80    /* keep forked system process from running */
#define RTS_NO_ENDPOINT 0x100   /* process cannot send or receive messages */
#define RTS_VMINHIBIT   0x200   /* not scheduled until pagetable set by VM */
#define RTS_PAGEFAULT    0x400  /* process has unhandled pagefault */
#define RTS_VMREQUEST    0x800  /* originator of vm memory request */
#define RTS_VMREQTARGET  0x1000 /* target of vm memory request */
#define RTS_PREEMPTED    0x4000 /* this process was preempted by a higher
                                   priority process */
#define RTS_NO_QUANTUM   0x8000 /* process ran out of its quantum */
#define RTS_BOOTINHIBIT  0x10000 /* not ready until VM has made it */
```

### 错误 2：`p_messbuf` 字段不存在

Seed 文档声称进程有 `p_messbuf` 字段。**实际上 Minix3 没有这个字段**。

**真实实现**：
```c
// proc.h
message p_sendmsg;       /* Message from this process if SENDING */
message p_delivermsg;    /* Message for this process if MF_DELIVERMSG */
```

### 错误 3：`p_getfrom`/`p_sendto` 类型错误

Seed 文档声称类型为 `proc_nr_t`。**实际上类型为 `endpoint_t`**，字段名也不同。

**真实实现**：
```c
// proc.h:75-76
endpoint_t p_getfrom_e;  /* from whom does process want to receive? */
endpoint_t p_sendto_e;   /* to whom does process want to send? */
```

### 错误 4：`p_memmap` 字段不存在

Seed 文档声称进程有 `p_memmap[NR_LOCAL_SEGS]` 字段。**实际上 Minix3 内核的 proc 结构没有这个字段**。

内存映射由 VM（虚拟内存管理器）服务进程管理，不在内核的 proc 结构中。

### 错误 5：死锁检测函数签名错误

Seed 文档给出了错误的死锁检测函数：
```c
// Seed 文档错误版本
static int deadlock(endpoint_t proc, endpoint_t *path, int depth)
```

**真实实现**：
```c
// proc.c:703
static int deadlock(
  int function,              /* trap number */
  register struct proc *cp,  /* pointer to caller */
  endpoint_t src_dst_e       /* src or dst process */
)
```

真实函数接受三个参数：函数类型、调用者进程指针、目标端点。不是 Seed 描述的路径数组。

### 错误 6：系统调用表完全错误

Seed 文档列出了大量 Linux 系统调用（如 `sys_epoll_create`、`sys_bpf`、`sys_kexec_load` 等）。**这些都不是 Minix3 的系统调用**。

Minix3 使用微内核架构，系统调用通过 IPC 发送给服务进程处理，不在内核中实现完整的系统调用表。

### 错误 7：进程状态标志名称错误

Seed 文档使用 `SLOT_FREE`、`NO_MAP`、`SENDING` 等名称。**实际名称带有 `RTS_` 前缀**：

```c
// 正确名称
RTS_SLOT_FREE, RTS_PROC_STOP, RTS_SENDING, RTS_RECEIVING, RTS_SIGNALED,
RTS_SIG_PENDING, RTS_P_STOP, RTS_NO_PRIV, RTS_NO_ENDPOINT, RTS_VMINHIBIT,
RTS_PAGEFAULT, RTS_VMREQUEST, RTS_VMREQTARGET, RTS_PREEMPTED, RTS_NO_QUANTUM,
RTS_BOOTINHIBIT
```

### 错误 8：消息结构体定义错误

Seed 文档给出了简化的消息结构。**实际消息结构更复杂**：

```c
// minix/ipc.h - 真实实现
typedef struct noxfer_message {
    endpoint_t m_source;    /* who sent the message */
    int m_type;             /* what kind of message is it */
    union {
        mess_u8  m_u8;
        mess_u16 m_u16;
        // ... 大量类型特定的消息格式
        u8_t size[56];  /* message payload may have 56 bytes at most */
    };
} message __ALIGNED(16);

// 编译期断言
typedef int _ASSERT_message[sizeof(message) == 64 ? 1 : -1];
```

### 错误 9：`p_rts_flags` 类型错误

Seed 文档声称 `p_rts_flags` 类型为 `char`。**实际类型为 `volatile u32_t`**：

```c
// proc.h:27
volatile u32_t p_rts_flags;  /* process is runnable only if zero */
```

### 错误 10：调度队列数量错误

Seed 文档声称有 16 个优先级队列。**实际数量由 `NR_SCHED_QUEUES` 定义**，需要查看具体配置。

### 错误 11：特权级定义错误

Seed 文档定义了 `INTR_PRIVILEGE`、`TASK_PRIVILEGE`、`USER_PRIVILEGE`。**这些常量在 Minix3 内核中不存在**。

Minix3 使用 `priv` 结构管理进程特权，通过 `p_priv` 指针访问。

### 错误 12：文件系统和设备驱动不变量不属于内核

Seed 文档详细描述了文件系统和设备驱动的不变量。**这些都不属于内核不变量**。

Minix3 微内核架构：
- 文件系统（VFS、MFS 等）是用户空间服务进程
- 设备驱动是用户空间服务进程
- 它们通过 IPC 与内核通信，其内部不变量不在内核中

---

## 六点八、审查补充不变量（以源码为准）

### 1. RTS_PREEMPTED 出队与回队

```
更高优先级进程入队触发 RTS_PREEMPTED：
- 设置标志时经由 RTS_SET 导致当前进程出队
- switch_to_user 清除 RTS_PREEMPTED 后，仍可运行则重新入队
- p_cpu_time_left > 0 → enqueue_head，否则 enqueue
```

**代码映射**：
- `kernel/proc.c`：`enqueue()` 中 `RTS_SET(p, RTS_PREEMPTED)`
- `kernel/proc.c`：`switch_to_user()` 中清除 `RTS_PREEMPTED` 并重新入队
- `kernel/proc.h`：`RTS_PREEMPTED` 注释

### 2. VM 请求队列一致性

```
vm_suspend() 调用时：
- caller/target 均不得已设置 RTS_VMREQUEST
- caller 设置 RTS_VMREQUEST 并加入 vmrequest 队列
```

**代码映射**：
- `kernel/proc.c`：`vm_suspend()` 断言与入队逻辑
- `kernel/system.c`：`clear_memreq()` 清理 VM 请求

### 3. VM 建表前的调度抑制

```
启动阶段用户进程被设置 RTS_VMINHIBIT/RTS_BOOTINHIBIT，
直到 VM 完成页表设置才可运行
```

**代码映射**：
- `kernel/main.c`：初始化用户进程设置 RTS_VMINHIBIT/RTS_BOOTINHIBIT
- `kernel/proc.h`：相关标志定义

### 4. VFS fproc 端点一致性

```
fproc 端点一致性要求：
- fp_endpoint 为 NONE 时 fp_pid 必须是 PID_FREE
- 否则 fp_pid 必须非 PID_FREE
```

**代码映射**：
- `servers/vfs/utility.c`：`isokendpt_f()` 断言与检查

### 5. VFS filp/vnode 引用一致性

```
锁定 filp 时必须满足：
- filp_count > 0
- filp_vno != NULL
- vnode 的 v_ref_count > 0
- filp_vno 在加锁前后保持一致
```

**代码映射**：
- `servers/vfs/filedes.c`：`lock_filp()` 相关断言

### 6. 消息结构尺寸固定

```
message 结构体固定为 64 字节（通过 _ASSERT_MSG_SIZE 校验）
```

**代码映射**：
- `include/minix/ipc.h`：`message` 定义与 `_ASSERT_MSG_SIZE`

---

## 七、总结：高危函数清单

以下是维护上述不变量时最容易出错的函数，在 Rust 重构时需要特别关注：

| 函数 | 文件 | 风险等级 | 相关不变量 |
|------|------|----------|------------|
| `RTS_SET` / `RTS_UNSET` | `proc.h` | **极高** | 就绪队列排他性 |
| `enqueue` / `dequeue` | `proc.c` | **极高** | 就绪队列排他性 |
| `mini_send` | `proc.c` | **极高** | 消息完整性、死锁检测、发送队列、权限检查 |
| `mini_receive` | `proc.c` | **极高** | 消息投递状态、死锁检测、发送队列 |
| `deadlock` | `proc.c` | **高** | 死锁检测回路判定 |
| `pick_proc` | `proc.c` | **高** | 调度优先级 |
| `BKL_LOCK` / `BKL_UNLOCK` | `spinlock.h` | **高** | BKL 互斥 |
| `restore_user_context` | `arch_system.c` | **高** | 上下文保存 |
| `switch_address_space` | `memory.c` | **高** | 地址空间隔离 |
| `copy_msg_from_user` | `proc.c` | **中** | 消息完整性 |
| `pg_alloc_page` | `pg_utils.c` | **中** | 物理内存分配 |
| `smp_schedule` | `smp.c` | **中** | CPU 本地变量 |
| `may_send_to` | `priv.h` | **中** | IPC 权限位图 |
| `isokendpt` | `const.h` (宏) | **中** | Endpoint 稳定性 |
| `do_clear` | `system/do_clear.c` | **中** | 发送队列清理 |

---

## 八、Rust 重构核心原则

1. **类型状态模式**：使用泛型参数编码进程状态，编译期保证状态转换合法
2. **所有权系统**：消息传递消耗所有权，防止并发修改
3. **RAII 守卫**：锁、地址空间、上下文使用 RAII 自动释放
4. **Option/Result**：显式处理可能失败的操作，不忽略错误
5. **受控的 unsafe**：将 unsafe 限制在最小范围，并在注释中说明安全性保证
6. **原子操作**：使用 `AtomicU32` 等类型替代 volatile + 手动同步
7. **生命周期**：使用生命周期参数防止悬垂指针

---

*文档生成时间：2026-03-02*
*适用于 minix-rs 项目内核重构*
