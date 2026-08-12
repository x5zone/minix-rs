# 21-syscall-clock Design（设计文档）

> **状态**: 完整设计（基于 21-outline.v1.md 经 outline-review 批准）
> **创建**: 2026-08-01
> **作者**: Trae (GLM-5.2)
> **前置**: 15-clock-timer.md（ClockState/TimerAction/TimerEntry/TimerId 定义）, 13-syscall-dispatch.md, 22-privilege.md, 17-syscall-process.md, 16-smp.md
> **C 源码**: `minix3/minix/kernel/system/do_times.c` (46 行), `do_setalarm.c` (78 行), `do_stime.c` (19 行), `do_settime.c` (58 行), `do_vtimer.c` (103 行)
> **Rust 实现**: `os/kernel/src/syscall_clock.rs` (648 行), `os/kernel/src/clock.rs` (ClockState/TimerAction), `os/kernel/src/proc.rs` (TimeStats: virt_left/prof_left)

---

## §1. 设计目标与约束

### 1.1 目标

重写 `os/kernel/src/syscall_clock.rs` 及其文档，使其：
1. **对齐 C ground truth**: `do_times.c` (46 行) + `do_setalarm.c` (78 行) + `do_stime.c` (19 行) + `do_settime.c` (58 行) + `do_vtimer.c` (103 行) 的完整语义，覆盖 TIMES/SETALARM/STIME/SETTIME/VTIMER 五个系统调用
2. **修复 review 发现的问题**: §4.2 "D5 实现说明 (2026-06-15 更新)" 迭代叙事；§4.3 VTIMER `/* virt_left */` 占位；测试 bullet 不可 grep；Ch3 平庸决策表；§4.1 VtimerType 值错误 (0/1 应为 1/2)
3. **避免 translate**: 用 Rust 类型系统重新表达 C 的整数类型（`VtimerType` enum）、函数指针（`TimerAction::NotifyAlarm`）、全局状态访问（ClockState 参数传递）、非原子字段（`AtomicU64` for virt_left/prof_left）
4. **与 15-clock-timer 衔接**: ClockState/TimerAction/TimerEntry/TimerId 在 15 定义，21 是其用户态接口；vtimer_check 在 15 实现（tick-internal）

### 1.2 约束

- `#![no_std]`（除 `#[cfg(test)]`）
- 所有 dispatch 函数接受 `&mut ClockState` / `&mut PrivTable` / `&ProcessTable` 参数（非全局变量）
- `virt_left`/`prof_left` 用 `AtomicU64`（SMP 安全，`Sync`）
- 代码注释引用 C 源码 `file:line`
- 无迭代叙事（"旧版/最初/后来/我们改成/D5 实现说明 (2026-XX-XX 更新)"）
- 无 `/* virt_left */` 占位，贴真实 AtomicU64 代码
- VtimerType 值对齐 C `com.h:420-421`（Virtual=1, Prof=2）

### 1.3 Ground Truth 验证

| C 函数 | 行号 | 职责 | Rust 归属 |
|--------|------|------|----------|
| `do_times` | do_times.c:22-44 | SELF 替换 → 查询 user/sys time → 三时钟源 | `dispatch_times` ✅ (syscall_clock.rs:108-163) |
| `do_setalarm` | do_setalarm.c:22-64 | SYS_PROC 检查 → time_left → set/reset timer | `dispatch_setalarm` ✅ (syscall_clock.rs:180-275) |
| `cause_alarm` | do_setalarm.c:69-76 | 闹钟到期 → `mini_notify(CLOCK, endpoint)` | `TimerAction::NotifyAlarm { endpoint }` ✅ (syscall_clock.rs:243) |
| `do_stime` | do_stime.c:15-18 | `set_boottime(boot_time)` | `dispatch_stime` ✅ (syscall_clock.rs:330-344) |
| `do_settime` | do_settime.c:18-57 | CLOCK_REALTIME 检查 → adjtime/set_realtime | `dispatch_settime` ✅ (syscall_clock.rs:361-415) |
| `do_vtimer` | do_vtimer.c:21-74 | SYS_PROC 检查 → VT_WHICH → set/query virt_left/prof_left | `dispatch_vtimer` ✅ (syscall_clock.rs:432-535) |
| `vtimer_check` | do_vtimer.c:81-103 | 时钟中断检查到期 → SIGVTALRM/SIGPROF | 在 15-clock-timer 实现（tick-internal, D7）⚠️ 跨文档 |

---

## §2. 核心数据结构与函数设计

### 2.1 VtimerType enum（D3 — 已实现）

```rust
/// Virtual timer type. C: `VT_VIRTUAL` / `VT_PROF` — com.h:420-421
///
/// D3: enum replaces C's integer `VT_WHICH` for type safety.
/// Values align with C: `VT_VIRTUAL = 1`, `VT_PROF = 2` (com.h:420-421).
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

**与 C 的差异**（anti-translate）:
- C `do_vtimer.c:33` 用整数比较 `m_ptr->VT_WHICH != VT_VIRTUAL && != VT_PROF` → Rust `VtimerType::try_from(which)` 返回 `Result`，非法值 `Err(()) → EINVAL`
- 值 `Virtual=1, Prof=2` 严格对齐 `com.h:420-421`（非 0/1）
- `#[repr(i32)]` 保证 FFI 布局兼容性，`as i32` 可安全转回 C 值

### 2.2 dispatch_times — 时间查询（A 组，已实现）

