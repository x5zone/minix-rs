# 09-stage-init 文档重组计划（plan.md）

> **状态**: 定稿（2026-08-16 首版；深度 review + minix3 源码回归 review 后定稿，见 §7）
> **范围**: `notes/rewrite/fork-syscall-rewrite/09-stage-init/`
> **目标**: 以 **init 状态机启动顺序为主线**重组 init 全部语义文档；最终覆盖 Minix3 init 全部语义，支撑 init 的 Rust 重写（`os/commands/sbin/init/`，当前为 stub）
> **对照**: `01-stage-kernel/`（讲述结构参照）、`minix3/sbin/init/`（ground truth）、`os/commands/sbin/init/`（Rust 实现）

---

## 1. 背景与动机

### 1.1 现状

09-stage-init 目录此前仅有占位 README（已移入 `draft/`），无正式文档。init 是 boot 链路的**终点**：`boot_image` 最后一项（`kernel/table.c:64`），在 RS 加载完所有系统服务后由 kernel 调度运行，启动登录进程与用户环境。

### 1.2 init 与 VM/PM/VFS 的执行模型差异（必须先说清）

VM/PM/VFS/RS 是**系统服务器**：单线程 IPC 事件循环（SEF 生命周期 + CALLMAP 分发 + receive→dispatch→reply）。**init 不是系统服务器**——它是普通用户进程（`rs/table.c:28` 登记为 `USR_F`），**没有 IPC 主循环、没有 SEF、没有 CALLMAP**。

init 的执行驱动只有两个：

1. **waitpid(-1) 阻塞循环**——等待子进程（shell / /etc/rc / getty / window）终止，`collect_child` 决定重启或清理；
2. **信号**——`SIGHUP`（→clean_ttys）、`SIGTERM`（→death）、`SIGTSTP`（→catatonia）、`SIGALRM`（→alrm_handler）、`SIGABRT`（→minixreboot，minix 专用）、`SIGUSR1`（→minixpowerdown，minix 专用）、致命信号（→disaster）。

因此 init 的"启动顺序主线"不是 IPC 服务注册顺序，而是**状态机推进顺序**（`transition()` 的主循环）。这是本计划与 02-stage-vm 计划的本质区别，也是 init 文档的叙事主轴。

### 1.3 新主线：init 状态机启动顺序

与 `01-stage-kernel` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则：

```
kernel boot（boot_image 最后一项 table.c:64；USR_F，rs/table.c:28）
  │  VM exec_bootproc 为全部 boot 进程加载 ELF（含 init，servers/vm/main.c:498-514）
  │  PM 初始化：INIT 父进程=自身、INIT_PID=1、scheduler=KERNEL（pm/main.c:188-204）
  │  RTS_VMINHIBIT/RTS_BOOTINHIBIT 解除 → kernel 调度 init 首次运行
  ▼  main() [init.c:229]
  ├─ 身份校验（getuid()!=0 → EPERM；getpid()!=1 → "already running"）   ← 01
  ├─ setsid() 建立初始会话                                              ← 01
  ├─ mfs_dev()：/dev/console 缺失 → 跑 MAKEDEV（minix 专用）              ← 01
  ├─ getopt：-s 单用户 / -f fastboot                                     ← 01
  ├─ 信号注册（handle/delset/sigprocmask）                               ← 02
  │    SIGABRT→minixreboot / SIGUSR1→minixpowerdown / 致命信号→disaster
  │    SIGHUP→clean_ttys / SIGTERM→death / SIGTSTP→catatonia / SIGALRM→alrm_handler
  ├─ close(0/1/2) + securelevel_present = has_securelevel()              ← 01/12
  └─ transition(requested_transition) [init.c:624]                       ← 02
       ├─ 's' single_user → fork shell → 退出/^D → runcom                ← 04
       ├─ 'r' runcom → runetcrc() 执行 /etc/rc（autoboot|fastboot）→ read_ttys ← 05
       ├─ 't' read_ttys → /etc/ttys 解析 → session 链表 + session_db → multi_user ← 06/07/08
       ├─ 'm' multi_user（稳态）→ start_getty × sessions + waitpid 主循环  ← 09
       │     └─ collect_child：getty 退出 → 重启 / SE_SHUTDOWN → 移除      ← 09
       ├─ 'T' clean_ttys（SIGHUP）→ 重读 /etc/ttys，关停下线行 → multi_user ← 10
       ├─ 'c' catatonia（SIGTSTP）→ 全部 SE_SHUTDOWN → multi_user          ← 11
       └─ 'd' death（SIGTERM）→ 三轮 SIGHUP/SIGTERM/SIGKILL → single_user  ← 11
```

