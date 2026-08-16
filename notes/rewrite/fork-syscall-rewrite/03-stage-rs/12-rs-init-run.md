# 12-rs-init-run: 服务启动与初始化协议

> **分类**: 阶段 4 — 服务创建与配置（从槽位到运行进程的第六步：运行）
> **源码**: `minix3/minix/servers/rs/manager.c`（`end_srv_init`—328、`run_service`—923、`start_service`—950）、`minix3/minix/servers/rs/utility.c:18-64`（`init_service`）、`minix3/minix/servers/rs/request.c:462-529`（`do_init_ready`）、`minix3/minix/servers/rs/request.c:890-938`（`do_upd_ready`）、`minix3/minix/servers/rs/main.c:591-626`（`sef_cb_init_response`/`sef_cb_lu_response`）、`minix3/minix/servers/rs/main.c:784-825`（`catch_boot_init_ready`）、`minix3/minix/include/minix/ipc.h:1855-1866`（`mess_rs_init`）、`minix3/minix/include/minix/sef.h:93-103`（`SEF_INIT_*`）
> **Rust 模块**: `os/servers/rs/src/ready.rs`（`init_flags`/`init_message`/`do_init_ready`/`ReadyOutcome`/`do_upd_ready`/`UpdReadyOutcome`/`end_srv_init`/`should_reply_ready`/`normalize_init_response`/`normalize_lu_response`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md`（创建）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/11-rs-publish.md`（发布）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md`（`reply`/`late_reply` 原语）
> **说明**: 创建（10）+ 发布（11）之后，服务是"活着但还没初始化"的进程。启动协议 = RS 发 `RS_INIT` 消息 → 服务自己初始化 → 回 ready → RS 收尾。**同一套协议在 boot 与运行时各用一次**：boot 的 Step 2/3 用 `catch_boot_init_ready` 同步阻塞捕获；运行时由主循环的 `RS_INIT`/`RS_LU_PREPARE` 分支异步分派到 `do_init_ready`/`do_upd_ready`。

---

## 1. 概念：用消息握手完成初始化

### 1.0 章节引言

RS 无法替服务初始化——只有服务自己知道"我的数据结构建好了"。RS 能做的只是：**发消息、等结果、做仲裁**（成功了放行 + 收尾；失败了判死；更新中则推进状态机）。本文档回答的问题是：**RS_INIT 握手协议的消息内容、合法性检查、成功/失败分支、以及 boot 与运行时两处使用有何不同**。

> **本章不讲什么**（机制一律移交）:
> - `cleanup_service`/`crash_service` 的机制（`15-rs-terminate-restart.md`）——本文档只陈述调用点
> - `rupdate_upd_move`/`end_update`/`start_update_prepare_next`/`start_update` 的机制（`16-rs-live-update.md`）——本文档只固化分支决策
> - `reply`/`late_reply`/`EDONTREPLY` 的原语（`06-rs-main-loop.md`）——本文档直接使用
> - `rs_asynsend` 与消息发送面（`19-rs-external-interfaces.md`）

### 1.1 为什么是消息握手（WHY）

RS 的单线程事件循环（06）不允许它阻塞等一个服务初始化——那会卡死整个系统（其他服务的 RS_UP/心跳都会排队）。所以协议是**异步的**：

```
RS                         服务
 │  RS_INIT（init_service）  │
 │ ────────────────────────→ │  服务开始初始化
 │                          │
 │                          │  …初始化完成…
 │  RS_INIT ready（m_source=服务，result=OK/err）
 │ ←──────────────────────── │
 │  do_init_ready            │
 │  ├─ OK     → reply OK + end_srv_init（fresh）
 │  ├─ OK     → 更新推进（16）
 │  └─ 失败   → crash_service（15）
 │  （返回 EDONTREPLY——主循环不 reply）