```rust
/// Dispatch SYS_TIMES.
/// C: `do_times()` — do_times.c:22-44
pub fn dispatch_times(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &ProcessTable,
) -> KcallResult {
    // C: do_times.c:28-29 — extract endpoint
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
            if let Some(rp) = proc_table.get(proc_nr) {
                (
                    rp.p_time.user_time.load(Ordering::Relaxed),  // D6: AtomicU64
                    rp.p_time.sys_time.load(Ordering::Relaxed),
                )
            } else { (0, 0) }
        } else { (0, 0) }
    } else { (0, 0) };

    // C: do_times.c:40-42 — always fill three clock sources
    let reply = MessKrnLsysSysTimes {
        boot_ticks: clock::get_monotonic(),   // do_times.c:40
        real_ticks: clock::get_realtime(),    // do_times.c:41
        user_time,
        system_time: sys_time,
        boot_time: clock::get_boottime(),     // do_times.c:42
        _padding: [0u8; 16],
    };
    unsafe { msg.m_u.m_krn_lsys_sys_times = reply; }
    KcallResult::Ok(OK)
}
```

**设计要点**:
- SELF 替换 (L120-124) 对应 do_times.c:33-34
- `AtomicU64::load(Relaxed)` (L132-133) 对应 do_times.c:37-38 — D6 原子读，对应 C 单字段读原子性
- 三时钟源 `get_monotonic/get_realtime/get_boottime` (L148-152) 对应 do_times.c:40-42 — 三者语义不可混淆（见 outline §1.1 表）

### 2.3 dispatch_setalarm — 同步闹钟（B 组，已实现）

```rust
/// Dispatch SYS_SETALARM.
/// C: `do_setalarm()` — do_setalarm.c:22-64
pub fn dispatch_setalarm(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &mut PrivTable,    // D5: 参数传入，非全局 priv()
    clock_state: &mut ClockState,  // D5: 参数传入，非全局时钟
) -> KcallResult {
    // C: do_setalarm.c:31-32 — extract parameters
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
        None => return KcallResult::Ok(EPERM),
    };

    // C: do_setalarm.c:39-46 — return time left on previous alarm
    let uptime = clock_state.uptime();   // C: get_monotonic()
    let time_left = {
        let kpriv = priv_table.get(caller_priv_id);
        if let Some(kpriv) = kpriv {
            match &kpriv.runtime.s_alarm_timer {
                None => TMR_NEVER,                          // tmr_is_set false
                Some((timer, _id)) => {
                    if timer.exp_time > uptime {            // tmr_is_first
                        timer.exp_time - uptime
                    } else { 0 }                            // expired
                }
            }
        } else { TMR_NEVER }
    };

    // C: do_setalarm.c:56-62 — set or reset timer
    if !use_abs_time && exp_time == 0 {
        // Reset alarm: C: reset_kernel_timer(tp)
        let kpriv = priv_table.get_mut(caller_priv_id);
        if let Some(kpriv) = kpriv {
            if let Some((_old_entry, old_id)) = kpriv.runtime.s_alarm_timer.take() {
                clock_state.reset_timer(old_id);   // D1: reset by TimerId
            }
        }
    } else {
        // Set alarm: C: set_kernel_timer(tp, exp_time, cause_alarm, endpoint)
        let actual_exp_time = if use_abs_time { exp_time } else { uptime + exp_time };
        let timer = TimerEntry {
            exp_time: actual_exp_time,
            action: TimerAction::NotifyAlarm {   // D2: enum 替代函数指针
                endpoint: caller.p_endpoint,
            },
        };
        let kpriv = priv_table.get_mut(caller_priv_id);
        if let Some(kpriv) = kpriv {
            if let Some((_old_entry, old_id)) = kpriv.runtime.s_alarm_timer.take() {
                clock_state.reset_timer(old_id);   // 先取消旧 timer
            }
            let id = clock_state.set_timer(timer.clone());  // D1: 返回 TimerId
            kpriv.runtime.s_alarm_timer = Some((timer, id)); // 存储 (entry, id)
        }
    }

    // C: do_setalarm.c:49 — return current uptime + time_left
    let reply = MessLsysKrnSysSetalarm {
        exp_time, time_left, uptime,
        abs_time: if use_abs_time { 1 } else { 0 },
        _padding: [0u8; 28],
    };
    unsafe { msg.m_u.m_lsys_krn_sys_setalarm = reply; }
    KcallResult::Ok(OK)
}
```

**设计要点**:
- D5 参数传递：`&mut PrivTable` + `&mut ClockState` 显式传入，测试可注入 `PrivTable::new()` + `ClockState::new()` 验证 set/reset 行为
- D1 BTreeMap dual-index：`set_timer(entry) -> TimerId` / `reset_timer(id)` 与 15-clock-timer D2 对接；`s_alarm_timer: Option<(TimerEntry, TimerId)>` 存储 id 用于后续 reset
- D2 TimerAction enum：`NotifyAlarm { endpoint }` 替代 C `cause_alarm` 函数指针 (do_setalarm.c:69-76)，到期时 ClockState 弹出 action → `mini_notify(CLOCK, endpoint)`
- time_left 三分支 (L209-218) 对应 do_setalarm.c:40-46

**cause_alarm 对应关系**: C `cause_alarm(proc_nr_e)` (do_setalarm.c:69-76) → `mini_notify(proc_addr(CLOCK), proc_nr_e)` (L75)。Rust 用 `TimerAction::NotifyAlarm { endpoint }` 携带 endpoint，ClockState 到期时 dispatch 到通知。

