# 14 — 定时器：`do_itimer` 三族与 `set_alarm`/`cause_sigalrm` 的 `ticks` 驱动

本文讲清 PM 的定时器如何以“`timeval ↔ ticks` 的向上取整与溢出钳位 + `ALARM_ON` 的周期状态 + `REAL` 经 `set_timer/cause_sigalrm→check_sig(SIGALRM)` 与 `VIRTUAL/PROF` 经 `sys_vtimer→check_vtimer` 的双后端 + `CLOCK notify → expire_timers` 的驱动”为完整链路，使 `setitimer(2)` 的 `it_value/it_interval` 在 wall-clock 与虚拟 CPU 时钟上的语义可区分，且 `SIGALRM` 的 `ksig==FALSE` 使其可被 `SIG_IGN` 忽略。

前置阅读：04-ipc-dispatch.md（`is_ipc_notify` 的 `CLOCK` 分发与 `ExpireTimers` 的回调模型）、11-signal-core.md（`check_sig` 的 `SIGALRM` 投递与 `process_ksig` 的 `SIGVTALRM/SIGPROF → check_vtimer` 重启）、02-mproc-struct.md（`MinixTimer/intervals[3]/ALARM_ON` 三件套的 `ProcessResources` 分层）、12-signal-handlers.md（`SignalState::pending & !mask` 的可忽略语义与 `without_unkillable`）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `check_sig` 的 `SIGALRM` 生产（11 `cause_sigalrm` 路径）与 `ProcTable` 的 `MinixTimer` 槽位（02）、`CLOCK` 通知的 `is_ipc_notify` 分发（04）的开发者；知道 `Timeval { tv_sec, tv_usec }` 与 `Itimerval { it_value, it_interval }` 的 POSIX 定义（`sys/time.h`）。

> **本章不讲什么**：
> - 内核 `minix_timer_t` 队列与 `TMRDIFF_MAX` 时序（`set_timer/expire_timers/tmr_exp_time/getticks/sys_vtimer` 的 `kernel/timers.c` 队列管理）—— `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md`
> - `CLOCK` 时钟源与 `system_hz` 的获取（`sys_hz`/`system_hz` 的 `libsys` 路径）—— `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/21-clock-device.md`
> - 信号的 `ignore/mask/caught` 投递细节与 `PROC_STOPPED/DELAY_CALL` 的延迟恢复—— `11-signal-core.md` / `12-signal-handlers.md` / `13-signal-flow.md`
>
> 本章只回答一个问题：**PM 如何为“一段时间后发 `SIGALRM`，若有区间则周期重设”的 `itimer` 语义建立 `timeval↔ticks` 的无溢出、向上取整的变换，并以 `ALARM_ON` 的 `Option` 状态与 `REAL`/`VIRTUAL` 双后端与 `CLOCK` 驱动完成到期→投递→重设的闭环**。

### 1.1 为什么 PM 需要三族定时器：`REAL` vs `VIRTUAL` vs `PROF` 的分野

POSIX 的三族 `itimer` 对应三种时钟源（`sys/time.h:ITIMER_REAL 0/VIRTUAL 1/PROF 2`，`NR_ITIMERS 3`）：

- `ITIMER_REAL`（`SIGALRM 14`）：**wall-clock**（真实时间），无论进程是否运行，到期即发 `SIGALRM`。Minix3 的 `REAL` 经 PM 本地 `mp_timer` + `set_timer/cause_sigalrm` 实现（`alarm.c:305-338`），信号由 PM 的 `check_sig(pid, SIGALRM, FALSE)` 生产（`alarm.c:343` `ksig==FALSE`）。
- `ITIMER_VIRTUAL`（`SIGVTALRM 26`）：**用户态 CPU 时间**（进程在用户态的 `ticks`），仅进程运行在用户态时计时。经内核 `sys_vtimer(VT_VIRTUAL 1)` 实现（`alarm.c:205`），信号由内核的 `SIGVTALRM` 经 `process_ksig:326-328` → `check_vtimer` 重设。
- `ITIMER_PROF`（`SIGPROF 27`）：**用户+内核 CPU 时间**（`profiling`），经内核 `sys_vtimer(VT_PROF 2)` 同路径（`alarm.c:205` `VT_PROF`）。

分野的必然：`REAL` 的 `ALARM_ON` 仅 `REAL` 私有（`mproc.h:90 0x10`，`02` 的 `RemainingFlags::ALARM_ON`），`VIRTUAL/PROF` 无此位（其状态在内核 `proc` 的计时队列）；`getset_vtimer` 的 `nptr/optr` 双指针即此分野的 Rust 端 `Option<Clock>`（§3.4）。

### 1.2 为什么 `timeval ↔ ticks` 必须向上取整与溢出钳位：`ALRM_EXP_TIME` 的类型困境

`alarm.c:30-51` 注释是理解变换权衡的一手证据，值得精读：

> *Large delays cause a lot of problems... converting from seconds to ticks can easily overflow... Fixing this requires a lot of ugly casts... ALRM_EXP_TIME has the right type (clock_t) although it is declared as long.*

三类大延迟风险：

1. **库的 `seconds→int` 截断**：`alarm(2)` 的 `unsigned seconds` 被库 cast 为 `int`，库在返回时把“负” `unsigned` 转为错误——调用者假定“大秒数总能透传”。
2. **`sec→ticks` 乘法溢出**：`system_hz * sec` 在 `unsigned long == long` 时易溢出（32 位下 `hz 100 * sec 2^31` 溢出）。
3. **内核 `ticks` 加法溢出**：内核 `timers.c` 的 `exptime = now + ticks` 同理溢出。

Minix3 的解法是两级钳位 + 向上取整：

- `ticks_from_timeval`（`33-65`）：`ticks = hz*sec; if (ticks/hz != sec) ticks = LONG_MAX`（`56-57` 乘法溢出钳位）+ `ticks += (hz*usec + US-1)/US`（`59` `usec→ticks` 向上取整，`US=1e6`）+ `ticks>LONG_MAX→LONG_MAX`（`62`）——“请求 1us 在 100Hz 下至少 1 tick”，大延迟不溢出内核队列。
- `timeval_from_ticks`（`70-76`）：`sec = ticks/hz`（`74`）+ `usec = (ticks%hz)*US/hz`（`75`）——`%hz` 先取余再乘 `US` 避免 `ticks*US` 溢出，是无溢出分解。

向上取整是“不饿死”的 fail-safe：请求 `1us` 若向下取整为 0 则 `set_alarm(0)` 会 `cancel_timer`（`307-310`），信号永不到期；向上取整保证至少 1 tick。

### 1.3 为什么 `ITIMER_REAL` 与 `VIRTUAL/PROF` 走不同后端：`set_timer` vs `sys_vtimer`

`do_itimer` 的 `switch(which)`（`125-140`）是分野的调度点：

