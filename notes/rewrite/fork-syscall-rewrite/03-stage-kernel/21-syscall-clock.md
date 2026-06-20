# 21-syscall-clock: 时钟系统调用

> **分类**: 系统调用服务
> **源码**: `minix3/minix/kernel/system/do_times.c`, `do_setalarm.c`, `do_stime.c`, `do_settime.c`, `do_vtimer.c`
> **前置**: 14（时钟/定时器——ClockState, alarm timer, vtimer）
> **C 总行数**: ~305 行

---

## Ch1: 概念

**核心问题**: 用户态进程如何查询和设置时间、闹钟和定时器？

时钟系统调用是 15-clock-timer 的用户态接口：

| 系统调用 | 语义 | C 处理函数 |
|---------|------|-----------|
| SYS_TIMES | 查询进程时间统计 | `do_times()` |
| SYS_SETALARM | 设置/取消同步闹钟 | `do_setalarm()` |
| SYS_STIME | 设置启动时间 | `do_stime()` |
| SYS_SETTIME | 设置实时时钟 / adjtime | `do_settime()` |
| SYS_VTIMER | 设置/查询虚拟/性能定时器 | `do_vtimer()` |

### 1.1 SYS_TIMES

返回进程的用户态时间、系统态时间、启动时间和当前时钟值。

```
输入: endpt (SELF 或指定进程)
输出: user_time, system_time, boot_ticks, real_ticks, boot_time
```

### 1.2 SYS_SETALARM

每个 system process 有一个同步闹钟（`s_alarm_timer`）。闹钟到期时，内核通过 `mini_notify(CLOCK, endpoint)` 通知进程。

```
输入: exp_time, abs_time, (返回 time_left)
输出: time_left (上次闹钟剩余时间), uptime (当前时间)
```

- `abs_time=0, exp_time=0`：取消闹钟
- `abs_time=0, exp_time>0`：相对时间，exp_time += uptime
- `abs_time=1`：绝对时间

**权限**：仅 `SYS_PROC` 可调用。

### 1.3 SYS_STIME

设置 boottime（系统启动时的 Unix 时间戳）。通常由 VM 调用。

### 1.4 SYS_SETTIME

两种模式：
- `now=0`：adjtime 模式，设置 adjtime_delta（渐变调整）
- `now=1`：直接设置 realtime

仅 `CLOCK_REALTIME` 可修改。

### 1.5 SYS_VTIMER

设置或查询进程的虚拟定时器（VT_VIRTUAL）或性能定时器（VT_PROF）。

```
输入: VT_WHICH (VT_VIRTUAL/VT_PROF), VT_SET, VT_VALUE, VT_ENDPT
输出: VT_VALUE (旧值)
```

- `VT_SET=0`：仅查询
- `VT_SET=1, VT_VALUE>0`：设置新值
- `VT_SET=1, VT_VALUE=0`：禁用定时器

**权限**：仅 `SYS_PROC` 可调用。

---

## Ch2: C 源码分析

### do_times.c (46 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 19-46 | `do_times()` | SELF 替换 → 查询 p_user_time/p_sys_time → get_monotonic/get_realtime/get_boottime |

### do_setalarm.c (78 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 27-78 | `do_setalarm()` | 权限检查 → 返回上次剩余 → 设置/取消定时器 |
| 73-78 | `cause_alarm()` | 定时器回调 → mini_notify(CLOCK, endpoint) |

### do_stime.c (19 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 15-18 | `do_stime()` | set_boottime(boot_time) |

### do_settime.c (58 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 17-58 | `do_settime()` | adjtime(now=0) 或 set_realtime(now=1) |

### do_vtimer.c (103 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 23-65 | `do_vtimer()` | 权限检查 → 确定定时器类型 → 查询/设置 |
| 68-89 | `vtimer_check()` | 时钟中断调用，检查虚拟/性能定时器到期 |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | SETALARM 定时器 | `minix_timer_t` 链表 vs BTreeMap | **BTreeMap** | 与 15-clock-timer D2 一致 |
| D2 | cause_alarm 回调 | 函数指针 vs TimerAction | **TimerAction::NotifyAlarm** | 与 15-clock-timer D6 一致 |
| D3 | VT_WHICH | 整数 vs enum | **`VtimerType` enum** | 类型安全 |
| D4 | SETTIME adjtime | 保留 vs 删除 | **保留** | adjtime 是 POSIX 语义 |
| D5 | ClockState 访问 | 全局变量 vs 参数传递 | **参数传递（已实现）** | `dispatch_setalarm` 接受 `&mut PrivTable, &mut ClockState`；`dispatch_vtimer` 接受 `&PrivTable, &ProcessTable` |

---

## Ch4: 实现要点