**每篇文档必须能回答一个问题：它位于状态机的哪个状态 / 由哪个函数调用 / 在 boot 链的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则。

### 1.4 次主线：会话生命周期

init 没有 fork 次主线；它的次主线是**登录会话生命周期**（session 的创建→运行→重启→关停），在 07~11 内展开：

```
session 生命周期
  ├─ read_ttys：/etc/ttys 每行 → new_session（07/08）
  ├─ multi_user：start_getty 启动 getty（09）
  ├─ collect_child：getty 退出 → 重启；SE_SHUTDOWN → 移除（09）
  ├─ clean_ttys：/etc/ttys 变化 → 新开/关停会话（10）
  └─ death/catatonia：全部 SE_SHUTDOWN → 回收（11）
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel`/`02-stage-vm` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。原 `draft/README.md` 保留在 `draft/` 作素材，新编号在顶层建立。

### 阶段总览（16 篇）

| 阶段 | 编号 | 文档 | 语义模块 | C 源码（init.c） | Rust 模块 | 变更 |
|------|------|------|---------|----------------|-----------|------|
| 0 总览 | 00 | `00-init-overview.md` | init 是什么、boot 链位置、状态机主线图、文档导航 | 全部 | 全部 | 新建 |
| 1 入口与身份 | 01 | `01-init-main-entry.md` | main() 入口：身份校验/setsid/getopt/close stdio/mfs_dev/securelevel 探测 → transition | `main`（229-367）、`mfs_dev`（1703-1790） | `os/commands/sbin/init/src/main.rs` | 新建 |
| 2 状态机骨架 | 02 | `02-init-state-machine.md` | 状态机：state_t/transition/requested_transition、信号注册与转换（handle/delset/transition_handler/alrm_handler）、7 状态常量 | `transition`（624-644）、`handle`（369-392）、`delset`（394-409）、`transition_handler`（1502-1526）、`alrm_handler`（1649-1659） | `state_machine.rs`（规划） | 新建 |
| 2 | 03 | `03-init-logging-failure.md` | 日志三件套 stall/warning/emergency + syslog、致命信号 disaster | `stall`（440-455）、`warning`（457-470）、`emergency`（472-488）、`disaster`（504-515） | `log.rs`（规划） | 新建 |
| 3 启动状态序列 | 04 | `04-init-single-user.md` | 单用户状态：fork shell、SECURE 密码、ALTSHELL、waitpid 循环、退出→runcom | `single_user`（694-877） | — | 新建 |
| 3 | 05 | `05-init-runcom.md` | 运行 /etc/rc：runcom/runetcrc、autoboot/fastboot、错误→single_user、chroot 决策 | `runcom`（974-1019）、`runetcrc`（879-972） | — | 新建 |
| 3 | 06 | `06-init-read-ttys.md` | /etc/ttys 解析：read_ttys 重建会话链表、do_setttyent | `read_ttys`（1222-1288）、`do_setttyent`（1792-1809） | `ttys.rs`（规划） | 新建 |
| 4 会话模型 | 07 | `07-init-session-model.md` | session_t 结构、SE_* 标志、new/free/setupargv/construct_argv、se_started 防抖动 | `new_session`（1142-1183）、`free_session`（1123-1140）、`setupargv`（1185-1220）、`construct_argv`（1101-1121） | `session.rs`（规划） | 新建 |
| 4 | 08 | `08-init-session-db.md` | 会话数据库：start_session_db/add/del/find、**Berkeley DB → HashMap（ARCH A-1）** | `start_session_db`（1021-1036）、`add_session`（1038-1060）、`del_session`（1062-1078）、`find_session`（1080-1099） | `session_db.rs`（规划） | 新建 |
| 5 运行态（稳态） | 09 | `09-init-multi-user.md` | 多用户稳态：multi_user、start_getty、start_window_system、setctty、collect_child、getty 防抖动 | `multi_user`（1528-1567）、`start_getty`（1321-1370）、`start_window_system`（1290-1319）、`setctty`（669-692）、`collect_child`（1460-1500） | — | 新建 |
| 5 | 10 | `10-init-clean-ttys.md` | clean_ttys：重读 /etc/ttys、SE_PRESENT/SE_SHUTDOWN、下线行 SIGHUP、n² 算法 | `clean_ttys`（1569-1632） | — | 新建 |
| 6 关停 | 11 | `11-init-shutdown.md` | catatonia（boring）+ death（三轮 kill + DEATH_WATCH） | `catatonia`（1634-1647）、`death`（1661-1701） | — | 新建 |
| 7 系统交互 | 12 | `12-init-sysctl-interaction.md` | securelevel 查询/设置 + CHROOT init.root 动态 sysctl 节点 | `has_securelevel`（544-566）、`getsecuritylevel`（568-592）、`setsecuritylevel`（594-621）、`createsysctlnode`（1811-1857）、`shouldchroot`（1859-1902） | — | 新建 |
| 7 | 13 | `13-init-utmp.md` | utmp/utmpx 会话日志：session_utmpx/make_utmpx/get_runlevel/utmpx_set_runlevel/clear_session_logs | `session_utmpx`（1372-1381）、`make_utmpx`（1383-1409）、`get_runlevel`（1411-1427）、`utmpx_set_runlevel`（1429-1458）、`clear_session_logs`（647-666） | — | 新建 |
| 7 | 14 | `14-init-external-contracts.md` | 对外契约：minixreboot/minixpowerdown、Ctrl-Alt-Del、PM 孤儿收养、shutdown 路径、USR_F 身份 | `minixreboot`（517-528）、`minixpowerdown`（530-541） | — | 新建 |
| 99 全局概念 | 99 | `99-init-global-concepts.md` | 常量（INIT_PID/INIT_BSHELL/状态字符/超时）、路径（/etc/rc、/etc/ttys、/dev/console）、全局状态总表（runcom_mode:151 / clang:173 / securelevel_present:177 / sessions:187 / session_db:194 / requested_transition:195,217（双默认值 runcom 与 single_user）/ boot_time:200 / current_state:201 / did_multiuser_chroot:210 / rootdir:211） | `pathnames.h`、`pm/const.h:9`、`paths.h` | — | 新建 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在状态机/boot 链中的位置与下一阶段的入口：

