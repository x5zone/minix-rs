# 16-smp-outline.v1.md — 文档结构契约

> **文档**: `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/16-smp.md`
> **C 源码**: `minix3/minix/kernel/smp.c` (204 行), `smp.h`, `cpulocals.h`, `proc.h` (相关字段)
> **Rust 实现**: `os/kernel/src/smp.rs` (911 行), `proc_table.rs` (SMP 相关), `sched.rs`
> **创建**: 2026-08-01
> **依据**: `16-smp-glm-structure.md`（知识点全集 + 诊断）
> **方法**: C 源码 → OS 理论 → Rust 对照（非反向）

---

## 一、章节骨架与主语

### Ch1 主语：CPU + 矛盾（"多个 CPU 如何共享一个内核？"）

核心问题：**多个 CPU 共享同一内核代码与数据时，如何在"并发执行"与"保持简单"之间取舍？**

Minix3 的回答：**用串行化换简单性**——BKL 保证同一时刻只有一个 CPU 在内核态，其余 CPU 自旋等待。这是粗粒度策略，牺牲并行性换取避免细粒度锁的 deadlock/ordering 难题。

| 节 | 标题 | 灵魂本质（一句话） | 概念组 |
|----|------|-------------------|--------|
| §1.1 | BKL：用串行化换简单性 | "BKL 是粗粒度 SMP 策略——同一时刻只有一个 CPU 在内核态，用互斥避免细粒度锁的死锁难题" | A |
| §1.2 | per-CPU 数据：CPU 私有状态为何免锁 | "每个 CPU 拥有私有数据副本，访问无需同步——这是 cache 局部性与免锁的联合产物" | B |
| §1.3 | IPI：CPU 间如何对话 | "IPI 是 CPU 间的软中断通知，分为异步通知（仅发不等待）与同步请求（等待目标 CPU 完成）" | C |
| §1.4 | CPU 亲和性：进程为何绑定 CPU | "进程绑定到特定 CPU 可减少迁移成本与 cache 失效——迁移需 IPI 同步保存完整上下文" | E |
| §1.5 | AP 启动握手：BSP 如何唤醒并等待 APs | "BSP 通过 INIT+SIPI 唤醒 AP，AP 完成初始化后递增计数器，BSP 释放 BKL 等待握手完成" | D |
| §1.6 | 硬件抽象：跨架构统一 IPI 接口 | "不同架构的 IPI 发送/确认/停机指令不同，必须抽象为 trait 以保持内核代码架构无关" | F |

### Ch2 主语：C 源码符号（file:line 锚定）

每节以 C 符号为单元，附 file:line，说明语义与调用关系。

### Ch3 主语：设计决策（hypothesis-driven）

采用"如果 X 设计会有 Y 问题所以用 Z"格式，禁止"旧版/最初/后来/我们改成"迭代叙事。

### Ch4 主语：Rust 实现（真实代码，非 stub）

贴 smp.rs 真实代码片段，标注 file:line。缺失函数诚实标注 DEFERRED + 理由。

### Ch5 主语：测试函数（可 grep 验证）

列出实际 `fn test_*` 函数名，每个测试对应一个被测行为。

---

## 二、详细大纲

### Ch1. 概念建构（concept-driven）

#### §1.1 BKL：用串行化换简单性

**灵魂本质**: BKL 是粗粒度 SMP 策略——同一时刻只有一个 CPU 在内核态，用互斥避免细粒度锁的死锁难题。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 多核系统中，内核数据结构（进程表、调度队列、IPC 链表）若被多 CPU 并发修改，需要细粒度锁保护每个数据结构。细粒度锁带来死锁（锁序约束）、活锁（优先级反转）、复杂性（每条路径都要分析锁交互）三大难题。
- **WHAT**: BKL 用一个全局自旋锁串行化所有内核代码路径。持有时禁止睡眠/调度/等待 IPC/等待锁；其他 CPU 在内核入口自旋等待。
- **HOW**: C 用 `SPINLOCK_DEFINE(big_kernel_lock)` (smp.c:27) + `BKL_LOCK()/BKL_UNLOCK()` 宏。BSP 在 `kmain()` 获取 BKL (main.c:149)，AP 在内核入口获取。BKL 持有期覆盖内核入口到 `switch_to_user()` 退出。