- `REAL → get_realtimer/set_realtimer`（`126-132`）：PM 本地 `mp_timer` + `mp_interval[0]`，`set_alarm(rmp, ticks)` 的 `set_timer(ticks, cause_sigalrm, ep)`（`305`）把到期回调钉在 `cause_sigalrm`，`CLOCK notify → expire_timers → cause_sigalrm → check_sig(SIGALRM)` 完成投递。
- `VIRTUAL/PROF → getset_vtimer`（`134-140`）：`sys_vtimer(VT_*)` 的 `nptr/optr` 双指针（`205`），信号由内核计时到期后以 `SIGVTALRM/PROF` 经 `process_ksig:326-328` → `check_vtimer` 的 `sys_vtimer(&interval)` 重设（`239-240`）——`PM` 不直接 `set_timer`，仅置 `mp_interval[which]` 并委托内核。

`getset_vtimer` 的 `optr/nptr` 双 `NULL` 初始（`169`）使“仅 `get` 或仅 `set`”的 `Option` 语义在一次 `sys_vtimer` 调用中完成（`205` 的 `nptr/optr` 可空）。

### 1.4 为什么 `do_itimer` 的 `which` 与 `setval/getval` 双指针语义：边界与双向 `sys_datacopy`

`alarm.c:101-110` 的两级校验：

- `which <0||>=3 → EINVAL`（`101`，`NR_ITIMERS 3` 边界）——`which` 的 `0/1/2` 穷尽使 `default→panic` 的 `143` 分支在 `ItimerWhich` 枚举下消失。
- `setval=(value!=0)` `getval=(ovalue!=0)` + `!set&&!get→EINVAL`（`107-110`）——`value/ovalue` 双 `vir_bytes` 指针判空的“至少一个非空”语义，`value` 经 `sys_datacopy` 读新（`116-118`）+ `is_sane`（`120-122`），`ovalue` 经 `sys_datacopy` 写旧（`148-150`）——双向 `sys_datacopy` 使 `do_itimer(get)` 的旧值无需 `set` 即可获取。

`is_sane_timeval` 的 `0<=sec<=MAX_SECS && 0<=usec<US`（`85-86`，`US=1e6`）是 `setval` 前置（`120-122`），`ticks_from_timeval` 的溢出钳位是后置 fail-safe——前者拒绝非法 `timeval`，后者钳位合法大值，二者互补。

### 1.5 为什么 `getset_vtimer` 的 `oldticks<=0→interval` 回绕：区间是“下次到期”而非“当前剩余”

`alarm.c:212` `if(oldticks<=0) oldticks = rmp->mp_interval[which]` 使“已过期但区间非零的虚拟定时器”在 `do_itimer(get)` 时返回区间而非 0——`VIRTUAL/PROF` 的 `get` 语义（`212-214` 的 `timeval_from_ticks`）返回“若定时器已过期，下一次到期是区间”。`set` 侧 `188-189` `if(newticks<=0) interval=0` 使“取消定时器时区间清零”——区间是“下一次到期”，取消时不应保留。

`get_realtimer` 同理回绕：`263` 行 `if(remaining<=0) remaining = interval` 使 `REAL` 的 `get` 在已过期但区间非零时返回区间（`247-273` 的 `ALARM_ON ? remaining : 0` 后回绕）。

### 1.6 为什么 `cause_sigalrm` 的 `ALARM_ON` 与 `interval` 分支：周期重设与 `ksig==FALSE` 可忽略

`alarm.c:330-343` 的三守卫 + 分支：

```
pm_isokendpt→IN_USE|EXITING→ALARM_ON 三守卫（323-331）若任一失败则静默 return（无效/已退出/无闹钟）
    ├── interval>0 → set_alarm(period)（337-338）周期重设（在 expire_timers 回调内 safe，alarm.c:334-335 注释）
    └── else → ~ALARM_ON（339）一次性到期清除
mp=mproc[0]; check_sig(SIGALRM,FALSE)（341-343）kSIG==FALSE 使 SIGALRM 可被 SIG_IGN 忽略（与 SIGVTALRM 的 ksig==TRUE 对照，process_ksig:334）
```

`337-339` 的周期分支与 `set_alarm` 的 `ticks>0→ALARM_ON` 对偶——`REAL` 的 `interval` 驱动周期，`VIRTUAL` 的 `interval` 经 `check_vtimer` 驱动（`239-240`），二者皆“区间非零则重设，否则清”。

### 1.7 与其他 OS 定时器的对照

Rust 改写不是照抄 `system_hz * tv_sec` 乘法，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `setitimer/getitimer`/`timer_create` + `CLOCK_REALTIME/VIRTUAL/PROF`。** Linux 的 `setitimer(ITIMER_REAL, &new, &old)` 同样 `which 0/1/2` + `it_value/it_interval` 双 `timeval` + `interval>0` 周期（`kernel/time/itimer.c:itimer_set`），`CLOCK_REALTIME` 经 `hrtimer` 队列（`kernel/time/hrtimer.c`）的 `expire→posix_timer_fn` vs Minix3 的 `set_timer→cause_sigalrm→check_sig`。Linux 的 `hrtimer` 以 `ktime_t`（`ns`）与 `CLOCK_MONOTONIC` 解耦 `HZ`，Minix3 以 `ticks/hz` 的 `CLOCK` 分辨率暴露 `HZ`（`74-75` 的 `ticks%hz*US/hz` 仍 `HZ` 依赖）；Linux 的 `timerfd_create` + `signalfd` 使到期可经 `fd` 轮询，PM 只能经 `SIGALRM` 信号（`ksig==FALSE` 可忽略）——`fd` 轮询是线程事件循环模型，`SIGALRM` 是 PM 单线程无事件循环复用的历史选择。Linux 的 `MAX_SECS` 同理钳位（`kernel/time/time.c:MAX_SEC_IN_JIFFIES`），Minix3 以 `LONG_MAX` 钳位（`57/62`）。

**Redox `TimeScheme` + `Timeout` 队列。** Redox 以 `TimeScheme { inner: Timeout { heap: BinaryHeap<TimeoutEntry> } }` + `common/src/time.rs` 的 `Duration`/`TimeSpec` 变换（`ns→ticks` 的 `duration_to_ticks` 同 `ticks_from_timeval` 的向上取整），`ITIMER_REAL` 的周期重设经 `Timeout::insert` 的 `next = now + interval`（与 `337-338` 的 `set_alarm(interval)` 同 `interval` 驱动）。Redox 的 `CLOCK` 以 `ksyscall` 的 `clock_gettime(CLOCK_MONOTONIC)`  + `Instant` 解耦 `HZ`，PM 以 `system_hz` 显式参的 `TicksConv { hz }`（`arch/src/arch/clock.rs:100` `ClockArch::new` 同型）保留 `HZ` 显式。

**`seL4` 周期 `Notification`。** `seL4` 无 `itimer`，以 `seL4_SetNTFN` + `timer` 驱动的 `Notification` 周期唤醒等待者（`sel4::Notification::signal`），PM 的 `set_alarm→cause_sigalrm→check_sig` 同为“到期→信号”但 `seL4` 的 `Notification` 为显式 capability（`seL4_Signal`），PM 的 `SIGALRM` 为隐式 `pending` 位图（`12` 的 `add_pending`）——显式 capability 可精准投递单线程，隐式位图需 `check_pending` 重检（13）而 `REAL` 的 `ksig==FALSE` 使 `SIGALRM` 仍可被 `SIG_IGN` 丢弃。

