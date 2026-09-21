# 09-stage-init 文档重建蓝图（deepseek）

## 0. 元数据

- 执行者：deepseek（AI agent 名称，产物后缀）
- 日期：2026-09-19
- 目标目录：`notes/rewrite/fork-syscall-rewrite/09-stage-init/`
- 仓库根目录：`/home/xzhao/github/minix-rs`
- 当前提交号：`6965e6885`；工作树有未提交修改，数据以工作树实测为准。
- 交付物：本文件。除本文件外不修改、不重命名、不移动、不删除任何文件；未提交任何变更。
- 本轮修订（2026-09-19，补做轮）：把 §5 十六篇契约的"知识点：K-xxx…K-yyy"区间展开为逐条清单（编号｜名称｜为什么归本篇，共 131 行，名称取自 §2.2 池表原文）；03 篇原缺该清单，本轮补上；订正 §2.4 统计摘要（总数 153→131、类型分布与存量/新增按池表实测重算）、§9.1 检查三、§9.2 的 G5 行与 §9.3 结论（153→131、17 篇契约→16 篇）。

### 0.1 审查范围

**范围内正式文档（16 篇）**：`00-init-overview.md`、`01-init-main-entry.md`、`02-init-state-machine.md`、`03-init-logging-failure.md`、`04-init-single-user.md`、`05-init-runcom.md`、`06-init-read-ttys.md`、`07-init-session-model.md`、`08-init-session-db.md`、`09-init-multi-user.md`、`10-init-clean-ttys.md`、`11-init-shutdown.md`、`12-init-sysctl-interaction.md`、`13-init-utmp.md`、`14-init-external-contracts.md`、`99-init-global-concepts.md`。

**参考材料**：`plan.md`（389 行，含 §5 函数级与外部契约覆盖表）、`todo.md`（97 行）、`README.md`、`archive/`、`draft/`、`00-master-plan/README.md`、`edge_todo.md` 的 E-INITSYS 条目。

**范围外**（只交叉引用）：内核 boot 流程与 VM 装载（01-stage-kernel）；PM 的进程/孤儿/重启对端（04-stage-pm）；RS 槽位登记（03-stage-rs）；getty/window/shutdown 等被 exec 的命令本体（18-stage-commands）；libc utmp/utmpx 实现与 who/last（14-stage-runtime、18-stage-commands）；内核/MIB 的 sysctl 承载（01/10-stage）；TTY 键盘与电源驱动（16-stage-drivers）；同目录他人重建产物（不读、不引）。

### 0.2 读取清单

**文档**：上述 16 篇全部（由两个只读探查任务通读并逐条核对锚点；主会话复核承重结论与计划覆盖表）。另读 `plan.md` §5（函数级映射 46 函数、外部契约 13 项、排除表）与 §1-§3。

**C 源码（ground truth）**：`minix3/sbin/init/init.c`（1902 行；46 个函数定义按行号逐一核对）、`minix3/sbin/init/pathnames.h`（40 行）、`sbin/init/init.8`（man 行为契约）、`sbin/init/Makefile`（构建变体 `RESCUEDIR`/`LETS_GET_SMALL` 等）、`sbin/init/NOTES`。对端：`kernel/table.c:64`、`rs/table.c:28`、`servers/vm/main.c:345-347,498-514`、`servers/pm/{main.c:188-204,forkexit.c:336,396,misc.c:223-224,schedule.c:34,73,const.h:9}`、`fs/procfs/service.c:195-207`、`drivers/tty/tty/arch/i386/keyboard.c:293-300`、`drivers/power/tps65217/tps65217.c:226`、`include/{paths.h,ttyent.h,utmpx.h,utmp.h}`、`lib/libc/gen/{getttyent.c,utmpx.c}`。

**Rust 实现**：`os/commands/sbin/init/src/` 20 个 `.rs`（5,488 行；实测 `#[test]` 136 个）；对端现状：`os/servers/pm/src/misc.rs:558-574`（reboot 对端）、`os/servers/mib/src/subtree/kern.rs:158-162` 与 `walker.rs:623`（`KERN_SECURELVL` 部分落地）、`os/commands/bin/proctools/src/utmp.rs`（compat utmp 解析器）、`os/drivers/tty/tty/src/keyboard.rs`（无 SIGABRT 发送）、`os/drivers/power/tps65217/`（无 SIGUSR1 通知）、`os/commands/sbin/`（无 shutdown 命令）。

### 0.3 证据命令与关键输出

```text
$ wc -l minix3/sbin/init/init.c → 1902；pathnames.h → 40
$ grep -c '^[a-z]' init.c 等价函数清单 → 46 个函数（plan §5.2 表）
$ find os/commands/sbin/init/src -name '*.rs' | wc -l → 20；xargs wc -l → 5488
$ grep -rc '#\[test\]' os/commands/sbin/init/src/*.rs | awk -F: '{s+=$2} END{print s}' → 136
$ grep -n 'KERN_SECURELVL' os/servers/mib/src/subtree/kern.rs → :158-162（部分落地）
$ grep -rn 'SIGABRT\|INIT_PROC_NR' os/drivers/tty/tty/src/keyboard.rs → 0（发送端缺失）
$ grep -rn 'SIGUSR1' os/drivers/power/tps65217/ → 0
$ ls os/commands/sbin/ → devdb diskfmt init maint mountinfo（无 shutdown）
$ grep -n 'pututxline\|logwtmpx' os/ -r → 仅 init 文档/注释；Rust 写路径为 append_file（ENOSYS）
$ grep -rho '工具生成' 09-stage-init/[0-9]*.md | wc -l → 39
$ grep -rho '\.design/' 09-stage-init/[0-9]*.md | wc -l → 0
$ git log --oneline -1 → 6965e6885
```

---

## 1. C 真序

### 1.1 阶段类型判定

判定为**状态机推进型用户进程**，既不是系统服务事件循环（无 IPC 主循环、无 SEF、无 callmap），也不是启动链型内核路径。init 的执行驱动只有两条：`waitpid` 阻塞循环（子进程退出事件）与信号（7 类 handler）。因此真序 = 启动八步 → 状态机七状态推进 → 会话生命周期。

### 1.2 真序表

**启动段**（boot 链终点到第一次 transition）：

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| S01 | boot_image 最后一项登记；`USR_F` 用户进程（非系统服务） | `kernel/table.c:64`；`rs/table.c:28` | 普通用户进程，pid 1 |
| S02 | VM `exec_bootproc` 加载 ELF；boot argv 固定 `{"init",NULL}` | `vm/main.c:498-514`、`345-347` | `-s/-f` 不经 boot 路径 |
| S03 | PM 初始化：INIT 父进程=自身、`INIT_PID=1`、scheduler=KERNEL | `pm/main.c:188-204`；`pm/const.h:9` | 孤儿收养的根 |
| S04 | 抑制解除，内核首次调度 init | 交叉引用 01-stage-kernel/09、10 | — |
| S05 | `main()`：身份校验（uid≠0 → EPERM；pid≠1 → "already running"） | `init.c:229,242-249` | 双重出生证明 |
| S06 | `setsid()` 建立初始会话 | `init.c:256` | 会话首进程 |
| S07 | `mfs_dev()`：/dev/console 缺失则跑 MAKEDEV（minix 专用；含 `#if 0` 死段） | `init.c:1703-1788`（死段 1716-1756） | 设备兜底 |
| S08 | `getopt "sf"`：单用户/快速启动 | `init.c:287-303` | 唯二启动参数 |
| S09 | 信号注册：`handle`（全集屏蔽 + SA_NOCLDSTOP）与 `delset`（放行 9 个） | `init.c:369-389,394-405,314-334` | 7 类 handler |
| S10 | `close(0/1/2)`；`securelevel_present = has_securelevel()` | `init.c:339-341,544-563` | 能力探测 |
| S11 | `transition(requested_transition)` 进入状态机 | `init.c:624-640` | 主循环 |

**状态机段**（七状态，`init.c:133-139`）：

| 状态 | 动作 | C 锚点 | 出口 |
|------|------|--------|------|
| 's' single_user | 安全级降 0；fork shell（SECURE 口令门 / ALTSHELL 回退）；waitpid 五结局 | `init.c:694-874` | 退出 → runcom（或 FASTBOOT 前进） |
| 'r' runcom | `runetcrc` fork `/etc/rc`（autoboot/fastboot）；chroot 决策；`did_multiuser_chroot` 二遍 | `init.c:974-1014,879-969` | 成功 → read_ttys；失败 → single_user |
| 't' read_ttys | 先清空再重建：`/etc/ttys` 解析 → `new_session` 链表 + session DB；DB 失败路由 | `init.c:1222-1285,1792-1806` | → multi_user |
| 'm' multi_user | 安全级 0→1；`start_window_system` → `start_getty` 全启动；waitpid 稳态 + `collect_child` | `init.c:1528-1564,1290-1319,1321-1370,1460-1497` | 信号驱动 T/c/d |
| 'T' clean_ttys | SIGHUP：重读 `/etc/ttys`，PRESENT 标记 diff，下线行 SIGHUP | `init.c:1569-1629` | → multi_user |
| 'c' catatonia | SIGTSTP：全部会话标 `SE_SHUTDOWN` | `init.c:1634-1643` | → multi_user |
| 'd' death | SIGTERM：三轮 HUP/TERM/KILL（`DEATH_WATCH=10`）；ESRCH/ECHILD 提前返回 | `init.c:1661-1698` | → single_user |

**会话生命周期段**：

| 步 | 动作 | C 锚点 |
|----|------|--------|
| L01 | `read_ttys` 逐行 `new_session`（三条件过滤），`device="/dev/"+name` | `init.c:1147-1149,1159` |
| L02 | `start_getty`：chroot 路径 → 防抖动 sleep（子进程内）→ window → `execv(getty)` | `init.c:1321-1370` |
| L03 | `waitpid` → `find_session` → `collect_child`：未知 pid 忽略 / 重启 / SHUTDOWN 移除 | `init.c:1460-1497` |
| L04 | utmp 记录：`session_utmpx`/`make_utmpx`/`get_runlevel`/`utmpx_set_runlevel`/`clear_session_logs` | `init.c:1372-1458,647-666` |
| L05 | 会话 DB：`start_session_db`/`add_session`/`del_session`/`find_session`（pid→会话索引） | `init.c:1021-1096` |

**信号路由表**（`init.c:314-334,369-405,1502-1522`）：SIGABRT→`minixreboot`、SIGUSR1→`minixpowerdown`、SIGFPE/ILL/SEGV/BUS→`disaster`、SIGHUP→'T'、SIGTERM→'d'、SIGTSTP→'c'、SIGALRM→`alrm_handler`（death 计时）。

