# 06-文件操作命令

> **状态**: 已完成，等待评审收敛
> **定位**: 交付因果链的日常操作主力——shell 调用的命令中，搬运、查看、判断文件的三十个
> **源码**: `minix3/bin/` 十五个（`cat`、`chmod`、`cp`、`df`、`echo`、`expr`、`ln`、`ls` 715 行、`mkdir`、`mv`、`pwd`、`rm`、`rmdir`、`sync`、`test` 717 行）、`minix3/usr.bin/` 十五个（`basename`、`dirname`、`du`、`false`、`find` 306 行、`flock`、`mkfifo`、`mktemp`、`pathchk`、`printf`、`stat`、`touch`、`true`、`xargs`、`xinstall`）、`minix3/usr.sbin/` 三个（`chroot`、`link`、`unlink`）、`minix3/minix/commands/truncate/`，合计 34 个命令
> **Rust 模块**: `os/commands/bin/fileops`（库包 `minix-fileops`：`mode.rs`、`testexpr.rs`、`path.rs`、`echo.rs`、`pathchk.rs`、`printf.rs`、`expr.rs`，74 个库测试通过）；`src/bin/` 薄壳九个已接线：echo、true、false、basename、dirname、pathchk、printf、expr（2026-09-17）；执行半经 `minix-sys` 顶层 `write` 与 exit（宿主端的传输失败语义待 edge E-SYSCALL-SIGN 修正后即为诚实退出码）；其余命令的薄壳随本域执行层批次在本 crate 内落地（文件读写待系统调用；原 `os/commands/bin/` 下占位壳已于 2026-09-17 删除，见 todo.md §6.1 步骤 3）
> **前置依赖**: `05-shell-family.md`（调用方：内建与外部的区分）、文件系统调用（`14-stage-runtime` 的文件部分）
> **不覆盖（移交）**: 文本处理（见 `07-text-filter.md`）、存储管理（见 `14` 到 `17` 各篇）、表达式求值之外的 `test` 内建包装（见 `05` 篇内建部分）

---

## 1. 概念：文件命令的四大家族

### 1.0 本章说明

本章按语义把 34 个命令分成四组：改属性、建关系、搬运查看、判断求值。每组共享一套核心语义，组内命令只是这套语义的不同入口。

> **本章不讲什么**：
>
> - 文本内容的逐行处理（见 `07-text-filter.md`）
> - 磁盘分区、挂载、检查（见 `14` 到 `17` 各篇）
> - 通配符展开（它属于 shell，见 `05` 篇边界）
>
> 本章只讲"对文件本身动手"的命令：权限、链接、复制、查看、判断。

### 1.1 改属性家族：权限是十二个二进制位

每个文件都带着十二个权限位：九个读写执行位（分属用户、组、其他三组）加三个特殊位（设用户标识、设组标识、粘滞）。`chmod` 就是改这十二个位的命令，两种写法：八进制（`755` 直接写出）与符号式（`u+x`、`go-w`、`a=rw`：谁、如何、是什么）。符号式的精髓在于相对修改——"给组加上写"而不必知道当前是什么，这在脚本里至关重要（绝对值写法要求先查出现状，相对写法一步到位）。

`chown` 改属主（用户与组），`touch` 改时间戳（不存在则建空文件——"摸一下"的本义），`truncate` 定长（把文件截到指定字节数，多退少补零）。四个命令分工正交：位、主、时、长各管一摊。

### 1.2 建关系家族：链接是给同一份数据起别名

Unix 文件的本质是"数据块加引用计数"：文件名只是指向数据的目录项。硬链接（`ln` 默认、`link`）是再建一个指向同一数据的目录项——两个名字、一份数据、引用计数加一，删一个另一个照常用。软链接（`ln -s`）是建一个存着路径的小文件——指向的是名字而非数据，原文件删了就悬空。`unlink` 一次删一个目录项（引用计数减一，到零才真正释放数据）。

记忆要点：硬链接不能跨文件系统（计数器是单文件系统内的）、不能链目录（防环路），软链接两者皆可但会悬空。`mkdir` 与 `rmdir` 是目录的建删（`rmdir` 拒绝非空目录——"删空篮子可以，倒掉整筐不行"，防误删的最后一道栏杆）。

### 1.3 搬运查看家族：复制、列目录、找文件、看空间

- **复制**（`cp` 548 行）：文件到文件、文件到目录、目录到目录（递归）三种形态，核心语义是"数据相同、属性按选项"（保留时间戳、保留权限、跟随或不跟随软链接，皆由选项控制）。`xinstall` 是"安装版复制"：复制加设属主权限加建目录一条龙，供软件安装脚本调用。
- **列目录**（`ls` 715 行）：名字、长格式（权限、链接数、属主、大小、时间）、排序、隐藏文件显隐——选项虽多，语义只有"选哪些文件、显示哪些列、按什么排序"三件事。
- **找文件**（`find` 306 行）：按名、按类型、按时间、按大小遍历目录树， trouv 到就执行动作（打印、删除、调外部命令）。它是"目录树的查询语言"，`xargs` 是它的搭档（把找出的文件名拼成命令行，避免参数过长）。
- **看空间**（`df` 看文件系统余量、`du` 看目录用量、`stat` 看单个文件全属性）：三者读的是同一套文件系统统计，只是粒度不同（系统级、目录级、文件级）。
- **定位**（`pwd` 打印当前目录、`basename` 取尾名、`dirname` 取目录部分、`pathchk` 校验路径合法性）：路径字符串的四种问法，全部是纯字符串操作——本库 `path.rs` 的由来。
- **同步与锁**（`sync` 刷缓存、`flock` 文件锁、`mkfifo` 建命名管道、`mktemp` 安全建临时文件）：并发与持久性的四个小工具，各自解决"多进程协作"的一个侧面。

### 1.4 判断求值家族：`test`、`expr`、`true`、`false`、`printf`、`echo`

脚本需要分支，分支需要条件，条件就是这组命令：

- `test`（`[`）是条件表达式求值器：文件测试（存在、可读、可写、可执行、是目录……）、字符串比较（相等、不等、空非空）、整数比较（等于、大于……）、逻辑组合（与、或、非、括号）。它的语法（`test.c` 四层递归：或 → 与 → 非 → 原子）是本库 `testexpr.rs` 的逐项对照。
- `expr` 是表达式求值器（算术、字符串匹配、长度），`true` 与 `false` 是恒真恒假（占位与循环条件用），`printf` 与 `echo` 是输出（`printf` 带格式、`echo` 简单直接）。
- 这组命令的共同点是"退出码即返回值"：条件真则退出码零，假则非零，`if` 语句读的就是这个码。这是 Unix"一切皆进程、进程靠退出码说话"哲学的缩影。

---

## 2. C 源码分析

### 2.1 `test.c`：四层递归求值（717 行）

求值器是教科书级的递归下降（第 160 到 163 行四个函数声明即文法）：或表达式调与表达式、与调非、非调原子、原子处理括号回绕到或。算子表（第 105 到 130 行三个表）把算子分成比较、文件状态、字符串三类。文件状态问询（`filstat`，第 166 行）与词法（`t_lex`，第 167 行）各守一端：前者问系统，后者切词，中间四层只讲逻辑。Rust 侧 `testexpr.rs` 的四层函数与之同名同序（`or_expr`、`and_expr`、`not_expr`、`primary`），是全阶段最工整的一次对照。

### 2.2 `chmod.c`：选项外壳加库调用

主函数（第 78 到 114 行：长短选项、递归、引用文件等）是典型的命令外壳，真正的模式语义在库里：第 166 行 `setmode` 把模式文本变成编辑脚本，第 216 行 `getmode` 把脚本应用到文件现有位上。Rust 侧 `mode.rs` 实现的正是这两步（解析加应用），文件遍历与系统调用留在执行层；`getmode` 认的命令里唯一需要「看文件本身是什么」的是 `X`（`setmode.c:148-150` 拿原始 `omode` 的目次位与三个执行位作条件，且只会或进去、不会清掉），因此 `apply_mode` 的入参必须是完整 `st_mode`，而不是剥掉类型位的十二个权限位。这种"外壳薄、库厚"的结构是 NetBSD 命令的通用风格，也是本阶段"决策下沉"策略的源码依据。

### 2.3 `ls`（715 行）、`cp`（548 行）、`find`（306 行）：三大量级命令的形态

- `ls` 的体量来自"列的组合"（十几种列、四种排序、三种显隐）而非算法深度——读它的源码应该按"选项分组"读，而不是按行读。
- `cp` 的体量来自"三种形态乘四种属性策略"（递归、跟随链接、保留属性、覆盖确认），核心复制循环反而短小。
- `find` 的体量来自"表达式树"（主表达式、连接词、动作），与 `test` 的表达式求值是表亲（一个求真值、一个遍历树执行）。

三者共同说明：文件命令的复杂性在"选项组合"，不在"算法"。这也是本篇文档采用"契约表"而非"逐行分析"的原因——组合语义用表述说最清楚。

### 2.4 小命令：每个都是一个纯函数

`basename`、`dirname`（字符串切分）、`true`、`false`（常量退出码）、`pwd`（取当前目录打印）、`sync`（调同步返回）、`mkdir`、`rmdir`、`link`、`unlink`、`mkfifo`（一次系统调用加错误处理）——每个小命令的核心都是一个纯函数加一层系统调用皮。本库先实现其中最纯的两个（路径切分），其余随系统调用落地逐个补齐。`chroot`（换根目录执行）是例外：它涉及进程根目录状态，归进程管理阶段，本篇只记其名。

### 2.5 命令契约总表（34 命令，行为摘要）

