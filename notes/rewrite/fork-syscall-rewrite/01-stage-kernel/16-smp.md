# 16-smp: 多核协同

> **分类**: 全局基建
> **C 源码**: `minix3/minix/kernel/smp.c` (204 行), `minix3/minix/kernel/smp.h`, `minix3/minix/kernel/cpulocals.h`, `minix3/minix/kernel/proc.h`
> **Rust 实现**: `os/kernel/src/smp.rs` (1439 行), `os/kernel/src/proc_table.rs` (SMP 相关), `os/kernel/src/sched.rs`
> **覆盖**: BKL (Big Kernel Lock)、per-CPU 数据、IPI 跨 CPU 调度、CPU 亲和性、AP 启动握手、跨架构硬件抽象
> **前置**: [11-scheduling-primitives.md](11-scheduling-primitives.md), [14-exception-interrupt.md](14-exception-interrupt.md), [15-clock-timer.md](15-clock-timer.md)

---

## 1. 概念建构

**核心问题**: 多个 CPU 共享同一内核代码与数据时，如何在"并发执行"与"保持简单"之间取舍？

**Minix3 的回答**: 用串行化换简单性——BKL 保证同一时刻只有一个 CPU 在内核态，其余 CPU 自旋等待。这是粗粒度策略，牺牲并行性换取避免细粒度锁的死锁/序/复杂性三大难题。

### 1.1 BKL：用串行化换简单性

**灵魂本质**: BKL 是粗粒度 SMP 策略——同一时刻只有一个 CPU 在内核态，用互斥避免细粒度锁的死锁难题。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 多核系统中，内核数据结构（进程表、调度队列、IPC 链表）若被多 CPU 并发修改，需要细粒度锁保护每个数据结构。细粒度锁带来三大难题：
  1. **死锁**——锁序约束随数据结构增多呈组合爆炸
  2. **活锁**——优先级反转导致低优先级持锁者被反复抢占
  3. **复杂性**——每条路径都要分析锁交互，code review 成本高
- **WHAT**: BKL 用一个全局自旋锁串行化所有内核代码路径。持有时禁止睡眠/调度/等待 IPC/等待锁；其他 CPU 在内核入口自旋等待。
- **HOW**: C 用 `SPINLOCK_DEFINE(big_kernel_lock)` ([smp.c:27](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)) + `BKL_LOCK()/BKL_UNLOCK()` 宏。BSP 在 `kmain()` 获取 BKL，AP 在内核入口获取。BKL 持有期覆盖内核入口到 `switch_to_user()` 退出。

**关键约束**:

1. 临界区禁止睡眠（spinlock 持有者睡眠会导致其他 CPU 死锁）
2. 阻塞操作前必须释放 BKL——`smp_schedule_sync` ([smp.c:86,103](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)) / `wait_for_APs` ([smp.c:44](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)) 在 wait 前 `BKL_UNLOCK()`
3. BKL 非递归——同 CPU 二次获取死锁；[smp.c:80](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) `assert(cpu != mycpu)` 间接体现

**单 CPU 退化**: `CONFIG_SMP` 未定义时 BKL 退化为 compiler fence（无竞争时 spinlock 无需自旋）。

### 1.2 per-CPU 数据：CPU 私有状态为何免锁

**灵魂本质**: 每个 CPU 拥有私有数据副本，访问无需同步——这是 cache 局部性与免锁的联合产物。

**核心字段**（C: [cpulocals.h:40-73](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/cpulocals.h) `struct __cpu_local_vars`）:

| 字段 | C 位置 | 语义 |
|------|--------|------|
| `proc_ptr` | cpulocals.h:40 | 当前运行进程（调度器快速访问） |
| `bill_ptr` | cpulocals.h:41 | 计费进程（时钟中断记账，可能是被抢占的系统进程） |
| `idle_proc` | cpulocals.h:42 | idle 进程存根（每 CPU 一个，无 runnable 进程时切入） |
| `pagefault_handled` | cpulocals.h:48 | 递归缺页检测（缺页处理中再次缺页会死锁） |
| `ptproc` | cpulocals.h:55 | 当前页表进程（共享页表的进程无法用 proc_ptr 判断 CR3 是否需重载） |
| `run_q_head/tail` | cpulocals.h:58-59 | per-CPU 就绪队列（入队/出队免锁，仅跨 CPU 迁移需 IPI 同步） |
| `cpu_is_idle` | cpulocals.h:60 | CPU 是否空闲 |
| `idle_interrupted` | cpulocals.h:62 | idle 被中断标志 |
| `tsc_ctr_switch` | cpulocals.h:65 | 上下文切换时间戳 |
| `cpu_last_tsc` | cpulocals.h:68 | 上次 TSC 读取 |
| `cpu_last_idle` | cpulocals.h:69 | 上次空闲时间 |
| `fpu_presence` | cpulocals.h:72 | FPU 是否存在 |
| `fpu_owner` | cpulocals.h:73 | FPU 当前所有者 |

**访问方式**:
- SMP: `get_cpulocal_var(name)` → `__cpu_local_vars[cpuid].name`
- 单 CPU: `__cpu_local_vars.name`（无数组下标）
- 跨 CPU 访问: `get_cpu_var(cpu, name)` → `__cpu_local_vars[cpu].name`

**cache 对齐**: C 源码 cpulocals.h:18 FIXME 注明"应 padding 防止 false sharing"，但未实现。Rust 版本同样未做 cache 行对齐（对齐 C 现状）。

### 1.3 IPI：CPU 间如何对话

**灵魂本质**: IPI 是 CPU 间的软中断通知，分为异步通知（仅发不等待）与同步请求（等待目标 CPU 完成）。

**两类 IPI**:

1. **异步 IPI** (`smp_schedule`, [smp.c:63-66](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)): 仅调用 `arch_send_smp_schedule_ipi(cpu)` 通知目标 CPU，不等待。用于抢占等无返回值场景。
2. **同步 IPI** (`smp_schedule_sync`, [smp.c:75-112](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)): 设置 flags+data → 发 IPI → 释放 BKL → 等待 flags 清零 → 重获 BKL。用于跨 CPU 调度操作（停止/抑制/迁移进程）。

**同步 IPI 的重入处理** ([smp.c:88-93,105-109](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)): 等待目标 CPU 时，若本 CPU 也收到 IPI（`sched_ipi_data[mycpu].flags` 非零），先 `BKL_LOCK()` 处理自己的 IPI（调用 `smp_sched_handler()`）再继续等待。这避免了 IPI 响应饥饿。

**IPI 标志** ([smp.c:21-23](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)):

| 标志 | 值 | 含义 |
|------|----|------|
| `SCHED_IPI_STOP_PROC` | 1 | 停止目标进程（设置 `RTS_PROC_STOP`） |
| `SCHED_IPI_VM_INHIBIT` | 2 | 设置 `RTS_VMINHIBIT`（地址空间正在变更） |
| `SCHED_IPI_SAVE_CTX` | 4 | 保存完整上下文（含 FPU 状态，用于迁移前） |

### 1.4 CPU 亲和性：进程为何绑定 CPU

**灵魂本质**: 进程绑定到特定 CPU 可减少迁移成本与 cache 失效——迁移需 IPI 同步保存完整上下文。

**亲和性来源**:
- 进程通过 `p_cpu` 字段绑定到 CPU
- fork 时继承父进程的 `p_cpu`
- `smp_schedule_migrate_proc` ([smp.c:142-154](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)) 显式迁移

**迁移成本**:

1. 同步 IPI 通知源 CPU 停止进程（`STOP_PROC | SAVE_CTX`）
2. 源 CPU 保存 FPU 状态（`SAVE_CTX` 分支，[smp.c:170-178](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)）
3. 修改 `p_cpu` 字段
4. 解除 `RTS_PROC_STOP` 让进程在新 CPU 上运行

**为什么不无脑迁移**: 迁移会导致 cache 冷启动（新 CPU 上无该进程的数据缓存）；频繁迁移抵消 per-CPU 队列的免锁优势。

### 1.5 AP 启动握手：BSP 如何唤醒并等待 APs

**灵魂本质**: BSP 通过 INIT+SIPI 唤醒 AP，AP 完成初始化后递增计数器，BSP 释放 BKL 等待握手完成。

**启动时序** ([smp.c:30-49](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)):

1. BSP 在 `kmain()` 持有 BKL，调用 `smp_init()` 发现并唤醒 APs
2. BSP 调用 `wait_for_APs_to_finish_booting()`:
   - 统计 `CPU_IS_READY` 的 CPU 数（容忍部分 AP 启动失败，smp.c:36-41）
   - `BKL_UNLOCK()` 让 AP 能进入内核 (smp.c:44)
   - 自旋等待 `ap_cpus_booted == n - 1` (smp.c:45-46)
   - `BKL_LOCK()` 重新获取 (smp.c:48)
3. AP 启动完成后调用 `ap_boot_finished(cpu)` (smp.c:51-54) 递增 `ap_cpus_booted`

**AP 启动协议** (arch-specific, x86-only):
- BSP 发 INIT IPI → 等待 → 发 SIPI（起始向量）
- AP 从实模式启动，切换到保护模式，跳转到内核入口
- aarch64/riscv64 用 PSCI/SBI（详见 §1.6 与 arch 层文档）

### 1.6 硬件抽象：跨架构统一 IPI 接口

**灵魂本质**: 不同架构的 IPI 发送/确认/停机指令不同，必须抽象为 trait 以保持内核代码架构无关。

**跨架构差异表**:

