# 07-scheduling: 进程调度

> **分类**: Kernel 进程抽象
> **源码**: `minix3/minix/kernel/proc.c` 调度部分(~700行)
> **说明**: pick_proc / enqueue / dequeue / switch_to_user / idle / 优先级队列——决定谁运行、运行多久

---

## 1. 概述

### 1.1 概念定义/作用

**进程调度**是操作系统内核的核心职责之一：在多个进程竞争 CPU 时，决定哪个进程获得 CPU 时间、运行多久、何时被替换。Minix3 的调度器采用**多级优先级队列**（Multi-Level Feedback Queue）策略，核心设计原则是：

1. **优先级驱动**：高优先级进程总是先于低优先级进程运行，同优先级按 FIFO 顺序
2. **可抢占**：当更高优先级进程就绪时，当前进程可被抢占
3. **时间片轮转**：每个进程分配一个时间量子（quantum），用完后让出 CPU
4. **用户空间调度器**：支持将调度策略委托给用户空间调度进程

Minix3 将调度机制（队列操作、上下文切换）与调度策略（优先级分配、时间片大小）分离。内核实现机制，策略部分可由用户空间调度器定制。

### 1.2 与 Minix3 的对应关系

Minix3 的调度实现在 `proc.c` 中，核心函数对应关系如下：

| 功能 | 函数 | 位置 |
|------|------|------|
| 选择下一个运行进程 | `pick_proc()` | proc.c:1785 |
| 进程入队 | `enqueue()` | proc.c:1595 |
| 进程入队（队首） | `enqueue_head()` | proc.c:1670 |
| 进程出队 | `dequeue()` | proc.c:1716 |
| 切换到用户态 | `switch_to_user()` | proc.c:299 |
| 空闲循环 | `idle()` | proc.c:176 |
| 时间片耗尽处理 | `proc_no_time()` | proc.c:1893 |
| 通知用户空间调度器 | `notify_scheduler()` | proc.c:1860 |
| 重置调度统计 | `reset_proc_accounting()` | proc.c:1912 |

调度队列数据结构定义在 `cpulocals.h` 中，每个 CPU 有独立的 `run_q_head[NR_SCHED_QUEUES]` 和 `run_q_tail[NR_SCHED_QUEUES]` 数组。

### 1.3 关键状态/机制说明

**优先级队列结构**：Minix3 定义了 `NR_SCHED_QUEUES(16)` 个优先级队列，编号 0（最高）到 15（最低）。每个队列是一个单向链表，通过 `p_nextready` 指针串联。`pick_proc()` 从最高优先级队列开始扫描，返回第一个非空队列的队首进程。

**抢占机制**：`enqueue()` 在将进程加入队列后，会检查新进程的优先级是否高于当前运行进程。若当前进程可抢占（`PREEMPTIBLE` 标志），则设置 `RTS_PREEMPTED` 标志使其让出 CPU。

**时间片管理**：每个进程有 `p_cpu_time_left`（剩余 CPU 时间）和 `p_quantum_size_ms`（时间量子大小）。时间片耗尽时，由用户空间调度器管理的进程通过 `notify_scheduler()` 通知调度器；内核调度的进程则直接重置时间片。

**switch_to_user 主循环**：这是调度的核心入口，负责处理抢占恢复、选择新进程、投递待处理消息、恢复上下文等。它不是简单的"选进程→切换"，而是一个包含多个检查点的状态机。

### 1.4 行为规则

1. **严格优先级**：`pick_proc()` 始终选择最高优先级就绪队列的队首进程，低优先级进程只有在高优先级队列为空时才能运行
2. **FIFO 同优先级**：同优先级进程按到达顺序排列，新进程加到队尾（`enqueue`），被抢占的进程加到队首（`enqueue_head`）
3. **抢占公平性**：被抢占的进程通过 `enqueue_head()` 放回队首，保证它在该优先级队列中下次优先运行
4. **时间片耗尽不抢占内核调度进程**：内核调度的进程（`proc_kernel_scheduler()` 为真）时间片耗尽后直接重置，不出队
5. **IDLE 兜底**：所有就绪队列为空时，CPU 进入 `idle()` 等待中断唤醒
6. **CPU 亲和性**：SMP 下每个进程有 `p_cpu` 字段，入队时加入对应 CPU 的队列

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 调度优先级常量

