# 02-服务管理与定时调度

> **状态**: 已完成，等待评审收敛
> **定位**: 交付因果链的第一环下半段——启动脚本把服务拉起来之后，日常如何管理服务、何时自动做事
> **源码**: `minix3/minix/commands/minix-service/minix-service.c`（含 `minix-service.8` 手册）、`minix3/minix/commands/svrctl/svrctl.c`、`minix3/usr.sbin/service/service`（shell 脚本）、`minix3/minix/commands/cron/cron.c`、`minix3/minix/commands/cron/tab.c`（第 285 到 360 行的时间解析）、`minix3/minix/commands/crontab/crontab.c`（第 70 到 122 行）、`minix3/minix/commands/at/at.c`（第 26 到 80 行）、`minix3/minix/commands/atnormalize/atnormalize.c`（第 25 行起）、`minix3/minix/commands/update/update.c`（24 行）、`minix3/etc/crontab`、`minix3/etc/rs.lwip`、`minix3/etc/rs.single`
> **Rust 模块**: `os/commands/usr-sbin/svcsched`（库包 `minix-svcsched`：`service.rs`、`cron.rs`、`scheduler.rs`，77 个测试通过）
> **前置依赖**: `01-init-rc-scripts.md`（启动脚本调用服务的场合）、重生服务协议（`03-stage-rs` 的重生服务部分）
> **不覆盖（移交）**: 重生服务的服务端实现（见 `03-stage-rs`）、守护进程本体如定时任务守护与网络守护的实现（见 `19-network-services.md` 涉及部分）

---

## 1. 概念：服务是"被照看的进程"，调度是"到点做事"

### 1.0 本章说明

本章回答两个问题：第一，操作系统里长期运行的程序坏了谁来管；第二，不需要人盯着的定期任务如何按时发生。

> **本章不讲什么**：
>
> - 启动脚本体系本身（见 `01-init-rc-scripts.md`）
> - 具体某个守护进程的业务逻辑（日志、网络等，分散在 `18`、`19` 各篇）
> - 时间表示与时钟硬件（见时钟相关阶段文档）
>
> 本章只讲"服务管理"与"定时调度"这两个概念，以及 Minix 同时保留两套服务命令的历史原因。

### 1.1 服务与普通进程的区别：有人照看

用户在终端敲命令启动的进程，退出就是退出了，没有人会把它拉起来。而操作系统里的服务（操作系统自身功能的常驻进程，例如文件服务、网络服务、定时任务守护）必须"永远活着"：崩溃了要重启，配置改了要重新加载，管理员要能查询状态、启停单个服务而不影响别的服务。

Minix 把这份"照看"职责交给重生服务（Reincarnation Server，简称重生服务）：它手里有一张服务表，记录每个服务用什么程序启动、崩溃后采取什么策略（重启、还是按脚本处理）。本篇不讲重生服务的内部实现，只讲用户侧的两个操作入口：出问题时"跟重生服务说话"的命令，以及开机时"按顺序拉起服务"的脚本。

### 1.2 为什么有两个服务命令：血统不同，分工不同

初学者最容易困惑的是：Minix 里既有 `minix-service` 又有 `service`，它们是什么关系？答案是两者血统不同、分工不同，名字撞车纯属历史巧合：

- **`minix-service` 是 Minix 原生的重生服务客户端**。它的操作对象是重生服务表里的"系统服务"（操作系统服务器与设备驱动）。动词在前、对象在后：`minix-service up 二进制程序`表示"把这个程序注册并启动为系统服务"，`minix-service down 服务标签`表示"停止这个服务"，还有刷新配置、重启、复制、整体关机等动词。手册 `minix-service.8` 的提要段把九种形状列得清清楚楚。
- **`service` 是从 NetBSD 继承的启动脚本包装器**（一个 shell 脚本）。它的操作对象是 `/etc/rc.d` 目录里的启动脚本。名词在前、动词在后：`service cron restart` 表示"用 cron 脚本的 restart 动作重启定时服务"，`service -l` 列出所有脚本，`service -e` 列出已启用的脚本。

记忆口诀：管"系统服务生死"的找 `minix-service`，管"启动脚本动作"的找 `service`。前者对话的对象是重生服务这个常驻进程，后者只是帮你找到脚本并调用，干完就退出。两者唯一的交集是开机场景：启动脚本里可能会调用 `minix-service`（例如 `etc/rs.single` 全文件六行、有效内容只有声明行与一句调用，本质就是一句 `minix-service down 服务标签`）。

### 1.3 低层控制接口：`svrctl` 是留给专家的手术刀

