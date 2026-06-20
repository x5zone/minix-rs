# 11-scheduling-primitives: 调度原语与进程状态机

> **分类**: Kernel 调度核心
> **源码**: `minix3/minix/kernel/proc.c:1595-1832`（enqueue/dequeue/pick_proc）, `proc.c:1893-1910`（proc_no_time）
> **前置**: 09（switch_to_user 调用这些原语）
> **C 总行数**: ~240 行

---

## 1. 概述

### 1.1 核心问题

内核如何管理进程的可运行状态？当进程状态变化时，如何高效地将其加入/移出调度队列？

Minix3 的调度模型是**多级优先级队列**：16 个优先级队列，同优先级 FIFO。调度原语（enqueue/dequeue/pick_proc）是状态机与调度器之间的桥梁。

### 1.2 进程状态机

```
                    ┌──────────────────────────────────────┐
                    │          RTS_FLAGS 状态空间            │
                    │                                      │
   SLOT_FREE ──→ [初始化] ──→ PROC_STOP ──→ [可运行]      │
                    │              ↑           │  ↓        │
                    │              │      VMINHIBIT  SENDING│
                    │              │           │  ↓        │
                    │         NO_QUANTUM  PAGEFAULT RECEIVING│
                    │              │           │  ↓        │
                    │         SIGNALED    BOOTINHIBIT SIGNALED│
                    │              │           │            │
                    │              └─── [不可运行] ←────────┘│
                    └──────────────────────────────────────┘

可运行 = RTS_FLAGS == 0（所有位清零）
不可运行 = RTS_FLAGS 的任何位被设置
```

**关键语义**：RTS_FLAGS 是位掩码，多个标志可以同时设置。进程可运行当且仅当**所有位都清零**。这意味着多个阻塞原因可以叠加，只有全部清除后进程才可运行。

### 1.3 16 级优先级队列

| 队列号 | 典型用途 | 进程示例 |
|--------|---------|---------|
| 0 | 最高优先级（时钟、硬中断） | CLOCK, IDLE |
| 1-3 | 系统任务 | SYSTEM, KERNEL |
| 4-6 | 核心服务器 | VM, PM, RS |
| 7-9 | 文件系统 | VFS |
| 10-12 | 网络服务 | INET |
| 13-15 | 用户进程 | 用户程序 |

---

## 2. C 源码分析

### 2.1 enqueue() — 入队

**源码**: `proc.c:1595-1668`

```c
void enqueue(register struct proc *rp) {
    int q = rp->p_priority;
    rdy_head = get_cpu_var(rp->p_cpu, run_q_head);
    rdy_tail = get_cpu_var(rp->p_cpu, run_q_tail);

    if (!rdy_head[q]) {
        rdy_head[q] = rdy_tail[q] = rp;
        rp->p_nextready = NULL;
    } else {
        rdy_tail[q]->p_nextready = rp;
        rdy_tail[q] = rp;
        rp->p_nextready = NULL;
    }

    // 抢占检查：新进程优先级高于当前运行进程
    if (cpuid == rp->p_cpu) {
        p = get_cpulocal_var(proc_ptr);
        if ((p->p_priority > rp->p_priority) &&
                (priv(p)->s_flags & PREEMPTIBLE))
            RTS_SET(p, RTS_PREEMPTED);  // 会调用 dequeue()
    }
    // SMP：如果目标 CPU 空闲，发送 IPI 唤醒
    else if (get_cpu_var(rp->p_cpu, cpu_is_idle)) {
        smp_schedule(rp->p_cpu);
    }
}
```

**关键语义**：
- 入队到**进程所属 CPU** 的队列（`rp->p_cpu`），而非当前 CPU
- 尾部插入，保证 FIFO 顺序
- 入队后检查是否需要抢占当前进程
- SMP：跨 CPU 入队时，如果目标 CPU 空闲则发送 IPI

### 2.2 enqueue_head() — 队头入队

**源码**: `proc.c:1675-1720`