```

`RS_INIT` 是 RS 与服务的**双向通道**：RS 用它发初始化请求，服务用同一个消息类型回 ready。主循环收到 `RS_INIT` 后按 `m_source` 查槽位、做合法性检查，然后分支。

### 1.2 boot 与运行时的两处使用（WHAT）

| | boot Step 2/3 | 运行时 |
|---|---|---|
| 路径 | `sef_cb_init_fresh` → `init_service` + `catch_boot_init_ready`（main.c:784-821） | 主循环 `RS_INIT` 分支 → `do_init_ready`（request.c:462-533） |
| 捕获方式 | **同步阻塞**：`sef_receive_status(endpoint, &m, &ipc_status)` 等指定服务（main.c:795） | **异步分派**：`sef_receive_status(ANY)` 后分类（06） |
| 回包 | `catch_boot_init_ready` 自己 `reply`（main.c:814） | `do_init_ready` 里 `reply`（request.c:521-522） |
| 例外 | **VM 不 reply**（异步回包，同步会死锁，main.c:809-815） | `RS_INITIALIZING` 门（request.c:477-483） |
| 失败 | `panic`（boot 不允许服务初始化失败，main.c:804-807） | `crash_service`（运行期失败判死，request.c:488-497） |

boot 的 Step 2 逐个启动 boot 服务（RS/VM 例外走 `init_service` 后由 VM 异步回包），Step 3 再 catch 剩余的 init ready（01 骨架已映射锚点）。运行时的 ready 处理则要区分 fresh/update 两条路径。

### 1.3 RS 自模拟 ready（main.c:591-626）

RS 自己是 boot 服务之一，但 RS 的初始化**不是消息驱动的**（没有进程能向 RS 发 RS_INIT）。SEF 框架让 RS 在完成自己的 `sef_cb_init_fresh` 后，**伪造**一个 `RS_INIT` ready 消息喂给 `do_init_ready`（`sef_cb_init_response`，main.c:591-609）；Live Update 时同理（`sef_cb_lu_response` → `do_upd_ready`，main.c:614-626）。这是"自举"：RS 用统一协议处理自己的初始化。

---

## 2. C 源码分析

### 2.1 run_service 与 start_service（manager.c:923-982）

**`run_service(rp, init_type, init_flags)`**（manager.c:923-945）是"运行原语"：
1. `sys_privctl(endpoint, SYS_PRIV_ALLOW, NULL)`（manager.c:932-934）——**允许服务运行**（03 的 priv 面；RS/VM 在 boot 时已被 main.c 特殊处理）；
2. `init_service(rp, init_type, init_flags)`（manager.c:937-939）——发 RS_INIT（见 2.2）。

失败都走 `kill_service(rp, errstr, s)`（15）。

**`start_service(rp, init_flags)`**（manager.c:950-982）是"完整启动编排"：
1. `rp->r_priv.s_init_flags |= init_flags`（manager.c:959）——Rust 侧 `ready::fold_init_flags(slot, init_flags)`（R14，OR 语义，测试锁定；replica 路径由 `service_create::link_replica` 承担，manager.c:751-752）；
2. `create_service(rp)`（manager.c:960-963）——10；
3. `activate_service(rp, NULL)`（manager.c:964）——10（无旧实例）；
4. `publish_service(rp)`（manager.c:967-970）——11；
5. `run_service(rp, SEF_INIT_FRESH, init_flags)`（manager.c:973-976）——本文档。

`start_service` 是 `RS_UP`（13）与 boot 共用的最高层编排。

### 2.2 init_service 与 RS_INIT 消息（utility.c:18-64）

`init_service(rp, type, flags)`（utility.c:18）：

1. `r_flags |= RS_INITIALIZING`；`r_alive_tm = getticks()`；`r_check_tm = r_alive_tm + 1`（utility.c:24-26）——**期望一个 period 内回 ready**（07 的心跳超时检查以此为据）。**R14**：这三行变异由 `ready::mark_initializing(slot, ticks)` 独立建模（utility.c:19-21，含测试）——此前全 crate 生产路径无一处写入 `RS_INITIALIZING`，12 接线若漏设该位，所有 ready 消息都会被 `do_init_ready` 门拒绝；
2. **ROOT_SYS_PROC 例外**（utility.c:28-31）：RS 自己的初始化"我们做完了"——直接 `return OK`，不发消息（RS 初始化由 `sef_cb_init_fresh` 完成，01）；
3. 推导 `old_endpoint`/`prepare_state`（utility.c:33-42）：`r_old_rp`（LU 旧版本）→ `r_upd.state_endpoint`/`r_upd.prepare_state`；否则 `r_prev_rp` → 其 endpoint；
4. `SF_USE_SCRIPT` → `flags |= SEF_INIT_SCRIPT_RESTART`（utility.c:44-47）——脚本重启的 init 要带上标记（sef.h:102）；
5. 装配消息（utility.c:49-60）：

| 字段 | 值 | C |
|------|----|---|
| `m_type` | `RS_INIT` | 50 |
| `type` | `(short)type`（SEF_INIT_FRESH/LU/RESTART） | 51 |
| `flags` | 扩展后的 init flags | 52 |
| `rproctab_gid` | `rinit.rproctab_gid`（boot 建的 grant，01/12） | 53 |
| `old_endpoint` | 上表推导值 | 54 |
| `restarts` | `(short) rp->r_restarts + 1` | 55 |
| `buff_addr`/`buff_len` | `r_map_prealloc_addr`/`len`（16 预分配 mmap），发完清零 | 56-60 |
| `prepare_state` | 上表推导值 | 58 |

6. `rs_asynsend(rp, &m, 0)`（utility.c:61）——异步发送，不阻塞。

### 2.3 do_init_ready：ready 处理的四步（request.c:462-529）

```
do_init_ready（request.c:462-529）
  ├─ 门：!RS_INITIALIZING → EINVAL（477-483，"unexpected init ready"）
  ├─ result != OK：
  │    ├─ init_strerror 打印（489-491）
  │    ├─ result==ERESTART && !SRV_IS_UPDATING → RS_REINCARNATE（492-493）
  │    ├─ crash_service(rp)（494，"模拟崩溃"）
  │    ├─ r_init_err = result（495）
  │    └─ 返回 EDONTREPLY（496）
  ├─ SRV_IS_UPDATING：
  │    ├─ rupdate.num_init_ready_pending--（507）
  │    ├─ r_flags |= RS_INIT_DONE（508）
  │    ├─ pending==0 → end_update(OK, RS_REPLY)（509-512，"update succeeded"）
  │    └─ 返回 EDONTREPLY（513）
  └─ fresh：
       ├─ r_flags &= ~RS_INITIALIZING；r_check_tm=0；r_alive_tm=getticks()（516-518）
       ├─ reply(endpoint, rp, &m)（521-522）——先回包再收尾
       ├─ end_srv_init(rp)（525）——见 2.6
       └─ 返回 EDONTREPLY（528）
