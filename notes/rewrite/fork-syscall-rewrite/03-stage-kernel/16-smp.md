# 16-smp: 多核协同

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/smp.c`, `minix3/minix/kernel/smp.h`, `minix3/minix/kernel/cpulocals.h`
> **说明**: BKL (Big Kernel Lock)、per-CPU 数据、IPI 跨 CPU 调度、CPU 亲和性
> **前置**: 11-scheduling-primitives.md
> **创建**: 2026-06-13

---

## 1. 概述

### 1.1 概念定义

Minix3 内核使用 BKL (Big Kernel Lock) 实现 SMP 同步——一个自旋锁保证同一时刻只有一个 CPU 在内核态执行。这是一种粗粒度但简单的 SMP 策略：

1. **BKL**：spinlock，临界区内禁止睡眠/调度/等待 IPC
2. **per-CPU 数据**：每个 CPU 独立的调度队列、当前进程指针、FPU 所有者
3. **IPI**：跨 CPU 中断，用于通知其他 CPU 执行调度操作（停止进程、VM 抑制等）
4. **CPU 亲和性**：进程绑定到特定 CPU，跨 CPU 迁移需要同步操作

### 1.2 与 Minix3 的对应关系

| Minix3 概念 | C 源码位置 | 说明 |
|-------------|-----------|------|
| `big_kernel_lock` | smp.c:27 | 全局内核自旋锁 |
| `bsp_cpu_id` | smp.h:17 | BSP CPU 编号 |
| `ncpus` | smp.h:12 | CPU 总数 |
| `struct cpu cpus[]` | smp.h:36 | CPU 状态数组 |
| `__cpu_local_vars` | cpulocals.h:67 | per-CPU 数据 |
| `smp_schedule()` | smp.c:75 | 发送调度 IPI |
| `smp_schedule_sync()` | smp.c:84 | 同步跨 CPU 操作 |
| `smp_sched_handler()` | smp.c:143 | IPI 调度处理 |
| `smp_ipi_sched_handler()` | smp.c:174 | IPI 确认+抢占 |

### 1.3 关键状态/机制说明

**BKL 约束**：
- 临界区内禁止：睡眠、调度、等待 IPC、等待锁
- 原因：BKL 是 spinlock（busy-wait），任何导致 CPU 让出执行权的操作都可能死锁
- `BKL_LOCK()` / `BKL_UNLOCK()` 成对使用

**per-CPU 数据** (`__cpu_local_vars`)：
- `proc_ptr`：当前运行的进程指针
- `bill_ptr`：计费进程指针
- `idle_proc`：idle 进程 slot 索引
- `ptproc`：当前页表进程
- `run_q_head/tail`：CPU 私有调度队列
- `cpu_is_idle`：CPU 是否空闲
- `idle_interrupted`：idle 被中断标志
- `tsc_ctr_switch`/`cpu_last_tsc`/`cpu_last_idle`：TSC 时间戳记账
- `fpu_presence`：FPU 是否存在
- `fpu_owner`：FPU 当前所有者

**IPI 调度操作**：
- `SCHED_IPI_STOP_PROC`：停止目标进程
- `SCHED_IPI_VM_INHIBIT`：设置 VMINHIBIT
- `SCHED_IPI_SAVE_CTX`：保存完整上下文（含 FPU）

### 1.4 行为规则

1. **BKL 持有者唯一**：同一时刻只有一个 CPU 持有 BKL
2. **IPI 异步性**：`smp_schedule()` 是异步的，仅发送 IPI；`smp_schedule_sync()` 是同步的，等待目标 CPU 完成
3. **CPU 就绪标志**：`CPU_IS_READY` 表示 CPU 已完成初始化
4. **AP 启动顺序**：BSP 释放 BKL → AP 获取 BKL → AP 初始化 → AP 释放 BKL → BSP 重新获取

---

## 2. C 源码分析

### 2.1 相关定义

| 常量/宏 | 值 | 源码位置 | 说明 |
|---------|-----|---------|------|
| `CONFIG_MAX_CPUS` | 32 | config.h | 最大 CPU 数 |
| `CPU_IS_BSP` | 1 | smp.h:30 | BSP 标志 |
| `CPU_IS_READY` | 2 | smp.h:31 | CPU 就绪标志 |
| `SCHED_IPI_STOP_PROC` | 1 | smp.c:21 | IPI 停止进程 |
| `SCHED_IPI_VM_INHIBIT` | 2 | smp.c:22 | IPI VM 抑制 |
| `SCHED_IPI_SAVE_CTX` | 4 | smp.c:23 | IPI 保存上下文 |

### 2.2 核心数据结构

**struct cpu**（smp.h）：
```c
struct cpu {
    u32_t flags;  // CPU_IS_BSP | CPU_IS_READY
};
```

**__cpu_local_vars**（cpulocals.h）：
```c
struct __cpu_local_vars {
    struct proc *proc_ptr;              // 当前运行进程
    struct proc *bill_ptr;              // 计费进程
    struct proc idle_proc;              // idle 进程存根
    int pagefault_handled;              // 缺页处理标志
    struct proc *ptproc;                // 当前页表进程
    struct proc *run_q_head[NR_SCHED_QUEUES]; // 就绪队列头
    struct proc *run_q_tail[NR_SCHED_QUEUES]; // 就绪队列尾
    int cpu_is_idle;                    // CPU 是否空闲
    int idle_interrupted;               // idle 中断标志
    u64_t tsc_ctr_switch;              // 上下文切换时间戳
    u64_t cpu_last_tsc;                // 上次 TSC 读取
    u64_t cpu_last_idle;               // 上次空闲时间
    char fpu_presence;                  // FPU 存在标志
    struct proc *fpu_owner;            // FPU 所有者
};
```

**sched_ipi_data**（smp.c）：
```c
struct sched_ipi_data {
    volatile u32_t flags;  // SCHED_IPI_* 位组合
    volatile u32_t data;   // 目标进程指针
};
```

### 2.3 关键函数分析

#### `smp_schedule()` — smp.c:75

异步发送调度 IPI 到目标 CPU。仅通知，不等待完成。

#### `smp_schedule_sync()` — smp.c:84-111

同步跨 CPU 操作：
1. 检查目标 CPU 是否有未完成的 IPI 请求，如有则等待
2. 设置 IPI 数据和标志
3. 发送 IPI
4. 释放 BKL，等待目标 CPU 完成处理
5. 重新获取 BKL

#### `smp_sched_handler()` — smp.c:143-170

IPI 调度处理函数，在目标 CPU 上执行：
1. 读取 IPI 标志
2. 根据 `SCHED_IPI_STOP_PROC` 设置 `RTS_PROC_STOP`
3. 根据 `SCHED_IPI_SAVE_CTX` 保存 FPU 状态
4. 根据 `SCHED_IPI_VM_INHIBIT` 设置 `RTS_VMINHIBIT`
5. 清除 IPI 标志

#### `smp_ipi_sched_handler()` — smp.c:174-183

IPI 确认处理：
1. 确认 IPI
2. 如果当前进程不是 IDLE，设置 `RTS_PREEMPTED`

#### `wait_for_APs_to_finish_booting()` — smp.c:33-51

BSP 等待所有 AP 完成启动：
1. 释放 BKL
2. 等待 `ap_cpus_booted == ncpus - 1`
3. 重新获取 BKL

### 2.4 调用关系

```
BSP boot:
  BKL_LOCK()
  → smp_init() → 启动 APs
  → wait_for_APs_to_finish_booting()
    → BKL_UNLOCK()
    → 等待 ap_cpus_booted
    → BKL_LOCK()

