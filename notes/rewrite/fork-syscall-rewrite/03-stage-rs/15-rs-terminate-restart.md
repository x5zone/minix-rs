# 15-rs-terminate-restart: 服务终止与恢复

> **分类**: 阶段 5 — 终止与恢复（生命周期状态机的终局处理）
> **源码**: `minix3/minix/servers/rs/manager.c`（`kill_service_debug`—360、`crash_service_debug`—380、`cleanup_service_debug`—405、`detach_service_debug`—497、`reincarnate_service`—1033、`terminate_service`—1055、`run_script`—1185、`restart_service`—1246、`get_service_instances`—1334）、`minix3/minix/servers/rs/proto.h:44-59`（`_debug` 宏）、`minix3/minix/servers/rs/const.h:25,50-51`
> **Rust 模块**: `os/servers/rs/src/recovery.rs`（`TerminateAction`/`TerminateDecision`/`terminate_decision`/`compute_backoff`/`script_reason`/`late_reply_result`/`CleanupDecision`/`cleanup_decision`/`MAX_DET_RESTART`/`BACKOFF_BITS`/`MAX_BACKOFF`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md`（`run_service`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md`（`clone_service`/`update_service`/`swap_slot`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/11-rs-publish.md`（`unpublish_service`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md`（`late_reply`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md`（`end_update`/`abort_update_proc`）
> **说明**: 本文档是**终止与恢复状态机**：13 的控制请求把服务推入停止路径，07 的心跳超时/SIGKILL 升级也汇聚到这里。`terminate_service`（manager.c:1055）是 RS 最密集的决策树之一——它决定一个死掉的服务是回滚、清理、刷新、backoff 重试还是直接重启。本文档同时绘制**终止/恢复状态机图**（plan §1.3 次主线的终局段）。

---

## 1. 概念：一个服务的"终局"

### 1.0 章节引言

一个服务从 `RS_UP` 诞生（13）→ 创建/发布/运行（10/11/12）→ 心跳监控（07）→ 被 `RS_DOWN`/`RS_REFRESH`/崩溃/超时杀死——然后呢？本文档回答：**退出事件发生后，RS 如何决定这个服务的命运**。`terminate_service` 是决策树，`cleanup_service` 是清理执行器，`restart_service`/`reincarnate_service`/`run_script` 是恢复执行器，`detach_service` 是"降级为普通服务"的旁路。

> **本章不讲什么**（机制一律移交）:
> - 控制请求（`13-rs-control-requests.md`）——`stop_service`/`RS_DOWN`/`RS_REFRESH` 的入口
> - 心跳与 SIGKILL 升级（`07-rs-period-heartbeat.md`）——`r_stop_tm` 超时后的 kill 路径
> - `end_update`/`abort_update_proc`/`SRV_IS_UPD_SCHEDULED` 分支（`16-rs-live-update.md`）——本文档只陈述调用点
> - `clone_service`/`update_service`/`swap_slot`（`10-rs-service-create.md`）、`run_service`（`12-rs-init-run.md`）、`unpublish_service`（`11-rs-publish.md`）
> - `sys_kill`/`sched_stop`/`srv_kill`/`sys_privctl`/`vm_set_priv`/`vm_memctl`/`ds_publish_label`（`19-rs-external-interfaces.md`）
> - RS 自身的 rollback 特例（`18-rs-self-lifecycle.md`）

### 1.1 终局处理（WHY）

服务退出后，RS 必须回答三个问题：

1. **为什么退出**——初始化失败？被显式停止？心跳超时？崩溃？更新中退出？（由 `r_flags` 的 `RS_INITIALIZING`/`RS_EXITING`/`RS_REFRESHING`/`RS_NOPINGREPLY`/`RS_UPDATING` 位携带）
2. **要不要重启**——`SF_NORESTART` 拒绝重启；`SF_CORE_SRV` 核心服务死亡直接 `_exit(1)`（RS 陪葬）；否则看 restarts 计数决定 backoff 还是立即重启；
3. **怎么清理**——脚本？降级 detach？释放槽位？重发 late reply？

`terminate_service` 用一株 30+ 分支的决策树回答这些问题。Rust 侧把它提炼为**纯决策函数**（`terminate_decision`）：输入槽位状态与策略标志，输出"做什么动作 + 要置哪些位"，动作的执行（IPC、fork、清理）全部留在调用方——这是 13/14 同款的"纯切片"边界。

### 1.2 终止/恢复状态机图（WHAT）

```
                       ┌──────────────────────────────────────────────┐
                       │           终止 / 恢复状态机（终局段）         │
                       └──────────────────────────────────────────────┘
  退出事件（崩溃/信号/超时/RS_DOWN/RS_FI 注入）
    │
    ▼
  terminate_service（manager.c:1055）── 决策树
    ├─ init 失败 + 更新中 → InitUpdateRollback（end_update → 16）
    ├─ init 失败 + SF_NO_BIN_EXP → Refresh（RS_REFRESHING → restart_service）
    ├─ init 失败（其他）→ CleanupAll（RS_EXITING）
    ├─ norestart（SF_NORESTART）→ CleanupAll
    │     ├─ SF_DET_RESTART 且 restarts<10 → 清理时 detach
    │     └─ 有脚本 → 清理时跑脚本
    ├─ RS_EXITING → CleanupAll
    │     ├─ SF_CORE_SRV 且非停机 → RS 自己 _exit(1)
    │     ├─ 更新调度中 → abort_update_proc（16）
    │     ├─ late_reply（OK/EDEADEPT）
    │     ├─ unpublish（11）+ 逐实例 cleanup_service
    │     └─ RS_REINCARNATE → reincarnate_service（clone_slot + start_service）
    ├─ RS_REFRESHING → restart_service（刷新路径）
    └─ 意外退出
          ├─ restarts>0 → Backoff（1<<MIN(restarts,62) 封顶 30）
          └─ 首次 → restart_service
                 ├─ 有脚本 → run_script（fork+execle sh ← ARCH A-1 缺口）
                 └─ 无脚本 → clone_service → update_service(RS_SWAP) → run_service
  cleanup_service（manager.c:405）── 两段式
    ├─ 第一段（RS_DEAD 未置）：断链 + RS_DEAD + DISALLOW/CLEAR_IPC_REFS + ~ACTIVE + late_reply(OK)
    └─ 第二段：sched_stop + srv_kill（非 detach）
                ├─ CLEANUP_SCRIPT → run_script
                ├─ CLEANUP_DETACH → detach_service（唯一 label 重发布 + 降权）
                └─ 否则 free_slot（REINCARNATE 除外）
