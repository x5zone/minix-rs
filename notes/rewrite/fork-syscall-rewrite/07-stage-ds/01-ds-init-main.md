# 01 — DS 启动入口与主循环：一个事件循环如何撑起整个注册中心

> **分类**: 服务器入口 / 主循环骨架
> **源码**: `minix3/minix/servers/ds/main.c`（132 行）
> **说明**: DS 是 boot 映像里第一个用户态服务，它的主循环只有三件事：收消息、分发、回信。本文讲清这三件事和启动时的三次 SEF 注册。

---

## 1 概念

### 1.1 目标读者与前置知识

面向第一次读 Minix3 用户态服务的读者。前置知识：C 语言，操作系统里"服务器循环收消息"的基本想法。IPC 原语（`sef_receive` / `ipc_send`）的细节不需要预习，本文只用它们的名字，语义在用到时一句话解释。

### 1.2 本章不讲什么

- 各请求的具体处理（publish/retrieve/…）——那是 07~11 的事，本篇只讲消息进来之后去哪。
- 消息里每个字段的含义——那是 02 的事。
- 表和槽位长什么样——那是 03/04 的事。

### 1.3 为什么主循环长成这样

DS 的本质是一个**单线程的查表服务器**：它不管理硬件，不调度进程，不碰页表。它的一生就是：等一封信，看信封上的调用号，查表或改表，回一封信。因为逻辑是线性的，主循环不需要线程、不需要锁——同一时刻只处理一封信，全局状态（当前处理谁的信）用两个全局变量 `who_e` / `callnr` 记住就行。

整个循环可以画成这样：

```
                ┌─────────────────────────────────┐
                │  sef_local_startup()：三次注册   │  启动时一次
                └───────────────┬─────────────────┘
                                ▼
┌─────────┐   ┌──────────────────────────┐   ┌───────────┐
│ 收信     │──▶│ 看调用号                  │──▶│ 回信       │
│ get_work │   │ notify→拒绝 / 已知→分发   │   │ reply     │
│ main.c   │   │ /未知→拒绝 / EDONTREPLY→  │   │ main.c    │
│ :109    │   │ 不回                      │   │ :123      │
└─────────┘   └──────────────────────────┘   └───────────┘
```

启动时的三次注册（`sef_local_startup`，`main.c:93-107`）分别是：正常启动回调 `sef_cb_init_fresh`、带状态重启回调（`SEF_CB_INIT_RESTART_STATEFUL`，重启不清表）、Live Update 状态转移钩子（`sef_llvm_ds_st_init`，把弱符号 `_magic_ds_st_init` 挂上）。三者的共同点是：**DS 的表是跨重启的资产**——重启一个注册中心还丢光所有注册项，那就没有意义了。

分发规则（`main`，`main.c:45-88`）按到达类型分三类：

| 到达 | 条件 | 动作 | C 位置 |
|------|------|------|--------|
| notify 通知 | `is_notify(callnr)` 为真（调用号落在 `0x1000~0x10FF` 区间，`com.h:90-93`） | 打印告警，回 `EINVAL` | `main.c:47-51` |
| 七种已知调用 | `DS_PUBLISH` / `RETRIEVE` / `RETRIEVE_LABEL` / `DELETE` / `SUBSCRIBE` / `CHECK` / `GETSYSINFO` | 调对应 handler | `main.c:54-72` |
| 其他一切 | `default`（包括已声明但无分支的 `DS_SNAPSHOT`） | 打印告警，回 `EINVAL` | `main.c:75-77` |

回信规则只有一条：除非 handler 返回 `EDONTREPLY`（"这次先不回，稍后另行通知"），否则 `m_type` 填结果码发回去（`main.c:82`）。DS 的 handler 目前从不返回 `EDONTREPLY`，但循环保留这个分支——这是所有 Minix3 服务的统一约定。

### 1.4 小结