### 1.3 序差表

| # | 运行时事实 | 教学序选择 | 理由 | 回指 |
|---|-----------|-----------|------|------|
| D-1 | 信号 handlers 在 S09 注册，但语义分属 02/03/11/14 | 02 讲注册与转换骨架，各 handler 语义散在 03/11/14 | 注册是骨架，语义依赖状态机 | 02 指向各篇；14 收外部契约 |
| D-2 | `mfs_dev` 是入口八步之一，却属设备兜底 | 01 完整讲（入口职责） | 与 main 的失败路径耦合 | 01 标注 minix 专用 |
| D-3 | `runcom` 可能经 chroot 跑第二遍 | 05 讲两遍，12 讲 `shouldchroot` 机制 | chroot 是 sysctl 交互面 | 05 调用点、12 机制 |
| D-4 | `collect_child` 的会话查找依赖 session DB | 08 在 09 之前 | 数据模型先行 | 09 开头回指 08 |
| D-5 | utmp 钩子分散在 05/07/08/09/11 的多个调用点 | 13 集中讲账本，各调用点一行 | 文件格式与 who 语义量大 | 各篇标注"记录点见 13" |
| D-6 | 安全级 boost 在 multi_user、降级在 single_user、探测在 main | 12 集中机制，04/09 只留调用 | 机制一处 | 12 开头回指三调用点 |
| D-7 | 孤儿收养/重启/调度继承是对端 PM 行为 | 14 讲外部契约 | init 语义离不开对端 | 14 只给契约不展开 PM |
| D-8 | Ctrl-Alt-Del/低电通知的发送端在驱动，Rust 侧尚缺 | 14 契约 + 现状标注 | 跨 stage 缺口 | 14 §现状；16-stage 交叉引用 |

---

## 2. 知识点全集

### 2.1 说明

编号 `K-NNN` 按新目录顺序分块；来源：存量（现有 16 篇）／新增（C/制品/理论承载，旧文未讲或有错，必须有锚）。

### 2.2 池总表

#### 00-init-overview（新：00）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-001 | init=普通用户进程（USR_F，非服务器） | 概念 | 存量 | 00 核心点；14 §1 | `rs/table.c:28` | 执行模型正名 |
| K-002 | boot 链终点（boot_image 最后一项） | 概念 | 存量 | 00 核心点 | `kernel/table.c:64` | 启动位置 |
| K-003 | 状态机主线七状态图 | 机制 | 存量 | 00 核心点；02 §2.2 | `init.c:133-139,624-640` | 叙事主轴 |
| K-004 | 双驱动（waitpid + 信号）执行模型 | 概念 | 存量 | 00 核心点；02 | `init.c:825-873,369-405` | 与服务器对照 |
| K-005 | Rust 实现形态（InitHost 单接缝 / DriverState 单所有者 / ScriptHost） | 架构演进 | 存量 | 00 Rust 实现形态 | `host.rs`/`driver.rs` 符号 | 改写骨架 |
| K-006 | 导航与边界 | 工具工程 | 存量 | 00 边界 | — | 阅读路线 |

#### 01-init-main-entry（新：01）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-010 | 八步入口全景（身份→setsid→mfs_dev→getopt→信号→close/探测→transition） | 机制 | 存量 | 01 §2.1 | `init.c:242-358` | 入口清单 |
| K-011 | 身份两行拒绝（uid/pid） | 机制 | 存量 | 01 §2.2 | `init.c:242-249` | 双重出生证明 |
| K-012 | getopt "sf" 语义与边界 | 接口与协议 | 存量 | 01 §2.3 | `init.c:287-303` | 启动参数 |
| K-013 | mfs_dev 三层逻辑与 `#if 0` 死段 | 机制 | 存量 | 01 §2.4 | `init.c:1703-1788` | 设备兜底 |
| K-014 | 信号注册调用点（10 信号/5 handler 组） | 接口与协议 | 存量 | 01 §2.5（旧写 8 类） | `init.c:314-334,326-327` | 信号名单 |
| K-015 | securelevel 探测调用点 | 机制 | 存量 | 01 §2.6 | `init.c:544-563` | 移交 12 |
| K-016 | close 0/1/2 与不可达 return | 机制 | 存量 | 01 §2.1 | `init.c:339-341,70` | 收尾 |
| K-017 | `BootArgs`/`parse_boot_args` 纯解析 | 架构演进 | 存量 | 01 §3.2 | `entry.rs` 符号 | 边界可测 |
| K-018 | `EntryDecision`/`decide_entry` 首状态决策 | 架构演进 | 存量 | 01 §3.4（旧引 `to_state_kind`） | `entry.rs`/`main.rs` 符号 | 决策显式 |
| K-019 | `console_present`/`ensure_console` seam | 架构演进 | 存量 | 01 §3.5 | `host.rs`/`entry.rs` 符号 | 副作用隔离 |
| K-020 | 身份校验现状（内联、uid ENOSYS 仅警告） | 架构演进 | 新增 | —（旧文称 `EntryError`/`check_identity`） | `main.rs:40-51` | 真实装配 |

#### 02-init-state-machine（新：02）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-025 | `state_func_t` 双重函数指针类型 | 数据结构 | 存量 | 02 §2.1 | `init.c:130-131` | 类型体操 |
| K-026 | 七状态字符大小写敏感（runlevel 协议） | 数据结构 | 存量 | 02 §2.2 | `init.c:133-139` | 状态编码 |
| K-027 | `handle` 全集屏蔽 + SA_NOCLDSTOP；`delset` 放行 9 个 | 机制 | 存量（纠错） | 02 §2.3（旧写 8 个） | `init.c:369-389,394-405` | 信号掩码 |
| K-028 | `transition` 主循环与空指针防御 | 机制 | 存量 | 02 §2.4 | `init.c:624-640` | 主循环 |
| K-029 | 信号→状态三映射与 default 清零 | 机制 | 存量 | 02 §2.5 | `init.c:1502-1522` | 转换表 |
| K-030 | `alrm_handler` 与 `clang` 计时 | 机制 | 存量 | 02 §2.5 | `init.c:1649-1655,173` | death 倒计时 |
| K-031 | `StateKind` 枚举与往返转换 | 架构演进 | 存量 | 02 §3.2 | `state_machine.rs` 符号 | 穷尽检查 |
| K-032 | `signal_to_state` 纯映射与从属号反查 | 架构演进 | 存量 | 02 §3.3 | `state_machine.rs` 符号 | 可测映射 |
| K-033 | `DriverState` 切片（Ledger/ChildCollector/SignalState） | 架构演进 | 存量 | 02 §3.4 | `driver.rs`/`signal_state.rs` 符号 | 单所有权 |
| K-034 | minix-sys 信号导出现状（`pub use minix_sys::signal as sig`） | 架构演进 | 新增（纠错） | 02 §3.3（旧称"待导出"） | `state_machine.rs:12-17` | E-INITSYS ④ 已闭 |

#### 03-init-logging-failure（新：03）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-040 | `STALL_TIMEOUT=30` 读屏语义 | 数据结构 | 存量 | 03 §2.1 | `init.c:95` | 停等动机 |
| K-041 | `stall`：日志 + closelog + sleep | 机制 | 存量 | 03 §2.2 | `init.c:440-450` | 失败惯例 |
| K-042 | `warning`/`emergency` 级别分界 | 机制 | 存量 | 03 §2.3 | `init.c:457-466,472-481` | 日志级别 |
| K-043 | `disaster` 遗言 + `_exit(sig)` | 机制 | 存量 | 03 §2.4 | `init.c:504-511` | 崩溃重启触发 |
| K-044 | `print_console`/`badsys` 明确排除 | 工具工程 | 存量 | 03 §2.5 | `init.c:411-432,490-498` | 阅读避坑 |
| K-045 | `Severity` 两级与 host 睡眠注入 | 架构演进 | 存量 | 03 §3.2/§3.3 | `log.rs`/`host.rs` 符号 | 可测输出 |

#### 04-init-single-user（新：04）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-050 | 退出即前进（FASTBOOT→runcom） | 概念 | 存量 | 04 §1 | `init.c:866-870` | 状态语义 |
| K-051 | 父进程三段（准备/fork/看护） | 机制 | 存量 | 04 §2.1 | `init.c:715-873` | 结构 |
| K-052 | 安全级先降 0 | 机制 | 存量 | 04 §2.2 | `init.c:723-725` | 抢修可写 |
| K-053 | SECURE 口令门四条件与重试/^D | 机制 | 存量 | 04 §2.3 | `init.c:747-763` | 条件信任 |
| K-054 | ALTSHELL 选择与双 exec 兜底 | 机制 | 存量 | 04 §2.3 | `init.c:768-783,803-810` | shell 回退 |
| K-055 | 看护循环五结局 | 机制 | 存量 | 04 §2.4 | `init.c:825-873` | wait 语义 |
| K-056 | `SingleUserOutcome` 四变体（旧文写五） | 架构演进 | 纠错 | 04 §3.3 | `single_user.rs` 符号 | 结局建模 |
| K-057 | 口令循环内联现状（旧文引 `classify_attempt`） | 架构演进 | 新增 | — | `single_user.rs`/`password.rs` 符号 | 无虚构函数 |

#### 05-init-runcom（新：05）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-060 | 只看退出码的黑盒契约 | 概念 | 存量 | 05 §1 | `init.c:959-968` | rc 可演化 |
| K-061 | autoboot/fastboot 参数 | 接口与协议 | 存量 | 05 §2.1 | `init.c:897-900` | 唯一传参 |
| K-062 | 子进程六步与两个失败码 | 机制 | 存量 | 05 §2.2 | `init.c:887-915` | 失败路由 |
| K-063 | 五归宿表（含 catatonia+SIGTERM 双条件） | 机制 | 存量 | 05 §2.3 | `init.c:917-968` | wait 语义 |
| K-064 | 两次执行与 `did_multiuser_chroot` | 机制 | 存量 | 05 §2.4 | `init.c:974-1013` | chroot 二遍 |
| K-065 | `RuncomResult` 三变体（旧文引 `RcOutcome`） | 架构演进 | 纠错 | 05 §3 | `runcom.rs` 符号 | 结局建模 |
| K-066 | 台账交接（`ledger.reboot`，旧文引 `deps.record_reboot`） | 架构演进 | 纠错 | 05 §4 | `driver.rs` 符号 | 单所有者 |