**结论（本章的设计基线）。** 把 C 的“`US` 宏 + `system_hz*sec` 乘法溢出钳位 + `MAX_SECS/US` 约束分散 + `which` 的 `int` 分派 + `optr/nptr` 双 `NULL` + `ALARM_ON` 位与 `mp_timer` 双重 + `cause_sigalrm` 的 `mp=mproc[0]` 伪装”改写为“`TicksConv { hz }` + `Timeval::is_sane` + `ItimerWhich` 枚举 + `VTimerCtl::vtimer` 的 `Option<Clock>` + `AlarmState { timer: Option<MinixTimer> }` 的 `Some/None` 唯一真源 + `SigSender::send_sigalrm` 显式端口”——与 Linux/Redox 的 `ITIMER_*` 三族 + `interval` 周期同源，又因 PM 单线程无共享而以 `&mut ProcTable` 的 `TicksConv` 显式参的向上取整收敛。

### 1.8 小结

1. **为什么三族**——`REAL` 的 `set_timer/cause_sigalrm→SIGALRM`  vs `VIRTUAL/PROF` 的 `sys_vtimer→SIGVTALRM/PROF→check_vtimer`，`ALARM_ON` 仅 `REAL` 私有。
2. **为什么向上取整与钳位**——`1us` 在 `100Hz` 下至少 1 tick（`+US-1`），`LONG_MAX` 钳位防大延迟溢出内核队列（`57/62`）。
3. **为什么双指针**——`value/ovalue` 的 `Option` 语义使 `do_itimer(get)` 无需 `set` 即可获取旧值，`which` 边界 `0..3` 使 `default→panic` 消失。
4. **为什么 `optr/nptr` → `Option`**——`VIRTUAL/PROF` 的 `sys_vtimer` 双 `NULL` 初始在 Rust 以 `Option<Clock>` 的“是否设置/是否获取”穷尽。
5. **为什么回绕**——`oldticks<=0→interval`（`212/263`）使“已过期但区间非零”返回区间而非 0，`newticks<=0→0`（`188-189`）使取消时区间清零。
6. **为什么 `ALARM_ON` → `Option`**——位与队列的双重性以 `Option<MinixTimer>` 唯一真源消除，`cause_sigalrm` 的 `interval>0?set:clear` 周期分支与 `ksig==FALSE` 可忽略对偶。

下一章逐行分析 C 的 `ticks_from_timeval`/`timeval_from_ticks`/`is_sane`/`do_itimer`/`getset_vtimer`/`check_vtimer`/`get/set_realtimer`/`set_alarm`/`cause_sigalrm`；第 3 章给出 Rust 的 `TicksConv`/`ItimerWhich`/`AlarmState`。

---

## 2 C 源码分析

### 2.1 `ticks_from_timeval`（`alarm.c:33-65`）

```c
static clock_t ticks_from_timeval(tv) struct timeval *tv;
{ // 33  timeval→ticks（向上取整，溢出钳位）
  clock_t ticks; // 35
  ticks = system_hz * (unsigned long) tv->tv_sec; // 55  sec→ticks 乘法
  if ( (ticks / system_hz) != (unsigned long)tv->tv_sec) { // 56  乘法溢出检测
    ticks = LONG_MAX; // 57  钳位
  } else {
    ticks += ((system_hz * (unsigned long)tv->tv_usec + (US-1)) / US); // 59  usec→ticks 向上取整
  }
  if (ticks > LONG_MAX) ticks = LONG_MAX; // 62  二次钳位
  return(ticks); // 64
}
```

`38-51` 注释的“三问题”在此兑现：`int`/`unsigned long` 的接口错配使 `tv_sec` 为 `unsigned` 却 cast 为 `int`，`sec→ticks` 乘法在 `sizeof(unsigned)==sizeof(long)` 时溢出，`ticks` 加法在内核 `exptime = now + ticks` 亦溢出。`59` 行 `+ (hz*usec+US-1)/US` 的 `US-1` 是向上取整的关键（`1us` 在 `100Hz` 下 `100*1+999999/1e6 =1`），`56-57` 的 `ticks/hz != sec` 为乘法溢出无 `__builtin_mul_overflow` 时的经典检测。

### 2.2 `timeval_from_ticks`（`alarm.c:70-76`）

```c
static void timeval_from_ticks(tv, ticks) struct timeval *tv; clock_t ticks;
{ // 70  ticks→timeval（无溢出分解）
  tv->tv_sec = (long) (ticks / system_hz); // 74  sec = ticks/hz
  tv->tv_usec = (long) ((ticks % system_hz) * US / system_hz); // 75  usec = (ticks%hz)*US/hz
}
```

`75` 行先 `ticks%hz` 再 `*US` 避免 `ticks*US` 溢出（`ticks` 可达 `LONG_MAX`，`LONG_MAX*US` 恒溢出），`74` 行 `ticks/hz` 的整数除法向零取整与 `Rust` 的 `i64 / hz` 同语义（`TicksConv` 的 `Duration` 变换无需浮点）。

### 2.3 `is_sane_timeval`（`alarm.c:82-87`）

```c
static int is_sane_timeval(struct timeval *tv)
{ // 82  setitimer 的合理区间校验
  return (tv->tv_sec >= 0 && tv->tv_sec <= MAX_SECS && // 85  sec 上界（timers.h: MAX_SECS 100M）
  	  tv->tv_usec >= 0 && tv->tv_usec < US); // 86  usec < 1e6（<US 非 <=US）
}
```

`85-86` 的 `MAX_SECS` 上界使 `ticks_from` 的 `hz*sec` 在 `MAX_SECS*hz` 范围内不溢出 `LONG_MAX`（`MAX_SECS~100M`，`hz 100 → 1e10 < 2^31`），`usec < US` 的互斥上界（`<US` 非 `<=US`）使 `59` 行 `hz*usec` 的 `usec` 上界 `999999` 不溢出 `hz*US`。

### 2.4 `do_itimer`（`alarm.c:92-154`）

