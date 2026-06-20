# 15-clock-timer: 时钟中断与定时器

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/clock.c`
> **说明**: 100Hz 时钟中断驱动调度、量子管理、同步闹钟、虚拟/性能定时器
> **前置**: 14-exception-interrupt.md
> **创建**: 2026-06-13

---

## 1. 概述

### 1.1 概念定义

时钟中断是内核调度的驱动力。Minix3 使用 100Hz（可配置 2~50000）的周期性定时器中断，每次中断执行以下工作：

1. **时间记账**：递增 `uptime`/`realtime`，更新进程的用户/系统时间
2. **量子管理**：递减当前进程的 `p_cpu_time_left`，耗尽时设置 `RTS_NO_QUANTUM`
3. **定时器检查**：检查同步闹钟（`s_alarm_timer`）和虚拟/性能定时器是否到期
4. **负载平均**：统计就绪队列进程数，更新 `kloadinfo`

### 1.2 与 Minix3 的对应关系

| Minix3 概念 | C 源码位置 | 说明 |
|-------------|-----------|------|
| `timer_int_handler()` | clock.c:70 | 时钟中断处理主函数 |
| `kclockinfo` | clock.h | 全局时钟状态（hz, uptime, realtime, boottime） |
| `clock_timers` | clock.c:37 | 同步闹钟定时器队列 |
| `init_clock()` | clock.c:47 | 初始化时钟变量 |
| `boot_cpu_init_timer()` | clock.c:294 | BSP 定时器初始化+注册 handler |
| `app_cpu_init_timer()` | clock.c:306 | AP 定时器初始化（无 handler 注册） |
| `set_kernel_timer()` | clock.c:229 | 设置内核定时器 |
| `reset_kernel_timer()` | clock.c:245 | 重置内核定时器 |
| `load_update()` | clock.c:260 | 负载平均更新 |
| `vtimer_check()` | do_vtimer.c:68 | 虚拟/性能定时器到期检查 |

### 1.3 关键状态/机制说明

**全局时钟状态** (`kclockinfo`)：
- `hz`：时钟频率（默认 100Hz）
- `uptime`：单调递增的启动后滴答数（每 tick +1）
- `realtime`：墙上时钟滴答数（受 adjtime 影响）
- `boottime`：UNIX epoch 到启动的秒数

**同步闹钟** (`s_alarm_timer`)：
- 每个 `struct priv` 有一个 `s_alarm_timer`
- 到期时通过 `cause_alarm()` → `mini_notify(CLOCK, target)` 通知目标进程
- 仅系统进程（`SYS_PROC`）可使用

**虚拟/性能定时器**：
- `p_virt_left`：用户态虚拟定时器（仅用户态时间递减）
- `p_prof_left`：性能分析定时器（用户态+系统态都递减）
- 到期时通过 `cause_sig()` 发送 `SIGVTALRM`/`SIGPROF`
- 通过 `MF_VIRT_TIMER`/`MF_PROF_TIMER` 标志启用/禁用

**adjtime 机制**：
- `adjtime_delta`：时间调整增量（正=加速，负=减速）
- 每隔一个 tick 调整：`realtime += (delta > 0) ? 2 : 0`
- 用于 NTP 等时间同步场景

### 1.4 行为规则

1. **BSP 独占**：`uptime`/`realtime`/`clock_timers`/`load_update` 仅 BSP 更新
2. **记账规则**：当前进程计用户时间；非 BILLABLE 进程的用户时间计为 billable 进程的系统时间
3. **量子耗尽**：`p_cpu_time_left` 递减到 0 时设置 `RTS_NO_QUANTUM`，通知调度器
4. **定时器到期**：`tmrs_exptimers()` 遍历 `clock_timers` 队列，调用每个到期定时器的 watchdog

---

## 2. C 源码分析

### 2.1 相关定义

| 常量/宏 | 值 | 源码位置 | 说明 |
|---------|-----|---------|------|
| `DEFAULT_HZ` | 100 | clock.h | 默认时钟频率 |
| `TMR_NEVER` | `LONG_MAX` | timers.h | 定时器永不触发 |
| `_LOAD_UNIT_SECS` | 5 | clock.h | 负载采样间隔（秒） |
| `_LOAD_HISTORY` | 12 | clock.h | 负载历史槽数 |
| `VT_VIRTUAL` | 0 | signal.h | 虚拟定时器类型 |
| `VT_PROF` | 1 | signal.h | 性能分析定时器类型 |
| `MF_VIRT_TIMER` | 0x002 | proc.h | 虚拟定时器活跃标志 |
| `MF_PROF_TIMER` | 0x004 | proc.h | 性能分析定时器活跃标志 |

### 2.2 核心数据结构

**kclockinfo**（clock.h）：
```c
struct clockinfo {
    clock_t hz;           // 时钟频率
    clock_t realtime;     // 墙上时钟（滴答数）
    clock_t uptime;       // 单调时钟（滴答数）
    time_t boottime;      // 启动时的 UNIX 时间戳
};
```

**kloadinfo**（clock.h）：
```c
struct loadinfo {
    u16_t proc_last_slot;              // 当前负载采样槽位
    u32_t proc_load_history[_LOAD_HISTORY]; // 负载历史
    clock_t last_clock;                // 上次更新时间
};
```

**minix_timer_t**（timers.h）：
```c
typedef struct minix_timer {
    struct minix_timer *tmr_next;  // 链表下一节点
    clock_t tmr_exp_time;          // 到期时间（单调滴答）
    tmr_func_t tmr_func;           // 到期回调函数
    int tmr_arg;                   // 回调参数（进程端点）
} minix_timer_t;
```

### 2.3 关键函数分析

#### `init_clock()` — clock.c:47-63

初始化时钟变量：清零 `kclockinfo`，从环境变量读取 `hz`（范围 2~50000，默认 100），清零 `kloadinfo`。

#### `timer_int_handler()` — clock.c:70-175

时钟中断主处理函数，每次 tick 调用：

1. BSP 递增 `uptime` 和 `realtime`（含 adjtime 调整）
2. 更新进程时间记账：`p->p_user_time++`，非 BILLABLE 进程的 `billp->p_sys_time++`
3. 递减虚拟/性能定时器：`p_virt_left--`/`p_prof_left--`
4. 调用 `vtimer_check()` 检查定时器到期
5. 调用 `load_update()` 更新负载平均
6. BSP 检查 `clock_timers` 是否有到期定时器，调用 `tmrs_exptimers()`
7. 调用 `arch_timer_int_handler()` 处理架构相关定时器操作

#### `boot_cpu_init_timer()` — clock.c:294-302

BSP 初始化本地定时器并注册 `timer_int_handler` 为中断处理函数。

#### `set_kernel_timer()` / `reset_kernel_timer()` — clock.c:229-257

设置/重置内核定时器。插入/移除 `clock_timers` 队列。

#### `load_update()` — clock.c:260-291

统计就绪队列进程数，按 `_LOAD_UNIT_SECS` 秒间隔采样到 `proc_load_history[]`。

### 2.4 调用关系/调用点分析

```
硬件定时器中断
  → IDT/LAPIC
  → timer_int_handler()
    → kclockinfo.uptime++ / realtime++
    → p_user_time++ / billp->p_sys_time++
    → p_virt_left-- / p_prof_left--
    → vtimer_check()
      → cause_sig(SIGVTALRM / SIGPROF)
    → load_update()
    → tmrs_exptimers(&clock_timers)
      → cause_alarm(endpoint)
        → mini_notify(CLOCK, endpoint)
    → arch_timer_int_handler()

