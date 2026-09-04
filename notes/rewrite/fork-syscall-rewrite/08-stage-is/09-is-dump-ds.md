# 09-is-dump-ds：DS 转储域

> **源码**：`minix3/minix/servers/is/dmp_ds.c`（全 52 行）+
> `servers/ds/store.h:12-29`（NR_DS_KEYS/data_store）+
> `include/minix/ds.h:12-29`（DSF_*/DS_MAX_KEYLEN 80）+
> `sys_config.h:9`（NR_SYS_PROCS 64）+ `com.h:65`（DS=6）
> **Rust**：`os/servers/is/src/dump_ds.rs`（新）
> **draft 素材**：`draft/tmp_dmp_ds.c.md`（行文底料；Rust 块废弃同 01 §3.0）
> **位置**：DumpId DataStore（03 表）；取数 SI_DATA_STORE（04 §2.4/§6 行；
> DS A-10 消费者一半）

---

## 1. 概念：盘点仓库

> **目标读者**：读完 01~08 的读者。前置知识：04 STDO、07 稀疏对照。
> 本章不讲 DS 服务器内部语义（`07-stage-ds`）与通道机制（04）。

DS 是键值仓库：128 个货架（槽位），有用货（IN_USE）无货（空）参半。
转储是**盘点**：从上次记账签（prev_i）接着数，跳过空货架，有货抄四项
（槽号/键/主人/类型值），一页 22 行，数完一轮签回零。下轮从签处继续——
与 07 的 dmap（一屏打完）对照：稠密翻页 vs 稀疏一屏，分页看有效行数。

### 1.3 边界声明

**前置依赖**：04（SI_DATA_STORE）+ 07（分页对照）+ 08（同值异标体例）。

**本篇职责**：dmp_ds.c 全部 + 布局 ABI。

**不覆盖（移交）**：DS 内部语义 → 07-stage-ds（A-10 生产侧）；全布局
权威 → DS crate；dump 体执行 → A-6。

> **本章小结**：128 槽盘点 + 四类型行 + 条件界游标 + 早返重放（§1.2）。
> 下一章 §2 逐行对应到 `dmp_ds.c`。

---

## 2. C 源码分析

### 2.1 data_store_dmp 全读（:9-51）

取表失败 → 同款 `"Error obtaining table from DS. Perhaps recompile IS?\n"`
（:15-17，布局失配证言三见）→ 双标题（:20-21）→ 条件界循环
`for(i = prev_i; i < NR_DS_KEYS && n < LINES; i++)`（:22，**三游标第三形**：
05 先比较后断、06/07 候选计数断、本篇双条件界——对照表见 §2.3 末）→
IN_USE 跳过（:24-25）→ 四类型分支（:27-42：U32 打值/STR 打指针内容/
MEM 打长度/LABEL 打值；`default: return` 早返，:41-42）→ 尾 `n++`
（:45，**打印后计数**——跳过与早返都不计数）→ 到尾归零否则 `"--more--\r"`
（:47-50）。

**早返语义**：未知类型触发循环内 `return`（:42）时，函数尾的
`prev_i = i`（:50）执行不到，`prev_i` 保持旧值。下轮从同一位置重放——
若该槽类型持续未知，转储每次都在此停住（C 无坏槽跳过；Rust 同行为，
§3 D3。后果可由行号直接推导，非推测）。

### 2.2 四类型行格式

`%-6d` 槽号 + `%-25s` 键 + `%-15s` 主人 + `%-10s` 类型词 + 值列
（U32/LABEL `%12u`，STR `%12s` 打**指针内容**，MEM `%12zu` 打长度）。
STR 的指针是 IS 本地拷贝内的地址（取表已拷全表）——安全因拷贝（点名，
防"用户态解引用服务指针"误读）。LABEL 与 U32 同存 `u.u32`（08 同款
同值异标）。

### 2.3 游标三形状对照

| 篇 | 界形 | 计数点 | 断点续跑 |
|---|---|---|---|
| 05 PROCLOOP | `++pagelines >= 22` 先比较 | Emit 即计数 | oldrp=断点行 |
| 06/07 | `++n > 22` 候选断 | 打印 22 后第 23 候选断 | prev_i=断点下标 |
| 09 本篇 | `i < N && n < 22` 条件界 | 尾 `n++` | prev_i=出界下标/早返保持 |

