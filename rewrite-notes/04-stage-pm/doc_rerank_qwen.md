# 04-stage-pm 文档重建蓝图（qwen）

## 执行头部

```text
your_name(AI agent name) = qwen
target_dir(关注的工作目录) = 04-stage-pm
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 04-stage-pm/doc_rerank_qwen.md，不改任何正文。
约束 = 未引用 .design/ 与 tmp_design_and_todo/；产物文件名带 _qwen 后缀；
       未读取任何其它 AI 的 doc_rerank_* 产物（本目录已存在的
       doc_rerank_deepseek.md / doc_rerank_glm.md 全程未打开）。
```

---

## 0. 元数据

- **执行者**：qwen
- **日期**：2026-09-19
- **目标目录**：`rewrite-notes/04-stage-pm/`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`561cf097b`

### 审查范围

**算作文档**：编号文档 `01`~`20`、总览 `00`、全局概念 `99`（共 22 篇）。
**算参考材料（不重建、不计数）**：`plan.md`、`todo.md`、`draft/`（旧 fork 主线素材，plan.md §8 已声明停止维护）。
**范围外**：`.design/`、`tmp_design_and_todo/`（项目规范：中间产物，正式文档引用即 P0-process-violation）；`01-stage-kernel/`、`02-stage-vm/`、`03-stage-rs/`、`05-stage-vfs/`、`06-stage-sched/` 的正文（仅作邻接 stage 边界核对）。

### 读取清单

| 类别 | 已读 |
|------|------|
| 本目录文档 | 00/99 全文；01 全文（概念章）；01~20 全部头部声明 + 章节骨架（grep `^#`）；plan.md §3.4/§5 边界与函数归属表全读 |
| C 源码 | `main.c`（424 行全读）、`table.c`（62 行全读，47 调用注册）；15 个 .c + 6 个 .h 行数字节核对 |
| 非 C 制品 | `os/servers/pm/src/ipc/decode.rs`、`mproc/wire.rs`、`os/libs/minix-types/src/ipc/{pm.rs,message.rs}` 头部；minix-sys wrapper 面（经 plan.md §4.1 四列表） |
| 边界材料 | `00-master-plan/README.md`（启动因果链）、`edge_todo.md`（E1/E6/E7 头 + 判定规则）、`03-stage-rs/00-rs-overview.md`（前阶段边界） |
| 写法范例 | `01-stage-kernel/06-todo.md`（新文档契约写法，只借写法不搬结论） |

### 使用的命令与关键输出（证据摘录）

```bash
wc -l minix3/minix/servers/pm/*.c *.h        # 15 .c + 6 .h = 4747 行 C，与 plan.md §5.1 一致
grep -c 'CALL(' minix3/minix/servers/pm/table.c  # 注册 47 个调用（PM_EXIT..PM_GETSYSINFO）
grep NR_PM_CALLS minix3/minix/include/minix/callnr.h  # = 48（基址 + 47 个）
# 断链成本：PM 文档被引用次数（文件名形态，含跨 stage）
grep -rnoE '[0-9]{2}-[a-z-]+\.md' 04-stage-pm/*.md   # 01 被引 64、04 被引 58、11 被引 50、05/02 各 45...
grep -rnoE '[0-9]{2}-[a-z-]+\.md' os/servers/pm/src os/libs/minix-types/src  # 代码内以完整文件名引用 PM 文档
# 非 C 主题缺口探测
grep -rlniE 'decode\.rs|线格式|codec' 04-stage-pm/*.md | grep -v doc_rerank  # 仅 plan/todo 命中，正式文档零覆盖
```

---

## 1. C 真序（运行时真序重建）

### 1.1 阶段类型判定

**服务事件循环型**。PM 是用户态服务器：一次性启动段（`main` → SEF → `sef_cb_init_fresh`）+ 永续事件循环段（`receive` → 分派 → `reply`）。据此按提示词 §九 的"服务事件循环型"组织：`为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景分组的请求处理 → 查询与杂项 → 与邻接服务的协议`，并以"一次请求的生命周期"而非"逐文件"作主线。此判定与本目录 `plan.md §1.2` 的"启动顺序主线"同源，也与 `01/02/03` stage 的兄弟结构一致。

### 1.2 真序表（直接从 C 源码重建，逐条带锚点）

启动段（`main.c`）：

| # | 动作 | C 锚点 | 归属文档 |
|---|------|--------|---------|
| S1 | 注册 SEF 回调：init_fresh / init_restart / signal_manager=process_ksig | main.c:114-126 | 01 |
| S2 | `sef_startup()` 交给框架，收 RS `SEF_INIT_FRESH` 后回调 init_fresh | sef.c / sef_init.c | 01 |
| S3 | 逐槽初始化 mproc 表 + 定时器（MP_MAGIC / mp_sigact / mp_eventsub） | main.c:147-152 | 01（调用面）/02（结构）/14（timer init） |
| S4 | 构建信号三集合 core/ign/noign_sset | main.c:157-165 | 99（定义）/11（消费） |
| S5 | `sys_getmonparams` 取 boot 参数 | main.c:169-170 | 01/20 |
| S6 | `sys_getimage` 取 boot image 表 | main.c:175-176 | 01 |
| S7 | 遍历 image 填充 INIT + 系统进程（身份/父/PRIV_PROC/nice/endpoint） | main.c:178-229 | 01（循环）/03（get_free_pid）/16（nice） |
| S8 | 逐条 `VFS_PM_INIT` 同步 + 末条 NONE `sendrec` 屏障 | main.c:220-236 | 01/05 |
| S9 | `system_hz = sys_hz()` | main.c:238 | 01/14/19 |
| S10 | `sched_init()` | main.c:241 → schedule.c | 01（调用点）/16（实现） |