```
01（入口）→ 02（状态机骨架）→ 03（日志基础设施）
→ 04（single_user）→ 05（runcom）→ 06（read_ttys）
→ 07/08（会话模型+DB）→ 09（multi_user 稳态）→ 10（clean_ttys）
→ 11（关停）→ 12~14（系统交互/日志/对外契约）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel`/`02-stage-vm` 一致：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明
6. **过渡**——在状态机/boot 链中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 组织原则（继承 01-stage-kernel §3）

1. **概念首次出现即完整解释**——后续只引用不复述
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：状态机骨架在 02、会话模型在 07、多用户稳态在 09）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在状态机哪个状态 / boot 链哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `09-init-multi-user.md §collect_child`）
- 与 kernel 文档交叉引用用 `../01-stage-kernel/NN-*.md`（boot 协议：`09-vm-boot-protocol.md`、`10-switch-to-user.md`；进程模型：`06-proc-init-boot-proc.md`）
- 与 PM/VFS/RS 文档交叉引用用 `../04-stage-pm/NN-*.md`、`../05-stage-vfs/NN-*.md`、`../03-stage-rs/NN-*.md`
- 对 draft 素材的引用一律指向 `draft/NN-*.md`，并标注"素材"
- **调用点 vs 机制**：入口/状态文档（01/02/04/05/09）只描述调用位置与状态转移，机制归机制文档（如 shouldchroot/getsecuritylevel/setsecuritylevel 调用点→12、utmpx_set_runlevel 调用点→13）；调用点文档用一行"机制见 NN"交叉引用，不展开实现
- 引用 C 源码一律用绝对路径 `minix3/sbin/init/init.c:NNN` 或 `minix3/minix/...`