启动注册三次（ fresh / stateful-restart / live-update ），循环三步（收、分、回），拒绝两类（notify 和未知号都回 `EINVAL`）。记住这张表，07~11 的每个 handler 都是往"分发"这一格里填内容。

---

## 2 C 源码分析

### 2.1 函数清单（`main.c` 共 4 个函数，无数据结构定义）

| 函数 | 位置 | 输入 | 输出/副作用 | 说明 |
|------|------|------|-------------|------|
| `main` | `main.c:28-91` | `argc/argv` | 永不返回；循环收发 | 先调 `env_setargs`，再 `sef_local_startup`，然后死循环 |
| `sef_local_startup` | `main.c:93-107` | 无 | 注册三个回调后 `sef_startup()` | 纯注册，不做业务初始化（真正的表初始化在 `sef_cb_init_fresh`，见 06） |
| `get_work` | `main.c:109-121` | 消息指针 | 写全局 `who_e`（谁发的）、`callnr`（什么号）；`sef_receive` 失败则 `panic` | 阻塞收信，是循环里唯一的等待点 |
| `reply` | `main.c:123-132` | `who_e`、消息指针 | `ipc_send` 发回；失败只打印，不重试 | 发信失败不panic——收信失败才panic，轻重分明 |

### 2.2 分发表逐项核对（`main.c:53-78`）

| 调用号 | 值（`com.h:498-507`） | handler | 位置 |
|--------|---------------------|---------|------|
| `DS_PUBLISH` | `0x800` | `do_publish` | `:54` |
| `DS_RETRIEVE` | `0x801` | `do_retrieve` | `:57` |
| `DS_RETRIEVE_LABEL` | `0x806` | `do_retrieve_label` | `:60` |
| `DS_DELETE` | `0x804` | `do_delete` | `:63` |
| `DS_SUBSCRIBE` | `0x802` | `do_subscribe` | `:66` |
| `DS_CHECK` | `0x803` | `do_check` | `:69` |
| `DS_GETSYSINFO` | `0x807` | `do_getsysinfo` | `:72` |
| `DS_SNAPSHOT`（`0x805`） | 已声明，无分支 | 落入 `default`，回 `EINVAL` | `:75-77` |

注意 `0x805` 的空缺：`DS_SNAPSHOT` 在 `com.h:505` 有名，在 `proto.h:16` 有声明（`do_snapshot`），但 `store.c` 里没有实现，主循环里没有分支。这是一个**死调用号**（A-7，见 02 和 12），发过来只会收到 `EINVAL`。

### 2.3 notify 判定：一个带 FIXME 的老宏

`is_notify`（`com.h:90-93`）的写法是 `(callnr - 0x1000) < 0x100`，按**无符号**比较。头文件里它标着 FIXME，但 DS 只能用它——DS 用 `sef_receive` 收信（拿不到内核状态），这是唯一能区分"通知"和"请求"的办法。无符号是关键：有符号比较会把 `0x800` 这样的正常调用号也误判成通知。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 调用号变枚举 | 裸 `int` 做 `switch`（`main.c:53`） | `DsCall` 枚举 + `from_raw` 回 `Option`（`dispatch.rs`） | 非法调用号不可表达：`None` 就是 `default` 分支，漏掉一个变体编译器会提醒，而 C 的 `switch` 漏 `case` 不吭声 |
| D2 | 到达分三类 | `if notify / switch / default` 散写 | `Incoming::{NotifyRefusal, Dispatch, Unknown}`（`dispatch.rs`） | notify 拒绝和未知号拒绝在 C 里回同样的 `EINVAL` 但原因是两回事（"没号可派" vs "没路可走"），两个名字让日志和测试能区分 |
| D3 | 死调用号不给名分 | `DS_SNAPSHOT` 有名无分支 | `DsCall` 里**没有** `Snapshot` 变体 | 给它一个变体就等于修了一条 C 从没修过的路；`from_raw(0x805) == None` 让"发快照号被拒"可测 |
| D4 | 回信规则变纯函数 | `if (r != EDONTREPLY) reply` 内联在循环里 | `should_reply(result: i32) -> bool`（`dispatch.rs`） | 规则只有一句话，值得一个名字 + 一个测试；循环体以后长什么样都不影响它 |
| D5 | 启动注册变类型 | 三个 `sef_setcb_*` 调用 + 一个弱符号钩子 | `DsInitKind::{Fresh, RestartStateful}` + `LiveUpdateHook`（`sef.rs`） | "哪种启动"是一个类型该表达的事；弱符号（LLVM magic 插桩）是 C 构建的特技，Rust 用显式序列化代替（A-6），钩子类型只保留"我要传什么"，不碰"我怎么插桩" |

