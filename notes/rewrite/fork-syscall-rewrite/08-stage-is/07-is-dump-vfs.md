# 07-is-dump-vfs：VFS 转储域

> **源码**：`minix3/minix/servers/is/dmp_fs.c`（全 83 行）+
> `servers/vfs/fproc.h:16-114` + `servers/vfs/dmap.h:16-18` +
> `servers/vfs/const.h:19-25`（BLOCKED_ON）/`:34`（LABEL_MAX 16）+
> `com.h:60`（VFS=1）+ `dmap.h:82`（NR_DEVICES 135）+
> `sys/syslimits.h:38`（OPEN_MAX 255）
> **Rust**：`os/servers/is/src/dump_vfs.rs`（新）
> **draft 素材**：`draft/tmp_dmp_fs.c.md`（行文底料；Rust 块废弃同 01 §3.0）
> **位置**：DumpId Fproc/Dtab（03 表）；取数 SI 双表（04 §2.4/§6 行）

---

## 1. 概念：借书登记加书架分布图

> **目标读者**：读完 01~06 的读者。前置知识：04 SI 双表、06 游标体例。
> 本章不讲 VFS 服务器内部语义（`05-stage-vfs`）与通道机制（04）。

VFS 转储分两张表：fproc 篇是**借书登记**（谁开了哪些 fd、会话身份、
阻塞在哪）；dmap 篇是**书架分布图**（主设备号归哪个驱动）。前者翻页
（进程数以百计），后者一屏打完（135 设备多数空槽）——同文件两种分页
策略，是"按数据量选格式"的实例。

### 1.3 边界声明

**前置依赖**：04（SI_PROC_TAB/DMAP_TAB）+ 06（游标/跳过体例对照）。

**本篇职责**：dmp_fs.c 全部 + 双布局 ABI。

**不覆盖（移交）**：VFS 内部语义 → 05-stage-vfs；全布局权威 → VFS
crate；SDEV 端点缺口 → VFS crate（C TODO 原文继承）；dump 体执行 → A-6。

> **本章小结**：双表双策略（翻页 + 一屏）+ fd 计数 + 阻塞端点分支（§1.2）。
> 下一章 §2 逐行对应到 `dmp_fs.c`。

---

## 2. C 源码分析

### 2.1 fproc_dmp（:25-64）

取表失败 → `"Error obtaining table from VFS. Perhaps recompile IS?\n"`
（:30-33，06 §2.1 同款布局失配证言）→ 标题列头（:35-36）→ 循环
（`prev_i` 起，**无初值语句**——`static int prev_i;` 零初值，:29；
`pid <= 0` 跳过**无例外**，:43，对照 06 的 `==0+保留`）→ fd 计数内循环
（:45-47，`fp_filp[j] != NULL` 累加，OPEN_MAX=255）→ 行：序号/pid/
tty 主次号（`major`/`minor` 宏展开，显示转述，不建模设备层）/umask（`0x%05x`）/
uid 对/gid 对/会话位（`!!(flags & FP_SESLDR)`，:55）/nfds/blocked_on/
复活位（`!!(flags & FP_REVIVED)`，:56）→ 端点分支：CDEV 打端点（:60-61）
否则 `" nil"`（:63），附原文 TODO（SDEV 无端点，:62 注释）。

### 2.2 dtab_dmp（:67-83）

取 DMAP 表 → 双标题行（:73-75）→ 135 循环：`driver == NONE` 跳过（:78）
→ 行 `" %13s %5d %10d\n"`（label/主号/驱动端点，:79）。**无 prev_i、
无 MORE**——dmap 稀疏（多数槽 NONE），135 行内天然一屏。这是"22 行惯例"
的反例：分页看的是**有效行数**而非表长，稀疏表不需要游标（设计注记，
后篇 09 同例）。

### 2.3 FP 位语义

`FP_SESLDR 0004`（fproc.h:96）/`FP_REVIVED 0002`（:94，八进制字面——
`!!` 转 0/1 后按 `%3d` 打印）；`FP_BLOCKED_ON_*` 0-6（const.h:19-25：
NONE/PIPE/FLOCK/POPEN/SELECT/CDEV/SDEV）。

### 2.4 双布局 ABI（子集）

fproc 所用字段（fproc.h 行号）：pid:18/endpoint:19（存而不用——转储打
序号 `i` 不打端点，点名）/filp:24/tty:27/blocked_on:29/umask:69/
uids:63-65/flags:16/cdev:88（union 别名 `fp_u.u_cdev`）。dmap：
driver:17/label:18（LABEL_MAX 16，const.h:34）。`[ARCH: A-4]` 快照提案 +
VFS 对齐待办（§3 D1 三处之二）。

