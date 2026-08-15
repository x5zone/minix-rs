# 13-rs-control-requests: 控制请求

> **分类**: 阶段 4 — 服务生命周期（控制面）
> **源码**: `minix3/minix/servers/rs/request.c`（`do_up`—15、`do_down`—111、`do_restart`—160、`do_clone`—208、`do_unclone`—253、`do_edit`—298、`do_refresh`—390、`do_shutdown`—431）、`minix3/minix/servers/rs/manager.c:988-1008`（`stop_service`）
> **Rust 模块**: `os/servers/rs/src/request.rs`（`up_init_flags`/`check_duplicates`/`mark_late_reply`/`StopSignal`/`stop_service`/`shutdown_apply`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/04-rs-access-control.md`（`check_call_permission`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md`（`copy_rs_start`/`check_request`/`init_slot`/`edit_slot`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md`（`clone_service`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md`（`start_service`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md`（`RS_LATEREPLY`）
> **说明**: 本文档是 RS 的**控制面分派层**：主循环（06）把 `RS_UP/DOWN/RESTART/CLONE/UNCLONE/EDIT/REFRESH/SHUTDOWN` 八类消息分派到对应 handler（`RS_UPDATE` 归 16）。每个 handler 都是"校验 → 调机制"的薄层；机制本身（创建/发布/运行/清理/更新）在 10/11/12/15/16。同时本文档绘制**服务生命周期次主线路径图**（plan §1.3）。

---

## 1. 概念：外部如何操作服务的生命周期

### 1.0 章节引言

前 12 篇回答了"RS 如何启动一个服务"；从本文档开始回答"**外部如何控制一个服务的一生**"。控制请求是唯一入口：`RS_UP` 诞生服务、`RS_DOWN` 停止、`RS_RESTART` 复活、`RS_EDIT` 改配置、`RS_CLONE`/`RS_UNCLONE` 增删副本、`RS_REFRESH` 重启并保留配置、`RS_SHUTDOWN` 全系统停机。本文档回答的问题是：**八个 handler 的公共骨架与各自校验是什么、停止原语如何被复用、生命周期全景长什么样**。

> **本章不讲什么**（机制一律移交）:
> - 访问控制判定（`04-rs-access-control.md`）——handler 只调用 `check_call_permission`
> - `copy_rs_start`/`copy_label`/`init_slot`/`edit_slot` 的消息面（`08-rs-slot-config.md`）
> - `start_service`/`run_service`（`12-rs-init-run.md`）、`clone_service`（`10-rs-service-create.md`）
> - `cleanup_service`/`cleanup_service_now`/`restart_service`/`crash_service`（`15-rs-terminate-restart.md`）
> - `do_update`/`do_upd_ready`（`16-rs-live-update.md`）
> - `reply`/`late_reply`/`EDONTREPLY`（`06-rs-main-loop.md`）

### 1.1 公共骨架（WHY）

八个 handler 共享同一个骨架：

```
① copy_label/copy_rs_start（08 消息面）── 从调用者地址空间拷参数
② lookup_slot_by_label（02）── 按 label 找槽
③ check_call_permission（04）── 调用者是否有权
④ 特有校验（EBUSY/EEXIST/ENOENT/TERMINATED 门…）
⑤ 调机制（start_service/stop_service/clone_service/edit_slot…）
⑥ 回包（OK / EDONTREPLY + RS_LATEREPLY）
```

这个骨架是"校验 → 动作"的经典命令模式：handler 不拥有机制，只做仲裁。Rust 侧对应：**纯校验**（`up_init_flags`/`check_duplicates`/`mark_late_reply`/`stop_service`/`shutdown_apply`）进 `request.rs`，动作挂接点留在调用方（19 组装）。

### 1.2 服务生命周期次主线路径图（WHAT）

```
                    ┌──────────────────────────────────────────────────┐
                    │                服务的一生（次主线）               │
                    └──────────────────────────────────────────────────┘
  RS_UP（主循环分派）── 13
    ├─ 04 访问控制：check_call_permission
    ├─ 08 slot 配置：alloc_slot + init_slot（含重复检查）
    ├─ 10 创建：start_service → create_service（fork/priv/sched/exec/VM）
    ├─ 11 发布：publish_service（DS/VFS/PCI/devman）
    ├─ 12 运行：run_service → RS_INIT 握手
    ├─ 07 监控：do_period 心跳/超时（r_alive_tm/r_check_tm/r_stop_tm）
    ├─ 13 停止：RS_DOWN/RS_REFRESH → stop_service（SIGTERM/SIGHUP → r_stop_tm）
    │         └─ 信号未处理 → 07/15 的 SIGKILL 升级
    ├─ 15 终止：terminate_service（restart/backoff/cleanup 分支）
    │         ├─ RS_DEAD 清理 + reincarnate
    │         └─ 恢复脚本（RS_RESTART 回环到 13）
    ├─ 16 热升级：RS_UPDATE → Live Update（prepare→update→init→end）
    └─ 13 RS_SHUTDOWN：全表 RS_EXITING（停机，禁止重启）
