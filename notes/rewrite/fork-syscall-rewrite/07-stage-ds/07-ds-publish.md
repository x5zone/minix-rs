# 07 — DS 发布：谁能立、立在哪、能不能盖

> **分类**: 数据面 handler / 发布
> **源码**: `minix3/minix/servers/ds/store.c:287-381`、`158-181`（`get_key_name`）
> **说明**: 发布是 DS 最复杂的写路径：七个拒绝理由、两种落法（新建/覆盖）、四种类型、四种堆动作。本文按 C 的顺序走一遍，并讲清 Rust 为什么把"判"和"立"拆开。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要实现或调用发布的读者。前置知识：02 的消息栏位，04 的分配/查找，05 的翻译与判定。`malloc`/`free` 知道是堆分配释放即可。

### 1.2 本章不讲什么

- 检索和删除——那是 08/09 的事。
- 订阅者怎么被唤醒——发布末尾调 `update_subscribers`，但那是 10 的事（本篇只讲"调了它"，不讲"它干了什么"）。
- 堆分配器选哪个——A-3 未决，本篇只讲"什么时候要堆、谁负责放"。

### 1.3 发布的七个拒绝

`do_publish`（`store.c:287-381`）按顺序过七道门，倒在任何一道都直接回错误码：

```
源无名？──────────────→ EPERM（:298，端点查不到名字）
label 但非 RS？───────→ EPERM（:302，只有 RS 能发布 label）
键不合法？────────────→ EINVAL（:306，长度/拷贝失败）
槽满了？（新建时）────→ ENOMEM（:317）
已存在且没说覆盖？────→ EEXIST（:325）
已存在、说了覆盖但门不让？→ EPERM（:321，PRIV_OVERWRITE 判定）
类型不认识？──────────→ EINVAL（:366）
```

顺序是语义的一部分：比如"键不合法"排在"槽满"前面——键都错了，不值得先占槽。全过之后才动堆、写栏、跑通知环（`:375`）。

### 1.4 两种落法，四种类型

- **新建**：没查到同名同类型条目 → 取槽 → 全栏写入。满了回 `ENOMEM`。
- **覆盖**：查到了 → 必须带 `OVERWRITE` 修饰（否则 `EEXIST`）→ 过 `PRIV_OVERWRITE` 门（05 的判定，门没设则过）→ 按类型重写值。

四种类型的写动作各不相同：`U32` 直接赋值（`:331`）；`LABEL` 存端点（`:334`，只有 RS 能到这）；`STR`/`MEM` 走堆——新槽 `malloc`（`:341`），旧槽只有"新长度超过已分配"才 `free + malloc`（`:344-349`，小改不动，省分配），然后 `safecopyfrom` 把数据从调用方拷进来（`:352`，失败则释槽回错），`STR` 末尾钉结束符（`:360-363`）。落盘 flags 只保留 priv + type（`IN_USE | (flags & INTERNAL)`，`:372`）——请求带的 `OVERWRITE`/`INITIAL` 是"修饰"，不是"属性"，不进表。

### 1.5 判立分离：Rust 为什么拆两步

C 的 `do_publish` 是"判 + 立"一锅：verdict、堆动作、栏写入、通知全在一个函数里。Rust 拆成 `plan_publish`（`publish.rs`，纯 verdict：七拒之一定位 + 新建/覆盖二选一）和 commit 半（堆/传输/写栏/通知，A-3 + 02/12 + 10）：

- verdict 是纯逻辑，给表和参数就能判——7 个拒绝理由 × 两种落法，单元测试全覆盖，不需要堆、不需要 IPC。
- commit 需要分配器（A-3 未定）和传输（02/12 未落地）——不定不等判，先判后立，到货即装。

这是"决策与执行分离"的常规做法（Linux 的 page fault 先 `find_vma` 判、再 `handle_mm_fault` 立；Redox 的方案调用先鉴权再执行），不是为拆而拆。

### 1.6 小结

七拒（源/签/键/满/存/盖/型），两落（新建全写，覆盖重写值），四型（数签直赋，串存走堆），判立分离（先 verdict，后 commit）。下一站 08——反向读：检索。

---

## 2 C 源码分析

### 2.1 键搬运（`get_key_name`，`store.c:158-181`）

`key_len ∈ [2, 80]` 校验（`:163`，裸 NUL 不成名，超 80 装不进道）→ `sys_safecopyfrom(key_grant)`（`:170`，跨服取键）→ 末字节钉 `\0`（`:178`）。Rust 侧纯半是 `check_key_len`（`publish.rs:40`），传输半归 02/12——08/09 复用同一门，不各立长度观。

