# 12 — DS 客户端库：线的另一头怎么打包请求

> **分类**: 客户端契约 / 协议闭环
> **源码**: `minix3/minix/lib/libsys/ds.c`（219 行）、`ds.h:40-68`
> **说明**: 前面 11 篇都是服务器侧。本篇讲线的另一头：客户端 16 个 `ds_*` 函数如何打包请求——借多大的 grant、朝哪个方向、字符串怎么封尾、check 的回信去哪读。传输本身（`_taskcall`）是 `minix-sys` 的事（A-8），本篇只定"打包规则"。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要写客户端调用或传输层的读者。前置知识：02 的消息结构和 grant 规则（本篇是它的客户端实例化），08/10 的检索与取阅（知道回信长什么样）。grant（跨进程内存授权）在 02 讲过，这里直接用。

### 1.2 本章不讲什么

- 服务器收到之后干什么——那是 07~11 的事。
- `_taskcall` / `cpf_grant_direct` 怎么实现——A-8，`minix-sys` 落地后接线，本篇只定"调它们之前要备好什么"。
- 死 API（`ds_*_map`、`do_snapshot`）——A-7 排除，02 §2.4 已列，本篇不复述。

### 1.3 16 个函数，三种形状

`ds.c` 的 16 个公开函数只有三种形状：

| 形状 | 函数 | 打包要点 |
|------|------|----------|
| 数字/端点直传 | `publish_label/u32`、4×`retrieve_*`（数/签两向）、4×`delete_*` | 值放 `val_in`/`val_out`（联合臂），键走 key grant（02 规则） |
| 字节块经 grant | `publish_raw/str/mem`、`retrieve_raw/str/mem` | 数据另借 grant（读 publish 用 `CPF_READ`，写 retrieve 用 `CPF_WRITE`），`val_len` 随行 |
| 订阅取阅 | `subscribe`、`check` | `subscribe` 把 pattern 当"键"发；`check` 借 80 可写 key grant 收键名，回信读**请求栏**（02 §1.3） |

### 1.4 字符串的封尾纪律

两处 `value[length-1] = '\0'`，方向相反，用意相同：

- 发布（`ds_publish_str`，`:79-87`）：`length = strlen + 1`，**借出之前**先钉尾——保证服务器收到的永远是合法 C 串。参数是 `char *`（可写）而非 `const char *`，就为了这一钉。
- 检索（`ds_retrieve_str`，`:150-157`）：`length = len + 1`，拷回之后再钉尾——服务器搬的是字节， terminator 由客户端补齐，保证调用者拿到的永远是合法 C 串。

结束符在**长度里**（`+1` 随行），这是 grant 大小和 `val_len` 都对得上的前提。忘 `+1`，要么截尾，要么越界——两种错法各有对应测试（`test_string_lengths_carry_terminator`）。

### 1.5 check 回信的"错位"读取

`ds_check`（`:209-219`）读回 `m.m_ds_req.flags`（条目类型掩码）和 `m.m_ds_req.owner`（发布者端点）——请求结构，不是响应结构（02 §1.3，10 §1.7）。第一次写传输代码的人一定会去 `m_ds_reply` 里找，找不到就当 bug 报——本篇和 `CheckReply` 类型就是防这个误会的：类型名即提醒，"回信在请求栏里"。

### 1.6 小结

三形（直传/块/阅），两钉（借前钉、收后钉），一错位（check 读请求栏）。12 篇收官：01 骨架、02 信封、03 表、04 槽、05 名权、06 启动、07 立、08 读、09 删、10 阅、11 像、12 线——DS 全阶段闭环。

---

## 2 C 源码分析

### 2.1 总机（`do_invoke_ds`，`:7-33`）

键 grant 定尺寸（`:13-19`：CHECK/LABEL 收 80 可写，其余发 `strlen+1` 只读）→ `cpf_grant_direct(DS)`（`:22-24`，无效→`ENOMEM`）→ 填 `key_grant/key_len`（`:28-29`）→ `_taskcall(DS_PROC_NR, type)`（`:31`）→ `cpf_revoke`（`:33`，借了必还）→ 回 `r`。

### 2.2 发布组（`:36-92`）

`publish_label`（`:36`，`val_in.ep` + LABEL 臂）/ `publish_u32`（`:46`，`val_in.u32` + U32 臂）/ `publish_raw`（`:56`，数据另借 READ grant + `val_len`）/ `publish_str`（`:79`，`strlen+1` + 借前钉尾 + STR 臂）/ `publish_mem`（`:87`，MEM 臂）。

### 2.3 检索组（`:92-162`）