事件循环段（`main.c:59-107`，每轮）：

| # | 动作 | C 锚点 | 归属文档 |
|---|------|--------|---------|
| L1 | `sef_receive_status(ANY)` 收消息 | main.c:61 | 04 |
| L2 | `is_ipc_notify` → CLOCK 则 `expire_timers(m_notify.timestamp)` 后 continue | main.c:65-71 | 04/14 |
| L3 | 提取 who_e/who_p/mp/call_nr；`pm_isokendpt` 验证 caller | main.c:74-78 | 03/04 |
| L4 | EXITING 进程的延迟调用直接丢弃（continue） | main.c:81-82 | 04/09 |
| L5 | `IS_VFS_PM_RS` 且源为 VFS → `handle_vfs_reply`，result=SUSPEND | main.c:84-87 | 04/05 |
| L6 | `PROC_EVENT_REPLY` → `do_proc_event_reply` | main.c:88-89 | 04/06 |
| L7 | `IS_PM_CALL` → `call_index=call_nr-PM_BASE` → `call_vec[]` 或 ENOSYS | main.c:90-103 | 04/表 |
| L8 | 各 call_vec 臂 → do_fork/do_exit/do_wait4/do_kill/… | table.c:15-61 | 07~20 |
| L9 | `result != SUSPEND` 则 `reply(who_p, result)` | main.c:106 | 04/05 |
| L10 | `reply` 填 mp_reply.m_type 后 `ipc_sendnb` | main.c:249-270 | 04 |
| L11 | `handle_vfs_reply` 11 路 switch + 尾部 `restart_sigs` | main.c:294-424 | 05/13 |

**真序结论**：现文档的 01→20 编号严格覆盖 S1-S10 + L1-L11 的时序，无逆序步骤。分派表 `call_vec[47]` 的臂按"调用群"落到 07~20，与真序无冲突（同一轮内各臂彼此独立，属并行体，§5.2）。

---

## 2. 知识点全集（存量池 + 新增池）

### 2.1 知识点池总表（按归属聚类，编号 stage 内唯一）

