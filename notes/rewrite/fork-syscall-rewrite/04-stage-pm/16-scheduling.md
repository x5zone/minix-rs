# 16 — 调度：`sched_init`/`sched_start_user`/`sched_nice` 的用户态交接与 `nice↔queue` 双射

本文讲清调度如何在 PM 侧以“`sched_init` 的 `INIT` 接管 + `sched_start_user` 的 `PRIV_PROC` 父继承 `INIT` + `sched_nice` 的 `KERNEL/NONE→EINVAL` 守卫 + `nice→queue` 与 `queue→nice` 的 `16:41` 线性缩放 + `do_getsetpriority` 的 `PRIO_PROCESS` 唯一支持与 `SUPER_USER` 三重及 `EACCES` 提优先限制”为完整链路，使 `nice` 的 `-20..20` 与内核队列 `0..15` 的优先级在用户态 `SCHED` 服务的三元 `maxprio/quantum/cpu` 上可区分，且 `USER_Q→0` 零点在 `sched_init` 与 `fill_boot_procs` 之间对齐。

前置阅读：01-pm-init-main.md（`sef_cb_init_fresh:241` `sched_init()` 调用点与 `USER_Q`/`USR_Q` `7→0` 零点、`PmServer::init` 末步时序）、04-ipc-dispatch.md（`call_vec` 的 `PM_GETPRIORITY/SETPRIORITY` 分发与 `ReplyIntent`）、02-mproc-struct.md（`ProcessResources { nice, scheduler }` 二元 + `MinixTimer` 的 `ALARM_ON` 互斥外）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `ProcTable` 的 `IN_USE && !PRIV_PROC` 扫描（01）、`call_vec` 的 `PM_GETPRIORITY/SETPRIORITY` 分发（04）、`mproc` 的 `nice/scheduler` 二元（02）的开发者；知道 `PRIO_MIN -20..PRIO_MAX 20` 与 `MAX_USER_Q 0..MIN_USER_Q 15` 的值域。

> **本章不讲什么**：
> - 内核调度器就绪队列与 `do_schedule`（`kernel/proc.c:schedule` 的 `pick_proc`）—— `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/11-scheduling-primitives.md`
> - SCHED 服务实现（`sched_start/sched_inherit/sched_stop` 的队列管理、`sched_set_nice` 的 `maxprio` 传播）—— `06-stage-sched`（`servers/sched`）
> - VFS 侧 `VFS_PM_FORK` 的 `NEW_PARENT` 语义（`05` 已覆盖 `NEW_PARENT` 的 `reply_to_new_parent`）
>
> 本章只回答一个问题：**PM 如何为“进程的 `nice` 值与内核队列的优先级互转、用户态调度器的 `maxprio/quantum/cpu` 三元交接、以及 `get/setpriority` 的权限与 `EACCES` 提升限制”建立 `nice↔queue` 的 `16:41` 双射与 `PRIV_PROC` 父继承 `INIT` 的特例**。

### 1.1 为什么 PM 需要用户态调度交接：内核只提供原语，策略在 `SCHED`

Minix3 的内核只提供 `SCHEDULING` 原语（`_taskcall` 到 `SCHED` 服务的 `sched_start/sched_inherit/sched_stop/sched_set_nice`），策略（`maxprio`、`quantum`、`cpu` 亲和）在用户态 `SCHED` 服务（`servers/sched`）中可配置（`minix/sched.h:7-12`）。PM 的 `sched_init` 使 `INIT` 从 `KERNEL` 调度（`main.c:199` `mp_scheduler = KERNEL`）切换到 `SCHED`（`schedule.c:37-43` 的 `sched_start(SCHED, INIT, INIT, USER_Q 7, USER_QUANTUM 200, cpu -1)`，`USER_DEFAULT_CPU -1`），此后 `fork` 的子进程经 `sched_inherit` 而非 `sched_start`（`schedule.c:79-83` 的 `sched_inherit(SCHED, child, inherit_from, maxprio)`，`inherit_from` 为父或 `INIT`），`sched_nice` 经 `SCHEDULING_SET_NICE` 的 `_taskcall` 通知 `SCHED` 调整 `maxprio`。

用户态交接的必然：`sched_init` 仅 `INIT` 满足 `IN_USE && !PRIV_PROC`（`schedule.c:33` 过滤，启动时仅 `INIT` 为用户进程），系统服务（`PRIV_PROC`）的 `mp_scheduler == NONE` 不接管（`schedule.c:33` 过滤），`SCHED` 自身亦 `PRIV_PROC` 不自接管（`schedule.c:33` 过滤 + `SCHED_PROC_NR 4` 的 `PRIV_PROC` 自保护）。

### 1.2 为什么 `nice → queue` 必须线性缩放与钳位：`41` 与 `16` 的非整数比

`PRIO_MIN -20..PRIO_MAX 20` 宽 `41`（`sys/resource.h:43-44`），`MAX_USER_Q 0..MIN_USER_Q 15` 宽 `16`（`config.h:66-74` `NR_SCHED_QUEUES 16`），`nice` 越界 `EINVAL`（`utility.c:93` `nice<MIN||>MAX→EINVAL`）与队列钳位 `MAX_USER_Q..MIN_USER_Q`（`utility.c:99-100` `new_q<MAX→MAX, >MIN→MIN`）的双重边界：

```
queue = MAX_USER_Q + (nice - PRIO_MIN) * (MIN-MAX+1) / (PRIO_MAX-PRIO_MIN+1)
     = 0 + (nice +20) *16/41  (utility.c:95-96)
nice = (queue - USER_Q) *41/16  (main.c:284-285)
```

`USER_Q 7` 的 `((7-7)*41/16=0)` 零点对齐 `nice 0 ↔ queue 7`（`config.h:69` `USER_Q = (MIN-MAX)/2 + MAX`），`MAX_USER_Q 0 → -17`（`(0-7)*41/16=-17` 钳 `PRIO_MIN -20` 外但 `nice_to_priority:99-100` 不钳 `PRIO` 侧而钳 `queue` 侧，`MAX` 的 `-17` 在 `get_nice_value:286-287` `PRIO_MIN` 钳 ` -20` 内）与 `MIN_USER_Q 15 → 20`（`(15-7)*41/16=20` 钳 `PRIO_MAX 20` 内）的端点截断如 `init.rs:817-824` 测试。