```

关键细节：**失败分支与 updating 分支都不 reply**（返回 `EDONTREPLY`，主循环不回复）；只有 fresh 成功路径 reply OK。`ERESTART`（sys/errno.h:196）特判——"服务重启"错误在非更新期要触发 `RS_REINCARNATE`（15 的复活路径）。**R13**：C 的槽位变异（`RS_REINCARNATE` 置位 + `r_init_err = result`、`RS_INIT_DONE` 置位、fresh 的三行复位）由 `ReadyDecision.mutations`（`SlotMutations` 载荷）携带，12 接线在动作 hook（crash_service/end_update/reply）后 `apply` 一次提交。

### 2.4 do_upd_ready：update 就绪（request.c:890-938）

`do_upd_ready` 处理 `RS_LU_PREPARE` 的 ready（16 的 prepare 阶段）：

1. 门（request.c:903-910）：`!rupdate.curr_rpupd || rp != rpupd->rp || RUPDATE_IS_INITIALIZING()` → `EINVAL`（"late/unexpected update ready"）；
2. `r_flags |= RS_PREPARE_DONE`（request.c:911）；
3. `result != OK` → `end_update(result, RS_REPLY)` + `EDONTREPLY`（request.c:917-922）——旧版本继续执行；
4. `start_update_prepare_next() != NULL`（request.c:930-932）——多组件更新还有下一个要 prepare → `EDONTREPLY`；
5. 否则 `start_update()`（request.c:934-935）——所有组件就绪，执行更新 + 请求新实例初始化。

本文档只固化分支决策；rpupd 链与 start_update 机制全在 16。

### 2.5 catch_boot_init_ready：boot 的同步捕获（main.c:784-821）

boot Step 2/3 用 `sef_receive_status(endpoint, &m, &ipc_status)`（main.c:795）**阻塞等待指定服务**的 ready：

1. 收包失败 / `m_type != RS_INIT` → `panic`（main.c:795-800）；
2. `result != OK` → `panic("unable to complete init for service")`（main.c:804-807）——**boot 期初始化失败是致命的**；
3. **VM 例外**（main.c:809-815）：`m_source != VM_PROC_NR` 才 `reply`；VM 是**异步**回包的，同步 reply 会导致死锁（注释原文："Synchronous replies could lead to deadlocks there"）；
4. 清 `RS_INITIALIZING`/`r_check_tm`/`r_alive_tm`（main.c:817-820）。

注意 boot 的捕获**不调 `end_srv_init`**（没有 prev 副本需要清理）。

### 2.6 end_srv_init：初始化完成收尾（manager.c:328-356）

`end_srv_init(rp)`（manager.c:328）：

1. `late_reply(rp, OK)`（manager.c:336）——如果 RS_LATEREPLY 挂起（RS_UP 的 NOBLOCK 场景），补发 OK（06）；
2. 有 `r_prev_rp`（副本重启场景，manager.c:338-353）：
   - `SRV_IS_UPD_SCHEDULED(prev)` → `rupdate_upd_move(prev, rp)`（manager.c:344-346）——把 prev 的更新计划移交新实例（16）；
   - `cleanup_service(prev)`（manager.c:347）——清理旧副本（15）；
   - `r_prev_rp = NULL`；`r_restarts += 1`（manager.c:348-349）；
3. `r_next_rp = NULL`（manager.c:354）。

"重启完成"的语义落在这里：新实例顶替旧副本，重启计数 +1。

---

## 3. Rust 设计决策

### 3.1 ready.rs 纯切片

启动协议的 IPC 面（`rs_asynsend`/`reply`、`RS_INIT` 消息收发）归 06/19；ready.rs 拥有**纯决策**：

| 函数 | C 对应 | 语义 |
|------|--------|------|
| `init_flags(use_script, flags)` | utility.c:44-47 | `SF_USE_SCRIPT → |SEF_INIT_SCRIPT_RESTART` |
| `mark_initializing(slot, ticks)` | utility.c:19-21 | 发 RS_INIT 前：置 `INITIALIZING` + `alive_tm = ticks` + `check_tm = ticks+1`（R14） |
| `fold_init_flags(slot, init_flags)` | manager.c:953 | `s_init_flags |= init_flags`（OR 语义，R14） |
| `init_message(...)` | utility.c:49-60 | RS_INIT 载荷装配（`InitMessage`） |
| `do_init_ready(flags, result, is_updating, pending, ticks)` | request.c:462-529 | 门 + 失败 + 分支 → `ReadyDecision { outcome, mutations }`（R13） |
| `do_upd_ready(result, gate_ok, has_next)` | request.c:890-938 | update 就绪分支 → `UpdReadyOutcome` |
| `end_srv_init(rp, has_prev)` | manager.c:336-354 | 槽位收尾（restarts/prev/next） |
| `should_reply_ready(src)` | main.c:812-815 | VM 异步例外 |
| `normalize_init_response`/`normalize_lu_response` | main.c:591-626 | EDONTREPLY 归一化 |

### 3.2 InitMessage 类型化（A-2 第一步）

`mess_rs_init`（ipc.h:1855-1866）映射为类型化结构：`init_type: u16`（C 的 `short`）、`rproctab_gid: Option<u32>`（02 P2-2：boot 建 grant 后才有值）、`old_endpoint: Option<Endpoint>`（fresh 时为 None）、`buff_addr: u64`/`buff_len: usize`（16 预分配 mmap 的传输槽）。C 从 `r_upd` 推导 `old_endpoint`/`prepare_state`（utility.c:33-42），Rust 由调用点注入（`r_upd` 未建模，02 P2-3）。

### 3.3 ReadyDecision 编码分支 + 变异载荷

`do_init_ready` 返回 `ReadyDecision`（`outcome` + `mutations: SlotMutations`），四变体对应 C 的四条路径：

```rust
pub struct ReadyDecision { outcome: ReadyOutcome, mutations: SlotMutations }  // R13
pub enum ReadyOutcome {
    Unexpected,                              // 门失败 → EINVAL
    InitFailed { result: i32, reincarnate: bool },  // crash（15 hook）
    UpdateInitDone { pending_remaining: usize },    // 更新推进（16 hook）
    FreshInitDone,                           // reply + end_srv_init
}
```

`reincarnate` 的推导（`result == ERESTART && !is_updating`，request.c:492-493）是纯逻辑，在 Rust 中显式编码；`crash_service`/`end_update`/`reply` 是调用点（15/16/06）。

**变异载荷（R13）**：C 的槽位变异随 `ReadyDecision.mutations` 携带，12 接线在动作 hook 后 `mutations.apply(rp)` 一次提交——`InitFailed` 携带 `set=REINCARNATE`（`ERESTART && !updating` 时）+ `init_err=Some(result)`（request.c:491-495）；`UpdateInitDone` 携带 `set=INIT_DONE`（request.c:509）；`FreshInitDone` 携带 `clear=INITIALIZING` + `check_tm=0` + `alive_tm=getticks()`（request.c:516-518，`ticks` 由调用点注入）。与 07（`PeriodDecision`）/15（`TerminateDecision`）的载荷风格统一。

### 3.4 EDONTREPLY 归一化

`do_init_ready`/`do_upd_ready` 在 C 中**总是**返回 `EDONTREPLY`（成功路径），主循环据此不 reply。SEF 自模拟回调把它归一化：

- `sef_cb_init_response`（main.c:591-609）：`EDONTREPLY → OK`；
- `sef_cb_lu_response`（main.c:614-626）：`EDONTREPLY → EGENERIC`（走到这步说明更新没发生，sys/errno.h:200）。

两个归一化分别建模为 `normalize_init_response`/`normalize_lu_response`。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/rs/src/ready.rs` 函数表见 §3.1；测试 12 项。配套修改：`minix-types` 补 `EGENERIC = 204`（sys/errno.h:200，`sef_cb_lu_response` 归一化需要）。

