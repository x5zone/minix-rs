# 05-shell 家族与环境

> **状态**: 已完成，等待评审收敛
> **定位**: 交付因果链的交互起点——登录程序交棒之后，谁解释用户敲下的每一行字
> **源码**: `minix3/bin/sh/`（Almquist shell，22 个 C 文件：`parser.c` 1686 行、`expand.c` 1640 行、`eval.c` 1366 行、`jobs.c` 1532 行、`exec.c` 1071 行、`redir.c` 400 行、`main.c` 376 行、`builtins.def` 93 行）、`minix3/bin/ksh/`、`minix3/bin/csh/`、`minix3/bin/hostname/hostname.c`（101 行，主函数第 57 行）、`minix3/usr.bin/uname/uname.c`（159 行，系统调用第 111 行）、`minix3/usr.bin/env/env.c`（99 行）、`minix3/usr.bin/getopt/getopt.c`（41 行）、`minix3/minix/commands/sysenv/sysenv.c`（81 行）、`minix3/etc/profile`、`minix3/etc/shrc`、`minix3/etc/csh.cshrc`、`minix3/etc/csh.login`、`minix3/etc/csh.logout`、`minix3/etc/hostname.file`（内容 `minix`）
> **Rust 模块**: `os/commands/bin/shell`（库包 `minix-shell`：`lexer.rs`、`expand.rs`、`redir.rs`、`script.rs`，36 个测试通过）；`os/commands/bin/sh` 仍为启动占位（派生执行待进程原语）
> **前置依赖**: `03-login-passwd.md`（登录链交棒到 shell）、终端属性（`13-terminal-termios.md` 的行规程部分）
> **不覆盖（移交）**: 命令工具本体（见 `06` 到 `24` 各篇）、终端驱动（见 `16-stage-drivers` 的终端部分）、作业控制的进程组实现（见进程管理阶段）

---

## 1. 概念：shell 是用户与系统之间的翻译官

### 1.0 本章说明

本章讲用户敲下的字符如何变成系统的动作：shell 读什么文件起步、如何把一行字切成单词、如何展开变量、如何接管输入输出、有哪些内建命令，以及三种 shell 方言的区别。

> **本章不讲什么**：
>
> - 具体每个外部命令的行为（见 `06` 到 `24` 各篇）
> - 作业控制的进程组与信号实现（见进程管理阶段）
> - 算术展开、命令替换、通配符展开的实现（后续阶段，本篇只定边界）
>
> 本章只讲 shell 的语言核心：分词、变量展开、重定向、启动文件、环境命令。

### 1.1 为什么 shell 既是程序又是语言

初学者容易把 shell 当成"一个 REPL（读取、求值、打印、循环）程序"，这只说对一半。shell 的另一半是一种完整的编程语言：启动脚本（`01` 篇的 `/etc/rc` 就是 shell 脚本）、用户模板（`03` 篇的 `/etc/profile`）、包管理脚本全都用它写。理解 shell 必须同时抓住两面：交互时它是翻译官（把键盘输入翻成进程创建），脚本里它是解释器（逐行执行、遇错继续或退出）。

这个双重身份解释了 shell 设计中很多"怪异"之处：为什么变量没有类型（字符串即一切，交互与脚本统一）、为什么错误默认不中断脚本（交互时敲错一行不能把 shell 炸掉，这个容忍度被带进了脚本语义）、为什么有"内建命令"（有些操作在子进程里做没有意义，见 1.4 节）。

### 1.2 三种方言：精简、Korn 风格、C 风格

Minix 带着三个 shell，不是冗余，而是三段历史：

- **精简 shell**（`bin/sh`，Almquist shell）：系统默认 shell，也是 `/bin/sh` 的指向。它小而快，语法是 POSIX 规定的可移植子集。启动脚本全部用它写——因为开机时只能假设这个 shell 存在。
- **Korn 风格 shell**（`bin/ksh`）：交互增强版，带命令行编辑、历史、关联数组等，适合日常登录使用。它的脚本兼容精简 shell 的语法，是超集关系。
- **C 风格 shell**（`bin/csh`）：语法模仿 C 语言（`if (...) then` 加括号），启动文件也自成体系（`csh.cshrc`、`csh.login`、`csh.logout`）。它与前两者语法不兼容，选用它意味着脚本要重写。

