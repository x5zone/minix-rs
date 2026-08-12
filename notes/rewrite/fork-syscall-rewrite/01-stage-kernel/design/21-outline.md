# 21-syscall-clock-outline.v1.md — 文档结构契约

> **文档**: `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/21-syscall-clock.md`
> **C 源码**: `minix3/minix/kernel/system/do_times.c` (46 行), `do_setalarm.c` (78 行), `do_stime.c` (19 行), `do_settime.c` (58 行), `do_vtimer.c` (103 行)
> **Rust 实现**: `os/kernel/src/syscall_clock.rs` (648 行), `os/kernel/src/clock.rs` (ClockState/TimerAction), `os/kernel/src/proc.rs` (TimeStats: virt_left/prof_left)
> **创建**: 2026-08-01
> **依据**: `21-syscall-clock-glm-structure.md`（知识点全集 + 诊断）
> **方法**: C 源码 → OS 理论 → Rust 对照（非反向）
> **衔接**: ClockState / TimerAction / TimerEntry / TimerId 在 15-clock-timer 定义，21 是其用户态接口

---

## 一、章节骨架与主语

### Ch1 主语：时间/定时器（"用户态如何查询和设置时间、闹钟和定时器？"）

核心问题：**用户态进程如何通过系统调用查询进程时间统计、设置同步闹钟、调整系统时钟、管理虚拟/性能定时器？**

Minix3 的回答：**5 个时钟系统调用构成时间服务的用户态接口**——TIMES 查询、SETALARM 闹钟、STIME/SETTIME 设置、VTIMER 虚拟定时器。它们复用 15-clock-timer 定义的 ClockState/TimerAction 内核基础设施，通过参数传入状态（非全局变量）保证可测试性。

| 节 | 标题 | 灵魂本质（一句话） | 概念组 |
|----|------|-------------------|--------|
| §1.1 | 时间查询：TIMES | "TIMES 返回进程的用户态/系统态 CPU 时间统计，加 monotonic/realtime/boottime 三时钟源——三时钟源语义不可混淆" | A |
| §1.2 | 同步闹钟：SETALARM | "每个 system process 一个 s_alarm_timer，到期通过 cause_alarm → mini_notify(CLOCK, endpoint) 通知——闹钟挂在 priv 结构上" | B, E |
| §1.3 | 时间设置：STIME + SETTIME | "STIME 设 boottime；SETTIME 双模式——adjtime 渐变调整 vs set_realtime 直接设置，仅 CLOCK_REALTIME 可改" | C |
| §1.4 | 虚拟/性能定时器：VTIMER | "VT_VIRTUAL 计用户态时间、VT_PROF 计用户+系统时间，virt_left/prof_left 递减到 0 时触发 SIGVTALRM/SIGPROF——vtimer_check 在时钟中断检查到期" | D, E |

### Ch2 主语：C 源码符号（file:line 锚定）

每节以 C 函数为单元，附 file:line，说明语义与调用关系。

### Ch3 主语：设计决策（hypothesis-driven）

采用"如果 X 设计会有 Y 问题所以用 Z"格式，禁止"旧版/最初/后来/我们改成"迭代叙事。

### Ch4 主语：Rust 实现（真实代码，非 stub）

贴 syscall_clock.rs 真实代码片段，标注 file:line。vtimer_check 标注在 15-clock-timer 实现（跨文档衔接）。

### Ch5 主语：测试函数（可 grep 验证）

列出实际 `fn test_*` 函数名，每个测试对应一个被测行为。

---

## 二、详细大纲

### Ch1. 概念建构（concept-driven）

#### §1.1 时间查询：TIMES

**灵魂本质**: TIMES 返回进程的用户态/系统态 CPU 时间统计，加 monotonic/realtime/boottime 三时钟源——三时钟源语义不可混淆。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 用户态需要知道进程消耗了多少 CPU 时间（性能分析、计费），以及当前系统时间（墙上时钟、启动时长）。这两个需求对应不同时钟源，不可混淆。
- **WHAT**: TIMES 一次调用返回 5 个值：`user_time`（用户态 tick）、`system_time`（系统态 tick）、`boot_ticks`（monotonic，启动后 tick）、`real_ticks`（realtime，墙上时钟 tick）、`boot_time`（boottime，启动 Unix 时间戳）。
- **HOW**: C `do_times` (do_times.c:22-44) 先做 SELF 替换（L33-34），若 endpoint 有效则读 `rp->p_user_time`/`rp->p_sys_time` (L37-38)，最后无条件填三时钟源 `get_monotonic()`/`get_realtime()`/`get_boottime()` (L40-42)。

