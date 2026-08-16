# 18-rs-self-lifecycle: RS 自身生命周期

> **分类**: 阶段 6 — Live Update（RS 自身特例）
> **源码**: `minix3/minix/servers/rs/main.c:436-590`（boot 自升级 + `sef_cb_init_restart`/`sef_cb_init_lu`）、`minix3/minix/servers/rs/update.c:230-366`（`srv_update`/`update_service`/`rollback_service`）、`minix3/minix/servers/rs/utility.c:387-412`（`update_sig_mgrs`）、`minix3/minix/servers/rs/manager.c:760-780`（`clone_service` 的 RS 备份信号管理器）、`minix3/minix/servers/rs/const.h:34-35,79-80,105-115`、`minix3/minix/include/minix/rs.h:197-198`、`minix3/minix/include/minix/const.h:151,154`
> **Rust 模块**: `os/servers/rs/src/self_lifecycle.rs`（`SelfUpgradeRole`/`self_upgrade_role`/`SwapFlag`/`should_pre_swap`/`rollback_swap_flag`/`SrvUpdateAction`/`srv_update_action`/`should_end_update_on_restart`/`lu_init_invariants`/`is_rs_restart_replica`/`SigMgrUpdate`/`sig_mgr_updates`/`rollback_needs_vm_update`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md`（SEF 回调注册、boot 锚点）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md`（update 状态机）
> **说明**: 本文档是 RS **自身**的生命周期特例：SEF provider 的自举（restart/LU init 回调）、boot 自升级流程、RS 的 rollback 特例与备份信号管理器。它依赖 19（`srv_fork`/`vm_update`/`sys_whoami` 等外部契约）、16（`end_update`/update 状态机）、12（`init_service`）——本文档只落地**角色分派、分支判定与断言**（`self_lifecycle.rs`）。

---

## 1. 概念：RS 自己怎么更新自己

### 1.0 章节引言

RS 是 SEF **provider**（01）：别的服务怎么初始化、怎么被更新，是 RS 替它们执行的。但 RS 自己被更新时，没有别人来执行这个流程——RS 必须自己当自己的 RS。这产生了一批"自指"特例：boot 时 fork 出自己的新版、restart/LU 时按新版本初始化自己、rollback 时把自己换回去。

> **本章不讲什么**（机制一律移交）:
> - 一般服务的 LU（`16-rs-live-update.md`）——`update_service`/`rollback_service` 的通用路径
> - SEF 回调注册表（`01-rs-boot-init.md` §3.3）——`SefCallbacks`/ARCH A-7 表结构
> - init 协议（`12-rs-init-run.md`）——`init_service`/`SEF_INIT_*` 消息
> - 外部 SEF/VM/PM 契约（`19-rs-external-interfaces.md`）——`srv_fork`/`vm_update`/`sys_whoami`/`vm_memctl`
> - swap_slot/activate_service（`10-rs-service-create.md`）

### 1.1 为什么 RS 需要特殊生命周期（WHY）

RS 是 boot 映像中第 2 个运行的用户服务（com.h:61，`RS_PROC_NR=2`），是"加载并启动其余用户服务"的角色。它一旦崩溃或需要升级，**没有别的服务能替它完成重启/更新编排**——因此 RS 把自己也当作一个受管服务，用同一套 `update_service`/`rollback_service` 机制处理自己，但在三处特殊化：

1. **boot 自升级**（USE_LIVEUPDATE，main.c:436-491）：RS 启动完成 4 步 init 后，立即 fork 自己的新版并 LU 过去——保证运行中的 RS 总是"新版本"；
2. **restart/LU init 回调**（main.c:499-545、549-585）：SEF init 协议轮到自己时，用 stateful 转移 + `update_service(RS_DONTSWAP)` 接管旧实例的 slot；
3. **rollback 特例**（update.c:330-366）：RS 回滚时可能只需 swap slot（旧版 RS 还在跑），并用 `sys_whoami` 判断自己是否就是 RS。

### 1.2 四个特例（WHAT）

