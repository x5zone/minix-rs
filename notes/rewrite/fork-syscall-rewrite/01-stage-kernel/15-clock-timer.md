# 15-clock-timer: 时钟中断与定时器

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/clock.c`, `minix3/minix/kernel/system/do_setalarm.c`, `minix3/minix/kernel/system/do_vtimer.c`, `minix3/minix/kernel/arch/i386/arch_clock.c`
> **Rust 实现**: `os/kernel/src/clock.rs`, `os/kernel/src/syscall_clock.rs`, `os/arch/src/arch/clock.rs`
> **说明**: 100Hz 时钟中断处理——时间记账、虚拟/性能定时器、同步闹钟、负载平均；quantum 递减归架构层
> **前置**: 14-exception-interrupt.md, 05-clock-interrupt-init.md, 10-switch-to-user.md, 11-scheduling-primitives.md
> **创建**: 2026-06-13 · **重写**: 2026-07-31

---

## Ch1: 概念

### 1.0 目标读者与本章定位

**目标读者**：已阅读 14-exception-interrupt（理解"三条激活路径 + 统一出口 switch_to_user"）与 05-clock-interrupt-init（理解 `init_clock()` 的启动阶段定位）的开发者。

**本章回答**：当时钟中断从硬件进入 `timer_int_handler()` 后，内核在"tick 粒度"上做了哪些事？这些事如何在 Rust 中以类型安全的方式重新表达？

**本章不讲**（避免与上下游文档重复）：
- 硬件定时器配置（8254 PIT / LAPIC Timer / ARM Generic Timer 初始化）→ 05-clock-interrupt-init + 14-exception-interrupt
- 时钟中断的入口汇编与 IDT 注册 → 14-exception-interrupt
- quantum 耗尽后的调度决策（`RTS_NO_QUANTUM` 设置、`proc_no_time` 调度）→ 10-switch-to-user + 11-scheduling-primitives
- 时钟系统调用的消息解析（SYS_SETALARM/SYS_VTIMER/SYS_STIME/SYS_SETTIME/SYS_TIMES）→ 21-syscall-clock
- AP 启动时的定时器注册 → 16-smp
- TSC 校准细节 → 04-platform-discovery

### 1.1 时钟中断在内核生命周期中的角色

14-exception-interrupt §1.2 把时钟中断定位为"三条激活路径之一"的硬件中断特例。本章承接该定位，回答"进入 `timer_int_handler()` 之后发生什么"。

CPU 在每个 tick 被定时器硬件中断一次，内核借这一时机完成**四类记账与触发**工作：

| 工作 | 数据载体 | 触发对象 | 频率 |
|------|---------|---------|------|
| 时间记账 | `kclockinfo.uptime`/`realtime`、`p_user_time`/`p_sys_time` | BSP 维护全局时钟；每个进程维护自己的时间累计 | 每 tick |
| 进程虚拟/性能定时器递减 | `p_virt_left`/`p_prof_left` | 当前进程 + billp（计费进程） | 每 tick（仅对应标志置位时） |
| 同步闹钟扫描 | `clock_timers` 链表 | 任意系统进程（通过 SYS_SETALARM 注册） | BSP 每 tick 扫描 |
| 负载平均采样 | `kloadinfo.proc_load_history[]` | 每 `_LOAD_UNIT_SECS` 秒切换采样槽 | 每 tick 累计 |

**量子（quantum）递减不在上表**——这是本章与旧文档的关键差异。C ground truth（clock.c:70-173）证明 `timer_int_handler()` 不递减 `p_cpu_time_left`；quantum 递减归 `context_stop()`（上下文切换路径，arch_clock.c:208-349，具体 line 326-330，基于 TSC delta；`arch_timer_int_handler()` 在 i386 为空函数）。详见 §1.4 行为规则 3 与 §3 D9。

### 1.2 与 Minix3 的对应关系

| Minix3 概念 | C 源码位置 | 说明 |
|-------------|-----------|------|
| `timer_int_handler()` | clock.c:70-173 | 时钟中断主处理函数（软件部分） |
| `arch_timer_int_handler()` | arch_clock.c:72-74 (i386) | i386 上为**空函数**；quantum 递减不在此 |
| `context_stop()` | arch_clock.c:208-349 (i386) | 上下文切换路径：**quantum 递减（line 326-330，基于 TSC delta）** + cpuavg + p_cycles 记账 |
| `kclockinfo` | clock.h | 全局时钟状态（hz, uptime, realtime, boottime） |
| `kloadinfo` | clock.h | 负载平均历史（circular buffer） |
| `clock_timers` | clock.c:37 | 同步闹钟定时器链表（BSP only） |
| `adjtime_delta` | clock.c:42 | adjtime 调整量（BSP only） |
| `init_clock()` | clock.c:47-64 | 初始化时钟变量 |
| `boot_cpu_init_timer()` | clock.c:294-304 | BSP 定时器初始化 + 注册 handler |
| `app_cpu_init_timer()` | clock.c:306-312 | AP 定时器初始化（不注册全局 handler） |
| `set_kernel_timer()` | clock.c:229-240 | 设置内核定时器（链表插入） |
| `reset_kernel_timer()` | clock.c:245-255 | 重置内核定时器（链表删除） |
| `load_update()` | clock.c:260-292 | 负载平均采样 |
| `cause_alarm()` | do_setalarm.c:69-76 | 闹钟到期通知（`mini_notify(CLOCK, ep)`） |
| `vtimer_check()` | do_vtimer.c:81-103 | 虚拟/性能定时器到期检查 + 发信号 |

### 1.3 关键状态/机制说明

**全局时钟状态** (`kclockinfo`)：
- `hz`：时钟频率（默认 100Hz，可配置 2..=50000）
- `uptime`：单调递增的启动后滴答数（每 tick +1，**不受 adjtime 影响**）
- `realtime`：墙上时钟滴答数（受 adjtime 影响，可能加速/减速）
- `boottime`：UNIX epoch 到启动的秒数

**同步闹钟** (`s_alarm_timer`)：
- 每个 `struct priv` 有一个 `s_alarm_timer`（`priv.h`）
- 到期时通过 `cause_alarm()` → `mini_notify(CLOCK, target)` 通知目标进程
- 仅系统进程（`SYS_PROC`）可使用（`do_setalarm.c:33`）
- 用 `tmr_func_t` 函数指针 + `tmr_arg` 整数作为到期回调

**虚拟/性能定时器**：
- `p_virt_left`：用户态虚拟定时器（仅用户态时间递减，到期发 `SIGVTALRM`）
- `p_prof_left`：性能分析定时器（用户态+系统态都递减，到期发 `SIGPROF`）
- 通过 `MF_VIRT_TIMER`/`MF_PROF_TIMER` 标志启用/禁用（`proc.h`）
- 检查函数 `vtimer_check()` 在 `do_vtimer.c:81-103`（注意：旧文档误标为 68-89）

**adjtime 机制**：
- `adjtime_delta`：时间调整增量（正=加速，负=减速）
- 每隔一个 tick 调整：奇数 tick 时 `realtime += (delta > 0) ? 2 : 0`，并使 delta 朝 0 收敛
- 用于 NTP 等时间同步场景，避免时间突变
- `uptime` 不受影响，保持严格单调

**负载平均**：
- `proc_load_history[_LOAD_HISTORY]`：circular buffer（`_LOAD_HISTORY=12`）
- 采样槽位按 `uptime / hz / _LOAD_UNIT_SECS` 循环（`_LOAD_UNIT_SECS=5`）
- 每个 tick 把"就绪队列进程总数"累加到当前槽位

### 1.4 行为规则

1. **BSP 独占全局时间**：`uptime`/`realtime`/`clock_timers`/`boottime`/`adjtime_delta` 仅 BSP 维护；AP 的 `timer_int_handler()` 只做进程记账与负载平均
2. **双重记账规则**：当前进程计用户时间（`p_user_time++`）；若当前进程非 BILLABLE，计费进程（billp）的 `p_sys_time++`——"一个进程的用户时间是另一个进程的系统时间"
3. **quantum 递减不在 `timer_int_handler()`**：C ground truth（clock.c:70-173）证明；quantum 递减归 `context_stop()`（上下文切换路径，基于 TSC delta；`arch_timer_int_handler()` 在 i386 为空函数），见 §3 D9
4. **profile timer 双重递减**：当前进程的 `p_prof_left` 与 billp 的 `p_prof_left` 都递减（profile 计 user+sys 时间，而 billp 的 sys 时间就是当前进程的 user 时间）
5. **vtimer 到期清标志**：`vtimer_check()` 在到期时清除 `MF_VIRT_TIMER`/`MF_PROF_TIMER`，避免重复触发
6. **同步闹钟按到期时间排序**：`clock_timers` 链表按 `tmr_exp_time` 升序；`tmrs_exptimers()` 从队首处理所有到期定时器
7. **BKL 保护**：整个 `timer_int_handler()` 在 BKL 下执行；`vtimer_check()` 注释说明只需防时钟 handler 干扰，不需额外锁（do_vtimer.c:83-88）

### 1.5 本章心智模型

读者读完本章应能回答三个问题：
1. **每个 tick 内核做了什么？** → 时间记账 + vtimer 递减 + 闹钟扫描 + 负载采样（§1.1 表格）
2. **quantum 在哪里递减？** → 不在 `timer_int_handler()`，归 `context_stop()`（上下文切换路径，基于 TSC delta；`arch_timer_int_handler()` 在 i386 为空函数）（§1.4 规则 3）
3. **BSP 与 AP 的时钟职责如何分离？** → BSP 独占全局时间 + 闹钟队列；AP 只做本地进程记账 + 负载采样（§1.4 规则 1）

---

## Ch2: C 源码分析

### 2.0 Claims-Evidence

| Claim | Evidence | Status |
|-------|----------|--------|
| `timer_int_handler` 不递减 `p_cpu_time_left` | `clock.c:70-173` 全函数无 `p_cpu_time_left` | ✅ verified（纠正旧文档错误） |
| `context_stop` 基于 TSC delta 递减 quantum（`arch_timer_int_handler` 在 i386 为空函数） | `arch_clock.c:326-330`（在 `context_stop` 内，line 208-349） | ✅ verified |
| BSP 独占 uptime/realtime 维护 | `clock.c:91` `if (cpu_is_bsp(cpuid))` | ✅ verified |
| 非 BILLABLE 进程 → billp->p_sys_time++ | `clock.c:118-120` | ✅ verified |
| billp->p_prof_left 也递减 | `clock.c:134-138` | ✅ verified |
| `vtimer_check` 在 `do_vtimer.c:81-103`（非 68-89） | `do_vtimer.c:81` `void vtimer_check` | ✅ verified（纠正旧文档行号） |
| `cause_alarm` 在 `do_setalarm.c:69-76`（非 73） | `do_setalarm.c:69` `static void cause_alarm` | ✅ verified（纠正旧文档行号） |
| `init_clock` 行号 47-64（非 47-63） | `clock.c:47-64` | ✅ verified |
| adjtime 每 odd tick 调整 realtime | `clock.c:97-100` `uptime & 0x1` | ✅ verified |
| `clock_timers` 链表按 `tmr_exp_time` 排序 | `clock.c:159-161` + timers.h `tmrs_settimer` | ✅ verified |
| 同步闹钟仅 SYS_PROC 可用 | `do_setalarm.c:33` `if (! (priv(caller)->s_flags & SYS_PROC)) return(EPERM);` | ✅ verified |
| `tmr_func_t` 是函数指针 + `tmr_arg` int | timers.h `struct minix_timer` | ✅ verified |

### 2.1 相关定义

| 常量/宏 | 值 | 源码位置 | 说明 |
|---------|-----|---------|------|
| `DEFAULT_HZ` | 100 | clock.h | 默认时钟频率 |
| `TMR_NEVER` | `LONG_MAX` | timers.h | 定时器永不触发 |
| `_LOAD_UNIT_SECS` | 5 | clock.h | 负载采样间隔（秒） |
| `_LOAD_HISTORY` | 12 | clock.h | 负载历史槽数 |
| `VT_VIRTUAL` | 1 | com.h:420 | 虚拟定时器类型（**注意：旧文档误标为 0**） |
| `VT_PROF` | 2 | com.h:421 | 性能分析定时器类型（**注意：旧文档误标为 1**） |
| `MF_VIRT_TIMER` | 0x002 | proc.h | 虚拟定时器活跃标志 |
| `MF_PROF_TIMER` | 0x004 | proc.h | 性能分析定时器活跃标志 |
| `BILLABLE` | 0x004 | const.h:144 | 可计费进程标志 |
| `SYS_PROC` | 0x010 | const.h:147 | 系统进程标志 |

### 2.2 核心数据结构

**`struct clockinfo`**（clock.h）—— 全局时钟状态：

```c
struct clockinfo {
    clock_t hz;           // 时钟频率（ticks/秒）
    clock_t realtime;     // 墙上时钟（滴答数，受 adjtime 影响）
    clock_t uptime;       // 单调时钟（滴答数，每 tick +1）
    time_t boottime;      // 启动时的 UNIX 时间戳（秒）
};
```

**`struct loadinfo`**（clock.h）—— 负载平均历史：

```c
struct loadinfo {
    u16_t proc_last_slot;                  // 当前负载采样槽位
    u32_t proc_load_history[_LOAD_HISTORY]; // 负载历史 circular buffer
    clock_t last_clock;                    // 上次更新时间
};
```

**`minix_timer_t`**（timers.h）—— 内核定时器节点（链表）：

```c
typedef struct minix_timer {
    struct minix_timer *tmr_next;  // 链表下一节点
    clock_t tmr_exp_time;          // 到期时间（单调滴答）
    tmr_func_t tmr_func;           // 到期回调函数（函数指针）
    int tmr_arg;                   // 回调参数（进程端点）
} minix_timer_t;
```

**`priv` 中的 `s_alarm_timer`**（priv.h）—— 每个系统进程的同步闹钟节点：

```c
struct priv {
    /* ... */
    minix_timer_t s_alarm_timer;  // 同步闹钟定时器（嵌入 priv 结构）
    /* ... */
};
```

> **关键观察**：`s_alarm_timer` 是 `priv` 的嵌入字段，不是指针。C 用 `&sp->s_alarm_timer` 作为 `minix_timer_t *tp` 传入 `set_kernel_timer`/`reset_kernel_timer`。这个**结构体地址稳定性**是 C 的 stable identity。Rust 不能用指针（数据结构不是链表节点），需重新设计 identity 机制（见 §3 D3）。

### 2.3 关键函数分析

#### `init_clock()` — clock.c:47-64

初始化时钟软件变量：清零 `kclockinfo`，从环境变量读取 `hz`（范围 2..=50000，默认 100），清零 `kloadinfo`。**不触碰硬件定时器**——硬件使能发生在更晚的 `bsp_finish_booting()`。

#### `timer_int_handler()` — clock.c:70-173

时钟中断主处理函数，每次 tick 调用。**不递减 quantum**（关键纠正）。处理步骤：

1. **watchdog 递增**（条件编译，clock.c:82-89）
2. **BSP 全局时间维护**（clock.c:91-104）：
   - `kclockinfo.uptime++`
   - 若 `adjtime_delta != 0 && uptime & 0x1`：`realtime += (delta > 0) ? 2 : 0`，`delta` 朝 0 收敛
   - 否则：`realtime++`
3. **进程记账**（clock.c:113-120）：
   - `p = proc_ptr`（当前进程），`billp = bill_ptr`（计费进程）
   - `p->p_user_time++`
   - 若 `!(priv(p)->s_flags & BILLABLE)`：`billp->p_sys_time++`
4. **虚拟/性能定时器递减**（clock.c:128-138）：
   - `MF_VIRT_TIMER` 置位且 `p_virt_left > 0`：`p_virt_left--`
   - `MF_PROF_TIMER` 置位且 `p_prof_left > 0`：`p_prof_left--`
   - 非 BILLABLE 且 `billp` 有 `MF_PROF_TIMER` 且 `billp->p_prof_left > 0`：`billp->p_prof_left--`
5. **vtimer 到期检查**（clock.c:145-148）：`vtimer_check(p)`；若 `p != billp`，`vtimer_check(billp)`
6. **负载平均**（clock.c:151）：`load_update()`
7. **BSP 闹钟扫描**（clock.c:153-161）：`tmr_has_expired(clock_timers, uptime)` → `tmrs_exptimers()`
8. **架构相关 tick**（clock.c:170）：`arch_timer_int_handler()` ← **i386 上为空函数；quantum 递减不在此，归 `context_stop()`**

#### `arch_timer_int_handler()` — arch_clock.c:72-74 (i386)

i386 上为**空函数**（arch_clock.c:72-74）。`timer_int_handler()` 末尾调用它（clock.c:170），但在 i386 上不做任何事。quantum 递减**不在此函数**，而在 `context_stop()` 中。

#### `context_stop()` — arch_clock.c:208-349 (i386)

上下文切换路径的核心函数，由 `proc.c:208/440/1956` 及汇编入口（mpx.S, apic_asm.S）调用。**quantum 递减在此**（arch_clock.c:326-330）：
```c
tsc_delta = tsc - *__tsc_ctr_switch;  // arch_clock.c:272
if (p->p_endpoint >= 0) {              // arch_clock.c:314 — 跳过 kernel/idle
    if (tsc_delta < p->p_cpu_time_left) {
        p->p_cpu_time_left -= tsc_delta;  // ← arch_clock.c:327 quantum 递减
    } else {
        p->p_cpu_time_left = 0;           // ← arch_clock.c:329 饱和到 0
    }
}
*__tsc_ctr_switch = tsc;                // arch_clock.c:342 — 更新基线
```

> **关键纠正**：旧文档与早期 design.md 骨架曾把 quantum 递减归到 `switch_to_user()` 或 `timer_int_handler()` 或 `arch_timer_int_handler()` 内。C ground truth 证明前三者皆错——`arch_timer_int_handler()` 在 i386 为空函数；`switch_to_user()` 只**检查** `!p_cpu_time_left`（proc.c:421-422），不递减；`timer_int_handler()` 完全不涉及 quantum。实际 quantum 递减归 `context_stop()`（上下文切换路径）。详见 §3 D9。

#### `boot_cpu_init_timer()` / `app_cpu_init_timer()` — clock.c:294-312

- BSP：初始化本地定时器并注册 `timer_int_handler` 为中断处理函数
- AP：初始化本地定时器，**不注册全局 handler**（AP 也有本地 tick，但 `cpu_is_bsp(cpuid)` 为 false，跳过全局时间维护）

#### `set_kernel_timer()` / `reset_kernel_timer()` — clock.c:229-255

设置/重置内核定时器。`tp` 是 `minix_timer_t *`（通常来自 `&sp->s_alarm_timer`），用于在链表中定位。

```c
void set_kernel_timer(minix_timer_t *tp, clock_t exp_time,
                     tmr_func_t watchdog, int arg) {
    tmrs_settimer(&clock_timers, tp, exp_time, watchdog, arg, NULL, NULL);
}
void reset_kernel_timer(minix_timer_t *tp) {
    if (tmr_is_set(tp))
        tmrs_clrtimer(&clock_timers, tp, NULL, NULL);
}
```

#### `cause_alarm()` — do_setalarm.c:69-76

同步闹钟到期回调。`tmr_arg` 存储目标进程端点，`mini_notify(CLOCK, proc_nr_e)` 发送通知。

```c
static void cause_alarm(int proc_nr_e) {
    mini_notify(proc_addr(CLOCK), proc_nr_e);
}
```

#### `vtimer_check()` — do_vtimer.c:81-103

检查虚拟/性能定时器是否到期。若到期，清除标志并发信号。

```c
void vtimer_check(struct proc * rp) {
    if ((rp->p_misc_flags & MF_VIRT_TIMER) && rp->p_virt_left == 0) {
        rp->p_misc_flags &= ~MF_VIRT_TIMER;
        rp->p_virt_left = 0;
        cause_sig(rp->p_nr, SIGVTALRM);
    }
    if ((rp->p_misc_flags & MF_PROF_TIMER) && rp->p_prof_left == 0) {
        rp->p_misc_flags &= ~MF_PROF_TIMER;
        rp->p_prof_left = 0;
        cause_sig(rp->p_nr, SIGPROF);
    }
}
```

> **注意**：`vtimer_check` 注释（do_vtimer.c:83-88）说明"called from clock task, only need to protect against clock handler interference"——这解释了为什么不需要额外锁：BKL 已保护整个 `timer_int_handler()`。

#### `load_update()` — clock.c:260-292

按 `_LOAD_UNIT_SECS` 秒间隔采样就绪队列进程数到 `proc_load_history[]`。槽位切换时清零新槽位。

### 2.4 调用关系

```
硬件定时器中断
  → IDT[IRQ+32] → irq_handle（14-doc §1.2）
  → timer_int_handler()                              [clock.c:70]
    ├── (BSP) kclockinfo.uptime++ / realtime++       [clock.c:91-104]
    ├── p->p_user_time++                              [clock.c:116]
    ├── (!BILLABLE) billp->p_sys_time++               [clock.c:118-120]
    ├── (MF_VIRT_TIMER) p->p_virt_left--              [clock.c:128-130]
    ├── (MF_PROF_TIMER) p->p_prof_left--              [clock.c:131-133]
    ├── (!BILLABLE && billp MF_PROF_TIMER) billp->p_prof_left--  [clock.c:134-138]
    ├── vtimer_check(p)                               [clock.c:145]  → do_vtimer.c:81
    │   └── cause_sig(SIGVTALRM / SIGPROF)
    ├── (p != billp) vtimer_check(billp)              [clock.c:147-148]
    ├── load_update()                                 [clock.c:151]  → clock.c:260
    ├── (BSP) tmrs_exptimers(&clock_timers, uptime)   [clock.c:159-161]
    │   └── cause_alarm(endpoint)                     → do_setalarm.c:69
    │       └── mini_notify(CLOCK, endpoint)
    └── arch_timer_int_handler()                      [clock.c:170]  → arch_clock.c:72-74 (i386 空函数)
        └── (quantum 递减不在此；归 context_stop() 上下文切换路径)
    ...

