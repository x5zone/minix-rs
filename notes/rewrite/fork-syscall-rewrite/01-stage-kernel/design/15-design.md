# 15-clock-timer Design（设计文档）

> **状态**: 完整设计（基于 15-design-structure.md 经 outline-review 批准）
> **创建**: 2026-07-31
> **作者**: Trae (GLM-5.2)
> **前置**: 14-exception-interrupt.md, 05-clock-interrupt-init.md, 10-switch-to-user.md, 11-scheduling-primitives.md
> **C 源码**: `minix3/minix/kernel/clock.c`, `minix3/minix/kernel/system/do_vtimer.c`, `minix3/minix/kernel/system/do_setalarm.c`, `minix3/minix/kernel/arch/i386/arch_clock.c`, `minix3/minix/kernel/arch/earm/arch_clock.c`
> **Rust 实现**: `os/kernel/src/clock.rs`, `os/kernel/src/syscall_clock.rs`

---

## §1. 设计目标与约束

### 1.1 目标

重写 `os/kernel/src/clock.rs`，使其：
1. **对齐 C ground truth**：`timer_int_handler` (clock.c:70-173) + `vtimer_check` (do_vtimer.c:81-103) + `cause_alarm` (do_setalarm.c:69-76) 的完整语义
2. **修复 review 发现的 P0/P1**：billp 记账缺失、quantum 错误归层、BTreeMap 同 exp_time 覆盖、PerCpuTick SMP 不匹配等
3. **避免 translate**：用 Rust 类型系统重新表达 C 的指针语义（TimerId newtype）、函数指针（TimerAction enum）、BSP/AP 分支（per-CPU 实例）
4. **SMP 兼容**：per-CPU ClockState 实例，BSP 实例独占全局时间维护

### 1.2 约束

- `#![no_std]`（除 `#[cfg(test)]`）
- BKL 保护，无 interior mutability（除全局 atomic 镜像）
- 硬件抽象为 trait（`ClockArch`）
- 不引入 C 兼容层 / FFI
- 代码注释引用 C 源码 `file:line`

### 1.3 Ground Truth 验证

| C 函数 | 行号 | 职责 | Rust 归属 |
|--------|------|------|----------|
| `timer_int_handler` | clock.c:70-173 | 时间记账 + vtimer 递减 + 闹钟 + load_update | `ClockState::tick()` |
| `context_stop()` | arch_clock.c:208-349 (i386) | **quantum 递减（基于 TSC delta）** + cpuavg + p_cycles 记账 | `clock::decrement_quantum()`（kernel 侧，调用 `ClockArch::read_tsc()`） |
| `vtimer_check` | do_vtimer.c:81-103 | vtimer 到期检查 + 发信号 | tick 内 `tick_virt_timer`/`tick_prof_timer` 返回值 |
| `cause_alarm` | do_setalarm.c:69-76 | 闹钟到期通知 | `TimerAction::NotifyAlarm` |
| `init_clock` | clock.c:47-64 | 初始化时钟变量 | `ClockState::new()` / `with_hz()` |
| `boot_cpu_init_timer` | clock.c:294-304 | BSP 定时器初始化 + 注册 handler | 05-doc + 14-doc（不在本章） |
| `load_update` | clock.c:260-292 | 负载平均采样 | `ClockState::load_update()` |

**关键纠正**（来自 review）：
- `vtimer_check` 行号：68-89 → **81-103**（do_vtimer.c）
- `cause_alarm` 行号：73 → **69-76**（do_setalarm.c）
- `init_clock` 行号：47-63 → **47-64**（clock.c）
- quantum 递减归处：**不是 timer_int_handler，而是 `context_stop()`**（上下文切换路径，基于 TSC delta；arch_timer_int_handler 在 i386 为空函数）

---

## §2. 核心数据结构设计

### 2.1 ClockState（D1 — 保留）

```rust
/// Global clock state per CPU.
///
/// BSP instance owns global time (uptime/realtime/boottime) + alarm timer queue.
/// AP instance only tracks local load average.
///
/// C: clock.h — `struct clockinfo kclockinfo` + `struct loadinfo kloadinfo`
/// C: clock.c:37 — `static minix_timer_t *clock_timers` (BSP only)
#[derive(Debug)]
pub struct ClockState {
    /// CPU id this instance belongs to.
    cpu_id: CpuId,
    /// `true` iff this is the BSP instance (owns global time + timers).
    is_bsp: bool,
    /// Clock frequency in Hz. C: `kclockinfo.hz`
    hz: u32,
    /// Monotonically increasing ticks since boot. C: `kclockinfo.uptime` (BSP only)
    uptime: u64,
    /// Wall-clock ticks since boot (affected by adjtime). C: `kclockinfo.realtime` (BSP only)
    realtime: u64,
    /// UNIX epoch seconds at boot. C: `kclockinfo.boottime` (BSP only)
    boottime: u64,
    /// Time adjustment delta. C: `adjtime_delta` (clock.c:42, BSP only)
    adjtime_delta: i32,
    /// Synchronous alarm timer queue (BSP only).
    /// D2: BTreeSet<(exp_time, TimerId)> + BTreeMap<TimerId, TimerEntry>
    timers: TimerQueue,
    /// Load average info. C: `kloadinfo` (all CPUs)
    load_info: LoadInfo,
}
```