| 特例 | C 锚点 | 关键机制 | 纯切片 |
|------|--------|---------|--------|
| boot 自升级 | main.c:436-491 | clone_slot + srv_fork + update_service(RS_SWAP) + cpf_reload + cleanup + vm_memctl(PIN) / SET_SYS + sched_init_proc + YIELD | `SelfUpgradeRole`（新/旧实例） |
| restart init | main.c:499-545 | stateful 转移 → 进行中 update 结束 → update_service(RS_DONTSWAP) → init_service → alarm | `should_end_update_on_restart` |
| LU init | main.c:549-585 | SEF_CB_INIT_LU_DEFAULT → update_service(RS_DONTSWAP) → 四断言 | `lu_init_invariants` |
| rollback + 备份信号管理器 | update.c:330-366 + utility.c:387-412 | sys_whoami/vm_update(ROLLBACK)/心跳重置；ROOT_SYS_PROC\|RST_SYS_PROC 触发 | `rollback_*`/`sig_mgr_updates`/`is_rs_restart_replica` |

---

## 2. C 源码分析

### 2.1 boot 自升级（main.c:436-491）

4 步 boot 完成、设置周期 alarm（main.c:433 `sys_setalarm(RS_DELTA_T)`）之后，进入 `USE_LIVEUPDATE` 分支（main.c:436-491）：

1. **clone_slot**（441-444）：克隆 RS 自己的 slot 到 `replica_rp`；
2. **srv_fork(0, 0)**（446-449）：fork 出 RS 副本（root:wheel）；`replica_pid = pid ? pid : getpid()`，`getprocnr` 拿 replica endpoint（450-454）；
3. **pid == 0（新实例）**（456-475）：`update_service(&rp, &replica_rp, RS_SWAP, 0)` 把旧 RS 换入新 RS（RS_SWAP 触发 `srv_update` 先换内核 slot）；`cpf_reload()` 重载状态 grants（17）；`cleanup_service(rp)` 清理旧实例；`vm_memctl(RS_PROC_NR, VM_RS_MEM_PIN)` 钉住新 RS 内存；
4. **pid != 0（旧实例）**（477-489）：`sys_privctl(replica_endpoint, SYS_PRIV_SET_SYS, &r_priv)` 设置新实例权限；`sched_init_proc(replica_rp)` 初始化调度；`sys_privctl(replica_endpoint, SYS_PRIV_YIELD)` 让出控制权给新实例（YIELD 后旧实例不再返回，`NOT_REACHABLE`）。

### 2.2 sef_cb_init_restart（main.c:499-545）

RS 被重启后的 init 回调：

1. `assert(info->endpoint == RS_PROC_NR)`；
2. **stateful 转移**：`SEF_CB_INIT_RESTART_STATEFUL(type, info)`（libsys 通用实现：从旧实例拷贝状态）——失败则返回；
3. 新 RS 接管：`old_rs_rp = rproc_ptr[RS_PROC_NR]`、`new_rs_rp = rproc_ptr[info->old_endpoint]`；
4. **若 update 进行中**：`SRV_IS_UPDATING(old_rs_rp)` → `end_update(ERESTART, RS_REPLY)`（16 的机制：以 ERESTART 理由结束 update）；
5. `update_service(&old_rs_rp, &new_rs_rp, RS_DONTSWAP, 0)`——**不预换**（restart 场景下 slot 已经就位）；
6. `init_service(new_rs_rp, SEF_INIT_RESTART, 0)`（12）；`sys_setalarm(RS_DELTA_T)` 重排周期 alarm。R14：12 的 `init_service` 建模拆为 `mark_initializing(slot, ticks)`（utility.c:19-21 的 `RS_INITIALIZING`/`alive_tm`/`check_tm` 三行变异）+ `init_message` 载荷——RS 自重启路径同样先置位再发 `RS_INIT`，否则 ready 门（request.c:477-483）拒绝。

### 2.3 sef_cb_init_lu（main.c:549-585）

RS 被 LU 后的 init 回调：

1. `assert(info->endpoint == RS_PROC_NR)`；
2. `sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL)`——把 restart 回调换成 stateful 版（LU 之后若再重启，需要状态转移）；
3. `SEF_CB_INIT_LU_DEFAULT(type, info)`（libsys 通用实现）——失败则返回；
4. 新旧 RS 定位（同 2.2）；`update_service(&old_rs_rp, &new_rs_rp, RS_DONTSWAP, 0)`；
5. **四断言**（main.c:580-583）：`RUPDATE_IS_UPDATING()` ∧ `RUPDATE_IS_INITIALIZING()` ∧ `num_rpupds > 0` ∧ `num_init_ready_pending > 0`——保证 LU 状态机处于正确阶段。

### 2.4 rollback_service 的 RS 特例（update.c:330-366）

