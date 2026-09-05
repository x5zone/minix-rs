# 16-rs-live-update: Live Update 状态机

> **分类**: 阶段 6 — Live Update（RS 最复杂状态机）
> **源码**: `minix3/minix/servers/rs/update.c`（1011 行：`rupdate_clear_upds`—7、`rupdate_add_upd`—23、`rupdate_set_new_upd_flags`—88、`rupdate_upd_init`—121、`rupdate_upd_clear`—135、`rupdate_upd_move`—164、`srv_update`—230、`update_service`—262、`rollback_service`—330、`update_period`—371、`start_update_prepare`—401、`start_update_prepare_next`—467、`start_update`—532、`start_srv_update`—621、`complete_srv_update`—657、`abort_update_proc`—707、`end_update_curr`—744、`end_update_before_prepare`—763、`end_update_prepare_done`—780、`end_update_initializing`—795、`end_update_rev_iter`—816、`end_update_debug`—865、`end_srv_update`—932）、`minix3/minix/servers/rs/request.c:534-889`（`do_update`）、`minix3/minix/servers/rs/const.h:58,75-76,83,114-120`、`minix3/minix/include/minix/sef.h:235-242`
> **Rust 模块**: `os/servers/rs/src/live_update.rs`（`LuFlags`/`UpdatePhase`/`update_phase`/`SEF_LU_STATE_*`/`resolve_prepare_maxtime`/`lu_flags_from_rss`/`vm_default_prealloc`/`validate_update_request`/`UpdateEntry`/`UpdateChain`/`EndUpdateRole`/`end_update_role`/`AbortAction`/`abort_action`/`end_srv_reply_flag`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`UpdateChain` 数据形状）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md`（`clone_service`/`update_service`/`swap_slot`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md`（`init_slot`/`inherit_service_defaults`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md`（`run_service`/`end_srv_init`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/04-rs-access-control.md`（`check_call_permission`）
> **说明**: 本文档是 Live Update 全状态机：`RS_UPDATE`（13 之后的第 16 篇）触发，`prepare → update → init → end/rollback` 四阶段。它依赖 17（state data）、18（RS 自身特例）、19（SEF/VM 契约）——本文档只落地**状态机本体**与**纯切片**（`live_update.rs`）。

---

## 1. 概念：不停机的服务替换

### 1.0 章节引言

重启一个服务意味着停摆；Live Update（LU）让服务在**不中断对外服务**的前提下换成新版本：RS 先创建新实例（10），旧实例继续运行，等新实例 prepare 完成、初始化完成后**原子切换**（swap），旧版本退出。若初始化失败，则回滚到旧版本。这就是 `prepare → update → init → end/rollback` 状态机。

> **本章不讲什么**（机制一律移交）:
> - 新实例的创建/swap（`10-rs-service-create.md`）——`clone_service`/`update_service`/`swap_slot`/`activate_service`
> - 初始化协议（`12-rs-init-run.md`）——`run_service`/`end_srv_init`/`SEF_INIT_LU`
> - 状态数据迁移（`17-rs-state-data.md`）——`init_state_data`/cpf grants
> - RS 自身的 LU 特例（`18-rs-self-lifecycle.md`）——`sys_whoami`/`vm_update(SF_VM_ROLLBACK)`
> - 外部 SEF/VM 契约（`19-rs-external-interfaces.md`）——`vm_memctl`/`vm_update`/`request_prepare_update_service`/`rs_receive_ticks`
> - 周期检查里的 update 超时（`07-rs-period-heartbeat.md`）——`update_period` 是 16 的调用点

### 1.1 为什么需要 LU（WHY）

Minix3 的系统服务（VM/PM/VFS/驱动）重启成本高：IPC 引用、内存映射、设备状态会丢失。LU 的核心不变量：**任何时刻都有一个可用版本**。为此：

- **prepare 阶段**：新实例创建但不运行（10），旧实例准备状态数据（17）；
- **update 阶段**：旧实例暂停、新实例接管（swap，10/16）；
- **init 阶段**：新实例以 `SEF_INIT_LU` 初始化（12），旧版本等待结果；
- **end/rollback**：成功 → 清理旧版本；失败 → 清理新版本、旧版本继续。