三者的关系可以用一句话记：写系统脚本用精简 shell（可移植），日常交互三者任选（个人习惯），写个人脚本跟随自己用的交互 shell（避免两种语法在脑子里打架）。

### 1.3 启动文件：登录、交互、非交互三条路

新 shell 启动时读哪些文件，取决于两件事：是不是登录 shell（程序名以横杠开头，见 `main.c` 第 198 行），以及是不是交互的。Almquist shell 的规则（`main.c` 第 196 到 215 行）可以画成一张表：

| 情形 | 读什么 | 为什么 |
|------|--------|--------|
| 登录 shell | 先 `/etc/profile`，再 `~/.profile` | 登录是"进门"，进门先看全系统告示，再看个人备忘 |
| 交互的非登录 shell，且用户标识一致 | `ENV` 变量指向的文件 | 每个新终端都要趁手，但别重复跑登录那套重的 |
| 特权身份不一致（真实与有效标识不同） | 跳过用户文件 | 用户控制的文件不可信，这是经典安全规则 |
| 非交互脚本 | 都不读 | 脚本要的是可预测，读用户文件会引入意外 |

实例：`minix3/etc/profile` 设置库路径与时区（全系统登录生效），`minix3/etc/shrc` 按主机名定制提示符（交互生效）。`ENV` 机制的高明之处在于把选择权交给用户：`ENV=/etc/shrc` 即全局交互配置，用户也可指向自己的文件覆盖。

### 1.4 内建命令：必须在 shell 自己体内执行的命令

大部分命令是外部程序（fork 出子进程执行），但有一类命令在子进程里执行毫无意义：`cd` 改的是进程自己的工作目录（子进程改完就退出，父 shell 纹丝不动）、`export` 改的是自己的环境表、`exit` 结束的是自己、`read` 读进的是自己的变量、`umask`、`wait`、`shift`、`eval`、`exec` 同理。`builtins.def`（93 行）登记了全部内建命令，每个条目标注了特殊性（`-s` 特殊内建：POSIX 要求错误直接退出脚本；`-u` 普通内建）。

判断题：`echo`、`printf`、`test`（`[`）既是内建又是外部程序——内建版本快（无派生开销），外部版本供 `find -exec`、`xargs` 这类"只能启动外部程序"的场合调用。两者行为必须一致，这是 POSIX 合规测试重点覆盖的。

### 1.5 环境命令：查看与构造执行环境的小工具

- `env`（99 行）：打印当前环境，或在修改后的环境里执行命令（`env 变量=值 命令`）。脚本用它实现"干净环境执行"，第一行 `#!/usr/bin/env 程序名` 也是这个机制的副产品。
- `printenv`：只打印（`env` 的只读子集）。
- `getopt`（41 行）：帮 shell 脚本解析选项（`optind = 2` 跳过程序名与选项字母表的细节在第 23 行），是"让脚本支持 `-abc` 捆绑选项"的标准件。
- `sysenv`（81 行）：Minix 特有的系统环境查询（经系统控制接口取系统参数），是 `svrctl`（`02` 篇）思想在环境面的延续。
- `hostname`（101 行）：无参数打印（经取主机名系统调用，第 83 行），有参数设置（经设主机名系统调用，第 80 行，限管理员）。出厂主机名存在 `etc/hostname.file`（内容就是 `minix` 五个字母）。
- `uname`（159 行）：经系统信息系统调用（第 111 行）打印系统名、版本、硬件架构等，是脚本判断"我跑在什么机器上"的标准入口。

---

## 2. C 源码分析

### 2.1 启动：`main.c` 第 196 到 215 行

主函数先判断程序名首字符是否为横杠（第 198 行）：是则依次读系统启动文件与用户启动文件（第 200 到 203 行）。随后进入第二段（第 205 到 211 行）：交互或非严格标准模式、且真实与有效用户组标识两两一致时，读 `ENV` 变量指向的文件。两段之间用带标号的状态注释分隔（`state1`、`state2`、`state3`），是老式 C 程序用标号表达"阶段"的写法。Rust 侧 `plan_startup` 与这段逐项对应（见第 4.2 节）。

