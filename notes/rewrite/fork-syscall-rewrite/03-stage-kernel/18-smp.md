# 18-smp: SMP 多核与 APIC

> **分类**: Kernel 多核与补全
> **源码**: `minix3/minix/kernel/smp.c`(205行), `arch/i386/apic.c`(1304行), `arch/i386/arch_smp.c`(360行)
> **说明**: Local APIC / I/O APIC 初始化、AP Boot 流程、CPU-local 变量、IPI 中断——多核架构的全貌

---

## 1. 概述

### 1.1 概念定义/作用

**SMP（Symmetric Multiprocessing）** 是 Minix3 对多核处理器的支持机制。在 SMP 系统中，多个 CPU 共享同一物理内存，每个 CPU 独立执行内核代码，通过 IPI（Inter-Processor Interrupt）进行跨 CPU 通信。

Minix3 的 SMP 设计采用**大内核锁（BKL）模型**：同一时刻只有一个 CPU 可以执行内核代码，其他 CPU 在内核入口等待锁。这种模型简化了内核的并发正确性，但限制了多核的并行性——内核操作是串行的，只有用户态代码可以真正并行执行。

SMP 的核心组件包括：

1. **CPU 局部变量（CPU-local variables）**：每个 CPU 有独立的调度队列、当前进程指针、FPU 拥有者等
2. **APIC（Advanced Programmable Interrupt Controller）**：Local APIC 处理本地中断和 IPI，I/O APIC 处理外部设备中断
3. **AP（Application Processor）启动**：BSP 唤醒 AP，AP 完成初始化后进入调度循环
4. **IPI（Inter-Processor Interrupt）**：跨 CPU 通信，用于调度请求、TLB 刷新、VMINHIBIT 操作

### 1.2 与 Minix3 的对应关系

| 功能 | 函数/宏 | 源文件 |
|------|---------|--------|
| SMP 初始化 | `smp_init()` | smp.c |
| AP 启动等待 | `wait_for_APs_to_finish_booting()` | smp.c:30 |
| AP 启动完成 | `ap_boot_finished()` | smp.c:52 |
| 调度 IPI | `smp_schedule()` | smp.c:63 |
| 同步调度请求 | `smp_schedule_sync()` | smp.c:75 |
| 停止进程 | `smp_schedule_stop_proc()` | smp.c:114 |
| VMINHIBIT 请求 | `smp_schedule_vminhibit()` | smp.c:123 |
| 迁移进程 | `smp_schedule_migrate_proc()` | smp.c:142 |
| IPI 调度处理 | `smp_sched_handler()` | smp.c:156 |
| IPI 确认处理 | `smp_ipi_sched_handler()` | smp.c:194 |
| APIC 初始化 | `lapic_init()` / `ioapic_init()` | arch/i386/apic.c |
| AP 启动 | `start_all_aps()` | arch/i386/arch_smp.c |
| CPU 局部变量 | `get_cpulocal_var()` | cpulocals.h |

### 1.3 关键状态/机制说明

**大内核锁（BKL）**：全局自旋锁 `big_kernel_lock`，确保同一时刻只有一个 CPU 在内核态执行。BSP 在 `kmain()` 中获取 BKL，AP 在进入内核时必须先获取 BKL。BKL 的持有时间从内核入口到 `switch_to_user()` 退出，覆盖了整个内核执行路径。

**CPU 局部变量**：`__cpu_local_vars` 结构体数组，每个 CPU 一个实例。SMP 下通过 `get_cpulocal_var(name)` 宏（展开为 `__cpu_local_vars[cpuid].name`）访问当前 CPU 的变量。单 CPU 编译时退化为直接结构体访问。

**sched_ipi_data**：每个 CPU 一个的调度 IPI 数据结构，包含 `flags`（请求类型）和 `data`（目标进程指针）。当一个 CPU 需要另一个 CPU 执行调度操作时，设置目标 CPU 的 `sched_ipi_data` 并发送 IPI。

**AP 启动流程**：BSP 通过 ACPI/MADT 表发现 AP，向 AP 发送 INIT + SIPI（Startup IPI）中断唤醒 AP。AP 从实模式启动，切换到保护模式后跳转到内核入口，完成初始化后调用 `ap_boot_finished()` 通知 BSP。

### 1.4 行为规则

1. **BKL 串行化**：同一时刻只有一个 CPU 执行内核代码，其他 CPU 自旋等待
2. **CPU 局部队列**：每个 CPU 有独立的调度队列，入队/出队操作无需跨 CPU 同步
3. **IPI 同步请求**：`smp_schedule_sync()` 发送 IPI 后等待目标 CPU 完成操作，期间释放 BKL 允许目标 CPU 执行
4. **进程 CPU 亲和性**：进程通过 `p_cpu` 绑定到特定 CPU，入队时加入对应 CPU 的队列
5. **FPU 所有权**：每个 CPU 的 FPU 由一个进程独占，迁移进程时需保存 FPU 状态
6. **AP 启动超时容忍**：若部分 AP 未成功启动，系统仍可运行（打印警告）

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 SMP 配置常量

