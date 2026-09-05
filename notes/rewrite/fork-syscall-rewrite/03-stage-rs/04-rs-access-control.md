# 04-rs-access-control: 调用者访问控制

> **分类**: 阶段 2 — 权限与隔离（boot Step 1 的机制之一：请求入口授权）
> **源码**: `minix3/minix/servers/rs/manager.c:21-130`（`caller_is_root`/`caller_can_control`/`check_call_permission`）、`minix3/minix/servers/rs/request.c`（11 个调用点）、`minix3/minix/lib/libsys/getepinfo.c:35-44`（`getnuid`）、`minix3/minix/servers/rs/const.h:105`（`RUPDATE_IS_UPDATING`）、`minix3/minix/include/minix/com.h:463-492`（RS_* 消息常量）
> **Rust 模块**: `os/servers/rs/src/access.rs`（`caller_is_root`/`caller_can_control`/`check_call_permission`）、`os/servers/rs/src/boot.rs`（`KernelApi::getnuid`）、`os/libs/minix-types/src/ipc/rs.rs`（RS_* 常量模块，ARCH A-2 第一步）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md`（主循环分类）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`r_flags`/`sys_flags`/`r_control` 字段归属、endpoint 索引）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/03-rs-privilege.md`（`s_flags & SYS_PROC` 判定、privctl 操作面）
> **说明**: 内核 priv 表管"服务能做什么"（03），RS 的访问控制管"谁有资格命令 RS"。每个 `RS_*` 控制请求在分派前先过 `check_call_permission`：root 或隔离策略（`r_control` 列表）二选一授权，再按目标槽状态应用五条规则。本文档建模两级授权模型、目标槽规则、11 个调用点、`getnuid` 外部依赖与 `RUPDATE_IS_UPDATING` 的消费规则。

---

## 1. 概念：谁有资格命令 RS

### 1.0 章节引言

RS 是系统服务的"管家"：它负责启动、停止、重启、更新服务（13~16 各文档）。如果任何人都能发 `RS_DOWN` 停掉一个服务，系统就毫无安全性可言。本文档回答的问题是：**一个 `RS_*` 请求到达主循环后，RS 凭什么决定"这个调用者可以对这（些）服务做这个操作"**。

> **本章不讲什么**（机制一律移交）:
> - 权限结构的构造与 privctl 操作（`03-rs-privilege.md`）——本文档只消费 `r_priv.s_flags & SYS_PROC` 一个位
> - `r_control` 列表的填充来源（`check_request`/`init_slot`/`edit_slot`，`08-rs-slot-config.md`）——本文档只读
> - 各请求 handler 的具体语义（`13-rs-control-requests.md`、`14-rs-query-requests.md`、`16-rs-live-update.md`）——本文档只做入口检查
> - reply/EDONTREPLY 机制（`06-rs-main-loop.md`）
> - `RUPDATE_IS_UPDATING` 背后的 Live Update 状态机（`16-rs-live-update.md`）——本文档只陈述"update 进行中 → EBUSY"规则
>
> 本章只回答一个问题：**两级授权模型 + 五条目标槽规则，每一条的 C 行号与语义是什么**。

### 1.1 核心问题：为什么 RS 需要自己的访问控制

内核的 priv 表（03）决定**服务能调用哪些内核接口、能向谁发 IPC**——这是"能力"层面的隔离。但"谁能指挥 RS 去启动/停止/更新一个服务"是**策略**层面的问题，内核不知道，因为内核不认识 `RS_UP` 请求。

Minix3 的答案（`manager.c:21-130`）是**两级授权**：

```
调用者（endpoint）
  │
  ├─ 通道 1: caller_is_root（getnuid → euid == 0）   —— root 一切
  │
  └─ 通道 2: caller_can_control（隔离策略）            —— 控制列表内的服务
        仅当请求有目标槽（rp != NULL）时启用
  │
  ▼
通道通过后，再按目标槽状态应用五条规则（仅 rp != NULL 时）
```

