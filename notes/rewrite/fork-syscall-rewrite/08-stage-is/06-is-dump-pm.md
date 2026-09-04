# 06-is-dump-pm：PM 转储域

> **源码**：`minix3/minix/servers/is/dmp_pm.c`（全 109 行）+
> `servers/pm/mproc.h:28-99`（字段/11 标志）+ `timers.h:35` +
> `libsys/getticks.c:1-13` + `sys/sigtypes.h:61-62` + `com.h:59`（PM=0）
> **Rust**：`os/servers/is/src/dump_pm.rs`（新）
> **draft 素材**：`draft/tmp_dmp_pm.c.md`（行文底料；Rust 块废弃同 01 §3.0）
> **位置**：DumpId Mproc/Sigaction（03 表）；取数 SI_PROC_TAB（04 §2.4/§6 行）

---

## 1. 概念：点名册加闹钟登记本

> **目标读者**：读完 01~05 的读者。前置知识：04 SI 通道、05 分页体例。
> 本章不讲 PM 服务器内部语义（`04-stage-pm`）与通道机制（04）。

PM 表转储回答两组问题：mproc 篇——"都有谁、什么状态"（点名册）；
sigaction 篇——"信号意向与闹钟"（谁屏蔽了什么、谁的铃还有多久响）。
后者引入本阶段第一张**时钟面**：`getticks()` 当前时刻与 `tmr_exp_time`
未来时刻的差。注意：转储读的是**单调时刻的快照**，不是订阅——两次翻页
之间时刻在走，alarm 剩余每次重算（§2.2）。

### 1.3 边界声明

**前置依赖**：04（SI_PROC_TAB）+ 05（分页/编码体例）。

**本篇职责**：dmp_pm.c 全部 + mproc ABI + 时钟面 + `\r` 分页。

**不覆盖（移交）**：PM 内部语义 → 04-stage-pm；mproc 全布局权威 → PM
crate（本篇快照 A-4 提案）；dump 体执行 → A-6。

> **本章小结**：单表双转储 + 11 位编码 + 时钟面 + 回车分页（§1.2）。
> 下一章 §2 逐行对应到 `dmp_pm.c`。

> **本章小结**：单表双转储 + 11 位编码 + 时钟面 + 回车分页（§1.2）。
> 下一章 §2 逐行对应到 `dmp_pm.c`。

---

## 2. C 源码分析

### 2.1 mproc_dmp（:41-73）

取表失败 → `"Error obtaining table from PM. Perhaps recompile IS?\n"`
（:47-50）——**措辞注意**："Perhaps recompile IS?" 暗示失败主因是**布局
失配**（IS 按旧 `sizeof(mproc)` 取，新 PM 表对不上，服务侧 EINVAL），
不是网络抖动。这是 A-4 的 C 侧证言：布局即契约，失配即失败，本篇快照
提案正为此服务。成功后标题 + 列头（:52-53）→ 循环（`prev_i` 起，
`pid==0 && i!=PM_PROC_NR` 跳过——0 号槽按规则保留显示（PM 自身行；规则
本身即证据，不另断言 pid 恒定性），:56）→ `++n > 22` 断（:57，**23 行候选处断，当页 22 行**——与 05 的
`>=` 不同，§3 D3 点名）→ 行格式（名/nr/parent/tracer/pid/父 pid/procgrp/
uid 对/gid 对/nice/flags，:58-66；父 pid 经 `mproc[mp_parent].mp_pid`
二次查表）→ 尾：到尾归零否则 `"--more--\r"`（:69-71，**回车非换行**，
与 05 的 `\n` 不同——覆盖式翻页，注释存证）。

### 2.2 sigaction_dmp（:75-109）

同取表 + `uptime = getticks()`（:82-85）→ 标题列头（:86-87：ignore/
catch/block/pending/alarm 五列）→ 同循环 → 行：名/nr + 三位图
`%08x`（ignore/catch/sigmask，:90-92）+ pending（:93）+ alarm 列：
`ALARM_ON` 置位则 `%8lu(exp-uptime)`（:94-95）否则 7 空格横线
`"       -"`（:96）。

**时钟面**：`getticks()` 经 kerninfo `kclockinfo->uptime`
（libsys/getticks.c:8-13），附原文注记"We assume atomic 32-bit field
retrieval. TODO: 64-bit support."——**C 自己承认 64 位未竟**，Rust 侧
`u32` 回绕减与之同构（§3 D4），不超前解决 TODO（越俎代庖是 P1 设计越界）。

### 2.3 flags_str 11 位（:21-39）

"WZAETUFspxd"（WAITING 0x2/ZOMBIE 0x4/ALARM_ON 0x10/EXITING 0x20/
TRACE_STOPPED 0x80/SIGSUSPENDED 0x100/VFS_CALL 0x400/PROC_STOPPED 0x8/
PRIV_PROC 0x2000/PARTIAL_EXEC 0x4000/DELAY_CALL 0x20000，mproc.h:87-102），
`static char str[12]`。注意 `'s'`（PROC_STOPPED）与 `'S'`（ SIG？无——
本表无 S，大写 S 空缺，05 的 RTS 表才有 S）——跨表字母不互通，各表独立
编码（A-12 按表落地，不统一）。

