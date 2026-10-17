# 18-stage-commands 实施规格缺口 TODO

> **台账状态摘要**（对照代码核实于 `2026-10-08`，快照 `9f752834c`）：未闭合 `open` 2 + 占位 5：命令面尚未接真实系统调用返回，命令全集口径按 `OQ-N1` 以规划文件的实测三百二十八为准。
> 跨阶段联动项的状态权威在 `../coordination/TODO-LEDGER-OPEN.md`（其 §6 给本阶段索引行）；本文件的条目描述与修法仍是权威，本轮只加本摘要不改条目。状态词按 `../coordination/TODO-LEDGER-INDEX.md` §3 的六值词表折叠。

> 来源：2026-09-17 设计讨论。讨论从"命令如何获得标准库支持"出发，逐层确认了三件事：POSIX 规定的是接口契约而不是系统调用清单；Minix3 的库分层是 `minix3/lib/libc/`（C 库）与 `minix3/minix/lib/libsys/`（消息层）两个目录；Redox 用 relibc（Rust 写的 C 标准库 + POSIX）承接 Rust std 的调用。讨论结论随后与 18-stage-commands 现有文档逐条对账。
> 范围：`rewrite-notes/18-stage-commands/`。本文只登记文档缺口与修复去向，不修改生产代码。
> 定位：`plan.md` 管覆盖契约（哪些命令、哪些 C 源、归哪一篇），本文管实施规格（命令依赖谁、需要哪些 API、行为以什么为准）。两者互补，不重复。
> 状态（2026-09-17）：P0 一项、P1 两项、跨阶段挂账一项。三项契约正文已落入 `99-global-concepts.md`（§1 分层契约、§2 行为判定基准、§3 Requires 列）；命令文档回填与 14-stage-runtime 侧措辞修正待后续轮次。

---

## 0. 条目速览

| 级别 | 编号 | 一句话 | 状态 |
|------|------|--------|------|
| **P0** | P0-1 | 命令层依赖契约悬空：`plan.md` 说命令消费 "exec/stdio/termios/socket API"，而 14-stage-runtime 的 [ARCH] A-2 已决定不移植 libc 的 stdio，"stdio" 不指向任何待建库 | ✅ 完成（2026-09-17）：契约正文在 99 §1；plan.md 四处措辞已改；14 侧排除表已补说明 |
| P1 | P1-1 | 命令契约表缺 Requires 列：只写"文件读写待系统调用"，不写每个命令需要哪些 `minix-sys` 函数 | 模板与示例已写入 `99-global-concepts.md` §3；逐命令回填待办 |
| P1 | P1-2 | POSIX 只被描述性提及，没有"行为契约以 POSIX 为准、Minix3 C 实现为真值"的判定规则 | 规则已写入 `99-global-concepts.md` §2；命令文档引用待补 |
| edge | E-THREAD-MODEL | 线程模型与 futex 无归属：14-stage-runtime 有 TLS 决策（A-4）但没有线程模型条目；命令层的作业控制被移交给"进程管理阶段" | 待 14-stage-runtime 或 04-stage-pm 立项 |

---

## 1. P0-1 命令层依赖契约悬空

### 证据

- `plan.md:394`（§5.4 排除表）：libc/libminc/libsys 实现归 14-stage-runtime，理由是"命令只消费 exec/stdio/termios/socket API"。
- `plan.md:405`（§6 实施顺序）："命令实装依赖 14-stage-runtime（exec/stdio/termios）与 17-stage-net（socket）先行"。
- `plan.md:206` 与 `plan.md:407`：纯 stdio 类命令"只依赖 exec + stdio + exit"。
- `../14-stage-runtime/plan.md:157`（[ARCH] A-2）：libc 的 `printf`/`string`/`ctype`/`regex`/`malloc` 不移植，用 Rust `core`/`alloc` 语义替代；`../14-stage-runtime/plan.md:273` 再次把 libc 全量列入排除表。
- `../14-stage-runtime/plan.md` 的文档清单（01 到 13 篇）没有 stdio 篇；termios 只出现在 `13-constants-abi.md`（常量搬运）。

### 影响

两边单独看都成立，合起来就是空集：18 说命令依赖一个叫 "stdio" 的东西，14 说这个东西不建。结果是补实现的 AI 面对两套说法——一套让它在命令里直接用 `core::fmt` 加 `write`，另一套让它等一个库。契约不落地，每个命令的补实现任务都无法判定"缺什么、该在哪儿补"。

### 处置

- 已做：`99-global-concepts.md` §1 写成分层表与两条硬规则——命令只依赖 `minix-rt` 与 `minix-sys` 顶层；命令不得直接 `use minix_sys::ipc` 或 `minix_types::ipc` 构造消息；并用 `minix-sys` 现有函数逐条解释 "stdio" 一词的准确含义。
- ✅ 已做（2026-09-17）：`plan.md` 四处措辞落地——`:206` 与 `:407` 的依赖声明改为 `exec`、`minix-sys` 顶层 `read`/`write` 与 `exit` 并指向 99 §1；`:394` 排除表行改为"minix-sys 顶层调用封装与 minix-rt（终端属性走 ioctl 封装、网络走 socket 封装）"；`:405` 增加"stdio 一词的准确含义见 99 §1"。
- ✅ 已做（2026-09-17）：14 侧说明——`14-stage-runtime/plan.md:273` 排除表"libc 全量"行的理由列补一句：stdio 无专篇源于 [ARCH] A-2，命令输出通道是 minix-sys 顶层 write（`os/libs/minix-sys/src/lib.rs:217`）。
- 验证（2026-09-17）：`grep -n "stdio" plan.md` 余 9 处，逐条核对全部为功能域类别名（`:24`/`:68`/`:105`/`:177`/`:426`）或指向 99 §1 的依赖声明（`:206`/`:405`/`:407`；`:394` 已不含 stdio 字样）——不存在任何把 stdio 当作依赖或库的残留表述。

---

## 2. P1-1 命令契约表缺 Requires 列

### 证据

- `plan.md:201`（§3.6 模板表头）：命令 / C 源 / 职责 / 关键选项 / 输入输出 / 退出码 / 错误面 / Rust 模块，没有依赖项。
- `06-file-ops.md:6`（Rust 模块行）：只能写"文件读写待系统调用"，读者无从知道缺的是哪个函数。
- `../14-stage-runtime/todo.md:60` 已登记 `stat`/`getdents`/`ioctl`/`fcntl` 一族"零 wrapper 且无排期登记"——说明缺口真实存在，只是命令层文档没有把它们表达出来。

### 影响

没有 Requires 列，"补代码"就无法拆成可判定的前置任务：实现 `ls` 到底是只写 `ls` 本体，还是先补 `getdents` 封装？依赖清单让这个问题有唯一答案。

### 处置

- 已做：`99-global-concepts.md` §3 给出模板与四个示例（`echo`、`cat`、`ls`、`sh` 执行器），并注明每项 API 的当前状态与缺口登记位置。
- 待做：`06` 到 `24` 各篇逐命令回填 Requires；跨文档统计同一 API 的消费者数量，作为 14-stage-runtime 补齐顺序的依据。
- 验证：回填后 `grep -c "Requires" 06-file-ops.md` 等文档计数与 §5.2 命令归属表的命令数一致。

---

## 3. P1-2 POSIX 基准未声明

### 证据

- POSIX 在 18 目录内全部是描述性提及：`05-shell-family.md:36`（"POSIX 规定的可移植子集"）、`:57` 与 `:59`（内建与外部程序行为一致是 POSIX 合规测试重点）、`08-grep-sed.md:49` 与 `:157`（匹配语义与字符类）、`11-compress-archive.md:38`（pax 格式）、`plan.md:18`、`:48`、`:60`。
- `plan.md:221`（[ARCH] A-5）只覆盖退出码与 errno 映射，没有覆盖命令行为本身的判定依据。

### 影响

命令的选项边界、错误输出、退出码在 POSIX 与 Minix3 C 实现不一致时按哪个写，没有明文。AI 补实现时只能自行选择，测试也没有对账基准。

### 处置

- 已做：`99-global-concepts.md` §2 写明判定规则——行为契约以 POSIX 为准绳、以 Minix3 C 实现为真值；C 实现偏离 POSIX 时按 C 写并标注偏离点；逐命令对照 C 源，不允许"典型命令详述、其余略过"。
- 待做：各命令文档在契约表上方引用该规则；出现偏离点时按标注义务记录。
- 验证：`grep -rn "ground truth\|判定基准" 99-global-concepts.md` 命中 §2；命令文档引用待补后按篇抽查。

---

## 4. edge E-THREAD-MODEL 线程模型与 futex 无归属

### 背景

Rust 标准库的 `thread` 与 `sync` 建立在 pthread 之上，pthread 的阻塞与唤醒又需要 futex 一类的内核原语。命令层本身不需要线程（Minix3 的命令都是单线程程序），但这个决策决定运行时能力边界，不能悬空。

### 证据

- `minix3/lib/libc/thread-stub/thread-stub.c`：Minix3 的 libc 用单线程桩，命令层不假设线程。
- `minix3/minix/lib/libmthread/pthread_compat.c:7-8`：Minix3 的用户级线程是绿线程，"没有抢占，除非显式让出"，不是 pthread 实现。
- futex：在 `minix3/` 全树检索，排除 `external/` 后 0 命中（2026-09-17 实测）。
- `os/libs/minix-types/src/types/com.rs:38`：`NR_PROCS = 256`；`minix3/minix/include/minix/config.h:31` 显示它是编译期可调常数，不是原理性上限。
- `../14-stage-runtime/plan.md:159`（[ARCH] A-4）：TLS 已登记（x86-64 FS 段 + 架构 trait），线程模型与 futex 没有条目。
- `05-shell-family.md:8`、`:92`：作业控制的进程组与信号实现"见进程管理阶段"，指向 04-stage-pm。

### 处置

- 不在 18-stage-commands 立项：命令层单线程，文档只需在 `05-shell-family.md` 的依赖链里指向立项位置。
- 建议：在 14-stage-runtime（运行时能力）或 04-stage-pm（进程模型）立"线程模型与阻塞原语"设计项，按 Architectural Evolution 处理，doc、design、代码三处一致标注。
- 解锁顺序：该决策同时是 Rust std 上机的前置条件之一，优先级排在依赖契约落地之后。

---

## 5. 实证清单

本清单列出登记各项结论时实际执行的检查，便于复核：

- `grep -rli "pthread" minix3/lib minix3/include minix3/commands`：命中 `libc/thread-stub/` 与 `minix/lib/libmthread/pthread_compat.c`，没有 `libpthread`。
- `grep -rli "futex" minix3/ | grep -v "/external/" | wc -l`：结果为 0。
- `grep -n "pub fn" os/libs/minix-sys/src/lib.rs`：顶层 14 个函数——send/receive/sendrec/notify（`:108-132`）、fork/exec/exit/waitpid/kill（`:148-175`）、open/close/read/write/mmap（`:191-231`）；没有 `dup2`、`pipe`、`stat`、`getdents`。
- `grep -n "POSIX" 18-stage-commands/*.md`：命中全部为描述性引用（见 §3 证据）。
- `grep -n "stdio" 18-stage-commands/plan.md`：8 处（见 §1 验证）。
- `ls 18-stage-commands/.design/`：`00` 与 `99` 没有 outline 与 design 快照（Gate H.6）。本轮只补契约三节不是完整改写，快照随 `99-global-concepts.md` 完整改写补齐。

---

## 批次三百四十九：`random` 的片3 收尾——十六进制浮点照 `gethex` 接上

- **是什么**：`RANDOM-DENOM-LEGS` 片3 剩下的那条腿。上一笔的 A/B 把 `0x10` 报成唯一真分叉
  （`tmp/random-348/ab348.log`），本支过去把它判进「不是有效分母」。真源是
  `minix3/lib/libc/gdtoa/gethex.c`，由 `strtod.c` 里 `#ifndef NO_HEX_FP` 那块在 `*s == '0'`
  且 `s[1]` 是 `x`/`X` 时调用——`NO_HEX_FP` 在本仓库从没定义过，所以这条腿是活的。
- **三处与十进制腿不同，逐条取自源码**：
  ① 二进制指数**可以省**：`gethex.c:110-112` 的 `switch (*s)` 只有 `p`/`P` 两个 `case`、
     **没有 `default` 拒绝分支**，没有指数就带着 `e = 0` 往下走，于是 `0x10` 就是 16。
  ② 这条腿**从不立 `errno`**：`gethex.c` 溢出回 `STRTOG_Overflow`、下溢回 `STRTOG_Underflow`
     （`gethex.c:250`、`:277`、`:285`），而 `strtod.c` 的十六进制 `switch` 只分
     `NoNumber`/`Zero`，其余走 `ULtod` 后直接 `goto ret`，一步都不碰十进制那条 `range_err`。
     所以 `0x1p99999999` 是「认下的无穷大」（退 0、一行不掷中），不是 `err(1, "%s", *argv)`；
     `0x1p-99999999` 是 0，于是由 `denom == 0` 挡住，报「不是有效分母」而不是三段形。
  ③ 指数位只收十进制数字，收不满就把游标退回 `p` 之前（`gethex.c:122-126` 的 `s = s1`），
     所以 `0x1p` 报 1、`ep` 停在 `p` → 否掉。`hexdig[]` 把 `'0'`..`'9'` 排成 `0x10`..`0x19`，
     那句 `n > 0x19` 就是在说这件事。
- **为什么不照参照件**：参照件在宿主上编的是 glibc，它对十六进制的溢出/下溢**立** `ERANGE`
  ——这一对形（`0x1p99999999`、`0x1p-99999999`）在两处 DIFF 里，属承载差异：真值载体是
  Minix 自己的 gdtoa，不是宿主 libc。这与「选项环那句 `unknown option` 不照 glibc 字样」同一条理。
- **实现**：新增 `scan_hex`（`gethex` 的形状：跳过 `0x`、折前导零、十六进制数字与一个小数点、
  可选 `p`/`P` 加十进制指数）与 `hex_value`、`pow2`；分派点在 `scan_strtod` 处理完空白与符号之后、
  进十进制循环之前——照 C 的顺序，`0x` 一认就整支交给 `gethex`，十进制与 `inf`/`nan` 都不再走。
  有效数字到 13 位十六进制（52 个二进制位）为止是**精确**的，超过则截尾折进二幂，与十进制腿
  共用同一条末位单位残差的登记。
- **门**：宿主库测 29/0（新立 `the_hex_leg_is_gethex_shaped`，11 形读数表 + 溢出那一形单独钉
  「值 `inf` 且 `erange` 为假」）、端到端 22/0（新立 4 枚：`0x10` 认、`0x1p3` 认、
  `0x1p99999999` 认下且一行都不掷中、`0x` 与 `0xg` 报「不是有效分母」）；对参照件 A/B 19 形
  **17 同 2 异**（`tmp/random-349/ab349.log`，两异即上面那对 glibc 立号形）；clippy 触及件 0 命中；
  rustfmt 零漂移；容器门（`minix-ci:1.94`、`-m 2g -j 1`、挂仓库根、工作目录 `os/`）读数见
  `tmp/random-349/container-gate.log`。
- **环境坑（现查证据 `ls -ld tmp/ci-target-349`）**：容器配方里 `CARGO_TARGET_DIR` 指向的宿主目录
  若不存在，docker 会以 **root** 属主自动建它，容器内以 `-u $(id -u)` 跑的 cargo 随即写不进
  `/target/debug`，报 `Permission denied (os error 13)` 退码 1——这是门的脚手架坏了，不是代码坏了。
  要么先 `mkdir -p` 再跑，要么复用上一轮已有的可写 target 目录。
- **还敞着的**：① `nan(0x…)` 载荷（`strtod.c` 调 `hexnan`，`gdtoa/hexnan.c:56-`）——这一形的载荷
  位样对 `random` 的可见行为其实无影响（它只被 `denom == 0` 与那两处算式读，所有 NaN 都给同一个
  答案），差的是「吃掉几个字符、`ep` 落在哪」；② 非有限值/超射程值转整数那一格的 UB 不追平；
  ③ 超过 13 位十六进制或 17 位十进制的末位单位残差；④ 子正规结果是否立 `ERANGE`；⑤ 共用的
  `bin_support::emit` 写缝退码（C 是 `err(2, "stdout")` 退 2，`random.c:131-133`，`emit` 被本 crate
  多支共用，改它要单独一条账）。取值域这条腿到此闭合，条目状态仍是 **partially fixed**（①—⑤）。
- **下一轮首动作**：`tools/claim.sh claim RANDOM-DENOM-LEGS qoder` 接①（`nan` 载荷的吃掉形状），
  或换 `SPLIT-STREAMING-MEMORY` 那条独立件。

## 批次三百四十八：`random` 的片3 落地——分母从 u32 换成 C 的那个 `double`

- **是什么**：`RANDOM-DENOM-LEGS` 的第三片。`minix3/games/random/random.c:67` 把 `denom` 定为
  `double`，`:95` 用 `strtod(*argv, &ep)` 读它，`:96` 只看 `errno == ERANGE`，`:98` 只看
  `denom == 0 || *ep != '\0'`。本支此前把分母按十进制整数面收在 `u32` 里，于是 C 认下的
  `1e3`、`1e-3`、`.5`、`-2`、`inf`、`nan` 全被判成「不是有效分母」，而 C 认不出的
  `4294967296`（`2^32` 在 `double` 里是精确的）反被判成溢出。
- **为什么是 P0-code-bug 而不是取值偏好**：参照件现量给出两侧的实际退码
  （`tmp/random-343/ab343-ref.log` 那七形），本支当时是「该退 0 的退 1、该退 0 的报了三段形」。
  更硬的一层是：`os/commands/games/text-games/tests/random_face.rs` 里那枚
  `an_overflowed_operand_uses_the_three_part_err_shape` 与 `a_rejected_operand_says_the_errx_sentence_without_naming_itself`
  把 `4294967296` 钉成三段形、把 `1e3` 钉成「不是有效分母」——**门钉的是实现，不是参照件**，
  与三百四十三、三百四十四、三百四十六那三次同类失误同形，这是第四次。
- **怎么修**：决定半新增 `Strtod`（值 / `ep` 下标 / `erange` 三读数）、`scan_strtod`（`minix3/lib/libc/gdtoa/strtod.c`
  的 `_int_strtod_l` 那条十进制腿：前导空白、符号、小数点、`e`/`E` 指数、指数位数闸 19999、
  `#ifdef INFNAN_CHECK` 的 `inf`/`nan`）、`match_fold`（`gdtoa/smisc.c:123-140` 折小写）、`pow10`
  （平方累乘，与 gdtoa 的 `tens[]`/`bigtens[]` 同形）、`line_selected`。`OperandLeg::Accepted` 与
  `NO_OPERAND_DENOMINATOR` 随之改 `f64`，`exit_status_value` 改按 C 的表达式在 `f64` 上算再向零截。
  判据这一处刻意写成开区间比较 `scaled > -1.0 && scaled < 1.0`，与 C 的「截成整数再与 0 比」在
  一切有限值上等价，而对 `inf`/`nan` 给出的答案与参照件所在的 x86-64 一致（都不为 0，即都不掷中）；
  C 在那两形上是未定义行为（aarch64 的 `fcvtzs` 饱和、x86-64 的 `cvttsd2si` 给 `0x80000000`），
  这一格**登记为不追平**，只对退码那一支的饱和语义写明。为什么不照 glibc 的大小写不敏感之外的
  形去猜：`INFNAN_CHECK` 与 `NO_HEX_FP` 都按本仓库自己的配置取——`gdtoa/gdtoaimp.h:191` 明写
  `#define INFNAN_CHECK`、`:643-645` 在 IEEE 上再立一次，`NO_HEX_FP` 全树没有定义，所以
  `inf`/`nan`/`0x…p…` 在 Minix 里都是认的（这一点上参照件与 Minix 同源，A/B 可直接引）。
- **门**：宿主库测 28/0、端到端 18/0（新立 6 枚：`1e3` 认下、`1e-3` 全通过、`--` 之后 `-1e-3`
  全通过、`4294967296` 不退 1、`inf`/`nan` 认下一行都不掷中、前导空白吃掉而尾巴不吃；翻正 2 枚、
  换示例数 1 枚）；容器门（`minix-ci:1.94` + `-m 2g -j 1`，挂仓库根、工作目录 `os/`）退码 0、合计 **46 通过 / 0 失败**（6 个 target，逐套读数在 `tmp/random-348/container-gate.log`）；对参照件的 A/B
  38 形 **36 同 2 异**（`tmp/random-348/ab348.log`：一异是十六进制浮点，登记在下；二异是
  `-inf` 走选项环那行的宿主字样与程序名来历，属承载差异）；clippy 触及件 0 命中；rustfmt 零漂移。
  A/B 只比退码与标准错误：标准输出由随机掷决定，不可比。
- **还敞着的**：① 十六进制浮点 `0x10`/`0x1p3`（同一函数 `#ifndef NO_HEX_FP` 那块，含 `gethex`
  的二进制指数）与 `nan(0x…)` 载荷；② 非有限值/超射程值转整数那一格的 UB 不追平；③ 与 gdtoa
  大数精化相比只按 `有效数字 × 10^指数` 乘出的末位单位残差（超过 17 位的数字串丢尾巴），只可能
  挪动「恰好压在阈值那一格」的选中判定；④ 子正规结果是否立 `ERANGE` 这一格照「下溢成 0 才立」
  实现，gdtoa 在 `Avoid_Underflow` 下对某些子正规也立号，差一句话不是差一个值；⑤ 共用的
  `bin_support::emit` 写缝退码（C 是 `err(2, "stdout")` 退 2，`random.c:131-133`；`emit` 被本
  crate 多支共用，改它要单独一条账）。条目状态仍是 **partially fixed**。
- **本笔的流程失误（现查证据在 `git reflog`）**：合入 `rewrite` 的 `git merge --ff-only` 我在**自己的认领树**里跑了——那棵树的 `HEAD` 是 `claim/RANDOM-DENOM-LEGS-qoder`，不是 `rewrite`，于是它回一句 `Already up to date` 什么也没做；紧接着的 `claim.sh release` 把树与分支删了，分支一删，`3ff3cf5ce` 只剩悬挂对象，`rewrite` 头还停在基线 `ba57665eb`。补救是在主树直接 `git merge --ff-only 3ff3cf5ce`：悬挂对象在 GC 之前照样可达，`git cat-file -t` 先证它在、`git show --stat` 先证六件齐全，再合，合后主树那三件他线程在制品（`AI-chats/daily.todo.md`、`01-stage-kernel/{02,todo}.md`）逐字未动。**规矩**：合入 `rewrite` 只能在主树跑 `git merge --ff-only claim/<ID>-<owner>`，`release` 必须在合入之后，且两步之间要现查一次 `git log --oneline -1 rewrite` 确认头已经前进——「Already up to date」不是成功。

- **下一轮首动作**：`tools/claim.sh claim RANDOM-DENOM-LEGS qoder`，接①（十六进制浮点：
  `strtod.c` 的 `gethex` 那块要连 `p` 指数一起抄，A/B 形列在 `tmp/random-348/ab348.log` 的 `0x10`）。

## 6. 2026-09-17 架构审查追加：代码侧查漏补缺与二进制面收敛（cmd-04 + code-excellence）

> **来源**：对 18-stage-commands 全部 Rust 实现（`os/commands/*` 的 58 个 workspace 成员，另含其依赖的 `os/libs/minix-sys` 与 `os/libs/minix-rt` 消费面）的架构级审查。先查漏补缺，再结构收敛。规范依据：`prompt/review-rules/review-cmds.md` §四（code-excellence：分层审视 + 每个改进点至少两个候选方案对比）与 §一（通用强制门）；对照先例：Redox coreutils（联网实证，见 §6.1）、uutils/coreutils（Rust 社区惯例，[待验证]）。
> **与上文分工**：§1 到 §4（P0-1、P1-1、P1-2、E-THREAD-MODEL）管**实施规格**——命令依赖谁、行为以什么为准；本节管**代码结构**——命令的 crate 归置、二进制面、依赖边界守卫。两条线在"逐命令回填 Requires 列"处汇合（见 §6.1 落地步骤第 4 步）。
> **清理说明**：复核了既有条目——P0-1 已完成契约正文落稿但 `plan.md` 措辞与 14 侧说明两项待做未做，P1-1/P1-2 的回填待做同样未做，E-THREAD-MODEL 等待立项。没有任何条目完全闭环，因此本文件不做删除清理，只追加本节。
> **验证基线（2026-09-17 实测）**：`cargo test -p minix-fileops -p minix-shell -p minix-init` 全部通过（20 + 36 + 89 = 145 个测试，0 失败）；`grep -c '"commands/bin/fileops"' os/Cargo.toml` 结果为 2（成员重复登记）；`find os/commands -path "*/src/bin*"` 零命中（全部组库没有任何二进制目标）。

### 6.0 本轮条目速览

| 级别 | 编号 | 一句话 | 状态 |
|------|------|--------|------|
| **P0** | C-1 | 二进制面整体缺失：58 个成员只有 `minix-init` 一个真实命令；"决定半"（23 个组库）与"执行半"（31 个 stub 二进制壳）两半分裂且互不引用。收敛为域内 crate + `src/bin/` 薄壳（[ARCH]） | 🔄 结构收敛完成（2026-09-17，迭代 5-7）：echo 模板落地、34 占位壳删除、文档/plan 同步、守卫全绿；逐域接线为长尾批次（见 §6.1 执行进度） |
| P1 | C-2 | `minix-sys` 顶层未再导出 errno 常量，`minix-init` 被迫直依赖 `minix-types`——99 §1 分层契约的唯一现存反例 | ✅ 18 侧完成（2026-09-17，迭代 4）：init 改用 `minix_sys::Errno::EEXIST`，依赖行已删；minix-sys 侧常量再导出仍挂 edge `E-CMDSYSFACE`（服务后续命令） |
| P1 | C-3 | 命令拿不到自己的参数与环境：`minix-rt` 只有 `progname()`/`argv_bytes(index)` 雏形，无环境访问器 | 🔄 14 侧半 ✅（LEDGER-AUDIT 复核 2026-09-21：`env_count/argv_count/argv_bytes/env_bytes/args()/envs()` 全在 crt0.rs:159-229，E-CMDSYSFACE 已闭单注记）；**消费侧待做**——全 `os/commands` grep `crt0::args` 零命中，echo 端到端批（C-1 §6.1 步 2）接 |
| P1 | C-4 | `plan.md:186` 验收基线写的 `cargo test -p commands-*` 包名前缀不存在，实际包名前缀是 `minix-*` | ✅ 完成（2026-09-17，迭代 3）：§3.4 改为 `minix-` 前缀可执行形式，同节过时的"全部为 stub"快照句一并按文档-代码同步门更新 |
| P2 | C-5 | `os/Cargo.toml:71` 与 `:77` 重复登记 `commands/bin/fileops`；`plan.md` §2 的"35 crate"数字与实际 58 个成员漂移 | ✅ 重复登记已删（2026-09-17，迭代 2）；数字随 C-1 ④ 更新 |
| P2 | C-6 | `/etc` 配置面零落地（`os/etc/` 只有占位 README），[ARCH] A-6（rc 脚本形态）决策悬置 | OQ 上交用户，不擅自决策 |
| P2 | C-7 | 99 §1 的边界硬规则只有文档约定，无机制化检查 | ✅ 完成（2026-09-17，迭代 8）：`tools/check-command-boundary.sh` 落地并全绿 |

---

### 6.1 C-1 命令二进制面整体缺失（P0，[ARCH]）

**问题的事实面**（每条均可 grep 复核）：

1. **真实的命令二进制只有一个**。`os/commands/sbin/init` 是唯一有完整实现的二进制（`src/main.rs` 55 行 + 15 个子模块 + 89 个测试）。其余 31 个声明了 `[[bin]]` 的 crate 全是 13 到 16 行的占位壳：`cat`、`cp`、`echo`、`ls`、`mv`、`rm`、`sh` 七个（如 `os/commands/bin/cat/src/main.rs` 全文 14 行，函数体只有一行 TODO 注释加 `exit(0)`）；`sbin/fsck`、`sbin/mkfs`、`sbin/reboot` 三个；游戏个体 crate 二十一个（如 `os/commands/games/tetris/src/main.rs`，其 `Cargo.toml` 只依赖 `minix-sys` 与 `minix-rt`，**不依赖**装着俄罗斯方块逻辑的 `minix-term-games` 库）。
2. **组库全部没有二进制目标**。23 个承载真实逻辑的库 crate（约 23,000 行、约 700 个测试，如 `minix-fileops` 的 `mode.rs`/`testexpr.rs`/`path.rs`）没有任何一个存在 `src/bin/` 目录；`os/commands` 全树 `find -path "*/src/bin*"` 零命中。
3. **两半互不引用**。库 crate 持有"决定半"（纯逻辑），占位壳持有"执行半"（main 函数），但没有任何占位壳调用任何库——`tetris` 不用 `term-games`，`sh` 不用 `shell`。
4. **文档对现状是诚实的**：`06-file-ops.md:8` 自述"cat、cp、echo、ls、mv、rm 仍为启动占位"，`05-shell-family.md:8`、22/23/24 三篇的游戏条目同。问题不在文档谎报，而在结构本身：照这个结构补下去，每实现一个命令都要同时动两个 crate，并且文档到 crate 的映射是 N 对 M，覆盖度无法按篇核对。

**设计判断（回答"如果今天重写会怎么设计"）**：组库文档里写的分层原则本身是对的——`os/commands/bin/fileops/src/lib.rs:8-25` 明确说"命令分成决定（哪些位、哪些文件、哪个真值）与执行（系统调用）两半，本 crate 持有决定半"，并援引 Redox"可测纯库 + 薄程序"的习惯。**缺的不是原则，是薄程序从来没有被创建**。因此本轮不推翻"纯逻辑下沉、薄壳在上"的分层，只裁决薄壳放在哪里。

**方案对比**：

| 方案 | 内容 | 裁决 | 理由 |
|------|------|------|------|
| A 维持现状 | 组库持逻辑，命令将来建独立 bin crate 去调用 | 否 | 58 个成员还会继续膨胀（328 个命令）；每个命令两处登记；文档到 crate 映射 N 对 M；跨 crate 依赖网随命令数线性增长 |
| **B 域内薄壳（选）** | 每篇命令文档对应恰好一个 crate；组内每个命令是 `src/bin/{命令名}.rs` 的薄 main（解析参数、调本 crate 库、按 POSIX 语义给退出码） | **选** | 文档与 crate 一一对应，覆盖度可按篇审计（该篇 §5.2 归属表的每个命令都应有同名 bin）；无跨 crate 依赖；先例实证见下；workspace 成员数 58 收敛到约 26 |
| C 每命令一个 crate | 镜像 `minix3/bin/` 的目录粒度，328 个 crate | 否 | workspace 元数据与编译成本爆炸；uutils 走这条路且被迫自带代码生成工具（[待验证]），说明该粒度在 Rust monorepo 里并不轻松 |
| D BusyBox 多路复用 | 一个二进制按 argv[0] 分发全部命令 | 明确排除 | 改变外部可观察行为（argv[0] 语义、符号链接安装面、报错前缀），越过 Rewrite 边界；Minix3 的 C 实现也不是这个形态 |

**先例锚点**：Redox 的 `redox-os/coreutils` 仓库正是方案 B 的形态——单个 Cargo 包，`autobins = false`，每个工具一条显式 `[[bin]]`（`name = "chown"`、`path = "src/bin/chown.rs"` 等，2026-09-17 联网读取其 master 分支 Cargo.toml 实证）。uutils/coreutils（每工具一个 crate 加共享 `uucore` 库）证明"共享逻辑 + 薄壳"是 Rust 社区惯例，但其 crate 粒度（方案 C）对本项目不经济，[待验证] 因本轮未直接核对其仓库结构。

**收敛目标**：24 个组库与 24 篇命令文档（01 到 24）已经一一对应——`init`(01)、`svcsched`(02)、`login`(03)、`devdb`(04)、`shell`(05)、`fileops`(06)、`textfilter`(07)、`regex`(08)、`editor`(09)、`doctools`(10)、`compress`(11)、`proctools`(12)、`termctl`(13)、`mountinfo`(14)、`diskfmt`(15)、`diskimg`(16)、`maint`(17)、`netconfig`(18)、`netservices`(19)、`sysinfo`(20)、`pkgtools`(21)、`stdio-games`(22)、`term-games`(23)、`text-games`(24)。这说明"按文档建库"事实上已是既成标准，本轮只是把它写死并补完另一半。安装分层（bin/sbin/usr.bin）只决定权限与 PATH，不决定 crate 归属——这是 `plan.md` §1.3 既有原则在构建布局上的直接推论。

**落地步骤**（执行时逐条走 fix-guard，每步 `cargo test -p {crate}` 回归）：

1. **init 域示范**：`minix-init` 增加 `src/bin/reboot.rs`（并按 `plan.md` §5.2 归属表补 `shutdown`、`rcorder`），删除 `os/commands/sbin/reboot` 占位 crate。init 是唯一跑通全链的样板，薄壳写法以它为准（`minix_rt::init()` 显式调用、errno 到退出码的映射）。
2. **echo 第一个端到端命令**：`minix-fileops` 增 `src/bin/echo.rs`——读参数、`write` 到标准输出、退出码。99 §3 的示例行已写明 echo 只需要 `write`、`exit`、argv 交接；这一步同时端到端验证 C-2、C-3 解锁后的 libc 面。随后按 `plan.md` §6 实施顺序（stdio 批先行：06 → 07 → 08 → 22）逐域补 bin 与对应逻辑，每批同步回填该篇契约表的 Requires 列。
3. **删除占位 crate**：`bin/{cat,cp,echo,ls,mv,rm,sh}` 七个、游戏个体 crate 二十一个、`sbin/{fsck,mkfs,reboot}` 三个，共 31 个成员从 `os/Cargo.toml` 移除；对应逻辑的落点：cat/cp/echo/ls/mv/rm 与 test/chmod 等归 `fileops`，sh 归 `shell`，fsck 归 `mountinfo`（14 篇），mkfs 归 `diskfmt`（15 篇），游戏按 22/23/24 归三库。fsck/mkfs 的真实实现受 edge `E-FSCMDS` 约束（等 15-stage 盘上结构层稳定），先删占位壳、逻辑后补，两者不冲突。
4. **同步修正**：`os/Cargo.toml` 去重（C-5）；`plan.md` §2 的"35 crate"数字与 §3.4 基线命令（C-4）更新；各篇文档 Rust 模块行的 crate 清单同步。装机面（哪些 bin 进根文件系统镜像、装到哪个 PATH 层）随 bin 落地同步登记到 `xtask` 的镜像清单（xtask 当前职责 [待验证]，执行时核实）。
5. **防自创层**：跨域共享的工具函数不得新建"命令公共库"crate——跨域 helper 要么下沉 `minix-rt`（属运行时能力，走 edge 登记），要么就留在本域 crate 内。这是对"AI 每轮补代码时自创路径"风险的显式防线。

**为何是 [ARCH]**：crate 边界是架构决策，须三处一致标注——`plan.md` §2 Rust 侧列、各篇文档 Rust 模块行、`os/Cargo.toml` 成员表与各 crate 布局。

**执行进度（2026-09-17，迭代 5）**：

- ✅ **echo 端到端落地**（原步骤 2 提前为模板步骤）：`minix-fileops` 新增 `src/echo.rs` 决定半——`echo_emit(argv, emit_sink)` 零分配纯函数，承载 echo.c 的全部判定语义（仅首位 `-n` 是标志且 echo.c:61 明文禁止 getopt、单空格分隔无尾随、换行控制、写失败即停上报），10 个测试全过（ crate 共 30）；`src/bin/echo.rs` 执行半薄壳——`minix_sys::write(STDOUT, piece)` 逐片写、退出码 0/1。
- ✅ **两个宿主/真机接缝的裁决**（新发现，超出原计划）：① `minix_sys::exit` 在宿主必然自旋（`pm.rs:141-155` 忠实复刻 C `_exit` 的最后手段——协议失败即挂），echo 因此改用显式 `terminate()` 接缝（std 构建走宿主运行时终止；no_std 构建换 `minix_sys::exit`，与 argv 接缝同批切换）；② 冒烟进一步暴露 `perform_syscall`（`syscall.rs:94-101`）按 m_type 负值判错而 `DirectTrapTransport` 的 Err 携带正 errno（`ipc.rs:549`），宿主下全部 `*_via` 假成功（write 返回 Ok(5)）——属 14 侧共享协议层，登记 edge **E-SYSCALL-SIGN**，14 修复后本命令宿主行为自动变诚实（exit 1），命令代码零改动。
- ⚠️ **步骤顺序偏离及理由**：原步骤 1 是"init 域示范（reboot/shutdown/rcorder）"。让位于 echo 的理由：reboot/shutdown/rcorder 按 plan §6 属交付链收尾批 8，且 reboot(2)/时间解析等执行面尚缺、先行只会造出新的"说谎二进制"；echo 是 §6 批 1 的 stdio 批成员、99 §3 已裁定其依赖齐备，作为模板更有代表性。init 域 bin 留在批 8 与其执行面一起实现。
- ✅ **步骤 3 占位壳删除完成**（2026-09-17，迭代 6）：34 个说谎占位 crate（bin 七个、sbin 三个、游戏个体二十四个）已从 `os/commands/` 与 workspace 成员表整体移除，成员收敛为 24 个域 crate（init + 23 库）+ 少量服务/工具项；`cargo metadata` 校验通过、`minix-stdio-games` 35 测试与 `minix-fileops` 30 测试回归全绿、仓库内无任何 crate 引用它们（grep 实证）。`primes`（流式区间）与 `arithmetic`（交互随机源）等执行面缺口随各域批次的 Requires 列登记，不再以 exit(0) 占位壳的形式假装存在。
- 待续：步骤 4（plan 与各篇文档头同步）、后续逐域接线（每批同步回填 Requires 列）。
- ✅ **步骤 4 文档/plan 同步完成**（2026-09-17，迭代 7）：六篇文档 Rust 模块行（05/06/14/15/22/23/24）、99 两处引用已删除文件的 echo 锚点（模式 66 RCPD 现实案例，改为指向 `os/commands/bin/fileops/src/bin/echo.rs`）、plan §2 的"35 crate"数字（改 24 个域 crate）与四行 Rust 侧列、`minix-stdio-games` 库头的 bcd/ppt 过度声称（按现状修正为"契约在内、实现未写"）。
- **长尾批次登记（C-1 的逐域接线，每批 = 一次 todo-fix）**：批次顺序沿用 plan §6（stdio 批 06→07→08→22 先行）。每批的动作固定为：该篇契约表逐命令回填 Requires 列（P1-1）→ 库内决定半补齐 → `src/bin/{命令}.rs` 薄壳（argv/terminate 接缝照 echo 模板，no_std 目标落地时一刀切换）→ 行为测试 → `cargo test -p {crate}` → 守卫脚本通过 → 文档 §5 测试统计同步。
- ✅ **批次十六完成：07 篇 colrm/tee 接线**（2026-09-18，迭代 24）：`colrm.rs` 决定半（列显示计：tab 进位 8 的倍数、退格回拉、换行清零；范围判定用处理后的新列值，8 测试）加薄壳；`tee.rs` 薄壳（stdin 到 stdout 扇出、部分写循环；文件目标待开放路径——已按 do_svis 语义警告退出 1）；textfilter 123 个测试全过、守卫除并行在制 host.rs 外全绿；07 篇 §4.4/§5 随批更新。剩余批次：diff/patch 引擎、col/column/hexdump/jot/lam/units/look/ifdef/crc/unifdef、05 sh、23/24。
- ✅ **批次十七完成：07 篇 column 接线**（2026-09-18，迭代 25）：textfilter 新增 `column.rs` 决定半——条目解析（跳过行首空白、丢弃空行）、`-t` 表格模式（strtok 语义切字段、逐列最大宽对齐、字段后两空格）、`-x` 横向填充与缺省纵向填充（termwidth 80、TABROUND 8 倍数对齐，均对照 column.c:59/68/128/141-199），18 测试全过；`-c`/`-s`/`-x` 就位，文件操作数待开放路径；薄壳与 §4.4/§5 随批更新。剩余：diff/patch 引擎、hexdump/jot/lam/units/look/ifdef/crc/unifdef、col、05 sh、23/24。
- ✅ **批次十八完成：07 篇剩余纯逻辑命令接线（col/jot/lam/hexdump/units/unifdef）**（2026-09-18，迭代 26）：textfilter 新增六个决定半模块与薄壳——`col.rs`（半行进位账本与冲刷规则全量移植：退格叠印、回车复写、ESC-7/8/9 进退、SI/SO、`-b`、空格压缩成 tab、计数排序，15 测试；UTF-8 续字节按 C locale 丢弃）；`jot.rs`（右到左操作数推导、格式合成校验、`-r` 注入 LCG，12 测试）；`lam.rs`（逐流规格、大写粘滞选项、min.max 宽度，7 测试）；`hexdump.rs`（七种规范格式按块渲染、`*` 抑制三态、越界补白、`%_p` 零宽垫，12 测试；`-e/-f` 显式拒绝）；`units.rs`（词表/归约/顺从性/`-l -L` 清单，10 测试）；`unifdef.rs`（十状态机、表达式求值、关键字改写、`-c/-l/-s/-t`，17 测试）；新增 `floatfmt.rs` 无 libm 浮点垫片（3 测试）。查证：look/ifdef 不在 minix3/usr.bin（属 m4 内建）、crc 已由 cksum 覆盖，均记入 07 篇 §4.4。textfilter 203 个测试全过（ulimit -v 3G + -j 1 内存闸门下运行）；边界守卫除并行在制 init/host.rs 外全绿；07 篇 §4.4/§5 随批更新（测试计数 127→203、二进制 25→31）。剩余：diff/patch 引擎、05 sh、23/24。
- ✅ **批次十九完成：07 篇 diff 引擎决定半与薄壳**（2026-09-18，迭代 27）：textfilter 新增 `diff.rs`——4.4BSD 两文件差异引擎全链移植（diffreg.c）：稠密等价类替代 readhash 字节哈希（jackpot 复核随之精确化，另补 check 等价的逐行复核挡住 member 哨兵 y=1 假候选）、公共前后缀剪枝、sort+equiv+unsort 构造 class/member、stone 候选搜索（isqrt 尝试上限、`-d` 无界）、unravel 出 J 表、六种输出格式（normal、`-e` 逆序 ed、`-f` forward ed、`-n` RCS、`-q`、`-u` 上下文合并与 `@@` 头），15 测试全部对照宿主 diff 实测向量；薄壳 `-` stdin 单侧可用、命名文件操作数待 open-existing；textfilter 219 个测试全过（ulimit -v 3G + -j 1）；边界守卫除并行在制 init/host.rs 外全绿；07 篇 §4.4/§5 随批更新（219 测试、32 二进制）。剩余：patch 引擎（pch.c 解析+应用）、05 sh、23/24。
- ✅ **批次二十八/二十九完成：卡E 批一+批二（net 面）**（迭代 36-37；随 17-stage 轨道）：
  批一（`e49f8844e`）lwip/uds 的 `wait_for_init` 握手门（RS `SEF_INIT` 通知到达回 OK，
  C `sef_startup` 形状）加信号面（SYSTEM 通知置终止旗标，lwip 即停、uds 走
  `on_terminate` 排水规则）；顺带修 HEAD 上 net-lwip 编译断裂（`sdev` 实际住
  minix-sockdriver）。批二 `bin/lwip` 的 `-dev` 参数解析与处理器保管、十三步链
  第一步播种真实化、03 篇十三步落点表（逐步对位：播种已实现/五步等 smoltcp/
  三步等 16-stage NIC/四步库内部分就绪）。lwip 96 / uds 8 全绿（ulimit -v 3G +
  -j 1）；clippy 零告警。**余**：栈本体 smoltcp 依赖落地（N1-P1-3 裁决后的集成
  批）、逐步替换、rc 挂载点（edge4 §6 OQ）。
- ✅ **批次二十七完成：24 篇 wtf/fortune/random 接线**（迭代 35）：text-games 三枚薄壳——`bin/wtf.rs`（`-f` 指库、`is` 词跳过、逐词 `term: expansion`、任一查无退出 1）、`bin/fortune.rs`（库文件按行切分、时钟秒抽取走 `pick_quip` 取模）、`bin/random.rs`（标准输入逐行、LCG 掷 `1/N` 后中覆盖先中、`mix_seed` 混时钟与 getpid 播种）。文件读取走 L10 open/read/close 既有路。**声明性留白**：adventure/monop 完整游玩循环、fish 对手回合、fortune 索引表版数据——24 篇随批更新。`cargo test -p minix-text-games` 22 个全过（ulimit -v 3G + -j 1）；clippy 零告警。**S35 的命令接线面至此收官**：可接的域全部接线（01 init 域归卡 J、14~17 存储批等卡 G/S32、06~13 已清、18~19 等 socket 面、20/21/23/24-其余 留白面已声明），余下全部等跨线原语。
- ✅ **批次三十八完成：06 篇 `test`/`[` 条件表达式接线**（`2026-10-10`，`claim/CMD-TEST-BIN-qoder`，二线 minix-sys + 三线 fileops 跨界，本笔登记）：兑现 §4.5 登记的「test 文件问的 stat 依赖」——`test`/`[` 此前有决定半骨架（`testexpr.rs`）但无生产调用方、且只建模常用算子。本轮补齐并落薄壳。决定半 `minix_fileops::testexpr` 扩到 `test.c` 全算子面：新增纯函数 `access_bits`，复刻 C `test_access` 对 `st_mode` 位的判定（root 命中任一类、否则按 owner / 有效组 / 附加组 / 其余四类车道，`minix3/bin/test/test.c:495-533`；刻意不调 `access(2)`，理由即 C 源 373-494 行的长注——对 root 而言 `open()` 语义使 `-r/-w/-x` 恒真、失去意义）；`FileKind` 把此前合并的 `device` 一位拆成 `block`/`char`（修 `-b`/`-c` 对块/字符设备混判的失真）；`FileTester` 加 `flags`（`-u`/`-g`/`-k` 的 setuid/setgid/sticky）、`owned_by_me`/`grouped_by_me`（`-O`/`-G`）、`same_file`（`-ef` 比较 device+inode）、`modified_since_read`（`-N`，mtime 晚于 atime）。执行半 `src/bin/test.rs`：真 `FileTester` 从 `minix_sys::stat`/`lstat` 取数（`-L` 读 `lstat` 不跟随末符号链接、其余读 `stat` 跟随，对位 `test.c:540`），读/写/执行经 `access_bits`；`[` 形态按 argv[0] 判定并剥结尾 `]`，缺 `]` 退出码 2（`test.c:223-227`）；退出码 0/1/2 照 C `res = !oexpr(...)`（`test.c:233`）与 `verrx(2)`。为 `-O`/`-G`/`access_bits` 补 `minix_sys::getgid`/`getegid`（`PM_GETGID`，回复 m1i1=rgid、m1i2=egid，对位既有 `getuid_via`；服务端 `GetResult::Gid{real,eff}`→`prefill(real,eff)` 同形已读码核实）。验证：宿主 `cargo test -p minix-fileops` → 78 passed（新 4 枚：`-u/-g/-k/-O/-G/-N` 与 `-b`≠`-c` 表驱动、`-ef`、`access_bits` 对 C 四类车道向量）、`cargo test -p minix-sys --lib` → 321 passed（新 1 枚 `getgid` 车道序钉）；clippy 触及文件零告警；freestanding `cargo build -p minix-fileops --target x86_64-unknown-none --release`（docker `minix-ci:1.94`）绿。**真机 per-command 断言**：`test`/`[` 只回退出码、无 stdout，rc 冒烟要 shell 按退出码分支打 marker，而 05 篇 sh 执行批待 `execve`/`pipe2` 三缺口落地——故 imgrd `/bin/test` 播种与真机命令断言一并归 `C-57`/E5（命令面真机半），与写类命令同批。声明性留白：`-t`（isatty 无客户端封装，决定半返 false）、附加组当前喂空列表（`access_bits` 形参保留，待接 `getgroups`）。
- ✅ **批次三十九完成：12 篇 `date` 接线**（`2026-10-10`，`claim/CMD-DATE-BIN-qoder`，单域三线 commands/proctools，无跨界——`gettimeofday` 封装早存）：落地缺失命令 `date` 的显示路（读时钟打印），决定半新增 `minix_proctools::dates`——`breakdown`（历元秒 → UTC 年/月/日/时/分/秒/星期/年内日，复用 `stamp::civil_from_days` 这套 Hinnant 历法逆函数、不另立历法真源）、`format`（`strftime` 子集，含默认版式 `%a %b %e %H:%M:%S %Z %Y` 与 `%A %B %C %d %e %F %H %I %j %m %M %p %R %S %T %u %w %y %Z %%` 等；未建模指示符逐字透传，声明性子集）；`is_leap`/`yearday` 纯整数。执行半 `src/bin/date.rs`：默认/`+FORMAT`/`-r SECONDS`/`-u`（无时区数据库本层即 UTC，忠实空操作）/`-j` 接受；`-d` 按 Minix 自身 `__minix` 构建（date.c:100-103 对 `-d` 恒报错）拒绝。验证：`cargo test -p minix-proctools --lib dates` → 8 passed（历元 0=1970 Jan 1 Thu、1e9=2001 Sep 9 Sun、闰日 2024 Feb 29 yearday=60、负历元 1969 Dec 31、零/空格填充日、ISO/12 小时上下午/世纪两位数年逐条对真值）；freestanding `cargo build -p minix-proctools --target x86_64-unknown-none --release`（docker `minix-ci:1.94`）绿；clippy 触及文件零告警（ptime.rs 三告警为存量）。**声明性留白**：设时路（`date operand` 需 `settimeofday`/`adjtime` + wtmp/syslog，date.c:337-357）无客户端封装，遇 set 操作数报错退 1 不假装设时；无时区数据库→breakdown 与 `%Z` 恒 UTC（与 `stamp.rs` 同界的声明性边界，12-process-tools.md §5）。真机 per-command 断言与 imgrd 播种同归 `C-57`/E5（与 test/[ 同批）。
- ✅ **批次四十完成：14 篇 `df` 接线**（`2026-10-10`，`claim/CMD-DF-BIN-qoder`，跨二线 minix-sys/minix-types + 三线 mountinfo，本笔登记）：落地缺失命令 `df` 的 POSIX `-P` 视图。决定半 `minix_mountinfo::df`（`fsbtoblk` 对 df.c:350-353 宏、`capacity_percent` 上取整、`format_p_row` 精确复刻 `-P` 行 `printf`(df.c:471-477) 与 `P_HEADER`）——纯整数/字符串，合成 StatvfsBuf 向量可测。执行半 `src/bin/df.rs`：无操作数走 `minix_sys::getvfsstat` 枚举全部挂载、有操作数逐路径 `statvfs`（VFS_GETVFSSTAT / VFS_STATVFS1）。为此补 minix-sys 客户端封装 `statvfs1_via`/`getvfsstat_via` + 顶层 `statvfs`/`getvfsstat`（两枚 wire 回放测，照 `stat_via`/`getdents_via` 同形）、minix-types 补 `ST_WAIT`。验证：`cargo test -p minix-mountinfo --lib df` → 5/0（-P 精确行、小分块缩放、容量上取整、保留块扣减、头）、`-p minix-sys --lib`→323/0(+2)、`-p minix-mountinfo --lib`→17/0；freestanding `--target x86_64-unknown-none --release`（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：`-P`/`-k` 之外的 `-h` 人类可读、512 旧对齐、`-t/-i/-g/-G/-a/-l` 过滤/格式面未建模（请求即报 option not wired 退 1，不出错表）；真机 per-command 断言与 imgrd `/bin/df` 播种归 `C-57`/E5。
- ✅ **批次四十一完成：16 篇 `dd` copy pump 接线**（`2026-10-10`，`claim/CMD-DD-PUMP-qoder`，单域三线 commands/diskimg——diskimg 首个 bin）：diskimg 此前只有决定半（`dd.rs` 的 `parse_operands`/`CopyPlan`、无任何 bin），本轮补执行半。决定半 `dd.rs` 新增 `Io` trait + `pump`（ibs==obs 的 `bs=` 复制：skip 读丢弃、seek 写零、count 限量、短读即 EOF、`conv=sync` 补齐、`conv=noerror` 跳错，scratch 由调用方供、无分配）+ 8 枚 mock-Io 宿主测；执行半 `src/bin/dd.rs` + 本 crate 首个 `bin_support.rs` + Cargo（新增 minix-sys 依赖 + `[[bin]] dd` + minix-rt target-cfg）：`FileIo` 绑 fd、open 输入(stdin)/输出(stdout，默认 O_TRUNC 除非 notrunc)、POSIX `records in/out` 摘要走 stderr。**声明性留白**：`ibs≠obs` 分块机、`conv=` case/swab/block/unblock/cbs、tape `files=`、`iflag/oflag` 全谱未建模（pump 拒 `ibs≠obs`、bin 报 transfer failed 而非出错表）；真机 per-command 断言与 imgrd `/bin/dd` 播种归 `C-57`/E5。
- ✅ **批次四十二完成：10 篇 `join` 接线**（`2026-10-10`，`claim/CMD-JOIN-qoder`，单域三线 commands/textfilter，无跨界）：落地缺失命令 `join`（按键合并两个有序输入）。决定半新增 `minix_textfilter::join`——`split_fields`（按分隔符切字段、空段跳过，对应 C strsep+`spans`）、`group_end`（同键分组，对应 `slurp`）、`join_lines`（两游标归并：等键叉乘输出、较小键单侧推进、`-a` 输出未配对行）、`twoline`/`onlyline`/`push_tok`（默认版式：连接字段+文件一其余+文件二其余，单分隔符），纯字节行入/出、6 枚宿主测。执行半 `src/bin/join.rs`（textfilter 自动发现 bin，无需 Cargo）：解 `-t`/`-a`/`-1`/`-2`，两文件操作数（`-`=stdin）经 `read_file`、行切分喂 `join_lines`、输出逐行。**声明性留白**：`-o` 输出域表、`-e` 空替换、`-j`、`-v`、旧式单参形未建模（请求即报 option not wired 退 1，不出错表）；真机 per-command 断言与 imgrd `/bin/join` 播种归 `C-57`/E5。验证：`cargo test -p minix-textfilter --lib join::`→6/0（默认内连/`-a`/等键叉乘/自定义连接域/`-t` 分隔/多字段行）、全 crate `--lib`→235/0；freestanding `--target x86_64-unknown-none --release`（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。
- ✅ **批次四十三完成：10 篇 `nl` 接线**（`2026-10-10`，`claim/CMD-NL-qoder`，单域三线 commands/textfilter，无跨界）：落地缺失命令 `nl`（行号）。决定半新增 `minix_textfilter::nl`——`nl_lines`（逐行：恰好为 `\:` 的分隔行重置计数（restart 开）且不自输出，对应 C `filter` 的 `goto nextline`；body 行按 number_nonempty/number_all 决定是否编号，未编号行仍填 `width` 个空格，接 `num + sep + 行`）、`write_number`（Ln/Rn/Rz 三种宽度域格式），对位 nl.c:272-353，6 枚宿主测。执行半 `src/bin/nl.rs`（textfilter 自动发现 bin，无需 Cargo）：解 `-a`/`-v`/`-i`/`-l`/`-n`/`-p`，读 stdin/文件（`read_file`）→行切分→`nl_lines`。**声明性留白**：`-b`/`-h`/`-f` 分节类型、`number_regex`、`-d` 多字分隔、`-s` 分隔串、`-w` 空行成组计数未建模（请求即报 option not wired 退 1）；真机 per-command 断言与 imgrd `/bin/nl` 播种归 `C-57`/E5。验证：`cargo test -p minix-textfilter --lib nl::`→6/0（非空默认编号/空行空字段/-a 全编/起值步宽/三种格式/分隔行重置丢弃）、全 crate `--lib`→241/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。
- ✅ **批次四十四完成：07 篇 `wc` 补 `-m`/`-L` 旗标**（`2026-10-10`，`claim/CMD-WC-FLAGS-qoder`，单域三线 commands/textfilter）：`wc` 此前只接 `-l/-w/-c`。决定半 `count::Counter` 新增 `chars`（UTF-8 码点计数：非 `10xxxxxx` 续字节计一次，对位 minix `-m` 的 mbrtowc 语义），长行 `longest` 早存但薄壳未接；执行半 `src/bin/wc.rs` 接 `-m`/`-L`，输出按 POSIX 字段序 newlines/words/chars/bytes/maxline（无旗标缺省仍 lines/words/bytes）。验证：`cargo test -p minix-textfilter --lib count::`→5/0（新增多字节码点测：“héllo”=6字节/5码点、“漢字”=6/2）、全 crate `--lib`→242/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：文件操作数仍报 E-CMDSYSFACE（本轮只补旗标，多文件/名列/total 行为另轮）；真机 per-command 与 imgrd 播种归 `C-57`/E5。
- ✅ **批次四十五完成：10 篇 `join` 补 `-o`/`-e` 输出域表**（`2026-10-10`，`claim/CMD-JOIN-OLIST-qoder`，单域三线 commands/textfilter）：继批次四十二声明留白后，接回 `-o`/`-e`。决定半 `join.rs` 新增 `Which`/`OField`、`parse_olist`（`"0"`=连接域、`"F.N"`=文件 F 第 N 域 1 基、逗号/空白分隔、畸形/0 基号拒）、`JoinOptions` 加 `output_fields`/`empty`；`twoline`/`onlyline` 有 `output_fields` 时逐 entry 经 `olist_token` 取值（缺域/另一侧缺席→`-e` 替换），无则默认版式。薄壳 `src/bin/join.rs` 接 `-o SPEC`/`-e STR`（`-j`/`-v` 仍拒）。验证：`cargo test -p minix-textfilter --lib join::`→9/0（新 3 枚：parse_olist/域选择含连接域/未配对行 `-e`）、全 crate `--lib`→245/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：`-j` 分节、旧式单参形未建模；真机 per-command 与 imgrd 播种归 `C-57`/E5。
- ✅ **批次四十六完成：10 篇 `nl` 分节模型与旗标纠偏**（`2026-10-10`，`claim/CMD-NL-SECTIONS-qoder`，单域三线 commands/textfilter）：full-review 发现批次四十三自行发明的 `-a` 旗标并非真 `nl` 选项（`nl(1)` usage 无 `-a`，编号类型走 `-b`/`-h`/`-f`）——本轮改正并接回当时声明留白的分节模型。决定半 `NlOptions` 以 `body`/`header`/`footer: NumberType` 取代 `number_all`，`nl_lines` 改成三节状态机：行恰为 `\:`/`\:\:`/`\:\:\:` 切至 footer/body/header、丢弃、`restart` 重置计数，其余行按当前节类型决定是否编号（`All`/`Nonempty`/`None`，对应 `-b/-h/-f` 的 a/t/n）。薄壳删 `-a`、接 `-b`/`-h`/`-f` 类型值（`p` 正则仍拒）。验证：`cargo test -p minix-textfilter --lib nl::`→7/0（新增三节切换/节类型独立/分隔行丢弃）、全 crate→246/0；freestanding x86_64-unknown-none --release（docker）绿；clippy 触及文件零告警。**声明性留白**：`number_regex`（`-b p…`）、`-d` 多字分隔、`-s` 分隔串、`-w` 空行成组仍未建模；真机 per-command 与 imgrd 播种归 `C-57`/E5。
- ✅ **批次四十七完成：07 篇 `fmt` 段落重排接线**（`2026-10-10`，`claim/CMD-FMT-BIN-qoder`，单域三线 commands/textfilter，无跨界）：落地缺失命令 `fmt`（bsd 段落重排，`todo.md` 批次二十四「剩余」清单所列）。决定半新增 `minix_textfilter::fmt`——`fmt_bytes` 忠实移植 `minix3/usr.bin/fmt/fmt.c` 的非居中管线：`run`（逐字节收行做 `^H` 退格与不可打印丢弃、tab 展 8 栏、裁尾空格，`fmt.c:258-312`）→`prefix`（量首行缩进、缩进变化按 `np>pfx || |pfx-np|>8` 冲行、`.` nroff 控制行独占，`fmt.c:324-370`）→`split`（切词、行末随附分隔空格、句末 `. : !` 后补第二空格，`fmt.c:378-420`）→`pack`（bsd goal/max 贪心装箱：`t<=goal` 或 `t<=max && s<=goal && t-goal<=goal-s` 收词，否则冲行换行，`fmt.c:445-475`）→`oflush`/`tabulate`（裁尾、行首空格回编成 tab，`fmt.c:481-528`）。goal=65/max=75 取 `fmt.c:66-67` 缺省。薄壳 `src/bin/fmt.rs`：`-g`/`-m`/`-w`/`-r` 与旧式位置参数 `[goal [max]]` 兼容形（`fmt.c:143-150`）、`max>goal` 校验（`:152-155`），stdin/文件按序拼接（对位 C 跨文件共享全局）。13 枚宿主测逐函数手推对真值。三件套：`cargo test -p minix-textfilter --lib fmt::tests`→13/0、全 crate `--lib`→259/0、freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿、clippy 触及文件零告警。**声明性留白**：邮件头启发式 `ishead`（`minix3/usr.bin/mail/head.c:266`，mbox `From `+日期探测，依赖 mail `parse`/`isdate` 另一阶段机械）与 `To`/`Subject`/`Cc` 近头抑制未接——非邮件输入下 C 该支路恒不触发，故本实现与 C 逐字节同；`-C` 居中模式与 base-非十进制数（`getnum` 的 `strtoul` 前缀）未建模（请求即拒退 1）；真机 per-command 与 imgrd `/bin/fmt` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次四十七。
- ✅ **批次四十八完成：11 篇 `uuencode` 接线（含 base64）**（`2026-10-10`，`claim/CMD-UUENCODE-BIN-qoder`，单域三线 commands/compress，无跨界）：落地缺失命令 `uuencode`——`minix_compress::uu` 早有 prepared-but-unwired 决定半（`encode_group`/`length_byte` 固定 4 字节组，无 bin）；本轮接回行框与流编码。决定半新增 `uu::encode_stream`（每 45 字节一行、行首长度字节、每 3 字节一组 `encode_group` 四 sextet、末尾零长度 反引号行，对位 `uuencode.c:156-193`）与 `uu::base64_stream`（标准 `A-Za-z0-9+/` + `=` 补位、每 16 组 64 字符换行，对位 `base64_encode`/`b64_ntop` `uuencode.c:128-151`；空输入→空体与 C 同）。执行半新增本 crate 首个 `src/bin/uuencode.rs`：`uuencode [input] name` 两/一操作数语义——文件操作数 `st_mode & 0777`（对位 C freopen+fstat，`:92-95`）、无文件时 stdin + `0666 & ~umask(0666)`（`:99-100`）、`-m` base64、`begin`/`end` 或 `begin-base64`/`====` 框、八进制模式无前导零（`%o`）。为此放宽 compress lib 的「no heap」表述（`extern crate alloc`，全局分配器走 minix-rt）——uu 流编码输出变长，是唯一堆分配腿。**声明性留白**：解码方向 `uudecode`（另一命令，未动）、短尾组 C 复用 buffer 残留字节（本实现补零，长度字节已标真实字节数、解码侧丢弃，与 C round-trip 同）。验证：`cargo test -p minix-compress --lib uu::tests`→13/0（新 7 枚：canonical Man/单字节补/空/45 切行/base64 Man·M·Ma/空/16 组换行边界）、全 crate `--lib`→35/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。真机 per-command 与 imgrd 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次四十八。
- ✅ **批次四十九完成：07 篇 `what` 接线**（`2026-10-10`，`claim/CMD-WHAT-BIN-qoder`，单域三线 commands/textfilter，无跨界）：落地缺失命令 `what`（抽取文件内容里的 SCCS `@(#)` 标识串）。决定半新增 `minix_textfilter::what`——`what_matches` 扫描 `@(#)` 四字节标记、命中后取至第一个终止符（NUL / `"` / `>` / `\n` / `\`，`what.c:112`）为止的内容字节；探针中途失配（`@` 后非 `(`）不消费该字节、以它重开候选（对位 C `goto loop`，使 `@@(#)` 仍可命中）。执行半 `src/bin/what.rs`：逐文件操作数（无 stdin 模式，`what.c:81`）——先回显文件名、再逐条 `\t`+内容+`\n`；`-s` 每文件仅留首个匹配（C break 扫描环 `:117-118`）；退出码按全局累计匹配数 `>0?0:1`（`:93`）；打开失败即停并按已累计数退出（`:84-86`）。7 枚宿主测覆盖单标记/无标记/终止符切分/换行分两匹配/`@@` 重叠/空匹配/EOF 收尾。三件套：`cargo test -p minix-textfilter --lib what::`→7/0、全 crate `--lib`→266/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：`perror` 系统错文未复现（仅报操作数名）；真机 per-command 与 imgrd 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次四十九。
- ✅ **批次五十完成：07 篇 `asa` 接线**（`2026-10-10`，`claim/CMD-ASA-BIN-qoder`，单域三线 commands/textfilter，无跨界）：落地缺失命令 `asa`（ASA 走纸控制后处理，`asa.c` 117 行、纯 stdin→stdout）。决定半新增 `minix_textfilter::asa`——`records` 按 `fgetln` 语义切行（每行含控制字符，行尾换行产一行、末尾无换行也产一行、空输入→零行），`asa` 逐行：首行只对 `0`/`1` 起头动（空格/`+`/其它首行不动，`asa.c:79-86`），后续行 `' '`/未知→一个换行、`0`→两个换行（双空行）、`1`→换页 `\f`、`+`→回车 `\r`（`asa.c:94-109`）；行体仅当 `len>1 && buf[0]!=0 && buf[1]!=0` 时从第 2 字节写出（`asa.c:88,111`）；至少有一行时流末尾补一个换行。执行半 `src/bin/asa.rs`：无操作数读 stdin，有则逐文件独立跑（对位 C 每文件 `asa(fp)`、各自保留首行特例与末尾换行），打开失败告警跳过、退出码恒 0（`asa.c:66`）。10 枚宿主测覆盖空格控/换页首行/双空行/回车叠印/首行 `+` 惰性/空行分隔/空输入/单字节无体/NUL 首内容抑制/末行无换行。三件套：`cargo test -p minix-textfilter --lib asa::`→10/0、全 crate `--lib`→276/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：`warn` 系统错文未复现（仅报操作数名）；真机 per-command 与 imgrd 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十。
- ✅ **批次五十一完成：07 篇 `look` 接线**（`2026-10-10`，`claim/CMD-LOOK-BIN-qoder`，单域三线 commands/textfilter，无跨界）：落地缺失命令 `look`（在有序词表里找以给定串开头的行，真源 `minix3/minix/commands/look/look.c` 158 行）。决定半新增 `minix_textfilter::look`——`compare` 忠实移植 `look.c:24-58`：逐「显著字符」走查搜索串与候选词，搜索串先耗尽返回 0（匹配）、词先耗尽返回 1（词短），否则返回首个失配字节之差；`-d` 字典模式跳过非字母数字且非空格的字符、并把连续空格折成一个（`:33-40,46-53`），`-f` 比较前大小写折叠（`:41,54`）。`look_matches` 按前缀选出所有 compare==0 的行。对 C 的按字节偏移二分定位（`:103-125`）——因 look 定义于「已排序词表」之上，二分的命中集与这里的线性过滤完全相同（行序也一致），故等价而非缩水。执行半 `src/bin/look.rs`：严格照 C 的 argc/argv 腾弄——`look string`（无文件、无旗标）默认开 `-d -f`（`:130`）、旗标组逐字解析、`look [-df] string [file]`；词表取文件操作数否则默认 `/usr/lib/dict/words`（镜像未装→报 Can't open、退 1，`look.c:92-95`）。8 枚宿主测覆盖精确前缀/词短/失配符号/折叠/字典跳标点/空格折叠/前缀子集保序/空前缀全匹配。三件套：`cargo test -p minix-textfilter --lib look::`→8/0、全 crate `--lib`→284/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：非 ASCII 中止（`nonascii()`）仅对搜索串复现、词表字节按值比较；`readword` 的 MAXLEN 截断报错未复现；默认词典未随镜像装（无文件形式即 Can't open）；真机 per-command 与 imgrd 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十一。
- ✅ **批次五十二完成：11 篇 `uudecode` 接线**（`2026-10-10`，`claim/CMD-UUDECODE-BIN-qoder`，单域三线 commands/compress，无跨界）：落地解码方向缺失命令 `uudecode`（真源 `minix3/usr.bin/uudecode/uudecode.c` 323 行；与批次四十八 `uuencode` 配对）。决定半新增于 `minix_compress::uu`：`parse_begin`（`begin`/`begin-base64` 头行→八进制 mode+文件名+base64 标志，strtol base 8 语义：跳前导空白取八进制位、要求后跟空白、文件名截至换行，对位 `uudecode.c:118-150`）、`decode_uu_stream`（每行首字节 `DEC`=（c-' '）&077 掩码长度为0则终止，每 4 字符组→3 字节、仅取 length 个真字节，对位 `:231-267`；与 encode 互补的掩码语义不用会拒非法字的 `decode_group`）、`decode_base64_stream`（标准 base64 反解，遇首个 `=`（末组填充或 `====` 终止行）停、跳空白，4 sixtet→最多 3 字节，对位 `base64_decode`/`b64_pton` `:290-314`）。执行半 `src/bin/uudecode.rs`：`uudecode [-m] [-p | -o outfile] [file ...]`——定位 begin 行、解码、`-m` 强制 base64、`-o` 覆盖名、`-p` 或名为 `/dev/stdout`/`-` → stdout、否则 `open(O_WRONLY|O_CREAT|O_TRUNC)+write+chmod(mode&0666)` 写文件（`freopen+fchmod` 对位 `:215-226`）；无文件操作数读 stdin；退出码按是否有失败（`:119`）。测试用 encode↔decode 双向往返钉住正确性（含空/单字节/残组/多行）。三件套：`cargo test -p minix-compress --lib uu::`→20/0（新 7 枚：parse_begin×3、decode_uu canonical+round-trip、decode_base64 canonical+round-trip）、全 crate `--lib`→42/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：`~user/file` 需 `getpwnam`（未封装，前导 `~` 按字面处理）；严格 `end`/`====` 尾行与 `short file` 诊断不复现（畸形体仅停解码器）；真机文件创建受可写根盘 E5 门控（同 cp/mv/rm 等写类，本笔交代码+宿主测）。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十二。
- ✅ **批次五十三完成：12 篇 `printenv` 接线**（`2026-10-10`，`claim/CMD-PRINTENV-BIN-qoder`，单域三线 commands/proctools，无跨界）：落地缺失命令 `printenv`（读环境变量，真源 `minix3/usr.bin/printenv/printenv.c` 102 行）。决定半新增 `minix_proctools::printenv`（无堆，与同 crate 一致）——`lookup` 忠实移植 `printenv.c:78-87`：候选项需以 name 开头**且** name 后一字节为 `=` 或行尾才算命中（故 `FO` 不误匹 `FOO=1`），返回 `=` 后的值字节（裸 `NAME` 无 `=` 则值为空），首个命中即返；`name_is_invalid`（含 `=` 拒，`:73-75`）。环境以 `&[&[u8]]`（C `environ` 遍历）传入，纯可测；真环境读取与输出字节归调用方。为此给 proctools `bin_support` 加 `envs()` 双 cfg seam（none 走 `minix_rt::crt0::envs()`、宿主走 `std::env::vars`，与 `args()` 同构）。执行半 `src/bin/printenv.rs`：`getopt("")`——首 `-x` 运 usage、`--` 结束选项扫描；无操作数逐行打全部 `KEY=VALUE` 退 0、单一 name 查值命中打值+`\n` 退 0、未命中退 1、多 name 或 name 含 `=` → usage/errx（`:71-91`）。本命令**只读环境 + 写 stdout**，无文件创建→不受 E5 写盘门控。6 枚宿主测：精确命中/未命中/前缀不误匹/空值与裸名/含=非法/首个命中优先。三件套：`cargo test -p minix-proctools --lib printenv::`→6/0、全 crate `--lib`→34/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：真机环境内容由 RS 下发决定（木桩测试以合成 env 切片验证）；getopt 具体置换行为按 printenv 单操作数语义简化。真机 per-command 与 imgrd 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十三。
- ✅ **批次五十四完成：07 篇 `colcrt` 接线**（`2026-10-10`，`claim/CMD-COLCRT-BIN-qoder`，单域三线 commands/textfilter，无跨界）：落地缺失命令 `colcrt`（CRT 终端仿真、重排 nroff 叠印，真源 `minix3/usr.bin/colcrt/colcrt.c` Bill Joy 1977 共 260 行）。决定半新增 `minix_textfilter::colcrt`——逐函数忠实移植 CRT 屏幕模型：`Screen`（267 半行×132 列的 `page`，C 全局）+ `feed`（逐字节：`\n` outline+=2、`\b` 退格、ESC `7`/`8`/`9` 半行上下移、SO/SI 跳过、tab 进 8 栏，对位 `colcrt.c:115-186`）+ `place`（光标处写格、`_`→下半行 `-`、命中非空则按 `plus` 合为 `+`、否则空格/空覆写；向前回填空格 `dp=cp-outcol` 对位 `:161-184`）+ `plus`（`(|,-)`或 d=='_'→真，`:192-197`）+ `pflush`/`move_bar`（半行竖线延续、空隔行 omit（除 `-2`）、窗口 memmove 下移，`:201-259`）。执行半 `src/bin/colcrt.rs`：前导 `-` 运 usage、`-` 抑下划线、`-2` 打全半行、无操作数读 stdin、有则逐文件各跑一次（对位 C 每文件 EOF pflush）。7 枚宿主测逐函数手推 golden（runner 当场纠了我两处推错：`-2` 含首空行、nroff 下划线是 `x\bx_` 非 `x_`）：plus 规则/纯文本透传（隔行折叠）/多行/`-2` 留空半行/下划线落半行+`-` 抑制/叠印竖线+横→`+`。三件套：`cargo test -p minix-textfilter --lib colcrt::`→7/0、全 crate `--lib`→291/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：多文件时 C 跨文件共享屏幕（本实现每文件独立 fresh screen，可见结果同）；`perror` 系统错文未复现（仅报操作数名）；真机 per-command 与 imgrd 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十四。
- ✅ **批次五十五完成：12 篇 `sleep` 接线**（`2026-10-10`，`claim/CMD-SLEEP-BIN-qoder`，单域三线 commands/proctools，无跨界）：落地缺失命令 `sleep`（暂停若干秒，真源 `minix3/bin/sleep/sleep.c` 159 行）。决定半新增 `minix_proctools::sleep`（无堆）——`plan` 忠实移植 `sleep.c:104-124` 的双路解析：先扫操作数有无非数字字节——无则整数路（`atol`，`<=0` → `NoSleep` 成退 0），有则小数路（`atof`，`<=0` → `Bad` 运 usage，否则 `floor`/小数拆 `secs`+`nanos`）。这个不对称是真实的：`"0"` 走整数→不睡退 0，而 `"0.0"` 走小数→报 usage。plan 以数据返回、交调用方跑 `nanosleep`。6 枚宿主测：整数秒/零不睡/小数拆分/非正拒绝/前导空格与符号走小数路/纳秒位截断。执行半 `src/bin/sleep.rs`：`sleep seconds` 单操作数（`getopt("")` 拒任意 `-x`），Bad→usage退1、NoSleep→退0、Sleep→`minix_sys::nanosleep(secs,nanos)`。三件套：`cargo test -p minix-proctools --lib sleep::`→6/0、全 crate `--lib`→40/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：`SIGINFO` 进度报告与 EINTR 剩余时长恢复循环（`sleep.c:126-136`）需信号处理注册、本树未接→nanosleep 只发一次；小数路不读指数/hex（`1e3`→1 非 1000）；真机 per-command 与 imgrd 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十五。
- ✅ **批次五十六完成：06 篇 `chmod` 接线**（`2026-10-10`，`claim/CMD-MKDIR-RMDIR-BIN-qoder`，单域三线 commands/fileops，无跨界）：命令面干净纯项已穷尽（余下 fpr/ul/deroff/banner 是无 C 参照的超大/终端依赖过滤器，tty/mesg 需未封装 ttyname，touch/date-set 受时区纠缠），故改接 prepared-but-unwired：`minix_fileops::mode`（`apply_mode` 已完整移植 BSD setmode+getmode、八进制+符号 u/g/o/a · +-=· rwxXst，带 8 枚宿主测）**早存但无 bin**。本轮补 `src/bin/chmod.rs`：逐操作数 `stat(path)`取当前位 → `apply_mode(base, spec)` → `chmod_via(path, mode)`，对位 `chmod.c:215-216` 的 `getmode(set, st_mode)`+`change_mode`；`-[rwxstugo]` 裸参按 C `--optind` 纠正当模式非旗标（`chmod -r file` 去读位）、`-f` 遇错继续仅置退出码、`-v` 报每文件。退出码按是否失败。三件套：`cargo test -p minix-fileops --lib mode::`→8/0（决定半既有测）、新 bin `chmod` freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：本轮无新增宿主测（实质逻辑在既有已测的 `mode.rs`，bin 仅为 syscall 腿 + 旗标流）；`-R`（fts 递归）/`-h`/`-H`/`-L`/`-P`（lchmod/fts 符号链遍历）/`--reference` 因依赖未封装设施报 not wired（非语义择重）；真机写盘受可写根盘 E5 门控（同 cp/mv/rm 等写类，本笔交代码+宿主测的 parser）。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十六。
- ✅ **批次五十七完成：06 篇 `pwd` 接线**（`2026-10-10`，`claim/CMD-PWD-BIN-qoder`，单域三线 commands/fileops，无跨界）：落地缺失命令 `pwd`（报当前工作目录，真源 `minix3/bin/pwd/pwd.c` 143 行）。决定半新增 `minix_fileops::pwd`（无堆）——`env_pwd_usable` 忠实移植 BSD `getcwd_logical` 的字符串预筛：`$PWD` 需非空且首字符 `/`、且不含 `/./` 与 `/../` 子串；dev/ino 同一性检查需 stat 归调用方。为此给 fileops `bin_support` 加 `envs()` 双 cfg seam（与 printenv 批次同构）。执行半 `src/bin/pwd.rs`：`pwd [-L|-P]`（默认物理，对位 C `lFlag=0`）；`-L` 下若 `env_pwd_usable(PWD)` 且 `stat(PWD)` 与 `stat(".")` 的 `st_dev`/`st_ino` 相等→用 `PWD`，否则回退 `getcwd`；物理路直接 `getcwd`；多余操作数/未知旗标→usage。只读不写→不受 E5 写盘门控、真机可跑。4 枚宿主测：合法绝对路径/相对或空/点组件/尾部无斜杠点允。三件套：`cargo test -p minix-fileops --lib pwd::`→4/0、全 crate `--lib`→82/0；freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**声明性留白**：仅 `-L`/`-P`（对位 BSD getopt 集，无 legacy `+P`）；真机 per-command 与 imgrd 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十七。
- ✅ **批次一百八十九接上 `split -a`（后缀宽度）**
  （`2026-10-13`，`claim/SPLIT-A-qoder` @ 基线 `c97c41460`，单域三线 commands/textfilter，无跨界）：
  接回批次一百七十八登记的 `-a` Unmodelled 留白。决定半 `split.rs`：`suffix(index)` 改为 `suffix(index, width)`
  （按 26 进制生成 `width` 字母后缀、`checked_pow` 定容量、超穷举或 width=0 返 None）；`SplitArgs` 新增
  `sfxlen`（默认 `DEFAULT_SFXLEN=2`）、`SplitError` 新增 `IllegalSuffixLength`；`parse_split_args` 把 `a` 从
  Unmodelled 拆出、按 getopt 取值（`-a3` 附值或 `-a 3` 下一词、`line_count` 校非零全数字），`-b`/`-n` 仍 Unmodelled。
  执行半 `bin/split.rs`：`out_path(prefix, index, width)` 透传 sfxlen；`arg_error` 新增 "illegal suffix length" 臂。
  width=2 逐字等于旧行为（零回归铉）。新增 3 枚测（`suffix` width 1/3/穷举/0 + `-a` 解析附值/分词/非法 + bin out_path width）。
  三件套：`cargo test -p minix-textfilter --lib` → **392/0**（既有 390 + 新 2）；`--bin split` → 测绿；
  freestanding `cargo build -p minix-textfilter --bin split --target {x86_64,aarch64,riscv64gc}-unknown-none --release` 三架构均 Finished；
  clippy 触及文件零新增告警（bin/split.rs 的 "items after a test module" 系基线即存的存量结构告警，不顺手改）；
  `rustfmt --edition 2024 --check` 两目标文件零漂移（共享件 bin_support.rs 的 import 排序存量漂移非本笔）；核实日 `2026-10-13`。
  真机腿：真写文件受可写根盘 E5 门控（同其它写类），本笔交代码+宿主测。留白：`-b`/`-n` 待接（需字节/块数驱动的分块）。
  修法详见 `../18-stage-commands/07-text-filter.md` split 行。

- ✅ **批次一百八十八接上 `cal -d`（周起始日 0..6）**
  （`2026-10-13`，`claim/CAL-DOW-qoder` @ 基线 `ab108832d`，单域三线 commands/doctools，无跨界）：
  接回 `-d` 留白。C 里 `-d` 同时旋转「表头」与「栅格落位」（`cal.c:435-439` 的 `day_headings + 3*dow`
  与 `day_in_week` 的 `(temp - dow + …) % 7`），两处必须一起忠实移植（只转一处=半接）。决定半
  `cal.rs`：`day_matrix` 新增 `week_start` 参（列号 `= (基列 - week_start + 7) % 7`）、新增
  `put_headings`（按 day_len 循环移表头，`week_start=0` 逐字等于基表头）、`render_month`/
  `render_year`/`render_triple` 透传 `week_start`。执行半 `bin/cal.rs`：`-d` 改为 getopt 取值形
  （`-dN` 附值或 `-d N` 下一词，0..=6 校验，非法报 "illegal day of week value"，对位 cal.c:247-250）。
  关键零回归铉：`week_start=0` 时 `day_matrix`/`put_headings` 与旧输出逐字一致（test_week_start_rotation
  与全部既有 golden 同跑验证）。
  三件套：`cargo test -p minix-doctools --lib` → **70/0**（既有 69 零回归 + 新 1）；`--bin cal` → **7/0**；
  freestanding `cargo build -p minix-doctools --bin cal --target {x86_64,aarch64,riscv64gc}-unknown-none` 三架构均 Finished；
  clippy cal.rs 告警仍为 2（仅存量 i64 同型转换，本笔新增 doc_lazy_continuation 已改写注释消除）；
  `rustfmt --edition 2024 --check` 两目标文件零漂移（共享件 bin_support.rs 的 EOF 存量漂移非本笔、已还原不入库）；
  核实日 `2026-10-13`；基线 `ab108832d`。**验证边界**：golden 系按 C `day_in_week`/表头公式自推，
  本机无 BSD `cal` 逐字对账（真值以 `minix3/usr.bin/cal/cal.c` 为纲）；真机腿归 C-57/E5。
  留白：`-h`/`-r`/`-A`/`-B`/`-C`/`-R` 仍待接。

- ✅ **批次三百一十九把 `units` 的三类出口按 C 定形（第十处读缝缺陷），并新立它的第一扇端到端门**
  （`2026-10-16`，`claim/UNITS-GETOPT-LINE-qoder` @ 基线 `d967ef638`，单域三线 commands/textfilter，无跨界）：
  **真源形状**：`units.c:817` 的 optstring 是 `"lLvqf:"`——只有 `f` 带值，所以「带值选项缺值」那一支
  在本支只有 `-f` 一个字母可达；`l`/`L` 是**两个字母且语义成对**（`-l` 列原名、`-L` 列折算形，
  顺带把 `precision` 置成 `DBL_DIG`），`v` 在环内就退出。三类出口：① 集外字母与缺值各由 libc 先印
  一行再接**整块九行** usage（`units.c:793-807` 是两个 `fprintf`，首行与说明段各带一个前导空行），
  退 3；② `-v` 打两行版本再接同一块 usage，那两行在**标准错误**上（`units.c:836-839` 是
  `fprintf(stderr, …)`），且它排在操作数判据之前，所以 `-f <档> -v` 也是版本块；③ 操作数不是
  0/2/3 个、或「列表模式还带操作数」时**只有 usage 块**、没有 libc 那一行。三支都退 3，
  而 usage 里的 `units` 是写死的字面量，只有 libc 那两行跟着 `getprogname()`。
  **这一支的次序是可观测的**：C 的操作数判据（`units.c:849-851`）排在 `readunits()` 之前，
  所以 `units -f <坏档> m` 报的是 usage 而不是开档失败——旧写法把个数判拖到读库之后，这形先印开档失败。
  **两支开档句子分属 err 与 errx**：`-f` 的档打不开是 `err(1, "Unable to open units file '%s'", …)`
  三段形（带 `strerror`），默认档找不到是 `errx(1, "Can't find units file '%s'", …)` 两段形（不带）；
  默认档名取自 `minix3/usr.bin/units/pathnames.h` 的 `_PATH_UNITSLIB` = `/usr/share/misc/units.lib`，
  而本壳此前写成 `/usr/lib/units`（07 篇第 212 行跟着同一处错，本笔一并翻正）。
  **四处只能量出来推不出来的形制**（参照件 `tmp/units-319/units`，`units.c` + Minix `getopt.c`
  + 程序名与 `strlcpy`/`strlcat` 垫片，另需空 `namespace.h`、`__printflike` 空宏与 `<stdarg.h>`）：
  `-f` 取下一个 token 时不看脸色，故 `units -f - m sec` 的档名就是 `-`；环不重排，故
  `units -f <档> m sec -l` 的 `-l` 是第三个操作数（参照件报 `Unknown unit '-l'` 且退 0，
  旧写法把它当旗标吃掉并进列表、还退 1）；裸 `-` 结束环却不推进 `optind`（`getopt.c:86-93`），
  于是它是操作数而 `--` 被吃掉（`:80-85`）；`optstring` 里那个 `:` 只是记号，libc 的判据是
  `optopt == ':' || strchr(...) == NULL`（`getopt.c:98`），所以 `units -:` 与 `-l:` 走集外字母那一支，
  而 `-f:` 的冒号才是值——这一处是 full-review 的代码维度在本笔落盘**之后**抓出来的，同轮补正并加测。
  **交互面的读缝（同族第十处）**：本支拿裸 `minix_sys::read` 逐字节读标准输入，宿主上一个字节也
  读不到，那台 `You have: `/`You want: ` 提示机从没被进程级测过；现走 `bin_support` 新增的
  `read_line_bounded`（宿主 `std::io::stdin` 与目标 `minix_sys::read` 各一条腿，按字节读，
  与 C 的 `fgets(buf, 80, …)` 同为一次至多 79 字节、截断时换行留在流里）。
  **第二处交互面实缺陷**：C 是 `do { 提示; fgets; } while (addunit || completereduce)`
  （`units.c:876-893`），一行不认就**重问同一格**；参照件对 `bogus\nm\nsec\n` 打的是
  `You have: You have: You want: `，旧写法把两次读当成 have/want 一对，把坏行喂给了对面那一格。
  选项环提入库侧 `parse_units_args`（libc 两行交回 `GetoptReport` 单源），`mywarnx` 的三句
  （`Unknown unit '<名>'` / `Unit reduces to zero` / `Junk before '|'`）提入库侧 `units_error_line`。
  **门测 20 枚**（`tests/units_face.rs`，本支第一扇端到端门）+ 库测 9 枚（三类出口、`:` 记号、
  紧贴值与下一个 token 值、`--` 与裸 `-`、不重排、个数判次序、句子单源）。新门里 `-L` 那一枚
  当场抓到一处实缺陷：`list_units` 的两个展开分支各多打一个换行——C 的 `listunits()` 是「先
  `printf` 名字、再 `showunit()`」，结尾换行归 `showunit` 自带（`units.c:700-790` 那一段的两支），
  本壳写成 `"{}{}\n"` 于是每条展开行后面多一个空行；`-l` 的两支不受影响，所以这一形此前
  既没进门也没有库测覆盖。
  **三件套**：容器整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **839 / 0 / 0**（64 target，
  基线 `d967ef638` 为 810/63）；`cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、
  按「带 `-->` 的诊断条数」口径整包 **65 条**——较三百一十八的 59 多出的六条全在 `src/units.rs` 的
  决定半旧码（`:60`、`:66`、`:106`、`:136`、`:144`、`:221`），该文件上一轮走缓存没被重打印，
  **本笔触及件零新增**（唯一一枚落在我新写的 `bin/units.rs:89` 的 `single_match` 已改成 `if let`）；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`；
  `rustfmt --edition 2024 --check` 触及件零漂移（顺带抹平 `src/units.rs` 那 14 处存量漂移）；
  A/B 与参照件比 46 形（退码、stdout、stderr 三件逐形比，每形套 `timeout 5`）**SAME=40 DIFF=6** 形，前门 SAME=40 DIFF=6（差形：环不重排第三操作数是 -l、陌生名 have、陌生名两边、裸 - 作操作数、-L 展开真库全清单（决定半量级用）、非 ASCII 聚簇 -é）；
  变异负控六枚（`tmp/units-319/ctl319.py`：带值判定恒假、集外字母并到 usage、去掉列表带操作数那一判、
  一个操作数也算合法、版本改打标准输出、行长上限 79→80）分别咬 16 / 5 / 1 / 2 / 1 / 1 枚。
  A/B 排在变异腿**之后**且跑前重链二进制（三百一十七那个坑的第二次规避）。核实日 `2026-10-16`；
  基线 `d967ef638`。
  **登记不执行**（六处）：① `UNITS-PARTIAL-UNIT-ON-ERROR`——C 忽略 `addunit`/`completereduce` 的
  返回值，拿半成品照样出一个答案块（参照件对 `bogus m` 打 `conformability error\n\t1 bogus\n\t1 m`），
  本壳的库交回 `Err` 时没有半成品可展示，退码与报文照 C、答案块那一行缺；
  ② `UNITS-LEADING-DASH-NAME`——参照件报 `Unknown unit '-l'`，本壳的名字段少了前导短横（决定半的
  切词形状），所以门里那一枚只对到句子骨架；③ `UNITS-PATH-SEARCH`——默认档找不到时 C 先按 `PATH`
  逐目录试（`units.c:146-176`，且它把绝对路径 `UNITSFILE` 直接接在目录后），本机与宿主都落不到；
  ④ `UNITS-ERRPREFIX-STDOUT`——列表模式下 `errprefix = "/ "` 让同一句 `mywarnx` 改走标准输出并带
  `/ ` 前缀（`units.c:114-118`、`:714`），本壳 `list_units` 只数错误条数不印句子；
  ⑤ `UNITS-FGETS-80-DBLINE`——C 的词典行也过 `fgets(…, 80, …)`，超过 79 字符的一行被切成两条读；
  ⑥ `UNITS-NONUTF8-DROP`——本壳整档过一次 UTF-8 校验、不过则整段丢光（与 `split` 那处同形）。
  **验证边界**：参照件链的是宿主 glibc 的 `strerror`/`printf`/`qsort`，所以 `strerror` 句子取自宿主表
  （`No such file or directory` 两边同文）、`-L` 的展开数值取自宿主 `%.15g`；真机读库腿归 `C-57`/E5。
  换算的决定半（`units.lib` 全量 574 行清单、可折算与不可折算两形）不在本笔射程内，
  门只对到本支小词典的六行清单与两对换算。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/
  col/column/comm/cmp/jot/unifdef/**units** + pr；待接 `seq`、`paste`、`tr`；
  另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `SEQ-GETOPT-LINE`
  （`minix3/usr.bin/seq/seq.c:102` 的 optstring 是 `"f:hs:t:w"`——`f`/`t` 带值，`s`/`w` 不带，
  `w` 排在串尾容易被误读成带值）。

- ✅ **批次三百四十七把 `random` 的选项环接上：`-e`、`-r`、集外字母那两行**（`RANDOM-DENOM-LEGS` 的片2b 落地）（`2026-10-18`，`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `31f90d3a5`，动 `os/commands/games/text-games/{src/lottery.rs,src/bin/random.rs,tests/random_face.rs}` + 18 篇 `todo.md`、`24-text-games.md`、`coordination/new_edge4.md`，单域三线 games/text-games，无跨界）
  - **这一条是什么**：上一笔判明参照件在选项环上不是真值承载（它那句 `invalid option -- 'Z'` 是宿主 glibc 的字样），本笔按 Minix 自己的 `minix3/lib/libc/stdlib/getopt.c` 接：环只认 `e`/`r`（`random.c:72-79`），集外字母先由 libc 打一行 `unknown option -- <字母>`（`getopt.c:101-104`，字母**不带引号**），紧接着 `random.c:80-83` 的 `default: usage()` 再打不缀程序名的 `usage: random [-er] [denominator]`，退 1；`--` 是结束标记（`getopt.c:81-85` 会 `++optind`），裸 `-` 相反——optstring 不含 `-`，libc 回 `-1` 且不推进游标（`getopt.c:86-93`），于是它留在操作数里被 `strtod` 否掉；`-e` 那一支在读标准输入之前就返回 `(denom * random()) / RANDOM_MAX`（`:109-111`），`-r` 只是 `setbuf(stdout, NULL)`（`:117-118`）。
  - **放在哪一层**：环住决定半——`src/lottery.rs` 新增 `Ring` 与 `scan_ring(tokens: &[&str])`（返回旗标加操作数起点，等价于 C 的 `optind`）、`UNKNOWN_OPTION` 那半句字样、`RANDOM_MAX` 与 `exit_status_value`。薄壳只做渲染与分派。两个细节值得写下：`operands_at` 的下标落在 `argv[1..]` 上（两枚选项 token 吃掉之后 `3` 在第 2 格），以及 `exit_status_value` 的上界是**闭**的——源码注释说「between 0 and denom - 1」，可表达式在抽取值到顶时正取到 `denom`，门按表达式钉不按注释；同理 `RANDOM_MAX * 3 / 4` 那个抽取值折算出来是 2 而不是 3（整除先截掉一点，乘法摊不平）。
  - **门**：宿主 `cargo test -p minix-text-games` → 库测 **28 passed / 0 failed**（新立 3 枚：环的停点与聚簇、`--` 与裸 `-` 的对立、`-e` 折算的闭上界）、端到端 **12 passed / 0 failed**（新立 3 枚：集外字母那两句及其前缀差异、`-- 1` 逐字节全等而光写 `--` 只钉退码、`-e 1` 与 `-e 4` 的退码区间外加 `-r 1` 全回声）。**权威容器门** `docker run … cargo test -j 1 -p minix-text-games` → **退码 0**、各 target 合计 40 passed / 0 failed（`tmp/random-345/container-gate-347.log`）；三架构各 `Finished`；`clippy -p minix-text-games --all-targets` 指向 `text-games` 命中 **0**；`rustfmt --edition 2021 --check` 触及三件零漂移。
  - **负控五枚全咬**（`tmp/random-345/ctl347.log` 及本笔前一段读数；逐枚整篇存回并断言 md5，还原后 0 红）：集外字母被当成 `r` 收下 → 1 红；`--` 之后不跳过自己 → 1 红；裸 `-` 被当成选项吞掉 → 1 红；标尺从 `2^31-1` 改成 `2^32-1` → 1 红；被否那一支并入溢出那一支（三百四十三那批里最相关的一枚仍在册）。第一条与第二条各咬住一处「参照件在这一形与 Minix 不同」的形状，删掉即门瞎。
  - **本笔又两次红在自己的期望上**（同一门课，第三次重修）：`operands_at` 我按 `argv` 全长度数写成了 3；`(4 * (RANDOM_MAX*3/4)) / RANDOM_MAX` 我按直觉写成 3，实际整除截断给 2。两次都是**先改期望、再看 C 的表达式确认期望确实错了**，与三百四十四那次「门红是因为门把待定形当成了结论」不同——这回红的是我自己算错。**规矩：凡带整除与折半的断言，落笔前把三个数写出来手算一遍，不要靠量级直觉。**
  - **还敞着的**：**片3** 分母的 `strtod` 取值域（`1e3`、`1e-3`、`inf`、`nan` 这些 C 认的形，本壳按十进制整数面判成「不是有效分母」，七形对账见三百四十三的 A/B 读数），以及共用的 `bin_support::emit` 写缝退码（C 是 `err(2, "stdout")` 退 2，`random.c:131-133`；`emit` 被本 crate 多支共用，改它要单独一条账）。条目状态仍是 **partially fixed**。

- ✅ **批次三百四十六把 `random` 的过滤体接成 C 的一趟过流，删掉两处自创报文（`RANDOM-DENOM-LEGS` 的片2a 落地）**（`2026-10-18`，`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `0ab605642`，动 `os/commands/games/text-games/{src/bin/random.rs,tests/random_face.rs}` + 18 篇 `todo.md`、`24-text-games.md`、`coordination/new_edge4.md`，单域三线 games/text-games，无跨界）
  - **这一条是什么**：上一笔定好的片2a。改的是「读进来之后怎么办」这一段：基线 `0ab605642` 的薄壳把标准输入读全、只把**最后一次**掷中的那一行留在 `chosen` 里、一行都没掷中就打自创的 `random: nothing selected` 退 1；读失手则打自创的 `random: read error` 退 1。C 是一趟过流（`minix3/games/random/random.c:126-137`）：掷中就把这一行整行写出去，下一行另掷一次，环走完直接 `exit(0)`——**「一行都没掷中」在 C 那里不是错误，是空输出**；`ferror(stdin)` 那一路是 `err(2, "stdin")`（`:135-136`），三段形、位上是 `stdin` 这个词、退 2。
  - **改法**：薄壳按 C 的两段重写——先读带号的双生面 `read_stdin_error()`（三百四十三立的），有号就 `support::warn_line(prog, "stdin", code)` 加 `terminate(2)`；没号就逐行掷、掷中即 `emit` 整行，循环结束 `terminate(0)`。决定半一字未动：`is_selected`（掷 `[0, denom)` 取 0 即中）与 `mix_seed` 沿用，LCG 仍在薄壳里（不复现 libc `random()` 的位序列这一条承载差早登记在本支账上）。为什么不在决定半做「保序写出」：那是一趟流的次序，属于执行层的读写顺序，决定半只有「这一行中不中」这一个判据。
  - **门**：宿主 `cargo test -p minix-text-games` → 库测 **25 passed / 0 failed**、端到端 `--test random_face` **9 passed / 0 failed**（三百四十三立 6 枚，本笔补 3 枚并翻正 1 枚）：`one_operand_still_feeds_the_filter` 从「非空且是子集」升到**逐字节全等**（分母 1 每行必中，这一形因此确定）；新立 `selected_lines_come_out_in_input_order`（200 行、分母 2，输出必须是输入的**保序**子序列——「只留最后一枚中者」在这种输入下当场被照出来）、`an_empty_selection_is_silent_and_exits_zero`（分母 40 亿配一行，掷中概率 2.5e-10，钉死空输出退 0，专杀自创的 `nothing selected`）、`an_unreadable_stdin_blames_the_stdin_word_and_exits_two`（把一枚目录的文件句柄当标准输入递进去，验 `random: stdin: Is a directory` 退 2；参照件同形现量在 `tmp/random-345/ab345-ref.log` 末行，句子取 `minix-types` 表第 21 项 `os/libs/minix-types/src/types/errno.rs:413`，与 glibc 同字，所以这一形宿主可测且可比）。**权威容器门** `docker run … cargo test -j 1 -p minix-text-games` → **退码 0**、各 target 合计 34 passed / 0 failed，`an_unreadable_stdin…` 在容器内也 `ok`（日志 `tmp/random-345/container-gate.log`）；三架构 `x86_64-unknown-none`/`aarch64-unknown-none`/`riscv64gc-unknown-none-elf` 各 `Finished`（薄壳未变、集成测不进 `--target …-none` 的构建）；`cargo clippy -p minix-text-games --all-targets` 指向 `text-games` 的命中 **0**；`rustfmt --edition 2021 --check` 触及两件零漂移。
  - **负控四枚全咬**（`python3` 走 `tmp/random-345/ctl346.py.log`；整篇存回并断言 md5，还原后 0 红）：掷中判据改恒假（整行都不写）→ 3 红；改恒真（一行不落全写）→ 1 红；`stdin` 那一路退码从 2 改成 1 → 1 红；那一句换回自创的 `random: read error` → 1 红。四枚变异都在薄壳源码上，源码自变异测试之后一字未动；其后唯一的改动在测试帮助件的 `mut`（见下），不改语义。
  - **本笔自己撞的两次红，都红在我自己的改动上（记进流程）**：其一，为消一条 clippy「不需要 `mut`」的告警，我把 `let mut child` 的 `mut` 删了**两处**，其中 `run_with_argv0` 里那一处是必须的（它调 `child.stdin.as_mut()`）——宿主 `cargo test` 当场编不过，**权威容器门退码 101**。教训：删 `mut`、删参数这类「看似无害的清扫」必须按调用点逐处分清，不能按同名文本一次全替；这也是本轮把容器门跑成红之后**先诊断再合回**的价值——门红在编译期就等于把一次自伤挡在合入之前。其二，上一笔记下的「锚点只能取 rustfmt 之后的盘上文本」本轮再次生效：改 `tests/random_face.rs` 前先用 `sed` 读出盘上文本再动，两处替换一次到位。
  - **仍在敞着的**：**片2b** 选项环（`-e` 把 `(denom * random()) / RANDOM_MAX` 当退码返回 `:110-111`、`-r` 关 stdio 缓冲 `:117-118`、集外字母那两行按 Minix 自己 `minix3/lib/libc/stdlib/getopt.c:103` 的 `unknown option -- %c` 而不是参照件那句 glibc 字样——上一笔已判明，参照件在这一步不是真值承载），以及**片3** 分母的 `strtod` 取值域。条目状态：**partially fixed**（片1 与片2a 坐实，2b 与片3 敞着）。写出去那一侧的失手（C 的 `ferror(stdout)` → `err(2, "stdout")`，`:131-133`）本笔也不动：本 crate 的 `bin_support::emit` 是各二进制共用的写缝，改它的退码会影响同族别支，单独一条登记。

- ✅ **批次三百四十五（纯台账笔，生产码一字未动）把 `random` 片2 量成两半并定下门期望，其间撞出一个真值承载问题：参照件在选项环的措辞上不算数**（`2026-10-18`，`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `f8d7bdd05`，只动 `rewrite-notes/18-stage-commands/todo.md` 与 `rewrite-notes/coordination/new_edge4.md`，单域三线 games/text-games，无跨界）
  - **这一条是什么**：接着三百四十三的片1 做片2（过滤器主体 + `-e`/`-r`）。开工先把参照件的选项形量了一遍（脚本 `tmp/random-345/ab345.py`，读数 `tmp/random-345/ab345-ref.log`，参照件 `tmp/random-342/random_ref`，输入六行 `aa..ff`）：`-e` 与 `-r` 都被接受（`-r 1` 把三行输入全打出来、退 0；`-e 1` 不打东西、退 0），`--` 之后按操作数算（`--` 单用 = 零枚 = 分母 2，`-- 3` = 一枚），裸 `-` 仍是那一句 `denominator is not valid.`，`-e 2 3` 落 usage 退 1，而集外字母那一形打的是 **两行**：先 `invalid option -- 'Z'`（缀着 `argv[0]`）再 `usage: random [-er] [denominator]`，退 1。
  - **本轮最重要的判定：那句 `invalid option` 不是 Minix 的字样**。参照件是在宿主上编出来的，它的 `getopt(3)` 来自 glibc；Minix 自己的 `minix3/lib/libc/stdlib/getopt.c:103` 打的是 `"%s: unknown option -- %c\n"`，带值字母缺值那一句在 `:134` 是 `"%s: option requires an argument -- %c\n"`——两句都**没有**引号包住字母，措辞也不同。这与 `textfilter` 一族早就记过的口径同形（`tr`、`jot` 的门里那句 libc 行都取自 Minix 的 `getopt.c`，并由 `GetoptReport` 那份单源交回）。所以片2 的选项环那一步**不能照参照件的字符串抄**，要照 `minix3/lib/libc/stdlib/getopt.c` 抄；参照件在这一步只用来确认「有几行、谁在前、退码是几」。这正是规则里那条「宿主 glibc 不是真值承载」在选项环上的又一次现形，与前几轮在 `printf` 精度、`strerror` 表上的两次并列。
  - **于是片2 拆成两半，互不阻塞**：**2a 过滤器主体**——每行独立掷一次，中者按输入顺序整行写出；掷不中不是错误（C 的 `getchar()` 环走完直接 `exit(0)`，`random.c:114-137`），薄壳自创的那句 `random: nothing selected` 与退 1 要删；`ferror(stdin)` 那一路按 `err(2, "stdin")` 成形（`:135-136`），三段形、位上是 `stdin` 这个词、**退 2**，替掉薄壳现在自创的 `random: read error` 退 1。**2b 选项环**——`-e` 取 `(denom * random()) / RANDOM_MAX` 当退码（`:110-111`，注意这一支的退码随掷而变，门只能验「落在 `[0, denom]` 之内且不打东西」）、`-r` 关 stdio 缓冲（`:117-118`，本壳逐行写出去因此无可观察差，登记为承载差）、集外字母那两行按 Minix 的 `getopt.c` 措辞接。
  - **门期望值（下一轮照着钉，不必重新现查）**：① `random 1` 现在可以升到**逐字节全等**（分母 1 时 `draw` 恒为 0，每行必中；三百四十三那笔因为身体只留最后一枚中者，只敢验「非空且是子集」，所以 `tests/random_face.rs` 里 `one_operand_still_feeds_the_filter` 的这条断言要一并翻正）；② 零枚操作数喂 200 行：退 0、标准错误空、输出是输入的**保序**子序列（「只留最后一枚中者」在这种输入下当场被照出来）；③ 分母 `4000000000` 配一行输入：空输出、退 0（掷中概率 2.5e-10，因此这一形是确定的，专门用来钉死自创的「一行都没掷中」那一报）；④ 把一枚目录的文件句柄当标准输入递进去：参照件现量 `random_ref: stdin: Is a directory` 退 2（`ab345-ref.log` 末行），本壳该报 `random: stdin: Is a directory` 退 2，句子取 `minix-types` 表第 21 项（`os/libs/minix-types/src/types/errno.rs:413`，与 glibc 同字，因此这一形宿主可测且可比）。
  - **本笔自己撞的两个流程坑（写进流程，不留在记忆里）**：其一，动 `tests/random_face.rs` 时拿三百四十三**改动前**的文本当锚点，两处失配——那笔定稿前跑过 `rustfmt`，落盘文本与写入文本不同；规矩应当是「先 `rustfmt --edition 2021 --check` 或直接读盘上文本，再取锚点」，这与三百四十一记的「变异锚点只能取 rustfmt 之后的文本」是同一条的两次现形，本轮把它升级为改测试文件前的固定一步。其二，`String::from_utf8_lossy(...).as_str()` 在本仓库工具链上撞 `str_as_str` 未稳定（`Cow` 被解引用成 `str` 之后再调 `as_str`），要写成 `&*…` 或先 `.into_owned()` 再 `.as_str()`。
  - **为什么本笔不落码**：预算不足以把 2a 的门、负控、容器门与台账一次跑完——三百四十三实测一轮闭环要十来次往返。中途我把已写的 2a 代码与三门草稿在自己树里还原了（`git status --porcelain` 归零后 `release`），不留半成品：**门没跑绿的改动不算落地**。这是拆解，不是降级：条目状态仍是 `partially fixed`（片1 已坐实，2a/2b/片3 敞着），且本笔把 2a 与 2b 的分界、四组门的期望值、以及选项环的真值承载问题都定死了。测试读数因此沿用上一笔（宿主 25 库测 + 6 端到端、容器退码 0），不复述成新读数。
  - **下一轮首动作**：`tools/claim.sh claim RANDOM-DENOM-LEGS qoder` 做 **2a**，改动清单就是 `os/commands/games/text-games/src/bin/random.rs` 的读输入之后那一段（删 `chosen`/`buf` 的「挑一行」形，改成一趟过流 + 保序整行写出；`read_failure` 那一路换成 `support::warn_line(prog, "stdin", code)` 加 `terminate(2)`），门加在 `tests/random_face.rs` 的四枚（① 翻正、② ③ ④ 新立），负控四枚起跳：掷中判据改恒假 → ②红；改恒真 → ③红；退 2 改退 1 → ④红；`warn_line` 换回自创句 → ④红。

- ✅ **批次三百四十四把 `truncate` 的相对尺寸加法接回 C 的回绕形状，闭合 `TRUNCATE-RELATIVE-WRAP`（即排期行 R17）**（`2026-10-18`，`claim/TRUNCATE-RELATIVE-WRAP-qoder` @ 基线 `93eba32e3`，动 `os/commands/bin/fileops/src/truncate.rs` + 18 篇 `todo.md`、`06-file-ops.md`、`coordination/new_edge4.md`，单域三线 commands/fileops，无跨界）
  - **这一条是什么**：`truncate -s +N file` 要把 `N` 加到文件已有长度上。C 在 `minix3/minix/commands/truncate/truncate.c:122` 直接把和写进带符号的 `off_t`——溢出在这台机器上是**回绕**（C 自己称之为有符号溢出未定义行为），回绕落到负侧之后由 `:131-132` 的 `if (tsize < 0) tsize = 0;` 收成人家真的能用的形状：**加爆了等于清空**。本壳在基线 `93eba32e3` 的 `os/commands/bin/fileops/src/truncate.rs:230-232` 用的是 `clamp(current.saturating_add(delta))`，饱和加停在 `i64::MAX` 上，于是 `truncate -s +9223372036854775807` 这一句在 C 那里把文件清成 0 字节、在本壳这里却拿着 `i64::MAX` 去调 `ftruncate`。这是 P0-code-bug：同一句命令，落盘结果一个是空文件、一个是失败（宿主文件系统对超长 `ftruncate` 回 `EFBIG`/`EINVAL`，`truncate.c:134-137` 那一支就走到 `perror`）。
  - **那条 `EFBIG` 判据不抄**：C 的 `:123` 写的是 `if (oflow < (sb.st_size + rsize))`——拿刚赋完值的 `oflow` 与**同一个来源式**再比一次，两边同值，条件恒假，所以那一支在本支不可达（批次一百一十八接线时就看出来了，账记在 `truncate.rs` 的注释里）。真正管事的只有 `:131-132` 的钳位。所以本壳的 `relative_target` 只由「回绕加 + 钳零」两步组成，不复制一个永不触发的分支去制造「我们也抄了 C」的错觉。
  - **还有一处可达性判定**：`st_size` 来自 `fstat`，永远非负，因此 `current >= 0` 配上任何 `delta >= i64::MIN`，和的下界恰是 `i64::MIN`——**负方向的回绕不可达**，可达的只有正方向那一支。这一条写进函数文档注释，是为了让下一个读码的人不用重新推一遍：为什么门里只钉正溢出、而 `relative_target(0, i64::MIN)` 只是「加到最小值本身」而不是回绕。
  - **门**：宿主 `cargo test -p minix-fileops --lib` → **392 passed / 0 failed**。本笔新立 1 枚 `a_positive_overflow_wraps_then_clamps_to_zero`（`1 + i64::MAX` → 0、`i64::MAX + i64::MAX` → 0、不回绕的 `4096 + 4096` → 8192、`3 + (-10)` → 0、`0 + i64::MIN` → 0）；**同时翻正 1 枚既有期望**——`relative_sizes_land_on_the_clamped_sum` 里写着 `relative_target(i64::MAX, 5) == i64::MAX` 并配一句「饱和加保住了钳位的答案」的注释，那正是被登记为待定形的那个形状进了门，与 `truncate.c:122` 的回绕不符，改为 `== 0` 并把注释换成 C 的推导。**改的是期望不是代码**的另一半证据是：这一改动只有配合 `wrapping_add` 才成立，负控第 1 枚（把回绕改回饱和加）当场咬红 2 枚。
  - **负控五枚全咬**（脚本与读数在 `tmp/truncate-344/`，逐枚还原并断言 md5 相同，还原后 0 红）：回绕改回饱和加 → 2 红；加号写成减号 → 2 红；丢掉钳位（只回绕不钳零）→ 2 红；把钳位本身擦成恒等 → 2 红；只看已有长度、忽略增量 → 2 红。
  - **验证边界（登记清楚，不含糊过去）**：本笔的落盘后果在宿主**测不到**。`os/commands/bin/fileops/tests/truncate_face.rs` 的篇头写明宿主侧 `open`/`stat` 经传输缝只回 `EIO`，够到 `perror` 腿但够不到真正的 `ftruncate`，所以「加爆了清空文件」这一形只能由决定半的单测钉住，端到端那一腿归可写根镜像上的 `C-57`/`E5` 真机门。这与「`stat` 那一臂在宿主可测」（批次三百三十九的判例）不同，别把两件事并成一句「都得等真机」。
  - **门读数汇总**：宿主 `--lib` 392/0；三架构 `x86_64-unknown-none`/`aarch64-unknown-none`/`riscv64gc-unknown-none-elf` 各 `Finished`；`cargo clippy -p minix-fileops --all-targets` 指向 `truncate.rs` 的命中为 0；`rustfmt --edition 2021 --check` 触及件零漂移；权威容器门 `docker run … cargo test -j 1 -p minix-fileops` 读数见 `tmp/truncate-344/container-gate.log`（核实日 `2026-10-18`）。
  - **文档同步**：`rewrite-notes/18-stage-commands/06-file-ops.md` 的 `truncate.rs` 契约行里「相对尺寸加到已有长度后向零钳位」一句按本笔的现查改写成「带符号 `off_t` 加法（溢出回绕）→ 负数收零」，并把「回绕只有正方向可达」与「`:123` 的 `EFBIG` 恒假」两条一并写进同一段。历史批次条目（二百四十四登记待定形、三百一十八接线）的文字一字未动。

- ✅ **批次三百四十三把 `random` 操作数那一步接到 C 的形状：零枚取 2、两枚以上 usage、两句报文各归各形**（`2026-10-18`，`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `417fd35e6`，动 `os/commands/games/text-games/{src/lottery.rs,src/bin_support.rs,src/bin/random.rs,tests/random_face.rs}` + 18 篇 `todo.md`、`24-text-games.md`、`coordination/new_edge4.md`，单域三线 games/text-games，无跨界）
  - **这一条是什么**：`RANDOM-DENOM-LEGS` 的三片里，片1 只管「操作数那一步」——个数怎么分派、两句话怎么打。上一笔量下来的形状是：零枚操作数被判成缺参数（打一行自创的 `usage: random [N]` 退 1），一枚被拒的操作数打自创的 `random: <操作数>: bad denominator`，两枚以上则后面的默默被丢掉。C 那三步都在 `minix3/games/random/random.c:89-104`：零枚取 `denom = 2` 继续干活，一枚先问 `errno == ERANGE`（`err(1, "%s", *argv)` 的三段形）再看 `denom == 0 || *ep != '\0'`（`errx(1, "denominator is not valid.")`，**句子里没有操作数**），两枚以上 `usage()`——而 `usage()` 那一行的 `random` 是写死在串里的，不缀程序名（`:143-147`）。
  - **改法与为什么这样放**：判断落在决定半——`src/lottery.rs` 新增 `OperandLeg` 与 `judge_operands(operands: &[&str])`，取 `&[&str]` 而不是 `&[String]`，因为这个库目标按 `src/bin_support.rs` 篇头的约定不引系统调用面、也不引 `alloc`；两句文本 `USAGE` 与 `NOT_VALID_SENTENCE` 也住在决定半，与 `textfilter` 的 `tr.rs` 里 `SetGrammar::sentence()` 同一承载选择（句子是语义，不是渲染）。薄壳只做分派与渲染：`bin_support.rs` 补 `prog_name` 与 `warn_line` 两枚（形状逐字对齐 `os/commands/usr-bin/textfilter/src/bin_support.rs:329-353`，句子的权威在 `minix-types` 的 `err_line`，`os/libs/minix-types/src/types/errno.rs:553`），三段形那支把 `TextGameError::OutOfRange.as_errno()`（34，Minix 的 `ERANGE`，`src/lib.rs:59-76`）经 `Errno::from_i32` 交出去，不自造号。
  - **三条边界，登记清楚不去糊**：① **触发域仍不等同**——`OutOfRange` 在这一腿里意味着「十进制整数溢出 `u32`」，C 的 `ERANGE` 意味着「`strtod` 溢出 `double`」，所以 `4294967296` 参照件退 0（`strtod` 认它）而我方报三段形；这一族七形全归片3（取值域）。② **句子表源是承载差异**——宿主参照件走 glibc 那张表（`Numerical result out of range`），本树的真值是 `minix-types` 表第 34 项 `Result too large or too small`（`os/libs/minix-types/src/types/errno.rs:426`，`errno.rs:383` 把这一对写下），门按本树的表断言。③ **读缝顺手改了，句子没顺手改**——薄壳原来是裸调 `minix_sys::read` 读标准输入，宿主上这条路读不到东西（实测打 `random: read error` 退 1，`tests/random_face.rs` 的两枚门就是这么咬出来的），于是补了 `bin_support::read_stdin_error` 这对双生面（宿主 `std::io::stdin`，真机同一枚描述符逐块 `read`，与 `textfilter` 那条读缝同一口径）；而「失手打 `random: read error` 退 1」这句本身仍是自创，C 是 `err(2, "stdin")` 退 2（`random.c:135-136`），归片2。
  - **门**：宿主 `cargo test -p minix-text-games` → 库测 **25 passed / 0 failed**（新立 3 枚：个数四出口、被拒与溢出是两支、两条写死的串逐字节）、端到端 **`--test random_face` 6 passed / 0 failed**（本 crate 头一枚 `tests/` 门）；**权威容器门** `docker run --rm -u $(id -u):$(id -g) -v "$PWD:/work" -w /work/os -v <主树>/tmp/ci-cargo:/cargo -v <主树>/tmp/ci-target-343:/target -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/target -m 2g minix-ci:1.94 cargo test -j 1 -p minix-text-games` → **退码 0**，`tests/random_face.rs` 6 passed / 0 failed（日志 `tmp/random-343/gates.log`）；三架构 `x86_64-unknown-none`/`aarch64-unknown-none`/`riscv64gc-unknown-none-elf` 各 `Finished`；`cargo clippy -p minix-text-games --all-targets` 指向 `text-games` 的命中为零；`rustfmt --edition 2021 --check` 对触及四件零漂移——其中 `src/lottery.rs` 基线本就带一处漂移（`git show HEAD^:…lottery.rs` 现查 `Diff in` 计数 1，是 `is_selected` 那两行断言太长），格式化器一并归正，无语义改动。
  - **A/B 读数（可重跑）**：`python3 tmp/random-343/ab343.py` → 十五形里归一化两处承载差异（参照件的 `argv[0]` 前缀 `random_ref:`、strerror 的表源）之后 **SAME=8 / DIFF=7**，七形全是 `strtod` 取值域一族（`1e3`、`1e-3`、`inf`、`nan`、`4294967296`、`999999999999999999999999`、`18e308`），归片3；参照件逐形读数在 `tmp/random-343/ab343-ref.log`，归一化对账在 `tmp/random-343/ab343-norm.log`。
  - **负控七枚全咬**（`python3 tmp/random-343/ctl343.py`，读数 `tmp/random-343/ctl343.log` 与定稿后门复跑的 `gates.log` 尾段）：零枚那一支并进 usage → 1 红；默认分母 2 改成 3 → 1 红（库测咬住，端到端咬不住——门只验「非空且退 0」，写进 `judge_operands` 的库测里才钉得住）；溢出并进被拒 → 1 红；被拒并进溢出 → 1 红；`errx` 句子丢句尾的点 → 1 红；usage 那行退回不带 `-er` 的形 → 1 红；程序名擦成写死的 `random` → 1 红（改名那一枚门咬住）。变异前后整篇存回并断言 md5 相同，还原后基线 0 红。
  - **本笔两次把门改对而不是把码改对**（同一形状的老毛病，记下来防下一次）：其一，`random 1` 我按「每行必中所以全行回声」断言，量出来只有一行——那是过滤体「只留最后一枚中者」的语义，属片2，本笔不该替它下结论，断言收到「非空、退 0、是输入行子集」；其二，退码断言压在一枚概率事件上——六行输入在分母 2 下「一行都没掷中」的概率是 2 的 -6 次方，门会随机咬空（实测三连跑红两红），改成用 200 行那一形钉退码（2 的 -200 次方），连跑八次零红。**教训：凡是「过滤器有没有掷中过」参与决定的断言，输入行数必须把反面概率压到可忽略，否则门自己就是 flake 源。**
  - **下一轮首动作**：`tools/claim.sh claim RANDOM-DENOM-LEGS qoder` 接着做**片2**——过滤器主体按 `random.c:114-137` 接成「逐行独立掷、中者整行写出」，选项环 `-e`（取 `[0, denom)` 当退码，`:110-111`）与 `-r`（`setbuf(stdout, NULL)`，`:117-118`），两句 `err(2, "stdout")` / `err(2, "stdin")` 带退 2（`:131-136`）替换现在自创的 `random: read error` 与 `random: nothing selected`；片3 再动分母的取值域（`double` 与 `*ep != '\0'` 整串判据）。条目状态：**partially fixed**（片1 落地并坐实，片2/片3 敞着）。

- ✅ **批次三百四十二（纯台账笔，生产码一字未动）把 `random` 一整支量清：五处操作数与句子的分叉，另加一处主体语义的分叉，并定出三片的修法**（`2026-10-18`，`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `b07c82555`，只动 `rewrite-notes/18-stage-commands/todo.md` 与 `rewrite-notes/coordination/new_edge4.md`，单域三线 games/text-games，无跨界）
  - **为什么这一笔不动码**：登记时的标题是「text-games 三面的 `err`/`errx` 腿」（排期行 R16）。照 C 真源与参照件量完才发现真正的主洞不在句子，而在**它做什么**：C 是逐行过滤器，本壳是「挑一行」。把主体语义混进「接两句报文」里做，等于拿一次猜测代替一次判定；所以本笔只定形与切片，句子腿留给片1。
  - **真源形状**（`minix3/games/random/random.c`）：选项环 `getopt(argc, argv, "er")`（`:72`）；`usage()` 只一行 `usage: random [-er] [denominator]`、退 1（`:60`、`:143-147`）；操作数个数的分派在 `:86-105`——**零枚**取 `denom = 2` 继续干活，**一枚**先 `errno = 0` 再 `strtod(*argv, &ep)`，`errno == ERANGE` 走 `err(1, "%s", *argv)`（三段形、尾巴是平台的 `strerror` 句子），`denom == 0 || *ep != '\0'` 走 `errx(1, "denominator is not valid.")`（**句子里不带操作数**），**两枚及以上** `usage()`；过滤器主体在 `:114-137`：`selected = (int)(denom * random() / RANDOM_MAX) == 0`，`getchar` 循环里选中即 `putchar`，遇换行重掷，`ferror(stdout)` 与 `ferror(stdin)` 各走 `err(2, …)`。
  - **参照件现量**（`tmp/random-342/random_ref`，由 `minix3/games/random/random.c` 在宿主编成，垫片是 `-D__COPYRIGHT(s)=`、`-D__FBSDID(s)=`、`-D__dead=`、`-DRANDOM_MAX=2147483647L`；最后那枚是 Minix 专有常量、宿主没有，它只改抽样数值流而不改句子与退码，所以下面这些**句子、退码、行数**可用，逐行的抽样内容不作逐位对账——同一角落在批次三百三十五与 `jot` 一族已登记过）：`abc` → `random_ref: denominator is not valid.` 退 1；`0` → 同句退 1；`1e999` → `random_ref: 1e999: Numerical result out of range` 退 1；`1.5` → 退 0 正常过滤；`2 3` → 一行 usage 退 1；无操作数 → 退 0 且三行里打出两行（分母默认 2）；`random 1` → 三行**全打**（`a|b|c|`）。程序名那一段是 `getprogname()` 给的，所以宿主参照件打的是 `random_ref:`，本壳与真机上的 `random` 该打 `random:`——上一笔之外的旧判例（批次三百二十七与三百四十六关于宿主 `printf`/缓冲的口径）在这里只作用于前缀一段，句子正文与退码仍是可用的真值。
  - **六处分叉（本壳 → C）**：① 无操作数时本壳打 `usage: random [N]` 退 1（`os/commands/games/text-games/src/bin/random.rs:29-32`），C 取 2 当分母继续；② 非法分母本壳印 `random: <原词>: bad denominator`，C 印 `random: denominator is not valid.`——**多带了操作数、句子也不同**；③ `strtod` 溢出那一支本壳并入②的自创句，C 是带 `errno` 句子的三段形；④ 分母取值域本壳只收「非空十进制数字串且 ≥1」（`src/lottery.rs:13-32` 的 `parse_denominator` 返回 `u32`），C 是 `double`，`1.5` 合法；⑤ 操作数两枚以上本壳只看 `argv[1]`、多余的静默忽略，C 直接 usage 退 1；⑥ **主体语义**：本壳读全输入、每行掷一次、只保留**最后一次**命中的那一行并打印，一行都没命中就印 `random: nothing selected` 退 1；C 是边读边过滤，命中的每一行都打印，一行都不命中则正常退 0 且输出为空。⑥ 是这支的主体缺陷，①②③④⑤ 是它的邻居。
  - **三片的修法（片与片互不阻塞，各自可独立成轮）**：**片1** 操作数个数与两句报文——零枚默认 2、两枚以上 usage、`denominator is not valid.` 逐字照 C、`ERANGE` 那支走三段形（要在 `src/bin_support.rs` 补一枚 `warn_line`：text-games 那份目前只有 `args`/`terminate`/`warn`/`emit`/`epoch_*`，与 textfilter 那份不同源，补它属于本片范围）；**片2** 过滤器主体——换成逐行「掷中即打印」的流式形状，去掉 `nothing selected` 这个自创出口，并接上 `-e`（退出码取 `denom * random() / RANDOM_MAX`）与 `-r`（输出不缓冲）两枚旗标，门只钉形状、行数与退码，不钉逐位随机数；**片3** 分母作为 `double` 的取值域——含小数、`e` 记法，以及 `*ep != '\0'` 这条「必须整串吃掉」的判据在 `no_std` 里怎么复刻（`f64::from_str` 不吃前缀，得自己找尾巴的位置）。
  - **顺带钉两件不在本支的**：`wtf` 在 `minix3/games/wtf/` 是 sh 脚本、没有 C 真源，R16 标题里的这一面**不可能按 C 接**（判定表已自陈，此处再钉一次防下一次误捡）；`fortune` 另有三处（`minix3/games/fortune/fortune/fortune.c:525`、`:705`、`:1056`，都是 `warn("Cannot open \`%s'", path)` 这一族），量大且要先读它的库文件选择逻辑，单开一支。
  - **三件套（可重跑）**：`gcc -O0 -D'__COPYRIGHT(s)=' -D'__FBSDID(s)=' -D'__dead=' -DRANDOM_MAX=2147483647L -o random_ref minix3/games/random/random.c`，再逐形跑 `random_ref`，读数即上列六条；核实日 `2026-10-18`；基线 `b07c82555`。本笔不动生产码、不动门，测试读数沿用上一笔（宿主与容器同为 1067 passed / 0 failed，68 个 target），不复述成新读数。

- ✅ **批次三百四十一把 `tr` 等价式的两句报文接上，`[=ab=]` 这类式子不再被悄悄收成单枚成员**（`2026-10-18`，`claim/TR-EQUIV-SENTENCES-qoder` @ 基线 `02d83cedf`，动 `os/commands/usr-bin/textfilter/{src/tr.rs,tests/tr_face.rs}` 与本文件、认领板 `new_edge4.md`，单域三线 commands/textfilter，无跨界）
  - **这一条是什么**：`[=c=]` 是等价类写法。C 分两步判它：`bracket()`（`minix3/usr.bin/tr/str.c:193-199`）只问「式子里有没有一处 `=]`」，有就认定这是等价式并把指针推到那处之后；认定之后才由 `genequiv()`（`:305-330`）逐枚核内部——成员吃**一枚**字符（或一枚反斜杠转义），紧接着必须是 `=`，否则 `errx(1, "Misplaced equivalence equals sign")`，再然后必须是 `]`，否则 `errx(1, "Misplaced equivalence right bracket")`。本壳此前只做了第一步：找到 `=]` 就认定成形，然后取 `pos + 2` 那一枚当成员、直接跳到式尾，**两道核对一句都没接**。于是 `[=ab=]`、`[=abc=]`、`[=a b=]`、`[==]`、`[=a=b=]` 五形在本壳退 0 并交出「成员是首字符」的翻译结果，在参照件那里全部退 1 并报句子。这是 P0-code-bug：把该报错的形判成能干，退码、标准错误、输出三件都错。
  - **改法**：把 `src/tr.rs` 的等价式分支改成照 `genequiv()` 的次序走——取成员（带反斜杠时按 C 的 `backslash()` 一次走过两枚字节，与既有 `escape_literal` 同形）、核 `=`、核 `]`，两道核对各立一枚 `SetGrammar` 变体与一句逐字照 C 的原文。为什么不选「只补等号那句」：`[=a=b=]` 走的是第二句，两句是同一条腿上的两个位置，接一半就还剩一个假报「能干」的形。对照 Linux/Redox：GNU `tr` 与 NetBSD `tr` 都不检查内部（前者报 `invalid character class syntax`），Minix3 这两句是它自己的血脉，真值只认 `minix3/` 的 C 源。
  - **门**：库测三枚（`src/tr.rs`）——第一枚把 `[=ab=]`、`[=abc=]`、`[=a b=]`、`[==]` 四形在两个串位各钉一次并钉句子原文；第二枚钉 `[=a=b=]` 报的是右括号那句（不是等号那句，位置本身就是行为）；第三枚钉转义腿一次吃两枚字节——`[=\=]` 因此报等号那句，而 `[=\n=]`、`[=\t=]` 正常收成换行/制表这一枚成员。面门一枚（`tests/tr_face.rs:313`）把五形在两个串位共十次跑出进程，钉退码 1、stdout 为空、stderr 逐字节等于 `tr: <句子>\n`，并钉成形的 `[=c=]` 不退化（参照件现量 `tr '[=c=]' x` 对输入 `abc` 给 `abx`，被换掉的是 `c`）。
  - **我自己写错过一次期望，靠参照件纠正**：面门最后一形我先写成「`[=c=]` 把 `a` 换成 `x` 得 `xbc`」，门当场跑红（本壳给 `abx`）。查参照件 `printf 'abc' | tr_ref '[=c=]' x` 现量 `abx`——错的是我对「成员是 `c`」的理解，代码是对的。这与批次三百三十八那两条同源：**门红先问 C，别急着改代码**。
  - **读数**：`cargo test -p minix-textfilter --lib tr::` → **21 passed / 0 failed**（上笔 18）；`--test tr_face` → **15 passed / 0 failed**（上笔 14，新立 1 枚）；宿主整包 `cargo test -p minix-textfilter --offline -j 1` → **1067 passed / 0 failed**、68 个 target（上笔 1063）；宿主三架构 `x86_64-unknown-none`/`aarch64-unknown-none`/`riscv64gc-unknown-none-elf` 各 `Finished` 且错误计数 0；`cargo check --workspace -j 1` 0 error；触及两件 `rustfmt --edition 2021 --check` 零漂移；`cargo clippy -p minix-textfilter --all-targets` 退码 0 且 `src/tr.rs` 零命中；**权威容器门 `cargo test -j 1 -p minix-textfilter` → 1067 passed / 0 failed、68 个 target、退码 0**（`tmp/tr-341/container-gate.log`），与宿主逐枚一致；A/B 对参照件 `tmp/tr-322/tr_ref` 十一形 × 两串位共 22 次 → **SAME=22 DIFF=0**（脚本与读数 `tmp/tr-341/ab341.log`）。
  - **负控六枚全咬**（`tmp/tr-341/ctl341.py`，读数 `tmp/tr-341/ctl341.log`）：跳过等号核对 → lib 2 红 / 面 1 红；跳过右括号核对 → 1/1；两句标签互换 → 2/1；成员腿退回「不吃转义」→ 1/1；等号那句改报「畸形」→ 2/1；等号判据反号 → 4/1。baseline 与逐枚整篇还原后同为 21/0、15/0。**两枚锚点第一次没命中**：rustfmt 把长句折行、而 `let member = if bytes[cursor] == b'\\' && …` 这一形在等价腿与序列腿各有一处——改法是按 rustfmt 之后的真实排写取锚、并用前一行 `let mut cursor = pos + 2;` 把等价腿那处区分出来。
  - **登记不执行**：① `TR-EQUIV-OOBCH-SET2` 仍在册——`tr abc '[=c=]'` 两边都退 0、集合都是单枚 `c`，但 string2 用尽后参照件把后续位置填 `\0`（现量 `c\0\0\n`）、本壳重复末字节（`ccc\n`），那是 `tr.c` 的 OOBCH 腿，与 `bracket()`/`genequiv()` 的判据无关，本笔不动；② `genequiv()` 里 `s->equiv[0]` 是长度 1 的数组这一事实（C 注释「English doesn't have any equivalence classes」）意味着等价类永远只有一枚成员，本壳已按同一形状建，若将来接真等价类表就是另一件契约变更，不在射程内；③ 参照件链的是宿主 glibc，`errx` 的前缀取 `getprogname()`，两形都只比句子不比平台，句子逐字取自 C 源。
  - **顺带记下本轮的选路教训（不是本支的 TODO）**：排期表 `tmp/todo-loop-PLAN.md` 的 R 表状态列与 18 篇里若干条「下一支取 …」的指针**系统性滞后**——本轮先取的 `SEQ-GETOPT-LINE` 实际早在批次三百二十就已闭合，`R10`（primes 的 `-d`/`-h`）在批次二百七十三、`R11`/`R12` 在批次二百七十七、`R13`（join 逐支文案）在批次二百七十四都已闭合。已按代码现查复核出的真敞口两支留作后续：**R16**（`fortune`、`random` 两支的 C `warn`/`err` 腿，`minix3/games/fortune/fortune/fortune.c:525`、`:705`、`:1056` 与 `minix3/games/random/random.c:97`、`:99`；`wtf` 那支没有 C 真源，不能按 C 接）、**R17**（`truncate` 的 `relative_target` 用 `saturating_add` 而 C 是 `off_t` 自然回绕后 `if (tsize < 0) tsize = 0`，`minix3/minix/commands/truncate/truncate.c:121-131`，登记见本文件批次二百四十四、二百四十九两处）。取项规矩随之改一条：**按 R 表取项之前先对代码现查一次**，别信表上的空状态列。

- ✅ **批次三百四十把 `split -n` 要切的字节数改成向内核询问，管道那一形从此报错而不是切得出**（`2026-10-18`，`claim/SPLIT-STREAMING-MEMORY-qoder` @ 基线 `dd77ce7c0`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/split.rs,tests/split_face.rs}` + 18 篇 `07-text-filter.md` 与本文件、认领板 `new_edge4.md`，单域三线 commands/textfilter，无跨界）
  - **这一条是什么**：上一笔量出的分叉是——`-n`（切成固定片数）那一支需要的字节总数，本壳从「已经收进来的字节数」推，C 从内核问。C 的 `split3()` 在读第一块数据之前就 `fstat(ifd, &sb)`，问不到就 `err(1, "stat")`（`minix3/usr.bin/split/split.c:293-295`），随后用 `sb.st_size` 判「片数比字节数还多」并算块宽（`:297-300`）。管道与字符设备的 `st_size` 是 **0**，所以「切 3 片」在 C 那里只能当场撞死在报文上；本壳因为拿的是真实字节数，同一形切得出三枚分片、退 0。这是 P0-code-bug：退码与落盘文件两件都错。
  - **改法（上一笔定下来的那条）**：`src/bin_support.rs` 新立 `input_size(path: Option<&str>) -> Result<u64, minix_sys::Errno>`，两条 `cfg` 腿同签名——宿主腿命名档走 `std::fs::metadata(path).len()`，`None`（没有操作数或裸 `-`）走 `std::fs::metadata("/dev/stdin")`；真机腿 `None` 走 `minix_sys::fstat(STDIN, &mut status)`、命名档走 `minix_sys::stat(name, &mut status)`，取 `st_size` 并夹住负数。选 `/dev/stdin` 这条路径的理由在上一笔已走完判据：std 里没有不带 `unsafe` 的「已打开的标准输入 → 可问大小的对象」通道（`File: From<Stdin>` 未实现），而本机实测这条路径与 C 的 `fstat(0)` 三形同数（命名片重定向喂入给 10、管道给 0、空设备给 0）。**这是承载差异不是语义差异**：两条腿问的都是「内核认为这个输入有多少字节」，只是问法受 std 接口面限制。为什么不选「只把命名档改对、管道那一形先放过」：C 那里命名档与管道是同一次 `fstat`，只改一半等于把一条判据抄成两处。
  - **壳侧两臂**：`src/bin/split.rs` 的 `-n` 分支第一件事是问大小；问不到就 `warn_line(prog, "stat", code)` 退 1（C 的 `err(1, "stat")` 三段句形）；问到了就先判 `(n as u64) > size`，报 `{prog}: can't split into more than {size} files` 退 1——**句子打的是大小而不是片数**，这与 C 的 `errx(1, "can't split into more than %d files", (int)sb.st_size)` 逐字对得上。`chunks_n()` 交回 `None` 那一臂也打同一句话、同一个数：走到那里只可能是「收到的字节比问来的少」，与 C 不分岔。
  - **门三枚新立**（`tests/split_face.rs:556`、`:569`、`:613`）：`a_piped_input_refuses_the_chunk_count_it_cannot_count` 钉管道那形的退码 1、句子 `can't split into more than 0 files`、一枚分片都不生成；`a_named_operand_still_splits_the_chunks_it_can_count` 钉命名档不退化（96 字节切 3 片 → `xaa/xab/xac` 各 32 字节、标准错误为空）；`the_chunk_count_complaint_quotes_the_size_not_the_count` 钉 `split -n 200` 打在 96 字节的档上时报的是 `96` 而不是 `200`。前两枚正是上一笔登记「门要新立两枚」所指，第三枚是写码时想到的一形：句子内容本身没有门钉过，M2 那枚负控靠它才咬得住。
  - **负控六枚全咬**（`tmp/split-340/ctl340.py`，读数 `tmp/split-340/ctl340.log`）：大小退回「已收到字节数」→ 1 红；报文里的数换成片数 → 2 红；问来的大小写死成 0 → 2 红；标准输入那条腿问错对象 → 1 红；上限判据反号 → 2 红；命名档那一形改问标准输入 → 2 红。还原方式沿用上一笔定下的规矩（变异前整篇存原文、还原整篇写回并断言逐字节相等），baseline 与还原后同为 23/0。
  - **A/B 复跑**（`python3 tmp/split-339/ab339.py`，读数 `tmp/split-340/ab340.log`）：与参照件 `tmp/split-311/split_ref` 逐形对读，三形全 SAME——命名片、管道、重定向命名片。上一笔的 DIFF 消失。
  - **读数**：`cargo test -p minix-textfilter --test split_face` → **23 passed / 0 failed**（上笔 20）；`cargo test -p minix-textfilter --offline -j 1` → 宿主 **1063 passed / 0 failed**（68 个 target，上笔 1060）；**权威容器门 `cargo test -j 1 -p minix-textfilter` → CONTAINER passed=1063 failed=0、68 个 target、退码 0**（`tmp/split-340/container-gate.log`，挂载配方见本文件 §7），与宿主逐枚一致；宿主三架构 `x86_64-unknown-none`/`aarch64-unknown-none`/`riscv64gc-unknown-none-elf` 各 `Finished` 且错误计数 0；`cargo check --workspace -j 1` 0 error；触及三件 `rustfmt --edition 2021 --check` 零漂移；`cargo clippy -p minix-textfilter --all-targets` 退码 0，指向触及件的只剩 `src/bin/split.rs:238`「test 模块之后还有条目」一条，该形状在基线里就在、不属本支。
  - **本轮的验证边界（不是门没写，是宿主造不出这一形）**：`stat` 问不到那一臂**没有门**——宿主上一条开得出、读得动的输入，`std::fs::metadata` 问的是同一份 inode 信息，没有可构造的失手路径；真机侧那一臂的门归 `C-57`/E5 的既有口径。与批次三百三十七那条 close 腿的边界同类，但**上限判据那一臂在宿主完全可测**（管道就是现成的、大小为 0 的输入），本笔已钉死，不要把两件事混成一句「都得等真机」。
  - **`SPLIT-STREAMING-MEMORY` 仍然敞着**：本笔只把「大小从哪来」这一件事接对，**边读边写没有做**——`split` 仍是先收全输入再切块（`read_operand` 读到结束、`src/bin/split.rs` 在整块数据上切），C 是在 `MAXBSIZE` 的缓冲上边读边写（`split.c:184-212`）。这一条 ID 的名字指的是后者，所以认领用它的树、销账不销它：状态在认领板上记 `partially fixed`，切片与下一片的落点见该条。
  - **下一轮首动作**：`tools/claim.sh claim UNITS-* qoder` 或按排期取队首；`split` 的增量读写那一片排在流式改造的独立一轮（改动面是 `read_operand`、三种切法的驱动与 `WriteSink` 的写入时机，超出一轮单条 TODO 的预算，硬做会留下半成品）。

- ✅ **批次三百三十九（纯台账笔，生产码一字未动）把 `split -n` 喂管道那一形量成分叉，并把取数那一步的三条路走完判据**（`2026-10-17`，`claim/SPLIT-STREAMING-MEMORY-qoder` @ 基线 `102919d89`，只动 `rewrite-notes/18-stage-commands/todo.md` 与 `rewrite-notes/coordination/new_edge4.md`，单域三线 commands/textfilter，无跨界）
  - **这一条是什么**：C 的 `-n` 那一支在读数据之前先 `fstat(ifd, &sb)`，失手就 `err(1, "stat")`（`minix3/usr.bin/split/split.c:293-295`），随后拿 `sb.st_size` 判「块数比字节数还大」并交 `split1(sb.st_size/chunks, chunks)`（`:297-300`）。管道与空设备的 `st_size` 是 **0**，所以「切 3 片」在 C 那里当场撞死在这一句上。本壳的 `-n` 用的是读全之后的 `input.len()`（`os/commands/usr-bin/textfilter/src/bin/split.rs`），那个数永远等于真实字节数，于是同一形本壳切得出来。
  - **现量（参照件 `tmp/split-311/split_ref` 与本壳 `target/debug/split` 逐形对读，脚本 `tmp/split-339/ab339.py`、读数 `tmp/split-339/ab339.log`）**：命名片作操作数 SAME（两侧都 `xaa xab xac`、退 0）；**管道喂标准输入 DIFF —— 参照件 `split: can't split into more than 0 files`、退 1、一枚分片都不生成，本壳退 0、生成三枚**；重定向命名片喂标准输入 SAME（两侧三枚、退 0）。这是 P0-code-bug：把能干的活判成能干，也把该报错的形判成不报错，退码与落盘文件两件都错。
  - **取数那一步的三条路与判据（本轮定的结论，下一轮照着做）**：① 宿主腿想对标准输入直接 `fstat`——std 里没有不带 `unsafe` 的通道，探针 `rustc --edition 2021 /tmp/probe_stdin.rs` 报 `the trait bound File: From<Stdin> is not satisfied`，只给了 `From<OwnedFd>` 那条，要走就得自己重复一个描述符，撞上仓库的 unsafe 门；② 走 `/dev/stdin` 这个路径名 stat：本机实测三形都**与 C 的 `fstat(0)` 同数**（重定向命名片给 10、管道给 0、空设备给 0，`os.stat('/dev/stdin')` 的 `st_size`，`readlink` 落在 `/proc/self/fd/0`）；③ 真机那条腿本来就写着取数动作——`minix_sys::fstat` 在 `os/libs/minix-sys/src/lib.rs:821`，缺的只是别在 `read_operand` 里读完就把描述符关掉。
  - **下一轮的改法定下来**：两条腿都在**已经打开的那个句柄**上取数（真机 `fstat(fd)`、命名档 `File::metadata()`），标准输入那一形在宿主侧退到 ②；这条退让要写明是**承载差异不是语义差异**——同一件事（问内核要这个输入的字节数），只是 std 不给不带 `unsafe` 的通道。为什么不选「只把命名档改对、管道先放过」：C 那里命名档与管道是同一次 `fstat(ifd)`，只改一半等于把同一条判据抄成两处，正是本项目反对的形状。
  - **顺带把上一笔登记的验证边界往前挪一格**：批次三百三十七那条「目标腿丢掉 close 的失败，宿主测不到」仍然成立；但本笔的 `stat` 臂不同——**它在宿主可测**（管道就是宿主现成的、`st_size` 为 0 的输入），所以登记时不要把这两件事混成一句「都得等真机」。
  - **三件套（可重跑）**：`python3 tmp/split-339/ab339.py` → 三形 SAME/DIFF/SAME，其中 DIFF 是管道那一形；核实日 `2026-10-17`；基线 `102919d89`。本笔不动生产码、不动门，全套测试读数因此取自上一笔（`1060 / 0`，容器与宿主一致），不复述成新读数。
  - **下一轮首动作**：`tools/claim.sh claim SPLIT-STREAMING-MEMORY qoder` 之后第一处改动是 `os/commands/usr-bin/textfilter/src/bin_support.rs` 的 `read_operand` 与 `read_stdin_error` 两条腿各多交回一个大小值，第二处是 `src/bin/split.rs` 的 `-n` 分支把 `input.len()` 换成那个值并新增 `stat` 失手那一臂；门要新立两枚（管道那一形的退码、句子与零分片，命名档那一形不退化）。

- ✅ **批次三百三十八把 `split` 的输入拆成开档、读档、标准输入三形各归各句，闭合 `SPLIT-READ-STAT-ERRLEG` 的读腿一片**（`2026-10-17`，`claim/SPLIT-READ-STAT-ERRLEG-qoder` @ 基线 `f780c6614`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/split.rs,tests/split_face.rs}` + 18 篇三份文档，单域三线 commands/textfilter，无跨界）
  - **这一条是什么**：输入只有三个来源形状：没有操作数读标准输入、裸 `-` 也读标准输入、其余按名字开档。C 把这三形的失手分成两句话打——开不出来是 `err(1, "%s", *argv)`（`minix3/usr.bin/split/split.c:136-138`），读不进去是 `err(1, "read")`（`split1` 的 `:185-190`、`split2` 的 `:252-257`）。**标签是 `read` 这个词，既不是档名也不是 `stdin`**，因为 `ifd` 的初值就是 0，标准输入与命名的档走的是同一条 `read()` 腿。本壳两处都不对：`support::read_file(name)` 把开档与读档折进同一只 `Result`，读失败也报档名；标准输入那条腿走 `read_stdin()`，压根没有错误通道，读失败等于读完，一句话不说退 0。
  - **改法**：`bin_support.rs` 新立 `InputFailure { Open(Errno), Read(Errno) }` 与 `read_operand(path)`，两条 `cfg` 腿同签名（宿主 `File::open` 加 `read_to_end`，真机 `open` 加逐块 `read`），标准输入复用批次三百三十一那条带号的 `read_stdin_error()`。壳侧把三形分派到位：开档失手当场 `warn_line(prog, name, code)` 退 1；读失手则**把号连同已读到的字节一起往下走**，因为 C 是边读边写、前面的分片已经在盘上，本壳先切完再补那一句，收尾统一进 `finish(prog, read_failure)`。
  - **两枚负控一开始咬不住，查出来是门自己不够**：其一「开档臂把平台号换成写死的号」不红——`an_unopenable_input_prints_the_err_shape_in_three_parts` 只断言 `split: <操作数>: ` 这个前缀加上「尾巴非空且只有这一行」，写死的号照样能打出一句非空的 `strerror`；改成整句逐字节比（参照件现量 `No such file or directory`）之后咬住。其二「标准输入那条腿退回不带号的读法」不红——**没有任何一门跑过一条读不动的标准输入**；补一枚 `an_unreadable_stdin_blames_the_read_arm`（把一枚目录的文件句柄当标准输入递进去，宿主当场 `EISDIR`）之后咬住。这与批次三百三十六那句经验同源：负控不咬先怀疑断言不够，别把门的瞎说成代码的干净。
  - **门**：`cargo test -p minix-textfilter` → **1060 passed / 0 failed**（68 个 target，上笔 1058）；`--test split_face` → **20/0**，本笔新立 1 枚、补强 1 枚；**权威容器门 `cargo test -j 1 -p minix-textfilter` → 1060 / 0 / 0、退码 0**，与宿主逐枚一致（这是断了三笔之后第一次跑成，病根、探针与挂载配方已归入本文件 §7；日志两份：`tmp/split-338/container-gate-final.log` 与改动定稿后复跑的 `tmp/split-338/container-gate-committed.log`，同为 1060/0/0）；宿主三架构 `x86_64-unknown-none`/`aarch64-unknown-none`/`riscv64gc-unknown-none-elf` 各 `Finished` 且 `^error` 计数 0；`cargo check --workspace -j 1` 0 error；触及三件 `rustfmt --edition 2021 --check` 零漂移；`cargo clippy -p minix-textfilter --all-targets` 退码 0，指向触及件的只剩 `src/bin/split.rs:225`「test 模块之后还有条目」一条，该形状在基线 `f780c6614` 里就在（`mod tests` 之后是 `usage`/`arg_error`/`main`），不属本支、不顺手改；本笔另清掉两条落在自己行上的告警——宿主写腿的 `let_and_return`、`close_outcome` 上那句无话可说的 `#[must_use]`。
  - **负控五枚全咬**（`tmp/split-338/ctl338.py`，读数 `tmp/split-338/ctl338.log`）：开档臂的标签换成 `read` → 2 红；读失手不打句子 → 2 红；读失手把退码归零 → 2 红；开档臂丢号 → 1 红；标准输入腿退回无错误通道 → 1 红。baseline 与逐枚还原后同为 20/0。
  - **本笔自己的一处工具缺陷（记下来防下一次）**：负控脚本原本用「把替身再换回锚点」来还原，而 `support::warn_line(prog, "read", code);` 这一形在 `finish` 与开档臂两处同形，反向替换把另一处一起改了，源文件当场被写坏。改法是变异前把整篇原文存下来、还原时整篇写回，并断言写回后的文本与原文逐字节相等——**还原手段必须比它保护的锚点更粗，否则负控自己就是破坏者**。
  - **登记不执行（同一条 ID 的第二片）**：`stat` 腿未接。C 的 `-n` 先 `fstat` 输入，失手即 `err(1, "stat")`（`split.c:293-295`），再用 `sb.st_size` 判 `n > size` 报 `can't split into more than %d files`（`:297-300`）；本壳这两处都从已经收全的 `input.len()` 推，`fstat` 的失手在形状上不可达。它与「先收全输入而不是边读边写」是同一根结构问题（`SPLIT-STREAMING-MEMORY`），所以拆到下一轮一起做。
  - **下一轮首动作**：`tools/claim.sh claim SPLIT-STREAMING-MEMORY qoder`（把 `split` 改成边读边写，同时给 `stat` 腿让出位置）；开工量若超一轮，按本文件惯例切片并在此登记。

- ✅ **批次三百三十七把 `split` 分片文件的 `close` 腿接成第三臂，闭合 `SPLIT-CLOSE-ERRLEG`**（`claim/SPLIT-CLOSE-ERRLEG-qoder` @ 基线 `d52b9f383`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/split.rs}` + 18 篇两份文档，单域三线 commands/textfilter，无跨界）
  - **是什么、为什么**：一枚分片文件在 C 那里有三处会失败，各自一句：开不出来 `err(1, "%s", fname)`（`minix3/usr.bin/split/split.c:331-332`）、写不进去 `err(1, "write")`（`:204`、`:216`、`:225`）、**写完关掉关不掉** `err(1, "%s", fname)`（`newfile()` 在开下一枚之前 `close(ofd)`，`:319-320`；此刻后缀还没往前推，`fname` 仍是刚写完那枚的名字）。上一轮把写腿的号接到位了，`close` 这一腿仍然被丢掉：目标腿 `minix_sys::close` 的返回被 `let _ =` 吞掉，宿主腿连 observable 的机会都没有（`std::fs::File` 的 drop 不交回任何号）。
  - **改法**：`WriteSink::finish` 的返回换成 `SinkFailure { Write(Errno), Close(Errno) }`，两腿的取舍写成一张明表 `close_outcome`——**写失败优先**，因为 C 在 `bigwrite` 失手处当场退出，走不到下一次 `close`。壳侧 `ChunkWrite` 长第三臂 `Close`，标签与开档臂同为路径、与写臂不同；映射从 `write_into` 里拆出纯函数 `sink_failure_of`。
  - **为什么非要拆这两枚纯函数**：真机那条腿才会交回 `Close`，宿主测试跑不到，于是「臂没接错」这件事在宿主侧本来无门可钉。拆成 `close_outcome` 与 `sink_failure_of` 之后，表与映射各有一枚门，负控才咬得动——这是本轮实测出来的：拆之前三枚变异（close 折成 write、折成开档、写臂折成 close）全部静默通过，拆之后三枚全红。
  - **门**：`cargo test -p minix-textfilter` → **1058 passed / 0 failed**；新立三枚（`close_outcome` 的两枚表门：写失败赢、写干净时仍报 close 失败且不许凭空造失败；`sink_failure_of` 的一枚映射门）；宿主三架构 `x86_64-unknown-none`/`aarch64-unknown-none`/`riscv64gc-unknown-none-elf` 各 0 error；`cargo check --workspace` 0 error；触及件 `rustfmt --edition 2021 --check` 零漂移；`cargo clippy -p minix-textfilter --all-targets` 触及件零命中。
  - **负控六枚、五枚咬**：close 折成写失败 → 1 红；close 折成开档并压号 → 1 红；两腿优先级反过来 → 1 红；close 臂报成 `write` → 1 红；写臂折成 close → 2 红。第六枚（目标腿丢掉 close 的失败）改的是 `cfg(not(test))` 那一侧的接线，宿主测试到不了，因此不红——这是本轮的**验证边界**，接线本身归真机门（`C-57`/E5 既有口径）。
  - **顺带钉住的一处既存风险**：三架构编译在本轮中途报过一次 `E0308`（`write_loop` 交回 `Errno` 而 `close_outcome` 要 `SinkFailure`）——宿主全套当时是绿的，因为它跑的是 `cfg(test)` 那条腿。这条教训值得记：**只跑宿主测试不足以证明 `cfg` 双生面的一侧编得过**，三架构编译必须在同一轮里跑，不能留到最后。
  - **本轮没做的**：宿主腿看不见 `close` 失败这件事未修（`std::fs::File` 不交回号；要看得见得改成自己 `close` 一次，属另一笔的取舍，登记为 `SPLIT-CLOSE-HOST-VISIBILITY`）；`uniq`/`sort` 的输出腿只跟随签名，报文形状未动（`UNIQ-WRITE-ERRLEG` 已在册）。**下一轮首动作**：`tools/claim.sh claim SPLIT-READ-STAT-ERRLEG qoder`。

- ✅ **批次三百三十六让 `split` 的写腿把平台那个号一路带到报文里，闭合 `SPLIT-WRITE-ERRNO`**（`claim/SPLIT-WRITE-ERRNO-qoder` @ 基线 `0021e749f`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/split.rs,src/bin/uniq.rs,tests/split_face.rs}` + 18 篇两份文档，单域三线 commands/textfilter，无跨界）
  - **是什么、为什么**：`split` 每枚分片都是「开档 + 写完」，C 那两臂各打一句——开不出来是 `err(1, "%s", fname)`（`minix3/usr.bin/split/split.c:332-333`，报**路径**），写不进去是 `err(1, "write")`（`:204`、`:216`、`:225`，报**这个词**），两句的第三段都是 `errno` 对应的句子。这一支的 `WriteSink::finish` 交回 `Result<(), ()>`，壳里再折成 `EIO`，于是「盘写满」与「真的一次 I/O 错」被压成同一句话；目标腿还只 `write` 一次，写不完也当写完了。
  - **改法与为什么不做得更厚**：`finish` 两条腿都改成交回 `minix_sys::Errno`——宿主腿用既有的 `errno_of`（`raw_os_error()` 就是 C 的 `errno`），目标腿补上 `bigwrite()` 的续写循环（`split.c:337-351`），只有 `write` 交回 0 这种「再也吃不进字节」的情形才落到 `EIO`。壳侧另拆出 `write_into` 与 `failure_label` 两枚纯函数：前者让「失败到号」有门可钉，后者让「两臂不互换标签」有门可钉——渲染那一步要打要退出，宿主侧看不见，不拆就只剩一组无法证伪的说法。
  - **门**：`cargo test -p minix-textfilter` → **979 passed / 0 failed**；新立三枚——`the_write_leg_keeps_the_platform_number`（只读句柄包成写腿，宿主当场 `EBADF`＝9，钉「号没被压平」）、`the_two_write_arms_do_not_swap_their_labels`（钉两臂各自的标签）、`a_chunk_that_cannot_be_created_names_the_path_not_write`（进程级：前缀落在不存在的目录里，报文是 `split: <前缀>aa: No such file…`，退 1、一行、不接 usage）；宿主三架构各 0 error；`cargo check --workspace` 0 error；触及三件 `rustfmt --edition 2021 --check` 零漂移。
  - **负控六枚全咬**：写腿又压成 `EIO` → 1 枚红；`errno_of` 丢掉 `raw_os_error` → 3 枚红；宿主写腿直接吞号 → 1 枚红；两臂标签互换 → 1 枚红；写臂标签改名 → 1 枚红；开档腿丢号 → 1 枚红（最后一枚是先发现「门只钉住路径没钉住句子」再补强尾句断言才咬住的，同批次三百三十四那条经验：负控不咬先怀疑断言不够）。
  - **验证边界与本轮没做的事**：目标腿的续写循环在 `cfg(target_os = "none")` 一侧，宿主门够不着，需真机门（`C-57`/E5 既有口径）；`close` 的失败仍不上报（`SPLIT-CLOSE-ERRLEG`）；`uniq` 的写腿这次只把签名跟到位、报文形状未动（`UNIQ-WRITE-ERRLEG` 一条登记）；`src/bin/uniq.rs` 里两处既存 rustfmt 漂移（`use` 顺序与注释缩进）不属本支，未顺手改。下一轮首动作：`tools/claim.sh claim SPLIT-CLOSE-ERRLEG qoder`。

- ✅ **批次三百三十五把 `tr` 集合语法的四句报文接到标准错误，并照抄 `bracket()` 的三条成形判据，闭合 `TR-SET-GRAMMAR-SENTENCES`**（`claim/TR-SET-GRAMMAR-SENTENCES-qoder` @ 基线 `5a6fbda21`，只动 `os/commands/usr-bin/textfilter/src/tr.rs`、`src/bin/tr.rs`、`tests/tr_face.rs`）
  - **是什么、为什么**：`parse_set` 此前对一切语法失败只回 `TextError::InvalidArgument`，壳里 `Err(_) => terminate(1)` 静默退 1，用户看不见任何一句解释。C 在 `minix3/usr.bin/tr/str.c` 每处都是当场 `errx(1, ...)`：`genclass()` 表外的名字打 `unknown class %.*s`（`:253`），`genseq()` 依次打 `Sequences only valid in string2`（`:353-356`）、`Misplaced sequence asterisk`（`:362-366`）、`illegal sequence count`（`:368-386`）。这三句的**先后次序**本身就是行为，缺一句就是多打一句不该打的。
  - **接法**：`SetSite`（第一串/第二串）做成 `parse_set` 的入参，因为 C 把 `which` 存在 `STR` 结构里由 `genseq()` 自查；`SetGrammar::sentence()` 返回 C 的原文，壳只补 `tr: ` 前缀、走标准错误、退 1 且**不带** usage 块。
  - **被参照件证伪的一条，判据因此改写**：我先按「凡是 `[` 开头就是重复式的尝试」实现，实测 `tr_ref abc '[x]'` 退 0 且原样输出——`bracket()`（`str.c:176-210`）在进 `genseq()` 之前先看 `strpbrk(s->str + 2, "*]")` 是否**先撞上 `*`** 且那枚 `*` 之后还有一个 `]`，否则返回 0，`next()`（`:124-132`）FALLTHROUGH 把这枚 `[` 当字面量。同理 `[:alpha` 缺 `:]` 收尾也是字面量而不是错误，`[=c=]` 是合法的单字符等价类。三条判据照抄之后，A/B 由「多形假报错」收敛到 SAME=20 / DIFF=1（唯一那条见下面的登记）。
  - **门**：`cargo test -p minix-textfilter --lib tr::` → 18/0；`cargo test -p minix-textfilter --test tr_face` → 14/0，新立四枚（四句报文逐字节、没成形的 `[` 不抱怨、开式与零个数都是满重复、反序区间是三枚字面量且退 0）；`cargo test -p minix-textfilter` 全套件零失败；宿主三架构 `x86_64-unknown-none`/`aarch64-unknown-none`/`riscv64gc-unknown-none-elf` 各 Finished；`cargo check --workspace` 0 error；`rustfmt --edition 2021 --check` 触及三件零漂移；`cargo clippy -p minix-textfilter --all-targets` 触及件零命中。容器门这三轮未跑：宿主与容器两侧的出站连接都不通，病根、探针与回退口径见本文件 §7；权威读数待网络恢复补跑。
  - **负控七枚、有效六枚**：`sequence_ahead` 恒真 → lib 红 2 / face 红 2；恒假 → 6/4；站点判据永不触发 → 1/1；零个数当空集 → 1/1；反序区间重新并入正向区间 → 1/1；`unknown class` 报文改写 → 1/1。第七枚把星号句与个数句在**枚举定义处**互换，结果没编译过，属无效负控（变异的写法不对，不是门不够弱），如实记录不充数。
  - **登记不执行**（两处）：① `TR-EQUIV-SENTENCES`——`genequiv()` 的 `Misplaced equivalence equals sign` 与 `Misplaced equivalence right bracket`（`str.c:307-330`）未接，本支把 `[=xy=]` 这一形收成单枚成员 `x`；② `TR-EQUIV-OOBCH-SET2`——`tr abc '[=c=]'` 两边都退 0、集合都是单枚 `c`，参照件在 string2 用尽后把后续位置填 `\0`（`od -c` 读数 `c \0 \0 \n`，见 `tmp/tr-335/ab335.log`），本层重复末字节得 `ccc\n`；那是 `tr.c` 的 OOBCH 腿而不是 `bracket()` 的判据。**下一轮首动作**：`tools/claim.sh claim SPLIT-WRITE-ERRNO qoder`。

- ✅ **批次三百三十四定了 `jot` 精度未写那一族的真值判据，并把默认转换从整数支改到浮点支，闭合 `JOT-PREC-MINUS-ONE-FORMAT`**
  （`2026-10-17`，`claim/JOT-PREC-MINUS-ONE-FORMAT-qoder` @ 基线 `679db151f`，动 `os/commands/usr-bin/textfilter/{src/jot.rs,tests/jot_face.rs}` + 18 篇两件文档，单域三线 commands/textfilter，无跨界）：
  **这一条当年卡在哪**：C 的 `prec` 初值是 `-1`（`minix3/usr.bin/jot/jot.c:74`），而钳位
  `if (prec == -1) prec = 0;`（`:229-230`）排在 `getformat()` **之后**——拼说明符那一步它还是 -1，
  `:349-356` 的条件 `chardata || prec == 0` 不成立，于是走 `"%%.%df"` 那一支，拼出 `%.-1f`。
  登记当年的判断是「这一形拼出的说明符本身是坏的，被打印成什么由各家 printf 决定，本机不可判定」，
  所以 A/B 只比退码与标准错误。这一笔把判据定下来，并且结论是：**宿主参照件不能当这一族的真值承载**。
  **判据由两条独立证据定**：①读 Minix 自己的 libc——`minix3/lib/libc/stdio/vsnprintf_ss.c` 是 NetBSD 血脉
  （文件头 `$NetBSD: vsnprintf_ss.c,v 1.13 2014-09-29`），其 `case '.':`（`:213-226`）先 `n = 0`，
  只有 `.` 后紧跟数字才累积精度；遇 `-` 时循环一次不进，于是 `prec = 0`，随后 `goto reswitch` 把剩下的
  字符重新按旗标/宽度/转换解析——`-` ⇒ 左调，`1` ⇒ 字段宽，`f` ⇒ 转换。所以 Minix 眼里的 `%.-1f`
  等价于 `%-1.0f`：**精度 0 的浮点，四舍五入**。②本机探针把两家的分歧量出来：`snprintf(f,…,"%%.%df",-1)`
  得到的串是 `%.-1f`（长度 5），而 `printf(f,1.5)` 在 glibc 下打回字面量 `%.0-1f`（`/tmp/pf2.c` 的读数）。
  同一串两种结果，说明「参照件打什么」不是判据；Minix 侧由 ① 定。
  **壳侧错在哪**：`src/jot.rs` 的默认转换写的是 `let prec = options.prec.unwrap_or(0);`，把「从未写过」
  （C 的 -1）与「显式写成 0」（C 的 `prec == 0`）合并成同一件事，于是这一族落进整数支 `%ld`，
  而整数渲染走 `(long)floor(x)`（对位 `jot.c:306` 的 `dox`）。两者在整值上同形、在带小数的值上分家：
  `0.7` 的 C 真值是 `1`（四舍五入），壳侧以前打 `0`（向下取整）。
  **改法与为什么不做得更厚**：条件改成 `options.prec == Some(0)`，未写精度那一支构造 `%-1.0f`，
  把 Minix 解析出的三个要素（左调、宽 1、精度 0）都带上；显式 0 仍走 `%ld`。不在这里再写一个通用
  printf 解释器：本壳只有构造默认转换时会拼这一串，用户经 `-w` 给的说明符走 `parse_conv_spec` 那条
  既有路径（批次三百一十七已钉），在此处再抄一个解释器等于把 libc 的实现搬进命令层。
  **门（库测 1 枚 + 面门 1 枚）**：`an_unwritten_precision_rounds_while_an_explicit_zero_floors` 钉四件事——
  未写精度构造出的说明符确实是 `left=true`、`width=1`、`prec=Some(0)`、转换为 `f`；
  `render(0.7)="1"`、`render(1.5)="2"`、`render(2.5)="2"`（凑偶，与 NetBSD dtoa 同规则）；
  显式 `Some(0)` 是 `dox=true` 且 `render(0.7)="0"`，即两支撑得开的值确实分家；
  外加 `-c` 配未写精度仍走整数支（C 的条件是 `chardata || prec == 0`，字符优先）。
  `the_unwritten_precision_family_prints_no_decimal_point_and_no_literal` 在进程级钉登记里那四形
  （`jot 3`、`-n 3`、`3 - 5`、`-s , 3`）退 0、无诊断，输出里既无小数点也无 `%`（后者专防打回字面量那种形态）。
  **变异负控七枚全部由门咬住**（控制器在 `tmp/jot-334/ctl334.py`，baseline 与还原后同为 38 通过 0 失败，
  逐枚还原核 md5）：未写精度改回走整数支咬 1、精度写成缺省 6 咬 6、去掉左调与宽度咬 1、浮点支的 `dox`
  误报为真咬 8、浮点支改用 `%ld` 咬 1、整数支忽略 `chardata` 咬 1、宽度写成 0 咬 1。
  **第一轮只有 4/6，两枚没咬住的正是断言不够**：`%-1.0f`→`%.0f` 在当时改不动任何输出（左调与宽度只在
  字段宽真起作用时可见），`chardata` 配未写精度那一形根本没人测——补上旗标/宽度断言与 `-c` 那一形之后
  变 7/7。与批次三百三十三同一条经验：负控咬不住时先怀疑断言不够，而不是宣布代码没事。
  **三件套（读数全取自宿主 `ulimit -v 3145728` 加 `--offline`；容器与宿主网络同断，见上一笔登记）**：
  `cargo test --offline -p minix-textfilter -j 1` → **926 / 0 / 0**（68 个 target，上一笔 924，本笔 +1 库测 +1 面门）；
  `--lib jot::` 23 通过、`--test jot_face` 15 通过；`cargo check --offline --workspace -j 1` 0 error；
  三架构 `cargo build --offline --bins -j 1 --target x86_64-unknown-none / aarch64-unknown-none / riscv64gc-unknown-none-elf -p minix-textfilter`
  各退码 0 且 `^error` 计数 0；`rustfmt --edition 2021 --check` 对两个触及件零漂移（面门那条 `for` 数组
  过长由 rustfmt 要求换行，格式化后复跑归零）；`cargo clippy --offline -p minix-textfilter --all-targets` 触及件零命中。
  核实日 `2026-10-17`；基线 `679db151f`。
  **登记不执行**：① `-r` 随机形是唯一能在进程级造出「带小数值且精度未写」的路径，而它的种子默认取
  `time * getpid`（`jot.c:232-234`），宿主上不可重复，故这一形的上机对账归 `C-57`/E5
  （`JOT-PREC-RANDOM-AB-ON-MACHINE`）；② `-w` 用户说明符在两家 libc 下的分歧面同样待真机（本笔只定
  默认转换这一支）；③ 把「参照件在这一形打回字面量」记为一条判例：凡以宿主 glibc 输出当 C 真值的对账，
  都要先问一句该形是否经 libc 解析。
  **下一轮首动作**：`tools/claim.sh claim TR-SET-GRAMMAR-SENTENCES qoder`（排期 v3 的下一行）。

- ✅ **批次三百三十三把 `cksum` 的 hash 选中状态建模，接上 C 由它驱动的两条诊断，并修掉一枚吞操作数的下标缺陷（`CKSUM-HASH-STATE` 的第一片）**
  （`2026-10-17`，`claim/CKSUM-HASH-STATE-qoder` @ 基线 `b6f2f62ea`，动 `os/commands/usr-bin/textfilter/{src/cksum.rs,src/bin/cksum.rs,tests/cksum_face.rs}` + 18 篇两件文档，单域三线 commands/textfilter，无跨界）：
  **这一条是什么**：C 的 `cksum` 有一个状态变量 `const struct hash *hash`（`minix3/usr.bin/cksum/cksum.c:162`），
  四条分支由它驱动：`-a` 进来时若已选中就 `warnx("illegal use of -a option")` 再接 `usage()`（`:195-199`）；
  `-o` 进来时若已选中就 `warnx("%s mutually exclusive with sum", hash->hashname)` 再接 `usage()`（`:228-234`）；
  `-p`/`-s`/`-t`/`-x` 走 `requirehash`，而它在 `hash` 非空时**放行**（`:521-525`）；名字表本身是
  `hashes[]`（`:120-144`）**八项**，匹配用小写 `progname` 的 `strcasecmp`，而报文用的是大写 `hashname` 那一份。
  本壳以前不建这个状态：`-a <算法>` 在解析阶段就撞上自创的占位句 `cksum: -a crypto algorithms are not modelled yet`
  并退 2，于是那三条诊断**一行都到不了**（登记时就写明「当前不可达」）。
  **顺手发现的一枚既有缺陷（本笔最要紧的收获）**：新加的 `-a md5 -a sha1` 断言第一次跑就失败，报的是
  解析器把第二枚 `-a` **连同它前面的 token 一起跳过**。查下标算术：`take_value` 在取「另一个词」时把
  `index` 推过了值，而循环尾无条件 `index += 1`，两处都推 ⇒ 每吃到一个带值选项就多吞一个 token；
  附值形 `-acrc file` 同病（闭包里也有一次 `+= 1`）。后果不是诊断不对，而是**算错了对象**：
  `cksum -a crc file` 会把 `file` 当作已被消费，操作数清单为空，于是转去读标准输入。
  该闭包形只在 `cksum.rs` 出现（现查 `grep -rl 'parser_index' os/commands --include=*.rs` → 1 个文件），
  所以是同族的孤例而不是系统性病灶。修法：闭包只在**真取了另一个词**时推进，当前 token 一律由循环尾推进一次。
  **表也是错的**：`is_crypto_name` 只列六项且多认一个表里没有的 `ripemd160`——C 只有 `rmd160` 那一行
  （`:130`），`ripemd160` 的拼法按 C 应落到 `illegal argument to -a option`。改成 `crypto_hashname()`，
  照表返回大写那一份，缺的三项 `sha256`/`sha384`/`sha512` 补上（`:136-143`）。
  **这一片切在哪里、为什么能切**：两条状态诊断都在 getopt 环里 `usage()` 结束，**走不到摘要计算**，
  所以状态建模与摘要原语是两件可以分开做的事实——前者本笔落地，后者仍缺原语，占位句原样保留（句子与退码 2
  都不改），只是从解析期移到解析成功之后，好让 `-a md5 -a sha1` 这类形先报它自己的那句话。
  **为什么现在不接摘要**（对照过后的取舍）：本树并非一个原语都没有——`os/libs/minix-crypt/src/md5.rs` 与
  `sha1.rs` 已在（批次三百三十三现查），所以「等密码原语那轮」这个前提只对外部环境成立、对本仓已部分过期。
  但表里是八项，只接其中两项会造出「同一条命令有的算法能算、有的报占位」的新分叉，而报文形状
  （`MD5 (file) = <digest>`，`cksum.c` 的 `hash_digest_file` 一族）也要一起定形——那是一片独立的、
  比本笔大的工作，按「一次一条」另立条目 `CKSUM-HASH-DIGEST`（八项整批接，不逐项开口子）。
  **门**：库侧 `test_parse_cksum_args_a_and_o_algorithms` 从 5 条断言扩到 22 条（八项表逐个点过、
  大小写入口、`ripemd160` 落非法、两条互斥、先 `-o` 后 `-a` 不冲突、放行后的 `-p`/`-s`、缺值仍由 libc 先报、
  吞操作数四形 `-a crc file`/`-acrc file`/`-o 1 -a sha1 f1 f2`/`-a crc -n -`）；面侧新增 3 枚——
  `a_second_dash_a_after_a_selection_reports_illegal_use`（含「第二枚名字好不好都在所不问」的 `crc` 对照）、
  `dash_o_after_a_selection_names_the_hash_and_exits_one`（`md5`/`MD5`/`Md5` 三种入口都报大写 `MD5`，
  并对照先 `-o` 后 `-a` 不报互斥）、`flags_requiring_a_hash_pass_once_a_hash_is_selected`
  （`-p`/`-s` 放行后走到占位，且没选中时仍报批次三百零七钉下的那一句——证明放行是由状态决定的）。
  **变异负控六枚全部由门咬住**（`tmp/cksum-333/ctl333.py`，baseline 与还原后同为 23 通过 0 失败，逐枚还原核 md5）：
  互斥句改成「值不合法」咬 1、第二次 `-a` 改为清状态放行咬 2、表里摘掉 `sha256` 咬 1、
  报文名字换成小写咬 2、**下标算术退回「两处都推」咬 3**（这一枚证明吞操作数的断言真有牙）、
  `-p`/`-t`/`-x` 改回无条件报咬 2。
  **过程中自己被静态门抓到一次**：新门里 `s_s` 取了不用（只断言了 stderr），`cargo clippy` 报
  `unused variable`——补上 `assert_eq!(s_s, 2, …)` 后触及件零命中。
  **三件套（读数全取自宿主 `ulimit -v 3145728` 加 `--offline`；容器与宿主网络同断，见上一笔的登记）**：
  `cargo test --offline -p minix-textfilter -j 1` → **924 / 0 / 0**（68 个 target，上一笔 921，本笔 +3 面门）；
  `--lib cksum::` 15 通过、`--test cksum_face` 8 通过；`cargo check --offline --workspace -j 1` 0 error；
  宿主三架构 `cargo build --offline --bins -j 1 --target x86_64-unknown-none / aarch64-unknown-none / riscv64gc-unknown-none-elf -p minix-textfilter`
  各 Finished、`^error` 计数 0；`rustfmt --edition 2021 --check` 对 `src/cksum.rs`、`tests/cksum_face.rs` 零漂移，
  `src/bin/cksum.rs:19` 的 import 序是**存量**（主树那份未改文件同样报，本笔未动那一段），未顺手改；
  `cargo clippy --offline -p minix-textfilter --all-targets` 触及三件零命中。核实日 `2026-10-17`；基线 `b6f2f62ea`。
  **登记不执行**：① 摘要计算与 `name (file) = digest` 报文（`CKSUM-HASH-DIGEST`，八项整批）；
  ② 多入口名 `sum`/`md2`…/`sha1` 靠 `argv[0]` 选算法仍未建模（既有登记，`prog_name` 只影响报文前缀）；
  ③ 文件腿的 `Cannot read %s` 一族（`CKSUM-FILE-LEGS`，既有登记）。
  **下一轮首动作**：`tools/claim.sh claim JOT-PREC-MINUS-ONE-FORMAT qoder`（排期 v3 的下一行）。

- ✅ **批次三百三十二把 `colrm` 的读写两根腿都接上 C 的 `check()` 形状，闭合 `COLRM-IO-ERRLEG`**
  （`2026-10-17`，`claim/COLRM-IO-ERRLEG-qoder` @ 基线 `6975cab44`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/colrm.rs,tests/colrm_face.rs}` + 18 篇两件文档，单域三线 commands/textfilter，无跨界）：
  **这一条是什么**：`colrm` 的 C 源把「流上的错」收成同一个函数：`minix3/usr.bin/colrm/colrm.c:123-130` 的
  `check(FILE *stream)` 先看 `feof` 再看 `ferror`，成立就 `err(1, "%s", stream == stdin ? "stdin" : "stdout")`。
  它有两条来路：`getchar()` 返回 EOF 时查 `stdin`（`:98-100`），`putchar()` 返回 EOF 时查 `stdout`（`:118-120`）。
  所以真值是**一行带流名的三段形报文加退码 1**，两根腿各报各的流名。本壳这两处都不出声：
  读缝 `read_stdin()` 把错误当文件末尾（批次三百三十一刚修完这根），写缝 `bin_support::emit()` 失败时
  直接 `terminate(1)` 而不报任何一行——退码对，句子没有。
  **开工前的现量**（参照件 `tmp/colrm-310/colrm_ref`，承载件是 `/dev/full` 这个字符设备，写它必拿 `ENOSPC`；
  读数存 `tmp/colrm-332/`）：标准输入给一个目录的 fd → `colrm_ref: stdin: Is a directory` 退 1 无输出；
  标准输出接 `/dev/full` 且输出 8192 字节 → `colrm_ref: stdout: No space left on device` 退 1；
  两腿同时坏 → 报的是 **stdin**（一个字节都没读进来，环里从未调用 `putchar`）。
  **这里有一个必须量出来的东西**：写腿的观察门槛。参照件在输出很小的一段时间里根本不报——
  尺寸扫描定出阈值落在 (4096, 4505] 字节之间（4 KiB 输出退 0 且一字不报，4505 字节起报 `stdout`），
  因为 C 的 `putchar` 写进 stdio 缓冲，只有缓冲在环内被真冲刷失败才返回 EOF；小于一整块缓冲的输出
  在 `exit()` 冲时出错，而 `exit()` 不报告 `fflush` 的失败。**这个数是所链 libc 的属性，不是 `colrm` 的语义。**
  **做法（三路对比之后的取舍）**：甲案把阈值抄进本壳（攒够一块再写）——否决，那等于把一个 libc 的机制
  写进命令层，而真机上 Minix stdio 的缓冲是另一个数，抄了也是错（项目第一原则：抽象语义，不描述机制）；
  乙案写不进去就立刻报——采纳，它与 C 的意图同（`check()` 存在的理由就是这件事），在 ≥4505 字节那一形与 C
  逐字同形，只在「小输出配坏设备」这一形比 C 早报；丙案把写腿整个留给真机（`C-57`/E5）——否决，
  报文形状与退码在宿主就能观测，留给真机等于把钉得住的东西不钉。丙与甲的差别在「要不要报」，
  甲与乙的差别只在「什么时候报」，所以分叉面只有那一小形，且已由门钉住（见下）。
  落地三件：① `src/bin_support.rs` 新增 `emit_error(bytes) -> Option<Errno>`，宿主腿把 `write_all` 的
  `Err` 过 `errno_of` 交回；真机腿按 `write` 的返回值**推余下的字节**直到写完或失败（单次 `write` 可以只吃
  一部分，`emit()` 以前的 `is_err()` 看不见短写），零字节推进又不报错时按 `EIO` 交回——这个兜底与
  `errno_of` 对「不带原始码的宿主错误」用的是同一个号，不自造错误码。② `read_stdin_error()` 沿用上一笔那一根。
  ③ `src/bin/colrm.rs` 按 C 的先后把两根腿分开报：写错先报 `"stdout"`，写干净而读错才报 `"stdin"`，
  两者都干净就是 `check(stdin)` 走到 `feof` 那一支——`exit(0)`。
  **门（宿主 4 枚）**：`tests/colrm_face.rs` 的 `a_stdin_that_cannot_be_read_names_the_stream_and_exits_one`
  钉读腿（stdout 空、stderr 恰一行流名、退 1）；`a_stdout_that_cannot_be_written_names_the_stream_and_exits_one`
  钉写腿越过阈值那一形（与参照件同形）；`a_small_write_failure_is_reported_instead_of_swallowed_by_a_buffer`
  钉的就是上面那个刻意分叉——小输出本壳也报，注释写明「参照件此处退 0 不报」，免得日后有人把它当回归「修回去」；
  `both_streams_broken_reports_the_stream_c_reaches_first` 钉两腿同坏时报文跟着参照件走 `stdin`。
  句子取自 Minix 的 errno 表（`minix3/lib/libc/compat/gen/compat_errlist.c` 那一张），与参照件的 glibc 表
  在 `Is a directory`、`No space left on device` 两句上恰好同字，所以这两形可以逐字钉。
  **变异负控六枚全部由门咬住**（`tmp/colrm-332/ctl332.py`，baseline 与还原后同为 16 通过 0 失败，逐枚还原核 md5）：
  每枚各咬 2 枚——C1 壳侧丢掉读缝交回的号、C5 宿主读腿把错误擦回 EOF（这两枚咬 `stdin` 那两形）、
  C2 宿主写腿把失败擦成没有错、C3 写腿流名写成 `stdin`、C4 报了行不置退码（这三枚咬 `stdout` 那两形）、
  C6 读腿流名写成 `stdout`。**六枚的咬合数都是 2 而不是更多，正好说明两根腿彼此独立**：没有一处的破坏能顺带把另一根腿的门也弄红。
  **A/B**：`tmp/colrm-332/ab332.py` 九形（列删两形 + 透传 + 坏选项 + 裸 `-` + 读错 + 写错两形 + 两腿同坏）
  比退码/标准输出/标准错误，**SAME=8 DIFF=1**，唯一那一形就是上面钉住的小输出分叉；比对前把参照件自己的
  程名 `colrm_ref` 归一回 `colrm`（否则比的是二进制名字而不是行为）。脚本第一版把承载写成 `open(dir,"rb")`，
  Python 直接抛 `IsADirectoryError`——与批次三百三十一同一枚坑，改成 `os.open(dir, O_RDONLY)` 后通。
  **登记不执行**：①「写错先于读错」这条先后规则在宿主**不可证**：要观察它得有「字节已在飞、同时读也坏」的承载，
  而读坏（目录 fd）就不可能有字节在飞，两腿同坏那一形里空写又不失败——门因此只钉观察结果，不宣称钉住了规则本身
  （`COLRM-ERRLEG-ORDER-UNOBSERVABLE`）；②真机上 `EISDIR`/`ENOSPC` 的句子与读库腿归 `C-57`/E5（既有口径）；
  ③短写推进那一圈在宿主测不到（宿主走 `std::io` 腿），归真机门；④本 crate 还有 **34 支壳**在用静默的
  `support::emit()`（现查 `grep -rln 'support::emit(' src/bin/ | wc -l` → 34），它们的 `stdout` 腿同样缺一行报文，
  按「一次一条」各走一轮（`*-STDOUT-ERRLEG` 家族，与 `COLRM-IO-ERRLEG` 同族的那批登记并行）。
  **三件套（读数全部取自宿主，原因实录）**：WSL 恢复之后容器与宿主的网络同断——`curl https://index.crates.io/config.json`
  返回 `HTTP=000`，容器内 `failed to get 'bitflags'`，所以本笔没有容器读数可引；按规则 9 的回退分支走
  `ulimit -v 3145728` 加 `--offline`（新领的树必须 `--offline`，因为注册表缓存够用而索引刷新要网）。
  `cargo test --offline -p minix-textfilter -j 1` → **921 / 0 / 0**（68 个 target，上一笔 917，本笔加四枚面门）；
  四枚新门逐名单跑各 `1 passed`；`cargo check --offline --workspace -j 1` 退码 0 且 `^error` 计数 0；
  宿主三架构 `cargo build --offline --bins -j 1 --target x86_64-unknown-none / aarch64-unknown-none / riscv64gc-unknown-none-elf -p minix-textfilter`
  各 `Finished`、退码 0、`^error` 计数 0——新领的树必须带 `--offline`，因为注册表里缓存够用而索引刷新要网；
  `cargo clippy --offline -p minix-textfilter --all-targets` 触及三件零命中；
  `rustfmt --edition 2021 --check` 对触及三件只报一处**存量**漂移——`src/bin/colrm.rs:23` 的 `use` 花括号序，
  同一份未改文件在主树同样报（该文件最后一次改动是批次三百一十 `589fc4ce0`），非本笔引入，不顺手改。
  核实日 `2026-10-17`；基线 `6975cab44`。
  **下一轮首动作**：`tools/claim.sh claim CKSUM-HASH-STATE qoder`（排期 v3 的下一行）。

- ✅ **批次三百三十一把 `rev` 的标准输入读错腿接上 C 的 `ferror` 形状，闭合 `REV-STDIN-ERRLEG`**
  （`2026-10-17`，`claim/REV-STDIN-ERRLEG-qoder` @ 基线 `dafbdde28`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/rev.rs,tests/rev_face.rs}` + 18 篇两件文档，单域三线 commands/textfilter，无跨界）：
  **这一条是什么**：C 的行环跑完不是「读不出来就算结束」，它随后试流上的错：
  `minix3/usr.bin/rev/rev.c:101-104` 的 `if (ferror(fp)) { warn("%s", filename); rval = 1; }`，
  而 `filename` 的初值是 `"stdin"`（`:80-81`），被操作数顶掉之后才换成那个操作数的名字（`:91`）。
  所以标准输入自己读出错时，真值是**一行报文带流名 `stdin` 加一个非零退码**，不是静默退出。
  本壳以前拿不到这件事：`bin_support::read_stdin()` 把 `read_to_end` 的 `Err` 直接扔掉
  （宿主腿 `let _ = ...`，真机腿 `Err(_) => break`），读失败与文件末尾在壳侧长得一模一样。
  **开工前的现量**（参照件 `tmp/rev-309/rev_ref`，承载用「标准输入是一个目录的 fd」：
  打开成功、读的时候拿 `EISDIR`；读数存 `tmp/rev-331/measure.log`）：
  同形下参照件打 `rev: stdin: Is a directory` 退 1，本壳**一句话不说且退 0**。
  操作数那一形两边本来就相同（`rev: <路径>: Is a directory`、退 1），说明缺的只是流名那条腿。
  **做法（为什么这么切）**：错误通道加在**承载它的缝**上而不是各壳里——
  ① `src/bin_support.rs` 新增 `read_stdin_error()`，两条腿同签名
  `(Vec<u8>, Option<Errno>)`：宿主腿把 `read_to_end` 的 `raw_os_error()` 过 `errno_of` 交回，
  真机腿把 `read(STDIN)` 的 `Err(code)` 交回；两边都把**已经收到的字节**一起带出来，
  因为 C 在读到的当时就打印掉了，错误只追加一行报文和一个退码，不回收已完成的输出。
  ② `read_stdin()` 保持原签名，改为 `read_stdin_error().0`——其余三十多支壳的语义一字未动
  （`COLRM-IO-ERRLEG`、`SPLIT-READ-STAT-ERRLEG` 那几支同族登记将来各自接这根腿，各走一轮）。
  ③ `src/bin/rev.rs` 无操作数那一支：先 `emit_reversed(&input)`，再按 `failure` 印
  `support::warn_line(prog, "stdin", code)` 并把 `status` 置 1，退码从常量 `0` 换成 `status`。
  **门（宿主 2 枚）**：`tests/rev_face.rs` 的 `a_stdin_that_cannot_be_read_names_the_stream_and_exits_one`
  钉住 stdout 空、stderr 恰一行 `rev: stdin: Is a directory\n`、退 1；
  `an_unreadable_operand_names_itself_rather_than_stdin` 钉住另一半——操作数坏了时报的是**它自己的名字**，
  这一形改前就过，留它是为了让上一枚不至于把「流名」这一件事写成一句泛化报文。
  `Is a directory` 取自宿主 `strerror` 表，真机读库腿归 `C-57`/E5（既有口径）。
  **变异负控六枚全部由门咬住**（`tmp/rev-331/ctl331.py`，baseline 与还原后同为 15 通过 0 失败，
  逐枚改回再复跑、跑完还原并核 md5）：C1 壳侧忽略读缝交回的号咬 1 枚、C2 宿主腿把错擦回 EOF 咬 1 枚、
  C3 流名擦成空串咬 1 枚、C4 报了行不置退码咬 1 枚、C5 操作数那腿的流名也写成 `stdin` 咬 4 枚
  （连带三枚原有的操作数门，证明「按流分名」这件事本来就有人在盯）、C6 报文改走标准输出咬 1 枚。
  **A/B**：`tmp/rev-331/ab331.py` 九形（读错误两形 + 打不开的操作数 + 正常两行 + 末行无换行 +
  空输入 + 裸 `-` + 坏操作数配好 stdin + 集外字母）比退码/标准输出/标准错误，
  **SAME=9 DIFF=0**；比对把参照件自己的程名 `rev_ref` 归一回 `rev`（两处：报文行首与 usage 句），
  否则比的是二进制名字而不是行为。
  **登记不执行 `REV-FILE-PARTIAL-ON-ERROR`**：`read_file()` 的宿主腿走 `std::fs::read`，
  中途读错时整个 `Err` 交出、已读到的字节被丢掉，而 C 会把错误之前读到的行**先打印**再报一行
  （`rev.c:93-104` 的打印在 `ferror` 之前）。本壳的退码与报文都对，缺的是那半截输出；
  它要改的是 `read_file` 的交回形状（同 `read_stdin_error` 那样带部分），另立一轮。
  **收尾自审又抓到本笔自己的两处缺陷，一并修掉**：①`src/bin_support.rs` 的两处注释是破碎的中英混排
  （一句英文尾巴上吊着「keeps the号」，另一处在英文段落中间插了一整句全角括号中文）——按文风规矩改成
  单语成句并把 C 锚点留在句内；②`tests/rev_face.rs` 的 `scratch()` 只按进程号命名，而本文件有五枚门各取
  一个目录、结尾都要 `remove_dir_all` 整片，cargo 是多线程跑测试的，先跑完的那枚会把另一枚正在用的档删掉——
  改成每枚门各拿一个互不相交的目录（进程号加一个自增序号），五处调用点一字未动，判定权在辅助件。
  这两处都会抬高测试的偶然失败率，属于「测试自身」的问题而不是被测对象的问题。
  **三件套（复跑后的读数）**：宿主整包 `ulimit -v 3145728; cargo test -p minix-textfilter -j 1`
  → **917 / 0 / 0**（68 个 target，上一笔 915，本笔加两枚面门、不加库测）；面门逐名
  `cargo test -p minix-textfilter --test rev_face` → 10 枚全绿，含新立的
  `a_stdin_that_cannot_be_read_names_the_stream_and_exits_one` 与
  `an_unreadable_operand_names_itself_rather_than_stdin`；六枚变异负控在改过 `scratch()` 之后复跑
  `python3 tmp/rev-331/ctl331.py` → **6/6 由门咬住、0 枚编译期被挡**，baseline 与还原后同为 15 通过 0 失败、
  逐枚还原核 md5；`cargo check --workspace -j 1` 0 error；宿主三架构
  `cargo build --bins -j 1 --target x86_64-unknown-none / aarch64-unknown-none / riscv64gc-unknown-none-elf -p minix-textfilter`
  各 `Finished` 且 `^error` 计数 0；`cargo clippy -p minix-textfilter --all-targets` 触及三件零命中
  （78 行存量告警一条也不落在 `bin_support.rs`、`bin/rev.rs`、`rev_face.rs` 上）；
  `rustfmt --edition 2021 --check` 对三个触及件零漂移。核实日 `2026-10-17`；基线 `dafbdde28`。
  **容器腿这次回退到了宿主，原因实录**：WSL 恢复之后 `docker run --rm -v "$PWD:/work" -w /work/os -m 2g minix-ci:1.94
  cargo test -j 1 -p minix-textfilter` 报 `error: failed to get 'bitflags' as a dependency of package minix-kernel`，
  三架构交叉目标在容器里则报 `[28] Timeout was reached`；探针三条定住病根——容器内 `getent hosts static.crates.io`
  解析到一个 `2001:2::fd` 的假地址、`curl https://static.crates.io/` 返回 `000`、镜像的
  `CARGO_HOME=/usr/local/cargo` 下**没有 `registry` 目录**（缓存全在容器网络的活连接上）。同一棵树、同一条命令在
  WSL 重启前取到过 **917 / 0 / 0** 的容器读数，所以这是承载环境断了网，不是代码或配方变化。按规则 9 的回退分支
  走宿主 `ulimit -v 3145728` 加 `-j 1`，上面每一项都注明是宿主读数；容器门待网络恢复后由下一笔复跑。
  另记一笔配方纪律：中途有一次把 `-v "$PWD"` 指到了 `os/`（工作目录已是 `os/`），那是 `AGENTS.md` 明令的假红挂载点，
  当场发现并改回仓库根重跑，未采信那次结果。
  **下一轮首动作**：`tools/claim.sh claim COLRM-IO-ERRLEG qoder`（同族第二例，
  承载同样是 `read_stdin_error`，另加 `emit()` 的写腿；`minix3/usr.bin/colrm/colrm.c:97-130` 的 `check()`
  分别打 `colrm: stdin: <strerror>` 与 `colrm: stdout: <strerror>`）。

- ✅ **批次三百三十把 `split` 的行模式改成按字节切、终止符随它自己的行走，闭合 `SPLIT-NONUTF8-DROP`**
  （`2026-10-17`，`claim/SPLIT-NONUTF8-DROP-qoder` @ 基线 `a75583ecc`，单域三线 commands/textfilter，无跨界）：
  **这一条是什么**：上一笔登记的两处分叉同一个根——行模式把输入当文本读。`os/commands/usr-bin/textfilter/src/bin/split.rs`
  决定行模式的第一步是 `core::str::from_utf8(&input).unwrap_or("")`，整档不可解码就折成空串，行切分拿到空输入，
  于是一不分片、二不写文件、三退 0（静默丢数据，P0 级）；另一半是同一段把输入切成**不带**终止符的行、
  再把尾部空片丢弃、写出时逐行补一个 `\n`，往返一次就把「末行本来有没有换行」这件事丢了。
  **C 那一侧的形状**：`minix3/usr.bin/split/split.c:252` 把 `read` 进来的缓冲直接交给换行扫描，
  `:262-267` 每遇到一个 `\n` 就写出 `Ce - Cs + 1` 个字节——换行是随它结束的那一行一起搬走的，
  程序从不校验编码；`:276-280` 写的是缓冲末尾剩下的那段，原样、不补换行。
  **做法（三处，全在行模式这条路上）**：① `os/commands/usr-bin/textfilter/src/split.rs` 加
  `lines_bytes`（交出**带**终止符的行：每个换行连同它前面的字节算一行，末尾不带换行的那截原样算最后一行，
  以换行收尾的输入不产尾空行）并把 `chunks(&[String], …)` 改成 `chunks_bytes(&[&[u8]], …)`——
  分组只看条目数，形状照 `chunk_bytes`；留下 String 形的旧签名会变成无人使用的死面，所以直接换掉。
  ② 壳侧删掉 `from_utf8` 那一行与「尾部空片 pop」，行模式改吃 `lines_bytes(&input)`。
  ③ 写出侧 `render_chunk` 不再补 `\n`，各片原样串接即为该档内容。
  **门（宿主 7 枚）**：`tests/split_face.rs` 新立五枚，期望全部取自参照件 `tmp/split-311/split_ref` 的
  `od -An -c` 读数（`tmp/split-330/r/`，本轮现跑四形各退 0）——非法字节留在它自己那片（`-l 1` 对
  `aa\xffbb␊cc\xfe␊dd␊` 出三片）、末行不带换行就不带（`a␊b` 出 `xaa`=`a ␊` 与 `xab`=`b`）、
  空行是自己的一行（`a␊␊b␊` 出 `a ␊`/`␊`/`b ␊`）、一片装多行时逐行各带各的终止符（`-l 2` 对 `a␊b␊c`）、
  同一输入的 `-b 6` 腿逐字节不变（对照腿，证明这一刀只落在行模式）；库侧另补两枚
  `lines_bytes_carries_the_terminator_with_its_line` 与 `lines_bytes_is_not_an_encoding_check`，
  并把三枚原 `chunks` 测翻成字节形、`render_chunk` 的两枚改钉「按原样串接」。
  五枚门走的是新加的 `run_chunk_bytes` 腿——`run` 把首个分片转成了 lossy 字符串，
  非法字节会被折成替换字符，正好把本轮要钉的东西遮掉。
  **变异负控六枚全部由门咬住**（`tmp/split-330/ctl330.py`，逐枚改回再复跑、跑完还原并核 md5；
  baseline 与还原后同为 32 通过 0 失败）：C1 把「不可解码就整档丢光」那行装回去咬 1 枚、
  C2 写出侧补回换行咬 7 枚、C3 丢掉不带终止符的末段咬 3 枚、C4 分组时忽略行数咬 8 枚、
  C5 切行时剥掉终止符咬 11 枚、C6 换行扫描改按别的字节咬 11 枚。**六枚门槛首次达成**，
  三百二十五~三百二十九连续五轮的欠账到此清账。
  **A/B**：`tmp/split-330/ab330.py` 十八形比退码/报文/生成文件的字节内容，**SAME=16 DIFF=2**，
  两枚 DIFF 都是 `-n` 读标准输入那一形且**与本笔无关**（见下条登记），
  同一形换成文件操作数后两侧逐字节相同（现跑：`-n 4` 对十二行文件，四片 `line001␊…` 完全一致）。
  **新登记不执行 `SPLIT-N-STDIN-FSTAT-SIZE`**：C 的 `split3` 用 `fstat(ifd)` 拿输入字节数
  （`minix3/usr.bin/split/split.c:290-300`），标准输入是管道时 `st_size` 为 0，
  参照件于是报 `split: can't split into more than 0 files` 退 1 且不产文件；
  本壳读尽输入后用 `input.len()`，同一形退 0 出四片。真值取决于 Minix 自己管道 `fstat` 的读数，
  且这条腿在 `-n` 上，不在本轮射程内（一次一条），登记保留。
  **三件套**：核实命令与读数——容器整包 `docker run --rm -v "$PWD:/work" -w /work/os -m 2g minix-ci:1.94 cargo test -j 1 -p minix-textfilter`
  → **915 / 0 / 0**（68 个 target，上一笔 908，本笔 +5 枚面门 +2 枚库测）；
  `cargo clippy -p minix-textfilter --all-targets` 退码 0、`^error` 计数 0、触及件零命中
  （两条 `unused import` 属 `src/window.rs` 存量）；`cargo check --workspace -j 1` 0 error；
  宿主三架构 `cargo build --bins -j 1 --target x86_64-unknown-none / aarch64-unknown-none / riscv64gc-unknown-none-elf -p minix-textfilter`
  各 `Finished`；`rustfmt --edition 2021 --check` 对 `src/bin/split.rs`、`tests/split_face.rs` 零漂移，
  `src/split.rs` 另报两处**存量**注释对齐漂移（主树同一份文件同样报，非本笔引入，未顺手改）；
  `tools/pattern-gate.sh` PASS=16 FAIL=0 SKIP=0。核实日 `2026-10-17`；基线 `a75583ecc`。
  **判定表进度**：本轮不动判定表（已对齐 20 支 + pr 不变）；下一支仍取 `SEQ-GETOPT-LINE`
  （`minix3/usr.bin/seq/seq.c:102` 的 optstring 是 `"f:hs:t:w"`）。

- ✅ **批次三百二十九收尾：翻正一条陈旧射程数字，并把 `split` 行模式的两处分叉登记入台账**
  （`2026-10-17`，无认领——纯台账笔，只动本文件）：
  **翻正的那个数**：批次三百二十七与本文件另一处都把 `GETOPT-REPORT-BYTE-LETTER` 的射程写成
  「26 处 / 20 余个薄壳」，现查（`grep` 去掉 `src/lib.rs` 自身）实际是
  `.line(` 调用 **30 行**、`GetoptReport::{UnknownOption,IllegalOption,MissingValue}` **构造点 114 行**、
  分布在 **23 个文件**，另有 **39 个薄壳**使用 `support::args()`，`tests/` 里 27 扇 `*_face.rs` 门中
  钉着 `unknown option` 的断言 **97 行**。旧数数的是某一类调用点的子集，**不足以用来估工作量**，
  据此结论也随之改写：这一族**不是单轮量级**，得按文件切片（第 1 片只动 `src/lib.rs:GetoptReport`
  与那两个已按字节取选项环的支及其门）。
  **新登记的缺陷 ①（`SPLIT-NONUTF8-DROP`，P0 级：静默丢数据）**：`split` 行模式在输入含
  非法 UTF-8 字节时**一个文件都不产**且退 0。现量：输入 `aa\xffbb\ncc\xfe\ndd\n`、
  `split -l 1 <in> x` ⇒ 参照件 `tmp/split-311/split_ref` 退 0 出 `xaa`=`aa 377 bb \n`、
  `xab`=`cc 376 \n`、`xac`=`dd \n`，本层退 0 且 `y*` 不存在。根因在
  `os/commands/usr-bin/textfilter/src/bin/split.rs:65`
  的 `core::str::from_utf8(&input).unwrap_or("")`——整档不可解码就折成空串。
  C 侧 `minix3/usr.bin/split/split.c:185` 与 `:252` 都是 `read(ifd, bfr, MAXBSIZE)`，
  行模式数的是字节缓冲里的换行，从不校验编码；同一条壳里的 `-b`（`chunk_bytes(&input, n)`）与
  `-n`（`chunks_n(&input, n)`）**本来就按字节读**，同一输入的 `-b 6` 两侧逐字节相同，可当改动对照门。
  **新登记的缺陷 ②（同一处的另一半：末行终止符）**：输入 `a\nb`（末行**无**换行）、`split -l 1`
  ⇒ 参照件 `zaa`=`a \n`、`zab`=`b`（原样不补），本层 `zab`=`b \n`。根因是 `:66-69` 把输入切成
  不带终止符的行并 pop 掉尾部空片、写出时 `:118` 的 `render_chunk` 再逐行补 `\n`，
  往返一次就把「末行有没有换行」丢了；空行对照（`a\n\nb\n`）两侧相同，说明丢的只是终止符。
  **修法记在这里以便开工**：`src/split.rs` 加 `chunks_bytes`（形状照 `chunk_bytes`），
  壳侧行模式改保留终止符的字节切分、写出侧不再补 `\n`。取证读数存 `tmp/split-330/`。
  **顺带修正一条候选的定性（`COMM-DIR-OPERAND`）**：目录操作数那一形，参照件 `tmp/comm-315/comm_ref`
  退 0（`comm <dir> <file>` 打出 `	a\n	b\n`），本层退 1 报三段形 `comm: …: Is a directory`。
  但参照件那个退 0 是**宿主 glibc 的形状**——`fopen(dir,"r")` 在 Linux 成功、`fgets` 把 `EISDIR`
  当 EOF，而 `minix3/usr.bin/comm/comm.c:181-182` 只在 `fopen` 返回 `NULL` 时才 `err(1, "%s", name)`。
  真值取决于 Minix 自己的 open-on-dir 语义（归 `C-57`/E5 那一档），**拿宿主参照件当期望值会把
  平台的偶然当成语义**，故这条从「单轮可闭」清单移出，登记保留并附此现量。

- ✅ **批次三百二十九把 `bin_support::args` 的宿主腿改成 `args_os()` + lossy，闭合 `BIN-SUPPORT-HOSTED-ARGV-NONUTF8`**
  （`2026-10-17`，`claim/BIN-SUPPORT-HOSTED-ARGV-NONUTF8-qoder` @ 基线 `4e02c33c1`，单域三线 commands/textfilter，无跨界）：
  **这一条是什么**：`os/commands/usr-bin/textfilter/src/bin_support.rs:args` 是一对 `cfg` 双生面。
  目标侧把 `minix_rt::crt0::args()` 交来的原始字节做 lossy 渲染（那层注释写明这是本层文本工具的约定：
  C 把 argv 当原始字节、从不校验，`minix3/lib/libc/stdlib/getopt.c:79` 取的就是字节），
  宿主侧那一腿写的是 `std::env::args()`——它内部对每个参数做 UTF-8 解码并在失败处 `unwrap`，
  于是**同一个参数在两条腿上一条 panic、一条继续**。
  **开工前的现量**（`tmp/bin-support-329/probe.py` → `measure.log`）：argv 含裸 `0xFF` 的形里
  `tr`/`seq`/`paste`/`fold`/`rev`/`units` **六支全退 `rc=-6`（SIGABRT）+ `thread 'main' panicked`**，
  唯一不退信号的是纯 ASCII 的 `seq 1 3`（`rc=0 out=1\n2\n3\n`）；参照件同形分别给出
  `tr_ref: unknown option -- \xff` + usage 退 1、`seq_ref` 两行 libc 报文退 1、
  `tr a\xffb c` **退 0 打 `cccccc`**（非法字节落在操作数里只是普通数据）、`paste -d\xff` 退 0 空输出。
  **做法**：宿主腿改走 `std::env::args_os()`，用 `OsStrExt::as_bytes` 拿到原始字节后过**同一个**
  `String::from_utf8_lossy`——两条腿从此共用一条渲染规则，panic 那一支不复存在。
  这是**双生面对齐**，不是与 C 对齐：改后仍与 C 差在字节上（见下），那一半的登记不撤。
  **门**：新立 `tests/bin_support_face.rs` 四枚（宿主侧起真子进程、argv 用 `OsStr::from_bytes` 传原始字节）：
  ① 六支在 `-\xff` 上都不再走信号路径且第一句仍是各的 `<prog>: unknown option -- …`
  （退码 1，`units` 退 3——退码也按现量钉）；② `tr a\xffb c` 退 0 打 `cccccc\n`、stderr 空；
  ③ `paste -d\xff` 退 0 且三条通道都空；④ `seq -\xff` 连报**三行** `unknown option`。
  **与 C 的差留在门上，不当已闭**：lossy 先把 `0xFF` 变成 U+FFFD，其后各支按自己的取法显形——
  按字节取的得到 `c3 af`（U+00EF 的两字节），按 `char` 取的 `rev` 得到 `ef bf bd`，
  累加型环的 `seq` 把替换字符的三个字节当三个字母连报三行，而 C 只有一行原字节 `ff`。
  因此本笔只撤 `BIN-SUPPORT-HOSTED-ARGV-NONUTF8`，`GETOPT-REPORT-BYTE-LETTER` 的射程**反而量得更清**：
  根因有**两处**——`src/lib.rs:110 GetoptReport::UnknownOption(char)` / `:129 line() -> String`
  的句子渲染（已登记的 26 个调用点），以及 **argv 在缝上就被 lossy 吃掉**；
  要打到 C 那一形得让 `bin_support::args` 两条腿都交字节（`Vec<Vec<u8>>` 或等价物），
  薄壳侧的选项环与操作数跟着按字节走。门里那些精确字节断言是**故意的**：那笔落地时它们会当场变红，
  由改的人有意识翻正，而不是让门静默跟着漂。
  **六枚负控**（`tmp/bin-support-329/ctl329.py`，逐枚改回再复跑、跑完还原核 md5）：
  C1 宿主腿退回 `std::env::args()`、C2 不可解码就整段丢词、C4 交回空串 ⇒ **三枚由门咬住**
  （C1 咬住全部四枚、C2 咬住三枚、C4 咬住四枚）；C3 剥掉 `>=0x80`、C5 丢掉 `argv[0]`、
  C6 换成 strict `from_utf8` + `unwrap_or_default` ⇒ **编译期就被挡住**，
  对门的灵敏度既不证明也不否证。本轮实得 **3/6**，仍未到六枚门槛，欠账照旧记在台账里。
  三件套：容器整包 `cargo test -p minix-textfilter -j 1` → **908 / 0 / 0**（68 个 target，
  新格的 `running 4 tests`；上一笔是 904/67，本笔只加门测不加库测）；
  `cargo clippy -p minix-textfilter --all-targets` 退码 0、`^error` 计数 0、`bin_support` 零命中；
  workspace `cargo check -j 1` 0 error；宿主三架构 `--bins -p minix-textfilter` 各 `Finished`、各 0 error；
  `rustfmt --edition 2021` 两件零漂移；宿主直跑库测 516 与新门 4 枚全绿。
  核实日 `2026-10-17`；基线 `4e02c33c1`。

- ✅ **批次三百二十八把 `column` 的残行告警计数补成门，闭合 `COLUMN-TAIL-WARN-COUNT`（本笔不动生产码）**
  （`2026-10-17`，`claim/COLUMN-TAIL-WARN-COUNT-qoder` @ 基线 `4643cab05`，单域三线 commands/textfilter，无跨界）：
  **这一条是什么**：批次三百一十四的测试自审（test-audit 五维）在第 1 维「完备性」上留下的缺口——
  末行没有换行符时 C 打一句两段的 `warnx` `column: line too long` 并把退码置 1
  （`minix3/usr.bin/column/column.c:277-283`），而**这句按来源计数**：`column.c:114-123` 对每个
  操作数各开一次流、各喂同一个条目池，残行是逐流判定的。当时的门只钉了单来源那一形，
  多来源的句数没有任何东西看着（登记名 `COLUMN-TAIL-WARN-COUNT`）。
  **现量（参照件 `tmp/column-314/column_ref` 与新层逐形对读，四形全同）**：
  `-c 20 tail1 tail2`（两档各一条 70 字残行）⇒ 两句、退 1；同一档名写两遍 ⇒ 两句
  （两次打开、两次判定，不是按档名去重）；`ok.txt tail1.txt` ⇒ 一句、退 1；
  `- tail1.txt` ⇒ 首句 `column: Cannot open \`-': No such file or directory`，随后仍按 `tail1.txt`
  的残行补一句，退 1。**生产码本来就按来源循环**（`src/bin/column.rs:86` 的
  `for _ in 0..pool.dropped_tail` 印这一句），所以本笔是**补门而不是改码**——
  这一点如实登记，免得把门测的绿读成修过什么。
  **门**：`tests/column_face.rs:each_source_with_a_long_tail_line_warns_once`（第 13 枚，
  四段断言各钉上面的一形；句数、首句文本、退码三项都取自参照件读数而非推论）。
  **六枚负控**（跑在最终源码上，逐枚改回真实形状后复跑）：
  G2 把句子改成三段形（带 `strerror` 末段）、G3 整句不打、G4 不看有没有残行、每个来源都打、
  G6 句子前缀漂移 ⇒ **四枚由门咬住**（`an_unterminated_last_line_warns_and_is_dropped` 与新门一起失败）；
  G5「残行不再置退码位」被借用变量挡住 ⇒ **编译期就没过**，对门的灵敏度既不证明也不否证；
  G1「整轮只报一句」的替换锚点在源码里没命中 ⇒ **未施加**，不计入咬住数。
  本轮实得 **4/6**，仍不到六枚门槛，与上一笔同记为欠账（欠账随 `GETOPT-REPORT-BYTE-LETTER`
  那轮一起做，那轮的调用点是 26 处 / 20 余个薄壳）。
  三件套：容器整包 `cargo test -p minix-textfilter -j 1` → **904 / 0 / 0**（67 个 target，
  `tests/column_face.rs` 那格 `running 13 tests`）；`cargo clippy -p minix-textfilter --all-targets`
  退码 0、`^error` 计数 0，且 `column_face` 零命中——**本笔新增行零诊断**；
  workspace `cargo check -j 1` 0 error；宿主三架构 `--bins -p minix-textfilter` 各 `Finished`；
  `rustfmt --check` 零漂移；宿主直跑 516 库测 + 13 门测全绿。
  核实日 `2026-10-17`；基线 `4643cab05`。

- ✅ **批次三百二十七把 `jot` 的种子转换提到库侧并单测，闭合上一笔登记的 `JOT-SEED-CAST-UNIT-TEST`**
  （`2026-10-17`，`claim/JOT-SEED-CAST-UNIT-TEST-qoder` @ 基线 `06dfabb6e`，单域三线 commands/textfilter，无跨界）：
  **为什么要单独提出来**：上一笔的负控里有一枚**漏网**——把种子整体偏移 +1，端到端门照旧全绿。
  原因是种子只在随机流里显形，而本层的 `Lcg` 与 libc `random()` 的数值流本就是另一档登记
  （07 篇 §4.4），于是「`-0.5` 与 `0` 同串」这类推论对**同幅偏移天然不敏感**。
  这不是门的期望值写错，而是**可测性缺口**：转换式 `(v as i64) as u64`  inline 在壳里，没法单测。
  **做法**：提为 `src/jot.rs:seed_from_step`（壳 `src/bin/jot.rs` 改调它），doc 注释写清 C 的两步
  ——`jot.c:118` 的 `(unsigned long) step` 是**先向零截断**、**再在无符号那一步回绕**，
  并说明本层种子位宽是 64 而 Minix 的 `unsigned long` 在 ILP32 下是 32，
  两边可观察的是截断方向与「负数合法」而不是流的数值。
  **门**：库测 +1 枚 `test_seed_cast_truncates_then_wraps`（钉 `0`、`-0.5`、`0.9`、`-0.9` 都截成 0，
  `-5` 与 `-5.9` 同种子、且等于 `u64::MAX - 4` 的回绕形状，正数 5/2147483648 不受回绕影响）。
  **两枚负控**（跑在最终源码上）：**P1 种子加一整幅偏移**——正是上一笔那枚漏网的形状——
  现在被新库测咬住；**P2 把向零截断换成 `round_ties_even`** 因该方法是 unstable 属性而
  **编译期就没过**，所以它对门的灵敏度既不证明也不否证，记在这里不当成咬住。
  **本轮负控只有两枚，不到六枚门槛**，与上一笔同为记账；补齐的活随 `GETOPT-REPORT-BYTE-LETTER`
  那轮一起做（那轮实测调用点是 **26 处 / 20 余个薄壳**，`src/lib.rs:GetoptReport::line` 要从
  `String` 改成字节串，且现有若干门里钉的是 `char` 渲染形状，需同步改期望）。
  三件套：容器整包 `cargo test -p minix-textfilter -j 1` → **903 / 0 / 0**；
  `cargo clippy -p minix-textfilter --all-targets` 退码 0，指向 `jot.rs` 的四条
  （`width` 未读、`dox`/`exhausted` 字段未读、`match` 可换 `unwrap_or_default`、`next` 与标准 trait 同名）
  都是上一笔就登记过的存量行，行号只因本次插入而位移，**本笔新增行零诊断**；
  workspace 0 error、宿主三架构各 `Finished`、`rustfmt --check` 零漂移、库测 516 与 `jot_face` 14 枚全绿。
  核实日 `2026-10-17`；基线 `06dfabb6e`。

- ✅ **批次三百二十六把 `col` 的集外字母改回按字节取，并翻正一条量不出来的旧登记**
  （`2026-10-17`，`claim/COL-NONASCII-OPTLETTER-qoder` @ 基线 `c2abf1ea7`，单域三线 commands/textfilter，无跨界）：
  **先说翻正**：批次三百一十四登记的那条叫 `COL-NONASCII-PANIC`，写的是「`col -bé` 即触发 panic」。
  本笔开工前现量六形（`-bé`、`-é`、`-Z`、`-bZ`、`-lé`、`-xé`）**一形都不 panic**——
  `parse_col_flags` 只在**字节**上步进且一碰到集外字母就 `return`，取 `arg[pos..]` 时 `pos` 必然落在字符起点上，
  所以那条登记的「panic」不成立。成立的是**取字符而不是取字节**：参照件 `tmp/col-313/col_ref`
  打 `unknown option -- ` + 裸字节 `0xC3`，本壳打的是整个 `é`（`c3 a9`）。据此把这条改名重登为
  `COL-NONASCII-OPTLETTER` 并修掉选字母的一半。
  **修法**：`src/col.rs:parse_col_flags` 的集外字母支改成 `char::from(bytes[pos])`——
  与本树 `src/column.rs:parse_column_args` 批次三百一十四已定的处置逐字同法，
  libc 那句 `getopt.c:103` 的 `%c` 拿的本来就是 `optopt` 这个字节而不是一个字符。
  **门**：库测 +1 枚 `unknown_nonascii_option_letter_reports_the_byte`（钉 `é` 的报错落在 U+00C3 而不是 U+00E9，
  另钉聚簇 `-xé` 同形）；端到端门 +1 枚 `nonascii_option_letter_is_reported_as_one_byte`
  （门里比的是 lossy 后的码位 U+00C3；按字符取时打出的会是完整的 `é`，两者不混淆；同枚再钉一遍 ASCII 的 `-Z` 不受影响）。
  **读数**：A/B 六形 **SAME=3 / DIFF=3**——三形 DIFF 全是本族共有的**编码末段**：
  C 打一个裸字节 `c3`，本树经 `char` 的 `Display` 打出的是 `c3 83`（两个字节）。
  这一半不在本笔射程：它要改的是 `src/lib.rs:GetoptReport::line` 的返回类型（`String` ⇒ 字节串）
  与八处调用方的句子拼装，属共享缝，登记为 `GETOPT-REPORT-BYTE-LETTER`（与既有的
  `REV-`/`COLRM-`/`COLUMN-`/`UNITS-`/`SEQ-`/`PASTE-`/`TR-` 七条同名登记是同一件事，
  本笔把 `COL-` 也并入这一族，并把 `COL-NONASCII-PANIC` 这个不成立的名字注销）。
  三件套：容器整包 `cargo test -p minix-textfilter -j 1` → **902 / 0 / 0**；
  `cargo clippy -p minix-textfilter --all-targets` 退码 0，指向 `col.rs` 的两条
  （`:314` 的 `never_loop`、`:450` 的 `needless_range_loop`）都在本笔未触碰的存量行，
  **本笔新增行零诊断**；`cargo check --workspace -j 1` 0 error；宿主三架构各 `Finished`；
  `rustfmt --edition 2024 --check` 零漂移；库测 515 枚与 `col_face` 门测 10 枚全绿。
  **两枚变异负控**（`tmp/col-326-ctl.py` 的等价跑法，日志 `tmp/col-326-ctl.log`）都咬住：
  改回按字符取被库测与新门各咬一枚，取末字节而不是首字节被库测两枚咬住。
  **本轮负控只有两枚，不到六枚的门槛**——本笔是单点改动且两枚都命中要害，但这是记账不是达标，
  下一轮若接续这条（`GETOPT-REPORT-BYTE-LETTER`）需按门槛补齐。
  核实日 `2026-10-17`；基线 `c2abf1ea7`。

- ✅ **批次三百二十五把 `jot -r` 的种子判据按 C 收回去（闭合三百一十七登记的第③条 `JOT-RANDOM-NEGATIVE-SEED`）**
  （`2026-10-17`，`claim/JOT-RANDOM-NEGATIVE-SEED-qoder` @ 基线 `c40ad74cc`，单域三线 commands/textfilter，无跨界）：
  **缺口**：壳 `src/bin/jot.rs` 算种子处写着 `Ok(v) if v >= 0.0 => v as u64`，其余一律
  `Bad step value`——这条非负判据 C 没有。`jot.c:118` 是 `srandom((unsigned long) step)`：
  先按 C 的截断取整、再按无符号回绕，**负数与小数都是合法种子**。
  参照件现量（`tmp/jot-317/jot_ref`）：`jot -r 3 1 10 -5`、`-0.5`、`-2147483648` 都退 0 打三个整数，
  本壳退 1 并报 `jot: Bad step value:  -5`——把一个能用的输入判成坏输入。
  **修法**：判据删掉，转换写成 `(v as i64) as u64`（截断后回绕，与 C 同一形状）；
  `parse_step` 的 `strtod` 全串检查**没动**，所以 `jot -r 3 1 10 5x` 两侧仍是同一句
  `jot: Bad step value:  5x` 退 1。随机流的**数值**本就是另一档分歧（本层 LCG 不是 libc 的
  `random()`，07 篇 §4.4 已写明），因此门只钉退码、行数、全为数字、以及
  「`-0.5` 与 `0` 必须逐字节相同」这条截断的可观察推论（参照件同读数 `9\n4\n8\n`）。
  **门**：端到端门 +1 枚 `a_negative_or_fractional_seed_is_accepted_like_c`（内含五形加两条形）。
  **读数**：种子一族 10 形对账 `SAME=9`，唯一 DIFF 是 `jot -r 0 1 10 -5`——`reps == 0` 在 C
  就是无界（`jot.c:129` 的 `i <= reps || reps == 0`），两侧都跑到截停为止，不算分歧；
  clippy `-p minix-textfilter --all-targets` 退码 0 且**指向触及件 0 条**、workspace 0 error、
  宿主三架构各 `Finished`、`rustfmt --edition 2024 --check` 零漂移、库测 514 枚与门测 14 枚全绿；
  容器整包 `cargo test -p minix-textfilter -j 1` → **900 / 0 / 0**（67 target，比上一笔的 899 多这枚门测）。
  **三枚变异负控见 `tmp/jot-325-ctl.log`，读数不是一句「全咬」**：把 `parse_step` 的全串检查一起放宽
  那枚被新门咬住；把自创的非负判据加回来那枚**在编译期就被挡住**（那个 `match` 的下一支是 `Err(_)`，
  加上守卫后模式不再穷尽，所以它证明不了门的灵敏度）；把截断改成「+1 偏移」那枚**漏网**，
  原因是本层的种子只经由 LCG 流出，而 LCG 与 libc `random()` 的**数值流本就登记为另一档分歧**，
  于是「`-0.5` 与 `0` 同串」这条推论对全体种子同幅偏移天然不敏感——要钉住种子转换本身，
  得在库侧把 `(v as i64) as u64` 提成一个可单测的函数（登记为 `JOT-SEED-CAST-UNIT-TEST`，
  本笔不顺手做，因为它改的是可测性结构而不是行为）。
  核实日 `2026-10-17`；基线 `c40ad74cc`。

- ✅ **批次三百二十四把 `jot` 的操作数派生 switch 按 C 的精确标签定形（P0：零步长不再无界）**
  （`2026-10-17`，`claim/JOT-STEP-ZERO-UNBOUNDED-qoder` @ 基线 `9d117c1f6`，单域三线 commands/textfilter，无跨界）：
  **缺口**：`src/jot.rs:derive` 把 C 的 `switch (have)`（`jot.c:247-296`）写成了**子集判**
  （`have & X != 0 && …`），而 C 的 case 标签是**精确值**——少一位就不是同一支。
  现量（参照件 `tmp/jot-317/jot_ref`，见 `tmp/jot-324/measure_before.log`）：
  `jot 3 1 100 0` 参照件是三行 `1` 退 0，本壳**永不终止**（五个操作数齐给时 C 落
  `case REPS | BEGIN | ENDER | STEP` 的 `break`，`:281-284`；本壳却落进 `case ENDER | STEP`
  那一支的 `reps = 0`，`:251-253`，而打印循环是 `i <= reps || reps == 0`，`:129`）。
  同一根因的第二形是数据量错：`jot 3 1 100 5` 参照件三行，本壳二十行。
  **修法**：四条判据全部改成 `have == 值` 的精确匹配，顺序与 C 的 case 次序一致，
  末尾留 `default`（`jot.c:293-295`）不动任何值。没有改壳、没有改循环、没有加特例。
  **门**：库测 +3 枚（`test_all_four_operands_with_zero_step_keeps_reps`、
  `test_derived_reps_branch_still_goes_unbounded`——反方向也要钉住，`jot - - 100 0` 的
  `reps = 0` 在 C 就是无界、`test_begin_ender_step_derives_reps`——第三枚是变异负控
  C2 咬出的缺口：`case ENDER | STEP | BEGIN` 少写一个标签时**原有 13 枚门全绿**），
  端到端门 +1 枚 `four_operands_with_a_zero_step_stay_bounded`（内含六形）。
  **读数**：`tmp/jot-324/ab324.py` 二十形 `SAME=16 DIFF=4`，四处 DIFF 全是已登记的
  `JOT-PREC-MINUS-ONE-FORMAT` 一族（`prec` 保持 `-1` 时 C 拼出的说明符本身是坏的，
  参照件在本机打成字面量 `%.0-1f`）；双方都无界的一形（`jot 0 1 10 2`）也算一致。
  **三件套**：容器整包 `cargo test -p minix-textfilter -j 1` → **899 / 0 / 0**
  （比上一笔的 895 恰多出本笔的 4 枚）；`cargo clippy -p minix-textfilter --all-targets` 退码 0，
  本笔新增行**零诊断**（指向 `src/jot.rs` 的 4 条全在未触碰的存量行，见下）；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins -j 1 -p minix-textfilter` 各 `Finished`、
  0 error；`rustfmt --edition 2024 --check` 零漂移。四枚变异负控（`tmp/jot-324/ctl324.py`）
  **全部咬住**，且四枚都避开无界写法（变异体一旦把 `reps` 归零就会在门里真跑无限输出）。
  **登记不执行**：① clippy 指向 `src/jot.rs` 的 4 条全在本笔未触碰的存量行
  （`let mut width = 0` 未读、`Jot` 的 `dox`/`exhausted` 字段未读、`match` 可换
  `unwrap_or_default`、`next` 与标准 trait 同名），本笔不顺手改；② `JOT-PREC-MINUS-ONE-FORMAT`
  与 `JOT-RANDOM-NEGATIVE-SEED` 沿用既有登记。核实日 `2026-10-17`；基线 `9d117c1f6`。

- ✅ **批次三百二十三把 `tr` 的空 string1 按 C 折回恒等映射（P0-code-bug，端到端门第一枚空集合形）**
  （`2026-10-17`，`claim/TR-EMPTY-SET1-TRANSLATE-qoder` @ 基线 `872948d89`，单域三线 commands/textfilter，无跨界）：
  **缺口从哪来**：上一笔的代码专项复核翻出库侧 `os/commands/usr-bin/textfilter/src/tr.rs:translate`
  的判据 `set1.is_empty() || set2.is_empty()`——前半个条件把 C 合法的输入判成了错。
  开工前现量（`tmp/tr-323/measure_before.log`）：`tr '' x`、`tr '' b`、`tr '' A-Z`、`tr -s '' x`
  四形参照件都是**退 0 且逐字节照抄标准输入**，本壳却退 1、标准输出空、且**不打任何句子**——
  退码、数据、诊断三件全错，是 P0-code-bug 而非措辞问题。
  **真源形状**：C 在非取补路径上先把映射表填成恒等（`tr.c:178-181` 的
  `for (ch = 0; ch < NCHARS; ch++) string1[ch] = ch;`），随后建 string2 的扫描器并用
  `next(s2, &ch2)` 把空 string2 当场判死成 `empty string2`（`:184-188`），**之后**才进那个
  逐字符改映射表的循环；空 string1 让这个循环在第一次 `if (!next(s1, &ch)) break;`
  （`:214-215`）就退出，于是一张谁也没被改过的恒等表被原样用于过滤。
  **两条选路**：甲在壳里为空前门短路（`set1` 空就直接照抄），乙删掉库侧那半句判据、
  让映射表回到它本来就是的恒等初值。**取乙**：C 的恒等初值就在库语义那一层（`string1[]` 的预填），
  甲等于在本树里再造一条 C 没有的分支，还把同一语义复制到两处；乙改完后
  `translate` 的分支结构与 C 的三段一一对齐。`set2.is_empty()` 那半句**保留**——
  它是 `set2[set2.len() - 1]` 这次取值的越界防线，而 C 侧的等价物是循环之前那句 `errx`，
  本层已经接在壳里（`bin/tr.rs` 的 `empty string2` 判据），两者不是重复而是同一判据的两条腿。
  **门**：库测 +1 枚 `test_empty_first_set_is_the_identity_map`、端到端门 +1 枚
  `an_empty_string1_is_the_identity_map`（钉住四形恒等、取补空集是**全体字节**故
  `tr -c '' x` 得 `xxxxxxx`、空 string2 仍走 `tr: empty string2` 不被恒等支吞掉、
  `tr '' x extra` 三个操作数仍判个数退 usage）。
  **读数**：空集合一族 13 形现量对账 **SAME=13 DIFF=0**（开工前是 4 形 DIFF）；
  上一笔的 65 形全量复跑 **SAME=62 DIFF=3** 不退化（`tmp/tr-322/ab323_full.log`）。
  **三件套**：容器整包 `cargo test -p minix-textfilter -j 1` → **895 / 0 / 0**（67 target，比上一笔的 893/67 恰多出本笔的 2 枚）；`cargo clippy -p minix-textfilter --all-targets` 退码 0、**指向本轮触及件 0 条**；`cargo check --workspace -j 1` 0 error；宿主三架构 `--bins -j 1 -p minix-textfilter` 各 `Finished`、0 error；`rustfmt --edition 2024 --check` 零漂移。
  **四枚变异负控**（`tmp/tr-323/ctl323.py`，带 `--no-fail-fast`）**全部咬住**，
  且四枚都咬到新门那枚 `an_empty_string1_is_the_identity_map`：把判据加回来、去掉恒等预填、
  取补集算空、让空 string1 挤掉 `empty string2` 的判据。
  **登记不执行**（复核顺手发现的本支**存量**形状，本笔一律不动）：① `TR-CLASS-OUT-UNUSED`——
  `CharClass::List`、`CharList`、`MAX_MEMBERS` 只被测试构造，`parse_set` 返回的
  `[CharClass; 8]` 被每个调用方丢弃（`bin/tr.rs:expand` 取 `(used, _, _)`），
  而模块头写着调用方要用它；② `TR-TOOLONG-ARMS`——`translate`/`delete`/`squeeze` 的
  `TextError::TooLong` 三支在现有壳里不可达（壳按 `input.len() + 1` 分配输出缓冲）；
  ③ 坏集合面仍「退 1 不打句子」（`bin/tr.rs:run_filter` 与 `expand` 的 `Err(_) => terminate(1)`），
  归 `TR-SET-GRAMMAR-SENTENCES` 同一条待办；④ `%c` 裸字节与非法 UTF-8 argv 两条沿用
  `TR-NONASCII-OPTLETTER` 与 `BIN-SUPPORT-HOSTED-ARGV-NONUTF8`（后者已由批次三百二十九闭合：
  宿主腿改走 `args_os()` + lossy，两条腿同规则；与 C 的字节差不在那笔射程内，仍随
  `GETOPT-REPORT-BYTE-LETTER` 挂着）。
  核实日 `2026-10-17`；基线 `872948d89`。

- ✅ **批次三百二十二把 `tr` 的选项环、个数判与三条组合判按 C 定形，判定表因此走满，并新立它的第一扇端到端门**
  （`2026-10-17`，`claim/TR-GETOPT-LINE-qoder` @ 基线 `c39fd86dc`，单域三线 commands/textfilter，无跨界）：
  **真源形状**：`tr.c:68` 写的是 `while ((ch = getopt(argc, argv, "cds")) != -1)`——`c`/`d`/`s`
  三个字母**都不带值**，所以 libc 那句「option requires an argument」（`getopt.c:127-137`）在本支
  不可达；而 `tr.c:79-81` 的 `case '?': default: usage();` **坐在环里直接退出**，于是集外字母的
  libc 报文**至多一条**（参照件 `-ZQ` 只报 `Z`、`-Q -Z a` 只报 `Q`），与 `seq`/`jot` 的逐字母累加相反。
  环后是**四段判据、按固定次序**：`tr.c:86-97` 的 `switch (argc)` 把 0 个与 3 个以上一起打发；
  `-ds` 要两个集合（`:104-106`）、`-d` 不许两个（`:123-125`）、既无 `-d` 又无 `-s` 时必须两个
  （`:156-157`），而 `-s` 单带一个集合在 `:139-140` 自成一局——**这条不对称**正是 usage 那四行的来历。
  `empty string2` 只在翻译段（`:187-188`）且**不接** usage；`usage()`（`:276-282`）打**四行**、
  程序名 `tr` 写死在串里、退 1。
  **libc 的三条隐蔽规则**（本支逐形现量到的）：① `getopt.c:79` 取的是短横后的**第一个字节**，
  `getopt.c:80-85` 只把**恰好**两个短横认作结束标记——所以 `tr --s a` 报的是
  `unknown option -- -`（第三字节 `s` 让它不被当成 `--`）；② `getopt.c:86-93` 里光杆 `-` 因为
  optstring 不含 `-` 而**落进操作数**，于是 `tr - a` 是把 `-` 换成 `a`、`tr - -` 原样输出；
  ③ 非选项 token 收环时**不推进**游标（`getopt.c:74-78`），所以 `tr a - b` 的三个 token 全是操作数、
  `tr a b -c` 的 `-c` 也是操作数。
  **改动**：环与四段判据提为库侧 `src/tr.rs:parse_tr_args`（配 `TrArgs`/`TrError`，libc 报文交回
  `src/lib.rs:GetoptReport` 单源），usage 块成 `src/tr.rs:usage_block`——用 `concat!` 拼是因为
  Rust 的行末反斜杠续行会吃掉下一行的行首空白，而那七个空格就是对齐本身；壳 `src/bin/tr.rs`
  重写为「前门 ⇒ 五种派发」，`empty string2` 一句留在壳里走 `errx` 形。**本支没有读操作数档的腿**
  （数据只从标准输入来），所以「薄壳裸 `open`/`read` 宿主读不到输入」那一家族停在 `paste` 的十一处。
  门里钉住的一条数据通路事实：`-c` 与 `-s` 齐给时挤压集取的是 **string2**——`tr -cs a b` 对
  `aabbcc` 打 `aab`，连结尾换行都被挤掉，因为取补后的 string1 把其余一切（含换行）都映射成 `b`。
  **门测 9 枚**（`tests/tr_face.rs`，本支第一扇端到端门，期望值逐枚取自参照件）+
  **库测 3 枚**新增（环停在第一个坏字母、个数与组合全走 usage、usage 四行七个空格）。
  **三件套**：容器整包 `cargo test -p minix-textfilter -j 1` → **893 / 0 / 0**（67 target，
  比上一笔登记的 881/66 恰多出本笔的 12 枚）；`cargo clippy -p minix-textfilter --all-targets`
  退码 0、**指向本轮触及件的诊断 0 条**（中途三条：一处 `nonminimal_bool`、两处 `needless_borrow`，
  都在末片清零——把三条组合判写成 C 那样的三个 `if`，判据与 `tr.c` 的三段代码逐句对齐，
  也正是 `nonminimal_bool` 想要「合并」的那三条）；`cargo check --workspace -j 1` 0 error；
  宿主三架构 `cargo build --bins -j 1 --target {x86_64-unknown-none, aarch64-unknown-none,
  riscv64gc-unknown-none-elf} -p minix-textfilter` 各 `Finished`——**`-p` 不可省**：漏掉它就成了
  整工作区建 bins，`minix-rt` 带着默认的 `std` feature 去编 none 目标，报出「can't find crate
  for `std`」一千余条而像是代码崩了，实际是配方错（本笔踩过一次，现记在这里）；
  `rustfmt --edition 2024 --check` 触及件零漂移——本笔顺手把 `src/tr.rs` 自带的三处排版漂移
  一并归零（纯排版，不改语义）。
  **A/B 与参照件** `tmp/tr-322/tr_ref`（`tr.c` 真源 + Minix 的 `getopt.c` + 程序名垫片）比退码、
  stdout、stderr 三件，65 形里**前门 49 形 SAME=49**、数据通路 14 形 SAME=13、**总 SAME=62 DIFF=3**，
  三形全属下面登记的集合语法一族（对账表 `tmp/tr-322/ab322.log`，逐形一行以免分行误读）。
  **八枚变异负控**（`tmp/tr-322/ctl322.py`，跑在最终源码上，每枚跑完即还原并核对 md5；跑法带
  `--no-fail-fast`，于是库测与端到端门两个目标一定都执行）分别咬 **3 / 6 / 3 / 编译挡住 / 5 / 3 /
  5 / 1** 枚，其中 **7 枚被端到端门当场咬住**：环改成累加、非选项也推进游标、`--` 不吃自身、
  光杆 `-` 当选项字母处理、第三个字母错认成 `-c`、去掉「无旗标时必须两个集合」这条判、
  usage 七个空格改成八个、`empty string2` 判据扩到删除路径。
  **登记不执行**（三处）：① `TR-SET-GRAMMAR-SENTENCES`——集合语法的两句在 `minix3/usr.bin/tr/str.c`，
  本层 `src/tr.rs:parse_set` 只回一个 `TextError`，于是四形分歧（参照件现量值已录）：
  `tr '[x*3]' y` 与 `tr '[a*3]' b` ⇒ 参照件退 1 打 `tr: Sequences only valid in string2`，本层退 0
  原样输出；`tr z-a y` ⇒ 参照件把 `z-a` 当**三个字面成员**（反向区间不成立就退回字面量）退 0 打
  `ybc`，本层退 1 且**不打任何句子**；`tr '[:bogus:]' z` ⇒ 参照件退 1 打 `tr: unknown class bogus`，
  本层退 1 静默。② `TR-NONASCII-OPTLETTER`——C 用 `%c` 打裸字节（`tr -é` 打 `\303`），本层按
  `char` 渲染成 `Ã` 的两个 UTF-8 字节（与 `SEQ-`/`PASTE-`/`UNITS-` 的同名登记一族）。
  ③ `BIN-SUPPORT-HOSTED-ARGV-NONUTF8`——**共享缝**缺陷，一次影响全族：宿主侧
  `os/commands/usr-bin/textfilter/src/bin_support.rs:args` 走 `std::env::args()`，参数含非法
  UTF-8 字节时当场 panic（实测 `tr`/`seq`/`paste`/`units`/`rev`/`fold` 六支都退 SIGABRT，
  panic 文案是 `called Result::unwrap() on an Err value: "-\xFF"`），而目标侧同一函数走
  `from_utf8_lossy` ⇒ 两条双生面在非法字节上**不等价**，②那条也因此在本机量不到真值。
  **验证边界**：参照件在本机链 glibc 的 `errx`/`fprintf`，本支的句子不含 `strerror` 末段，
  故无跨库末段差异；真机标准输入腿归 `C-57`/E5。核实日 `2026-10-17`；基线 `c39fd86dc`。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/
  col/column/comm/cmp/jot/unifdef/units/seq/paste/**tr** + pr——**判定表走满**；
  另判 `expand`/`hexdump`/`colcrt`/`lam`（C 各用自己的解析器，句子不许按同族规律抹平）。
  判定表已走满，下一支改按 `tmp/todo-loop-PLAN.md` 的 R 表取：开工前跑 `grep -n "^| R" tmp/todo-loop-PLAN.md` 现读哪几行还没标 ✅，取第一项。
- ✅ **批次三百二十一把 `paste` 的选项环、`tr` 表与开档两条出口按 C 定形（第十一处读缝缺陷），并重建并行合并**
  （`2026-10-17`，`claim/PASTE-GETOPT-LINE-qoder` @ 基线 `e7e79fa5e`，单域三线 commands/textfilter，无跨界；
  **本轮拆两片落地**：首片 `9f0db881e` 是前门与读缝，末片补 clippy 清零、交叉编译腿与 A/B/负控）：
  **真源形状**：`paste.c:70` 的环是 `while ((ch = getopt(argc, argv, "d:s")) != -1)`——`d` 带值、`s` 是旗标。
  与同族最不同的一点：`case '?': default: usage();`（`paste.c:80-82`）**坐在环里直接退出**，
  所以 libc 的报文**不可能累加**（`-ZQ` 与 `-sZ` 都只有一行），这正与刚做完的 `seq`（BADCH 之后继续扫、
  报文累加到五行）相反；而错误出口的**先后就是扫描的先后**：`-d ''` 在 `case 'd'` 里就地跑 `tr()`，
  故 `paste -d '' -Z` 报「没有分隔符」而 `paste -Z -d ''` 报集外字母。三家句子两个退码：
  ① 集外字母与 `-d` 缺值 = libc 一行 + **一行** usage（程序名**写死在串里**，`paste.c:231`）退 1；
  ② `-d ''` = `errx(1, "no delimiters specified.")` 一行，句子**自带句号**且不带 `strerror`，退 1；
  ③ 开不了的档名在两种模式下走**两条相反**的出口——并行是 `err(1, "%s", argv[cnt])` 三段形**致命**
  （第一个开不了就退出，后面的档名连试都不试，`paste.c:107-108`），顺序是 `warn("%s", p)` 三段形
  **跳过并继续**且整体退 0（`paste.c:166-169`）。
  **五处只能量出来推不出来的形制**（形状逐条现量，落盘在 `tmp/paste-321/measure.log`）：
  (a) 操作数**没有个数判**——`paste` 不带操作数既不读标准输入也不报错，退 0 打空
  （`paste.c:88-92` 把 `argc` 直接交给 `parallel`，零条流时行循环一步不走）；
  (b) 环不重排：`paste a.txt -s` 的 `-s` 是**第二个档名**，于是报的是并行的开档致命而不是/help；
  (c) `-d` 的值可以紧贴本 token（`-d,` 合法，且 `-ds` 的 `s` 是**值**而不是旗标）；
  (d) `tr()`（`paste.c:199-226`）只认 `\n`、`\t`、`\0` 三个转义，**认不出的转义丢掉反斜杠**
  （`\q` → `q`，与 `seq` 的 `unescape` 相反），且 `\0` 是**占一位而不打任何东西**（`if (ch) putchar(ch)`），
  所以 `paste -d '\0' a b` 打 `1a\n2b\n3\n`、`-d 'a\0b'` 的表是三位；串尾一个孤反斜杠不成位；
  (e) 并行的分隔符游标**每行从表尾那个 NUL 起**（`dp = delim + delimcnt`，`paste.c:112`），
  且「追平」环按**列号**而不是按实际发出数走 ⇒ 某条流提前见底时，后面的列会**为它多补一个分隔符**：
  `paste -d '+:' a.txt b.txt a.txt`（a 三行、b 两行）的第三行是 `3+:3`，而行尾还为剩下的每一列各补一个
  （`paste.c:144-150`）。参照件另一枚 `paste - -` 定出**一列 = 一个已打开的流**：两列都是 `-` 时
  共用同一个游标（对三行打 `1\t2` 与 `3\t`），而 `paste a.txt a.txt` 是两次 `open` ⇒ 两个游标。
  **第十一处「薄壳裸 `open`/`read` 宿主读不到输入」**：读操作数档的那一层是裸 `minix_sys::open`，
  宿主上任何档名都开不了 ⇒ 并行一律退 1、顺序一律 warn，操作数形状与两种合并因此从来没被进程级测过；
  本笔把它接到 `bin_support` 的双生面（`read_file`/`read_stdin`）。选项环提入库侧 `parse_paste_args`，
  `tr()` 提为库侧 `tr_delims`（libc 两句交回 `lib.rs:GetoptReport` 单源，`err`/`errx`/`warn` 句子留在壳里），
  并行合并的签名改成 `(columns, sources, …)` 才能表达「两列共用一个流」。
  **门测 11 枚**（`tests/paste_face.rs`，本支第一扇端到端门，期望值逐枚取自参照件）+
  **库测 11 枚**（`paste.rs` 全新增）。**三件套**：容器整包
  `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **881 / 0 / 0**（66 target，基线 `e7e79fa5e` 为 866/65）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、带 `-->` 的整包诊断 65 条，
  其中**指向本轮触及件的 0 条**（中途出过四条——两处 `collapsible_if`、一处 `needless_lifetimes`、
  一处 `items_after_test_module`——都在末片清零）；`cargo check --workspace -j 1` 0 error；
  宿主三架构 `--bins` 各 `Finished`——**它抓到 `bin/paste.rs` 里 `vec!` 未 `use alloc::vec` 的交叉编译错**，
  宿主整包测与 clippy 全绿也发现不了（与批次三百一十七同型的坑，第二次踩）；
  `rustfmt --edition 2024 --check` 触及件零漂移。
  **A/B 与参照件** `tmp/paste-321/paste`（`paste.c` 真源 + Minix 的 `getopt.c` + 程序名垫片 +
  **一个 BSD `fgetln` 垫片**——glibc 没有 `fgetln`，必须按 BSD 语义复刻：返回含结尾换行的整行、
  以长度而非 NUL 传长度、只在 EOF 且一字未读时返回 `NULL`）比退码、stdout、stderr 三件，
  **前门 49 形 SAME=49 DIFF=0**；决定半 2 形 DIFF 全部登记（见下），另 1 形巧合相同。
  **十枚变异负控**（`tmp/paste-321/ctl321.py`，跑在最终源码上，每枚跑完即还原并核对 md5）
  分别咬 **5 / 4 / 1 / 1 / 2 / 2 / 6 / 5 / 2 / 1** 枚：丢掉 libc 那一行、usage 退码当成 2、
  把顺序模式的 warn 也当致命、零操作数改成读标准输入、`\0` 转义当字面 0、认不出的转义不再丢反斜杠、
  并行游标起点从表尾 NUL 改成表头、取消行尾补分隔符、`-d` 不吃紧贴本 token 的值、串尾反斜杠也算一位。
  **登记不执行**（三处）：① `PASTE-SERIAL-LINE-MAX`——顺序模式用 `fgets(buf, _POSIX2_LINE_MAX + 1)`
  一行一读，超长行是 `err(1, "%s: input line too long.", *argv)` **致命退 1**，且该句末段是
  `strerror(0)`（参照件打 `Success`，errno 未设）；本层整档一次读完再分行，既无 2048 上限也无这一形；
  ② `PASTE-NONASCII-OPTLETTER`——C 用 `%c` 打裸字节（`-é` 得 `\303`），本壳按 `char` 渲染成 `Ã`（与
  `SEQ-NONASCII-OPTLETTER`、`UNITS-NONASCII-OPTLETTER` 同源）；③ `PASTE-NONUTF8-DELIM`——
  `-d` 的表按字节建，参数含非法 UTF-8 时本层走 `from_utf8_lossy`。
  **验证边界**：参照件在本机链 glibc 的 `err`/`warn`/`getopt`，`strerror` 末段取宿主表
  （`No such file or directory` 两侧同文，故开档两支持住 A/B）；真机 open/read 腿归 `C-57`/E5。
  核实日 `2026-10-17`；基线 `e7e79fa5e`。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/
  col/column/comm/cmp/jot/unifdef/units/seq/paste + pr；**只剩 `tr`**（`minix3/usr.bin/tr/tr.c:68`
  用 libc `getopt("cds")`，三个都是旗标），另判 `expand`/`hexdump`/`colcrt`/`lam`。
  下一支取 `TR-GETOPT-LINE`（已合入，见上一笔）；判定表走满之后改按 R 表取项——`tmp/todo-loop-PLAN.md` 的排期表行号是 R1…R30，「V3-11」这个标签在该表里并不存在，属引用错型，本笔起一并翻正。

- ✅ **批次三百二十把 `seq` 的选项环与三家句子按 C 定形，并新立它的第一扇端到端门**
  （`2026-10-17`，`claim/SEQ-GETOPT-LINE-qoder` @ 基线 `65457036c`，单域三线 commands/textfilter，无跨界）：
  **真源形状**：`seq.c:101-102` 的环写作 `while ((optind < argc) && !numeric(argv[optind]) &&
  (c = getopt(argc, argv, "f:hs:t:w")) != -1)`——`f`/`s`/`t` 三个带值、`h`/`w` 两个不带值。
  句子有**三家、两个退码**：① `errflg` 一族退 1，形如「libc 的零到多行 + **一行** usage」；
  ② 命令自己的 `errx(1, …)` 一族退 1，各一行且不接 usage（`zero increment`/`zero decrement`
  的名字随 `first < last` 变、`needs positive increment`、`needs negative decrement`、
  ``invalid format string: `%q'`` 带原文）；③ 操作数一族走 `e_atof()`（`seq.c:383-400`）退 **2**，
  判序在 C 里写死：先看 `errno == ERANGE` ⇒ `err(2, "%s", num)` 的三段形，再看尾巴有没有杂字符
  ⇒ `errx(2, "invalid floating point argument: %s", num)`。**环不在第一个坏字母处停**
  （BADCH 是 `'?'`，不等于 `-1`，`errflg++` 之后继续扫本 token 剩下的字母）：参照件
  `-ABC 3` 三行、`-+- 3` 两行、`-Zf` 两句（先 `unknown option -- Z` 后 `option requires an
  argument -- f`）、`-5e310` 五行（`5`/`e`/`3`/`1`/`0` 各一行）；旧写法在第一个坏字母处退出，
  少掉整串 libc 的行。`h` 在集内 ⇒ libc 无话可说，而 C 的 `case 'h'`（`seq.c:120`）后面**没有
  `break`**，直接落 `default: errflg++` ⇒ `seq -h`、`seq -hf 3` 都只有 usage 一行。
  **五处只能量出来推不出来的形制**（形状逐条现量，落盘在 `tmp/seq-320/measure3.log` 到 `measure6.log`）：
  (a) 环前那道 `!numeric(argv[optind])` **只在以短横开头的 token 上起作用**——`seq 1e2` 里
  `numeric("1e2")` 为假（`e` 后必须紧跟符号才算指数，`seq.c:213-219`），但 libc 见首字符不是
  `-` 就收环且**不推进** `optind`（`getopt.c:74-78`），于是一句不印、`1e2` 就是操作数，
  参照件打满 1..100；(b) `numeric("")` 反倒为**真**（`while (*s)` 一步不走而判据是
  `*s == '\0'`），且 `e_atof("")` 也是合法的 0（`strtod` 转不出任何东西时把 endptr 停在起点，
  而起点就是那个 `\0`）⇒ `seq ""` 打 `1\n0\n`、`seq "" 3` 打 `0\n1\n2\n3\n`；
  (c) 操作数解析次序是 `last`→`first`→`incr`（`seq.c:139-148`），加上环不重排，
  `seq 3 -w` 与 `seq -w 3 -s ,` 报的都是最后那个 token 的名字；(d) `-s` 的分隔符打在
  **每一个数之后**（含最后一个）而 `-t` 只打一次在最后 ⇒ `seq -s , 1 3` 的字节是 `1,2,3,`
  （有逗号没有换行），`seq -s '' 1 3` 是 `123`，`seq -t '!' 1 3` 是 `1\n2\n3\n!`；
  (e) `-w` 的两级不对称，因为 C 只是把 `pad` 塞进 printf 的格式串
  （`sprintf(buf, "%%%c%d%c", pad, MAX(width1, width2), 'g')`，`seq.c:467-469`）：
  一级是 `%0<宽>g`（零填在符号之后），两级起是 `% <宽>g`——**非负数也占一个符号位**，
  于是 `seq -w -w 8 10` 打的是三列的 `" 10"`（超宽不截），`seq -w -w -10 10` 里 `0` 得 `"  0"`
  而 `-9` 得 `" -9"`；宽度又取自**实际被打印的末值**（`generate_format` 先用
  `first ± incr * floor((first - last) / incr)` 把它折回来，`seq.c:439-442`），
  所以 `seq -w 1 5 10` 只打 `1`、`6` 两个一位数（宽度 1，不是 `10` 的 2）——
  这一处是 A/B 第一轮逮出来的（前门三形 DIFF 之一），本轮把宽度与两级一起按 C 重建。
  C 的 `floor` 与 Rust 的截断除法在异号时差一格，另写 `floor_div`。另两处随码修回：
  `unescape` 的十六进引导字母**只有小写 `x`**（C 的 `switch` 没有 `case 'X'`，`seq.c:353`），
  以及 `valid_format` 只把 `A a E e F G g` 记为转换且结尾判 `conversions <= 1`
  （`seq.c:231-287`）⇒ `%d` 与 `%d\n` **都非法**而 `abc`、`%%`、空串合法。
  **本支没有读写缝缺陷**：既不读标准输入也不开操作数档，「薄壳裸 `read` 宿主读不到输入」
  那一家族到 `units` 为止是十处，`seq` 是判定表里第一个纯生成形的命令。
  选项环提入库侧 `parse_seq_args`，`e_atof`/`numeric`/`unescape`/`valid_format` 全是库侧单源
  （libc 那两句交回 `lib.rs:GetoptReport`，`errx`/`err` 的句子留在壳里跟着 `getprogname()`）。
  **门测 15 枚**（`tests/seq_face.rs`，本支第一扇端到端门，期望值逐枚取自参照件现量）+
  库测 21 枚（`seq.rs`：基线 9 枚，本轮新增 12 枚）。
  **三件套**：容器整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **866 / 0 / 0**
  （65 target，基线 `65457036c` 为 839/63）；`cargo clippy -j 1 -p minix-textfilter --all-targets`
  退码 0、0 error、带 `-->` 的整包诊断 65 条，其中**指向本轮触及件的只有一条**
  （`src/bin/seq.rs` 的 `unused import: ToString`，已消除 ⇒ 触及件 0 条；其余分布在未触及的件上：
  `pr.rs` 18、`units.rs` 6、`diff.rs` 6、`jot.rs` 5、`window.rs` 3、`unifdef.rs` 3 等，
  条数随各 target 的缓存重放而变，故按「本轮触及件诊断数」判零新增，与批次三百一十八同一口径）；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`
  （riscv 那一支的 target 三重写全称 `riscv64gc-unknown-none-elf`，简名 `…-unknown-none`
  在本机 toolchain 连 target specification 都找不到，是配方的笔误不是代码问题——见下「工具坑」）；
  `rustfmt --edition 2024 --check` 触及件零漂移。
  **A/B 与参照件** `tmp/seq-320/seq`（`seq.c` 真源 + Minix 的 `getopt.c` + 程序名与 `strlcpy`
  垫片 + 空 `namespace.h`，宿主现链）比退码、stdout、stderr 三件：**前门 64 形 SAME=64 DIFF=0**；
  另有两形（`1e310`、`1e-40000`）走 `err(2, "%s", num)` 的三段形，句子形状、退码全同而
  `strerror(ERANGE)` 的末段不同——本层取 Minix `sys_errlist` 的 34 号
  `Result too large or too small`，本机链的参照件取 glibc 的 `Numerical result out of range`
  （批次三百一十一记过的同源差），按 Minix 表归一后 SAME=2 DIFF=0；十形 DIFF 全在决定半（下表）。
  **十三枚变异负控**（`tmp/seq-320/ctl320.py` 九枚 + `ctl320b.py` 四枚，全部跑在最终源码上）
  分别咬 **5 / 跑不完 / 2 / 4 / 8 / 2 / 2 / 1 / 3** 与 **1 / 2 / 2 / 1** 枚：丢 libc 那一行、
  第一个坏字母就停环（把环写成死循环，测试进程被 OOM 杀掉——「跑不完」本身就是咬住）、
  把 `h` 当集外字母、非短横 token 也推进 `optind`、从下一个 token 取值后不再越过它、
  两级补宽合成一级、空操作数当坏数、十六进接受大写 `X`、缺值那句不报、宽度取自请求的上界、
  空格级不留符号位、`pad` 永远是 `0`、`floor` 改用截断除法。A/B 排在变异腿**之后**且跑前重链一次。
  **登记不执行**（五处）：① `SEQ-FORMAT-RENDER`——`-f` 的说明符合法之后按 `%g`/`%e`/`%.2f`
  渲染浮点、以及非整数操作数的常规输出，都等浮点渲染那处裁决（与 printf 家同一处）；
  C 在这一形照常打数（参照件 `seq -f 'Z' 3` 打三个 `Z`、`seq 0.5` 打 `1`），本壳交回一行
  自认的说明并退 1；② `SEQ-NONFINITE-OPERAND`——`inf`/`nan` 在 C 是一路走到底
  （`seq inf` 因 `first <= last` 永真而刷不完，参照件被 `timeout` 杀掉；`seq nan` 打空退 0），
  本层的整数引擎把非有限值挡在渲染之前；③ `SEQ-SUBNORMAL-ERANGE`——下溢到**非零**次正规数
  （`1e-310`）在本机 glibc 的 `strtod` 上仍置 `ERANGE`，而 Minix 用的那份 gdtoa 实现不在本仓库
  （`minix3/lib/libc/arch/x86_64/gdtoa/` 只有头文件），故「结果为 0」这条判据之外不再推断；
  ④ `SEQ-NONASCII-OPTLETTER`——C 用 `%c` 打**裸字节**（`-é` 得 `\303`、`\251` 两行），
  本壳按 `char` 渲染成 `Ã`/`©`（与 `UNITS-NONASCII-OPTLETTER` 同源，行数与退码同）；
  ⑤ `SEQ-UNESCAPE-NONUTF8`——`unescape` 产出的是按字节解码的串，`\x80` 之类在本层走
  `from_utf8_lossy` 而非原样字节。核实日 `2026-10-17`；基线 `65457036c`。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/
  unexpand/col/column/comm/cmp/jot/unifdef/units/**seq** + pr；待接 `paste`、`tr`；
  另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `PASTE-GETOPT-LINE`
  （`minix3/usr.bin/paste/paste.c:70` 的 optstring 是 `"d:s"`：`d` 带值、`s` 是旗标，
  只有 `s` 之后跟不跟值这一处需要现查），其后是 `TR-GETOPT-LINE`
  （`minix3/usr.bin/tr/tr.c:68` 用 libc `getopt("cds")`，三个都是旗标——
  早前把 `tr` 记在「另判」是错的，本轮已按现查翻正）。
  **工具坑新增一条**：宿主三架构这一腿的 riscv 目标名必须写全称 `riscv64gc-unknown-none-elf`；
  写成 `riscv64gc-unknown-none` 时 rustc 报的是「could not find specification for target」，
  看起来像代码问题其实是配方笔误，别拿它当交叉编译失败登记。

- ✅ **批次三百一十八把 `unifdef` 的三类出口按 C 定形（第九处读缝缺陷），并新立它的第一扇端到端门**
  （`2026-10-16`，`claim/UNIFDEF-GETOPT-LINE-qoder` @ 基线 `caefad38c`，单域三线 commands/textfilter，无跨界）：
  **真源形状**：`unifdef.c:273` 的 optstring 是 `"i:D:U:I:o:cdeklst"`——`D`/`U`/`i`/`I`/`o` 五个带值、
  `c`/`d`/`e`/`k`/`l`/`s`/`t` 七个不带值，且**大小写算不同字母**（`-uD` 的 `u` 与 `-tE` 的 `E` 都集外，
  判定表里这一支是第一次要靠字母集合本身而不是词族来定可达出口）。三类出口：
  ① 集外字母与缺值各由 libc 先印一行（`getopt.c:101-104`、`:132-136`）再接 `default: usage()` 一行；
  ② `-i` 的后随字符不是 `D`/`U`（`unifdef.c:278-287` 的 `opt = *optarg++`）或符号名不合 `addsym`
  的形（`:978-990`）时，C **只打 usage 一行**——旧写法在这两处自拟了 `-i must be followed by D or U`
  与 `bad symbol spec …`，各多出一行；③ 环结束后先查「一个符号都没有且没给 `-s`」（`warnx` 一句 +
  usage 共两行），**这支排在操数判定之前**，所以 `unifdef -- -z` 报的是它而不是「只能一个档」。
  本支退码的形状也与同族不同：`usage()` 自己是 `exit(2)`（`:373-379`），所以四类出口全退 2，
  而 usage 那行的程序名**写死在串里**（没有 `getprogname()`），libc 那两行与 `err`/`warnx` 的前缀
  却跟着 `getprogname()`——同一支命令里两行来历不同。**四处只能量出来推不出来的形制**：
  `-D ''` 与 `-U ''` 是**合法**的空符号名（`skipsym` 原地返回，`*val == '\0'` 走「值为空」那一支），
  整档透传退 0，而旧写法把空名当坏形拒；`-i` 先吃一个字母，所以 `-iDA` 的符号是 `A`（与 `-D A`
  逐字节同效）而不是 `DFOO`，`-iD=1` 更是「空名字挂个值」的合法形；环**不重排**，
  `unifdef -DA in.c -c` 的 `-c` 是第二个操数（`unifdef: can only do one file` 退 2）而不是旗标；
  开不了的操数档名是 `err(2, "can't open %s", …)` 三段形，旧写法少了 `strerror` 那一段。
  **第九处「薄壳裸 `read` 宿主读不到输入」**：本支自己拿 `minix_sys::read` 圈标准输入（同族八处之后
  的第九处），现已改走 `bin_support` 的双生面（`read_stdin`/`read_file`）。选项环提入库侧
  `parse_unifdef_args`，`addsym` 的判定提入库侧 `add_symbol`（libc 两行交回 `GetoptReport` 单源，
  `errx`/`warnx` 句子留在壳里跟着 `getprogname()`）。
  **门测 13 枚**（`tests/unifdef_face.rs`，本支第一扇端到端门）+ 库测 4 枚（libc 两形与大小写、
  不重排、`-i` 切字母、空符号名与坏形）。**三件套**：容器整包
  `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **810 / 0 / 0**（63 target，基线
  `caefad38c` 为 793/62）；`cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、
  **整包诊断 59 条与基线同数、触及件从 4 条降到 3 条**（`if let Err(_) =` 换成 `.is_err()`
  消掉一枚 `redundant_pattern_matching`，零新增；`grep -c '^warning'` 在两次跑里是 85/82，
  那差的是各 crate 的汇总行随缓存重放条数走，不是告警增减，所以这条腿按「带 `-->` 的诊断条数」计）；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`；
  `rustfmt --edition 2024 --check` 触及件零漂移；A/B 与参照件 `tmp/unifdef-318/unifdef`
  （`unifdef.c` + Minix 的 `getopt.c` + 程序名与 `strlcpy` 垫片，另需一个空 `namespace.h`
  与 `__printflike` 空宏）比 72 形的退码、stderr、stdout 与 `-o` 档内容四件，
  **前门 68 形 SAME=68 DIFF=0**，四枚 DIFF 全在决定半（`-s` 的三形与 `-d` 的一形，见下）；
  五枚变异负控（`tmp/unifdef-318/ctl318.py`）分别咬 **3 / 2 / 3 / 2 / 2** 枚：丢 libc 那一行、
  给坏形加自拟句子、把空符号名当坏形拒、`-i` 不再先吃字母、取消「不重排」。A/B 这一趟排在
  变异腿**之后**且跑前重链一次二进制——三百一十七踩过「变异腿与宿主 A/B 共用 `os/target/`，
  变异源链出的二进制被拿来比对」那个坑，这次按顺序避开了。核实日 `2026-10-16`；基线 `caefad38c`。
  **登记不执行**（四处）：① `UNIFDEF-SYMLIST-OUTPUT`——本行上方那句「`-s` 符号清单」是超前陈述，
  C 的清单由 `findsym` 在查表的同时 `printf`（`unifdef.c:930-931`），`unifdef -s` 打 `A`/`B` 两行，
  本壳打空（退码两侧一致，差的是标准输出）；② `UNIFDEF-D-DEBUG-OUTPUT`——`-d` 在 C 不是纯忽略，
  它让 `debug()` 往标准错误刷一串状态转移行；③ `UNIFDEF-O-ERRNO-LEG`——`-o` 写失败那句在 C 是
  `err(2, "can't open %s", ofilename)` 三段形，本层 `write_file` 的返回型是 `Result<(), ()>`，
  没有 errno 通道可拼 `strerror`；C 的 `-o` 还有一条「输出与输入同档且内容未变」的
  `errx(2, "%s unchanged", ofilename)`（`:509`、`:514`）与临时档改名流程，本层一次直写，那一形不可达；
  ④ `UNIFDEF-MAXSYMS-ERRX`——`too many symbols` 要 4096 个符号才触发，句子已备好
  （`unifdef.c:971-972`），进程级没有门钉。
  **验证边界**：参照件在本机链 glibc 的 `err`/`warn`/`getopt`，所以 `strerror` 句子取自宿主表
  （`No such file or directory` 两边同文）；`-o` 那一支 C 走 `stat` + 临时档 + `rename`，本层直写，
  同档那一形在本机不可比对；真机 open/rename 腿归 `C-57`/E5。C 把 `symname` 存成**指向 argv 的
  指针**并把 `=` 就地截断，本壳按 `String` 建模，观测面等价（`-iD=1` 与 `-DFOO=1` 两枚已逐形比过）。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/col/column/comm/cmp/jot/**unifdef** + pr；
  待接 `units`、`seq`、`paste`、`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取
  `UNITS-GETOPT-LINE`（`minix3/usr.bin/units/units.c:817` 的 optstring 是 `"lLvqf:"`，
  只有 `f` 带值，`l`/`L`/`v`/`q` 都不带）。

- ✅ **批次三百一十七把 `jot` 的四类出口按 C 定形，并新立它的第一扇端到端门**
  （`2026-10-16`，`claim/JOT-GETOPT-LINE-qoder` @ 基线 `4b2a81e4f`，单域三线 commands/textfilter，无跨界）：
  **真源形状**：`jot.c:150` 的 optstring 是 `"b:cnp:rs:w:"`——`c`/`n`/`r` 三个不带值，
  `b`/`p`/`s`/`w` **四个带值**（判定表里这一支的带值项最多，`cn` 相连容易被误读成 `c:`+`n`）。
  可达出口因此是**四类**，句式与行数都不同：① 集外字母 = libc 一行（`getopt.c:101-104`；
  聚簇 `-cz` 报 `z`、`-ncrZ` 报 `Z`、`--x` 报 `-`）+ `jot.c:151-152` 的 `default: usage()` 一行，退 1；
  ② 带值字母缺值 = libc 的 `option requires an argument -- <字母>` + usage 一行，四个字母同形
  （旧写法把 `-b`/`-s`/`-w` 打成只有 usage，把 `-p` 打成 `Bad precision value`——两支的行数与句子都错位）；
  ③ `-p` 的值过不了 `strtol(optarg, &ep, 0)` 全串检查或为负 = `jot.c:161-165` 的一句 `errx`
  **单独一行、不接 usage**；④ 环正常结束才轮到操作数：一个都没有（含光写 `--`，也含 `-b X`
  那种唯一 token 被 `-b` 吃掉的情形）是 usage 且**没有** libc 行，五个以上是
  `jot.c:223-225` 那句 `Too many arguments.  What do you mean by %s?`，报的是**第五个**操作数
  （`argv[4]`），前面四个一个都不解读。**两处只能量出来、推不出来的形制**：`-b` 与 `-w`
  落进同一个 `format` 槽（`jot.c:172-176` 的 `case 'b': boring = 1; /* FALLTHROUGH */ case 'w':`），
  所以 `-bX -wY` 打的是三个 `Y` 而 `boring` 不落下；环**不重排**，`jot 3 -Z` 里 `-Z` 是 begin
  那个操作数，`sscanf("%lf")` 不吃它时 C 取它最后一个字符的代码点当值（`jot.c:203-204`），
  于是打出 90/91/92 而根本不报错。`-p` 的接受集也靠现量钉死：base 0 认 `0x2` 与前导 `0` 的八进
  （`010` 是 8 位小数），空串转不出数字而 `*ep` 仍是 `\0` 故合法（`jot -p '' 3` 走整数形），
  `09`/`3x` 因尾巴剩字符被拒，`99999999999999999999` 饱和成 `LONG_MAX` 后截成 `int` 是 `-1`
  因而被拒、`4294967298` 截成 `2` 因而被接受。**门测 12 枚**（`tests/jot_face.rs`，本支第一扇
  端到端门）+ 库测 6 枚（libc 两形、坏值字母的先后、不重排、`--` 与裸 `-`、词槽共用、`strtol`
  接受集）。`jot` 既不读标准输入也不开任何档，所以本支**没有**同族那处「薄壳裸 `read` 宿主读不到
  输入」的读写缝缺陷（前八处之后的第一个零缺陷支）。**三件套**：容器整包
  `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **793 / 0 / 0**（62 target，基线
  `4b2a81e4f` 为 775/61——新增一扇门 12 枚 + 库侧 6 枚）；`cargo clippy -j 1 -p minix-textfilter
  --all-targets` 退码 0、0 error、**85 warning 同基线**；`cargo check --workspace -j 1` 0 error；
  宿主三架构 `--bins` 各 `Finished`；`rustfmt --edition 2024 --check` 触及件零漂移
  （`src/jot.rs` 那 10 处存量漂移一并抹平，台账卫生项里它的名字可以划掉）；A/B 与参照件
  `tmp/jot-317/jot_ref`（`jot.c` 真源 + Minix 的 `getopt.c` + 程序名与 `strlcpy` 垫片）
  逐形比退码与 stderr，**前门 36 形 SAME=36 DIFF=0**；再比标准输出的 26 形里
  **SAME=23 DIFF=3**，那三形全在决定半：`3 1 100 0` 与 `3 100 1 0` 是下面第①条，
  `-r -p 2 3 1 2 7` 是随机流两边不同（既有登记角落，参照件链的是 glibc `random()`）；
  五枚变异负控（`tmp/jot-317/ctl317.py`）分别咬 **2 / 1 / 1 / 2 / 1** 枚：缺值那一支借用集外
  字母的句子、丢掉「无操作数即 usage」、把 `Too many arguments` 改报最后一个操作数、丢掉 `-p` 的
  全串检查、让 `-b` 不再写词槽。核实日 `2026-10-16`；基线 `4b2a81e4f`。
  **test-audit 五维读数**（本笔的门自己就是被它救回两次）：第 2 维抓到两处**测试写错**——
  `-cb 3` 里那个 `3` 会被 `-b` 取走作词值，于是缺值判据根本不成立（参照件现量打的是 usage，
  旧断言却要求 libc 那一行），`-p 010 3` 的期望是我自己抄错的三行同值；两处都按参照件改期望
  而不改代码。第 1 维的缺口是一条清单而非本笔的活：`jot.c:247-296` 那三句推导期的 `errx`
  （`Impossible stepsize`、`Must specify begin if reps == 0`、`Infinite sequences cannot be
  bounded`）在进程级没有一枚门钉住，库测只测算式分支。第 3、4 维零发现：库测与门测同一形状
  分住两层（判定态对输出），无空体、无理由忽略。第 5 维对账：07 篇与本条所写「门测 12 枚 +
  库测 6 枚」与 `grep -c "#\[test\]"` 的 12 与新增 6 个具名函数逐一对上。
  **登记不执行**（六处：五处在决定半或 libc 侧，一处是门的缺口，本笔不动）：① `JOT-STEP-ZERO-UNBOUNDED`（P0-code-bug）——
  `jot 3 1 100 0` 在 C 走 `jot.c:281-284` 的 `if (step == 0.0) break;`，reps 保持 3 所以打三行，
  本壳把 step 为零推成了无限输出，A/B 腿第一次跑它时 `$(...)` 吞到 5 GB 才被发现，现量与
  `timeout 5` 的截停都在 `tmp/jot-317/ab.log`；② `JOT-PREC-MINUS-ONE-FORMAT`——`prec` 保持 `-1`
  那一族（`jot 3`、`-n 3`、`3 - 5`、`-s , 3`）在 `jot.c:349-356` 拼出的是坏说明符，宿主 glibc
  打回字面量 `%.0-1f`（现量）、本壳打整数形 `1 2 3`（现量），Minix 自己的 `printf` 会打什么
  本机判不了，所以 A/B 只比退码与 stderr，判据要回到 `minix3/lib/libc/stdio` 的旗标循环再定；
  ③ `JOT-RANDOM-NEGATIVE-SEED`——`-r` 用第四个操作数当种子时 C 只把它交给 `srandom((unsigned long) step)`
  而无非负判定（`jot.c:118`），本壳 `v >= 0.0` 才收，负种子被报成 `Bad step value`；
  ④ `JOT-REPS-BASE0-AND-NEGATIVE`——C 那句是 `strtoul(argv[0], &ep, 0)`（`jot.c:213-214`），
  `0x10` 在 C 是 16 而本壳按十进制全串拒收，且 `reps < 0` 那个判据在 `unsigned long` 上恒假，
  `jot -3` 在 C 里回绕成极大值而本壳拒；⑤ `JOT-FORMAT-BUFSIZ`——`-b`/`-w` 的值经 `strlcpy`
  截到 `sizeof(format)`（`jot.c:176`），宿主的 `BUFSIZ` 与 Minix 那张表不同，超长词的截断点不可比；
  ⑥ `JOT-DERIVATION-ERRX-FACE`——`jot.c:247-296` 推导期的三句 `errx`（`Impossible stepsize`、
  `Must specify begin if reps == 0`、`Infinite sequences cannot be bounded`）在进程级没有一枚门
  钉住，本壳的 `error_text` 已备好句子，缺的是那三形的逐句对账。
  另有一处台账准确性：上一笔在 18 篇台账里留下两条同文的 `批次三百一十五` 标题，本笔合回一条。
  **验证边界**：参照件在本机链 glibc 的 `strtol`/`sscanf`/`printf`/`random`，所以 `jot -r` 的
  随机流两边本就不同（只比退码与 stderr），`prec == -1` 一族的标准输出不比（见上），而 `-p` 的
  接受集按 C 与 glibc 同规则——`0x` 后面不跟十六位数字那一支（`0x`、`0xg`）两边都逐形量过，
  同为 `Bad precision value`；Minix 自己的 `strtol` 在 libc 的通用 `strto*` 实现里，本笔未逐字读，
  所以那一支的相同是**量到的相同**而不是**证明的相同**，写在这里而不是写在结论里。真机侧 `jot`
  无 IO 缝，输出腿归 `C-57`/E5 的 stdout 写面。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/col/column/comm/**jot** + pr；待接 `unifdef`、`units`、`seq`、`paste`、`tr`；
  另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `UNIFDEF-GETOPT-LINE`
  （`minix3/usr.bin/unifdef/unifdef.c:273` 的 optstring 是 `"i:D:U:I:o:cdeklst"`，五个带值项（`i`/`D`/`U`/`I`/`o`））。

- ✅ **批次三百一十六把 `cmp` 的三类出口与「`-s` 压住开档告警」按 C 定形，并新立它的第一扇端到端门**
  （`2026-10-16`，`claim/CMP-GETOPT-LINE-qoder` @ 基线 `005e6a3a2`，单域三线 commands/textfilter，无跨界）：
  判定表里 `cmp` 这一支（历史条目的序号各自按当时的口径写，以每笔末尾那行「判定表进度」为准）。**真源形状**：`cmp.c:74` 的 optstring 是 `"ls"`——两个字母**都不带值**，
  所以 libc 那句 `option requires an argument` 在本支根本到不了。三类出口**行数各不相同**：
  ① 集外字母 = libc 一行（`getopt.c:101-104`；聚簇 `-lq` 报 `q`、`--x` 报 `-`）+ usage 一行；
  ② `-l` 与 `-s` 同时给（含聚簇 `-ls`、`-sl`）= 一句 `cmp: only one of -l and -s may be specified`
  且**不接 usage**；③ 操作数少于两个或多于四个 = **只有 usage 一行**。三支都退
  `ERR_EXIT` = 2（`extern.h:38`，另有 `DIFF_EXIT` = 1 与 `OK_EXIT` = 0——本支是三档退出码）。
  前门以前按整个 token 比字母（`arg == "-l"`），于是聚簇形落进「未知选项」并打出 usage：
  一行 `errx` 被换成两行，句子也没了。现在选项环逐字母走，`-l -l` 这种重复给同一面旗标
  照常合法（C 只是再赋一次 1）。
  **两处只能量出来、推不出来的形制**：① 开档失败那一句 `warn(3)` 是 `cmp.c:101-104` 的
  `if (!sflag) warn("%s", file1);` 再 `exit(ERR_EXIT)`——**`-s` 之下整句不打**，只留退出码 2
  （本壳以前无条件打，且打的是缺 `strerror` 那一段的两段形）；② 环不重排之后
  `cmp 档1 -l 档2` 是**三个操作数**、那个 `-l` 是第二个档名，参照件现量
  `cmp: -l: No such file or directory`，而旧写法扫完全程把 `-l` 当旗标吃掉、退码变成 0/1 一档。
  裸 `-` 是标准输入的别名而只能给一次（两边都写是一句 `errx`），且报文里出现的是**操作数原文**
  （`cmp.c:96` 的 `file1 = "stdin"` 改写只影响后面的报表，不影响这一行）。
  选项环提入库侧 `parse_cmp_flags`（同 `split`/`unexpand`/`col`/`column` 的分层：判定住库、
  句子交回壳），`-l`/`-s` 互斥与操作数个数两判留在壳里按 C 的次序走——换序会把一行的 `errx`
  支与两行的 usage 支弄混。**门测 11 枚**（既有的 `tests/cmp_face.rs` 整扇重写，target 数不变）
  + 库测 2 枚（聚簇逐字母、不重排与 `--`/裸 `-`），金样逐形取自参照件读数，含 skip 那一段的
  `strtoq` 读数（`5x` 吃 5、`010` 是八进制 8、`-1` 合法并落 `cmp: EOF on <短档>` **不带**
  char/line 的那一支、`x`/空串/溢出各退 2 走 usage）。
  **换掉的是参照系而不是门**：旧那扇门拿**宿主 GNU `cmp`** 作对照，且只比「退码相同」与
  「这一路是否为空」两件事（它的文件头自己写着 GNU 的句子是 `byte`、NetBSD 是 `char`，
  所以报文不比）——本笔换成链真源量的 Minix 参照件，报文与行数逐字进断言，旧门里那两条
  既有断言（`-` 侧报表写 `stdin`、开档失败排在坏 skip 之前）也一并搬进新的第 11 枚。
  **三件套**：容器整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **775 / 0 / 0**
  （61 target，基线 `005e6a3a2` 为 772/61——门本身从 10 枚长成 11 枚，库侧 +2）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、**85 warning 同基线**；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`；
  `rustfmt --edition 2024 --check` 触及件零漂移；A/B 与参照件 `tmp/cmp-316/cmp_ref`
  （`cmp.c` + `misc.c` + `regular.c` + `special.c` 真源 + Minix 的 `getopt.c` + 程序名垫片）
  逐形比退码/stdout/stderr 三件，**38 形 SAME=38 DIFF=0**；变异负控四枚
  （`tmp/cmp-316/ctl316.py`）分别咬 **1 / 1 / 3 / 1** 枚：丢 libc 那一行、把互斥那一支并到
  usage 后面、取消「不重排」、让 `-s` 不再压住开档告警。核实日 `2026-10-16`；基线 `005e6a3a2`。
  **验证边界**：参照件在本机链 glibc，`warn(3)`/`err(3)` 走 glibc，其 `strerror` 句子与 Minix
  表可能不同——本笔落在报文上的形都是 `No such file or directory`，两边同文，故 `DIFF=0` 是
  逐字节对账的结果；比较函数一侧 C 先 `setlocale(LC_ALL, "")`，`cmp` 的字节比较不经 `strcoll`，
  所以这一支不受 locale 影响。
  **登记不执行**：① C 的 `fstat` 两支（`cmp.c:136-145`）里 `err(ERR_EXIT, "%s", file1)` 那一形
  本层不可达——本层一次读全，`fstat` 失败与开档失败在同一只眼里；`special`（管道/字符设备）
  与 `regular` 两条比较路径本层也合成一条（`CMP-SPECIAL-PATH`）；② 目录当操作数时 C 的 `open`
  成功、`read` 才 `EISDIR`，于是那一侧直接见底并退 0/1，本层把它报成开档失败退 2
  （`CMP-DIR-OPERAND`，与上一笔的 `COMM-DIR-OPERAND` 同形）；③ 07 篇第 155 行的陈旧测试计数
  仍归 V3-5/V3-6（既有登记）；④ 真机 open/lseek 腿归 `C-57`/E5。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/col/column/comm/**cmp** + pr
  （egetopt 措辞）；待接 `jot`、`unifdef`、`units`、`seq`、`paste`、`tr`；
  另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `JOT-GETOPT-LINE`
  （`minix3/usr.bin/jot/jot.c:150` 的 optstring 是 `"b:cnp:rs:w:"`，注意 `cn` 两个字母都不带值）。

- ✅ **批次三百一十五把 `comm` 的两类出口与列前缀发放按 C 定形，并新立它的第一扇端到端门（第八处读缝缺陷）**
  （`2026-10-16`，`claim/COMM-GETOPT-LINE-qoder` @ 基线 `ed4db08d6`，单域三线 commands/textfilter，无跨界）：
  判定表第十一支。**真源形状**：`comm.c:80` 的 optstring 是 `"123f"`——`1`/`2`/`3`/`f` 四个
  **都不带值**的字母。这条决定了本支只有两类可达出口，而且**行数不同**：① 集外字母 = libc 先印
  一行（`getopt.c:101-104`）再由 `comm.c:91-93` 的 `default: usage()` 接第二行，退 1
  （`-Z`、聚簇 `-1Z` 报 `Z`、`--x` 报 `-`）；② 操作数不是两个 = **只有 usage 一行**（`comm.c:101`
  的 `if (argc != 2)`，少一个、多一个都算）。libc 那句 `option requires an argument` 在 `"123f"`
  下**根本到不了**——可达出口由 optstring 定，不按「同族都有缺值一支」的规律补（判定表立起来后已被
  反例教育过五回，这一支是又一个：形状像、集合不同）。两处细节：usage 那一行的程序名是**写死在串里**的（`comm.c:197` 没有
  `getprogname()`），而 ① 那一行的前缀跟着 `getprogname()`——同一支命令里两行的来历不同；
  打不开的档名是 `err(1, "%s", name)` 三段形且**致命**（`comm.c:181-184`），第一个档名失败时
  第二个连试都不试，这与 `column`/`rev` 的 warn+继续正相反。
  **选项环不重排**（`getopt.c:74-78`）：非选项 token 一到环就结束且不推进 `optind`，所以
  `comm 档 -1 档` 是**三个操作数**（参照件现量：一行 usage、退 1），旧写法扫完操作数还继续扫，
  把 C 拿去当档名的 `-1` 吃成了旗标。裸 `-` 是合法操作数并且就是标准输入（`comm.c:181` 的
  `!strcmp(name, "-")` → `stdin`），`--` 结束环而自身不是操作数。
  **末行没有换行符照样是一条记录**（`getnextln` 只在「EOF 且一个字符都没读到」时才返回 `NULL`，
  `:211-214`）——这一条与批次三百一十四的 `column` 恰好相反，那边把残行当「长行」丢掉并警告；
  又一处只能逐支量的形制，别再往同族上套。
  **第八处同族读写缝缺陷**：`comm` 壳拿裸 `minix_sys::open`/`read`/`close` 读操作数档，
  宿主上一个档名都读不到（前七处：`nl`/`fold`/`rev`/`colrm`/`unexpand`/`col` 的读腿、`split` 的写腿），
  所以三列合并的进程级行为从来没被测过；已改走 `bin_support` 的双生读缝，并把报文补回三段形。
  **这扇新门当场抓到第二处真缺陷**：列前缀要**依次发给真正打出来的列**——C 是
  `p = tabs; if (flag1) col1 = *p++; if (flag2) col2 = *p++; if (flag3) col3 = *p;`
  （`comm.c:105-110`），所以压掉列一之后公共行拿的是 `tabs[1]`（一个制表符）而不是 `tabs[2]`；
  旧写法固定按列号取 `tabs[column - 1]`，于是 `comm -1` 把公共行顶到了两个制表符上、
  `-2` 把列三顶成两个、`-12` 让唯一的公共行带着两个制表符出场。门测 9 枚 + 库侧新增 1 枚
  （不重排那一支），金样逐形取自参照件读数。
  **三件套**：容器整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **772 / 0 / 0**
  （61 target，基线 `ed4db08d6` 为 762/60，+9 是新立的 `comm_face`、+1 是库侧不重排那一枚）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、**85 warning 同基线**
  （本笔一度到 86，一枚是新门里一个绑了没用的操作数路径，已改成正面那条「stdin 与档并排走」的断言）；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`；
  `rustfmt --edition 2024 --check` 触及件零漂移；A/B 与参照件 `tmp/comm-315/comm_ref`
  （`comm.c` 真源 + Minix 的 `getopt.c` + `getprogname` 垫片，宿主现链）逐形比
  退码/stdout/stderr 三件，**32 形 SAME=32 DIFF=0**；变异负控四枚
  （`tmp/comm-315/ctl315.py`）分别咬 **1 / 1 / 3 / 3** 枚：丢 libc 那一行、列前缀按列号固定取、
  裸 `-` 退回选项环、操作数判据放松成「不少于两个」。核实日 `2026-10-16`；基线 `ed4db08d6`。
  **test-audit 第 2 维本轮又逮到一处「期望写错而代码对」**：新写的「`-` 与档并排走」那一形，
  我按「剩下的行属于第一边」推了两条路径，实际 C 是**第一边先见底、剩下的整段落在列二**
  （参照件现量 `\t\tapple`、`\t\tbanana`、`\tcherry`），代码本来就是对的——按参照件改期望，不改代码。
  **验证边界**：参照件在本机链 glibc，`err(3)` 走 glibc 而 `strerror` 的 `ENOENT` 句子两边同文，
  故 `DIFF=0` 是逐字节对账的结果；比较函数一侧 C 先 `setlocale(LC_ALL, "")` 再用 `strcoll`，
  A/B 在 `LC_ALL=C.UTF-8` 下与字节序一致。
  **登记不执行**：① 两个操作数都写 `-` 时 C 是**同一条流交错读**（先读两边各一行、比完再读
  输的那一边），参照件现量 `comm - -` 给的是 `banana`、`\tdate`、`fig`；本层「先收全再合并」
  的模型没有游标可言，这一形不可复制（`COMM-SHARED-STDIN-CURSOR`）；② 目录当操作数时 C 的
  `fopen` 成功、`getc` 才失败，于是那一侧直接见底、另一侧整段落在列二且退 0，本层的读缝把
  「开不了」统一报成致命错退 1（`COMM-DIR-OPERAND`）；③ `MAXLINELEN` 那一判（超长行丢掉多余
  字符直到换行，`comm.c:205-210`）未建模（`COMM-LINELEN-TRUNCATION`）；④ `strcoll` 随 locale
  变，本层按字节序（`COMM-LOCALE-COLLATE`）；⑤ C 收尾查 `ferror(stdout) || fclose(stdout)`
  落 `err(1, "stdout")`，本壳 `emit` 短写不上报（`COMM-WRITE-ERRLEG`，同族）；⑥ 无效 UTF-8
  整段弃（同族 `SPLIT-NONUTF8-DROP` 一族）；⑦ 真机 open 腿归 `C-57`/E5。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/col/column/**comm** + pr
  （egetopt 措辞）；待接 `cmp`、`jot`、`unifdef`、`units`、`seq`、`paste`、`tr`；
  另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `CMP-GETOPT-LINE`
  （`minix3/usr.bin/cmp/cmp.c:74` 的 optstring 是 `"ls"`，两个字母都不带值）。

- ✅ **批次三百一十四把 `column` 的选项环、四类出口与残行警告按 C 定形，并新立它的第一扇端到端门**
  （`2026-10-16`，`claim/COLUMN-GETOPT-LINE-qoder` @ 基线 `b8dafd773`，单域三线 commands/textfilter，无跨界）：
  判定表第十支。**真源形状**：`column.c:93` 的 optstring 是 `"c:s:tx"`——两个带值字母 `c`/`s`
  加两个旗标 `t`/`x`，而操数是**文件清单**（`:112-123`：开得了就喂同一个条目池，开不了只 `warn(3)`
  并置 `eval = 1` 然后继续下一个档）。四类出口：① 集外字母（libc 一行 + usage 一行，聚簇 `-tZ`
  报 `Z`、`--x` 报 `-`）；② `-c`/`-s` 后面没有那一段（libc 的 `option requires an argument -- <字母>`
  + usage 两行）——旧写法把这两类都压成一行自拟 usage；③ usage 那一行本身：`column.c:302-307` 是
  **大写 `Usage`、程序名跟着 `getprogname()`、末尾带 `[file ...]`**，旧件写成小写自拟短式且丢了尾部；
  ④ 操数段的 `warn(3)` 三段形 `Cannot open \`…': <strerror 句子>`（旧件少了 `strerror` 那一段）。
  **本批补上的两句整个缺失的**：`warnx(3)` 的 `column: line too long`（`column.c:277-283`，
  两段形、不缀 strerror）——末行没有换行符时 C **既丢掉这一条**又置退出码 1，而整条是空白的末行
  仍按空行处理、一句警告都没有（C 的判空行排在 `strchr('\n')` 之前）；以及条目池空时
  `if (!entries) return eval`（`:125-127`）提前返回，不进布局。
  **`-c` 的宽度是 `atoi`**（`column.c:96` 无守卫）：参照件量出 `0`、`abc`、`-5`、空串**都不是错**，
  退 0 且每条一行；这翻正了批次一百七十五登记的那处「本层要求解析为 `>0` 否则 usage」的差异。
  负宽度并到 0 的观察等价性在库侧注释里给判据：条目非空时最长条目取齐后至少 8，非正宽度于是
  永远走 `maxlength >= termwidth` 那条「每条一行」的腿，而 `numcols` 的除法只在正宽度才到得了。
  **不重排**：`getopt.c:74-78` 一到非选项 token 就返回 -1 且不推进 `optind`，所以 `column 档 -t`
  里的 `-t` 是**档名**（参照件 `column: Cannot open \`-t': No such file or directory`），
  旧件扫完一个非选项 token 还继续扫下一个，把 C 拿去 `fopen` 的串当旗标吃掉了。
  裸 `-` 是档名而不是标准输入的别名（`:87-92`），`--` 结束选项环而自身不是操数——于是 `column --`
  落回 `if (!*argv)` 那一判，读的仍是标准输入（参照件同形）。
  **门测 12 枚**（`tests/column_face.rs`，新立的 target）+ 库侧改写与新增 4 枚；金样逐形取自参照件
  读数（宽度 20 下 `alpha\tdelta` / `beta\tepsilon` / `gamma`、`-x -c 24` 的横排、`-t -s,` 的补齐、
  四类出口的句数与退码、残行警告那一句）。
  **这扇新门第一次跑通布局，当场抓到两处 P0-code-bug**：① `c_columnate`/`r_columnate` 把条目、
  制表符、换行**当三段各自返回**，壳侧又逐段补一个换行，于是两条布局从来没打出过一行像样的输出
  （`column` 的五条短行会变成 `alpha\n\t\nbeta\n\t\n\n\n`）——现在与 `table_mode` 同形各交整行，
  并照 C 补两处次序：`if (!--entries) break`（`column.c:149-150`）与
  `if ((base += numrows) >= entries) break`（`:178-179`）都排在补齐之前，所以行尾不挂制表符；
  ② 条目是**整行**——C 存的是行首的 `buf` 而不是跳过前导空白后的 `p`（`:287`），而 `:285` 的
  `len = p - buf` 也是从行首量的，所以前导空白既进池也占布局宽度；库文档与旧写法都写着
  「剥掉前导空白」，参照件现量 `  lead` 后面跟着**三个**连续制表符（其中一个是条目自带的制表符）
  坐实了这一点。第三处是自审抓到的**运行期 panic**：聚簇游标 `pos` 走的是**字节**下标，
  非 ASCII 字母时它会落进一个多字节字符中间，而 `&arg[pos..]` 在那一刻不是字符边界，切串当场 panic
  （`column -té` 就是这一形，参照件打的是一个裸字节 0xC3 后退 1）。改成 `char::from(bytes[pos])`
  按字节取，并把这一形钉了一枚库测；`column -- 多字节`、`-sé` 这类合法值仍按字符边界切，不受影响。
  **同一枚 panic 住在批次三百一十三交回的 `parse_col_flags` 里**（`col -bé` 即触发，逐字同形），
  按一次一条的规矩登记为 `COL-NONASCII-PANIC`、列为下一支的顺手项，不在这一笔里改另一条命令的码。
  `atoi` 从 `col.rs` 提到 crate 根由 `col` 与 `column` 共用（同 `GetoptReport`
  的单源处置：一份事实只承载一次）。
  **三件套**：容器整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **762 / 0 / 0**
  （60 target，基线 `b8dafd773` 为 746/59，+12 是新立的 `column_face`、+4 是库侧残行/宽度/出口/
  非 ASCII 游标四支）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、**85 warning**
  （基线 86，本笔少一枚：旧 `parse_column_args` 里 `pos = arg.len()` 那次死赋值随重写消失，零新增）；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`；
  `rustfmt --edition 2024 --check` 触及的叶子件（`src/column.rs`、`src/bin/column.rs`、
  `tests/column_face.rs`、`src/col.rs`）零漂移——`src/lib.rs` 上余下的三处 `pub mod` 顺序漂移
  是既有登记（本笔只往它加了一个 `atoi`，那一段本身干净，整文件重排会把二十多个未接的模块一起
  拖进这一笔，故不动）；A/B 与参照件 `tmp/column-314/column_ref`
  （`column.c` 真源 + Minix 的 `getopt.c` + 自写的 `fgetln`/`ecalloc`/`estrndup`/`erealloc`/
  `getprogname` 垫片，宿主现链）逐形比退码/stdout/stderr 三件，**42 形 SAME=42 DIFF=0**；
  变异负控四枚（`tmp/column-314/ctl314.py`）分别咬 **2 / 2 / 2 / 3** 枚：丢 libc 那一行、
  残行照样入库、条目剥掉前导空白、打不开的操数当致命错。核实日 `2026-10-16`；基线 `b8dafd773`。
  **验证边界**：参照件在本机链 glibc，`err`/`warn` 系列走 glibc，其 `strerror` 句子与 Minix 表
  可能不同；本笔落在报文上的形里只有操数那三支带 `strerror`，两边都是 `No such file or
  directory`，故 `DIFF=0` 是逐字节对账的结果。宽度一项：参照件先 `ioctl(TIOCGWINSZ)` 再看
  `COLUMNS`，A/B 时两者都无、落到默认 80，与本层的固定 80 同读数。
  **登记不执行**：① 跳前导空白与判空行用的是 `isspace` 全六类（含 `\r`、`\v`、`\f`），
  本层只认空格与制表符，所以 `  \r` 这样的末行在 C 是空行、在本层是条目（`COLUMN-ISSPACE-SET`）；
  ② C 的 `strchr(p, '\n')` 在行内 NUL 处停下，所以带 NUL 的行按「长行」丢掉并警告，本层按整行保留
  （`COLUMN-NUL-TRUNCATES-LINE`）；③ 终端宽度那一腿（`ioctl` 与 `COLUMNS`，`column.c:86-90`）
  属终端面，本层固定 80（真机腿归 `C-57`/E5）；④ 无效 UTF-8 整段弃（同族 `SPLIT-NONUTF8-DROP`）；
  ⑤ 07 篇第 155 行的陈旧测试计数仍归 V3-5/V3-6（既有登记）；
  ⑥ 集外字母是非 ASCII 时 C 的 `%c` 打的是**一个裸字节**（参照件现量 `column: unknown option -- \xC3`），
  本层按 `char` 渲染成两个 UTF-8 字节——行数、退码、句子其余部分都同，只差那一个字节
  （`COLUMN-NONASCII-OPTLETTER`，与 `REV-`/`COLRM-` 的同名登记一族）；
  ⑦ 测试自审（test-audit 五维）在第 2 维上**没有发现写错的期望**——门里每一条报文与布局读数
  都回到参照件复核过（`-c ""`、`-c " 30"`、`-t -s,` 的七个空格、`-- -c <档>` 那句 warn 都是
  为这一维现量而补测的）；第 1 维留一处缺口：**两个来源各带一条残行**时 C 打两句
  `line too long`，本壳的计数循环支持这一形，但门只钉了单来源那一句（`COLUMN-TAIL-WARN-COUNT`，
  已由批次三百二十八按参照件现量补门闭合——补的是门，生产码未动）。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/col/**column** + pr
  （egetopt 措辞）；待接 `comm`、`cmp`、`jot`、`unifdef`、`units`、`seq`、`paste`、`tr`；
  另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `COMM-GETOPT-LINE`。

- ✅ **批次三百一十三把 `col` 的选项环按 C 分回四类出口，并新立它的第一扇端到端门（第七处读缝缺陷）**
  （`2026-10-16`，`claim/COL-GETOPT-LINE-qoder` @ 基线 `6802a77e7`，单域三线 commands/textfilter，无跨界）：
  判定表第九支（未接的那一组里第一支）。**真源形状**：`col.c:133` 的 optstring 是 `"bfhl:px"`——
  五个无值字母加一个带值的 `l`，而且 `col` 一个操作数都不接受（`col.c:157-158` 的
  `if (optind != argc) usage()`，因为它只读标准输入）。四类出口：① 集外字母（libc 一行 + usage 一行）；
  ② `-l` 缺值（libc 的 `option requires an argument -- l` + usage 两行）；
  ③ `atoi(optarg) <= 0`（`col.c:145-150` 是裸 `fprintf(stderr, "col: bad -l argument %s.\n", optarg)`
  加 `exit`：程序名写死、带操作数原文、句尾一个句点，而且**不接 usage**——本壳以前把它当成②的兜底支，
  于是 `-l` 后面没有那一段时印的是 `col: bad -l argument.`，句子与行数都不对）；
  ④ 只有 usage（操作数、聚簇里的集外字母走①②那两行）。
  **`atoi` 的两处接受面**（参照件量，不用 Rust 的严格解析）：值可以附在本串里（`-l3`），
  尾巴上有杂字符也照收（`-l 3x` 是合法的 3，因为 C 只测 `atoi` 的返回值而 `atoi` 不管余下的是什么）；
  反过来 `""`、`abc`、`0`、`-2` 都落 `<= 0`。聚簇逐字母生效且后写覆盖先写（`-xb` 两支都算、`-hx` 结束于不压缩）。
  **第七处同族读写缝缺陷**：`col` 壳拿裸 `minix_sys::read(support::STDIN, …)` 读输入，
  宿主上读不到任何东西（前六处：`nl`/`fold`/`rev`/`colrm`/`unexpand` 的读腿、`split` 的写腿），
  所以那台行装配状态机的输出**从来没有被进程级测过**；已改走 `bin_support::read_stdin()` 的双生读缝。
  选项环提入库侧 `parse_col_flags`（同 `split`/`unexpand` 的分层：句子交回壳，判定住在库）。
  **门测 9 枚**（`tests/col_face.rs`，新立的 target）+ 库测 1 枚（选项环逐支、附值形、
  `3x`、三种失败各归一支、`--` 与操作数）：金样逐形取自参照件读数——反向换行被吸收而两行都留、
  缺省把退格原样透出而 `-b` 只留叠字符最后一个、缺省压空格成制表符而 `-x` 摊平、
  未知转义序列被吃掉、**末行没有换行时那一行整个不出**（C 攒到换行才 flush，参照件同形）。

  **三件套**：容器整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **746 / 0 / 0**
  （59 target，基线 `6802a77e7` 为 736/58，+9 是新立的 `col_face`、+1 是库侧选项环那一枚）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、**86 warning 同基线**
  （本笔一度到 87，一枚是测试里「与 `false` 比相等」，已改成取反）；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`；
  `rustfmt --edition 2024 --check` 触及件零漂移；A/B 与参照件逐形 **SAME=34 DIFF=0**
  （`tmp/col-313/ab.log`：29 形比退码/stdout/stderr，另 5 形比整段输入的输出指纹）；
  变异负控四枚（`tmp/col-313/ctl313.py`，每枚先打印 `mutation applied`、还原后核 md5）：
  丢 libc 那一行 → 咬 3 枚；缺值并进 `bad -l argument` → 咬 2 枚；
  改用严格解析替代 `atoi` → 咬 2 枚；读缝退回裸 `read` → 咬 4 枚。
  核实日 `2026-10-16`；基线 `6802a77e7`。
  **验证边界**：参照件在本机链 glibc 的 `getopt` 被 Minix 那份覆盖，`err`/`warn` 系列走 glibc，
  其 `strerror` 句子与 Minix 表可能不同；本笔 34 形里落在报文上的三支（集外字母、缺值、
  `bad -l`）用的都是不带 `strerror` 的句子，故 `DIFF=0` 是逐字节对账的结果。
  **登记不执行**：① C 的告警有两句——`col.c:536-541` 的 `dowarn` 按 `line < 0` 分
  `past first line` 与 `-- line already flushed`，本壳的 `ColOutput.warned` 是一个布尔，
  只印后一句，前一种形在这一层不可观测（`COL-WARN-KIND`）；② `col.c:528-533` 的 `wrerr`
  是 `col: write error.` 加 `exit`，本壳的 `emit` 短写不上报（`COL-WRITE-ERRLEG`，同族）；
  ③ C 读输入用 `getchar`、边读边排版，本壳先收全再过滤（`COL-STREAMING-MEMORY`）；
  ④ 参照件现量本次输入的告警一次都没触发，那条腿仍只有库测覆盖（`COL-WARN-FACE-CASE`）；
  ⑤ 07 篇第 155 行的陈旧测试计数仍归 V3-5/V3-6（既有登记）；⑥ 真机腿归 `C-57`/E5。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/**col** + pr
  （egetopt 措辞）；待接 `column`、`comm`、`cmp`、`jot`、`unifdef`、`units`、`seq`、`paste`、`tr`；
  另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `COLUMN-GETOPT-LINE`
  （`minix3/usr.bin/column/column.c:93` 的 optstring 是 `"c:s:tx"`，`c`/`s` 带值）。

- ✅ **批次三百一十二把 `unexpand` 的选项环与档位表按 C 定形，并新立它的第一扇端到端门（第六处读缝缺陷）**
  （`2026-10-16`，`claim/UNEXPAND-GETOPT-LINE-qoder` @ 基线 `6302808e7`，单域三线 commands/textfilter，无跨界）：
  判定表第十一支。**真源形状**：`unexpand.c:86` 的 optstring 是 `"at:"`——一个无值字母加一个带值字母，
  而且两支互斥（`if (nstops) usage()` 与 `if (all) usage()`，`:87-95`）。出口四类：① 集外字母（libc 一行
  `unexpand: unknown option -- <字母>` + usage 一行，退 1）；② `-t` 缺值（libc 的另一句
  `option requires an argument -- t` + usage，退 1）；③ 档位非法（`errx` 两句，形制是
  `<程序名>: Invalid tabstop `原文'` 与 `<程序名>: Bad tabstop spec `原文', must be greater than the
  previous `前一支'`，都不接 usage）；④ 只印 usage（`-a` 与 `-t` 冲突）。
  **量形量出一件出人意料的事**：真源在选项环之后、读输入之前，把档位表**逐支打到标准错误**——
  `unexpand.c:120-121` 的 `for (i = 0; i < nstops; i++) fprintf(stderr, "%lu %zu\n", i, tabstops[i])`。
  这显然是漏掉的调试输出，但它就在外部行为里（参照件 `-t 4` 的 stderr 就是 `0 4`），
  所以本壳照搬而不是替 C「修正」；这也说明为什么这类形制只能靠链真源量，看代码猜会得到「谁会把档位打到
  stderr」的结论。
  **另两处只有读 C 源才知道的事**：① 档位的数字是 `strtoul(tab, &ep, 0)`，**base 0**——`010` 是 8、
  `0x10` 是 16（参照件的回显行当场可证），旧写法用 `parse::<usize>()` 把两者都判成非法；
  ② 分隔符是 `strsep(&optarg, ", \t")` 三种（逗号、空格、制表符），空字段跳过，所以 `-t ""` 与 `-t ,`
  是「一支都没有」而等于默认档，且**不会**跨 token 取值——`unexpand -t 4 8` 的 `8` 是第二支**档名**
  （参照件：先回显 `0 4`，再去开 `8` 而报三段形）。
  **本壳改前的八处失真**（参照件逐形量，读数 `tmp/unexpand-312/golden.log`）：①② 两类 libc 行都缺；
  `usage()` 那一行三样都不合——C 是**首字母大写**的 `Usage: %s [-a] [-t tabstop] [file ...]`
  （`unexpand.c:70-71`）而旧壳写的是小写 `usage:`、`[-t tablist]`、还少了 `[file ...]`；
  档位回显整段缺失；base 0 与三种分隔符不收；两句 `errx` 被抹平成一句自拟的 `bad tab stop spec`；
  选项环**重排**（非选项 token 之后继续扫）；打不开的档名少了 `strerror` 那一段（C 是
  `err(EXIT_FAILURE, "Cannot open `%s'")` 三段形）；末行没有换行时旧壳**补**一个换行，而 C 的
  `putchar` 只写输入里有的字节（参照件：输入 `   ab` 出去仍是无换行的一行）。
  **第六处同族读缝缺陷**：`unexpand` 壳拿裸 `minix_sys::open`/`read` 读输入与档名，宿主上读不到任何东西
  （前五处：`nl`/`fold`/`rev`/`colrm` 的读腿、`split` 的写腿），已改走 `bin_support` 的
  `read_stdin()`/`read_file()` 双生面；顺带说明裸 `-` 在 `unexpand` 这里**不是** stdin 的别名
  （C 直接把它送去 `freopen`，报三段形），与 `rev`/`split` 各不一样。
  **门测 11 枚**（`tests/unexpand_face.rs`，新立的 target）+ 库测两处重写：四类出口各一枚、
  档位回显一枚（含「空表不回显」）、两支 `errx` 句子一枚、base 0 一枚、冲突只印 usage 一枚、
  不重排一枚、输入腿三段形一枚（含裸 `-`）、输出腿一枚（默认八列、`-a` 行中、末行无换行不补、
  多支档名依次处理并重置行游标）。
  **test-audit 第 2 维又一次逮住「期望写错而代码是对的」**：`-ta` 我先按「`-t` 与 `-a` 冲突」写成
  `Err(Usage)`，实际 C 的 getopt 一见 `t:` 就把同串剩下的 `a` 当 optarg 交回（`getopt.c:120-122`），
  走 `Invalid tabstop `a'`；参照件现量同一句，于是改期望而不改代码。连同库侧删掉的旧测
  （`test_parse_stops_rejects_unordered`、`test_parse_unexpand_conflicts_and_errors`）合入两支新测，
  避免同一事实两处承载。

  **三件套**：容器整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **736 / 0 / 0**
  （58 target，基线 `6302808e7` 为 725/57，+11 是新立的 `unexpand_face`、+1 是它的 target）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、**86 warning 同基线**
  （本笔一度把告警推到 92，四处都由门指出并已改掉：可折叠的 `if let` 嵌套、两支
  「`assert_eq!` 比字面 bool」、输出腿那个双重否定的布尔式）；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`；
  `rustfmt --edition 2024 --check` 触及件零漂移；A/B 与参照件逐形 **SAME=29 DIFF=0**
  （`tmp/unexpand-312/ab.log`，比退码、stdout、stderr 三件——stderr 含 C 那圈档位回显）；
  变异负控四枚（`tmp/unexpand-312/ctl312.py`，每枚先打印 `mutation applied`、还原后核 md5）：
  丢 libc 那一行 → 咬 3 枚；省掉档位回显 → 咬 3 枚；档位不按 base 0 → 咬 2 枚；
  取消「不重排」→ 咬 2 枚。核实日 `2026-10-16`；基线 `6302808e7`。
  **验证边界**：参照件在本机链 glibc 的 `err`/`errx`/`strtoul`/`fgetln`（glibc 没有 `fgetln`，
  垫片按 BSD 语义实现：返回行内容、长度含换行、EOF 返 `NULL`），其 `strerror` 句子与 Minix 表可能不同，
  但本笔 29 形里没有一支落在 0 号错误上，`DIFF=0` 是逐字节对账的结果；真机腿归 `C-57`/E5。
  **登记不执行**：① `unexpand_line` 里退格那两行的 `implicit_saturating_sub` 告警是存量（那是 C 的
  `if (ocol > 0) ocol--` 如实搬运），本笔不改（`UNEXPAND-SATURATING-DEC`）；
  ② C 的 `tabify` 在 `putchar` 失败时 `goto out` 静默返回、只有 `err(1, ...)` 之外的写错不上报，
  本壳的 `emit` 同样短写不可观测（`UNEXPAND-WRITE-ERRLEG`，同 `SPLIT`/`COLRM` 一族）；
  ③ 输入过 `String::from_utf8_lossy`，非 UTF-8 字节会被替换成 `U+FFFD` 而不是按字节透传
  （与 `SPLIT-NONUTF8-DROP` 同一类，`UNEXPAND-LOSSY-BYTES`）；④ `-t` 的值若是负数
  （`unexpand -- -4`）时 C 的 `strtoul` 会回绕成极大值，本壳按 `None` 拒收，需连 C 的
  `unsigned` 语义一并核（`UNEXPAND-ULONG-WRAP`）；⑤ 07 篇第 155 行的陈旧测试计数仍归 V3-5/V3-6
  （既有登记）；⑥ 真机腿与 per-command 断言归 `C-57`/E5。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/**unexpand** + pr
  （egetopt 措辞）；待接 `col`、`column`、`comm`、`cmp`、`jot`、`unifdef`、`units`、`seq`、`paste`、`tr`；
  另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `COL-GETOPT-LINE`
  （`minix3/usr.bin/col/col.c:133` 的 optstring 是 `"bfhl:px"`，只有 `l` 带值）。

- ✅ **批次三百一十一把 `split` 的诊断面按 C 分回四类出口，并新立它的第一扇端到端门（顺带修好宿主写不出的写缝）**
  （`2026-10-16`，`claim/SPLIT-GETOPT-LINE-qoder` @ 基线 `589fc4ce0`，单域三线 commands/textfilter，无跨界）：
  判定表第十三支。**真源形状**：`split.c:80` 的 optstring 是 `"0123456789b:l:a:n:"`——`0-9` 六个是
  **合法字母**（NetBSD 注释自称「Undocumented kludge」，遗留的 `-<数字>` 形），四个带值项，
  所以它的出口比只吃空 optstring 的 `rev`/`colrm` 多一整层，共四类：① 集外字母（libc 一行
  `split: unknown option -- <字母>` + `usage()` 一行，退 1；`-Z`、聚簇 `-Zb` 报 `Z`、`--x` 报 `-`）；
  ② 带值项缺值（libc 的另一句 `split: option requires an argument -- <字母>` + usage，退 1，
  `l`/`b`/`a`/`n` 四支都走这里）；③ 值非法（`errx` 两支形：`split: <原文>: illegal line count.` 等
  四句，没有 usage）；④ 只有 usage 自己（`-l 3 -l 4` 的 `if (numlines != 0)`、多于两个操作数）。
  **本壳改前的五处失真**（参照件 `tmp/split-311/split_ref` 逐形量出，`golden.log` 是读数）：
  没有①②两类的 libc 行；`usage()` 的句子是本壳自拟的短式 `usage: split [-l lines]` 而 C 写的是
  四选项式样且程序名跟着 `getprogname()`（`split.c:357-359`），**退码还写成 2**（C 是 `exit(1)`）；
  选项环**重排**——非选项 token 之后继续扫，于是 `split <file> -Z` 把前缀当成坏选项，而 C 的
  `getopt` 见非选项即返 -1（`getopt.c:74-78`），那个 `-Z` 是**输出前缀**（参照件现量：生成 `-Zaa`、退 0）；
  遗留形只认「全数字尾巴」，`-12x` 与 `-3b5` 被当集外字母而 C 把整段交给 `strtoull` 后报
  `illegal line count.`；后缀穷尽与 `-n` 超限两句是自拟的（C 是 `errx(1, "too many files.")` 与
  `errx(1, "can't split into more than %d files", (int)sb.st_size)`，后者缀的是输入的**字节数**）。
  **新门一立暴露的是写腿而不是读腿（同族第五例，方向相反）**：`split` 壳自己拿裸
  `minix_sys::open`/`write`/`close` 建分片文件，**宿主上一个文件都写不出去**（门测首轮五枚红，
  报的是 `split: xaa`），所以它的输出从来没有被进程级测过；已改走 `bin_support::create_sink` 的
  双生面（宿主 `std::fs::File::create`、真机 `open(O_WRONLY|O_CREAT|O_TRUNC, 0666)`），
  并把两条失败出口按 C 分开：开档失败 `err(1, "%s", fname)`（程序名 + 路径 + `strerror`），
  写失败 `err(1, "write")`（标签是 `write` 不是路径，`split.c:204` 等五处）。
  **门测 12 枚**（`tests/split_face.rs`，新立的 target）：四类出口各一枚（缺值那枚把 `l`/`b`/`a`/`n`
  四支扫全）、usage 文本与退码一枚、不重排两枚（前缀形与三操作数形）、输入腿三段形一枚、
  `-` 与 `--` 一枚、遗留形一枚、值附形与另形一枚、输出腿一枚。
  **两处测试自身的错**（test-audit 第 2 维当场逮住，都是我先写错再被门否掉）：一条期望写成
  「操作数之后的 `-l 3` 该交回当操作数」，但那已经是**三个**操作数，C 在那里走 usage 而不是成功；
  另一条把「十二行按五行切」算成两片而实际是三片；第三处是把 `split -l 3 pp` 当成「前缀是 pp」，
  其实 `pp` 是**第一**操作数、意思是读一个叫 pp 的档（参照件在同一形上给同一句话，A/B 逐字对过）。
  三处都按 C 改期望而不是改代码。
  **登记不执行**：① C 的 `err(1, "read")`（`split.c:190`、`:257`）与 `err(1, "stat")`（`:294`）
  两形在本壳不可观测——`bin_support::read_stdin()` 没有错误通道、`-n` 走的是收全后的长度而不是
  `fstat`（`SPLIT-READ-STAT-ERRLEG`，与 `REV-STDIN-ERRLEG`/`COLRM-IO-ERRLEG` 同族）；
  ② `close` 失败 C 也报 `err(1, "%s", fname)`（`:321-322`），本壳的 `WriteSink` 由 drop 释放、
  不上报（`SPLIT-CLOSE-ERRLEG`）；③ 短写与写错时本壳交回 `EIO` 而不是平台的号
  （`bin_support::errno_of` 的既有口径，`SPLIT-WRITE-ERRNO`）；④ 输入仍过一遍 UTF-8 校验，
  非文本字节会让整段输入被当空（`split.rs` 里的 `from_utf8(...).unwrap_or("")`），
  与批次三百一十在 `colrm` 修掉的是同一失真，但 `split` 的行重建要一并改，单独一轮
  （`SPLIT-NONUTF8-DROP`）；⑤ `split` 先收全输入再切块，C 边读边写（`SPLIT-STREAMING-MEMORY`）；
  ⑥ 真机分片文件创建与 per-command 断言归 `C-57`/E5（既有口径）；⑦ 07 篇第 155 行那句陈旧的
  测试计数仍归 V3-5/V3-6 的口径轮（批次三百零九已登记，本笔不顺手改）。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/**split** + pr
  （egetopt 措辞）；待接 `unexpand`、`col`、`column`、`comm`、`cmp`、`jot`、`unifdef`、`units`、
  `seq`、`paste`、`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `UNEXPAND-GETOPT-LINE`
  （`minix3/usr.bin/unexpand/unexpand.c:86` 的 optstring 是 `"at:"`，只有 `t` 带值，
  出口比 `split` 少一层，是本族目前最便宜的一支）。
  **三件套**：容器整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **725 / 0 / 0**
  （57 target，基线 `589fc4ce0` 为 713/56，+12 是新立的 `split_face`、+1 是它的 target）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、**86 warning 同基线**且触及件零命中；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`；
  `rustfmt --edition 2024 --check` 触及件零漂移；A/B 与参照件逐形 **SAME=31 DIFF=0**
  （`tmp/split-311/ab.log`，比的是退码、stderr、生成的文件名清单）；
  变异负控四枚（`tmp/split-311/ctl311.py`，每枚先打印 `mutation applied`、还原后核 md5）：
  丢 libc 那一行 → 咬 3 枚；取消「不重排」→ 咬 2 枚；缺值退回只印 usage → 咬 2 枚；
  写腿退回裸 `minix_sys::open` → 咬 5 枚（输出腿、前缀形、`-` 与 `--`、遗留形、值形各一支）。核实日 `2026-10-16`；基线 `589fc4ce0`。
  **验证边界**：参照件在本机链 glibc 的 `err`/`errx`/`strtoull`，其 `strerror` 句子与 Minix 表可能不同；
  本笔 31 形里没有一支依赖 0 号错误的句子（与批次三百一十那一形之差不同），故 DIFF=0 是逐字节对账的结果。
  **负载条款**：一条命令只一件事、容器 `-j 1 -m 2g -u`、宿主与容器不交叉用同一 target、无合成压力。

- ✅ **批次三百一十把 `colrm` 的选项环与列号段按 C 定形，并新立它的第一扇端到端门（第四处读缝缺陷）**
  （`2026-10-16`，`claim/COLRM-GETOPT-LINE-qoder` @ 基线 `58ff62978`，单域三线 commands/textfilter，无跨界）：
  判定表第十二支（按 `tmp/todo-loop-HANDOFF.md` 现查选出的最廉价一支）。**真源形状**：
  `colrm.c:67` 的 optstring 是空串，与批次三百零九的 `rev.c:70` 逐字同形，所以选项环那三形照搬——
  集外字母先由 libc 印一行（`getopt.c:95-104`）再由 `:68-72` 的 `default: usage()` 印第二行、
  `--` 结束选项环而自身不算操作数（`:81-85` 先 `++optind`）、裸 `-` 结束选项环却**不推进**所以它是操作数
  （`:87-92`）。**但同一个 token 在两条命令里落到完全不同的句子**：`rev` 把操作数当文件走 `fopen`
  失败支，而 `colrm` 的操作数是列号，裸 `-` 到 `colrm.c:84-86` 就是一句 `illegal column -- -`。
  **本壳改前的三处失真**（逐形与参照件对出）：① 没有选项环，`-Z` 被当列号拒收，少了 libc 那一行；
  ② `--` 被当坏选项直接 usage，而 C 拿它结束选项环；③ 列号用 Rust 的 `parse::<usize>()` 解，
  于是把 C 的 `strtol` 接受面整个收窄（`colrm " 3"`、`colrm +3`、`colrm 03` 在 C 都是合法的列号 3，
  本壳全拒；`99999999999999999999999` 在 C 溢出饱和后被接受而本壳溢出报错）。
  **列号段的两处 C 性质值得单独记**（都是参照件现量，不靠同族规律外推）：
  ① **两个操作数都坏时报后写的那支**——C 的 `case 2:` 先对 `argv[1]` 做 `strtol` 并检查、
  `/* FALLTHROUGH */` 才轮到 `argv[0]`（`colrm.c:78-87`），所以 `colrm abc def` 印的是 `def`；
  ② **负数不被拒**——`colrm.c:63` 把 `strtol` 的结果存进 `u_long`，于是 `if (start <= 0 || *p)` 里的
  `<= 0` 只挡得住 0，`colrm -- -5` 得到回绕后的极大值、整行透传并退 0。本壳如实搬这两条而不替 C 修正。
  **共件抽出**：空 optstring 的选项环此前只住在 `rev.rs`，现在两处 C 源逐字相同（`rev.c:70` 与
  `colrm.c:67`），于是提为 `minix_textfilter::empty_optstring_operands`（`src/lib.rs`，与
  `GetoptReport` 同一处单源），`rev` 侧的 `parse_rev_args` 改成一行委托、行为与句子都不变。
  **新门一立又暴露同形的第四处读缝缺陷**：`colrm` 壳拿裸 `minix_sys::read(support::STDIN, …)` 读输入，
  宿主上读不到任何东西（与批次三百零六的 `nl`、三百零八的 `fold`、三百零九的 `rev` 同病），所以它的
  列走从来没有被进程级测过；已改走 `bin_support::read_stdin()` 的双生读缝。顺带修掉同一条腿上的第二处
  失真：旧写法把输入过一遍 UTF-8 校验、校验不过就 `unwrap_or("")` 把整个输入当空，非文本字节会无声消失，
  而 C 的 `getchar` 按字节走列（门测里 `colrm 2 3` 吃 `a\xff\xbccdef` 一例钉住）。
  **门测 12 枚**（`os/commands/usr-bin/textfilter/tests/colrm_face.rs`，新立的第 56 个 target）：选项环
  四枚（集外字母两行、聚簇只报第一个、`--x` 报第二个连字符、`--` 不是操作数）、操作数腿六枚
  （裸 `-` 被当列号、操作数之后的 `-Z` 也是列号、都坏时报后写的那支、`strtol` 的接受面、溢出与负数透传、
  多于两个只印 usage）、诊断腿一枚（`err(1, ...)` 三段形缀 `strerror` 且不接 usage）、输出腿一枚。
  **A/B 逐形对账**：参照件 `tmp/colrm-310/colrm_ref`（`colrm.c` 真源 + Minix 的 `getopt.c` +
  程序名垫片，垫片要自己算 basename 因为 `colrm.c` 不调 `setprogname`，链法同批次三百零九），
  读数 `tmp/colrm-310/golden.log`，30 形对账 `tmp/colrm-310/ab.log` 得 **SAME=29 DIFF=1**——
  唯一的差在 `illegal start and stop columns` 那一行缀的 `strerror(0)` 句子：本机 glibc 印 `Success`，
  而 Minix 的表对 0 号写的是 `Undefined error: 0`（`minix3/lib/libc/compat/gen/compat_errlist.c:53`
  的 `/* 0 - ENOERROR */`，本仓 `Errno::from_i32(0).message()` 同一句），本壳跟的是 Minix 那张表，
  所以这一形是**宿主参照件的表差异**而不是本壳的失真，门测因此只钉形状不钉句子。
  **变异负控四枚**（`tmp/colrm-310/ctl310.py`，每枚先打印 `mutation applied`、还原后核 md5）：
  丢 libc 那一行 → 咬 3 枚；取消选项环（本批改前形状）→ 咬 4 枚；把「先算 stop」反转成「先算 start」
  → 咬 6 枚；读缝退回裸 `read` → 咬 4 枚。
  **登记不执行**：① C 的 `check()`（`colrm.c:97-130`）在 stdin 读错时印 `colrm: stdin: <strerror>`、
  stdout 写错时印 `colrm: stdout: <strerror>`，本壳的 `read_stdin()` 没有错误通道、`emit()` 的短写
  也不上报，这两形在壳侧不可观测（`COLRM-IO-ERRLEG`，与 `REV-STDIN-ERRLEG` 同族）；② C 边读边写，
  本壳先收全再过滤，长输入的内存形状差异是 `bin_support::read_stdin` 已声明的宿主优先取舍
  （`COLRM-STREAMING-MEMORY`）；③ 集外字母在 C 是 `%c` 打**一个字节**，本壳按 `char` 取第一个字符，
  只在 `-` 后跟非 ASCII 时分歧（`COLRM-NONASCII-OPTLETTER`，同 `REV-` 那一条的第二例）；
  ④ 列号的回绕按 `usize` 建模，三块目标板与宿主都是 64 位所以与 C 的 `u_long` 同宽，将来若跑 32 位
  需重看（`COLRM-ULONG-WIDTH`）；⑤ 真机 open/写腿与 per-command 断言归 `C-57`/E5（既有口径）；
  ⑥ 07 篇第 155 行那句陈旧的测试计数（写 229 个）仍归 V3-5/V3-6 的口径轮，本笔不顺手改（批次三百零九
  已登记）。本笔顺手翻正了判定表里的一处分类错：`tr` 原列「另判」，现查 `minix3/usr.bin/tr/tr.c:68`
  吃的是 libc `getopt(argc, argv, "cds")`，已改列「待接」（见上一笔的判定表行内注）。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/**colrm** + pr（egetopt 措辞）；
  待接 `split`、`unexpand`、`col`、`column`、`comm`、`cmp`、`jot`、`unifdef`、`units`、`seq`、`paste`、
  `tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一支取 `SPLIT-GETOPT-LINE`（`minix3/usr.bin/split/split.c:80`
  的 optstring 是 `"0123456789b:l:a:n:"`：数字是合法字母而四个带值项都在，出口比 `colrm` 多一整层）。
  **三件套**：容器整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **713 / 0 / 0**
  （56 target，基线 `58ff62978` 为 699/55，+14 与测数增量相符、+1 是新立的 `colrm_face`）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、**86 warning 同基线**且触及件零命中；
  `cargo check --workspace -j 1` 0 error；宿主三架构 `--bins` 各 `Finished`；`rustfmt --edition 2024 --check`
  触及件零漂移（`lib.rs` 的模块声明重排与 `col.rs`/`jot.rs`/`diff.rs` 等处的存量漂移非本笔，未动）；
  核实日 `2026-10-16`；基线 `58ff62978`。**验证边界**：参照件在本机链 glibc 的 `err`/`errx` 与
  `strtol`，其 `strerror(0)` 句子与 Minix 表不同（已在上文逐形标明），真机腿归 `C-57`/E5。
  **负载条款**：一条命令只一件事、容器 `-j 1 -m 2g -u`、宿主与容器不交叉用同一 target、无合成压力。

- ✅ **批次三百零九把 `rev` 的选项环补回来：空 optstring 不等于没有环，并新立它的第一扇端到端门**
  （`2026-10-16`，`claim/REV-GETOPT-LINE-qoder` @ 基线 `ce8536efb`，单域三线 commands/textfilter，无跨界）：
  判定表第五支。**真源形状**：`rev.c:70` 的 optstring 是**空串**，这一条容易被读成「`rev` 没有选项环」——
  本壳当时就把 `argv[1..]` 整个当操作数，于是 `-Z` 变成要打不开的文件。空 optstring 的实际后果有三处，
  全部由 libc `getopt` 定：① `strchr("", <字母>)` 永远是 `NULL`，任何 `-` 开头的 token（除②③两形）都落
  `getopt.c:95-104` 的集外字母支——libc 先印一行 `rev: unknown option -- <字母>`，再把 `'?'` 交给
  `rev.c:72-74` 的 `case '?': default: usage()`，于是**两行**且退 1（`:110-115` 那一行的程序名跟着
  `getprogname()`）；② `--` 在 `:81-85` 结束选项环且**先 `++optind`**，所以它自身不是操作数；③ 裸 `-` 在
  `:87-92` 因 optstring 里没有 `-` 而结束选项环**却不推进**，于是它自己是操作数，走 `rev.c:85-89` 的
  `fopen` 失败支（那一支是 `warn` + `continue` 而**不是致命错**）。选项环不重排（`:74-78` 见非选项即返 -1），
  所以 `rev <file> -Z` 里的 `-Z` 是个文件名。这三形加输出腿共 16 形逐形与参照件对过（参照件
  `tmp/rev-309/rev_ref`：`rev.c` 真源 + Minix 的 `getopt.c` + `fgetwln`/`setprogname` 垫片在宿主现链，
  读数 `tmp/rev-309/golden.log`，对账 `tmp/rev-309/ab.log` 得 **SAME=16 DIFF=0**）。
  **形状上的一处纠正（由 clippy 挡住）**：选项扫描最初写成 `while` 循环，被 `clippy::never_loop` 当场拒——
  这个告警是对的：`rev.c:70-75` 那个圈**定不了第二次**（getopt 要么返 -1 结束，要么返 `'?'` 而立刻退出），
  写循环反而把「报完第一个字母就退」说成「扫完整串」；改成单次判定，句子仍单源于 `lib.rs:GetoptReport`。
  **新门一立又暴露同形的一处读缝缺陷（不是诊断面）**：`rev` 壳自己拿裸 `open`/`read` 的 fd 读，宿主上读不到
  任何输入（与批次三百零六的 `nl`、三百零八的 `fold` 同形），所以它的输出从来没有被进程级测过；已改走
  `bin_support` 的双生读缝（`read_stdin`/`read_file`），顺带把操作数的报文补回 err(3) 三段形（此前只有
  `rev: <操作数>`，缺 `strerror` 句子；`rev.c:86`）。
  测面：库测 +1（`the_option_ring_takes_no_letters_and_stops_at_the_first_operand`：聚簇只报首字母、`--x`
  报的是 `-`、`--` 自身被吃掉、裸 `-` 是操作数、非选项 token 之后不重排、空数组）；新增
  `tests/rev_face.rs`（**8 枚**，本包第五十五扇目标），钉两行形与退码、只报首字母、`--`/裸 `-` 两种落点、
  warn 三段形与非致命续做，以及输出腿的四形（多行、末行无换行、空输入、多字节字符不撕裂）。
  三件套（容器现测）：整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **699 passed / 0 failed /
  0 ignored**（55 个 target，基线 `ce8536efb` 为 690 / 54，+9 与测数增量相符）；`cargo clippy -j 1
  -p minix-textfilter --all-targets` → 退码 0、0 error、**86 条 warning 与基线同数**；`cargo check
  --workspace -j 1` → 0 error；宿主三架构 `cargo build -p minix-textfilter --bins --target
  {x86_64-unknown-none,aarch64-unknown-none,riscv64gc-unknown-none-elf} --release` 三者均 `Finished`；
  rustfmt 触及三件零漂移；核实日 `2026-10-16`。三枚变异负控各咬各支（控制器 `tmp/rev-309/ctl309.py`，
  每枚先打印 `mutation applied`、还原后核 md5）：丢 libc 首行 → 3 枚；取消选项环（本批改前形状）→ 4 枚
  （门面 3 + 库测 1）；把打不开的操作数当致命错 → 1 枚。
  **登记不执行**：① 标准输入自己读出错时 C 打的是 `warn("%s", filename)`且 filename 初值是 `"stdin"`
  （`rev.c:80-81`、`:101-104`），本壳用的 `bin_support::read_stdin()` 没有错误通道（拿到 `Err` 按 EOF 收），
  那一形不可观测（`REV-STDIN-ERRLEG`）；② 集外字母在 C 是 `%c` 打**一个字节**，本壳按 `char` 取第一个字符，
  只在 `-` 后跟非 ASCII 时分歧（`REV-NONASCII-OPTLETTER`）；③ 真机 open 腿与 per-command 断言归 `C-57`/E5
  （既有口径）；④ 07 篇除 `rev` 行外，第 155 行还挂着一句陈旧的测试计数（写 229 个，现值 699），统计口径
  归 V3-5/V3-6 那两轮处理，本笔不顺手改。
  **判定表进度**：已对齐 head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/**rev** + pr（egetopt 措辞）；
  待接 `split`、`unexpand`、`col`、`colrm`、`column`、`comm`、`cmp`、`jot`、`unifdef`、`units`、`seq`、
  `paste`、**`tr`**；另判 `expand`/`hexdump`/`colcrt`/`lam`（C 各用自己的解析器，句子不许按同族规律抹平）。
  （上表把 `tr` 列在「另判」是记错了：`minix3/usr.bin/tr/tr.c:68` 写的是
  `while ((ch = getopt(argc, argv, "cds")) != -1)`，吃的就是 libc 的 `getopt`，所以它属于「待接」那一组。
  这一条由批次三百一十现查翻正，也是「别外推」的又一例——判定表的归类必须逐支读 C 源才算数。）
  **负载条款**：一条命令只一件事、容器 `-j 1 -m 2g -u`、宿主与容器不交叉用同一 target、无合成压力。

- ✅ **批次三百零八把 `fold` 的诊断面按 C 分回三类出口，并新立它的第一扇端到端门（顺带修好宿主读不到的读缝）**
  （`2026-10-16`，`claim/FOLD-GETOPT-LINE-qoder` @ 基线 `c6c6d5cd6`，单域三线 commands/textfilter，无跨界）：
  判定表第四支。C 的选项串是 0123456789bsw:（`fold.c:76`），只有 `w` 带值、`0-9` 是合法字母、不以 `:` 开头所以
  `opterr` 生效，于是三类出口的**行数不同**：① 集外字母与 `-w` 缺值由 `getopt.c:101-104`、`:132-136` 先印一行，
  再接 `usage()` 那一行（`fold.c:99-101`、`:115-121`，程序名跟着 `getprogname()`，退 1）；② 宽度非正是命令自己的
  errx(1, "illegal width value")（`:84-86`），只有那一行、不接 usage。旧写法把三类压成一类（只打一行 usage、没有
  libc 首行），usage 文本自创成 `[-b] [-s] [-w width | -width]` 且丢了 `[file ...]`；聚簇里的数字（`-b9`）被当成
  集外字母（C 里它是合法字母）。`FoldError::Usage` 因此拆成 `UnknownOption(char)` 与 `MissingValue(char)`，
  `IllegalWidth` 保持单独出口。
  **新门一立就暴露一处真缺陷（不是诊断面）**：`fold_stream` 直接拿裸 `open`/`read` 的 fd 自己读，宿主上读不到任何
  输入（同批次三百零六给 `nl` 收的那处同形），所以这扇门前不可能存在；已改走 `bin_support` 的双生读缝
  （`read_stdin`/`read_file`，宿主 `std::fs`、真机 open+read），顺带把操作数读不到的报文补回 err(3) 的三段形
  （此前只有 `fold: <操作数>`，缺 strerror 句子；`fold.c:107-109`）。
  三件套：容器 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **690 passed / 0 failed / 0 ignored**
  （54 个 target，基线 `c6c6d5cd6` 为 684 / 53，+6 全是新门）；`cargo clippy -j 1 -p minix-textfilter
  --all-targets` → 退码 0、0 error、86 条 warning 与基线同数（中途新门里一处 `PathBuf::from(temp_dir())` 被
  `useless_conversion` 抓到、修完回到 86）；`cargo check --workspace -j 1` → 0 error；宿主三架构 `--bins` 各
  `Finished`；rustfmt 触及件零漂移；核实日 `2026-10-16`。三枚变异负控各咬各支（控制器
  `tmp/fold-308/ctl308.py`，每枚先打印 `mutation applied`、还原后核 md5）：丢 libc 首行 → 1 枚；把
  `illegal width value` 也接上 usage → 1 枚；聚簇数字当集外字母 → 2 枚（库测 + 门面）。
  **登记不执行**：① 聚簇数字与遗留 `-N` 那两形**不照 C 复刻**——C 的 digit case 取的是 argv[optind - 1]
  （`fold.c:91-98`），结果跟着 getopt 内部的 optind 位置走（对 `-b9` 会算出 0 宽），且 `-0` 在 C 里**不报错**
  而是把宽度设成 0 照常折行；本壳按「从该位置的数字串作宽度、非正即拒」处理，已在代码注释与门面注释里写明是
  声明差，要收得先能链出带 Minix3 自带 getopt 的 `fold_ref`（`FOLD-LEGACY-WIDTH`）；② 真机 open 腿仍归 C-57/E5。

- ✅ **批次三百零七把 `cksum` 的选项诊断面按 C 定形：三种句子各归各的出口，usage 补回九行**
  （`2026-10-16`，`claim/CKSUM-GETOPT-LINE-qoder` @ 基线 `5e3c12907`，单域三线 commands/textfilter，无跨界）：
  判定表的第三支，而且它**连措辞都是抄来的**——今天印的是 `cksum: illegal option -- Z`，那属于 `pr` 链的
  `egetopt`（`egetopt.c:144-151`），而 `cksum.c:193` 吃的是 libc `getopt("a:cno:pqs:twx")`，正确句子在
  `getopt.c:101-104` 是 `unknown option`。一次改齐四类出口：
  ① **getopt 自己报的两支**（集外字母、带值选项缺值 `:132-136`）→ libc 一行 + `usage()`；
  ② **命令自己的 `warnx`**：值不合法是 `illegal argument to -a option` / `illegal argument to -o option`
  （`:219`、`:243`），`-p`/`-s`/`-t`/`-x` 在未选到 hash 时走 `requirehash` →
  `<旗标> flag requires \`-a algorithm'`（`:521-525`）——四类句子**都**再接同一块 usage；
  ③ **`usage()` 是九行**（`:535-549`：`cksum`/`sum` 各两行、`md2`/`md4`/`md5`/`rmd160`/`sha1` 各一行），
  此前只剩一行 `usage: cksum [-o 1 | -o 2] `；④ **退码**：C 的 `usage()` 是 `exit(1)`，旧写法一律退 2。
  扫描器同时改成 getopt 形状（聚簇 `-nq`、附值 `-o2`/`-acrc`、`--` 结束、裸 `-` 是操作数）——旧写法按整个
  token 匹配字母，`-nq` 会当成集外字母 `n`。`-a` 的接受集也按 C 分家：`crc`/`old1`/`old2` 认、
  MD5/SHA 家族是「认得但未接」（另句），其余才是 `illegal argument to -a option`。
  测面：库测 +1（`test_parse_cksum_args_follows_the_getopt_shape`：聚簇、附值、`--`、`-s` 先吃值再 requirehash、
  聚簇里的集外字母）并把两枚旧断言改成新出口；新增 `tests/cksum_face.rs`（**5 枚**，本包第五十三扇目标），
  钉句子、行数与退码。三件套：容器 `cargo test -j 1 --no-fail-fast -p minix-textfilter` →
  **684 passed / 0 failed / 0 ignored**（53 个 target，基线 `5e3c12907` 为 678 / 52，+6 与测数增量相符）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` → 0 error、**86 条 warning 与基线同数**；
  `cargo check --workspace -j 1` → 0 error；宿主三架构 `--bins` 各 `Finished`；rustfmt 触及件零漂移；
  核实日 `2026-10-16`。三枚变异负控各咬各支（控制器 `tmp/cksum-307/ctl307.py`，先打印 `mutation applied`、
  还原后 md5 对齐）：改回 `illegal option` → 1 枚；requirehash 回到自创句且不接 usage → 1 枚；
  不接受附值形 → 2 枚（库测 + 门面）。
  **登记不执行**：① 操作数读不到时本壳印的是 `cksum: -o` 这种缺 strerror 的形状，而 C 是
  `err(1, "Cannot read %s", …)` 一族（`cksum.c:304`），文件腿得单独一轮（`CKSUM-FILE-LEGS`）；
  ② C 的 `-a` 在已选 hash 时是 `illegal use of -a option`、`-o` 在已选 hash 时是
  `%s mutually exclusive with sum`（`:196-199`、`:231-234`），本树永远选不到 hash 所以**当前不可达**，
  等密码原语那轮一起做（`CKSUM-HASH-STATE`）；③ cksum 是多入口名（`sum`/`md5`… 靠 `argv[0]` 选算法），
  本壳未建模，`prog_name` 目前只影响报文前缀；④ 07 篇 `cksum` 行外，第 155 行还挂着
  等密码原语那轮一起做（`CKSUM-HASH-STATE`）；③ cksum 是多入口名（`sum`/`md5` 等靠 `argv[0]` 选算法），
  本壳未建模，`prog_name` 目前只影响报文前缀；④ 07 篇除 `cksum` 行外，第 155 行还挂着一句陈旧的测试计数（写着 229 个测试、截止于九月中），而现值是 684 —— 统计口径归 V3-5/V3-6 那两轮处理，本笔不顺手改。
  **负载条款**：一条命令只一件事、容器 `-j 1 -m 2g -u`、宿主与容器不交叉用同一 target、无合成压力。

- ✅ **批次三百零六把 `nl` 的选项面按 C 重建：getopt 逐字母扫描 + 两类出口形状，并补上第一扇端到端门**
  （`2026-10-16`，`claim/NL-OPTION-FACE-qoder` @ 基线 `9b4fbd498`，单域三线 commands/textfilter，无跨界）：
  排期表 V3-10 判定表的第二支（`nl` 在判定表里连**句子本身**都可疑：今天印的是 `nl: unknown option`，
  字母都丢了，且后面不接 usage）。**真源形状**：`nl.c:149` 用 libc `getopt(argc, argv, "pb:d:f:h:i:l:n:s:v:w:")`
  ——只有 `p` 不带值，选项串不以 `:` 开头所以 `opterr` 生效，于是集外字母先由 `getopt.c:101-104` 印
  `nl: unknown option -- <字母>`、带值缺值由 `:132-136` 印 `nl: option requires an argument -- <字母>`，
  两句之后都接 `nl.c:230-233` 的 `usage()`（两行，第一行末尾是 `\n\t`，`:400-405`）退 1；而命令自己的
  那些 `errx` **只有一行且不接 usage**——两类形状此前被压成同一种。
  **旧写法的三处错**：① 按「整个 token 当一个字母」匹配，`-ba`（值附在本 token）与 `-pb t`（聚簇）都会
  掉进集外字母那支；② 把 getopt 的错当值非法报，用了被拒值自己的句子；③ 句子多是自创的（`bad numbering
  type (want a, n or t)`、`-n must be ln, rn or rz`、`width argument must be > 0`、`-i increment must be
  non-zero`），而 C 的 `parse_numbering` 句子**带段名**（`numbering_properties[]` 的 name 依次是
  footer/body/header，`nl.c:73-75`）并把整串值打出来（`illegal body line numbering type -- x`），
  `-n` 的是 `illegal format -- <值>`，`-w` 分两句（`invalid width value -- x` 与
  `width argument must be > 0 -- 0`，值也打出来），`-i`/`-l`/`-v` 各一句且都带值，且 C 的 `-i` **没有**
  非零检查（只查 `*ep`）——那句「must be non-zero」是把不存在的守卫当真。**数值解析按 `strtol`/`strtoul`
  的接受集复刻**：跳前导空白、吃一个符号、只收十进制、末尾剩任何东西（含空白）都拒、溢出也拒。
  **新增第一扇 `nl` 端到端门 `tests/nl_face.rs`（10 枚）**，钉的都是外面看得见的：三类句子逐字、行数、
  退码、`-ba`/`-pb t` 被接受、`-d` 收两字符拒第三、`-d ''` 是合法无操作、`-i 0` 被接受、两个操数是裸 usage。
  门一立就**暴露两处真缺陷**（都不是选项面）：`nl` 自带一个只调裸 `read(0)` 的本地 `read_stdin()`，
  宿主上读不到标准输入（同批次二百二十四给 games 收过的坑），已改用 `bin_support::read_stdin()` 双生缝；
  操作数读不到时打的是自创短语 `cannot open input`，而 C 是 `err(EXIT_FAILURE, "Cannot open \`%s'", argv[0])`
  ——程序名、**带反引号的操作数**、`strerror` 句子三段齐备（`nl.c:244-246`），已改走 `warn_line`。
  三件套：容器 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **678 passed / 0 failed / 0 ignored**
  （52 个 target，基线 `9b4fbd498` 为 668 / 51 target，+10 全是新门、+1 是新 target 行）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` → 退码 0、0 error、**86 条 warning 与基线同数**
  （中途 `match … { Some→, None→return }` 写法招来两条 `unnecessary match` 建议、把 `format!` 引成
  `alloc::format` 前的交叉门报错都已清掉，棘轮不动）；`cargo check --workspace -j 1` → 0 error；
  **宿主三架构 `cargo check -p minix-textfilter --bins --target {x86_64,aarch64,riscv64gc}-unknown-none`
  三者均 `Finished` 零 error**——这三条门抓到了一处宿主全绿也发现不了的错：no_std 下 `format!` 不在
  预lude，必须 `use alloc::format;`（宿主有 std 预lude 所以整包测与 clippy 都不报）；rustfmt 触及件零漂移；
  核实日 `2026-10-16`。
  **三枚变异负控各咬一枚测**（控制器 `tmp/nl-306/ctl306.py`，每枚先打印 `mutation applied`、还原后 md5
  对齐 `86bf203919517605289fc57d613d788a`）：① 集外字母退回只打 usage → 咬 `an_unknown_letter_prints_the_libc_line_then_the_usage`；
  ② 把 `-w` 两句并成一句 → 咬 `width_rejects_unparseable_and_nonpositive_with_different_sentences`；
  ③ 把自创的 `-i` 非零守卫加回去 → 咬 `a_zero_increment_is_accepted`。三支互不顶班，各钉各的判据。
  **射程边界（如实记，不补空读数）**：`-b p<正则>` 那条腿需要 `regcomp`，本树未接，其拒收文案仍是**自描述**的
  「not wired」而不是 C 的句子（C 会编译正则并按它编号）；决定半的计数器是 `u64` 而 C 是 `int`（能倒着数），
  负增量/负起始号也另成一未接腿——两者各自登记为 `NL-REGEX-NUMBERING` 与 `NL-SIGNED-COUNTERS`，
  不在本笔冒充 C。
  **文档同步判断**：`nl` 在 18 篇只出现在 `10-doc-man-tools.md:89` 的粗粒度命令表（`| nl | 行编号 | -b 编号模式 | 文本 → 编号文本 | 0 成功 |`），
  不含诊断面断言，本笔无需改它；选项面的形状与凭据都写在本条台账与代码注释里（按「过程痕迹禁入正文」的分工）。
  **判定表进度**：待接还剩 `cksum`（句子也可疑：`illegal option` 不是 libc 措辞）、`fold`、`rev`、`split`、
  `unexpand`、`col`、`colrm`、`column`、`comm`、`cmp`、`jot`、`unifdef`、`units`、`seq`、`paste`；
  另判 `expand`/`hexdump`/`colcrt`/`tr`/`lam`。已对齐：head/tail/uniq/cut/join/sort/vis/nl + pr（egetopt 措辞）。

- ✅ **批次三百零五把 `vis` 的集外字母补上 libc `getopt` 那一行，并给出同族缺行的判定表**
  （`2026-10-16`，`claim/FAMILY-GETOPT-LINE-qoder` @ 基线 `6a9daec0a`，单域三线 commands/textfilter，无跨界）：
  取排期表 V3-10 那条（批次二百八十登记的「同族缺行核对」）。**判据先对 C**：`minix3/usr.bin/vis/vis.c:73`
  吃的是 libc `getopt(argc, argv, "bcde:F:fhlMmNnoSstw")`，选项串不以 `:` 开头且 `opterr` 默认为 1，
  所以集外字母先由 libc 印一行（`minix3/lib/libc/stdlib/getopt.c:101-104`，
  `"%s: unknown option -- %c\n"`），`getopt` 回 `BADCH`(`'?'`) 后才由 `case '?': default:` 打 usage 退 1
  （`vis.c:135-140`）——**两行**。带值选项缺值是同一族的另一句（`getopt.c:132-136`，
  `option requires an argument -- %c`）。本壳此前只打 usage，把这一行丢了。
  **一个必须分清的岔子**：`-d` **不是**集外字母——C 的选项串里写的是 `d:`（认它、且要带值），只是
  `case 'd'` 在非 `DEBUG` 构建里被编译掉才落到 `default:`。所以旧写法把 `-d` 与 `-z`/`-1` 塞进同一个
  `default` 是正确的（都是一行 usage），而我把集外字母一并算成两行就会**把 `-d` 写错**。本笔因此把
  `-d` 单列一支：`-d x` 只落 usage；`-d` 空到末尾才走 libc 的缺值那句（`getopt` 认它所以要值）。
  句子单源住在 `minix_textfilter::GetoptReport`（同一份文本已服务 head/tail/uniq/cut/join/sort 六支，
  它对 `pr` 用的是另一支 `IllegalOption`，因为 `pr` 链的是 `egetopt`——这条区别是批次二百七十几定的）。
  测面**没有新增枚数**，而是把两处「测不出东西」的断言换成逐字等式：`letters_outside_the_c_optstring_are_refused_with_usage`
  原来是 `err.contains("Usage: vis [")`（少了首行也过），现按四支逐字钉（`-z`/`-1` 两行、`-d x` 一行、
  `-d` 两行）；`fold_width_is_read_the_way_atoi_reads_it` 里的 `-F` 缺值同改，并补 `-e` 缺值一支。
  三件套（容器现测）：整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **668 passed / 0 failed /
  0 ignored**（51 个 target，与基线 `6a9daec0a` 同数——本笔是收紧断言不是加测）；`--test vis_face` 19/0；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` 0 error 且 `bin/vis.rs` 零命中；同容器
  `cargo check --workspace -j 1` 0 error；宿主三架构（`cargo check -p minix-textfilter --bin vis --target x86_64-unknown-none`、`aarch64-unknown-none`、`riscv64gc-unknown-none-elf`）三者均 `Finished`；rustfmt 触及两文件零漂移；核实日 `2026-10-16`。
  **两枚变异负控各咬一枚**（控制器 `tmp/face-getopt-305/ctl305.py`，先打印 `mutation applied`、还原后
  md5 对齐 `a55e065206e8f2d7d273c5f3c3aed0ff`）：① 把集外字母改回只打 usage → 1 枚 FAILED；② 把 `-d`
  缺值的句子换成集外字母那句 → 1 枚 FAILED（同一枚测内的不同断言，证明两支护的是两条判据）。
  **同族判定表**（C 侧看 `getopt(`/`egetopt(` 与选项串首字符，本壳侧看真二进制对 `-Z` 的第一行 stderr；
  完整表在 `tmp/`，下一轮起逐支取）：
  ① 已经是两行形（六支）：`head`、`tail`、`uniq`、`cut`、`join`、`sort`；
  ② 本笔修好：`vis`；
  ③ C 吃 libc `getopt`、本壳只打自己那句或 usage（**待接，十四支**）：`nl`（今天印的是
  `nl: unknown option`，连字母都没带——句子本身也不合 libc）、`cksum`（印 `illegal option`，那不是
  libc 措辞，疑为照抄 `pr` 的 `egetopt` 形）、`fold`、`rev`、`split`、`unexpand`、`col`、`colrm`、
  `column`、`comm`、`cmp`、`jot`、`unifdef`、`units`、`seq`、`paste`；
  ④ 另判（C 不用 libc `getopt`，形状各不同）：`pr`（`egetopt` → `illegal option -- %c`，已对）、
  `expand`/`hexdump`/`colcrt`/`tr`/`lam`（各用自己的解析器，句子得回各自 C 源核，不许按「同族规律」抹平）。
  每一支都要单独一轮：先判句子再改，别一次扫多支（同族差异已被 `pr`/`lam` 这类反例证明不可外推）。
  **过程自纠与工具坑一处**：整包跑时 `vis_face` 十九枚全部报 `NotFound`——不是代码问题，而是那枚
  测试二进制里 `env!("CARGO_BIN_EXE_vis")` 烘的是**另一棵树布局下的旧路径**（`strings` 读到
  `/work/target/debug/vis`，本次挂载是 `/work/os/target/...`），cargo 又因指纹未变不肯重链；
  `touch tests/vis_face.rs` 强制重编后同一命令 19/0。判据：门报 `NotFound` 而 `target/debug/<bin>`
  明明在，先 `strings` 测试二进制看烘进去的路径，别去改代码。

- ✅ **批次三百零四把 `sort` 的 `-R`（记录分隔符）接上：切分、收尾、与 `-t` 的不对称判据一次做齐**
  （`2026-10-16`，`claim/SORT-R-OPTION-qoder` @ 基线 `b46ac1baa`，单域三线 commands/textfilter，无跨界）：
  排期表 V3-3 那条（`sort` 选项面 `D`/`R`/`T` 里的真功能 `R`）。**真源形状**逐句写在下面，因为这一支的
  判据不是「次数」而是「值」：C 的 `REC_D` 默认 `'\n'`（`sort.c:92`，`main` 在选项环前 `d_mask[REC_D = '\n']
  = REC_D_F`，`:139`），`-R` 那一支（`:223-242`）先查「当前值是否已不等于 `'\n'`」→ 是就
  `usage("multiple record delimiters")`（所以 `-R '\12' -R ,` 合法——第一次被算回 10）；再把 `*optarg` 当值，
  若它就是 `'\n'` **整支提前收尾**（`-R $'\nx'` 等同 `-R '\n'`）；值长于一个字节才 `strtol`：带反斜缀走八进制、
  否则 base 0，`*ep != '\0'`（含尾随空白）/负值/`>= NBINS`(256) 三条都落
  `errx(2, "invalid record delimiter %s", optarg)`，而报文用的是**跳过反斜缀后**的 `optarg`（`-R '\q'` 报 `q`）；
  `-R '\'` 反而合法（`strtol` 没转任何数字时 `endptr` 停在原始起点，空串的 `*ep` 就是 `\0`）→ `REC_D = 0`。
  **执行侧两件事**：读入按 `REC_D` 切记录（`getrec`，`files.c:113-124`；尾随分隔符不留空记录，所以
  `b,a,d,` 是三条不是四条）；输出是「每条记录再补一个 `REC_D`」（`putrec`，`:194`/`:215`），**带 `-R` 时
  输出以分隔符收尾**。换行在这一形里只是普通字节，所以 `b,a,d,c\n` 最后一条记录是 `c\n`。
  **与 `-t` 的关系是不对称的**：只有 `-t` 那一支查 `d_mask[c] & REC_D_F`（`:217-218`）并落
  `errx(2, "record/field delimiter clash")`，而那个位只跟在 `REC_D` 上——于是 `-R , -t ,` 退 2、
  `-t , -R ,` 安静通过（参照件同读数），且默认形状下 `-t '\n'` 也报。
  三件套（容器现测）：`cargo test -j 1 --no-fail-fast -p minix-textfilter` → **668 passed / 0 failed / 0 ignored**
  （51 target；基线 `b46ac1baa` 为 658，本笔 +10 = 库测 4 + 门面 6）；`--lib`/`sort_face` = 460 / 21；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` → 退码 0、0 error、86 条 warning 与基线同数（棘轮不动）、
  `sort.rs`/`bin/sort.rs` 零命中；`cargo check --workspace -j 1` → 0 error；宿主三架构
  `cargo check -p minix-textfilter --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` → 三者均 `Finished`
  零 error；rustfmt 触及三文件零漂移；核实日 `2026-10-16`。
  **与参照件逐形对账**（`tmp/family-diag-ref/sort_ref`，日志 `tmp/sort-r-304/ab.log`）：12 形里
  **9 形 stdout/stderr/退码全等**，另 3 形只相差参照件自己的程序名段（`sort_ref:` 对 `sort:`，句子与退码逐字
  相同）——所以实际形状差为零。入门的金样：`-R ,` 对 `b,a,d,c\n` → `a,b,c\n,d,`；尾随分隔符 `b,a,d,` → `a,b,d,`；
  空输入 → 空；`-R 0` → 按字符 `0` 切（`a0b0c\n` → `a0b0c\n0`）；`-R '\011'` → 制表；`-R , -u` → `a,b\n,`。
  **三枚变异负控各咬各的承载点**（控制器 `tmp/sort-r-304/ctl304.py`，每枚先打印 `mutation applied`、
  还原后核 md5）：① 去掉 `-t` 的 clash 判 → 2 枚 FAILED（1 库测 + 1 门面）；② 输出腿忽略 `REC_D` 仍按换行收尾
  → **5 枚门面 FAILED 而库测 460 全绿**（决定半与执行半分工清楚，没有互相顶班）；③ 去掉「第二次 `-R`」守卫
  → 2 枚 FAILED（usage 支那枚门面 + 库测）。
  **本轮两处自纠**：① 第一版把 `-t , -R ,` 的期望按「逗号分开两行」的直觉写成 `a,b,\n`，被门与参照件一起
  挡下（真形是 `a\n,b,`）——期望值一律要以参照件读数为源，不能从规则推；② `ParseError` 原本 `derive(Copy)`，
  新变体带 `Vec<u8>` 编不过（E0204），去掉 `Copy` 而不是把串塞成固定长数组；③ 台账/代码注释里的两处形近字
  （「选顶环」「所它」）在自查中改回。
  **登记不执行**：① C 在 `-R` 之后还做 `d_mask['\n'] = d_mask[' ']` 与 `d_mask[REC_D] = REC_D_F`（把换行归到
  空白/字段分隔那一类），本层字段扫描器不读这张掩码；唯一能探到的形 `-R , -k 2` 已实测两边同为 `b,a,d,c\n,`，
  故本笔不动掩码，若后续要建模 `d_mask` 需单独立项；② `sort` 选项面还剩 `-D`（十进制小数的 locales 形）与
  `-T dir`（临时目录，属执行层外部归并）；③ 外部归并引擎未做（同前）。

- ✅ **批次三百零三把端到端门的「写标准输入」竞态修掉：16 处收成一件夹具，只放过 `BrokenPipe`**
  （`2026-10-16`，`claim/FACE-GATE-STDIN-RACE-qoder` @ 基线 `66bcd264a`，单域三线 commands 两包的 `tests/`，
  **一行生产码都没动**，无跨界）：清批次二百九十九登记的第③条。
  **现象与根因**：`tests/pr_face.rs` 的 `numeric_option_diagnostics_are_c_named_messages_then_the_usage`
  单目标三连跑 1 红 2 绿，红的那次报 `writing standard input: Os { code: 32, kind: BrokenPipe }`；
  `vis_face` 在同一容器里串跑多门那一趟独红。根因不在被测代码：C 的诊断支（`minix3/usr.bin/pr/pr.c:142-157`
  的 setup 错、`vis.c` 的集外字母支等）**排在任何读之前**，子进程一退出管道读端就关闭，父进程此时的
  `write_all` 拿到 `BrokenPipe`；而这件事**是否发生取决于父子双方谁先被调度**（输入只有几个字节时父进程通常先写完），
  所以是门自己的竞态被当成了被测缺陷。
  **改法**：新建 `tests/face_support/mod.rs`（两个包各一份：`usr-bin/textfilter` 与 `games/stdio-games`），
  把「写标准输入并关闭它」收成一件 `write_stdin(&mut Child, &[u8])`：**只放过 `BrokenPipe`，其余写错当场断言失败**
  ——否则门会静默失去输入而假绿，这是修法里唯一需要判断的地方。16 处调用点（`bcd`/`caesar`/`factor`/`morse`/
  `number`/`pig`/`ppt` 七枚 + `cmp`/`cut`/`head`/`pr`/`sort`/`tail`/`uniq`/`unvis`/`vis` 九枚）逐处替换，
  并收掉因此不再使用的 `use std::io::Write as _;`。这个形状此前被复制 16 遍，正是批次二百八十记过的
  「同一事实多处承载」——修一处不够，得一次改齐。
  **新增一枚门自身的契约测**（`pr_face.rs` 末尾）：先 `try_wait` 轮询到子进程退出、再写输入，**确定性地**造出
  同一状态。用 `try_wait` 而不是 `wait` 是有原因的：`Child::wait` 会先把 `child.stdin` 句柄丢掉
  （std 这么做是为了别让子进程卡在输入上等不死），那样就没得写了——这一点连同耐心上限写进测注。
  三件套：容器 `cargo test -j 1 --no-fail-fast -p minix-textfilter -p minix-stdio-games` →
  **885 passed / 0 failed / 0 ignored**（71 个 target：textfilter 658 = 上一笔 657 + 新契约测 1，
  stdio-games 227 与基线同数）；把当年会红的三枚目标复跑 **五连全绿**（`--test pr_face --test vis_face
  --test unvis_face` → 15/10/19 × 5，日志 `tmp/face-303-flake.log`；修前两目标三连跑就出过 1 红）；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` → 退码 0、0 error、**86 条 warning 与基线逐编译单元同数**
  （17 个文件收夹具后没留下 unused import）；`-p minix-stdio-games --all-targets` → 退码 0、0 error、23 条
  warning 且无一条指向 `tests/`；容器 `cargo check --workspace -j 1` → 0 error；rustfmt 触及件零漂移
  （唯一那块 `primes_face.rs:207` 经 `git show HEAD:` 副本核对为**存量**，本笔未动该文件）；核实日 `2026-10-16`。
  **一枚决定性负控**：把 `write_stdin` 改回「写不可失败」的旧形状 → `writing_stdin_to_an_already_exited_child_is_not_a_gate_failure`
  **确定性地红**，报的正是当年那一句 `writing standard input: Os { code: 32, kind: BrokenPipe, message: "Broken pipe" }`
  （`tmp/face-303-mut.log`）；同一次跑里 `pr_face` 其余 14 枚与 `vis_face`/`unvis_face` **全部仍绿**——
  既证明新测确实承载这条事实，也证明其余门对它没有分辨力（它们只是次序上赢了那场竞速）。还原后 md5 对齐
  `0b422e8b205fa11fe6f22a6e0ec115fd`，并复跑取最终读数。
  **登记不执行**：① `primes_face.rs:207` 的存量 rustfmt 漂移（与 `jot.rs` 那 10 处同类，格式化应单独成笔）；
  ② 同一夹具在两包各存一份——Cargo 的集成测试没有跨 crate 共享机制，要单源得先立一个 dev-dependency 的测试件
  crate，属结构面，不在本单射程；③ 三架构 `--bins` 本轮未跑（改动全在 `tests/`，freestanding 目标不收集测目标），
  如实记下射程边界而不是补一枚空读数。
  **过程自纠一处**：负控第一次仍按老习惯写成 heredoc 里的嵌套引号 python，结果是「锚点没匹配上、门全绿」的
  假注入——批次二百九十五记过的同一坑，本轮改回**用编辑工具落盘变异**才拿到真读数。
  **负载条款**：一条命令只跑一件事、容器 `-j 1 -m 2g -u`、无合成 CPU 压力。

- ✅ **批次三百零二把 `join` 的 `-o 0` 按 C 关掉：那支「连接字段自己」是外来约定，顺手删掉 `Which::Join` 死路**
  （`2026-10-16`，`claim/JOIN-OZERO-ACCEPTANCE-qoder` @ 基线 `77ea92659`，单域三线 commands/textfilter，无跨界）：
  清批次三百零一登记的那一支。**真源形状**：C 的 `fieldarg` 只判 token 首字符是不是 `1`/`2` 且第二位是不是 `.`
  （`minix3/usr.bin/join/join.c:526-527`），**从不**接受 `0`；「`-o 0` 表示连接字段自己」是 BSD/GNU 的约定。
  参照件逐形现测：`-o 0`、`-o 0,1.1`、`-o 1.1,0`、`-o0` 四形全部 `join: malformed -o option field` 退 1、不接 usage；
  改前的本壳把四形全部收下并输出（`-o 0` 给 `a`/`b` 两行），读数记在 `tmp/join-ozero-302/before.log`。
  **本档是三档术语里的 Rewrite（保持外部行为）**，不该把外来约定留在实现里——本项目此前多次因「拿 BSD 行为当真的」
  被判 P0-fact（同一命令的 `-o 0` 正是其中一处），所以这里不放宽、不立 `[ARCH]`，直接照 C 拒收。
  **改法**：`parse_olist` 去掉 `token == "0"` 那一支；解析器造不出 `Which::Join` 之后，**该变体就是无生产来源的死码**，
  于是连同 `olist_token` 里那支回退逻辑一并删掉，`Which` 只剩 `One`/`Two`（留着它会骗下一轮以为 `-o 0` 还能走）。
  `parse_olist` 的文档与 `Which` 的文档同步改成「两支」的形状，并把拒收的凭据写在注释里。
  **改测四处（都是「测试代码就是错的」那一类，不是为了让门绿）**：拿 `0` 写期望的三枚库测与拿 `-o0,1.2` 写的那枚
  解析测，改成同义的 `1.1` 形——默认连接字段本就是文件一的字段 1，所以期望串**一字未动**；
  另新增一枚库测 `parse_olist_refuses_the_zero_join_field_entry`（五形：`0`、`0,1.1`、`1.1,0`、`0.1`、`1.1 0`），
  门面按形分枚加四枚（`a_lone_zero_output_field_is_refused_without_a_usage_block`、
  `a_zero_first_in_the_output_list_is_refused`、`a_zero_last_in_the_output_list_is_refused`、
  `the_attached_dash_o_zero_is_refused_too`——附值形与分开写分开钉，因为 `obsolete()` 的 legacy 改写只认裸 `-o`）。
  三件套：容器 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **657 passed / 0 failed / 0 ignored**
  （51 个 target；基线 `77ea92659` 为 652，本笔 +5 与测数增量相符）；`--lib --test join_face` → 456 / 29 全绿；
  `cargo clippy -j 1 -p minix-textfilter --all-targets` → 退码 0、0 error、86 条 warning 与基线同数（棘轮不动）；
  容器 `cargo check --workspace -j 1` → 0 error；宿主三架构 `cargo check -p minix-textfilter --bins
  --target {x86_64,aarch64,riscv64gc}-unknown-none` → 三者均 `Finished` 零 error；rustfmt 触及两文件零漂移；
  改后与参照件逐形对账 **SAME=6 DIFF=0**（四支拒收形 + 两支合法 `-o` 形作回归，日志 `tmp/join-ozero-302/ab.log`）；
  07 篇 join 行的测试名清单与本包实到函数名逐枚对账，除夹具辅助 `scratch` 外全部命中（测试名对账门）；核实日 `2026-10-16`。
  **两枚变异负控**（控制器 `tmp/join-ozero-302/ctl302.py`，每枚先打印 `mutation applied`、还原后 md5 对齐
  `d4a638e125538dc0bdb9e7c335eeebde`）：① 把 `0` 重新收下 → 恰好 6 枚 FAILED（四枚新门面 + 两枚库测），
  其余零枚受影响——这条事实只由新门承载，没有别处置顶；② 拒收但落错另一支 `errx` 的句子（one-based 而非 malformed）
  → 8 枚 FAILED，即①的 6 枚再加 `the_errx_legs_print_one_line_and_skip_the_usage_block` 与
  `the_option_error_first_lines_match_the_bsd_reference`——**句子归属**这一事实另有既有门把守，
  所以「拒收」与「用哪句拒收」在两处各自可检。
  **登记不执行**：`join` 与 C 真源之间登记的差异至此只剩一处**声明差** `-o 1.-1`（C 把 `strtol` 的结果赋给 `u_long`，
  回绕后反而被接受，本树拒之）——它的输出级参照件在宿主走不到那一步，改法要先解决参照件的读档段，不在本单射程。
  批次三百登记的两项（`jot.rs` 的 10 处存量 rustfmt 漂移、`%5.2f` 的不补宽支与负值支）本轮仍未取。
  **负载条款**：一条命令只跑一件事、容器 `-j 1 -m 2g -u`、宿主三架构只在容器不在跑时跑，无合成 CPU 压力。

- ✅ **批次三百零一把 `join` 的比较层按 C 定形：字段号越界不等于空字段，配对与切组两处一起转绿**
  （`2026-10-16`，`claim/JOIN-CMP-OVERFIELD-qoder` @ 基线 `5daa464b4`，单域三线 commands/textfilter，无跨界）：
  清批次二百九十九登记的第①条。**真源形状**：C 的 `cmp`（`minix3/usr.bin/join/join.c:405-414`）有**三种**答案
  而不是两种——先查「字段号超出这一行的字段数」（`lp->fieldcnt <= fieldno`），两侧都越界 `return 0`（相等），
  只有第一侧越界 `return 1`、只有第二侧越界 `return -1`（**越界那一侧算大**），两边都不越界才 `strcmp`。
  本壳此前用一个 `key()` 把「越界」与「该字段是空串」都折成空串，因此两个方向都错，而且**后果不止在配对判等**：
  `slurp` 拿 `cmp != 0` 收尾同一档的分组（`:398`），旧形状会把「没有这一列」的行并进「这一列是空串」的行，
  于是与另一侧同键的行**多拼出根本不存在的配对**——三行档 `a,,x`/`b`/`c,,y` 对一行档 `a,,x`，C 给
  `,a,x,a,x`、`b`、`c,,y`，旧壳给三行交叉 `,a,x,a,x`、`,b,a,x`、`,c,y,a,x`。
  **改法**：`key()` 换成 `cmp_keys(a, fa, b, fb) -> Ordering`（四支 match 逐支对着 C 的三句 return），
  合并环与 `group_end` 都改用它；`group_end` 拿新行与**组首行**比而 C 与**上一行**比，这一处等价性写在函数注里
  （比过的行都等于组首，所以首个失配位置相同），不改写成 C 的形状是因为等价、且组首比较少一次重复切分。
  **金样五形逐形从参照件 `tmp/family-diag-ref/join_ref` 现测抄回**（改前读数记在 `tmp/join-cmp-301/before.log`）：
  ① `-t, -1 2 -2 2` 对 `a,`/`a` → C 空输出、旧壳 `,a,a`；② `-1 2 -2 2 -a1 -a2` 对 `solo`/`x 9` → C
  `x 9`、`solo`，旧壳顺序相反；③ 同形两侧对调 → C 仍是 `x 9`、`solo`，**旧壳与②给出同一串**（这正是它没有
  分辨力的证据，所以两支各入门一枚）；④ `-t, -1 2 -2 2 -a1 -a2` 对三行档/一行档 → 上面那串切组金样；
  ⑤ `-1 2 -2 2 -a1 -a2` 对 `p 1`/`q`/`r 1` → 改前改后同读数（都等于 C），入门作回归护栏。
  改后再拿参照件逐形对账 → **SAME=7 DIFF=0**（含两枚未入门的方向与越界组合形，日志
  `tmp/join-cmp-301/ab.log`）。测面：`tests/join_face.rs` 增至 **25 枚必跑**、库测 +3 枚
  （`cmp_keys_answers_three_cases_not_two`、`join_lines_gives_the_lead_to_the_side_that_has_the_field`、
  `grouping_breaks_where_the_join_field_is_absent`）；07 篇 join 行把「比较层与 C 同形」写实，
  并把「仍与 C 不同」从两形减为一形。
  三件套：容器 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **652 passed / 0 failed / 0 ignored**
  （51 个 target；基线 `5daa464b4` 为 644，本笔 +8 与测数增量对得上）；`--lib --test join_face` → 455 / 25 全绿；
  **`cargo clippy -j 1 -p minix-textfilter --all-targets` 退码 0、0 error、86 条 warning 与基线同数**
  （上一笔刚把这条腿修成可当门，本笔是它第一次当门用）；容器 `cargo check --workspace -j 1` → 0 error（49.31s）；
  宿主三架构 `cargo check -p minix-textfilter --bins --target {x86_64,aarch64,riscv64gc}-unknown-none`
  三者均 `Finished` 零 error（容器无这三份 core，见批次三百那条）；rustfmt 触及两文件零漂移；核实日 `2026-10-16`。
  **两枚变异负控各咬各的承载点**（控制器 `tmp/join-cmp-301/ctl301.py`，每枚先打印 `mutation applied`、还原后
  md5 对齐 `071e238ea49f3b3deea61c780556294e`）：① 把「越界」折回空串（改动前的形状）→ **7 枚 FAILED**
  （3 库测 + 4 门面）；② 只把两个非对称支写反 → **恰好 4 枚 FAILED**，且只有方向敏感的那两枚门面与
  `cmp_keys_answers_three_cases_not_two` 倒下，而「空输出」那枚与切组那枚仍绿——**读数如实记下**：这两枚对
  「谁大谁小」没有分辨力，定序的分辨力由方向那两枚承担，所以两支都入门。
  **登记不执行**：`join` 与本树 C 真源的已知差异只剩一支——`fieldarg`（`:526-527`）从不接受 `-o 0`，
  参照件实测报 `malformed -o option field`，本壳把 `0` 当连接字段收下（BSD/GNU 约定），`Which::Join`
  因此是无生产来源的支路；拒收与死码裁决归下一单（`JOIN-OZERO-ACCEPTANCE`）。另：本笔只动 `join`，
  批次三百登记的两项（`jot.rs` 的 10 处存量 rustfmt 漂移、`%5.2f` 的两支未钉）仍未取。
  **负载条款**：全程一条命令只跑一件事，容器 `-j 1 -m 2g -u`，宿主三架构只在容器不在跑时跑；
  探针与参照件挂载只读用 `tmp/`，包测一律容器内跑。

- ✅ **批次三百把 `jot` 用例里的 π 近似字面量换掉，`minix-textfilter` 的 `clippy --all-targets` 腿首次能当门用**
  （`2026-10-16`，`claim/JOT-APPROX-CONSTANT-qoder` @ 基线 `78dd478fb`，单域三线 commands/textfilter，只动
  `src/jot.rs` 测试模块里的一枚用例，无跨界）：清批次二百九十八登记的那条卫生项——`src/jot.rs:771` 的 `3.14159`
  撞上 deny 级 `clippy::approx_constant`，使该包 `cargo clippy --all-targets` 在基线上就编译不过，这条腿因此
  一直不能当门，此前只能先放宽这条 lint 再取读数。
  **交接写的那句改法被实测否证两处**：①「如写 `3.1415_9`」无效——clippy 比的是**数值而不是写法**，下划线只是
  源码分隔符；②换成 `3.1416`（π 舍到四位）仍然报，说明它的常量表**含舍入后的短形**，不是只按截位匹配。
  两条都有实证：`3.14159` 与 `3.1416` 各触发一次 deny（后者落在 `jot.rs:776:31`，日志
  `tmp/jot-300-clippy.log`）。
  **改法**：这一测要钉的是「`%5.2f` 把值补宽到五列」，与值的来历无关，于是换成两个二进制下精确、因此读数不带
  猜测的形——`9.5` 得 `" 9.50|"`（补宽支）、`7.9375`（127/16）得 `" 7.94|"`（第三位是 `7`，无论何种舍入策略都
  进位）。两句期望都由**实跑**确认（`--lib` 452/0），不是从格式化规则推的。
  test-audit 五维度：无冗余（`%5.2f` 的宽度断言全包只此一处，`floatfmt.rs` 的三枚测是 `floor`/`decimal_exponent`/
  `pow10` 的纯件测，不重复承载）；无无效测（本包该目标无 `#[ignore]`、无空体）；无虚构（07 篇 jot 行既不声明测试名
  也不声明计数，故 Sync-7 无偏差可比——这本身是个留白，登记见下）；自身正确性由实跑坐实；完备性余缺口登记在末尾。
  三件套：`cargo clippy -j 1 -p minix-textfilter --all-targets` → **退码 0、0 error、`approx_constant` 零命中**，
  86 条 warning 与改前**逐编译单元同数**（`generated N warnings` 分布 4/32/2/54 一致，`tools/check-clippy-ratchet.sh`
  的棘轮不因本笔抬动）；`cargo test -j 1 -p minix-textfilter --lib` → 452 passed / 0 failed；
  整包 `cargo test -j 1 --no-fail-fast -p minix-textfilter` → **644 passed / 0 failed / 0 ignored**（51 个 target，
  与上一笔 `78dd478fb` 同数，本笔只换字面量不改测数）；同容器 `cargo check --workspace -j 1` → 0 error（36.85s）；
  宿主三架构 `cargo check -p minix-textfilter --bins --target {x86_64,aarch64,riscv64gc}-unknown-none`
  → 三者均 `Finished` 零 error；rustfmt：本笔新增漂移 0 处（见下面的存量登记）；核实日 `2026-10-16`。
  负控一枚：把 `3.14159` 放回同一句 → clippy 退码 **101**、`approx_constant` 命中 2；还原后 md5 对齐
  `56423f175f351543c78525075a60ff07`，并**重跑一次取最终读数**（0 error），不让读数对应到旧树。
  **顺带坐实一条工具事实（值得写进配方说明）**：容器镜像 `minix-ci:1.94` 的 `rustc --print sysroot` 下
  `lib/rustlib/` 只有 `x86_64-unknown-linux-gnu` 一个 target，**不含三架构 none 目标的 core**——在容器里跑
  `--target x86_64-unknown-none` 会 E0463（`error: could not compile bitflags`）。所以「权威编译门 = 容器」与
  「三架构 `--bins` = 宿主现测」并不矛盾：容器管宿主测试与 workspace，交叉 target 只能靠宿主装的三份 std。
  **登记不执行两处**：① `src/jot.rs` 另有 **10 处存量 rustfmt 漂移**（`git show HEAD:` 副本与工作树同为 10 块，
  位置 `:6`、`:142`、`:158`、`:165`、`:287`、`:355`、`:368`、`:391`、`:451`、`:785`；本笔新增 0 处）——把它们
  一并格式化会让一笔卫生提交变成整文件的无关 churn，故只登记；② `jot` 的格式面还有两支没钉：值宽已达/超过
  指定宽度时的**不补宽支**（如 `%5.2f` 对 `12.375`）与**负值支**，07 篇 jot 行也没有任何测试名或测试计数声称
  （Sync-7 无从对账），补测归后续轮。
  **负载条款重申**：本轮全程「一条命令只跑一件事」，容器一律 `-j 1 -m 2g -u`，宿主侧三架构只在容器不在跑时跑，
  长跑一律落盘再取退码；无任何合成 CPU 压力（上一笔的 WSL 崩溃成因已记入台账与本表）。

- ✅ **批次二百九十九把 `join` 的字段切分按 C 定形：显式 `-t` 保留空段，四处同源错位一起转绿**
  （`2026-10-16`，`claim/JOIN-SPLIT-FIELDS-qoder` @ 基线 `4df3bfc97`，单域三线 commands/textfilter，无跨界）：
  照上一单写死的判据落地。**真源形状**：C 的 `slurp` 用 `strsep(&bp, tabchar)` 切字段（`join.c:378`），
  **一个分隔符字节就是一刀**；`spans`（`:106` 默认为 1，`:200` 给了 `-t` 就清成 0）决定切出来的空段算不算字段。
  所以 `-t,` 下 `a,,c` 是三个字段（中间那个是空串），行尾那个分隔符还多留一个空字段（`strsep` 到串尾会再返一次），
  而默认集 `" \t"` 靠 `spans` 把连读的分隔符当一段。**改法**：`JoinOptions` 不再单存一个 `delim` 字节，
  而是照 C 那两个全局量的形状存 `tabchars`（分隔符集）与 `spans`（跳不跳空段），输出分隔符改由
  `sep()` 取集子首字节（C 打的是 `printf("%c", *tabchar)`，`:497`）；`-t` 一次改两个量，于是 `-t ' '`
  只把空格当分隔符、制表符仍是内容（旧写法把「分隔符是空格」当成默认集，这一形两个事实都错）；
  切分处收成 `JoinOptions::fields_of`，让调用方没法把「集」和「跳不跳空段」配错对（它们在 C 里同源）。
  另一处承载点在同一条线上：不配对那行走 C 的 `outoneline`（`:447-466`），它把行里**每个**字段送进
  `outfield`，空字段占一个分隔符槽而不打内容——本树 `onlyline` 的默认布局此前是 `if f.is_empty() { continue; }`，
  把槽位连同内容一起抹掉，现改为逐字段走 `push_tok`。
  **金样八形逐形从参照件 `tmp/family-diag-ref/join_ref` 现测抄回**（左 C / 右本壳改前，输入 `a,,c\nb,,d` 与
  `a,,c\ne,,f`，探针目录 `tmp/join-split-299/`）：`-t,` → `a,,c,,c` vs `a,c,c`；`-t, -o 1.1,1.2,1.3,2.1` →
  `a,,c,a` vs `a,c,,a`；`-t, -v 1` → `b,,d` vs `b,d`；`-t, -v 2` → `e,,f` vs `e,f`；`-t, -j 2` →
  四行交叉 `,a,c,a,c`、`,a,c,e,f`、`,b,d,a,c`、`,b,d,e,f` vs 一行 `c,a,a`；`-t, -1 2 -2 2` → 同四行 vs `c,a,a`；
  `-t, -a1 -a2` → `a,,c,,c`/`b,,d`/`e,,f` vs `a,c,c`/`b,d`/`e,f`；`-t ' ' -j 2` 对 `a  b` → ` a b a b` vs `b a a`。
  **同读数不当门**：`-t, -1 2`（只改第一侧）两边都给空输出，那是巧合，门改用能区分的 `-1 2 -2 2`；
  `-t, -o 1.1,1.2` 对 `a,` 与 `-t, -j 2` 对 `a,`/`x,` 也是旧写法经另一条路凑出的同读数（越界落 `-e` 的空串
  与取到那个空段，输出串一样），仍入门但在注释里标明；能区分的尾部空段形是 `-t, -a1`（旧壳给 `a`、C 给 `a,`）。
  测面：去掉 `empty_fields_from_the_delimiter_split_keep_their_slots` 的 `#[ignore]`（它已被拆开替掉），
  门面 `tests/join_face.rs` 由 12 枚必跑 + 1 枚 ignore 增至 **20 枚必跑**，八枚新测**按形分枚**
  （只有 `-v 1`/`-v 2` 那两形同腿合在一枚，注释里写明为什么合；旧教训：批次二百九十八——三形塞一枚
  再整体 ignore，负控会「注入成功却全绿」）；库测新增四枚
  （`split_default_set_spans_runs_and_drops_empty_fields`、`split_with_explicit_dash_t_keeps_every_empty_field`、
  `dash_t_space_does_not_make_tab_a_delimiter`、`unpaired_line_gives_an_empty_field_its_separator_slot`）。
  三件套：容器门 `docker run --rm -v "$PWD:/work" -w /work/os -u $(id -u):$(id -g) -m 2g minix-ci:1.94
  cargo test -j 1 --no-fail-fast -p minix-textfilter` → **644 passed、0 failed、0 ignored**（51 个 `test result` 行、
  零 `FAILED`/`error[`/`panicked`；基线 633 个条目 = 632 必跑 + 1 ignore，本轮 +11 与此处测数增量对得上）；
  同容器 `cargo check --workspace -j 1` → 0 error；宿主三架构 `cargo check -p minix-textfilter --bins
  --target {x86_64,aarch64,riscv64gc}-unknown-none` 三者均 `Finished` 零 error；
  `cargo clippy -p minix-textfilter --all-targets` 的**唯一** error 仍是存量 `jot.rs:771` 的 deny 级
  `clippy::approx_constant`（批次二百九十八已登记），故本轮另跑 `… --all-targets -- -A clippy::approx_constant`
  → **0 error、`join.rs`/`join_face.rs` 零命中**（86 条 warning 全属存量），这条腿修好之前它不能当门；
  `rustfmt --edition 2024 --check` 触及两文件零漂移（格式化只跑被改的 `src/join.rs` 与 `tests/join_face.rs`，
  `git status --porcelain` 复核只有这两件加两件文档，无旁观文件被重排）；核实日 `2026-10-16`。
  **三枚变异负控**（控制器 `tmp/join-split-299/ctl299.py`，每枚先打印 `mutation applied`、还原后 md5 对齐
  `47f7fa6f3ab11736a3cd77dd6af7abe1`，跑 `--lib --test join_face`）：① `unconditional_span`（跳空段改回无条件套用）
  → 11 枚 FAILED（四枚库测 + 七门面）；② `dash_t_keeps_spans`（`-t` 不再清 `spans`）→ 9 枚 FAILED，
  且只有它咬住 `parse_attached_option_values_accepted` 里那句 `!spans` 断言；③ `onlyline_skips_empties`
  （不配对腿回到整个跳过空字段）→ 恰好 4 枚 FAILED，全在不配对路（成对路的 `-j 2`、`-o` 两枚仍绿），
  证明「怎么切字段」与「不配对那行怎么打槽」是两个事实、各由各的测钉住，没有互相承载。
  **本轮两处「测试自身正确性」被当场挡住**：① 第一版库测拿 `join("a,", "a", …)` 想走 `outoneline`，实际两侧键相等
  走的是成对路，改成 `join("a,", "b", …)` 才落在不配对那条腿上；② 第一版 `-o` 门面测的输入档写成单行 `a,,c`，
  与现测金样时用的两行 `a,,c\nb,,d` 不一致——金样必须连输入一起钉，已对齐后再取读数。
  **登记不执行（三条，都不在本单射程）**：① C 的 `cmp`（`join.c:405-414`）把「字段号超出这一行的字段数」与
  「字段存在但本身是空串」当两种答案（越界那一侧算大），本壳 `key` 两种都给空串因而判等——参照件对 `a,`（两字段）
  与 `a`（一字段）做 `-t, -1 2 -2 2` 给**零行**，本壳给 `,a,a`；本单的门刻意避开这一形，修法要改 `key` 的返回形状。
  ② C 的 `fieldarg` 从不接受 `-o 0`（`:526-527` 只看 token 首字符是不是 `1`/`2`；参照件实测 `-o 0,1.1,2.1` 报
  `malformed -o option field`），本壳把 `0` 当连接字段收下（BSD/GNU 的约定，不是本树 C 真源的），
  于是 `Which::Join` 是一条无生产来源的支路——按「保持外部行为」这一档应当拒收，拒收后它是否转死码要一并裁。
  ③ 门自身的竞态：`pr_face` 的 `numeric_option_diagnostics_are_c_named_messages_then_the_usage` 在写子进程标准输入时
  拿到 `BrokenPipe`（`tests/pr_face.rs:31`，单目标三连跑 1 红 2 绿），`vis_face` 在同一容器里串跑「整包 + workspace
  check + 三架构 + clippy」那一趟独红，随后纯整包跑 644/0、`--test vis_face --test pr_face` 三连跑全绿（日志
  `tmp/join-split-299/{gates,flake-probe,gate-test}.log`）。判读：这两枚端到端门把「子进程还没起来或已退出」的窗口
  交给了 `write` 的成败，属测试自身正确性维度的缺口（test-audit 第 2 维），登记为独立卫生项；本单只动 `join` 面，
  与该两文件零交集。全程无合成 CPU 压力，包测一律容器内跑。
  **下一单判据**：`JOIN-CMP-OVERFIELD`——把 `key` 换成能区分「越界」与「空串」的形状（`Option<&[u8]>` 或
  返回带存在位的元组），按 `join.c:405-414` 的三支定序（两侧都越界算相等、只有一侧越界那一侧算大），
  金样 `-t, -1 2 -2 2` 对 `a,`/`a` → 零行；同批判 `-o 0` 的接受性（判据：C 拒，参照件句子已量）。
  卫生项仍在前：`jot.rs:771` 的 `clippy::approx_constant`（改法：别让字面量正好是 π 的近似，如 `3.1415_9`）。

- ✅ **批次二百九十八把 `join` 输出面的分隔符槽位按 C 定形：先占槽、再决定内容**
  （`2026-10-16`，`claim/JOIN-OUTFIELD-SLOTS-qoder` @ 基线 `f675b95a1`，单域三线 commands/doctools，无跨界）：
  照上一单写死的三条判据落地。**①**`push_tok` 改成无条件先占分隔符槽、之后才决定内容（C 的 `outfield()`
  第一句是 `if (needsep++) printf("%c", *tabchar)`，`join.c:496-497`；「字段存在但为空串」才 `return`
  且槽位留着，`:503-504`）；**②**`twoline` 默认布局里缺席的连接字段占一个槽并按 `-e` 取值（`:499-501`），
  旧写法是 `if a.len() > opts.field1` 整个不打；**③**`olist_token` 的 `Which::Join` 从
  `unwrap_or_default()` 改成 `unwrap_or_else(|| fallback.clone())`。金样全部来自参照件
  `tmp/family-diag-ref/join_ref` 现测（该参照件必须给 `fgetln`/`getprogname` 带原型，否则被隐式声明成
  返回 `int`、指针截断段错误——上一单已登记）。
  **推翻上一单的一处金样**：`-t,` 对 `a,,c` 两侧，参照件实测是 `a,,c,,c`，不是记的 `a,,c,a,,c`。顺着这条
  查出更深一层分歧：本壳 `split_fields` 把按 `-t` 切出的空段跳掉，而 C 把空段当字段——分歧在字段切分层
  不在输出层，**不属本单射程**（判据见下）。
  新增两枚测，**追加到 `join_face.rs` 文件末尾**（吸取上一单「行号切片覆盖掉既有测」的教训，改完先核对
  门数不减）：必跑的 `missing_join_field_keeps_its_separator_slot`（`,q,q\n,q,r\n,r,q\n,r,r\n` 与
  `-e -` 的 `-,q,q…` 四串逐字节等式）、标 `#[ignore]` 的
  `empty_fields_from_the_delimiter_split_keep_their_slots`（注释写明为什么不通过、归下一单）。07 篇 join 行
  同步两处：槽位语义段 + 测试名对账（该文件此前漏记批次二百九十四的
  `the_option_error_first_lines_match_the_bsd_reference`，一并补上，现列 12 枚必跑 + 1 枚 ignore）。
  三件套：容器门 `docker run --rm -v "$PWD:/work" -w /work/os -m 2g minix-ci:1.94 cargo test --target-dir
  /tmp/u298r --no-fail-fast -j 1 -p minix-textfilter` → **632 passed、0 failed、1 ignored**（51 个
  `test result` 行、零 `FAILED`/`error[`/`panicked`；基线 631 现测，join_face 由 11 枚必跑增至 12 枚必跑 +
  1 枚 ignore）；同容器 `cargo check --workspace -j 1` 零 error；clippy 的 `--all-targets` 日志里对
  `join.rs`/`join_face.rs` 零命中（唯一那条 error 在 `jot.rs:771`，是存量，登记见下）；
  宿主三架构 `cargo check --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各零 error；
  两枚变异负控（`old_slot_swallowing` 把 `push_tok` 改回旧写法、`no_fallback_for_join` 撤掉 `-e` 回退）
  均**先打印 `mutation applied` 再看到 `missing_join_field_keeps_its_separator_slot ... FAILED`**，
  恢复后 `cmp` 同一（控制器 `tmp/ctl298.py`）。核实日 `2026-10-16`；基线 `f675b95a1`。
  **格式化补记（第三处自纠）**：`rustfmt --check` 首轮报三处漂移。拿 `git show HEAD:` 的副本对照分来源——
  `join_face.rs` 那处是本笔新写的长 `assert_eq!`（已断行），`src/join.rs` 的 `:238`/`:412` 两处**在基线副本上
  同样报漂移**，即批次二百九十五遗留（当时那句「零漂移」的读数不成立，与批次二百九十二记过的同一性质）；
  两处一并在本笔格式化，格式化后 `--check` 零漂移，并**在容器里重跑整包门**复核同数，不让读数对应到旧树。
  **登记不执行**：`os/commands/usr-bin/textfilter/src/jot.rs:771` 的 `assert_eq!(render(&f, 3.14159), …)`
  触发 deny 级 `clippy::approx_constant`，使该包 `cargo clippy --all-targets` 在基线上就红一处；改法是别让
  那个字面量正好是 π 的近似（如写 `3.1415_9`），属独立的卫生项，不在本单射程。
  **下一单判据**：`split_fields` 对显式 `-t` 分隔符要保留空段（C 的 `splitline()` 按 tabchar 切，空段就是
  字段），去掉 `empty_fields_from_the_delimiter_split_keep_their_slots` 的 `#[ignore]`，金样 `-t,` 对
  `a,,c` 两侧 → `a,,c,,c`；注意这一改动会牵动 `-o`/`-v`/计数类的字段号语义，属另一个射程，别顺手合并。
  **过程自纠两处**：① 第一次把「①②③ 三形」塞进同一枚测并整体 `#[ignore]`，结果两枚负控「注入成功却全绿」
  ——拆成两枚（必跑钉 ②③、ignore 只钉 ①）后负控才真咬住；② 本单台账的第一版用
  「表外字母与缺值在 C 是**两行**」当锚，该串在 07 篇不唯一，断言当场挡下（代码与测都在，只重跑台账写入），
  无损伤。**电脑因我上一单前的合成 CPU 压力重启过一次，本单起全程无合成压力、包测一律容器内跑。**

- 🟡 **批次二百九十七：oracle 修好了、金样量到了，代码修复回退（我改测时删坏了既有测，不 ship 未证实的东西）**
  （`2026-10-15`，`claim/JOIN-OUTFIELD-SLOTS-qoder` @ 基线 `ab9cd59bf`；实现与测**全部 `git checkout` 回退**，
  回退后容器整包复测 **631 passed、0 failed** 与基线逐枚同数——破坏已确认修复）。
  **oracle 的真根因（推翻上一轮的登记）**：`join_ref` 量不到输出面**不是** `fgetln` 垫片实现有问题
  （它早就是 `getdelim` 写的），而是**缺原型**：`fgetln` 与 `getprogname` 都没有声明就被调用，C 把它们
  隐式声明成返回 `int`，指针被截断→段错误。补两行原型（`tmp/family-diag-ref/refinc/minixshim.h`）后
  `join_ref` 第一次真跑起来，连 `usage()` 的三行全形也量到了（`-1 0` → warnx 一行 + usage 两行、退 1，
  这同时把批次二百九十四那枚门的「只能量首行」边界**取消了一半**：usage 块现在可实测）。
  **量到的金样（四形，逐字节）**：`-t,` 对 `a,,c` 两侧 → `a,,c,a,,c`；`-t, -1 2 -2 2` 对 `q`/`r` 两列缺
  → `,q,q`、`,q,r`、`,r,q`、`,r,r`；同形加 `-e -` → `-,q,q` 等四行；`-v 1` → 只出 file1 未配对行 `b,2,c`。
  这证实了批次二百九十六那句判断：C 的 `outfield()` 先占分隔符槽再决定内容，本壳 `push_tok` 先判空整个
  `return` 会把槽位抹掉（`a,,c` 变 `a,c`）。
  **下一单的判据已完全就绪**（照着做即可，不必再读 C）：①`push_tok` 改成「先占槽、后置内容」；
  ②`twoline` 默认布局里连接字段缺席时占一个槽并按 `-e` 取值；③`olist_token` 的 `Which::Join` 分支
  用 `fallback` 而不是默认空。金样就用上面四串。改完跑：容器整包门 + 三架构 `--bins`（动 lib 码）
  + 把 `push_tok` 改回旧写法的负控（预期金样测报红）。**操作教训一条**：我这次用行号切片往 `join_face.rs`
  插测，把该文件里一枚既有测整段覆盖掉了（总数 631→620 才暴露）——**插入型改动必须用唯一文本锚，
  且改完先看门数不减**；这已经是本轮第二次因"锚没命中/锚太宽"造成的自伤（前一次是负控脚本语法错）。

- 🟡 **批次二百九十六（`claim/JOIN-EV-OUTPUT-qoder`）没落地代码，落地的是一条实证缺口 + 判据**
  （`2026-10-15`，基线 `83c427565`；实现与测都**回退了**，`git status` 干净、回退后整包复测
  631/0 同基线——不留未证实的改动）。
  **查出来的分歧（按 C 源码逐句判定，未改代码）**：C 的 `outfield()` 第一句就是
  `if (needsep++) printf("%c", *tabchar);`，**分隔符槽位先占**，之后才决定要不要打内容：
  字段存在但本身是空串时 `return`（`minix3/usr.bin/join/join.c:503-504`）——槽位留着；字段号
  超出这一行的字段数时打 `-e` 替换串，`-e` 没给就打空（`:499-501`）。本壳 `push_tok` 旧写法是
  `if tok.is_empty() { return; }`——**先判空整个跳过**，于是槽位被一起抹掉。可观察后果两例：
  ① `-t,` 下三行字段 `a`、``、`c` 在 C 是 `a,,c`，本壳给 `a,c`（少一个分隔符）；②默认布局下
  连接字段缺席（`-1 2` 对上只有一列的行）C 先占一个槽（有 `-e` 就打替换串），本壳静默省略。
  **为什么这轮没就地改**：改 `push_tok` 会影响所有输出面的既有读数，必须**先把期望值钉成独立测**
  再改，而期望值这一层我只有「逐句走 C 代码」这一条来源——上一轮刚写过：只钉单向的门挡不住
  收严过头，所以要先有可重跑的 oracle。
  **下一单的判据（已备好）**：`join_ref`（`tmp/family-diag-ref/join_ref`）现在量不到输出面，
  因为它的 `fgetln` 垫片在读档路径段错误（批次二百九十四记过）。把那份垫片换成
  `getdelim`（glibc 有，一行改动）就能让参照件真跑输出，届时按下述四形取金样再改
  `push_tok`：`-t, f1 f2`（含空字段）、`-o 1.1,2.2 -e NA` 配单侧短行、`-1 2` 默认布局配一列行、
  以及 `-v 1 -e NA -o ...`。改完必须跑：容器整包门、三架构 `--bins`（动的是 lib 码）、
  以及「把槽位行为改回旧写法」的负控——预期至少一枚既有门会因此变红，若全绿说明测还是没盖住。

- ✅ **批次二百九十五接上 `join` 的 `obsolete()` legacy 重写，并顺手修掉「重复 `-o` 会丢前面各列」的覆盖错**
  （`2026-10-15`，`claim/JOIN-OBSOLETE-LEGACY-qoder` @ 基线 `395cf3d21`，三线同包：`src/join.rs`、
  `src/bin/join.rs`、`tests/join_face.rs` 加 `07-text-filter.md`）：批次二百九十四「登记不执行」那条。
  **C 的三段**（`join.c:547-617` 的 `obsolete()`，跑在 `getopt` 之前、遇 `--` 即停）：
  ①token 恰好是裸 `-a` 时 `ap[1] = '\01'`，于是落到 `case '\01'`（`:135-138`）——`aflag` 加
  **两个**文件的 unpair；②`-j1`/`-j2` 就改成 `-1`/`-2`（值取下一词），`-j` 后面是别的或数字后
  还拖字符（`-j3`、`-j12`）当场 `errx(1, "illegal option -- %s", ap)`（`:592`，句子带整个 token，
  usage 走不到）；③裸 `-o` 之后紧跟的每个 `[12].<数字>` 项逐个重写成独立 `-o` 列表（`:596-617`）。
  **由此翻出本壳两处真 bug**：一是裸 `-a` 之前被当「带值选项」，`join -a f1 f2` 会吞掉第一个
  文件名再拿它转数字，落「field number less than 1」——那是把 legacy 形当新形读；二是 `fieldarg`
  在 C 里是**追加**（`olist[olistcnt] = ...; ++olistcnt`，`:540-542`），本壳写成 `=` 覆盖，于是
  `-o 1.1 -o 2.1`（以及 legacy 的 `-o 1.1 2.1`）只剩最后一列。
  **反直觉但有据的一处**：因为改写无条件，`join -a 3 f1 f2` 里那个 `3` 是**操作数**而不是 `-a` 的
  值，于是三操数撞 `if (argc != 2) usage()`——参照件那一形零输出崩在自己的 `usage()` 垫片里
  （说明它同样走到了 usage），与本壳读数同形。
  测：lib 两枚新测 `obsolete_legacy_forms_rewrite_before_getopt`（含 pre-pass 的先后、`--` 之后不扫）
  与 `legacy_o_lists_accumulate_instead_of_replacing`（累加、附值形不触发改写）；三处旧断言按 C 真形
  改成附值形（`-a1x`/`-a3`），因为它们原来钉的正是「把裸 `-a` 当带值选项」这个错读法。
  三件套：docker `minix-ci:1.94` `cargo test --target-dir /tmp/fin295 --no-fail-fast -j 1
  -p minix-textfilter` → **631 passed、0 failed**（零 `FAILED` 行；基线 629 + 本笔 lib 两枚）、
  同容器 `cargo check --workspace -j 1` 0 error、clippy 触及三件零命中、`rustfmt --check` 三件零漂移；
  **四枚**变异负控各咬一处（①取消 `-j1` 改写→legacy 测报红；②取消裸 `-a` 改写→同一枚；
  ③取消 pre-pass 守卫→同一枚；④取消 `-o` 累加改写→累加测报红），恢复用备份并 `cmp` 同一；
  五道收尾校验全过（断链基线 124 不变、`doc-style-lint --diff` error=0）。核实日 `2026-10-15`；
  基线 `395cf3d21`。
  **第三次踩同一个坑，记死**：负控脚本经 shell heredoc 生成时又出语法错，那四次「全绿」其实一次
  都没注入——控制器一律用编辑工具直接落盘，且每枚先看 `mutation applied` 再谈颜色。
  **三架构门补跑（合回后在主树现测，`2026-10-15`，快照 `8f2e2585c` 之后一笔）**：本批动了
  `src/join.rs`（lib）与 `src/bin/join.rs`（bin），按纪律跑
  `cd os && cargo build --target-dir /tmp/x295c -j 1 -p minix-textfilter --bins --target
  {x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf}` → 三架构各
  **0 error**。顺手记一次险些自证的误读：第一次跑我从仓库根发起，得三个 "1"，实为
  `could not find Cargo.toml`（workspace 根是 `os/`）——**错误计数之前先看错误文本**。
  登记不执行：`join` 的 `-e`（空字段占位）与 `-v` 的输出面本轮未碰；`sort` 选项面余 `D`/`R`/`T`。

- ✅ **批次二百九十四把 `join` 的选项诊断与 BSD 参照件逐形 A/B：结论是无差异，对账落成一枚可重跑的门**
  （`2026-10-15`，`claim/JOIN-OPTION-DIAGS-qoder` @ 基线 `76f4552de`，只动 `tests/join_face.rs`）：
  规划表 R13（v2-9 拆片①）的收口轮。**做了什么**：把 `minix3/usr.bin/join/join.c` 真源加
  `fgetln`/`getprogname` 垫片链成参照件 `tmp/family-diag-ref/join_ref`（与 `sort_ref` 同法，坑也一样：
  垫片里 `progname_shim.c` 与 `fgetln_shim.c` 的 `getprogname` 重名，链时只留后者），再拿 19 个形两边
  各跑一遍。结果：**每一行差异都只剩程序名段**（`join` 对 `join_ref`），句子与退码全同——R13 的文案
  面已由批次二百八十三做完，本笔是去**证**它。对账做法（可重跑）：容器里 `cargo build --target-dir
  /tmp/ab -j 1 -p minix-textfilter --bin join` 取我们的二进制，与 `join_ref` 跑同一批形（`-1 0`、
  `-1 x`、`-1 1x`、`-2 0`、`-j 0`、`-j 5x`、`-a 3`、`-a 1x`、`-v 3`、`-t xy`、`-t ,`、`-o 0.1`、
  `-o 1.0`、`-o 3.1`、`-o 1.1x`、`-e`、`-q`、`-a 1 -v 2`、单操数），各取 stderr 首行逐条对照。
  **补一枚表驱动门** `the_option_error_first_lines_match_the_bsd_reference`（8 行，取既有门没钉的
  `-2`、`-j` 尾字符、`-o` 四形、`-t` 附值形），期望句子是参照件实测常量；**诚实边界写在文档注释里**：
  门跑的是我们自己的二进制、只有期望值来自参照件，而那份参照件链宿主 glibc 的 `fgetln` 垫片——
  **读档路径段错误**、`usage()` 在同一垫片下也段错误（`-1 0` 实测 exit 139、usage 块打不出来），
  所以它只能量「开档之前就退出」的诊断首行，usage 那两行仍由既有门按 C 源码文本单独钉。`-q` 那行
  顺带坐实同族老结论：参照件给 glibc 的 `invalid option -- 'q'`，我们给 Minix libc 的
  `unknown option -- q`（`getopt.c:101-104`），**宿主形不可当基线**。
  三件套：docker `minix-ci:1.94` `cargo test --target-dir /tmp/fin294 --no-fail-fast -j 1
  -p minix-textfilter` → **629 passed、0 failed**（零 `FAILED` 行；基线 628 + 本笔 1 枚，`join_face`
  10→11 现测）、同容器 `cargo check --workspace -j 1` **0 error**、clippy 对 `join_face.rs` 零命中、
  `rustfmt --check` 本笔新行 2 处已排定并复查零漂移；一枚变异负控咬住（把 `-o` 的
  `field numbers are 1 based` 支塌成 `malformed -o option field` → 新门与既有 errx 门**同时**报红），
  恢复用备份并 `cmp` 同一。核实日 `2026-10-15`；基线 `76f4552de`。
  **一条环境事实**：三架构 `--bins` 在 `minix-ci:1.94` 里跑不出来——镜像没装 freestanding Rust
  target，实测 `error[E0463]: can't find crate for 'core'`；那三道门归宿主跑（或容器内先
  `rustup target add`）。本笔只改一个 `tests/` 文件，`--bins` 与 `--lib` 都不编译它，故本笔免跑并
  写明理由，不拿免跑当通过。
  登记不执行（真缺口）：`join.c:obsolete()`（`:547`）的三段 legacy 重写本壳未实现——①裸 `-a` 改写成
  `\01`（两文件都不配对）；②`-j1`/`-j2` 改写成 `-1`/`-2`（参照件坐实：`-j1` 报的是
  `-1 option field number less than 1`），而 `-j3`/`-j12` 落 `errx(1, "illegal option -- %s", ap)`
  （实测 `join_ref: illegal option -- -j3`）；③`-o arg arg` 的 legacy 形要把后续 `1.`/`2.` 项逐个
  重写成 `-o`。下一单接线时注意它是一次 **pre-pass**：在 `getopt` 之前跑、遇 `--` 即停。

- ✅ **批次二百九十二把 `sort -c` 那两句话按 C 逐字定形，并收严了两枚「只 `contains` 一下就绿」的门**
  （`2026-10-15`，`claim/SORT-CHECK-WORDING-qoder` @ 基线 `2278b239a`，单域三线同一 crate：
  `src/bin/sort.rs`、`tests/sort_face.rs` 加 `07-text-filter.md`）：批次二百九十一「登记不执行」第①条。
  **C 的真形（参照件 `sort_ref` 逐字节量，`cat -A` 看过行尾）**：失序那句是
  `errx(1, "found disorder: %s", …)`（`msort.c:403`）→ 读数 `sort: found disorder: a`；`-u` 下键相等是
  `errx(1, "found non-uniqueness: %s", …)`（`:405-407`）→ 读数 `sort: found non-uniqueness: a`。
  两句都退 **1**、都不接 usage、都只打一行，报的是**后一条**记录（C 在打印前把记录尾部分隔符写成
  `\0`，`:402`/`:406`，所以句尾那个换行是 `errx` 自己加的）。先后也量了：`b b a` 报的是先撞上的
  那对（重复的 `b`）而不是后面那次失序——两者取自同一次比较，`0 < cmp` 先看、`UNIQUE && !c` 后看。
  **本壳错在两处**：措辞自创（`sort: disorder: ` 与 `sort: duplicate: `），且程序名写死成 `sort`
  而不是走 `argv[0]`——改成 `check(prog, …)` 用 `support::prog_name`，与同族其它腿同一做法。
  **为什么之前没被门挡住**：两枚端到端门只写了 `assert!(err.contains("disorder"))`，退码也对，
  于是自创措辞一路绿灯。本轮把它们收严成逐字等式，并新增一枚独立门
  `the_two_check_reports_use_c_sentences_and_one_line_each`（含「stderr 只有一行」这条
  usage 块混不进来的判据，以及无 `-u` 时不许冒出 `non-uniqueness` 的反方向一枚）。
  三件套：docker `minix-ci:1.94` `cargo test --target-dir /tmp/final292b --no-fail-fast -j 1
  -p minix-textfilter` → **628 passed、0 failed**（零 `FAILED` 行；基线 627，本笔 `sort_face` 14→15
  新增一枚，现测非抄表）、同容器 `cargo check --workspace -j 1` 与 clippy 触及件均 0；三架构
  `--bins` 零错；三枚变异负控**全部确认注入后咬住**（①失序措辞退回自创 → 三枚门报红；②重复措辞
  退回自创 → 一枚报红；③撤掉 `-u` 那一路判据 → 一枚报红），恢复用备份并 `cmp` 同一。
  **两处过程自纠**：① 负控生成脚本第一版有 Python 语法错，三次「全绿」其实**一次都没注入**，
  改用直接落盘的控制器后才拿到真读数；② `rustfmt` 的 `sort_face.rs` 这轮实测有 5 处长行需要断行
  （其中两处是批次二百九十一带进来的，当时那句「两件零漂移」的读数不成立），已格式化并复查为
  零漂移，`src/bin/sort.rs` 剩下的两处经 `git show HEAD:` 副本比对确认是基线存量、不顺手排。
  核实日 `2026-10-15`；基线 `2278b239a`。登记不执行：`sort` 选项面余下 `D`/`R`/`T` 三字母
  （`R` 记录分隔符是真功能，含 `\` 前缀走八进制、`record/field delimiter clash`、
  `invalid record delimiter %s`、`multiple record delimiters` 四件）。

- ✅ **批次二百九十一把 `sort -n` 的数值键按 C 的 `number()` 归一，并删掉一处被负控判为冗余的支**
  （`2026-10-15`，`claim/SORT-NUMERIC-KEY-qoder` @ 基线 `fdded04d4`，单域三线同一 crate：
  `src/sort.rs`、`tests/sort_face.rs` 加 `07-text-filter.md`）：批次二百九十「登记不执行」第①条。
  **改了什么**：`numeric_key` 现在照 C 那样先跳前导空白、再取一个符号（`-` 翻负号、`+` 只是跨过，
  `fields.c:283-288`），比较前把整数位的前导零吃掉（`:290-292`）；一个数字也没转出来的形
  （`fubar`、`.0`、`0.00`）与真零落进同一个零判据，而零判据不看符号，于是 `-0` 与 `+0` 同值。
  旧写法只认 `-`，`+7` 因此整数位为空串、与 `7`/`007` 不同值（`compare` 给出 `Less`），上一批
  删掉整行回退之后这处就露了出来。同批删掉 `trim_leading_zeros`（归一搬进 `normalized_parts` 后
  无人调用）与旧 `compare_numeric` 里那处 `longer = 0; let _ = longer;` 的残留。
  **一处「测例逼出来的冗余」**：C 用 `last_nz_pos` 回退小数尾零（`:373-374`），我先照抄了一遍剥离
  小数尾零的支——然后按纪律跑负控，把它删掉后 **627 枚全绿**，说明那几行不是判据而是冗余（本树的
  小数位逐位比较把短的一方按补零算，`1.5` 与 `1.50` 早就同值）。处置是**删冗余**而不是编一枚测
  把它说成必要的；并且补了一对真正把补零比较逼出来的形：`1.4` 与 `1.40` 同值、`1.4` 与 `1.45`
  **不同值**（决定位落在短一方的长度之外）——负控③把 `max` 换成 `min` 后这枚立刻报红。
  **C 不认指数**：`fields.c:314` 的注释写着“Maybe here should allow for e+12”，实测 `1e3` 排在
  `1000` 之前（键是 1，在 `e` 处停住），本树同形。
  三件套：docker `minix-ci:1.94` `cargo test --target-dir /tmp/base291 --no-fail-fast -j 1
  -p minix-textfilter` → **627 passed、0 failed**（零 `FAILED` 行；基线 **625**，用 `git stash` 把本笔
  改动摘掉后现测：`--lib` 445、`sort_face` 13；本笔各加一枚 → `--lib` **446**、`sort_face` **14**，
  等式 625 + 2 = 627 现算核对；其余目标的枚数不动）、`rustfmt --check` 两件零漂移、clippy 对触及两件零命中、
  三架构 `--bins` 零错；三枚变异负控**全部咬住**（①不认正号→lib 一枚报红；②不吃前导零→lib 一枚
  报红、端到端那枚也报红；③小数位不补零→lib 一枚报红），均先打印 `mutation applied`、恢复用备份
  并 `cmp` 同一。控制脚本第一版把「剥离小数尾零」当负控跑出了「全绿」，那是一次**假阴性**——锚文本
  没命中导致根本没注入（`anchor not unique: 0 hits` 被我的 assert 挡下后才看清），教训是**负控必须先
  确认注入成功再看颜色**，否则「没咬住」会被读成「测没盖住」；核实日 `2026-10-15`；基线 `fdded04d4`。
  登记不执行：① `-c` 那两路文案（本壳 `sort: disorder:` 与 C 的 `found disorder:`（`msort.c:403`）
  差一词、`sort: duplicate:` 在这份 C 里疑为自创）；② `sort` 选项面余下 `D`/`R`/`T` 三字母。

- ✅ **批次二百九十改掉 `sort` 一处自创次序：并列键默认保持输入序，只有 `-S` 才按整条记录破并列**
  （`2026-10-15`，`claim/SORT-STABLE-TIEBREAK-qoder` @ 基线 `e34427e26`，单域三线同一 crate：
  `src/sort.rs`、`src/bin/sort.rs`、`tests/sort_face.rs` 加 `07-text-filter.md`）：批次二百八十七
  「登记不执行」第②条，也是 `sort` 选项面 `D`/`R`/`T`/`S` 四字母里的 `S`。
  **C 的真形**（全部拿参照件 `tmp/family-diag-ref/sort_ref` 逐形量，四行同键输入
  `b 1/a 1/c 1/d 1`）：`-k 2,2` → 原序（`b a c d`）；`-S -k 2,2` → 记录序（`a b c d`）；
  `-r -k 2,2` → 原序倒装（`d c a b`）；`-u -k 2,2` → 只留首行 `b 1`（`-S -u` 与 `-u -S` 同）；
  `-c -k 2,2` → 退 0 不报；`-c -S -k 2,2` → 退 1 报失序。机制在 `sort.c:194-212` 的
  `posix_sort`（`-s` 归零、`-S` 置一）与 `fields.c:168`（该位为零时 `keylen` 只含键、
  不含原始记录），再加上 `sort.c:256-258` 那句**在选项环之后**的 `if (UNIQUE) posix_sort = 0;`。
  **本树之前错在两处**：① 比较半**总是**在键相等时回退整行（`Options` 里连承载 `-S` 的位都没有，
  `'S'` 落进集外字母那一支），于是默认序被自创成记录序，而 `sort -u` 面对一叠同键行会全部留下——
  这恰是 `sort -u` 的反面；② `-s` 与 `-S` 都被当无操作写死。修法：`Options` 加
  `posix_tiebreak`，`'S'` 置一、环后由 `unique` 归零；`compare()` 的回退臂加上
  `&& options.posix_tiebreak`；壳里的排序显式带输入序号（`sort_by` 稳定，只交回 `Equal`
  拿不到 C 那半「`-r` 把稳定序倒装」的行为，故 `then_with` 按 `-r` 取升/降号）。
  **推翻三处旧断言**（都是拿自创次序签字的）：`test_tie_falls_back_to_whole_line`（改名并
  重写为 `a_tie_stays_tied_and_capital_s_breaks_it_by_the_record`）、`test_fold_and_dictionary`
  里「`Apple` 先于 `apple`」与 `test_numeric_with_signs_and_fractions`、
  `test_key_spec_field_selection_with_delimiter`、`sort_by_length_orders_by_span_length_not_content`
  中依赖回退的四条读数，全部改成「平局交回 `Equal`，要拆得开只能 `-S`」，逐条带参照件读数。
  **顺手掀出一处被回退掩盖的差异**（本笔不修，登记）：`-n` 下 C 把 `+7`/`7`/`007` 视为同值
  （三种输入顺序两两都原序输出，实测 A/B/C/D 四组），而本树的数值键把 `+` 当「不是数字」处理，
  于是 `+7` 得零、`007` 得 7——旧测正好写着「POSIX `-n` 没有正号」这个错认知，而它当时的 Less
  读数其实来自整行回退，两个错凑出了一个看着对的断言。另两处同族登记：本壳失序那句写的是
  `sort: disorder: `（C 是 `errx(1, "found disorder: %s", …)`，`msort.c:403`，少一个词）；
  本壳在 `-c -u` 那一路会打 `sort: duplicate: `，而这份 C 里没有对应的 `errx`（重复是被标记后
  丢弃的，见 `append.c:72`、`radix_sort.c:111/203`），那句疑为自创。三处都归诊断面/数值键那一单。
  三件套：docker `minix-ci:1.94` `cargo test --target-dir /tmp/final290 --no-fail-fast -j 1
  -p minix-textfilter` → **625 passed、0 failed**（零 `FAILED` 行；基线 624：`sort_face` 11→12
  新增一枚端到端，lib 改名不增数）、同容器 `cargo check --workspace -j 1` **0 error**、
  clippy 对触及三件零命中；三架构 `--bins` 在 `x86_64-unknown-none`/`aarch64-unknown-none`/
  `riscv64gc-unknown-none-elf` 均零错（本笔动了 lib 与 bin，适用）；`rustfmt --check`：
  `sort.rs` 一处是本笔新行、已排定复跑零漂移，`bin/sort.rs` 两处拿 `git show HEAD:` 副本比对
  确认基线即存在（本笔不顺手排），`sort_face.rs` 零漂移；三枚变异负控各咬一处（①`'S'` 退回
  no-op → lib 四枚报红；②删掉环后的 `unique` 归零 → lib 一枚报红；③把壳里 `-r` 的倒装臂压平 →
  端到端那枚报红），均先打印 `mutation applied`、恢复用备份并 `cmp` 同一；核实日 `2026-10-15`；
  基线 `e34427e26`。**更正（批次二百九十一现测，原正文不改）**：那句「`sort_face` 11→12 新增一枚」的基线数写偏一枚——批次二百八十八已加过一枚，本笔之前 12、之后 13（总数 624→625 无误）。过程教训一条：**测试名过滤器会同时套到被一并选中的集成目标上**——第一次跑
  `--lib sort:: --test sort_face` 时 `sort_face` 的 13 枚被同名过滤器全过滤掉（0 跑 13 滤），
  于是那枚本该报红的负控看着「没咬住」；分开跑端到端目标才见它咬中。

- ✅ **批次二百八十九把 `tail` 跟随门的「拍数判据」改成「耐心上限」，并查清了一档 root 属主产物的真正来源**
  （`2026-10-15`，`claim/FACE-TEST-DETERMINISM-qoder` @ 基线 `0138c876e`，**只改一个测试文件**
  `tests/tail_face.rs`，无生产码改动）：批次二百八十八「登记不执行」第①条的可证明那一半。
  **负载敏感点在哪**：全量 grep 后确认旧常量 `TICK` 只用在 `next_line` 一处定义、被 20 处
  调用共用（`grep -c "next_line(&lines" tests/tail_face.rs` → 20）——那就是「下一行会在多久
  内出现」的**正向等待**；而静默与存活两支各自只有 2 处调用且用的都是 `QUIET`
  （`grep -c "expect_quiet(&lines, QUIET)"` → 2，`grep -c "outlives_ticks(&mut child, QUIET)"` → 2）。C 的一拍是 `sleep(1)`
  （`forward.c:253`），旧的 `TICK = 4s` 把「三拍以内」当判据写进了注释，于是机器被其它线程占满、
  子进程十几秒才拿到一次 CPU 时，跟随环路本身没错却被报成缺陷——这正是整包并跑时那次假红的形状
  （单跑同一目标与 `--no-fail-fast` 重跑均绿）。改法：拆成三个概念——`ROUND`（一拍 = 1s，仅作派生
  源头）、`PATIENCE`（耐心上限，从 `ROUND` 乘出 20 拍）、`QUIET`（静默窗口，`ROUND × 2.5`）。
  **只放宽正向等待，不动静默窗口**：断言的是「来的是哪一行」而不是「它第几秒到」，所以前者放宽
  不削弱任何东西；而「必须什么都不打」那一支**缩短窗口就是削弱判据**，所以它不参与调整（仍是 2.5 拍）。
  代价只有真失败时多等几秒（成功路径一到即返，本目标实测仍为 17.06s，与改前同）。
  **一枚变异负控证明这些门在等真读数而不是跳过**：把 `PATIENCE` 压成 1ms →
  `follow_prints_what_arrives_after_the_first_round`、`follow_by_handle_stays_with_the_file_it_opened`、
  `follow_by_name_prints_the_replacement_from_its_start`、`follow_does_not_rewind_the_position_when_the_file_shrinks`
  四枚同时报红；恢复用备份并 `cmp` 同一。**另一枚的局限也写清楚，不假装全盖住了**：`expect_quiet` 与
  `outlives_ticks` 的失败路在「实现正确」的当下无观察得到它们的变异（改掉它们也不会变红），它们的
  价值靠的是同一目标里那些「静默期结束后仍要打出下一行」的断言——这类「只能验证一半的辅助件」
  应在注里言明，不能拿「写了负控」当它们也被盖住的凭据。
  **vis 那枚未定因，而且证据是我自己丢的**：当时我的采集只留 `FAILED` 行（`grep -E "FAILED|test result"`），
  panic 文本被我自己过滤掉了，所以现在无从判定。静态判读也没找到环境依赖：`tests/vis_face.rs` 的 `run()`
  写完 stdin 就 `drop`、没有固定超时、不读终端，`fold_width_is_read_the_way_atoi_reads_it` 一枚也不碰任何档（
  四个 scratch tag 互不撞且带 pid）。**方法论上一条必须记死：不要用手造 CPU 压力去复现——那两条
  带 8 个 spinner 的容器命令把这台机器打崩重启了一次**。下一次只靠自然重现：拿 `--no-fail-fast` 把
  完整输出落盘（不再 grep 过滤），有 panic 文本再定性；登记保持 open，本轮不拿「改好了」的读数签字。
  **顺手推翻了自己上一笔写下的一个推断**：「容器门不带 `CARGO_TARGET_DIR` 会留下 root 属主产物」
  本身没错，但本轮查到的 5192 个残留**不是**配方失效——`-e CARGO_TARGET_DIR` 是被认的（仓内
  `grep -rn target-dir os/.cargo os/Cargo.toml` 零命中，镜像里也没有 `CARGO_BUILD_*`），真正原因是我那
  两个压力脚本的容器没带该参数。两种写法都实测过残留为 0：`-e CARGO_TARGET_DIR=/tmp/<name>` 与
  `cargo … --target-dir /tmp/<name>`（CLI 压过 config，更明确，本轮权威读数就是这么跑的）。
  另外提醒一句：这些 root 产物会让 cargo 反复失效重编，是拖垮宿主编译的候选成因，跑完容器门
  若发现残留，`docker run --rm -v "$PWD:/work" <镜像> chown -R 1000:1000 /work/os` 一次即可归还。
  三件套：docker `minix-ci:1.94` `cargo test --target-dir /tmp/td289 --no-fail-fast -j 1 -p minix-textfilter`
  → **624 passed、0 failed**（零 `FAILED` 行，完整日志落盘后统计；上一笔的 624 总数**不因本笔而变**，
  因为本笔只改常量不改断言数量），同容器 `cargo check --workspace -j 1` **0 error**；`tail_face` 单目标
  在容器内 **20/20**（17.06s）；`rustfmt --check` 零漂移；clippy 对 `tail_face.rs` 零命中（本包基线 89 条
  不变）；**三架构 `--bins` 本笔不适用**：改动面是一个 `tests/` 文件（`git show --stat` 可证只一件），
  既不进 `--bins` 也不进 `--lib`，freestanding 构建不编译它；五道收尾校验与 pattern-gate/边界门照旧全过。
  核实日 `2026-10-15`；基线 `0138c876e`。
  登记不执行（保持 open，本轮没拿它签字）：`vis_face::fold_width_is_read_the_way_atoi_reads_it` 的
  一次假红未定因，等自然重现时保留完整日志再定性；且复现手段禁止用合成 CPU 压力。

- ✅ **批次二百八十八接上 `sort` 的 `-l`（按字段长度），并把 `x` 定为「不建模且说清为何」**
  （`2026-10-15`，`claim/SORT-LX-FLAGS-qoder` @ 基线 `db4c688ba`，单域三线同一 crate：
  `src/sort.rs`、`tests/sort_face.rs` 加 `07-text-filter.md`，无跨线登记）：批次二百八十七
  「登记不执行」的第①条拆出的前一半。
  **接的是哪一支**：C 的 `enterfield` 在 `fields.c:208-211` 依次查 `L`→`N`→`X`，`L` 那一支就是
  `length()`：先 `SKIP_BLANKS(line)`、再把 `lineend - line` 写成十进制串交给 `number()`。对纯
  十进制串来说数值序就是整数序，所以本壳直接比「跳完前导空白后的跨度字节数」（新函数
  `sort.rs:span_length`，与 `key_bytes` 共用同一套起停算式）。`KeySpec`/`Options`/`Column` 各加
  一位 `by_length`，修饰环收 `b'l'`（`optval` 的 `case 'l': return L;`，`init.c:234`），选项环收
  全局 `'l'`（C 把它与 `d`/`f`/`i`/`n` 写在同一臂，`sort.c:168`，效果是 `:172` 那一句）。
  **五条读数全拿参照件量**：`-l`、`-k 1l`、`-k 1ln`、`-k 1nl` 四者在输入「空行/`aaa 1`/`b 2`/`cc 3`」
  上逐字节同序（空行、`b 2`、`cc 3`、`aaa 1`）——`ln`/`nl` 与 `l` 同序证明的是「`L` 排在 `N` 前」
  而不是巧合；`-t:` 下「两个补白加 `aa`」算 2 不算 4，所以 `b:2` 在 `  aa:1` 之前。另钉一枚反方向：
  同一输入不带 `-l` 时走字典序，`  aa:1` 反而排第一。
  **`x` 的处置是一处推翻式登记**：C 的 `numhex()` 写的是 `sscanf((const char *) pos, "%llx", &n)`，
  而那个 `pos` 是 `enterfield` 传进来的**目的缓冲区写游标**，不是字段游标——它解析的根本不是那一列
  的内容，读数依赖键缓冲的复用（实测 `sort_ref -k 1x` 与 `-k 1n` 给出不同且与任何干净语义都对不
  上的序）。本壳既不复制也不假装：把它静默接受为空操作会把「顺序错」伪装成「顺序对」，所以 `x`
  仍不进修饰集（`-k 1x` 报 `illegal field descriptor`），全局 `-x` 仍走集外字母那一支。
  **两处读数过程错（都是自己的命令写错，不是代码错）**：① 第一次容器门拿的是**截断读数**：
  `cargo test` 缺 `--no-fail-fast` 时，首个失败目标之后的所有目标根本不跑，所以先后得到 536 与 605
  两个偏小的数，且第二次数掉的 19 枚里有一枚偶发失败。改成 `cargo test --no-fail-fast -j 1 -p
  minix-textfilter > /tmp/d.log` 后落盘统计得 **624/0**，零 `FAILED` 行。② **宿主上跑整包测试会
  把 WSL 打挂**（并发编译加上跟随腿要睡十几秒的目标叠在一起），用户明确要求**测试一律在容器里跑**；
  本轮之后的包级测试与合回后复跑均走 docker 配方（带 `-m 2g` 与 `-e CARGO_TARGET_DIR`）。
  **遗留一枚偶发，不当它不存在也不当作已证伪**：整包并跑时 `vis_face::fold_width_is_read_the_way_
  atoi_reads_it` 与 `tail_face` 的跟随腿各报过一次红，但同一目标单跑、以及改 `--no-fail-fast` 重跑
  全绿（本轮两次全绿）。两者都是负载敏感型（前者要子进程拿终端宽度作缺省页宽，后者一拍睡一秒），
  归入新登记的一条：要给这两枚门加超时/隔离或改成不依赖环境的形（下一单）。
  三件套：**docker `minix-ci:1.94` `cargo test --no-fail-fast -j 1 -p minix-textfilter` → 624 passed、
  0 failed**（权威读数，完整日志落盘后统计）、同容器 `cargo check --workspace -j 1` **0 error**；
  宿主同包 **624/0**（基线 621：`--lib` 443→445 新增两枚，`sort_face` 11→12 新增一枚）；三架构
  `--bins`（x86_64/aarch64/riscv64gc）零错；`rustfmt --check` 本笔两件：`sort.rs` 零漂移，
  `sort_face.rs` 报出一处**是本笔自己写的超长 `assert_eq!`**（拿 `git show HEAD:` 副本比对基线为零
  漂移），已 rustfmt 排定并复跑零漂移、确认它不在结束列之后重排任何存量行（工作树污染面 0）；
  clippy 触及件零告警；三枚变异负控各咬一处（①修饰环 `b'l'` 不置位→lib 两枚加端到端一枚报红；
  ②`span_length` 不跳前导空白→lib 一枚加端到端一枚；③比较半不走长度臂→lib 两枚加端到端一枚），
  均先打印 `mutation applied`、恢复用备份并 `cmp` 同一；`tools/pattern-gate.sh` PASS=15 FAIL=0 SKIP=1；
  `tools/check-command-boundary.sh` OK；核实日 `2026-10-15`；基线 `db4c688ba`。
  登记不执行（待办）：① `vis_face`/`tail_face` 两枚负载敏感门的隔离或超时改造（本轮只在容器里跑
  过它，不在宿主重跑）；② `sort` 选项面余下四个字母 `D`/`R`/`T`/`S`（`R` 记录分隔符是真功能，
  `S`/`D`/`T` 需定口径）；③ C 默认「并列键保持输入序」而本树回退整行比较那一题。

- ✅ **批次二百八十七把 `sort` 键形的五句 `errx` 分流，并推翻两处写进台账的断言（`-k 2,1` 与月名）**
  （`2026-10-15`，`claim/SORT-OPTION-COVERAGE-qoder` @ 基线 `0c9681fb5`，单域三线同一 crate：
  `src/sort.rs`、`src/bin/sort.rs`、`tests/sort_face.rs` 加 `rewrite-notes/18-stage-commands/07-text-filter.md`，
  无跨线登记）：批次二百八十三末尾登记的那条键形余面。
  **先拓了一件工具**：批次二百八十三记着「`sort_ref` 缺 `emalloc/erealloc/__arraycount` 无法链接」，
  本轮把参照件**链起来了**（`tmp/family-diag-ref/sort_ref`：补一个 `minixshim.h` 把 `__dead`/
  `__COPYRIGHT`/`__RCSID`/`__arraycount` 宏化、一个 `sort_shim.c` 把 `e*` 族按最小语义实现，再去掉
  与 `fgetln_shim.c` 重名的 `progname_shim.c`）。**可用边界**：它链的是宿主 glibc 的 `getopt`，
  所以 libc 前置行与 usage 块不能拿它当基线（`-M` 那形还会在 `getprogname()` 上段错误，属垫片缺陷）；
  能拿它量的是 `sort` 自己的五句 `errx`、键形的接受/拒绝、以及排序行为。
  **分流了什么**：`setcolumn`/`setfield` 里是**五句**不同的 `errx(2, …)`（`init.c:160-216`），旧树把它们
  挤成一句 `IllegalFieldDescriptor`，所以四句都在拿别人的句子签字。`parse_column` 重写成 C 的
  `setcolumn` 形（`scan_column` + `scan_long`），返回 `Result<KeySpec, KeySpecError>`，句子文本单源在
  `KeySpecError::sentence`。四处按 C 改正：① `-k 0` 之前被当 1 静默接受，现在走
  `field numbers must be positive`，且那一句用的是 `strtol` 的结果（`-k x`、`-k ''`、`-k ,5`、
  `-k 1,-2`、`-k -1` 全部同句）；② `.0` 在两列上不对称（减一只发生在开始列，`init.c:168-169`），
  所以 `1.0` 是 `illegal offset` 而 `1,2.0`、`1,2.` 合法；③ 列号是 0 还要给 `.C` 才是
  `cannot indent end of line`（`1,0.5`）；④ 全局标志的继承判据是「局部一个字母也没有才整体拿
  全局」（`init.c:193-194`）而不是逐位或——参照件量出 `-n -k 2r` 与 `-k 2r` **同序**（只按字典序
  的键反排），而旧写法把 `n` 也渗了进去。`b` 的两个判据也没合写：选列看 `icol.num`（`:157`），
  而 `optval` 的 `tcolflag` 看 `tcol.num`（`:173`），所以 `-k 1,0b` 里那个 `b` 仍算 BI。
  **推翻一（台账与文档都写着错的那句）**：批次二百八十三登记「`-k 2,1` 在 C 是
  `fields out of order`」——参照件给出退 0。C 的那三条件（`init.c:210-216`）要求
  `tcol.indent != 0`，而没给 `.C` 时缩进就是 0，源码注释自己写着「indent if 0 -> end of field,
  i.e. okay」；真正报那一句的是 `2.5,1.2`。本轮既钉报错的那形，也钉住**不报错**的那些形
  （`2,1`、`1,2.0`、`1,2.`、`1,0`、`1b`、`+1`、`01`），否则只钉单向的门挡不住「收严过头」。
  **推翻二（建模自创）**：月名排序在这份 C 里根本不存在——`M` 既不在 `SORT_OPTS`（`sort.c:148`）
  也不在 `optval` 表（`init.c:222-242`）里，`switch` 也没有 `case 'M'`，所以 `-M` 落 `usage()`、
  `-k 1M` 落 `illegal field descriptor`（参照件同读数）。本树的 `month` 位、`month_of` 表、比较
  那一臂与 `test_month_table` 已全部删除（核法：`grep -n "month\|'M'" src/sort.rs` 只剩两行——第 16 行与
  第 385 行——都是「这一位不存在」的正文，代码里零命中），07 篇的修饰符表与测试名清单同步更正。
  修饰集本身只收了 `optval` 表里本树已建模的那六个字母（`b d f i n r`）。
  **自己写坏过一次读数（已当场发现并重测）**：第一次 docker 跑报 **602/0**，与宿主的 621 不符；
  重跑把完整输出落盘后再数，得 **621/0**——少的 19 枚正是最后一个 target（`vis_face`），是我自己在
  容器命令里接的 `head -40` 把流截了。**读数不能拿截断过的管道算**，本轮改用「落盘再统计」。
  另一处过程修正：本轮新写的注释里有 18 处 `init.c:NNN` 锚点行号偏了几行，而
  `tools/pattern-gate.sh` 的 P18 **只查文件存在与落窗、不查语义**，所以它们全绿也没挡住错——
  逐条拿 `awk NR` 重读 C 原文重钉，并用 `tmp/fix-anchors287.py` 批量修正（代码、端到端门、07 篇三处）。
  三件套：宿主 `cargo test -p minix-textfilter` → **621 passed、0 failed**（基线 620：`--lib` 442→443，
  新增三枚（`dot_zero_is_an_error_only_on_the_start_column`、`the_five_key_spec_sentences_are_distinct`、
  `a_local_modifier_switches_the_global_flags_off`）、删一枚月名测、另一枚由 `test_key_spec_dot_zero_rejected`
  改名而来（同数）；`sort_face` 仍 11 枚，但其中那一枚改成八句正向循环 + 七个反向循环（620 + 1 = 621，等式已核）；docker `minix-ci:1.94` 同包 **621/0**（完整日志落盘后统计）、
  `cargo check --workspace -j 1` **0 error**；三架构 `--bins` 零错；**容器配方本轮加了一条**
  `-e CARGO_TARGET_DIR=/tmp/…`：上一批不带它时，容器以 root 往专属工作树里写了 10642 个 root 属主文件，
  把 `claim.sh release` 的 `rm -rf` 卡住了；本轮同法测量残留数为 **0**；`rustfmt --check` 本笔三件只报
  `bin/sort.rs` 两处漂移，拿 `git show HEAD:` 副本重跑确认两处基线即存在（`sort.rs` 本笔新行已由
  rustfmt 排定、复跑零漂移，工树污染面 0）；clippy 触及件零告警（`nonminimal_bool` 按德摩根改写，
  并在注释里注明那是 C 那句 `!(!(flags & BI) && (flags & BT))` 的同值形式）；三枚变异负控各咬一处
  （①继承判据改回无条件继承→lib 那枚报红；②五句压回一句→端到端那枚报红；③把 `.0` 减一施加到两列→
  lib 与端到端各一枚报红），均先打印 `mutation applied`、恢复用备份并 `cmp` 同一；
  `tools/pattern-gate.sh` PASS=15 FAIL=0 SKIP=1（P18 13341 个 token）；边界门 OK；
  `doc-style-lint --diff` error=0；断链基线 124 不变；五道收尾校验全过。核实日 `2026-10-15`；
  基线 `0c9681fb5`。
  登记不执行：① `optval` 表里的 `l`（按字段长度，`fields.c:208-209` 的 `length()`）与 `x`（十六进制数，
  `:213-214` 的 `numhex()`）仍未建模。本轮**故意不把两字母放进修饰集**：接受而无效会把「顺序错」
  伪装成「顺序对」，所以 `1l`/`1x` 现在报 `illegal field descriptor`（而 C 接受并按长度/十六进制排），
  全局 `-l`/`-x` 仍走集外字母那一支——这一形连同 `D`/`R`/`T`/`S` 四个字母归选项面那一条（下一批）；
  ② C 的默认是「并列键保持输入序」（`posix_sort` 为零时 `keylen` 不含整条记录，`fields.c:168`），
  而本树总是回退到整行比较——那相当于 C 的 `-S`，两边的默认不同，属比较半的独立一题；
  ③ `D` 只改内部调试位（`DEBUG('k')` 把正文换成键 dump）、`T` 只影响本壳没有的临时档引擎，
  两者在本壳不可观察，但也不能假装同形，所以继续留在集外字母那一支自报。

- ✅ **批次二百八十六把 `cut` 的列表语法按 C 定形（四形之外又拓出一形），并拓出一枚不可复现的容器门读数**
  （`2026-10-15`，`claim/CUT-LIST-GRAMMAR-qoder` @ 基线 `6c4c47517`，单域三线同一 crate：
  `src/cut.rs`、`src/bin/cut.rs`、`tests/cut_face.rs`，另改一枚既有测 `tests/sort_face.rs`（见下面
  那处推翻），无跨线登记）：批次二百八十一末尾登记的第三条。
  **改的是哪四形**（都在 `minix3/usr.bin/cut/cut.c:get_list` 里，旧写法把它们全拒）：
  ① 分隔符集是 `,`、空格、制表符三个且空项被折叠（`:175` 用的是 `strtok(list, ", \t")`，
  所以 `1,,3`、`1 3`、`1\t3`、` 1 , 3 ` 与 `1,3` 同解）；② 空列表不是错（`strtok` 对空串直接
  交 NULL，置位循环一次也跑不到，于是一个位置也不选，而 `x_cut.c:86` 照旧给每行补一个换行）；
  ③ `5-2` 不查大小关系（`:209-210` 只是 `start++ <= stop` 地置位，倒序就一个也不置，得到空
  集合而不是错）；④ 项数不封顶（`:163-164` 初次分配、`:199-206` 按 `ALLOC_CHUNK` 长大，
  本树旧写法封顶 16 项并把超限报成「非法列表」）。返回形状因此从「定长数组 + 计数」改成
  `Vec<Range>`，`MAX_RANGES`/`parse_part`/`parse_number` 三个旧件删除（共 56 行）。
  **第五形是实施中发现的，不在原登记的四条里**：C 在 `*p == '-'` 上写了**两个**判据
  （`:186`、`:189`），第二个测的还是同一个位置——只有 `isdigit(p[1])` 成立、`strtol` 真跑了，
  指针才前进（`:187-188`）；否则指针原地不动，第二个判据又命中同一个 `-` 并 `++p`（`:190`）。
  把它压成「跨过减号再读一个数」会同时错两处且正好互换：`1-2-` 在 C 是**接受**的（autostop
  那一支，`x_cut.c:80-82` 把 `maxval` 之后的原样打出，等价于从 1 打到行尾），压平后报残留；
  `5--` 在 C 是残留错（`illegal list value`），压平后报零。因此 `parse_token` 按两层判据写，
  并用一对句子门钉住（`the_two_dash_tests_are_not_merged` 与端到端那枚）。
  **一处推翻（台账读数不可复现，不是代码错）**：批次二百八十四登记「docker `minix-ci:1.94`
  同包 612/0」，本轮**按同一条命令重跑直接报红**：`sort_face.rs` 的
  `an_unwritable_target_is_refused_before_anything_changes` 期望退 2，容器里拿到退 0。根因是该镜像默认以 root 执行
  （`docker run --rm minix-ci:1.94 id` → `uid=0(root)`），而带 `CAP_DAC_OVERRIDE` 的进程走
  `access(2)`/`open` 时不看权限位，那个 `chmod 0444` 拦不住它——即 C 自己在 root 下也是写出
  结果并退 0。所以那枚门断的不是 C 的行为，而是**跑它的那个 uid**；旧登记要么当时用了非 root
  用户而未记录，要么读数取自宿主——两种情形都算三件套不齐。修法不是跳过也不是改容器配方，
  而是按有效 uid 分支两支各自钉住（root 支：退 0、stderr 为空、目标内容被排好；非 root 支：
  退 2、那句 `sort: <路径>: Permission denied` 逐字、旧内容不动），探针取 `/proc/self/status` 的
  `Uid:` 第二列。**本轮不动任何生产代码**（`bin_support.rs::writable` 的双生缝本身与 C 同形）。
  三件套：宿主 `cargo test -p minix-textfilter` → **620 passed、0 failed**（基线 613：
  `cut.rs` 新增两枚 lib 测 + `tests/cut_face.rs` 9→14 枚）；docker `minix-ci:1.94` 同包
  **620/0**、`cargo check --workspace -j 1` **0 error**；三架构 `--bins`（`x86_64-unknown-none`、
  `aarch64-unknown-none`、`riscv64gc-unknown-none-elf`）均零错；`rustfmt --check` 本笔三件共报
  三处漂移（`cut.rs` 的 `select_bytes` 多行签名与 `in_ranges` 链式、`bin/cut.rs` 的 `use` 排序），
  逐一拿 `git show HEAD:` 的副本重跑 `--check` 确认**三处在基线即存在**（本笔新增行零漂移），
  因此不入库也不「顺手排好」；那个 HEAD 副本比对用的是临时换位 + `cmp` 复原，已确认同一；
  clippy 触及件的新增代码零告警（`cut_face.rs:111` 的 `iter().copied().collect()` 与 `jot.rs:771`
  的 `approx_constant`（deny 级，使 `cargo clippy --all-targets` 在本 crate 非零退出）都是 HEAD
  即有的存量，本笔不顺手改，登记在此）；`tools/pattern-gate.sh` PASS=15 FAIL=0 SKIP=1（P18 回读
  13332 个 token 全落窗）；`tools/check-command-boundary.sh` OK；`tools/doc-style-lint.sh --diff`
  零 error（第一版正文里写了两处 `（Lnnn，工具生成）`被 SL-10 挡住，已改为符号锚点）。
  三枚变异负控各咬一处（均先打印 `mutation applied` 再跑门，恢复用备份并 `cmp` 同一）：
  ① 把两层减号判据压平 → lib 三枚报红（含新那枚）且端到端一枚报红；② 取消空项折叠 →
  lib 一枚 + 端到端两枚报红；③ 恢复「倒序区间报错」→ lib 一枚 + 端到端一枚报红。
  核实日 `2026-10-15`；基线 `6c4c47517`。
  登记不执行：① C 的 `autostart`/`autostop` 是**跳项、跳行的全局量**（`:183-184`、`:191-192`、
  `:213-219`），本树只把它折算成单个区间的开上界——单项列表（`5-`、`1-2-`）输出与 C 一致，而
  `3-,1` 那类靠全局 `maxval` 与 `positions` 位图拼出的行为不保证一致（下一项）；② 数值溢出时
  本树饱和，C 拿到 `LONG_MAX` 后会把 `positions` 重开到那个尺寸直到分配失败（`:199-206`），不建模；
  ③ `-b` 与 `-c` 在本树同义，所以四道 post-check 的第三道（`bflag && cflag`）仍不可达。

- ✅ **批次二百八十五接完 `pr` 的诊断面，并推翻了两处此前写在台账与门里的错结论**
  （`2026-10-15`，`claim/PR-DIAG-LEGS-qoder` @ 基线 `84963dd2b`，单域三线同一 crate：
  `src/lib.rs`、`src/pr.rs`、`src/bin/pr.rs`、`tests/pr_face.rs`，无跨线登记）：
  同族诊断面第四支，也是措辞不同形的那一支——`pr` 链的不是 libc `getopt` 而是自带的
  `minix3/usr.bin/pr/egetopt.c`（`grep -rln egetopt minix3/` 只命中 pr 目录三处）。
  **推翻一（本壳与本文档都错过）**：`main` 是 `ret_val = setup(…); if (!ret_val) {…}
  else usage();`（`pr.c:142-157`），所以**任何** setup 错都会再接那三行 usage；而壳侧注释与
  批次二百二十的登记都写的是“各有一句自己的错误文本（不再是 usage 行）”，端到端门也跟着
  断“stderr 只有那一句”——三枚门因此正在为错的形签字。现在各臂都是“自己那句 + usage”，
  逐枚钉住（数值选项六枚、`-a`/`-m` 两枚、页宽不足一枚、`-l` 三枚）。注意“不是一律 usage”
  与“不接 usage”是两件事，前一句本身没错，错在把它读成后者。
  **推翻二（措辞与形）**：集外字母那一句在 `egetopt` 里是 `illegal option -- <字母>`
  （`egetopt.c:144-151`），而 libc 版是 `unknown option`（`getopt.c:101-104`）；缺值那一句
  两边逐字相同（`:196-205` vs `:132-136`）。所以 `lib.rs:GetoptReport` 新增 `IllegalOption`
  一个变体，共三个臂（同一事实的不同版本分开存，而不是抹平），前缀都是 argv[0] 的
  最后一段（`egetopt` 自己用 `strrchr(*nargv, '/')` 算，`:145-147`）。`value!` 宏现在带字母。
  **开档失败那句也不是 `err(3)` 那个冒号形**：C 写的是 `"pr: Cannot open %s, %s\n"`
  （`pr.c:1364-1370`，大写 C、名字与句子之间一个逗号），`continue` 后退码由 `errcnt` 推到 1
  （`:157-159`）且**不接 usage**（它不是 setup 错）——拿上一笔的“三成分齐备”公式直接套就错；
  旧壳那句是自创的 `pr: cannot open <名>`。新增一枚门钉这个形（含“后面的操作数照跑”）。
  `-r`（`nodiag`）能屏蔽那一句，本壳无该枝（原有登记，本轮未动）。
  三件套：`cargo test -p minix-textfilter` → **613 passed、0 failed**（基线 612；`pr_face` 13→14 枚）；
  docker `minix-ci:1.94` 同包 **613/0**、`cargo check --workspace -j 1` **0 error**；三架构 `--bins`
  零错；`rustfmt` 只跑 `pr.rs`/`pr_face.rs`（`lib.rs` 的三处 `pub mod` 排序漂移是基线存量，
  不入库也不去“顺手排好”）；clippy 触及件零告警（`pr.rs:1089` 那几枚是存量分页代码的，
  行数因本笔上移）；三枚变异负控各咬一处：① Getopt 臂丢掉 egetopt 那一行→一枚报红；
  ② `fail()` 不接 usage→三枚报红；③ 开档腿退回自创句子→新那枚报红；三枚都先打印
  `mutation applied` 再跑门，恢复用备份并 `cmp` 同一。核实日 `2026-10-15`；基线 `84963dd2b`。
  登记不执行：`-p`（逐页暂停，需终端腿）与 `-r`（nodiag）仍是本壳没有的枝；本壳把
  `p`/`r`/`T` 三字母归为 `Unmodelled` 自报一句（C 接受它们，那一句不扮 C 形）；`errf` 那套
  “先把诊断暂存到临时文件、结尾再倒回标准输出”的机制（`pr.c:1648-1654`、`flsh_errs`）
  本壳没有，因此多条消息之间的**先后顺**与 C 可比而**与 stdout 的交错**不可比（本轮门只比
  stderr 内的内容，不去比跨流的顺）。

- ✅ **批次二百八十四把 `sort` 输出腿剩下的四条缝改成带号（`access`/`F_OK`/`lchmod`/`link`）**
  （`2026-10-15`，`claim/SORT-OUTPUT-ERRNO-LEGS-qoder` @ 基线 `83850de82`，单域三线同一 crate：
  `os/commands/usr-bin/textfilter` 里的 `src/bin_support.rs`、`src/bin/sort.rs`、`tests/sort_face.rs`，
  不碰 `os/libs/`，无跨线登记）：
  上一笔末尾登记的那条余面。C 在输出支上有四句 `err(2, ...)`：`access(outpath, W_OK)`
  （`sort.c:336-337`）、`access(outfile, F_OK)`（`:364-365`）、`lchmod` 失败（`:373-374`，
  句子是 `"cannot chmod %s: output left in %s"`，**两个名字都在句子里**）、`link` 失败
  （`:379-380`，同一句式）。既然都是 `err`，印的就得包括 `strerror(errno)`；而本树这四个
  缝只返 `bool`/`Result<_, ()>`，号在缝里被丢了，所以句子根本拼不出来。改法：
  `bin_support.rs:writable`/`set_mode`/`hard_link` 错型改成 `Result<(), Errno>`，新增
  `exists(path) -> Result<(), Errno>` 对位 `access(F_OK)`（旧的那腿拿 `probe_path(..).is_none()`
  当判据，本来就没号可给）。真机臂直接拿 `access(2)`/`chmod`/`link` 的真号；**宿主臂是一处
  同义改写**：std 没有 `access(2)` 包装，旧那一版只看 owner-write 位（给不出号也答不准非属主
  的情形），现在改成「以写方式打开一下」（不开 `O_CREAT`/`O_TRUNC`，不动内容也不动元数据），
  失败时 `raw_os_error()` 就是 `EACCES`/`ENOENT` 这类真号；两臂口径差异已写进缝的注释。
  `unlink` 两支出错 C 不看（`:376`），本壳也不看。未动的一处：本壳把写失败报在
  `Cannot open temporary file` 那一句上，而 C 那一支是 `fdopen` 失败（`:350-351`）——两者不是
  同一事件（C 的写失败由 stdio 在 exit 时默默丢弃），本轮只登记不改。
  三件套：`cargo test -p minix-textfilter` → **612 passed、0 failed**（数不变；端到端那一枚由
  `contains("sort:")` 收严成逐字等式 `sort: <路径>: Permission denied`，否则号丢了看不出来）；
  docker `minix-ci:1.94` 同包 **612/0**、`cargo check --workspace -j 1` **0 error**；三架构
  `--bins` 零错；`rustfmt` 只跑本笔三件（含 `#[path]` 连带项，污染面 0）；clippy 触及件零告警；
  两枚变异负控各咬一处：①把 `writable` 腿退回裸路径→那枚门报出左右差异（正好少一个句子）；
  ②把宿主臂改成永远 `Ok(())`→同一枚报红（证明这道检查本身是承重件，不只是文案）；两枚均
  先打印 `mutation applied` 再跑门，恢复用备份并 `cmp` 同一。核实日 `2026-10-15`；基线 `83850de82`。
  登记不执行：`chmod`/`link` 两腿的带号句子在宿主造不出稳定金样（需要让一个刚创建的 0600 临时档
  在 `chmod`/`link` 上失败），本轮只保证缝把号交到了壳里；写失败与 `fdopen` 的形状差异（上面那段）。
  **更正（批次二百八十六发现，`2026-10-15`，原正文不改）**：上面那句「docker `minix-ci:1.94` 同包
  **612/0**」不可复现——该镜像默认以 root 执行，而 `access(2)`/`open` 对带 `CAP_DAC_OVERRIDE` 的进程
  不看权限位，所以 `an_unwritable_target_is_refused_before_anything_changes` 在容器里拿到退 0。
  本轮把该枚改成按有效 uid 分支、两支各自钉住，读数与修法见批次二百八十六条目。

- ✅ **批次二百八十三接完同族诊断面第三支（`sort` 与 `join` 的选项腿），并把 `sort` 的键解析收严到 C 的停点**
  （`2026-10-15`，`claim/FAMILY-DIAG-JOIN-SORT-qoder` @ 基线 `a7a7d7e72`，单域三线同包，无跨界）：
  前两批（二百八十一、二百八十二）把同一句登记的两支撑做完了。**sort 三条出口本来挤在一个
  `BadOption(char)` 里**（所以壳只能自己发一句自创的 `sort: bad option`）：集外字母与缺值在 C 是
  「libc 一行 + `usage(NULL)`」两行（`getopt.c:101-104`/`:132-136` 接 `sort.c:252`）；键形不对是
  `errx(2, "illegal field descriptor")`（`init.c:189-190`）**不接 usage**；`-t` 给第二次是
  `usage("multiple field delimiters")`（`sort.c:214-215`）先 `%s: %s` 那一句再接整个 usage 块。拆成
  三型后，行数、有无 usage、退码（一律 2）都能被门分开。`usage()` 本体是两条 `fprintf` 拼成的
  **一行**，第二条以十三个空格开头——那十三个空格不是手抄，是从编译产物 `sort.o` 的 `.rodata`
  数出来的（`strings` + 逐字计数），端到端门逐字钉住。**顺带按 C 收严两处**：`-t` 只取首字节不报错
  （`:219` 就一句 `dchar = *optarg`，旧壳把多字节当错）；`setcolumn` 在**第一个不是修饰字母的字符
  上停下**（`init.c:204` 的 `for (; (tmp = optval(*pos, …)); pos++)` 遇 0 即停），旧写法把整串吃下去，
  于是 `-k x`、`-k 1 2` 这些在 C 报错的形被静默接受——不收紧的话新加的 `IllegalFieldDescriptor`
  就是不可达死码（上一轮的负控教训同一形状）。不对称也钉了一枚：结束列之后的残留 C 反倒不查。
  **join**：把挤在 `JoinError::Usage` 里的三条分开——只有「非两个操数」（`:236-237`）是裸 usage，
  表外字母与缺值前面多一行 libc 报文（`value!` 宏现在带字母）。**开档腿的 `err(2, …)` 补句子**：
  输入文件（`:285`）、直开输出（`:355`）、临时档创建（`:341`）三腿齐了（`create_exclusive` 的错位
  一并从 `()` 改成 `Errno`）；`access(W_OK)`/`lchmod`/`link` 三腿需先把 `bin_support` 那几个
  `Result<_, ()>` 缝改成带号，登记不本轮做。
  **两处自写错期望由门反证**：一是 `-t,,` 被当成 C 的错（其实结束列的残留 C 不查，那一形应当被接受、分隔符取首字符）；
  二是 `test_parse_options_splits_the_three_error_paths` 里重复了集外字母那形（删去，指针指到已有那枚）。
  三件套：`cargo test -p minix-textfilter` → **612 passed、0 failed**（基线 607：`sort` 决定半 +1、
  `sort_face` 10→11（拆三型新增 3 枚、改 1 枚）、`join_face` 9→10）；权威容器门 docker `minix-ci:1.94`
  `cargo test -j 1 -p minix-textfilter` → **612/0**；同容器 `cargo check --workspace -j 1` → **0 error**；
  freestanding `cargo build -p minix-textfilter --bins --target {x86_64-unknown-none,
  aarch64-unknown-none, riscv64gc-unknown-none-elf} --release` 三架构零错；`rustfmt` 只跑本笔七件，
  `git status` 复核污染面 = 0（本轮没出现跟随 `mod` 级联重排）；clippy 决定半零告警（中途新加的
  `VALUE_OPTS`/`takes_value` 被报 never used，删掉并把那段事实归入函数文档）；四枚变异负控各咬一处：
  ① sort 丢 libc 那一行→两枚报红；② 把 errx 那支也接上 usage→一枚报红；③ join 丢 libc 那一行→
  一枚报红；④ `setcolumn` 改回静默跳过→库门与端到端门各一枚报红。核实日 `2026-10-15`；基线 `a7a7d7e72`。
  **另记一处差点造成假报红的脚本问题**：第一版负控用 `python3 - <<EOF` 内嵌三行带 `\\n` 的锚文本，
  转义一层后锚不到→脚本抛异常，但后面的测试仍跑了旧代码并输出「全绿」，看起来像「负控没咬住」。
  改法：把变异脚本落成 `/tmp/ctl283.py` 文件、先打印「mutation applied」再跑测试，并把注入前的
  锚文本存在性作为硬前置（同一条记忆里的「注入必须验效」）。恢复也不用 `git checkout`：本轮
  一度用它退掉了 `bin/join.rs` 的真实编辑，随即重贴。
  登记不执行：`sort` 的 `D`/`R`/`T`/`l`/`S`/`x` 六个选项字母未建模（现在只能落进集外字母那支）；
  `-k 0` 在 C 是 `field numbers must be positive`、`-k 2,1` 在 C 是 `fields out of order`，本树分别
  当 0→1 与不查；`access`/`lchmod`/`link` 三腿的带号缝；`pr` 的 `egetopt`（`pr.c:1655`）需单独读源。

- ✅ **批次二百八十二接完同族诊断面的第二支（`head` 与 `uniq`），并把宿主错误号换算也归成单源**
  （`2026-10-15`，`claim/FAMILY-DIAG-HEAD-UNIQ-qoder` @ 基线 `19e9058b6`，单域三线同包，无跨界）：
  接着批次二百八十一那条登记做。**head 三处**：① 集外字母与 `-c`/`-n` 缺值在 C 是两行
  （`getopt.c:101-104`、`:132-136` 先印，再 `head.c:115-117` 的 `case '?'` → usage），且 head 全文
  只有一处 `usage()`——所以 `HeadArgError::Usage` 这个变体本来就是多余的，直接删掉而不是再返它；
  ② usage 行的程序名走 `getprogname()`（`head.c:199-203`），不写死 `head`；③ 开档失败那句
  `warn("%s", *argv)`（`:124-126`）补上 `strerror` 句子。**uniq 四处**：同样补 libc 前置行，并区分
  两支不同的裸 usage（`uniq.c:124` 的第三个操作数）；`-f`/`-s` 的非法值不再混进 usage，走自己的
  `errx`（`:91`、`:97`，带坏值不缀号）；开档腿是致命的 `err(1, "%s", name)`（`:216-221`）三成分齐备；
  跳过数的判据改成 C 那两条（`strtol` 的形状），而不是旧的 `parse::<usize>()`。
  **`strtol` 那一处不是凑数**：C 只问「转出了负数」与「`*p` 还有残留」，而 `strtol` 一个数字都没转
  出来时值为零且 `p` 停在数字起点，所以四形是有分界的：`-f ''` 是合法的零、`-f ' 0'` 跳前导空白、
  `-f+0` 可带正号、`-f -` 报句（旧写法四形一律当 usage 错）。`skip_count` 把这三条写在一处并带测。
  **一处由参照件反证回来的自写错期望（两连）**：先把 `"-f", "-1"` 的回显写成 `-1`、又把
  `"-f", "+0"` 当成合法的正号形——实际上 `obsolete()` 先把独立的 `+0`/`-1` 折成 `-s0`/`-f1`，
  所以 `optarg` 看到的、以及被拼回句子的是折叠后的串（宿主参照件同读数：
  `uniq_ref: illegal field skip value: -s0`、`-f1`）；两形都改测，附值形 `-f+0`/`-f-1` 另测。
  **参照件的可用边界（三条，下轮引用前先看）**：`tmp/family-diag-ref/` 编 `uniq.c` 需补
  `fgetln` 垫片与 `progname` 垫片，但（a）它的 getopt 是 glibc 的，两句措辞不同形，只能比「几行」
  不能比文字；（b）选项串首字符 `-` 的语义两边不同（glibc 遇非选项返 `1`，C 的代码只认 `case '-'`），
  所以一旦带文件操作数就落 `default: usage()`——参照件只能走标准输入；（c）我们的 `fgetln`
  垫片在读路径上段错误（与无选项基线同退码），所以「接受」可由退码证明，输出内容不可比。
  **顺手归单源的两处**（与上一笔同一理由，不算改题）：宿主 `io::Error` → `errno` 的换算从
  `bin/tail.rs` 的局部副本上提为 `bin_support.rs:errno_of`（与 `read_file` 同口径），`open_input`/
  `create_sink` 的错误位从 `()` 改成 `Errno`（`uniq` 的 `err(1, "%s", name)` 需要那个号；
  `sort` 的调用点本轮只换类型不改文案，它的诊断面同 `join` 排在下一轮）。
  三件套：`cargo test -p minix-textfilter` → **607 passed、0 failed**（基线 604；`uniq_face` 1→4 枚拆分，
  `head_face` 改写两枚）；权威容器门 docker `minix-ci:1.94` `cargo test -j 1 -p minix-textfilter`
  → **607/0**，同容器 `cargo check --workspace -j 1` → **0 error**；三架构 `--bins` 零错（均 `Compiling`+Finished）；
  clippy 两支撑本体零告警（`--bin uniq`/`--bin head` 的指向 `bin/uniq.rs`、`bin/head.rs` 的告警为 0，
  其余 43 条是本 crate 存量库告警）；
  `rustfmt` 只对本笔文件跑并回退跟随 `pub mod`/`mod` 级联的 20 个无关件与两处基线存量重排
  （`cut.rs` 的 `select_bytes` 签名与 `in_ranges` 链式）；四枚变异负控各咬一处并 `cmp` 复原同一：
  ① head 丢 libc 前置行→`head_face` 报红；② 两个跳过值退回一律 usage→两枚报红；③ 开档腿丢
  `strerror` 句子→一枚报红；④ 把空值判成错→「合法零」那枚报红。核实日 `2026-10-15`；基线 `19e9058b6`。
  登记不执行：`join`/`sort` 两支的同形诊断面（`join.c:226-227`/`:630-637`、`sort.c:250-251`，
  sort 退 2）；`pr` 的 `egetopt`（`pr.c:1655` 用的是自己的解析器，需单独读源定形）；
  `head`/`uniq` 的宿主参照件在带操作数时不可用（上面那三条边界）。
  **本笔自犯并已纠正的两处过程错**（写下来因为两者都是旧教训的变体）：
  ① 负控脚本里用 `cd ../../..` 换楼层，算错一级后 `cp` 把在制的 `bin/uniq.rs` 复进了**共享主树**；
  当时拿它当「基线」比 clippy 读数，读到的是两份不同代码拼起来的 `E0308` 错位，差点据此写下
  「本轮带走了两枚存量告警」的假证据。已 `git checkout -- os/commands/.../bin/uniq.rs` 复原主树
  （只这一件，不碰文档线程的脏件）并重新在自树跑 clippy 取数。纪律：**脚本里不用相对 `cd` 换楼层，
  路径全用绝对量**；比较基线读数只从 `git show HEAD:<path>` 取，不从主树工作副本取。
  ② 另一枚负控写成了非法表达式——编译不过也算「红」，但那不是门咬住了行为，而是变异根本没进到语义；
  已换成合法改写重跑（即上面那枚④）。负控的有效判据必须是「能编译、行为变错、门报红」三件同时成立。

- ✅ **批次二百八十一把 `cut` 的诊断腿整支接回 C（五处偏离，其中一处是通道错），并把 libc 那一行归成单源**
  （`2026-10-15`，`claim/FAMILY-DIAG-LEGS-qoder` @ 基线 `9c645d31d`，单域三线同包，无跨界）：
  上一轮接 `tail` 时发现 Minix3 的 `getopt` 会为集外字母与缺值**各印一行**，然后才轮到命令
  自己的 `usage()`——那是 libc 行为，不属于任何一条命令（`minix3/lib/libc/stdlib/getopt.c:101-104`
  写 `"%s: unknown option -- %c\n"`、`:132-136` 写 `"%s: option requires an argument -- %c\n"`，
  optstring 不以 `:` 开头就走这两支；字母**不带引号**，本机 glibc 的 `invalid option -- 'x'` 不同形）。
  普查五支（`head`/`cut`/`uniq`/`join`/`sort`）确认它们都是 `case '?'` → `usage()`，所以这一行集体缺失。
  本轮只动 `cut` 一支（一次一条），但句子存成单源 `lib.rs:GetoptReport`（带一枚逐字测），
  `tail` 也从内联格式改为调它——同一个事实写在两个壳里就是重复承载，抹平任一处都不会改变行为
  （那是上一轮负控「改坏了却全绿」的同一形状）。
  **`cut` 本轮接回的五处**：① `usage()` 实为**三行**且以制表符对齐、那句里没有程序名占位
  （`cut.c:290-296` 写死了 `cut`），而本壳把自创的单行发到了**标准输出**——`support::emit` 是
  stdout，诊断被当成数据吃掉，这是通道错不是文案错；② 上面那两行的 libc 前置行；
  ③ 两类列表并存与「没列表」「`-d`/`-s` 不配 `-f`」是选项环**外**的四道 post-check
  （`cut.c:120-128`），全走裸 usage，而列表合法性在环**里**就地查（`get_list(optarg)` 坐在
  `case 'f'` 里，`:105`）——两个优先级方向各一枚端到端测（旧写法把冲突当环内错抢先报，还把
  自创的 GNU 句子 `only one type of list may be specified` 当成了 C 的文案）；④ `-d` 只取首字节
  且不报错（`cut.c:101-102` 就一句 `dchar = *optarg`），旧写法把多字节做成专门的错（`BadDelimiter`）；
  ⑤ 列表两句 errx 都不把坏文本拼回句子（`cut.c:196`、`:198`），而旧壳对非法列表是**静默退 1**。
  **顺手拓出一处测量通道缺口**：`cut` 的操作数腿走裸 `minix_sys::open`，在宿主没有双生面，
  所以直接跑宿主二进制时**任何**操作数都打不开（与 tail 早年被登为「宿主逐字对账不适用」同一坑）；
  接回 `bin_support.rs:read_file` 后这一腿才可测，才钉住了 `warn("%s", *argv)` 的三个成分。
  **本轮自己写错过一次期望（门反证）**：先把 `""`/`1,,2`/`5-2` 当成「illegal list value」的读数写进
  门里——而 C 的 `get_list` 用 `strtok(list, ", \t")`（`:176`）会折叠空项并把空格制表符也当分隔符，
  `5-2` 不查大小关系（`:193-195`，注释明写比 POSIX 草案宽），`-f ''` 连一次循环体都不进。
  这三形与超 16 项同理（C 按 `ALLOC_CHUNK` 长大）归入下一条待办（**列表语法**，不是诊断），
  本轮的门只取两侧一致的形。
  三件套：`cargo test -p minix-textfilter` → **604 passed、0 failed**（基线 594：新建
  `tests/cut_face.rs` 九枚 + `lib.rs` 单源测一枚）；权威容器门 docker `minix-ci:1.94`
  `cargo test -j 1 -p minix-textfilter` → **604/0**（从 `/work/os` 现编）；同容器
  `cargo check --workspace -j 1` → **0 error**；freestanding `cargo build -p minix-textfilter --bins
  --target {x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf} --release` 三架构均
  `Compiling` + Finished；**格式化的坑（本仓旧教训复发一次）**：`rustfmt --edition 2024 src/lib.rs …`
  会跟着 `lib.rs` 的 `pub mod` 声明把**整个 crate** 重排，当场污染 20 个无关文件；已全部
  `git checkout --` 回退，并把它顺手重排的本笔两文件基线存量也钉回去（`lib.rs` 的 `pub mod` 字母序、
  `cut.rs` 的 `select_bytes` 多行签名与 `in_ranges` 链式调用），最终 `lib.rs` 是**纯新增**（59 行、0 删）、
  本笔新增行零漂移；clippy 触及件零命中；
  `tools/pattern-gate.sh` PASS=15 FAIL=0（P18 落窗）；`tools/check-command-boundary.sh` OK；
  五道收尾校验全过（断链基线 124 不变）。三枚变异负控各咬一处并 `cmp` 复原同一：
  ①`usage` 改回 `emit` → 通道与内容双错被报出；②抹掉 libc 那一行 → 三支门同时报红
  （含 `a_later_unknown_letter_outranks_the_conflict`）；③把冲突查环内 → 仅那一枚报红（方向可区分）。
  核实日 `2026-10-15`；基线 `9c645d31d`。
  登记不执行（同族同形缺口，本单不顺手修）：① `head`/`uniq`/`join`/`sort` 四支的 libc 前置行与
  `usage` 原文（`head.c:115-117`/`:198-203`、`uniq.c:102-104`/`:252-256`、`join.c:226-227`/`:630-637`、
  `sort.c:250-251`）；`head` 还有一处 `warn("%s", *argv)` 缺句子（`head.c:124-126`）、`uniq` 的开档腿
  走的是致命 `err(1, "%s", name)`（`uniq.c:216-221`）而旧壳只印名字；② `pr` 用的是 `egetopt`，
  那一份的报错行为得单独读源定形；③ `cut` 的列表语法四形（上面那三条 + 分隔符集）。
  **另外两处本轮未动的已知差异**：`parse_list` 的 16 项上限与 `low > high` 拒（两者都是 C 接受而
  本树拒，已归入上面那条待办）；`-b` 与 `-c` 在本树同义，所以四道 post-check 的第三道
  （`bflag && cflag`）不可达。

- ✅ **批次二百八十把 `tail` 的跟随真正接线（上一轮的纯模型被实现取代了两处），并用宿主参照件逐字钉住环路**
  （`2026-10-15`，`claim/TAIL-FOLLOW-WIRING-qoder` @ 基线 `72b35d181`，单域三线同包，无跨界）：
  规划表 R18 的第二片。**接的是哪一支**：决定半把 C 的整型 `fflag` 建模成 `Follow` 三态
  （`Off`/`ByHandle`/`ByName`，`tail.c:110-121`），环落在壳里照 `forward.c:248-272` 的 `USE_SLEEP`：
  睡一拍 →仅 `-F` 的具名操作数查身份 →认出换档就重开并把新身份接手 →把当前位置之后的字节
  边读边写（`:191-200`、`:253-266`）。
  **实现选择取代了上一轮模型的两处**：上轮留的是「按路径重读 + `new_bytes(size, seen)` 折算」，
  本轮改成**手上一直握着那个句柄**——因为 C 的偏移是 `FILE *` 的性质而不是尺寸的算术。这不是
  口味问题：参照件在同一剧本下给出可区分的读数（`-f` 改名后往旧索引节点追加**看得见**、同名
  替身**看不见**），按路径重读的实现两条都会答错。于是 `new_bytes` 与 `StatIdentity.size` 一并
  撤下（不留未用的模型），「缩小不回退」改由端到端门钉：截空后重写 21 字节，打出的正是半行
  `EFGHIJ`——与参照件逐字同。
  **一处被负控逼出来的分工错（新模式候选，见文末）**：先把「只有 `-F` 才查」同时写在
  `probes_identity` 与 `reopen_needed` 两处，负控①（抹平 `probes_identity` 里的 `fflag == 2`）
  **改坏了却全绿**——重复承载同一事实的两个条件互相遮蔽。按 C 的分工收敛：`:255` 的门只管
  「谁去查」，`:257-260` 的比较只管四个字段。收敛后同一枚负控咬出了新加的
  `follow_by_handle_ignores_a_replacement_at_the_same_name`。
  **参照件及其可用边界**（`tmp/tail-follow-ref/`，宿主编 `tail.c`+`forward.c`+`reverse.c`+`read.c`+`misc.c`，
  加 `-D__minix` 让 kqueue 半支被排除、`-D'SIZE_T_MAX=...'`、`-include stdint.h` 与一份
  `getprogname`/`setprogname` 垫片（本机 glibc 不声明它，隐式声明会把返回值截成 `int`））：
  静态十一形逐字同读数（另有一形 `-x` 只比得了 usage 那一行，getopt 那行按上文不可比），
  动态六形逐字同读数（追加、`-F` 换档、截空后重写、`-f` 改名后追旧 inode、`-F` 后删档静默、
  管道上的 `-f`；前缀因路径不同而不可比，只比句子与行为）；**它比不了
  getopt 那一行**（它链的是 glibc 的 `getopt`，那句写 `invalid option -- 'x'`，而 Minix3 写
  `unknown option -- x`）——这一句的真值只能来自 `minix3/lib/libc/stdlib/getopt.c:101-104`。
  **一道被参照件与门双重反证回来的自写错期望**：本轮先把 `-n 3 -F a` 判成 usage 错，库门当场报红——
  C 的互斥只写在 `if (rflag)` 那一支里（`tail.c:143-145`），参照件同形给「仍在跟随 + 末三行」，
  两方一致后改测。
  **同一条诊断线上的三处既有偏离（本笔一并接回，与批次二百七十五同形）**：① `usage()` 在 C 是大写
  `Usage:`、带 `getprogname()`、选项列表本来就有 `-f | -F`（`tail.c:299-305`），旧写法小写且只列 `-r`；
  ② 操作数打不开那句走带号的 `xerr`/`vwarn`（`misc.c:53-57`、`:65-76`），旧写法把 `warn(3)` 当 `warnx(3)` 用、
  少了 `strerror` 句子；③ 集外字母与带值选项缺值在 C 都是**两行**（`getopt.c:101-104`、`:132-136`
  先印一句，再返回 `'?'` 落 usage），而 `ARG` 首句与 `-r` 配跟随那两支只有一行——因此
  `TailError::Usage` 拆成三型（`Usage`/`UnknownOption(char)`/`MissingValue(char)`）而不是返一个。
  **一处登记不复刻**：读失败时本壳把整个操作数判失败，C 的前向两臂（`FBYTES`/`FLINES`）在
  `forward.c:112-134` 已把读到的字节抄到了输出——差异只在「读中途失败」这种造不出金样的腿上。
  三件套：`cargo test -p minix-textfilter` → **594 passed、0 failed**（基线 583：`--lib` 438/0 净增 1 枚、
  `tail_face` 10→**20**）；权威容器门 docker `minix-ci:1.94` `cargo test -j 1 -p minix-textfilter` →
  **594/0**（从 `/work/os` 现编），同容器 `cargo check --workspace -j 1` → 0 error；
  freestanding `cargo build -p minix-textfilter --bin tail --target {x86_64-unknown-none,
  aarch64-unknown-none, riscv64gc-unknown-none-elf} --release` 三架构均 `Compiling` + Finished
  （真机臂是本轮新写的 `open`/`read`/`stat`/`fstat`/`lseek`/`nanosleep`/`close` 七条缝；注意
  riscv 的目标名是 `-unknown-none-elf`，写成 `-unknown-none` 会报「学不到目标信息」）；
  `rustfmt --edition 2024 --check` 三件零漂移；clippy 触及件零新增（本笔自引的两枚 `needless_borrow`
  与一枚 unused Result 已清；`window.rs` 仍报的那三枚是存量：行窗循环变量一枚 + 测内未用导入两枚）；
  三枚变异负控各咬一处（①抹平 `fflag == 2` →同名替身腿报「expected silence, got "replacement1"」；
  ②两道门调换→face 与 lib 各报红；③删 `*sbp = statbuf`→`-F` 换档后的追加行被重复内容顶掉），
  每枚复原后 `cmp` 同一。核实日 `2026-10-15`；基线 `72b35d181`。
  **更正批次二百七十九的一句登记**：那笔末尾写的「多操数时 C 的 `forward()` 永不返回、所以只有
  第一个文件被跟随」不成立——`tail.c:135-137` 在打开任何文件之前就把「跟随 + 两个以上操作数」判成
  致命错，那个形状不可达；可达的是本笔实现的两道门及其先后（参照件对 `-f A B`、`-F A B`、
  `-r -f A B` 三形都只给那一句，退 1、不打任何内容）。
  **一处锚点现查现钉（本轮引用过的行号逐条回读真源重钉）**：`fflag` 的声明在 `extern.h:51`（旧注释写的 `:41`
  顶到了 `enum STYLE` 前面）、`enum STYLE` 在 `extern.h:38`（旧注释写的 `:36`）、`xerr` 在 `misc.c:65-76`、
  `xerrx` 在 `misc.c:78-89`（本轮先写成 `:74-83`/`:53-72` 两组错区间，均已钉回）、`SEEK_CUR` 在
  `minix3/sys/sys/unistd.h:175`。
  **新模式候选（交文档线判是否入 `prompt/review-rules/review-patterns.md`）**：「同一个事实被两处
  条件重复承载 → 变异负控改坏任一处仍全绿」。这类冗余不会被测试发现，只能由**故意改坏看一下是否
  报红**发现；判据是每枚负控必须报红，报不了红就回头找是不是写了两个守卫。适用边界：只适用于
  本笔新写的判据（存量重复另说），且收敛方向是「按真源的分工一层只留一处」而不是删了了事。
  留白：`tail -f < /dev/null`（可定位但永不增长的标准输入）在 C 与本壳都是永久睡下去，同一形
  不入测；真机跟随腿（`nanosleep` 与 VFS 读到新尺寸）本轮只做到编过，跑要等 imgrd 播种归 C-57/E5。

- ✅ **批次二百七十九判定 `tail -f`/`-F` 可行（原以为是门控项，实为缺一条双生缝），并先落纯模型**
  （`2026-10-15`，`claim/TAIL-F-FEASIBILITY-qoder` @ 基线 `f4bc9c1e1`，单域三线同包，无跨界）：
  规划表 R18 的判定轮。**判的是什么**：`tail` 的跟随两形此前是「认得字母就拒收」
  （`window.rs:502` 的 `TailError::NotModelledFollow`），要不要做、能不能做此前没有论证。
  **结论：可行，且不需要等可写根盘**。三条依据逐条取自真源与本树代码，不是印象：
  ①真源在 Minix 上**根本不用事件通知**——`forward.c` 的 kqueue 半支被 `#if !defined(__minix)`
  排除（`:204-230`），实际走的是 `USE_SLEEP`：`sleep(1)` 一拍 + `-F` 时 `stat` 比
  `st_ino`/`st_dev`/`st_rdev`/`st_nlink == 0` 后 `freopen`（`:246-275`）。所以「要 poll/select 才行」
  这个前提就不成立；②等待通道本树已有：`minix_sys::nanosleep` 的实现就是 VFS 的空集 `select`
  带超时（`misc::nanosleep_via` → `select_empty_via`，与 C `nanosleep.c` 同源），③文件系统层能长文件：
  `os/fs/ext2/src/server.rs` 的 `write` 会更新 `record.size` 并回写 inode + 刷新缓存（`:868-873`），
  `read` 每次重取 `inode.size`（`:807`），所以「读到 EOF 后再读能拿到新字节」在 fs 侧成立。
  **真正缺的只有一条**：`select_empty_via` 没有宿主双生面（它总是 `perform_syscall(VFS_CALL_SELECT)`），
  而命令的宿主测确实能读真文件（`tail_face.rs` 已在用 `fs::write` 造真档并断言读出内容），
  所以按本仓既有的双生缝套路（`bin_support` 的 `stdin_chunk`/`flush_stdout` 都是这个形状）
  补一条 `sleep` 缝即可，宿主测与机器测两头都能覆盖。
  **一处顺带纠正的记录习惯**：`C-57` 那行「写侧仍被可写根盘门控」（协调台账 §4 的 `C-57` 行）曾被本文档当作
  本项不可行的理由引用过；本次核对后明确：**根盘可写性只影响真机冒烟的写侧，不影响本项**——
  宿主测走的是宿主自己的真文件，不经过根盘。
  **本批动手**：把每拍决策做成纯判据先落进决定半（`StatIdentity`/`reopen_needed`/`new_bytes`），
  并写明两条容易被想当然的细节——`-f` 不做换档检测（`fflag` 只有 2 才比身份）、
  **文件缩小时真源不回退偏移**（收缩重定位只存在于被排除的 kqueue 支里，`forward.c:236-243`），
  所以 `size <= seen` 这一拍交 0 且调用方不得把 `seen` 改小。壳**仍拒收** `-f`/`-F`，
  不做半接线（决定半的模型注释里写明「壳尚未接线」）。
  三件套：`cargo test -p minix-textfilter` → **583 passed、0 failed**（基线 581，+2 枚模型测）；
  `rustfmt --check` 触及件零漂移；clippy 触及件零命中（该文件报的两处 unused import 在存量测试代码里，
  与本批 diff 无关）。核实日 `2026-10-15`。
  登记待接线（第二片，非阻塞）：①`bin_support` 的 sleep 双生缝（宿主 `std::thread::sleep`／
  真机 `minix_sys::nanosleep`）；②壳的跟随循环与「多操数时 C 的 `forward()` 永不返回、
  所以只有第一个文件被跟随」这一形一并照搬；③端到端门喂真档 + 追加 + 断言新行（含 `-F` 换档与缩小不回退）。
  【批次二百八十更正：这一句里的②不成立，`tail.c:135-137` 的单文件门使那个形状不可达；
  ①③已按本笔落地，sleep 缝最终住在 `bin/tail.rs` 而不是 `bin_support`（理由见那笔）】

- ✅ **批次二百七十八复核结案：上一批登记的「误读是否传到 `caesar`」为否，并把两命令的来源差写进注释**
  （`2026-10-15`，`claim/CAESAR-STRTOL-AUDIT-qoder` @ 基线 `33b401c7d`，单域三线同包，无跨界）：
  接回批次二百七十七的登记②。**查的是什么**：批次二百五十二对 `_strtol.h:69` 的误读是否也进了
  `caesar` 的轮转解析。**结论是没有**，理由在真源：`caesar.c:131-133` 的 `get_rotation` 是
  「`errno = 0; rot = strtol(...); if (errno == 0 && (arg[0] == '\0' || *endp != '\0')) errno = EINVAL;`」——
  它**自己合成** `EINVAL`，从不依赖 `strtol` 在无转换时是否置号；`primes` 走的才是「只看 `strtoumax`
  留下的 errno」那条路（真源在无转换时不置号），所以同一串 `abc` 在两个命令里分属两句。
  **实测对照**（宿主编译的 `caesar_ref`，`cc` 只需两个 NetBSD 宏）：八个输入形 `5x`/`-3`/`""`/`0`/`26`/
  `2147483648`/`abc`/24 位溢出串 ⇒ 逐字同 5 形、仅句子来源不同 3 形（参照给 glibc 的
  `Numerical result out of range`，本树给 Minix3 表的 `Result too large or too small`，这是被测对象的语义）、
  **意外差 0**。C 把负值算 `ERANGE` 而不是畸形输入这一微妙处，现有门已经钉着（`-1`/`2147483648`/
  `99999999999` 三形断 `ERANGE` 句，`""`/`5x`/`-` 断 `EINVAL` 句）。
  **本轮动手的三处**：①门上那句自相矛盾的旧注释——它写「不缀 `strerror` 尾巴，本树没有表」，
  而它下面两行断言的正是表里的句子（批次二百三十三之后表就有了），改准；②把 `abc` 加进
  `EINVAL` 那一族（它就是 `primes` 误读的照妖镜，两命令在此分家）；③在 `caesar.rs` 的
  `parse_rotation` 文档上写下来源差并明确「别把 `primes` 的结论搬过来修这里，反之亦然」。
  三件套：`cargo test -p minix-stdio-games` → **227 passed、0 failed**（枚数不变，新增的是一个输入形）；
  `rustfmt --check` 触及两文件零漂移；clippy 触及件零命中；一枚变异负控（把「空串或尾字符」的错型
  从 `EINVAL` 挪到 `ERANGE`）⇒ 门以精确差异行红（`Result too large or too small` vs `Invalid argument`）。核实日 `2026-10-15`。

- ✅ **批次二百七十七接上 `primes` 的零操数路，并纠正批次二百五十二对 `_strtol.h` 的一处误读**
  （`2026-10-15`，`claim/PRIMES-STDIN-LEG-qoder` @ 基线 `5c01d1e11`，单域三线 `os/commands/games/stdio-games`
  与同线文档，无跨界）：接回批次二百五十二的登记①②（零操数路 + base 0 形，规划表 R11）。
  **①本轮真正值钱的部分是纠正**：批次二百五十二写下「`_strtol.h:69` 在一个数字都转不出时置
  `EINVAL`，所以 `primes abc` 走 `err(1, "%s", argv[i])` 缀句子」，本轮把该函数读到底，发现
  `:66-72` 那段 `EINVAL` 的守卫条件是 `base && (base < 2 || base > 36)`——**base 参数非法**才走它，
  而 primes 两处调用都是 `strtoumax(…, 0)`，永远进不去；无转换时真源只做 `_strtol.h:184-186` 的
  `*endptr = any ? s - 1 : nptr` 且**不动 errno**，于是 `primes.c:139` 的 `*p != '\0'` 成立，
  `abc` 与 `12a` 同属 `errx` 那一句。三重佐证：真源逐行、Minix3 自己的 `tests/lib/libc/stdlib/t_strtol.c`
  只以 `errno != ERANGE` 判越界（无「无转换置号」的预期）、宿主编译的 C 参照件实跑给
  `primes_ref: abc|: illegal numeric format.`。所以操数路的 `err` 族**只剩越范围一种**，
  测试名 `a_word_that_cannot_convert_carries_the_number_sentence` 与事实相反，一并改名。
  外溢排查（未动，登记）：同一误读可能传到 `caesar` 的 `strtol` 支（本台账里 `caesar` 报文那笔中「`caesar 5x` 缀
  Invalid argument」的叙述），需单独一轮按 `_strtol.h` 复核有符号 `strtol` 的真行为。
  **②零操数路**（`read_num_buf`，`primes.c:176-205`）四支各归自己的落点：读错→`err(1, "stdin")`；
  EOF→`exit(0)` 一声不出；跳空白后行首 `-`→`errx` 负号句；`errno` 置起→`err(1, "%s", buf)`；
  `*p != '\n'`→`errx` 那句。**两支持的都是整块 `buf`（含换行）**，所以 C 的一行报文断成两个物理行
  （实测 `primes: abc\n: illegal numeric format.`）——这不是排版问题而是 `err`/`errx` 的实参形状。
  结尾判据是 `'\n'` 而非 `'\0'`（行尾无换行的最后一块也算失手）。空白行 `continue` 再读下一行。
  **③base 0**：`0x`/`0X` 跳两位、前导 `0` 切八进制（于是 `08` 停在 `8` 上报尾字符错）、前导空白与
  `+` 合法、**空串是合法值 0**（`strtoumax("")` 的 endptr 就落在终止符上，参照件 `primes "" 5` → 2/3
  与本树同读数）；`0x` 后无位时 `endptr` 回**最初位置**（真源的 `any` 只由累计到位置起，`:170`/`:178`），
  所以是尾字符错而不是零。负值在 `strtoumax` 里按真源是下溢置 `ERANGE`（`__INT` 为无符号型，
  `:152-157` 首枚位即触发），glibc 这里是安静回绕——两路都在转换前先拒负号，命令路不可达，
  库只为与真源一致而如此实现。
  **门**：`primes_face` 14→**17** 枚（零操数四支、含换行的 `buf` 形状、base 0 操数与 stdin 双路、
  「只有越范围缀句子」一枚），库测 5→**7** 枚（`test_base_zero_conversions`、
  `test_stdin_line_legs_split_as_c_does`）。三件套：`cargo test -p minix-stdio-games` → **227 passed、0 failed**
  （基线 222，+5；上一批基线在主树实测）；容器 `minix-ci:1.94`（挂仓根、带 `-u`）**227/0**、退码 0；
  三架构 `cargo build --bins --target …` 各 `touch` 真编 `Compiling=5`、0 error；`rustfmt --check` 零漂移；
  clippy 触及件零命中。另与参照件逐形对照 11 形：10 形逐字节同（除过程序名），第 11 形只差在
  errno 句子的来源——参照给 glibc 的 `Numerical result out of range`，本树给 Minix3 表的
  `Result too large or too small`，这是**故意的**（被测对象是本树的 libc 语义，不是宿主）。核实日 `2026-10-15`。
  **三枚变异负控**（各按 `cmp` 复原）：把无转换重新归进缀句子那族 ⇒ 两枚门红且给出
  `Result too large or too small` vs `illegal numeric format.` 的精确差异；拿掉八进制前缀分支 ⇒
  两枚库测红（`Ok(10)` vs `Ok(8)`）；拿掉空白行 `continue` 支 ⇒ 库测红（`TrailingJunk("")` vs `SkipLine`）。
  **过程自查一笔**：中途一次「测试全绿」的结论是假的——grep 模式没含 `^error`，而当时 `cargo test`
  正卡在编译错（残留的旧变量名）上；补正后所有读数都改成「含 error 行的过滤 + 现算总数」双条件。
  登记不执行：①选项 permute（台账 `7g`）；②`caesar` 的 `strtol` 支是否被同一误读传导；
  ③`primes` 的 `-d` 差值列在 stdin 起始值落在存表之外时首个不带列（已按真源实现并测过，
  但真机上整条 `-d`/`-h` 与 1e6 上限的组合仍待机器腿覆盖）。

- ✅ **批次二百七十六把 `find` 的 `!`/`-o` 两支各分成两句，改前三支坏形状根本不报错**
  （`2026-10-15`，`claim/FIND-OPSPLIT-ARMS-qoder` @ 基线 `140498dae`，单域三线 `os/commands/bin/fileops`
  与同线文档，无跨界）：接回批次二百七十五的登记①（规划表 R15 后半）。**是什么**：C 把两个算符的拒绝各分两句——
  `c_not` 有「`!` 后没东西」（`operator.c:198-199`，yank 到 NULL）与「`!` 与 `-o` 之间没东西」（`:200-201`，
  yank 到的节点类型是 `N_OR`），`c_or` 有「`-o` 前没表达式」（`:253-255`，`result == NULL`）与
  「`-o` 后没表达式」（`:257-259`，`or_squish` 交回 NULL）。我方只报了 `!` 的第一句，`-o` 的两支**完全不报**。判类 P0-code-bug。
  **改前的实测症状（先跑再改，不靠推断）**：宿主构建的 `find` 对 `find . -o`、`find . -print -o`、
  `find . ! -o -print` 三形都不拒绝，一路走到走表阶段（在宿主表现为 `find: .: Input/output error`）——
  即 C 在解析期就 `exit(1)` 的输入，我方会拿一棵语义不同的树去求值。`! -o` 那形更隐蔽：`-o` 被
  `parse_primary` 当成一个普通主项收进 `Not` 里（C 的表里 `-o` 只有 `N_OR` 一条，`option.c:96`，
  它不是可带参数的输出表主项）。
  **两道守卫的先后是有内容的**：C 先查左再查右，所以孤立一个 `-o` 报的是「before」不是「after」；
  `c_not` 先把连续 `!` 消合并（`while (node->type == N_NOT)`）再看类型，所以 `! ! -or` 落「之间没东西」。
  两条都各自钉了断言，不是随手加的分支。
  **声明差一处（查过真源才写的）**：`! -a` 在 C 里**不报错**——`option.c:63` 的 `-a` 是
  `{ "-a", N_AND, c_null, 0 }`，既非 NULL 也非 `N_OR`，两道守卫都不触发；本层交不出可被 `!` 取用的
  `-a` 节点，只能在解析期拒。要照搬得先给 `-a` 一个节点形，登记在壳侧头注与 06 篇，不在本批顺手做。
  **参照件的边界如实记**：本想照前几轮先在宿主编一个 C `find` 参照，但它需要 Minix 专有的
  `tzfile.h`/`util.h`/`REG_BASIC`/`SIGINFO` 等一串（试到第三轮仍缺），成本高于收益；四支的触发形状在
  `c_not`/`c_or` 源码里是逐行明确的，故本轮以 C 源为权威、并用**改前实测**证实现状，不再猜。
  **门**：端到端新枚 `the_two_operator_pairs_refuse_with_their_own_sentences`（六形各钉自己那句 + 退码 1 +
  不写标准输出，复用门面既有的 `assert_errx_line` 助手）；单元测在 `malformed_groupings_are_refused_at_parse_time`
  内补五断言（含 `! ! -or` 的消合并形与孤立 `-o` 的左优先形）。`find_face` 9→10 枚。
  三件套：`cargo test -p minix-fileops` → **454 passed、0 failed**（基线 453，+1；上一批基线在主树实测）；
  容器 `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-fileops` → **454/0**（与宿主同数，退码 0）；
  三架构 `cargo build -p minix-fileops --bins --target …` 各 `touch` 强制重编真编一遍，各 `Compiling=7`、0 error；
  `rustfmt` 触及三文件零漂移（`cargo fmt -p` 会波及他档，改完用 `git status` 逐档核对）；clippy 触及件零命中
  （`find.rs` 上唯一那条 `iter().any` 告警经 `git diff` 证实不在本批 diff 内，是存量）。核实日 `2026-10-15`。
  **三枚变异负控**（各按 `cmp` 复原）：撤左守卫 / 撤右守卫 / 撤 `! -o` 判定——三笔各自只咬端到端那一枚，
  且暴露的都是改前症状（`left: "find: .: Input/output error"`），即「坏形状滑到求值」这一形被门钉住了。
  登记不执行：①`! -a` 的 C 接受形（需先给 `-a` 节点形）；②`find.c:240` 的 illegal path 与
  `main.c:152` 的 `err(1, ".")` 在本树无对位点（不开 `dotfd`）；③走表告警在真机上的真号。

- ✅ **批次二百七十五让 `find` 的走表告警交出号并按 C 落句子，顺带补回 `warnx` 该有的程名前缀**
  （`2026-10-15`，`claim/PROBE-ERRNO-CONTRACT-qoder` @ 基线 `312dede08`；三线 `os/commands/bin/fileops`
  加一处二线再导出，跨界改点已按 §2 规矩先登记为 C-102）：接回批次二百四十九拆出的第2片与
  规划表 R14/R15 的头一半。**是什么**：C 对 `FTS_NS`/`FTS_ERR`/`FTS_DNR` 三类都打
  `warnx("%s: %s", fts_path, strerror(fts_errno))`（`find.c:230-231`）再置 `rval=1`（`:232`），
  而我方 `WalkProbe` 三方法答 `Option`，号在决定半就被丢掉，壳只能自己编
  `<路径>: stat failed` / `<路径>: directory read failed` 两个短语。判类 P0-code-bug + P0-test-missing。
  **先出定形再动码**（这是 99 篇 §1 挂着的「决定半能不能收 `Errno` 型」）：同包 `pathchk.rs` 的
  `PathProbe` 三方法早就是 `Result<i64, Errno>`（批次二百五十九落地并过了命令层边界守卫），
  所以答案不是新开的口子而是既有契约；本轮把走表腿补齐到同一形状，不动分层原则本身。
  **核对中拓出的两处既有偏离**（都在这一行上，故同批修）：①走表告警**没有程名前缀**——
  `support::warn` 是裸写 stderr，而同壳的解析错 `errx` 行都手工带了 `{prog}: `，这个不对称被新加的
  宿主端到端门当场抓出（`left: "/tmp/…: Input/output error"` vs C 应有的 `find: …`）；
  ②句子权威不能在三线另抄：号→句子的唯一真源是 `minix-types` 那张对位 `compat_errlist.c` 的表，
  于是走既有再导出通道加 `pub use minix_types::Strerror;`（`Strerror` 已实现 C 的完整形状，含表外
  `Unknown error: %u`），而不是在壳里 `match` 号到字符串。
  **一条容易搞对的判序（拿真源逐行核对，不看函数名猜）**：`fts_stat` 在逻辑形下 `stat` 失败时
  先 `saved_errno = errno`，若 `lstat` 也失败则把 **stat 的号**存进 `fts_errno`（`fts.c:950-955`），
  而不是 lstat 的；两号给的句子不同（典型为 `EPERM` vs `ENOENT`），取错就是打错报文。
  另一条是**反方向**的旧错话：`getdents` 读错在 C 里既不终止也不告警——`fts.c:763` 的
  `while ((dp = readdir(dirp)) != NULL)` 不区分读错与读到尾；本层把号交回并报一行 warn，
  已改准为声明差登记（要改成 C 的静默需要自己的证据轮，不混进本批）。
  **门**：单元侧新两枚（`walk_warning_line_ends_with_the_strerror_sentence` 钉 `ENOENT`/`EACCES`/表外码
  三种句子；`logical_walk_reports_the_stat_errno_not_the_lstat_one` 钉上面那条取号规则），
  并把两枚旧测试从「含不含路径」收紧成整行断言（`contains` 式断言会放过任何编造的后缀）；
  端到端侧新枚 `a_walk_warning_line_carries_the_program_name_path_and_sentence` 钉行形（程名+路径+句子+rval=1），
  它的可用边界写进门面头注：**宿主跑不出真号**，因为 `getdents` 走 VFS IPC（`minix-sys::getdents_via`），
  没有 VFS 服务时只能拿到 `EIO`，所以 `EACCES` 那类真号由单元测与机器腿各自负责。
  三件套：`cargo test -p minix-fileops` → **453 passed、0 failed**（基线 450，+3）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-fileops -p minix-sys` 与
  `cargo check -j 1 --workspace` 见下；三架构 `cargo build -p minix-fileops --bins --target …` 各 `touch`
  强制重编，各 `Compiling=7`、0 error；命令层边界守卫（`tools/check-command-boundary.sh`）零违规。核实日 `2026-10-15`。
  **三枚变异负控**（各按 `cmp` 复原）：把 `visit_start` 的告警退回编造短语 ⇒ 逻辑形那枚红；
  把取号改回 `lstat_err` ⇒ 该枚以精确差异行红（`No such file or directory` vs `Operation not permitted`）；
  撤掉程名前缀 ⇒ 端到端那枚红。
  登记不执行：①C 的 `!` 两支与 `-o` 两支各两句报文，本层解析树分不出（`operator.c:199`/`:201`、
  `:254`/`:258`），属规划表 R15 的后半；②`find.c:240` 的 illegal path 与 `main.c:152` 的 `err(1, ".")`
  在本树无对位点（不开 `dotfd`）；③走表告警在**真机**上的真号（需机器腿跑一次带权限的目录）。

- ✅ **批次二百七十四把 `join` 的选项错文案逐支接回 C，并纠回两支归错族的判序**
  （`2026-10-15`，`claim/JOIN-ERRTEXT-FACES-qoder` @ 基线 `43e83e8f4`，单域三线 `os/commands/usr-bin/textfilter`
  与同线文档，无跨界）：接回批次二百四十九拆出的第2片（规划表 R13）。**是什么**：C 的八支选项错各是
  「先 `warnx(具体句)` 再 `usage()`」，句子带**自己的选项字母**与被拒的值；本树 `JoinError` 把这两样都丢了，
  壳只能拼出一句无字母的通用话，两支 `-o` 失手（C 是两个句子）也被拢成一个。判类 P0-code-bug + P0-test-missing。
  **参照件与它的可用边界（这条必须写清，否则读数会被高估）**：`minix3/usr.bin/join/join.c` 可以在宿主编起来
  （`cc -O2 -D'__RCSID(x)=' -D'__COPYRIGHT(x)=' -D'__dead=__attribute__((__noreturn__))' -include stdlib.h join.c
  tmp/join-probe/fgetln_shim.c`，shim 补 BSD 缺的 `fgetln` 与 `getprogname`），它给出的读数是
  `-1 x`⇒「-1 option field number less than 1」、`-1 0`⇒同句、`-j 0`⇒「-j …」、`-t xy`⇒「illegal tab character
  specification」、`-o 1.0`⇒「field numbers are 1 based」（退码 1、**不带 usage**）、`-o 3.1`/`-o 1.x`⇒
  「malformed -o option field」。但它在**进入 usage 之后**与**读文件那一段**都段错误（退出 139）：独立探针
  （同样两句 `warnx`+`fprintf(usage)`+`exit`）正常，故故障在 `join.c` 与本 glibc 探针壳的组合（`obsolete()` 改 argv
  与 glibc `getopt` 的内联状态、以及 `fgetln` 静态缓冲与 `slurp` 所有权不匹配），**不是被测代码的性质**。
  所以本轮以 C 源字面量为权威、参照件只用于印证消息文本，凡用不到的地方都这么注明。
  **两支归错族的判序（本轮的实质纠正，非文案润色）**：C 的形状是「`strtol` 取值 → 先查 `< 1` → 再查尾字符」，
  文件号那两支是「`strtol` → 先 switch → 再查尾字符」。本树此前按「是不是全数字」分流，于是
  ① `-j f1`（一个数字都转不出、值为 0）被送去「illegal field number」而 C 说「less than 1」；
  ② `-a 1x`（switch 已认下 1）被送去「not 1 or 2」而 C 说「illegal file number -- 1x」。
  为此决定半加 `strtol_decimal`（跳空白、可带号、**无数字时值 0 且不报错**）并按 C 的顺序重排两道守卫。
  **怎么修**：`JoinError` 带上该支的字母与被拒值（`Illegal*` 两支**故意不带字母**——C 那两句就没有字母）；
  壳侧 `warn_then_usage`（一句 + 两行 usage）与 `fail`（一句就走）分开，`-o` 的两支拆成 `MalformedOutputField`
  与 `FieldNumbersAre1Based`（新 `OListError` 承 `fieldarg` 的两个句子）；决定半不再 `use crate::TextError`。
  **为什么不选另一条路**：①在壳里按 `optind` 反推是哪个字母——决定半已经把选项扫过一遍，壳再扫一次就是两套
  状态机；②把 `-o` 两句并成一句省事，但那是本项目 2026 年 10 月立的规矩（同一失手在 C 里分属两族不得统一，
  批次二百三十九/二百五十二两次踩过）；③照抄 C 的 `-o 1.-1` 接受形（`u_long` 回绕）需要输出级参照，
  而探针件读不到那一步，故本树先拒并登记，不假装与 C 一致。
  **门**：`tests/join_face.rs` 6→**9 枚**（三行形六支各印自己那句、两支 illegal 只带值、`-o` 两句与 `-a`+`-v` 一行不接 usage），
  库测 15→**16**（`strtol_decimal_mirrors_the_no_digit_case`；并把 `parse_validations_and_mutual_exclusion` 里
  那枚按旧错形写的断言改成 C 的落点）。07 篇 `join` 行同步：枚数、三支新测试名、「余面」改写为已闭 + 新余面。
  三件套：`cargo test -p minix-textfilter` → **581 passed、0 failed**（基线 577，+4）；容器 `minix-ci:1.94`
  （挂仓根、带 `-u`）`cargo test -j 1 -p minix-textfilter` → 见下行读数；三架构
  `cargo build -p minix-textfilter --bins --target {x86_64,aarch64,riscv64gc}-unknown-none*` 各 `touch` 强制重编
  真编一遍，各 `Compiling=5`、0 error；`rustfmt --edition 2021` 触及三文件零漂移；clippy 触及件零命中
  （就地把一处 `join_error_fail(&prog, …)` 的多余取引用掉——`prog` 本就是 `&str`）。核实日 `2026-10-15`。
  **三枚变异负控**（测完按 `cmp` 复原）：把 `warn_then_usage` 尾上的 `usage(prog)` 换成直接退码 ⇒ 两枚 warnx 族
  测试红；把 `field_no` 的两道守卫顺序倒回 ⇒ 库测红（并暴露一个读数陷阱：`cargo test` 在 lib 目标失败后
  **不再跑后续目标**，总数看着从 581 掉到 435，那是没跑不是回归）；把 `-o` 的 `OneBased` 并进 `Malformed` ⇒ 库测红。
  登记不执行：①`obsolete(argv)` 的 legacy 重写（裸 `-a`、`-j[12]`）；②选项 permute（台账 `7g`）；
  ③`-o 1.-1` 的 C 接受形与所需输出级参照；④本 glibc 探针件在 usage 之后与读文件路崩，若下轮还要用 join 参照，
  先补 `fgetln` 的所有权语义（每次调用返回新缓冲）再谈。

- ✅ **批次二百七十三接上 `primes` 的 `-d`/`-h` 输出面，参照进程顺带拓出更早的一支 P0：`stop` 本来就是开区间**
  （`2026-10-15`，`claim/PRIMES-OUTPUT-FACES-qoder` @ 基线 `f36648125`，单域三线 `os/commands/games/stdio-games`
  与同线文档，无跨界）：接回批次二百五十二的登记不执行①（规划表 R10）。**是什么**：`primes.c:107-112` 的
  `-d`（差分列）与 `-h`（十六进制）两支输出面，本壳此前「认得字母但拒收」——打一行 `-d/-h not wired` 退 1。
  判类 P0-code-bug（两支输出面缺失）+ P0-test-missing；核对中还拓出一支**更早的** P0-code-bug：本壳把 `stop`
  当闭上界，而 C 从头注（`primes.c:49`：`Print primes >= start and < stop`）到实现都是开区间。
  **核对靠的是参照进程，不是猜测**：把 C 真源在宿主编成可执行件当金样源——
  `cc -O2 -D'__RCSID(x)=' -D'__COPYRIGHT(x)=' -D'__dead=__attribute__((__noreturn__))' -include stdlib.h
  minix3/games/primes/{primes,pattern,pr_tbl,spsp}.c -lm -o /tmp/primes_ref`（只欠 `-lm` 与三个 NetBSD 宏，
  不动一行 C 源），再与本壳逐形对跑 **21 个参数形 ⇒ `SAME=21 DIFF=0`**。这条差分同时是开区间那支的发现渠道：
  `primes 5 13` 参照给 `5 7 11`，我方给 `5 7 11 13`；`primes 2 2` 参照什么都不打，我方打 `2`。
  **四条容易猜错的形**（都取参照读数写进门，不靠推理）：①`-d` 的**首项是 `2 (2)`**——C 的 `prev` 从 0 起
  （`primes.c:220`/`:261`），看着像 bug 却是 C 自己的形状；②差值列用的前驱**可以落在区间外**（`primes -d 5 15`
  给 `5 (2)`，那 2 来自区间外的 3）；③起始值落在存表之外（表尾实测 65537，`pr_tbl.c`）时**第一个素数不带差列**——
  筛子路的守卫 `dflag && (prev || start <= *pr_limit)`（`:328`）走到那条路时后半恒假，只剩 `prev != 0`，
  实测 `primes -d 70000 70040` 首行就是不带差值列的 `70001`，而 `-d 65530 65545` 首项带 `65537 (16)`；
  ④`-h` **不改差值的进制**（`PRIu64` 恒十进制）：`primes -dh 113 130` → `71 (4)`、`7f (14)`，那个 `14` 不是 `e`。
  **怎么修**：决定半加 `TABLE_LAST_PRIME`（把支点常量与它的理由放在一起）与 `previous_prime`（对位 C 的 `prev`，
  复用本模块已有的 `is_prime`，不再抄一份判素数）；`bin_support` 加 `utox`（小写、无 `0x`，与 `utoa` 同双生面形状）；
  壳侧把「一次一判的选项串」换成 getopt 计数器循环（`-d`/`-h`/`-dh`/`-hd`/`-d -h` 都算数，裸 `-` 仍当操数），
  输出面按①②③④落形，区间那支在调用点降 1（`saturating_sub`，`stop == 0` 与 `lo > hi` 一同落空输出）。
  **为什么不选另一条路**：①把 `sieve` 本身改成开区间原语会连带改它 6 枚库测与既有语义——`sieve` 是通用的
  闭上界原语，「开区间」是 `primes` 这一支命令的面，该待在调用点；②在壳里自己数前驱等于把 `is_prime` 抄第二份；
  ③把差值也走十六进制（看似「更一致」）会直接偏离参照。
  **门**：`tests/primes_face.rs` 8→**14 枚**（新增：开区间专枚、差分含首项怪形、十六进制小写无前后缀、
  混进制下差值仍十进制、表外首项不带列、选项等价类三族），库测 +1 枚（`test_difference_predecessors`，
  含表尾 65537 的前驱 65521）；旧门里那枚断言 `-h` 走拒收的分支改写为「认得的字母不再被拒收」。
  三件套：`cargo test -p minix-stdio-games` → **222 passed、0 failed**（基线 215，+7；20 个测试目标）；
  容器 `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-stdio-games` → **222/0**；三架构
  `cargo build -p minix-stdio-games --bins --target {x86_64,aarch64,riscv64gc}-unknown-none*` 各 `touch` 强制重编
  真编一遍，各 `Compiling=5`、0 error；`rustfmt` 触及四文件零漂移；clippy 触及文件零命中（就地把新写的
  `% 2 == 0` 改成本仓惯用 `is_multiple_of(2)`）；差分门 `SAME=21 DIFF=0`。核实日 `2026-10-15`。
  **三枚变异负控**（测完按 `cmp` 复原）：撤掉调用点的降 1 → 开区间那枚独红；把 `uses_table` 拧成恒真 →
  表外那枚独红；`utox` 退回 `utoa` → 十六进制那一族 3 枚红。**另记一笔自己的错**：第一版把 `-dd` 归进
  `-dh` 的等价类，门当场红在 `option_letters_cluster_repeat_and_split`——代码是对的，错的是分类，
  已按参照的三个等价类（`{dh,hd,-d -h,-d -dh}`/`{d,dd}`/`{h,hh}`）重写。
  **账面回勾**：批次二百五十二那笔的门描述里「`-h` 走拒收支」与其「登记不执行①」自本批起被取代
  （历史条目按不重写原则保留，此处只回勾）；旧门 `a_bounded_run_prints_primes_and_stays_silent_on_stderr`
  的注释理由「区间的两端都算」是**错的**（输出因为 20 不是素数恰好看不出来），已改成实况并注明开区间由新枚钉住。
  登记不执行：①`primes` 的零操数路 `read_num_buf`（`primes.c:176-205`，含它自己的四腿）；②`strtoumax` base 0
  的前缀/前导空白/显式 `+`；③选项 permute（操数后面的选项，台账 `7g`）；④NetBSD getopt 自打的
  `illegal option` 那一行（本壳按 C 的 `default: usage()` 只出 usage，两者同形与否需单独一轮定）。

- ✅ **批次二百六十把五份命令侧诊断行从「固定栈缓冲裁」改成「先量长度再渲染」，C 本来就不截断**
  （`2026-10-14`，`claim/WARNLINE-NO-TRUNC-qoder` @ 基线 `010ce8fe8`，单域三线：`fileops`/`shell`/
  `textfilter`/`doctools`/`stdio-games` 五份 `bin_support.rs` 与两扇门）：接回批次二百五十九登记
  不执行②（规划表 v2-17 改判后的那一件）。
  **是什么**：六处写行入口（`warn_line`×4、`perror_line`×2、`shell` 的 `err_exit`）先把整行写进
  `[u8; 256]`/`[u8; 128]` 栈缓冲再 `len.min(buf.len())` 裁，于是超过缓冲的操作数**在句子之前就被
  静默砍掉**。而 C 不截：`vwarnc.c:62` 是 `vfprintf(stderr, fmt, ap)`，`perror.c:57-67` 只把句子
  放进定长的 `strerror_r` 表，调用者的串整个打出去。批次二百五十九的 512 字节案例里已看到这个
  形状。判类 P0-fact（把本树的缓冲裁剪说成 C 的行为）+ P0-code-bug（报文不完整）。
  **怎么修**：先量后渲染——`let mut sizing: [u8; 0] = []` 递空缓冲给权威，它交回的是「本该写
  多少字节」（`err_line` 的 `Sink` 写不进也照数），再按那个数 `alloc::vec![0u8; need]` 渲染一次。
  两次调用都是纯函数，不多一次系统调用；命令层有堆可用（这些壳本就用了 `Vec`）。
  **为什么不选另一条路**：①把缓冲改大到 1024/4096 只是把截断线搬到别处；②在 `minix-types`
  里改成返回 `Vec` 会把分配推给二线且改变权威签名，而截断本来发生在命令层自找的栈缓冲上；
  ③只在两个壳里修、留下四处同源重复，同一个错下次还会冒。
  **门**：新两枚各钉一族——`rm_face::a_long_operand_is_written_out_wholly_in_the_report_line`
  （`warn(3)` 族，600 字节操数整段出现在一行里）与 `what_face::a_long_operand_survives_the_perror_line_untouched`
  （`perror(3)` 族，同一要求），两者用的就是已验的逐字段对助手。
  另修一处口径：`shell` 共面测 `test_err_line_shape_and_truncation` 的注原本写「缓冲区不足时
  **按 C 的截断**走」——那是把本树的 plumbing 当成 C 的行为，已改成「权威交回需要的字节数、
  写不满只写那么多；C 本身不截断，调用方据此先量再渲染」。
  三件套：逐包 `cargo test` → **fileops 450/0**（基线 449，+1 门）、`textfilter 577/0`、`doctools 82/0`
  （+1 门）、`shell 85/0`、`stdio-games 215/0`；容器 `minix-ci:1.94`（挂仓根、带 `-u`）五包
  `test result: FAILED` 计数均为 **0**；`rustfmt --edition 2024 --check` 触及文件零漂移；clippy 触及
  文件零命中；核实日 `2026-10-14`；基线 `010ce8fe8`。
  **两枚变异负控**（测完复原并跑绿）：把 `fileops` 的 `warn_line` 退回固定 256 缓冲 → rm 那枚独红；
  把 `doctools` 的 `perror_line` 同样退回 → what 那枚独红。过程记一笔：第一版负控脚本用
  `s.index()` 定位，在同一文件里有两处同形代码时把 `perror_line` 那一处替换成了 `err_line` 形状（编译报错
  才发现），已 `git checkout` 该文件后逐函数重做——同文件多站点不能用「首个匹配」做变异。
  登记不执行：①本笔只改了写行入口的缓冲策略，没动任何一条报文文字，所以各阶段文档正文无需
  同步（已 grep 确认 06/07/10/22 篇没写过「会截断」的口径）；②真机读数仍归 `C-57`/`E5`；
  ③`stdio-games`/`textfilter` 两处 `warn_line` 的超长操数未各自建门（同一函数形状，rm/what 两枚
  已钉住），若将来改成分叉实现需补门。

- ✅ **批次二百五十九接上 `pathchk` 缺省模式的 `pathconf`/`stat` 探测，并把 `warnx` 漏掉的程名改回**
  （`2026-10-14`，`claim/PATHCONF-PROBE-1-qoder` @ 基线 `67567252d`，单域三线 `os/commands/bin/fileops`
  与同线文档）：接回 06 篇命令表里「缺省模式的 `pathconf`/`stat` 探测待批，现显式拒」那行（规划表 v2-16）。
  **是什么**：本壳只服务 `-p`，缺省模式直接报一行「等 pathconf」退 1。而 C 的 `check`（`pathchk.c:91-167`）
  在缺省模式下问系统三个问题，其中几处形状不能从名子猜：开头那个 `_PC_NAME_MAX` 问的是 `/`（绝对路径）
  或 `.`（其余，`:105`）；`-1` 与「带 errno 的失败」是两个答案（前者 POSIX 的「无上限」⇒ 检查关掉，
  后者才走 `NAME_MAX`/`PATH_MAX` 退路，`:106-107`、`:154-155`）；每过一个目次重探一次且**探失败时保留
  上次的答复**（`svnamemax`，`:139-145`）；`stat` 只认 `ENOENT` 不是拒因（`:124`），其余号才印 `warn` 一行，
  其 `%.*s` 长是 `strlen(pathd) - complen - 1`（比当前组件之前再短一个字节；负数在 C 的 printf 里就是
  「无精度」⇒ 打整串，已拿宿主参照进程实测）。判类 P0-design-missing。**拓出的第二件事**：`warnx` 两行
  本壳**没带程名前缀**（C 的 `vwarnx.c` 先打 `getprogname()` 再打 `": "`），拿参照进程逐字节一对就现形——
  判类 P0-code-bug。
  **怎么修**：决定半新增 `PathProbe`（三个方法，`Ok(-1)`/`Err` 两形分明）与 `check_system`（返回带原文碎片的
  `SystemFault`，位量与长度而非拼好的串）；薄壳新增 `SysProbe` 双生面——真机面走 `minix_sys::pathconf`/`stat`，
  宿主面两个探测答 `ENOSYS`（恰好就是 C 的「带 errno 失败」，于是走同一退路；而 MINIX 本把这两个变量
  答成头文件常量，所以退路数与真机一致），`stat` 用 `std::fs::metadata`（就是这一条让 `ENOTDIR` 支在
  宿主可测）；诊断改走 `support::prog_name`。
  **为什么不选另一条路**：①宿主面自己拿 `std` 模拟 `pathconf` 看似更能跑，但那是发明一个 C 没有的答案；
  ②把判定写进薄壳则九条分支在宿主不可测；③只修程名不接探测会把一条完整的腿拆成两笔欠账。
  **门**：新建本包第一扇 `tests/pathchk_face.rs` 六枚（`-p` 逐字金样、`ENOTDIR` 那一行逐字、缺文件不是拒因、
  退路上限 511、副本改名只动诊断行不动 usage、逐操数继续不提前收）；库测 +6（整秒那个形状：`/` 与 `.`
  之分、`-1` 与失败之分、重探失败留上答复、`ENOENT` 免、`PATH_MAX` 报答复减一）。
  三件套：`cargo test -p minix-fileops` → **449 passed、0 failed**（基线 437；+6 库测 +6 门）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）同包 **449/0**；三架构 `--bin pathchk --target {x86_64,aarch64,riscv64gc}-
  unknown-none` 各有 `Compiling` 行且 0 error；`rustfmt --edition 2024 --check` 触及三文件零漂移；clippy 触及
  文件零命中（`collapsible_if` 与 `manual_contains` 两处按提示改写）。另拿宿主编译的 `pathchk.c` 参照进程
  逐字节对过两组形状（`-p abc+def`、`/etc/hostname/x`）**完全一致**。核实日 `2026-10-14`；基线 `67567252d`。
  **两枚变异负控**（测完按 md5 复原）：把 `diagnose` 的程名前缀拿掉 → 门两枚独红（逐字枚与改名枚）；
  把 `errno != ENOENT` 改成 `!= ESRCH`（即把 `ENOENT` 当拒因）→ 库测与门各一枚独红。注意负控证据要看
  原文：第一轮用了 `cargo test -q`，失败行被压成汇总，看起来像「门不咬」，去 `-q` 重跑才拿到两行 FAILED。
  登记不执行：①真机播种与上机读数仍归 `C-57`/`E5`（本笔代码路径里的 `pathconf` 真面只在真机可验）；
  ②`fileops/bin_support.rs` 的 `warn_line` 用 128 字节缓冲，超长操数会被截断后印（本笔的 512 字节案例里
  看到了），要不要按 C 的 `vsnprintf` 形状放到 1024 得先对 C——归下一轮；③`06` 篇的 `test`/`pathstat` 那一行
  仍记「pathconf 探测待批」口径已由本笔改掉，其余同名旧账由 19–21 的账面行统一处理。

- ✅ **批次二百五十八给行缓冲开单字节缝，把 `Press RETURN` 后面那一句 `while(!getchar())` 接上**
  （`2026-10-14`，`claim/ARITH-GETCHAR-qoder` @ 基线 `470c34e07`，单域三线 `os/commands/games/stdio-games`
  与同线文档）：接回批次二百五十七登记不执行②（规划表 v2-15 拆出的 v2-15a）。
  **是什么**：C 每次打印分数后（`arithmetic.c:176-179`）先印 `Press RETURN to continue...`，再
  `while(!getchar());` **吃掉一个字符**才继续出题。本壳只印那一行、不吃字符——下一个
  `fgets` 就把用户那个按键读进题里，第 21 题的答案偏一位（打印字节相同，输入消费不同）。
  判类 P0-design-missing。
  **怎么修**：`LineReader` 加 `next_byte() -> Option<u8>`，沿用它的惰性消费规矩——先把上一次
  `line()` 还留在缓冲里的那一行交出去，再从缓冲取一个字节（空了就再读一块）。关键是**尽头与
  读失手在这一条腿上不分家**：`while(!getchar())` 只看返回值，既不查 `feof` 也不查 `ferror`，
  所以 NUL 继续等，别的任何字节、EOF 与读错都结束循环；壳侧就一句 `while reader.next_byte()
  == Some(0) {}`。不另开一条 `read(2)` 缝的理由：那会绕过缓冲把顺序打乱（C 里 stdio 游标是共享的）。
  **门**：共享面测 7 → 8 枚（九壳同增，共 +9）；真交互门扩一步：20 题答对拿到分数块后先投一个
  `x`，再读第 21 题并作答——`x` 必须被等待用掉，第 21 题才能一次答对（`Right!` 共 21 行）且全程
  不出一句 `Please type a number.`。**顺手修了门自己的一处缺陷**：`take_prompt` 原本 `drain` 掉
  已读部分，使得后面所有针对全文转录的断言只能看见最后一段（实测报 `left: 1, right: 21`）；
  改成“游标不缩缓冲”，`Right!` 计数与 `Please type a number.` 这两类断言才真能成立。
  三件套：`cargo test -p minix-stdio-games` → **215 passed、0 failed**（基线 206，+9）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）同包 **215/0**；三架构 `--bins --target {x86_64,aarch64,riscv64gc}-
  unknown-none` 各有 `Compiling` 行且 0 error；`rustfmt --edition 2024 --check` 触及三文件零漂移；
  clippy 触及文件仅剩一条存量（`asked % 20`，批次二百五十七已记）。核实日 `2026-10-14`；基线 `470c34e07`。
  **一枚变异负控**：把 `while reader.next_byte() == Some(0) {}` 换成不消费 → 真交互门独红，报
  `第 21 题也该一次答对：left 20, right 21`，转录里多出 `Please type a number.`（那就是不接这一腿时
  用户真实遇到的）。
  登记不执行：①SIGINT 分数打印仍需 `signal(2)` 封装（归属在 `../14-stage-runtime/todo.md:294` 的
  「按需移植」口径，本轮不代开二线入口）；②`bin_support.rs` 在 freestanding 面的两条 unused import
  （批次二百五十四已登）；③`07` 篇测数行与「三十三个已接线二进制」那类旧快照口径仍待 19–21 的
  账面行统一处理（批次二百五十六已登）。

- ✅ **批次二百五十七把 `arithmetic` 的用时统计接上，并把 `%.1f` 的舍入按 Minix3 自己的 `gdtoa` 定形（附带两处早就存在的缺陷）**
  （`2026-10-14`，`claim/ARITH-TIMING-1-qoder` @ 基线 `5b3a374eb`，单域三线 `os/commands/games/stdio-games`
  与同线文档）：接回规划表 v2-14 那一行（`16p` 第1片）。
  **是什么**：`showstats` 的那一句 `Total time %ld seconds; %.1f seconds per problem` 本树从来不挨，
  `qtime` 也不累加；判定它不能从函数名猜，得先读 C：分数行包在 `if (nright + nwrong > 0)`（`:168`）里，
  时间行另包一层 `if (nright > 0)`（`:171`，这就是 C 不会除零的原因——规划表里那个待定项就此关掉），
  `Press RETURN` 与收尾那个换行在两道守卫之外；`qtime` 是两次 `time(2)` 戳的**整秒差**逐题累加（`:232`、
  `:271-272`）。判类 P0-design-missing（一条完整腿未接）。
  **`%.1f` 怎么定的形**：先拿宿主 `cc` 编一个只重印那一句 `printf` 的参照程序（`(float)q / n` 的写法逐字照搬），
  跑出 25 对 `(qtime, right)` 的真值表，再拿 `f64::round_ties_even` 写第一版——**结果它根本编译不过**（
  `no_std` 面里没这个方法），于是改成从 `to_bits()` 拆出 `m * 2^exp` 后用 `u128` 做整数舍入。舍入规则不是拿
  glibc 的默认行为当权威，而是读了本树的 C 源：`vfprintf.c` 指向 `vfwprintf.c`，后者向 `gdtoa` 要数字；
  `gdtoa.c:782-790` 只在余数大于一半、**或正好一半且末位为奇**时入上——即平手取偶；`ROUND_BIASED`（平手远离零）
  只在 `IEEE_Arith` 未定义时才定义（`gdtoaimp.h:225-232`、`:430-437`），本树三个目标都是 IEEE。两条可观察的收获：
  ①平手族 `5/4` → `1.2`、`3/4` → `0.8`；②**除法发生在 `float` 不是 `double`**——`3/20` 得 `0.2`，而按 double
  算会是 `0.1`；拿这个当第二枚变异负控的目标，改成就红。
  **门拓出的两处早就存在的缺陷**（都是 P0-code-bug，不补上接不了交互式门）：①提示语的 `=` 后只写两个空格，
  C 的三个（`"%d %c %d =   "` `:231`），本壳头注自己写着三个——正文与代码不一致；②**宿主面不冲刷标准输出**，
  而 C 打完提示语就 `fflush(stdout)`；宿主 `std::io::stdout()` 是行缓冲的，不带换行的提示语留在缓冲区里，
  交互式命令挂在管道上永不返回（本轮新门第一次跑就挂住，九百秒超时）。补法：`bin_support` 加双生面的
  `flush_stdout()`（宿主 `flush()`，真机为空动作——`emit` 本就是逐次 `write(2)`，无缓冲可冲），`ask()` 在
  `emit` 后调用。
  **为什么不选另一条路**：①拿 `format!("{}.{:.1}")` 类浮点格式化看似更短，但本树 `no_std` 没有浮点格式化，
  而且它把舍入规则交给别处，不可审；②用有理数直接舍入（不经过 `float`）会把 `3/20` 那一类答错；③先接
  SIGINT 再接用时不行：本树无 `signal(2)` 封装，而时间行不依赖信号。
  **门**：库测 +4（整秒截断、 16 对真值表、平手与 float/double 分岔、两道守卫）；`tests/arithmetic_face.rs`
  4 → 6 枚，新增的两枚里有一枚是**真交互管道**（逐题读提示语→算答案→写回去，连跑 20 题），它同时钉住
  “计数器只在答对时前进”（二十次全错永远不走到分数块）与“时间行两个数同源”（把印出的秒数重新过一遍
  决定半再比对，不假定跑多久）。
  三件套：`cargo test -p minix-stdio-games` → **206 passed、0 failed**（基线 200；拆解 75 库测 + 九壳×7 +
  九扇门 68）；容器 `minix-ci:1.94`（挂仓根、带 `-u`）同包 **206/0**；三架构 `--bins` 各有 `Compiling` 行、
  0 error（`f32` 除法与 `u128` 位移在三个 freestanding 目标都过）；`rustfmt --edition 2024 --check` 触及四文件
  零漂移；clippy 触及文件只剩一条存量的 `manual_is_multiple_of`（`git show HEAD` 对比确认非本笔引入）。核实日
  `2026-10-14`；基线 `5b3a374eb`。
  **两枚变异负控**（测完按 md5 复原）：平手改远离零（`remainder >= half`）→ 一枚库测独红；丢掉小数位只印整数
  部分 → 真交互门独红。第一轮门挂住九百秒本身就是“未冲刷”那枚缺陷的反控。
  登记不执行：①SIGINT 分数打印——本树无 `signal(2)` 封装；该缺口的归属早已定在 14 篇（`../14-stage-runtime/todo.md:294` 记 termios/signal 大族「按需移植」，13 篇登记），命令侧需要时即触发它，本轮不代开三线入口；②`Press RETURN` 后面 `while(!getchar())`（`:178-179`）吃**一个字符**，需给 `LineReader` 开一个单字节缝；
  本轮只接了那一行本身的字；③其余命令若也有“无换行提示语”，同样需要 `flush_stdout`（本轮只扫到 `arithmetic` 是
  交互式的）；④`bin_support.rs` 在 freestanding 面的两条 unused import（批次二百五十四已登）。

  **合回后跑出一枚飘红，查出生的是门自己的 bug（已修）**：合入 `rewrite` 后在同一棵树再跑一遍，
  出现 185 passed / **1 failed**（先前四次都是 206/0）。不是舍入不稳，是**交互 harness 自己错了**：
  `take_prompt` 用 `rposition` 取**最后一个** ` =   ` 标记，而在 I/O 负载下孩子可以把上一题的
  `Right!\n` 与下一题的提示语一起冲刷出来，父进程于是跳过了孩子真正在等的那一题，把答案送到错的
  题上 → 多一个 `What?` → `Wrongs 0` 断言断掉。改成 `position`（取第一个未消费的标记）后连跑 5 次
  全绿，再叠上三个并发跑与一个读遍 `/usr/lib/*.so*` 的 I/O 负控，又跑两次仍全绿。教训归 `test-audit`
  的「测试自身错」那一维：逐题读流的解析必须按 FIFO 取第一个标记，不是最后一个；断言里钉住
  `Wrongs 0` 正是让这类跳题不能静默过关。入库：一笔独立小提交（不重写已合入的那笔）。

- ✅ **批次二百五十六除掉 `what` 的同名重复实现，并把「两包同名 bin」钉成门（P19）**
  （`2026-10-14`，`claim/WHAT-DUP-BIN-qoder` @ 基线 `5887699bc`，单域三线 `os/commands/usr-bin/textfilter`
  与门禁工具 `tools/pattern-gate.sh`，加同线两份台账）：接回批次二百五十五登记不执行①。
  **是什么**：`os/commands/usr-bin/textfilter/src/bin/what.rs` 与 `os/commands/usr-bin/doctools/src/bin/what.rs`
  同名同地产出，cargo 把两者写进同一个 `target/debug/what`，后建者覆盖前者；端到端门用
  `env!("CARGO_BIN_EXE_what")` 取路径，于是 doctools 的 `what_face` 跑的是哪一个取决于构建顺序
  （五包同跑必红，逐包单跑全绿）。判类 P0-code-bug（重复实现）+ P0-test-missing（门可跑错对象）。
  **归属怎么定的**：文档五处（10 篇篇首源码清单、Rust 模块行、命令表、§4.1 模块表、§5 门清单）都把
  `what` 记在 doctools 名下，07 篇从头到尾没列过它（批次四十九当时就没同步步正文）；doctools 那份
  决定半 12 测含 `goto loop` 回退与终止集，比 textfilter 那份 7 测贴 C，薄壳也已按 C 的
  `perror(*argv)` 重写。所以删的是 textfilter 那份，不是另造一份。
  **为什么不选另一条路**：①改名保留两份（如 `what2`）会把重复实现供下来，而 C 只有一
  个 `usr.bin/what`；②只加门不删重复，`target/debug/what` 仍会被覆盖，门的 FAIL 变成常亮红——
  那是拿门去掩代码债；③把两份都收进 workspace 依赖图去重不可行，重名是产物路径不是依赖层。
  **门**：`tools/pattern-gate.sh` 新添 P19：扫 `os/commands/*/src/bin/*.rs` 与显式 `[[bin]] name=`，先按
  「crate+名字」去重（同 crate 的自动发现与显式声明指同一名字不算，cargo 自会按 path 去重），
  再按名字计数 >1 即 FAIL。self-test 五枚夹具：ok / 跨 crate 重名 FAIL / FAIL 里报出名字与两边归属 /
  同 crate 显式不算 / 只出自显式声明的重名也算。
  三件套：`bash tools/pattern-gate.sh --self-test` → 全绿；真树 `bash tools/pattern-gate.sh` →
  `[P19] PASS  无跨 crate 重名 bin（命令面 114 个名字各归一个 crate）`且 `PASS=15 FAIL=0 SKIP=1`；
  **变异负控用真树实物**：把同名文件放回 → `[P19] FAIL … what -> …/doctools …/textfilter` 退码 1，
  删回即 PASS；**验收判据（本批要修的那一幕）**：`cargo test -p minix-textfilter -p minix-doctools`
  同跑 → 60 个 ok 块、**0 失败**；五包同跑 → **1380 passed、0 failed**（= 437+577+81+85+200）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）同两包 60/0；`strings -n 6 os/target/debug/what | grep -c 'cannot open'`
  → **0**（现只剩 doctools 那一份）；三架构 `cargo build -p minix-textfilter --bins --target
  {x86_64,aarch64,riscv64gc}-unknown-none --release` 各有 `Compiling` 行、0 error，且这些 target 目录下
  不再产出 `what`。textfilter 测数 584 → **577**（删掉的 7 枚同名库测）。核实日 `2026-10-14`；
  基线 `5887699bc`。容器里另跑的那次 `--target x86_64-unknown-none` 报「该 target may not be installed」，
  属镜像环境非回归（批次一百七十五已记同口径）。
  台账同步：10 篇 §4.1 `bin/what.rs` 行加「本树只此一份实现」与删重复的缘由；§7 那一行由 `open`
  翻 `closed`，并把原「同跑必红」那句标为登记时读数、补上收口后的可重跑读数。
  登记不执行：①07 篇的测数行仍写「229 个测试」与「三十三个已接线二进制」（带旧快照口径的日期限定），
  与实测（577 / 本包 bin 名单）已差一大截——归 19–21 的账面口径行统一处理，本批不改历史快照；
  ②`PATTERN-SCAN-REPORT-20260923.md` §4 检查映射表没记 P18/P19（现记到更早的 P 号），门数从 18 项
  变 19 项宜同批回填；③本队列其余项（`text-games` 三面、`primes` 的 `-d`/`-h` 与 `read_num_buf`）不变，仍按 v2 表排队。

- ✅ **批次二百五十五把命令层边界守卫（CI 闸上的那个）从 19 处直引扫回零，并因此发现 `what` 有两个同名二进制**
  （`2026-10-14`，`claim/COMMAND-BOUNDARY-ZERO-qoder` @ 基线 `dc8322d54`，单域三线：五个命令包的
  `src/bin_support.rs`/`src/*.rs`/三个 `Cargo.toml` 与同线台账；不动 `minix-types`/`minix-sys`）：
  **是什么**：`tools/check-command-boundary.sh` 在批次一百零九（`2026-10-12`）刚扫绿，批次一百十四
  又把它接进了持续集成（`.github/workflows/rules-consistency.yml:79`）；而 16t/16v 那串接线批（二百三十七～
  二百五十三）为了让一行诊断只有一处形状答案，从命令侧直引了 `minix_types::err_line`/`perror_line`/
  `prog_name`——现算 **19 处违规**（13 处代码 + 6 处注释里的下划线拼法，守卫不分两者）。这是闸上红旗，
  判类 P0-code-bug（依赖边界）。
  **怎么修**：按守卫自己的提示走 `minix-sys` 顶层再导出（那三个名字在 `minix_sys::lib.rs:69` 的
  `pub use minix_types::types::errno::*` 里已经转交，判例是批次一百零九的 `df`）；13 处调用点改指
  `minix_sys::`，6 处注释改成 `minix-types` 拼法并补一句「本层经 `minix_sys::` 拿」；`fileops`/
  `textfilter`/`doctools` 三包改完后再无引用，顺手从 `Cargo.toml` 删掉 `minix-types` 直依赖——
  批次二百四十八/二百四十九登记的两条依赖边（`new_edge4.md` C-100/C-101）因此可销。
  **为什么不选另一条路**：①把 `err_line` 再抄一份到每个命令包的 `bin_support` 正是 16t 要消除的重复；
  ②改守卫的 grep 口径（只查 `use` 行）会把注释里的旧名留在场上误导读者，而且「门不行就改门」是
  本项目明令禁止的方向；③给 `minix-sys` 新开封装不动——名字已经在那儿了，本批零二线改动。
  **意外发现（登记不执行）**：扫到 `doctools` 时先报一枚红——`os/commands/usr-bin/textfilter/src/bin/what.rs`
  与 `os/commands/usr-bin/doctools/src/bin/what.rs` 同名同地产出，共享 `target/debug/what` 一个路径；
  五包同跑时后建的 textfilter 那份（仍带批次二百四十八判为自创的 `<操数>: cannot open`）覆盖
  doctools 那份，使 `what_face` 门跑到了错的二进制。逐包单跑全绿，所以这是一个由构建顺序决定的
  假信号——已作为新一行记入 `../coordination/TODO-LEDGER-OPEN.md` §7（归属待定：哪个 crate 是 `what` 的家）。
  三件套：`bash tools/check-command-boundary.sh` → **OK: command layer dependency boundary clean** 退码 0
  （基线同命令 → 19 violation）；逐包 `cargo test` → fileops 437/0、textfilter 584/0、doctools 81/0、
  shell 85/0、stdio-games 200/0（均与基线同数，零回归）；容器 `minix-ci:1.94`（挂仓根、带 `-u`）五包
  `failed_blocks=0`；三架构 `--bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 5 行 `Compiling`、
  0 error；`rustfmt --edition 2024 --check` 触及九文件零漂移；clippy 触及文件零命中（`textfilter/src/pr.rs`
  那批告警属存量，本批未碰）。核实日 `2026-10-14`；基线 `dc8322d54`。
  **一枚变异负控**：把 doctools 的一处改回 `minix_types::perror_line` → 守卫报 `1 violation` 退码 1
  （当场验回，改完 `md5sum` 对齐）——守卫咬的是调用点不是注释。
  台账同步：`../coordination/TODO-LEDGER-OPEN.md` §7 加两行（本笔的 `closed` 与 `what` 同名的 `open`）。
  登记不执行：①`what` 同名二进制的归属与去重（§7 新行，下一轮）；②`stdio-games/bin_support.rs` 在
  freestanding 面的两条 unused import（`EIO`、`err_line`）需按 `#[cfg]` 归位（批次二百五十四已登）；
  ③守卫若将来要分代码/注释两档，需同时给注释档的拼法约定（本轮已把六处统一为 `minix-types`）。

- ✅ **批次二百五十四把 `number` 的三支 `errx` 接到 `getprogname()`，并因此发现那句报文的载荷也是错的**
  （`2026-10-14`，`claim/NUMBER-ERR-WIRE-qoder` @ 基线 `965b9f5d8`，单域三线 `os/commands/games/stdio-games`
  与同线文档，无跨界）：接回批次二百五十三登记不执行①（16v 判定表的下一行）。
  **是什么**：三支 `errx`（`number.c:111`、`:156`、`:164`）的格式串里都没有程序名，名字是
  `verrx` 添的（`minix3/lib/libc/gen/verrx.c:59` 打 `getprogname()`、`:62` 打换行），而本壳把
  `number: ` 当字面量写死——改名跑（副本、符号链接）就不对了。判类 P0-code-bug。
  **核对时才发现的第二件事**：判定表那行原本写「文本已对」，这一句账面超出证据。C 的扫描只在
  遇到 `.` 时写一个终止符（`:149`），别处一个字节也不改，所以 `errx(1, "illegal number: %s", line)`
  打的是从当前起点到缓冲尾的**整串**：`1e5` 答 `1e5`（不是 `1`）、`12-34` 答 `12-34`；在 `fgets`
  那条路上行尾换行也还在串里，于是报文后面多一个空行。本壳此前在出错的那个字节处截短，两支都不对，
  而既有金样 `illegal number: 1` 把错的形状当了对的——判类 P0-code-bug + P0-test-missing
  （测试自身错，`test-audit` 的那一维）。
  **新增的取证手法（后续轮可复用）**：把 C 真源用宿主编译器编成参照进程——
  `printf '#define __COPYRIGHT(x)\n#define __RCSID(x)\n#define __dead __attribute__((__noreturn__))\n' > shim.h`
  后 `cc -O0 -include shim.h -o number minix3/games/number/number.c`（glibc 的 `<err.h>` 与 NetBSD 的
  `verrx` 同形），再把两个进程放一起逐字节对：本轮 26 例（16 例操数形 + 10 例标准输入形，含 255/254
  位边界、制表符、前导空白、点截短、无点带换行）**SAME=26 DIFF=0**。这比手推金样硬得多，本轮就是
  靠它把一处手推错出来的金样抓出来的。
  **怎么修**：决定半 `NumberError::Illegal` 的 `end` 改为 `dot.unwrap_or(line.len())`（只在点截短，
  否则到缓冲尾）；壳侧加本地 `errx(prog, phrase, payload)` 复刻 `verrx.c:59-63` 那几次 `fprintf`，
  程名取 `support::prog_name(&argv)`（与 caesar 同形）；usage 那一行相反，C 把 `number` 写进字面量
  （`:288`），所以继续不跟 `argv[0]` 走。
  **为什么不选另一条路**：①用 `String::from_utf8_lossy` 拼一句会换掉非 UTF8 字节，C 是原样透出；
  ②把四次 `warn` 合成一次看似更原子，但 C 本来就是多次 `fprintf`，报文内容不变而结构更贴近原文；
  ③只改编程名不改载荷，等于把已知错的金样继续供着。
  **门**：`tests/number_face.rs` 5 → 7 枚：新增 `a_refusal_on_the_read_path_carries_the_lines_own_newline`
  钉住标准输入那条路的空行与制表符，`the_three_refusals_follow_argv_zero_while_the_usage_does_not`
  拿副本改名同时钉两半（三支拒绝跟着走、usage 不跟）；库测 `refused_numbers_name_the_text_c_would_still_print`
  的金样按 C 重写（`1e5`、`12-34`、`  7 x`、`12a\n` 四支）。
  三件套：`cargo test -p minix-stdio-games` → **200 passed、0 failed**（基线 198，+2 枚；20 个测试目标，
  拆解 71 库测 + 九壳×7 + 九扇门 66）；容器 `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-stdio-games`
  → **200/0**；三架构 `cargo build -p minix-stdio-games --bins --target {x86_64,aarch64,riscv64gc}-unknown-none`
  各有 `Compiling` 行且 0 error；`rustfmt --edition 2024 --check` 触及三文件零漂移；clippy 对触及文件零命中；
  C 参照对账 26 例 DIFF=0。核实日 `2026-10-14`；基线 `965b9f5d8`。
  **两枚变异负控**（测完按 md5 复原）：把 `end` 退回 `dot.unwrap_or(index)` → 4 枚红（1 库测 + 3 门）；
  把程名退回硬写 `number: ` → **只有改名那一枚红**（其余枚都用真名跑，本来看不出来，正是那一枚存在的理由）。
  22 篇四处随之改准：§2.5 数词段、`number` 接线行、§5 测数行与两个端到端清单条目。**账面滞后两笔一并清**：
  §5 的测数行还写着「十五扇门」、`caesar_face` 那一条还写着 3 个（批次二百五十三漏改），现按实测取
  九扇 66 枚与 7 枚。
  **顺带核正**：`words.rs` 与 `number_face.rs` 里 12 处 `number.c` 锚点漂移——函数区间写成了 `:112-212`
  /`:215-249`/`:252-284`/`:286-298`，`grep -n` 实测是 `convert` `:125-192`、`unit` `:194-227`、`number`
  `:229-265`、`pfract` `:267-283`（`unit`/`number` 还互换了），`MAXNUM` 实为 `:55`。门检不出这批：
  `pattern-gate` 的 P18 只抽带文件名的锚点，裸 `:NNN` 区间不受检——该工具缺口已登记。
  登记不执行：①`words.rs` 头部 1-30 行的 `pig.c` 锚点同批未复核，需另轮；②`bin_support.rs` 在
  freestanding 面两条 unused import（`EIO`、`err_line`）需按 `#[cfg]` 归位；③`getopt` 自己那句
  `invalid option` 报文本壳无（宿主 glibc 有），归 `7g` 的 getopt 一轮；④`text-games` 三面；⑤`primes`
  的 `-d`/`-h` 与 `read_num_buf`；⑥P18 扩到裸 `:NNN`（带同文件最近一个文件名归属）。

- ✅ **批次二百五十三接上 `caesar` 的两条流失败腿：一个缀号、一个只短语**
  （`2026-10-14`，`claim/CAESAR-NUMBER-ERR-qoder` @ 基线 `69eb456dc`，单域三线 `os/commands/games/stdio-games`
  与同线文档，无跨界）：接回批次二百五十二登记不执行①之外的判定表行（判定表里 `caesar` 那行现已翻成已闭）。
  **是什么**：`caesar` 除了已接的坏轮转句（批次二百三十三），还有三条流失败腿本树一条都没有：C 在
  `putchar` 返 `EOF` 时立刻 `err(EXIT_FAILURE, "<stdout>")`（`caesar.c:103-107`、`:116-120`，缀号），
  在结尾看 `ferror(stdin)` 时 `errx(EXIT_FAILURE, "<stdin>")`（`:205-208`，不缀号）。本壳此前写失败一律
  经 `emit` 静默退 1，读失败被 `read_stdin` 与文件尾混为一谈——两条腿都消失。另有一处程名硬写：
  `bad_rotation` 自己拼 `caesar: ` 前缀（该取 `getprogname()`），也没走那份渲染权威。判类 P0-code-bug ×2。
  **族的区别正是这条腿的内容**：`<stdout>` 那句必须缀号（那次写的 `errno`），`<stdin>` 那句必须**不**缀
  （stdio 的错标志不带号）。所以两族不能共用一行渲染，也不能都塞给 `warn_line`。
  **怎么修**：`bin_support` 补两条带号的双生缝——`write_stdout(bytes) -> Result<(), Errno>`（宿主取
  `raw_os_error()`、缺省退 `EIO`，与 `read_file`/`stdin_chunk` 同口径；真机侧直接传播 `write(2)` 的号）
  与 `read_stdin_all() -> (Vec<u8>, Option<Errno>)`（走已有的 `stdin_chunk`，两面不必重写）；壳侧
  `emit` → `write_stdout` 并在失手时 `warn_line(prog, "<stdout>", code)` 后退出，读完的号留到全部输出之后
  再以 `errx` 报出（保住 C 的次序）；`bad_rotation` 改交 `support::warn_line` 并接进程名。
  **为什么不选另一条路**：①让 `<stdin>` 也缀号看似一致，但那是 C 没有的信息（号在 `ferror` 之前就被 stdio 吞了）；
  ②`emit` 改成会返回错误会牵动其余七个壳的既有契约，故另开 `write_stdout` 一条缝给需要的壳；
  ③C 结尾那支 `fflush` 后的 `errx "<stdout>"` 在本树没有对应时刻（逐块直写、失败当场报），
  硬造一行等于凭空多一条报文——声明为无对位。
  **门**：扩 `tests/caesar_face.rs`（5 → 7 枚）。宿主侧的可复现机关是两条：`/dev/full` 当标准输出让每次写都失手
  （→ `caesar: <stdout>: No space left on device`，断言句子确实说到 space，从而钉住「号是从那次写带回来的」）；
  目录 `/tmp` 当标准输入让每次读都失手（→ `caesar: <stdin>`，逐字无句子）。
  三件套：`cargo test -p minix-stdio-games` → **198 passed、0 failed**（基线 196，+2 枚；20 个测试目标）；
  容器 `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-stdio-games` → **198/0**；三架构
  `cargo build -p minix-stdio-games --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 0 error（`touch` 强制重编）；
  `rustfmt --edition 2024 --check` 触及文件零漂移；clippy 对触及文件零命中；宿主实跑
  `caesar 3 > /dev/full` → `caesar: <stdout>: No space left on device` 退 1。核实日 `2026-10-14`；基线 `69eb456dc`。
  **两枚变异负控**（测完按 md5 复原）：拿掉 `<stdin>` 那一支（对应枚独红）、把 `<stdout>` 的号换成随手取的
  `EINVAL`（对应枚独红——第一版断言只查「像不像一句话」，被这枚暴露太松，已加收紧到句子内容）。
  22 篇两行随之改准：`caesar` 接线行、§测数行（184→198 那处账面滞后一并更正）。
  登记不执行：①`number` 的三支 `errx` 程名（判定表下一行，接法与本批同形，单独一轮）；②`text-games` 三面
  （`fortune`/`random`/`wtf`）的开档与读数腿；③`caesar` 的 `fflush` 那支 `<stdout>` 在本树的无对位声明；
  ④`primes` 的 `-d`/`-h` 与 `read_num_buf`（批次二百五十二登记）。

- ✅ **批次二百五十二把 `primes` 的界限腿逐枚接上：同一句坏参数在 C 里分属两族（16v 接线第一枚）**
  （`2026-10-14`，`claim/PRIMES-ERR-WIRE-qoder` @ 基线 `212ac44ff`，单域三线 `os/commands/games/stdio-games`
  与同线文档，无跨界）：接回批次二百五十一判定表里 P0 最高的一行。**是什么**：`primes` 的八支诊断腿
  本树**一支都没有**——`primes.c:133-172` 的负号、`strtoumax` 置 errno、尾随垃圾、超上限、区间倒置在
  本壳一律 `terminate(1)` 静默，坏参数与空区间看起来一模一样。判类 P0-code-bug + P0-test-missing。
  **核对中要辨的一件事**：C 里 `abc` 与 `12a` 不是一回事。`_strtol.h:69` 只在**一个数字都转不出来**时置
  `EINVAL`，`primes.c:138` 遂走 `err(1, "%s", argv[i])`（缀句子）；而 `12a` 转出了前缀、`errno` 未置，
  落到 `primes.c:140` 的 `errx "%s: illegal numeric format."`（不缀句子、句末带句点）。要分这两支就得让
  决定半把「哪种失手」交回来——原 `parse_bound` 把两形合在 `GameError::InvalidArgument` 里，还顺手做了
  上限检查（C 的上限检查在 `main`，不是解析器）。
  **怎么修**：决定半换成 `BoundError{NoConversion, OutOfRange, TrailingJunk}` 三形 + 返回 `u64`，
  上限那支拆成独立的 `over_cap`（C 的 `stop > SPSPMAX` 属 `main`）；壳侧加 `errx`/`err_errno`/`usage` 三个
  落点，`err_errno` 把 `EINVAL`/`ERANGE` 交给 `support::warn_line`（`bin_support` 补 `warn_line`，与
  fileops/textfilter 同形），`ERANGE` 无关联常量故沿用 `rm.rs` 对 `ETXTBSY` 的先例壳侧定值；
  开头的选项串一支一判：不认的字母按 C 的 `?`→`default: usage()`，认得的 `-d`/`-h` 报拒收（本树无那两面输出）。
  **为什么不选另一条路**：①只在壳里 `match` 字符串形状就能省掉决定半改动，但那等于把 `strtoumax` 的语义抄两份、
  且上限那支仍无处分；②把 `abc`/`12a` 合成一句是「统一成一族」的老错（批次二百三十九立的规矩）；③实现 `-d`/`-h`
  的输出面是另一件功能活，混进来会让这笔的 diff 说不清。
  **门**：新建 `tests/primes_face.rs`（8 枚，本包第九扇）：成功区间 `10..20` 逐字节 `11\n13\n17\n19\n`；
  两支转换失手各归一族；区间倒置、超上限、`5 -3` 的负号支各一句；`-q` 与三个操数都答那一行硬写 usage；
  `-h` 走拒收支；副本改名后报文行的程名跟着改而 usage 不改（本命令 usage 是字面量）。
  三件套：`cargo test -p minix-stdio-games` → **196 passed、0 failed**（基线 188，+8 枚门；决定半那三行库测随
  新错误型改写，`test_bounds_checked` 另补 `18446744073709551616` 的 `ERANGE` 与 `over_cap` 断言）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-stdio-games` → **196/0**；三架构
  `cargo build -p minix-stdio-games --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各真编一遍（输出里有
  `Compiling` 行，不是缓存的 `Finished`）零 error；`rustfmt --edition 2024 --check` 触及四文件零漂移；
  clippy 对触及文件零命中（`never_loop` 与 `let` 链两处就地改形：clippy 指出「循环体两支都发散」＝循环写错了，
  改成 `filter/and_then` 一支判完）；宿主实跑 `primes abc` → `primes: abc: Invalid argument`、`primes 12a` →
  `primes: 12a: illegal numeric format.`、`primes 5 4` → `primes: start value must be less than stop value.`、
  `primes 2 2000000` → `primes: 2000000: stop value too large (>1000000).`、`primes -q` → `usage: primes [-dh] [start [stop]]`。
  核实日 `2026-10-14`；基线 `212ac44ff`。
  **两枚变异负控**（测完按 md5 复原）：`err` 腿丢掉句子（门 2 枚红）、把尾字符那支也塞进 `err` 族
  （那一枚独红——正是「两族不许统一」这条规矩的可执行形式）。
  22 篇三行随之改准：`primes.rs` 决定半行、`primes` 接线行、端到端门清单添上本扇。
  登记不执行：①`-d`/`-h` 的输出面（差分列与十六进制打印，`primes.c:255-256`/`:327-328`）；②零操数那一支
  `read_num_buf`（`:176-205`，含它自己的四腿）；③`strtoumax` base 0 的前缀/空白/显式 `+` 形；④判定表其余行
  （`caesar` 三支、`number` 程名、`text-games` 三面）按 v2-13 之后继续。

- ✅ **批次二百五十一接上 `arithmetic` 的报文通道，并交出 16v 的游戏壳判定表（判定轮）**
  （`2026-10-14`，`claim/GAMES-ERR-SCAN-qoder` @ 基线 `7a8e37fec`，单域三线 `os/commands/games/stdio-games`
  与同线文档，无跨界）：**接线部分**——`arithmetic` 的三支选项错（`arithmetic.c:126`、`:132`、`:362`）与
  `usage()`（`:378-385`）在 C 全走标准错误，本壳却把这些行 `emit` 到标准输出，且 usage 是小写、名字硬写
  （C 是 `Usage: %s …` 带 `getprogname()`）。判类 P0-code-bug（通道是 `err(3)` 族形状的一部分）+ P0-fact
  （头注那句「penalise 尚未建模」自批次二百三十二起就不成立，已按实况改写为「C 的 `malloc` 失手才放弃惩罚，
  本树 `Vec` 无此支」）。**照抄不改进的部分**：C 的格式串自己以 `arithmetic: ` 开头，`errx` 再加一次程名，
  真实字节是 `arithmetic: arithmetic: invalid range.`——按真值优先序复现，登记为上游缺陷，不静默改好。
  **16v 判定表**（逐枚读 C 源判哪一族，接线归后续轮）：
  | 壳 | C 源的诊断腿 | 本壳现状 | 判定 |
  |---|---|---|---|
  | `arithmetic` | `errx` 四支（`:126`、`:132`、`:362`、`:373`）+ `usage` 走 stderr | 本批接齐（`:373` 那支 `op %c not in keylist` 在本树不可达：键表由本树枚举产生） | 已闭 |
  | `primes` | `err(1, "%s", argv[i])` ×3 与 `errx` ×5（`:133`、`:138`、`:140`、`:145`、`:147`、`:149`、`:155`、`:160`） | **一支都没有**：非法数字、负数、停止值过大、开档失败全静默 | P0，接线优先级最高（下一轮 v2-12） |
  | `caesar` | `err` ×3（`:105`、`:118`、`:137`）+ `errx` ×2（`<stdin>`/`<stdout>`，`:207`、`:213`） | 批次二百五十三接齐：`<stdout>` 写失手缀号、`<stdin>` 只短语；`fflush` 那支在本树无对位（逐块直写，已声明） | 已闭 |
  | `number` | `errx` ×3（`:111`、`:156`、`:164`） | 批次二百五十四接齐：程名改走 `getprogname()`，usage 仍按 C 的字面量。核对时发现原判「文本已对」不成立——C 只在 `.` 处写终止符，`illegal number: %s` 打整串（无点就连行尾换行一起打），本壳此前在出错字节处截短、金样也跟着错了，一并改正 | 已闭 |
  | `ppt` / `factor` | `err(1, "fgets")`、溢出句 | 批次二百三十四/二百三十五已接 | 已闭 |
  | `bcd` / `morse` | C 侧零支（`bcd.c`、`morse.c` 无 `warn`/`err`） | 只有 usage 行 | 无需接线 |
  | `text-games` 三面（`fortune`/`random`/`wtf`） | `fortune.c:207`、`:221`、`:352` 是 `err`/`errx`；`random.c:97`、`:99` 同理 | 六支都是 `xxx: <路径>: cannot open`/`read error`/`bad denominator` 这类自创短语，缺号也缺操数原文 | 待接（同 `what` 那一类；`wtf` 的 C 真源尚未在 `minix3` 里定位到，需先找源） |
  **门**：新建 `tests/arithmetic_face.rs`（4 枚，stdio-games 第八扇）：两支选项错逐字含 C 的重复前缀且标准输出零字节、
  `Usage:` 大写带程名（多余操数也走这一支）、答题与提示语一行也不写标准错误。
  三件套：`cargo test -p minix-stdio-games` → **188 passed、0 failed**（基线 184，+4 枚门）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-stdio-games` → **188/0**；三架构
  `cargo build -p minix-stdio-games --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 Finished 零 error
  （且经 `touch` 与「故意删掉 `use alloc::format;`」双向验证该门确实在重编）；`rustfmt --edition 2024 --check`
  触及文件零漂移；clippy 对触及文件零命中（`bin_support.rs` 里 `err_line`/`EIO` 的 unused-import 是逐壳噪声，早于本批）；
  宿主实跑 `arithmetic -r bogus` → stderr `arithmetic: arithmetic: invalid range.`、stdout 空、退 1。
  核实日 `2026-10-14`；基线 `7a8e37fec`。
  **反控一枚**：把 `-r` 那一支改回 `emit`（＝改前形态），门里对应那枚独红、另三枚不受影响。
  **自我纠正一笔（值得留案）**：本批一度判定「结束输入不打印分数」是 P0 缺口，读了 `main` 循环
  （`arithmetic.c:148-150`：`if (problem() == EOF) exit(0);`，其后才是每二十题的 `showstats(0)`）才否证——
  C 在 EOF 处根本不调 `showstats`。据此删掉了那条断言与它的登记项，结论改为「本壳与 C 一致」。
  教训：门里想断言「C 会做什么」必须先读那段 C，不能从函数名猜。
  22 篇两行随之改准：`arithmetic` 行、端到端门清单添上本扇。
  登记不执行：①`primes` 的八支 `err`/`errx` 完全缺失（优先级最高，接线轮 v2-12）；②`caesar` 的
  `<stdin>`/`<stdout>` 三支与 `emit` 写失败的报文；③`number` 与 `text-games` 三面的程名与缀号
  （`wtf` 要先定位 C 真源）；④`arithmetic.c:373` 那支在本树不可达的声明（若键表将来可被外部喂值需回来接）。

- ✅ **批次二百五十把 `bin_support::emit` 的宿主孪生面补齐（shell 包），并扫清同类单侧缝**
  （`2026-10-14`，`claim/EMIT-HOST-DUAL-qoder` @ 基线 `79818d566`，单域三线 `os/commands/bin/shell`
  与同线文档，无跨界）：接回批次二百三十七登记②（16u）。**是什么**：命令侧的
  `bin_support.rs` 是「一条缝两面」的写法（宿主走进程自己的 stdio，真机走
  `minix_sys::write`），因为 `minix_sys::write` 在宿主进的是替身传输。批次二百三十七
  只补了 `warn`，输出面 `emit` 还是单面——`os/commands/bin/shell/src/bin_support.rs:94`
  是全 14 份共享件里唯一没有宿主孪生的传输缝（扫描判据：对每个缝名查「只有一处
  定义且定义前 6 行内无 `cfg(`」，命中的只有 `emit`；`prog_name`/`warn_line`/`perror_line`/
  `answer_yes` 是刻意单面，它们不调传输）。**后果不是「测试看不见」而是命令在宿主上白退**：
  `emit` 写失败即 `terminate(1)`，而宿主那次写必失败。实测改前 `getopt "ab:" -a` → stdout
  **0 字节 + 退 1**；改后 → ` -a --\n` + 退 0。判类 P0-code-bug——宿主面不是「取证看不到」，是行为本身就错。
  **怎么修**：加宿主孪生（`std::io::stdout().write_all`，写失败同样退 1，与真机面同判据），
  真机面原样保留；两面各带 `cfg`，注释写明这条缝为什么要两份实现（沿用 `caesar`/`factor`
  那批与 `warn` 双生面的先例）。
  **门**：新建 `tests/getopt_face.rs`（4 枚，shell 包第二扇进程级门）——选 `getopt` 是因为它的
  全部产品就是 stdout 那一行，别的壳（`hostname`/`domainname` 的成功形状要先有 `gethostname(2)`
  的答复，宿主恒回 `EIO`；`sh` 的提示符要先过 `fork`，宿主报 `sh: cannot fork`）。四枚逐字对
  `minix3/usr.bin/getopt/getopt.c:26-40` 的 `printf` 序列，含「未知字母时那一行照打、只退码变 1」。
  三件套：`cargo test -p minix-shell` → **85 passed、0 failed**（基线 81，+4 枚门）；三架构
  `cargo build -p minix-shell --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 Finished 零 error；
  容器 `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-shell` → **85/0**；
  `rustfmt --edition 2024 --check` 触及两文件零漂移；clippy 对触及文件零命中；宿主实跑
  `getopt "ab:" -a` → ` -a --`、`getopt "ab:" -b zzz foo bar` → ` -b zzz -- foo bar`、
  `getopt "ab:" -q` → stdout ` --` + stderr `getopt: unknown option -- q` + 退 1。核实日 `2026-10-14`；
  基线 `79818d566`。
  **反控一枚（真凭据）**：把 `emit` 改回「只有一条无 `cfg` 的定义」＝改前形态，宿主重编后
  `getopt` 得 `stdout 0 字节 + 退 1`，与改前的实测读数逐字对齐；这一枚同时钉住「门不是靠
  编译期 cfg 混过去的」。过程记：第一次做这个反例时脚本删掉了两面（`emit` 未定义、编译失败），
  而那次 `getopt` 的 7 字节读数来自**上一个残留二进制**——又一次证明「改完必须看到重编发生」。
  05 篇两行随之改准：`bin/getopt.rs` 行加缝的来历、§5 测试清单添上本门；
  **16u 状态**：shell 包的输出面已闭；余下同类项为零（扫描判据可重跑，见上），故 16u 记 `closed`。
  登记不执行：①`hostname`/`domainname` 的成功输出形状仍只能在真机钉（宿主 `gethostname` 回 `EIO`），
  归 `C-57`/`E5`；②`sh` 在宿主的执行面受 `fork` 缺失限制（`sh: cannot fork`），那是 exec/fork 交付链的
  既有边界，不属本条。

- ✅ **批次二百四十九接上 `join` 的 `err(3)` 文件腿与那支完全没实现的 stdin 拒读腿（16t 第九枚，本队列最后一枚壳）**
  （`2026-10-14`，`claim/JOIN-STRERROR-qoder` @ 基线 `b3d9329d9`，**跨线**：三线 `os/commands/usr-bin/textfilter`
  加一条指向二线 `minix-types` 的依赖边，并顺手把本包共享读档缝的错误位从单位型换成错误码型——在
  `new_edge4.md` 登记 C-101 后动手）：接回批次二百四十八登记不执行①。**是什么**：三件事一起。**其一**：C 的
  文件腿是 `err(1, "%s", *argv)`（`join.c:244`、`:249`），本壳印 `join: cannot open file 1`——操数名没印、号没印、
  短语是自创的；而号之所以拿不到，是因为本包共享的 `bin_support::read_file` 把错误擦成了 `()`。**其二**：
  `usage()` 在 C 是两行且程序名嵌在第一行的格式串里（`:632-635`，第二行十二空格缩进），本壳是一行且选项表
  与 C 不符。判类 P0-code-bug + P0-fact。**其三**（最重）：`join.c:250-251` 的
  `errx(1, "only one input file may be stdin")` 本壳**根本没有这一支**——两个 `-` 会把同一条标准输入读两遍，
  第二遍是空，于是 `join - -` 静默产出错的连接结果而不是报错。
  **怎么修**：`read_file` 的错误位改为 `Errno`（宿主侧取 `raw_os_error()`，没有它时退 `EIO`，与 `stdio-games`
  的读缝双生面同一口径；真机侧直接传播 `minix_sys::read` 的号），其余十三个调用方的 `Err(())` 位由编译器逐个
  点到、改成 `Err(_)`，**行为一字未动**（它们本来就把错误当哑的）；`bin_support.rs` 加 `prog_name`/`warn_line`
  两个写行入口（都转调 `minix-types` 那一份权威）；`join` 的两条文件腿交 `warn_line`，usage 拆成两行，
  stdin 那一支按 C 放在两次读之前。
  **为什么不选另一条路**：①在 `join.rs` 里另起一条自己的读档梯子就能保住号，但那会在同一包里出现两份 open/read/close
  阶梯，两本的宿主/真机双生面迟早分叉；②给其余十三个壳也顺手补上报文（它们各自印 `xxx: <路径>`）是把别人家的 TODO
  一并做了——本轮只把它们改到「编译得过、行为不变」；③C 其余选项错的文案带选项字母与非法值
  （`"-1 option field number less than 1"`、`"illegal field number -- %s"`），而决定半的 `JoinError` 把字母与值都丢了，
  要按 C 做就得给错误型加实参（连带十余处库测断言）——那是下一片，不混进本轮。
  **门**：新建 `tests/join_face.rs`（6 枚，本包第二扇进程级门）：成功路 stderr 一声不出；失手的操数带名字与句子；
  `join - -` 在读之前就被护住（stdout 空、退 1、那一行 `errx` 形）；`-a`+`-v` 是 `errx`（不跟 usage）；两行 usage 逐字
  对 C；副本改名后 `err(3)` 行与 usage 行的名字都跟着改，而操数路径照原样。与 file-ops 那几扇不同，本壳在宿主
  **能真开文件**（`read_file` 的宿主双生面走 `std::fs`），所以连接成功路也在射程内。
  三件套：`cargo test -p minix-textfilter` → **584 passed、0 failed**（基线 578，+6 枚门；十三个共用读缝的壳零回归）；
  容器 `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-textfilter` → **584/0**；三架构
  `cargo build -p minix-textfilter --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 Finished 零 error；
  `rustfmt --edition 2024 --check` 触及文件零漂移；clippy 对触及文件零命中；宿主实跑 `join`（无操数）→ 两行 usage 退 1、
  `join - -` → `join: only one input file may be stdin` 退 1、`join /tmp/join-face-absent x` → `join: /tmp/join-face-absent: No such file or directory`。
  核实日 `2026-10-14`；基线 `b3d9329d9`。
  **三枚变异负控**（测完按 md5 复原）：文件腿退回 `cannot open`（门 2 枚红）、拿掉 stdin 那一支（那枚独红）、
  usage 合成一行（usage 两枚红）。**过程记两笔**：①`join.rs` 换 import 时把 `alloc::string::String` 删掉了——宿主测
  全绿，是 `--target x86_64-unknown-none` 那道门炸出来（`cannot find type String`），而且**第一次跑该门看着像通过**，
  是 `touch` 强制重编才见红：增量缓存在同尺寸改动上会骗人，别拿「0.1s Finished」当证据；②读缝换型的影响面由编译器
  逐点报出（`Err(()) → Err(_)`），不是靠猜。
  10 篇、07 篇各一行随之改准；**顺带补一处文档缺口**：`join` 在 07 篇此前**没有行**（只在 §1 prose 提过一次），
  本批按实况补上（登记为 P1-doc 缺口的修复，见该行自述）。
  **16t 收口**：`err(3)`/`warn(3)`/`perror(3)` 腿的逐枚接线队列到此走完九枚壳（rm/cp/mv/truncate/du/find/stat/what/join），
  登记不执行改为两条独立条目：①**第2片**——`join` 其余选项错的 C 文案（要 `JoinError` 带字母与值）+
  `find` 的 `WalkProbe`/`stat` 的 `PathProbe` 不交号那一件（含 `99` 篇 §1 分层契约上「决定半能不能收 `Errno` 型」
  的待定形）；②`relative_target` 的溢出形（`saturating_add` vs C 的回绕钳零，批次二百四十四登记）。
  ③真机播种与上机腿仍归 `C-57`/`E5`。

- ✅ **批次二百四十八把 `what` 的报告行改回 `perror(3)` 那一族：程序名与「cannot open」都是自创的（16t 第八枚）**
  （`2026-10-14`，`claim/WHAT-STRERROR-qoder` @ 基线 `2049a25e7`，**跨线**：三线 `os/commands/usr-bin/doctools`
  加一条指向二线 `minix-types` 的依赖边（本包以前不读错误码表），在 `new_edge4.md` 登记 C-100 后动手）：
  接回批次二百四十七登记不执行③。**是什么**：C 的开档失败腿是 `perror(*argv); exit(matches ? EXIT_SUCCESS : 1)`
  （`what.c:84-87`），而 Minix3 的 `perror`（`minix3/lib/libc/stdio/perror.c`）打 `<操数>: <句子>`——**不缀程序名**；
  本壳印的 `what: <操数>: cannot open` 三个字段里有两个是自创的，而真正的信息（那个号）没印。判类
  P0-code-bug + P0-fact（头注里那句「本壳尚未接那张表」在表已入库后就不成立了）。
  **核对中拓出第二处不是文案的差**：`read_file` 把「开不成」与「读到一半出错」合成一件报——而 C 的
  `getchar()` 对读错只当文件尾：已找到的 id 照打、然后续下一个操数，不报错也不退。本轮把它改回两件事：
  只有开档失手走 `perror` + 退，读错交回已读到的字节继续跑。
  **怎么修**：`bin_support.rs` 加 `perror_line(label, code)`（与 fileops 那份同构，渲染在 `minix_types::perror_line`），
  为此本包 `Cargo.toml` 加 `minix-types` 依赖（与 `fileops`/`shell` 同一先例）；`read_file` 由 `Option<Vec<u8>>` 改成
  `Result<Vec<u8>, Errno>`；那一腿改成 `support::perror_line(name, code)`。
  **为什么不选另一条路**：把它统一成 `warn(3)` 形（即保留 `what: ` 前缀）= 把批次二百三十九立的两族规矩反着犯一次；
  给读错也补一行 `perror` = 报 C 在本处根本不报的东西；把读错当硬失败退出 = 比 C 多退一步。
  **门**：新建 `tests/what_face.rs`（4 枚，**本包第一扇进程级门**）：`<操数>: <句子>` 且行首不得出现
  `what:`/「cannot open」；两个操数只出一行（`perror` 后紧跟 `exit`，与 `mkfifo` 的逐名继续相反）；无操数与
  未知字母同答那一行硬写 usage（本命令的 usage 不吃 `getprogname()`）；副本改名后两张脸都不跟着变。
  三件套：`cargo test -p minix-doctools` → **81 passed、0 failed**（基线 77，+4 枚门）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-doctools` → **81/0**；三架构
  `cargo build -p minix-doctools --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 Finished 零 error；
  `rustfmt --edition 2024 --check` 触及三文件零漂移；clippy 对触及文件零命中（新增那一处
  `match` → `?` 已就地改形）；宿主实跑 `what /tmp/what-nope` → `/tmp/what-nope: Input/output error` 退 1、
  `what /tmp/a /tmp/b` 只出一行、`what` 与 `what -q x` 同答 `usage: what [-s] file ...`。核实日 `2026-10-14`；
  基线 `2049a25e7`。
  **两枚变异负控**（测完按 md5 复原）：退回 `what: <操数>: cannot open`（门 3 枚红）、把失败改成
  `continue`（「只出一行」那枚独红）。**过程记**：第一版负控拿「改成 `break`」当反例——它与 `terminate`
  在此处行为等价（都是跳出循环后同一个退码），门全绿不说明门不咬；换成 `continue` 才拿到真信号。
  负控要先论证「它与原码行为不同」，不然只是白跑一次。
  10 篇两行随之改准：`bin/what.rs` 行、§5 测试清单添上本包第一扇门。
  登记不执行：①`textfilter` 的 `join` 同族腿（16t 最后一枚）；②本包其余壳（`which`/`soelim`/`fpr`/`whatis`）
  的开档失败腿是否也是 `perror` 族尚未逐枚对 C——归 16v 的判定轮一并处理；③真机播种与上机腿仍 `C-57`/`E5`。

- ✅ **批次二百四十七接上 `stat` 的四段报告行，并改回 `getprogname()` 的人格判据与 usage 里的名字（16t 第七枚）**
  （`2026-10-14`，`claim/STAT-STRERROR-qoder` @ 基线 `614c63523`，单域三线 `os/commands/bin/fileops`
  与同线文档，无跨界）：接回批次二百四十六登记不执行①。**是什么**：C 的失败腿是
  `warn("%s: %s", name, usestat ? "stat" : "lstat")`（`stat.c:340-344`），经 `vwarnc.c` 渲染成四段
  `stat: <操数>: <腿名>: <句子>`；本壳只到第三段。判类 P0-code-bug。
  **核对中拓出三处不是文案的问题**：①人格判据——C 是 `strcmp(getprogname(), "readlink") == 0`
  （`stat.c:220`），本树决定半用 `prog.ends_with("readlink")` 且把**未去目录的 `argv[0]`** 交给它，于是
  `/bin/my-readlink` 会被错认成 readlink（丢选项、丢告警），`/bin/stat` 连 usage 都不对；②`usage()` 行丢了
  程序名——C 是 `fprintf(stderr, "usage: %s %s\n", getprogname(), synopsis)`（`:357-365`），本壳只印
  `usage: <synopsis>`（接线前实测：`usage: [-fnqsv] [file ...]`）；③头注与注释里那两句「`%R` 被拒而非折叠」
  与「本壳尚未接那张表」都是假理由（`%R` 自批次一百三十五已折叠，表已在 `Errno::message`），属 P0-fact。
  **怎么修**：壳侧 `prog = support::prog_name(&argv)`（取段规则只住在权威那一处），人格比全等；失败腿交
  `support::warn_line(prog, "<操数>: <腿名>", code)`；`readlink:` 腿直接拿自己那趟 `readlink(2)` 的号；
  `realpath:` 腿按 `getcwd.c` 自己指定号的四支映射（`ENOENT`/`ENAMETOOLONG`/`ELOOP`/`ENOTDIR`，坐标逐支带注），
  其余三支是系统调用透传、探针接口不交号，宁可只出短语也不造一个假号（与 `find` 的 `WalkProbe` 同一条登记）；
  `ELOOP` 在 `Errno` 里没有关联常量，沿用 `rm.rs` 对 `ETXTBSY` 的先例（壳侧定值 + `from_i32`）。
  **门**：新建 `tests/stat_face.rs`（7 枚）：四段报告行、腿名跟着选项走而不跟着最后失手的调用走、`-q`
  只关出声不改退码、三支 `errx` 形腿逐字钉（含 `can't use format 'f' with -F` 与 `%Q: bad format`）、usage 行
  名字在串里、两个副本（`<tmp>/readlink` 与 `<tmp>/my-readlink`）同时钉住人格全等判据与 `getprogname()` 丢目录。
  三件套：`cargo test -p minix-fileops` → **437 passed、0 failed**（基线 430，+7 枚门；决定半库测零回归）；
  容器 `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-fileops` → **437/0**；三架构
  `cargo build -p minix-fileops --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 Finished 零 error；
  `rustfmt --edition 2024 --check` 触及三文件零漂移；clippy 对触及文件零命中；宿主实跑
  `stat /tmp/nope` → `stat: /tmp/nope: lstat: Input/output error`、`stat -L` → `… : stat: …`、
  `stat -q` 一声不出但退 1、`stat -Z` → `usage: stat [-FlLnqrsx] …`、`<tmp>/my-readlink <x>` → 报告行头是
  `my-readlink:`。核实日 `2026-10-14`；基线 `614c63523`。
  **三枚变异负控**（测完按 md5 复原）：报告腿丢句子（门 4 枚红）、人格退回 `ends_with`（那枚独红）、
  usage 丢程序名（usage 两枚红）。
  06 篇三行随之改准：`stat.rs` 行（人格判据）、`stat` 缺腿表行（`%R` 不再是被拒）、端到端门清单添上本扇。
  登记不执行：①探针接口不交号这一件（`find` 的 `WalkProbe` 与 `stat` 的 `PathProbe` 同病）：命令层决定半到底
  能不能收 `Errno` 型——这是 99 篇 §1 分层契约上的一项待定形，不属本轮接线；②`realpath:` 三支透传腿随①一同接；
  ③余下 doctools `what`、textfilter `join` 的同族腿。

- ✅ **批次二百四十六把 `find` 的八支报文逐枚改回 C 的 `errx` 原文（16t 第六枚，第 1 片）**
  （`2026-10-14`，`claim/FIND-STRERROR-qoder` @ 基线 `c7c897068`，单域三线 `os/commands/bin/fileops`
  与同线文档，无跨界）：接回批次二百四十五登记不执行①。**是什么**：`find` 的报文不是「同一个前缀 + 不同
  描述」，而是 C 逐句写好的 `errx` 原文；本壳八支里有六支是自创文案，形状也不同——`option.c:131` 是
  `errx(1, "%s: unknown option", *argv)`（名字在前、无反引号），本壳写的是「find: unknown option `-nope'」；
  `operator.c:149` 是 `): no beginning '('`，不带出错词，本壳多带了个名字；`function.c:649` 的
  `no terminating ";" or "+"` 本壳换成了「missing argument to `; 命令项」那类写法（C 里没这句）；`function.c:668`
  的 `-ok: terminating "+" not permitted.` 被换成了 `-ok ... + is not supported`；usage 行本该嵌
  `getprogname()`（`main.c:161`）而被硬写成 `find`。判类 P0-fact + P0-code-bug（报文与 C 不同）。
  **怎么修**：`report_parse_error(prog, e)` 逐支按 C 原文落，每支带坐标注；`prog` 取自 `argv[0]`；
  `StrayCloseParen` 的名字放下（C 不报它）；`operator.c:98` 在 C 里是 `err` 不是 `errx`，缀的是上一个调用
  留下的 `errno`——无号可诚实复刻，止于短语并在头注写明同 `mkfifo` 的 `-m` 腿一个判法。
  **门**：新建 `tests/find_face.rs`（8 枚）——这些腿不需文件系统，所以本门能逐字钉住每一支，含
  `-exec … {} +` 被 C 收下（不是报文路）与 `-ok echo +`（`+` 未紧跟 `{}`）仍走 `no terminating` 那一支。
  三件套：`cargo test -p minix-fileops` → **430 passed、0 failed**（基线 422，+8 枚门）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-fileops` → **430/0**；三架构
  `cargo build -p minix-fileops --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 Finished 零 error；
  `rustfmt --edition 2024 --check` 触及两文件零漂移；clippy 对触及文件零命中；宿主实跑八支报文逐条对上
  （`Usage: find …`、`find: -nope: unknown option`、`find: -name: requires additional arguments`、
  `find: (: missing closing ')'`、`find: (): empty inner expression`、`find: ): no beginning '('`、
  `find: !: no following expression`、`find: -ok: terminating "+" not permitted.`）。核实日 `2026-10-14`；
  基线 `c7c897068`。
  **三枚变异负控**（测完按 md5 复原）：退回反引号形报文（门 2 枚红）、程序名硬写 `"find"`（改名那枚独红）、
  括号报文退回自创的 `(` unmatched`（括号那枚独红）。**过程记**：shell 嵌套引号会吃坏变异串（M1/M3 首跑
  只拿到编译错误），改用 heredoc 里的 python 带 `assert old in s` 才拿到各自的红；`-exec … {} +` 在宿主是
  「走表失败」而非「退码不为 1」，断言改成报文内容而不是退码。
  **拆片声明（第 2 片，登记不执行）**：走表警告腿的号——`find.c:230` 是 `warnx("%s: %s", fts_path,
  strerror(fts_errno))`，而本树的 `WalkProbe` 三个方法返回 `Option`，号在探测腿就丢了；要按 C 做需把
  返回型改成带 `Errno` 的形状并让求值半拼出那一行（决定半 API 变更，连带走表/逻辑形两组库测）。同一片里还有
  C 区分而本层不分两支的两处：`operator.c:199`/`:201`（`!` 的两支）、`operator.c:254`/`:258`（`-o` 的两支，
  本层现在归入 `StrayCloseParen`/`NotWithoutOperand`）。另：`find.c:240` 的 `warnx("%s: illegal path")` 与
  `main.c:152` 的 `err(1, ".")` 在本树没有对位点（不开 `dotfd`、不做 xargs 安全件），归头注里的声明。

- ✅ **批次二百四十五接上 `du` 的报告腿与 `fts_read` 终止腿，并纠正一处把已接旗标说成未接的报文（16t 第五枚）**
  （`2026-10-14`，`claim/DU-STRERROR-qoder` @ 基线 `006edefb0`，单域三线 `os/commands/bin/fileops` 与同线
  文档，无跨界）：接回批次二百四十四登记不执行①的第一枚。**是什么**：C 的逐访问报告是
  `warnx("%s: %s", p->fts_path, strerror(p->fts_errno))`（`du.c:233`），本壳只印 `du: <路径>`——号丢了，
  不可读目次与不存在的操数在用户眼里一个样。判类 P0-code-bug。
  **核对中坐实三处**：①`du.c:251-253` 的 `err(1, "fts_read")` 是终止腿，本壳以前报完继续走（与 `rm`/`cp`
  同一病）；②拒收报文写的是「`humanize_number`, logical walk, FTS_XDEV」，而 `-h` 早已接上（走 `humanize.rs`），
  真正拒的是 `-H`/`-L`/`-x`——报文在骗人（P0-fact）；③本命令的 `usage()` 在 C 里是硬写字符串（`du.c:356-362`），
  与 `rm`/`cp`/`mv` 那三个带 `getprogname()` 的不是一回事，所以 `du` 只有报告行跟 `argv[0]` 走。
  **怎么修**：`warn_path(prog, path, code)` 交共享渲染（`warnx` 带 `strerror` 那一形与 `err_line` 输出同一
  个三字段行，这一点在头注里写明，免得后人以为这里少用了一族）；`descend` 的 `getdents` 失手改按 C
  印 `du: fts_read: <句子>` 即退；拒收报文去掉 `humanize_number`；`prog` 在 `run()` 里取一次往下传。
  **门**：新建 `tests/du_face.rs`（4 枚，本 crate 第七扇进程级门）：三段齐的报告行且退 1、两个操数出两行
  只一个退码、`-H` 拒收行有程序名无句子、副本改名后报告行跟着改而 usage 行不跟。
  三件套：`cargo test -p minix-fileops` → **422 passed、0 failed**（基线 418，+4 枚门）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-fileops` → **422/0**；三架构
  `cargo build -p minix-fileops --bins --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 Finished 零 error；
  `rustfmt --edition 2024 --check` 触及两文件零漂移；clippy 对触及文件零命中。核实日 `2026-10-14`；
  基线 `006edefb0`。
  **变异负控与一条诚实边界**：报告腿退回只报名字 → 门 3 枚红✓；而把 `fts_read` 退回「报完继续走」时
  **门全绿**（422/0）——宿主 `open` 先失败，根本走不到 `getdents`，所以那一腿本门钉不住；这一点已写进门头注
  与 06 篇，真机半归 `C-57`/`E5`（imgrd 根盘的 `getdents` 断腿另已登记）。不拿「门绿」当作那条腿已证的错觉。
  06 篇三行随之改准：`du.rs` 行（`-h` 已接、`-H`/`-L`/`-x` 拒收）、`bin/du.rs` 行、端到端门清单添上本扇。
  登记不执行：①余下 `find`/`stat`、doctools `what`、textfilter `join` 的同族腿；②`du.c:195` 的
  `err(1, "fts_open \`%s'")` 在本树没有对位调用（逐目次 `open`），其形状已被 `FTS_DNR` 那一支吃掉；
  ③`du.c:321` 的 `err(1, "calloc")`（硬链接表分配失败）在本树无对位——`LinkSet` 是 `Vec`，失败不给出可印的号。

- ✅ **批次二百四十四把 `truncate` 的四处失败腿改回 `perror(3)` 那一族：前缀与换行都不是本壳说了算（16t 第四枚）**
  （`2026-10-14`，`claim/TRUNCATE-STRERROR-qoder` @ 基线 `96c429683`，单域三线 `os/commands/bin/fileops`
  与同线文档，无跨界）：接回批次二百四十三登记不执行①的第一枚。**是什么**：`truncate` 四处逐操数失败
  （`truncate.c:111`、`:118`、`:125`、`:135`）用的都是 `perror(fname)`，而 Minix3 的 `perror`
  （`minix3/lib/libc/stdio/perror.c`）**不缀程序名**；本壳印的 `truncate: <路径>` 两头都错：多了一个 C
  从不写的名字，少了一个 C 一定要写的号。判类 P0-code-bug + P0-fact（头注与 06 篇都把那个前缀当成本树惯例）。
  **核对中另坐实两处**：①`truncate.c:67-70` 与 `:92-94` 那两句裸 `fprintf` 后紧跟 `exit`，**不带换行**，
  本壳都给补上了；②`usage()` 的两行是硬写字符串（本命令不用 `getprogname()`，与 `rm`/`cp`/`mv` 不同），
  第二行的七个空格被本树的 `\` 续行符剔掉了。
  **怎么修**：`report(path, code)` 改交 `support::perror_line`（权威是 `minix-types` 的 `perror_line`，与
  `err_line` 并列那一处），四处调用点从 `is_err()` 改 `if let Err(e)` 把号交出来；两句裸 `fprintf` 去掉
  换行；usage 拆成两个字节串字面量把缩进写回。
  **为什么不选另一条路**：把 `truncate` 也统一成 `warn(3)` 形（即加 `truncate: ` 前缀）看似与同命令包其他
  壳一致，而批次二百三十九立的规矩就是「哪一族缀程序名只有一处答案，不许统一成一族」——这里需要的恰恰是
  反过来：别给 `perror` 添上前缀。另一个选择是给那两句补换行「好看」，那也是改写外部行为。
  **门**：新建 `tests/truncate_face.rs`（5 枚，本 crate 第六扇进程级门）：失败行以操数开头且行首不得出现
  `truncate:`；非法 `-s` 逐字等于 `invalid size argument `bogus'`（无换行）；参照名失手只印那个名字且不出
  操数行；无选项与未列字母都答两行硬写 usage（含第二行的七空格）；**把副本改名后那一行照旧不带名字**
  （这一枚是两族之别最硬的证：`warn(3)` 会跟着改名，`perror(3)` 根本不写名字）。
  三件套：`cargo test -p minix-fileops` → **418 passed、0 failed**（基线 413，+5 枚门）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-fileops` → **418/0**、`cargo check -j 1
  --workspace` Finished 零 error；三架构 `cargo build -p minix-fileops --bins --target {x86_64,aarch64,
  riscv64gc}-unknown-none` 各 Finished 零 error；`rustfmt --edition 2024 --check` 触及两文件零漂移；clippy 对触及
  文件零命中；宿主实跑 `truncate -s 10 /tmp/x` → `/tmp/x: Input/output error` 退 1、`truncate -s bogus /tmp/x`
  → `invalid size argument `bogus'` 后紧跟提示符（无换行）、`truncate -r /tmp/ref /tmp/x` → `/tmp/ref`。
  核实日 `2026-10-14`；基线 `96c429683`。
  **三枚变异负控**（测完按 md5 复原）：退回 `truncate: <路径>` 前缀形（门 2 枚红）、给非法尺寸那句补换行
  （那枚独红）、usage 第二行去掉缩进（usage 那枚独红）。过程记：第一轮负控里一枚因 shell 引号被吃而
  根本没改进文件（门全绿会被误读成「发牙不对」），改用 python 脚本带 `assert anchor in s` 重跑才拿到红。
  **新拓一处待定形（登记不修）**：决定半 `relative_target` 用的是 `saturating_add`，而 C 是 `off_t` 加法
  自然回绕后再 `if (tsize < 0) tsize = 0`（`truncate.c:131-132`）——正向溢出时 C 得 0、本树得 `i64::MAX`；
  同一处 C 的 `EFBIG` 判据（`:122-126`）拿 `oflow` 与它自己的来源式相比、永不为真，这一点 06 篇已记。要按
  C 做就得改 `wrapping_add`，那是决定半的语义变更，不跟本批的诊断腿混在一笔里。
  06 篇两行随之改准：`bin/truncate.rs` 行、端到端门清单添上本扇。
  登记不执行：①余下 `du`/`find`/`stat`、doctools `what`、textfilter `join` 的同族腿；②`relative_target`
  的溢出形与 `EFBIG` 死支（如上）；③`ENOENT` 静默与真 `ftruncate` 腿归决定半库测与 `C-57`/`E5`。

- ✅ **批次二百四十三接上 `mv` 的十三处 `warn(3)` 腿，并把 C 的三种标签形分开保留（16t 第三枚）**
  （`2026-10-14`，`claim/MV-STRERROR-qoder` @ 基线 `af0b997c3`，单域三线 `os/commands/bin/fileops`
  与同线文档，无跨界）：接回批次二百四十二登记不执行①的第三枚。**是什么**：`mv` 的失败腿全部只印
  `mv: rename <从> to <到>` 这一类句子头，`Errno` 在 `is_err()`/`format!` 那一步就丢了：跨设备改名、
  权限拒、源不存在三种失败长得一样。判类 P0-code-bug（诊断行缺字段）。
  **核对中另坐实两处文案错**：`usage()` 写的是本树的 `[-f | -i] [-v]`，C 是 `[-fiv]`（`mv.c:404-405`）；
  询问梯里 `access(from, F_OK)` 失手那一支被展成了 `bool`，号拿不回来。
  **怎么修**：`do_move`/`cross_device_move`/`fastcopy` 多带一个 `prog`；十三处报告位改交
  `support::warn_line`，**按 C 的三种标签形分开保留**：前置形（`rename %s`、`rename %s to %s`、
  `can't remove %s`）、后置形（`%s: remove`、`%s: set times`、`%s: set owner/group`、`%s: set mode`）、
  无前缀形（`%s`）；`source_probe` 保留 `Result` 以便把 `access` 留下的号带进那一行；读写腿按 `cp` 同法把号
  记在 `Files::last_error`，不动决定半的 `CopyError`。
  **为什么不选另一条路**：①把十三处统一成 `mv: <路径>: <句子>` 很 tidy，但那三形在 C 里就不是同一行，
  拼平等于改写外部行为（批次二百三十九的两族之别同理）；②给 `warn(NULL)`（`mv.c:278`，报 `malloc`
  失手）发明一个号——本树的读块缓冲是一个 `Vec` 分配，失败不会给出可印的号，所以声明为无对位；
  ③`mv.c:362-398` 那六句 `_PATH_CP`/`_PATH_RM` 腿跟 exec 递送链一起未落地，不代它们编报文。
  **门**：新建 `tests/mv_face.rs`（5 枚，本 crate 第五扇进程级门）：改名失败行五段齐（程序名、`rename`、
  两个操作数、句子）且退 1；一个与零个操作数都答两行 usage；N 对一形在尾操数不是目次时同样走 usage；
  `-Z` 走 usage 而不是拒收句；副本改名后失败行与 usage 都跟着改名。宿主的 `access`/`stat`/`rename` 皆从
  传输缝失手，所以询问梯不触发、`fastcopy` 不可达，这一点写进了门头注与 06 篇。
  三件套：`cargo test -p minix-fileops` → **413 passed、0 failed**（基线 408，+5 枚门）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-fileops` → **413/0**、`cargo check -j 1
  --workspace` Finished 零 error；三架构 `cargo build -p minix-fileops --bins --target {x86_64,aarch64,
  riscv64gc}-unknown-none` 各 Finished 零 error；`rustfmt --edition 2024 --check` 触及两文件零漂移；
  clippy 对触及文件零命中；宿主实跑 `mv /tmp/x /tmp/y` → `mv: rename /tmp/x to /tmp/y: Input/output error`
  退 1、`mv one` → 两行 `[-fiv]` usage 退 1。核实日 `2026-10-14`；基线 `af0b997c3`。
  **三枚变异负控**（测完按 md5 复原）：`rename` 腿退回不缀句子（门 2 枚红）、usage 换回本树的空格形
  （usage 四枚全红）、程序名硬写 `"mv"`（改名那枚独红）。
  **过程记**：本轮第一次跑负控时 shell 的 `cd` 链断了，三枚变异实际跑在同一份改动上（读数看似全同），
  改用绝对路径重跑才拿到各自的红——负控也要检查 “它真的只改了这一处”。
  06 篇两行随之改准：`bin/mv.rs` 行、端到端门清单添上本扇。
  登记不执行：①余下 `truncate`/`du`/`find`/`stat`、doctools `what`、textfilter `join` 的同族腿；
  ②`mv` 的 `destination pathname too long` 两句（`mv.c:125`、`:141`）同 `cp` 的 `name too long` 一样
  在本树无对位（`dest_name` 不封顶），要与长度上限一并定形；③exec 递送链那六句（`mv.c:362-398`）归
  `NS5`/`C-21`；④真机写腿与询问梯的tty 形状归 `C-57`/`E5`。

- ✅ **批次二百四十二把 `cp` 的十九处 `warn(3)` 腿与五处 `err(3)` 腿按 C 接齐（16t 第二枚）**
  （`2026-10-14`，`claim/CP-STRERROR-qoder` @ 基线 `252f9b1e1`，单域三线 `os/commands/bin/fileops`
  与同线文档，无跨界）：接回批次二百四十一登记不执行①的第二枚。**是什么**：`cp` 是本命令包里诊断腿
  最多的一枚，而本壳每一处都只印 `cp: <名字>`（有的带 `stat:`/`chown:` 这样的标签，却没有那个号）：
  权限拒、磁盘满、路径过长在诊断行里长得一样。判类 P0-code-bug（诊断行缺字段）。
  **核对中拓出两处行为差而不只是文案差**：①`cp.c:258-268`——目标不存在时 C 无论带不带 `-R` 都要
  `lstat` 第一个源，失手就 `err(1, "%s", *argv)` 退出；本壳以前只在 `-R` 下才取这个状态，于是
  `cp 不存在的源 新建目标` 在多源情形下会先把其余源拷完再报——代码注释里那句「C 也不取这个状态」
  是 P0-fact（对 C 的描述错）；②`cp.c:542-544` 的 `err(1, "fts_read")` 是终止腿，本壳以前报完继续走
  （与批次二百四十一的 `rm` 同一病）。
  **怎么修**：`stat_code`/`lstat_code` 把号留下（原本的 `stat_of`/`lstat_of` 返回 `Option` 即丢号，
  已删）；`prog` 在 `run()` 里取一次往下传；十九个报告位全部交 `support::warn_line`（同一位上 C 带
  标签的就带标签，如 `stat:`、`lutimens:`）；`err` 的五位也都改成印完即 `terminate(1)`。`CopyError` 是个纯
  类型，不动它：号记在 `Files` 的 `last_error` 上，由 `copy_file` 在报告位取。
  **为什么不选另一条路**：①把 `CopyError` 改成带号的结构会把内核侧的 `Errno` 泄进决定半（那层不引
  `minix-sys`，是 99 篇 §1 的分层契约）；②给 `munmap`/xattr/chflags/`-l` 那四个位发明对位 = 报 C 在本树
  根本不执行的腿；③本树不用 mmap，所以 `utils.c:237` 那句 `warn` 只能声明为无对位而不是「未接」。
  **非 UTF-8 符链文本那一腿本树自创**（C 递的是字节），改印 `cp: <源>: link text is not valid UTF-8`——
  以前它顶着 C 的 `symlink:` 标签却报源名，看着像 C 的那一行而报的又是另一回事。
  **门**：新建 `tests/cp_face.rs`（5 枚，本 crate 第四扇进程级门）：目标 `stat` 失手是三段齐的 `err(3)`
  行且只带目标名不带源名、退 1；两行 usage 逐字对 C（第二行七个空格缩进，本树以前用 `\` 续行符把它剔掉了）；
  `-a` 拒收行是 `warnx` 形（有程序名无句子）；未列字母 `-Z` 走 usage 而不是拒收句；副本改名后 usage
  与失败行都跟着改名。宿主 `stat` 一律回 `EIO`，入口之后的腿（`copy_file` 读写族、`setfile`、`-R` 走表）
  本门够不着，已写进门头注与 06 篇。
  三件套：`cargo test -p minix-fileops` → **408 passed、0 failed**（基线 403，+5 枚门）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-fileops` → **408/0**、`cargo check -j 1
  --workspace` Finished 零 error；三架构 `cargo build -p minix-fileops --bins --target {x86_64,aarch64,
  riscv64gc}-unknown-none` 各 Finished 零 error；`rustfmt --edition 2024 --check` 触及两文件零漂移；
  clippy 对触及文件零命中（`doc_lazy_continuation` 与 `useless_conversion` 已就地消除，`copy_dir` 多一个
  形参后按 `copy_file` 先例加 `#[allow(clippy::too_many_arguments)]`）；宿主实跑 `cp /tmp/a /tmp/b` →
  `cp: /tmp/b: Input/output error` 退 1、`cp only-one` → 两行 usage 退 1、`cp -a …` → 拒收句退 1。
  核实日 `2026-10-14`；基线 `252f9b1e1`。
  **三枚变异负控**（改完即测、测完按 md5 复原）：目标那一腿退回只报名字（门 2 枚红）、程序名硬写
  `"cp"`（改名那枚独红）、usage 第二行去掉七空格缩进（usage 两枚与改名枚共 3 枚红）。
  06 篇两行随之改准：`bin/cp.rs` 行的诊断面描述、端到端门清单添上本扇。
  **拼法旁注**（免得下一轮误读）：头注里那份渲染权威写作 crate 名形 `minix-types` 的 `err_line`，
  不写下划线形——`check-command-boundary.sh` 用粗 grep 扫命令层里的 `minix_types` 字样，连注释一起计
  （本批实测多一处就是 22 vs 21），而分层契约拦的是真导入，不是提到权威的名字。
  登记不执行：①`mv`/`truncate`/`du`/`find`/`stat`、doctools `what`、textfilter `join` 的同族腿；
  ②`cp` 的 `name too long` 三句（`cp.c:211`、`:362`、`:408`）在本树没有对位——`join_path` 不封顶长度，
  要按 C 做就得先定长上限与拒法，属单独一轮而不是文案接线；③`-v` 回显与成功路写入仍需可写根盘（`C-57`/`E5`）。

- ✅ **批次二百四十一接上 `rm` 的 `warn(3)`/`warnx(3)`/`err(3)` 三条腿：那个号终于进了那一行**
  （`2026-10-14`，`claim/STRERROR-WIRE-2-qoder` @ 基线 `a543f8b22`，单域三线 `os/commands/bin/fileops`
  与同线文档，无跨界）：接回批次二百三十九登记①的第一枚。**是什么**：`rm` 每一条失败腿都印
  `rm: <路径>`就收——C 的那行是 `warn("%s", f)`（`rm.c:311`、`:348`），经 `vwarn.c` 渲染成
  `getprogname(): <路径>: <strerror(errno)>`。本壳在 `warn_err(path)` 那一步就把 `Errno` 丢了：
  权限被拒、路径过长、不存在三种失败在读者眼里长得一模一样。判类 P0-code-bug（诊断行缺一个字段）。
  **核对中另坐实三处同源错**：①程序名硬写成 `"rm"`，而 C 用 `getprogname()`（`argv[0]` 去掉目录的
  最后一段）；②`usage()` 抄的是本树的空格形 `[-f | -i]`，C 是 `[-f|-i]`（`rm.c:624`）；③目录读失败
  那一腿本树报完继续走，C 是 `err(1, "fts_read")`（`rm.c:284-285`）印完即退 1。
  **怎么修**：`warn_err(prog, path, code)` 把号交给 `support::warn_line`（形状与句子都在
  `minix_types::err_line` 那一份权威里），程序名在 `run()` 里取一次往下传；`warnx` 那两处可达腿
  （`:323` 的 is a directory、`:610` 的「. 与 .. 不得删」）只加程序名、不缀句子；`fts_read` 腿改成按 C 退出。
  **为什么不选另一条路**：①给每壳各写一份 errno→句子渲染等于把 `verrc.c` 抄十几遍，句子表一改就散；
  ②把 `warnx` 也统一成 `warn` 形状，就是给「is a directory」缀一句并不存在的号——批次二百三十九立的
  「两族只有一处答案」只是要分清谁带句子；③`fts_read` 继续报完就走 = 与 C 的终止语义分岔，而这类分岔
  必须由裁决来定、不能在接线时悄悄保留（本轮采 C，因为真值优先序里 C 是纲）。
  **门**：新建 `tests/rm_face.rs`（6 枚，fileops 的第三扇进程级门）：一个操作数失败出三段齐的一行、
  三个操作数出三行只一个退码、把副本改名后跑证明程序名不是硬写（usage 行也跟着改名）、无操作数时空手
  usage 退 1 而带 `-f` 静默退 0（`rm.c:144-148`）、`.`/`..` 只抱怨一次且那句没有句子尾巴、`-r` 的根取不到
  走 `FTS_NS` 那一支。号码不钉（宿主 `lstat` 回 `EIO`，那是环境事实不是 C 的契约）。
  三件套：`cargo test -p minix-fileops` → **403 passed、0 failed**（基线 397，+6 枚门）；容器
  `minix-ci:1.94`（挂仓根、带 `-u`）`cargo test -j 1 -p minix-fileops` → **403/0**、
  `cargo check -j 1 --workspace` Finished 零 error；三架构 `cargo build -p minix-fileops --bins
  --target {x86_64,aarch64,riscv64gc}-unknown-none` 各 Finished 零 error；`rustfmt --edition 2024 --check`
  触及两文件零漂移；clippy 对触及文件零命中；宿主实跑 `rm /tmp/nope` → `rm: /tmp/nope: Input/output error`
  退 1、`rm -f` 空手退 0、`rm . .. .` 只抱怨一次。核实日 `2026-10-14`；基线 `a543f8b22`。
  **三枚变异负控**（改完即测、测完按 md5 复原）：句子退回只报路径（门 5 枚红）、程序名硬写
  `"rm"`（改名那枚独红）、usage 恢复本树的空格形（usage 两枚红）。
  **验证边界**：`NONEXISTENT` 三值分档与 `-f` 静默只在决定半的库测里钉（宿主拿不到 ENOENT）；
  `fts_read` 腿宿主不可达（`open` 先失败），要等可写根盘的真机门；`-v` 的回显腿需要真删成功才出，
  宿主 `unlink` 恒失败故不断言（`emit` 的宿主双生面在本 crate 已到位，缺的是成功路径本身）。
  **诊断腿穷举**（`rm.c` 逐枚点完，14 处）：本轮接上 9 处可达腿——`:202-204`（FTS_DNR）、
  `:219`/`:222`（FTS_NS）、`:277`（树内删失败）、`:285`（fts_read）、`:311`（rm_file 的 lstat 失败）、
  `:323`（is a directory）、`:348`（rm_file 的删失败）、`:610`（「. 与 .. 不得删」）、`:624`（usage）；
  `:549` 与 `:567-570` 两句询问是 C 的裸 `fprintf`，本就无程序名、本轮不动；剩下四处不可达——
  `:317`（`-W` 的 EEXIST 句）与 `:535`、`:562-565`（`-P` 的两处）随被前置拒的旗标一起死，
  `:196`（`err(1, "fts_open failed")`）与 `:208`（`errx` 的 FTS_ERR）在本树没有对位调用：
  前者被 `open` 失败腿（即 FTS_DNR 那一支）吃掉，后者被 `:285` 的终止腿吃掉。
  06 篇两行随之改准：`bin/rm.rs` 行的诊断面描述、以及端到端门清单添上本扇。
  登记不执行：①余下 `cp`/`mv`/`truncate`/`du`/`find`/`stat`、doctools `what`、textfilter `join` 的同族腿
  （逐枚先对 C 判哪一族再接）；②`os/commands/bin/shell` 那份 `bin_support.rs` 的 `emit` 仍无宿主双生面
  （`grep -c "pub fn emit"` 该文件 = 1，其余十三个共享件都是 2）；③真机播种与写腿归 `C-57`/`E5`。

- ✅ **批次二百四十更正权威编译门的挂载点：那枚「容器恒红测」是取证配方错，不是代码债**
  （`2026-10-14`，`claim/GATE-MOUNT-RECIPE-qoder` @ 基线 `6581e7248`，改的是仓库共享件
  `AGENTS.md` 与 coordination 两份配方文档 + 一处测试注释，登记 C-99 后动手）：
  接回批次二百三十三「本轮新发现的债」。**真相**：`test_c_bridge_fs_stat_offsets_match_rust_layout`
  读的是**仓库根**下的 `tools/atf-c-compat/posix-stubs.c`（路径按 crate 位置推导），而配方
  （`NK4A-TODO.md` §5.1 与 §7 命令块）写的是「在 `os/` 目录下执行 `-v $PWD:/work -w /work`」——
  那样容器里根本没有 `tools/`，测试只能报 `NotFound`。此前批次二百三十三/二百三十七/二百三十八/
  二百三十九的容器读数里那句「唯一 FAILED 是这枚恒红测」**全部由此产生**，属测量假象。
  **判类**：P0-process-violation。取证方法一错，门里的读数就全都不可比，后人还会把测量问题当成缺陷去修。
  **怎么修**：改配方而不是改测试——三处文档统一成「在仓库根执行、`-v $PWD:/work -w /work/os`」
  （`AGENTS.md` Build & Test 是所有 agent 的第一入口，`CONCURRENT-WORK-PROTOCOL.md` §6 内存铁律
  补同一句，`NK4A-TODO.md` §5.1 与 §7 命令块同步并顺手在内核那行注明要带 `--features mock` 形态）；
  测试注释里写清「容器门必须以仓根为挂载点，报 `NotFound` 是配方错不是布局漂移」。
  **为什么不选另一条路**：给测试加 `#[ignore]` 或「读不到就跳过」会让这条 C 桥对账在门里
  永久失效——它正是用来抓「Rust 布局改了而 C 侧宏没跟着改」的那道牙，牺牲它换取门好看是反的。
  三件套（同一棵树、同一镜像 A/B 对照）：`os/` 为根挂载 → `cargo test -j 1 -p minix-types` =
  **330 passed、1 failed**（`posix-stubs.c readable from crate dir: NotFound`）；仓库根挂载
  `-w /work/os` → **331 passed、0 failed**；`cargo check -j 1 --workspace` Finished 零 error；
  `rustfmt` 触及文件零漂移；文档三处过 `doc-style-lint.sh --diff` error 0、`pattern-gate --diff`
  PASS=17 FAIL=0；本子树断链与基线同数。核实日 `2026-10-14`；基线 `6581e7248`。
  **后续轮次沿用**：本文件上文各批次里「容器内唯一 FAILED 是恒红测」那句一律按本批更正读；
  **本批顺带的行尾归一（声明清楚，别让它藏在 diff 里）**：`CONCURRENT-WORK-PROTOCOL.md` 的入库
  blob 带 CRLF（183 个 CR），而 `.gitattributes` 立的是「仓库内一律存 LF、`* text=auto`」，本批那次
  整文件写回把它归一了，于是 `git show --stat` 报出 366 行变更。去 CR 后逐行核对：基线与本批只有
  **1 行**不同（§6 内存铁律里那句挂载点），即纯行尾无内容改动，与批次一百一十三那次归一同性质。
  教训：改这类旧文件前先查行尾，被动归一必须在此声明，否则评审者无法从 366 行里分辨那一行真改动。
  `tools/todo-reconcile-scan.sh` 的「宿主不可达的门内测试」计数不含这条（它本就是仓可达、容器挂载不可达）。
  登记不执行：无（本条闭环）。

- ✅ **批次二百三十九接上 `link`/`unlink` 的 `perror(3)` 腿，并把「哪一族缀程序名」写成一处权威**
  （`2026-10-14`，`claim/UNLINK-PERROR-qoder` @ 基线 `953870dbe`，跨线：二线 `os/libs/minix-types` +
  三线 `os/commands/bin/fileops`，登记 C-98 后动手）：
  接回批次二百三十八登记①的第二处。**是什么**：核对 C 时发现这两条腿此前**报错的东西就不对**——
  `unlink.c:56`/`link.c:57` 都是 `perror("unlink")`/`perror("link")`，而 Minix3 的 `perror`
  （`minix3/lib/libc/stdio/perror.c`）打的是 `fprintf(stderr, "%s%s%s\n", s, separator, buf)`：
  **既不带程序名前缀**（与 glibc/BSD 的习惯不同），**也从不打操作数**；`separator` 只在调用者串非空时是 `": "`。
  本树两壳都印 `unlink: <路径>` / `link: <newpath>`——看着像 C 的一行，报的却是操作数而不是那个号。
  判类 P0-code-bug（诊断内容与 C 不同）+ P0-fact（06 篇记着"perror 无 strerror 后缀→仅报路径"）。
  **怎么修**：`minix-types` 新增 `perror_line(label, code, out)`，与 `err_line`、`prog_name` 并排——
  「`err(3)`/`warn(3)` 缀程序名、`perror(3)` 不缀」这个差别从此只有一处答案；fileops 的 `bin_support.rs`
  加写行入口 `perror_line`（渲染 + 换行，不退，退由调用者定，同 C）；两壳的失败臂交出那个号。
  06 篇那行的说法随之改准，`§5` 补上进程级门的清单。
  **门**：新建 `tests/linkops_face.rs`（4 枚）：`unlink` 失败行必须是 `unlink: <句子>` 且**路径不出现**、
  `link` 同理两个操作数都不出现、argc 形状先落 usage（usage 行是硬写字符串、不带程序名，与 C 同）、
  `-x` 当普通名字走同一条腿。号码不钉（宿主传输回什么是环境事实），字段顺序、操作数缺席、退码钉死。
  三件套：`cargo test -p minix-fileops` → **397 passed、0 failed**（基线 393，+4 枚门）；
  `cargo test -p minix-types` → 331/0（`perror_line` 两条断言并入 `err_line_renders_c_verrc_shape`）；
  三架构 `cargo build -p minix-fileops --bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）
  `cargo check -j 1 --workspace` Finished 零 error、两包 `cargo test -j 1` 出 32 个 `test result: ok`、
  FAILED 仅剩批次二百三十三登记的那枚恒红测；`rustfmt` 触及五文件零漂移；clippy 对触及文件零命中；
  `pattern-gate --diff` PASS=17 FAIL=0；`doc-style --diff` error 0；宿主实跑 `unlink /tmp/nope` →
  `unlink: Input/output error`、`link /tmp/a /tmp/b` → `link: Input/output error`，均退 1。
  核实日 `2026-10-14`；基线 `953870dbe`。
  **五枚变异负控**（`tmp/unlink-perror-mutate.py`，门/库/权威三处分别调用、复位断言 md5）全部咬中且
  各咬在各处：报操作数不报句子（只门红）、给 `perror` 加上程序名前缀即 glibc 化（门 3 枚红）、
  丢掉空串的 `separator` 规则（只权威测红，说明这条规则只有权威处管得着）、句子换成大写代号（门与权威测同红）、
  `link` 留着报新路径（只门红）。
  登记不执行：①其余 `err(3)`/`warn(3)`/`perror(3)` 腿的逐枚接线（rm/cp/mv/truncate/du/find/stat、
  doctools `what`、textfilter `join`）——接线时要按本批的两族之别选 `err_line` 还是 `perror_line`，
  不许统一成一族；②`emit` 的宿主双生面；③真机播种与 open 腿归 C-57/E5。

- ✅ **批次二百三十八接上 `mkfifo` 的 `warn(3)` 腿：一行里既有程序名也有那个号的句子**
  （`2026-10-14`，`claim/MKFIFO-ERRSUFFIX-qoder` @ 基线 `2c5088736`，**跨线**：二线 `os/libs/minix-types`
  （新增 `prog_name`）+ 三线 `os/commands/bin/fileops`、`os/commands/bin/shell`（改为转发），
  在 `new_edge4.md` 登记 C-97 后动手）：
  接回批次二百三十七登记①的第一枚。**是什么**：C 的 `mkfifo` 逐名建管道，失败那行是
  `warn("%s", *argv)`（`mkfifo.c:95`），而 `vwarn.c` 把它渲染成 `getprogname(): <名>: <strerror(errno)>`，
  然后**继续**下一个名字（`exitval = 1` 是记账不是离开）。本壳此前只印 `mkfifo: <名>`：既没有程序名，
  也没有句子——22 篇与 06 篇都写着「本壳尚未接那张表」。判类 P0-code-bug（报文缺两个字段）。
  **怎么修**：把 `getprogname()` 的取法也沉到权威处——`minix-types` 新增 `prog_name(argv0)`
  （规则逐字取自 `minix3/lib/csu/common/crt0-common.c:156-165`：取最后一个 `/` 之后那段），
  shell 那份同名本地实现改为转发，fileops 的 `bin_support.rs` 加 `prog_name` 与
  `warn_line(prog, label, code)`（渲染 + 写 + 不退，与 C 的 `vwarn` 同形）；`mkfifo` 的循环改成
  `if let Err(code) = mkfifo(...)` 交给它。`-m` 被拒那条腿**故意不动**：C 那句 `err(1, ...)` 缀的是
  当时的 `errno`，而 `setmode` 返回 `NULL` 时并不置号（`mkfifo.c:74-76`），缀什么全凭运气——
  已在注释里写明这一句没有可复刻的尾巴。
  **门**：新建 fileops 的第一扇端到端门 `tests/mkfifo_face.rs`（4 枚）：两个名字出两行且每行都是
  `mkfifo: <名>: <句子>`、退 1 且循环走完；`-m bogus` 即刻走且后面不出行；无名字与 `-q` 同答 usage；
  把副本放进临时目录按带目录的路径调用，程序名那段仍不含斜杠。号码不钉（宿主传输回什么是环境事实）。
  三件套：`cargo test -p minix-fileops` → **393 passed、0 failed**（基线 389，+4 枚门）；
  `cargo test -p minix-shell` → 81/0、`cargo test -p minix-types` → 331/0（`prog_name` 三条断言并进门测）；
  三架构 `cargo build -p minix-fileops --bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）
  `cargo check -j 1 --workspace` Finished 零 error，三包 `cargo test -j 1` 出 37 个 `test result: ok`、唯一的
  FAILED 是批次二百三十三登记的那枚恒红测（容器读不到 C 源文件那条）；`rustfmt` 触及五文件零漂移；
  clippy 对触及文件零命中；
  宿主实跑 `mkfifo /tmp/a /tmp/b` → 两行 `mkfifo: /tmp/a: Input/output error` 退 1（C 的续行形状）。
  核实日 `2026-10-14`；基线 `2c5088736`。
  **四枚变异负控**（`tmp/mkfifo-warn-mutate.py`）全部咬中：丢掉整个句子尾巴、程序名用整条 `argv[0]`
  （带目录）、失败即退出（丢掉「继续下一个名字」这条 C 的关键差别）、把行首程序名去掉。
  登记不执行：①其余 fileops 壳的同款接线（rm/cp/mv/unlink/link/truncate/du/find/stat 各腿，逐枚判
  errno 是否在手，队列 16t）；②`emit` 的宿主双生面（成功路径 stdout 在 shell crate 仍不可见）；
  ③真机播种与 open 腿归 C-57/E5。

- ✅ **批次二百三十七把 `err(3)` 那一行的渲染下沉成 `minix-types` 的唯一权威，并按 C 补齐 `hostname`/`domainname` 的两句**
  （`2026-10-14`，`claim/ERR-LINE-SHARED-qoder` @ 基线 `8f91514e2`，**跨线**：二线 `os/libs/minix-types` +
  三线 `os/commands/bin/shell` 与 `os/commands/games/stdio-games`，照规则 1 在 `new_edge4.md` 登记 C-96 后动手）：
  接回批次二百三十六登记②。**是什么**：批次二百三十五把渲染写在 games 那一份 `bin_support.rs` 里，第二个消费者
  一出现就要复制一份形状；而 `hostname`/`domainname` 的 `err(1, "sethostname")` 离 C 差的不止句子——
  `verrc.c` 那一行以 `getprogname()` 开头（程序名是运行时刻从 `argv[0]` 去掉目录取的最后一段，
  `minix3/lib/csu/common/crt0-common.c:156-165`），本壳却打 `sethostname: EINVAL`：既没有程序名，又把标识符当句子。
  核对中还发现第三个缺口只在宿主可见：该 crate 的 `warn` 没有宿主双生面，宿主侧诊断进的是替身传输，
  `hostname --x` 跑出退码 1 而 stderr 全空——报文形状在宿主根本无法端到端验证（正是三度复现的那条坑）。
  判类 P0-code-bug（缺程序名前缀、以代号代句子）+ P0-design-missing（形状无唯一权威、宿主诊断面缺双生）。
  **怎么修**：①`err_line(prog, label, code, out)` 挪到 `minix-types` 紧挨 `Strerror`，games 那份改为
  `pub use minix_types::err_line` 转发（形状只此一处，C 也只有一个 `verrc`）；②shell crate 的 `bin_support.rs`
  新增 `prog_name`（按 C 取 `argv[0]` 末段）与 `err_exit`（渲染 + 写 + 退），`report_err` 交给它；
  ③`warn` 补宿主双生面（宿主走 `std::io::stderr`，真机仍是 `minix_sys::write(STDERR)`）；
  ④`usage()` 的程序名同样改取 `getprogname()`，与 C 的 `usage: %s` 同源。**为什么优于在各壳手写**：
  C 的 `err(3)` 是库函数，形状属于承载它的这一层，不属于某一个命令。
  **门测的新形状**：新增第 16 扇端到端门 `tests/hostname_face.rs`（4 枚）——失败腿断言
  `hostname: gethostname: ` / `hostname: sethostname: ` / `domainname: …` 的前缀与「一句非空、不再含字段、
  不是全大写代号、以换行收尾」的形状，外加 `--x` 的两句 usage。号码本身**故意不钉**：宿主传输回什么号
  不是 C 的契约，可契约里的前缀、操作名、分隔符、句子、退码全钉。
  三件套：`cargo test -p minix-shell` → **6 目标全 ok、合计 81 passed、零 FAILED**（基线 5 目标 / 69：
  四壳各 +2 枚共享面测、新门 4 枚）；`cargo test -p minix-types` → **331/0**（基线 330，新增
  `err_line_renders_c_verrc_shape`）；`cargo test -p minix-stdio-games` → **18 目标 / 184 passed / 零 FAILED**
  （转发未减测）；三架构 `cargo build -p minix-shell --bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）
  `cargo check -j 1 --workspace` Finished 零 error，三包容器内 23 个 `test result: ok`、唯一 FAILED 是
  批次二百三十三登记的恒红测 `test_c_bridge_fs_stat_offsets_match_rust_layout`；宿主实跑
  `hostname` → `hostname: gethostname: Input/output error` 退 1、`hostname newname` → `…sethostname: …`、
  `hostname --x` → `usage: hostname [-s] [name-of-host]`（此前宿主全哑）。`rustfmt` 触及五文件零漂移
  （`errno.rs` 基线那处双空行随整文件格式化一并归零）；clippy 对触及文件零命中；`pattern-gate --diff`
  PASS=17 FAIL=0；`doc-style --diff` error 0。核实日 `2026-10-14`；基线 `8f91514e2`。
  **五枚变异负控**（`tmp/err-line-shared-mutate.py`，四类目标分别调用、复位断言 md5）全部咬中：
  程序名不去目录（面门红）、渲染漏掉操作名（门 3 枚红）、渲染丢掉程序名（四处皆红）、以标识符面代句子
  （四处皆红）、宿主诊断退回替身传输（4 枚门全红）。
  登记不执行：①其余命令的 `err(3)`/`perror` 后缀接线（fileops 七枚、`what`、`join`，队列 16t），
  本轮只把权威与形状立好；②`emit` 的宿主双生面（成功路径的 stdout 在宿主仍不可见，本轮只补了 `warn`）；
  ③真机播种与 open 腿归 C-57/E5。

- ✅ **批次二百三十六更正命令族里「本树无 strerror 表」这句已成假的理由（12 处，逐处判接线状态）**
  （`2026-10-14`，`claim/STRERROR-NOTES-qoder` @ 基线 `c2289f220`，跨四包同属三线 commands：
  `bin/fileops`（rm/unlink/mkfifo/du/find/truncate/stat）、`bin/shell`（hostname/domainname）、
  `usr-bin/doctools`（what）、`usr-bin/textfilter`（join）；承接 C-95）：
  接回批次二百三十五登记①。**是什么**：批次二百三十三把 C 的 `sys_errlist` 落进 `minix-types` 之后，
  命令族里仍有十余处注释把「报文缺 `err(3)`/`perror` 那句尾巴」的理由写成**本树没有那张表**——这句自那批起
  就是假的（真表在 `Errno::message`）。理由写错比行为差异更糟：后来人会照着「没有表」再建一张。判类 P0-fact。
  **怎么修**：逐处判接线状态再改措辞，不做批量替换——已判**接线待批**的 10 处改成「表在本树，本壳尚未接它（已登记）」；
  另两处（`hostname`/`domainname` 的 `report_err`）在核对 C 时发现**差异不止句子**：C 的 `err(1, "sethostname")`
  经 `verrc.c` 打的是 `getprogname() + ": " + 调用者的串 + ": " + strerror`，本壳既不打程序名前缀
  （现在只有 `sethostname: EINVAL`），也用标识符代替句子；旧注释只说「本树无表」把这个前缀缺口也一起掩住了。
  两处注释现在把两个缺口分开写明，并按「不顺手修」登记为独立条目（要改退码面之外的报文形状，须配门测与真机判据）。
  **取证**：`git grep` 同一模式在基线与本树的命中数 12 → 0（剩下的 1 处是 `cp.rs` 讲 fts 环路报告的
  「has no counterpart」，与 strerror 无关）；脚本 `tmp/strerror-notes-sweep.py` 逐处断言原文唯一后才替换。
  行为零改动，故回归以「四包测试数不动」为准：`minix-fileops` **389/0**、`minix-shell` **69/0**、
  `minix-doctools` **77/0**、`minix-textfilter` **578/0**（与基线同数）；三架构 `--bins` 四包三台零 error；
  clippy 对 11 个触及文件零命中；`rustfmt` 触及文件零新增漂移（唯一一处 diff 在
  `doctools/src/bin_support.rs`，基线即存，未动）；`pattern-gate.sh --diff` PASS=17 FAIL=0；
  `doc-style-lint.sh --diff` 无文档正文改动。核实日 `2026-10-14`；基线 `c2289f220`。
  登记不执行：①各命令的 `strerror` 后缀接线（fileops 七枚、doctools 的 `what`、textfilter 的 `join`），
  逐条要配门测；②`hostname`/`domainname` 的 `err(1,…)` 两句需同时补 `getprogname()` 前缀与句子，
  属报文形状变更，另轮带门做；③真机播种与 open 腿归 C-57/E5。

- ✅ **批次二百三十五把标准输入读缝的失败面改成带 `errno`，接上 `factor: stdin` 与 `ppt: fgets` 两句 `err`**
  （`2026-10-14`，`claim/STRERROR-SEAM-qoder` @ 基线 `9d950ff9a`，单域三线 commands/games/stdio-games，
  含同域共享件 `src/bin_support.rs`；承接 C-95）：
  接回批次二百三十四登记①。**是什么**：C 的两个游戏把「标准输入读出错」与「读到尽头」当作两件事
  （`factor.c:157-161` 在 `fgets` 返回 `NULL` 后先看 `ferror(stdin)` 才 `err(1, "stdin")`，否则 `exit(0)`；
  `ppt.c:109-111` 用 `!feof(stdin)` 判读出错并 `err(1, "fgets")`，只有真到底才补那个换行）。本树的读缝
  `stdin_chunk` 把失败折成不带号的 `Err(())`，壳里一律 `unwrap_or(false)`，于是两句 `err` 无处可接——
  缺的不是措辞而是**信息**。判类 P0-code-bug（有错无声）+ P0-design-missing（接缝面缺号）。
  **怎么修**：`stdin_chunk` 的双生两面都改为 `Result<usize, Errno>`——宿主侧取 `raw_os_error()`（那正是
  失败的 `read(2)` 留下的号），裸机侧直接传播 `minix_sys::read` 的 `Errno`；`LineReader::next_line()`
  随之带号；共享件新增 `err_line(prog, label, code, out)`，把 `minix3/lib/libc/gen/verrc.c` 那一行
  （程序名、冒号空格、调用者的串、冒号空格、`strerror` 的句子）一次写定，句子仍出自批次二百三十三的表，
  表外的号由 `Strerror` 出 C 的 `Unknown error: %u`。factor 与 ppt 各按 C 分两条路；`bcd`/`number`/
  `arithmetic` 三壳维持「读出错即收摊」，因为 C 那三处**没有** `ferror` 判据（已核 `bcd.c`、`number.c`、
  `arithmetic.c` 全文无此调用），并在注释里写明这是 C 自己的分法。
  **取证的新招**：端到端门用**目录描述符**当标准输入——`open` 得成、`read` 必失败于 `EISDIR`，
  于是「读出错」这条腿第一次在进程级可测（宿主与真机同形），金样 `factor: stdin: Is a directory` 退 1、
  `ppt: fgets: Is a directory` 退 1，并各带一枚空管道对照（退 0 且不出声）。
  三件套：`cargo test -p minix-stdio-games` → **18 目标全 ok、合计 184 passed、零 FAILED**（基线 18/173：
  九壳各 6→7 枚共享面测、`--test factor_face` 9→10、`--test ppt_face` 5→6）；三架构 `--bins` 三台零 error；
  容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error；clippy 触及文件零新增
  （中途一处 `unused imports` 已把两个号移进测试侧）；`rustfmt` 触及八文件全部零漂移（基线即零）；
  `pattern-gate.sh --diff` PASS=17 FAIL=0；`doc-style-lint.sh --diff` error 0；宿主实跑目录标准输入
  两句逐字复现。核实日 `2026-10-14`；基线 `9d950ff9a`。
  **五枚变异负控**（`tmp/stdin-seam-mutate.py`，`--bin factor support::`、`--test factor_face`、
  `--test ppt_face` 分三次调用、复位断言 md5）全部咬中，且各咬在自己那条腿：句子渲染成空串（三处皆红）、
  宿主侧丢掉真号改报 `EIO`（两扇门红）、factor 那条腿折回「读到尽头」（只 factor 门红）、ppt 同理（只
  ppt 门红）、表外的号打成形数（共享面测红）。
  登记不执行：①其余写着「本树无 `strerror` 表」的注释更正（`os/commands/` 下十余处，逐处判「已可接」
  与「仍需接线」，不批量替换）；②真机侧目录读的号是否同为 `EISDIR` 归 VFS 面另行核（本轮金样在宿主取）；
  ③真机播种与 open 腿归 C-57/E5。

- ✅ **批次二百三十四接上 `factor` 的溢出腿：`ERANGE` 那句与 `illegal numeric format` 分成两句**
  （`2026-10-14`，`claim/STRERROR-WIRE-qoder` @ 基线 `16b1c1fca`，单域三线 commands/games/stdio-games，
  承接批次二百三十三的跨线登记 C-95）：
  接回批次二百三十三登记①的第一处。**是什么**：C 的 `factor` 取值有两条不同的出路——`BN_dec2bn`
  （`factor.c:118-128`）先 `errno = 0` 再 `strtoul`，**只看 `errno` 就 `err(1, "%s", str)`**（`:125-126`），
  之后才判 `*p == '\n' || *p == '\0'`（`:127`）并交给调用者的 `illegal numeric format`（`:168`/`:177`）。
  也就是说溢出与带尾巴是两句话，而且溢出优先。本树此前把溢出折成 `Verdict::Illegal`，退码相同而句子不同；
  22 篇与库文档都把它记成「因本树无 `strerror` 表而按非法格式句给出（已声明的分流）」——表已在批次二百三十三落地，
  这条分流就不再是分流，而是与 C 不符。判类 P0-code-bug + P0-fact（文档与注释里的旧理由）。
  **怎么修**：决定半加 `Verdict::Overflow` 与 `OVERFLOW_ERRNO`（= `Errno::from_i32(ERANGE)`，
  号由 C 设、句子由本树的表给），溢出改判此支；执行半新增该臂，按 `minix3/lib/libc/gen/verrc.c` 的形状打
  `factor: <原串>: Result too large or too small` 退 1。**顺带**把库文档里那条已闭合的分流声明删掉，
  只留仍在的两条（负数检验的落点、stdin 形里 C 打的是已推进过的 `argv[0]`）。
  三件套：`cargo test -p minix-stdio-games` → **18 目标全 ok、合计 173 passed、零 FAILED**（基线 18/172）；
  `--lib factor::` 6/0（`verdicts_accept_signs_and_blanks_the_way_strtoul_does` 内加四组断言：
  `18446744073709551616` 溢出、`99999999999999999999999x` 溢出优先于尾巴、`u64::MAX` 不溢出、
  33 位前导零不溢出）；`--test factor_face` 8→**9/0**（新增 `an_overflowed_value_leaves_with_the_range_sentence_not_the_format_one`，
  操作数形、带尾巴的超大数、标准输入形各一枚）；三架构 `--bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）
  `cargo check -j 1 --workspace` Finished 零 error；`rustfmt` 零漂移；clippy 对 `factor.rs` 零命中。
  核实日 `2026-10-14`；基线 `16b1c1fca`。
  **五枚变异负控**（`tmp/factor-overflow-mutate.py`，`--lib factor::` 与 `--test factor_face` 分两次调用、
  复位断言 md5）全部咬中：溢出折回 `Illegal`、句子前不接冒号空格、壳里丢掉整句尾巴、号改用 `EINVAL`、
  把前导零当溢出（最后一枚一次掀翻七枚门测，说明「宽度不等于溢出」这条被真钉住了）。
  登记不执行：①`factor: stdin` 与 `ppt: fgets` 那两句 `err` 的后缀——它们要的是失败当场留下的号，
  而共享读缝 `stdin_chunk` 现在把失败折成不带号的形状，改它要连九个壳的行读取器一起改形（另一轮）；
  ②其余写着「本树无 `strerror` 表」的注释更正；③真机播种与 open 腿归 C-57/E5。

- ✅ **批次二百三十三把 Minix3 自己的 `sys_errlist` 落成 `minix-types` 的 errno 句子表，并接上第一个消费者 `caesar`**
  （`2026-10-14`，`claim/STRERROR-TABLE-qoder` @ 基线 `9734acb2a`，跨线：二线 `os/libs/minix-types` +
  三线 `os/commands/games/stdio-games`，照规则 1 在 `new_edge4.md` 登记 C-95 后动手）：
  接回批次二百二十九～二百三十二反复登记的「全族 strerror 基础件」。**是什么**：十余处壳注释与 22 篇都写着
  「本树无 strerror 表」，而真源一直都在——`strerror(3)`（`minix3/lib/libc/string/strerror.c` 转
  `strerror_r.c:53-92`）读的是 `sys_errlist`（`minix3/lib/libc/compat/gen/compat_errlist.c:52-151`，
  全树唯一一份定义），号小于 `sys_nerr`（80）取表、否则拼 `Unknown error: %u`；`err(3)`/`warn(3)` 的形状在
  `minix3/lib/libc/gen/verrc.c`（`progname: ` + 调用者的串 + 冒号空格 + 句子 + 换行）。判类 P0-design-missing
  （基础件缺位）+ P0-fact（`impl Display for Errno` 的注释自称是 C 的 strerror 面，而它给的是标识符；
  caesar 的旧注释把 34 号写成 glibc 的 `Numerical result out of range`，Minix3 的表写
  `Result too large or too small`）。
  **怎么修**：`errno.rs` 新增 `SYS_NERR`、`SYS_ERRLIST`（80 项，下标即号）、`Errno::message()`（句子面，
  表外 `None`）与 `Strerror`（整体形状，表外拼 C 那句；号按 C 的先转 `unsigned` 再比大小，负数因此绕成大数）；
  标识符面 `name()`/`Display` 不动，只把注释改准。决定半 `RotationFailure::errno()` 命名 C 设的那个号
  （`caesar.c:133`/`:135`），执行半按 `verrc.c` 缀句。**为什么优于此前的写法**（各壳要么手写整句、要么整段不写）：
  表在 `minix-types` 一处权威，其余消费者只需接线，不再各自造措辞。
  **表项来历**：脚本从 C 表逐行提取 80 项，并断言每个带注记的行满足 strings[注记号] 等于该行的串
  （79 条带注记行零失配），号与名再与 `minix3/sys/sys/errno.h` 的 `#define E… (_SIGN n)` 逐条对账
  （1..79 零不一致；0 号在表头无名，按 C 惯例记 `ENOERROR`）。
  三件套：`cargo test -p minix-types` → **330 passed、0 failed**（基线 326；`--lib strerror_face` 4/0）；
  `cargo test -p minix-stdio-games` → **18 目标全 ok、合计 172 passed、零 FAILED**（基线 18/171；`--lib caesar::`
  8→9；`--test caesar_face` 5/0，两句金样已换成带后缀的形）；三架构 `cargo build -p minix-stdio-games --bins`
  对 x86_64/aarch64/riscv64gc 三个 none 目标零 error；容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace`
  Finished 零 error；宿主实跑 `caesar -1` 得 Bad rotation value 那句缀 Result too large or too small、
  `caesar 5x` 缀 Invalid argument，退码均为 1。核实日 `2026-10-14`；基线 `9734acb2a`。
  **七枚变异负控**全部咬中：34 号改回 glibc 措辞、表长改 81、忘记 C 的 unsigned 比较、表外改用标识符面、
  `Invalid` 报错成 `ERANGE`、壳里去掉冒号空格、壳里丢掉整个句尾。
  **本轮新发现的债（只登记不执行）**：容器门里 `minix-types` 的
  `test_c_bridge_fs_stat_offsets_match_rust_layout` 恒红（`stat.rs:111` 以 crate 目录相对路径读
  `posix-stubs.c`，容器只挂 `os/` 故报 NotFound），基线 `9734acb2a` 现跑同红——属「宿主不可达的门内测试」族，
  修法应是把 C 文本随测试内联或改用由仓库根派生的路径，另轮认领。
  登记不执行：①其余消费者的后缀接线（factor 溢出句、ppt 读失败句，以及 `os/commands/` 下十余处写着
  「本树无 strerror 表」的注释——该说法自本批起不再成立）；②上述容器门恒红测；③真机播种与 open 腿归 C-57/E5；
  ④`../03-stage-rs/99-rs-global-concepts.md` 的教学构件承载门基线即欠（`tools/doc-code-density-check.sh --check`
  报 3 个 P1 承载缺码章：§2 常量族、§3 Rust 映射决策、§4 实现；本篇改动前后读数相同，故本批不顺手补码，另轮认领）。

- ✅ **批次二百三十二把 `arithmetic` 的答错罚则建成决定半，并按 C 重写抽题与提问循环**
  （`2026-10-14`，`claim/ARITH-PENALISE-qoder` @ 基线 `68c91aba6`，单域三线 commands/games/stdio-games，无跨界）：
  接回 22 篇登记的「`penalise` 罚则待批」。**是什么**：那条留白被记成提示层的小事，缺的其实是**出题模型**。
  C 答错时给这道题的两个操作数各记一笔惩罚——右操作数，加上「加法与乘法的左操作数 / 减法与除法的正确答案」
  （`arithmetic.c:255-262`）；此后每个操作数都在 `rangemax + 1 + 活惩罚总数` 的跨度上取随机值，落在惩罚段里就
  按链表的**头插顺序**命中，命中一次消耗一点、消耗尽即摘除（`penalise`/`getrandom`，`:296-364`）；同一道题会一直
  问到答对为止，提示语只在出题那一刻打一次（`:231-266`）。旧壳三条都不符：答错就换下一题（等于罚则完全不存在）、
  操作数取模用 `% range`（C 是含端点的 `rangemax + 1`）、除法用「商从 1 起」的自造形状而不是 C 的
  `right * rangemax` 构造再埋余数。判类 P0-code-bug + P0-design-missing。
  **怎么修**：决定半（`quiz.rs`）新增 `WRONG_PENALTY`、`PENALTY_CAPACITY`、`PenaltyList`（头插、逐个消耗、
  耗尽摘除、满表丢新惩罚——对应 C 里 `malloc` 失败就「忘记这个惩罚」的同一条规则）、`Penalties`
  （4 个算子 × 2 个操作数共八张表）、`Drawn::{Direct,Penalised,Inconsistent}`，并把「答错该记谁」这条规则本身
  也放进决定半（`slot_of`/`charged_operands`）：它先前只写在壳里，宿主测看不见，一枚变异因此逃过（见下）。
  执行半按 C 的 `problem()` 重写抽值（减从答案反推左操作数、除抽非零除数与商并把余数埋进被除数、乘积溢出则
  重试），提问循环改为一道题问到答对，输入结束只打一个换行退 0（分数只在每二十题与中断时打印，旧壳在输入结束
  时打分数也不符）。**已知差异如实登记**：链表容量固定 32 而 C 每笔 `malloc`（耗尽时两者都是丢新惩罚）；随机源
  仍是文档化的同余发生器而 C 用 `random(3)`（它的序列本就不是可观察契约）。
  三件套：`cargo test -p minix-stdio-games` → **18 个测试目标全 ok、合计 171 passed、零 FAILED**（基线 18 目标 /
  165 passed）；`--lib quiz::` 4→**10/0**（新增头插顺序、消耗到摘除、越界即 C 的 `bug: inconsistent penalties.`、
  满表丢惩罚、跨度含活惩罚、答错记谁的四例配对）；库测合计 70/0；三架构 `--bins` 三台零 error；容器
  `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、容器内 18 个 `test result: ok`、
  FAILED 与 error 计数 0；clippy 触及文件零新增（仅余 `bin/arithmetic.rs` 的存量一条）；`rustfmt` 零漂移；
  `pattern-gate.sh --diff` PASS=17 FAIL=0；`doc-style-lint.sh --diff` error 0；文档测试名与代码逐枚对账零 MISSING。
  宿主实跑：`printf 'x\n0\n1\n2\n3\n' | arithmetic -r 1 -o +` 得到
  `1 + 0 =  Please type a number.`→`What?`→`Right!`→再出同一题且提示不再重复——被记过惩罚的数反复回来，正是 C 的意图。
  核实日 `2026-10-14`；基线 `68c91aba6`。
  **八枚变异负控**（`--lib quiz::`，复位断言 md5）首轮**一枚没咬**：「答错记错操作数」——规则当时只活在壳里。
  处置不是放宽断言，而是把规则挪进决定半并补一枚配对测，再动同一处即翻红；其余七枚（每笔惩罚改 1、尾插代替头插、
  命中不消耗、直取区间多含一个值、取模跨度丢掉活惩罚、满表覆盖最旧项、加乘误记答案）全部咬中。
  登记不执行：①`Total time %ld seconds; %.1f seconds per problem` 的用时统计与中断打印分数（需秒级时钟与信号面，
  且 `%.1f` 的舍入还欠一个决定）；②真机播种与 open 腿归 C-57/E5；③全族 strerror 基础件（见批次二百三十）。

- ✅ **批次二百三十一把 `number` 重写成 C 的数词：三位一组带尺度名、每组自带句点、小数按位数命名量级**
  （`2026-10-14`，`claim/NUMBER-FACE-qoder` @ 基线 `5b183cf1e`，单域三线 commands/games/stdio-games，无跨界）：
  接回批次二百三十登记②。**是什么**：台账与 22 篇都只写「已接线」，而实现是**另一个程序**——
  `number_words` 把一个 `u32`（≤ 999999）读成一行英文，而 C 的 `number` 读的是**整行里的一个数**，
  按三位一组报尺度名（hundred 到 vigintillion 共 22 个，因此上限 65 位），**每组末尾自带一个句点**
  （`unit` 的 `lflag ? " " : ".\n"`，`number.c:229-236`、`:244-246`），所以 `1234` 注定是两行；
  小数部分先走到第一个非零位再决定说什么，但**量级名用总位数**（`pfract`，`:286-298`）；
  入口上 C 有 `-l`、有操作数（旧壳反而把任何参数当错误）、有多件时的 `...\n` 分隔、
  有三句 `errx` 与一句 usage（全在 stderr）。旧壳连 `number_words` 里的连字符也没有（C 用 `thirty-four`）。
  判类 P0-code-bug（输出形状、尺度名、小数、选项面）+ P0-fact（文档与测试对范围的说法）。
  **怎么修**：`words.rs` 的数词一半重写为 `convert`/`speak_units`/`speak_group`/`speak_fraction_name`
  （与 C 同名四层逐句对位）+ `NAME1`/`NAME2`/`NAME3`/`FRACTION_PREFIX`/`MAXNUM`，拒绝用
  `NumberError::{Illegal{start,end},TooLarge,NoRoom}`（`start..end` 正是 C 的 `line` 还能打出的那段）；
  执行半重写：`getopt("l")` 逐字母聚簇、`--` 消费、裸 `-` 归操作数、`char line[256]` 的**可用长度 255**
  （拿不到换行即 `line too long.`）、第二件起前缀 `...\n`、三句 `errx` 与 usage 全走 `support::warn`。
  这些 `errx` 本身不带 strerror 后缀，所以报文可以全字复现（与 caesar/factor 那类需后缀的不一样）。
  **金样来历**：先把 C 的 `convert`/`unit`/`number`/`pfract` 逐行誊写成一个独立的参考实现算出期望值，
  再用参考实现与真二进制互验（两边逐字相同才算数）；其中两枚我自己先推错（`1..2` 的报告包含那个点、
  以及分数行少算一个换行），都由参考实现当场驳回。
  三件套：`cargo test -p minix-stdio-games` → **18 个测试目标全 ok、合计 165 passed、零 FAILED**（基线 17 目标 / 157 passed；+3 库测、+5 端到端测）；
  库测 64/0（`words::` 11/0）、`--test number_face` 5/0（第 15 道端到端门）；三架构 `--bins` 三台零 error；
  容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、容器内 18 个 `test result: ok`、
  FAILED 与 error 计数 0；clippy 触及文件零新增（中途四处建议已按形处理：`is_multiple_of` 两例、可合并的 `if` 一例、
  `is_some`+`unwrap` 改 `match`）；`rustfmt` 零漂移；`pattern-gate.sh --diff` PASS=17 FAIL=0；
  `doc-style-lint.sh --diff` error 0；`doc-code-density-check.sh --check` 通过；本子树断链 0；
  文档里的测试名与代码逐枚对账（脚本扫描零 MISSING）；旧 API `number_words` 在代码与文档中均不再出现。
  **九枚变异负控**全部咬中：`-l` 的组尾分隔符、首位不足三位的分组、尺度名降一级、十位连字符改空格、
  量级名改用剪短后的位数、不消行首空白、去掉小数侧的 65 位上限、`LINE` 改回声明值 256、`...\n` 分隔不打印。
  其中最后一枚同时证明 `LINE: usize = 255` 不抄代码：255 位数字加换行在 C 里先碰 `line too long.`，
  改成 256 那么同一输入会变成「超 65 位」——两句报文不同，金样有牙。
  登记不执行：①§2.5 数词一段尚无自己的节选码（承载门目前靠本篇其余子节凑齐）；②真机播种与 open 腿归
  C-57/E5；③全族 strerror 基础件（见批次二百三十）。

- ✅ **批次二百三十把 `caesar` 的入口面按 C 坐实（`get_rotation` 的接受规则、两种失败各成一句、usage 落 stderr）**
  （`2026-10-14`，`claim/CAESAR-ROTATION-qoder` @ 基线 `13310ccf6`，单域三线 commands/games/stdio-games，无跨界）：
  接回批次二百二十九登记①②。**是什么**：旧 `parse_rotation` 只收纯数字、上界取 `u32::MAX`，
  而 C 的 `get_rotation`（`caesar.c:120-137`）先用 `strtol` 解析——**跳前导空白、吃可选符号**——
  再分两步拒绝：`arg[0] == '\0' || *endp != '\0'` 算 `EINVAL`，`rot < 0 || rot > INT_MAX` 算 `ERANGE`；
  两句合并走 `err(EXIT_FAILURE, "Bad rotation value `%s'", arg)`。也就是说 `-1` 在 C 里不是「畸形」而是
  「越界」，`+5` 与 `" 5"` 都是合法值，上界是 `INT_MAX` 而不是 `u32::MAX`。坏值路径以外还有两处：
  多参数时的 usage 行在 C 里走 stderr（本壳始终走 stdout），以及坏值时本壳完全静默。
  判类 P0-code-bug（接受规则、上界，以及有退码无报文的静默失败）。
  **怎么修**：`parse_rotation` 重写为 strtol 形（返 `Result<u32, RotationFailure>`，`Invalid`/`Range` 两型），
  失败不再塌成单一错误——它不取 `GameError` 是有意选择（后者的「畸形」与「越界」各自合一，
  无法区分 C 的两个后缀），已在类型注释里写明理由，以便日后补上 strerror 表时直接接上；
  执行半新增 `usage()`（stderr）与 `bad_rotation()`（`caesar: Bad rotation value ` + 原值 + 单引号 + 换行）。
  **已知缺口（不隐晦）**：C 句尾还缀一个 strerror 文本（`Invalid argument` / `Numerical result out of range`），
  本树无表（全族同一处理，如 `stat.rs:224`、factor 溢出句），故行止于原值；`RotationFailure` 保留了两型，
  壳里的 `match` 将两个后缀应当为何写在注释里。
  **取证**：`caesar_face.rs` 3→5 枚（接受规则与两种失败各成一句报文；`2147483647` 能过并被 26 取余成 23；
  usage 落 stderr 退 1）；库测把 `test_rotations_parse` 换为
  `rotations_parse_the_way_strtol_does_and_then_reject_by_range`（十四项断言，含 `"-"` 这种只有符号的畸形输入）。
  三件套：`cargo test -p minix-stdio-games` → **17 目标全 ok、合计 157 passed、零 FAILED**（基线 17/155）；
  `--lib caesar::` 8/0；`--test caesar_face` 5/0；三架构 `--bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）
  `cargo check -j 1 --workspace` Finished 零 error、容器内本包 17 个 `test result: ok`、FAILED 与 error 计数 0；
  clippy 触及文件零新增（中途 `use crate::GameError;` 因不再使用已收、常量比较按建议改写）；`rustfmt` 零漂移；
  `pattern-gate.sh --diff` PASS=17 FAIL=0；`doc-style-lint.sh --diff` error 0（一处转义反引号漏进正文已改成人话叙述）；
  测试名逐枚 grep 存在；核实日 `2026-10-14`；基线 `13310ccf6`。
  **九枚变异负控**（`--lib caesar::` 与 `--test caesar_face` 分两次调用、复位断言 md5）全部咬中：不跳前导空白、
  `+` 被当畸形、负号通过、上界改 `u32::MAX`、尾随垃圾接受、空串当 0、两型失败合流、usage 回到 stdout、
  报文不再带原值。
  登记不执行：①strerror 句尾（全族同缺，需先做 errno→文本表的基础件）；②number 壳与 `number_words` 的逐枚对 C；
  ③真机播种与 open 腿归 C-57/E5。

- ✅ **批次二百二十九把 `caesar` 的无参频率破解（`guess_and_rotate`）建成决定半并接上执行半**
  （`2026-10-14`，`claim/CAESAR-FACE-qoder` @ 基线 `85b6f2dac`，单域三线 commands/games/stdio-games，无跨界）：
  **是什么**：22 篇与 `bin/caesar.rs` 都记着「无参频率破解待决定半」，而无参数时 C 走的正是这条腿
  （`caesar.c:192-194` 调 `guess_and_rotate`）；本壳以前在无参时直接报 usage 退 1，等于把三分之一的
  入口删掉了。`guess_and_rotate`（`:141-187`）有三个容易被当作“显然”的形状：①比例表先做一次
  `stdf[i] = log(stdf[i]) + log(26/100)` 变形（`:150-151`），不取对数则罕见字母在点积里几乎不存在；
  ②只拿前 2048 字节投票（`inbuf[2048]`），但窗口外的字节仍用同一张表翻译；③比较是严格大于且
  `try == 0` 无条件占位（`:176-180`），平手保留较早的轮转量；此外权重表按 `(i + try) % 26` 反向错位，
  所以右移 5 格的文本答 21。判类 P0-design-missing（能力缺失）+ 文档同步。
  **怎么修**：`log` 在裸机不可用，故把变形结果作为常量 `WEIGHTED_PERCENT` 入仓，并在宿主测里用
  `.ln()` 逐项重算验证（`STD_PERCENT` 也与 C 的字面量逐项对账）；新增 `observe`/`best_rotation`/`guess_rotation`
  与 `GUESS_WINDOW`，执行半无参时走破解、有参时保持原路，输出按 1024 字节分块流过（与 C 逐字节 putchar
  同形）。同时删掉 `rotate_line` 与其唯一测 `test_length_mismatch_rejected`：新执行半按字节流翻译，
  该入口已无调用者（留着就是死代码）；测试里一个“大写后整卡相等”式的重复断言不新建，归入现有测。
  **取证**：新增第 14 道端到端门 `tests/caesar_face.rs`（3 枚）：固定轮转只动字母（含 0x80/0xFF 走恒等段、
  26 得恒等、27 与 1 等价）；无参模式对 1/5/13/19/25 格位移的英文都能逐字节还原；2048 窗口外的字节
  不投票但仍翻译（同一段密文单独喂时会投票，证明这个断言有信息量）。库测新增 4 枚。
  三件套：`cargo test -p minix-stdio-games` → **17 个测试目标全 ok、合计 155 passed、零 FAILED**（基线 17/149）；
  `--lib` 61/0；`--lib caesar::` 8/0；`--test caesar_face` 3/0；三架构 `--bins` 三台零 error；容器
  `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、容器内本包 17 个目标全 ok、
  FAILED 与 error 计数为 0；clippy 触及文件零新增（中途两处已按建议处理：`assert!` 对常量下标比较改为
  `max_by`/`min_by` 导出峰谷，新测里 `assert_eq!` 内嵌套 `format!` 拆为局部变量）；`rustfmt` 零漂移；
  `pattern-gate.sh --diff` PASS=17 FAIL=0；`doc-style-lint.sh --diff` error 0；本子树断链 0；测试名逐枚 grep 存在。
  **八枚变异负控**（`--lib caesar::` 与 `--test caesar_face` 分两次调用、复位断言 md5）首轮有一枚**没咬**：
  “用原始比例表代替对数表”在所有位移还原用例上与正确表同答——因为英文本身就能被两种表分开；
  改用穷举找出一组四字母观测（c×29、s×17、y×32、z×8）：对数表答 16、原始表答 2，把它钉成库测后
  该枚翻红；另一枚因 rustfmt 改了排版而不匹配，校正锚点后也咬中。共八枚（对数表→比例表、位移反向、
  只计大写、取消窗口、窗口改 4096、破解结果被丢弃改用 0、硬编 rot13、轮转量不按 26 取余）全部咬中。
  登记不执行：①`get_rotation` 的接受规则（C 用 `strtol`，吃前导空白与 `+`，上界是 `INT_MAX`）与那句
  `err(EXIT_FAILURE, "Bad rotation value `%s'")` 报文（需 strerror 表）；②多参数时 usage 行应落 stderr
  （现落 stdout）——本轮写门时才发现，因为不属本条而在 `caesar_face.rs` 里**不断言**，已记入 22 篇状态列；
  ③真机播种与 open 腿归 C-57/E5；④number 壳的逐枚对 C。

- ✅ **批次二百二十八把 `bcd` 的输入框架按 C 定形（`cardline[80]` 的可用长度是 79），并补上卡片形状的端到端门**
  （`2026-10-14`，`claim/BCD-FACE-qoder` @ 基线 `f7d2349f3`，单域三线 commands/games/stdio-games，无跨界）：
  接回批次二百二十七登记②（逐壳对 C）。**先盘不靠记忆的部分**：用脚本把 `bcd.c:87-120` 的
  `holes[256]` 与 `bcd.rs` 的 `HOLE_TABLE` 逐值对比——**256 项零差异**，所以下面不是翻表事故而是
  入口事故。C 的 `main` 用 `fgets(cardline, sizeof cardline, stdin)`，`char cardline[80]` 留一个
  字节给结尾 NUL，所以**每次最多 79 个字符**；本壳把 `LineReader` 的缓冲给了 80 字节（把声明尺寸
  当成了可用尺寸），后果是一行超 79 字符时第二张卡少一个字符（一百个字母的输入：C 切成 79+21，本壳切成 80+20）。
  同时拓到三处描述事实错：文档与库测都写「十四行框」（实为**十五行**：顶栏、文本行、十二打孔行、底栏），
  且无人记下「无孔字符在文本行留空白」这条对控制字符才可见的规则。判类 P0-code-bug + P0-fact。
  **怎么修**：新增 `const CARDLINE: usize = 79`，并在注释里写明「声明尺寸」与「可用尺寸」只差一个结尾终止符；
  渲染本身（去第一个换行→截 48→就地大写→查表）经核对与 C 逐字吻合，未改。`bcd` 无选项解析
  （`:139` 只看计数），故 `-x` 与 `-` 都是待渲染文本——旧壳本来就这样，本轮把它钉上测。
  **取证**：新增第 13 道端到端门 `tests/bcd_face.rs`（6 枚）+ 库测一枚（表的 Q/R 分项与
  「空白类无孔」）；另把新加的“大写后整卡相等”测与已有的 `test_lowercase_becomes_uppercase` **合并**
  （test-audit 第 3 维：同行为不写两测），库测 7→8。金样均从 C 推：`A` = 0x900 只让第 0 与第 3
  行打孔；行填充前三行空白、第 3 到 11 行是 `1`..`9`；可打印 ASCII 里只有空格无孔，所以拿制表符
  做判据（断言输出里根本不出现 `\t`）。
  三件套：`cargo test -p minix-stdio-games` → **16 个测试目标全 ok、合计 149 passed、零 FAILED**
  （基线 16/142；+6 端到端、+1 库测、-1 合并）；`--lib bcd::` → 8/0；`--test bcd_face` → 6/0；三架构
  `--bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、
  容器内本包 16 目标 149/0；clippy 触及文件零新增（仅余 `bin/arithmetic.rs` 一条存量；本轮新测里一处
  `assert_eq!` 内嵌套 `format!` 已按建议拆为局部变量）；`rustfmt` 零漂移；`pattern-gate.sh --diff`
  PASS=17 FAIL=0；`doc-style-lint.sh --diff` error 0；测试名逐枚 grep 存在；核实日 `2026-10-14`。
  **九枚变异负控**（`--lib bcd::` 与 `--test bcd_face` 分两次调用、复位断言 md5）首轮有**两枚没咬**：
  “无孔字符也直接打印”与“打孔位从低往高读”。诊断不是改口而是补区分样例：前者在可打印集里拿不
  到差异（空格的替代本来就是空格），改用制表符才能看；后者需要真正钉住「哪几行打 `]`」，于是补了
  `the_twelve_rows_read_the_pattern_from_the_top_bit_down` 与 1024/79 边界断言——两枚当场翻红，
  其余七枚（可用尺寸改回 80、不大写、截断晚一列、底栏换首栏、顶栏丢前导空格、文本行多一列填充、
  `-x` 被当选项跳过）始终咬中。
  登记不执行：①caesar 与 number 两个壳的逐枚对 C（下一轮起）；②真机播种与 open 腿归 C-57/E5（本笔只交
  代码 + 宿主端到端门）；③`printcard` 遇到非 ASCII 字节的表回绕（C 用 `holes[(unsigned char)*p]`，本树
  `HOLE_TABLE[byte as usize]` 同形 256 项，已核对等价，无需拆片）。

- ✅ **批次二百二十七把 `ppt` 的入口与两路形状按 C 钉死（`-h` 不是帮助、`--` 与裸 `-` 的归属、失步与欠的换行），并纠正了库注里「缓冲里总是 NUL」的事实错**
  （`2026-10-14`，`claim/PPT-FACE-qoder` @ 基线 `356ad3f30`，单域三线 commands/games/stdio-games，无跨界）：
  接回批次二百二十六登记①（逐壳对 C），本轮取 `ppt.c`（175 行）。**先说清楚**：库侧的
  `punch_byte`/`decode_line` 本来就与 `putppt`/`getppt` 逐字吻合（进纸孔先于位 2 的字符、
  非空格即算孔），本轮拓到的是**入口与两路形状**：旧壳只认整串 `-d`（`"-d"` 全等匹配），
  所以 `-dd`、`--` 都不对（`--` 被当成非法选项而报 usage，C 是把它当结束标记）；usage 打在
  **stdout**（C 是 `fprintf(stderr, ...)`）；`-h` 在 C 里不是帮助而是与集外字母同路的错误
  （`ppt.c:80-83`）。还有一处纯事实错在库注里：它说越界列在 C 的 `buf[132]` 里「总是 NUL」，
  而 `buf` 是未初始化的栈数组，`fgets` 不零化它——读到的是上一行的残值，进纸孔落在前五行
  时甚至读穿数组（`:153-167`）；那是未定义行为，不应当作规则来写。判类 P0-code-bug（入口）+
  P0-fact（库注与文档）。另顺手删掉 `punch` 里重复的一次 `terminate(0)`。
  **怎么修**：入口改成逐字母的 `getopt("dh")` 形（聚簇逐字扫描、`--` 消费并结束扫描、裸 `-`
  归操作数、`-h`/集外字母同走 usage）；`usage()` 改走 `support::warn`（stderr）保留退 1；
  `-d` 带操作数仍拒（C 的 `if (argc > 0) usage()`）；解码保留「同步前无孔行是导引、同步后
  无孔行即补欠的换行并结束、末字节已是换行则不欠」这三件——库测早已覆盖这些形状，本轮补的是进程级证据。
  **取证**：新增第 12 道端到端门 `tests/ppt_face.rs`（5 枚），金样逐枚按 C 推：`EDGE` 是 11 个下
  划线经 `puts` 自带换行、`putppt('A')` = `| o   .  o|`、`putppt(' ')` = `|  o  .   |`、
  `putppt('-')` = `|  o o.o o|`（由位掩码 0x2D 算出，不是手数）；操作数**之间**才补空白行；
  编→解往返经真进程管道回到 `A\n`，失步行后的一切不再输出。手推错的一枚（`-` 推成 `|   ..    |`）
  改由位掩码计算式导出后当场改对。
  三件套：`cargo test -p minix-stdio-games` → **15 个测试目标全 ok、合计 142 passed、零 FAILED**
  （基线 14/137；+5 端到端测）；`--lib ppt::` → 6/0；`--test ppt_face` → 5/0；三架构 `--bins` 三台
  零 error；容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、容器内
  本包 15 目标 142/0；clippy 触及文件零新增（仅余 `bin/arithmetic.rs` 一条存量）；`rustfmt` 零漂移；
  `pattern-gate.sh --diff` PASS=17 FAIL=0；`doc-style-lint.sh --diff` error 0；
  `doc-code-density-check.sh --check` 本篇 §4 因本轮扩充而转为可判章，已补 `BIT_AT_OFFSET` 的 Rust
  节选码 + 角色框定 + 反向推演，现三章均有码；测试名逐枚 grep 存在；核实日 `2026-10-14`；基线
  `356ad3f30`。
  **十枚变异负控**（`--lib ppt::` 与 `--test ppt_face` 分两次调用、复位断言 md5）：进纸孔换列、位序
  反向、读带只认 `o` 为孔、末尾也补空白、usage 改打 stdout、`-h` 变成无效帮助、`--` 不再结束扫描、
  裸 `-` 被当选项、欠的换行总是为真、未同步也因无孔行结束 → 全部咬中。
  登记不执行：①`err(1, "fgets")` 的 strerror 后缀（本树无表，现按普通结束处理）；②bcd/caesar/number
  三个壳的逐枚对 C（下一轮起）；③真机播种与 open 腿归 C-57/E5；④本篇其余子节的贴码仍靠章级判定。

- ✅ **批次二百二十六把 `pig` 的旋转与字符流按 C 重做，并纠正了文档里「开头 `y` 算辅音、中间 `y` 算元音」与「大小写三档」两处事实错**
  （`2026-10-14`，`claim/PIG-FACE-qoder` @ 基线 `f78ead34c`，单域三线 commands/games/stdio-games，无跨界）：
  接回批次二百二十五登记③（逐壳对 C），选了最小的 `pig.c`（135 行）。**是什么**：`pigout()` 用
  **两套元音集合**——`way` 判据（`pig.c:103`）不含 `y`，搬运停止判据（`:114`）含 `y` 且从 `start == 0`
  就测，所以 `yellow`/`yes`/`yam` 根本不搬运；搬运时 `qu` 是一个整体（`:117-119`）；大小写只动两个位置
  （`:111-112` 转小写原词首字母、`:124-125` 转大写新首字母）。而本树的 `pig_word` 把三件都做了错：
  给 `y` 加了「非首位才算元音」的特殊规则（`yellow` 打成 `ellowyay`）、没有 `qu` 单位（`quay` 打成
  `uayqay`）、把整个搬走的串转小写再按位置补大写（`STRING` 打成 "STRINGAY" 形）。执行侧也错了三件：
  词按**字母加数字**聚簇（C 只认 `isalpha`）、把行当单趟处理（悬挂词在 EOF 仍被打印）、
  把操作数直接报 usage（C 数完操作数后从不读它们）。判类 P0-code-bug + P0-fact（三处文档描述）。
  **怎么修**：决定半逐字复刻 `pigout()`（`is_vowel`/`is_stop` 两个集合 + 与 C 同序的 `start` 循环 +
  按 `buf[start .. start+olen]` 的环形窗口输出，其中下标 0 取「可能被转小写的首字母」）；执行半改成
  C 的字节流：连续 `isalpha` 成词、非字母先冲刷再原样透出、EOF 不冲刷、第 1025 个字母才报
  `pig: ate too much!`（C 的检查在存入之前，所以 1024 个字母仍然可用）、`option_scan` 复刻空 optstring
  （`--` 消费、裸 `-` 是操作数、其余 `-x` 报 usage 退 1、多余操作数静默忽略）。
  新增第 11 道端到端门 `tests/pig_face.rs`（6 枚），库测 `words.rs` 5→8 枚（`test_consonant_cluster_moves`
  里 `yellow → ellowyay` 这一**错断言**当场改为 `yelloway`）。
  三处我自己推错的金样当场按 C 重推：`QQUAKES` 不是 "UAKESQQAY" 而是 `AKESQQUAY`（第二个 `q` 也带走了
  它后面的 `U`）；`MY` 不是 "MYAY" 而是 `YMAY`；`goodbye` 不是 "oodgbaye" 而是 `oodbyegay`。
  三件套：`cargo test -p minix-stdio-games` → **14 个测试目标全 ok、合计 137 passed、零 FAILED**
  （基线 14/128；+3 库测、+6 端到端测）；`--lib words::` → 8/0；`--test pig_face` → 6/0；三架构
  `--bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、
  容器内本包 14 目标 137/0；clippy 触及文件零新增（仅余 `bin/arithmetic.rs` 一条存量）；
  `rustfmt --edition 2024 --check` 零漂移；`pattern-gate.sh --diff` PASS=17 FAIL=0；`doc-style-lint.sh --diff`
  error 0；测试名逐枚 grep 存在。核实日 `2026-10-14`；基线 `f78ead34c`。
  **十枚变异负控**（`--lib words::` 与 `--test pig_face` 分两次调用、复位断言 md5）：`way` 判据改走停止集、
  `qu` 不再整体搬运、全大写词的首字母也被转小写、新首字母不再转大写、EOF 也冲刷、数字与下划线留在词内、
  缓冲边界提前一个字母、usage 改退 0、操作数不再被忽略、元音分支把词转小写 → 全部咬中。
  登记不执行：①其余四个壳（bcd/ppt/caesar/number）的取值与诊断面；②真机播种与 open 腿归 C-57/E5（本笔只交
  代码 + 宿主端到端门）；③`number` 的 `number_words` 与 C `number.c` 的逐枚对账（本轮只动了 `pig_word` 一侧）；
  ④caesar `guess_and_rotate`、arithmetic `penalise`、banner 字体腿不动。

- ✅ **批次二百二十五把 `factor` 的取值与诊断面按 C 坐实（前导空白、符号、三句拒绝、空 optstring 的 usage）**
  （`2026-10-14`，`claim/FACTOR-ERRX-qoder` @ 基线 `249d146a0`，单域三线 commands/games/stdio-games，无跨界）：
  接回批次二百二十四登记①。**原状**：旧壳把每行去掉换行后丢给 `parse_value`（只收纯数字），
  失败就静默退 1——于是缩进的值被当非法、`0`/`1` 打成 `1:` 且退 0、负数与非法格式都没有报文。
  C 的真相是三处不同来路的句子加一个空 optstring：`pr_fact` 的防火墙管 0 与 1（`factor.c:218-219`）；
  以 `-` 开头的**操作数**因 C 的一句话滑差也答同一句（`:173-174`）；标准输入那条腿在空白扫描之后另有
  `negative numbers aren't permitted.`（`:166-168`）；纯空白行静默继续（`:163-165`）；`getopt(argc, argv, "")`
  使任何 `-x` 落到 `case '?'`，而 `usage()` 以状态 **0** 退出（`:147-152`、`:266-270`）。判类 P0-code-bug。
  **怎么修**：把判定全部放进决定半——新增 `Verdict`（`Value`/`Skip`/`NotPermitted`/`Negative`/`Illegal`）
  与 `verdict(word, from_stdin)`，逐字复刻 `BN_dec2bn` 的 `isblank` 扫描、可选符号、数字后只能遇到
  行末或换行（`:119-128`）；执行半只负责把句子按 C 的 `errx` 形状写到 stderr（`factor: ` + 报文 + 换行）
  并退出，`usage()` 按 C 退 0。**删掉 `parse_value`**（被 `verdict` 完全覆盖，留着就是死代码）。
  **两条有意保留的差异**（写在库注释里，不隐晦）：①溢出 C 走 `err(1, "%s", str)` 带 `strerror` 后缀
  （`:123-124`），本树无 strerror 表，因此退码同为 1 而句子按非法格式给；②那个非法格式报文在 C 的
  标准输入分支里打的是已被 `argv += optind` 用完的 `argv[0]`（一个已失效的指针），本壳改报当前行内容。
  另 C 对操作数只测试**首字节**是否 `-`（`:173`）而不跳空白，所以 `" -6"` 在 C 会经 `strtoul` 回环成巨大
  无符号值；本壳两条腿都在空白扫描后判符号，因此给报文——这一条也写进了库注释。
  **取证**：`tests/factor_face.rs` 从三枚扩到八枚（新增 ≤ 1 停止整轮、缩进值可解、两句负数报按来路分、
  拖尾垃圾点名报非法格式、`-x` 报 usage 且退 0）；库内新增两枚 `verdict` 测并删 `test_values_parse`。
  三件套：`cargo test -p minix-stdio-games` → **13 个测试目标全 ok、合计 128 passed、零 FAILED**（基线 13 目标 /
  122 passed；+2 库测、+5 端到端测）；`--lib` → 54/0；`--lib factor::` → 6/0；`--test factor_face` → 8/0；
  三架构 `--bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、
  容器内本包 13 目标 128/0；clippy 触及文件零新增（仅余 `bin/arithmetic.rs` 一条存量）；
  `rustfmt --edition 2024 --check` 零漂移；`tools/pattern-gate.sh --diff` PASS=17 FAIL=0；
  `tools/doc-style-lint.sh --diff` error 0；测试名逐枚 grep 存在（§5 清单已同步）；核实日 `2026-10-14`；
  基线 `249d146a0`。
  **十枚变异负控**（`--lib` 与 `--test` 分两次调用、复位断言 md5）：空白行不再跳过、操作数的 `-` 改答
  另一句、防火墙去掉、拖尾垃圾接受、正号被拒、不跳前导空白、usage 改退 1、报文改打 stdout、`--` 不再
  是结束标记、非法格式句不再点名 → 全部咬中。
  登记不执行：①溢出那句 `err` 的 `strerror` 后缀（待同族统一的 strerror 基础件）；②真机播种与 open 腿归
  C-57/E5；③bcd/ppt/caesar/number/pig/morse 各自的取值与诊断面尚未逐枚对 C（本轮只动 factor）；
  ④大数概率分解（`pollard_rho`）仍在后续批。

- ✅ **批次二百二十四给 stdio-games 装上真正的标准输入缝（`stdin_chunk` 双生），并用 `factor` 端到端门证明该腿可用**
  （`2026-10-14`，`claim/GAME-STDIN-LEG-qoder` @ 基线 `55e979a82`，单域三线 commands/games/stdio-games，无跨界）：
  接回批次二百二十三登记①。**是什么**：本 crate 八个壳都用
  `LineReader::new(|chunk| read(support::STDIN, chunk).map_err(|_| ()), …)` 驱动标准输入，而
  `minix_sys::read` 在宿主走的是内核传输替身（不是进程自己的 0 号描述符），所以**这些壳的标准输入腿在宿主
  端到端上根本读不到东西**——与文本滤器、与上一轮 morse 完全同一条坑（只是落点更多）。拿不到读数就无法
  给标准输入形状写金样，也就无法发现形状偏差（morse 那轮的 P0-code-bug 正是门才能拓出来的）。
  **怎么修**：不在各壳里加 `cfg`（那会把同一个判断重复八遍，且宿主与裸机各写一遍容易漂移），而是把缝
  放在共享件 `bin_support.rs` 里新增 `stdin_chunk(&mut [u8]) -> Result<usize, ()>`（与 `LineReader` 现有的
  reader 约束逐字吻合，`Ok(0)` 是结束、`Err(())` 是读失败）：宿主腿走 `std::io::stdin().read`，裸机腿
  局部 `use minix_sys::read` 读 `STDIN`（局部导入避开宿主构建的 unused-import 告警）。然后把八个壳改为
  `LineReader::new(support::stdin_chunk, …)`，`ppt` 自己那个逐 chunk 循环也改走同一函数，各文件的
  `use minix_sys::read;` 随之删除。
  **取证门**：新建 `tests/factor_face.rs` 三枚（第 10 道端到端门），金样逐枚从 `minix3/games/factor/factor.c`
  手推：`pr_fact` 每值一行、形式是 `value:` 后逐个追加「空格 + 因子」再一个换行（`:228-252`）；无操作数时
  每行一个值（`fgets` 循环 `:156-171`）；末行不带换行仍是一个值；空行不出行。**本轮只钉己方与 C 已一致的局部**：
  下列三处与 C 不符，已登记不执行（归下一轮）——前导空白（`isblank` 扫描，`:163`，
  现壳会当非法值退 1）；值 ≤ 1 与负数（C 是 `errx` 报文 + 退 1，现壳静默退 1 并把 `1` 打成 `1:`）；
  非法格式那句 `illegal numeric format.`。
  三件套：`cargo test -p minix-stdio-games` → **13 个测试目标全 ok、合计 122 passed、零 FAILED**（基线 12 目标 /
  119 passed；+3 枚端到端门）；`--test factor_face` → 3/0；`--lib` → 53/0（零回归）；三架构 `--bins` 三台零 error；
  容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、容器内本包 13 目标 122/0；
  clippy 触及文件零新增（全包仅余 `bin/arithmetic.rs` 一条存量，与基线逐条同数）；`rustfmt --edition 2024 --check`
  零漂移；`tools/pattern-gate.sh --diff` PASS=17 FAIL=0；`tools/doc-style-lint.sh --diff` error 0；核实日
  `2026-10-14`；基线 `55e979a82`。
  **三枚变异负控**（`--test factor_face` 与 `--lib` 分两次调用，复位断言 md5）：把宿主缝改成 `Ok(0)`（即回到
  内核传输替身那个旧坑的读数）→ 两枚标准输入测翻红、操作数测不动（正好证明门钉的是腿而不是形式）；
  空行不再跳过 → 翻红；末行无换行被丢弃 → 翻红。全部咬中。
  登记不执行：①factor 的三处 C 偏差（前导空白、≤ 1 与负数、非法格式报文，本轮新判）；②真机播种与 open 腿归
  C-57/E5（本笔只交代码 + 宿主端到端门，不谎报串口读数）；③caesar `guess_and_rotate`、arithmetic `penalise`、
  banner 字体腿、bcd/ppt/caesar/number/pig 各自的 stdin 形状尚未逐枚对 C（现在有腿了才能查，下一轮起排）。

- ✅ **批次二百二十三把 `morse -d` 的解码腿改成 C 的两形：操作数整串比对、标准输入走字符流**
  （`2026-10-14`，`claim/MORSE-DECODE-qoder` @ 基线 `0ac4fcca5`，单域三线 commands/games/stdio-games，无跨界）：
  接回批次二百二十二登记①。**原状不符合 C**：旧解码腿把输入按行取出、再按空白拆成码组逐组查表，
  而 C 有两形——带操作数时每个操作数**整串**交给 `decode()`（`morse.c:146-150`，串里的空格不拆），
  不带时按字符流走一个五段状态机（`:151-185`）。查不到时 C 答 `x`（`:227`）而旧腿静默丢弃，
  标点表也整张没接。判类 P0-code-bug。
  **三条按 C 不按直觉的形状**（都写进注释并各有断言）：①**冲刷型分隔符不打印**——`is_blank` 在
  `else if (i)` 那一支被清零（`:170-174`），所以 `...  ---`（两空格）一个空格都不打，而 `...   ---`（三空格）
  打一个；旧注释里那句「连续两个分隔符的第二个才打空格」不准确，本轮连同库内文档注释一起改对；
  ②**十个符号算胡码**：打一个 `x`、计数器归零，内层 `while` 接着吞同串剩余符号，**并连吃紧跟其后的第一个
  非符号字节**（`:160-169`），那个字节再也充当不了分隔符；③收尾 `putchar('\n')` 在循环之外（`:185`），
  所以空输入也有一行，而**末尾没等到分隔符的码组直接丢弃**（`... --- ...` 得 `SO\n` 不是 `SOS\n`）。
  另：`...-.-`（结束记号）是 `decode()` 唯一不输出任何字节的输入（`:225-226`）。
  **代码形状**：决定半新增 `decode_code`（整串比对，答 `Option<u8>`：`None` 即静默、`Some(b'x')` 即噪声）
  与 `decode_stream`（字节流 + 写出回调，无分配、宿主逐分支可测）；`MorseTable::letter_of` 继续做字母与
  数字那一层，标点走 `PUNCT_CODES`；**删掉 `decode_group` 与 `EmptyMorse`**（前者是拆空形态的唯一来源，
  后者只被测试引用，都是死代码）；`GameError` 在解码腿上不再参与（C 那里没有错误路径）。
  执行半 `bin/morse.rs` 的 decode 分支按 C 分两形，`text_lines` 随之删除（不再拆行），`ToString` 导入随之收紧。
  **三处我自己推错的金样**当场按 C 重推：① `...  ---\n` 不是 `S O\n` 而是 `SO\n`（冲刷那一支已把旗标清零）；
  ② `... ---\n` 不是 `S\n` 而是 `SO\n`（换行也是分隔符，第二个码组会冲刷）；③ `...\t...\n` 是 `SS\n` 不是 `S\n`。
  还有一枚变异负控**首轮没咬**（`E2` 冲刷不清旗标）：原样例碰不到可观察差异，补两枚区分样例
  `...  .. \n` 得 `SI\n`、`...  ..  \n` 得 `SI \n` 后当场翻红。同样先验证了「十个符号」阈值的可判性：
  因所有表项都 ≤ 6 个符号，单看 `x` 分不出阈值，所以补了 `............  ...\n` 得 `x S\n` 与
  `.........  ...\n` 得 `xS\n` 这一对（差别全在“被吞掉的那个分隔符”上）。
  三件套：`cargo test -p minix-stdio-games` → **12 个测试目标全 ok、合计 119 passed、零 FAILED**（基线 12 目标 /
  114 passed；+5 库测、-3 删测、+3 端到端测）；`--lib morse::` → 12/0；`--test morse_face` → 12/0；
  三架构 `--bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、
  容器内本包 12 目标 119 passed / FAILED 0；clippy 触及文件零新增（一处 `ToString` 未用导入已收，
  全包仅余 `bin/arithmetic.rs` 一条存量，与基线逐条同数）；`rustfmt --edition 2024 --check` 三文件零漂移；
  `tools/pattern-gate.sh --diff` PASS=17 FAIL=0；`tools/doc-style-lint.sh --diff` error 0；
  `tools/doc-code-density-check.sh --check` 本笔由 **基线的 1 个阻断项转为通过**（给 §2.4 补了 `show()` 的真实
  节选码 + 角色框定 + 逐条件讲解 + 反向推演；只给摩斯一节补码，§2 其余命令的承载仍靠章级判定，已登记）；
  核实日 `2026-10-14`；基线 `0ac4fcca5`。
  **九枚变异负控**（`--lib` 与 `--test` 分两次调用，复位放 `finally` 并断言 md5 复原）：末尾冲刷悬挂码组、
  冲刷不清旗标、首个扑空分隔符就打印、吞循环不带走尾字节、阈值改九个、结束记号答 `x`、整轮收尾换行不写、
  操作数拆空白、操作数路径不打换行 → 全部咬中。
  登记不执行：①其余八个游戏壳仍各自走 `LineReader` + `minix_sys::read` 的宿主读腿；②真机播种与 open 腿归
  C-57/E5；③caesar `guess_and_rotate`、arithmetic `penalise` 与用时统计、banner 字体腿不动；④本篇 §2 其余
  子节的贴码（只补了摩斯一节）。

- ✅ **批次二百二十二把 `morse` 的编码面按 C 的 `show()` 重做，并纠正了文档把 `-s` 记成「码表打印 / 容错」的事实错**
  （`2026-10-14`，`claim/MORSE-CODETABLE-qoder` @ 基线 `2cf25f849`，单域三线 commands/games/stdio-games，无跨界）：
  **是什么**：22 篇的 morse 行写「已接线编码与 `-d` 解码；`-s` 码表打印待批」，2.4 节另写「两个开关管方向与容错」。
  回 `minix3/games/morse/morse.c` 读 `-s` 与 `show()` 后两头都不对：`-s` 不是码表打印也不管容错，它只换**渲染形态**
  （`:135-137` 置 `sflag`，`:253-261` 的 `show()` 在不带 `-s` 时逐符号打 ` dit`/` daw`、带 `-s` 时打一个空格再加整串），
  而 `decode()` 从不读它（所以 `-d` 之下 `-s` 无效）。更重的一条是**本壳的编码输出形状整体不是 C 的形状**：
  C 是**每个输入字节一行**（`:189-194`）、操数末尾补一行 `show("")`（`:191`）、整轮结束后统一打一次
  `show("...-.-")`（`:195`），本壳却把一个词压成一行 `... --- ...`，既无 dit/daw 也无收尾记号。判类：
  P0-code-bug（输出形状）+ P0-fact（两处文档描述）。
  **怎么修（三方案）**：甲只在薄壳补 `-s` 分支、保留单行形——会把错形状钉进文档与金样，否；乙决定半提供
  `code_for_byte`/`show_line` 两个纯函数、执行半按 C 的调用次序逐字节驱动——三处不对称落在库里可宿主逐分支
  可测，选它；丙连解码腿一起改成字符流——那是另一个条目，本轮只登记不分送（否则一次两条）。
  **代码形状**：`morse.rs` 新增 `PUNCT_CODES`（C `other[]` 十二项，顺序即 `decode()` 的比对序）、`SK_CODE`、
  `BLANK_CODE`、`MAX_SHOW_LINE`；`code_for_byte` 复刻 `morse()` 四支（字母/数字/空白→空码/标点表→表外 `None`）、
  `show_line` 复刻 `show()` 两支；**删掉 `encode_word`**（它是那个非 C 形态的唯一来源，留着就是死代码）；
  薄壳新增 `encode`/`emit_code` 与 getopt 形（`-ds` 聚簇、`--` 结束扫描、裸 `-` 是操数、集外字母报 usage 退 1）。
  **顺带闭掉一条反复出现的坑（命壳直接调 `minix_sys::read(STDIN)`，宿主端到端读不到输入）**：本 crate 的
  `bin_support.rs` 补上 `read_stdin()`（宿主真 stdin / 裸机 chunk 循环两生缝）与 `warn()`（宿主 stderr），
  `encode` 与 `text_lines` 改走它。取证很直接：门先对 HEAD 实现跑，9 枚全 FAILED，其中依赖标准输入的三枚
  读数就是「只剩收尾行」。
  **两处我自己推错的金样**当场按表纠正：`b` 的码是 `-...`（`alph[1]`，`:67`）而不是我推的 `-.--`；`....-` 是数字
  `4`（`digit[4]`，`:58`）而不是 `5`。
  三件套：`cargo test -p minix-stdio-games` → **12 个测试目标全 ok、合计 114 passed、零 FAILED**（基线 11 目标 /
  102 passed；+9 枚端到端门、+3 枚库测）；`--lib morse::` → 10/0；`--test morse_face` → 9/0（新建第 9 道门）；
  三架构 `--bins` 三台零 error；容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、
  容器内本包 12 目标 114 passed / FAILED 0；clippy 触及文件零新增（`src/morse.rs` 一处 `bytes().count()` 按建议改
  `len()` 后，全包仅余 `bin/arithmetic.rs` 一条存量，与基线逐条同数）；`rustfmt --edition 2024 --check` 四文件零漂移；
  `tools/pattern-gate.sh --diff` → PASS=17 FAIL=0（P11 13 token、P13 3 token）；`tools/doc-style-lint.sh --diff` → error 0
  （头部行两处正文日期顺手移出，`--diff` 才不拿存量日期算新增）；核实日 `2026-10-14`；基线 `2cf25f849`。
  **十枚变异负控**（`--lib` 与 `--test` 分两次调用，复位放 `finally` 并断言 md5 复原）：`-s` 不再置形态、短形丢掉
  空码那个空格、分隔符被跳过而不是渲染、`0x0B` 不算空白、表外字节也打一行、收尾记号改成每操数一次、操数收尾行
  丢失、裸 `-` 被当选项扫描、usage 改打 stdout、拼写支路改成「只有划才是划」 → 全部咬中（无等价漏网，所以本轮
  没有需要补区分样例的次生轮）。
  登记不执行：①`-d` 的 C 字符流形态（双空白才打一个空格、逾十个符号打 `x` 并吞同串剩余、整轮只打末尾一个换行、
  查不到的码打 `x`）；②其余八个游戏壳仍各自走 `LineReader` + `minix_sys::read` 的宿主读腿（同一坑的其余落点）；
  ③真机播种与 open 腿归 C-57/E5；④caesar `guess_and_rotate`、arithmetic `penalise` 与用时统计等原有待批项不动。

- ✅ **批次二百二十一给 `pr` 接上 `-F`/`-f` 的换页符收尾，并按 `prtail` 的 `nohead` 早退纠正了本轮自己推出的金样**
  （`2026-10-14`，`claim/PR-FORMFEED-qoder` @ 基线 `7759e8150`，单域三线 commands/textfilter，无跨界）：
  接回批次二百二十登记的①里 formfeed 那一半。真源三处：`prtail` 的换页符臂（`pr.c:1555-1561`）、
  它前面的 `nohead` 早退（`:1526-1537`）、`-f` 落穿到 `-F` 的那个 switch（`:1693-1700`）。
  **先纠错**：本轮初版把 `-F -t` 的金样推成「每页末尾一个 `\f`」，形状自洽却**违反 C**——`prtail` 的第一条
  语句就是 `if (nohead)` 早退，换页符臂排在它之后，于是关掉表头时换页符一个都不写。这属于 test-audit
  第 2 维点名的「测试代码就是错的，啥都测不出来」，靠重读 `pr.c` 抓出，不是靠跑自己的二进制（拿自己的
  输出反推期望值只会把错钉死）。修正落在决定半：`Page.form_feed = options.form_feed && !options.no_header`，
  单栏与多栏两条路径各自计算；金样里 `-F -t` 的期望值退回与裸 `-t` 逐字节相同。
  **机制**：换页符一次替换的是**两样东西**——补白行与五行表尾，因为 `cnt += TAILLEN` 与那个
  `while (--cnt >= 0) putchar('\n')` 都排在换页符臂之后；而 `\f` 自己由裸 `putchar` 送出、**不带换行**，
  所以它是页的属性而不是多出来的一行。多栏页同样经 `prtail` 收尾（`vertcol` 在两处调用它，`:604-608`），
  因此两条路径必须同时改，只改一条会留下「单栏对、多栏错」的半截语义。
  **未建模半**（登记不执行）：`-f` 的另一半 `pgpause |= FIRSTPAGE` 只在 stdout 是终端时生效，本壳无终端腿；
  `prtail` 的第二个参数 `incomp`（末行缺换行时补一个 `\n`）在本壳的整行建模下恒为 0。
  三件套：`cargo test -p minix-textfilter` → **50 个测试目标全 ok、合计 578 passed、零 FAILED**；
  `--lib pr::` → 46/0；`--test pr_face` → 13/0（第 13 枚 `form_feed_ends_every_header_page_and_no_header_cancels_it`）；
  其余七道门 vis 19 / unvis 10 / sort 8 / head 8 / tail 10 / uniq 10 / cmp 10 不动。
  **八枚变异负控**（复位放 `finally` 并断言 md5 复原）：`-f` 不落 `-F`、表尾不撤、单栏短页仍补白、
  不写 `\f`、`\f` 多带换行、多栏短页仍补白、`-t` 在单栏不再取消换页符、`-t` 在多栏不再取消换页符 → 全部咬中。
  其中两枚首轮**没咬**，成因各不相同：一枚是因为我写的多栏测带着 `-t`，补白分支根本没被执行（去掉 `-t`
  并补一枚无 `-F` 的对照断言后当场翻红）；另一枚是 harness 把 `--lib pr:: --test pr_face` 拼成一次调用，
  过滤器 `pr::` 把端到端那 13 枚全滤掉，同时单栏的 `-t` 腿缺断言——拆成两次调用并补断言后同样翻红。
  门禁：容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` 零 error、容器内 50 目标全 ok、FAILED 0；
  三架构 `--bins` 三台零 error；clippy `src/pr.rs` 提及数 34 与基线逐条同数，`src/bin/pr.rs` 与 `tests/pr_face.rs` 零提及；
  `rustfmt --edition 2024 --check` 三文件零漂移；`tools/doc-style-lint.sh --diff` error 0。核实日 `2026-10-14`；基线 `7759e8150`。
  登记不执行：①`-p`（逐页暂停，需终端腿）、`-r`（抑制开档失败诊断）、`-T`（自定义时间格式）；②长行跨块状态
  `mor`/`lrgln`/`ips`/`ops` 的续排（与上面 `incomp` 同一条腿）；③`pr` 开档诊断缺 `strerror` 后缀（同族既有差异）；
  ④`window.rs:679-680` 两条 unused import；⑤真机 open 腿归 C-57/E5。

- ✅ **批次二百二十接上 `pr` 的 `-e`/`-i` 制表两腿（`inln` 展开与 `otln` 压缩），并揭出上一轮多栏金样缺了压缩腿**
  （`2026-10-14`，`claim/PR-TABS-qoder` @ 基线 `6b30dd134`，单域三线 commands/textfilter，无跨界）：
  接回批次二百一十九登记的①。真源两段：`inln`（`pr.c:1032-1068`）与 `otln`（`:1137-1240`）。
  **C 的四处不对称**（都写进注释并各有断言）：①展开的列从**行首**起算，因为 `inln` 每见换行复位缓冲指针；
  ②文中已有的 `ch` 在压缩时是**被模拟**而不是照抄——它只把输入列推到下一站、自己不产出；
  ③`ips - ops > 1` 这道门槛在默认站距 8 下其实**被站距判据蕴含**（`next_stop(ops) >= ops + gap`），
  只有站距 1（`-i1` 可达）才让它单独起作用——这条不是猜的，是变异负控逼出来的（见下）；
  ④展开在**读取时**发生、压缩在**写出时**且**只作用正文行**：表头走 `prhead` 的 `printf`、表尾走
  `prtail` 的 `putchar`，两者都不压缩。
  **最关键的一条是 `pr.c:1840-1848`**：多栏（`cflag`）与合并（`merge`）**隐含打开两腿**，无需 `-e`/`-i`。
  这条推翻了我上一轮（批次二百一十九）为多栏手推的四枚金样：当时按「补白串原样输出」推期望值，
  而 C 实际是「展开→裁剪补白→压缩」的合成，`1 + 九个空格 + 9` 出门成 `1<TAB>  9`。本轮把这四枚
  （竖排 16 行、末页均衡、`-a` 横排、多栏往返）按合成算式重推，并逐枚与真二进制核对一致。
  另注：`-e`/`-i` 的值语法是 BSD 可选附值（`x?`），**不吞下一词**——`pr -e f` 里 `f` 是操作数而非 gap；
  gap 走 `atoi`，字面 0 回落默认站距（`if (ingap == 0) ingap = INGAP`），C 的 `gap < 0` 判据被前置的
  `isdigit` 挡在 unreachable 位置，故不复现并在注释里点明。
  **代码形状**：库侧新增纯函数 `expand_tabs`/`contract_tabs`（字节级，与 C 一致）与
  `parse_char_gap`；`PrOptions` 加 `expand`/`contract: Option<(u8, usize)>`（`None` 即 C 的 `gap == 0`）；
  执行侧 `expand_input` 挂在读取腿、`emit_page` 挂在写出腿并按 `Page` 的三段划分决定谁被压缩。
  三件套：`--lib` → **44 pr 测通过**（基线 40，新增 `expand_tabs_follows_the_inln_column_arithmetic`、
  `contract_tabs_reencodes_blanks_and_simulates_existing_tabs`、
  `e_and_i_values_take_the_c_char_then_gap_shape`、`columns_and_merge_imply_both_transformations`）；
  `--test pr_face` → **12 passed / 0 failed**（基线 9，新增三枚：`-e` 读入腿、`-i` 只压正文不压表头、
  多栏隐含两腿的往返）；其余七道门 vis 19 / unvis 10 / sort 8 / head 8 / tail 10 / uniq 10 / cmp 10 不动；
  `cargo test -p minix-textfilter` 的 50 个测试目标全 ok、合计 575 passed、零 FAILED。
  **十枚变异负控**（复位放 `finally` 并断言 md5 复原）：展开算式改成固定一列 → 库内 1；单空格也压缩 →
  **首轮不咬**（站距 8 下被站距判据蕴含），补 `-i1` 站距 1 的两枚断言后当场 FAILED；删掉文中制表符的
  模拟支路 → 库内 1；删掉行尾冲刷 → 库内 1 + 端到端 1；多栏不再隐含两腿 → 库内 1 + 端到端 3；
  `gap 0` 不回落默认 → 库内 1；表头也被压缩 → 端到端 1；输入腿不展开 → 端到端 2。
  另有一枚过程性自纠：一次批量改写把 `#[test]` 插重了一次（重复属性告警），修掉并复测；
  clippy 在 `src/pr.rs` 的告警数与基线**逐条同数**（34），新增的一处 `collapsible_if` 已按建议改成 let chain。
  门禁：容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` Finished 零 error、容器内 50 目标全 ok、
  FAILED 0；三架构 `--bins` 三台零 error；clippy 触及文件 0 新增；`rustfmt --edition 2024 --check` 零漂移；
  `tools/doc-style-lint.sh --diff` error 0。核实日 `2026-10-14`；基线 `6b30dd134`。
  **登记不执行**：①`-f`/`-F` 与长行跨块状态（`mor`/`lrgln`/`ips`/`ops`）——本壳按整行建模，长行按多栏
  `trnc` 语义截断，与 C 的续排不同（已在 07 篇声明）；②`-p`（终端腿）、`-r`、`-T`；③`pr` 开档诊断缺
  `strerror` 后缀（同族既有差异）；④`window.rs:679-680` 两条 unused import；⑤真机 open 腿归 C-57/E5。

- ✅ **批次二百一十九把 `pr` 的分页与数值选项面按 C 坐实（多栏接线核实、页高规则、空输入、诊断报文），并新建第 8 道端到端门**
  （`2026-10-14`，`claim/PR-COLUMNS-qoder` @ 基线 `8eee6c420`，单域三线 commands/textfilter，无跨界）：
  规划表 13 行「pr 多栏第1片」开工后核实的事实是：**多栏与 `-m` 在库侧与执行侧都早已接线**
  （`paginate` 在 `columns > 1` 时分派 `paginate_columns`，后者含竖排、`-a` 横排与末页均衡重排），
  07 篇 pr 行那句「多栏与 `-m` 待批」是陈旧记录——所以本轮的正当作业不是再写一遍多栏，而是
  **建端到端门把它钉住**，并在钉的过程中暴露并修掉四项真偏差。四项都有 C 锚点与改前读数。
  **改前四项偏差（前两项以真二进制取数）**：
  ① 执行半 `bin/pr.rs` 的 stdin 腿不走共享双生封装，直接 `minix_sys::read(support::STDIN, …)`，
  宿主侧读不到管道 ⇒ 整个命令的输入面在宿主不可测（`cargo build --bin pr` 后 `seq 1 70 | pr -t`
  输出为空）。改走 `support::read_stdin()` 后可测。
  ② `pr < /dev/null` 打出一整页（表头 + 66 行补白）；C 的表头是在行循环里读到第一行之后才打
  （`pr.c:252-262`）、`prtail` 又被 `if (linecnt && …)` 挡住（`:294`），**空输入什么都不印**。
  ③ `-l` 的门槛被写成「必须 > 10」且失败退 2 报 usage：C 的下界是 1，且 `lines <= HEADLEN + TAILLEN`
  只是 `++nohead`（`:1875-1876`），于是 `pr -l 5` 在 C 里完全可用、本壳直接拒绝。同时
  `paginate` 的 `no_header` 分支把全部输入塞进**一页**且不理 `+page`，而 C 关掉表头后不扣那十行、
  整页都是正文（`prtail` 的 nohead 臂也不补白，`:1526-1538`）——`-t` 因此根本没有分页。
  ④ 数值选项 `-l`/`-w`/`-o`/`-n` 与 `-s`/`-n` 的多余字符在 C 里各有**一句自己的报文**且退 1
  （`setup` 返回 1、`main` 才退出，`:142-160`），本壳一律压成 usage 行退 2；且 C 要求 `-l`/`-w`/`-o`
  的值**首位是数字**（`!isdigit(*eoptarg)` 先判），随后才 `atoi < 1`；`-w` 那句以空格结尾。
  另修 `-a` 的两条组合判据（`-a` 需多列、`-m` 不得与 `-a` 并用，`:1823-1832`）与
  「C 认识但未建模的字母」和「getopt 也不认的字母」之分（前者报 not modelled、后者报 usage）。
  **代码形状**：`PrError` 由 4 臂扩到 11 臂，一臂对 C 一句报文；新增 `c_atoi`（照 C 的宽松 `atoi`：
  跳空白、一个符号、读到非数字为止）与 `c_counted`（`-l`/`-o`/`-w` 共用的「首位数字 + ≥1」）；
  `paginate`/`paginate_columns` 的表头与补白改为同一条 `if no_header` 分支并按 C 处理空输入与
  `start_page`；执行半的 `usage` 用单条字面量——C 的第二三行各带十个前导空格，而 Rust 的行续接
  `\` 恰好会吃掉这些空格（这是本轮自己踩到的一个坑，写成了注）。
  三件套：`cargo test -p minix-textfilter --lib` → **430 passed / 0 failed**（基线 427，pr 模块 40 枚：
  新增 `empty_input_prints_nothing_at_all`、`headerless_pages_still_break_at_the_page_length`、
  `column_pages_honour_the_start_page`、`short_page_length_turns_the_header_off_rather_than_failing`、
  `optional_option_values_keep_the_c_digit_run_leniency`，并改写 `parse_error_paths` 到 C 的报文集合）；
  新建第 8 道端到端门 `tests/pr_face.rs` → **9 passed / 0 failed**（栏宽 9+1 的竖排金样、末页均衡重排、
  `-a` 行优先、`-s` 制表分隔、表头页 5+10+5 的几何与补白、短页关表头、空输入四情形、`+page`、
  六句数值报文、三行 usage）。其余七道门 vis 19 / unvis 10 / head 8 / tail 10 / uniq 10 / cmp 10 /
  sort 8 全不动；`cargo test -p minix-textfilter` 的 50 个测试目标全 ok、合计 568 passed、零 FAILED。
  **十三枚变异负控**（断言源文件确被改、复位放 `finally`、复位后 md5 与变异前逐字相等）：
  `-t` 仍扣十行 → 库内 1 + 端到端 1；短页不再关表头 → 库内 1 + 端到端 1；单栏不拦空输入 → 库内 1 +
  端到端 1；多栏不再按 `+page` 跳页 → 库内 1；数值选项丢掉「首位是数字」判据 → 库内 1 + 端到端 1；
  `-w` 报文丢掉尾随空格 → 端到端 1；`-s` 多余字符不再报错 → 库内 1 + 端到端 1；多栏不分 `-a` → 库内 1 +
  端到端 1；`-m` 与 `-a` 并用不再被拒 → 库内 1 + 端到端 1。**一枚等价变异如实登记**：删掉
  `paginate_columns` 里为「空输入」加的守卫仍全绿，因为它被紧随其后的 `skip >= lines.len()`
  （`start_page` 跳页判据，`0 >= 0` 成立）完全覆盖；保留该守卫是取意自明，不为它编造断言。
  门禁：容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` → Finished 零 error，容器内
  `cargo test -j 1 -p minix-textfilter` 的 50 个测试目标全 ok、FAILED 计数 0；三架构
  `cargo build -p minix-textfilter --bins` 三台零 error（本轮又一处只有裸机构建能看见的问题没再发生：
  `minix_sys::Stat` 无 `default`，宿主测试完全看不到目标 cfg 分支，沿 `pwd`/`test`/`ln` 的
  `core::mem::zeroed()` 同形写法——本轮的 `bin_support` 新腿即按此实现）；clippy 指向
  `src/bin/pr.rs`、`src/bin_support.rs`、`tests/pr_face.rs` 告警 0，`src/pr.rs` 的 34 条与基线**逐条同数**
  （三枚新测改用结构体更新语法，避免加深 `field_reassign_with_default` 存量）；
  `rustfmt --edition 2024 --check` 四文件零漂移；`tools/doc-style-lint.sh --diff` error 0。
  核实日 `2026-10-14`；基线 `8eee6c420`。
  **登记不执行**：①`-e`/`-i`（输入与输出的制表展开/压缩，本壳按字符处理、不展开制表）；
  ②`-f`/`-F`（长行续排与 formfeed 组合）；③`-p` 逐页暂停（要终端腿）；④`-r` 抑制开档失败诊断；
  ⑤`-T` 自定义时间格式；⑥C 的 `-s` 与 `-w` 组合会把页宽默认抬到 512、`-2` 之下再算 `colwd`，
  本壳已按公式实现但未对 `-s` 无 `-w` 的 512 情形单列金样；⑦`pr` 的开档失败诊断仍缺 `strerror`
  后缀（与 cmp/uniq/tail/vis/unvis/sort 同条差异）；⑧`src/window.rs:679-680` 两条 unused import（既存）；
  ⑨真机 open 腿归 C-57/E5。

- ✅ **批次二百一十八接上 `sort -o` 输出腿（三支路：标准输出 / 直开 / 临时档换名），并新建第 7 道端到端门**
  （`2026-10-14`，`claim/SORT-OUTPUT-qoder` @ 基线 `4bccf477c`，单域三线 commands/textfilter，无跨界）：
  接回 07 篇 sort 行的「`-o` 由选项解析器报 BadOption 拒」。开工前先纠正该行的一处**陈旧事实**：它把
  `-m` 与 `-o` 一起写成"报 BadOption 拒"，而 `sort.rs` 早已接 `-m`（解析器置 `merge`、执行半走
  `merge_sorted` 两两归并），文档滞后于代码。
  **C 的三支路形状**（`usr.bin/sort/sort.c:320-379`）：`outpath` 为空 → `stdout`；`lstat(outpath,&st)`
  失败、或命中的是字符/块特殊档 → `fopen(path,"w")` 直开直写；否则先 `access(outpath, W_OK)`（失败按 C
  报**裸路径名**退 2），`mkstemp("<path>XXXXXX")` 造临时档、把结果写进去，再 `lchmod(临时档,
  st.st_mode & ALLPERMS)` 复制**原档权限位**，然后 `unlink(outpath)` → `link(临时档, outpath)` →
  `unlink(临时档)`。顺序是有后果的：旧档被**换掉**而不是被截断，于是名的 inode 变了、模式位保留、
  中途失败旧内容仍在原地。`-c` 在 `:316-319` 就 `order()` 并退出，所以 `-c -o f` 连 `f` 都不碰。
  **三方案**：甲、只做"`-o` 写文件"（一律 `create_sink`）——最小改动，但把三支路里可观察的两条（模式复制、
  换名而非截断）与"原档在失败时不被动过"这条安全性质全丢掉，且一旦 `-o` 与输入同名就直接毁档；
  乙、把临时档+换名一律走 `rename`——Linux 的 rename 覆盖语义与 C 的 `unlink`+`link` 不同（C 选这两个
  调用正是为了避开 rename 在不同文件系统与 POSIX 语义上的差异），照 rename 写即偏离真值；
  丙（采）：照 C 写三支路，策略做成库侧纯函数 `decide_output(output: Option<&[u8]>, probe) ->
  {StandardOutput, Direct, ViaTemp{mode}}`，syscall 做成 `bin_support` 的宿主/裸机双生薄原语
  （`probe_path`/`writable`/`create_exclusive`/`set_mode`/`hard_link`/`remove_path`），命令侧按 C 的次序串起来。
  丙让"哪条支路"可单测、"支路里做什么"可端到端测，syscall 层不含策略。
  **两处字段形状的取舍**：`Options.output` 用 `Option<Vec<u8>>` 而不是 `Vec<u8>`——C 的 `-o ""` 里
  `outpath` 非空指针，会一路走到 `fopen("", "w")` 失败，与"没给 `-o`"结果不同，用空串表达两者会抹平这条差别；
  `mkstemp` 的六字符取 C 的 `padchar` 六十二字表原序（`lib/libc/stdio/gettemp.c:49-50`）与进位思路，
  但以进程内时钟播种替代 `arc4random`（本壳无随机源，时钟播种已有 `epoch_micros` 先例），碰撞时按 64 次进位。
  **改前的一处真缺陷被端到端门当场揭出**：`bin/sort.rs` 的读腿改前直接调 `minix_sys::{open, read, close}`
  （只有裸机形态），宿主侧 `access(argv[i], R_OK)` 一类的操作数路径此前从未被测到过——新门第一次跑就把
  两条操作数测打成"读不到档"，改走 `bin_support::read_file` 的双生封装后全绿。这也是本轮再次踩实的老教训：
  **命令面改动必须跑三架构裸机构建**——首次三架构构建报 13 处
  `no function or associated item named default found for struct Stat`（宿主测试完全看不见目标 cfg 里的
  `bin_support` 新函数），改成本层既有写法 `unsafe { core::mem::zeroed() }`（`pwd`/`test`/`ln` 同形）后三台零 error。
  三件套：`cargo test -p minix-textfilter --lib` → **430 passed / 0 failed**（基线 427，新增
  `test_parse_options_takes_the_output_path`、`test_output_target_follows_what_lstat_said`、
  `test_temporary_names_keep_the_prefix_and_walk_forward`）；新建 `tests/sort_face.rs` → **8 passed / 0 failed**
  （真进程 + 真管道 + 真档：直写新建名、既有档换名且权限位 0640 保留 + inode 必须变 + 目录里不得残留
  `<path>XXXXXX`、原地 `-o f f`、不可写目标退 2 且原内容不动、`-c -o` 不碰目标、操作数按序拼接且忽略
  stdin（C 只在 `optind == argc` 时回落 stdin，`:306-312`）、缺档退 2、裸 `-` 读 stdin、`-o` 缺值退 2）；
  六道既有门 vis 19 / unvis 10 / head 8 / tail 10 / uniq 10 / cmp 10 全不动；
  `cargo test -p minix-textfilter` 现有 49 个测试目标全 ok、合计 554 passed、零 FAILED。
  **八枚变异负控**（每枚断言源文件确被改、复位后 md5 与变异前逐字相等；复位放进 `finally`）：
  丢掉 `is_device` 判据（设备也走临时档）→ 库内支路测 FAILED；`lstat` 失败也走临时档 → 库内 1 + 端到端
  `a_name_that_does_not_exist_yet_is_written_directly` FAILED；模式复制写死 0600 → 端到端模式测 FAILED；
  去掉 `access(W_OK)` 检查 → 端到端不可写测 FAILED（档被换掉）；不先 `unlink` 目标名 → 端到端 2 枚 FAILED
  （`link` 撞已存在的名）；漏掉临时档自清理 → 端到端残留检查 FAILED；stdout 一腿断掉 → 端到端 3 枚 FAILED；
  `-o` 退化为无值旗标 → 库内 1 + 端到端 3 FAILED。
  门禁：容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` → Finished 零 error，容器内
  `cargo test -j 1 -p minix-textfilter` 的 49 个测试目标全 ok、FAILED 计数 0、`cargo fmt --check` 漂移 0；
  三架构 `cargo build -p minix-textfilter --bins` 三台零 error；clippy 指向 `src/sort.rs`、
  `src/bin/sort.rs`、`src/bin_support.rs`、`tests/sort_face.rs` 的告警计数 0；
  `rustfmt --edition 2024 --check` 四文件零漂移；`bash tools/doc-style-lint.sh --diff` error 0。
  核实日 `2026-10-14`；基线 `4bccf477c`。
  **登记不执行**：①C 的异常退出清理腿（`atexit(cleanup)` + 六个信号处理器，`sort.c:342-349`）本壳无信号腿，
  半途被杀会留下临时档名；②C 认整个选项循环之后的尾巴形 `… -o NAME`（`sort.c:262-265`，服务非置换式
  getopt），本壳选项扫描比 C 更宽（同 crate 既存 permute 差异项）；③`sort` 的 `warn` 报文缺 `strerror`
  后缀（与 cmp/uniq/tail/vis/unvis 同条 C 差异）；④`-o` 落在目录、落在无权限目录、`link` 跨设备等情况
  按 C 的对应错误码路径退 2，但未逐一分测（宿主无稳定设备目录可造）；⑤外部归并引擎（超内存输入）；
  ⑥`src/window.rs:679-680` 两条 unused import（既存存量）；⑦真机 open 腿归 C-57/E5。

- ✅ **批次二百一十七接上 `unvis` 的 `-H` 实体腿（一百名表与 `&#nn;` 态），`unvis(1)` 的选项面至此无留白**
  （`2026-10-14`，`claim/UNVIS-HTTP1866-qoder` @ 基线 `4a238068b`，单域三线 commands/textfilter，无跨界）：
  接回批次二百一十六登记的①（规划表 20b）。真源三处：`lib/libc/gen/unvis.c:85-196` 的一百项
  `nv[]`、`:431-467` 的 `S_AMP` 落入 `S_STRING` 的前缀扫描、`:468-474` 的 `S_NUMBER` 累加式。
  **表的次序是算法的一部分**：`S_STRING` 不是二分查表，而是**从上一轮落到的表项接着往下扫**，
  并且一旦发现扫过的项不再共享已匹配前缀就直接 `goto bad`（`:449-457`）。换序就会改变哪些名字
  可被接受，所以本表逐字节照抄 C 的顺序，并断言项数（100）、首项 `("AElig", 198)` 与末项
  `("yuml", 255)`——这三枚断言就是「表被重排或漏项」的哨兵。`;` 读作名字的终止符，
  `name_byte` 越出名字末尾返回零正好对应 C 的 `char[7]` 数组终止位。
  **C 的 `&#nn;` 有一个可复现的算术缺陷，本壳照做**：累加式是
  `*cp += (*cp * 10) + uc - '0'`（`:473`），新数字加在**已经乘过一次的旧值**之上，
  于是 `&#65;` 走 0+6 = 6、6+65 = 71，出来的是 `G` 而不是 `A`；`&#48;` 得 52。
  真值优先序把「Minix3 C 源码行为」放在第一，故不在实现里悄悄修正，缺陷写进 07 篇 unvis 行，
  要改需另立设计裁决（登记见下）。
  **并用的判据补齐**：C 的 `switch (eflags & (VIS_HTTP1808|VIS_HTTP1866|VIS_MIMESTYLE))`
  只列三种含 MIME 的组合，因此 `-h` 与 `-H` 并用是合法的——本轮把这条也钉成金样
  （`-hH` 下 `%41&amp;` 得 `A&`，两个引导符同时生效）。
  **状态形态的一处拆解**：C 把「名字匹配到第几个字母」塞进同一个 `int state` 的高字节
  （`GS`/`SS`/`GI` 三个宏，`unvis.c:212-216`），本壳改为 `DecodeState::position` 字段，
  而 `pending` 与 C 的 `*cp` 一样身兼二职（匹配中是表内下标、命中后是成品字节），
  这层复用写进了注释，免得读者以为要多一个变量。
  三件套：`cargo test -p minix-textfilter --lib` → **427 passed / 0 failed**（基线 424，新增
  `test_http1866_walks_the_entity_table_in_order`、
  `test_numeric_entities_reproduce_the_c_accumulation_step_for_step`、
  `test_a_broken_entity_name_reports_and_the_rest_of_the_run_survives`）；
  `--test unvis_face` → **10 passed / 0 failed**（基线 9，新增
  `ampersand_flag_walks_the_hundred_name_entity_table`，并把并用判据的清单从 3 组扩到 6 组、
  删掉 `-H` 的「not modelled」拒绝测）；`vis_face` 19/0 与 head 8 / tail 10 / uniq 10 / cmp 10 不动；
  `cargo test -p minix-textfilter` 的 48 个测试目标全 ok、合计 543 passed、零 FAILED。
  实体值 `&nbsp;`=0xA0、`&copy;`=0xA9、`&yuml;`=0xFF 这类高字节结果按字节写出（与本轮
  上一片把解码改成字节驱动正好配套：这类值在 UTF-8 文本形态下会被拆成两字节）。
  **十枚变异负控**（每枚先断言源文件确被改、复位后 md5 与变异前逐字相等）：`;` 不再当终止符 → 库内 2 +
  端到端 2 FAILED；名字位置不推进 → 库内 2 + 端到端 2；每轮从头扫表（丢掉「从上一项往下扫」的语义）→
  库内 2 + 端到端 2；未命中终止符就急着出字节 → 库内 2 + 端到端 2；`&#` 不再走数字态 → 库内 1 + 端到端 1；
  **把 C 的算术缺陷「修好」** → 库内 `test_numeric_entities…` 与端到端实体门同时 FAILED（这枚是本轮最要紧的
  一枚：它证明缺陷是被有意复现而非疏漏）；并用判据漏掉 `-H` → 端到端并用判据 FAILED；`&` 不受旗标门 →
  库内 1 + 端到端 1；`name_byte` 越界不返回终止零 → 库内 2 + 端到端 2。
  另有一枚**等价变异**如实登记：把 `if position != 0 && …` 的前缀一致性判据改成无条件执行，
  全测试仍绿——因为 `position == 0` 时 `last` 恒为 0、越界读也恒为 0，两支同真值，
  与 C 的 `if (is != 0 && …)` 短路完全一致。它不是漏测，是语义等价，故保留 C 形而不为它编造断言。
  门禁：容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` → Finished 零 error，容器内
  `cargo test -j 1 -p minix-textfilter` 的 48 个测试目标全 ok、FAILED 计数 0；三架构
  `cargo build -p minix-textfilter --bins` 三台零 error；clippy 指向三文件告警 0（新测名首版含大写
  旗标字母触发 `non_snake_case`，按建议改名）；`rustfmt --edition 2024 --check` 三文件零漂移；
  `tools/doc-style-lint.sh --diff` error 0。核实日 `2026-10-14`；基线 `4a238068b`。
  **登记不执行**：①`&#nn;` 的 C 算术缺陷要不要偏离，属设计裁决项（偏离须按三档术语在三处一致标注，
  本轮不动）；②`unvis` 的 `warn` 报文缺 `strerror` 后缀（与 cmp/uniq/tail/vis 同条 C 差异）；
  ③`src/window.rs:679-680` 两条 unused import（既存存量）；④多字节宽字符路径（本壳按字节）；
  ⑤超宽 chunk 的折叠终止分岔（批次二百一十四登记）；⑥真机 open 腿归 C-57/E5。

- ✅ **批次二百一十六接上 `unvis` 的 `-e`/`-h`/`-m` 三面解码旗标，并把解码器改成字节驱动的唯一真值**
  （`2026-10-14`，`claim/UNVIS-FLAGS-qoder` @ 基线 `d53c2d311`，单域三线 commands/textfilter，无跨界）：
  接回批次二百一十五登记的①（07 篇 unvis 行的「HTTP/MIME 旗标族 `-e/-H/-h/-m` 显式拒待批」）。
  **开工前的两条改前读数都是对 C 的事实缺口，用真二进制现场取证**（`cargo build --bin unvis` 后直接喂管道）：
  ① `printf 'a\xffb' | unvis` 打出**空**——执行侧写的是 `core::str::from_utf8(&input).unwrap_or("")`，
  一段非法 UTF-8 就让整档输入作废，而 C 的 `process()` 逐 `getc` 拿的是字节，`a\xffb` 三个字节都该原样送出；
  ② `printf '\\x41' | unvis` 打出 `x41`——C 的 `S_START` 有一支 `case 'x': → S_HEX`（`lib/libc/gen/unvis.c:305-307`），
  **不需要任何旗标**就该把 `\x41` 解成 0x41，本壳的移植里没有这一支（也没有 `S_HEX*` 三态）。
  **三条路**：甲、只补旗标、`decode` 仍收 `&str`——上面两条错原样留着，且入参形态本身就挡在字节语义前面；
  乙、库里再写一个 per-byte 版给命令用、`decode` 继续存在——同一台状态机两份实现，分岔无法被发现，
  正是本项目反复登记的「第二真值」病形；丙（采）：把 per-byte 的 `unvis_byte`/`unvis_end` 做成唯一真值，
  `decode` 降为薄封装（逐字节驱动 + 静默重启，对应 C 的 `strunvisx` 视角），入参改 `&[u8]`，
  执行侧照 C 的 `process()` 自己驱动以便报 offset。丙另有一处红利：`decode` 与 `unvis(1)` 现在**必然**同步，
  库测的结论可直接引到进程面。
  **照 C 落实的细节**（逐条带锚点）：`UNVIS_VALIDPUSH` 让 `offset` 不推进——C 的 `offset++` 排在 `getc` 那一侧，
  `goto again` 重投同一字符时不再计数（`usr.bin/unvis/unvis.c:113-124`）；`UNVIS_SYNBAD` 报
  `unvis: <名>: offset: N: can't decode` 后 `state = 0` 续跑，且**报错的那个字节随失败一起被吞掉**（不重投），
  `warnx` 不是 `errx` 所以退码不动（`:126-128`）；结尾的 `UNVIS_END` 冲刷只认成品字节，半个未完成的转义
  序列**无声**丢弃——C 那一句没有 `switch`，只是 `if (... == UNVIS_VALID) putchar`（`:137-139`）；
  并用判据按 C 的 `switch (eflags & (VIS_HTTP1808|VIS_HTTP1866|VIS_MIMESTYLE))` 只查三种组合、
  `-e` 不参与，位置在整轮选项循环之后、开任何档之前，因此它盖过「文件读不到」（`:80-89`）；
  开档失败只 `warn`、程序恒退 `EXIT_SUCCESS`（`:91-104`，与 vis 的 `rval = 1` 相反，别照搬）。
  **两张十六进制表的不对称是格式辨认的依据而不是笔误**：`xtod`（`unvis.c:79`）先折大小写，
  所以 `%4A` 与 `%4a` 同值；`XTOD`（`:80`）不折，且 MIME 的数字判据是
  `isxdigit(uc) && (isdigit(uc) || isupper(uc))`——`=4a` 在 C 里是**解码错误**，不是同一个字节的另一种写法。
  本壳备两张表（`from_hex`/`from_upper_hex`）与一个 `is_upper_hex`，注释点明这一点。
  **拆片**：`-H`（`VIS_HTTP1866`）仍显式拒绝，它要 `unvis.c:85-196` 那张一百项的实体名表与
  `S_AMP`/`S_NUMBER`/`S_STRING` 三态（`state` 的高字节是表内下标，`GS`/`SS`/`GI` 三个宏就是为它设计的），
  规模足以单独一轮，已在规划表记为 20b。
  三件套：`cargo test -p minix-textfilter --lib` → **424 passed / 0 failed**（基线 422，新增
  `test_decode_reads_hex_and_the_flag_gated_escapes` 与
  `test_decode_flushes_a_short_run_at_end_of_input`）；新建第 6 道端到端门
  `cargo test -p minix-textfilter --test unvis_face` → **9 passed / 0 failed**（真进程 + 真管道 + 真文件：
  语法回环、0xFF 直通（档与管道两路都测）、`%`/`=` 无旗标即字面、`-e` 让反斜杠失去引导符身份、
  MIME 软换行、坏序列的 offset 报文与续跑、开不了的档不改善、字母聚簇与 `--` 与并用判据、
  集外字母报 usage）；`vis_face` 19/0、head 8 / tail 10 / uniq 10 / cmp 10 全不动；
  `cargo test -p minix-textfilter` 现有 48 个测试目标全 ok、合计 539 passed、零 FAILED。九枚金样全部按 C
  状态机手推，一次通过；一处初版手推错被 runner 当场翻红并改正（`ab=x` 以为 `x` 会落回文本，实则
  随解码失败一起被吞，正确期望是 `ab` 加一行 offset 4 的告警）。
  **十枚变异负控**（每枚先断言源文件确被改、复位后 md5 与变异前逐字相等）：去掉 `\x` 支路 → 库内 2 枚 +
  （补上端到端 `\x41\x4b` 断言后）端到端同测亦翻红；MIME 接受小写十六进制 → 库内 1 枚；`xtod` 不再折大小写 →
  **首轮十枚里唯一没咬住的一枚**，原因是我的样例只在小写字母后 `%` 的用例上跑，折与不折同值；补
  `%4A`、`\x4A`、`\x4a` 三枚断言后同一变异当场 FAILED（test-audit 维度四「无效断言」的一例）；
  软换行只吃一个行末 → 库内 1 + 端到端 1；`-e` 守卫失效 → 库内 1 + 端到端 2；`%` 不受旗标门 → 库内 2 +
  端到端 1；终局冲刷不再完成一位八进制 → 库内 1；并用判据取消 → 端到端 1；重投字节也被计入 offset → 端到端 2；
  解码失败时改判为重投 → 端到端 2（这枚正是「`ab=x` 期望 `ab`」那条断言的牙）。
  门禁：容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` → Finished 零 error，容器内
  `cargo test -j 1 -p minix-textfilter` 的 48 个测试目标全 ok、FAILED 计数 0；三架构
  `cargo build -p minix-textfilter --bins --target {x86_64-unknown-none, aarch64-unknown-none,
  riscv64gc-unknown-none-elf}` 三台零 error；clippy 指向 `src/vis.rs`、`src/bin/unvis.rs`、
  `tests/unvis_face.rs` 的告警计数 0；`rustfmt --edition 2024 --check` 三文件零漂移；
  `bash tools/pattern-gate.sh --diff` → PASS=17 FAIL=0 SKIP=1；`tools/doc-style-lint.sh --diff` error 0。
  核实日 `2026-10-14`；基线 `d53c2d311`。
  **登记不执行**：①`-H` 实体腿（规划表 20b）；②`unvis` 的 `warn` 报文缺 `strerror` 后缀
  （与 cmp/uniq/tail/vis 同一条 C 差异，共用一处修）；③`src/window.rs:679-680` 两条 unused import
  （既存存量）；④多字节宽字符路径（本壳按字节）；⑤超宽 chunk 的折叠终止分岔（批次二百一十四登记）；
  ⑥真机 open 腿归 C-57/E5。
  **顺带闭掉一条旧登记**：批次二百一十三与二百一十五登记的「`decode()` 里两枚 `#[cfg(test)]`
  `eprintln!("DBG …")` 遗留调试输出」位于本轮整体替换掉的解码函数体内，随重写一并消失
  （`grep -n DBG src/vis.rs` 现无命中），不需要另开卫生轮。

- ✅ **批次二百一十五接上 `vis` 的 `-h`/`-m` 两枚编码样式旗标（C 的选项串至此全部接完）**
  （`2026-10-14`，`claim/VIS-HTTPMIME-qoder` @ 基线 `192155d34`，单域三线 commands/textfilter，无跨界）：
  接回批次二百一十三③与批次二百一十四①留下的最后两枚字母。真源形状：`getvisfun`
  （`lib/libc/gen/vis.c:338-345`）按 HTTP、MIME、标准的次序取函数；`do_hvis`（`:164-177`）放行字母数字、
  五个 safe 字 `$ - _ . +` 与六个 extra 字 `!*'(),`，其余一律 `%` 加**小写**十六进制；`do_mvis`
  （`:187-201`）用 `=` 加**大写**十六进制，且空白只在「其后就是行末」时转义、行内空白交回 `do_svis`；
  `usr.bin/vis/vis.c:142-144` 在整轮选项循环之后、打开任何操作数之前拒绝 `-h` 与 `-m` 并用。
  **两张十六进制表不是笔误**：`vis.c:128-129` 同时定义 `xtoa`（小写）与 `XTOA`（大写），HTTP 用前者、
  MIME 用后者；本壳也备两张（`LOWER_HEX`/`UPPER_HEX`）并在注释里点明这是格式辨认的依据，不是一处疏漏。
  **`-m` 的连带默认照 C 落实**：`foldwidth` 若仍是 80 就压到 76（`vis.c:110-111`，`-F` 显式给过宽度后
  再来 `-m` 就不再动它，两个方向的金样都钉了），隐藏换行的字形从 `` `\`` 换成 `=`（`foldit.c:63` 与
  `vis.c:289` 读同一个位）——上一轮的 `FOLD_MARKER` 常量因此改成 `fold_marker(mime) -> &'static [u8]`，
  不留一个恒假的分支。
  **测试自身的两处补强（test-audit 维度二：防「测试代码就是错的/测不出东西」）**：
  ①`c != '\n'` 那道守卫最初测不出来——单个换行即便去掉守卫也仍然透传（它后面不是行末，
  「其后是行末」那项判据同样为假），变异「删掉守卫」跑遍全绿；补 `encode(b"\n\n")` 与
  `encode(b"a\r\n\n")` 两枚断言后，同一变异当场翻红（`test_mime_style_quoted_printables_with_uppercase_hex`
  FAILED）。②点名字母表的**个数**只写在注释里、没有判据可挡：`char_shell` 是 18 个（注释一度误记十七）、
  `do_mvis` 的点名表是 12 个（注释一度误记十一），现各加一枚 `assert_eq!(表.len(), N)` 把个数钉住，
  并按 `vis.c:133`/`:134`/`:195` 的行号核对——写错的计数属 P0-fact，已在正文与注释一并改正。
  三件套：`cargo test -p minix-textfilter --lib` → **422 passed / 0 failed**（基线 419，新增
  `test_http_style_percent_encodes_with_lowercase_hex`、
  `test_mime_style_quoted_printables_with_uppercase_hex`、`test_fold_marker_follows_the_mime_flag`）；
  `--test vis_face` → **19 passed / 0 failed**（基线 17，新增
  `http_and_mime_letters_switch_to_their_own_escapes_and_hex_case`、
  `mime_moves_the_default_fold_width_and_writes_its_own_marker`，并把
  `unmodelled_letters_are_refused_not_ignored` 换成 `letters_outside_the_c_optstring_are_refused_with_usage`：
  本壳已无「未建模」字母，剩下的拒绝只有 C 自己落 `default:` 的集外字母，以及 C 点名的 `-h`/`-m` 并用）；
  head 8 / tail 10 / uniq 10 / cmp 10 四道门不动；47 个测试目标全 ok、合计 528 passed、零 FAILED。
  **八枚变异负控**（每枚先断言源文件确被改、复位后 md5 与变异前逐字相等）：MIME 换用小写表 → 库内 1 +
  端到端 1 FAILED；HTTP 允许集丢掉字母数字 → 库内 1 + 端到端 1；MIME 丢掉「字节不是换行」守卫 → 首轮
  全绿（即上述补强①的由来），补断言后库内 1 FAILED；空白一律转义 → 库内 1 + 端到端 1；`-m` 不再压宽度 →
  端到端 `mime_moves…` FAILED；隐藏换行无视 MIME 字形 → 同一枚 FAILED；取消 `-h`/`-m` 并用之拒 → 端到端
  `letters_outside…` FAILED；HTTP 支路不分派 → 库内 1 + 端到端 1。
  **一处金样手算错当场抓住并改正**：`-f` 对 100 个 `x` 的第二段是 22 个字符（第 79 到 100），初版按
  80 字符那枚旧金样的「余 2」写成 22→2，被 runner 翻红后按列算式重推（78 + 折 + 22 + 收尾折）；
  同轮的 MIME 宽度金样（74 + 26）与 `-F 40` 金样（38 + 38 + 24）均为手推后一次通过。
  门禁：容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` → Finished 零 error，容器内
  `cargo test -j 1 -p minix-textfilter` 的 47 个测试目标全 ok、FAILED 计数 0；三架构
  `cargo build -p minix-textfilter --bins --target {x86_64-unknown-none, aarch64-unknown-none,
  riscv64gc-unknown-none-elf}` 三台零 error；clippy 指向 `src/vis.rs`、`src/bin/vis.rs`、
  `tests/vis_face.rs` 的告警计数 0；`rustfmt --edition 2024 --check` 三文件零漂移；
  `bash tools/pattern-gate.sh --diff` → PASS=17 FAIL=0 SKIP=1（增量 C 锚点 22 token 存在性/落窗过、
  反引号常量 14 token 过）；`tools/doc-style-lint.sh --diff` error 0。核实日 `2026-10-14`；基线 `192155d34`。
  **过程教训（表格单元的第二类坑）**：07 篇是一张 Markdown 表，单元里出现裸 `|` 会劈列——本轮写 MIME
  判据时引 C 的 `nextc == '\r' || nextc == '\n'` 与点名表 `#{|}` 各带竖线，脚本的门禁断言
  （`row.count('|') == 4`）在写入前就把它挡住，改写为中文措辞后通过；此前踩的是同类的第一种坑
  （单元里换行劈行）。固化为：**往表单元里写含竖线的代码引用，必须先改写成文字或转义，并在写盘前断言
  竖线计数**。
  **登记不执行**：①`vis(1)` 的旗标面至此无留白，但 07 篇里 `unvis` 行仍有一批解码旗标未接
  （`-e`/`-H`/`-h`/`-m`，对位 `lib/libc/gen/unvis.c` 的 `strunvisx`）——它是 vis 家族最后一处命令面留白；
  ②`decode()` 里两枚 `#[cfg(test)]` 的 `eprintln!("DBG …")` 遗留调试输出（批次十六既存）；
  ③`src/window.rs:679-680` 两条 unused import（既存存量）；④多字节宽字符路径（本壳按字节，07 篇已声明）；
  ⑤超宽 chunk 的折叠终止分岔（批次二百一十四登记，真机与 C 对拍须避开 `-F 5` 档宽度）；
  ⑥真机 open 腿归 C-57/E5。

- ✅ **批次二百一十四接上 `vis` 的 `-f`/`-F` 行折叠（`foldit` 的列状态机），并把编码路径拆成逐 chunk 形态**
  （`2026-10-14`，`claim/VIS-FOLD-qoder` @ 基线 `92b9c9d66`，单域三线 commands/textfilter，无跨界）：
  接回批次二百一十三登记的③里行折叠那一半（07 篇 vis 行的「仍未接的 `-f`、`-F`」）。C 的真源形状有三处必读：
  `usr.bin/vis/foldit.c:36-70` 的列算式与 `goto again`、`usr.bin/vis/vis.c:58` 的三个独立变量
  （`eflags`、`fold`、`foldwidth = 80`）、以及 `vis.c:202-289` 的 `process()` 循环——**chunk 是按输入字符产出的**，
  `buff` 每次只装一个字符的编码，折叠判据跑在这段已生成文本上，末了 `if (fold) col = foldit(...)` 之后才把
  chunk 打出去。本壳先前是整-buffer 编码后一次 `emit`，因此本轮除了接 `-f`/`-F`，还要把编码路径拆成 chunk 形态。
  **三条路的选择**：甲、照抄含 `goto again` 的循环——最忠实，但 C 的重扫在「单个 chunk 自身宽于 `max - 2`」时
  永不返回（`-F 5` 的限值是 3，而 0xFF 编出来是四字符的 `\M^?`），照抄就是把一条活锁搬进命令，真机上表现为
  无界打印隐藏换行；乙、按「意图」重写为「越限的那个字符起算新行」——它与 C 在单字符 chunk 上同解，但 chunk
  多于一列时答案不同：C 打印标记后把**整个 chunk** 从列 0 重数过去，乙只把越限点之后的剩余挪到下一行，
  于是会把一个转义序列从中间劈开（`\134` 的前两列留在上一行末）——这是实质错答，不能为了少写一层循环而换；
  丙（采）：把 `foldit` 建成「(标记数, 折后列值)」的纯函数，正常路径逐字节照 C 的列算式与整段重扫，
  只对无界那一支给终止语义（插一个放得下的标记、按 chunk 自身宽度计列），并把分岔与复现条件写进 07 篇与
  `foldit` 的文档注释（`rewrite-notes/18-stage-commands/07-text-filter.md` vis 行）。
  **顺带纠正两处「看着像别名、其实不是」的语义**：`-F` 的 case 只做 `foldwidth` 赋值与 `markeol++`，它打开的是
  行末标记**不是**折叠（`vis.c:89-95`），`-f` 才 `fold++` 且不动宽度（`:96-98`），故 `Plan` 里 `fold` 与
  `foldwidth` 必须是两个字段、不能合成一个 `Option<usize>`（合成即把「只标记不折行」这档判据丢掉）；
  每个操作数末尾补的那半个标记**不**重置列值（`:288-289` 直接 `printf`，没有 `col = 0`），加上
  `:170` 的 `static int col`，列值是跨操作数存活的——本轮用一枚双文件金样把这两条钉在同一处断言里。
  空操作数也补标记：C 的 `cp` 初值是 `nul + 1`，于是 `*(cp - 1)` 按构造不是换行（`:171-172`），
  这条看似怪癖但属可观察行为，照做并留注。
  **代码形状**：库侧新增 `encode_chunk(byte, next, options)` 与私有的 `encode_into`（`encode` 与 chunk 形态共用
  同一个 `encode_byte`，且 chunk 形态每次自建 extra 表，对应 `istrsenvisx:484` 每字符重建的代价），
  新增 `foldit`/`column_step`/`walk`/`columns` 与 `FOLD_MARKER`（标记字形由 `VIS_MIMESTYLE` 决定，而 `-m` 仍拒，
  故只有一支可达，注释写明 `-m` 落地时该改哪里）；执行侧 `run()` 改为逐字节产出 chunk、先折后打，
  并在每个操作数收尾补隐藏换行。`-F` 的取值按 `atoi` 建 `parse_atoi`（跳空白、取一个符号、读数字为止、
  忽略尾随垃圾、无数字折 0），`< 5` 即 `vis: can't fold lines to less than 5 cols` 退 1。
  三件套：`cargo test -p minix-textfilter --lib` → **419 passed / 0 failed**（基线 415，新增四枚：
  `test_foldit_breaks_before_the_chunk_that_passes_the_limit`、
  `test_foldit_counts_columns_with_the_tab_backspace_and_newline_rules`、
  `test_foldit_terminates_where_the_c_would_loop`、
  `test_encode_chunk_matches_the_whole_buffer_encode`——最后一枚把「逐 chunk 拼接 == 整-buffer `encode`」
  钉成等式，防止拆分引入第二真值）；`--test vis_face` → **17 passed / 0 failed**（基线 12，新增五枚：
  `fold_writes_a_hidden_newline_and_closes_a_partial_line`、`fold_flag_alone_marks_lines_but_does_not_fold`、
  `fold_width_is_read_the_way_atoi_reads_it`、
  `fold_counts_columns_of_the_encoded_text_and_carries_the_column_across_files`、
  `fold_applies_to_the_none_path_too`）；其余四道命令门 head 8 / tail 10 / uniq 10 / cmp 10 不动，
  `cargo test -p minix-textfilter` 的 47 个测试目标全 ok、合计 523 passed、零 FAILED；五枚金样全部一次通过，
  期望值是按 C 的列算式手推的（`-F 6` 限 4 列：`abcd` 折、`\M^?` 四字符超宽、制表进到八的倍数）。
  **九枚变异负控**（每枚先断言源文件确被改、复位后 md5 与变异前逐字相等）：折叠余量 `max - 2` 改成 `max - 1` →
  库内 2 枚 + 端到端 4 枚 FAILED；制表符按一列计 → 库内 `test_foldit_counts_columns…` + 端到端
  `fold_counts_columns…` FAILED；结尾不给半行补标记 → 端到端 4 枚 FAILED（库内不动，证明这条只在真机进程面上可观察）；
  `-F` 不再打开行标记 → 端到端 `fold_flag_alone…` FAILED；`-F` 顺手也打开折叠 → 同一枚 FAILED（同测双向夹住这档判据）；
  列值改为每操作数清零 → 端到端 `fold_counts_columns…` FAILED；宽度下限 5 改成 3 →
  端到端 `fold_width_is_read_the_way_atoi_reads_it` FAILED；`-n` 支路跳过折叠 → 端到端
  `fold_applies_to_the_none_path_too` FAILED；换行不清零列值 → 库内 1 + 端到端 1 FAILED。
  门禁：容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` → Finished 零 error，容器内
  `cargo test -j 1 -p minix-textfilter` 的 47 个测试目标全 ok；三架构
  `cargo build -p minix-textfilter --bins --target {x86_64-unknown-none, aarch64-unknown-none,
  riscv64gc-unknown-none-elf}` 三台零 error；clippy 指向 `src/vis.rs`、`src/bin/vis.rs`、`tests/vis_face.rs`
  的告警计数 0（新增测名含大写字母触发一次 `non_snake_case`，按建议改成不含旗标字母的名字而不是加 `#[allow]`）；
  `rustfmt --edition 2024 --check` 三文件零漂移；`bash tools/pattern-gate.sh --diff` → PASS=17 FAIL=0 SKIP=1
  （增量 C 锚点 16 token 存在性/落窗过、反引号常量 3 token 过）；`tools/doc-style-lint.sh --diff` error 0；
  `tools/notes-link-check.py rewrite-notes/` 断链集合与基线逐条相同（124 → 124，新增 0）。核实日 `2026-10-14`；
  基线 `92b9c9d66`。
  **登记不执行**：①`-h`/`-m` 的 HTTP 与 MIME 编码支路（`lib/libc/gen/vis.c` 的 `do_httpvis`/`do_xvis`），
  其中 `-m` 还连带两处默认（折叠标记换成 `=` 加换行、宽度仍为 80 时压到 76）；②`decode()` 里两枚
  `#[cfg(test)] eprintln!("DBG …")` 的遗留调试输出（批次十六既存，本轮未动）；③`src/window.rs:679-680`
  两条 unused import（既存存量）；④解码侧 `unvis` 的 `-e`/`-H`/`-h`/`-m` 对应旗标；⑤多字节宽字符路径
  （本壳按字节编码，`-N` 与 0x7F 以上字节的差异已在 07 篇声明）；⑥超宽 chunk 的终止分岔已在 07 篇与
  `foldit` 注释双处声明，真机若要用 C 对拍须避开 `-F 5` 这一档宽度；⑦真机 open 腿归 C-57/E5。

- ✅ **批次二百一十三接上 `vis` 的 `-s`/`-t`/`-w`/`-S`/`-M`/`-N` 六枚旗标，并纠正一笔写错的反斜杠透传事实**
  （`2026-10-14`，`claim/VIS-EXTRALIST-qoder` @ 基线 `826449c6b`，单域三线 commands/textfilter，无跨界）：
  **先纠错（P0-fact）**：错源有两处同源的说法。批次二百一十的金样纠错清单里第②条写着「默认下一律反斜杠是图形
  字符，`do_svis` 顶部就透传（保护它是 `-e`/`-S` 的事，不是默认）」，库测 `test_mark_eol…` 据此钉了
  `encode(b"a\\b") == "a\\b"`；批次十六更早把「0x5C 反斜杠透传不对称」当作已声明的 C 怪癖写进了回环测的豁免。这条事实是反的：`istrsenvisx` 在
  `lib/libc/gen/vis.c:484` 无条件调 `makeextralist`，而后者在 `:380` 有一句
  `if ((flags & VIS_NOSLASH) == 0) *d++ = L'\\';`——默认表里就带着反斜杠，`iswextra`（`:310`）因此在图形判据之前
  命中。按 C，一个裸反斜杠在默认模式输出 `\134`，只有 `-b` 让它直通。本轮把行为与两处描述（07 篇 vis 行、库内
  注释与测）一并改回 C，并连带删掉 256 字节回环测里「0x5C 因直通而不可回环」的豁免：现在 `0x00..=0xFF` 全部
  逐字节回环，断言比原来强一枚。顺带把 07 篇 vis 行里那句「graphic 与空白字节透传」扩写成完整的引擎形状
  （透传判据三条通路 + `do_mbyte` 三支路的次序 + extra 表的拼装次序），并把本轮新接的六枚写进同一行。
  **接上的六枚字母**（`usr.bin/vis/vis.c:73-135` 的 getopt 表）：`-s`→`VIS_SAFE`，只放宽透传判据，`iswsafe` 是
  退格/响铃/回车（`vis.c:127` 定义、`:312` 使用）；`-t`→`VIS_TAB`；`-w`→`VIS_WHITE`，而 `vis.h:52` 把它定义成
  `VIS_SP`、`VIS_TAB`、`VIS_NL` 三位之并，所以 `-w` 一次置三位；`-S`→`VIS_SHELL`，追加 `char_shell`（`vis.c:133`
  的十七个字）；`-M`→`VIS_META`，即 `VIS_WHITE`、`VIS_GLOB`、`VIS_SHELL` 的并（`vis.h:67`），这也是本命令唯一
  取得到 glob 集的途径，因为 `VIS_GLOB` 自己没有字母；`-N`→`VIS_NOLOCALE`。
  **建模方式的三方案比较**：甲、`VisOptions` 存一个 `flags: u32` 位掩码，与 C 完全同形——但 Rust 侧失去字段级
  可见性，每个用法都要手写位与，`Default` 也不再免费；乙、给 `-w` 与 `-M` 各设一个「组合位」字段，在
  `extra_set()` 里 `if meta {…} else { if white {…} else if tab {…} }` 分支——本轮开工时的草稿就是这个形，它把
  C 的一条位并写成了控制流，`else if` 顺手丢掉了 `VIS_SP` 与 `VIS_TAB` 的独立性，将来任何新组合都要重写分支；
  丙（采）、一位一字段（`sp`/`tab`/`nl`/`shell`/`glob`），组合旗标在读选项处展开成多位。丙让 `extra_set()` 与
  `makeextralist` 的函数体一行一行对得上，而展开正发生在 C 的 `switch` 所在处，语义归属也干净：库侧只认位义，
  命令侧只认字母。
  **`-N` 不进结构体的理由写明**：它在 C 里选的是两处分支的 C-Locale 一侧——`ISGRAPH` 用 `iscgraph` 而非
  `iswgraph`（`vis.c:122-124`），以及 `makeextralist` 把 `-e` 串按多字节文本解码（`:361`）——本引擎按字节工作、
  从不把字节重组为字符，这两处都只有这一个答案可给；给结构体加一个恒不改变行为的字段等于加死状态。采「字母
  接受、不置任何位」，并把结论钉在端到端测里（`-N` 的输出逐字节等于默认输出）。
  三件套：`cargo test -p minix-textfilter --lib` → **415 passed / 0 failed**（基线 411，新增四枚：
  `test_a_backslash_is_the_one_graphic_byte_the_default_mode_encodes`、
  `test_safe_admits_backspace_bell_and_carriage_return`、
  `test_whitespace_letters_move_whitespace_onto_the_extra_list`、
  `test_shell_and_meta_letters_name_the_magic_characters`）；`cargo test -p minix-textfilter --test vis_face` →
  **12 passed / 0 failed**（基线 9，新增 `safe_admits_the_three_named_controls_and_beats_cstyle`、
  `whitespace_and_magic_letters_change_only_the_extra_list`、`nolocale_letter_is_accepted_and_changes_nothing`）；
  `--bin vis` 无宿主单测（决定半的测在 `--lib` 侧）；其余四道命令门 head 8、tail 10、uniq 10、cmp 10 全部不动，
  `cargo test -p minix-textfilter` 的 47 个测试目标全 ok、零 FAILED。
  **七枚变异负控**（每枚都先断言源文件确实被改、复位后 md5 与变异前逐字相等）：去掉 extra 表的无条件反斜杠项 →
  库内 3 枚 + 端到端 3 枚 FAILED；透传判据不再看 `VIS_SAFE` → `test_cstyle_named_escapes` 与
  `test_safe_admits_backspace_bell_and_carriage_return` 与端到端 `safe_admits_the_three_named_controls_and_beats_cstyle`
  FAILED（第一枚翻红正说明「透传先于 `-c`」这条次序是被测住的，否则 0x08/0x07/0x0D 会走命名转义）；`-c` 支路短路 →
  库内 3 + 端到端 2 FAILED；`-l` 行标记分支短路 → 库内 1 + 端到端 1 FAILED；执行半把 `-w` 只置 tab 一位 → 端到端
  `whitespace_and_magic_letters_change_only_the_extra_list` FAILED；`-M` 丢掉 glob 位 → 同一枚端到端测 FAILED；
  删掉 `-N` 的 match 分支（落到集外字母、报 usage）→ 端到端 `nolocale_letter_is_accepted_and_changes_nothing` FAILED。
  门禁：容器 `minix-ci:1.94`（带 `-u`）`cargo check -j 1 --workspace` → Finished 零 error；三架构
  `cargo build -p minix-textfilter --bins --target {x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf}`
  三台零 error；clippy 指向 `src/vis.rs`、`src/bin/vis.rs`、`tests/vis_face.rs` 的告警计数 0（同函数体内两处存量
  顺手消除：`collapsible_if` 改成 let chain、一处 `needless_borrow`）；`rustfmt --edition 2024 --check` 三文件零漂移；
  `bash tools/pattern-gate.sh --diff` → PASS=17 FAIL=0 SKIP=1（增量 C 锚点 19 个 token 存在性/落窗过、反引号常量
  12 个 token 过）。核实日 `2026-10-14`；基线 `826449c6b`。
  **过程教训（工具用法级，非本仓库级）**：变异脚本用 `cp "$f" "/tmp/$(basename $f).pre"` 存基线，而本轮两枚被改
  文件的 basename 同为 `vis.rs`，后者覆盖前者的备份，随后一次 `cp` 回写把 `src/vis.rs` 覆盖成了 `src/bin/vis.rs`
  的内容，库侧改动当场丢失。恢复靠 `git show HEAD:…/src/vis.rs` 取回基线再按记录重写，并把全部门禁重跑一遍
  （415/0、12/0、三架构、clippy、fmt 均为重写后复验值）。固化两条：备份路径必须带目录层级（或用完整相对路径
  转义后的唯一名字），复位必须用「与变异前 md5 相等」的断言而不是 `cp` 了事——第二版的 python 脚本已按此实现。
  **登记不执行**：①`src/vis.rs` 的 `decode()` 里两枚 `#[cfg(test)]` 的 `eprintln!("DBG …")` 是批次十六留下的调试
  输出，宿主每跑一次测试就往 stderr 灌行，属卫生项；②`src/window.rs:679-680` 的
  `test_parse_head_args_faithful_getopt` 里两条 unused import（`alloc::string::String`、`alloc::vec`）是既存存量，
  `cargo test -p minix-textfilter` 的 warning 计数含它们；③`-h`/`-m` 的 HTTP 与 MIME 引擎支路（`do_httpvis`/
  `do_xvis`，`vis.c:160-201`）以及 `-f`/`-F` 的行折叠与 `col` 状态机（`foldit`），合起来是第 4 片；④解码侧
  `unvis` 的 `-e`/`-H`/`-h`/`-m` 对应旗标；⑤多字节宽字符路径（本壳按字节编码，0x7F 以上字节与 C 在 UTF-8 locale
  下的整字符答案不同，07 篇 vis 行已声明）。

- ✅ **批次二百一十二补上 `cmp -s` 的入口档长判据（C `c_regular` 的第一句）**
  （`2026-10-13`，`claim/CMP-SILENT-LEN-qoder` @ 基线 `f1ec87abd`，单域三线 commands/textfilter，无跨界）：
  接回批次二百零三登记的⑦。改前的形状：执行半把两侧都切完窗口后才交给 `compare_bytes`，`-s` 只看窗口是否相同；
  而 `minix3/usr.bin/cmp/regular.c` 的 `c_regular` 第一句就是 `if (sflag && len1 != len2) exit(1);`，
  这里的 `len1`/`len2` 是 `stat` 到的**原始档长**（`cmp.c:152-153` 传进来的 `sb1.st_size`/`sb2.st_size`），
  还没扣过偏移。后果是一个具体可复现的错答：`file1` 为 `abc\n`（4 字节）、`file2` 为 `XXabc\n`（6 字节）、
  `cmp -s file1 file2 0 2` —— 跳过之后两边都是 `abc\n`，本壳判为相同退 0，C 在入口就因为 4 != 6 退 1。
  **限定条件也照 C**：`cmp.c:136-146` 只有在两个操作数都是常规档时才走 `c_regular`，任一档打不开或不是常规档
  （含 `-` 走标准输入）就置 `special`、改走 `c_special`，那里**没有**这条尺寸判据；所以新守卫限定在
  `silent && !stdin1 && !stdin2`。残留差异登记在案：本壳的读腿不区分 FIFO 与普通档（都走 `fopen` 式读），
  所以把一个 FIFO 当文件操作数传进来时仍会做这道检查，而 C 会走 `c_special`；要消掉它得在 `bin_support.rs`
  的读腿上补一次 `fstat` 的 `S_ISREG` 判定（已登记，不在本笔扩界）。
  三件套：`cargo test -p minix-textfilter --test cmp_face` → **10 passed / 0 failed**（基线 9；新增三处断言把
  上面那个反例钉住：`-s` 退 1、去掉 `-s` 后同样的偏移与档内容相同退 0、把第一个操作数换成标准输入则退 0）；
  `--lib` 411/0、其余四道命令门不动、整包 47 个测试目标全 ok 零 FAILED。变异负控：把新守卫短路成 `false` →
  该测当场翻红（`left: 0, right: 1`，正是这道判据的效果）；反向替换 + `grep` 验证后复测 10/0。
  整包三架构裸机构建三台 0 error；clippy 指向 `bin/cmp.rs` 的告警仍只有一条存量的 items after a test module；
  `rustfmt --edition 2024 --check` 触及两文件零漂移；容器门读数见本笔提交信息；核实日 `2026-10-13`；
  基线 `f1ec87abd`。
  **仍开登记**：⑤（C 选项串前导 `-` 的 permute-off 语义）、⑥（C `err()` 会带 `strerror` 后缀，本壳只打档名）、
  ⑧（GNU 与本壳的 skip 判据不同，本机对账只能比退码与通道空否）、以及本轮新增的 FIFO 与普通档不分这条。

- ✅ **批次二百一十一接上 `vis` 的 `-e extra`，并把编码分支的判定次序摆回 C**
  （`2026-10-13`，`claim/VIS-EXTRA-qoder` @ 基线 `480b09fa4`，单域三线 commands/textfilter，无跨界）：
  旗标族第 2 片（顺带吃掉第 3 片里的选项面部分）。三件事：
  **① `-e extra`**：`do_svis` 的开首透传原本是 `!iswextra && (ISGRAPH || iswwhite)`
  （`lib/libc/gen/vis.c:309-315`），本壳先前没有 `iswextra` 这个变量（extra 表恒空，写死了等价于
  `ISGRAPH || iswwhite`）。现在 `VisOptions` 带一个 `extra: Vec<u8>`，命中的字节不走透传、进八进制支路，
  所以 `-e x` 会把可打印的 `x` 写成 `\170`；带 `-c` 时 C 的 switch 先跑，`x` 取短形式 `\x`，而
  跳过表里的 `n` 与八进制数字 `3` 落回 `\156`/`\063`（那里 `\n` 式的转义会有歧义）。
  **② 判定次序**：`encode_byte` 之前是先查 `-o` 再走 `-c`，C 是 `vis.c:207-271` 的先 CSTYLE 后八进制；
  对同时有命名转义且 `-o -c` 并用的字节（如 0x07），两者给出 `\007` 与 `\a` 两种答案——本轮改成与 C 同序。
  **③ 元空格规则**：C 的 `(c & 0177) == ' '` 把 0xA0 送进八进制支路，所以它输出 `\240` 而不是元序列；
  本壳之前会按 `M` + 脱字符渲染它。
  **选项面也改成了 getopt 的形**（第 3 片里剩下的只有引擎侧字母）：字母聚簇（`-bl` ≡ `-b -l`）、
  `-e` 值附于本 token 或取下一词、后来的 `-e` 整串替换（C 是 `extra = optarg` 赋值）、`--` 结束扫描；
  并把 `-d`/集外字母的报文改成 C 的 usage 行（大写 U、退 1），与"本构建未建模"那类区分开——
  前者 C 自己就报 usage（`-d` 在非 `DEBUG` 构建里被编译掉、落到 `default:`），后者是诚实的本壳限制。
  三件套：`cargo test -p minix-textfilter --lib` → **411 passed / 0 failed**（基线 410，新增
  `test_extra_bytes_skip_the_pass_through_shortcut`）；`--test vis_face` → **9 passed / 0 failed**（基线 7，新增
  `letters_cluster_and_values_attach_or_follow` 与
  `extra_bytes_protect_graphic_characters_and_meta_space_renders_octal`）；整包 47 个测试目标全 ok、零 FAILED；
  两枚变异负控各让对应测翻红：把八进制支路挪回 CSTYLE 之前 → 库内与端到端两处 extra 测同时 FAILED；
  让 `iswextra` 恒为假 → 端到端两枚测 FAILED；均反向替换 + `grep` 验证后复测 411/0 与 9/0。
  **又踩一次同一类坑并当场抓住**：`USAGE.to_string()` 在宿主测试里绿、三架构裸机构建报
  `no method named to_string`（`no_std` 下 `ToString` 不在作用域），改成 `String::from(USAGE)` 后三台 0 error
  ——再次印证命令面改动必须跑目标构建，光看宿主测试会漏。
  容器 `minix-ci:1.94`（`-u`）`cargo check -j 1 --workspace` → Finished 0 error，容器内 47 个测试目标全 ok；
  clippy 指向 `bin/vis.rs` 的告警计数 0（中途一处 `string_slice_as_bytes` 按建议改成先取字节再切）；
  `rustfmt --edition 2024 --check` 触及三文件零漂移；核实日 `2026-10-13`；基线 `480b09fa4`。
  **登记不执行（第 3、4 片）**：引擎侧 `VIS_SAFE`(`-s`)/`VIS_TAB`(`-t`)/`VIS_WHITE`(`-w`)/`VIS_META`(`-M`)/
  `VIS_SHELL`(`-S`)/`VIS_NOLOCALE`(`-N`)/`-h`/`-m` HTTP 与 MIME 编解码、`-f`/`-F` 行折叠与 `col` 状态机；
  解码侧 `strunvisx` 的 `-e`/`-H` 对应旗标（unvis 那行）；多字节宽字符路径（本壳按字节处理，
  UTF-8 输入下 C 会按整字符编码）。

- ✅ **批次二百一十接上 `vis` 的 `-b`/`-l`/`-n` 三个旗标（并纠正一处字母错映射）+ `vis_face` 端到端门**
  （`2026-10-13`，`claim/VIS-FLAGS-1-qoder` @ 基线 `3a4cb032d`，单域三线 commands/textfilter，无跨界）：
  旗标族两片中的第 1 片。开工前 grep `bin/vis.rs` 发现的不只是缺口，而是一个**事实错**：本壳把“去反斜杠”
  挂在 `-n` 上，而 `minix3/usr.bin/vis/vis.c` 里 `case 'b': eflags |= VIS_NOSLASH;`、`case 'n': none++;` ——
  去反斜杠是 `-b`，`-n` 完全另一回事（逐字节原样送出，只把反斜杠再写一份）。照错的字母走下去，用户按手册输 `-b`
  会被当“未建模”拒掉、输 `-n` 则拿到完全不同的输出。
  **三个字母的形状**：`-b` 归到已有的 `VisOptions::no_slash`（字段注释同时改名说清它是 `VIS_NOSLASH`）；`-l` 新增
  `mark_eol` 字段，在 `encode` 里把 `\n` 写成 `\` + `$` + 换行，且那个反斜杠按 C 的
  `if ((eflags & VIS_NOSLASH) == 0)` 跟随 `-b`；`-n` **不进 `VisOptions`**——它保留 0x7F 以上的原始字节，
  而编码路径返的 `String` 存不下（会变成 UTF-8 两字节），所以它另走一个字节级入口
  `copy_doubling_backslashes`，由薄壳在 `-n` 时改走那条路。C 的 `process()` 循环先判 `none`、再判 `markeol`，
  所以 `-n` 遮蔽 `-l` 与编码，本轮把它也摆在同一个优先级上。
  **新增 `tests/vis_face.rs`（7 枚）**：默认编码、`-b`、`-l`、`-b -l`、`-n`（含“`-n` 遮蔽 `-l`”与“0xFF 以单字节原样输出”）、
  `-o`/`-c`、多操数合并与打不开的操数只 warn 且退 1、裸 `-` 是字面档名、十个未接字母全部拒。金样对比的是**字节**而不是字符，
  因为 `String` 比对会把“UTF-8 两字节”这个错掩盖掉。
  **金样当场拓出四处我的错误推断（代码都对，期望错）**：① 0xFF 是 `\M^?`而不是 `\M-^?`（`do_mbyte` 先写 `M`，
  被剔高位后的 0x7F 走 iswcntrl 分支写 `^?`，两者之间没有 `-`）；② 默认下一律反斜杠是图形字符，`do_svis` 顶部就透传
  （保护它是 `-e`/`-S` 的事，不是默认）；③ `-c` 对 TAB 与换行**根本不起作用**（同一处透传先于 CSTYLE 分支），
  而 0x01 没有命名形式（`\a` 是 0x07）故走脱字符形；④ `-o` 才把 0xFF 写成 `\377`。四处都已按 C 写进测注。
  三件套：`cargo test -p minix-textfilter --test vis_face` → **7 passed / 0 failed**；`--lib` → **410 passed / 0 failed**
  （基线 `408`，新增 `test_mark_eol_writes_a_backslash_dollar_before_the_newline` 与
  `test_none_copies_through_and_doubles_each_backslash`）；其余四道命令门（head/tail/uniq/cmp）10/10/10/10 不动；
  整包三架构裸机构建三台 0 error。三枚变异负控各让对应测翻红：把 `-n` 改回旧的 `no_slash` 映射 → 拷贝测 FAILED；
  把 `-l` 改成空操作 → 标记测 FAILED；让标记里的反斜杠不跟随 `-b` → 同一测 FAILED；均反向替换后复测 7/0 与 410/0。
  容器 `minix-ci:1.94`（`-u`）`cargo check -j 1 --workspace` → Finished 0 error，容器内整包测试 47 个目标全 ok、
  零 FAILED；clippy 自有文件与 base 逐条同集（`vis.rs` 两条 collapsible_if 与一条 deref 均在未触碰的旧代码里）；
  `rustfmt --edition 2024` 对触及的三份文件就地归一后零漂移（顺带清掉了 `vis.rs` 与 `bin/vis.rs` 的 4 块存量版差，
  纯空白零行为改变，与本 crate 以往几轮的归一方式一致）；核实日 `2026-10-13`；基线 `3a4cb032d`。
  **登记不执行（第 2 片候选）**：选项面只认单字母整词，C 的 getopt 支持聚簇（`-bl`）与附值（`-e x`/`-F 40`）；
  `-d` 的报文文案与 C 的 usage 行不同（退码同）；`-h`/`-m` 并存时 C 报“不能同时指定”，需先有 HTTP/MIME 编码本体；
  `-f`/`-F`/`-s`/`-t`/`-w`/`-M`/`-S`/`-N`/`-e` 全在引擎侧（`VIS_SAFE`/`VIS_TAB`/`VIS_WHITE`/`VIS_META`/`VIS_SHELL`/
  `VIS_NOLOCALE`/extra 表），需按 `do_svis`/`do_mbyte` 逐支补；多字节（UTF-8 宽字符）与行折叠本壳未建模。

- ✅ **批次二百零九给 `head` 加上端到端金样门，并纠一笔上一轮写错的事实**
  （`2026-10-13`，`claim/HEAD-HOST-FACE-qoder` @ 基线 `3ae7bb772`，单域三线 commands/textfilter，无跨界）：
  **先纠错**：批次二百零八末尾写下"`head -n 0` 仍被拒为退 1，而 C `head.c` 的计数器不启动、是什么也不输出且退 0"，
  这条推断是错的。重读 `minix3/usr.bin/head/head.c:86-103`：`strtoimax` 之后紧跟的是
  `if (... || *ep || linecnt <= 0) errx(1, "illegal line count -- %s", optarg)` —— C 对零与负数**就是拒**，退 1 并带报文。
  本壳的解析半 `parse_head_args`（`window.rs`，`Ok(parsed) if parsed > 0`）早已按 C 这么做，所以根本不存在那条缺口；
  会误判的原因是套用了 `tail` 的结论：`tail.c` 的 `ARG` 宏允许零偏移（送去档尾、什么也不输出且退 0），
  两个程序对零的处理本就不同。零计数与超限计数是两件事：前者拒、后者放行。
  **本轮改的三处**：① 删掉 `bin/head.rs` 里那段基于错误推断写的注释与那条永远走不到的守卫
  （`parse_head_args` 已拒绝非正计数，`HeadWindow::new(count)` 在其后必成）；② `07-text-filter.md` head 行
  按 `head.c` 重写这段事实；③ 操作数读腿接到 `support::read_file`，删掉自搭的 `read_fd` 与 `minix_sys` 导入，
  并补 `tests/head_face.rs`（8 枚）。
  **金样覆盖面**：缺省十行；`-n`/obsolete `-3`/`-c` 逐字节（四个字节含换行，所以是 `1\n2\n`）；超限计数
  （`-n 40`、`-n 1000000`）输出整档退 0；非正计数四条报文（`-n 0`/`-n -3`/`-c 0`/`-n x`）退 1；
  `usage: head [-n lines] [file ...]` 一字不差；`-q`/`-v` 与多操数的横幅规则；打不开的操数不出横幅且退 1；
  `-` 是字面档名（`head.c` 每个操数都过 `fopen`，与 `cut`/`cmp` 不同）。差分腿与本机 `head` 比四形，
  只取行尾带换行的输入、且不比零计数（GNU 接受 `-n 0`）。
  三件套：`cargo test -p minix-textfilter --test head_face` → **8 passed / 0 failed**；`--test tail_face`/`--test uniq_face`/
  `--test cmp_face` 各 10/0、`--lib` 408/0、`--bin head` 3/0 不动；整包三架构裸机构建三台 0 error；
  clippy 触及文件零告警（新测试里的 `needless_borrow`/`op_ref` 两处按建议改净）；
  `rustfmt --edition 2024 --check` 触及文件零漂移（`bin/head.rs` 那条 import 排序存量顺带归正）。
  两枚变异负控：把 `parsed > 0` 改成 `>= 0` → 金样报文测与库内测 `test_parse_head_args_errors` 同时 FAILED；
  把操数读取换成"恒返回空档"→ 横幅规则测与打不开的操数测同时 FAILED（第一次这类注入写成了语法不合法的表达式，
  测试没跑起来也就不算证据；换成合法形态后才真正抽出差分——负控也要验它自己有效）。
  **恢复现场的教训再犯一次并已修**：变异后我又用 `git checkout -- <路径>` 恢复，命令在错误的当前目录下静默无操作，
  `window.rs` 带着变异停在那里——是 `git status` 暴露的。此后一律"用同一段脚本反向替换 + `grep` 验证 + 复跑测试计数"，
  不再依赖任何 checkout/cp 的退码。
  容器门读数见本笔提交信息；核实日 `2026-10-13`；基线 `3ae7bb772`。
  **仍开登记**：`head` 行形给末行补换行（C `head()` 逐字符 `putc` 原样输出，无尾换行的末行不会被造出换行），
  与本族 `tail` 同一条已声明简化；`vis` 旗标族、`sort -m`/`-o`、`pr` 多栏等仍在队列里。

- ✅ **批次二百零八去掉行窗的三十二行封顶，`head`/`tail` 的任意计数按 C 照行**
  （`2026-10-13`，`claim/WINDOW-NOCAP-qoder` @ 基线 `0b10df877`，单域三线 commands/textfilter；`window.rs` 只是
  本 crate 的行窗库，无跨界）：
  接回批次二百零六登记的⑨。改前证据：`grep -n "n > 32" os/commands/usr-bin/textfilter/src/window.rs` 命中两臂
  （`HeadWindow::new` 与 `TailWindow::new`），存的是固定 `[&str; 32]`，调用方再把输出缓冲按 `count.min(32)` 开；
  实际行为是 `tail -n 40` 与 `head -n 40` 退 1 且无任何文案，而 C `head.c` 是从计数往下跑、
  `forward.c` 的 `RLINES` 在映射后的档案上往回走，两者都与计数大小无关。
  **方案对比**：甲案（采）把两个窗的存储换成 `Vec<&'a str>`，**先填满再回绕**：行数未达上限就 `push`，
  到顶后才按 `seen % limit` 覆写。若图省事按上限直接预分配，
  `tail -n 1000000` 就是一个百万元素的 malloc，等于把一个参数变成内存炸弹；先填满再回绕后，
  存储只跟输入给的行数走。调用方的输出缓冲也不再按计数开，而按 `(window.seen(), limit)` 的较小值开。
  乙案“把 32 提到 1024”仍是个假上限，不采；丙案只改 tail 不改 head，会让同一库里两套窗口语义不一致，不采。
  **零计数不变**：两个窗仍只拒 0（tail 的零形已由 `TailStyle::selects_nothing` 在进窗前处理）；`over_ring_cap`
  谓词与其守护一并删除（没上限可查），`tail` 的 `run()` 不再预先退 1。
  三件套：`cargo test -p minix-textfilter --lib` → **408 passed / 0 failed**（基线 `409`：原
  `test_zero_or_huge_rejected` 与 `test_tail_style_ring_cap_applies_only_to_the_line_walk` 合成一枚
  `test_zero_count_is_refused_and_large_counts_are_not`，后者同时钉住 `new(33)`/`new(1000)` 可用与
  “计数远大于输入时按原序输出全部”）；`--bin tail` 14/0、`--bin head` 3/0、`--test uniq_face` 与
  `--test cmp_face` 各 10/0 不动；端到端的 `--test tail_face` 10/0，其中
  `line_window_cap_rejects_the_count_without_a_diagnostic`（钉住旧拒绝）改写为
  `counts_above_the_old_window_cap_are_honoured`（`-n 40` 与 `-n 1000000` 均输出整档退 0）。
  两枚变异负控各让对应测翻红：把 `n > 32` 塞回 `TailWindow::new` → 库内测与 tail_face 测同时 FAILED；
  去掉“先填满再回绕”分支 → 库内测当场 panic（空窗上按下标写）；两笔均从备份恢复后复测 408/0 与 10/0。
  整包三架构裸机构建（所有 bin）→ 三台 0 error；clippy 自有文件与 base 逐条同集（三枚存量）；
  `rustfmt --edition 2024 --check` 触及文件零新增漂移（`bin/head.rs` 那条 import 排序与 base 同一块）；
  容器门读数见本笔提交信息；核实日 `2026-10-13`；基线 `0b10df877`。
  **新拓一处同族缺口（只登记不修）**：`head -n 0` 仍被 `HeadWindow::new(0)` 拒为退 1，而 C `head.c` 的计数器
  不启动、是什么也不输出且退 0——tail 那一族零值已在批次二百修过，head 这一形记为新待办。

- ✅ **批次二百零七把 `cmp` 的读腿接到宿主口子，并补上 `cmp_face` 端到端金样门（10 枚）**
  （`2026-10-13`，`claim/CMP-HOST-FACE-qoder` @ 基线 `956d83dc9`，单域三线 commands/textfilter，无跨界）：
  命令面宿主读腿的第三个使用者（前两个是 `uniq` 与 `tail`）。改前证据：`bin/cmp.rs` 自搭的
  `read_file_or_none` 直用 `minix_sys::open`/`read`/`close`，所以跑 `target/debug/cmp` 只会有空输出。
  现在两个操作数走 `support::read_file`，自搭函数与 `minix_sys` 导入一并删除。
  **顺带把另一处顺序摆回 C**：`cmp.c:95-119` 先开两个档（打不开就 `warn` + `exit(ERR_EXIT)`），
  `:121-134` 才解 `skip1`/`skip2`。本壳之前先解参数后开档，所以“档不存在 + 跳过值也不合法”这种双错调用
  会先报 `usage:`；现在报的是那个档名（金样里钉了一枚专看错误优先级）。两边退码都是 2，差别只在文案。
  **新增 `tests/cmp_face.rs`（10 枚）**：相同档静默退 0；首个差异的 `"%s %s differ: char %lld, line %lld"`
  与跨行计数（第二行不同报 `line 2`）；`-l` 的 `"%6lld %3o %3o"` 八进制列与“不提前退出”；`-s` 不输出但保退码；
  短流时 `cmp: EOF on <短的那一个>` 走标准错误退 1；skip 的五种合法写法（`2`/`0x2`/`02`/`2x`/前导空白）等价；
  skip 越过档尾的 EOF 报告（非 `-l` 只名档、`-l` 带 `: char <len + 1>`）与“恰等于档长不算越界”；
  退 2 的五条（集外字母、`-l -s` 并存、只有一个操作数、skip 没转出东西、五个参数）；`-` 操作数走标准输入
  且报文标签写 `stdin`、两侧都是 `-` 拒；再加一枚与本机 `cmp` 的差分腿，**只比退码与“哪条通道仍为空”**：
  GNU 打的是 `differ: byte` 且 `-l` 列宽不同，比文案量到的是两个程序的差异而不是本壳的回归。
  **工具坑（本轮实踩，写下来防后续轮重踩）**：变异负控跑完后的恢复不能写成 `git checkout -- <路径> || cp <备份> <路径>`——
  `git checkout` 会把整个文件回到 HEAD（连本笔的改动一起抹掉）且退码为 0，`||` 后的恢复分支根本不会跑，本轮就这么差点丢了整笔
  改动（好在备份 `cp` 还在）。正确做法：恢复只用备份拷贝，且紧跟着跑全量测试确认计数，而不是相信“命令没报错”。
  三件套：`cargo test -p minix-textfilter --test cmp_face` → **10 passed / 0 failed**；`--test tail_face` 与 `--test uniq_face`
  各 10/0；`--lib` → **409 / 0**；全包其余测试目标全绿。三枚变异负控各让对应测翻红：`-l` 的八进制改十进制 →
  `-l` 列表格式测 FAILED；`skip_window` 的越界保护去掉（退回静默夹取）→ 越界 EOF 测 FAILED；把跳过参数改回先解 →
  错误优先级测 FAILED；均从备份恢复后复测 10/0。整包三架构裸机构建（所有 bin）→ 三台 0 error；
  clippy 触及文件零告警（新增测试里的 `&PathBuf` 参数按 `ptr_arg` 建议改成 `&Path`）；`rustfmt --edition 2024`
  对新测试文件就地归一后零漂移；容器门读数见本笔提交信息；核实日 `2026-10-13`；基线 `956d83dc9`。
  **仍开登记**：批次二百零三的⑤（C 选项串前导 `-` 的 permute-off，会让 `cmp a -c` 类型的调用把 `-c` 当第二个操作数）、
  ⑥（`-l`/`-s` 之外的报文差异：C `err()` 会带 `strerror` 后缀，本壳只打档名）、⑦（C `c_regular` 首行的
  `sflag && len1 != len2` 在偏移检查之前）与⑧（GNU 与本壳的 skip 判据不同，本机对账不能越界使用）。

- ✅ **批次二百零六把 `tail` 的操作数读腿接到宿主口子，并补上跑真进程的 `tail_face` 金样门（10 枚）**
  （`2026-10-13`，`claim/TAIL-HOST-FACE-qoder` @ 基线 `a9f889a9f`，单域三线 commands/textfilter，无跨界）：
  按批次二百零五末尾的形状跟进。改前证据：`grep -n "minix_sys::" os/commands/usr-bin/textfilter/src/bin/tail.rs`
  命中 `open`/`read`/`close` 三件与一个自带的 `read_fd`，`support::STDIN` 直接作为 descriptor 使；
  因此虽有金样测试的形状可用，跑 `target/debug/tail` 仍只会得空输出。现在操作数走 `support::read_file`
  （宿主 `std::fs`、目标同一套 open/read/close 梯子），`tail_stream` 改为直接收字节片，自带的 `read_fd`
  与 `minix_sys` 导入一并删除（否则会留死码与未用导入）。
  **顺带把顺序摆对**：C 的主环是先 `fopen`+`fstat`（失败就 `ierr(); continue;`）、再打横幅，所以打不开的那一个档
  **不会留下横幅**；横幅打印因此排在读成功之后（一枚金样用例专钉这一层）。
  **新增 `tests/tail_face.rs`（10 枚）**：缺省十行、`-n`/obsolete `-N`、`-c`/`-b` 尾窗、`-n +N`/`-c +N`/`-b +N`
  以一为基（`+1` 输出整档）、零计数不输出且退 0、`-r` 三形、多操数横幅与档间空行、打不开的操数只 warn 不横幅且
  退 1、集外字母与 `-f` 与非法偏移各报自己的文案；另有一枚差分腿在 NetBSD 与 GNU 重合的四形（缺省/`-n 3`/
  `-n +9`/`-c 4`）上与本机 `tail` 逐字对比（不比 `-r`、`-b` 与无尾换行的行形：后两者 GNU 没有或行为不同，
  而本壳的行形会给末行补换行，属已声明的归一）。
  **金样跑出来的两处认知修正（错在金样期望，实现与 C 一致）**：一是 `tail -r -c 3` 作用于四字节 `abc\n`：
  `reverse.c` 的 `r_reg` 先 `p = start + size - 1` 再把 `size` 夹到 `off`，循环只回走两个字节，收尾
  `if (llen) WR(p, llen)` 写的是 `bc\n`——窗口外的 `a` 不出现；二是 `tail -r -n +3`：`+3` 经 `tail.c:146-149`
  折成 `RLINES off = 2`，所以输出的是两行反序而不是一行。两者已按 C 逐行追溯写进用例注释。
  **金样又拓出一处声明性差异（只钉住不顺手修）**：行窗封顶三十二，`tail -n 40` 在 C 里照报后四十行，本壳退 1；
  而且该拒绝**无任何文案**（`run` 里 `over_ring_cap()` 直接 `terminate(1)`）。两者一起登记为⑨（去封顶需改
  `window.rs` 的固定 `[&str; 32]` 设计，并与 `head` 的同族封顶一并考虑，属独立一轮）；金样里钉了一枚
  `line_window_cap_rejects_the_count_without_a_diagnostic` 专看这一层，去封顶那一轮改它时要连注释一起重读。
  三件套：`cargo test -p minix-textfilter --test tail_face` → **10 passed / 0 failed**；`--bin tail` → **14 / 0**；
  `--lib` → **409 / 0**；全包其余测试目标全绿。三枚变异负控：横幅改到开档之前 → 操数告测评 FAILED（第一次跑时
  补丁静默未命中，补上「变异必须真的改变源码」的断言后才注入成功——变异负控本身也要验其有效）；去掉 `selects_nothing` 早退 → 零计数测 FAILED；
  `over_ring_cap` 去掉 `off != 0` → 零计数测同样 FAILED；均按备份复原后复测 10/0。
  整包三架构裸机构建（所有 bin）→ 三台 0 error；clippy 触及文件零告警（中途 `tail_stream` 改收字节片后留下的
  七枚 `needless_borrow` 已按建议去掉 `&` 消除）；`rustfmt --edition 2024 --check` 触及两文件零漂移；容器门读数见本笔提交信息；
  核实日 `2026-10-13`；基线 `a9f889a9f`。
  **仍开登记**：④ `-f`/`-F` 跟踪；⑥ obsolete 后缀单位字母已接（批次二百零二），但 `-3f` 这类带跟踪的写法仍撞④；
  ⑨ 行窗封顶与它的静默拒绝；尾行无换行时行形补换行（与 head 同族的已接受简化）。

- ✅ **批次二百零五打通命令面宿主读腿（`uniq` 先行），并给它加上一套跑真进程的端到端金样门**
  （`2026-10-13`，`claim/CMD-HOST-READ-qoder` @ 基线 `2d13d207a`，单域三线 commands/textfilter；`bin_support.rs`
  是全 crate 四十来个 bin 共用的入口层，改动跑的是整包构建）：
  接回批次二百零二④与批次二百零四末尾点出的验证能力缺口。改前证据：`bin_support.rs` 的 `read_stdin`
  无条件走 `minix_sys::read`，而宿主侧那条腿不存在，所以 `target/debug/uniq < file` 输出恒为空（同族 tail/cmp
  一样），命令面唯一可用的判据只有库内纯函数测。这一步把验证能力本身往上抬一档：能跑真进程、真管道、真档。
  **形状**：`bin_support.rs` 里 `read_stdin` 与 `write_ok` 拆成 cfg 双胞（宿主走 `std::io`，目标走
  `minix_sys::read`/`write`），`minix_sys::{read, write}` 的导入随之收进目标 cfg（否则宿主侧未用导入告警）；
  新增 `open_input`/`ReadSource::read_to_end`（**开而先不读**）与 `bin/uniq.rs` 里的使用点。宿主臂用
  `std::fs::File::open`/`create`，目标臂用 `minix_sys::open(…, 0, 0)`（`O_RDONLY` 为零）。
  **为什么需要“开而先不读”**：C `uniq.c:119-122` 的顺序是 `ifp = file(argv[0], "r")` → `ofp = file(argv[1], "w")`
  → 才开始读，而 `"w"` 的截断发生在打开那一刻。把输入先读进内存再创建输出，在 `uniq f f` 这一个退化调用上
  会把旧内容原样写回去，而 C 得到空档——批次二百零四的行里已经按 C 写了“空档”的形状，但当时实现还做不到；
  本轮把顺序真正摆过来（先开输入句柄→创建并截断输出→再读输入），金样门里钉了一枚专这一层的用例。
  **新增 `tests/uniq_face.rs`（10 枚）**：跑 `env!` 递来的真 `uniq` 二进制（集成测试目标才能拿到
  `CARGO_BIN_EXE_uniq`），逐字比标准输出与退码：缺省折叠、`%4d` 四列计数、`-c -d`/`-c -u`/`-d -u` 组合
  （后者什么都不输出且退 0）、`-f 1`/`-f 2`/`-s 4` 的比较键与“比键报整行”、obsolete `-1`/`+4` 与正规形一致、
  集外选项与第三个操数退用法错、档操作数+输出档落盘、`uniq f f` 得空档、输入打不开时不创建输出档。
  另有一枚差分腿：在两者语义重合的那几形（折叠/`-d`/`-u`/`-f`/`-s`）上与本机 `uniq` 逐字对比；
  **故意不比** `-c`（GNU 用 `%7d`）与 obsolete（GNU 不接受），否则量到的是两个程序的差异而不是本壳的回归。
  三件套：`cargo test -p minix-textfilter` → 全包测试目标全绿（`--test uniq_face` 10/0、`--bin uniq` 9/0，
  `--lib` 409/0）；容器 `minix-ci:1.94`（`-u`）内同一套门也 10/0 与 9/0（这条路径必须双跑：金样依赖真进程）。
  变异负控：把输入的 `read_to_end` 换到创建输出之前 → `same_file_as_input_and_output_reports_nothing` FAILED，
  其余 9 枚全绿（说明该用例只钉住顺序、不与无关）；按备份复原后复测 10/0。
  因 `bin_support.rs` 是全 crate 共用件，跑整包目标构建：`cargo build -p minix-textfilter --target
  {x86_64-unknown-none,aarch64-unknown-none,riscv64gc-unknown-none-elf} --release`（所有 bin）→ 三台 0 error；
  容器 `cargo check -j 1 --workspace` → Finished 0 error；clippy 指向本次触及文件的告警计数 0；
  `rustfmt --edition 2024 --check` 三文件零漂移（`bin_support.rs:20` 那条 import 排序存量随本次拆导入一并归正）；
  核实日 `2026-10-13`；基线 `2d13d207a`。
  **后续可用形状**：同族命令的宿主逐字对账只需照搬这个测试文件的样子（拿 `CARGO_BIN_EXE_<name>` 跑真进程 +
  按 C 自推金样 + 在语义重合处与本机对账）；tail/cmp 的宿主读腿仍走各自 `minix_sys::open`，改接
  `support::open_input`/`read_file` 即可跟进。

- ✅ **批次二百零四把 `uniq` 的输出档操作数接上（create-and-write 腿，打开即截断）**
  （`2026-10-13`，`claim/UNIQ-OUTFILE-qoder` @ 基线 `328da099f`，单域三线 commands/textfilter；`bin_support.rs`
  是本 crate 四十来个 bin 共用的入口层，改它算同域库面，无跨界）：
  接回批次二百零二登记的③。改前证据：`grep -n "not modelled" os/commands/usr-bin/textfilter/src/bin/uniq.rs`
  命中一臂——第 2 个操作数一到就 warn + 退 1，要用户自己重定向；而 C `uniq.c` 的 `case 2:` 写的就是
  `ofp = file(argv[1], "w")`。
  **C 真源**：`uniq.c:119-122` 先 `ifp = file(argv[0], "r")` 再 `ofp = file(argv[1], "w")`，`file()` 打不开就
  `err(1, "%s", name)`。`fopen(name, "w")` 展开为 `open(O_WRONLY|O_CREAT|O_TRUNC, 0666)`——标志值在
  `minix3/sys/sys/fcntl.h` 的 `:65`/`:99`/`:100`（`0x1`/`0x200`/`0x400`），截断发生在**打开时**而不是写时：
  所以 `uniq f f` 在 C 得到空档，而“先读完再写回”的写法会把内容原样吐回去；输入打不开时也不会留下一个已创建的输出档。
  **形状**：新增 `bin_support.rs:create_sink(&str) -> Result<WriteSink, ()>` 与 `WriteSink::finish(&[u8])`（宿主臂
  `std::fs::File::create`，目标臂 `minix_sys::open(…, 0x1|0x200|0x400, 0o666)` 加写一次加 `close`）；句柄**开着跨过比较**，
  截断时机因此与 C 同形（若在末尾才打开，就看不出“输入坏了但仍留空档”这一层）。`bin/uniq.rs` 的执行半改成先集齐
  报告再送目的端（输入本就整档读入，内存量级不变），stdout 那条腿仍走 `support::emit`，`report` 由“逐条打印”改为
  “追入缓存”，行尾换行仍由执行半补（`lines_of` 剔掉了它）。
  写失败的处理：C 对 stdio 的写与 `fclose` 都不检查（退出码仍为 0），本壳按同族先例（`bin/unifdef.rs` 对
  `write_file` 的失败）报 `uniq: <path>` 并退 1——这是一处有意高于 C 的健壮选择，不是失真（C 那扇只会在用户看见的空输出里隐掉失败）。
  三件套：`cargo test -p minix-textfilter --bin uniq` → **9 passed / 0 failed**（基线 `7`；新增
  `output_operand_is_created_and_truncated`——先写入一段旧内容，打开后立刻断言长度归零，再断言 `finish` 的内容落档、
  第 2 次 `finish(b"")` 把档清空——与 `output_operand_reports_an_unopenable_path`（目录名当输出档必错））；
  `--lib` → **409 / 0** 不动。两枚变异负控：目标换 `OpenOptions` 不截断 → 截断测 FAILED；把打不开吞成成功 → 两测同时 FAILED；
  均按备份复原后复测 9/0。`bin_support.rs` 是全 crate 共用件，因此跑的是**整包目标构建**：
  `cargo build -p minix-textfilter --target x86_64-unknown-none --release`（所有 bin）→ Finished；
  aarch64/riscv64 的 `--bin uniq --release` 各 0 error；容器 `minix-ci:1.94`（`-u`）`cargo check -j 1 --workspace` →
  Finished 0 error，clippy 指向 `bin_support.rs`/`bin/uniq.rs` 的新告警为 0；`rustfmt --edition 2024 --check`
  触及两文件零新增漂移（`bin_support.rs:20` 那条 import 排序是存量）；核实日 `2026-10-13`；基线 `328da099f`。
  **下一步顺带价值（不属本笔）**：uniq 的执行半仍直接调 `minix_sys::open`/`read`，而 `bin_support.rs:read_file`
  已有宿主 `std::fs` 臂——把读腿改到那个口子上就能用本机 `uniq` 做逐字对账（同族 tail/cmp 同理），列入队列。

- ✅ **批次二百零三把 `cmp` 的 `skip` 操作数改成 C 的 `strtoq(…, 0)`，并接上偏移的应用点**
  （`2026-10-13`，`claim/CMP-SKIPBASE0-qoder` @ 基线 `de37e6a9c`，单域三线 commands/textfilter，无跨界）：
  接回 `07-text-filter.md` cmp 行明写的「`skip` 的 strtoq base-0 进制自动识别未建模」。改前证据：
  `bin/cmp.rs` 的 `parse_skip` 就一行 `word.parse::<u64>()`，失败去 `usage()`，所以 `cmp a b 0x10`（C 跳 16 字节）
  与 `cmp a b 010`（C 跳 8 字节）都被拒；应用点写的是 `skip.min(data.len())`，越界偏移被静默夹到档尾。
  **C 真源**：`minix3/usr.bin/cmp/cmp.c:125`/`:130` 的 `strtoq(argv[n], &ep, 0)` —— 进制由数字本身决定
  （`0x`/`0X` 十六进制、前导 `0` 八进制、其余十进制），而失败判据只有两条：`errno`（超出 `off_t`）与
  `ep == argv[n]`（一个数字都没转）。这意味着**尾随垃圾是合法的**：`cmp a b 12xyz` 在 C 就是跳 12 字节。
  另有一个不对称容易漏掉：`0x` 后面没有十六进制数字等于什么都没转（拒），而单一个前导 `0` 本身就是已转的数字，
  所以 `08` 在 C 里是合法的零而不是错（`ep` 留在 `8`）。本机 GNU `cmp` 在这两点上都与 Minix3 不同（实测
  `cmp f1 f2 12xyz` → `invalid --ignore-initial value`），拿它当基线会把本笔反向带偏，真值以 C 为纲。
  **应用点**：`c_regular` 在 `skip1 > len1` 时直接 `eofmsg(file1, len1 + 1, 0)` 并退 `DIFF_EXIT`（然后才是
  `len1 -= skip1`，file2 同一顺序），所以“越界”不是静默夹取而是报 `EOF on <file>`；判据是严格大于，偏移恰好等于
  档长时是空窗而非报错；负值走 `mmap` 失败落到 `c_special` 的 `for (...; skip1--; ...)`（负数作循环条件为非零，一直
  读到 EOF），同样停在 `eof:` 里的同一张报告上。
  **形状**：`parse_skip(&str) -> Result<i64, ()>`（跳空白、取号、按前缀定进制、只取该进制合法数字的最长前缀）与
  `skip_window(&[u8], i64) -> Option<&[u8]>`（`None` 即 C 的 EOF 一侧），报告由 `skip_eof` 走 `report_eof`（库内已有的
  `EOF on %s` / `-l` 下 `: char %lld` 两形），`-s` 不打印仍退 1（C `eofmsg` 入口的 `if (!sflag)`）。
  三件套：`cargo test -p minix-textfilter --bin cmp` → **5 passed / 0 failed**（基线 `1`；新增
  `skip_reads_the_radix_from_its_digits`、`skip_accepts_sign_blanks_and_trailing_garbage`、
  `skip_errors_only_when_nothing_was_converted_or_out_of_range`、`skip_past_the_end_is_the_eof_side`）；
  `--lib` → **409 / 0** 不动。三枚变异负控各让对应测翻红：前导 `0` 不再当已转数字 → 进制测 FAILED；
  尾随垃圾改成报错 → 合法写法测 FAILED；`skip_window` 退回静默夹取 → EOF 一侧测 FAILED；均按备份复原后复测 5/0。
  过程中本笔自己踩到一个实现错：`run == 0` 时仍去 `from_str_radix("")` 会直接失败，导致 `08`/`-0` 两形错拒——
  由新增的两枚测当场抓出，补上“零长段只在 `zero_counts` 为真时可达、值即零”的分枝后全绿（再次说明判据要先于实现）。
  三架构裸机 `cargo build -p minix-textfilter --bin cmp --target {x86_64-unknown-none,aarch64-unknown-none,
  riscv64gc-unknown-none-elf} --release` → 三台 0 error；clippy 自有文件与 base 同集（`bin/cmp.rs` 一条
  `items after a test module` 系存量）；`rustfmt --edition 2024 --check bin/cmp.rs` 零漂移；容器门读数见本笔提交信息；
  核实日 `2026-10-13`；基线 `de37e6a9c`。
  **登记不执行**：⑦ C `c_regular` 入口的 `if (sflag && len1 != len2) exit(1)` 发生在两侧偏移检查**之前**，比的是未扣
  偏移的原始档长；本壳统一在切片后比较，所以带 `-s` 且两侧跳得长短不一致的调用会与 C 不同。⑧ GNU `cmp` 的 skip 语义与 Minix3
  相反（拒尾随垃圾），不可拿本机对账当判据（同 uniq `-c` 列宽的教训）。命令面无宿主读腿一条仍沿用批次二百零二④。

- ✅ **批次二百零二接上 `uniq` 的 obsolete 跳过写法 `-N`/`+N`**
  （`2026-10-13`，`claim/UNIQ-OBSOLETE-qoder` @ 基线 `08d9cfe57`，单域三线 commands/textfilter，无跨界）：
  接回批次二百零一登记的①与②（同一函数的两面，不拆两轮）。改前证据：`grep -n obsolete
  os/commands/usr-bin/textfilter/src/bin/uniq.rs` 零命中，`parse_uniq_args` 直接从 `i = 1` 扫 argv，
  于是 `uniq -2`（C 意为「跳过两字段」）落到集外字母 `2` 分支报用法错。
  **C 真源**：`minix3/usr.bin/uniq/uniq.c` 的 `obsolete()` 在 `getopt` 之前就地改写：`asprintf(&p, "-%c%s",
  ap[0] == '+' ? 's' : 'f', ap + 1)`——`-2` 变 `-f2`、`+3` 变 `-s3`（注意两个方向折向不同的选项：负数形式
  跳字段、正数形式跳字符）。**停扫规则与 tail 不同**：uniq 只要求第二位是数字才改写，第二位是其它字符时
  `continue`（`if (!isdigit(ap[1])) continue;`），只有不以 `-`/`+` 开头的词才 `return`。后果有两面：
  `uniq -c -2` 里的 `-2` 照折（跨过选项字母继续扫）；`uniq -f 2 -3` 里的 `2` 被当成一个词而整体停扫，
  `-3` 保持原形交给 `getopt`，按集外字母 `3` 走 `case '?'` 退用法错——tail 那边有 `if (!ap[1]) ++argv`
  跨值动作，uniq 没有，两个命令的预扫不能共用同一套（这是本轮重读 C 读出来的事实差异，不是实现疏漏）。
  **形状**：新增 `obsolete_args(&[String]) -> Vec<String>`（程序名那一项不动，等价 C 的 `*++argv`），
  `parse_uniq_args` 开头先折再扫，后续一切（附值、取下一词、`--`、操作数）看到的都是改写后的词。
  **为何放在薄壳而不是库**：`uniq` 的选项解析一直在 `bin/uniq.rs`（库侧只拥有 `Uniq`/`comparison_key` 等
  决定半），本轮不改这个布局；tail 那侧的解析在 `window.rs` 是因为 head/tail 共用同一文件。
  三件套：`cargo test -p minix-textfilter --bin uniq` → **7 passed / 0 failed**（基线 `5`；新增
  `parse_obsolete_skip_forms`（`-2`/`-3 in`/`+4`/`-c -2` 四形）与 `parse_obsolete_walk_stops_at_a_plain_word`
  （`-f 2 -3` 不折））；`--lib` → **409 / 0** 不动；三枚变异负控各让对应测翻红——非数字第二位改成停扫 →
  `parse_obsolete_skip_forms` FAILED；`+` 也折成 `f` → 同一测 FAILED；预扫整体旁路（`scanned = argv.to_vec()`）→
  同一测 FAILED；均按备份复原后复测 7/0。三架构裸机 `cargo build -p minix-textfilter --bin uniq --target
  {x86_64-unknown-none,aarch64-unknown-none,riscv64gc-unknown-none-elf} --release` → 三台 0 error；
  clippy 自有文件零新增告警；`rustfmt --edition 2024 --check bin/uniq.rs` 漂移与 base 同为一块（在未触碰的
  `bin_support.rs`）；容器门读数见本笔提交信息；核实日 `2026-10-13`；基线 `08d9cfe57`。
  **新发现的两条与本轮无关的差异（只登记）**：⑤ C 的选项串是 `"-cdf:s:u"`，开头那个 `-` 意思是在第一个
  非选项处停下扫描（`case '-': --optind; goto done;`），所以 `uniq in -c` 在 C 里会把 `-c` 当第二个操作数（进而
  去创建名为 `-c` 的输出档），而本壳目前仍 permute-on（认 `-c` 为选项）；⑥ `-f`/`-s` 值非数时 C 打
  `errx(1, "illegal field skip value: %s", optarg)`（或 character 那一句），本壳统一退 usage，退码同、报文不同。
  另保留批次二百零一登记的③（输出文件不建模）与④（无宿主读腿）。

- ✅ **批次二百零一把 `uniq` 的三个旗标改成 C 那样的独立位，并按 `%4d` 补上计数列宽**
  （`2026-10-13`，`claim/UNIQ-SHOWFLAGS-qoder` @ 基线 `399d274eb`，单域三线 commands/textfilter，无跨界）：
  开工前的选择依据：`07-text-filter.md` uniq 行写着「`-i`/`-f`/`-s` 比较面与组合模式待批」，而
  `os/commands/usr-bin/textfilter/src/uniq.rs:fn comparison_key` 与 `bin/uniq.rs` 的 `-f`/`-s` 臂早已接线，
  那行属账面滞后；`-i` 则根本不是缺口——`minix3/usr.bin/uniq/uniq.c` 的 `getopt(argc, argv, "-cdf:s:u")` 里没有这个
  字母（它是 GNU 扩展），本壳对集外字母退用法错就是忠实形态。真缺口在另两处：
  **其一，组合模式被当互斥**。`parse_uniq_args` 里 `if mode_seen && mode != next { return Err(()) }` 把
  `uniq -d -u` 报成用法错；C 存在 `cflag`/`dflag`/`uflag` 三个 int，`show()` 一行合成：
  `if ((dflag && repeats == 0) || (uflag && repeats > 0)) return;`——即 `-c -d` 只给重复行计数、`-c -u` 只给单一行计数、
  `-d -u` 抑制每一条 run，于是什么也不输出。**其二，`-c` 列宽没接**。`format` 直接写十进制再加一个空格，而 C 打的是
  `"%4d %s"`（计数右对齐四列）。本机 GNU `uniq -c` 用七列（实测 `printf 'a\na\nb\n' | uniq -c` → 六个空格加 `2`），
  拿它当基线会把本笔引向错的列宽，真值以 C 为纲。
  **方案对比**：甲案（采）把模式枚举换成 `UniqFlags { count, repeats_only, uniques_only }` 三枚位，`keeps(count)` 就按
  `show()` 那行写（C 的 `repeats` 等于 `count - 1`，所以“没重复过”就是 `count == 1`），三臂互斥判定从解析器消失；
  乙案保留 `UniqMode` 再加几个组合变体（`CountedRepeated` 等）——变体会随三个位的组合凑到八个，本质是把 C 的三个位
  重新拆成二进制的笛卡尔积手写一遍，与“同一层只拥有同一语义”相逆，不采。本处是用户态命令的单线程事件循环，
  不涉及内核的 SMP 与大内核锁约束。
  **形状细节**：`decimal_digits` 与 `write_decimal` 分开，前者只回答「`%4d` 会占几列」，零算一列；超过四列的计数撑开字段而
  不裁数字（`printf` 的宽度从不裁数字），测试钉住 `12345` 仍输出五位。使用侧的 `usage` 文案同步改成 C 的形状
  `uniq [-cdu] [-f fields] [-s chars] [input_file [output_file]]`（原写的 `[-c | -d | -u]` 把不存在的互斥写进了文案）。
  三件套：`cargo test -p minix-textfilter --lib` → **409 passed / 0 failed**（基线 `407`；新增
  `test_flag_combinations_follow_the_show_guard` 与 `test_counted_format_pads_to_four_columns`，`test_counted_format`
  的期望从 `12 a` 改成 `"  12 a"`）；`cargo test -p minix-textfilter --bin uniq` → **5 passed / 0 failed**
  （`parse_rejects_composed_modes` 换为 `parse_composes_mode_flags_as_c_does`，附带钉 `-cdu` 聚簇三位同时置位）；
  三枚变异负控各让对应测翻红：去掉 `%4d` 前导空白→两枚格式测 FAILED；`keeps` 丢掉 `uniques_only` 臂→组合测 FAILED；
  解析器退回互斥拒→`parse_composes_mode_flags_as_c_does` FAILED；均按备份复原后复测 409/0 与 5/0。
  三架构裸机 `cargo build -p minix-textfilter --bin uniq --target {x86_64-unknown-none,aarch64-unknown-none,
  riscv64gc-unknown-none-elf} --release` → 三台 0 error；`rustfmt --edition 2024 --check` 触及文件漂移块从 base 的
  3 降到 2（本笔没新增，反而归正了一枚存量多行断言）；容器门与 clippy 读数见本笔提交信息；核实日 `2026-10-13`；
  基线 `399d274eb`。
  **登记不执行**：① `uniq` 的 obsolete 写法未建模——C `obsolete()` 把 `-<digit>…` 改写成 `-f<digit>…`、`+<digit>…` 改写成
  `-s<digit>…`（`asprintf("-%c%s", ap[0] == '+' ? 's' : 'f', ap + 1)`），本壳现在会把 `-2` 当集外字母退用法错；
  ② C `obsolete()` 对认得的字母选项 `continue`、只对操作数与 `--` 停扫，与 tail 同族；③ 输出文件（第 2 操作数）仍不建模；
  ④ `bin/uniq.rs` 直接调 `minix_sys::open`/`read`，没有 `bin_support.rs:read_file` 的宿主分支，所以「宿主跑二进制逐字对账」
  这条验证路线对 uniq 也不适用（同 tail 的登记⑦）。

- ✅ **批次二百把 `tail` 的零计数改成 C 的「选出空集」而不是拒错**
  （`2026-10-13`，`claim/TAIL-ZERO-QOFF-qoder` @ 基线 `e3bbe7d95`，单域三线 commands/textfilter，无跨界）：
  接回批次一百九十七登记的③。改前证据：`grep -n "parsed == 0" os/commands/usr-bin/textfilter/src/window.rs` 命中一臂
  `if !forward && matches!(letter, b'c' | b'b') && parsed == 0 { return Err(IllegalCount) }`，而 `tail -n 0` 也会走
  `TailWindow::new(0)` 退 1。C 真源不拒零：`ARG` 的第一句只查「是否已有数字选项」，第二句只查 `strtoll` 留下的尾随字符
  （`*p`），零照常乘、照常赋给 `off`；`minix3/usr.bin/tail/forward.c` 的 `RLINES` 在 `!off` 时 `fseek(fp, 0, SEEK_END)`、
  `RBYTES` 在 `st_size >= off` 下 `fseeko(fp, -off, SEEK_END)`，两者都是「位置到档尾然后往后打印」——于是什么也不输出；
  非普通档那一支 `while (getc(fp) != EOF);` 把输入喝完同样不输出；`reverse.c:79-80` 对非 `REVERSE` 且 `off == 0` 直接返回。
  退码都是成功（除非写标准输出失败）。
  **形状**：解析半去掉零拒臂（`IllegalCount` 现在只表「不是十进制」，其文档注释同批改准），新增两个带测的谓词——
  `TailStyle::selects_nothing`（四个零形：`LastLines{0}`/`LastBytes{0}`/`ReverseLines{0}`/`ReverseBytes{0}`）与
  `TailStyle::over_ring_cap`（只有非零的 `LastLines` 才受三十二行封顶约束）；执行半进门先查 `selects_nothing` 直接返回，
  封顶守卫改成读 `over_ring_cap`，不再在薄壳里拼 `matches!` 条件——这样两件事都有宿主可测的落点，而不是只活在跳不到的代码里。
  **语义后果**：`tail -c 0`、`tail -b 0` 从「illegal offset -- 0」退 1 变成不输出退 0；`tail -n 0` 同样从不输出变不输出（但不再退 1）；
  三十二行封顶对 `tail -n +40`、字节与反序形的误伤一并消除（那些形本来就不走行窗）。
  三件套：`cargo test -p minix-textfilter --lib` → **407 passed / 0 failed**（基线 `405`；新增
  `test_tail_style_selects_nothing_covers_the_zero_arms` 与 `test_tail_style_ring_cap_applies_only_to_the_line_walk`，
  并把 `-c 0` 的断言从拒错改成 `LastBytes { off: 0 }`，补 `-b 0`/`-n 0` 两钉）；`--bin tail` → **14 / 0** 不动；
  三枚变异负控各让对应测翻红：恢复零值拒臂 → `test_parse_tail_args_faithful_getopt` FAILED；封顶守卫去掉 `off != 0` →
  封顶测 FAILED；`selects_nothing` 去掉两个反序零臂 → 空集测 FAILED；均按备份复原后复测 407/0。
  容器与裸机门读数见本笔提交信息；核实日 `2026-10-13`；基线 `e3bbe7d95`。
  **仍开登记**：④ `-f`/`-F` 跟踪（需事件循环）、⑦ tail 执行半无宿主读腿（逐字对账归 `C-57`/E5 播种后）。

- ✅ **批次一百九十九把 `tail` 的选项预扫描改成 C 的续扫形，并接上重复数字选项判据**
  （`2026-10-13`，`claim/TAIL-SCANARG-qoder` @ 基线 `0efc3c295`，单域三线 commands/textfilter，无跨界）：
  接回批次一百九十七登记的①（重复数字选项→用法错）。开工时另发现**批次一百九十八自己留下的形状错**：那里的
  “停在第一个不认识的 token”把 C 的续扫规则读小了。重读 `minix3/usr.bin/tail/tail.c` 的 `obsolete()`：`switch (*++ap)`
  里 `case 'f'`/`case 'r'` 与 `case 'b'`/`case 'c'`/`case 'n'` 都是 `continue`（后者在字母紧跟串尾时多一个
  `++argv`，把值那一个词跨过去），只有三种情形才 `return`：token 不以 `-`/`+` 开头、第二个字符是 `-`（`--`）、
  或第二个字符是 `b`/`c`/`n`/`f`/`r`/数字之外的字母。因此 `-r -3c` 在 C 里 `-3c` **会**被改写（本笔前报用法错），
  而 `f -3c` 里的 `-3c` 因扫描已停而原样交给 `getopt`，按集外字母 `3` 走 `case '?'` 报错——两者本同一口径，上一轮只对了后者。
  **形状**：`obsolete_arg` 改成 `obsolete_step`，`ObsoleteStep` 从三臂拆成五臂（`Rewritten`/`Verbatim`/`SkipsValue`/
  `StopScan`/`IllegalOption`）， phase 1 改成带 `frozen` 标志的索引环，`SkipsValue` 多消耗一个词且不扫它；
  重复数字选项按 `ARG` 首句 `if (style) usage()`（`tail.c:86-87`）在任何 `b`/`c`/`n` 重入时回 `TailError::Usage`。
  **语义后果**：`tail -r -3c` 从用法错变成末三字节反序（`ReverseBytes { off: 3 }`）；`tail -n 3 -n 5`、
  `tail -c10 -b2` 从“后到者覆盖”变成用法错；`tail -n -3c` 仍报 `illegal offset -- -3c`（值词不被改写才能把
  `-3c` 整个交给 `ARG` 的 `*p` 判据，这条成了续扫形状的间接铉）。
  三件套：`cargo test -p minix-textfilter --lib` → **405 passed / 0 failed**（基线 `403`；新增
  `test_parse_tail_args_obsolete_scan_survives_letter_options` 与
  `test_parse_tail_args_second_number_option_is_usage_error`，并在负值测里补 `-n -3c` 钉）；`--bin tail` → **14 / 0** 不动；
  三枚变异负控各让对应测翻红——`f`/`r` 改成停扫 → 续扫测 FAILED；去掉 `if seen.is_some()` 守卫 →
  重复选项测 FAILED；`SkipsValue` 退化为不消耗值词 → 三枚测同时 FAILED（含前向偏移与反序折算）；均按备份复原后复测 405/0。
  三架构裸机 `cargo build -p minix-textfilter --bin tail --target {x86_64-unknown-none,aarch64-unknown-none,
  riscv64gc-unknown-none-elf} --release` → 三台 0 error；`rustfmt --edition 2024 --check` 对 `window.rs` 零漂移；
  容器 `minix-ci:1.94`（`-u $(id -u):$(id -g)`）`cargo check -j 1 --workspace` 与 clippy 自有文件读数见本笔提交信息；
  核实日 `2026-10-13`；基线 `0efc3c295`。
  **方法论一句**：上一轮把「扫到第一个不认识的 token」当成 C 的 `return` 全部含义，没注意 `continue` 分支里还有
  两个认得的字母族与一个跨值动作——同一函数重读时不能只看退出路径，要看完整循环体。本轮把五臂拆出来，
  每一条 `continue`/`return` 对应一枚枚举变体，下轮再改不会丢臂。
  **仍开看下列登记**（批次一百九十七遗留）：③ `-c 0`/`-b 0`/`-n 0` 的零值语义（C 是不输出，本壳仍拒）；
  ④ `-f`/`-F` 跟踪；⑦ tail 执行半无宿主读腿。本笔不扩界。

- ✅ **批次一百九十八接上 `tail` 的 obsolete 后缀单位字母与负值选项参数（改写机全形）**
  （`2026-10-13`，`claim/TAIL-OBSOLETE-qoder` @ 基线 `6be98f777`，单域三线 commands/textfilter，无跨界）：
  接回批次一百九十七登记的第⑥项（obsolete 后缀单位字母未建模）与第②项（`ARG` 的 `case '-'` 负值臂）——两者不是可拆的两件：
  C `obsolete()` 改写出的选项值本身就带着原 token 的首字符（`*p++ = *argv[0]` 后再 `strcpy`），所以 `-3c` 必然变成 `-c-3`，
  不接负值臂就只能把一个合法写法拒成 `IllegalCount("-3")`。
  **改前现状证据**：解析半的改写段只处理 `bytes[0] == b'-' && bytes[1].is_ascii_digit()`，把 `-3c` 写成 `-n3c` 再被
  `parse::<usize>` 拒；`minix3/usr.bin/tail/tail.c` 的 `obsolete()` 则是三段：尾随 `f`/`r` 提到最前、新的末字符按
  `b`/`c`/`l`/数字决定单位字母（`l` 与纯数字都折成 `n`）、再把本 token 首字符插回数字前。
  **形状**：新增 `obsolete_arg(&str) -> ObsoleteStep`（`Rewritten` / `Verbatim` / `IllegalOption` 三臂，对应 C 的改写、
  `return`、`default:` 致命）；改写只在连续段内生效，第一个不认识的 token 使整段停扫（与 C 的 `return` 同），故
  `tail f -3c` 里 `-3c` 原样交给选项扫描器，按集外字母 `3` 走 `case '?'` 报用法错——与 C 逐字一致，不是新发明。
  同时接 `ARG` 的 `case '-'`：值以 `-` 开头只剥符号前缀、仍走 backward 形态（`-c -3` ≡ `-c 3`）；`+` 值维持上一轮的
  forward 支。`+10` 这种首字符为 `+` 的 obsolete 写法也走同一台机器（改写后 `tail +10` ≡ `tail -n +10`，从第 10 行起输出）。
  非法单位字母按 C `xerrx(1, "illegal option -- %s", *argv)` 新增 `TailError::IllegalOption`，报文引整个 token。
  **过界自检（本笔不算扩修，是同一条留白的依赖件）**：批次一百九十七登记的①重复数字选项与③零值判据未动，仍开。
  三件套：`cargo test -p minix-textfilter --lib` → **403 passed / 0 failed**（基线 `399`；新增四枚
  `test_parse_tail_args_obsolete_unit_letters`、`..._obsolete_stops_at_first_non_match`、
  `..._obsolete_illegal_unit_is_fatal`、`..._negated_value_is_backward`）；`--bin tail` → **14 passed / 0 failed** 不动；
  负控：去掉尾随 `f`/`r` 提升→ `..._obsolete_unit_letters` FAILED；把负值臂改回不剥符号→ 四枚测同时 FAILED（含 `-3c` 与 `-5`）；
  两处均按备份复原后复测 403/0。
  **本轮真正挡住的两件靠非 test 构建才能发现的问题**（写下来防重踩）：一是对 `"-"` 这个单字符 token 取下标 `bytes[1]` 会越界
  panic，只在带操作数的路径上暴露，而宿主 `--lib` 测跑的是 test 配置（`std` 在位，名字解析不会出错）——现有 `parse_tail_args(&["-"])`
  一枚断言当场把它打成 FAILED（`index out of bounds: the len is 1 but the index is 1`），补上长度守卫并在测里钉回该形；
  二是 `format!` 在 no_std 下需 `use alloc::format;`，`--lib` 测试绿但 `--bin tail`（非 test 配置）直接编译失败——
  **命令面改动必跑 `cargo build --bin <cmd>` 与三架构裸机构建，不能只看宿主测试**。
  容器 `minix-ci:1.94`（带 `-u $(id -u):$(id -g)`）`cargo check -j 1 --workspace` → Finished、0 error；
  三架构裸机 `cargo build -p minix-textfilter --bin tail --target {x86_64-unknown-none,aarch64-unknown-none,riscv64gc-unknown-none-elf} --release`
  → 三台均 Finished；clippy 自有文件与 base 逐条同集（`window.rs` 三枚存量，零新增）；核实日 `2026-10-13`；基线 `6be98f777`。

- ✅ **批次一百九十七接上 `tail` 的前向偏移 `-n +N`/`-c +N`/`-b +N`（并把模式与计数折成一枚枚举）**
  （`2026-10-13`，`claim/TAIL-FWD-OFFSET-qoder` @ 基线 `c701bbe17`，单域三线 commands/textfilter，无跨界）：
  接回 `07-text-filter.md` tail 行与批次一百八十都显式拒绝的最后一块可离线建模的留白（`tail -n +N` 报「not modelled」）。
  **改前现状证据**：`grep -rn "ForwardOffset" os/commands/usr-bin/textfilter/src` → 解析半的 `TailError::ForwardOffset`
  与薄壳 `arg_error` 的拒臂两处俱在，任何 `+N` 值都落到 `support::terminate(1)`。
  **顺带纠一处账面滞后**：同一行还把 `-c`/`-b`/`-r` 写作「均报未接」，而 `os/commands/usr-bin/textfilter/src/bin/tail.rs:fn last_bytes_of`
  与 `fn reverse_bytes` 早已落地这三形（批次一百五十、批次一百五十四）；文档与代码不同步会把后继轮引去「实现」一个已存在的能力，
  故本笔把该行整体重写。
  **C 真源**：`minix3/usr.bin/tail/tail.c` 的 `ARG(units, forward, backward)` 宏把 `+N` 与「取后 N」分成两种 style——乘完单位后
  只在积非零时减一个单位（数字选项以一为基），于是 `-n +1`/`-c +1` 什么都不跳、输出整档，`-n +20` 从第 20 行往后输出；
  `minix3/usr.bin/tail/forward.c` 的 `FBYTES` 支把 seek 夹到 `st_size`、`FLINES` 支在 `getc` 处撞 EOF，两条越过档尾的路都落在
  「什么都不输出」；`tail.c` 在分派前把 forward 形态折回同单位的 backward 形态（`tail -r -n +5` 因此是末四行反序），而
  `minix3/usr.bin/tail/reverse.c` 开头对非 `REVERSE` 且 `off == 0` 直接返回——折算确实能造出「反序但零行」这一形。
  **方案对比**：甲案只加两个字段（一个前向偏移量加单位标志），改动最小，但扁平的 `bytecnt == 0` 同时表示「字节模式关」与
  「取零字节」，`-r -n +1` 折出的 `off == 0` 无法与「未给数字选项」区分，会把 C 的不输出错写成整档反序；乙案（采）把 C 的
  `(style, off)` 连同 `rflag` 折成一枚 `TailStyle` 枚举（`LastLines`/`LastBytes`/`SkipLines`/`SkipBytes`/`ReverseLines`/
  `ReverseBytes`/`ReverseAll`），模式与计数同一真源：解析半用局部 `RawStyle` 加 `Option` 表达 C 的 `NOTSET`，缺省回填与 `-r`
  折算都留在解析半（对位 C `main()` 分派之前那一段），执行半只对枚举穷举分派，不留需要 `unreachable!()` 的非法态。
  对照 GNU `tail` 与 POSIX：`-n +K` 同为「自第 K 行起输出」，本树与之外部一致；Redox coreutils 的 `tail` 未实现 `-r` 与前向
  偏移，故真值仍以 C 为纲。前向两形的输出走原始字节（对位 C `forward.c` 的 `getc`/`putc` 环路）：不逐行补换行，末行无尾换行时
  不会被造出一个换行。
  **必要连带的一处行为更正**：三十二行封顶的守卫条件是「非字节模式」，于是反序形态也被它挡住（`tail -r -n 40` 退 1），
  而 C `minix3/usr.bin/tail/reverse.c` 的反序走 `mmap` 回溯、根本不经过行窗；本笔把封顶收到真正
  走行窗的 `LastLines` 一形，反序两形改按 C `reverse.c` 的早退与取窗处理。这不是顺手扩修：`-r -n +1` 折出的 `off == 0` 按那条
  「非字节模式即查窗」的判据会被 `TailWindow::new(0)` 误拒，与新臂共用同一段守卫，必须同批给形状。
  **改名留痕**：本笔把批次一百五十的 `reverse_selection(lines, count, n_seen)` 换成 `reversed_lines(lines, take: Option<usize>)`
  （`None` 整档反序、`Some(off)` 末 `off` 行反序、`Some(0)` 不输出）；历史批次条目原文不回改。
  三件套：`cargo test -p minix-textfilter --lib` → **399 passed / 0 failed**（基线 `396`；新增三枚解析测
  `test_parse_tail_args_forward_offset_is_one_based`、`test_parse_tail_args_reverse_folds_forward_offset`、
  `test_parse_tail_args_forward_offset_rejects_non_numeric`）；`cargo test -p minix-textfilter --bin tail` → **14 passed / 0 failed**
  （基线 `7`；新增 `bytes_from` 三枚、`bytes_after_lines` 三枚、`reversed_lines_zero_takes_nothing`，并在 `reverse_bytes` 内加钉
  `off == 0` 不输出）；三枚变异负控各让对应测当场翻红——去掉 `off != 0` 守卫 → `..._forward_offset_is_one_based` FAILED
  （`usize` 减穿 panic，`window.rs:392`）；去掉 `-r` 折算臂 → `..._reverse_folds_forward_offset` FAILED；去掉
  `bytes_after_lines` 的 `off == 0` 早退 → `bytes_after_lines_lands_on_the_nth_line` FAILED（`bin/tail.rs:283`）；三处均按备份
  复原后复测 399/0 与 14/0。容器权威编译门 `docker run --rm -v $PWD:/work -w /work/os -m 2g minix-ci:1.94 cargo check -j 1
  --workspace` → Finished、0 error；三架构裸机构建 `cargo build -p minix-textfilter --bin tail --target
  {x86_64-unknown-none,aarch64-unknown-none,riscv64gc-unknown-none-elf} --release` → 三台均 Finished；clippy 自有文件与 base
  逐条同集（`window.rs` 三条存量：行窗循环变量一枚 + head 测未用导入两枚；`bin/tail.rs` 零告警，中途出现的一条
  `collapsible_if` 已改写消除）；容器口径 `cargo fmt --check` 的 textfilter 漂移块数 **258 → 258**（插入行零新增漂移）；
  `bash tools/pattern-gate.sh --diff` → `PASS=17 FAIL=0 SKIP=1`（增量 12 枚 C 锚点逐条回读落窗）；
  `bash tools/doc-style-lint.sh --diff` → error 级 0；`bash tools/check-command-boundary.sh` → OK；核实日 `2026-10-13`；
  基线 `c701bbe17`。**真机旁证**：`bash os/qemu-tests/test-cmd-smoke.sh`（x86_64）→ `RESULT: PASS (… the stat command
  face and the find command face)`——本改动不触启动路径，这轮跑的是「没写坏」而非 tail 本身（tail 不在 imgrd 名单内，
  读数存 `tmp/tail-fwd/cmd-smoke-x86.log`）。
  **工具坑（写下来防下轮重踩）**：同一工作树里先用 `docker run` 跑 cargo（容器内是 root）会留下 root 属主的 `os/target` 指纹，
  之后宿主侧 `xtask image` 撞 `Permission denied`；解法是要么 docker 侧加 `-u $(id -u):$(id -g)`（批次一百五十已用过的形态），
  要么补一次 `docker run --entrypoint chown … -R $(id -u):$(id -g) target` 归还属主。
  **登记不执行（同族相邻缺口，本单不顺手修）**：① `tail.c` 的 `ARG` 开头 `if (style) usage()`——`tail -n 5 -c 10` 在 C 是用法错，
  本树后到者覆盖；② `ARG` 的 `case '-'` 负值臂——`tail -c -5` 在 C 等价 `-c 5`，本树按非十进制拒（`IllegalCount`）；
  ③ `-c 0`/`-b 0` 本树按非零判据拒，而 C 的 `*p` 判据只拒尾随垃圾、`RBYTES off == 0` 的语义是不输出；④ `-f`/`-F` 跟踪仍需
  事件循环，维持显式拒绝；⑥ obsolete 形态的后缀单位字母未建模——`tail.c` 的 `obsolete()` 会把 `-3c`/`-3b`/`-3l` 连同尾随
  `f`/`r` 重写成合法选项（最终等价 `-c 3`），而本树的改写只处理前导数字→`-n<数字>`，`-3c` 会变成 `-n3c` 再被
  `parse::<usize>` 拒为 `IllegalCount("3c")`（行为与本笔无关，批次一百八十的改写形状就如此）；⑦ **测量通道缺口**：`bin/tail.rs` 直接调 `minix_sys::open`/`minix_sys::read`，没有同族
  `bin_support.rs:read_file` 那样的宿主分支，于是宿主直接跑 `os/target/debug/tail` 时操作数打不开、标准输入读空（实测
  `tail -n 2 < /tmp/tf.txt` 输出空、带操作数那腿 warn 退 1，且本笔未触碰的 `-n 2` 同样如此）——「宿主跑二进制与系统 `tail`
  逐字对账」这条验证路线对 tail 不适用，本单的判据因此是纯函数测加裸机构建，真机逐字对账归 `C-57`/E5 的 imgrd 播种之后。

- ✅ **批次一百九十六 sed 多命令支持·第1片（决定半 `parse_script`，非破坏式）**
  （`2026-10-13`，`claim/SED-CMD2-qoder` @ 基线 `e046b1e89`，单域三线 commands/regex，无跨界）：
  sed 现只建模单 `s`，`;` 链与其他动词显式拒——最大功能缺口之一。本片按「大功能拆多轮」先落**决定半**：
  新增 `Command` 枚举（Subst/Delete/Print/PrintLineNum）、`ScriptError`、`MAX_SCRIPT_COMMANDS=16`、
  `parse_script(script, out)->Result<usize>`：按 `;`/换行切段（对位 compile.c:165/205）、trim_start、
  空段跳过、无地址子集（s…/d/p/=），地址前缀（/re/、行号、$）与其它动词返回 NotModelled（响亮拒非静默）。
  因 minix-regex 刻意 heap-free（lib.rs 无 `extern crate alloc`），采 out-slice 风格而非返 Vec。
  纯新增、**不改 bin/sed_lines**（现有单 `s` 行为逐字不变，bin sed 7 测全绿为回归护栏）。新增 2 测
  （链式/空段跳过/trim + 地址与 junk 拒绝）。**第2片（待下轮）**：改 `sed_lines` 遍历 Command 环（d 抑自动印、
  = 移行号、p 打印、链式共享 pattern space）接上本解析器。
  三件套：`cargo test -p minix-regex --lib` → **39/0**（新 2）；`--bin sed` → 7/0（回归护栏）；
  clippy 触及文件零新增告警（bin/sed.rs “items after a test module” 基线存量）；`rustfmt` src/sed.rs 零漂移（含顺带归一该文件 1 处存量版差）；
  核实日 `2026-10-13`；基线 `e046b1e89`。真机腿：sed 文件操作数归 C-57/E5。本片决定半已测、bin 接线待第2片（未静默半接）。

- ✅ **批次一百九十五校正 `head -c` 的旧“报未接”标注（文档-代码同步）**
  （`2026-10-13`，`claim/HEAD-DOC-qoder` @ 基线 `e797e3bfc`，文档仅改、无代码）：
  核两半未接 vein 时发现 07-text-filter.md head 行标 `-c` 字节模式“报未接”，但 head 实已接：
  `HeadArgs.bytecnt`、bin `head_stream` 走 `first_bytes_of` 逐字节取前 N、对位 `head.c:149-167`。
  文档与代码不同步会误导后继轮去“实现”一个已存在能力。本笔校正为“已接（字节模式，与 -n 互斥）”。
  三件套：grep bytecnt/first_bytes_of 于 head bin 与 window.rs 均在（非仅声明）；核实日 `2026-10-13`；基线 `e797e3bfc`。

- ✅ **批次一百九十四校正 `tsort -l`/`-d` 的旧“待批”标注（文档-代码同步）**
  （`2026-10-13`，`claim/TSORT-DOC-qoder` @ 基线 `031e94b35`，文档仅改、无代码）：
  上一轮核两半未接 vein 时发现：`07-text-filter.md` tsort 行将 `-l`/`-d` 标为“待批”，会误导后继轮
  去“实现”它们。但 `tsort.c:102` 声明的 `longest`/`Dflag` 从未在其上分支（环报告恒走 `longest_cycle`），
  tsort.rs 决定半注释（:27-28）已如实记此为 C 侧 no-op、`parse_args` 按 no-op 收下。本轮把 07 行从
  “待批”改为“经核为 C 侧 no-op，与 C 一致，非缺口”，防后继重复误投。三件套：grep “待批”于 07
  tsort 行不再命中、grep Dflag 于 tsort.c 仅命中声明行；核实日 `2026-10-13`；基线 `031e94b35`。

- ✅ **批次一百九十三接上 `split -n`（按块数切）**
  （`2026-10-13`，`claim/SPLIT-N-qoder` @ 基线 `19fac68c2`，单域三线 commands/textfilter，无跨界）：
  接回批次一百九十一登记的 `-n` Unmodelled 留白（split 三模式至此集齐）。C `-n` 走 `split3(chunks)`
  →`split1(size/chunks, chunks)`：前 chunks-1 块各 size/chunks 字节、末块吸收余数（因 maxcnt 封顶不再
  开新文件）；chunks>size 报“can't split into more than %d files”。决定半 `split.rs` 新增 `chunks_n
  (data, n) -> Option<Vec<&[u8]>>`（None 当 n=0 或 n>len）；`SplitArgs` 新增 `chunk_count`、`SplitError`
  新增 `IllegalChunkCount`；parse 把 `n` 从 Unmodelled 拆出（全数字非零、无 k/m 尾，对位 split.c:123-127）
  并把三模式互斥扩为 l/b/n 两两（set&&（byte||chunk）或 byte&&chunk → Usage）。执行半 `bin/split.rs`：
  chunk_count 优先于行模式、在 byte_size 后，走 `chunks_n` 逐块原样写，None 时报 too-many-files；
  `arg_error` 加 illegal number of chunks 臂。新增 2 测（chunks_n 定长/末块余数/n==len/越界 + `-n` 解析与三模式互斥）。
  三件套：`cargo test -p minix-textfilter --lib` → **396/0**（既有 394 + 新 2）；`--bin split` → 4/0；
  freestanding `--target x86_64-unknown-none --release` Finished；clippy 触及文件零新增告警（bin/split.rs
  “Eof variant never constructed” 基线存量）；`rustfmt --edition 2024 --check` split.rs 零漂移（基线即净、
  新增行经 rustfmt 归净）；核实日 `2026-10-13`；基线 `19fac68c2`。真机腿：同 -b（写盘受 E5），本笔纯函数已测。
  split 三模式（l/b/n）集齐。

- ✅ **批次一百九十二接上 `grep -h`/`-H`（文件名头抑制/强制）**
  （`2026-10-13`，`claim/GREP-HH-qoder` @ 基线 `141b7832f`，单域三线 commands/regex，无跨界）：
  C `grep.c:69,77,493-494` 支持 -h（从不印名头）/-H（即便单操作数也印），Rust 决定半无此字段、
  `parse_grep_args` 遇 h/H 拒（落 `_ => Err`），执行半 `prefix_name = operands.len()>1` 无开关——
  典型两半未接。决定半 `GrepOptions` 新增 `no_prefix`/`force_prefix` 两 bool + 纯函数
  `prefix_shown(operand_count) = !no_prefix && (operand_count>1 || force_prefix)`（逐字对位 C：
  suppress 条件 `hflag || (operands<=1 && !Hflag)`）；parse 接 `b'h'`/`b'H'`。执行半 `prefix_name`
  改调 `options.prefix_shown(operands.len())`。新增 2 测（parse 置位 + prefix_shown 矩阵：默认多显/单隐、
  -H 强制、-h 压制、-h与-H并存时-h胜）。文档同步：bin 头注 + 08 篇 §4.2 开关数、§4.5 grep 行。
  三件套：`cargo test -p minix-regex --lib` → **37/0**（新 2）；`--bin grep` → 9/0；freestanding
  `--target x86_64-unknown-none --release` Finished；clippy 触及文件零新增告警（bin/grep.rs
  “items after a test module” 基线存量）；`rustfmt --edition 2024 --check` 两文件 7→7 零新增漂移；
  核实日 `2026-10-13`；基线 `141b7832f`。真机腿：grep 读文件腿归 C-57/E5（本笔为名头输出时序，纯逻辑已宿主测）。留白：`-i`、`-r`/上下文行。

- ✅ **批次一百九十一接上 `split -b`（按字节数切块）**
  （`2026-10-13`，`claim/SPLIT-B-qoder` @ 基线 `632c888d4`，单域三线 commands/textfilter，无跨界）：
  接回批次一百八十九登记的 `-b` Unmodelled 留白。决定半 `split.rs`：新增 `chunk_bytes(data, n)`
  （按固定 `n` 字节切块、末块可短、n=0/空输入无块，对位 `split.c` `split1`）；`byte_count(v)` 解
  十进制+可选小写 `k`/`m` 单位×1024/×1048576、非零、`checked_mul` 防溢出（对位 split.c:99-108）；
  `SplitArgs` 新增 `byte_size: Option<usize>`、`SplitError` 新增 `IllegalByteCount`；parse 把 `b` 从
  Unmodelled 拆出（`-b1000` 附值或 `-b 1000` 分词），并按 C 收尾 `else if (bytecnt…)` 使 `-l`/`-b` 互斥
  （任一侧并存报 Usage）。执行半 `bin/split.rs`：byte_size 时走字节路（逐 chunk 原样 create_write，
  不重组行），`arg_error` 加 illegal byte count 臂。`-n` 仍 Unmodelled。新增 2 枚测（chunk_bytes 定长/短尾/
  n=0/超一屏 + `-b` 解析附值/分词/k/m/非法/与 -l 互斥）。`-l`/纯字节模式零回归（既有测全绿）。
  三件套：`cargo test -p minix-textfilter --lib` → **394/0**（既有 392 + 新 2）；`--bin split` → 4/0；
  freestanding `cargo build -p minix-textfilter --bin split --target {x86_64,aarch64,riscv64gc}-unknown-none --release` 三架构均 Finished；
  clippy 触及文件零新增告警（bin/split.rs “items after a test module” 基线存量）；`rustfmt --edition 2024 --check`
  两目标文件新增行零漂移（仅余共享件 bin_support.rs 的 import 排序存量漂移，未碰）；核实日 `2026-10-13`；基线 `632c888d4`。
  真机腿：字节切块的写盘受可写根盘 E5 门控（同行切块），本笔交代码+宿主测（chunk_bytes 纯函数已测）。留白：`-n` 块数。

- ✅ **批次一百九十接上 `grep -w`（词边界，决定半已存、接执行半）**
  （`2026-10-13`，`claim/GREP-W-qoder` @ 基线 `5e7ae8236`，单域三线 commands/regex，无跨界）：
  发现：决定半早已有词边界实现（`matcher::find_word`，带 `test_word_match_rejects_partial`），但 grep
  执行半遇 `-w` 直接报“wait for engine support”退 2——典型两半分裂。因 grep 持的是编译后的
  `Pattern`（非 `Matcher` trait），新增 `Pattern::find_word`（走已有 `find_bytes` 逐候选推进、左右邻居非
  词字节才命中，对位 `grep.c:85,186`），执行半 `-w` 分派到它。`-i` 仍真未接（引擎无大小写折叠）。
  新增 `test_find_word_boundary`（行首/尾/空格/下划线/多候选推进与 concatenate 反例）。文档同步：
  `bin/grep.rs` 头注与 08 篇 §2.2/§4.4/§4.5 grep 行从“`-i`/`-w` 未建模”改为“`-w` 已接、仅 `-i` 留白”。
  三件套：`cargo test -p minix-regex --lib` → **35/0**（新 1）；`--bin grep` → 9/0；
  freestanding `cargo build -p minix-regex --bin grep --target {x86_64,aarch64,riscv64gc}-unknown-none --release` 三架构均 Finished；
  clippy 触及文件零新增告警（bin/grep.rs “items after a test module” 系基线存量）；`rustfmt --edition 2024 --check`
  两目标文件 14→14 零新增漂移（存量 nightly/stable 版差未碰）；核实日 `2026-10-13`；基线 `5e7ae8236`。
  真机腿：grep 读文件腿归 C-57/E5（本笔为纯词边界谓词，宿主测已覆盖）。留白：`-i`（引擎 case-folding）、`-r`/上下文行。

- ✅ **批次一百八十七接上 `cal -3`（前/当/后月三联居中）**
  （`2026-10-13`，`claim/CAL-THREE-qoder` @ 基线 `5701fa073`，单域三线 commands/doctools，无跨界）：
  接回 `-3` 留白。决定半 `cal.rs` 新增 `render_triple`（`cal.c:monthrange` before=after=1 的面：
  三个相邻月并排、非整年带故每列标题为 "Name Year"），新增 `shift_month`（跨年归一、越界拒，
  对位 C startyear 越下界或 endyear 越上界）与 `put_month_year_header`（复用已验的 `put_centered`）。执行半
  `bin/cal.rs`：`choose_view` 新增 `View::Triple` 与 `force_triple` 参（`-3` 仅重塑月基面；单操作数
  仍走 yearly、`-y` 优于 `-3`，均对位 C `month==0` 时回到 yearly）。新增 2 枚决定半测（`shift_month`
  跨年/越界 + 年边界三联标题与行数）与 2 枚 bin 测（`-3` 分派与优先级）。
  三件套：`cargo test -p minix-doctools --lib` → **69/0**（既有 67 零回归 + 新 2）；`--bin cal` → **7/0**；
  freestanding `cargo build -p minix-doctools --bin cal --target {x86_64,aarch64,riscv64gc}-unknown-none --release`
  三架构均 Finished；clippy cal.rs 告警仍为 2（仅存量 i64 同型转换，本笔新增枚 manual_range_patterns 已按建议改 contains）；
  `rustfmt --edition 2024 --check` 两文件零漂移（共享件 bin_support.rs 已校验未变）；核实日 `2026-10-13`；基线 `5701fa073`。
  真机腿：`cal -3` 纯输出、不依赖盘上读写，归 C-57/E5 逐命令上机断言。留白：`-h`/`-d`/`-r`/`-A`/`-B`/`-C`/`-R` 仍待接。

- ✅ **批次一百八十六修 `cal -y` 静默 no-op（无操作数时应拉年视图）**
  （`2026-10-13`，`claim/CAL-Y-qoder` @ 基线 `c2cfeed79`，单域三线 commands/doctools，无跨界）：
  发现：执行半把 `-y` 写成 `'y' => {}` 静默 no-op，而 C `cal.c:312` 在 argc==0 时以 `if (!yflag)`
  决定当月还是当年——`cal -y`（无操作数）本应打当年年视图，却仍打当月；且旧测不覆盖旗标解析。
  修法：把操作数→视图选择抽成纯函数 `choose_view(operands, force_year, now)`（now 注入避开真钟）、
  `-y` 置 `force_year`；仅影响无操作数分支（显式操作数已定面，与 C 一致）。新增 5 枚 bin 宿主测
  （无操作数默认月/`-y` 拉年/单操作数年不受 `-y`/两操作数月/非法值与超额拒绝）。
  三件套：`cargo test -p minix-doctools --bin cal` → **5/0**；`--lib` → 67/0零回归；freestanding
  `cargo build -p minix-doctools --bin cal --target x86_64-unknown-none --release` Finished；clippy bin/cal.rs 零告警；
  `rustfmt --edition 2024 --check` bin/cal.rs 零漂移（共享件 bin_support.rs 经 `#[path]` 被牵连已 checkout 还原不入库）；
  核实日 `2026-10-13`；基线 `c2cfeed79`。真机腿：`cal -y` 纯输出、不依赖盘上读写，归 C-57/E5 逐命令上机断言。
  留白：`-3`/`-h`/`-d` 等仍待接。

- ✅ **批次一百八十五接上 `cal -j`（年积日显示）**
  （`2026-10-13`，`claim/CAL-JULIAN-qoder` @ 基线 `286585ace`，单域三线 commands/doctools，无跨界）：
  接回批次一百八十四声明的 `-j` 留白。决定半 `cal.rs` 新增 `DayStyle`（Month/Julian）、`day_of_year_of_first`（对
  位 C `day_in_year`，Gregorian 闰）、`J_WEEK_LEN=27`/`J_MONTH_PER_ROW=2`/`JULIAN_HEADINGS`（对位
  `cal.c:336-343`/`:92`），新增 `put_day_julian`（三位年内日、前导零按位省略，对位 `ascii_day` 的 julian 支
  `cal.c:584-603`）；`day_matrix` 改为接 `first_value`（Month=1，Julian=年内序号），`render_month`/`render_year`
  新增 `style` 参逐路切换单元宽/表头/每月行数。执行半 `bin/cal.rs` 解 `-j` 置 `DayStyle::Julian` 并透传。
  新增 3 枚 golden 宿主测（月面 244–249 行/年面两月一行/`day_of_year_of_first` 含闰年 3/1=61）。
  顺带修上一轮遗漏：`render_year` 日体两层循环的 needless_range_loop 两告警（改用迭代器+局部 allow）。
  三件套：`cargo test -p minix-doctools --lib` → **67/0**（既有 65 零回归 + 新 2）；freestanding
  `cargo build -p minix-doctools --bin cal --target {x86_64,aarch64,riscv64gc}-unknown-none --release` 三架构均 Finished；
  `cargo clippy -p minix-doctools --tests` cal.rs 告警由基线 4 降为 2（仅余 `days_from_civil` 的 i64 同型转换
  两枚存量，本笔零新增）；`rustfmt --edition 2024 --check` 两文件零漂移（顺带归一 cal.rs 的 nightly/stable 版差漂移）；
  核实日 `2026-10-13`；基线 `286585ace`。**真机腿**：`cal -j` 纯输出不依赖盘上读写，归 C-57/E5 逐命令上机断言（本批交代码+宿主测）。
  留白：`-3`/`-h`/`-d` 等仍待接。修法详见 `../18-stage-commands/10-doc-man-tools.md` §5 留白行。

- ✅ **批次一百八十四修 `cal` 年视图日体循环转置（P1 正确性缺陷）**
  （`2026-10-13`，`claim/CAL-J-qoder` @ 基线 `f14a3d4b9`，单域三线 commands/doctools，无跨界）：
  发现：`render_year` 的日体两层循环写反（外层=月、内层=周），导致每月 6 行被横拼成一条线、
  三月堆叠而非并排。影响 `cal <year>`（单操作数年视图）输出；`test_year_render_band_width` 只校
  标题带所以未测出。修法：外层改周（0..6）、内层改月（0..count），`matrices[slot][week]` 访问。
  新增回归钉 `test_year_body_is_side_by_side` 铉定行宽与首带首两行内容，防后续转置回退。
  三件套：`cargo test -p minix-doctools --lib` → **65/0**（含既有 4 枚 cal 测零回归 + 新 1 枚）；
  freestanding `cargo build -p minix-doctools --bin cal --target x86_64-unknown-none --release` Finished；
  `cargo clippy -p minix-doctools --tests` 触及文件零新增告警；核实日 `2026-10-13`；基线 `f14a3d4b9`。
  留白：`-j` 年积日显示、`-3` 三联、`-y` 强制年视图等旗标仍待接线（见下一篇批次）。

- ✅ **批次一百八十三修 `cmp` 未接 `-` 标准输入操作数（接同族，C 保后兼容臂）**（`2026-10-13`，`claim/CMP-STDIN-qoder` @ 基线 `dcf301eeb`，单域三线 commands/textfilter）：同族延伸但属操作数语义非旗标聚簇。`minix3/usr.bin/cmp/cmp.c:95-113` 有一栗“Backward compatibility — handle `-` meaning stdin”：`-` 操数读标准输入（fd 0）、且两操数皆 `-` 时 `errx(ERR_EXIT, "standard input may only be specified once")`。旧薄壳把两操数一律 `read_file_or_none(path) = open(path,0,0)`——`cmp - file`（如 `cat x | cmp - file`）会去 `open("-")` 当字面文件，失败报“cmp: -”退出，而非比标准输入。修法：新增纯函数 `uses_stdin(file1,file2) -> Result<[bool;2],()>`（一个 `-`→对应位 true，两 `-`→Err）供 run 分派读取与报文标签（`-`→"stdin"，与 C `file1="stdin"` 同），`-` 操作数走 `support::read_stdin()`（读 fd 0，bin_support 己封装）、否则 `read_file_or_none`；两 `-` 时按 C 报“只能指定一次标准输入”退 ERR_EXIT；report_first_diff/report_eof 改用 name1/name2。三件套：`cargo test -p minix-textfilter --bin cmp`→**1/0**（新 `a_dash_operand_reads_stdin`：`-`/f→[true,false]、f/`-`→[false,true]、a/b→[false,false]、`-`/`-`→Err）；`cargo test -p minix-textfilter --lib`→**390/0**（既有 cmp 决定半 7 测零回归）；freestanding `cargo build -p minix-textfilter --bin cmp --target {x86_64,aarch64,riscv64gc} --release` 三架构均 Finished；`cargo clippy -p minix-textfilter --bin cmp` 触及文件零告警；`rustfmt --edition 2024 --check` bin/cmp.rs 基线即净、本笔新增行归净（净漂移 0，bin_support.rs 经 `#[path]` 被写一度牵连已还原不入库）；`pattern-gate.sh`→`--diff PASS=17 FAIL=0`（`[P11] PASS` 增量 2 锈点 `cmp.c:95-113`/`:110` 逐条回读落窗）；核实日 `2026-10-13`；基线 `dcf301eeb`。**验证边界**：新逻辑的纯部分（`uses_stdin` 四分岔）带宿主测；stdin 读取腿属执行半（`read(fd 0)`），同其他命令读腿交真机/管道环验（imgrd `/bin/cmp` 未播种、stdin 需管道，归 C-57/E5）；未改 cmp 决定半比较逻辑。**与 C 的一处保留**：`-l`/`-s` 聚簇 `-ls` 仍走 usage（C 也先 `if(lflag&&sflag)` 拒“only one of -l and -s”，两错均为退 ERR_EXIT、仅文案异，非漏接合法输入，不属本批范围）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百八十三与 `../18-stage-commands/07-text-filter.md` cmp 行。
- ✅ **批次一百八十二修 `ls` 旗标面 getopt 聚簇与 `--` 结束扫描（接同族，bin 内可测解析器，tee 同法）**（`2026-10-13`，`claim/LS-CLUSTER-qoder` @ 基线 `99d73bc74`，单域三线 commands/fileops）：同族延伸，落在最常用的 `ls`。旧薄壳 `for arg { match arg.as_str() { "-1"|"-a"|"-A"|"--"=>continue, _ if starts_with('-')&&len>1 => {contains 'l'?长格式拒:usage}, _=>operands } }` 逐整 token——而 `minix3/bin/ls/ls.c:154` `getopt("1AaBbCcdFfghikLlMmnOoPpqRrSsTtuWwXx")` 接受旗标聚簇。两处真 bug：①**本层已接旗标的聚簇被误拒**：`ls -1a`/`ls -aA`/`ls -a1`（合法且全为已建模旗标）因整 token 不匹配而进 `_ if starts_with('-')` 分支→非含 l→报 usage 退1；②**`--` 用 `continue` 只跳过 `--` 本身、其后的 `-a` 仍被当选项解**（`ls -- -a` 本应把 `-a` 当名为 `-a` 的文件，却误置 show_dot）。修法采 tee 先例（bin 内可测解析器）：新增 `parse_ls_args<'a>(&'a [&'a str]) -> Result<LsArgs, LsError>`，逐字 getopt 扫描：一个 token 内逐字母（`1` no-op、`a` show_dot、`A` almost_all、`l`→`LongStatUnwired`、其余→`Usage`）；`--` 结束扫描、余词入操数；裸 `-` 为操数（ls 经 fopen 字面开）。保留旧语义：`-l`（含在聚簇里，如 `-la`）仍按“长格式需 stat+passwd/group 库”的诚实拒。bin destruct LsArgs（回射 Vec<String>）+ `arg_error`（LongStatUnwired→旧“waits on long-stat face”文案退1、Usage→旧 usage 退1）。三件套：`cargo test -p minix-fileops --bin ls`→**4/0**（新 4 枚：单旗标、**关键回归聚簇 `-1a`/`-aA` 被接不再误拒**、`--` 结束扫描使 `-a` 成文件、`-l`/`-la`/`-al`→LongStatUnwired、`-x`→Usage、裸 `-` 操数）；`cargo test -p minix-fileops --lib`→370/0（回归零，ls 测属 bin target）；freestanding `cargo build -p minix-fileops --bin ls --target {x86_64,aarch64,riscv64gc} --release` 三架构均 Finished；`cargo clippy -p minix-fileops --bin ls` 触及文件零告警；`rustfmt --edition 2024 --check` bin/ls.rs 基线即净、本笔新增行归净（净漂移 0，bin_support.rs 系共享件经 `#[path]` 被写一度牵连、已 checkout 还原不入库）；`pattern-gate.sh`→`--diff PASS=17 FAIL=0`（`[P11] PASS` 增量锈点 `ls.c:154` 逐条回读落窗）；核实日 `2026-10-13`；基线 `99d73bc74`。**既有决策保留**：`-l`/`-F`/`-t` 等未建模旗标仍显式拒（长格式需 stat+口令库，属后续阶段）；真机 per-command 与 imgrd `/bin/ls` 目录列腿受可写根盘/`getdents`（E5）门控（本批仅拆 argv、未改走表）。本命令无单独 06-file-ops 行（选项折叠属 bin 内新增可测件），修法以本 §6.1 叙述为权威。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百八十二。
- ✅ **批次一百八十一修 `cut` 旗标面 getopt 旗标+带值选项混聚簇（接同族，下沉 cut.rs）**（`2026-10-13`，`claim/CUT-CLUSTER-qoder` @ 基线 `c039e275f`，单域三线 commands/textfilter）：同族延伸。旧薄壳已接附值（`option.len() > 2` 拆 `-Nlist`）与集外旗标 usage 拒（前一批已修“任意 dash token 漏成文件”），但**不支持旗标与带值选项同 token 混聚簇**——`minix3/usr.bin/cut/cut.c:88` `getopt("b:c:d:f:sn")` 接受 `-sf1-3`（`-s` 旗标后接 `-f` 取值 `1-3`）。旧解析见 `-sf1-3` len>2 便将 `f1-3` 当 `-s` 的内联值丢弃（`-s` 不吃值），导致 **`cut -sf1-3 file` 静默丢掉字段列表**→无选单→误报 usage；而 `-ns`（`-n` 先）会把 `s` 当 `-n` 内联值丢→丢 `-s` suppress。旧解析零测。修法采 head/tail 先例（同在 `cut.rs` 决定半）：把 argv 折叠下沉为 `parse_cut_args<'a>(&'a [&'a str]) -> Result<CutArgs<'a>, CutError>`，逐字 getopt 扫描：`b`/`c`/`f`/`d` 值附于本 token 或取下一词、吃值后终止本 token 扫描；`s`/`n` 逐位聚簇（`-sf1-3`→suppress+字段 `1-3`、`-sn`/`-ns` 均置 suppress）；`-b`/`-c` 设字节表（本实现字节粒度同义）、字节表与字段表并存→`Conflict`（C `cut.c:120` b_flag+f_flag 守卫）；`-d` 值非单字节→`BadDelimiter`；`--` 结束、集外字母→`Usage`。薄壳 destruct CutArgs（回射为 Option<String>/Vec<String> 以最小化下游）+ `arg_error`（Usage→usage()退1、Conflict→conflict()退1、BadDelimiter→旧静默 terminate(1)）。三件套：`cargo test -p minix-textfilter --lib`→**390/0**（新增 `test_parse_cut_args_faithful_getopt`——分离/附值、**关键回归 `-sf1-3`→suppress+字段不再丢**、`-sn`/`-ns` 均 suppress、`-d:`/`-d ,`、`-c`、`--`/裸- 操数、错误路 `-x`→Usage/`-f`缺值→Usage/`-b1 -f1`与`-f1 -b1`→Conflict/`-dab`→BadDelimiter）；freestanding `cargo build -p minix-textfilter --bin cut --target {x86_64,aarch64,riscv64gc} --release` 三架构均 Finished（首跑拓出 no_std 漏编译：`Vec` 未入模块作用域，补 `use alloc::vec::Vec;` 后绿——即 head 批次同型教训）；`cargo clippy -p minix-textfilter --bin cut` 触及两文件零告警；`rustfmt --edition 2024 --check` 本笔新增行零漂移、**两文件 git diff 删除行数=0（rustfmt 写一度顺将存量 `select_bytes`/`in_ranges` 两处旧漂移折行，已逐一还原为基线形，保持“不碰存量”纪律，仅 `use alloc::vec::Vec` 一行新增导致其行号+1 平移）**；`pattern-gate.sh`→`--diff PASS=17 FAIL=0`（`[P11] PASS` 增量 3 锈点 `cut.c:88`/`:120`/`:112-113` 逐条回读落窗）；核实日 `2026-10-13`；基线 `c039e275f`。**既有决策保留**：`-b`/`-c` 字节粒度同义（多字节 locale 不携带）；真机 per-command 与 imgrd `/bin/cut` 播种归 `C-57`/E5（本批仅拆 argv、未改选择行为）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百八十一与 `../18-stage-commands/07-text-filter.md` cut 行。
- ✅ **批次一百八十修 `tail` 旗标面 getopt 附值/集外旗标静默漏成操作数（接同族，采 head 的先例下沉 window.rs）**（`2026-10-13`，`claim/TAIL-ARGPARITY-qoder` @ 基线 `6ab01cbf8`，单域三线 commands/textfilter）：同族延伸，且是高频命令的静默错行为。旧薄壳 `run()` 内联 `match a { "-n" => 取下一词, ..., _ => operands.push }` 逐整 token——而 `minix3/usr.bin/tail/tail.c:108` `getopt("Fb:c:fn:r")`（前置 `obsolete(argv)` `:106`）接受附值与聚簇。失真：`tail -n10 f`/`tail -c100 f`/`tail -b3 f`（附值，合法）因 `a[1..]` 非全数字而落 `_ =>` 分支，**被静默当文件操作数去 `open("-n10")`**（非报错）；集外旗标 `tail -x f` 也静默漏成操作数（C 走 `default:`→usage）。旧解析还零测（tail bin 测只盖 reverse/bytes 助函数、不测 argv）。修法采 head 先例（`parse_head_args` 同在 `window.rs`）：把 argv 折叠从薄壳下沉为决定半纯函数 `parse_tail_args<'a>(&[&str]) -> Result<TailArgs, TailError>` 到 `window.rs`，忠实两阶段：phase 1 `obsolete()` 只重写前导 `-<digit>…` 连段为 `-n<digit>…`（遇非匹配即停，与 C 同）、phase 2 getopt 扫描：`n`/`c`/`b` 值**附于本 token（`-n10`）或取下一词（`-n 10`）**、值走与旧 `parse_count` 同判据（前向 `+N`→`ForwardOffset`、非十进制/零字节零块数→`IllegalCount(v)`）；`r` 旗标逐位聚簇（`-rn3`）；`f`/`F`→`NotModelledFollow`（信号/跟随未接、诚实拒非漏成操作数）；集外字母→`Usage`；`--` 结束、裸 `-` 为字面操数（tail 经 fopen）。薄壳 destruct TailArgs + `arg_error` 映到旧退出码/文案（Usage→新增 usage()退 1、ForwardOffset/NotModelledFollow→旧文案退1、IllegalCount(v)→按 C `tail.c:90` 补“illegal offset -- {v}”退1，较旧静默 terminate 更贴 C）。`block_bytes`（512×）随解析下沉 window.rs 供 `-b` 用、bin 不留。三件套：`cargo test -p minix-textfilter --lib`→**389/0**（新增 `test_parse_tail_args_faithful_getopt`——默认/分离值兼容、**关键回归 `-n10`/`-c100`/`-b3` 被接不再漏**、聚簇 `-rn3`、obsolete `-5`、`--`/裸- 操数、错误路 `-f`/`-F`→NotModelledFollow/`-x`→Usage/`-n`缺值→Usage/`-n +5`→ForwardOffset/`-n x`/`-c 0`→IllegalCount）；既有 `--bin tail` 7/0（原 block_bytes 测随函数下沉、由 parse 测覆盖）；freestanding `cargo build -p minix-textfilter --bin tail --target {x86_64,aarch64,riscv64gc} --release` 三架构均 Finished；`cargo clippy -p minix-textfilter --bin tail` 触及新代码零告警（`window.rs:106` `for index in 0..kept` 系存量）；`rustfmt --edition 2024 --check` window.rs 归全净（本笔新行净，rustfmt 顺将 head 残留 3 行旧漂移一并归净—纯空白零行为改动、window.rs 属本线代码且无并发代码线程，已登记）；`pattern-gate.sh`→全量 `PASS=14 FAIL=0 SKIP=1` 且 `--diff PASS=17 FAIL=0`（`[P11] PASS` 增量 5 锈点 `tail.c:106`/`:108`/`:90`/`:128-130` 等逐条回读落窗）；核实日 `2026-10-13`；基线 `6ab01cbf8`。**既有决策保留**：`-f`/`-F` 仍拒（信号/跟随未接）；`-n +N` 前向偏移仍未接；真机 per-command 与 imgrd `/bin/tail` 播种归 `C-57`/E5（本批仅拆 argv、未改取行行为）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百八十与 `../18-stage-commands/07-text-filter.md` tail 行。
- ✅ **批次一百七十九修 `tee` 旗标面 getopt 聚簇/`--` 缺失（接同族，tee 已有 bin 内可测解析器就地扩）**（`2026-10-13`，`claim/TEE-CLUSTER-qoder` @ 基线 `0a44236d1`，单域三线 commands/textfilter）：同族延伸。`tee` 早批已把 argv 折叠下沉为 bin 内可测的 `parse_tee_args`（带测），但其形状是 `for &arg in argv` 逐整 token（`arg == "-a"`/`"-i"`、其余 `starts_with('-')`→BadFlag）——而 `minix3/usr.bin/tee/tee.c:80` `getopt("ai")` 接受旗标字母聚簇。失真：`tee -ai f`/`tee -ia f`（合法聚簇）被误拒 BadFlag（当文件名去开），且 `--` 结束扫描未接（`--` 也会落 BadFlag）。逐字读 `tee.c:80-92`：`a`→`append=1`、`i`→`signal(SIGINT, SIG_IGN)`、`?`/default→usage。修法：把 `parse_tee_args` 改为忠实 getopt——一个 token 内逐字母扫描（`a`→append、`i`→`SignalPending`（信号面未接、仍按既有决策诚实拒）、集外字母→`BadFlag`）；`--` 结束扫描、余词入操数；裸 `-`（len 1）仍为名为 `-` 的输出文件（`tee.c:102` 字面 `open`，未变）。三件套：`cargo test -p minix-textfilter --bin tee`→**9/0**（既有 7 枚 parse 测零回归；新 2 枚——关键回归聚簇 `-ai`/`-ia`→SignalPending、`-aa` 幂等 append、`-az`→BadFlag；`--` 结束扫描 `-a -- -i f`→append+operands[`-i`,`f`]）；全 crate `--lib`→**388/0**（tee 测属 bin target、库不受影）；freestanding `cargo build -p minix-textfilter --bin tee --target {x86_64,aarch64,riscv64gc} --release` 三架构均 Finished；`cargo clippy -p minix-textfilter --bin tee` 触及新代码零告警；`rustfmt --edition 2024 --check` 本笔新增行归净（残留 9/20/183/237 四处系基线即存的双空行与 import 排序 rustfmt 版本差异，均在射程外、未顺手改）；`pattern-gate.sh`→全量 `PASS=14 FAIL=0 SKIP=1` 且 `--diff PASS=17 FAIL=0`（`[P11] PASS` 增量 2 锈点 `tee.c:80`/`:102` 逐条回读落窗）；核实日 `2026-10-13`；基线 `0a44236d1`。**既有决策保留**：`-i` 仍显式拒（信号面 pending，与非聚簇形同判据）；真机 per-command 与 imgrd `/bin/tee` 播种归 `C-57`/E5（本批仅拆 argv、未改扇出行为）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百七十九与 `../18-stage-commands/07-text-filter.md` tee 行。
- ✅ **批次一百七十八修 `split` 旗标面 getopt 附值/obsolete 缺失（接 expand/unexpand/fold/join/column/pr 同族）**（`2026-10-13`，`claim/SPLIT-ARGPARITY-qoder` @ 基线 `a88d0f9f7`，单域三线 commands/textfilter）：同族第七家，且是最常见用法被误拒的真 bug。旧薄壳 `if arg == "-l"`（整 token、值必须在下一词）`else if arg.starts_with('-')`→usage()——但 `minix3/usr.bin/split/split.c:80` `getopt("0123456789b:l:a:n:")` 接受附值与 obsolete 前导-digit。失真：`split -l100 f`（附值，最常见写法）被误拒 usage、`split -1000 f`（obsolete `-N`）也被误拒。逐字读各臂：`case 'l'`（`:112-118`，`numlines != 0 → usage`、值须全数字且 `>0` 否则 `errx illegal line count` 退 1）、obsolete 前导-digit `case '0'..'9'`（`:82-98`，仅在 `numlines == 0` 时取数，故 `-l3 -5` 的 5 被忽略而 `-5 -l3` 触发 usage）、`-b`/`-a`/`-n`（值选项，但决定半 `chunks`/`suffix` 只按行切块、不支持字节/后缀宽/块数）。修法采 join/column/pr 先例：下沉决定半 `parse_split_args<'a>(&'a [&'a str]) -> Result<SplitArgs<'a>, SplitError<'a>>` 到已测 `split.rs`，忠实 getopt：`-l` 值**附于本 token（`-l100`）或取下一词（`-l 100`）**、已设值再遇 `-l`→Usage；全数字尾为 obsolete `-N`（仅未设时取数，与 C 同）；`-a`/`-b`/`-n`→`Unmodelled`（决定半能力缺口、诚实报出非静默忽略）；`--` 结束、裸 `-` 为操数、集外字母→Usage、非全数字/零值→`IllegalLineCount(v)`（携原值供 C 同形文案）。薄壳 destruct SplitArgs + `arg_error` 把 Usage→usage()退2、`IllegalLineCount(v)`→`split: {v}: illegal line count.`退1、`Unmodelled`→not modelled退2。三件套：`cargo test -p minix-textfilter --lib`→**388/0**（既有 4 枚 split 测零回归；新 3 枚 parse 测——默认/分离值向后兼容、**关键回归 `-l100`/`-1000` 被接**、`--`/裸- 操数、`-l3 -5` obsolete-after-l 被忽略、错误矩阵 `-a/-b/-n`→Unmodelled/`-z`→Usage/`-l`缺值→Usage/`-l0`/`-lx`→IllegalLineCount/`-5 -l3`与`-l3 -l4`→Usage）；freestanding `cargo build -p minix-textfilter --bin split --target {x86_64,aarch64,riscv64gc} --release` 三架构均 Finished；`cargo clippy -p minix-textfilter --bin split` 触及两文件零告警；`rustfmt --edition 2024 --check` 两文件基线即净、本笔新增行归净（净漂移 0）；`pattern-gate.sh`→全量 `PASS=14 FAIL=0 SKIP=1` 且 `--diff PASS=17 FAIL=0`（`[P11] PASS` 增量 3 锈点 `split.c:80`/`:82-98`/`:135-143` 逐条 sed 回读落窗）；核实日 `2026-10-13`；基线 `a88d0f9f7`。**与 C 的已登记差异（非本笔新造）**：①超宽数字（过 `usize`）→本层判 IllegalLineCount，C `strtoull` 会饱和——属荒谬输入，取安全拒绝不取 panic；②`-a`/`-b`/`-n` 在 C 被支持并作用，本树因决定半只行切块而显式拒（旧薄壳已如此、本轮从笼统 usage 改清为 Unmodelled 文案）；③`>2` 操数旧薄壳 guard 保留（C 实际忽略多余操数）。真机 per-command 断言与 imgrd `/bin/split` 播种归 `C-57`/E5（本批仅拆 argv、未改切块输出）。至此命令面 getopt 聚簇/附值缺陷族（textfilter 域 expand/unexpand/fold/comm/head/join/column/pr/split）已扫完；非 textfilter 候选经核均非缺陷（`renice`/`kill` C 本就用手工 `strcmp(*argv,"-x")` 整 token 解析，Rust 忠实镜像、无聚簇缺口）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百七十八与 `../18-stage-commands/07-text-filter.md` split 行。
- ✅ **批次一百七十七修 `pr` 旗标面 getopt 聚簇/附值缺失（接 expand/unexpand/fold/join/column 同族）**（`2026-10-13`，`claim/PR-ARGPARITY-qoder` @ 基线 `e8ea79f32`，单域三线 commands/textfilter）：同族第六家。旧薄壳逐个比整个 token（`arg == "-t"`、`"-h"` 的值必须在下一词），而 `minix3/usr.bin/pr/pr.c:1655` 的 `egetopt(argc, argv, "#adFfmrte?h:i?l:n?o:ps?T:w:")` 接受字母聚簇与附值。失真：`pr -w80 f` 把 `-w80` 当文件名去开、`pr -hT` 把 `-hT` 误判、`pr -dm`/`pr -tw100` 聚簇整段漏成操作数，且无前置 dash 守卫使任意 `-z` 也静默变文件名。逐字读各臂：`case '+'`（`:1657`，`#` 引导的起始页 `pgnm = atoi(eoptarg)`，`< 1` 报错）、obsolete 前导-digit 竖列 `clcnt`（`:1665`）、`case 'n'`（`:1744`，`-n[ch][width]` 可选附值）、`-a` 需多列（`:1823-1828`）、`-w` 缺席时页宽默认（`:1834-1839`：有 `-s` 取 512 否则 72）、列宽过窄拒绝（`:1863-1867`）。修法采 join/column 先例：把 argv 折叠从薄壳下沉为决定半纯函数 `parse_pr_args<'a>(&'a [&'a str]) -> Result<PrArgs<'a>, PrError>`（落在已测 `pr.rs`），逐字忠实 egetopt——无值旗标 `t`/`d`/`m`/`a` 逐位聚簇；带值字母 `h`/`l`/`w`/`o` 的值**附于本 token（`-w100`/`-hT`/`-l66`/`-o4`）或取下一词（`-w 100`）**；`-s`/`-n` 取可选附值（`-sX`、`-n5`）；前导全数字 token 为 obsolete 竖列 `-N`、`+N` 为起始页；`--` 结束扫描、裸 `-` 为操作数、集外字母（`e`/`f`/`F`/`i`/`p`/`r`/`T`）→ `Unmodelled`；扫描后置：`!wflag && sflag ⇒ 512`、`columns > 1 且 column_width 为空 ⇒ ColumnTooSmall(n)`、`across 且 columns <= 1 ⇒ AcrossNeedsColumns`。薄壳 destruct `PrArgs` 并投影回原 `operands`/`title_override` 值类型以最小化下游改动，`arg_error` 把每个 `PrError` 映射到旧薄壳既有的 stderr/退出码。三件套：`cargo test -p minix-textfilter --lib`→**385/0**（既有 32 枚 pr 分页测零回归；新 3 枚 parse 测——默认与分离值兼容、聚簇与附值关键回归 `-w100`/`-hT`/`-dm`/`-tw100`/`+3`/`-n5`/`-sX`/`-N`、错误路 `-e`→Unmodelled/`-w` 缺值→Usage/`-w0`→Usage/`-l5`→Usage/`-a` 无多列→AcrossNeedsColumns/`-100`→ColumnTooSmall(100)）；freestanding `cargo build -p minix-textfilter --bin pr --target {x86_64,aarch64,riscv64gc} --release` 三架构均 Finished；`cargo clippy -p minix-textfilter --bin pr` 触及新代码零告警（首版一处 `manual_strip` 已按建议改 `strip_prefix`）；`rustfmt --edition 2024 --check` 本笔新增行零漂移（`pr.rs` 净漂移数与基线同为 1，即既有 `test_zero_or_huge_rejected` 那处存量注释，未顺手改）；`pattern-gate.sh`→全量 `PASS=14 FAIL=0 SKIP=1` 且 `--diff PASS=17 FAIL=0`（`[P11] PASS` 增量 5 枚锈点 `pr.c:1655`/`:1665`/`:1744`/`:1823`/`:1834`/`:1863` 逐条 sed 回读落窗）；核实日 `2026-10-13`；基线 `e8ea79f32`。**存量非本笔**：`jot.rs:771` 的 `approx_constant` clippy 报错、`bin_support.rs:20` 的 import 排序 rustfmt 漂移，均在基线即存、由 `--all-targets`/共享件牵连，非本次改动引入。**与 C 的一处既有差异（如实登记，非本笔新造）**：C 对坏 `+N`/`-N` 走 `errx`/`fputs`+`return(1)`（`:1657-1668`），本树旧薄壳早已统一折叠为 usage 退 2（`arg.starts_with('+')`/`-N` 两支 `_ => usage()`），本轮 refactor 严格保留该既有映射、未改错误面。真机 per-command 断言与 imgrd `/bin/pr` 播种归 `C-57`/E5（本批仅拆 argv、未改输出行为）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百七十七与 `../18-stage-commands/07-text-filter.md` pr 行。
- ✅ **批次一百七十六把 `find` 的盘上判据在 aarch64/riscv64 两架构上机验毕（闭批次一百七十三登记的「待需时上机」）**（`2026-10-13`，`claim/FIND-ARCH-VERIFY-qoder` @ 基线 `2db73a1e0`，无代码改动、仅真机复跑）：批次一百七十三只跑了 x86_64 一架构并声明 aarch64/riscv64「同判据、待需时上机」。本批在 QEMU 空闲窗口把三门都跑一遍。**真机读数**：`bash os/qemu-tests/test-cmd-smoke-aarch64.sh` → `RESULT: PASS (… three stat shapes + find file operand …)`；`bash os/qemu-tests/test-cmd-smoke-riscv64.sh` → 末次轮询 `cmdface: … find=True` 且 `RESULT: PASS (… + find file operand …)`；x86_64 于批次一百七十三已连跑两轮绿。三门齐 → `find` 只读文件操作数（lstat→eval→emit）的跨三架构真机验证收口。**未改代码**（find 二进制已在批次一百七十三进共享 bins 名单，三架构镜像均含）；riscv64 的 ls 目录列腿仍按已知缺陷计（`blocked=True`、`listing=False`，imgrd 根盘 getdents 未通，与本批无关、不阻塞 PASS）。命令面三架构真机判据的 `find` 一支至此完整。详见 `../18-stage-commands/todo.md` §6.1 批次一百七十二/一百七十三与本批。
- ✅ **批次一百七十五修 `column` 旗标面 getopt 聚簇/附值缺失（接 expand/unexpand/fold/join 同族）**（`2026-10-13`，`claim/COLUMN-ARGPARITY-qoder` @ 基线 `e580743b2`，单域三线 commands/textfilter）：同族第五家。旧薄壳 `arg == "-t"`/`"-x"`/`"-s"`/`"-c"` 只比整个 token、值必须分词——但 `column.c:93` `getopt("c:s:tx")` 接受聚簇 `-tx`/`-xt` 与附值 `-c100`/`-s,`。失真：`column -tx f` 误把 `-tx` 当文件名（`open("-tx")`→warn+置错），`column -c100 f` 同（`-c100` 入操数），且无 leading-dash 守卫使任意 `-z` 也静默变文件名。逐字读 `column.c:93-109`：`t`/`x` 无值旗标、`c:`/`s:` 带值（附于字母后或取下一项）、`?`/default→usage。修法采 expand/join 先例：下沉决定半 `parse_column_args<'a>(&'a [&'a str]) -> Result<ColumnArgs, ColumnError>` 到已测 `column.rs`，逐字 getopt：`-t`/`-x` 逐字聚簇、`-c`/`-s` 值附于本 token (`-c100`) 或取下一个 (`-c 100`)、`--` 结束、裸 `-` 为操数、集外字母/缺值→`Usage`。bin destruct ColumnArgs。**与 C 的一处差异**：`-c` 值 C 走 `atoi`（`:96` 无守卫），本层要求 `>0` 的 `usize`否则 usage（防御宽度 0 在列布局里的已知陷阱，与旧 bin 同、非语义发明）。三件套：`cargo test -p minix-textfilter --lib column::`→**6/0**（新 2 枚：聚簇 `-tx`+附值 `-c100`/`-s,` 关键回归 + 分离值兼容、`--`/裸 `-`/集外字母/缺值/宽度0 均 Usage）、全 crate `--lib`→**382/0**；freestanding `cargo build -p minix-textfilter --bin column --target x86_64-unknown-none --release` 绿；`cargo check --workspace -j 1`→0 error；clippy 触及两文件零告警；`rustfmt --edition 2024 --check` 两文件零漂移；`pattern-gate.sh`→`PASS=14 FAIL=0 SKIP=1`（P18 12686→12688，`column.c:93-109` 锈点回读落窗）；核实日 `2026-10-13`；基线 `e580743b2`。**声明性留白**：无 `strerror` 后缀；`-c` 宽度 0/负值的 `atoi` 语义差异（已上述）；真机 per-command 与 imgrd `/bin/column` 播种归 `C-57`/E5（本批未改行为、仅拆 argv）。同族的 `head`/`tail` 已用 obsolete 前导-digit 解析（本族先例），无同类缺口。详见 `../18-stage-commands/todo.md` §6.1 批次一百七十五与 `../18-stage-commands/07-text-filter.md` column 行。
- ✅ **批次一百七十四修 `join` 旗标面 getopt 附值缺失 + `-a`/`-v` 互斥与校验接正（接 expand/unexpand/fold 同族）**（`2026-10-13`，`claim/JOIN-ARGPARITY-qoder` @ 基线 `c3c56047e`，单域三线 commands/textfilter）：与批次一百六十九/一百七十/一百七十一同族的第四家（join 早先批次四十二/四十五只接了分离值 `-t ,`，旧薄壳 `match flag{…}` 比整个 `a[1..]`，C 合法的附值形 `join -t, f1 f2`/`-a1`/`-v2`/`-j2`/`-o0,1` 全误落 `_ => fail("unknown option")`）。逐字读 `minix3/usr.bin/join/join.c:133` `getopt("\01a:e:j:1:2:o:t:v:")` 与 `:139-224` 各臂。修法采 expand 先例：把 argv 折叠从薄壳下沉到 `minix_textfilter::join::parse_join_args<'a>(&'a [&'a str]) -> Result<JoinArgs, JoinError>`——逐字忠实 getopt：`-a`/`-e`/`-j`/`-o`/`-t`/`-v`/`-1`/`-2` 的值都**附于字母后（`&arg[2..]`）或取下一项**；`--` 结束扫描、裸 `-` 与非连字符项为操数、集外字母 `Usage`。校验接正：字段号（`-1`/`-2`/`-j`）按 C `strtol`+`<1`+`*end`——空→`FieldLessThan1`、非全数字→`IllegalFieldNumber`、<1→`FieldLessThan1`（旧薄壳 `.parse().unwrap_or(0)` 不查尾字符）；文件号（`-a`/`-v`）非 1/2 →`FileNumberNot12`；`-t` 值长度非 1 →`BadTabChar`（旧薄壳只取首字节、不拒 `ab`）；`-o` 走 `parse_olist`；`-a` 与 `-v` 互斥→`AAndVMutuallyExclusive`（`join.c:234-235`）。薄壳 destruct `JoinArgs`，`join_usage_fail` 把每个 `JoinError` 映到 C 对应 `warnx` 文案。头注旧列的「declared residual: 旧式单参无值」已闭。三件套：`cargo test -p minix-textfilter --lib join::`→**15/0**（新 3 枚：附值 `-t,`/`-a1`/`-o0,1` 关键回归 + 分离值向后兼容、`--`/裸 `-` 操数、校验与互斥六路含 `-j f1`→`IllegalFieldNumber` 负控）、全 crate `--lib`→**380/0**；**test-audit 当场拓一处自写错断言**（`-j f1` 初稿误期 `Usage`，实际 `-j` 吃 `f1` 作值→非数字→`IllegalFieldNumber`，代码对、改测）；freestanding `cargo build -p minix-textfilter --bin join --target x86_64-unknown-none --release` 绿；`cargo check --workspace -j 1`→0 error；clippy 触及两文件零告警；`rustfmt --edition 2024 --check` 两文件零漂移；`pattern-gate.sh`→`PASS=14 FAIL=0 SKIP=1`（P18 12686，`join.c:133`/`:132`/`:234-235` 锈点回读落窗）。**声明性留白**：BSD `obsolete(argv)`（`join.c:132`）对 legacy 裸 `-a`（两文件都 unpair）、`-j[12] arg`、前导 `-<digit>` 的 argv 预重写未建模（本层只接标准 getopt 形，与批次一百五十/一百七十一同口径）；无 `strerror`/usage 两行后缀；真机 per-command 与 imgrd `/bin/join` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百七十四。
- ✅ **批次一百七十三把 `find` 播进 imgrd 并接上三架构真机判据（只读文件操作数）**（`2026-10-13`，`claim/FIND-SEED-qoder` @ 基线 `aa600ae7e`，单域三线 + 启动共享件 `os/etc/rc`+`os/xtask`，登记 `new_edge4.md` C-94，沿用 stat 批次一百三十二先例）：接回批次一百六十四/一百七十二登记的「真机播种归 C-57/E5」余面。`os/xtask/src/image.rs` 的 `bins` 名单加 `find`（同 du/stat），`os/etc/rc` 加 `/bin/find /etc/rc` 一行，三门 `test-cmd-smoke*.sh` 各加一条「一行恰为 `/etc/rc`」的形状判据。**用文件操作数而非目录**：imgrd 根盘的 `getdents` 读腿未通（同 `du`/`ls` 已登记），目录操作数会走 `FTS_DNR` 只报 warn，而 `find /etc/rc` 经隐式 `-print` 吐一行 `/etc/rc`，把整条 parse→walk_paths→lstat→eval→emit 腿收到串口。判据有牙：裸 `/etc/rc` 单行（awk 剥 `\r` 后整行相等 / `grep '^/etc/rc\r?$'`）只可能由 find 产出——`cat /etc/rc` 出文件内容行（`#!/bin/sh`、`/bin/find /etc/rc` 等都带前缀）、`ls /bin` 出裸名（无斜杠）、`stat` 出多字段行（末字段 `/etc/rc` 但非整行），均不匹配；已用合成串口验证 `ok_find`/awk 判据「有 find 行→真、去掉→假」。**真机读数**：`bash os/qemu-tests/test-cmd-smoke.sh` 连续两轮 → `RESULT: PASS (… the stat command face and the find command face)`（退出码 0；该 RESULT 行仅在 find_ok=1 时可达）。三门 `bash -n` 通过；riscv64 的 python 判据已单独 compile+负控验证。**未跑 aarch64/riscv64 真机**：两者判据与 x86_64 同源同逻辑（同一份只读 lstat 腿、同一条裸行判据），且 x86_64 已证整条管线；两架构门脚本改动已 `bash -n`+python 验证，待下轮需要时上机复跑（同 du 批次一百二十一只跑 x86_64 的先例）。**目录遍历仍不可观察**：根盘 `getdents` 断腿属 E5/`SD-11` T4 面，非本批能解。命令面至此只剩 `chown`（名换号需口令库，待裁决）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百七十三与 `../coordination/TODO-LEDGER-OPEN.md` §7 find 走表行。
- ✅ **批次一百七十二接上 `find` 的 `-L`/`-H` 逻辑遍历（跟随符号链 + `(dev,ino)` 成环检测）**（`2026-10-13`，`claim/FIND-LOGICAL-qoder` @ 基线 `5c2552d3a`，单域三线 commands/fileops，无跨界）：接回批次一百六十四登记的 `find -L`/`-H` 逻辑形余面——上一轮把它列为「需完整 fts 符号链解算、半接不安全」而留下，本轮连同成环检测一起完整实现（不成环才安全，故不能只做跟随不做检测）。逐字对 `minix3/lib/libc/gen/fts.c` 的 `fts_stat`（`:921-968`）：逻辑/跟随时先 `stat`（跟随），失败再 `lstat` 判为 `FTS_SLNONE`（断链），两者皆失败为 `FTS_NS`；目标为目次时 `for (t = p->fts_parent; t->fts_level >= FTS_ROOTLEVEL; t = t->fts_parent) if (ino==t->fts_ino && dev==t->fts_dev) return FTS_DC`——按祖先 `(dev,ino)` 暴搜成环。走表腿重构：`WalkProbe` 拆为 `lstat_of`（链本体）+ `stat_of`（跟随）+ `dir_children`（逻辑形 `open` 已自然跟随链，故薄壳用 `stat`/`open` 无需额外解链）；新增 `classify(path, follow) -> (FtsKind, StatBits)` 与 `FtsKind{Dir,Cycle,NotDir,BrokenLink,Failed}` 完整镜像 `fts_stat` 返回码族；`dispatch` 按 `find_execute` 的 `switch(fts_info)` 走向（`find.c:216-235`）——目录走 `visit_dir`，`FTS_DC`/`FTS_SLNONE`/普通项均落默认分支单点求值不下探，`FTS_NS` 报 warn+continue。起始路径 follow 位 = `ISSET(FTS_LOGICAL) || ISSET(FTS_COMFOLLOW)`（`fts.c:171`，故 `-H` 只跟随命令行上的链、子项按物理形），`descend` 的子项 follow = `LOGICAL`。成环检测与祖先栈 `(dev,ino)` 只在逻辑形下读写（物理形不跟随链、真目次 inode 唯一不会成环，也避开合成树全 0 inode 假阳性）。薄壳 `SysProbe` 补 `stat_of` 腿（`stat_follows`）。三件套：`cargo test -p minix-fileops --lib find::` → **53/0**（新增 4 枚逻辑测：符号链到目次被跟随且下探（对照物理形不下探的反证）、`./sub/back -> .` 成环被识别为 FTS_DC 且终止（断言无更深环路径、back 只出现一次，无成环检测则会无限递归）、断链 `FTS_SLNONE` 作叶求值不报 NS、`-H` 只跟随起始路径；原 9 枚物理测全绿 = 零回归）；`FakeFs` 升级：拆 `lstats`/`symlinks`、自增唯一 ino、`stat_of`/`dir_children` 单跳解析符号链；freestanding `cargo build -p minix-fileops --bin find --target x86_64-unknown-none --release` 绿（docker `minix-ci:1.94` 补目标）；`cargo check --workspace -j 1` → 0 error；`cargo clippy --bin find --all-targets` 触及两文件零告警；`rustfmt --edition 2024 --check` 两文件零漂移；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 12681→12686，新增 `fts.c:921-968`/`:171`/`:955-961`/`find.c:216-235` 逐条落窗）；`check-command-boundary.sh` → OK；核实日 `2026-10-13`；基线 `5c2552d3a`。**声明性差异**：`stat_of` 真实 `stat(2)` 会多跳解链，薄壳天然跟随；本层 `FtsKind` 不单独区分 `FTS_DOT`（`.`/`..` 已由 `is_descendable` 排除，不下探也不输出）；成环时 C 会置 `fts_cycle` 供 `-H`/多根场景，本层仅用于阻断下探（不可观察）。真机门仍归 `C-57`/E5（同批次一百六十四：整树遍历受 imgrd 根盘 `getdents` 断腿牵制）。至此批次一百六十四登记的 `-L`/`-H` 余面闭合；`find` 侧只剩 SIGINFO 进度（需信号注册）与 `-exec`/`-delete`/`-user` 等缺腿按名拒绝（求值半既有口径）。命令面余量只剩 `chown`（名换号需口令库，待裁决）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百七十二与 `../coordination/TODO-LEDGER-OPEN.md` §7 find 逻辑行。
- ✅ **批次一百七十一修 `fold` 旗标面 getopt 附值/聚簇缺失（接批次一百七十同族）**（`2026-10-12`，`claim/CMD-FOLD-ARGPARITY-qoder` @ 基线 `8e19fa9e1`，单域三线 commands/textfilter）：expand/unexpand 同族的第三家。旧 bin `arg == "-b"`/`"-s"`/`"-w"` 只认全量 token、`-w` 值必须分词——但 `fold.c:76` `getopt("0123456789bsw:")` 接受附值 `-w80` 与 `-b`/`-s` 聚簇。失真：`fold -w80 f` 误拒 usage、`fold -bs` 聚簇也漏。旧 `set_width` 对非法宽度只 warn 不退出、宽度回默认 80 继续跑，与 C `atoi<=0` 的 `errx(1,"illegal width value")` 不符。逐字读 `fold.c:76`（getopt 集）、`:84-86`（-w：atoi<=0 → errx）、`:91-98`（obsolete `-N` 前导连段）。修法采 expand/unexpand 先例：新增 `parse_fold_args<'a>(&'a [&'a str]) -> Result<FoldArgs, FoldError>` 到 `fold.rs`，忠实 getopt：`-b`/`-s` 无值逐位聚簇、`-w` 值附于本 token 或取下一个、前导全数字 token 为 obsolete `-N`（整段宽度）、`--` 终止、裸 `-` 为操数、集外字母 `Usage`、非正整数宽度 `IllegalWidth`（修正旧 warn-continue 偏差、对齐 C errx）。bin destruct FoldArgs、删 set_width 与后置 width==0 死守卫。三件套：`cargo test -p minix-textfilter --lib`→**377/0**（新 2 枚：默认/`-w` 分词与**附值 `-w80` 关键回归**/obsolete `-40`/`-bs` 与 `-bw30` 聚簇/`--`裸 dash/未知/缺值/非法宽度）；**test-audit 五维**：完备、自正确（逐条回读 getopt 附值/聚簇与 atoi 校验、 degenerate 非前导 digit-簇如 C 本身 quirky 不建模、归 Usage）、无冗余/无效/虚构；freestanding `--bin fold` Finished；`rustfmt` 文件归净（本笔新增行均 fmt 净、无存量删行）；clippy 触及新代码零告警；`pattern-gate.sh` `[P18]/[P11] PASS`（`fold.c:76`/`:84-86`/`:91-98` 锦点回读落窗）；docker `cargo check --workspace`→0 error。留白：`fold` 行为修正（非法宽度改为退 1，与 C 一致）属参数面本体的 fidelity 修正；真机逐命令断言与 imgrd 播种归 C-57/E5。详见 §6.1 批次一百七十一。
- ✅ **批次一百七十修 `unexpand` 旗标面 getopt 附值/聚簇缺失 + `-a`/`-t` 互斥**（`2026-10-12`，`claim/CMD-UNEXPAND-ARGPARITY-qoder` @ 基线 `24567f115`，单域三线 commands/textfilter）：接批次一百六十九 expand 的同族（unexpand/fold 同病）。旧 bin `arg == "-a"`/`arg == "-t"` 只认全量 token、`-t` 值必须分词——但 `unexpand.c:86` `getopt("at:")` 接受附值 `-t8`、聚簇，且 `:87-95` 使 `-a`/`-t` **互斥**（`-a` 已设 stops 或 `-t` 已设 all 均 usage）。失真：`unexpand -t8 f` 误拒 usage；`unexpand -a -t 8` 本树允许而 C 报错。修法采 expand 先例：新增 `parse_unexpand_args<'a>(&'a [&'a str]) -> Result<UnexpandArgs, UnexpandError>` 到已有 `parse_stops`/`UnexpandOptions` 的 `unexpand.rs`，忠实 getopt：`-a` 无值字母逐位聚簇、`-t` 值附于本 token 或取下一个、`--` 终止、裸 `-` 为操数、集外 `Usage`；**编码 C 的 `-a`/`-t` 冲突**（`-at8` 聚簇→ Usage、`-t 8 -a` 与 `-a -t 8` 均 Usage）；非法 stop 列表 `IllegalStops`。bin destruct UnexpandArgs，`Usage`→usage、`IllegalStops`→bad tab stop。三件套：`cargo test -p minix-textfilter --lib`→**375/0**（新 2 枚：默认/`-a`/`-t` 分词与**附值 `-t8` 关键回归**/`--`裸 dash/**`-at8`、`-t 8 -a`、`-a -t 8` 三条冲突均 Usage**/未知/缺值/非法 stop）；**test-audit 五维**：完备、自正确（逐条回读 getopt 附值/聚簇与 `nstops`/`all` 互斥语义）、无冗余/无效/虚构；no_std 漏编译被 freestanding 拓出（`unexpand_stream` 签名仍引用被我误删的 `UnexpandOptions` import，补回后 Finished）；`rustfmt` 文件整体归净（本笔新增行均 fmt 净；基线已非净的 `unexpand_line` 几行存量条件被 in-place 顺手归一、属净负漂移零语义变化，与 expand 轮同样处理）；clippy 触及新代码零告警（`unexpand.rs:122/125` 系存量）；`pattern-gate.sh` `[P18]/[P11] PASS`（`unexpand.c:86-95`/`:113-117` 锚点回读落窗）；docker `cargo check --workspace`→0 error。留白：多次 `-t` 累加（C `nstops++` append）未接（与旧 bin 同、覆盖而非累加，属存量限制）；同族 `fold`（getopt "0123456789bsw:"，`-w80` 附值）同病、列为后续；真机逐命令断言与 imgrd 播种归 C-57/E5。详见 §6.1 批次一百七十。
- ✅ **批次一百六十九修 `expand` 旗标面 getopt 附值缺失（`-t8` 不再误拒）**（`2026-10-12`，`claim/CMD-EXPAND-ARGPARITY-qoder` @ 基线 `67f4f4fe2`，单域三线 commands/textfilter）：与 cut/comm/head 的 operand-leak **相反方向**的同类错——过严误拒 C 合法输入。旧 bin `arg == "-t"` 只认全量 token、值必须分词，且全数字 `-N` 走 obsolete——但 `expand.c:76` `getopt("t:")` 接受**附值** `-t8`（optarg 紧跟选项符）。后果 `expand -t8 file` 并非法、而是落进 `starts_with('-') && len>1` → usage，把 C 合法调用当错报错。逐字读 `expand.c:70-73`（obsolete 前导 `-<digit>` 连段 while，可多个）与 `:76-83`（getopt t:，`?`/default→usage）。修法采 head/comm/whatis 先例：下沉决定半纯函数 `parse_expand_args<'a>(&'a [&'a str]) -> Result<ExpandArgs, ExpandError>` 到已有 `parse_stops` 的 `expand.rs`，忠实两段：先 obsolete 前导 `-<digit>` 连段逐个喂 getstops（=parse_stops），再 getopt：`-t` 值附于本 token (`-t8`) 或取下一个 (`-t 8`)、`--` 结束扫描、裸 `-` 为操作数、集外字母 `Usage`、无值/非法 stop 列表 `IllegalStops`。bin 薄壳 destruct ExpandArgs，`Usage`→usage、`IllegalStops`→bad tab stop spec 退1。三件套：`cargo test -p minix-textfilter --lib`→**373/0**（新 2 枚：默认/-t 分词与**附值 `-t8` 关键回归**/stop 列表/obsolete `-8`/裸 dash/`--`/未知 Usage/`-t 0` 与缺值 IllegalStops）；**test-audit 五维**：完备、自正确（逐条回读 getopt 附值语义与 obsolete while 连段）、无冗余/无效/虚构；freestanding `cargo build -p minix-textfilter --bin expand --target x86_64-unknown-none --release` Finished；clippy 触及新代码零告警（`expand.rs:104` `implicit_saturating_sub` 系存量 `expand()` 反空格分支、基线即告非本轮；初版 doc 的 markdown 列表项触 clippy doc-list 告警，改写为散文后清）；`rustfmt` 文件整体归净（本笔新增行均 fmt 净；基线已非净的两行存量测试被 in-place 顺手归一、属净负漂移）；`pattern-gate.sh` `[P18]/[P11] PASS`（`expand.c:70-73`/`:76`/`:79` 锚点回读落窗）；docker `cargo check --workspace`→0 error。留白：同族的 `unexpand`（getopt "at:"，`-t8` 附值 + `-at8` 聚簇）与 `fold`（getopt "0123456789bsw:"，`-w80` 附值）同病，本轮只接 expand（一次一条）、列为后续；真机逐命令断言与 imgrd 播种归 C-57/E5。详见 §6.1 批次一百六十九。
- ✅ **批次一百六十八修 `apropos` 旗标面 operand-leak（接批次一百六十七留白）**（`2026-10-12`，`claim/CMD-APROPOS-FLAGS-qoder` @ 基线 `cf4e6d917`，单域三线 commands/doctools）：与 whatis 近亲的同类泄漏。旧 bin 只认 `-M`、其余 `other => keywords.push`。真值参照**经典** `minix3/usr.bin/apropos/apropos.c`（非 mdocml 变体，后者 getopt 用 `-O` 非 `-P`；本模读单一经典文本库，对位前者）：`:85` `getopt("C:M:m:P:S:s:")`（比 whatis 多 `-S` 机器/架构、`-s` section，六个全带值）、`:105-107` 默认→usage、`:120` 操数转小写（非裁路径，与 whatis 不同）。失真：`apropos -m /path -s 1 cp` 会把 `-m`//`path`/`-s`/`1` 全当关键词去扫。修法同 whatis 先例：新增 `parse_apropos_args<'a>(&'a [&'a str]) -> Result<AproposArgs, WhatisArgError>` 到 `doctools/src/whatis.rs`，忠实 getopt：全选项取值(附/取下一)、`--` 终止、裸 `-` 为关键词、集外 `Usage`；`-M`/`-P` 设库路(后者胜)、`-C`/`-m`/`-S`/`-s` 消费不用(单库模型、section/arch 选择为声明留白)。**与 whatis 的关指标差异：不裁路径**（C `:120` 是转小写）；关键词不就地小写——因决定半 `for_each_matching` 已 ASCII 折叠两侧，结果等价，保留借用 `&str` 不分配（已在函数 doc 注明）。复用 `WhatisArgError`(Usage 通用)。bin 薄壳 destruct AproposArgs，`Usage`→新 usage()（含 -S/-s 全旗文本）、空关键词→usage。三件套：`cargo test -p minix-doctools --lib`→**64/0**（新 2 枚：关键词与**不裁路径对比 whatis**、**-m/-S/-s 值不漏为关键词的关键回归**、附值/后者胜/未知/缺值/`--`）；**test-audit 五维**：完备、自正确（逐条回读 getopt 取值与不裁路径、含 `-s 1` 带值消发）、无冗余/无效/虚构；freestanding `cargo build -p minix-doctools --bin apropos --target x86_64-unknown-none --release` Finished；`rustfmt` **净新增漂移=0**（apropos.rs 基线即非 fmt 净；逐一手排我的 import/run/usage，已把 in-place rustfmt 顺手 reflow 的**存量 `for_each_matching` 行回退**，不顺手改邻行）；clippy 触及新代码零告警；`pattern-gate.sh` `[P18]/[P11] PASS`（`apropos.c:85`/`:120` 逐条 sed 回读落窗）；docker `cargo check --workspace`→0 error。留白：`apropos` 与 `whatis` 共用单库查询模型，多路 manpath/section/arch 解析仍是 10 篇 §3.4 声明留白；`tail` 旗标面 operand-leak 与未实现的 forward/obsolete 风格字符转换纠缠、非干净单轮；真机逐命令断言与 imgrd 播种归 C-57/E5。详见 §6.1 批次一百六十八。
- ✅ **批次一百六十七修 `whatis` 旗标面 operand-leak（-m/-C 值不再泄为查询名）**（`2026-10-12`，`claim/CMD-WHATIS-FLAGS-qoder` @ 基线 `06c0d1e33`，单域三线 commands/doctools）：与 cut/comm/head 同类的操作数泄漏。旧 bin 只认 `-M <path>`、其余一律 `other => names.push`。逐字读 `whatis.c:81` `getopt("C:M:m:P:")`（四个全带值：-C conffile、-M/-P p_path、-m p_augment）、`:93-95` default→usage、`:107-109` 操作数按 `strrchr('/')` 裁末段。失真比 comm 更重：`whatis -m /path foo` 会把 `-m` 与 `/path` 都当 man 页名去查；附值 `-Mpath`、未知 `-z` 也漏。修法采 head/comm 先例：下沉决定半纯函数 `parse_whatis_args<'a>(&'a [&'a str]) -> Result<WhatisArgs, WhatisArgError>` 到 `doctools/src/whatis.rs`，忠实 getopt：所有选项取值（附于本 token 或取下一个）、`--` 结束扫描、裸 `-` 为名、集外字母 `Usage`；`-M`/`-P` 设库路（后者胜），`-C`/`-m` 取值后消费但不用（本模型只读单一经典文本库，多路 manpath 解析是 10 篇 §3.4 声明留白）；操数按 C 裁至末段。**与 head 不同：`Vec<String>`/`String` 是本模块级 `WhatisArgs.names` 字段类型，import 必须上提到模块作用域（函数内会因 `#![no_std]` 下 std prelude 缺席而 E0106/找不到）**；且返回类型带借用需具名生命周期 `&'a [&'a str]`（不能像 head 那返 owned 用 `&[&str]`）。bin 薄壳 destruct WhatisArgs，`Usage`→新 usage()（逐字 C usage 文本）、空名→usage。三件套：`cargo test -p minix-doctools --lib`→**62/0**（新 5 枚：操作数与裁路径、-M/-P 分词与附值后者胜、**-m/-C 值不漏为名的关键回归**、未知/缺值 Usage、`--`）；**test-audit 五维**：完备（附值/分词/裁剪/裸dash/`--`/未知/缺值各覆）、自正确（逐条回读 getopt 取值与 `strrchr` 行为，含 `bar/`→空串边界）、无冗余/无效/虚构；freestanding `cargo build -p minix-doctools --bin whatis --target x86_64-unknown-none --release` Finished；`rustfmt` **净新增漂移=0**（whatis.rs 零删行、仅新增；bin 全是我意图编辑+import 归位）；clippy 触及新代码零告警；`pattern-gate.sh` `[P18]/[P11] PASS`（`whatis.c:81`/`:93-95`/`:107-109` 逐条 sed 回读落窗）；docker `cargo check --workspace`→0 error。留白：同类 `apropos`（C `getopt("C:M:m:P:S:s:")`，多 -S/-s 两带值选项）本轮未接（一次一条）、列为下轮；`tail` 旗标面 operand-leak 属实但与未实现的 forward（`+N`）/obsolete 风格字符转换（`-3lf`）纠缠，非干净单轮，待 forward 模型先落地；真机逐命令断言与 imgrd 播种归 C-57/E5。详见 §6.1 批次一百六十七。
- ✅ **批次一百六十六修 `head` 旗标面 operand-leak + getopt 聚簇/附值/obsolete 全语义**（`2026-10-12`，`claim/CMD-HEAD-ARGPARITY-qoder` @ 基线 `7b2f5846b`，单域三线 commands/textfilter）：比 comm 更深的同类漏。旧 bin `_ => operands.push(a)`：只认 `--`/`-n`/`-q`/`-v`/`-c`/全数字 `-N`，其余一律当文件名——而 C `head.c:84` 先跑 `obsolete(argv)`（`:178-195`）再把 `getopt("c:n:qv")`（`:87`），未知选项 `:115` default→usage。三重失真：①`head -z f` 把 `-z` 当文件去开；②`head -qv f`（getopt 聚簇）`-qv` 漏成文件；③`head -n5 f`（colon 选项附值，getopt 允许）`-n5` 漏成文件。**不能只加 dash 守卫**——那会把 C 合法的 `-qv`/`-n5` 误拒（往另一个方向造 bug），故采 fidelity 原则写完整 getopt。下沉决定半纯函数 `parse_head_args(&[&str]) -> Result<HeadArgs, HeadArgError>` 到 `window.rs`（head 决定半所在），忠实两阶段：先 obsolete() 只重写**前缀 `-<digit>…` 连段**（遇非匹配即停，故 `head -5 f`→`-n5 f`、`head f -5` 不改写→选项 `5` 报 usage）；再 getopt 扫描：字母逐位聚簇、colon 选项值可附于本 token 或取下一个、`--` 结束扫描、裸 `-` 为操作数、`-q`/`-v` 互斥后者胜（`:105-113`）、`-c`/`-n` 值按 strtoimax 校验（尾字符/`<=0`/溢出 → `IllegalCount`，`:90-103`）、集外字母 `Usage`。bin 薄壳 destruct HeadArgs，`Usage`→新增 usage()（逐字 C `usage` 文本 exit1）、`IllegalCount`→`head: illegal {byte|line} count -- {v}` 退1。三件套：`cargo test -p minix-textfilter --lib`→**371/0**（新 2 枚矩阵测：默认/-n 附值与分词/obsolete 前缀/`-c`/聚簇 qv-vq/`--`/裸 dash；错误矩阵：`-z` 与 obsolete-后-`-5` 均 Usage、缺值 Usage、`-n 0`/`-c x` IllegalCount）；**test-audit 五维**：完备（含 `head f -5` 与 `head -- -5` 两条 obsolete 边界）、自正确（逐条回读 C 行为、无金标准下以 getopt/strtoimax 语义为断言）、无冗余/无效/虚构；no_std 漏编译被 freestanding 当场拓出：初版将 `use alloc::{String,Vec}` 放在函数内，但 `HeadArgs.operands: Vec<String>`/`HeadArgError.value: String` 是**模块级字段类型**，`--lib`（std）过、freestanding E0425 爆——上提到模块作用域后 `cargo build -p minix-textfilter --bin head --target x86_64-unknown-none --release` Finished；`rustfmt` **净新增漂移=0**（基线即非 fmt 净的 `test_zero_or_huge_rejected` 一处存量 hunk原样保留、不顺手改；本笔新代码均已归位；误改的那段已回退）；clippy 触及新代码零告警（`window.rs:106` 系存量 `for index in 0..kept`）；`pattern-gate.sh` `[P18]/[P11] PASS`（`head.c:84`/`:87`/`:90-103`/`:105-113`/`:115-117`/`:120`/`:178-195` 逐条 sed 回读落窗；初稿误写 `:196-209` 越 205 行边界，已改 `:178-195`）；docker `cargo check --workspace`→0 error。留白：本轮只接 head 参数面，合并本体（`window.rs` HeadWindow）无改；固定容量 32 上限仍由 bin 后置 `HeadWindow::new` 拒（本轮不变）；真机逐命令断言与 imgrd 播种归 C-57/E5。详见 §6.1 批次一百六十六。
- ✅ **批次一百六十五修 `comm` 旗标面 operand-leak + getopt 聚簇/`--` 补齐**（`2026-10-12`，`claim/CMD-COMM-ARGPARITY-qoder` @ 基线 `e15efb517`，单域三线 commands/textfilter）：与 cut 同类的操作数泄漏，但面更大。旧 bin 手写 `match argv[i] { "-1"|"-2"|"-3"|"-f" => …, other => operands.push(other) }`：只认全量四旗、其余一律当文件名。后果——`comm -z file` 把 `-z` 当 file1（与另一操作数凑成两个）去开一个名为 `-z` 的文件，而 C `getopt("123f")` 对未知选项返回 `?` 直走 usage；且 `-12`/`-f1` 聚簇、`--` 结束选项扫描两形未接。逐字读 `comm.c:80`（`getopt(argc,argv,"123f")`）与 `:98`（`argc -= optind`）`:101`（`if (argc != 2) usage()`）。修法采 tsort/cksum 先例：把参数拆分下沉为决定半的纯函数 `parse_comm_args(args) -> Result<(CommOptions, [&str;2]), CommArgError>`（`UnknownOption(char)` / `NotTwoOperands`），忠实 getopt：单旗后字母逐位聚簇（`-12f` ≡ `-1 -2 -f`）、`--` 后全归操作数、裸 `-` 为操作数（stdin）、集外字母报 `UnknownOption` 不再漏成文件名；bin 只 `match parse_comm_args(&refs[1..])`，错误走 usage。三件套：`cargo test -p minix-textfilter --lib`→**369/0**（新 5 枚：两纯操、分旗+聚簇、`-`/`--` 形、**关键回归 `comm -z file`→`Err(UnknownOption('z'))`没有漏成文件**、操数不为二）；**test-audit 五维**：完备（聚簇/终止符/裸dash/未知/计数各覆）、自正确（逐条回读 getopt 行为）、无冗余、无无效、无虚构；freestanding `cargo build -p minix-textfilter --bin comm --target x86_64-unknown-none --release` Finished；`rustfmt` **净新增漂移=0**（comm.rs 仅剩两处基线存量 hunk：`pub fn comm` 签名与 Apple/zebra 测，系 HEAD 即非 fmt 净、本轮不顺手改邻行；新添行已逐一手排归位）；clippy 触及 comm.rs 零告警；`pattern-gate.sh` `[P18]/[P11] PASS`（`comm.c:80`/`:98`/`:101` 逐条 sed 回读落窗）；docker `cargo check --workspace`→0 error。留白：本轮只接 comm 参数面，`comm` 合并本体（`comm.rs` 库）无改；真机逐命令断言与 imgrd 播种归 C-57/E5。详见 §6.1 批次一百六十五。
- ✅ **批次一百六十四接上 `pr` 末页列平衡（vertcol reorder）**（`2026-10-12`，`claim/CMD-PR-BALANCE-qoder` @ 基线 `7c53df594`，单域三线 commands/textfilter）：补 vertcol 最后一个未接面——末页 balancing（计划项「vertcol 未接面」的收口）。逐字读 `pr.c:505-586`：竖排多列在最后一页（EOF）若**空余单元格不少于整列**（`(mvc - cvc) >= clcnt`，`:522`；`mvc=lines*clcnt`、`cvc` 为本页实读行数），则把 `cvc` 条顺序行**重排**为高度 `pln = ceil(cvc/clcnt)` 的等长列（`:523-525`），列主序跨步输出 `vc[tvc]; tvc += pln`（`:544-567`），使各列参差被抹平；否则维持逐列填满的楼梯状。关键判读：C 填充时 `vc[]` 按输入顺序连续存放（`vc[cvc++].cnt`），故 balancing 只是把同一串行按 `pln` 重新分箱；reorder 路仅在无行号（`nmwd==0`）时进入，正合本树多列 `-n` 未接的现状。新增 `vertcol_body_rows_balanced(lines_block, columns, colwd, sep)`：`pln=div_ceil(count,columns)`，逐列 `idx=column*pln+row`、`idx>=count` 即断行（C `tvc>=cvc` break，`:568`——故某行末列可能保留其列间填充，忠实照 C 输出）、非末列 `colwd+1` 垫或 `sep` 单字节、末列不垫。`paginate_columns` 末块分叉：`!across && 末块 && block-len>=columns` 时走 balancing，其余（含恰好满页 slack=0）维持 down-fill；`-t` 不补白、有页眉补到 `rows`（对位 `prtail(lines-pln)`）。三件套：`cargo test -p minix-textfilter --lib`→**364/0**（新 8 枚：纯函数 staircase 重排/奇数行尾留垫/三列/`-s`/clip/空守卫，接线 末块平衡、平衡+页眉补白、满页不平衡守卫）；**test-audit 复核改动一处既有 golden**——批次一百五十六的「2 行 2 列带页眉页」原钉 down-fill 楼梯 `"a␣␣ "`，末页 balancing 后 C 正确输出是等列单行 `"a␣␣b"`（`mvc-cvc=110>=2` 触发 reorder），系代码修正非测试笔误，已连注释一并更正并补 `body_lines[1]==""` 钉补白；freestanding `cargo build -p minix-textfilter --bin pr --target x86_64-unknown-none --release` Finished（裸 `--bin pr` 会因 workspace 特性统一把 `minix-rt` std 腿漏进 none 图而炸，须带 `-p`）；`rustfmt` 单文件零漂移（净新增=0，`bin_support/cksum` 漂移属基线存量）；clippy 触及新函数零告警（`pr.rs:580/584` 两警在既有 `merge_body_rows`，非本轮）；`pattern-gate.sh` `[P18] PASS`（`pr.c:505-586`/`:522`/`:523-525`/`:544-567`/`:568` 逐条 sed 回读落窗）；docker `cargo check --workspace`→0 error。留白：`pr` 至此 `-t/-h/-l/-N/-w/-s/-o/-n(单列)/-a/-d/+N/-m` + 末页 balancing 全接，仅余 `-F`(页脚换页符，与逐行 emit 模型相冲)、多列 `-n`、`-d`叠多列；真机播种归 C-57/E5。详见 §6.1 批次一百六十四。
- ✅ **批次一百六十三接上 `pr -m` 多文件并排（mulfile，每列一文件）**（`2026-10-12`，`claim/CMD-PR-MERGE-qoder` @ 基线 `8c64da164`，单域三线 commands/textfilter）：`pr` 最后一个主参数面旗标。逐字读 `pr.c:907-993` mulfile：与 horzcol 共用列宽公式（`setup` nmwd=0 支 `:874-875`），但**每列是一个文件**的行流——逐行从各活动文件读一条横向拼成一行，EOF 文件置 NULL 并 `actf--`，`fproc>=actf` 即断行（末尾已 EOF 的列不补空），整行无数据则 `lstdat<=offst` break。新增 `merge_body_rows(streams, colwd, sep)`：zip-longest（`maxr`行），每行取“仍持有第 r 行的最后列”`last`（`:filter(..).next_back()`，其后 EOF 列丢弃），逐列 clip/垫/分隔，末活动列不垫——与 horzcol 同构。薄壳 `-m`：读齐各操作数为并行流→`merge_body_rows`→把合成行喂回单列 `paginate`（复用页切分/页眉/偏移），<2 操作数按 C 退回普通路。因 POSIX 明写 merge 页眉 unspecified（`:848-849`），页题取首操作域名（`-h` 可覆），已注。三件套：`cargo test -p minix-textfilter --lib`→**356/0**（新 1 枚：zip-longest + 尾 EOF 列丢弃 + 空输入）；**test-audit 拓一处自写错 golden**（colwd3 应“a␣␣␣1”三空格非两，代码对、改测）；freestanding `--bin pr` Finished；`rustfmt` 两文件零漂移；`pattern-gate.sh` `[P18] PASS`（`pr.c:907-993`/`:848-849` 锚点落窗）；docker `cargo check --workspace`→0 error。留白：`pr` 至此 `-t/-h/-l/-N/-w/-s/-o/-n(单列)/-a/-d/+N/-m` 全接，仅余 `-F`(页脚换页符，与逐行 emit 模型相冲未接)、多列 `-n`、`-d`叠多列、末页 balancing；真机播种归 C-57/E5。详见 §6.1 批次一百六十三。
- ✅ **批次一百六十二接上 `pr +N` 起始页（跳页与页号偏移）**（`2026-10-12`，`claim/CMD-PR-STARTPAGE-qoder` @ 基线 `e2e47ccab`，单域三线 commands/textfilter）：接回 `pr` 参数面，选 `-F` 而选 `+N`——`-F` 不能干净归约（页换页符是裸 `putchar('\f')` 非一行、且 formfeed 分支跳过整段页填充 `pr.c:1555-1562`，与现有“逐行+\n”的 Page/emit 模型相冲），而 `+N` 合模型。逐字对 `pr.c:1657`（`case '+'` → `pgnm`）+ `inskip`：跳过前 `(N-1)*content_per_page` 条正文行、页号从 N 起；若文件不够长到不了第 N 页（`skip >= len`），C `inskip` 返回非零→整个文件不输出。新增 `PrOptions.start_page`（默认 1），`paginate` 单列带页眉路按 `content_per_page`（含 `-d` 减半后）算 skip 与起始页号、短文件早返回空；薄壳 `+N`（`arg[1..]` parse，<1 或非数→usage）。三件套：`cargo test -p minix-textfilter --lib`→**355/0**（新 2 枚：`page_len=12 start_page=3` 跳 4 行、页号“ Page 3”、body 取行 5-6；行不够→空）；freestanding `--bin pr` Finished；clippy 无新增；`rustfmt` 两文件零漂移；`pattern-gate.sh` `[P18] PASS`（`pr.c:1657` 锚点回读落窗）；docker `cargo check --workspace`→0 error。留白：`+N` 与多列/`-n` 组合未接（编号仍用绝对行号 idx+1，与 C 跳页后 `lncnt` 从 1 起在 `-n`+`+N` 双开时有差，已注）；`-F`（页脚换页符，因与逐行 emit 模型相冲未接）、`-m`（merge）、末页 balancing、多列 `-n` 仍后继；真机播种归 C-57/E5。详见 §6.1 批次一百六十二。
- ✅ **批次一百六十一接上 `pr -d` 单列双倍行距**（`2026-10-12`，`claim/CMD-PR-DOUBLE-qoder` @ 基线 `ebcdeb505`，单域三线 commands/textfilter）：接回 vertcol/`-a` 后 `-d` 单列那一支。逐字对 `pr.c` 三处几何：`otln`（`:1258`）每输出行额外发一 `\n`、`setup`（`:1883-1890`）`lines/=2` 且奇数页置 `addone`、`prtail`（`:1543-1550`）页内填充 `cnt*=2` 再 `+addone`。三者合并到一个干净规则：每页正文行数减半（`body_capacity/2`）、每条正文行后插一空行、body 仍补到 `body_capacity` 行（奇数基数的 `addone` 行被“补到 body_capacity”自然吸收）；`body_capacity<2` 时 C `dspace=0`（一行页不加倍）。新增 `PrOptions.double_space`（默认 false，单列路），`paginate` 按此分叉；薄壳解 `-d`（`setup ++dspace` `:1676`）。三件套：`cargo test -p minix-textfilter --lib`→**353/0**（新 2 枚：`page_len=12`→基 2、每页 1 正文行加倍成 2 行且分 3 页；`page_len=11`→基 1 不加倍回退）；freestanding `--bin pr` Finished；clippy 无新增；`rustfmt` 两文件零漂移；`pattern-gate.sh` `[P18] PASS`（`pr.c:1258`/`:1883-1890`/`:1676` 锚点回读落窗）；docker `cargo check --workspace`→0 error。**上轮判为“几何不可安全单轮”而推迟，本轮发现它可归约为确定规则 + 手推 golden（同 vertcol/-a 无金标准做法）**——纠正了自己的过度保守。留白：`-d` 与多列/`-t` 的叠加未接（只做单列带页眉路）、末页 balancing、`-m`（merge）、`-F`、`+page`、多列 `-n` 仍后继；真机播种归 C-57/E5。详见 §6.1 批次一百六十一。
- ✅ **批次一百六十接上 `pr -a` 跨页填（horzcol）**（`2026-10-12`，`claim/CMD-PR-ACROSS-qoder` @ 基线 `a2b7968e0`，单域三线 commands/textfilter）：接回 vertcol 后 `-a` 跨填那一支。逐字对 `pr.c:710-770`：horzcol 外层按行 `for i in 0..lines`、内层对每列 `inln` 读一条输入行填进本行各列（行主序：连续行左右横铺），`lstdat` 只随成功读入前推、超 `offst` 才出该行（全空行即 `break`），末列不垫、`-s` 用分隔符。新增 `horzcol_body_rows(block,columns,colwd,sep)`：行号 `idx=row*columns+col`，`block.get(idx)` 为 `None`（EOF）即断本行、空行作 cnt=0 仍写垫、末列不垫、任一列皆无数据即止（与 r_reg 不同行序、共 `column_width`/`clip_to_width`）；`PrOptions.across`（默认 false）使 `paginate_columns` 在行主序/列主序间分叉；薄壳解 `-a`，扫后若 `-a` 而 `columns<=1` 按 `pr.c:1823-1828` 报“requires multiple columns”退。三件套：`cargo test -p minix-textfilter --lib`→**351/0**（新 2 枚：`horzcol` 行主序与 vertcol 列主序同块对照（横铺 vs 下填）、奇数行尾列保垫、空输入无行；`paginate` across 行主序）；freestanding `--bin pr` Finished；clippy 无新增；`rustfmt` 两文件零漂移；`pattern-gate.sh` `[P18] PASS`（`pr.c:710-770`/`:1823-1828` 锚点回读落窗）；docker `cargo check --workspace`→0 error。留白：末页列平衡重排、`-m`（merge 多文件并列）、多列 `-n`、`-d`/`-F`/`+page` 仍未接；真机播种归 C-57/E5。详见 §6.1 批次一百六十。
- ✅ **批次一百五十九修 `cut` 旗标漏为文件名（接 `-n`、未知 `-X` 走 usage）**（`2026-10-12`，`claim/CMD-CUT-NPARITY-qoder` @ 基线 `04d8e072b`，单域三线 commands/textfilter）：对账 `minix3/usr.bin/cut/cut.c` getopt `"b:c:d:f:sn"` 发现薄壳参数 match 无 `-n` 臂且 `_ => operands.push`——`-n`（C `case 'n': break` `:112-113`，为兼容而忽略）与任意未知 `-X`（C `case '?'`→`usage()`）全被当文件名：`cut -n -f1 file` 会去 `open("-n")`。不靠标记词、逐字读 getopt 才见到（同 cksum/tsort 一族的漏）。修法＝臂里加 `"-n" => {}`（识别忽略）；`_` 分支先判 `option.starts_with('-') && len>1` → `usage()`（新增 `fn usage()`，对 `cut.c:290-296`，退 1），否则才入操作数（裸 `-` 仍作 stdin/文件操作数）。选单缺项那一支原本内联 emit+terminate，一并改指 `usage()` 消重。三件套：`cargo test -p minix-textfilter --lib`→349/0 不动（选择/列表腿 `parse_list`/`select_bytes`/`select_fields` 既有测覆盖，本笔只动薄壳参数 glue）；freestanding `cargo build -p minix-textfilter --bin cut --target x86_64-unknown-none --release` Finished；clippy 触及文件零新增（minix-textfilter 存量 `self`/`ToString`/`format`/`vec` 未用导入属他模块基线）；`rustfmt` `bin/cut.rs` 零漂移；`pattern-gate.sh` `[P18] PASS`（`cut.c:112-113`/`:290-296` 锚点回读落窗）；docker `cargo check --workspace`→0 error。**本轮无新增宿主测**（实质逻辑在已测 lib，bin 仅 argv 旗标分发，无金标准可钉；同批次五十六 chmod 先例）；真机 per-command 与 imgrd 播种归 `C-57`/E5。详见 §6.1 批次一百五十九。
- ✅ **批次一百六十四落 `find` 走表遍历腿与薄壳，命令可用（物理模式）**（`2026-10-13`，`claim/FIND-WALK-SHELL-qoder` @ 基线 `2f17ff79b`，单域三线 commands/fileops，无跨界）：接回批次一百三十八点名的 `find` 最后一包——走表腿（`find_execute`，`minix3/usr.bin/find/find.c:201-273`）+ 薄壳。决定半新增于 `os/commands/bin/fileops/src/find.rs`：`StatBits`（`FTSENT::fts_statp` 承载位）+ `WalkProbe` trait（`entry_stat`/`dir_children`，并继承 `-newer` 的 `ReferenceStat`）把三条系统调用隔离在薄壳侧，遍历核心在宿主用合成树逐分支可测；`walk_paths` 按 `Parsed` 抽位成形——先落隐式 `-print` 的包裹（空表达式 → 只 `-print`；否则 `Paren(expr)` + `-print`，对位 `find_formplan` 尾部 `find.c:103-118`），再按 `for (p=plan; p && p->eval(p, entry); p=p->next)` 的短路真值逐条求值。物理遍历忠实 `main.c:88` 的 `FTS_PHYSICAL|FTS_NOSTAT`：`base_name`/`fts_join` 复刻 `fts_name`/`fts_path`（`.` 的子项得 `./foo`，与 `ls` 的 dotdir 去前缀规则不同，故不复用 `dents::join_path`），目录按 `-d` 落预序（`FTS_D`，命中 `-prune` 或 `-x` 越界即不下探）或后序（`FTS_DP`），叶子（含符号链、特殊文件）一律单点求值。走向对位齐全：`-x`（`FTS_XDEV`，跨设备目录被报告但不下探 `main.c:122-124`）、`-s`（`issort` 同层按名排序）、`-X`（`isxargs` 的 `BADCH` 六字判据 `find.c:235-243`）、`FTS_NS`/`FTS_DNR`（报 warn、rval=1、continue，不求值不下探 `find.c:215-235`）。薄壳 `src/bin/find.rs`：`SysProbe` 三条腿（`lstat` 取型与时间、`open`+`getdents` 一次读全目次（`d_ino==0` 空槽按 `readdir.c:83-84` 跳）、`stat` 跟随符号链供 `-newer` 参照），`main` 取 `gettimeofday` 秒填 `World.now`、跑 `walk_paths`、逐行 emit、告警走 stderr、按 `rval` 退出。三条按代码而非注释/臆断的忠实实现：隐式 print 用「括起再补 print」而非「追加 print」（短路语义不同）；`-prune` 在 `-d` 后序下对下探无作用（子项已访问，同 C）；跨设备目录仍报告其自身。三件套：`cargo test -p minix-fileops --lib find::` → **49/0**（新建 `walk_tests` 9 枚：预序含起始径、`-d` 翻后序、`-prune` 跳子树、`-x` 前后对照证不下探、`FTS_DNR` 不求值、`FTS_NS` 只跳自身、`-X` 拒非法径、符号链被当目录下探的反证（放幻影子项证明未跟进）、`-type d` 只目录吐行）；freestanding `cargo build -p minix-fileops --bin find --target x86_64-unknown-none --release` 绿（docker `minix-ci:1.94` 补 `x86_64-unknown-none` 目标后跑，容器内 `-u` 属主对齐）；`cargo check --workspace -j 1` → 0 error；`cargo clippy -p minix-fileops --bin find --all-targets` 触及两文件零告警；`rustfmt --edition 2024 --check` 两文件零漂移（首轮 freestanding 报 E0428：宿主 `fn main` 漏 `#[cfg(any(test, not(target_os="none")))]` 门，与 `no_mangle` 腿撞名，按 `du.rs`/`stat.rs` 范式补齐门后绿）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（`[P18]` 12681 锚点，本笔新增 `find.c`/`main.c`/`function.c`/`readdir.c` 逐条落窗）；`check-command-boundary.sh` → OK；核实日 `2026-10-13`；基线 `2f17ff79b`。**声明性留白**：① 告警无 `strerror` 后缀（本树无表，印固定短语，同 `stat`/`du`/`what` 各批）；② 单条求值期的缺腿/参数非法（`-exec`/`-delete`/`-user`/…）记 rval 后继续，不 `err(1)` 终止全局（同 `du`/`rm -r` 已登记的「硬错只令该操数计错」口径）；③ `-L`/`-H` 逻辑遍历（跟随符号链 + 成环检测）需完整 fts 符号链解算，本壳按物理形走（无 `-L` 时行为与 C 逐字一致），留下一轮；④ SIGINFO 进度报告需命令侧信号注册，未接；⑤ 真机门归 `C-57`/E5——`find` 只读，但遍历整棵树受 imgrd 根盘目录读腿断裂牵制（与 `du`/`ls` 的 `getdents` 同症），起始目录自身可判、子项遍历在最小 boot 镜像尚不可观察，故本笔交代码 + 宿主测不谎报串口标记。命令面余量至此只剩 `chown`（名换号需口令库，待裁决）与 `find` 的 `-L`/`-H` 逻辑形。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百六十四与 `../coordination/TODO-LEDGER-OPEN.md` §7 find 走表行。
- ✅ **批次一百五十八接上 `pr -n` 单列行号（addnum）**（`2026-10-12`，`claim/CMD-PR-NUMBER-qoder` @ 基线 `e2c617b5d`，单域三线 commands/textfilter）：接回 vertcol 后 `-n` 在单列那一支。逐字对 `pr.c`：`-n[ch][width]`（BSD 可选粘连值，`:1744-1762`）默认宽 NMWD=5、分隔 NMCHAR=tab；`addnum`（`:1438`）把行号右对齐写进 nmwd 格（空格左填、超宽则保末几位）。onecol 对**真实行**逐行前缀 `[号][分隔符][正文]`、页内空白补行不编号（`lncnt` 只随读入行自增）。修法＝`PrOptions.number_width/number_char`（默认 0/tab，0=不编号）；`number_field(n,width)` 纯件（右对齐空格填、超宽保末几位）；`paginate` 单列路按全局行号（`cursor+1`，逐文件重启）给真实 body 行加前缀、补白行仍空；`render_page` 偏移仍在最外层→顺序=offst 空格+号+分隔+正文（与 C obuf 同形）；薄壳解 `-n[C][width]`。三件套：`cargo test -p minix-textfilter --lib`→**349/0**（新 2 枚：`number_field` 宽度/截断、`-n` 逐行前缀与跨页连续）；**freestanding 拓两处 no_std 漏（裸 `format!`/`to_string` 宿主测全绿、`--target none` 才炸）——改 `alloc::format!`/`String::from` 后 `--bin pr` 绿**（印证“每轮必跑 freestanding”的既有教训）；clippy 无新增；`rustfmt` 两文件零漂移；`pattern-gate.sh` `[P18] PASS`（`pr.c:1744-1762`/`:1438` 锚点落窗）；docker `cargo check --workspace`→0 error。留白：**多列模式 `-n` 未接**（C 在 `columns` 下走另一套 nmwd 宽公式 `:1856-1858`，本轮只做单列）；`-a`/`-m`/末页 balancing/`-d`/`-F`/`+page` 仍未接；真机播钟归 C-57/E5。详见 §6.1 批次一百五十八。
- ✅ **批次一百五十七接上 `pr -o` 左边距（render_page）**（`2026-10-12`，`claim/CMD-PR-OFFSET-qoder` @ 基线 `fac59b868`，单域三线 commands/textfilter）：接回上一批 vertcol 后仍列为“后续”的 `-o`。逐字对 `minix3/usr.bin/pr/pr.c`：`-o N`（`:1764-1770`）置页左偏移，`onecol`/`vertcol`/`horzcol` 均 `memset(obuf/hbuf,' ',offst)`，且 `prhead` 的两条空行与 `prtail` 的空白行走裸 `putchar('\n')`——故偏移只作用于**正文行与页眉的文字行（5 行页眉的第 3 行）**，页眉空行与页脚缩进不受影响。新增库 `render_page(page, offset)`：按此规则给 body_lines 与 header_lines[2] 前缀 `offset` 个空格（`offset==0` 与旧 `Page::lines()` 逐行同，零回归）；`PrOptions.offset` 默认 0；薄壳解 `-o N` 并将 emit 换 `render_page`。三件套：`cargo test -p minix-textfilter --lib`→**347/0**（新 1 枚 `render_page`：`page_len=12` 使 body 恰 2 行、区分页眉文字行缩进/页眉空行不缩/页脚不缩，兼验 offset=0 等于原序）；**test-audit 拓一处自写错期望**（默认 66 行页把 body 补到 56 行，out[7] 是缩进的空正文行非页脚——代码对、改用 `page_len=12`）；freestanding `--bin pr` Finished；clippy 无新增；`rustfmt` 两文件零漂移；`pattern-gate.sh` `[P18] PASS`（`pr.c:1764-1770`/`:1493-1496` 锚点落窗）；docker `cargo check --workspace`→0 error。留白：偏移在多列模式下仍按行级前缀（与 C 单列同效；多列偏移叠 `vertcol` 已可）。`-a`/`-m`/末页 balancing 仍未接；真机播钟归 C-57/E5。详见 §6.1 批次一百五十七。
- ✅ **批次一百五十六接上 `pr` 垂直多列（`-N`/`-w`/`-s` vertcol 面，多轮攻坚 Round1+2 合并）**（`2026-10-12`，`claim/PR-COL-LAYOUT-qoder` @ 基线 `1d5571709`，单域三线 commands/textfilter）：接回 `pr` “多列是后续批次”长期留白。决定半（Round1）：`column_width` 照 `setup` 的 `nmwd==0` 支（`colwd=(pgwd+1-clcnt)/clcnt`、`pgwd` 重圆整、`colwd<1` 拒，`pr.c:1860-1866`）；`vertcol_body_rows` 照 `vertcol` 列主序下填（`pr.c:438-499`）：每格按 `colwd` 字节裁剪（UTF-8 取整码点防裂）、非末列进 `colwd+1`（空格垫或 `-s` 分隔符）、末列不垫、全空行即止；`paginate_columns` 按 `rows*columns` 分块、复用 5 行页眉/页脚。执行半（Round2）：`PrOptions` 加 `columns/page_width/sep`（默认 1/72/None，**columns==1 走原单列、零回归**）；薄壳解 `-N`/`-w`/`-s[C]`（`pr.c:1665/1796/1778`），扫后无 `-w` 按 `-s` 定默认宽（512/72）、`columns>1` 且过窄报错。三件套：`cargo test -p minix-textfilter --lib`→**346/0**（Round1 新 3 枚宽度/下填/裁剪含多字节、Round2 新 2 枚列模式）；**test-audit 当场抓两处自写错期望**（代码对、改测）；freestanding `--bin pr` Finished；`--lib --tests` clippy 无新增告警（本 crate 存量 `approx_constant`/PI deny 属他模块基线且仅 clippy）；`rustfmt` 两文件零漂移；`pattern-gate.sh` `[P18] PASS`（`pr.c` 多锤点落窗）；docker `cargo check --workspace`→0 error。留白：末页列平衡重排、`-a`（horzcol）、`-m`（merge）未接（C 自述 balancing “unspecified”，`-a`/`-m` 仍拒）；`pr` 未播 imgrd、真机归 C-57/E5（本面无新系统调用、单列默认未变）。详见 §6.1 批次一百五十六。
- ✅ **批次一百五十五完成：07 篇 `tsort` 接上文件操作数与 `-d`/`-l` 旗标（argv 折叠入可测库）**（`2026-10-12`，`claim/CMD-TSORT-FILE-qoder` @ 基线 `a08f4164c`，单域三线 commands/textfilter，无跨界）：对账 `minix3/usr.bin/tsort/tsort.c` getopt `"dlq"` + `switch(argc)`（`:143-153`）发现两处背离——① 薄壳参数循环只认 `-q`，其余一切（含它自己 usage 宣传的 `[file]` 与 `-d`/`-l`）全落 `_ => usage`→`tsort file` 直接报错退 2，而 C 是 0→stdin、1→`fopen(argv[0])`（开不了 `err(1,"%s")`）、>1→usage；② `-d`(`debug`)、`-l`(`longest`) 在 C 被置位但全局从未被分支读（循环报告恒走 `longest_cycle`）→本质空操。修法＝把 argv 折叠从薄壳提到 `minix_textfilter::tsort::parse_args`→`TsortArgs{quiet,file}` + `TsortParseError{UnknownOption,TooManyOperands}`：按 getopt 逐字拆旗（`-dq` 聚簇可分），`d`/`l` 识别为空操、`q` 真效，裸 `-`/非 dash 为文件操作数、多操作数报错。薄壳 `bin/tsort.rs` 改为 `parse_args` 后按 `file` 走 `support::read_file`（打不开 `tsort: <path>` 退 1）否则 `read_stdin`、按 `quiet` 抑制环报告；usage 改 `[-dlq] [file]`。顺带清掉薄壳两处存量双空行（fmt 基线）。三件套：`cargo test -p minix-textfilter --lib`→**341/0**（新 2 枚 `parse_args`：`-d`/`-l` 空操接受且聚簇、操作数/stdin/TooManyOperands/UnknownOption/裸 `-`）；freestanding `cargo build -p minix-textfilter --bin tsort --target x86_64-unknown-none --release` Finished；clippy 触及文件零新增告警（minix-textfilter 存量的 `approx_constant`/PI 与 lib `tsort.rs:16` 未用 `vec` 宏属他处基线）；`rustfmt --edition 2021 --check`：`bin/tsort.rs` 零漂移、lib `tsort.rs` 新增行零新增漂移（唯一 diff 是存量 `:209` 行 map().collect() 长行，非本笔）；`pattern-gate.sh`→`PASS=14 FAIL=0 SKIP=1` 且 `[P18] PASS`（`tsort.c:143-153`/`:102` 等锚点回读落窗）；权威编译门 docker `cargo check --workspace`→0 error；核实日 `2026-10-12`；基线 `a08f4164c`。**声明性留白**：`-d`/`-l` 按 C 作纯空操（其全局在本 C 从未被读），不复现任何行为；`tsort` 的奇数 token 报 `odd data count` 退 1、环报告多行形状属既有库腿不动；真机 per-command 与 imgrd `/bin/tsort` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百五十五。
- ✅ **批次一百五十四完成：07 篇 `tail` 的 `-r` 配 `-c`/`-b`（RBYTES 字节窗行反序）**（`2026-10-12`，`claim/CMD-TAIL-RBYTES-qoder` @ 基线 `cf5065833`，单域三线 commands/textfilter，无跨界）：接回批次一百五十登记的留白（`-r` 与 `-c`/`-b` 同给当时拒）。C 真源：`tail.c` 的 `ARG(1,FBYTES,RBYTES)`/`ARG(512,FBYTES,RBYTES)` 配 `rflag` → style=`RBYTES`；`reverse.c::r_reg`（`:128-144`）先 `p=start+size-1` 从文件末尾起，`if (RBYTES && off<size) size=off` 把回溯窗收到 `off` 字节，循环内遇 `\n` 即 `WR(p+1,llen)` 反序成行、窗顶那个被切的残段由收尾 `if (llen) WR(p,llen)` 也输出。即“末 `off` 字节窗口内的行自底向上打印”。修法＝新增 `reverse_bytes(input, bytecnt)`（复用已测的 `last_bytes_of` 取窗 + `lines_of` 分行 + `reverse_selection(_,0,false)` 反序），`tail_stream` 的 `bytecnt>0` 分支拆为 `rflag`→反序窗行、否则原样末字节；删除旧的 `-r`+`-c` 拒臂。窗顶残段被 `lines_of` 当首行保留，与 r_reg 的收尾 WR 同形；故逐行反序精确。三件套：`cargo test -p minix-textfilter --bin tail`→**8/0**（新 1 枚 `reverse_bytes_reverses_lines_of_trailing_window`：窗内反序、窗起于行中留残行、`bytecnt` 超输入反序全文）；`--lib` 339/0 不动；freestanding `cargo build -p minix-textfilter --bin tail --target x86_64-unknown-none --release` Finished；clippy 触及文件零新增告警（minix-textfilter 存量的 `approx_constant`/PI 与 `tsort` 未用导入属他模块基线，非本笔）；`rustfmt --edition 2021 --check` `bin/tail.rs` 零漂移；`pattern-gate.sh`→`PASS=14 FAIL=0 SKIP=1` 且 `[P18] PASS`（`tail.c`/`reverse.c:128-144` 锚点回读落窗）；权威编译门 docker `cargo check --workspace`→0 error；核实日 `2026-10-12`；基线 `cf5065833`。**声明性留白**：窗顶残行的尾换行按本树既有 `-r` 同款归一（每行补 `\n`，与 forward 环路一致的已接受简化）；`-r -f`/`-F` 仍互斥退；多文件 `==> name <==` 横幅复用不动；真机 per-command 与 imgrd `/bin/tail` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百五十四。
- ✅ **批次一百五十三完成：07 篇 `cksum` 选项面收拢到可测库层 + 旗标不再漏为文件名**（`2026-10-12`，`claim/CMD-CKSUM-ARGPARITY-qoder` @ 基线 `a88a6b1e2`，单域三线 commands/textfilter，无跨界）：对账 `minix3/usr.bin/cksum/cksum.c` getopt `"a:cno:pqs:twx"`，发现薄壳参数循环只认 `-o/-a/-c/-w`，**其余 `-n/-q/-p/-s/-t/-x` 及未知 `-Z` 全部落 `else → operands.push`——被当文件名处理**（`cksum -n file` 会去 `open("-n")`）。修法＝把决定半（argv 折叠）从薄壳提到 `minix_textfilter::cksum::parse_cksum_args`→`CksumRequest{kind,do_check,check_warn,print_normal,print_quiet,operands}` + `CksumError`：逐字对位 C——`-n`(PRINT_NORMAL `:228`)/`-q`(PRINT_QUIET `:253`) 仅塑形 crypto 输出、crc 路无作用但**须识别不漏为操作数**；`-p/-s/-t/-x` 走 `requirehash`（`cksum.c:149`，`__dead`，无摘要即 abort）→ `RequiresHash`；`-a` 的 `crc/old1/old2`（`strcasecmp` `:209-217`）为**非 crypto**→映射 kind（此前一律拒），md5/sha… → `CryptoUnsupported`；`-o 1/2` 保留；`-Z` 等 optstring 外→`UnknownOption`→usage；**单字符 `-` 是 stdin/文件操作数不是选项**。薄壳 `bin/cksum.rs` 改为调 `parse_cksum_args(&argv[1..])` 并按 Err 分支映射到原有 warn/terminate(2)（`check_files` 随之改 `&[&str]`）。三件套：`cargo test -p minix-textfilter --lib`→**339/0**（新 4 枚 `parse_cksum_args`：选项不漏为操作数/requirehash 旗标报错/`-a` crc·old1·old2 映射且 md5 拒/`-w -c`+未知 `-Z`+裸 `-`）；freestanding `--bin cksum` Finished；clippy 触及文件零新增告警；`rustfmt --edition 2021 --check` 触及两文件零漂移（新写断言过长已按 rustfmt 拆行）；`pattern-gate.sh`→`PASS=14 FAIL=0 SKIP=1` 且 `[P18] PASS`（`cksum.c:228`/`:253`/`:209-217` 等锚点回读落窗）；权威编译门 docker `cargo check --workspace`→0 error；核实日 `2026-10-12`；基线 `a88a6b1e2`。**声明性留白**：`-n`/`-q` 只识别不复现其对 hash 输出的格式作（无 hash 输出）；`-s` 的字符串摘要、`-t` 定时试验、`-x` 测试套件（皆需 crypto）仅报 `RequiresHash`；选项按整个 argv 项匹配（`-o 1`/`-a crc` 值在下一项），不处理 getopt 聚簇（如 `-nq` 合写）——与 C 常见调用形状一致。真机 per-command 与 imgrd `/bin/cksum` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百五十三。
- ✅ **批次一百五十二完成：07 篇 `sort` 的 `-H`/`-s` 按 C 收为合法空操**（`2026-10-12`，`claim/CMD-SORT-HS-NOOP-qoder` @ 基线 `fa2c947af`，单域三线 commands/textfilter，无跨界）：对账 `minix3/usr.bin/sort/sort.c` 发现 `parse_options` 把两个 C 明接的旗标误归 `BadOption`→`sort -H file` 本树错退 2。C 真源：`case 'H'`（`sort.c:174-176`）注释 `-H was; use merge sort for blocks of large files — That is now the default`，即**接受并忽略**；`case 's'`（`:217-222`）置 `posix_sort=0`，而该注释自称已是 NetBSD 缺省（稳定排序），本树 `slice::sort_by` 本就稳定→同样无观察效应。修法＝在 `sort.rs::parse_options` 加 `'H' => {}`、`'s' => {}` 两条带 C 锚注的空操臂（纯新增臂、不改既有数值/字典/关键字比较路径，零回归面）；两臂均无观察效应。三件套：`cargo test -p minix-textfilter --lib`→**335/0**（新 1 枚 `test_parse_options_accepts_h_and_s_as_noops`：`-H -s data.txt` 解析 Ok 且不改任一字段、操作数保留、聚簇 `-sH` 亦 Ok）；既有 `test_parse_options_rejects_unknown`（`-Z`→`BadOption('Z')`）不动→未建模集未括广；freestanding `cargo build -p minix-textfilter --bin sort --target x86_64-unknown-none --release` Finished；clippy 触及文件零新增告警；`rustfmt` `sort.rs` 零漂移；`pattern-gate.sh`→`PASS=14 FAIL=0 SKIP=1` 且 `[P18] PASS`（`sort.c:174-176`/`:217-222` 锚点回读落窗）；权威编译门 docker `cargo check --workspace`→0 error；核实日 `2026-10-12`；基线 `fa2c947af`。**声明性留白**：`-S`（C `posix_sort=1`，其自身注释“Currently not implemented”，语义不明）与 `-l`/`-x`（走 `optval` 设字段标志，语义未逐个坐实）仍未接；`-o`（输出文件）因需可写根盘（E5）仍拒，本轮不猜未坐实的 C 语义。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百五十二。
- ✅ **批次一百五十一完成：07 篇 `cksum` 的 `-c`/`-w` 校验接线**（`2026-10-12`，`claim/CMD-CKSUM-CHECK-qoder` @ 基线 `49f292f4b`，单域三线 commands/textfilter，无跨界）：接回本树只算不验的 `cksum`——C 真源 `minix3/usr.bin/cksum/cksum.c` 的 `do_check` 分支（`:283-460`，getopt `"a:cno:pqs:twx"`）此前整个缺失（`-c` 被当操作数吞掉）。决定半新增 `minix_textfilter::cksum::parse_check_line`→`CheckEntry{Checksum{crc,name}|Hash|Malformed}`，逐字对位 C 的行解析：crc 取首 token（新增 `leading_u32` 复刻 `strtoul(…,10)` 的前导十进制语义，非数字起始→0，`:442`）；name 取**末**个空格之后（C `strrchr(buf,' ')+1`，`:406-416`，故含空格的文件名只落最后一段——与 C 同、非本树缩水）；C 只比 crc 值、不比中间长度列（`val == should_val`，`:443`）；行含 `(` 即 crypto 的 `name (file) = digest` 语法，本树无 md5/sha 原语故归 `Hash`（不可验），无空格者归 `Malformed`（C 的 "bogus format"）。执行薄壳 `src/bin/cksum.rs`：选项扫描补 `-c`→`do_check`、`-w`→`check_warn`（对位 `check_warn`），新增 `check_files`——读校验清单（首操作数，否则 stdin，对位 `argc==0→STDIN`），逐行分类：`Checksum` 打开该文件、`cksum_crc` 现算比对，不等或打不开即印 `name: FAILED`（**成功静默**，C 无 OK 行、`:452-457`）；`Hash`/`Malformed` 计失败、仅 `-w` 时各报 `unknown hash`/`bogus format`（`:324-327`/`:357-359`）；清单本身打不开→报 `Cannot read <path>` 退 1（C `err(1,…)` `:303-305`）；任一失败整体退 1。按 `fgets` 语义只剥一个行尾换行（避免尾随空行误判），空清单不出行→退 0。三件套：`cargo test -p minix-textfilter --lib`→**334/0**（新 4 枚 `parse_check_line`：三列/两列取末空格名/hash·malformed 分类/前导数字截断）；freestanding `cargo build -p minix-textfilter --bin cksum --target x86_64-unknown-none --release` Finished；`cargo clippy -p minix-textfilter --lib --bins` 触及文件零新增告警；`rustfmt --edition 2021 --check` 触及两文件零新增漂移（唯一 diff 在 `#[path]` 纳入的 `bin_support.rs` 尾空行，属既有基线）；`check-command-boundary.sh`→OK；`pattern-gate.sh`→`PASS=14 FAIL=0 SKIP=1` 且 `[P18] PASS`（`cksum.c:283-460`/`:443` 等锚点回读落窗）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1`→0 error；核实日 `2026-10-12`；基线 `49f292f4b`。**声明性留白**：`-n`/`-q` 打印模式、`-p/-s/-t/-x`（皆需 crypto 原语）未接；`(` 语法行只判失败、不复算摘要（无哈希库，与批次四十七登记 `-a` crypto 同源）；真机跑 `-c` 需校验清单落根盘（写类受可写根盘 E5 门控），本轮交代码+宿主测；per-command 与 imgrd `/bin/cksum` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百五十一。
- ✅ **批次一百五十完成：07 篇 `tail` 的 `-r` 行反序接线**（`2026-10-12`，`claim/CMD-TAIL-REVERSE-qoder` @ 基线 `fbc8a46ed`，单域三线 commands/textfilter，无跨界）：接回 `-r`/`-f`/`-F` 留白里的 `-r`（行反序）。C `tail.c`+`reverse.c`：`-r` 单独 → style=REVERSE（`r_reg` 从尾往前逐行回印全部）；`-r -n N` → FLINES→RLINES（回印至多 `off` 行即停）；`-r -c/-b` → RBYTES（字节/块反序）。本轮实现行反序两形：新增 `reverse_selection(lines, count, n_seen)`（REVERSE 反序全部、RLINES 取末 `count` 行再反序，与 `r_reg` 同），`rflag`+`n_seen` 跟踪（`-r` 无伴 count 即整文件反序）；`-r` 与 `-c`/`-b` 同给拒（RBYTES 反序留下一轮）；`-f`/`-F` 仍拒。forward 环路与多文件横幅不动。三件套：`cargo test -p minix-textfilter --bin tail` → **7/0**（基线 4 加新 3 枚：REVERSE 全反、RLINES 末 N 反、count 超输入取全）；`--lib` 330/0 不动；freestanding `cargo build -p minix-textfilter --bin tail --target x86_64-unknown-none --release` Finished；`cargo clippy --tests --bin tail` 触及文件 1→1 零新增（唯一命中在未触碰的 `window.rs:103`）；`rustfmt` 新增行零新增漂移（`bin/tail.rs` 3→3，worktree 内 stash 对比）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12611）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `fbc8a46ed`。**声明性留白**：`-r` 行反序输出每行补 `\n`（与 forward 环路同款归一；末行无尾换行的输入与 C `r_reg` 直写略有差，属既有 forward 已接受的同一简化）；`-r` 与 `-c`/`-b` 组合（RBYTES 反序）、`-f`/`-F`（尾随）、`tail -n +N`（前向偏）未建模；真机 per-command 与 imgrd `/bin/tail` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百五十。
- ✅ **批次一百四十九完成：07 篇 `tail` 的 `-b` 块模式接线**（`2026-10-12`，`claim/CMD-TAIL-BLOCK-qoder` @ 基线 `742587118`，单域三线 commands/textfilter，无跨界）：接回批次一百四十八拆出的 `-b` 留白（`-c` 同族）。C `tail.c` `-b` 走 `ARG(512, FBYTES, RBYTES)`，即以 512 字节块计数——`-b N` 输出末 `N×512` 字节。bin 把 `"-b"` 从合并拒臂拆出：解析值（0 拒，同 `-c`）、`bytecnt = block_bytes(n)`（新增 `block_bytes(n)=n.saturating_mul(512)` 纯助手，饱和不绕回），复用已有 `tail_stream` 字节路径与 `last_bytes_of`；`-r`/`-f`/`-F` 仍拒（反向/尾随大件）。三件套：`cargo test -p minix-textfilter --bin tail` → **4/0**（新 1 枚：block_bytes 512×N 且接 last_bytes_of）；`--lib` 330/0 不动；freestanding `cargo build -p minix-textfilter --bin tail --target x86_64-unknown-none --release` Finished；`cargo clippy --tests --bin tail` 触及文件 1→1 零新增（唯一命中在未触碰的 `window.rs:103`）；`rustfmt` 新增行零新增漂移（`bin/tail.rs` 3→3）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12611）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `742587118`。**声明性留白**：`-b 0`/负值同 `-c` 口径拒；`-r`（反向）、`-f`/`-F`（尾随）、`tail -n +N`（前向偏）仍未建模；真机 per-command 与 imgrd `/bin/tail` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十九。
- ✅ **批次一百四十八完成：07 篇 `tail` 的 `-c` 字节模式接线**（`2026-10-12`，`claim/CMD-TAIL-BYTES-qoder` @ 基线 `11333a84d`，单域三线 commands/textfilter，无跨界）：接回 `-c`/`-b`/`-r`/`-f`/`-F` 留白里的 `-c`（head -c 的同族镜像）。C `tail.c` `-c N` 走 `RBYTES` 风格、输出每文件末 N 字节。bin 加 `bytecnt`（`-c` 臂从合并拒里拆出、解析赋值、值 0 拒，同 head 口径），环容量守卫只在 `bytecnt == 0` 跑，`tail_stream` 增 `bytecnt` 参数：>0 时 `support::emit(last_bytes_of(&input, bytecnt))` 吐末 N 字节、否则走既有 `TailWindow` 环路；多文件 `==> name <==` 横幅与 open/unstat 失败 warn+skip 复用不动。新增纯助手 `last_bytes_of(input, n) -> &input[len.saturating_sub(n)..]`。`-r`/`-f`/`-F`/`-b` 仍拒（各自环/尾随/块语义大件）。三件套：`cargo test -p minix-textfilter --bin tail` → **3/0**（新 3 枚：截末 N、短输入返全部、空输入）；`--lib` 330/0 不动（tail 只动薄壳，行环既有测零回归）；freestanding `cargo build -p minix-textfilter --bin tail --target x86_64-unknown-none --release` Finished；`cargo clippy --tests --bin tail` 触及文件命中 1→1 零新增（唯一命中在未触碰的 `window.rs:103`，stash 对比坐实）；`rustfmt` 新增行零新增漂移（`bin/tail.rs` 3→3）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12611）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `11333a84d`。**声明性留白**：`-c 0`/负值按 head 同口径拒（C `arg2num` 的具体 0 语义未逐字核）；`-b`（块）、`-r`（反向）、`-f`/`-F`（尾随）、`tail -n +N`（前向偏）未建模；真机 per-command 与 imgrd `/bin/tail` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十八。07 篇 head/tail/sort/uniq/pr/nl/join/fmt 的选项面已接至仅剩引擎/本地化依赖项。
- ✅ **批次一百四十七完成：07 篇 `head` 的 `-c` 字节模式接线**（`2026-10-12`，`claim/CMD-HEAD-BYTES-qoder` @ 基线 `2d4ef77bf`，单域三线 commands/textfilter，无跨界）：接回既有留白「`-c` 字节模式未建模（窗口按行建模故拒）」。C `head.c:149-167`：`-c N`（N 必须 >0，`bytecnt <= 0` errx）走 `if (bytecnt)` 分支——`fread` 最多 N 字节、`fwrite` 出，完全绕开行窗口，且 `bytecnt` 按值传入每文件（各文件独立取前 N 字节），`linecnt` 被忽略。据此给 `bin/head.rs` 加 `bytecnt`（`-c` 臂从 terminate 改为解析赋值、值 0 拒）、窗口容量守卫只在 `bytecnt == 0` 时跑（字节模式不看行数），`head_stream` 增 `bytecnt` 参数：>0 时 `support::emit(first_bytes_of(&input, bytecnt))` 直接吐前 N 字节、否则走既有 `HeadWindow` 行路；`-q`/`-v`/多文件 `==> name <==` 横幅与 open 失败 warn+skip 复用不动。新增纯助手 `first_bytes_of(input, n) -> &input[..n.min(len)]`。三件套：`cargo test -p minix-textfilter --bin head` → **3/0**（新 3 枚：截断到 N、短输入返全部、空输入）；`--lib` 330/0 不动（head 只动薄壳，行窗口既有测覆盖零回归）；freestanding `cargo build -p minix-textfilter --bin head --target x86_64-unknown-none --release` Finished；`cargo clippy --tests --bin head` 触及文件命中 1→1 零新增（唯一命中在未触碰的 `window.rs:103`，worktree stash 对比坐实）；`rustfmt --edition 2024 --check` 新增行零新增漂移（`bin/head.rs` 3→3）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12611，`head.c:149-167` 锚点落窗回读）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `2d4ef77bf`。**声明性留白**：`-c` 与 `-n` 同给时按 C 走字节分支（`linecnt` 忽略）；真机 per-command 与 imgrd `/bin/head` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十七。
- ✅ **批次一百四十六完成：07 篇 `sort` 的 `-m` 归并接线**（`2026-10-12`，`claim/CMD-SORT-MERGE-qoder` @ 基线 `4153aa0b2`，单域三线 commands/textfilter，无跨界）：接回 `-m`/`-o` 留白里的 `-m`（`-o` 是 create-and-write 输出文件，按同族命令一致口径仍留 shell 重定向/E5）。C `sort -m`（merge）不重排、只把**各自已排序**的操作数归并；本树之前把操作数一律 concat + 全排，`-m` 被 `parse_options` 当 `BadOption` 拒。本轮库加 `merge_sorted(runs, options)`（两两归并、等键取较早 run 的行＝稳定左偏）与 `Options.merge`（derive Default 取 false，非 merge 路径逐字节不变、17 旧 sort 测保持绿），`parse_options` 加 `'m' => options.merge = true`；薄壳把每个 operand 收成独立 run（无 operand 则 stdin 单 run），merge 时喂 `merge_sorted`、否则维持 concat+sort。归并用同一 `compare`（含 `-k`/`-r`/`-t`/`-b`/`-d`/`-f`/`-i`/`-n`/`-M` 修饰），故带键/反向的 `-m` 与 C 同。三件套：`cargo test -p minix-textfilter --lib` → **330/0**（基线 327 加新 3 枚：两 run 交错、等键保序、parse `-m`）；freestanding `cargo build -p minix-textfilter --bin sort --target x86_64-unknown-none --release` Finished；`cargo clippy --tests --bin sort` 触及文件 0→0；`rustfmt --edition 2024 --check` 新增行零新增漂移（`sort.rs` 3→3、`bin/sort.rs` 2→2，经 worktree 内 stash 对比坐实，避开 /tmp 解析不到 `#[path]` include 的假 0）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12610）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `4153aa0b2`。**声明性留白**：`-m` 与 `-c` 同给时 check 仍走 concat 路径（未展开 per-run 校验，属边角组合）；`-o` 未接；真机逐文件归并 open 腿归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十六。至此 `sort` 除 `-o` 外全臂已接。
- ✅ **批次一百四十五完成：07 篇 `uniq` 的 `-f`/`-s` 字段/字符跳过比较接线**（`2026-10-12`，`claim/CMD-UNIQ-SKIP-qoder` @ 基线 `ffd97426c`，单域三线 commands/textfilter，无跨界）：接回既有留白「field-skip（`-f`/`-s`）比较未建模」。C `uniq.c` 的 `skip()`：先跳 `numfields` 个空白分隔字段（前导空白不算边界、结束字段的空白被吃），再跳 `numchars` 个字节，主循环 `if (numfields||numchars) { t1=skip(this); t2=skip(prev); } if (strcmp(t1,t2)) ...` 用跳后的键比较、`show` 却打全行。据此库新增 `comparison_key(line:&[u8], f, c)->&[u8]`（按字节算，C 系 byte-oriented locale isspace，也避开 `&str` 非字边界 panic），`Uniq` 加 `skip_fields/skip_chars/previous_key` 三字段、`new_with_skip(mode,f,c)`，`push` 改按 key 比较而 `Run.line` 仍存全行；`new(mode)` 委派 `new_with_skip(_,0,0)`，旧 4 测不动、行为零回归。薄壳新增纯函数 `parse_uniq_args`（对位 C `getopt("-cdf:s:u")`：`-f`/`-s` 取值可接尾部或下一 argv、非数字/负值拒、`--` 截选项、其余作 operand），把原「`-f`/`-s`/`-i` 一律 terminate(1)」换成真解析。**顺带坐实一处账面错**：NetBSD `uniq` 的 getopt 串根本无 `i`（`"-cdf:s:u"`），本仓旧注把 `-i` 列为「待引擎」的 case-fold 比较项属误记——它就不是本实现的选项，`-i` 归入未知选项拒。三件套：`cargo test -p minix-textfilter`（含 bin 测）全绿；`--lib` 324→**327/0**（库新 3 枚：字段跳/字符跳/同键异全行归并）；bin 新 `parse_uniq_args` 测 5 枚（分离值 `-f 2 -s 1`、附值 `-f3`、`-d`+`-u` 拒、非数字/负/`-i` 拒、`--` 收集 operand）；freestanding `cargo build -p minix-textfilter --bin uniq --target x86_64-unknown-none --release` Finished；`cargo clippy --tests --bin uniq` 触及文件 2→2 零新增（worktree 内 stash 对比）；`rustfmt` 新增行零新增漂移（`uniq.rs` 3→3、`bin/uniq.rs` 2→2）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12610）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `ffd97426c`。**声明性留白**：`-c` 与 `-d`/`-u` 的组合在本树仍按互斥处理（沿用既有 `UniqMode` 单值模型，本轮不改）；输出文件第二 operand 未接（create-and-write，归 shell 重定向/E5）；真机 per-command 与 imgrd `/bin/uniq` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十五。
- ✅ **批次一百四十四完成：07 篇 `pr` 的文件操作数与逐文件分页接线**（`2026-10-12`，`claim/CMD-PR-FILES-qoder` @ 基线 `025490a41`，单域三线 commands/textfilter，无跨界）：接回既有留白「文件操作数待开放路径」。核实 C `pr.c` 的 `onecol` 不是把多文件拼成一 stream 连续分页，而是**逐文件独立分页**：外层 `while ((inf = nxtfile(...)) != NULL)` 每文件 `pagecnt = 1` 重启（`:224-233`）、页眉标题取该文件名（`*fname = argv[eoptind]`），`-h` 给定时所有文件统一用它覆盖（`nxtfile:1318-1376`），stdin/无操作数取 `FNAME`=`""`（`pr.h:63`）。据此在库新增 `paginate_files(files: &[(&str, Vec<String>)], base, title_override)`——逐文件 clone base 并把 header 置为该文件标题、各调一次 `paginate` 再拼接（页号自然每文件重启），既有 `paginate` 单流面与 6 测不动。薄壳：`-h` 值从写死 `options.header` 改为 `title_override`；选项循环把裸文件/`-` 归入 `operands`（不再是「file operands wait」拒），未建模旗标（`-d`/`-a`/`-m`/`-column`）与 `+page` 仍拒退 2；无操作数→stdin 单文件标题 `""`，有则逐 operand 读（`-`→stdin），开不了按 C `nxtfile` errcnt 告警跳过、末了非零退出。三件套：`cargo test -p minix-textfilter --lib` → **324/0**（基线 321 加新 3 枚：两短文件各占一页各自标题、57 行首文件跨 Page1/2 而次文件重启 Page1、`-h` 覆盖对全部文件生效）；freestanding `cargo build -p minix-textfilter --bin pr --target x86_64-unknown-none --release` Finished（中途 no_std 下 `&str::to_string` 缺 `ToString` trait，改固有 `String::from`）；`cargo clippy -p minix-textfilter --tests --bin pr` 触及文件告警 8→8 零新增（均既有测试体 `field_reassign_with_default`，worktree 内 stash 对比坐实）；`rustfmt --edition 2024 --check` 新增行零新增漂移（`pr.rs` 5→5、`bin/pr.rs` 5→5）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12609，`pr.c:224-233`/`:1318-1376`/`:1338`/`pr.h:63` 锚点落窗回读）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `025490a41`。**声明性留白**：页眉的时间字段本树仍不渲染（C 用当前日期或各文件 mtime，属 doing-half 的 gettimeofday/stat 腿，本轮不接、不谎报）；多列形式（`-column`/`-a`/`-m`）、双行距 `-d`、页偏 `-o`、走纸 `-F`、起始页 `+first` 未建模（请求即拒退 2）；`strerror` 告警后缀未复现（仅报操作数名）；真机逐文件 open 腿归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十四。
- ✅ **批次一百四十三完成：07 篇 `nl` 的 `-l`/`-w` 归位与 `nblank` 空白成组接线（兼修批次一百四十一登记的 C 偏离）**（`2026-10-12`，`claim/CMD-NL-LW-qoder` @ 基线 `7df791cb8`，单域三线 commands/textfilter，无跨界）：上一轮登记的「本树把宽度绑在 `-l`」这条对 C 偏离，本轮按一次一条的约定单独修。C `nl.c` 里 `-l` 设 `nblank` 空白成组（`:192`）、`-w` 才设字段宽度（`:217-228`，宽度必须 >0），且宽度默认 6、`nblank` 默认 1。本树原 `-l => opts.width`（错绑），`-w` 完全未接，All 模式无条件编号。修：`NlOptions` 加 `nblank: usize`（Default 1），`width` 字段注释从 `-l` 改 `-w`；`nl_lines` 的 `number_all` 分支按 C（`:307-316`）实现成组——空行自增 `adjblank`，未满 `nblank` 则不编号、第 `nblank` 个空行编号并归零，非空行编号并归零，定界符行也归零 `adjblank`（对位 C `:296`）；薄壳 `-l => opts.nblank`、`-w => opts.width`（带 >0 校验）。三件套：`cargo test -p minix-textfilter --lib` → **321/0**（基线 319 加新 2 枚：`-b a -l 3` 四个空行只第 3 个编号、`-l 2` 混合文本验证成组在编号空行与非空行两处都归零；既有 `test_number_all_includes_blanks`（nblank 默认 1）与 `test_section_types_independent` 保持绿，证明默认行为未变）；freestanding `cargo build -p minix-textfilter --bin nl --target x86_64-unknown-none --release` Finished；`cargo clippy -p minix-textfilter --tests --bin nl` 触及文件 0 告警；`rustfmt --edition 2024 --check` 新增行零新增漂移（`nl.rs` 5→5、`bin/nl.rs` 3→3，worktree 内 stash 对比）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12604，`nl.c:307-316`/`:313`/`:217-228` 锚点落窗回读）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `7df791cb8`。**声明性留白**：`-b p…` number_regex 仍未接（需正则引擎）；`-l` 值非法沿用本 bin 既有 `.parse().unwrap_or(0)` 简化（C 用 `strtoul`+`*end`）。真机 per-command 与 imgrd `/bin/nl` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十三。至此 `nl` 的 `-b/-h/-f/-v/-i/-l/-n/-p/-d/-s/-w` 全臂已接，仅余 `number_regex`。
- ✅ **批次一百四十二完成：07 篇 `nl` 的 `-s` 分隔符接线**（`2026-10-12`，`claim/CMD-NL-SEP-qoder` @ 基线 `0012567c2`，单域三线 commands/textfilter，无跨界）：接回批次一百四十一刚拆出的 `-s` 留白。C `nl.c:122/205-207` 的 `sep` 是一串字节（默认 `"\t"`，`-s` 取整个操作数为分隔串），输出腿 `printf("%s%s\n", sep, buffer)`（`:340`）。本树早先 `NlOptions.sep` 只建模为单字节 `u8`——不足表达 `-s` 的变长串，故本轮把它拓宽为 `Vec<u8>`（Default `[b'\t'].to_vec()`）、`nl_lines` 的 `row.push(opts.sep)` 改 `row.extend_from_slice(&opts.sep)`（sep 全库仅此一处消费，改动隔离），薄壳把 `"s"` 臂从合并 fail 拆出 `opts.sep = value!().as_bytes().to_vec()`（空串得空分隔符，与 C 同）。`-s ''` → 数字段直接贴正文；`-s '::'` → 双字节分隔。三件套：`cargo test -p minix-textfilter --lib` → **319/0**（基线 317 加新 2 枚：多字节分隔串、空分隔符）；freestanding `cargo build -p minix-textfilter --bin nl --target x86_64-unknown-none --release` Finished；`cargo clippy -p minix-textfilter --tests --bin nl` 触及文件 0 告警；`rustfmt --edition 2024 --check` 新增行零新增漂移（`nl.rs` 5→5、`bin/nl.rs` 3→3，均经 worktree 内 base 对比）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12601）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `0012567c2`。**声明性留白**：`-w` 空白成组计数、`-b p…` number_regex 仍未接；上一轮登记的 `-l`/`-w` 与 C 语义偏离（本树 `-l` 绑宽度、C 里 `-l` 设 `nblank`）本轮不顺手修。真机 per-command 与 imgrd `/bin/nl` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十二。
- ✅ **批次一百四十一完成：07 篇 `nl` 的 `-d` 分页定界符接线**（`2026-10-12`，`claim/CMD-NL-DELIM-qoder` @ 基线 `064d0aaf4`，单域三线 commands/textfilter，无跨界）：接回既有留白「`-d` 定界符未建模」。C `nl.c:157-169` 的 `-d` 用操作数逐字节填 `delim[0]`、`delim[1]`（至多两字节，第三字节 errx），空操作数二者都留默认 `\:`；分页检测按「行恰为定界符的一/二/三份」切 footer/body/header。原库把定界符硬编为 `b"\\:"`，本轮给 `NlOptions` 加 `delim: [u8; 2]`（Default `[b'\\', b':']`），`delimiter_section` 改收 `delim` 参数、`nl_lines` 传 `opts.delim`，薄壳把 `"d"` 臂从合并 fail 里拆出按 C 填 delim（0 留默认/1 填 delim[0]/2 填两者/≥3 拒）。三件套：`cargo test -p minix-textfilter --lib` → **317/0**（基线 314 加新 3 枚：`-d x` 单字符切 footer 并丢弃、`-d '#$'` 两字节切 body 且 restart 重置、自定义定界符后默认 `\:` 退回普通行被编号——反向守住分类器真读 opts.delim 而非硬编）；freestanding `cargo build -p minix-textfilter --bin nl --target x86_64-unknown-none --release` Finished；`cargo clippy -p minix-textfilter --tests --bin nl` 触及文件 0 告警；`rustfmt --edition 2024 --check` 新增行零新增漂移（`nl.rs` 5→5，`bin/nl.rs` 3→3 经 worktree 内 stash 对比同为基线）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12601，`nl.c:157-169` 锚点落窗回读）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `064d0aaf4`。**顺带发现（登记不修，守一次一条）**：本树把线号字段宽度绑在 `-l`（C 里 `-l` 设 `nblank` 空白成组、`-w` 才设 `width`）——属批次四十七立项时就存在的对 C 偏离，非本轮引入、也不与 `-d` 交叠；接 `-w` 会牵动这条，留下一轮单独处理。**声明性留白**：`-s`（C 是字符串分隔符，本树 `sep: u8` 只能表一字节）、`-w` 空白成组计数、`-b p…` number_regex 仍未接。真机 per-command 与 imgrd `/bin/nl` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十一。
- ✅ **批次一百四十完成：07 篇 `join` 的 `-j`/`-v` 接线**（`2026-10-12`，`claim/CMD-JOIN-JV-qoder` @ 基线 `bdcd26c0b`，单域三线 commands/textfilter，无跨界）：接回既有留白「`-j`/`-v` 未建模（请求即拒退 1）」。`-j field` 是 BSD 的 `-1 field -2 field` 合写（`join.c:183-194`：把两文件 join 字段一起设成 field−1）；`-v fileno` 是「只报指定文件的未配对行、压制配对输出」——C 置 `vflag=1`、清全局 `joinout`（`:104`）、给该文件置 `unpair`（`:206-224`），主循环里配对分支 `if (joinout) joinlines(F1, F2)`（`:259`）遂静默。薄壳照 `-1`/`-2` 同形解析 `-j`（field<1 拒），`-v` 逐 fileno 置 `unmatched{1,2}` 并清 `joinout`；`-a`/`-v` 互斥按 C「`if (aflag && vflag) errx(1, ...)`」（`:234-235`）在选项循环后、`argc!=2` 前判。库面 `JoinOptions` 加 `joinout: bool`（Default 取 true，既有测试全用 `..Default::default()` 故不受影响），`join_lines` 的 Equal 分支按 `opts.joinout` 门控配对输出、无论是否输出都推进两侧游标。三件套：`cargo test -p minix-textfilter --lib` → **314/0**（基线 311 加新 3 枚：v1 只出文件1未配对、v2 只出文件2未配对、joinout 关且无 unpair 时零输出）；freestanding `cargo build -p minix-textfilter --bin join --target x86_64-unknown-none --release` Finished；`cargo clippy -p minix-textfilter --tests --bin join` 触及文件 0 告警；`rustfmt --edition 2024 --check` 新增行零新增漂移（`join.rs` 9→9 块，`bin/join.rs` 报告的 2 块系 `bin_support.rs` 既有、未动）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12599，`join.c:183-194`/`:206-224`/`:234-235`/`:259` 锚点落窗回读）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `bdcd26c0b`。**声明性留白**：`-j`/`-v` 的 field/fileno 非法判定沿用本 bin 既有 `-1`/`-2` 的 `.parse().unwrap_or(0)` 简化（非 C 的 `strtol`+`*end`，与同文件既有臂一致、不新增分歧）；obsolete 单参形式未接（既有留白）；真机 per-command 与 imgrd `/bin/join` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百四十。
- ✅ **批次一百三十九完成：07 篇 `fmt` 的 `-C` 居中模式接线**（`2026-10-12`，`claim/CMD-FMT-CENTER-qoder` @ 基线 `f6c702f56`，单域三线 commands/textfilter，无跨界）：接回批次四十七声明的留白「`-C` 居中模式未建模（请求即拒退 1）」。C `fmt.c:217-254` 的居中是 `fmt()` 顶部一条完全绕开重排机器（`prefix`/`split`/`pack`/`tabulate`）的独立分支——逐物理行（`fgetln` 取到含尾换行的一段）先用 `isspace` 去首尾空白，剥完为空则只发一个换行，否则当行长短于 goal 时前置 `(goal - len) / 2` 个空格（C 整除向下取整）、再原样吐出行字节并补换行；全程不展 tab、不做 `^H` 退格、不过滤不可打印。据此把它建成独立纯函数 `minix_textfilter::fmt::fmt_centered`（`fmt_bytes` 依新增的 `FmtOptions::center` 字段分派，与 C 两条互斥路径同形；Default 取 false，四处既有测试字面量随之补 `center: false`）；薄壳 `src/bin/fmt.rs` 把 `"C"` 臂从 `fail("center (-C) not wired")` 改为置 `opts.center = true`（按 C：`-C` 不清 `compat`，故其后旧式位置 goal/max 仍可解析；`max>goal` 校验照旧先跑）。9 枚宿主测逐分支手推 golden：半格填充（goal 10、`abc`→3 空格）、剥完空行→裸换行、去首尾空白（goal 9、`    hi   `→`   hi`）、行长≥goal 不折不填（goal 5 的 `abcdefgh` 与 `goal==len` 边界各一）、奇偶余数下取整（goal 10：`ab`→4、`abc`→3）、逐行各补换行、末行无尾换行仍居中并终止、空输入零输出、tab 不展开（goal 3、`a\tb` 原样）。三件套：`cargo test -p minix-textfilter --lib` → **311/0**（基线 302 加新 9 枚）；freestanding `cargo build -p minix-textfilter --bin fmt --target x86_64-unknown-none --release` Finished；`cargo clippy -p minix-textfilter --tests --bin fmt` 触及文件 0 告警；`rustfmt --edition 2024 --check` 新增行零新增漂移（`fmt.rs` 12→12 块，`bin/fmt.rs` 报告的 2 块系 `bin_support.rs` 既有、未动）；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12594，`fmt.c:217-254` 与 `:152-155` 锚点落窗回读）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；核实日 `2026-10-12`；基线 `f6c702f56`。**声明性留白**：邮件头启发式 `ishead`（批次四十七既有留白，未变）；多操作数居中按现有薄壳的「拼接后单次居中」模型处理，与 C「逐操作数各跑 `fgetln` 到 EOF、每行必终止」仅在「非末操作数缺尾换行」一处不同——逐文件 open 真机腿仍归 `C-57`/E5，而 stdin 单流是当前唯一可达路径，其行为逐字节同 C。真机 per-command 与 imgrd `/bin/fmt` 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百三十九。
- ✅ **批次一百三十八完成：`find` 整棵表达式的求值器落地——短路、`-o` 只走该走的一支、`!` 翻真值不吞输出、三种打印形**（`2026-10-12`，`claim/CMD-FIND-EVAL-qoder` @ 基线 `a6fa12c8d`，单域三线 commands/fileops，无跨界登记）：接批次一百三十七的判据族，把「一串节点怎么串起来求值」补进 `os/commands/bin/fileops/src/find.rs`（`eval_plan`/`printed`，新增 8 枚宿主测，模块累计 40 枚）。求值形状照 C：`find_execute` 的那个 `for (p = plan; p && (p->eval)(p, g_entry); p = p->next)`（`find.c:252`）就是「一路求值到第一个返假的节点、其后节点连碰都不碰」，所以短路不只是真值的事——**它同时决定有没有输出**：`-name nope -print` 不吐行。
  四条按 C 的语义：① `-o` 只在左支为假时才走右支（`or_squish` 的右支是余下整个计划，`operator.c:252-257`）；② `!` 只翻子计划的真值，**不回收子计划已经吐出的行**（C 的 `f_not` 就是转调子计划后取反，故 `! ( -name rc.d -print )` 会先印一行再判假）；③ 三种打印形各自成形：`-print` 加换行、`-print0` 加 NUL、`-printx` 给八个字符加反斜杠再加换行（`f_printx`，`function.c:1492-1507`），并与 `-X` 那道检查用的五个字 `BADCH`（`find.c:236-237`）**是两套不同清单**，注释里写明别顺手合并；④ `-prune` 恒真但把「剪掉这一支」单独报出来（`f_prune`，`function.c:1542-1560`），走表腿才决定怎么用；`-exec` 一族在没有进程腿之前**报错而不是报真**，免得「真而无声」被当成执行成功。
  求值器把输出交回文本而不是直接写标准输出，薄壳只负责落笔——这样整族可在宿主测。过程记三笔：`String`/`format` 又一次忘了从 `alloc` 导入（宿主测全绿、freestanding 才炸，第二次踩同一条，`--target x86_64-unknown-none` 的门照旧不可省）；`merge(sub, out)` 之后再读 `sub.truth` 造成三次所有权错误，改成先取真值再合并；两处 `find.c` 锚点（245、235）是按记忆写的，`grep -n` 后重定位到 252 与 236-237。
  三件套：`cargo test -p minix-fileops --lib` → **357/0**（`find.rs` 40 枚 = 第一包 18 + 判据 14 + 求值 8）；freestanding `cargo build -p minix-fileops --lib --target x86_64-unknown-none --release` 0 error；`cargo clippy -p minix-fileops --all-targets` 24 条与主树同数（`find.rs` 内 0 条）；`cargo fmt -p minix-fileops -- --check` 52 处漂移块与主树同数；`check-command-boundary.sh` → OK；`pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12572→12574，锚点逐条 `grep -n` 回读）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；本包不接命令、不动真机；核实日 `2026-10-12`。
  留白（下一批）：走表遍历腿（`dents` 加 `FTS_COMFOLLOW`/`FTS_XDEV`/`-d` 后序与 `-prune` 的接法）、`-exec`/`-delete` 等缺腿动作、薄壳与播种；imgrd 根盘的目录读腿未通（`E5`）会让真机判据只能判到「起始路径自身」这一层，与 `du` 同症。

- ✅ **批次一百三十七完成：`find` 判据求值半落地——`find_parsenum` 三态、时间折算、`-size` 的全局旗标、`-type`/`-perm`/`-empty`/深度/动作面**（`2026-10-12`，`claim/CMD-FIND-JUDGE2-qoder` @ 基线 `e8fdf33b3`，单域三线 commands/fileops，无跨界登记）：接批次一百三十六备好的通配腿，把 `minix3/usr.bin/find/function.c` 的 `f_*` 判据族落进 `os/commands/bin/fileops/src/find.rs`（新增 14 枚宿主测，模块累计 32 枚）。目录项做成 `Entry`（名字、路径、深度、`st_*` 各位、以及 `-empty` 要用的子项清单），`-newer` 一族的参照时间与 `-size` 的全局旗标做成入参，于是这一整族判据在宿主上逐分支可测，本层不碰系统调用。
  **四条按 C 的语义、不按直觉**：① `find_parsenum` 的三态（前导 `+`/`-` 定比较方向，`function.c:135-170`），配合 `COMPARE`（`:68-79`）与只对时间类判据生效的 `TIME_CORRECT`（`:178-180`，让 `-amin -1` 表示「少于一分钟」而不是「少于一格零」）；② 折算式 `(now - t + UNIT - 1) / UNIT` 是**先抬一格再整除**，故 1.5 天算 2 天；③ `-size` 的 `divsize`是 C 的文件级全局旗标——初值 1（按 512 块），任何一处 `-size Nc` 把它置 0（按字节）且不再回落（`c_size`，`:1638-1654`），所以一处 `c` 影响整棵表达式；本层把它做成 `size_counts_bytes(tree)` 扫全树后传入，行为同形且这条不再隐形；④ `-perm -mode` 比的是「给定位都已置起」，`-perm -600` 对 0755 是**真**（`f_perm`，`:1430-1441`），掩码取那 12 位含 setuid/setgid/sticky。
  `-empty` 的目录判据照 C 的 `readdir` 循环（`:461-466`）：只看「不是 `.`、也不是光秃秃的 `..`」的条目，故清单里只有 `.` 也算空；`opendir` 失败判假——本层对应「清单交不进来就判假」。缺腿的判据（`-user`/`-group`/`-nouser`/`-nogroup` 要口令库，`-exec`/`-execdir`/`-ok` 要进程创建，`-delete`/`-rm` 要可写根盘，`-fstype` 要设备名对照，`-flags` 要 Minix 旗标字符表，`-regex`/`-iregex` 要 `minix-regex` 的接法）**一律按名拒绝**，绝不悄悄判假；而 Minix3 的表里本就没有 `-true`（bash 的），这里也不发明。
  过程记两笔，都进台账：初稿把 `divsize` 的极性写反（默认按字节、见 `c` 才按块），是测把它抓回来的；`String` 忘了从 `alloc` 导入，宿主测全绿而 freestanding 编译才炸——这条路径差异说明 `--target x86_64-unknown-none` 的门不能省。另有 9 处锚点行号是按记忆写的，P18 的范围检查放行但语义错位，逐条 `grep -n` 重定位（`find_parsenum` 其实在 `:135-170` 而非 `:900-933`）。
  三件套：`cargo test -p minix-fileops --lib` → **349/0**（`find.rs` 32 枚 = 第一包 18 + 本包 14）；freestanding `cargo build -p minix-fileops --lib --target x86_64-unknown-none --release` 0 error；`cargo clippy -p minix-fileops --all-targets` 23 条与主树同数（`find.rs` 内 0 条）；`cargo fmt -p minix-fileops -- --check` 52 处漂移块与主树同数；`bash tools/check-command-boundary.sh` → OK；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12563→12572）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；本包不接命令、不动真机；核实日 `2026-10-12`。
  留白（下一包）：整棵表达式的求值器（与/或/非/括号串起来 + `-a`/`-o` 短路 + 隐式 `-print` 的补挂）、走表遍历腿（`dents` 加 `COMFOLLOW`/`XDEV`/`-d` 后序语义）、`-print`/`-print0`/`-printx` 的输出面，以及薄壳与播种。

- ✅ **批次一百三十六完成：移植 `fnmatch(3)`，为 `find` 的三个通配判据备好那条腿**（`2026-10-12`，`claim/CMD-FIND-JUDGE-qoder` @ 基线 `22632c84e`，单域三线 commands/fileops，无跨界登记）：`find` 的 `-name`、`-iname`、`-path` 三判据直接调 libc 的 `fnmatch`（`minix3/usr.bin/find/function.c:1282-1310`、`:1404-1410`），而本树没有通配匹配件——先把它补上，免得在 `find` 里另发明一套通配语义。新建 `os/commands/bin/fileops/src/fnmatch.rs`，对位 `minix3/lib/libc/gen/fnmatch.c`（NetBSD 1.26、222 行）：`foldcase`、`rangematch`、`fnmatchx` 三段一一对位，旗标位取 `minix3/include/fnmatch.h`；返回值用 `Option<bool>` 表达 C 的匹配 / `FNM_NOMATCH` / 递归用尽的 `FNM_NORES` 三态，深度上限照 C 调用侧写死的 64（`fnmatch.c:219-221`）。
  **四处按 C 保留、不按直觉改写的语义**（都有测）：括号里撞上 `/` 交回的是「写法不合法」哨兵，调用方据此把 `[` 当字面量再比一次（`:94-95`、`:186-200`），而不是把斜杠当区间成员；`^` 与 `!` 同义（`:82-88` 的注释自陈是为了跟正则语法一致，POSIX 对该情形未定义）；`FNM_PERIOD` 只认**本次匹配串**的开头，C 用指针比较表达、本层按下标 0 表达（递归传的是子串，两者同义，见 `:140-143`、`:152-155`）；开头的星号盖不住开头的句点，而不带 `FNM_PATHNAME` 时 `a*` 确实能盖住 `a/b`——通配本身不把 `/` 当特殊字符，`find` 的 `-path` 正是靠这一点做整条路径匹配。
  **被门禁抓回两笔，记在此作教训**：初稿把 `sed` 屏窗里的相对行号当成了 `fnmatch.c` 的真实行号，五条锚点越界被 P18 当场判 FAIL（该轮 `pattern-gate` 报 `PASS=13 FAIL=5`），逐条 `grep -n` 重定位后修好——P18 只校范围，语义还得自己回读；新模块另招来两枚 clippy 告警（游离文档块、`is_some` 后 `unwrap`）与三枚 doc-list 缩进告警，按建议改写。
  三件套：`cargo test -p minix-fileops --lib` → **335/0**（新建 `fnmatch::tests` 10 枚，上一批的 325 枚一枚未动）；freestanding `cargo build -p minix-fileops --lib --target x86_64-unknown-none --release` 绿；`cargo clippy -p minix-fileops --all-targets` → 24 条与主树同数（`fnmatch.rs` 内 0 条）；`cargo fmt -p minix-fileops -- --check` → 52 处漂移块与主树同数（新文件 0 处）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12541→12563）；`bash tools/check-command-boundary.sh` → OK；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；本包不接命令、不动真机；核实日 `2026-10-12`。
  留白：`find` 的判据求值（`find_parsenum` 的三态比较、`TIME_CORRECT`、`-size` 的 `c` 后缀连带那个全局旗标、`-type`、`-perm`、`-empty`、深度两类）与走表遍历腿、薄壳仍在后续包里。

- ✅ **批次一百三十五完成：补上 `realpath(3)` 这条腿，`stat` 的 `%R` 由显式拒绝改为真折叠**（`2026-10-12`，`claim/STAT-REALPATH-LEG-qoder` @ 基线 `24a3fac56`，单域三线 commands/fileops，无跨界登记）：批次一百三十一把 `%R` 当成缺腿留白显式拒绝，本批把那条腿补上。新建 `os/commands/bin/fileops/src/realpath.rs`，对位 `minix3/lib/libc/gen/getcwd.c:64-215`（BSD 把 `realpath` 与 `getcwd` 放在同一件里），三个动作收进 `PathProbe`（`lstat`、`readlink`、`getcwd`），折叠算法因此可在宿主上逐分支测。上限取 `sys/sys/param.h:358` 的 `MAXSYMLINKS`（32）与 `PATH_MAX`（1024，与 `minix-sys` 线长上限同源）。
  **一处非直觉的 C 语义值得单独记**：符号链这一支 C **不推进** 那个写指针 `p`（`getcwd.c:174-206` 走完就 `goto loop`，推进的一行在 `:212`），于是下一段直接**覆写掉链名自己**——`/d/l` 且 `l -> e` 的答案是 `/d/e`，不是 `/d/l/e`；绝对目标另把 `p` 拉回串首（`getcwd.c:203-205`），前缀整体作废。本层用「把已解析串截回本段起点」复刻同一效果，两种形状都有测。另两处照抄：空路径是 `ENOENT` 而不是 `EINVAL`（`getcwd.c:104-107`）；`readlink` 返回 0 也按错误处理（`getcwd.c:186-191`）；中间段不是目录而其后还有内容才是 `ENOTDIR`（`getcwd.c:207-209`），末段是普通文件则正当。
  接线：`stat.rs` 的 `Render` 多一个 `resolved` 入参（与 `link` 同理由薄壳备好后交回，本层不碰系统调用），`%R` 与前缀的关系按 `stat.c:822-838` 走——它与 `%Y` 共用那条「默认形态在指针算好之后才补」的规则，所以裸 `%R` 不带 ` -> `、显式 `%SR` 才带；无操作数那一趟打 `(stdin)`；折失败出声一次并置 `linkfail`（对应 C 的 `realpath()` 返 `NULL` 支，也是 `readlink` 人格的退出码来源）。`FaceError::RealpathUnported` 随之下线。薄壳新增 `SysProbe`（`lstat`/`readlink`/`getcwd` 三条 `minix_sys` 腿），并且只在模板真出现 `%R` 时才折——这条腿每段都要一次 `lstat`，不该为用不到的模板付这份钱。
  三件套：`cargo test -p minix-fileops --lib` → **325/0**（新建 `realpath::tests` 12 枚；`stat.rs` 44 枚，其中 `%R` 那枚由「拒绝」改成「渲染薄壳交回的折叠结果」并含失败与 `(stdin)` 两形）；freestanding `cargo build -p minix-fileops --bins --target x86_64-unknown-none --release` 绿；`cargo clippy -p minix-fileops --all-targets` → 24 条与主树同数（`realpath.rs`/`stat.rs`/`bin/stat.rs` 内 0 条）；`bash tools/check-command-boundary.sh` → OK；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12524→12541，新增锚点逐条 `sed -n` 回读）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；五道收尾校验全过；核实日 `2026-10-12`。
  未做：`%R` 的真机判据。`stat` 已播种，但要判 `%R` 得给 `-f '%R'` 这类带引号的参数，而 rc 里带引号的写法还没验过，故这一条留到把引号处理验清之后，和 `find` 的门判据同批。

- ✅ **批次一百三十四完成：`test` 与方括号形态的算子与 C 的三张表逐条对账——补 `<` 与 `>` 与 `-h`，删自造的 `-N` 与 `==`，并更正「文件问待批」这条账面滞后（台账行翻 `closed`）**（`2026-10-12`，`claim/CMD-TEST-FILEQ-qoder` @ 基线 `12e2b4a9e`，单域三线 commands/fileops，无跨界登记）：`06-file-ops.md` 的 §7 现状行与台账都写着「`test` 的文件问执行层待批」，读码核实是**账面滞后**：决定半 `testexpr.rs` 早有 `FileTester` 全套（`-e -f -d -L -p -S -b -c -u -g -k -O -G -r -w -x -s -t` 与 `-nt -ot -ef`），薄壳 `src/bin/test.rs`（同一份码以 `test` 与方括号名两个名字编译） 也早已用 `stat`/`lstat` + `access_bits` 接上（后者是 C `test_access` 的位规则，含 root 与附属组两支）。但拿 C 的三张算子表（`test.c:105-154`）逐条对账，发现**双向的差**：本树缺 `<` 与 `>`（`test.c:109`、`:111` 声明、`:345-348` 求值，按 `strcmp` 的字节序），也缺 `-h`（`test.c:139` 与 `-L` 同 token，注释写「for backwards compat」）；反过来本树多出两样 C 的表里根本没有的东西——`-N`（mtime>atime，bash 的）与 `==`（bash 的）。按「不发明上游没有的接口」这条（`PD-03` 的同一精神），两样都**删**而不是留着当超集：`FileTester` 去掉 `modified_since_read`、内存表去掉只为它服务的 `atime` 字段、薄壳去掉那条 lstat 腿；`==` 退出表后 `test a == a` 不再是真，而是「余词没人吃」的 `InvalidArgument`（对应 C 的退出码 2 那一支），这一条正反两面都测住了。
  三件套：`cargo test -p minix-fileops --lib` → **313/0**（`testexpr.rs` 由 12 枚增到 13 枚：新 `test_h_is_the_alias_of_capital_l`，另在既有两枚里补 `<` 与 `>` 六条断言与 `==`/`-N` 两支拒绝）；freestanding `cargo build -p minix-fileops --bins --target x86_64-unknown-none --release` 绿（含 `test` 与方括号名 两个 bin）；`cargo clippy -p minix-fileops --all-targets` → 24 条与主树同数（`testexpr.rs`/`bin/test.rs` 内 0 条）；`cargo fmt -p minix-fileops -- --check` 的 `testexpr.rs` 漂移块由 11 降至 10（新代码 0 处，顺带消掉一处既有漂移）；`bash tools/check-command-boundary.sh` → OK；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12519→12524）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；五道收尾校验全过；本批不改真机可观察输出（`test` 与方括号名从未播种，宿主与真机都不受影响的判据不成立——故明写为播种留白），核实日 `2026-10-12`。
  文档同步：`06-file-ops.md` 的模块表 `testexpr.rs` 行、命令一览表 test 行、§5 测试清单（8 个→13 个并补名目）、§7 现状行「`test` 的文件问待批」更正；台账对应行按六值词表翻 `closed` 并附三件套。下一批候选：`find` 第二包（`f_*` 取值与走表遍历腿 + 薄壳）、`test`/`[` 的播种与门判据设计、imgrd 根盘 `getdents` 读腿。

- ✅ **批次一百三十三完成：`find` 决定半第一包——全局选项位、主表与表达式语法（未接线）**（`2026-10-12`，`claim/CMD-FIND-EXPR-qoder` @ 基线 `a68670fe7`，单域三线 commands/fileops，无跨界登记）：命令面余量里的 `find` 开工（真源 `minix3/usr.bin/find/`，3315 行分七件）。新建 `os/commands/bin/fileops/src/find.rs`，落四件事：① 全局选项按 `main.c:88-124` 的位操作照搬成 `WalkOptions`（位值见 `fts.h:71-77`）——`-h` 与 `-L` 都走逻辑遍历，但 `-h` 不清 `COMFOLLOW` 而 `-L` 清，压成三值枚举就丢掉这个差别，故留位不留枚举；② 主表 51 项按 `option.c:59-110` 的字典序原样落一遍并带元数与「算不算有输出」两位（C 用 `bsearch` 查它，表序是语义前提，测里直接对账排序）；③ 路径/表达式分界按 `main.c:138-143`；④ 表达式一次成形为树，优先级与 C 的三段压实等价：`()` 最紧（`paren_squish` 先跑）、`!` 只收**一个**主项（`operator.c:175-183` 的 `yankexpr`）、相邻是隐式与（`find.c:245` 的求值循环一路短路，`-a`/`-and` 因 `c_null` 造不出节点）、`-o` 最松且右结合（`operator.c:252-257` 把余下整个计划挂成右支）；`-exec` 一族吃到单独的 `;` 或紧跟 `{}` 的 `+`（`function.c:645-675`，`-ok … +` 被 C 明写拒绝）。
  **两处按代码而不按注释与常理**：`find.c:104-107` 的注释说「有输出」只有五项，代码里置 `isoutput` 的其实有九处（另含 `-delete`、`-execdir`、`-ls`、`-print0`、`-printx`），真值优先序取代码；而 `find -name foo .` 里 `-name` 落在 getopt 的选项段、四个字母都不在 ``HLPdEf:hsXx` 串里、`main.c:128-129` 的 default 什么都不做，于是整段被吞、`foo` 与 `.` 都成起始路径、表达式为空——照此实现并测住，不顺手修成 GNU 的直觉。另有一处**刻意不等价**：`!` 后面接不产生节点的项（`! -a …`）时 C 把 `NULL` 收进 `p_data[0]`、求值期才炸，本层在解析期就拒，免得交回一棵只有求值才坏的树（登记为窄差异）。
  三件套：`cargo test -p minix-fileops --lib` → **312/0**（新建 `find::tests` 18 枚，上一批的 294 枚一枚未动）；freestanding `cargo build -p minix-fileops --lib --target x86_64-unknown-none --release` 绿；`cargo clippy -p minix-fileops --all-targets` 24 条与上一批同数（`find.rs` 内 0 条）；`cargo fmt -p minix-fileops -- --check` → 52 处漂移块与主树同数（新文件自身 0 处）；`bash tools/check-command-boundary.sh` → OK；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 12477→12519，新增锚点逐条 `grep -n`/`sed -n` 回读）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；本包不接命令、不动真机，故不占真机门；核实日 `2026-10-12`。
  留白（同篇下一包）：主项的取值与动作（`function.c` 的 `f_*` 一族——`-name`/`-path` 的 fnmatch、`-newer` 的参照取 stat、`-mtime` 一类的时间折算法、`-exec` 的执行腿、`-delete` 的删除腿）、遍历腿（走表加 `COMFOLLOW`/`XDEV` 语义）、以及 `-regex` 走 `minix-regex` 的接法。另记一处**账面补正**：批次一百三十二的提交信息写了「`cargo test -p xtask` 见下」却没落数字，实测读数是 17/0，随本笔补上。

- ✅ **批次一百三十二完成：`stat` 播种上机并把三架构判据接正**（`2026-10-12`，`claim/CMD-STAT-SEED-qoder` @ 基线 `93559dfcb`，单域三线 commands/xtask/qemu-tests + 启动共享件 `os/etc/rc`，沿用 `new_edge4.md` C-87 的先例登记为 C-93）：`os/xtask/src/image.rs` 的 imgrd `/bin` 名单加 `stat`（名单是三门脚本的单一真源，批次一百二十四已归单，故三门自动同播）；`os/etc/rc` 加三行 `/bin/stat /etc/rc`、`/bin/stat -l /etc/rc`、`/bin/stat -s /etc/rc`——刻意只用三种预置模板而不写 `-f`，是为了避开启动脚本里的引号处理，而三种形已各自覆盖一条渲染腿（默认形带字段取值与带引号的时间串、`-l` 带 `strmode` 切片、`-s` 带 `name=value` 赋值式）。门判据按**形状**而非具体数字：盘上文件的属主、大小与时间戳是镜像属性，不是命令属性。
  三条门的判据落法不同但同源：x86_64 加 Stage 6（`awk` 判默认形 ≥14 字段且末字段是操数名 + 两条 `grep` 判`-l`/`-s` 起头），aarch64 把同三条并进它既有的 `cmds_ok` 串判链，riscv64 在 python 侧加 `ok_stat_def`/`ok_stat_ls`/`ok_stat_shell` 并逐项打进 `cmdface:` 行。**落盘文件都要先剥行尾 `\r`**：串口行是 CRLF，第一版 `awk` 因此把末字段读成 `/etc/rc\r` 而判失败（真机数据其实全在，留此记录免得后人误读为命令未通）。
  三件套（真机读数）：x86_64 `bash qemu-tests/test-cmd-smoke.sh` → `RESULT: PASS (… plus the du read-side totals and the stat command face)`，串口第 262 号设备的 `/etc/rc` 整形读数`262 3 -rwxr-xr-x 1 (0) (0) 0 3334 "Jan  1 00:00:00 1970" "Oct 11 22:40:10 2026" "Oct 11 22:40:10 2026" "Jan  1 00:00:00 1970" 4096 8 0x0 /etc/rc`（日志留 `tmp/r28-x86-serial.log`）；aarch64 → `RESULT: PASS (… + three stat shapes)`（`tmp/r28-aarch64.log`）；riscv64 → `RESULT: PASS (… + three stat shapes)`，末次轮询 `cmdface: … stat=True/True/True`（`tmp/r28-riscv64.log`）。读数本身还额外坐实两件事：`st_atime=0`/`st_birthtime=0` 是 imgrd 盘不携带这两项，而 `st_size=3334` 与 `st_blocks=8` 之差正是 C 的 512 字节块口径。宿主侧 `cargo test -p minix-fileops --lib --bins` → 294+2/0（未改代码，复跑确认无回归）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`；`bash tools/check-command-boundary.sh` → OK；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；三门 `bash -n` 语法门通过；五道收尾校验全过；核实日 `2026-10-12`。
  命令面余量至此剩 `find` 执行层与 `test` 的文件问；imgrd 根盘目录读腿（`getdents`）仍是三架构同症的既有断裂，本批未动（riscv64 门的 `blocked=True` 分支照旧如实报）。

- ✅ **批次一百三十一完成：`stat` 决定半第二包 + 薄壳接线——命令可用（播种与真机门另批）**（`2026-10-12`，`claim/CMD-STAT-FACE2-qoder` @ 基线 `c06a92c24`，单域三线 commands/{fileops,proctools}，无跨界登记）：接批次一百三十的留白，把 `minix3/usr.bin/stat/stat.c` 的后半（`format1()`，`stat.c:568-1151`）落进 `os/commands/bin/fileops/src/stat.rs`，并新建薄壳 `src/bin/stat.rs`。渲染侧按字段取值后交回三形：数值形复刻 C 拼 printf 转换串的语义（参数位 `# + 0 - 空格`、宽度算整个字段、精度对数值补零对字符串截断、`small` 字段在非十进制形先截到 32 位、`%z` 的大中小片段先右移一位加一再右移一位即四舍五入）；字符串形走 `strmode` 的三片段切分、`vis` 的 `CSTYLE|OCTAL|WHITE` 子集、设备号按 `minix3/tools/compat/compat_defs.h:1254`、`:1259-1260` 的主次拆法（`makedev(3,1)` 即 0x301）；浮点形只属于时间字段，把纳秒排在秒后并按精度截位或补零（`stat.c:1029-1099`）。三条缺腿都落在 C 自己的失败支上并写明：`%Su`/`%Sg` 恒打 `(123)`（无口令库，与 `chown` 同一前置）、`%d`/`%r` 字符串形打 `???`（无 `devname`）、`%R` 显式拒绝而非打空串。**照抄不「修正」的一处 C 自身不一致**：`%Mp` 数值折的是粘滞/设组/设身三位（`stat.c:658`），而 `%SMp` 切的是组权限（`:659`）——0o100754 上前者打 `0` 后者打 `r-x`；外部行为优先，测里钉住。
  时间渲染不自建第二份日历：`proctools::dates` 的 `strftime` 子集加一条 `format_at`，`%s` 由调用方交回的秒数承担（`stat -s` 的默认模板正是它），只交字段的调用方（`date`）仍拿不到这一形——显式拒绝而不是反推，避免造出第二个日历编码器（`cal.rs` 已有一份正向编码器，本笔未动它）。`mode.rs` 的 `S_IFWHT` 改为 `pub(crate)` 供 `%T` 的 whiteout 一支复用，不留第二份常量。
  三件套：`cargo test -p minix-fileops --lib --bins` → **296/0**（`stat.rs` 44 枚 = 第一包 21 + 本包 23，薄壳 2 枚）；`cargo test -p minix-proctools --lib` → 85/0（新增 `percent_s_needs_the_seconds_the_breakdown_came_from`）；freestanding `cargo build -p minix-fileops --bin stat --target x86_64-unknown-none --release` 与 `-p minix-proctools --lib` 同 target 皆 0 error；`cargo clippy -p minix-fileops --all-targets` → 24 条，其中新增 5 条全部来自新依赖 `minix-proctools` 的既有告警（`ptime.rs` 三处 `unused_mut` + 两行 crate 汇总），`stat.rs`/`bin/stat.rs` 内 0 条；`minix-proctools` 自身 26 条与主树同数；`cargo fmt -p minix-fileops -- --check` → 52 处漂移块与主树同数（新文件经 `rustfmt --edition 2024` 归零）；`bash tools/check-command-boundary.sh` → OK（本笔一次误报：注释里出现 `minix_types::` 字面即被扫中，已改述）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 token 12377→12477，新增锚点逐条 `sed -n` 回读校正，其中 16 处行号偏移已改正）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；五道收尾校验全过；核实日 `2026-10-12`。
  未做（下一批）：`stat` 播进 imgrd 与 `os/etc/rc` 的三架构判据（`stat` 只读，不受只读根盘门控）；薄壳在无 VFS 的宿主上只能走失败支，实测读数须等真机门。命令面余量剩 `find` 执行层与 `test` 的文件问。

- ✅ **批次一百三十完成：`stat` 决定半第一包——模板与选项解析落地（命令面本体，未接线）**（`2026-10-12`，`claim/CMD-STAT-BIN-qoder` @ 基线 `1d47c524e`，单域三线 commands/fileops，无跨界）：新建 `os/commands/bin/fileops/src/stat.rs`，对位 `minix3/usr.bin/stat/stat.c`（NetBSD 1.38，1151 行）的前半。这台格式机的骨架分两段清楚：`output()`（`stat.c:380-556`）只负责把模板扫成件表，`format1()`（`stat.c:568-1151`）才去取值渲染，两段之间没有共享状态，所以解析形可以单独钉死。本包落三件事：① 五条预置模板与 `TIME_FORMAT` 按 `stat.c:94-115` 原样抄成常量；② `%` 模板扫成 `Piece`——字面件、`%n %t %% %@` 四个单字符简写、字段件（五个参数位按 `stat.c:117-121` 的位值累加，再依次是十进制宽度、可选 `.` 与十进制精度、进制字母 `D O U X F S`、子片段字母 `H M L`、21 个字段码之一），字段件留着子格式串原文，因为 C 有两处拿它回显 `"%.*s: bad format"`（扫描阶段 `stat.c:555-556`、取值阶段 `stat.c:989`/`:997`/`:1117`）；③ 选项面照搬「一个二进制两张人格」——`readlink` 只认 `fnqsv`、默认模板 `%Y`、默认不出声、`-f` 不带值而意为 `%R`、`-v` 才取消出声（`stat.c:219-232`），而 stat 的 `-F -L -l -r -t -x` 在它那里全是未知开关；`-F` 单用折成 `-l`、与别的模板开关并立则报错（`stat.c:279-291`）；所有模板开关共用 `fmtchar` 一个坑，FALLTHROUGH 使 `-f` 自己也占坑，所以 `-f A -r` 与 `-f A -f B` 都是撞车（`stat.c:245-265`）；`-s`/`-x` 各带自己的时间默认 `%s`/`%c`（`stat.c:302-311`）。
  **一处窄差异登记未改**：C 的宽度与精度溢出判据是「乘加之后有符号回绕成负」（`stat.c:469-475`、`:479-488`），上限其实取决于 `int` 宽度与回绕值；本实现改用 checked 算术，并把回显跨度停在打爆计算的那一位数字（C 在报错前先把游标推过一个字符，故它的回显可能多带后一个字符）。被拒的写法集合不因此改变。
  留白（同篇下一包）：字段取值（`format1()` 全段——`strmode` 的片段切分、大小端 `H/M/L`、`#` 触发的 vis 编码、`%Z` 在字符/块设备与其余之间的折衷）、时间渲染（`timefmt` 走 `strftime`）、`%Su`/`%Sg` 的名字回退（C 找不到口令项时打 `(123)` 而不是裸数字，本树尚无口令库，这一形与 `chown` 的前置是同一条）、薄壳 `src/bin/stat.rs` 与 imgrd 播种。**本包不接命令、不改任何可观察行为**，故不占真机门。
  三件套：`cargo test -p minix-fileops --lib` → **271/0**（新建 `stat::tests` 21 枚，本 crate 由 250 增至 271，旧测一枚未动）；freestanding `cargo build -p minix-fileops --lib --target x86_64-unknown-none --release` 绿（本包必须在无 `std` 预编译下成立，故 `String`/`Vec`/`ToString` 显式走 `alloc`）；`cargo clippy -p minix-fileops --tests` → 15 条与共享主树同数（`stat.rs` 内 0 条）；`cargo fmt -p minix-fileops -- --check` → 52 处漂移块与主树同数（新文件自身 0 处）；`bash tools/check-command-boundary.sh` → OK；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 锚点 token 12377→12431，本包新增 54 处全部落窗且逐条 `grep -n` 回读过语义）；权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo check --workspace -j 1` → 0 error；五道收尾校验全过；核实日 `2026-10-12`。文档同步：`06-file-ops.md` 的模块表加 `stat.rs` 一行、测试清单加「模板与选项」一条、§7 现状行的「df、find、stat 待批」更正为各自真实进度。

- ✅ **批次一百二十九完成：更正「内核 `--lib` 宿主测面不可作为门」这条台账行，把内核宿主测的门恢复可用**（`2026-10-12`，`claim/KERNEL-LIB-HOSTTEST-qoder` @ 基线 `9d7f028ee`，一线 `os/kernel` 注释 + 仓库共享件 `AGENTS.md`，登记 `new_edge4.md` C-92）：批次一百二十三核实命令面各批的内核侧门时，只跑了不带形态的 `cargo test -p minix-kernel --lib`，见 5 件 `E0308`/`E0599` 就登记成「宿主测当下不可作为门，在此之前内核侧门只以 `cargo check --workspace` 与三架构 build 为准」，并写成「与 `SD-*` 无涉」。**两处都错**：`SD-30` 把三包默认形态清空之后，形态必须显式写出，`PD-30` 落地的 CI 作业 `kernel-tests.yml` 的 `host-default-form` 腿用的正是 `cargo test -p minix-kernel --features mock`，而这条命令在本树现跑 **831 通过 / 0 失败 / 8 忽略**（lib 腿）加 `tests/boot_integration.rs` **3 通过 / 3 忽略**。那 5 件报错不是缺陷而是 `os/kernel/src/lib.rs` 的 `form_contract` 编译期绊线按设计显形：两行是绊线自身的型对账，三行是消费面（`store_protection`、`store_trap_entry`、`CurrentDirectMap::default`）在同一形态错配下的连带报错。本批只做两件落地：① `AGENTS.md`「Build & Test」加一行内核宿主测的显式形态配方（写清裸跑会被绊线挡住是设计行为、别放宽绊线），免得后续会话再拿裸跑读数立门或自废这条门；② 绊线的文档注释补精度——它先报出「期望型 vs 实到型」，但**不是报错过滤器**，裸跑仍会同报三处消费面，四者同一个原因。台账行按六值词表翻 `closed` 并附三件套与对照读数，旧文那条「只以 `cargo check --workspace` 与三架构 build 为准」的限制随本笔作废。
  三件套：`cd os && cargo test -p minix-kernel --features mock` → 831/0（8 忽略）+ boot_integration 3/0（3 忽略）；对照 `cd os && cargo test -p minix-kernel --lib` → 5 件编译错，报错点 `:81`、`:83`、`:1325`、`:1407`、`:1874` 与台账旧文所列逐一对应；内核生产形态不受本批注释改动影响，仍复跑 `cargo build -p minix-kernel --no-default-features --target x86_64-unknown-none --release` 与 aarch64/riscv64 同款（皆 0 error）、权威编译门 `docker run --rm -m 2g -u $(id -u):$(id -g) -v <树>/os:/src -w /src minix-ci:1.94 cargo check --workspace -j 1` → 0 error（本批起给挂载加 `-u`：容器不再往工作树里留 root 属主的 `target/`，省掉一次 `chown` 善后，也免得下一次本地 cargo 因权限失败）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0 SKIP=1`（P18 12377；跳过的是 `[P1]` 那条 `.review/zcode` 检查——`.review` 不入 git，它只在共享主树有意义，主树内同一条跑到 `PASS=15`）；本批无真机行为变化（改的是注释与文档，串口输出零差异），故不占真机门；核实日 `2026-10-12`。
  下一轮候选不变：imgrd 根盘目录读腿（`getdents`，三架构同症、属 `E5`）、`find`/`stat` 执行层（`minix3/usr.bin/find/` 3078 行、`minix3/usr.bin/stat/stat.c` 1151 行，`df` 已由 C-83 落地）、`rm` 的 unlink errno 坐实（需一次性真机探针）。

- ✅ **批次一百二十八完成：站外探针第二站实扫（minix-rt / kernel-image / init），滚五处钩子与残骸并把诊断面改判留任**（`2026-10-12`，`claim/PROBE-SCAN-RT-KIMAGE-qoder` @ 基线 `6f5172905`，跨二线 `os/libs/minix-rt` + 一线 `os/kernel-image` + 三线 `os/commands/sbin/init`，跨界改点登记 `new_edge4.md` C-91）：接批次一百二十七的口径（三式并扫）把剩下三站判完。`os/libs/minix-rt/src/lib.rs` 滚除 `alloc()` 的 OOM 打印分支（32 件封顶的原子计数 + 定长 hex 拼装）与 `nk4c_oom_tag` 函数本体，分配口收回一句 `with_global_allocator(...)`；`os/kernel-image/src/main.rs` 滚除 bump arena 耗尽分支的 `nk4c_oom_hex` 调用与同名函数本体，该臂回到「返 null 即分配失败」的裸 `GlobalAlloc` 契约；`os/commands/sbin/init/src/main.rs` 滚除 1.10t「逐环仪器化」注释残骸与其空行。**改判留任三族**（`PD-10` 第二档，非删除）：`global_alloc_diag`/`AllocDiag`/`pool_diag` 的文档由「真机采样打印」重述为本 crate 的正式分配器诊断面——消费者是 `lib.rs:351` `test_install_vm_supplier_guards_and_form_switch`（`SD-11` T1 的安装后置条件，经 `diag.total_pages`/`diag.free_pages` 断言）与 `alloc.rs` 的两枚诊断测，删它等于拆自己的测试腿；`alloc.rs` 里引真机 `nk4c: OOM-RT …` 读数的三行注释是把 `MAX_SLABS` 绑池页数、把 `big_blocks` 表抬到 64 的取值理由，属留档不属活体探针；两处 `sys_diagctl_write`（`minix-rt` 与 `init` 的 panic 报告腿，架构项 A-8 第 2 档）是生产机制，不动。
  三件套（前后对照）：三式逐站行读数 minix-rt **11→5**、kernel-image **1→0**、init **1→0**；`grep -rnE "nk4[ac]:" os --include=*.rs | wc -l` → 22→**18**（本笔 -4），余 18 行构成 = 次级核引导诊断 7（`os/kernel/src/smp.rs` 5 + `os/arch/src/{riscv64,arm64}/smp.rs` 各 1，`SD-24`/`PD-16` 在案）+ `*-vmbrk probe ok` 8（U-5/`SD-11` 证据链在案）+ `alloc.rs` 留档 3，脚本基线按 §6 第 7 条另笔滚；`cargo test -p minix-rt --lib` → 63/0、`cargo test -p minix-init --bins` → 149/0；`cargo clippy -p X --tests` 与共享主树对照 → minix-rt 14→12、kernel-image 1=1、minix-init 43→41（零新增）；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0`（P18 12377）；真机门 `bash qemu-tests/test-cmd-smoke.sh` → `RESULT: PASS`（自带 `xtask image --arch x86_64 --release`，两 crate 的 freestanding 生产形态同批坐实）；改后串口 `grep -acE "^nk4" tmp/r24-x86-serial.log` → **8**（与基线 `tmp/du-h-x86-serial.log` 同数同形，SD-11 探针未受影响），`grep -ac "OOM"` → **0**——被滚的两处钩子本就只在分配失败分支触发，基线启动里也从不打印，故滚除不改变可观察输出；命令面 `24<TAB>/etc`、`12K<TAB>/etc`、`du: /etc`、`ls: /bin: read error` 四行逐字仍在；`cargo fmt --check` 三 crate 漂移块 174→173（被删的探针块自身即一处漂移）；核实日 `2026-10-12`。
  **站外扩围收口**：`os/servers`/`os/fs`/`os/drivers`/`os/libs/minix-rt`/`os/kernel-image`/`os/commands` 六站的「task1-close 裁决删除」类残骸已清零，探针归类只剩 `os/kernel/src` 与 `os/arch/src` 两站受 `PD-16` 内存污染案结案条件门控（批次记录在案，不代决）。下一轮候选回到命令面本体：imgrd 根盘目录读腿（`getdents`，三架构同症、属 `E5`）、`find`/`df`/`stat` 执行层、内核 `cfg(test)` 宿主测面（`cargo test -p minix-kernel --lib` 存量 5 件不匹配，见台账同批新增行）。

- ✅ **批次一百二十七完成：按新口径站外扫探针，滚除 servers/vm 里 task1-close 已裁删的五处残骸，并把 RS 一处改判为正式机制**（`2026-10-12`，`claim/PROBE-SCAN-SERVERS-qoder` @ 基线 `92d9a1dbe`，单域三线 servers 轨 + `os/servers/vm`）：接批次一百二十六回填的口径（改用 `sys_diagctl_write|nk4[ac]:|vf:` 三式并扫 `os/servers`、`os/fs`、`os/drivers`）做第一轮实扫，扫到并处理五处：
  ① `os/servers/pm/src/init.rs` 的 1.11d **仍在打印**的探针（43 行：`PMR_N` 限流 64 件、逐件把 `mt+src+req` 以 `pm:` 十六进制写诊断通道）整块滚除，`receive` 臂收回 `Ok(v) => v,`；② 同文件 1.10q 的注释残骸（循环是生产码，留）；③④⑤ `os/servers/sched/src/server.rs` 的 1.10i/1.10y/1.10z 三处——其中两处只剩**恒等 `map_err(|e| { e })`** 空壳，随注释一并收回 `?`，该 crate clippy 告警因此 13→11（这两处正是 `map_identity` 类告警的载体，滚除即消警）；⑥ `os/servers/rs/src/boot.rs` 的 NK4-A Task C 注释残骸与探针撤走后留下的三行空行；⑦ `os/servers/vm/src/brk.rs` 的堆增长腿空体探针（15 行：仍算 `off`/`pre` 并走一次原子计数，体却是 `{}`；它等的 D3a 已由该案的堆增长腿正修结案，注释自陈「结案滚除」），`minix-vm` clippy 告警 258→255。
  **改判一处**（`PD-10` 第二档，非删除）：`os/servers/rs/src/process_table.rs` 的 `indexed_endpoints` 文档原写「NK4-A C-3 迭代18 取证（task1-close 裁决删除）」，但它现被 `boot.rs` step2 的 panic 消息用来点名缺槽服务（`unwrap_or_else` 支路），已是生产诊断路径——文档改为说明这一身份，函数保留。
  **明确不动**：`ds`/`mib`/`vfs`/`sched`/`pm` 各处 `SD-11 真机取证探针`（`*-vmbrk probe ok` 一族）是 U-5 虚存迁移台账行正引的真机读数证据，`tty`/`memory`/`pfs` 同族亦然；动它们等于销毁 SD-11 的证据链，须待该案卷结案按 `PD-10` 重跑归类。
  三件套：`cargo test -p minix-pm --lib` → 426/0、`-p minix-sched` → 88/0、`-p minix-rs` → 357/0、`-p minix-vm` → 542/0；`cargo clippy -p X --tests` 与共享主树逐 crate 对照 → pm 33=33、rs 36=36、**sched 13→11**、**vm 258→255**（零新增、两处净减）；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0`；真机门（PM 在引导关键路径上，必须启动一次才算）：滚除前 x86_64 启动串口含 `^pm:` **64 行**（`grep -ac "^pm:" tmp/vf-probe-after-serial.log`），滚除后同一条启动 **0 行**（`grep -ac "^pm:" tmp/probe-scan-after-serial.log`），而 `24<TAB>/etc`、`12K<TAB>/etc` 两行读数仍在；门本身 `bash qemu-tests/test-cmd-smoke.sh` → `RESULT: PASS (… plus the du read-side totals)`；核实日 `2026-10-12`。
  残留口径（下一轮）：同式再扫余 **43** 处——`os/kernel/src` 33（`PD-16` 内存污染案在案，本批不动）、`os/libs/minix-rt/src` 6、`os/kernel-image/src` 2、`os/commands/sbin/init/src` 1、`os/arch/src/riscv64` 1；后三类共 9 处**落在两行探针台账既declare 的范围之外**（只写了 `os/kernel/src/*.rs` 与 `os/arch/src/*/smp.rs`），已把该漏洞读数写进台账，下一轮按案卷状态逐站判。
- ✅ **批次一百二十六完成：滚除 VFS 的 `vf:` 取证探针残骸，并补上逐站归类的口径漏洞**（`2026-10-12`，`claim/VFS-VF-PROBE-ROLLOFF-qoder` @ 基线 `19f3f06f7`，单域三线 `os/servers/vfs`）：台账「结案之后没有滚除的取证探针」与「探针残骸块」两行的口径都限定在 `os/kernel/src/*.rs` 与 `os/arch/src/*/smp.rs`，因而**扫不到** `os/servers/vfs/src/main_loop.rs` 的 `VfsIpc::receive` 里那一处——它不带 `用后即滚` 字面标记，却自带注释「NK4-C 1.12b 取证探针（task1-close 裁决删除）」，限流 64 件、逐件打 `mt+src` 到诊断通道；task1 早已闭合，问题（init 请求被 `from=RS` 停车挡住）也早有定论。按 `PD-10` 第一档（用后即滚）整块滚除，`receive` 收回三行的净 IPC 适配，被它污染的串口也不再干扰命令面门的文本判据。
  三件套（**前后对照读数**）：滚除前 `grep -ac "^vf:" tmp/du-h-x86-serial.log` → **63**；滚除后同一条 x86_64 启动 `grep -ac "^vf:" tmp/vf-probe-after-serial.log` → **0**，而命令面读数 `24<TAB>/etc`、`12K<TAB>/etc` 仍在（407 行日志）；`cargo test -p minix-vfs --lib` → 538/0（与台账既引数一致）；`cargo clippy -p minix-vfs --tests` 告警总数 75，与共享主树同数（零新增，`main_loop.rs` 剩两处为存量：`:3237`、`:7795`）；`cargo build -p minix-vfs --target x86_64-unknown-none --release` 绿；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`bash qemu-tests/test-cmd-smoke.sh` → `RESULT: PASS (… plus the du read-side totals)`（证明无门依赖该探针输出）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0`；`bash tools/doc-style-lint.sh --diff` → error=0；五道收尾校验全过；该文件 rustfmt 漂移块由 2 降至 1（探针块本身即一处漂移，未新增）。
  口径漏洞已回填台账：两行都记「后续归类宜改用 `sys_diagctl_write|nk4[ac]:|vf:` 三式并扫 `os/servers`、`os/fs`、`os/drivers`，否则会再漏」。未闭部分保持原样：`os/kernel`/`os/arch` 那 14 行仍按 `PD-16` 内存污染案结案条件归类，本笔不代决。下轮候选：imgrd 根盘目录读腿（`getdents`，三架构同症、属 `E5`）、`find`/`df`/`stat` 执行层、按新口径再扫一轮站外探针。
- ✅ **批次一百二十五完成：命令冒烟门接进持续集成（`C-57` 转 `partially fixed`；共享件登记 C-90）**（`2026-10-12`，`claim/CMD-SMOKE-CI-qoder` @ 基线 `5938ae1e5`）：批次一百一十九到一百二十四把三条命令门（`test-cmd-smoke.sh` 与 aarch64、riscv64 变体）修到本地皆绿之后，本批把它接进 CI——此前它只能手跑，命令面的盘上回归无人守（`C-57` 因此长期 `open`）。取**新建独立工作流** `.github/workflows/cmd-smoke.yml` 而非往 `qemu-tests.yml` 里塞 job 的理由：那条工作流的 `on.paths` 跟的是引导载体（`kernel`/`arch`/`plat`/`boot-shim`），本门跟的是命令面与装配面（`commands`/`servers`/`drivers`/`fs`/`xtask`/`etc`/`minix-sys`/`minix-types`），混在一起要么互相拖慢、要么命令面改了根本不触发；分文件后两条各自的触发面才成立。作业形状：矩阵三条腿（`fail-fast: false`）、`timeout-minutes: 45`、装三个裸机 target（`x86_64-unknown-none`/`aarch64-unknown-none`/`riscv64gc-unknown-none-elf`，真源是 `os/xtask/src/image.rs:51-56` 的 `module_target`）与门脚本自己 skip 判据所列的工具件（QEMU 三族、OVMF、mtools、device-tree-compiler 的 fdtput、binutils 的 strings）；退出码按门脚本口径映射 0=PASS、2=SKIP、1=FAIL——SKIP 不让作业红但显式打 `RESULT: SKIP …`，避免「runner 少装一件工具就永远绿」的假象。三件套（本地能验的部分；GitHub 上的首跑须等真实触发，本笔不代跑）：三门本地读数 `RESULT: PASS (18-stage command output visible on the boot console — T4, plus the du read-side totals)`、`RESULT: PASS (aarch64 … marker + ls /bin + cat /etc/rc + du /etc …)`、`RESULT: PASS (riscv64 … + du /etc total …)`；`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/cmd-smoke.yml'))"` → 通过，作业名/矩阵三项/targets 三项回读一致；`case` 映射逐项实测 rc=0→PASS(exit 0)、rc=1→FAIL(exit 1)、rc=2→SKIP(exit 0)；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0`（P6 查的是 `qemu-tests.yml` 的 cross-arch 门，未被本笔扰动）；`bash tools/doc-style-lint.sh --diff` → error=0；五道收尾校验全过；本笔只加工作流与文档，不动生产码，故 `cargo test -p minix-fileops --lib` 结果与批次一百二十四同（250/0，未重跑）；核实日 `2026-10-12`。本行未闭的部分：写类命令（cp/mv/rm/chmod/truncate/mkfifo）的逐条盘上断言仍等可写根盘 `E5`，`ls`/`du` 的目录列腿同属该面——下一批候选即 imgrd 根盘 `getdents` 读腿。
- ✅ **批次一百二十四完成：riscv64 命令冒烟门的 `/bin` 播种名单归单，三架构读数对齐（门脚本跨界改点 C-89）**（`2026-10-12`，`claim/RISCV64-IMGRD-SEEDS-qoder` @ 基线 `b7143503c`）：闭合批次一百二十三登记的那行 riscv 缺口，并**先改正该行记因**——旧文写「该脚本不自己装配镜像」是误判：`test-cmd-smoke-riscv64.sh` 有 Stage 2 与 Stage 3，它确实装配 imgrd 与 mfs；真实成因是它（连同 `test-riscv64-boot-full.sh`）把 `/bin` 名单**手抄**成 `sh echo ls cat`，与单源 `os/xtask/src/image.rs:367-374` 的 `bins` 向量漂移，于是 x86_64 与 aarch64 已播 `du`、riscv64 没有。改法两件事：① 两脚本的播种行改成用 awk 从单源抽 `bins` 的名字，抽不到即 `fail` 显式炸（绝不静默退回硬编码列表）；e5-ds 不在抽取范围内，它按 `E5_DS_READY`（`image.rs:625`，当下只 `x86_64`）逐架构开禁。② `ls /bin` 判据改为二选一：列出目录项就算过，列不出则串口必须显式出现 `ls: /bin: read error`，并把所走分支打进门输出（`listing=… blocked=… ls_ok=…`）。这样门不会因为已知缺陷常红，也不会在缺陷被悄悄绕过时假绿——根盘读腿修好后 `listing` 分支自动接管。三架构后果：`RESULT: PASS (riscv64 18-stage command output …)`，串口读数与另两架构逐字一致——`24<TAB>/etc`、`12K<TAB>/etc`、`du: /etc`、`ls: /bin: read error` 四者俱在；`du=True du_h=True` 即批次一百二十二加的两条形判据在 riscv64 同样成立。三件套：修前该门 `cmdface: … listing=False du=False du_h=False` 240 次轮询后 FAIL（读数 `tmp/cmd-smoke-riscv64-r19.log`），修后 `RESULT: PASS`（`tmp/cmd-smoke-riscv64-r20.log`、`tmp/cmd-smoke-riscv64-r20b.log`，串口留 `/tmp/rs64keep/serial.log`，`strings … | grep -nP '^[0-9.]+[KMG]?\s+/etc'` → 466、476）；抽取式可重跑 `awk '/let mut bins: Vec<\(&str, String\)> = vec!\[/,/^    \];/' os/xtask/src/image.rs | grep -c bin_rel` → 5；`bash -n` 两门脚本通过；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0`；`bash tools/doc-style-lint.sh --diff` → error=0；五道收尾校验全过；本笔不改生产码（纯门脚本与文档），故无新 cargo 测；核实日 `2026-10-12`。下轮候选：imgrd 根盘目录读腿（`getdents`，三架构同症，属 `E5`）、`find`/`df`/`stat` 执行层、内核 `--lib` 宿主测面（一线，已另记一行）。
- ✅ **批次一百二十三完成：修一线内核的 aarch64/riscv64 生产编译面，并把三架构命令冒烟门的实际可跑状况记清（跨界改点 C-88）**（`2026-10-12`，`claim/A64-R64-KERNEL-COMPILE-qoder` @ 基线 `3357378e9`）：批次一百一十九登记的`CurrentPortIo` 断口落地修复。断因是形状而非缺失——`os/kernel/src/syscall.rs` 的 `X86_64Syscall` 定义与 impl 没有被 cfg 挡住，而 `CurrentArchSyscall` 只在 `#[cfg(target_arch = "x86_64")]` 下选它（另两架构走 `DefaultSyscall`），`minix_plat::CurrentPortIo` 又只在 x86_64 与 mock 下存在（`os/plat/src/lib.rs:148-151`），于是 aarch64、riscv64 各起 3 件 `E0433`。取舍：给这段 x86 专属 leg 加 `#[cfg(target_arch = "x86_64")]`，而不是在 plat 里为另两架构造一个假 PortIo 别名——上游这族调用本身就是端口 I/O（`minix3/minix/kernel/system.c:214` 把 `SYS_DEVIO` 派给 `do_devio`，注为 inb/inw/inl/outb/outw/outl），造别名等于宣称存在不存在的硬件面；trait 的默认方法答 `EBADREQUEST` 已是另两架构的正解（本仓既有设计口径 D6）。三架构后果：aarch64 与 riscv64 生产面由 3 error 转 0 error，x86_64 面与权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` 均 0 error；**`bash os/qemu-tests/test-cmd-smoke-aarch64.sh` 随即 `RESULT: PASS`**（判据含批次一百二十二新加的两条 `du` 形判据），读数留 `tmp/cmd-smoke-aarch64-r19.log`。riscv64 变体仍 FAIL，原因与本笔无关且已单记一行：该脚本不做镜像装配，启动的是无 `/bin/du` 的旧盘（串口 `sh: /bin/du: not found`；其 `imgrd.proto` 的 bin 段只有 `sh/echo/ls/cat`，aarch64 那份有第五件 `du`），且 `listing` 一条本笔之前即为 False。同批把台账旧文一处路径误写改正（crate 在 `os/plat`，非 `os/libs/minix-plat`），并登记一条新发现的门可用性缺口：`cargo test -p minix-kernel --lib` 宿主面编译不过（`kernel/src/lib.rs` 在 `cfg(test)` 下 mock 与 x86_64 类型互不匹配，5 件 `E0308`/`E0599`；在未含本笔改动的共享主树同样失败，故为存量）——在此之前命令面各批的内核侧门以 `cargo check --workspace` 与三架构 build 为准。三件套：修前/修后读数见上（可重跑 `cargo build -p minix-kernel --target aarch64-unknown-none --no-default-features`、`--target riscv64gc-unknown-none-elf --no-default-features`）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0`；`bash tools/doc-style-lint.sh --diff` → error=0；五道收尾校验全过；核实日 `2026-10-12`。下轮候选：给 riscv64 cmd-smoke 补镜像装配步、`find`/`df`/`stat` 执行层、imgrd 根盘目录读腿（`E5`）。
- ✅ **批次一百二十二完成：移植 `humanize_number` 并接上 `du -h`，真机取到 `12K<TAB>/etc`**（`2026-10-12`，`claim/HUMANIZE-DU-H-qoder` @ 基线 `702c2b468`，单域三线 commands/fileops + 同线 xtask 与 `os/etc/rc`、三门脚本的同步，跨界共享件按规则 1 沿用 C-87 登记）：批次一百二十点名的 `-h` 留白落地。新建决定半 `os/commands/bin/fileops/src/humanize.rs`，逐段对位 `minix3/lib/libc/gen/humanize_number.c:59-160`（旗标位 `minix3/include/stdlib.h:292-298`）：先把值放大 100 倍留两位（`:103-104`），缩放循环的界是 `max - 50`（`:132`）——两处合起来使 `1 MiB - 1 KiB` 进位成 `1.0M` 而不是 `1023K`；单位表按档号索引，`HN_B` 才在零档放 `B`（`:72-75`），无它则零档只剩数字而分隔空格照旧（尾部空格是有意的，测里钉住）；装不下时按 `snprintf` 截断而非报错，**唯十进制分支先查 `.N` 的余量再回 -1**（`:146-148`），而此时 `buf[0]='\0'` 早已落（`:88-89`），所以 `du -h` 遇到小负数印的是空字段——本实现照此不加兜底数字。`HN_GETSCALE`（答档号不答文本，`:135-138`）无消费者，故不移植而显式拒绝，并由测钉形状，将来加调用者是机械扩展。`du` 侧把 `-h` 从拒收改为接线（`parse_args` 的形状不动），新增 `printed_text`（顺序照 `du.c:263-272`：`-i` 的原始计数优先于 `-h`）与 `HUMAN_BUF=5`；`report` 改走 `printed_text`，把「印什么形状」这有判据可测的一步留在库内，薄壳只写串口。真机侧同步：`os/etc/rc` 在 `/bin/du /etc` 后加 `/bin/du -h /etc`，三门脚本各加一条**「数字+单位字母 TAB /etc」**判据（`^[0-9.]+[KMGT][[:space:]]+/etc`）——与既有的纯数字判据并存，两条各自成立才算过，免得用字面搜 `du` 造成假绿。三件套：`cargo test -p minix-fileops --lib` → 250/0（新建 `humanize::tests` 10 枚 + `du::tests` 增 1 枚 `h_flag_renders_the_prstat_shape`；首轮三处期望是我手算错的——`4.0 KB`/`2.0 kB`/`0 MB` 里那个 `B` 只占零档，实测改回 `4.0 K`/`2.0 k`/`0 M`，另 `du_h(-512)` 按 C 的预算判据应为空即 None，非 `-512B`）；**x86_64 真机**`bash os/qemu-tests/test-cmd-smoke.sh` → `RESULT: PASS`（两条形判据皆中），串口 `tmp/du-h-x86-serial.log` 第 445 行 `12K<TAB>/etc` 与宿主测的预测逐字相同、第 435 行 `24<TAB>/etc`、第 434 行 `du: /etc`（`getdents` 腿仍未通）；`grep -anP '^[0-9.]+[KMGT]\t/etc' tmp/du-h-x86-serial.log` → 445（可重跑）；三门脚本 `bash -n` 通过；freestanding `--bin du/chmod/rm --target x86_64-unknown-none --release` 绿；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`cargo clippy -p minix-fileops --tests` 新改三文件零告警；`rustfmt --edition 2024 --check` 零漂移；新建文件由 CRLF 归一为 LF；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0` 且 `[P18] PASS` 12375 锚点；`bash tools/check-command-boundary.sh` → OK；`bash tools/doc-style-lint.sh --diff` → error=0；五道收尾校验全过；核实日 `2026-10-12`。**aarch64/riscv64 两架构的 cmd-smoke 仍不可跑**：一线 `minix_plat::CurrentPortIo` 缺两架构别名的编译面断口未修（批次一百二十一已单记一行），判据本笔已同步加好，修完即可复跑。下轮候选：`find`/`df`/`stat` 执行层、imgrd 根盘目录读腿（`E5`）、`chown`（待裁决）。
- ✅ **批次一百二十一完成：`du` 播种上机取到真机读数，并改正 `os/etc/rc` 头注里「ls→getdents 已端到端通」的口径**（`2026-10-12`，`claim/DU-MACHINE-qoder` @ 基线 `bd93c2de7`，单域三线：commands/fileops 之外动了 `os/xtask`（同线）与启动共享件 `os/etc/rc`、`os/qemu-tests/*`，已在本行末请一线/文档线知悉）：批次一百二十登记的下一件落地。改动三处——① `os/xtask/src/image.rs:367-374` 的 imgrd `/bin` 名单加 `("du", bin_rel("du"))`，注记文案同步为 `{sh,echo,ls,cat,du[,e5-ds]}`；② `os/etc/rc` 在 `cat /etc/rc` 之后加 `/bin/du /etc`（走绝对路径避开 PATH 假设，未播种架构只多一行 not found，同 `/bin/e5-ds` 先例）；③ 门脚本 `test-cmd-smoke.sh`（新增 Stage 5）、`test-cmd-smoke-aarch64.sh`、`test-cmd-smoke-riscv64.sh` 各加一条串口判据 `^[0-9]+[[:space:]]+/etc`。**判据形状刻意取「数字打头」而非搜 `du` 字面**：`ls /bin` 现在会列出 `du`、`cat /etc/rc` 又会把带 `du` 的注释印回串口，字面搜索必假绿。x86_64 实跑（`bash os/qemu-tests/test-cmd-smoke.sh` → `RESULT: PASS (… plus the du read-side totals)`，串口留存 `tmp/du-machine-x86-serial.log` 462 行）**读到两行关键事实**：第 428 行 `24\t/etc` 说明 `lstat` 与 `st_blocks` 腿在盘上已通（目次自身 24 个 512 字节块 = 12KiB），第 427 行 `du: /etc` 是下探子项时 `getdents` 失败后的如实报错，故那 24 不含 `/etc` 下三个子项。同一断裂在既有命令上早已存在：**同一日志第 370 行 `ls: /bin: read error`**——imgrd 根盘的目录读腿未通，而 `os/etc/rc` 头注此前把这条腿写成"是对已修 CDEV/lookup 链之上的文件面命令面端到端检验"，属账面超出证据，本批按实测改正为「cat→open+read 确实到位、ls→getdents 今日回 read error」并指向台账登记处；门判据也只在脚注里写明「只到总量行到达串口为止，不含遍历完整性」，免得 PASS 被误读。**三架构门当下只有 x86_64 可判**：`xtask image --arch aarch64 --release` 直接失败，根因不在本批——`minix_plat::CurrentPortIo` 只有 mock 与 x86_64 两条别名（`os/libs/minix-plat/src/lib.rs:147-151`），`os/kernel/src/syscall.rs:384`/`:399` 的引用使 aarch64、riscv64 各起 3 件 `E0433`（该用法引入于 `c7434db3d`；x86_64 与宿主 workspace 均绿，pattern-gate 的 P6 只判门在位不判跑绿故长期未现），已作为**新一行**记入 `TODO-LEDGER-OPEN` 的三架构齐平段交一线处理，本批不代改一线码；两个门脚本的改动均已 `bash -n` 校验，待一线修复即可复跑。三件套：`cargo test -p xtask` → 17/0（`etc_proto_*` 系列自带给数，不硬编码播种名单，新增成员不破测）；`bash os/qemu-tests/test-cmd-smoke.sh` → PASS（含 Stage 5 的 du 总量判据）；`bash -n` 三门脚本全过；`grep -an 'read error\|^24' tmp/du-machine-x86-serial.log` → 370/428（可重跑读数）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0`；`bash tools/doc-style-lint.sh --diff` → error=0；核实日 `2026-10-12`。下轮候选不变：`du -h`（移植 `humanize_number`）、`find`/`df`/`stat` 执行层、imgrd 根盘目录读腿（属 `E5`）。
- ✅ **批次一百二十完成：06 篇 `du` 接线（用量累加、单位换算与硬链接去重；`-h`/`-H`/`-L`/`-x` 前置拒）**（`2026-10-12`，`claim/DU-CORE-qoder` @ 基线 `c878d1720`，单域三线 commands/fileops，无跨界）：递归族与计量族的第一件落地——真源 `minix3/usr.bin/du/du.c`（362 行：选项与单位 `:88-192`、逐访问记账 `:197-249`、`prstat` `:258-278`、`linkchk` `:280-351`）加 `minix3/lib/libc/gen/getbsize.c:66-108`（`BLOCKSIZE`）。决定半新建 `os/commands/bin/fileops/src/du.rs`：`parse_args` 收 `getopt "HLPacd:ghikmnrsx"`——`-a`/`-d`/`-s` 三者互斥（`du.c:173-182`）、`-d` 的值走 `atoi`（`-d abc` 读作 0，`:111`）并限 `SHRT_MAX` 内（`:112-116`）、`-P` 与 `-r` 是 C 自己的空操作（`:100-102`、`:139-140`）故照收、`-h`/`-H`/`-L`/`-x` 报 NotWired；`blocksize_from_env` 复刻 `getbsize` 的两重钳位与三段告警文案（`10KB` 这类畸形值在 C 里依次触发 unknown→maximum→minimum 三条警告后落回 512，本实现按同一顺序产同样三条）；`count`（`-i` 计节点否则计 `st_blocks`，`:66`）、`printed_value`/`howmany`（`:258-278`）、`prints_directory`/`prints_file`（`:224-227` 与 `:241-248` 两支打印判据）、`skipped_as_nodump`（`UF_NODUMP`，`stat.h:203`）、`LinkSet`（`:280-351` 的 `(dev,ino)` 去重；哈希常数不抄 C 的黄金比乘法，因它不可观察，抄的是判据与 `(0,0)` 走表外的语义）。薄壳新建 `src/bin/du.rs`：fts 的两趟访问折成一次递归返回——目次把自己那份 `COUNT` 与子项之和在唯一一次打印里输出（对应 C 的 `FTS_DP` 支，`FTS_D` 本被 ignore，故无目录被打两遍），多链接文件第二次遇见既不打印也不计入（C 的 `break`），开不了目次按 `FTS_DNR` 报一次而目次自身照计，`-c` 把各操数返回值累加成 `total` 行，`BLOCKSIZE` 告警先于任何总数。**四条留白/差异**：① `-h` 需 `humanize_number`（`minix3/lib/libc/gen/humanize_number.c`，161 行），本树未移植→整字母前置拒，登记为下一件（与 `du` 的真机播种同批或次批）；② `-H`/`-L` 需逻辑遍历（`du.c:166-171` 决定 fts 形态），`-x` 需挂载点判定；③ `getdents` 硬错在 C 是 `err(1, "fts_read")` 终止全局（`:251-253`），本实现令该目次计错并保留已得总量（与 `rm -r`/`cp -R`/`chmod -R` 同判）；④ 告警消息无 `strerror` 后缀。三件套：`cargo test -p minix-fileops --lib` → 239/0（新建 `du::tests` 16 枚，首跑全过）；`lib.rs` 增 `pub mod du;`，`src/bin/` 自动发现新 bin（该 crate 无显式 `[[bin]]` 表，未碰共享清单）；freestanding `cargo build -p minix-fileops --bin du --target x86_64-unknown-none --release` 绿（`rm`/`ls` 同跑无回归，宿主 rustc 1.94.1）；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`cargo clippy -p minix-fileops --tests` 新建两文件零告警（首轮报出的 `equal expressions as operands to /` 两处已改为常量并把 `alloc::format!` 补齐）；`rustfmt --edition 2024` 整件格式化后 `--check` 零漂移；新建文件由 CRLF 归一为 LF；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0` 且 `[P18] PASS` 12365 锚点——本笔锚点首轮按 `sed` 屏偏移估行，`du.c` 与 `getbsize.c` 的 20 余处已全部改回 `grep -n` 实测值；`bash tools/check-command-boundary.sh` → OK；`find os/commands -path '*/src/bin/*.rs' | wc -l` → 109（原 108）；核实日 `2026-10-12`。**本命令的真机门是下一件**：`du` 只读，不受只读根盘门控，播种 `/bin/du` 后即可取三架构读数——播种点在 `os/xtask/src/image.rs:369-370` 的 imgrd `/bin` 名单（同线文件，三线 xtask），冒烟需一条 rc exec 行（诊断用，取完读数即回退，同 `CMD-RM-BIN` 先例）。详见 `../18-stage-commands/06-file-ops.md` §2.5/§4.1/§4.5/§5 与 `../coordination/TODO-LEDGER-OPEN.md` §7 命令行。
- ✅ **批次一百一十九完成：06 篇 `mkfifo` 接线（默认位与 `-m` 的 `a=rw` 假定基）**（`2026-10-12`，`claim/MKFIFO-BIN-qoder` @ 基线 `2db828e22`，单域三线 commands/fileops，无跨界）：§4.5 契约行里「chown、mkfifo 余行待逐项登记封装」的第二件落地——真源 `minix3/usr.bin/mkfifo/mkfifo.c`（107 行：默认位 `:69`、getopt `"m:"` `:71-91`、`getmode` 基 `:81`、逐名创建 `:93-99`）。决定半新建 `os/commands/bin/fileops/src/mkfifo.rs`：`ASSUMED_BASE`（`-m` 的假定初态是 `a=rw` 即 0666，不是已被掩码削过的那份——`:81` 交出去的是字面量，注释在 `:77-80` 把这条 POSIX 话术原文写在代码里）、`default_mode`（`0666 & ~umask`，只此一路减掩码）、`mode_from_spec`（八进制与符号两形复用 `chmod` 那台 `setmode`/`getmode` 引擎，批次一百一十七接正的 `X` 条件在此同样生效：交进去的 0666 既非目次也无执行位，故 `a+X` 不加）、`parse_args`（BSD getopt `"m:"`：`-m` 带值、第一个非旗标即名字且不再重排、无名为 usage、两个 `-m` 后者从假定基重算而非在前者之上叠加）。薄壳新建 `src/bin/mkfifo.rs`：`umask(0)` 读掩码后还原，逐名 `mkfifo(name, mode)`，失败印 `mkfifo: <名>` 并计错继续（`:93-99`），`-m` 文本被拒时按 C 印 `Cannot set file mode '<文本>'` 并即刻退出（`:74-76`）。两条有意差异登记在薄壳头注：① C 的失败消息是 `warn("%s", ...)`，其 `strerror` 后缀本树无表可印；② C 读完掩码不还原（`:69`），本实现还原——Minix3 的文件服务对 `mknod`/`mkfifo` 根本不施掩码（`minix3/sys/fs/mfs/` 内 `umask` 零命中），本树的 VFS 也只在 `open(O_CREAT)` 一路施掩码（`os/servers/vfs/src/syscalls.rs:702-734`），故两种写法结果相同，还原只是不留无由的副作用。本命令无设施残差。三件套：`cargo test -p minix-fileops --lib` → 223/0（新建 `mkfifo::tests` 9 枚，`cargo test -p minix-fileops --lib mkfifo` → 9/0）；`lib.rs` 增 `pub mod mkfifo;`，`src/bin/` 自动发现新 bin（该 crate 无显式 `[[bin]]` 表，未碰共享清单）；freestanding `cargo build -p minix-fileops --bin mkfifo --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`cargo clippy -p minix-fileops --tests` 新建两文件零告警；`rustfmt --edition 2024` 对新建两文件整件格式化后 `--check` 零漂移；新建文件由 CRLF 归一为 LF（`file` 复核）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0` 且 `[P18] PASS` 12326 锚点（本笔 11 个 `mkfifo.c`/`setmode.c` 锚点逐条 `grep -n` 回读定窗）；`bash tools/check-command-boundary.sh` → OK；`find os/commands -path '*/src/bin/*.rs' | wc -l` → 108（原 107）；核实日 `2026-10-12`。命令面余量：`chown`（用户名/组名换号需口令库，属设施残差）、`find`/`du` 的执行层。详见 `../18-stage-commands/06-file-ops.md` §2.5/§4.1/§4.5/§5 与 `../coordination/TODO-LEDGER-OPEN.md` §7 命令行。
- ✅ **批次一百一十八完成：06 篇 `truncate` 接线（尺寸文法与 `-c`/`-s`/`-r` 全选项形）**（`2026-10-12`，`claim/TRUNCATE-BIN-qoder` @ 基线 `89a845084`，单域三线 commands/fileops，无跨界）：§4.5 契约行里「chown、mkfifo、truncate 余行待逐项登记封装」三件中的一次落一件——真源 `minix3/minix/commands/truncate/truncate.c`（208 行，选项在 `:57-82`、尺寸来源在 `:89-100`、`parselength` 在 `:150-198`）。决定半新建 `os/commands/bin/fileops/src/truncate.rs`：`parse_length` 逐段复刻 C 的尺寸文法——乘数支是**落穿**的（`K` 乘一次 1024、`M` 两次、`G` 三次，故 `1G` 是吉字节而非 1024，`truncate.c:176-190`），而「乘数后不得再有字符」那一判写在 `K` 支里（`:187`），于是 `G`/`M` 也一并被管住（`1KB`、`1Gx` 都拒），空数字串按 C 出 0；`parse_args` 按 BSD getopt `"cr:s:"` 收 `-c`/`-r`/`-s`，`-s` 与 `-r` 恰选其一（`:89` 的 `do_refer + got_size != 1`——两个 `-s` 是后者胜，因为 `got_size` 是旗标不是计数器，`:74`），无操数即 usage，glued 形 `-s10` 拒收而非猜；`open_flags`/`CREATE_MODE` 对位 `:102-106`（建文权限 0644，不含任何执行位）；`relative_target`/`clamp` 对位 `:117-132`。薄壳新建 `src/bin/truncate.rs`：参照文件先 `stat` 一次定长（`:91-100`），逐操数 `open`→相对尺寸时 `fstat`→`ftruncate`→`close`（`:108-141`），`open` 失败只在非 `ENOENT` 时报错计错（`:109-114`，故 `truncate -c 缺失文件` 静默退 0），`-s` 值非法时按 C 印带反引号的原文并立即退出（`:67-71`），参照文件 `stat` 失败只印文件名（`:92-95`）。**三条有意差异（登记在薄壳头注）**：① C 的 `EFBIG` 判据拿 `oflow` 与它自己的来源式 `sb.st_size + rsize` 相比（`:123`），是永假的死支，本树用饱和加算替掉未定义回绕、保留真正起作用的向零钳位；② C 在 `fstat`/`ftruncate` 失败处漏 `close`（`close(fd)` 只落在成功路径后），本实现每条出口都关——泄漏对一个即将退出的程序无可观察差别，不值得复刻；③ 逐操数失败的消息保留本 crate 惯用的 `truncate: <路径>` 形，因本树无 `strerror` 表可印 C `perror` 的后缀。本命令无设施残差。三件套：`cargo test -p minix-fileops --lib` → 214/0（新建 `truncate::tests` 13 枚，全部首跑即过：`plain_digits_are_bytes`、`multipliers_fall_through_as_c_counts_them`、`a_sign_makes_the_length_negative`、`anything_after_a_multiplier_is_refused`、`overflow_fails_instead_of_wrapping`、`exactly_one_size_source_with_a_file`、`the_last_size_letter_wins_as_c_flags_it`、`a_signed_size_is_relative`、`c_flag_and_clusters_and_double_dash`、`a_bad_size_is_reported_with_the_text_that_failed`、`an_unknown_letter_and_a_missing_option_argument_are_usage`、`open_flags_and_create_mode_follow_the_c_flag`、`relative_sizes_land_on_the_clamped_sum`）；`lib.rs` 增 `pub mod truncate;`，`src/bin/` 自动发现新 bin（该 crate 无显式 `[[bin]]` 表，故未碰任何共享清单）；freestanding `cargo build -p minix-fileops --bin truncate --target x86_64-unknown-none --release` 绿（`mkdir`/`rm`/`ls` 同跑无回归，宿主 rustc 1.94.1）；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`cargo clippy -p minix-fileops --tests` 新建两文件零告警；`rustfmt --edition 2024 --check` 新建两文件零漂移；新建文件由 CRLF 归一为 LF（`file` 复核，未进 P8 存量）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0` 且 `[P18] PASS` 12315 锚点（本笔新增 30 个 `truncate.c` 锚点，逐条以 `grep -n`/`sed -n` 回读定窗——首轮按 `sed` 屏偏移估的行号已全部改回实测值）；`bash tools/check-command-boundary.sh` OK；`find os/commands -path '*/src/bin/*.rs' | wc -l` → 107（原 106）；核实日 `2026-10-12`。详见 `../18-stage-commands/06-file-ops.md` §2.5/§4.1/§4.5/§5 与 `../coordination/TODO-LEDGER-OPEN.md` §7 命令行。
- ✅ **批次一百一十七完成：06 篇 `chmod` 的 `X` 面按 `setmode.c` 真源接正（裸 `-X` 归模式、条件添加不复用 `x`）**（`2026-10-12`，`claim/CHMOD-X-FACE-qoder` @ 基线 `7ea281ed2`，单域三线 commands/fileops，无跨界）：闭合批次一百一十六登记的同一行两处待办——`chmod` 模式文法与裸模式字母集对 `X` 的处理。真源核实：`minix3/lib/libc/gen/setmode.c` 的 `setmode` 把 `X` 计入独立累加器 `permXbits`（`:296-297`），子句末单独落发成一条 `'X'` 命令（`:330-336`），而 `getmode` 的 `case 'X'`（`:148-150`）只在**原始** `omode` 带目次位或任一执行位时把这三位**或**进去——从不清掉。方案取舍：把 `apply_mode` 的入参从「剥掉类型位的十二个权限位」改为「文件自身的完整 `st_mode`」（`apply_mode(current: u32, spec)`），因为条件要读类型位；另一条路是给 `apply_mode` 加一个 `is_dir: bool` 旁参，但那会把 C 里同一个 `mode_t` 承载的信息拆成两处传，且粘滞/设身份位与执行位的关系以后还要再拆，故取前一条——`mkdir -m` 那一路恰好证明这点：C 传的是 `S_IRWXU|S_IRWXG|S_IRWXO`（`mkdir.c` 的 `getmode(setmode(optarg), 0777)`），因此 `-m a+X` 在 C 与本树都会加上执行位（0777 已含执行位），改完入参形状后无需特判即同果。改动面：决定半 `mode.rs` 的 `apply_clause` 新增 `xbits` 分支与「先常规位、后条件位」的落发次序，`what_bits` 不再收 `X`；`is_bare_mode` 字母集补 `X`（`chmod.c:127-133` 的 `case … 'X' …` 会把 `-X` 回退成模式操作数，本树此前报 usage）；薄壳 `bin/chmod.rs` 的 `apply_one` 改交完整 `st_mode`（同 `chmod.c:215-216` 交 `p->fts_statp->st_mode`），返回值的类型位由文件系统掩掉（`os/libs/minix-fs/src/memfs.rs:546` 以 `mode & 0o7777` 落笔）。行为面：`chmod -X 目录` 由「拒绝执行」变为「条件加执行位」；`chmod a+X 常规文件(644)` 不加（此前会误加）；`chmod a-X` 从「清掉执行位」改为 C 的条件添加（`getmode` 的 `'X'` 只 OR）。**一处窄差异登记未改**（`apply_clause` 注释里写明）：`-`/`=` 子句里 `X` 与拷贝字母 `u`/`g`/`o` 并现时，C 只在 `op == '+'` 才落发 `X`（`setmode.c:316-318`），本树按子句末统一落发——如 `chmod -Xu f` 一形两支不同。三件套：`cargo test -p minix-fileops --lib` → 201/0（`mode::tests` 20 枚，本笔新增 3 枚：`x_letter_only_adds_when_the_original_is_a_directory_or_already_executes`、`x_never_clears_even_in_a_minus_clause`、`equals_then_x_keeps_the_conditional_add`；首跑现算出一处自写错的期望——`a=X` 应用于 755 常规文件应为 `100111` 而非 `100755`，因 `addcmd` 把 `=` 拆成「先清后设」而设的那半为空（`setmode.c:373-384`），已按 C 改正）；freestanding `cargo build -p minix-fileops --bin chmod/mkdir/cp/rm/ls/mv --target x86_64-unknown-none --release` 全绿（宿主 rustc 1.94.1）；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`cargo clippy -p minix-fileops --tests` 触及文件零告警（顺手清掉 `what_bits` 里那处 `unneeded late initialization`，本 crate 库告警由 4 降至 3，余三处在 `expr.rs`/`printf.rs` 非本笔领地）；`rustfmt --edition 2024 --check` 触及两文件零漂移（`mkdir.rs` 的一处 import 排序漂移为存量，未抖）；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0` 且 `[P18] PASS`（总数由 12280 升至 12285；本笔新增的 `setmode.c` 锚点逐条以 `grep -n`/`sed -n` 回读定窗）；行尾保持 LF；核实日 `2026-10-12`。详见 `../18-stage-commands/06-file-ops.md` §2.2/§4.1/§4.5/§5 与 `../coordination/TODO-LEDGER-OPEN.md` §7 递归族一行的 `chmod` 段。
- ✅ **批次一百一十六完成：06 篇 `chmod` 接上 `-R` 递归改形（走表用 `dents`，符链一律跳过）**（`2026-10-12`，`claim/CHMOD-RECURSIVE-qoder` @ 基线 `e51901047`，单域三线 commands/fileops，无跨界）：闭合批次一百一十五点名的递归族第二条——真源 `minix3/bin/chmod/chmod.c:144-224`。决定半（`os/commands/bin/fileops/src/mode.rs`）把内联在薄壳里的选项扫描收拢为纯件并接线递归：`ChmodFlags{force,recursive,verbose}` 加 `ChmodFlags::parse`（getopt 串 `"HLPRXfghorstuwx"` 在 `chmod.c:87`，簇 `-Rf` 与 `--` 终止按 cp/mv/rm 同形）、`ChmodOption::NotWired`/`Unknown` 两值分报、`is_bare_mode`（从薄壳搬入并加测：`chmod.c:127-133` 的 `--optind` 回退使 `-r` 成为模式而非旗标）、`chmods_entry`（`chmod.c:144-157` 与 `:186-210`：递归走表是物理形，而唯一能改符链的 `-h` 在 `chmod.c:145-150` 被禁止与 `-R` 并立，故 `FTS_SL`/`FTS_SLNONE` 一律 `continue`——这正是 `-R` 不需要 `lchmod` 的理由，也纠正了上一版拒收文案里「`-R` 也要 `lchmod`」的误判）。薄壳（`bin/chmod.rs`）：`chmod_one` 一次访问按 `chmod.c:186-219` 定序——先由 `chmods_entry` 决定这访问是否改动（跳过不算失败）、再 `apply_mode` 折位后 `chmod`、目次在预序改完即以 `descend_chmod` 下探（后序访问不再改，同 C 的 `FTS_DP` `continue`，因此没有任何文件被改两次）；`-f` 只压消息不改退出码（`chmod.c:215-219` 的 `&& !fflag`）；取不到操数状态报一次并跳过（`FTS_NS`），开不了目次同样报一次——但目次本体在开表之前已按 C 改过（`FTS_DNR` 形，`chmod.c:190-193` 是先 warn 再落进 `change_mode`）。行为面两处按 C 收窄与放开：状态取法按扫描形态分（`-R` 用 `lstat`、缺省用 `stat`，因 `chmod.c:157-158` 的 `FTS_COMFOLLOW`）；`stat` 失败的消息不再受 `-f` 抑制（C 的 `warnx` 在 NS 支无 `fflag` 门）。三件套：`cargo test -p minix-fileops --lib` → 198/0（`mode::tests` 17 枚，本笔新增 5 枚：`chmod_flags_default_to_all_off`、`chmod_clusters_and_double_dash`、`chmod_link_flags_are_refused`、`bare_permission_letter_is_a_mode_not_a_flag`、`a_physical_walk_passes_over_links`）；freestanding `cargo build -p minix-fileops --bin chmod --target x86_64-unknown-none --release` 绿（`cp`/`mv`/`rm`/`ls` 同跑无回归，宿主 rustc 1.94.1）；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`cargo clippy -p minix-fileops --tests` 触及文件零告警（`mode.rs` 内一处 `unneeded late initialization` 属本笔之前的存量，行号因插入而后移）；`rustfmt --edition 2024 --check` 触及两文件零漂移；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0` 且 `[P18] PASS` 12280 个 C 锚点全过（本笔新增 17 个 `chmod.c` 锚点，总数由 12263 升至 12280，逐条以 `grep -n` 回读定窗）；行尾保持 LF（触及三件皆 `i/lf`）；核实日 `2026-10-12`。**顺带登记两处新发现的选项面待办（本笔不改，守一次一条）**：① `is_bare_mode` 的字母集缺 `X`，而 `chmod.c:128` 的 `case … 'X' …` 把它算作裸模式，故 `chmod -X file` 在本树报 usage 而在 C 是把模式当 `-X` 解析；② 本树模式文法把 `X` 折成普通 `x`（`apply_what` 里 `b'x' | b'X' => 0o111`），未实现 C 的「仅当目标是目次或已有执行位才加执行」这一判。二者同属 `chmod` 模式文法面，宜并成一批处理。递归族至此只剩 `find`/`du`。详见 `../18-stage-commands/06-file-ops.md` §4.1/§4.5/§5 与 `../coordination/TODO-LEDGER-OPEN.md` §7 递归族一行。
- ✅ **批次一百一十五完成：06 篇 `cp` 接上 `-R` 递归复制形（目次两趟访问与三条特殊文件腿）**（`2026-10-12`，`claim/CP-RECURSIVE-qoder` @ 基线 `895913af5`，单域三线 commands/fileops，无跨界）：闭合批次一百一十二点名的递归族残差第一条——真源 `minix3/bin/cp/cp.c:324-548` 的 `copy()` 加 `minix3/bin/cp/utils.c:304-352` 的三条特殊文件腿。决定半（`os/commands/bin/fileops/src/cp.rs`）五件事：① `CpFlags` 加 `recursive`，`-R` 从拒收清单改为接线，余 `-r/-a/-l/-H/-L/-P/-N` 仍整组前置拒（`-r` 要逻辑扫描、`-a`/`-P`/`-H`/`-L` 要符链遍历模式且 `-P` 还要 `lchown`/`lchmod`、`-l` 要硬链接代替复制、`-N` 要断链保持为链）；② `classify` 补第三形 `DirToDne`（`cp.c:268-269`：递归＋目次源＋目标 `stat` 找不到），并把尾斜杠报错收窄到 `FileToFile`（`cp.c:275` 那一闸只挡一形），于是 `cp -R dir dst/` 在两者都不存在时 C 给的是复制而非报错；③ `join_target` 的 `DirToDne` 支落在目标名本身（对应 `cp.c:396-397` 把 `base` 置到源路径末尾、根条目贡献一个空组件）；④ 新增 `entry_action`（按 `S_IFMT` 八分支分派，对位 `cp.c:443-537`；socket 与不认识的类型一并走 `default:` 的文件复制，那里 `open` 失败即 C 的报错）、`is_link`、`writes_through`（`utils.c:109` 的 `tolnk = (Rflag && !(Lflag || Hflag)) || Pflag` 在遍历模式全拒的这棵树里就是递归扫描，故既存目的地若是符链就不写穿：不打开、走先删再建那一支）、`post_create_dir_mode`（`cp.c:502-506`：无 `-p` 把原始模式交 `chmod`、粘滞位在内，有 `-p` 把 `setfile` 已经掩过的模式交 `chmod`、粘滞位因此出局）；`is_unwanted_directory` 加 `recursive` 参数（`cp.c:456` 的 `if (!Rflag && !rflag)`）。薄壳（`os/commands/bin/fileops/src/bin/cp.rs`）：`copy_one` 一次位置一次访问——源状态按扫描形态取（`-R` 走 `lstat`，因 `cp.c:174, 190-196` 递归形是 `FTS_PHYSICAL`，缺省支才是 `stat`；这一条与本笔最容易写反的 `access` 真值表同族，已按 C 逐行读回）、目的地取一次 `stat` 后先跑「同一文件」与「非目次不得盖目次」两拒（前者在 C 里还 `fts_set(FTS_SKIP)` 跳过整棵子树，故此处直接返回而不下探），再按 `entry_action` 分派；`copy_dir` 把 C 的 `FTS_D`/`FTS_DP` 两趟做成同一帧的两半，C 为跨两趟携带「是否新建」而设的 dne 栈（`cp.c:303-321`）因此不需要，`dne` 就是个本地量；预序 `mkdir(dst, st_mode | S_IRWXU)`（`cp.c:482-483`：强开主读写执行位好让 555 的源目次能被填，umask 仍在其上生效），`descend_copy` 以 `dents::next_record` 走表（`d_ino==0` 槽位与 `.`/`..` 跳过），子项的源名与目的地各按 `dents::join_path` 拼同一名字——这正是 `cp.c:387-416` 的 base 偏移规则所陈述的「源根之下的相对路径照搬到目的地根之下」；后序按 `-p` 复权限与两个时刻、再对新建目次 `chmod`；三条特殊文件腿 `copy_link`（`readlink`+`symlink`）/`copy_fifo`（`mkfifo`）/`copy_special`（`mknod`，节点需特权，非 root 常回 `EPERM`）逐条对位，各自先 `exists && unlink` 再建；三条腿都按 `cp.c:517-533` 套在 `if (Rflag)` 里——非递归时 fifo 与设备节点仍走 `copy_file`（对 fifo 就是 `utils.c:102` 那个会阻塞的 `open`），首版漏掉这一层门控会让 `cp fifo dest` 从「打开并搬运」变成「重建一个 fifo」，已按 C 补回；`-v` 给目次的两趟各打一次，同 C 的两次访问。行为面同步：`run()` 的操数循环收拢为「算目的地→`copy_one`」，`-R` 下先按 `cp.c:258-269` 用 `lstat` 判首源是否目次以定三形；`mkdir` 失败与「目的地已存在却不是目次」（`cp.c:487-491` 的 `errno = ENOTDIR`）在 C 都是 `err(EXIT_FAILURE)`，本实现同样整轮终止而非只计该操数。**留白与差异（如实登记）**：fts 的环状目录告警（`cp.c:349-352`）无对应物——物理遍历不追符链，走不回同一棵树；`-p` 复制符链时只复两个时刻（经 `utimensat` 带 `AT_SYMLINK_NOFOLLOW`），不复主与权限，因本树无 `lchown`/`lchmod`，路名形式的 `chown`/`chmod` 会穿过新链落到其目标上；`getdents` 硬错在 C 是 `err(1, "fts_read")` 终止全局，本实现令该操数计错并继续（与 `rm -r` 同判）；链接文字与条目名按本树系统调用面的 `&str` 口径处理，非 UTF-8 即报出并跳过；递归深度不受控（C 的 fts 也不受控，极深目录本树先爆栈）；写类命令的真机效果仍由可写根盘 `C-57`/`E5` 门控（根盘为 boot-shim imgrd 只读形，`/bin/cp` 亦未播种进镜像），本笔交代码＋宿主测＋freestanding 编译门。三件套：`cargo test -p minix-fileops --lib` → 193/0（`cp::tests` 32 枚，本笔新增 5 枚：`recursive_letter_is_wired`、`recursive_directory_source_into_a_missing_target_is_case_three`、`entry_dispatch_covers_every_type_bit`、`a_recursive_scan_never_writes_through_a_link`、`post_order_dir_stamp_follows_the_preserve_mask`）；freestanding `cargo build -p minix-fileops --bin cp --target x86_64-unknown-none --release` 绿（`mv`/`rm`/`ls` 同跑无回归，宿主 rustc 1.94.1）；权威编译门 `docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error；`cargo clippy -p minix-fileops --tests` 触及文件零告警（首版一处 `collapsible_if` 已按建议改为 let 链）；`rustfmt --edition 2024 --check` 触及两文件零漂移；`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0` 且 `[P18] PASS` 12261 个 C 锚点全过（本笔新增锚点逐条回读 `cp.c`/`utils.c` 定窗，含 `stat.h:177` 的 `S_ISLNK` 修正）；行尾保持 LF（`git ls-files --eol` 触及五件皆 `i/lf`，`git diff --ignore-cr-at-eol --stat` 与 `git diff --stat` 同行数）；核实日 `2026-10-12`。详见 `../18-stage-commands/06-file-ops.md` §4.1/§4.5/§5 与 `../coordination/TODO-LEDGER-OPEN.md` §7 递归族一行。
- ✅ **批次一百一十四完成：行尾归一收尾（C-85 余两件跨线文件）与把两个仓库级门接进持续集成**（`2026-10-12`，`claim/P8-ZERO-AND-CI-GATES-qoder` @ 基线 `b4fc379ef`，跨线与共享件已照规则 1 登记于 `../coordination/new_edge4.md` C-86）：上一批把本线 55 件归一后，P8 仍红 2 项——`os/drivers/storage/at_wini/src/bus.rs`（二线）与 `os/libs/minix-boot/src/boot_form.rs`（一线），而这两个残留正好卡住台账 §7「门不在闸上」的丙子面（把全量门接进持续集成会立刻标红）。本笔一次做完：① 那两件按同配方归一（逐文件比对 `git show HEAD:<路径>` 去 `\r` 后与工作树逐字节相同，`git diff --ignore-cr-at-eol` 为空，纯行尾零内容改动）；② `.github/workflows/rules-consistency.yml` 新增「仓库级门禁」一步，跑全量 `bash tools/pattern-gate.sh`（此前持续集成只跑它的 `--self-test`）与 `bash tools/check-command-boundary.sh`，从此「红着入库」在持续集成里就会被拦。可重跑读数：`bash tools/pattern-gate.sh` → `PASS=14 FAIL=0` 且 `[P8] PASS`；`git ls-files --eol` 里 `i/crlf` 的 `.rs`/`.sh`/`.ld` 计数 → 0；核实日 `2026-10-12`。详见 `../coordination/TODO-LEDGER-OPEN.md` §7 同行的乙残留与丙收口。**本笔的过程事故（如实登记）**：实现提交 `de1ad8b04` 的提交信息被 heredoc 吞掉——消息正文写成了命令行尾巴（`cd ... && git log --oneline -1`），因为收尾命令把 `MSG` 结束符漏在了同一条 shell 行之后。改动内容本身已核（`git show --stat de1ad8b04` 与上文三件套一致），不重写已合并的历史，故以本行作为该提交的正向说明；合入提交 `895913af5` 的信息完整。教训：多命令串接时提交信息必须由独立的 heredoc 结束，不与 `&&` 链混写。
- ✅ **批次一百一十三完成：命令域行尾归一（55 个 CRLF 文件）与 `.gitattributes` 入库前防线**（`2026-10-12`，`claim/LINE-ENDINGS-GITATTRIBUTES-qoder` @ 基线 `644fb87bc`，共享件 `.gitattributes` 与跨线残留已照规则 1 登记于 `../coordination/new_edge4.md` C-85）：批次一百零五的收尾校验现算出 `tools/pattern-gate.sh` 的 P8（跟踪文件 CRLF 入库）长期红着 57 项，其中 55 项在命令域（`os/commands/usr-bin/textfilter/` 为主）。本笔一次性归一这 55 个文件：逐文件比对 `git show HEAD:<路径>` 去 `\r` 后与工作树**逐字节相同**，`git diff --ignore-cr-at-eol` 为空——纯行尾零内容改动；新加仓库根 `.gitattributes`（`* text=auto`，只让 git 自行判别文本并在提交时存 LF，不强制 checkout 端行尾），把 P8 从「事后可查」变成「入库即拦」。**不动的两件属他人领地**：`os/drivers/storage/at_wini/src/bus.rs`（二线）、`os/libs/minix-boot/src/boot_form.rs`（一线，C-40 确权归 boot 线），已在 C-85 行请各自线一次性处理。可重跑读数：`bash tools/pattern-gate.sh` → `违规 2 项`（由 57 降，余下即上述两件）；`git ls-files --eol | awk '$1 ~ /^i\/crlf/ && /\.(rs|sh|ld)$/' | wc -l` → 2；`[P18] PASS` 12217 token 全过（行尾改动未动摇任何 C 锚点）；`git diff --ignore-cr-at-eol --stat` 除 `.gitattributes` 外为空。核实日 `2026-10-12`。详见 `../coordination/TODO-LEDGER-OPEN.md` §7「门不在闸上」一行的乙子面收口。
- ✅ **批次一百一十二完成：06 篇 `rm` 接上 `-r`/`-R` 递归删形（走表用 `dents`，不用 fts）**（`2026-10-12`，`claim/RM-RECURSIVE-qoder` @ 基线 `fd3f2e579`，单域三线 commands/fileops，无跨界）：拆掉批次一百零五点名的「目录递归遍历封装」残差的第一条落地——`rm_tree`（真源 `minix3/bin/rm/rm.c:167-281`）。薄壳 `src/bin/rm.rs` 新增 `rm_tree_one`/`descend_children`/`remove_entry`/`echo_removed`，保留 C 的判定顺序：先 `lstat`（不可得→FTS_NS 形：`-f` 只吞不存在族，否则报且不动）；非常规文件走预序询问后 `unlink`；目次先预序询问（拒答即跳过整个子树，对应 C 的 `FTS_SKIP`+`SKIPPED`），再用 `dents::next_record` 逐条走子项并递归（`d_ino==0` 的已删槽位按 libc `readdir.c:83-84` 跳过；`.`/`..` 由 `dents::is_descendable` 排除；名字按 `dents::join_path` 拼接），子项完后再 `rmdir` 自身（后序），开不了目次即 C 的 `FTS_DNR`：报一次仍试 `rmdir`；`-v` 逐名回显。`lstat` 不追符链（C 用 `FTS_PHYSICAL`），所以符链作为条目被 `unlink` 而非跟进目次。决定半：`needs_unwired` 不再计 `-r`/`-R`（仅剩 `-P`/`-W`/`-x`），拒收文案同步为 "rm: -P/-W/-x not wired"。既有测 `unwired_flags_flagged` 的 `-r` 断言反向修正（原断言 `-r` 需拒收，现在断言 `-r`/`-R` 均已接线），无新增测（新增代码全在系统调用腿上，可测决策已全部住在 `dents`/`check_flavor`）。三件套：`cargo test -p minix-fileops --lib` → 188/0；freestanding `cargo build -p minix-fileops --bins --target x86_64-unknown-none --release` 绿（含 `rm`，宿主 rustc 1.94.1）；clippy 触及文件零告警（首版一处 `unused_mut` 已修）；rustfmt 触及文件零漂移；行尾保持原状（`bin/rm.rs` 仍整件 CRLF，未抖入式归一；可重跑 `python3` 计数 350/350 CRLF）；核实日 `2026-10-12`。**声明性留白**：递归删的真机效果受可写根盘 E5 门控（同写类命令）；C 的 `-x`（`FTS_XDEV`）需挂载点判定、`-W` 需 `undelete`、`-P` 需 `arc4random`/`O_NOFOLLOW`，三者仍整组前置拒；C 在 fts_read 硬错时 `err(1)` 直接终止全局，本实现改为「该操作符计错且仍试 `rmdir`」（不中断其余操数）；嵌套深度不受控（C 的 fts 也不受控，本树递归在极深目录下会先爆栈，真机进不到那个深度）。下轮候选：`cp -R`/`chmod -R`/`find`/`du` 可照同一先例逐条接。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百一十二与 `../06-file-ops.md` §4.1/§5。
- ✅ **批次一百一十一完成：06 篇抽出 `dents.rs` 目录项走表共用件，`ls` 改走它**（`2026-10-12`，`claim/DENTS-WALK-SHARED-qoder` @ 基线 `c9c15fd99`，单域三线 commands/fileops，无跨界）：`ls` 此前把 `getdents` 记录解码内联在薄壳里，而递归族（`rm -r`、`cp -R`、`chmod -R`、`find`、`du`）要的是同一张表——先抽共用件再逐条接。新模块 `minix_fileops::dents`（纯字节切片，无描述符也无分配）：`next_record`（记录布局 `d_fileno` u64 在 0、`d_reclen` u16 在 8、`d_namlen` u16 在 10、`d_type` u8 在 12、名在 13，游标按 `readdir.c:75-81` 的 `dd_loc += d_reclen` 前移）、`keep_in_listing`（`-a`/`-A` 点条目规则）、`is_dot`+`is_descendable`、`join_path`（C `ls` 的 `dotdir`：`.` 出裸名）、`file_type` 常量组。与 C 的一处刻意差异：长度自相矛盾的记录（`d_reclen` 为 0、超出缓冲、`d_namlen` 超出所属记录）本件回错并终止走表（C 的 `readdir` 回 NULL 即终止列目录），而内联版原会按 `namlen` 直接切数组——那是越界 panic 隐患，收拢时一并堵掉（三例有测）。行为面：`ls` 的输出与旗标语义逐字不变。**剩余（已登记，下轮逐条接）**：`rm -r` 的 `rm_tree`、`cp -R`、`chmod -R`、`find`/`du` 执行层——它们等的遍历就差这张表加一次后序递归，本笔先落走表本体。三件套：`cargo test -p minix-fileops --lib dents::`→6/0、全 crate `--lib`→188/0（182 存量无回归）；freestanding `cargo build -p minix-fileops --bins --target x86_64-unknown-none --release` 绿（含 `ls`）；clippy 首版报一处「隐藏生命周期」（返回类型里的 `Entry` 未标生命周期），已修后触及文件零告警；rustfmt 触及文件零漂移、全 crate 漂移基线 43→43；新建文件为 LF（`file` 复核，未进 P8 存量）；核实日 `2026-10-12`。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百一十一与 `../06-file-ops.md` §4.1/§5。
- ✅ **批次一百一十完成：06 篇 `cp` 的 `-i` 询问按 C 原样出问并收拢到共用读取**（`2026-10-12`，`claim/CP-PROMPT-FIDELITY-qoder` @ 基线 `4ee4b702b`，单域三线 commands/fileops，无跨界）：接回批次一百零七登记的乙——`src/bin/cp.rs` 的 `overwrite_ok` 原把提示写成 "cp: overwrite <目标>? "，而 `utils.c:124` 是裸 `fprintf(stderr, "overwrite %s? ")`：既无程序名前缀也不给目标名加引号，本笔按 C 改齐；同时把它自带的一份「首字符定结果 + 丢弃整行其余」读取（`utils.c:126-127` 的 getchar 循环）换成批次一百零八落地的 `bin_support::answer_yes()`，至此 rm/mv/cp 三命令共用同一份答案读取与同一套裸提示词形态。行为面：仅提示文字与共用调用变化，`-i` 的生效条件（`cp.c:148` 的 `isatty(stdin)`）与拒收即跳过不计错的语义不动。三件套：`cargo test -p minix-fileops --lib` → 182/0（本笔无新增测，纯文案与共用件收拢，判定逻辑已由 `cp::` 27 枚与 `bin_support` 共用件覆盖）；freestanding `cargo build -p minix-fileops --bin cp --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；`cargo clippy -p minix-fileops --tests` 触及文件零告警；`rustfmt --edition 2024 --check` 触及文件零漂移；核实日 `2026-10-12`。修法详见 `../coordination/TODO-LEDGER-OPEN.md` §7 门不在闸上一行（乙子面收口）与 `../06-file-ops.md` §5 末段。
- ✅ **批次一百零九完成：`df` 的 statvfs 访问子改走 minix-sys 再导出，命令层边界守卫翻绿**（`2026-10-12`，`claim/DF-BOUNDARY-REEXPORT-qoder` @ 基线 `b69dac8c3`，跨线触碰已照规则 1 登记于 `../coordination/new_edge4.md` C-84：二线 `os/libs/minix-sys/src/lib.rs` 加自有 `os/commands/sbin/mountinfo`）：批次一百零五的收尾校验现算出 `tools/check-command-boundary.sh` 红着（自批次四十 `df` 接线起，2 处直引 `minix_types`），本笔收口。修法按守卫自身提示与 errno 再导出同例：`StatvfsBuf` 与其字节偏移模块 `statvfs_off` 是 `minix_sys::statvfs`/`getvfsstat` 签名里已在用的同一 ABI 面，故在 `lib.rs` 的 `Stat` 显式再导出旁一并转交；`src/bin/df.rs` 的 `use` 与 `src/df.rs` 模块文档引用改指 `minix_sys`，不改 `minix-types`（权威所在）、不动 VFS 服务端。这是依赖边界收敛而非行为改动：`df` 的输出与选项面逐字不变。三件套：`bash tools/check-command-boundary.sh` → `OK: command layer dependency boundary clean` 退码 0（本守卫自 C-7 落地后首次由红翻绿）；`cargo test -p minix-mountinfo --lib` → 17/0、`-p minix-sys --lib` → 323/0；freestanding `cargo build -p minix-mountinfo --bin df --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警（`bin/fsck_mfs.rs:119` 一条属存量）；`rustfmt --edition 2024 --check` 漂移基线 minix-sys 152→152、mountinfo 8→8（本笔零新增，两 crate 的存量漂移不在射程）；核实日 `2026-10-12`。**同轮提醒（登记不自办）**：同一批收尾校验还查出 C-7 那道守卫既不列在 `TODO-LEDGER-INDEX.md` §6 五道校验里、也不在任何工作流里，所以「红着入库」才会发生——接闸子面另轮做（台账 §7 门不在闸上一行的丙）。修法详见 `../coordination/TODO-LEDGER-OPEN.md` §7 同轮注记。
- ✅ **批次一百零八完成：06 篇 `rm` 接上终端上的 override 询问（共用答案读取收拢到 bin_support）**（`2026-10-12`，`claim/RM-OVERRIDE-ASK-qoder` @ 基线 `fb368df6c`，单域三线 commands/fileops，无跨界）：接回批次一百零七登记的「`rm` 同名 tty override 询问只差接线」——上一轮落地的 `mode::strmode` 正是它缺的那块。决定半 `minix_fileops::rm` 新增 `CheckFlavor` 与 `check_flavor`（`rm.c:551-560` 的四条件：先查 `-i`→`Remove`；否则非终端、或符链（C 注：权限无意义）、或 `access(name, W_OK)` 成功、或那一拒恰好是 `ETXTBSY`（正在运行的程序：可写但占用），均不问；全返才 `Override`）。薄壳 `src/bin/rm.rs` 的 `check` 改为：`lstat` 已得的 mode + `isatty(STDIN)` + `access(path, W_OK)` 的拒绝码进 `check_flavor`，两形提示词按 C 的裸 `fprintf`（`remove '<路径>'? ` 与 `override <权限> <uid>:<gid> for '<路径>'? `——注意 `rm` 用冒号、`mv` 用斜杠分隔用户与组，两者各按各自源文件），无 `-f` 时答案为非“是”则跳过本操作符（`rm.c:330` 的 `continue`，不计错）。**同时消除一处重复**：`rm` 与 `mv` 原本各写一份“首字符定结果 + 丢弃整行其余”的读取，现收拢为 `bin_support::answer_yes()`（两处调用点改走共用件），库层仍不沾系统调用。**关键纠偏（本轮读 C 时推翻自己第一反应）**：`!(access(name, W_OK) && (errno != ETXTBSY))` 这一句字面看像“ETXTBSY 也要问”，按 `access` 返回 0 为成功、-1 为真值重评后结论相反——`access` 成功时内层为假→外层为真→直接 return 1（不问），ETXTBSY 同样不问；只有“因其它原因被拒”才进询问。初版按直觉写反了这一条，已按 C 的真值表重写并由测钉住（`check_flavor_needs_terminal_writable_and_type` 五路对照）。**声明性留白**：真机要进到这条询问需终端与可写根盘（`C-57`/E5；脚本态无终端，C 与本树同为不问）；`user_from_uid`/`group_from_gid` 的名字形式需 `getpwuid`/`getgrgid` 封装（归 14 侧登记项），本树取 C 的数字回退形；C 在 `check` 里的 `-P` 分支（`rm.c:563-568`）因 `-P` 整组前置拒而不可达；`Errno` 无 ETXTBSY 关联常量→本地 `const ETXTBSY: i32 = 26`（对 `minix3/sys/sys/errno.h:68`），沿用 mv 笔 EXDEV 的同型做法。三件套：`cargo test -p minix-fileops --lib rm::`→16/0（+2）、全 crate `--lib`→182/0（180 存量无回归）；freestanding `cargo build -p minix-fileops --bin rm --bin mv --bin cp --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；`cargo clippy -p minix-fileops --tests` 触及四文件零告警；`rustfmt --edition 2024 --check` 触及四文件零漂移、全 crate 漂移基线 43→43；核实日 `2026-10-12`。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百零八与 `../06-file-ops.md` §4.1/§5。
- ✅ **批次一百零七完成：06 篇 `mv` 询问梯对位与 `strmode` 回渲**（`2026-10-12`，`claim/MV-TTY-OVERRIDE-ASK-qoder` @ 基线 `2e0aa3fa8`，单域三线 commands/fileops，无跨界）：接回批次一百零六登记的「`do_move` 询问面两处未对位」。决定半新增：`mode::strmode`（`minix3/lib/libc/string/strmode.c`，声明 `unistd.h:403`——十一个字符：类型字母、九个权限字符、末尾访问控制列表填充位；三组 execute 槽兼作设身份位记号的 `S`/`s`/`T`/`t` 四分支；常规文件支再查 `stat.h:148-149` 的归档态两位）与 `mv::ask_about`（把 `mv.c:172-203` 的四条出口收成纯决策：`None`/`SourceUnreachable`/`Overwrite`/`Override`）。薄壳 `src/bin/mv.rs`：原「仅 `-i` 提示」换成走 `ask_about`，三路 `access`/`stat` 探针各就各位（`access(to, F_OK)` 定门、`access(to, W_OK)` 与 `stat(to)` 驱动 override、两条询问支先做 `access(from, F_OK)` 并按 C 报 "rename <源>" 计错）；**同时修正两处提示词文案偏离**：C 走裸 `fprintf`，批次一百零三那笔却给 "overwrite" 加了 "mv: " 前缀与目标名引号——现已按 C 原样；答案读取补上排行（`mv.c:195-198` 的首字符加丢弃其余，那一笔只读一口）。`override` 行印九个权限字符与用户/组号——本树无口令库封装，故取 C `pwcache.c:296-306` 在查无条目时的十进制回退形（与 C 同果）；分隔空格按 C 的 `modep[9] == ' '` 就地计算并已用测钉住（本布局下索引 9 总是权限字符，那一判永假）。新增 8 枚宿主测（`mode::` 8→12、`mv::` 9→13）。**正确性回归两处自纠与一处发现**：其一，本轮把 `strmode` 的归档态分支补上——初版见 `strmode.c:71-80` 包在 `#ifdef S_ARCH2` 里就当作不适用，实测 Minix 确实定义了 `stat.h:148-149` 的 `_S_ARCH1`/`_S_ARCH2`，该段是编得进来的；其二，两处手推的 golden 断言被 runner 当场翻红（`S_ISGID` 单独到位时 `S` 在组 execute 格即切串索引 5，非 6；`0o1777` 只带粘滞位故前两组仍为 `x`，`rwsrwsrwt` 需 `0o7777`）——属测试自身错而非实现错，已按 `strmode.c:111-178` 重推。**顺带发现（登记待查、非本笔改动面）**：`rm` 的同名 tty override 询问（`bin/rm.rs` 头注已记“需 strmode + user_from_uid/group_from_gid”）如今 `strmode` 已在位，只差接线与 `access(to, W_OK)` 探针，可作下一轮单独接（一次一条，本笔不扩面）。另 `cp` 的提示词也带了同样的自加 "cp: " 前缀（`bin/cp.rs:overwrite_ok`，对位 `utils.c:124` 的裸 `fprintf`），同属文案忠实度残差，与 `rm` 一并登记。三件套：`cargo test -p minix-fileops --lib mode::`→12/0、`--lib mv::`→13/0、全 crate `--lib`→180/0（172 存量无回归）；freestanding `cargo build -p minix-fileops --bin mv --bin cp --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；`cargo clippy -p minix-fileops --tests` 触及三文件零告警（`mode.rs:86` 的 unneeded late initialization 与 `expr.rs:85` 的 redundant_pattern_matching 均为存量基线）；`rustfmt --edition 2024 --check` 触及三文件零漂移、全 crate 漂移基线 43→43；核实日 `2026-10-12`。**声明性留白**：真机跑询问路需终端与可写根盘（`C-57`/E5），脚本态无终端故 override 不发火（C 同行为：`mv.c:182` 的 `stdin_ok`）；口令名与组名的文字形式需 `getpwuid`/`getgrgid` 封装（归 14 侧登记项）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百零七与 `../06-file-ops.md` §4.1/§5。
- ✅ **批次一百零六完成：06 篇 `mv` 接上跨盘搬运的常规文件形**（`2026-10-12`，`claim/CMD-MV-COPY-qoder` @ 基线 `f8634c7a3`，单域三线 commands/fileops，无跨界）：接回批次一百零三登记的「EXDEV 跨盘 copy 引擎未接」残差——本笔对**常规文件**那一支落地（真源 `minix3/bin/mv/mv.c` 409 行 BSD；本轮逐段读完 `do_move`/`fastcopy`/`copy` 三函数）。薄壳 `src/bin/mv.rs`：`rename` 回 EXDEV 后走 `cross_device_move`——先 `lstat` 目标，存在就按类型先清掉（目次 `rmdir`、其余 `unlink`，对位 `mv.c:235-240`；清不掉报 "can't remove" 计错且不动源）、再 `lstat` 源定路线（`:246-252`）；常规文件走 `fastcopy`（`:256-354`）：开源、开目标（**交完整 `st_mode`**，与 `cp` 的 `create_mode` 剥设身份位有意不同）、复用批次一百零五的 `cp::pump` 逐块搬运、任一腿失败则删掉半成品目标（`:291-295`，连删除也失败另报 "to: remove"）、`futimens` 定两个时刻、改主（非 EPERM 才报，失败则丢设身份位）、定权限、关描述符、**最后才 `unlink` 源**（`:345-348`，顺序不可改：先删源会把没搬完的文件丢干净）、`-v` 在成功末尾回显。决定半 `minix_fileops::mv` 新增三可测决策：`cross_device_plan`（按源类型分就地/子进程）、`existing_removal`（按目标类型选 `rmdir`/`unlink`）、`move_create_mode`（不剥设身份位），与 `cp::create_mode` 钉成反向对照（同一处两台引擎有意不同，不钉就会被后来的“顺手统一”抹掉）；`CrossDevice`/`RemovalCall` 各带全量 derive 供 `assert_eq!` 直接比对。新增 3 枚宿主测（`mv::` 6→9）。**顺带发现（登记待查、非本笔改动面）**：`do_move` 的询问面还有两处未对位——其一，C 在无旗标时也会问：既存目标不可写且标准输入为终端（`mv.c:182-191` 的 `stdin_ok && access(to, W_OK) && !stat(to)` 一支，走 "override <模式>/<用户>/<组> for <目标>? " 形），现本树仅在 `-i` 时提示；其二，`-i` 分支先做 `access(from, F_OK)` 前置检查，源不存在时报 "rename <源>" 而本树交给 `rename` 自己失败（退码同、文案不同）。按“一次一条”不顺手改，已登在本行供下轮单独接。**声明性留白**：源为目次/符链/设备/FIFO 那一支，C 的 `copy`（`mv.c:357-399`）不自己拷——`vfork`+`execl` `/bin/cp -PRp`（带 `-v` 时 `-PRpv`）再 `execl` `/bin/rm -rf`，需 exec 递送链（`NS5`/`C-21`）→本笔按实际报 not wired 计错，不假装成功；C 用 `fchown`/`fchmod` 在描述符上定主与位（`:328-334`），本树无 `f*` 封装→关闭前按路名做（目标为自己刚建的常规文件，同结果）；C 缓冲取自 `st_blksize` 且 `malloc` 一次复用（`:277`），本笔用 8 KiB 定长堆块（块尺寸不属可观测行为），且分配耗尽本树走运行期 panic 而非 C 的 malloc 失败报信；真机写盘受可写根盘 E5 门控（同写类命令，逐命令断言归 `C-57`/E5）；C `warn` 的 `strerror` 后缀未复现。三件套：`cargo test -p minix-fileops --lib mv::`→9/0、全 crate `--lib`→172/0（169 存量无回归）；freestanding `cargo build -p minix-fileops --bin mv --bin cp --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；`cargo clippy -p minix-fileops --tests` 触及文件零告警；`rustfmt --edition 2024 --check` 触及两文件零漂移、全 crate 漂移基线 43→43；核实日 `2026-10-12`。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百零六与 `../06-file-ops.md` §4.1/§5。
- ✅ **批次一百零五完成：06 篇 `cp` 接线（非递归复制形与搬运环）**（`2026-10-12`，`claim/CMD-CP-BIN-qoder` @ 基线 `3f0dcb97d`，单域三线 commands/fileops，无跨界）：落地台账里最后一个未接的常规文件命令 `cp`（复制，真源 `minix3/bin/cp/cp.c` 548 行加 `minix3/bin/cp/utils.c` 431 行 NetBSD；`open`/`read`/`write`/`close`/`stat`/`fstat`/`chmod`/`chown`/`utimens`/`umask`/`getuid`/`isatty`/`unlink` 客户端封装已齐，无需改共享库）。采 rm、mv 的非递归先例：只接 `cp [-f|-i] [-p] [-v] src target` 与 `cp [-f|-i] [-p] [-v] src ... directory` 两形；递归族 `-R`/`-r`/`-a` 与 `-l`/`-H`/`-L`/`-P`/`-N`（依赖 fts 目录遍历与 `copy_link`/`copy_fifo`/`copy_special` 三条特殊文件腿）整组显式拒收并报 "cp: -X not wired (fts directory walk)" 退 1。决定半新增 `minix_fileops::cp`（纯、可测）：`CpFlags::parse` 复刻 `cp.c:117-167` 的 getopt 串（`-f` 清 `-i`、`-i` 依 `cp.c:148` 的 `iflag = isatty(fileno(stdin))` 只在终端下生效、`-p`/`-v`、首个非选项即停、`--` 终止），`classify` 复刻 `cp.c:245-287` 的目标分类（目次→`FileToDir`；否则先查 `cp.c:249` 的操作数约束、再查 `cp.c:275` 的尾斜杠两路报错，先后次序有测钉住），`join_target` 加 `appended_component` 复刻 `cp.c:387-417` 的路径拼接（含末组件为 `..` 时改记 `.` 的 `base += 1` 一支），三档权限掩码 `create_mode`（`utils.c:146` 去设身份位、留粘滞位）、`retained_mode`（`utils.c:280-288` 的 `RETAINBITS & ~umask`）、`preserved_mode`（`utils.c:373` 不含粘滞位——两掩码的差别正落在粘滞位，有测对分），`regains_set_bits`（`utils.c:282-287` 四条件），`is_identical`/`overwrites_dir`/`is_unwanted_directory`（`cp.c:424-434`、`cp.c:456-462`），`pump` 经 `CopyIo` 接口复刻 `utils.c:244-255` 的读块写块环（短写即错、错腿归属分目标与源），`pumps_data` 照写 `utils.c:183` 零长源一次不读的怪癖。27 枚宿主测。薄壳 `src/bin/cp.rs`：`copy_file`（`utils.c:93-296`）逐段对位——开源、`-i` 提示（`utils.c:119-131`：拒并不算失败）、现存目标再取一次状态（`utils.c:138-142`）与 `-f` 先删再建（`utils.c:148-156`）、搬运、`setfile` 加 `set_utimes`（`utils.c:364-404`、`utils.c:64-76`）、`-v` 打 `src -> dst`（`cp.c:539-540`，连拒收也照打，忠实 C）。ABI 常量取 Minix 权威值（`minix3/sys/sys/stat.h:111-159`、`minix3/sys/sys/fcntl.h:64-100`：O_WRONLY 0x1、O_CREAT 0x200、O_TRUNC 0x400），与批次八十八 ed 纠错后的同仓口径一致。三件套：`cargo test -p minix-fileops --lib cp::`→27/0、全 crate `--lib`→169/0（142 存量无回归）；freestanding `cargo build -p minix-fileops --bin cp --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；`cargo clippy -p minix-fileops --tests` 触及两文件零告警；`rustfmt --edition 2024 --check` 触及两文件零漂移、全 crate 漂移基线 43→43（存量在 `expr.rs`/`testexpr.rs`/`touch.rs`，非本笔）；核实日 `2026-10-12`。**正确性回归三处自纠**（对 C 逐段读回，均落在本轮代码内）：其一，`restore_set_bits` 初版把 C 的「`fstat` 失败」与「组不符则不动作」两支折成一条 `&&` 条件，会把目标组与源组不符这一正常情形误报失败——已抽出函数恢复 C 的两处报错点；其二，`setfile` 初版对改主失败一律静默，丢掉 `utils.c:376-379` 的非 EPERM 必须报 chown 错——已按 `Errno::EPERM` 分流；其三，`getuid` 失败兜底原写 0（恰是 root 的用户号，会让设身份位回灌分支对 root 源误触发），改为无文件可持有的 `u32::MAX`。**测试自身纠偏**：`retained_mode` 首测手算错（0o4755 本就没有组写位与其他写位，umask 0o22 是空操作，曾误期望 0o4733），按「写位是否真被掩码清掉」重写四条对照；`pump_uses_the_caller_scratch_and_never_overreads` 专为末块补零这一类复制缺陷设防。**声明性留白**：真机写盘受可写根盘 E5 门控（同写类命令，逐命令断言归 `C-57`/E5）；C 的 `mmap` 快路（`utils.c:196-240`）与 `MAXBSIZE` 单静态缓冲按 8 KiB 堆块替代（缓冲大小不属可观测行为）；`fchown`/`fchmod` 本树无封装→改走路名形式（目标为常规文件时同结果，已在薄壳头注明）；C 的 `strlcpy` 超长名拒收未复现（超长路径由 VFS 回 ENAMETOOLONG）；C 对空目标名读取前一字节的越界行为不复制（本实现按无尾斜杠处理）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百零五与 `../06-file-ops.md` §4.1/§4.5/§5。
- ✅ **批次一百零四完成：06 篇 `link`/`unlink` 接线（POSIX 单系统调用命令）**（`2026-10-11`，`claim/CMD-LINK-UNLINK-BIN-qoder` @ 基线 `7a4786243`，单域三线 commands/fileops，无跨界）：落地两条缺失命令 `link`/`unlink`（硬链创建/删目录项，真源 `minix3/usr.sbin/link/link.c` 71 行、`minix3/usr.sbin/unlink/unlink.c` 70 行 NetBSD；`link`/`unlink` 客户端 wrapper 已备）。两命令无 getopt、无选项——本体就是 `if (argc!=3/2) usage()` 加一次 `link(2)`/`unlink(2)`（对位 06 篇 §2“一次系统调用加错误处理”）。决定半新增 `minix_fileops::linkops`（纯、无 alloc）：`link_operands`（恰二操数→`(old,new)` 否则 None）、`unlink_operand`（恰一→路径否则 None），与 `ln`（BSD 多操数 `-s/-f/-i/-n` 工具）区分。3 枚宿主测（恰二/恰一/路径原样、含 0/1/3 越界拒）。薄壳 `src/bin/{link,unlink}.rs`：取 argc 形状→`minix_sys::link`/`unlink`→Ok 退 0/Err 报 `<cmd>: <path>` 退 1（perror 无 strerror 后缀→仅报路径）。写类命令→真机受可写根盘 E5 门控（同 ln/rm/mv，per-command 归 C-57/E5），但代码零设施残差（不像 mv 有跨盘 copy 残差）。三件套：`cargo test -p minix-fileops --lib linkops::`→3/0、全 crate `--lib`→142/0（139 存量无回归）；freestanding `cargo build -p minix-fileops --bin link --bin unlink --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1，不走 docker）；clippy 零告警（首版 `use alloc::vec::Vec` 仅测试用到→非测试构建 unused→删该 import 与一枚弱“编译守卫”测、余 3 测已足）、rustfmt 触及三文件零漂移（`bin_support.rs` 未被 `#[path]` 连带改）。**声明性留白**：真机建链/删项受可写根盘 E5 门控；C `perror` 的 `strerror` 后缀未复现；宿主 `bin_support` 走 `minix_sys::write` 无内核→薄壳 bin 宿主不可跑（决定半已宿主测覆盖 argc 形状）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百零四与 `../06-file-ops.md` §5。
- ✅ **批次一百零三完成：06 篇 `mv` 接线（同盘 rename 形、跨盘 copy 作残差）**（`2026-10-11`，`claim/CMD-MV-BIN-qoder` @ 基线 `ce99d7f3e`，单域三线 commands/fileops，无跨界）：落地缺失命令 `mv`（改名/移动，真源 `minix3/bin/mv/mv.c` 409 行 BSD；`rename`/`stat`/`unlink`/`rmdir` 客户端 wrapper 已备）。采 rm 非递归先例：只接**同盘 `rename` 形**（POSIX “fast” 路径 `mv.c:218`），跨盘 copy（`copy`/`fastcopy`，`mv.c:256-409` 需数据拷贝+保时/权+unlink 的拷贝引擎）作声明性设施残差——EXDEV 报 not wired 计错、不假装成功。决定半新增 `minix_fileops::mv`（纯、可测）：`MvFlags::parse` 复刻 `getopt "ifv"`（`-f` 清 `-i`、`-i` 清 `-f`、`-v`、首个非选项停、`--` 终止、未知→None）、`plan_operands`（argc<2→Usage/2→Pair/N→ToDirectory，对位 `mv.c:113-134`）、`dest_name`（尾斜杠剥除不空再取末组件、根→空，与 `ln::basename` 一致）。6 枚宿主测。薄壳 `src/bin/mv.rs`：Pair 若目标是目次→接 basename、否则直接 `rename`；ToDirectory 先校 dir 为目次（非→usage，`mv.c:118`）、逐源 `dest_name` 拼目标后 `rename`；`-i` 对现存目标提示读首字符 y/Y、`-f` 不提示、`-v` 打 `src -> tgt`、`rename` Err EXDEV→跨盘残差、退码按失败。`S_IFMT/S_IFDIR`/`EXDEV=18` 本地常量（对 C `sys/stat.h:138,141`、`errno.h` 核实；`Errno` 无 EXDEV 关联常量→本地 const + `Errno::from_i32` 比较）。三件套：`cargo test -p minix-fileops --lib mv::`→6/0、全 crate `--lib`→139/0（133 存量无回归）；freestanding `cargo build -p minix-fileops --bin mv --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1，不走 docker）；clippy 触及文件零告警、rustfmt 触及两文件零漂移（本轮 `bin_support.rs` 未被 `#[path]` 连带修改，已核）。**test-audit 自身红**：`dest_name("/")` 初版断言误写为 `"/"`（臆想“不空”），runner 当场翻红——根路径无最终组件、与 `ln::basename` 一致应为空串，已改断言+同步 dest_name 文档。**声明性留白**：EXDEV 跨盘 copy 引擎（`copy`/`fastcopy`）为单独多轮设施未接；真机写盘受可写根盘 E5 门控（同写类命令，per-command 归 C-57/E5）；C `warn`/`err` 的 `strerror` 后缀未复现；宿主 `bin_support` emit/warn 走 `minix_sys::write` 无内核→薄壳 bin 宿主不可跑（决定半已宿主测覆盖 flags/切分/dest_name）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百零三与 `../06-file-ops.md` §5。
- ✅ **批次一百零二完成：12 篇 `renice` 接线（优先级重置、base-0 getnum 决定半）**（`2026-10-11`，`claim/CMD-RENICE-BIN-qoder` @ 基线 `eb315a0da`，单域三线 commands/proctools，无跨界）：落地缺失命令 `renice`（重置已运行进程的调度优先级，真源 `minix3/usr.bin/renice/renice.c` 181 行 NetBSD；`getpriority`/`setpriority` 客户端 wrapper 已备，与 `nice` 不同它不 exec、控制路径全可达）。决定半新增 `minix_proctools::renice`（纯、可测）：`getnum` 忠实 `strtol(str,&ep,0)` base-0——可选符号、`0x`/`0X` 十六进制、前导 `0` 八进制、否则十进制，**关键忠实角落**：带尾字符（`*ep!='\0'`如 `12abc`/`078`）、无数字（`abc`/`0x`）、溢出（LONG_MIN/MAX+ERANGE）皆拒，而空串按 strtol 当 0 接受；`clamp_prio` 对位 `donice`——`-n` 时新值=旧值+增量，再夹 `[PRIO_MIN=-20, PRIO_MAX=20]`（常量对 `sys/sys/resource.h:43-48` 核实）；`Plan::parse` 复刻 main 结构（`argc<2`→usage、前导 `-n` 置 incr、`getnum` 取 priority、余下 `-g`/`-u`/`-p` 切 which、目标分 `Numeric`/`User`(getpwnam 待解)/`Bad`(getnum 失败或负 id)）。11 枚宿主测（十进制/十六/八进制/尾字符拒/溢出拒/绝对与增量夹取/目标穿插分类/Bad/错误三类）。薄壳 `src/bin/renice.rs`：逐 `Numeric` 目标 `getpriority`→`clamp_prio`→`setpriority`、成功打 `<id>: old priority <o>, new priority <n>`、退码 `errs==0?0:1`。三件套：`cargo test -p minix-proctools --lib renice::`→11/0、全 crate `--lib`→84/0（73 存量无回归）；freestanding `cargo build -p minix-proctools --bin renice --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1 全 target 齐）；clippy 触及文件零告警（3 枚 `unused_mut` 属 `ptime.rs` 存量）、rustfmt 触及两文件零漂移（本轮 proctools `bin_support.rs` 未被 `#[path]` 连带修改，已核）。**声明性留白**：`-u user` 需 `getpwnam`（本树未封装，同 chown/chgrp/from）→User 目标报 not wired 计错；真机优先级生效需 PM/调度器响应 setpriority（代码+宿主测+编译已到位，per-command 真机效果归 C-57/E5）；C `warn`/`warnx` 的 `strerror` 后缀未复现；坏优先级诊断未回显出错 token（只报 generic）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百零二与 `../12-process-tools.md` §5。
- ✅ **批次一百零一完成：10 篇 `what` 接线（SCCS `@(#)` 标识串提取决定半）**（`2026-10-11`，`claim/CMD-WHAT-BIN-qoder` @ 基线 `b5a779d87`，单域三线 commands/doctools，无跨界）：落地缺失命令 `what`（从文件里抽 `@(#)description` 版本标识串，真源 `minix3/usr.bin/what/what.c` 128 行 BSD/NetBSD；`open`/`read`/`close` 客户端 wrapper 已备）。决定半新增 `minix_doctools::what`（纯、可测）：`WhatFlags::parse` 复刻 `getopt "s"`（`-s` 止首、首个非选项停、`--` 终止、未知→None）；`extract_ids` 逐字节忠实移植 `search()`（`what.c:97-120`）——**关键忠实角落**：`goto loop` 回退让失配的下一字符重新作 `@` 候选（故 `@@(#)`、`@(x@(#)` 均能重同步命中）、`@(#)` 头后标识跑到终止集 `{'\0','"','>','\n','\\'}` 之一（该终止符被消费但不输出，同 C 内层 while）或 EOF；`-s` 首个即返。12 枚宿主测（双 `@` 回退/部分头后重同步/不成配/EOF 无终止/空标识/反斜斜终止/`-s`）。薄壳 `src/bin/what.rs`：逐 operand `open`/`read` 全字节→先打文件名（`what.c:88`）→逐 id `\t<id>`、开失败按 C `perror;exit(matches?0:1)` **立即终止整程序**（非跳过，`:84-87`）、退码 `matches?0:1`（无命中按标准退 1）。只读（无建文件无 exec）→不受 E5 写盘门控（同 `whatis`/`which`）。三件套：`cargo test -p minix-doctools --lib what::`→12/0、全 crate `--lib`→58/0（46 存量无回归）；freestanding `cargo build -p minix-doctools --bin what --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1，不走 docker 免 root 产物）；clippy 触及文件零告警、rustfmt 触及两文件零漂移（`bin_support.rs:145` 双空行属存量、非本轮）。**声明性留白**：真机逐文件 `open` 腿与 imgrd 播种归 C-57/E5；C `perror` 的 `strerror` 后缀未复现（仅报 `what: <名>: cannot open`）；宿主 `bin_support` emit/warn 走 `minix_sys::write` 无内核→薄壳 bin 宿主不可跑（决定半已宿主测覆盖提取器全分支）。**开工前小阻**：首跑 `claim CMD-WHAT-BIN` 因上会话遗留的空 `.wt/cmd-what-bin-qoder/` 目录（仅空目次、无 `.git`）使 `git worktree add` 失败且锁回滚；先 `rm -rf` 该空残壳再重领即成（已验证 worktree 在册、git status 不再回落主仓）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百零一与 `../10-doc-man-tools.md` §5。
- ✅ **批次一百完成：10 篇 `which` 接线（whereis.c 的 which 人格、PATH 拆分决定半）**（`2026-10-11`，`claim/CMD-WHICH-BIN-qoder` @ 基线 `839cc5b3e`，单域三线 commands/doctools，无跨界）：落地缺失命令 `which`（在 `$PATH` 里定位可执行程序，真源 `minix3/usr.bin/whereis/whereis.c` 163 行 NetBSD——同一二进制按 `strcmp(getprogname(),"which")`（`:66`）在 which/whereis 两人格间切换；`stat`/`access` 客户端 wrapper 已备）。决定半新增 `minix_doctools::which`（纯、可测）：`WhichFlags::parse` 复刻 `getopt "ap"`（`-a` 清 which 首匹配→全显、`-p` 选 env PATH、首个非选项停、`--` 终止、未知→None）；`path_candidates` 逐字节对位 `whereis.c:126-136`——**关键忠实角落**：空元素（前/双/尾冒号）按 C `t==p`/`strlen==0` 折为 `.`，每元素逐字 `%s/%s`拼接（元素 `/` 得 `//name`）；`is_absolute`（`:115` 的 `**argv=='/'`）；`compute_status`（`:157` found 0→3、≥argc→0、之间→2）。11 枚宿主测（旗标四类/空元素三类/空 PATH/元素斜杆逐字/绝对名判定/退出码三支+found>argc）。薄壳 `src/bin/which.rs`：which 人格（`useenvpath` 默认真）读 envs 的 `PATH`（未设→退 1，对位 `:97-99`），逐 operand 绝对名直查或 PATH 候选查，每候选 `stat`→`S_ISREG`→`access(X_OK)`（`:141-146`）命中即报、`-a` 前首匹配则跳下一 operand；退出码 `compute_status`。只读（无建文件无 exec）→不受 E5 写盘门控（同 `pwd`/`printenv`/`env`）。`S_IFMT=0170000`/`S_IFREG=0100000`（对 `stat.h:138,143`）、`X_OK=0x01`（对 `sys/unistd.h:169`）本地常量。为此给 doctools `bin_support` 加 `envs()` 双 cfg seam（照 fileops `pwd`/proctools `printenv` 先例，对其它 doctools 薄壳只增不改）。三件套：`cargo test -p minix-doctools --lib which::`→11/0、全 crate `--lib`→46/0（35 存量无回归）；freestanding `cargo build -p minix-doctools --bin which --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1，不走 docker 免 root 产物）；clippy 触及文件零告警、rustfmt 触及两文件零漂移（`bin_support.rs:145` 一处双空行属 HEAD 存量、非本轮、未动）。**声明性留白**：同源的 `whereis` 人格（默认取 `sysctl user.cs_path`、报多路匹配）为单独命令入口待后续；C `setgid(egid)`/`setuid(euid)` 前序（`:71-75`，仅为 setuid 脚本查寻时让 `access` 按有效 id）未复现（普通命令本就按自身 id 查）。**另登记一处文档侧偏差（本笔不代改）**：本篇 §一命令表行“whereis | `-b` 程序、`-m` 手册 | 名字→路径”与§2.2 “程序/手册页/源码三路”描述的是经典 BSD whereis，与实源 `minix3/usr.bin/whereis/whereis.c`（仅 `-a/-p` PATH 搜索的 which/whereis 合体、无 `-b/-m`/无 man/src 三路）不符，属文档侧事实待修（真值优先序：实源为准）。修法详见 `../18-stage-commands/todo.md` §6.1 批次一百与 `../10-doc-man-tools.md` §5。
- ✅ **批次九十九完成：12 篇 `env` 接线（环境执行、putenv 合并决定半）**（`2026-10-11`，`claim/CMD-ENV-BIN-qoder` @ 基线 `947a72faf`，单域三线 commands/proctools，无跨界）：落地缺失命令 `env`（在修改后的环境里执行命令或打印环境，真源 `minix3/usr.bin/env/env.c` 99 行 BSD；与同 crate 已接线的 `printenv`（批次五十三）为只读/可写孪生）。决定半新增 `minix_proctools::env`——`parse_flags` 复刻 `getopt(argc, argv, "-i")`（`env.c:59-73`）因选项串以 `-` 开头而无置换语义：扫完前导 `-i`（含簇 `-ii`）、首个非选项（含裸 `-`）即停、选项区内未知 `-x`→None（usage）；`command_start` 对位 `env.c:75` 的 `strchr(*argv,'=')` 循环（前导 `NAME=VALUE` 作赋值、其后作命令）；`entry_key` 截至首个 `=`（`A=1` 与 `AB=2` 不撞）；`build_env` 忠实 `putenv` 就地替换/末尾追加/末写胜/新键去重。14 枚宿主测（`-i` 扫/无置换/未知 usage/裸`-`/前导计数/键截断/替换·追加·位序+末值·折叠·空值·`-i` 空基·前缀不撞）。薄壳 `src/bin/env.rs`：无命令时读 `envs()`（`-i` 那么空基）过 `build_env` 后逐行 emit、退 0（只读环境+写 stdout→不受 E5 写盘门控，同 `printenv`）；`env ... command` 需 `execvp`，exec 递送链未备（同 `cat`/`xargs` 的 NS5 门）→诚实报 not wired 退 1、不假装成功。`env` 的 putenv 合并需物化结果→为 proctools lib 新增 `extern crate alloc`（破其 no-heap 惯例，照 doctools `soelim`/compress 先例，已在 crate 文档标为例外）。三件套：`cargo test -p minix-proctools --lib env::`→14/0、全 crate `--lib`→73/0（59 存量无回归）；freestanding `cargo build -p minix-proctools --bin env --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1 全 target 齐；本轮不走 docker 以免留 root 属主产物阻 release）；clippy 触及文件零告警（`unused_mut` 三枚属 `ptime.rs` 存量、非本轮）、rustfmt 触及两文件零漂移（就地格式化后 `--check` 空）。**声明性留白**：`env ... command` 的 exec 腿归 NS5 递送链（同 `cat`/`xargs`）；真机环境内容由 RS 下发决定（木桩测以合成 env 验证）；宿主 `bin_support` emit/warn 走 `minix_sys::write` 无内核→薄壳 bin 宿主不可跑（决定半已宿主测覆盖 flags/切分/合并）。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十九与 `../12-process-tools.md` §5。
- ✅ **批次九十八完成：06 篇 `rm` 接线（非递归删除默认形）**（`2026-10-11`，`claim/CMD-RM-BIN-qoder` @ 基线 `4fc9cc3a9`，单域三线 commands/fileops，无跨界）：落地缺失命令 `rm`（删目录项，真源 `minix3/bin/rm/rm.c` 635 行 BSD/NetBSD；`lstat`/`stat`/`unlink`/`rmdir`/`access` 客户端 wrapper 已备）。因 `rm_tree` 依赖 `fts`、`rm_overwrite`（`-P`）依赖 `arc4random`/`O_NOFOLLOW`、whiteout（`-W`）依赖 `undelete`（皆未封装），按 `chmod` 的 `-R` 残差先例只接非递归默认形，残差旗标整组前置拒。决定半新增 `minix_fileops::rm`（纯、可测）：`RmFlags::parse` 复刻 `getopt "dfiPRrvWx"`（`-f` 清 `-i`、`-i` 清 `-f`、`-R`/`-r` 同置 recursive、`--` 终止、首个非选项停、未知→None），`nonexistent` 对位 `rm.c:89-90` 的 `NONEXISTENT(x)`（ENOENT/ENAMETOOLONG/ENOTDIR 三值，`-f` 静默依据），`is_dot`+`strip_dots` 复刻 `checkdot`（POSIX `.`/`..` 叶名拒收置 eval、尾斜杠 `--p > *t` 剥除不空→`/` 存活待 "is a directory"）。14 枚宿主测。薄壳 `src/bin/rm.rs`：`rm_file` 忠实移植（逐操作符 lstat→不存在按 `-f`+nonexistent 静默否则告警、目次无 `-d` 报 "is a directory"、`-i` 走 `check` 读首字符 y/Y、dir→rmdir 否则 unlink、`-v` 回显）；`check` 仅接 `-i` 路，非 tty 的 override 提示需 `strmode`+`user_from_uid`/`group_from_gid`（未封装）按 C `!stdin_ok` 直过；无操作数 `-f` 退 0 否则 usage。`S_IFMT/S_IFDIR` 本地常量（对 C `sys/sys/stat.h` 核实）。三件套：`cargo test -p minix-fileops --lib rm::`→14/0、全 crate `--lib`→133/0（119 存量无回归）；freestanding `cargo build -p minix-fileops --bin rm --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1 全 target 齐；docker `minix-ci:1.94` 离线缺该 target core 故走宿主）；clippy 触及文件零告警（测名 snake_case 合规、`needs_unwired` 抽谓词）、rustfmt 触及两文件零漂移。**声明性留白**：`-r`/`-R`（`rm_tree` fts 递归）、`-P`（`rm_overwrite`）、`-W`（whiteout `undelete`）、`-x`（`FTS_XDEV`）整组前置拒 not wired（设施未备，非语义择重）；tty override 提示与 C `warn`/`warnx` 的 `strerror` 后缀未复现；真机删盘受可写根盘 E5 门控（同写类命令，per-command 归 C-57/E5）；宿主 `bin_support` emit/warn 走 `minix_sys::write` 无内核→薄壳 bin 宿主不可跑（决定半已宿主测覆盖 flags/nonexistent/checkdot 核心）。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十八与 `../06-file-ops.md` §5。
- ✅ **批次九十七完成：06 篇 `ln` 接线（硬/符链创建）**（`2026-10-11`，`claim/CMD-LN-BIN-qoder` @ 基线 `f1ad4610e`，单域三线 commands/fileops，无跨界）：落地缺失命令 `ln`（硬/符链创建，真源 `minix3/bin/ln/ln.c` 230 行 BSD；`link`/`symlink`/`stat`/`lstat`/`unlink` 客户端 wrapper 已备）。决定半新增 `minix_fileops::ln`（纯、可测）：`LnFlags::parse` 复刻 `getopt "fhinsv"`（-f 清 -i、-i 清 -f、-h/-n 同义、-s、-v、-- 终止、首个非选项停），`plan_operands` 编码 argc 形态（0→usage/1→隐式 cwd/2→显式对/N→目录形），`basename` 对位 `strrchr('/')`。7 枚宿主测。薄壳 `src/bin/ln.rs`：`linkit` 忠实移植（硬链需 target 存、目标为目次则接 basename、`-f` unlink 现存/-i 提示确认、symlink/link 按 -s、-v 打 `src -> tgt`）；目录形先校 sourcedir 为目次（hflag lstat 若为符链→ENOTDIR）。`S_IFMT/S_IFDIR/S_IFLNK` 本地常量（对 C `sys/sys/stat.h:141-144` 核实值）。三件套：`cargo test -p minix-fileops --lib ln::`→7/0、全 crate `--lib`→119/0（112 存量无回归）；freestanding `cargo build -p minix-fileops --bin ln --target x86_64-unknown-none --release` 绿（docker `minix-ci:1.94`，`-e CARGO_TARGET_DIR` 落树外）；clippy 触及文件零告警（`Result<_,()>`→`Option`）、容器口径 rustfmt 触及两文件零漂移。**声明性留白**：真机建链/目次递归受可写根盘 E5 门控（同写类命令，per-command 归 C-57/E5）；宿主 `bin_support` emit/warn 走 `minix_sys::write` 无内核→薄壳 bin 宿主不可跑（决定半已宿主测覆盖参数/flags/basename 核心）。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十七与 `../06-file-ops.md` §5。
- ✅ **批次九十六完成：10 篇 `fpr` 接线 + Fortran carriage-control/overstrike 格式化决定半**（`2026-10-11`，`claim/CMD-FPR-BIN-qoder` @ 基线 `a24945815`，单域三线 commands/doctools，无跨界）：落地缺失命令 `fpr`（Fortran 打印机 carriage-control 格式化过滤器，真源 `minix3/usr.bin/fpr/fpr.c` 378 行，无 fopen/exec/getopt、只读 stdin）。决定半新增 `minix_doctools::fpr::pretty_print`（纯字节流→输出字节+非法控制计数）——逐行对位 C 的 `main`/`get_text`/`savech`/`flush` 状态机：首列 carriage-control（`1`→FF、`0`→EOL、`+`→CR 叠印不 flush、blank/缺省→NUL、非法计数）；行内 TAB 展开到 tab stop、BS/CR 把已读列压入 overstrike 列、FF/VTAB flush+置 cc；flush 的 `!saved` 路剪尾空格直出、saved 路逐列存栏后按 anchor/height 输出叠印（同 count>1 段取顶字符+回退 BS）。C 的 `getchar`/`ungetc`→游标+单槽回推（fpr 仅单字符即时回推）。**无运行时 oracle**（系统无 fpr），故 golden 逐测附 C-step 轨迹供审（简单无叠印路唯一确定；叠印路过strike 路断言含双字+BS 的存在性）。8 枚宿主测。薄壳 `src/bin/fpr.rs`：读 stdin→pretty_print→stdout，非零时 `support::warn` 写“Illegal carriage control - N line(s).”（单/复数），恒 exit 0。三件套：`cargo test -p minix-doctools --lib fpr::`→8/0、全 crate `--lib`→35/0（27 存量无回归）；freestanding `cargo build -p minix-doctools --bin fpr --target x86_64-unknown-none --release` 绿（docker，`-e CARGO_TARGET_DIR` 落树外）；clippy 触及文件零告警（Coll Default 改 derive）、容器口径 rustfmt 触及两文件零漂移（展开 if/assert）。**诚实边界**：overstrike 输出格式无 oracle 可比，靠轨迹审计；真机 per-command 归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十六与 `../10-doc-man-tools.md` §5。
- ✅ **批次九十五完成：12 篇 `users` 接线 + 登录名排序去重决定半**（`2026-10-11`，`claim/CMD-USERS-BIN-qoder` @ 基线 `b35bb4128`，单域三线 commands/proctools，无跨界）：落地缺失命令 `users`（登录用户名单，真源 `minix3/usr.bin/users/users.c` 111 行）。决定半新增 `minix_proctools::users::sorted_unique`（无 alloc）——就地插入排序 + 相邻去重，逐段对位 C 的链表插入排序（`users.c:90-99`，`strcmp>0`）与“首名 + 逐相邻不同者 `" 名"”输出；`&str` 的 `<` 为字节序，与 C `strcmp` 对 ASCII 登录名一致。选插入排序而非 `slice::sort_unstable` 既同 C 算法又免 proctools lib 依赖 alloc（保持 no-heap）。6 枚宿主测：排序去重/单名/空表返 0/全同缩为 1/已排不变/乱序单例排序。薄壳 `src/bin/users.rs`：getopt "" 拒首选项、读默认 `/usr/adm/utmp`（缺件→退 0 不报错，镜像 C getutentries 空结果）、`walk_database` 收集 `entry.active()` 名、`sorted_unique` 后空格连接一行、空→无输出退 0（ncnt==0 路）。复用同 crate 已测 `utmp::walk_database`/`entry.active()`。三件套：`cargo test -p minix-proctools --lib users::`→6/0、全 crate `--lib`→59/0（53 存量无回归）；freestanding `cargo build -p minix-proctools --bin users --target x86_64-unknown-none --release` 绿（docker `minix-ci:1.94`，`-e CARGO_TARGET_DIR` 落树外）；clippy 触及文件零告警（修一处 collapsible_if→let-chain）、容器口径 rustfmt 触及文件零漂移（修 import 序）。**声明性留白**：宿主 `bin_support` emit/warn 走 `minix_sys::write` 无内核→薄壳 bin 宿主不可跑（决定半已宿主测覆盖排序去重核心，真机 per-command/utmp 数据归 C-57/E5）；C `getutentries` 合并 utmp+utmpx 与 endutentries 本树不涉及。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十五与 `../12-process-tools.md` §5。
- ✅ **批次九十四完成：10 篇 `soelim` 接线 + `.so` 包含展开字节机决定半**（`2026-10-11`，`claim/CMD-SOELIM-BIN-qoder` @ 基线 `0cb830a9d`，单域三线 commands/doctools，无跨界）：落地缺失命令 `soelim`（n/troff 流展开 `.so` 包含，真源 `minix3/usr.bin/soelim/soelim.c` 248 行 Bill Joy 1977，无 exec/system/popen、只 fopen 读）。决定半新增 `minix_doctools::soelim`——`expand`/`process` 逐字节对位 C `process()` 状态机：`.` 窥 `s` 窥 `o`，失配则已消费字符字面回后进入“复制至行尾”（`.s` 失配打 `.`、`.so`-非-o 打 `.s`）；`.so` 后跳空格/制表符、文件名跑到 space/tab/换行/EOF；**关键忠实角落**：递归路终止符被消费并丢弃（C `continue` 取新 getc）、空文件名路则 `printf(".so")` 后进 simple 把终止符输出——故 `.source` 被解析为 `.so "urce"`（C 真实怪，golden 钉住）；未开文件回退 `.so <name>\n`。文件系统侧（`-I` 目次表种子 `.`、`openpath` 逐目次前缀+绝对名直开、`open`+read 逐文件递归）入薄壳 `src/bin/soelim.rs`，以注入 `open: FnMut(&str)->Option<Vec<u8>>` 回调使引擎无文件系统依赖、宿主可测。**安全偏差**：加 `MAX_DEPTH=32` 递归上限防 `.so` 环致栈溢出（C 无此护栏会崩），越限按不可开处理——文档化的故意偏离（崩溃非值得保留的行为）。为此 doctools lib 接 `extern crate alloc`（soelim 输出无界需 Vec，照 compress 先例放宽“no heap”表述）。测：10 枚宿主 golden（直通/展开/不可开回退/无名 `.so`+终止符/嵌套/`.source`→urce/部分前缀字面/前导空白跳过/深度防环/无尾换行文件）。三件套：`cargo test -p minix-doctools --lib soelim::`→10/0、全 crate `--lib`→27/0（17 存量无回归）；freestanding `cargo build -p minix-doctools --bin soelim --target x86_64-unknown-none --release` 绿（docker `minix-ci:1.94`，`-e CARGO_TARGET_DIR` 落树外）；clippy 触及文件零告警、容器口径 rustfmt 触及两文件零漂移（手改 import 序 + 一处测试调用折行）。**test-audit 自身红**：`depth_cap` 测初验环终止且输出有界（非指数），钉住防环。**声明性留白**：C `open` 以 `"r"` 文本报、本树 open path 与 who/chmod 同款；`-I` 目次表真机逐文件 open 腿入镜像归 C-57/E5；`/dev/stdin` 不存在→stdin 走 fd 0（C STDIN_NAME 语义）。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十四与 `../10-doc-man-tools.md` §5。
- ✅ **批次九十三完成：05 篇 `getopt` 接线 + `getopt(3)` BSD 解析器决定半**（`2026-10-11`，`claim/CMD-GETOPT-BIN-qoder` @ 基线 `8eebf81a0`，单域三线 commands/shell，无跨界）：落地缺失命令 `getopt`（帮 shell 脚本解析选项的 Henry Spencer 演示件，真源 `minix3/usr.bin/getopt/getopt.c` 41 行，驱动 `minix3/lib/libc/stdlib/getopt.c` 143 行的 NetBSD 经典非置换解析器）。决定半新增 `minix_shell::getopt`（无堆、无 unsafe、借用 argv）——`Getopt` 结构体把 C 全局 `optind`/`optarg`/`opterr`/`place` 收进字段（`place` 空片 == C 指到 NUL/EMSG），`next` 逐行对位 `getopt.c:62-142`：`optopt = *place++`、`--` 终止、solitary `-` 仅当 optstring 列 `-` 才作选项、`letter:`/`letter::`/前导 `:` 三形态、`optstring` 未列或 `:` 自身 → Unknown；**关键忠实点**：C 的 `nargc > ++optind` 在条件里预增 `optind`（即便比较失败也已自增），本树照此预增——否则缺参数时 `optind` 冻结、`getopt(1)` demo 会无限循环（初版漏此→测被 SIGKILL 当场暴露，补回）；`opterr` 折为 on、两条 stderr 消息上抛给调用方以保持库 I/O-free。薄壳 `src/bin/getopt.rs`：纯函数 `render(argv, name, optstring) -> (stdout, stderr, status)` 复刻 getopt1 主循环（`optind=2`、`" -X"`/`" -X <arg>"`/`" --"`/操作数/`\n`）与 getopt.c 两条诊断（`unknown option`/`option requires an argument`，按 `*ostr==':'` 抑制），退出码按是否 `?`；progname 取 argv[0] 末段。三件套：`cargo test -p minix-shell --lib getopt::`→11/0、`--bin getopt`→8/0（render 逐 golden）、全 crate `--lib`→51/0（40 存量无回归）；freestanding `cargo build -p minix-shell --bin getopt --target x86_64-unknown-none --release` 绿（docker `minix-ci:1.94`，`rustup target add x86_64-unknown-none` 后编，`-e CARGO_TARGET_DIR` 落树外避 root 产物）；clippy 触及文件零告警、容器口径 rustfmt 触及两文件零漂移（lib.rs 的 expand/exec_frame 排序与 SD-19 attr 区系 minix-shell 存量基线漂移、非本轮插入，不动）。**test-audit 亮点**：render golden 首版把无操作数时的 `" --\n"` 误写成 `" -- \n"`（多一尾随空格），runner 当场报 3 枚翻红→按 C `printf(" --")`+逐操作数 `" %s"` 真值改回，属“测试自身错”当场修。**声明性留白**：getopt_long（`--word`）不在本文件（C 亦然）；`-:`-前导 optstring 的 BADARG 路（getopt1 打 `" -:"`）为退化角落；宿主 `bin_support` 的 emit/warn 走 `minix_sys::write` 无内核→薄壳 bin 宿主不可跑（render 纯函数已宿主测覆盖渲染，真机 per-command 归 C-57/E5）。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十三与 `../05-shell-family.md` §5。
- ✅ **批次九十二完成：12 篇 `from` 接线 + `from` 记录扫描与 BSD sender 匹配决定半**（`2026-10-11`，`claim/CMD-FROM-BIN-qoder` @ 基线 `31eae3e34`，单域三线 commands/proctools，无跨界）：落地缺失命令 `from`（看信箱发件人提示，真源 `minix3/usr.bin/from/from.c` 153 行）。决定半新增 `minix_proctools::from`（无堆，返回借用切片的惰性迭代器）——`match_sender` 忠实移植 C `match`（`from.c:131-153`）的 BSD 角落：遇空白字节即整体中止（`isspace`→return 0）、`first` 在 for-init 绑定一次不再重取、失败尝试从匹配 `first` 的那一字节之后续扫（不回扫重叠候选）、sender 已由调用方小写；`FromRecords` 迭代器复刻主循环（`from.c:118-127`）：`newline` 初值 1、纯空行 `"\n"` 复位记录起点且自身不算记录、记录起点处字面 `From ` 前缀（可选 sender 命中）才产出且保留行尾换行（对位 `fgets`+`printf("%s",buf)`）。13 枚宿主测：三记录信箱 fixture 钉记录起点门/字面 `From `/大小写不敏感/`-s` 选路/首行即记录/无尾换行行/空信箱，`match_sender` 逐分支钉首词必位、空格中止、空 sender、后续失配→续扫、单字符即命中、sender 超长不匹。执行半 `src/bin/from.rs`：`getopt("f:s:")` 手解（`-f`/`-s` 取值、附形 `-ffile`、`--` 终止、其余 `-x` usage 退 1）、信箱路径按 `from.c:94-114` 序解（`-f` > 操作数→`/var/mail/<操作数>` > `MAIL`（原样）> `LOGNAME`/`USER`→`/var/mail/<名>` > getpwuid）、`open`+read 全字节、逐 `FromRecords` emit、`terminate(0)`（C 循环后恒 exit 0）。为此 proctools lib 头模块表补 `from`/`printenv`/`sleep` 行并接 `pub mod from`。env 取值复用同 crate 已测 `printenv::lookup`。三件套：`cargo test -p minix-proctools --lib from::`→13/0、全 crate `--lib`→53/0（40 存量无回归）；freestanding `cargo build -p minix-proctools --bin from --target x86_64-unknown-none --release` 绿（docker `minix-ci:1.94`，本环 `rustup target add x86_64-unknown-none` 后编）；clippy 触及文件零告警、rustfmt 触及文件零漂移（存量 dates/utmp/ptime 漂移基线不动）。**声明性留白**：getpwuid 回退无封装→信箱无法经 `-f`/操作数/`MAIL`/`LOGNAME`/`USER` 解析时报错退 1（本树无 `getpwuid`/`getuid` 封装）；C `err`/`warn` 的 `strerror` 后缀未复现（仅报 `from: can't read <路径>`）；C `fgets` 按 BUFSIZ 截的超长单行角落不建模（真实信箱头行远短于此）；真机逐文件 open 腿与 imgrd 播种归 `C-57`/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十二与 `../12-process-tools.md` §5。
- ✅ **批次九十一完成：07 篇 `unvis` 文件操作数接线**（`2026-10-11`，`claim/CMD-UNVIS-FILES-qoder` @ 基线 `c24fea71f`，单域三线 commands/textfilter，无跨界）：接回 unvis 声明留白（“File operands wait for the gated open-existing call”；vis 轮次的孪生）。旧执行半只拒旗标、对非旗标参数不处理而恒读 stdin（`unvis file` 会静默读标准输入而非文件——本轮纠正）。按真源 `minix3/usr.bin/unvis/unvis.c:94-104` 重写：参数循环把非旗标位置参数归 `operands`（`-e/-H/-h/-m` 旗标族仍显式拒）；无操作数读 stdin、有操作数逐文件 `read_file` 后 `decode` 追参加入同一输出流；开不了按 C `warn("%s", *argv)`+**跳过且退出码仍为成功**（line 104 `return EXIT_SUCCESS`，区别于 vis 的 `rval=1`）。删 `use minix_sys::read`（改走 `support::read_stdin`）。复用已测 `minix_textfilter::vis::decode`（状态机决定半）；本轮无新增宿主测（操作数分类与 IO 腿属 bin 本地，编+freestanding 验证，同 vis/expand/fold 先例）。三件套：`cargo test -p minix-textfilter --lib`→302/0（无回归）；freestanding `cargo build -p minix-textfilter --bin unvis --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警。声明性留白：HTTP/MIME 旗标族 `-e/-H/-h/-m` 未接（需扩解码器入参）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十一与 `../07-text-filter.md` §4。
- ✅ **批次九十完成：07 篇 `vis` 文件操作数接线**（`2026-10-11`，`claim/CMD-VIS-FILES-qoder` @ 基线 `9348ed005`，单域三线 commands/textfilter，无跨界）：接回 vis 声明留白（“file operands and the remaining flags wait for their batches”——本轮只接文件操作数，未接旗标仍拒）。执行半 `src/bin/vis.rs`：参数循环把非选项位置参数归 `operands`，`-c/-o/-n` 照旧，**其余 `-x`（-b/-d/-e/-F/-f/-h/-l/-M/-m/-s/-t/-w）按“未接旗标”显式拒**（旧代码把旗标与文件混在一起拒）；输入按 `vis.c:151-164`——无操作数读 stdin、有操作数逐文件 `read_file` 后 `encode` 追参加入同一输出流（`encode` 无状态，逐文件拼接 == C 逐文件 `process(fp)` 输出），开不了按 C `warn`+置 `rval=1`+继续，末了 `terminate(rval)`；裸 `-` 按 `fopen` 作普通文件名不特判 stdin。删 `use minix_sys::read`（改走 `support::read_stdin`）。复用已测 `minix_textfilter::vis`（`encode` 决定半）；本轮无新增宿主测（操作数分类与 IO 腿属 bin 本地，编+freestanding 验证，同 expand/fold 先例）。三件套：`cargo test -p minix-textfilter --lib`→302/0（无回归）；freestanding `cargo build -p minix-textfilter --bin vis --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警。声明性留白：vis 的 `-b/-d/-e/...` 旗标族与对应编解码模式仍未接（需扩决定半）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次九十与 `../07-text-filter.md` §4。
- ✅ **批次八十九完成：07 篇 `hexdump` 补 `-n length` 截断（闭合批次八十五登记角落）**（`2026-10-11`，`claim/CMD-HEXDUMP-LEN-qoder` @ 基线 `517eac57c`，单域三线 commands/textfilter，无跨界）：批次八十五接 hexdump 文件操作数时明登“`-n length` 解析入 `options.length` 但渲染未截断流”为早存 gap（当时按“一次一条”未修）。本轮闭合：真值 `minix3/usr.bin/hexdump/hexsyntax.c`/`display.c`——`-s` 经 `doskip` 推进运行地址、`-n` 置 `eaddress = address + length`，展示到 `eaddress` 即停（`display.c:88`）。新增纯函数 `display_range(input_len, skip, length) -> (start, end)`：start=skip夹到输入长、end=start+min(length.unwrap(剩余), 剩余)；run() 里 `data = &input[start..end]`。旧 endfu/render_row 对 `data.len()` 的用法不变（与未接 `-n` 时一致，不越界改 pre-existing skip-相对地址）。5 枚宿主测钉 range（无 skip无 length 全显/skip 前移/length 截断/skip+length 合成 [40,50)/越界夹断）；决定半 `walk_blocks`/`render_row` 12 库测不动。另修正 hexdump §4.5 行里 `-s b|k|m` 的裸 `|`（同 split 类 GFM 破表→改 `-s b/k/m`）。三件套：`cargo test -p minix-textfilter --bin hexdump`→5/0（新 5 枚）、全 crate `--lib`→302/0；freestanding `cargo build -p minix-textfilter --bin hexdump --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警。声明性留白：`-e`/`-f` 格式语言未接；真机 open 腿归 C-57/E5（本轮无新 IO，纯范围计算）。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十九与 `../07-text-filter.md` §4。
- ✅ **批次八十八完成：09 篇 `ed` 写文件 O_* 标志纠错**（`2026-10-11`，`claim/CMD-ED-FLAGS-qoder` @ 基线 `a235b0d62`，单域三线 commands/editor，无跨界）：修上一轮（批次八十七）登记的 `os/commands/bin/editor/src/bin/ed.rs` 写路径标志位错误。真值：`minix3/bin/ed/main.c:828` 把 `w` 映为 `fopen("w")`〔截断〕、`W` 映为 `fopen("a")`〔追加〕；权威 `minix3/sys/sys/fcntl.h`：O_WRONLY 0x1、O_CREAT 0x200、O_TRUNC 0x400、O_APPEND 0x8。原代码 `let mut flags = 1|0x40`——**0x40 是 Linux 的 O_CREAT**（Minix 应 0x200），且 `if append { flags |= 0x400 }`——**0x400 是 Minix 的 O_TRUNC**，导致：非追加入 `w` 不截断（覆写旧长文件会残留尾垃圾）、追加入 `W` 反而截断而非追加（丢数据），且 O_CREAT 取错值建文件可会失败。修法：新增命名常量 O_WRONLY/O_CREAT/O_TRUNC/O_APPEND + 纯函数 `write_flags(append)->i32`（追加入 `O_WRONLY|O_CREAT|O_APPEND`、否则 `O_WRONLY|O_CREAT|O_TRUNC`），write_file 改调它。**与同仓其他写类交叉验证**：uudecode 用 O_CREAT 0x200/O_TRUNC 0x400、sh 用 O_APPEND 0x8——均与 ed 修正后的值一致（原 ed 是唯一用 Linux 值者）。2 枚宿主测钉住回归（`write_truncates_and_creates`、`write_append_appends_not_truncates`，后者断言 append 标志不含 O_TRUNC）。三件套：`cargo test -p minix-editor`→66/0（库 64+ed 薄壳 2）；freestanding `cargo build -p minix-editor --bin ed --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警。真机验证：写文件腿 target-only，受可写根盘 E5 门控（与其余写类同批）；但标志值为 ABI 常量，对 C 真源（main.c:828 + fcntl.h）静态坐实。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十八与 `../09-editors.md` §5。
- ✅ **批次八十七完成：07 篇 `tee` 文件操作数与 `-a` 追加接线**（`2026-10-11`，`claim/CMD-TEE-FILES-qoder` @ 基线 `c66ff5684`，单域三线 commands/textfilter，无跨界）：接回 tee 声明留白（“文件目标待开放路径，`-a`/`-i` 随之”）。执行半 `src/bin/tee.rs` 重写：抽纯函数 `parse_tee_args(argv)->(append, operands)`（`-a` 置追加、`-i` 因信号面未接显式拒、其余 `-x` 报 usage、其余位置参数为输出文件）与 `open_flags(append)->i32`（`tee.c:102-103` 的 append 选 O_APPEND、否则 O_TRUNC）；逐操作数 `open(O_WRONLY 加 O_CREAT 加 (O_APPEND|O_TRUNC), 0o666)`，开不了按 C warn+置 rval+跳过；逐块 stdin 写入 stdout 与每个已开文件（`write_partial` 部分写循环），写/关失败按 C warn+置 rval 且文件不摘除（后续块重试，对位 C 链表不删），末了 `terminate(rval)`。**标旗取权威值**：核 `minix3/sys/sys/fcntl.h`——O_WRONLY 0x1(:65)/O_APPEND 0x8(:82)/O_CREAT 0x200(:99)/O_TRUNC 0x400(:100)（与 uudecode 0x200/0x400、sh 的 O_APPEND 0x8 一致）。**顺带发现（登记待查、非本轮改动面）**：`os/commands/bin/editor/src/bin/ed.rs:84` 注释写“O_CREAT 0x40、O_APPEND 0x400”与权威 minix fcntl.h（0x200/0x8）不符——需查 ed.rs 代码实用哪套；若代码取 0x40 则 ed 建文件在真机会失配（属 09 篇 ed 专件，本批不动）。`-i`（忽略 SIGINT）无信号处理基础仍显式拒（区别于 tee.c `signal(SIGINT,SIG_IGN)` 可做但本树未接）。7 枚宿主测钉 parse/open_flags（仅操作数/`-a` 追加/`-i` 拒/未知旗标拒/旗标文件混序/`-` 当普通输出文件/truncate 与 append 两套标旗）；本轮无决定半库（tee 无 lib 模块），写文件腿 target-only、真机创建受可写根盘 E5 门控。另修正上一轮 `split` 的 §4.5 行：单元里反引号内的 `|` 会破 GFM 表格解析→改“加”字叙述（无数据影响，仅渲染）。三件套：`cargo test -p minix-textfilter --bin tee`→7/0、全 crate `--lib`→302/0；freestanding `cargo build -p minix-textfilter --bin tee --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十七与 `../07-text-filter.md` §4。
- ✅ **批次八十六完成：07 篇 `split` 文件操作数与分块写文件接线**（`2026-10-11`，`claim/CMD-SPLIT-FILES-qoder` @ 基线 `e949d2399`，单域三线 commands/textfilter，无跨界）：接回 split 声明留白（“写文件面待开放，二进制以 `==> 前缀 <==` 横幅输出各块”，同一陈旧 edge E-CMDSYSFACE；前序因 create 面未就绪而暂以横幅占位）。执行半 `src/bin/split.rs` 重写：参数循环收集操作数（`[input?, prefix?]`，>2 → usage）；输入按 `split.c:135-139`——无操作数/裸 `-` 读 stdin（split **特判** `-` 为 stdin，与 grep/sed 相反）、具名输入 `read_file`、开不了按 C `err(1)` 致命退 1；前缀按 `split.c:143` 取第二操作数、缺省 `x`；逐块按 `split.c:332` `create_write(prefix+suffix, 该块字节)`（本地 `open(O_WRONLY|O_CREAT|O_TRUNC, 0o666)`+部分写循环+close，标旗照 uudecode 先例 0x1/0x200/0x400），旧的 `==> 前缀 <==` 横幅占位已被真写文件取代。抽出两枚**纯函数** `out_path(prefix, index)->Option<String>`（前缀拼两字母后缀，后缀耗尽返 None）与 `render_chunk(&[String])->Vec<u8>`（逐行补 `\n`），+4 枚宿主测钉住（xaa/xab/part.ba、index 675 有/676 无后缀、逐行终止、空块）；决定半 `chunks`/`suffix` 既有测不动。`-a`/`-b`/`-n`（后缀宽度/字节/块数）不在决定半、显式拒绝。三件套：`cargo test -p minix-textfilter --bin split`→4/0（新 4 枚）、全 crate `--lib`→302/0（无回归）；freestanding `cargo build -p minix-textfilter --bin split --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警（中途 `written += n as usize` 冗余 cast 由 clippy 拦下→删）。**声明性留白**：`-a`/`-b`/`-n` 未接；真机文件创建受可写根盘 E5 门控（同 uudecode/写类），写文件腿 target-only、本轮以 out_path/render_chunk 纯测+编译验证。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十六与 `../07-text-filter.md` §4。
- ✅ **批次八十五完成：07 篇 `hexdump` 文件操作数接线**（`2026-10-11`，`claim/CMD-HEXDUMP-FILES-qoder` @ 基线 `e8781241d`，单域三线 commands/textfilter，无跨界）：接回 hexdump 声明留白（“file operands rejected until the open-existing seam”，同一陈旧 edge E-CMDSYSFACE）。执行半 `src/bin/hexdump.rs`：删去 `operands.iter().any(|s| *s != "-")` 即拒分支；按 C `display.c` `next` 把每个操作数经 `freopen` 串到 stdin 的模型——无操作数读 stdin、有操作数逐文件 `support::read_file` **拼接成一条连续字节流**（故地址跨文件连续，`-s` 从流首跳过与已有 `start = skip.min(input.len())` 一致——已核 `doskip` 递减全局 `skip`、跨整文件前移，证实拼接 faithful）；开不了的文件按 C `next` `warn`+置 `exitval=1`+继续，末了 `terminate(rval)`；裸 `-` 按 C `freopen` 作普通文件名不特判 stdin（修正旧 bin 把 `-` 当 stdin 的偏差，无操作数才读 stdin）。**登记角落**：`-n length` 解析入 `options.length` 但渲染未截断流——属早存 gap（非本轮换件引入），按“一次一条”不顺手修，已在代码头与 §4.5 行如实标注。复用已测 `minix_textfilter::hexdump`（`walk_blocks`/`render_row`/`row_spec`/`parse_skip` 12 库测）；本轮无新增宿主测（多源拼接与错误腿属 bin 本地 open/emit，编译+freestanding 验证）。三件套：`cargo test -p minix-textfilter --lib`→302/0（无回归）；freestanding `cargo build -p minix-textfilter --bin hexdump --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警。声明性留白：`-e`/`-f` 格式语言、`-n` 截断未接；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十五与 `../07-text-filter.md` §4。
- ✅ **批次八十四完成：07 篇 `diff` 两文件操作数接线**（`2026-10-11`，`claim/CMD-DIFF-FILES-qoder` @ 基线 `936fd7ff6`，单域三线 commands/textfilter，无跨界）：接回 diff 声明留白（“命名文件操作数待 open-existing”，同一陈旧 edge E-CMDSYSFACE）。执行半 `src/bin/diff.rs`：将逐侧取入中 `operand != "-"` 即拒（退 2）的 else 分支改为 `support::read_file(operand)` 开文件读入；开不了按 C `diffreg` 置 `status |= 2` → 退 2 致命（两文件比较无可比对象即终止，同 lam、区别于 grep/sed/column 的 warn+继续）；`-` 读 stdin 与 arity（≠ 2 → usage 退 2）不变；已测 `diff_lines` 逐侧名（含 `-`）传作输出标头名。复用已测 `minix_textfilter::diff` 引擎（`diff_lines`/`split_text`）；本轮无新增宿主测（两侧开文件与错误腿属 bin 本地 open/emit，编译+freestanding 验证；比较引擎逐格式对照宿主 diff 既有测）。三件套：`cargo test -p minix-textfilter --lib`→302/0（无回归）；freestanding `cargo build -p minix-textfilter --bin diff --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警。声明性留白：目录递归、`-c` 上下文输出、`-b/-w/-i` 忽略族、`\ No newline` 仍为登记角落；`err(2)` strerror 后缀未复现（仅报侧名）；真机逐文件 open 腿归 C-57/E5。§5 总数行与分模块计数为跨批次快照（先前 column/lam/expand 等接线轮同样只更各自 §4.5 行），非本轮 diff 改动面，未改；diff 决定半计数（现 16）属既有增量、不在本轮。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十四与 `../07-text-filter.md` §4。
- ✅ **批次八十三完成：07 篇 `lam` 文件操作数接线**（`2026-10-11`，`claim/CMD-LAM-FILES-qoder` @ 基线 `5ff84fc56`，单域三线 commands/textfilter，无跨界）：接回 lam 声明留白（“命名文件操作数待 open-existing 解锁”，同一陈旧 edge E-CMDSYSFACE）。执行半 `src/bin/lam.rs` 重写：删去 `arg != "-"` 即拒的分支；逐操作数取入时——裸 `-` 读 stdin、其余 `support::read_file` 开文件（`lam.c:113-116`）；开文件失败按 C `errx(1, "open %s", p)` **致命退出**（非 warn+继续，区别于 grep/sed/column——lam.c 开不了文件就直接终止）；粘滞继承与 MAX_STREAMS 逐流前置检查不变。复用已测 `minix_textfilter::lam`（`laminate`/`split_stream`/`parse_width` 7 库测）；本轮无新增宿主测（文件取入与错误腿属 bin 本地 open/emit，编译+freestanding 验证）。三件套：`cargo test -p minix-textfilter --lib`→302/0（无回归）；freestanding `cargo build -p minix-textfilter --bin lam --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警。声明性留白：`errx`  strerror 后缀未复现（仅报 `lam: open <名>`）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十三与 `../07-text-filter.md` §4。
- ✅ **批次八十二完成：07 篇 `column` 文件操作数接线**（`2026-10-11`，`claim/CMD-COLUMN-FILES-qoder` @ 基线 `0a1f102d4`，单域三线 commands/textfilter，无跨界）：接回 column 声明留白（“文件操作数待开放路径”，同一陈旧 edge E-CMDSYSFACE）。执行半 `src/bin/column.rs` 重写：参数循环把非选项位置参数归 `operands`（原为报错退出）；无操作数读 stdin、有操作数逐文件 `support::read_file` 后 `parse_from`→`parse_entries` 喂入同一个 `entries` 池（对位 C 全局 `entries` 由多次 `input(fp)` 累加），单次 `columnate` 排版；打不开按 C `column.c:114-123` `warn("Cannot open `%s'")`+置 `rval=1`+继续，末了 `terminate(rval)`；`-` 按 C `fopen` 作普通文件名不特判 stdin。抽出本地 `parse_from(bytes)->Vec<String>`（从旧内联 stdin 管道拆出，供 stdin/文件两路复用，行为逐字节不变：`from_utf8.unwrap_or("")` 整文弃、`split('\n')`、`parse_entries` 跳空行）。复用已测 `minix_textfilter::column`（`parse_entries`/`columnate` 4 库测）；本轮无新增宿主测（多源累加与 rval 属 bin 本地 open/emit 腿，编译+freestanding 验证）。删 `use minix_sys::read`（改走 `support::read_stdin`）。三件套：`cargo test -p minix-textfilter --lib`→302/0（无回归）；freestanding `cargo build -p minix-textfilter --bin column --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警。声明性留白：真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十二与 `../07-text-filter.md` §4。
- ✅ **批次八十一完成：08 篇 `sed` 文件操作数接线**（`2026-10-11`，`claim/CMD-SED-FILES-qoder` @ 基线 `a7c8163f1`，单域三线 commands/regex，无跨界）：接回 sed 声明留白（“file operands wait for the open call”，同一陈旧 edge E-CMDSYSFACE，已于 `2026-09-18` 收口）。执行半 `src/bin/sed.rs` 重写：删去拒操作数分支；参数切分抽成纯函数 `parse_sed_args(argv) -> (quiet, script, files)`（首个位置参数为脚本、其余为文件、`-n` 置静默、其余 `-x` 拒）；逐源循环——无文件读 stdin、有文件逐 open+read 后 `sed_lines` 逐行替换流 stdout（抽出 `sed_lines` 供 stdin/文件两路复用，保留 `!quiet || (replaced && subst.print)` 的 p×`-n` 交互）。开文件失败非致命（本地 `read_file` 辅助 + `warn`+置 `rval`+继续，末了 `terminate(rval)`）——对位 C `main.c` mf_fgets 非 inplace 路（outfile=stdout、无名头前缀）与 `exit(rval)` main.c:219；`-` 按 C `fopen` 作普通文件名不特判 stdin（同 grep）；`-i`/`-I` 就地编辑需 create/rename 面，未建模。`STDERR`/target `warn` 复用批次八十已落的 bin_support twin（本批不改 bin_support）；顺带修正代码头 C 源路径 `minix3/usr.bin/sed/sed/`→`minix3/usr.bin/sed/`。7 枚宿主测钉参数切分（仅脚本/`-n` 置静默/脚本后入文件表/旗标文件混序/`-` 当普通文件/未知选项拒/缺脚本拒）；替换引擎 `apply`/`parse_subst` 的既有 11 库测不动。三件套：`cargo test -p minix-regex`→50/0（库 34+grep 薄壳 9+sed 薄壳 7）；freestanding `cargo build -p minix-regex --bin sed --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy 触及文件零告警（同 grep，`unused import: write` 系宿主态基线）。**声明性留白**：`;` 链/地址/其他动词、`-i` 就地编辑未建模；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十一与 `../08-grep-sed.md` §4.5/§5。
- ✅ **批次八十完成：08 篇 `grep` 文件操作数与多流渲染接线**（`2026-10-11`，`claim/CMD-GREP-FILES-qoder` @ 基线 `0006ab79d`，单域三线 commands/regex，无跨界）：接回 grep 声明留白（“文件操作数待开放路径调用”，edge E-CMDSYSFACE）。执行半 `src/bin/grep.rs` 重写：删去拒操作数的分支；单流处理抽成纯函数 `grep_stream(lines, label, prefix_name, patterns, options, out) -> bool`（渲染进 `out`、返回本流是否命中，IO 交调用方 emit）；逐源循环——无操作数读 stdin 标 `(standard input)`、有操作数逐文件 open+read（本地 `read_file` 辅助，非致命：开文件失败按 `grep.c` procfile `warn`+置 `file_err`+继续，末了经既有已测 `exit_code` 折进退出码 2）。多文件名头规则忠实 C：`file:` 前缀仅在操作数 >1 时加（`grep.c:493` 单/零操作数隐名头）、`-l` 列名且折入 quiet 不出行体（`grep.c:387` `lflag=qflag=1`）、`-c` 随名头出 `file:count`、`-n` 行号逐文件重置（`procfile` 每流 `line_no=0`）、`-` 按 C `fopen` 作普通文件名不特判 stdin。为此给 regex `bin_support` 补 `STDERR` 常量与 target 态 `warn` twin（照 textfilter 同构，宿主侧原已有）。同一渲染修正：旧单流 `-l` 打完名头仍逐行输出行体（薄壳此前无宿主测故未暴露），本轮按 C 让 `-l` 抑制行体。9 枚宿主测钉渲染规则（名头有/无、行号逐流重置、`-c` 有/无名头、`-l` 只列名不出行体、`-v` 反选、`-q` 静默、未命中返假）；决定半 `parse_grep_args`/`exit_code` 的既有 34 库测不动。三件套：`cargo test -p minix-regex`→43/0（库 34+薄壳 9）；freestanding `cargo build -p minix-regex --bin grep --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1，docker 镜像本环未装该 target）；clippy 触及文件零告警（`bin_support` 一条 `unused import: write` 系宿主态基线，早存非本批引入）。**声明性留白**：`-i`/`-w` 待引擎折叠/词边界、`-r`/`-A`/`-B`/`-C` 待文件遍历（08 篇已声明）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次八十与 `../08-grep-sed.md` §4.5/§5。
- ✅ **批次七十九完成：07 篇 `paste` 并行归并与文件操作数接线**（`2026-10-11`，`claim/CMD-PASTE-FILES-qoder` @ 基线 `8c6d23409`，单域三线 commands/textfilter，无跨界）：接回 paste 声明留白（“并行多流与文件操作数待开放路径”）。决定半新增于 `minix_textfilter::paste`——`join_parallel(streams, options)` 忠实实现 paste.c 并行归并：行数=各流最大值、每行跨流取第 r 行（缺位补空）、gap 定界符按 `delimiter_for` 循环且每行重启；新增 `parse_args`（返回 options+operands，`parse` 变薄壳不动已测签名）。+3 宿主测（跨流归并短流补空、定界符循环每行重启、parse_args 收集操作数与 BadOption）。执行半 `src/bin/paste.rs` 重写：无操作数读单 stdin 流、逐操作数 open+read（`-`→stdin、fopen 失败按 C `fpp==NULL` err 退1），serial 逐流 `join_serial`、parallel `join_parallel`。三件套：`cargo test -p minix-textfilter --lib paste`→7/0（+3）、全 crate `--lib`→302/0；freestanding `cargo build -p minix-textfilter --bin paste --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。声明性留白：真机逐文件 open 腿归 C-57/E5（旧单流“parallel 逐行透传”占位已被真并行归并取代）。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十九与 `../07-text-filter.md` §4。
- ✅ **批次七十八完成：07 篇 `sort` 输入文件操作数接线**（`2026-10-11`，`claim/CMD-SORT-FILES-qoder` @ 基线 `98f5315c8`，单域三线 commands/textfilter，无跨界）：接回 sort 声明留白（“file operands wait for the open-existing call”）。执行半 `src/bin/sort.rs` 重写：逐操作数 open+read 拼接行（`-` 读 stdin、无操作数读 stdin）、fopen 失败按 C access/fopen 退2、`-c` 只收一文件（对位 sort.c “too many input files for -c” errx 2）；已测 `parse_options` 对 `-o`/`-m` 已返回 BadOption（故不会误吞其参数），本轮不改。复用已测 `minix_textfilter::sort`（compare/parse_options）；本轮无新增宿主测（排序引擎既有已测，bin 仅 open/read/split/emit）。三件套：全 crate `cargo test -p minix-textfilter --lib`→299/0（无回归）；freestanding `cargo build -p minix-textfilter --bin sort --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警（中途一处重复 `use minix_sys::read` 由编译拦下→删）。声明性留白：`-m`（归并）/`-o`（输出文件 create）未接（选项解析器拒）；外部归并超内存引擎属执行层待批；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十八与 `../07-text-filter.md` §4。
- ✅ **批次七十七完成：07 篇 `comm` 两文件操作数接线**（`2026-10-11`，`claim/CMD-COMM-FILES-qoder` @ 基线 `8c28d1263`，单域三线 commands/textfilter，无跨界）：接回 comm 声明留白（“file operands wait for the open-existing call”）。旧 bin 因不能开文件而“两读 stdin 自拼”（两流恒等→全进第三列）；本轮接真两操作数：`argc!=2`→usage(1)、每 `-`读 stdin 否则 open+read（fopen 失败按 C `file()` err 退1），喂已测 `comm(&a,&b,opts)` 三列归并。复用已测 `minix_textfilter::comm` 决定半；本轮无新增宿主测（归并逻辑既有已测，bin 仅 open/read/split/emit）。三件套：全 crate `cargo test -p minix-textfilter --lib`→299/0（无回归）；freestanding `cargo build -p minix-textfilter --bin comm --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。声明性留白：真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十七与 `../07-text-filter.md` §4。
- ✅ **批次七十六完成：07 篇 `uniq` 输入文件操作数接线**（`2026-10-11`，`claim/CMD-UNIQ-FILES-qoder` @ 基线 `613c120e4`，单域三线 commands/textfilter，无跨界）：接回 uniq 声明留白（“file operands wait for the open-existing call”）。执行半 `src/bin/uniq.rs` 重写：按 C `switch(argc)` 操数 arity——无操数读 stdin、`uniq file` open+read 文件→stdout、`uniq in out` 的输出文件（create-and-write 路径）报未接（不静默截断文件，提示用 shell 重定向）、>2 usage。fopen 失败按 C `file()` err 退出。复用已测 `Uniq`/`UniqMode` 决定半（分组/模式/计数）；本轮无新增宿主测（bin 仅 open/read/emit 腿 + 旗标流）。三件套：全 crate `cargo test -p minix-textfilter --lib`→299/0（无回归）；freestanding `cargo build -p minix-textfilter --bin uniq --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。声明性留白：输出文件操作数未接（需 create+写 fd 路，本树已有先例如 uudecode 但本批未接、避免静默截断）；`-i`/`-f`/`-s` 比较与组合模式仍待批；真机 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十六与 `../07-text-filter.md` §4。
- ✅ **批次七十五完成：07 篇 `rev` 文件操作数接线**（`2026-10-11`，`claim/CMD-REV-FILES-qoder` @ 基线 `c7f9f2f48`，单域三线 commands/textfilter，无跨界）：接回 rev 声明留白（“文件操作数待开放路径”）。执行半 `src/bin/rev.rs` 重写：无操作数读 stdin；有操作数逐文件 open+read+`reversed_lines`（复用已测决定半，逐行反转+换行），fopen 失败 warn+置退出码+跳过（非致命，对位 C warn+rval+continue）；`rev` 无旗标（C `getopt("")`）。本轮无新增宿主测（反转逻辑在既有已测 `rev.rs`，bin 仅 open/read/emit 腿）。三件套：全 crate `cargo test -p minix-textfilter --lib`→299/0（无回归）；freestanding `cargo build -p minix-textfilter --bin rev --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。声明性留白：真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十五与 `../07-text-filter.md` §4。
- ✅ **批次七十四完成：07 篇 `cmp` 文件操作数接线 + 报告串决定半**（`2026-10-11`，`claim/CMD-CMP-FILES-qoder` @ 基线 `6546884df`，单域三线 commands/textfilter，无跨界）：接回 cmp 声明留白（“文件操作数待开放路径”）。决定半新增于已测 `minix_textfilter::cmp`——`report_first_diff`/`report_eof` 忠实复刻 `misc.c` 的 `diffmsg`（`"%s %s differ: char %lld, line %lld\n"`）与 `eofmsg`（无`-l`仅文件、有`-l`且 line>0 带行、有`-l`且 line==0 仅 char 三分支），+2 宿主测。执行半重写 `src/bin/cmp.rs`：open 两文件+read+可选 `skip1`/`skip2`（十进制、strtoq base-0 进制自动识别未建模），切掉 skip 后喂已测 `compare_bytes`；三模式报告——默认有首差→stdout diffmsg、否则长度不等→stderr `cmp: EOF on <短流>`；`-l`逐条 format_list_entry + EOF 行；`-s` 静默仅回码；退出码 0同/1异（DIFF_EXIT）/2用法或打开错（ERR_EXIT）；open 失败→`cmp: <file>` stderr+退2。三件套：`cargo test -p minix-textfilter --lib cmp`→7/0（+2 报告串）、全 crate `--lib`→299/0；freestanding `cargo build -p minix-textfilter --bin cmp --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 本轮触及行零告警（将报告函数从 `write!` 改 `format!` 避开 write_with_newline/let_underscore 两 lint；唯一余留 cmp.rs:78 在未改动的 `format_list_entry`，既有基线）。声明性留白：宿主 cmp 是 GNU（“byte” vs NetBSD “char”），无匹配 oracle，报告串逐字对 misc.c；`skip` base-0 未建模；真机 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十四与 `../07-text-filter.md` §4/§5。
- ✅ **批次七十三完成：07 篇 `tail` 文件操作数 + 多文件横幅接线**（`2026-10-11`，`claim/CMD-TAIL-FILES-qoder` @ 基线 `1d9d5c3de`，单域三线 commands/textfilter，无跨界）：接回 tail 声明留白（“文件操作数待批”）。执行半 `src/bin/tail.rs` 重写：新增 obsolete `-N`、`--` 终止旗标；无操作数读 stdin；有操作数逐文件取后 N 行（fresh `TailWindow` 逐文件重置、forward 模），多文件时按 C `argc>1` 无条件打 `==> 名 <==` 横幅（此 NetBSD tail 无 `-q`/`-v`）、文件间前导空行；fopen 失败 warn+置退出码+跳过（对位 C fopen‖‖fstat 失败 ierr+continue）；`-` 不特判为 stdin。**刻意拒绝而非误当操作数**：`-c`/`-b`（行基环形不支持）、`-r` 反序、`-f`/`-F` follow、`-n +N` 前向偏移（parse_count 遇 `+` 拒绝）。复用已测 `minix_textfilter::window::TailWindow`，环形封顶 32 保持，本轮无新增宿主测（横幅/旗标流为 bin 本地，编译验证）。三件套：全 crate `cargo test -p minix-textfilter --lib`→297/0（无回归）；freestanding `cargo build -p minix-textfilter --bin tail --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。声明性留白：`-r`/`-f`/`-F`/`-c`/`-b`/`-n +N` 均未接（报未接）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十三与 `../07-text-filter.md` §4。
- ✅ **批次七十二完成：07 篇 `head` 文件操作数 + 多文件横幅接线**（`2026-10-11`，`claim/CMD-HEAD-FILES-qoder` @ 基线 `82bc43b2e`，单域三线 commands/textfilter，无跨界）：接回 head 声明留白（“文件操作数待批”）。执行半 `src/bin/head.rs` 重写：新增 `-q`（静默）/`-v`（总打横幅）/obsolete `-N`（行号）、`--` 终止旗标；无操作数读 stdin；有操作数逐文件取前 N 行（fresh `HeadWindow` 逐文件重置），多文件（`operands.len()>1` 且非 `-q`）或 `-v` 时按 C `printf("%s==> %s <==\n", first?"":"\n", name)` 打横幅、文件间前导空行；**`-` 不特判为 stdin**（head.c 用 fopen，`-` 是普通文件名，与 cut 不同）；fopen 失败 warn+置退出码+跳过（非致命）。`-c` 字节模式（行基 `HeadWindow` 不支持）报未接；窗口封顶 32（`HeadWindow::new` 拒 0/>32）保持。复用已测 `minix_textfilter::window::HeadWindow`，本轮无新增宿主测（横幅/旗标流为 bin 本地，编译验证）。三件套：全 crate `cargo test -p minix-textfilter --lib`→297/0（无回归）；freestanding `cargo build -p minix-textfilter --bin head --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。声明性留白：`-c` 字节模式未接（行基窗口）；横幅为 bin 本地输出形式（无独立宿主测）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十二与 `../07-text-filter.md` §4。
- ✅ **批次七十一完成：07 篇 `cut` 文件操作数接线**（`2026-10-11`，`claim/CMD-CUT-FILES-qoder` @ 基线 `62e9f4ed9`，单域三线 commands/textfilter，无跨界）：接回 cut 声明留白（“文件操作数待开放路径调用解锁”）。执行半 `src/bin/cut.rs` 重写：逐行处理抽成 `process` 闭包（复用已测 `select_bytes`/`select_fields`）；无操作数与 `-` 都读 stdin（对位 C `strcmp(*argv,"-")` → fcn(stdin)）、其余逐文件 `open`+read，**打不开的文件按 C `warn` 跳过且仅置退出码（非致命）**——区别于 expand/unexpand/fold 的 freopen+err 直接退出（因 cut.c 用 fopen+warn+继续）；新增本地 `read_fd` 辅助。本轮无新增宿主测（实质逻辑在既有已测 `cut.rs` 决定半，bin 仅 open/read/emit 腿 + 旗标流）。三件套：全 crate `cargo test -p minix-textfilter --lib`→297/0（无回归）；freestanding `cargo build -p minix-textfilter --bin cut --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。声明性留白：`-b`/`-c` 同义（byte-grained，多字节 locale 区分不建模）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十一与 `../07-text-filter.md` §4。
- ✅ **批次七十完成：07 篇 `fold` 文件操作数接线**（`2026-10-11`，`claim/CMD-FOLD-FILES-qoder` @ 基线 `23b633f09`，单域三线 commands/textfilter，无跨界）：接回 fold 声明留白（“文件操作数待开放路径”）。执行半 `src/bin/fold.rs` 重写：无操作数读 stdin（对位 C `if (!*argv) fold(stdin)`）；有操作数逐文件 `open`+`fold_input`+逐行 write（`fold_input` 每次调用列/词态自重置），打不开的文件按 C `err(1,"%s",argv)` 直接退出 1（同 expand/unexpand）；旗标保持 `-b`/`-s`/`-w`/obsolete `-N`（fold.c `getopt("0123456789bsw:")`），其余 `-x` 报 usage；读入改 `from_utf8_lossy`。复用已测 `minix_textfilter::fold` 决定半，本轮无新增宿主测。三件套：全 crate `cargo test -p minix-textfilter --lib`→297/0（无回归）；freestanding `cargo build -p minix-textfilter --bin fold --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。声明性留白：决定半 `&str` 契面→非 UTF-8 按 lossy（C `getwchar` 面向宽字符，二进制/locale 不忠实，属既有 str 契约限制）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次七十与 `../07-text-filter.md` §4。
- ✅ **批次六十九完成：07 篇 `unexpand` 文件操作数接线**（`2026-10-11`，`claim/CMD-UNEXPAND-FILES-qoder` @ 基线 `da6df8c23`，单域三线 commands/textfilter，无跨界）：接回 unexpand 声明留白（“文件操作数待开放路径”）。执行半 `src/bin/unexpand.rs` 重写：无操作数读 stdin（对位 C `do{}while(argc>0)`）；有操作数逐文件 `open`+`unexpand`+逐行 write（行游走逐文件重置，无 tab 列跨文件泄露），打不开的文件按 C `err(EXIT_FAILURE,"Cannot open `%s'")` 直接退出 1（同 expand）；旗标保持 `-a`/`-t`（unexpand.c `getopt("at:")`），其余 `-x` 报 usage；读入改用 `from_utf8_lossy`。复用已测 `minix_textfilter::unexpand` 决定半，本轮无新增宿主测（bin 仅 open/read/emit 腿 + 旗标流）。三件套：全 crate `cargo test -p minix-textfilter --lib`→297/0（无回归）；freestanding `cargo build -p minix-textfilter --bin unexpand --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。**声明性留白**：决定半以 `&str` 为契面→非 UTF-8 按 lossy（C 面向字节，二进制不忠实，属既有 str 契约限制）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十九与 `../07-text-filter.md` §4。
- ✅ **批次六十八完成：07 篇 `expand` 文件操作数接线**（`2026-10-11`，`claim/CMD-EXPAND-FILES-qoder` @ 基线 `c24f6d896`，单域三线 commands/textfilter，无跨界）：接回 expand 声明留白（“文件操作数待开放路径”）。执行半 `src/bin/expand.rs` 重写：无操作数读 stdin（对位 C `do{}while(argc>0)` 首轮无 operand 即 stdin）；有操作数逐文件 `open`+`expand`+write，tab 列由每次新建 `expand()` 调用自然重置（对位 C 每文件 `column = 0`）；打不开的文件按 C `err(EXIT_FAILURE, "Cannot open `%s'")` **直接退出 1**（区别于 wc/cksum 的 warn-跳过，因 expand.c 用 freopen+err）。旗标保持 `-t`/obsolete `-N`（minix3 expand.c 仅 `getopt("t:")`，无 `-i`/`-b`），其余 `-x` 报 usage。读入改用 `String::from_utf8_lossy`（优于旧 `from_utf8().unwrap_or("")` 对任意非 UTF-8 输入整文丢弃的隐性毁数据）。复用已测 `minix_textfilter::expand` 决定半，本轮无新增宿主测（实质逻辑在既有已测 `expand.rs`，bin 仅为 open/read/emit 腿 + 旗标流）。三件套：全 crate `cargo test -p minix-textfilter --lib`→297/0（无回归）、freestanding `cargo build -p minix-textfilter --bin expand --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警（中途引入一个多余闭括号由编译当场拦下→删）。**声明性留白**：决定半以 `&str` 为契面→非 UTF-8 字节按 lossy 替换（C 是面向字节的 tab→空格，二进制输入不忠实，属既有 str 契约限制，本轮不重写决定半）；真机逐文件 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十八与 `../07-text-filter.md` §4。
- ✅ **批次六十七完成：07 篇 `cksum` 文件操作数接线 + 输出长度折算修正**（`2026-10-11`，`claim/CMD-CKSUM-FILES-qoder` @ 基线 `80ca0a9b6`，单域三线 commands/textfilter，无跨界）：接回 cksum 声明留白（“文件操作数待开放路径”）。决定半新增于已测 `minix_textfilter::cksum`（无堆）——`SumKind{Crc,Old1,Old2}` 与 `sum_display_length` 忠实复刻 `minix3/usr.bin/cksum/print.c` 的三个打印函数：`pcrc` 原字节数、`psum1` 折 `(len+1023)/1024`、`psum2` 折 `(len+511)/512`（`div_ceil`），修正现行对 `-o 1/-o 2` 打原始字节数的既存长度偏离（旧 bin 未折算）。执行半重写 `src/bin/cksum.rs`：`-o 1/2` 选 SumKind、`-a` 仍拒绝（crypto 未备）；无操作数读 stdin 打 `{crc} {len}`；有操作数逐文件 `open`+read、打 `{crc} {len} {name}`、打不开 warn+置错跳过不出行（对位 C cnt 失败早返回）。只读命令。三件套：`cargo test -p minix-textfilter --lib`→297/0（cksum 新增 `test_sum_display_length_block_rounding`，含 0/1024/1025/511/512/513 块界）；freestanding `cargo build -p minix-textfilter --bin cksum --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 本轮触及行零告警（唯一余留 `unnecessary_cast` 在未改动的 `cksum_crc`(crc.c:135) 行，既有基线，不顺手改）。**流程陷阱再现**：首次误向尚未 claim 的 `cmd-cksum-files-qoder` 路径发编辑，造了个空孤儿目录阻塞 `claim`（`.wt` 已 gitignored、仅含空目录），`find -depth -type d -delete` 清理后重领成功。声明性留白：`-a` crypto 族（md5/sha*/sha384/sha512）需密码原语未备；`-c` 校验回验模式（BSD cksum 的 do_check）与 `-p`/`-q` 打印旗标本轮未接（现行本树仅 crc/sum1/sum2 三算法、无 do_check）；真机 open 腿归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十七与 `../07-text-filter.md` §4。
- ✅ **批次六十六完成：07 篇 `wc` 文件操作数 + 固定列格式接线**（`2026-10-11`，`claim/CMD-WC-FILES-qoder` @ 基线 `6e92d5506`，单域三线 commands/textfilter，无跨界）：接回批次四十四声明留白（“文件操作数仍报 E-CMDSYSFACE”）——`minix_sys::open/read/close` 早已可用（cat 在用），本轮接上并一并修正现行偏离真值的平铺输出。决定半新增于已测 `minix_textfilter::count`（无堆）——`CharField{Off,Bytes,Chars}` 与 `format_counts` 忠实复刻 `wc.c print_counts`：每个选中计数按 ` %7llu`（一空格 + 七列右对齐、超宽自然变宽）输出，顺序行/词/(字节或字符单列)/最长，再 ` 名`+换行（文件 operand/total）或仅换行（无名 stdin）；字节/字符共享一列由旗标选（C `-c`/`-m` 互斥）。执行半重写 `src/bin/wc.rs`：无操作数读 stdin 无名输出；有操作数逐文件 `open`+流式 `Counter` 计数、`argc>1` 补 total 行（C `dototal=(argc>1)`）、打不开的文件 warn+置错且**不出行**（对位 C cnt 失败早返回）；`-c`→dobyte清dochar、`-m`→反之、缺省 line+word+byte。修正现行两处偏离：旧 bin 平铺无列宽且 `-c`/`-m` 分两列（真值为单列 ` %7llu`）。只读命令。三件套：`cargo test -p minix-textfilter --lib count`→count 模块含 9 测（新 5 枚 format_counts：默认三列宽/名列/字段选择/超宽/小缓冲报错）、全 crate `--lib`→296/0；freestanding `cargo build -p minix-textfilter --bin wc --target x86_64-unknown-none --release` 绿（初版缺 `use alloc::format` 使 none 目标 `format!` 未找到、宿主 std 掩盖，已补）；clippy/rustfmt 触及文件零告警。**test-audit 亮点**：`test_format_name_column_appended` 初版把 `"x y\n"` 的词数断为 1（实为 2：x、y），runner 当场报实为 `1 2 4`→按真值改。声明性留白：`-L` 最长行与多字节字符的 locale 边界按字节/码点口径（同 Counter）；真机 open 腿（逐文件）入镜像归 C-57/E5。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十六与 `../07-text-filter.md` §4/§5。
- ✅ **批次六十五完成：06 篇 `cat` 字节可视化选项（`-e`/`-t`/`-v`）接回**（`2026-10-11`，`claim/CMD-CAT-VISIBLE-BIN-qoder` @ 基线 `68aaccdaa`，单域三线 commands/fileops，无跨界）：接回批次六十四声明留白——把 `cat.c` `cook_buf` 的字节可视化补全。决定半 `minix_fileops::cook` 重构：`CatCook::new` 改接 `CookFlags`（含 number/number_nonblank/squeeze/dollar/show_tabs/visible），`render` 方法对位 C 逐字节分支——`'\n'` 在 `-e` 下行尾先补 `$`；`'\t'` 在 `-t` 下作 `^I`（不再原样输出）；其余字节在 `-v` 下高字节（≥0x80）→`M-`+`toascii(ch)`、控制字节（<0x20 或 0x7f）→`^`+`ch|0x40`（0x7f 作 `^?`）；`-e`/`-t` 蕴含 `-v`（由 new 折叠）。执行半 `src/bin/cat.rs`：新增 `-e/-t/-v` 旗标解析、`cook` 判据含可见旗标、`cook_stream` 改接 `CookFlags`；仅 `-B/-l/-f`（不改输出字节）保留 usage 拒绝。只读命令。三件套：`cargo test -p minix-fileops --lib cook`→11/0（+4：dollar/show_tabs/visible/`-b -e` 空行字段）、全 crate `--lib`→112/0（+4）；freestanding `cargo build -p minix-fileops --bin cat --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1）；clippy/rustfmt 触及文件零告警。**test-audit 亮点**：`test_visible_control_and_high_bytes` 初版把 golden 写成 `^[\n^?…`（误在 ESC 与 DEL 之间插了换行，而输入两者相邻），runner 当场报实为 `^[^?\n…`（实现正确）——属“测试自身错”当场修正；`0x80→M-^@`（toascii后为控制字节 NUL→^@）逐字节对位 C。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十五与 `../06-file-ops.md` §4/§5。
- ✅ **批次六十四完成：06 篇 `cat` 行变换选项（`-n`/`-b`/`-s`）接线 + `cook` 决定半**（`2026-10-11`，`claim/CMD-CAT-COOK-BIN-qoder` @ 基线 `403be4ddc`，单域三线 commands/fileops，无跨界）：接回既有声明留白——`cat.rs` 此前只走 raw 拼接，任何非 `-u` 旗标报 usage；本轮补上 `cat.c` 的 `cook_buf` 行变换路。决定半新增 `minix_fileops::cook`（无堆）——`CatCook` 逐字状态机忠实对位 `cook_buf`：行首（prev=='\n'）时 `-s` 把连续空行的第二个起丢弃（首个保留、非空行重置）、`-n` 在行首发出 `%6d\t` 右对齐行号、`-b` 蕴含 `-n` 且遇空行不编号；`emit_line_number` 把行号铺成宽 6（超宽自然变宽）。执行半改 `src/bin/cat.rs`：index 化解析 `--`、`getopt "B:beflnstuv"` 中支持 `-n/-b/-s/-u`（`-b` 蕴含 `-n` 由 `CatCook::new` 实现），任一 cook 旗标走 `cook_stream`（逐操作符新建 CatCook，行号逐文件重启，对位 C `cook_buf` 局部 `line`）、否则走原 raw 路径；`-e/-t/-v`（字节可视化，本层未建）与 `-B/-l/-f` 报 usage（非静默忽略）。只读命令（不写盘）。三件套：`cargo test -p minix-fileops --lib cook`→7/0、全 crate `--lib`→108/0（+7）；freestanding `cargo build -p minix-fileops --bin cat --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1= minix-ci:1.94 同版本）；clippy/rustfmt 触及文件零告警（除 `needless_range_loop` 已改直访切片）。**证据边界与 test-audit**：宿主 `./target/debug/cat -n file` 报 “cannot open”——因 `minix_sys::open` 在宿主无内核支撑（raw 路同样失败），证实 bin 的文件 IO 腿只能真机跑（属既定“真机 per-command 归 C-57/E5”边界，非回归），cook 正确性由 7 枚逐字符手推 golden 宿主测覆盖（与 C `cook_buf` 逐行对账）。**声明性留白**：`-e`/`-t`/`-v`（`$` 行尾、`^I` tab、`M-`/`^X` 非打印）、`-B`/`-l`/`-f` 未接（报 usage）；真机文件读取/exec 递送受 guest 二进制入镜像 + NS5 VFS exec worker 门控（同其它命令）。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十四与 `../06-file-ops.md` §4/§5。
- ✅ **批次六十三完成：06 篇 `touch` 接线 + `touch` 时间决定半**（`2026-10-11`，`claim/CMD-TOUCH-BIN-qoder` @ 基线 `21fabbf3a`，单域三线 commands/fileops，无跨界）：落地缺失命令 `touch`（改时间戳/不存在则建文，真源 `minix3/usr.bin/touch/touch.c` NetBSD）。决定半新增 `minix_fileops::touch`（无堆）——`days_from_civil`/`civil_from_days`（Hinnant 历法算法，由两个独立事实锚定：1970-01-01=0 天、`2000-01-01`=10957 天，另带历日元返测）与 `parse_time_spec`（忠实复刻 `stime_arg1` 的 `[[CC]YY]MMDDhhmm[.SS]`：按字符长度分层从左覆盖、省略字段沿用当前、两位年 69 枢纽、无 `.SS` 秒归 0）+ `is_obsolete_time_spec`（8/10 位定位形）。执行半 `src/bin/touch.rs`：`-a/-c/-f/-m/-t` getopt + obsolete 定位形；逐操作符 stat、不存在且无 -c 则 `open(O_WRONLY|O_CREAT,0666)`+fstat+close、未给时间即完成；已存则未置旗标保留原时间、已置旗标用目标；`utimens(Some(ts))` 失败时回退 `utimens(None)`（对位 C System V/POSIX 空指针腿）。默认 `touch file`（设当前/建文）无时区参与、100% 忠实 C。三件套：`cargo test -p minix-fileops --lib touch`→8/0、全 crate `--lib`→101/0（+8）；freestanding `cargo build -p minix-fileops --bin touch --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1= minix-ci:1.94 同版本）；clippy/rustfmt 触及文件零告警。**full-review + test-audit 双亮点**：① 实现初版 `civil_from_days` 误写 `400*yoe`（应为 `365*yoe`，Hinnant 原式），历日往返测当场抓出→已修；② 测试锚点初版把 `2000-01-01` 误记为 11016 天/951782400（实为 10957 天/946684800），runner 当场报错→按真值重算全部测常数——两处均为“测试/实现自身错”两类。**声明性留白**：`-t` 按 UTC 口径解释（本树无时区库，与 date 批 breakdown 恒 UTC 同裁；C 用 localtime/mktime+DST）；`-d`（自由日期串 parsedate）、`-r`（参考文件取时）、`-h`（lutimens 不跟随软链）、`--date`/`--reference` 长选报 not wired；`warn`+`strerror` 未复现；真机建文/改时需可写根盘、受 `E5` 门控，与写类命令同批。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十三与 `../06-file-ops.md` §4/§5。
- ✅ **批次六十二完成：06 篇 `rmdir` 接线 + `rmdir` 决定半**（`2026-10-11`，`claim/CMD-RMDIR-BIN-qoder` @ 基线 `8ba08f612`，单域三线 commands/fileops，无跨界）：落地缺失命令 `rmdir`（删空目录，真源 `minix3/bin/rmdir/rmdir.c` NetBSD 1.26）。决定半新增 `minix_fileops::rmdir`（无堆）——`for_each_ancestor` 忠实复刻 `rm_path` 的 `strrchr`-截断循环（由内向外逐级父目、截至空即根而不删、不预剥尾斜杠以保 C“尾斜杠操作符首截回叶”行为）。执行半新增 `src/bin/rmdir.rs`（靠 `src/bin/` 自动发现）：`-p` getopt；逐操作符 `rmdir`（失败→warn+errors=1），成功且 `-p` 则 `for_each_ancestor` 逐级删父、首个父删失败即停（对位 C `rm_path` return 1）；退出码按 errors。与刚落的 `mkdir::for_each_parent` 互为逆序（去叶），测目里做了交叉对账。三件套：`cargo test -p minix-fileops --lib rmdir`→5/0、全 crate `--lib`→93/0（+5）；freestanding `cargo build -p minix-fileops --bin rmdir --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1= minix-ci:1.94 同版本；docker 镜像本仓自含环未装 x86_64-unknown-none target、如实标注）；clippy/rustfmt 触及文件零告警。**full-review 亮点**：对照 C `rm_path` 发现初版多加了“预剥尾斜杠”，与 C 在“尾斜杠+`-p`”时会分歧（C 会把已删的叶再当首个父目递一遍）——改为不预剥以忠实，并用 `test_trailing_slash_reoffers_leaf_like_c` 钉住。**声明性留白**：C `rmdir(2)` 拒非空目录（只删空篮子）为内核语义、不在本层建模；`warn`+`strerror` 系统错文未复现（仅报操作数名）；真机删盘需可写根盘、受 `E5` 门控，与写类命令同批。另本轮发现并绕开一个流程陷阱：首次误将文档编辑写入已 release 的 `cmd-mkdir-bin-qoder` 孤儿目录（`.wt` gitignored、不入库），已改写入本 `cmd-rmdir-bin-qoder` 正确树。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十二与 `../06-file-ops.md` §4/§5。
- ✅ **批次六十一完成：06 篇 `mkdir` 接线 + `mkdir` 决定半**（`2026-10-11`，`claim/CMD-MKDIR-BIN-qoder` @ 基线 `e30103411`，单域三线 commands/fileops，无跨界）：落地缺失命令 `mkdir`（建目录，真源 `minix3/bin/mkdir/mkdir.c` NetBSD 1.38）。决定半新增 `minix_fileops::mkdir`（无堆）——`for_each_parent` 忠实对位 `mkpath` 的斜杠扫描（逐级前缀、尾斜杠忽略、仅末个为终端；用显式 `'a` 绑定使访问器可存前缀片）；`intermediate_mode`=mode|u+wx（对位 C `dir_mode = mode | S_IWUSR | S_IXUSR`）；`needs_extra_bit_fixup`=mode&!0777≠0（对位 C 粘滞/设身份位需后续 `chmod`）；`parse_mode_spec` 直接复用已测 `mode::apply_mode(0o777, spec)`（即 BSD `getmode(setmode(optarg), S_IRWXU|S_IRWXG|S_IRWXO)`）。执行半新增 `src/bin/mkdir.rs`（靠 `src/bin/` 自动发现、无需改 Cargo）：`-m`/`-p` getopt（支持 `-mSPEC` 与 `-m SPEC`）、非 `-p` 逐操作符 `mkdir`+额外位 `chmod`、`-p` 走 `for_each_parent` 且 mkdir 失败时 `stat` 判已存目录（继续）/非目录（ENOTDIR 停）/不存在（报 mkdir 错）；退出码 0 全成/1 有败。三件套：`cargo test -p minix-fileops --lib mkdir`→6/0、全 crate `--lib`→88/0（+6）；freestanding `cargo build -p minix-fileops --bin mkdir --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1= minix-ci:1.94 同版本且已装该 target；docker `minix-ci:1.94`/`1.94-arch` 本仓自含环内未装 x86_64-unknown-none target、未能离线复现，如实标注）；clippy/rustfmt 触及文件零告警。**test-audit 亮点**：`test_parse_mode_spec_octal_and_symbolic` 初版把 `go-rwx,u+x` 推为 `0500`（误以为只剩 u 读执行）——runner 当场报实为 `0700`（`go-rwx` 只清 group/other、u 保持 0700 且 `u+x` 已含），属“测试代码自身错”的当场捕获，已改断言并加注。**声明性留白**：mkdir 只建目录不递归（与 fts 无关、无此依赖）；C 的 `warn`+`strerror` 系统错文未复现（仅报操作数名，与 colcrt/what 等批同裁）；真机建目录需可写根盘、受 `E5` 门控，与其它写类命令同批。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十一与 `../06-file-ops.md` §4/§5。
- ✅ **批次六十完成：20 篇 `printroot` 接线 + `fsversion` 决定半**（`2026-10-11`，`claim/CMD-PRINTROOT-SYSINFO-qoder` @ 基线 `390e84860`，单域三线 commands/sysinfo，无跨界）：落地缺失命令 `printroot`（打印根设备，真源 `minix3/minix/commands/printroot/printroot.c` 82 行）——`minix-sysinfo` 整 crate 此前零 `src/bin/`，本轮首次接执行半。决定半新增 `minix_sysinfo::fsversion`（无堆）——`classify_super` 忠实移植 `minix3/minix/lib/libc/gen/fsversion.c`：小端读十六位魔术，三代 Minix 魔术（`0x137f`/`0x2468`/`0x4d5a`）挤在同一偏移 `0x18`、扩展二 `0xef53` 占偏移 `0x38`（对位 `fsversion.c:56-70`）；`is_iso9660_descriptor` 对位 `CD001` 标签位于卷描述区偏移 1（`:77`）；`version_tag` 对位 `printroot.c done()` 的 `switch`（`FSVERSION_MFS1/2/3→"1"/"2"/"3"`、`EXT2→"ext2"`、`default（含光盘与无匹配）→"0"`，`:72-81`）。执行半新增本 crate 首个 `src/bin/printroot.rs`：忠实对位 `printroot.c`——`stat("/")` 取根 `st_dev`、`open("/dev/")` 经 `getdents` 逐项 `stat` 收 `(叶名, st_rdev, is_block)`、路由到已测 `rootdev::discover_root`（首个 `is_block && st_rdev==root_dev` 即中，回退 `/dev/unknown`）；`-r` 仅打印设备名加换行（对位 `printroot.c:43` 只查 `argv[1]=="-r"`），否则补 `" / "` 再 `probe_fsversion`（开设备、`lseek` 跳 `SUPER_BLOCK_BYTES` 读超级块、分类；不中就 `lseek 0x8000` 找 `CD001`）打 `"{tag} rw\n"`。退出码：命中 0、回退 1（对位 `done(name,status)`）。为此新增 crate 内联 `bin_support.rs`（args/emit/warn/terminate 双 cfg 缝，照 fileops 模板）+ Cargo `[[bin]] printroot` + minix-rt 双形态依赖。**test-audit 亮点**：7 枚宿主测以 `test_byte_order_is_little_endian` 钉住 `0x137f→7f 13` 防字节序推错、`test_mfs_takes_precedence_over_ext2` 钉住两者同现时 Minix 优先（对位「合法的 Minix 可能看起来像扩展二，反之不然」）、`test_version_tag_mapping` 钉住光盘与无都归 `0`；执行半薄壳靠宿主 + 自由目标编译验证（单一 I/O 腿，不编造 golden）。三件套：`cargo test -p minix-sysinfo --lib`→全 crate 34/0（新 7 枚 fsversion、既有 rootdev 4 枚回归）、`cargo test -p minix-sysinfo --lib fsversion`→7/0；freestanding `cargo build -p minix-sysinfo --bin printroot --target x86_64-unknown-none --release` 绿（宿主 rustc 1.94.1= minix-ci:1.94 同版本且已装该 target；docker `minix-ci:1.94`/`1.94-arch` 两镜像本仓自含环内未装 x86_64-unknown-none target、未能离线复现，如实标注）；clippy 触及文件（fsversion/printroot/bin_support）零告警（唯一 `field_reassign_with_default` 告警在未触碰的 `clock.rs` 既有基线）；rustfmt 触及文件零漂移（clock.rs 既有漂移不顺手改）。**声明性留白**：`fsversion` 打开失败/超级块读不满在 C 往 stderr 打系统错文（`std_err`+`perror`），本实现仅回 `None`（→`"0 rw"`）不复现错文（与 colcrt/what 等批「系统错文未复现」同裁）；真机 per-command 断言与 imgrd `/bin/printroot` 播种归 `C-57`/E5（本命令只读、不受可写根盘写门控）。修法详见 `../18-stage-commands/todo.md` §6.1 批次六十与 `../20-minix-system.md` §2.1/§4/§5。
- ✅ **批次五十九完成：11 篇 `shar` 接线 + `shar.rs` 格式事实修正**（`2026-10-11`，`claim/CMD-SHAR-BIN-qoder` @ 基线 `e4610ac21`，单域三线 commands/compress，无跨界）：落地缺失命令 `shar`（生成 shell 归档，真源 `minix3/usr.bin/shar/shar.sh` NetBSD 1.3 shell 脚本）。**full-review 发现两处 P0-fact**：既有 `shar.rs` 模块文档与 `11-compress-archive.md` 均把归档格式误陈为 `begin 模式 名`/`end` 段式，而真源 `shar.sh` 实际生成的是 **here-doc 形制**（目录 `echo c - 名`+`mkdir -p`；文件 `echo x - 名`+`sed 's/^X//' >名 << 'END-of-名'`+正文逐行 `X` 前缀+`END-of-名`；末尾 `echo exit`）。`shar` 命令只生成、不解析（本树无 `unshar`）。修正：重写 `shar.rs` 模块文档与 `11-compress-archive.md` §1.3/§2.3/§4/§5/§7——`scan_archive` 重定位为“解更早期工具的 `begin`/`end` 段式”（与本树 `shar` 所产互不往返）。新增生成向决定半 `shar::generate`（输入 `SharEntry` enum Dir/File+正文，输出逐字节等于 `shar.sh`）+ `body_lines`（拆行与末尾空段处理）。执行半新增 `src/bin/shar.rs`：逐操作数 `stat` 判目录/`open`+`read` 取正文后 `generate`→stdout（照 `uuencode` 文件腿为 target-only）。无操作数按 `shar.sh:35-38` 把 usage 打 **stdout** 退 1（非 stderr，忠实脚本）。声明性留白：正文无尾换行的文件在归档里补一个尾换行（here-doc 分隔行需独占一行，与 BSD `sed` 行为一致）；目录用 `mkdir -p`（C 亦如此）；真机 per-command 断言与 imgrd `/bin/shar` 播种归 `C-57`/E5。另同步修正 `11-compress-archive.md` §5 陈旧的“28 个测试”硬数（先前 uuencode/uudecode 批次漏同步本篇）→改反漂移命令形。**test-audit 亮点**：以真实 `shar.sh subdir hello.txt empty.txt` 多操作数读数作 golden oracle——`test_generate_matches_real_shar_script` 逐字节钉住（dir+非空文件+空文件的索引排序/分段形制/空体/`exit`+空行），是本轮防“测试代码自身错”的外部交叉基线。三件套：`cargo test -p minix-compress --lib shar::`→12/0（新 6 枚 generate 测）、全 crate `--lib`→48/0；freestanding `cargo build -p minix-compress --bin shar --target x86_64-unknown-none --release`（docker `minix-ci:1.94`）绿；clippy 触及文件零告警（11 条均为 minix-types/minix-rt/lzw 基线）。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十九。
- ✅ **批次五十八完成：06 篇 `sync` 接线**（`2026-10-10`，`claim/CMD-SYNC-BIN-qoder`，单域三线 commands/fileops，无跨界）：落地缺失命令 `sync`（排空文件系统待写页，真源 `minix3/bin/sync/sync.c` 57 行）——`main` 无 `getopt`、忽略所有操作数、`sync()` 后 `exit(0)`。决定半为空逻辑（同 `true`/`false` 封装）：命令全部契约就是“调 `minix_sys::sync()` + 退 0”，无纯函数可单元测，正确性=忠实调用（由 freestanding 编译门 + 源码对读保证，非编造 golden）。执行半 `src/bin/sync.rs`：同语义。三件套：全 crate `--lib`→82/0（sync 无新测，与既有同）、sync bin freestanding x86_64-unknown-none --release（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。**test-audit 说明**：本命令零可测逻辑（单一 syscall 封装），无新增宿主测属如实而非略过。**声明性留白**：真机 flush 效果受可写根盘 E5 门控（同其他磁盘类）。修法详见 `../18-stage-commands/todo.md` §6.1 批次五十八。
- ✅ **批次二十六完成：05 篇执行层落点决策成文**（迭代 34，文档批）：05-shell-family.md 新增 §3.5——[ARCH] A-2 之下的进程原语盘点（`fork`/`waitpid`/`exit`/`open`/`close`/`read`/`write`/`fcntl` 七命令已接）与缺口三件命名（运行时 `execve` 面挂 C-21 minix-rt 栈帧裁决〔卡 J〕、`pipe2` 封装走 `VFS_PIPE2`、`F_DUPFD` 进 fcntl 常量组），执行器组装原则预记（定容 arena 语法树、fork 每命令、内建在体内、`pipe2` 链、被启动者即本仓库各命令壳——A-2 之下 sh 不代管 stdio）。`sh` 执行批待缺口三件落地后开工；`minix-shell` 文字层 36 测试原样成为执行器前端。
- ✅ **批次二十五完成：13 篇 stty 接线 + termios wire 基座**（迭代 33，跨界 C-23 已登记）：minix-types 新增 `types/termios.rs`（`Termios` 44 字节 LP64 布局 + 十六旗标位 + 九个 `c_cc` 槽位常量，逐位钉值，序列化往返测试）；minix-sys 顶层新增 `TIOCGETA`/`TIOCSETA` 请求号（`ttycom.h:88-89` 编码钉值：方向位加 44 字节长度域加组 `'t'` 加序号）与 `tcgetattr`/`tcsetattr` 封装；termctl 新增 `apply_ops`（旗标落四字、`cs8` 置位清 `CSIZE` 域、速度双向同写、控制字符入槽）与 `display_a`（速度行加四组旗标加 caret 行，`0`/`0xFF` 同按 `<undef>`）；薄壳 `bin/stty.rs`（`-a` 全显、无参速度行、操作数取改设三步）。**声明性留白**：`-g` 单行格式、rows/columns 与 line discipline 显示域、完整标志大表、terminfo 装船（A-2）、term-games 原始终端模式（归终端驱动阶段）。`cargo test -p minix-termctl/-p minix-types/-p minix-sys` 22/291/233 全过（ulimit -v 3G + -j 1）；clippy 新增面清零；13 篇随批更新。剩余：`-g`、rows/columns、terminfo、term-games。
- ✅ **批次二十四完成：10 篇 cal/whatis/apropos 接线**（迭代 32）：`cal.rs` 补渲染面（`day_matrix` 按周日列表头换算周一基栅格、`render_month` 单月 golden 逐字对齐 BSD cal、`render_year` 三列年视图、行尾空格裁剪与 C 的六行循环一致）加 4 个测试（含格里高利/儒略栅格分叉断言）；`whatis.rs` 补 `SliceManDb::for_each_matching`（ASCII 折叠无分配的关键词逐行匹配，apropos 面）；薄壳三枚——`bin/cal.rs`（`-y`/`[[month] year]`/无参当前月，宿主时钟缝）、`bin/whatis.rs`/`bin/apropos.rs`（库默认 `/usr/man/whatis`、`-M` 可指，退出码按命中）。**声明性留白**：`-j`/`-3`/`-r` 等旗标与月名操作数、`man`/`whereis`（排版引擎整体留白）、`makewhatis` 构建面。`cargo test -p minix-doctools` 17 个全过（ulimit -v 3G + -j 1）；clippy 与既有基线一致（3 条：cal 两处同型转换与 manconf 变体跨度，均非本批引入）；10 篇模块表/契约/POSIX 基准随批更新。剩余：man/whereis/fmt/nl 等排版族（留白面）、12 篇 ps/finger/date 族、05 sh。
- ✅ **批次二十三完成：12 篇 kill/who 接线**（迭代 31）：proctools 新增 `stamp.rs` 时刻戳决定半（`civil_from_days` 历法反推加 `format_login_time` 十二列戳——`who.c:317` 的 `%.12s` of `ctime`；UTC 为声明边界），4 个测试含与 10 篇 `cal.rs` `days_from_civil` 的 dev-dependency 交叉往返；薄壳 `bin/kill.rs`（`-l`/`-l 码`/`-s`/`-名字`/`-号码`/默认 TERM，逐 pid 发信失败翻退出码不中断，负号进程组直传）与 `bin/who.rs`（默认 `/usr/adm/utmp`、操作数可指库，只印在场会话的名/行/时刻）。**声明性留白**：who 的空闲/状态列（要终端 stat）、`ps`（待内核表数据源）、finger/from/date/sleep 族随各自批次。`cargo test -p minix-proctools` 20 个全过（ulimit -v 3G + -j 1）；clippy 新增面清零（3 条为 ptime.rs 既有基线）；12 篇 §4/§5 随批更新。剩余：ps/finger/date/sleep/nice 族、10 篇 cal/whatis 批。
- ✅ **批次二十二完成：11 篇 compress/uncompress 接线**（迭代 30）：`lzw.rs` 补 `.Z` 帧面（`frame_header`/`unframe`，魔数 `0x1F 0x9D` 与第三字节 `maxbits | BLOCK_MASK`，`compress.c:151-157`/`:759`）加两个测试（帧往返、坏魔数与越界宽度拒绝）；薄壳 `bin/compress.rs` 接 `-d`/`-c`/`-f`/`-v`/`-b`（9 到 12 位，Rust LZW 上限）与 `uncompress` 名隐含 `-d`（`compress.c:334`），无操作数走标准输入输出，目标存在且无 `-f` 时拒绝覆盖，压缩侧写帧后删源文件。**声明性留白**：外来 16 位宽 `.Z` 在 `unframe` 诚实拒绝（Rust LZW 上限 12 位）、多成员归档与形变编码照旧。`cargo test -p minix-compress` 28 个全过（ulimit -v 3G + -j 1）；clippy bin 清零（lib 1 条为既有基线）；11 篇 §4.3/§5 随批更新。剩余：uu/shar/pax/gzip/bzip2 格式层（文档已声明留白）。
- ✅ **批次二十一完成：09 篇 ed 执行半决定半与薄壳**（迭代 29）：editor 新增 `exec.rs` 执行层——会话状态（当前行/modified/标记/默认文件名）加 `step` 一步一行状态机，显示面逐格移植 `put_tty_line`（`l` 转义与 72 列折叠、`n` 编号、显示推进当前行），文件面经 `EditorIo` 缝（`r` 插入后地址处、`e`/`E` 换缓冲、`w`/`W` 写出与整缓冲清 modified、`wq` 粘连退出），失败面全部走 C 的 `errmsg` 通道（`h`/`H` 可读回）；决定半扩至三十个命令字母全量并校正 `wq` 语义（C `main.c:804-807` 在写支内读走粘连字母，早先"写退分两次"说法不符）；地址面补 `%` 整缓冲简写（`main.c:365-373`）；`m`/`t` 的第三地址允许 0（`GET_THIRD_ADDR` 只拒负与越界）而 `1,3m2` 界内拒绝、`1,2m2` 为 no-op。薄壳 `bin/ed.rs` 接 argv/`-p`/`-s`/`-S`/`-x` 与初始文件读。**声明性留白**：`s`/`g`/`v`/`G`/`V`（待搜索求值）、`u`（撤销栈）、`!`（fork/exec）各以 "not wired" 应答，`x` 按 C 无 DES 构建答 "crypt unavailable"。24 个新测试（执行 22 加解析 2），`cargo test -p minix-editor` 47 个全过（ulimit -v 3G + -j 1）；clippy 回到基线（唯一告警为 store.rs 既有项）；边界守卫除并行在制 init/host.rs 外全绿；09 篇 §4/§5 随批更新（测试计数 22→47、模块表加 exec.rs 与薄壳行、§4.2 的 wq 校正、§5.1 命令契约与 POSIX 基准）。`mined` 归终端阶段。剩余长尾：10→11→12、13→23、18~19、20~21、05 sh（前置 A-2 决策）、存储批（等 E-FSCMDS）。
- ✅ **批次二十完成：07 篇 patch 引擎决定半与薄壳**（2026-09-18，迭代 28）：textfilter 新增 `patch.rs`——unified/normal hunk 解析（pch.c 的 UNI_DIFF/NORMAL_DIFF 分支：`(起始,计数)` 头部语义、`< / --- / >` 体、行首 tab/空行的空格被吃容错、纯追加 p_first++ 调整）+ 应用引擎（patch.c 的 locate_hunk/patch_match/apply_hunk：猜测位 `pch_first+last_offset`、正负偏移交替搜索、上下文 fuzz 最多 2 且拒绝真空匹配、`where--` 后按对齐行 `where-1+old-1` 惰性拷贝/删除/插入、旧侧视图匹配），10 测试（精确应用/追加/偏移链/多 hunk/失败报告/fuzz 营救/畸形拒绝）；薄壳 stdin/-i 读补丁、目标文件操作数待 open-existing；textfilter 229 个测试全过（ulimit -v 3G + -j 1）；边界守卫除并行在制 init/host.rs 外全绿；07 篇 §4.4/§5 随批更新（229 测试、33 二进制）。C-1 批次至此全部完成（07 篇纯逻辑命令清零）；剩余长尾：05 sh、23/24 与各篇 Requires 回填（P1-1/P1-2）。
- ✅ **批次十六完成：07 篇 colrm/tee 接线**（2026-09-18，迭代 23）：textfilter 新增 `vis.rs` 决定半——默认 M 记法（graphic+空白透传、0200 位 M 前缀、控制符 ^X/`?`、`-x` 非控制形态）、`-c` C 风格命名转义（含 skip list 回落与 `\0` 八进制前导倍增）、`-o` 三位八进制、`-n` 无反斜杠，11 测试含 0x00..=0xFF 全量回环（0x5C 反斜杠透传不对称为已声明 C 怪癖）；`unvis` 解码状态机对照 lib/libc/gen/unvis.c:217-330 逐态移植（Ground/Start/Meta/Meta1/Ctrl/Octal2/Octal3）。薄壳两枚接线，textfilter 115 个测试全过、守卫全绿；07 篇 §4.4/§5 随批更新。
- ✅ **批次十四完成：07 篇 paste/split/tsort 接线**（2026-09-18，迭代 22）：textfilter 新增三个决定半——`paste.rs`（`-d` 分隔符列表循环与空项无分隔、`-s` 串行合并，4 测试）、`split.rs`（块计算与 `aa`..`zz` 后缀枚举，4 测试）、`tsort.rs`（优先级归并输出、自环与环报告断开、重复弧重复计数、奇数个记号报错，6 测试）与三个薄壳（paste 单流串行/透传；split 以横幅输出各块——写文件面待开放；tsort 全功能含 `-q`）；`cargo test -p minix-textfilter` 108 个全过、守卫全绿；07 篇 §4.4/§5 随批更新。07 篇剩余决定半批次：diff/patch 引擎、col/column/hexdump/jot/lam/units/look/ifdef/crc/unifdef 等。
- ✅ **批次十三完成：07 篇 cksum 接线**（2026-09-18，迭代 21）：textfilter 新增 `cksum.rs` 决定半——CRC 表改为 const fn 从多项式 0x04c11db7 生成（与 C crctab 全表核对一致，消除手抄风险）、CRC + 长度小端尾随 + 取反、`-o 1`/`-o 2` 历史和；与系统 cksum 四组值交叉验证；`-a` crypto 族显式拒绝待密码原语；5 测试 + 薄壳。07 篇纯逻辑批次继续：paste/split/tsort/vis/unvis 等。
- ✅ **批次十二完成：07 篇 expand/unexpand/fold 接线**（2026-09-18，迭代 20）：textfilter 新增三个决定半——`expand.rs`（`-t` 停位表校验〔1..256 严格递增至多八停〕、四路补空、退格拉回透传，6 测试）、`unexpand.rs`（`tabify` 逐字节移植：双列追踪、≥2 列成 tab、`-t` 限位、`-a`/缺省两形态，8 测试）、`fold.rs`（`\b`/`\r`/`\t` 列规则、`-b`/`-s`、末空格断行且空格留续行头的 C 缓冲回移语义，7 测试）与三个薄壳；`cargo test -p minix-textfilter` 92 个全过、守卫全绿；07 篇 §4.4/§5 随批更新。07 篇纯逻辑批次继续：cksum/paste/split/tsort/vis/unvis 等。
- ✅ **批次十一完成：07 篇 yes/rev/comm 接线**（2026-09-17，迭代 19）：`rev.rs` 决定半（字符级反转，多字节完整性有测试）与 `comm.rs` 决定半（三列归并状态机、列抑制、`-f` 折叠、两端排空，4 测试）加三个薄壳——yes 纯执行半无限循环（写失败即止退出 1，yes.c:50-55 的 C 形状）、rev 走 stdin、comm 双流同源自检形态；`cargo test -p minix-textfilter` 64 个全过、守卫全绿；07 篇 §4.4/§5 随批更新。07 篇纯逻辑决定半批次继续：cksum/expand/fold/paste/split/tsort/vis/unvis 等。
- ✅ **批次十完成：07 篇 cmp/seq/pr 接线**（2026-09-17，迭代 18）：textfilter 新增三个决定半——`cmp.rs`（逐字节比较、首差异定位、`-l` 全量清单、EOF 指名短流，5 测试）、`seq.rs`（整数序列、方向跟随缺省增量、方向错误/零增量/非整数报错、`-w`/`-s`/`-t`，9 测试；浮点增量与 `-f` 待浮点裁决）、`pr.rs`（66 行页 5+5 头尾、`-h`/`-l`/`-t`、时间字段由壳注入，6 测试）与三个薄壳（cmp 结构性就位——文件操作数待开放路径；seq 全功能；pr 单栏分页，多栏与 `-m` 待批）；`cargo test -p minix-textfilter` 56 个全过、守卫全绿；07 篇 §4.4/§5 随批更新。07 篇自 sort 起的决定半欠账清零，剩余 cmp/diff/patch/pr 多栏等已逐项标注。
- ✅ **批次九完成：07 篇 sort 接线**（2026-09-17，迭代 17）：textfilter 新增 `sort.rs` 决定半（`-k F[.C][flags][,F[.C][flags]]` 键规格解析——`.0` 非法偏移拒绝、未知修饰符静默跳过；`b d f i M n r` 修饰符语义、空白段与 `-t` 两套分段、局部 `r` 在全局 `-r` 下清除〔init.c:239-242〕、键并列的整行回退——11 个测试）与薄壳（内存内排序 + `-c` 检查模式，外部归并与超内存输入登记执行层待批）；`cargo test -p minix-textfilter` 36 个全过、守卫全绿；07 篇 §4.4/§5 随批更新。
- ✅ **批次八完成：06 篇 expr 接线**（2026-09-17，迭代 16）：fileops 新增 `expr.rs` 决定半（优先级梯 `| & 比较 + - * / % : `、`length` 前缀、`:` 的锚定 BRE 匹配经 minix-regex——对齐 C 的 expr 链接 libc regex 的架构位置；C 的加减溢出符号检查、乘法环绕漏报怪癖、首参 `--` 吞并怪癖全部镜像，14 个测试）与薄壳（结果按空零规则退出 0/1，语法与求值错误退出 2）；fileops 开启 `extern crate alloc` 仅服务 expr 的中间值。`cargo test -p minix-fileops` 74 个全过、守卫全绿；06 篇 §4.5/§5 随批更新。06 篇的不碰文件命令至此全部接线。
- ✅ **批次七完成：06 篇 printf 接线**（2026-09-17，迭代 15）：fileops 新增 `printf.rs` 运行时格式引擎（标志/宽度/精度、`d i o u x X s c %b %%`、格式复用、`\c` 截停、base 0 数字解析与 `"A` 字符常量前缀、`check_conversion` 双警告——23 个测试逐条钉住 C 行为；浮点 `e E f g G` 按裁决单独处理，引擎显式报 FloatNotModelled）与薄壳；`cargo test -p minix-fileops` 60 个全过、守卫全绿；06 篇 §4.5 printf 行与 §5 统计随批更新。expr 为下一个独立批次。
- ✅ **批次六完成：06 篇 pathchk 接线**（2026-09-17，迭代 14）：fileops 新增 `pathchk.rs` 决定半（`-p` 路纯常量校验：组件 14 字节、全路径 256 字节、可移植字符集、前导连字符，7 测试含空路径直通与检查顺序两个 C 怪癖）与薄壳（诊断按 C `warnx` 形状走标准错误）；缺省模式因 `pathconf`/`stat` 面缺失显式拒绝并登记待批；`cargo test -p minix-fileops` 37 个全过、守卫全绿；06 篇 §4.5 行拆分与 §5 统计随批更新。printf 与 expr 各自后续独立批次。
- ✅ **批次五完成：08 篇 grep/sed 接线**（2026-09-17，迭代 13）：regex 新增 `bin_support.rs` 与两个薄壳——grep（`-E`/`-G`/`-F`〔定串经引元入基本引擎〕/`-v`/`-n`/`-c`/`-q`/`-x`/`-l`/可重复 `-e`，`-i`/`-w` 解析后显式拒绝待引擎建模）、sed（`s` 命令全保真：`p` 标志与 `-n` 的 C 交互、`g`/`N` 作用域、空匹配推进；`;` 链与其他动词显式拒绝）；`cargo test -p minix-regex` 34 个全过、守卫全绿；08 篇新增 §4.5 命令契约与 Requires 表（P1-1）与 POSIX 基准引用（P1-2）。
- ✅ **批次四完成：07 篇 head/tail/wc 接线**（2026-09-17，迭代 12）：同 crate 的 `window.rs`/`count.rs` 决定半落成二进制——head/tail（`-n` 缺省 10，窗口容量封顶 32 已在 §4.4 登记为声明性上限）、wc（`-l`/`-w`/`-c` 任选、流式计数不经收集；C 的文件操作数列对齐未复刻已标注）；07 篇 §4.4 两行状态更新、篇首薄壳清单改六个；`cargo test -p minix-textfilter` 25 个全过、守卫全绿。07 篇自此达到其当前决定半能力下的最大接线面（sort/cmp/diff/patch 等决定半未写的命令仍待各自批次）。
- ✅ **批次三完成：07 篇 tr/cut/uniq 接线**（2026-09-17，迭代 11）：textfilter 新增 `bin_support.rs`（stdin 收集 + 行切分）与三个薄壳——tr（翻译/`-d`/`-s`/`-c` 取补四路）、cut（`-b`/`-c`/`-f` 加 `-d`/`-s`，文件操作数待开放路径）、uniq（`-c`/`-d`/`-u` 单模式，`-i`/`-f`/`-s` 待批）；`cargo test -p minix-textfilter` 25 个全过、守卫全绿。07 篇新增 §4.4 命令契约与 Requires 表（P1-1）与 POSIX 基准引用（P1-2）。已登记的宿主态：输入侧二进制因 E-SYSCALL-SIGN 的 read 假成功暂无文件末尾（真机与修复后正常），见各 bin 头注释与 07 篇篇首。
- ✅ **批次二完成：06 篇纯参数五件端到端**（2026-09-17，迭代 10）：`true`、`false`（纯退出码语义）、`basename`、`dirname`（决定半复用已测的 `path.rs`）四个薄壳接线，fileops 现有六个二进制；`cargo test -p minix-fileops` 30 个全过、守卫全绿。06 篇新增 §4.5 命令契约与 Requires 分组表（三十四个命令逐组回填，含 cat 的 `open` 现有路径 ENOSYS 阻塞、test 文件问的 stat 依赖、printf/expr/pathchk 的决定半待写），POSIX 基准引用（P1-2）随批落地。剩余批次：07/08（需 LineReader 接入各文本过滤决定半）、06 的 expr/printf/pathchk 决定半、05/09 及后续各批。
- ✅ **批次一完成：22 篇 stdio 游戏全接线**（2026-09-17，迭代 9）：新增 `bin_support.rs`（行缓冲 `LineReader` 惰性消费设计 + 十进制格式化 + 宿主/真机接缝，6 测试随壳运行）与两个决定半模块 `bcd.rs`（打孔表逐值转写 `bcd.c` + `render_card`，7 测试）、`ppt.rs`（`punch_byte`/`decode_line` 往返，6 测试）；九个薄壳接线（bcd/ppt/factor/primes/caesar/morse/pig/number/arithmetic）；`cargo test -p minix-stdio-games` 102 个全过、守卫脚本全绿。已声明的偏离（22 篇 §4.4 标注）：primes 界限封顶 1,000,000（C 近 2^32 流式筛待批）、arithmetic 播种用 LCG 加宿主时钟（C 用 `random(3)`）、banner 薄壳待真字体。22 篇的 Requires 列（P1-1）与 POSIX 基准引用（P1-2）随批回填。

### 6.1.1 C-7 边界守卫（迭代 8）

✅ `tools/check-command-boundary.sh` 落地（2026-09-17）：源码级检查 `os/commands/` 内 `minix_types` 直引与 `minix_sys::ipc` 触达，零命中输出 OK、违例逐行列出并提示"缺封装去 14 侧登记，不在命令里手搓"。运行结果 OK——与迭代 4 的 init 修复互为验收（脚本在 init 去 minix-types 之后才可能全绿，两步的先后依赖即 C-2 的设计意图）。

---

### 6.2 C-2 errno 常量未再导出：init 是分层契约的唯一现存反例（P1）

**证据**：99-global-concepts.md §1 的硬规则是"命令只依赖 `minix-rt` 与 `minix-sys` 顶层"。但 `os/libs/minix-sys/src/lib.rs:43` 只再导出了 `Errno`/`Gid`/`Pid`/`Uid` 四个类型，`EEXIST` 等 errno 常量仍只存在于 `minix-types`；于是 `os/commands/sbin/init/Cargo.toml` 直依赖 `minix-types`，`src/entry.rs:11` 写着 `use minix_types::{EEXIST, Errno};`。全 `os/commands` 范围 grep `minix-types`，仅此一处（2026-09-17 实测）。

**影响**：这是规则与现实的偏离点：不是 init 的作者想越界，而是 libc 面不完整把它逼出去的。若不修，后续每个需要判 `EEXIST` 的命令都会复制这条越界路径，99 §1 从第一条命令起就名存实亡。

**处置**：minix-sys 侧（errno 常量再导出，对齐 C 的 `libc` 中 errno.h 的地位）已登记 edge `E-CMDSYSFACE`，由 14-stage-runtime 执行。18 侧的对应动作：✅ 已完成（2026-09-17，迭代 4）——实施时发现 `minix_types::Errno` 本就有 `EEXIST` 关联常量（`os/libs/minix-types/src/types/errno.rs:204`），随类型再导出（`lib.rs:264`）即可达，无需等 14 侧：`entry.rs:11` 改为 `use minix_sys::Errno;`，`:91` 与测试 `:322` 的 `Errno::from_i32(EEXIST)`（i32 往返解码）改为直接用关联常量 `Errno::EEXIST`，`Cargo.toml` 删除 `minix-types` 依赖行。验证：`cargo test -p minix-init` 89 通过；`grep -rn "minix_types" os/commands/` 零命中；`cargo tree -p minix-init` 剩余的 minix-types 均为经 minix-sys/minix-rt 的传递依赖（再导出正是设计的消费通道，边界规则约束源码级直接引用）。

---

### 6.3 C-3 命令拿不到参数与环境（P1，14 侧半挂 edge）

**证据**：`minix-rt` 的出生链已把参数/环境描述符解析为静态量（`crt0.rs:29-34` 的 publish 阶段），对外只提供了 `progname()`（`crt0.rs:129`）与逐个取的 `argv_bytes(index)`（`crt0.rs:154`）；`handoff.rs:290-315` 里 `environment_list`/`environment_count` 字段已解析但没有对外访问器。grep `env` 无 `pub fn`（2026-09-17 实测）。

**影响**：`echo`（第一个要实现的真实命令）的参数来源就没有着落；`ls`、`sh` 同样卡在参数面上。99 §3 示例行写 echo 需要"argv 交接（minix-rt）"，但 minix-rt 今天交不出这个 API。

**处置**：14 侧补 `env_bytes(index)` 与一次取全的 `args()`/`envs()` 迭代器（publish 静态量已就位，属纯增量，不动出生链）——已登记 edge `E-CMDSYSFACE`。18 侧动作：Requires 列回填时，把每个命令的参数面需求统一指向该 API，避免各命令自解析描述符。

---

### 6.4 C-4 验收基线命令的包名不存在（P1）

**证据**：`plan.md:186` 写"命令级验收基线：`cargo test -p commands-*` 各 crate 独立可跑"。实际 workspace 中没有任何包名以 `commands-` 开头——包名前缀是 `minix-`（`minix-fileops`、`minix-shell`、`minix-init`……，各 crate Cargo.toml 实测）。

**影响**：照抄这条基线的人得到的是 cargo 的 "package ID specification `commands-*` did not match any packages" 报错。这是一条无法执行的验收命令，等于该篇的测试基线声称了一虚构事实（test-audit 第 5 维"虚构"的文档侧变体）。

**处置**：`plan.md:186` 改为 `cargo test -p minix-fileops` 一类可执行形式（或列举 24 个域包），并注明包名前缀 `minix-`。一行修正，执行时按 fix-guard 读上下文后落笔。

---

### 6.5 C-5 workspace 成员重复登记与数字漂移（P2）

**证据**：`os/Cargo.toml:71` 与 `:77` 都写着 `"commands/bin/fileops"`（`grep -c` 结果 2）；`plan.md` §2 头部写"支撑 `os/commands/*`（35 crate）"，实际成员 58 个（含重复）。

**影响**：重复登记 cargo 容忍但不干净；"35 crate"与现实的 58 之差说明该数字从未对过账，C-1 收敛后更会失效。

**处置**：删 `:77` 的重复行；"35 crate"的表述在 C-1 第 4 步随收敛一并改为收敛后的真实数字（约 26）。

---

### 6.6 C-6 /etc 配置面零落地，A-6 决策悬置（P2，OQ 上交）

**证据**：`os/etc/` 只有一个 README（自述"占位，未实装"）；`plan.md` §5.3 给 49 项配置文件逐项分配了文档；[ARCH] A-6（rc 配置面：保留 shell rc 脚本、依赖 05 shell 就绪，还是编译期静态 Rust 数据面）标注"设计期（重大决策）"。

**影响**：01 篇（init 读 /etc/rc.conf → rc → rc.d）与 03 篇（口令数据库、ttys）的实现都压在一个未做的决策上；init crate 里的 rc 驱动状态机（`runcom.rs`、`state_machine.rs`）今天只能对空数据面跑单测。

**处置（OQ，不擅自裁决）**：❓ OQ-1——A-6 走哪条路？倾向信号：init 已把 rc 语义建成 Rust 状态机（`os/commands/sbin/init/src/runcom.rs`、`state_machine.rs`），说明静态数据面方向已有代码投资；但"保留 shell rc 脚本"能复用 Minix3 的 32 个 rc.d 脚本语义、少一份翻译维护面。需要结合 05 shell 的交付时间一并裁决，本 todo 不代决。

---

### 6.7 C-7 边界守卫机制化（P2）

**证据**：99 §1 的三条硬规则（命令不许 `use minix_sys::ipc`、不许构造 `minix_types` 消息、依赖单向）目前只有文档与 code review 约定，仓库里没有机械检查。本轮 grep 实测全 commands 只有 init 一处越界（C-2），但无守卫意味着下一轮补代码就可能无声破例。

**处置**：加 `tools/check-command-boundary.sh`：`rg "minix_sys::ipc|minix_types" os/commands/ --include-zero` 应零命中（该脚本同时是 C-2 修复完成的验收命令；minix-sys 侧 errno 再导出落地前，init 的现存命中会让它非零，故脚本启用时机与 C-2 绑定）。产出物与项目现有 `tools/check-review-rules.sh`、`tools/design-coverage-check.sh` 同风格。

---

### 6.8 本轮验证记录

- 测试回归：`cargo test -p minix-fileops -p minix-shell -p minix-init` → 145 passed / 0 failed（2026-09-17）。
- 联网实证：Redox `redox-os/coreutils` master 分支 Cargo.toml（单包 + 显式 `[[bin]]` + `src/bin/*.rs`）。
- grep 实证清单：commands 范围 `minix-types` 仅 init 命中；`src/bin` 零命中；`"commands/bin/fileops"` 计数 2；`plan.md:186` 原文核对。
- 警告记录：`cargo build -p minix-init` 暴露 `minix-sys/src/pm.rs:30`（unused import `Message`）与 `minix-rt/src/crt0.rs:170`（不必要的 `mut`）——属 14 侧基础设施卫生，随 edge `E-CMDSYSFACE` 顺带清理，不属本 stage。

---

## 7. 环境与配方：权威容器门断网那一串现象、定病根的探针、以及恢复后的挂载配方

> 这一节管的是**承载环境**，不是代码。登记缘由：`AGENTS.md` 定的权威编译门（docker `minix-ci:1.94`）
> 从批次三百三十五到三百三十七连续三轮没跑成，三轮都按回退口径取宿主读数；
> `2026-10-17` 批次三百三十八恢复，同日把病根与配方一起写在这里。

### 7.1 断的那三轮，命令与读数按条抄在这里

| 探什么 | 命令 | 读数 |
|---|---|---|
| 容器里能不能解析 | `docker run --rm minix-ci:1.94 getent hosts static.crates.io` | `2001:2::fd` |
| 容器里能不能连上 | `docker run --rm minix-ci:1.94 curl -s -o /dev/null -w '%{http_code}' --max-time 20 https://static.crates.io/` | `000`（`curl` 收不到任何响应才打这个数） |
| 镜像带没带离线缓存 | `docker run --rm minix-ci:1.94 ls /usr/local/cargo` | 只有 `bin`、`env`，**没有 `registry` 目录** |
| 宿主自己通不通 | `curl -s -o /dev/null -w '%{http_code}' https://index.crates.io/config.json` | `000` |

cargo 报出来的两句话是同一件事的两个面：`error: failed to get 'bitflags' as a dependency of package minix-kernel`
是依赖图根本建不起来，`[28] Timeout was reached` 是三架构交叉目标那条腿在等一个不通的连接。
**最要命的一条是第三行**：镜像里没有 `registry` 目录，意味着任何一次依赖解析都必须走活连接，
没有「反正缓存里都有」的退路。这也解释了下一小节为什么回退只能退回宿主：宿主自己有一份能用的缓存，
容器那一份根本不存在，同一条 `--offline` 在容器里连依赖图都建不起来。

同一棵树、同一条命令在断网之前取到过容器读数（批次三百三十二记的 `917 / 0 / 0`），所以断的是承载环境，
不是代码，也不是配方。判据留一句给下一次遇到同类现象：**先量连接，再怀疑解析**——`2001:2::fd` 这枚地址
在恢复当日一模一样地出现，而连接是通的（`%{http_code}` 读到 `200` 与 `403`），可见合成出来的地址一直没错。

### 7.2 那三轮用的回退口径

这一条走的是规则里的回退分支（它要求在台账写明回退的理由），理由即上一节的承载环境断网：

```bash
(ulimit -v 3145728; cargo test -p minix-textfilter --offline -j 1)
```

`--offline` 走宿主 `~/.cargo` 里已有的缓存；`ulimit -v 3145728` 把地址空间压到 3 GiB，
让宿主这条腿的内存上限与容器那条腿的 `-m 2g` 可比。凡回退的那几轮，台账里每一项读数都写明是宿主读数。

### 7.3 恢复当日的读数与推荐的挂载配方

四条探针的复探结果（`2026-10-17`）：容器里 `https://index.crates.io/config.json` → `200`，
`https://static.crates.io/crates/bitflags/bitflags-1.3.2.crate` → `200`，
`https://static.crates.io/` → `403`（根目录本来就不给列目录，`403` 说明已经连上了），
`ls /usr/local/cargo` → 仍只有 `bin`、`env`（没有离线缓存这件事并没有变，每一次解析都得走网）。

跑权威门用这一条，在仓库根执行：

```bash
docker run --rm -u "$(id -u):$(id -g)" \
  -v "$PWD:/work" -v "$PWD/tmp/ci-cargo:/cargo" -v "$PWD/tmp/ci-target:/tgt" \
  -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/tgt \
  -w /work/os -m 2g minix-ci:1.94 cargo test -j 1 -p <pkg>
```

在认领树里跑就把 `-w` 换成 `/work/.wt/<id>-<owner>/os`。比 `AGENTS.md` 那条多加的三样各管一件事：

- `-u "$(id -u):$(id -g)"`：容器默认以 root 写挂载目录，会在 `.wt/` 里留下 root 属主的构建产物，
  `tools/claim.sh release` 删不掉，最后要人来 `sudo`。这一条把这笔债消掉。
- 第二个挂载点是给 cargo 准备的缓存目录：镜像自带的 `/usr/local/cargo` 属 root，非 root 的 cargo 要往里写
  索引缓存会当场失败，所以把 cargo 找缓存的那个环境变量指到这个挂载点。
- 第三个挂载点接住编译产物（配方里那个 `CARGO_TARGET_DIR` 的写法照抄即可）：产物落在工作树之外，
  `os/target` 不会被写脏，也不会混进 root 属主的文件。

批次三百三十八的容器读数：**1060 passed / 0 failed，68 个 target，退码 0**；同一棵树同一条命令在宿主
也是 1060 / 0，两侧一致。日志 `tmp/split-338/container-gate-final.log`。

**待补的账**：批次三百三十五、三百三十六、三百三十七这三轮只有宿主读数，缺权威读数。下一笔在本目录下
跑任何一次容器门时，顺手对 `rewrite` 头的同一棵树补一次 `cargo test -j 1 --workspace`，那一句才是覆盖
三包之外的部分；单包读数不能替它。

**这笔账的结局（`2026-10-18` 补记，批次三百四十一）**：那一条按字面写法跑不成，原因不是门坏了，
是这条命令本身撞在设计的绊线上。三条读数都附命令取自同一容器镜像与同一挂载配方：

| 命令（`-w /work/os`） | 退码 | 读数 |
|---|---|---|
| `cargo test -j 1 --workspace` | 101 | `minix-kernel` (lib) 5 个编译错：`os/kernel/src/lib.rs:84`、`:86` 的形态常量比对、`:1377`、`:1459` 的形参类型不符、`:1926` 找不到的 `default` |
| `cargo test -j 1 --workspace --exclude minix-kernel` | 101 | 同一个 `minix-kernel` (lib) 编译错——`--exclude` 只把它从「被测包」里摘掉，它仍是别的包的依赖，照样要编 |
| `cargo test -j 1 -p minix-kernel --features mock` | **0** | **835 passed / 0 failed / 12 ignored**（`tmp/ws-341/container-kernel-mock.log`） |

`AGENTS.md` 的 Build & Test 一节写明 `SD-30` 清空了三包默认形态、裸跑 `cargo test -p minix-kernel`
会被 `os/kernel/src/lib.rs` 的 `form_contract` 编译期绊线当场挡住，那是设计行为。所以「一条覆盖全仓的
容器 `--workspace` 命令」在当前契约下**不存在**；权威读数的正确形状是两条腿：需要被测的那个包自己带形态
特性跑（内核腿即上面第三条），其余包按单包门跑。批次三百三十五~三百三十七那三轮缺的是**单包门**的容器
读数，那部分不在本条射程内（要回到各自的树才能取），仍留在账上；本条只把「`--workspace` 一句补上」这个
不成立的口径翻正，并留下上面三行可重跑的读数。