三形同效（22 行当页），异构同心——各篇游标独立类型（体例延续）。

### 2.4 data_store 布局 ABI（子集）

flags:17/key[80]:18/owner:19/u.u32/u.mem{data,length}:21-28
（store.h）。DS_MAX_KEYLEN 80（ds.h:29）。NR_DS_KEYS=128（2×64）。
DSF：IN_USE 0x001/MASK 0xFF0/U32 0x010/STR 0x020/MEM 0x040/LABEL 0x100
（ds.h:12-22）。`[ARCH: A-4]` 快照提案 + DS 对齐待办（§3 D1 三处之二；
A-10 消费者姿态见 §3 D1）。

---

## 3. Rust 设计决策

### 3.1 D1：快照（A-4 + A-10 消费）

`DsEntrySnap`（flags/key[80]/owner[80]/scalar）——STR 指针与 MEM reallen
不进快照（输出层直读源缓冲；标量面 `u32_or_len` 足矣）。`[ARCH: A-4]`
三处 + A-10 消费者注记（生产侧 `07-stage-ds` A-10）。

### 3.2 D2：类型枚举 + 缺省中止

`DsValKind` 四变体 + `decode`（未知 None）+ `DsCursor::aborted` 早返语义
（保持 next 重放）。LABEL/u32 枚举区分（08 同款）。

### 3.3 D3：游标条件界（第三形）

`push(used) -> bool`（满 22 即停）+ `step_index`（i++）+ `finish
(exhausted/aborted)`（到尾归零/早返保持）。SKIP 不计数（continue 先于
n++ 的对应）。

### 3.4 D4：体延后（A-6）+ 格式常量逐字

双标题 + 四类型词；`run_dump` DataStore 臂续空。

---

## 4. 实现详解

### 4.1 模块树（增量）

```text
os/servers/is/src/dump_ds.rs — DsEntrySnap/DsValKind/decode/DsCursor/DSF_*/NR/格式常量（D1-D4）
```

### 4.2 关键签名（与 §3 一致，Gate D-5 依据）

```rust
pub const NR_DS_KEYS: usize = 128; DS_MAX_KEYLEN: usize = 80;
pub const DSF_IN_USE/MASK_TYPE: u32;
pub struct DsEntrySnap { flags: i32, key/owner: [u8;80], scalar: u32 }
pub enum DsValKind { U32, Str, Mem, Label } + decode(i32) -> Option + word() -> &'static str
pub const fn ds_skipped(i32) -> bool;
pub struct DsCursor; push(bool) -> bool; step_index(usize); finish(bool, bool); next/aborted();
```

常量权威位置（§2.4g）：本篇常量唯一定义于 `dump_ds.rs`。

### 4.3 关键不变量

1. 满 22 即停（条件界）。
2. 早返保持 next（重放）。
3. LABEL/u32 枚举区分。
4. 体执行待 A-6。

---

## 5. 测试要点

| # | 场景 | 期望 | C 依据 |
|---|---|---|---|
| T1 | 类型解码四值 + 未知 None + 常量值 | — | §2.4 |
| T2 | 跳过规则 | — | §2.1 |
| T3 | 游标 22/跳过/到尾/中止重放 | — | §2.1/§2.3 |
| T4 | 格式逐字 | — | §2.2 |

### 5.3 测试统计（截至 2026-09-04）

- `cargo test -p minix-is`：**80 passed, 0 failed**（`dump_ds` 新增 4）。
- `cargo clippy/check -p minix-is`：本 crate 零警告。
- 完整清单：`rg "#\[test\]" os/servers/is/src/dump_ds.rs`。

---

## 6. 过渡

DS 域闭环（三游标集齐：先比较/候选断/条件界）。下一篇 10
（`10-is-dump-vm.md`）取 VM 三通道 + 批状态机（首屏 header + 连续折叠 +
LINES 边界）——最复杂的游标（prev_i + prev_base 双游标）。

---

## 7. 参见

- `04-is-data-acquisition.md` §2.4/§2.5/§6 DS 行：SI_DATA_STORE 通道
- `03-is-dump-dispatch.md`：DumpId DataStore 归属本篇
- `07-is-dump-vfs.md` §2.2：稀疏一屏对照
- `07-stage-ds/`：DS 服务器 + A-10 生产侧
- `draft/tmp_dmp_ds.c.md`：行文底料
- plan §4（A-4）/§5.3（09 函数清单）