```c
static void enqueue_head(struct proc *rp) {
    // 与 enqueue 相同的队列选择逻辑
    // 但插入到队头而非队尾
    if (!rdy_head[q]) {
        rdy_head[q] = rdy_tail[q] = rp;
        rp->p_nextready = NULL;
    } else {
        rp->p_nextready = rdy_head[q];
        rdy_head[q] = rp;
    }
    rp->p_accounting.dequeues--;
    rp->p_accounting.preempted++;
}
```

**用途**：被抢占的进程重新入队时使用队头，保证公平性——它还有剩余时间片，应该优先于同优先级的其他进程。

### 2.3 dequeue() — 出队

**源码**: `proc.c:1716-1780`

```c
void dequeue(struct proc *rp) {
    int q = rp->p_priority;
    // 遍历链表找到 rp 并移除
    prev_xp = NULL;
    for (xpp = &rdy_head[q]; *xpp; xpp = &(*xpp)->p_nextready) {
        if (*xpp == rp) {
            *xpp = (*xpp)->p_nextready;
            if (rp == rdy_tail[q]) {
                rdy_tail[q] = prev_xp;
            }
            break;
        }
        prev_xp = *xpp;
    }
    // 统计：记录出队时间
}
```

**关键语义**：
- 遍历链表查找，O(n)（n = 同优先级进程数）
- 从**进程所属 CPU** 的队列中移除
- 更新 head/tail 指针

### 2.4 pick_proc() — 选择进程

**源码**: `proc.c:1804-1832`

```c
static struct proc * pick_proc(void) {
    rdy_head = get_cpulocal_var(run_q_head);
    for (q = 0; q < NR_SCHED_QUEUES; q++) {
        if (!(rp = rdy_head[q])) continue;
        assert(proc_is_runnable(rp));
        if (priv(rp)->s_flags & BILLABLE)
            get_cpulocal_var(bill_ptr) = rp;
        return rp;
    }
    return NULL;
}
```

**关键语义**：
- 从最高优先级（0）开始扫描
- 返回第一个非空队列的队头进程
- 如果是 BILLABLE 进程，更新 bill_ptr（时间统计）
- 无可运行进程返回 NULL

### 2.5 proc_no_time() — 时间片用完

**源码**: `proc.c:1893-1910`

```c
void proc_no_time(struct proc *p) {
    if (!proc_kernel_scheduler(p) && priv(p)->s_flags & PREEMPTIBLE) {
        notify_scheduler(p);  // 出队 + 通知调度服务器
    } else {
        p->p_cpu_time_left = ms_2_cpu_time(p->p_quantum_size_ms);  // 重置时间片
    }
}
```

**关键语义**：
- 用户调度的可抢占进程：通知调度服务器（出队）
- 内核调度或不可抢占进程：直接重置时间片

---

## 3. Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| RTS_FLAGS | u32 + 常量 vs bitflags | **bitflags!** | 编译期类型安全，位运算自文档化 |
| 队列数据结构 | 链表 vs 数组索引 | **数组索引（ProcNr）** | 避免 unsafe 指针，链表操作通过索引实现 |
| enqueue 抢占检查 | 内联 vs 回调 | **返回 bool** | 调用方决定是否设置 PREEMPTED |
| per-CPU 队列 | CpuLocal vs 参数传递 | **参数传递（cpu_id）** | 显式化，避免隐式全局状态 |
| pick_proc 返回 | 引用 vs ProcNr | **Option\<ProcNr\>** | 与 ProcessTable 索引一致 |
| 优先级类型 | i32 vs u8 | **u8** | 0-15 范围，无需负值 |

---

## 4. 实现要点

### 4.1 RtsFlagsBits

