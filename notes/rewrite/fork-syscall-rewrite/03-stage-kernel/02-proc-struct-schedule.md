# 02-proc-struct-schedule - 进程结构体调度字段

> 本文档分析 `minix3/minix/kernel/proc.h` 第 51-80 行，讲解进程结构体的调度相关字段。

---

## 1. 概述

本节介绍进程结构体中与调度相关的字段。这些字段控制进程的优先级、时间片分配、CPU 亲和性等调度行为。

**调度字段概览**：

| 字段 | 类型 | 作用 |
|------|------|------|
| `p_priority` | `char` | 当前优先级 |
| `p_cpu_time_left` | `u64_t` | 剩余 CPU 时间 |
| `p_quantum_size_ms` | `unsigned` | 时间片大小（毫秒） |
| `p_scheduler` | `struct proc *` | 用户态调度器指针 |
| `p_cpu` | `unsigned` | 当前运行的 CPU |
| `p_cpu_mask` | `bitchunk_t[]` | 允许运行的 CPU 掩码（SMP） |

这些字段共同决定了进程何时、在哪个 CPU 上、运行多长时间。

### 1.1 调度器设计

MINIX 采用**多级优先级队列调度**，核心特点：

1. **优先级队列**：共 16 个调度队列（`NR_SCHED_QUEUES = 16`）
2. **数值越小优先级越高**：`TASK_Q = 0` 最高，`MIN_USER_Q = 15` 最低
3. **抢占式调度**：高优先级进程总是抢占低优先级进程
4. **时间片轮转**：同优先级进程轮流运行

**优先级分布**：

```c
#define TASK_Q        0    /* 内核任务（最高优先级） */
#define MAX_USER_Q    0    /* 用户进程最高优先级 */
#define USER_Q        7    /* 用户进程默认优先级 */
#define MIN_USER_Q    15   /* 用户进程最低优先级 */
```

**调度流程**：
1. 从最高优先级队列开始扫描
2. 选择第一个非空队列的队首进程
3. 进程用完时间片后移到队列尾部
4. 高优先级进程就绪时抢占当前进程

### 1.2 与 fork 的关系

fork 时子进程的调度字段处理：

**继承自父进程**：
- `p_priority`：继承父进程优先级
- `p_scheduler`：继承父进程的调度器指针
- `p_quantum_size_ms`：继承父进程的时间片大小

**重新初始化**：
- `p_cpu_time_left = 0`：子进程没有剩余时间片
- `RTS_NO_QUANTUM`：设置标志，等待调度器分配时间片

**关键代码**：

```c
/* the child process is not runnable until it's scheduled. */
RTS_SET(rpc, RTS_NO_QUANTUM);
rpc->p_cpu_time_left = 0;
```

子进程创建后不能立即运行，必须等待调度器（如 PM）为其分配时间片，清除 `RTS_NO_QUANTUM` 标志后才能被调度。

---

## 2. C 源码分析

本节详细分析 `struct proc` 中与调度相关的字段定义，来源 `minix/kernel/proc.h`：

```c
struct proc {
  // ... 前面的字段见 01-proc-struct-basic.md
  
  char p_priority;              /* current process priority */
  u64_t p_cpu_time_left;        /* time left to use the cpu */
  unsigned p_quantum_size_ms;   /* assigned time quantum in ms */
  struct proc *p_scheduler;     /* who should get out of quantum msg */
  unsigned p_cpu;               /* what CPU is the process running on */
#ifdef CONFIG_SMP
  bitchunk_t p_cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)];
  bitchunk_t p_stale_tlb[BITMAP_CHUNKS(CONFIG_MAX_CPUS)];
#endif
  
  // ... 后续字段
};
```

这些字段控制进程的调度行为，包括优先级、时间片、CPU 亲和性等。

### 2.1 p_priority 字段

`p_priority` 是进程的当前优先级，决定进程在哪个调度队列中：

```c
char p_priority;  /* current process priority */
```

**作用**：

1. **队列索引**：`p_priority` 直接作为调度队列数组的索引
2. **调度顺序**：优先级数值越小，越先被调度
3. **动态调整**：可通过 `sched_proc()` 函数修改

**与调度队列的关系**：