`RS_UPDATE` 消息（13 之后的主循环分派）是唯一入口；`RS_LU_PREPARE`（服务的 prepare 完成通知）走 `do_upd_ready`（12/16）。`update_period`（07）负责超时。

### 1.2 LU 状态机总图（WHAT）

```
                    ┌──────────────────────────────────────────────────────┐
                    │             Live Update 状态机（全局 rupdate）       │
                    └──────────────────────────────────────────────────────┘
  Idle ── RS_UPDATE（do_update, request.c:534）
    │      ① 校验（五查）→ ② 建新实例（10）→ ③ state data（17）→ ④ rupdate_add_upd
    ▼
  Scheduled（num_rpupds>0, !RS_UPDATING）
    │  start_update_prepare（update.c:401）
    ▼
  Preparing ── 逐服务发 RS_LU_PREPARE ── RS_LU_PREPARE 回（do_upd_ready, 12/16）
    │  start_update（update.c:532，VM multi 时序 + rs_receive_ticks）
    ▼
  Updating（RS_UPDATING）── start_srv_update/complete_srv_update（update.c:621/657）
    ▼
  Initializing（RS_INITIALIZING）── 新实例 SEF_INIT_LU 初始化（12）→ RS_INIT 回
    │
    ▼
  Ended（end_update, update.c:865）
    ├─ 成功：end_srv_init（12）+ 清理旧版本（end_srv_update）
    └─ 失败：rollback_service（update.c:330）回滚 + 清理新版本
    │
    └─ 任何阶段可 abort（abort_update_proc, update.c:707）：
         Scheduled → rupdate_clear_upds；Initializing → end_update(reason, RS_REPLY)；
         否则 → end_update(reason, RS_CANCEL)
```

`UpdatePhase` 四态（Idle/Scheduled/Updating/Initializing）由 `rupdate.flags` 的 `RS_UPDATING`/`RS_INITIALIZING` 位 + `num_rpupds` 解码（const.h:105,111）——这是 ARCH A-6 的核心：C 的位标志 + 宏判定 → 类型化枚举。

---

## 2. C 源码分析

### 2.1 do_update：LU 入口与校验（request.c:534-889）

`do_update` 的完整流程：

1. **消息面**（request.c:554-570）：`copy_rs_start`（08）+ `copy_label`（08）+ `lookup_slot_by_label`（02）；
2. **标志解码**（request.c:574-605）：
   - `RSS_SELF_LU`/`RSS_FORCE_SELF_LU` → `SEF_LU_SELF`（579-580）；
   - `RSS_PREPARE_ONLY_LU` → `SEF_LU_PREPARE_ONLY`（582-583）；
   - `RSS_ASR_LU` → `SEF_LU_ASR`（585-586）；
   - `!prepare_only && RSS_DETACH` → `SEF_LU_DETACHED`（588-589）；
   - **VM 默认 mmap 预分配**（591-599）：`rss_map_prealloc_bytes <= 0 && endpoint == VM && ((lu_flags & (SELF|ASR)) != SELF || RSS_FORCE_INIT_ST) && RS_VM_DEFAULT_MAP_PREALLOC_LEN > 0` → 设为 8MB（const.h:83）——非同一性更新给 VM 预映射内存；
   - `RSS_NOMMAP_LU || rss_map_prealloc_bytes > 0` → `SEF_LU_NOMMAP`（601-605）；