| 命令 | 职责 | 关键选项 | 输入输出 | 退出码 |
|------|------|---------|---------|--------|
| cat | 拼接文件到标准输出 | `-n` 行号 | 文件或标准输入 → 标准输出 | 0 成功，1 出错 |
| chmod | 改权限位 | `-R` 递归 | 模式加文件 | 0 成功，1 出错 |
| chown | 改属主 | `-R` 递归 | 属主加文件 | 0 成功，1 出错 |
| cp | 复制 | `-r` 递归、`-p` 保留、`-i` 确认 | 源 → 目标 | 0 成功，1 出错 |
| df | 文件系统余量 | `-h` 易读 | 无 → 表格 | 0 成功 |
| echo | 打印参数 | `-n` 去换行 | 参数 → 标准输出 | 0 |
| expr | 求表达式值 | — | 表达式 → 结果 | 0 非空非零，1 空或零，2 出错 |
| ln | 建链接 | `-s` 软链接 | 目标加链接名 | 0 成功，1 出错 |
| ls | 列目录 | `-l` 长格式、`-a` 全部、`-t` 按时 | 目录 → 列表 | 0 成功，1 出错 |
| mkdir | 建目录 | `-p` 建父目录 | 目录名 | 0 成功，1 出错 |
| mv | 移动改名 | `-i` 确认 | 源 → 目标 | 0 成功，1 出错 |
| pwd | 当前目录 | — | 无 → 路径 | 0 |
| rm | 删除 | `-r` 递归、`-f` 强制 | 文件 | 0 成功，1 出错 |
| rmdir | 删空目录 | — | 目录名 | 0 成功，1 出错 |
| sync | 刷缓存 | — | 无 | 0 |
| test/`[` | 条件求值 | 文件问 `-e -f -d -c -b -p -S -L -h -u -g -k -O -G -r -w -x -s -t`；字符串 `= != < > -n -z`；整数 `-eq -ne -gt -ge -lt -le`；组合 `! -a -o` 与括号，另有 `-nt -ot -ef`（C 的表里没有 `==` 与 `-N`，本树也不发明） | 表达式 | 0 真，1 假，2 出错 |
| basename | 取尾名 | 后缀参数 | 路径 | 0 |
| dirname | 取目录部分 | — | 路径 | 0 |
| du | 目录用量 | `-a` 逐个、`-d 深度`、`-s` 汇总、`-c` 总计、`-i` 计节点、`-k`/`-m`/`-g` 单位、`-n` 跳 `UF_NODUMP`（`-h`/`-H`/`-L`/`-x` 本树待接） | 目录 → 每行「数字 TAB 名字」 | 0 成功，任一处读不到为 1 |
| false | 恒假 | — | 无 | 1 |
| find | 遍历查找 | 表达式 | 目录 → 命中 | 0 成功，1 出错 |
| flock | 文件锁 | — | 文件加命令 | 0 成功 |
| mkfifo | 建命名管道 | `-m 模式`（八进制或符号） | 管道名一个以上 | 0 成功，任一处失败为 1 |
| mktemp | 安全临时文件 | 模板 | 无 → 路径 | 0 成功 |
| pathchk | 校验路径 | — | 路径 | 0 合法，1 非法 |
| printf | 格式化输出 | — | 格式加参数 | 0 |
| stat | 文件全属性 | `-L` 跟随、`-n` 不补末行换行、`-q` 不出声、`-l`/`-F`/`-r`/`-s`/`-x` 换预置模板、`-f 模板` 自定义、`-t 时间模板`（`%R` 走 `realpath.rs` 的折叠） | 文件 → 按模板渲染的一行 | 0 成功，任一路径取不到为 1 |
| touch | 改时间戳 | `-c` 不建 | 文件 | 0 成功 |
| true | 恒真 | — | 无 | 0 |
| xargs | 拼命令行 | `-0` 零分隔 | 名单 → 执行 | 0 成功 |
| xinstall | 安装复制 | `-o` 属主、`-m` 权限 | 源 → 目标 | 0 成功 |
| chroot | 换根执行 | — | 新根加命令 | 命令的码 |
| link/unlink | 建删目录项 | — | 路径 | 0 成功 |
| truncate | 定长文件 | `-c`（不建）、`-s [+|-]尺寸[K|k|M|m|G|g]`、`-r 参照文件`——后两者必选其一 | 文件一个以上 | 0 成功，任一处失败为 1 |

---

## 3. Rust 设计决策

### 3.1 为什么先实现"判断"与"切分"，不实现"读写"

34 个命令里，读写文件（`cat`、`cp`、`ls` 读目录）的都需要系统调用（本阶段没有），而判断（`test` 表达式、`chmod` 模式、`basename` 切分）是纯文字与数字。本库实现后者三组：`testexpr`（表达式求值）、`mode`（模式算术）、`path`（路径切分）。这不是挑软柿子——`test` 与 `chmod` 的"难"本来就在语义（优先级、符号解析），而不在系统调用；把难的部分先钉住，执行层落地时只剩薄薄一层。

参照系：Linux 的 coreutils 把同样的逻辑放在 C 库函数里（`getmode` 思想同源），Redox 的 coreutils 重写版（uutils 风格）同样先立纯函数再接系统。这条路是三方殊途同归。

### 3.2 为什么表达式求值器与 C 同名同序

`or_expr`、`and_expr`、`not_expr`、`primary` 四个函数名是刻意与 `test.c` 的 `oexpr`、`aexpr`、`nexpr`、`primary` 对应的（Rust 侧用全名，C 侧是缩写——命名风格差异，不是语义差异）。对照的好处是评审时可以逐函数 diff：C 的或层处理 `-o`，Rust 的或层处理 `-o`，行为差异无处藏身。这是"谨防翻译"的反面教材式用法：结构对照是为了可验证，而不是为了省思考——每个算子的语义仍逐条用测试钉住。

### 3.3 为什么文件测试走接口

`-f`、`-d`、`-r` 这些问句的答案在文件系统里，不在表达式里。`FileTester` 接口（存在、类型、权限、大小、时间）配 `EmptyFs`（全否，安全默认）与 `TableFs`（16 条内存表）两个实现，执行层的活实现以后直接插进来。值得展开的是 `-t`（描述符是否终端）：测试器没有终端概念，数字描述符恒报否——这是诚实的阶段能力声明，不是 bug（终端判定归终端阶段）。

### 3.4 为什么模式解析把"创建掩码"交出去

符号模式里"无主语"（`+x`）的含义是"按创建掩码允许的位加"——创建掩码是进程状态，库不知道。本库的 `who_mask` 对无主语返回全位，并在文档里写明"执行器负责与掩码取交"。把不知道的东西 honest 地交出去，比猜一个默认值正确得多（猜错掩码会导致权限放大，是安全问题）。

---

## 4. 实现详解

### 4.1 模块结构

`os/commands/bin/fileops`（库包名 `minix-fileops`）判断层源文件如下（全量以 `ls os/commands/bin/fileops/src/*.rs` 为准）：

