# 03-proc-struct-accounting - 进程结构体统计字段

> 本文档分析 `minix3/minix/kernel/proc.h` 第 81-120 行，讲解进程结构体的统计相关字段。

---

## 1. 概述

进程统计字段用于记录进程的资源使用情况，包括 CPU 时间、调度统计、IPC 计数等。这些信息对于系统监控、性能分析和调度决策至关重要。

Minix3 的进程统计分为两大类：
1. **调度统计**：`p_accounting` 结构体，用于调度器决策
2. **资源统计**：用户/系统时间、CPU 周期等，用于系统监控

### 1.1 进程统计

进程统计的主要目的包括：

1. **调度决策**：调度器根据 `p_accounting` 中的统计信息（如等待时间、抢占次数）做出调度决策
2. **资源监控**：`p_user_time` 和 `p_sys_time` 记录进程在用户态和内核态的执行时间
3. **性能分析**：`p_cycles` 系列字段精确记录 CPU 周期消耗
4. **IPC 统计**：记录同步/异步 IPC 调用次数，用于分析进程间通信模式

### 1.2 与 fork 的关系

fork 时，子进程的统计字段需要重置，原因如下：

1. **统计独立性**：子进程是新进程，应从零开始统计
2. **调度公平性**：避免子进程继承父进程的调度统计（如等待时间）

在 `do_fork.c` 中，fork 时调用 `reset_proc_accounting(rpc)` 重置调度统计，并清零周期计数：

```c
reset_proc_accounting(rpc);
rpc->p_cpu_time_left = 0;
rpc->p_cycles = 0;
rpc->p_kcall_cycles = 0;
rpc->p_kipc_cycles = 0;
rpc->p_tick_cycles = 0;
cpuavg_init(&rpc->p_cpuavg);
```

---

## 2. C 源码分析

本节详细分析 `proc.h` 中定义的统计相关字段，这些字段位于进程结构体的第 54-68 行。

### 2.1 p_accounting 结构体

`p_accounting` 是一个嵌套结构体，专门用于收集调度相关的统计信息。这些信息会被传递给进程的调度器（`p_scheduler`），用于做出调度决策。

```c
struct {
    u64_t enter_queue;      /* time when enqueued (cycles) */
    u64_t time_in_queue;    /* time spent in queue */
    unsigned long dequeues;
    unsigned long ipc_sync;
    unsigned long ipc_async;
    unsigned long preempted;
} p_accounting;
```

该结构体在 `proc.c` 的 `reset_proc_accounting()` 函数中被重置：

```c
void reset_proc_accounting(struct proc *p)
{
    p->p_accounting.preempted = 0;
    p->p_accounting.ipc_sync  = 0;
    p->p_accounting.ipc_async = 0;
    p->p_accounting.dequeues  = 0;
    p->p_accounting.time_in_queue = 0;
    p->p_accounting.enter_queue = 0;
}
```

#### 2.1.1 enter_queue 字段

`enter_queue` (`u64_t`) 记录进程进入调度队列的时间戳（以 CPU 周期为单位）。

**用途**：
- 在进程入队时，通过 `read_tsc_64()` 读取当前时间戳计数器（TSC）
- 在进程出队时，计算 `当前时间 - enter_queue` 得到等待时间
- 累加到 `time_in_queue` 字段

**相关代码**（`proc.c` 入队时）：
```c
read_tsc_64(&(get_cpulocal_var(proc_ptr)->p_accounting.enter_queue));
```

**出队时计算等待时间**：
```c
if (rp->p_accounting.enter_queue) {
    read_tsc_64(&tsc);
    tsc_delta = tsc - rp->p_accounting.enter_queue;
    rp->p_accounting.time_in_queue += tsc_delta;
    rp->p_accounting.enter_queue = 0;
}
```

#### 2.1.2 time_in_queue 字段

`time_in_queue` (`u64_t`) 累计进程在调度队列中的等待时间（以 CPU 周期为单位）。

**用途**：
- 调度器可根据此值判断进程是否"饥饿"（等待时间过长）
- 用于计算进程的平均等待时间
- 帮助调度器做出公平调度决策

**更新时机**：每次进程出队时累加等待时间，值越大说明进程等待越久。