存量条目来源为现文档 + C 源码；新增条目来源标注为 C 锚点、非 C 制品或操作系统理论。**类型**：概念/机制/数据结构/接口协议/约束不变量/架构演进/工具工程/测试。

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 |
|------|------|------|------|---------|------|
| K-001 | PM 四重权威（表所有者/信号管理/生命周期编排/记账） | 概念 | 存量 | 00 | 00 §1.2 |
| K-002 | SEF 注册—分发框架与三回调 | 机制 | 存量 | 01 | main.c:114-126 |
| K-003 | `sef_cb_init_fresh` 八步依赖链 | 机制 | 存量 | 01 | main.c:147-241 |
| K-004 | boot image 填充第一代进程树（INIT 自父/系统进程挂 RS） | 机制 | 存量 | 01 | main.c:188-215 |
| K-005 | VFS_PM_INIT 逐条 send + 末条 sendrec 屏障 | 协议 | 存量 | 01/05 | main.c:220-236 |
| K-006 | `struct mproc` 全字段 + 19 flag 正交位 + mpsigact 独立表 | 数据结构 | 存量 | 02 | mproc.h:24-104 |
| K-007 | mproc[NR_PROCS]/procs_in_use/pm_isokendpt/find_proc | 数据结构 | 存量 | 03 | glo.h、utility.c:108 |
| K-008 | PID 生成器 get_free_pid（NR_PIDS 轮转、先自增相位） | 机制 | 存量 | 03 | utility.c:get_free_pid |
| K-009 | 主循环三路分发 + EXITING 丢弃 | 机制 | 存量 | 04 | main.c:59-107 |
| K-010 | `call_vec[47]` 分发表 + 调用号 | 接口 | 存量 | 04 | table.c、callnr.h |
| K-011 | SUSPEND → `ReplyIntent` 三变体异步回复模型 | 架构演进 | 存量 | 04 | main.c:106 / dispatcher.rs:49 |
| K-012 | `reply()` 复用持久 mp_reply 载荷 | 机制 | 存量 | 04 | main.c:249-270 |
| K-013 | tell_vfs / VFS_CALL 置位 / 11 路 handle_vfs_reply | 协议 | 存量 | 05 | utility.c:tell_vfs、main.c:294-424 |
| K-014 | NEW_PARENT / UNPAUSED 标志语义 | 约束 | 存量 | 05 | main.c:327-331 |
| K-015 | PROC_EVENT 订阅/发布、NR_SUBS=4 串行化 | 机制 | 存量 | 06 | event.c |
| K-016 | do_fork 全链路（槽位/vm_fork/复制/get_free_pid/VFS_PM_FORK/tracer SIGSTOP） | 机制 | 存量 | 07 | forkexit.c:do_fork |
| K-017 | do_srv_fork（RS 专用/PRIV_PROC 继承/UID-GID 注入） | 机制 | 存量 | 08 | forkexit.c:do_srv_fork |
| K-018 | do_exit→exit_proc→exit_restart + zombify + check_parent + 收养 | 机制 | 存量 | 09 | forkexit.c:246-417 |
| K-019 | do_wait4 三环 + tell_parent/tell_tracer + rusage + TOLD_PARENT | 机制 | 存量 | 10 | forkexit.c:475-806 |
| K-020 | do_kill/do_srv_kill/check_sig/sig_proc/process_ksig + 权限/广播 | 机制 | 存量 | 11 | signal.c:197/207/294/384 |
| K-021 | do_sigaction/sigprocmask/sigpending/sigsuspend/sigreturn/sig_send + SA_* | 接口 | 存量 | 12 | signal.c:40-196/776-855 |
| K-022 | check_pending/restart_sigs/unpause/stop_proc/try_resume + DELAY_CALL/SIGSNDELAY | 机制 | 存量 | 13 | signal.c:226-293/652-776 |
| K-023 | do_itimer/set_alarm/check_vtimer + ITIMER 三族 + CLOCK notify 接线 | 机制 | 存量 | 14 | alarm.c |
| K-024 | do_get/do_set（uid/gid/groups/session）+ TAINTED + VFS 转发 | 接口 | 存量 | 15 | getset.c |
| K-025 | sched_init/sched_start_user/sched_nice + nice↔queue 双射 | 机制 | 存量 | 16 | schedule.c、main.c:275 |
| K-026 | do_exec/do_newexec/exec_restart/do_execrestart + PARTIAL_EXEC/tracer | 机制 | 存量 | 17 | exec.c |
| K-027 | do_trace 全 T_* + trace_stop + W_STOPCODE | 接口 | 存量 | 18 | trace.c、ptrace.h |
| K-028 | do_time/stime/getres/gettime/settime + 双时钟 | 接口 | 存量 | 19 | time.c |
| K-029 | 杂项查询族 sysuname/getsysinfo/getprocnr/getepinfo/svrctl/reboot/getrusage/sprofile/mcontext | 接口 | 存量 | 20 | misc.c、profile.c、mcontext.c |
| K-030 | 常量语义半径（NR_PIDS/NO_TRACER/NO_EVENTSUB/_NSIG/NR_PROCS） | 约束 | 存量 | 99 | const.h |
| K-031 | endpoint 代际编码（防 ABA） | 数据结构 | 存量 | 99 | _ENDPOINT、pm_isokendpt |
| K-032 | 全局状态七件套显式化（ARCH A-3） | 架构演进 | 存量 | 99/03 | glo.h:16-26 |
| K-033 | mproc 分层建模（Identity/State/Resources/Context，ARCH A-1/A-2） | 架构演进 | 存量 | 02 | mproc.rs、lifecycle.rs |
| K-034 | 错误保真规约（透传型错误携带原始 errno，§4.2） | 约束 | 存量 | plan §4.2 散落 | 各错误枚举 |
| K-035 | 内核能力四列对照（能力↔trait↔minix-sys↔kernel 对端，V3） | 接口 | 存量 | plan §4.1 | exit.rs KernelGateway |
| **K-101** | **入站消息线格式：C `message` union ↔ `MessageUnion` repr(C) 镜像 + MESSAGE_PAYLOAD_SIZE 布局断言** | **接口协议** | **新增** | **无（散落 04/05/07）** | `ipc/decode.rs:1-13`、`minix/ipc.h` |
| **K-102** | **每调用一个安全解码函数（unsafe 收敛单点，ARCH A-4 消费端）** | **机制** | **新增** | **无（decode.rs 无归属文档）** | `ipc/decode.rs`（decode 臂 + SAFETY 论证） |
| **K-103** | **typed IPC：PmRequest/PmResponse/PmError + codec trait（ARCH A-4）** | **架构演进** | **新增** | **无（A-4 仅在 plan §4）** | `minix-types/src/ipc/pm.rs` |
| **K-104** | **出站快照线格式：mproc 表 → `MProcSnap`（76B/槽，SI_PROC_TAB，A-4 子集裁定）** | **接口协议** | **新增** | **无（wire.rs 无归属文档）** | `mproc/wire.rs:1-20`、`minix-types` MProcSnap |
| **K-105** | **`_ASSERT_MSG_SIZE`/布局断言纪律（C 与 Rust 双侧对齐）** | **约束** | **新增** | **无** | decode.rs const assert、ipc.h |
| **K-106** | **PM 的 no_std 构建与 cfg feature 门面（ENABLE_SYSCALL_STATS/SPROFILE/批次接线）** | **工具工程** | **新增** | **散落 01/20/99 一句** | Cargo.toml features（todo P2-1） |
| **K-107** | **PM 测试架构：seam+mock 端口替身 / run_once 端到端集成层 / "测试全绿但行为错误"教训（TSTL/CSL）** | **测试** | **新增** | **无（各篇 §5 只列测试名）** | tests/run_once_integration.rs、todo §11.4/§12.5 |
| **K-108** | **PM 单线程事件循环的并发结论（无跨 CPU 共享，!Send 合理，异于内核 BKL）** | **约束** | **新增** | **00 §1 隐含，无专节** | AGENTS.md 执行模型、init.rs PmServer |