| Rust 文件 | 对应 C 源码位置 | 职责 |
|-----------|----------------|------|
| `lib.rs` | — | 错误类型（`FileOpError`，22 对应参数无效、2 对应文件缺失）与模块组织 |
| `mode.rs` | `chmod.c:87-136` getopt 与裸模式字母回退（`:127-133`）、`:144-157` 遍历形态、`:186-210` 访问判定、`:215-219` 应用；`chmod.c:166`（解析）、`:216`（应用）思想；`lib/libc/string/strmode.c`（声明 `unistd.h:403`） | `ChmodFlags::parse`（`-f`/`-R` 接线，`-h`/`-H`/`-L`/`-P` 拒收，簇与 `--`、裸 `-[perm]` 归为模式）、`chmods_entry`（物理走表跳过符链）、`is_bare_mode`；八进制与符号模式解析应用（`apply_mode`，入参是文件自身的完整 `st_mode`——`X` 要查其中的目次位与既有执行位，`setmode.c:148-150`）；`strmode` 反向回渲——十一个字符（类型字母、九个权限字符、末尾访问控制列表填充位），三组 execute 槽兼作设身份位记号（`S`/`s`/`T`/`t` 四分支），常规文件支再查归档态两位（`a`/`A`） |
| `bin/chmod.rs` | `chmod.c:141-142` 操数、`:164-172` 模式取法、`:186-219` 逐访问应用 | `chmod [-f] [-v] mode file ...` 与 `-R` 递归形：逐操数按扫描形态取状态（`-R` 走 `lstat`，缺省走 `stat`）→`chmods_entry` 决定这一访问是否改动（物理走表里的符链一律跳过）→`apply_mode` 折位后 `chmod`→目次在预序改完即以 `dents` 下探子项（后序访问不再改，同 C 的 `FTS_DP` `continue`）、开不了目次报一次仍算已改（`FTS_DNR` 形）、取不到状态报且跳过（`FTS_NS` 形）；`-f` 只压消息不改退出码；`-h`/`-H`/`-L`/`-P` 与 `--reference` 前置拒 |
| `cook.rs` | `cat.c` 的 `cook_buf` | `cat` 行/字节变换：`-n`/`-b` 行号（`%6d\t`）、`-s` 连续空行压缩、`-e` 行尾 `$`、`-t` tab→`^I`、`-v` 非打印→`^X`/`M-x`（逐字状态机） |
| `mkdir.rs` | `mkdir.c` 的 `mkpath` 与 `-m` `getmode` 思想 | `-p` 逐级前缀遍历、中间/终端模式、额外位判、`-m` 复用 `apply_mode` |
| `rmdir.rs` | `rmdir.c` 的 `rm_path` | `-p` 由内向外逐级父目录遍历（复刻 `strrchr` 截断、首败即停、不删根） |
| `touch.rs` | `touch.c` 的 `stime_arg1`/`stime_arg2` | `[[CC]YY]MMDDhhmm[.SS]` 时间解析与 UTC 历法↔历元换算（`-t`/obsolete 定位形） |
| `truncate.rs` | `minix/commands/truncate/truncate.c:57-82` getopt、`:89-100` 尺寸来源、`:150-198` `parselength`、`:117-132` 相对尺寸与钳位 | `parse_length`（`[+-]?[0-9]+[GMK]`，乘数支落穿：`K` 一个 1024、`M` 两个、`G` 三个；「乘数后不得再有字符」那一判写在 `K` 支里，因而同时管住 `G`/`M`）、`parse_args`（`-c`/`-r`/`-s`，`-s` 与 `-r` 互斥且必选其一、无操数即 usage、两个 `-s` 后者胜）、`open_flags`/`CREATE_MODE`（`-c` 丢 `O_CREAT`，建文权限 0644）、`relative_target`/`clamp`（相对尺寸加到已有长度后向零钳位；C 的 `EFBIG` 判据拿 `oflow` 与它自己的来源式相比，永不为真） |
| `bin/truncate.rs` | `truncate.c:91-100` 参照文件、`:108-143` 逐操数 | `truncate [-c] -s [+|-]size[K|k|M|m|G|g] file ...` 与 `truncate [-c] -r rfile file ...` 全形：参照文件先 `stat` 一次定长，逐操数 `open`（`O_WRONLY`，缺 `-c` 时并 `O_CREAT`，建文权限 0644）→相对尺寸时 `fstat` 取现有长度→`ftruncate`→`close`；`open` 失败只在非 `ENOENT` 时报错计错（故 `truncate -c 缺失文件` 静默退 0），`-s` 值非法时按 C 打印带反引号的原文并立即退出，参照文件 `stat` 失败只印文件名；C 在 `fstat`/`ftruncate` 失败处漏 `close`，本实现每条出口都关 |
| `mkfifo.rs` | `usr.bin/mkfifo/mkfifo.c:69` 默认位、`:71-91` getopt 与 `-m`、`:81` `getmode` 基 | `ASSUMED_BASE`（`-m` 的假定初态是 `a=rw` 即 0666，不是已被掩码削过的那份）、`default_mode`（0666 减创建掩码）、`mode_from_spec`（八进制与符号两形都走 `chmod` 那台 `setmode`/`getmode` 引擎）、`parse_args`（`getopt "m:"`：`-m` 带值、后随的第一个非旗标即管道名、无名为 usage、两个 `-m` 后者重算） |
| `bin/mkfifo.rs` | `mkfifo.c:93-99` 逐名创建 | `mkfifo [-m mode] fifoname ...`：`umask(0)` 读掩码后还原，逐名 `mkfifo(name, mode)`，失败印 `mkfifo: <名>` 并计错继续；`-m` 文本被 `setmode` 拒时按 C 印`Cannot set file mode '<文本>'` 并即刻退出 |
| `du.rs` | `usr.bin/du/du.c:88-192` 选项与单位、`:197-249` 逐访问记账、`:258-278` `prstat`、`:280-351` `linkchk`；`lib/libc/gen/getbsize.c:66-108`（`BLOCKSIZE`） | `parse_args`（`-a`/`-d`/`-s` 三者互斥、`-d` 走 `atoi` 且限 `SHRT_MAX` 内、`-P` 与 `-r` 是 C 自己的空操作、`-h`/`-H`/`-L`/`-x` 拒收）、`blocksize_from_env`（`getbsize` 的两重钳位与三段告警文案）、`count`（`-i` 计节点否则计 `st_blocks`）、`printed_value`/`howmany`（单位换算向上取整）、`prints_directory`/`prints_file`（两处打印判据）、`skipped_as_nodump`（`UF_NODUMP`）、`LinkSet`（硬链接去重：`(dev, ino)` 见过与否，`(0,0)` 对按 C 的 `sawzero` 存在表外） |
| `bin/du.rs` | `du.c:197-255` 主循环与总计 | `du [-P] [-a | -d depth | -s] [-cijkmnr] [file ...]`：`lstat` 取自身、目次以 `dents` 下探并把自己 `COUNT` 与子项之和在唯一一次打印中输出（对应 C 的 `FTS_DP` 支，故无目录被打两遍）、`-c` 把各操数返回值累加成 `total` 行；多链接文件第二次遇见既不打印也不计入（C 的 `break`），单链接文件不查表；开不了目次按 `FTS_DNR` 报一次而目次自身照计；`BLOCKSIZE` 的告警在任何总数之前先出 |
| `humanize.rs` | `lib/libc/gen/humanize_number.c:59-160`；旗标位 `include/stdlib.h:292-298` | `humanize_number` 的移植：按缓冲区宽度自动定档（放大 100 倍预留两位、与 `max - 50` 比，使 9.9 可进到 10 而不再降一档）、单位表由档号索引且 `HN_B` 才在零档放 `B`、放不下时按 `snprintf` 的语义截断而非报错（十进制分支除外，它先查 `.N` 的余量再回 -1，此时缓冲已被清成空串）；`HN_GETSCALE` 未移植并显式拒绝 |
| `find.rs` | `usr.bin/find/{main.c,option.c,find.c,operator.c,function.c}` | `find` 决定半第一包：全局选项按 `main.c:88-124` 的**位操作**照搬（`-h` 不清 `COMFOLLOW` 而 `-L` 清，故两者不能折成一个三值枚举）；主表 51 项按 `option.c:59-110` 的字典序原样落一遍（C 用 `bsearch` 查它），每项带「要不要参数」与「算不算有输出」两位；路径与表达式的分界按 `main.c:138-143` 的判据；表达式一次成形为树，优先级与 C 的三段压实等价（`()` 最紧、`!` 只收一个主项、相邻是隐式与、`-o` 最松且右结合）；`-exec` 一族吃到单独的 `;` 或紧跟 `{}` 的 `+`。取值与遍历在下一包 |
| `stat.rs` | `usr.bin/stat/stat.c`（预置模板宏 94-115、字段码宏 158-178、`main` 的选项与模板选择 200-318、`output()` 的模板扫描 380-556、`format1()` 的取值与渲染 568-1151） | `stat` 的决定半。五条预置模板原样抄成常量；`%` 模板扫成件表（字面件 / `%n %t %% %@` 四个简写 / 字段件），字段件带参数位、宽度、精度、进制字母、子片段字母与字段码，并留着子格式串原文供两处坏格式回显；渲染侧按字段取值后交回数值/字符串/浮点三形（`strmode` 的三片段切分、大小端设备号按 `compat_defs.h:1254`、`vis` 的 `CSTYLE|OCTAL|WHITE` 子集、`F` 形把纳秒排在秒后并按精度补零），时间走 `dates::format_at`；选项面按人格分（同一份码既当 `stat` 也当 `readlink`，后者只认 `fnqsv`），模板开关共用一个坑所以第二个必撞。`%R` 自批次一百三十五起走 `realpath.rs` 的折叠，前缀规则与 `%Y` 同一条 |
| `ln.rs` | `ln.c` `main`/`getopt` | `LnFlags::parse`（getopt "fhinsv"）、`plan_operands`（argc 形态）、`basename`（目录形接名） |
| `bin/ln.rs` | `ln.c` `linkit` | 硬/符链创建：`lstat`/`stat`/`unlink`/`link`/`symlink`、目次 append basename、`-f`/`-i`/`-v` |
| `rm.rs` | `rm.c` `main` getopt、`check`、`checkdot`、`NONEXISTENT` | `RmFlags::parse`（getopt "dfiPRrvWx"，`-f`/`-i` 互清、`-r`/`-R` 同置递归）、`nonexistent`（ENOENT/ENAMETOOLONG/ENOTDIR 三值判，`-f` 静默依据）、`check_flavor`（`rm.c:551-560` 的询问三出口：`-i`→`Remove`；终端且非符链且 `access(W_OK)` 因非 ETXTBSY 的原因拒→`Override`；否则不问）、`is_dot`+`strip_dots`（POSIX `.`/`..` 叶名拒收、尾斜杠剥除不空） |
| `bin/rm.rs` | `rm.c` `rm_file`/`rm_tree`/`check` | 非递归默认形：逐操作符 `lstat`→目次无 `-d` 报 "is a directory"→`check` 按 `check_flavor` 出问（`remove '<路径>'? ` 或 `override <权限> <uid>:<gid> for '<路径>'? `，两者均为 C 的裸 `fprintf`，无程名前缀）→`rmdir`/`unlink`、`-f` 抑制不存在、`-v` 回显；`-r`/`-R` 已接：`rm_tree` 以 `dents` 走目录（预序先问可否跳过整个子树→子项先删→后序 `rmdir`、符链不追、`d_ino==0` 的已删槽位跳过），`-P/-W/-x` 仍设施残差整组前置拒 |
| `mv.rs` | `mv.c:88-159` getopt、operand 形；`mv.c:172-203` 询问梯；`mv.c:236`/`:251`/`:272` 跨盘决策 | `MvFlags::parse`（getopt "ifv"，`-f`/`-i` 互清）、`plan_operands`（argc<2→Usage/2→Pair/N→ToDirectory）、`dest_name`（尾斜杠剥除后取末组件、根→空）、`ask_about`（无旗标下目标不可写且标准输入为终端→`Override`；`-i`→`Overwrite`；两条询问支先查源可达否则 `SourceUnreachable`；`-f` 或目标不存在→`None`）、`cross_device_plan`（常规文件→就地拷，其余→需子进程）、`existing_removal`（既存目标按类型选 `rmdir`/`unlink`）、`move_create_mode`（交完整 `st_mode`，与 `cp::create_mode` 剥设身份位有意不同） |
| `bin/mv.rs` | `mv.c:153-253` `do_move`、`mv.c:256-354` `fastcopy` | 同盘 `rename` 形＋跨盘常规文件形；询问梯逐段对位 `mv.c:172-203`（`access(to, F_OK)` 定门、`access(to, W_OK)` 与 `stat(to)` 驱动 `override` 形、两条询问支先做 `access(from, F_OK)`）；提示词按 C 的裸 `fprintf` 不加程名前缀也不加引号；`override` 行印九个权限字符与用户/组号（本树无口令库→取 C `pwcache.c:296-306` 的数字回退形）；`-v` 打 `src -> tgt`；`EXDEV` 走 `fastcopy`（开目标交整模式→复用 `cp::pump`→失败删半品→`futimens`/改主/定位→关描述符→`unlink` 源）；源非常规文件那一支 C 靠 `execl` `/bin/cp -PRp` 加 `/bin/rm -rf`（`mv.c:357-399`），exec 递送链未落地→按实际报 not wired |
| `cp.rs` | `cp.c:117-167` getopt、`:209-297` 目标分类与路径拼接、`:443-537` 类型分派、`utils.c:109`/`:142-143` 目的地符链判定、`:145-146`/`:280-288`/`:373` 位算法、`:244-255` 搬运环 | `CpFlags::parse`（getopt "HLNPRfailprv"，`-f`/`-i` 互清、`-i` 依 `isatty(stdin)` 才生效、`-R` 置递归）、`classify`（目标为目次→`FileToDir`；`-R` 下目次源配缺失目标→`DirToDne`；否则单源→`FileToFile`，多源先报错、尾斜杠只在 `FileToFile` 上报错）、`join_target`+`appended_component`（末组件拼接、`..` 记作 `.`、`DirToDne` 落在目标名本身）、`create_mode`/`retained_mode`/`preserved_mode`/`post_create_dir_mode`（四种位掩码分档）、`entry_action`（八分支类型分派）、`writes_through`（`-R` 不写穿既存符链目的地）、`is_dir`/`is_reg`/`is_link`、`is_identical`/`overwrites_dir`/`is_unwanted_directory`、`pump`（经 `CopyIo` 接口的读块写块环） |
| `bin/cp.rs` | `cp.c:324-548` `copy`（含 `:455-516` 目次的两趟访问与 `:303-321` 的 dne 栈）、`utils.c:93-296` `copy_file`、`:304-352` 三条特殊文件腿、`:364-404` `setfile`、`:64-76` `set_utimes` | 非递归形＋`-R` 递归形：逐源按扫描形态取状态（`-R` 走 `lstat`、否则 `stat`）→同设备同 inode 报「同一文件」并跳过整棵子树、目次覆盖与非目次互斥检查→按 `entry_action` 分派：常规文件开源开窗搬运（现存目标截断、`-R` 遇符链先删再按 `create_mode` 建、`-f` 失败先删再建）→目次预序 `mkdir`（`st_mode | S_IRWXU`）后以 `dents` 走表下探、后序按 `-p` 复权限时刻再 `chmod`→符链 `readlink`+`symlink`、fifo `mkfifo`、设备节点 `mknod`（三条特殊文件腿只在 `-R` 下重建，非递归时按 `cp.c:517-533` 的 `else` 支仍走文件复制——对 fifo 就是 `utils.c:102` 那个会阻塞的 `open`）→`-p` 复权限与两个时刻、set-id 源按组回灌→`-v` 打 `src -> dst`（目次两趟各打一次，同 C 的 `FTS_D`/`FTS_DP` 两次访问）；`-r/-a/-l/-H/-L/-P/-N` 整组前置拒（逻辑扫描、符链遍历模式、硬链接复制未落地） |
| `dents.rs` | `sys/sys/dirent.h` 记录布局、`lib/libc/gen/readdir.c:75-81` 游标推进 | `getdents` 记录解码的共用件：`next_record`（按 `d_reclen` 前移，长度自相矛盾即终止而不越界读）、`keep_in_listing`（`-a`/`-A` 的点条目规则）、`is_dot`+`is_descendable`（递归不下探 `.`/`..`）、`join_path`（C `ls` 的 `dotdir` 规则：`.` 出裸名）、`file_type` 常量组；`bin/ls.rs` 的走表已改读本件 |
| `linkops.rs` | `usr.sbin/link/link.c`、`usr.sbin/unlink/unlink.c` | `link_operands`（恰二操数）、`unlink_operand`（恰一操数）——POSIX argc 形状校验（无 getopt） |
| `bin/link.rs`、`bin/unlink.rs` | 各 `main` | 一次 `link(2)`/`unlink(2)` + 错处理（perror 无 strerror 后缀→仅报路径）、退码 0/1 |
| `realpath.rs` | `lib/libc/gen/getcwd.c:64-215`（`realpath` 与 `getcwd` 同处一件）、上限 `sys/sys/param.h:358` 的 `MAXSYMLINKS` | `realpath(3)` 的移植：逐段折 `.`、`..` 与符号链。相对路径先接 cwd；绝对目标让已积累的前缀整体作废；C 在链这一支**不推进** `p`，于是下一段覆写掉链名自己（`/d/l` 且 `l -> e` 的答案是 `/d/e`），本层用「回退到段首长度」复刻同一效果。三个动作收进 `PathProbe`，故折叠算法在宿主上逐分支可测 |
| `testexpr.rs` | `test.c:105-154`（三张算子表）、`test.c:160-163`（四层声明）、`test.c:341-348`（字符串比较）、`test.c:495-578`（`test_access`/`filstat`） | 四层求值（`evaluate`）与 `FileTester` 接口。算子覆盖与 C 的表逐条对齐：`<` 与 `>` 字符串比较与 `-h`（`-L` 的向后兼容别名）本批补上，`-N` 与 `==` 因 C 的表里没有而删去（宁可少而不发明） |
| `path.rs` | `basename`、`dirname` 语义 | 路径切分（`basename`、`dirname`） |

