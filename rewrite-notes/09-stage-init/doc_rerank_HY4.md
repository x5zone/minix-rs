# 09-stage-init 文档重建蓝图（HY4）

## 执行头部

```text
your_name(AI agent name) = HY4
target_dir(关注的工作目录) = 09-stage-init
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 09-stage-init/doc_rerank_HY4.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _HY4 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

---

## 0. 元数据

| 项 | 值 |
|---|---|
| 执行者 | HY4 |
| 日期 | 2026-09-19 |
| 目标目录 | `notes/rewrite/fork-syscall-rewrite/09-stage-init/` |
| 仓库根 | `/home/xzhao/github/minix-rs` |
| 当前提交 | `ebc8ae72b3a7c8cc1a7bb9ac1eecff218401c626`（branch `rewrite`） |

### 0.1 审查范围

- **文档（重建对象，16 篇）**：`00-init-overview.md`、`01-init-main-entry.md`、
  `02-init-state-machine.md`、`03-init-logging-failure.md`、`04-init-single-user.md`、
  `05-init-runcom.md`、`06-init-read-ttys.md`、`07-init-session-model.md`、
  `08-init-session-db.md`、`09-init-multi-user.md`、`10-init-clean-ttys.md`、
  `11-init-shutdown.md`、`12-init-sysctl-interaction.md`、`13-init-utmp.md`、
  `14-init-external-contracts.md`、`99-init-global-concepts.md`。
- **参考材料（不重建，只作知识来源与线索）**：`README.md`、`plan.md`、`todo.md`、
  `draft/`（旧占位）、`archive/todo-V1-archive-2026-09-18.md`。
- **中间产物（按项目规范不引用内容）**：本目录 `.design/`；仓库 `tmp_design_and_todo/`。
  本蓝图仅在 §8 断链统计中登记"代码注释里的设计契约锚点字符串"这一事实，不读取其内容。
- **范围外**：其它 stage 目录（仅读 `00-master-plan/README.md`、`08-stage-is/00-is-overview.md`、
  `14-stage-runtime/02-crt0-start.md`、`18-stage-commands/01-init-rc-scripts.md` 作边界判定）。

### 0.2 读取清单

| 类别 | 已读内容 |
|---|---|
| C 源码 | `minix3/sbin/init/init.c`（1902 行，全文逐段精读）、`minix3/sbin/init/pathnames.h`（40 行）、`minix3/sbin/init/Makefile`（20 行）、`minix3/sbin/init/NOTES`（119 行）、`minix3/sbin/init/init.8`（390 行） |
| C 头文件 | `minix3/include/ttyent.h`、`minix3/include/utmpx.h`（常量段）、`minix3/lib/libc/gen/getttyent.c`（行数据，字段语义见旧 doc 06 转述 + 本文核对） |
| 外部对端（C） | `minix3/minix/kernel/table.c:64`、`minix3/minix/servers/rs/table.c:28`、`minix3/minix/servers/vm/main.c:331-370,514`、`minix3/minix/servers/pm/main.c:188-204`、`minix3/minix/servers/pm/forkexit.c:336,396`、`minix3/minix/fs/procfs/service.c:195-207`、`minix3/minix/drivers/tty/tty/arch/i386/keyboard.c:300`、`minix3/minix/drivers/power/tps65217/tps65217.c:226` |
| 配置/脚本制品 | `minix3/etc/ttys`（真实数据）、`minix3/etc/rc`（468 行，头部）、`minix3/sbin/shutdown/`（存在）、`minix3/libexec/getty/`（存在） |
| Rust 实现 | `os/commands/sbin/init/src/` 全部 20 个文件（`main.rs` 全文、`host.rs` 公开面、`driver.rs` 公开面与测试名、其余按旧文档转述核对）、`Cargo.toml` |
| Rust 共享库 | `os/libs/minix-types/src/types/signal.rs`（信号编号权威） |
| 边界材料 | `00-master-plan/README.md`、`08-stage-is/00-is-overview.md`、`14-stage-runtime/02-crt0-start.md`、`18-stage-commands/01-init-rc-scripts.md`、`09-stage-init/plan.md`、`09-stage-init/todo.md` |

### 0.3 使用的命令与关键输出（证据摘录）

```bash
# 文档清单与体量
$ wc -l 09-stage-init/*.md
25 00 · 251 01 · 196 02 · 150 03 · 128 04 · 103 05 · 88 06 · 88 07
75 08 · 79 09 · 74 10 · 72 11 · 72 12 · 77 13 · 96 14 · 40 99

# 交叉引用统计（编号文档内部，文件名形式）
$ rg -o "\b(0[0-9]|1[0-4]|99)-init-[a-z-]+\.md" *.md | wc -l   → 111（含 plan/README）
编号文档内部：45 处；README.md：15；plan.md：35。

# 代码注释里的设计契约锚点（NN 为旧文档编号）
$ git grep -n "\.md" -- os/commands/sbin/init/   → 18 处（17 个模块 + host.rs 一处 plan.md 提及）

# 实测测试基线（本轮）
$ cargo test -p minix-init   → 135 passed; 1 failed; 0 ignored
  失败：host::tests::test_minix_host_honest_enosys_for_missing_wrappers
      left: Err(Errno(5))   right: Ok(())   （host.rs:651，register_handlers）
$ rg -c "fn test_" os/commands/sbin/init/src/*.rs | sum → 136 个测试函数

# 外部契约锚点核对
$ rg -n "INIT_PROC_NR" minix3/minix/kernel/table.c            → 64: {INIT_PROC_NR, "init"}
$ rg -n "INIT_PROC_NR" minix3/minix/servers/rs/table.c        → 28: {INIT_PROC_NR, "init", USR_F}
$ sed -n '345p' minix3/minix/servers/vm/main.c                → char *argv[] = { ip->proc_name, NULL };
$ rg -n "exec_bootproc" minix3/minix/servers/vm/main.c        → 331（定义）/ 514（调用）
$ rg -n "sys_kill(INIT_PROC_NR, SIGABRT)" .../keyboard.c      → 300
$ rg -n "kill(1, SIGUSR1)" .../tps65217.c                     → 226
$ rg -n "INIT_PROC_NR" minix3/minix/servers/pm/forkexit.c     → 336, 396
$ rg -n "INIT_PROC_NR|INIT_PID" minix3/minix/servers/pm/main.c→ 188, 194, 195, 203, 204
$ rg -n "INIT_PROC_NR" minix3/minix/fs/procfs/service.c       → 204（service_active 内）

# init.c 的 _exit 编码全表（16 处）
$ rg -n "_exit\(" minix3/sbin/init/init.c
510(_exit(sig)) 523(1) 536(1) 683(1) 687(2) 756(0) 810(3) 909(4) 915(5)
1315(6) 1346(7) 1368(8) 1725(9) 1767(10) 1784(11) 1787(12)

# 信号编号权威（os/libs/minix-types/src/types/signal.rs 注释锚点）
SIGHUP=1 SIGABRT=6 SIGBUS=10 SIGALRM=14 SIGTERM=15 SIGTSTP=18 SIGUSR1=30
```

> **事实核对提示（B 相必读）**：旧文档 12 篇的 §5.1 统一写"143 个通过（全 crate 口径），
> 0 失败"，本轮实测为 **135 passed / 1 failed**（见 §0.3）。重建后所有"测试统计"一律
> 以实测命令输出为准，禁止转述数字。

---

## 1. C 真序（运行时真序重建）

### 1.1 阶段类型判定

init 同时具备两种形态，本蓝图**按"启动链型为主干、稳态转事件循环"**处理：

- 它是 boot 链的**终点**（`kernel/table.c:64` 最后一项，VM `exec_bootproc` 加载，
  `vm/main.c:514`），`main()` 是一次性的线性出生流程 → **启动链型**。
- `transition()`（`init.c:624-640`）之后进入"运行当前状态函数 → 取下一状态 → 再运行"
  的无限循环，稳态 `multi_user`（`init.c:1559-1561`）是 `waitpid(-1)` 阻塞 + 信号唤醒
  → **事件循环型**（不是 IPC 事件循环，是 wait/信号事件循环）。

理由：主干的因果链是"boot 链如何把系统带到可登录状态"，这决定教学序；稳态事件循环
只是主干的最后一站，不另立类型。

### 1.2 真序表（逐条可核对）

**B 段：boot 期（非 init 代码，但决定 init 的出生条件）**

| # | 动作 | 锚点 | 说明 |
|---|---|---|---|
| B1 | kernel 登记 init 为 boot_image 最后一项 | `minix3/minix/kernel/table.c:64` | 模块槽位顺序的最后一位 |
| B2 | RS 登记表把 init 标为 `USR_F` 普通用户进程 | `minix3/minix/servers/rs/table.c:28` | 不是系统服务 |
| B3 | VM 为 boot 进程加载 ELF | `minix3/minix/servers/vm/main.c:331,514` | `exec_bootproc` |
| B4 | init 的 argv 固定为 `{proc_name, NULL}` = `{"init", NULL}` | `minix3/minix/servers/vm/main.c:345` | `-s`/`-f` 在真 boot 路径不出现 |
| B5 | PM 初始化：INIT 父进程=自身、`mp_pid=INIT_PID=1` | `minix3/minix/servers/pm/main.c:188,194-195` | PID 1 是约定的位置 |
| B6 | PM：RS 的父进程也是 INIT | `minix3/minix/servers/pm/main.c:203-204` | 同 B5 一批 |
| B7 | 抑制解除 → kernel 首次调度 init | `01-stage-kernel/`（本 stage 不展开） | 前置知识 |

**M 段：`main()`（`init.c:229-367`）**

| # | 动作 | 锚点 | 说明 |
|---|---|---|---|
| M1 | `gettimeofday(&boot_time)`（SUPPORT_UTMPX） | `init.c:238` | 台账用的开机时刻 |
| M2 | `getuid() != 0` → `errno=EPERM; err(1,NULL)` | `init.c:242-245` | 不返回 |
| M3 | `getpid() != 1` → `errx(1,"already running")` | `init.c:248-249` | 不返回 |
| M4 | `setsid()` 失败仅 `warn` | `init.c:255-256` | 卫生措施 |
| M5 | （非 Minix 分支）`setlogin("root")` | `init.c:262-265` | Minix 构建跳过 |
| M6 | `mfs_dev()`：`/dev/console` 缺失则 fork 跑 MAKEDEV | `init.c:268-271, 1703-1788` | 失败改首状态为 single_user |
| M7 | `openlog("init", LOG_CONS, LOG_AUTH)` | `init.c:278` | 不打开文件 |
| M8 | `getopt("sf")`：`-s`→single_user，`-f`→FASTBOOT | `init.c:287-303` | 多余参数只 warning |
| M9 | 信号注册：Minix 分支 `handle`×5（minixreboot/minixpowerdown/disaster/transition_handler/alrm_handler） | `init.c:315-320` | 顺序固定 |
| M10 | `sigfillset` + `delset` 放行八类 + `sigprocmask(SIG_SETMASK)` | `init.c:321-329` | 先全屏蔽再放行 |
| M11 | `SIGTTIN`/`SIGTTOU` 置 `SIG_IGN` | `init.c:330-334` | job control 保护 |
| M12 | `close(0/1/2)` | `init.c:339-341` | 描述符卫生 |
| M13 | `createsysctlnode()`（CHROOT 构建）造 `init.root` 节点 | `init.c:343-346, 1811-1857` | 默认 `"/"` |
| M14 | `securelevel_present = has_securelevel()` | `init.c:353, 544-563` | 能力探测 |
| M15 | `transition(requested_transition)` | `init.c:358` | 默认 `runcom`（`:195`），LETS_GET_SMALL 为 `single_user`（`:217`） |

**T 段：`transition()`（`init.c:624-640`）**

| # | 动作 | 锚点 | 说明 |
|---|---|---|---|
| T1 | `s == NULL` 直接返回 | `init.c:628-629` | 防御性 |
| T2 | 每轮先 `utmpx_set_runlevel(get_runlevel(current_state), get_runlevel(s))` | `init.c:631-635` | SUPPORT_UTMPX 构建 |
| T3 | `current_state = s`（初始为 `death`，`init.c:201`） | `init.c:635` | |
| T4 | `s = (state_t)(*s)()` 调状态函数取下一状态 | `init.c:638` | 永不退出 |

**S 段：`single_user()`（`init.c:694-874`）**

| # | 动作 | 锚点 |
|---|---|---|
| S1 | `did_multiuser_chroot = 0` | `init.c:717` |
| S2 | `getsecuritylevel()`，>0 则 `setsecuritylevel(0)` | `init.c:723-725` |
| S3 | SIGTSTP/SIGHUP 置 `SIG_IGN` 并保存旧动作 | `init.c:727-731` |
| S4 | fork；子：`setctty(_PATH_CONSTTY)` 否则 `setctty(_PATH_CONSOLE)` | `init.c:732,736-739` |
| S5 | 子：SECURE 口令门（条件见 `init.c:749-750`），`^D` → `_exit(0)` | `init.c:747-763` |
| S6 | 子：ALTSHELL 提示，`PATH=INIT_PATH` | `init.c:768-783, 799` |
| S7 | 子：解屏蔽 → `execv(shell)` → 失败 `execv(INIT_BSHELL)` → 睡 30 → `_exit(3)` | `init.c:790-810` |
| S8 | 父 fork 失败：emergency + 收尸 + 恢复信号 + 返回 single_user | `init.c:813-823` |
| S9 | 父 `requested_transition=0`；`waitpid(-1,&status,WUNTRACED)` 循环 | `init.c:825-841` |
| S10 | 五结局：stopped→SIGCONT；请求→跳转；SIGKILL→`sigsuspend` 死等；其它信号→重来；正常退出→置 FASTBOOT 进 runcom | `init.c:836-873` |

**R 段：`runcom()` + `runetcrc()`（`init.c:974-1014, 879-969`）**

| # | 动作 | 锚点 |
|---|---|---|
| R1 | `runetcrc(0)`：子分支 SIG_IGN(TSTP/HUP) → `setctty(_PATH_CONSOLE)` → 组 argv → 解屏蔽 | `init.c:887-902` |
| R2 | 子：CHROOT 且 `trychroot` → `chroot(rootdir)` 失败 `_exit(4)` | `init.c:904-910` |
| R3 | 子：`execv(INIT_BSHELL, argv)`，失败 stall 后 `_exit(5)` | `init.c:913-915` |
| R4 | 父 fork 失败：emergency + 收尸 + 睡 30 + 回 single_user | `init.c:917-923` |
| R5 | 父 `waitpid(-1, WUNTRACED)` 循环，WIFSTOPPED → SIGCONT | `init.c:931-947` |
| R6 | SIGTERM 且 `requested_transition==catatonia` → `sigsuspend` 静默等重启 | `init.c:949-957` |
| R7 | 非 WIFEXITED 或非零退出 → single_user；零退出 → read_ttys | `init.c:959-968` |
| R8 | `runcom`：`shouldchroot()` 为真则 `runetcrc(1)`，成功置 `did_multiuser_chroot=1` | `init.c:990-998` |
| R9 | `runcom_mode = AUTOBOOT`；写 reboot 台账（`~` 行） | `init.c:1005-1012` |
| R10 | 返回 read_ttys | `init.c:1013` |

**TT 段：`read_ttys()`（`init.c:1222-1285`）**

| # | 动作 | 锚点 |
|---|---|---|
| TT1 | `sessions == NULL` 时写 BOOT_TIME 记录；若 wtmpx 非空再写 DOWN_TIME（取 atime/mtime 较大者） | `init.c:1229-1247` |
| TT2 | 销毁旧链表（有进程先 `clear_session_logs`），`sessions = NULL` | `init.c:1252-1260` |
| TT3 | `start_session_db()` 失败：chroot 过→death，否则→single_user | `init.c:1262-1271` |
| TT4 | `do_setttyent()`（chroot 时拼 `rootdir + /etc/ttys`） | `init.c:1273, 1792-1806` |
| TT5 | `getttyent()` 循环 → `new_session(sp, ++session_index, typ)` 挂链尾 | `init.c:1279-1281` |
| TT6 | `endttyent()` → 返回 multi_user | `init.c:1282-1284` |

**MU 段：`multi_user()`（`init.c:1528-1564`）**

| # | 动作 | 锚点 |
|---|---|---|
| MU1 | `requested_transition = 0` | `init.c:1535` |
| MU2 | `getsecuritylevel()==0` → `setsecuritylevel(1)`（`-1` 表示管理员"别动"） | `init.c:1543-1544` |
| MU3 | 遍历 sessions，`se_process==0` 的 `start_getty`，失败则请求 clean_ttys 并 break | `init.c:1546-1557` |
| MU4 | `start_getty`：子分支 chroot（`_exit(7)`）→ 防抖动（睡在子进程里）→ 窗口系统（`_exit(6)`）→ 解屏蔽 → `execv` 失败 `_exit(8)` | `init.c:1321-1370` |
| MU5 | `while(!requested_transition) waitpid(-1,&status,0)`（**不带** WUNTRACED）→ `collect_child` | `init.c:1559-1561` |
| MU6 | `collect_child`：未知 pid 忽略 → `clear_session_logs` → `del_session` → SHUTDOWN 摘链释放 / 重启并 `add_session` | `init.c:1460-1497` |
| MU7 | 返回 `requested_transition` | `init.c:1563` |

**CT 段：`clean_ttys()`（`init.c:1569-1629`）**：清 PRESENT（`:1577-1578`）→ `do_setttyent`（`:1580`）
→ 逐行匹配（命中更新 index/开关，熄火或解析失败置 SHUTDOWN+SIGHUP；未命中 `new_session`）（`:1583-1617`）
→ `endttyent`（`:1619`）→ 收尾：无 PRESENT 者 SHUTDOWN+SIGHUP（`:1621-1626`）→ 返回 multi_user（`:1628`）。

**CA/D 段**：`catatonia`（`:1634-1643`，全置 SHUTDOWN 回 multi_user）；
`death`（`:1661-1698`，全置 SHUTDOWN → 写 shutdown 台账 → 三轮 `kill(-1, {HUP,TERM,KILL})`，
每轮 `clang=0` + `alarm(10)` + 收割循环，ESRCH/ECHILD 提前回 single_user，三轮耗尽 warning
"ps axl advised" → 返回 single_user）。

**异步支线（与状态机并行，随时插入）**：`transition_handler`（`:1502-1522`）、
`alrm_handler`（`:1649-1655`）、`minixreboot`/`minixpowerdown`（`:517-538`）、
`disaster`（`:504-511`）。

---

## 2. 知识点全集

> 编号按**新目录篇章顺序**编排，便于 §5 契约直接引用。
> 来源：`存量` = 现有文档已承载；`新增` = 现有文档未讲，由 C 源码 / 非 C 制品 / OS 理论承载。
> "现位置"列中 `—` 表示新增知识点无旧位置。

### 2.1 总表

| 编号 | 名称 | 类型 | 来源 | 现位置 | 锚点 | 读者收益（能回答什么） | 新去向 |
|---|---|---|---|---|---|---|---|
| K-001 | init 是 boot 链终点 | 架构 | 存量 | 00/01/14 | `kernel/table.c:64` | init 何时被调度 | 00 §2 |
| K-002 | USR_F 非系统服务登记 | 接口 | 存量 | 00/14 | `rs/table.c:28` | init 是不是服务 | 00 §3 |
| K-003 | 没有 IPC/SEF/CALLMAP：waitpid+信号状态机 | 概念 | 存量 | 00/plan §1.2 | `init.c:1535-1563` | 与其它 stage 服务的差别 | 00 §3 |
| K-004 | VM 加载 ELF 与固定 argv `{"init",NULL}` | 接口 | 存量 | 01/14 | `vm/main.c:331,345,514` | `-s/-f` 何时才有用 | 00 §2 / 08 §4 |
| K-005 | PM：INIT 父=自身、INIT_PID=1 | 接口 | 存量 | 01/14 | `pm/main.c:188-195` | PID 1 为何特殊 | 00 §2 |
| K-006 | RS 的父进程也是 INIT；调度继承 | 接口 | 存量 | 14 | `pm/main.c:203-204`、`pm/schedule.c:34,73` | INIT 的亲属范围 | 08 §4 |
| K-007 | procfs `service_active` 对 INIT 返回假 | 接口 | 存量 | 14 | `procfs/service.c:195-207` | 为什么查不到 init 服务 | 08 §4 |
| K-008 | 七状态字符与主线图 | 概念 | 存量 | 00/02 | `init.c:133-139` | 全局路线图 | 00 §4 |
| K-009 | 次主线：登录会话生命周期 | 概念 | 存量 | 00 | `init.c:1142-1497` | 会话在哪几篇 | 00 §4 |
| K-010 | `boot_time` 开机时刻采样 | 数据 | 新增 | — | `init.c:238` | 台账时间戳来源 | 01 §2 |
| K-011 | 身份双校验（EPERM / already running） | 约束 | 存量 | 01 | `init.c:242-249` | 为何必须 root+pid1 | 01 §2 |
| K-012 | `setsid()` 初始会话与宽容失败 | 机制 | 存量 | 01 | `init.c:255-256` | 为何与终端解耦 | 01 §2 |
| K-013 | `setlogin()` 属非 Minix 分支（排除） | 约束 | 存量 | 01 | `init.c:262-265` | 构建差异 | 01 §2 / 99 §5 |
| K-014 | `mfs_dev()`：console 缺失跑 MAKEDEV | 机制 | 存量 | 01 | `init.c:1703-1788` | 无 /dev 时怎么办 | 01 §3 |
| K-015 | `mfs_dev` 内 `#if 0` 调试段 | 约束 | 存量 | 01 | `init.c:1716-1756` | 哪些代码不用读 | 01 §3 / 99 §6 |
| K-016 | `openlog("init", LOG_CONS, LOG_AUTH)` | 接口 | 存量 | 01/03 | `init.c:278` | 日志通道何时建立 | 01 §2 |
| K-017 | `getopt "sf"` 两字母语义 | 机制 | 存量 | 01 | `init.c:287-303` | 运维后门 | 01 §3 |
| K-018 | 默认首状态 runcom（LETS_GET_SMALL 下 single_user） | 约束 | 存量 | 01/99 | `init.c:195,217,302-303` | 默认路径 | 01 §2 / 99 §4 |
| K-019 | `close(0/1/2)` 描述符卫生 | 机制 | 存量 | 01 | `init.c:339-341` | 守护进程范式 | 01 §2 |
| K-020 | 信号注册调用点顺序（handle×5 + delset + sigprocmask + TTIN/TTOU） | 机制 | 存量 | 01/02 | `init.c:315-334` | 注册顺序事实 | 01 §2（调用点）→ 03 |
| K-021 | `has_securelevel()` 探测调用点 | 机制 | 存量 | 01/12 | `init.c:353` | 能力探测时机 | 01 §2（调用点）→ 06 |
| K-022 | `transition(requested_transition)` 跳转不返回 | 机制 | 存量 | 01/02 | `init.c:358` | 线性段终点 | 01 §2 |
| K-023 | `BootArgs`/`EntryDecision` 纯决策（Rust） | 演进 | 存量 | 01 | `entry.rs` | 为何可单测 | 01 §4 |
| K-024 | `check_identity` 返回 Result（Rust） | 演进 | 存量 | 01 | `entry.rs` | 库/策略分离 | 01 §4 |
| K-025 | `ensure_console` MAKEDEV 兜底（Rust） | 演进 | 存量 | 01 | `entry.rs` | C 的 helper 折叠 | 01 §4 |
| K-026 | 死项排除：_PATH_SLOGGER / INIT_MOUNT_MFS / print_console / badsys | 约束 | 存量 | 01/03/plan §5.4 | `pathnames.h:39`、`init.c:102,411,490` | 不必读的代码 | 99 §6 |
| K-027 | `state_t`/`state_func_t` 递归 typedef 体操 | 数据结构 | 存量 | 02 | `init.c:130-131` | C 为何绕弯 | 02 §2 |
| K-028 | 七状态宏（大小写敏感） | 数据 | 存量 | 02 | `init.c:133-139` | 状态词汇 | 02 §2 |
| K-029 | `transition` 主循环三步（记台账→推进→永循环） | 机制 | 存量 | 02 | `init.c:624-640` | 主循环全貌 | 02 §2 |
| K-030 | `current_state` 初值为 death | 数据 | 存量 | 02/99 | `init.c:201` | 首条 runlevel 记录为何是 d→r | 02 §2 |
| K-031 | `StateKind` 枚举 + `as_char/from_char`（Rust） | 演进 | 存量 | 02 | `state_machine.rs` | 类型安全替代函数指针 | 02 §4 |
| K-032 | `run_transition` + `DriverState` 单所有者（Rust） | 演进 | 存量 | 02/00 | `driver.rs:39-201` | C 全局去哪了 | 02 §4 |
| K-033 | `ChildCollector`/`Ledger` 视图切分（Rust） | 演进 | 存量 | 02 | `driver.rs:103-153` | 状态间共享状态怎么传 | 02 §4 |
| K-034 | LETS_GET_SMALL 单状态变体 | 约束 | 存量 | 02/99 | `init.c:142-149` | 小构建差异 | 02 §2 / 99 §5 |
| K-035 | `handle()` 可变参数批量注册 | 机制 | 存量 | 02 | `init.c:369-389` | 注册机制 | 03 §2 |
| K-036 | `sa_mask` 全集 + `SA_NOCLDSTOP` + `XXX SA_RESTART?` 未决 | 约束 | 存量 | 02 | `init.c:380-385` | 未决问题如实转述 | 03 §2 |
| K-037 | `delset()` 掩码删除 | 机制 | 存量 | 02 | `init.c:394-405` | 先全屏蔽再放行 | 03 §2 |
| K-038 | Minix 分支五组 handler 与信号归属 | 接口 | 存量 | 01/02/03/14 | `init.c:315-320` | 哪个信号归谁 | 03 §2 |
| K-039 | `transition_handler` 三映射 + default 清零 | 机制 | 存量 | 02 | `init.c:1502-1522` | 信号如何变成状态请求 | 03 §2 |
| K-040 | `alrm_handler` 置 `clang` | 机制 | 存量 | 02/11 | `init.c:1649-1655` | 闹钟只置标志 | 03 §2 |
| K-041 | `requested_transition` 异步写 / 同步认领桥 | 概念 | 存量 | 02 | `init.c:825,1535,1559,1563` | 信号上下文为何不做实事 | 03 §1 |
| K-042 | 信号编号权威（HUP1/ABRT6/BUS10/ALRM14/TERM15/TSTP18/USR130） | 数据 | 存量 | 02/14 | `minix-types/src/types/signal.rs` | 别按 Linux 编号认人 | 03 §2 / 99 §3 |
| K-043 | `SignalState` 原子位、spawn 移出信号上下文（Rust 自觉偏离） | 演进 | 存量 | 02/14 | `signal_state.rs` | 机制偏离了什么 | 03 §4 |
| K-044 | `SignalSpec` 数据 + `InitHost::register_handlers`（Rust） | 演进 | 存量 | 02 | `host.rs` | 注册为何是数据 | 03 §4 |
| K-045 | `signal_to_state` 纯函数（Rust） | 演进 | 存量 | 02 | `state_machine.rs` | 映射可单测 | 03 §4 |
| K-046 | `sigsuspend` 死等（SIGKILL 后 / 静默重启） | 机制 | 存量 | 04/05(旧) | `init.c:855-856,955-956` | 状态函数不返回的情形 | 03 §2 → 09/10 引用 |
| K-047 | `stall`：LOG_ALERT + 睡 30 秒（读屏时间） | 机制 | 存量 | 03 | `init.c:440-450` | 为何要睡 | 04 §2 |
| K-048 | `warning`：不睡 | 机制 | 存量 | 03 | `init.c:457-466` | 分级差异 | 04 §2 |
| K-049 | `emergency`：LOG_EMERG | 机制 | 存量 | 03 | `init.c:472-481` | 最高级 | 04 §2 |
| K-050 | `disaster`：遗言 + 睡 30 + `_exit(sig)` | 机制 | 存量 | 03 | `init.c:504-511` | init 崩溃为何等于重启 | 04 §2 |
| K-051 | `STALL_TIMEOUT=30` | 数据 | 存量 | 03/99 | `init.c:95` | 常量 | 04 §2 / 99 §3 |
| K-052 | syslog 缺口（ARCH A-3）与 console 直写 | 演进 | 存量 | 03 | plan §4 A-3 | 日志去哪了 | 04 §4 |
| K-053 | `Severity` 两级 + `console_write`（Rust） | 演进 | 存量 | 03 | `log.rs` | 两条 severity | 04 §4 |
| K-054 | session logger 的 NB 未实现（诚实缺口） | 约束 | 存量 | 03 | `init.c:438,455,470` | 不补完的理由 | 04 §2 |
| K-055 | `DisasterAction::ExitWith` 数据化（Rust） | 演进 | 存量 | 03 | `log.rs` | 发散语义可断言 | 04 §4 |
| K-056 | fork 三分支语义（0 子 / -1 失败 / >0 父） | 机制 | 新增 | — | `init.c:732,887,1331` | 所有状态函数的骨架 | 05 §2 |
| K-057 | waitpid 选项差异（WUNTRACED vs 0） | 约束 | 存量 | 04/09(旧) | `init.c:827,932,1560,1687` | 暂停为何只在救援态可见 | 05 §2 |
| K-058 | `WaitStatus` 分类（EXITED/SIGNALED/STOPPED） | 数据结构 | 存量 | 04(旧) | `wait.h:53-70` | 结局判定 | 05 §2 |
| K-059 | `EINTR` 重试 / `ECHILD` 收工 | 约束 | 存量 | 04/11(旧) | `init.c:830-831,1691` | 等待循环的出口 | 05 §2 |
| K-060 | `_exit` 编码总表（0/1/2/3/4/5/6/7/8/9/10/11/12 + sig） | 数据 | 新增 | — | `init.c`（16 处，见 §0.3） | 每个子进程的遗言含义 | 05 §3 |
| K-061 | 子进程 exec 前解屏蔽 + handler 在 exec 后复位 | 机制 | 存量 | 05/09(旧) | `init.c:790-791,902,1306,1362` | 子进程信号环境 | 05 §2 |
| K-062 | `ParsedCommand`：exec_path 与 argv[0] 分离 | 演进 | 存量 | 05/07/14(旧) | `init.c:899-900 vs 521-522` | 两种 exec 形状 | 05 §3 |
| K-063 | `WaitStatus` 上移 minix-types（Rust） | 演进 | 新增 | — | `wait.rs`（8 行）、todo P1-4 | 类型归属 | 05 §4 |
| K-064 | 剧本宿主 `fork_outcomes` 队列与 `catch_unwind`（Rust） | 工具 | 存量 | 04(旧) | `single_user.rs` | fork 返回两次怎么测 | 05 §4 / 18 §3 |
| K-065 | `has_securelevel` 能力探测 | 机制 | 存量 | 12 | `init.c:544-563` | 老内核上不硬调 | 06 §2 |
| K-066 | `get/setsecuritylevel` | 机制 | 存量 | 12 | `init.c:568-618` | 读写语义 | 06 §2 |
| K-067 | 单用户降 0 / 多用户 `==0` 才升 1（-1 表示别动） | 约束 | 存量 | 04/09(旧) | `init.c:723-725,1543-1544` | 档位如何变化 | 06 §2 |
| K-068 | `createsysctlnode`：`init` 节点 + `init.root` 字符串节点默认 `/` | 机制 | 存量 | 12 | `init.c:1811-1857` | 节点谁造的 | 06 §2 |
| K-069 | `shouldchroot`：读值 / ENOENT 重建 / 非字符串拒绝 / `/` 不 chroot | 机制 | 存量 | 12 | `init.c:1859-1900` | 第二遍 rc 的判据 | 06 §2 |
| K-070 | `rootdir` / `did_multiuser_chroot` 全局与 Rust 归宿 | 数据 | 存量 | 99 | `init.c:210-211` | 状态放在哪 | 06 §2 / 99 §4 |
| K-071 | chroot 后 ttys 路径拼接（`do_setttyent`） | 机制 | 存量 | 06/12 | `init.c:1792-1806` | 读哪张表 | 06 §3 → 11 引用 |
| K-072 | getty 子进程内 chroot 失败 `_exit(7)` | 机制 | 新增 | — | `init.c:1340-1348` | 遗漏的退出码 | 06 §3 / 15 §2 |
| K-073 | securelevel/chroot 在 minix-rs 的 defer（ARCH A-4/A-5） | 演进 | 存量 | 12 | plan §4 | 缺口边界 | 06 §4 |
| K-074 | `InitHost::securitylevel/init_root/chroot`（Rust） | 演进 | 存量 | 12 | `host.rs` | 接缝位置 | 06 §4 |
| K-075 | utmp/utmpx/wtmp/wtmpx 四个文件与路径 | 数据 | 存量 | 13/99 | `utmpx.h:39-40`、`utmp.h:42-43` | 账本在哪 | 07 §2 |
| K-076 | 记录类型（INIT/LOGIN/DEAD/RUN_LVL/BOOT_TIME/DOWN_TIME） | 数据 | 存量 | 13（BOOT/DOWN 缺） | `utmpx.h:57-64`、`init.c:1233,1244` | 四种以上记录 | 07 §2 |
| K-077 | `make_utmpx` 字段构造（name/line/pid/tv/session/ut_id 取行尾） | 机制 | 存量 | 13 | `init.c:1383-1409` | 一条记录长什么样 | 07 §2 |
| K-078 | `get_runlevel` 七分支 + 未知回 DEATH | 机制 | 存量 | 13 | `init.c:1411-1427` | 状态字符与 runlevel 的桥 | 07 §2 |
| K-079 | `utmpx_set_runlevel` 与 `sessions==NULL` 短路 | 约束 | 存量 | 13 | `init.c:1438-1441` | 为何 /var 可写前不记账 | 07 §2 |
| K-080 | `RUNLVL_MSG` 与 `e_exit`/`e_termination` 携带新旧级 | 数据 | 存量 | 13 | `utmpx.h:74`、`init.c:1443-1447` | runlevel 记录字段 | 07 §2 |
| K-081 | `logwtmpx` 的 `~` 行 reboot/shutdown 历史 | 机制 | 存量 | 13 | `init.c:1008,1674` | who/last 的历史来源 | 07 §2 |
| K-082 | SUPPORT_UTMP + SUPPORT_UTMPX 双写 | 约束 | 存量 | 13/plan | `Makefile:8` | 为何写两遍 | 07 §2 |
| K-083 | on-disk 编码临时态（ARCH A-2 残余） | 演进 | 存量 | 13 | `utmp.rs` encode | 编码为何是文本 | 07 §4 |
| K-084 | BOOT_TIME/DOWN_TIME 开机记录（read_ttys 内，依 wtmpx 大小推断停机时刻） | 机制 | 新增 | — | `init.c:1229-1247` | 开机/停机记录从哪来 | 07 §2 |
| K-085 | 双通道写顺序（utmpx 成功才写 wtmpx） | 约束 | 新增 | — | `init.c:654-658` | 两步写的因果 | 07 §2 → 14 §2 |
| K-086 | `minixreboot`：SIGABRT → fork/exec `/sbin/shutdown -r now` | 接口 | 存量 | 14 | `init.c:517-525` | Ctrl-Alt-Del 发生了什么 | 08 §3 |
| K-087 | `minixpowerdown`：SIGUSR1 → `-p` | 接口 | 存量 | 14 | `init.c:530-538` | 低电发生了什么 | 08 §3 |
| K-088 | Ctrl-Alt-Del → `sys_kill(INIT_PROC_NR, SIGABRT)` | 接口 | 存量 | 14 | `keyboard.c:300` | 信号谁发的 | 08 §3 |
| K-089 | 低电 → `kill(1, SIGUSR1)` | 接口 | 存量 | 14 | `tps65217.c:226` | 信号谁发的 | 08 §3 |
| K-090 | PM 孤儿收养到 INIT | 接口 | 存量 | 14 | `pm/forkexit.c:396` | init 为何会收到陌生 pid | 08 §3 |
| K-091 | INIT 死亡只栈回溯，不 panic | 接口 | 存量 | 14 | `pm/forkexit.c:336-341` | init 死了 PM 怎么办 | 08 §3 |
| K-092 | reboot 路径 stop init | 接口 | 存量 | 14 | `pm/misc.c:224` | 关机链路 | 08 §3 |
| K-093 | init 死 → 系统自动重启；boot 期找不到 init → panic | 约束 | 新增 | — | `init.8:310-317` | 为何 init 不能死 | 08 §3 |
| K-094 | kernel 尝试多个 init 路径（/sbin/init、oinit、init.bak、/rescue/init） | 约束 | 新增 | — | `init.8:54-61` | 救援路径 | 08 §3 |
| K-095 | shutdown 命令 argv 契约（路径 vs argv[0]） | 接口 | 存量 | 14 | `init.c:521-535` | 与 rc 的 argv 形状差异 | 08 §3 / 05 §3 |
| K-096 | `InitHost` 单一接缝（ARCH: init-host-seam） | 演进 | 存量 | 01/02/03/12 | `host.rs:55-177` | 机器副作用收在哪 | 08 §4（总述） |
| K-097 | 单用户是抢修通道、退出即前进（置 FASTBOOT） | 概念 | 存量 | 04 | `init.c:866-870` | 退出语义 | 09 §1 |
| K-098 | 三段结构（准备/fork/看护） | 机制 | 存量 | 04 | `init.c:715-873` | 状态函数骨架 | 09 §2 |
| K-099 | 安全级降级到 0 | 机制 | 存量 | 04/12 | `init.c:723-725` | 抢修要可写一切 | 09 §2（调用点）→ 06 |
| K-100 | SIG_IGN 窗口（TSTP/HUP）与 `satstp`/`sahup` 恢复 | 机制 | 存量 | 04 | `init.c:727-731,867-868` | 为何临时忽略 | 09 §2 |
| K-101 | 子进程 `setctty`（constty 优先 console） | 机制 | 存量 | 04/09 | `init.c:736-739` | 控制终端获得 | 09 §2 → 15 §2 |
| K-102 | SECURE 口令门三条件 | 机制 | 存量 | 04 | `init.c:749-750` | 何时要求口令 | 09 §2 |
| K-103 | 空口令 `^D` → `_exit(0)` 放弃抢修 | 约束 | 存量 | 04 | `init.c:755-756` | EOF 语义 | 09 §2 |
| K-104 | ALTSHELL 提示与回退 | 机制 | 存量 | 04 | `init.c:768-783` | 备用 shell | 09 §2 |
| K-105 | exec 双次兜底 + `_exit(3)` | 机制 | 存量 | 04 | `init.c:797-810` | 为何 exec 两次 | 09 §2 / 05 §3 |
| K-106 | `setenv("PATH", INIT_PATH)` | 数据 | 存量 | 04 | `init.c:799` | 环境变量 | 09 §2 |
| K-107 | 看护五结局 | 机制 | 存量 | 04 | `init.c:836-873` | 循环的五种出口 | 09 §2 |
| K-108 | single_user fork 失败**不睡**（与 runcom 不同） | 约束 | 存量 | 04/05 | `init.c:813-823 vs 917-923` | 两处不可混淆 | 09 §2 / 10 §2 |
| K-109 | `password.rs` 门控与 libcrypt 后端缺失（ARCH A-12） | 演进 | 存量 | 04/todo | `password.rs` | 后端缺失的代价 | 09 §4 |
| K-110 | /etc/rc 是黑盒，退出码是唯一协议 | 概念 | 存量 | 05 | `init.c:959-968` | 契约边界 | 10 §1 |
| K-111 | argv 组装（"sh" /etc/rc autoboot 或空） | 机制 | 存量 | 05 | `init.c:897-900` | fastboot 少一个参数 | 10 §2 |
| K-112 | runetcrc 子分支六步 | 机制 | 存量 | 05 | `init.c:887-915` | 子进程做了什么 | 10 §2 |
| K-113 | chroot 失败 `_exit(4)` / exec 失败 stall+`_exit(5)` | 约束 | 存量 | 05 | `init.c:906-915` | 非零码导向 | 10 §2 / 05 §3 |
| K-114 | 五归宿（fork 失败睡 30 / stop / catatonia+SIGTERM 静默 / 非正常或非零 / 零退出） | 机制 | 存量 | 05 | `init.c:917-968` | 出口表 | 10 §2 |
| K-115 | 两遍 rc 与 `did_multiuser_chroot` | 机制 | 存量 | 05 | `init.c:990-998` | 第二遍何时发生 | 10 §2 |
| K-116 | `runcom_mode` 成功后重置 AUTOBOOT | 数据 | 存量 | 05/99 | `init.c:1005` | 状态重置 | 10 §2 |
| K-117 | reboot 台账写入点 | 机制 | 存量 | 05/13 | `init.c:1007-1012` | 谁记开机 | 10 §2（调用点）→ 07 |
| K-118 | `runcom`/`runetcrc` 双层实体（Rust） | 演进 | 存量 | 05 | `runcom.rs` | 尝试 vs 策略 | 10 §4 |
| K-119 | /etc/ttys 四列语义与 TTY_ON/TTY_SECURE | 数据 | 存量 | 06 | `ttyent.h:57-58` | 表格式 | 11 §2 |
| K-120 | 先销毁旧链表再重建 | 概念 | 存量 | 06 | `init.c:1252-1260` | 为何不做 diff | 11 §2 |
| K-121 | DB 打开失败：chroot 过→death，否则→single_user | 约束 | 存量 | 06 | `init.c:1262-1271` | 失败分叉 | 11 §2 |
| K-122 | getttyent 字段语义（引号模式开关/精确 token/off 显式清除/window= 带值/# 注释） | 机制 | 存量 | 06 | `getttyent.c` skip/scmp | 解析器坑 | 11 §2 |
| K-123 | 少于四列不拒绝（status=0，随后被过滤） | 约束 | 存量 | 06 | `init.c:1147` | 解析与过滤分离 | 11 §2 |
| K-124 | `fparseln` 续行反斜杠未实现（已知边界） | 约束 | 存量 | 06 | 旧 doc 06 §3 | 诚实边界 | 11 §4 |
| K-125 | golden 测试取 `minix3/etc/ttys` 真实行 | 工具 | 存量 | 06 | `minix3/etc/ttys` | 测试数据来源 | 11 §4 / 18 §3 |
| K-126 | `session_t` 十字段 | 数据结构 | 存量 | 07 | `init.c:156-170` | 节点长什么样 | 12 §2 |
| K-127 | SE_SHUTDOWN / SE_PRESENT 标志 | 数据结构 | 存量 | 07 | `init.c:161-162` | 两个位 | 12 §2 |
| K-128 | `new_session` 四关过滤 | 机制 | 存量 | 07 | `init.c:1142-1180` | 建节点的失败路径 | 12 §2 |
| K-129 | `setupargv`（getty = "getty 终端名"；window 可选） | 机制 | 存量 | 07 | `init.c:1185-1217` | argv 从哪来 | 12 §2 |
| K-130 | `construct_argv` 空格制表分词器 | 机制 | 存量 | 07 | `init.c:1101-1118` | 分词规则 | 12 §2 |
| K-131 | `free_session` 释放顺序 | 机制 | 存量 | 07 | `init.c:1123-1137` | RAII 替代 | 12 §2 |
| K-132 | `se_started` 防抖动用途 | 数据 | 存量 | 07/09 | `init.c:159` | 时间戳为何存在 | 12 §2 → 15 §2 |
| K-133 | `build_session` + 链表不入节点（Rust） | 演进 | 存量 | 07 | `session.rs` | 所有权改造 | 12 §4 |
| K-134 | waitpid 只给 pid，需要反向索引 | 概念 | 存量 | 08 | `init.c:1469-1470` | DB 为何存在 | 13 §1 |
| K-135 | `dbopen(NULL,...)` 内存哈希、无持久化 | 机制 | 存量 | 08 | `init.c:1027` | 表随进程消失 | 13 §2 |
| K-136 | start/add/del/find 四函数与静默失败 | 机制 | 存量 | 08 | `init.c:1021-1096` | 宽容语义 | 13 §2 |
| K-137 | ARCH A-1：Berkeley DB → HashMap | 演进 | 存量 | 08 | plan §4 A-1 | 依赖消除 | 13 §4 |
| K-138 | `SessionDb` trait + `HashMapDb` + `FakeDb`（接缝外唯一 trait） | 演进 | 存量 | 08 | `session_db.rs` | 为何不收敛 | 13 §4 |
| K-139 | `session_utmpx`：name 三选一、line 去 `/dev` 前缀、LOGIN/DEAD | 机制 | 存量 | 13 | `init.c:1372-1381` | 会话台账挂载点 | 14 §2 |
| K-140 | `clear_session_logs` 两步写（utmpx 成功才写 wtmpx） | 机制 | 存量 | 13 | `init.c:647-662` | 关闭会计文件 | 14 §2 |
| K-141 | add/del_session 的 utmp 挂钩 | 机制 | 存量 | 08/13 | `init.c:1054-1056,1072-1074` | 挂载位置 | 14 §2 |
| K-142 | 双通道四函数（logoutx/logwtmpx + logout/logwtmp） | 约束 | 存量 | 13 | `init.c:654-661` | SUPPORT_UTMP 与 UTMPX 并存 | 14 §2 |
| K-143 | 台账回调如何注入（`record_reboot`/`record_shutdown`） | 演进 | 存量 | 05/11/13 | `runcom.rs`/`shutdown.rs` | 谁接台账 | 14 §4 |
| K-144 | 稳态 = 全启动 + 睡觉 | 概念 | 存量 | 09 | `init.c:1546-1562` | 稳态在做什么 | 15 §1 |
| K-145 | 安全级 `==0` 升 1 | 约束 | 存量 | 09 | `init.c:1543-1544` | 档位提升判据 | 15 §2 |
| K-146 | `start_getty` 顺序（chroot→防抖→窗口→解屏蔽→exec） | 机制 | 存量 | 09 | `init.c:1321-1370` | 子进程次序 | 15 §2 |
| K-147 | 防抖动常量（SPACING 5 / SLEEP 30）与"睡在子进程里" | 机制 | 存量 | 09 | `init.c:1350-1355` | 拖住的是 exec | 15 §2 |
| K-148 | `start_window_system` + WINDOW_WAIT=3（`_exit(6)`） | 机制 | 存量 | 09 | `init.c:1290-1316,1357-1360` | 窗口系统 | 15 §2 |
| K-149 | `setctty`：setsid + nanosleep(250ms) + open + login_tty（`_exit(1)/(2)`） | 机制 | 存量 | 09 | `init.c:669-689,98` | 成人礼四步 | 15 §2 |
| K-150 | `collect_child` 四动作 | 机制 | 存量 | 09 | `init.c:1460-1497` | 回收逻辑 | 15 §2 |
| K-151 | 未知 pid 忽略（孤儿与陌生子进程） | 约束 | 存量 | 09 | `init.c:1469-1470` | 为何不报警 | 15 §2 |
| K-152 | 启动失败请求 clean_ttys | 约束 | 存量 | 09 | `init.c:1489,1551` | 严重麻烦的出口 | 15 §2 |
| K-153 | 稳态 waitpid **不带** WUNTRACED | 约束 | 存量 | 09 | `init.c:1560` | 与救援态相反 | 15 §2 |
| K-154 | 时钟不可读降级（Rust ENOSYS） | 演进 | 存量 | 09 | `multi_user.rs` | 诚实失败 | 15 §4 |
| K-155 | PRESENT 标记做 diff 的三问 | 概念 | 存量 | 10 | `init.c:1577-1626` | 重读算法 | 16 §1 |
| K-156 | n² 算法自嘲与"重读很少发生" | 约束 | 存量 | 10 | `init.c:1567` | 为何不优化 | 16 §2 |
| K-157 | 四动作 `diff_line`（Rust） | 机制 | 存量 | 10 | `clean_ttys.rs` | diff 分类 | 16 §2 |
| K-158 | 序号变化只 warning 并跟随 | 约束 | 存量 | 10 | `init.c:1592-1597` | utmp index 漂移 | 16 §2 |
| K-159 | 熄火 / 解析失败 → SHUTDOWN + SIGHUP | 机制 | 存量 | 10 | `init.c:1598-1612` | 优雅下线 | 16 §2 |
| K-160 | 收尾：无 PRESENT 者全 SHUTDOWN + SIGHUP | 机制 | 存量 | 10 | `init.c:1621-1626` | 消失的行 | 16 §2 |
| K-161 | 文件读不到 = 全体退役 | 约束 | 存量 | 10 | `clean_ttys.rs` | 空文件行为 | 16 §2 |
| K-162 | catatonia：全置 SHUTDOWN 回 multi_user（温柔关停） | 机制 | 存量 | 11 | `init.c:1634-1643` | 假死是什么 | 17 §2 |
| K-163 | death 三轮 HUP/TERM/KILL + DEATH_WATCH=10 + alarm/clang | 机制 | 存量 | 11 | `init.c:1661-1698` | 真死流程 | 17 §2 |
| K-164 | `kill(-1)` ESRCH 提前回；ECHILD 提前回 | 约束 | 存量 | 11 | `init.c:1681-1692` | 提前收工 | 17 §2 |
| K-165 | 杀不完 → "ps axl advised" | 机制 | 存量 | 11 | `init.c:1695` | 认栽的出口 | 17 §2 |
| K-166 | 关机台账先写 | 机制 | 存量 | 11/13 | `init.c:1674` | 顺序 | 17 §2 |
| K-167 | 关停唯一返回态 single_user | 约束 | 存量 | 11 | `init.c:1682,1692,1697` | 闭环 | 17 §2 |
| K-168 | 三层测试：纯函数 / 实体剧本回放 / 全图逐边界 | 工具 | 新增 | — | `driver.rs` boot_chain 测试 | 测试怎么分层 | 18 §2 |
| K-169 | `ScriptHost` 剧本宿主：动作记录、不真睡 | 工具 | 新增 | — | `host.rs` | 双实现问题如何消失 | 18 §3 |
| K-170 | 发散语义测试：`catch_unwind` + exits 队列 | 工具 | 存量 | 04(旧) | `single_user.rs` | `_exit` 怎么断言 | 18 §3 |
| K-171 | 实测基线（2026-09-19：135 passed / 1 failed） | 工具 | 新增 | — | §0.3 | 文档不许转述数字 | 18 §2 |
| K-172 | clippy 残余 32 条为 E-INITSYS 门控 API 面 | 工具 | 存量 | todo §1 P2-6 | todo | 不删的理由 | 18 §4 |
| K-173 | 等待态清单（E-INITSYS / E-CMDSYSFACE / E-ISBOOT / E5） | 约束 | 存量 | todo §2 | `edge_todo.md` | 缺口归属 | 18 §4 |
| K-174 | 常量表（INIT_PID / INIT_BSHELL / INIT_PATH / 状态字符 / 超时 / dtrtime） | 数据 | 存量 | 99 | `init.c:92-108` | 查表 | 99 §2 |
| K-175 | 路径表（/etc/rc、/etc/ttys、/dev/console、/dev/constty、utmpx/wtmpx、/sbin/shutdown） | 数据 | 存量 | 99 | `pathnames.h:40`、`paths.h:62-63` | 查表 | 99 §2 |
| K-176 | C 全局十项 → Rust 单所有者归宿（ARCH: init-host-seam 状态侧） | 数据 | 存量 | 99 | `init.c:151-211` | 全局变量去哪了 | 99 §4 |
| K-177 | 构建变体宏 → feature flags（ARCH A-7） | 演进 | 存量 | 99/plan | `Makefile:8-17` | 七宏如何映射 | 99 §5 |
| K-178 | crate 形态：`minix-init` 走 `minix-rt` 的 std feature；no_std 三件裁决 | 演进 | 新增 | — | `Cargo.toml`、todo P1-2 | 二进制怎么构建 | 99 §5 |
| K-179 | 参数来自 `minix_rt::crt0`（非 std env） | 接口 | 新增 | — | `main.rs:66-69` | argv 来源 | 01 §4 / 99 §5 |

### 2.2 统计摘要

| 项 | 数 |
|---|---|
| 知识点总数 | 179 |
| 其中存量 | 163（91.1%） |
| 其中新增 | 16（8.9%）：K-010、K-056、K-060、K-063、K-072、K-084、K-085、K-093、K-094、K-168、K-169、K-171、K-178、K-179，另 K-095/K-142 部分内容新增 |
| 按类型分布 | 机制 62 · 约束 34 · 数据 25 · 接口 21 · 演进 25 · 概念 12 · 工具 6 · 数据结构 4 · 架构 1 |
| 主讲述点唯一性 | 每条知识点在 §5 契约中只出现在一个"讲什么"清单里，其余为引用 |

---

## 3. 覆盖审计

### 3.1 主题全集与来源

| 来源 | 取材方式 |
|---|---|
| C 源码符号 | `init.c` 46 个函数定义（44 生效 + `print_console` `#if 0` + `badsys` 非 Minix 分支）、7 个状态宏、5 个超时常量、16 处 `_exit`、session_t 十字段、SE_* 两位 |
| OS 通用概念 | PID 1 语义、会话与控制终端、孤儿收养、作业控制信号保护、runlevel、防抖动、syslog 分级、Berkeley DB 内存哈希 |
| 非 C 制品 | Makefile（构建宏）、init.8（外部行为契约）、NOTES（POSIX 依据）、pathnames.h、ttyent.h/utmpx.h/utmp.h、getttyent.c、minix3/etc/ttys、minix3/etc/rc、os/commands/sbin/init/（Cargo.toml + 20 模块） |
| 阶段边界契约 | `plan.md §5.3`（13 项外部契约）、`todo.md §2`（edge 等待态）、`00-master-plan/README.md`（启动因果链） |

### 3.2 覆盖缺口表（主题在全集里但没有任何一篇讲，或讲得不完整）

| # | 缺口 | 证据锚点 | 处置 |
|---|---|---|---|
| G-01 | `_exit` 编码没有总表，0/1/2/3/4/5/6/7/8/9/10/11/12 散在六篇 | `init.c` 16 处（见 §0.3） | 新建 05 §3 收总表；99 §2 索引 |
| G-02 | getty 子进程内 chroot 失败 `_exit(7)` 未在任何一篇出现 | `init.c:1340-1348` | 并入 06 §3 + 15 §2；新增 K-072 |
| G-03 | BOOT_TIME / DOWN_TIME 开机与停机记录（含依 wtmpx 大小推断停机时刻）未讲全 | `init.c:1229-1247` | 并入 07 §2；新增 K-084 |
| K-085 类 | utmpx 写成功才写 wtmpx 的两步顺序只在旧 13 篇测试名里露出，正文未讲 | `init.c:654-658` | 并入 07 §2 + 14 §2 |
| G-04 | 子进程模型（fork 三分支、waitpid 选项差异、WaitStatus、EINTR/ECHILD）无主讲述点，散在四篇 | `init.c:827,932,1560,1687` | 新建 05 |
| G-05 | 测试基建（三层测试、剧本宿主、golden、发散断言、实测基线）无主讲述点 | `host.rs`/`driver.rs` | 新建 18 |
| G-06 | init 死亡的外部后果（系统自动重启、boot 期找不到 init 则 panic、kernel 的四个候选路径）只在 man page 里 | `init.8:54-61,310-317` | 并入 08 §3；新增 K-093/K-094 |
| G-07 | `boot_time` 采样（main 首行）未讲 | `init.c:238` | 并入 01 §2 |
| G-08 | 信号编号权威表只在旧 02/14 的测试名里出现，正文无表 | `minix-types/src/types/signal.rs` | 并入 03 §2 + 99 §3 |
| G-09 | crate 形态与构建（Cargo.toml、std feature、no_std 三件裁决、crt0 取参）未讲 | `Cargo.toml`、`main.rs:66-69`、todo P1-2 | 并入 99 §5 + 01 §4 |
| G-10 | `InitHost` 单一接缝只有分散叙述，缺一处总述（它是全 crate 的架构主脊） | `host.rs:1-54` | 并入 08 §4 作总述（与状态机主线同一层） |
| G-11 | `sigsuspend` 死等作为"状态函数不返回"的情形，在旧 04/05 各半句 | `init.c:855-856,955-956` | 并入 03 §2（信号语义），09/10 引用 |
| G-12 | 与相邻 stage 的边界（18-stage-commands 讲 /etc/rc 脚本体系、14-stage-runtime 讲 crt0）未在 00 声明 | `18-stage-commands/01-init-rc-scripts.md:5-8`、`14-stage-runtime/02-crt0-start.md:1-6` | 并入 00 §5 |
| G-13 | `getttyent` 的 `fparseln` 续行未实现这一诚实边界只在旧 06 的正文里 | 旧 doc 06 §3 | 保留在 11 §4 |
| G-14 | 实测测试基线（135/1）与文档声明（143/0）不一致 | §0.3 | 并入 18 §2，并作为全目录数字纪律 |

### 3.3 重复主题表

| # | 主题 | 现有重复位置 | 处置 |
|---|---|---|---|
| D-01 | securelevel 的调用（降级/升级） | 旧 04 §2.2、旧 09 §2、旧 12 §2 | 主讲述点 = 新 06；旧 04/09 位置改为一行调用点 |
| D-02 | chroot 的三处消费（runcom、read_ttys 路径、getty） | 旧 05、旧 06、旧 09、旧 12 | 主讲述点 = 新 06 §3；其余为调用点 |
| D-03 | utmp 台账 | 旧 05（reboot 记录）、旧 06（BOOT 记录）、旧 08（add/del 挂钩）、旧 11（shutdown 记录）、旧 13 | 主讲述点 = 新 07（记录与 runlevel）+ 新 14（会话挂载）；其余为调用点 |
| D-04 | `collect_child` | 旧 04（等待循环）、旧 05、旧 09、旧 11 | 主讲述点 = 新 15 §2；09/10/17 只讲"它对本篇循环的影响" |
| D-05 | waitpid 循环的五/四结局（single_user 与 runetcrc 结构几乎相同） | 旧 04 §2.4、旧 05 §2.3 | 通用骨架归 05；两篇只列差异 |
| D-06 | 状态机主线图 | 旧 00、旧 README、plan §1.3 | 主讲述点 = 新 00 §4；README/plan 改引用 |
| D-07 | 常量（STALL_TIMEOUT 等） | 旧 03 §2.1、旧 99、旧 09 §2 | 主讲述点 = 新 99 §2；其余只写值不解释 |
| D-08 | init 不是服务（USR_F/procfs） | 旧 00、旧 14 | 主讲述点 = 新 00 §3（身份），新 08 §4 讲对端证据 |

### 3.4 越界主题表

| # | 越界内容 | 现有位置 | 正确归属 |
|---|---|---|---|
| O-01 | `mfs_dev` 里 mount_mfs 创建 /dev 的细节（`#if 0` 段） | 旧 01 §2.4 展开 | 标注死代码即可，机制归 18-stage-commands（MAKEDEV） |
| O-02 | /etc/rc 脚本内部（rc.d/rcorder/rc.conf） | 旧 05 未越界，但与 18-stage-commands/01 主题相邻 | 新 10 §1 显式声明"脚本内容是黑盒"，细节归 `../18-stage-commands/01-init-rc-scripts.md` |
| O-03 | crt0 与运行时启动 | 无（缺口） | 归 `../14-stage-runtime/02-crt0-start.md`；本 stage 只讲"参数从 crt0 来" |
| O-04 | 内核 sysctl 实现 | 旧 12 边界声明已正确 | 保持指向 `../01-stage-kernel/` |
| O-05 | boot 镜像与页表建立 | 旧 00/01 提到但没展开 | 保持指向 `../01-stage-kernel/`、`../02-stage-vm/` |

### 3.5 非 C 主题十项逐项回答

| 主题 | 在本 stage 讲不讲 | 落点 |
|---|---|---|
| 链接与加载 | **不讲**（init 由 VM `exec_bootproc` 加载，`vm/main.c:331,514`） | 00 §2 只引一句；机制归 `../02-stage-vm/` |
| 镜像与内存布局 | **只讲一个事实**：init 是 `boot_image` 最后一项（`kernel/table.c:64`） | 00 §2；布局机制归 `../01-stage-kernel/` |
| 汇编入口与陷阱进入 | **不讲**（init 是纯 C 用户程序，无汇编入口、无陷阱处理） | 99 §6 明确排除 |
| 启动装配 | **讲一段**：参数由 `minix_rt::crt0` 提供，非 std env（`main.rs:66-69`） | 01 §4；crt0 机制归 `../14-stage-runtime/02-crt0-start.md` |
| 构建与工具链 | **讲**：Makefile 七宏（MFS_DEV_IF_NO_CONSOLE/SUPPORT_UTMP/SUPPORT_UTMPX/ALTSHELL/SECURE/CHROOT/SMALLPROG）+ LIBCRYPT → Rust feature flags；Cargo.toml 与 std feature | 99 §5；ARCH A-7 落在同一处 |
| 跨模块接口与线格式 | **讲，且是重点**：/etc/ttys 行格式（11）、utmp 记录字段与编码临时态（07/14）、rc argv 协议（10）、shutdown argv 协议（08）、boot argv 协议（08）、`_exit` 编码表（05） | 分散在 05/07/08/10/11/14；99 §2 作索引 |
| 错误路径 | **讲**：`_exit` 编码总表 + 各状态的失败归宿 | 05 §3 + 各状态篇的"失败归宿"小节 |
| 关闭与退出 | **讲**：catatonia/death（17）、disaster 与 init 死亡的后果（04/08） | 17 §2、04 §2、08 §3 |
| 并发与同步 | **讲**：无 IPC 无锁；并发只来自信号——异步写 / 同步认领桥（`requested_transition`、`clang`）；Rust 用原子位与 `Arc<AlarmFlag>` | 03 §1/§4；15 §4 讲时钟降级 |
| 测试基建 | **新建一篇讲**：三层测试、ScriptHost、golden、发散断言、实测基线、clippy 残余与等待态 | 18 全篇 |

---

## 4. 新目录

### 4.1 总表（20 篇）

| 编号 | 标题 | 一句话定位 | 分组 |
|---|---|---|---|
| 00 | init 的定位与 boot 链终点 | init 是什么、不是什么、状态机全景与导航 | A 定位 |
| 01 | 入口：出生证明八步 | `main()` 从头到 `transition()` | B 出生与基础设施 |
| 02 | 状态机骨架与分派 | 七状态、`state_t`、`transition` 主循环 | B |
| 03 | 信号：注册、handler 与异步请求桥 | 信号如何进入状态机而不在信号上下文做事 | B |
| 04 | 日志与致命故障 | 四级日志与 disaster 的遗言 | B |
| 05 | 子进程模型：fork、exec、等待与退出码 | 所有状态函数共用的进程骨架与 `_exit` 编码总表 | B |
| 06 | 系统交互：securelevel 与 init.root | 两个运行时旋钮与三处 chroot 消费点 | B |
| 07 | 会话台账（上）：utmp/utmpx 记录与 runlevel | 账本的记录类型、构造与状态变迁记录 | B |
| 08 | 对外契约：谁在什么时候找 init | boot 期、运行期、查询期的全部对端 | B |
| 09 | 状态 's'：单用户抢修 | 口令门 + shell + 看护循环 | C 启动三站 |
| 10 | 状态 'r'：/etc/rc 与两遍启动 | 黑盒脚本的退出码协议与 chroot 第二遍 | C |
| 11 | 状态 't'：/etc/ttys 解析与重建 | 表格式、字段语义与链表重建 | C |
| 12 | 会话对象：session_t 与命令行分词 | 节点结构与两条命令向量 | D 会话模型 |
| 13 | 会话数据库：pid → session | 反向索引的四个函数与 ARCH A-1 | D |
| 14 | 会话台账（下）：会话记录挂载点 | add/del 与会话退出时的两步写 | D |
| 15 | 状态 'm'：多用户稳态 | 全启动 + 睡觉 + 回收 | E 稳态与关停 |
| 16 | 状态 'T'：重读终端表与 diff | PRESENT 标记三问 | E |
| 17 | 状态 'c'/'d'：假死与关机 | 温柔关停与三轮 kill | E |
| 18 | 测试与验证基建 | 三层测试、剧本宿主、golden 与实测基线 | F 收口 |
| 99 | 全局概念：常量、路径、状态归宿、构建变体 | 查表篇，不参与教学序 | F |

### 4.2 阅读路径

- **主线（必读，按执行序）**：00 → 01 → 02 → 03 → 04 → 05 → 09 → 10 → 11 → 12 → 13 → 15 → 16 → 17
- **支线（按需，可跳）**：06（旋钮）、07 + 14（账本）、08（对外契约）、18（测试）
- **可跳读 / 查表**：99
- **最短路径（只想知道 init 干什么）**：00 → 02 → 15 → 17

### 4.3 并行体的组织规则

本 stage **没有真正的并行体**（七个状态是线性状态机，不是几十个并列成员）。
唯一具有并行特征的是**外部对端**（kernel/VM/RS/PM/procfs/tty/power/命令），
按"汇聚点 + 触发时机"组织在 08 篇内分三组：

1. **boot 期对端**（谁让 init 出生）：kernel 表、VM 加载、RS 登记、PM 父子关系；
2. **运行期对端**（谁在运行中找 init）：键盘驱动 Ctrl-Alt-Del、电源驱动低电、PM 孤儿收养；
3. **查询期对端**（谁在观察 init）：procfs `service_active`、shutdown 命令、kill 命令（运维）。

代表成员精讲：`sys_kill(INIT_PROC_NR, SIGABRT)`（`keyboard.c:300`）讲透"驱动怎样通过一个
信号号改变系统运行级"，其余按差异表收束。

---

## 5. 每篇契约

> 格式：定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 / 验收标准。

### 00-init-overview：init 的定位与 boot 链终点

- **一句话定位**：回答"init 是什么、在 boot 链的哪一环、为什么它不是服务、全套文档怎么读"。
- **讲什么**：K-001、K-002、K-003、K-004、K-005、K-008、K-009
- **不讲什么**：一切机制细节（01~18）；/etc/rc 脚本体系（交 `../18-stage-commands/01-init-rc-scripts.md`）；
  crt0 与运行时启动（交 `../14-stage-runtime/`）；boot 镜像与页表建立（交 `../01-stage-kernel/`、`../02-stage-vm/`）
- **前置**：无（声明读者需具备"进程/信号/文件描述符"基础常识）
- **后置**：01~18 全部
- **事实底线**：
  - C：`minix3/sbin/init/init.c:133-139`（七状态）、`minix3/minix/kernel/table.c:64`、
    `minix3/minix/servers/rs/table.c:28`、`minix3/minix/servers/vm/main.c:331,345,514`、
    `minix3/minix/servers/pm/main.c:188-195,203-204`、`minix3/minix/fs/procfs/service.c:195-207`
  - 制品：`minix3/sbin/init/init.8:47-165`（状态表与职责）、`minix3/sbin/init/NOTES:85-119`（设计笔记）
  - Rust：`os/commands/sbin/init/src/main.rs:33-125`（接线）、`driver.rs:39-201`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-001 | boot 链终点 | 架构 | `table.c:64` | 定位的第一事实 | 存量 00/01/14 |
  | K-002 | USR_F 登记 | 接口 | `rs/table.c:28` | 身份判断依据 | 存量 00/14 |
  | K-003 | 非服务执行模型 | 概念 | `init.c:1535-1563` | 与其它 stage 划界 | 存量 00/plan §1.2 |
  | K-004 | 固定 argv 与 ELF 加载 | 接口 | `vm/main.c:331,345,514` | 出生条件 | 存量 01/14 |
  | K-005 | PM 父子与 PID 1 | 接口 | `pm/main.c:188-195` | PID 1 语义 | 存量 01/14 |
  | K-008 | 七状态与主线图 | 概念 | `init.c:133-139` | 全局路线图 | 存量 00/02 |
  | K-009 | 会话次主线 | 概念 | `init.c:1142-1497` | 导航用 | 存量 00 |

- **验收标准**：读者合上本篇能画出 boot 链到 init 的四跳（kernel 表 → VM 加载 → PM 登记 → 调度），
  说出 init 与 VM/PM 服务在执行模型上的唯一差别（无 IPC 主循环），并指出七状态图的三个入口
  （默认 runcom、`-s`、设备缺失降级）与三个出口（death→single_user、catatonia→multi_user、clean_ttys→multi_user）。
  必须画出一张状态转换图（含触发信号名）。

### 01-init-main-entry：入口与出生证明八步

- **一句话定位**：`main()` 的每一步为什么必须发生，以及哪一步失败会怎样。
- **讲什么**：K-010、K-011、K-012、K-013、K-014、K-015、K-016、K-017、K-018、K-019、K-020（调用点）、K-021（调用点）、K-022、K-023、K-024、K-025、K-179
- **不讲什么**：信号 handler 语义（交 03）；日志通道机制（交 04）；securelevel 机制（交 06）；
  `transition` 循环（交 02）；MAKEDEV 脚本内部（交 `../18-stage-commands/`）
- **前置**：00
- **后置**：02、06、09
- **事实底线**：`init.c:229-367`（main）、`init.c:1703-1788`（mfs_dev）、`init.c:151,195,217`（默认全局）、
  `pathnames.h:39-40`；Rust `main.rs:33-125`、`entry.rs`
- **知识点清单**（摘录关键行，完整见 §2.1 K-010~K-025、K-179）

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-010 | boot_time 采样 | 数据 | `init.c:238` | main 首行动作 | 新增（C 源码） |
  | K-011 | 身份双校验 | 约束 | `init.c:242-249` | 出生证明之一 | 存量 01 |
  | K-012 | setsid 与宽容失败 | 机制 | `init.c:255-256` | 出生证明之二 | 存量 01 |
  | K-014 | mfs_dev | 机制 | `init.c:1703-1788` | Minix 移植层 | 存量 01 |
  | K-017 | getopt "sf" | 机制 | `init.c:287-303` | 参数决策 | 存量 01 |
  | K-019 | close(0/1/2) | 机制 | `init.c:339-341` | 出生证明之三 | 存量 01 |
  | K-179 | 参数来自 crt0 | 接口 | `main.rs:66-69` | 与 runtime stage 的分界 | 新增（Rust） |

- **验收标准**：能按 M1~M15 顺序复述 main 的每一步并给出行号；能解释"为什么 `-s` 在正常 boot
  路径不会出现"（`vm/main.c:345` 固定 argv）；能说出三处"失败只 warning 继续"的位置
  （setsid、未知参数、mfs_dev 的 warn）及其共同理由；Rust 侧能指出 `parse_boot_args`
  与 `decide_entry` 的分工（解析纯、决策纯、执行薄）。

### 02-init-state-machine：状态机骨架与分派

- **一句话定位**：七个字符、一个永循环、一张分派表——状态机的骨架。
- **讲什么**：K-027、K-028、K-029、K-030、K-031、K-032、K-033、K-034
- **不讲什么**：各状态函数体（09~11、15~17）；信号注册与 handler（03）；runlevel 台账机制（07）
- **前置**：00、01
- **后置**：03、09、15
- **事实底线**：`init.c:130-139`、`init.c:624-640`、`init.c:141-149`、`init.c:201`；
  Rust `state_machine.rs`、`driver.rs:39-201`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-027 | 递归 typedef | 数据结构 | `init.c:130-131` | 骨架类型 | 存量 02 |
  | K-028 | 七状态宏 | 数据 | `init.c:133-139` | 骨架词汇 | 存量 02 |
  | K-029 | transition 三步 | 机制 | `init.c:624-640` | 主循环 | 存量 02 |
  | K-030 | current_state 初值 | 数据 | `init.c:201` | 首条台账成因 | 存量 02/99 |
  | K-031~K-033 | Rust 分派与单所有者 | 演进 | `driver.rs` | 骨架的 Rust 形态 | 存量 02 |

- **验收标准**：能默写 `transition` 的七行并解释每行的顺序不可交换（先记台账后推进）；
  能说明 `current_state` 初值为 `death` 导致首条 runlevel 记录是 `d→r`；
  能说清 C 的函数指针分派与 Rust `match` 分派的等价性，以及 `DriverState` 为什么只有一个所有者。

### 03-init-signal-bridge：信号、handler 与异步请求桥

- **一句话定位**：信号只许写请求，主循环只认请求——异步与同步之间唯一的桥。
- **讲什么**：K-020（机制）、K-035、K-036、K-037、K-038、K-039、K-040、K-041、K-042、K-043、K-044、K-045、K-046
- **不讲什么**：minixreboot/minixpowerdown 的 fork/exec 内容（交 08）；disaster 的日志（交 04）；
  各状态如何认领请求（交 09/15/16/17）
- **前置**：02
- **后置**：08、09、15、16、17
- **事实底线**：`init.c:369-405`（handle/delset）、`init.c:315-334`（注册顺序）、
  `init.c:1502-1522`、`init.c:1649-1655`、`init.c:855-856,955-956`（sigsuspend 死等）；
  信号编号 `os/libs/minix-types/src/types/signal.rs`；Rust `signal_state.rs`、`host.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-035/K-037 | handle/delset | 机制 | `init.c:369-405` | 注册机制本体 | 存量 02 |
  | K-036 | SA_NOCLDSTOP 与 XXX SA_RESTART | 约束 | `init.c:380-385` | 诚实转述未决 | 存量 02 |
  | K-039 | transition_handler 三映射 | 机制 | `init.c:1502-1522` | 桥的写法侧 | 存量 02 |
  | K-041 | 异步写/同步认领 | 概念 | `init.c:825,1535,1559` | 桥的概念侧 | 存量 02 |
  | K-042 | 信号编号权威 | 数据 | `signal.rs` | 编号表 | 存量 02/14 |
  | K-043 | SignalState 与 spawn 移出 | 演进 | `signal_state.rs` | 自觉偏离 | 存量 02/14 |
  | K-046 | sigsuspend 死等 | 机制 | `init.c:855-856` | 状态函数不返回 | 存量 04/05 |

- **验收标准**：能列出 Minix 分支五组 handler 与各自信号号（含 SIGABRT=6、SIGUSR1=30）；
  能解释"信号上下文里只写变量"的三条理由；能说出 `default: requested_transition = 0` 的含义；
  能指出 Rust 把 `minixreboot` 的 fork 挪出信号上下文是机制层偏离而外部次序不变（可观察次序一致）。

### 04-init-logging-failure：日志与致命故障

- **一句话定位**：四级日志的分工，以及 init 崩溃时为什么要"留 30 秒给人看再自杀"。
- **讲什么**：K-047、K-048、K-049、K-050、K-051、K-052、K-053、K-054、K-055
- **不讲什么**：状态转换（02）；重启挂钩（08）；各状态的 stall 调用点清单（散在各状态篇）
- **前置**：02、03
- **后置**：09、10、15
- **事实底线**：`init.c:440-450,457-466,472-481,504-511`、`init.c:95`（STALL_TIMEOUT）、
  `init.c:438,455,470`（NB 注释）；排除项 `init.c:411-432`（`#if 0`）、`init.c:490-498`（非 Minix）；
  Rust `log.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-047~K-049 | stall/warning/emergency | 机制 | `init.c:440-481` | 三级分工 | 存量 03 |
  | K-050 | disaster | 机制 | `init.c:504-511` | 致命路径 | 存量 03 |
  | K-054 | session logger NB | 约束 | `init.c:438` | 诚实缺口 | 存量 03 |
  | K-052/K-053 | syslog 缺口与 Severity | 演进 | `log.rs` | ARCH A-3 | 存量 03 |

- **验收标准**：能按"睡不睡 + 级别"二维表区分四个函数；能解释 `_exit(sig)` 的退出码即信号号、
  以及它为何等价于请求重启；能列出两处被明确排除的函数并给出排除依据（`#if 0` / 非 Minix 分支）。

### 05-init-process-model：子进程模型、等待与退出码（新建）

- **一句话定位**：所有状态函数共用的那套 fork/exec/waitpid 骨架，以及 16 个 `_exit` 码的对照表。
- **讲什么**：K-056、K-057、K-058、K-059、K-060、K-061、K-062、K-063、K-064；并承接 K-095/K-105/K-113 的"退出码"侧
- **不讲什么**：哪个状态 fork 什么（09/10/15）；口令门（09）；窗口系统细节（15）
- **前置**：02、03、04
- **后置**：09、10、11、15、17
- **事实底线**：`init.c:732,887,1331`（fork 三处）、`init.c:827,932,1560,1687`（waitpid 四处）、
  `init.c:790-791,902,1306,1362`（解屏蔽）、16 处 `_exit`（见 §0.3）、`wait.h:53-70`；
  Rust `wait.rs`（8 行，类型已上移 minix-types）、`session.rs::ParsedCommand`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-056 | fork 三分支 | 机制 | `init.c:732,887,1331` | 骨架 | 新增（C 源码汇总） |
  | K-057 | waitpid 选项差异 | 约束 | `init.c:827,932,1560,1687` | 四处对比 | 存量 04/09 |
  | K-058 | WaitStatus 分类 | 数据结构 | `wait.h:53-70` | 判定基础 | 存量 04 |
  | K-060 | _exit 编码总表 | 数据 | `init.c` 16 处 | 唯一总表 | 新增（C 源码汇总） |
  | K-062 | exec_path 与 argv[0] | 演进 | `init.c:899 vs 521` | 两种形状 | 存量 05/07/14 |
  | K-063 | WaitStatus 上移 | 演进 | `wait.rs` | 类型归属 | 新增（Rust/todo P1-4） |

- **验收标准**：给出一张完整的 `_exit` 码表（0/1/2/3/4/5/6/7/8/9/10/11/12 + sig），
  每格写"位置、触发条件、父进程看到的结果"；能回答"为什么稳态的 waitpid 看不到暂停的子进程"；
  能说出 EINTR 与 ECHILD 在等待循环中的两种不同处理（重试 vs 收工）。

### 06-init-sysctl-interaction：securelevel 与 init.root

- **一句话定位**：init 向内核要的两个旋钮：写保护档位与第二遍启动的新根。
- **讲什么**：K-065、K-066、K-067、K-068、K-069、K-070、K-071、K-072、K-073、K-074
- **不讲什么**：内核 sysctl 实现（交 `../01-stage-kernel/`）；rc 脚本如何使用新根（交 10）
- **前置**：01（探测调用点）、05（chroot 在子进程里）
- **后置**：09、10、11、15
- **事实底线**：`init.c:544-563,568-618,1811-1857,1859-1900`、`init.c:723-725,1543-1544`（消费点）、
  `init.c:1792-1806`（ttys 路径）、`init.c:1340-1348`（getty chroot）；
  Rust `sysctl.rs`、`host.rs:107-120,158-160`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-065~K-066 | has/get/set | 机制 | `init.c:544-618` | 旋钮读写 | 存量 12 |
  | K-067 | 降 0 / 升 1（==0 才升） | 约束 | `init.c:723-725,1543-1544` | 档位策略 | 存量 04/09 |
  | K-068~K-069 | 节点创建与 shouldchroot | 机制 | `init.c:1811-1900` | 新根判据 | 存量 12 |
  | K-071~K-072 | 三处 chroot 消费 | 机制 | `init.c:1792-1806,1340-1348` | 消费点收敛 | 存量 06/12 + 新增 K-072 |
  | K-073~K-074 | defer 缺口与接缝 | 演进 | `sysctl.rs`/`host.rs` | ARCH A-4/A-5 | 存量 12 |

- **验收标准**：能画出"节点创建（main）→ 读值（runcom）→ 两处 chroot 消费（rc 子进程、getty 子进程）
  → ttys 路径拼接"的完整数据流；能说出 `-1` 档位的含义（管理员说别动）；
  能列出 `_exit(4)` 与 `_exit(7)` 两个 chroot 失败码的位置差异。

### 07-init-utmp-ledger：会话台账（上）——记录与 runlevel

- **一句话定位**：`who`/`last` 看到的那本账：记录类型、字段构造与状态变迁记录。
- **讲什么**：K-075、K-076、K-077、K-078、K-079、K-080、K-081、K-082、K-083、K-084、K-085
- **不讲什么**：会话结构（12）；DB（13）；会话退出时的挂载细节（14）
- **前置**：02（七状态字符）、05（`_exit` 与退出状态）
- **后置**：10、14、17
- **事实底线**：`init.c:1383-1409`（make_utmpx）、`init.c:1411-1427`（get_runlevel）、
  `init.c:1429-1451`（utmpx_set_runlevel）、`init.c:1229-1247`（BOOT/DOWN）、`init.c:1008,1674`（logwtmpx）、
  `minix3/include/utmpx.h:39-40,57-64,74`、`minix3/sbin/init/Makefile:8`（双开宏）；Rust `utmp.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-076 | 六种记录类型 | 数据 | `utmpx.h:57-64`、`init.c:1233,1244` | 账本科目 | 存量 13 + 新增 BOOT/DOWN |
  | K-077 | make_utmpx 字段 | 机制 | `init.c:1383-1409` | 记录构造 | 存量 13 |
  | K-078/K-080 | runlevel 映射与 RUNLVL_MSG | 机制/数据 | `init.c:1411-1451` | 状态与字符的桥 | 存量 13 |
  | K-079 | sessions==NULL 短路 | 约束 | `init.c:1438-1441` | 时机克制 | 存量 13 |
  | K-084 | BOOT/DOWN 记录 | 机制 | `init.c:1229-1247` | 开机停机来源 | 新增（C 源码） |
  | K-085 | 双通道两步写顺序 | 约束 | `init.c:654-658` | 写顺序 | 新增（C 源码） |

- **验收标准**：能列出 init 实际写入的六种记录类型及其触发时机；能解释 `ut_id` 取行尾的规则；
  能回答"为什么第一次进 read_ttys 之前不记 runlevel"（/var 尚未可写）；
  能说出 on-disk 编码为何是临时态（无共享消费者，ARCH A-2 残余）。

### 08-init-external-contracts：对外契约——谁在什么时候找 init

- **一句话定位**：认识 init 的人越多，它的契约越不能变；本篇清点全部对端与 init 死亡的后果。
- **讲什么**：K-004（boot argv 侧）、K-006、K-007、K-086、K-087、K-088、K-089、K-090、K-091、K-092、K-093、K-094、K-095、K-096
- **不讲什么**：handler 如何写请求（03）；shutdown 命令内部（交 `../18-stage-commands/`）；
  PM 进程管理实现（交 `../04-stage-pm/`）
- **前置**：03（信号）、05（fork/exec）
- **后置**：无（终态篇，被 00 摘要引用）
- **事实底线**：`init.c:517-538`；`keyboard.c:300`；`tps65217.c:226`；`pm/forkexit.c:336,396`；
  `pm/misc.c:224`；`pm/main.c:203-204`；`pm/schedule.c:34,73`；`procfs/service.c:195-207`；
  `vm/main.c:345`；`init.8:54-61,310-317`；Rust `contracts.rs`、`host.rs`（接缝总述）
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-086/K-087 | 两个 Minix 挂钩 | 接口 | `init.c:517-538` | 信号转进程 | 存量 14 |
  | K-088/K-089 | 两个驱动源 | 接口 | `keyboard.c:300`、`tps65217.c:226` | 谁发信号 | 存量 14 |
  | K-090/K-091/K-092 | PM 三契约 | 接口 | `pm/forkexit.c:336,396`、`pm/misc.c:224` | 亲属关系 | 存量 14 |
  | K-093/K-094 | init 死亡后果与候选路径 | 约束 | `init.8:54-61,310-317` | 契约底线 | 新增（man page） |
  | K-096 | InitHost 单一接缝总述 | 演进 | `host.rs:1-54` | 架构主脊 | 存量 01/02/03/12 |

- **验收标准**：能对三组对端（boot 期/运行期/查询期）各举出至少一个带行号的证据；
  能解释"init 死 = 系统重启"这条契约与 `disaster` 的 `_exit(sig)` 如何咬合；
  能说清 `InitHost` 为什么只有一个（消除单实现 trait 的形状）以及 `SessionDb` 为何留在接缝外。

### 09-init-single-user：状态 's'——单用户抢修

- **一句话定位**：口令门 + shell + 看护循环，退出即前进。
- **讲什么**：K-097、K-098、K-099（调用点）、K-100、K-101（调用点）、K-102、K-103、K-104、K-105、K-106、K-107、K-108、K-109
- **不讲什么**：`/etc/rc`（10）；`collect_child` 的完整机制（15，本篇只讲"首次启动时它对循环无影响"）；
  securelevel 机制（06）；setctty 机制（15）
- **前置**：03、04、05、06
- **后置**：10
- **事实底线**：`init.c:694-874`；`init.c:736-739`（setctty 调用点）；`init.c:813-823`（fork 失败不睡，与 10 对比）；
  Rust `single_user.rs`、`password.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-102 | 口令门三条件 | 机制 | `init.c:749-750` | 条件信任 | 存量 04 |
  | K-103 | ^D → _exit(0) | 约束 | `init.c:755-756` | 放弃抢修 | 存量 04 |
  | K-107 | 看护五结局 | 机制 | `init.c:836-873` | 循环出口 | 存量 04 |
  | K-108 | fork 失败不睡 | 约束 | `init.c:813-823` | 与 runcom 对比 | 存量 04/05 |
  | K-100 | SIG_IGN 窗口 | 机制 | `init.c:727-731` | 临时忽略与恢复 | 存量 04 |

- **验收标准**：能列出看护循环的五种结局及其判定次序；能解释 `^D` 与 shell 退出码 1 为何走同一条
  FASTBOOT 之路（C 只看 WIFEXITED）；能指出本篇 fork 失败**不睡**而 runcom 睡 30 秒的差异；
  能说明口令门在 libcrypt 后端缺失时的行为（一律拒绝，管理员代价是 `^D` 走多用户）。

### 10-init-runcom：状态 'r'——/etc/rc 与两遍启动

- **一句话定位**：脚本是黑盒，退出码是唯一协议；新根里可能还要跑第二遍。
- **讲什么**：K-110、K-111、K-112、K-113、K-114、K-115、K-116、K-117（调用点）、K-118
- **不讲什么**：/etc/rc 脚本体系与 rc.d/rcorder（交 `../18-stage-commands/01-init-rc-scripts.md`）；
  `shouldchroot` 机制（06）；台账机制（07）
- **前置**：05、06、09
- **后置**：11
- **事实底线**：`init.c:974-1014`、`init.c:879-969`、`init.c:897-900`（argv）、`init.c:1005-1012`；
  Rust `runcom.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-111 | argv 组装 | 机制 | `init.c:897-900` | fastboot 少一参 | 存量 05 |
  | K-114 | 五归宿 | 机制 | `init.c:917-968` | 出口表 | 存量 05 |
  | K-115 | 两遍 rc | 机制 | `init.c:990-998` | chroot 第二遍 | 存量 05 |
  | K-118 | runcom/runetcrc 双层 | 演进 | `runcom.rs` | 尝试 vs 策略 | 存量 05 |

- **验收标准**：能说出"静默等重启"需要哪**两个**条件同时成立（SIGTERM + `requested_transition==catatonia`）；
  能解释 argv[2] 在 fastboot 时为空指针（只传两个参数）；能指出 fork 失败路径睡 30 秒而 single_user 不睡；
  能说明本篇与 `18-stage-commands/01` 的边界（本篇只讲 init 如何调用脚本，不讲脚本内容）。

### 11-init-read-ttys：状态 't'——/etc/ttys 解析与链表重建

- **一句话定位**：把四列文本翻译成内存链表，先清空再重建。
- **讲什么**：K-119、K-120、K-121、K-122、K-123、K-124、K-125、K-071（chroot 路径调用点）、K-084（BOOT 记录调用点）
- **不讲什么**：会话节点结构（12）；DB（13）；台账机制（07）；窗口系统（15）
- **前置**：06、07、05
- **后置**：12
- **事实底线**：`init.c:1222-1285`、`init.c:1792-1806`（do_setttyent）、`minix3/include/ttyent.h:57-58`、
  `minix3/lib/libc/gen/getttyent.c`（skip/scmp/fparseln）、`minix3/etc/ttys`（golden 源）；Rust `ttys.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-119 | 四列语义与标志 | 数据 | `ttyent.h:57-58` | 表格式 | 存量 06 |
  | K-122 | 字段语义三坑 | 机制 | `getttyent.c` skip/scmp | 解析器坑 | 存量 06 |
  | K-123 | 短行不拒绝 | 约束 | `init.c:1147` | 解析/过滤分离 | 存量 06 |
  | K-121 | DB 失败分叉 | 约束 | `init.c:1262-1271` | chroot 决定去向 | 存量 06 |
  | K-125 | golden 数据来源 | 工具 | `minix3/etc/ttys` | 真实行 | 存量 06 |

- **验收标准**：能解释引号是"模式开关"而非字段前缀、token 是精确匹配（`ondemand` 不是 `on`）、
  `off` 是显式清除且按序生效；能用 `minix3/etc/ttys` 的真实行（含引号 getty 与空 getty）走一遍解析；
  能说出 chroot 过之后 DB 失败为什么去 death 而不是 single_user。

### 12-init-session-model：会话对象与命令行分词

- **一句话定位**：/etc/ttys 的一行在内存里的样子：设备 + 两条命令向量 + 两个标志。
- **讲什么**：K-126、K-127、K-128、K-129、K-130、K-131、K-132、K-133
- **不讲什么**：DB（13）；启动与防抖动（15）；重读 diff（16）
- **前置**：11
- **后置**：13、14、15、16
- **事实底线**：`init.c:156-170`（session_t）、`init.c:1101-1118`（construct_argv）、
  `init.c:1123-1137`（free_session）、`init.c:1142-1180`（new_session）、`init.c:1185-1217`（setupargv）；
  Rust `session.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-126 | 十字段 | 数据结构 | `init.c:156-170` | 节点全貌 | 存量 07 |
  | K-128 | 四关过滤 | 机制 | `init.c:1142-1180` | 建节点失败路径 | 存量 07 |
  | K-130 | 分词器 | 机制 | `init.c:1101-1118` | 分词规则 | 存量 07 |
  | K-132 | se_started | 数据 | `init.c:159` | 防抖动字段 | 存量 07/09 |

- **验收标准**：能画出 session_t 的字段表并指出 `se_process==0` 表示空闲；
  能说明两条命令向量的 exec 形状与 rc/shutdown 不同（路径就是第一个词）；
  能列出 `new_session` 返回 NULL 的四条路径。

### 13-init-session-db：会话数据库 pid → session

- **一句话定位**：waitpid 只给 pid，init 需要按 pid 反查是哪条线路。
- **讲什么**：K-134、K-135、K-136、K-137、K-138；K-141 的挂载点位置（机制在 14）
- **不讲什么**：节点结构（12）；台账记录内容（07/14）；回收逻辑（15）
- **前置**：12
- **后置**：14、15
- **事实底线**：`init.c:1021-1096`；Rust `session_db.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-135 | dbopen(NULL) 内存表 | 机制 | `init.c:1027` | 无持久化 | 存量 08 |
  | K-136 | start/add/del/find | 机制 | `init.c:1021-1096` | 四函数 | 存量 08 |
  | K-137/K-138 | ARCH A-1 与 trait 保留 | 演进 | `session_db.rs` | 依赖消除 | 存量 08 |

- **验收标准**：能解释"关旧开新"的语义与它在 read_ttys 重建时的作用；
  能说出三处静默返回（DB 未开时 add 静默、find 未命中返回空、del 失败只记 emergency）
  以及它们共同的哲学（能修就修）；能说明 `SessionDb` 为何是唯一留在接缝外的 trait。

### 14-init-session-ledger-mounts：会话台账（下）——挂载点

- **一句话定位**：会话生与死时，账本上各写哪两笔。
- **讲什么**：K-139、K-140、K-141、K-142、K-143
- **不讲什么**：记录类型与字段（07）；DB 四函数（13）；会话结构（12）
- **前置**：07、12、13
- **后置**：15
- **事实底线**：`init.c:1372-1381`（session_utmpx）、`init.c:647-662`（clear_session_logs）、
  `init.c:1054-1056,1072-1074`（add/del 挂钩）、`init.c:654-661`（双通道）；Rust `utmp.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-139 | session_utmpx 三选一 | 机制 | `init.c:1372-1381` | 登录/死亡记录 | 存量 13 |
  | K-140 | clear_session_logs 两步 | 机制 | `init.c:647-662` | 关闭会计文件 | 存量 13 |
  | K-142 | 双通道四函数 | 约束 | `init.c:654-661` | UTMP+UTMPX 并存 | 存量 13 |
  | K-143 | 台账回调注入 | 演进 | `runcom.rs`/`shutdown.rs` | 接线方式 | 存量 05/11/13 |

- **验收标准**：能说清"DEAD 进 utmpx、成功才进 wtmpx"的两步顺序及其失败语义；
  能指出 add/del_session 各自的台账挂钩位置；能解释 `record_reboot`/`record_shutdown`
  如何由调用方接到 `logwtmpx`。

### 15-init-multi-user：状态 'm'——多用户稳态

- **一句话定位**：启动全部，然后睡觉；醒来处理，再睡回去。
- **讲什么**：K-144、K-145、K-146、K-147、K-148、K-149、K-150、K-151、K-152、K-153、K-154、
  K-101（setctty 机制）、K-072（getty chroot 调用点）、K-132（se_started 消费点）
- **不讲什么**：重读终端表（16）；关停（17）；会话节点字段定义（12）
- **前置**：05、06、12、13、14
- **后置**：16、17
- **事实底线**：`init.c:1528-1564`、`init.c:1321-1370`（start_getty）、`init.c:1290-1316`（窗口）、
  `init.c:669-689`（setctty）、`init.c:1460-1497`（collect_child）、`init.c:92-98`（常量）；
  Rust `multi_user.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-149 | setctty 四步 | 机制 | `init.c:669-689` | 成人礼 | 存量 09 |
  | K-147 | start_getty 次序 | 机制 | `init.c:1321-1370` | 子进程次序 | 存量 09 |
  | K-150 | collect_child 四动作 | 机制 | `init.c:1460-1497` | 回收主讲述点 | 存量 09 |
  | K-153 | 不带 WUNTRACED | 约束 | `init.c:1560` | 与救援态相反 | 存量 09 |
  | K-147 | 防抖睡在子进程里 | 机制 | `init.c:1350-1355` | 拖住 exec | 存量 09 |

- **验收标准**：能说清三处"最容易读漏"的 C 语义：稳态 waitpid 不带 WUNTRACED、
  安全级是 `==0 则升 1` 而非大于 0、防抖动的睡眠发生在子进程里；
  能列出 `collect_child` 的四动作及其判定次序；能解释窗口系统是 getty 子进程里的再一次 fork。

### 16-init-clean-ttys：状态 'T'——重读终端表与 diff

- **一句话定位**：用存在性标记回答"还在、新来、消失"三问。
- **讲什么**：K-155、K-156、K-157、K-158、K-159、K-160、K-161
- **不讲什么**：会话创建机制（12）；回收摘链（15）；台账（14）
- **前置**：12、15
- **后置**：无（被 15 回指）
- **事实底线**：`init.c:1569-1629`（含 `init.c:1567` 的 n² 自嘲注释）；Rust `clean_ttys.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-155 | PRESENT 三问 | 概念 | `init.c:1577-1626` | 算法灵魂 | 存量 10 |
  | K-158 | index 变化只警告 | 约束 | `init.c:1592-1597` | utmp 索引漂移 | 存量 10 |
  | K-161 | 读不到即全体退役 | 约束 | `clean_ttys.rs` | 空文件行为 | 存量 10 |

- **验收标准**：能按"清标记 → 逐行对 → 收尾关停"三步复述算法；能解释为何序号变化只警告不重启 getty；
  能说出"真正的摘链发生在 15 的回收路径，本篇只标记不释放"。

### 17-init-shutdown：状态 'c'/'d'——假死与关机

- **一句话定位**：假死是关登录，真死是三轮升信号杀全家。
- **讲什么**：K-162、K-163、K-164、K-165、K-166、K-167
- **不讲什么**：单条会话回收细节（15）；台账字段（07/14）
- **前置**：03（clang/alarm）、15
- **后置**：09（闭环回 single_user）
- **事实底线**：`init.c:1634-1643`、`init.c:1661-1698`、`init.c:96`（DEATH_WATCH=10）、
  `init.c:1667`（death_sigs）；Rust `shutdown.rs`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-163 | 三轮 kill + alarm | 机制 | `init.c:1661-1698` | 真死流程 | 存量 11 |
  | K-164 | ESRCH/ECHILD 早退 | 约束 | `init.c:1681-1692` | 提前收工 | 存量 11 |
  | K-166 | 关机台账先写 | 机制 | `init.c:1674` | 顺序 | 存量 11/13 |

- **验收标准**：能按轮次复述 death 的每一步（置标记 → 写台账 → 三轮 kill → alarm(10) → 收割 → 早退判据）；
  能区分 ESRCH 与 ECHILD 两个早退条件；能说出唯一返回态是 single_user 及其闭环含义。

### 18-init-testing：测试与验证基建（新建）

- **一句话定位**：init 这种"全程系统调用"的程序，怎么在没有真机的情况下被测出来。
- **讲什么**：K-168、K-169、K-170、K-125（golden 源）、K-171、K-172、K-173
- **不讲什么**：各篇的具体测试清单（留在各篇 §5）；业务机制本身
- **前置**：05（剧本宿主的 fork 回放）、15（主循环可步进）
- **后置**：无
- **事实底线**：`os/commands/sbin/init/src/host.rs`（ScriptHost）、`driver.rs:376-539`（boot_chain 测试）、
  `single_user.rs`（catch_unwind 断言）、`minix3/etc/ttys`（golden 源）、`todo.md §1`（clippy 残余表与等待态）；
  本轮实测：`cargo test -p minix-init` → 135 passed / 1 failed（见 §0.3）
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-168 | 三层测试 | 工具 | `driver.rs` 测试 | 分层 | 新增（Rust 实测） |
  | K-169 | ScriptHost | 工具 | `host.rs` | 单实现 trait 的消解 | 新增（Rust） |
  | K-170 | 发散断言 | 工具 | `single_user.rs` | `_exit` 怎么测 | 存量 04 |
  | K-171 | 实测基线 | 工具 | §0.3 | 数字纪律 | 新增（本轮实测） |
  | K-173 | 等待态清单 | 约束 | `todo.md §2` | 缺口归属 | 存量 todo |

- **验收标准**：能说清三层测试各自的被测对象与代表测试名；能解释"fork 返回两次"在剧本宿主里
  如何变成"脚本回放两次"、子进程 `_exit` 如何用 `catch_unwind` + exits 队列断言；
  本篇必须给出**实测命令与输出**，禁止转述历史数字；必须列出当前失败项及其归属（E-INITSYS 门控面）。

### 99-init-global-concepts：常量、路径、状态归宿与构建变体

- **一句话定位**：查表篇——读者随时回来查，不参与教学序。
- **讲什么**：K-042（编号表索引）、K-060（退出码索引）、K-026（死项排除）、K-018、K-174、K-175、K-176、K-177、K-178
- **不讲什么**：一切机制解释（只给值、给位置、给去向）
- **前置**：无（声明为查表篇）
- **后置**：无
- **事实底线**：`init.c:92-108`（常量）、`init.c:151-211`（全局）、`minix3/sbin/init/pathnames.h`、
  `minix3/include/paths.h:62-63,121,125`、`minix3/include/utmpx.h:39-40`、`minix3/include/utmp.h:42-43`、
  `minix3/sbin/init/Makefile:8-17`、`os/commands/sbin/init/Cargo.toml`
- **知识点清单**

  | 编号 | 名称 | 类型 | 锚点 | 归本篇理由 | 来源 |
  |---|---|---|---|---|---|
  | K-174/K-175 | 常量与路径表 | 数据 | `init.c:92-108`、`pathnames.h` | 查表 | 存量 99 |
  | K-176 | C 全局 → Rust 归宿 | 数据 | `init.c:151-211` | 状态归宿 | 存量 99 |
  | K-177 | 构建变体 → feature | 演进 | `Makefile:8-17` | ARCH A-7 | 存量 99/plan |
  | K-178 | crate 形态 | 演进 | `Cargo.toml` | no_std 裁决 | 新增（Rust/todo） |
  | K-026 | 死项排除表 | 约束 | 多处 | 不必读的代码 | 存量 plan §5.4 |

- **验收标准**：常量表每一行都能给出 C 锚点与 Rust 落点；退出码表与信号表与 05/03 完全一致（同一事实不两写，
  本篇只索引）；死项排除表列出四项（_PATH_SLOGGER、INIT_MOUNT_MFS、print_console、badsys）及排除依据。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---|---|---|---|---|---|---|
| C-01 | 重写（加厚） | `00-init-overview.md`（25 行） | `00-init-overview.md` | 25 行承载不了"定位 + boot 链 + 状态图 + 导航 + 与相邻 stage 边界" | K-001~K-009 | 存量全部保留；新增边界声明（§5 00 不讲什么） |
| C-02 | 原样迁移 | `01-init-main-entry.md` | `01-init-main-entry.md` | 编号与内容均符合执行序 | K-010~K-025、K-179 | 存量原地；新增 K-010、K-179；K-020/K-021 降为调用点 |
| C-03 | **拆分** | `02-init-state-machine.md` | `02-init-state-machine.md` + `03-init-signal-bridge.md` | 旧 02 一篇同时是"概念教学（状态机）"与"机制手册（信号注册与桥）"，违反单篇单语义；且信号是横切关注点，被 08/09/15/16/17 共同依赖 | K-027~K-034 留 02；K-035~K-046 出 03 | 存量去向：信号四机制 → 03 §2；Rust SignalState/SignalSpec → 03 §4；新增：无（纯搬迁） |
| C-04 | 重排（前移） | `03-init-logging-failure.md` | `04-init-logging-failure.md` | 仅为让编号连续占用（信号独立成篇后顺延） | K-047~K-055 | 存量原样 |
| C-05 | **新建** | — | `05-init-process-model.md` | fork/waitpid/退出码在旧 04/05/09/11 四篇重复展开且无 `_exit` 总表（G-01/G-04） | K-056~K-064、K-060 | 存量抽取：旧 04 §2.4 的 wait 语义、旧 05 §2.3、旧 09 §2、旧 11 §2 的等待循环；新增：K-056、K-060、K-063（C 源码汇总 + Rust wait.rs） |
| C-06 | **重排（大幅前移）** | `12-init-sysctl-interaction.md` | `06-init-sysctl-interaction.md` | 旧 12 在末尾，但它被 01（探测）、09（降级）、10（chroot）、15（升 1）四处调用；前移后调用点读者不必跳到目录尾部，且符合 C 真序（M13/M14 在 transition 之前） | K-065~K-074 | 存量原样搬移；新增 K-072（`_exit(7)`，C 源码 `init.c:1340-1348`） |
| C-07 | **拆分 + 重排** | `13-init-utmp.md` | `07-init-utmp-ledger.md`（记录与 runlevel）+ `14-init-session-ledger-mounts.md`（会话挂载） | 旧 13 内部有依赖环：`session_utmpx` 需要 session_t（旧 07），而 `get_runlevel`/`utmpx_set_runlevel` 只需要状态字符（旧 02）。拆开后前半可前移到基础设施层，后半留在会话模型之后，消除"读 13 需要先读 07 再回头"的往返 | K-075~K-085 → 07；K-139~K-143 → 14 | 存量去向：make_utmpx/get_runlevel/utmpx_set_runlevel/logwtmpx → 07 §2；session_utmpx/clear_session_logs/add-del 挂钩 → 14 §2；新增：K-084（BOOT/DOWN，`init.c:1229-1247`）、K-085（两步写，`init.c:654-658`） |
| C-08 | **重排（大幅前移）** | `14-init-external-contracts.md` | `08-init-external-contracts.md` | 旧 14 在末尾，但它讲的是 init 的**身份与对端**，只依赖信号（03）与 fork/exec（05），不依赖任何状态函数；前移后 00 的"不是服务"论断立刻有证据支撑 | K-086~K-096 | 存量原样；新增 K-093、K-094（man page `init.8:54-61,310-317`）；K-096（InitHost 总述）从旧 01/02/03/12 四处抽取 |
| C-09 | 重排 | `04-init-single-user.md` | `09-init-single-user.md` | 编号顺延（基础设施层前移） | K-097~K-109 | 存量原样；K-099/K-101 降为调用点（机制在 06/15） |
| C-10 | 重排 | `05-init-runcom.md` | `10-init-runcom.md` | 同上 | K-110~K-118 | 存量原样；新增与 18-stage-commands 的边界声明 |
| C-11 | 重排 | `06-init-read-ttys.md` | `11-init-read-ttys.md` | 同上 | K-119~K-125 | 存量原样；台账调用点改为指向 07（不再是前向） |
| C-12 | 重排 | `07-init-session-model.md` | `12-init-session-model.md` | 同上 | K-126~K-133 | 存量原样 |
| C-13 | 重排 | `08-init-session-db.md` | `13-init-session-db.md` | 同上 | K-134~K-138 | 存量原样；utmp 挂钩机制移出到 14 |
| C-14 | 重排 | `09-init-multi-user.md` | `15-init-multi-user.md` | 同上 | K-144~K-154 | 存量原样；setctty 机制从旧 04 收回本篇（K-149） |
| C-15 | 重排 | `10-init-clean-ttys.md` | `16-init-clean-ttys.md` | 同上 | K-155~K-161 | 存量原样 |
| C-16 | 重排 | `11-init-shutdown.md` | `17-init-shutdown.md` | 同上 | K-162~K-167 | 存量原样 |
| C-17 | **新建** | — | `18-init-testing.md` | 测试基建是十项非 C 主题之一，现无主讲述点（G-05）；且"文档转述数字"已成事实错误（G-14） | K-168~K-173 | 来源：旧 04 §3.3（catch_unwind）、旧 06 §3（golden）、`host.rs`（ScriptHost）、`driver.rs`（boot_chain）、`todo.md §1-§2`（clippy 与等待态）、本轮实测 |
| C-18 | 重写（加厚 + 改性质） | `99-init-global-concepts.md`（40 行） | `99-init-global-concepts.md` | 40 行的表承载不了常量、路径、状态归宿、构建变体、死项排除五类；且要新增"只索引不解释"的纪律（退出码与信号号不在本篇重述） | K-174~K-179、K-026 | 存量原样；新增 K-178（crate 形态与 no_std 裁决，来源 `Cargo.toml` + todo P1-2） |
| C-19 | 归档（B 相执行） | `plan.md` §2 阶段表 / §3.4 边界表 / §6.1 状态跟踪 | —（改指向新目录） | 这三张表全部以旧编号为坐标，重建后失效；plan.md 的历史 review 记录（§7）保留作归档 | — | 内容不删，随旧文档归档 |

---

## 7. 缺漏新篇（非 C 主题逐项落实）

| 主题 | 是否新建 | 归哪一篇 | 原料来源 | 验收标准 |
|---|---|---|---|---|
| 链接与加载 | 否 | 00 §2（一句） | `vm/main.c:331,514` | 能说清 init 不是自己加载自己 |
| 镜像与内存布局 | 否 | 00 §2（一句事实） | `kernel/table.c:64` | 只讲槽位顺序最后一位 |
| 汇编入口与陷阱进入 | 否 | 99 §6（明确排除） | init 为纯 C 用户程序，全文件无 asm | 明确写出"本 stage 无此主题" |
| 启动装配 | 否（并入） | 01 §4 | `main.rs:66-69`、`minix_rt::crt0`；机制交 14-stage-runtime | 能指出参数不来自 std env |
| 构建与工具链 | 否（并入） | 99 §5 | `Makefile:8-17`、`Cargo.toml`、todo P1-2 | 七个宏逐一给出 Rust 侧取舍 |
| 跨模块接口与线格式 | 否（分散） | ttys→11、utmp→07/14、argv→05、退出码→05、信号号→03 | 各 C 锚点 | 每种格式都能给出字段级定义 |
| 错误路径 | 是（部分） | 05 §3（`_exit` 总表） | `init.c` 16 处 | 16 个码全部有位置、条件、父进程所见结果 |
| 关闭与退出 | 否（已有） | 17 + 04 §2 + 08 §3 | `init.c:1634-1698`、`init.8:310-317` | 能区分"假死/真死/init 自己死"三条路径 |
| 并发与同步 | 否（并入） | 03 §1/§4 | `requested_transition`/`clang`、Rust `SignalState` | 能说清"无锁，只有一座桥" |
| 测试基建 | **是** | 18 全篇 | `host.rs`、`driver.rs`、`single_user.rs`、`minix3/etc/ttys`、`todo.md`、本轮实测 | 见 18 篇验收标准 |

**另外两处缺口的落实**：

- **G-06（init 死亡的后果）** 落实在 08 §3，原料为 `init.8:310-317`（系统自动重启 +
  `panic: init died (signal %d, exit %d)`）与 `init.8:54-61`（kernel 的四个候选路径）。
- **G-12（相邻 stage 边界）** 落实在 00 §5 与各篇"不讲什么"：
  `/etc/rc` 脚本体系 → `../18-stage-commands/01-init-rc-scripts.md`；
  crt0 与运行时 → `../14-stage-runtime/02-crt0-start.md`；
  boot 协议与页表 → `../01-stage-kernel/`、`../02-stage-vm/`。

---

## 8. 锚点迁移与断链成本

### 8.1 编号映射（旧 → 新）

| 旧 | 新 | 类型 |
|---|---|---|
| 00 | 00 | 重写 |
| 01 | 01 | 迁移 |
| 02 | 02 + 03 | 拆分 |
| 03 | 04 | 迁移（编号顺延） |
| 04 | 09 | 迁移 |
| 05 | 10 | 迁移 |
| 06 | 11 | 迁移 |
| 07 | 12 | 迁移 |
| 08 | 13 | 迁移 |
| 09 | 15 | 迁移 |
| 10 | 16 | 迁移 |
| 11 | 17 | 迁移 |
| 12 | 06 | 前移 |
| 13 | 07 + 14 | 拆分 + 前移 |
| 14 | 08 | 前移 |
| 99 | 99 | 重写 |
| — | 05、18 | 新建 |

### 8.2 节级锚点迁移表

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|---|---|---|---|---|
| 01 §2.5（S7 信号注册调用点） | 只列注册顺序 | 01 §2（保留调用点）+ 03 §2（机制） | 拆分 | 中：旧正文说"归 02"，需改为 03 |
| 01 §2.6（S8 securelevel 探测） | 探测一行 | 01 §2（调用点）+ 06 §2 | 拆分 | 中：旧写"见 12"，需改 06 |
| 02 §2.3（handle/delset） | 注册机制 | 03 §2 | 原样搬移 | 低 |
| 02 §2.5（transition_handler/alrm_handler） | 三映射与 clang | 03 §2 | 原样搬移 | 低 |
| 02 §3.5（SignalState 自觉偏离） | Rust 机制偏离 | 03 §4 | 改写 | 低 |
| 03 全篇 | 日志四件套 | 04 全篇 | 原样搬移 | 低（编号变化） |
| 04 §2.2（安全级降级） | 调用点 | 09 §2（调用点）+ 06 §2 | 拆分 | 中：旧写"见 12" |
| 04 §2.1/§2.4（等待循环与 collect_child） | 循环骨架与回收 | 09 §2（本篇影响）+ 05 §2（骨架）+ 15 §2（机制） | 拆分 | **高**：旧 04 §2.4 的"五结局"表要拆成"通用等待骨架（05）+ 本篇差异（09）" |
| 05 §2.3（五归宿） | 出口表 | 10 §2（本篇）+ 05 §2（骨架） | 拆分 | 高（同上） |
| 06 §2（开机记录 1229-1247） | "机制见 13" | 11 §2（调用点）+ 07 §2（机制） | 改写 | **高**：旧 06 有前向引用（13 在后面），新目录消除 |
| 06 §3（getttyent 字段语义） | 解析三坑 | 11 §2 | 原样搬移 | 低 |
| 07 全篇 | session_t | 12 全篇 | 原样搬移 | 低 |
| 08 全篇 + utmp 挂钩 | DB + 挂钩 | 13 全篇 + 14 §2 | 拆分 | 中 |
| 09 §2（setctty/collect_child/防抖） | 稳态机制 | 15 §2 | 原样搬移 | 低 |
| 10 全篇 | clean_ttys | 16 全篇 | 原样搬移 | 低 |
| 11 全篇 | catatonia/death | 17 全篇 | 原样搬移 | 低 |
| 12 全篇 | securelevel/chroot | 06 全篇 | 原样搬移 | 低（编号变化大，引用多） |
| 13 §2（session_utmpx/clear_session_logs） | 会话挂载 | 14 §2 | 拆分 | 中 |
| 13 §2（make_utmpx/get_runlevel/set_runlevel） | 记录与 runlevel | 07 §2 | 拆分 | 中 |
| 14 全篇 | 对外契约 | 08 全篇 + 00 §3（身份摘要） | 拆分 | 中 |
| 99 §2/§3 | 常量与状态归宿 | 99 §2/§4 | 改写扩充 | 低 |

### 8.3 引用迁移表

| 旧引用所在 | 形式 | 数量 | 新目标 | 验证方式 |
|---|---|---|---|---|
| 各编号文档 §7 参见 + §6 过渡 | 文件名（如 `09-init-multi-user.md`） | 45 | 按 §8.1 映射 | `rg -o "\b(0[0-9]|1[0-8]|99)-init-[a-z-]+\.md"` 逐条替换后复查 0 残留 |
| 各编号文档正文 | 数字（如"见 09"、"机制见 12"） | 未逐条统计（B 相用上条同一命令 + `rg "见 [0-9]+"` 覆盖） | 按 §8.1 映射 | 替换后跑一次前向引用扫描（见 §9 G3） |
| `README.md` 文档清单表 | 文件名 + 语义模块 | 15 行 | 20 行新表 | `rg "init-.*\.md" README.md` |
| `README.md` 启动链路图 | 目录名 | 1 处 | 不变 | 目视 |
| `plan.md` §2 阶段表 | 编号 + 文件名 | 16 行 | 随旧文档归档（C-19） | — |
| `plan.md` §3.4 边界表 | 编号 | 16 行 | 随旧文档归档 | — |
| `plan.md` §5.2 函数级映射表 | 编号（"文档"列） | 46 行 | 随旧文档归档；B 相应按新编号重建此表（它是覆盖契约，新目录需一份新版本） | 新表逐函数核对 `init.c` 46 个定义 |
| `plan.md` §6.1 状态跟踪表 | 编号 | 16 行 | 随旧文档归档 | — |
| `os/commands/sbin/init/src/*.rs` 顶部注释 | 设计契约锚点（NN 编号，指向中间产物目录，本蓝图不引用其内容） | 18 处 | NN 按 §8.1 映射（01/02/03/05/06/07/08/09/10/11/12/13/14） | `git grep -n "\.md" -- os/commands/sbin/init/` 逐条替换 |
| 其它 stage（`10/11/12 stage` 的 plan.md、`edge_todo.md`、`edge2.md`、`edge3.md`） | 目录名 `09-stage-init` | 9 处 | **不变**（目录名不改） | `git grep -c "09-stage-init"` 应仍为 12 个文件 |

### 8.4 断链成本摘要

| 项 | 数 |
|---|---|
| 受影响文档内引用（文件名形式） | 45 |
| `README.md` | 15 |
| `plan.md`（归档，不迁移） | 35 |
| 代码注释设计契约锚点 | 18 |
| **合计需修改** | **78 处**（不含 `plan.md` 的 35 处归档项） |
| 热点文件 | `plan.md`（35）、`README.md`（15）、`02-init-state-machine.md`（8）、`01-init-main-entry.md`（5） |
| 建议批量方式 | ① 先按 §8.1 写一张 `sed` 映射表（旧编号→新编号，注意**两轮替换**：先改 12/13/14 三个前移编号到临时占位，再统一落位，避免 12→06 与 06→11 互相踩踏）；② `rg -l` 列出文件后逐文件 `--fixed-strings` 替换；③ 替换完成跑 `rg "init-[a-z-]+\.md" | rg -v "<新编号>"` 残留检查；④ 代码注释 18 处单独处理 |

> **两轮替换的必要性**：映射中存在 `06→11`、`12→06` 这类交叉，单轮顺序替换会把旧 06 误改成 11
> 之后又被 `12→06` 的产物覆盖。必须用临时前缀（例如先全部改成 `NEW-xx`）。

---

## 9. 验证与自检门

### 9.1 四种机械检查

| 检查 | 方法 | 结果 |
|---|---|---|
| 前向引用扫描 | 逐篇检查 §5 契约的"前置"字段，确认只指向更早编号 | **通过**：00 无前置；01←00；02←00/01；03←02；04←02/03；05←02/03/04；06←01/05；07←02/05；08←03/05；09←03/04/05/06；10←05/06/09；11←06/07/05；12←11；13←12；14←07/12/13；15←05/06/12/13/14；16←12/15；17←03/15；18←05/15；99 无前置（查表篇）。无一处指向更大编号 |
| 依赖关系图无环 | 由上式构图 | **通过**：图为分层 DAG，跨层引用全部指向更小编号 |
| 覆盖率 100% | §2.1 的 179 条逐条在 §5 契约的"讲什么"中出现一次 | **通过**：179/179 有去向；无删除项（旧文档没有需要丢弃的知识点，只有需要降级的"调用点"，其机制均已落到某篇） |
| 断链成本统计 | §8.3/§8.4 | **完成**：78 处需修改，热点已标出，两轮替换方案已给 |

### 9.2 自检门

| 门 | 检查内容 | 结果 | 证据 |
|---|---|---|---|
| G1 | C 真序逐条可核对（抽十条） | **通过** | 抽验：`init.c:238`（M1）、`:353`（M14）、`:624-640`（T）、`:1502-1522`（S 段异步）、`:1340-1348`（MU4 的 `_exit(7)`）、`:1229-1247`（TT1）、`:1543-1544`（MU2）、`:1691`（D 段 ECHILD）、`vm/main.c:345`（B4）、`keyboard.c:300`（K-088）——10/10 与 §0.3/§1.2 的输出一致 |
| G2 | 知识点池完整：每个 C 文件、每个非 C 制品都有归属或明确排除 | **通过** | `init.c` 46 个函数：44 生效函数全部映射到 01/02/03/04/05/06/07/09/10/11/12/13/14/15/16/17；`print_console`、`badsys` 明确排除（K-026）。非 C 制品：`Makefile`→99 §5、`init.8`→08/00、`NOTES`→00、`pathnames.h`→99、`ttyent.h`→11、`utmpx.h`/`utmp.h`→07、`getttyent.c`→11、`etc/ttys`→11/18、`etc/rc`→10（黑盒，细节交 18-stage-commands）、Rust 20 模块→各篇 §4 |
| G3 | 前向引用为零 | **通过** | 见 §9.1 第一项 |
| G4 | 依赖图无环 | **通过** | 见 §9.1 第二项 |
| G5 | 覆盖率 100% + 新增条目有锚点 | **通过** | 179/179 有去向；16 条新增全部带 C 行号 / 制品路径 / Rust 文件（K-010、K-056、K-060、K-063、K-072、K-084、K-085、K-093、K-094、K-168、K-169、K-171、K-178、K-179 等）；明确删除项：**无** |
| G6 | 拆分/合并写清去向、新建写清来源（抽查十处） | **通过** | C-03（02 拆 02+03）：存量 K-035~K-046 去向逐一列出；C-07（13 拆 07+14）：存量去向分两组列出；C-05（新建 05）：来源为旧 04 §2.4 / 旧 05 §2.3 / 旧 09 §2 / 旧 11 §2 的抽取 + C 源码汇总；C-17（新建 18）：来源为 `host.rs`/`driver.rs`/`single_user.rs`/`minix3/etc/ttys`/`todo.md`/本轮实测；C-06/C-08/C-18 的新增项均给出锚点 |
| G7 | 每篇契约七要素齐全 | **通过** | 20 篇均含：一句话定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 / 验收标准 |
| G8 | 锚点迁移表覆盖所有变化文档的每一节；引用迁移覆盖文档与代码注释 | **通过** | §8.2 覆盖 16 篇旧文档的全部主要小节（22 行）；§8.3 覆盖文档间引用（45）、README（15）、plan（35）、代码注释（18）、跨 stage（9，不变） |
| G9 | 事实断言有锚点；推测项已标注 | **通过（含一处待验证）** | 所有 C/Rust 断言均带 `文件:行号`；**待验证 1**：Rust `default_signal_spec()` 的 6 个 handler 中未见 disaster 组（SIGFPE/SIGILL/SIGSEGV/SIGBUS）条目（`host.rs` 测试断言 `handlers.len()==6`），与 C 的 `handle(disaster, ...)` 调用点（`init.c:317`）是否等价，需 B 相在 03 篇核对后写明（**推测**：可能是缺口，也可能是该组经另一通道安装）；**待验证 2**：本轮失败的 `test_minix_host_honest_enosys_for_missing_wrappers`（`Err(Errno(5))` vs `Ok(())`）是否与宿主传输相关，需 B 相在 18 篇如实记录 |

### 9.3 序差表（教学序 vs C 真序）

| # | C 真序事实（带锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---|---|---|---|
| X-01 | `createsysctlnode()` 在 main（`init.c:345`），`shouldchroot()` 在 runcom（`init.c:990`） | 二者同篇（06） | 同一 sysctl 节点的一写一读，拆开讲要重复解释节点语义 | 10 §2 回指 06 §2 |
| X-02 | utmp 写入分散在 transition（`:631-635`）、read_ttys（`:1233`）、add/del（`:1054,1072`）、runcom（`:1008`）、death（`:1674`）、clear_session_logs（`:652`）六处 | 集中在 07 + 14 | 横切关注点按依赖分组，不按调用点分组 | 02/10/13/17 各留一行调用点 |
| X-03 | `collect_child` 由 single_user（`:828`）、runetcrc（`:933`）、multi_user（`:1561`）、death（`:1688`）四处调用 | 机制归 15 | 它依赖 session 与 DB（12/13），无法前置 | 09/10/17 各讲"它对本篇循环的影响"（首次启动时 sessions 为空，对本篇循环无影响），不要求跳读 15 |
| X-04 | securelevel 消费点在 single_user（`:723`）、multi_user（`:1543`） | 机制前移到 06 | 探测与消费同属一个旋钮 | 09 §2、15 §2 留调用点 |
| X-05 | 会话台账挂载（add/del 挂钩）在 multi_user 之前发生 | 14 在 15 之前 | 与 C 一致，无差异 | — |
| X-06 | `mfs_dev` 在 main 早期（`:269`） | 01 §3 | 与 C 一致 | — |

### 9.4 结论与待用户裁决的问题

**结论：本蓝图为可执行状态（未完成项仅 2 条待验证，见 G9）。**

待裁决：

1. **旧文档归档方式的粒度**：建议 `plan.md`/`todo.md` 保持原位（前者是历史 review 记录、
   后者是开口项台账，两者都还有检索价值），仅把其中"以旧编号为坐标"的三张表标注为
   "已由新目录取代"。是否允许在 B 相同时**新建一份新编号覆盖契约表**（替代 plan §5.2），
   请裁决。
2. **05 与 18 两篇的篇幅**：05 承担 `_exit` 全表 + 等待语义，18 承担全部测试基建。
   这两篇是本蓝图唯一真正"加出来"的内容，若共识蓝图认为应由既有篇吸收（例如把 `_exit`
   表并入 99、把测试基建并入 00），本蓝图建议**不要**——那会让 99 变成既查表又教学、
   让 00 变成既导航又讲测试，违反单篇单语义。
3. **实测基线的处理**：`cargo test -p minix-init` 当前 135 passed / 1 failed，
   与全部旧文档声明的 143/0 不符。建议 B 相在 18 篇如实记录并在各篇 §5 引入"以实测为准"
   的数字纪律；是否需要在重建同时修掉这条失败（属代码修复，非文档重建），请裁决。
