# 07-rs-period-heartbeat: 周期检查与心跳监控

> **分类**: 阶段 3 — 主循环与监控（心跳状态机）
> **源码**: `minix3/minix/servers/rs/request.c:943-1046`（`do_period`）、`request.c:1051-1090`（`do_sigchld`）、`minix3/minix/servers/rs/update.c:371-396`（`update_period`）、`minix3/minix/servers/rs/const.h:31,34-35,39,48-51,58,110,114-115`（标志与常量）
> **Rust 模块**: `os/servers/rs/src/monitor.rs`（`period_decision`/`effective_period`/`has_update_timed_out`/`sigchld_cleanup`/`PeriodAction`/常量族）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`r_period`/`r_backoff`/`r_stop_tm`/`r_alive_tm`/`r_check_tm` 字段）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md`（主循环 ClockNotify 分支）
> **说明**: RS 是微内核里唯一"看门狗"：内核不管服务死没死，RS 靠**心跳协议**发现故障并触发恢复。本文档建模 `do_period` 的三分支状态机（backoff 复活 / SIGTERM→SIGKILL / ping 超时 crash）、free pass 例外、`do_sigchld` 的子进程清理与 `update_period` 的 update 准备超时。

---

## 1. 概念：微内核的看门狗

### 1.0 章节引言

Minix3 的微内核设计里，一个服务崩溃**内核不会自动重启它**——那是 RS 的工作（`15-rs-terminate-restart.md`）。但 RS 怎么知道服务崩溃了？答案是**心跳协议**：RS 周期性地向有 `r_period` 的服务发 `ipc_notify`（ping），服务在每次收到消息（含 notify）时回复心跳（实际是内核把 notify 的 timestamp 记入 `r_alive_tm`，见 06 §2.1）。没回复 = 可能死了 = 触发恢复。

> **本章不讲什么**（机制一律移交）:
> - `restart_service`/`crash_service`/`cleanup_service` 的恢复机制（`15-rs-terminate-restart.md`）
> - `end_update`/update 状态机（`16-rs-live-update.md`）——`update_period` 只陈述调用点（plan §2.2 豁免）
> - 主循环分类与 reply 协议（`06-rs-main-loop.md`）
> - `do_sigchld` 里 `rupdate_clear_upds` 的链语义（16）
>
> 本章只回答一个问题：**`do_period` 在每次时钟 notify 时对每个服务做什么决策**——三条恢复路径的触发条件与 C 行号。

### 1.1 为什么需要心跳（WHY）

服务可能以多种方式死亡：正常退出（`_exit`）、崩溃（信号）、挂起（死锁）。RS 需要区分并分别处理：

```
服务死亡方式          RS 的检测手段                恢复路径
──────────────────────────────────────────────────────────────
反复快速退出          r_backoff 二进制退避          backoff 计数归零 → restart_service
SIGTERM 后不退出      r_stop_tm 超时                crash_service（模拟崩溃）→ 自动重启
不响应心跳（挂起）    r_alive_tm < r_check_tm 超时   crash_service（模拟崩溃）→ 自动重启
```

三种检测都发生在 `do_period` 的**同一趟全表扫描**里（request.c:960-1041），每次时钟 notify 执行一次。时钟周期是 `RS_DELTA_T = system_hz`（const.h:49，每秒一次）。

### 1.2 心跳协议的两时间戳（WHAT）

每个服务槽有两个时间戳（02 §字段表）：

- **`r_alive_tm`**（type.h:73）：最近一次**心跳**时间——服务发任何消息（含 notify 回复）时由主循环写入（06 §2.1，main.c:87）。
- **`r_check_tm`**（type.h:72）：最近一次**ping**时间——RS 发出 `ipc_notify` 时写入（request.c:1037）。

判定逻辑：**`r_alive_tm < r_check_tm`** 说明"ping 发出后服务还没回过心跳"——答案待定；超过 `2 × period` 仍未答 → 服务挂起 → crash。

---

## 2. C 源码分析

### 2.1 `do_period`（request.c:943-1046）三分支

```c
void do_period(m_ptr)                                /* request.c:943 */
{
  clock_t now = m_ptr->m_notify.timestamp;           /* request.c:948 */
  /* update 进行中且非 initializing → 检查 update 状态（→16） */
  if(RUPDATE_IS_UPDATING() && !RUPDATE_IS_INITIALIZING()) { /* 953-954 */
      update_period(m_ptr);                          /* request.c:954 */
  }
  for (rp=...; rp<END_RPROC_ADDR; rp++) {            /* request.c:960 */
      if ((rp->r_flags & RS_ACTIVE) &&               /* request.c:963 */
          (!SRV_IS_UPDATING(rp) ||
           ((rp->r_flags & (RS_INITIALIZING|RS_INIT_DONE|RS_INIT_PENDING))
              == RS_INITIALIZING))) {
          period = rp->r_period;                     /* request.c:966 */
          if(rp->r_flags & RS_INITIALIZING) {        /* request.c:967 */
              period = SRV_IS_UPDATING(rp) ?
                  UPD_INIT_MAXTIME(&rp->r_upd) : RS_INIT_T; /* 968-969 */
          }
          if (rp->r_backoff > 0) {                   /* request.c:975 */
              rp->r_backoff -= 1;                    /* request.c:976 */
              if (rp->r_backoff == 0) {              /* request.c:977 */
                  restart_service(rp);               /* request.c:978 → 15 */
              }
          }
          else if (rp->r_stop_tm > 0                 /* request.c:985 */
           && now - rp->r_stop_tm > 2*RS_DELTA_T
           && rp->r_pid > 0) {
              rp->r_stop_tm = 0;                     /* request.c:987 */
              crash_service(rp);                     /* request.c:988 → 15 */
          }
          else if (period > 0) {                     /* request.c:994 */
              if (rp->r_alive_tm < rp->r_check_tm) { /* request.c:1004 */
                  if (now - rp->r_alive_tm > 2*period &&   /* 1005 */
                      rp->r_pid > 0 &&
                      !(rp->r_flags & RS_NOPINGREPLY)) {   /* 1006 */
                      init_flag = rp->r_flags & RS_INITIALIZING;
                      rp->r_flags &= ~RS_INITIALIZING;
                      rp2 = lookup_slot_by_flags(RS_INITIALIZING); /* 1011-1013 */
                      rp->r_flags |= init_flag;
                      if(rp2 != NULL && !SRV_IS_UPDATING(rp)) {  /* 1015 */
                           rp->r_alive_tm = now;     /* 1020 — free pass */
                           rp->r_check_tm = now+1;   /* 1021 */
                           continue;                 /* 1022 */
                      }
                      rp->r_flags |= RS_NOPINGREPLY; /* 1024 */
                      crash_service(rp);             /* 1025 → 15 */
                      if(rp->r_flags & RS_INITIALIZING) { /* 1026 */
                          rp->r_init_err = EINTR;    /* 1027 */
                      }
                  }
              }
              else if (now - rp->r_check_tm > rp->r_period) { /* 1035 */
  		  ipc_notify(rpub->endpoint);        /* 1036 — ping */
		  rp->r_check_tm = now;              /* 1037 */
              }
          }
      }
  }
  if (OK != (s=sys_setalarm(RS_DELTA_T, 0)))        /* request.c:1044-1046 */
      panic("couldn't set alarm: %d", s);
}
```

三分支语义（**if/else if 链，互斥**）：

1. **`r_backoff > 0`**（request.c:975-978）：二进制退避计数。服务反复快速退出时 `terminate_service`（15）设 `r_backoff = MAX_BACKOFF`；每周期减 1，归零时 `restart_service`。归零前不做任何其他检查。
2. **`r_stop_tm > 0 && now - r_stop_tm > 2*RS_DELTA_T`**（request.c:985-989）：SIGTERM（`stop_service`，13 设置 `r_stop_tm`）发出后两个周期（2 秒）没退出 → `crash_service`（模拟崩溃，走自动重启路径）。`r_pid > 0` 要求进程还存在。
3. **心跳检查**（request.c:994-1038）：见 §2.3。

注意外层条件（request.c:963）：只有 **`RS_ACTIVE`** 且**非 update 中**（或 update 且处于 INITIALIZING）的槽才检查——update 期间的一般服务不参与心跳（避免与 LU 状态机竞争）。

### 2.2 周期计算与常量（request.c:965-969 + const.h:48-49,51,58,116）

```c
#define RS_INIT_T	(system_hz * 10)	/* 允许 init 10 秒 */      /* const.h:48 */
#define RS_DELTA_T	(system_hz)		/* 每 T 检查一次 */        /* const.h:49 */
#define MAX_BACKOFF      30                     /* 最大退避（RS_DELTA_T 单位）*/ /* const.h:51 */
#define RS_DEFAULT_PREPARE_MAXTIME 2*RS_DELTA_T /* 默认 prepare 上限 */          /* const.h:58 */
#define UPD_INIT_MAXTIME(RPUPD) ((RPUPD)->prepare_maxtime != RS_DEFAULT_PREPARE_MAXTIME \
                                 ? (RPUPD)->prepare_maxtime : RS_INIT_T)         /* const.h:116 */