`retrieve_label_name`（`:92`，`val_in.ep` → LABEL 读，键经 key grant 回）/ `retrieve_label_endpt`（`:103`，LABEL 臂读 `val_out.ep`）/ `retrieve_u32`（`:115`，U32 臂读 `val_out.u32`）/ `retrieve_raw`（`:127`，数据借 WRITE grant，`*length = val_len` 写回）/ `retrieve_str`（`:150`，`len+1` + 收后钉尾）/ `retrieve_mem`（`:159`）。

### 2.4 删除组与订阅组（`:164-219`）

`delete_*` ×4（`:164-197`，各类型臂 + 键 grant）/ `subscribe`（`:200`，pattern 当键发，flags 透传）/ `check`（`:209`，80 可写 key grant + 请求栏回读）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | grant 规则纯化 | `if/else` 内联（`:13-19`） | `key_grant(call, name_len)`（`client.rs`） | 02 D4 同源：两条规则一个名字，传输落地即调 |
| D2 | 封尾变函数 | `value[length-1] = '\0'` 两处手写 | `terminate(buffer, length)`（`client.rs`） | 两处同式，一处实现；空缓冲不钉（C 靠"长度恒 ≥1"避 UB，Rust 靠类型避——`length == 0` 直接返） |
| D3 | 长度 `+1` 归边 | 各函数手写 `strlen + 1` | `publish_str_len` / `retrieve_str_len` | `+1` 是契约（结束符随行），不是算术——命名函数让"忘 +1"可测 |
| D4 | 标志组装变函数 | 各函数手写 `flags` | `flags::{publish_*, arm_only}`（`client.rs`） | 类型臂 + 修饰透传的组装只有一种对法；`arm_only` 把"读/删只用裸臂"钉死 |
| D5 | 错位回读变类型 | 读 `m.m_ds_req.*`（`:215-216`） | `CheckReply { entry_type, owner }`（`client.rs`） | 类型即文档：看到 `CheckReply` 就知道"回信不在响应里" |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/ds/src/
└── client.rs — 本篇：GrantDirection / key_grant / publish_str_len /
                retrieve_str_len / terminate / flags / CheckReply
```

传输（grant 生效、`_taskcall` 发送、revoke）归 `minix-sys`（A-8，当前 stub）：接线时 `key_grant` 的返回值直 feed 给 `cpf_grant_direct` 的尺寸与方向参数，`terminate` 的调用点与 C 同位（借前/收后）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 键 grant 规则 | `ds.c:13-19` | `client.rs`（`key_grant`） | 收 80 可写 / 发 strlen+1 只读 |
| 串长 | `ds.c:79-87,150-157` | `client.rs`（`publish/retrieve_str_len`） | 结束符随行 |
| 封尾 | `ds.c:84,155` | `client.rs`（`terminate`） | 钉末字节，空缓冲不钉 |
| 标志组装 | `ds.c:36-197` | `client.rs`（`flags`） | 类型臂 + 修饰透传 |
| check 回读 | `ds.c:215-216` | `client.rs`（`CheckReply`） | 类型掩码 + 发布者端点 |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 借了必还 | 传输接线约（revoke 点） | `ds.c:33` |
| 串恒封尾 | 借前钉 / 收后钉 | `ds.c:84,155` |
| check 读请求栏 | `CheckReply` 类型 | `ds.c:215-216` |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`，本篇 5 个测试。

| 测试名 | 覆盖 C 位置 | 行为 |
|--------|-------------|------|
| `test_check_and_label_lend_roomy_writable_grant` | `ds.c:13-16` | 收回答：80 + 可写 |
| `test_other_calls_lend_strlen_plus_one_readable` | `ds.c:17-19` | 发名字：strlen+1 + 只读 |
| `test_string_lengths_carry_terminator` | `ds.c:79,153` | 串长恒 +1 |
| `test_terminate_pins_last_byte` | `ds.c:84,155` | 钉尾 + 空缓冲不钉 |
| `test_flag_assembly` | `ds.c:36-92` | 臂 + 修饰透传 |

---

## 6 过渡

客户端库讲完了，DS 全阶段闭环。收官总结见 `00-ds-overview.md`（待总览更新）与 `99-ds-global-concepts.md`（常量/错误码/跨服务引用收口）——两篇不在本次 01~12 范围内，列入后续工作（§7）。

## 7 参见

- C 源：`minix3/minix/lib/libsys/ds.c`（全文 219 行）、`minix3/minix/include/minix/ds.h:40-68`
- 阶段文档：`11-ds-getsysinfo.md`（上一站）、`02-ds-message-contract.md`（grant 规则的服务器侧）
- Rust 实现：`os/servers/ds/src/client.rs`
- 对端：`minix-sys`（A-8，传输接线点）