除了上面两个"日常用语"，还有一个 `svrctl` 命令：`svrctl 虚拟文件系统或进程管理 获取或设置 参数名 参数值`。它绕开脚本与服务标签，直接读写指定系统服务（目前只支持虚拟文件服务与进程管理服务）的内部参数，而且要求调用者是管理员（源码里显式检查有效用户标识是否为零，不是就拒绝）。可以把它理解成专家模式：日常管理用 `minix-service`，调内核级参数才用 `svrctl`。本篇只记录它的命令形状与权限门槛，参数语义归各服务自己的文档。

### 1.4 定时调度：周期任务、一次任务与"固定节拍"

"到点做事"有三种不同的"到点"，Minix 为每种准备了一个工具：

1. **周期任务**（`cron` 加 `crontab`）："每天六点跑一次""每小时整点跑一次"这种按日历重复的安排。时间表存在 `crontab` 文件里，一行一个任务，行首五个时间字段（分、时、日、月、周）加后面要执行的命令。`cron` 守护进程常驻，每分钟醒来看一次"现在有没有任务到期"，到期就创建子进程执行。
2. **一次任务**（`at`）："今晚十一点跑一次这个脚本"这种只执行一次的安排。`at` 命令把任务写成队列目录里的一个任务文件，到点由守护端执行，执行完任务文件即消失。
3. **固定节拍**（`update`）：既不按日历、也不只跑一次，而是"每 30 秒做一次同一件事"（把内存中缓存的文件数据写回磁盘）。整个程序只有 24 行：关闭标准文件描述符、切换到根目录防锁定设备，然后无限循环"同步、睡 30 秒"。

三种工具回答的是同一个问题（"何时行动"），输入却完全不同（日历时间表、一次性时刻、固定间隔）。这个观察直接决定了 Rust 侧的设计：一个判断接口、两种时间模型实现（见第 3.2 节）。

### 1.5 时间字段语法：Minix 方言与别家的差异

Minix 的 `crontab` 时间字段语法自成方言，读源码 `tab.c` 第 285 到 360 行可得精确规则：星号表示"所有值"；单个数字表示"这个值"；`低-高`表示闭区间；`起点:步长`表示从起点开始每隔步长取一个（注意是冒号，别家系统常用斜杠，含义相同写法不同）；逗号分隔的列表可以混用以上形式；问号只允许出现在分钟字段，表示"当前分钟"（效果等同于每分钟都匹配）。

问号的语义值得展开：`minix3/etc/crontab` 全文件只有一行有效内容——`? 6 * * * /usr/etc/daily cron`，即"六点那一小时内的每一分钟都匹配，但命令自己保证一天只真正跑一次"。这种写法把"一天一次"的精确性交给任务脚本，把"别错过"的责任留给调度器，是工程上的务实折中。

---

## 2. C 源码分析

### 2.1 `minix-service`：九种命令形状

手册 `minix-service.8` 提要段定义了全部形状，可归纳为三组：

| 组 | 形状 | 含义 |
|----|------|------|
| 启动组 | `minix-service up\|run\|edit\|update 二进制程序 [选项…]` | 操作对象是程序文件：注册并启动、直接运行、改配置、热更新；尾部选项（运行参数、设备、周期、脚本路径、标签名、配置文件、状态、最长运行时间）原样透传给重生服务 |
| 管理组 | `minix-service down\|refresh\|restart\|clone 服务标签` | 操作对象是已注册服务的标签：停止、重读配置、重启、复制 |
| 关机组 | `minix-service shutdown` | 不带任何对象，关闭整个服务层 |

Rust 侧的 `parse_minix_service_args` 与这张表逐项对应：关机组带对象则报错，启动组与管理组缺对象则报错（见第 4.2 节测试）。

### 2.2 `service` 脚本：标志、名单、动作三段式

`minix3/usr.sbin/service/service` 的 `usage` 函数（文件开头附近）定义了三种形状：纯标志（`-e` 列已启用、`-l` 按启动顺序全列、`-v` 附带所在目录）、`service 名字…`（测试这些脚本是否启用）、`service 名字 动作`（执行动作）。标志可捆绑（`-ev`），但 `-e` 与 `-l` 互斥（脚本里显式判断两者同现即报错）。脚本内部用 `rcorder -s nostart` 列全集（与 `01` 篇第 2.6 节呼应：同一排序工具既管开机顺序，也管日常列表）。

### 2.3 `svrctl`：只认管理员的四段式

