# 21-syscall-clock: 时钟系统调用

> **分类**: 系统调用服务
> **C 源码**: `minix3/minix/kernel/system/do_times.c` (46 行), `do_setalarm.c` (78 行), `do_stime.c` (19 行), `do_settime.c` (58 行), `do_vtimer.c` (103 行)
> **Rust 实现**: `os/kernel/src/syscall_clock.rs` (641 行), `os/kernel/src/clock.rs` (`ClockState`/`TimerAction`/`TimerEntry`/`TimerId`), `os/kernel/src/proc.rs` (`TimeStats`：`virt_left`/`prof_left`)
> **前置**: [15-clock-timer.md](15-clock-timer.md)（`ClockState`/`TimerAction`/`TimerEntry`/`TimerId` 定义），[13-syscall-dispatch.md](13-syscall-dispatch.md)，[22-privilege.md](22-privilege.md)，[17-syscall-process.md](17-syscall-process.md)，[16-smp.md](16-smp.md)
> **no_std 约束**: `#![no_std]`（`#[cfg(test)]` 除外）；仅依赖 `alloc::collections::{BTreeSet, BTreeMap}`，无外部 crate

---

## Ch1: 概念

**核心问题**：用户态进程如何通过系统调用查询进程时间统计、设置同步闹钟、调整系统时钟、管理虚拟/性能定时器？

**Minix3 的回答**：5 个时钟系统调用构成时间服务的用户态接口——TIMES 查询、SETALARM 闹钟、STIME/SETTIME 设置、VTIMER 虚拟定时器。它们复用 15-clock-timer 定义的 `ClockState`/`TimerAction` 内核基础设施，通过参数传入状态（非全局变量）保证可测试性。`ClockState`/`TimerAction`/`TimerEntry`/`TimerId` 定义见 15-clock-timer，`vtimer_check` 的递减与发信号也在 15 实现（tick-internal）。

| 系统调用 | 语义 | C 处理函数 | 权限 |
|---------|------|-----------|------|
| SYS_TIMES | 查询进程时间统计 + 三时钟源 | `do_times()` (do_times.c:22-44) | 任意进程 |
| SYS_SETALARM | 设置/取消同步闹钟 | `do_setalarm()` (do_setalarm.c:22-64) | SYS_PROC |
| SYS_STIME | 设置 boottime | `do_stime()` (do_stime.c:15-18) | VM（信任） |
| SYS_SETTIME | 设置实时时钟 / adjtime | `do_settime()` (do_settime.c:18-57) | PM/VM（信任） |
| SYS_VTIMER | 设置/查询虚拟/性能定时器 | `do_vtimer()` (do_vtimer.c:21-74) | SYS_PROC |

### 1.1 时间查询：TIMES

**灵魂本质**：TIMES 一次返回 5 个值——用户态/系统态 CPU 时间 + monotonic/realtime/boottime 三时钟源，三时钟源语义不可混淆。

**WHY → WHAT → HOW 弧线**：
- **WHY**：用户态需要知道进程消耗了多少 CPU 时间（性能分析、计费），以及当前系统时间（墙上时钟、启动时长）。这两个需求对应不同时钟源，不可混淆。
- **WHAT**：TIMES 返回 5 个值：`user_time`（用户态 tick）、`system_time`（系统态 tick）、`boot_ticks`（monotonic）、`real_ticks`（realtime）、`boot_time`（boottime）。
- **HOW**：C `do_times` (do_times.c:22-44) 先做 SELF 替换 (do_times.c:33-34)，若 endpoint 有效则读 `rp->p_user_time`/`rp->p_sys_time` (do_times.c:37-38)，最后无条件填三时钟源 (do_times.c:40-42)。

**三时钟源语义**（关键不可混淆）：

| 时钟源 | C 函数 | 语义 | 可设置? |
|--------|--------|------|---------|
| monotonic | `get_monotonic()` (do_times.c:40) | 启动后 tick 数，单调递增，不受 adjtime 影响 | 否 |
| realtime | `get_realtime()` (do_times.c:41) | 墙上时钟 tick 数（受 adjtime 影响） | 是（SETTIME now≠0） |
| boottime | `get_boottime()` (do_times.c:42) | 系统启动时的 Unix 时间戳 | 是（STIME） |

**SELF 语义**：`endpt == SELF` (-2) 表示"查询自己"，内核替换为 `caller->p_endpoint` (do_times.c:33-34)。`endpt == NONE` 表示不查询特定进程时间，仅返回时钟值 (do_times.c:35)。

**并发安全**：do_times.c:29-32 注释指出时钟中断 handler 可能并发更新时间字段，但单字段读是原子的。Rust 用 `AtomicU64::load(Ordering::Relaxed)` 天然表达此语义（见 Ch3 D6）。

### 1.2 同步闹钟：SETALARM

**灵魂本质**：每个 system process 一个 `s_alarm_timer`，到期通过 `cause_alarm` → `mini_notify(CLOCK, endpoint)` 通知——闹钟挂在 priv 结构上。

**WHY → WHAT → HOW 弧线**：
- **WHY**：系统进程需要内核级定时通知（用户进程用 SIGALRM 经 PM 转发，系统进程直接用内核闹钟）。闹钟需关联到进程的 priv 结构（每个 system process 独立）。
- **WHAT**：SETALARM 设置/取消调用者的同步闹钟。返回上次闹钟剩余 `time_left` + 当前 `uptime`。
- **HOW**：C `do_setalarm` (do_setalarm.c:22-64) 先 SYS_PROC 权限检查 (do_setalarm.c:33)，取 `priv(caller)->s_alarm_timer` (do_setalarm.c:36)，算 `time_left` (do_setalarm.c:40-46)，返回 `uptime` (do_setalarm.c:49)，最后 set/reset timer (do_setalarm.c:56-62)。到期回调 `cause_alarm` (do_setalarm.c:69-76) 调 `mini_notify(proc_addr(CLOCK), proc_nr_e)` (do_setalarm.c:75)。

**绝对/相对时间语义** (do_setalarm.c:56-61)：`!abs_time && exp_time==0` → 取消闹钟 `reset_kernel_timer(tp)` (do_setalarm.c:56-57)；`!abs_time && exp_time>0` → 相对时间，`exp_time += uptime` 转绝对 (do_setalarm.c:59-60)；`abs_time` → 直接用 `exp_time` (do_setalarm.c:61)。