整数截断使双射非完全可逆：`nice 0→queue 7→nice 0` 可逆，`nice 1→queue 7` 的 `(1+20)*16/41=8`? 实际 `(21*16/41=8)` 的 `queue 8 → nice (8-7)*41/16=2` 的 `1→2` 漂移为 `16:41` 非整数比的固有量化误差。

### 1.3 为什么 `sched_start_user` 有 `PRIV_PROC` 父继承 `INIT` 特例：`PRIV_PROC` 的调度上下文不可继承

`schedule.c:71-76` 的 `PRIV_PROC` 父继承 `INIT` 分支：

```
if (mproc[parent].mp_flags & PRIV_PROC) {
    assert(mproc[parent].mp_scheduler == NONE); // 72  PRIV_PROC 父的调度器必 NONE
    inherit_from = INIT_PROC_NR;               // 73  继承自 INIT 而非真实父
} else {
    inherit_from = mproc[parent].mp_endpoint;  // 75  否则继承自真实父
}
```

系统服务经 `regular fork` 产的恢复脚本（`forkexit.c:96-100` 的 `PRIV_PROC` 父调 `SCHED` 同理）不应继承 `PRIV_PROC` 父的 `NONE` 调度上下文（`RS` 等系统服务的 `mp_scheduler==NONE` 无 `maxprio/quantum`），而应继承 `INIT` 的用户策略（`INIT` 的 `mp_scheduler==KERNEL` 启动后经 `sched_init` 已为 `SCHED`，`INIT` 的 `maxprio` 为 `USER_Q` 的 `nice 0` 队列）。`fork_from` 的 `Privilege::Kernel` 父调 `SCHED` 的同源分支（`fork.rs:354-362`）与此对偶。

### 1.4 为什么 `sched_nice` 拒绝 `KERNEL/NONE` 调度器：能力守卫

`schedule.c:98-99` `if scheduler==KERNEL||NONE→EINVAL`（`95-97` 注释“内核或未指派调度器的进程不可改 nice”）的“能力守卫”：`KERNEL`（`main.c:199` 的 `INIT` 初始 `KERNEL` 在 `sched_init` 前）的 `nice` 不由 `SCHED` 管理（内核 `proc` 的 `priority` 直接 `MAX_USER_Q`？），`NONE`（`main.c:213` 的 `RS` 等系统服务的 `mp_scheduler==NONE`）的 `nice` 无调度器可通知（`_taskcall` 无目标），用户态调度器（`SCHED`）才支持 `SCHEDULING_SET_NICE` 的 `_taskcall`（`schedule.c:105-107` `m.m_pm_sched_scheduling_set_nice.endpoint/maxprio`）。

### 1.5 为什么 `do_getsetpriority` 的权限是 `eff` 三重与 `EACCES` 提优先限制：读与写的非对称

`misc.c:251-271` 的权限与提优先限制：

- `which!=PRIO_PROCESS→EINVAL`（`251-252`）仅支持进程粒度（`PRIO_PGRP/USER` 的 `EINVAL` 在 `SchedWhich` 一处枚举消失）；
- `who==0→mp` 否则 `find_proc`（`254-258`）的“自身 vs 目标”分派（`PRIO_PROCESS` 的 `0` 为 `caller` 自身）；
- 读 `GET` 与写 `SET` 共享 `eff!=SUPER_USER && eff!=target_eff && eff!=target_real→EPERM`（`260-262`）的 `may_get_prio` 三重（`eff==SUPER_USER` 或 `eff==target_eff` 或 `eff==target_real`）—— `real` 的审计源与 `eff` 的行权源双重匹配；
- 写 `SET` 额外 `target_nice > arg_pri && eff!=SUPER_USER→EACCES`（`270-271`）仅 `root` 可降低 `nice`（提升优先级，`nice` 小 `→` 优先级高），`real` 的非 `root` 可提高 `nice`（降低优先级，`nice` 大 `→` 优先级低）而不可降低。

`EACCES`（`13`）与 `EPERM`（`1`）的 `errno` 区分（`sys/errno.h:1/13`）使“不可读他人”与“不可提他人优先级”在 `may_get_prio` 与 `may_set_prio` 双谓词中可区分。

### 1.6 为什么 `get_nice_value` 是 `nice_to_priority` 的逆：`USER_Q→0` 零点与截断

`main.c:284-285` `nice = (queue-USER_Q)*41/16` 与 `utility.c:95` `queue = MAX_USER_Q + (nice-PRIO_MIN)*16/41` 互逆但因整数截断非完全双射（`16:41` 非整数比）：`USR_Q==SRV_Q==USER_Q==7 → nice 0` 零点对齐（`priv.h:93-95` 的 `SRV_Q/USR_Q` 皆 `USER_Q`），`MAX_USER_Q 0→-17` 的 `(0-7)*41/16=-17` 钳 `PRIO_MIN -20` 外但 `get_nice_value:286-287` `PRIO_MAX/MIN` 钳 ` -20..20` 内与 `nice_to_priority:99-100` 的 `MAX..MIN` 钳对偶。

### 1.7 与其他 OS 状态机的对照

Rust 改写不是照抄 `MAX_USER_Q + (nice-PRIO_MIN)*16/41` 整数算式，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `setpriority/getpriority`/`nice` + `CFS`。** Linux 的 `setpriority(PRIO_PROCESS, who, prio)` 同样 `which==PRIO_PROCESS` 仅进程粒度（`kernel/sys.c:SYSC_setpriority` 的 `which!=PRIO_PROCESS→EINVAL`），`who==0→current` 否则 `find_task_by_vpid`（与 `find_proc` 同 `who==0→self`），`capable(CAP_SYS_NICE)` 的 `EPERM` 三重（`cred->euid==0` 或 `euid==target_euid` 等）与 PM 的 `eff==SUPER_USER || eff==target_eff || eff==target_real` 同源但 Linux 以 `CAP_SYS_NICE` 能力替代 `SUPER_USER` 位。Linux `nice` 的 `PRIO_MIN -20..19`（`sys/resource.h: PRIO_MAX 20` 但 `nice` 上界 `19`）与 `CFS` 的 `load_weight` 的 `sched_prio_to_weight[40]` 非线性映射（`kernel/sched/core.c: load_weight`） vs Minix3 的 `16:41` 线性缩放—— `CFS` 的非线性使 `nice -20` 的权重为 `nice 0` 的 `~30` 倍，Minix3 的线性使 `nice -20→queue 0` 的优先级为 `nice 0→queue 7` 的 `7` 级差。`getpriority` 的 `return 20 - prio` 的 `USER_PRIO` 偏移与 PM 的 `nice-PRIO_MIN` 偏移同零点平移。