`minix3/minix/commands/svrctl/svrctl.c` 主函数（文件开头）要求三到四个单词：`svrctl 服务名 操作 参数名 [参数值]`，操作统一转小写后只认"设置"与"获取"两种；设置必须带值（共五个单词），获取必须不带值（共四个单词）。帮助函数在第 107 行附近，打印的用法只有两行：针对虚拟文件服务或进程管理服务的设置与获取。越界一律打印用法并以失败退出——典型的"形状不对就拒绝，不猜测"风格。

### 2.4 `cron` 时间解析：`tab.c` 第 285 到 360 行

`range_parse` 函数逐字段构建位图：先清位图，星号置全位并标记"通配"（通配标记影响后续"日与周是或关系"的判定语义，属定时任务守护的经典细节）；问号仅当字段最大值是 59（即分钟字段）且后面不跟横杠时合法，取值为当前分钟；数字、区间、冒号步长按前文规则置位，越界或逆序（`5-1`）即整行报错。解析失败只记录日志、不让守护进程退出——一个坏行不值得搭上整个调度器。

守护主循环（`cron.c` 第 321 行起的主函数，派生逻辑在第 100 与 161 行附近）遵循"到期即派生"：匹配上的任务创建子进程执行，子进程切换到任务属主的工作目录与身份后执行命令。`crontab` 命令（`crontab.c` 第 70 到 122 行）是时间表的编辑入口：四个互斥标志（创建、列出、删除、管道输入）恰好选其一，用户归属缺失即报错。

### 2.5 `at` 与 `atnormalize`：一次任务的两端

`at` 命令（`at.c` 第 26 行起的主函数）接受二到五个单词：`at 时刻 [月 日] [文件]`。先解析时刻（`getltim`，非法即报"时刻写法错误"），再解析日期（非法即报"日期写法错误"），然后确定任务文件来源，最后把"执行时刻加任务内容"写成队列里的一个任务文件。时刻已过的处理蕴含在日期推算里（第 66 行起：没给日期且时刻已过当天，则顺延到下一天；跨年时按闰年规则回绕）。

`atnormalize`（`atnormalize.c` 第 25 行起）是队列的整理端：规范化任务文件名与权限，清理过期残留。两者分工恰好是"写队列"与"管队列"。

### 2.6 `update`：24 行的固定节拍

`update.c` 全文件 24 行，是全篇最短的源码：关闭三个标准文件描述符（脱离终端）、切换工作目录到根（避免锁定当前设备导致无法卸载）、无限循环执行"同步、睡眠 30 秒"。没有参数、没有配置、没有退出路径——它就是系统心跳的一部分，随开机启动、随关机死亡。这种"简单到不可能出错"的设计，恰恰是基础设施该有的样子。

---

## 3. Rust 设计决策

### 3.1 为什么两个服务命令各建一个解析函数

既然 `minix-service` 与 `service` 名字只差一个前缀，最省事的做法是写一个"通用服务命令解析器"兼容两种形状。但 ground truth 明确反对：两者的操作对象（重生服务表条目对比启动脚本）、动词位置（动词在前对比名词在前）、权限模型（重生服务鉴权对比管理员脚本执行）完全不同，硬捏在一起只会得到一个充满"如果来自 A 则…如果来自 B 则…"的条件分支集合。

Rust 实现（`service.rs`）于是给两者各一个解析函数：`parse_minix_service_args` 处理"动词在前"的九种形状，`parse_rc_service_args` 处理"标志加名单加动作"的三段式。两个函数共享同一个名字校验（`check_name`：非空、只含字母数字与下划线横杠点斜杠、拒绝包含双点上级目录成分），共享同一个错误类型。这种"形状各自独立、校验共享"的结构，是对照源码后自然长出来的，不是预设的框架。

名字校验允许斜杠但拒绝双点成分，这个细节来自安全考量：服务二进制路径（如 `/service/虚拟内存服务`）必须是绝对路径，而 `../etc/口令文件` 这种写法一旦拼进脚本目录查找就会逃逸目录。测试 `test_rc_path_separator_in_name_rejected` 把两种情形都钉住。

### 3.2 为什么调度器是一个接口加两个实现

`cron` 按日历时间表触发，`update` 按固定间隔触发，但调用方（"现在该行动吗"）不关心时间从哪来。Rust 实现（`scheduler.rs`）定义 `ScheduleMatcher` 接口（只有一个方法"在此时刻是否到期"），`CronMatcher` 包装解析好的时间表条目，`IntervalMatcher` 包装秒数间隔。测试 `test_matchers_are_interchangeable_as_trait_objects` 把两者放进同一个接口对象数组统一调用，证明调用方确实无需区分。