定义于 `minix3/minix/include/minix/config.h:63-74`：

| 常量 | 值 | 含义 |
|------|-----|------|
| `NR_SCHED_QUEUES` | 16 | 优先级队列数（必须等于最低优先级 + 1） |
| `TASK_Q` | 0 | 最高优先级，内核任务使用 |
| `MAX_USER_Q` | 0 | 用户进程最高优先级 |
| `USER_Q` | 7 | 用户进程默认优先级（对应 nice 0） |
| `MIN_USER_Q` | 15 | 用户进程最低优先级 |
| `USER_QUANTUM` | 200 | 默认用户进程时间量子（毫秒） |
| `USER_DEFAULT_CPU` | -1 | 默认 CPU 分配（-1 表示不改变当前 CPU） |

#### 2.1.2 调度相关 RTS 标志

| 标志 | 值 | 调度含义 |
|------|-----|---------|
| `RTS_PREEMPTED` | 0x4000 | 被更高优先级进程抢占，恢复时放回队首 |
| `RTS_NO_QUANTUM` | 0x8000 | 时间片耗尽，等待调度器分配新时间片 |

#### 2.1.3 调度相关特权标志

| 标志 | 值 | 调度含义 |
|------|-----|---------|
| `PREEMPTIBLE` | 0x002 | 进程可被抢占 |
| `BILLABLE` | 0x004 | 进程可被计费（`pick_proc` 设置 `bill_ptr`） |

#### 2.1.4 CPU 局部变量（调度相关）

定义于 `minix3/minix/kernel/cpulocals.h:37-75`：

| 字段 | 类型 | 含义 |
|------|------|------|
| `proc_ptr` | `struct proc *` | 当前运行进程指针 |
| `bill_ptr` | `struct proc *` | 计费进程指针（时钟中断用） |
| `idle_proc` | `struct proc` | IDLE 进程存根 |
| `run_q_head[NR_SCHED_QUEUES]` | `struct proc *` | 就绪队列头指针数组 |
| `run_q_tail[NR_SCHED_QUEUES]` | `struct proc *` | 就绪队列尾指针数组 |
| `cpu_is_idle` | `int` | CPU 是否空闲 |
| `fpu_owner` | `struct proc *` | FPU 当前所属进程 |

### 2.2 核心数据结构

#### 2.2.1 优先级就绪队列

Minix3 使用 16 个优先级队列，每个队列是一个以 `p_nextready` 串联的单向链表：

```
优先级 0 (TASK_Q):  proc_A → proc_B → NULL
优先级 1:           NULL
优先级 2:           proc_C → NULL
...
优先级 15 (MIN_USER_Q): proc_D → proc_E → NULL
```

- `run_q_head[q]`：指向队列 q 的第一个进程
- `run_q_tail[q]`：指向队列 q 的最后一个进程
- 空队列：`run_q_head[q] == NULL`

每个 CPU 有独立的队列集合（SMP 下通过 `get_cpu_var(cpu, run_q_head)` 访问）。

#### 2.2.2 调度统计子结构（p_accounting）

定义于 `minix3/minix/kernel/proc.h:48-55`，用于向用户空间调度器报告进程行为：

| 字段 | 类型 | 含义 |
|------|------|------|
| `enter_queue` | `u64_t` | 入队时刻（TSC 周期） |
| `time_in_queue` | `u64_t` | 队列中等待总时间 |
| `dequeues` | `unsigned long` | 出队次数 |
| `ipc_sync` | `unsigned long` | 同步 IPC 次数 |
| `ipc_async` | `unsigned long` | 异步 IPC 次数 |
| `preempted` | `unsigned long` | 被抢占次数 |

### 2.3 关键函数分析

#### 2.3.1 pick_proc()——选择下一个运行进程

`minix3/minix/kernel/proc.c:1785-1813`

```c
static struct proc * pick_proc(void)
```

**功能**：从当前 CPU 的就绪队列中选择最高优先级的可运行进程。

