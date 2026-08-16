# 14-rs-query-requests: 查询请求

> **分类**: 阶段 4 — 服务生命周期（观测面）
> **源码**: `minix3/minix/servers/rs/request.c`（`do_getsysinfo`—1095、`do_lookup`—1144、`do_sysctl`—1181、`do_fi`—1229）、`minix3/minix/servers/rs/utility.c:69-77,142-222,485-546`（`fi_service`/`srv_to_string_gen`/`srv_upd_to_string`/`print_services_status`/`print_update_status`）
> **Rust 模块**: `os/servers/rs/src/query.rs`（`GetsysinfoTable`/`getsysinfo_table`/`NAME_BUF_LEN`/`lookup_name_len`/`SysctlAction`/`classify_sysctl`/`RS_FI_CRASH` + `SI_*`/`RS_SYSCTL_*` 常量）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`lookup_slot_by_label`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/04-rs-access-control.md`（`check_call_permission`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md`（`copy_label`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md`（`EDONTREPLY`/`rs_asynsend`）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md`（`start_update_prepare`/`abort_update_proc`）
> **说明**: 本文档是 RS 的**只读观测面**：13 回答了"如何操作服务的生命周期"，本文档回答"如何**看**服务的状态、导出进程表、触发故障注入"。四个 handler 中，`do_lookup`/`do_getsysinfo` 不改变任何槽位，`do_sysctl` 的 `UPD_*` 子功能委托 16，`do_fi` 是唯一的"注入"入口（把服务搞崩，然后交给 15 恢复）。

---

## 1. 概念：观测与注入

### 1.0 章节引言

13 的控制请求是生命周期的"操作杆"；本文档是**观测面板**：`RS_LOOKUP`（按名字查端点）、`RS_GETSYSINFO`（导出进程表快照）、`RS_SYSCTL`（服务状态打印 + update 控制入口）、`RS_FI`（故障注入）。前三个只读，第四个主动注入故障——但它不改变 RS 的槽位状态，而是把崩溃交给 15 的恢复路径，因此语义上仍属于"观测/调试面"。

> **本章不讲什么**（机制一律移交）:
> - 权限判定（`04-rs-access-control.md`）——`check_call_permission` 在 handler 内只被调用
> - `copy_label` 的消息面（`08-rs-slot-config.md`）
> - `start_update_prepare`/`abort_update_proc` 与 rpupd 链（`16-rs-live-update.md`）——`do_sysctl` 只委托
> - `sys_datacopy`/`rs_asynsend`/诊断输出面（`19-rs-external-interfaces.md`）
> - `EDONTREPLY` 与 `late_reply` 消费（`06-rs-main-loop.md`）
> - 服务崩溃后的恢复（`15-rs-terminate-restart.md`）——`RS_FI` 是崩溃的**触发源**之一

### 1.1 只读观测面（WHY）

RS 是唯一拥有**全部服务槽位**（`rproc`/`rprocpub`）的用户态服务器。其他组件需要"某个服务在哪个端点""现在系统上跑着哪些服务"这类事实时，只有两个来源：

- **`RS_LOOKUP`**——按 label 查 endpoint。这是 RS 的"电话簿"：`devman`、`service` 工具等通过它把服务名解析成可通信的端点号。
- **`RS_GETSYSINFO`**——把整张进程表（私有 + 公共）拷贝给调用者。C 里是**裸内存拷贝**（`sizeof(struct rproc) * NR_SYS_PROCS` 字节）；Rust 侧必须重新设计（见 §3.2）。
- **`RS_SYSCTL`**——5 个子功能：`SRV_STATUS` 打印服务清单；`UPD_*` 是 Live Update 的控制入口（16 的状态机）；`UPD_STATUS` 打印更新状态。

这四个 handler 的共同点是**不改变生命周期**（`do_fi` 注入的崩溃由内核/15 处理，RS 槽位只在 15 的恢复路径中变化）。

### 1.2 查询请求在主循环中的位置（WHAT）

06 的分发表把四类消息送进来：