AP boot:
  → ap_boot_finished(cpu)
  → BKL_LOCK()
  → 初始化 per-CPU 数据
  → switch_to_user()

运行时跨 CPU 操作:
  smp_schedule_stop_proc(p)
    → if runnable: smp_schedule_sync(p, SCHED_IPI_STOP_PROC)
    → else: RTS_SET(p, RTS_PROC_STOP)

  smp_schedule_vminhibit(p)
    → if runnable: smp_schedule_sync(p, SCHED_IPI_VM_INHIBIT)
    → else: RTS_SET(p, RTS_VMINHIBIT)

  smp_schedule_migrate_proc(p, dest_cpu)
    → smp_schedule_sync(p, STOP_PROC | SAVE_CTX)
    → p->p_cpu = dest_cpu
    → RTS_UNSET(p, RTS_PROC_STOP)
```

### 2.5 设计要点/特殊处理

1. **BKL 释放窗口**：`smp_schedule_sync()` 在等待目标 CPU 时释放 BKL，其他 CPU 可以进入内核
2. **递归 IPI 处理**：等待目标 CPU 时，如果本 CPU 也收到 IPI，先处理自己的 IPI 再继续等待
3. **单 CPU 退化**：`CONFIG_SMP` 未定义时，`cpuid=0`，所有 per-CPU 变量退化为全局变量

---

## 3. Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | BKL 表达 | `static mut` + spinlock | **`Spinlock<()>`** ([2026-06-13] 框架已实现) | 类型安全，避免裸 `static mut`。框架见 [`smp.rs::bkl_lock` / `bkl_unlock` / `BklGuard`]——`AtomicBool` + CAS 自旋，no_std 友好不依赖第三方 crate。但**尚未在所有跨 CPU 访问点全面接入**（`P0-01`/`P0-17` 仍待 `proc_table.rs`/`ipc.rs`/`sched.rs` 改用 `bkl_lock()` 包裹）。 |
| D2 | per-CPU 数据 | `Vec<CpuLocal>` vs `[CpuLocal; MAX_CPUS]` | **`[CpuLocal; MAX_CPUS]`** | 固定大小，no_std 兼容 |
| D3 | CPU 状态 | `u32_t flags` | **bitflags `CpuFlags`** | 类型安全 |
| D4 | IPI 数据 | `volatile u32` | **`AtomicU32`** | Rust 标准原子操作 |
| D5 | IPI 标志 | 裸位操作 | **bitflags `SchedIpiFlags`** | 类型安全 |
| D6 | smp_schedule_sync | 保留 BKL 释放/重获模式 | **保留**（BklGuard 非 RAII） | C 行为对齐，但标注安全约束。`BklGuard` 故意不实现 `Drop` 释放——避免隐藏 sendrecv 等阻塞点必须显式 `bkl_unlock()` 再 `bkl_lock()` 的模式 |
| D7 | 单 CPU 退化 | 编译时 vs 运行时 | **运行时 CAS（无 cfg gate）** | 零虚拟开销——单 CPU 时 CAS 一次成功，不进入自旋；保持代码路径单一 |

---

## 4. 实现详解

> **ARCHITECTURE NOTE — BKL 框架覆盖范围 (2026-06-15)**:
> `smp.rs` 提供了 `bkl_lock()` / `bkl_unlock()` / `BklGuard` 三件套（D1 落地）。各模块的 BKL 文档覆盖现状如下（尚未实际包裹 `bkl_lock()`，属于 `P0-01` 范围）：
>
> | 共享数据 | 访问点 | BKL 文档状态 |
> |---------|--------|------------|
> | `ProcessTable::procs[]` | `proc_table.rs` | 模块级文档声明 BKL 要求；方法注释 `// caller must hold BKL` |
> | `KProcess::p_nextready` / `p_caller_q` / `p_q_link` | `ipc.rs:240,408-410,632` | 注释声明 BKL 保护 |
> | `Scheduler` 队列操作 | `sched.rs` | 缺少 BKL 文档（`P0-05` per-CPU 运行队列未实现，待一并补充） |
> | `IrqManager::hooks[]` / `actids[]` | `irq_manager.rs:14` | 模块级文档声明 single-threaded under BKL |
> | `ClockState` | `clock.rs:317-318` | 注释声明 BKL 保护 |
>
> 上述每一处都需要在 `&mut self` 方法入口加 `let _g = bkl_lock();` 并在出口 `bkl_unlock();` 显式释放（受 D6 约束：`BklGuard` 故意非 RAII）。预计 `P0-01` 修复时一次性接入。