`rollback_service(new_rpp, old_rpp)` 对 RS 特殊处理：

```c
if ((*old_rpp)->r_pub->endpoint == RS_PROC_NR) {
    sys_whoami(&me, name, sizeof(name), &priv_flags, &init_flags);  /* 342 */
    if (me != RS_PROC_NR) {
        vm_update(new->endpoint, old->endpoint, SF_VM_ROLLBACK);     /* 345-346 */
    }
    /* RS rollback 后可能错过心跳回复，重发请求 */
    for (rp = BEG_RPROC_ADDR; rp < END_RPROC_ADDR; rp++)
        if (rp->r_flags & RS_ACTIVE)
            rp->r_check_tm = 0;                                      /* 350-352 */
}
else {
    swap_flag = RS_INIT_PENDING ? RS_DONTSWAP : RS_SWAP;
    if (swap_flag == RS_SWAP) sys_privctl(new, SYS_PRIV_DISALLOW, NULL);  /* 冻结新实例 */
    update_service(new_rpp, old_rpp, swap_flag, SF_VM_ROLLBACK);
}
assert(r == OK);
```

要点：旧实例是 RS 时，若当前进程**就是** RS（`me == RS_PROC_NR`，即 RS 在自回滚场景下运行），只需 swap slot；否则（旧 RS 已死，由新 RS 进程执行回滚）才需要 `vm_update(SF_VM_ROLLBACK)` 把内核 slot 换回。回滚后把全部 `RS_ACTIVE` 服务的 `r_check_tm` 清零——RS 可能错过了心跳周期，强制立刻重检。

`SF_VM_ROLLBACK` 有两个传递通道：RS 特例分支在 `me != RS` 时**直接** `vm_update(SF_VM_ROLLBACK)`（update.c:345）；else 分支（一般服务回滚）则经 `update_service(..., SF_VM_ROLLBACK)`（update.c:362）→ `srv_update` → `vm_update` 间接传给 VM。

### 2.5 srv_update 的 VM multi 时序（update.c:230-257）

`srv_update(src_e, dst_e, sys_upd_flags)` 选择内核/VM 的 slot 交换通道：

```c
if (src_e == VM_PROC_NR) {
    sys_update(src_e, dst_e, sys_upd_flags & SF_VM_ROLLBACK ? SYS_UPD_ROLLBACK : 0);
} else if (!RUPDATE_IS_UPD_VM_MULTI() || RUPDATE_IS_VM_INIT_DONE()) {
    vm_update(src_e, dst_e, sys_upd_flags);
} else {
    /* skip：VM 参与 multi-component 更新时，由 VM 在 state transfer 时统一处理 */
}
```

- **VM 是源**：只做内核部分（`sys_update`），VM 自身的新实例在初始化时完成其余部分；回滚时传 `SYS_UPD_ROLLBACK`；
- **非 VM**：`!VM-multi || VM init 完成` → `vm_update`；**VM-multi 且 VM 尚未 init 完成** → 跳过（VM 会在 state transfer / rollback 时统一处理，避免提前交换 slot 破坏时序）。

### 2.6 update_sig_mgrs 备份信号管理器（utility.c:387-412 + manager.c:766-778）

RS 重启时，其他服务可能正在以 RS 为信号管理器——RS 挂了它们就收不到信号。`clone_service` 在创建 RS 的 replica 时（manager.c:766-767）检查：

```c
rs_flags = ROOT_SYS_PROC | RST_SYS_PROC;            /* 0x100 | 0x800 = 0x900 */
if ((replica_rp->r_priv.s_flags & rs_flags) == rs_flags) {
    update_sig_mgrs(rs_rp,     SELF, replica_rpub->endpoint);  /* RS 的备份 = replica */
    update_sig_mgrs(replica_rp, SELF, NONE);                    /* replica 无备份 */
}
```

`update_sig_mgrs`（utility.c:387-412）：`sys_getpriv` 同步权限结构 → 设置 `s_sig_mgr = sig_mgr`、`s_bak_sig_mgr = bak_sig_mgr` → `sys_privctl(SYS_PRIV_UPDATE_SYS)` 写回内核。效果：RS 挂掉时内核自动把它的服务转给备份信号管理器（replica）。

