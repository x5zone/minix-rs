# 19 — 时间：`do_time/do_stime/do_gettime/do_getres/do_settime` 的 `boottime + clock/hz` 与双时钟

本文讲清时间如何在 PM 侧以“`CLOCK_REALTIME` 的 `boottime + realtime/hz` 与 `(realtime%hz)*1e9/hz` 无溢出分解 + `CLOCK_MONOTONIC` 的 `ticks` 单调 + `1e9/hz` 的 `HZ` 分辨率显式化 + `boottime = sec - realtime/hz` 的锚点重定 + `now` 的渐变 vs 跳变 + `clock_time` 的直通合成”为完整链路，使 `time(2)`/`stime(2)`/`clock_gettime(2)`/`clock_getres(2)`/`clock_settime(2)` 的五调用在 `SUPER_USER` 单门与 `MONOTONIC` 不可变不变量中可区分。

前置阅读：01-pm-init-main.md（`system_hz = sys_hz()` 的 `HertzProvider` 与 `monitor_params` 的 `hz` 来源）、04-ipc-dispatch.md（`ReplyIntent::Reply` 的同步回复与 `SUSPEND` 非本章）、02-mproc-struct.md（`Clock/Time=i64` 的 `A-11` 64 位与 `Credentials::is_superuser` 的 `eff==0` 单判据）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `ProcTable` 的 `system_hz` 显式参（01 的 `TicksConv { hz }`）、`CLOCK` 的 `is_ipc_notify` 非时间但 `expire_timers` 的 `ticks` 驱动（04）、`SUPER_USER 0` 的 `is_superuser` 一处谓词（15）的开发者；知道 `timespec { tv_sec, tv_nsec }` 与 `clockid_t` 的 `CLOCK_REALTIME 0/MONOTONIC 3` 定义（`sys/time.h:283/288`）。

> **本章不讲什么**：
> - 内核 `do_settime` 的 `now==0→adjtime_delta` 渐变与 `boottime<=sec` 的 `set_boottime` 分支（`kernel/system/do_settime.c:25-52` `set_realtime/set_boottime/adjtime_delta`）—— `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/21-clock-device.md`
> - 内核 `do_stime` 的 `set_boottime` 最终落点（`kernel/system/do_stime.c:17` `set_boottime(boot_time)`）—— 同上
> - `clock` 中断的 `timer_int_handler` 的 `uptime/realtime` 双增与 `adjtime_delta` 隔拍（`kernel/clock.c:timer_int_handler` `kclockinfo.uptime++` 与 `adjtime_delta` 的 `uptime&0x1` 隔拍）
> - `system_hz` 的 `DEFAULT_HZ` 初始化（`kernel/clock.c:init_clock` `env_get("hz") → kclockinfo.hz`）
> - VFS 时间戳（`02-stage-vfs/02-fproc-struct.md` 的 `va_ctime/mtime`）
>
> 本章只回答一个问题：**PM 如何为“`sec = boottime + clock/hz` 与 `nsec = (clock%hz)*1e9/hz` 的无溢出分解 + `REALTIME` 的 wall-clock 与 `MONOTONIC` 的 ticks 单调 + `1e9/hz` 的 `HZ` 分辨率 + `boottime = sec - realtime/hz` 的锚点重定 + `now` 的渐变 vs 跳变”的五时间调用建立 `CLOCK_REALTIME` 可设置而 `MONOTONIC` 不可设置的双时钟语义**。

### 1.1 为什么 PM 需要 `CLOCK_REALTIME` vs `MONOTONIC` 双时钟：wall-clock vs 单调滴答

`CLOCK_REALTIME 0` 与 `CLOCK_MONOTONIC 3`（`sys/time.h:283/288`，`CLOCK_MONOTONIC` 为 `3` 非 `1`——`1` 是 `ITIMER_VIRTUAL` 的 `VT_VIRTUAL`，不可混淆）的分野在 `time.c:31-36` 的 `switch(clk_id)` 一处显式：

- `CLOCK_REALTIME`（`case CLOCK_REALTIME: clock=realtime`，`32-33`）：**wall-clock**（真实时间），`realtime` 为 `kclockinfo.realtime` 的校正 `ticks`（`getuptime(&ticks,&realtime,&boottime)` 的第二参，经 `settime/adjtime` 可跳变/渐变）。`do_gettime(REALTIME)` 的 `sec=boottime+realtime/hz`（`42` `boottime` 为自 epoch 秒数 + `realtime/hz` 自 boot 秒数）与 `do_time` 的 `clock_time(&tv)` 同源（`time.c:99` `clock_time` 的 `boottime+realtime/hz` 合成，`libsys/clock_time.c:13-40`）。
- `CLOCK_MONOTONIC`（`case CLOCK_MONOTONIC: clock=ticks`，`35-36`）：**单调时钟**（`uptime`），`ticks` 为 `kclockinfo.uptime` 的单调滴答（自 boot 单增，`timer_int_handler` 的 `kclockinfo.uptime++` 每 `hz` 一拍）。`do_gettime(MONOTONIC)` 的 `sec=boottime+ticks/hz` 的 `boottime` 同为 epoch 锚点但 `ticks` 不经 `adjtime` 校正——`MONOTONIC` 的“不可设置”在 `time.c:84-86` 显式 `case CLOCK_MONOTONIC: /* monotonic cannot be changed */ default: return EINVAL`（`do_settime` 的 `MONOTONIC→EINVAL`）。

分野的必然：`REALTIME` 的 `sys_settime(now,REALTIME,sec,nsec)` 可 `set_boottime/set_realtime` 跳变（`kernel/system/do_settime.c:39-52`），`MONOTONIC` 的 `settime(3)` 恒 `EINVAL` 的不可变不变量是“单调滴答不可回拨”的语义保障（`clock_gettime(MONOTONIC)` 的返回值在 `realtime` 跳变时仍单增）。`do_getres` 的 `REALTIME/MONOTONIC→0,1e9/hz` 双分支与 `default→EINVAL`（`54-63`）同 `EINVAL` 边界——双时钟分辨率同 `HZ` 约束，`tv_sec` 恒 0 因 `1e9/hz < 1e9` 秒级不足（`58` 注释 *tv_sec is always 0 since system_hz is an int*）。

### 1.2 为什么 `sec = boottime + clock/hz` 与 `nsec = (clock%hz)*1e9/hz` 必须先取余再乘 `1e9`：无溢出分解

`time.c:42-44` 的分解是理解“`hz` 显式分解”的关键：

```c
mp->mp_reply.m_pm_lc_time.sec  = boottime + (clock / system_hz);           // 42  sec 的 boottime+clock/hz
mp->mp_reply.m_pm_lc_time.nsec = (uint32_t)((clock % system_hz) * 1000000000ULL / system_hz); // 44  nsec 的 %hz*1e9/hz
```

`realtime` 可达 `LONG_MAX`（`kclockinfo.realtime` 32 位 `0x7fffffff` 在 `hz=100` 时 `sec≈2147万`），`clock*1e9` 恒溢出（`0x7fffffff*1e9≈2e18 > 2^31`）。`clock%hz` 先取余（`0..hz-1`，`hz<=50000` 的 `init_clock` 上界，`kernel/clock.c:init_clock` 的 `HZ 2..50000`）再 `*1e9` 不溢出（`49999*1e9=5e13 < 9e18` 的 `i64::MAX` 64 位下恒不溢出，`A-11` 64 位扩展无需 `40000*25000` 分叉）。`boottime + clock/hz` 的 `sec` 合成为“epoch 锚点 + 自 boot 秒数”（`boottime` 为 `time_t` 的 epoch 秒数，`clock/hz` 为整数除法向零取整与 Rust `i64/hz` 同语义）。