### 4.1 CpuLocal — per-CPU 数据

> 设计决策 D2：固定大小数组，no_std 兼容。

```rust
/// Per-CPU local data, equivalent to C's `__cpu_local_vars`.
///
/// C: cpulocals.h — `struct __cpu_local_vars`
pub struct CpuLocal {
    /// Currently running process. C: `proc_ptr`
    pub proc_ptr: Option<ProcNr>,
    /// Billable process for time accounting. C: `bill_ptr`
    pub bill_ptr: Option<ProcNr>,
    /// Slot index of the idle kernel task for this CPU. C: `idle_proc`
    pub idle_proc: ProcNr,
    /// Process that currently owns this CPU's page tables. C: `ptproc`
    pub ptproc: Option<ProcNr>,
    /// Whether this CPU is idle. C: `cpu_is_idle`
    pub cpu_is_idle: bool,
    /// Whether the idle loop was interrupted and needs wakeup. C: `idle_interrupted`
    pub idle_interrupted: bool,
    /// TSC at last context switch. C: `tsc_ctr_switch`
    pub tsc_ctr_switch: u64,
    /// Last raw TSC reading on this CPU. C: `cpu_last_tsc`
    pub cpu_last_tsc: u64,
    /// Last time this CPU went idle (TSC). C: `cpu_last_idle`
    pub cpu_last_idle: u64,
    /// Whether a pagefault is already being handled. C: `pagefault_handled`
    pub pagefault_handled: bool,
    /// Whether this CPU has an FPU. C: `fpu_presence`
    pub fpu_presence: bool,
    /// FPU owner process. C: `fpu_owner`
    pub fpu_owner: Option<ProcNr>,
    /// Per-CPU scheduler (ready queues). C: `run_q_head[]` / `run_q_tail[]`
    /// in cpulocals.h:58-59.
    pub scheduler: Scheduler,
}
```

