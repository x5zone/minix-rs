# 99-rs-global-concepts: 全局概念词典

> **分类**: 全局概念（常量词典）
> **源码**: `minix3/minix/servers/rs/const.h`（25,28-43,48-51,58,74-80,83,105-120）、`minix3/minix/servers/rs/error.c`、`minix3/minix/include/minix/com.h:342-353,442-446,463-492,627,736,741-745`、`minix3/minix/include/minix/rs.h`（RSS_*/SF_*）、`minix3/minix/include/minix/sef.h`（SEF_INIT_*/SEF_LU_*/SEF_CB_*）、`minix3/minix/include/minix/ipc_filter.h`（IPCF_*/ANY_*）
> **Rust 模块**: `minix-types`（`ipc/rs.rs`、`types/com.rs`、`types/endpoint.rs`、`types/errno.rs`）、`os/servers/rs/src/`（各消费模块）
> **前置**: 无（词典，各篇引用）
> **说明**: 本文档是 03-stage-rs 的**常量词典**：15 个 RS_* 消息类型、16 个 `r_flags`、13 个 `SF_*`、`RSS_*`/`SEF_*` 标志、系统操作码、endpoint 与错误表，全部给出 C 锚点与 **Rust 权威位置**（避免跨模块重复定义）。机制语义一律移交 01~19 对应篇。

---

## 1. 概念：一本常量词典

### 1.0 章节引言

RS 的常量分散在 4 个头文件：`const.h`（服务状态标志、时间常量）、`com.h`（消息类型、系统操作码）、`rs.h`（`RSS_*`/`SF_*`）、`sef.h`（`SEF_*`）。各机制文档引用这些常量时如果各自定义，会出现跨模块漂移（const 权威位置问题）——本文档给每个常量族一个**权威 Rust 位置**。

> **本章不讲什么**（机制一律移交）:
> - 消息如何被消费（13/14/16 对应篇）
> - 标志位如何驱动状态机（07/15/16/17 对应篇）
> - 错误如何产生（各机制文档）

### 1.1 为什么需要词典（WHY）

- **覆盖契约**：plan §5.3 要求 15 个 IPC 消息类型、16 个 `r_flags`、13 个 `SF_*`、`RSS_*`/`SEF_*` 标志面全部进入覆盖清单；
- **权威位置**：同一常量只在**一处** Rust 定义（如 `SF_VM_ROLLBACK` 只在 `service_slot.rs::SysFlags`），其余消费点引用；
- **快照可追溯**：C 锚点精确到行，避免行号漂移传播。

### 1.2 词典结构（WHAT）

六族：消息类型（§2.1）、时间与状态机常量（§2.2）、服务标志（§2.3）、系统操作码与 endpoint（§2.4）、错误表（§2.5）、Rust 映射（§3）。

---

## 2. 常量族

### 2.1 RS_* 消息类型（com.h:463-492）

`RS_RQ_BASE = 0x700`（com.h:463）；`RS_UP`~`RS_FI` 共 15 个主类型，**gap 10-19**（`RS_LOOKUP=8`、`RS_GETSYSINFO=9` 后跳到 `RS_INIT=20`）：

| 常量 | 值 | C 锚点 | Rust | 消费篇 |
|------|-----|--------|------|--------|
| `RS_UP` | 0x700 | com.h:465 | `minix_types::ipc::rs::RS_UP` | 13 |
| `RS_DOWN` | 0x701 | com.h:466 | 同上 | 13 |
| `RS_REFRESH` | 0x702 | com.h:467 | 同上 | 13 |
| `RS_RESTART` | 0x703 | com.h:468 | 同上 | 13 |
| `RS_SHUTDOWN` | 0x704 | com.h:469 | 同上 | 13 |
| `RS_UPDATE` | 0x705 | com.h:470 | 同上 | 16 |
| `RS_CLONE`/`RS_UNCLONE` | 0x706/0x707 | com.h:471-472 | 同上 | 13 |
| `RS_LOOKUP` | 0x708 | com.h:474 | 同上 | 14 |
| `RS_GETSYSINFO` | 0x709 | com.h:476 | 同上 | 14 |
| `RS_INIT` | 0x714 | com.h:478 | 同上 | 12 |
| `RS_LU_PREPARE` | 0x715 | com.h:479 | 同上 | 12/16 |
| `RS_EDIT` | 0x716 | com.h:480 | 同上 | 13 |
| `RS_SYSCTL` | 0x717 | com.h:481 | 同上 | 14 |
| `RS_FI` | 0x718 | com.h:482 | 同上 | 14 |