```

每个控制请求都对应路径图上的一个箭头：`RS_UP` 是起点，`RS_DOWN`/`RS_REFRESH` 是停止，`RS_RESTART` 是 15 的恢复回环，`RS_UPDATE` 是 16 的旁路。

---

## 2. C 源码分析

### 2.1 do_up：创建入口（request.c:15-106）

`do_up` 是"启动新服务"的完整流程：

1. `check_call_permission(m_source, RS_UP, NULL)`（request.c:26-28）——目标槽尚不存在，只查调用者身份（04）；
2. `alloc_slot`（request.c:30-34）——失败 `ENOMEM`；
3. `copy_rs_start`（request.c:38-42）——从调用者地址空间拷贝 `rs_start`（08）；
4. `check_request`（request.c:43-46）——参数校验（08）；
5. **init_flags 映射**（request.c:50-61）：`RSS_FORCE_INIT_CRASH/FAIL/TIMEOUT/DEFCB`（minix3/minix/include/minix/rs.h:42-45，调试钩子）→ `SEF_INIT_CRASH/FAIL/TIMEOUT/DEFCB`（sef.h:98-101），随 RS_INIT 消息传给服务（12）；
6. `init_slot`（request.c:63-68）——槽位落地（08）；
7. **重复检查**（request.c:70-87）：label 重 → `EBUSY`；`dev_nr > 0` 且重 → `EBUSY`；任一 domain 重 → `EBUSY`；
8. `start_service`（request.c:89-93）——10/11/12 的编排；
9. **回包双模式**（request.c:95-105）：
   - `RSS_NOBLOCK` → 立即 `return OK`（调用者不等初始化）；
   - 否则 `r_flags |= RS_LATEREPLY; r_caller = m_source; r_caller_request = RS_UP` + `return EDONTREPLY`——**初始化完成后补发 reply**（12 的 `end_srv_init` → `late_reply`，06）。

### 2.2 do_down：停止入口（request.c:111-155）

1. `copy_label` → `lookup_slot_by_label`（不存在 → `ESRCH`）；
2. `check_call_permission(m_source, RS_DOWN, rp)`（04）；
3. **TERMINATED 分支**（request.c:137-146）：服务已死（恢复脚本正在执行 RS_DOWN）→ `unpublish_service` + `cleanup_service` + `return OK`——**立即完成清理，不等退出**；
4. 否则 `stop_service(rp, RS_EXITING)`（manager.c:988-1008，见 2.7）+ `RS_LATEREPLY` 记录 + `EDONTREPLY`——**服务退出后补发 reply**（15 的 terminate 路径）。

### 2.3 do_restart：恢复脚本入口（request.c:160-203）

`do_restart` **只允许恢复脚本调用**（`RS_RESTART` 权限 + 前提）：

1. `copy_label` → `lookup_slot_by_label`（`ESRCH`）→ 权限检查；
2. **TERMINATED 门**（request.c:186-191）：服务还在运行 → `EBUSY`（"We can only be asked to restart a service from a recovery script"）；
3. **script 保护**（request.c:196-200）：把 `r_script` 存到局部变量、清空槽位、调 `restart_service`（15）、再恢复——**防止重启脚本递归调用自己**。

### 2.4 do_clone / do_unclone（request.c:208-293）

- **do_clone**（request.c:208-248）：权限检查后，`r_next_rp` 已存在 → `EEXIST`（"Don't clone if a replica is already available"）；否则 `rpub->sys_flags |= SF_USE_REPL` + `clone_service(rp, RST_SYS_PROC, 0)`（10）——失败回滚 `SF_USE_REPL`。
- **do_unclone**（request.c:253-293）：`!(sys_flags & SF_USE_REPL)` → `ENOENT`；否则清 `SF_USE_REPL`，有 `r_next_rp` 则 `cleanup_service_now(r_next_rp)` + 断链。

`SF_USE_REPL` 是"副本策略"开关：置位后 RS 会在重启/编辑时自动重建副本（do_edit 的 2.5 第 8 步也读它）。

### 2.5 do_edit：运行时重配置（request.c:298-385）

`do_edit` 是 8 步重配置（服务不停机；第 8 步 replica 重建仅在 `SF_USE_REPL` 时发生）：

1. `copy_rs_start` + `copy_label`（request.c:306-317）——`rs_start` 里带 label（`rss_label`）；
2. `lookup_slot_by_label` + 权限检查（request.c:319-330）；
3. `sys_getpriv(&rp->r_priv, endpoint)`（request.c:335-339）——同步内核 priv；
4. `sched_stop(r_scheduler, endpoint)`（request.c:341-345）——让调度器放手；
5. `edit_slot(rp, &rs_start, m_source)`（request.c:347-351）——按 `rs_start` 改槽位（08，含 `RSS_COPY`/`RSS_REUSE` 分支）；
6. `sys_privctl(endpoint, SYS_PRIV_UPDATE_SYS, &rp->r_priv)`（request.c:353-358）+ `vm_set_priv(endpoint, &vm_call_mask[0], !!(s_flags & SYS_PROC))`（request.c:360-365）——提交新 priv/VM 掩码；
7. `sched_init_proc(rp)`（request.c:367-371）——重新初始化调度；
8. **replica 重建**（request.c:373-382）：`SF_USE_REPL` 时清理旧 `r_next_rp` + `clone_service(rp, RST_SYS_PROC, 0)`（失败只警告，不中断）。

注意第 4 步的 `sched_stop` 与第 7 步的 `sched_init_proc` 成对：编辑期间服务暂时脱离调度器，改完重新接入。

### 2.6 do_refresh / do_shutdown（request.c:390-457）

- **do_refresh**（request.c:390-426）：`stop_service(rp, RS_REFRESHING)`（不是 `RS_EXITING`！）+ `RS_LATEREPLY`（request = `RS_REFRESH`）+ `EDONTREPLY`。`RS_REFRESHING` 让 15 的退出处理走"刷新"路径（旧实例清理 + 新实例以相同配置重启）。
- **do_shutdown**（request.c:431-457）：`check_call_permission(RS_SHUTDOWN, NULL)`（`m_ptr == NULL` 时跳过——内核触发）；`shutting_down = TRUE`（glo.h:51）；**全表扫描**：所有 `RS_IN_USE` 槽 `r_flags |= RS_EXITING`（request.c:449-455）——停机后 15 的重启逻辑（`shutting_down` 检查）不会再复活服务。

### 2.7 stop_service：停止原语（manager.c:988-1008）

`stop_service(rp, how)` 是 do_down/do_refresh 共用的停止原语：

1. **信号选择**（manager.c:1003）：`endpoint != RS_PROC_NR ? SIGTERM : SIGHUP`——普通服务友好信号 SIGTERM；**RS 自己用 SIGHUP**（RS 的 SEF 信号处理器把 SIGHUP 当作停止请求，06/18）；
2. `r_flags |= how`（manager.c:1005）——`RS_EXITING` 或 `RS_REFRESHING`（"退出后做什么"）；
3. `sys_kill(endpoint, signo)`（manager.c:1006）——先友好信号；
4. `r_stop_tm = getticks()`（manager.c:1007）——**记录时间**：07 的心跳检查据此判定"给了 SIGTERM 但没退出 → SIGKILL 升级"。

---

## 3. Rust 设计决策

### 3.1 request.rs 纯切片

八个 handler 的 IPC 面（`copy_rs_start`/`copy_label`/`sys_kill`/`sys_getpriv`/`sched_stop`/`sys_privctl`/`vm_set_priv`/`sched_init_proc`）归 08/19；`request.rs` 拥有**纯校验与簿记**：

| 函数 | C 对应 | 语义 |
|------|--------|------|
| `up_init_flags(rss)` | request.c:50-61 | `RSS_FORCE_INIT_*` → `SEF_INIT_*` 位映射 |
| `check_duplicates(table, label, dev_nr, domains)` | request.c:70-87 | label/dev_nr/domain 三查 → `EBUSY` |
| `mark_late_reply(slot, caller, request)` | request.c:101-103 等 | `RS_LATEREPLY` 三字段一致写 |
| `StopSignal` + `stop_service(table, rp, how, ticks)` | manager.c:988-1008 | 信号选择 + 标志/计时器；`sys_kill` 由调用方发 |
| `shutdown_apply(table)` | request.c:447-455 | 全表 `RS_EXITING` + `true`（shutting_down） |

### 3.2 StopSignal 建模

C 用 `SIGTERM`/`SIGHUP` 宏（libc 信号号 15/1）；Rust `StopSignal` 枚举（`Term`/`Hangup`）+ `as_i32()`，`sys_kill` 面（19）消费。选择逻辑纯化：`endpoint == RS → Hangup`（manager.c:1003）。

### 3.3 handler 骨架的公共化

C 的八个 handler 重复"copy → lookup → 权限 → 动作"。Rust 侧不强制共享骨架（避免过度抽象），而是把**纯校验**集中到 `request.rs`，让每个 handler 组装时只写"调哪个机制"。`mark_late_reply` 保证 `RS_LATEREPLY` 三字段（flags/caller/caller_request）一致写，06 的 `late_reply` 消费时不会读到半更新状态。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/rs/src/request.rs` 函数表见 §3.1；`SEF_INIT_*` 四个常量（sef.h:98-101）一并建模。