**Redox `sched_yield` + `Scheme` 票据。** Redox 以 `context::Context { status: Runnable, sched : Sched }` + `scheme::Scheme` 的 `Park` 票据与 `sched_yield` 的协作式让渡，`nice` 的优先级经 `common/src/elf.rs` 的 `priority` 字段与 `kernel/scheme` 的 `SCHED` 队列同 `minix/sched.h` 的 `maxprio/quantum` 三元。Redox 无 `PRIV_PROC` 父继承 `INIT` 特例（`Context` 的 `parent` 无 `PRIV_PROC` 语义），Minix3 的 `71-76` 分支为 `regular fork` 的 `PRIV_PROC` 父调 `SCHED` 的配套（`fork.rs:354-362` 同理）使恢复脚本继承 `INIT` 策略。

**`seL4` `sched_context` 显式票据。** `seL4` 的 `seL4_SchedContext_Bind` + `seL4_TCB_SetPriority` 的显式 `sched_context` 能力与 `MCP`（`Maximum Controlled Priority`）的 `EACCES` 提优先限制（`MCP < new_prio → EACCES`）与 PM 的 `target_nice > arg_pri && !super→EACCES` 同“仅 `root` 可提优先”，`seL4` 的 `Notification` 周期唤醒 vs `CLOCK notify` 的 `expire_timers` 同 `SCHED` 驱动但 `seL4` 的调度票据为显式 `capability`（`seL4_Signal`），PM 的 `nice` 为隐式 `mp_nice` 位图。

**结论（本章的设计基线）。** 把 C 的“`sched_start` 裸 `endpoint/m_type` + `nice→queue` 的 `16:41` 分散整数算式 + `switch(call_nr)` 的 `call_vec` 分派 + `switch(which)` 的 `PRIO_PROCESS` 唯一支持 + `eff!=SUPER_USER` 三重散落”改写为“`SchedCtl` 的 `start/inherit/set_nice` 显式三参 + `NiceMapping { MAX/MIN/USER_Q, PRIO_MIN/MAX }` 的 `to_queue/to_nice` 双函数 + `SchedWhich/Who` 枚举穷尽 + `may_get/set_prio` 一处谓词”——与 Linux/Redox 的 `nice` 双射 + `SUPER_USER` 三重同源，又因 PM 单线程无共享而以 `&mut ProcTable` 的 `NiceMapping` 显式参的 `16:41` 线性收敛。

### 1.8 小结

1. **为什么用户态调度交接**——内核提供原语，`SCHED` 持有策略，`sched_init` 使 `INIT` 从 `KERNEL` 切到 `SCHED`，此后 `fork` 经 `inherit` 而非 `start`。
2. **为什么 `nice→queue` 线性缩放**——`41` 与 `16` 的 `16:41` 非整数比，`PRIO_MIN/MAX` 边界 `EINVAL` 与队列钳位 `MAX..MIN` 双重边界，`USER_Q 7→0` 零点对齐。
3. **为什么 `PRIV_PROC` 父继承 `INIT`**——系统服务经 `regular fork` 产的恢复脚本不应继承 `PRIV_PROC` 父的 `NONE` 调度上下文，而应继承 `INIT` 的用户策略。
4. **为什么 `KERNEL/NONE→EINVAL`**——内核或未指派调度器的进程不可改 `nice`，仅用户态调度器支持 `SCHEDULING_SET_NICE`。
5. **为什么权限三重与 `EACCES`**——`eff==SUPER_USER || eff==target_eff || eff==target_real→EPERM` 的读三重与 `target_nice > arg_pri && !super→EACCES` 的写提优先限制，`EPERM` 与 `EACCES` 区分。
6. **为什么 `queue→nice` 是逆**——`nice = (queue-USER_Q)*41/16` 与 `queue = MAX + (nice-MIN)*16/41` 互逆但因截断非完全双射。

下一章逐行分析 C 的 `sched_init`/`sched_start_user`/`sched_nice`/`nice_to_priority`/`get_nice_value`/`do_getsetpriority`；第 3 章给出 Rust 的 `NiceMapping`/`SchedWhich`/`may_set_prio`。

---

## 2 C 源码分析

### 2.1 `sched_init`（`schedule.c:20-50`）

```c
void sched_init(void)
{ // 20  启动时 INIT 接管
  struct mproc *trmp; endpoint_t parent_e; int proc_nr, s; // 22-24
  for (proc_nr=0, trmp=mproc; proc_nr < NR_PROCS; proc_nr++, trmp++) { // 26  全表扫描
    if (trmp->mp_flags & IN_USE && !(trmp->mp_flags & PRIV_PROC)) { // 33  仅用户进程（启动时仅 INIT）
      assert(_ENDPOINT_P(trmp->mp_endpoint) == INIT_PROC_NR); // 34  必 INIT 槽
      parent_e = mproc[trmp->mp_parent].mp_endpoint; // 35  父 endpoint
      assert(parent_e == trmp->mp_endpoint); // 36  INIT 自父
      s = sched_start(SCHED_PROC_NR, trmp->mp_endpoint, parent_e, USER_Q 7, USER_QUANTUM 200, -1, &trmp->mp_scheduler); // 37-43  maxprio 7/quantum 200/cpu -1
      if (s != OK) printf("PM: SCHED denied taking over scheduling of %s: %d\n", trmp->mp_name, s); // 44-47  非 panic
    }
  }
}
```

`33` 行 `IN_USE && !PRIV_PROC` 过滤使 `sched_init` 仅 `INIT` 满足（系统服务 `PRIV_PROC` 不接管，`SCHED` 自身亦 `PRIV_PROC`），`34` 行 `INIT_PROC_NR` 断言与 `36` 行 `parent_e==self` 的 `INIT` 自父对偶（`01` 的 `MP_IS_INIT` 自父），`37-43` 行 `sched_start` 的 `USER_Q 7`/`USER_QUANTUM 200`/`cpu -1`（`USER_DEFAULT_CPU -1`）三元与 `minix/sched.h:7-12` 的 `sched_start` 六参对偶，`44-47` 行非 `panic` 的 `printf` 使 `SCHED` 拒绝接管时仅告警（`01` 的 `init_scheduling` 同 `eprintln`）。