#### 06-init-read-ttys（新：06）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-070 | ttys 四列表与 TTY_ON/SECURE | 数据结构 | 存量 | 06 §1 | `ttyent.h:57-58` | 登录开关 |
| K-071 | 先清空再重建策略 | 机制 | 存量 | 06 §1 | `init.c:1252-1260` | 重读语义 |
| K-072 | read_ttys 六步与 DB 失败双路由 | 机制 | 存量 | 06 §2 | `init.c:1229-1284,1262-1271` | 降级路径 |
| K-073 | chroot 感知路径（`do_setttyent`） | 机制 | 存量 | 06 §1/§3 | `init.c:1792-1806` | 路径拼接 |
| K-074 | new_session 三条件过滤 | 机制 | 存量 | 06 §2 | `init.c:1147-1149` | 开/关行 |
| K-075 | A-6 原生解析器替代 getttyent | 架构演进 | 存量 | 06 §3 | `ttys.rs` 符号；`getttyent.c` | 无 libc 依赖 |
| K-076 | 引号/quoted 空 getty/精确 token/off 后赢/转义引号 | 机制 | 存量 | 06 §3 | `getttyent.c:129-134,174-188` | 解析语义 |
| K-077 | 已知边界（续行反斜杠未实现） | 工具工程 | 存量 | 06 §3 | `fparseln` | 诚实缺口 |

#### 07-init-session-model（新：07）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-085 | `session_t` 全字段与 `SE_*` 标志 | 数据结构 | 存量 | 07 §2.1 | `init.c:156-170,161-162` | 内存模型 |
| K-086 | `se_started` 防抖动用途 | 数据结构 | 存量 | 07 §1 | `init.c:159` | 与 09 衔接 |
| K-087 | new_session 四关 | 机制 | 存量 | 07 §2.2 | `init.c:1142-1180` | 失败路径 |
| K-088 | `device="/dev/"+name` | 机制 | 存量 | 07 §2.2 | `init.c:1159` | 路径拼接 |
| K-089 | setupargv 两段分词与 `construct_argv` 空格制表切分 | 机制 | 存量 | 07 §2.3 | `init.c:1101-1118,1185-1217` | argv 来源 |
| K-090 | free_session 释放顺序 | 机制 | 存量 | 07 §2.3 | `init.c:1123-1137` | 内存管理 |
| K-091 | 链表指针不入节点/值类型 RAII | 架构演进 | 存量 | 07 §3 | `session.rs` 符号 | 避免侵入式别名 |
| K-092 | `ParsedCommand` 两形状（exec_path 与 argv） | 架构演进 | 存量 | 07 §3 | `session.rs`/`contracts.rs` 符号 | exec 路径 ≠ argv0 |

#### 08-init-session-db（新：08）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-095 | pid→会话反向索引（waitpid 只有 pid 的困境） | 概念 | 存量 | 08 §1 | `init.c:1021-1096` | 数据必要性 |
| K-096 | `dbopen(NULL,HASH)` 内存表（无持久化） | 数据结构 | 存量 | 08 §1 | `init.c:1027` | 语义边界 |
| K-097 | start/add/del/find 四原语（DB 未开静默） | 机制 | 存量 | 08 §2 | `init.c:1021-1096` | DB 全貌 |
| K-098 | A-1：Berkeley DB → `HashMap` | 架构演进 | 存量 | 08 头块 [ARCH A-1] | `session_db.rs` 符号 | 无 db 依赖 |
| K-099 | `SessionDb` trait 是接缝外唯一 trait | 架构演进 | 存量（纠错） | 08 §3（host.rs 注释冲突） | `session_db.rs`/`host.rs` | 接缝边界 |
| K-100 | 值语义（索引 `copied()`，非指针 memmove） | 架构演进 | 纠错 | 08 §3 | `session_db.rs:55` | 现状 |

#### 09-init-multi-user（新：09）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-105 | 稳态=全启动 + waitpid 回收循环 | 概念 | 存量 | 09 §1 | `init.c:1528-1564` | 稳态定义 |
| K-106 | 安全级 0→1（-1 别动） | 机制 | 存量 | 09 §3 | `init.c:1543-1544` | 升级语义 |
| K-107 | `start_window_system` 先于 getty，失败 `_exit(6)` | 机制 | 存量 | 09 §2 | `init.c:1290-1319` | 启动顺序 |
| K-108 | `start_getty` 四步（chroot→防抖→window→exec） | 机制 | 存量 | 09 §2 | `init.c:1321-1370` | getty 启动 |
| K-109 | `setctty`（setsid+DTR+login_tty） | 机制 | 存量 | 09 §2 | `init.c:669-689` | 控制终端 |
| K-110 | `collect_child` 三处理（忽略/重启/移除） | 机制 | 存量 | 09 §2 | `init.c:1460-1497` | 回收 |
| K-111 | 三时间常量 `GETTY_SPACING/SLEEP/DTRTIME` | 数据结构 | 存量 | 09 §2 | `init.c:92-94,98` | 防抖动参数 |
| K-112 | `getty_delay_secs`/`CollectAction` 现状（分类器已删） | 架构演进 | 纠错 | 09 §3/§4 | `multi_user.rs` 符号 | 无虚构函数 |
| K-113 | `multi_user(host, collector, signals)` 真实签名 | 架构演进 | 纠错 | 09 §4 | `multi_user.rs:163-167` | 装配形状 |

#### 10-init-clean-ttys（新：10）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-120 | PRESENT 标记 diff（还在/新来/消失） | 机制 | 存量 | 10 §1 | `init.c:1569-1629` | 重读算法 |
| K-121 | n² 算法注释（重读少发生） | 工具工程 | 存量 | 10 §1 | `init.c:1567` | 简单正确优先 |
| K-122 | 清标记→逐行匹配→收尾关停三步 | 机制 | 存量 | 10 §2 | `init.c:1577-1626` | 流程 |
| K-123 | 序号变化 warning 与解析失败挂 SIGHUP | 机制 | 存量 | 10 §2 | `init.c:1593-1596,1606-1612` | 行为细节 |
| K-124 | `diff_line` 分类器已删（旧文引） | 架构演进 | 纠错 | 10 §3 | `clean_ttys.rs` 符号 | 无虚构函数 |
| K-125 | `_db` 参数未使用现状 | 工具工程 | 新增 | — | `clean_ttys.rs:30` | 诚实缺口 |

#### 11-init-shutdown（新：11）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-130 | catatonia=关登录（标全部 SHUTDOWN） | 概念 | 存量 | 11 §1 | `init.c:1634-1643` | 假死语义 |
| K-131 | death 三轮升级（HUP→TERM→KILL） | 机制 | 存量 | 11 §1/§2 | `init.c:1661-1698,1667` | 真死语义 |
| K-132 | `DEATH_WATCH=10` 与 `clang`/alarm 计时 | 数据结构 | 存量 | 11 §2 | `init.c:96,1684-1685` | 计时 |
| K-133 | `kill(-1)` ESRCH 与 ECHILD 提前返回 | 机制 | 存量 | 11 §2 | `init.c:1681-1682,1691-1692` | 收工条件 |
| K-134 | `ps axl` warning（三轮杀不完） | 工具工程 | 存量 | 11 §2 | `init.c:1695` | 失败告警 |
| K-135 | `DEATH_SEQUENCE` 与 `&AtomicBool` 现状（旧文引 `Arc<AlarmFlag>`） | 架构演进 | 纠错 | 11 §4 | `shutdown.rs:35` | 装配形状 |
| K-136 | 台账入口 `ledger.shutdown`（旧文引 `deps.record_shutdown`） | 架构演进 | 纠错 | 11 §4 | `driver.rs:188` | 单所有者 |

#### 12-init-sysctl-interaction（新：12）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-140 | securelevel 写保护档（单用户 0/多用户 1） | 概念 | 存量 | 12 §1 | `init.c:545-618` | 两个旋钮之一 |
| K-141 | init.root 第二根（rc 在新根第二遍） | 概念 | 存量 | 12 §1 | `init.c:1811-1900` | 另一个旋钮 |
| K-142 | `has_securelevel` 探测（ENOENT→0） | 机制 | 存量 | 12 §2 | `init.c:544-563` | 能力探测 |
| K-143 | `getsecuritylevel`/`setsecuritylevel` 语义 | 机制 | 存量 | 12 §2 | `init.c:568-589,594-618` | 查询/设置 |
| K-144 | `createsysctlnode`（init/root，默认 `/`） | 机制 | 存量 | 12 §2 | `init.c:1811-1857` | 节点创建 |
| K-145 | `shouldchroot`（ENOENT 重建/非串拒绝） | 机制 | 存量 | 12 §2 | `init.c:1859-1900` | chroot 决策 |
| K-146 | A-4 状态修正：MIB 已有 `KERN_SECURELVL`（部分落地） | 架构演进 | 新增（纠错） | 12 §3（旧称"内核均无"） | `mib/subtree/kern.rs:158-162`、`walker.rs:623` | 对端现状 |
| K-147 | A-5 init.root 完全缺失（init 侧 ENOSYS） | 架构演进 | 存量 | 12 §3 | `host.rs:228-238` | 缺口语义 |
| K-148 | `should_chroot` 非空判断（Rust 简化） | 架构演进 | 存量 | 12 §4 | `sysctl.rs` 符号 | 决策差异 |

#### 13-init-utmp（新：13）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-155 | who/last 的数据来源（LOGIN/DEAD/RUN_LVL/BOOT/SHUTDOWN） | 概念 | 存量 | 13 §1 | `init.c:647-1451` | 账本用途 |
| K-156 | `session_utmpx` 三选一（getty/window/空） | 机制 | 存量 | 13 §2 | `init.c:1372-1381` | 记录构造 |
| K-157 | `make_utmpx`（零化/拷名/ut_id 取行尾） | 机制 | 存量 | 13 §2 | `init.c:1383-1409` | 字段填充 |
| K-158 | `get_runlevel` 七状态映射与未知 DEATH | 机制 | 存量 | 13 §2 | `init.c:1411-1427` | runlevel |
| K-159 | `utmpx_set_runlevel` 空会话短路 | 机制 | 存量 | 13 §2 | `init.c:1429-1451` | /var 未可写 |
| K-160 | `clear_session_logs` 双通道注销 | 机制 | 存量 | 13 §2 | `init.c:647-662` | 清理 |
| K-161 | SUPPORT_UTMP/UTMPX 双开（Makefile:8） | 工具工程 | 存量 | 13 §1 | `sbin/init/Makefile:8` | 构建事实 |
| K-162 | A-2 缺口：Rust 写路径 ENOSYS + 文本临时编码 | 架构演进 | 存量 | 13 §3 | `utmp.rs`/`host.rs:333` | 现状 |
| K-163 | `RecordType::Dead=7` 与 C `DEAD_PROCESS=8` 不一致 | 约束与不变量 | 新增（纠错） | 13 §3（旧文背书 7） | `utmp.rs:45`；`utmpx.h:64` | 真值 bug |
| K-164 | BOOT_TIME/DOWN_TIME 记录未建模 | 工具工程 | 新增 | —（旧文未披露） | `init.c:1233,1244`；`utmp.rs` 枚举 | 覆盖缺口 |
| K-165 | proctools 已有 compat utmp 解析器（旧文称"无读者"过强） | 接口与协议 | 新增 | — | `os/commands/bin/proctools/src/utmp.rs` | 现状 |

