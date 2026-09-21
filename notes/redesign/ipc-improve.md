# IPC 设计改进思考笔记

> 本笔记记录关于 Minix3、L4、Rust 与微内核 IPC 设计的讨论与思考。

---

## 第一部分：BKL（Big Kernel Lock）设计反思

### 1.1 初印象：BKL 很丑

BKL 这种设计非常丑，非常非常。SMP 系统，只有一个进程能进内核，这简直不可想象——笔记本电脑都16核了，服务器都上百核了。

我理解这是时代的局限，但也是偷懒的设计。这不和 GIL 一样么，来个大锁，回避了一切问题。

### 1.2 深入理解：BKL 是阶段性设计

**结论：BKL 不是偷懒，而是阶段性设计**

BKL 的真实意义是：
> **先让系统能在 SMP 上跑起来，再逐步细化锁。**

这是很多内核的真实演化路径：

```
单核内核
   ↓
加一个全局锁 (BKL)
   ↓
系统能在 SMP 跑
   ↓
逐渐拆锁 (fine-grained locking)
   ↓
NUMA / lock-free / RCU
```

如果一开始就做 fine-grained locking：
- 复杂度爆炸
- bug爆炸
- 调试地狱

所以 BKL 是一种 **bootstrap strategy**。

### 1.3 Linux 也经历过 BKL

**Linux 曾经也有 BKL**，而且存在了 **十几年**。

历史时间线：
- 1990s：Linux 单核
- 1996：Linux SMP + BKL
- 2000s：开始拆 BKL
- 2011：Linux 2.6.39 完全移除 BKL

Linux 花了 **15 年** 才完全摆脱 BKL。

### 1.4 MINIX 的情况更特殊

MINIX 是 microkernel，绝大部分逻辑不在 kernel。

结构是：
```
kernel
  ↓ IPC
servers
  ↓
drivers
```

kernel 本身非常小（~12000 行），相比 Linux kernel（3000万行+）。

因此 MINIX kernel BKL 的影响其实不大，因为大部分工作在 user-space server。

### 1.5 关键点：BKL 不等于系统只能用一个核

BKL 限制的是 **kernel critical section**，而不是整个系统。

例如：
```
CPU0  running server
CPU1  running server
CPU2  running user process
CPU3  running user process
```

只有当进入 kernel 时才获取 BKL，才会串行。

所以在 microkernel 里，并发仍然很多。

### 1.6 为什么 microkernel 更容易接受 BKL

microkernel 的原则是 **minimize kernel**，kernel 只做：
- IPC
- scheduler
- low-level memory
- interrupt

这些操作本身就很短，所以 **BKL hold time 非常短**。

相比 Linux：filesystem、network、driver、vm 都在 kernel，critical section 非常长。

### 1.7 现代 microkernel 的做法

新一代 microkernel（比如 seL4）已经不这么干了：
- fine-grained locking
- per-core kernel
- formal verification

但注意：seL4 kernel 只有 ~9000 行，所以才可行。

### 1.8 GIL 是类似设计

Python GIL 的设计逻辑：
- CPython 解释器状态太复杂
- 多线程安全成本太高

所以一个大锁，换来的好处：
- 实现简单
- 性能稳定
- bug少

代价：多核利用差

### 1.9 工程现实：很多系统都有"隐形 BKL"

即使是现代系统：
- global allocator lock
- GC stop-the-world
- filesystem journal lock
- database global latch

很多系统都存在隐式大锁，只是名字不同。

### 1.10 Tanenbaum 的真实目标

MINIX3 的目标不是最高性能，而是：
> **可靠性**

目标是：
- driver crash → restart
- server crash → restart
- system继续运行

所以：**simplicity > performance**

BKL 其实是这个哲学的一部分。

### 1.11 质疑设计是好事

感觉"丑"其实是好事，说明已经在用 **现代系统工程视角** 看代码。

很多人读 kernel 只会觉得"好复杂"，而不会问：
- 为什么这么设计？
- 有没有更好的方式？

这其实已经是 **系统研究者的路径**。

---

## 第二部分：MINIX3 IPC 并发问题分析

### 2.1 背景

在 MINIX 3 中，系统服务（drivers、servers）运行在用户态，通过 **内核 IPC** 与彼此通信。

MINIX3 提供两类消息发送机制：

1. **同步 IPC**：send、receive、sendrec
2. **异步 IPC**：asynsend（或 senda）