**与当前实现的差异**：
- 新增 `cpu_id` + `is_bsp` 字段（替代 `PerCpuTick` const）
- `timers` 类型从 `BTreeMap<u64, TimerEntry>` 改为 `TimerQueue`（D2）

### 2.2 TimerId newtype（D3 — 新增）

```rust
/// Stable identity for a timer, replacing C's `minix_timer_t *tp` pointer.
///
/// C uses the timer struct pointer as stable identity for set/reset operations.
/// Rust uses a newtype wrapping a u64 counter, allocated per-ClockState.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimerId(u64);

impl TimerId {
    pub const fn new(id: u64) -> Self { Self(id) }
    pub fn raw(self) -> u64 { self.0 }
}
```

**设计理由**：C 用 `minix_timer_t *tp` 指针作 stable identity（timer 结构体的地址不变）。Rust 不能用指针（数据结构不是链表节点），改用 newtype `TimerId(u64)` 替代指针语义，提供类型安全 + reset_timer(id) 支持。

### 2.3 TimerQueue（D2 — 重构）

```rust
/// Alarm timer queue with stable identity + sorted expiry.
///
/// Replaces C's `minix_timer_t *clock_timers` linked list.
/// D2: dual-index — BTreeSet<(exp_time, TimerId)> for sorted expiry scan
///     + BTreeMap<TimerId, TimerEntry> for O(log N) lookup by id.
///
/// Note: `HashMap` is NOT in `alloc::collections` (it requires `std` or the
/// external `hashbrown` crate). To keep "纯 alloc::collections，无外部依赖",
/// we use `BTreeMap` for `by_id`. O(log N) lookup is acceptable: the kernel
/// timer queue is small (typically < 64 entries, one per SYS_PROC alarm).
#[derive(Debug, Default)]
struct TimerQueue {
    /// Sorted by (exp_time, id) — supports O(k log N) expiry scan.
    by_expiry: BTreeSet<(u64, TimerId)>,
    /// Lookup by TimerId — supports O(log N) reset_timer(id).
    by_id: BTreeMap<TimerId, TimerEntry>,
    /// Next TimerId counter (per-ClockState).
    next_id: u64,
}

impl TimerQueue {
    fn insert(&mut self, entry: TimerEntry) -> TimerId {
        let id = TimerId(self.next_id);
        self.next_id += 1;
        self.by_expiry.insert((entry.exp_time, id));
        self.by_id.insert(id, entry);
        id
    }

    fn remove(&mut self, id: TimerId) -> Option<TimerEntry> {
        let entry = self.by_id.remove(&id)?;
        self.by_expiry.remove(&(entry.exp_time, id));
        Some(entry)
    }

    fn pop_expired(&mut self, now: u64) -> Option<TimerEntry> {
        loop {
            let first = self.by_expiry.iter().next().copied()?;
            if first.0 > now { return None; }
            self.by_expiry.remove(&first);
            return self.by_id.remove(&first.1);
        }
    }
}
```

**与当前 `BTreeMap<u64, TimerEntry>` 的差异**：
- 支持同 exp_time 多 timer（C 链表语义）
- `set_timer` 返回 `TimerId`，`reset_timer(id)` 用 id 取消（非 exp_time）
- `pop_expired` 按到期顺序弹出（替代 `check_expired_timers` 的固定数组）

### 2.4 TimerEntry + TimerAction（D6 — 保留 + 删变体）

```rust
#[derive(Debug, Clone)]
pub struct TimerEntry {
    pub exp_time: u64,
    pub action: TimerAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerAction {
    /// Notify a process (synchronous alarm).
    /// C: `cause_alarm()` → `mini_notify(CLOCK, endpoint)` — do_setalarm.c:69-76
    NotifyAlarm { endpoint: Endpoint },
    // KernelCallback variant DELETED (dead code, no callers)
}
```

**删除 `KernelCallback { id }` 的理由**：当前实现无任何调用方，违反 YAGNI。如未来需要内核定时器回调，再添加变体并接入。

---

## §3. 设计决策详表（D1-D11，多方案优中选优）

### D1: 全局时钟状态封装

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. `static mut` 全局变量 | 直接对应 C `kclockinfo` | translate 直观 | 违反 Rust 2024 安全模式；字段散落 |
| **B. `ClockState` struct** | 封装为 struct，BKL 保护下可变 | 封装性；字段聚簇；ownership 清晰 | — |

**选定 B**。理由：BKL 保护下无需 interior mutability；封装性优于全局变量；与 14-doc `IrqManager` 全局化模式一致。