子功能：`RS_SYSCTL_SRV_STATUS=1`..`UPD_STATUS=5`（com.h:485-489，`ipc::rs::sysctl`）、`RS_FI_CRASH=1`（com.h:492）。

### 2.2 时间与状态机常量（const.h）

**时间常量**（hz 依赖公式，不硬编码——见 §3.2）：

| 常量 | 公式/值 | C 锚点 | Rust 处理 |
|------|---------|--------|-----------|
| `RS_INIT_T` | `system_hz * 10` | const.h:48 | 公式，参数注入（07/12） |
| `RS_DELTA_T` | `system_hz` | const.h:49 | 公式，参数注入（07 `delta_t`） |
| `RS_DEFAULT_PREPARE_MAXTIME` | `2 * RS_DELTA_T` | const.h:58 | `live_update.rs::default_prepare_maxtime` 默认值参数 |
| `RS_VM_DEFAULT_MAP_PREALLOC_LEN` | 8MB | const.h:83 | `live_update.rs`（纯常量） |
| `MAX_DET_RESTART` | 10 | const.h:25 | `recovery.rs` |
| `MAX_BACKOFF` | 30 | const.h:51 | `recovery.rs` |
| `BACKOFF_BITS` | `sizeof(long)*8` = 64 | const.h:50 | `recovery.rs` |
| `RS_REPLY`/`RS_CANCEL` | 1/2 | const.h:75-76 | `live_update.rs` |
| `RS_DONTSWAP`/`RS_SWAP` | 0/1 | const.h:79-80 | `self_lifecycle.rs::SwapFlag` |

**`r_flags` 16 位**（const.h:28-43，`service_slot.rs::RFlags`）：

| 位 | 值 | 含义 |
|----|-----|------|
| `RS_IN_USE` | 0x001 | slot 占用 |
| `RS_EXITING` | 0x002 | 期待退出 |
| `RS_REFRESHING` | 0x004 | 刷新中 |
| `RS_NOPINGREPLY` | 0x008 | 心跳未回 |
| `RS_TERMINATED` | 0x010 | 已终止 |
| `RS_LATEREPLY` | 0x020 | RS_DOWN 未回 |
| `RS_INITIALIZING` | 0x040 | init 中 |
| `RS_UPDATING` | 0x080 | update 中 |
| `RS_PREPARE_DONE` | 0x100 | prepare 完成 |
| `RS_INIT_DONE` | 0x200 | init 完成 |
| `RS_INIT_PENDING` | 0x400 | init 挂起 |
| `RS_ACTIVE` | 0x800 | 活动实例 |
| `RS_DEAD` | 0x1000 | 待清理 |
| `RS_CLEANUP_DETACH` | 0x2000 | 清理时 detach |
| `RS_CLEANUP_SCRIPT` | 0x4000 | 清理时跑脚本 |
| `RS_REINCARNATE` | 0x8000 | 退出后新 endpoint 重启 |

### 2.3 服务标志（rs.h + sef.h）

**`RSS_*`**（rs.h:33-52，`slot.rs::RssFlags`，20 个）：`RSS_COPY=0x01`/`RSS_REUSE=0x04`/`RSS_NOBLOCK=0x08`/`RSS_REPLICA=0x10`/`RSS_BATCH=0x20`/`RSS_SELF_LU=0x40`/`RSS_ASR_LU=0x80`/`RSS_FORCE_SELF_LU=0x100`/`RSS_PREPARE_ONLY_LU=0x200`/`RSS_FORCE_INIT_CRASH=0x400`/`RSS_FORCE_INIT_FAIL=0x800`/`RSS_FORCE_INIT_TIMEOUT=0x1000`/`RSS_FORCE_INIT_DEFCB=0x2000`/`RSS_SYS_BASIC_CALLS=0x4000`/`RSS_VM_BASIC_CALLS=0x8000`/`RSS_NOMMAP_LU=0x10000`/`RSS_DETACH=0x20000`/`RSS_NORESTART=0x40000`/`RSS_FORCE_INIT_ST=0x80000`/`RSS_NO_BIN_EXP=0x100000`。

