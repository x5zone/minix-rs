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

**时间片管理**：每个进程有 `p_cpu_time_left`（剩余 CPU 时间）和 `p_quantum_size_ms`（时间量子大小）。时间片耗尽时，由用户空间调度器管理的可抢占进程通过 `notify_scheduler()` 通知调度器；内核调度的进程或不可抢占的进程则直接重置时间片继续运行。

**switch_to_user 主循环**：这是调度的核心入口，负责处理抢占恢复、选择新进程、投递待处理消息、恢复上下文等。它不是简单的"选进程→切换"，而是一个包含多个检查点的状态机。

### 1.4 行为规则

1. **严格优先级**：`pick_proc()` 始终选择最高优先级就绪队列的队首进程，低优先级进程只有在高优先级队列为空时才能运行
2. **FIFO 同优先级**：同优先级进程按到达顺序排列，新进程加到队尾（`enqueue`），被抢占的进程加到队首（`enqueue_head`）
3. **抢占公平性**：被抢占的进程通过 `enqueue_head()` 放回队首，保证它在该优先级队列中下次优先运行
4. **时间片耗尽处理分策略**：用户空间调度器管理的可抢占进程时间片耗尽后通知调度器（出队等待新时间片）；内核调度进程或不可抢占进程直接重置时间片继续运行
5. **IDLE 兜底**：所有就绪队列为空时，CPU 进入 `idle()` 等待中断唤醒
6. **CPU 亲和性**：SMP 下每个进程有 `p_cpu` 字段，入队时加入对应 CPU 的队列

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 调度优先级常量

定义于 `minix3/minix/include/minix/config.h:66-77`：

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

> 完整 RTS 标志定义参见 [06-proc-struct.md](06-proc-struct.md)。

| 标志 | 值 | 调度含义 |
|------|-----|---------|
| `RTS_PREEMPTED` | 0x4000 | 被更高优先级进程抢占，恢复时放回队首 |
| `RTS_NO_QUANTUM` | 0x8000 | 时间片耗尽，等待调度器分配新时间片 |

#### 2.1.3 调度相关特权标志

> 完整特权标志定义参见 [06-proc-struct.md](06-proc-struct.md)。定义于 `minix3/minix/include/minix/const.h:143-144`。

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

> 完整字段分析参见 [06-proc-struct.md](06-proc-struct.md)。定义于 `minix3/minix/kernel/proc.h:48-55`，用于向用户空间调度器报告进程行为：

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
2. 取进程优先级 `q = rp->p_priority`，获取进程所属 CPU 的 `rdy_head` / `rdy_tail`（通过 `get_cpu_var(rp->p_cpu, ...)` 访问目标 CPU 的队列）
3. 若队列空：`rdy_head[q] = rdy_tail[q] = rp`，`rp->p_nextready = NULL`
4. 若队列非空：`rdy_tail[q]->p_nextready = rp`，`rdy_tail[q] = rp`，`rp->p_nextready = NULL`
5. **抢占检查**（仅同 CPU）：若进程入队到当前 CPU（`cpuid == rp->p_cpu`），且新进程优先级高于当前进程（`p->p_priority > rp->p_priority`）且当前进程可抢占，则 `RTS_SET(p, RTS_PREEMPTED)` 使当前进程让出 CPU
6. SMP 下：若进程入队到其他 CPU 且该 CPU 空闲，调用 `smp_schedule()` 唤醒
7. 记录当前时刻到**当前运行进程**的 `p_accounting.enter_queue`（`get_cpulocal_var(proc_ptr)->p_accounting.enter_queue`，proc.c:1653）