**time_left 三分支** (do_setalarm.c:40-46)：timer 未设（`!tmr_is_set(tp)`）→ `TMR_NEVER` (do_setalarm.c:40-41)；timer 未到期（`tmr_is_first`）→ `exp_time - uptime` (do_setalarm.c:42-43)；timer 已到期 → `0` (do_setalarm.c:44-45)。

**权限**：仅 `SYS_PROC` 进程可调用 (do_setalarm.c:33)。用户进程的 SIGALRM 由 PM 转发，不直接走 SETALARM。

### 1.3 时间设置：STIME + SETTIME

**灵魂本质**：STIME 设 boottime；SETTIME 双模式——adjtime 渐变调整 vs `set_realtime` 直接设置，仅 CLOCK_REALTIME 可改。

**STIME** (do_stime.c:15-18)：设置启动时间 Unix 时间戳，由 VM 在初始化时调用。实现仅一行 `set_boottime(m_ptr->m_lsys_krn_sys_stime.boot_time)` (do_stime.c:17)。

**SETTIME 双模式** (do_settime.c:18-57)：

| 模式 | 条件 | 行为 | C 行号 |
|------|------|------|--------|
| adjtime | `now == 0` | 渐变调整：`set_adjtime_delta(ticks)`，`ticks = sec*hz + nsec/(1e9/hz)` | do_settime.c:29-34 |
| set time | `now != 0` | 直接设置：算 `timediff=sec-boottime`，`set_realtime(newclock)` | do_settime.c:37-57 |

**约束**：
- 仅 `CLOCK_REALTIME` 可改 (do_settime.c:25-26)，monotonic 不可设——monotonic 是内核单调时钟，受设置会破坏不变量
- set time 模式防负值：`sec <= boottime` 或 `timediff_ticks` 越界 → 修正 boottime + `set_realtime(1)` (do_settime.c:43-48)

**adjtime 为何保留**：POSIX adjtime(2) 语义——渐变调整时钟避免时间跳变，NTP 守护进程依赖。删除会破坏 POSIX 完整性（见 Ch3 D4）。

### 1.4 虚拟/性能定时器：VTIMER

**灵魂本质**：VT_VIRTUAL 计用户态时间、VT_PROF 计用户+系统时间，`virt_left`/`prof_left` 递减到 0 时触发 SIGVTALRM/SIGPROF——`vtimer_check` 在时钟中断检查到期。

**两类定时器** (do_vtimer.c:33-34, com.h:420-421)：

| 类型 | C 常量 | 值 | 计数范围 | 到期信号 | 字段 | 标志 |
|------|--------|---|---------|---------|------|------|
| 虚拟 | `VT_VIRTUAL` | 1 | 仅用户态时间 | `SIGVTALRM` | `p_virt_left` | `MF_VIRT_TIMER` |
| 性能 | `VT_PROF` | 2 | 用户+系统时间 | `SIGPROF` | `p_prof_left` | `MF_PROF_TIMER` |

> **数值对齐**：`VT_VIRTUAL = 1`、`VT_PROF = 2` 来自 C `com.h:420-421`。Rust `VtimerType` enum 的 `Virtual = 1`、`Prof = 2` 严格对齐此定义（见 Ch3 D3）。

**WHY → WHAT → HOW 弧线**：
- **WHY**：进程需要"按 CPU 时间非墙上时间"的定时器（profiling、用户态 CPU 限制）。VT_VIRTUAL 仅计用户态，VT_PROF 计用户+系统，区分用途。
- **WHAT**：VTIMER 设置/查询进程的虚拟/性能定时器。`VT_SET=0` 仅查询；`VT_SET=1` 设置新值（`value>0` 启用，`value=0` 禁用）。返回旧值。
- **HOW**：C `do_vtimer` (do_vtimer.c:21-74) 先 SYS_PROC 检查 (do_vtimer.c:31)，验证 `VT_WHICH` (do_vtimer.c:33-34)，SELF 替换 + `isokendpt` (do_vtimer.c:37-38)，确定 `pt_flag`/`pt_left` (do_vtimer.c:45-51)，取旧值 (do_vtimer.c:54-58)，若 VT_SET 则 clear flag → set/clear value → set flag (do_vtimer.c:60-69)，返回旧值 (do_vtimer.c:71)。

**vtimer_check 到期机制** (do_vtimer.c:81-103)：由时钟中断调用，**在 15-clock-timer 实现**（tick-internal，见 Ch3 D7）：
- `MF_VIRT_TIMER && p_virt_left==0` → 清标志 + `cause_sig(rp->p_nr, SIGVTALRM)` (do_vtimer.c:91-95)
- `MF_PROF_TIMER && p_prof_left==0` → 清标志 + `cause_sig(rp->p_nr, SIGPROF)` (do_vtimer.c:98-102)
- 并发安全 (do_vtimer.c:83-88)：clock handler 只递减 `p_virt_left`/`p_prof_left`，不修改 `p_misc_flags`，故 `vtimer_check` 无需锁

**权限**：仅 `SYS_PROC` 可调用 (do_vtimer.c:31)。用户进程经 PM 设置。

---

## Ch2: C 源码分析

### 2.1 do_times.c (46 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_times(caller, m_ptr)` | do_times.c:22-44 | TIMES 主函数：SELF 替换 (L33-34) → endpoint 校验 (L35) → 读 user/sys time (L37-38) → 填三时钟源 (L40-42) |
| 并发注释 | do_times.c:29-32 | 时钟 handler 可并发更新单字段，但单字段读原子 |

### 2.2 do_setalarm.c (78 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_setalarm(caller, m_ptr)` | do_setalarm.c:22-64 | SETALARM 主函数：参数提取 (L31-32) → SYS_PROC 检查 (L33) → 取 `s_alarm_timer` (L36) → `uptime=get_monotonic()` (L39) → time_left 三分支 (L40-46) → 返回 uptime (L49) → reset/set timer (L56-62) |
| `cause_alarm(proc_nr_e)` | do_setalarm.c:69-76 | 闹钟到期回调 → `mini_notify(proc_addr(CLOCK), proc_nr_e)` (L75) |

### 2.3 do_stime.c (19 行) + do_settime.c (58 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_stime(caller, m_ptr)` | do_stime.c:15-18 | STIME：`set_boottime(boot_time)` (L17) |
| `do_settime(caller, m_ptr)` | do_settime.c:18-57 | SETTIME 主函数：CLOCK_REALTIME 检查 (L25-26) → adjtime 模式 `now==0` `set_adjtime_delta(ticks)` (L29-34) / set time 模式 `now!=0` `timediff=sec-boottime` (L39-40) → 负值保护 (L43-48) → `set_realtime(newclock)` (L55) |
| ticks 转换 | do_settime.c:31-32 | `sec*system_hz + nsec/(1e9/system_hz)` |