与 `alarm.c:74-75` 的 `sec=ticks/hz` + `usec=(ticks%hz)*US/hz` 同型（`US=1e6` vs `NSEC=1e9` 仅常数不同，`alarm.c:75` 的 `ticks%hz*US/hz` 同样先取余再乘 `US`）。64 位下的统一：`TicksConv { hz }` 的 `timeval_from_ticks` 与 `time.rs:decompose_clock` 的 `timespec` 分解共享 `hz` 显式参，`CLOCK` 分辨率的 `1e9/hz` 与 `alarm` 的 `US` 分解同源但 `NSEC` 常量收敛到 `NSEC_PER_SEC 1_000_000_000`。

### 1.3 为什么 `do_getres` 的 `tv_sec=0, tv_nsec=1e9/hz` 是 `HZ` 分辨率的显式化

`time.c:58-60` 的分辨率是 `HZ` 的直接映射：

- `100Hz → 10ms`（`1e9/100=10_000_000ns`），`60Hz → 16.6ms`，`1000Hz → 1ms`，`50000Hz → 20us`——`tv_sec` 恒 0 因 `hz` 为 `int`（`58` 注释 *tv_sec is always 0 since system_hz is an int*，`1e9/hz < 1e9` 秒级分辨率不足，`tv_sec=0` 是 `HZ` 的“秒级不足”显式）。
- `1e9/hz` 的整数除法向零取整与 `timer.rs:TicksConv` 的 `HZ` 显式参同语义（`hz=100` 时 `1e9/100=10_000_000` 的 `nsec` 精度与 `clock_getres` 的 `CLOCK` 分辨率同位）。

`do_getres` 的 `REALTIME/MONOTONIC` 双分支 `58-60` 与 `default→EINVAL`（`63`）使双时钟分辨率同 `HZ` 约束——`MONOTONIC` 的分辨率亦 `1e9/hz`（`uptime` 的滴答周期同 `realtime` 的 `hz`），二者在 `getres` 层无分派差异仅 `gettime` 层 `REALTIME→realtime` vs `MONOTONIC→ticks` 分派。

### 1.4 为什么 `do_stime` 的 `boottime = sec - realtime/hz` 是“以当前 `realtime` 为锚点的 `boottime` 重定”

`time.c:124` 的锚点重定是理解 `stime` 语义的关键：

```c
boottime = m_in.m_lc_pm_time.sec - (realtime / system_hz); // 124  sec - realtime/hz
sys_stime(boottime);                                          // 126  set_boottime(boottime)
```

`sec` 为调用者传入的“新 `boottime+realtime/hz` 的目标秒数”（`m_in.m_lc_pm_time.sec` 的 `time_t`，`stime(2)` 的 `time_t *t`），减去 `realtime/hz` 的“已逝秒数”（`kclockinfo.realtime / system_hz` 的整数除法向零取整）得到新 `boottime`（自 epoch 秒数）。`realtime` 为 `getuptime` 的第二参（`getuptime(&uptime,&realtime,&boottime)` 的 `realtime` 校正 `ticks`，`libsys/getuptime.c:9-22` 的 `kclockinfo.realtime` 原子读）。`sys_stime(boottime)` 的 `set_boottime(boottime)`（`kernel/system/do_stime.c:17`）使下一次 `clock_time` 的 `boottime+realtime/hz` 合成即新 `sec`——`boottime` 的“锚点重定”与 `do_gettime` 的 `sec=boottime+clock/hz` 合成对偶（`do_stime` 的 `sec - realtime/hz → boottime` 与 `do_gettime` 的 `boottime+clock/hz → sec` 对偶）。

`do_stime` 的 `SUPER_USER` 门（`119-121` `mp_effuid != SUPER_USER→EPERM`）与 `do_settime:75-77` 同谓词（`is_superuser` 一处谓词，`A-12`），`getuptime` 的 `panic` 守卫（`122` `getuptime!=OK→panic`）与 `do_gettime:28` 同守卫——二者在 `stime` 的“以当前 `realtime` 为锚点”与 `gettime` 的“以 `boottime+clock/hz` 为合成”中共享 `getuptime` 三值。

### 1.5 为什么 `do_settime` 的 `now` 参区分“立即设置”与“`adjtime` 渐变”：跳变 vs 隔拍

`time.c:81-83` 的 `sys_settime(now,clk,sec,nsec)` 四参直通在 PM 侧仅透传，内核侧 `now==0` vs `now!=0` 在 `kernel/system/do_settime.c:29-52` 一处分叉：

- `now==0`（`29-35` `if now==0 → ticks=sec*hz+nsec/(1e9/hz); set_adjtime_delta(ticks)`）：**渐变**（`adjtime`），`ticks` 为增量（`sec*hz + nsec/(1e9/hz)` 的 `1e9/hz` 向零取整），`adjtime_delta` 的隔拍在 `kernel/clock.c:timer_int_handler` 的 `if(adjtime_delta!=0 && uptime&0x1)` 的 `uptime&0x1` 隔拍（偶 `ticks` 快进 `2`、奇 `ticks` 停滞，渐变速率 `0.5 tick/tick`）。
- `now!=0`（`39-52` `else → timediff=sec-boottime; timediff_ticks=timediff*hz; if(sec<=boottime||timediff_ticks<LONG_MIN/2||>LONG_MAX/2)→set_boottime(sec); else newclock=timediff_ticks+nsec/(1e9/hz)→set_realtime(newclock)`）：**跳变**（立即设置），`timediff_ticks` 的溢出守卫（`LONG_MIN/2`/`LONG_MAX/2`）后 `set_boottime` 或 `set_realtime` 的直接赋值。

PM 侧 `do_settime` 的 `now` 直通（`81` `m_in.m_lc_pm_time.now` → `sys_settime(now,...)`）使 `adjtime` 的渐变与 `settime` 的跳变在内核侧一处实现，PM 仅 `SUPER_USER` 门 + `MONOTONIC→EINVAL` 不可变守卫（`75-77` `EPERM` + `84-86` `MONOTONIC cannot be changed`）。

### 1.6 为什么 `do_time` 的 `clock_time(&tv)` 与 `do_gettime(CLOCK_REALTIME)` 在 `sec` 同值但 `nsec` 精度分叉

`time.c:99` 的 `clock_time(&tv)` 经 `libsys/clock_time.c:13-40` 的 `boottime+realtime/hz` 合成与 `do_gettime(REALTIME)` 的 `boottime+realtime/hz` 合成在 `sec` 层同算式：

- `clock_time.c:33-40` 的 `nsec` 分叉：`if(system_hz < LONG_MAX/40000)→ nsec=(realtime%hz)*40000/hz*25000`（`40000*25000=1e9` 的 `LONG_MAX/40000≈53686` 阈值，`hz=5e4` 时不取该分支而 `nsec=0` 退化） vs `time.c:44` 的 `nsec=(clock%hz)*1e9/hz` 的 `ULL` 直算——二者在 `hz<53686` 时 `nsec` 同值（`40000*25000=1e9` 的 `LONG_MAX` 溢出规避在 32 位下必要），64 位 `hz: i64` 的 `realtime%hz*1e9/hz` 恒不溢出（`49999*1e9=5e13<9e18`），`40000*25000` 分叉退化为 `*1e9` 直算（`A-11` 64 位扩展）。
- `do_time` 无 `SUPER_USER` 门（`time.c:94-104` 无 `effuid` 检查，`gettimeofday` 为只读），`do_gettime/settime` 有 `clk_id` 边界（`39/63/86` 的 `default→EINVAL`）——`do_time` 的只读与 `do_gettime` 的双时钟写读分派对偶。

### 1.7 与其他 OS 状态机的对照

