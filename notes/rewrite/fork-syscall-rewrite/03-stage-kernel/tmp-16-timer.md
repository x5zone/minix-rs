# 16-timer: 时钟与定时器

> **分类**: Kernel 时间与初始化
> **源码**: `minix3/minix/kernel/clock.c`(312行), `arch/i386/arch_clock.c`(440行)
> **说明**: 时钟滴答、虚拟/剖面定时器、看门狗、实时时钟——内核的时间基础设施

---

## 1. 概述

### 1.1 概念定义/作用

**时钟与定时器**是 Minix3 内核的时间基础设施，为系统提供时间计量、定时唤醒和进程计时功能。时钟中断是内核最频繁的外部事件之一，每次中断驱动内核更新时间、检查定时器、管理进程时间片。

Minix3 的时钟系统提供以下功能：

1. **系统时间维护**：`uptime`（单调递增的 tick 计数）和 `realtime`（可调整的墙上时钟 tick 计数）
2. **同步闹钟定时器**：系统进程通过 `sys_setalarm` 设置的内核定时器，到期时通过 IPC 通知
3. **虚拟定时器（Virtual Timer）**：仅计用户态运行时间，到期发送 SIGVTALRM
4. **剖面定时器（Profile Timer）**：计用户态+内核态运行时间，到期发送 SIGPROF
5. **进程时间统计**：`p_user_time`（用户态 tick）和 `p_sys_time`（内核态 tick）
6. **负载平均**：统计就绪进程数，供 `getloadavg(3)` 使用

### 1.2 与 Minix3 的对应关系

| 功能 | 函数 | 源文件 |
|------|------|--------|
| 时钟中断处理 | `timer_int_handler()` | clock.c:70 |
| 时钟初始化 | `init_clock()` | clock.c:47 |
| BSP 定时器初始化 | `boot_cpu_init_timer()` | clock.c:294 |
| AP 定时器初始化 | `app_cpu_init_timer()` | clock.c:306 |
| 获取单调时间 | `get_monotonic()` | clock.c:203 |
| 获取墙上时间 | `get_realtime()` | clock.c:178 |
| 设置闹钟 | `do_setalarm()` | system/do_setalarm.c |
| 获取运行时间 | `do_times()` | system/do_times.c |
| 虚拟定时器 | `do_vtimer()` | system/do_vtimer.c |
| 设置启动时间 | `do_stime()` | system/do_stime.c |
| 设置系统时间 | `do_settime()` | system/do_settime.c |
| 架构相关定时器 | `arch_timer_int_handler()` | arch/i386/arch_clock.c |
| 虚拟定时器检查 | `vtimer_check()` | clock.c（内联） |

### 1.3 关键状态/机制说明

**kclockinfo 全局结构**：存储时钟的全局状态，包括 `hz`（时钟频率，默认 100Hz）、`uptime`（单调 tick）、`realtime`（可调 tick）、`boottime`（UNIX 纪元秒数）。

**clock_timers 队列**：内核的同步闹钟定时器队列，使用 `minix_timer_t` 链表实现。每个系统进程有一个 `s_alarm_timer`，通过 `sys_setalarm` 设置。到期时定时器的 watchdog 函数被调用，向进程发送通知。

**时间调整（adjtime）**：`adjtime_delta` 变量控制系统时间的渐进调整。正值加速 realtime（每两个 tick 前进 2），负值减速（每两个 tick 前进 0），直到 delta 归零。这实现了 NTP 风格的时间平滑调整。

**BSP 与 AP 的时钟差异**：BSP（Bootstrap Processor）维护全局 `uptime` 和 `realtime`，处理同步闹钟定时器。AP（Application Processor）仅更新本地进程计时和虚拟定时器，不维护全局时间。

### 1.4 行为规则

1. **BSP 唯一维护全局时间**：`uptime` 和 `realtime` 仅在 BSP 的时钟中断中递增
2. **单调时间不可调**：`uptime` 严格单调递增，不受 `adjtime` 影响
3. **墙上时间可调**：`realtime` 通过 `adjtime_delta` 渐进调整，每两个 tick 调整一次
4. **进程计费规则**：当前进程计用户时间；若当前进程不可计费（非 BILLABLE），计费进程计系统时间
5. **虚拟定时器仅计用户态**：`p_virt_left` 仅在进程运行于用户态时递减
6. **剖面定时器计用户+系统态**：`p_prof_left` 在进程运行时递减，且计费进程的 profile 也递减
7. **定时器到期通知 CLOCK 任务**：同步闹钟到期后，CLOCK 任务通过 IPC 通知设置闹钟的进程

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 时钟频率常量

| 常量 | 值 | 含义 |
|------|-----|------|
| `DEFAULT_HZ` | 100 | 默认时钟频率（100Hz = 10ms/tick） |
| `kclockinfo.hz` | 可配置 | 实际时钟频率（2 ~ 50000） |