### 2.4 do_vtimer.c (103 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_vtimer(caller, m_ptr)` | do_vtimer.c:21-74 | VTIMER 主函数：SYS_PROC 检查 (L31) → VT_WHICH 验证 (L33-34) → SELF 替换 + isokendpt (L37-38) → pt_flag/pt_left 确定 (L45-51) → 旧值读取 (L54-58) → VT_SET: clear flag → set/clear value → set flag (L60-69) → 返回旧值 (L71) |
| `vtimer_check(rp)` | do_vtimer.c:81-103 | 时钟中断调用：VIRT 到期 (L91-95) `MF_VIRT_TIMER && virt_left==0` → 清标志 + `cause_sig(SIGVTALRM)`；PROF 到期 (L98-102) 同理 → `SIGPROF` |
| 并发注释 | do_vtimer.c:83-88 | clock handler 只递减 `p_virt_left`/`p_prof_left`，不修改 `p_misc_flags`，无需锁 |

### 2.5 调用关系图

**SETALARM 闹钟生命周期**：用户态 `sys_setalarm` → `dispatch_setalarm` [syscall_clock.rs:175]（SYS_PROC 检查 → 取 `s_alarm_timer` 算 time_left → `set_timer(TimerEntry{NotifyAlarm{endpoint}})` 返回 `TimerId` → 存储 `(entry, id)` 到 `priv.runtime.s_alarm_timer`）→ 时钟中断到期 → `ClockState::collect_expired_timers` [clock.rs] → `pop_expired` → `TimerAction::NotifyAlarm{endpoint}` → `mini_notify(CLOCK, endpoint)`（对应 C `cause_alarm` do_setalarm.c:75）。

**VTIMER 到期生命周期**：用户态 `sys_vtimer(VT_VIRTUAL, VT_SET, value, endpt)` → `dispatch_vtimer` [syscall_clock.rs:421]（SYS_PROC 检查 → `store(value)` 到 `virt_left` + set `VIRT_TIMER` → 返回旧值）→ 时钟中断 tick → tick handler [clock.rs] → `tick_virt_timer()` CAS 递减 `virt_left` [proc.rs:693] → 递减到 0 → SIGVTALRM（对应 C `vtimer_check` do_vtimer.c:91-95）。

---

## Ch3: 设计决策

> **设计原则**：避免 C 代码的 translate，用 Rust 类型系统重新表达 C 的整数类型（`VtimerType` enum）、函数指针（`TimerAction::NotifyAlarm`）、全局状态访问（`ClockState` 参数传递）、非原子字段（`AtomicU64` for `virt_left`/`prof_left`）。每个决策采用"如果 X 设计会有 Y 问题所以用 Z"的 hypothesis 推理。

### D1: SETALARM 定时器表达 — `minix_timer_t` 链表 vs BTreeMap dual-index

**假设性推理**：
- 如果用 C 的 `minix_timer_t` 链表（`tp->tmr_next`）：Rust 无内置链表节点嵌入模式，且链表删除需 O(N) 扫描或维护双向指针，内存安全难保证。
- 如果用 `BTreeMap<u64, TimerEntry>`（exp_time 作 key）：同 exp_time 的多 timer 会覆盖——C 链表支持同 exp_time，内核场景需要。
- 所以用 `BTreeSet<(u64, TimerId)>` + `BTreeMap<TimerId, TimerEntry>` dual-index（与 15-clock-timer D2 一致）：BTreeSet 按 (exp_time, id) 排序支持到期扫描，BTreeMap 按 `TimerId` O(log N) 查找支持 `reset_timer(id)`。

**实现**：`clock_state.set_timer(entry) -> TimerId` / `reset_timer(id)` (syscall_clock.rs:246,221)。`s_alarm_timer: Option<(TimerEntry, TimerId)>` 存储 id 用于后续 reset。

### D2: cause_alarm 回调 — 函数指针 vs TimerAction enum

**假设性推理**：
- 如果用 C 的函数指针 `tmr_func_t` (do_setalarm.c:61 `cause_alarm`)：Rust 函数指针 `fn(i32)` 无闭包捕获，endpoint 需额外存 timer 结构体；类型不安全，任何函数指针都能传入。
- 如果用 trait object `Box<dyn TimerCallback>`：堆分配 + 动态分发，no_std 下需 alloc 且增加间接调用开销。
- 所以用 `TimerAction` enum（与 15-clock-timer D6 一致）：`NotifyAlarm { endpoint }` 变体携带 endpoint，enum 分发编译期穷尽，无堆分配。

**实现**：`TimerAction::NotifyAlarm { endpoint: caller.p_endpoint }` (syscall_clock.rs:231-236)。到期时 ClockState 弹出 action，dispatch 到 `mini_notify(CLOCK, endpoint)`。

### D3: VT_WHICH 表达 — 整数 vs VtimerType enum

**假设性推理**：
- 如果用裸 `i32`（C 方式 do_vtimer.c:33）：`which != VT_VIRTUAL && != VT_PROF` 魔法数字比较，易写错值（如把 VT_PROF 误写为 1）。
- 如果用 `const` 常量：仍是整数，无类型安全，函数参数无法区分"任意 i32"与"vtimer 类型"。
- 所以用 `VtimerType` enum + `TryFrom<i32>`：编译期穷尽，`try_from(which)` 返回 `Result`，非法值 `Err(()) → EINVAL`。值 `Virtual=1, Prof=2` 对齐 C `com.h:420-421`。

**实现**：`enum VtimerType { Virtual=1, Prof=2 }` + `impl TryFrom<i32>` (syscall_clock.rs:43-60)。

### D4: SETTIME adjtime — 删除 vs 保留

**假设性推理**：
- 如果删除 adjtime 模式（仅保留 `set_realtime`）：失去 POSIX adjtime(2) 语义——渐变调整时钟避免时间跳变，NTP 场景必需。删除会破坏 POSIX 完整性，且 C ground truth 有完整实现 (do_settime.c:29-34)。
- 如果保留但简化（不实现渐变逻辑）：`adjtime_delta` 字段无消费者，等于死代码。
- 所以保留完整 adjtime 模式：`set_adjtime_delta(ticks)` 写入 ClockState，渐变逻辑在 15-clock-timer 的 tick handler 消费 delta。