**注意**：优先级数值越小越高，因此 `p->p_priority > rp->p_priority` 表示当前进程优先级低于新进程。第 7 步记录的是当前运行进程的入队时刻而非被入队进程的——这是 Minix3 的一个特殊设计，用于跟踪当前进程何时因入队操作被中断。

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
5. 记录当前时刻到**当前运行进程**的 `p_accounting.enter_queue`（proc.c:1702，同 `enqueue()` 的特殊设计）

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
5. 更新统计：`dequeues++`，计算 `time_in_queue` 增量（若 `enter_queue > 0`，则 `time_in_queue += tsc - enter_queue`，并重置 `enter_queue = 0`）
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
2. **处理抢占**：若当前进程被抢占（`RTS_PREEMPTED`），清除抢占标志；若清除后进程仍可运行（`proc_is_runnable(p)`），则根据是否有剩余时间片决定 `enqueue_head`（放回队首）或 `enqueue`（加到队尾）；若进程不可运行则不重新入队
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
3. 调用 `switch_address_space_idle()` 切换到确保内核映射可用的地址空间（非 SMP 专用，所有配置均调用）
4. 设置 `cpu_is_idle = 1`（SMP）
5. BSP 上重新启动本地定时器（AP 上停止定时器，因为计时由 BSP 负责）
6. 调用 `context_stop(KERNEL)` 停止内核时间统计
7. 调用 `halt_cpu()` 使 CPU 进入低功耗状态
8. 中断唤醒后恢复执行，返回到 `switch_to_user` 的 `pick_proc()` 循环

#### 2.3.7 proc_no_time()——时间片耗尽处理

`minix3/minix/kernel/proc.c:1893-1910`

```c
void proc_no_time(struct proc *p)
```

**功能**：进程时间片耗尽时的处理，根据进程类型采取不同策略。

**行为**：
- **用户空间调度器管理的可抢占进程**（`!proc_kernel_scheduler(p) && PREEMPTIBLE`）：调用 `notify_scheduler()` 通知调度器，进程被 dequeue 并设置 `RTS_NO_QUANTUM`
- **其他情况**（`proc_kernel_scheduler(p) || !PREEMPTIBLE`）：直接重置时间片 `p_cpu_time_left = ms_2_cpu_time(p->p_quantum_size_ms)`，进程继续运行。这包括两种情况：
  1. **内核调度的进程**：没有用户空间调度器，无需通知
  2. **不可抢占的进程**：即使有用户空间调度器，也不因时间片耗尽而让出 CPU

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
       │         └─ 重置时间片           // 内核调度/不可抢占
       └─ restore_user_context(p)  // 不返回