**三时钟源语义**（关键不可混淆）:
| 时钟源 | C 函数 | 语义 | 可设置? |
|--------|--------|------|---------|
| monotonic | `get_monotonic()` (do_times.c:40) | 启动后 tick 数，单调递增 | 否 |
| realtime | `get_realtime()` (do_times.c:41) | 墙上时钟 tick 数（=boottime 对应的 Unix tick + monotonic） | 是（SETTIME） |
| boottime | `get_boottime()` (do_times.c:42) | 系统启动时的 Unix 时间戳 | 是（STIME） |

**SELF 语义**: `endpt == SELF` (-2) 表示"查询自己"，内核替换为 `caller->p_endpoint` (do_times.c:33-34)。`endpt == NONE` 表示不查询特定进程时间，仅返回时钟值 (do_times.c:35)。

**并发安全**: do_times.c:29-32 注释指出时钟中断 handler 可能并发更新时间字段，但单字段读是原子的。Rust 用 `AtomicU64::load(Relaxed)` 天然表达此语义。

#### §1.2 同步闹钟：SETALARM

**灵魂本质**: 每个 system process 一个 s_alarm_timer，到期通过 cause_alarm → mini_notify(CLOCK, endpoint) 通知——闹钟挂在 priv 结构上。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 系统进程需要内核级定时通知（用户进程用 SIGALRM 经 PM，系统进程直接用内核闹钟）。闹钟需关联到进程的 priv 结构（每个 system process 独立）。
- **WHAT**: SETALARM 设置/取消调用者的同步闹钟。返回上次闹钟剩余 `time_left` + 当前 `uptime`。`abs_time=0,exp_time=0` 取消；`abs_time=0,exp_time>0` 相对时间；`abs_time=1` 绝对时间。
- **HOW**: C `do_setalarm` (do_setalarm.c:22-64) 先 SYS_PROC 权限检查 (L33)，取 `priv(caller)->s_alarm_timer` (L36)，算 `time_left` (L40-46)，返回 `uptime` (L49)，最后 set/reset timer (L56-62)。到期回调 `cause_alarm` (do_setalarm.c:69-76) 调 `mini_notify(proc_addr(CLOCK), proc_nr_e)` (L75)。

**权限**: 仅 `SYS_PROC` 进程可调用 (do_setalarm.c:33)。用户进程的 SIGALRM 由 PM 转发，不直接走 SETALARM。

**time_left 三分支** (do_setalarm.c:40-46):
- timer 未设 → `TMR_NEVER`
- timer 未到期（`tmr_is_first(uptime, exp_time)`）→ `exp_time - uptime`
- timer 已到期 → `0`

**绝对/相对时间** (do_setalarm.c:56-61):
- `!abs_time && exp_time==0` → `reset_kernel_timer(tp)` 取消
- `!abs_time && exp_time>0` → `exp_time += uptime` 转绝对
- `abs_time` → 直接用 `exp_time`
- 设置时 `set_kernel_timer(tp, exp_time, cause_alarm, caller->p_endpoint)` (L61)

#### §1.3 时间设置：STIME + SETTIME

**灵魂本质**: STIME 设 boottime；SETTIME 双模式——adjtime 渐变调整 vs set_realtime 直接设置，仅 CLOCK_REALTIME 可改。

**STIME** (do_stime.c:15-18):
- 语义：设置启动时间 Unix 时间戳，由 VM 在初始化时调用
- 实现：`set_boottime(m_ptr->m_lsys_krn_sys_stime.boot_time)` (L17)

**SETTIME 双模式** (do_settime.c:18-57):

| 模式 | 条件 | 行为 | C 行号 |
|------|------|------|--------|
| adjtime | `now == 0` | 渐变调整：`set_adjtime_delta(ticks)`，ticks = sec*hz + nsec/(1e9/hz) | L29-34 |
| set time | `now != 0` | 直接设置：算 `timediff=sec-boottime`，`set_realtime(newclock)` | L37-55 |

**约束**:
- 仅 `CLOCK_REALTIME` 可改 (do_settime.c:25-26)，monotonic 不可设
- set time 模式防负值：`sec <= boottime` 或 `timediff_ticks` 越界 → 修正 boottime + `set_realtime(1)` (do_settime.c:43-48)

**adjtime 为何保留**: POSIX adjtime(2) 语义——渐变调整时钟避免时间跳变（NTP 场景）。删除会破坏 POSIX 完整性。

#### §1.4 虚拟/性能定时器：VTIMER