```c
int do_itimer(void)
{ // 92  setitimer(2) 的 PM 侧入口
  struct itimerval ovalue, value; int setval, getval; int r, which; // 95-97
  which = m_in.m_lc_pm_itimer.which; // 100  m_lc_pm_itimer.which（ipc.h:469 `which`）
  if (which < 0 || which >= NR_ITIMERS) return(EINVAL); // 101  0..2 边界（NR_ITIMERS 3，const.h:17）
  setval = (m_in.m_lc_pm_itimer.value != 0); // 107  value 非空→set
  getval = (m_in.m_lc_pm_itimer.ovalue != 0); // 108  ovalue 非空→get
  if (!setval && !getval) return(EINVAL); // 110  至少一个非空
  if (setval) { // 115
    r = sys_datacopy(who_e, m_in.m_lc_pm_itimer.value, // 116  读新值
        PM_PROC_NR, (vir_bytes)&value, (phys_bytes)sizeof(value));
    if (r != OK) return(r); // 118  EFAULT 等透传
    if (!is_sane_timeval(&value.it_value) || // 120
        !is_sane_timeval(&value.it_interval)) return(EINVAL); // 121-122  双 is_sane
  }
  switch (which) { // 125
  	case ITIMER_REAL : // 126  0
   		if (getval) get_realtimer(mp, &ovalue); // 127  先 get 旧（若需）
   		if (setval) set_realtimer(mp, &value); // 129  再 set 新
   		r = OK; // 131
   		break;
  	case ITIMER_VIRTUAL : // 134  1
  	case ITIMER_PROF : // 135  2
 		getset_vtimer(mp, which, (setval) ? &value : NULL, // 136  委托 VTimer（getset 合一）
 			(getval) ? &ovalue : NULL); // 137
   		r = OK; // 139
   		break;
  	default: panic("invalid timer type: %d", which); // 143  which 已边界，理论不可达
  }
  if (r == OK && getval) { // 147  get 需写回用户
    r = sys_datacopy(PM_PROC_NR, (vir_bytes)&ovalue, // 148  写旧
        who_e, m_in.m_lc_pm_itimer.ovalue, (phys_bytes)sizeof(ovalue));
  }
  return(r); // 153  OK 或 sys_datacopy 错误
}
```

`107-110` 的“至少一个非空”与 `101` 的 `which` 边界使 `do_itimer(get)` 与 `set` 可独立（`value==NULL` 时仅 `get`，`ovalue==NULL` 时仅 `set`），`126-140` 的 `REAL` vs `VIRTUAL/PROF` 分派使 `REAL` 走 `get/set_realtimer` 的 PM 本地 `mp_timer`，`VIRTUAL/PROF` 走 `getset_vtimer` 的内核 `sys_vtimer`，`143` 的 `panic` 在 `ItimerWhich` 枚举下消失。

### 2.5 `getset_vtimer`（`alarm.c:160-216`）

```c
static void getset_vtimer(rmp, which, value, ovalue)
 struct mproc *rmp; int which; struct itimerval *value, *ovalue; // 160
{ // 160  VIRTUAL/PROF 的 VTimer 包装（委托内核 VT_*）
  clock_t newticks, *nptr; clock_t oldticks, *optr; int r, num; // 162-164
  optr = nptr = NULL; // 169  双 NULL 初始（get/set 可空）
  if (ovalue != NULL) { // 174  需 get 旧
  	optr = &oldticks; // 175
  	timeval_from_ticks(&ovalue->it_interval, rmp->mp_interval[which]); // 177  先拷贝 interval→ovalue.interval（非 ticks）
  }
  if (value != NULL) { // 183  需 set 新
  	newticks = ticks_from_timeval(&value->it_value); // 184
  	nptr = &newticks; // 185
  	if (newticks <= 0) rmp->mp_interval[which] = 0; // 188-189  取消时区间清零
  	else rmp->mp_interval[which] = ticks_from_timeval(&value->it_interval); // 191-192  区间存 ticks
  }
  switch (which) { // 196
  case ITIMER_VIRTUAL: num = VT_VIRTUAL; break; // 197  1
  case ITIMER_PROF:    num = VT_PROF;    break; // 198  2
  default:             panic("invalid vtimer type: %d", which); // 199
  }
  if ((r = sys_vtimer(rmp->mp_endpoint, num, nptr, optr)) != OK) panic("sys_vtimer failed: %d", r); // 205-206
  if (ovalue != NULL) { // 208  需 get 后回填
  	if (oldticks <= 0) oldticks = rmp->mp_interval[which]; // 212  回绕：已过期但区间非零时返回区间
  	timeval_from_ticks(&ovalue->it_value, oldticks); // 214  ticks→timeval 回填
  }
}
```

`174-178` 的 `ovalue→optr` 与 `183-193` 的 `value→nptr` 使 `sys_vtimer` 的 `nptr/optr` 双 `NULL` 语义在一次调用中完成 `get` 与 `set`（`205`），`212` 行 `oldticks<=0→interval` 回绕与 `263` 行 `get_realtimer` 的 `remaining<=0→interval` 同理（“已过期但区间非零时返回区间而非 0”），`188-189` 的 `newticks<=0→0` 使取消时区间清零（区间是“下次到期”而非“当前剩余”）。

### 2.6 `check_vtimer`（`alarm.c:222-241`）

```c
void check_vtimer(int proc_nr, int sig)
{ // 222  VIRTUAL/PROF 重启（signal.c:326-328 经 process_ksig 调用）
  register struct mproc *rmp; int which, num; // 224-225
  rmp = &mproc[proc_nr]; // 227
  switch (sig) { // 230
  case SIGVTALRM: which = ITIMER_VIRTUAL; num = VT_VIRTUAL; break; // 231  26
  case SIGPROF:   which = ITIMER_PROF;    num = VT_PROF;    break; // 232  27
  default: panic("invalid vtimer signal: %d", sig); // 233
  }
  if (rmp->mp_interval[which] > 0) sys_vtimer(rmp->mp_endpoint, num, &rmp->mp_interval[which], NULL); // 239-240  区间非零则重设
}
```

`230-234` 的 `sig→which/num` 翻译与 `getset_vtimer` 的 `196-200` 同表（`SIGVTALRM→ITIMER_VIRTUAL/VT_VIRTUAL`，`SIGPROF→ITIMER_PROF/VT_PROF`），`239-240` 的 `interval>0→sys_vtimer(&interval)` 使 `VIRTUAL/PROF` 的周期重设在 `SIGVTALRM/PROF` 到期后由 `process_ksig` 间接触发（`signal.c:326-328` `check_vtimer(proc_nr, signo)` 后 `check_sig` 的 `SIGVTALRM` 投递）。

### 2.7 `get_realtimer`（`alarm.c:247-273`）

```c
static void get_realtimer(struct mproc *rmp, struct itimerval *value)
{ // 247  读取 REAL 定时器剩余与区间
  clock_t exptime; clock_t uptime; clock_t remaining; // 249-251
  if (rmp->mp_flags & ALARM_ON) { // 254  有闹钟
    uptime = getticks(); // 255  当前 ticks
    exptime = tmr_exp_time(&rmp->mp_timer); // 256  到期 ticks
    remaining = exptime - uptime; // 258  剩余 ticks
    if (remaining <= 0) remaining = rmp->mp_interval[ITIMER_REAL]; // 263  回绕：已过期但区间非零时返回区间
  } else remaining = 0; // 265  无闹钟→0
  timeval_from_ticks(&value->it_value, remaining); // 269  剩余→timeval
  timeval_from_ticks(&value->it_interval, rmp->mp_interval[ITIMER_REAL]); // 272  区间→timeval
}
```

`254-265` 的 `ALARM_ON ? remaining : 0` + `263` 回绕与 `getset_vtimer` 的 `212` 同理，`256` 行 `tmr_exp_time` 的 `mp_timer` 读与 `255` 行 `getticks` 的当前 `ticks` 差即剩余（`expire_timers` 回调的 `cause_sigalrm` 前 `remaining<=0` 时亦回绕）。

### 2.8 `set_realtimer`（`alarm.c:279-294`）