两级授权的差异是关键：`RS_UP`（启动新服务）、`RS_SHUTDOWN`（系统关停）、`RS_GETSYSINFO`（查询系统信息）**没有目标槽**（`rp == NULL`），所以只有 root 能做；其余请求有目标槽，控制列表内的服务也能触发。

> **术语**：本文的"root"指**有效用户 ID 为 0**（`euid == 0`），由 PM 的 `PM_GETEPINFO` 查询（§2.1），不是 RS 自己的判断。

### 1.2 目标槽规则：五条（WHAT）

对带目标槽的请求，`check_call_permission` 按顺序应用五条规则，任一失败即返回错误：

| # | 规则 | C 位置 | 错误 | 语义 |
|---|------|--------|------|------|
| 1 | 目标必须是系统进程，除非是 `RS_EDIT` | manager.c:103-105 | EPERM | 用户进程只能被编辑，不能被 UP/DOWN/RESTART/… |
| 2 | Live Update 进行中禁止任何调用 | manager.c:108-110 | EBUSY | `RUPDATE_IS_UPDATING()`（const.h:105）；防更新期间状态被并发修改 |
| 3 | 目标已有调用进行中（late reply / 初始化中）禁止再调用 | manager.c:113-116 | EBUSY | `RS_LATEREPLY`/`RS_INITIALIZING` 位；防重入 |
| 4 | 目标已终止，只允许 `RS_DOWN`/`RS_RESTART` | manager.c:119-121 | EPERM | 终止态是"清理/复活"专用通道 |
| 5 | 核心服务禁 `RS_DOWN` | manager.c:124-126 | EPERM | `SF_CORE_SRV`；核心服务（如 RS/VM/PM）不能被停 |

五条规则有明确顺序：先权限（1），再并发（2/3），再状态（4/5）。规则 2 是 update 状态机的唯一消费点——本文档只陈述规则，状态机在 16 展开。

### 1.3 为什么是"root 或 控制列表"而不是"root 且 控制列表"

`caller_is_root` 与 `caller_can_control` 是**或**关系（manager.c:91-94）：

```c
call_allowed = caller_is_root(caller);
if(rp) {
    call_allowed |= caller_can_control(caller, rp);
}
```

设计意图：root 是"管理员"通道，控制列表是"服务间协作"通道。典型例子：VM 在 `r_control` 里列出它需要控制的更新目标服务，从而在不需要 root 的情况下参与 Live Update 的准备（16 展开）。root 通道不依赖 `r_control` 填充，因此 boot 早期（控制列表尚未配置）root 仍可驱动 RS。

---

## 2. C 源码分析

### 2.1 `caller_is_root`（manager.c:21-34）+ `getnuid`（getepinfo.c:35-44）

```c
static int caller_is_root(endpoint)                     /* manager.c:21 */
endpoint_t endpoint;
{
  uid_t euid;

  euid = getnuid(endpoint);                             /* manager.c:27 */
  if (rs_verbose && euid != 0)
	printf("RS: got unauthorized request from endpoint %d\n", endpoint);

  return euid == 0;                                     /* manager.c:33 */
}
```

`getnuid`（`lib/libsys/getepinfo.c:35-44`）向 PM 发 `PM_GETEPINFO` 查询进程的有效 uid：

```c
uid_t
getnuid(endpoint_t proc_ep)                             /* getepinfo.c:35 */
{
	uid_t uid;
	int r;

	if ((r = getepinfo(proc_ep, &uid, NULL)) < 0)       /* getepinfo.c:40 */
		return (uid_t) r;                              /* 负 errno 转 uid_t */

	return uid;
}
```

**关键语义（fail-closed）**：`getepinfo` 失败时返回负 errno（如 `-ENOENT`），C 里被强转成 `uid_t`（无符号），**任何负 errno 转成 uid_t 后都不可能是 0**——所以 PM 不可达/端点不存在时 `euid != 0`，调用者被拒。这个"错误即拒绝"是刻意设计：访问控制失败必须关闭大门，不能打开。