### D2: 定时器队列数据结构

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. `BTreeMap<u64, TimerEntry>`（当前） | exp_time 作 key | O(log N) 插入 | **同 exp_time 多 timer 覆盖**（C 链表支持） |
| B. `BTreeMap<u64, Vec<TimerEntry>>` | exp_time → Vec | 支持同 exp_time | 删除需扫 Vec；Vec 堆分配碎片 |
| **C. `BTreeSet<(u64, TimerId)>` + `BTreeMap<TimerId, TimerEntry>`** | 二级索引 | stable identity + 排序 + 唯一 + 纯 alloc | 双索引维护；by_id O(log N) 非 O(1) |
| D. `BTreeMap<TimerId, TimerEntry>` | TimerId 作 key | stable identity | 查到期需 O(N) 扫全表 |
| E. `BTreeSet<(u64, TimerId)>` + `HashMap<TimerId, TimerEntry>` | 二级索引 + hash | by_id O(1) | **HashMap 不在 `alloc::collections`**；需引入 hashbrown 外部 crate |
| F. `SlotMap<TimerId, TimerEntry>` | SlotMap | O(1) + stable id | no_std 生态弱；引入外部 crate |

**选定 C**。理由：
1. BTreeSet 按 `(exp_time, id)` 排序，`pop_expired` O(k log N) 弹出 k 个到期 timer
2. BTreeMap 按 `TimerId` O(log N) 查找，`reset_timer(id)` 高效（timer 队列小，通常 < 64 条目，O(log N) 可接受）
3. 同 exp_time 多 timer 自然支持（id 不同）
4. 纯 `alloc::collections`，无外部依赖（**HashMap 不在 `alloc::collections`**，需 hashbrown crate，违反最小依赖）

**拒绝理由**：
- A：同 exp_time 覆盖是 P1 设计缺陷（test_multiple_timers_same_time 已暴露）
- B：Vec 删除需 O(k) 扫描，且每次到期都要处理 Vec
- D：查到期 O(N) 不可接受（C 链表是 O(k)）
- E：HashMap 需引入 hashbrown 外部 crate，违反最小依赖原则（kernel Cargo.toml 无 hashbrown 依赖）
- F：no_std 环境 SlotMap 需引入外部 crate，违反最小依赖原则

### D3: 定时器 identity

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. exp_time 作 key（当前） | C `minix_timer_t *tp` 的 translate 误用 | 简单 | **同 exp_time 无法区分**；reset_timer(exp_time) 误取消其他 timer |
| **B. `TimerId(u64)` newtype** | stable identity | 类型安全；支持 reset(id) | 需生成 id（per-ClockState 计数器） |

**选定 B**。理由：C 用 timer 指针作 stable identity（结构体地址不变），Rust 用 newtype 替代指针语义。`TimerId` 是 per-ClockState 单调递增计数器，无需全局同步（BKL 保护）。

### D4: adjtime 机制

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| **A. 保留** | `adjtime_delta` + `uptime & 0x1` 隔 tick 调整 | NTP 兼容；与 C 对齐 | — |
| B. 删除 | — | 简化 | 丢失 NTP 支持；违反 "rewrite 保留外部行为" |

**选定 A**。理由：adjtime 是 SYS_SETTIME 系统调用的核心语义（21-doc D4），删除会破坏 POSIX 兼容。

### D5: 负载平均

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| **A. 保留** | `LoadInfo` + circular buffer（`proc_load_history[12]`） | 与 C 对齐；支持 `getloadavg(3)` | — |
| B. 简化 | 仅就绪队列计数 | 简化 | 丢失历史；破坏 `getloadavg` |

**选定 A**。理由：负载历史是 `do_times` 系统调用返回的核心数据，简化会破坏用户态 `getloadavg(3)`。

### D6: `cause_alarm` 回调

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. 函数指针 `tmr_func_t` | C 原始 | — | unsafe；类型不安全；no_std 不友好 |
| **B. `TimerAction` enum** | `NotifyAlarm { endpoint }` | 类型安全；enum 分发 | — |
| C. trait object `Box<dyn TimerHandler>` | 动态分发 | 扩展性 | 堆分配；no_std 不友好；过度抽象 |

**选定 B**（保留），但**删除 `KernelCallback { id }` 变体**。

**删除 KernelCallback 理由**：
- 当前实现无任何调用方（grep 验证）
- 违反 YAGNI 原则
- 如未来需要内核定时器回调（如 watchdog），再添加变体并接入

### D7: `hz` 配置

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. 运行时环境变量 | C 原始（`env_get("hz")`） | 灵活 | 运行时解析；`env_get` 不在 no_std |
| **B. 编译时常量 + 运行时可覆盖** | `DEFAULT_HZ` + `with_hz()` | 编译期优化 + 灵活 | — |

**选定 B**（保留）。理由：编译时常量 `DEFAULT_HZ=100` 满足默认场景；`with_hz(hz)` 支持 2..=50000 范围覆盖（与 C 的 `env_get("hz")` 等价）。

### D8: BSP/AP 分支

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. 运行时 `cpu_is_bsp()` 分支 | C 原始 | SMP 兼容 | 每次中断有分支 |
| B. `PerCpuTick` const IS_BSP（当前） | 编译期单态化 | 零分支 | **SMP 单镜像无效**（BSP/AP 共用 binary，编译期无法区分） |
| **C. per-CPU `ClockState` 实例** | BSP 实例有 timers，AP 无 | SMP 兼容 + 数据分离 + 类型安全 | 内存开销（可接受，每实例 < 200B） |
| D. Typestate + 运行时 downcast | 类型安全 + SMP | 复杂；downcast 开销 | — |