```

`RS_DOWN`（13）把服务置 `RS_EXITING`，`RS_REFRESH` 置 `RS_REFRESHING`，心跳超时（07）置 `RS_NOPINGREPLY`——三者在决策树上走不同的分支，但都汇聚到 `cleanup_service`/`restart_service` 这两个执行器。

---

## 2. C 源码分析

### 2.1 terminate_service：决策树（manager.c:1055-1180）

`terminate_service` 是终止处理的唯一入口。按顺序：

1. **初始化失败分支**（manager.c:1069-1092）：
   - `SRV_IS_UPDATING(rp)`（const.h:114：`r_flags & RS_UPDATING`）→ 更新中初始化失败 = 状态迁移失败 → `end_update(r_init_err, RS_REPLY)`（1071-1076，16）+ `r_init_err = ERESTART` + **提前 return**；
   - `SF_NO_BIN_EXP` → `RS_REFRESHING`（1084-1086，"当作刷新处理"）；
   - 其他 init 失败 → `RS_EXITING`（1090-1091，"不重启"）。
   - **注意 C 的 fall-through**：后两个分支只**置位**，然后继续往下走主决策树（1078-1091 没有 return）。
2. **全局更新中止**（1100-1104）：`RUPDATE_IS_UPDATING()` → `abort_update_proc(ERESTART)`（16）——恢复动作前先中止进行中的更新（避免与更新服务产生依赖纠缠）。
3. **norestart 检测**（1105-1117）：`norestart = !(RS_EXITING) && SF_NORESTART`；置 `RS_EXITING`；`SF_DET_RESTART && r_restarts < MAX_DET_RESTART`（const.h:25）→ `RS_CLEANUP_DETACH`（1108-1111）；`r_script[0] != '\0'` → `RS_CLEANUP_SCRIPT`（1114-1115）。
4. **RS_EXITING 分支**（1119-1152）：
   - `SF_CORE_SRV && !shutting_down` → **RS 自己 `_exit(1)`**（1121-1123）——核心服务死亡，整个系统无法继续；
   - `SRV_IS_UPD_SCHEDULED(rp)`（const.h:120，读 `r_upd`）→ `abort_update_proc(EDEADSRCDST)`（1127-1129，16）；
   - **late reply 结果**（1134-1135）：`r_caller_request == RS_DOWN` 或（`RS_REFRESH && norestart`）→ `OK`，否则 `EDEADEPT`（13 的 `RS_DOWN`/`RS_REFRESH` 调用者等这个回复）；
   - `unpublish_service(rp)`（1138，11）；
   - `get_service_instances(rp, &rps, &nr_rps)`（1141，2.7）+ 逐实例 `cleanup_service`（1143）；
   - `RS_REINCARNATE` → 清位 + `reincarnate_service(rp)`（1150-1151，2.5）。
5. **RS_REFRESHING 分支**（1155-1157）：`restart_service(rp)`——刷新 = 以相同配置重启。
6. **意外退出分支**（1164-1178）：
   - `r_restarts > 0` → backoff（1165-1172）：`r_backoff = 1 << MIN(restarts, BACKOFF_BITS-2)`（`BACKOFF_BITS` = `sizeof(long)*8` = 64，const.h:50）→ 封顶 `MAX_BACKOFF`（30，const.h:51）→ `SF_USE_COPY && backoff > 1` 折叠为 1（镜像在内存里，重启便宜）→ `SF_NO_BIN_EXP` 直接 1；
   - 首次退出 → `restart_service(rp)`（1177-1178）。

### 2.2 cleanup_service：两段式清理（manager.c:405-491）

C 里是 `cleanup_service` 宏（proto.h:51-52）→ `cleanup_service_debug(file, line, rp)`（manager.c:405）。**两段式**由 `RS_DEAD` 位区分（`cleanup_service_now` 宏 proto.h:53-54 就是连调两次强制跑完两段）：

**第一段（RS_DEAD 未置，manager.c:416-446）——"标记死亡 + 解耦"**：
1. 断四链（`r_next_rp`/`r_prev_rp`/`r_new_rp`/`r_old_rp`，422-436）——先摘除实例链；
2. `r_flags |= RS_DEAD`（438）——**下次进入直接走第二段**；
3. `sys_privctl(endpoint, SYS_PRIV_DISALLOW)` + `SYS_PRIV_CLEAR_IPC_REFS`（441-442）——内核侧禁止运行 + 清 IPC 引用；
4. `r_flags &= ~RS_ACTIVE`（443）——不再是活动实例；
5. `late_reply(rp, OK)`（446）——把 13 挂起的调用者放行（OK，因为退出确实是预期路径）。

**第二段（RS_DEAD 已置，manager.c:451-490）——"真清理"**：
1. 读 `cleanup_script = RS_CLEANUP_SCRIPT`、`detach = RS_CLEANUP_DETACH`（451-452）；
2. 非 detach：`sched_stop(r_scheduler, endpoint)`（461，19）+ `srv_kill(r_pid, SIGKILL)`（470，19）——让调度器放手 + 让 PM 杀掉残留进程（`r_pid == -1` 时警告跳过）；
3. `RS_CLEANUP_SCRIPT` → 清位 + `run_script(rp)`（475-478）——跑恢复脚本；
4. `detach` → `detach_service(rp)`（483-485，2.4）；
5. 否则 `free_slot(rp)`（489-490）——释放槽位，**除非 `RS_REINCARNATE`**（槽位要留给 reincarnate 复用）。

### 2.3 kill_service / crash_service：RS 主动处决（manager.c:360-399）

- `kill_service(rp, errstr, err)`（宏 proto.h:44-45 → `kill_service_debug` manager.c:360-375）：打印错误（非停机时）→ `r_flags |= RS_EXITING`（"预期退出"）→ `crash_service_debug` 模拟崩溃 → 返回 err。**语义：崩掉服务且不允许重启**；
- `crash_service(rp)`（宏 proto.h:48-49 → `crash_service_debug` manager.c:380-399）：`rpub->endpoint == RS_PROC_NR` 时 **RS 自己 `exit(1)`**（392-394）；否则 `sys_kill(endpoint, SIGKILL)`（397）。

它们被 `restart_service`/`run_script` 用作"恢复失败 → 处决"的兜底（2.5/2.6 的 `kill_service(rp, "...", r)` 调用点）。

### 2.4 detach_service：降级旁路（manager.c:497-530）

`detach_service`（宏 proto.h:57-58）把服务从"系统服务"降级为**普通用户进程**（`RS_NORESTART` + `DET_RESTART` 且 restarts 超限时，2.1 的 CLEANUP_DETACH 触发）：

1. 用静态计数器生成**唯一 label**：`snprintf(rpub->label, "%lu.%s", ++detach_counter, label)` + `ds_publish_label`（510-513，11）——以新 label 重新发布，避免与旧 label 冲突；
2. 重置槽位：`r_flags = RS_IN_USE | RS_ACTIVE`（516）——**保留活动身份**；
3. 降权：`sys_flags &= ~(SF_CORE_SRV | SF_DET_RESTART)`（517）、`r_period = 0`（518）、`dev_nr = 0`（519）、`nr_domain = 0`（520）；
4. `sys_privctl(endpoint, SYS_PRIV_ALLOW)`（522）——允许继续运行。

降级后的服务不再受 RS 重启/监控管理——它是"被放生的服务"。

### 2.5 restart_service / reincarnate_service：两条恢复路径（manager.c:1246-1298, 1033-1052）

**restart_service**（1246-1298）是刷新/首次意外退出的恢复执行器：

1. `late_reply(rp, OK)`（1253）——刷新路径的调用者立即放行；
2. **有脚本**（1256-1261）：`run_script(rp)`，失败 → `kill_service(rp, "unable to run script", errno)`；
3. **无脚本**（1265-1285）：`r_next_rp == NULL` → `clone_service(rp, RST_SYS_PROC, 0)`（10）造副本，失败 → kill；`update_service(&rp, &replica_rp, RS_SWAP, 0)`（1276，10/16 的 swap 机制）把新实例换入；`run_service(replica_rp, SEF_INIT_RESTART, 0)`（1283，12）让新实例以 RESTART 模式初始化；
4. `SF_DET_RESTART && r_restarts < MAX_DET_RESTART` → `RS_CLEANUP_DETACH`（1290-1292）——重启后旧实例降级保留。

**reincarnate_service**（1033-1052）是 `RS_REINCARNATE` 的恢复路径（**只有 terminate_service 调用**）：`clone_slot(old_rp, &rp)`（10）复制槽位 → 清 endpoint 索引 → `start_service(rp, SEF_INIT_FRESH)`（12）以**全新**身份启动 → `r_restarts + 1`。"reincarnate" = 以新端点重生（区别于 restart 的原地换实例）。

### 2.6 run_script：恢复脚本执行器（manager.c:1185-1243）

`run_script`（static）执行恢复脚本：

1. **reason 字符串**（1195-1199）：`RS_REFRESHING` → `"restart"`、`RS_NOPINGREPLY` → `"no-heartbeat"`、否则 `"terminated"`；`incarnation_str` = `r_restarts` 计数（1200）；
2. `fork()`（1209）：
   - 子进程（1212-1219）：`execle(_PATH_BSHELL, "sh", r_script, label, reason, incarnation_str, NULL, envp)`——**shell 脚本**，参数把"谁、为什么、第几次"传给脚本；
   - 父进程（1220-1235）：`getprocnr(pid, &endpoint)` + `sys_privctl(SYS_PRIV_SET_USER)`（降权为普通用户）+ `vm_set_priv(endpoint, NULL, FALSE)` + `sys_privctl(SYS_PRIV_ALLOW)` + `vm_memctl(RS_PROC_NR, VM_RS_MEM_PIN, 0, 0)`（fork 后重钉 RS 自身内存）——每步失败都 `kill_service` 处决；
3. 脚本的 label/reason/incarnation 参数让脚本决定"恢复成什么样"（这正是 `do_restart`（13）能回来的原因：脚本自己发 `RS_RESTART`）。

> **[ARCH: A-1]** — `run_script` 的 `fork()+execle(sh)`（manager.c:1209-1219）依赖 libc fork；no_std 无 libc。外部行为保持（恢复脚本仍由 shell 执行），实现方案（委托 INIT/受限执行器，或标注 defer）随 10/15 的设计决策落地；doc 只陈述调用点与缺口。

### 2.7 get_service_instances：实例收集（manager.c:1334-1352）

`get_service_instances(rp, &rps, &nr_rps)` 用 `static struct rproc *instances[5]`（1340）收集一个服务的全部实例：`rp` 自身 + `r_prev_rp` + `r_next_rp` + `r_old_rp` + `r_new_rp`（1344-1348）——固定顺序。02 已用 `ServiceInstances` 迭代器替代该静态数组（ARCH A-3，`process_table.rs`），`cleanup_service` 的逐实例循环（1143）消费它。

---

## 3. Rust 设计决策

### 3.1 recovery.rs 纯决策切片

与 13/14 同款：IPC/动作面（`sys_kill`/`sched_stop`/`srv_kill`/`sys_privctl`/`vm_*`/`ds_publish_label`/fork）归 19，`unpublish_service` 归 11，`clone_service`/`update_service` 归 10，`run_service` 归 12，`late_reply` 归 06，`end_update`/`abort_update_proc` 归 16；`recovery.rs` 拥有**决策树与纯计算**：

| 函数 | C 对应 | 语义 |
|------|--------|------|
| `TerminateAction` + `TerminateDecision` | manager.c:1055-1180 | 决策输出：动作 + 待置位 `set_flags` |
| `terminate_decision(flags, sys_flags, restarts, has_script, shutting_down, is_updating)` | manager.c:1065-1178 | 完整决策树（6 输入 → 5 动作） |
| `compute_backoff(restarts, no_bin_exp, use_copy)` | manager.c:1163-1174 | `1 << min(restarts, 62)` 封顶 30；`USE_COPY` 折叠；`NO_BIN_EXP` → 1 |
| `script_reason(flags)` | manager.c:1195-1199 | `"restart"`/`"no-heartbeat"`/`"terminated"` |
| `late_reply_result(caller_request, norestart)` | manager.c:1134-1135 | `RS_DOWN`/`RS_REFRESH+norestart` → `OK`；否则 `EDEADEPT` |
| `CleanupDecision` + `cleanup_decision(flags)` | manager.c:451-452 | 第二段清理分类（script/detach） |
| `MAX_DET_RESTART`/`BACKOFF_BITS`/`MAX_BACKOFF` | const.h:25,50-51 | 常量（10/64/30） |

### 3.2 TerminateAction 建模

C 的决策树靠**内联置位 + fall-through**（init 失败分支置 `RS_REFRESHING`/`RS_EXITING` 后继续走主树）；Rust 把"决策"与"置位副作用"分离：

- `TerminateAction` 枚举 5 个变体，**非法状态不可表达**（例如"rollback 同时 refresh"不可能构造）；
- `TerminateDecision.set_flags` 汇总 C 决策过程中设置的全部 `r_flags` 位，由调用方一次性应用——避免了 C 的"边走边置位"导致的中间态可观测性问题（06 的 late_reply 等消费者只看到终态）。
- `CleanupAll { norestart, reincarnate, core_fatal }` 携带 EXITING 分支的三个布尔上下文，`Refresh`/`Backoff { backoff }`/`Restart` 对应另三个分支。

### 3.3 更新谓词的注入

决策树读两个更新谓词，建模方式不同：

- `SRV_IS_UPDATING(rp)`（const.h:114，读 `r_flags & RS_UPDATING`）——以 `is_updating` 布尔参数注入（INITIALIZING 分支的 rollback 判定，C 1071-1076）；决策层不直接读 `UPDATING` 位；
- `SRV_IS_UPD_SCHEDULED(rp)`（const.h:120，读未建模的 `r_upd` 描述符，02 P2-3）——**不在 `terminate_decision` 内建模**：EXITING 分支的 `abort_update_proc(EDEADSRCDST)`（C 1127-1129）是 16 的动作钩子，由调用方在 `CleanupAll` 执行时接线判定（§2.1 步骤 4 只陈述调用点）。

`terminate_decision` 的签名因此是 `(flags, sys_flags, restarts, has_script, shutting_down, is_updating)`——6 个纯输入，无表访问。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/rs/src/recovery.rs`：`TerminateAction`/`TerminateDecision`/`CleanupDecision` 三个类型 + `terminate_decision`/`compute_backoff`/`script_reason`/`late_reply_result`/`cleanup_decision` 五个函数 + 三个常量。模块 doc 注明动作钩子（16/06/11/19/ARCH A-1）为调用点。