```c
/* 选择进程时 */
for (q = 0; q < NR_SCHED_QUEUES; q++) {
    if ((rp = rdy_head[q])) {
        /* 找到优先级最高的就绪进程 */
        break;
    }
}
```

#### 2.1.1 优先级范围

优先级范围由 `NR_SCHED_QUEUES` 定义，共 16 级：

| 范围 | 名称 | 用途 |
|------|------|------|
| 0 | `TASK_Q` | 内核任务（时钟、系统任务等） |
| 0 | `MAX_USER_Q` | 用户进程最高优先级 |
| 1-6 | - | 高优先级用户进程 |
| 7 | `USER_Q` | 默认用户进程优先级 |
| 8-14 | - | 低优先级用户进程 |
| 15 | `MIN_USER_Q` | 用户进程最低优先级 |

**有效范围检查**：

```c
/* sched_proc() 中的验证 */
if ((priority < TASK_Q && priority != -1) || priority > NR_SCHED_QUEUES)
    return EINVAL;
```

`-1` 表示保持当前优先级不变。

#### 2.1.2 fork 时的优先级继承

fork 时子进程通过结构体复制继承父进程的优先级：

```c
*rpc = *rpp;  /* copy 'proc' struct，包括 p_priority */
```

**继承行为**：

- 子进程 `p_priority` = 父进程 `p_priority`
- 子进程进入与父进程相同的调度队列
- 子进程需要调度器重新分配时间片才能运行

**注意**：虽然优先级继承，但子进程设置了 `RTS_NO_QUANTUM`，必须等待调度器（PM/sched）调用 `sched_proc()` 分配时间片后才能运行。

### 2.2 p_cpu_time_left 字段

`p_cpu_time_left` 记录进程剩余的 CPU 时间（以 CPU 周期为单位）：

```c
u64_t p_cpu_time_left;  /* time left to use the cpu */
```

**作用**：

1. **时间片计数**：记录当前时间片还剩多少时间
2. **调度决策**：当减为 0 时，触发时间片用完事件
3. **精确计时**：使用 CPU 周期而非滴答，精度更高

**时间片用完处理**：

```c
/* 时钟中断中检查 */
if (proc_ptr->p_cpu_time_left == 0) {
    RTS_SET(proc_ptr, RTS_NO_QUANTUM);
    /* 通知调度器 */
}
```

#### 2.2.1 剩余时间片

剩余时间片表示进程在当前调度周期内还能运行多久：

**时间片生命周期**：

1. **分配**：调度器通过 `sched_proc()` 设置初始值
2. **消耗**：进程运行时递减
3. **耗尽**：减为 0 时触发 `RTS_NO_QUANTUM`

**与 p_quantum_size_ms 的关系**：

```
p_cpu_time_left = p_quantum_size_ms * cycles_per_ms
```

`p_quantum_size_ms` 是毫秒为单位的时间片大小，`p_cpu_time_left` 是转换为 CPU 周期后的实际计数器。

#### 2.2.2 fork 时的初始化

fork 时子进程的时间片初始化为 0：

```c
rpc->p_cpu_time_left = 0;
RTS_SET(rpc, RTS_NO_QUANTUM);
```

**原因**：

1. **时间片不继承**：父进程剩余的时间片不应给子进程
2. **等待调度**：子进程需要调度器重新分配时间片
3. **避免竞争**：防止父子进程同时使用相同的时间片

**后续流程**：

PM 完成进程创建后，会调用 `sched_proc()` 为子进程分配新的时间片，然后清除 `RTS_NO_QUANTUM` 标志。

### 2.3 p_quantum_size_ms 字段

`p_quantum_size_ms` 记录分配给进程的时间片大小（毫秒）：

```c
unsigned p_quantum_size_ms;  /* assigned time quantum in ms */
```

**作用**：

1. **时间片配置**：存储调度器分配的时间片大小
2. **重新调度参考**：时间片用完后，按此值重新分配
3. **进程属性**：不同进程可有不同的时间片大小

**默认值**：

```c
#define USER_QUANTUM 200  /* 默认用户进程时间片 200ms */
```

#### 2.3.1 时间片大小

时间片大小决定进程每次被调度后能连续运行多长时间：

**影响时间片大小的因素**：