context_stop(proc)                                    [proc.c:208/440/1956]  → arch_clock.c:208-349
    └── p->p_cpu_time_left -= tsc_delta               [arch_clock.c:326-330]

SYS_SETALARM 系统调用
  → do_setalarm()                                     [do_setalarm.c:22]
    ├── SYS_PROC 权限检查                              [do_setalarm.c:33]
    ├── 读取旧 s_alarm_timer.time_left                [do_setalarm.c:40-46]
    ├── reset_kernel_timer(tp)  // 若 exp_time=0      [do_setalarm.c:57]
    └── set_kernel_timer(tp, exp_time, cause_alarm, endpoint)  [do_setalarm.c:61]

SYS_VTIMER 系统调用
  → do_vtimer()                                       [do_vtimer.c:21]
    ├── SYS_PROC 权限检查                              [do_vtimer.c:31]
    ├── 设置/清除 MF_VIRT_TIMER / MF_PROF_TIMER       [do_vtimer.c:60-69]
    └── 设置 p_virt_left / p_prof_left
```

### 2.5 设计要点/特殊处理

1. **BSP vs AP 职责分离**：`uptime`/`realtime`/`clock_timers` 仅 BSP 维护（`cpu_is_bsp(cpuid)` 分支），AP 的 `timer_int_handler` 跳过全局时间，只做进程记账与负载平均
2. **adjtime 限速**：`uptime & 0x1` 检查确保每两个 tick 才调整一次 realtime，避免过快调整；`uptime` 本身不受影响，保持严格单调
3. **非 BILLABLE 进程双重记账**：内核任务的"用户时间"计为 billable 进程的"系统时间"——这保证了用户进程的系统时间被正确归因
4. **profile timer 双重递减**：当前进程的 `p_prof_left` 与 billp 的 `p_prof_left` 都可能递减——profile 计 user+sys 时间，而 billp 的 sys 时间就是当前进程的 user 时间
5. **`vtimer_check` 无额外锁**：BKL 已保护整个 `timer_int_handler()`，注释明确说明只需防时钟 handler 干扰（do_vtimer.c:83-88）
6. **`clock_timers` 链表排序**：按 `tmr_exp_time` 升序，`tmrs_exptimers()` 从队首处理所有到期定时器，O(k) 处理 k 个到期定时器

---

## Ch3: Rust 设计决策

> **设计原则**：避免 C 代码的 translate，用 Rust 类型系统重新表达 C 的指针语义（TimerId newtype）、函数指针（TimerAction enum）、BSP/AP 分支（per-CPU 实例）。每个决策都列多方案对比，优中选优。完整方案对比见 §3。

### 3.1 决策 D1：全局时钟状态封装

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. `static mut` 全局变量 | 直接对应 C `kclockinfo` | translate 直观 | 违反 Rust 2024 安全模式；字段散落 |
| **B. `ClockState` struct** | 封装为 struct，BKL 保护下可变 | 封装性；字段聚簇；ownership 清晰 | — |

**选定 B**。理由：BKL 保护下无需 interior mutability；封装性优于全局变量；与 14-doc `IrqManager` 全局化模式一致。

### 3.2 决策 D2：定时器队列数据结构

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. `BTreeMap<u64, TimerEntry>` | exp_time 作 key | O(log N) 插入 | **同 exp_time 多 timer 覆盖**（C 链表支持） |
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

### 3.3 决策 D3：定时器 identity

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. exp_time 作 key | C `minix_timer_t *tp` 的 translate 误用 | 简单 | **同 exp_time 无法区分**；reset_timer(exp_time) 误取消其他 timer |
| **B. `TimerId(u64)` newtype** | stable identity | 类型安全；支持 reset(id) | 需生成 id（per-ClockState 计数器） |

**选定 B**。理由：C 用 timer 指针作 stable identity（结构体地址不变），Rust 用 newtype 替代指针语义。`TimerId` 是 per-ClockState 单调递增计数器，无需全局同步（BKL 保护）。

**持久性论证**（id 作为 stable identity 是否安全）：`TimerId` 是 per-ClockState 计数器，若 `ClockState` 重建则 id 会重置——但 `ClockState` 是 BKL 保护的全局状态，boot 后**不重建**，id 在系统生命周期内单调不减，因此可以充当 stable identity。Live update 等重建场景由 16-smp.md / 24-cross-space-runtime.md 处理，不在本章范围。`TimerId` 存入 `KPriv.s_alarm_timer`（字段类型变更为 `Option<(TimerEntry, TimerId)>`）无序列化风险：KPriv 不跨重启序列化（boot 时重建），Debug 输出由自动派生覆盖（tuple 实现 Debug）。

### 3.4 决策 D4：adjtime 机制

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 保留** | `adjtime_delta` + `uptime & 0x1` 隔 tick 调整 | NTP 兼容；与 C 对齐 | — |
| B. 删除 | — | 简化 | 丢失 NTP 支持；违反 "rewrite 保留外部行为" |

**选定 A**。理由：adjtime 是 SYS_SETTIME 系统调用的核心语义（21-doc D4），删除会破坏 POSIX 兼容。

### 3.5 决策 D5：负载平均

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 保留** | `LoadInfo` + circular buffer | 与 C 对齐；支持 `getloadavg(3)` | — |
| B. 简化 | 仅就绪队列计数 | 简化 | 丢失历史；破坏 `getloadavg` |

**选定 A**。理由：负载历史是 `do_times` 系统调用返回的核心数据，简化会破坏用户态 `getloadavg(3)`。

### 3.6 决策 D6：`cause_alarm` 回调

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. 函数指针 `tmr_func_t` | C 原始 | — | unsafe；类型不安全；no_std 不友好 |
| **B. `TimerAction` enum** | `NotifyAlarm { endpoint }` | 类型安全；enum 分发 | — |
| C. trait object `Box<dyn TimerHandler>` | 动态分发 | 扩展性 | 堆分配；no_std 不友好；过度抽象 |

**选定 B**（保留），但**删除 `KernelCallback { id }` 变体**。删除理由：当前实现无任何调用方，违反 YAGNI；如未来需要内核定时器回调（如 watchdog），再添加变体并接入。

### 3.7 决策 D7：`hz` 配置

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. 运行时环境变量 | C 原始（`env_get("hz")`） | 灵活 | 运行时解析；`env_get` 不在 no_std |
| **B. 编译时常量 + 运行时可覆盖** | `DEFAULT_HZ` + `with_hz()` | 编译期优化 + 灵活 | — |

**选定 B**（保留）。理由：编译时常量 `DEFAULT_HZ=100` 满足默认场景；`with_hz(hz)` 支持 2..=50000 范围覆盖（与 C 的 `env_get("hz")` 等价）。

### 3.8 决策 D8：BSP/AP 分支

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. 运行时 `cpu_is_bsp()` 分支 | C 原始 | SMP 兼容 | 每次中断有分支 |
| B. `PerCpuTick` const IS_BSP（旧实现） | 编译期单态化 | 零分支 | **SMP 单镜像无效**（BSP/AP 共用 binary，编译期无法区分） |
| **C. per-CPU `ClockState` 实例** | BSP 实例有 timers，AP 无 | SMP 兼容 + 数据分离 + 类型安全 | 内存开销（可接受，每实例 < 200B） |
| D. Typestate + 运行时 downcast | 类型安全 + SMP | 复杂；downcast 开销 | — |

**选定 C**。理由：
1. SMP 单镜像模型下，BSP/AP 在运行时确定（同一个 binary 在所有 CPU 上运行），编译期 const 无法区分
2. per-CPU 实例天然支持 SMP：每个 CPU 有自己的 `ClockState`，BSP 实例的 `is_bsp=true` 拥有 `timers`，AP 实例 `is_bsp=false` 的 `timers` 为空（不变量）
3. 数据局部性：per-CPU 实例缓存友好
4. 类型安全：`is_bsp` 字段运行时判断，但数据结构层面 AP 实例的 `timers` 始终为空

**过渡方案**（架构演进 ARCH）：SMP 框架就绪前，保留单一全局 `ClockState` 实例（`is_bsp=true`），通过 `tick_bsp` / `tick_ap` 便捷方法区分。SMP 就绪后迁移到 per-CPU 实例。

### 3.9 决策 D9：tick() quantum 职责

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. tick 内 `quantum.consume(1)`（旧实现） | — | — | **违反 C 语义**：quantum 不在 `timer_int_handler`；且用固定 1 而非 TSC delta |
| B. 移除 quantum，归 switch_to_user | 与 10-doc 表面对齐 | — | **错误**：switch_to_user 只检查 `!p_cpu_time_left`（proc.c:421-422），不递减 |
| **C. 移除 quantum，归架构层（基于 TSC delta）** | 与 C `context_stop()` 对齐 | 正确性；基于 TSC delta | 需 `ClockArch` trait 扩展（或 kernel 函数调用 trait） |

**选定 C**。理由（**修正 review 骨架的错误归处**）：
- C ground truth：`context_stop()` (arch_clock.c:326-330，在 line 208-349 函数内) 基于 `tsc_delta` 递减 `p_cpu_time_left`；`arch_timer_int_handler()` 在 i386 为空函数
- 14-doc §1.2 verified："timer_int_handler 不递减 quantum | clock.c:70-173 无 p_cpu_time_left"
- 10-doc §2.1 阶段 4：switch_to_user 只**检查** `!p_cpu_time_left`（proc.c:421-422），不递减

**实现方式调整**：原设计在 `ClockArch` trait 上加 `arch_tick(&mut KProcess, ...)` 方法，但 `KProcess` 在 `minix-kernel` crate，`ClockArch` 在 `minix-arch` crate，会造成循环依赖。改为：在 `minix-kernel` 中实现 `clock::decrement_quantum()` 函数，调用 `ClockArch::read_tsc()` 获取 TSC delta，再应用到 `current_proc`。这**保留了 D9 的核心不变量**：quantum 不在 `ClockState::tick()`。

### 3.10 决策 D10：billp 记账接口

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. `_is_billable: bool` 未使用（旧实现） | — | — | **缺失 C 关键行为**（billp->p_sys_time + billp->p_prof_left + vtimer_check(billp)） |
| **B. 显式 `billp: Option<&mut KProcess>` 参数** | 调用方传入 billp | 显式；对齐 C `get_cpulocal_var(bill_ptr)` | 需调用方查找 billp |
| C. 传 `&mut ProcessTable` | 内部查找 billp | 接口简单 | 过宽；隐藏依赖；borrow 冲突（current_proc + billp 同表） |
| D. 封装 `AccountingCtx` | billp 引用包装 | 可扩展 | 过度封装；YAGNI |

**选定 B**。理由：
1. 显式传 billp 与 C `get_cpulocal_var(bill_ptr)` 语义对齐
2. 调用方（trap 入口）已知 billp（per-CPU `bill_ptr` 变量）
3. `Option<&mut KProcess>` 处理 `is_billable=true` 时 billp=None（无需记账）
4. 避免 `&mut ProcessTable` 的 borrow 冲突（current_proc 和 billp 可能同表）

### 3.11 决策 D11：vtimer_check 函数去留

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| A. 保留独立 `vtimer_check` 函数（旧实现） | — | — | 与 tick 内 `tick_virt_timer`/`tick_prof_timer` 逻辑重复 |
| **B. 删除独立函数** | tick 内已递减+检查 | 无重复 | 进程退出清理需新函数 |
| C. `vtimer_check` 改为 `vtimer_cleanup_on_exit` | 退出时用 | 语义清晰 | — |

**选定 B + C 组合**。理由：
1. 删除独立 `vtimer_check`：与 tick 内 `tick_virt_timer`/`tick_prof_timer` 返回 bool 重复
2. tick 内通过 `tick_virt_timer()` / `tick_prof_timer()` 返回 `expired: bool`，直接设置 `vtimer_expired`
3. 如需进程退出时清理 vtimer 标志，新增 `vtimer_cleanup_on_exit(proc)` 函数（当前无此需求，YAGNI）

---

## Ch4: 实现详解

> 实现位置：`os/kernel/src/clock.rs`、`os/kernel/src/syscall_clock.rs`、`os/arch/src/arch/clock.rs`。设计与实现同步：本节代码片段与源码一致，行号引用以源码为准。

### 4.1 ClockState — 全局时钟状态（D1, D8）

> 设计决策 D1（封装为 struct）+ D8（per-CPU 实例 + is_bsp 标志）。

```rust
// os/kernel/src/clock.rs