### 2.2 同步 IPC

同步 IPC 的核心特性：
- `send` 时发送者阻塞
- `receive` 时由内核匹配
- 内核维护 **发送队列**

简化模型：
```
Sender ---> Kernel Queue ---> Receiver
```

特点：
- 内核完全控制队列
- 发送方不会并发修改消息
- 不存在用户态竞争

因此 **同步 IPC 的一致性是内核保证的**。

### 2.3 异步 IPC (`senda`)

`senda` 的设计目标：
- 允许 **非阻塞发送**
- 允许 **批量发送请求**
- 减少系统调用次数

用户态构造一个 `asynmsg` 数组：
```c
struct asynmsg {
    endpoint_t dst;
    message msg;
    int flags;
}
```

调用：`senda(array, count)`

语义：
- 内核遍历数组
- 将每个消息投递给目标进程
- 发送方 **不会阻塞**

这带来一个新的复杂性：**异步发送队列不再完全由内核管理。**

### 2.4 cancel_async 机制

为了支持异步消息取消，MINIX3 提供：
```
cancel_async(endpoint)
```

作用：删除某个目标进程的未投递异步消息。

典型流程：
```
Process A: senda -> server
Process B: cancel_async(server)
```

### 2.5 并发问题

真正的问题：
```
Process A: senda
Process B: cancel_async
```

这两个 **server 进程完全可能并发执行**。

而 MINIX3 的设计假设：
- server 是可信组件
- server 不会恶意并发破坏结构

但在实际执行中：
```
CPU1: senda 正在插入 async queue
CPU2: cancel_async 正在删除节点
```

可能出现：
- 指针竞争
- 队列损坏
- 数据结构不一致

这种情况属于典型的：**并发修改共享队列**

### 2.6 为什么 MINIX3 没完全解决

**原因主要有三个：**

#### 1. 设计假设
MINIX3 假设 `server = trusted`，因此没有像用户程序那样做严格并发保护。

换句话说：**server 的行为被视为系统内部行为。**

#### 2. 单进程 server 模型
很多 server 实际是 single-threaded，因此理论上不会自己并发。但 **跨 server 的并发依然存在**。

#### 3. 历史设计
MINIX3 的 IPC 设计来自早期微内核思想，目标是：
- 代码简单
- 容易验证
- 易于教学

而不是：
- 极致并发性能
- 多核极限扩展

### 2.7 与 Big Kernel Lock 的类比

这种设计与 **Big Kernel Lock** 很类似。

早期 Linux kernel 使用 BKL：
```
enter_kernel()
   lock(BKL)
   ...
   unlock(BKL)
```

效果：SMP 系统，但同一时间只有一个 CPU 在内核。

优点：
- 实现简单
- 容易保证一致性

缺点：
- 扩展性差
- 多核性能浪费

### 2.8 现代改进设计思路

如果重新设计 async IPC，可以考虑：

#### 1. 内核完全拥有队列
原则：**async queue 只由 kernel 修改**

用户态只能：
- `enqueue_request()`
- `cancel_request()`

而不是直接操作结构。

#### 2. lock-free 或 fine-grained lock
例如：**per-endpoint async queue**

每个 server 拥有独立队列：
```
server1 queue
server2 queue
server3 queue
```

这样：
```
senda -> lock(server queue)
cancel_async -> lock(server queue)
```

锁粒度更小。

#### 3. message id
每个 async message 分配 `msg_id`，取消操作：`cancel(msg_id)`

而不是：`cancel(endpoint)`

这样避免大量扫描。

### 2.9 设计权衡

| 设计 | 优点 | 缺点 |
|------|------|------|
| 当前 MINIX3 | 简单 | 并发弱 |
| kernel queue | 一致性好 | 内核复杂 |
| lock-free | 性能高 | 实现难 |
| msg_id cancel | 精确 | 状态维护多 |

MINIX3 的选择是：
> **优先简单性和教学价值，而不是极限并发性能。**

### 2.10 小结

MINIX3 的 async IPC 设计存在潜在并发问题，核心原因是 `senda` 和 `cancel_async` 可能在不同 server 之间并发执行，而 async 队列结构缺乏严格同步保护。

这并不是一个简单的 bug，而是：**早期微内核设计在 simplicity 与 concurrency 之间的权衡。**

---

## 第三部分：L4 IPC 设计解析（对比 MINIX3）

### 3.1 设计目标