### 4.2 关键不变量

1. **决策树顺序与 C 一致**（manager.c:1069-1178）：init 失败 → norestart → EXITING → REFRESHING → backoff/restart——顺序不可交换（例如 norestart 检测必须在 EXITING 分支之前，因为 norestart 会置 EXITING 位）。C 的全局 RUPDATE abort（1099-1102）是 16 的动作钩子，由调用方在决策前执行，不在 `terminate_decision` 内建模（§3.3）。
2. **init 失败分支的 fall-through 语义保留**：`SF_NO_BIN_EXP` 置 `RS_REFRESHING`、其他置 `RS_EXITING` 后**继续**走主树（与 C 1078-1091 一致）；唯一提前 return 是更新中 rollback（1071-1076）。
3. **`set_flags` 汇总**：决策过程置的位（`REFRESHING`/`EXITING`/`CLEANUP_DETACH`/`CLEANUP_SCRIPT`）全部出现在 `set_flags`，调用方一次应用；`InitUpdateRollback` 的 `set_flags` 为空。
4. **backoff 封顶**：`1 << min(restarts, 62)` 先移位再封顶 `MAX_BACKOFF`（C 顺序 manager.c:1166-1167）；`USE_COPY` 折叠仅在 `backoff > 1` 时（1168-1169）。
5. **`late_reply_result` 是纯函数**：不读槽位 caller_request 之外的状态，四组合（DOWN/REFRESH+norestart/REFRESH/其他）全覆盖。
6. **`cleanup_decision` 只分类**：第一段（RS_DEAD 标记/disallow/late_reply）与第二段的执行（sched_stop/srv_kill/run_script/free_slot）都是调用方动作；`detach` 时跳过 sched_stop/srv_kill（C 455-472 的语义由调用方保证）。