**关键约束**:
1. 临界区禁止睡眠（spinlock 持有者睡眠会导致其他 CPU 死锁）
2. 阻塞操作前必须释放 BKL（`smp_schedule_sync` smp.c:86,103 / `wait_for_APs` smp.c:44 在 wait 前 `BKL_UNLOCK()`）
3. BKL 非递归（同 CPU 二次获取死锁；smp.c:80 `assert(cpu != mycpu)` 间接体现）

**单 CPU 退化**: `CONFIG_SMP` 未定义时 BKL 退化为 compiler fence（无竞争时 spinlock 无需自旋）。

#### §1.2 per-CPU 数据：CPU 私有状态为何免锁

**灵魂本质**: 每个 CPU 拥有私有数据副本，访问无需同步——这是 cache 局部性与免锁的联合产物。

**核心字段**（C: cpulocals.h:37-75 `struct __cpu_local_vars`）:
- `proc_ptr`: 当前运行进程（调度器快速访问）
- `bill_ptr`: 计费进程（时钟中断记账，可能是被抢占的系统进程）
- `idle_proc`: idle 进程存根（每 CPU 一个，无 runnable 进程时切入）
- `ptproc`: 当前页表进程（共享页表的进程无法用 proc_ptr 判断 CR3 是否需重载）
- `run_q_head/tail`: per-CPU 就绪队列（入队/出队免锁，仅跨 CPU 迁移需 IPI 同步）
- `cpu_is_idle`/`idle_interrupted`: idle 状态标志
- `tsc_ctr_switch`/`cpu_last_tsc`/`cpu_last_idle`: TSC 时间戳记账
- `fpu_presence`/`fpu_owner`: FPU 所有权（FPU 是 CPU 私有资源，迁移需保存/恢复）
- `pagefault_handled`: 递归缺页检测（缺页处理中再次缺页会死锁）

**访问方式**:
- SMP: `get_cpulocal_var(name)` → `__cpu_local_vars[cpuid].name` (cpulocals.h:15)
- 单 CPU: `__cpu_local_vars.name` (cpulocals.h:27，无数组下标)

**cache 对齐**: C 源码 cpulocals.h:18 FIXME 注明"应 padding 防止 false sharing"，但未实现。Rust 版本同样未做 cache 行对齐（对齐 C 现状）。

#### §1.3 IPI：CPU 间如何对话

**灵魂本质**: IPI 是 CPU 间的软中断通知，分为异步通知（仅发不等待）与同步请求（等待目标 CPU 完成）。

**两类 IPI**:
1. **异步 IPI** (`smp_schedule`, smp.c:63-66): 仅调用 `arch_send_smp_schedule_ipi(cpu)` 通知目标 CPU，不等待。用于抢占等无返回值场景。
2. **同步 IPI** (`smp_schedule_sync`, smp.c:75-112): 设置 flags+data → 发 IPI → 释放 BKL → 等待 flags 清零 → 重获 BKL。用于跨 CPU 调度操作（停止/抑制/迁移进程）。

**同步 IPI 的重入处理** (smp.c:88-93,105-109): 等待目标 CPU 时，若本 CPU 也收到 IPI（`sched_ipi_data[mycpu].flags` 非零），先 `BKL_LOCK()` 处理自己的 IPI（调用 `smp_sched_handler()`）再继续等待。这避免了 IPI 响应饥饿。

**IPI 标志** (smp.c:21-23):
- `SCHED_IPI_STOP_PROC` (1): 停止目标进程（设置 `RTS_PROC_STOP`）
- `SCHED_IPI_VM_INHIBIT` (2): 设置 `RTS_VMINHIBIT`（地址空间正在变更）
- `SCHED_IPI_SAVE_CTX` (4): 保存完整上下文（含 FPU 状态，用于迁移前）

#### §1.4 CPU 亲和性：进程为何绑定 CPU

**灵魂本质**: 进程绑定到特定 CPU 可减少迁移成本与 cache 失效——迁移需 IPI 同步保存完整上下文。

**亲和性来源**:
- 进程通过 `p_cpu` 字段绑定到 CPU (proc.h)
- fork 时继承父进程的 `p_cpu`
- `smp_schedule_migrate_proc` (smp.c:142-154) 显式迁移