### 2.2 `caller_can_control`（manager.c:39-76）

```c
static int caller_can_control(endpoint, target_rp)      /* manager.c:39 */
endpoint_t endpoint;
struct rproc *target_rp;
{
  int control_allowed = 0;
  register struct rproc *rp;
  register struct rprocpub *rpub;
  char *proc_name;
  int c;

  proc_name = target_rp->r_pub->proc_name;              /* manager.c:49 */

  /* 在 rproc 表里找调用者自己的槽（endpoint 匹配） */
  for (rp = BEG_RPROC_ADDR; rp < END_RPROC_ADDR; rp++) {
	if (!(rp->r_flags & RS_IN_USE))                     /* manager.c:53 */
		continue;

	rpub = rp->r_pub;
	if (rpub->endpoint == endpoint) {                   /* manager.c:57 */
		break;
	}
  }
  if (rp == END_RPROC_ADDR) return 0;                   /* manager.c:61 找不到调用者 → 拒绝 */

  /* 扫调用者的隔离策略列表 */
  for (c = 0; c < rp->r_nr_control; c++) {
	if (strcmp(rp->r_control[c], proc_name) == 0) {     /* manager.c:64 */
		control_allowed = 1;
		break;
	}
  }

  return control_allowed;                               /* manager.c:75 */
}
```

语义要点：

1. **调用者必须是 RS 表内服务**（`RS_IN_USE` 且 endpoint 匹配，manager.c:52-60）。表外端点（如普通用户进程）直接拒绝——C 用"扫描到表尾"表示找不到（manager.c:61），Rust 用 `endpoint_slot()` 的 `None` 等价（ARCH A-4）。
2. **`RS_IN_USE` 复核保留在访问层（R30，2026-09-06）**：C 的扫描每行都验 `RS_IN_USE`（manager.c:52-53），而 Rust 的 `endpoint_slot()` 是裸 `rproc_ptr` 镜像（重组中的行合法流经它，见 02 §3.5）。因此本函数在索引命中后**显式复核 in-use**（access.rs，fail-closed）——陈旧索引条目解析到已释放行时拒绝授权，等价于 C 的"扫描跳过非 in-use 行"。测试：`test_caller_can_control_skips_non_in_use_caller_row`。
3. **匹配对象是目标的 `proc_name`**（`r_pub->proc_name`），不是 label。`proc_name` 是进程可执行名（如 `"vm"`、`"pm"`），`label` 是发布名（如 `"service"`、`"vm"`）——两者在 boot 后通常相同，但隔离策略按可执行名匹配（与 `RS_LOOKUP` 的 label 匹配区分，14 展开）。
4. 列表长度 `r_nr_control` 与内容 `r_control[]` 的填充发生在请求参数校验阶段（`check_request`/`init_slot`，08），本文档只读不写。

### 2.3 `check_call_permission`（manager.c:81-130）

```c
int check_call_permission(caller, call, rp)             /* manager.c:81 */
endpoint_t caller;
int call;
struct rproc *rp;
{
  struct rprocpub *rpub;
  int call_allowed;

  /* Caller should be either root or have control privileges. */
  call_allowed = caller_is_root(caller);                /* manager.c:91 */
  if(rp) {
      call_allowed |= caller_can_control(caller, rp);   /* manager.c:93 */
  }
  if(!call_allowed) {
      return EPERM;                                     /* manager.c:96 */
  }

  if(rp) {
      rpub = rp->r_pub;

      /* Only allow RS_EDIT if the target is a user process. */
      if(!(rp->r_priv.s_flags & SYS_PROC)) {            /* manager.c:103 */
          if(call != RS_EDIT) return EPERM;             /* manager.c:104 */
      }

      /* Disallow the call if an update is in progress. */
      if(RUPDATE_IS_UPDATING()) {                       /* manager.c:108 */
      	  return EBUSY;
      }

      /* Disallow the call if another call is in progress for the service. */
      if((rp->r_flags & RS_LATEREPLY)                   /* manager.c:113 */
          || (rp->r_flags & RS_INITIALIZING)) {         /* manager.c:114 */
          return EBUSY;
      }

      /* Only allow RS_DOWN and RS_RESTART if the service has terminated. */
      if(rp->r_flags & RS_TERMINATED) {                 /* manager.c:119 */
          if(call != RS_DOWN && call != RS_RESTART) return EPERM; /* manager.c:120 */
      }

      /* Disallow RS_DOWN for core system services. */
      if (rpub->sys_flags & SF_CORE_SRV) {              /* manager.c:124 */
          if(call == RS_DOWN) return EPERM;             /* manager.c:125 */
      }
  }

  return OK;                                            /* manager.c:129 */
}
```