#### 2.1.3 dequeues 计数

`dequeues` (`unsigned long`) 统计进程被调出 CPU 的次数。

**用途**：
- 记录进程被调度器选中执行后又放回队列的次数
- 高 dequeues 值可能表示进程频繁被抢占或时间片用完
- 调度器可据此调整进程优先级或时间片

**更新时机**：
- 进程出队时 `dequeues++`
- 进程因抢占重新入队时 `dequeues--`（表示未真正执行完）

**传递给调度器**：
```c
m_no_quantum.m_krn_lsys_schedule.acnt_deqs = p->p_accounting.dequeues;
```

#### 2.1.4 ipc_sync 计数

`ipc_sync` (`unsigned long`) 统计进程执行的同步 IPC 调用次数。

**同步 IPC 类型**：
- `SEND`：发送消息（阻塞直到对方接收）
- `RECEIVE`：接收消息（阻塞直到有消息）
- `SENDREC`：发送并等待回复
- `SENDNB`：非阻塞发送

**更新时机**（`proc.c`）：
```c
case SEND:
case RECEIVE:
case SENDREC:
case SENDNB:
{
    caller_ptr->p_accounting.ipc_sync++;
    return do_sync_ipc(caller_ptr, call_nr, ...);
}
```

**用途**：调度器可据此判断进程是 IPC 密集型还是 CPU 密集型。

#### 2.1.5 ipc_async 计数

`ipc_async` (`unsigned long`) 统计进程执行的异步 IPC 调用次数。

**异步 IPC 类型**：
- `NOTIFY`：异步通知（不阻塞）
- `SENDA`：异步发送数组消息

**更新时机**（`proc.c`）：
```c
case NOTIFY:
case SENDA:
{
    caller_ptr->p_accounting.ipc_async++;
    // 处理异步 IPC...
}
```

**用途**：异步 IPC 通常用于轻量级通信，高 ipc_async 值表示进程频繁进行非阻塞通信。

#### 2.1.6 preempted 计数

`preempted` (`unsigned long`) 统计进程被抢占的次数。

**抢占场景**：
- 高优先级进程变为就绪时，当前进程可能被抢占
- 进程时间片未用完就被调出 CPU

**更新时机**（`proc.c`）：
```c
/* 进程被抢占后重新入队 */
rp->p_accounting.dequeues--;
rp->p_accounting.preempted++;
```

**用途**：
- 高 preempted 值表示进程频繁被高优先级进程打断
- 调度器可据此调整进程优先级，减少抢占开销

### 2.2 p_dequeued 字段

`p_dequeued` (`clock_t`) 记录进程最后一次被调出队列的单调时间。

**用途**：
- 主要供 `ps(1)` 命令使用
- 计算进程的当前运行时长
- 显示进程已运行多长时间

**更新时机**（`proc.c`）：
```c
/* For ps(1), remember when the process was last dequeued. */
rp->p_dequeued = get_monotonic();
```

**类型说明**：`clock_t` 是系统时钟滴答数，通过 `get_monotonic()` 获取。

### 2.3 p_user_time 字段

`p_user_time` (`clock_t`) 记录进程在用户态执行的累计时间（以时钟滴答为单位）。

**用途**：
- 实现 `times()` 系统调用
- `ps(1)` 显示进程用户态时间
- 性能分析和资源统计

#### 2.3.1 用户态时间

用户态时间统计进程执行用户代码的时间，不包括系统调用和内核服务时间。

**更新时机**（`clock.c` 时钟中断处理）：
```c
p = get_cpulocal_var(proc_ptr);
p->p_user_time++;

if (! (priv(p)->s_flags & BILLABLE)) {
    billp->p_sys_time++;  /* 非计费进程的时间记到 bill_ptr */
}
```

**说明**：每个时钟滴答（通常 1ms），当前运行的进程 `p_user_time` 递增。

#### 2.3.2 fork 时的初始化

fork 时，子进程的用户时间初始化为 0，因为子进程是新进程，尚未执行任何用户代码。

**代码**（`do_fork.c`）：
```c
rpc->p_user_time = 0;  /* set all the accounting times to 0 */
rpc->p_sys_time = 0;
```

**设计理由**：子进程从 fork 返回后开始独立执行，其时间统计应从零开始，不继承父进程的累计时间。