3. **init flags**（request.c:607-623）：`RSS_FORCE_INIT_CRASH/FAIL/TIMEOUT/DEFCB` → `SEF_INIT_*`（同 13 的 `up_init_flags`）+ `RSS_FORCE_INIT_ST` → `SEF_INIT_ST`；**`init_flags |= lu_flags`**（622-623）——LU 标志同时作为 init 标志传给新实例；
4. **target label**（request.c:627-641）：`rss_trg_label.l_len > 0` → 查 `state_endpoint`（状态迁移的自定义源）；
5. **权限**（request.c:643-644）：`check_call_permission(RS_UPDATE, rp)`（04）；
6. **纯校验五查**（request.c:648-681）：
   - `prepare_state == SEF_LU_STATE_NULL` → `EINVAL`（648-650）；
   - `prepare_maxtime == 0` → `RS_DEFAULT_PREPARE_MAXTIME`（653-655，= `2*RS_DELTA_T`，const.h:58）；
   - `RUPDATE_IS_UPDATING()` → `EBUSY`（659-663）；
   - 已调度：`!batch_mode` → `EBUSY`（666-668）；`SRV_IS_UPD_SCHEDULED(rp)` → `EINVAL`（669-671）——batch 模式只允许追加未在链上的服务；
   - `prepare_only && endpoint ∈ {VM,PM,VFS} && prepare_state != SEF_LU_STATE_UNREACHABLE` → `EINVAL`（674-681）；`prepare_only && endpoint == RS` → `EINVAL`（683-686）；
7. **建新实例**（request.c:691-760）：self update → `clone_service(rp, LU_SYS_PROC, init_flags)`（10）；普通 → `alloc_slot` + `init_slot` + `inherit_service_defaults` + `create_service`（08/10）+ `r_new_rp`/`r_old_rp` 互链；`ROOT_SYS_PROC`（RS 更新时）→ `update_sig_mgrs` 信号管理器备份（18）；heap/map 预分配（`vm_memctl`，19）；
8. **state data + grants**（request.c:762-836）：`init_state_data`（17）+ `cpf_grant_direct` 三件套（state data/ipcf_els/eval，失败 `rupdate_upd_clear` + `ENOMEM`）；
9. **入链**（request.c:838-845）：`rpupd->prepare_state/state_endpoint/prepare_tm/prepare_maxtime` 填充 + `rupdate_add_upd`（2.2）；
10. **回包**（request.c:847-860）：batch → 立即 `OK`；`start_update_prepare` 返回 `ESRCH`（链空）→ `OK`；`noblock` → `OK`；否则 `rupdate.last_rpupd->rp` 写 LATEREPLY 三字段（`RS_UPDATE`）+ `EDONTREPLY`——新版本初始化完成后补发 reply（12/06）。

### 2.2 rpupd 链操作（update.c:7-183）

`rprocupd` 描述符（type.h:30-42）内嵌在每个 `rproc` 的 `r_upd` 字段；全局 `rupdate`（type.h:43-52）持有链头尾/当前/VM/RS 指针。五个链操作：

- **`rupdate_clear_upds`**（7-21）：`RUPDATE_ITER` 遍历清每个描述符 + `RUPDATE_CLEAR()`（memset 全局）——abort scheduled 时用；
- **`rupdate_add_upd`**（23-86）：**部分排序插入**——为支持 multi-component-with-VM，链序为"普通服务（头）→ … → VM（尾前）→ RS（尾）"：
  1. 断言 `next_rpupd == NULL && prev_rpupd == NULL`（30-31）——条目必须是自由的；
  2. 找插入点（42-48）：从 `last_rpupd` 起，若尾部是 RS 且新条目不是 RS → 前移；若再往前的尾部是 VM 且新条目不是 VM/RS → 再前移；
  3. 插入（50-62）：头部插入更新 `first_rpupd = curr_rpupd = rpupd`；否则挂到 `prev_rpupd` 之后；`num_rpupds++`；
  4. **标志传播**（64-67）：新条目的 `lu_flags & (INCLUDES_VM|INCLUDES_RS|MULTI)` 传播到**全链**每个条目的 `lu_flags` 和 `init_flags`；
  5. **VM/RS 指针**（69-72）：`!vm_rpupd && INCLUDES_VM` → `vm_rpupd = rpupd`；否则 `!rs_rpupd && INCLUDES_RS` → `rs_rpupd = rpupd`；
- **`rupdate_set_new_upd_flags`**（88-120）：`num_rpupds > 0` → `MULTI`；从 `last_rpupd` 传播 `INCLUDES_VM|INCLUDES_RS`；`PREPARE_ONLY` 提前返回；VM/RS 条目自动置 `INCLUDES_VM`/`INCLUDES_RS`；
- **`rupdate_upd_init`**（121-134）：零化 + `prepare_state_data_gid = GRANT_INVALID` + `state_endpoint = NONE` + `rp = rp`；
- **`rupdate_upd_clear`**（135-163）：清 `r_new_rp`（cleanup_service，15）+ revoke grants + free state data + 重新 init；
- **`rupdate_upd_move`**（164-183）：描述符随实例移动（`dst_rp->r_upd = src_rp->r_upd`）。