**实现**：`clock_state.set_adjtime_delta(ticks)` (syscall_clock.rs:376)，ticks = `sec*hz + nsec/(1e9/hz)` (do_settime.c:31-32)。

### D5: ClockState 访问 — 全局变量 vs 参数传递（核心 anti-translate）

**假设性推理**：
- 如果用全局变量访问 ClockState（C 方式 do_setalarm.c:39 `get_monotonic()` 全局 / do_setalarm.c:57 `reset_kernel_timer(tp)` 隐式全局时钟）：测试时无法注入 mock ClockState——`get_monotonic()` 读全局 atomic，`reset_kernel_timer` 操作全局 timer 队列。单元测试无法隔离时间状态，只能测权限路径（EPERM），无法测 set/reset 行为。
- 如果用 `thread_local!`：no_std 无线程，且 SMP 内核无 thread-local 语义。
- 如果用 trait + 全局单例：仍是全局，测试需替换全局状态，并发不安全。
- 所以用参数传递：`dispatch_setalarm(caller, msg, priv_table: &mut PrivTable, clock_state: &mut ClockState)` 显式传入状态。测试可构造 `ClockState::new()` + `PrivTable::new()` 注入，验证 set/reset 行为。

**实现**：所有 dispatch 函数接受 `&mut ClockState` / `&mut PrivTable` / `&ProcessTable` 参数 (syscall_clock.rs:175-180, 319-324, 350-355, 421-426)。`caller_has_sys_proc_with_table(caller, priv_table)` 同样参数传入 (syscall_clock.rs:282)。

**与 redox 对比**：redox 用 `time::monotonic` 全局接口 + `scheme::time` 用户态驱动；minix-rs 显式传入 `ClockState` 更可测，对齐 C 的内核通知模型。

### D6: virt_left/prof_left 存储 — Cell/u64 vs AtomicU64

**假设性推理**：
- 如果用 `Cell<u64>`：`Cell` 非 `Sync`，无法跨 CPU 共享（SMP 内核硬约束），`&KProcess` 无法传递到其他 CPU。
- 如果用 `u64` + 锁：vtimer 字段高频读写（每个 tick 递减），锁开销大且 vtimer_check 注释 (do_vtimer.c:83-88) 明确"无需锁"。
- 如果用 `RefCell<u64>`：同 Cell，非 Sync。
- 所以用 `AtomicU64`：`Sync` + 无锁，`compare_exchange_weak` 递减 (proc.rs:700,716)，SMP 安全。C 靠注释约定"clock handler 只递减"的并发安全，Rust 用原子操作编译期保证。

**实现**：`TimeStats.virt_left: AtomicU64` / `prof_left: AtomicU64` (proc.rs:671-672)。dispatch_vtimer 用 `load(Ordering::Relaxed)` 读 / `store(Ordering::Release)` 写 (syscall_clock.rs:469,478,497,500,508,511)。

### D7: vtimer_check — standalone 函数 vs tick-internal

**假设性推理**：
- 如果保留 C 的 standalone `vtimer_check(rp)` 函数 (do_vtimer.c:81-103)：需在 21 文档重复实现，但 vtimer_check 由时钟中断调用，属于 15-clock-timer 的 tick handler 职责。21 是用户态接口（set/query），15 是内核 tick 机制（递减/到期），职责分离。
- 如果在 21 实现 vtimer_check：跨文档职责混乱，21 依赖 15 的 tick 调度。
- 所以 vtimer_check 语义内联到 15-clock-timer 的 tick handler（与 15 D11 一致）：`tick_virt_timer()`/`tick_prof_timer()` 递减并返回是否到期 (proc.rs:693,709)，clock.rs 调用并处理 SIGVTALRM/SIGPROF。21 仅负责 set/query 接口。

**实现**：21 文档 Ch4 标注 vtimer_check 在 15-clock-timer 实现（跨文档衔接），Ch6 参见引用 15。

---

## Ch4: 实现详解

> 实现位置：`os/kernel/src/syscall_clock.rs`。代码片段与源码一致，行号引用以源码为准。`ClockState`/`TimerAction`/`TimerEntry`/`TimerId` 在 15-clock-timer 定义，本章仅展示 syscall_clock.rs 的用户态接口侧。

### 4.1 VtimerType enum — 类型安全的定时器类型（D3）

> 设计决策 D3：用 enum + `TryFrom<i32>` 替代 C 的整数比较，值严格对齐 `com.h:420-421`。

```rust
// os/kernel/src/syscall_clock.rs:43-60

/// Virtual timer type. C: `VT_VIRTUAL` / `VT_PROF` — com.h:420-421.
///
/// Values strictly align with C: `VT_VIRTUAL = 1`, `VT_PROF = 2`.
/// `#[repr(i32)]` preserves the ABI for IPC message compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VtimerType {
    /// Virtual timer (counts user-mode time). C: `VT_VIRTUAL = 1`
    Virtual = 1,
    /// Profile timer (counts user + system time). C: `VT_PROF = 2`
    Prof = 2,
}

impl TryFrom<i32> for VtimerType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Virtual),
            2 => Ok(Self::Prof),
            _ => Err(()),
        }
    }
}
```

**与 C 的差异**（D3 见 Ch3）：`#[repr(i32)]` 保证 FFI 布局兼容性，`as i32` 可安全转回 C 值；非法 `which` 经 `TryFrom` 返回 `Err(()) → EINVAL`，替代 C 整数比较 (do_vtimer.c:33)。

### 4.2 dispatch_times — 时间查询（A 组）