### 2.4 dispatch_stime + dispatch_settime — 时间设置（C 组，已实现）

```rust
/// Dispatch SYS_STIME. C: `do_stime()` — do_stime.c:15-18
pub fn dispatch_stime(
    _caller: &mut KProcess,
    msg: &Message,
    clock_state: &mut ClockState,   // D5: 参数传入
) -> KcallResult {
    // C: do_stime.c:17 — set_boottime(boot_time)
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_stime };
    let boot_time = req.boot_time;
    clock_state.set_boottime(boot_time);
    KcallResult::Ok(OK)
}

/// Dispatch SYS_SETTIME. C: `do_settime()` — do_settime.c:18-57
pub fn dispatch_settime(
    _caller: &mut KProcess,
    msg: &Message,
    clock_state: &mut ClockState,   // D5: 参数传入
) -> KcallResult {
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
        // D4: 保留 POSIX adjtime 语义
        let ticks = (sec as i64 * hz as i64 + nsec / (1_000_000_000 / hz as i64)) as i32;
        clock_state.set_adjtime_delta(ticks);
        return KcallResult::Ok(OK);
    }

    // C: do_settime.c:37-57 — set time mode (now != 0)
    let boottime = clock_state.boottime();
    let timediff = sec as i64 - boottime as i64;
    let timediff_ticks = timediff * hz as i64;

    // C: do_settime.c:43-48 — prevent negative realtime
    if sec <= boottime
        || timediff_ticks < i32::MIN as i64 / 2
        || timediff_ticks > i32::MAX as i64 / 2
    {
        clock_state.set_boottime(sec);
        clock_state.set_realtime(1);
        return KcallResult::Ok(OK);
    }

    // C: do_settime.c:52-55 — calculate new realtime
    let newclock = (timediff_ticks + nsec / (1_000_000_000 / hz as i64)) as u64;
    clock_state.set_realtime(newclock);
    KcallResult::Ok(OK)
}
```

**设计要点**:
- D4 adjtime 保留：`set_adjtime_delta(ticks)` 写入 ClockState，渐变逻辑在 15-clock-timer tick handler 消费 delta（POSIX adjtime(2) 语义，NTP 场景）
- CLOCK_REALTIME 检查 (L376-378) 对应 do_settime.c:25-26 — monotonic 不可设
- 负值保护 (L400-408) 对应 do_settime.c:43-48 — boottime 错误时修正

### 2.5 dispatch_vtimer — 虚拟/性能定时器（D 组，已实现）

```rust
/// Dispatch SYS_VTIMER. C: `do_vtimer()` — do_vtimer.c:21-74
pub fn dispatch_vtimer(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &PrivTable,     // D5: 参数传入
    proc_table: &ProcessTable,  // D5: 参数传入
) -> KcallResult {
    let m2 = msg_m2(msg);
    // C: do_vtimer.c:30-33 — extract parameters
    let which = m2.m2i1;            // VT_WHICH
    let set = m2.m2i2 != 0;         // VT_SET
    let value = m2.m2l1 as u64;     // VT_VALUE
    let endpt = m2.m2l2 as i32;     // VT_ENDPT

    // C: do_vtimer.c:31 — SYS_PROC permission check
    if !caller_has_sys_proc_with_table(caller, priv_table) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_vtimer.c:33-34 — validate timer type
    let vtype = match VtimerType::try_from(which) {  // D3: enum 替代整数
        Ok(v) => v,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:37-39 — SELF replacement + endpoint validation
    let target_endpoint = if endpt == SELF {
        caller.p_endpoint
    } else {
        Endpoint(endpt)
    };
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };
    let target = match proc_table.get(target_nr) {
        Some(rp) => rp,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:45-58 — determine flag and retrieve old value
    // D6: 真实 AtomicU64 代码，非 /* virt_left */ 占位
    let (pt_flag, old_value) = match vtype {
        VtimerType::Virtual => {
            let flag = MiscFlagsBits::VIRT_TIMER;
            let old = if target.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER) {
                target.p_time.virt_left.load(Ordering::Relaxed)   // 真实 AtomicU64 读
            } else { 0 };
            (flag, old)
        }
        VtimerType::Prof => {
            let flag = MiscFlagsBits::PROF_TIMER;
            let old = if target.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
                target.p_time.prof_left.load(Ordering::Relaxed)   // 真实 AtomicU64 读
            } else { 0 };
            (flag, old)
        }
    };

    // C: do_vtimer.c:60-69 — set new value if VT_SET
    if set {
        target.p_misc_flags.clear(pt_flag);   // C: do_vtimer.c:61 disable first
        if value > 0 {
            // C: do_vtimer.c:64-66 — set new value + re-enable
            match vtype {
                VtimerType::Virtual => {
                    target.p_time.virt_left.store(value, Ordering::Release);  // 真实 AtomicU64 写
                }
                VtimerType::Prof => {
                    target.p_time.prof_left.store(value, Ordering::Release);  // 真实 AtomicU64 写
                }
            }
            target.p_misc_flags.set(pt_flag);
        } else {
            // C: do_vtimer.c:67-68 — clear timer value
            match vtype {
                VtimerType::Virtual => {
                    target.p_time.virt_left.store(0, Ordering::Release);  // 真实 AtomicU64 写
                }
                VtimerType::Prof => {
                    target.p_time.prof_left.store(0, Ordering::Release);  // 真实 AtomicU64 写
                }
            }
        }
    }

    // C: do_vtimer.c:71 — return old value in VT_VALUE
    unsafe { msg.m_u.m_m2.m2l1 = old_value as i64; }
    KcallResult::Ok(OK)
}
```