---

## 5. 测试要点

`recovery.rs` 内测试（`cargo test -p minix-rs --lib recovery`，11 项）：

1. `terminate_decision` init 失败 + `NO_BIN_EXP` → `Refresh` + `set_flags` 含 `REFRESHING`（C 1078-1086 fall-through 到 1155-1157）。
2. init 失败（其他）→ `CleanupAll` + `set_flags` 含 `EXITING`（C 1090-1091 fall-through）。
3. init 失败 + `is_updating` → `InitUpdateRollback`（C 1071-1076 提前 return）。
4. `NORESTART` + `DET_RESTART` + 有脚本 → `CleanupAll{norestart:true}` + 三置位（C 1105-1117）。
5. `restarts >= MAX_DET_RESTART` → 不置 `CLEANUP_DETACH`（C 1108-1111）。
6. `CORE_SRV` 且非 shutdown → `core_fatal:true`；shutdown 中 → false（C 1121-1123）。
7. `REINCARNATE` → `reincarnate:true`（C 1146-1151）。
8. `compute_backoff`：restarts 0/1/4/10（封顶 30）、`NO_BIN_EXP` → 1、`USE_COPY` 折叠（C 1163-1174）。
9. `script_reason` 三值（C 1195-1199）。
10. `late_reply_result` 四组合（C 1134-1135）。
11. `cleanup_decision` 两组合（C 451-452）。