```rust
bitflags::bitflags! {
    pub struct RtsFlagsBits: u32 {
        const SLOT_FREE    = 0x01;    // 进程槽未使用
        const PROC_STOP    = 0x02;    // 进程被停止
        const SENDING      = 0x04;    // 正在发送消息（阻塞）
        const RECEIVING    = 0x08;    // 正在接收消息（阻塞）
        const SIGNALED     = 0x10;    // 有信号待处理
        const SIG_PENDING  = 0x20;    // 信号挂起
        const P_STOP       = 0x40;    // PM 停止
        const NO_PRIV      = 0x80;    // 无特权
        const NO_ENDPOINT  = 0x100;   // 无端点
        const VMINHIBIT    = 0x200;   // VM 抑制
        const PAGEFAULT    = 0x400;   // 页错误
        const VMREQUEST    = 0x800;   // VM 请求挂起
        const VMREQTARGET  = 0x1000;  // VM 请求目标
        const PREEMPTED    = 0x4000;  // 被抢占
        const NO_QUANTUM   = 0x8000;  // 时间片用完
        const BOOTINHIBIT  = 0x10000; // 启动抑制
    }
}
```

### 4.2 MiscFlagsBits

```rust
bitflags::bitflags! {
    pub struct MiscFlagsBits: u32 {
        const DELIVERMSG    = 0x01;   // 消息待投递
        const KCALL_RESUME  = 0x02;   // 内核调用需恢复
        const SC_DEFER      = 0x04;   // 系统调用延迟
        const SC_TRACE      = 0x08;   // 系统调用跟踪
        const SC_ACTIVE     = 0x10;   // 系统调用活跃
        // ... 其他标志
    }
}
```

### 4.3 Scheduler

```rust
pub struct Scheduler {
    run_q_head: [Option<ProcNr>; NR_SCHED_QUEUES],
    run_q_tail: [Option<ProcNr>; NR_SCHED_QUEUES],
}
```

### 4.4 enqueue/dequeue 语义保证

- `enqueue()`: 进程必须可运行（assert），入队到尾部
- `enqueue_head()`: 进程必须可运行 + 有剩余时间片，入队到头部
- `dequeue()`: 进程必须不可运行（assert），从链表中移除
- `pick_proc()`: 返回最高优先级队列的队头，无可运行返回 None

---

## 5. 测试

### 5.1 单元测试

| 测试 | 验证内容 |
|------|---------|
| `test_rts_flags_runnable` | 所有位清零 = 可运行 |
| `test_rts_flags_not_runnable` | 任何位设置 = 不可运行 |
| `test_rts_flags_multiple` | 多个标志叠加，全部清除才可运行 |
| `test_enqueue_dequeue` | 入队后 pick_proc 返回该进程 |
| `test_enqueue_head_ordering` | enqueue_head 进程在同优先级中最先被选中 |
| `test_pick_proc_priority` | 高优先级进程优先于低优先级 |
| `test_pick_proc_empty` | 无可运行进程返回 None |
| `test_proc_no_time_user` | 用户调度 + 可抢占 → 通知调度器 |
| `test_proc_no_time_kernel` | 内核调度 → 重置时间片 |
| `test_preempted_flag` | 高优先级入队触发低优先级进程 PREEMPTED |

---

## 6. 补充：struct proc 进程控制块详细分析

> 来源：tmp-06-proc-struct.md, draft/01~07-proc-struct-*.md

### 6.1 进程表布局

Minix3 的进程表是一个全局静态数组 `proc[NR_TASKS + NR_PROCS]`，定义在 `proc.h` 中：

- **前 NR_TASKS 个槽位**（索引 0 ~ NR_TASKS-1）：内核任务（IDLE、CLOCK、SYSTEM 等），`p_nr` 为负数
- **后 NR_PROCS 个槽位**（索引 NR_TASKS ~ NR_TASKS+NR_PROCS-1）：用户进程（PM、VFS、VM、INIT 等），`p_nr` 为非负数

关键常量：
- `NR_TASKS = 5`：内核任务数（ASYNCM=-5, IDLE=-4, CLOCK=-3, SYSTEM=-2, KERNEL=-1）
- `NR_PROCS = 256`：最大用户进程数
- `NR_SYS_PROCS = 64`：系统特权结构数
- `PMAGIC = 0xC0FFEE1`：proc 指针有效性魔数

### 6.2 struct proc 字段详解