### 2.2 `sched_start_user`（`schedule.c:55-84`）

```c
int sched_start_user(endpoint_t ep, struct mproc *rmp)
{ // 55  继承式启动（fork 与 VFS_PM_FORK_REPLY 成功路径）
  unsigned maxprio; endpoint_t inherit_from; int rv; // 57-59
  if ((rv = nice_to_priority(rmp->mp_nice, &maxprio)) != OK) return rv; // 62  nice→queue
  if (mproc[rmp->mp_parent].mp_flags & PRIV_PROC) { // 71  父为系统服务
    assert(mproc[rmp->mp_parent].mp_scheduler == NONE); // 72  必 NONE
    inherit_from = INIT_PROC_NR; // 73  继承自 INIT 而非真实父
  } else {
    inherit_from = mproc[rmp->mp_parent].mp_endpoint; // 75  否则继承自真实父
  }
  return sched_inherit(ep, rmp->mp_endpoint, inherit_from, maxprio, &rmp->mp_scheduler); // 79-83  maxprio 继承
}
```

`62` 行 `nice_to_priority` 的 `maxprio` 变换与 `D4` 的 `NiceMapping::to_queue` 同算式，`71-76` 行 `PRIV_PROC` 父继承 `INIT` 分支与 `fork` 的 `PRIV_PROC` 父调 `SCHED`（`forkexit.c:96-100`）同源但 `inherit_from` 为 `INIT` 端点而非调度器，`79-83` 行 `sched_inherit` 的 `maxprio` 继承使子进程优先级跟随 `nice` 而非 `USER_Q` 固定。

### 2.3 `sched_nice`（`schedule.c:89-112`）

```c
int sched_nice(struct mproc *rmp, int nice)
{ // 89  改 nice 并通知 SCHED
  int rv; message m; unsigned maxprio; // 91-93
  if (rmp->mp_scheduler == KERNEL || rmp->mp_scheduler == NONE) return (EINVAL); // 98-99  KERNEL/NONE→EINVAL 守卫
  if ((rv = nice_to_priority(nice, &maxprio)) != OK) return rv; // 101  nice→queue
  m.m_pm_sched_scheduling_set_nice.endpoint = rmp->mp_endpoint; // 105  endpoint
  m.m_pm_sched_scheduling_set_nice.maxprio = maxprio; // 106  maxprio
  if ((rv = _taskcall(rmp->mp_scheduler, SCHEDULING_SET_NICE, &m))) return rv; // 107  _taskcall 通知
  return (OK); // 111
}
```

`98-99` 行 `KERNEL/NONE→EINVAL` 守卫在 `do_getsetpriority` 的 `EACCES` 前置——先判调度器能力再判权限（`misc.c:270-271` 的 `nice>target && !super→EACCES` 需 `sched_nice` 已可 `can_nice`），`105-107` 行 `endpoint/maxprio` + `SCHEDULING_SET_NICE 5` 的 `_taskcall` 与 `SchedCtl::set_nice` 同编码（`minix/com.h: SCHEDULING_SET_NICE`）。

### 2.4 `nice_to_priority`（`utility.c:91-103`）

```c
int nice_to_priority(int nice, unsigned* new_q)
{ // 91  nice→queue 线性缩放
  if (nice < PRIO_MIN || nice > PRIO_MAX) return(EINVAL); // 93  -20..20 边界
  *new_q = MAX_USER_Q + (nice-PRIO_MIN) * (MIN_USER_Q-MAX_USER_Q+1) / (PRIO_MAX-PRIO_MIN+1); // 95-96  0 + (nice+20)*16/41
  if ((signed) *new_q < MAX_USER_Q) *new_q = MAX_USER_Q; // 99  钳下界
  if (*new_q > MIN_USER_Q) *new_q = MIN_USER_Q; // 100  钳上界
  return (OK); // 102
}
```

`93` 行 `nice` 边界 `PRIO_MIN -20..20` 与 `get_nice_value:93` 同集合，`95-96` 行 `16/41` 的 `MIN-MAX+1=16` 与 `PRIO_MAX-MIN+1=41` 非整数比缩放与 `main.c:284-285` 的 `41/16` 逆缩放互补，`99-100` 行队列钳位 `0..15` 与 `get_nice_value:286-287` 的 `PRIO` 钳对偶。

### 2.5 `get_nice_value`（`main.c:276-289`）

```c
static int get_nice_value(int queue)
{ // 276  queue→nice 逆缩放
  int nice_val = (queue - USER_Q) * (PRIO_MAX-PRIO_MIN+1) / (MIN_USER_Q-MAX_USER_Q+1); // 284-285  (queue-7)*41/16
  if (nice_val > PRIO_MAX) nice_val = PRIO_MAX; // 286  钳上界 20
  if (nice_val < PRIO_MIN) nice_val = PRIO_MIN; // 287  钳下界 -20
  return nice_val; // 288
}
```

`284-285` 行 `(queue-7)*41/16` 的 `i32` 截断与钳位 `PRIO_MAX/MIN`（`-20..20`）与 `nice_to_priority:99-100` 的 `MAX..MIN` 钳对偶（`USER_Q 7→0` 零点对齐，`MAX 0→-17` 的 `(0-7)*41/16=-17` 钳 ` -20` 内与 `MIN 15→20` 的 `(8*41/16=20)` 钳 `20` 内的端点截断如 `init.rs:817-824` 测试）。

### 2.6 `do_getsetpriority`（`misc.c:239-286`）

```c
int do_getsetpriority(void)
{ // 239  get/setpriority 的 PM 侧入口
  int r, arg_which, arg_who, arg_pri; struct mproc *rmp; // 241-242
  arg_which = m_in.m_lc_pm_priority.which; arg_who = m_in.m_lc_pm_priority.who; arg_pri = m_in.m_lc_pm_priority.prio; // 244-246  which/who/prio 三参（ipc.h:484 `which/who/prio`）
  if (arg_which != PRIO_PROCESS) return(EINVAL); // 251-252  仅 PRIO_PROCESS 0 支持（PRIO_PGRP 1/USER 2→EINVAL 在 SchedWhich 一处枚举消失）
  if (arg_who == 0) rmp = mp; // 254-255  who==0→mp（caller 自身）
  else if ((rmp = find_proc(arg_who)) == NULL) return(ESRCH); // 257-258  否则 find_proc
  if (mp->mp_effuid != SUPER_USER && mp->mp_effuid != rmp->mp_effuid && mp->mp_effuid != rmp->mp_realuid) return EPERM; // 260-262  eff 三重
  if (call_nr == PM_GETPRIORITY) return(rmp->mp_nice - PRIO_MIN); // 265-266  GET→nice-PRIO_MIN 的 USER_PRIO 偏移（resource.h: GET 返回 USER_PRIO）
  if (rmp->mp_nice > arg_pri && mp->mp_effuid != SUPER_USER) return(EACCES); // 270-271  仅 root 可降低 nice 提优先级
  if ((r = sched_nice(rmp, arg_pri)) != OK) return r; // 280  调度器通知
  rmp->mp_nice = arg_pri; // 284  回填
  return(OK); // 285
}
```