**统计摘要**：存量 35 条（覆盖全部 15 .c + 6 .h + 47 调用 + ARCH A-1~A-13）；新增 8 条（K-101~K-108），全部带非 C 制品或代码锚点。

### 2.2 存量与新增分组

- 存量 K-001~K-035：受步骤 4 去向规则约束（不丢知识）。
- 新增 K-101~K-108：不受去向规则约束，直接以证据锚点入契约。

### 2.3 重复与主讲述点标记

- K-008 get_free_pid：主讲述点 03；01/07 仅引用（plan D-2 已裁决）。
- K-025 nice 双函数（get_nice_value↔nice_to_priority）：主讲述点 16；01 引用（plan D-4）。
- K-011/K-013 SUSPEND 与 VFS 回复：分派模型主讲述点 04，异步回复状态机主讲述点 05（plan D-3 已切边界）。
- K-101~K-105 线格式：当前**无主讲述点**（散落 04/05/07/13/17/20），是本蓝图的核心重建动因（见步骤 4/6）。

---

## 3. 覆盖审计

### 3.1 主题全集与来源（四路）

1. **C 源码符号**：109 个 C 符号（todo Gate A coverage-extract），全部落入 K-001~K-035；`callnr.h` 47 调用号全部落 04/07~20。
2. **操作系统通用概念**：进程状态机（09/10）、地址空间移交（07→VM stage）、权限模型（15/08）、信号投递回环（11~13）、CPU 记账（10/19/20）——均有归属。
3. **非 C 制品**：见 §3.4 逐项。
4. **阶段边界契约**：PM 由 RS 加载（起点定在 main()，boot image 来源回指 01-stage-kernel/06）；PM↔VM（02-stage-vm）、PM↔VFS（05-stage-vfs）、PM↔SCHED（06-stage-sched）交叉引用，不在本 stage 展开对端。

### 3.2 覆盖缺口表

| 缺口 | 说明 | 建议 | 落实 |
|------|------|------|------|
| G-a 线格式与解码（K-101~K-105） | ARCH A-4 是贯穿 47 调用的横切汇聚点；`decode.rs`/`wire.rs`/`minix-types/pm.rs` 三个活代码模块**无任何归属文档**；读者要理解"一条 PM 消息怎么在线上传输/解码"须跨 04/05/07/13/17/20 六篇拼凑 | **新建篇章**（汇聚点） | 新 21（步骤 6） |
| G-b 构建/feature 门面（K-106） | cfg feature（SYSCALL_STATS/SPROFILE）散落 01/20/99 各一句，无收口 | 并入 00 工程面小节（不独立成篇） | 操作 O3 |
| G-c 测试架构（K-107） | 各篇 §5 只列测试名，"seam+mock 端口替身""run_once 集成层""测试全绿但行为错误"的横切纪律无家 | 并入 00 新增"测试架构"小节（不独立成篇，避免与各篇 §5 重复） | 操作 O3 |
| G-d 单线程并发结论（K-108） | PM 异于内核 BKL 的执行模型未显式声明 | 并入 00 §1（一段声明） | 操作 O3 |
| G-e 汇编入口/trap 桥 | `_start`/int-33 腿/minix-rt 直桩 | **不在本 stage**（归 14-stage-runtime + edge E1） | §3.4 |
| G-f ELF 加载/内存布局/页表 | PM 自身被 RS 加载、地址空间由 VM 建 | **不在本 stage**（归 03-stage-rs / 02-stage-vm） | §3.4 |

### 3.3 重复主题表

| 主题 | 现重复展开处 | 新目录主讲述点 | 其余处置 |
|------|-------------|--------------|---------|
| 线格式解码 | 04（call_vec）/05（VFS_PM_*）/07（Wait4）/13（SigMsgWire）/20（getsysinfo） | 新 21 | 各篇改为回指 21，只保留本调用族字段选取 |
| get_free_pid | 01/03/07 | 03 | 01/07 引用（已如此） |
| nice 换算 | 01/16 | 16 | 01 引用（已如此） |

### 3.4 越界主题表

| 篇 | 越界内容 | 正确归属 |
|----|---------|---------|
| 04 | 内联讲 union 字段选取（decode 细节） | 移交新 21 |
| 12 | §8 独立"接线（S3 余件）"破坏 7 章模板 | 归并进 §4 实现详解（原地，无重编号） |

### 3.5 非 C 主题逐项回答（提示词固定清单）