```

#### 2.4.2 enqueue / dequeue 调用点

**enqueue 调用点**：

| 调用者 | 场景 |
|--------|------|
| `RTS_UNSET` 宏 | 进程从不可运行变为可运行时自动调用 |
| `switch_to_user()` | 被抢占进程无剩余时间片时重新入队 |

**dequeue 调用点**：

| 调用者 | 场景 |
|--------|------|
| `RTS_SET` 宏 | 进程从可运行变为不可运行时自动调用 |
| `RTS_SETFLAGS` 宏 | 直接设置标志值时若进程可运行且新值非 0 |

> `enqueue_head` 不在上述调用点中列出，因为它仅被 `switch_to_user()` 内部调用，不通过 `RTS_UNSET` 触发。

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

> 完整 SMP 调度分析参见 [18-smp.md](18-smp.md)。

SMP 配置下，调度器有以下扩展：

1. **CPU 局部队列**：每个 CPU 有独立的 `run_q_head` / `run_q_tail`，避免跨 CPU 锁竞争
2. **跨 CPU 入队唤醒**：`enqueue()` 中若进程入队到空闲 CPU，调用 `smp_schedule()` 唤醒
3. **CPU 亲和性**：进程通过 `p_cpu` 绑定到特定 CPU，入队时加入对应 CPU 的队列
4. **TLB 一致性**：`switch_to_user()` 中检查 `MF_FLUSH_TLB`，必要时刷新 TLB

#### 2.5.6 IDLE 进程的特殊处理

IDLE 进程是每个 CPU 的"兜底"进程，有特殊处理：

1. 初始化时设置 `RTS_PROC_STOP`，永远不会被 `pick_proc()` 选中
2. `idle()` 函数手动设置 `proc_ptr` 为 IDLE 进程，绕过正常调度
3. IDLE 进程共享一个 `idle_priv` 结构，标志为 `IDL_F`（`SYS_PROC | BILLABLE`，参见 [06-proc-struct.md](06-proc-struct.md)）
4. SMP 下每个 CPU 有独立的 `idle_proc` 实例

#### 2.5.7 enqueue 中 enter_queue 的特殊记录方式

`enqueue()` 和 `enqueue_head()` 在记录入队时刻时，写入的是**当前运行进程**（`proc_ptr`）的 `p_accounting.enter_queue`，而非被入队进程的。这是 Minix3 的一个特殊设计：

- C 源码：`read_tsc_64(&(get_cpulocal_var(proc_ptr)->p_accounting.enter_queue))`（proc.c:1655, 1702）
- 目的：跟踪当前进程何时因调度事件被中断，而非被入队进程何时开始等待
- `dequeue()` 中则正确地记录了被出队进程的 `time_in_queue` 增量和 `p_dequeued` 时刻

## 3. Rust 设计决策

### 3.1 优先级队列：数组索引替代指针链表

**C 方案**：16 个优先级队列，每个队列是通过 `p_nextready` 指针串联的单向链表，使用指针指针模式遍历。

**Rust 方案**：使用 `ArrayVec<Option<ProcNr>, NR_SCHED_QUEUES>` 作为头指针数组 + 进程结构体中的 `p_nextready: Option<ProcNr>` 维持链表。

**理由**：
1. C 的指针链表在 Rust 中无法直接表达——进程存储在 `ProcessTable` 的 `Box<[KProcess]>` 中，没有裸指针
2. `Option<ProcNr>` 是 C 指针的安全替代：`None` 等价于 `NULL`，`Some(nr)` 等价于指向 `proc[nr]` 的指针
3. 指针指针模式在 Rust 中可以用 `&mut Option<ProcNr>` 替代——它是对链表节点的可变引用
4. 链表操作（enqueue/dequeue）需要 `&mut ProcessTable`，与 BKL 保护的内核执行模型一致

### 3.2 调度器结构：CpuLocal 与 ProcessTable 的协作

**C 方案**：调度队列存储在 `cpulocals.h` 的 per-CPU 结构中，通过 `get_cpu_var()` 访问。

**Rust 方案**：将调度队列从 per-CPU 结构中提取出来，作为 `Scheduler` 结构体嵌入 `CpuLocal`。

**理由**：
1. C 的 `get_cpu_var()` 是隐式的全局状态访问，Rust 应显式传递
2. `Scheduler` 持有 `run_q_head` / `run_q_tail` 数组，操作需要 `&mut ProcessTable` 参数
3. 每个 CPU 的 `CpuLocal` 包含一个 `Scheduler` 实例，SMP 下通过 CPU ID 索引
4. `pick_proc()` / `enqueue()` / `dequeue()` 方法在 `Scheduler` 上实现，需要 `&ProcessTable` 来解析 `ProcNr` → `&KProcess`

### 3.3 switch_to_user 的状态机表达

**C 方案**：使用 `goto` 标签（`not_runnable_pick_new`、`check_misc_flags`）实现状态机回跳。

**Rust 方案**：使用 `loop` + `continue` 替代 `goto`，将各阶段组织为方法调用。

**理由**：
1. Rust 没有 `goto`，`loop` + `continue` 是等价表达
2. 各阶段（处理抢占、选择进程、处理杂项标志、检查时间片）可以拆分为独立方法，提高可读性
3. 状态机的回跳点通过 `continue` 到 `loop` 顶部实现，语义清晰

### 3.4 时间片管理：类型安全的量子表示

**C 方案**：`p_cpu_time_left` 是 `u64_t`（TSC 周期），`p_quantum_size_ms` 是 `unsigned`（毫秒），通过 `ms_2_cpu_time()` 转换。

**Rust 方案**：定义 `CpuTime` newtype（TSC 周期）和 `QuantumMs` newtype（毫秒），转换通过 `QuantumMs::to_cpu_time()` 方法。

**理由**：
1. 混用 TSC 周期和毫秒是常见的 bug 来源——newtype 在编译期阻止混淆
2. `ms_2_cpu_time()` 是架构相关的转换函数，应通过 trait 抽象
3. `p_cpu_time_left == 0` 的检查可以用 `CpuTime::is_zero()` 方法表达

### 3.5 proc_no_time 的策略分支

**C 方案**：`if (!proc_kernel_scheduler(p) && PREEMPTIBLE)` 两个条件组合决定策略。

**Rust 方案**：使用模式匹配，将进程调度类型明确分类。

**理由**：
1. C 的条件组合隐含了三种情况（内核调度+可抢占、内核调度+不可抢占、用户空间调度+可抢占），但 else 分支将两种不同语义合并
2. Rust 可以用 `match` 明确区分：`KernelScheduled` / `UserScheduledPreemptible` / `UserScheduledNonPreemptible`
3. 但为了与 Minix3 语义对齐，保持与 C 相同的分支逻辑，仅用方法名提高可读性

### 3.6 enter_queue 记录的修正

**C 方案**：`enqueue()` 和 `enqueue_head()` 记录当前运行进程的 `enter_queue`，这是一个令人困惑的设计。

**Rust 方案**：记录**被入队进程**的 `enter_queue`，与 `dequeue()` 中计算 `time_in_queue` 的逻辑一致。

**理由**：
1. C 的设计看起来是 bug 或历史遗留——`dequeue()` 中用被出队进程的 `enter_queue` 计算 `time_in_queue`，但 `enqueue()` 中记录的却是当前进程的 `enter_queue`
2. 如果 `enter_queue` 用于计算等待时间，应该记录被入队进程的入队时刻
3. 这是修正性不对齐：Rust 修正了 C 的逻辑不一致，需要注释说明

### 3.7 IDLE 进程处理

**C 方案**：IDLE 进程通过 `RTS_PROC_STOP` 标志阻止被 `pick_proc()` 选中，`idle()` 函数手动设置 `proc_ptr`。

**Rust 方案**：保持相同语义——IDLE 进程初始化时设置 `PROC_STOP`，`idle()` 方法手动设置当前进程指针。

**理由**：
1. IDLE 进程不是正常调度的进程，它是一个"不存在"的兜底
2. `PROC_STOP` 标志确保 `pick_proc()` 永远不会返回 IDLE 进程
3. `idle()` 绕过正常调度是正确的设计——没有进程可运行时，CPU 需要一个"占位符"

## 4. 实现详解

### 4.1 Scheduler 结构体

> 设计决策：§3.2（调度器结构）

`Scheduler` 持有每个 CPU 的就绪队列头尾指针，所有操作需要 `&ProcessTable` 参数来解析 `ProcNr`。

```rust
pub struct Scheduler {
    run_q_head: [Option<ProcNr>; NR_SCHED_QUEUES],
    run_q_tail: [Option<ProcNr>; NR_SCHED_QUEUES],
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            run_q_head: [None; NR_SCHED_QUEUES],
            run_q_tail: [None; NR_SCHED_QUEUES],
        }
    }
}
```

### 4.2 pick_proc 实现

> 设计决策：§3.1（数组索引替代指针链表）

```rust
impl Scheduler {
    pub fn pick_proc(&self, table: &ProcessTable) -> Option<ProcNr> {
        for q in 0..NR_SCHED_QUEUES {
            if let Some(nr) = self.run_q_head[q] {
                let proc = table.get(nr);
                debug_assert!(proc.is_some_and(|p| p.is_runnable()));
                return Some(nr);
            }
        }
        None
    }
}
```

行为与 C 的 `pick_proc()` 一致：从优先级 0 开始扫描，返回第一个非空队列的队首进程。`bill_ptr` 的设置由调用者（`switch_to_user`）负责。

### 4.3 enqueue 实现

> 设计决策：§3.1（数组索引替代指针链表）、§3.6（enter_queue 修正）

```rust
impl Scheduler {
    pub fn enqueue(
        &mut self,
        nr: ProcNr,
        table: &mut ProcessTable,
        current_nr: Option<ProcNr>,
        cpu_id: u32,
    ) {
        let proc = table.get(nr).expect("enqueue: invalid proc nr");
        debug_assert!(proc.is_runnable());

        let q = proc.get_priority().get() as usize;
        debug_assert!(q < NR_SCHED_QUEUES);

        let nextready_was = table.get_mut(nr).unwrap().p_nextready.take();
        debug_assert!(nextready_was.is_none(), "enqueue: p_nextready not None");

        match self.run_q_tail[q] {
            None => {
                self.run_q_head[q] = Some(nr);
                self.run_q_tail[q] = Some(nr);
            }
            Some(tail_nr) => {
                table.get_mut(tail_nr).unwrap().p_nextready = Some(nr);
                self.run_q_tail[q] = Some(nr);
            }
        }

        if let Some(cur_nr) = current_nr {
            let cur = table.get(cur_nr).expect("enqueue: invalid current");
            let cur_prio = cur.get_priority().get();
            let new_prio = proc.get_priority().get();
            if cur_prio > new_prio && cur.is_preemptible() {
                table.rts_set(cur_nr, rts::PREEMPTED);
            }
        }

        // Minix3 bug fix: record enter_queue for the enqueued process,
        // not the current process (C records proc_ptr's enter_queue).
        let tsc = read_tsc();
        table.get_mut(nr).unwrap().p_accounting.record_enqueue(tsc);
    }
}
```

**与 C 的差异**：`enter_queue` 记录被入队进程而非当前进程（§3.6 修正性不对齐）。

### 4.4 enqueue_head 实现

> 设计决策：§3.1（数组索引替代指针链表）

```rust
impl Scheduler {
    pub fn enqueue_head(
        &mut self,
        nr: ProcNr,
        table: &mut ProcessTable,
    ) {
        let proc = table.get(nr).expect("enqueue_head: invalid proc nr");
        debug_assert!(proc.is_runnable());
        debug_assert!(proc.has_cpu_time_left());

        let q = proc.get_priority().get() as usize;

        match self.run_q_head[q] {
            None => {
                self.run_q_head[q] = Some(nr);
                self.run_q_tail[q] = Some(nr);
                table.get_mut(nr).unwrap().p_nextready = None;
            }
            Some(_) => {
                table.get_mut(nr).unwrap().p_nextready = self.run_q_head[q];
                self.run_q_head[q] = Some(nr);
            }
        }

        let acc = &mut table.get_mut(nr).unwrap().p_accounting;
        let tsc = read_tsc();
        acc.record_enqueue(tsc);
        // Minix3 bug fix: same as enqueue, record for the enqueued process.
        acc.dequeues.fetch_sub(1, Ordering::AcqRel);
        acc.preempted.fetch_add(1, Ordering::AcqRel);
    }
}
```

### 4.5 dequeue 实现

> 设计决策：§3.1（数组索引替代指针链表）

C 使用指针指针模式遍历链表，Rust 使用 `&mut Option<ProcNr>` 实现等价逻辑：

```rust
impl Scheduler {
    pub fn dequeue(&mut self, nr: ProcNr, table: &mut ProcessTable) {
        let proc = table.get(nr).expect("dequeue: invalid proc nr");
        debug_assert!(!proc.is_runnable());

        let q = proc.get_priority().get() as usize;

        // Pointer-pointer pattern: &mut Option<ProcNr> replaces struct proc **
        let mut link = &mut self.run_q_head[q];
        let mut prev: Option<ProcNr> = None;

        loop {
            match *link {
                Some(cur_nr) if cur_nr == nr => {
                    *link = table.get(cur_nr).unwrap().p_nextready;
                    if self.run_q_tail[q] == Some(nr) {
                        self.run_q_tail[q] = prev;
                    }
                    break;
                }
                Some(cur_nr) => {
                    prev = Some(cur_nr);
                    link = &mut table.get_mut(cur_nr).unwrap().p_nextready;
                }
                None => panic!("dequeue: process not found in queue"),
            }
        }

        let tsc = read_tsc();
        let acc = &mut table.get_mut(nr).unwrap().p_accounting;
        acc.record_dequeue(tsc);
        acc.dequeues.fetch_add(1, Ordering::AcqRel);

        table.get_mut(nr).unwrap().p_dequeued.store(
            get_monotonic(),
            Ordering::Release,
        );
    }
}
```

**与 C 的对应**：`&mut Option<ProcNr>` 等价于 C 的 `struct proc **xpp`——它是对链表节点中"下一个指针"的可变引用，修改 `*link` 等价于 C 的 `*xpp = (*xpp)->p_nextready`。

### 4.6 switch_to_user 实现

> 设计决策：§3.3（状态机表达）

```rust
impl Scheduler {
    pub fn switch_to_user(
        &mut self,
        table: &mut ProcessTable,
        cpu_local: &mut CpuLocal,
    ) -> ! {
        let current_nr = cpu_local.proc_ptr;

        // Phase 1: if current is still runnable, go to misc flags
        if let Some(nr) = current_nr {
            if table.get(nr).is_some_and(|p| p.is_runnable()) {
                // skip to check_misc_flags
            } else {
                // not_runnable_pick_new
                loop {
                    self.handle_preempted(current_nr, table);
                    let next_nr = match self.pick_proc(table) {
                        Some(nr) => nr,
                        None => {
                            self.idle(table, cpu_local);
                            continue;
                        }
                    };
                    cpu_local.proc_ptr = Some(next_nr);

                    // switch_address_space handled by arch layer

                    // check_misc_flags
                    loop {
                        let p = table.get(next_nr).unwrap();
                        if !p.is_runnable() {
                            break; // goto not_runnable_pick_new
                        }
                        let misc = p.p_misc_flags.load();
                        if misc == 0 {
                            break;
                        }
                        // handle misc flags (kcall_resume, delivermsg, sc_defer, etc.)
                        // each handler may make process not runnable
                        self.handle_misc_flags(next_nr, table);
                    }

                    // check quantum
                    let p = table.get(next_nr).unwrap();
                    if !p.has_cpu_time_left() {
                        self.proc_no_time(next_nr, table);
                    }

                    // final check
                    if !table.get(next_nr).unwrap().is_runnable() {
                        continue; // goto not_runnable_pick_new
                    }

                    // restore_user_context — does not return
                    arch_finish_switch_to_user(table, next_nr, cpu_local);
                }
            }
        }
        // ... (simplified, actual implementation in arch layer)
        unreachable!("switch_to_user must not return")
    }
}
```

**注意**：实际实现中，`restore_user_context()` 是架构相关的，不返回。上述代码展示了状态机的 `loop` + `continue` 结构，与 C 的 `goto` 标签等价。

### 4.7 proc_no_time 实现

> 设计决策：§3.5（策略分支）

```rust
impl Scheduler {
    pub fn proc_no_time(&mut self, nr: ProcNr, table: &mut ProcessTable) {
        let p = table.get(nr).unwrap();
        let is_kernel_scheduled = p.scheduler.is_none()
            || p.scheduler == Some(nr);
        let is_preemptible = p.is_preemptible();

        if !is_kernel_scheduled && is_preemptible {
            self.notify_scheduler(nr, table);
        } else {
            // kernel-scheduled or non-preemptible: reset quantum
            let quantum_ms = p.p_sched.quantum_size_ms.load(Ordering::Acquire);
            let cpu_time = ms_to_cpu_time(quantum_ms);
            table.get_mut(nr).unwrap().p_sched.cpu_time_left.store(
                cpu_time,
                Ordering::Release,
            );
        }
    }
}
```

### 4.8 idle 实现

> 设计决策：§3.7（IDLE 进程处理）

```rust
impl Scheduler {
    pub fn idle(
        &mut self,
        table: &mut ProcessTable,
        cpu_local: &mut CpuLocal,
    ) {
        let idle_nr = proc_nr::IDLE;
        cpu_local.proc_ptr = Some(idle_nr);

        let idle_proc = table.get(idle_nr).unwrap();
        if idle_proc.is_billable() {
            cpu_local.bill_ptr = Some(idle_nr);
        }

        // switch_address_space_idle — SMP only
        #[cfg(CONFIG_SMP)]
        switch_address_space_idle(table, cpu_local);

        cpu_local.cpu_is_idle = true;

        // BSP: restart timer; AP: stop timer
        if cpu_local.cpu_id == bsp_cpu_id() {
            restart_local_timer();
        } else {
            stop_local_timer();
        }

        halt_cpu();
    }
}
```

### 4.9 notify_scheduler 实现

```rust
impl Scheduler {
    fn notify_scheduler(&mut self, nr: ProcNr, table: &mut ProcessTable) {
        let p = table.get(nr).unwrap();
        debug_assert!(p.scheduler.is_some() && p.scheduler != Some(nr));

        // dequeue the process
        table.rts_set(nr, rts::NO_QUANTUM);

        let msg = SchedulingMessage::no_quantum(
            p.p_endpoint,
            p.p_accounting.time_in_queue.load(Ordering::Acquire),
            p.p_accounting.dequeues.load(Ordering::Acquire),
            p.p_accounting.ipc_sync.load(Ordering::Acquire),
            p.p_accounting.ipc_async.load(Ordering::Acquire),
            p.p_accounting.preempted.load(Ordering::Acquire),
            cpu_id(),
            cpu_load(),
        );

        table.get_mut(nr).unwrap().p_accounting.reset();

        let sched_nr = p.scheduler.unwrap();
        mini_send(nr, sched_nr, &msg, FROM_KERNEL);
    }
}
```

## 5. 测试要点

### 5.1 优先级队列操作

| 测试场景 | 验证点 | 对应设计 |
|---------|--------|---------|
| 空队列入队 | `run_q_head[q] == run_q_tail[q] == Some(nr)` | §4.3 |
| 非空队列入队 | 尾节点 `p_nextready` 更新，`run_q_tail` 更新 | §4.3 |
| 队首入队 | `run_q_head` 更新，原队首成为 `p_nextready` | §4.4 |
| 出队中间节点 | 前驱 `p_nextready` 跳过被删节点 | §4.5 |
| 出队队尾节点 | `run_q_tail` 更新为前驱 | §4.5 |
| 出队唯一节点 | `run_q_head` 和 `run_q_tail` 都变为 `None` | §4.5 |
| `pick_proc` 空队列 | 返回 `None` | §4.2 |
| `pick_proc` 多优先级 | 返回最高优先级队首 | §4.2 |

### 5.2 抢占机制

| 测试场景 | 验证点 | 对应设计 |
|---------|--------|---------|
| 高优先级入队触发抢占 | 当前进程 `RTS_PREEMPTED` 被设置 | §4.3 |
| 同优先级入队不触发抢占 | 当前进程 `RTS_PREEMPTED` 未设置 | §4.3 |
| 不可抢占进程不被抢占 | `PREEMPTIBLE` 未设置时 `RTS_PREEMPTED` 不设置 | §4.3 |
| 被抢占进程放回队首 | `enqueue_head` 而非 `enqueue` | §4.4 |

### 5.3 时间片管理

| 测试场景 | 验证点 | 对应设计 |
|---------|--------|---------|
| 用户空间调度进程时间片耗尽 | `notify_scheduler` 被调用，`RTS_NO_QUANTUM` 设置 | §4.7 |
| 内核调度进程时间片耗尽 | 时间片直接重置，进程不出队 | §4.7 |
| 不可抢占进程时间片耗尽 | 时间片直接重置，即使有用户空间调度器 | §4.7 |

### 5.4 switch_to_user 状态机

| 测试场景 | 验证点 | 对应设计 |
|---------|--------|---------|
| 当前进程仍可运行 | 跳过选择，直接处理 misc_flags | §4.6 |
| misc_flags 处理后进程不可运行 | 跳回选择新进程 | §4.6 |
| 时间片耗尽后进程不可运行 | 跳回选择新进程 | §4.6 |
| 所有队列为空 | 进入 `idle()` | §4.6, §4.8 |

### 5.5 IDLE 进程

| 测试场景 | 验证点 | 对应设计 |
|---------|--------|---------|
| IDLE 进程不被 `pick_proc` 选中 | `PROC_STOP` 标志阻止 | §3.7 |
| `idle()` 设置 `proc_ptr` | 手动设置绕过正常调度 | §4.8 |
| `idle()` 后中断唤醒 | 返回 `pick_proc()` 循环 | §4.8 |

### 5.6 enter_queue 修正验证

| 测试场景 | 验证点 | 对应设计 |
|---------|--------|---------|
| enqueue 记录被入队进程的 enter_queue | 与 C 不同，记录被入队进程 | §3.6 |
| dequeue 的 time_in_queue 计算正确 | 使用被入队进程的 enter_queue | §4.5 |

## 6. 参见

- [06-proc-struct.md](06-proc-struct.md)——进程结构体定义，包含 `p_rts_flags`、`p_accounting`、`p_nextready`、`PREEMPTIBLE`/`BILLABLE` 标志
- [08-endpoint.md](08-endpoint.md)——进程 endpoint 与调度器通信
- [11-privilege.md](11-privilege.md)——特权结构 `kpriv`，包含 `PREEMPTIBLE`/`BILLABLE` 标志定义
- [16-timer.md](16-timer.md)——时钟中断与时间片耗尽检测
- [18-smp.md](18-smp.md)——SMP 调度扩展，CPU 局部队列与跨 CPU 唤醒
- [17-main-init.md](17-main-init.md)——内核初始化，IDLE 进程创建