#[derive(Debug)]
pub struct ClockState {
    /// CPU id this instance belongs to.
    /// C: `cpuid` — `get_cpulocal_var(cpu)`
    cpu_id: CpuId,
    /// `true` iff this is the BSP instance (owns global time + alarm timers).
    /// C: `cpu_is_bsp(cpuid)` — clock.c:91
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
    /// C: `clock_timers` (clock.c:37)
    /// D2: TimerQueue (BTreeSet + BTreeMap) replaces linked list.
    timers: TimerQueue,
    /// Load average info. C: `kloadinfo` (all CPUs)
    load_info: LoadInfo,
}
```

**与 C 的关键差异**：
- 新增 `cpu_id` + `is_bsp` 字段（替代 `cpu_is_bsp(cpuid)` 全局查询）
- `timers` 类型从 C 链表改为 `TimerQueue`（D2）
- 所有字段 BKL 保护下可变，无 interior mutability

**全局原子镜像**：`uptime`/`realtime`/`boottime` 通过 `CLOCK_UPTIME`/`CLOCK_REALTIME`/`CLOCK_BOOTTIME` 三个 `AtomicU64` 镜像到全局，使 `get_monotonic()`/`get_realtime()`/`get_boottime()` 无需 `&ClockState` 即可读取（用于 scheduler 等不便传递 `&ClockState` 的路径）。

### 4.2 TimerId — 定时器 stable identity（D3）

> 设计决策 D3：用 newtype 替代 C 的 `minix_timer_t *tp` 指针语义。

```rust
// os/kernel/src/clock.rs

