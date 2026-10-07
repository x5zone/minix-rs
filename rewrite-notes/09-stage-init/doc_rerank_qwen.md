# 09-stage-init 文档重建蓝图（qwen）

## 0. 元数据

- **执行者**：qwen
- **日期**：2026-09-19
- **目标目录**：`rewrite-notes/09-stage-init/`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`40dfbbeb91fca33079791190b577145b828797c6`
- **阶段类型判定**：**启动链型 + 状态机型混合**（见 §1）。init 是 boot 链路终点的用户态进程，`main()` 是线性出生流程，之后进入 `transition()` 状态机主循环。按 §9「启动链型」处理：以启动/运行时序为骨架，非启动路径（sysctl 交互、utmp 账本、对外契约）归入支线组。

### 0.1 审查范围

- **算文档（16 篇）**：`00-init-overview`、`01`~`14`、`99-init-global-concepts`。
- **算参考材料（不重建正文）**：`plan.md`（覆盖契约 §5、ARCH 清单 §4、review 记录 §7）、`todo.md`（Rust 实现开口项）、`README.md`（导航）、`archive/todo-V1-archive-2026-09-18.md`、`draft/README.md`（旧占位）。
- **范围外**：其它 AI 的 `doc_rerank_deepseek.md` / `doc_rerank_glm.md`（**按落盘规则禁止读取，本蓝图未参考**）；`.design/`、`tmp_design_and_todo/`（项目规范：中间产物，正式文档禁止引用）。

### 0.2 读取清单

- **文档**：16 篇正文全读（头部声明 + §1~§7 章节）。
- **C 源码**：`minix3/sbin/init/init.c`（1902 行，46 个函数定义，逐行核对声明行号）、`pathnames.h`（40 行）、`Makefile`（20 行）、`init.8`（man page）、`NOTES`（NetBSD 说明）；外部契约源：`minix3/minix/kernel/table.c`、`minix3/minix/servers/rs/table.c`、`minix3/minix/servers/pm/main.c`、`minix3/minix/servers/vm/main.c`、`minix3/minix/drivers/tty/tty/arch/i386/keyboard.c`、`minix3/minix/drivers/power/tps65217/tps65217.c`、`minix3/lib/libc/gen/getttyent.c`、`include/paths.h`、`include/ttyent.h`。
- **非 C 制品**：boot 镜像登记（`table.c`）、ELF 加载（`vm/main.c`）、crt0 入口（`minix-rt::crt0`，见 §3 缺口）、Makefile 构建变体宏、utmp/wtmpx 线格式（`utmpx.h`/`utmp.h`）。
- **边界材料**：`00-master-plan/README.md`（阶段划分）、`edge_todo.md`（E-INITSYS/E-ISBOOT/E-CMDSYSFACE）、`../08-stage-is/`（前一 stage，IS 信息服务，与本 stage 用户态 init 无概念重叠）、`../01-stage-kernel/06/09/10`（前置 boot 协议）。
- **Rust 实现入口**：`os/commands/sbin/init/src/`（crate `minix-init`，**20 个模块**，5488 行；文档只声明 19，见 §6 变更）。

### 0.3 使用的命令与关键证据

```bash
# C 函数声明行核对（与文档 §5.2 逐项一致，46/46）
grep -nE '^[a-zA-Z_][a-zA-Z_0-9 ]*\(' minix3/sbin/init/init.c   # main@230 transition@625 ...
# 断链成本：目录内部交叉引用
grep -rohE '见 [0-9]{2}|[0-9]{2}-init-[a-z-]+\.md|\.\./0[0-9]-stage' 09-stage-init/[0-9]*.md  # ~150 处
# 跨 stage 对 09 的引用：仅 5 处，均为「参照 README 模式」，无编号硬依赖
grep -rn "09-stage-init" notes/ os/ --include=*.md --include=*.rs | grep -v "09-stage-init/"
# 锚点缺陷：27/27 工具生成符号锚点函数名错误（脚本逐条比对声明行，见 §1.3）
```

---

## 1. C 真序（运行时真序重建）

> 直接从 `init.c` + boot 制品重建，未从现有文档转述。行号 = 函数声明起始行（ANSI 风格，返回类型在上一行，故锚点行号即声明行，与文档口径一致）。

### 1.1 boot 装配（进入 `main()` 之前）