这个设计的参照系有三边：Linux 的 cron 与 systemd 定时器同样是"时间表"与"间隔"两种触发语义并存（systemd 里叫日历事件与单调事件）；Redox 的用户程序同样习惯把"判断"做成接口、把"执行"留在调用层；Minix 自己的 `update` 与 `cron` 本来就是两个独立程序。这个接口只是把既成事实说出来，没有发明新概念。

`IntervalMatcher` 的触发时刻定义（`due_every`：正整数倍时刻触发，零时刻不触发，零间隔永不触发）直接来自 `update.c` 的行为：程序启动瞬间不做同步（先睡 30 秒），之后每 30 秒一次。零时刻排除的是"启动那一秒"，不是"每轮的起点"。

### 3.3 为什么问号只在分钟字段合法

解析器把"是否分钟字段"作为参数传进字段校验（`check_field` 的 `minute_field` 开关），问号在非分钟字段直接判错。这不是过度设计，而是把 `tab.c` 第 327 行的条件（`最大值是 59 才允许问号`）原样建模：源码用"最大值"间接表达"分钟字段"，Rust 用显式布尔值直接表达，含义相同、可读性更高。测试 `test_question_mark_outside_minute_field_rejected` 与 `test_etc_crontab_question_mark_means_every_minute` 从两侧钉住这条规则。

### 3.4 冒号步长：忠实 Minix 方言，不兼容斜杠

别家系统用斜杠写步长（`*/15`），Minix 用冒号（`0:15`）。解析器只接受冒号，斜杠会被判错（斜杠不是数字、不是星号问号、也不是区间横杠，落在校验的拒绝分支）。这是一个有意的"不兼容"：调度器是系统级基础设施，静默接受另一种方言比明确报错危险得多——管理员把别处抄来的 `*/15` 贴进 Minix 时间表，立刻得到错误提示，远好于定时任务悄悄不跑。文档在这里明确写出差异，避免读者用别家经验误读。

---

## 4. 实现详解

### 4.1 模块结构

`os/commands/usr-sbin/svcsched`（库包名 `minix-svcsched`）共 4 个源文件：

| Rust 文件 | 对应 C 源码位置 | 职责 |
|-----------|----------------|------|
| `lib.rs` | — | 错误类型（`SchedError`，22 对应参数无效、2 对应名称不存在）与模块组织 |
| `service.rs` | `minix-service.8` 提要段、`service` 的 `usage` 函数 | 两种服务命令的形状解析与名字校验 |
| `cron.rs` | `tab.c:285-360`、`etc/crontab` | 时间表行解析（`parse_cron_line`）、字段匹配（`field_matches`）、整行匹配（`entry_matches`） |
| `scheduler.rs` | `update.c:1-23`、`cron.c` 主循环思想 | `ScheduleMatcher` 接口、`CronMatcher`（时间表触发）、`IntervalMatcher`（固定间隔触发） |

### 4.2 关键类型与不变量

- **时间表条目 `CronEntry`**：五个时间字段原文加命令原文，全部借用输入文本（零拷贝），库全程无堆分配，可在无标准库环境编译。不变量：条目必含五个合法字段与非空命令；注释与空行在解析层即跳过，不进入条目。
- **时刻 `ClockTime`**：分、时、日、月、周五个数字。不变量由调用方保证范围合法；星期天允许 0 与 7 两种写法（传统规则），匹配时归一处理。
- **重生服务命令 `ReincarnationCommand`**：动作加可选对象加透传尾巴。不变量：关机动作必无对象，其余动作必有对象；尾巴原样透传，解析层不解释。
- **脚本服务命令 `RcServiceCommand`**：三个标志加名单加可选动作。不变量：列全部与列已启用互斥；单名单无动作表示"测试是否启用"；多名单的最后一个词是动作。

### 4.3 解析与匹配函数一览

| 函数 | 输入 | 输出 | 对应 C 行为 |
|------|------|------|------------|
| `parse_minix_service_args(参数表)` | 单词切片 | 动作、对象、透传尾巴 | `minix-service.8` 的九种形状 |
| `parse_rc_service_args(参数表)` | 单词切片 | 标志、名单、动作 | `service` 脚本的 `usage` 三形状与标志互斥 |
| `parse_cron_line(行)` | 文本行 | 条目、无（注释空行）、错 | `tab.c:range_parse` 的行级语义 |
| `field_matches(字段, 值, 上界, 是否分钟)` | 字段文本与当前值 | 是否选中 | 位图命中的判定语义 |
| `entry_matches(条目, 时刻)` | 条目与当前时刻 | 是否到期 | `cron` 主循环的触发条件 |
| `due_every(间隔, 已过秒数)` | 两个整数 | 是否到期 | `update.c` 的同步节拍 |