规则细节：

- **`SYS_PROC` 位**：`r_priv.s_flags & SYS_PROC` 是"目标是不是系统服务"的判定（priv 结构语义见 03 §2.1）。非系统服务（用户进程）只允许 `RS_EDIT`（`request.c:329` 的 `do_edit` 就是改服务配置用）。
- **`RUPDATE_IS_UPDATING()`**（const.h:105）= `rupdate.flags & RS_UPDATING`。注意它与规则 3 的 `RS_INITIALIZING` 位不同：前者是 RS 全局的 update 状态（16 的状态机），后者是单个服务的初始化标志。二者都返回 EBUSY，但语义不同（全局 vs 单服务）。
- **`RS_TERMINATED`**：服务已终止（15 的 terminate 流程设置），此时只允许 DOWN（清理）或 RESTART（复活）。注意规则 5 的 `SF_CORE_SRV` 例外——核心服务即使 terminated 也不能 DOWN（`RS_RESTART` 可以，如 `do_restart` 对 RS 自身）。
- **错误码映射**：`EPERM`（权限不足）与 `EBUSY`（忙）都是 Minix3 errno（`include/errno.h`），Rust 侧 `Result<(), i32>` 直接返回负 errno，不发明新错误码。

### 2.4 11 个调用点（request.c）

`check_call_permission` 是**入口检查**：每个请求 handler 的第一件事。grep 实证的 11 个调用点：

| 行 | handler | call 参数 | rp | 说明 |
|----|---------|-----------|-----|------|
| request.c:27 | `do_up` | `RS_UP` | NULL | 启动服务，无目标 → 仅 root |
| request.c:133 | `do_down` | `RS_DOWN` | 目标槽 | 停止服务 |
| request.c:183 | `do_restart` | `RS_RESTART` | 目标槽 | 重启服务 |
| request.c:232 | `do_clone` | `RS_CLONE` | 目标槽 | 克隆服务（18） |
| request.c:277 | `do_unclone` | `RS_UNCLONE` | 目标槽 | 撤销克隆 |
| request.c:329 | `do_edit` | `RS_EDIT` | 目标槽 | 编辑服务配置（08） |
| request.c:412 | `do_refresh` | `RS_REFRESH` | 目标槽 | 刷新服务（15） |
| request.c:439 | `do_shutdown` | `RS_SHUTDOWN` | NULL | 系统关停 → 仅 root |
| request.c:643 | `do_update` | `RS_UPDATE` | 目标槽 | Live Update（16） |
| request.c:1104 | `do_getsysinfo` | **0** | NULL | 查询系统信息 → 仅 root |
| request.c:1253 | `do_fi` | `RS_FI` | 目标槽 | 故障注入（14） |

两个特殊点：

1. **`do_getsysinfo` 传 `call=0`**（request.c:1104）：0 不是任何 RS_* 消息类型（`RS_RQ_BASE=0x700`，com.h:463）。因为 `rp=NULL` 时 `call` 参数只参与"非系统进程仅 RS_EDIT"规则（该规则在 `if(rp)` 块内，NULL 时跳过），传什么值都等价——传 0 是 C 源码里"无关紧要"的写法。Rust 签名保留 `call: i32`，语义是"root-only 调用不校验 call 值"。
2. **`do_shutdown` 的 NULL 例外**（request.c:439）：`RS_SHUTDOWN` 通知 RS 系统即将关停，属于系统级操作，不需要目标槽，仅 root。