do_setalarm()
  → set_kernel_timer(tp, exp_time, cause_alarm, endpoint)
  → reset_kernel_timer(tp)

do_vtimer()
  → 设置/清除 MF_VIRT_TIMER / MF_PROF_TIMER
  → 设置 p_virt_left / p_prof_left

do_stime()
  → set_boottime()

do_settime()
  → set_realtime() / set_adjtime_delta()

do_times()
  → 读取 p_user_time / p_sys_time / boottime / uptime / realtime
```

### 2.5 设计要点/特殊处理

1. **BSP vs AP**：`uptime`/`realtime`/`clock_timers` 仅 BSP 维护，AP 的 `timer_int_handler` 只做记账和量子管理
2. **adjtime 限速**：每隔一个 tick 才调整一次 realtime（`uptime & 0x1` 检查），避免过快调整
3. **非 BILLABLE 进程记账**：内核任务的"用户时间"计为 billable 进程的"系统时间"
4. **vtimer_check 无锁**：注释说明时钟中断处理中调用，不会被系统任务中断，只需防时钟 handler 干扰

---

## 3. Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | 全局时钟状态 | `static mut` vs `ClockState` struct | **`ClockState` struct** | 封装性，BKL 保护下可变 |
| D2 | 定时器队列 | 链表 vs `BTreeMap<clock_t, Timer>` | **`BTreeMap`** | `no_std` 可用，O(log N) 插入/删除 |
| D3 | adjtime | 保留 vs 删除 | **保留** | NTP 场景仍需时间调整 |
| D4 | 负载平均 | 保留 vs 简化 | **保留** | 与 C 行为对齐 |
| D5 | `minix_timer_t` 链表 | 保留指针链表 vs 索引链表 vs BTreeMap | **BTreeMap + TimerEntry** | 避免指针，类型安全 |
| D6 | `cause_alarm` 回调 | 函数指针 vs enum 分发 | **enum `TimerAction`** | 类型安全，无函数指针 |
| D7 | `hz` 配置 | 运行时环境变量 vs 编译时常量 | **编译时常量 + 运行时可覆盖** | 保留 C 的灵活性 |
| D8 | BSP/AP 分支 | `cpu_is_bsp()` 运行时检查 vs per-CPU 回调 | **per-CPU 回调** | 避免每次中断做条件判断 |

---

## 4. 实现详解

### 4.1 ClockState — 全局时钟状态

> 设计决策 D1：封装为 struct，BKL 保护下可变。

```rust
/// Global clock state, equivalent to C's `kclockinfo` + `kloadinfo` + `clock_timers`.
///
/// All fields are protected by BKL. No interior mutability needed.
///
/// C: clock.h — `struct clockinfo kclockinfo` + `struct loadinfo kloadinfo`
pub struct ClockState {
    /// Clock frequency in Hz. C: `kclockinfo.hz`
    hz: u32,
    /// Monotonically increasing ticks since boot. C: `kclockinfo.uptime`
    uptime: u64,
    /// Wall-clock ticks since boot (affected by adjtime). C: `kclockinfo.realtime`
    realtime: u64,
    /// UNIX epoch seconds at boot. C: `kclockinfo.boottime`
    boottime: u64,
    /// Time adjustment delta (positive=speed up, negative=slow down).
    /// C: `adjtime_delta` (clock.c:42)
    adjtime_delta: i32,
    /// Synchronous alarm timer queue. C: `clock_timers` (clock.c:37)
    timers: alloc::collections::BTreeMap<u64, TimerEntry>,
    /// Load average info. C: `kloadinfo`
    load_info: LoadInfo,
}
```

### 4.2 TimerEntry — 定时器条目

> 设计决策 D5/D6：BTreeMap 替代链表，enum 替代函数指针。

```rust
/// A timer entry in the clock timer queue.
///
/// Replaces C's `minix_timer_t` linked list node.
/// C: timers.h — `struct minix_timer`
pub struct TimerEntry {
    /// Expiration time in monotonic ticks. C: `tmr_exp_time`
    exp_time: u64,
    /// Action to take when timer expires. Replaces C's `tmr_func_t` callback.
    pub action: TimerAction,
}