定义于 `minix3/minix/kernel/proc.h:22-137`，按功能分组：

**寄存器与上下文**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_reg` | `struct stackframe_s` | 进程寄存器保存帧，上下文切换时保存/恢复 |
| `p_seg` | `struct segframe` | 段描述符（x86 下含 CR3 页表根指针、FPU 状态） |

**进程标识**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_nr` | `proc_nr_t` (int) | 进程槽位号，负数为内核任务，非负为用户进程，生命周期不变 |
| `p_priv` | `struct priv *` | 指向特权结构，系统进程有独立实例，用户进程共享默认实例 |
| `p_endpoint` | `endpoint_t` (int) | 含 generation 的进程标识，slot 重用时 generation 递增 |
| `p_name` | `char[16]` | 进程名（含 `\0`） |
| `p_magic` | `int` | 有效性魔数（PMAGIC = 0xC0FFEE1） |

**运行时状态**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_rts_flags` | `volatile u32_t` | 运行时标志，== 0 时可运行 |
| `p_misc_flags` | `volatile u32_t` | 杂项标志，不影响可运行性 |

**调度信息**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_priority` | `char` | 当前优先级 |
| `p_cpu_time_left` | `u64_t` | 剩余 CPU 时间 |
| `p_quantum_size_ms` | `unsigned` | 分配的时间量子（毫秒） |
| `p_scheduler` | `struct proc *` | 用户空间调度器进程，NULL 表示内核默认调度 |
| `p_cpu` | `unsigned` | 进程运行的 CPU 编号 |
| `p_nextready` | `struct proc *` | 就绪队列中下一个进程 |

**调度统计（p_accounting）**

| 字段 | 类型 | 含义 |
|------|------|------|
| `enter_queue` | `u64_t` | 入队时刻（CPU 周期） |
| `time_in_queue` | `u64_t` | 队列中等待时间 |
| `dequeues` | `unsigned long` | 出队次数 |
| `ipc_sync` | `unsigned long` | 同步 IPC 次数 |
| `ipc_async` | `unsigned long` | 异步 IPC 次数 |
| `preempted` | `unsigned long` | 被抢占次数 |

**时间统计**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_dequeued` | `clock_t` | 最近一次出队的 uptime |
| `p_user_time` | `clock_t` | 用户态时间（tick） |
| `p_sys_time` | `clock_t` | 内核态时间（tick） |
| `p_virt_left` | `clock_t` | 虚拟定时器剩余 tick |
| `p_prof_left` | `clock_t` | profile 定时器剩余 tick |
| `p_cycles` | `u64_t` | 消耗的 CPU 周期 |
| `p_kcall_cycles` | `u64_t` | 内核调用消耗的周期 |
| `p_kipc_cycles` | `u64_t` | IPC 消耗的周期 |
| `p_tick_cycles` | `u64_t` | 一个 tick 内累积的周期 |
| `p_cpuavg` | `struct cpuavg` | 运行 CPU 平均值（供 ps(1) 使用） |

**IPC 消息传递**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_caller_q` | `struct proc *` | 向此进程发送消息的进程队列头 |
| `p_q_link` | `struct proc *` | 发送等待队列中的链接 |
| `p_getfrom_e` | `endpoint_t` | 想从谁接收（RECEIVING 时有效） |
| `p_sendto_e` | `endpoint_t` | 想向谁发送（SENDING 时有效） |
| `p_pending` | `sigset_t` | 待处理的内核信号位图 |
| `p_sendmsg` | `message` | 发送方消息（SENDING 时有效） |
| `p_delivermsg` | `message` | 待投递给此进程的消息（MF_DELIVERMSG 时有效） |
| `p_delivermsg_vir` | `vir_bytes` | 消息投递目标虚拟地址 |

**VM 请求（p_vmrequest）**