#### 2.1.2 定时器相关 MISC_FLAGS

| 标志 | 值 | 含义 |
|------|-----|------|
| `MF_VIRT_TIMER` | 0x002 | 虚拟定时器运行中 |
| `MF_PROF_TIMER` | 0x004 | 剖面定时器运行中 |

#### 2.1.3 负载平均常量

| 常量 | 含义 |
|------|------|
| `_LOAD_UNIT_SECS` | 负载采样单元（秒） |
| `_LOAD_HISTORY` | 负载历史缓冲区大小 |

### 2.2 核心数据结构

#### 2.2.1 kclockinfo 全局时钟信息

| 字段 | 类型 | 含义 |
|------|------|------|
| `hz` | `unsigned` | 时钟频率（tick/秒） |
| `uptime` | `clock_t` | 单调递增的 tick 计数（启动后） |
| `realtime` | `clock_t` | 可调整的墙上时钟 tick 计数 |
| `boottime` | `time_t` | 系统启动时的 UNIX 纪元秒数 |

#### 2.2.2 kloadinfo 负载信息

| 字段 | 类型 | 含义 |
|------|------|------|
| `proc_last_slot` | `u16_t` | 上次采样的时间槽 |
| `proc_load_history[]` | `u32_t[]` | 负载历史环形缓冲区 |
| `last_clock` | `clock_t` | 上次更新时刻 |

#### 2.2.3 minix_timer_t 定时器结构

| 字段 | 类型 | 含义 |
|------|------|------|
| `tmr_exp_time` | `clock_t` | 到期时刻（uptime tick） |
| `tmr_func` | `tmr_func_t` | 到期时调用的 watchdog 函数 |
| `tmr_arg` | `int` | watchdog 函数参数 |
| `tmr_next` | `minix_timer_t *` | 队列中下一个定时器 |

### 2.3 关键函数分析

#### 2.3.1 timer_int_handler()——时钟中断处理

`minix3/minix/kernel/clock.c:70-173`

```c
int timer_int_handler(void)
```

**功能**：BSP/AP 的时钟中断处理函数，每次时钟 tick 调用。

**行为**（按执行顺序）：

1. **看门狗 tick**（`USE_WATCHDOG`）：递增 `watchdog_local_timer_ticks`
2. **BSP 全局时间更新**：
   - `kclockinfo.uptime++`
   - `realtime` 调整：若 `adjtime_delta != 0` 且 uptime 为奇数，加速或减速
3. **进程时间统计**：
   - `p->p_user_time++`（当前进程用户时间）
   - 若当前进程不可计费：`billp->p_sys_time++`（计费进程系统时间）
4. **虚拟/剖面定时器递减**：
   - `p_virt_left--`（若 `MF_VIRT_TIMER` 且 `p_virt_left > 0`）
   - `p_prof_left--`（若 `MF_PROF_TIMER` 且 `p_prof_left > 0`）
   - 计费进程的 `p_prof_left--`（若当前进程不可计费）
5. **虚拟定时器到期检查**：`vtimer_check(p)` 和 `vtimer_check(billp)`
6. **负载平均更新**：`load_update()`
7. **BSP 同步闹钟检查**：若 `clock_timers` 有到期定时器，调用 `tmrs_exptimers()`
8. **串口调试**（`DEBUG_SERIAL`）：`do_ser_debug()`
9. **架构相关处理**：`arch_timer_int_handler()`

#### 2.3.2 init_clock()——时钟初始化

`minix3/minix/kernel/clock.c:47-64`

```c
void init_clock(void)
```

**功能**：初始化时钟变量。

**行为**：
1. 清零 `kclockinfo`
2. 从环境变量 `hz` 读取时钟频率（2 ~ 50000），默认 `DEFAULT_HZ(100)`
3. 清零 `kloadinfo`

#### 2.3.3 set_kernel_timer() / reset_kernel_timer()——内核定时器操作

`minix3/minix/kernel/clock.c:229-255`

```c
void set_kernel_timer(minix_timer_t *tp, clock_t exp_time,
    tmr_func_t watchdog, int arg)
void reset_kernel_timer(minix_timer_t *tp)
```

**功能**：设置/重置内核定时器。

**行为**：
- `set_kernel_timer`：调用 `tmrs_settimer()` 将定时器插入 `clock_timers` 队列，按到期时间排序
- `reset_kernel_timer`：调用 `tmrs_clrtimer()` 从队列中移除定时器

**使用场景**：系统进程的 `s_alarm_timer`、看门狗定时器、内核内部定时器。

#### 2.3.4 load_update()——负载平均更新

`minix3/minix/kernel/clock.c:260-292`

```c
static void load_update(void)
```

**功能**：统计当前就绪进程数，更新负载历史。