```rust
// os/kernel/src/syscall_clock.rs:104-158

/// Dispatch SYS_TIMES.
///
/// C: `do_times()` — do_times.c:22-44
///
/// Retrieve accounting information for a process.
///
/// # Implementation
///
/// Full implementation matching C `do_times`:
/// 1. SELF replacement: `endpt == SELF` → use `caller.p_endpoint`.
/// 2. Endpoint validation via `ProcessTable::endpoint_to_nr()`.
/// 3. If endpoint is valid and not NONE: read `p_user_time` + `p_sys_time`.
/// 4. Always read `get_monotonic()`, `get_realtime()`, `get_boottime()`.
/// 5. Pack into `MessKrnLsysSysTimes` reply overlay.
pub fn dispatch_times(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &ProcessTable,
) -> KcallResult {
    // C: do_times.c:33-34 — extract endpoint (SELF replacement inline)
    msg.debug_check_m_type_any(&[Syscall::Times as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_times };
    let endpt = req.endpt;

    // C: do_times.c:33-34 — SELF replacement
    let target_endpoint = if endpt == SELF {
        caller.p_endpoint
    } else {
        Endpoint(endpt)
    };

    // C: do_times.c:35-38 — if valid endpoint, read user/sys time
    let (user_time, sys_time) = if target_endpoint != Endpoint::NONE {
        if let Some(proc_nr) = proc_table.endpoint_to_nr(target_endpoint) {
            // C: do_times.c:36-38 — rp = proc_addr(proc_nr)
            if let Some(rp) = proc_table.get(proc_nr) {
                (
                    rp.p_time.user_time.load(Ordering::Relaxed),
                    rp.p_time.sys_time.load(Ordering::Relaxed),
                )
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        }
    } else {
        // C: do_times.c:35 — if e_proc_nr == NONE, skip user/sys time
        (0, 0)
    };

    // C: do_times.c:40-42 — always fill these fields
    let reply = MessKrnLsysSysTimes {
        boot_ticks: clock::get_monotonic(),
        real_ticks: clock::get_realtime(),
        user_time,
        system_time: sys_time,
        boot_time: clock::get_boottime(),
        _padding: [0u8; 16],
    };

    // Write reply into message
    msg.debug_check_m_type_any(&[Syscall::Times as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    msg.m_u.m_krn_lsys_sys_times = reply;

    KcallResult::Ok(OK)
}
```

**设计要点**（D6 见 Ch3）：三时钟源 `get_monotonic`/`get_realtime`/`get_boottime` 对应 do_times.c:40-42，语义不可混淆（见 §1.1 表）；`AtomicU64::load(Relaxed)` 读 user/sys time 对应 C 单字段读原子性 (do_times.c:29-32 并发注释)。

### 4.3 dispatch_setalarm — 同步闹钟（B 组）

```rust
// os/kernel/src/syscall_clock.rs:175-264

/// Dispatch SYS_SETALARM.
///
/// C: `do_setalarm()` — do_setalarm.c:22-64 (cause_alarm: 69-76)
///
/// Set or cancel a synchronous alarm timer for a system process.
/// The alarm fires via `mini_notify(CLOCK, endpoint)`.
///
/// # Implementation
///
/// Full implementation matching C `do_setalarm`:
/// 1. SYS_PROC permission check via PrivTable.
/// 2. Get `s_alarm_timer` from caller's KPriv.
/// 3. Calculate time_left on previous alarm.
/// 4. Return time_left and current uptime.
/// 5. Set or reset timer in ClockState.
pub fn dispatch_setalarm(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    // C: do_setalarm.c:31-32 — extract parameters
    msg.debug_check_m_type_any(&[Syscall::Setalarm as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_setalarm };
    let exp_time = req.exp_time;
    let use_abs_time = req.abs_time != 0;

    // C: do_setalarm.c:33 — SYS_PROC permission check
    if !caller_has_sys_proc_with_table(caller, priv_table) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_setalarm.c:36 — get timer from priv structure
    let caller_priv_id = match caller.priv_id {
        Some(id) => id,
        None => return KcallResult::Ok(EPERM), // already checked above, defensive
    };

    // C: do_setalarm.c:39-46 — return time left on previous alarm
    let uptime = clock_state.uptime();
    let time_left = {
        let kpriv = priv_table.get(caller_priv_id);
        if let Some(kpriv) = kpriv {
            match &kpriv.runtime.s_alarm_timer {
                None => TMR_NEVER,
                Some((timer, _id)) => {
                    timer.exp_time.saturating_sub(uptime)
                }
            }
        } else {
            TMR_NEVER
        }
    };

    // C: do_setalarm.c:56-62 — set or reset timer
    if !use_abs_time && exp_time == 0 {
        // Reset alarm: C: do_setalarm.c:57 — reset_kernel_timer(tp)
        let kpriv = priv_table.get_mut(caller_priv_id);
        if let Some(kpriv) = kpriv
            && let Some((_old_entry, old_id)) = kpriv.runtime.s_alarm_timer.take() {
                clock_state.reset_timer(old_id);
            }
    } else {
        // Set alarm: C: do_setalarm.c:61 — set_kernel_timer(tp, exp_time, cause_alarm, caller->p_endpoint)
        let actual_exp_time = if use_abs_time {
            exp_time
        } else {
            uptime + exp_time
        };

        let timer = TimerEntry {
            exp_time: actual_exp_time,
            action: TimerAction::NotifyAlarm {
                endpoint: caller.p_endpoint,
            },
        };

        // Remove existing timer if any, then set the new one.
        // D3: set_timer returns a TimerId that must be stored for later
        // reset_timer(id) (15-clock-timer.md §4.4).
        let kpriv = priv_table.get_mut(caller_priv_id);
        if let Some(kpriv) = kpriv {
            if let Some((_old_entry, old_id)) = kpriv.runtime.s_alarm_timer.take() {
                clock_state.reset_timer(old_id);
            }
            let id = clock_state.set_timer(timer.clone());
            kpriv.runtime.s_alarm_timer = Some((timer, id));
        }
    }

    // C: do_setalarm.c:49 — return current uptime + time_left
    let reply = MessLsysKrnSysSetalarm {
        exp_time,
        time_left,
        uptime,
        abs_time: if use_abs_time { 1 } else { 0 },
        _padding: [0u8; 28],
    };
    msg.debug_check_m_type_any(&[Syscall::Setalarm as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    msg.m_u.m_lsys_krn_sys_setalarm = reply;

    KcallResult::Ok(OK)
}
```

**设计要点**（D1/D2/D5 见 Ch3）：
- time_left 对应 do_setalarm.c:40-46 三分支——`Option` 替代 C `tmr_is_set(tp)` 标志检查（`None`→`TMR_NEVER`）；未到期/到期两分支合并为 `saturating_sub`（`exp_time < uptime` → 0，语义等价 C 的 `tmr_is_first` 判断：未到期 `exp_time - uptime`、到期 0）
- `cause_alarm` 对应：C `cause_alarm(proc_nr_e)` (do_setalarm.c:69-76) → `mini_notify(proc_addr(CLOCK), proc_nr_e)` (do_setalarm.c:75)；Rust 用 `TimerAction::NotifyAlarm { endpoint }` 携带 endpoint，ClockState 到期时 dispatch 到通知

### 4.4 dispatch_stime + dispatch_settime — 时间设置（C 组）