此外 `do_init_ready`（request.c:462）、`do_upd_ready`（request.c:890）**不调用** `check_call_permission`——它们是服务发给 RS 的**回复**（RS_INIT/RS_LU_PREPARE 消息，12 展开），RS 校验的是调用者是否就是目标服务自身，走另一条路径（`rs_isokendpt` + 槽状态检查，02/12）。

### 2.5 `RUPDATE_IS_UPDATING`（const.h:105，归属 16）

```c
#define RUPDATE_IS_UPDATING() (rupdate.flags & RS_UPDATING)   /* const.h:105 */
```

这是 RS 全局 update 状态（`rupdate`，type.h）的访问宏，16-rs-live-update.md 拥有其状态机语义。本文档只陈述消费规则：`check_call_permission` 在目标槽规则 2 中读取它，update 进行中任何带目标的调用都返回 EBUSY。宏在 Rust 侧对应 `updating: bool` 参数——由主循环/请求层从 update 状态计算后传入（16 接线）。

---

## 3. Rust 设计决策

### 3.1 模块与签名（D1）

`os/servers/rs/src/access.rs` 提供三个纯函数，与 C 的三个函数一一对应：

```rust
pub fn caller_is_root(euid: Result<u32, Errno>) -> bool;
pub fn caller_can_control(caller: Endpoint, target: &ServiceSlot, table: &RProcTable) -> bool;
pub fn check_call_permission(
    caller: Endpoint, call: i32, rp: Option<&ServiceSlot>,
    table: &RProcTable, updating: bool, caller_euid: Result<u32, Errno>,
) -> Result<(), Errno>;
```

设计差异：

- **`rp: Option<&ServiceSlot>`** 编码 C 的 `rp == NULL` 双语义（无目标调用 + 目标槽检查跳过），杜绝裸指针。
- **`Result<(), Errno>`** 直接映射 C 的 `int` 返回（负 errno），`OK` → `Ok(())`，EPERM/EBUSY → `Err(EPERM/EBUSY)`（A-12 fail-closed 家族）。
- **`updating: bool` 显式参数**：C 依赖全局 `rupdate.flags`，Rust 把全局状态作为参数注入，函数保持纯函数可测试性。调用方（主循环分发层，06）从 update 状态计算该布尔。
- **`caller_euid` 注入（T5，2026-08-16）**：`getnuid` 查询由 **shell**（19 接线层 / 主循环分发）执行，
  把 `Result<u32, Errno>` 传入决策函数——`access.rs` 不再 import `KernelApi`，syscall 面只出现在接线层
  （monitor 模式，todo §13）。shell 每处权限检查做一次 `sys.getnuid(caller)`，与 C 的
  `caller_is_root(caller)` 恒先查询一致（manager.c:91）。

### 3.2 `caller_is_root` → `KernelApi::getnuid`（D2，A-12）

```rust
pub fn caller_is_root(euid: Result<u32, Errno>) -> bool {
    euid.map(|euid| euid == 0).unwrap_or(false)
}
```

`KernelApi::getnuid`（boot.rs:78）是 19 要接线的 PM_GETEPINFO 封装
（`sys.getnuid(endpoint) → Result<u32, Errno>`）。**T5 后查询与决策分离**：shell 执行
`sys.getnuid(caller)`，把 `Result` 传给纯函数。**fail-closed 语义显式化**：C 里"负 errno 强转 uid_t
永不 0"是隐式的，Rust 用 `unwrap_or(false)` 让"查询失败 → 拒绝"一目了然。测试覆盖三态：
root（`Ok(0)`）/ 非 root（`Ok(1000)`）/ getnuid 失败（`Err(...)`，access.rs tests），
`check_call_permission` 层同样覆盖 fail-closed 路径。