### 2.3 update_service 与 rollback_service（update.c:262-370）

- **`update_service(src_rpp, dst_rpp, swap_flag, sys_upd_flags)`**（262-329）：`srv_update`（230）→ `vm_update`（19）准备 VM；swap_slot 换入新实例 + pid/endpoint 重排（10）；失败时回滚；`swap_flag` 区分 `RS_SWAP`（restart 的 15 路径）与 LU 路径；
- **`rollback_service(new_rpp, old_rpp)`**（330-370）：RS 特例——`sys_whoami` + `vm_update(SF_VM_ROLLBACK)`（18），恢复旧实例为活动版本。

### 2.4 prepare 阶段（update.c:401-531）

- **`start_update_prepare(allow_retries)`**（401-466）：逐服务 `request_prepare_update_service`（19，发 `RS_LU_PREPARE`）；VM 在 multi 更新中最后处理；
- **`start_update_prepare_next`**（467-531）：VM multi 预分配推进（`vm_update` 准备下一组件）。

### 2.5 start_update（update.c:532-620）

- **`start_update()`**（532-620）：`RS_UPDATING` 置位；VM multi 时序（`rs_receive_ticks` 超时等待 VM 初始化，update.c:590——**唯一调用点**，ARCH A-9：IPC filter + `sys_setalarm2` 双源 → `receive_timeout` 原语，19 落地）；逐服务 `start_srv_update`。

### 2.6 start_srv_update / complete_srv_update（update.c:621-706）

- **`start_srv_update(rpupd)`**（621-656）：对单服务执行 update 动作（RS 自身条目 → YIELD 特例，18）；
- **`complete_srv_update(rpupd)`**（657-706）：update 完成后通知新实例开始 `SEF_INIT_LU` 初始化（12）。

### 2.7 abort_update_proc（update.c:707-743）

`abort_update_proc(reason)`（被 15 的 terminate_service 与 14 的 `RS_SYSCTL_UPD_STOP` 调用）：

1. `reason != OK` 断言；`!updating && !scheduled` → `EINVAL`（716-717）；
2. 未进行中（scheduled）→ `rupdate_clear_upds()` + `OK`（725-726）——直接清链；
3. 进行中且 `RS_INITIALIZING` → `end_update(reason, RS_REPLY)`（729-731）——假装当前服务初始化失败；
4. 否则 → `end_update(reason, RS_CANCEL)`（733-735）——假装 prepare 失败。

### 2.8 end_update 族（update.c:744-1011）