### 3.4 每篇文档边界声明（执行级）

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.2 函数清单为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、boot 链位置、状态机主线图、文档导航、设计原则 | 一切机制细节 |
| 01 | 00 + kernel `06-proc-init-boot-proc`/`09-vm-boot-protocol` + PM 初始化（04-stage-pm） | main() 全流程：身份校验/setsid/getopt/mfs_dev/信号注册/close stdio/securelevel 探测/transition 入口 | 状态机细节（02）、信号 handler 语义（02/03/14） |
| 02 | 01（信号注册调用点） | state_t/transition/requested_transition/transition_handler/alrm_handler/7 状态常量、信号→状态转换表 | 各状态函数体（04~11）、日志（03）、minix 专用 reboot 挂钩（14） |
| 03 | 02 | stall/warning/emergency/syslog、disaster 致命信号路径 | 状态转换（02）、minixreboot/minixpowerdown（14） |
| 04 | 02/03 | single_user 全流程：fork/exec shell、SECURE 密码检查、ALTSHELL、waitpid 循环、退出→runcom | /etc/rc（05）、ttys（06） |
| 05 | 04 | runcom/runetcrc：/etc/rc 执行、autoboot/fastboot 参数、错误回退、chroot 决策（shouldchroot 调用点） | chroot sysctl 机制（12） |
| 06 | 02/03 | read_ttys：/etc/ttys 解析、会话链表重建、session_db 启动、boot_time 记录 | 会话结构细节（07）、DB 实现（08） |
| 07 | 06 | session_t 全字段、SE_* 标志、new/free/setupargv/construct_argv、se_started 防抖动时间 | DB（08）、启动/重启流程（09） |
| 08 | 07 | start_session_db/add/del/find、ARCH A-1（Berkeley DB→HashMap） | session 字段（07） |
| 09 | 06/07/08 | multi_user：start_getty 循环、waitpid 主循环、collect_child 重启、start_window_system、setctty、getty spacing/sleep | clean_ttys（10）、关停（11） |
| 10 | 07 | clean_ttys：重读 /etc/ttys、SE_PRESENT/SE_SHUTDOWN 处理、n² 算法 | multi_user 主循环（09） |
| 11 | 09 | catatonia/death：SE_SHUTDOWN 全置、death_sigs 三轮、DEATH_WATCH/alrm_handler 联动 | 单条会话回收细节（09） |
| 12 | 01（探测调用点）/05（chroot 调用点） | securelevel has/get/set、CHROOT createsysctlnode/shouldchroot、ARCH 标注 | 内核 sysctl 实现（`../01-stage-kernel/` 对应文档） |
| 13 | 06/07/09 | utmp/utmpx：session_utmpx/make_utmpx/get_runlevel/utmpx_set_runlevel/clear_session_logs、ARCH A-2 | 会话生命周期（07~11） |
| 14 | 02/03 | minixreboot/minixpowerdown、Ctrl-Alt-Del 链路（keyboard.c:300）、PM 孤儿收养（pm/forkexit.c）、shutdown 路径、USR_F 身份 | 信号机制本身（02） |
| 99 | 无 | 常量/路径/全局状态总表 | 一切机制细节 |

### 3.5 测试基线

`cargo test -p minix-init` = 0 passed / 0 failed（2026-08-16，stub `main.rs` 仅 `init() + loop {}`）。后续每篇文档改写时在此基线上增量补测试。

### 3.6 review gate 接入

- 每篇改写后必须走 review 工作流：Step 0 预检（4 条 `ls` + `tools/design-coverage-check.sh fork-syscall-rewrite --stage 09-stage-init`）→ Blocker Gates（0/A/B/C/D/D-6/E/G/H）→ scan 产物写入 `.review/codex/init/{NN}-{name}/`
- Step 0.3 嵌入生成 `{NN}-outline.v*.md` / `{NN}-outline-review.v*.md` / `{NN}-design.v*.md`（Gate H.6/H.1，不允许 N/A）
- P0 未清不得标完成；doc 与 code 保持同步

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。init 是用户态程序，ARCH 集中于"libc/文件系统依赖的 Rust 化"与"minix-rs 基础服务缺口"。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及文档 | 状态 |
|---|---------|------------|--------------|---------|------|
| A-1 | **会话数据库** | Berkeley DB 内存哈希表（`dbopen(NULL, O_RDWR, 0, DB_HASH, NULL)`，`db.h:72,213`），pid→session 指针映射 | `HashMap<pid_t, Rc<Session>>`（或 slot 数组），消除 libdb 依赖 | 08 | 演进（结构替换，行为等价） |
| A-2 | **utmp/utmpx 会话日志** | libc utmp 接口写 `/var/run/utmpx`、`/var/log/wtmpx`（`utmpx.h:39-40`、`utmp.h:42-43`），`pututxline`/`logoutx`；minix 构建同时启用 SUPPORT_UTMP+SUPPORT_UTMPX 双日志（Makefile） | 待定：minix-rs 用户态文件/记录服务未就绪 → **defer**，标注语义契约（LOGIN_PROCESS/DEAD_PROCESS/RUN_LVL 记录点） | 13 | **缺口**：defer + 语义契约 |
| A-3 | **syslog 日志** | `openlog("init", LOG_CONS, LOG_AUTH)` + `vsyslog`（stall/warning/emergency） | minix-rs 无 syslog 服务 → 简化通道（console 直写或经 VFS），**defer** | 03 | **缺口**：defer |
| A-4 | **securelevel** | `sysctl(KERN_SECURELVL)` 查询/设置（`has/get/setsecuritylevel`），kernel 维护安全级别 | minix-rs kernel 无 securelevel 语义 → **defer**，标注契约（单用户降级 0、多用户升 1） | 12 | **缺口**：defer |
| A-5 | **CHROOT init.root sysctl 节点** | `createsysctlnode` 动态创建 `init.root` 字符串节点（`CTL_CREATE`），`shouldchroot` 读取 | minix-rs 无 sysctl 服务 → **defer**（或简化：编译期常量） | 12 | **缺口**：defer |
| A-6 | **/etc/ttys 解析** | libc `getttyent/setttyent/endttyent`（`minix3/lib/libc/gen/getttyent.c`） | Rust 自有解析器（`ttys.rs`）：`name getty status [window]` 格式 + `TTY_ON/TTY_SECURE` 标志 | 06/10 | 演进（结构替换） |
| A-7 | **构建变体编译宏** | `LETS_GET_SMALL`/`ALTSHELL`/`SECURE`/`CHROOT`/`SUPPORT_UTMP`/`SUPPORT_UTMPX`/`MFS_DEV_IF_NO_CONSOLE`（Makefile） | Rust feature flags / 默认取舍（minix 默认全开 ALTSHELL+SECURE+CHROOT+UTMP+UTMPX+MFS_DEV_IF_NO_CONSOLE） | 01/99 | 设计差异 |
| A-8 | **信号处理** | `sigaction/sigprocmask`（`handle/delset`，`SA_NOCLDSTOP`，注释 "XXX SA_RESTART?"） | minix-rt 信号抽象支持状态待确认 → **依赖/缺口** | 02 | 待确认 |
| A-9 | **进程等待** | `waitpid(-1, &status, WUNTRACED)` 阻塞 + EINTR 重试 | minix-sys 进程等待抽象（依赖 PM 服务） | 04/05/09/11 | 依赖 |
| A-10 | **时间/睡眠** | `sleep/nanosleep/alarm/gettimeofday`（`dtrtime`、GETTY_SPACING/GETTY_SLEEP/WINDOW_WAIT/DEATH_WATCH/STALL_TIMEOUT） | minix-rt 时间抽象（依赖 kernel 时钟） | 03/09/11/13 | 依赖 |