```rust
// os/kernel/src/syscall_clock.rs:319-333

/// Dispatch SYS_STIME.
///
/// C: `do_stime()` — do_stime.c:15-18
///
/// Set the boot time (Unix timestamp when the system was booted).
///
/// # Implementation
///
/// Full implementation matching C `do_stime`:
/// 1. Extract `boot_time` from `m_lsys_krn_sys_stime.boot_time`.
/// 2. Call `ClockState::set_boottime()` which updates both the
///    internal field and the global `CLOCK_BOOTTIME` atomic.
/// 3. Return OK.
pub fn dispatch_stime(
    _caller: &mut KProcess,
    msg: &Message,
    clock_state: &mut ClockState,
) -> KcallResult {
    // C: do_stime.c:17 — set_boottime(m_ptr->m_lsys_krn_sys_stime.boot_time)
    msg.debug_check_m_type_any(&[Syscall::Stime as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_stime };
    let boot_time = req.boot_time;

    clock_state.set_boottime(boot_time);

    KcallResult::Ok(OK)
}
```

```rust
// os/kernel/src/syscall_clock.rs:350-404

/// Dispatch SYS_SETTIME.
///
/// C: `do_settime()` — do_settime.c:18-58
///
/// Set the real-time clock or adjust time gradually (adjtime).
///
/// # Implementation
///
/// Full implementation matching C `do_settime`:
/// 1. Validate `clock_id == CLOCK_REALTIME` (C:25-26).
/// 2. If `now == 0`: adjtime mode — convert sec+nsec to ticks and
///    call `set_adjtime_delta()` (C:29-34).
/// 3. If `now != 0`: set-time mode — compute `timediff_ticks` from
///    `sec - boottime`, validate range, and call `set_realtime()`
///    (C:35-57). If boottime was wrong, correct it.
pub fn dispatch_settime(
    _caller: &mut KProcess,
    msg: &Message,
    clock_state: &mut ClockState,
) -> KcallResult {
    // C: do_settime.c:25-52 — parameters read inline (C has no extraction)
    msg.debug_check_m_type_any(&[Syscall::Settime as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_settime };
    let now = req.now;
    let clock_id = req.clock_id;
    let sec = req.sec;
    let nsec = req.nsec;

    // C: do_settime.c:25-26 — only CLOCK_REALTIME allowed
    if clock_id != CLOCK_REALTIME {
        return KcallResult::Ok(EINVAL);
    }

    let hz = clock_state.system_hz();

    // C: do_settime.c:29-34 — adjtime mode (now == 0)
    if now == 0 {
        // Convert delta from seconds + nanoseconds to ticks
        // C: do_settime.c:31-32 — ticks = (sec * system_hz) + (nsec / (1000000000 / system_hz))
        let ticks = (sec as i64 * hz as i64 + nsec / (1_000_000_000 / hz as i64)) as i32;
        clock_state.set_adjtime_delta(ticks);
        return KcallResult::Ok(OK);
    }

    // C: do_settime.c:35-57 — set time mode (now != 0)
    let boottime = clock_state.boottime();

    // C: do_settime.c:39 — timediff = sec - boottime
    let timediff = sec as i64 - boottime as i64;
    // C: do_settime.c:40 — timediff_ticks = timediff * system_hz
    let timediff_ticks = timediff * hz as i64;

    // C: do_settime.c:43-48 — prevent negative realtime
    if sec <= boottime
        || timediff_ticks < i32::MIN as i64 / 2
        || timediff_ticks > i32::MAX as i64 / 2
    {
        // C: boottime was likely wrong, try to correct it
        clock_state.set_boottime(sec);
        clock_state.set_realtime(1);
        return KcallResult::Ok(OK);
    }

    // C: do_settime.c:51-53 — calculate new realtime in ticks
    let newclock = (timediff_ticks + nsec / (1_000_000_000 / hz as i64)) as u64;
    clock_state.set_realtime(newclock);

    KcallResult::Ok(OK)
}
```

**设计要点**（D4/D5 见 Ch3）：
- CLOCK_REALTIME 检查对应 do_settime.c:25-26 — monotonic 不可设
- 负值保护对应 do_settime.c:43-48 — boottime 错误时修正

### 4.5 dispatch_vtimer — 虚拟/性能定时器（D 组）

> 展示真实 `AtomicU64` 代码（无占位符）。`virt_left`/`prof_left` 是 `AtomicU64` (proc.rs:671-672)，SMP 安全。