| 非 C 主题 | 在哪里讲 / 为何不在本 stage |
|-----------|---------------------------|
| 链接与加载 | 不在本 stage：PM 的 ELF 由 RS `srv_execve` 加载（03-stage-rs/09）；PM 文档从 `main()` 起步 |
| 镜像与内存布局 | 不在本 stage：PM 地址空间由 VM 建（02-stage-vm）；boot image 语义回指 01-stage-kernel/06 |
| 汇编入口与陷阱进入 | 不在本 stage：`_start`/trap 桥/int-33 腿归 14-stage-runtime + edge E1；01 §3.2 仅记"Rust 去框架"决策 |
| 启动装配 | 在 01（SEF 注册—分发 + 八步 init_fresh） |
| 构建与工具链 | 并入 00 工程面小节（K-106）；无独立成篇必要 |
| 跨模块接口与线格式 | **新 21**（K-101~K-105）：VFS_PM_*/SCHEDULING/PROC_EVENT/PM 调用族的消息布局与解码汇聚 |
| 错误路径 | errno 映射 + 错误保真规约（K-034）：定义入新 21，各调用族字段选取回各篇 |
| 关闭与退出 | 在 09（do_exit 链）+ 20（reboot/sys_abort）；PM 自身被 RS 终止不在本 stage |
| 并发与同步 | 在 00（K-108 单线程事件循环声明，无跨 CPU 共享） |
| 测试基建 | 并入 00 测试架构小节（K-107）；逐调用测试名仍在各篇 §5 |

---

## 4. 新目录

### 4.1 总判断：保守重排（保留全部现有编号，零重命名）

**裁决**：本 stage 现有 22 篇沿启动时序线性组织，经核对满足四条硬标准（见 §9），且与 kernel/vm/rs 三个兄弟 stage 同构。全量重排的断链成本实测极高（§8：文档间文件名引用 ≈ 800 处 + 代码内以完整文件名引用 PM 文档 + plan.md §3.4/§5.3 大量裸编号引用），触发提示词 §二的历史教训 I-14（"看不清成本的重建不做"）。因此本蓝图 **不重排、不拆分、不合并既有编号文档**，只做：

1. **新建 1 篇**（21，补齐 A-4 线格式汇聚点，加法编号，零断链）；
2. **原地小节归并/边界修订若干**（不移动文件、不改编号）；
3. **00 导航重组**（加分组、标注前置、并入工程面与测试架构）。

编号不追加到 20 之前、不重排 01-20，正是为了把断链成本压到接近零。

### 4.2 新目录总表（★ = 本篇新建，◇ = 原地修订不动编号，其余不变）

| 分组 | 编号 | 标题 | 一句话定位 | 变更 |
|------|------|------|-----------|------|
| 总览 | 00 | pm-overview | 角色边界 + 分层 + 阅读路线 + **工程面 + 测试架构 + 单线程声明** | ◇ O3 |
| 骨架（启动与运行时） | 01 | pm-init-main | PM 如何建立第一代进程世界 | ◇ O4（回指 21） |
| 骨架 | 02 | mproc-struct | 进程结构字段与状态分层 | 不变 |
| 骨架 | 03 | mproc-table | 槽位/endpoint/PID 身份管理 | 不变 |
| 骨架 | 04 | ipc-dispatch | 主循环与消息分发 | ◇ O5（decode 移交 21） |
| 骨架 | 05 | vfs-interaction | VFS 异步协议与回复状态机 | ◇ O4 |
| 骨架 | 06 | event-subscription | 进程事件发布/订阅 | 不变 |
| **骨架** | **21** | **pm-wire-codec** | **一条 PM 消息怎么在线上传输与解码（A-4 汇聚）** | **★ 新建** |
| 生命周期 | 07 | pm-fork | do_fork 全链路 | ◇ O4 |
| 生命周期 | 08 | pm-srv-fork | RS 专用特权 fork | 不变 |
| 生命周期 | 09 | pm-exit | 退出/僵尸/收养链 | 不变 |
| 生命周期 | 10 | pm-wait | wait4 三环与 rusage | 不变 |
| 信号 | 11 | signal-core | 信号生成与分发核心 | 不变 |
| 信号 | 12 | signal-handlers | 处理器安装与掩码 | ◇ O2（§8→§4） |
| 信号 | 13 | signal-flow | 延迟/停止/恢复 | ◇ O4 |
| 服务 | 14 | itimer | 定时器三族 | 不变 |
| 服务 | 15 | credentials | 身份与凭证 | 不变 |
| 服务 | 16 | scheduling | 用户态调度交接 | 不变 |
| 服务 | 17 | exec | 执行替换状态机 | ◇ O4 |
| 服务 | 18 | trace | ptrace T_* 全族 | ◇ O4 |
| 服务 | 19 | time | 时间与双时钟 | 不变 |
| 服务 | 20 | misc-queries | 杂项查询与控制面 | ◇ O4 |
| 词汇 | 99 | global-concepts | 常量/编码/三集合/全局态（建议先读） | ◇ O3（导航标注） |

### 4.3 阅读路径