> **A-1~A-7 为 init 自身范围的 ARCH；A-8~A-10 是 minix-rs 基础库依赖，需在写文档时核实 minix-rt/minix-sys 实际能力，标注 已实现/缺口。**

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源文件 → 新文档映射

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `sbin/init/init.c` | 1902 | 01~14 全部（函数级见 §5.2） | 已核对 |
| `sbin/init/pathnames.h` | 40 | 99（`_PATH_SLOGGER`/`_PATH_RUNCOM` 等路径常量） | 已核对 |
| `sbin/init/init.8` | man page | 00/02（状态机行为契约：7 状态语义、信号映射、POSIX 职责） | 已核对 |
| `sbin/init/Makefile` | 21 | A-7（构建变体编译宏） | 已核对 |
| `sbin/init/NOTES` | NetBSD 说明 | 02 参考（POSIX 对 init 的职责要求：孤儿回收/job control/controlling terminal） | 参考 |
| `include/paths.h` | — | 99（`_PATH_BSHELL`/`_PATH_CONSOLE`/`_PATH_CONSTTY`） | 已核对 |
| `include/utmpx.h`/`utmp.h`/`ttyent.h`/`db.h` | — | 13/06/08（路径与接口定义） | 已核对 |

### 5.2 函数级映射（init.c 46 个函数定义）

> 依据 `rg -n '^(static )?(int|void|long|pid_t|char|session_t|state_func_t|state_t|DB|struct|sig_t)[a-zA-Z_ *]*$' minix3/sbin/init/init.c`（46 命中）。