**灵魂本质**: VT_VIRTUAL 计用户态时间、VT_PROF 计用户+系统时间，virt_left/prof_left 递减到 0 时触发 SIGVTALRM/SIGPROF——vtimer_check 在时钟中断检查到期。

**两类定时器** (do_vtimer.c:33-34, com.h:420-421):

| 类型 | C 常量 | 值 | 计数范围 | 到期信号 | 字段 | 标志 |
|------|--------|---|---------|---------|------|------|
| 虚拟 | `VT_VIRTUAL` | 1 | 用户态时间 | `SIGVTALRM` | `p_virt_left` | `MF_VIRT_TIMER` |
| 性能 | `VT_PROF` | 2 | 用户+系统时间 | `SIGPROF` | `p_prof_left` | `MF_PROF_TIMER` |

**WHY → WHAT → HOW 弧线**:
- **WHY**: 进程需要"按 CPU 时间非墙上时间"的定时器（profiling、用户态 CPU 限制）。VT_VIRTUAL 仅计用户态，VT_PROF 计用户+系统，区分用途。
- **WHAT**: VTIMER 设置/查询进程的虚拟/性能定时器。`VT_SET=0` 查询旧值；`VT_SET=1` 设置新值（value>0 启用，value=0 禁用）。返回旧值。
- **HOW**: C `do_vtimer` (do_vtimer.c:21-74) 先 SYS_PROC 检查 (L31)，验证 `VT_WHICH` (L33-34)，SELF 替换 + `isokendpt` (L37-38)，确定 `pt_flag`/`pt_left` (L45-51)，取旧值 (L54-58)，若 VT_SET 则 clear flag → set/clear value → set flag (L60-69)，返回旧值 (L71)。

**vtimer_check 到期机制** (do_vtimer.c:81-103):
- 时钟中断调用 `vtimer_check(rp)` (L81)
- `MF_VIRT_TIMER && p_virt_left==0` → 清标志 + `cause_sig(rp->p_nr, SIGVTALRM)` (L91-95)
- `MF_PROF_TIMER && p_prof_left==0` → 清标志 + `cause_sig(rp->p_nr, SIGPROF)` (L98-102)
- 并发安全 (L83-88)：clock handler 只递减 `p_virt_left/p_prof_left`，不修改 `p_misc_flags`，故 vtimer_check 无需锁

**权限**: 仅 `SYS_PROC` 可调用 (do_vtimer.c:31)。用户进程经 PM 设置。

---

### Ch2. C 源码分析（file:line 锚定）

#### §2.1 do_times.c (46 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_times(caller, m_ptr)` | do_times.c:22-44 | TIMES 主函数 |
| SELF 替换 | do_times.c:33-34 | `endpt==SELF ? caller->p_endpoint : endpt` |
| endpoint 校验 | do_times.c:35 | `e_proc_nr != NONE && isokendpt(...)` |
| user/sys time 读取 | do_times.c:37-38 | `rp->p_user_time` / `rp->p_sys_time` |
| boot_ticks | do_times.c:40 | `get_monotonic()` |
| real_ticks | do_times.c:41 | `get_realtime()` |
| boot_time | do_times.c:42 | `get_boottime()` |

#### §2.2 do_setalarm.c (78 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_setalarm(caller, m_ptr)` | do_setalarm.c:22-64 | SETALARM 主函数 |
| 参数提取 | do_setalarm.c:31-32 | `exp_time` / `use_abs_time` |
| SYS_PROC 检查 | do_setalarm.c:33 | `priv(caller)->s_flags & SYS_PROC` 否则 EPERM |
| timer 获取 | do_setalarm.c:36 | `tp = &(priv(caller)->s_alarm_timer)` |
| uptime | do_setalarm.c:39 | `get_monotonic()` |
| time_left 计算 | do_setalarm.c:40-46 | 未设→TMR_NEVER；未到期→差值；到期→0 |
| 返回 uptime | do_setalarm.c:49 | `m_ptr->...uptime = uptime` |
| reset timer | do_setalarm.c:56-57 | `!abs_time && exp_time==0` → `reset_kernel_timer(tp)` |
| set timer | do_setalarm.c:58-61 | `set_kernel_timer(tp, exp_time, cause_alarm, caller->p_endpoint)` |
| `cause_alarm(proc_nr_e)` | do_setalarm.c:69-76 | 闹钟到期回调 |
| mini_notify | do_setalarm.c:75 | `mini_notify(proc_addr(CLOCK), proc_nr_e)` |