```
主循环 dispatch（06）── request(call_nr)
  ├─ RS_LOOKUP    → do_lookup     ← 本文档 2.1：label → endpoint
  ├─ RS_GETSYSINFO→ do_getsysinfo ← 本文档 2.2：进程表导出
  ├─ RS_SYSCTL    → do_sysctl     ← 本文档 2.3：SRV_STATUS / UPD_*（→16）
  └─ RS_FI        → do_fi         ← 本文档 2.4：故障注入（→15 恢复）
```

它们位于主循环的 `RS_*` 请求分支，与 13 的控制请求同层；区别只在 handler 内部"只读" vs "写槽位"。

---

## 2. C 源码分析

### 2.1 do_lookup：label → endpoint（request.c:1144-1177）

`do_lookup` 是四个 handler 中最简单的一个：

1. 长度校验（request.c:1154-1157）：`name_len < 2 || name_len >= 100` → `EINVAL`（`namebuf` 是 `static char[100]`，request.c:1147）；
2. `sys_datacopy(m_source, name, SELF, namebuf, len)`（request.c:1159-1164）——从调用者地址空间拷名字（19 面）；
3. `namebuf[len] = '\0'`（request.c:1166）；
4. `lookup_slot_by_label(namebuf)`（request.c:1168-1171）——未命中 `ESRCH`（02：只查 `RS_ACTIVE` 实例）；
5. 回填 `m_rs_req.endpoint = rpub->endpoint`（request.c:1173）+ `OK`。

注意 `name_len` 的上界 100 是**本地 static 缓冲区**的尺寸，不是 `RS_MAX_LABEL_LEN`（16）——名字缓冲区允许比 label 长，`lookup_slot_by_label` 用 `strcmp` 比较时以 label 的 16 字节为实际语义。

### 2.2 do_getsysinfo：进程表导出（request.c:1095-1142）

`do_getsysinfo` 把 RS 的进程表按三种口径拷贝给调用者：

1. 权限（request.c:1104-1105）：`check_call_permission(m_source, 0, NULL)`——**注意 call 参数是 0**，不是某个 RS_* 消息号；由于 `rp == NULL`，04 的判定只剩 `caller_is_root`（manager.c:21-38；`check_call_permission` 内调用点 87）——**只有 root 能导出进程表**；
2. 口径选择（request.c:1111-1133）：
   - `SI_PROC_TAB`（=2，sysinfo.h:11）：拷 `rproc[]` 私有表，`len = sizeof(struct rproc) * NR_SYS_PROCS`；
   - `SI_PROCALL_TAB`（=12，sysinfo.h:16）：先拷私有表（`len > size` → `EINVAL`，request.c:1120-1121），`dst_addr += len; size -= len`，然后 **FALLTHROUGH** 到公共表；
   - `SI_PROCPUB_TAB`（=11，sysinfo.h:15）：拷 `rprocpub[]` 公共表；
   - default（request.c:1131-1132）→ `EINVAL`；
3. 最终 `len != size` → `EINVAL`（request.c:1135-1136）——调用者给的缓冲区必须**恰好**等于表的大小；
4. `sys_datacopy(SELF, src, dst_proc, dst_addr, len)`（request.c:1138）——19 面。

C 的 FALLTHROUGH 语义值得注意：`SI_PROCALL_TAB` 在**第一段拷贝成功之后**才检查第二段的 `len != size`（request.c:1122-1123 拷贝 → 1135-1136 检查）——也就是说 C 在 size 介于 64 行与 128 行字节之间时，会先写出半张表再返回 `EINVAL`。Rust 纯切片没有这个副作用（见 §3.2），对外可见的只有 errno。

### 2.3 do_sysctl：系统控制（request.c:1181-1228）

`do_sysctl` 按 `m_rs_req.subtype` 分派 5 个子功能：

| subtype | 值（com.h:485-489） | 行为 |
|---------|--------------------|------|
| `RS_SYSCTL_SRV_STATUS` | 1 | `print_services_status()`（utility.c:485） |
| `RS_SYSCTL_UPD_START` | 2 | `start_update_prepare(1)`（16）→ 若 `ESRCH` 视为 OK；`print_update_status()`；返回 OK（只准备不运行） |
| `RS_SYSCTL_UPD_RUN` | 3 | 同上，但**立即启动**：`rupdate.last_rpupd->rp` 写 `RS_LATEREPLY` + `r_caller` + `r_caller_request = RS_UPDATE` 三字段（request.c:1204-1207），返回 `EDONTREPLY`（完成后补发 reply） |
| `RS_SYSCTL_UPD_STOP` | 4 | `abort_update_proc(EINTR)`（16）→ `print_update_status()` → 返回结果 |
| `RS_SYSCTL_UPD_STATUS` | 5 | `print_update_status()` |
| default | — | `EINVAL`（request.c:1217-1219） |