| 步 | 动作 | 锚点 | 说明 |
|---|---|---|---|
| B0 | init 登记为 boot_image 最后一项 | `minix3/minix/kernel/table.c:64` | `{INIT_PROC_NR, "init"}` |
| B1 | RS 表标记为 USR_F（非系统服务） | `minix3/minix/servers/rs/table.c:28` | 无 SEF/CALLMAP |
| B2 | PM 初始化：INIT 父=自身、INIT_PID=1、sched=KERNEL | `minix3/minix/servers/pm/main.c:188-204`、`pm/const.h:9` | PID 1 锚点语义 |
| B3 | VM `exec_bootproc` 加载 init ELF，固定 argv `{"init",NULL}` | `minix3/minix/servers/vm/main.c:498-514`、`345-347` | `-s/-f` 真实 boot 不出现 |
| B4 | 解除 RTS_VMINHIBIT → kernel 首次调度 init | （01-stage-kernel 前置） | 见 `../01-stage-kernel/10-switch-to-user.md` |
| B5 | crt0 `_start` → 取 argv → 调 `main` | `minix-rt::crt0::argv_count/argv_bytes`（Rust 侧，见 `09-stage-init/todo.md:60`） | **C 侧属 libc crt0；文档未讲，见 §3 缺口 G-A** |

### 1.2 `main()` 线性出生流程（init.c:230-367）

| 步 | 动作 | 锚点 | 归属文档 |
|---|---|---|---|
| M1 | 身份校验 `getuid()!=0`→EPERM；`getpid()!=1`→"already running" | `init.c:242-249` | 01 |
| M2 | `setsid()` 建初始会话（失败仅 warn） | `init.c:255-256` | 01 |
| M3 | 非 Minix 分支 `setlogin("root")`（`#if !defined(__minix)`） | `init.c:262-265` | 01（标注不生效构建） |
| M4 | `mfs_dev()`：无 console → fork MAKEDEV；失败降级 single_user | `init.c:268-271`、def `1703` | 01 |
| M5 | `openlog("init", LOG_CONS, LOG_AUTH)` | `init.c:278` | 03 |
| M6 | `getopt "sf"`：`-s`→single_user，`-f`→FASTBOOT | `init.c:287-303` | 01 |
| M7 | 信号注册 `handle()`/`delset()`（8 类映射 + 屏蔽 + 忽略 TTIN/TTOU） | `init.c:310-334`、def `370`/`395` | 调用点 01，实现 02 |
| M8 | `close(0/1/2)` | `init.c:339-341` | 01 |
| M9 | `createsysctlnode()`（CHROOT，建 `init.root`） | def `1812` | 12 |
| M10 | `has_securelevel()` → `securelevel_present` | `init.c:353`、def `545` | 调用点 01，机制 12 |
| M11 | `transition(requested_transition)` — 交出控制权 | `init.c:358`、def `625` | 02 |

### 1.3 状态机主循环与七状态（`transition()` init.c:625-640）

主循环：`utmpx_set_runlevel(get_runlevel(current), get_runlevel(s))` → `current_state = s` → `s = (*s)()`，永不做输入解析。首状态默认 `runcom`（`init.c:195`；`LETS_GET_SMALL` 下 `single_user`，`init.c:217`；`current_state` 初值 `death`，`init.c:201`）。

| 状态 | 字符 | 函数 | def | 出口 |
|---|---|---|---|---|
| single_user | `'s'` | `single_user` | 695 | shell 正常退出 → runcom（置 FASTBOOT）；`init.c:866-870` |
| runcom | `'r'` | `runcom`→`runetcrc` | 975 / 880 | rc 零退出 → read_ttys；非零 → single_user |
| read_ttys | `'t'` | `read_ttys` | 1223 | 重建会话链表 → multi_user |
| multi_user | `'m'` | `multi_user` | 1529 | 稳态；waitpid 收割；返回被请求状态 |
| clean_ttys | `'T'` | `clean_ttys` | 1570 | 重读 ttys → multi_user（SIGHUP 触发） |
| catatonia | `'c'` | `catatonia` | 1635 | 全置 SHUTDOWN → multi_user（SIGTSTP） |
| death | `'d'` | `death` | 1662 | 三轮 kill → single_user（SIGTERM） |

**信号→状态转换表**（`transition_handler` `init.c:1503-1522`）：SIGHUP→clean_ttys、SIGTERM→death、SIGTSTP→catatonia、default→0（清零，继续当前态）。旁路 handler：`alrm_handler`（`1651`，置 `clang`）、`disaster`（`505`，致命信号 `_exit(sig)`）、`minixreboot`（`518`，SIGABRT→fork `shutdown -r`）、`minixpowerdown`（`531`，SIGUSR1→fork `shutdown -p`）。

**结论（§5.1 标准 2）**：现有 00–14 编号顺序 = 上表运行时序（入口→状态机→启动三站→会话→稳态→重读→关停→交互），**已满足「执行与因果序优先」，无需重排**。

---

## 2. 知识点全集（存量池）

> 存量条目来自现有 16 篇；新增条目由 §3 覆盖审计追加。编号 `K-###`，多 AI 对齐键 = 名称 + 锚点。主讲述点标注去重结果。