/// Action to take when a timer expires.
///
/// Replaces C's `tmr_func_t` function pointer + `tmr_arg` integer.
/// C: `cause_alarm(proc_nr_e)` — do_setalarm.c:73
pub enum TimerAction {
    /// Notify a process (synchronous alarm). C: `cause_alarm()` → `mini_notify(CLOCK, ep)`
    NotifyAlarm { endpoint: Endpoint },
    /// Custom kernel timer callback (for future use).
    KernelCallback { id: usize },
}
```

### 4.3 timer_int_handler — 时钟中断处理

> 设计决策 D8：per-CPU 回调区分 BSP/AP 逻辑。
> 2026-06-13 更新：D8 由「`if is_bsp` 运行时分支」升级为「`PerCpuTick` 标记类型 + 编译期单态化」，BSP-only 路径在 AP 构建中**完全消失**（无分支开销）。

```rust
// os/kernel/src/clock.rs

/// Per-CPU tick role marker (sealed — only BspTick / ApTick).
///
/// D8: encode BSP/AP distinction in the type system so the hot path
/// has no runtime branch. `tick_bsp` / `tick_ap` are convenience wrappers
/// that select the right marker.
pub trait PerCpuTick: sealed::Sealed {
    const IS_BSP: bool;
}
pub enum BspTick {}  // const IS_BSP: bool = true
pub enum ApTick  {}  // const IS_BSP: bool = false

