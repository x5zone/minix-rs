# 11 — DS 镜像外借：别的服务器怎么一次性读走整张表

> **分类**: 数据面 handler / 系统信息查询
> **源码**: `minix3/minix/servers/ds/store.c:653-678`、`servers/is/dmp_ds.c:8-45`、`lib/libsys/getsysinfo.c:22`
> **说明**: `do_getsysinfo` 是 DS 最短的 handler（两道门 + 一次拷贝），但它背着全阶段最重的契约：03 的 192 字节布局、04 的分配顺序，在这里一次性兑现。本文讲清这两道门和这份契约。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要实现或调用镜像查询的读者。前置知识：03 §1.6 的跨服契约（布局即协议），01 的分发表。`sys_datacopy` 知道是"内核帮两个进程拷内存"即可。

### 1.2 本章不讲什么

- 条目布局为什么是 192 字节——那是 03 的事（本篇只用结论）。
- 客户端 `getsysinfo` 的通用包装——`libsys/getsysinfo.c:22` 只是把 `DS_PROC_NR + DS_GETSYSINFO` 转交，一句话带过。
- IS 的显示逻辑——`dmp_ds.c` 的分页打印是 IS 的事，本篇只讲它对 DS 的**要求**。

### 1.3 两道门，一次拷贝

`do_getsysinfo`（`:653-678`）只做三件事：查 `what` 是不是 `SI_DATA_STORE`（`:659-665`，不是→`EINVAL`）→ 查 `size` 是不是正好 `sizeof(ds_store)`（`:668-669`，多一字节少一字节都→`EINVAL`）→ `sys_datacopy` 从自己拷给调用者（`:671-672`）→ `OK`。

size 的**精确匹配**值得多说一句：短了会截断镜像（IS 读到半个条目），长了会把 DS 内存里表后面的东西漏出去（越界读）。C 两边都不让——"正好"之外全拒。这是对"布局即协议"最便宜的保险：调用者和实现对"表多大"的认知必须逐字节一致，否则宁可不服务。

### 1.4 契约的三方兑现

镜像查询是 03（布局）+ 04（顺序）+ 本篇（门与拷）的汇合点：

```
03 定布局（192 字节/槽，#[repr(C)] + 断言锁死）
  → 04 定顺序（首适应升序，镜像第 N 个即表第 N 个）
    → 本篇定门拷（what 对、size 正好，才拷 image_bytes）
      → IS 直读（dmp_ds.c 按 struct 解释，无解析无版本）
```

任一环松了，IS 静默读错——没有错误码，只有错数据。所以本篇的测试锁 `image_bytes() == 192 * 128`（常量式，非 `size_of` 自证：断言必须有一个**手写**的期望值，否则实现和测试会一起错）。

调用链（`getsysinfo.c:22`）：客户端调通用 `getsysinfo(DS_PROC_NR, SI_DATA_STORE, buf, sizeof)` → 转成 `DS_GETSYSINFO` 发 DS → DS 回拷。IS 的 `data_store_dmp`（`dmp_ds.c:8`）就是这么拿快照分页显示的（22 行一页，`prev_i` 轮转）。

### 1.5 小结

两门（问对、量准），一拷（全表直拷），三方兑现（布局+顺序+门拷）。下一站 12——线的另一头：客户端库。

---

## 2 C 源码分析

### 2.1 查询 verdict（`:653-678`）

`switch(what)`（`:659`）：`SI_DATA_STORE` 取源地址 + 全表长（`:660-662`），default→`EINVAL`（`:664-665`）→ size 精确比（`:668-669`）→ `sys_datacopy(SELF→caller)`（`:671-672`，失败打日志回错码）→ `OK`。

### 2.2 消费者（`dmp_ds.c:8-45`）

取快照（`:15`，`sizeof` 对账——IS 侧也按 192×128 算，两边同数）→ 分页（`:22` 行，`prev_i` 轮转）→ 逐槽显示槽号/键/主/类型值（`:30-41`）。失败只打印（`:16-18`），不崩——显示工具不配崩。

### 2.3 转交（`libsys/getsysinfo.c:22`）

通用入口按目标服务器转交，DS 侧表现为 `DS_GETSYSINFO` 调用（01 分发表 `:72`）。无 DS 特有逻辑，不展开。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 镜像长变算式 | `sizeof(ds_store)` 用时现算（`:662,668`） | `image_bytes() = size_of::<DataEntry>() * NR_DS_KEYS`（`getsysinfo.rs`） | 长是布局与容量的函数，不是魔法数；表变布局变，镜像自动跟 |
| D2 | 查询号本地定 | `SI_DATA_STORE` 在 `sysinfo.h:13` | `SI_DATA_STORE = 5`（`getsysinfo.rs`，引 C 行） | `minix-types` 暂无 sysinfo 模块（A-1 余部）；值引源注释，不自创 |
| D3 |  verdict 纯化 | 门 + 拷贝一锅 | `plan_getsysinfo(what, size)` 回镜像长 | 拷贝是传输（02），"让不让拷、拷多少" 是 verdict；verdict 纯可测 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/ds/src/
└── getsysinfo.rs — 本篇：SI_DATA_STORE / GetsysinfoReject / image_bytes / plan_getsysinfo
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 查询号 | `sysinfo.h:13` | `getsysinfo.rs`（`SI_DATA_STORE`） | 唯一合法 `what` |
| 两拒 | `store.c:659-669` | `getsysinfo.rs`（`GetsysinfoReject`） | 问错/量错→`EINVAL` |
| 镜像长 | `store.c:662` | `getsysinfo.rs`（`image_bytes`） | 条目长 × 表容量 |
| 查询 verdict | `store.c:659-672` | `getsysinfo.rs`（`plan_getsysinfo`） | 两门（C 序），过则回长 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 量差一字节也拒 | 精确 `!=` 比 | `store.c:668-669` |
| 镜像长手写期望 | 测试锁 `192 * 128` | `dmp_ds.c:15` 对账 |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`，本篇 5 个测试。

| 测试名 | 覆盖 C 位置 | 行为 |
|--------|-------------|------|
| `test_image_is_entry_times_table` | `:662` | 镜像 = 条目 × 表（手写 192×128） |
| `test_exact_size_passes` | `:668` | 量准过 |
| `test_wrong_what_refuses` | `:659-665` | 问错拒 |
| `test_short_and_long_sizes_refuse` | `:668-669` | 长短都拒（含 0） |
| `test_errno_mapping` | 全章 | 两拒→`EINVAL` |

---

## 6 过渡

服务器侧七个 handler 讲完了（07 发布、08 检索、09 删除、10 订阅取阅、11 镜像）。下一站 12——线的另一头：客户端库（grant 怎么借、字符串怎么封尾、check 回信怎么读）。

## 7 参见

- C 源：`minix3/minix/servers/ds/store.c:653-678`、`servers/is/dmp_ds.c`、`lib/libsys/getsysinfo.c:22`
- 阶段文档：`10-ds-subscribe-check.md`（上一站）、`12-ds-client-library.md`（下一站）、`03-ds-data-structures.md`（布局契约的源头）
- Rust 实现：`os/servers/ds/src/getsysinfo.rs`