```

- 正常服务：`period = r_period`（槽字段，08 的 `edit_slot` 从 `rss_period` 写入）。
- **INITIALIZING 服务**：`period = UPD_INIT_MAXTIME`（update 中）或 `RS_INIT_T`（普通初始化，10 秒）——初始化阶段给更长的宽限。`UPD_INIT_MAXTIME` 只有 `prepare_maxtime` 显式覆盖（≠ `RS_DEFAULT_PREPARE_MAXTIME`）时才用覆盖值，否则回落 `RS_INIT_T`（const.h:116）——默认给 LU 初始化 10 秒宽限，而非 2 秒。
- `MAX_BACKOFF = 30`：退避上限（30 秒）；`r_backoff` 的设定在 15。

### 2.3 心跳检查与 free pass（request.c:1004-1037）

```
alive_tm < check_tm？          ← ping 发出后无心跳回复
  ├─ 否（服务回复过）：到期再 ping
  │     now - check_tm > period → ipc_notify（ping）+ check_tm = now
  └─ 是（答案待定）：
        now - alive_tm > 2*period && pid > 0 && !NOPINGREPLY？
          ├─ 否 → 继续等
          └─ 是（超时）：
               另一个服务 INITIALIZING 且本服务非 update 中？
                  ├─ 是 → free pass：alive_tm = now, check_tm = now+1（跳过本轮）
                  └─ 否 → NOPINGREPLY 置位 + crash_service（模拟崩溃）
                           + 若 INITIALIZING → r_init_err = EINTR