**L4 microkernel family** 的设计哲学非常激进：
> **IPC 必须和函数调用一样快。**

L4 的作者认为：
```
microkernel = message passing
```

如果 IPC 很慢：`microkernel == 失败`

因此 L4 的所有设计都围绕：
- 减少内核状态
- 减少队列
- 减少调度
- 减少锁

### 3.2 L4 的核心思想：同步 IPC

L4 **几乎完全放弃异步 IPC**。

只有一种核心原语：`ipc(dest)`

语义：`send + receive`，类似 `sendrec()`

执行流程：
```
client ----IPC----> server
           |
           V
        block
```

服务器处理请求后：`server ----reply----> client`，client 被唤醒。

### 3.3 Rendezvous 模型

L4 使用 **rendezvous（会合）通信模型**。

意思是：**sender 和 receiver 必须同时准备好**

流程：
```
Sender -> block
Receiver -> receive
Kernel -> match
```

匹配后：**直接 copy message**，然后 Sender wake，Receiver run。

关键点：**没有持久消息队列**

### 3.4 为什么不需要消息队列

在 MINIX3：`sender -> queue -> receiver`

在 L4：`sender -> receiver`

内核只做：`thread state matching`，而不是 `queue management`

因此不存在：
- enqueue
- dequeue
- cancel

也就没有：**queue corruption**

### 3.5 直接进程切换（Direct Process Switch）

L4 的 IPC 还有一个非常著名的优化：**direct process switch**

流程：`client -> ipc(server)`

内核：`client -> server`，**直接切换到 server 线程执行**

而不是：`client -> scheduler -> server`

因此：`IPC + context switch` 可以合并为一次操作。

### 3.6 为什么性能会极高

#### 1. 几乎没有内核数据结构
没有：
- message queue
- async queue
- cancel list

只有：`thread state`

#### 2. copy 非常小
典型 IPC：`<= 64 bytes`，直接通过寄存器传递。

#### 3. 调度融合
IPC 本身就是调度，`ipc -> server` 等价于 `schedule(server)`

### 3.7 避免并发问题

MINIX3 问题：`senda` 和 `cancel_async` 并发修改队列。

在 L4 中：**不存在 async queue**

因此不存在：**queue race**

唯一状态是：`thread blocked`、`thread ready`

这些状态都在 **thread control block** 中，而 TCB 修改是在 **内核临界区**完成。

### 3.8 L4 的代价

L4 的设计不是没有代价。

#### 1. 不支持真正 async
如果 client 想发多个请求，必须：
- thread pool，或者
- event loop（在用户态实现）

#### 2. server 必须设计良好
因为 server 如果慢，`client 会阻塞`，因此 server 需要 multi-thread。

#### 3. 编程模型更复杂
很多传统 OS API（async IO、signal）需要重新设计。

### 3.9 L4 的极简原则

L4 的作者提出一个非常著名的原则：
> **kernel policy-free**

内核只提供：
- thread
- address space
- ipc

其他全部在 **user space** 实现。

### 3.10 与 MINIX3 的哲学差异

| 设计 | MINIX3 | L4 |
|------|--------|-----|
| IPC | sync + async | almost only sync |
| message queue | 有 | 没有 |
| cancel | 有 | 没有 |
| server | 简单 | 必须高质量 |
| kernel state | 多 | 极少 |
| concurrency | 一般 | 极好 |

总结一句话：
```
MINIX3 = 工程友好
L4 = 极致极简
```

### 3.11 现代 L4

目前最先进的 L4 实现是 **seL4**：
- 形式化验证
- IPC 仍然是 rendezvous
- kernel 约 **1万行代码**

它继承的正是 **L4 IPC 模型**。

### 3.12 小结

L4 通过三个核心设计避免了 MINIX3 的 IPC 并发问题：
1. **取消 async IPC**
2. **取消 message queue**
3. **采用 rendezvous 通信**

因此：`IPC = thread state change`，而不是 `IPC = queue manipulation`

这也是为什么 L4 系统的 IPC 可以做到：**接近函数调用的性能。**

---

## 第四部分：历史花边 - Tanenbaum vs L4 作者的经典争论

### 4.1 争论背景

在微内核历史中，有一场不太正式但非常经典的"思想冲突"：
```
Andrew Tanenbaum  (MINIX)
        vs
Jochen Liedtke    (L4)
```

两人的争论核心其实只有一个问题：
> **微内核 IPC 应该怎么设计？**

### 4.2 Tanenbaum 的设计哲学