#### §2.3 do_stime.c (19 行) + do_settime.c (58 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_stime(caller, m_ptr)` | do_stime.c:15-18 | STIME：`set_boottime(boot_time)` (L17) |
| `do_settime(caller, m_ptr)` | do_settime.c:18-57 | SETTIME 主函数 |
| CLOCK_REALTIME 检查 | do_settime.c:25-26 | `clock_id != CLOCK_REALTIME → EINVAL` |
| adjtime 模式 | do_settime.c:29-34 | `now==0` → `set_adjtime_delta(ticks)` |
| ticks 转换 | do_settime.c:31-32 | `sec*system_hz + nsec/(1e9/system_hz)` |
| set time 模式 | do_settime.c:37-55 | `now!=0` → `set_realtime(newclock)` |
| timediff 计算 | do_settime.c:39-40 | `timediff=sec-boottime` / `timediff_ticks=timediff*hz` |
| 负值保护 | do_settime.c:43-48 | `sec<=boottime || 越界` → 修正 boottime + `set_realtime(1)` |
| newclock 计算 | do_settime.c:52-53 | `timediff_ticks + nsec/(1e9/system_hz)` |
| set_realtime | do_settime.c:55 | `set_realtime(newclock)` |

#### §2.4 do_vtimer.c (103 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_vtimer(caller, m_ptr)` | do_vtimer.c:21-74 | VTIMER 主函数 |
| SYS_PROC 检查 | do_vtimer.c:31 | `priv(caller)->s_flags & SYS_PROC` 否则 EPERM |
| VT_WHICH 验证 | do_vtimer.c:33-34 | `!= VT_VIRTUAL && != VT_PROF → EINVAL` |
| SELF 替换 | do_vtimer.c:37 | `endpt==SELF ? caller->p_endpoint : endpt` |
| isokendpt 校验 | do_vtimer.c:38 | `isokendpt(proc_nr_e, &proc_nr)` |
| pt_flag/pt_left 确定 | do_vtimer.c:45-51 | VT_VIRTUAL→`MF_VIRT_TIMER`/`&p_virt_left`；VT_PROF→`MF_PROF_TIMER`/`&p_prof_left` |
| 旧值读取 | do_vtimer.c:54-58 | `p_misc_flags & pt_flag ? *pt_left : 0` |
| VT_SET 语义 | do_vtimer.c:60-69 | clear flag → value>0? set pt_left+set flag : pt_left=0 |
| 旧值返回 | do_vtimer.c:71 | `m_ptr->VT_VALUE = old_value` |
| `vtimer_check(rp)` | do_vtimer.c:81-103 | 时钟中断调用，检查到期 |
| VIRT 到期 | do_vtimer.c:91-95 | `MF_VIRT_TIMER && virt_left==0` → 清标志 + `cause_sig(SIGVTALRM)` |
| PROF 到期 | do_vtimer.c:98-102 | `MF_PROF_TIMER && prof_left==0` → 清标志 + `cause_sig(SIGPROF)` |
| 并发注释 | do_vtimer.c:83-88 | clock handler 只递减不设标志，无需锁 |

#### §2.5 调用关系图

**SETALARM 闹钟生命周期**:
```
用户态: sys_setalarm(exp_time, abs_time)
  └─ kernel: dispatch_setalarm [syscall_clock.rs:180]
       ├─ SYS_PROC 检查
       ├─ 取 s_alarm_timer, 算 time_left
       ├─ set_timer(TimerEntry{NotifyAlarm{endpoint}}) → TimerId
       └─ 存储 (entry, id) 到 priv.runtime.s_alarm_timer

时钟中断到期:
  └─ ClockState::collect_expired_timers [clock.rs:773]
       └─ pop_expired → TimerAction::NotifyAlarm{endpoint}
            └─ mini_notify(CLOCK, endpoint)  [对应 C cause_alarm do_setalarm.c:75]
```

**VTIMER 到期生命周期**:
```
用户态: sys_vtimer(VT_VIRTUAL, VT_SET, value, endpt)
  └─ kernel: dispatch_vtimer [syscall_clock.rs:432]
       ├─ SYS_PROC 检查
       ├─ store(value) 到 virt_left + set(VIRT_TIMER)
       └─ 返回旧值

时钟中断 tick:
  └─ tick handler [clock.rs:901]
       ├─ tick_virt_timer() → virt_left 递减 [proc.rs:610]
       └─ 递减到 0 → SIGVTALRM  [对应 C vtimer_check do_vtimer.c:91-95]
```

---

### Ch3. 设计决策（hypothesis-driven）

#### D1. SETALARM 定时器表达：minix_timer_t 链表 vs BTreeMap dual-index