两个 C 事实需要如实记录：

1. **`do_sysctl` 没有权限检查**——C 代码中没有任何 `check_call_permission` 调用（对比 do_getsysinfo 的 1104 行）。这是 ground truth，Rust 侧不自行补权限门（保持外部行为一致）；
2. `UPD_RUN` 的 LATEREPLY 三字段写在 `rupdate.last_rpupd->rp` 上——`last_rpupd` 是 16 的 rpupd 链成员，本文档只陈述调用点，三字段一致性由 16 落地时接线（`mark_late_reply` 原语见 13）。

### 2.4 do_fi：故障注入（request.c:1229-1260 + utility.c:69-77）

`do_fi` 是 RS 对单个服务的故障注入入口：

1. `copy_label`（request.c:1237-1241）——从调用者地址空间拷 label（08）；
2. `lookup_slot_by_label`（request.c:1244-1249）——未命中 `ESRCH`（`rs_verbose` 时打印）；
3. `check_call_permission(m_source, RS_FI, rp)`（request.c:1253-1254）——**这是唯一有目标槽位的权限检查**（04：root 或 control 权限）；
4. `fi_service(rp)`（request.c:1257-1259）→ `rs_asynsend(rp, &m, 0)`：
   - `m.m_type = COMMON_REQ_FI_CTL`（= `COMMON_RQ_BASE + 2` = `0xE02`，com.h:597,607）；
   - `m.m_lsys_fi_ctl.subtype = RS_FI_CRASH`（=1，com.h:492）；
   - `rs_asynsend` 是**非阻塞异步发送**（06），不等待服务处理。

注入 `RS_FI_CRASH` 后，服务收到 `COMMON_REQ_FI_CTL` 消息自毁，内核把崩溃通知给信号管理器/RS，随后进入 **15** 的 `crash_service` 恢复路径——所以 `do_fi` 是 15 状态机的"触发源"之一。

### 2.5 状态打印族（utility.c:142-222, 485-546）

三个打印函数共享两个格式化助手：

- **`srv_to_string_gen(rp, is_verbose)`**（utility.c:142-188）：一行服务描述。格式：
  ```
  service 'LABEL'<active><version>(slot N, ep E, pid P[, cmd C, script S, proc N, major D, flags 0x…, sys_flags 0x…])
  ```
  - `<active>`：`RS_ACTIVE` → `*`，否则空格（`srv_active_str` 宏，utility.c:158）；
  - `<version>`：有 `r_new_rp`/`r_next_rp` → `-`（更新/副本中的旧版本），有 `r_old_rp`/`r_prev_rp` → `+`，否则空格（`srv_version_str` 宏，utility.c:159-160）；
  - 空 cmd/script/proc_name 渲染为 `_`（`srv_str` 宏，utility.c:157）；
  - verbose 时附加 cmd/script/proc_name/dev_nr/flags/sys_flags；
- **`print_services_status()`**（utility.c:485-511）：`PRINT_SEP()` 分隔线 + 按 slot 序遍历所有 `RS_IN_USE` 槽打印 `srv_to_string_gen(rp, 1)`，统计 `RS_ACTIVE` 数（services）与 in-use 数（instances），末尾汇总；
- **`print_update_status()`**（utility.c:516-546）：无更新（`!RUPDATE_IS_UPDATING() && !RUPDATE_IS_UPD_SCHEDULED()`，const.h:105,111）→ 打印 "No update is in progress or scheduled"；否则打印 multi/single 头（`RUPDATE_IS_UPD_MULTI()` = `num_rpupds > 1`，const.h:112）+ `RS_UPDATING`/`RS_INITIALIZING` 位 + `rs_rpupd`/`vm_rpupd` 存在位，再用 `RUPDATE_ITER` 逐描述符打印 `srv_to_string(rp)` + `srv_upd_to_string(rpupd)`；
- **`srv_upd_to_string(rpupd)`**（utility.c:189-218）：单个 update 描述符的详细行——`lu_flags` 8 位（`SEF_LU_SELF/ASR/MULTI/PREPARE_ONLY/NOMMAP/DETACHED/INCLUDES_RS/INCLUDES_VM`）、`init_flags` 4 位（`SEF_INIT_FAIL/CRASH/TIMEOUT/DEFCB`）、`prepare_state`、`eval_addr`、`prepare_tm/maxtime`、endpoint、`state_data_gid`、prev/next 端点。