Rust 改写不是照抄 `time.c:42-44` 的 `boottime+clock/hz` 分解，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `clock_gettime(CLOCK_REALTIME/CLOCK_MONOTONIC)` + `CLOCK_MONOTONIC` 不可设置。** Linux 的 `clock_gettime(CLOCK_REALTIME, &ts)` 经 `ktime_get_real_ts64` 的 `tk_core.timekeeper` 的 `xtime_sec + nsec` 合成（`kernel/time/timekeeping.c:ktimer`），`CLOCK_MONOTONIC` 经 `ktime_get_ts64` 的 `tk_core.monotonic` 单增（`kernel/time/time.c:clock_settime` 的 `CLOCK_MONOTONIC→EINVAL` 与 `time.c:84-86` `monotonic cannot be changed` 同谓词）。Linux 的 `CLOCK_REALTIME` 可 `clock_settime` 而 `MONOTONIC` 恒 `EINVAL`——Minix3 的 `sys_settime` 的 `clk_id != REALTIME→EINVAL`（`kernel/system/do_settime.c:25-26`）与此同谓词。Linux 的 `clock_getres` 返回 `1e9/HZ` 的 `tick_nsec` 与 `time.c:60` 的 `1e9/hz` 同算式（`HZ` 的 `CONFIG_HZ` 定值 vs Minix3 的 `system_hz` 动值）。Linux 的 `CONFIG_64BIT_TIME` 的 `time64_t` 在 `2038` 后以 `timespec64` 64 位（`A-11` 同扩展），Minix3 的 `time_t` 在 32 位下 `2038` 溢出在 `Rust` 以 `Time=i64` 64 位扩展收敛（`A-11`）。

**Redox `TimeScheme` + `Instant::now().duration_since(boot)`。** Redox 以 `kernel/time: TimeScheme` 的 `Instant::now()` 单调（`Instant` 的 `ticks` 单增，经 `arch/clock::monotonic` 的 `rdtsc` 或 `HPET`）与 `SystemTime::now()` 的 wall-clock（`wall = Instant::now().duration_since(boot) + boottime` 的 `boottime` 锚点）+ `common/src/time.rs` 的 `duration_to_ticks` 同 `ticks_from` 的向上取整，`clock_gettime(CLOCK_REALTIME/MONOTONIC)` 经 `ksyscall` 的 `TimeSpec { tv_sec: Time, tv_nsec: i64 }` 变换（`ns→ticks` 的 `duration_to_ticks` 同 `ticks_from_timeval` 的 `+US-1`）。Redox 的 `CLOCK` 以 `ksyscall` 的 `clock_gettime(CLOCK_MONOTONIC)` + `Instant` 解耦 `HZ`，PM 以 `system_hz` 显式参的 `decompose_clock(boottime, clock, hz)` 保留 `HZ` 显式（`arch/src/arch/clock.rs:ClockArch::new` 同型）。

**`seL4` 的 `seL4_BenchmarkGetTime` 的 `ticks` 单调。** `seL4` 无 `clock_gettime`，以 `seL4_BenchmarkGetTime` 的 `ticks` 单调（`kernel/src/arch/x86/kernel/boot.c` 的 `tsc` 或 `arm/kernel/boot.c` 的 `cntvct`）与 `seL4_BenchmarkResetLog` 的 `boottime` 重置，PM 的 `realtime` 的 `set_realtime` 跳变在 `seL4` 以 `BenchmarkResetLog` 重置替代 `settime`——无 `boottime` 锚点的“无 wall-clock 世界”与 Minix3 的 `boottime+realtime/hz` 有锚点对偶，Rust 以 `decompose_clock` 的 `boottime` 显式参收敛。

**结论（本章的设计基线）。** 把 C 的“`tick/realtime/boottime` 三值 + `boottime+clock/hz` 的 `CLOCK_REALTIME/MONOTONIC` 分派 + `clock%hz*1e9/hz` 的先取余再乘 `1e9` + `boottime=sec-realtime/hz` 锚点重定 + `now` 的渐变 vs 跳变直通 + `clock_time` 的 `40000*25000` 溢出分叉散落”改写为“`ClockId { Realtime=0, Monotonic=3 }` + `ClockSource::uptime→(ticks,realtime,boottime)` + `decompose_clock(boottime,clock,hz)` 的 `%hz*1e9/hz` 纯函数 + `clock_resolution(hz)` 的 `1e9/hz` 纯函数 + `BootTimeCtl::set_boottime` + `SetTimeCtl::set_time(now,clk,sec,nsec)` + `ClockTime::clock_time→Timespec`”——与 Linux `clock_gettime` 的 `REALTIME/MONOTONIC` 不可变对偶 + Redox `TimeScheme` 的 `Instant::now()` 单调同源，又因 PM 单线程无共享而以 `&mut ProcTable` 的 `is_superuser` 一处谓词 + `hz: Clock` 显式参的无溢出分解收敛。

### 1.8 小结

1. **为什么双时钟**——`REALTIME` 的 `boottime+realtime/hz` wall-clock 可 `settime` vs `MONOTONIC` 的 `ticks` 单调不可设置（`84-86` `EINVAL`）。
2. **为什么先取余再乘 `1e9`**——`realtime` 可达 `LONG_MAX`，`clock*1e9` 恒溢出；`clock%hz` 先取余（`0..hz-1`）再 `*1e9` 不溢出（`44` 的 `U64` 显式）。
3. **为什么 `tv_sec=0`**——`1e9/hz < 1e9` 秒级分辨率不足，`tv_sec` 恒 0 因 `hz` 为 `int`（`58` 注释）。
4. **为什么 `boottime=sec-realtime/hz`**——`sec` 为目标秒数，减 `realtime/hz` 已逝秒数得新 `boottime` 锚点（`124`）。
5. **为什么 `now` 分渐变 vs 跳变**——`now==0→adjtime_delta` 隔拍 vs `now!=0→set_boottime/set_realtime` 跳变（`kernel/do_settime.c:29-52`）。
6. **为什么 `clock_time` 与 `gettime(REALTIME)` 同值**——二者 `sec` 同 `boottime+realtime/hz` 合成，`nsec` 在 64 位下同 `realtime%hz*1e9/hz`（`40000*25000` 分叉退化）。

下一章逐行分析 C 的 `do_gettime/do_getres/do_settime/do_time/do_stime` 与 `getuptime/clock_time`；第 3 章给出 Rust 的 `ClockId/decompose_clock/clock_resolution/ClockSource/BootTimeCtl/SetTimeCtl/ClockTime`。

---

## 2 C 源码分析

### 2.1 `do_gettime`（`time.c:22-47`）

```c
int do_gettime(void)
{ // 22  CLOCK_GETTIME 的 PM 侧入口（REALTIME→realtime vs MONOTONIC→ticks）
  clock_t ticks, realtime, clock; time_t boottime; int s; // 24-26  temp ticks/realtime/boottime + s
  if ( (s=getuptime(&ticks, &realtime, &boottime)) != OK) // 28  getuptime 的 panic 守卫（libsys/getuptime.c:9-22）
  	panic("do_time couldn't get uptime: %d", s); // 29  三值原子读失败则 panic（同 122 的 stime 守卫）
  switch (m_in.m_lc_pm_time.clk_id) { // 31  m_lc_pm_time.clk_id（ipc.h:469 `clk_id: clockid_t`，0/3）
	case CLOCK_REALTIME: clock = realtime; break; // 32-33  REALTIME 0→realtime（wall-clock）
	case CLOCK_MONOTONIC: clock = ticks; break; // 35-36  MONOTONIC 3→ticks（单调 uptime）
	default: return EINVAL; // 39  invalid/unsupported clock_id → EINVAL（与 getres:63/settime:86 同谓词）
  }
  mp->mp_reply.m_pm_lc_time.sec = boottime + (clock / system_hz); // 42  sec = boottime + clock/hz（整数除法向零取整）
  mp->mp_reply.m_pm_lc_time.nsec = // 43
	(uint32_t) ((clock % system_hz) * 1000000000ULL / system_hz); // 44  nsec = (clock%hz)*1e9/hz（先取余再乘 U64 显式，避免 clock*1e9 溢出）
  return(OK); // 46  同步回复（04 的 ReplyIntent::Reply 直接回复，非 SUSPEND）
}
```

`28` 行 `getuptime` 的 `panic` 守卫与 `122` `do_stime` 同守卫（`getuptime!=OK→panic` 的 `time.c:28` 与 `122` 同谓词），`31-39` 的 `CLOCK_REALTIME/MONOTONIC` 分派使 `REALTIME` 可 `settime` 而 `MONOTONIC` 不可（`do_settime:84-86`），`42-44` 的 `%hz*1e9/hz` 先取余再乘 `U64` 显式与 `alarm.c:75` 的 `%hz*US/hz` 同型（`US=1e6` vs `1e9` 仅常数不同）。