> **Rust 映射（T5，2026-08-16）**：`sched.rs::update_sig_mgrs` 拆分为纯核心 `set_sig_mgrs`
> （接收 shell 的 `sys.getpriv` 结果，返回 `SigMgrCommit`）+ shell 提交（`privctl(UpdateSys)`）。
> 12/16 的调用方执行顺序：`let synced = sys.getpriv(endpoint)?;` →
> `let c = set_sig_mgrs(&mut priv_, synced, ep, sig_mgr, bak);` →
> `sys.privctl(c.endpoint, PrivCtlOp::UpdateSys, Some(c.priv_))?;`（03 §3.7）。

---

## 3. Rust 设计决策

### 3.1 self_lifecycle.rs 纯切片

与 16/17 同款：IPC/内核面（`srv_fork`/`getprocnr`/`sys_privctl`/`sched_init_proc`/`vm_memctl`/`vm_update`/`sys_update`/`sys_whoami`/`cpf_reload`）归 19，`end_update` 归 16，`init_service` 归 12，`clone_slot`/`swap_slot`/`activate_service`/`cleanup_service` 归 10/15；`self_lifecycle.rs` 拥有**角色分派、分支判定与断言**：

| Rust | C 锚点 | 语义 |
|------|--------|------|
| `SelfUpgradeRole`/`self_upgrade_role` | main.c:456 | `pid==0` → 新实例 |
| `SwapFlag`/`should_pre_swap` | const.h:79-80 + update.c:287-288 | `RS_SWAP` 才预换 |
| `rollback_swap_flag` | update.c:355 | `RS_INIT_PENDING` → `RS_DONTSWAP` |
| `SrvUpdateAction`/`srv_update_action` | update.c:230-257 | sys_update/vm_update/skip 三分支 |
| `should_end_update_on_restart` | main.c:521-522 | restart 时结束进行中 update |
| `lu_init_invariants` | main.c:580-583 | LU init 四断言 |
| `is_rs_restart_replica` | manager.c:766-767 | `0x900` 掩码 |
| `SigMgrUpdate`/`sig_mgr_updates` | manager.c:771-773 | 备份信号管理器对 |
| `rollback_needs_vm_update` | update.c:342-345 | `me != RS_PROC_NR` |

### 3.2 类型化

- `SwapFlag { DontSwap, Swap }`：取代 C 的 `RS_DONTSWAP=0`/`RS_SWAP=1` 裸 int——非法值不可表达；
- `SrvUpdateAction { SysUpdate { rollback: bool }, VmUpdate, Skip }`：`srv_update` 的三分支收敛为一个枚举，`rollback` 布尔对应 `sys_upd_flags & SF_VM_ROLLBACK`（rs.h:198）是否置位；
- `SelfUpgradeRole { NewInstance, OldInstance }`：boot 自升级的 fork 分支。

`RUPDATE_IS_UPD_VM_MULTI()`（const.h:113：`vm_rpupd && num_rpupds > 1`）与 `RUPDATE_IS_VM_INIT_DONE()`（const.h:107：VM 的 `RS_INIT_DONE`）是外部状态，以 `is_vm_multi`/`vm_init_done` 布尔注入。

### 3.3 不变量