Rust 侧，整个打印族（格式化 + 输出）归 **19 的诊断输出面**：`query.rs` 不持有打印函数，只提供 `do_sysctl` 的 subtype 分类（§3.1）。`print_update_status` 的逐描述符部分还依赖 rpupd 链数据形状——该形状未建模（02 Gate B G1，P2-3 DEFERRED），随 16 落地。

---

## 3. Rust 设计决策

### 3.1 query.rs 纯分类切片

与 13 的 `request.rs` 同款：IPC 面（`sys_datacopy`/`rs_asynsend`/打印输出）归 19，`copy_label` 归 08，权限归 04，update 状态机归 16；`query.rs` 拥有**纯分类与校验**——把"这条请求是哪张表/哪个子功能/名字长度合不合法"的判定从 handler 里提出来，handler 组装时直接消费判定结果：

| 函数 | C 对应 | 语义 |
|------|--------|------|
| `GetsysinfoTable` + `getsysinfo_table(what)` | request.c:1111-1133 | `SI_*` 三口径枚举；未知 `what` → `EINVAL` |
| `NAME_BUF_LEN` + `lookup_name_len(len)` | request.c:1154-1157 | `len < 2 || len >= 100` → `EINVAL` |
| `SysctlAction` + `classify_sysctl(subtype)` | request.c:1185-1221 | 5 子功能枚举；未知 subtype → `EINVAL` |
| `RS_SYSCTL_*` 常量 | com.h:485-489 | 五个子功能号 |
| `RS_FI_CRASH` | com.h:492 | 故障注入子类型（=1） |
| `SI_PROC_TAB`/`SI_PROCPUB_TAB`/`SI_PROCALL_TAB` | sysinfo.h:11,15-16 | 三口径常量（=2/11/12） |

`COMMON_REQ_FI_CTL`（`0xE02`）消息构造与 `lookup_endpoint`（label → endpoint 解析）不落在 `query.rs`——它们分别是 19 的 IPC 面与 handler 组装的一部分；`query.rs` 保持"能判定、不动作"的纯切片边界。

### 3.2 表导出的选择模型（ARCH）

C 的 `do_getsysinfo` 拷出的是 `struct rproc`/`struct rprocpub` 的**原始内存**（`sizeof(struct rproc) * NR_SYS_PROCS` 字节），并在 handler 内完成全部尺寸校验（`len > size` / `len != size` → `EINVAL`）。Rust 侧 `ServiceSlot` 是合并后的类型化结构（ARCH A-2/A-3），其内存布局**不是** C ABI，不能安全地按字节拷出。演进决策：

- `getsysinfo_table(what)` 把"哪张表"（私/公/两者）选出来，`GetsysinfoTable` 枚举直接对应 C 的 switch 三分支；
- 字节长度、`sys_datacopy` 拷贝与尺寸校验（request.c:1113-1115,1120-1121）归 **19**：`minix-types` 定义 `RsProcTable` 快照类型时决定 wire format（参照 ipc/vm.rs 既有模式）；
- C 的"先拷贝第一段再失败"副作用（request.c:1122-1123 拷贝 → 1135-1136 检查）在纯分类中不可见，但对外 errno 结果一致。

> **[ARCH: A-2/A-3]** — C 裸字节表拷贝 → 类型化表选择（`GetsysinfoTable`）+ 19 序列化器。行为对照点：`request.c:1111-1138`；design：本文档 §3.2；code：`query.rs::getsysinfo_table`。

### 3.3 常量归属