1. **进程类型**：系统进程可能获得更长的时间片
2. **nice 值**：`nice` 值影响优先级和时间片
3. **调度策略**：用户态调度器可自定义分配策略

**时间片与响应性的权衡**：

- **大时间片**：吞吐量高，但响应延迟大
- **小时间片**：响应快，但上下文切换开销大

**MINIX 默认**：200ms，适合交互式系统。

### 2.4 p_scheduler 字段

`p_scheduler` 指向负责调度该进程的调度器进程：

```c
struct proc *p_scheduler;  /* who should get out of quantum msg */
```

**作用**：

1. **用户态调度**：允许用户态进程充当调度器
2. **时间片通知**：进程时间片用完时通知调度器
3. **调度委托**：内核将调度决策委托给用户态服务

**调度器指针的值**：

| 值 | 含义 |
|-----|------|
| `NULL` | 使用内核默认调度策略 |
| 指向自身 | 使用内核默认调度策略 |
| 指向其他进程 | 由该进程（用户态调度器）负责调度 |

#### 2.4.1 调度器指针

调度器指针实现了 MINIX 的**用户态调度器**机制：

**判断是否使用内核调度**：

```c
#define proc_kernel_scheduler(p)  ((p)->p_scheduler == NULL || \
                                    (p)->p_scheduler == (p))
```

**工作流程**：

1. 进程时间片用完
2. 内核检查 `p_scheduler`
3. 若为用户态调度器，发送消息通知
4. 用户态调度器决定新的时间片和优先级
5. 调用 `sched_proc()` 更新进程调度参数

这种设计允许实现复杂的调度策略（如实时调度、公平调度）而不修改内核。

#### 2.4.2 默认调度器

当 `p_scheduler` 为 `NULL` 或指向自身时，进程使用内核默认调度策略：

**初始化**：

```c
/* proc.c 中进程表初始化 */
rp->p_scheduler = NULL;  /* no user space scheduler */
```

**设置用户态调度器**：

```c
/* do_schedctl.c */
if (flags & SCHEDCTL_FLAG_KERNEL) {
    p->p_scheduler = NULL;  /* 使用内核调度 */
} else {
    p->p_scheduler = caller;  /* 调用者成为调度器 */
}
```

**MINIX 的调度服务**：`sched` 服务进程可以作为用户态调度器，实现更复杂的调度策略。

### 2.5 p_cpu 字段

`p_cpu` 记录进程当前运行的 CPU 编号：

```c
unsigned p_cpu;  /* what CPU is the process running on */
```

**作用**：

1. **CPU 标识**：记录进程在哪个 CPU 上执行
2. **调度决策**：帮助调度器选择合适的 CPU
3. **缓存亲和性**：尽量让进程在同一 CPU 运行，利用缓存

**单核系统**：`p_cpu` 始终为 0。

**多核系统**：进程可能在不同 CPU 间迁移，`p_cpu` 记录最近运行的 CPU。

#### 2.5.1 CPU 亲和性

CPU 亲和性（CPU Affinity）指进程倾向于在特定 CPU 上运行的特性：

**软亲和性**：

- 调度器尽量让进程在之前运行的 CPU 上继续运行
- 利用 CPU 缓存中的数据，减少缓存失效
- `p_cpu` 字段记录上次运行的 CPU

**硬亲和性**：

- 通过 `p_cpu_mask` 限制进程只能在特定 CPU 上运行
- 用于实时系统或特定硬件访问场景

**亲和性的好处**：

1. **缓存效率**：减少跨 CPU 的缓存迁移
2. **内存局部性**：NUMA 系统中保持内存访问局部性
3. **减少迁移开销**：避免不必要的上下文切换开销

#### 2.5.2 SMP 支持

SMP（对称多处理）环境下，调度器需要选择合适的 CPU：

**CPU 选择策略**：

1. **优先原 CPU**：如果原 CPU 空闲，优先选择
2. **负载均衡**：选择负载最轻的 CPU
3. **亲和性约束**：受 `p_cpu_mask` 限制

**调度流程**：

```c
/* 选择 CPU */
if (cpu == -1) {
    /* 使用默认策略：优先原 CPU */
    cpu = p->p_cpu;
}
/* 检查 CPU 掩码 */
if (!get_cpumask_bit(p, cpu)) {
    /* 选择允许的 CPU */
    cpu = pick_allowed_cpu(p);
}
```

