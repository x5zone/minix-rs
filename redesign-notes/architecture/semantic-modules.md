# Minix3 语义模块设计文档

> **目的**: 为 `code reading → rewrite → redesign` 路线提供可执行的抽象模型
> 
> **与 progress.md 的区别**: 
> - progress.md = 阅读进度跟踪（按文件组织）
> - 本文档 = 设计抽象（按语义模块组织）

---

## 📐 模块依赖图

```
                    ┌─────────────┐
                    │   proc      │  进程控制块（核心数据）
                    └──────┬──────┘
                           │
           ┌───────────────┼───────────────┐
           │               │               │
           ▼               ▼               ▼
    ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
    │  scheduler  │ │     IPC     │ │  interrupt  │
    │  (调度核心)  │ │  (消息传递)  │ │  (中断处理)  │
    └──────┬──────┘ └──────┬──────┘ └──────┬──────┘
           │               │               │
           └───────────────┼───────────────┘
                           │
                           ▼
                    ┌─────────────┐
                    │ RTS 状态机  │  运行时状态（隐式核心）
                    └─────────────┘
```

---

## 🔥 模块 A: 调度核心 (Scheduler Core)

### 最小接口

```rust
fn pick_next() -> Option<ProcId>;
fn enqueue(proc: ProcId);
fn dequeue(proc: ProcId);
fn set_priority(proc: ProcId, prio: u8);
```

### 状态

```rust
struct Scheduler {
    run_queue: [VecDeque<ProcId>; NR_SCHED_QUEUES],  // 多级反馈队列
    ready_bitmap: u32,                               // 位图加速查找
}
```

### 不变量 ⚠️

1. **唯一性**: 一个进程最多在一个队列中
2. **一致性**: `ready_bitmap` 第 i 位为 1 ⟺ `run_queue[i]` 非空
3. **优先级**: `pick_next()` 返回最高优先级非空队列的队首

### Minix3 实现

| 文件 | 函数 |
|------|------|
| `kernel/proc.c` | `enqueue()`, `dequeue()`, `pick_proc()` |
| `kernel/proc.h` | `p_sched` 字段 |

### 设计缺陷

- **BKL 限制并发**: 所有调度操作持有全局锁
- **无负载均衡**: SMP 场景下每个 CPU 独立调度，可能导致负载不均

---

## 🔥 模块 B: IPC 核心

### 最小接口

```rust
fn send(src: ProcId, dst: ProcId, msg: &Message) -> Result<(), IpcError>;
fn receive(dst: ProcId, src_filter: SrcFilter) -> Result<Message, IpcError>;
fn notify(src: ProcId, dst: ProcId) -> Result<(), IpcError>;
fn sendnb(src: ProcId, dst: ProcId, msg: &Message) -> Result<(), IpcError>;  // 非阻塞
fn senda(table: &[AsyncMsg]) -> Result<(), IpcError>;  // 异步批量
```

### 状态

```rust
struct IpcState {
    send_queue: HashMap<ProcId, VecDeque<ProcId>>,  // dst -> 等待发送的 src 队列
    receive_queue: HashMap<ProcId, VecDeque<ProcId>>, // dst -> 等待接收的 src 队列
    notify_bitmap: HashMap<ProcId, u32>,  // dst -> 待处理的 notify 位图
}
```

### 不变量 ⚠️

1. **互斥性**: 一个进程不能同时在 `run_queue` 和 `send_queue`/`receive_queue`
2. **匹配性**: `send(src, dst)` 成功 ⟺ dst 正在等待 src（或 ANY）
3. **零拷贝**: 消息直接从发送者地址空间拷贝到接收者，无内核缓冲
4. **同步 rendezvous**: 发送者阻塞直到接收者接收（除非 notify/sendnb）

### Minix3 实现

| 文件 | 函数 |
|------|------|
| `kernel/proc.c` | `mini_send()`, `mini_receive()`, `mini_notify()` |
| `kernel/ipc.h` | `WILLRECEIVE`, `CANRECEIVE` 宏 |
| `include/minix/ipc.h` | `message` 结构体 |

### 核心发现

**IPC 本质 = 同步 rendezvous + 队列阻塞 + 零拷贝**

```
发送者                    接收者
   │                        │
   │  send(dst, msg)        │  receive(src)
   │  ─────────────────────>│
   │      (阻塞等待)         │
   │                        │
   │  <─────────────────────│
   │      (拷贝完成)         │
   │                        │
   ▼                        ▼
 继续执行                  继续执行
```