/// Stable identity for a timer, replacing C's `minix_timer_t *tp` pointer.
///
/// C uses the timer struct pointer as stable identity for set/reset operations
/// (`set_kernel_timer(tp, ...)` / `reset_kernel_timer(tp)`). Rust cannot use
/// pointers (timers are not intrusive linked-list nodes), so we use a newtype
/// wrapping a per-`ClockState` monotonic counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimerId(u64);

impl TimerId {
    pub const fn new(id: u64) -> Self { Self(id) }
    pub fn raw(self) -> u64 { self.0 }
}
```

**设计要点**：
- `TimerId` 是 per-ClockState 单调递增计数器，BKL 保护下无需原子操作
- `set_timer()` 返回 `TimerId`，调用方必须存储以便后续 `reset_timer(id)`
- 与 C `minix_timer_t *tp` 的对应：C 用结构体地址（不变），Rust 用 u64 计数器（不变）

### 4.3 TimerQueue — 双索引定时器队列（D2）

> 设计决策 D2：`BTreeSet<(exp_time, TimerId)>` + `BTreeMap<TimerId, TimerEntry>` 双索引。

```rust
// os/kernel/src/clock.rs

#[derive(Debug, Default)]
struct TimerQueue {
    /// Sorted by (exp_time, id) — supports O(k log N) expiry scan.
    by_expiry: BTreeSet<(u64, TimerId)>,
    /// Lookup by TimerId — supports O(log N) reset_timer(id).
    /// BTreeMap (not HashMap) because `HashMap` is not in `alloc::collections`.
    by_id: BTreeMap<TimerId, TimerEntry>,
    /// Next TimerId counter (per-ClockState, monotonic).
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
        let first = self.by_expiry.iter().next().copied()?;
        if first.0 > now { return None; }
        self.by_expiry.remove(&first);
        self.by_id.remove(&first.1)
    }
}
```

**与 C 链表的关键差异**：
- 同 exp_time 多 timer 自然支持（id 不同，`(exp_time, id)` 唯一）——修复了旧实现 `BTreeMap<u64, _>` 同 exp_time 覆盖的 P1 缺陷
- `pop_expired` 按到期顺序弹出（BTreeSet 自动按 `(exp_time, id)` 排序）
- 纯 `alloc::collections`，无外部依赖

### 4.4 TimerEntry + TimerAction — 定时器条目与动作（D6）

> 设计决策 D6：enum 替代函数指针；删除 `KernelCallback` 变体。

```rust
// os/kernel/src/clock.rs