### 2.2 分词：`parser.c` 的底层问题

语法分析器（1686 行）之上，最底层回答的是本库 `lexer.rs` 的问题：引号、反斜杠、注释如何影响分词。C 侧分词与建语法树交织（读到单词立即决定节点类型）；Rust 侧只做到单词层——因为命令工具（`env` 的参数、`xargs` 的输入、启动文件的逐行读取）需要的恰好是这一层。语法树（管道、条件、循环、函数定义）是后续阶段的事，本篇在第 3.4 节明确留白。

### 2.3 展开：`expand.c` 的七种展开

展开器（1640 行，入口 `expandarg` 第 138 行）处理七种展开：变量、命令替换、算术（`expari` 第 355 行）、路径通配、波浪线、字段切分、引号去除。本库实现变量子集（见第 3.2 节），其余六种的缺席不是遗漏而是排序：变量展开是启动文件与环境工具的刚需，其余六种依赖执行器（命令替换要派生进程、通配要读目录、算术要表达式求值），在执行器就绪前实现它们只能是桩。

### 2.4 重定向：`redir.c` 的算子与 discipline

重定向层（400 行）分两层：识别算子（本库 `redir.rs`），执行重定向（复制文件描述符、执行完恢复，见文件头注释第 99 到 119 行的压栈弹出 discipline）。节点类型（`NTOFD`、`NFROMFD`，第 130 行）说明 C 侧把"复制描述符"与"打开文件"建模成不同节点——Rust 侧的操作枚举（七种）与之一一对应，只是把"执行"留给了以后。

### 2.5 求值与作业：`eval.c` 与 `jobs.c` 的边界

求值器（1366 行）遍历语法树执行命令，作业控制（1532 行，`fgcmd`、`bgcmd` 第 288 到 403 行）管理前后台任务组。两者都依赖进程原语（派生、等待、进程组、终端前台），本阶段一个都不实现——文档在这里明确写出依赖链（进程管理阶段 → 执行器 → 作业控制），而不是含糊地说"以后再说"。

### 2.6 内建登记：`builtins.def`

93 行的登记表是"内建全集"的权威来源：`cd`、`echo`、`eval`、`exit`、`export`、`printf`、`read`、`shift`、`umask`、`wait`、`test`（含 `[` 别名）等。每行标注特殊性，生成脚本据此产生分派表。Rust 侧当前不实现任何内建执行（全部是执行器职责），但 `06` 篇的 `test` 表达式求值器正是 `test` 内建的决策核心——内建的"判断"部分先行、"执行包装"随后，是本阶段一以贯之的策略。

---

## 3. Rust 设计决策

### 3.1 为什么先做"文字层"，不做"执行层"

一个 shell 从文字到动作要过五关：读入、分词、展开、执行、作业管理。后两关需要进程原语（本阶段没有），前三关是纯文字处理。把前三关做成无标准库库，立即获得三样东西：启动文件的解析能力（`01` 篇悬置的 A-6 向前走了一步）、环境工具的参数处理、可独立测试的 36 个测试。反之，若硬上执行器，只能得到一堆"创建进程（未实现）"的桩——上一阶段评审确立的"诚实留白优于虚假实现"原则在这里同样适用。

这个分层与 Redox 的做法一致：Redox 的用户程序把参数解析与决策放在普通库里充分测试，系统调用集中在薄层。本库走得更远——连标准库都不依赖。

### 3.2 为什么变量展开只需要读接口

展开器要查变量，但"变量存在哪"（进程环境块、shell 内部表、启动文件预置）是执行器的事。于是定义只读接口 `Environ`（按名取值），配两个实现：`EmptyEnv`（全缺席，安全默认值）与 `TableEnv`（16 条内存表，测试与启动文件预置用）。赋值语义（`${变量=值}`）被响亮地拒绝——没有堆内存就存不住新字符串，假装支持只会埋下悬垂引用。这个拒绝是深思熟虑的：Redox 的同类解析器同样把"展开"与"赋值"分属不同层。