1. **restart 的 update 结束门**（main.c:521-522）：`old_rs_rp` 处于 `RS_UPDATING` 才 `end_update(ERESTART, RS_REPLY)`——RS 重启不能遗留半途的 update。
2. **LU init 四断言**（main.c:580-583）：`UPDATING` ∧ `INITIALIZING` ∧ `num_rpupds>0` ∧ `num_init_ready_pending>0`——全过才认为 LU 状态机就位。
3. **RS-replica 掩码**（manager.c:766-767）：`(s_flags & (ROOT_SYS_PROC|RST_SYS_PROC)) == (ROOT_SYS_PROC|RST_SYS_PROC)`——同时是 root 系统进程**且**是被重启的系统进程实例才需要备份信号管理器。
4. **RS rollback 的 VM 通道**（update.c:342-345）：只有 `me != RS_PROC_NR` 才 `vm_update(SF_VM_ROLLBACK)`——RS 自回滚只需 swap slot。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/rs/src/self_lifecycle.rs`：

- `SwapFlag`/`SelfUpgradeRole`/`SrvUpdateAction`/`SigMgrUpdate`：类型化枚举/结构；
- 判定函数：`self_upgrade_role`/`should_pre_swap`/`rollback_swap_flag`/`srv_update_action`/`should_end_update_on_restart`/`lu_init_invariants`/`is_rs_restart_replica`/`sig_mgr_updates`/`rollback_needs_vm_update`；
- 复用：`PrivFlags`（privilege.rs，`ROOT_SYS_PROC`/`RST_SYS_PROC`）、`SysFlags`（service_slot.rs，`VM_ROLLBACK`）、`RupdateFlags`（process_table.rs，`UPDATING`/`INITIALIZING`）。

### 4.2 关键不变量

1. `SwapFlag` 位值对应 const.h:79-80（`RS_DONTSWAP=0`/`RS_SWAP=1`）。
2. `srv_update_action` 三分支互斥：VM 源 → `SysUpdate`；否则 `!multi || init_done` → `VmUpdate`；否则 `Skip`。
3. `sig_mgr_updates` 仅在 `is_rs_restart_replica` 时返回 `Some`；RS 实例的备份 = replica endpoint，replica 的备份 = `NONE`。
4. `lu_init_invariants` 全真才允许继续（C 是 assert，Rust 是布尔判定——调用方决定 fail-closed 策略）。

---

## 5. 测试要点

`self_lifecycle.rs` 内测试（`cargo test -p minix-rs --lib self_lifecycle`，10 项，10/10 已落地）：

1. `self_upgrade_role`：`pid==0` → NewInstance；非 0 → OldInstance。
2. `should_pre_swap`：`Swap` → true；`DontSwap` → false。
3. `rollback_swap_flag`：`init_pending` → `DontSwap`；否则 `Swap`。
4. `srv_update_action`：VM 源 → `SysUpdate{rollback}`（`SF_VM_ROLLBACK` 位测试）；非 VM + `!multi || init_done` → `VmUpdate`；非 VM + multi 且未 init → `Skip`。
5. `should_end_update_on_restart`：true/false 两路。
6. `lu_init_invariants`：四断言组合（全真 → true；任一假 → false）。
7. `is_rs_restart_replica`：`0x900` → true；`0x100`/`0` → false。
8. `sig_mgr_updates`：RS-replica → `Some(({SELF, replica}, {SELF, NONE}))`；否则 None。
9. `rollback_needs_vm_update`：`me == RS` → false；`me == PM` → true。
10. `test_constants`：`ROOT_SYS_PROC|RST_SYS_PROC == 0x900`（const.h:79-80）、`VM_ROLLBACK == 0x080`（rs.h:198）、`UPDATING == 0x080`/`INITIALIZING == 0x040`（const.h:151,154）。
10. 常量表：`SwapFlag` 位值、`ROOT_SYS_PROC|RST_SYS_PROC == 0x900`、`SF_VM_ROLLBACK == 0x080`。

测试总数声明：本文档范围为 `self_lifecycle` 模块测试数（以该模块 `cargo test` 输出为准）。全局 `cargo test -p minix-rs --lib` 通过数随并行模块增长（见 12 §5 的累计值约定）。

---

## 6. 过渡

RS 自身生命周期是 LU 机制图的"自指环"：

- **上游**：01（SEF 回调注册表）→ 本 doc（回调机制）；16（update 状态机）→ 本 doc（RS 的 swap/rollback 特例）；
- **下游**：`init_service`/`SEF_INIT_RESTART`/`SEF_INIT_LU` 的 init 协议在 **12-rs-init-run** 展开；`srv_fork`/`vm_update`/`sys_whoami`/`vm_memctl`/`sys_privctl` 在 **19-rs-external-interfaces** 落地；`cpf_reload` 依赖 **17-rs-state-data** 的 grants 生命周期；
- **rollback 汇合**：RS rollback（本 doc）与一般服务 rollback（16）共用 `update_service`——swap 机制在 10。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md` —— SEF 回调注册表（ARCH A-7）、boot 锚点
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— `update_service`/`rollback_service` 通用路径、`end_update`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md` —— `init_service`/`SEF_INIT_*`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/17-rs-state-data.md` —— `cpf_reload`/grants 生命周期
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md` —— `clone_slot`/`swap_slot`/`activate_service`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/19-rs-external-interfaces.md` —— `srv_fork`/`vm_update`/`sys_whoami`/`sys_privctl` 契约
- `minix3/minix/servers/rs/main.c:436-590`、`update.c:230-366`、`utility.c:387-412`、`manager.c:760-780`、`const.h:34-35,79-80,105-115`、`include/minix/rs.h:197-198`、`include/minix/const.h:151,154` —— ground truth