### 2.1 存量知识点（按现有文档归组）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主讲述点 |
|---|---|---|---|---|---|---|
| K-001 | PID 1 三重出生证明（身份/会话/描述符卫生） | 概念 | 存量 | 01 | init.c:242-249,255,339 | 01 |
| K-002 | 首状态双默认值（runcom 195 / single_user 217） | 约束 | 存量 | 01/02/99 | init.c:195,217 | 01（99 收录，02 引用） |
| K-003 | mfs_dev 三层探测（快路径/死代码/MAKEDEV 兜底） | 机制 | 存量 | 01 | init.c:1703-1788 | 01 |
| K-004 | boot argv 固定 `{"init",NULL}`，-s/-f 仅运维后门 | 接口/协议 | 存量 | 01/14 | vm/main.c:345-347 | 14（01 引用） |
| K-005 | state_t 函数指针类型体操 | 数据结构 | 存量 | 02 | init.c:130-131 | 02 |
| K-006 | 七状态字符与转换（桥模型：信号只写请求） | 机制 | 存量 | 02 | init.c:133-139,1503 | 02 |
| K-007 | handle/delset 注册 + `XXX SA_RESTART?` 未决 | 接口 | 存量 | 02 | init.c:370,395,384 | 02 |
| K-008 | transition 主循环 + utmp 挂钩内嵌 | 机制 | 存量 | 02 | init.c:625-640 | 02（utmp 机制→13） |
| K-009 | 三级日志 stall/warning/emergency（停 30 秒读屏） | 机制 | 存量 | 03 | init.c:441,458,473 | 03 |
| K-010 | disaster 致命信号 `_exit(sig)` | 机制 | 存量 | 03 | init.c:505 | 03 |
| K-011 | print_console(#if 0)/badsys(非 minix) 死代码 | 约束 | 存量 | 03 | init.c:412,491 | 03 |
| K-012 | 口令门布尔式 + SECURE/ALTSHELL | 机制 | 存量 | 04 | init.c:747-783 | 04 |
| K-013 | single_user 看护五结局 | 机制 | 存量 | 04 | init.c:836-870 | 04 |
| K-014 | 安全级降级调用点 | 约束 | 存量 | 04/12 | init.c:723-725 | 12（04 引用） |
| K-015 | rc argv 组装（autoboot/fastboot） | 接口 | 存量 | 05 | init.c:897-900 | 05 |
| K-016 | runcom 两次执行 + chroot 决策 | 机制 | 存量 | 05 | init.c:990-1005 | 05（chroot 机制→12） |
| K-017 | rc 五归宿（fork失败睡30秒） | 机制 | 存量 | 05 | init.c:917-968 | 05 |
| K-018 | ttys 四列格式 + TTY_ON/SECURE + do_setttyent chroot 感知 | 数据结构 | 存量 | 06 | init.c:1222,1792 | 06 |
| K-019 | getttyent 字段语义（引号模式/精确 token/off 清除/window=） | 机制 | 存量 | 06 | getttyent.c:skip | 06 |
| K-020 | session_t 全字段 + SE_* 标志 + se_started 防抖 | 数据结构 | 存量 | 07 | init.c:157-169 | 07 |
| K-021 | new_session 四关 + construct_argv 分词 | 机制 | 存量 | 07 | init.c:1143,1102 | 07 |
| K-022 | 会话 DB = pid→session 内存哈希（ARCH A-1） | 数据结构/架构演进 | 存量 | 08 | init.c:1022,1039,1063,1081 | 08 |
| K-023 | multi_user 稳态：全启动 + waitpid(-1) 收割 | 机制 | 存量 | 09 | init.c:1529-1564 | 09 |
| K-024 | setctty 控制终端成人礼 | 机制 | 存量 | 09 | init.c:670 | 09 |
| K-025 | 防抖三常量（子进程内睡 / 稳态 waitpid 无 WUNTRACED / 安全级 ==0 才升） | 约束 | 存量 | 09 | init.c:92-94,1559,1541 | 09 |
| K-026 | collect_child 重启/摘链 | 机制 | 存量 | 09 | init.c:1461 | 09 |
| K-027 | clean_ttys 存在性标记 diff（n² 算法） | 机制 | 存量 | 10 | init.c:1570,1567 | 10 |
| K-028 | catatonia 温柔关停 vs death 三轮杀 | 机制 | 存量 | 11 | init.c:1635,1662 | 11 |
| K-029 | DEATH_WATCH/alrm/clang 计时 + ESRCH/ECHILD 早退 | 机制 | 存量 | 11 | init.c:1684-1689 | 11 |
| K-030 | securelevel has/get/set（能力探测先行） | 机制 | 存量 | 12 | init.c:545,569,595 | 12 |
| K-031 | createsysctlnode/shouldchroot（init.root） | 机制 | 存量 | 12 | init.c:1812,1860 | 12 |
| K-032 | utmpx 账本：LOGIN/DEAD/RUN_LVL/BOOT/SHUTDOWN | 接口/协议 | 存量 | 13 | init.c:1373,1384,1412,1430 | 13 |
| K-033 | 空会话短路（sessions==NULL 时 /var 未挂载不记账） | 约束 | 存量 | 13 | init.c:1438-1441 | 13 |
| K-034 | USR_F 非服务身份 + 众所周知用户进程 | 概念/架构 | 存量 | 14 | rs/table.c:28,procfs/service.c | 14 |
| K-035 | 两挂钩信号转进程（Ctrl-Alt-Del→SIGABRT / 低电→SIGUSR1） | 接口 | 存量 | 14 | init.c:518,531; keyboard.c:300; tps65217.c:226 | 14 |
| K-036 | PM 孤儿收养 / INIT 死只栈回溯 / reboot 停 init | 接口/协议 | 存量 | 14 | pm/forkexit.c:336,396; pm/misc.c:224 | 14 |
| K-037 | 常量/路径/全局状态总表 | 数据结构 | 存量 | 99 | pathnames.h,paths.h,const.h | 99 |

### 2.2 新增知识点（由 §3 覆盖审计追加，来源类型=新增）

| 编号 | 名称 | 类型 | 来源 | 证据锚点 |
|---|---|---|---|---|
| K-101 | crt0 `_start` → `main` 的 C 运行时入口与 argv 获取 | 接口/机制 | **新增** | Rust `minix-rt::crt0::argv_count/argv_bytes`（`09-stage-init/todo.md:60`）；C 侧 libc crt0（跨 stage，边界声明） |
| K-102 | InitHost 单接缝（决策/副作用分离，live ENOSYS 降级） | 架构演进 | **新增（现散落 9 篇，无主）** | `09-stage-init/todo.md` Fix #8、plan.md §4 A-11、`os/commands/sbin/init/src/host.rs` |
| K-103 | DriverState 单所有者 + ChildCollector/Ledger 视图 | 架构演进 | **新增（现散落，无主）** | plan.md §4 A-11、`00-init-overview.md`、`os/.../driver.rs` |
| K-104 | ScriptHost 剧本宿主（测试宿主 vs live 宿主双面） | 工具与工程 | **新增（现散落，无主）** | `01`§3.5、`02`§3.6、`03`§3.3、`os/.../host.rs` |
| K-105 | 构建变体宏（LETS_GET_SMALL/SECURE/ALTSHELL/CHROOT/SUPPORT_UTMP(X)/MFS_DEV_IF_NO_CONSOLE）→ Rust feature | 架构演进 | **新增（现 25 处散落，仅 A-7 一行）** | `minix3/sbin/init/Makefile`、plan.md §4 A-7 |
| K-106 | 信号投递机制的归属边界（PM 投递 + kernel trap + sigreturn 桩属邻接 stage） | 约束/边界 | **新增（未显式回答）** | `edge_todo.md` E-INITSYS（minix-rt sigreturn 桩、PM dispatch 臂） |

### 2.3 统计摘要

- 存量 37 组（覆盖 46 个 C 函数 + 外部契约 13 项 + 死代码 4 项）；新增 6 组（K-101~106）。
- 按现有文档分布：00=导航、01=5、02=4、03=3、04=3、05=3、06=2、07=2、08=1、09=4、10=1、11=2、12=2、13=2、14=3、99=1。
- 重复项：K-002（01/02/99）、K-004（01/14）、K-014（04/12）、K-016（05/12）、K-008/K-032（02/13）——均为「调用点 + 机制分离」的正确分层，标主讲述点后其余改为引用，**无越权重复展开**。

---

## 3. 覆盖审计

### 3.1 主题全集来源（四路）

1. C 符号：init.c 46 函数（§1 已枚举）、结构体（`session_t`、`ttyent`）、宏（7 状态字符、超时常量、SE_*）、`#if 0`/非 minix 死代码。
2. OS 通用概念：状态机、会话/控制终端、孤儿收养、信号异步边界、守护进程范式、防抖动、runlevel。
3. 非 C 制品：boot 镜像登记、ELF 加载、crt0 入口、Makefile 构建变体、utmp 线格式、信号投递 trampoline。
4. 边界契约：13 项跨服务契约（§5.3）。

### 3.2 覆盖缺口表

| 缺口 | 主题 | 建议 | 依据 |
|---|---|---|---|
| G-A | crt0 `_start`→main + argv 获取 | **并入 01**（新增 §0.5「从 ELF 入口到 main」），显式声明 libc crt0 语义属 runtime stage，本 stage 只讲「init 拿到的 argv 从哪来」 | K-101；01 现从 `main` 起，读者不知 argv 出处 |
| G-B | Rust 机器接缝架构（InitHost/DriverState/ScriptHost） | **新建 15 篇**（§4）；00 只留一句指针 | K-102/103/104；9 篇引用 `[ARCH: init-host-seam]` 却无主文档 |
| G-C | 构建变体宏 → feature 映射 | **并入 99**（A-7 展开为完整表：宏名→门控的代码路径→Rust 默认） | K-105；25 处散落无归口 |
| G-D | 信号投递/返回机制归属 | **并入 14**（新增边界段），或 02 尾声明「投递属 PM+kernel，见 E-INITSYS」 | K-106；当前只讲注册不讲投递 |

### 3.3 重复主题表

无需删并重（§2.3 已列的分层属正确设计）。仅统一主讲述点归属：K-002 主 01、K-014/K-016 主 12、K-004 主 14。

### 3.4 越界主题表

| 篇 | 越界内容 | 正确归属 |
|---|---|---|
| 01 | §3.4「完整七状态枚举在 02 定义」——已声明子集并注释，**不算越界**（符合 §5.1 标准 3 首次即完整的最小暴露） | — |
| 02 | §3.5/§3.6 展开 SignalState/InitHost 实现细节，与 G-B 主文档重叠 | 移交新 15 篇做全景；02 保留 handler 语义所需的最小提及 |
| 05 | §3 展开两次 chroot 执行流程，`should_chroot` 机制在 12 | 05 保留调用点，机制引用 12（现状已如此，确认即可） |

### 3.5 非 C 主题逐项回答（§步骤 3 固定清单）

| 主题 | 在哪讲 / 为何不在本 stage |
|---|---|
| 链接与加载 | ELF 由 VM `exec_bootproc` 加载（`vm/main.c:498-514`）：00/14 声明为外部契约；加载实现属 02-stage-vm，不重复展开 |
| 镜像与内存布局 | boot_image 末项 `table.c:64`：00/14；内存布局属 01-stage-kernel 前置 |
| 汇编入口与陷阱进入 | crt0 `_start`→main（G-A→01）；信号 sigreturn 桩属 runtime/PM（G-D→14/E-INITSYS） |
| 启动装配 | boot 登记 + USR_F + PM 父=自身：00/01/14（已覆盖） |
| 构建与工具链 | Makefile 变体宏（G-C→99）；crate 接线（G-B→15） |
| 跨模块接口与线格式 | 信号族 + utmp/wtmpx 格式 + shutdown 命令行：02/13/14（已覆盖） |
| 错误路径 | fork/exec/wait 失败归宿：04/05/09 各状态内联（已覆盖） |
| 关闭与退出 | death 三轮 + catatonia + reboot：11/14（已覆盖） |
| 并发与同步 | 单线程状态机 + 异步信号边界（`requested_transition` 桥）：02（已覆盖） |
| 测试基建 | ScriptHost 剧本宿主 + 三层测试：G-B→15 集中，各篇 §5 保留清单 |

---

## 4. 新目录（推荐：保留 00–14+99 主线骨架，增量三处结构操作，不重编号）

> **为什么不整体重排/重编号**：§1 已证现有序号 = 运行时序，重排零读者收益；目录内交叉引用约 **150 处**、跨 stage 引用 5 处，重编号纯制造断链（触发 `01-stage-kernel/todo.md` I-14 历史教训「重编号断链风险大于收益」）。故本蓝图的重建 = **保留编号骨架 + 逐篇按契约重写正文 + 三处增量操作（一新增、两并入、零归档）+ 一项跨篇锚点修正**。

| 编号 | 标题 | 一句话定位 | 变更 |
|---|---|---|---|
| 00 | init 总览 | 是什么、boot 位置、状态机图、导航 | 重写：Rust 实现形态段瘦身为指向 15 的一句话 |
| 01 | 入口与进程身份 | 从 ELF 入口到 transition 的线性出生 | 重写 + **并入 G-A**（新增 §0.5 crt0→main→argv） |
| 02 | 状态机骨架与信号转换 | 桥模型：信号写请求、主循环认领 | 重写（接缝细节移交 15） |
| 03 | 日志与致命信号 | 停 30 秒的日志 + disaster | 重写 |
| 04 | 单用户抢修态 | 口令门 + shell + 看护循环 | 重写 |
| 05 | 运行启动脚本 | /etc/rc 黑盒，退出码是唯一协议 | 重写 |
| 06 | 读终端表与会话重建 | ttys → 会话链表 | 重写 |
| 07 | 会话结构与生命周期 | session_t 内存化身 | 重写 |
| 08 | 会话数据库 | pid→会话反向索引（ARCH A-1） | 重写 |
| 09 | 多用户稳态 | 全启动 + waitpid 收割 | 重写 |
| 10 | 重读终端表 | 存在性标记 diff | 重写 |
| 11 | 假死与关机 | 标 SHUTDOWN vs 三轮杀 | 重写 |
| 12 | 安全级别与新根 | 两个运行时旋钮 | 重写 |
| 13 | 会话日志账本 | 给 who/last 看的账 | 重写 |
| 14 | 对外契约 | 众所周知的用户进程 | 重写 + **并入 G-D**（信号投递归属边界） |
| **15** | **Rust 实现架构：机器接缝与宿主** | **决策/副作用分离的唯一接缝总览** | **新增**（G-B） |
| 99 | 常量/路径/全局/构建变体总表 | 全篇共用查表 | 重写 + **并入 G-C**（构建变体表） |

**阅读路径**：主线 01→02→03→04→05→06→07→08→09→10→11（状态机运行时序）；支线（可跳读，机制查阅型）12/13/14；元层 00（入口导航）、15（实现架构，代码落地读者在 02 后随时可读）、99（查表）。

---

## 5. 每篇契约（仅列发生变更/新增的 4 篇；未列篇章沿用现状七要素契约，B 相按 §7 锚点修正 + 正文重写执行）

### 15-init-rust-architecture（新增，G-B）

- **一句话定位**：把散落在 01/02/03/08/09/12/13 的 `[ARCH: init-host-seam]` 收敛为一篇——init 的 Rust 重写如何把「决策」与「机器副作用」分离。
- **讲什么**：K-102（InitHost 单接缝面：fork/exec/waitpid/kill/信号安装/setsid/ctty/uid/alarm/时钟/睡眠/路径探测/console，live 对缺失 minix-sys 封装诚实 ENOSYS）、K-103（DriverState 单所有者切成 ChildCollector/Ledger 两视图，消 C 全局别名）、K-104（ScriptHost 剧本宿主 vs MinixSysHost，三层测试）、K-106（接缝之下的边界面：E-INITSYS live 半等待）。
- **不讲什么**：各状态函数语义（04~11）、信号→状态映射（02）、日志级别（03）——本篇只讲**承载它们的宿主结构**。
- **前置**：02（知道有哪些机器动作被推迟）、01（entry 决策/执行分层）。后置：无（元层，被 00 指向）。
- **事实底线**：C 侧 `init.c` 无对应结构（libc 直调散布各函数，plan §4 A-11 原语）；Rust 侧 `os/commands/sbin/init/src/host.rs`、`driver.rs`、`signal_state.rs`、`wait.rs`；跨 stage 依赖 `edge_todo.md` E-INITSYS。
- **知识点清单**：K-102/103/104/106（来源全为新增，锚点见 §2.2）。
- **验收标准**：读者能回答「为什么七个按关注点拆的 seam trait 收敛成一个 InitHost」「fork 失败/时钟不可读时 live 宿主怎么降级、剧本宿主怎么断言」；必须画出 InitHost ↔ DriverState ↔（ChildCollector/Ledger）↔ 状态函数 的关系图；必须给出「决策纯函数 + host trait 边界」与 C「边解析边改全局」的对比表。

### 01-init-main-entry（重写 + 并入 G-A）

- **新增 §0.5「从 ELF 入口到 main」**：承载 K-101——init 作为 VM `exec_bootproc` 加载的 ELF，真实入口是 crt0 `_start`，其经 `minix-rt::crt0::argv_count/argv_bytes` 取 boot argv（固定 `{"init",NULL}`，`vm/main.c:345-347`）后调 `main(argc,argv)`。**边界声明**：crt0/trap 属 runtime/01-stage-kernel，本篇只讲「main 收到的 argv 从哪来、为何 boot 时非空参数不出现」。
- 其余七要素不变（现契约已完整）。验收增补：读者能回答「`-s`/`-f` 何时真正出现」（答：运维手动/非 boot 路径，因 VM 固定 argv）。

### 99-init-global-concepts（重写 + 并入 G-C）

- **展开构建变体表**（K-105）：`LETS_GET_SMALL`（minix 默认不启用→Rust 不实现）、`SECURE`（口令门 `init.c:747-763`）、`ALTSHELL`（备用 shell `768-783`）、`CHROOT`（`createsysctlnode`/`shouldchroot`）、`SUPPORT_UTMP`+`SUPPORT_UTMPX`（双日志，Makefile 同开）、`MFS_DEV_IF_NO_CONSOLE`（`init.c:268-271`）；每条列「宏 → 门控的代码路径 → Rust feature/默认取舍」。锚点：`minix3/sbin/init/Makefile`、plan §4 A-7。
- 验收：每个宏都能指到 `init.c` 生效/非生效构建的具体行为差异。

### 14-init-external-contracts（重写 + 并入 G-D）

- **新增「信号投递的归属边界」段**（K-106）：02/03/14 讲的都是 handler **注册**与**语义**；信号**如何送达** init（PM 投递 → kernel trap → 用户态 trampoline → `sigreturn`）**不在 init 内**，属 `04-stage-pm` + `14-stage-runtime`，经 `edge_todo.md` E-INITSYS 关联。本篇补一句显式边界，防读者误以为投递链是 init 的一部分。
- 其余不变。

---

## 6. 变更表

| 操作编号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|---|---|---|---|---|---|---|
| O-1 | 新建 | — | 15 | InitHost/DriverState/ScriptHost 被 9 篇引用却无主文档 | K-102,103,104,106（新增） | 来源=C 无对应 + Rust host.rs/driver.rs + A-11 |
| O-2 | 并入 | （缺） | 01 §0.5 | crt0→main→argv 入口未讲 | K-101（新增） | 来源=minix-rt crt0 + vm/main.c:345 |
| O-3 | 并入 | A-7 一行 | 99 表 | 构建变体宏 25 处散落无归口 | K-105（新增） | 来源=Makefile |
| O-4 | 并入 | （缺） | 14 边界段 | 信号投递归属未答 | K-106（新增） | 来源=E-INITSYS |
| O-5 | 改写（跨篇） | 01~14/99 头部 27 个工具生成锚点 |  bare `init.c:NNN` | 锚点符号名全错（§7） | 非知识点，呈现层 | 见 §8 F1 |
| O-6 | 瘦身 | 00 Rust 实现形态段 | 00 一句指针→15 | 避免与 15 重复 | K-102 主讲述点迁 15 | 存量搬迁 |
| O-7 | 移交 | 02 §3.5/§3.6 接缝细节 | 15（02 保留最小提及） | 越界收敛 | K-102/104 | 存量搬迁 |
| O-8 | 数量更正 | 00/README 称「19 模块」 | 更正为 20 | 实读 `os/commands/sbin/init/src/` 20 个 .rs | 文档-代码同步 | — |

存量知识点全部有去向（未列的 03~13 保留原篇），新增 K-101~106 全部有证据锚点，**无凭空条目、无丢失条目**。

---

## 7. 缺漏新篇（G-A~G-D 逐项落实）

- **G-A crt0→main**：主题=init 真实入口；重要性=读者困惑「argv 哪来」；原料=`vm/main.c:345-347` + Rust crt0；归 01 §0.5；验收=能解释 boot 路径 argv 恒为 `{"init",NULL}`。
- **G-B 机器接缝架构**：主题=决策/副作用分离；重要性=被 9 篇 `[ARCH: init-host-seam]` 引用却 homeless；原料=host.rs/driver.rs/signal_state.rs + A-11；归新 15；验收=关系图 + 纯函数/trait 对比表。
- **G-C 构建变体**：主题=编译宏→feature；重要性=SECURE/ALTSHELL/CHROOT 行为散落各状态篇；原料=Makefile + A-7；归 99；验收=每宏指到具体代码差异。
- **G-D 信号投递边界**：主题=投递/返回非 init 职责；原料=E-INITSYS；归 14；验收=显式声明跨 stage 归属。

四项均已落实为篇章，无「待定」。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（跨篇，覆盖全部变更文档）

| 旧位置 | 旧内容（一句话） | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 00「Rust 实现形态」整段 | InitHost/DriverState 概述 | 15 主体；00 留指针 | 拆分+瘦身 | 低风险 |
| 01 头部 `init.c:make_utmpx（L229，工具生成）` | main 锚点（符号名错） | `init.c:230` | 改写（F1） | 27 处同病，见 8.3 |
| 01 §1 后 | （无 crt0 入口） | 新增 §0.5 | 新增 | 无断链 |
| 02 §3.5/§3.6 | 接缝/SignalState 细节 | 15；02 缩为最小提及 | 合并+移交 | 需同步 02 §7 参见加 15 |
| 99 A-7 一行 | 构建变体 | 99 表 | 改写 | 无断链 |
| 14 尾部 | （无投递边界） | 新增段 | 新增 | 无断链 |
| 新增 15 | — | 新文件 | 新建 | 需 00/README/SUMMARY 收录 |

### 8.2 引用迁移表

| 旧引用 | 出现处 | 新目标 | 验证方式 |
|---|---|---|---|
| `[ARCH: init-host-seam]`（01/02/03/08/09/12/13/14/99） | 各篇接缝提及 | 追加「详见 15」一行 | grep `init-host-seam` 后就近有指向 15 的引用 |
| 「机制见 12 / 见 13」 | 04/05/09 | 不变（编号未动） | 无需改 |
| 00/README「19 模块」 | 2 处 | 改「20 模块」 | `ls os/commands/sbin/init/src/*.rs | wc -l` |
| `../01-stage-kernel/06/09/10`、`../04/05/03-stage` | 前置引用 | 不变 | 无需改 |
| 代码注释 `covered in NN` / `see NN-doc` | grep `os/commands/sbin/init/src` | 编号未变，无需改 | — |

### 8.3 断链成本摘要

- **目录内交叉引用**：约 150 处（`见 NN` + `NN-init-*.md` 文件名 + `../0N-stage`）——**这正是拒绝整体重编号的核心依据**；本蓝图不动 01–14/99 编号，此 150 处引用零改动。
- **跨 stage 对 09 的引用**：5 处（`edge3.md`、`10/11/12-stage plan.md`「参照 README 模式」、`00-master-plan/README.md`），均引用目录/README 而非具体编号篇，**新增 15 不破坏它们**。
- **锚点修正（F1，独立于重建）**：27 处 `[X]（LN，工具生成）` 锚点的函数名与声明行不匹配（脚本逐条比对 0/27 相符；如 01 把 `main` 标成 `make_utmpx`、02 把 `transition` 标成 `setsecuritylevel`、09 把 `multi_user` 标成 `transition_handler`）。**行号正确、符号名错误**。修正方式：B 相重写每篇头部/参见锚点为 `init.c:NNN`（去符号名）或回填正确符号名（`main`/`transition`/`stall`/`single_user`/`runcom`/`runetcrc`/`read_ttys`/`start_session_db`/`multi_user`/`setctty`/`clean_ttys`/`catatonia`/`has_securelevel`/`session_utmpx`/`clear_session_logs`/`minixreboot`）。可批量：`sed` 按 §1.2/§1.3 已核表替换，或交由 `tools/anchor-resolve.sh` 重解析。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：新目录仅追加 15（元层，读者 02 后可读）；01→15、02→15 为「实现细节指向合成篇」，非概念前向依赖。主线 01–11 顺序 = 运行时序，**零读者级前向引用**。PASS（附注：现「机制见 12/13」为调用点/机制分层的合规后置引用，不违规）。
2. **依赖图无环**：00→{01,15}；01→02→{03,04}；04→05→06→{07,12}；07→08→09；09→{10,11,13}；14→{01,02,09,11}；15→{01,02}；99→无。逐边核对更早或同级，**无环**。PASS。
3. **覆盖率 100%**：存量 K-001~037 全部有归属篇；新增 K-101~106 全部落 §7 具体篇章；无「找不到去向」的新增条目被拒。PASS。
4. **断链成本**：已量化（§8.3：目录内 ~150、跨 stage 5、锚点 27），核心决策（不重编号）正是为规避 150 处断链。PASS。

### 9.2 自检门

| 门 | 结果 |
|---|---|
| G1 真序逐条可核对 | ✅ §1 锚点 = 独立 grep 声明行，与文档 §5.2 一致（随机抽 main@230/transition@625/multi_user@1529/death@1662/new_session@1143 均符） |
| G2 每个 C 文件/制品有归属或排除 | ✅ 46 函数 + 死代码 4（print_console/badsys/#if 0 段/_PATH_SLOGGER）+ 非 C 制品逐项（§3.5） |
| G3 前向引用为零 | ✅（合规后置引用已区分） |
| G4 依赖图无环 | ✅ |
| G5 覆盖率 100%，新增有锚点 | ✅ |
| G6 拆分/合并写清去向，新建写清来源 | ✅ O-1~O-8 逐条；存量搬迁（O-6/O-7）与新增来源（O-1~O-4）分列 |
| G7 每篇契约七要素齐全 | ✅ 变更 4 篇给全契约；未变 12 篇现状契约已七要素齐（本蓝图确认沿用） |
| G8 锚点迁移表覆盖变更文档每节、引用表覆盖文档+代码 | ⚠ 目录编号未变，引用零迁移；变更限于 01/02/14/99/00 + 新 15；代码注释无编号依赖（§8.2）。F1 锚点修正单列 8.3 |
| G9 事实断言都有锚点，推测已标注 | ✅ 无「待验证」遗留（E-INITSYS 客户端面状态引 edge_todo 现况） |

### 9.3 结论

09-stage-init 是**已高度收敛**的 stage：现有 00–14+99 编号顺序即运行时序，覆盖经两轮 review 与本次独立核对均为 46/46 完整。**不建议整体重排/重编号**（150 处内部引用断链，零读者收益，触发 I-14 教训）。

**推荐 B 相执行的重建 = 增量而非推倒**：
1. 新增 **15-init-rust-architecture**（收编 homeless 的 InitHost/DriverState/ScriptHost 接缝架构，G-B）；
2. 并入 **G-A（01 §0.5 crt0→main）**、**G-C（99 构建变体表）**、**G-D（14 信号投递边界）**；
3. 跨篇执行 **F1 锚点修正**（27 处工具生成符号名全错）与 **O-8 模块数更正（19→20）**；
4. 其余各篇按现有契约重写正文（不新增编号、不搬目录）。

**待用户裁决**：
- (a) 新增篇用 **15**（尾部追加，最小扰动）还是插入语义位（如 02.5/14.5，会牵动编号）？本蓝图默认 15。
- (b) F1 锚点修正是否并入本 stage 的 B 相，还是交 `tools/anchor-resolve.sh` 全库批量处理（跨 stage 工具动作）？