```

**free pass 的动机**（注释 request.c:999-1003）：另一个服务正在初始化（重启）时，可能有"诡异依赖"（如重启中的服务短暂不可达），给超时服务一次免费通行证，避免误杀。free pass 只给一次（`check_tm = now+1` 保证下一周期重新判定）。

**`RS_NOPINGREPLY`**（const.h:31）：一旦判定超时，置位后不再发 ping（`!(r_flags & RS_NOPINGREPLY)` 条件，request.c:1006）——服务已被 crash_service 处置，后续由恢复路径接管。

### 2.4 `sys_setalarm` 重排（request.c:1044-1046）

`do_period` 末尾 `sys_setalarm(RS_DELTA_T, 0)` 重新调度下一个时钟 notify——主循环的 ClockNotify 由此**自驱动**（01 的 boot 末尾首次设置）。失败 panic。

### 2.5 `do_sigchld`（request.c:1051-1090）

```c
void do_sigchld()                                    /* request.c:1051 */
{
  while ( (pid = waitpid(-1, &status, WNOHANG)) != 0 ) { /* request.c:1063 */
      rp = lookup_slot_by_pid(pid);                  /* request.c:1064 */
      if(rp != NULL) {
          /* RS 不是该进程的信号管理器时也走到这里：清理实例 + 必要
           * 时补 late reply（RS_LATEREPLY，06 §2.2）。 */
          get_service_instances(rp, &rps, &nr_rps);  /* request.c:1077 */
          for(i=0;i<nr_rps;i++) {                    /* request.c:1078 */
              if(SRV_IS_UPDATING(rps[i])) {          /* request.c:1079 */
                  rps[i]->r_flags &= ~(RS_UPDATING|RS_PREPARE_DONE
                      |RS_INIT_DONE|RS_INIT_PENDING); /* request.c:1080 */
                  found = 1;
              }
              free_slot(rps[i]);                     /* request.c:1083 → 15 */
          }
          if(found) {
              rupdate_clear_upds();                  /* request.c:1086 → 16 */
          }
      }
  }
}
```

语义：PM 通知 RS 有死子进程（SIGCHLD，06 §2.6）。`waitpid(-1, WNOHANG)` 循环回收；按 pid 找槽；对该服务的**全部实例**（replica 链，A-3）：

1. 若实例在 update 中（`SRV_IS_UPDATING`，const.h:114），清 update 位（`UPDATING|PREPARE_DONE|INIT_DONE|INIT_PENDING`，request.c:1080）并标记 `found`；
2. `free_slot` 释放实例槽（→15）；
3. 若有 update 位被清 → `rupdate_clear_upds` 清理全局 update 链（→16）。

Rust 侧 `sigchld_cleanup(table, pid)`（monitor.rs）纯化第 2/3 步：`lookup_by_pid` → `instances_of` → 清位 + `free_slot`，返回 `SigchldOutcome.update_cleared` 供调用方决定 `rupdate_clear_upds`。

### 2.6 `update_period`（update.c:371-396，调用点豁免→16）

```c
void update_period(message *m_ptr)                   /* update.c:371 */
{
  rpupd = rupdate.curr_rpupd;                        /* update.c:381 */
  has_update_timed_out = (rpupd->prepare_maxtime > 0) &&  /* update.c:386 */
      (now - rpupd->prepare_tm > rpupd->prepare_maxtime);
  if(has_update_timed_out) {                         /* update.c:392 */
      printf("RS: update failed: maximum prepare time reached\n");
      end_update(EINTR, RS_CANCEL);                  /* update.c:394 → 16 */
  }
}
```

`do_period` 在 update 进行中调用它（request.c:953-955）：检查当前 update 的 prepare 阶段是否超时（`prepare_maxtime`，默认 `RS_DEFAULT_PREPARE_MAXTIME`=2 秒，const.h:58），超时则 `end_update(EINTR, RS_CANCEL)`——**调用点豁免**：`end_update` 的机制在 16，本文档只陈述触发条件。Rust 侧 `has_update_timed_out(now, prepare_tm, prepare_maxtime)`（monitor.rs）纯化该判定。

---

## 3. Rust 设计决策

### 3.1 monitor.rs 纯决策函数（D1）

```rust
pub enum PeriodAction {                              // request.c:975-1038 分支
    Nothing, BackoffTick, Restart, StopTimeoutCrash,
    PingRequest, PingTimeoutCrash, FreePass,
}
pub fn effective_period(rp, hz) -> i64               // request.c:965-969
pub fn upd_init_maxtime(hz, prepare_maxtime: Option<i64>) -> i64  // const.h:116；prepare_maxtime 未建模（16 DEFERRED）→ None = C 默认分支 RS_INIT_T
pub struct PeriodDecision { action: PeriodAction, mutations: SlotMutations }  // R13
pub fn period_decision(now, rp, hz,
    another_initializing: bool, is_updating: bool) -> PeriodDecision  // request.c:975-1038