**设计要点**:
- **真实 AtomicU64 代码**（非 `/* virt_left */` 占位）: `target.p_time.virt_left.load(Ordering::Relaxed)` (L480) / `store(value, Ordering::Release)` (L506) / `prof_left` 同理 (L489,509,517,520)
- D3 VtimerType enum：`try_from(which)` 返回 `Result`，值 `Virtual=1, Prof=2` 对齐 com.h:420-421
- D6 AtomicU64：`virt_left`/`prof_left` 是 `AtomicU64` (proc.rs:588-589)，`Sync` + 无锁，SMP 安全
- MiscFlagsBits bitflags：`is_set`/`set`/`clear` (L479,500,512) 替代 C 裸位操作 `p_misc_flags & pt_flag` (do_vtimer.c:54,61,65)

**virt_left/prof_left 定义**（proc.rs:583-599，跨文档引用）:
```rust
/// Time statistics structure. C: proc.h — p_user_time/p_sys_time/p_virt_left/p_prof_left
#[derive(Debug)]
pub struct TimeStats {
    pub user_time: AtomicU64,
    pub sys_time: AtomicU64,
    pub virt_left: AtomicU64,   // C: p_virt_left — do_vtimer.c:47
    pub prof_left: AtomicU64,   // C: p_prof_left — do_vtimer.c:50
}
```

**vtimer_check 跨文档衔接**（D7）: C `vtimer_check(rp)` (do_vtimer.c:81-103) 在 15-clock-timer 实现（tick-internal）：
- `tick_virt_timer()` (proc.rs:610) CAS 递减 virt_left，到 0 返回 true → SIGVTALRM (clock.rs:901)
- `tick_prof_timer()` (proc.rs:626) CAS 递减 prof_left，到 0 返回 true → SIGPROF (clock.rs:907)
- 21 仅负责 set/query 接口，到期机制在 15

### 2.6 caller_has_sys_proc_with_table — 权限检查（E 组，已实现）

```rust
/// Returns true iff `caller.priv_id` is `Some` AND the matching KPriv
/// entry is a SYS_PROC (i.e. has `PrivFlagsBits::SYS_PROC` set).
///
/// C: do_setalarm.c:33, do_vtimer.c:31 — `priv(caller)->s_flags & SYS_PROC`
///
/// D5: takes `&PrivTable` as parameter (not global `priv()`), enabling
/// test injection of `PrivTable::new()` for EPERM path coverage.
///
/// Fail-closed: if `priv_id` is `None` we conservatively return `false`.
pub(crate) fn caller_has_sys_proc_with_table(
    caller: &KProcess,
    priv_table: &PrivTable,
) -> bool {
    let Some(priv_id) = caller.priv_id else {
        return false;
    };
    priv_table
        .get(priv_id)
        .map(KPriv::is_sys_proc)
        .unwrap_or(false)
}
```

**设计要点**:
- D5 参数传入：`&PrivTable` 显式传入，测试可构造空表验证 fail-closed
- fail-closed 语义：`priv_id=None → false` (L294-296)，对应 C `return(EPERM)` (do_setalarm.c:33)

---

## §3. 设计决策详表（D1-D7，hypothesis-driven）

### D1: SETALARM 定时器表达 — minix_timer_t 链表 vs BTreeMap dual-index

| 选项 | 机制 | 优点 | 缺点 |
|------|------|------|------|
| A. C 链表 `minix_timer_t` | `tp->tmr_next` | C 原生 | Rust 无节点嵌入；O(N) 删除；内存安全难保证 |
| B. `BTreeMap<u64, TimerEntry>` | exp_time 作 key | O(log N) | **同 exp_time 多 timer 覆盖** |
| **C. `BTreeSet<(u64, TimerId)>` + `BTreeMap<TimerId, TimerEntry>`** | dual-index | stable identity + 排序 + 唯一 | 双索引维护 |
| D. `SlotMap<TimerId, TimerEntry>` | SlotMap | O(1) + stable id | no_std 生态弱；外部 crate |

**选定 C**（与 15-clock-timer D2 一致）。理由：BTreeSet 按 (exp_time, id) 排序支持 `pop_expired`，BTreeMap 按 TimerId O(log N) 查找支持 `reset_timer(id)`。timer 队列小（通常 < 64），O(log N) 可接受。

**实现**: `clock_state.set_timer(entry) -> TimerId` / `reset_timer(id)` (syscall_clock.rs:230,256)。`s_alarm_timer: Option<(TimerEntry, TimerId)>` 存储 id。

### D2: cause_alarm 回调 — 函数指针 vs TimerAction enum

| 选项 | 机制 | 优点 | 缺点 |
|------|------|------|------|
| A. C 函数指针 `tmr_func_t` | `cause_alarm` (do_setalarm.c:61) | C 原生 | 无类型安全；endpoint 需额外存 timer |
| B. `Box<dyn TimerCallback>` | trait object | 多态 | 堆分配 + 动态分发；no_std 需 alloc |
| **C. `TimerAction` enum** | `NotifyAlarm { endpoint }` | 类型安全；编译期穷尽；无堆分配 | enum 变体需枚举 |