**迁移成本**:
1. 同步 IPI 通知源 CPU 停止进程（`STOP_PROC | SAVE_CTX`）
2. 源 CPU 保存 FPU 状态（`SAVE_CTX` 分支，smp.c:170-178）
3. 修改 `p_cpu` 字段
4. 解除 `RTS_PROC_STOP` 让进程在新 CPU 上运行

**为什么不无脑迁移**: 迁移会导致 cache 冷启动（新 CPU 上无该进程的数据缓存）；频繁迁移抵消 per-CPU 队列的免锁优势。

#### §1.5 AP 启动握手：BSP 如何唤醒并等待 APs

**灵魂本质**: BSP 通过 INIT+SIPI 唤醒 AP，AP 完成初始化后递增计数器，BSP 释放 BKL 等待握手完成。

**启动时序** (smp.c:30-54):
1. BSP 在 `kmain()` 持有 BKL，调用 `smp_init()` 发现并唤醒 APs
2. BSP 调用 `wait_for_APs_to_finish_booting()` (smp.c:30):
   - 统计 `CPU_IS_READY` 的 CPU 数（容忍部分 AP 启动失败，smp.c:36-41）
   - `BKL_UNLOCK()` 让 AP 能进入内核 (smp.c:44)
   - 自旋等待 `ap_cpus_booted == n - 1` (smp.c:45-46)
   - `BKL_LOCK()` 重新获取 (smp.c:48)
3. AP 启动完成后调用 `ap_boot_finished(cpu)` (smp.c:51-54) 递增 `ap_cpus_booted`

**AP 启动协议** (arch-specific, x86-only):
- BSP 发 INIT IPI → 等待 → 发 SIPI（起始向量）
- AP 从实模式启动，切换到保护模式，跳转到内核入口
- aarch64/riscv64 用 PSCI/SBI（详见 arch 层文档）

#### §1.6 硬件抽象：跨架构统一 IPI 接口

**灵魂本质**: 不同架构的 IPI 发送/确认/停机指令不同，必须抽象为 trait 以保持内核代码架构无关。

**跨架构差异表**:

| CPU 问题 | x86_64 | aarch64 | riscv64 | Rust 抽象 |
|---------|--------|---------|---------|-----------|
| 发送 IPI | APIC ICR 写 | GIC GICD_SGIR | PLIC/SBI | `SmpArch::send_sched_ipi(cpu)` |
| 停机 CPU | `hlt` | `wfi` | `wfi` | `SmpArch::halt_cpu()` |
| IPI 确认 | APIC EOI | GIC EOIR | PLIC claim | `SmpArch::ack_ipi()` |
| 忙等提示 | `pause` | `yield` | `pause` | `core::hint::spin_loop()` (统一) |
| AP 启动 | INIT+SIPI | PSCI CPU_ON | SBI HSM | `SmpArch::boot_ap(cpu, entry)` (x86-only 标注) |

**设计原则**: 内核代码（smp.rs）只依赖 trait 方法，不出现 `#[cfg(target_arch)]` 行为选择。各架构在 arch 层提供 trait 实现。

---

### Ch2. C 源码分析（file:line 锚定）

#### §2.1 全局状态与常量

| 符号 | 位置 | 说明 |
|------|------|------|
| `ncpus` | smp.c:7 | CPU 总数（运行时检测） |
| `ht_per_core` | smp.c:8 | 每物理核的超线程数 |
| `bsp_cpu_id` | smp.c:9 | BSP CPU 编号 |
| `struct cpu cpus[CONFIG_MAX_CPUS]` | smp.c:11 | CPU 状态数组 |
| `CONFIG_MAX_CPUS` | config.h | 最大 CPU 数上限（32） |
| `CPU_IS_BSP` | smp.h:31 | BSP 标志位（值=1） |
| `CPU_IS_READY` | smp.h:32 | CPU 就绪标志位（值=2） |

#### §2.2 BKL 定义

| 符号 | 位置 | 说明 |
|------|------|------|
| `SPINLOCK_DEFINE(big_kernel_lock)` | smp.c:27 | 全局内核自旋锁定义 |
| `SPINLOCK_DEFINE(boot_lock)` | smp.c:28 | AP 启动同步锁 |
| `BKL_LOCK()/BKL_UNLOCK()` | spinlock.h | 获取/释放 BKL 宏 |