| 函数 | 行号 | 文档 | 语义归属 |
|------|------|------|---------|
| `main` | 229 | 01 | 入口 |
| `mfs_dev` | 1703 | 01 | console 缺失时建 /dev |
| `handle` | 369 | 02 | 信号注册 |
| `delset` | 394 | 02 | 信号掩码删除 |
| `transition` | 624 | 02 | 状态机主循环 |
| `transition_handler` | 1502 | 02 | 信号→状态转换 |
| `alrm_handler` | 1649 | 02 | DEATH_WATCH 闹钟 |
| `stall` | 440 | 03 | 日志+睡眠 |
| `warning` | 457 | 03 | 日志 |
| `emergency` | 472 | 03 | 日志 |
| `disaster` | 504 | 03 | 致命信号 |
| `single_user` | 694 | 04 | 状态 's' |
| `runetcrc` | 879 | 05 | /etc/rc 执行 |
| `runcom` | 974 | 05 | 状态 'r' |
| `read_ttys` | 1222 | 06 | 状态 't' |
| `do_setttyent` | 1792 | 06 | ttys 打开（chroot 感知） |
| `new_session` | 1142 | 07 | 会话创建 |
| `free_session` | 1123 | 07 | 会话释放 |
| `setupargv` | 1185 | 07 | getty/window argv 组装 |
| `construct_argv` | 1101 | 07 | 命令行分词 |
| `start_session_db` | 1021 | 08 | DB 打开 |
| `add_session` | 1038 | 08 | DB 插入 |
| `del_session` | 1062 | 08 | DB 删除 |
| `find_session` | 1080 | 08 | DB 查找 |
| `multi_user` | 1528 | 09 | 状态 'm'（稳态） |
| `start_getty` | 1321 | 09 | getty 启动 |
| `start_window_system` | 1290 | 09 | window 系统启动 |
| `setctty` | 669 | 09 | controlling terminal |
| `collect_child` | 1460 | 09 | 子进程回收/重启 |
| `clean_ttys` | 1569 | 10 | 状态 'T' |
| `catatonia` | 1634 | 11 | 状态 'c' |
| `death` | 1661 | 11 | 状态 'd' |
| `has_securelevel` | 544 | 12 | securelevel 探测 |
| `getsecuritylevel` | 568 | 12 | securelevel 查询 |
| `setsecuritylevel` | 594 | 12 | securelevel 设置 |
| `createsysctlnode` | 1811 | 12 | init.root 节点创建 |
| `shouldchroot` | 1859 | 12 | chroot 决策 |
| `session_utmpx` | 1372 | 13 | utmpx 会话记录 |
| `make_utmpx` | 1383 | 13 | utmpx 记录构造 |
| `get_runlevel` | 1411 | 13 | 状态→runlevel 字符 |
| `utmpx_set_runlevel` | 1429 | 13 | runlevel 转换记录 |
| `clear_session_logs` | 647 | 13 | 会话日志清理 |
| `minixreboot` | 517 | 14 | SIGABRT→shutdown -r |
| `minixpowerdown` | 530 | 14 | SIGUSR1→shutdown -p |
| `print_console` | 411 | **排除**（`#if 0` 死代码） | §5.4 |
| `badsys` | 490 | **排除**（`#if !defined(__minix)`） | §5.4 |

### 5.3 外部契约（跨服务/内核）

| 契约 | 证据 | 文档 |
|------|------|------|
| init 是 boot_image 最后一项 | `minix3/minix/kernel/table.c:64` | 00/14 |
| init 是 USR_F 用户进程（非系统服务） | `minix3/minix/servers/rs/table.c:28` | 00/14 |
| PM：INIT 父进程=自身、INIT_PID=1、scheduler=KERNEL | `minix3/minix/servers/pm/main.c:188-204`、`pm/const.h:9` | 01/14 |
| PM：孤儿进程收养到 INIT | `minix3/minix/servers/pm/forkexit.c:336,396` | 14 |
| PM：INIT 死亡 → stacktrace，不 panic | `minix3/minix/servers/pm/forkexit.c:336-341` | 14 |
| PM：reboot 路径 stop init | `minix3/minix/servers/pm/misc.c:224` | 14 |
| PM：RS（root sysproc）的父进程也是 INIT | `minix3/minix/servers/pm/main.c:203` | 14 |
| PM：调度参数可继承自 INIT | `minix3/minix/servers/pm/schedule.c:34,73` | 14 |
| procfs：INIT 在 RS 表中但不算系统服务（`service_active` 对 INIT 返回 false） | `minix3/minix/fs/procfs/service.c:195-207` | 14 |
| boot argv：VM 固定 argv=`{"init",NULL}`，`-s/-f` 在 boot 路径不生效（init.8 的 kernel 传 `-s` 为 NetBSD 文档，minix VM 加载路径无参数） | `minix3/minix/servers/vm/main.c:346-347`（argv 定义） | 01/14 |
| kernel：Ctrl-Alt-Del → SIGABRT(init) | `minix3/minix/drivers/tty/tty/arch/i386/keyboard.c:300` | 14 |
| kernel：VM 为 boot 进程（含 init）加载 ELF | `minix3/minix/servers/vm/main.c:498-514`（exec_bootproc） | 00/01 |
| power 驱动：低电 → SIGUSR1(init) | `minix3/minix/drivers/power/tps65217/tps65217.c:226` | 14 |
| exec 依赖：`/bin/sh`、getty、window 经 PM/VFS exec | `init.c:797-808`（single_user execv/INIT_BSHELL fallback）、`init.c:1312`（window execv）、`init.c:1365`（getty execv） | 04/09 |
| 文件依赖：`/etc/rc`、`/etc/ttys`、`/dev/console`、`/dev/constty` | `pathnames.h`、`paths.h:62-63`、`ttyent.h` | 05/06/99 |