- **主线（骨架）**：00 → 99（词汇，可当字典随查） → 01 → 02 → 03 → 04 → **21** → 05 → 06。
- **生命周期支线**：07 → 08 → 09 → 10。
- **信号支线**：11 → 12 → 13。
- **服务支线（并行，可跳读）**：14 / 15 / 16 / 17 / 18 / 19 / 20 各成单元，按调用群分组，无相互前置。
- **线格式回读**：任何一篇讲"字段选取"时回指 21；21 是所有调用族的共同底座。

**并行体组织（§5.2）**：07~20 属同一分派表 `call_vec[47]` 的并行臂，不强行排线性序；已按语义聚簇（生命周期/信号/服务），组内 07 作 fork 代表成员讲透"跨服务器语义编排协议"，其余按差异收束。主线（启动链 01 + 分派 04 + VFS 异步 05 + 线格式 21）与支线（各服务臂）分离声明。

---

## 5. 每篇契约

> 仅列**发生变更**的篇目契约（新建 1 + 原地修订 O2~O5 涉及篇）。未列出的 02/03/06/08~11/14~16/19 契约不变（现文档已满足七要素，见 §9 G7）。

### 21-pm-wire-codec（★ 新建）

- **一句话定位**：把散落在六篇里的"PM 消息在线上传输/解码"这一横切关注点收进一处，回答"一条 PM IPC 消息的字节布局是什么、C union 怎么变成 Rust 类型化载荷、unsafe 解码面收敛在哪"。
- **讲什么**：K-101（MessageUnion repr(C) 镜像 + 载荷尺寸上界）、K-102（每调用一个安全解码函数、unsafe 单点 + SAFETY 论证）、K-103（typed IPC PmRequest/PmResponse/PmError + codec trait、errno 映射）、K-104（出站快照 MProcSnap 76B/槽、A-4 子集裁定表）、K-105（布局断言纪律 `_ASSERT_MSG_SIZE` ↔ const assert）、K-034（错误保真规约在此定义，各调用族引用）。
- **不讲什么**：
  - 分派骨架与 `PmCall` 枚举判别（交给 04，本篇只讲"判别之后参数怎么从载荷解出"）；
  - 每条 VFS_PM_* 回复的语义处理（交给 05，本篇只讲其消息布局）；
  - 各调用族字段的业务语义（fork 的 slot、wait4 的 status 等，交给 07/10…，本篇只给字节布局与解码入口）。
- **前置**：04（分发/ReplyIntent）、99（callnr.h 调用号、com.h 消息常量）。
- **后置**：05（VFS_PM_* 布局）、07/08/09/10（生命周期载荷）、13（SigMsgWire）、17（exec 参数块）、20（getsysinfo 快照）——全部回指本篇。
- **事实底线**：
  - C：`minix/ipc.h`（message union 各成员 + `_ASSERT_MSG_SIZE`）、`com.h`（VFS_PM_* 字段）、`callnr.h`（调用号）、`forkexit.c`/`signal.c`/`trace.c` handler 开头的 `m->m_lc_pm_xxx` 字段选取；
  - 非 C 制品：`os/servers/pm/src/ipc/decode.rs`（全模块）、`os/servers/pm/src/mproc/wire.rs`、`os/libs/minix-types/src/ipc/pm.rs` + `message.rs`（MessageUnion/PmRequest/PmResponse/MProcSnap）、`MESSAGE_PAYLOAD_SIZE`。
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为何归本篇 | 来源 |
  |------|------|------|------|-----------|------|
  | K-101 | 入站消息线格式 | 接口协议 | decode.rs:1-13 / ipc.h | 布局单点 | 新增（非 C 制品） |
  | K-102 | 解码单点（unsafe 收敛） | 机制 | decode.rs | A-4 消费端 | 新增 |
  | K-103 | typed IPC + codec trait | 架构演进 | minix-types/ipc/pm.rs | A-4 本体 | 新增 |
  | K-104 | 出站快照 MProcSnap | 接口协议 | mproc/wire.rs:1-20 | A-4 子集 | 新增 |
  | K-105 | 布局断言纪律 | 约束 | decode.rs const assert | 双侧对齐 | 新增 |
  | K-034 | 错误保真规约 | 约束 | plan §4.2 / 各错误枚举 | 线格式是错误载体 | 存量（现散落） |

- **验收标准**：读者能独立回答——(a) 一条 `m_lc_pm_wait4` 请求从 C union 到 Rust `PmRequest::Wait4{..}` 经过哪几步、unsafe 出现在哪一处、为什么限制在 Copy 结构按值读取；(b) 为什么 MProcSnap 只镜像 76 字节而非整个 mproc、裁掉字段的判据是什么；(c) `_ASSERT_MSG_SIZE` 与 Rust const assert 各自防的是什么事故。必须画出"C message union ↔ MessageUnion ↔ decode_xxx ↔ PmRequest ↔ handler"的映射关系图；必须给出 A-4 子集裁定表（mp_reply[64]/mp_sigact/mp_sgroups… 为何不进快照）。

### 00-pm-overview（◇ O3）

