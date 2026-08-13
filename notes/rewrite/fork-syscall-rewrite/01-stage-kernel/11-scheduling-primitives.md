# 11-scheduling-primitives: 调度原语与进程状态机

> **分类**: Kernel 调度核心
> **C 源码**: `minix3/minix/kernel/proc.c:1595-1813`（enqueue/enqueue_head/dequeue/pick_proc）, `proc.c:1893-1910`（proc_no_time）, `system.c:642-723`（sched_proc）
> **Rust 源码**: `os/kernel/src/sched.rs`, `os/kernel/src/proc_table.rs`, `os/kernel/src/proc.rs`
> **前置**: 06（struct proc / RTS_FLAGS）、10（switch_to_user 调用方）

---

## 1. 概述

### 1.1 核心问题：进程可运行状态与调度队列的一致性

内核的核心职责之一是决定"下一个运行哪个进程"。这依赖两个要素：

1. **进程的可运行状态**——通过 `RTS_FLAGS` 位掩码表达（所有位清零 = 可运行）
2. **就绪队列**——按优先级组织的进程链表

这两者必须始终保持一致：**一个进程在就绪队列中，当且仅当它是可运行的**。这是调度原语要维护的核心不变量（INV-1）。

如果违反这个不变量会发生什么？
- 不可运行的进程留在队列中 → `pick_proc` 可能选中它 → 非法运行
- 可运行的进程不在队列中 → 永远不会被选中 → 饿死

调度原语（enqueue / dequeue / pick_proc / proc_no_time）就是 RTS 状态机与就绪队列之间的桥梁。每当进程状态变化时，这些原语负责原子地更新队列，维护不变量。

### 1.2 Minix3 调度模型：16 级优先级队列

Minix3 采用**多级优先级队列**调度模型：

| 常量 | 值 | 含义 |
|------|-----|------|
| `TASK_Q` | 0 | 内核任务（最高优先级） |
| `MAX_USER_Q` | 0 | 用户进程最高优先级 |
| `USER_Q` | 7 | 用户进程默认优先级 |
| `MIN_USER_Q` | 15 | 用户进程最低优先级 |
| `NR_SCHED_QUEUES` | 16 | 队列总数 |

> **反直觉说明**：优先级数值越小，优先级越高。`0` 是最高优先级，`15` 是最低。这与"数值大 = 优先级高"的直觉相反，源于 Unix `nice` 值的传统。

同优先级队列内遵循 **FIFO**（先进先出）原则。调度器总是从最高优先级（0）开始扫描，取第一个非空队列的队头进程。

### 1.3 与上下游文档的关系

| 方向 | 文档 | 关系 |
|------|------|------|
| 上游 | 06-proc-init-boot-proc | `struct proc` / `RTS_FLAGS` 位定义 / `rts_set` 联动 |
| 上游 | 10-switch-to-user | `switch_to_user` 主循环如何调用这些原语 |
| 下游 | 12-ipc-core | `SENDING`/`RECEIVING` 标志如何触发 dequeue |
| 下游 | 16-smp | per-CPU 队列的 SMP 扩展 |
| 相关 | 09-vm-boot-protocol | `VMINHIBIT` 对调度的影响 |

**本文档的边界**：只讲四个调度原语的机制（enqueue/dequeue/pick_proc/proc_no_time）和 `sched_proc` 参数更新。不讲调用方（10）、不讲 IPC 阻塞语义（12）、不讲 SMP 扩展（16）。

---

## 2. C 源码分析

### 2.1 数据结构：就绪队列与进程链表

Minix3 的就绪队列使用 **per-CPU 头尾数组 + 单链表** 组织：

```c
// cpulocals.h:58-59
EXTERN struct proc *run_q_head[NR_SCHED_QUEUES];
EXTERN struct proc *run_q_tail[NR_SCHED_QUEUES];

// proc.h: struct proc 内
struct proc *p_nextready;  // 链表下一个指针
int p_priority;            // 决定队列号 (0-15)
int p_cpu;                 // 决定哪个 CPU 的队列
```