### 4.2 关键类型与不变量

- **权限位 `u16`**：低十二位是权限（九位读写执行加三位特殊），高四位是文件类型（八进制替换时保留）。不变量：符号操作只碰主语掩码内的位；`=` 先清后设；空 `what` 只允许配 `=`（清零语义）。
- **表达式求值器**：四层优先级（或低于与低于非）。不变量：空表达式为假；剩余单词即错（不猜测）；括号必须配对；整数溢出即错（不回绕）。
- **文件测试器 `FileTester`**：存在、类型、权限、大小、时间五问。不变量：未知路径行为等同空文件系统；新旧比较缺一边即假（不报错）。
- **路径切分**：纯字符串。不变量：尾斜杠先剥（根除外）；空路径即错；后缀相等整名时保留（`basename b.c b.c` 得 `b.c`）。

### 4.3 函数一览

| 函数 | 输入 | 输出 | 对应 C 行为 |
|------|------|------|------------|
| `apply_mode(现有位, 模式文本)` | 位与文本 | 新位 | `setmode` 加 `getmode` 语义 |
| `CatCook::push(字节, 发出器)` | 一个输入字节与回调 | cooked 字节序列 | `cook_buf` 的行号与压缩语义 |
| `for_each_parent(路径, 访问器)` | 路径与回调 | 逐级前缀 | `mkpath` 的斜杠扫描 |
| `for_each_ancestor(路径, 访问器)` | 路径与回调 | 逐级父目（由内向外） | `rm_path` 的 `strrchr` 截断 |
| `parse_time_spec(历元, 规格)` | 参考历元与时间串 | 目标历元秒 | `stime_arg1` 的长度分层覆盖与 69 年枢纽 |
| `evaluate(测试器, 单词表)` | 接口对象与参数 | 真假 | `test.c` 四层语义 |
| `basename(路径, 后缀)` | 路径与后缀 | 尾名 | `basename` 语义 |
| `dirname(路径)` | 路径 | 目录部分 | `dirname` 语义 |

---

### 4.5 命令契约与 Requires

行为判定沿用 `99-global-concepts.md` §2 的规则：以 POSIX 为准绳、以 Minix3 C 实现为真值，C 偏离处在"当前状态"列标注。依赖面按 §1 分层契约只含 `minix-sys` 顶层与 `minix-rt`；同一 Requires 组内命令共用一行：