**选定 C**（与 15-clock-timer D6 一致）。理由：enum 变体携带 endpoint，dispatch 编译期穷尽，无堆分配。

**实现**: `TimerAction::NotifyAlarm { endpoint: caller.p_endpoint }` (syscall_clock.rs:243-245)。

### D3: VT_WHICH 表达 — 整数 vs VtimerType enum

| 选项 | 机制 | 优点 | 缺点 |
|------|------|------|------|
| A. 裸 `i32` (C 方式) | `which != VT_VIRTUAL && != VT_PROF` | 简单 | 魔法数字；易写错值 |
| B. `const` 常量 | `const VT_VIRTUAL: i32 = 1` | 命名 | 仍是整数；无类型安全 |
| **C. `VtimerType` enum + `TryFrom<i32>`** | enum | 编译期穷尽；类型安全 | 需 TryFrom 实现 |

**选定 C**。理由：`try_from(which)` 返回 `Result`，非法值 `Err(()) → EINVAL`。值 `Virtual=1, Prof=2` 严格对齐 `com.h:420-421`。

**实现**: `enum VtimerType { Virtual=1, Prof=2 }` + `impl TryFrom<i32>` (syscall_clock.rs:46-65)。

### D4: SETTIME adjtime — 删除 vs 保留

| 选项 | 机制 | 优点 | 缺点 |
|------|------|------|------|
| A. 删除 adjtime | 仅 set_realtime | 简化 | **失去 POSIX adjtime(2) 语义**；NTP 场景失效 |
| B. 保留但简化 | delta 字段无消费者 | 字段存在 | 死代码 |
| **C. 保留完整** | `set_adjtime_delta(ticks)` + 15 消费 | POSIX 完整 | 跨文档依赖 |

**选定 C**。理由：adjtime(2) 是 POSIX 渐变调整时钟语义（避免时间跳变），NTP 守护进程依赖。C ground truth 有完整实现 (do_settime.c:29-34)。渐变逻辑在 15-clock-timer tick handler 消费 delta。

**实现**: `clock_state.set_adjtime_delta(ticks)` (syscall_clock.rs:387)。

### D5: ClockState 访问 — 全局变量 vs 参数传递（核心 anti-translate）

| 选项 | 机制 | 优点 | 缺点 |
|------|------|------|------|
| A. 全局变量 (C 方式) | `get_monotonic()` 全局 / `reset_kernel_timer(tp)` 隐式全局 | C 原生 | **不可测试**：无法注入 mock ClockState |
| B. `thread_local!` | 线程局部 | 隔离 | no_std 无线程；SMP 无 thread-local |
| C. trait + 全局单例 | 全局 trait object | 抽象 | 仍全局；并发不安全 |
| **D. 参数传递** | `&mut ClockState` 显式传入 | **可测试**；无全局 | 签名变长 |

**选定 D**（核心 anti-translate）。理由：如果用全局变量访问 ClockState，测试时无法注入 mock——`get_monotonic()` 读全局 atomic，`reset_kernel_timer` 操作全局 timer 队列。单元测试无法隔离时间状态，只能测权限路径（EPERM），无法测 set/reset 行为。参数传递后，测试可构造 `ClockState::new()` + `PrivTable::new()` 注入，验证完整行为。

**实现**: 所有 dispatch 函数接受 `&mut ClockState` / `&mut PrivTable` / `&ProcessTable` 参数 (syscall_clock.rs:180-185, 330-334, 361-365, 432-437)。`caller_has_sys_proc_with_table(caller, priv_table)` 同样参数传入 (syscall_clock.rs:293)。

**与 redox 对比**: redox 用 `time::monotonic` 全局接口；minix-rs 显式传入 ClockState 更可测。

### D6: virt_left/prof_left 存储 — Cell/u64 vs AtomicU64

| 选项 | 机制 | 优点 | 缺点 |
|------|------|------|------|
| A. `Cell<u64>` | interior mutability | 简单 | **非 `Sync`**；无法跨 CPU 共享 |
| B. `u64` + 锁 | mutex 保护 | 强一致 | 高频读写下锁开销大；C 注释明示无需锁 |
| C. `RefCell<u64>` | 动态借用 | 运行时检查 | **非 `Sync`** |
| **D. `AtomicU64`** | 原子操作 | `Sync` + 无锁 + SMP 安全 | 需显式内存序 |

**选定 D**。理由：vtimer 字段高频读写（每个 tick 递减），AtomicU64 无锁且 `Sync`。C 靠注释约定"clock handler 只递减不设标志"的并发安全 (do_vtimer.c:83-88)，Rust 用 `compare_exchange_weak` 递减 (proc.rs:617,633) 编译期保证。

**实现**: `TimeStats.virt_left: AtomicU64` / `prof_left: AtomicU64` (proc.rs:588-589)。dispatch_vtimer 用 `load(Relaxed)` 读 / `store(Release)` 写 (syscall_clock.rs:480,489,506,509,517,520)。

### D7: vtimer_check — standalone 函数 vs tick-internal

| 选项 | 机制 | 优点 | 缺点 |
|------|------|------|------|
| A. standalone `vtimer_check(rp)` (C 方式) | 独立函数 do_vtimer.c:81-103 | C 原生 | 21 重复实现；跨文档职责混乱 |
| B. 21 实现 vtimer_check | 21 含到期逻辑 | 自包含 | 21 依赖 15 tick 调度；职责不清 |
| **C. tick-internal**（与 15 D11 一致） | `tick_virt_timer`/`tick_prof_timer` 内联到 tick handler | 职责分离 | 跨文档引用 |