**选定 C**。理由：
1. SMP 单镜像模型下，BSP/AP 在运行时确定（同一个 binary 在所有 CPU 上运行），编译期 const 无法区分
2. per-CPU 实例天然支持 SMP：每个 CPU 有自己的 `ClockState`，BSP 实例的 `is_bsp=true` 拥有 `timers`，AP 实例 `is_bsp=false` 的 `timers` 为空
3. 数据局部性：per-CPU 实例缓存友好
4. 类型安全：`is_bsp` 字段运行时判断，但数据结构层面 AP 实例的 `timers` 始终为空（不变量）

**与 PerCpuTick 的兼容性**：保留 `tick_bsp` / `tick_ap` 便捷方法，但内部改为 `if self.is_bsp { ... }` 运行时分支（替代 const 单态化）。分支开销可接受（每次中断一次分支预测命中）。

### D9: tick() quantum 职责

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. tick 内 `quantum.consume(1)`（当前） | — | — | **违反 C 语义**：quantum 不在 `timer_int_handler`；且用固定 1 而非 TSC delta |
| B. 移除 quantum，归 switch_to_user | 与 10-doc 对齐 | — | **错误**：switch_to_user 只检查 `!p_cpu_time_left`，不递减（proc.c:421-422） |
| **C. 移除 quantum，归上下文切换路径** | 与 C `context_stop()` 对齐 | 正确性；基于 TSC delta | 需 kernel 函数调用 `ClockArch::read_tsc()`（避免循环依赖） |

**选定 C**。理由（**修正 review 骨架的错误归处**）：
- C ground truth：`context_stop()` (arch_clock.c:326-330) 基于 `tsc_delta` 递减 `p_cpu_time_left`（在上下文切换路径调用，非时钟中断）
- 14-doc §1.2 verified："timer_int_handler 不递减 quantum | clock.c:70-173 无 p_cpu_time_left"
- 10-doc §2.1 阶段 4：switch_to_user 只**检查** `!p_cpu_time_left`（proc.c:421-422），不递减
- 因此 quantum 递减归 `clock::decrement_quantum()`（kernel 侧，调用 `ClockArch::read_tsc()` 获取 TSC delta），基于 TSC delta

**实现方式调整**：原设计在 `ClockArch` trait 上加 `arch_tick(&mut KProcess, ...)` 方法，但 `KProcess` 在 `minix-kernel` crate，`ClockArch` 在 `minix-arch` crate，会造成循环依赖。改为：在 `minix-kernel` 中实现 `clock::decrement_quantum()` 函数，调用 `ClockArch::read_tsc()` 获取 TSC delta，再应用到 `current_proc`。详见 §4.5。

**实现**：
```rust
// ClockArch trait 不变，仅提供 read_tsc()（os/arch/src/arch/clock.rs）
pub trait ClockArch: Sized + Send + Sync {
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self;
    fn init_timer(&mut self, hz: u32);
    fn read_ticks(&self) -> u64;
    fn read_tsc(&self) -> u64 { self.read_ticks() }
}

// ClockState::tick() 不再调用 quantum.consume
// quantum 递减由 clock::decrement_quantum() 完成（三层架构，详见 §4.5）
//   L1: decrement_quantum(proc) — 生产入口，读硬件 TSC + 全局 SMP_STATE
//   L2: decrement_quantum_with_tsc(proc, tsc) — 测试入口，注入 TSC
//   L3: decrement_quantum_in(smp, proc, tsc) — 核心逻辑，注入 &mut SmpState
```

**拒绝理由**：
- A：违反 C 语义 + 用 1 而非 TSC delta（P0-2）
- B：归处错误（switch_to_user 只检查不递减）

### D10: billp 记账接口

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. `_is_billable: bool` 未使用（当前） | — | — | **缺失 C 关键行为**（billp->p_sys_time + billp->p_prof_left + vtimer_check(billp)） |
| **B. 显式 `billp: Option<&mut KProcess>` 参数** | 调用方传入 billp | 显式；对齐 C `get_cpulocal_var(bill_ptr)` | 需调用方查找 billp |
| C. 传 `&mut ProcessTable` | 内部查找 billp | 接口简单 | 过宽；隐藏依赖；borrow 冲突（current_proc + billp 同表） |
| D. 封装 `AccountingCtx` | billp 引用包装 | 可扩展 | 过度封装；YAGNI |

**选定 B**。理由：
1. 显式传 billp 与 C `get_cpulocal_var(bill_ptr)` 语义对齐
2. 调用方（trap 入口）已知 billp（per-CPU `bill_ptr` 变量）
3. `Option<&mut KProcess>` 处理 `is_billable=true` 时 billp=None（无需记账）
4. 避免 `&mut ProcessTable` 的 borrow 冲突（current_proc 和 billp 可能同表）