| 命令 | Requires（最小 API） | 当前状态 |
|------|---------------------|---------|
| echo | `write`、`exit`、argv 交接 | 已接线（`src/bin/echo.rs`） |
| true、false | `exit` | 已接线（纯退出码，忽略操作数） |
| basename、dirname | `write`、`exit`、argv 交接 | 已接线（决定半在 `path.rs`） |
| cat | `open`（现有路径）、`read`、`write`、`close` | API 已备（`open_existing_via` 实装，`minix-sys/src/vfs.rs:696`）；拼接与行变换选项已接（`-n`/`-b`/`-s` 走 `cook_buf` 决定半 `cook.rs`，逐操作符新建状态使行号逐文件重启）；`-e`/`-t`/`-v`（字节可视化）已接（`-e`/`-t` 蕴含 `-v`）；仅 `-B`/`-l`/`-f`（不改输出字节）报 usage；真实执行阻塞 = guest 二进制形态（freestanding bin 尚未交叉编译入镜像）+ exec 递送链（NS3 门 bug 已修，NS5 VFS exec worker 待实装） |
| cp、ln、mv、rm、mkdir、rmdir、chmod、chown、touch、mkfifo、truncate、link、unlink | 上一行全部，加各自的方向调用（建链/截断/权限位面） | mkdir、rmdir 已接线（`src/bin/{mkdir,rmdir}.rs`）；ln、rm、mv 已接线（见下行）；link、unlink 已接线（`src/bin/{link,unlink}.rs`，决定半 `linkops.rs` 的 argc 形状校验，整命令就一次 `link(2)`/`unlink(2)`、无设施残差）；cp 已接线非递归形与 `-R` 递归形（`src/bin/cp.rs`，决定半 `cp.rs`）——`-f`/`-i`/`-p`/`-v` 与目标分类/路径拼接/四种权限掩码分档全部对位 `cp.c`+`utils.c`，`-R` 的走表以 `dents` 替 fts（与 `rm -r` 同法，预序建目次、后序定权限），余 `-r/-a/-l/-H/-L/-P/-N`（逻辑扫描、符链遍历模式、硬链接复制）整组前置拒；chmod 已接线非递归形与 `-R` 递归形（`src/bin/chmod.rs`，决定半 `mode.rs` 的 `ChmodFlags::parse`＋`chmods_entry`），走表同以 `dents` 替 fts，符链在物理走表里一律跳过，故 `-R` 不需要 `lchmod`；`X` 按 `setmode.c` 的条件语义接正（裸 `-X` 亦归模式）；余 `-h`/`-H`/`-L`/`-P` 与 `--reference` 前置拒；truncate 已接线全选项形（`src/bin/truncate.rs`，决定半 `truncate.rs`：`parselength` 的落穿乘数与溢出检查、`-s`/`-r` 互斥必选其一、`-c` 的 ENOENT 静默），本命令无设施残差；mkfifo 已接线全形（`src/bin/mkfifo.rs`，决定半 `mkfifo.rs`：默认 `0666 & ~umask`、`-m` 走 `getmode` 的 `a=rw` 假定基），chown 余行待登记封装（用户名与组名换成号需口令库） |
| df、du、find、stat、`test` 的文件问 | `stat`、`getdents` | 两桩封装在位并被本域命令使用（`ls`、`rm -r`、`cp -R`、`chmod -R`、`du` 均在读它们；`../14-stage-runtime/todo.md:60` 那条登记的现状归 14 线核对）；`du` 已接线（`src/bin/du.rs`，决定半 `du.rs`，含硬链接去重、`BLOCKSIZE` 单位换算与走 `humanize.rs` 的 `-h` 渲染）并已播种上机：x86_64 串口回 `24<TAB>/etc` 与 `12K<TAB>/etc` 两行（`lstat`/`st_blocks` 与两种渲染腿皆通），另有 `du: /etc` 一行是下探的 `getdents` 腿失败如实报错（同 `ls: /bin: read error`，imgrd 根盘目录读腿属 `E5`）；判据在 `os/qemu-tests/test-cmd-smoke*.sh`，三条腿已由 `.github/workflows/cmd-smoke.yml` 按矩阵在持续集成里跑；`df` 已接线（`os/commands/sbin/mountinfo/src/df.rs` 与 `src/bin/df.rs`）；`stat` 已接线（决定半 `stat.rs` 含模板、选项与渲染三件，薄壳 `src/bin/stat.rs`）并已播种上机：`/bin/stat /etc/rc`、`/bin/stat -l /etc/rc`、`/bin/stat -s /etc/rc` 三行走的是同一条 `lstat` IPC 腿，x86_64 串口的整形读数是 `262 3 -rwxr-xr-x 1 (0) (0) 0 3334 "Jan  1 00:00:00 1970" "Oct 11 22:40:10 2026" "Oct 11 22:40:10 2026" "Jan  1 00:00:00 1970" 4096 8 0x0 /etc/rc`，三条门脚本各按三种形状判据（不断言具体数字，因盘上属主与时间戳是镜像属性）)，其中 `%R` 自批次一百三十五起走本树的 `realpath(3)` 移植（读 `lstat`/`readlink`/`getcwd` 三条腿）、`%Su`/`%Sg` 走 C 的「查无口令项」支打 `(123)`、`%d`/`%r` 字符串形打 `???`；`test` 的文件问**早已接线**（决定半 `testexpr.rs` 的 `FileTester` + 薄壳 `src/bin/test.rs`（同一份码以 `test` 与方括号名两个名字编译） 走 `stat`/`lstat`），「待批」是账面滞后：批次一百三十四按 C 的三张算子表逐条对账，补上缺的 `<` 与 `>` 字符串比较与 `-h` 别名，并删去 C 的表里根本没有的 `-N` 与 `==`；`test`/方括号形态尚未播进 imgrd（成功时不输出，门判据需另行设计），这一条是**播种**的留白而非执行层的留白。`find` 的第一包已落（全局选项位、51 项主表、表达式语法树），取值与遍历在下一包 |
| stat | `lstat`、`stat`（`-L`）、`fstat`（无操作数时问标准输入）、`readlink`（`%Y`）、`getcwd` 加逐段 `lstat`（`%R` 走 `realpath.rs`） | 已接线（`src/bin/stat.rs`，决定半 `stat.rs`：模板与选项在 `parse_args`/`parse_format`，取值与渲染在 `render`）；`%R`（realpath）与 `devname`/口令库三处缺腿按 C 的失败支或显式拒绝处理，见上表；已播种并由三条门的 stat 断言收（x86_64 串口整形读数见 §7）|
| touch | `gettimeofday`、`stat`、`open`（`O_WRONLY|O_CREAT`）、`fstat`、`close`、`utimens` | 已接线（`src/bin/touch.rs`，决定半 `touch.rs`：`parse_time_spec` 与 UTC 历法换算）；默认设当前时间/缺则建文（无时区参与，100% 忠实）；`-t`/obsolete 定长解析按 UTC 口径（本树无时区库，与 date 批同裁）；`-d`/`-r`/`-h`/长选报 not wired |
| pathchk | `write`、`exit`、argv 交接 | 已接线 `-p` 路（决定半 `pathchk.rs`：`_POSIX_NAME_MAX`/`_POSIX_PATH_MAX` 常量界限、可移植字符集、前导连字符；诊断走标准错误）；缺省模式的 `pathconf`/`stat` 探测待批，现显式拒绝 |
| printf | `write`、`exit`、argv 交接 | 已接线（决定半 `printf.rs`：运行时格式引擎——标志/宽度/精度与 `d i o u x X s c %b %%`、格式复用、`\c` 截停、base 0 数字解析与字符常量前缀；浮点转换 `e E f g G` 按裁决单独处理，现显式拒绝） |
| expr | `write`、`exit`、argv 交接；`:` 匹配加 `minix-regex`（BRE 引擎，对齐 C 的 expr 链接 libc regex） | 已接线（决定半 `expr.rs`：优先级梯 `| & 比较 + - * / % : `、`length` 关键字、C 的溢出/除零/非整数检查——含全正大数乘法环绕漏报的已声明怪癖、前导 `--` 怪癖；错误面退出 2） |
| mktemp | `open` 的建路径（已实现）加 `O_EXCL` 语义确认 | 待批 |
| xargs | `fork`、`exec`（调用方备栈形态）、`waitpid` | 待 `exec` 调用方协议批 |
| pwd、sync、flock、chroot、xinstall | 各自调用面（getcwd/update/flock/chroot/安装语义） | 待 14 侧登记 |

---

## 5. 测试要点

`cargo test -p minix-fileops`：全量以 `cargo test -p minix-fileops --lib` 实际输出为准（本篇直接相关的测包含在内，历史全量数字详见对应 `.review` 底账）。已接线的命令薄壳（echo、true、false、basename、dirname、pathchk、printf、expr、chmod、pwd、sync、test、date、df、mkdir、rmdir、ln、rm、mv、link、unlink、cp、truncate、mkfifo、du 等）其决定半由库测覆盖。
- **表达式求值**（`expr.rs`，14 个）：`test_arithmetic_and_precedence`（优先级与括号）、`test_or_and_returns`（`|`/`&` 的空零判定）、`test_comparisons_numeric_and_string`（数值与字节序两路）、`test_match_returns_length_or_group`（锚定匹配、组一提取、无组 "0"/有组空串两种未中形态）、`test_length_keyword`（含 C 的结合行为：length 后的加法在外层）、`test_empty_operand_is_integer_zero`（空串即整数零）、`test_divide_by_zero_is_an_error`、`test_overflow_is_an_error`（加减溢出检查、乘法的已声明怪癖：全正大数环绕漏报）、`test_non_integer_arithmetic_operand_rejected`、`test_out_of_range_operand_rejected`（含 i64::MIN 边界与正数上界两路）、`test_leading_dash_operand_needs_no_quirk`、`test_leading_double_dash_quirk`（首参 `--` 吞与不吞两路）、`test_trailing_operator_is_syntax_error`、`test_exit_relevance_zero_result_shape`。
- **printf 引擎**（`printf.rs`，23 个）：`test_literal_text_passes_through`、`test_percent_percent_is_one_percent`、`test_decimal_and_string_basics`、`test_missing_operands_are_zero_and_empty`（干列表补零与空串）、`test_format_reuses_over_extra_operands`（格式复用）、`test_flags_plus_space_and_minus`、`test_zero_flag_zero_fills_against_width`（0 旗标与精度互斥）、`test_precision_on_integers`（含精度零渲染零值为空）、`test_unsigned_octal_hex_upper`、`test_negative_unsigned_rejected`（`getuintmax` 拒负）、`test_alternate_form_prefixes`（`#` 的 0/0x/0X 前缀）、`test_string_width_and_precision`、`test_char_takes_first_byte`、`test_star_width_comes_from_operands`（`*` 取操作数）、`test_escapes_expand_in_the_format`（八进制至多三位、十六进制两位）、`test_c_escape_halts_output`（`\c` 截停）、`test_unknown_escape_warns_but_passes_through`、`test_numeric_operands_use_base_zero`（base 0 解析）、`test_character_constant_prefix_operand`（`"A` 字符常量）、`test_bad_number_warns_status_one`、`test_b_expands_operand_escapes`（`%b` 操作数转义）、`test_float_conversions_are_declared_unsupported`、`test_missing_format_character_is_an_error`。

重点行为与测试的对应（以下函数名均可用 `rg "fn 测试名" os/commands/bin/fileops` 复现）：