```rust
// os/kernel/src/syscall_clock.rs:421-522

/// Dispatch SYS_VTIMER.
///
/// C: `do_vtimer()` — do_vtimer.c:21-74 (vtimer_check: 81-103)
///
/// Set and/or retrieve the value of a process's virtual or profile timer.
///
/// # Implementation
///
/// Full implementation matching C `do_vtimer`:
/// 1. SYS_PROC permission check via PrivTable.
/// 2. Validate timer type (VT_VIRTUAL=1 / VT_PROF=2).
/// 3. SELF replacement + endpoint validation via ProcessTable.
/// 4. Retrieve old value from `p_virt_left` / `p_prof_left`.
/// 5. If VT_SET: write new value and set/clear MiscFlags.
/// 6. Return old value in reply message.
pub fn dispatch_vtimer(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &PrivTable,
    proc_table: &ProcessTable,
) -> KcallResult {
    // C: do_vtimer.c:33-71 — M2 parameters read inline (VT_WHICH:33,
    // VT_ENDPT:37, VT_SET:60, VT_VALUE:63/71)
    msg.debug_check_m_type_any(&[Syscall::Vtimer as i32]);
    let m2 = msg_m2(msg);
    let which = m2.m2i1;        // VT_WHICH
    let set = m2.m2i2 != 0;     // VT_SET
    let value = m2.m2l1 as u64; // VT_VALUE
    let endpt = m2.m2l2 as i32; // VT_ENDPT

    // C: do_vtimer.c:31 — SYS_PROC permission check
    if !caller_has_sys_proc_with_table(caller, priv_table) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_vtimer.c:33-34 — validate timer type
    let vtype = match VtimerType::try_from(which) {
        Ok(v) => v,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:37-38 — SELF replacement + endpoint validation
    let target_endpoint = if endpt == SELF {
        caller.p_endpoint
    } else {
        Endpoint(endpt)
    };

    // C: do_vtimer.c:38 — isokendpt check
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:39 — rp = proc_addr(proc_nr)
    let target = match proc_table.get(target_nr) {
        Some(rp) => rp,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:45-58 — determine flag/field + retrieve old value
    let (pt_flag, old_value) = match vtype {
        VtimerType::Virtual => {
            let flag = MiscFlagsBits::VIRT_TIMER;
            let old = if target.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER) {
                target.p_time.virt_left.load(Ordering::Relaxed)
            } else {
                0
            };
            (flag, old)
        }
        VtimerType::Prof => {
            let flag = MiscFlagsBits::PROF_TIMER;
            let old = if target.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
                target.p_time.prof_left.load(Ordering::Relaxed)
            } else {
                0
            };
            (flag, old)
        }
    };

    // C: do_vtimer.c:60-69 — set new value if VT_SET
    if set {
        // C: do_vtimer.c:61 — disable timer first
        target.p_misc_flags.clear(pt_flag);

        if value > 0 {
            // C: do_vtimer.c:63-65 — set new timer value + re-enable
            match vtype {
                VtimerType::Virtual => {
                    target.p_time.virt_left.store(value, Ordering::Release);
                }
                VtimerType::Prof => {
                    target.p_time.prof_left.store(value, Ordering::Release);
                }
            }
            target.p_misc_flags.set(pt_flag);
        } else {
            // C: do_vtimer.c:66-68 — clear timer value
            match vtype {
                VtimerType::Virtual => {
                    target.p_time.virt_left.store(0, Ordering::Release);
                }
                VtimerType::Prof => {
                    target.p_time.prof_left.store(0, Ordering::Release);
                }
            }
        }
    }

    // C: do_vtimer.c:71 — return old value in VT_VALUE
    // Write old_value back into the message's m2_l1 field
    // SAFETY: `m_type == SYS_VTIMER` guarantees the M2 format is active.
    // Writing to `m_m2.m2l1` is sound per `#[repr(C)]` union layout.
    msg.m_u.m_m2.m2l1 = old_value as i64;

    KcallResult::Ok(OK)
}
```

**virt_left/prof_left 定义**（跨文档引用）：`TimeStats` (proc.rs:668-672) 的 `virt_left`/`prof_left` 字段为 `AtomicU64` (proc.rs:671-672)，对应 C `p_virt_left` (do_vtimer.c:47) / `p_prof_left` (do_vtimer.c:50)。完整结构定义见 15-clock-timer。

**设计要点**（D3/D6 见 Ch3）：
- MiscFlagsBits bitflags：`is_set`/`set`/`clear` 替代 C 裸位操作 `p_misc_flags & pt_flag` (do_vtimer.c:54,61,65)

**vtimer_check 跨文档衔接**（D7）：C `vtimer_check(rp)` (do_vtimer.c:81-103) 在 15-clock-timer 实现（tick-internal，`tick_virt_timer`/`tick_prof_timer` CAS 递减 → SIGVTALRM/SIGPROF），21 仅负责 set/query 接口（详见 §1.4 与 Ch3 D7）。

### 4.6 caller_has_sys_proc_with_table — 权限检查（E 组）

```rust
// os/kernel/src/syscall_clock.rs:282-290

/// Returns true iff `caller.priv_id` is `Some` AND the matching KPriv
/// entry is a SYS_PROC (i.e. has `PrivFlagsBits::SYS_PROC` set).
///
/// Used by `dispatch_setalarm` / `dispatch_vtimer` to gate syscalls
/// that only system processes may invoke (Minix3: `do_setalarm.c:33`,
/// `do_vtimer.c:31`).
///
/// Fail-closed: if `priv_id` is `None` we conservatively return `false`
/// so the syscall is rejected rather than silently allowed.
///
/// # Legacy note
///
/// The old `caller_has_sys_proc()` used `PrivTable::new()` which created
/// a fresh empty table — always returning `false` (fail-closed but
/// over-rejecting). This version uses the real PrivTable passed from
/// the dispatcher.
pub(crate) fn caller_has_sys_proc_with_table(caller: &KProcess, priv_table: &PrivTable) -> bool {
    let Some(priv_id) = caller.priv_id else {
        return false;
    };
    priv_table
        .get(priv_id)
        .map(KPriv::is_sys_proc)
        .unwrap_or(false)
}
```

**设计要点**（D5 见 Ch3）：fail-closed 语义 `priv_id=None → false`，对应 C `return(EPERM)` (do_setalarm.c:33)；`&PrivTable` 参数传入使测试可构造空表验证。

---

## Ch5: 测试

> 测试位置：`os/kernel/src/syscall_clock.rs` `#[cfg(test)] mod tests`。所有测试函数可 grep 验证：`rg "fn test_" os/kernel/src/syscall_clock.rs --type rust -n`。