`251-252` 行 `which!=PRIO_PROCESS→EINVAL` 与 `SchedWhich` 枚举穷尽（`PRIO_PROCESS 0` 唯一支持），`254-258` 行 `who==0→mp` 否则 `find_proc` 的“自身 vs 目标”分派与 `GETPID` 的 `who_p` 显式 `caller` 同源，`260-262` 行 `eff` 三重与 `may_get_prio` 同谓词（`eff==SUPER_USER || eff==target_eff || eff==target_real`），`265-266` 行 `GET→nice-PRIO_MIN` 的 `USER_PRIO` 偏移（`sys/resource.h: GET` 返回 `nice - PRIO_MIN` 的 `0..40` 值域）与 `SET` 的 `nice` 存 `mp_nice` 对偶，`270-271` 行 `nice>pri && !super→EACCES` 的“仅 `root` 可提优先”与 `EPERM` 的“不可读他人”区分。

### 2.7 调度协议（`minix/sched.h:7-12` + `minix/com.h: SCHEDULING_*` + `minix/config.h:66-74`）

- `sched_start(SCHED, schedulee, parent, maxprio 7, quantum 200, cpu -1, &newsched)`（`sched.h:7-12` 六参，`schedule.c:37-43` `USER_Q/USER_QUANTUM`）+ `sched_inherit(SCHED, schedulee, parent, maxprio, &newsched)`（`sched.h:10-12` 四参，`schedule.c:79-83` `maxprio` 继承）+ `sched_stop(SCHED, schedulee)`（`sched.h:7` 二参，`06-stage-sched` 的 `sched_stop` 队列清理）
- `SCHEDULING_START 0` / `SCHEDULING_SET_NICE 5`（`minix/com.h: SCHEDULING_*`，`schedule.c:37`/`105` 的 `_taskcall` 编码）+ `m_pm_sched_scheduling_set_nice.endpoint/maxprio`（`ipc.h: ` `mess_pm_sched_scheduling_set_nice`，`schedule.c:105-106`）
- `NR_SCHED_QUEUES 16` / `MAX_USER_Q 0` / `MIN_USER_Q 15` / `USER_Q 7` / `USER_QUANTUM 200`（`config.h:66-74`）+ `PRIO_MIN -20`/`PRIO_MAX 20`（`resource.h:43-44`）

### 2.8 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `sched_init` 仅 `INIT` | `schedule.c:33` `IN_USE && !PRIV_PROC` | 启动时仅 `INIT` 为用户进程（系统服务 `PRIV_PROC`） | 不变量 |
| `PRIV_PROC` 父继承 `INIT` | `schedule.c:71-73` | `fork` 子的父为系统服务时继承自 `INIT` 而非真实父 | 不变量 |
| `KERNEL/NONE→EINVAL` | `schedule.c:98-99` | 内核或未指派调度器的进程不可改 `nice` | 可恢复 `EINVAL` |
| `PRIO_MIN/MAX` 边界 | `utility.c:93` `nice<MIN||>MAX→EINVAL` | `nice` 越界 | 可恢复 `EINVAL` |
| `which==PRIO_PROCESS` 唯一 | `misc.c:251-252` | `PRIO_PGRP/USER→EINVAL` | 可恢复 `EINVAL` |
| `EPERM` 三重 | `misc.c:260-262` | `eff!=SUPER_USER && eff!=target_eff && eff!=target_real` | 可恢复 `EPERM` |
| `EACCES` 提优先 | `misc.c:270-271` | `target_nice > arg_pri && !super` 仅 `root` 可提优先 | 可恢复 `EACCES` |
| `GET→USER_PRIO` 偏移 | `misc.c:266` `nice-PRIO_MIN` | `GET` 返回 `0..40` 的 `USER_PRIO` | 不变量 |

---

## 3 Rust 设计决策

Rust 改写遵循“显式 `ProcTable::user_procs` 迭代器 + `inherit_parent` 一处方法 + `NiceMapping` 双射 + `SchedWhich/Who` 枚举 + `may_get/set_prio` 一处谓词 + `SchedCtl::set_nice` trait”的 8 决策，保留 C 的 `IN_USE && !PRIV_PROC` 扫描与 `16:41` 线性缩放，但以类型系统使 `PRIO_PROCESS` 判据与 `SUPER_USER` 三重显式化。以下决策对应设计契约 `.design/16-design.v1.md` 的 D1–D8。

### D1：`sched_init` 的扫描收敛到迭代器（ARCH A-8）

- **C**：`26` `for proc_nr,trmp` 扫描 + `33` 过滤。
- **Rust**：`ProcTable::user_procs() -> impl Iterator<Item=UserSlot>`（`IN_USE && !PRIV_PROC` 过滤的 `which` 迭代器），`fn sched_init(table, sched: &mut dyn SchedCtl) -> Vec<(UserSlot, Result<(), SchedError>)>`（遍历 `user_procs`，每项 `debug_assert_eq!(slot, INIT_PROC_NR)` 后 `sched.start(SCHED, schedulee, parent, USER_Q, USER_QUANTUM, cpu=-1)`，失败 `Err` 收集后 `eprintln` 而非 `panic`，与 `44-47` 同 `printf`）。

### D2：`sched_start_user` 的 `PRIV_PROC` 父继承收敛到 `inherit_parent`（ARCH A-8）