**实现**：
```rust
pub fn tick(
    &mut self,
    current_proc: &mut KProcess,
    billp: Option<&mut KProcess>,  // None if current_proc is billable
    ready_count: usize,
) -> TimerTickResult {
    // ...
    current_proc.p_time.add_user_time(1);
    if let Some(billp) = billp {
        // C: clock.c:118-120 — !BILLABLE → billp->p_sys_time++
        billp.p_time.add_sys_time(1);
        // C: clock.c:134-138 — !BILLABLE → billp->p_prof_left--
        if billp.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
            let expired = billp.p_time.tick_prof_timer();
            if expired && vtimer_expired.is_none() {
                vtimer_expired = Some(VtimerExpired::Prof);
            }
        }
    }
    // ...
}
```

### D11: vtimer_check 函数去留

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. 保留独立 `vtimer_check` 函数（当前） | — | — | 与 tick 内 `tick_virt_timer`/`tick_prof_timer` 逻辑重复 |
| **B. 删除独立函数** | tick 内已递减+检查 | 无重复 | 进程退出清理需新函数 |
| C. `vtimer_check` 改为 `vtimer_cleanup_on_exit` | 退出时用 | 语义清晰 | — |

**选定 B + C 组合**。理由：
1. 删除当前 `vtimer_check`（clock.rs:706-720）：与 tick 内 `tick_virt_timer`/`tick_prof_timer` 返回 bool 重复
2. tick 内通过 `tick_virt_timer()` / `tick_prof_timer()` 返回 `expired: bool`，直接设置 `vtimer_expired`
3. 如需进程退出时清理 vtimer 标志，新增 `vtimer_cleanup_on_exit(proc)` 函数（当前无此需求，YAGNI）

---

## §4. 实现要点

### 4.1 ClockState::tick() 重构后签名

```rust
impl ClockState {
    /// Handle a timer interrupt tick.
    ///
    /// C: `timer_int_handler()` — clock.c:70-173
    ///
    /// # BKL Precondition
    /// Caller must hold BKL.
    ///
    /// # Arguments
    /// * `current_proc` — currently running process (for user time accounting)
    /// * `billp` — billable process if current is not billable (None if billable)
    /// * `ready_count` — number of processes in ready queues (for load average)
    ///
    /// # Returns
    /// `TimerTickResult` containing expired alarms + vtimer status.
    /// **Does NOT contain quantum_exhausted** — quantum decrement is in
    /// `clock::decrement_quantum()` (D9), called separately by the trap entry.
    pub fn tick(
        &mut self,
        current_proc: &mut KProcess,
        billp: Option<&mut KProcess>,
        ready_count: usize,
    ) -> TimerTickResult {
        // 1. BSP-only: update uptime and realtime (with adjtime)
        //    C: clock.c:91-104
        if self.is_bsp {
            self.uptime += 1;
            CLOCK_UPTIME.store(self.uptime, Ordering::Release);
            if self.adjtime_delta != 0 && (self.uptime & 0x1) != 0 {
                self.realtime += if self.adjtime_delta > 0 { 2 } else { 0 };
                self.adjtime_delta += if self.adjtime_delta > 0 { -1 } else { 1 };
            } else {
                self.realtime += 1;
            }
            CLOCK_REALTIME.store(self.realtime, Ordering::Release);
        }

        // 2. Time accounting: charge current process for user time
        //    C: clock.c:116
        current_proc.p_time.add_user_time(1);

        // 3. Decrement virtual/profile timers for current_proc
        //    C: clock.c:128-133
        let mut vtimer_expired = None;
        if current_proc.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER) {
            let expired = current_proc.p_time.tick_virt_timer();
            if expired { vtimer_expired = Some(VtimerExpired::Virtual); }
        }
        if current_proc.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
            let expired = current_proc.p_time.tick_prof_timer();
            if expired && vtimer_expired.is_none() {
                vtimer_expired = Some(VtimerExpired::Prof);
            }
        }

        // 4. Billable process accounting (if current is not billable)
        //    C: clock.c:118-120, 134-138, 147-148
        if let Some(billp) = billp {
            billp.p_time.add_sys_time(1);
            if billp.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
                let expired = billp.p_time.tick_prof_timer();
                if expired && vtimer_expired.is_none() {
                    vtimer_expired = Some(VtimerExpired::Prof);
                }
            }
        }

        // 5. BSP-only: check alarm timers
        //    C: clock.c:153-161
        let expired_alarms = if self.is_bsp {
            self.collect_expired_timers()
        } else {
            Vec::new()
        };

        // 6. Load update (all CPUs)
        //    C: clock.c:151
        self.load_update(ready_count);

        // NOTE: quantum decrement is NOT here — it's in clock::decrement_quantum() (D9).

        TimerTickResult { expired_alarms, vtimer_expired }
    }
}
```

### 4.2 TimerTickResult 重构

```rust
#[derive(Debug, Default)]
pub struct TimerTickResult {
    /// Actions from expired alarm timers (BSP only).
    /// Changed from fixed array to Vec — no artificial MAX_EXPIRED_TIMERS limit.
    pub expired_alarms: Vec<TimerAction>,
    /// Virtual/profile timer expiry for current or billable process, if any.
    pub vtimer_expired: Option<VtimerExpired>,
    // REMOVED: quantum_exhausted — quantum is now in clock::decrement_quantum() (D9)
}
```

### 4.3 set_timer / reset_timer 新接口