```c
static void set_realtimer(struct mproc *rmp, struct itimerval *value)
{ // 279  设置 REAL 定时器到期与区间
  clock_t ticks; clock_t interval; // 281-282
  ticks = ticks_from_timeval(&value->it_value); // 285  it_value→ticks
  interval = ticks_from_timeval(&value->it_interval); // 286  it_interval→ticks
  if (ticks <= 0) interval = 0; // 289  取消时区间清零（与 getset_vtimer 188-189 同理）
  set_alarm(rmp, ticks); // 292  置/清 ALARM_ON（305-310）
  rmp->mp_interval[ITIMER_REAL] = interval; // 293  区间存 ticks（与 getset_vtimer 191-192 同存）
}
```

`285-286` 的双 `ticks_from` 与 `289` 的 `ticks<=0→0` 使 `set_realtimer` 的 `it_value/it_interval` 的 `timeval→ticks` 双变换后区间归零与 `getset_vtimer` 的 `newticks<=0` 语义一致（取消定时器时不保留区间），`292-293` 的 `set_alarm` 置/清与区间存使 `ALARM_ON` 与 `mp_interval[REAL]` 的“到期+区间”对偶。

### 2.9 `set_alarm`（`alarm.c:299-311`）

```c
void set_alarm(rmp, ticks) struct mproc *rmp; clock_t ticks;
{ // 299  置/清 REAL 闹钟（ALARM_ON 位与 set_timer 对偶）
  if (ticks > 0) { // 303  置闹钟
    assert(ticks <= TMRDIFF_MAX); // 304  TMRDIFF_MAX 时序断言（kernel/timers.h）
    set_timer(&rmp->mp_timer, ticks, cause_sigalrm, rmp->mp_endpoint); // 305  到期回调 cause_sigalrm
    rmp->mp_flags |=  ALARM_ON; // 306  置位
  } else if (rmp->mp_flags & ALARM_ON) { // 307  清闹钟（有闹钟时才清）
    cancel_timer(&rmp->mp_timer); // 308  取消队列
    rmp->mp_flags &= ~ALARM_ON; // 309  清位
  }
}
```

`304` 行 `TMRDIFF_MAX` 断言与 `D1` 的 `LONG_MAX` 钳位对偶（`ticks_from` 的 `LONG_MAX` 保证 `ticks <= TMRDIFF_MAX`，`TMRDIFF_MAX` 来自 `timers.h` 的队列最大差），`305` 行 `set_timer` 的 `cause_sigalrm` 回调与 `CLOCK notify → expire_timers` 接线（`main.c:65-67`）构成“到期→回调→`check_sig`”的 `REAL` 周期。

### 2.10 `cause_sigalrm`（`alarm.c:317-344`）

```c
static void cause_sigalrm(int arg)
{ // 317  REAL 到期回调（expire_timers 经 set_timer 触发）
  int proc_nr_n; register struct mproc *rmp; // 319-320
  if(pm_isokendpt(arg, &proc_nr_n) != OK) { printf("PM: ignoring timer for invalid endpoint %d\n", arg); return; } // 323-326  endpoint 失效静默
  rmp = &mproc[proc_nr_n]; // 328
  if ((rmp->mp_flags & (IN_USE | EXITING)) != IN_USE) return; // 330  已退出/未在用则静默
  if ((rmp->mp_flags & ALARM_ON) == 0) return; // 331  无闹钟则静默（已 cancel）
  if (rmp->mp_interval[ITIMER_REAL] > 0) set_alarm(rmp, rmp->mp_interval[ITIMER_REAL]); // 337-338  区间非零则周期重设（在 expire_timers 回调内 safe，334-335 注释）
  else rmp->mp_flags &= ~ALARM_ON; // 339  一次性到期清位
  mp = &mproc[0]; /* pretend the signal comes from PM */ // 341  全局 mp 伪装（与 process_ksig:312 同伪装）
  check_sig(rmp->mp_pid, SIGALRM, FALSE /* ksig */); // 343  ksig==FALSE 使 SIGALRM 可被 SIG_IGN 忽略（与 SIGVTALRM 的 TRUE 对照）
}
```

`323-331` 的三守卫（`pm_isokendpt→IN_USE|EXITING→ALARM_ON`）与 `11` 的 `process_ksig:300-309` 双守卫同型（`ALARM_ON` 替代 `IN_USE|EXITING` 的 `EDEADEPT`），`337-339` 的 `interval>0?set_alarm:clear` 周期分支与 `CLOCK` 驱动的 `expire_timers` 回调内 `set_timer` 安全（`334-335` 注释 *from within this callback from the expire_timers function. This is safe*），`343` 行 `ksig==FALSE` 使 `SIGALRM` 的 `SIG_IGN` 可忽略（`11` 的 `badignore` 不覆盖 `SIGALRM`，`ign_sset` 亦不含 `SIGALRM`）。

### 2.11 消息与类型（`sys/time.h` / `com.h:420-421` / `mproc.h:62-63/90` / `main.c:65-67`）

- `ITIMER_REAL 0` / `VIRTUAL 1` / `PROF 2` / `NR_ITIMERS 3`（`sys/time.h:??`，`alarm.c:101/126/134-135`）、`VT_VIRTUAL 1` / `VT_PROF 2`（`com.h:420-421`）、`mp_timer/mp_interval[3]`（`mproc.h:62-63`）、`ALARM_ON 0x10`（`mproc.h:90`）、`struct itimerval { it_interval, it_value: timeval }`（`sys/time.h`）、`mess_lc_pm_itimer { which, value, ovalue }`（`ipc.h:469` `which/value/ovalue`，`_ASSERT 56B`）、`SIGALRM 14` / `SIGVTALRM 26` / `SIGPROF 27`（`sys/signal.h`）、`CLOCK notify`（`main.c:65-67` `is_ipc_notify: CLOCK → expire_timers`，`kernel/clock.c:notify`）。

### 2.12 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `ticks` 向上取整与 `LONG_MAX` 钳位 | `alarm.c:59/62` | `1us` 在 `100Hz` 下至少 1 tick，大延迟不溢出 | 不变量 |
| `is_sane` 的 `MAX_SECS/US` | `alarm.c:85-86` | `sec>MAX_SECS` 或 `usec>=US` → `EINVAL` | 可恢复 |
| `REAL` 的 `ALARM_ON` 唯一性 | `mproc.h:90` / `alarm.c:254/306/309/331` | `ALARM_ON` 时 `mp_timer` 在队列 | 不变量（Option 唯一真源） |
| `VIRTUAL/PROF` 的 `sys_vtimer` 路径 | `alarm.c:205/240` | `REAL` 走 `set_timer`，`VIRTUAL` 走 `sys_vtimer` | 不变量 |
| `newticks<=0→interval=0` | `alarm.c:189/289` | 取消时区间清零 | 不变量 |
| `oldticks<=0→interval` 回绕 | `alarm.c:212/263` | 已过期但区间非零时返回区间 | 不变量 |
| `SIGALRM` 的 `ksig==FALSE` | `alarm.c:343` | `SIGALRM` 可被 `SIG_IGN` 忽略（与 `SIGVTALRM` 的 `TRUE` 对照） | 不变量 |

