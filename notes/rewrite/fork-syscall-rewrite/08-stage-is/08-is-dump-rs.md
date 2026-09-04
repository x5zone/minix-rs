# 08-is-dump-rs：RS 转储域

> **源码**：`minix3/minix/servers/is/dmp_rs.c`（全 74 行）+
> `servers/rs/type.h:63-79`（rproc 字段）+ `include/minix/rs.h:58`
> （LABEL 16）/`:165-184`（rprocpub）/`:194-196`（SF_USE_*）+
> `servers/rs/const.h:18`（CMD 512）/`:28-39`（RS_*）+ `com.h:61`（RS=2）
> **Rust**：`os/servers/is/src/dump_rs.rs`（新）
> **draft 素材**：`draft/tmp_dmp_rs.c.md`（行文底料；Rust 块废弃同 01 §3.0）
> **位置**：DumpId Rproc（03 表）；取数 SI 双表（04 §2.4/§6 行 + STDO 陷阱）

---

## 1. 概念：花名册加考核档案

> **目标读者**：读完 01~07 的读者。前置知识：04 STDO 陷阱、06/07 快照体例。
> 本章不讲 RS 服务器内部语义（`03-stage-rs`）与通道机制（04）。

RS 转储回答：每号服务"是谁"（PUB 表：label/端点/设备号）与"怎么样"
（PRIV 表：pid/心跳/重启次数/状态位）。身份与状态分开存放——PUB 可给
MIB 之类外部读者看，PRIV 只有 RS 自己全懂。IS 一次拉两张表，按槽位
对齐拼行（§2.1）。

### 1.3 边界声明

**前置依赖**：04（SI_PROCPUB_TAB/PROC_TAB + STDO）+ 03（Rproc 变体）。

**本篇职责**：dmp_rs.c 全部 + 双布局 ABI。

**不覆盖（移交）**：RS 内部语义 → 03-stage-rs；全布局权威 → RS crate；
dump 体执行 → A-6。

> **本章小结**：双表对齐拼行 + 双源编码 + IN_USE 过滤（§1.2）。
> 下一章 §2 逐行对应到 `dmp_rs.c`。

---

## 2. C 源码分析

### 2.1 rproc_dmp（:26-58）

双取短路或（:33-34 同行：任一失败即 `"Error obtaining table from RS.
Perhaps recompile IS?\n"` + return）→ 标题列头（:40-41）→ 循环
（`prev_i` 起，`NR_SYS_PROCS` 界）→ `RS_IN_USE` 过滤（:44）→ 九列行：
label/端点/pid/双源编码/dev/周期/alive_tm/重启数/命令串（:46-54，
`%5dx` 的 `x` 为字面后缀）→ 尾翻页（:55-57，`\r` 体例）。

**短路旧值风险**：`||` 短路下，若第一取成功、第二取失败，函数直接返回——
rproc 旧值无人读（直接返，无脏读）。反之第一取失败亦然。双取是"全有或
全无"，无半更新行（与"部分失败继续"模式不同，点名）。

### 2.2 s_flags_str 双源 6 位（:61-73）

"AUNCR" + NUL（A: RS_ACTIVE 0x800/U: RS_UPDATING 0x80/E: RS_EXITING 0x002/
N: RS_NOPINGREPLY 0x008/C: SF_USE_COPY 0x008/R: SF_USE_REPL 0x020）。
**双源同值陷阱**：`0x008` 在 r_flags 里是 N，在 sys_flags 里是 C——同值
异义，签名强制双参（§3 D2），合并即错。

### 2.3 双布局 ABI（子集）

rprocpub（rs.h:165-184）：取 sys_flags:167/endpoint:168/dev_nr:172/
label:177（LABEL 16，rs.h:58）。rproc（type.h）：r_pid:63/r_restarts:66/
r_flags:68/r_period:71/r_alive_tm:73。`r_args[512]`（:79，CMD 512，
const.h:18）**不进快照**（输出层直读源缓冲，§3 D1）。`[ARCH: A-4]` 快照
提案 + RS 对齐待办（三处之二）。

---

## 3. Rust 设计决策

### 3.1 D1：双快照（A-4）+ r_args 排除

`RprocpubSnap`（4 字段）+ `RprocSnap`（5 字段），C 序，`#[repr(C)]`，
待办 TODO。r_args 512B 不进快照（注释存证）。

### 3.2 D2：双源编码 const fn

`rs_flags_str(r_flags, sys_flags) -> [u8;7]`。否决合并单参（同值异义）。

### 3.3 D3：IN_USE 过滤谓词 + 体延后

`rproc_in_use` + 格式常量逐字；`run_dump` Rproc 臂续空（A-6）。

---

## 4. 实现详解

### 4.1 模块树（增量）

```text
os/servers/is/src/dump_rs.rs — 双快照/双源编码/过滤谓词/2 格式常量（D1-D3）
```

### 4.2 关键签名（与 §3 一致，Gate D-5 依据）

```rust
pub struct RprocpubSnap { sys_flags: u32, endpoint/dev_nr: i32, label: [u8;16] }
pub struct RprocSnap { r_pid/r_restarts: i32, r_flags: u32, r_period: i32, r_alive_tm: u32 }
pub const fn rproc_in_use(u32) -> bool;
pub const fn rs_flags_str(u32, u32) -> [u8;7];
pub const RPROC_TITLE/COLUMNS: &str;
```

常量权威位置（§2.4g）：本篇常量唯一定义于 `dump_rs.rs`。

### 4.3 关键不变量

1. 双取全有或全无（短路或）。
2. 双源永不合并（同值异义）。
3. r_args 不进快照。
4. 体执行待 A-6。

---

## 5. 测试要点

| # | 场景 | 期望 | C 依据 |
|---|---|---|---|
| T1 | 双源编码（含同值异义对） | — | §2.2 |
| T2 | 过滤谓词 | — | §2.1 |
| T3 | 快照零构造 | — | §2.3 |
| T4 | 格式逐字 | — | §2.1 |

### 5.3 测试统计（截至 2026-09-04）

- `cargo test -p minix-is`：**76 passed, 0 failed**（`dump_rs` 新增 4）。
- `cargo clippy/check -p minix-is`：本 crate 零警告。
- 完整清单：`rg "#\[test\]" os/servers/is/src/dump_rs.rs`。

---

## 6. 过渡

RS 域闭环（双表对齐 + STDO 另一半兑现）。下一篇 09（`09-is-dump-ds.md`）
取 DS 单表 + DSF 位语义 + 环形翻页游标——IS 作为 DS A-10 消费者契约的
另一半（DS 侧 `07-stage-ds` A-10，本篇消费）。

---

## 7. 参见

- `04-is-data-acquisition.md` §2.4/§6 RS 行：SI 双表 + STDO 陷阱
- `03-is-dump-dispatch.md`：DumpId Rproc 归属本篇
- `07-is-dump-vfs.md` §2.4：快照体例对照
- `03-stage-rs/`：RS 服务器内部语义
- `draft/tmp_dmp_rs.c.md`：行文底料
- plan §4（A-4/A-12）/§5.3（08 函数清单）