#### §2.3 per-CPU 数据结构

`struct __cpu_local_vars` (cpulocals.h:37-75) — 详见 §1.2 字段表。

访问宏:
- `get_cpulocal_var(name)` → `__cpu_local_vars[cpuid].name` (SMP) / `__cpu_local_vars.name` (单 CPU)
- `get_cpu_var(cpu, name)` → `__cpu_local_vars[cpu].name` (跨 CPU 访问)

#### §2.4 IPI 调度数据与标志

```c
struct sched_ipi_data {
    volatile u32_t flags;  // SCHED_IPI_* 位组合
    volatile u32_t data;   // 目标进程指针（cast 自 struct proc*）
};
static struct sched_ipi_data sched_ipi_data[CONFIG_MAX_CPUS];  // smp.c:19
```

标志常量 (smp.c:21-23):
- `SCHED_IPI_STOP_PROC` = 1
- `SCHED_IPI_VM_INHIBIT` = 2
- `SCHED_IPI_SAVE_CTX` = 4

#### §2.5 核心函数 file:line 索引

| 函数 | 位置 | 语义 |
|------|------|------|
| `wait_for_APs_to_finish_booting()` | smp.c:30-49 | BSP 释放 BKL 等待 APs，容忍部分失败 |
| `ap_boot_finished(cpu)` | smp.c:51-54 | AP 递增 `ap_cpus_booted` |
| `smp_ipi_halt_handler()` | smp.c:56-61 | IPI 停机：ack + 停定时器 + arch halt |
| `smp_schedule(cpu)` | smp.c:63-66 | 异步 IPI：仅发不等待 |
| `smp_schedule_sync(p, task)` | smp.c:75-112 | 同步 IPI：设数据→发 IPI→释放 BKL→等待→重获 BKL；含重入处理 |
| `smp_schedule_stop_proc(p)` | smp.c:114-121 | if runnable: sync(STOP_PROC); else: RTS_SET(PROC_STOP) |
| `smp_schedule_vminhibit(p)` | smp.c:123-130 | if runnable: sync(VM_INHIBIT); else: RTS_SET(VMINHIBIT) |
| `smp_schedule_stop_proc_save_ctx(p)` | smp.c:132-140 | sync(STOP_PROC \| SAVE_CTX) — 迁移前保存 FPU |
| `smp_schedule_migrate_proc(p, dest_cpu)` | smp.c:142-154 | sync(STOP \| SAVE_CTX) → 改 p_cpu → RTS_UNSET(PROC_STOP) |
| `smp_sched_handler()` | smp.c:156-187 | IPI 处理：读 flags→STOP_PROC 设 RTS→SAVE_CTX 保存 FPU→VM_INHIBIT 设 RTS→清 flags |
| `smp_ipi_sched_handler()` | smp.c:194-204 | IPI ack + 若当前非 IDLE 设 RTS_PREEMPTED |

#### §2.6 调用关系图（时序）

