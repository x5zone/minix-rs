# 18-stage-commands 实施规格缺口 TODO

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
