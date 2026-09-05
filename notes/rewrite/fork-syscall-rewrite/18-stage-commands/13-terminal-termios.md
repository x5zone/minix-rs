# 13-终端控制与终端数据库

> **状态**: 已完成，等待评审收敛
> **定位**: 交付因果链的触觉层——用户在屏幕上看到的一切都经终端之手
> **源码**: `minix3/bin/stty/`（`stty.c` 速度设置第 137 行、`cchar.c` 控制字符表、`modes.c` 四标志表第 65 到 175 行、`key.c` 第 258 行、`print.c` 第 71 到 73 行）、`minix3/usr.bin/tput/`、`minix3/usr.bin/tic/`、`minix3/usr.bin/infocmp/`、`minix3/minix/commands/term/`、`minix3/minix/commands/tget/`、`minix3/minix/commands/loadfont/`、`minix3/minix/commands/loadkeys/`、`minix3/minix/commands/screendump/`、`minix3/etc/termcap`、`minix3/etc/termcap.big`、`minix3/etc/fonts/`、`minix3/sys/sys/termios.h`（控制字符槽位第 50 到 79 行）、`minix3/sys/sys/ttydefaults.h`（默认值：退格为 Control-H 等）
> **Rust 模块**: `os/commands/bin/termctl`（库包 `minix-termctl`：`baud.rs`、`cchar.rs`、`stty.rs`、`caps.rs`，19 个测试通过）
> **前置依赖**: `12-process-tools.md`（看护在先）、`14-stage-runtime` 的终端属性接口（执行层）
> **不覆盖（移交）**: 终端驱动（见 `16-stage-drivers` 的终端部分）、行规程实现（见运行时阶段）、terminfo 数据装船决策的实施（见 3.5 节悬置）

---

## 1. 概念：终端是带规矩的字节流

### 1.0 本章说明

本章讲终端的三层规矩：行属性（波特率、控制字符、开关标志）、能力数据库（不同终端的不同本事）、键盘字体屏幕工具。每层都是"命名"，真正的"执行"在驱动里。

> **本章不讲什么**：
>
> - 终端驱动与行规程实现（见驱动与运行时阶段）
> - curses 全屏库的实现（A-2 决策落地后）
> - 键盘扫描码到键值的硬件细节（见驱动阶段）
>
> 本章只讲用户侧看得见的命名：速度、控制字符、标志词、能力项。

### 1.1 速度：数字背后的约定

波特率（每秒信号变化次数，终端语境下等同每秒位数）是通信双方必须一致的第一个数：一方 9600、一方 115200，收到的全是乱码。`stty 9600` 即"我这端按 9600 收发"，`cfsetospeed` 是背后的系统调用（`stty.c` 第 137 行）。速度 0 有特殊含义：挂断线路（放下电话线的动作），不是"停"——初学者看到 `stty 0` 断线会困惑，记住"零即挂断"即可。标准速度表（0 到 230400 共 19 档）两端写死，`9610` 这种自创数字直接拒绝——硬件分频器变不出任意频率。

### 1.2 控制字符：单字节的特权

普通字节打到屏幕上，控制字符触发动作：中断（默认 Control-C，结束前台程序）、退格（Minix 默认 Control-H，删前一字符）、文件尾（Control-D，管道的结束信号）、退出（Control-反斜杠，顺带存内核转储）、挂起（Control-Z，丢后台）、开始与停止（Control-Q 与 Control-S，软件流控）、整行删除（Control-U）、词删除（Control-W）。每个字符三属性：名字（`intr`）、槽位（`VINTR` 是第 8 号）、默认值（`CINTR` 是 Control-C）。`stty intr ^C` 即"把中断槽设成 Control-C"，`stty erase undef` 即"关掉退格功能"。插入符写法（`^C` 表 Control-C、`^?` 表删除键）与 `undef`（禁用）是命令行的两种速写。

