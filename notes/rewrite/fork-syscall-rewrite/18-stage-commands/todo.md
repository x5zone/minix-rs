# 18-stage-commands 实施规格缺口 TODO

> 来源：2026-09-17 设计讨论。讨论从"命令如何获得标准库支持"出发，逐层确认了三件事：POSIX 规定的是接口契约而不是系统调用清单；Minix3 的库分层是 `minix3/lib/libc/`（C 库）与 `minix3/minix/lib/libsys/`（消息层）两个目录；Redox 用 relibc（Rust 写的 C 标准库 + POSIX）承接 Rust std 的调用。讨论结论随后与 18-stage-commands 现有文档逐条对账。
> 范围：`notes/rewrite/fork-syscall-rewrite/18-stage-commands/`。本文只登记文档缺口与修复去向，不修改生产代码。
> 定位：`plan.md` 管覆盖契约（哪些命令、哪些 C 源、归哪一篇），本文管实施规格（命令依赖谁、需要哪些 API、行为以什么为准）。两者互补，不重复。
> 状态（2026-09-17）：P0 一项、P1 两项、跨阶段挂账一项。三项契约正文已落入 `99-global-concepts.md`（§1 分层契约、§2 行为判定基准、§3 Requires 列）；命令文档回填与 14-stage-runtime 侧措辞修正待后续轮次。

---

## 0. 条目速览

| 级别 | 编号 | 一句话 | 状态 |
|------|------|--------|------|
| **P0** | P0-1 | 命令层依赖契约悬空：`plan.md` 说命令消费 "exec/stdio/termios/socket API"，而 14-stage-runtime 的 [ARCH] A-2 已决定不移植 libc 的 stdio，"stdio" 不指向任何待建库 | 契约正文已写入 `99-global-concepts.md` §1；`plan.md` 措辞待改 |
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
- 待做：把 `plan.md:206`、`:394`、`:405`、`:407` 的 "stdio/termios" 措辞改为指向 `99-global-concepts.md` §1。
- 待做：14-stage-runtime 的 plan 补一句说明——stdio 无专篇是因为 A-2 决定用 `core`/`alloc` 替代，命令的输出通道是 `minix-sys` 的 `write`（`os/libs/minix-sys/src/lib.rs:217`）。
- 验证：`grep -n "stdio" plan.md` 当前 8 处（`:24`、`:68`、`:105`、`:177`、`:206`、`:394`、`:405`、`:407`，2026-09-17 实测），改完后每一处都应能对到 §1 的表述。

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