**设计要点**：
- `run_q_head[q]` / `run_q_tail[q]` 是 per-CPU 的——每个 CPU 有自己的一组队列，避免跨 CPU 锁竞争
- `p_nextready` 形成单链表，队尾的 `p_nextready = NULL`
- 进程入队时用 `rp->p_cpu` 而非"当前 CPU"决定队列——这支持跨 CPU 入队（一个 CPU 可以把进程放到另一个 CPU 的队列上）

### 2.2 enqueue() — 入队尾部

`enqueue()`（proc.c:1595-1659）有三个职责：

1. **入队**：将进程加到 `p_priority` 对应队列的尾部
2. **抢占检查**：如果新进程优先级高于当前运行进程，且当前进程可抢占，触发抢占
3. **记录 enter_queue**：更新 accounting 统计

```c
// proc.c:1595-1659 (简化)
void enqueue(struct proc *rp) {
    int q = rp->p_priority;
    struct proc **xpp = &(get_cpulocal_var(run_q_head)[q]);
    // 遍历到队尾
    while (*xpp != NULL) xpp = &(*xpp)->p_nextready;
    *xpp = rp;
    get_cpulocal_var(run_q_tail)[q] = rp;

    // 抢占检查
    if (priv(rp)->s_flags & PREEMPTIBLE) {
        struct proc *cur = get_cpulocal_var(proc_ptr);
        if (cur->p_priority > rp->p_priority) {
            RTS_SET(cur, RTS_PREEMPTED);
        }
    }

    // ⚠️ C bug: enter_queue 写入当前运行进程而非被入队进程
    get_cpulocal_var(proc_ptr)->p_accounting.enter_queue = read_tsc();
}
```

> **Minix3 C bug 揭示**（§3.6 设计决策依据）：
>
> 最后一行 `get_cpulocal_var(proc_ptr)->p_accounting.enter_queue` 写入的是**当前运行进程**（`proc_ptr`），而不是**被入队的进程**（`rp`）。
>
> 这导致 `time_in_queue` 统计不准确——统计的是当前进程的时间戳，而非被入队进程的时间戳。Rust 版本已修复此 bug（见 §3.6）。

### 2.3 enqueue_head() — 入队头部

`enqueue_head()`（proc.c:1670-1711）用于**被抢占进程重入队**：

```c
void enqueue_head(struct proc *rp) {
    int q = rp->p_priority;
    rp->p_nextready = get_cpulocal_var(run_q_head)[q];
    get_cpulocal_var(run_q_head)[q] = rp;
    if (get_cpulocal_var(run_q_tail)[q] == NULL)
        get_cpulocal_var(run_q_tail)[q] = rp;

    // accounting: dequeues--, preempted++
    rp->p_accounting.dequeues--;
    rp->p_accounting.preempted++;
}
```

**为什么用队头而非队尾？** 被抢占的进程还有剩余时间片，应该优先于同优先级的新来进程。队头入队保证了这种公平性——它不会被"插队"。

### 2.4 dequeue() — 链表移除

`dequeue()`（proc.c:1716-1780）使用 C 的 **pointer-pointer** 惯用法遍历链表：

```c
void dequeue(struct proc *rp) {
    int q = rp->p_priority;
    struct proc **xpp = &(get_cpu_var(rp->p_cpu, run_q_head)[q]);
    struct proc *prev_xp = NULL;

    while (*xpp != rp) {
        prev_xp = *xpp;
        xpp = &(*xpp)->p_nextready;
    }
    *xpp = rp->p_nextready;
    if (get_cpu_var(rp->p_cpu, run_q_tail)[q] == rp)
        get_cpu_var(rp->p_cpu, run_q_tail)[q] = prev_xp;

    // accounting
    rp->p_accounting.time_in_queue = read_tsc() - rp->p_accounting.enter_queue;
    rp->p_accounting.dequeues++;
}
```

**pointer-pointer 惯用法**：`struct proc **xpp` 指向链表节点的 `p_nextready` 字段。当找到目标节点时，`*xpp = rp->p_nextready` 直接修改前驱的 next 指针，无需队首特殊处理。`prev_xp` 仅用于更新 tail 指针。

> Rust 如何替代这个惯用法？见 §3.4。

### 2.5 pick_proc() — 选进程

`pick_proc()`（proc.c:1785-1813）从最高优先级扫描，取第一个非空队列队头：