**`SF_*` 13 位**（rs.h:191-203，`service_slot.rs::SysFlags`）：`SF_CORE_SRV=0x001`/`SF_SYNCH_BOOT=0x002`/`SF_NEED_COPY=0x004`/`SF_USE_COPY=0x008`/`SF_NEED_REPL=0x010`/`SF_USE_REPL=0x020`/`SF_VM_UPDATE=0x040`/`SF_VM_ROLLBACK=0x080`/`SF_VM_NOMMAP=0x100`/`SF_USE_SCRIPT=0x200`/`SF_DET_RESTART=0x400`/`SF_NORESTART=0x800`/`SF_NO_BIN_EXP=0x1000`。

**`SEF_*`**（sef.h）：`SEF_INIT_FRESH=0`/`SEF_INIT_LU=1`/`SEF_INIT_RESTART=2`（sef.h:93-95，`sef.rs::SefInitType`）；`SEF_LU_*` 八位（sef.h:235-242，`live_update.rs::LuFlags`）；`SEF_LU_STATE_NULL=0`/`WORK_FREE=1`/`REQUEST_FREE=2`/`PROTOCOL_FREE=3`/`EVAL=4`/`UNREACHABLE=5`/`PREPARE_CRASH=6`（sef.h:213-220）；`SEF_CB_INIT_RESTART_STATEFUL`/`SEF_CB_INIT_LU_DEFAULT`（sef.h:85,88，libsys 通用回调，19 契约）。

**`IPCF_*`/`ANY_*`**（ipc_filter.h，`state_data.rs::IpcfFlags`/`ANY_*`）：`MATCH_M_SOURCE=0x1`/`MATCH_M_TYPE=0x2`/`EL_BLACKLIST=0x4`/`EL_WHITELIST=0x8`；`ANY_USR=0xFC00`/`ANY_SYS=0x17C00`/`ANY_TSK=0x1FC00`。

### 2.4 系统操作码与 endpoint

**`SYS_PRIV_*`**（com.h:342-353，`privilege.rs::PrivCtlOp`，11 操作码）：`SYS_PRIV_ALLOW=1`/`SYS_PRIV_DISALLOW=2`/`SYS_PRIV_SET_SYS=3`/`SYS_PRIV_SET_USER=4`/`SYS_PRIV_ADD_IO=5`/`SYS_PRIV_ADD_MEM=6`/`SYS_PRIV_ADD_IRQ=7`/`SYS_PRIV_QUERY_MEM=8`/`SYS_PRIV_UPDATE_SYS=9`/`SYS_PRIV_YIELD=10`/`SYS_PRIV_CLEAR_IPC_REFS=11`。**`SYS_STATE_*`**（com.h:442-446，`minix-types::types::com`，99 补齐）：`CLEAR_IPC_REFS=1`/`SET_STATE_TABLE=2`/`ADD_IPC_BL_FILTER=3`/`ADD_IPC_WL_FILTER=4`/`CLEAR_IPC_FILTERS=5`。

**endpoint 常量**（`minix-types::types::endpoint`）：`RS_PROC_NR=2`（com.h:61）、`VM_PROC_NR=8`、`PM=0`/`VFS=1`/`SCHED=4`/`DS=6`；`ANY`/`NONE`/`SELF`（endpoint.h:54-56）；`MAX_NR_TASKS=1023`（com.h:55）。

**`VM_RS_MEM_*`**（com.h:741-745，`minix-types::ipc::rs`）：`PIN=0`/`MAKE_VM=1`/`HEAP_PREALLOC=2`/`MAP_PREALLOC=3`/`GET_PREALLOC_MAP=4`；`VM_RS_UPDATE = VM_RQ_BASE+41 = 0xC29`（com.h:736,627）。

### 2.5 错误表（error.c）

| 表 | 错误码 | 含义 | Rust |
|----|--------|------|------|
| init | `ENOSYS` | 服务不支持该初始化类型 | `minix-types::errno` |
| init | `ERESTART` | 服务请求初始化重置（=200，sys/errno.h:196） | 同上 |
| lu | `ENOSYS` | 服务不支持 live update | 同上 |
| lu | `EINVAL` | 服务不支持所需状态 | 同上 |
| lu | `EBUSY` | 服务当前无法 prepare | 同上 |
| lu | `EGENERIC` | prepare 时发生通用错误（=204，sys/errno.h:200） | 同上 |