### 3.3 `caller_can_control` → endpoint 索引 + control 列表（D3，A-4）

```rust
let Some(caller_slot) = table.endpoint_slot(caller) else { return false; };
let caller = &table.get(caller_slot);
caller.control[..caller.nr_control.max(0) as usize]
    .iter()
    .any(|c| c == proc_name)
```

- C 的"全表扫描找调用者槽"（manager.c:52-60）→ `RProcTable::endpoint_slot()` 的 O(1) 索引（ARCH A-4，`rproc_ptr[]` 等价，02 §3.4）。
- C 的 `strcmp` → Rust `&str`/`Label` 的 `==`（`proc_name`/`control` 均为 `Label` 类型，02）。
- C 的 `r_nr_control`（int）→ Rust 切片长度由 `nr_control` 限定；`max(0)` 防御负值（C 的 int 可能为负的健壮性对齐）。

### 3.4 `check_call_permission` → 规则顺序保持（D4）

五条规则与 C 完全同序（manager.c:91-126）：

1. root || control，否则 EPERM（manager.c:91-97）
2. 目标非系统进程且非 RS_EDIT → EPERM（manager.c:103-105）
3. `updating` → EBUSY（manager.c:108-110）
4. LATEREPLY || INITIALIZING → EBUSY（manager.c:113-116）
5. TERMINATED 且非 DOWN/RESTART → EPERM（manager.c:119-121）
6. CORE_SRV 且 DOWN → EPERM（manager.c:124-126）

位判定用 `RFlags`/`SysFlags` bitflags（02 定义，`service_slot.rs`），`r_priv.s_flags & SYS_PROC` 对应 `PrivFlags::SYS_PROC`（03 定义，`privilege.rs`）。

### 3.5 RS_* 消息常量 → `minix-types` 的 `ipc/rs.rs`（D5，A-2 第一步）

`check_call_permission` 的 `call` 参数需要 RS_* 常量。ARCH A-2（typed message 全量）是 99/19 的大工程，本文档先落地**第一步**：`os/libs/minix-types/src/ipc/rs.rs` 提供 15 个 `RS_*` 常量（com.h:465-482）+ `RS_SYSCTL_*` 子功能（com.h:485-489）+ `RS_FI_CRASH`（com.h:492），全部带 `#[cfg(test)]` 值断言测试（防止将来重排常量时静默漂移）。typed payload 结构（`mess_rs_*`，ipc.h:1855-1906）留待 99/19。

---

## 4. 实现详解（access.rs）

模块结构：

```
access.rs
├─ caller_is_root(euid: Result<u32, Errno>) -> bool  // manager.c:21-34（T5：shell 注入 getnuid 结果）
├─ caller_can_control(caller, target, table) -> bool  // manager.c:39-76
├─ check_call_permission(caller, call, rp, table, updating, caller_euid) -> Result<(), Errno>
│   ├─ 规则 1: root || control（manager.c:91-97）
│   ├─ 规则 2: SYS_PROC / RS_EDIT（manager.c:103-105）
│   ├─ 规则 3: updating → EBUSY（manager.c:108-110）
│   ├─ 规则 4: LATEREPLY|INITIALIZING → EBUSY（manager.c:113-116）
│   ├─ 规则 5: TERMINATED 限 DOWN/RESTART（manager.c:119-121）
│   └─ 规则 6: CORE_SRV 禁 DOWN（manager.c:124-126）
└─ #[cfg(test)] 单元测试（17+ 测试族，见 §5）
```

关键不变量：

1. **入口唯一性**：`check_call_permission` 是 11 个 handler 的唯一入口检查函数——不存在绕过它的第二路径（grep request.c 仅 11 处调用，§2.4）。
2. **纯函数（T5）**：无任何 IO——`getnuid` 结果由 shell 注入；表与槽均为借用参数，无全局状态。
3. **顺序不可重排**：规则顺序是 C 语义（先权限后并发，先全局后单服务），测试断言具体错误码（EPERM vs EBUSY）保证顺序。
4. **fail-closed 总则**：任何查询失败（getnuid `Err`、调用者不在表内）→ 拒绝，绝不静默放行。