### 5.1 现有测试（10 个，已实现）

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_vtimer_type_values_match_c` | VtimerType 值对齐 C (Virtual=1, Prof=2) | `VT_VIRTUAL`/`VT_PROF` com.h:420-421 |
| `test_vtimer_type_try_from` | `TryFrom<i32>` 合法/非法值 | do_vtimer.c:33-34 |
| `test_clock_realtime` | `CLOCK_REALTIME=0` | do_settime.c:25 |
| `test_ksc_caller_has_sys_proc_no_priv_id_rejected` | priv_id=None 拒绝 | do_setalarm.c:33 |
| `test_ksc_caller_has_sys_proc_fresh_table_rejects_all` | 空 PrivTable 拒绝 | do_setalarm.c:33 |
| `test_ksc_setalarm_non_sys_proc_returns_eperm` | SETALARM 非 SYS_PROC 返回 EPERM | do_setalarm.c:33 |
| `test_ksc_vtimer_non_sys_proc_returns_eperm` | VTIMER 非 SYS_PROC 返回 EPERM | do_vtimer.c:31 |
| `test_ksc_priv_flags_sys_proc_bit_definition` | IDL_F/SRV_F 含 SYS_PROC, USR_F 不含 | `SYS_PROC` 位 |
| `test_dispatch_times_self_replacement` | TIMES SELF 替换 + reply 填充 | do_times.c:33-34 |
| `test_dispatch_setalarm_reset_timer` | SETALARM 非 SYS_PROC 返回 EPERM（reset 路径前置检查） | do_setalarm.c:33 |

### 5.2 待补充测试（行为覆盖，DEFERRED）

> 以下测试函数名为设计预期，尚未实现。D5 参数传递使其可注入 mock 状态验证行为（非仅 EPERM 路径）。

| 测试函数（待实现） | 验证行为 | 依赖 |
|---------|---------|------|
| `test_dispatch_stime_sets_boottime` | STIME 设置 boottime | `ClockState::boottime()` |
| `test_dispatch_settime_adjtime` | SETTIME now=0 走 adjtime | `ClockState::set_adjtime_delta` |
| `test_dispatch_settime_set_realtime` | SETTIME now=1 走 set_realtime | `ClockState::set_realtime` |
| `test_dispatch_settime_rejects_non_realtime` | clock_id≠CLOCK_REALTIME 返回 EINVAL | do_settime.c:25-26 |
| `test_dispatch_setalarm_set_timer` | SETALARM 设置 timer 返回 TimerId | `ClockState::set_timer` |
| `test_dispatch_setalarm_time_left` | SETALARM 返回上次剩余 time_left | do_setalarm.c:40-46 |
| `test_dispatch_vtimer_set_virtual` | VTIMER set VT_VIRTUAL 写 virt_left + set flag | do_vtimer.c:60-69 |
| `test_dispatch_vtimer_query_prof` | VTIMER query VT_PROF 返回旧值 | do_vtimer.c:54-58 |

### 5.3 测试注入策略（D5 参数传递的收益）

D5 参数传递的核心收益：测试可构造 `PrivTable::with_sys_proc()` + `ClockState::new()` 注入，验证 set/reset 行为，而非仅 EPERM 路径。例如 `test_dispatch_setalarm_set_timer` 注入含 SYS_PROC 的 `PrivTable` + 空白 `ClockState`，设置 `exp_time=100, abs_time=0`（相对时间），调用 `dispatch_setalarm` 后断言 `kpriv.runtime.s_alarm_timer.is_some()` 验证 timer 已设置。

---

## Ch6: 参见

### 6.1 上游文档（前置依赖）

- [15-clock-timer.md](15-clock-timer.md) — `ClockState`/`TimerAction`/`TimerEntry`/`TimerId` 定义；`vtimer_check`/`tick_virt_timer`/`tick_prof_timer` 实现；`set_timer`/`reset_timer` 方法；adjtime 渐变逻辑消费方（✅ 已同步）
- [13-syscall-dispatch.md](13-syscall-dispatch.md) — 系统调用分发框架，`dispatch_*` 函数接入点；`kernel_call_dispatch` 传入 `&mut ClockState`/`&mut PrivTable`/`&ProcessTable`（✅ 已对接）
- [22-privilege.md](22-privilege.md) — SYS_PROC 权限位定义，`PrivTable`/`KPriv` 结构，`is_sys_proc()` 方法（✅ 已对接）
- [17-syscall-process.md](17-syscall-process.md) — 进程系统调用，`p_time`/`p_misc_flags` 字段；exit 时 vtimer 清理（✅ 已对接）
- [16-smp.md](16-smp.md) — `AtomicU64` 的 SMP 安全性，BKL 与时钟状态访问（✅ 已对接）

### 6.3 redox 对比

minix-rs 用参数传入 `ClockState` (D5) 替代 redox 全局 `time::` 接口、`TimerAction::NotifyAlarm` 替代信号重排队、内核 tick `AtomicU64` 递减替代用户态驱动、保留 POSIX adjtime。

---

## 附录 A: C↔Rust 差异矩阵（anti-translate 与类型增强项）

> 仅列出体现 anti-translate / 类型增强的关键映射；纯语义对齐项（如 `do_times`→`dispatch_times`、`set_boottime`→`set_boottime`）已在 Ch2/Ch4 行内注释体现，此处不重复。

| C 符号 | C 位置 | Rust 表达 | 差异类型 | 理由 |
|--------|--------|----------|---------|------|
| `priv(caller)` | do_setalarm.c:33 | `caller_has_sys_proc_with_table(caller, priv_table)` | anti-translate | 参数传入，非全局 priv() (D5) |
| `priv(caller)->s_alarm_timer` | do_setalarm.c:36 | `Option<(TimerEntry, TimerId)>` | anti-translate | Option 替代 tmr_is_set；TimerId 替代指针 (D1) |
| `reset_kernel_timer(tp)` / `set_kernel_timer(tp,...)` | do_setalarm.c:57,61 | `reset_timer(id)` / `set_timer(entry)` | anti-translate | TimerId 替代指针 (D1) |
| `cause_alarm` 函数指针 | do_setalarm.c:61,69-76 | `TimerAction::NotifyAlarm { endpoint }` | anti-translate | enum 变体替代函数指针 (D2) |
| `VT_WHICH != VT_VIRTUAL && != VT_PROF` | do_vtimer.c:33-34 | `VtimerType::try_from(which)` | 类型增强 | enum + TryFrom 替代整数比较 (D3) |
| `VT_VIRTUAL=1` / `VT_PROF=2` | com.h:420-421 | `VtimerType::Virtual=1` / `Prof=2` | 语义对齐 | 值严格对齐 com.h |
| `&rp->p_virt_left` / `&rp->p_prof_left` | do_vtimer.c:47,50 | `virt_left.load(Relaxed)` / `prof_left.load(Relaxed)` | anti-translate | AtomicU64 替代 clock_t* (D6) |
| `rp->p_user_time` / `rp->p_sys_time` | do_times.c:37-38 | `user_time.load(Relaxed)` / `sys_time.load(Relaxed)` | 类型增强 | AtomicU64 替代 clock_t (D6) |
| `rp->p_misc_flags &/= |= pt_flag` | do_vtimer.c:54,61,65 | `is_set` / `clear` / `set` | 类型增强 | bitflags 替代裸位操作 |
| `vtimer_check(rp)` | do_vtimer.c:81-103 | 在 15-clock-timer 实现（tick-internal） | 跨文档 | D7 职责分离 |

---

## 附录 B: DEFERRED 项

| DEFERRED 项 | 说明 | 跨文档归属 |
|------------|------|-----------|
| `vtimer_check` 递减与发信号 | C `vtimer_check(rp)` (do_vtimer.c:81-103) 的递减 + SIGVTALRM/SIGPROF | 15-clock-timer（`tick_virt_timer`/`tick_prof_timer` + clock.rs 信号分发） |
| adjtime 渐变逻辑消费 | `set_adjtime_delta(ticks)` 写入后的 delta 消费（奇数 tick realtime+=2 等） | 15-clock-timer tick handler |
| `mini_notify(CLOCK, endpoint)` 实现 | `TimerAction::NotifyAlarm` 到期后的通知分发 | 15-clock-timer（ClockState 到期 dispatch） |
| 行为测试（8 个） | STIME/SETTIME/SETALARM/VTIMER 的行为测试（非仅 EPERM 路径） | 待补充（见 §5.2） |