`SI_*`（sysinfo.h:11,15-16）、`RS_SYSCTL_*`（com.h:485-489）、`RS_FI_CRASH`（com.h:492）落在 `query.rs`，与 `dispatch.rs` 既有 RS_* 本地常量同款模式：正式家在 minix-types（ARCH A-2），19/99 落地时迁移（见 §7 的 99）。`RS_*` 消息号本体已在 `os/libs/minix-types/src/ipc/rs.rs`（com.h:463-482）。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/rs/src/query.rs` 函数表见 §3.1：两个判定函数（`getsysinfo_table`/`classify_sysctl`）、一个校验函数（`lookup_name_len`）、两组常量（`SI_*`/`RS_SYSCTL_*`）+ `RS_FI_CRASH`。无状态、无 I/O——handler 组装与 IPC 面全部在调用方（19）。

### 4.2 关键不变量

1. **未知输入永不产生动作**：`getsysinfo_table` 的 default（request.c:1131-1132）与 `classify_sysctl` 的 default（request.c:1217-1219）都返回 `EINVAL`，不存在"静默忽略"分支。
2. **长度校验先于一切**（request.c:1152-1156）：`lookup_name_len` 在拷贝前拒绝 `len < 2 || len >= 100`，永不触碰表。
3. **分类与动作分离**：`query.rs` 只回答"是什么"，不回答"做什么"——`UPD_*` 动作委托 16，打印委托 19，拷贝委托 19，注入消息构造委托 19。
4. **表选择与字节编码分离**（ARCH）：`GetsysinfoTable` 是语义选择；C 布局尺寸不是 Rust 布局，字节编码只存在于 19 的序列化器。

---

## 5. 测试要点

`query.rs` 内测试（`cargo test -p minix-rs --lib query`）：

1. `getsysinfo_table`：三值映射到 `GetsysinfoTable::{ProcTab,ProcPubTab,ProcAllTab}`；未知值（`99`）→ `EINVAL`。
2. `lookup_name_len`：`0`/`1`/`100`/`101` → `EINVAL`；`2`/`99` → `Ok`。
3. `classify_sysctl`：5 子功能映射到 `SysctlAction` 五个变体；`0`/`6` → `EINVAL`。
4. `RS_FI_CRASH`：= 1（com.h:492）。

测试总数声明：本文档范围为 `query` 模块测试数（4 项，以该模块 `cargo test` 输出为准）。全局 `cargo test -p minix-rs --lib` 通过数随并行模块增长（见 12 §5 的累计值约定）。

---

## 6. 过渡

查询请求把观测面与生命周期的其余部分连接起来：

- **`RS_SYSCTL` 的 `UPD_*` 子功能**是 **16-rs-live-update** 状态机的控制入口：`UPD_START`/`UPD_RUN` 调 `start_update_prepare`，`UPD_STOP` 调 `abort_update_proc`，`UPD_RUN` 的 LATEREPLY 三字段消费点在 06/16；
- **`RS_FI` 注入崩溃**后，服务进入 **15-rs-terminate-restart** 的恢复路径（`crash_service`/backoff/restart）；
- **`RS_GETSYSINFO` 的序列化器**与 `RS_LOOKUP` 的消息面在 **19-rs-external-interfaces** 落地；`SI_*`/`COMMON_REQ_FI_CTL`/`RS_SYSCTL_*` 常量随 99/19 迁移到 minix-types。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` —— `lookup_slot_by_label`、`RProcTable`、`UpdateChain` 数据形状
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/04-rs-access-control.md` —— `check_call_permission`（`rp=NULL` 时仅 root）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md` —— `copy_label`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/13-rs-control-requests.md` —— 控制面（`mark_late_reply` 原语同源）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— `start_update_prepare`/`abort_update_proc`/rpupd 链（`UPD_*` 委托）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/15-rs-terminate-restart.md` —— `RS_FI` 触发后的恢复路径
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/06-rs-main-loop.md` —— `EDONTREPLY`/`rs_asynsend`/分发表
- `minix3/minix/servers/rs/request.c:1095-1263`、`utility.c:69-80,142-222,485-546` —— ground truth
- `minix3/minix/include/minix/sysinfo.h:11,15-16` —— `SI_*` 常量
- `minix3/minix/include/minix/com.h:485-492,597,607` —— `RS_SYSCTL_*`/`RS_FI_CRASH`/`COMMON_REQ_FI_CTL`