#### 关于调度队列的放置

C 源码把 `run_q_head[NR_SCHED_QUEUES]`/`run_q_tail[NR_SCHED_QUEUES]` 放进 `__cpu_local_vars`（cpulocals.h:58-59），实现真正的 per-CPU 就绪队列。

**当前状态（P0-05 渐进式修复，2026-06-16）**：`CpuLocal` 已新增 `scheduler: Scheduler` 字段，与 C cpulocals.h:58-59 对齐。`ProcessTable` 新增 `sched_for_cpu(cpu_id)`/`sched_for_cpu_mut(cpu_id)` 方法，`sched_enqueue_head`/`sched_dequeue` 新增 `cpu_id` 参数（C 使用 `get_cpu_var(rp->p_cpu, run_q_head)`），`select_next_process` 中 `pick_proc` 改用 `sched_for_cpu`。

**单 CPU 配置**：`sched_for_cpu(0)` 返回 `&self.sched`（BSP scheduler），`CpuLocal::scheduler` 已初始化但未直接使用。这是因为 `dequeue_from_queue` 需要同时持有 `&mut Scheduler` 和 `&mut [KProcess]`，在 `ProcessTable` 方法中通过 `sched_for_cpu_mut` 借用 `&mut self` 后无法再借用 `&mut self.procs`。

**SMP 迁移路径**：当 `CONFIG_SMP=ncpus > 1` 落地时，`ProcessTable::sched` 将被移除，所有调度通过 `SmpState.cpu_locals[cpu].scheduler` 分发。此时 `Scheduler` 和 `procs` 分属不同 owner（`SmpState` vs `ProcessTable`），借用冲突自然解决。

### 4.2 SmpState — 全局 SMP 状态

> 设计决策 D1/D2/D3：封装为 struct，BKL 保护。

```rust
/// Global SMP state, equivalent to C's `ncpus`, `bsp_cpu_id`, `cpus[]`.
pub struct SmpState {
    /// Number of CPUs. C: `ncpus`
    ncpus: u32,
    /// BSP CPU ID. C: `bsp_cpu_id`
    bsp_cpu_id: u32,
    /// Per-CPU state array. C: `struct cpu cpus[CONFIG_MAX_CPUS]`
    cpus: [CpuState; MAX_CPUS],
    /// Per-CPU local data. C: `__cpu_local_vars CPULOCAL_ARRAY`
    cpu_locals: [CpuLocal; MAX_CPUS],
    /// IPI scheduling data. C: `sched_ipi_data[CONFIG_MAX_CPUS]`
    sched_ipi_data: [SchedIpiData; MAX_CPUS],
    /// Number of APs that have finished booting. C: `ap_cpus_booted`
    ap_cpus_booted: AtomicU32,
}
```

### 4.3 SchedIpiFlags — IPI 标志

> 设计决策 D5：bitflags 替代裸位操作。