```c
struct proc *pick_proc(void) {
    for (int q = 0; q < NR_SCHED_QUEUES; q++) {
        if (get_cpulocal_var(run_q_head)[q] != NULL) {
            struct proc *rp = get_cpulocal_var(run_q_head)[q];
            // BILLABLE: 更新 bill_ptr 用于时钟中断计费
            if (priv(rp)->s_flags & BILLABLE)
                get_cpulocal_var(bill_ptr) = rp;
            return rp;
        }
    }
    return NULL;
}
```

**bill_ptr 的作用**：时钟中断发生时，中断处理时间记到 `bill_ptr` 指向的进程头上。谁触发中断就记到谁头上——这是公平的 CPU 计费方式。

### 2.6 proc_no_time() — 时间片耗尽

`proc_no_time()`（proc.c:1893-1910）根据调度策略分支处理：

```c
void proc_no_time(struct proc *p) {
    if (!proc_kernel_scheduler(p) && priv(p)->s_flags & PREEMPTIBLE) {
        // 用户调度 + 可抢占: 通知用户态调度器
        RTS_SET(p, RTS_NO_QUANTUM);
        sched(p);  // 发送 SCHEDULING_NO_QUANTUM 消息给 p_scheduler
    } else {
        // 内核调度: 直接重置时间片
        p->p_cpu_time_left = ms_2_cpu_time(p->p_quantum_size_ms);
    }
}
```

**策略分支的本质**：
- **用户调度的进程**（有 `p_scheduler` 指向用户态调度服务器）：时间片耗尽后，内核不直接决定新时间片，而是通知用户态调度器重新调度
- **内核调度的进程**（`p_scheduler == NULL` 或指向自己，如 CLOCK/SYSTEM/IDLE）：不可抢占，时间片耗尽直接重置

这种设计将调度策略（用户态）与调度机制（内核态）分离。

### 2.7 RTS_SET / RTS_UNSET 宏的隐藏联动

Minix3 的 `RTS_SET` / `RTS_UNSET` 宏不仅仅是设置标志位——它们还**自动触发 enqueue/dequeue**：

```c
// proc.h:206-215（简化展示：实际宏保存 rts_flags 旧值以避免二次读取）
#define RTS_SET(rp, f)                                                 \
    do {                                                               \
        int was_runnable = proc_is_runnable(rp);                       \
        (rp)->p_rts_flags |= (f);                                      \
        if (was_runnable && !proc_is_runnable(rp)) dequeue(rp);        \
    } while (0)

#define RTS_UNSET(rp, f)                                               \
    do {                                                               \
        int was_runnable = proc_is_runnable(rp);                       \
        (rp)->p_rts_flags &= ~(f);                                     \
        if (!was_runnable && proc_is_runnable(rp)) enqueue(rp);        \
    } while (0)
```

**联动的本质**：调用方无需手动维护队列一致性。只要通过宏访问标志位，不变量 INV-1（"在队列中 ⟺ 可运行"）自动维护。但如果直接 `p_rts_flags |= f`（绕过宏），队列就会不一致——这是 C 代码的脆弱性来源。

---

## 3. Rust 设计决策

本章用 Rust 类型系统重新表达 C 的调度原语。每个决策都包含"为什么选 A 非 B"的取舍分析。

### 3.1 队列数据结构：数组索引替代指针链表

**决策**：用 `Option<ProcNr>` 替代 C 的 `struct proc *` 链表。

```rust
// sched.rs
pub struct Scheduler {
    run_q_head: [Option<ProcNr>; priority::NR_SCHED_QUEUES],
    run_q_tail: [Option<ProcNr>; priority::NR_SCHED_QUEUES],
}
```

**为什么不用指针链表？** Rust 所有权模型下，`struct proc *` 链表需要 `unsafe`（进程归 `ProcessTable` 所有，`Scheduler` 只持有引用）。用 `ProcNr`（i32 索引）+ 数组查询是安全替代——`None` 表示空槽位，`Some(nr)` 引用进程表中的进程。

**ARCH 标记**：这是架构演进（ARCH-1），从 C 的指针链表改为数组索引 + 表查询。

### 3.2 Scheduler struct 拆分：纯队列操作与字段更新分离