**BSP 启动时序**:
```
kmain() [main.c:149]
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

---

### Ch3. 设计决策（hypothesis-driven）

#### D1. BKL 表达：Spinlock vs RwLock vs Mutex

**假设性推理**:
- 如果用 `RwLock`：BKL 临界区禁止睡眠，但 `RwLock` 的 write 侧可能阻塞等待 reader 释放，等价于睡眠 → 违反 BKL 约束。
- 如果用 `Mutex`：`Mutex` 的 `lock()` 返回 `Result`， poisoning 语义与 spinlock 语义不符；且 `Mutex` 在 no_std 下需自旋或 futex，内核态无 futex。
- 所以用 `AtomicBool` + CAS 自旋（等价于 C 的 `SPINLOCK_DEFINE`）：无睡眠风险，no_std 友好，不依赖第三方 crate。

**实现**: `static BKL_LOCKED: AtomicBool` + `compare_exchange(false, true, Acquire, Relaxed)` 自旋 (smp.rs)。

#### D2. per-CPU 容器：固定数组 vs Vec

**假设性推理**:
- 如果用 `Vec<CpuLocal>`：`Vec` 需动态分配，no_std 下需 `alloc` crate；且 per-CPU 数据在内核启动早期就需访问（早于 allocator 初始化）。
- 如果用 `Box<[CpuLocal]>`：同上，依赖 allocator。
- 所以用 `[CpuLocal; MAX_CPUS]` 固定大小数组：编译期分配在 BSS，启动早期可访问，no_std 兼容。

**实现**: `cpu_locals: [CpuLocal; MAX_CPUS]` (smp.rs)。`MAX_CPUS = 32` 对齐 `CONFIG_MAX_CPUS` (config.h)。

#### D3. BKL 释放模式：非 RAII vs RAII

**假设性推理**:
- 如果 `BklGuard` 实现 `Drop` 自动释放：`smp_schedule_sync` 等函数在阻塞等待前需显式释放 BKL，但 RAII 的 Drop 只在作用域结束触发——无法在作用域中间释放。
- 如果用 `Drop` + 嵌套作用域：每次释放/重获都要新开作用域，代码可读性差，且容易遗漏重获。
- 所以 `BklGuard` 故意不实现 `Drop` 释放——显式 `bkl_unlock()` 标记释放点，`bkl_lock()` 标记重获点，阻塞操作前后的 BKL 状态变化可见。

**实现**: `pub struct BklGuard { _private: () }` 不实现 Drop (smp.rs)。提供 RAII 版本 `BklGuardRaii`（通过 Drop 释放）用于无阻塞操作的普通临界区。

#### D4. BKL 类型见证：BklSection witness（Rust 独有）

**假设性推理**:
- C 用注释约定"调用者必须持有 BKL"，但编译器不检查。
- 如果 Rust 也只用注释：同样无编译期保证，调用者可能忘记加锁。
- 所以用类型系统编码不变量：`BklSection<'a>` 是一个持有 BKL 的类型见证，通过 `smp_state_with()` 返回 `&BklSection`——只有持有见证才能访问 `SmpState` 的可变方法。

**实现**: `BklSection<'a>` + `smp_state_with(|s: &BklSection| { ... })` (smp.rs)。这是 Capability pattern：类型见证作为能力令牌。

#### D5. IPI 标志：bitflags vs 裸位

**假设性推理**:
- 如果用 `u32` 裸位操作：`flags |= 1` / `flags & 2` 等魔法数字，类型不安全，易写错位。
- 如果用 `enum`：IPI 标志是位组合（`STOP_PROC | SAVE_CTX`），enum 无法表达位组合语义。
- 所以用 `bitflags!` 宏：类型安全，支持位组合（`|`/`&`/`contains`），且 `from_bits_truncate` 容忍未知位。

**实现**: `bitflags! { pub struct SchedIpiFlags: u32 { const STOP_PROC = 1; ... } }` (smp.rs)。

#### D6. IPI 数据原子性：AtomicU32 vs volatile

**假设性推理**:
- 如果用 `volatile u32`（C 方式）：Rust 无 `volatile` 关键字，需用 `UnsafeCell` + `volatile_read/write`，unsafe 块增多。
- 如果用 `AtomicU32`：标准库提供 Acquire/Release 语义，安全且明确内存序；跨 CPU 通信天然需要原子操作。
- 所以用 `AtomicU32`：`flags: AtomicU32` + `load(Acquire)` / `store(Release)` / `fetch_or(AcqRel)`。

**实现**: `SchedIpiData { flags: AtomicU32, target_proc: AtomicU32 }` (smp.rs)。

#### D7. arch 抽象：SmpArch trait（新增）

**假设性推理**:
- 如果用 `#[cfg(target_arch)]` 行为选择：内核代码出现架构分支，违反"硬件抽象为 trait"原则；每加一个架构需修改 smp.rs。
- 如果用函数指针表：C 风格，类型不安全，且无法利用 Rust 的 trait dispatch 优化。
- 所以定义 `trait SmpArch { fn send_sched_ipi(cpu); fn halt_cpu(); fn ack_ipi(); fn boot_ap(cpu, entry); }`：各架构在 arch 层提供实现，内核代码依赖 trait。

**实现**: 新增 `SmpArch` trait (smp.rs)；x86_64/aarch64/riscv64 在 `os/arch/src/arch/` 提供实现。x86_64 的 `boot_ap` 用 INIT+SIPI，aarch64 用 PSCI，riscv64 用 SBI。