**行为**：
1. 从优先级 0（最高）开始，依次扫描 `run_q_head[0]` ~ `run_q_head[NR_SCHED_QUEUES-1]`
2. 找到第一个非空队列，取其队首进程
3. 若该进程是 BILLABLE 的，设置 `bill_ptr` 为该进程（用于时钟计费）
4. 返回该进程指针
5. 若所有队列都为空，返回 NULL

**关键约束**：返回的进程保证 `proc_is_runnable()` 为真（`p_rts_flags == 0`）。

#### 2.3.2 enqueue()——进程入队（尾部）

`minix3/minix/kernel/proc.c:1595-1659`

```c
void enqueue(register struct proc *rp)
```

**功能**：将可运行进程加入其优先级对应的就绪队列尾部。

**行为**：
1. 断言 `proc_is_runnable(rp)`——只有可运行进程才能入队
2. 取进程优先级 `q = rp->p_priority`，获取对应 CPU 的 `rdy_head` / `rdy_tail`
3. 若队列空：`rdy_head[q] = rdy_tail[q] = rp`，`rp->p_nextready = NULL`
4. 若队列非空：`rdy_tail[q]->p_nextready = rp`，`rdy_tail[q] = rp`，`rp->p_nextready = NULL`
5. **抢占检查**：若新进程优先级高于当前进程（`p->p_priority > rp->p_priority`）且当前进程可抢占，则 `RTS_SET(p, RTS_PREEMPTED)` 使当前进程让出 CPU
6. SMP 下：若进程入队到其他 CPU 且该 CPU 空闲，调用 `smp_schedule()` 唤醒
7. 记录入队时刻到 `p_accounting.enter_queue`

**注意**：优先级数值越小越高，因此 `p->p_priority > rp->p_priority` 表示当前进程优先级低于新进程。

#### 2.3.3 enqueue_head()——进程入队（头部）

`minix3/minix/kernel/proc.c:1670-1711`

```c
static void enqueue_head(struct proc *rp)
```

**功能**：将被抢占的进程放回就绪队列头部，保证公平性。

**行为**：
1. 断言进程可运行且 `p_cpu_time_left > 0`（被抢占的进程还有剩余时间片）
2. 若队列空：同 `enqueue()`
3. 若队列非空：`rp->p_nextready = rdy_head[q]`，`rdy_head[q] = rp`（插入队首）
4. 更新统计：`dequeues--`，`preempted++`

**与 enqueue 的区别**：`enqueue` 加到队尾（新进程排队），`enqueue_head` 加到队首（被抢占进程优先恢复）。

#### 2.3.4 dequeue()——进程出队

`minix3/minix/kernel/proc.c:1716-1780`

```c
void dequeue(struct proc *rp)
```

**功能**：将不可运行进程从就绪队列中移除。

**行为**：
1. 断言 `!proc_is_runnable(rp)`——只有不可运行进程才出队
2. 内核任务额外检查栈保护字 `STACK_GUARD`
3. 遍历对应优先级队列的链表，找到 `rp` 后将其从链表中摘除
4. 若 `rp` 是队尾，更新 `rdy_tail[q]` 为前驱节点
5. 更新统计：`dequeues++`，计算 `time_in_queue` 增量
6. 记录出队时刻 `p_dequeued = get_monotonic()`

**使用指针指针（pointer pointer）模式**：遍历时使用 `struct proc **xpp`，避免对队首节点的特殊处理。

#### 2.3.5 switch_to_user()——切换到用户态

`minix3/minix/kernel/proc.c:299-474`

```c
void switch_to_user(void)
```

**功能**：调度的主入口，在内核处理完中断/系统调用后调用，选择并切换到下一个用户进程。

**行为**（按执行顺序）：

1. **检查当前进程**：若当前进程仍可运行，跳到 `check_misc_flags` 处理杂项标志
2. **处理抢占**：若当前进程被抢占（`RTS_PREEMPTED`），清除抢占标志，若有剩余时间片则 `enqueue_head`（放回队首），否则 `enqueue`（加到队尾）
3. **选择新进程**：循环调用 `pick_proc()`，若无就绪进程则 `idle()` 等待中断
4. **切换地址空间**：`switch_address_space(p)`
5. **处理杂项标志**（循环处理，直到所有标志清零）：
   - `MF_KCALL_RESUME`：恢复被中断的内核调用
   - `MF_DELIVERMSG`：投递待传递消息
   - `MF_SC_DEFER`：执行延迟的系统调用
   - `MF_SC_TRACE`：触发系统调用追踪事件
   - `MF_SC_ACTIVE`：清除系统调用活跃标志
