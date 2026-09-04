# 05 — DS 身份与权限：端点和名字如何互查，谁能碰谁的条目

> **分类**: 身份映射 / 权限判定
> **源码**: `minix3/minix/servers/ds/store.c:110-156`
> **说明**: 内核报过来的是端点（数字），表里存的是名字（字符串）。本文讲清两个翻译函数（端点→名、名→端点）和一个权限函数（选择性保护），以及 Rust 为什么把"翻译"和"判定"拆成两个模块。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要写 handler 权限逻辑的读者。前置知识：04 的查找（翻译函数调它们），02 的权限门标志。端点（endpoint）的概念现场解释：一句话——内核里标识一个进程的数字。

### 1.2 本章不讲什么

- 表和槽位长什么样——那是 03/04 的事。
- boot 时 `"rs"` 这个 owner 从哪来——那是 06 的事。
- 各 handler 具体在哪一步调权限检查——那是 07~11 的事（本篇只讲判定本身）。

### 1.3 两个翻译：端点→名，名→端点

DS 的世界里有两种身份：内核给的**端点**（数字，如 RS 是 2）和表里存的**名字**（字符串，如 `"rs"`）。消息从内核来，带的是端点；表里记的是名字。翻译是必需的：

- **端点→名**（`ds_getprocname`，`store.c:110-125`）：三条路——DS 自己（端点 `DS_PROC_NR`）直接回 `"ds"`（`first_proc_name`，`:118`，不查表：自己的标签从没人发布，查也查不到）；其他端点走 label 表反查（`:121`，04 的端点反查）；没发布过标签的端点回 `NULL`（`:124`，无名者）。
- **名→端点**（`ds_getprocep`，`store.c:130-138`）：走 label 表正查（`:135-136`）；查不到直接 `panic`（`:137`）。

`panic` 值得停一下：名→端点查不到，C 让整个 DS 崩溃。这在 10 的订阅者扫描里是个真实风险（订阅者的名字掉线了，扫到它就崩）。Rust 的处理见 D2——翻译只说"没有"，崩不崩由调用链决定。

### 1.4 权限：选择性保护

`check_auth`（`store.c:143-153`）只有两条规则：

1. **门没设，直接过**（`:148-149`）：`flags & perm` 为空 → 放行。保护是**可选的**——发布者不设门，条目就是公开的。这和 Unix 文件权限默认开放不同，和"默认拒绝"的防火墙思维也不同：DS 的门是"选择性保护"，设了才关。
2. **门设了，比名字**（`:151-152`）：条目 owner 等于调用者名 → 放行，否则拒绝。无名调用者（`NULL`）在门设了时必拒——没名字就没法比，默认关。

三道门（`PRIV_RETRIEVE` / `PRIV_OVERWRITE` / `PRIV_SUBSCRIBE`）互相独立：一个条目可以"可读不可覆"、"可订不可读"，任意组合。

### 1.5 翻译和判定为什么分开

C 的 `check_auth` 签名是 `(条目, 端点, 门)`——它内部调 `ds_getprocname` 翻译。Rust 拆成两步：`identity::resolve_name` 翻译（05 前半）+ `auth::check_auth` 判定（05 后半，签名是 `(条目, 名字, 门)`）。理由：判定是纯逻辑（给名字就能判，不碰表），翻译要碰表。拆开后判定可独立测试（给什么名字、什么门，判什么——连表都不用建），翻译的表依赖也只留在一处。调用方多写一行（先翻译再判定），换来两处各自可测。

对照 Redox 的做法：Redox 的 capability 检查也是"解析（找 capability）"和"判定（权位够不够）"两步，原因相同——解析要走表，判定是位运算。 furry 的分层直觉是跨系统的：一句话——**碰表的和不碰表的别写进一个函数**。

### 1.6 小结

翻译两函（端点→名三路，名→端点查不到 C 会 panic）、权限两条（门没设过，设了比名）、模块拆两块（翻译碰表，判定纯算）。下一站 06 讲 DS 启动时 RS 的服务表怎么"影"进 DS（`map_service`，owner=`"rs"` 的来源）。

---

## 2 C 源码分析

### 2.1 端点→名（`ds_getprocname`，`store.c:110-125`）

```c
DS_PROC_NR → "ds"（:118-119，自报，不查表）
其他 → lookup_label_entry(ep)->key（:121-122，经 label 表）
无标签 → NULL（:124）
```

自报优先的原因：DS 自己的标签从没人发布（`map_service` 只给别人登记），查表必空——先答自己的名，少一次必空的表扫。

### 2.2 名→端点（`ds_getprocep`，`store.c:130-138`）