#### D8. per-CPU 索引：ProcNr vs 裸指针

**假设性推理**:
- 如果用 `*mut KProcess` 裸指针（C 方式）：不安全，可能悬垂；无法做借用检查。
- 如果用 `Rc<RefCell<KProcess>>`：跨 CPU 共享 `Rc`/`RefCell` 违反 SMP 安全（project_memory 硬约束）。
- 所以用 `ProcNr`（进程表索引 newtype）：用索引替代指针，通过 `ProcessTable` 统一访问；BKL 保证索引有效性。

**实现**: `proc_ptr: Option<ProcNr>` / `fpu_owner: Option<ProcNr>` (smp.rs)。`Option` 替代 C 的 `NULL` 检查。

#### D9. 单 CPU 退化：运行时 CAS vs cfg gate

**假设性推理**:
- 如果用 `#[cfg(feature = "smp")]` 编译时门控：单 CPU 编译时移除 BKL 逻辑，但代码路径分裂为两份，维护成本高。
- 如果用运行时 CAS（无 cfg gate）：单 CPU 时 CAS 一次成功（无竞争），不进入自旋；代码路径单一。
- 所以用运行时 CAS：零虚拟开销，代码路径单一，`CONFIG_SMP` 未定义时 `ncpus=1` 自动退化。

**实现**: `bkl_lock()` 总是用 CAS (smp.rs)，无 `#[cfg]` 门控。

---

### Ch4. 实现详解（真实代码）

#### §4.1 CpuLocal — per-CPU 数据

贴 `smp.rs` 的 `CpuLocal` struct 完整定义 + `new()` + `set_running()` + `note_context_switch()`。标注 C 字段对应关系。

#### §4.2 CpuState — CPU 状态

贴 `CpuState` struct + `CpuFlags` bitflags + `set_flag/clear_flag/test_flag/is_ready` 方法。

#### §4.3 SmpState — 全局 SMP 状态

贴 `SmpState` struct + `new_single_cpu()` + `cpu_is_bsp()`/`cpu_is_ready()`/`cpu_local()`/`cpu_local_mut()` 方法。

#### §4.4 SchedIpiData — IPI 调度数据

贴 `SchedIpiData` struct + `SchedIpiFlags` bitflags + `load_flags/set_flags/clear_flags/has_pending/set_target/get_target` 方法。

#### §4.5 BKL 实现

贴 `BKL_LOCKED: AtomicBool` + `bkl_lock()`/`bkl_unlock()` + `BklGuard`（非 RAII）+ `BklSection` witness + `smp_state_with()`。

**BKL 接入点清单**（当前状态）:
- `kernel_call_dispatch()` — 系统调用入口获取
- `kernel_call_finish()` — 系统调用完成释放
- `ExceptionDispatcher::handle()` — 异常处理获取/释放
- `kmain()` step 8.5 — 启动时获取
- `switch_to_user()` — 调度前释放

**待接入点**（诚实标注 DEFERRED）:
- Timer tick handler
- IPC sendrecv suspend/resume 路径
- Per-CPU run queue 跨 CPU 访问

#### §4.6 SmpArch trait（新增，DEFERRED）

```rust
/// Architecture-specific SMP operations.
///
/// Each architecture must provide an implementation of this trait.
/// Kernel code depends only on the trait, not on `#[cfg(target_arch)]`.
pub trait SmpArch {
    /// Send a schedule IPI to the target CPU.
    /// C: arch_send_smp_schedule_ipi(cpu) — smp.c:65
    fn send_sched_ipi(cpu: u32);

    /// Halt the current CPU (called by smp_ipi_halt_handler).
    /// C: arch_smp_halt_cpu() — smp.c:60
    fn halt_cpu();

    /// Acknowledge an IPI.
    /// C: ipi_ack() — smp.c:58,198
    fn ack_ipi();