替代方案及否决：把 `main` 循环本身也搬进 Rust（如 `loop { receive; triage; reply }`）——否决，因为收发原语（`sef_receive` / `ipc_send`）依赖 IPC 落地（A-8，`minix-sys` 还是 stub），现在搬只能搬个空壳； verdict 层（判到达、判回信）是纯逻辑，先落地、可先测。

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/ds/src/
├── dispatch.rs   — 本篇：DsCall / Incoming / triage / is_notify / should_reply
├── sef.rs        — 本篇：DsInitKind / LiveUpdateHook
├── main.rs       — 二进制入口（调库 init，占位，IPC 落地后接循环）
└── lib.rs        — 模块导出
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 七种调用 | `main.c:54-72` | `dispatch.rs:23`（`DsCall`） | `from_raw` 映射已知号，未知回 `None` |
| 到达三类 | `main.c:47-78` | `dispatch.rs:62`（`Incoming`）+ `triage` | notify→拒绝，已知→分发，其余→未知 |
| notify 判定 | `com.h:90-93` | `dispatch.rs:94`（`is_notify`） | 无符号区间比较，原样保留 |
| 回信规则 | `main.c:82` | `dispatch.rs:103`（`should_reply`） | 仅 `EDONTREPLY`（203）不回 |
| 启动种类 | `main.c:93-107` | `sef.rs:18`（`DsInitKind`） | Fresh / RestartStateful |
| 转移钩子 | `sef_llvm_ds_st_init` | `sef.rs:34`（`LiveUpdateHook`） | 只定"我要传什么" |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 未知号必拒 | `from_raw` 回 `None` → `Unknown` | `main.c:75-77` |
| notify 永不分发 | `triage` 先判 `is_notify` | `main.c:47-51` |
| 非 EDONTREPLY 必回 | `should_reply` | `main.c:82` |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`（workspace 根在 `os/`），本篇 3 个测试。

| 测试名 | 覆盖 C 位置 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_call_numbers` | `com.h:498-507` | 七号映射正确；死号 `0x805` 回 `None` | `dispatch.rs` |
| `test_triage` | `main.c:47-78` | notify 拒、已知派、未知拒 | `dispatch.rs` |
| `test_should_reply` | `main.c:82` | 仅 `EDONTREPLY` 不回 | `dispatch.rs` |

测试策略：分发用"七对七加一死号"锁定；到达分类用边界值（`0x1000` / `0x10FF` / `0x42`）锁定。

---

## 6 过渡

本篇讲的是 DS 的"骨架"：信怎么进来、去哪、回不回。骨架有了，下一站是 02——信封里装了什么（调用号、消息结构、标志位、grant 约定），也就是 handler 们每天要拆的信。

## 7 参见

- C 源：`minix3/minix/servers/ds/main.c`（全文 132 行）、`minix3/minix/include/minix/com.h:90-93,498-507`
- 阶段文档：`00-ds-overview.md`（总览）、`02-ds-message-contract.md`（下一站，协议面）
- Rust 实现：`os/servers/ds/src/dispatch.rs`、`os/servers/ds/src/sef.rs`
- 对端：`../01-stage-kernel/12-ipc-core.md`（`sef_receive` / `ipc_send` 原语）