### 5.4 明确排除 / 跳过的项

| 项 | 处理 | 依据 |
|----|------|------|
| `print_console`（init.c:411-437） | **死代码**（`#if 0` 包裹），标注跳过 | grep 实证 |
| `badsys`（init.c:490-502） | **非 minix 分支**（`#if !defined(__minix)`），标注跳过 | grep 实证 |
| NetBSD 注释块（`NOTES`） | 仅作 POSIX 语义参考，非代码 | 文档性质 |
| `SUPPORT_UTMP` 与 `SUPPORT_UTMPX` 双写 | minix Makefile 同时定义二者（双日志），按 C 现状保留 | Makefile 实证 |
| `MFS_DEV_IF_NO_CONSOLE` 内嵌 `#if 0` 调试段 | 死代码，标注跳过 | grep 实证 |
| `dbopen` 的 Berkeley DB 全量语义 | 仅用 DB_HASH 内存哈希（`dbopen(NULL,...)`），无持久化；Rust 侧 HashMap 等价 | `start_session_db`（1021-1036）实证 |
| LETS_GET_SMALL 变体 | minix 默认构建不启用（Makefile else 分支），Rust 侧不实现 | Makefile 实证 |
| `_PATH_SLOGGER`（pathnames.h:10） | **死常量**（init.c 无使用者，历史遗留），标注跳过 | `rg '_PATH_SLOGGER' init.c` = 0 命中 |
| `INIT_MOUNT_MFS`（init.c:102,106） | 仅用于 `mfs_dev` 的 `#if 0` 调试段（init.c:1720），生效构建不用，标注跳过 | grep 实证 |

---

## 6. 实施路线

> 每篇新文档 = 按 §3 讲述结构 + §4 ARCH 标注 + §5 覆盖契约新建（init 无旧主线文档，全部新建；draft/README.md 仅作占位背景参考）。所有 P0 修复完成后才可推进下一篇。

1. **00-init-overview 新建**（总览 + boot 链位置 + 状态机主线图 + 文档导航）
2. **01-init-main-entry 新建**（入口全流程，02~14 的锚点）
3. **02-init-state-machine 新建**（状态机骨架 + 信号转换表）
4. **03-init-logging-failure 新建**（日志 + 故障路径）
5. **04~06 启动状态序列**（single_user → runcom → read_ttys，按执行顺序）
6. **07/08 会话模型**（session 结构 + DB，ARCH A-1）
7. **09~11 运行态与关停**（multi_user 稳态 → clean_ttys → shutdown）
8. **12~14 系统交互**（sysctl、utmp、对外契约）
9. **99-init-global-concepts 新建** + `checklist.md`（函数级基线，参照 02-stage-vm/checklist.md 模式，实现期创建）
10. **kernel/PM/RS 侧交叉引用核对**（如有指向 09-stage-init 的引用，同步更新）

### 6.1 文档改写状态跟踪

| 编号 | 状态 | 完成日期 | scan 产物 |
|------|------|---------|-----------|
| 00 | pending | — | `.review/codex/init/00-init-overview/` |
| 01 | pending | — | `.review/codex/init/01-init-main-entry/` |
| 02 | pending | — | `.review/codex/init/02-init-state-machine/` |
| 03 | pending | — | `.review/codex/init/03-init-logging-failure/` |
| 04 | pending | — | `.review/codex/init/04-init-single-user/` |
| 05 | pending | — | `.review/codex/init/05-init-runcom/` |
| 06 | pending | — | `.review/codex/init/06-init-read-ttys/` |
| 07 | pending | — | `.review/codex/init/07-init-session-model/` |
| 08 | pending | — | `.review/codex/init/08-init-session-db/` |
| 09 | pending | — | `.review/codex/init/09-init-multi-user/` |
| 10 | pending | — | `.review/codex/init/10-init-clean-ttys/` |
| 11 | pending | — | `.review/codex/init/11-init-shutdown/` |
| 12 | pending | — | `.review/codex/init/12-init-sysctl-interaction/` |
| 13 | pending | — | `.review/codex/init/13-init-utmp/` |
| 14 | pending | — | `.review/codex/init/14-init-external-contracts/` |
| 99 | pending | — | `.review/codex/init/99-init-global-concepts/` |

---

## 7. 深度 review 记录

### 7.1 深度 review（第一轮，2026-08-16）

**方法**：语义模块拆分核对（§2 阶段表 ↔ init.c 函数面）+ 叙事顺序核对（状态机主线 vs 函数调用顺序）+ 前向引用核对（调用点 vs 机制）+ 精确性核对（行号/常量/外部契约 grep 实证）。