### 3.3 为什么单词是"片段串"而不是"字符串"

引号去除让单词不再是输入的连续子串（`a"b c"d` 的内容是 `a`、`b c`、`d` 三段）。两种表示可选：拼成新字符串（要分配内存），或保留片段串（零拷贝）。本库选片段串（`Word`：8 个借用片段加最强引用级别），调用方按需拼接。代价是调用方多写一个拼接循环，收益是全程无分配——在无标准库约束下，这是唯一不撒谎的表示。

### 3.4 明确留白：语法树、执行器、作业控制

`lexer` 之后的三关（语法树、执行、作业）不在本库：语法树需要递归节点（无分配 arena 是后续设计课题），执行需要进程原语，作业需要进程组与终端前台。三者各有一篇后续文档的位置，本篇把接口切在"单词串进、单词串出"的边界上——执行器就绪时，`lexer`、`expand`、`redir`、`script` 原样成为它的前端。

---

## 4. 实现详解

### 4.1 模块结构

`os/commands/bin/shell`（库包名 `minix-shell`）共 5 个源文件：

| Rust 文件 | 对应 C 源码位置 | 职责 |
|-----------|----------------|------|
| `lib.rs` | — | 错误类型（`ShellError`，22 对应语法错误与超长）与模块组织 |
| `lexer.rs` | `parser.c` 分词底层 | 引用感知分词（`split_words`，单词为片段串） |
| `expand.rs` | `expand.c` 变量子集 | `Environ` 接口、`EmptyEnv` 与 `TableEnv`、 `$`、`${}` 展开 |
| `redir.rs` | `redir.c` 算子层 | 七种重定向算子识别（`parse_redir`） |
| `script.rs` | `main.c:196-215` | 启动文件序列决策（`plan_startup`） |

### 4.2 关键类型与不变量

- **单词 `Word`**：8 个借用片段加引用级别。不变量：片段全部借用同一输入行；引用级别是片段中最强者（单引号强于双引号强于无）；空单词（`''`）片段数为零但单词有效——调用方用 `is_empty` 区分"无词"与"空词"。
- **环境接口 `Environ`**：按名取值。不变量：缺席返回空（不是错误）；`TableEnv` 首中获胜（与 C 库"先定义胜出"的顺序语义一致）。
- **重定向 `Redirection`**：算子、描述符、目标。不变量：双字节算子优先匹配（`>>` 赢 `>`，`>&` 赢 `>`，`<>` 赢 `<`）；无目标即语法错误；描述符超 255 即错。
- **启动计划 `StartupPlan`**：有序文件表。不变量：登录必读两文件；特权不一致跳过用户文件；空 `ENV` 视同未设。

### 4.3 函数一览

| 函数 | 输入 | 输出 | 对应 C 行为 |
|------|------|------|------------|
| `split_words(行)` | 文本行 | 单词表 | 分词底层语义 |
| `expand_word(单词, 环境, 状态, 进程号, 缓冲)` | 单词与上下文 | 展开字节数 | `expandarg` 的变量子集 |
| `parse_redir(单词)` | 单词 | 重定向或无 | 算子识别语义 |
| `plan_startup(种类, 交互, 严格, 标识一致, 环境文件)` | 启动上下文 | 文件序列 | `main.c:196-215` |

---

## 5. 测试要点

`cargo test -p minix-shell`：**36 个测试，全部通过**（截至 2026-09-06）。

重点行为与测试的对应（以下函数名均可用 `rg "fn 测试名" os/commands/bin/shell` 复现）：