### 2.4 p_sys_time 字段

`p_sys_time` (`clock_t`) 记录进程在内核态执行的累计时间（以时钟滴答为单位）。

**用途**：
- 统计进程执行系统调用的时间
- 分析内核服务开销
- 与 `p_user_time` 一起计算进程总 CPU 时间

#### 2.4.1 内核态时间

内核态时间统计进程因系统调用、缺页处理等进入内核后执行的时间。

**更新时机**（`clock.c`）：
```c
if (! (priv(p)->s_flags & BILLABLE)) {
    billp->p_sys_time++;  /* 非计费进程的时间记到 bill_ptr */
}
```

**计费机制**：
- `BILLABLE` 标志的进程：时间记到自身
- 非 `BILLABLE` 进程（如系统服务）：时间记到 `bill_ptr`（通常是调用者）

#### 2.4.2 fork 时的初始化

fork 时，子进程的系统时间同样初始化为 0。

**代码**（`do_fork.c`）：
```c
rpc->p_user_time = 0;
rpc->p_sys_time = 0;
```

**设计理由**：子进程尚未执行任何系统调用，其内核态时间应从零开始统计。

### 2.5 p_virt_left 字段

`p_virt_left` (`clock_t`) 记录虚拟定时器（Virtual Timer）的剩余滴答数。

**用途**：
- 实现 `setitimer(ITIMER_VIRTUAL)` 系统调用
- 仅统计进程在用户态执行的时间
- 定时器到期时发送 `SIGVTALRM` 信号

#### 2.5.1 虚拟定时器

虚拟定时器仅在进程于用户态执行时递减，不包括内核态时间。

**更新时机**（`clock.c`）：
```c
if ((p->p_misc_flags & MF_VIRT_TIMER) && (p->p_virt_left > 0)) {
    p->p_virt_left--;
}
```

**触发条件**：
- `MF_VIRT_TIMER` 标志被设置
- 进程在用户态执行时，每个时钟滴答递减
- 递减到 0 时触发 `SIGVTALRM` 信号

#### 2.5.2 fork 时的处理

fork 时，子进程的虚拟定时器被禁用并清零。

**代码**（`do_fork.c`）：
```c
rpc->p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | ...);
rpc->p_virt_left = 0;  /* disable, clear the process-virtual timers */
rpc->p_prof_left = 0;
```

**设计理由**：
- 定时器是进程特定的资源，不应继承
- 子进程需要自己设置定时器
- 避免子进程意外收到 `SIGVTALRM` 信号

### 2.6 p_prof_left 字段

`p_prof_left` (`clock_t`) 记录 profiling 定时器（Profile Timer）的剩余滴答数。

**用途**：
- 实现 `setitimer(ITIMER_PROF)` 系统调用
- 统计进程在用户态和内核态的总时间
- 定时器到期时发送 `SIGPROF` 信号

#### 2.6.1 profiling 定时器

Profiling 定时器在进程执行时（包括用户态和内核态）递减，常用于性能分析工具（如 gprof）。

**更新时机**（`clock.c`）：
```c
if ((p->p_misc_flags & MF_PROF_TIMER) && (p->p_prof_left > 0)) {
    p->p_prof_left--;
}
```

**与虚拟定时器的区别**：
| 定时器类型 | 统计范围 | 信号 |
|-----------|---------|------|
| Virtual (`p_virt_left`) | 仅用户态 | `SIGVTALRM` |
| Profile (`p_prof_left`) | 用户态 + 内核态 | `SIGPROF` |

---

## 3. 周期计数字段

本节分析 CPU 周期计数相关字段，这些字段使用 `u64_t` 类型，提供比时钟滴答更精确的时间统计。

### 3.1 p_cycles 字段

`p_cycles` (`u64_t`) 记录进程消耗的 CPU 周期总数。

**用途**：
- 精确的性能分析（比时钟滴答更精确）
- CPU 使用率计算
- 性能监控工具（如 `top`、`ps`）

#### 3.1.1 CPU 周期统计

CPU 周期通过读取时间戳计数器（TSC，Time Stamp Counter）来统计。TSC 是 x86 架构提供的硬件计数器，每个时钟周期递增。