| 常量 | 值 | 含义 |
|------|-----|------|
| `CONFIG_MAX_CPUS` | 可配置 | 最大 CPU 数 |
| `CONFIG_SMP` | 编译选项 | 启用 SMP 支持 |

#### 2.1.2 调度 IPI 标志

| 标志 | 值 | 含义 |
|------|-----|------|
| `SCHED_IPI_STOP_PROC` | 1 | 停止进程（设置 RTS_PROC_STOP） |
| `SCHED_IPI_VM_INHIBIT` | 2 | 设置 VMINHIBIT |
| `SCHED_IPI_SAVE_CTX` | 4 | 保存完整上下文（含 FPU） |

#### 2.1.3 CPU 状态标志

| 标志 | 含义 |
|------|------|
| `CPU_IS_READY` | CPU 已完成初始化 |
| `CPU_IS_BOOTED` | CPU 已启动 |

### 2.2 核心数据结构

#### 2.2.1 struct cpu（CPU 描述符）

| 字段 | 类型 | 含义 |
|------|------|------|
| `flags` | `unsigned` | CPU 状态标志 |
| `cpu_id` | `unsigned` | CPU 编号（APIC ID） |
| `cpu_name` | `char[]` | CPU 名称字符串 |

#### 2.2.2 struct __cpu_local_vars（CPU 局部变量）

| 字段 | 类型 | 含义 |
|------|------|------|
| `proc_ptr` | `struct proc *` | 当前运行进程指针 |
| `bill_ptr` | `struct proc *` | 计费进程指针 |
| `idle_proc` | `struct proc` | IDLE 进程存根 |
| `pagefault_handled` | `int` | 页缺失处理标志（防递归） |
| `ptproc` | `struct proc *` | 当前加载的页表所属进程 |
| `run_q_head[NR_SCHED_QUEUES]` | `struct proc **` | 就绪队列头指针 |
| `run_q_tail[NR_SCHED_QUEUES]` | `struct proc **` | 就绪队列尾指针 |
| `cpu_is_idle` | `int` | CPU 是否空闲 |
| `fpu_owner` | `struct proc *` | FPU 当前所属进程 |

#### 2.2.3 struct sched_ipi_data（调度 IPI 数据）

| 字段 | 类型 | 含义 |
|------|------|------|
| `flags` | `volatile u32_t` | 请求类型（SCHED_IPI_* 位组合） |
| `data` | `volatile u32_t` | 目标进程指针（强制转换） |

### 2.3 关键函数分析

#### 2.3.1 smp_init()——SMP 初始化

`minix3/minix/kernel/smp.c`

```c
void smp_init(void)
```

**功能**：初始化 SMP 子系统，启动所有 AP。

**行为**：
1. 通过 ACPI/MADT 表发现 CPU 拓扑
2. 初始化 Local APIC 和 I/O APIC
3. 调用 `start_all_aps()` 启动所有 AP
4. 等待所有 AP 完成启动：`wait_for_APs_to_finish_booting()`
5. 若 AP 启动失败，回退到单 CPU 模式

#### 2.3.2 smp_schedule_sync()——同步调度请求

`minix3/minix/kernel/smp.c:75-112`

```c
static void smp_schedule_sync(struct proc *p, unsigned task)
```

**功能**：向目标 CPU 发送同步调度请求，等待操作完成。

**行为**：
1. 获取目标 CPU 编号 `cpu = p->p_cpu`
2. 若目标 CPU 已有待处理请求，先等待其完成（期间处理本 CPU 的请求）
3. 设置 `sched_ipi_data[cpu].data = p`，`sched_ipi_data[cpu].flags |= task`
4. 发送调度 IPI：`arch_send_smp_schedule_ipi(cpu)`
5. 释放 BKL，等待目标 CPU 清除 flags
6. 等待期间处理本 CPU 的请求（避免死锁）

**死锁避免**：两个 CPU 互相发送同步请求可能导致死锁。`smp_schedule_sync()` 在等待时检查并处理本 CPU 的请求，打破可能的循环依赖。

#### 2.3.3 smp_sched_handler()——IPI 调度处理

`minix3/minix/kernel/smp.c:156-187`

```c
void smp_sched_handler(void)
```

**功能**：处理本 CPU 收到的调度 IPI 请求。

**行为**：
1. 读取 `sched_ipi_data[cpu].flags`
2. 若 `SCHED_IPI_STOP_PROC`：`RTS_SET(p, RTS_PROC_STOP)` 停止进程
3. 若 `SCHED_IPI_SAVE_CTX`：保存 FPU 状态（若进程使用过 FPU 且是本 CPU 的 FPU 拥有者）
4. 若 `SCHED_IPI_VM_INHIBIT`：`RTS_SET(p, RTS_VMINHIBIT)` 设置 VMINHIBIT
5. 清除 `sched_ipi_data[cpu].flags = 0`（通知请求方操作完成）

#### 2.3.4 smp_ipi_sched_handler()——IPI 确认处理

`minix3/minix/kernel/smp.c:194-204`