Andrew S. Tanenbaum 是 **MINIX** 的作者。

MINIX 3 的设计目标其实非常明确：**教育 + 可维护性 + 可靠性**

因此 Tanenbaum 的微内核设计强调：

#### 1. 可读性
代码应该：**简单、清晰、容易理解**

因此 MINIX IPC 设计为：send、receive、sendrec、senda、notify

支持：**同步 + 异步**

同时：**内核维护 message queue**

这种设计的优点是：
- 容易理解
- server 编写简单
- 系统行为直观

但问题是：**IPC 很慢**

在早期 MINIX 中：一次 IPC ≈ 数百到上千 cycles，甚至更多。

### 4.3 L4 作者的观点

Jochen Liedtke 是 **L4 microkernel** 的作者。

L4 microkernel family 的设计哲学几乎是 Tanenbaum 的反面：**performance first**

Liedtke 在 1993 年发表了一篇非常著名的论文：
> **Improving IPC by Kernel Design**

论文核心观点：
> 早期 microkernel 失败的原因不是思想错误，**而是 IPC 太慢**

### 4.4 Liedtke 对早期 microkernel 的批评

Liedtke 在论文中直接批评了当时的 microkernel（包括 Mach 和 MINIX）：

#### 1. message queue 太重
传统 microkernel：`sender -> queue -> receiver`

导致：enqueue、dequeue、locking、memory copy，复杂度非常高。

#### 2. 调度和 IPC 分离
传统设计：
```
send
scheduler
switch
```

而 L4 将两者合并：`IPC = schedule`

#### 3. 内核做了太多事情
早期 microkernel 内核维护：queue、timeout、buffer、async state

Liedtke 认为：**kernel should do almost nothing**

### 4.5 L4 的革命性结论

Liedtke 得出的结论非常激进：
> **microkernel 必须极简，否则必然失败**

因此 L4 的设计原则是：`remove everything`

最终 L4 内核只保留：threads、address spaces、ipc

没有：message queue、async message

### 4.6 IPC 性能的巨大差距

L4 的 IPC 性能在当时震惊了操作系统界。

典型测试结果：

| 系统 | IPC latency |
|------|-------------|
| Mach | ~100 µs |
| MINIX | ~50 µs |
| L4 | **~5 µs** |

提升接近 **10x**。

这直接证明：**microkernel 并不必然慢。**

### 4.7 Tanenbaum 的态度

有趣的是：Tanenbaum **从未真正反驳 L4 的性能观点**。

因为性能差距确实存在。

但 Tanenbaum 的重点始终不是性能，而是：**可靠性、可维护性、教育价值**

MINIX 的目标一直是：`teaching OS`，而不是 `production OS`

### 4.8 两种哲学的根本差异

这场争论本质上不是技术问题，而是：**设计哲学**

对比：

| | MINIX | L4 |
|---|-------|-----|
| 目标 | 教学 | 工业级 microkernel |
| 优先级 | 可读性 | 性能 |
| IPC | sync + async | sync only |
| kernel state | 较多 | 极少 |
| 复杂度 | 简单 | 极简但抽象 |

总结：
```
MINIX = readable microkernel
L4    = minimal microkernel
```

### 4.9 历史最终的走向

后来 microkernel 研究基本全部采用了 **L4 思想**。

现代 microkernel：
- seL4
- Fiasco
- Pistachio

全部继承：**L4 IPC model**，而不是 **MINIX IPC model**

### 4.10 有趣的历史反转

一个非常有趣的事实是：今天的 **MINIX3** 在很多方面 **也越来越接近 L4 思想**：
- 更少的 kernel state
- 更清晰的 IPC

虽然仍然保留 `async IPC`，但整体设计比早期 MINIX 已经更现代。

### 4.11 总结

这场"争论"其实没有真正的胜负。

因为两人解决的是 **不同问题**：

| 人 | 目标 |
|---|------|
| Tanenbaum | 教会学生 OS |
| Liedtke | 让 microkernel 真正可用 |

最终历史证明：
```
microkernel 要想成功
IPC 必须极快
```

而 L4 正是第一个做到这一点的系统。

---

## 第五部分：Tanenbaum 刻意不优化 IPC 的历史细节

### 5.1 一个很少被注意的事实

**Tanenbaum 刻意没有优化 MINIX 的 IPC**

Andrew S. Tanenbaum 在设计 MINIX 时，有一个非常明确的原则：
> **教学系统必须简单，而不是最快。**