- **`end_update_curr`**（744-762）：当前描述符的结束动作——`result != OK && SRV_IS_UPDATING_AND_INITIALIZING(new_rp) && rpupd != rs_rpupd` → `rollback_service`，然后 `end_srv_update`；
- **`end_update_before_prepare`**（763-779）：还在等 prepare 的服务 → 只清理新版本，旧版本继续（`result != OK`）；
- **`end_update_prepare_done`**（780-794）：prepare 完成后被阻塞的服务 → `end_srv_update(rpupd, result, RS_REPLY)`；
- **`end_update_initializing`**（795-815）：初始化中的服务 → `result != OK && rpupd != rs_rpupd` → rollback；`end_srv_update(..., RS_REPLY)`；
- **`end_update_rev_iter`**（816-864）：从 `last_rpupd` 反向遍历，按当前/curr 相对位置与 `RUPDATE_IS_INITIALIZING()` 把每个非 prepare-only 条目分类为 `is_curr`/`is_before_prepare`/`is_prepare_done`/`is_initializing`，分派到四个 end 函数；`skip_rpupd`/`only_rpupd` 用于"VM 最后处理"（865 的 `end_update` 先跑非 VM、再跑 VM）；
- **`end_update_debug`**（865-931，宏 `end_update` proto.h:121-122）：RS 新实例 active 且失败 → `exit(1)`（883-887）；先取消所有 prepare-only（发 `SEF_LU_STATE_NULL` 取消消息）；`end_update_rev_iter` 两遍（非 VM / VM）；成功时逐描述符 `end_srv_init(new_rp)`（12）；`late_reply(last_rpupd->rp, result)`（06）；`rupdate_upd_clear(last)` + `RUPDATE_CLEAR()`；清所有 `old_endpoint`/`new_endpoint` 与 `SF_VM_UPDATE/ROLLBACK/NOMMAP` 标志；
- **`end_srv_update`**（932-1011）：单服务的结束——**surviving/exiting 选择**（964-968）：`result == OK ? (new, old) : (old, new)`；surviving 清 `RS_INITIALIZING` + `check_tm = 0` + `alive_tm = getticks()`（与 12 `ReadyDecision::FreshInitDone` 的 `SlotMutations` 同构，R13：16 接线复用 `mutations.apply` 提交）；断 `r_new_rp`/`r_old_rp` 链；清 `RS_UPDATING|PREPARE_DONE|INIT_DONE|INIT_PENDING`；`RS_REPLY` → reply；`RS_CANCEL` → 发取消消息；exiting 逐实例 `cleanup_service`（15，`SEF_LU_DETACHED` 时先置 `RS_CLEANUP_DETACH` 降级）。**VM multi 特例**（952-956）：`result == OK && new 是 VM && multi` → reply_flag 改 `RS_CANCEL`（VM 已在 multi 流程中被单独回复过）。

---

## 3. Rust 设计决策

### 3.1 live_update.rs 纯切片

与 13/14/15 同款：IPC/动作面（`clone_service`/`alloc_slot`/`init_slot`/`create_service`/`vm_memctl`/`vm_update`/`sys_whoami`/`request_prepare_update_service`/`rs_receive_ticks`）归 10/17/18/19，`end_srv_init` 归 12，`late_reply` 归 06，`cleanup_service` 归 15；`live_update.rs` 拥有**状态机本体与纯判定**：

| 函数 | C 对应 | 语义 |
|------|--------|------|
| `LuFlags` | sef.h:235-242 | 8 个 `SEF_LU_*` 位 |
| `UpdatePhase` + `update_phase(flags, num_rpupds)` | const.h:105,111 | `Idle/Scheduled/Updating/Initializing`（ARCH A-6） |
| `SEF_LU_STATE_NULL`/`SEF_LU_STATE_UNREACHABLE` | sef.h:213,219 | 状态值 0/5 |
| `resolve_prepare_maxtime(maxtime, default)` | request.c:653-655 | 0 → 默认（R32 改名：与 monitor 的同名校量函数区分） |
| `lu_flags_from_rss(rss, map_prealloc_bytes)` | request.c:574-623 | RSS_* → `(LuFlags, init_flags)` |
| `vm_default_prealloc(...)` | request.c:591-599 | VM 默认 mmap 预分配 |
| `validate_update_request(...)` | request.c:648-686 | 校验门 → `EBUSY`/`EINVAL`（含 NULL state） |
| `UpdateEntry` + `UpdateChain::add` | update.c:23-86 | 部分排序插入 + 标志传播（ARCH A-3） |
| `EndUpdateRole` + `end_update_role(...)` | update.c:822-847 | rev-iter 角色分类 |
| `AbortAction` + `abort_action(phase)` | update.c:707-743 | 阶段分派 |
| `RS_REPLY`/`RS_CANCEL` + `end_srv_reply_flag(...)` | const.h:75-76, update.c:952-956 | VM multi 取消判定 |

### 3.2 UpdatePhase 类型化（ARCH A-6）

C 用两个位（`RS_UPDATING`/`RS_INITIALIZING`）+ 三个宏（`RUPDATE_IS_UPDATING`/`IS_UPD_SCHEDULED`/`IS_UPD_MULTI`，const.h:105-113）表达全局状态，宏遍历（`RUPDATE_ITER`/`RUPDATE_REV_ITER`）在链上裸指针游走。Rust 侧：