### 2.2 `do_getres`（`time.c:53-65`）

```c
int do_getres(void)
{ // 53  CLOCK_GETRES 的 PM 侧入口（分辨率 1e9/hz）
  switch (m_in.m_lc_pm_time.clk_id) { // 55  m_lc_pm_time.clk_id（同 gettime:31）
	case CLOCK_REALTIME: // 56  REALTIME 0
	case CLOCK_MONOTONIC: // 57  MONOTONIC 3（双时钟分辨率同 HZ）
		/* tv_sec is always 0 since system_hz is an int */ // 58  tv_sec 恒 0 注释
		mp->mp_reply.m_pm_lc_time.sec = 0; // 59  sec=0（1e9/hz < 1e9 秒级不足）
		mp->mp_reply.m_pm_lc_time.nsec = 1000000000 / system_hz; // 60  nsec=1e9/hz（整数除法向零取整，hz=100→10ms，hz 隐式 HZ 分辨率）
		return(OK); // 61
	default: return EINVAL; // 63  invalid/unsupported → EINVAL（与 gettime:39/settime:86 同谓词）
  }
}
```

`58` 行注释 *tv_sec is always 0 since system_hz is an int* 使 `do_getres` 的 `sec` 恒 0 显式（`1e9/hz < 1e9` 秒级分辨率不足），`60` 行 `1e9/hz` 的 `HZ` 分辨率与 `04` 的 `system_hz=sys_hz()` 消费者（`01` 的 `TicksConv { hz }` 同 `hz` 显式参）。

### 2.3 `do_settime`（`time.c:71-88`）

```c
int do_settime(void)
{ // 71  CLOCK_SETTIME 的 PM 侧入口（SUPER_USER 门 + MONOTONIC 不可变）
  int s; // 73
  if (mp->mp_effuid != SUPER_USER) { // 75  effuid != 0 → EPERM（15 的 is_superuser 一处谓词，mproc.h:40-46  Credentials 三元但本章仅 eff 单判据）
      return(EPERM); // 76
  }
  switch (m_in.m_lc_pm_time.clk_id) { // 79  m_lc_pm_time.clk_id（同 gettime:31）
	case CLOCK_REALTIME: // 80  REALTIME 0
		s = sys_settime(m_in.m_lc_pm_time.now, m_in.m_lc_pm_time.clk_id, // 81  sys_settime 的 now（int 0→adjtime 渐变 vs !=0→settime 跳变，kernel/system/do_settime.c:29-52）
			m_in.m_lc_pm_time.sec, m_in.m_lc_pm_time.nsec); // 82  sec:time_t + nsec:long（0..1e9-1）
		return(s); // 83  OK 或 sys_settime 的 errno 透传（EINVAL 等）
	case CLOCK_MONOTONIC: /* monotonic cannot be changed */ // 84  MONOTONIC 3→不可设置注释
	default: return EINVAL; // 86  invalid/unsupported → EINVAL（与 gettime:39/getres:63 同谓词）
  }
}
```

`75-77` 的 `SUPER_USER` 门与 `do_stime:119-121` 同谓词（`eff!=0→EPERM` 一处谓词，`15` 的 `Credentials::is_superuser` 复用），`84-86` `monotonic cannot be changed` 注释使 `CLOCK_MONOTONIC` 不可设置不变量显式（`MONOTONIC→EINVAL` 在 `kernel/do_settime.c:25-26` `clk_id!=REALTIME→EINVAL` 对偶），`81-83` 的 `sys_settime(now,clk,sec,nsec)` 四参直通在 PM 侧仅透传（`now` 的渐变 vs 跳变在 `kernel/do_settime.c:29-52` 一处实现）。

### 2.4 `do_time`（`time.c:94-104`）

```c
int do_time(void)
{ // 94  GETTIMEOFDAY 的 PM 侧入口（clock_time 直通，time(2) 的 PM_FORK 旁）
/* Perform the time(tp) system call. */ // 96  注释：time(tp) 的旧调用（gettimeofday 的前身）
  struct timespec tv; // 97  timespec { tv_sec:time_t, tv_nsec:long }（sys/timespec.h）
  (void)clock_time(&tv); // 99  clock_time(&tv) 的 boottime+realtime/hz 合成（libsys/clock_time.c:13-40）
  mp->mp_reply.m_pm_lc_time.sec = tv.tv_sec; // 101  reply.sec = tv_sec（time_t）
  mp->mp_reply.m_pm_lc_time.nsec = tv.tv_nsec; // 102  reply.nsec = tv_nsec（long 0..1e9-1）
  return(OK); // 103  同步回复（04 的 Reply）
}
```

`99` 行 `clock_time(&tv)` 经 `libsys/clock_time.c:13-40` 的 `boottime+realtime/hz` 与 `do_gettime(REALTIME)` 的 `decompose_clock` 同源但 `40000*25000` 溢出分叉（`clock_time.c:33-40` `LONG_MAX/40000` 阈值在 64 位下退化为 `*1e9` 直算，`A-11`）。`time.c` 的 `do_time` 无 `SUPER_USER` 门（只读），`do_gettime/settime` 有 `clk_id` 边界——只读直通与双时钟写读分派对偶。

### 2.5 `do_stime`（`time.c:110-131`）

```c
int do_stime(void)
{ // 110  STIME 的 PM 侧入口（SUPER_USER 门 + boottime=sec-realtime/hz 锚点重定）
/* Perform the stime(tp) system call. Retrieve the system's uptime (ticks */ // 112  注释：Retrieve uptime 与 boottime 重定
  clock_t uptime, realtime; time_t boottime; int s; // 115-117  temp uptime/realtime/boottime + s
  if (mp->mp_effuid != SUPER_USER) { // 119  effuid != 0 → EPERM（同 settime:75-77）
      return(EPERM); // 120
  }
  if ( (s=getuptime(&uptime, &realtime, &boottime)) != OK) // 122  getuptime 的 panic 守卫（同 gettime:28）
      panic("do_stime couldn't get uptime: %d", s); // 123
  boottime = m_in.m_lc_pm_time.sec - (realtime/system_hz); // 124  boottime = sec - realtime/hz（锚点重定，sec 为 m_in.sec 的目标秒数，realtime/hz 为已逝秒数）
  s= sys_stime(boottime);		/* Tell kernel about boottime */ // 126  sys_stime(boottime) 的 set_boottime（libsys/sys_stime.c:9 `boot_time=boottime → _kernel_call(SYS_STIME)`）
  if (s != OK) panic("pm: sys_stime failed: %d", s); // 128  sys_stime!=OK→panic（双守卫，与 gettime:28/122 同型）
  return(OK); // 130  同步回复（04 的 Reply）
}
```

`124` 行 `boottime = sec - realtime/hz` 的锚点重定与 `do_gettime:42` 的 `sec=boottime+realtime/hz` 合成对偶（`stime` 的 `sec-realtime/hz→boottime` 与 `gettime` 的 `boottime+clock/hz→sec` 对偶），`122/128` 的 `panic` 双守卫（`getuptime!=OK→panic` + `sys_stime!=OK→panic`）与 `28` 的 `getuptime` 单守卫同型——`stime` 的双 `panic` 使 `boottime` 重定在内核侧一处实现（`kernel/system/do_stime.c:17` `set_boottime`）。

### 2.6 `getuptime`（`libsys/getuptime.c:9-22`）