| CPU 问题 | x86_64 | aarch64 | riscv64 | Rust 抽象 |
|---------|--------|---------|---------|-----------|
| 发送 IPI | APIC ICR 写 | GIC GICD_SGIR | PLIC/SBI | `SmpArch::send_sched_ipi(cpu)` |
| 停机 CPU | `hlt` | `wfi` | `wfi` | `SmpArch::halt_cpu()` |
| IPI 确认 | APIC EOI | GIC EOIR | PLIC claim | `SmpArch::ack_ipi()` |
| 忙等提示 | `pause` | `yield` | `pause` | `SmpArch::pause()` (统一 `core::hint::spin_loop`) |
| AP 启动 | INIT+SIPI | PSCI CPU_ON | SBI HSM | `SmpArch::boot_ap(cpu, entry)` |
| 当前 CPU ID | GS base | TPIDR_EL1 | sscratch | `SmpArch::current_cpu()` |

**设计原则**: 内核代码（smp.rs）只依赖 trait 方法，不出现 `#[cfg(target_arch)]` 行为选择。各架构在 arch 层提供 trait 实现。

**与 redox OS 对比**: redox-relibc 内核不用 BKL，而是用 per-CPU 调度器 + `RwLock` 保护上下文切换 + 原子操作保护共享队列。redox 的设计更细粒度但复杂度更高。minix-rs 选择保留 BKL 以对齐 Minix3 的外部行为（串行化语义、IPC 协议、调度时机），这是 Rewrite 级约束——同一外部行为用 Rust 类型系统重新表达，而非引入新的同步策略。

---

## 2. C 源码分析

### 2.1 全局状态与常量