6. **时间片检查**：若 `p_cpu_time_left == 0`，调用 `proc_no_time()`
7. **最终检查**：处理完所有标志后再次确认进程可运行
8. **恢复上下文**：`arch_finish_switch_to_user()` → FPU 处理 → `restore_user_context(p)`（不返回）

**关键设计**：`switch_to_user` 不是一个简单的"选进程→切换"操作，而是一个状态机。处理杂项标志时可能导致进程变为不可运行（如消息投递触发页缺失），此时需跳回 `not_runnable_pick_new` 重新选择进程。

#### 2.3.6 idle()——空闲循环

`minix3/minix/kernel/proc.c:176-232`

```c
static void idle(void)
```

**功能**：当没有可运行进程时，将 CPU 置于低功耗状态等待中断。

**行为**：
1. 设置 `proc_ptr` 为 IDLE 进程
2. 若 IDLE 进程是 BILLABLE 的，设置 `bill_ptr`
3. 切换到 VM 进程的地址空间（SMP 下确保内核映射可用）
4. 设置 `cpu_is_idle = 1`（SMP）
5. 停止本地定时器（AP 上不需要计时）
6. 调用 `halt_cpu()` 使 CPU 进入低功耗状态
7. 中断唤醒后恢复执行，返回到 `switch_to_user` 的 `pick_proc()` 循环

#### 2.3.7 proc_no_time()——时间片耗尽处理

`minix3/minix/kernel/proc.c:1893-1910`

```c
void proc_no_time(struct proc *p)
```

**功能**：进程时间片耗尽时的处理，分两种策略。

**行为**：
- **用户空间调度器管理的进程**（`!proc_kernel_scheduler(p) && PREEMPTIBLE`）：调用 `notify_scheduler()` 通知调度器，进程被 dequeue 并设置 `RTS_NO_QUANTUM`
- **内核调度的进程**：直接重置时间片 `p_cpu_time_left = ms_2_cpu_time(p->p_quantum_size_ms)`，进程继续运行

#### 2.3.8 notify_scheduler()——通知用户空间调度器

`minix3/minix/kernel/proc.c:1860-1891`

```c
static void notify_scheduler(struct proc *p)
```

**功能**：向进程的用户空间调度器发送 `SCHEDULING_NO_QUANTUM` 消息，携带调度统计信息。

**行为**：
1. 设置 `RTS_NO_QUANTUM` 使进程出队
2. 构造 `SCHEDULING_NO_QUANTUM` 消息，包含：
   - 进程 endpoint
   - 队列等待时间、出队次数、同步/异步 IPC 次数、抢占次数
   - CPU 编号、CPU 负载
3. 重置进程调度统计 `reset_proc_accounting(p)`
4. 通过 `mini_send()` 以内核身份发送消息给调度器

### 2.4 调用关系/调用点分析

#### 2.4.1 调度核心调用链

```
中断/系统调用返回
  └─ switch_to_user()
       ├─ [当前进程被抢占?]
       │    ├─ enqueue_head(p)  // 有剩余时间片
       │    └─ enqueue(p)       // 无剩余时间片
       ├─ pick_proc()           // 选择新进程
       │    └─ 遍历 run_q_head[0..15]
       ├─ [无就绪进程?]
       │    └─ idle()           // CPU 空闲
       ├─ switch_address_space(p)
       ├─ [处理 misc_flags]
       │    ├─ kernel_call_resume()
       │    ├─ delivermsg()
       │    └─ arch_do_syscall()
       ├─ [时间片耗尽?]
       │    └─ proc_no_time()
       │         ├─ notify_scheduler()  // 用户空间调度
       │         └─ 重置时间片           // 内核调度
       └─ restore_user_context(p)  // 不返回
```

#### 2.4.2 enqueue / dequeue 调用点

**enqueue 调用点**：

| 调用者 | 场景 |
|--------|------|
| `RTS_UNSET` 宏 | 进程从不可运行变为可运行时自动调用 |
| `switch_to_user()` | 被抢占进程无剩余时间片时重新入队 |
| `enqueue_head()` | 被抢占进程有剩余时间片时放回队首 |