---

## 3 Rust 设计决策

Rust 改写遵循“显式 `TicksConv` + `Timeval::is_sane` + `ItimerWhich` 枚举 + `VTimerCtl` 的 `Option<Clock>` + `remaining` 纯函数 + `AlarmState` 的 `Option` 唯一真源 + `SigSender` 显式端口”的 8 决策，保留 C 的 `which` 分派与 `interval` 回绕，但以类型系统使 `ticks` 变换与 `ALARM_ON` 状态显式化。以下决策对应设计契约 `.design/14-design.v1.md` 的 D1–D8。

### D1：`ticks ↔ timeval` 的 `US` 与 `LONG_MAX` 收敛到 `TicksConv`（ARCH A-7/A-11）

- **C**：`alarm.c:19` `US 1e6` 宏 + `55-62` 乘法溢出钳位 + `74-75` 分解。
- **Rust**：`struct TicksConv { hz: Clock }` + `ticks_from_timeval(&self, tv: &Timeval) -> Clock`（`hz*sec` 的 `checked_mul` 省 `ticks/hz!=sec` 检测，`hz*usec` 的 `checked_mul` + `+US-1` 向上取整，`>i64::MAX→MAX` 钳位）+ `timeval_from_ticks(&self, ticks: Clock) -> Timeval`（`%hz` 先取余再 `*US` 的无溢出分解，`Clock=i64` 的 `A-11` 64 位扩展）。
- **为什么**：`C` 的 `LONG_MAX` 钳位在 `64` 位下为 `i64::MAX`，`Rust` 的 `checked_mul` 替代 `ticks/hz!=sec` 的除法检测，`US-1` 的向上取整保留“`1us→1tick`”的 fail-safe。

### D2：`is_sane_timeval` 收敛到 `Timeval::is_sane`（ARCH A-7）

- **C**：`85-86` `sec 0..=MAX_SECS && usec 0..US`。
- **Rust**：`impl Timeval { fn is_sane(&self) -> bool { 0<=sec && sec<=MAX_SECS && 0<=usec && usec < US } }`（`MAX_SECS` 收敛到 `minix-types::MAX_SECS` 的 `100M` 锁定，`US=1_000_000`）。
- **为什么**：`is_sane` 为 `do_itimer` 的 `setval` 前置校验（`120-122`），与 `ticks_from` 的溢出钳位互补——前者拒绝非法 `timeval`，后者钳位合法大值；方法化使 `getset_vtimer` 的 `VIRTUAL` 路径亦复用同一校验。

### D3：`do_itimer` 的 `which` 与 `setval/getval` 收敛到 `ItimerWhich` + `ItimerOp`（ARCH A-7）

- **C**：`101` `which` 边界 + `107-110` 双指针判空。
- **Rust**：`enum ItimerWhich { Real=0, Virtual=1, Prof=2 }` + `TryFrom<i32>`（`0→Real` 等），`struct ItimerOp { set: Option<Itimerval>, get: bool }`（`set: None` 对应 `value==NULL`，`get: false` 对应 `ovalue==NULL`，`!set&&!get→EINVAL`），`fn do_itimer(table, caller, which, op, &mut dyn VTimerCtl, &mut dyn TimerCtl, &mut dyn CopyCtl) -> Result<Option<Itimerval>, ItimerError>`。
- **为什么**：`C` 的 `value/ovalue` 双 `vir_bytes` 指针判空在 Rust 以 `Option` 显式，`which` 的 `int` 在 Rust 以 `ItimerWhich` 穷尽（`default→panic` 的 `143` 分支消失）。

### D4：`getset_vtimer` 的 `optr/nptr` 收敛到 `Option<Clock>` 的 `VTimerCtl` trait（ARCH A-7）

- **C**：`168-206` 双 `NULL` 初始 + `sys_vtimer(num,nptr,optr)`。
- **Rust**：`trait VTimerCtl { fn vtimer(&mut self, ep: Endpoint, which: ItimerWhich, set: Option<Clock>, get: Option<&mut Clock>) -> i32 }`（`set: Some(ticks)` 对应 `nptr=&newticks`，`get: Some(&mut oldticks)` 对应 `optr=&oldticks`）。

### D5：`get_realtimer` 的 `ALARM_ON ? exptime-uptime : 0` 抽为纯函数（ARCH A-7）

- **C**：`254-265` 的 `ALARM_ON` 分支与回绕可独立测。
- **Rust**：`fn remaining_real(interval: Clock, timer: &Option<MinixTimer>, now: Clock) -> Clock` 纯函数（`Some(exptime) ? exptime-now : 0` + `<=0→interval` 回绕），`get_realtimer` 调用之并双 `timeval_from_ticks`。

### D6：`set_alarm` 的 `ALARM_ON` 位收敛为 `Option<MinixTimer>`（ARCH A-7）

- **C**：`299-311` 的 `ALARM_ON` 位与 `mp_timer` 队列实体双重。
- **Rust**：`ALARM_ON` 唯一真源为 `timer: Option<MinixTimer>` 的 `Some/None`（`RemainingFlags::ALARM_ON` 移除或 `#[deprecated]` 兼容），`TimerCtl::set(ep, ticks, cause_sigalrm)` + `cancel(ep)` trait。

### D7：`cause_sigalrm` 三守卫 + 重设收敛到 `TimerExpiry` 端口（ARCH A-7）

- **C**：`323-343` 三守卫 + `337-339` 区间分支 + `341-343` `mp=mproc[0]; check_sig(SIGALRM,FALSE)`。
- **Rust**：`fn cause_sigalrm(table, ep: Endpoint, sig_sender: &mut dyn SigSender) -> bool`（`pm_isokendpt→IN_USE|EXITING→ALARM_ON→set_or_clear→check_sig`，`ksig==FALSE` 显式参，`SigSender::send_sigalrm(pid)` 抽象 `check_sig`）。

### D8：常量收敛到 `minix-types`（单一真相）

- **C**：`ITIMER_* / VT_* / SIGALRM / ALARM_ON / MAX_SECS / US`。
- **Rust**：`minix-types: ITIMER_REAL 0` 等、`VT_VIRTUAL 1` 等、`SIGALRM 14`、`MAX_SECS`、`US` 单一真相（`alarm.c:19/85` 数值锁定，测试 `test_constants_match_c`）。

### ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-7 定时器抽象 | `TicksConv` + `ItimerWhich`/`VTimerCtl`/`TimerCtl`/`AlarmState`（D1/D3/D4/D6） | `timer.rs` + 本文档 §3.1/3.4/3.6 + 计划 §4 |
| A-11 64位 | `Clock=i64` + `Timeval { sec:i64, usec:i64 }`（D1） | `mproc/mproc.rs` + 本文档 §3.1 + 计划 §4 |
| A-3 全局→显式 | `VTimerCtl`/`TimerCtl`/`SigSender` 显式传参（D4/D6/D7） | `timer.rs` 注释 + 本文档 §3.4/3.6/3.7 + 计划 §4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── mproc/
│   └── mproc.rs         — ProcessResources { timer: Option<MinixTimer>, intervals: [Clock;3] }（A-7，ALARM_ON 已收敛为 Option）
├── timer.rs             — TicksConv + Timeval/Itimerval + ItimerWhich + do_itimer/getset_vtimer/check_vtimer/get_realtimer/set_realtimer/set_alarm/cause_sigalrm + CLOCK 驱动
└── ipc/
    └── mod.rs           — （无新增，VTimer 的 sys_vtimer 经 timer.rs 的 trait 注入）