**假设性推理**:
- 如果用 C 的 `minix_timer_t` 链表（`tp->tmr_next`）：Rust 无内置链表节点嵌入模式，且链表删除需 O(N) 扫描或维护双向指针，内存安全难保证。
- 如果用 `BTreeMap<u64, TimerEntry>`（exp_time 作 key）：同 exp_time 的多 timer 会覆盖——C 链表支持同 exp_time，POSIX 不要求但内核场景需要。
- 所以用 `BTreeSet<(u64, TimerId)>` + `BTreeMap<TimerId, TimerEntry>` dual-index（与 15-clock-timer D2 一致）：BTreeSet 按 (exp_time, id) 排序支持到期扫描，BTreeMap 按 TimerId O(log N) 查找支持 reset_timer(id)。

**实现**: `clock_state.set_timer(entry) -> TimerId` / `reset_timer(id)` (syscall_clock.rs:230,256)。`s_alarm_timer: Option<(TimerEntry, TimerId)>` 存储 id 用于后续 reset。

#### D2. cause_alarm 回调：函数指针 vs TimerAction enum

**假设性推理**:
- 如果用 C 的函数指针 `tmr_func_t`（do_setalarm.c:61 `cause_alarm`）：Rust 函数指针 `fn(i32)` 无闭包捕获，endpoint 需额外存 timer 结构体；类型不安全，任何函数指针都能传入。
- 如果用 trait object `Box<dyn TimerCallback>`：堆分配 + 动态分发，no_std 下需 alloc 且增加间接调用开销。
- 所以用 `TimerAction` enum（与 15-clock-timer D6 一致）：`NotifyAlarm { endpoint }` 变体携带 endpoint，enum 分发编译期穷尽，无堆分配。

**实现**: `TimerAction::NotifyAlarm { endpoint: caller.p_endpoint }` (syscall_clock.rs:243-245)。到期时 ClockState 弹出 action，dispatch 到 `mini_notify(CLOCK, endpoint)`。

#### D3. VT_WHICH 表达：整数 vs VtimerType enum

**假设性推理**:
- 如果用裸 `i32`（C 方式 do_vtimer.c:33）：`which != VT_VIRTUAL && != VT_PROF` 魔法数字比较，易写错值（如把 VT_PROF 误写为 1）。
- 如果用 `const` 常量：仍是整数，无类型安全，函数参数无法区分"任意 i32"与"vtimer 类型"。
- 所以用 `VtimerType` enum + `TryFrom<i32>`：编译期穷尽，`try_from(which)` 返回 `Result`，非法值 `Err(()) → EINVAL`。值 `Virtual=1, Prof=2` 对齐 C `com.h:420-421`。

**实现**: `enum VtimerType { Virtual=1, Prof=2 }` + `impl TryFrom<i32>` (syscall_clock.rs:46-65)。

#### D4. SETTIME adjtime：删除 vs 保留

**假设性推理**:
- 如果删除 adjtime 模式（仅保留 set_realtime）：失去 POSIX adjtime(2) 语义——渐变调整时钟避免时间跳变，NTP 场景必需。删除会破坏 POSIX 完整性，且 C ground truth 有完整实现 (do_settime.c:29-34)。
- 如果保留但简化（不实现渐变逻辑）：adjtime_delta 字段无消费者，等于死代码。
- 所以保留完整 adjtime 模式：`set_adjtime_delta(ticks)` 写入 ClockState，渐变逻辑在 15-clock-timer 的 tick handler 消费 delta。

**实现**: `clock_state.set_adjtime_delta(ticks)` (syscall_clock.rs:387)，ticks = `sec*hz + nsec/(1e9/hz)` (do_settime.c:31-32)。

#### D5. ClockState 访问：全局变量 vs 参数传递（核心 anti-translate）

**假设性推理**:
- 如果用全局变量访问 ClockState（C 方式 do_setalarm.c:39 `get_monotonic()` 全局 / L57 `reset_kernel_timer(tp)` 隐式全局时钟）：测试时无法注入 mock ClockState——`get_monotonic()` 读全局 atomic，`reset_kernel_timer` 操作全局 timer 队列。单元测试无法隔离时间状态，只能测权限路径（EPERM），无法测 set/reset 行为。
- 如果用 `thread_local!`：no_std 无线程，且 SMP 内核无 thread-local 语义。
- 如果用 trait + 全局单例：仍是全局，测试需替换全局状态，并发不安全。
- 所以用参数传递：`dispatch_setalarm(caller, msg, priv_table: &mut PrivTable, clock_state: &mut ClockState)` 显式传入状态。测试可构造 `ClockState::new()` + `PrivTable::new()` 注入，验证 set/reset 行为。