### 4.2 关键不变量

1. **LATEREPLY 三字段一致写**：`mark_late_reply` 一次写齐 `flags/caller/caller_request`，避免 06 读到半状态。
2. **`stop_service` 只做槽位侧 + 返回信号**：`sys_kill` 由调用方发送；信号选择（RS→SIGHUP）是纯函数。
3. **`do_up` 的重复检查在 `start_service` 之前**（request.c:70-90）：重复服务永远不进创建路径。
4. **`RS_REFRESHING` vs `RS_EXITING` 是两条不同的退出意图**（manager.c:1005）：15 据此分派刷新/终止路径。
5. **`shutdown_apply` 后服务不再被重启**：全表 `RS_EXITING` + `shutting_down`（15 检查）。

---

## 5. 测试要点

`request.rs` 内 6 项测试（`cargo test -p minix-rs --lib request` 过滤子串会命中其他模块的 `*request` 测试，共 15 通过；以 `request::tests` 6 项为准）：

1. `up_init_flags`：四个 `RSS_FORCE_INIT_*` 独立映射到 `SEF_INIT_*`。
2. `check_duplicates`（拆两个测试函数）：label 命中 → `EBUSY`；dev_nr（仅 >0）/ domain 命中 → `EBUSY`。
3. `mark_late_reply`：三字段一次写齐。
4. `stop_service`：RS endpoint → `Hangup`；其他 → `Term`；`how` 置位；`stop_tm` 记录。
5. `shutdown_apply`：全表 IN_USE 槽 `EXITING`；返回 `true`。