### 2.2 发布主流程（`store.c:297-378`）

源（`:297-299`，`ds_getprocname(src)`，NULL→`EPERM`）→ label 门（`:302`，label 且源非 RS→`EPERM`）→ 键（`:306`）→ 寻（`:310` 名型双判；`:312-313` label 未命中再按端点查——同一 label 两种找法，防"改名不同步"的双重登记）→ 落（`:317-325`，新建取槽/满 `ENOMEM`；覆盖要 `OVERWRITE` 否则 `EEXIST`，`:321` 门不过 `EPERM`）→ 写（`:329-372`，四型四动作，见 §1.4）→ 通知（`:375` `update_subscribers(dsp, 1)`）→ `OK`。

### 2.3 堆动作细节（`:341-363`）

新槽 `malloc(len)`（`:341`）；旧槽 `len > reallen` 才换（`:344-349`）；`safecopyfrom` 失败释堆回错（`:352-359`，不留半截）；记 `length`，`STR` 钉尾（`:360-363`）；未知类型 `EINVAL`（`:366`，在堆动作之后、写栏之前——类型错了，堆已释，不脏表）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 判立分离 | 一锅烩 | `plan_publish`（verdict）+ commit 半（待 A-3/02/12/10） | verdict 纯可测；commit 依赖未决事项，不等 |
| D2 | 拒绝变枚举 | 七个 `return errno` | `PublishReject` 七变体 + `errno()`（`publish.rs`） | 错误码与 Minix3 一一对应（无自创）；总数进类型，第八种拒绝不可表达 |
| D3 | 落法变枚举 | `dsp` 指针 + 隐式新旧 | `PublishTarget::{Create, Overwrite}`（`publish.rs`） | commit 半不再复判：verdict 说了是新建还是覆盖，commit 照做 |
| D4 | 键门共享 | `get_key_name` 内联 bounds | `check_key_len` 纯半（`publish.rs:40`），08/09 复用 | 长度观只有一处，三处同门 |
| D5 | 传输剥离 | `safecopyfrom` 内联 | verdict 收已搬运的 `key` 切片 | "键怎么来的"（grant）与"键合不合法"分开，前者归 02/12 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/ds/src/
├── publish.rs — 本篇：MIN_KEY_LEN / check_key_len / PublishReject / PublishTarget / plan_publish
├── slots.rs   — 04：取槽与查找（verdict 的 instruments）
├── auth.rs    — 05：覆盖门判定
└── notify.rs  — 10：通知环（commit 半调它，本篇只声明调用点）
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 键门 | `store.c:163` | `publish.rs:31,40` | 2..=80，否则 `EINVAL` |
| 七拒 | `store.c:297-326,366` | `publish.rs:53`（`PublishReject`） | 变体→errno 一一对应 |
| 两落 | `store.c:310-326` | `publish.rs:89`（`PublishTarget`） | 新建取槽 / 覆盖已授权 |
| 发布 verdict | `store.c:297-326,329,366` | `publish.rs:115`（`plan_publish`） | 六步（C 序）：源→签→键→寻→落→型 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 键门三处同源 | `check_key_len` 唯一长度观 | `store.c:163` |
| 非 RS 不立签 | `is_rs` 门在 verdict 第二步 | `store.c:302` |
| 修饰不落盘 | 落盘掩码 priv+type | `store.c:372` |
| 新发布必过环 | commit 半调 `update_subscribers(…,1)` | `store.c:375` |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`，本篇 9 个测试。

| 测试名 | 覆盖 C 位置 | 行为 |
|--------|-------------|------|
| `test_reject_unknown_source` | `:298` | 无名源 `EPERM` |
| `test_reject_label_not_rs` | `:302` | 非 RS 立签 `EPERM` |
| `test_reject_bad_key` | `:306` | 坏键 `EINVAL` |
| `test_reject_exists` / `test_overwrite_*` | `:321-325` | 无修饰 `EEXIST` / 有修饰过门 |
| `test_reject_full_house` | `:317` | 满 `ENOMEM` |
| `test_reject_waste_type` | `:366` | 未知类型 `EINVAL` |

---

## 6 过渡

发布（写）讲完了。下一站 08——反向读：检索（按名读、按端点读、读多少、谁不许读）。

## 7 参见

- C 源：`minix3/minix/servers/ds/store.c:287-381,158-181`
- 阶段文档：`06-ds-boot-mapping.md`（上一站）、`08-ds-retrieve.md`（下一站）、`10-ds-subscribe-check.md`（通知环）
- Rust 实现：`os/servers/ds/src/publish.rs`