**dequeue 调用点**：

| 调用者 | 场景 |
|--------|------|
| `RTS_SET` 宏 | 进程从可运行变为不可运行时自动调用 |
| `RTS_SETFLAGS` 宏 | 直接设置标志值时若进程可运行且新值非 0 |

#### 2.4.3 时钟中断与调度

```
时钟中断
  └─ timer_int_handler()
       ├─ 更新 uptime / realtime
       ├─ 检查进程定时器（虚拟/profile）
       ├─ load_update()         // 更新负载平均
       └─ [当前进程时间片耗尽?]
            └─ proc_no_time()
```

### 2.5 设计要点/特殊处理

#### 2.5.1 机制与策略分离

Minix3 的调度设计明确区分了**机制**（mechanism）和**策略**（policy）：

- **机制**（内核实现）：`enqueue` / `dequeue` / `pick_proc` / `switch_to_user`——这些函数决定了"如何"管理队列和切换进程
- **策略**（可定制）：优先级分配、时间片大小——可由用户空间调度器（`p_scheduler`）决定

`proc_kernel_scheduler(p)` 宏判断进程是否由内核默认调度：`p->p_scheduler == NULL || p->p_scheduler == p`。非内核调度的进程时间片耗尽时，内核通过 IPC 通知其调度器，由调度器决定新的优先级和时间片。

#### 2.5.2 enqueue_head 的公平性保证

被抢占的进程通过 `enqueue_head()` 放回队首而非队尾，这是关键的设计选择。原因：

1. 被抢占不是进程的"错"——它还有剩余时间片，不应被惩罚
2. 放回队首保证它在同优先级进程中最先恢复运行
3. 断言 `p_cpu_time_left > 0` 确保只有还有时间片的进程才能用 `enqueue_head`

对比：时间片耗尽的进程（`RTS_NO_QUANTUM`）重新入队时用 `enqueue()` 加到队尾——它已经用完了自己的时间份额。

#### 2.5.3 switch_to_user 的状态机设计

`switch_to_user()` 不是线性流程，而是一个包含多个回跳点的状态机：

- `not_runnable_pick_new`：处理抢占后选择新进程
- `check_misc_flags`：循环处理杂项标志
- 每个标志处理后都检查进程是否仍可运行，不可运行则跳回重新选择

这种设计是因为处理杂项标志（如投递消息、恢复内核调用）可能触发新的阻塞（如页缺失），需要重新调度。

#### 2.5.4 指针指针（Pointer Pointer）模式

`dequeue()` 使用 `struct proc **xpp` 遍历链表，这是 Minix3 内核代码的标志性模式：

```c
for (xpp = &rdy_head[q]; *xpp; xpp = &(*xpp)->p_nextready) {
    if (*xpp == rp) {
        *xpp = (*xpp)->p_nextready;  // 直接修改前驱的 next 指针
        break;
    }
}
```

优势：无需对队首节点做特殊处理，删除操作统一为 `*xpp = (*xpp)->p_nextready`。

#### 2.5.5 SMP 调度扩展

SMP 配置下，调度器有以下扩展：

1. **CPU 局部队列**：每个 CPU 有独立的 `run_q_head` / `run_q_tail`，避免跨 CPU 锁竞争
2. **跨 CPU 入队唤醒**：`enqueue()` 中若进程入队到空闲 CPU，调用 `smp_schedule()` 唤醒
3. **CPU 亲和性**：进程通过 `p_cpu` 绑定到特定 CPU，入队时加入对应 CPU 的队列
4. **TLB 一致性**：`switch_to_user()` 中检查 `MF_FLUSH_TLB`，必要时刷新 TLB

#### 2.5.6 IDLE 进程的特殊处理

IDLE 进程是每个 CPU 的"兜底"进程，有特殊处理：

1. 初始化时设置 `RTS_PROC_STOP`，永远不会被 `pick_proc()` 选中
2. `idle()` 函数手动设置 `proc_ptr` 为 IDLE 进程，绕过正常调度
3. IDLE 进程共享一个 `idle_priv` 结构，标志为 `IDL_F`
4. SMP 下每个 CPU 有独立的 `idle_proc` 实例