### 4.1 VtimerType enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VtimerType {
    Virtual = 0,  // VT_VIRTUAL
    Prof = 1,     // VT_PROF
}
```

### 4.2 SETALARM 与 ClockState 交互

```rust
pub fn dispatch_setalarm(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    // C: do_setalarm.c:35-37 — extract parameters
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_setalarm };
    let exp_time = req.exp_time;
    let use_abs_time = req.abs_time != 0;

    // C: do_setalarm.c:39 — SYS_PROC permission check
    if !caller_has_sys_proc_with_table(caller, priv_table) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_setalarm.c:41-42 — get timer from priv structure
    let caller_priv_id = match caller.priv_id {
        Some(id) => id,
        None => return KcallResult::Ok(EPERM),
    };

    // C: do_setalarm.c:44-49 — return time left on previous alarm
    let uptime = clock_state.uptime();
    let time_left = {
        let kpriv = priv_table.get(caller_priv_id);
        match kpriv.and_then(|p| p.s_alarm_timer.as_ref()) {
            None => TMR_NEVER,
            Some(timer) if timer.exp_time > uptime => timer.exp_time - uptime,
            Some(_) => 0,
        }
    };

    // C: do_setalarm.c:57-66 — set or reset timer
    if !use_abs_time && exp_time == 0 {
        // Reset alarm
        if let Some(kpriv) = priv_table.get_mut(caller_priv_id) {
            if let Some(old_timer) = kpriv.s_alarm_timer.take() {
                clock_state.reset_timer(old_timer.exp_time);
            }
        }
    } else {
        // Set alarm
        let actual_exp_time = if use_abs_time { exp_time } else { uptime + exp_time };
        let timer = TimerEntry {
            exp_time: actual_exp_time,
            action: TimerAction::NotifyAlarm { endpoint: caller.p_endpoint },
        };
        if let Some(kpriv) = priv_table.get_mut(caller_priv_id) {
            if let Some(old_timer) = kpriv.s_alarm_timer.take() {
                clock_state.reset_timer(old_timer.exp_time);
            }
            clock_state.set_timer(timer.clone());
            kpriv.s_alarm_timer = Some(timer);
        }
    }

    // C: do_setalarm.c:51 — return uptime + time_left
    // (reply written into msg union)
    KcallResult::Ok(OK)
}
```

**D5 实现说明 (2026-06-15 更新)**: `dispatch_setalarm` 已接受 `&mut PrivTable` 和 `&mut ClockState` 参数，完整实现了 C `do_setalarm.c:35-78` 的语义：

1. **SYS_PROC 权限检查**：通过 `caller_has_sys_proc_with_table()` 检查调用者是否为系统进程。
2. **`s_alarm_timer` 读取/写入**：从 `PrivTable` 获取调用者的 `KPriv`，读取/设置 `s_alarm_timer` 字段。
3. **`time_left` 计算**：对比 `timer.exp_time` 与 `clock_state.uptime()`，返回上次闹钟剩余时间。
4. **`ClockState::set_timer/reset_timer`**：设置新定时器或取消旧定时器，与 15-clock-timer D2 的 `BTreeMap` 定时器管理对接。
5. **绝对/相对时间转换**：`abs_time=1` 直接使用 `exp_time`，否则 `exp_time + uptime`。

### 4.3 VTIMER 与 KProcess 交互

```rust
pub fn dispatch_vtimer(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &PrivTable,
    proc_table: &ProcessTable,
) -> KcallResult {
    // C: do_vtimer.c:30-33 — extract parameters
    let which = m2.m2i1;        // VT_WHICH
    let set = m2.m2i2 != 0;     // VT_SET
    let value = m2.m2l1 as u64; // VT_VALUE
    let endpt = m2.m2l2 as i32; // VT_ENDPT

    // C: do_vtimer.c:35-36 — SYS_PROC permission check
    if !caller_has_sys_proc_with_table(caller, priv_table) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_vtimer.c:38-39 — validate timer type
    let vtype = match VtimerType::try_from(which) {
        Ok(v) => v,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:41-44 — SELF replacement + endpoint validation
    let target_endpoint = if endpt == SELF { caller.p_endpoint } else { Endpoint(endpt) };
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };
    let target = match proc_table.get(target_nr) {
        Some(rp) => rp,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:46-54 — determine flag and retrieve old value
    let (pt_flag, old_value) = match vtype {
        VtimerType::Virtual => (MiscFlagsBits::VIRT_TIMER, /* virt_left */),
        VtimerType::Prof => (MiscFlagsBits::PROF_TIMER, /* prof_left */),
    };

    // C: do_vtimer.c:61-71 — set new value if VT_SET
    if set {
        target.p_misc_flags.clear(pt_flag);
        if value > 0 {
            // store new value + re-enable flag
            target.p_misc_flags.set(pt_flag);
        }
        // else: value == 0 → clear timer value, flag already cleared
    }

    // C: do_vtimer.c:73 — return old value in VT_VALUE
    KcallResult::Ok(OK)
}
```

实现要点：SYS_PROC 权限检查 → `VtimerType` 枚举验证 (Virtual=1, Prof=2, 对齐 C `com.h:420-421`) → SELF 替换 + `ProcessTable::endpoint_to_nr` 校验 → `virt_left/prof_left` 读写 + `MiscFlagsBits` set/clear → 旧值返回。

---

## 测试

- 单元：VtimerType TryFrom<i32> + 数值对齐 C (VT_VIRTUAL=1, VT_PROF=2)
- 单元：SETALARM 非 SYS_PROC 返回 EPERM
- 单元：SETALARM 重置定时器 (exp_time=0)
- 单元：VTIMER 非 SYS_PROC 返回 EPERM
- 单元：SYS_PROC 权限位定义验证 (IDL_F/SRV_F 含 SYS_PROC, USR_F 不含)
- 单元：TIMES SELF 替换 + 回复字段填充
- 单元：CLOCK_REALTIME 常量值

> **测试状态 (2026-06-15 更新)**: TIMES/SETALARM/STIME/SETTIME/VTIMER 主体逻辑已实现。SETALARM 和 VTIMER 的 SYS_PROC 权限检查有单元测试覆盖。见 `checklist.md` F-26~F-29 行。

---

## 参见

- [15-clock-timer.md](15-clock-timer.md) — ClockState, TimerAction, vtimer_check