```c
void smp_ipi_sched_handler(void)
```

**功能**：确认 IPI 并抢占当前进程。

**行为**：
1. 确认 IPI：`ipi_ack()`
2. 若当前进程非 IDLE，设置 `RTS_PREEMPTED` 使其让出 CPU

#### 2.3.5 smp_schedule_migrate_proc()——进程迁移

`minix3/minix/kernel/smp.c:142-154`

```c
void smp_schedule_migrate_proc(struct proc *p, unsigned dest_cpu)
```

**功能**：将进程从当前 CPU 迁移到目标 CPU。

**行为**：
1. 在源 CPU 上停止进程并保存完整上下文（含 FPU）
2. 修改 `p->p_cpu = dest_cpu`
3. 解除 `RTS_PROC_STOP`，进程将在目标 CPU 的调度队列中运行

#### 2.3.6 wait_for_APs_to_finish_booting()——等待 AP 启动

`minix3/minix/kernel/smp.c:30-49`

```c
void wait_for_APs_to_finish_booting(void)
```

**功能**：BSP 等待所有 AP 完成启动。

**行为**：
1. 统计已就绪的 CPU 数量
2. 若不等于 `ncpus`，打印警告
3. 释放 BKL，等待 `ap_cpus_booted == n - 1`
4. 重新获取 BKL

### 2.4 调用关系/调用点分析

#### 2.4.1 SMP 启动流程

```
BSP: kmain()
  └─ smp_init()
       ├─ ACPI/MADT 表解析 → 发现 CPU 拓扑
       ├─ lapic_init() / ioapic_init()
       ├─ start_all_aps()
       │    └─ 向每个 AP 发送 INIT + SIPI
       │         └─ AP 从实模式启动
       │              └─ AP 入口 (arch_smp_init.S)
       │                   ├─ 切换到保护模式
       │                   ├─ 加载 GDT/IDT
       │                   ├─ 启用分页
       │                   └─ ap_boot_finished() → 进入调度
       └─ wait_for_APs_to_finish_booting()
            └─ BKL_UNLOCK → 等待 AP → BKL_LOCK
```

#### 2.4.2 跨 CPU 调度操作

```
CPU A: 需要停止 CPU B 上的进程 P
  └─ smp_schedule_stop_proc(P)
       └─ smp_schedule_sync(P, SCHED_IPI_STOP_PROC)
            ├─ sched_ipi_data[B].flags = SCHED_IPI_STOP_PROC
            ├─ arch_send_smp_schedule_ipi(B)
            └─ BKL_UNLOCK → 等待 flags 清零

CPU B: 收到 IPI
  └─ smp_sched_handler()
       ├─ RTS_SET(P, RTS_PROC_STOP)
       └─ sched_ipi_data[B].flags = 0  → CPU A 恢复
```

### 2.5 设计要点/特殊处理

#### 2.5.1 大内核锁的取舍

BKL 模型的优势是简单——内核代码无需考虑并发，所有数据结构的访问都是串行的。代价是内核操作无法并行，多核仅在用户态代码并行执行时才有收益。对于 Minix3 这种微内核（内核代码路径短），BKL 的性能损失可以接受。

#### 2.5.2 CPU 局部变量的缓存行问题

`cpulocals.h` 中有 FIXME 注释：CPU 局部变量数组 `__cpu_local_vars[CONFIG_MAX_CPUS]` 的元素可能共享缓存行，导致伪共享（false sharing）。理想情况下应填充结构体使每个 CPU 的实例对齐到独立缓存行，但当前实现未做此优化。

#### 2.5.3 smp_schedule_sync 的死锁避免

两个 CPU 互相发送同步请求时，`smp_schedule_sync()` 在等待期间检查并处理本 CPU 的请求。这种"在等待中服务"的模式打破了循环依赖：CPU A 等待 CPU B 时，若 CPU B 也在等待 CPU A，CPU A 会先处理 CPU B 发来的请求，然后 CPU B 才能完成 CPU A 的请求。

#### 2.5.4 FPU 迁移

进程迁移时必须保存和恢复 FPU 状态。`SCHED_IPI_SAVE_CTX` 标志触发 FPU 保存：若进程使用过 FPU 且是源 CPU 的 FPU 拥有者，调用 `save_local_fpu()` 保存状态后调用 `release_fpu()` 释放所有权。目标 CPU 在进程恢复执行时重新初始化 FPU。

#### 2.5.5 AP 启动的容错

若部分 AP 未成功启动（硬件故障、ACPI 表错误等），系统仍可运行。`wait_for_APs_to_finish_booting()` 仅打印警告，不 panic。`ncpus` 变量记录实际可用的 CPU 数，调度器据此分配进程。

#### 2.5.6 APIC 与传统 PIC 的回退

若 APIC 不可用（`config_no_apic`）或 SMP 被禁用（`config_no_smp`），`smp_single_cpu_fallback()` 回退到传统 PIC 和单 CPU 模式。这确保了 Minix3 在不支持 APIC 的硬件上仍可运行。