| 字段 | 类型 | 含义 |
|------|------|------|
| `nextrestart` | `struct proc *` | VM 重启链中下一个进程 |
| `nextrequestor` | `struct proc *` | VM 请求链中下一个请求者 |
| `type` | `int` | 挂起操作类型（VMSTYPE_SYS_NONE=0 / KERNELCALL=1 / DELIVERMSG=2 / MAP=3） |
| `saved.reqmsg` | `message` | 挂起的请求消息 |
| `req_type` | `int` | VM 请求类型 |
| `target` | `endpoint_t` | VM 请求目标 |
| `params.check.start` | `vir_bytes` | 内存范围起始 |
| `params.check.length` | `vir_bytes` | 内存范围长度 |
| `params.check.writeflag` | `u8_t` | 写访问标志 |
| `vmresult` | `int` | VM 处理结果 |

### 6.3 RTS_FLAGS 完整定义

| 标志位 | 值 | 含义 | 置位场景 |
|--------|-----|------|---------|
| `RTS_SLOT_FREE` | 0x01 | 进程槽位空闲 | 进程退出或初始化时 |
| `RTS_PROC_STOP` | 0x02 | 进程被停止 | `sys_stop()` 或 IDLE 初始化 |
| `RTS_SENDING` | 0x04 | 进程阻塞于发送 | `mini_send()` 目标未就绪 |
| `RTS_RECEIVING` | 0x08 | 进程阻塞于接收 | `mini_receive()` 无消息可用 |
| `RTS_SIGNALED` | 0x10 | 有新内核信号到达 | 信号管理器发送信号 |
| `RTS_SIG_PENDING` | 0x20 | 信号处理中暂不可运行 | 信号处理流程中 |
| `RTS_P_STOP` | 0x40 | 进程被追踪（ptrace） | 调试器 attach |
| `RTS_NO_PRIV` | 0x80 | fork 的系统进程尚未获得特权 | `sys_fork()` 后特权未就绪 |
| `RTS_NO_ENDPOINT` | 0x100 | 进程不能发送/接收消息 | endpoint 未分配 |
| `RTS_VMINHIBIT` | 0x200 | 等待 VM 设置页表 | fork/exec 后页表未就绪 |
| `RTS_PAGEFAULT` | 0x400 | 进程有未处理的页缺失 | 访问未映射内存 |
| `RTS_VMREQUEST` | 0x800 | VM 内存请求的发起者 | 发起 VM 内存请求 |
| `RTS_VMREQTARGET` | 0x1000 | VM 内存请求的目标 | 作为 VM 内存请求目标 |
| `RTS_PREEMPTED` | 0x4000 | 被更高优先级进程抢占 | 调度时发现更高优先级 |
| `RTS_NO_QUANTUM` | 0x8000 | 时间片用完 | 时钟中断检测到量子耗尽 |
| `RTS_BOOTINHIBIT` | 0x10000 | 启动阶段等待 VM 就绪 | 系统启动初始化 |

### 6.4 MISC_FLAGS 完整定义

| 标志位 | 值 | 含义 |
|--------|-----|------|
| `MF_REPLY_PEND` | 0x001 | IPC_REQUEST 的回复待处理 |
| `MF_VIRT_TIMER` | 0x002 | 进程虚拟定时器运行中 |
| `MF_PROF_TIMER` | 0x004 | 进程 profile 定时器运行中 |
| `MF_KCALL_RESUME` | 0x008 | 内核调用被中断需恢复 |
| `MF_DELIVERMSG` | 0x040 | 有消息待投递给此进程 |
| `MF_SIG_DELAY` | 0x080 | 发送完成后需发送信号 |
| `MF_SC_ACTIVE` | 0x100 | 系统调用追踪：正在系统调用中 |
| `MF_SC_DEFER` | 0x200 | 系统调用追踪：延迟系统调用 |
| `MF_SC_TRACE` | 0x400 | 系统调用追踪：触发系统调用事件 |
| `MF_FPU_INITIALIZED` | 0x1000 | FPU/扩展寄存器已初始化（64 位重写中更名为 MF_EXT_REG_INITIALIZED） |
| `MF_SENDING_FROM_KERNEL` | 0x2000 | 消息来自内核 |
| `MF_CONTEXT_SET` | 0x4000 | 不修改上下文 |
| `MF_SPROF_SEEN` | 0x8000 | profile 已观测此进程 |
| `MF_FLUSH_TLB` | 0x10000 | 运行前需刷新 TLB（SMP） |
| `MF_SENDA_VM_MISS` | 0x20000 | 异步发送因 VM 修改地址空间而失败 |
| `MF_STEP` | 0x40000 | 单步执行 |
| `MF_MSGFAILED` | 0x80000 | 消息传递失败 |
| `MF_NICED` | 0x100000 | 用户降低了进程最大优先级 |