### 设计缺陷

1. **无超时机制**: `send()` 可能永久阻塞
2. **无死锁检测**: 循环等待无法自动发现
3. **异步消息语义混乱**: `-1` 既表示"初始状态"又表示"处理中状态"（Bug）

---

## 🔥 模块 C: RTS 状态机 (Runtime State Machine)

> ⚠️ **这是 Minix3 最 tricky 的设计之一**

### 状态定义

```rust
bitflags! {
    struct RtsFlags: u32 {
        const SLOT_FREE     = 0x0001;  // 进程槽位空闲
        const NO_PROC       = 0x0002;  // 无进程（伪进程）
        const SENDING       = 0x0004;  // 正在发送
        const RECEIVING     = 0x0008;  // 正在接收
        const SIGNALED      = 0x0010;  // 有信号待处理
        const SIG_PENDING   = 0x0020;  // 信号挂起
        const P_STOP        = 0x0040;  // 被调试器停止
        const NO_PRIV       = 0x0080;  // 无特权
        const NO_ENDPOINT   = 0x0100;  // 无端点
        const VMINHIBIT     = 0x0200;  // VM 禁止调度
        const PAGEFAULT     = 0x0400;  // 页错误处理中
    }
}
```

### 状态转换（隐式状态机）

```rust
fn rts_set(proc: ProcId, flags: RtsFlags) {
    let was_runnable = is_runnable(proc);
    proc.rts_flags |= flags;
    let now_runnable = is_runnable(proc);
    
    if was_runnable && !now_runnable {
        dequeue(proc);  // 自动出队！
    }
}

fn rts_unset(proc: ProcId, flags: RtsFlags) {
    let was_runnable = is_runnable(proc);
    proc.rts_flags &= !flags;
    let now_runnable = is_runnable(proc);
    
    if !was_runnable && now_runnable {
        enqueue(proc);  // 自动入队！
    }
}

fn is_runnable(proc: ProcId) -> bool {
    (proc.rts_flags & !(SIGNALED | SIG_PENDING)) == 0
}
```

### 不变量 ⚠️

1. **自动调度**: `RTS_SET` 可能触发 `dequeue()`，`RTS_UNSET` 可能触发 `enqueue()`
2. **非阻塞标志**: `SIGNALED` 和 `SIG_PENDING` 不影响可运行性
3. **原子性**: 状态变更必须是原子的（BKL 保护）

### Minix3 实现

| 文件 | 函数 |
|------|------|
| `kernel/proc.h` | `RTS_SET`, `RTS_UNSET` 宏 |
| `kernel/proc.c` | `enqueue()`, `dequeue()` |

### 设计缺陷

1. **隐式副作用**: 状态变更自动触发调度操作，难以追踪
2. **状态爆炸**: 多个标志位组合，难以穷举所有可能状态
3. **无类型安全**: 位操作容易出错

### Redesign 建议

```rust
enum ProcState {
    Free,
    Runnable,
    Sending { dst: ProcId },
    Receiving { src_filter: SrcFilter },
    Stopped { reason: StopReason },
    PageFault { addr: VirtAddr },
}

struct Process {
    state: ProcState,
    pending_signals: Vec<Signal>,
}
```

---

## 🔥 模块 D: 中断 → 事件 → IPC

### 数据流

```
硬件中断
    │
    ▼
IDT 入口 (mpx.S)
    │
    ▼
irq_handle() (interrupt.c)
    │
    ├─> 调用用户注册的钩子
    │       │
    │       ▼
    │   mini_notify(dst)  ──> 发送通知给驱动进程
    │
    ▼
中断返回
    │
    ▼
驱动进程被唤醒
    │
    ▼
驱动处理事件
```

### 最小接口

```rust
fn put_irq_handler(irq: u8, handler: Box<dyn Fn()>) -> HandlerId;
fn rm_irq_handler(irq: u8, id: HandlerId);
fn enable_irq(irq: u8);
fn disable_irq(irq: u8);
```

### 状态

```rust
struct IrqState {
    handlers: [Vec<IrqHandler>; NR_IRQ_VECTORS],  // 共享中断支持
    enabled_bitmap: u32,
    spurious_count: [u32; NR_IRQ_VECTORS],  // 虚假中断计数
}
```

### 不变量 ⚠️

1. **共享中断**: 一个 IRQ 可能有多个处理程序，必须全部轮询
2. **位图 ID**: HandlerId = 1, 2, 4, 8...（最多 32 个处理程序）
3. **调用者内存**: 钩子结构体由调用者提供，内核不分配