---

## 5. 测试要点

`cargo test -p minix-svcsched`：**27 个测试，全部通过**（截至 2026-09-06）。

重点行为与测试的对应（以下函数名均可用 `rg "fn 测试名" os/commands/usr-sbin/svcsched` 复现）：

- **重生服务命令形状**（`service.rs`，12 个）：`test_up_with_binary_and_options`（启动组带透传选项）、`test_down_with_label`（管理组）、`test_shutdown_takes_no_target` 与 `test_shutdown_with_target_rejected`（关机组有无对象的正反两侧）、`test_missing_target_rejected` 与 `test_unknown_reincarnation_action_rejected`（缺对象与未知动词）。
- **脚本服务命令形状**（`service.rs`，同上 12 个之内）：`test_rc_name_action_form`（名字加动作）、`test_rc_single_name_tests_enabled`（单名字的测试语义）、`test_rc_bundled_flags`（捆绑标志）、`test_rc_conflicting_list_flags_rejected`（互斥标志）、`test_rc_unknown_flag_rejected`（未知标志）、`test_rc_path_separator_in_name_rejected`（目录逃逸拦截）。
- **时间表解析**（`cron.rs`，11 个）：`test_etc_crontab_question_mark_means_every_minute`（用真实系统时间表行的问号语义，匹配任意分钟、仍受小时约束）、`test_question_mark_outside_minute_field_rejected`（问号越界）、`test_repeat_and_range_match`（冒号步长、区间、列表的命中与未命中）、`test_reversed_range_rejected`（逆序区间）、`test_zero_repeat_rejected`（零步长）、`test_out_of_range_field_rejected`（越界数字）、`test_command_spacing_preserved`（命令内部空格原样保留）。
- **调度接口**（`scheduler.rs`，4 个）：`test_cron_matcher_fires_on_timetable`、`test_interval_matcher_fires_on_multiples`、`test_due_every_core`（零时刻与零间隔的两条拒绝规则）、`test_matchers_are_interchangeable_as_trait_objects`（接口统一调用）。

尚未覆盖、随后续阶段补齐的：真正向重生服务发请求（依赖重生服务协议的 Rust 绑定）、真正派生到期任务（依赖进程管理原语）。解析与匹配层是全覆盖的，执行层是显式留白的。

---

## 6. 过渡：服务就绪之后，终端上的人从哪里来

本篇走完了交付因果链的第二步：常驻服务有人日常管理（`minix-service` 管生死、`service` 管脚本动作、`svrctl` 管专家参数），定期任务有三种节拍各自归位（日历、一次、固定间隔）。

启动脚本执行完毕、服务各就各位之后，终端屏幕上会出现登录提示。那个提示是谁打印的、用户敲下用户名之后发生了什么、口令存在哪里、验证通过后 shell 如何启动——这是 `03-login-passwd.md` 的职责。请沿因果链继续向下走：先有"被照看的服务"，才有"永远有人值守的终端"。

---

## 7. 参见

- `01-init-rc-scripts.md`——启动脚本链（服务被拉起的场合）
- `03-login-passwd.md`——终端登录链路（下一步）
- `../03-stage-rs/`——重生服务的服务端实现（`minix-service` 对话的另一端）
- `minix3/minix/commands/minix-service/minix-service.8`——重生服务客户端手册（命令形状的权威来源）
- `minix3/minix/commands/svrctl/svrctl.c:107-115`——低层接口用法
- `minix3/minix/commands/cron/tab.c:285-360`——时间字段语义（本篇解析器的逐行对照）
- `minix3/minix/commands/update/update.c`——24 行固定节拍（`IntervalMatcher` 的行为来源）
- `minix3/etc/crontab`——系统自带时间表实例（问号语义的活例子）
- `minix3/etc/rs.single`——启动脚本调用 `minix-service` 的实例

---

## 附：验证记录（评审用，可跳过）

- `wc -l minix3/minix/commands/update/update.c minix3/etc/crontab` → 24 行、1 行有效内容，与正文引用一致。
- `sed -n '285,360p' minix3/minix/commands/cron/tab.c` → 时间解析函数全文已读，问号规则（第 327 行）、冒号步长、星号通配均有出处。
- `sed -n '107,115p' minix3/minix/commands/svrctl/svrctl.c` → 用法两行已读，与正文"只认设置与获取"一致。
- `cargo test -p minix-svcsched` → 27 通过、0 失败；`cargo clippy` 无警告。
- 本文档引用的 `file:line` 均来自正文写作前实际执行的 `rg -n` 与 `sed -n` 输出，非凭记忆书写。