- **C**：`71-76` `if parent PRIV_PROC { inherit_from=INIT } else parent_endpoint`。
- **Rust**：`fn inherit_parent(table, rmp_parent: UserSlot) -> Endpoint`（`if is_kernel_process(parent) { assert(scheduler==NONE); INIT_PROC_NR } else { parent.endpoint() }`，`A-8`），`fn sched_start_user(table, ep, rmp, sched: &mut dyn SchedCtl) -> Result<(), SchedError>`（`62` `nice_to_priority` 后 `inherit_parent` 后 `sched.inherit(ep, rmp.endpoint, inherit_from, maxprio)`）。

### D3：`sched_nice` 的 `KERNEL/NONE→EINVAL` 守卫收敛到 `can_nice`（ARCH A-8）

- **C**：`98-99` `if scheduler==KERNEL||NONE→EINVAL`。
- **Rust**：`fn can_nice(scheduler: Endpoint) -> bool { scheduler != KERNEL && scheduler != NONE }`（`Endpoint::is_kernel` 判据在 `Privilege::can_nice` 一处，`A-8`），`fn sched_nice(table, rmp, nice, sched: &mut dyn SchedCtl) -> Result<(), SchedError>`（`98-99` 守卫先于 `nice_to_priority`）。

### D4：`nice ↔ queue` 的线性缩放收敛到 `NiceMapping`（ARCH A-8）

- **C**：`utility.c:95-96` `queue = MAX + (nice-MIN)*16/41` + `main.c:284-285` `nice = (queue-USER_Q)*41/16`。
- **Rust**：`struct NiceMapping { MAX_USER_Q: i32, MIN_USER_Q: i32, PRIO_MIN: i32, PRIO_MAX: i32 }` + `fn to_queue(&self, nice: i32) -> Result<u32, EINVAL>`（`nice<MIN||>MAX→EINVAL` 先于缩放，缩放后钳位 `MAX..MIN`）+ `fn to_nice(&self, queue: i32) -> i32`（`queue-USER_Q)*41/16` 的 `i32` 截断与钳位 `PRIO_MAX/MIN` 与 C 一致，`USER_Q 7→0` 零点对齐）。

### D5：`do_getsetpriority` 的 `which/who` 收敛到枚举（ARCH A-2）

- **C**：`251-252` `which!=PRIO_PROCESS→EINVAL` + `254-258` `who==0→mp` 否则 `find_proc`。
- **Rust**：`enum SchedWhich { Process }` + `TryFrom<i32>`（`0→Process` `PRIO_PROCESS 0`，其余 `EINVAL`），`enum Who { Slf, Pid(Pid) }` + `fn resolve(table, caller, who: i32) -> Result<UserSlot, SchedError>`（`0→Slf` 否则 `find_proc` → `ESRCH`），`fn do_getsetpriority(table, caller, which, who, pri, sched) -> Result<i32, SchedError>`（`which` 先于 `who` 校验，与 `251-252` 同序）。

### D6：权限三重收敛到 `may_set_prio`（ARCH A-12 外）

- **C**：`260-262` `eff!=SUPER_USER && eff!=target_eff && eff!=target_real→EPERM` + `270-271` `nice>pri && !super→EACCES`。
- **Rust**：`fn may_get_prio(caller: &Credentials, target: &Credentials) -> bool` + `fn may_set_prio(caller, target, target_nice, arg_pri) -> Result<(), SchedError>`（`may_get` 先于 `may_set` 的 `EACCES` 判据，`target_nice > arg_pri` 的“降低 `nice` 提优先”与 `EACCES` 同谓词）。

### D7：`SCHEDULING_SET_NICE` 的 `_taskcall` 收敛到 `SchedCtl::set_nice` trait（ARCH A-8）

- **C**：`105-107` `m.m_pm_sched_scheduling_set_nice.endpoint/maxprio` + `_taskcall(SCHEDULING_SET_NICE)`。
- **Rust**：`trait SchedCtl { fn start(&mut self, sched, schedulee, parent, maxprio, quantum, cpu) -> i32; fn inherit(&mut self, sched, schedulee, parent, maxprio) -> i32; fn set_nice(&mut self, sched, schedulee, maxprio) -> i32; }`（`A-8` 硬件抽象，`minix/com.h: SCHEDULING_*` 单一真相到 `minix-types`）。

### D8：常量收敛到 `minix-types`（单一真相）

- **C**：`resource.h:43-44` `PRIO_MIN -20/PRIO_MAX 20`、`config.h:66-74` `NR_SCHED_QUEUES 16/MAX/MIN/USER_Q/USER_QUANTUM 200`、`sched.h: SCHED_PROC_NR 4`。
- **Rust**：`minix-types: PRIO_MIN/MAX -20/20`、`NR_SCHED_QUEUES 16`、`MAX/MIN/USER_Q`、`USER_QUANTUM 200`、`SCHED_PROC_NR 4`、`SCHEDULING_SET_NICE 5` 单一真相（`config.h:68-74` 数值锁定，测试 `test_constants_match_c`）。

### ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-8 调度客户端 | `SchedCtl` 的 `start/inherit/set_nice`（D1/D2/D3/D7） | `sched.rs` + 本文档 §3.1/3.3/3.7 + 计划 §4 |
| A-2 flag→枚举 | `SchedWhich/Who`（D5） | `sched.rs` + 本文档 §3.5 + 计划 §4 |
| A-3 全局→显式 | `SchedCtl` 显式 `caller: UserSlot`（D5/D6） | `sched.rs` 注释 + 本文档 §3.5/3.6 + 计划 §4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── mproc/
│   └── mproc.rs         — ProcessResources { nice, scheduler }（已存，sched_init 回填 SCHED，sched_nice 后 mp_nice=pri）
├── sched.rs             — NiceMapping + SchedWhich/Who + may_get/set_prio + SchedCtl trait + sched_init/sched_start_user/sched_nice/do_getsetpriority + nice_to_priority/get_nice_value 纯函数
└── ipc/
    └── mod.rs           — （无新增，sched 的 sys 调用经 sched.rs 的 trait 注入）