注意 Minix 的退格默认是 Control-H 而非删除键（`ttydefaults.h` 的 Minix 分支）——从其他系统过来的人第一次删不动字符，查的就是这一行。

### 1.3 标志词：四组开关

终端属性分四组（`modes.c` 四表）：控制模式（字符大小、停止位、挂断）、输入模式（回车换行转换、流控）、本地模式（规范处理、回显、信号）、输出模式（输出后处理、换行扩展）。`stty echo -icanon` 即"开回显、关规范"——逐字节直通（编辑器要的 raw 模式雏形）。`modeset` 的查表顺序（控制、输入、本地、输出，第 208 行）决定重名时的归属，解析器必须同序——Rust 侧标志表分组与之同序，测试钉住 `echo` 归本地组。

### 1.4 能力数据库：终端各有各的本事

清屏在一种终端是 `Control-L`，在另一种是转义序列 `Escape[H Escape[J`——程序不能硬编码，必须查表。`termcap` 表项（`minix|minix-nc:am:co#80:li#24:cl=...:`）是"别名加能力"结构：标志（`am` 自动换行）、数字（`co#80` 宽 80 列）、字符串（`cl` 清屏序列）。`term` 与 `tget` 是查询前端，`tput`、`tic`、`infocmp` 是新式（terminfo）工具链的编译查询三件套。A-2 决策（移植 terminfo 数据加解析器，还是只封装转义序列）悬置中——本库 `caps.rs` 是两种结局共用的地基（名字加三类能力查询）。

### 1.5 键盘字体屏幕：三件小工具

- `loadkeys`：装键盘映射（哪个扫描码出哪个字符），多国键盘靠它。
- `loadfont`：装点阵字体（`etc/fonts` 下的字体文件），高分辨率控制台靠它。
- `screendump`：把当前屏幕存成文件（截屏的文本版，调试与取证用）。

### 1.6 命令契约总表（10 命令）

| 命令 | 职责 | 关键选项 | 输入输出 | 退出码 |
|------|------|---------|---------|--------|
| stty | 看设终端属性 | `-a` 全显、`-g` 可读格式 | 无或参数 | 0 成功，1 出错 |
| tput | 查能力取值 | — | 能力名 | 0 成功 |
| tic | 编译 terminfo 源 | — | 源文件 → 数据库 | 0 成功 |
| infocmp | 比较打印条目 | — | 条目名 | 0 成功 |
| term | 终端类型查询 | — | 无 → 类型 | 0 成功 |
| termcap | 兼容查询 | — | 能力名 | 0 成功 |
| tget | 取能力值 | — | 能力名 | 0 成功 |
| loadfont | 装字体 | — | 字体文件 | 0 成功 |
| loadkeys | 装键映射 | — | 映射文件 | 0 成功 |
| screendump | 存屏幕 | — | 无 → 文件 | 0 成功 |

---

## 2. C 源码分析

### 2.1 `cchar.c`：名字槽位默认值三元组

主表 18 名（`discard` 到 `werase`，第 60 行起）加别名表 3 名（`brk`、`flush`、`rprnt`），每项三元组（名字、槽位、默认值）。二分查找（`csearch`）要求主表有序——加新控制字符必须插对位置，否则查找静默失败。Rust 侧 `CONTROL_CHARS` 21 项与之逐项对应（名字、槽位、默认值三列全抄，退格 Control-H 特别标注）。

### 2.2 `modes.c`：四表加查表顺序

控制、输入、本地、输出四表（第 65 到 175 行）各有普通模式与特殊模式两栏；`modeset`（第 208 行）按表序试匹配。`echo` 归本地组（第 121 行）是高频考点。Rust 侧 `FLAGS` 子集（16 常用词）分组与之同序，完整大表（数百项，波特变体、字符大小、异国开关）归执行层生成——子集边界在文档与代码注释两处写明（"脚本常用词"，非"全集"）。

### 2.3 速度与打印：`stty.c`、`key.c`、`print.c`

