# 08 — DS 检索：按名读、按端点读，以及"读多少"和"谁不许读"

> **分类**: 数据面 handler / 检索
> **源码**: `minix3/minix/servers/ds/store.c:383-454`
> **说明**: 检索是发布的镜面：发布是"判了才能立"，检索是"判了才能读"。本文讲清两条读路径（按名、按端点）、字节读的截断规则，以及检索门。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要实现或调用检索的读者。前置知识：07 的发布流程（检索是它的逆操作），05 的权限判定，02 的响应消息。`MIN` 取小和 grant 传输的概念现场解释。

### 1.2 本章不讲什么

- 发布怎么写——那是 07 的事。
- 订阅者怎么被唤醒——那是 10 的事。
- 客户端每个读函数的包装——那是 12 的事（本篇只讲服务器侧 verdict）。

### 1.3 两条读路径

| 路径 | C 函数 | 输入 | 输出 | 权限门 |
|------|--------|------|------|--------|
| 按名读 | `do_retrieve`（`:383-427`） | 键名 + 类型 | 值（数/端点直填回信，串/存经 grant 拷） | `PRIV_RETRIEVE`（05 判定） |
| 按端点读 | `do_retrieve_label`（`:432-451`） | 端点 | 键名（含结束符，经 key grant 拷） | **无** |

第二条路径没有权限门——C 不查。这不是疏忽：label 表本来就是公开的注册表（谁是 RS、谁是 VFS，人人可问），设门没有意义。但记住这个不对称：按名读可能 `EPERM`，按端点读永远不 `EPERM`（只有 `ESRCH` 和拷贝错）。

### 1.4 字节读的截断规则

数字和端点一次全回（4 字节，填进回信）。字符串和内存块走 grant 拷，拷多少取小：`MIN(调用方要的, 条目存的)`（`:412`）。调用方房间小 → 截断（静默的，但回信 `val_len` 写了实际搬的字节数，`:420`——短读和全读可区分）；调用方房间大 → 全搬。要多少、有多少，取小者——这是所有"变长读"的通用契约（`read(2)` 同理）。

### 1.5 检索门：四拒

按名读四步（C 序）：键门（`EINVAL`）→ 查无（`ESRCH`）→ 门闭（`EPERM`，`PRIV_RETRIEVE`，05 判定：门没设则过）→ 类型臂未知（`EINVAL`，`:423`——查到了但类型臂是 0 或多位，读哪臂都无意义）。

### 1.6 小结

两径（名读有门，端点读无门），截断取小（回信写实搬），四拒（键/空/门/型）。下一站 09——删（检索的破坏版，多了级联）。

---

## 2 C 源码分析

### 2.1 按名读（`do_retrieve`，`:383-427`）

取键（`:393`，07 的键门）→ 查（`:397`，名型双判，无→`ESRCH`）→ 门（`:399`，`check_auth(RETRIEVE)`，不过→`EPERM`）→ 按臂回：`U32` 填 `val_out.u32`（`:405`）、`LABEL` 填 `val_out.ep`（`:408`）、`STR`/`MEM` 取小拷（`:412-413`）回 `val_len`（`:420`）、未知臂 `EINVAL`（`:423`）→ `OK`。

### 2.2 按端点读（`do_retrieve_label`，`:432-451`）

端点反查（`:438`，04 的端点反查，无→`ESRCH`）→ `safecopyto(key_grant, key, strlen+1)`（`:442-444`，键含结束符）→ `OK`。无门、无类型分支——label 只有一种形状。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 读 verdict 纯化 | 查表 + 门 + 回填一锅 | `plan_retrieve`（`retrieve.rs`）：键门→查找→门→臂，传输剥离 | 拷贝（`safecopyto`）是传输（02/12），"读什么、读多少、许不许" 是 verdict；verdict 纯可测 |
| D2 | 命中变枚举 | 回填不同回信栏 | `RetrieveHit::{Number, Label, Bytes{len}}` | 读到什么，一看类型就知道；字节臂只定长度（拷是传输的事） |
| D3 | 取小变具名函数 | `MIN` 宏内联（`:412`） | `truncated_len(requested, stored)` | 一句话的规则值得一个名字 + 三个测试（小于/大于/等于） |
| D4 | 端点读独立函数 | 独立 C 函数 | `plan_retrieve_label` 独立（不复用 `plan_retrieve`） | 两径门不同（有门/无门）、查不同（名查/端点查）——硬复用会把"无门"这条关键差异藏进参数 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/ds/src/
├── retrieve.rs — 本篇：RetrieveReject / RetrieveHit / truncated_len / plan_retrieve / plan_retrieve_label
├── slots.rs    — 04：名查与端点反查（verdict 的 instruments）
├── auth.rs     — 05：检索门判定
└── publish.rs  — 07：check_key_len（键门共享，D4 于 07）
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 四拒 | `store.c:393-423` | `retrieve.rs`（`RetrieveReject`） | 键/空/门/型→errno 一一对应 |
| 三命中 | `store.c:405-420` | `retrieve.rs`（`RetrieveHit`） | 数/签/字节长 |
| 取小 | `store.c:412` | `retrieve.rs`（`truncated_len`） | `min(要, 存)` |
| 名读 verdict | `store.c:393-427` | `retrieve.rs`（`plan_retrieve`） | 五步（C 序） |
| 端点读 verdict | `store.c:438-450` | `retrieve.rs`（`plan_retrieve_label`） | 反查，无门 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 键门三处同源（07/08/09） | `check_key_len` 唯一长度观 | `store.c:163` |
| 端点读无门 | 独立函数，无 auth 调用 | `store.c:432-451` 全文 |
| 短读可区分 | `Bytes{len}` 即实搬长 | `store.c:420` |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`，本篇 8 个测试。

| 测试名 | 覆盖 C 位置 | 行为 |
|--------|-------------|------|
| `test_open_entry_reads_back` | `:405` | 公开条目数回读 |
| `test_guarded_entry_refuses_stranger` | `:399-400` | 设门拒生人，主人过 |
| `test_missing_entry_is_esrch` | `:397` | 查无 `ESRCH` |
| `test_bad_key_is_einval` | `:393` | 坏键 `EINVAL` |
| `test_unknown_type_arm_is_einval` | `:423` | 未知臂 `EINVAL` |
| `test_truncation_is_min` | `:412` | 取小三态 |
| `test_label_lookup_by_endpoint` | `:438` | 端点反查 |
| `test_errno_mapping` | 全章 | 四拒→errno |

---

## 6 过渡

检索（读）讲完了。下一站 09——删：查无、键门和 08 一样，但"只有主人能删"（门一律关），删 label 还带级联（订阅和条目一起清）。

## 7 参见

- C 源：`minix3/minix/servers/ds/store.c:383-454`
- 阶段文档：`07-ds-publish.md`（上一站，写的镜面）、`09-ds-delete.md`（下一站）、`12-ds-client-library.md`（读函数的客户端面）
- Rust 实现：`os/servers/ds/src/retrieve.rs`