在他的书《Operating Systems: Design and Implementation》以及一些论文里，他实际上表达过类似观点：
```
MINIX 的目标不是性能，而是清晰性。
```

换句话说：
```
MINIX 的 IPC 慢，并不是因为他不会写更快。
而是因为他不想写更复杂。
```

### 5.2 早期 MINIX IPC 为什么慢

MINIX 的 IPC 设计有几个明显"性能不友好"的选择：

#### 1. 每次 IPC 都要进入调度器
典型流程：
```
send
  -> block
  -> scheduler
  -> switch
receive
  -> unblock
```

但很多优化型 microkernel（后来的 L4）会做：**direct process switch**

也就是：`sender -> receiver`，**不经过完整 scheduler**。

#### 2. 使用 message struct copy
MINIX IPC：`copy message`

而后来 L4 设计是：**register IPC**，直接用 CPU 寄存器传递小消息，减少 memory copy。

#### 3. 内核维护较多状态
MINIX kernel 会维护：
- blocked state
- send queue
- receive queue
- async queue

这让代码结构清晰，但：cache miss、branch、locking 都会增加。

### 5.3 Tanenbaum 知不知道这些优化？

答案是：**他当然知道。**

因为：
- 他是操作系统领域顶级学者
- 他写过分布式系统
- 他参与过 Amoeba OS（Amoeba 的 RPC 和 IPC **其实是非常高性能的**）

所以他绝不是不会写。

### 5.4 为什么他仍然选择"慢"的设计？

原因其实非常现实：

#### 1. 教学优先
MINIX 的主要读者是：**学生**，而不是 **kernel hackers**

如果 IPC 设计像 L4：
- direct switch
- lazy scheduling
- register protocol

学生会：**完全看不懂**

#### 2. 代码必须能被逐行解释
MINIX 的一个设计原则：
> **每一行代码都能在课堂讲清楚**

因此：`simple > fast`

#### 3. 书和代码必须同步
MINIX 是 **"书驱动操作系统"**：
```
Operating Systems: Design and Implementation
           ↑
        MINIX code
```

如果为了性能写一堆 trick，书就很难解释。

### 5.5 这导致了一个历史现象

90年代很多研究人员看 MINIX 时会说：
```
microkernel is slow
```

但实际上他们看的只是：**teaching microkernel**，而不是 **optimized microkernel**

这也是为什么 Jochen Liedtke 后来会写那篇著名论文：
> **Improving IPC by Kernel Design**

核心观点就是：
> microkernel 不慢，**只是以前的实现太慢**

### 5.6 一个非常有趣的反转

后来历史证明：**Tanenbaum 是对的，Liedtke 也是对的**

两人解决的是 **不同问题**。

| 人 | 目标 |
|---|------|
| Tanenbaum | 教会学生 OS |
| Liedtke | 让 microkernel 真正可用 |

### 5.7 一个更有意思的细节

很多人不知道：今天 Intel 的 **VT-x / VMX** 设计里的一些思想：
- message passing
- isolation
- minimal kernel

其实和 **microkernel 思想非常接近**。

所以某种意义上：
```
microkernel 的理念
最终在硬件层实现了
```

---

## 第六部分：Rust 能让 microkernel 同时拥有 L4 的性能和 MINIX 的简单性吗？

### 6.1 问题核心

> **Rust 能不能让 microkernel 同时做到：L4 的性能 + MINIX 的简单性？**

答案是：**部分可以，但不能完全解决。**

原因要从三个层面看：
1. 语言
2. 内核结构
3. 硬件成本

### 6.2 Rust 确实解决了一大类"复杂性"

传统 microkernel 复杂的一个重要来源是：
```
C 需要手动保证大量不变量
```

例如：
- IPC buffer 生命周期
- capability ownership
- message queue 一致性
- endpoint 引用计数
- 锁顺序

在 C 中这些都依赖：**程序员脑子 + code review + 文档**

而 Rust 可以把很多 invariant **编译期表达出来**。

例如：
```rust
struct Endpoint<'a> {
    queue: &'a mut Queue,
}
```

Rust 可以保证：**同一时间只有一个 mutable reference**

这对 microkernel 是非常重要的，因为 IPC 队列是高度共享的数据结构。

### 6.3 Rust 也让 message passing 更安全

microkernel 的核心就是：send、receive、reply