#### 14-init-external-contracts（新：14）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-170 | USR_F 身份与 procfs `service_active` 不算系统服务 | 概念 | 存量 | 14 §1 | `rs/table.c:28`；`procfs/service.c:195-207` | 身份契约 |
| K-171 | `minixreboot`/`minixpowerdown` fork+exec shutdown | 机制 | 存量 | 14 §2.1 | `init.c:517-538` | 重启路径 |
| K-172 | boot 映像最后一项与 boot argv | 契约 | 存量 | 14 §2.2 | `kernel/table.c:64`；`vm/main.c:345-347` | 启动契约 |
| K-173 | PM：INIT 父=自身/INIT_PID=1/scheduler | 契约 | 存量 | 14 §2.2 | `pm/main.c:188-204`；`pm/const.h:9` | 身份契约 |
| K-174 | 孤儿收养与 INIT 死亡只栈回溯 | 契约 | 存量 | 14 §2.2 | `pm/forkexit.c:336-341,396` | 生命周期 |
| K-175 | reboot 路径 `sys_stop(INIT)` | 契约 | 存量 | 14 §2.2 | `pm/misc.c:223-224` | 关机协同 |
| K-176 | 调度继承与 RS 父行 | 契约 | 存量（纠错） | 14 §2.2 | `pm/schedule.c:34,73`；`pm/main.c:204` | 继承契约 |
| K-177 | Ctrl-Alt-Del→SIGABRT / 低电→SIGUSR1 | 契约 | 存量 | 14 §2.2 | `keyboard.c:300`；`tps65217.c:226` | 两驱动通知 |
| K-178 | Rust 现状：shutdown 命令缺失、两发送端缺失、请求未消费 | 工具工程 | 新增 | 14 §1/§4（旧文含糊） | `os/commands/sbin`；`keyboard.rs`；`contracts.rs` | 跨 stage 缺口 |
| K-179 | `request_for`/`shutdown_argv` 纯映射（无静态契约表） | 架构演进 | 纠错 | 14 §3 | `contracts.rs` 符号 | 真实形态 |

#### 99-init-global-concepts（新：99）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-185 | 七状态字符/超时族/DTR 常量表 | 数据结构 | 存量 | 99 核心点 | `init.c:92-98,133-139` | 常量单点 |
| K-186 | 路径族（rc/ttys/console/utmpx/utmp）与权威头 | 数据结构 | 存量（纠错） | 99 核心点/常量表 | `paths.h:62`、`ttyent.h:40`、`utmpx.h` | 路径真值 |
| K-187 | `_PATH_SLOGGER` 死常量 | 工具工程 | 存量 | 99 核心点 | `pathnames.h:39` | 排除项 |
| K-188 | `INIT_PID 1` 与内核/MIB 端点 | 数据结构 | 存量 | 99 核心点 | `pm/const.h:9` | 身份常量 |
| K-189 | C 全局→单所有者视图映射（11 项） | 架构演进 | 存量 | 99 全局状态 | `init.c:151-211`；`driver.rs` | A-4 依据 |
| K-190 | ARCH A-1…A-12 状态总表 | 架构演进 | 新增 | —（散见各篇） | `session_db.rs`/`utmp.rs`/`sysctl.rs` 等符号 | 决策导航 |
| K-191 | A-7 构建变体为计划而非现状（Cargo 无 feature） | 工具工程 | 新增（纠错） | 99 核心点 | `Cargo.toml`；`sbin/init/Makefile` | 现状诚实 |
| K-192 | 测试基线 136 与模块分布 | 测试性质 | 新增 | 各篇 §5.1（143 过期） | 实测 | 验证边界 |
| K-193 | 常量→Rust 标识符落点映射 | 工具工程 | 存量 | 99 常量表 | `multi_user.rs:23-25` 等 | 落点索引 |

### 2.3 重复与主讲述点

| 主题 | 旧文出现处 | 主讲述点（新） |
|------|-----------|----------------|
| 安全级机制 | 04 §2.2、09 §3、12 全篇 | 12；04/09 调用点 |
| collect_child 回收 | 04 §2.4、09 §2、11 §6 | 09 |
| utmp 记录点 | 05/07/08/09/11 各一句、13 全篇 | 13 |
| chroot | 05 §2.4、12 §2 | 12（机制）＋05（调用） |
| 信号名单 | 01 §2.5、02 §2.3、14 §2.2 | 02（注册）＋14（外部两信号） |
| session_t/DB | 07/08 | 07（结构）＋08（索引） |
| 常量表 | 99 与各篇 | 99（总表）；各篇一行 |
| 测试总数 | 03-14 各 §5.1（143） | 各篇只报本模块；99 汇总 136 |

### 2.4 统计摘要

- 知识点总数：**131 条**（§2.2 池总表的实际行数；编号区间为 K-001…K-193）。类型分布：机制 51、架构演进 32、概念 12、工具工程 12、数据结构 12、契约 6、接口与协议 4、约束与不变量 1、测试性质 1。
- 存量 119 条、新增 12 条；新增集中在 K-020/K-034/K-057/K-125/K-146/K-163/K-164/K-165/K-178/K-190/K-191/K-192 一带（时间差与跨 stage 现状类）。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路：① C 符号（`init.c` 46 个函数、`session_t`/`state_t` 类型、七状态字符、超时族、信号表）；② 操作系统通用概念（PID 1 职责：孤儿收养、会话与控制终端、runlevel、utmp、串行启动、单用户抢修、chroot 第二根、信号驱动状态机、服务重启）；③ 非 C 制品（`pathnames.h`、`init.8` man 行为契约、`NOTES` POSIX 职责、`Makefile` 构建变体、`paths.h`/`ttyent.h`/`utmpx.h`/`utmp.h`、libc `getttyent.c`/`utmpx.c`、对端 PM/VM/kernel/procfs/驱动）；④ 阶段边界契约（boot 链位置、PM 进程语义、MIB securelevel、TTY/电源驱动通知、shutdown 命令、proctools utmp 解析器）。

### 3.2 C 源与制品 → 新文档映射

| 源/制品 | 规模 | 新文档 | 核对 |
|---------|------|--------|------|
| `init.c` | 1902 行 / 46 函数 | 01-14 按函数分片（`plan.md` §5.2 全表） | 46 函数逐一落篇，无遗漏 |
| `pathnames.h` | 40 行 | 99（路径常量） | `_PATH_RUNCOM:40`、`_PATH_SLOGGER:39`（死） |
| `init.8`（man） | — | 02（状态语义/POSIX 职责）、00（边界） | 行为契约来源 |
| `sbin/init/NOTES` | — | 02（job control/孤儿回收/控制终端） | 参考 |
| `sbin/init/Makefile` | 21 行 | 99（构建变体：`RESCUEDIR`/`LETS_GET_SMALL`/`SUPPORT_UTMP*`） | A-7 计划 vs 现状 |
| `include/{paths.h,ttyent.h,utmpx.h,utmp.h}` | — | 99/13/06 | 路径与格式 |
| `lib/libc/gen/getttyent.c` | — | 06（解析语义对照） | `skip:174-188`、`q^=:185` |
| 对端（PM/VM/kernel/procfs/键盘/电源） | — | 14（13 项外部契约） | plan §5.3 全核 |
| `os/commands/sbin/init/src/` | 20 文件 5,488 行 | 各篇 §实现 + 99 | 实测 136 测试 |

### 3.3 覆盖缺口表

| # | 主题 | 证据 | 现状 | 建议 |
|---|------|------|------|------|
| G-01 | Rust 标识符虚构（8 处） | `EntryError`/`check_identity`/`to_errno`/`to_state_kind`/`classify_attempt`/`classify_wait`/`classify_rc_exit`/`RcOutcome`/`deps.record_reboot`/`deps.record_shutdown`/`Arc<AlarmFlag>`/`classify_collect`/`diff_line` 全不在 crate | 文档与实现脱节 | 各篇按真实符号重写（K-020/K-056/K-057/K-065/K-066/K-112/K-124/K-135/K-136/K-179） |
| G-02 | utmp `RecordType::Dead=7` vs C `DEAD_PROCESS=8` | `utmp.rs:45`；`utmpx.h:64` | 代码 bug + 文档背书 | 13 篇按 C 真值记录并标偏离（K-163） |
| G-03 | BOOT_TIME/DOWN_TIME 未建模 | `init.c:1233,1244`；`utmp.rs` 枚举只有四类 | 覆盖缺口未披露 | 13 篇显式缺口（K-164） |
| G-04 | A-4 securelevel 实际部分落地 | `mib/subtree/kern.rs:158-162`；`walker.rs:623` | 旧文称"内核均无" | 12 篇改现状（K-146） |
| G-05 | A-7 构建变体为计划而非现状 | `Cargo.toml` 无 feature；C Makefile 有 `RESCUEDIR` 等 | 旧文按现状写 | 99 篇标注计划（K-191） |
| G-06 | 测试总数 143 vs 实测 136 | 13 处 §5.1 | 过期 | 各篇按模块实测；99 汇总（K-192） |
| G-07 | 模块数 19 vs 20 | 00 头块 | 措辞与文件数矛盾 | 00 修正 |
| G-08 | 信号数 8 vs 9 | `init.c:326-327` delset 放行 9 | 计数错 | 01/02 修正（K-014/K-027） |
| G-09 | Ctrl-Alt-Del/低电发送端 Rust 缺失、shutdown 命令缺失 | `os/drivers/tty/tty/src/keyboard.rs` 零命中；`os/commands/sbin/` 无 shutdown | 跨 stage 缺口未披露 | 14 篇"现状"节（K-178） |
| G-10 | `SessionDb` trait 接缝归属矛盾 | `host.rs:5-7` 注释 vs `session_db.rs` | 代码注释冲突 | 08 篇如实记录，挂 99 待办（K-099） |
| G-11 | `clean_ttys` 的 `_db` 参数未使用 | `clean_ttys.rs:30` | 未披露 | 10 篇一行（K-125） |
| G-12 | proctools 已有 utmp 解析器 | `os/commands/bin/proctools/src/utmp.rs` | 旧文称"无任何读者"过强 | 13 篇改现状（K-165） |
| G-13 | 02 篇 Markdown 结构缺陷 | 缺 `## 4.` H2；`### 3.5` 粘行 | 渲染错误 | B 相修正 |
| G-14 | "工具生成"锚 39 处系统性错配 | 8 篇头块/§7 均为"前一函数名 + 本函数行号" | 污染 | 全部重锚（§8.2） |

