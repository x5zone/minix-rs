# 02 — DS 消息契约：信封上每个栏位的含义

> **分类**: 协议面 / 消息与标志
> **源码**: `minix3/minix/include/minix/com.h:498-507`、`ipc.h:93-116`、`ds.h`、`sysinfo.h:13`
> **说明**: DS 的全部跨进程对话都经过两种消息结构和一套标志位。本文把每个栏位、每个标志、每条 grant 规则讲清楚——这是 07~11 所有 handler 的共同前置知识。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要写 handler 或客户端代码的读者。前置知识：01 的分发表（知道有七种调用），C 结构体和位运算。grant（跨进程内存授权）的概念在 1.3 现场解释。

### 1.2 本章不讲什么

- 服务器收到信之后查哪张表——那是 03/04 的事。
- 谁有资格读写——那是 05 的事。
- 客户端库每个函数的包装——那是 12 的事（本篇只讲 grant 的通用规则）。

### 1.3 消息长什么样

DS 的请求和响应是两种定长消息（都放在 Minix3 通用的 `message` 联合里，56 字节净荷内能装下，布局没有歧义）：

**请求** `mess_ds_req`（`ipc.h:107-115`，7 个栏位）：

| 栏位 | 类型 | 用途 |
|------|------|------|
| `key_grant` | grant id | 键名的跨进程授权：读（发键出去）或写（收键回来） |
| `key_len` | 长度 | 键字节数；合法范围 2~80（含结束符） |
| `flags` | 标志 | 类型臂 + 权限门 + 修饰（OVERWRITE / INITIAL），详见 1.4 |
| `val_in` | `union ds_val` | 带入的值：grant（STR/MEM 数据）/ `u32`（数字）/ `ep`（端点） |
| `val_len` | 长度 | STR/MEM 的数据长度 |
| `owner` | 端点 | 99% 的时间不用——唯一的例外是 `do_check` 把它**复用**成回信栏（见下） |
| `padding` | 填充 | 对齐，无语义 |

**响应** `mess_ds_reply`（`ipc.h:100-104`）：`val_out`（同形状的联合）+ `val_len`（实际搬了多少字节）。

**值联合** `union ds_val`（`ipc.h:94-98`）：`grant` / `u32` / `ep` 三选一。注意它**没有 label 臂**——label 的端点走 `u32` 臂（03 会细讲"标签寄数字道"）。

grant 规则（`do_invoke_ds`，`lib/libsys/ds.c:7-33`）只有两条：`CHECK` 和 `RETRIEVE_LABEL` 是"收回答"，借 80 字节**可写**授权；其余调用是"发名字"，借 `strlen + 1` 字节**只读**授权。方向搞反，数据就流错了方向。

一个容易错过的细节：`do_check` 的回信不走 `m_ds_reply`，而是**写回请求结构**——`m_ds_req.flags` 填条目类型，`m_ds_req.owner` 填发布者端点（`store.c:570-572`）。客户端的 `ds_check` 就是这么读回来的（`ds.c:215-216`）。

### 1.4 标志位全集（`ds.h:12-32`）

| 名字 | 值 | 含义 | 哪里用 |
|------|----|------|--------|
| `DSF_IN_USE` | `0x001` | 槽位被占用 | 取/寻/放三处门判（04） |
| `DSF_PRIV_RETRIEVE` | `0x002` | 置位时仅 owner 可读 | `do_retrieve`（08） |
| `DSF_PRIV_OVERWRITE` | `0x004` | 置位时仅 owner 可覆盖 | `do_publish`（07） |
| `DSF_PRIV_SNAPSHOT` | `0x004` | 与上一行**同值**，死定义 | 无处引用（A-7） |
| `DSF_PRIV_SUBSCRIBE` | `0x008` | 置位时条目仅对 owner 可订阅匹配 | `check_sub_match`（10） |
| `DSF_TYPE_U32` | `0x010` | 数字类型 | 类型四臂 |
| `DSF_TYPE_STR` | `0x020` | 字符串类型 | 类型四臂 |
| `DSF_TYPE_MEM` | `0x040` | 内存块类型 | 类型四臂 |
| `DSF_TYPE_LABEL` | `0x100` | 标签类型（值为端点） | 类型四臂 |
| `DSF_MASK_TYPE` | `0xFF0` | 类型掩码（含 `0x80` 等未用位） | 取类型臂 |
| `DSF_MASK_INTERNAL` | `0xFFF` | 落盘掩码：priv + type 保留 | publish 写 flags（07） |
| `DSF_OVERWRITE` | `0x1000` | 允许覆盖已存在条目（订阅复用：允许替换旧订阅） | 07 / 10 |
| `DSF_INITIAL` | `0x2000` | 订阅后立即全表匹配并通知 | 10 |

两个"坑"要记住：`0x80` 在类型掩码里占了一位但从未命名（以后有人用了这一位，掩码不用改）；`PRIV_SNAPSHOT` 和 `PRIV_OVERWRITE` 同值——快照功能从没实现（A-7），这个名字是历史残留，读到不要当真。

### 1.5 调用号与查询号

`DS_RQ_BASE = 0x800`（`com.h:498`），7 个活号 + 1 个死号（见 01 的表，不复述）。`SI_DATA_STORE = 5`（`sysinfo.h:13`）是 `do_getsysinfo` 唯一接受的查询号（11）。

### 1.6 小结

请求 7 栏、响应 2 栏、值联合 3 选 1、标志 12 个（1 死）、grant 两条规则、check 回信复用请求栏。handler 篇不会再解释这些，只会引用。