    /// Boot an AP (x86-only: INIT+SIPI; aarch64: PSCI; riscv64: SBI).
    /// C: arch/i386/smp.c
    fn boot_ap(cpu: u32, entry: usize);
}
```

**DEFERRED 状态**: trait 定义后，各 arch 实现需在 `os/arch/src/arch/` 提供。当前仅 x86_64 的 AP 启动协议（INIT+SIPI）在 C 源码中有完整实现，aarch64/riscv64 需参考 PSCI/SBI 规范。

#### §4.7 缺失函数诚实标注（DEFERRED）

以下函数在 C 源码中存在但 Rust 尚未实现，标注 DEFERRED + 理由:

| 函数 | C 位置 | DEFERRED 理由 |
|------|--------|--------------|
| `smp_schedule_sync()` | smp.c:75-112 | 依赖 `SmpArch::send_sched_ipi` trait 实现 + BKL release/reacquire 测试 |
| `smp_schedule_stop_proc()` | smp.c:114-121 | 依赖 `smp_schedule_sync` |
| `smp_schedule_vminhibit()` | smp.c:123-130 | 依赖 `smp_schedule_sync` |
| `smp_schedule_stop_proc_save_ctx()` | smp.c:132-140 | 依赖 `smp_schedule_sync` |
| `smp_schedule_migrate_proc()` | smp.c:142-154 | 依赖 `smp_schedule_sync` + FPU save |
| `smp_sched_handler()` 完整语义 | smp.c:156-187 | 当前 `handle_sched_ipi` 仅读 flags+清零，缺 RTS_SET/FPU save |
| `smp_ipi_sched_handler()` | smp.c:194-204 | 依赖 `SmpArch::ack_ipi` |
| `smp_ipi_halt_handler()` | smp.c:56-61 | 依赖 `SmpArch::halt_cpu` + `stop_local_timer` |
| `wait_for_APs_to_finish_booting()` | smp.c:30-49 | 依赖 `arch_pause`（已用 `spin_loop` 替代）+ SMP 启动测试 |
| `boot_lock` | smp.c:28 | AP 启动同步锁，随 `wait_for_APs` 一并实现 |

---

### Ch5. 测试（可 grep 函数名）

#### §5.1 现有测试（已实现）

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_smp_state_single_cpu` | 单 CPU 初始化：ncpus=1, bsp=0, BSP+READY flags | `new_single_cpu` |
| `test_smp_state_multi_cpu` | 多 CPU 初始化：ncpus>1, 非 BSP 无 READY | `new_multi_cpu` |
| `test_cpu_flags` | CpuFlags bitflags set/clear/test | `cpu_set_flag/clear_flag/test_flag` |
| `test_cpu_local` | CpuLocal 字段默认值 | `CpuLocal::new` |
| `test_cpu_local_default` | Default trait 等价于 new | `Default for CpuLocal` |
| `test_cpu_local_set_running` | set_running 设置 proc_ptr+bill_ptr | `set_running` |
| `test_cpu_local_note_context_switch` | note_context_switch 记录 TSC | `note_context_switch` |
| `test_sched_ipi_flags` | SchedIpiFlags 位组合操作 | `SchedIpiFlags` |
| `test_sched_ipi_data` | SchedIpiData flags/target 读写 | `SchedIpiData` |
| `test_ap_boot_counting` | ap_boot_finished 递增计数 | `ap_boot_finished` |
| `test_handle_sched_ipi` | handle_sched_ipi 读 flags+清零 | `smp_sched_handler` (部分) |
| `test_handle_sched_ipi_empty` | 无 IPI 时 handle_sched_ipi 无操作 | `smp_sched_handler` |
| `test_bkl_lock_unlock` | BKL 获取/释放 | `bkl_lock/bkl_unlock` |
| `test_bkl_guard_is_marker` | BklGuard 是零成本标记 | `BklGuard` |
| `test_bkl_reentrant_is_caller_responsibility` | BKL 非递归是调用者责任 | `bkl_lock` SAFETY |
| `test_bkl_section_provides_typed_access` | BklSection 提供类型安全访问 | `BklSection` |
| `test_bkl_section_paired_unlock` | BklSection 配对解锁 | `smp_state_with` |
| `test_bkl_section_drop_does_not_unlock` | BklGuard Drop 不释放 BKL | D3 决策 |