### 3.4 重复主题表

见 §2.3。高重复组：安全级（04/09/12）、collect_child（04/09/11）、utmp 调用点（05/07/08/09/11/13）、信号名单（01/02/14）、常量表（99 与各篇）、测试总数（03-14 互抄）。

### 3.5 越界主题表

| # | 旧位置 | 越界内容 | 正确归属 |
|---|--------|----------|----------|
| O-01 | 02 §3.5 | minixreboot fork/exec 机制展开 | 14；02 只留信号→状态 |
| O-02 | 02 §5 | shutdown 请求（14）与 utmp 边界（13）测试 | 各主篇 |
| O-03 | 04 §5 | 口令后端（password.rs）细节 | 12/实现备注；04 留决策 |
| O-04 | 05 §2.4 | utmp reboot 记录点行号 | 13；05 留调用一行 |
| O-05 | 08 头/§2 | utmp 挂钩重复 | 13 |
| O-06 | 09 §3/§4 | 安全级语义与签名现状 | 12（机制）；09 调用点 |
| O-07 | 10 §4 | rebuild_command（07 领域） | 07；10 只做 diff |
| O-08 | 11 §2/§4 | 台账与 utmp 关机记录 | 13/驱动；11 留一行 |
| O-09 | 12 §5 | 单用户降级测试（04 语义） | 12 保留机制侧，04 语义侧 |
| O-10 | 13 §4 | reboot/shutdown 台账调用点 | 驱动；13 留调用 |
| O-11 | 14 §1/§2.2 | 与 02 桥模型重叠、PM 细节展开 | 02；14 只给契约 |
| O-12 | 99 常量表 | 与各篇重复（总表定位，可接受） | 99 主家，各篇一行 |

### 3.6 非 C 主题逐项回答（固定清单）

| 主题 | 在哪里讲 | 依据 |
|------|----------|------|
| 链接与加载 | 不在本 stage：01 §boot 链 + 交叉引用 | VM `exec_bootproc` 加载 ELF（`vm/main.c:498-514`）；加载器归 01-stage-kernel/06、09 |
| 镜像与内存布局 | 07（`session_t` 内存模型）；99（常量容量） | `init.c:156-170`；无独立镜像 |
| 汇编入口与陷阱进入 | 不在本 stage（明示） | init 是普通用户程序，`main` 由 libc/PM exec 进入 |
| 启动装配 | 01（八步入口）＋14（boot argv/对端初始化） | `init.c:242-358`；`pm/main.c:188-204` |
| 构建与工具链 | 99（C Makefile 变体 `RESCUEDIR`/`LETS_GET_SMALL`/`SUPPORT_UTMP*`；Cargo 现状） | `sbin/init/Makefile`；`os/commands/sbin/init/Cargo.toml` |
| 跨模块接口与线格式 | 14（外部契约 13 项）、06（ttys 格式）、13（utmp/utmpx 格式）、07（argv 组装） | plan §5.3；各头文件 |
| 错误路径 | 03（日志三件套/disaster）、04（口令门）、05（rc 失败）、11（收工条件）、12（ENOENT 容忍） | 各函数锚 |
| 关闭与退出 | 11（catatonia/death）、14（reboot/powerdown/PM stop） | `init.c:1634-1698`；`pm/misc.c:223-224` |
| 并发与同步 | 99（单线程；信号 handler 内仅置标志，fatal 例外 `_exit`） | `init.c:504-511,1502-1522` |
| 测试基建 | 各篇 §5（按模块实测）；99（136 测试现状）；无 C 侧 init 单元测试（man/NOTES 是行为契约） | `os/commands/sbin/init/src` 实测；`init.8` |

---

## 4. 新目录

### 4.0 结构决策与理由

**结论：保留 00-14 + 99 的 16 篇编号与顺序；不新增、不合并、不拆分；全部原地重建（含 00/99 现状同步），并在 14 与 99 增加结构性新节。**

理由：

1. **顺序与状态机同构**：01（入口）→ 02/03（骨架与基础设施）→ 04-06（启动三状态 s/r/t）→ 07/08（会话模型与索引）→ 09（稳态 m）→ 10/11（T/c/d）→ 12-14（横切交互）→ 99。逐篇前置检查全部指向更早编号（§9.1）。
2. **缺陷是"实现先行、文档滞后"型**：`minix-init` 于 09-18 完成实体接线（20 文件 136 测试），文档中 12 处 Rust 标识符为虚构、测试数 143 全错、A-4/A-7/utmp 状态过期。重建即可。
3. **新增长点落位**：外部制品现状矩阵（G-09）进 14；ARCH 状态表/测试基线/常量落点/构建现状进 99；utmp 真值与缺口的修正进 13；A-4/A-5 现状进 12。
4. **已评估否决**：R-01 合并 04/05/06 为"启动三状态"一篇——三者各自有独立失败语义（口令门/rc 五归宿/DB 双路由），合并后 330+ 行且窗口期回滚语义丢失；R-02 拆 01——入口八步本身是一个连贯流程；R-03 为"Rust 接线现状"新建篇——内容是状态表，进 99；R-04 将 07/08 合并——结构与索引是值类型与容器两个层次，分开保持单一语义。

### 4.1 新篇章总表

| 组 | 编号 | 标题（新 H1） | 一句话定位 |
|----|------|---------------|-----------|
| 0 总览 | 00 | init 整体架构与 boot 链位置 | pid 1 是什么、状态机怎么读 |
| 1 入口 | 01 | 入口与进程身份 | 八步入口：身份、会话、设备兜底、参数、信号、探测 |
| 2 骨架 | 02 | 状态机骨架与信号转换 | 七状态、信号路由、主循环 |
| 2 | 03 | 日志三件套与致命信号 | stall/warning/emergency/disaster |
| 3 启动状态 | 04 | 单用户抢修态 | 口令门、ALTSHELL、五结局 |
| 3 | 05 | 运行启动脚本 | /etc/rc 黑盒契约与 chroot 二遍 |
| 3 | 06 | 读终端表与会话重建 | /etc/ttys 解析与 DB 失败路由 |
| 4 会话模型 | 07 | 会话结构与生命周期 | session_t、argv 组装、释放 |
| 4 | 08 | 会话数据库 | pid→会话索引（A-1 HashMap） |
| 5 稳态 | 09 | 多用户稳态 | window/getty 启动与 collect_child |
| 5 | 10 | 重读终端表 | PRESENT diff 与关停标记 |
| 6 关停 | 11 | 假死与关机 | catatonia 与 death 三轮 |
| 7 交互 | 12 | 安全级别与新根 | securelevel 与 init.root |
| 7 | 13 | 会话日志账本 | utmp/utmpx 记录与 runlevel |
| 7 | 14 | 对外契约与对端现状 | 13 项契约 + Rust 对端缺口 |
| 8 全局 | 99 | 全局概念与状态收口 | 常量、全局映射、ARCH、测试、构建 |

### 4.2 阅读路径

- **主线**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 99。
- **"开机到 login"路径**：01 → 02 → 05 → 06 → 07/08 → 09。
- **"抢修与恢复"路径**：04 → 11 → 09。
- **"信号驱动"路径**：02（路由表）→ 03（fatal）→ 10（HUP）→ 11（TERM/TSTP）→ 14（ABRT/USR1）。
- **对端读者路径**（PM/内核/MIB/驱动/commands）：14 → 13（utmp）→ 12（sysctl）→ 99（ARCH/测试现状）。
- **可跳读**：08（DB 仅一个索引）、12/13（横切交互）、99（工具篇）。

---

## 5. 每篇契约

共同纪律：C 锚点 `文件:行号/符号`（初值行以 `static` 行算，与 plan §5.2 口径一致）；Rust 锚点只用符号名 + 快照日期；不引用隐藏中间产物；每篇统一头部声明块（状态/定位/前置/边界/源码/Rust 模块）；"Rust 现状"句写入日 grep 复核。

### 00-init-overview

- 定位：init 是 pid 1 普通用户进程；状态机主线与阅读导航。
- 讲什么：K-001…K-006。重点：USR_F 身份、boot 链终点、七状态主线图、双驱动模型、Rust 三形态（InitHost/DriverState/ScriptHost）、边界。
- 不讲什么：机制细节（01-14）。
- 前置：无。后置：全部。
- 事实底线：`rs/table.c:28`、`kernel/table.c:64`、`init.c:133-139,624-640`；`host.rs`/`driver.rs` 符号；Rust 20 文件/136 测试实测。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-001 | init=普通用户进程（USR_F，非服务器） | 执行模型正名 |
| K-002 | boot 链终点（boot_image 最后一项） | 启动位置 |
| K-003 | 状态机主线七状态图 | 叙事主轴 |
| K-004 | 双驱动（waitpid + 信号）执行模型 | 与服务器对照 |
| K-005 | Rust 实现形态（InitHost 单接缝 / DriverState 单所有者 / ScriptHost） | 改写骨架 |
| K-006 | 导航与边界 | 阅读路线 |
- 验收标准：模块数按 20 个文件写；E-INITSYS ④ 状态改为已闭合（`pub use minix_sys::signal as sig`）；主线图与 02 一致；不再引用参考材料章节号。

### 01-init-main-entry

- 定位：main 八步入口与首状态决策。
- 讲什么：K-010…K-020。
- 不讲什么：状态函数体（04-11）、securelevel 机制（12）、信号 handler 语义（02/03/14）。
- 前置：00。后置：02-14。
- 事实底线：`init.c:229-358,1703-1788,287-303,314-334,544-563`；`entry.rs`/`main.rs`/`host.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-010 | 八步入口全景（身份→setsid→mfs_dev→getopt→信号→close/探测→transition） | 入口清单 |
| K-011 | 身份两行拒绝（uid/pid） | 双重出生证明 |
| K-012 | getopt "sf" 语义与边界 | 启动参数 |
| K-013 | mfs_dev 三层逻辑与 `#if 0` 死段 | 设备兜底 |
| K-014 | 信号注册调用点（10 信号/5 handler 组） | 信号名单 |
| K-015 | securelevel 探测调用点 | 移交 12 |
| K-016 | close 0/1/2 与不可达 return | 收尾 |
| K-017 | `BootArgs`/`parse_boot_args` 纯解析 | 边界可测 |
| K-018 | `EntryDecision`/`decide_entry` 首状态决策 | 决策显式 |
| K-019 | `console_present`/`ensure_console` seam | 副作用隔离 |
| K-020 | 身份校验现状（内联、uid ENOSYS 仅警告） | 真实装配 |
- 验收标准：八步对照表逐行；mfs_dev 死段标注；信号清单 10 信号/5 组；Rust 身份校验按内联现状（无虚构 `EntryError`）；entry.rs 12 测试逐名。