```

### 4.2 `sched.rs`：双射与调度交接

```rust
pub struct NiceMapping { pub MAX_USER_Q: i32, pub MIN_USER_Q: i32, pub PRIO_MIN: i32, pub PRIO_MAX: i32 } // 0,15,-20,20
impl NiceMapping {
    pub fn to_queue(&self, nice: i32) -> Result<u32, SchedError> // nice<MIN||>MAX→EINVAL 先于缩放，缩放后钳位 MAX..MIN (95-96/99-100)
    pub fn to_nice(&self, queue: i32) -> i32 // (queue-USER_Q)*41/16 的 i32 截断与钳位 PRIO_MAX/MIN (284-287)
}
pub enum SchedWhich { Process } // PRIO_PROCESS 0 唯一支持
pub enum Who { Slf, Pid(Pid) } // 0→Slf 否则 find_proc
pub trait SchedCtl { fn start(&mut self, sched: Endpoint, schedulee: Endpoint, parent: Endpoint, maxprio: i32, quantum: i32, cpu: i32) -> i32; fn inherit(...); fn set_nice(...); }
pub fn sched_init(table: &mut ProcTable, sched: &mut dyn SchedCtl) -> Vec<(UserSlot, i32)>
pub fn sched_start_user(table: &mut ProcTable, ep: Endpoint, rmp: UserSlot, sched: &mut dyn SchedCtl) -> Result<(), SchedError>
pub fn sched_nice(table: &mut ProcTable, rmp: UserSlot, nice: i32, sched: &mut dyn SchedCtl) -> Result<(), SchedError>
pub fn do_getsetpriority(table: &mut ProcTable, caller: UserSlot, which: i32, who: i32, pri: i32, sched: &mut dyn SchedCtl) -> Result<i32, SchedError>
```

- `NiceMapping::to_queue`：`if nice<MIN||>MAX→EINVAL` 先于缩放，`MAX + (nice-MIN)*16/41` 的 `i32` 缩放后 `MAX..MIN` 钳位（`99-100`），`to_nice` 的 `(queue-USER_Q)*41/16` 的 `i32` 截断与 `PRIO_MAX/MIN` 钳位（`284-287`）与 C 一致，`USER_Q 7→0` 零点对齐。
- `sched_init`：`user_procs()` 迭代 `IN_USE && !PRIV_PROC` 的 `INIT` 单项，`assert(_ENDPOINT_P==INIT_PROC_NR)` 后 `sched.start(SCHED, schedulee, parent, USER_Q, USER_QUANTUM, cpu=-1)`，失败 `eprintln` 非 `panic`（`44-47`）。
- `sched_start_user`：`NiceMapping::to_queue` 后 `inherit_parent` 的 `PRIV_PROC→INIT` 分支后 `sched.inherit` 四参继承（`71-83`）。
- `sched_nice`：`can_nice` 守卫先于 `nice_to_priority`，`sched.set_nice` 的 `endpoint/maxprio` + `SCHEDULING_SET_NICE`（`105-107`）。
- `do_getsetpriority`：`SchedWhich::try_from(which)` 先于 `Who::resolve(who)`，`may_get_prio` 三重先于 `GET→nice-PRIO_MIN`（`266`）与 `SET` 的 `may_set_prio` 的 `EACCES` 后于 `sched_nice`（`280`）+ `mp_nice=pri`（`284`）。

### 4.3 `mproc/mproc.rs`：`nice/scheduler` 二元

```rust
// ProcessResources { nice: i32, scheduler: Endpoint } // 已存
// sched_init 回填 Endpoint::SCHED，sched_nice 后 mp_nice=pri
```

`sched_init` 回填 `Endpoint::SCHED`（`schedule.c:43` `&mp_scheduler`），`sched_nice` 成功后 `mp_nice=pri`（`misc.c:284`）与 `do_getsetpriority` 的 `SET` 路径同存。

### 4.4 `init.rs` 接线：`sched_init` 调用点

```rust
// PmServer::init 末步 sched_init 的 SCHED 交接（init.rs:239-250）
// PmServer::init_scheduling 已抽象为 SchedCtl::start 的 Ok(SCHED) 占位，16 落真实 start
```

生产 `SchedCtl::start` 的 `SCHED_PROC_NR 4` 常量与 `com.h: SCHEDULING_START` 单一真相到 `minix-types`。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 | 检测 |
|---|--------|--------|-----------|------|
| 1 | `sched_init` 仅 `INIT` | `schedule.c:33` `IN_USE && !PRIV_PROC` | `user_procs()` 迭代 `INIT` 单项 | `test_sched_init_only_init` |
| 2 | `PRIV_PROC` 父继承 `INIT` | `schedule.c:71-73` | `inherit_parent` 的 `PRIV_PROC→INIT` | `test_sched_start_user_priv_parent` |
| 3 | `KERNEL/NONE→EINVAL` | `schedule.c:98-99` | `can_nice` 守卫先于缩放 | `test_sched_nice_kernel_none_inval` |
| 4 | `PRIO_MIN/MAX` 边界 | `utility.c:93` | `NiceMapping::to_queue` 先于缩放 | `test_nice_to_priority_bounds` |
| 5 | `which==PRIO_PROCESS` 唯一 | `misc.c:251-252` | `SchedWhich::try_from` 穷尽 | `test_do_getsetpriority_which_inval` |
| 6 | `EPERM` 三重 | `misc.c:260-262` | `may_get_prio` | `test_do_getsetpriority_eperm` |
| 7 | `EACCES` 提优先 | `misc.c:270-271` | `may_set_prio` 的 `nice>pri && !super` | `test_do_getsetpriority_eacces` |
| 8 | `GET→USER_PRIO` 偏移 | `misc.c:266` `nice-PRIO_MIN` | `do_getsetpriority` 的 `GET→0..40` | `test_do_getsetpriority_get_user_prio` |

---

## 5 测试矩阵

> 基线：`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **278 passed / 0 failed**（原 264 + 本档新增 ~14：`sched.rs` 12 + `mproc/mproc.rs` 2）。结果见 `cargo test` 末段统计段（§2.4j 格式）。

### 5.1 `sched.rs`（调度协议与双射）