错误表本体在 error.c:15-18（init）/22-27（lu）；`init_strerror`/`lu_strerror`（error.c:48/56）是错误码→字符串映射，wire-up 时按此表实现。

---

## 3. Rust 映射决策

### 3.1 权威位置映射表

| 族 | 权威 Rust 位置 | 重复定义策略 |
|----|---------------|-------------|
| RS_* 消息类型/子功能 | `minix-types::ipc::rs` | 唯一 |
| SI_* 查询分类 | `os/servers/rs/src/query.rs` | 唯一 |
| SEF_LU_*/SEF_LU_STATE_*/RS_REPLY/RS_CANCEL | `os/servers/rs/src/live_update.rs` | 唯一 |
| MAX_DET_RESTART/BACKOFF_BITS/MAX_BACKOFF | `os/servers/rs/src/recovery.rs` | 唯一 |
| r_flags/SF_* | `os/servers/rs/src/service_slot.rs` | 唯一 |
| RSS_* | `os/servers/rs/src/slot.rs` | 唯一 |
| SYS_PRIV_* | `os/servers/rs/src/privilege.rs` | 唯一 |
| SYS_STATE_* | `minix-types::types::com` | 唯一（99 补齐） |
| IPCF_*/ANY_* | `os/servers/rs/src/state_data.rs` | 唯一 |
| SEF_INIT_*/SEF_CB_* | `os/servers/rs/src/sef.rs` + 19 契约 | 唯一 |
| VM_RS_MEM_*/VM_RS_UPDATE | `minix-types::ipc::rs` | 唯一（19 补齐） |
| endpoint/errno | `minix-types::types::endpoint`/`errno` | 唯一 |

### 3.2 hz 依赖公式不硬编码

`RS_INIT_T`/`RS_DELTA_T`/`RS_DEFAULT_PREPARE_MAXTIME` 依赖 `system_hz`（运行时读取，main.c:181）——Rust 侧以**参数注入**默认值（`live_update.rs::default_prepare_maxtime(maxtime, default)` 已按此设计），不在 no_std 侧硬编码 hz。

### 3.3 快照标识符可验证性

快照 `minix3/` 中的标识符保持规范 Minix3 名字，可 grep 实证：`error.c:26` 写 `EGENERIC`、`manager.c:766` 写 `ROOT_SYS_PROC`、`lib/libsys/sef_st.c:151` 写 `sys_getpriv`——不存在重命名。Rust 侧 `errno.rs` 的 `EGENERIC` 常量（errno.rs:98，=204）按 99 表落地；本文档与 19 均使用**规范 Minix3 名字** + 可验证行锚点。

---

## 4. 实现（minix-types 缺口补齐）

`os/libs/minix-types/src/types/com.rs` 新增 `SYS_STATE_*` 段（com.h:442-446，五操作码）。其余常量已在 §3.1 权威位置落地，本模块不重复定义。

---

## 5. 测试要点

- `minix-types::types::com` 新增测试（`cargo test -p minix-types --lib sys_state`，1 项，1/1 已落地）：`SYS_STATE_*` 五操作码值（1..5）。
- 既有消息类型测试（`ipc::rs` 的 `RS_*` 值）与 `VM_RS_MEM_*` 测试（19）共同构成词典的 Rust 验证面。

测试总数声明：本文档范围为 `types::com` 新增 `SYS_STATE_*` 测试数（1 项稳定值）。

---

## 6. 过渡

词典是各篇的**常量查找入口**：

- 00-rs-overview（导航）引用本文档为常量权威位置；
- 各机制文档（01~19）引用常量时指向 §3.1 映射表，不重复定义；
- 错误表（§2.5）与 19 的消息契约共同支撑 wire-up 阶段的错误处理。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/00-rs-overview.md` —— 导航枢纽
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/19-rs-external-interfaces.md` —— 消息槽类型化（ARCH A-2）、VM_RS_MEM_* 落地
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— SEF_LU_*/SEF_LU_STATE_* 消费
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/17-rs-state-data.md` —— IPCF_*/ANY_* 消费
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` —— r_flags/RupdateFlags 消费
- `minix3/minix/servers/rs/const.h`、`error.c`、`include/minix/com.h:342-353,442-446,463-492`、`rs.h`、`sef.h`、`ipc_filter.h` —— ground truth