### 6.5 RTS_SET / RTS_UNSET 宏详解

**RTS_SET(rp, f)**：置位标志并自动出队

```c
#define RTS_SET(rp, f)
    do {
        const int rts = (rp)->p_rts_flags;
        (rp)->p_rts_flags |= (f);
        if(rts_f_is_runnable(rts) && !proc_is_runnable(rp)) {
            dequeue(rp);
        }
    } while(0)
```

先保存旧标志，置位新标志。若进程从可运行变为不可运行（旧标志为 0，新标志非 0），自动调用 `dequeue()`。

**RTS_UNSET(rp, f)**：清位标志并自动入队

```c
#define RTS_UNSET(rp, f)
    do {
        int rts;
        rts = (rp)->p_rts_flags;
        (rp)->p_rts_flags &= ~(f);
        if(!rts_f_is_runnable(rts) && proc_is_runnable(rp)) {
            enqueue(rp);
        }
    } while(0)
```

先保存旧标志，清位指定标志。若进程从不可运行变为可运行（旧标志非 0，新标志为 0），自动调用 `enqueue()`。

### 6.6 进程访问宏

> 来源：tmp_08-proc-macros.md, draft/08-proc-macros.md

**地址范围宏**：

| 宏 | 含义 |
|-----|------|
| `BEG_PROC_ADDR` | 进程表起始地址（`&proc[0]`） |
| `BEG_USER_ADDR` | 用户进程起始地址（`&proc[NR_TASKS]`） |
| `END_PROC_ADDR` | 进程表结束地址（`&proc[NR_TASKS + NR_PROCS]`） |

**指针/编号转换宏**：

| 宏 | 定义 | 含义 |
|-----|------|------|
| `proc_addr(n)` | `&proc[NR_TASKS + (n)]` | 进程号 → 进程指针 |
| `proc_nr(p)` | `p->p_nr` | 进程指针 → 进程号 |

**属性检查宏**：

| 宏 | 含义 |
|-----|------|
| `isokprocn(n)` | 检查进程号是否合法（0 <= n < NR_PROCS） |
| `isemptyp(p)` | `p->p_rts_flags == RTS_SLOT_FREE` |
| `iskernelp(p)` | `p < BEG_USER_ADDR`，判断是否为内核任务 |
| `isusern(n)` | 判断进程号是否为用户进程 |

### 6.7 proc_init() — 进程表初始化

`minix3/minix/kernel/proc.c:119-159`

**行为**：
1. 遍历 `proc[0]` ~ `proc[NR_TASKS + NR_PROCS - 1]`，对每个槽位：
   - 置 `p_rts_flags = RTS_SLOT_FREE`（标记空闲）
   - 置 `p_magic = PMAGIC`
   - 设 `p_nr` 从 `-NR_TASKS` 递增
   - 初始化 `p_endpoint = _ENDPOINT(0, p_nr)`（generation 为 0）
   - 清空调度器指针、优先级、时间片
   - 调用 `arch_proc_reset(rp)` 做架构相关初始化
2. 遍历 `priv[0]` ~ `priv[NR_SYS_PROCS - 1]`，对每个特权结构：
   - 置 `s_proc_nr = NONE`（标记空闲）
   - 设 `s_id` 为索引值
   - 建立 `ppriv_addr` 快速索引