```c
int getuptime(clock_t * uptime, clock_t * realtime, time_t * boottime)
{ // 9  kclockinfo 三值原子读（minix_kerninfo->kclockinfo 的 uptime/realtime/boottime）
	minix_kerninfo = get_minix_kerninfo(); // uptime 的 TODO: 64-bit support 注释（getuptime.c:9 注释 TODO: 64-bit support 的 32 位原子读假设）
	if (uptime != NULL) *uptime = minix_kerninfo->kclockinfo->uptime; // uptime 单调 ticks（timer_int_handler 的 uptime++）
	if (realtime != NULL) *realtime = minix_kerninfo->kclockinfo->realtime; // realtime 校正 ticks（adjtime_delta 的隔拍校正）
	if (boottime != NULL) *boottime = minix_kerninfo->kclockinfo->boottime; // boottime epoch 秒数（set_boottime/settime 的锚点）
	return OK; // OK 恒返回（panic 守卫在 PM 侧 time.c:28/122，getuptime 本身不返回错误）
}
```

`getuptime` 的 `uptime`（单调 `ticks`）vs `realtime`（`adjtime_delta` 校正 `ticks`）vs `boottime`（`epoch` 秒数）三值与 `do_gettime:42-44` 的 `boottime+clock/hz` 合成的 `clock=realtime` vs `clock=ticks` 分派对偶（`REALTIME→realtime` 的 wall-clock 与 `MONOTONIC→ticks` 的单调 tick 在 `getuptime` 三值一处读）。

### 2.7 `clock_time`（`libsys/clock_time.c:13-40`）

```c
time_t clock_time(struct timespec *tv)
{ // 13  time 的 PM 侧直通合成（boottime+realtime/hz 与 boottime+clock/hz 同源）
	boottime = kclockinfo.boottime; realtime = kclockinfo.realtime; system_hz = kclockinfo.hz; // realtime/boottime/hz 三值读（同 getuptime 的 kclockinfo 三值）
	sec = boottime + realtime / system_hz; // sec 的 boottime+realtime/hz 合成（同 do_gettime:42 的 REALTIME 分支）
	if (tv != NULL) { tv->tv_sec = sec; // tv_sec 的 boottime+realtime/hz
		if (system_hz < LONG_MAX / 40000) // 33  hz<LONG_MAX/40000≈53686 时 40000*25000=1e9 的 LONG_MAX 溢出规避
			tv->tv_nsec = (realtime % system_hz) * 40000 / system_hz * 25000; // 33-40  nsec 的 (realtime%hz)*40000/hz*25000（40000*25000=1e9 的分步乘法避免 realtime%hz*1e9 溢出 LONG_MAX）
		else tv->tv_nsec = 0; // 40  hz>=LIMIT 时 nsec=0 退化（hz=5e4 时不取该分支而 0）
	}
	return sec; // sec 的 return（tv_sec 与 return 同值）
}
```

`33-40` 的 `40000*25000=1e9` 分步乘法为 32 位下 `LONG_MAX 0x7fffffff` 的溢出规避（`realtime%hz<5e4` 时 `*40000=2e9<2^31`，`2e9/5e4=4e4`，`4e4*25000=1e9` 分步不溢出），64 位 `i64` 的 `realtime%hz*1e9` 恒不溢出（`5e4*1e9=5e13<9e18`），分叉退化为 `*1e9` 直算（`A-11`）。

### 2.8 内核 `do_settime` 的 `now` 分叉（`kernel/system/do_settime.c:25-52`）

`kernel/system/do_settime.c:25-52` 的 `now==0→adjtime_delta` 渐变（`29-35` `ticks=sec*hz+nsec/(1e9/hz)→set_adjtime_delta(ticks)`）vs `now!=0→timediff=sec-boottime → timediff*hz → set_boottime/set_realtime` 跳变（`39-52`），PM 侧仅 `sys_settime(now,clk,sec,nsec)` 四参直通（`81-83`）——PM 的职责是 `SUPER_USER` 门 + `MONOTONIC→EINVAL` 不可变守卫（`75-77`/`84-86`），内核的职责是 `adjtime` 隔拍与 `set_boottime/set_realtime` 落点（移交 `01-stage-kernel/21`）。

### 2.9 内核 `do_stime` 的 `set_boottime` 落点（`kernel/system/do_stime.c:17`）

`kernel/system/do_stime.c:17` 的 `set_boottime(boot_time)` 唯一（`m_lsys_krn_sys_stime.boot_time` 的 `time_t` 直存 `kclockinfo.boottime`），PM 侧 `do_stime:124` 的 `boottime=sec-realtime/hz` 已算得新 `boottime` 锚点——PM 的职责是锚点重定（`sec-realtime/hz` 的整数除法），内核的职责是 `kclockinfo.boottime` 的 `set_boottime` 存储（移交 `01-stage-kernel/21`）。

### 2.10 消息与类型（`sys/time.h:283/288` `CLOCK_REALTIME 0/MONOTONIC 3` + `ipc.h:469` `mess_lc_pm_time/mess_pm_lc_time` + `callnr.h:41/46-48` `PM_GETTIMEOFDAY 28/STIME 7/CLOCK_* 33-35` + `glo.h:25` `system_hz`）

- `CLOCK_REALTIME 0`（`sys/time.h:283`）/ `CLOCK_MONOTONIC 3`（`sys/time.h:288` `3` 非 `1`——`1` 是 `ITIMER_VIRTUAL` 的 `VT_VIRTUAL`）、`timespec { tv_sec:time_t, tv_nsec:long }`（`sys/timespec.h`）
- `mess_lc_pm_time { time_t sec; clockid_t clk_id; int now; long nsec; padding 36B; _ASSERT 56B }`（`ipc.h:469` `sec/clk_id/now/nsec` 四参，`time.c:81` 的 `now/clk/sec/nsec` 直通）
- `mess_pm_lc_time { time_t sec; long nsec; padding 44B; _ASSERT 56B }`（`ipc.h:469` `sec/nsec` 双参，`time.c:42/44/59/101` 的 `reply` 载荷）
- `PM_STIME 7`（`callnr.h:20` `PM_BASE+7`）/ `PM_GETTIMEOFDAY 28`（`callnr.h:41` `PM_BASE+28`）/ `PM_CLOCK_GETRES 33`（`callnr.h:46`）/ `PM_CLOCK_GETTIME 34`（`callnr.h:47`）/ `PM_CLOCK_SETTIME 35`（`callnr.h:48` `PM_BASE+33-35` 三族时钟号，`table.c:21/42/47-49` 的 `CALL(PM_*)=do_*` 分派）
- `system_hz u32_t`（`glo.h:25` `system_hz`，`main.c:238` `system_hz=sys_hz()` 的 `HertzProvider`，`kclockinfo.hz` 的 `env_get("hz")→kclockinfo.hz` 的 `init_clock` 初始化，`hz` 范围 `2..50000` 的 `kernel/clock.c:init_clock` 的 `HZ 2..50000` 守卫）

### 2.11 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `CLOCK_MONOTONIC` 不可设置 | `time.c:84-86` `MONOTONIC→EINVAL` | `do_settime(MONOTONIC)→EINVAL` | 可恢复 `EINVAL` |
| `SUPER_USER` 门 | `time.c:75-77/do_stime:119-121` `effuid!=0→EPERM` | `non-SUPER→EPERM` | 可恢复 `EPERM` |
| `boottime = sec - realtime/hz` | `time.c:124` `sec - realtime/hz` | `stime` 的锚点重定 | 不变量 |
| `sec = boottime + clock/hz` 与 `nsec = (clock%hz)*1e9/hz` | `time.c:42/44` | `gettime` 的无溢出分解 | 不变量 |
| `do_getres` 的 `1e9/hz` | `time.c:60` `1000000000/hz` | `CLOCK` 分辨率 `1e9/hz` | 不变量 |
| `clock_time` 的 `40000*25000` 分叉 | `clock_time.c:33-40` `LONG_MAX/40000` 阈值 | `hz<53686→40000*25000` 否则 `0` 在 64 位下退化为 `*1e9` | 不变量（溢出规避） |

---

## 3 Rust 设计决策

Rust 改写遵循“显式 `ClockId` + `decompose_clock/clock_resolution` 纯函数 + `ClockSource/BootTimeCtl/SetTimeCtl/ClockTime` 显式端口”的 8 决策，保留 C 的 `clock→boottime` 合成与 `boottime` 锚点重定，但以类型系统使 `CLOCK_REALTIME/MONOTONIC` 分派与 `MONOTONIC` 不可变显式化。以下决策对应设计契约 `.design/19-design.v1.md` 的 D1–D8。