### Minix3 实现

| 文件 | 函数 |
|------|------|
| `kernel/interrupt.c` | `put_irq_handler()`, `rm_irq_handler()`, `irq_handle()` |
| `kernel/arch/i386/mpx.S` | IDT 入口 |

### 设计缺陷

1. **无优先级**: 共享中断的处理程序按注册顺序轮询，无优先级
2. **虚假中断处理粗糙**: 指数退避报告，但无根本解决方案
3. **中断上下文限制**: 处理程序不能阻塞，只能发送 notify

---

## 🔥 模块 E: 时间子系统

### 最小接口

```rust
fn get_uptime() -> Duration;
fn set_timer(proc: ProcId, timer: TimerId, expire: Duration);
fn cancel_timer(proc: ProcId, timer: TimerId);
```

### 状态

```rust
struct TimeState {
    uptime: Duration,
    timers: BTreeMap<Duration, Vec<Timer>>,  // 按过期时间排序
    load_history: CircularBuffer<LoadSample>,  // 负载统计
}
```

### 不变量 ⚠️

1. **BILLABLE 机制**: 微内核服务时间归属用户进程
2. **虚拟定时器**: ITIMER_VIRTUAL 只计算用户态时间
3. **多核同步**: BSP 维护全局时间，AP 只维护本地定时器

### Minix3 实现

| 文件 | 函数 |
|------|------|
| `kernel/clock.c` | `timer_int_handler()`, `set_kernel_timer()` |

### 设计缺陷

1. **NTP 调整粗糙**: 每 2 tick 调整 1 tick，可能引入抖动
2. **负载统计粒度**: 150 槽位 × 6 秒，粒度较粗

---

## 📊 已发现的设计缺陷汇总

| 模块 | 缺陷 | 影响 | Redesign 方向 |
|------|------|------|---------------|
| 调度 | BKL 限制并发 | SMP 性能差 | 无锁调度队列 |
| 调度 | 无负载均衡 | SMP 负载不均 | 全局调度器 + 负载迁移 |
| IPC | 无超时机制 | 可能永久阻塞 | `send_timeout()` |
| IPC | 无死锁检测 | 循环等待无法发现 | 死锁检测算法 |
| IPC | 异步消息语义混乱 | Bug 风险 | 状态机重构 |
| RTS | 隐式副作用 | 难以追踪 | 显式状态机 |
| RTS | 状态爆炸 | 难以穷举测试 | 类型安全枚举 |
| 中断 | 无优先级 | 延迟不可控 | 优先级队列 |
| 时间 | NTP 调整粗糙 | 时间抖动 | 平滑调整算法 |

---

## 🎯 Rewrite 路线图

### 阶段 1: IPC 核心（无 kernel）

```rust
struct Process {
    id: ProcId,
    state: ProcState,
    message: Option<Message>,
}

struct IpcCore {
    processes: Vec<Process>,
    send_queue: HashMap<ProcId, VecDeque<ProcId>>,
    receive_queue: HashMap<ProcId, VecDeque<ProcId>>,
}

impl IpcCore {
    fn send(&mut self, src: ProcId, dst: ProcId, msg: Message) -> Result<(), IpcError>;
    fn receive(&mut self, dst: ProcId, src_filter: SrcFilter) -> Result<Message, IpcError>;
}
```

**测试点**:
- 死锁检测
- 顺序一致性
- 零拷贝正确性

### 阶段 2: 调度核心

```rust
struct Scheduler {
    run_queue: [VecDeque<ProcId>; NR_PRIORITIES],
}

impl Scheduler {
    fn enqueue(&mut self, proc: ProcId);
    fn dequeue(&mut self, proc: ProcId);
    fn pick_next(&self) -> Option<ProcId>;
}
```

### 阶段 3: RTS 状态机

```rust
enum ProcState {
    Runnable,
    Sending { dst: ProcId },
    Receiving { src_filter: SrcFilter },
    Stopped,
}

impl ProcState {
    fn is_runnable(&self) -> bool {
        matches!(self, ProcState::Runnable)
    }
}
```

---

## 📝 待补充

- [ ] 模块 F: 内存管理（VM 相关）
- [ ] 模块 G: 信号处理
- [ ] 模块 H: SMP 同步（IPI、spinlock）
- [ ] 性能关键路径分析
- [ ] 安全边界分析

---

*创建于: 2025-03-27*
*基于: progress.md 阅读进度 + GPT 建议重构*