- **分词**（`lexer.rs`，10 个）：`test_blanks_separate_words`（空白分隔）、`test_comment_dropped`（注释截断）、`test_single_quotes_verbatim` 与 `test_double_quotes_keep_blanks`（两种引用）、`test_mixed_word_runs` 与 `test_mixed_word_flattens`（混合词三片段与拼接）、`test_backslash_escapes_blank`（转义空白）、`test_unterminated_single_quote_rejected` 与 `test_unterminated_double_quote_rejected`（未闭合引用）、`test_empty_line_gives_no_words`（空行）。
- **展开**（`expand.rs`，13 个）：`test_plain_lookup` 与 `test_braced_lookup`（两种变量写法）、`test_unset_expands_empty`（缺席为空）、`test_default_when_unset`、`test_default_keeps_set_value`、`test_colon_default_treats_empty_as_unset`（冒号版视空为缺席、非冒号版只认缺席）、`test_alternate_value`（条件值）、`test_error_when_unset`（问号报错）、`test_assign_rejected_loudly`（赋值响亮拒绝）、`test_unclosed_brace_rejected`（花括号未闭合）、`test_special_parameters`（退出状态与进程号参数）、`test_strong_quote_suppresses`（单引号抑制）、`test_env_trait_objects`（接口统一）。
- **重定向**（`redir.rs`，7 个）：`test_output_truncate`、`test_error_redirect_with_fd`（描述符前置）、`test_append_and_input`、`test_duplicate_and_close`（复制与关闭）、`test_read_write_and_clobber`（读写与强制覆盖）、`test_plain_word_is_none`（普通词不是重定向）、`test_missing_target_rejected`（无目标报错）。
- **启动序列**（`script.rs`，6 个）：`test_login_shell_reads_both_profiles`（登录两文件）、`test_interactive_shell_reads_env_file`（交互读环境文件）、`test_login_plus_env_chains_three`（登录加交互三文件）、`test_privileged_shell_skips_env`（特权跳过）、`test_empty_env_reads_nothing`（空环境文件视同未设）、`test_batch_shell_reads_nothing`（批处理不读）。

尚未覆盖、随后续阶段补齐的：命令替换、算术展开、通配展开（依赖执行器与文件系统）、语法树与执行器、作业控制（依赖进程原语）。文字层是全覆盖的，执行层是显式留白的。

---

## 6. 过渡：会说话之后，去搬东西

本篇走完了交付因果链的交互起点：shell 知道读哪些文件起步、如何切词、如何展开变量、如何识别重定向。用户在提示符后敲下的每一个命令，都要经过这四道工序才变成进程。

但翻译官只管"说什么"，不管"做什么"：`cp` 如何复制、`ls` 如何列目录、`test` 如何判断、`find` 如何遍历——这些"动手"的命令是 `06-file-ops.md` 的职责。请沿因果链继续向下走：先有"解释命令的 shell"，再有"被 shell 调用的命令"。

---

## 7. 参见

- `03-login-passwd.md`——登录链交棒到 shell（本篇的入口）
- `06-file-ops.md`——文件操作命令（下一步：shell 调用的命令）
- `13-terminal-termios.md`——终端属性（交互的物理基础）
- `minix3/bin/sh/main.c:196-215`——启动文件逻辑（`plan_startup` 的逐行对照）
- `minix3/bin/sh/builtins.def`——内建全集（93 行）
- `minix3/bin/sh/expand.c:138`——展开入口
- `minix3/bin/sh/redir.c:99-130`——重定向 discipline 与节点类型
- `minix3/etc/profile`、`minix3/etc/shrc`——启动文件实例

---

## 附：验证记录（评审用，可跳过）

- `wc -l minix3/bin/sh/parser.c minix3/bin/sh/expand.c minix3/bin/sh/eval.c minix3/bin/sh/jobs.c minix3/bin/sh/exec.c minix3/bin/sh/redir.c minix3/bin/sh/main.c` → 1686、1640、1366、1532、1071、400、376 行，与正文引用一致。
- `sed -n '196,215p' minix3/bin/sh/main.c` → 登录判断与环境文件逻辑已读，与 `plan_startup` 逐项对应。
- `grep -c "" minix3/bin/sh/builtins.def` → 93 行，与正文"93 行"一致。
- `cat minix3/etc/hostname.file` → `minix`，与正文一致。
- `cargo test -p minix-shell` → 36 通过、0 失败；`cargo clippy` 无警告。
- 本文档引用的 `file:line` 均来自正文写作前实际执行的 `rg -n` 与 `sed -n` 输出，非凭记忆书写。