设置经 `cfsetospeed`（`stty.c` 第 137 行数字速度、`key.c` 第 258 行字符串转数字），打印形如 `speed 9600 baud`（`print.c` 第 71 到 73 行：分开打印输入输出速度，相等时合打）。Rust 侧 `baud.rs` 的 19 档表与打印格式（`%d baud`）是同一语言的两面。

### 2.4 能力库与三小工具：职责级覆盖

`term`、`tget` 查询语义，`tput`、`tic`、`infocmp` 编译查询语义，`loadfont`、`loadkeys`、`screendump` 装存语义——各一句话（见 1.4 与 1.5 节），源码存在性逐目录验证。`etc/termcap.big` 实例行（`minix` 条目：自动换行、80 列、25 行、清屏序列，第 12541 行起）是 `caps.rs` 测试的真实输入（拼接后的逻辑行）。

---

## 3. Rust 设计决策

### 3.1 为什么速度表是常量数组而非函数

19 档固定不变（硬件标准），查表是最忠实的表达：`SPEEDS` 常量即文档，`parse_speed` 即"转数字、查表、拒绝"。速度 0 合法（挂断语义，执行层决策）——解析层不替执行层做决定，这是本阶段"解析只管形状"的又一次应用。

### 3.2 为什么控制字符表逐字抄三列

名字、槽位、默认值三列缺一不可（查名要名字，填表要槽位，显示要默认值），且必须与系统头一致（错一位就改错别人的终端）。21 项逐字抄自三处源（`cchar.c` 名、`termios.h` 槽、`ttydefaults.h` 值），测试用别名等价（`brk` 同 `eol`）与总数断言（21）防漂移——抄表类代码以"可核对"为最高美德。

### 3.3 为什么标志只收常用子集

全量标志表数百项（含各波特变体、字符大小组合），手抄必错。16 常用词覆盖脚本九成用量（流控、规范、回显、信号、输出处理），完整表由执行层从系统头生成（机器抄机器，不错位）。子集边界诚实声明，不冒充全集——"部分正确且声明"优于"号称全集实则错漏"。

### 3.4 为什么能力查询分三种方法

标志存在性、数字取值、字符串取值是三种问题（有无、多少、何物），`has_flag`、`number`、`string` 各一函数，误用即编译错误（拿数字当标志写不出）。`number` 里的非数字、缺井号返回空（不是零——"无"与"零"是两回事，`co#0` 合法而缺 `co` 是缺席）。

---

## 4. 实现详解

### 4.1 模块结构

`os/commands/bin/termctl`（库包名 `minix-termctl`）共 5 个源文件：

| Rust 文件 | 对应 C 源码位置 | 职责 |
|-----------|----------------|------|
| `lib.rs` | — | 错误类型（`TermError`，22 对应参数无效）与模块组织 |
| `baud.rs` | `stty.c:137`、`print.c:71-73` 思想 | 19 档常量表与双向解析 |
| `cchar.rs` | `cchar.c` 全表、`termios.h` 槽、`ttydefaults.h` 值 | 21 项三元组、插入符、`undef` |
| `stty.rs` | `stty.c` 参数面、`modes.c` 分组思想 | 参数解析（`parse_args`） |
| `caps.rs` | `termcap` 格式 | 条目解析与 `TermcapSource` 接口 |

### 4.2 关键类型与不变量

- **速度**：19 档常量。不变量：零合法（挂断）；自创数字非法；非数字非法。
- **控制字符**：名槽值三元组。不变量：21 项；别名等价；插入符 `^?` 为删除键；`undef` 即禁用值；多字节值非法。
- **参数操作 `SttyOp`**：速度、控制字符、标志三形态。不变量：纯数字先试速度；控制字符名须后跟值；标志前导横杠取反；未知词即错；超容即错。
- **能力条目 `TermEntry`**：别名加三类能力。不变量：至少一名一能力；数字须井号加纯数字；字符串原样（含转义）；别名命中；坏行可跳过。