### 4.2 关键不变量

1. **`RS_INITIALIZING` 是 ready 的合法性前提**：没有它，ready 消息是"意外的"（`EINVAL`）——RS 可能收到重复/伪造的 ready。
2. **失败与 updating 路径不 reply**：只有 fresh 成功路径 `reply OK`（request.c:521-522）；`EDONTREPLY` 是正常返回码而非错误。
3. **VM 永不收到 boot ready 的 reply**（main.c:812-815）——同步 reply 会死锁。
4. **boot 期初始化失败 = panic**，运行期 = crash_service：同一协议、两种严重度（main.c:805-806 vs request.c:494）。
5. **`end_srv_init` 先 reply 后收尾**（request.c:521-525）：服务先被放行，RS 再清理 prev 副本。

---

## 5. 测试要点

`ready.rs` 内 15 项测试（`cargo test -p minix-rs --lib ready` 过滤含 `dispatch::test_classify_ready`，共 16 通过）：

1. `init_flags`：SF_USE_SCRIPT 置位/不置位（2 断言组）。
2. `init_message`：type（SEF_INIT_RESTART=2）/flags/gid/old_endpoint/restarts/buff/prepare_state 全字段。
3. `mark_initializing`（R14）：发 RS_INIT 前置迁移——`INITIALIZING` 置位 + `alive_tm=ticks` + `check_tm=alive_tm+1`（utility.c:18-21，reply 在周期内）。
4. `fold_init_flags`（R14）：`priv_.init_flags |= flags`（OR 非替换，manager.c:953）。
5. `do_init_ready` 门：无 `RS_INITIALIZING` → `Unexpected`。
6. `do_init_ready` 失败：`ERESTART`+非更新 → `reincarnate: true`；非 ERESTART → false；更新中 ERESTART → false（request.c:492-493 的三态）。
7. `do_init_ready` 更新完成：pending 递减到 0 / 非零 → `UpdateInitDone`（R4：递减前
   `debug_assert!(pending > 0)`——C 调用方保持 `num_init_ready_pending > 0`（main.c:586 assert），
   underflow 是程序错误而非静默饱和；`test_do_init_ready_pending_underflow_panics` 锁死该语义）。
