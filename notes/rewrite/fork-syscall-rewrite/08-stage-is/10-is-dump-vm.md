# 10-is-dump-vm：VM 转储域

> **源码**：`minix3/minix/servers/is/dmp_vm.c`（全 157 行）+
> `include/minix/vm.h:40-71`（3 结构）+ `sys/sys/mman.h:62-65`
> （PROT 1/2/4）
> **Rust**：`os/servers/is/src/dump_vm.rs`（新）
> **draft 素材**：`draft/tmp_dmp_vm.c.md`（行文底料；Rust 块废弃同 01 §3.0）
> **位置**：DumpId Vm（03 表）；取数 PROCTAB + vm_info×3（04 §2.1/§2.5/§6 行）

---

## 1. 概念：抄写长卷

> **目标读者**：读完 01~09 的读者。前置知识：04 region 三元组、09 三游标。
> 本章不讲 VM 服务器内部语义（`02-stage-vm`）与通道机制（04）。

VM 转储面对的是**不定长**数据：进程数 × 每进程区间数，事先不知道多少行。
对策三件套：① 分摞取（region 游标一批 LINES 个）；② 相同段落记"同上×N"
（连续相同区间折叠）；③ 书签续抄（prev_i + prev_base 双游标）。
本篇是三游标里最复杂的一个（09 §2.3 对照表收官行）。

### 1.3 边界声明

**前置依赖**：04（vm_info 三通道 + region 三元组）+ 09（三游标对照）。

**本篇职责**：dmp_vm.c 全部 + 三布局 ABI。

**不覆盖（移交）**：VM 内部语义 → 02-stage-vm；全布局权威 → VM crate；
dump 体执行 → A-6。

> **本章小结**：分摞取 + 折叠 + 双游标 + 边界防御（§1.2 抄写长卷）。
> 下一章 §2 把折叠机、首屏、批循环逐行对应到 `dmp_vm.c`。

> **本章小结**：分摞取 + 折叠 + 双游标 + 边界防御（§1.2）。
> 下一章 §2 逐行对应到 `dmp_vm.c`。

---

## 2. C 源码分析

### 2.1 print_region：折叠状态机（:11-50）

三静态（`vri_count/vri_prev_set/vri_prev`，:13-14）+ 相邻判定四等
（prot/flags/length 相等 + `addr == prev.addr + prev.length`，:18-23）+
吸收计数（:25-27）+ 延迟打印（下个**不同**区间到来时先打
`"  (contiguously repeated %d more times)\n"`，:35-38——**数的是"多重复
几次"，不含首个**）+ NULL 双用（:29 清 `prev_set` + :35-38 若有挂起数则
`return` 前已打印 + :43 无区间则直接返）+ 区间行
`"  %08lx-%08lx %c%c%c (%lu kB)\n"`（:45-51，`length/1024L` 下取整）+
每次打印 `(*n)++`（:38/:51——含折叠行，分页器看得见折叠）。

**跨表残留**：静态量按进程不清零——上进程尾区间与下进程首区间比较，
不同则正常打印（首区间与"无"比永不等，由 `vri_prev_set` 守卫，:20）。
语义照录（§3 D2），不"修复"为每进程重置（改了反而漂移）。

### 2.2 vm_dmp 首屏（:59-80）

`prev_i == -1` 首轮：`vm_info_stats`（失败 warn + return，:65-69）→
总数行（`%lu kB` ×4，`pagesize/1024` 换算，:71-75）→ **两个空行各计一行**
（:76-79，`printf("\n"); n++;` ×2——空行占分页额度，点名）→ `prev_i++`
（-1→0，:80）。静态表 `proc[]`/`vri[LINES]`（:60-61，vri 批缓冲恰一屏）。

### 2.3 主循环：双游标批机（:83-155）

取 proctab（失败 warn + return，:83-86）→ `for (i = prev_i; i < N && n <
LINES; i++, prev_base = 0)`（:88，**每轮迭代尾清游标**——新进程从头取）→
跳过（task 下/空槽，:89）→ `first = (prev_base == 0)`（:92）→ 拉批
（`LINES - first` 个，:94）→ 负值分支（报错行 + `n++` + continue，:96-101）→
首屏分支（:103-122）：**容量预检** `n + 1 + r > LINES` 则 `prev_base = 0`
+ break（:105-107，**header 都不打**——整批让给下页）→ usage（失败报错行，
:110-113）→ header 行（total/common/shared，:117-122）→ `n++` →
内层续取循环（`while (r > 0)`：逐区间 `print_region`（:125-127）→
`LINES - n - 1 <= 0` 即停（:130）→ 续拉（:131-132，负值报错行 :135-137））→
`print_region(NULL)` 收尾（:140）→ `n > LINES` 内错分支（:142，
`"IS: internal error"`——**防御性**：预检保证到不了，不断言可达）→
`n == LINES` 即停（:143）→ 擦除行 `"        \n"`（8 空格，:146，
注释亲口承认"may have to wipe out the --more-- from below"——盖掉上屏残痕）→
`n++`（:147）。

### 2.4 游标语义（:150-155）

到尾（`i >= N`）→ `i = -1, prev_base = 0`（:150-152，下轮重进首屏）；
中断 → `"--more--\r"` + `prev_i = i`（:154-155，`prev_base` 保持——续取
上次没搬完的进程）。三游标集齐对照：