**选定 C**（与 15-clock-timer D11 一致）。理由：vtimer_check 由时钟中断调用，属于 15-clock-timer 的 tick handler 职责。21 是用户态接口（set/query），15 是内核 tick 机制（递减/到期），职责分离。

**实现**: 21 文档标注 vtimer_check 在 15 实现；`tick_virt_timer()`/`tick_prof_timer()` (proc.rs:610,626) CAS 递减，clock.rs:901,907 调用并处理 SIGVTALRM/SIGPROF。

---

## §4. 限制与约束

### 4.1 权限模型约束

| 系统调用 | 权限要求 | C 行号 | 检查方式 |
|---------|---------|--------|---------|
| SYS_SETALARM | SYS_PROC | do_setalarm.c:33 | `caller_has_sys_proc_with_table` |
| SYS_VTIMER | SYS_PROC | do_vtimer.c:31 | `caller_has_sys_proc_with_table` |
| SYS_TIMES | 无 | do_times.c:22-44 | 任意进程可查询 |
| SYS_STIME | 无（由 VM 调用） | do_stime.c:15-18 | 信任 VM |
| SYS_SETTIME | 无（由 PM/VM 调用） | do_settime.c:18-57 | 信任调用者 |

**fail-closed**: `priv_id=None → false → EPERM` (syscall_clock.rs:294-296)。

### 4.2 跨文档依赖

| 依赖项 | 提供方 | 21 使用方式 |
|--------|--------|------------|
| `ClockState` | 15-clock-timer §2.1 | dispatch 函数参数 |
| `TimerAction::NotifyAlarm` | 15-clock-timer §2.4 D6 | SETALARM 构造 |
| `TimerEntry` / `TimerId` | 15-clock-timer §2.2-2.4 | s_alarm_timer 存储 |
| `set_timer`/`reset_timer` | 15-clock-timer ClockState 方法 | SETALARM 调用 |
| `set_boottime`/`set_realtime`/`set_adjtime_delta` | 15-clock-timer ClockState 方法 | STIME/SETTIME 调用 |
| `get_monotonic`/`get_realtime`/`get_boottime` | 15-clock-timer 全局函数 | TIMES 调用 |
| `tick_virt_timer`/`tick_prof_timer` | 15-clock-timer D11 | vtimer_check 实现 |
| `TimeStats.virt_left/prof_left` | proc.rs:588-589 | VTIMER 读写 |
| `MiscFlagsBits::VIRT_TIMER/PROF_TIMER` | proc.rs:151-152 | VTIMER 标志 |

### 4.3 时钟源语义约束

| 时钟源 | 可设置? | 设置方式 | C 函数 |
|--------|---------|---------|--------|
| monotonic | 否 | — | `get_monotonic()` (do_times.c:40) |
| realtime | 是 | SETTIME now=1 | `set_realtime(newclock)` (do_settime.c:55) |
| boottime | 是 | STIME / SETTIME 修正 | `set_boottime(sec)` (do_stime.c:17, do_settime.c:46) |
| adjtime | 是（渐变） | SETTIME now=0 | `set_adjtime_delta(ticks)` (do_settime.c:33) |

### 4.4 测试覆盖现状

| 系统调用 | 权限测试 | 行为测试 | 状态 |
|---------|---------|---------|------|
| TIMES | — | test_dispatch_times_self_replacement | ⚠️ 仅 SELF 替换 |
| SETALARM | test_ksc_setalarm_non_sys_proc_returns_eperm | test_dispatch_setalarm_reset_timer（仅 EPERM 路径） | ❌ 缺 set/time_left 行为 |
| STIME | — | — | ❌ 无测试 |
| SETTIME | — | — | ❌ 无测试 |
| VTIMER | test_ksc_vtimer_non_sys_proc_returns_eperm | — | ❌ 缺 set/query 行为 |
| VtimerType | test_vtimer_type_values_match_c / test_vtimer_type_try_from | — | ✅ 类型测试 |

---

## §5. 接入点（dispatch 函数注册）

| 系统调用 | dispatch 函数 | 接入位置 | 状态 |
|---------|--------------|---------|------|
| SYS_TIMES | `dispatch_times` | 13-syscall-dispatch 分发表 | ✅ |
| SYS_SETALARM | `dispatch_setalarm` | 13-syscall-dispatch 分发表 | ✅ |
| SYS_STIME | `dispatch_stime` | 13-syscall-dispatch 分发表 | ✅ |
| SYS_SETTIME | `dispatch_settime` | 13-syscall-dispatch 分发表 | ✅ |
| SYS_VTIMER | `dispatch_vtimer` | 13-syscall-dispatch 分发表 | ✅ |

dispatch 函数由 `kernel_call_dispatch` 调用，传入 `&mut ClockState` / `&mut PrivTable` / `&ProcessTable`（D5 参数传递）。

---

## 附录 A: C↔Rust 差异矩阵