**每个 CPU 独立调度队列**：

```c
struct proc *run_q_head[NR_SCHED_QUEUES];  /* 每个 CPU 有自己的队列 */
struct proc *run_q_tail[NR_SCHED_QUEUES];
```

### 2.6 SMP 相关字段

SMP 相关字段仅在 `CONFIG_SMP` 定义时编译：

```c
#ifdef CONFIG_SMP
  bitchunk_t p_cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)];  /* CPU 掩码 */
  bitchunk_t p_stale_tlb[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]; /* TLB 过期标记 */
#endif
```

**条件编译原因**：

1. **节省内存**：单核系统不需要这些字段
2. **简化代码**：避免单核系统处理 SMP 逻辑
3. **性能优化**：减少结构体大小，提高缓存效率

**Rust 处理方式**：

使用 `cfg` 条件编译或泛型参数，而非 C 的宏条件编译。

#### 2.6.1 p_cpu_mask

`p_cpu_mask` 是一个位图，标记进程允许在哪些 CPU 上运行：

```c
bitchunk_t p_cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)];
```

**位图结构**：

- 每一位对应一个 CPU
- 位为 1 表示允许在该 CPU 上运行
- 位为 0 表示禁止在该 CPU 上运行

**使用场景**：

1. **CPU 绑定**：将进程绑定到特定 CPU
2. **隔离**：将关键进程与普通进程隔离到不同 CPU
3. **NUMA 优化**：将进程限制在本地内存节点的 CPU

**操作示例**：

```c
/* 设置允许在 CPU 0 和 CPU 2 上运行 */
set_cpumask_bit(p, 0);
set_cpumask_bit(p, 2);
clear_cpumask_bit(p, 1);
```

#### 2.6.2 p_stale_tlb

`p_stale_tlb` 记录哪些 CPU 的 TLB 中可能有该进程的过期页表项：

```c
bitchunk_t p_stale_tlb[BITMAP_CHUNKS(CONFIG_MAX_CPUS)];
```

**TLB 过期问题**：

在 SMP 系统中，当一个 CPU 修改了页表，其他 CPU 的 TLB 可能还缓存着旧的映射。需要在进程运行前刷新这些过期条目。

**工作流程**：

1. 进程 A 在 CPU 0 运行，TLB 缓存了其页表项
2. 进程 A 迁移到 CPU 1
3. CPU 0 的 TLB 中仍有进程 A 的缓存
4. 设置 `p_stale_tlb[0] = 1`
5. 下次进程 A 在 CPU 0 运行前，刷新 TLB

**与硬件抽象的关系**：

TLB 刷新是硬件相关操作，需要通过 trait 抽象：

```rust
trait TlbFlush {
    fn flush(&self);
    fn flush_all(&self);
}
```

---

## 3. Rust 设计决策

本节讨论如何用 Rust 实现进程结构体的调度相关字段。

**核心设计原则**：

1. **类型安全**：优先级使用新类型，避免无效值
2. **trait 抽象**：调度器通过 trait 抽象，支持多种实现
3. **条件编译**：SMP 字段使用 `#[cfg]` 控制
4. **零成本抽象**：调度字段的设计不应引入运行时开销

**与基本字段的关系**：

调度字段在 `KProcess` 结构体中作为扩展，与基本字段（`p_nr`, `p_endpoint`, `p_rts_flags`）共同构成完整的进程结构体。

### 3.1 调度器 trait

调度器通过 trait 抽象，支持内核调度器和用户态调度器：

```rust
/// 调度器 trait
pub trait Scheduler {
    /// 时间片用完通知
    fn on_quantum_expired(&self, proc: &KProcess);
    
    /// 进程就绪通知
    fn on_process_ready(&self, proc: &KProcess);
    
    /// 选择下一个运行的进程
    fn pick_next(&self) -> Option<&KProcess>;
}

/// 内核默认调度器
pub struct KernelScheduler;

impl Scheduler for KernelScheduler {
    fn on_quantum_expired(&self, proc: &KProcess) {
        /* 重新分配默认时间片 */
    }
    
    fn on_process_ready(&self, _proc: &KProcess) {}
    
    fn pick_next(&self) -> Option<&KProcess> {
        /* 从优先级队列选择 */
        None
    }
}
```