`lookup_entry(name, LABEL)`（`:135`）→ 取 `u.u32`（`:136`，label 寄数字道，03 §2.4）→ 查不到 `panic`（`:137`）。调用点：`do_check` 回填发布者端点（`:570`）、`update_subscribers` 解析订阅者（`:213`）——两处都是"名字应该在"的上下文，C 用 panic 表达这种"不应该"。Rust 认为"应该在"不是"一定在"（D2）。

### 2.3 权限判定（`check_auth`，`store.c:143-153`）

门未置（`:148`）→ 1；置了则 `source && !strcmp(owner, source)`（`:152`，短路：无名直接 0）。调用点：`do_retrieve`（`PRIV_RETRIEVE`）、`do_publish` 覆盖（`PRIV_OVERWRITE`）、`check_sub_match`（`PRIV_SUBSCRIBE`）。注意 `do_delete` **不用它**——删除是无条件比 owner（09 细讲）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 自名先答 | `if (ep == DS_PROC_NR)` 先判 | `resolve_name` 先判 `Endpoint::DS`（`identity.rs`） | 同 C；表的 label 道里永远没有自己，查表必空，先答省一次扫描 |
| D2 | 翻译不 panic | 查不到 `panic`（`:137`） | `resolve_endpoint` 回 `Option`（`identity.rs`） | 崩不崩是调用链的事：10 的扫描走的是"名字必须在"的假设，假设破了该跳过该记日志，不该连带整个注册中心陪葬；`CheckHit::reply_owner` 同理。panic 是 C 的偷懒，不是语义 |
| D3 | 译判分离 | `check_auth(条目, 端点, 门)` 内含翻译 | `resolve_name`（`identity.rs`）+ `check_auth(条目, 名字, 门)`（`auth.rs`） | 判定变纯函数，无表可测；翻译的表依赖收敛一处（§1.5） |
| D4 | 无名者默认关 | `source && ...` 短路 | `caller: Option<&[u8]>`，`None` 在门设时判否 | 类型即短路：`map(key_eq).unwrap_or(false)`，与 C 同 verdict，不同表达 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/ds/src/
├── identity.rs — 本篇前半：resolve_name / resolve_endpoint / DS_SELF_NAME
└── auth.rs     — 本篇后半：check_auth
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 自名 | `store.c:115-119` | `identity.rs:26`（`DS_SELF_NAME`） | `"ds"` 常量道 |
| 端点→名 | `store.c:110-125` | `identity.rs:44`（`resolve_name`） | 自报 → label 反查 → `None`（出借表内道，不拷贝，D1 详注） |
| 名→端点 | `store.c:130-138` | `identity.rs:64`（`resolve_endpoint`） | label 正查 → `Some` / `None`（不 panic） |
| 权限判定 | `store.c:143-153` | `auth.rs:32`（`check_auth`） | 门没设过，设了比名 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 自名不查表 | 先判 `Endpoint::DS` | `store.c:118` |
| 门没设必过 | `!intersects(perm)` 先返真 | `store.c:148` |
| 无名遇设门必拒 | `Option::None → false` | `store.c:152` 短路 |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`，本篇 9 个测试（identity 5 + auth 4）。

| 测试名 | 覆盖 C 位置 | 行为 |
|--------|-------------|------|
| `test_self_name_first` | `store.c:118-119` | 自名不查表 |
| `test_name_through_label` / `test_unknown_endpoint_nameless` | `store.c:121-124` | 反查经 label 道 / 无名回 `None` |
| `test_endpoint_roundtrip` / `test_unknown_name_endpointless` | `store.c:135-137` | 正查往返 / 未知回 `None`（不 panic） |
| `test_open_gate_allows_stranger` | `store.c:148-149` | 门没设，连无名者都过 |
| `test_owner_passes_set_gate` | `store.c:151-152` | 主人过自己的门 |
| `test_stranger_refuses_set_gate` | `store.c:151-152` | 生人不过设了的门，未设的门仍过 |
| `test_nameless_caller_closed` | `store.c:152` | 无名遇设门必拒 |

---

## 6 过渡

身份和权限讲完了：谁是谁（翻译），谁能碰谁（判定）。下一站 06——DS 启动时做的第一件实事：把 RS 的整张服务表登记进来（`sef_cb_init_fresh` + `map_service`）。

## 7 参见

- C 源：`minix3/minix/servers/ds/store.c:110-156`
- 阶段文档：`04-ds-slot-management.md`（上一站，查找）、`06-ds-boot-mapping.md`（下一站，owner="rs" 的来源）
- Rust 实现：`os/servers/ds/src/identity.rs`、`os/servers/ds/src/auth.rs`