### D1：`mess_lc_pm_time/mess_pm_lc_time` 收敛到 `ClockId` + `TimeRequest/Timespec`（ARCH A-11）

- **C**：`ipc.h:469` `mess_lc_pm_time { sec:time_t, clk_id:clockid_t, now:int, nsec:long }` + `mess_pm_lc_time { sec:time_t, nsec:long }` 的裸 `int` 四参。
- **Rust**：`enum ClockId { Realtime=0, Monotonic=3 }` + `TryFrom<i32>`（`0→Realtime` + `3→Monotonic` else `Err(InvalidClock)` → `EINVAL`，`time.c:39/63/86` 的 `default→EINVAL` 在 `TryFrom` 一处），`struct TimeRequest { clk: ClockId, now: bool, sec: Time, nsec: i64 }`（`now: int` 在 Rust 以 `bool`），`struct Timespec { sec: Time, nsec: i64 }`（`sys/timespec.h:timespec` 的 `Time/i64` 物化，`nsec` 约束 `0..1e9`）。
- **为什么**：`C` 的 `clockid_t` 裸 `int` 的 `0/3` 分散在 `time.c:31-39/55-63/79-86` 三处 `switch`，`TryFrom` 一处使 `default→EINVAL` 在类型层穷尽（`default→EINVAL` 的 `39/63/86` 三处在 `TryFrom` 一处，`monotonic cannot be changed` 不变量在 `ClockId::Monotonic→Err(Inval)` 显式）。

### D2：`do_gettime` 的 `getuptime` 三值收敛到 `ClockSource` trait（ARCH A-11）

- **C**：`time.c:28` `getuptime(&ticks,&realtime,&boottime)` 的 `panic` 守卫 + 三值原子读。
- **Rust**：`trait ClockSource { fn uptime(&self) -> Result<(Clock, Clock, Time), TimeError> }`（`getuptime` 三值原子读显式参，`Ok((ticks,realtime,boottime))` 对应 `getuptime==OK`，`Err→panic` 的 `time.c:29` 在 Rust 以 `Result::unwrap_or_else(panic)` 对偶，`A-11` 64 位 `Clock=i64, Time=i64`，`hz: Clock` 为 `TicksConv { hz }` 的显式参）。
- **为什么**：`C` 的 `getuptime` 的全局 `kclockinfo` 读在 Rust 以 `ClockSource` 注入（`timer.rs:ClockSource::getticks` 同 `A-11`），`system_hz` 全局在 Rust 以 `hz: Clock` 显式参（`01` 的 `system_hz=sys_hz()` 已显式，`timer.rs:TicksConv { hz }` 已 `hz: Clock`）。

### D3：`sec = boottime + clock/hz` 与 `nsec = (clock%hz)*1e9/hz` 收敛到 `decompose_clock` 纯函数（ARCH A-11）

- **C**：`time.c:42-44` `sec=boottime+clock/hz` + `nsec=(clock%hz)*1e9/hz` 的 `U64` 显式。
- **Rust**：`const NSEC_PER_SEC: i64 = 1_000_000_000; fn decompose_clock(boottime: Time, clock: Clock, hz: Clock) -> Timespec { Timespec { sec: boottime + clock / hz, nsec: (clock % hz) * NSEC_PER_SEC / hz } }` 纯函数（`clock%hz` 先取余再 `*1e9` 不溢出，`clock<=i64::MAX` 时 `clock%hz<50000`，`5e4*1e9=5e13<9e18`，`A-11`）。
- **为什么**：`C` 的 `clock_time` 的 `40000*25000` 分叉在 64 位下 `*1e9` 无需分叉，抽为纯函数使 `do_gettime` 的 `boottime+clock/hz` 合成可脱离 `ProcTable` 独立测（与 `alarm.c:74-75` 的 `ticks%hz*US/hz` 同型，共享 `hz` 显式参）。

### D4：`do_getres` 的 `1e9/hz` 收敛到 `clock_resolution` 纯函数（ARCH A-7）

- **C**：`time.c:60` `nsec=1e9/hz`。
- **Rust**：`fn clock_resolution(hz: Clock) -> Timespec { if hz==0 { Timespec { sec:0, nsec:0 } } else { Timespec { sec:0, nsec: NSEC_PER_SEC / hz } } }` 纯函数（`tv_sec` 恒 0 因 `1e9/hz < 1e9`，`hz==0→0` 守卫与 `TicksConv` 同守卫）。
- **为什么**：`clock_resolution` 纯函数使 `1e9/hz` 在 `hz=100→10ms`、`hz=1000→1ms` 的分辨率可单元测，`REALTIME/MONOTONIC→0,1e9/hz` 双分支与 `default→EINVAL` 在 `TryFrom` 一处（`D1`）。

### D5：`do_stime` 的 `SUPER_USER` 门 + `boottime = sec - realtime/hz` 收敛到 `BootTimeCtl` trait（ARCH A-3）

- **C**：`time.c:119-128` `SUPER_USER` 门 + `getuptime` + `boottime=sec-realtime/hz` + `sys_stime(boottime)`。
- **Rust**：`trait BootTimeCtl { fn set_boottime(&mut self, boottime: Time) -> i32 }` + `fn do_stime(table: &ProcTable, caller: UserSlot, sec: Time, hz: Clock, src: &dyn ClockSource, ctl: &mut dyn BootTimeCtl) -> Result<(), TimeError>`（`is_superuser(caller)` 一处谓词，`boottime=sec-realtime/hz` 的整数除法向零取整）。
- **为什么**：`SUPER_USER` 门在 Rust 以 `is_superuser` 一处谓词（`15` 的 `Credentials::is_superuser` 复用），`getuptime` 的 `panic` 与 `sys_stime` 的 `panic` 在 Rust 以 `Result::unwrap_or_else(panic)` 对偶。

### D6：`do_settime` 的 `SUPER_USER` 门 + `REALTIME→sys_settime` vs `MONOTONIC→EINVAL` 收敛到 `SetTimeCtl` trait（ARCH A-3）

- **C**：`time.c:75-88` `SUPER_USER` 门 + `MONOTONIC→EINVAL` 不可变 + `sys_settime` 四参直通。
- **Rust**：`trait SetTimeCtl { fn set_time(&mut self, now: bool, clk: ClockId, sec: Time, nsec: i64) -> i32 }` + `fn do_settime(table: &ProcTable, caller: UserSlot, req: TimeRequest, ctl: &mut dyn SetTimeCtl) -> Result<(), TimeError>`（`Monotonic→Err(Inval)` 在 `ClockId` 枚举层显式）。
- **为什么**：`CLOCK_MONOTONIC` 不可设置在 `ClockId::Monotonic→Err(Inval)` 一处显式，`sys_settime` 四参直通在 `SetTimeCtl` 抽象（`timer.rs:VTimerCtl` 同型）。

### D7：`do_time` 的 `clock_time(&tv) → Timespec` 收敛到 `ClockTime` trait（ARCH A-11）

- **C**：`time.c:99` `clock_time(&tv) → Timespec`。
- **Rust**：`trait ClockTime { fn clock_time(&self) -> Timespec }` + `fn do_time(clock: &dyn ClockTime) -> Timespec` 纯函数（`40000*25000` 分叉在 64 位下退化为 `*1e9` 直算）。
- **为什么**：`C` 的 `40000*25000` 分叉在 64 位下 `realtime%hz*1e9/hz` 恒不溢出，抽为 `ClockTime` 使 `do_time` 脱离 `ProcTable` 独立测。

### D8：常量收敛到 `minix-types`（单一真相）