**决策**：`Scheduler` 只管队列数组，`ProcessTable` 协调字段更新。

```rust
// sched.rs — Scheduler 方法返回 EnqueueInfo，不碰进程字段
pub fn enqueue_queue_tail(&mut self, nr: ProcNr, q: usize) -> EnqueueInfo {
    let old_tail = self.run_q_tail[q];
    // ... 更新 head/tail
    EnqueueInfo { queue: q, old_tail }
}

// proc_table.rs — ProcessTable 用 EnqueueInfo 更新进程字段
pub fn sched_enqueue(&mut self, nr: ProcNr, ...) {
    let info = self.sched.enqueue_queue_tail(nr, q);
    // 用 info.old_tail 更新进程的 p_nextready
}
```

**为什么拆分？** 避免**自借用**问题。如果 `Scheduler` 持有 `&ProcessTable`，那么 `&mut self.sched` + `&mut self.procs` 会违反 Rust 的借用规则（不能同时有两个 `&mut self`）。拆分后，`Scheduler` 方法返回信息（`EnqueueInfo`），`ProcessTable` 用信息更新字段——分离关注点，借用分离。

### 3.3 优先级类型：Priority newtype + u8

**决策**：`Priority` 是 `u8` newtype，构造时校验 `0..=15`。

```rust
// proc.rs
pub struct Priority(u8);

impl Priority {
    pub const fn new(value: u8) -> Option<Self> {
        if value <= priority::MIN_USER_Q {
            Some(Self(value))
        } else {
            None
        }
    }
}
```

**为什么是 `u8` 而非 `i8`？** 优先级范围是 0-15，`u8` 自然表达"非负"。用 `i8` 会暗示负值合法——而 C 的 `-1` 哨兵不是优先级值，是 `sched_proc` 的参数语义（见 §3.8）。

**替代方案拒绝**：
- `Priority(usize)`：usize 平台相关；`u8` 足够且最小
- `Priority(i8)`：暗示负值合法；与"优先级非负"语义矛盾

### 3.4 dequeue 实现：prev + found 两次遍历

**决策**：Rust 用 `prev: Option<ProcNr>` + `found: bool` 替代 C 的 pointer-pointer。

```rust
// sched.rs
pub fn dequeue_from_queue(&mut self, nr: ProcNr, q: usize, procs: &mut [KProcess]) {
    let mut prev: Option<ProcNr> = None;
    let mut current = self.run_q_head[q];
    let mut found = false;

    // 第一次遍历：找节点 + 记录 prev
    while let Some(cur_nr) = current {
        if cur_nr == nr { found = true; break; }
        prev = Some(cur_nr);
        current = /* next via procs[cur_nr].p_nextready */;
    }

    // 更新前驱的 nextready 指向被删节点的后继
    // 更新 tail（如果删的是尾节点）
}
```

**为什么不用 pointer-pointer？** C 的 `struct proc **xpp` 在 Rust 中需要 `&mut Option<ProcNr>`，语法复杂且可读性差。两次遍历是 O(n)，但同优先级队列长度通常 < 10，性能差异可忽略。

**ARCH 标记**：这是架构演进（ARCH-3），从 C 的 pointer-pointer 改为两次遍历。

### 3.5 enqueue 抢占检查：内联 rts_set

**决策**：抢占检查内联在 `sched_enqueue` 中，直接调用 `rts_set(PREEMPTED)`。

```rust
// proc_table.rs — sched_enqueue Phase 3
if cur_cpu == cpu_id && cur_prio > new_prio && cur_preemptible {
    self.rts_set(cur_nr, RtsFlagsBits::PREEMPTED);
}
```

**为什么不返回 bool 让调用方决定？** 封装更好——调用方无需关心抢占细节。`rts_set(PREEMPTED)` 自动联动 `sched_dequeue`（被抢占进程出队），保证一致性。

### 3.6 enter_queue 修复：写入被入队进程

**决策**：Rust 修复 Minix3 C bug，`enter_queue` 写入被入队进程而非 `proc_ptr`。

```rust
// proc_table.rs — sched_enqueue Phase 4
// Design decision §3.6: fix Minix3 bug (C writes proc_ptr, not nr)
let tsc = read_tsc();
self.get_mut(nr).unwrap().p_accounting.record_enqueue(tsc);
```