**设计要点**：

- 调度器 trait 定义调度相关行为
- `p_scheduler` 字段可以是 `Option<&dyn Scheduler>`
- 内核调度器是默认实现

### 3.2 时间片管理

时间片使用 `AtomicU64` 管理，支持多核安全访问：

```rust
use core::sync::atomic::{AtomicU64, AtomicU32, Ordering};

/// 时间片管理
pub struct Quantum {
    /// 剩余 CPU 时间（周期数）
    pub cpu_time_left: AtomicU64,
    /// 时间片大小（毫秒）
    pub size_ms: AtomicU32,
}

impl Quantum {
    pub fn new(size_ms: u32) -> Self {
        Self {
            cpu_time_left: AtomicU64::new(0),
            size_ms: AtomicU32::new(size_ms),
        }
    }
    
    /// 分配新时间片
    pub fn allocate(&self, cycles_per_ms: u64) {
        let total = self.size_ms.load(Ordering::Relaxed) as u64 * cycles_per_ms;
        self.cpu_time_left.store(total, Ordering::Release);
    }
    
    /// 消耗时间
    pub fn consume(&self, cycles: u64) -> bool {
        let left = self.cpu_time_left.load(Ordering::Acquire);
        if left <= cycles {
            self.cpu_time_left.store(0, Ordering::Release);
            true  // 时间片用完
        } else {
            self.cpu_time_left.store(left - cycles, Ordering::Release);
            false
        }
    }
}
```

### 3.3 SMP 抽象

SMP 相关字段使用条件编译和泛型抽象：

```rust
/// CPU ID 类型
pub type CpuId = u32;

/// CPU 掩码（可变大小）
#[cfg(feature = "smp")]
pub struct CpuMask {
    bits: [AtomicU32; MAX_CPUS / 32],
}

#[cfg(feature = "smp")]
impl CpuMask {
    pub fn new() -> Self {
        Self { bits: [AtomicU32::new(0); MAX_CPUS / 32] }
    }
    
    pub fn is_set(&self, cpu: CpuId) -> bool {
        let idx = cpu as usize / 32;
        let bit = 1u32 << (cpu % 32);
        (self.bits[idx].load(Ordering::Acquire) & bit) != 0
    }
    
    pub fn set(&self, cpu: CpuId) {
        let idx = cpu as usize / 32;
        let bit = 1u32 << (cpu % 32);
        self.bits[idx].fetch_or(bit, Ordering::AcqRel);
    }
}

/// SMP 扩展字段
#[cfg(feature = "smp")]
pub struct SmpExt {
    pub cpu_mask: CpuMask,
    pub stale_tlb: CpuMask,
}

#[cfg(not(feature = "smp"))]
pub struct SmpExt;
```

**设计要点**：

- 使用 `#[cfg(feature = "smp")]` 控制编译
- 单核系统 `SmpExt` 为空结构体，零开销
- CPU 掩码操作使用原子类型保证线程安全

---

## 4. 实现

本节给出调度相关字段的 Rust 实现代码。代码位于 `os/kernel/src/proc.rs`。

**实现范围**：

1. 优先级字段（`Priority` 新类型）
2. 时间片管理（`Quantum` 结构体）
3. CPU 亲和性字段
4. SMP 扩展（条件编译）

**暂不实现**：

- 完整的调度器 trait 实现（需要 IPC 模块）
- 用户态调度器交互（需要 sched 服务）

### 4.1 调度字段定义