**更新方式**（`arch/i386/arch_clock.c`）：
```c
read_tsc_64(&tsc);
p->p_cycles = p->p_cycles + tsc - *__tsc_ctr_switch;
```

**统计时机**：
- 进程被调度出 CPU 时
- 计算本次运行消耗的周期数 = 当前 TSC - 上次切换时的 TSC
- 累加到 `p_cycles`

#### 3.1.2 fork 时的初始化

fork 时，子进程的 CPU 周期数初始化为 0。

**代码**（`do_fork.c`）：
```c
rpc->p_cycles = 0;
rpc->p_kcall_cycles = 0;
rpc->p_kipc_cycles = 0;
rpc->p_tick_cycles = 0;
```

**设计理由**：子进程是新进程，尚未消耗任何 CPU 周期。

### 3.2 p_kcall_cycles 字段

`p_kcall_cycles` (`u64_t`) 记录进程执行系统调用时消耗的 CPU 周期数。

**用途**：
- 分析系统调用开销
- 识别系统调用密集型进程
- 性能调优参考

### 3.3 p_kipc_cycles 字段

`p_kipc_cycles` (`u64_t`) 记录进程执行 IPC 操作时消耗的 CPU 周期数。

**用途**：
- 分析 IPC 开销
- 识别 IPC 密集型进程
- 优化进程间通信性能

### 3.4 p_tick_cycles 字段

`p_tick_cycles` (`u64_t`) 记录当前时钟滴答内累计的 CPU 周期数。

**用途**：
- 用于时钟中断处理时的周期统计
- 在时钟滴答边界更新 `p_user_time` 和 `p_sys_time`
- 避免频繁更新统计字段

### 3.5 p_cpuavg 字段

`p_cpuavg` (`struct cpuavg`) 记录进程的 CPU 使用率平均值，用于 `ps(1)` 显示。

**结构体定义**（`minix/type.h`）：
```c
struct cpuavg {
    clock_t ca_base;    /* start of current per-second slot, or 0 */
    uint32_t ca_run;    /* running ticks since start of slot, FSCALE */
    uint32_t ca_last;   /* running ticks during last second, FSCALE */
    uint32_t ca_avg;    /* decaying CPU utilization average, FSCALE */
};
```

**用途**：
- 计算进程的 CPU 使用率百分比
- 使用衰减平均算法平滑显示
- 初始化通过 `cpuavg_init(&rpc->p_cpuavg)` 完成

---

## 4. Rust 设计决策

本节讨论如何用 Rust 实现统计字段，重点考虑类型安全、原子操作和可扩展性。

### 4.1 统计结构体

将统计字段封装为独立的结构体，提高代码组织性。

**设计考虑**：
1. `Accounting` 结构体：对应 `p_accounting`，包含调度统计
2. `TimeStats` 结构体：包含用户/系统时间统计
3. `CpuCycles` 结构体：包含周期计数统计

**优点**：
- 字段分组清晰
- 便于批量重置
- 支持独立测试

### 4.2 时间类型

选择合适的时间类型，平衡精度和兼容性。

**类型选择**：
| 字段 | C 类型 | Rust 类型 | 说明 |
|-----|--------|----------|------|
| 时钟滴答 | `clock_t` | `u64` | 系统滴答数 |
| CPU 周期 | `u64_t` | `u64` | TSC 计数 |
| 时间戳 | `u64_t` | `u64` | TSC 值 |

**设计原则**：
- 使用 `u64` 统一表示，避免溢出
- 提供类型别名 `ClockTicks` 和 `CpuCycles` 增强可读性
- 考虑使用 `Duration` 类型进行时间计算

### 4.3 原子计数

统计字段可能被多个 CPU 并发访问，需要考虑同步机制。

**同步策略**：
1. **读多写少**：使用 `AtomicU64` 进行原子更新
2. **批量重置**：在进程创建/销毁时，单线程环境下可直接赋值
3. **性能优先**：统计精度可适当牺牲，避免锁开销

**实现建议**：
```rust
pub struct Accounting {
    pub enter_queue: AtomicU64,
    pub time_in_queue: AtomicU64,
    pub dequeues: AtomicU32,
    pub ipc_sync: AtomicU32,
    pub ipc_async: AtomicU32,
    pub preempted: AtomicU32,
}
```