**C bug 对比**：
```c
// C (proc.c:1653) — 写入当前运行进程（错误）
get_cpulocal_var(proc_ptr)->p_accounting.enter_queue = read_tsc();

// Rust (proc_table.rs:521) — 写入被入队进程（正确）
self.get_mut(nr).unwrap().p_accounting.record_enqueue(tsc);
```

这是**修正性不对齐**——C 源码有 bug，Rust 修正之。注释明示 "Design decision §3.6: fix Minix3 bug"。

### 3.7 IDLE 进程处理：PROC_STOP 阻止 pick_proc

**决策**：IDLE 进程设 `RTS_PROC_STOP`，永不在就绪队列中。

IDLE 进程不通过正常调度路径。`switch_to_user` 无就绪进程时直接调用 `idle()` 函数（见 10 文档）。IDLE 的 `RTS_PROC_STOP` 标志确保 `rts_set`/`rts_unset` 联动时不会将其入队。

### 3.8 sched_proc：Option 哨兵替代 C 式 -1 + SchedParams 参数聚合

**决策 1**：`sched_proc` 参数用 `Option<u8>` / `Option<u32>` 替代 C 的 `i32` + `-1`。

**决策 2**：四个调度参数聚合为 `SchedParams` 结构体，将函数参数从 5 个减少到 2 个（`p` + `params`），提高 API 可读性。

```rust
// sched.rs — SchedParams 结构体
pub struct SchedParams {
    pub priority: Option<u8>,   // None = 保持当前（替代 C 的 -1）
    pub quantum: Option<u32>,   // None = 保持当前
    pub cpu: Option<u32>,       // None = 保持当前
    pub niced: bool,
}

// sched.rs — 新签名
pub fn sched_proc(
    p: &mut KProcess,
    params: SchedParams,
) -> Result<(), SchedProcError>
```

**为什么用 Option 而非 -1 哨兵？**

C 的方式：`priority: i32`，`-1` 表示"保持当前"。这导致：
1. `Priority` 类型被污染为 `i8`（要能存 `-1`）
2. 调用方可能误传 `-2`（合法的 `i32` 值，但语义非法）
3. 类型系统无法防止哨兵值泄漏到优先级比较逻辑

Rust 的方式：`Option<u8>`，`None` 表示"保持当前"。这：
1. `Priority` 用 `u8`（优先级本身不需要负值）
2. 类型系统防止"忘记检查 -1"
3. `Option` 是 Rust 表达"可选"的惯用法

**为什么用 SchedParams 聚合参数？**

原 5 参数签名在调用方需要传递 4 个 `Option` + 1 个 `bool`，参数顺序易错。`SchedParams` 结构体：
1. 命名字段消除参数顺序歧义（`priority:`, `quantum:`, `cpu:`, `niced:` 自文档化）
2. 调用方意图更清晰（`SchedParams { priority: None, quantum: Some(100), cpu: None, niced: false }` 一目了然）
3. 未来扩展新参数时不需修改函数签名（仅需扩展结构体字段）

**ARCH 标记**：这是架构演进（ARCH-4），从 C 的 `errno` 返回改为 `Result<(), SchedProcError>`。

调用方（`syscall.rs` / `syscall_process.rs`）负责将 C 消息中的 `i32` -1 哨兵转换为 `Option`，然后构造 `SchedParams`：

```rust
// syscall.rs — 转换 C i32 哨兵为 Option，构造 SchedParams
let priority_opt = match sched.priority {
    -1 => None,
    v if v >= 0 => Some(v as u8),
    _ => return KcallResult::Ok(EINVAL), // priority < 0 && != -1
};
// ...
crate::sched::sched_proc(target, SchedParams {
    priority: priority_opt, quantum: quantum_opt, cpu: cpu_opt, niced,
})?;
```

---

## 4. 实现要点

### 4.1 RtsFlagsBits / MiscFlagsBits bitflags 定义

Rust 用 `bitflags!` 宏生成类型安全的位运算。16 个 RTS 位 + 18 个 MISC 位的值与 C `proc.h:142-180` 完全对齐。