**实现**: 所有 dispatch 函数接受 `&mut ClockState` / `&mut PrivTable` / `&ProcessTable` 参数 (syscall_clock.rs:180-185, 432-437)。`caller_has_sys_proc_with_table(caller, priv_table)` 同样参数传入 (syscall_clock.rs:293)。

#### D6. virt_left/prof_left 存储：Cell/u64 vs AtomicU64

**假设性推理**:
- 如果用 `Cell<u64>`：`Cell` 非 `Sync`，无法跨 CPU 共享（SMP 内核硬约束），`&KProcess` 无法传递到其他 CPU。
- 如果用 `u64` + 锁：vtimer 字段高频读写（每个 tick 递减），锁开销大且 vtimer_check 注释 (do_vtimer.c:83-88) 明确"无需锁"。
- 如果用 `RefCell<u64>`：同 Cell，非 Sync。
- 所以用 `AtomicU64`：`Sync` + 无锁，`compare_exchange_weak` 递减 (proc.rs:617,633)，SMP 安全。C 靠注释约定"clock handler 只递减"的并发安全，Rust 用原子操作编译期保证。

**实现**: `TimeStats.virt_left: AtomicU64` / `prof_left: AtomicU64` (proc.rs:588-589)。dispatch_vtimer 用 `load(Relaxed)` 读 / `store(Release)` 写 (syscall_clock.rs:480,489,506,509,517,520)。

#### D7. vtimer_check：standalone 函数 vs tick-internal

**假设性推理**:
- 如果保留 C 的 standalone `vtimer_check(rp)` 函数 (do_vtimer.c:81-103)：需在 21 文档重复实现，但 vtimer_check 由时钟中断调用，属于 15-clock-timer 的 tick handler 职责。21 是用户态接口（set/query），15 是内核 tick 机制（递减/到期），职责分离。
- 如果在 21 实现 vtimer_check：跨文档职责混乱，21 依赖 15 的 tick 调度。
- 所以 vtimer_check 语义内联到 15-clock-timer 的 tick handler（与 15 D11 一致）：`tick_virt_timer()`/`tick_prof_timer()` 递减并返回是否到期 (proc.rs:610,626)，clock.rs:901,907 调用并处理 SIGVTALRM/SIGPROF。21 仅负责 set/query 接口。

**实现**: 21 文档 Ch4 标注 vtimer_check 在 15-clock-timer 实现（跨文档衔接），Ch6 参见引用 15。

---

### Ch4. 实现详解（真实代码）

#### §4.1 dispatch_times — 时间查询

贴 `syscall_clock.rs:108-163` 的 `dispatch_times`，标注：
- SELF 替换 (L120-124) 对应 do_times.c:33-34
- AtomicU64 读取 user/sys time (L132-133) 对应 do_times.c:37-38
- 三时钟源 (L148-152) 对应 do_times.c:40-42
- reply 写入 (L160) 对应 do_times.c:37-42

#### §4.2 dispatch_setalarm — 同步闹钟

贴 `syscall_clock.rs:180-275` 的 `dispatch_setalarm`，标注：
- SYS_PROC 检查 (L194) 对应 do_setalarm.c:33
- `s_alarm_timer` 读取 (L209-218) 对应 do_setalarm.c:40-46
- `TimerAction::NotifyAlarm { endpoint }` (L243-245) 对应 do_setalarm.c:61,69-76
- `clock_state.set_timer` / `reset_timer` (L230,256) 对应 do_setalarm.c:57,61
- 参数传入 `&mut ClockState` (L184) — D5 anti-translate

**删除"D5 实现说明 (2026-06-15 更新)"迭代叙事**，改为事实陈述：dispatch_setalarm 接受 `&mut PrivTable, &mut ClockState` 参数，完整实现 do_setalarm.c:35-78 语义。

#### §4.3 dispatch_stime + dispatch_settime — 时间设置

贴 `syscall_clock.rs:330-344` (stime) + `L361-415` (settime)，标注：
- `set_boottime(boot_time)` (L341) 对应 do_stime.c:17
- CLOCK_REALTIME 检查 (L376-378) 对应 do_settime.c:25-26
- adjtime 模式 (L383-389) 对应 do_settime.c:29-34
- set time 模式 (L392-414) 对应 do_settime.c:37-57
- 负值保护 (L400-408) 对应 do_settime.c:43-48

#### §4.4 dispatch_vtimer — 虚拟/性能定时器