8. `do_init_ready` fresh：→ `FreshInitDone`。
9. `do_upd_ready` 四分支：门失败 / prepare 失败 / 还有下一个 / start_update。
10. `end_srv_init`：has_prev → restarts+1 + prev/next 清空（`test_end_srv_init_bookkeeping`）；无 prev → 只清 next、restarts 保留（`test_end_srv_init_no_prev`，manager.c:354）。
11. `should_reply_ready`：VM → false；VFS/PM → true。
12. `normalize_init_response`：result 非 OK 优先 / EDONTREPLY → OK / 其他错误透传。
13. `normalize_lu_response`：EDONTREPLY → EGENERIC / 其他透传。

测试总数声明：本文档范围为 **15 项**（`ready` 模块内）。全局 `cargo test -p minix-rs --lib` = 208 通过（2026-08-16，随并行模块增长，以各 doc 范围为准）。

---

## 6. 过渡

`end_srv_init` 之后，服务**正式运行**：`RS_INITIALIZING` 已清、reply 已回、prev 副本已清理（重启场景）。RS 对它的控制从"启动编排"切换到"日常监控"：

- **07-rs-period-heartbeat**：`r_alive_tm`/`r_check_tm` 从 `init_service` 就开始记账（utility.c:24-26），心跳检查随即接管；
- **13-rs-control-requests**：`RS_DOWN` → `stop_service`（SIGTERM → SIGKILL 升级），运行期请求从这里进入；
- **15-rs-terminate-restart**：`crash_service`/`kill_service`/`cleanup_service` 是本文档所有失败分支的目的地；
- **16-rs-live-update**：`do_upd_ready` 的四分支与 `end_srv_init` 的 `rupdate_upd_move` 是更新状态机的两个入口。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md` —— `reply`/`late_reply`/`EDONTREPLY` 原语（ready 处理的载体）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/07-rs-period-heartbeat.md` —— `init_service` 设置的 `r_check_tm = alive_tm + 1` 的消费点
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/15-rs-terminate-restart.md` —— `crash_service`/`kill_service`/`cleanup_service` 机制
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— `do_upd_ready`/`rupdate_upd_move`/`end_update` 机制
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md` —— boot Step 2/3 锚点与 `rproctab_gid` 创建点
- `minix3/minix/servers/rs/manager.c:328-356,923-982`、`utility.c:18-68`、`request.c:462-533,890-942`、`main.c:591-626,784-825` —— ground truth
- `minix3/minix/include/minix/ipc.h:1855-1866` —— `mess_rs_init`