```rust
// proc.rs — RTS 标志位（节选）
bitflags::bitflags! {
    pub struct RtsFlagsBits: u32 {
        const SLOT_FREE = 0x01;
        const PROC_STOP = 0x02;
        const SENDING = 0x04;
        const RECEIVING = 0x08;
        // ...
        const PREEMPTED = 0x4000;
        const NO_QUANTUM = 0x8000;
        const BOOTINHIBIT = 0x10000;
    }
}
```

> 完整的 16 个 RTS 位和 18 个 MISC 位定义参见 06 文档 §3.3。本文档不重复列表。

`bitflags!` 的优势：编译期检查位掩码合法性，`.set()` / `.clear()` / `.is_set()` 自文档化，无需手写位运算。

### 4.2 Scheduler struct 与队列操作

`Scheduler` 持有 16 个队列的 head/tail 数组，提供纯队列操作（不碰进程字段）：

```rust
// sched.rs
impl Scheduler {
    pub fn pick_proc(&self, procs: &[KProcess]) -> Option<ProcNr> {
        // 从优先级 0 扫描到 15，取第一个非空队列队头
        for q in 0..priority::NR_SCHED_QUEUES {
            if let Some(nr) = self.run_q_head[q] {
                return Some(nr);
            }
        }
        None
    }

    pub fn enqueue_queue_tail(&mut self, nr: ProcNr, q: usize) -> EnqueueInfo {
        // 更新 head/tail，返回 old_tail 供调用方更新 p_nextready
    }

    pub fn enqueue_queue_head(&mut self, nr: ProcNr, q: usize) { /* ... */ }

    pub fn dequeue_from_queue(&mut self, nr: ProcNr, q: usize, procs: &mut [KProcess]) {
        // 两次遍历：找节点 + 更新前驱指针
    }
}
```

**借用分离**：`Scheduler` 方法只操作 `&mut [Option<ProcNr>]`（队列数组），不碰 `[KProcess]`（进程字段）。这避免了自借用。

> **已知缺口**：Rust 实现当前未更新 `bill_ptr`（C: proc.c:1804-1809）。`set_bill_to_idle` 方法（proc_table.rs:358）仅用于 IDLE 计费初始化。完整的 bill_ptr 联动待实现（TODO）。

### 4.3 ProcessTable 协调：sched_enqueue / sched_dequeue / sched_proc_no_time

`ProcessTable` 包装 `Scheduler`，协调队列操作与字段更新。以 `sched_enqueue` 为例：

```rust
// proc_table.rs — sched_enqueue 四阶段
pub fn sched_enqueue(&mut self, nr: ProcNr, current_nr: Option<ProcNr>, cpu_id: CpuId) {
    let q = self.get(nr).map_or(0, |p| p.get_priority().get() as usize);
    debug_assert!(q < 16, "sched_enqueue: priority out of range");

    // Phase 1: 队列数组更新（委托 Scheduler）
    let info = self.sched.enqueue_queue_tail(nr, q);

    // Phase 2: 进程字段更新（p_nextready）
    {
        let procs = &mut self.procs;
        let nr_idx = nr_to_idx(nr).unwrap();
        procs[nr_idx].p_nextready.store(NONE_PROC_NR, Ordering::Relaxed);
        if let Some(tail_nr) = info.old_tail {
            let tail_idx = nr_to_idx(tail_nr).unwrap();
            procs[tail_idx].p_nextready.store(nr.0, Ordering::Relaxed);
        }
    }

    // Phase 3: 抢占检查（仅同 CPU 且高优先级）
    if let Some(cur_nr) = current_nr {
        let (cur_prio, cur_cpu, cur_preemptible) = {
            let cur = self.get(cur_nr).expect("sched_enqueue: invalid current nr");
            (
                cur.get_priority().get(),
                cur.p_sched.cpu.load(Ordering::Acquire),
                cur.get_priority().get() != 0,
            )
        };
        let new_prio = q as u8;
        if cur_cpu == cpu_id.raw() && cur_prio > new_prio && cur_preemptible {
            self.rts_set(cur_nr, RtsFlagsBits::PREEMPTED);
        }
    }

    // Phase 4: 记录 enter_queue（§3.6: 修复 C bug）
    let tsc = read_tsc();
    self.get_mut(nr).unwrap().p_accounting.record_enqueue(tsc);
}
```