#[derive(Debug, Clone)]
pub struct TimerEntry {
    /// Expiration time in monotonic ticks. C: `tmr_exp_time`
    pub exp_time: u64,
    /// Action to take when timer expires. Replaces C's `tmr_func_t` callback.
    pub action: TimerAction,
}

/// Action to take when a timer expires.
///
/// Replaces C's `tmr_func_t` function pointer + `tmr_arg` integer.
/// C: `cause_alarm(proc_nr_e)` — do_setalarm.c:69-76
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerAction {
    /// Notify a process via synchronous alarm.
    /// C: `cause_alarm()` → `mini_notify(CLOCK, endpoint)`
    NotifyAlarm { endpoint: Endpoint },
    // KernelCallback variant DELETED (D6): was dead code with no callers.
    // If kernel-internal timer callbacks are needed in the future (e.g.
    // watchdog), add a new variant here and wire up the dispatch.
}
```

### 4.5 ClockState::tick() — 时钟中断处理（D8, D9, D10）

> 设计决策 D8（per-CPU 实例 + is_bsp 运行时分支）+ D9（quantum 不在此）+ D10（显式 billp 参数）。

```rust
// os/kernel/src/clock.rs

/// Handle a timer interrupt tick.
///
/// C: `timer_int_handler()` — clock.c:70-173
///
/// # BKL Precondition
///
/// Caller must hold the BKL. The C pattern is "caller holds BKL", not
/// "callee acquires BKL".
///
/// # Arguments
///
/// * `current_proc` — the currently running process (for time accounting)
/// * `billp` — the billable process if `current_proc` is not billable;
///   `None` if `current_proc` is itself billable (D10).
/// * `ready_count` — number of processes in ready queues (for load average)
///
/// # Returns
///
/// `TimerTickResult` containing expired alarms and vtimer status.
/// **Does NOT contain `quantum_exhausted`** — quantum decrement is handled
/// by `clock::decrement_quantum()` (D9), which the caller invokes separately.
///
/// # Performance (R-06)
///
/// This method collects expired alarms into a `Vec<TimerAction>`. For
/// hot paths (e.g. timer interrupt handler in production), prefer
/// `tick_with` which uses a callback and avoids heap allocation.
pub fn tick(
    &mut self,
    current_proc: &mut KProcess,
    billp: Option<&mut KProcess>,
    ready_count: usize,
) -> TimerTickResult {
    // R-06 (2026-08-12): Delegate to `tick_with` with a Vec collector.
    // Pre-allocate with capacity 4 to avoid reallocation on first pushes.
    let mut expired_alarms = Vec::with_capacity(4);
    let vtimer_expired = self.tick_with(
        current_proc, billp, ready_count,
        |action| expired_alarms.push(action),
    );
    TimerTickResult { expired_alarms, vtimer_expired }
}