---

## 5. 测试要点

`cargo test -p minix-rs --lib access` 中 access 相关测试（access.rs `#[cfg(test)]`，6 项，6/6 已落地；全局测试数是并行模块增长快照，非承诺）：

| 测试 | 覆盖 |
|------|------|
| `test_caller_is_root` | `Ok(0)` → true；`Ok(1000)` → false；`Err(...)`（getnuid 失败）→ false（fail-closed） |
| `test_caller_can_control_policy` | 无策略 → denied；控制列表含目标 proc_name → allowed；未知调用者（不在表内）→ denied |
| `test_caller_can_control_corrupt_count_fails_closed`（D3） | `nr_control` 超 `RS_NR_CONTROL` → denied（不 panic，fail-closed） |
| `test_caller_can_control_skips_non_in_use_caller_row`（R30） | 调用者行已释放但索引条目陈旧 → denied（manager.c:52-53 的 in-use 复核） |
| `test_check_call_permission_root` | root + 无目标（RS_UP）→ OK；非 root + 无目标 → EPERM；getnuid 失败 → EPERM（fail-closed） |
| `test_check_call_permission_target_rules` | 五条目标槽规则逐条：用户进程仅 RS_EDIT / updating EBUSY / LATEREPLY EBUSY / TERMINATED 限 DOWN·RESTART / CORE_SRV 禁 DOWN（非 DOWN 调用放行） |

**T5 后测试不再需要 mock**：决策函数接收查询结果（`Ok(0)`/`Ok(1000)`/`Err(...)`），`table()` 构造最小
`RProcTable`（含 VFS/PM/TTY 槽），纯数据驱动，不依赖 `KernelApi` 实现。

---

## 6. 过渡：从"谁能命令"到"消息怎么送达"

本文档是请求入口的第一道门：主循环（06）收到 `RS_*` 消息后，handler 第一行调用 `check_call_permission`（§2.4 的 11 个调用点）。通过后：

- 若请求是**服务启动**（`RS_UP`），进入 08-slot-config 的参数校验（`check_request`）与 10-service-create 的创建流程；
- 若请求是**控制/查询**（DOWN/RESTART/…/FI），进入 13/14 的具体语义；
- 若请求是 **Live Update**（`RS_UPDATE`），进入 16 的状态机——其中"update 进行中"正是本文档规则 3 的 `updating` 来源。

下一篇 `05-rs-ipc-sendmask.md` 回到 boot Step 1 的机制面：RS 如何为服务计算**允许发往的 IPC 目标集合**（`s_ipc_to`），它与本文档的"谁控制谁"是正交的——05 管"服务能向谁发消息"，04 管"谁有资格命令 RS"。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md` — 主循环分类与 boot 时序
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` — `RFlags`/`SysFlags`/`r_control` 字段、endpoint 索引（ARCH A-4）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/03-rs-privilege.md` — `SYS_PROC` 位、privctl 操作面
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/05-rs-ipc-sendmask.md` — boot Step 1 的 IPC 掩码机制
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md` — `r_control` 填充（check_request/init_slot）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/13-rs-control-requests.md`、`14-rs-query-requests.md`、`16-rs-live-update.md` — 被本入口保护的 handler
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/99-rs-global-concepts.md` — RS_* 消息常量全表
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/19-rs-external-interfaces.md` — `getnuid`（PM_GETEPINFO）消息契约
- `minix3/minix/servers/rs/manager.c:21-130`、`request.c`、`lib/libsys/getepinfo.c:35-44`、`servers/rs/const.h:105`、`include/minix/com.h:463-492` — ground truth
- `os/servers/rs/src/access.rs`、`os/libs/minix-types/src/ipc/rs.rs` — Rust 实现