| C 符号 | C 位置 | Rust 表达 | 差异类型 | 理由 |
|--------|--------|----------|---------|------|
| `do_times` | do_times.c:22-44 | `dispatch_times` | 语义对齐 | 参数传入 ProcessTable |
| `endpt == SELF` | do_times.c:33-34 | `if endpt == SELF { caller.p_endpoint }` | 语义对齐 | — |
| `rp->p_user_time` | do_times.c:37 | `rp.p_time.user_time.load(Relaxed)` | 类型增强 | AtomicU64 替代 clock_t |
| `get_monotonic/get_realtime/get_boottime` | do_times.c:40-42 | `clock::get_monotonic/get_realtime/get_boottime` | 语义对齐 | 三时钟源 |
| `do_setalarm` | do_setalarm.c:22-64 | `dispatch_setalarm` | 语义对齐 | 参数传入 ClockState+PrivTable |
| `priv(caller)` | do_setalarm.c:33 | `caller_has_sys_proc_with_table(caller, priv_table)` | anti-translate | 参数传入，非全局 priv() |
| `priv(caller)->s_alarm_timer` | do_setalarm.c:36 | `kpriv.runtime.s_alarm_timer: Option<(TimerEntry, TimerId)>` | anti-translate | Option 替代 tmr_is_set；TimerId 替代指针 |
| `tmr_is_set(tp)` | do_setalarm.c:40 | `Option::None`/`Some` match | anti-translate | Option 替代标志检查 |
| `reset_kernel_timer(tp)` | do_setalarm.c:57 | `clock_state.reset_timer(old_id)` | anti-translate | TimerId 替代指针 |
| `set_kernel_timer(tp, exp_time, cause_alarm, endpoint)` | do_setalarm.c:61 | `clock_state.set_timer(TimerEntry{action: NotifyAlarm{endpoint}})` | anti-translate | TimerAction enum 替代函数指针 |
| `cause_alarm(proc_nr_e)` | do_setalarm.c:69-76 | `TimerAction::NotifyAlarm { endpoint }` | anti-translate | enum 变体替代函数指针 |
| `mini_notify(proc_addr(CLOCK), proc_nr_e)` | do_setalarm.c:75 | ClockState 到期 dispatch | 语义对齐 | 由 15 实现 |
| `do_stime` | do_stime.c:15-18 | `dispatch_stime` | 语义对齐 | 参数传入 ClockState |
| `set_boottime(boot_time)` | do_stime.c:17 | `clock_state.set_boottime(boot_time)` | 语义对齐 | — |
| `do_settime` | do_settime.c:18-57 | `dispatch_settime` | 语义对齐 | 参数传入 ClockState |
| `clock_id != CLOCK_REALTIME` | do_settime.c:25-26 | `if clock_id != CLOCK_REALTIME { return Ok(EINVAL) }` | 语义对齐 | — |
| `set_adjtime_delta(ticks)` | do_settime.c:33 | `clock_state.set_adjtime_delta(ticks)` | 语义对齐 | D4 保留 POSIX |
| `set_realtime(newclock)` | do_settime.c:55 | `clock_state.set_realtime(newclock)` | 语义对齐 | — |
| `do_vtimer` | do_vtimer.c:21-74 | `dispatch_vtimer` | 语义对齐 | 参数传入 PrivTable+ProcessTable |
| `VT_WHICH != VT_VIRTUAL && != VT_PROF` | do_vtimer.c:33-34 | `VtimerType::try_from(which)` | 类型增强 | enum + TryFrom 替代整数比较 |
| `VT_VIRTUAL=1`/`VT_PROF=2` | com.h:420-421 | `VtimerType::Virtual=1`/`Prof=2` | 语义对齐 | 值对齐 com.h |
| `&rp->p_virt_left` | do_vtimer.c:47 | `target.p_time.virt_left.load(Relaxed)` | anti-translate | AtomicU64 替代 clock_t* |
| `&rp->p_prof_left` | do_vtimer.c:50 | `target.p_time.prof_left.load(Relaxed)` | anti-translate | AtomicU64 替代 clock_t* |
| `rp->p_misc_flags & pt_flag` | do_vtimer.c:54 | `target.p_misc_flags.is_set(flag)` | 类型增强 | bitflags 替代裸位 |
| `rp->p_misc_flags &= ~pt_flag` | do_vtimer.c:61 | `target.p_misc_flags.clear(flag)` | 类型增强 | bitflags |
| `rp->p_misc_flags |= pt_flag` | do_vtimer.c:65 | `target.p_misc_flags.set(flag)` | 类型增强 | bitflags |
| `vtimer_check(rp)` | do_vtimer.c:81-103 | 在 15-clock-timer 实现（tick-internal） | 跨文档 | D7 职责分离 |
| `cause_sig(rp->p_nr, SIGVTALRM)` | do_vtimer.c:94 | clock.rs:901 处理 | 跨文档 | 15 实现 |
| `MF_VIRT_TIMER`/`MF_PROF_TIMER` | do_vtimer.c:46,49 | `MiscFlagsBits::VIRT_TIMER`/`PROF_TIMER` | 类型增强 | bitflags |

---

## 附录 B: redox 对比

| 维度 | redox | minix-rs | 选择理由 |
|------|-------|---------|---------|
| 定时器到期通知 | `context::signal::requeue` 重新排队信号 | `TimerAction::NotifyAlarm` + `mini_notify(CLOCK, endpoint)` | minix-rs 对齐 C 通知模型；redox 用信号重排队 |
| 单调时钟 | `time::monotonic` 全局接口 | `clock::get_monotonic()` 全局 + `ClockState::uptime()` 实例 | minix-rs 双轨：全局读 + 实例可测 |
| 时钟状态访问 | 全局 `time::` 接口 | 参数传入 `&mut ClockState` (D5) | minix-rs 参数传入更可测；redox 全局更简洁 |
| 用户态定时器 | `scheme::time` 用户态驱动 | 内核 vtimer + SIGVTALRM/SIGPROF | 不同设计哲学；minix-rs 对齐 C |
| adjtime | 无渐变调整 | `set_adjtime_delta(ticks)` 保留 POSIX 语义 | minix-rs 保留 POSIX 完整性 |
| vtimer 递减 | 用户态驱动计数 | `AtomicU64::compare_exchange_weak` 内核 tick 递减 | minix-rs 内核 tick，SMP 安全 |