测试总数声明：本文档范围为 **6 项**（`request` 模块内）。全局 `cargo test -p minix-rs --lib` = 181 通过（随并行模块增长，以各 doc 范围为准）。

---

## 6. 过渡

控制请求把服务推入生命周期的不同阶段：

- **停止路径**：`RS_DOWN`/`RS_REFRESH` → `stop_service` 之后，服务的命运交给 **15-rs-terminate-restart**（SIGTERM 未处理 → SIGKILL 升级 → `terminate_service` 的 restart/backoff/cleanup 分支）；
- **恢复路径**：`RS_RESTART`（恢复脚本）与 `RS_REINCARNATE` 都汇聚到 15 的 `restart_service`；
- **更新路径**：`RS_UPDATE`（16）绕过停止/重启，走 Live Update 状态机；`do_edit` 的 replica 重建与 `do_clone` 为更新预置副本。
- **停机**：`RS_SHUTDOWN` 之后 15 的重启逻辑失效，系统进入有序停机。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/04-rs-access-control.md` —— 全部 handler 的权限门
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md` —— `copy_rs_start`/`copy_label`/`init_slot`/`edit_slot`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md` —— `start_service`/`end_srv_init`/`late_reply` 消费点
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/07-rs-period-heartbeat.md` —— `r_stop_tm` 与 SIGKILL 升级
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/15-rs-terminate-restart.md` —— `cleanup_service`/`restart_service`/`crash_service`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— `RS_UPDATE`/`do_upd_ready`
- `minix3/minix/servers/rs/request.c:15-457`、`manager.c:988-1008` —— ground truth
- `minix3/minix/include/minix/sef.h:98-101` —— `SEF_INIT_*` 调试标志