- **模式算术**（`mode.rs`，20 个）：`test_octal_replaces_low_bits`（八进制替换且保留文件类型位）、`test_bad_octal_rejected`（非八进制数字、超长、空串）、`test_add_and_remove`（加减）、`test_set_exactly`（精确设置与清空）、`test_special_bits`（设标识位与粘滞位）、`test_copy_between_classes`（类间复制 `g=u`）、`test_chained_clauses`（逗号多子句）、`test_bad_clause_rejected`（非法主语、缺算子、缺内容）、`strmode_type_letters`（九种类型字母加归档态 `a`/`A` 只在常规文件支里查、无可识别类型位回 `?`）、`strmode_permission_characters`（全开/`644`/全关/每位单开四形）、`strmode_set_id_slots_replace_the_execute_letter`（设身份与执行的四种组合各占一格；`0o7777` 才出 `rwsrwsrwt`，`0o1777` 只带粘滞位故前两组仍为 `x`）、`strmode_pads_with_a_space_after_the_nine_permissions`（第十一位是访问控制列表填充位，而 `mv.c:188` 判的索引 9 总是权限字符——那一判永假，分隔空格因此总输出）、`chmod_flags_default_to_all_off`/`chmod_clusters_and_double_dash`（`-R`/`-f` 接线、簇、`--`、未知字母报 usage）、`chmod_link_flags_are_refused`（`-h`/`-H`/`-L`/`-P` 逐字母拒收）、`bare_permission_letter_is_a_mode_not_a_flag`（`chmod.c:127-133` 的 `--optind` 回退：`-r` 是模式不是旗标）、`a_physical_walk_passes_over_links`（`chmod.c:201-210`：物理走表跳过符链，非递归扫描才跟进）、`x_letter_only_adds_when_the_original_is_a_directory_or_already_executes`（`setmode.c:148-150` 的四路条件：不带执行位的常规文件不加、目次加满三执行位、已有一格执行位补其余两组、`g+X` 仍受主语掩码约束）、`x_never_clears_even_in_a_minus_clause`（`setmode.c:330-336` 的落发形状：`-X` 只条件添加，`-xX` 清掉又加回，条件读的是原始模式）、`equals_then_x_keeps_the_conditional_add`（`a=X` 先清空再条件加，`setmode.c:373-384` 把 `=` 拆成 clear+set）。
- **建目录**（`mkdir.rs`，6 个）：`test_absolute_path_prefixes`（`/a/b/c` 拆为三前缀、仅末个为终端）、`test_relative_path_prefixes`（相对路无前导斜杆）、`test_leading_and_trailing_slashes`（纯斜杆根路无可建、尾斜杆仍终端于末个真组件）、`test_intermediate_mode_forces_user_wx`（中间级强制 `u+wx`）、`test_extra_bit_fixup`（粘滞/设身份位需后续 `chmod`）、`test_parse_mode_spec_octal_and_symbolic`（八进制保额外位、符号仅动主语掩码内位——本轮纠了一处推错的断言 `go-rwx,u+x` 应为 `0700` 非 `0500`）。
- **删目录**（`rmdir.rs`，5 个）：`test_relative_walks_innermost_first`（`a/b/c` 由内向外先 `a/b` 后 `a`）、`test_absolute_stops_before_root`（`/x/y` 只到 `/x`、截至空即根而不删）、`test_single_component_has_no_ancestors`（单组件无父）、`test_trailing_slash_reoffers_leaf_like_c`（忠实 C `rm_path` 不预剥尾斜杠：`a/b/` 首截回叶 `a/b`）、`test_reverse_of_parent_prefixes`（与 `mkdir::for_each_parent` 去叶逆序的交叉对账）。
- **改时间**（`touch.rs`，8 个）：`test_days_from_civil_anchors`（独立锚点 1970=0 天、`2000-01-01`=10957 天——本轮一度把后者误记 11016、runner 当场报错，已纠）、`test_civil_round_trip`（历日⇄历元往返含负值与闰界）、`test_full_ccyy_specification`（12 位全覆盖 `2025-01-02 15:30:45`=1735831845）、`test_two_digit_year_pivot`（99→1999、70→1970、69→1969 枢纽）、`test_omitted_fields_carry_current_time`（省略字段沿用当前、仅分钟）、`test_seconds_default_zero_unless_explicit`（无 `.SS` 秒归 0 而非沿用当前）、`test_malformed_rejected`（奇长/非数字/`.SS` 非两位）、`test_obsolete_time_spec_shape`（8/10 位纯数字定位形）。
- **建链**（`ln.rs`，7 个）：`flags_parse_known_letters`/`combined_cluster_flags`（getopt "fhinsv" 单体与簇）、`f_and_i_last_wins_both_ways`（-f/-i 互清双向）、`unknown_option_is_error`（`-z`→None）、`double_dash_ends_options`、`operand_shapes`（argc 0/1/2/N 形态）、`basename_strips_dir`。
- **删除**（`rm.rs`，16 个）：`flags_parse_known_letters`/`combined_cluster_flags`（getopt "dfiPRrvWx"）、`f_and_i_last_wins_both_ways`（`-f`/`-i` 互清双向，对位 `rm.c` 的 `case 'f'`/`case 'i'`）、`r_lower_and_upper_both_set_recursive`（`-r`/`-R` 同置）、`unwired_flags_flagged`（`-P/-W/-x` → `needs_unwired`，而 `-r`/`-R` 自走表抽成共用件后已不再计入）、`unknown_option_is_error`、`double_dash_ends_options`、`first_operand_ends_option_scan`（无置换：首个非选项后皆操数）、`nonexistent_triage`（ENOENT/ENAMETOOLONG/ENOTDIR 真、EPERM/EINVAL 假）、`check_flavor_minus_i_wins_without_consulting_anything`（`rm.c:551` 的“先查 -i”：终端/类型/访问位看也不看）、`check_flavor_needs_terminal_writable_and_type`（四个守卫各缺即不问：非终端、符链（“权限无意义”）、可写、ETXTBSY 拒）、`is_dot_exact`（`.`/`..` 真、`...`/空假）、`strip_dots_rejects_dot_and_dotdot`、`strip_dots_basenames_through_dirs`（`./.`、`dir/..`、`..//` 拒）、`strip_dots_strips_trailing_slash_but_keeps_root`（`a/`→`a`、`/` 不空）、`strip_dots_keeps_plain_operands_in_order`。
- **截长**（`truncate.rs`，13 个）：`plain_digits_are_bytes`、`multipliers_fall_through_as_c_counts_them`（`truncate.c:176-190` 的落穿：`1G` 是吉字节而非 1024）、`a_sign_makes_the_length_negative`（含 `-+5` 被拒）、`anything_after_a_multiplier_is_refused`（`truncate.c:187` 的尾字符判同时管住 K/M/G，空串按 C 出 0）、`overflow_fails_instead_of_wrapping`（`ASSIGN_CHK_OFLOW` 的两处守卫）、`exactly_one_size_source_with_a_file`（`truncate.c:89` 的 `do_refer + got_size != 1` 与缺操数）、`the_last_size_letter_wins_as_c_flags_it`、`a_signed_size_is_relative`（文本首字符的符号决定相对形，`truncate.c:72`）、`c_flag_and_clusters_and_double_dash`、`a_bad_size_is_reported_with_the_text_that_failed`（带反引号的原文，且先于任何文件动作退出）、`an_unknown_letter_and_a_missing_option_argument_are_usage`（含 glued `-s10` 的拒收形状）、`open_flags_and_create_mode_follow_the_c_flag`（0644 里没有任何执行位）、`relative_sizes_land_on_the_clamped_sum`（`truncate.c:131-132` 的向零钳位，以及 C 永不到达的 `EFBIG` 一支）。
- **建命名管道**（`mkfifo.rs`，9 个）：`default_mode_subtracts_the_creation_mask`（`mkfifo.c:69` 的 `0666 & ~umask(0)`，四档掩码）、`the_default_applies_when_no_m_flag_is_given`、`m_flag_octal_replaces_the_assumed_base_wholesale`（`-m 777` 连 0777 的掩码都碰不到，因为 `:81` 交的是字面 0666）、`m_flag_symbolic_is_relative_to_a_rw`（`a+x`→777、`u=r`→466、`g-w`→646、两个 `-m` 后者重算）、`x_adds_nothing_to_an_a_rw_base`（`X` 的条件读的是交进去的 0666，既非目次也无执行位）、`several_names_keep_their_order_and_one_mode`、`scanning_stops_at_the_first_name_and_dash_dash_ends_it`（BSD getopt 不重排）、`usage_for_unknown_letters_missing_values_and_no_name`（含 glued `-m600`）、`a_bad_mode_is_reported_with_the_text_that_failed`。
- **目录用量**（`du.rs`，17 个）：`defaults_count_blocks_to_a_depth_of_everything`、`no_operand_means_the_current_directory`（`du.c:184-187` 的 `.` 兜底）、`units_are_divided_by_512`（`-k`→2、`-m`→2048、`-g`→2097152，`du.c:192`）、`listing_modes_are_mutually_exclusive`（`-a` 遇 `-s`/`-d`、`-s` 遇 `-d` 皆 usage，`du.c:173-182`）、`depth_comes_from_s_or_d`、`atoi_shape_and_the_short_window_for_d`（`-d abc` 按 `du.c:111` 的 `atoi` 读作 0，`32768` 越界、`-1` 为负皆 usage）、`unwired_letters_are_named_and_the_noop_letters_are_taken`（`-P`/`-r` 与原样调用等价）、`clusters_and_operand_names_survive`（BSD getopt 不重排）、`blocksize_environment_is_read_the_way_getbsize_reads_it` 与 `environment_values_below_or_above_the_window_are_clamped_with_warnings`（`8`→512 一条告警、`-1` 直取最小、`10KB` 走三段告警、`9999999999G` 封顶到 1G）、`counts_and_printed_values_follow_the_unit`、`printing_depths_mirror_the_two_arms`（`-s` 只印操数自身、无 `-a` 时深层文件并入目次）、`nodump_flag_skips_only_under_n`、`a_link_is_counted_once_however_many_names_it_has`、`the_zero_pair_is_tracked_outside_the_table`、`the_table_grows_and_keeps_every_answer_it_already_gave`（5000 项跨多次倍增）。
- **单位渲染**（`humanize.rs`，10 个）：`du_shape_renders_the_single_letter_units`（`du -h` 那一形：12288 字节出 `12K`、1024 出 `1.0K`）、`the_two_digit_headroom_is_what_decides_the_scale`（`1 MiB - 1 KiB` 也进位成 `1.0M`，因为循环比的是 `max - 50`）、`a_fixed_scale_divides_exactly_that_many_times`、`hn_b_is_what_names_the_byte_and_the_default_separator_is_a_space`（无 `HN_B` 时零档只剩数字，分隔空格却仍在，尾部空格是有意的）、`si_units_divide_by_a_thousand_and_spell_the_kilo_lower_case`、`a_suffix_is_part_of_the_length_budget`、`a_wide_rendering_is_clipped_and_a_decimal_one_is_refused`（两种失败形状：截断与 -1）、`negative_values_keep_the_sign_and_share_the_same_budget`（符号多占一格，小负数因此出空串）、`a_scale_beyond_the_table_and_an_empty_buffer_are_refused`、`the_unported_getscale_mode_is_refused_not_mis_shaped`。
- **路径折叠**（`realpath.rs`，12 个）：`an_absolute_path_of_plain_directories_comes_back_cleaned`（重复斜杠与尾斜杠都收掉）、`dot_and_dotdot_are_folded_without_asking_the_file_system`、`a_missing_component_is_the_stat_failure`、`a_relative_path_starts_from_the_working_directory`、`an_absolute_symlink_restarts_the_accumulated_prefix`、`a_relative_symlink_lands_inside_its_own_directory`（链这一支覆写掉链名自己）、`a_symlink_chain_is_followed_to_the_end`、`too_many_symlink_hops_is_a_loop`、`a_non_directory_in_the_middle_is_enotdir`、`a_path_beyond_the_buffer_is_enametooolong`、`a_broken_link_target_reports_through_the_readlink_leg`、`the_root_alone_resolves_to_itself`。
- **模板与选项**（`stat.rs`，21 个）：`plain_bytes_pass_through`、`the_four_simple_forms_are_one_character_each`（`%n %t %% %@`）、`a_bare_datum_needs_no_flags_or_width`（裸字段码就是合法写法，参数位与宽度都有默认）、`flags_are_or_ed_in_bit_order_from_the_c_macros`（五个参数位字母按 C 的位值累加）、`width_then_precision_then_form_then_subfield`（`%04OLp` 一形把四段都吃满）、`a_dot_without_digits_still_sets_precision_zero`（孤零零一个 `.` 精度就是 0）、`every_c_datum_letter_parses`（21 个字母逐一对账）、`unknown_letters_are_bad_format_and_the_span_includes_the_offender`、`a_trailing_percent_is_bad_format_without_eating_a_real_char`、`a_hilo_needs_a_datum_after_it`、`oversized_width_falls_into_the_same_bad_format`（宽度与精度两串数字都判溢出）、`mixed_literal_and_specs_keep_their_order`、`the_default_plan_is_def_format_and_default_timefmt`、`each_template_switch_picks_its_predefined_format`（`-F` 单用也折成 `-l` 那条形）、`shell_and_linux_forms_carry_their_own_time_default`（`-s` 带 `%s`、`-x` 带 `%c`）、`two_template_switches_conflict`、`the_ls_f_flag_only_pairs_with_the_l_flag`、`dash_f_shares_the_template_slot_so_the_second_switch_conflicts`（`-f` 自己就占坑，缺值走 usage，`-fVALUE` 与 `-nqf%N` 的贴簇写法都认）、`clustered_switches_and_operand_boundary`、`unknown_switch_is_usage`、`the_readlink_personality_flips_three_things_at_once`（默认 `%Y`、默认不出声、`-f` 不取值而意为 `%R`、`-v` 才取消出声，stat 的模板开关在它那里全是未知开关）。