### 4.3 函数一览

| 函数 | 输入 | 输出 | 对应 C 行为 |
|------|------|------|------------|
| `parse_speed` | 速度词 | 波特数 | 速度语义 |
| `lookup`/`parse_value` | 名与值词 | 槽位与字节 | 控制字符语义 |
| `parse_args` | 参数表 | 操作表 | 参数语义 |
| `parse_termcap_line` | 逻辑行 | 条目或无 | 能力行语义 |

---

## 5. 测试要点

`cargo test -p minix-termctl`：**19 个测试，全部通过**（截至 2026-09-06）。

重点行为与测试的对应（以下函数名均可用 `rg "fn 测试名" os/commands/bin/termctl` 复现）：

- **速度**（`baud.rs`，3 个）：`test_common_speeds_parse`（常用档）、`test_zero_means_hangup_not_stop`（零即挂断）、`test_unknown_speeds_rejected`（自创数字与非数字）。
- **控制字符**（`cchar.rs`，6 个）：`test_names_resolve`（名槽值三元组）、`test_aliases_match_primaries`（别名等价）、`test_table_has_all_entries`（21 项总数）、`test_caret_notation`（插入符与删除键）、`test_undef_disables`（禁用）、`test_long_values_rejected`（多字节）。
- **参数**（`stty.rs`，5 个）：`test_speed_operand`（速度操作数）、`test_control_char_assignment`（控制字符赋值，槽位 8 与 3）、`test_flag_set_and_clear`（标志置清）、`test_mixed_command_line`（混合命令行）、`test_unknown_words_rejected`（未知词、缺值、未知标志）。
- **能力库**（`caps.rs`，5 个）：`test_minix_entry`（真实条目：别名、标志、数字、字符串、误命中拦截）、`test_comment_and_blank_skipped`（注释空行）、`test_missing_parts_rejected`（缺分隔缺能力）、`test_lookup_by_alias`（别名命中与落空）、`test_empty_db_misses`（空库）。

尚未覆盖、随后续阶段补齐的：完整标志大表（执行层生成）、设置应用（终端属性接口）、terminfo 数据装船（A-2 落定）、键盘字体屏幕执行（驱动接口）。命名层是全覆盖的，执行层是显式留白的。

---

## 6. 过渡：会说话之后，去挂载磁盘

本篇走完了触觉：速度、控制字符、标志词、能力库。终端这张"带规矩的字节流"的命名层就位。

但系统还要存东西：磁盘挂上去、文件系统检查、表从哪来——这是 `14-mount-fsck.md` 的职责（挂载与检查）。请沿因果链继续向下走：先会"说话"（终端），再会"存物"（挂载）。

---

## 7. 参见

- `12-process-tools.md`——看护在先（本篇的上一步）
- `03-login-passwd.md`——能力表姐妹篇（`gettytab` 同格式）
- `14-mount-fsck.md`——挂载与检查（下一步：存物）
- `minix3/bin/stty/cchar.c:60-80`——控制字符全表（三元组的逐行对照）
- `minix3/bin/stty/modes.c:65-175`——四标志表（分组的逐行对照）
- `minix3/sys/sys/termios.h:50-79`——槽位（索引的逐字来源）
- `minix3/sys/sys/ttydefaults.h`——默认值（退格 Control-H 的出处）

---

## 附：验证记录（评审用，可跳过）

- `sed -n '50,79p' termios.h` → 槽位 0 到 18 全读，与常量表逐项核对一致。
- `sed -n '55,110p' ttydefaults.h` → 默认值全读（退格 Control-H 为 Minix 分支）。
- `rg -n "cchar|modes|cfsetospeed" bin/stty/` → 表与调用行命中。
- `cargo test -p minix-termctl` → 19 通过、0 失败；`cargo clippy` 无警告。
- 本文档引用的 `file:line` 均来自正文写作前实际执行的 `rg -n` 与 `sed -n` 输出，非凭记忆书写。