```rust
bitflags::bitflags! {
    pub struct SchedIpiFlags: u32 {
        const STOP_PROC = 1;
        const VM_INHIBIT = 2;
        const SAVE_CTX = 4;
    }
}
```

---

## 5. 测试要点

1. **SmpState 初始化**：ncpus=1（单核默认），bsp_cpu_id=0
2. **cpu_is_bsp**：BSP 返回 true，其他返回 false
3. **CpuLocal 默认值**：proc_ptr=None, cpu_is_idle=false
4. **SchedIpiFlags 位操作**：设置/清除/检查
5. **AP 启动计数**：ap_boot_finished 递增计数器

---

## 6. 补充：SMP 详细分析

> 来源：tmp-18-smp.md

### 6.1 大内核锁（BKL）

全局自旋锁 `big_kernel_lock`，确保同一时刻只有一个 CPU 在内核态执行。BSP 在 `kmain()` 中获取 BKL，AP 在进入内核时必须先获取 BKL。BKL 的持有时间从内核入口到 `switch_to_user()` 退出，覆盖了整个内核执行路径。

### 6.2 CPU 局部变量

`__cpu_local_vars` 结构体数组，每个 CPU 一个实例。SMP 下通过 `get_cpulocal_var(name)` 宏（展开为 `__cpu_local_vars[cpuid].name`）访问当前 CPU 的变量。单 CPU 编译时退化为直接结构体访问。

### 6.3 sched_ipi_data

每个 CPU 一个的调度 IPI 数据结构，包含 `flags`（请求类型）和 `data`（目标进程指针）。当一个 CPU 需要另一个 CPU 执行调度操作时，设置目标 CPU 的 `sched_ipi_data` 并发送 IPI。

### 6.4 AP 启动流程

BSP 通过 ACPI/MADT 表发现 AP，向 AP 发送 INIT + SIPI（Startup IPI）中断唤醒 AP。AP 从实模式启动，切换到保护模式后跳转到内核入口，完成初始化后调用 `ap_boot_finished()` 通知 BSP。

### 6.5 SMP 行为规则

1. **BKL 串行化**：同一时刻只有一个 CPU 执行内核代码，其他 CPU 自旋等待
2. **CPU 局部队列**：每个 CPU 有独立的调度队列，入队/出队操作无需跨 CPU 同步
3. **IPI 同步请求**：`smp_schedule_sync()` 发送 IPI 后等待目标 CPU 完成操作，期间释放 BKL 允许目标 CPU 执行
4. **进程 CPU 亲和性**：进程通过 `p_cpu` 绑定到特定 CPU，入队时加入对应 CPU 的队列
5. **FPU 所有权**：每个 CPU 的 FPU 由一个进程独占，迁移进程时需保存 FPU 状态
6. **AP 启动超时容忍**：若部分 AP 未成功启动，系统仍可运行（打印警告）

### 6.6 调度 IPI 标志

| 标志 | 值 | 含义 |
|------|-----|------|
| `SCHED_IPI_STOP_PROC` | 1 | 停止进程（设置 RTS_PROC_STOP） |
| `SCHED_IPI_VM_INHIBIT` | 2 | 设置 VMINHIBIT |
| `SCHED_IPI_SAVE_CTX` | 4 | 保存完整上下文（含 FPU） |

### 6.7 SMP 函数列表

| 功能 | 函数 | 源文件 |
|------|------|--------|
| SMP 初始化 | `smp_init()` | smp.c |
| AP 启动等待 | `wait_for_APs_to_finish_booting()` | smp.c:30 |
| 调度 IPI | `smp_schedule()` | smp.c:63 |
| 同步调度请求 | `smp_schedule_sync()` | smp.c:75 |
| 停止进程 | `smp_schedule_stop_proc()` | smp.c:114 |
| VMINHIBIT 请求 | `smp_schedule_vminhibit()` | smp.c:123 |
| 迁移进程 | `smp_schedule_migrate_proc()` | smp.c:142 |
| IPI 调度处理 | `smp_sched_handler()` | smp.c:156 |
| IPI 确认处理 | `smp_ipi_sched_handler()` | smp.c:194 |

---

## 7. 参见

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — per-CPU 调度队列
- [15-clock-timer.md](15-clock-timer.md) — BSP/AP 时钟中断差异
- [22-privilege.md](22-privilege.md) — VMINHIBIT 与权限