```rust
//! os/kernel/src/proc.rs
//! 调度相关字段实现

use core::sync::atomic::{AtomicU64, AtomicU32, AtomicI8, Ordering};

/// 优先级范围常量
pub mod priority {
    pub const TASK_Q: i8 = 0;
    pub const MAX_USER_Q: i8 = 0;
    pub const USER_Q: i8 = 7;
    pub const MIN_USER_Q: i8 = 15;
    pub const NR_SCHED_QUEUES: usize = 16;
}

/// 优先级新类型（封装有效性检查）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priority(i8);

impl Priority {
    pub fn new(value: i8) -> Option<Self> {
        if value >= priority::TASK_Q && value <= priority::MIN_USER_Q {
            Some(Self(value))
        } else {
            None
        }
    }
    
    pub fn get(&self) -> i8 {
        self.0
    }
    
    pub fn is_kernel(&self) -> bool {
        self.0 == priority::TASK_Q
    }
}

impl Default for Priority {
    fn default() -> Self {
        Self(priority::USER_Q)
    }
}

/// 时间片管理
#[derive(Debug)]
pub struct Quantum {
    pub cpu_time_left: AtomicU64,
    pub size_ms: AtomicU32,
}

/// CPU ID
pub type CpuId = u32;

/// 调度字段扩展
#[derive(Debug)]
pub struct SchedFields {
    pub priority: AtomicI8,
    pub quantum: Quantum,
    pub cpu: AtomicU32,
}

impl SchedFields {
    pub fn new() -> Self {
        Self {
            priority: AtomicI8::new(priority::USER_Q),
            quantum: Quantum::new(200),
            cpu: AtomicU32::new(0),
        }
    }
}
```

### 4.2 调度器接口

```rust
/// 调度器 trait（核心接口）
pub trait Scheduler: Send + Sync {
    /// 时间片用完回调
    fn on_quantum_expired(&self, proc: &KProcess);
    
    /// 进程就绪回调
    fn on_process_ready(&self, proc: &KProcess);
}

/// 调度控制接口
pub trait SchedControl {
    /// 设置进程调度参数
    fn set_params(&self, proc: &KProcess, priority: i8, quantum_ms: u32, cpu: CpuId);
    
    /// 获取进程调度器
    fn get_scheduler(&self, proc: &KProcess) -> Option<&dyn Scheduler>;
}

/// 内核默认调度器
pub struct KernelScheduler;

impl KernelScheduler {
    pub const fn new() -> Self {
        Self
    }
}

impl Scheduler for KernelScheduler {
    fn on_quantum_expired(&self, _proc: &KProcess) {
        /* 内核调度器重新分配默认时间片 */
    }
    
    fn on_process_ready(&self, _proc: &KProcess) {
        /* 加入调度队列 */
    }
}

/// 全局内核调度器实例
pub static KERNEL_SCHEDULER: KernelScheduler = KernelScheduler::new();
```

### 4.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_priority_valid() {
        assert!(Priority::new(0).is_some());
        assert!(Priority::new(7).is_some());
        assert!(Priority::new(15).is_some());
        assert!(Priority::new(-1).is_none());
        assert!(Priority::new(16).is_none());
    }
    
    #[test]
    fn test_priority_default() {
        let p = Priority::default();
        assert_eq!(p.get(), priority::USER_Q);
    }
    
    #[test]
    fn test_priority_is_kernel() {
        let kernel = Priority::new(priority::TASK_Q).unwrap();
        assert!(kernel.is_kernel());
        
        let user = Priority::new(priority::USER_Q).unwrap();
        assert!(!user.is_kernel());
    }
    
    #[test]
    fn test_quantum_allocate() {
        let q = Quantum::new(200);
        assert_eq!(q.size_ms.load(Ordering::Relaxed), 200);
        
        q.allocate(1_000_000);  // 1M cycles per ms
        assert_eq!(q.cpu_time_left.load(Ordering::Relaxed), 200_000_000);
    }
    
    #[test]
    fn test_quantum_consume() {
        let q = Quantum::new(200);
        q.allocate(1);  // cpu_time_left = 200 * 1 = 200
        
        assert!(!q.consume(50));  // 200 - 50 = 150
        assert_eq!(q.cpu_time_left.load(Ordering::Relaxed), 150);
        
        assert!(q.consume(200));  // 150 < 200, 用完
        assert_eq!(q.cpu_time_left.load(Ordering::Relaxed), 0);
    }
    
    #[test]
    fn test_sched_fields_new() {
        let sf = SchedFields::new();
        assert_eq!(sf.priority.load(Ordering::Relaxed), priority::USER_Q);
        assert_eq!(sf.cpu.load(Ordering::Relaxed), 0);
    }
}
```

---

## 5. 参见

- [01-proc-struct-basic](01-proc-struct-basic.md) - 基本字段
- [03-proc-struct-accounting](03-proc-struct-accounting.md) - 统计字段