### 02-init-state-machine

- 定位：七状态骨架、信号注册与转换、主循环。
- 讲什么：K-025…K-034。
- 不讲什么：各状态体（04-11）、日志（03）、utmpx 挂钩（13）、重启挂钩（14）。
- 前置：01。后置：03-14。
- 事实底线：`init.c:130-139,369-405,624-640,1502-1522,1649-1655`；`state_machine.rs`/`signal_state.rs`/`driver.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-025 | `state_func_t` 双重函数指针类型 | 类型体操 |
| K-026 | 七状态字符大小写敏感（runlevel 协议） | 状态编码 |
| K-027 | `handle` 全集屏蔽 + SA_NOCLDSTOP；`delset` 放行 9 个 | 信号掩码 |
| K-028 | `transition` 主循环与空指针防御 | 主循环 |
| K-029 | 信号→状态三映射与 default 清零 | 转换表 |
| K-030 | `alrm_handler` 与 `clang` 计时 | death 倒计时 |
| K-031 | `StateKind` 枚举与往返转换 | 穷尽检查 |
| K-032 | `signal_to_state` 纯映射与从属号反查 | 可测映射 |
| K-033 | `DriverState` 切片（Ledger/ChildCollector/SignalState） | 单所有权 |
| K-034 | minix-sys 信号导出现状（`pub use minix_sys::signal as sig`） | E-INITSYS ④ 已闭 |
- 验收标准：补回缺失的 `## 4.` H2 与可见的 `### 3.5`；delset 9 信号；信号→状态三映射表；Rust 映射符号真实；测试 21（五文件）按模块列。

### 03-init-logging-failure

- 定位：失败路径的三级日志与致命信号。
- 讲什么：K-040…K-045。
- 不讲什么：状态转换（02）、重启挂钩（14）。
- 前置：02。后置：04-14。
- 事实底线：`init.c:95,411-432,440-511`；`log.rs`/`host.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-040 | `STALL_TIMEOUT=30` 读屏语义 | 停等动机 |
| K-041 | `stall`：日志 + closelog + sleep | 失败惯例 |
| K-042 | `warning`/`emergency` 级别分界 | 日志级别 |
| K-043 | `disaster` 遗言 + `_exit(sig)` | 崩溃重启触发 |
| K-044 | `print_console`/`badsys` 明确排除 | 阅读避坑 |
| K-045 | `Severity` 两级与 host 睡眠注入 | 可测输出 |
- 验收标准：头锚 `stall:440`（不再写成 `print_console`）；死代码/非 Minix 排除说明；disaster 与 `_exit(sig)` 的重启含义一句指 14；crate 数字按 136。

### 04-init-single-user

- 定位：'s' 状态：抢修 shell 的全流程。
- 讲什么：K-050…K-057。
- 不讲什么：安全级机制（12）、setctty/collect_child（09）、rc（05）。
- 前置：02、03。后置：05、09、12。
- 事实底线：`init.c:694-874`（降级 723-725、口令 747-763、ALTSHELL 768-783、看护 825-873）；`single_user.rs`/`password.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-050 | 退出即前进（FASTBOOT→runcom） | 状态语义 |
| K-051 | 父进程三段（准备/fork/看护） | 结构 |
| K-052 | 安全级先降 0 | 抢修可写 |
| K-053 | SECURE 口令门四条件与重试/^D | 条件信任 |
| K-054 | ALTSHELL 选择与双 exec 兜底 | shell 回退 |
| K-055 | 看护循环五结局 | wait 语义 |
| K-056 | `SingleUserOutcome` 四变体（旧文写五） | 结局建模 |
| K-057 | 口令循环内联现状（旧文引 `classify_attempt`） | 无虚构函数 |
- 验收标准：口令门四条件与 ^D/重试路径；ALTSHELL 双 exec 兜底；五结局；Rust 结局 4 变体（旧文 5 纠正）；删除虚构 `classify_attempt`/`classify_wait`。

### 05-init-runcom

- 定位：'r' 状态：/etc/rc 黑盒执行与 chroot 二遍。
- 讲什么：K-060…K-066。
- 不讲什么：shouldchroot 机制（12）、setctty/collect_child（09）、utmp（13）。
- 前置：04。后置：06、12、13。
- 事实底线：`init.c:879-1014`；`runcom.rs`/`driver.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-060 | 只看退出码的黑盒契约 | rc 可演化 |
| K-061 | autoboot/fastboot 参数 | 唯一传参 |
| K-062 | 子进程六步与两个失败码 | 失败路由 |
| K-063 | 五归宿表（含 catatonia+SIGTERM 双条件） | wait 语义 |
| K-064 | 两次执行与 `did_multiuser_chroot` | chroot 二遍 |
| K-065 | `RuncomResult` 三变体（旧文引 `RcOutcome`） | 结局建模 |
| K-066 | 台账交接（`ledger.reboot`，旧文引 `deps.record_reboot`） | 单所有者 |
- 验收标准：argv 两参数（autoboot/fastboot）；子进程六步与 `_exit(4/5)`；五归宿；两遍执行与 `did_multiuser_chroot`；Rust 结局 `RuncomResult`/台账 `ledger.reboot` 真实符号。

### 06-init-read-ttys

- 定位：'t' 状态：/etc/ttys 解析与会话链表重建。
- 讲什么：K-070…K-077。
- 不讲什么：session_t 结构（07）、DB 实现（08）、utmp（13）、chroot 机制（12）。
- 前置：05。后置：07、08。
- 事实底线：`init.c:1222-1285,1792-1806,1147-1149`；`ttyent.h:40,57-58`；`getttyent.c:129-188`；`ttys.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-070 | ttys 四列表与 TTY_ON/SECURE | 登录开关 |
| K-071 | 先清空再重建策略 | 重读语义 |
| K-072 | read_ttys 六步与 DB 失败双路由 | 降级路径 |
| K-073 | chroot 感知路径（`do_setttyent`） | 路径拼接 |
| K-074 | new_session 三条件过滤 | 开/关行 |
| K-075 | A-6 原生解析器替代 getttyent | 无 libc 依赖 |
| K-076 | 引号/quoted 空 getty/精确 token/off 后赢/转义引号 | 解析语义 |
| K-077 | 已知边界（续行反斜杠未实现） | 诚实缺口 |
- 验收标准：头锚 `read_ttys:1222`；`_PATH_TTYS` 仅指 `ttyent.h:40`；解析语义（quoted/off/转义/token 精确）与 libc 对照；DB 失败双路由；16 测试逐名。

### 07-init-session-model

- 定位：session_t 值类型与 argv 生命周期。
- 讲什么：K-085…K-092。
- 不讲什么：DB 增删查（08）、getty 启动（09）、utmp（13）。
- 前置：06。后置：08、09、13。
- 事实底线：`init.c:156-170,1101-1217,1123-1180`；`session.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-085 | `session_t` 全字段与 `SE_*` 标志 | 内存模型 |
| K-086 | `se_started` 防抖动用途 | 与 09 衔接 |
| K-087 | new_session 四关 | 失败路径 |
| K-088 | `device="/dev/"+name` | 路径拼接 |
| K-089 | setupargv 两段分词与 `construct_argv` 空格制表切分 | argv 来源 |
| K-090 | free_session 释放顺序 | 内存管理 |
| K-091 | 链表指针不入节点/值类型 RAII | 避免侵入式别名 |
| K-092 | `ParsedCommand` 两形状（exec_path 与 argv） | exec 路径 ≠ argv0 |
- 验收标准：字段全表与 `SE_*`；四关失败路径；分词与 argv0 两形状；`BuildReject` 单变体如实；6 测试逐名。

### 08-init-session-db

- 定位：pid→会话索引：Berkeley DB → HashMap。
- 讲什么：K-095…K-100。
- 不讲什么：会话生命周期（07/09）、utmp 挂钩执行（13）。
- 前置：07。后置：09。
- 事实底线：`init.c:1021-1096`；`session_db.rs`/`host.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-095 | pid→会话反向索引（waitpid 只有 pid 的困境） | 数据必要性 |
| K-096 | `dbopen(NULL,HASH)` 内存表（无持久化） | 语义边界 |
| K-097 | start/add/del/find 四原语（DB 未开静默） | DB 全貌 |
| K-098 | A-1：Berkeley DB → `HashMap` | 无 db 依赖 |
| K-099 | `SessionDb` trait 是接缝外唯一 trait | 接缝边界 |
| K-100 | 值语义（索引 `copied()`，非指针 memmove） | 现状 |
- 验收标准：四原语与"DB 未开静默"；值语义（索引非指针）；`SessionDb` trait 接缝归属矛盾如实记录并挂 99；5 测试逐名。

### 09-init-multi-user