贴 `syscall_clock.rs:432-535` 的 `dispatch_vtimer`，**展示真实 AtomicU64 代码**（非 `/* virt_left */` 占位）：
- `VtimerType::try_from(which)` (L451) 对应 do_vtimer.c:33-34
- SELF 替换 + endpoint 校验 (L457-467) 对应 do_vtimer.c:37-39
- **真实 virt_left/prof_left 读写** (L480,489,506,509,517,520)：
  ```rust
  let old = if target.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER) {
      target.p_time.virt_left.load(Ordering::Relaxed)  // 真实代码，非占位
  } else { 0 };
  // ...
  target.p_time.virt_left.store(value, Ordering::Release);  // 真实代码
  ```
- MiscFlagsBits set/clear (L500,512) 对应 do_vtimer.c:61-66

**VtimerType 值修正**: `Virtual = 1, Prof = 2`（对齐 com.h:420-421），非 0/1。

**vtimer_check 跨文档衔接**: 标注 vtimer_check (do_vtimer.c:81-103) 在 15-clock-timer 实现（tick_virt_timer/tick_prof_timer + SIGVTALRM/SIGPROF）。

#### §4.5 caller_has_sys_proc_with_table — 权限检查

贴 `syscall_clock.rs:293-301`，标注：
- `caller.priv_id` + `priv_table.get(id).is_sys_proc()` 对应 do_setalarm.c:33 / do_vtimer.c:31
- fail-closed: `priv_id=None → false` (L294-296)
- 参数传入 `&PrivTable` — D5 anti-translate

---

### Ch5. 测试（可 grep 函数名）

#### §5.1 现有测试（已实现）

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_vtimer_type_values_match_c` | VtimerType 值对齐 C (Virtual=1, Prof=2) | `VT_VIRTUAL/VT_PROF` com.h:420-421 |
| `test_vtimer_type_try_from` | TryFrom<i32> 合法/非法值 | do_vtimer.c:33-34 |
| `test_clock_realtime` | CLOCK_REALTIME=0 | do_settime.c:25 |
| `test_ksc_caller_has_sys_proc_no_priv_id_rejected` | priv_id=None 拒绝 | do_setalarm.c:33 |
| `test_ksc_caller_has_sys_proc_fresh_table_rejects_all` | 空 PrivTable 拒绝 | do_setalarm.c:33 |
| `test_ksc_setalarm_non_sys_proc_returns_eperm` | SETALARM 非 SYS_PROC 返回 EPERM | do_setalarm.c:33 |
| `test_ksc_vtimer_non_sys_proc_returns_eperm` | VTIMER 非 SYS_PROC 返回 EPERM | do_vtimer.c:31 |
| `test_ksc_priv_flags_sys_proc_bit_definition` | IDL_F/SRV_F 含 SYS_PROC, USR_F 不含 | `SYS_PROC` 位 |
| `test_dispatch_times_self_replacement` | TIMES SELF 替换 + reply 填充 | do_times.c:33-34 |
| `test_dispatch_setalarm_reset_timer` | SETALARM 非 SYS_PROC 返回 EPERM | do_setalarm.c:33 |

#### §5.2 待补充测试（行为覆盖）

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_dispatch_stime_sets_boottime` | STIME 设置 boottime | `ClockState::boottime()` |
| `test_dispatch_settime_adjtime` | SETTIME now=0 走 adjtime | `ClockState::set_adjtime_delta` |
| `test_dispatch_settime_set_realtime` | SETTIME now=1 走 set_realtime | `ClockState::set_realtime` |
| `test_dispatch_settime_rejects_non_realtime` | clock_id≠CLOCK_REALTIME 返回 EINVAL | do_settime.c:25-26 |
| `test_dispatch_setalarm_set_timer` | SETALARM 设置 timer 返回 TimerId | `ClockState::set_timer` |
| `test_dispatch_setalarm_time_left` | SETALARM 返回上次剩余 time_left | do_setalarm.c:40-46 |
| `test_dispatch_vtimer_set_virtual` | VTIMER set VT_VIRTUAL 写 virt_left + set flag | do_vtimer.c:60-69 |
| `test_dispatch_vtimer_query_prof` | VTIMER query VT_PROF 返回旧值 | do_vtimer.c:54-58 |

---

### Ch6. 参见

- [15-clock-timer.md](15-clock-timer.md) — ClockState, TimerAction, TimerEntry, TimerId 定义；vtimer_check/tick_virt_timer 实现；set_timer/reset_timer 方法
- [13-syscall-dispatch.md](13-syscall-dispatch.md) — 系统调用分发框架，dispatch_* 函数接入点
- [22-privilege.md](22-privilege.md) — SYS_PROC 权限位定义，PrivTable/KPriv 结构
- [17-syscall-process.md](17-syscall-process.md) — 进程系统调用，p_time/p_misc_flags 字段
- [16-smp.md](16-smp.md) — AtomicU64 的 SMP 安全性，BKL 与时钟状态访问