/// Zero-allocation variant of `tick` for hot paths (R-06, 2026-08-12).
///
/// Instead of collecting expired alarms into a `Vec` (which heap-allocates
/// on push), this variant invokes `on_expired` for each expired alarm
/// timer inline. Production interrupt handlers should prefer this variant;
/// tests and convenience code may use `tick` which returns a
/// `TimerTickResult` with `Vec<TimerAction>`.
pub fn tick_with<F>(
    &mut self,
    current_proc: &mut KProcess,
    billp: Option<&mut KProcess>,
    ready_count: usize,
    mut on_expired: F,
) -> Option<VtimerExpired>
where F: FnMut(TimerAction) {
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

    // 5. BSP-only: invoke callback for each expired alarm timer
    //    C: clock.c:153-161
    //    R-06: zero-allocation — callback instead of Vec::push.
    if self.is_bsp {
        while let Some(entry) = self.timers.pop_expired(self.uptime) {
            on_expired(entry.action);
        }
    }

    // 6. Load update (all CPUs)
    //    C: clock.c:151
    self.load_update(ready_count);

    vtimer_expired
}
```

> **R-06（2026-08-12）热路径零分配**：原 `tick()` 在每次调用都用 `Vec::new()` 分配（`collect_expired_timers` 内部），timer 中断属于高频热路径。重构为 `tick_with(callback)` 零分配生产 API + `tick()` 兼容包装（`Vec::with_capacity(4)` 预分配）。生产代码（中断入口）应使用 `tick_with`；测试和便捷代码继续用 `tick`。`collect_expired_timers` 辅助方法已移除（逻辑内联到 `tick_with`）。

**与 C 的语义对齐**：
- 步骤 1-6 与 `timer_int_handler()` 的步骤一一对应
- billp 通过 `Option<&mut KProcess>` 显式传递，`None` 表示当前进程可计费（`BILLABLE`）
- quantum 递减**不在此方法**——D9 修正旧实现的错误归处

### 4.6 TimerTickResult — tick 返回值

```rust
// os/kernel/src/clock.rs

#[derive(Debug, Default)]
pub struct TimerTickResult {
    /// Actions from expired alarm timers (BSP only).
    /// C: `tmrs_exptimers()` output — clock.c:159-161
    pub expired_alarms: Vec<TimerAction>,
    /// Virtual/profile timer expiry for current or billable process, if any.
    /// C: `vtimer_check()` output — do_vtimer.c:81-103
    pub vtimer_expired: Option<VtimerExpired>,
    // REMOVED: quantum_exhausted — quantum is now in clock::decrement_quantum() (D9)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtimerExpired {
    /// Virtual timer expired → `SIGVTALRM`. C: `VT_VIRTUAL` (1)
    Virtual,
    /// Profile timer expired → `SIGPROF`. C: `VT_PROF` (2)
    Prof,
}
```

### 4.7 set_timer / reset_timer — 定时器管理接口（D3）

> 设计决策 D3：`set_timer` 返回 `TimerId`，`reset_timer` 接受 `TimerId`。

```rust
// os/kernel/src/clock.rs

impl ClockState {
    /// Set a kernel timer. Returns the assigned `TimerId` for later reset.
    /// C: `set_kernel_timer()` — clock.c:229-240
    ///
    /// # Panics
    ///
    /// Panics if called on an AP instance (alarm timers are BSP-only).
    pub fn set_timer(&mut self, entry: TimerEntry) -> TimerId {
        assert!(self.is_bsp, "set_timer called on AP ClockState (timers are BSP-only)");
        self.timers.insert(entry)
    }

    /// Reset (remove) a kernel timer by `TimerId`.
    /// C: `reset_kernel_timer()` — clock.c:245-255
    pub fn reset_timer(&mut self, id: TimerId) -> Option<TimerEntry> {
        if !self.is_bsp { return None; }
        self.timers.remove(id)
    }
}
```

**与 C 的接口差异**：
- C：`set_kernel_timer(tp, exp_time, watchdog, arg)` —— `tp` 是 `minix_timer_t *`（结构体地址），调用方持有结构体内存
- Rust：`set_timer(entry: TimerEntry) -> TimerId` —— 调用方传入 entry（所有权转移），返回 `TimerId` 用于后续 reset
- C：`reset_kernel_timer(tp)` —— 用 `tp` 指针定位
- Rust：`reset_timer(id: TimerId)` —— 用 `id` 定位

### 4.8 syscall_clock.rs 调用方适配（D3 配套）

> `dispatch_setalarm` 改用新 `set_timer`/`reset_timer` 接口；`KPriv.s_alarm_timer` 字段类型变更以存储 `TimerId`。

**KPriv 字段变更**（`os/kernel/src/kpriv.rs`）：

```rust
pub(crate) struct PrivRuntime {
    /// Synchronous alarm timer + its `TimerId` for queue management.
    ///
    /// `None` when no alarm is set. When `Some`, holds `(entry, id)` where
    /// `id` is the `TimerId` returned by `ClockState::set_timer()`, used to
    /// call `ClockState::reset_timer(id)` when the alarm is cancelled or
    /// replaced (15-design.md §4.4).
    pub(crate) s_alarm_timer: Option<(crate::clock::TimerEntry, crate::clock::TimerId)>,
    // ... other fields
}
```

**dispatch_setalarm 调用方**（`os/kernel/src/syscall_clock.rs`）：

```rust
// Reset path (exp_time == 0 && !abs_time):
if let Some((_old_entry, old_id)) = kpriv.runtime.s_alarm_timer.take() {
    clock_state.reset_timer(old_id);  // 用 TimerId reset，非 exp_time
}

// Set path:
if let Some((_old_entry, old_id)) = kpriv.runtime.s_alarm_timer.take() {
    clock_state.reset_timer(old_id);  // 先取消旧 timer
}
let id = clock_state.set_timer(timer.clone());
kpriv.runtime.s_alarm_timer = Some((timer, id));  // 存储 (entry, id)
```

> **关键修正**：旧 Rust 实现调用 `reset_timer(old_timer.exp_time)`（用 exp_time 作 key），会误取消同 exp_time 的其他 timer。新实现用 `TimerId` 精确定位。

### 4.9 ClockArch trait — 硬件定时器抽象（D9 配套）

> 设计决策 D9：quantum 递减归架构层。`ClockArch` trait 在 `os/arch/src/arch/clock.rs` 定义，提供 `read_tsc()`；quantum 递减的具体实现在 `os/kernel/src/clock.rs` 的 `decrement_quantum()` 函数中（避免 `minix-arch` → `minix-kernel` 循环依赖）。

```rust
// os/arch/src/arch/clock.rs

pub trait ClockArch: Sized + Send + Sync {
    /// Create an instance from a timer descriptor.
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self;

    /// Configure and start the hardware timer at the given frequency.
    /// C: init_clock() hardware portion + arch_init() APIC timer
    fn init_timer(&mut self, hz: u32);

    /// Read the current hardware tick count.
    fn read_ticks(&self) -> u64;