- **渲染形态**（`stat.rs`，23 个）：`the_default_template_lays_out_every_field_in_order`（默认模板整行逐字）、`the_raw_template_leaves_the_quotes_off_and_uses_numbers`（`-r` 形里 `%#p` 打 `0100644`、`%f` 不打十六进制）、`the_ls_like_templates_mark_type_and_follow_the_link`（`-l`/`-F` 两条形同 `%N%T%SY`）、`the_shell_and_linux_templates_print_assignments_and_blocks`、`mode_string_form_drops_the_padding_and_the_octal_default_keeps_the_type_bits`（字符串形丢掉 `strmode` 的末尾填充位）、`the_three_subfield_letters_slice_the_permission_groups`（中位是 C 自己的一处不一致：数值折特殊三位、字符串切组权限，照抄）、`hex_with_pound_and_zero_padding_put_the_zeros_after_the_prefix`（宽度算整个字段）、`sign_flags_only_ride_the_decimal_form`（无符号形不吃 `+`）、`precision_pads_the_digits_while_width_pads_the_field`（数值精度补零、字符串精度截断）、`size_subfields_divide_by_powers_of_two_and_round`（先移一位加一再移一位的四舍五入）、`times_default_to_seconds_and_the_string_form_uses_the_time_template`、`the_float_form_lays_the_nanoseconds_behind_the_decimal_point`（`%.0Fm` 只打秒、超九位补零）、`the_link_arrow_appears_only_for_the_explicit_string_form`（裸 `%Y` 不带 ` -> `）、`the_readlink_personality_reports_a_missing_target_through_the_report`、`filename_subfields_split_the_directory_and_the_basename`（尾斜杠先剥、无斜杠时高位片段打 `.`）、`filetype_names_itself_in_the_high_form_and_marks_it_in_the_low_form`、`device_datums_split_major_and_minor_and_sizerdev_folds_to_the_pair`、`the_devname_branch_prints_the_marks_c_prints_when_it_finds_nothing`、`ids_print_parenthesised_without_a_password_database_behind_them`、`the_pound_flag_on_a_string_vis_encodes_control_and_space`、`unsupported_subfield_or_form_is_the_same_bad_format_c_reports`、`the_realpath_letter_is_refused_instead_of_rendering_empty`、`literals_and_simple_forms_reach_the_output_in_order`。薄壳 `src/bin/stat.rs` 另有 2 枚：`to_data_maps_every_status_field_by_name` 与`the_link_test_uses_the_same_mask_and_value_as_the_deciding_half`。- **改名/移动**（`mv.rs`，13 个）：`flags_parse_known_letters`/`f_and_i_last_wins_both_ways`（getopt "ifv"、`-f`/`-i` 互清）、`combined_cluster_and_double_dash`（簇 `-iv`、`--` 终止）、`unknown_option_is_error`、`operand_shapes`（argc<2→Usage/2→Pair/N→ToDirectory）、`dest_name_strips_trailing_slash_and_dir`（`dir/name`→`name`、`dir/name/`→`name`、`/`→空、`a//b//`→`b`）、`cross_device_split_is_on_the_source_type`（常规文件→就地拷；目次与符链→需子进程——`mv.c:246` 是 `lstat`，所以符链不走就地那一支）、`destination_removal_follows_its_type`（目次目标走 `rmdir`、常规文件与符链走 `unlink`，对位 `mv.c:236` 的三元表达式）、`move_create_mode_keeps_what_cp_create_mode_strips`（两台引擎在同一处的有意差异被反向钉住：`mv.c:272` 交整模式，`utils.c:146` 剥设身份位）、`no_force_or_missing_target_means_no_question`（`mv.c:172` 的 `!fflg && !access(to, F_OK)` 门：`-f` 与目标不存在都不问）、`interactive_asks_overwrite_and_checks_the_source_first`（`-i` 先查源可达报 `rename <源>`；且 `mv.c:92` 不问终端——与 `cp` 相反）、`override_needs_terminal_unwritable_and_statable`（三个条件各缺即不问：非终端、目标可写、目标不可 `stat`）、`interactive_branch_precedes_the_override_branch`（两条件同时成立时 `mv.c:176` 先判 `iflg`，问的是 overwrite 形）。
- **目录项走表**（`dents.rs`，6 个）：`record_decodes_layout_and_next_offset`（手工拼记录钉五处偏移与游标步进）、`short_tail_and_empty_buffer_end_the_walk`（不足头长是正常终点非错误）、`contradictory_lengths_stop_the_walk_instead_of_running_off`（`d_reclen` 为 0、超出缓冲、`d_namlen` 超出所属记录三例）、`dot_entries_follow_the_listing_flags`（`-a`/`-A` 与默认三态）、`descends_only_into_real_names`、`path_joining_matches_c_ls_dotdir_rule`。
- **链建删**（`linkops.rs`，3 个）：`link_needs_exactly_two`（恰二才 Some、0/1/3 → None）、`unlink_needs_exactly_one`（恰一）、`link_operands_preserve_paths_verbatim`（原样交 `link(2)`、无 basename/目次加接）。
- **复制**（`cp.rs`，32 个）：`defaults_are_all_off`/`f_and_i_clear_each_other_last_wins`/`clusters_and_double_dash`（getopt "HLNPRfailprv"、`-f`/`-i` 互清、簇与 `--`）、`interactive_only_when_stdin_is_a_terminal`（`cp.c:148` 的 `iflag = isatty(fileno(stdin))`：管道输入下 `-i` 不生效）、`unwired_letters_are_reported_not_ignored`（`-r/-a/-l/-H/-L/-P/-N` 逐字母拒收）+`recursive_letter_is_wired`（`-R` 已接线：置递归不清他位、`-Rv` 成簇）、`unknown_letter_is_usage_error`、`directory_target_takes_every_source_count`/`non_directory_target_takes_exactly_one_source`（目标分类与 argc 约束；现存目次目标优先于三形）、`recursive_directory_source_into_a_missing_target_is_case_three`（`cp.c:268-269`：`-R`＋目次源＋缺失目标才成三形，尾斜杠在三形下不报错、`join_target` 落在目标名本身）、`source_count_is_checked_before_the_slash`（`cp.c:249` 的 usage 先于 `:275` 的尾斜杠报错）、`trailing_slash_on_a_file_target_is_refused`（现存目标→"is not a directory"、缺失目标→"directory does not exist" 两路）、`trailing_slashes_strip_but_root_survives`（`a//`→`a`、`/` 不空）、`dotdot_appends_a_dot`（`..` 与 `a/..` 拼接时记作 `.`）、`joining_inserts_exactly_one_separator`（`dir`+`a/b.txt`→`dir/b.txt`、`/`+`x`→`/x`、`FileToFile` 不拼）、`create_mode_drops_set_id_bits_only`（粘滞位保留）、`retained_mode_keeps_sticky_and_respects_umask`、`preserved_mode_drops_sticky`（`utils.c:373` 与 `:280` 两掩码的差别正落在粘滞位）、`retain_branch_needs_all_four_conditions`（非 `-p`、目标新建、源带设身份位、源主为己）、`a_recursive_scan_never_writes_through_a_link`（`utils.c:109`＋`:142-143`：`-R` 遇既存符链目的地不写穿）、`post_order_dir_stamp_follows_the_preserve_mask`（`cp.c:502-506`：无 `-p` 交原始模式含粘滞位，有 `-p` 交 `setfile` 已掩过的模式）、`entry_dispatch_covers_every_type_bit`（八类 `S_IF*` 分派、权限位不影响类型掩码）、`identical_needs_both_device_and_inode`、`directory_target_may_not_be_overwritten_by_a_file`、`unwanted_directory_matches_the_mode_bits`、`pump_moves_every_block_and_counts_them`/`pump_stops_immediately_on_an_empty_input`/`pump_uses_the_caller_scratch_and_never_overreads`（逐块搬运与计数、末块不补零）、`short_write_is_a_write_failure`/`failed_write_is_reported_as_write`/`failed_read_is_reported_as_read`（错腿归属：短写与写失败算目标、读失败算源）、`zero_sized_source_never_enters_the_loop`（`utils.c:183` 的零长源不读）。
- **拼接行变换**（`cook.rs`，11 个）：`test_plain_passthrough_without_flags`（无旗标逐字透传）、`test_number_every_line`（`-n` 每行 `%6d\t`）、`test_number_nonblank_skips_blank_lines`（`-b` 空行不编号但仍出换行）、`test_squeeze_collapses_consecutive_blanks`（`-s` 连续空行压成一个、单空行保留）、`test_squeeze_then_number_line_numbers_survive_squeeze`（`-n`+`-s` 组合下行号跳被压行不占号）、`test_no_trailing_newline_last_line_still_numbered`（无尾换行末行仍编号）、`test_line_number_field_widens_past_six_digits`（行号超 6 位不加截断）、`test_dollar_marks_line_end_and_implies_visible`（`-e` 行尾补 `$`）、`test_show_tabs_renders_caret_upper_i`（`-t` tab 作 `^I`）、`test_visible_control_and_high_bytes`（`-v`：0x1b→`^[`、0x7f→`^?`、0x80→`M-^@`、0xe1→`M-a`）、`test_be_blank_line_keeps_blank_number_field`（`-b -e` 空行占六空格字段再制表符、行尾 `$`）。
- **表达式求值**（`testexpr.rs`，13 个）：`test_file_status_operators`（六问真假）、`test_empty_fs_answers_no`（空系统全否）、`test_string_operators`（相等、不等、空非空、裸串，外加 `<` 与 `>` 的字节序与「`==` 不在表里故落语法错」）、`test_integer_operators`（六比较加非数字报错）、`test_boolean_precedence`（与高于或、非、括号）、`test_newer_older`（新旧比较与缺席为假）、`test_malformed_rejected`（括号不配对、缺操作数、多余单词）、`test_testers_share_the_trait`（接口统一）、`test_owner_group_and_modified_operators`（`-O`/`-G`，外加 C 表里没有的 `-N` 与 `==` 都落退出码 2 那一支）、`test_h_is_the_alias_of_capital_l`（`-h` 与 `-L` 读同一判据）、`test_block_and_char_are_distinct`、`test_same_file_ef`（同设备同节点）、`test_access_bits_matches_c_test_access`（`test_access` 的位规则含 root 与附属组）。
- **路径切分**（`path.rs`，4 个）：`test_basename_common`（四种形状）、`test_basename_suffix`（后缀三规则）、`test_dirname_common`（六种形状）、`test_empty_path_rejected`（空路径）。
- **echo 输出布局**（`echo.rs`，10 个）：`test_no_args_prints_bare_newline`（无参数只换行）、`test_single_arg_then_newline`、`test_arguments_separated_by_single_spaces`（单空格分隔、无尾随空格）、`test_leading_nflag_prints_nothing_extra`（`echo -n` 零输出）、`test_leading_nflag_suppresses_only_the_newline`、`test_second_nflag_is_data_not_flag`（getopt 禁令：仅首位 `-n` 是标志，后随的 `-n` 是数据）、`test_double_dash_is_ordinary_data`（echo 无 `--` 约定）、`test_empty_argument_keeps_both_spaces`、`test_write_failure_stops_emission_and_reports`（写失败即停并上报）、`test_write_failure_on_trailing_newline_reports`。