### 2.4 mproc 布局 ABI（子集）

所用字段（mproc.h 行号）：pid:28/parent:33/tracer:34/procgrp:30/
name:80/realuid:41/effuid:42/realgid:44/effgid:45/nice:75/flags:66/
ignore:53/catch:54/sigmask:55/sigpending:57（各 `__bits[0]`，sigset 为
4×u32，sigtypes.h:61-62——**只转储首字**，如实记录）/timer:62
（tmr_exp_time，timers.h:35）。`[ARCH: A-4]` 快照提案 + PM 对齐待办
（§3 D1 三处之二）。

### 2.5 排除

无（109 行全覆盖；`#if` 块无；死代码无）。

---

## 3. Rust 设计决策

### 3.1 D1：MProcSnap 子集（A-4）

16 字段（§4.2），C 声明序，`#[repr(C)]`，待办 TODO。否决全镜像/复用 PM
活体类型（另 crate 未定）。

### 3.2 D2：11 位编码 const fn

`pm_flags_str(u32) -> [u8;12]`。否决 String。

### 3.3 D3：PmCursor（跳过 + 22 界 + 回绕）

`push(pid, idx)`：跳过规则 + `n>22` More（05 对照：本篇 22 行当页）；
`finish(exhausted)`：到尾归零/`--more--\r` 续跑；两 dump 各持一实例
（各 static prev_i，:45/:79）。`MORE_CR` 常量逐字（`\r`）。

### 3.4 D4：alarm 回绕减（不超前）

`alarm_left(on, exp, uptime) -> Option<u32>`（wrapping 减；None 表横线）。
C 的 64 位 TODO 原样继承，不解决（注释存证）。

### 3.5 D5：体延后（A-6）+ 格式常量逐字

4 标题/列头常量；`run_dump` Mproc/Sigaction 臂续空。

---

## 4. 实现详解

### 4.1 模块树（增量）

```text
os/servers/is/src/dump_pm.rs — MProcSnap/pm_flags_str/PmCursor/PmAction/alarm_left/5 格式常量（D1-D5）
```

### 4.2 关键签名（与 §3 一致，Gate D-5 依据）

```rust
pub struct MProcSnap { mp_pid/parent/tracer: i32, mp_name: [u8;16], mp_procgrp: i32, mp_realuid/effuid/realgid/effgid: u32, mp_nice: i32, mp_flags: u32, mp_ignore0/catch0/sigmask0/sigpending0: u32, mp_timer_exp: u32 }
pub const fn pm_flags_str(u32) -> [u8;12];
pub enum PmAction { Skip, Emit, More }
pub struct PmCursor; push(pid: i32, idx: usize) -> PmAction; finish(exhausted: bool); next() -> usize;
pub const fn alarm_left(bool, u32, u32) -> Option<u32>;
pub const MPROC_TITLE/COLUMNS/SIGACTION_TITLE/COLUMNS/MORE_CR: &str;
```

常量权威位置（§2.4g）：本篇常量唯一定义于 `dump_pm.rs`（05 体例延续，
各篇独立）。

### 4.3 关键不变量

1. 跳过规则含 PM 自身保留（`pid==0 && idx!=0`）。
2. 22 行当页（`>22` 断，非 `>=`）。
3. `\r` 分页（非 `\n`）。
4. alarm 未置位恒横线；回绕减。
5. 体执行待 A-6。

---

## 5. 测试要点

| # | 场景 | 期望 | C 依据 |
|---|---|---|---|
| T1 | 11 位零/全/单比特 | — | §2.3 |
| T2 | 跳过矩阵（0+0 留 / 0+5 跳 / 非零过） | — | §2.1 |
| T3 | 22Emit+More 界 + 续跑位 | — | §2.1 |
| T4 | finish 到尾归零/中断续跑 | — | §2.1 |
| T5 | alarm 差/横线/回绕 | — | §2.2 |
| T6 | 格式逐字（含 `\r`） | — | §2.1/§2.2 |

### 5.3 测试统计（截至 2026-09-04）

- `cargo test -p minix-is`：**66 passed, 0 failed**（`dump_pm` 新增 7）。
- `cargo clippy/check -p minix-is`：本 crate 零警告。
- 完整清单：`rg "#\[test\]" os/servers/is/src/dump_pm.rs`。

---

## 6. 过渡

PM 域闭环（单表双转储 + 时钟面）。下一篇 07（`07-is-dump-vfs.md`）取 VFS
双表（PROC + DMAP）+ fd 计数 + FP 位语义——首个"一服务两布局"的转储。

---

## 7. 参见

- `04-is-data-acquisition.md` §2.4/§6 PM 行：SI_PROC_TAB 通道
- `03-is-dump-dispatch.md`：DumpId Mproc/Sigaction 归属本篇
- `05-is-dump-kernel.md` §2.1/§3 D3：PROCLOOP 体例对照（`>=` vs `>`）
- `04-stage-pm/`：PM 服务器内部语义
- `draft/tmp_dmp_pm.c.md`：行文底料
- plan §4（A-4/A-12）/§5.3（06 函数清单）