    /// Read the CPU's Time Stamp Counter (cycle counter).
    ///
    /// C: `read_tsc_64()` — arch/i386/arch_clock.c / arch/earm/arch_clock.c
    ///
    /// # D9: quantum decrement
    ///
    /// The kernel's `clock::decrement_quantum()` calls this to compute the
    /// TSC delta since the last tick, then decrements the current process's
    /// `p_cpu_time_left`. This matches C's `context_stop()`
    /// (arch_clock.c:326-330 within line 208-349: `p->p_cpu_time_left -= tsc_delta`).
    fn read_tsc(&self) -> u64 {
        self.read_ticks()
    }

    /// Stop the per-CPU local timer.
    ///
    /// Called by the SMP `ipi_halt_handler` before halting a CPU
    /// (smp.rs:682-686). Disables the LAPIC Timer (x86-64), Generic
    /// Timer (ARM64), or CLINT timer (RISC-V) to prevent interrupts
    /// during the halt.
    ///
    /// C: `stop_local_timer()` — inline in `smp_ipi_halt_handler()`
    /// (smp.c:56-61) as `lapic_stop_timer()`.
    fn stop_local_timer(&mut self);

    /// Initialize and start the statistical profiling timer.
    ///
    /// Called by `SYS_SPROF` with `action=PROF_START` and
    /// `intr_type=PROF_RTC` (misc.rs:899-902).
    ///
    /// C: `init_profile_clock(freq)` — sprofile.c:init_profile_clock
    fn init_profile_clock(&mut self, hz: u32) -> Result<(), ()>;

    /// Stop the statistical profiling timer.
    ///
    /// Called by `SYS_SPROF` with `action=PROF_STOP` (misc.rs:926-928).
    ///
    /// C: `stop_profile_clock()` — sprofile.c:stop_profile_clock
    fn stop_profile_clock(&mut self);
}
```

**架构映射**：

| Method | x86-64 | ARM64 | RISC-V |
|--------|--------|-------|--------|
| `new()` | store PIT freq + LAPIC base from `PitDesc` | no-op (CNTFRQ read at runtime) | store CLINT addrs from `ClintDesc` |
| `init_timer()` | 8254 PIT divisor / LAPIC Timer | ARM Generic Timer (CNTFRQ/CNTPCT) | CLINT mtimecmp |
| `read_ticks()` | TSC (rdtsc) | CNTPCT_EL0 | mtime (MMIO) |
| `read_tsc()` | TSC (rdtsc) | CNTPCT_EL0 | mtime (MMIO) |
| `stop_local_timer()` | LAPIC Timer mask | Generic Timer disable | CLINT mtimecmp clear |
| `init_profile_clock()` | RTC profiling | second Generic Timer channel | Err (no spare CLINT) |
| `stop_profile_clock()` | RTC disable | second channel disable | no-op |

### 4.9.1 decrement_quantum — quantum 递减实现（D9）

`decrement_quantum()` 是 D9 在 kernel 侧的落点。对应 C `context_stop()` 的 quantum 递减段（arch_clock.c:314,326-330,342，在 line 208-349 函数内），但**不包含** C 中其他 `context_stop` 的工作（`p_cycles`/`kbill_ipc`/`tsc_per_state`/`cpuavg` 等记账），这些归后续文档。

为支持单元测试，拆为三层：

```rust
// os/kernel/src/clock.rs

/// 生产入口：读取硬件 TSC + 使用全局 SMP_STATE。
/// C: context_stop() quantum 递减段 — arch_clock.c:326-330 (within line 208-349)
///
/// # Safety
/// 调用方必须持有 BKL（或处于单线程 boot/test 上下文）。
pub fn decrement_quantum(current_proc: &mut KProcess) -> bool {
    decrement_quantum_with_tsc(current_proc, read_tsc())
}

/// 测试入口：注入 TSC 值（test build 中 read_tsc() 返回 0）。
/// 仍使用全局 SMP_STATE。
pub(crate) fn decrement_quantum_with_tsc(
    current_proc: &mut KProcess,
    current_tsc: u64,
) -> bool {
    // SAFETY: caller holds BKL.
    unsafe {
        match crate::try_smp_state() {
            Some(smp) => decrement_quantum_in(smp, current_proc, current_tsc),
            None => false,
        }
    }
}

/// 核心逻辑：所有依赖注入。单元测试直接调用，避免并行测试对
/// 全局 SMP_STATE 的数据竞争（UB）。
fn decrement_quantum_in(
    smp: &mut crate::smp::SmpState,
    current_proc: &mut KProcess,
    current_tsc: u64,
) -> bool {
    let cpu = smp.bsp_cpu_id();
    let last_tsc = match smp.cpu_local_mut(cpu) {
        Some(local) => {
            let last = local.tsc_ctr_switch;
            // C: arch_clock.c:342 — `*__tsc_ctr_switch = tsc;`
            // 不论进程类型都更新基线（kernel/idle 也推进 TSC 计数器）。
            local.tsc_ctr_switch = current_tsc;
            last
        }
        None => return false,
    };

    // 首次调用：last_tsc=0 → 建立基线，不递减。
    if last_tsc == 0 { return false; }

    let delta = current_tsc.saturating_sub(last_tsc);
    if delta == 0 { return false; }

    // C: arch_clock.c:314 — 跳过 kernel/idle 任务（endpoint < 0）。
    if current_proc.p_endpoint.get() < 0 { return false; }

    // C: arch_clock.c:326-330 — `p_cpu_time_left -= tsc_delta`（饱和到 0）。
    // Quantum::consume 通过 CAS 执行饱和递减，quantum 耗尽时返回 true。
    current_proc.p_sched.quantum.consume(delta)
}
```

**三层拆分的设计动机**：

| 层 | 职责 | 依赖 |
|----|------|------|
| `decrement_quantum` | 生产入口 | 硬件 TSC + 全局 SMP_STATE |
| `decrement_quantum_with_tsc` | 注入 TSC（绕过硬件） | 全局 SMP_STATE |
| `decrement_quantum_in` | 核心逻辑 | 注入 `&mut SmpState` |

`decrement_quantum_in` 的存在是**测试驱动的设计改进**：C 用全局 `*__tsc_ctr_switch` 因为 C 没有良好的依赖注入模式；Rust 通过参数化 `&mut SmpState` 使核心逻辑可在不触碰全局状态的情况下测试，避免并行测试间的 UB。

**与 C 的语义对齐**：
- 步骤顺序：更新基线 → 计算 delta → 跳过判断 → 递减 quantum（与 arch_clock.c:272,314,326-330,342 一致）
- 饱和语义：`Quantum::consume` 内部 CAS 实现的饱和递减与 C 的 `if (tsc_delta < cpu_time_left) ... else p_cpu_time_left = 0;` 等价
- 跳过规则：`endpoint < 0` 对应 C `p_endpoint >= 0` 的反条件

### 4.10 load_update — 负载平均采样

```rust
// os/kernel/src/clock.rs

impl ClockState {
    /// Update load average tracking.
    /// C: `load_update()` — clock.c:260-292
    fn load_update(&mut self, ready_count: usize) {
        let slot = ((self.uptime / self.hz as u64 / LOAD_UNIT_SECS)
                    % LOAD_HISTORY as u64) as u16;

        if slot != self.load_info.proc_last_slot {
            self.load_info.proc_load_history[slot as usize] = 0;
            self.load_info.proc_last_slot = slot;
        }

        self.load_info.proc_load_history[slot as usize] += ready_count as u32;
        self.load_info.last_clock = self.uptime;
    }
}
```

**与 C 的接口差异**：C 在 `load_update()` 内部遍历 `run_q_head` 计算就绪进程数；Rust 由调用方传入 `ready_count`，分离了"统计"与"采样"职责，便于测试。

---

## Ch5: 测试要点

> 测试位置：`os/kernel/src/clock.rs` `#[cfg(test)] mod tests`（44 个测试）+ `os/kernel/src/syscall_clock.rs`（10 个测试，`clock::` 过滤器匹配 `syscall_clock::`）。当前 54 个测试全部通过（`cargo test -p minix-kernel --lib clock::`）。