`sched_dequeue` 两阶段：移除队列 + accounting。`sched_enqueue_head` 三阶段：队列 + 字段 + accounting（dequeues--, preempted++）。

### 4.4 rts_set / rts_unset 联动封装

Rust 用方法封装替代 C 的宏，实现 RTS 标志修改自动触发 enqueue/dequeue：

```rust
// proc_table.rs
pub fn rts_set(&mut self, nr: ProcNr, flags: RtsFlagsBits) {
    let was_runnable = self.get(nr).is_some_and(|p| p.is_runnable());
    if let Some(p) = self.get_mut(nr) {
        p.p_rts_flags.set(flags);
    }
    let is_runnable = self.get(nr).is_some_and(|p| p.is_runnable());
    // INV-1: 可运行 → 不可运行 → 自动出队（仅当仍在调度队列中）
    if was_runnable && !is_runnable && self.is_in_scheduler(nr) {
        // C uses `get_cpu_var(rp->p_cpu, run_q_head)` — the process's
        // assigned CPU determines which per-CPU queue to dequeue from.
        let cpu_id = self.get(nr)
            .map(|p| CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire)))
            .unwrap_or(CpuId::BSP);
        self.sched_dequeue(nr, cpu_id);
    }
}
```

调用方无法绕过——必须通过 `rts_set` / `rts_unset`，保证不变量。

### 4.5 sched_proc_no_time 与 notify_scheduler 联动

```rust
// proc_table.rs
pub fn sched_proc_no_time(&mut self, nr: ProcNr) {
    let (kernel_scheduled, preemptible, quantum_ms) = {
        // R-15: `nr` 是耗尽时间片的可运行进程，必在表中
        let p = self.get(nr).expect("sched_proc_no_time: invalid proc nr");
        let ks = p.p_sched.scheduler.is_none() || p.p_sched.scheduler == Some(p.p_nr);
        let pre = p.get_priority().get() != 0;
        let qms = p.p_sched.quantum.size_ms.load(Ordering::Acquire);
        (ks, pre, qms)
    };

    if !kernel_scheduled && preemptible {
        // 用户调度 + 可抢占: rts_set(NO_QUANTUM) 自动出队 + 通知调度器
        self.rts_set(nr, RtsFlagsBits::NO_QUANTUM);
        self.notify_scheduler(nr);
    } else {
        // 内核调度: 重置 cpu_time_left
        let cpu_time = crate::clock::ms_to_cpu_time(quantum_ms);
        self.get_mut(nr)
            .unwrap()
            .p_sched
            .quantum
            .cpu_time_left
            .store(cpu_time, Ordering::Release);
    }
}
```

**notify_scheduler 已实现**（proc_table.rs:643）：`rts_set(NO_QUANTUM)` 出队后，`notify_scheduler` 构建 `SCHEDULING_NO_QUANTUM` 消息（`mess_krn_lsys_schedule`，C: proc.c:1860-1891）并发送给用户态调度器。发送失败时 C panic，Rust 以 `panic!` 匹配（内核源发送失败 = 内核完整性错误）。

### 4.6 sched_proc 参数验证与字段更新

`sched_proc` 9 步流程与 C `system.c:642-723` 对齐。参数通过 `SchedParams` 结构体聚合传入（§3.8）：

```rust
// sched.rs
pub fn sched_proc(
    p: &mut KProcess,
    params: SchedParams,
) -> Result<(), SchedProcError> {
    // Step 1: 验证 priority 范围（Some(v) 时 v <= 15）
    if let Some(v) = params.priority { if v > priority::MIN_USER_Q { return Err(InvalidArgument); } }
    // Step 2: 验证 quantum 范围（Some(v) 时 v >= 1）
    if let Some(v) = params.quantum { if v < 1 { return Err(InvalidArgument); } }
    // Step 3: 验证 CPU（SMP stub，单 CPU 总是 OK）
    // Step 4: 检测变化 → 设置 RTS_NO_QUANTUM
    // Step 5: 应用 priority（if Some）
    // Step 6: 应用 quantum + 重置 cpu_time_left（if Some）
    // Step 7: 应用 CPU affinity（if Some）
    // Step 8: 应用 niced 标志
    // Step 9: 清除 RTS_NO_QUANTUM
    Ok(())
}
```