---

## 5. 实现

本节给出统计字段的 Rust 实现代码。

### 5.1 Accounting 结构体定义

```rust
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// 时钟滴答类型
pub type ClockTicks = u64;

/// CPU 周期类型
pub type CpuCycles = u64;

/// 调度统计结构体
#[derive(Debug)]
pub struct Accounting {
    /// 入队时间戳（CPU 周期）
    pub enter_queue: AtomicU64,
    /// 队列中累计等待时间（CPU 周期）
    pub time_in_queue: AtomicU64,
    /// 出队次数
    pub dequeues: AtomicU32,
    /// 同步 IPC 次数
    pub ipc_sync: AtomicU32,
    /// 异步 IPC 次数
    pub ipc_async: AtomicU32,
    /// 被抢占次数
    pub preempted: AtomicU32,
}

impl Accounting {
    pub fn new() -> Self {
        Self {
            enter_queue: AtomicU64::new(0),
            time_in_queue: AtomicU64::new(0),
            dequeues: AtomicU32::new(0),
            ipc_sync: AtomicU32::new(0),
            ipc_async: AtomicU32::new(0),
            preempted: AtomicU32::new(0),
        }
    }

    pub fn reset(&self) {
        self.enter_queue.store(0, Ordering::Release);
        self.time_in_queue.store(0, Ordering::Release);
        self.dequeues.store(0, Ordering::Release);
        self.ipc_sync.store(0, Ordering::Release);
        self.ipc_async.store(0, Ordering::Release);
        self.preempted.store(0, Ordering::Release);
    }
}

impl Default for Accounting {
    fn default() -> Self {
        Self::new()
    }
}

/// 时间统计结构体
#[derive(Debug)]
pub struct TimeStats {
    /// 用户态时间（时钟滴答）
    pub user_time: AtomicU64,
    /// 内核态时间（时钟滴答）
    pub sys_time: AtomicU64,
    /// 虚拟定时器剩余
    pub virt_left: AtomicU64,
    /// Profiling 定时器剩余
    pub prof_left: AtomicU64,
}

impl TimeStats {
    pub fn new() -> Self {
        Self {
            user_time: AtomicU64::new(0),
            sys_time: AtomicU64::new(0),
            virt_left: AtomicU64::new(0),
            prof_left: AtomicU64::new(0),
        }
    }
}

impl Default for TimeStats {
    fn default() -> Self {
        Self::new()
    }
}

/// CPU 周期统计结构体
#[derive(Debug)]
pub struct CyclesStats {
    /// 总 CPU 周期
    pub total: AtomicU64,
    /// 系统调用周期
    pub kcall: AtomicU64,
    /// IPC 周期
    pub kipc: AtomicU64,
    /// 当前滴答周期
    pub tick: AtomicU64,
}

impl CyclesStats {
    pub fn new() -> Self {
        Self {
            total: AtomicU64::new(0),
            kcall: AtomicU64::new(0),
            kipc: AtomicU64::new(0),
            tick: AtomicU64::new(0),
        }
    }
}

impl Default for CyclesStats {
    fn default() -> Self {
        Self::new()
    }
}
```

### 5.2 统计方法实现