> **R-06（2026-08-12）新增测试**：`test_tick_with_invokes_callback_on_expiry`（callback 在 timer 过期时被调用）+ `test_tick_with_no_allocation_on_empty_expiry`（无过期时 callback 不调用，零分配热路径）+ `test_tick_with_matches_tick_behavior`（`tick` 与 `tick_with` 行为一致）。

### 5.1 L1 对偶测试（C-Rust 行为一致）

| 测试名 | C 行为 | Rust 断言 |
|--------|--------|----------|
| `test_tick_bsp_increments_uptime_realtime` | clock.c:92-102 | uptime+1, realtime+1 |
| `test_tick_ap_no_uptime_update` | clock.c:91 `if (cpu_is_bsp)` | AP uptime 不变 |
| `test_adjtime_speed_up` | clock.c:97-100 奇数 tick realtime+=2 | delta=3 → tick1: rt=2, delta=2 |
| `test_adjtime_slow_down` | clock.c:99 奇数 tick realtime+=0 | delta=-2 → tick1: rt=0, delta=-1 |
| `test_billp_sys_time_accounting` | clock.c:118-120 !BILLABLE | billp.sys_time+1 |
| `test_billp_prof_timer_decrement` | clock.c:134-138 !BILLABLE | billp.prof_left 递减 |
| `test_billp_prof_timer_expiry_reports_prof` | clock.c:147-148 vtimer_check(billp) | billp 到期报告 Prof |
| `test_vtimer_virtual_expiry` | do_vtimer.c:91-95 | VIRT_TIMER + virt_left=0 → Virtual |
| `test_vtimer_prof_expiry` | do_vtimer.c:98-102 | PROF_TIMER + prof_left=0 → Prof |
| `test_timer_set_and_expire` | clock.c:159-161 tmrs_exptimers | set_timer(t=3), tick 3 → expired |
| `test_timer_reset_by_id` | clock.c:245-255 reset_kernel_timer | set_timer → reset_timer(id) → 不触发 |
| `test_user_time_accounting` | clock.c:116 p->p_user_time++ | tick 后 user_time+1 |

### 5.2 L2 契约测试（trait/类型契约）

| 测试名 | 契约 |
|--------|------|
| `test_timer_id_uniqueness` | 同一 ClockState 内 set_timer 多次返回不同 TimerId |
| `test_timer_queue_same_exp_time` | 同 exp_time 两个 timer 都能到期触发（修复 P1：旧 BTreeMap 同 exp_time 覆盖） |
| `test_ap_state_set_timer_panics` | AP 实例 set_timer panic（D8: AP 不拥有 timer 队列） |
| `test_ap_state_reset_timer_returns_none` | AP 实例 reset_timer 返回 None |
| `test_load_update_slot_rotation` | uptime 推进 → slot 切换 → history 清零 |
| `test_timer_queue_pop_expired_order` | 3 个 timer 不同 exp_time → 按顺序弹出 |
| `test_tick_with_invokes_callback_on_expiry` (R-06) | `tick_with` callback 在 timer 过期时被调用一次 |
| `test_tick_with_no_allocation_on_empty_expiry` (R-06) | 无过期时 callback 不调用（零分配热路径） |
| `test_tick_with_matches_tick_behavior` (R-06) | `tick` 与 `tick_with` 行为一致（结果相同） |

### 5.3 边界用例

| 测试名 | 边界 |
|--------|------|
| `test_clock_state_hz_bounds` | hz=1 / hz=50001 → fallback DEFAULT_HZ |
| `test_adjtime_zero_delta` | delta=0 → realtime 每tick+1 |
| `test_timer_never_expires` | exp_time=u64::MAX → 不触发 |
| `test_vtimer_no_expiry_when_flag_not_set` | virt_left=0 但 MF_VIRT_TIMER 未设 → None |
| `test_billp_no_accounting_when_none` | billp=None → sys_time 不变 |
| `test_ap_set_boottime_no_op` | AP 实例 set_boottime 无效 |

### 5.4 quantum 递减测试（D9 配套）

`clock::decrement_quantum()` 函数已实现，对应 C 的 `context_stop()` 中 quantum 递减段（arch_clock.c:314,326-330,342，在 line 208-349 函数内）。为支持单元测试，拆为三层（见 §4.9.1）：

| 测试名 | C 行为 | Rust 断言 |
|--------|--------|----------|
| `test_decrement_quantum_no_smp_state_returns_false` | 早期 boot 无 `SMP_STATE` | 返回 false，不改 `cpu_time_left` |
| `test_decrement_quantum_first_call_no_baseline` | `*__tsc_ctr_switch=0` 时建立基线 | 返回 false，更新 `tsc_ctr_switch` |
| `test_decrement_quantum_decrements_cpu_time_left` | arch_clock.c:326-327 `tsc_delta < cpu_time_left` | quantum 减少 delta，不报耗尽 |
| `test_decrement_quantum_reports_exhaustion` | arch_clock.c:328-329 `else p_cpu_time_left = 0` | delta == quantum → 返回 true，quantum=0 |
| `test_decrement_quantum_overshoot_saturates_to_zero` | arch_clock.c:328-329 饱和到 0（不回绕） | delta >> quantum → 返回 true，quantum=0 |
| `test_decrement_quantum_skips_kernel_tasks` | arch_clock.c:314 `if (p->p_endpoint >= 0)` | KERNEL 端点 -1 → 不递减 quantum，但更新基线 |
| `test_decrement_quantum_zero_delta_returns_false` | TSC 未推进（退化情况） | delta=0 → 返回 false，不改 quantum |
| `test_decrement_quantum_multiple_ticks_accumulate` | 多 tick 累积 delta | 3 次调用总 delta = 60_000，quantum 减 60_000 |

**测试策略**：通过 `decrement_quantum_in(smp, proc, tsc)` 注入 `&mut SmpState`，避免并行测试对全局 `SMP_STATE` 的数据竞争（UB）。生产入口 `decrement_quantum(proc)` 内部调用全局 `try_smp_state()`，仅在 QEMU 集成测试中验证。

---

## Ch6: 参见

### 6.1 上游文档（前置依赖）

- [14-exception-interrupt.md](14-exception-interrupt.md) — 时钟中断的入口路径与"三条激活路径"定位；§1.2 verified "timer_int_handler 不递减 quantum"
- [05-clock-interrupt-init.md](05-clock-interrupt-init.md) — `init_clock()` 的启动阶段定位；硬件定时器配置
- [10-switch-to-user.md](10-switch-to-user.md) — quantum 检查（`!p_cpu_time_left`）在 switch_to_user 阶段 4；§2.1 仅检查不递减
- [11-scheduling-primitives.md](11-scheduling-primitives.md) — quantum 大小设置（`ms_2_cpu_time(p_quantum_size_ms)`）；调度原语

### 6.2 下游文档（引用本章）

- [16-smp.md](16-smp.md) — AP 定时器初始化（`app_cpu_init_timer()`）；BSP/AP 时钟职责分离
- [21-syscall-clock.md](21-syscall-clock.md) — 时钟系统调用（SYS_SETALARM/SYS_VTIMER/SYS_STIME/SYS_SETTIME/SYS_TIMES）的用户态接口

### 6.4 跨文档一致性

| 文档 | 关系 | 状态 |
|------|------|------|
| 14-exception-interrupt | 14 §1.2 把时钟中断定位为"三条激活路径之一"；15 承接进入+返回 | ✅ 已双向 |
| 05-clock-interrupt-init | 05 讲 `init_clock()` 初始化，15 讲运行时 tick | ✅ 已双向 |
| 10-switch-to-user | 10 §2.1 阶段 4 quantum 检查，15 D9 quantum 递减归架构层 | ✅ 已协调（10 仅检查不递减） |
| 11-scheduling-primitives | 11 quantum 设置，15 quantum 递减 | ✅ 已双向 |
| 16-smp | 16 AP 定时器初始化，15 BSP/AP 职责分离 | ✅ 已双向 |
| 21-syscall-clock | 21 时钟系统调用，15 内核实现 | ✅ 已同步（21 已更新为 `reset_timer(id)` TimerId 接口） |