impl ClockState {
    /// BSP-side tick wrapper (D8).
    pub fn tick_bsp(&mut self, current_proc: &mut KProcess,
                    is_billable: bool, ready_count: usize) -> TimerTickResult {
        self.tick::<BspTick>(current_proc, is_billable, ready_count)
    }

    /// AP-side tick wrapper (D8).
    pub fn tick_ap(&mut self, current_proc: &mut KProcess,
                   is_billable: bool, ready_count: usize) -> TimerTickResult {
        self.tick::<ApTick>(current_proc, is_billable, ready_count)
    }

    /// Handle a timer interrupt tick.
    ///
    /// C: `timer_int_handler()` — clock.c:70-175
    pub fn tick<P: PerCpuTick>(
        &mut self,
        current_proc: &mut KProcess,
        _is_billable: bool,
        ready_count: usize,
    ) -> TimerTickResult {
        // 1. BSP-only: update uptime and realtime (with adjtime)
        //    D8: branch resolved at compile time via P::IS_BSP.
        if P::IS_BSP {
            self.uptime += 1;
            if self.adjtime_delta != 0 && (self.uptime & 0x1) != 0 {
                self.realtime += if self.adjtime_delta > 0 { 2 } else { 0 };
                self.adjtime_delta += if self.adjtime_delta > 0 { -1 } else { 1 };
            } else {
                self.realtime += 1;
            }
        }

        // 2. Accounting: user time for current, system time for billable
        current_proc.p_time_stats.add_user_time(1);

        // 3. Decrement virtual/prof timers
        // ... (see §4.4)

        // 4. BSP-only: check alarm timers (D8 — monomorphized away on AP)
        let expired_alarms = if P::IS_BSP {
            self.check_expired_timers()
        } else {
            alloc::vec::Vec::new()
        };

        // 5. All CPUs: load update
        self.load_update(ready_count);

        TimerTickResult { expired_alarms, /* … */ }
    }
}
```

### 4.4 Virtual/Profile Timer Check

```rust
/// Check if a process's virtual or profile timer has expired.
///
/// C: `vtimer_check()` — do_vtimer.c:68-89
pub fn vtimer_check(proc: &mut KProcess) -> Option<VtimerExpired> {
    if proc.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER)
        && proc.p_time_stats.virt_left.load(Ordering::Acquire) == 0
    {
        proc.p_misc_flags.clear(MiscFlagsBits::VIRT_TIMER);
        return Some(VtimerExpired::Virtual);
    }
    if proc.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER)
        && proc.p_time_stats.prof_left.load(Ordering::Acquire) == 0
    {
        proc.p_misc_flags.clear(MiscFlagsBits::PROF_TIMER);
        return Some(VtimerExpired::Prof);
    }
    None
}
```

### 4.5 LoadInfo — 负载平均

```rust
/// Load average tracking. C: `struct loadinfo kloadinfo`
const LOAD_UNIT_SECS: u64 = 5;
const LOAD_HISTORY: usize = 12;