- `test_nice_to_priority_bounds`：`nice -20→0`/`0→7`/`20→15` 与越界 `-21/21→EINVAL`（`utility.c:93/95-96`）
- `test_get_nice_value`：`queue 0→-17`/`7→0`/`15→20` 与钳位 `-20/20`（`main.c:284-287`）
- `test_nice_roundtrip`：`nice 0→queue 7→nice 0` 可逆，`nice 1→queue 7→nice 0` 的量化误差（`16:41` 非整数比）
- `test_sched_init_only_init`：`IN_USE && !PRIV_PROC` 仅 `INIT` 接管（`schedule.c:33`），`PRIV_PROC` 不接管
- `test_sched_start_user_priv_parent`：`PRIV_PROC` 父继承 `INIT`（`71-76`）vs 真实父
- `test_sched_nice_kernel_none_inval`：`KERNEL/NONE→EINVAL`（`98-99`）
- `test_do_getsetpriority_which_inval`：`which!=PRIO_PROCESS→EINVAL`（`251-252`）
- `test_do_getsetpriority_who_zero_self`：`who==0→Self`（`254-255`）vs `find_proc` `ESRCH`
- `test_do_getsetpriority_eperm`：`eff!=SUPER_USER && eff!=target_eff && eff!=target_real→EPERM`（`260-262`）
- `test_do_getsetpriority_eacces`：`nice>pri && !super→EACCES`（`270-271`）仅 `root` 可提优先
- `test_do_getsetpriority_get_user_prio`：`GET→nice-PRIO_MIN` 的 `0..40` `USER_PRIO` 偏移（`266`）
- `test_do_getsetpriority_set_updates_nice`：`SET→sched_nice→mp_nice=pri`（`280/284`）与 `SCHEDULING_SET_NICE` 编码
- `test_constants_match_c`：锁定 `PRIO_MIN -20/PRIO_MAX 20`（`resource.h`）、`MAX_USER_Q 0/MIN_USER_Q 15/USER_Q 7`（`config.h`）、`USER_QUANTUM 200`、`SCHED_PROC_NR 4`

### 5.2 `mproc/mproc.rs`（`nice/scheduler` 二元）

- `test_nice_scheduler_default`：`nice 0` + `scheduler Endpoint::NONE` 默认
- `test_sched_init_fills_scheduler`：`sched_init` 回填 `SCHED`（`schedule.c:43`）

### 5.3 `minix-types`（常量）

- `test_constants_match_c`：锁定 `PRIO_MIN/MAX` 等单一真相（`sys/resource.h`/`config.h`）

测试策略：`SchedCtl` 的 `start/inherit/set_nice` 均 `TestSched` mock 可注入 `OK/EINVAL` 与 `maxprio/quantum` 计数；`NiceMapping` 的双射纯函数脱离 `ProcTable` 独立测；`do_getsetpriority` 的 `EPERM/EACCES` 双重错误在 `may_get/set_prio` 一处谓词验证。

---

## 6 过渡

本篇在 `sched_init` 的 `INIT` 接管与 `do_getsetpriority` 的 `GET/SET` 之间，是 `01` 的 `fill_boot_procs` 的 `nice=0` 初始与 `05` 的 `VFS_PM_FORK_REPLY` 的 `sched_start_user` 成功路径的调度器交接；`get_nice_value` 的 `USER_Q→0` 零点为 `03` 的 `procs_in_use` 初始化时 `nice` 默认值：

```
01-pm-init-main.md（fill_boot_procs 的 nice=0 初始与 sched_init 调用点）
  │
  └─► 本章（sched_init 的 INIT 接管→sched_start_user 的 PRIV_PROC 父继承 INIT→sched_nice 的 KERNEL/NONE 守卫→nice↔queue 双射→do_getsetpriority 的 EPERM/EACCES 权衡）
         │
         ├─► 05-vfs-interaction.md（VFS_PM_FORK_REPLY→sched_start_user 的 maxprio 继承，VFS 回复成功路径的调度器交接）
         └─► 17-exec.md（do_exec 的 exec_restart 后 sched_start_user 再继承，PARTIAL_EXEC 与 TAINTED 的调度无关但共享 ProcTable 锁）
```

`nice_to_priority` 的 `MAX + (nice-MIN)*16/41` 与 `get_nice_value` 的 `(queue-USER_Q)*41/16` 互逆但因截断非完全双射（`init.rs:817` 测试 `MAX→-17` 与 `MIN→20` 的端点截断）使 `nice 0→queue 7→nice 0` 可逆而 `nice 1→queue 7→nice 0` 的量化误差为 `16:41` 非整数比的固有。

阅读顺序提示：若想先理解“内核调度器就绪队列与 `do_schedule`”，下一站 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/11-scheduling-primitives.md`（`kernel/proc.c:schedule` 的 `pick_proc`）；若想理解“`SCHED` 服务实现”，下一站 `06-stage-sched`（`servers/sched` 的 `sched_start/sched_set_nice` 队列管理）。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/schedule.c` 全文（`20-50` `sched_init` 等）、`minix3/minix/servers/pm/utility.c:91-103`（`nice_to_priority`）、`minix3/minix/servers/pm/main.c:276-289`（`get_nice_value`）、`minix3/minix/servers/pm/misc.c:239-286`（`do_getsetpriority`）、`minix3/minix/include/minix/config.h:66-74`（`NR_SCHED_QUEUES/MAX/MIN/USER_Q/USER_QUANTUM`）、`minix3/sys/sys/resource.h:43-44`（`PRIO_MIN -20/PRIO_MAX 20`）、`minix3/minix/include/minix/sched.h:7-12`（`sched_start/sched_inherit`）
- PM 阶段文档：01-pm-init-main.md（`sched_init` 调用点与 `USER_Q` 零点）、02-mproc-struct.md（`nice/scheduler` 二元）、04-ipc-dispatch.md（`call_vec` 分发）、05-vfs-interaction.md（`VFS_PM_FORK_REPLY` 的 `sched_start_user` 成功路径）、11-signal-core.md（`SIGCHLD` 不经调度）、01-stage-kernel/11-scheduling-primitives.md（内核 `schedule` 原语）、06-stage-sched（`SCHED` 服务实现）
- 阶段内顺序：01 → **本章（16）** → 05（`VFS_PM_FORK_REPLY` 的 `sched_start_user` 成功路径 `maxprio` 继承已在 `05` 实现，但 `16` 落 `nice_to_priority` 真实变换）→ 17（`do_exec` 的 `exec_restart` 后 `sched_start_user` 再继承）
- OS 模式参考：Linux `setpriority`/`nice` + `CFS` `load_weight`（`kernel/sched/core.c`）、Redox `sched_yield` + `Scheme` 票据（`kernel/context`）、`seL4` `sched_context` 显式票据与 `MCP` 的 `EACCES`（见 §1.7）
- Rust 实现：`os/servers/pm/src/mproc/mproc.rs`（`nice/scheduler` 二元）、`os/servers/pm/src/sched.rs`（`NiceMapping`/`SchedWhich`/`may_set_prio`/`SchedCtl`/`sched_init/start_user/sched_nice/do_getsetpriority`）