#### §5.2 待补充测试（DEFERRED 函数实现后）

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_smp_schedule_sync` | 同步 IPI 设置 flags + 等待清零 | `SmpArch` trait + mock |
| `test_smp_schedule_stop_proc_runnable` | runnable 进程走 sync 路径 | `smp_schedule_sync` |
| `test_smp_schedule_stop_proc_not_runnable` | 非 runnable 直接 RTS_SET | `smp_schedule_stop_proc` |
| `test_smp_sched_handler_full` | STOP_PROC/SAVE_CTX/VM_INHIBIT 完整语义 | `smp_sched_handler` 完整 |
| `test_smp_ipi_sched_handler` | IPI ack + 非 IDLE 设 PREEMPTED | `SmpArch::ack_ipi` |

---

### Ch6. 参见

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — per-CPU 调度队列与 pick_proc
- [15-clock-timer.md](15-clock-timer.md) — BSP/AP 时钟中断差异 + quantum 递减
- [14-exception-interrupt.md](14-exception-interrupt.md) — BKL 在异常入口的获取
- [10-switch-to-user.md](10-switch-to-user.md) — BKL 在 switch_to_user 的释放
- [22-privilege.md](22-privilege.md) — VMINHIBIT 与权限管理
- [17-syscall-process.md](17-syscall-process.md) — fork/exec 的 CPU 亲和性继承

---

## 三、知识点覆盖矩阵

| 概念组 | Ch1 | Ch2 | Ch3 | Ch4 | Ch5 |
|--------|-----|-----|-----|-----|-----|
| A. BKL | §1.1 | §2.2 | D1, D3, D4 | §4.5 BklSection | test_bkl_* |
| B. per-CPU | §1.2 | §2.3 | D2, D8 | §4.1 CpuLocal | test_cpu_local_* |
| C. IPI | §1.3 | §2.4, §2.5 | D5, D6 | §4.4 SchedIpiData | test_sched_ipi_* |
| D. CPU 状态 | §1.5 | §2.1 | D9 | §4.2, §4.3 SmpState | test_smp_state_*, test_ap_boot_* |
| E. 跨 CPU 调度 | §1.4 | §2.5 | D7 | §4.7 (DEFERRED) | §5.2 待补充 |
| F. arch 抽象 | §1.6 | §2.5 | D7 | §4.6 SmpArch | (arch 层测试) |

---

## 四、断裂修复表

| 断裂点 | 修复方案 |
|--------|---------|
| BKL witness (A.6) Ch3 未讲 | Ch3 D4 新增 hypothesis 推理 |
| smp_schedule_sync (C.2) 全断裂 | Ch2 §2.5 补 file:line；Ch3 不涉及（DEFERRED）；Ch4 §4.7 诚实标注；Ch5 §5.2 待补充 |
| smp_sched_handler 完整语义 (C.7) | Ch2 §2.5 补完整语义；Ch4 §4.7 标注当前仅读 flags |
| smp_ipi_sched_handler (C.8) 全断裂 | Ch2 §2.5 补 file:line；Ch4 §4.7 DEFERRED |
| 跨 CPU 调度操作 (E.0-E.4) 全断裂 | Ch2 §2.5 补 4 函数；Ch4 §4.7 DEFERRED |
| arch 抽象 (F.0-F.4) 全断裂 | Ch1 §1.6 新增；Ch3 D7 新增；Ch4 §4.6 SmpArch trait |
| boot_lock (A.7) 全断裂 | Ch2 §2.2 提及；Ch4 §4.7 DEFERRED |
| 开发文档味（9 处） | 重写时全部删除迭代叙事/tmp 引用/P0-XX ID |
| 测试不可 grep | Ch5 §5.1 列出 18 个实际测试函数名 |

---

## 五、自检

- [x] Ch1 主语是 CPU/OS/矛盾，非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch2 每个符号带 file:line
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] Ch3 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] Ch4 贴真实代码，缺失函数诚实标注 DEFERRED + 理由
- [x] Ch5 测试函数可 grep 验证（`fn test_*`）
- [x] 知识点覆盖矩阵完整（A-F 六组）
- [x] 断裂修复表完整（9 处断裂 + 修复方案）
- [x] 无 tmp 文件引用
- [x] 无内部 review ID（P0-XX/P1-XX）
- [x] 无迭代叙事日期（2026-XX-XX）
- [x] 跨架构统一抽象（SmpArch trait）
- [x] anti-translate 体现（ProcNr 索引/Option/bitflags/BklSection witness）