- 定位：'m' 稳态：窗口/getty 启动与回收循环。
- 讲什么：K-105…K-113。
- 不讲什么：utmp 记录（13）、chroot 机制（12）、会话结构（07）。
- 前置：07、08。后置：10、11、13。
- 事实底线：`init.c:1290-1370,1460-1497,1528-1564,669-689`；`multi_user.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-105 | 稳态=全启动 + waitpid 回收循环 | 稳态定义 |
| K-106 | 安全级 0→1（-1 别动） | 升级语义 |
| K-107 | `start_window_system` 先于 getty，失败 `_exit(6)` | 启动顺序 |
| K-108 | `start_getty` 四步（chroot→防抖→window→exec） | getty 启动 |
| K-109 | `setctty`（setsid+DTR+login_tty） | 控制终端 |
| K-110 | `collect_child` 三处理（忽略/重启/移除） | 回收 |
| K-111 | 三时间常量 `GETTY_SPACING/SLEEP/DTRTIME` | 防抖动参数 |
| K-112 | `getty_delay_secs`/`CollectAction` 现状（分类器已删） | 无虚构函数 |
| K-113 | `multi_user(host, collector, signals)` 真实签名 | 装配形状 |
- 验收标准：window 先于 getty 与 `_exit(6)`；防抖动 sleep 在子进程内（拖 exec 不拖父）；安全级 `==0` 升 1；三处理回收；真实签名 `multi_user(host, collector, signals)`；9 测试逐名。

### 10-init-clean-ttys

- 定位：'T' 状态：/etc/ttys 变化 diff 与关停标记。
- 讲什么：K-120…K-125。
- 不讲什么：new_session/setupargv 机制（07）、回收执行（09）。
- 前置：07、09。后置：09。
- 事实底线：`init.c:1567-1629`；`clean_ttys.rs:30` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-120 | PRESENT 标记 diff（还在/新来/消失） | 重读算法 |
| K-121 | n² 算法注释（重读少发生） | 简单正确优先 |
| K-122 | 清标记→逐行匹配→收尾关停三步 | 流程 |
| K-123 | 序号变化 warning 与解析失败挂 SIGHUP | 行为细节 |
| K-124 | `diff_line` 分类器已删（旧文引） | 无虚构函数 |
| K-125 | `_db` 参数未使用现状 | 诚实缺口 |
- 验收标准：三步（清标记/逐行匹配/收尾关停）；序号变化 warning 与失败挂 SIGHUP；删除虚构 `diff_line`；`_db` 未使用如实；6 测试逐名。

### 11-init-shutdown

- 定位：'c' 假死与 'd' 真死。
- 讲什么：K-130…K-136。
- 不讲什么：utmp 关机记录（13）、PM 重启路径（14）。
- 前置：02、09。后置：14。
- 事实底线：`init.c:96,1634-1698,1667`；`shutdown.rs:35`、`driver.rs:188` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-130 | catatonia=关登录（标全部 SHUTDOWN） | 假死语义 |
| K-131 | death 三轮升级（HUP→TERM→KILL） | 真死语义 |
| K-132 | `DEATH_WATCH=10` 与 `clang`/alarm 计时 | 计时 |
| K-133 | `kill(-1)` ESRCH 与 ECHILD 提前返回 | 收工条件 |
| K-134 | `ps axl` warning（三轮杀不完） | 失败告警 |
| K-135 | `DEATH_SEQUENCE` 与 `&AtomicBool` 现状（旧文引 `Arc<AlarmFlag>`） | 装配形状 |
| K-136 | 台账入口 `ledger.shutdown`（旧文引 `deps.record_shutdown`） | 单所有者 |
- 验收标准：三轮与两提前返回；`&AtomicBool` 现状（非 `Arc<AlarmFlag>`）；台账入口真实；5 测试逐名。

### 12-init-sysctl-interaction

- 定位：securelevel 与 init.root 两个运行时旋钮。
- 讲什么：K-140…K-148。
- 不讲什么：内核/MIB 实现（01/10-stage）；调用方状态语义（04/05/09）。
- 前置：01、04。后置：99。
- 事实底线：`init.c:544-618,1811-1900`；`mib/subtree/kern.rs:158-162`、`walker.rs:623`；`sysctl.rs`/`host.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-140 | securelevel 写保护档（单用户 0/多用户 1） | 两个旋钮之一 |
| K-141 | init.root 第二根（rc 在新根第二遍） | 另一个旋钮 |
| K-142 | `has_securelevel` 探测（ENOENT→0） | 能力探测 |
| K-143 | `getsecuritylevel`/`setsecuritylevel` 语义 | 查询/设置 |
| K-144 | `createsysctlnode`（init/root，默认 `/`） | 节点创建 |
| K-145 | `shouldchroot`（ENOENT 重建/非串拒绝） | chroot 决策 |
| K-146 | A-4 状态修正：MIB 已有 `KERN_SECURELVL`（部分落地） | 对端现状 |
| K-147 | A-5 init.root 完全缺失（init 侧 ENOSYS） | 缺口语义 |
| K-148 | `should_chroot` 非空判断（Rust 简化） | 决策差异 |
- 验收标准：A-4 改为"MIB 部分落地 + init 侧 ENOSYS"；A-5 缺口语义；`should_chroot` 简化点；4 测试逐名。

### 13-init-utmp

- 定位：who/last 的账本：记录构造、runlevel、注销。
- 讲什么：K-155…K-165。
- 不讲什么：libc 文件格式实现（14/18-stage）；reboot/shutdown 台账调用点（驱动/05/11）。
- 前置：02、07。后置：14。
- 事实底线：`init.c:647-666,1372-1458`；`utmpx.h:64`；`utmp.rs`/`host.rs:333` 符号；`proctools/src/utmp.rs`。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-155 | who/last 的数据来源（LOGIN/DEAD/RUN_LVL/BOOT/SHUTDOWN） | 账本用途 |
| K-156 | `session_utmpx` 三选一（getty/window/空） | 记录构造 |
| K-157 | `make_utmpx`（零化/拷名/ut_id 取行尾） | 字段填充 |
| K-158 | `get_runlevel` 七状态映射与未知 DEATH | runlevel |
| K-159 | `utmpx_set_runlevel` 空会话短路 | /var 未可写 |
| K-160 | `clear_session_logs` 双通道注销 | 清理 |
| K-161 | SUPPORT_UTMP/UTMPX 双开（Makefile:8） | 构建事实 |
| K-162 | A-2 缺口：Rust 写路径 ENOSYS + 文本临时编码 | 现状 |
| K-163 | `RecordType::Dead=7` 与 C `DEAD_PROCESS=8` 不一致 | 真值 bug |
| K-164 | BOOT_TIME/DOWN_TIME 记录未建模 | 覆盖缺口 |
| K-165 | proctools 已有 compat utmp 解析器（旧文称"无读者"过强） | 现状 |
- 验收标准：`RecordType::Dead` 与 C 值不一致必须显式（不得背书 7）；BOOT_TIME/DOWN_TIME 缺口显式；A-2 写路径 ENOSYS/文本临时态；proctools 解析器现状；9 测试逐名。

### 14-init-external-contracts

- 定位：13 项外部契约与 Rust 对端现状。
- 讲什么：K-170…K-179。
- 不讲什么：对端服务实现（各 stage）。
- 前置：01、02、09、11。后置：99。
- 事实底线：plan §5.3 全锚；`os/commands/sbin`；`keyboard.rs`；`tps65217`；`contracts.rs`/`host.rs` 符号。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-170 | USR_F 身份与 procfs `service_active` 不算系统服务 | 身份契约 |
| K-171 | `minixreboot`/`minixpowerdown` fork+exec shutdown | 重启路径 |
| K-172 | boot 映像最后一项与 boot argv | 启动契约 |
| K-173 | PM：INIT 父=自身/INIT_PID=1/scheduler | 身份契约 |
| K-174 | 孤儿收养与 INIT 死亡只栈回溯 | 生命周期 |
| K-175 | reboot 路径 `sys_stop(INIT)` | 关机协同 |
| K-176 | 调度继承与 RS 父行 | 继承契约 |
| K-177 | Ctrl-Alt-Del→SIGABRT / 低电→SIGUSR1 | 两驱动通知 |
| K-178 | Rust 现状：shutdown 命令缺失、两发送端缺失、请求未消费 | 跨 stage 缺口 |
| K-179 | `request_for`/`shutdown_argv` 纯映射（无静态契约表） | 真实形态 |
- 验收标准：13 项契约逐条带对端锚；Rust 现状矩阵三行（shutdown 命令缺/两发送端缺/请求无消费者）；`request_for` 等真实符号；8 测试逐名。

### 99-init-global-concepts

- 定位：常量、路径、全局映射与 ARCH/测试/构建收口。
- 讲什么：K-185…K-193。
- 不讲什么：机制流程（一行定义 + 主篇）。
- 前置：00。后置：无。
- 事实底线：`init.c:92-98,133-139,151-211`；`paths.h:62`、`ttyent.h:40`、`pathnames.h:39-40`；`Cargo.toml`；实测 136。
- 知识点清单：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-185 | 七状态字符/超时族/DTR 常量表 | 常量单点 |
| K-186 | 路径族（rc/ttys/console/utmpx/utmp）与权威头 | 路径真值 |
| K-187 | `_PATH_SLOGGER` 死常量 | 排除项 |
| K-188 | `INIT_PID 1` 与内核/MIB 端点 | 身份常量 |
| K-189 | C 全局→单所有者视图映射（11 项） | A-4 依据 |
| K-190 | ARCH A-1…A-12 状态总表 | 决策导航 |
| K-191 | A-7 构建变体为计划而非现状（Cargo 无 feature） | 现状诚实 |
| K-192 | 测试基线 136 与模块分布 | 验证边界 |
| K-193 | 常量→Rust 标识符落点映射 | 落点索引 |
- 验收标准：路径每行单一权威头；A-7 标注计划非现状；ARCH 状态表逐项带符号锚；测试分布按 20 文件实测；全局映射 11 项与 `driver.rs` 对应。

---

## 6. 变更表

### 6.1 操作总表

| 操作编号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|----------|------|--------|--------|------|-----------|------|
| OP-01 | 重锚（系统性） | 8 篇头块与 §7 的 39 处"工具生成"锚 | 各篇按实测函数锚 | 符号取前一函数、行号取本函数 | K-010…K-179 | 全部重锚 |
| OP-02 | 纠错 | 01 §3.3/§4.2/§5（虚构标识符/13 测试） | 01 现状 | crate 无 `EntryError` 等 | K-018/K-020 | 原地 |
| OP-03 | 纠错 | 02 §2.3/§3.3/§4.3/结构缺陷 | 02 现状 + 补 H2 | delset 9；minix-sys 已导出；无 `to_state_kind` | K-027/K-034 | 原地 |
| OP-04 | 纠错 | 03 头锚/§5.1 | 03 现状 | `print_console` 误锚 | K-041/K-044 | 原地 |
| OP-05 | 纠错 | 04 §3.3/§4/§5 锚 | 04 现状 | 4 变体；内联口令；锚 723-725 | K-052/K-056/K-057 | 原地 |
| OP-06 | 纠错 | 05 §3/§4/§5 锚 | 05 现状 | `RuncomResult`/`ledger.reboot` | K-065/K-066 | 原地 |
| OP-07 | 纠错 | 06 头锚/§7/§5 锚 | 06 现状 | `_PATH_TTYS` 真值 | K-073/K-075 | 原地 |
| OP-08 | 纠错 | 08 §3/§5.1 | 08 现状 | `.copied()`；trait 矛盾记录 | K-098/K-099/K-100 | 原地 |
| OP-09 | 纠错 | 09 §3/§4/§5 锚 | 09 现状 | 分类器已删；真实签名 | K-112/K-113 | 原地 |
| OP-10 | 纠错 | 10 §3/§5 锚 | 10 现状 | `diff_line` 已删；new_session 1616 | K-124 | 原地 |
| OP-11 | 纠错 | 11 §3/§4/§5 | 11 现状 | `AtomicBool`；`ledger.shutdown` | K-135/K-136 | 原地 |
| OP-12 | 纠错 | 12 §1/§3 | 12 现状 | A-4 MIB 部分落地 | K-146 | 原地 |
| OP-13 | 纠错 | 13 §3/§4/§5/§7 | 13 现状 | Dead 值/记录类型缺口/台账 | K-163/K-164 | 原地 |
| OP-14 | 纠错 | 14 §1/§3/§4/§5 | 14 现状 + 对端缺口矩阵 | Rust 发送端/命令缺失 | K-178/K-179 | 原地 |
| OP-15 | 新增 | — | 14 §对端现状矩阵 | 三处跨 stage 缺口 | K-178 | 新节 |
| OP-16 | 新增 | — | 99 §ARCH 状态/测试/构建/常量落点 | 全局收口 | K-190/K-191/K-192/K-193 | 新节 |
| OP-17 | 纠错 | 99 核心点/常量表 | 99 现状 | A-7 计划；`_PATH_TTYS` 真值；`GETTY_*` 名 | K-186/K-191 | 原地 |
| OP-18 | 纠错 | 00 头/形态节 | 00 现状 | 20 文件；E-INITSYS ④ 已闭 | K-005 | 原地 |
| OP-19 | 计数修正 | 03-14 §5.1（143×13 处） | 各篇按模块实测（136） | 过期 | K-192 | 各篇 |
| OP-20 | 边界收束 | 02 §3.5/§5、05 §2.4、10 §4、11 §4、13 §4 | 14/13/07/驱动 | §3.5 越界表 O-01…O-10 | — | 各主家 |