- **C**：`CLOCK_REALTIME 0/MONOTONIC 3` + `PM_GETTIMEOFDAY 28/STIME 7/CLOCK_* 33-35` + `SYS_STIME 39/SETTIME 40` + `NSEC_PER_SEC 1e9`。
- **Rust**：`minix-types: CLOCK_REALTIME 0` 等、`PM_GETTIMEOFDAY 28` 等、`SYS_STIME 39` 等、`NSEC_PER_SEC 1_000_000_000` 单一真相（`time.c:44` `1000000000ULL` 数值锁定，测试 `test_constants_match_c`）。
- **为什么**：`system_hz` 不存 `minix-types` 全局（`01` 的 `system_hz=sys_hz()` 为显式参，`timer.rs:TicksConv { hz }` 已 `hz: Clock`，本章复用 `hz: Clock` 直参）。

### ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-11 64位 | `Clock=i64, Time=i64, ClockId::TryFrom, decompose_clock` 的 `%hz*1e9/hz`（D1/D3） | `minix-types:clock.rs` + 本文档 §3.1/3.3 + 计划 §4 |
| A-3 全局→显式 | `ClockSource/BootTimeCtl/SetTimeCtl/ClockTime` 显式 `caller: UserSlot` + `hz: Clock`（D2/D5/D6） | `time.rs` 注释 + 本文档 §3.2/3.5/3.6 + 计划 §4 |
| A-7 分辨率抽象 | `clock_resolution(hz)` 的 `1e9/hz` 纯函数（D4） | `time.rs` 注释 + 本文档 §3.4 + 计划 §4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── time.rs              — ClockId + Timespec/TimeRequest + NSEC_PER_SEC + decompose_clock/clock_resolution + ClockSource/BootTimeCtl/SetTimeCtl/ClockTime trait + do_time/do_stime/do_gettime/do_getres/do_settime + handle_time_call 分派
├── mproc/
│   └── mproc.rs         — （无新增，ProcessResources 已含 credentials 的 is_superuser）
└── ipc/
    └── mod.rs           — （无新增，time 的 sys 调用经 time.rs 的 trait 注入；pm.rs 的 PmRequest 若已含 time 变体则复用 TimeRequest）
```

### 4.2 `time.rs`：时钟分解与五调用

```rust
pub const NSEC_PER_SEC: i64 = 1_000_000_000;
pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 3;
pub const PM_GETTIMEOFDAY: i32 = 28;
pub const PM_STIME: i32 = 7;
pub const PM_CLOCK_GETRES: i32 = 33;
pub const PM_CLOCK_GETTIME: i32 = 34;
pub const PM_CLOCK_SETTIME: i32 = 35;