```rust
impl Accounting {
    /// 记录入队时间
    pub fn record_enqueue(&self, tsc: CpuCycles) {
        self.enter_queue.store(tsc, Ordering::Release);
    }

    /// 记录出队并累加等待时间
    pub fn record_dequeue(&self, tsc: CpuCycles) {
        let enter = self.enter_queue.load(Ordering::Acquire);
        if enter > 0 && tsc > enter {
            let delta = tsc - enter;
            self.time_in_queue.fetch_add(delta, Ordering::AcqRel);
        }
        self.enter_queue.store(0, Ordering::Release);
        self.dequeues.fetch_add(1, Ordering::AcqRel);
    }

    /// 记录同步 IPC
    pub fn record_ipc_sync(&self) {
        self.ipc_sync.fetch_add(1, Ordering::AcqRel);
    }

    /// 记录异步 IPC
    pub fn record_ipc_async(&self) {
        self.ipc_async.fetch_add(1, Ordering::AcqRel);
    }

    /// 记录抢占
    pub fn record_preempt(&self) {
        self.preempted.fetch_add(1, Ordering::AcqRel);
    }
}

impl TimeStats {
    /// 增加用户态时间
    pub fn add_user_time(&self, ticks: ClockTicks) {
        self.user_time.fetch_add(ticks, Ordering::AcqRel);
    }

    /// 增加内核态时间
    pub fn add_sys_time(&self, ticks: ClockTicks) {
        self.sys_time.fetch_add(ticks, Ordering::AcqRel);
    }

    /// 递减虚拟定时器
    pub fn tick_virt_timer(&self) -> bool {
        let left = self.virt_left.load(Ordering::Acquire);
        if left > 0 {
            self.virt_left.store(left - 1, Ordering::Release);
            left == 1
        } else {
            false
        }
    }

    /// 递减 profiling 定时器
    pub fn tick_prof_timer(&self) -> bool {
        let left = self.prof_left.load(Ordering::Acquire);
        if left > 0 {
            self.prof_left.store(left - 1, Ordering::Release);
            left == 1
        } else {
            false
        }
    }
}

impl CyclesStats {
    /// 累加 CPU 周期
    pub fn add_cycles(&self, cycles: CpuCycles) {
        self.total.fetch_add(cycles, Ordering::AcqRel);
    }

    /// 累加系统调用周期
    pub fn add_kcall_cycles(&self, cycles: CpuCycles) {
        self.kcall.fetch_add(cycles, Ordering::AcqRel);
    }

    /// 累加 IPC 周期
    pub fn add_kipc_cycles(&self, cycles: CpuCycles) {
        self.kipc.fetch_add(cycles, Ordering::AcqRel);
    }
}
```

### 5.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accounting_new() {
        let acc = Accounting::new();
        assert_eq!(acc.enter_queue.load(Ordering::Relaxed), 0);
        assert_eq!(acc.dequeues.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_accounting_reset() {
        let acc = Accounting::new();
        acc.dequeues.store(10, Ordering::Release);
        acc.ipc_sync.store(5, Ordering::Release);
        acc.reset();
        assert_eq!(acc.dequeues.load(Ordering::Relaxed), 0);
        assert_eq!(acc.ipc_sync.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_accounting_enqueue_dequeue() {
        let acc = Accounting::new();
        acc.record_enqueue(1000);
        acc.record_dequeue(1500);
        
        assert_eq!(acc.time_in_queue.load(Ordering::Relaxed), 500);
        assert_eq!(acc.dequeues.load(Ordering::Relaxed), 1);
        assert_eq!(acc.enter_queue.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_accounting_ipc() {
        let acc = Accounting::new();
        acc.record_ipc_sync();
        acc.record_ipc_sync();
        acc.record_ipc_async();
        
        assert_eq!(acc.ipc_sync.load(Ordering::Relaxed), 2);
        assert_eq!(acc.ipc_async.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_time_stats() {
        let ts = TimeStats::new();
        ts.add_user_time(10);
        ts.add_sys_time(5);
        
        assert_eq!(ts.user_time.load(Ordering::Relaxed), 10);
        assert_eq!(ts.sys_time.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_virt_timer() {
        let ts = TimeStats::new();
        ts.virt_left.store(3, Ordering::Release);
        
        assert!(!ts.tick_virt_timer());  // 3 -> 2
        assert!(!ts.tick_virt_timer());  // 2 -> 1
        assert!(ts.tick_virt_timer());   // 1 -> 0, expired
        assert!(!ts.tick_virt_timer());  // already 0
    }

    #[test]
    fn test_cycles_stats() {
        let cs = CyclesStats::new();
        cs.add_cycles(1000);
        cs.add_kcall_cycles(200);
        cs.add_kipc_cycles(50);
        
        assert_eq!(cs.total.load(Ordering::Relaxed), 1000);
        assert_eq!(cs.kcall.load(Ordering::Relaxed), 200);
        assert_eq!(cs.kipc.load(Ordering::Relaxed), 50);
    }
}
```

---

## 6. 参见

- [02-proc-struct-schedule](02-proc-struct-schedule.md) - 调度字段
- [04-proc-struct-ipc](04-proc-struct-ipc.md) - IPC 字段