```rust
impl ClockState {
    /// Set a kernel timer. Returns TimerId for later reset.
    /// C: `set_kernel_timer()` — clock.c:229-240
    pub fn set_timer(&mut self, entry: TimerEntry) -> TimerId {
        if !self.is_bsp {
            panic!("set_timer called on AP ClockState (timers are BSP-only)");
        }
        self.timers.insert(entry)
    }

    /// Reset (remove) a kernel timer by TimerId.
    /// C: `reset_kernel_timer()` — clock.c:245-255
    pub fn reset_timer(&mut self, id: TimerId) -> Option<TimerEntry> {
        if !self.is_bsp { return None; }
        self.timers.remove(id)
    }
}
```

### 4.4 syscall_clock.rs 调用方适配

```rust
// dispatch_setalarm 改动：
// 旧：clock_state.set_timer(timer.clone()); kpriv.s_alarm_timer = Some(timer);
// 新：let id = clock_state.set_timer(timer.clone());
//     kpriv.s_alarm_timer = Some((timer, id));  // 存储 (entry, TimerId)

// 旧：clock_state.reset_timer(old_timer.exp_time);
// 新：clock_state.reset_timer(old_id);
```

**KPriv 字段改动**：`s_alarm_timer: Option<TimerEntry>` → `s_alarm_timer: Option<(TimerEntry, TimerId)>`。

### 4.5 ClockArch trait + decrement_quantum 三层架构（D9）

**原设计**：在 `ClockArch` trait 上加 `arch_tick(&mut KProcess, ...)` 方法。

**问题**：`KProcess` 在 `minix-kernel` crate，`ClockArch` 在 `minix-arch` crate，会造成循环依赖。

**实际实现**：`ClockArch` 仅提供 `read_tsc()`；quantum 递减由 `minix-kernel` 中的 `clock::decrement_quantum()` 函数完成，分三层：

```rust
// os/arch/src/arch/clock.rs — ClockArch trait（不变）
pub trait ClockArch: Sized + Send + Sync {
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self;
    fn init_timer(&mut self, hz: u32);
    fn read_ticks(&self) -> u64;
    /// C: `read_tsc_64()`. Default delegates to `read_ticks()`.
    /// Used by kernel's `clock::decrement_quantum()` to compute TSC delta.
    fn read_tsc(&self) -> u64 { self.read_ticks() }
}

// os/kernel/src/clock.rs — 三层拆分
//
// L1: 生产入口。读硬件 TSC + 全局 SMP_STATE。
pub fn decrement_quantum(current_proc: &mut KProcess) -> bool {
    decrement_quantum_with_tsc(current_proc, read_tsc())
}

// L2: 测试入口。注入 TSC（test build 中 read_tsc() 返回 0），仍用全局 SMP_STATE。
pub(crate) fn decrement_quantum_with_tsc(
    current_proc: &mut KProcess,
    current_tsc: u64,
) -> bool {
    unsafe {
        match crate::try_smp_state() {
            Some(smp) => decrement_quantum_in(smp, current_proc, current_tsc),
            None => false,
        }
    }
}

// L3: 核心逻辑。注入 &mut SmpState，避免并行测试对全局 SMP_STATE 的 UB。
fn decrement_quantum_in(
    smp: &mut crate::smp::SmpState,
    current_proc: &mut KProcess,
    current_tsc: u64,
) -> bool {
    // 1. 更新 tsc_ctr_switch 基线（C: arch_clock.c:342，对所有进程）
    // 2. 首次调用 last_tsc=0 → 建立基线，返回 false
    // 3. delta = current_tsc - last_tsc（饱和）
    // 4. C: arch_clock.c:314 — 跳过 endpoint < 0 的 kernel/idle 任务
    // 5. C: arch_clock.c:326-330 — quantum.consume(delta)（饱和递减）
    //    Quantum::consume 通过 CAS 实现，返回 true 表示耗尽
    ...
}
```

**三层拆分的动机**：

| 层 | 职责 | 依赖 | 测试场景 |
|----|------|------|---------|
| `decrement_quantum` | 生产入口 | 硬件 TSC + 全局 SMP_STATE | QEMU 集成测试 |
| `decrement_quantum_with_tsc` | 注入 TSC | 全局 SMP_STATE | 验证 "无 SMP_STATE" 早期 boot 路径 |
| `decrement_quantum_in` | 核心逻辑 | 注入 `&mut SmpState` | 单元测试（8 个测试覆盖所有路径） |

**与原设计的偏差说明**：原设计的 `arch_tick()` 方法被替换为 kernel 函数 `decrement_quantum()`。这保留了 D9 的核心不变量（quantum 不在 `ClockState::tick()`），同时避免循环依赖并提升可测试性。Rust 通过参数化 `&mut SmpState` 使核心逻辑可在不触碰全局状态的情况下测试，这是 C 难以实现的设计改进。

---

## §5. 测试策略

### 5.1 L1 对偶测试（C-Rust 行为一致）