- **变更点**：在现有导航骨架上并入三块——(1) §1 增"单线程事件循环 vs 内核 BKL"执行模型声明（K-108）；(2) 新增"工程面"节收编构建与 cfg feature 门面（K-106）；(3) 新增"测试架构"节收编 seam+mock / run_once 集成层 / "测试全绿但行为错误"教训（K-107）；(4) §2.3 阅读路线标注 99 为"先读词汇表"、加入 §4.2 的四分组。
- **讲什么**：K-001、K-106、K-107、K-108 + 导航。
- **不讲什么**：一切机制细节（各机制文档）；测试用例逐条（各篇 §5）。
- **前置**：无。**后置**：全部。**事实底线**：`os/servers/pm/src/lib.rs`（crate 与 feature 声明）、`tests/run_once_integration.rs`、`Cargo.toml`。**验收**：读者能据此判断"我要查某机制/某测试/某 feature 该去哪个文件"。

### 04-ipc-dispatch（◇ O5）

- **变更点**：把 §2/§4 中内联的"union 字段选取/解码细节"移出，改为回指 21；保留分发骨架、`PmCall` 判别、`ReplyIntent`、EXITING 丢弃、reply 载荷复用。
- **前置**：01~03。**后置**：05/06/21/07~20。**验收**：分发一章不含任何按成员读 union 的实现细节（全部下沉 21）。

### 12-signal-handlers（◇ O2）

- **变更点**：现 §8"接线（S3 余件）：check_pending 的生产消费点"归并进 §4 实现详解，恢复 7 章模板。内容不删，只换章位置。
- **验收**：文档章节骨架回到 `概念/C源码/Rust设计/实现/测试/过渡/参见` 七章，与兄弟篇一致。

### 01/05/07/13/17/18/20（◇ O4，交叉引用增量）

- **变更点**：各篇首次触及"从 message 取参数/投递载荷/快照布局"处，加一句指向 21（例：`字段布局与解码见 21-pm-wire-codec.md`）。纯增量引用，不改正文结构。

---

## 6. 缺漏新篇（步骤 3 缺口逐项落实，不留空）

| 缺口 | 落实 | 原料来源 | 验收 |
|------|------|---------|------|
| G-a 线格式与解码 | **新建 21** | C `ipc.h`/`com.h`/`callnr.h`；非 C 制品 `decode.rs`/`wire.rs`/`minix-types/ipc/pm.rs` | 见 §5 契约 21 |
| G-b 构建/feature | 并入 00 工程面节 | Cargo.toml + todo P2-1 | 00 含 feature→行为映射一小表 |
| G-c 测试架构 | 并入 00 测试架构节 | run_once_integration.rs + todo §11.4/§12.5（TSTL/CSL） | 00 说明 seam/mock 端口替身纪律 + 集成层职责 |
| G-d 单线程并发 | 并入 00 §1 一段 | AGENTS.md 执行模型 + init.rs | 00 显式声明 PM !Send/Rc/RefCell 合理，异于内核 |
| G-e 汇编入口/trap | 否决独立成篇（不在本 stage） | 归 14-stage-runtime + edge E1 | 01 §3.2 保留决策注记 + 交叉引用 |
| G-f 加载/内存布局 | 否决（不在本 stage） | 归 03-stage-rs / 02-stage-vm | 00/01 交叉引用 |

> 明确不新建"G12 工程专篇"：构建/测试/并发的体量不足以撑一篇独立文档，且会与 plan.md/todo.md 重复；折叠进 00 更符合"单篇单语义、控制长度"的取向。

---

## 7. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|------|------|--------|--------|------|-----------|------|
| O1 | 新建 | 无 | 21-pm-wire-codec.md | A-4 线格式无归属文档，横切知识散落六篇 | K-101~K-105、K-034 | 新增方向：均带证据锚点 |
| O2 | 拆分（章内归并） | 12 §8 | 12 §4 | 恢复 7 章模板一致性 | K-022 生产消费点 | 存量方向：原样搬移（同篇内） |
| O3 | 合并（并入总览） | plan/todo 散落工程面 | 00 新增 3 节 | K-106/107/108 无文档之家 | K-106、K-107、K-108 | 新增方向：锚点 Cargo.toml/integration.rs/AGENTS.md |
| O4 | 重排（引用增量） | 01/05/07/13/17/18/20 内联字段选取处 | 指向 21 | 建立汇聚点单一权威 | K-101~K-104 | 存量：正文保留，加回指 |
| O5 | 拆分（边界） | 04 内联 decode 细节 | 21 | 单篇单语义（分发不含线格式） | K-101、K-102 | 存量方向：从 04 迁至 21 |

> 无归档、无重编号。所有旧编号文档保留原文件名与原编号。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（仅覆盖发生变化处）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|--------|---------|---------|
| 04 §2/§4 union 字段选取段 | 按成员读 message 的实现细节 | 21 §2 | 拆分迁移 | 低（04 加回指即可） |
| 12 §8 接线 | check_pending 生产消费点 | 12 §4 末 | 章内归并 | 极低（同文件内，无跨文件引用） |
| plan/todo 工程叙述 | 构建/feature/测试架构/单线程 | 00 新增节 | 提炼合并 | 无（plan/todo 保留原文，00 为正式文档归口） |
| 各篇散落线格式知识 | 见 §2.3 | 21 | 汇聚 | 低 |