| 符号 | 位置 | 说明 |
|------|------|------|
| `ncpus` | [smp.c:7](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | CPU 总数（运行时检测） |
| `ht_per_core` | [smp.c:8](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | 每物理核的超线程数 |
| `bsp_cpu_id` | [smp.c:9](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | BSP CPU 编号 |
| `struct cpu cpus[CONFIG_MAX_CPUS]` | [smp.c:11](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | CPU 状态数组 |
| `CONFIG_MAX_CPUS` | config.h | 最大 CPU 数上限（32） |
| `CPU_IS_BSP` | [smp.h:32](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.h) | BSP 标志位（值=1） |
| `CPU_IS_READY` | [smp.h:33](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.h) | CPU 就绪标志位（值=2） |
| `cpu_is_bsp(cpu)` | [smp.h:19](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.h) | `(bsp_cpu_id == cpu)` 宏 |

### 2.2 BKL 定义

| 符号 | 位置 | 说明 |
|------|------|------|
| `SPINLOCK_DEFINE(big_kernel_lock)` | [smp.c:27](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | 全局内核自旋锁 |
| `SPINLOCK_DEFINE(boot_lock)` | [smp.c:28](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | AP 启动同步锁 |
| `BKL_LOCK()/BKL_UNLOCK()` | spinlock.h | 获取/释放 BKL 宏 |
| `ap_cpus_booted` | [smp.c:25](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | 已启动 AP 计数（volatile） |

### 2.3 per-CPU 数据结构

`struct __cpu_local_vars` ([cpulocals.h:37-75](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/cpulocals.h)) — 详见 §1.2 字段表。

访问宏:
- `get_cpulocal_var(name)` → `__cpu_local_vars[cpuid].name` (SMP) / `__cpu_local_vars.name` (单 CPU)
- `get_cpu_var(cpu, name)` → `__cpu_local_vars[cpu].name` (跨 CPU 访问)

### 2.4 IPI 调度数据与标志

```c
struct sched_ipi_data {
    volatile u32_t flags;  // SCHED_IPI_* 位组合
    volatile u32_t data;   // 目标进程指针（cast 自 struct proc*）
};
static struct sched_ipi_data sched_ipi_data[CONFIG_MAX_CPUS];  // smp.c:19
```

标志常量 ([smp.c:21-23](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c)):
- `SCHED_IPI_STOP_PROC` = 1
- `SCHED_IPI_VM_INHIBIT` = 2
- `SCHED_IPI_SAVE_CTX` = 4

### 2.5 核心函数 file:line 索引

| 函数 | 位置 | 语义 |
|------|------|------|
| `wait_for_APs_to_finish_booting()` | [smp.c:30-49](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | BSP 释放 BKL 等待 APs，容忍部分失败 |
| `ap_boot_finished(cpu)` | [smp.c:51-54](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | AP 递增 `ap_cpus_booted` |
| `smp_ipi_halt_handler()` | [smp.c:56-61](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | IPI 停机：ack + 停定时器 + arch halt |
| `smp_schedule(cpu)` | [smp.c:63-66](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | 异步 IPI：仅发不等待 |
| `smp_schedule_sync(p, task)` | [smp.c:75-112](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | 同步 IPI：设数据→发 IPI→释放 BKL→等待→重获 BKL；含重入处理 |
| `smp_schedule_stop_proc(p)` | [smp.c:114-121](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | if runnable: sync(STOP_PROC); else: RTS_SET(PROC_STOP) |
| `smp_schedule_vminhibit(p)` | [smp.c:123-130](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | if runnable: sync(VM_INHIBIT); else: RTS_SET(VMINHIBIT) |
| `smp_schedule_stop_proc_save_ctx(p)` | [smp.c:132-140](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | sync(STOP_PROC \| SAVE_CTX) — 迁移前保存 FPU |
| `smp_schedule_migrate_proc(p, dest_cpu)` | [smp.c:142-154](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | sync(STOP \| SAVE_CTX) → 改 p_cpu → RTS_UNSET(PROC_STOP) |
| `smp_sched_handler()` | [smp.c:156-187](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | IPI 处理：读 flags→STOP_PROC 设 RTS→SAVE_CTX 保存 FPU→VM_INHIBIT 设 RTS→清 flags |
| `smp_ipi_sched_handler()` | [smp.c:194-204](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) | IPI ack + 若当前非 IDLE 设 RTS_PREEMPTED |

### 2.6 调用关系图

**BSP 启动时序**:

```
kmain() [main.c]
  ├─ BKL_LOCK()
  ├─ smp_init()  → 发现 APs, 发 INIT+SIPI
  └─ wait_for_APs_to_finish_booting() [smp.c:30]
       ├─ 统计 ready CPUs
       ├─ BKL_UNLOCK()  [smp.c:44]
       ├─ while (ap_cpus_booted != n-1) arch_pause()
       └─ BKL_LOCK()  [smp.c:48]
```

**AP 启动时序**:

```
AP 实模式入口
  └─ 切换保护模式 → 跳转内核入口
       └─ ap_boot_finished(cpu)  [smp.c:51]
            └─ ap_cpus_booted++
       └─ BKL_LOCK()
       └─ 初始化 per-CPU 数据
       └─ switch_to_user()  → BKL_UNLOCK()
```

**运行时跨 CPU 操作时序**（以 stop_proc 为例）:

```
CPU A: smp_schedule_stop_proc(p)  [smp.c:114]
  ├─ if proc_is_runnable(p):
  │   └─ smp_schedule_sync(p, STOP_PROC)  [smp.c:75]
  │        ├─ 等待 sched_ipi_data[cpu].flags == 0  (可能重入处理)
  │        ├─ sched_ipi_data[cpu].data = p
  │        ├─ sched_ipi_data[cpu].flags |= STOP_PROC
  │        ├─ arch_send_smp_schedule_ipi(cpu)
  │        ├─ BKL_UNLOCK()
  │        ├─ while (flags != 0) { if (my flags) { BKL_LOCK(); smp_sched_handler(); BKL_UNLOCK(); } }
  │        └─ BKL_LOCK()
  └─ else:
      └─ RTS_SET(p, RTS_PROC_STOP)

CPU B (target): 收到 IPI
  └─ smp_ipi_sched_handler()  [smp.c:194]
       ├─ ipi_ack()
       └─ if (curr != IDLE): RTS_SET(curr, RTS_PREEMPTED)
  └─ smp_sched_handler()  [smp.c:156]
       ├─ p = sched_ipi_data[cpu].data
       ├─ if (flags & STOP_PROC): RTS_SET(p, RTS_PROC_STOP)
       ├─ if (flags & SAVE_CTX): 保存 FPU
       ├─ if (flags & VM_INHIBIT): RTS_SET(p, RTS_VMINHIBIT)
       └─ sched_ipi_data[cpu].flags = 0
```

### 2.7 关键约束汇总

1. **BKL 释放窗口**：`smp_schedule_sync()` 在等待目标 CPU 时释放 BKL，其他 CPU 可以进入内核
2. **递归 IPI 处理**：等待目标 CPU 时，如果本 CPU 也收到 IPI，先处理自己的 IPI 再继续等待
3. **单 CPU 退化**：`CONFIG_SMP` 未定义时，`cpuid=0`，所有 per-CPU 变量退化为全局变量
4. **AP 启动超时容忍**：若部分 AP 未成功启动，系统仍可运行（[smp.c:40-41](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/smp.c) 打印警告）

---

## 3. Rust 设计决策

> 本章采用 hypothesis-driven 格式："如果 X 设计会有 Y 问题所以用 Z"。

### D1. BKL 表达：AtomicBool+CAS vs RwLock vs Mutex

**假设性推理**:
- 如果用 `RwLock`：BKL 临界区禁止睡眠，但 `RwLock` 的 write 侧可能阻塞等待 reader 释放，等价于睡眠 → 违反 BKL 约束。
- 如果用 `Mutex`：`Mutex` 的 `lock()` 返回 `Result`，poisoning 语义与 spinlock 语义不符；且 `Mutex` 在 no_std 下需自旋或 futex，内核态无 futex。
- 所以用 `AtomicBool` + CAS 自旋（等价于 C 的 `SPINLOCK_DEFINE`）：无睡眠风险，no_std 友好，不依赖第三方 crate。

**实现**: [`static BKL_LOCKED: AtomicBool`](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs) + `compare_exchange(false, true, Acquire, Relaxed)` 自旋。

### D2. per-CPU 容器：固定数组 vs Vec

**假设性推理**:
- 如果用 `Vec<CpuLocal>`：`Vec` 需动态分配，no_std 下需 `alloc` crate；且 per-CPU 数据在内核启动早期就需访问（早于 allocator 初始化）。
- 如果用 `Box<[CpuLocal]>`：同上，依赖 allocator。
- 所以用 `[CpuLocal; MAX_CPUS]` 固定大小数组：编译期分配在 BSS，启动早期可访问，no_std 兼容。

**实现**: `cpu_locals: [CpuLocal; MAX_CPUS]` ([smp.rs:341](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs))。`MAX_CPUS = 32` ([smp.rs:61](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)) 对齐 `CONFIG_MAX_CPUS` (config.h)。

### D3. BKL 释放模式：统一 RAII guard（R-05，2026-08-12）

**演进说明**：原设计（D6）提供两种 guard——非 RAII `BklGuard`（用于阻塞路径）+ RAII `BklGuardRaii`（用于普通临界区）。R-05 将两者统一为单一 RAII 类型，消除"忘记调用 `bkl_unlock()`"类死锁。

**假设性推理**:
- 如果 `BklGuard` 不实现 `Drop`：调用方可能忘记 `bkl_unlock()`，导致 BKL 永久持有 → 死锁。原设计用 `BklGuardRaii` 缓解普通路径，但阻塞路径仍需手动管理。
- 如果 `BklGuard` 实现 `Drop` 自动释放：阻塞路径需在作用域中间释放（如 `smp_schedule_sync` 等待前），RAII 的 Drop 只在作用域结束触发。
- **R-05 解法**：`BklGuard` 实现 `Drop` 释放 BKL（RAII），同时提供：
  1. `BklGuard::release()` ——显式提前释放（消费 guard，Drop 不再触发），用于阻塞操作前释放。
  2. `core::mem::forget(guard)` ——跨函数 BKL 传递（如 `kernel_call_dispatch` → `kernel_call_finish`），BKL 保持持有，在目标函数显式 `bkl_unlock()`。
- 所以统一为单一 RAII 类型：普通临界区让 guard 自然 drop；阻塞路径用 `release()`；跨函数传递用 `mem::forget` + 显式 `bkl_unlock()`。

**实现**: [`BklGuard`](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs) ([smp.rs:798](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)) — RAII（`Drop` 调用 `bkl_unlock()`）+ [`BklGuard::release()`](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs) ([smp.rs:819](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)) 显式提前释放。原 `BklGuardRaii`/`bkl_lock_raii` 已移除（[smp.rs:991](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)）。

### D4. BKL 类型见证：BklSection witness（Rust 独有）

**假设性推理**:
- C 用注释约定"调用者必须持有 BKL"，但编译器不检查。
- 如果 Rust 也只用注释：同样无编译期保证，调用者可能忘记加锁。
- 如果用 `BklGuard` 的引用作为参数：guard 不可复制，且需在每个函数签名中传递，侵入性大。
- 所以用类型系统编码不变量：`BklSection<'a>` 是一个持有 BKL 的零成本 witness，通过 `smp_state_with()` 返回 `&SmpState`——只有持有见证才能访问 `SmpState` 的可变方法。这是 Capability pattern：类型见证作为能力令牌。

**实现**: [`BklSection<'a>`](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs) ([smp.rs:867](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)) + [`bkl_lock_section()`](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs) ([smp.rs:877](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)) + [`smp_state_with()`](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs) ([smp.rs:905](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs))。

> **`BklProtected` marker trait**（FIX-07: R-01 soundness 修复，2026-08-12）：`SyncUnsafeCell` 的 `unsafe impl Sync` 不再是无约束 blanket impl，而是 gated on sealed marker trait `BklProtected`（[`bkl_protected` 模块](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）。只有 9 个已审计类型可被 `SyncUnsafeCell` 包裹（`ProcessTable`/`PrivTable`/`IrqManager`/`SmpState`/`FreePdeSlots`/`IpcFilterPool`/`KRandomness` + `KernelInfo`/`MemMapEntry`），从根上消除 `RefCell<T>`/`Rc<T>` 等 `!Sync` 类型被误装进 `static` 的 soundness 漏洞。`BklProtected` 与 `BklSection` witness 互补：前者控制"哪些类型可以装进 `static`"，后者控制"哪些调用可以访问 `static` 内部"。详见 [06-proc-init-boot-proc.md §4.3](./06-proc-init-boot-proc.md)。

### D5. IPI 标志：bitflags vs 裸位

**假设性推理**:
- 如果用 `u32` 裸位操作：`flags |= 1` / `flags & 2` 等魔法数字，类型不安全，易写错位。
- 如果用 `enum`：IPI 标志是位组合（`STOP_PROC | SAVE_CTX`），enum 无法表达位组合语义。
- 所以用 `bitflags!` 宏：类型安全，支持位组合（`|`/`&`/`contains`），且 `from_bits_truncate` 容忍未知位。

**实现**: [`SchedIpiFlags`](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs) ([smp.rs:78-91](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs))。

### D6. IPI 数据原子性：AtomicU32 vs volatile

**假设性推理**:
- 如果用 `volatile u32`（C 方式）：Rust 无 `volatile` 关键字，需用 `UnsafeCell` + `volatile_read/write`，unsafe 块增多。
- 如果用 `AtomicU32`：标准库提供 Acquire/Release 语义，安全且明确内存序；跨 CPU 通信天然需要原子操作。
- 所以用 `AtomicU32`：`flags: AtomicU32` + `load(Acquire)` / `store(Release)` / `fetch_or(AcqRel)`。

**实现**: [`SchedIpiData { flags: AtomicU32, target_proc: AtomicU32 }`](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs) ([smp.rs:308](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs))。

### D7. arch 抽象：SmpArch trait

**假设性推理**:
- 如果用 `#[cfg(target_arch)]` 行为选择：内核代码出现架构分支，违反"硬件抽象为 trait"原则；每加一个架构需修改 smp.rs。
- 如果用函数指针表：C 风格，类型不安全，且无法利用 Rust 的 trait dispatch 优化。
- 所以定义 `trait SmpArch`：各架构在 arch 层提供实现，内核代码依赖 trait；通过泛型 `<A: SmpArch>` 静态分发，零虚拟开销。

**实现**: [`SmpArch`](file:///home/xzhao/github/minix-rs/os/arch/src/arch/smp.rs) trait 定义在 arch crate（[arch/smp.rs](file:///home/xzhao/github/minix-rs/os/arch/src/arch/smp.rs)），内核通过 `pub use minix_arch::SmpArch;`（[smp.rs:114](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)）re-export。四个实现已全部落地：
- `X86_64SmpArch`（[x86_64/smp.rs](file:///home/xzhao/github/minix-rs/os/arch/src/x86_64/smp.rs)）— LAPIC ICR 发 IPI + EOI 确认 + INIT/SIPI 启动 AP
- `AArch64SmpArch`（[arm64/smp.rs](file:///home/xzhao/github/minix-rs/os/arch/src/arm64/smp.rs)）— GIC SGIR 发 IPI + EOIR 确认 + PSCI `CPU_ON` 启动 AP
- `Riscv64SmpArch`（[riscv64/smp.rs](file:///home/xzhao/github/minix-rs/os/arch/src/riscv64/smp.rs)）— SBI `send_ipi` + `hart_start` 启动 AP
- `MockSmpArch`（[arch/smp.rs](file:///home/xzhao/github/minix-rs/os/arch/src/arch/smp.rs)）— 测试用 no-op 实现

编译期别名 `minix_arch::CurrentSmpArch`（[lib.rs:298-311](file:///home/xzhao/github/minix-rs/os/arch/src/lib.rs)）按 target_arch 选择后端，内核代码零 `#[cfg(target_arch)]`。trait 作为泛型约束 `<A: SmpArch>` 在 `schedule_sync`/`ipi_sched_handler`/`wait_for_aps`/`ipi_halt_handler` 等函数中使用（见 §4.7-4.12）。

### D8. per-CPU 索引：ProcNr vs 裸指针

**假设性推理**:
- 如果用 `*mut KProcess` 裸指针（C 方式）：不安全，可能悬垂；无法做借用检查。
- 如果用 `Rc<RefCell<KProcess>>`：跨 CPU 共享 `Rc`/`RefCell` 违反 SMP 安全（project_memory 硬约束）。
- 所以用 `ProcNr`（进程表索引 newtype）：用索引替代指针，通过 `ProcessTable` 统一访问；BKL 保证索引有效性。

**实现**: `proc_ptr: Option<ProcNr>` / `fpu_owner: Option<ProcNr>` ([smp.rs:139](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs))。`Option` 替代 C 的 `NULL` 检查。

### D9. 单 CPU 退化：运行时 CAS vs cfg gate

**假设性推理**:
- 如果用 `#[cfg(feature = "smp")]` 编译时门控：单 CPU 编译时移除 BKL 逻辑，但代码路径分裂为两份，维护成本高。
- 如果用运行时 CAS（无 cfg gate）：单 CPU 时 CAS 一次成功（无竞争），不进入自旋；代码路径单一。
- 所以用运行时 CAS：零虚拟开销，代码路径单一，`CONFIG_SMP` 未定义时 `ncpus=1` 自动退化。

**实现**: `bkl_lock()` 总是用 CAS ([smp.rs:946](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs))，无 `#[cfg]` 门控。

### D10. CPU ID 类型：CpuId newtype vs type alias（R-10，2026-08-12）

**演进说明**：原设计用 `pub type CpuId = u32;`（type alias），编译器无法区分 CPU id 与普通 `u32`，`cpu_id as u32` / `cpu_id as usize` 散布全代码库。R-10 引入 `pub struct CpuId(u32);` newtype，提供类型安全和集中化的 `MAX_CPUS` 校验。

**假设性推理**:
- 如果保留 `type CpuId = u32`：编译器视 `CpuId` 与 `u32` 为同一类型，任何 `u32` 值（包括 `ncpus`、错误码、ABI 字段）都可作为 `cpu` 参数传递，无类型保护。
- 如果用 `pub struct CpuId(u32)` newtype：编译器强制区分 CPU id 与其他 `u32`；`new(u32) -> Option<Self>` 可在构造时校验 `< MAX_CPUS`。
- 如果 newtype 不提供 `raw()`/`index()`：调用方需用 `as` 转换访问内部值，破坏封装。所以提供 `raw() -> u32`（ABI 边界）和 `index() -> usize`（数组索引）两个访问器。

**保留为 `u32` 的边界**:
- `SmpArch` trait（`minix-arch` crate）—— arch 层不依赖 kernel 的 `CpuId`，避免循环依赖；kernel 调用时用 `.raw()` 转换。
- `SchedFields.cpu: AtomicU32` —— `CpuId` 不是原子类型；边界用 `CpuId::new_unchecked(load())` / `store(cpu.raw())`。
- ABI 结构体（`cpuinfo_t.cpu_id`、`ProcInfoStruct.p_cpu`）—— 匹配 C 布局，必须用 `u32`。
- `ncpus: u32` —— 是 CPU 计数，非 id，保留 `u32`。

**实现**: [`CpuId`](file:///home/xzhao/github/minix-rs/os/kernel/src/proc.rs) ([proc.rs:461](file:///home/xzhao/github/minix-rs/os/kernel/src/proc.rs)) — `pub struct CpuId(u32)` + `NONE`/`BSP` 常量 + `new`/`new_unchecked`/`raw`/`index`/`is_bsp`/`is_none` 方法。`SmpState.bsp_cpu_id` 字段 + 15 个 `cpu` 参数函数全部迁移到 `CpuId`。

**redox 对照**: redox `LogicalCpuId(u32)` newtype 采用相同 pattern。redox 不带 `MAX_CPUS` 校验；minix-rs 的 `CpuId::new()` 增加校验因为 minix-rs 有固定 `MAX_CPUS=32` 数组。

---

## 4. 实现详解

> 本章贴 `os/kernel/src/smp.rs` 真实代码片段，标注 file:line。DEFERRED 项诚实标注理由。

### 4.1 常量与 bitflags

```rust
/// Maximum number of CPUs.
/// C: `CONFIG_MAX_CPUS` — config.h
pub const MAX_CPUS: usize = 32;  // smp.rs:61

bitflags::bitflags! {
    /// CPU state flags.
    /// C: smp.h:32-33 — `CPU_IS_BSP` / `CPU_IS_READY`
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CpuFlags: u32 {
        const BSP = 1;   // CPU_IS_BSP
        const READY = 2; // CPU_IS_READY
    }
}  // smp.rs:70-77

bitflags::bitflags! {
    /// IPI scheduling task flags.
    /// C: smp.c:21-23 — `SCHED_IPI_STOP_PROC` / `SCHED_IPI_VM_INHIBIT` / `SCHED_IPI_SAVE_CTX`
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SchedIpiFlags: u32 {
        const STOP_PROC = 1;  // SCHED_IPI_STOP_PROC
        const VM_INHIBIT = 2; // SCHED_IPI_VM_INHIBIT
        const SAVE_CTX = 4;   // SCHED_IPI_SAVE_CTX
    }
}  // smp.rs:85-93
```

### 4.2 SmpArch trait（D7）

> trait 定义在 arch crate [`os/arch/src/arch/smp.rs`](file:///home/xzhao/github/minix-rs/os/arch/src/arch/smp.rs)；内核通过 `pub use minix_arch::SmpArch;`（[smp.rs:114](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)）re-export，避免内核 smp.rs 出现 `#[cfg(target_arch)]`。

```rust
/// Architecture-specific SMP operations.
/// D7: All hardware operations abstracted as trait methods.
/// Kernel code depends only on the trait, not on `#[cfg(target_arch)]`.
pub trait SmpArch {
    /// Send a schedule IPI to the target CPU.
    /// C: `arch_send_smp_schedule_ipi(cpu)` — smp.c:65
    fn send_sched_ipi(cpu: u32);

    /// Halt the current CPU (called by smp_ipi_halt_handler).
    /// C: `arch_smp_halt_cpu()` — smp.c:60
    fn halt_cpu();

    /// Acknowledge an IPI.
    /// C: `ipi_ack()` — smp.c:58,198
    fn ack_ipi();

    /// Boot an Application Processor (AP).
    /// x86-only: INIT+SIPI; aarch64: PSCI; riscv64: SBI.
    /// C: arch/i386/smp.c
    fn boot_ap(cpu: u32, entry: usize);

    /// Pause the CPU in a busy-wait loop (hint to CPU, not a trap).
    /// C: `arch_pause()` — smp.c:46
    /// All architectures use `core::hint::spin_loop()` (unified).
    fn pause() {
        core::hint::spin_loop();
    }

    /// Return the current CPU's ID.
    /// C: `cpuid` macro — arch/i386/include/arch_smp.h:11.
    /// x86_64: read from GS segment base; aarch64: TPIDR_EL1; riscv64: sscratch.
    /// Returns 0 (BSP) if SMP is not yet initialized.
    fn current_cpu() -> u32;
}  // arch/smp.rs:46-110
```

**实现状态**: ✅ 四个实现全部落地：
- `X86_64SmpArch`（[x86_64/smp.rs:150](file:///home/xzhao/github/minix-rs/os/arch/src/x86_64/smp.rs)）— LAPIC ICR + EOI + INIT/SIPI（`wait_icr_idle` + `lapic_write` ICR_HIGH/LOW）
- `AArch64SmpArch`（[arm64/smp.rs:115](file:///home/xzhao/github/minix-rs/os/arch/src/arm64/smp.rs)）— GIC SGIR + EOIR + PSCI `CPU_ON`（`gic_write` + PSCI ecall）
- `Riscv64SmpArch`（[riscv64/smp.rs:65](file:///home/xzhao/github/minix-rs/os/arch/src/riscv64/smp.rs)）— SBI `send_ipi` + `hart_start`（SBI ecall）
- `MockSmpArch`（[arch/smp.rs:119](file:///home/xzhao/github/minix-rs/os/arch/src/arch/smp.rs)）— `#[cfg(feature = "mock")]` 测试用 no-op

`minix_arch::CurrentSmpArch` 编译期别名（[lib.rs:298-311](file:///home/xzhao/github/minix-rs/os/arch/src/lib.rs)）按 `target_arch` 选择后端，内核代码零 `#[cfg(target_arch)]`。

### 4.3 CpuLocal — per-CPU 数据（D2）

```rust
/// Per-CPU local data, equivalent to C's `__cpu_local_vars`.
/// C: cpulocals.h:37-75 — `struct __cpu_local_vars`
///
/// Each CPU owns a private instance. Access via `SmpState::cpu_local(cpu)`.
/// BKL serializes cross-CPU access; same-CPU access is lock-free.
#[derive(Debug)]
pub struct CpuLocal {
    /// Currently running process. C: `proc_ptr` (cpulocals.h:40)
    pub proc_ptr: Option<ProcNr>,
    /// Billable process for time accounting. C: `bill_ptr` (cpulocals.h:41)
    pub bill_ptr: Option<ProcNr>,
    /// Slot index of the idle kernel task. C: `idle_proc` (cpulocals.h:42)
    pub idle_proc: ProcNr,
    /// Process owning this CPU's page tables. C: `ptproc` (cpulocals.h:55)
    pub ptproc: Option<ProcNr>,
    /// Whether this CPU is idle. C: `cpu_is_idle` (cpulocals.h:60)
    pub cpu_is_idle: bool,
    /// Whether idle loop was interrupted. C: `idle_interrupted` (cpulocals.h:62)
    pub idle_interrupted: bool,
    /// TSC at last context switch. C: `tsc_ctr_switch` (cpulocals.h:65)
    pub tsc_ctr_switch: u64,
    /// Last raw TSC reading. C: `cpu_last_tsc` (cpulocals.h:68)
    pub cpu_last_tsc: u64,
    /// Last time this CPU went idle. C: `cpu_last_idle` (cpulocals.h:69)
    pub cpu_last_idle: u64,
    /// Recursive pagefault detection. C: `pagefault_handled` (cpulocals.h:48)
    pub pagefault_handled: bool,
    /// Whether this CPU has an FPU. C: `fpu_presence` (cpulocals.h:72)
    pub fpu_presence: bool,
    /// FPU owner process. C: `fpu_owner` (cpulocals.h:73)
    pub fpu_owner: Option<ProcNr>,
    /// Per-CPU scheduler (ready queues). C: `run_q_head[]`/`run_q_tail[]` (cpulocals.h:58-59)
    pub scheduler: Scheduler,
}  // smp.rs:137-173
```

**与 C 的差异**（anti-translate）:
- `struct proc *` → `Option<ProcNr>`：用索引替代裸指针，`Option` 替代 `NULL` 检查
- `int cpu_is_idle` → `bool`：Rust 布尔类型更精确
- `char fpu_presence` → `bool`：同上
- `struct proc idle_proc` (嵌入结构体) → `idle_proc: ProcNr` (索引)：节省内存，进程表统一管理

> **`ptproc` 字段的 BSP 临时镜像**（P9-4，2026-08-13）：`CpuLocal::ptproc`（[smp.rs:147](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)）是 ptproc 跟踪的 SMP 最终归宿——每 CPU 记录"当前 CR3 装的是哪个进程"，供 `setcr3()` 的 `if (p == ptproc)` CR3-reload 决策用。但 SMP 尚未落地，`CpuLocal` 数组在单 CPU 下只有 BSP 一份且 `ptproc` 字段未在 `dispatch_vmctl(SetAddrSpace)` 路径中读写。
>
> 为让 `SetAddrSpace` 的 Step 3（`TlbArch::set_active_root`）在 BSP 单核阶段就能工作，P9-4 在 [os/kernel/src/lib.rs:2036](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs) 引入临时全局 `CURRENT_PTPROC_NR: AtomicI32`（sentinel = `i32::MIN`）+ 访问器 `current_ptproc_nr()` / `set_current_ptproc_nr()`（[lib.rs:2054/2075](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）。`init_post_and_memory` 在 arch 级 `set_ptproc` 之后调用 `set_current_ptproc_nr(VM_PROC_NR)` 记录 VM 为当前 ptproc；`dispatch_vmctl(SetAddrSpace)` 用 `current_ptproc_nr() == Some(target.p_nr)` 判断是否 reload CR3。这镜像 C 的 `get_cpulocal_var(ptproc) = vm`（protect.c:372）+ `if (p == get_cpulocal_var(ptproc))`（arch_do_vmctl.c:31）。详见 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md)。
>
> **SMP 落地时的迁移**：当 SMP 启用后，应**移除** `CURRENT_PTPROC_NR` 全局，改用 `CpuLocal::ptproc` 作为唯一真相源——通过 `cpu_local_mut(cpu).ptproc` 访问。`current_ptproc_nr()` / `set_current_ptproc_nr()` 访问器的实现将改为委托 per-CPU 字段（`current_ptproc_nr()` 读当前 CPU 的 `CpuLocal::ptproc`，`set_current_ptproc_nr(nr)` 写当前 CPU 的 `CpuLocal::ptproc`）。这样 `dispatch_vmctl` 的调用代码无需改动——只有访问器内部实现从"读全局"切到"读 per-CPU"。CpuLocal 的 BKL 保护（§1.2）保证 per-CPU 字段访问的线程安全。
>
> **`CURRENT_ROOT_PHYS` 同模型镜像**（P9-5，2026-08-13）：`init_proc_and_boot` 在非 mock 路径下需要包装 `arch_boot_impl` 创建并激活的 bootstrap 页表根以加载 VM ELF。为此在 [os/kernel/src/lib.rs:2103](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs) 引入 `CURRENT_ROOT_PHYS: AtomicU64`（sentinel = `u64::MAX`）+ 访问器 `current_root_phys()` / `set_current_root_phys()`。`arch_boot_impl` 在 `enable()` 成功后立即调用 `set_current_root_phys(root_page)`；`init_proc_and_boot` 通过 `current_root_phys().expect(...)` + `Paging::from_active_root` 包装根。SMP 落地时该全局也应迁移到 `CpuLocal::root_phys`，与 `ptproc` 同步迁移——每个 CPU 记录自己装入 CR3/TTBR0/satp 的根地址。详见 [09-vm-boot-protocol.md §4.9](09-vm-boot-protocol.md)。

### 4.4 CpuState（D3 — bitflags 替代裸 u32）

```rust
/// Per-CPU state entry.
/// C: smp.h:34-36 — `struct cpu { u32_t flags; }`
#[derive(Debug)]
pub struct CpuState {
    flags: CpuFlags,
}  // smp.rs:226-229

impl CpuState {
    pub const fn new() -> Self {
        Self { flags: CpuFlags::empty() }
    }
    pub fn set_flag(&mut self, flag: CpuFlags) { self.flags |= flag; }
    pub fn clear_flag(&mut self, flag: CpuFlags) { self.flags -= flag; }
    pub fn test_flag(&self, flag: CpuFlags) -> bool { self.flags.contains(flag) }
    pub fn is_ready(&self) -> bool { self.test_flag(CpuFlags::READY) }
}
```

### 4.5 SchedIpiData + SchedIpiFlags（D5/D6）

```rust
/// IPI scheduling data for cross-CPU operations.
/// C: smp.c:14-17 — `struct sched_ipi_data`
///
/// D6: `AtomicU32` replaces C's `volatile u32_t` for safe cross-CPU access
/// with explicit memory ordering.
#[derive(Debug)]
pub struct SchedIpiData {
    /// IPI task flags. C: `volatile u32_t flags`
    flags: AtomicU32,
    /// Target process number. C: `volatile u32_t data` (cast from `struct proc *`)
    ///
    /// D8: `ProcNr` index replaces C's raw pointer cast.
    target_proc: AtomicU32,
}  // smp.rs:271-276

impl SchedIpiData {
    pub const fn new() -> Self {
        Self {
            flags: AtomicU32::new(0),
            target_proc: AtomicU32::new(0),
        }
    }
    pub fn load_flags(&self) -> SchedIpiFlags {
        SchedIpiFlags::from_bits_truncate(self.flags.load(Ordering::Acquire))
    }
    pub fn set_flags(&self, flags: SchedIpiFlags) {
        self.flags.store(flags.bits(), Ordering::Release);
    }
    pub fn clear_flags(&self) {
        self.flags.store(0, Ordering::Release);
    }
    pub fn has_pending(&self) -> bool {
        !self.load_flags().is_empty()
    }
    pub fn set_target(&self, proc: ProcNr) {
        self.target_proc.store(proc as u32, Ordering::Release);
    }
    pub fn get_target(&self) -> ProcNr {
        self.target_proc.load(Ordering::Acquire) as ProcNr
    }
}
```

### 4.6 SmpState — 全局 SMP 状态

```rust
/// Global SMP state, equivalent to C's `ncpus`, `bsp_cpu_id`, `cpus[]`,
/// `__cpu_local_vars`, and `sched_ipi_data[]`.
///
/// C: smp.h:12-19, smp.c:7-11, cpulocals.h:37-75
///
/// All mutable access must be under BKL protection.
#[derive(Debug)]
pub struct SmpState {
    /// Number of CPUs. C: `ncpus` (smp.c:7)
    ncpus: u32,
    /// BSP CPU ID. C: `bsp_cpu_id` (smp.c:9)
    /// R-10 (2026-08-12): Changed from `u32` to `CpuId` newtype for type safety.
    bsp_cpu_id: CpuId,
    /// Per-CPU state array. C: `struct cpu cpus[CONFIG_MAX_CPUS]` (smp.c:11)
    cpus: [CpuState; MAX_CPUS],
    /// Per-CPU local data. C: `__cpu_local_vars CPULOCAL_ARRAY` (cpulocals.h:75)
    cpu_locals: [CpuLocal; MAX_CPUS],
    /// IPI scheduling data. C: `sched_ipi_data[CONFIG_MAX_CPUS]` (smp.c:19)
    sched_ipi_data: [SchedIpiData; MAX_CPUS],
    /// Number of APs that have finished booting. C: `ap_cpus_booted` (smp.c:25)
    ap_cpus_booted: AtomicU32,
}  // smp.rs:333-346
```

构造与访问方法（[smp.rs:352-480](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)）:

```rust
impl SmpState {
    /// Create a new SmpState for a single-CPU (BSP-only) configuration.
    /// C: `ncpus = 1`, `bsp_cpu_id = 0`, `cpu_set_flag(bsp_cpu_id, CPU_IS_READY)`
    pub fn new_single_cpu() -> Self { /* ... */ }

    /// Create SmpState with a specified number of CPUs.
    pub fn with_ncpus(ncpus: u32, bsp_cpu_id: u32) -> Self { /* ... */ }

    pub fn ncpus(&self) -> u32 { self.ncpus }
    pub fn bsp_cpu_id(&self) -> u32 { self.bsp_cpu_id }
    pub fn cpu_is_bsp(&self, cpu: u32) -> bool { cpu == self.bsp_cpu_id }
    pub fn cpu_is_ready(&self, cpu: u32) -> bool { /* ... */ }

    pub fn cpu_local(&self, cpu: u32) -> Option<&CpuLocal> { /* ... */ }
    pub fn cpu_local_mut(&mut self, cpu: u32) -> Option<&mut CpuLocal> { /* ... */ }

    /// Record that an AP has finished booting. C: `ap_boot_finished(cpu)` (smp.c:51)
    pub fn ap_boot_finished(&self) {
        self.ap_cpus_booted.fetch_add(1, Ordering::AcqRel);
    }

    /// Check if all APs have finished booting.
    pub fn all_aps_booted(&self) -> bool { /* ... */ }
}
```

### 4.7 smp_sched_handler — 完整 IPI 处理

```rust
/// Full IPI scheduling handler with RTS_SET and FPU save.
///
/// C: smp.c:156-187 — `smp_sched_handler()`
///
/// Reads flags → STOP_PROC sets RTS_PROC_STOP →
/// SAVE_CTX saves FPU → VM_INHIBIT sets RTS_VMINHIBIT →
/// clears flags.
///
/// Requires `&mut ProcessTable` for RTS_SET operations.
pub fn sched_handler_full(
    &mut self,
    proc_table: &mut crate::proc_table::ProcessTable,
    cpu: CpuId,
) {
    let ipi = &self.sched_ipi_data[cpu.index()];
    let flags = ipi.load_flags();
    if flags.is_empty() { return; }
    let target = ipi.get_target();

    // C: smp.c:167-169 — STOP_PROC
    if flags.contains(SchedIpiFlags::STOP_PROC) {
        proc_table.rts_set(target, RtsFlagsBits::PROC_STOP);
    }

    // C: smp.c:170-179 — SAVE_CTX (FPU save)
    if flags.contains(SchedIpiFlags::SAVE_CTX) {
        let used_fpu = proc_table
            .get(target)
            .map(|p| p.p_misc_flags.get().contains(MiscFlagsBits::EXT_REG_INITIALIZED))
            .unwrap_or(false);
        if used_fpu && self.cpu_locals[cpu.index()].fpu_owner == Some(target) {
            // C: disable_fpu_exception(); save_local_fpu(p, FALSE); release_fpu(p);
            //
            // Save the process's FPU state to its per-process `fpu_state`
            // buffer so it can be restored when the process resumes on
            // the target CPU after migration. Then release FPU ownership.
            //
            // `CurrentFpuArch` is a stateless ZST (selected at compile time
            // via `#[cfg(target_arch)]` in the arch crate), so constructing
            // a default instance is zero-cost. `FpuArch::save` issues the
            // architecture-specific save instruction (fxsave on x86-64,
            // stp q0..q31 on aarch64, fsd f0..f31 on riscv64).
            use minix_arch::{CurrentFpuArch, FpuArch};
            let fpu_arch = CurrentFpuArch::default();
            fpu_arch.disable_exception();
            if let Some(p) = proc_table.get_mut(target) {
                fpu_arch.save(&mut p.fpu_state);
            }
            // release_fpu(p): clear per-CPU fpu_owner so the next FPU
            // access traps (lazy restore on target CPU).
            self.cpu_locals[cpu.index()].fpu_owner = None;
        }
    }

    // C: smp.c:180-182 — VM_INHIBIT
    if flags.contains(SchedIpiFlags::VM_INHIBIT) {
        proc_table.rts_set(target, RtsFlagsBits::VMINHIBIT);
    }

    // C: __insn_barrier() + clear flags — smp.c:185-186
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    ipi.clear_flags();
}  // smp.rs:481-551
```

**已接入**: FPU save (`save_local_fpu`) — `FpuArch` trait 已实现（三架构 save/restore/disable_exception + `Default`），per-process `State` buffer 存储在 `KProcess.fpu_state: CurrentFpuState`（`proc.rs:985`）。`CurrentFpuState` 是 `CurrentFpuArch::State` 的类型别名（`arch/src/lib.rs:201-219`）：mock=ZST / x86-64 FXSAVE=512B / aarch64 FPSIMD=528B / riscv64 F/D=264B。`sched_handler_full` SAVE_CTX 路径已调用 `FpuArch::save(&mut owner.fpu_state)`；`fork_from` 通过 `CpuContextArch::inherit_fpu_state` 继承父进程 FPU 状态。

### 4.8 smp_schedule_sync — 同步 IPI

```rust
/// Synchronous cross-CPU scheduling operation.
///
/// C: smp.c:75-112 — `smp_schedule_sync(p, task)`
///
/// Sets IPI data → sends IPI → releases BKL → waits for completion →
/// reacquires BKL. Handles reentrant IPI while waiting.
///
/// # Safety contract
/// - Caller must hold BKL on entry
/// - `target_cpu` must differ from `current_cpu`
/// - BKL is released during wait; caller must not hold any other lock
pub fn schedule_sync<A: SmpArch>(
    &mut self,
    proc_table: &mut crate::proc_table::ProcessTable,
    target_cpu: CpuId,
    current_cpu: CpuId,
    target_proc: ProcNr,
    task: SchedIpiFlags,
) {
    debug_assert!(target_cpu != current_cpu, "schedule_sync: target == current CPU");
    debug_assert!(target_cpu.raw() < self.ncpus, "schedule_sync: target_cpu out of range");

    // Wait if another CPU has a pending request to the same target.
    // C: smp.c:85-95
    if self.sched_ipi_data[target_cpu.index()].has_pending() {
        bkl_unlock();
        while self.sched_ipi_data[target_cpu.index()].has_pending() {
            // Reentrant: handle our own IPI if pending
            if self.sched_ipi_data[current_cpu.index()].has_pending() {
                // R-05: forget guard — explicit bkl_unlock() below
                core::mem::forget(bkl_lock());
                self.sched_handler_full(proc_table, current_cpu);
                bkl_unlock();
            }
            A::pause();
        }
        // R-05: forget guard — BKL stays held, released later by caller
        core::mem::forget(bkl_lock());
    }

    // Set IPI data and flags
    self.sched_ipi_data[target_cpu.index()].set_target(target_proc);
    self.sched_ipi_data[target_cpu.index()].set_flags(task);
    // C: __insn_barrier() — smp.c:99
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    A::send_sched_ipi(target_cpu.raw());

    // Wait until target CPU finishes
    // C: smp.c:103-111
    bkl_unlock();
    while self.sched_ipi_data[target_cpu.index()].has_pending() {
        if self.sched_ipi_data[current_cpu.index()].has_pending() {
            // R-05: forget guard — explicit bkl_unlock() below
            core::mem::forget(bkl_lock());
            self.sched_handler_full(proc_table, current_cpu);
            bkl_unlock();
        }
        A::pause();
    }
    // R-05: forget guard — BKL stays held, released later by caller
    core::mem::forget(bkl_lock());
}  // smp.rs:552-605
```

**设计要点**:
1. 泛型 `<A: SmpArch>` 实现 trait 静态分发，零虚拟开销
2. `proc_table: &mut ProcessTable` 传入因为 `sched_handler_full` 需要修改进程的 RTS 标志
3. `current_cpu` 参数显式传入（测试时可 mock），生产代码从 `cpuid()` 获取
4. `core::sync::atomic::fence(SeqCst)` 替代 C 的 `__insn_barrier()`

### 4.9 跨 CPU 调度封装（4 个函数）

[smp.rs:606-687](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs):

```rust
/// Stop a process on a remote CPU.
/// C: smp.c:114-121 — `smp_schedule_stop_proc(p)`
pub fn schedule_stop_proc<A: SmpArch>(
    &mut self, proc_table: &mut crate::proc_table::ProcessTable,
    proc_nr: ProcNr, current_cpu: CpuId,
) {
    let (is_runnable, target_cpu) = proc_table
        .get(proc_nr)
        .map(|p| {
            let cpu = CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire));
            (p.is_runnable(), cpu)
        })
        .unwrap_or((false, CpuId::BSP));

    if is_runnable {
        self.schedule_sync::<A>(proc_table, target_cpu, current_cpu, proc_nr, SchedIpiFlags::STOP_PROC);
    } else {
        proc_table.rts_set(proc_nr, RtsFlagsBits::PROC_STOP);
    }
}

/// Set VMINHIBIT on a process on a remote CPU.
/// C: smp.c:123-130 — `smp_schedule_vminhibit(p)`
pub fn schedule_vminhibit<A: SmpArch>(
    &mut self, proc_table: &mut crate::proc_table::ProcessTable,
    proc_nr: ProcNr, current_cpu: CpuId,
) {
    // same shape as schedule_stop_proc, but with SchedIpiFlags::VM_INHIBIT
    // (actual code duplicates the load → CpuId::new_unchecked → unwrap_or(CpuId::BSP) pattern)
    let (is_runnable, target_cpu) = proc_table
        .get(proc_nr)
        .map(|p| {
            let cpu = CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire));
            (p.is_runnable(), cpu)
        })
        .unwrap_or((false, CpuId::BSP));

    if is_runnable {
        self.schedule_sync::<A>(proc_table, target_cpu, current_cpu, proc_nr, SchedIpiFlags::VM_INHIBIT);
    } else {
        proc_table.rts_set(proc_nr, RtsFlagsBits::VMINHIBIT);
    }
}

/// Stop a process and save its full context (for migration).
/// C: smp.c:132-140 — `smp_schedule_stop_proc_save_ctx(p)`
pub fn schedule_stop_proc_save_ctx<A: SmpArch>(
    &mut self, proc_table: &mut crate::proc_table::ProcessTable,
    proc_nr: ProcNr, current_cpu: CpuId,
) {
    let target_cpu = proc_table
        .get(proc_nr)
        .map(|p| CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire)))
        .unwrap_or(CpuId::BSP);
    self.schedule_sync::<A>(
        proc_table, target_cpu, current_cpu, proc_nr,
        SchedIpiFlags::STOP_PROC | SchedIpiFlags::SAVE_CTX,
    );
}

/// Migrate a process to a different CPU.
/// C: smp.c:142-154 — `smp_schedule_migrate_proc(p, dest_cpu)`
pub fn schedule_migrate_proc<A: SmpArch>(
    &mut self, proc_table: &mut crate::proc_table::ProcessTable,
    proc_nr: ProcNr, current_cpu: CpuId, dest_cpu: CpuId,
) {
    self.schedule_stop_proc_save_ctx::<A>(proc_table, proc_nr, current_cpu);
    // C: p->p_cpu = dest_cpu; RTS_UNSET(p, RTS_PROC_STOP);
    if let Some(p) = proc_table.get_mut(proc_nr) {
        p.p_sched.cpu.store(dest_cpu.raw(), Ordering::Release);
    }
    proc_table.rts_unset(proc_nr, RtsFlagsBits::PROC_STOP);
}
```

### 4.10 ipi_sched_handler + ipi_halt_handler

[smp.rs:688-718](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs):

```rust
/// IPI schedule handler: ack + preempt current process.
/// C: smp.c:194-204 — `smp_ipi_sched_handler()`
pub fn ipi_sched_handler<A: SmpArch>(
    &mut self, proc_table: &mut crate::proc_table::ProcessTable,
    current_cpu: CpuId,
) {
    A::ack_ipi();
    let curr = self.cpu_locals[current_cpu.index()].proc_ptr;
    if let Some(curr_nr) = curr {
        if curr_nr != proc_nr::IDLE {
            proc_table.rts_set(curr_nr, RtsFlagsBits::PREEMPTED);
        }
    }
}

/// IPI halt handler: ack + stop local timer + halt CPU.
/// C: smp.c:56-61 — `smp_ipi_halt_handler()`
///
/// Stops the per-CPU local timer before halting to prevent timer
/// interrupts during the halt. Delegates to `clock::stop_local_timer()`,
/// which constructs a transient `CurrentClockArch` instance (same pattern
/// as `clock::read_tsc()`).
pub fn ipi_halt_handler<A: SmpArch>(&self) {
    A::ack_ipi();
    crate::clock::stop_local_timer();
    A::halt_cpu();
}
```

### 4.11 wait_for_APs

[smp.rs:719-745](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs):

```rust
/// BSP waits for all APs to finish booting.
/// C: smp.c:30-49 — `wait_for_APs_to_finish_booting()`
///
/// Releases BKL → waits for `ap_cpus_booted == ncpus - 1` →
/// reacquires BKL. Tolerates partial AP boot failure.
pub fn wait_for_aps<A: SmpArch>(&self) {
    // Count ready CPUs (tolerate partial failure)
    // C: smp.c:36-41
    let n = self.cpus.iter().filter(|c| c.is_ready()).count() as u32;
    if n != self.ncpus {
        // C: printf("WARNING: only %d out of %d cpus booted\n", n, ncpus)
        use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole};
        Console::write_str("WARNING: not all CPUs booted\n");
    }

    // Release BKL so APs can enter kernel
    // C: smp.c:44
    bkl_unlock();

    // Wait for APs
    // C: smp.c:45-46
    let expected = n.saturating_sub(1);
    while self.ap_cpus_booted.load(Ordering::Acquire) != expected {
        A::pause();
    }

    // Reacquire BKL
    // C: smp.c:48
    // R-05: forget guard — BKL stays held, released later by caller
    core::mem::forget(bkl_lock());
}
```

### 4.12 BKL 实现（D1/D3/D4）

```rust
/// The single Big Kernel Lock protecting cross-CPU shared kernel state.
/// C: `SPINLOCK_DEFINE(big_kernel_lock)` — smp.c:27
static BKL_LOCKED: AtomicBool = AtomicBool::new(false);  // smp.rs:781

/// Guard returned by `bkl_lock`. RAII: dropping releases the BKL.
///
/// R-05 (2026-08-12): `BklGuard` is now RAII by default — `Drop` calls
/// `bkl_unlock()`. This eliminates the "forgot to call `bkl_unlock()`"
/// class of deadlocks. For code paths that need to keep the BKL held
/// after the guard's scope (e.g. `kernel_call_dispatch` →
/// `kernel_call_finish`), use `core::mem::forget(guard)`. For explicit
/// early release, use [`BklGuard::release`].
///
/// D6 update: The old design had two types — `BklGuard` (non-RAII) and
/// `BklGuardRaii` (RAII). R-05 unifies them into a single RAII type.
/// Cross-function BKL transfer uses `mem::forget`; blocking-IPC release/
/// reacquire uses `release()`/`bkl_lock()`.
///
/// C: `BKL_LOCK()` / `BKL_UNLOCK()` — smp.c:27, spinlock.h
pub struct BklGuard {
    /// If `true`, `Drop` will call `bkl_unlock()`. Set to `false` by
    /// [`release`](Self::release) to prevent double-unlock.
    active: bool,
}  // smp.rs:798-802

impl BklGuard {
    /// Obtain a BKL section witness that borrows this guard (R-03).
    pub fn section(&self) -> BklSection<'_> { /* ... */ }  // smp.rs:810

    /// Explicitly release the BKL early, consuming the guard.
    /// After `release()`, `Drop` will NOT call `bkl_unlock()` again.
    pub fn release(mut self) {
        self.active = false;
        bkl_unlock();
    }  // smp.rs:819-822
}

impl Drop for BklGuard {
    fn drop(&mut self) {
        if self.active { bkl_unlock(); }
    }
}  // smp.rs:825-831

/// Compile-time witness that the caller holds the BKL.
/// D4: Capability pattern — type-level proof of BKL ownership.
#[must_use = "BklSection is a witness; it does NOT release the BKL on drop"]
pub struct BklSection<'a> {
    _lifetime: core::marker::PhantomData<&'a BklGuard>,
}  // smp.rs:867-869

pub fn bkl_lock_section<'a>() -> BklSection<'a> {
    // R-05: BklGuard is now RAII (Drop releases BKL), so we must forget
    // the guard to keep the BKL held. The caller must call bkl_unlock()
    // explicitly. The BklSection witness proves to the type system that
    // the BKL was acquired.
    let guard = bkl_lock();
    core::mem::forget(guard);
    BklSection { _lifetime: core::marker::PhantomData }
}  // smp.rs:877-885

pub fn smp_state_with<'a, 'b>(
    _section: &'a BklSection<'b>, state: &'a SmpState,
) -> &'a SmpState {
    state
}  // smp.rs:905-916

pub fn bkl_lock() -> BklGuard {
    while BKL_LOCKED
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    BklGuard { active: true }
}  // smp.rs:946-955

pub fn bkl_unlock() {
    debug_assert!(BKL_LOCKED.load(Ordering::Acquire),
        "bkl_unlock() called but BKL is not held");
    BKL_LOCKED.store(false, Ordering::Release);
}  // smp.rs:980-988

// R-05: bkl_lock_raii() and BklGuardRaii have been removed.
// BklGuard is now RAII (Drop releases BKL). Use bkl_lock() for all
// critical sections. For cross-function BKL transfer, use mem::forget(guard).
// For explicit early release, use guard.release().
```

### 4.13 BKL 接入点清单

**已接入**（grep 验证 `rg "bkl_lock\|bkl_unlock" os/kernel/src/`）:

| 接入点 | 文件 | 说明 | 验证 |
|--------|------|------|------|
| 系统调用入口 | `syscall.rs:411,566` | `bkl_lock()` 在 `kernel_call_dispatch` / `dispatch_ipc_entry` | ✅ grep 确认 |
| 系统调用完成 | `syscall.rs:2580,2605` | `bkl_unlock()` 在 `kernel_call_finish` 的所有返回路径 | ✅ grep 确认 |

**待接入**（DEFERRED — C 源码有但 Rust 尚未接入）:

| 接入点 | C 位置 | Rust 状态 | 接入方式 |
|--------|--------|----------|---------|
| 异常处理入口 | arch trap entry | ❌ DEFERRED | `exception_dispatcher.rs::handle` 需加 `bkl_lock()` |
| kmain 启动 | main.c:149 (step 8.5) | ❌ DEFERRED | `lib.rs::kmain` 需在 `switch_to_user` 前获取 BKL |
| switch_to_user 释放 | main.c (switch_to_user) | ❌ DEFERRED | `switch_to_user` 需在调度循环前 `bkl_unlock()` |
| 系统调用恢复 | kernel_call_resume | ✅ 已接入（syscall.rs:2622） | 重入 `kernel_call_dispatch`（BKL 在 411 重新获取）→ `kernel_call_finish`（2580/2605 释放）；生产调用方 `proc_table.rs:859`（VM resume 路径） |

**共享数据 BKL 保护**（DEFERRED）:

| 共享数据 | 访问点 | 接入方式 |
|---------|--------|---------|
| `ProcessTable::procs[]` | `proc_table.rs` 跨 CPU 方法 | `let _g = bkl_lock();` |
| `KProcess::p_nextready`/`p_caller_q` | `ipc.rs` | 同上 |
| `Scheduler` 队列操作 | `sched.rs` | 同上 |
| `IrqManager::hooks[]` | `irq_manager.rs` | 同上 |
| `ClockState` | `clock.rs` | 同上 |
| Timer tick handler | `clock.rs::do_clocktick` | 同上 |
| IPC sendrecv suspend/resume | `ipc.rs` | 同上 |

> **注意**: `BklSection` witness pattern (D4) 当前仅在测试中使用，尚未作为生产代码函数参数。待异常处理/共享数据接入时，可将 `BklSection` 作为 `&mut ProcessTable` 等方法的参数，实现编译期 BKL 持有证明。
>
> **`BklGuard::section()` + `*_with()` accessor 模式**（FIX-09: R-03 编译期 BKL 见证，2026-08-12）：上述 D4 witness pattern 已接入生产代码。`BklGuard` 新增 `section(&self) -> BklSection<'_>` 方法，从已持有的 guard 派生见证；`lib.rs` 7 个全局访问器（`proc_table`/`priv_table`/`irq_manager`/`try_irq_manager`/`smp_state`/`try_smp_state`/`ipc_filter_pool`）新增 `*_with(&BklSection<'_>)` 安全版本，BKL 持有证明从运行时 `# Safety` 注释升级为编译期类型约束。boot 期单线程路径使用 `*_boot_unchecked()` 显式标注。`kernel_call_dispatch` 在 BKL 获取后立即派生 `BklSection` 并传递给 `dispatch_irqctl`（使用 `irq_manager_with`）。旧 `unsafe` 版本保留供未迁移路径使用，`bkl_is_locked()` 扩展到 `debug_assertions` 构建供诊断使用。

### 4.14 DEFERRED 函数依赖矩阵

| 函数 | C 位置 | Rust 实现状态 | 依赖 |
|------|--------|--------------|------|
| `schedule_sync` | smp.c:75-112 | ✅ 已实现 | `SmpArch::send_sched_ipi` |
| `schedule_stop_proc` | smp.c:114-121 | ✅ 已实现 | `schedule_sync` |
| `schedule_vminhibit` | smp.c:123-130 | ✅ 已实现 | `schedule_sync` |
| `schedule_stop_proc_save_ctx` | smp.c:132-140 | ✅ 已实现 | `schedule_sync` |
| `schedule_migrate_proc` | smp.c:142-154 | ✅ 已实现 | `schedule_stop_proc_save_ctx` |
| `sched_handler_full` | smp.c:156-187 | ✅ 已实现（FPU save 已接入 `KProcess.fpu_state`） | per-process `FpuArch::State` 存储（`KProcess.fpu_state: CurrentFpuState`，三架构均有 `Default` impl） |
| `ipi_sched_handler` | smp.c:194-204 | ✅ 已实现 | `SmpArch::ack_ipi` |
| `ipi_halt_handler` | smp.c:56-61 | ✅ 已实现 | `clock::stop_local_timer()` + `SmpArch::halt_cpu` |
| `wait_for_aps` | smp.c:30-49 | ✅ 已实现 | `SmpArch::pause`（已有 `spin_loop`） |
| `boot_ap` | arch/i386/smp.c | ✅ 已实现（arch crate） | `SmpArch::boot_ap` — x86_64 INIT+SIPI / aarch64 PSCI / riscv64 SBI |
| `smp_init` | arch/i386/smp.c | DEFERRED | ACPI/MADT 表解析 + `SmpArch::boot_ap` |
| FPU save (`save_local_fpu`) | smp.c:175 | ✅ 已实现 | `KProcess.fpu_state: CurrentFpuState` 字段（`proc.rs:985`）+ `FpuArch::save/restore` trait 方法（三架构 impl，含 `Default`）；`sched_handler_full` 在 SAVE_CTX 路径调用 `FpuArch::save(&mut owner.fpu_state)` |
| `stop_local_timer` | smp.c:59 | ✅ 已实现（[clock.rs:267](file:///home/xzhao/github/minix-rs/os/kernel/src/clock.rs)） | `ClockArch::stop_local_timer` |
| `boot_lock` | smp.c:28 | DEFERRED | 随 `smp_init` 一并实现 |

---

## 5. 测试

> 本章列出实际 `fn test_*` 函数名，可通过 `rg "fn test_" os/kernel/src/smp.rs` grep 验证。

### 5.1 已实现测试（28 个，[smp.rs:1016-1439](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)）

| 测试函数 | 验证行为 | 对应 C 符号 / 设计决策 |
|---------|---------|----------------------|
| `test_smp_state_single_cpu` | 单 CPU 初始化：ncpus=1, bsp=0, BSP+READY flags | `new_single_cpu` |
| `test_smp_state_multi_cpu` | 多 CPU 初始化：ncpus>1, 非 BSP 无 READY | `with_ncpus` |
| `test_cpu_flags` | CpuFlags bitflags set/clear/test | `cpu_set_flag/clear_flag/test_flag` |
| `test_cpu_local` | CpuLocal 字段访问 | `cpu_local/cpu_local_mut` |
| `test_cpu_local_default` | Default trait 等价于 new | `Default for CpuLocal` |
| `test_cpu_local_set_running` | set_running 设置 proc_ptr+bill_ptr | `set_running` |
| `test_cpu_local_note_context_switch` | note_context_switch 记录 TSC | `note_context_switch` |
| `test_sched_ipi_flags` | SchedIpiFlags 位组合操作 | `SchedIpiFlags` (D5) |
| `test_sched_ipi_data` | SchedIpiData flags/target 读写 | `SchedIpiData` (D6) |
| `test_ap_boot_counting` | ap_boot_finished 递增计数 | `ap_boot_finished` (smp.c:51) |
| `test_handle_sched_ipi` | handle_sched_ipi 读 flags+清零 | `smp_sched_handler` (部分) |
| `test_handle_sched_ipi_empty` | 无 IPI 时 handle_sched_ipi 无操作 | `smp_sched_handler` |
| `test_bkl_lock_unlock` | BKL 获取/释放 | `bkl_lock/bkl_unlock` (D1) |
| `test_bkl_reentrant_is_caller_responsibility` | BKL 非递归是调用者责任 | `bkl_lock` SAFETY |
| `test_bkl_section_provides_typed_access` | BklSection 提供类型安全访问 | `BklSection` (D4) |
| `test_bkl_section_paired_unlock` | BklSection 配对解锁 | `smp_state_with` |
| `test_bkl_section_drop_does_not_unlock` | BklSection Drop 不释放 BKL（witness 非守卫） | D4 决策 |
| `test_bkl_guard_section_enables_with_accessors` | BklGuard::section() 派生见证 | R-03 `*_with()` accessor |
| `test_bkl_guard_section_lifetime_tied_to_guard` | BklSection 生命周期绑定 BklGuard | R-03 类型约束 |
| `test_bkl_guard_releases_on_drop` | BklGuard Drop 释放 BKL（RAII） | R-05 `BklGuard` RAII |
| `test_bkl_guard_nested_release` | RAII + 显式 unlock 共存 | R-05 `mem::forget` + RAII |
| `test_bkl_guard_release_method` | `guard.release()` 显式提前释放 | R-05 `BklGuard::release` |
| `test_smp_arch_trait_mock` | SmpArch trait 可 mock | `SmpArch` (D7) |
| `test_sched_handler_full_empty` | sched_handler_full 空标志无操作 | `smp_sched_handler` (smp.c:156) |
| `test_sched_handler_full_stop_proc` | STOP_PROC 设 RTS_PROC_STOP | `smp_sched_handler` (smp.c:167) |
| `test_sched_handler_full_vminhibit` | VM_INHIBIT 设 RTS_VMINHIBIT | `smp_sched_handler` (smp.c:180) |
| `test_ipi_sched_handler_idle_no_preempt` | IDLE 进程不设 PREEMPTED | `smp_ipi_sched_handler` (smp.c:194) |
| `test_wait_for_aps_single_cpu` | BSP 等待 APs 完成（单 CPU 退化） | `wait_for_APs` (smp.c:30) |

### 5.2 待补充测试（DEFERRED 函数完善后）

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_smp_schedule_sync` | 同步 IPI 设置 flags + 等待清零 | 多 CPU 模拟环境 |
| `test_smp_schedule_stop_proc_runnable` | runnable 进程走 sync 路径 | `schedule_sync` 多 CPU |
| `test_smp_schedule_stop_proc_not_runnable` | 非 runnable 直接 RTS_SET | `schedule_stop_proc` |
| `test_smp_sched_handler_full_save_ctx` | SAVE_CTX 保存 FPU（已接入 `KProcess.fpu_state`） | `FpuArch` trait + `KProcess.fpu_state` 字段 |
| `test_smp_ipi_sched_handler_non_idle` | 非 IDLE 进程设 PREEMPTED | 多 CPU 模拟 |
| `test_smp_ipi_halt_handler` | halt CPU 调用 | `SmpArch::halt_cpu` mock |
| `test_smp_schedule_migrate_proc` | 迁移修改 p_cpu + RTS_UNSET | `schedule_sync` 多 CPU |

### 5.3 从 todo.md 转移的待办（2026-08-14，todo.md 已清空）

> 以下项目来自 `01-stage-kernel/todo.md`（原 §6.3/§6.4/§6.8/§12.2），属 SMP/多核语义范围，随 todo.md 清空转移至此。

| 待办 | 原出处 | 当前状态 |
|------|--------|---------|
| init/load 顺序约束缺少测试（`init_proc_and_boot` → `init_post_and_memory` 调用顺序违反时应 panic） | todo §6.3 [P1] | 未实现 |
| `init_ap` 路径验证缺失——x86_64 `init_ap` 仍为 `panic!` 占位（protection.rs:368），占位触发后无测试 | todo §6.4 [P1] | 未实现 |
| QEMU GDB 脚本未集成到 CI（手动脚本） | todo §6.8 [P2] | 未实现 |
| ptproc per-CPU 语义跟踪：`PostInitArch::set_ptproc` 三架构占位（`let _ = vm_page_table`）——Direct Map 取代 createpde 后单核语义可接受（占位有测试 + 07 doc 注释），多核 per-CPU ptproc 待 SMP 落地 | todo §12.2 [P1] | 占位（单核 OK），SMP 跟踪 |

---

## 6. 参见

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — per-CPU 调度队列与 pick_proc
- [15-clock-timer.md](15-clock-timer.md) — BSP/AP 时钟中断差异 + quantum 递减
- [14-exception-interrupt.md](14-exception-interrupt.md) — BKL 在异常入口的获取
- [10-switch-to-user.md](10-switch-to-user.md) — BKL 在 switch_to_user 的释放
- [22-privilege.md](22-privilege.md) — VMINHIBIT 与权限管理
- [17-syscall-process.md](17-syscall-process.md) — fork/exec 的 CPU 亲和性继承