---

## 2 C 源码分析

### 2.1 调用号（`com.h:498-507`，基 `DS_RQ_BASE = 0x800`）

`PUBLISH +0` / `RETRIEVE +1` / `SUBSCRIBE +2` / `CHECK +3` / `DELETE +4` / `SNAPSHOT +5`（死）/ `RETRIEVE_LABEL +6` / `GETSYSINFO +7`。注意 `+5` 的空缺是有意的（死号），不是笔误。

### 2.2 消息结构（`ipc.h:93-116`）

- `union ds_val`：`grant`（`cp_grant_id_t`）、`u32`（`u32_t`）、`ep`（`endpoint_t`），三臂共用 4 字节。
- `mess_ds_req`：`key_grant`、`key_len`、`flags`、`val_in`、`val_len`、`owner`（端点，回信复用）、填充。
- `mess_ds_reply`：`val_out`、`val_len`、填充。
- `mess_lsys_getsysinfo`（`ipc.h:1065-1071`）：`what` / `size` / `where`，11 用。

### 2.3 标志与常量（`ds.h` 全文件 68 行）

`DSF_*` 见 1.4 表；`DS_MAX_KEYLEN = 80`（`:35`，含结束符）；`DS_DRIVER_UP = 1`（`:37`，事件号，本组源码内无使用，驱动库用）。

### 2.4 死代码确认（A-7，grep 实证）

| 项 | 证据 |
|----|------|
| `do_snapshot` | 仅 `proto.h:16` 声明，`servers/ds/` 无定义 |
| `DS_SNAPSHOT` | `com.h:505` 声明，`main.c` switch 无分支，`lib/libsys/ds.c` 无发送 |
| `ds_*_map` 四个函数 | `ds.h:53-58` 声明，全 `minix3/` 树无实现 |
| `DSF_PRIV_SNAPSHOT` | `ds.h:21` 定义，`store.c` 零引用 |

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 标志变 bitflags | 散落 `#define`（`ds.h:12-26`） | `DsFlags`（`minix-types/src/types/com.rs`） | 类型臂和权限门都是位运算，用 bitflags 后 `intersects` / `contains` 把"交集即成员"这类规则写进类型；`PRIV_SNAPSHOT` 同值问题用文档注释标明，不改值（改值会破坏 ABI 对照） |
| D2 | 调用号变常量 + 枚举两层 | 裸 `#define` | 常量（`minix-types`）+ `DsCall` 枚举（`dispatch.rs`，01 D1） | 常量层保证数值可对照 C，枚举层保证分发无遗漏 |
| D3 | check 回信复用显式化 | 直接写 `m_ptr->m_ds_req.flags/owner`（`store.c:570-572`） | `CheckHit::reply_type` / `reply_owner`（`check.rs`）+ `CheckReply`（`client.rs`） | "请求栏里装回信"是全系统最容易误读的地方，值得两个命名函数；`reply_owner` 回 `Option`（C 会 panic 的情况，见 05 D2） |
| D4 | grant 规则变纯函数 | `if/else` 内联在 `do_invoke_ds`（`ds.c:13-19`） | `key_grant(call, name_len)`（`client.rs`） | 两条规则值得一个名字 + 两个测试；transport 落地时直接调它 |

---

## 4 实现详解

### 4.1 模块结构

```
minix-types/src/types/com.rs  — DsFlags / DS_* 常量 / DSF_MASK_* / DS_MAX_KEYLEN
os/servers/ds/src/
├── dispatch.rs   — DsCall（调用号的枚举面，01）
├── check.rs      — CheckHit::reply_type / reply_owner（回信复用面）
└── client.rs     — key_grant / flags / CheckReply（客户端纯逻辑面）
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 标志全集 | `ds.h:12-26` | `minix-types/.../com.rs`（`DsFlags`） | 位运算类型化，值与 C 一致 |
| 调用号 | `com.h:498-507` | 同上（`DS_*` 常量） | 数值与 C 一致 |
| grant 规则 | `ds.c:13-19` | `client.rs`（`key_grant`） | CHECK/LABEL 收 80 可写，其余发 strlen+1 只读 |
| check 回信 | `store.c:570-572` | `check.rs` + `client.rs`（`CheckReply`） | 类型掩码 + 发布者端点 |

---

## 5 测试要点

| 测试名 | 覆盖 | 行为 |
|--------|------|------|
| `minix-types` 的 `test_ds_messages` | `com.h:498-507`、`ds.h` | 常量值对照 |
| `test_check_and_label_lend_roomy_writable_grant` | `ds.c:13-16` | 收回答：80 + 可写 |
| `test_other_calls_lend_strlen_plus_one_readable` | `ds.c:17-19` | 发名字：strlen+1 + 只读 |
| `test_flag_assembly` | `ds.c:36-92` | 类型臂 + 修饰透传 |

---

## 6 过渡

协议面讲完了：以后看到 `flags` 知道是哪些位，看到 grant 知道往哪流。下一站 03——这些名字和值在服务器内存里到底存在哪（两张表的形状）。

## 7 参见

- C 源：`minix3/minix/include/minix/{com.h,ipc.h,ds.h,sysinfo.h}`、`minix3/minix/lib/libsys/ds.c:7-33`
- 阶段文档：`01-ds-init-main.md`（上一站）、`03-ds-data-structures.md`（下一站）、`12-ds-client-library.md`（grant 规则的另一半）
- Rust 实现：`os/libs/minix-types/src/types/com.rs`、`os/servers/ds/src/client.rs`