pub struct LoadInfo {
    proc_last_slot: u16,
    proc_load_history: [u32; LOAD_HISTORY],
    last_clock: u64,
}
```

---

## 5. 测试要点

1. **ClockState 初始化**：hz 默认 100，uptime/realtime/boottime 为 0
2. **tick 计数**：每次 tick uptime+1，realtime+1
3. **adjtime**：正 delta 时每隔 tick realtime+2，delta 递减
4. **定时器设置/到期**：设置定时器后，tick 到达 exp_time 时返回 NotifyAlarm
5. **定时器重置**：reset 后不再触发
6. **vtimer_check**：virt_left=0 且 MF_VIRT_TIMER 设置时返回 Virtual
7. **负载平均**：采样槽位随 uptime 推进变化

---

## 6. 补充：时钟详细分析

> 来源：tmp-16-timer.md

### 6.1 BSP/AP 时钟职责分离

SMP 下，BSP 负责全局时间维护和同步闹钟定时器，AP 仅更新本地进程计时。这避免了多 CPU 同时修改全局时间变量的竞态条件。AP 的时钟中断频率与 BSP 相同，但不递增 `uptime` 和 `realtime`。

### 6.2 进程计费的双重规则

时钟中断中，当前进程的 `p_user_time` 总是递增。但若当前进程不可计费（如内核任务），计费进程的 `p_sys_time` 递增。这确保了用户进程的系统时间被正确归因——当内核代表用户进程执行时，时间被计入该用户进程。

### 6.3 adjtime 的渐进调整

`adjtime_delta` 实现了 NTP 风格的时间平滑调整。不是一次性跳变，而是每两个 tick 调整一个 tick（加速前进 2 或减速前进 0）。这避免了时间突变对应用程序的影响。`uptime` 不受调整影响，保持严格单调。

### 6.4 虚拟定时器与剖面定时器的区别

- **虚拟定时器**（`MF_VIRT_TIMER`）：仅计用户态时间，到期发送 `SIGVTALRM`
- **剖面定时器**（`MF_PROF_TIMER`）：计用户态+系统态时间，到期发送 `SIGPROF`

剖面定时器还额外递减计费进程的 `p_prof_left`——因为一个进程的用户时间是另一个进程（计费进程）的系统时间。

### 6.5 clock_timers 队列的排序

`clock_timers` 队列按到期时间排序，最早到期的在队首。`tmrs_exptimers()` 从队首开始处理所有到期定时器，直到遇到未到期的定时器为止。O(k) 处理 k 个到期定时器。

### 6.6 看门狗定时器

`USE_WATCHDOG` 条件编译下，`watchdog_local_timer_ticks` 在每次时钟中断时递增。看门狗代码定期检查此变量是否在增长，若停滞则认为内核死锁，触发重启。

### 6.7 时钟频率常量

| 常量 | 值 | 含义 |
|------|-----|------|
| `DEFAULT_HZ` | 100 | 默认时钟频率（100Hz = 10ms/tick） |
| `kclockinfo.hz` | 可配置 | 实际时钟频率（2 ~ 50000） |

### 6.8 minix_timer_t 定时器结构

| 字段 | 类型 | 含义 |
|------|------|------|
| `tmr_exp_time` | `clock_t` | 到期时刻（uptime tick） |
| `tmr_func` | `tmr_func_t` | 到期时调用的 watchdog 函数 |
| `tmr_arg` | `int` | watchdog 函数参数 |
| `tmr_next` | `minix_timer_t *` | 队列中下一个定时器 |

---

## 7. 参见

- [14-exception-interrupt.md](14-exception-interrupt.md) — 时钟中断的入口路径
- [11-scheduling-primitives.md](11-scheduling-primitives.md) — 量子耗尽与调度
- [21-syscall-clock.md](21-syscall-clock.md) — 时钟相关系统调用
- [16-smp.md](16-smp.md) — AP 定时器初始化