3. 初始化 IDLE 进程：每个 CPU 一个 IDLE 进程，设置 `p_endpoint = IDLE`，`p_priv = &idle_priv`，`p_rts_flags |= RTS_PROC_STOP`（永不调度）

---

## 7. 补充：调度详细分析

> 来源：tmp-07-scheduling.md

### 7.1 switch_to_user() — 切换到用户态

`minix3/minix/kernel/proc.c:299-474`

调度的主入口，在内核处理完中断/系统调用后调用。**不是简单的"选进程→切换"，而是一个包含多个回跳点的状态机**：

1. **检查当前进程**：若当前进程仍可运行，跳到 `check_misc_flags`
2. **处理抢占**：若当前进程被抢占（`RTS_PREEMPTED`），清除抢占标志；若清除后进程仍可运行，根据是否有剩余时间片决定 `enqueue_head` 或 `enqueue`
3. **选择新进程**：循环调用 `pick_proc()`，若无就绪进程则 `idle()` 等待中断
4. **切换地址空间**：`switch_address_space(p)`
5. **处理杂项标志**（循环处理，直到所有标志清零）：
   - `MF_KCALL_RESUME`：恢复被中断的内核调用
   - `MF_DELIVERMSG`：投递待传递消息
   - `MF_SC_DEFER`：执行延迟的系统调用
   - `MF_SC_TRACE`：触发系统调用追踪事件
   - `MF_SC_ACTIVE`：清除系统调用活跃标志
6. **时间片检查**：若 `p_cpu_time_left == 0`，调用 `proc_no_time()`
7. **恢复上下文**：`arch_finish_switch_to_user()` → FPU 处理 → `restore_user_context(p)`（不返回）

**关键设计**：处理杂项标志时可能导致进程变为不可运行（如消息投递触发页缺失），此时需跳回重新选择进程。

### 7.2 idle() — 空闲循环

`minix3/minix/kernel/proc.c:176-232`

当没有可运行进程时，将 CPU 置于低功耗状态等待中断：

1. 设置 `proc_ptr` 为 IDLE 进程
2. 调用 `switch_address_space_idle()` 切换到确保内核映射可用的地址空间
3. 设置 `cpu_is_idle = 1`（SMP）
4. BSP 上重新启动本地定时器（AP 上停止定时器）
5. 调用 `halt_cpu()` 使 CPU 进入低功耗状态
6. 中断唤醒后返回到 `switch_to_user` 的 `pick_proc()` 循环

### 7.3 notify_scheduler() — 通知用户空间调度器

`minix3/minix/kernel/proc.c:1860-1891`

向进程的用户空间调度器发送 `SCHEDULING_NO_QUANTUM` 消息：

1. 设置 `RTS_NO_QUANTUM` 使进程出队
2. 构造消息，包含进程 endpoint、队列等待时间、出队次数、IPC 次数、抢占次数、CPU 编号等
3. 重置进程调度统计 `reset_proc_accounting(p)`
4. 通过 `mini_send()` 以内核身份发送消息给调度器

### 7.4 enqueue 中 enter_queue 的特殊记录方式

`enqueue()` 和 `enqueue_head()` 在记录入队时刻时，写入的是**当前运行进程**（`proc_ptr`）的 `p_accounting.enter_queue`，而非被入队进程的。这是 Minix3 的一个特殊设计，用于跟踪当前进程何时因调度事件被中断。

### 7.5 指针指针（Pointer Pointer）模式

`dequeue()` 使用 `struct proc **xpp` 遍历链表，无需对队首节点做特殊处理：

```c
for (xpp = &rdy_head[q]; *xpp; xpp = &(*xpp)->p_nextready) {
    if (*xpp == rp) {
        *xpp = (*xpp)->p_nextready;
        break;
    }
}
```

---

## 8. 参见

- [10-switch-to-user](10-switch-to-user.md) — 调用 enqueue/dequeue/pick_proc
- [09-vm-boot-protocol](09-vm-boot-protocol.md) — VMINHIBIT 对调度的影响
- [12-ipc-core](12-ipc-core.md) — SENDING/RECEIVING 对调度的影响