| 篇 | 形状 | 续跑键 |
|---|---|---|
| 05 | `>=` 先比较 | oldrp 行指针 |
| 06/07 | `>` 候选断 | prev_i 下标 |
| 09 | `&&` 条件界 | prev_i 下标（早返保持） |
| 10 本篇 | 双游标 + 容量预检 | prev_i 进程 + prev_base 区间基 |

### 2.5 三布局 ABI（子集）

stats（vm.h:40-46：pagesize/total/free/largest/cached）、usage（:48-52
取 total/common/shared）、region（:59-64 全）。PROT 1/2/4
（sys/mman.h:62-65，POSIX 值）。`[ARCH: A-4]` 快照提案 + VM 对齐待办
（§3 D1 三处之二）。

---

## 3. Rust 设计决策

### 3.1 D1：三快照（A-4）

`VmStatsSnap`（5）/`VmUsageSnap`（3）/`VmRegionSnap`（4，全——region
结构小，无子集必要）。C 序，`#[repr(C)]`，待办 TODO。

### 3.2 D2：折叠纯状态机

`FoldState{count, prev}` + `push(Option) -> FoldAction`（Buffered/
FlushRepeat/FlushRegion/FlushEnd）。NULL 收尾两用照录；跨表残留照录
（不清零）；FlushRepeat 后调用方重喂当前区间（单步语义契约，注释存证）。

### 3.3 D3：批机纯状态机（双游标）

`BatchCursor{prev_i, prev_base}` + `first_screen_done/is_first_batch/
fits_first_batch`（容量预检含 `prev_base = 0` 重置副作用——C 同构，
注释存证）。内错分支只留常量（不断言可达）；擦除行常量逐字。

### 3.4 D4：保护位 const fn + kB 截断注记

`prot_chars`（rwx）；`/1024L` 下取整注记（格式化层事，不单列函数）。

### 3.5 D5：体延后（A-6）+ 格式常量逐字

5 常量词；`run_dump` Vm 臂续空。

---

## 4. 实现详解

### 4.1 模块树（增量）

```text
os/servers/is/src/dump_vm.rs — 三快照/FoldState/FoldAction/BatchCursor/prot_chars/5 格式常量（D1-D5）
```

### 4.2 关键签名（与 §3 一致，Gate D-5 依据）

```rust
pub struct VmStatsSnap { vsi_pagesize: u32, vsi_total/free/largest/cached: u64 }
pub struct VmUsageSnap { vui_total/common/shared: u64 }
pub struct VmRegionSnap { vri_addr/length: u64, vri_prot/flags: i32 }
pub const PROT_READ/WRITE/EXEC: i32; pub const fn prot_chars(i32) -> [u8;3];
pub enum FoldAction { Buffered, FlushRepeat(u32), FlushRegion, FlushEnd }
pub struct FoldState; push(Option<&VmRegionSnap>) -> FoldAction;
pub const VM_LINES: u32 = 24;
pub struct BatchCursor { prev_i: i32, prev_base: u64 }; first_screen_done/is_first_batch/fits_first_batch;
```

常量权威位置（§2.4g）：本篇常量唯一定义于 `dump_vm.rs`。

### 4.3 关键不变量

1. 折叠四等 + 跨表不重置。
2. FlushRepeat 后重喂（单步契约）。
3. 容量预检重置 prev_base。
4. 内错分支不可达（防御常量）。
5. 体执行待 A-6。

---

## 5. 测试要点

| # | 场景 | 期望 | C 依据 |
|---|---|---|---|
| T1 | 折叠吸收/冲刷/重喂/跨表 | — | §2.1 |
| T2 | 断裂（间隔/变属性） | — | §2.1 |
| T3 | 容量预检重启 | — | §2.3 |
| T4 | 保护位 | — | §2.5 |
| T5 | 格式词逐字 + LINES=24 | — | §2.2/§2.3 |

### 5.3 测试统计（截至 2026-09-04）

- `cargo test -p minix-is`：**86 passed, 0 failed**（`dump_vm` 新增 6）。
- `cargo clippy/check -p minix-is`：本 crate 零警告。
- 完整清单：`rg "#\[test\]" os/servers/is/src/dump_vm.rs`。

---

## 6. 过渡：阶段完成

VM 域闭环。08-stage-is 文档计划（plan §6，00 跳过、99 延后）至此
**10 篇全 reviewed**：01 启动骨架 → 02 协议 → 03 分派 → 04 数据面 →
05~10 六转储域。遗留缺口（生产接线 + GET_KMESSAGES kernel 侧 + 六布局
对齐 + 输出通道 A-6 + `run_dump` 体）见各篇 scan；99 全局概念待后续。

---

## 7. 参见

- `04-is-data-acquisition.md` §2.1/§2.5/§6 VM 行：PROCTAB + vm_info 三通道
- `03-is-dump-dispatch.md`：DumpId Vm 归属本篇
- `09-is-dump-ds.md` §2.3：三游标对照
- `02-stage-vm/26-vm-queries.md`：VM_INFO 服务端
- `draft/tmp_dmp_vm.c.md`：行文底料
- plan §4（A-4）/§5.3（10 函数清单）/§6（路线收官）