---

## 3. Rust 设计决策

### 3.1 D1：双快照（A-4）

`FProcSnap`（pid/tty/umask/uids/flags/blocked_on）+ `DmapSnap`
（driver/label[16]）。filp 表不进快照（255 指针无意义，计数另计 D2）。
否决全镜像/复用 VFS 活体类型。

### 3.2 D2：fd 计数切片纯函数

`count_fds(&[bool]) -> u32`（调用方传占用窗；OPEN_MAX_FD=255 常量供长
断言）。否决定长数组参数（调用方构造 255 元数组笨重；切片等价语义）。

### 3.3 D3：blocked_on 枚举 + SDEV 缺口继承

`BlockedOn` 七变体 + `decode`（未知值 None，前向兼容）+
`blocked_endpoint(Cdev → Some，余 None)`——SDEV 的 None 即 C TODO 的
显式继承（注释点名，非本篇债）。

### 3.4 D4：跳过差异点名 + 游标隔离

fproc `<=0` 无例外（06 对照）；`VfsCursor` 独立新类型（06 已 CONVERGED
不动；同构注释存证）；dtab 零游标（稀疏一屏）。

### 3.5 D5：体延后（A-6）+ 格式常量逐字

5 标题/列头/nil 常量；`run_dump` Fproc/Dtab 臂续空。

---

## 4. 实现详解

### 4.1 模块树（增量）

```text
os/servers/is/src/dump_vfs.rs — FProcSnap/DmapSnap/BlockedOn/count_fds/blocked_endpoint/fproc_skipped/dmap_skipped/VfsCursor/6 格式常量（D1-D5）
```

### 4.2 关键签名（与 §3 一致，Gate D-5 依据）

```rust
pub struct FProcSnap { fp_pid/fp_tty: i32, fp_umask/uids/gids: u32…, fp_flags: u32, fp_blocked_on: i32 }
pub struct DmapSnap { dmap_driver: i32, dmap_label: [u8;16] }
pub enum BlockedOn { None/Pipe/Flock/Popen/Select/Cdev/Sdev } + code/decode
pub fn count_fds(&[bool]) -> u32; pub const fn blocked_endpoint(BlockedOn, i32) -> Option<i32>;
pub const fn fproc_skipped(i32) -> bool; dmap_skipped(i32, i32) -> bool;
pub struct VfsCursor; push(pid, idx) -> bool; finish(bool); next() -> usize;
pub const OPEN_MAX_FD: usize = 255; FP_SESLDR/FP_REVIVED: u32;
```

常量权威位置（§2.4g）：本篇常量唯一定义于 `dump_vfs.rs`。

### 4.3 关键不变量

1. 跳过 `<=0` 无例外（06 对照）。
2. SDEV 端点恒 None（TODO 继承）。
3. dtab 零游标（稀疏一屏）。
4. 体执行待 A-6。

---

## 5. 测试要点

| # | 场景 | 期望 | C 依据 |
|---|---|---|---|
| T1 | 计数（空/部分）+ OPEN_MAX 值 | — | §2.1 |
| T2 | 跳过三值（0/-3/1） | — | §2.1 |
| T3 | FP 位八进制值 | — | §2.3 |
| T4 | 端点分支（CDEV/SDEV/PIPE）+ 解码未知 | — | §2.1 |
| T5 | 游标翻页 + 跳过不计数 + 回绕 | — | §2.1 |
| T6 | dtab 跳过 + 列宽逐字 | — | §2.2 |

### 5.3 测试统计（截至 2026-09-04）

- `cargo test -p minix-is`：**72 passed, 0 failed**（`dump_vfs` 新增 6）。
- `cargo clippy/check -p minix-is`：本 crate 零警告。
- 完整清单：`rg "#\[test\]" os/servers/is/src/dump_vfs.rs`。

---

## 6. 过渡

VFS 域闭环（双表双策略）。下一篇 08（`08-is-dump-rs.md`）取 RS 双表
（PUB + PRIV）+ RS_* 位语义——`SI_PROC_TAB` 同表异主陷阱（04 D4）的另一半
在此兑现（RS 那一路）。

---

## 7. 参见

- `04-is-data-acquisition.md` §2.4/§6 VFS 行：SI 双表通道
- `03-is-dump-dispatch.md`：DumpId Fproc/Dtab 归属本篇
- `06-is-dump-pm.md` §2.1/§3 D3：跳过/游标体例对照
- `05-stage-vfs/`：VFS 服务器内部语义
- `draft/tmp_dmp_fs.c.md`：行文底料
- plan §4（A-4/A-12）/§5.3（07 函数清单）