```

### 4.2 `timer.rs`：变换与分派

```rust
pub const US: i64 = 1_000_000;
pub const MAX_SECS: i64 = 100_000_000; // timers.h
pub struct TicksConv { pub hz: i64 } // system_hz
impl TicksConv {
    pub fn ticks_from_timeval(&self, tv: &Timeval) -> Clock // 向上取整 + LONG_MAX→i64::MAX 钳位
    pub fn timeval_from_ticks(&self, ticks: Clock) -> Timeval
}
pub struct Timeval { pub tv_sec: i64, pub tv_usec: i64 } impl Timeval { pub fn is_sane(&self)->bool }
pub struct Itimerval { pub it_interval: Timeval, pub it_value: Timeval }
pub enum ItimerWhich { Real=0, Virtual=1, Prof=2 } + TryFrom<i32>
pub struct ItimerOp { pub set: Option<Itimerval>, pub get: bool }
pub trait VTimerCtl { fn vtimer(&mut self, ep: Endpoint, which: ItimerWhich, set: Option<Clock>, get: Option<&mut Clock>) -> i32; }
pub trait TimerCtl { fn set(&mut self, ep: Endpoint, ticks: Clock); fn cancel(&mut self, ep: Endpoint); fn exptime(&self, ep: Endpoint) -> Clock; }
pub trait ClockSource { fn getticks(&self) -> Clock; }
pub trait SigSender { fn send_sigalrm(&mut self, pid: Pid); }

pub fn do_itimer(table: &mut ProcTable, caller: UserSlot, which: i32, op: ItimerOp, hz: i64, vctl: &mut dyn VTimerCtl, tctl: &mut dyn TimerCtl, sigcpy: &mut dyn CopyCtl) -> Result<Option<Itimerval>, ItimerError>
pub fn getset_vTimer(table: &mut ProcTable, target: UserSlot, which: ItimerWhich, set: Option<Itimerval>, get: bool, conv: &TicksConv, vctl: &mut dyn VTimerCtl) -> Option<Itimerval>
pub fn check_vtimer(table: &mut ProcTable, proc_nr: usize, sig: i32, vctl: &mut dyn VTimerCtl)
pub fn get_realtimer(table: &ProcTable, target: UserSlot, conv: &TicksConv, now: Clock) -> Itimerval
pub fn set_realtimer(table: &mut ProcTable, target: UserSlot, val: &Itimerval, conv: &TicksConv, tctl: &mut dyn TimerCtl)
pub fn set_alarm(table: &mut ProcTable, target: UserSlot, ticks: Clock, tctl: &mut dyn TimerCtl)
pub fn cause_sigalrm(table: &mut ProcTable, ep: Endpoint, sig: &mut dyn SigSender) -> bool
pub fn handle_clock_notify(table: &mut ProcTable, now: Clock, tctl: &mut dyn TimerCtl, sig: &mut dyn SigSender) // expire_timers 驱动
```

- `ticks_from_timeval`：`checked_mul(hz, sec) → None→MAX` + `checked_mul(hz, usec) → (hz*usec+US-1)/US` 向上取整 + `checked_add` 钳位（`59/62`）。
- `do_itimer`：`which` 边界 `101` + `!set&&!get→EINVAL` `110` + `set→is_sane` `120-122` 双校验 + `REAL→get/set_realtimer` vs `VIRTUAL/PROF→getset_vtimer` 分派 `125-140` + `get→写旧` `148-150`（`CopyCtl` mock）。
- `getset_vtimer`：`optr/nptr → Option`（`174-193` 的 `newticks<=0→0` + `oldticks<=0→interval` 回绕 `212`）。
- `set_alarm`：`ticks>0→tctl.set` + `Some` vs `ticks<=0→tctl.cancel` + `None`（`304-310`，`ALARM_ON` 已收敛为 `Option`）。

### 4.3 `mproc/mproc.rs`：`ALARM_ON` 的唯一真源

```rust
// Before (ALARM_ON 位与 timer 双重):
// flags: RemainingFlags(ALARM_ON) + timer: Option<MinixTimer>
// After (D6 收敛):
// timer: Option<MinixTimer>  // Some → ALARM_ON, None → ~ALARM_ON
// flags: RemainingFlags = PARTIAL_EXEC|TAINTED  // ALARM_ON 已移除（或 deprecated 兼容）
```

`ProcessResources::alarm_on() -> bool { self.timer.is_some() }` 兼容 `alarm.c:254/331` 的 `ALARM_ON` 检查，写侧 `set_alarm` 的 `Some/None` 置位即 `ALARM_ON` 翻转。

### 4.4 `init.rs` 接线：`CLOCK notify → expire_timers`

```rust
// main.c:65-67  is_ipc_notify(CLOCK) → expire_timers → cause_sigalrm 回调
// timer.rs: handle_clock_notify(table, now=getticks(), tctl, sig)
//   遍历 ProcTable 的 Some(timer) 且 exptime <= now 的槽位，调用 cause_sigalrm
```

生产 `ClockSource::getticks()` + `TimerCtl::exptime` 注入，测试 `TestClockSource { now }` + `TestTimerCtl { timers }` 计数。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 | 检测 |
|---|--------|--------|-----------|------|
| 1 | `ticks` 向上取整 | `alarm.c:59` `+US-1` | `TicksConv::ticks_from` 的 `(hz*usec+US-1)/US` | `test_ticks_upward_rounds` |
| 2 | `LONG_MAX` 钳位 | `alarm.c:57/62` | `checked_mul→None→MAX` | `test_ticks_overflow_clamps` |
| 3 | `is_sane` 的 `MAX_SECS/US` | `alarm.c:85-86` | `Timeval::is_sane` | `test_is_sane_timeval` |
| 4 | `ALARM_ON` 唯一性 | `mproc.h:90` / `alarm.c:254/306` | `Option<MinixTimer>` | `test_alarm_on_is_option` |
| 5 | `newticks<=0→0` | `alarm.c:189/289` | `if newticks<=0 { interval=0 }` | `test_interval_zero_on_cancel` |
| 6 | `oldticks<=0→interval` 回绕 | `alarm.c:212/263` | `if oldticks<=0 { oldticks=interval }` | `test_interval_returned_when_expired` |
| 7 | `SIGALRM` 的 `ksig==FALSE` | `alarm.c:343` | `SigSender::send_sigalrm(pid)` 单播 | `test_cause_sigalrm_ksig_false` |

---

## 5 测试矩阵

> 基线：`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **242 passed / 0 failed**（原 228 + 本档新增 ~14：`timer.rs` 12 + `mproc/mproc.rs` 2）。结果见 `cargo test` 末段统计段（§2.4j 格式）。