尚未覆盖、随后续阶段补齐的：文件读写命令的执行层（`cat`、`ls` 占位二进制，待文件系统调用；`rm` 非递归默认形与 `-r`/`-R` 递归形已接线（后者含 tty 的 override 询问，批次一百零八、一百一十二），其 `-P` overwrite 待 `arc4random`/`O_NOFOLLOW`、`-W` whiteout 待 `undelete`、`-x` 需挂载点判定；`cp` 非递归形与 `-R` 递归形已接线（决定半 + 薄壳 + 搬运环；走表以 `dents` 替 fts，符链、fifo、设备节点三条特殊文件腿随 `-R` 一起接上），余 `-r`（逻辑扫描）、`-a`/`-H`/`-L`/`-P`（符链遍历模式，其中 `-P` 还要 `lchown`/`lchmod`）、`-l`（硬链接代替复制）、`-N`（断链保持为链）待逐项接；`-p` 的 fchown/fchmod 形态因本树无 `f*` 封装改走路名形式（目标为常规文件时同果），复制符链时的 `-p` 因此只复时刻、不复主与权限（路名形式的 `chown`/`chmod` 会穿过新复制出的链落到其目标上）；`chmod` 非递归形与 `-R` 递归形已接线（决定半 `mode.rs` 加薄壳 `bin/chmod.rs`，走表以 `dents` 替 fts），余 `-h`/`-H`/`-L`/`-P`（改符链本身与遍历模式，需 `lchmod`）与 `--reference` 前置拒；`X` 面已按 `setmode.c` 接正（裸 `-X` 归为模式而非旗标、`X` 走条件添加而非等同 `x`），仅剩一处窄差异登记在 `apply_clause`：`-`/`=` 子句里 `X` 与拷贝字母 `u`/`g`/`o` 并现时 C 丢弃 `X` 位（`setmode.c:316-318`），本树按子句末统一落发；`mv` 跨盘形已接常规文件那一支（`fastcopy`，搬运环复用 `cp::pump`），剩两支：源为非常规文件时 C 靠 `execl` `/bin/cp -PRp` 加 `/bin/rm -rf`（需 exec 递送链，`NS5`/`C-21`），以及 `do_move` 询问面的两处未对位分支（既存目标不可写且标准输入为终端时的 override 询问、`-i` 分支先验源存在的 `access(from, F_OK)` 前置）——后者已由批次一百零六接上，同时新落了共用的 `mode::strmode`，`rm` 的同名残差（终端上对不可写常规文件的 override 询问）已由批次一百零八接上（`rm::check_flavor` 加 `bin/rm.rs` 的出问腿，并把 rm 与 mv 共用的答案读取收拢到 `bin_support::answer_yes`）；询问面文案残差三处（`mv`、`rm`、`cp`）已全部按 C 的裸 `fprintf` 对齐（批次一百零七、一百零八、一百一十），三命令共用同一 `bin_support::answer_yes`）、目录遍历命令（`find`、`du` 的执行层——走表已在 `dents`，还差各自的谓词与计量）、`chroot`（待进程根目录状态）。`echo` 已在本轮补上执行半（`src/bin/echo.rs`）：决定半在库内全测，执行半经 `minix-sys` 顶层 `write` 与 exit 收尾，宿主端行为待 edge E-SYSCALL-SIGN 修正传输失败符号后即为诚实的 exit 1。判断与切分层是全覆盖的，其余执行层是显式留白的。

---

## 6. 过渡：搬得动文件之后，去读懂文字

本篇走完了日常操作的第一组：文件的属性、关系、搬运、判断。三十四个常用动作里，不碰文件的八件（echo、true、false、basename、dirname、pathchk -p、printf、expr）已经端到端可用，其余的决策核心已经就位，只剩读写执行层待系统调用。

但文件里面装的是文字，文字有文字的玩法：取头取尾、排序去重、计数切割、字符转换——这是 `07-text-filter.md` 的职责。请沿因果链继续向下走：先会"搬文件"，再会"读文件里的行"。

---

## 7. 参见

- `05-shell-family.md`——调用方（内建与外部的区分）
- `07-text-filter.md`——文本过滤（下一步：行内操作）
- `minix3/bin/test/test.c:t_op（L105，工具生成）`——算子表与四层声明（求值器的逐行对照）
- `minix3/bin/chmod/chmod.c:main（L166，工具生成）`、`216`——模式解析与应用
- `minix3/bin/ls/ls.c`、`minix3/bin/cp/cp.c`、`minix3/usr.bin/find/find.c`——三大量级命令（715、548、306 行）

---

## 附：验证记录（评审用，可跳过）

- `wc -l minix3/bin/ls/ls.c minix3/bin/cp/cp.c minix3/usr.bin/find/find.c minix3/bin/test/test.c` → 715、548、306、717 行，与正文引用一致。
- 34 命令目录存在性：`bin` 十五、`usr.bin` 十五、`usr.sbin` 三（`chroot`、`link`、`unlink`）、`minix/commands/truncate`，逐个 `[ -d ]` 验证通过，无缺失。
- `cargo test -p minix-fileops` → 20 通过、0 失败；`cargo clippy` 无警告。
- 本文档引用的 `file:line` 均来自正文写作前实际执行的 `rg -n` 与 `sed -n` 输出，非凭记忆书写。