### 8.2 引用迁移表

| 旧引用 | 出现处（样例） | 新目标 | 验证方式 |
|--------|--------------|--------|---------|
| `04-ipc-dispatch.md §union` | 07/05 若曾指向 | 21-pm-wire-codec.md | `grep -n 'union\|解码\|decode' *.md` 逐处核对指向 21 |
| 各篇"字段布局"提及 | 01/05/07/13/17/18/20 | 追加 `见 21` | 新篇 21 落地后 grep `21-pm-wire-codec` 应有 ≥7 入站引用 |
| 现有 01~20/99 全部文件名引用 | 文档间 + 代码注释（≈800 处文件名引用） | **不变**（零重编号） | 迁移前后 `grep -rnoE '[0-9]{2}-[a-z-]+\.md'` 计数一致 |

### 8.3 断链成本摘要

- **受影响既有引用**：**0**（不重编号、不改任何现有文件名，800+ 文件名引用与代码内引用全部保持有效）。
- **新增引用**：21 的入站引用约 7~10 处（O4 增量）+ 00 内部小节引用。
- **热点文件**：01（被引 64）、04（58）、11（50）、05/02（45）——正因这些是热点，重排它们的代价最高，本蓝图选择完全规避。
- **批量修改方式**：仅新增文件 `21-pm-wire-codec.md` + 对 00/04/12 的定点小改 + 对各篇加一行回指；无需任何 sed 批量改号。

> 结论：把断链成本压到接近零，同时补齐了唯一有实证支撑的覆盖缺口（A-4 线格式）。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：新目录前置字段逐篇核对——21 前置 04/99（均更早/词汇表）；00 无前置；其余不变，现编号已保证依赖前置。**通过**。
2. **依赖关系图无环**：21 依赖 04/99，被 05/07/13/17/20 回指；04→21→各调用族为 DAG（线格式不反向依赖具体服务）。**通过**。
3. **覆盖率**：知识点池 K-001~K-035（存量）全部有归属（映射现文档，未丢）；K-101~K-108（新增）全部落入契约或 00 并入项，逐条带锚点。**100%**。
4. **断链成本**：既有引用受影响 0 处，热点文件不动。**通过**。

### 9.2 自检门逐门结果

| 门 | 结果 |
|----|------|
| G1 C 真序逐条可核对 | **通过**：抽查 S1-S10/L1-L11 共 21 条锚点，随机 10 条（S1/S3/S5/S7/L2/L5/L7/L8/L9/L11）对 `main.c`/`table.c` 核对，行号一致 |
| G2 知识点池完整 | **通过**：15 .c + 6 .h + 47 调用全部有归属；非 C 制品 decode.rs/wire.rs/minix-types-pm.rs/Cargo.toml/integration.rs 逐项有去向或"明确排除加理由"（G-e/G-f） |
| G3 前向引用为零 | **通过**（§9.1-1） |
| G4 依赖图无环 | **通过**（§9.1-2） |
| G5 覆盖率 100% | **通过**：存量全去向、新增全锚点、无删除项（§9.1-3） |
| G6 拆分/合并写清去向、新建写清来源 | **通过**：O1（新增）六知识点全带 C/制品锚点；O2/O5（拆分/迁移）存量随迁 |
| G7 每篇契约七要素 | **通过**：变更篇目（§5）七要素齐全；未变更篇目现文档已具七要素（章节 grep 证实 7 章模板） |
| G8 锚点迁移覆盖变化文档 | **通过**：仅 00/04/12 + 各篇回指发生变化，逐节列出（§8.1/8.2） |
| G9 事实断言带锚点 | **通过**：全部真序/池/缺口断言带 C 或制品锚点；推测项已标注（见下） |

### 9.3 待用户裁决的问题

1. **是否接受"零重编号的保守重排"**：本蓝图判定全量重排成本（≈800 引用 + 兄弟 stage 一致性）大于收益，选择只新建 21 + 原地修订。若你坚持要一次彻底重排编号，需另立专项处理断链，本蓝图不覆盖。
2. **新 21 的接线时机**：A-4 目前仅 Wait4/Ptrace/SrvFork 等少数调用有 wire 成员（多数落 E7 edge）。建议 21 现在即建为"fail-closed 契约篇"（讲现有 wire + 标注 E7 待补），而非等全部 wire 落地——请确认。
3. **工程面/测试架构折叠 vs 独立成篇**：本蓝图把 K-106/107/108 折进 00（体量不足一篇）。若你希望 PM 工程面也单列一篇（对齐某些 stage 的"contribution/testing"专篇惯例），可改判，代价是 00 与新篇需划清边界。

> **推测项声明**：§8.3 的"≈800 引用"为按每篇入站文件名引用的求和估计（grep 实测头部 01=64…逐篇计数见步骤 5 输出），未去重跨 stage 同名文件；用于量级判断，精确值以 B 相开工前的 `grep -rc` 全量清点为准。