pub fn heartbeat_mutations(timestamp: Clock) -> SlotMutations          // main.c:85-91 心跳写活标（R25）
pub fn has_update_timed_out(now, prepare_tm, prepare_maxtime) -> bool  // update.c:386
pub fn sigchld_cleanup(table, pid) -> Option<SigchldOutcome>         // request.c:1051-1090
```

设计差异：

- **决策与副作用分离**：C 的 `do_period` 在判定同时执行 `restart_service`/`crash_service`/`ipc_notify`；Rust 的 `period_decision` 只返回 `PeriodDecision { action, mutations }`，动作副作用由调用方（未来主循环集成，06/15/19）执行，**槽位变异**（`r_backoff -= 1`、`r_stop_tm = 0`、`r_check_tm = now`、`r_alive_tm/check_tm` 的 free pass、`NOPINGREPLY` + `r_init_err = EINTR`）以 `SlotMutations` 载荷显式携带（R13）——调用方 `mutations.apply(rp)` 一次提交，漏变异从"注释约定"变成编译期缺口（对照 Redox 变异权 token 风格）。
- **`another_initializing`/`is_updating` 参数注入**：C 靠全局表查询（`lookup_slot_by_flags(RS_INITIALIZING)`，request.c:1013）与 `SRV_IS_UPDATING`（const.h:114）——Rust 把这两个判定结果作为布尔参数传入，`period_decision` 不触表（保持纯函数）。
- **常量族函数化**：`RS_INIT_T`/`RS_DELTA_T`/`RS_DEFAULT_PREPARE_MAXTIME` 依赖运行时 `system_hz`（GET_HZ，01/19）→ `init_timeout(hz)`/`delta_t(hz)`/`default_prepare_maxtime(hz)`；`UPD_INIT_MAXTIME` → `upd_init_maxtime(hz, prepare_maxtime: Option<i64>)`（覆盖值 ≠ 默认才生效，否则 `RS_INIT_T`——`prepare_maxtime` 未建模（16 DEFERRED），当前传 `None`）；`MAX_BACKOFF` 是编译期常量 30。
- **心跳写活标是独立决策（R25，2026-09-06）**：`r_alive_tm` 的常规写入路径不在 `do_period` 里，而在主循环的 notify 分支——C 是 main.c:87 `rproc_ptr[who_p]->r_alive_tm = m.m_notify.timestamp`（timestamp 由内核填，ipc.h:1715，每个 notify 都有效）。Rust 建模为 `heartbeat_mutations(timestamp) -> SlotMutations`（只带 `alive_tm` 一个字段的载荷），与 `period_decision` 同一种"决策返回载荷、调用方 apply"模式；NULL 槽告警分支（main.c:89-90"unexpected notify"）留在调用方——表查询归它。分类结果 `HeartbeatNotify { source, timestamp }`（06 §3）自带 timestamp，handler 无需回看消息。

> **N2 修复（2026-08-16，todo §11）**：`period_decision` 的"period 到期 → ping"分支
> （request.c:1035-1037）必须比较**原始 `rp->r_period`**，而不是 `effective_period` 的结果。
> 旧实现误用有效 period：对正在初始化的 boot 槽（`r_period=0`，main.c:338），C 语义是
> 每个 `RS_DELTA_T` 都 ping 一次（`now - r_check_tm > 0` 恒真），旧 Rust 版要等
> `RS_INIT_T`（10 秒）才 ping 一次，节律差 10 倍。`effective_period` 现在只用于
> "answer pending 超时"（2×period，request.c:1004-1006）与 `period==0` 门
> （request.c:972），与 C 逐分支对齐。测试：`test_initializing_zero_period_pings_every_tick`
> 锁死该语义（INITIALIZING + r_period=0 → 每 tick `PingRequest`；非初始化 period-0 → `Nothing`）。

### 3.2 时间类型（D4）

`Clock = i64`（minix-types），所有时间运算用 `i64` + `saturating_sub`（防下溢——C 的 `now - alive_tm` 无符号语义，Rust 显式饱和）。`r_pid` 用 `Option<Pid>`（`None` = -1），**R19**：C 条件是严格的 `r_pid > 0`（request.c:987/1007），Rust 用 `pid.is_some_and(|p| p > 0)`——`Some(0)`（`getnpid` 落槽异常值）按"无进程" fail-closed，不触发 crash 路径。

### 3.3 sigchld 纯表操作（D3）

`sigchld_cleanup` 把 C 的"waitpid 循环 + 表操作"拆开：waitpid（19 接线）由调用方循环，本函数只做 `lookup_by_pid → instances_of → 清位 → free_slot`（request.c:1064-1083）。`free_slot` 的表级不变量在 02 §3.6；`rupdate_clear_upds` 归 16。

---

## 4. 实现详解（monitor.rs）

模块结构（已实现，181 tests 总盘中 monitor 9 个）：

```
monitor.rs
├─ init_timeout/delta_t/default_prepare_maxtime（const.h:48-49,58）
├─ MAX_BACKOFF（const.h:51）
├─ PeriodAction 七变体（request.c:975-1038 分支）
├─ effective_period（request.c:965-969）
├─ period_decision（request.c:975-1038）
│   ├─ backoff 分支（Restart/BackoffTick）
│   ├─ stop_tm 超时分支（StopTimeoutCrash）
│   ├─ ping 待答分支（PingTimeoutCrash/FreePass）
│   └─ period 到期分支（PingRequest）
├─ has_update_timed_out（update.c:385-386）
├─ sigchld_cleanup（request.c:1064-1083）
└─ #[cfg(test)] 9 个测试（§5）
```

关键不变量：

1. **分支互斥**：if/else if 链（request.c:975-1038）在 `PeriodAction` 上体现为单值返回——一个槽一次决策。
2. **free pass 只救一次**：`FreePass` 的 `mutations.alive_tm = now; check_tm = now+1`（request.c:1020-1021，R13）由载荷携带，调用方 `apply` 即完成。
3. **NOPINGREPLY 终态**：`PingTimeoutCrash` 的 `mutations.set` 携带 `NOPINGREPLY`（request.c:1024；初始化中另有 `init_err = EINTR`，request.c:1027-1028），`apply` 后后续 `period_decision` 对该槽返回 `Nothing`（条件 `!NOPINGREPLY`，request.c:1006）——测试断言该阻断。
4. **update 位清理集合固定**：`UPDATING|PREPARE_DONE|INIT_DONE|INIT_PENDING` 四位的清理由 `sigchld_cleanup` 保证与 C 一致（request.c:1080）。

---

## 5. 测试要点

`cargo test -p minix-rs --lib`（208 passed，monitor 相关 12 个）：

| 测试 | 覆盖 |
|------|------|
| `test_effective_period_normal` | 正常服务用 `r_period` |
| `test_effective_period_initializing` | INITIALIZING → RS_INIT_T；INITIALIZING+UPDATING → RS_INIT_T（UPD_INIT_MAXTIME 默认分支） |
| `test_upd_init_maxtime` | None → RS_INIT_T；显式覆盖（≠ 默认）生效；== 默认回落 RS_INIT_T |
| `test_backoff_restart` | backoff 递减（BackoffTick）→ 归零 Restart |
| `test_stop_timeout` | SIGTERM 超 2×RS_DELTA_T → StopTimeoutCrash；窗口内 Nothing |
| `test_ping_timeout_with_free_pass` | 超时 + 他人初始化 → FreePass；无他人 → PingTimeoutCrash；update 中无 free pass |
| `test_ping_request` | period 到期 → PingRequest；未到期 Nothing |
| `test_initializing_zero_period_pings_every_tick`（N2） | 初始化 + `r_period=0`（boot 槽 main.c:338）→ 每 tick Ping（request.c:1035 用原始 period 判定）；非初始化 period-0 → 无 ping（request.c:972 `period > 0` gate） |
| `test_zero_pid_is_no_process` | `Some(0)` 不触发 stop/ping 超时 crash（R19） |
| `test_nopingreply_blocks_crash` | NOPINGREPLY 阻断重复 crash |
| `test_update_timeout` | prepare 超时判定 + maxtime=0 不超时 |
| `test_heartbeat_mutations_refresh_alive_tm`（R25） | 心跳载荷只写 `alive_tm`（main.c:85-91），槽位其余字段不动 |
| `test_sigchld_cleanup` | 实例链释放 + update_cleared + 槽位清空 |

---

## 6. 过渡：从"监控"到"配置"

心跳监控是**运行时**的第一块机制：它由主循环的 ClockNotify（06）驱动，决策交给 15 的恢复路径。但心跳监控依赖一个前提——**服务有正确的 `r_period` 配置**（`period > 0` 才检查，request.c:994）。这个配置来自 08-slot-config：`edit_slot` 从 `rs_start->rss_period` 写入 `r_period`（manager.c:1684-1687）。

下一篇 `08-rs-slot-config.md` 回到**服务创建路径**：`rs_start` 请求参数如何校验（`check_request`）、如何拷入槽位（`copy_rs_start`/`copy_label`）、`init_slot`/`edit_slot` 如何落地全部配置字段（含 `r_period`/`r_ipc_list`/call masks/control labels）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` — 周期/心跳字段与 `lookup_*`/`instances_of`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md` — ClockNotify 分支、心跳时间戳写入、SIGCHLD 入口
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md` — `r_period` 配置来源
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md` — `clone_service`（rs_idle_period 补 replica）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/13-rs-control-requests.md` — `stop_service` 设置 `r_stop_tm`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/15-rs-terminate-restart.md` — restart/crash/cleanup 机制
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` — `end_update`/`rupdate_clear_upds`/`update_period` 消费点
- `minix3/minix/servers/rs/request.c:943-1090`、`update.c:371-396`、`const.h:31,34-35,39,48-51,58,110,114-116` — ground truth
- `os/servers/rs/src/monitor.rs` — Rust 实现