### 6.2 明确删除项

无知识点删除。删除的表述：① 12 处虚构 Rust 标识符（OP-02/03/05/06/09/10/11/14）；② "143 测试"（OP-19）；③ "内核均无 securelevel"（OP-12）；④ A-7 按现状的表述（OP-17）；⑤ "18 类模块 19 文件"（OP-18）；⑥ 13 篇对 `RecordType::Dead=7` 的背书（OP-13）。

---

## 7. 缺漏新篇

不新建篇章；缺漏落实为已有篇新节：

| # | 主题 | 为什么重要 | 原料 | 落点 | 验收 |
|---|------|-----------|------|------|------|
| N-01 | utmp 真值与覆盖缺口 | 代码 bug 不得被文档背书 | `utmp.rs:45`；`utmpx.h:64`；`init.c:1233,1244` | 13 | Dead 值显式；两记录类型缺口显式 |
| N-02 | A-4/A-5 对端现状 | 读者需要知道哪个旋钮通 | `mib/subtree/kern.rs:158-162`；`host.rs:228-238` | 12 + 99 | 两行现状带锚 |
| N-03 | Rust 对端缺口矩阵 | shutdown 命令/两发送端缺失会误导 | `os/commands/sbin`；`keyboard.rs`；`tps65217` | 14 | 三行带证据 |
| N-04 | 接缝边界矛盾 | `SessionDb` 归属两说 | `host.rs:5-7`；`session_db.rs` | 08 + 99 | 矛盾如实记录并挂待办 |
| N-05 | 测试基线 | 13 处数字互抄且全错 | 实测 136 | 各篇 + 99 | 每篇只报本模块 |
| N-06 | 构建现状 | A-7 被当现状 | `Cargo.toml`；`sbin/init/Makefile` | 99 | 计划/现状分栏 |
| N-07 | 常量落点映射 | 常量散在业务模块 | `init.c:92-98` → `multi_user.rs:23-25` 等 | 99 | 每行给 C 锚与 Rust 符号 |
| N-08 | `clean_ttys._db` 未用 | 诚实缺口 | `clean_ttys.rs:30` | 10 | 一行 |
| N-09 | proctools 解析器 | 旧文否定过强 | `proctools/src/utmp.rs` | 13 | 现状一行 |
| N-10 | init 与 man/NOTES 的 POSIX 职责 | 行为契约的第三来源 | `init.8`、`NOTES` | 02 | job control/孤儿/控制终端三条 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

本阶段无编号变化；16 篇原地重建。发生节级搬家/压缩/新增的位置：

| 旧位置 | 旧内容 | 新位置 | 类型 |
|--------|--------|--------|------|
| 01 §3.3/§3.4 | 虚构标识符叙述 | 01（真实内联/符号） | 改写 |
| 02 §3.5 | reboot fork/exec 机制 | 14；02 留信号→状态 | 压缩 |
| 03 §2.4 尾句 | PM 重启语义 | 14 | 压缩 |
| 05 §2.4 utmp 行 | reboot 记录点 | 13 | 压缩 |
| 08 头/§2 utmp 挂钩 | 重复 | 13；08 留调用一行 | 压缩 |
| 09 §3 安全级 | 机制 | 12；09 调用点 | 压缩 |
| 10 §4 rebuild_command | 07 领域 | 07；10 只做 diff | 压缩 |
| 11 §4 台账/utmp | 驱动/13 领域 | 各主家 | 压缩 |
| 12 §5 单用户测试 | 04 语义 | 12 保留机制侧 | 保留 |
| 13 §4 台账调用 | 驱动调用点 | 13 留调用一行 | 压缩 |
| 14 §1/§2.2 | 与 02 重叠的桥模型 | 02；14 契约 | 压缩 |
| 99 常量表 | 与各篇重复 | 99 主家；各篇一行 | 保留 |
| 各篇 §5.1 总数 | 143 | 各模块实测 | 改写 |

### 8.2 引用迁移表

| 旧引用 | 位置（实测） | 新目标 | 验证 |
|--------|--------------|--------|------|
| `NN-init-*.md` 互引 | stage 内 45 处 | 编号不变；B 相重生成 | `rg -no '[0-9]{2}-init-[a-z-]+\.md'` 对表 |
| `09-stage-init` 目录引用 | 其它位置 1 处 | 目录名不变，零改动 | `rg -n '09-stage-init' notes/ os/` |
| "（工具生成）"占位锚 | 39 处 | 重锚/删除 | `rg -c '工具生成' 0*.md 1*.md 99*.md` 归零 |
| `.design/` 引用 | 0 处（文档） | 保持零（`entry.rs` 代码注释 1 处另清，归 Rust 卫生） | 零命中 |
| 代码引用文档名 | 0 处 | 无需迁移 | `rg -n '0[0-9]-init-' os/commands/sbin/init` |

### 8.3 断链成本摘要

- 编号变化：0 处；内容搬迁：13 处（§8.1）。
- 卫生清理：占位锚 39 处、测试总数 13 处、虚构标识符 12 处、状态过期 6 类。
- 热点文件：`01`（虚构标识符最多）、`02`（结构缺陷 + 状态过期）、`13`（真值 bug）、`14`（对端缺口）、`99`（收口新节）。
- 批量方式：B 相按契约重写；机械检查 `rg -n '工具生成|143 个|EntryError|classify_|RcOutcome|record_reboot|record_shutdown|AlarmFlag' 09-stage-init/*.md` 归零。

---

## 9. 验证与自检门

### 9.1 四种机械检查

**检查一：前向引用扫描**

| 新篇 | 前置 | 新篇 | 前置 |
|------|------|------|------|
| 00 | 无 | 08 | 07 |
| 01 | 00 | 09 | 07、08 |
| 02 | 01 | 10 | 07、09 |
| 03 | 02 | 11 | 02、09 |
| 04 | 02、03 | 12 | 01、04 |
| 05 | 04 | 13 | 02、07 |
| 06 | 05 | 14 | 01、02、09、11 |
| 07 | 06 | 99 | 00 |

全部指向更小编号。**通过。**

**检查二：依赖图**：约 25 条边全部小号→大号；12/13/14 并列收口。DAG 无环。**通过。**

**检查三：覆盖率**：131 条知识点全部有归属；无删除项；新增 12 条带锚。**通过。**

**检查四：断链成本**：见 §8.3。**通过。**

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|----|------|------|
| G1 C 真序可核对 | 通过 | 抽十：S01 `table.c:64`、S05 `init.c:242-249`、S07 `init.c:1703-1788`、S09 `init.c:369-405`、S11 `init.c:624`、's' `init.c:694-874`、'r' `init.c:879-1014`、't' `init.c:1222-1285`、'm' `init.c:1528-1564`、'd' `init.c:1661-1698` |
| G2 池完整 | 通过 | 46 函数全落篇（plan §5.2 对表）；头文件/man/NOTES/Makefile/对端 13 契约全有归属；排除项（print_console/badsys/死常量）入 99 |
| G3 前向引用为零 | 通过 | §9.1 检查一 |
| G4 依赖图无环 | 通过 | §9.1 检查二 |
| G5 覆盖率 100% | 通过 | 131/131；新增 12 带锚；删除项为空 |
| G6 双方向核对（抽十） | 通过 | OP-01/02/05（重锚与纠错去向）、OP-15/16（新增来源=实测与对端）、OP-20（边界去向）等十处逐条有去向或来源 |
| G7 契约要素齐全 | 通过 | 17 个契约（16 篇 + 99）均含定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单/验收 |
| G8 锚点与引用迁移覆盖 | 通过 | §8.1 十三处；§8.2 覆盖 45 处互引、39 占位、1 目录引用 |
| G9 事实锚与推测标注 | 通过 | 抽查：`kernel/table.c:64`、`rs/table.c:28`、`pm/const.h:9`、`pm/main.c:188-204`、`init.c:133-139`、`init.c:326-327`（9 信号）、`init.c:1667`（三轮）、`utmpx.h:64`（DEAD=8）、`mib/subtree/kern.rs:158-162`、实测 136 测试。无未标注推测；Rust 标识符与数字均按写入日实测复核 |

### 9.3 结论与待用户裁决

**结论**：蓝图完成（131 条知识点全有去向，16 篇契约齐全，G1-G9 全通过）。重建方案：16 篇编号与顺序不动，全部原地重建；14 增对端现状矩阵，99 增 ARCH/测试/构建/常量落点收口；无新增/合并/拆分。

**待裁决**：
1. utmp `RecordType::Dead=7` 与 C `DEAD_PROCESS=8` 的差异，按"文档记录 C 真值 + Rust 偏离台账"处理是否确认（涉及是否顺手修 Rust 常量，修属 B 相范围外）；
2. BOOT_TIME/DOWN_TIME 记录是否要求 Rust 侧补齐（本蓝图只要求文档披露缺口，补齐属实现任务）；
3. A-7 构建变体是否保留为计划（本蓝图取向）还是要求删除；
4. 12 处虚构 Rust 标识符是否需要在契约中同时给"替代实现位置"（本蓝图已给符号，B 相照写）。

**未完成声明**：无。