错误码与 Minix3 对齐：`EINVAL=22`（InvalidArgument）、`EBADCPU=42`（BadCpu）。

> 骨架展示，完整实现见 [sched.rs:321-427](os/kernel/src/sched.rs)。

---

## 5. 测试

测试覆盖核心路径，命名表达意图（非 `test_1`）：

| 测试函数 | 覆盖路径 | C 对应 |
|---------|---------|--------|
| `test_enqueue_empty_queue` | 空队列入队 | proc.c:1595 |
| `test_enqueue_non_empty_queue` | 非空队列入队 + p_nextready 链接 | proc.c:1595 |
| `test_enqueue_head` | 队头入队 | proc.c:1670 |
| `test_dequeue_only_process` | 唯一进程出队（经 rts_set 联动） | proc.c:1716 |
| `test_pick_proc_empty` | 空队列 pick → None | proc.c:1785 |
| `test_pick_proc_highest_priority` | 高优先级优先 | proc.c:1785 |
| `test_proc_no_time_kernel_scheduled` | 内核调度重置时间片 | proc.c:1893 |
| `test_proc_no_time_user_scheduled_preemptible` | 用户调度设置 NO_QUANTUM | proc.c:1893 |
| `test_sched_proc_priority_change` | priority Some(v) 更新 | system.c:684 |
| `test_sched_proc_priority_none_keeps_value` | priority None 保持当前 | system.c:684 |
| `test_sched_proc_priority_overflow_rejected` | priority > 15 → EINVAL | system.c:645 |
| `test_sched_proc_priority_too_high_rejected` | priority = 17 → EINVAL（越界拒绝） | system.c:645 |
| `test_sched_proc_quantum_update_resets_cpu_time` | quantum 更新重置 cpu_time_left | system.c:686 |
| `test_sched_proc_quantum_zero_rejected` | quantum == 0 → EINVAL | system.c:647 |
| `test_sched_proc_niced_flag_set/clear` | niced 标志设置/清除 | system.c:695 |
| `test_sched_proc_cpu_update` | CPU affinity 更新 | system.c:691 |
| `test_sched_proc_full_update_with_all_params` | 全参数端到端 | system.c:642 |
| `test_sched_proc_error_to_errno` | 错误码映射 | EINVAL=22, EBADCPU=42 |
| `test_sched_proc_c_parity_step1_priority_validation` | C parity：priority 16 → EINVAL / 15 → OK | system.c:644-645 |
| `test_sched_proc_c_parity_step2_quantum_validation` | C parity：quantum 0 → EINVAL / 1 → OK | system.c:647-648 |
| `test_sched_proc_c_parity_step8_niced_flag` | C parity：niced 设置/清除 MF_NICED | system.c:695-698 |
| `test_sched_proc_c_parity_step9_no_quantum_cleared` | C parity：sched_proc 后 RTS_NO_QUANTUM 清除 | system.c:698 |

> **§3.8 迁移说明**：`test_sched_proc_priority_negative_rejected` 已删除（`u8` 类型系统在编译期防止负值）。替换为 `test_sched_proc_priority_overflow_rejected`（运行期检查 `> 15`）。

---

## 6. 参见

| 文档 | 关联内容 |
|------|---------|
| [06-proc-init-boot-proc](06-proc-init-boot-proc.md) | `struct proc` 完整字段 / `RTS_FLAGS` 16 位完整表 / `rts_set` 联动设计 |
| [10-switch-to-user](10-switch-to-user.md) | `switch_to_user` 主循环如何调用 enqueue/dequeue/pick_proc / `idle()` 实现 |
| [09-vm-boot-protocol](09-vm-boot-protocol.md) | `VMINHIBIT` 标志对调度的影响 |
| [12-ipc-core](12-ipc-core.md) | `SENDING`/`RECEIVING` 标志如何触发 dequeue |
| [16-smp](16-smp.md) | per-CPU 队列的 SMP 扩展 / 跨 CPU IPI 唤醒 |