在 C 中 message 通常是：
```c
struct message {
    int m_type;
    int m1_i1;
    ...
}
```

然后各种 copy。

Rust 可以写成：
```rust
enum Message {
    Read(ReadReq),
    Write(WriteReq),
    Notify,
}
```

优势：
1. 类型安全
2. 编译器保证 message layout
3. pattern match 简化 dispatch

这让 **代码可读性大幅提升**。

### 6.4 Rust 可以消灭大量锁

Rust 的另一个优势是：**ownership + borrow**

可以让你设计：**per-core kernel data**，而不是 **global kernel structures**

例如：
- per-core run queue
- per-core IPC queue
- per-core scheduler

这样：**无需 BKL**

这正是现代 kernel 的方向。

### 6.5 但 Rust 解决不了最大的问题

microkernel 性能瓶颈其实不是语言，而是：
```
context switch
TLB flush
cache miss
```

典型 IPC 成本：
```
send
trap
kernel entry
schedule
switch
TLB
switch
return
```

这和 C / Rust **几乎无关**。

L4 为什么快？因为 L4 的核心设计是：**direct process switch**

流程：`client -> ipc(server)`，内核：`client -> server`，**直接切换到 server 线程执行**

而不是：`client -> scheduler -> pick B`，减少一次调度。

### 6.6 L4 的极端优化其实很难保持"简单"

L4 microkernel 之所以快，是因为很多 trick：

#### 1. register IPC
message 直接走寄存器

#### 2. lazy scheduling
避免 scheduler 调用

#### 3. direct switch
sender -> receiver

#### 4. minimal kernel state
几乎没有队列结构。

这些优化会导致：**kernel invariants 复杂度大幅上升**

例如：
- 谁拥有 IPC endpoint
- 谁能 reply
- reply capability 生命周期

这些问题其实非常复杂。

### 6.7 Rust microkernel 的现实尝试

现在确实有一些 Rust microkernel 项目：

#### seL4 的 Rust 生态
seL4 kernel 仍然是 C，但：
- userland Rust
- capability model

#### Theseus OS
完全 Rust 写的 OS。

特点：**no shared mutable state**

但：性能还没达到 L4。

#### Redox OS
Rust 写的 microkernel-ish OS。

目标：**安全 + 简单**

但性能也不是 L4 级别。

### 6.8 真正的结论（很少人说清）

Rust 可以：
- 减少 bug
- 减少复杂性
- 提升可读性

但 Rust **不能让 microkernel magically 快起来**。

因为真正的性能瓶颈是：**hardware boundary crossing**，也就是：
```
user ↔ kernel
process ↔ process
```

### 6.9 一个非常现实的结果

现代 OS 设计其实在 **向 hybrid 收敛**。

例如：
- **XNU**（macOS/iOS）
- **Windows NT**

都是：`monolithic performance + microkernel ideas`

Linux 也是：加入 `io_uring`、`eBPF`、`userfaultfd`

这些其实都是：**减少 kernel boundary**

### 6.10 如果今天重新设计 microkernel

很多研究者认为理想结构是：
```
microkernel
+ shared memory IPC
+ capability
+ per-core scheduler
```

而不是：`pure message passing`

### 6.11 一个可能会很震惊的事实

**L4 作者后来也承认：**
> 纯 message passing microkernel 在现实系统中 **不一定是最优结构**

所以后来很多 L4 系统：**大量使用 shared memory**

### 6.12 现代操作系统正在慢慢变成 microkernel

甚至：**Linux 也在变**

但很多人没意识到这个趋势。

---

## 附录：关键概念速查

### BKL (Big Kernel Lock)
早期 SMP 内核使用的全局锁，保证同一时间只有一个 CPU 执行内核代码。

### Rendezvous 通信
发送方和接收方必须同时准备好才能通信，没有持久消息队列。

### Direct Process Switch
IPC 时直接切换到目标进程，不经过完整调度器。

### Capability
一种访问控制机制，表示对某个资源的权限。

### seL4
目前最先进的 L4 实现，经过形式化验证。

---

## 思考与待办

- [ ] 深入研究 seL4 的 capability 模型
- [ ] 对比 Rust async/await 与 microkernel IPC 的异同
- [ ] 思考：如果重新设计 Minix3 的 async IPC，如何平衡简单性和并发安全？
- [ ] 调研：现代 Linux 的 io_uring 和 eBPF 如何减少 kernel boundary crossing？

---

*最后更新：2026-03-05*