测试总数声明：本文档范围为 `recovery` 模块测试数（11 项，以该模块 `cargo test` 输出为准）。全局 `cargo test -p minix-rs --lib` 通过数随并行模块增长（见 12 §5 的累计值约定）。

---

## 6. 过渡

终止/恢复状态机是服务生命周期的终点，也是与其他状态机的接口：

- **更新互斥**：`terminate_service` 的三处 update 钩子（rollback `end_update`、全局 `abort_update_proc(ERESTART)`、调度中 `abort_update_proc(EDEADSRCDST)`）都是 **16-rs-live-update** 的机制——恢复动作与更新动作互斥，由 16 的状态机仲裁；
- **RS 自身特例**：`SF_CORE_SRV` 死亡的 `_exit(1)` 与 RS 自己被 kill 的 `exit(1)`（crash_service 392-394）在 **18-rs-self-lifecycle** 展开；
- **恢复入口**：`RS_RESTART`（13，恢复脚本回环）与 `RS_REINCARNATE` 汇聚到本文档的 `restart_service`/`reincarnate_service`；
- **触发源**：`RS_FI`（14 的故障注入）与 07 的 SIGKILL 升级都是退出事件的来源，汇入 `terminate_service` 决策树。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/13-rs-control-requests.md` —— `stop_service`/`RS_DOWN`/`RS_REFRESH`/`RS_RESTART` 入口
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/07-rs-period-heartbeat.md` —— `r_stop_tm`/SIGKILL 升级、`RS_NOPINGREPLY`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— `end_update`/`abort_update_proc`/`SRV_IS_UPD_SCHEDULED`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md` —— `clone_service`/`update_service`/`swap_slot`/`clone_slot`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md` —— `run_service`/`start_service`/`SEF_INIT_RESTART`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/11-rs-publish.md` —— `unpublish_service`/`ds_publish_label`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/14-rs-query-requests.md` —— `RS_FI` 故障注入触发源
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/18-rs-self-lifecycle.md` —— RS 自身死亡/rollback 特例
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` —— `ServiceInstances` 迭代器（ARCH A-3）
- `minix3/minix/servers/rs/manager.c:360-530,1033-1052,1055-1180,1185-1243,1246-1298,1334-1352`、`proto.h:44-59`、`const.h:25,50-51,114,120` —— ground truth