---

## 附录 C: 测试策略

### C.1 现有测试（10 个，已实现）

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_vtimer_type_values_match_c` | VtimerType 值对齐 C (Virtual=1, Prof=2) | com.h:420-421 |
| `test_vtimer_type_try_from` | TryFrom<i32> 合法/非法值 | do_vtimer.c:33-34 |
| `test_clock_realtime` | CLOCK_REALTIME=0 | do_settime.c:25 |
| `test_ksc_caller_has_sys_proc_no_priv_id_rejected` | priv_id=None 拒绝 | do_setalarm.c:33 |
| `test_ksc_caller_has_sys_proc_fresh_table_rejects_all` | 空 PrivTable 拒绝 | do_setalarm.c:33 |
| `test_ksc_setalarm_non_sys_proc_returns_eperm` | SETALARM 非 SYS_PROC 返回 EPERM | do_setalarm.c:33 |
| `test_ksc_vtimer_non_sys_proc_returns_eperm` | VTIMER 非 SYS_PROC 返回 EPERM | do_vtimer.c:31 |
| `test_ksc_priv_flags_sys_proc_bit_definition` | IDL_F/SRV_F 含 SYS_PROC, USR_F 不含 | SYS_PROC 位 |
| `test_dispatch_times_self_replacement` | TIMES SELF 替换 + reply 填充 | do_times.c:33-34 |
| `test_dispatch_setalarm_reset_timer` | SETALARM 非 SYS_PROC 返回 EPERM | do_setalarm.c:33 |

### C.2 待补充测试（8 个，行为覆盖）

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

### C.3 测试注入策略（D5 参数传递的收益）

```rust
// D5 参数传递使行为测试可注入 mock 状态
#[test]
fn test_dispatch_setalarm_set_timer() {
    let mut p = proc_with_priv_id(Some(static_priv_id(0)));
    // 注入含 SYS_PROC 的 PrivTable
    let mut priv_table = PrivTable::with_sys_proc(static_priv_id(0));
    let mut clock_state = ClockState::new();
    let mut msg = Message::default();
    // 设置 exp_time=100, abs_time=0 (相对时间)
    unsafe { msg.m_u.m_lsys_krn_sys_setalarm.exp_time = 100; }
    unsafe { msg.m_u.m_lsys_krn_sys_setalarm.abs_time = 0; }

    let result = dispatch_setalarm(&mut p, &mut msg, &mut priv_table, &mut clock_state);
    assert_eq!(result, KcallResult::Ok(OK));
    // 验证 timer 已设置
    let kpriv = priv_table.get(static_priv_id(0)).unwrap();
    assert!(kpriv.runtime.s_alarm_timer.is_some());
}
```

D5 参数传递的核心收益：测试可构造 `PrivTable::with_sys_proc()` + `ClockState::new()` 注入，验证 set/reset 行为，而非仅 EPERM 路径。

---

## 自检

- [x] §1 目标约束完整（对齐 C + 修复 P0/P1 + anti-translate + 与 15 衔接）
- [x] §1 Ground Truth 验证完整（7 个 C 函数 + Rust 归属）
- [x] §2 数据结构与函数设计完整（VtimerType + 5 个 dispatch + 权限检查）
- [x] §2 anti-translate 体现（VtimerType enum/TimerAction/参数传递/AtomicU64）
- [x] §2 virt_left/prof_left 真实 AtomicU64 代码（非 `/* */` 占位）
- [x] §2 VtimerType 值 Virtual=1/Prof=2（对齐 com.h:420-421）
- [x] §2 vtimer_check 跨文档衔接（15-clock-timer 实现）
- [x] §2 删除"D5 实现说明 (2026-06-15 更新)"迭代叙事
- [x] §3 设计决策详表完整（D1-D7 hypothesis-driven）
- [x] §3 D5 核心推理："如果用全局变量访问 ClockState 会有什么问题？→ 不可测试 → 参数传递"
- [x] §4 限制约束完整（权限模型 + 跨文档依赖 + 时钟源语义 + 测试覆盖现状）
- [x] §5 接入点清单完整（5 个 dispatch 函数）
- [x] 附录 A C↔Rust 差异矩阵完整（28 项）
- [x] 附录 B redox 对比完整（6 维度，含 context::signal::requeue + time::monotonic）
- [x] 附录 C 测试策略完整（10 现有 + 8 待补充 + 注入策略）
- [x] 无迭代叙事（"旧版/最初/后来/我们改成/D5 实现说明 (2026-XX-XX 更新)"）
- [x] 无 tmp 文件引用
- [x] 无内部 review ID（P0-XX/P1-XX）
- [x] 无迭代叙事日期（"2026-XX-XX 更新/Fixed"）
- [x] 与 15-clock-timer 衔接（ClockState/TimerAction 在 15 定义，21 是用户态接口）
- [x] anti-translate 体现（VtimerType enum/TimerAction/ClockState 参数传递/AtomicU64）