| # | 等级 | 发现 | 修复 |
|---|------|------|------|
| I-1 | P1 | §5.3 exec 依赖行号错误（767-782 非 single_user execv 位置） | 修正为 797-808（execv/INIT_BSHELL fallback）+ window 1312 + getty 1365 |
| I-2 | P1 | §5.3 缺 boot argv 契约：VM `exec_bootproc` 固定 argv=`{"init",NULL}`（vm/main.c:346-347），`-s/-f` 在真实 boot 路径不生效（init.8 的 "kernel 传 -s" 是 NetBSD 文档） | 新增契约行（01/14） |
| I-3 | P1 | §5.3 缺 PM 侧契约：RS 的父进程也是 INIT（pm/main.c:203）、调度继承（pm/schedule.c:34,73） | 新增 2 行 |
| I-4 | P1 | §5.3 缺 procfs 契约：`service_active` 对 INIT 返回 false（procfs/service.c:195-207，"not a system service"） | 新增契约行（14） |
| I-5 | P2 | §2 99 行全局状态清单不完整（缺 boot_time/current_state/did_multiuser_chroot/rootdir；requested_transition 有双默认值 195/217） | 补全为 10 项全局状态 + 行号 |
| I-6 | P2 | §5.4 缺死常量：`_PATH_SLOGGER`（init.c 无使用者）、`INIT_MOUNT_MFS`（仅 `#if 0` 段） | 新增 2 行排除 |
| I-7 | P2 | §3.3 缺"调用点 vs 机制"引用规则（05/09 的 shouldchroot/getsecuritylevel 调用点指向后置机制文档 12） | 新增规则行 |
| I-8 | P2 | A-2 utmp 只提 utmpx，未提 SUPPORT_UTMP 双写 | 补齐 |
| I-9 | P2 | 精确性：pathnames.h 行数 29→40；getty execv 1366-1369→1365；forkexit 334-340→336-341；design-coverage-check 调用法（--stage 09-stage-init）；03-stage-kernel→01-stage-kernel 引用 | 全部修正 |

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：`rg` 提取 init.c 全部函数定义，与 §5.2 表逐项比对（名称 + 行号）；§5.3 外部契约 13 项逐条 grep 实证；§5.4 排除表 7 项逐条实证。

**证据**：

```bash
$ rg -n '^[a-zA-Z_][a-zA-Z_0-9 ]*\(' minix3/sbin/init/init.c | wc -l
50   # 候选：46 个真实函数定义 + __COPYRIGHT/__RCSID/typedef 误命中
$ python3 逐函数比对 §5.2：0 缺失，0 错名，46/46 行号按声明行验证一致
$ rg -n 'INIT_PROC_NR' minix3/minix/kernel/table.c minix3/minix/servers/rs/table.c   # 64 / 28
$ rg -n 'exec_bootproc|argv\[\] = \{ ip->proc_name' minix3/minix/servers/vm/main.c  # 498-514 / 346
$ rg -n 'INIT_PROC_NR' minix3/minix/servers/pm/main.c minix3/minix/servers/pm/forkexit.c minix3/minix/servers/pm/misc.c  # 188-204/336,396/224
$ rg -n 'SIGABRT' minix3/minix/drivers/tty/tty/arch/i386/keyboard.c                  # 300
$ rg -n 'SIGUSR1' minix3/minix/drivers/power/tps65217/tps65217.c                     # 226
$ rg -n '_PATH_SLOGGER|INIT_MOUNT_MFS' minix3/sbin/init/init.c                       # 0 / 102,106,1720(#if 0)
$ cargo test -p minix-init  # 0 passed / 0 failed（stub 基线）
```

**结论**：init.c 46 个函数定义（44 生效 + print_console `#if 0` 死代码 + badsys 非 minix 分支）全部映射到新文档，0 缺失；外部契约（kernel/PM/RS/procfs/power/tty）13 项全部实证；排除表 7 项全部实证；ARCH A-1~A-10 与 minix 现状对照成立（A-1~A-7 init 自身范围，A-8~A-10 标注为基础库依赖待核实）。**覆盖完整性通过**。

---

## 8. 参见

- `draft/` — 旧占位素材（README）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则/过渡章节）
- `../01-stage-kernel/09-vm-boot-protocol.md`、`10-switch-to-user.md` — init 首次运行的前置 boot 协议
- `../00-master-plan/README.md` — 目录重排与新主线说明
- `minix3/sbin/init/` — C 源码（ground truth）
- `os/commands/sbin/init/` — Rust 实现（stub）