### 5.1 `timer.rs`（定时器三族与变换）

- `test_ticks_upward_rounds`：`1us` 在 `100Hz` 下 `ticks=1`（`59` 向上取整）
- `test_ticks_overflow_clamps`：`sec=MAX_SECS+1` 或大 `sec` 钳位 `i64::MAX`（`57/62`）
- `test_timeval_roundtrip`：`ticks→timeval→ticks` 往返（`74-75` 分解无溢出）
- `test_is_sane_timeval`：`MAX_SECS/US` 边界（`85-86`，`usec==US→false`）
- `test_do_itimer_which_bound`：`which=-1/3→EINVAL`（`101`）
- `test_do_itimer_set_get_both_empty`：`!set&&!get→EINVAL`（`110`）
- `test_do_itimer_real_getset`：`REAL` 的 `get_realtimer/set_realtimer` 分派（`126-132`）
- `test_getset_vtimer_interval_zero_on_cancel`：`newticks<=0→0`（`188-189`）
- `test_getset_vtimer_returns_interval_when_expired`：`oldticks<=0→interval`（`212`）
- `test_check_vtimer_restarts_when_interval`：`interval>0→vtimer(&interval)`（`239-240`）
- `test_get_realtimer_alarm_on` / `test_get_realtimer_no_alarm`：`ALARM_ON` 分支（`254-265`）
- `test_set_realtimer_zero_clears_interval`：`ticks<=0→0`（`289`）
- `test_set_alarm_sets_and_clears`：`ticks>0→Some` vs `0→None`（`304-310`）
- `test_cause_sigalrm_three_guards`：`invalid→IN_USE→ALARM_ON` 三守卫（`323-331`）
- `test_cause_sigalrm_periodic_resets`：`interval>0→set_alarm` 周期（`337-338`）
- `test_handle_clock_notify_expires`：`CLOCK notify → expire → cause_sigalrm` 接线（`main.c:65-67`）

### 5.2 `mproc/mproc.rs`（`ALARM_ON` 唯一真源）

- `test_alarm_on_is_option`：`timer Some/None` 与 `ALARM_ON` 互为真值（`254/306`）
- `test_intervals_default_zero`：`intervals [0;3]` 默认零

### 5.3 `minix-types`（常量）

- `test_constants_match_c`：锁定 `ITIMER_REAL 0/VIRTUAL 1/PROF 2`（`sys/time.h`）、`VT_VIRTUAL 1`（`com.h:420`）、`SIGALRM 14`（`sys/signal.h`）、`MAX_SECS/US`

测试策略：`VTimerCtl`/`TimerCtl`/`SigSender`/`ClockSource` 均 `Test*` mock 可注入 `exptime/now` 与计数；`ticks` 变换纯函数脱离 `ProcTable` 独立测；`do_itimer` 的 `sys_datacopy` 双向经 `CopyCtl` mock 验 `set/get` 的 `is_sane` 拒绝与 `which` 边界。

---

## 6 过渡

本篇在 `CLOCK notify → expire_timers → cause_sigalrm → check_sig(SIGALRM)` 的 `REAL` 定时器位置，是 05 的 `VFS` 异步与 06 `EVENT` 串行之外唯一的 `CLOCK` 驱动；`VIRTUAL/PROF` 的 `sys_vtimer` 与 `check_vtimer` 为 11 的 `SIGVTALRM/PROF` 的内核侧来源：

```
04-ipc-dispatch.md（is_ipc_notify: CLOCK → expire_timers）
  │
  └─► 本章（REAL: set_alarm→expire→cause_sigalrm→check_sig(SIGALRM) / VIRTUAL: sys_vtimer→check_vtimer→sys_vtimer）
         │
         ├─► 11-signal-core.md（SIGALRM 的 check_sig 四态与 SIGVTALRM 的 check_vtimer 重启）
         ├─► 13-signal-flow.md（check_pending 的 pending&!mask 重检与 PROC_STOPPED 的 break，需本章 ALARM_ON 时的 SIGALRM 不丢失）
         └─► 15-credentials.md（do_set 的 TAINTED 与 setuid 的 VFS 转发，定时器不与其 uid 交互但共享 ProcTable）
```

`cause_sigalrm` 的 `interval>0?set:clear` 周期分支与 `set_alarm` 的 `Option` 唯一真源，使 `REAL` 的 `it_interval` 驱动周期（`itimer(2)` 的 `it_interval` 非零时周期）；`VIRTUAL` 的 `interval` 经 `check_vtimer` 驱动周期，二者皆“区间非零则重设，否则清”。

阅读顺序提示：若想先理解“内核计时到期如何发信号”，下一站 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md`（`set_timer/expire_timers` 的 `minix_timer_t` 队列与 `TMRDIFF_MAX`）；若想理解“信号到达后如何重检”，下一站 `13-signal-flow.md` 的 `check_pending` 与 `restart_sigs`。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/alarm.c` 全文（`33-65` `ticks_from_timeval` 等）、`minix3/minix/servers/pm/mproc.h:62-63`（`mp_timer/mp_interval[3]`）+ `mproc.h:90`（`ALARM_ON`）、`minix3/minix/servers/pm/main.c:65-67`（`CLOCK notify → expire_timers`）、`minix3/minix/include/sys/time.h:ITIMER_*`（`REAL/VIRTUAL/PROF`）、`minix3/minix/include/minix/com.h:420-421`（`VT_VIRTUAL/VT_PROF`）、`minix3/sys/sys/signal.h:14`（`SIGALRM 14`）
- PM 阶段文档：11-signal-core.md（`check_sig` 的 `SIGALRM` 投递与 `SIGVTALRM` 的 `check_vtimer` 重启）、02-mproc-struct.md（`MinixTimer/intervals/ALARM_ON` 三件套）、04-ipc-dispatch.md（`is_ipc_notify` 的 CLOCK 分发）、13-signal-flow.md（`check_pending` 的 `pending&!mask` 重检与 `PROC_STOPPED` 的 break）
- 内核接口：`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md`（`set_timer/expire_timers/tmr_exp_time/getticks/sys_vtimer` 的 `minix_timer_t` 队列与 `TMRDIFF_MAX`）、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/21-clock-device.md`（`CLOCK` 时钟源与 `system_hz`）
- 阶段内顺序：11/12/13 → **本章（14）** → 15（`TAINTED` 与 `credentials` 的信号边界）→ 16（`sched_stop` 的直毁，绕过本章的 `ALARM_ON`）
- OS 模式参考：Linux `setitimer`/`hrtimer` + `CLOCK_REALTIME/VIRTUAL`、`Redox TimeScheme` + `Timeout` 队列、`seL4` 周期 `Notification`（见 §1.7）
- Rust 实现：`os/servers/pm/src/mproc/mproc.rs`（`MinixTimer/intervals` 的 `Option` 唯一真源）、`os/servers/pm/src/timer.rs`（`TicksConv`/`ItimerWhich`/`do_itimer`/`set_alarm`/`cause_sigalrm`/`handle_clock_notify`）