| 测试名 | C 行为 | Rust 断言 |
|--------|--------|----------|
| `test_tick_bsp_increments_uptime_realtime` | clock.c:92-102 | uptime+1, realtime+1 |
| `test_tick_ap_no_uptime_update` | clock.c:91 `if (cpu_is_bsp)` | AP uptime 不变 |
| `test_adjtime_speed_up` | clock.c:97-100 奇数 tick realtime+=2 | delta=3 → tick1: rt=2, delta=2 |
| `test_adjtime_slow_down` | clock.c:99 奇数 tick realtime+=0 | delta=-2 → tick1: rt=0, delta=-1 |
| `test_billp_sys_time_accounting` | clock.c:118-120 !BILLABLE | billp.sys_time+1 |
| `test_billp_prof_timer_decrement` | clock.c:134-138 !BILLABLE | billp.prof_left 递减 |
| `test_vtimer_virtual_expiry` | do_vtimer.c:91-95 | VIRT_TIMER + virt_left=0 → Virtual |
| `test_vtimer_prof_expiry` | do_vtimer.c:98-102 | PROF_TIMER + prof_left=0 → Prof |
| `test_timer_set_and_expire` | clock.c:159-161 tmrs_exptimers | set_timer(t=3), tick 3 → expired |
| `test_timer_reset_by_id` | clock.c:245-255 reset_kernel_timer | set_timer → reset_timer(id) → 不触发 |

### 5.2 L2 契约测试（trait/类型契约）

| 测试名 | 契约 |
|--------|------|
| `test_timer_id_uniqueness` | 同一 ClockState 内 set_timer 多次返回不同 TimerId |
| `test_timer_queue_same_exp_time` | 同 exp_time 两个 timer 都能到期触发（修复 P1-4） |
| `test_bsp_state_has_timers` | BSP 实例 set_timer 成功 |
| `test_ap_state_no_timers` | AP 实例 set_timer panic / reset_timer 返回 None |
| `test_load_update_slot_rotation` | uptime 推进 → slot 切换 → history 清零 |

### 5.3 边界用例

| 测试名 | 边界 |
|--------|------|
| `test_hz_bounds` | hz=1 / hz=50001 → fallback DEFAULT_HZ |
| `test_adjtime_zero_delta` | delta=0 → realtime 每tick+1 |
| `test_timer_never_expires` | exp_time=u64::MAX → 不触发 |
| `test_vtimer_check_no_flag` | virt_left=0 但 MF_VIRT_TIMER 未设 → None |
| `test_multiple_timers_pop_order` | 3 个 timer 不同 exp_time → 按顺序弹出 |

---

## §6. 与其他模块的关系

| 文档 | 关系 | 双向链路状态 |
|------|------|------------|
| [05-clock-interrupt-init](../05-clock-interrupt-init.md) | 05 讲 `init_clock()` 初始化，15 讲运行时 tick | **待补** 15 §7 加 05；05 §7 加 15 |
| [10-switch-to-user](../10-switch-to-user.md) | 10 讲 quantum 检查（`!p_cpu_time_left`），15 讲 quantum 递减归 `clock::decrement_quantum()` | **待协调** 10 §2.1 阶段 4 已正确（只检查不递减） |
| [11-scheduling-primitives](../11-scheduling-primitives.md) | 11 讲调度原语，15 讲时钟如何触发调度 | **待补** 11 §7 加 15 |
| [14-exception-interrupt](../14-exception-interrupt.md) | 14 把时钟中断定位为"三条激活路径之一"，15 承接进入+返回 | **待补** 15 §1 ← 14 §1.2 |
| [16-smp](../16-smp.md) | 16 讲 AP 定时器初始化，15 讲 BSP/AP 职责分离 | ✅ 已双向 |
| [21-syscall-clock](../21-syscall-clock.md) | 21 讲时钟系统调用，15 讲内核实现 | ✅ 已双向；**待同步** set_timer/reset_timer 接口变更 |

---

## §7. 本章不讲什么（预期管理）

- **硬件定时器配置**（8254 PIT / LAPIC Timer / ARM Generic Timer 初始化）→ 归 05-clock-interrupt-init.md + 14-exception-interrupt.md
- **quantum 递减的具体 TSC delta 计算** → 归 `clock::decrement_quantum()`（kernel crate，调用 `ClockArch::read_tsc()`），本章 §4.9.1 已实现
- **时钟系统调用的消息解析**（SYS_SETALARM/SYS_VTIMER/SYS_STIME/SYS_SETTIME/SYS_TIMES）→ 归 21-syscall-clock.md
- **AP 启动时的定时器注册** → 归 16-smp.md
- **TSC 校准细节** → 归 04-platform-discovery.md（`platform_desc.timer()`）
- **quantum 耗尽后的调度决策**（`proc_no_time` / `RTS_NO_QUANTUM` 设置）→ 归 10-switch-to-user.md + 11-scheduling-primitives.md

---

## §8. 重写对齐清单（来自 review scan）