pub type Clock = i64;
pub type Time = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timespec { pub sec: Time, pub nsec: i64 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockId { Realtime=0, Monotonic=3 } + TryFrom<i32>

pub struct TimeRequest { pub clk: ClockId, pub now: bool, pub sec: Time, pub nsec: i64 }

pub trait ClockSource { fn uptime(&self) -> Result<(Clock, Clock, Time), TimeError>; }
pub trait BootTimeCtl { fn set_boottime(&mut self, boottime: Time) -> i32; }
pub trait SetTimeCtl { fn set_time(&mut self, now: bool, clk: ClockId, sec: Time, nsec: i64) -> i32; }
pub trait ClockTime { fn clock_time(&self) -> Timespec; }

pub fn decompose_clock(boottime: Time, clock: Clock, hz: Clock) -> Timespec // sec=boottime+clock/hz, nsec=(clock%hz)*1e9/hz
pub fn clock_resolution(hz: Clock) -> Timespec // sec=0, nsec=1e9/hz

pub fn do_time(clock: &dyn ClockTime) -> Timespec
pub fn do_stime(table: &ProcTable, caller: UserSlot, sec: Time, hz: Clock, src: &dyn ClockSource, ctl: &mut dyn BootTimeCtl) -> Result<(), TimeError>
pub fn do_gettime(src: &dyn ClockSource, clk: ClockId, hz: Clock) -> Result<Timespec, TimeError>
pub fn do_getres(clk: ClockId, hz: Clock) -> Result<Timespec, TimeError>
pub fn do_settime(table: &ProcTable, caller: UserSlot, req: TimeRequest, ctl: &mut dyn SetTimeCtl) -> Result<(), TimeError>
```

- `decompose_clock`：`sec=boottime+clock/hz` + `nsec=(clock%hz)*NSEC_PER_SEC/hz` 的先取余再乘 `1e9`（`44` 的 `%hz*1e9/hz`，`clock%hz<50000` 时 `*1e9=5e13<9e18`）。
- `do_gettime`：`uptime()→(ticks,realtime,boottime)` + `match clk { Realtime→realtime, Monotonic→ticks }` + `decompose_clock(boottime, clock, hz)`（`31-44`）。
- `do_stime`：`!is_superuser→Err(Perm)` + `uptime()→(_,realtime,_)` + `boottime=sec-realtime/hz` + `ctl.set_boottime(boottime)`（`119-128`，双 `panic` 在 Rust 以 `unwrap_or_else(panic)` 对偶但本档返回 `Result` 后由调用层决定是否 `panic`）。
- `do_settime`：`!is_superuser→Err(Perm)` + `clk==Monotonic→Err(Inval)` + `ctl.set_time(now,clk,sec,nsec)`（`75-88`）。
- `do_time`：`clock.clock_time()` 直通（`99`）。

### 4.3 `os/libs/minix-types/src/types/clock.rs`：64 位扩展

已存 `Clock=i64, Time=i64`（`A-11` 64 位扩展，`Time` 为 `time_t` 的 64 位物化，`2038` 问题收敛），本章复用 `Clock/Time=i64` 直参（`decompose_clock` 的 `boottime: Time` + `clock: Clock`）。

### 4.4 `os/servers/pm/src/mproc/mproc.rs` 与 `os/servers/pm/src/mproc/credentials.rs`

`is_superuser(&self)->bool { user.effective==0 }`（`credentials.rs: Credentials::is_superuser`）为 `do_settime/do_stime` 的 `SUPER_USER` 门一处谓词（`75-77/119-121` 的 `eff!=0→EPERM` 5 处收敛），本章仅 `eff` 单判据（`real/saved` 不参与时间门）。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 | 检测 |
|---|--------|--------|-----------|------|
| 1 | `CLOCK_MONOTONIC` 不可设置 | `time.c:84-86` `MONOTONIC→EINVAL` | `ClockId::Monotonic→Err(Inval)` | `test_do_settime_monotonic_inval` |
| 2 | `SUPER_USER` 门 | `time.c:75-77/119-121` `eff!=0→EPERM` | `!is_superuser→Err(Perm)` | `test_do_settime_perm` / `test_do_stime_perm` |
| 3 | `boottime = sec - realtime/hz` | `time.c:124` | `boottime=sec-realtime/hz` | `test_do_stime_boottime` |
| 4 | `sec=boottime+clock/hz, nsec=(clock%hz)*1e9/hz` | `time.c:42/44` | `decompose_clock` | `test_decompose_clock` |
| 5 | `do_getres` 的 `1e9/hz` | `time.c:60` | `clock_resolution(hz)` | `test_do_getres_resolution` |
| 6 | `clock_time` 直通 | `time.c:99` | `ClockTime::clock_time` | `test_do_time_clock_time` |

---

## 5 测试矩阵

> 基线：`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **约 275 passed / 0 failed**（原 260 + 本档新增 ~15：`time.rs` 13 + `minix-types` 2）。结果见 `cargo test` 末段统计段（§2.4j 格式）。

### 5.1 `time.rs`（双时钟分解与五调用）

- `test_decompose_clock`：`boottime=1000, clock=150, hz=100 → sec=1001, nsec=500_000_000`（`42-44` 的 `boottime+clock/hz` 与 `clock%hz*1e9/hz` 无溢出，`49999*1e9` 不溢出）
- `test_clock_resolution`：`hz=100→10ms` + `hz=1000→1ms` + `hz=0→0`（`60` 的 `1e9/hz`，`58` 的 `tv_sec=0`）
- `test_clock_id_try_from`：`0→Realtime` + `3→Monotonic` + `1/2/99→EINVAL`（`31/55/79` 的 `default→EINVAL`，`CLOCK_MONOTONIC 3` 非 `1`）
- `test_do_gettime_realtime_vs_monotonic`：`REALTIME→realtime` vs `MONOTONIC→ticks` 分派（`32-36`）+ `ticks=150, realtime=250, boottime=1000, hz=100 → REALTIME sec=1002, MONOTONIC sec=1001`（`42-44` 的 `clock/hz` 分派差异）
- `test_do_gettime_invalid_clock`：`clk_id=1→EINVAL`（`39` `default→EINVAL`）
- `test_do_getres_realtime_monotonic`：`REALTIME→0,1e9/hz` + `MONOTONIC→0,1e9/hz`（`56-60` 双时钟同分辨率）
- `test_do_getres_invalid_clock`：`clk_id=99→EINVAL`（`63`）
- `test_do_settime_perm`：`eff!=0→EPERM` + `eff==0→set_time` 直通（`75-77` `SUPER_USER` 门）
- `test_do_settime_monotonic_inval`：`MONOTONIC→EINVAL` 不可变（`84-86` `monotonic cannot be changed`）
- `test_do_stime_perm`：`eff!=0→EPERM`（`119-121`）
- `test_do_stime_boottime`：`sec=2000, realtime=500, hz=100 → boottime=1995`（`124` `sec-realtime/hz`）
- `test_do_time_clock_time`：`clock_time→Timespec { sec, nsec }` 直通（`99-103`，`clock_time` 的 `boottime+realtime/hz` 与 `decompose_clock` 同源）
- `test_do_settime_now_passthrough`：`now=false→false` 与 `now=true→true` 的 `adjtime` 渐变 vs 跳变透传（`81` `now` 四参）

### 5.2 `minix-types`（常量）

- `test_constants_match_c`：锁定 `CLOCK_REALTIME 0/MONOTONIC 3`（`sys/time.h:283/288`）、`PM_GETTIMEOFDAY 28/STIME 7/CLOCK_* 33-35`（`callnr.h:41/46-48`）、`NSEC_PER_SEC 1e9`（`time.c:44` `1000000000ULL`）
- `test_time_types_64bit`：`Clock/Time=i64` 的 `A-11` 64 位扩展（`2038` 不溢出）

测试策略：`ClockSource/BootTimeCtl/SetTimeCtl/ClockTime` 均 `Test*` mock 可注入 `ticks/realtime/boottime/hz` 与计数；`decompose_clock` 纯函数脱离 `ProcTable` 独立测；`do_gettime` 的 `getuptime` 失败在 Rust 以 `Result::Err→TimeError::NoUptime` 对偶（C 的 `panic` 在测试 mock 不触发）；`do_settime` 的 `now` 四参在 `TestSetTimeCtl { last_now }` 计数验“渐变 vs 跳变”透传。

---

## 6 过渡

本篇在 `CLOCK_REALTIME→realtime` 与 `MONOTONIC→ticks` 的 `boottime+clock/hz` 合成位置，是 14 的 `REAL` 定时器 `CLOCK notify → cause_sigalrm` 的 wall-clock 消费者与 `01` 的 `system_hz=sys_hz()` 的 `hz` 显式参消费方；`MONOTONIC` 的单调性为 14 的 `deadline` 语义对照（`14` 的 `ticks` 定时器无 `boottime`，本章的 `REAL` 有 `boottime`）：

```
14-itimer.md（REAL: set_alarm→expire→cause_sigalrm→check_sig(SIGALRM) 的 interval 周期，hz 显式）
  │
  └─► 本章（REALTIME: decompose_clock(boottime, realtime, hz) 的 wall-clock 合成 / MONOTONIC: decompose_clock(boottime, ticks, hz) 的单调，hz 显式 + boottime 锚点）
         │
         ├─► 15-credentials.md（do_set 的 SUPER_USER 门与本章 do_settime/do_stime 的 SUPER_USER 同源，但 15 的 TAINTED 不经本章）
         └─► 20-misc-queries.md（do_getsysinfo 的 SI_* 与本章 clock 无直接交互，但 SI_PROC_TAB 的 do_getsysinfo 的 boottime 来自本章 getuptime 的 boottime 同 kclockinfo.boottime）
```

`CLOCK_MONOTONIC` 的“不可设置”与 `REALTIME` 的“可设置”对偶，使 `date -s` 的 `clock_settime(REALTIME)` 可改 `boottime/realtime` 而 `CLOCK_MONOTONIC` 的 `uptime` 始终单增——`Monotonic` 的 `EINVAL` 不变式在 `ClockId::Monotonic→Err(Inval)` 一处显式。

阅读顺序提示：若想先理解“内核侧 `settime` 如何改 `boottime/realtime`”，下一站 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/21-clock-device.md`（`kernel/system/do_settime.c:29-52` 的 `adjtime_delta/set_boottime/set_realtime`）；若想理解“定时器如何用 `ticks` 定周期”，下一站 `14-itimer.md` 的 `set_alarm(ticks)` 的 `ticks` 周期。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/time.c` 全文（`22-47` `do_gettime` + `53-65` `do_getres` + `71-88` `do_settime` + `94-104` `do_time` + `110-131` `do_stime`）、`minix3/minix/include/minix/ipc.h:469`（`mess_lc_pm_time/mess_pm_lc_time` 的 `sec/clk_id/now/nsec` 四参 + `sec/nsec` 双参）、`minix3/minix/include/minix/callnr.h:41/46-48`（`PM_GETTIMEOFDAY 28/STIME 7/CLOCK_GETRES 33/CLOCK_GETTIME 34/CLOCK_SETTIME 35`）、`minix3/sys/sys/time.h:283/288`（`CLOCK_REALTIME 0/MONOTONIC 3`）、`minix3/minix/lib/libsys/getuptime.c:9-22`（`kclockinfo三值`）、`minix3/minix/lib/libsys/clock_time.c:13-40`（`boottime+realtime/hz` 的 `40000*25000` 分叉）
- PM 阶段文档：01-pm-init-main.md（`system_hz=sys_hz()` 的 `HertzProvider`）、04-ipc-dispatch.md（`ReplyIntent::Reply` 的同步回复）、14-itimer.md（`TicksConv { hz }` 的 `%hz*US/hz` 与 `decompose_clock` 同型）、02-mproc-struct.md（`Clock/Time=i64` 的 `A-11` 64 位）、15-credentials.md（`SUPER_USER` 门与 `TAINTED` 不经本章）
- 内核接口：`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/21-clock-device.md`（`kclockinfo.{uptime,realtime,boottime}` 的三值与 `do_settime` 的 `adjtime_delta` 渐变）、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md`（`kclockinfo.hz/system_hz` 的 `DEFAULT_HZ`）
- 阶段内顺序：01/04 → **本章（19）** → 20（`do_sysuname/do_getsysinfo` 的 `SI_*` 不经 `clock`，但 `SI_PROC_TAB` 的 `do_getsysinfo` 的 `boottime` 来自本章 `getuptime` 同源）→ 14（`ticks` 定时器无 `boottime`，本章 `REAL` 有 `boottime` 的对照）
- OS 模式参考：Linux `clock_gettime` 的 `CLOCK_REALTIME/MONOTONIC` + `clock_settime` 的 `MONOTONIC→EINVAL`（`kernel/time/time.c`）、Redox `TimeScheme` + `Instant::now()`（`kernel/time/time.rs`）、`seL4` `BenchmarkGetTime` 的 `ticks` 单调（见 §1.7）
- Rust 实现：`os/servers/pm/src/time.rs`（`ClockId/Timespec/TimeRequest + decompose_clock/clock_resolution + ClockSource/BootTimeCtl/SetTimeCtl/ClockTime + do_time/do_stime/do_gettime/do_getres/do_settime` 的五调用）、`os/libs/minix-types/src/types/clock.rs`（`Clock=i64, Time=i64` 的 `A-11` 64 位）、`os/libs/minix-types/src/ipc/pm.rs`（`CLOCK_REALTIME/MONOTONIC` 的 `ClockId` 枚举与 `Timespec` 类型）