- `UpdatePhase` 四态枚举，`update_phase` 从 `RupdateFlags` + `num_rpupds` 解码——**非法状态不可表达**（例如"Initializing 但 UPDATING 位未置"不可能构造）；
- 链遍历用 `UpdateChain` 的迭代器（正向 `RUPDATE_ITER` 对应 `iter()`、反向 `RUPDATE_REV_ITER` 对应 `rev_iter()`），杜绝裸指针；
- `abort_action`/`end_update_role` 以 `UpdatePhase` 为输入，编译器保证穷尽匹配。

> **[ARCH: A-6]** — C 位标志 + 宏遍历 → `UpdatePhase` enum + 显式链迭代器。行为对照点：`update.c:707-743`、`816-864`、`const.h:105-113`；design：本文档 §3.2；code：`live_update.rs::update_phase`/`abort_action`/`end_update_role`。

### 3.3 UpdateChain 索引链（ARCH A-3）

C 的 `rprocupd *prev_rpupd/next_rpupd` 双向裸指针链（type.h:40-41）→ `Vec<UpdateEntry>` + `Option<usize>` 索引链。`UpdateEntry` 自带 `endpoint`（插入排序需要），不触碰 `ServiceSlot`；`vm`/`rs` 指针用 `Option<usize>` 表达 `rupdate.vm_rpupd`/`rs_rpupd`（type.h:50-51）。

**P2-3 收口边界**：`UpdateEntry` 建模 rpupd 的决策相关字段（`lu_flags`/`init_flags`/`prepare_state`/`state_endpoint` + 链 + vm/rs 索引）；`prepare_tm`/`prepare_maxtime`（`clock_t` 计时，07 的 `update_period` 超时消费）与 grants（`prepare_state_data*`/`prepare_state_data_gid`，17 的 `rs_state_data` 形状 + cpf）**显式 DEFERRED**——02 的 P2-3 以"决策字段建模 + 计时/grants 显式 DEFERRED"关闭。

### 3.4 更新谓词注入