**行为**：
1. 计算当前时间槽：`(uptime / hz / _LOAD_UNIT_SECS) % _LOAD_HISTORY`
2. 若进入新时间槽，清零该槽的计数器
3. 遍历所有优先级就绪队列，统计就绪进程数
4. 累加到当前时间槽的计数器

#### 2.3.5 do_setalarm()——设置同步闹钟

`minix3/minix/kernel/system/do_setalarm.c`

```c
int do_setalarm(struct proc *caller, message *m_ptr)
```

**功能**：为系统进程设置同步闹钟定时器。

**行为**：
1. 从消息中提取目标进程和到期时间
2. 若到期时间为 0，重置定时器（`reset_kernel_timer`）
3. 若到期时间非 0，设置定时器（`set_kernel_timer`），watchdog 函数为 `cause_alarm`
4. `cause_alarm` 到期时向进程发送通知

#### 2.3.6 do_times()——获取运行时间

`minix3/minix/kernel/system/do_times.c`

```c
int do_times(struct proc *caller, message *m_ptr)
```

**功能**：返回进程的用户态时间、系统态时间和系统 uptime。

#### 2.3.7 do_vtimer()——虚拟/剖面定时器

`minix3/minix/kernel/system/do_vtimer.c`

```c
int do_vtimer(struct proc *caller, message *m_ptr)
```

**功能**：设置或查询进程的虚拟/剖面定时器。

**行为**：
1. 从消息中提取目标进程、定时器类型和剩余时间
2. 设置 `MF_VIRT_TIMER` / `MF_PROF_TIMER` 标志
3. 设置 `p_virt_left` / `p_prof_left` 剩余 tick

### 2.4 调用关系/调用点分析

#### 2.4.1 时钟中断处理链

```
硬件时钟中断
  └─ arch_timer_int_handler()（架构相关入口）
       └─ timer_int_handler()
            ├─ [BSP] uptime++, realtime 更新
            ├─ 进程时间统计 (p_user_time, p_sys_time)
            ├─ 虚拟/剖面定时器递减
            ├─ vtimer_check() → cause_sig(SIGVTALRM/SIGPROF)
            ├─ load_update()
            ├─ [BSP] clock_timers 到期检查
            │    └─ cause_alarm() → mini_notify()
            └─ arch_timer_int_handler()（架构相关后处理）
```

#### 2.4.2 同步闹钟定时器路径

```
系统进程调用 sys_setalarm(exp_time)
  └─ do_setalarm()
       └─ set_kernel_timer(&s_alarm_timer, exp_time, cause_alarm, proc_nr)
            └─ [到期时] cause_alarm()
                 └─ mini_notify(CLOCK, proc_endpoint)
```

#### 2.4.3 虚拟定时器到期路径

```
时钟中断 → p_virt_left-- → p_virt_left == 0
  └─ vtimer_check()
       └─ cause_sig(proc_nr, SIGVTALRM)
            └─ PM 处理信号
```

### 2.5 设计要点/特殊处理

#### 2.5.1 BSP/AP 时钟职责分离

SMP 下，BSP 负责全局时间维护和同步闹钟定时器，AP 仅更新本地进程计时。这避免了多 CPU 同时修改全局时间变量的竞态条件。AP 的时钟中断频率与 BSP 相同，但不递增 `uptime` 和 `realtime`。

#### 2.5.2 进程计费的双重规则

时钟中断中，当前进程的 `p_user_time` 总是递增。但若当前进程不可计费（如内核任务），计费进程的 `p_sys_time` 递增。这确保了用户进程的系统时间被正确归因——当内核代表用户进程执行时，时间被计入该用户进程。

#### 2.5.3 adjtime 的渐进调整

`adjtime_delta` 实现了 NTP 风格的时间平滑调整。不是一次性跳变，而是每两个 tick 调整一个 tick（加速前进 2 或减速前进 0）。这避免了时间突变对应用程序的影响。`uptime` 不受调整影响，保持严格单调。

#### 2.5.4 虚拟定时器与剖面定时器的区别

- **虚拟定时器**（`MF_VIRT_TIMER`）：仅计用户态时间，到期发送 `SIGVTALRM`
- **剖面定时器**（`MF_PROF_TIMER`）：计用户态+系统态时间，到期发送 `SIGPROF`

剖面定时器还额外递减计费进程的 `p_prof_left`——因为一个进程的用户时间是另一个进程（计费进程）的系统时间。

#### 2.5.5 clock_timers 队列的排序

`clock_timers` 队列按到期时间排序，最早到期的在队首。`tmrs_exptimers()` 从队首开始处理所有到期定时器，直到遇到未到期的定时器为止。这确保了定时器处理的效率——O(k) 处理 k 个到期定时器，而非 O(n) 遍历全部。

#### 2.5.6 看门狗定时器

`USE_WATCHDOG` 条件编译下，`watchdog_local_timer_ticks` 在每次时钟中断时递增。看门狗代码定期检查此变量是否在增长，若停滞则认为内核死锁，触发重启。