---

## 三、知识点覆盖矩阵

| 概念组 | Ch1 | Ch2 | Ch3 | Ch4 | Ch5 |
|--------|-----|-----|-----|-----|-----|
| A. 时间查询 | §1.1 | §2.1 do_times.c | D6 (AtomicU64) | §4.1 dispatch_times | test_dispatch_times_* |
| B. 同步闹钟 | §1.2 | §2.2 do_setalarm.c | D1, D2 | §4.2 dispatch_setalarm | test_ksc_setalarm_* |
| C. 时间设置 | §1.3 | §2.3 do_stime/settime.c | D4 | §4.3 dispatch_stime/settime | test_dispatch_stime/settime_* |
| D. vtimer | §1.4 | §2.4 do_vtimer.c | D3, D6, D7 | §4.4 dispatch_vtimer | test_vtimer_* / test_ksc_vtimer_* |
| E. 权限 | §1.2,§1.4 | §2.2,§2.4 | D5 | §4.5 caller_has_sys_proc | test_ksc_* |
| F. anti-translate | (贯穿) | (贯穿) | D1-D7 | §4 全部 | (贯穿) |

---

## 四、断裂修复表

| 断裂点 | 修复方案 |
|--------|---------|
| ClockState 参数传递 (F.2) Ch3 未讲 | Ch3 D5 新增 hypothesis："如果用全局变量访问 ClockState 会有什么问题？→ 不可测试 → 参数传递" |
| virt_left/prof_left 占位 (D.4) | Ch4 §4.4 贴真实 `AtomicU64::load/store` 代码 (syscall_clock.rs:480,489,506,509,517,520)，删除 `/* virt_left */` 占位 |
| VtimerType 值错误 (D.0) | Ch4 §4.4 修正为 Virtual=1/Prof=2（对齐 com.h:420-421） |
| STIME/SETTIME 测试断裂 (C 组) | Ch5 §5.2 补 test_dispatch_stime_sets_boottime / test_dispatch_settime_adjtime / test_dispatch_settime_set_realtime |
| SETALARM 行为测试断裂 (B 组) | Ch5 §5.2 补 test_dispatch_setalarm_set_timer / test_dispatch_setalarm_time_left |
| VTIMER 行为测试断裂 (D 组) | Ch5 §5.2 补 test_dispatch_vtimer_set_virtual / test_dispatch_vtimer_query_prof |
| vtimer_check 跨文档 (D.7) | Ch3 D7 说明 vtimer_check 在 15 实现；Ch4 §4.4 标注；Ch6 参见 15 |
| §4.2 迭代叙事 (D5 实现说明) | Ch4 §4.2 删除"D5 实现说明 (2026-06-15 更新)"，改事实陈述 |
| Ch3 平庸决策表 | Ch3 改为 hypothesis-driven（D1-D7 每个有"如果 X 会有 Y 问题所以用 Z"） |
| 测试不可 grep | Ch5 §5.1 列出 10 个实际 `fn test_*` 函数名 |
| 测试节迭代叙事 | Ch5 删除"测试状态 (2026-06-15 更新)"，改为可 grep 函数名清单 |

---

## 五、自检

- [x] Ch1 主语是时间/定时器，非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch1 采用 WHY→WHAT→HOW 弧线（§1.1 TIMES 为典型）
- [x] Ch2 每个符号带 file:line
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] Ch3 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] Ch3 D5 核心推理："如果用全局变量访问 ClockState 会有什么问题？→ 不可测试 → 参数传递"
- [x] Ch4 贴真实代码，virt_left/prof_left 非 `/* */` 占位
- [x] Ch4 VtimerType 值为 Virtual=1/Prof=2（对齐 com.h:420-421）
- [x] Ch4 删除"D5 实现说明 (2026-06-15 更新)"迭代叙事
- [x] Ch4 vtimer_check 标注在 15-clock-timer 实现（跨文档衔接）
- [x] Ch5 测试函数可 grep 验证（`fn test_*`）
- [x] 知识点覆盖矩阵完整（A-F 六组）
- [x] 断裂修复表完整（11 处断裂 + 修复方案）
- [x] 无 tmp 文件引用
- [x] 无内部 review ID（P0-XX/P1-XX）
- [x] 无迭代叙事日期（2026-XX-XX）
- [x] anti-translate 体现（VtimerType enum/TimerAction/ClockState 参数传递/AtomicU64）
- [x] 与 15-clock-timer 衔接（ClockState/TimerAction 在 15 定义，21 是用户态接口）