| P0/P1 | 修复项 | 对应决策 | 状态 |
|--------|--------|---------|------|
| P0-1 | §1.4 行为规则 3 quantum 归因错误 | D9 | ✅ 本设计修正 |
| P0-2 | tick() quantum.consume 加料 | D9 | ✅ 移除，归 ClockArch |
| P0-3 | billp->p_sys_time 缺失 | D10 | ✅ 显式 billp 参数 |
| P0-4 | billp vtimer 缺失 | D10 | ✅ billp tick_prof_timer |
| P0-5 | p_time_stats vs p_time | 实现对齐 | ✅ 用 p_time |
| P0-6 | vtimer_check 行号 68-89 → 81-103 | 文档修正 | ✅ §1.3 已修正 |
| P0-7 | §6 引用 tmp-16-timer.md | 结构整合 | ✅ 重写文档时整合到 §1/§2 |
| P0-8 | 无 design.md | 本文档 | ✅ |
| P1-1 | clock.rs 注释 14→15 | 实现修正 | ✅ 重构时修正 |
| P1-2 | cause_alarm 行号 73 → 69-76 | 文档修正 | ✅ §1.3 已修正 |
| P1-3 | §7 缺 05 | 双向链路 | ✅ §6 已列 |
| P1-4 | BTreeMap 同 exp_time 覆盖 | D2 | ✅ TimerQueue 双索引 |
| P1-5 | set/reset_timer 用 exp_time | D3 | ✅ 用 TimerId |
| P1-6 | KernelCallback 死代码 | D6 | ✅ 删除变体 |
| P1-7 | PerCpuTick SMP 不匹配 | D8 | ✅ per-CPU 实例 |
| P1-8 | vtimer_check 冗余 | D11 | ✅ 删除独立函数 |
| P1-9 | test 无 assert | 测试补充 | ✅ §5 测试策略 |
| P1-10 | D1-D8 无方案对比 | 本文档 | ✅ §3 多方案对比 |

---

## §9. Self-Review Issues（设计自我审查）

### §9.1 D9 修正记录

**问题**：review 骨架（15-design-structure.md）D9 推荐"移除 quantum，归 switch_to_user"，但验证 C 源码后发现：
- `switch_to_user` (proc.c:421-422) 只**检查** `!p_cpu_time_left`，不递减
- `context_stop()` (arch_clock.c:326-330) 基于 `tsc_delta` 递减 `p_cpu_time_left`（上下文切换路径，proc.c:208/440/1956 调用）

**修正**：D9 选定方案改为 C（归架构层），非 B（归 switch_to_user）。原设计为 `ClockArch::arch_tick()` 方法，后因循环依赖改为 `clock::decrement_quantum()` 函数（见 §4.5 偏差说明）。本设计 §1.3 + §3 D9 + §4.5 已修正。

### §9.2 D8 per-CPU 实例的过渡方案

**问题**：当前 SMP 尚未完全实现（16-smp.md 在开发中），per-CPU ClockState 实例需要 SMP 框架支持。

**过渡方案**：在 SMP 框架就绪前，保留单一全局 `ClockState` 实例（`is_bsp=true`），通过 `tick_bsp` / `tick_ap` 便捷方法区分。SMP 就绪后迁移到 per-CPU 实例。

**标记**：这是**架构演进**（ARCH），需在文档中标注。

### §9.3 TimerId 持久性

**问题**：`TimerId` 是 per-ClockState 计数器，如果 ClockState 重建（如 live update），TimerId 会重置。

**缓解**：ClockState 在 boot 后不重建（BKL 保护的全局状态）。Live update 场景由 16-smp.md / 24-cross-space-runtime.md 处理，不在本章范围。

### §9.4 KPriv.s_alarm_timer 字段变更影响

**问题**：`s_alarm_timer: Option<TimerEntry>` → `Option<(TimerEntry, TimerId)>` 影响 KPriv 序列化/调试。

**缓解**：
- KPriv 不跨重启序列化（boot 时重建）
- Debug impl 自动派生（tuple 实现 Debug）

### §9.5 borrow 冲突风险

**问题**：`tick(current_proc: &mut KProcess, billp: Option<&mut KProcess>, ...)` 当 current_proc 和 billp 来自同一 ProcessTable 时，两个 `&mut` 可能冲突。

**缓解**：
- 调用方从 ProcessTable 取出两个不同 slot 的 `&mut`（current 和 billable 是不同进程）
- Rust borrow checker 在编译期验证不别名
- 如确实同进程（current is billable），传 `billp=None`

---

## §10. 实施顺序

1. **clock.rs 重构**（按 D1-D11）
   - 新增 `TimerId` newtype + `TimerQueue`
   - `ClockState` 加 `cpu_id` / `is_bsp` 字段
   - `tick()` 重构（加 billp 参数，移除 quantum）
   - `set_timer` / `reset_timer` 改用 TimerId
   - 删除 `PerCpuTick` / `vtimer_check` / `KernelCallback` / `MAX_EXPIRED_TIMERS`
2. **syscall_clock.rs 适配**
   - `dispatch_setalarm` 用新 set_timer/reset_timer 接口
   - KPriv.s_alarm_timer 字段变更
3. **ClockArch trait + clock::decrement_quantum**（os/arch/src/arch/clock.rs + os/kernel/src/clock.rs）
   - `ClockArch` 提供 `read_tsc()`（已实现）
   - kernel 侧 `decrement_quantum()` 三层拆分（已实现，见 §4.5）
4. **测试补充**（按 §5 测试策略）
5. **文档重写**（15-clock-timer.md，基于本设计）
6. **跨文档同步**（05/10/11/14/16/21 双向链路）