`validate_update_request` 的 `already_scheduled`（`SRV_IS_UPD_SCHEDULED(rp)`，const.h:120，读 `r_upd`）与 `phase`（全局）分离：全局态用 `UpdatePhase`（可纯解码），**单服务**的"已在链上"以布尔注入——与 15 的 `is_updating` 同款模式。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/rs/src/live_update.rs`：

- 常量/位：`LuFlags`（8 位）、`SEF_LU_STATE_NULL=0`/`SEF_LU_STATE_UNREACHABLE=5`、`RS_REPLY=1`/`RS_CANCEL=2`；
- 全局态：`UpdatePhase` + `update_phase`；
- 入口判定：`lu_flags_from_rss`/`vm_default_prealloc`/`resolve_prepare_maxtime`/`validate_update_request`；
- 链：`UpdateEntry`/`UpdateChain`（`add`/`iter`/`rev_iter`/`len`/`vm`/`rs` 访问器/`is_preparing_only`）；
- 结束/中止：`EndUpdateRole`/`end_update_role`/`AbortAction`/`abort_action`/`end_srv_reply_flag`。

### 4.2 关键不变量

1. **部分排序序不可破坏**（update.c:42-48）：链序恒为"普通 → … → VM → RS"；`add` 断言条目未链接（C 30-31）。
2. **标志传播是全链的**（update.c:64-67）：新条目的 `INCLUDES_VM|INCLUDES_RS|MULTI` 写进**每个**已存在条目——后插入的 multi 服务会影响先插入的服务（它们都要知道自己是 multi 的一部分）。
3. **`vm`/`rs` 指针只设一次**（update.c:69-72）：`!vm_rpupd && INCLUDES_VM` 才设——首个 VM 条目成为 `vm_rpupd`。
4. **`update_phase` 解码顺序**：`INITIALIZING` 优先于 `UPDATING`（初始化是进行中的子态）；`num_rpupds > 0 && !UPDATING` 才是 `Scheduled`（const.h:111）。
5. **`end_update_role` 精确复刻 rev-iter 语义**（update.c:822-847）：反向遍历中 `is_after_curr` 是累积状态（curr 之后的所有条目）；`initializing` 时 curr 之前的条目是 `Initializing` 角色，否则是 `PrepareDone`。
6. **`abort_action` 分派**：scheduled → 清链（无 end_update）；initializing → `EndWithReply`；updating → `EndWithCancel`（update.c:722-733）。
7. **`init_flags |= lu_flags`**（request.c:622-623）：LU 标志随 init 标志传给新实例——`lu_flags_from_rss` 的返回值保证这一关系。

---

## 5. 测试要点

`live_update.rs` 内测试（`cargo test -p minix-rs --lib live_update`，10 项）：

1. `update_phase`：四态解码（含 `num_rpupds>0` 的 `Scheduled`、`INITIALIZING` 优先）。
2. `lu_flags_from_rss`：`SELF_LU|FORCE_SELF_LU` → `SELF`；`PREPARE_ONLY_LU` → `PREPARE_ONLY`；`DETACH` 仅在非 prepare-only；`NOMMAP_LU`/`map_prealloc_bytes>0` → `NOMMAP`；`FORCE_INIT_*` → `SEF_INIT_*`；`init_flags` 含 lu。
3. `vm_default_prealloc`：VM + 非同一性 → 默认 8MB；其他 → 原值。
4. `validate_update_request`：校验门各自命中（含 `prepare_state == NULL` → `EINVAL`）+ 全通过。
5. `UpdateChain::add` 排序：RS 最后、VM 紧前、普通服务头部；插入后 `iter()` 顺序断言。
6. `test_chain_insert_between_vm_and_rs`：后插入的普通服务插到 VM/RS 之前（update.c:42-48）；`rev_iter()` 与 RUPDATE_REV_ITER 一致（update.c:98-104）。
7. `add` 标志传播：`INCLUDES_VM` 条目插入后全链 `lu_flags`/`init_flags` 含该位；`vm()` 指针指向首个。
8. `end_update_role`：四角色（`initializing` 真/假两族的 4 组合）。
9. `abort_action`：四阶段分派。
10. `resolve_prepare_maxtime`：0 → 默认值；非 0 → 原值（R32 改名）。
10. `end_srv_reply_flag`：VM + multi + 成功 → `RS_CANCEL`；否则原值。

测试总数声明：本文档范围为 `live_update` 模块测试数（以该模块 `cargo test` 输出为准）。全局 `cargo test -p minix-rs --lib` 通过数随并行模块增长（见 12 §5 的累计值约定）。

---

## 6. 过渡

Live Update 是 RS 机制图的"环"：

- **状态数据**：`init_state_data`/cpf grants 在 **17-rs-state-data** 展开——prepare 阶段的数据迁移依赖它；
- **RS 自身**：self update 的信号管理器备份、rollback 特例（`sys_whoami`/`vm_update(SF_VM_ROLLBACK)`）、RS YIELD 在 **18-rs-self-lifecycle** 展开；
- **外部契约**：`vm_memctl`/`vm_update`/`request_prepare_update_service`/`rs_receive_ticks`（ARCH A-9）在 **19-rs-external-interfaces** 落地；
- **超时**：`update_period`（07）在 prepare 超时后调 `end_update`——16 的调用点；
- **终止互斥**：15 的 `terminate_service` 在更新中退出时调 `abort_update_proc`（16）——两条状态机通过 abort 汇合。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` —— `UpdateChain`/`RupdateFlags` 数据形状
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md` —— `clone_service`/`update_service`/`swap_slot`/`activate_service`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md` —— `init_slot`/`inherit_service_defaults`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md` —— `run_service`/`end_srv_init`/`do_upd_ready`/`SEF_INIT_LU`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/15-rs-terminate-restart.md` —— `terminate_service` 的 abort 调用点
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/17-rs-state-data.md` —— `init_state_data`/cpf grants
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/18-rs-self-lifecycle.md` —— RS 自身 LU/rollback 特例
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/07-rs-period-heartbeat.md` —— `update_period`/超时
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/14-rs-query-requests.md` —— `RS_SYSCTL_UPD_*` 控制入口
- `minix3/minix/servers/rs/update.c`（1011 行）、`request.c:534-889`、`const.h:58,75-76,83,105-120`、`include/minix/sef.h:235-242` —— ground truth
