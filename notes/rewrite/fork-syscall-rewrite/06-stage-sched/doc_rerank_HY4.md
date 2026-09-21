# 06-stage-sched 文档重建蓝图（HY4）

## 0. 元数据

### 0.1 执行头部

```text
your_name(AI agent name) = HY4
target_dir(关注的工作目录) = 06-stage-sched
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_HY4.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _HY4 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

### 0.2 范围声明

| 类别 | 清单 |
|------|------|
| 文档（纳入重建） | `00-sched-overview.md`、`01`~`14` 共 14 篇编号正文、`99-global-concepts.md` |
| 参考材料（只作证据，不进新目录） | `plan.md`、`todo.md`、`archive/*.md`、`.design/*`、`draft/*`（draft 是 00/01/99 的旧素材，B 相取料后归档） |
| 范围外 | `.review/codex/sched/*`（review 中间产物）、`os/target/**`（构建产物）、其它 stage 正文 |
| 前序 stage 边界 | `01-stage-kernel/11-scheduling-primitives.md`（内核就绪队列与 `sched_proc` 已是它的主权）、`04-stage-pm/16-scheduling.md`（`nice↔queue` 换算与 `do_getsetpriority` 权限是它的主权）、`03-stage-rs/08-rs-slot-config.md` 与 `03-stage-rs/16-rs-live-update.md`（RS 槽配置与 Live Update 是它的主权） |
| 执行环境 | 分支 `rewrite`，提交 `2d9d1f0aa`，日期 2026-09-19 |

### 0.3 读取清单

**文档**：15 篇编号文档全部头部声明 + 全部章节骨架（`grep '^## \|^### '`），正文按知识点抽读（01/06/09/10/12 精读关键节）。

**C 源码（本 stage 主权，全量读）**：

```text
minix3/minix/servers/sched/main.c        137 行  主入口 + 主循环 + SEF 回调
minix3/minix/servers/sched/schedule.c    369 行  四个处理函数 + pick_cpu + schedule_process + 平衡
minix3/minix/servers/sched/utility.c      74 行  no_sys + 两个端点校验 + 白名单
minix3/minix/servers/sched/schedproc.h    39 行  struct schedproc + IN_USE
minix3/minix/servers/sched/sched.h        18 行
minix3/minix/servers/sched/proto.h        21 行
minix3/minix/servers/sched/Makefile        —     PROG=sched, SRCS=main.c schedule.c utility.c
```

**C 源码（契约对端，按点抽读）**：

```text
minix3/minix/include/minix/com.h:801-807              五种 SCHEDULING_* 消息号
minix3/minix/include/minix/config.h:63-77             队列常量与 USER_QUANTUM / USER_DEFAULT_CPU
minix3/minix/include/minix/ipc.h                      mess_* 五个消息体（272/1101/1112/1437/1444/1827/1912）
minix3/minix/include/minix/priv.h:45-50               SRV_F / USR_F（PREEMPTIBLE 的来源）
minix3/minix/include/minix/const.h:143                PREEMPTIBLE
minix3/minix/include/minix/type.h:122-125             struct machine（processors_count / bsp_id）
minix3/minix/kernel/system.c:642-699                  sched_proc（内核写入）
minix3/minix/kernel/system/do_schedule.c              do_schedule（权限 + 下发）
minix3/minix/kernel/system/do_schedctl.c              do_schedctl（归属切换）
minix3/minix/kernel/proc.c:1592-1740                  enqueue / enqueue_head / dequeue
minix3/minix/kernel/proc.c:1860-1919                  notify_scheduler / proc_no_time / reset_proc_accounting
minix3/minix/kernel/proc.h:30-34,178-179,262          p_priority / p_cpu_time_left / p_scheduler / MF_NICED
minix3/minix/kernel/arch/i386/arch_clock.c:310-330     MF_NICED → CP_NICE 记账分支
minix3/minix/lib/libsys/sched_start.c                 sched_inherit / sched_start（客户端选路）
minix3/minix/lib/libsys/sched_stop.c                  sched_stop（两条短路）
minix3/minix/lib/libsys/sys_schedule.c                sys_schedule 五参数
minix3/minix/lib/libsys/sys_schedctl.c                sys_schedctl 四参数
minix3/minix/lib/libsys/sys_setalarm.c                sys_setalarm2（闹钟）
minix3/minix/servers/pm/schedule.c:20-112             sched_init / sched_start_user / sched_nice
minix3/minix/servers/pm/utility.c:91-103              nice_to_priority 换算
minix3/minix/servers/pm/main.c:241,373                sched_init 调用点 / fork 回复后接管
minix3/minix/servers/pm/forkexit.c:425-441            exit_restart 的 sched_stop
minix3/minix/servers/pm/misc.c:265-290                do_getsetpriority 的权限门
minix3/minix/servers/rs/utility.c:364-382             sched_init_proc（两断言 + 6 参数）
minix3/minix/servers/rs/main.c:320-322,376            r_scheduler/r_priority/r_quantum 与调用点
minix3/minix/servers/rs/request.c:342                 改槽时的取消
minix3/minix/servers/rs/manager.c:461                 清理时的取消
minix3/minix/servers/rs/type.h:92                     r_scheduler 字段
```

**非 C 制品（逐项检查）**：`minix3/minix/servers/sched/Makefile` + `minix.service.mk`（C 构建）、`kernel/table.c` boot_image 登记（镜像布局）、`os/servers/sched/Cargo.toml`（bin `minix-sched` + lib `minix_sched` 双目标）、`os/libs/minix-types/src/types/com.rs:85-103`（消息号权威）、`os/servers/sched/src/` 22 文件 4525 行（Rust 实现）、`cargo test -p minix-sched` 基线（todo.md §0：81 passed）、`tools/design-coverage-check.sh --stage 06-stage-sched`。

**边界材料**：`00-master-plan/README.md:24,40,53`、`edge_todo.md`、`06-stage-sched/plan.md`、`06-stage-sched/todo.md`、`05-stage-vfs/00-vfs-overview.md`（上一个 stage 的 overview 写法，作为 00 篇的模板参照）。

### 0.4 关键命令与证据摘录

| 命令 | 关键输出 | 用途 |
|------|---------|------|
| `wc -l 0*.md 1*.md 9*.md` | 00=20、99=20、01~14 各 191~273 行、合计 3286 行 | 00/99 是空骨架，正文篇高度等长（模板化） |
| `grep -rn "SUSPEND" minix3/minix/servers/sched/` | 仅 `main.c:90` 一处命中 | SCHED 从不产生 SUSPEND，判回复条件是死分支 |
| `grep -rn "pick(" os/servers/sched/src` | 仅 `server.rs:378,389` 两处（均在 `do_start` 内） | Rust 未复现 C `schedule_process:302` 的每次重选 |
| `grep -rn "06-stage-sched" --include=*.md notes/` | 命中 `07-stage-ds/*`、`edge3.md`；无一篇引用 01~14 篇名 | 外部断链成本极低 |
| `for f in *.md; grep -o "[0-9][0-9]-[a-z0-9-]*\.md" \| wc -l` | 02=18、05=23、06=19、07=17、14=17、01/03=15/16 … 合计约 222 | 内部交叉引用热点 |
| `ls 03-stage-rs/08-*.md 03-stage-rs/16-*.md 04-stage-pm/16-*.md` | 三个文件均存在 | 旧 13/14 的出站引用零断链，B 相必须保持 |
| `git rev-parse --short HEAD` | `2d9d1f0aa` | 元数据 |

---

## 1. C 真序

### 1.1 阶段类型判定

**服务事件循环型**（`main()` → 一次装填 → 无限 `sef_receive_status` → 分发 → 回复），另有两条不经过 SCHED 主动轮询的入口：**内核推送**（时间片耗尽通知）与**闹钟推送**（CLOCK 通知）。因此真序分三段：**启动段**（1.2）、**循环段**（1.3）、**请求生命周期**（1.4、1.5），外加**闹钟支线**（1.6）。

判据：`main.c:35` `while (TRUE)` + `main.c:39` `sef_receive_status(ANY, ...)`；没有任何 SCHED 自设的定时器线程，周期性工作由 `sys_setalarm` 借 CLOCK 通知驱动（`schedule.c:340,367`）。

### 1.2 启动段真序（S0~S8）

| # | 动作 | 锚点 | 说明 |
|---|------|------|------|
| S0 | RS 从 boot_image 登记 SCHED 槽位并装载镜像 | `kernel/table.c` boot_image；`00-master-plan/README.md:24`（06 = SCHED，由 RS 加载） | SCHED 不是内核任务，靠 RS 运行时加载 |
| S1 | RS 为 SCHED 配调度参数后调 `sched_init_proc` | `rs/main.c:320-322`（`r_scheduler`/`r_priority`/`r_quantum`）、`rs/main.c:376` | 系统进程"申请先行"：生下来就有调度者 |
| S2 | `main()` 入口，调 `sef_local_startup()` | `main.c:22,32` | 二进制第一件事是 SEF 启动 |
| S3 | 登记两个初始化回调 + `sef_startup()` | `main.c:111-121` | fresh 有 SCHED 代码，restart 由 libsef 通用实现 |
| S4 | `sef_cb_init_fresh`：`sys_getmachine(&machine)`，失败 panic | `main.c:126-131`；`type.h:123-124` | 拿到 `processors_count` 与 `bsp_id` |
| S5 | `init_scheduling()`：`balance_timeout = 5 * sys_hz()` | `schedule.c:18,334-338` | 每秒 tick 数由内核给出，不写死 |
| S6 | `sys_setalarm(balance_timeout, 0)`，失败 panic | `schedule.c:340-341` | 闹钟在此装一次，之后每轮重发 |
| S7 | 进入 `while (TRUE)` | `main.c:35` | 启动段结束 |
| S8 | PM 侧 `sched_init()` 把 INIT 交给 SCHED | `pm/main.c:241`、`pm/schedule.c:20-50` | 用户态第一个被接管的进程（跨进程，与 S0~S7 并行） |

### 1.3 循环段真序（L1~L12）

| # | 动作 | 锚点 | 说明 |
|---|------|------|------|
| L1 | `sef_receive_status(ANY, &m_in, &ipc_status)`，失败 panic | `main.c:39-40` | 同时拿消息体与 IPC 状态（来源 + 标志） |
| L2 | 取 `who_e = m_in.m_source`、`call_nr = m_in.m_type` | `main.c:41-42` | |
| L3 | 先判 `is_ipc_notify(ipc_status)` | `main.c:45` | 通知优先于调用 |
| L4 | 通知且来源 `CLOCK` → `balance_queues()` | `main.c:47-49` | 其它来源的通知静默丢弃 |
| L5 | 通知路径 `continue`，**不回复** | `main.c:54` | 通知没有回复语义 |
| L6 | 调用号 `SCHEDULING_INHERIT` / `SCHEDULING_START` → `do_start_scheduling` | `main.c:58-61` | 两种消息共一个入口 |
| L7 | `SCHEDULING_STOP` → `do_stop_scheduling` | `main.c:62-64` | |
| L8 | `SCHEDULING_SET_NICE` → `do_nice` | `main.c:65-67` | |
| L9 | `SCHEDULING_NO_QUANTUM`：先查 `IPC_FLG_MSG_FROM_KERNEL` | `main.c:68-71` | 非内核来源 → 打日志 + `EPERM` |
| L10 | 内核来源 → `do_noquantum`，失败只打 Warning，`continue` 不回复 | `main.c:72-77` | 唯一"处理失败也不回复"的调用 |
| L11 | 其它调用号 → `no_sys` → `ENOSYS` | `main.c:85-86`、`utility.c:18-23` | |
| L12 | `result != SUSPEND` 才 `reply(who_e, &m_in)`；发送失败只打印 | `main.c:90-93`、`main.c:101-106` | **SCHED 四个处理函数没有一处返回 SUSPEND**（见 3.5 F-1） |

### 1.4 一次 START 请求的完整生命周期（RS 系统进程路径）

| # | 位置 | 动作 | 锚点 |
|---|------|------|------|
| 1 | RS | 组装 6 参数（调度器/被调度者/父亲=RS/队列/时间片/CPU），带两断言 | `rs/utility.c:364-377` |
| 2 | libsys | `sched_start`：`scheduler_e == NONE` 直接 OK；`== KERNEL` 走 `sys_schedctl`；否则发 `SCHEDULING_START` | `sched_start.c:57-88` |
| 3 | SCHED | 类型断言 → 白名单 → 空槽校验 → 填槽 → 边界检查 | `schedule.c:146-166` |
| 4 | SCHED | init 自父亲特例（临时 `USER_Q`/200/BSP）→ START 分支覆盖为显式值 | `schedule.c:171-197` |
| 5 | SCHED | `sys_schedctl(0, ep, 0,0,0)` 接管：内核把 `p_scheduler` 指向 SCHED | `schedule.c:218` → `do_schedctl.c:40-43` |
| 6 | SCHED | 置 `IN_USE`（**在接力成功之后**） | `schedule.c:223` |
| 7 | SCHED | `pick_cpu(rmp)` → `schedule_process(ALL)`；`EBADCPU` 则标死重选重试 | `schedule.c:226-231` |
| 8 | SCHED→kernel | `schedule_process` 内部**再次** `pick_cpu`，按掩码挑三个字段（未选填 -1），算 `niced` | `schedule.c:302-319` |
| 9 | kernel | `do_schedule` 校验 `caller == p_scheduler`，否则 EPERM | `do_schedule.c:20-21` |
| 10 | kernel | `sched_proc`：范围检查 → 可运行则置 `RTS_NO_QUANTUM` 出队 → 写优先级/时间片/CPU/MF_NICED → 清位入队 | `system.c:644-699` |
| 11 | SCHED | 回复里写 `scheduler = SCHED_PROC_NR` | `schedule.c:246` |
| 12 | libsys/RS | 把回复里的调度器写回 `r_scheduler`（可能不是请求对象） | `sched_start.c:96` |

PM 用户进程路径的差异：第 1 步换成 `pm/schedule.c:55-84` 的 `sched_start_user`（先 `nice_to_priority` 再 `sched_inherit`，父亲是 `PRIV_PROC` 时改为继承 INIT），第 4 步走 `SCHEDULING_INHERIT` 分支（抄父进程"当前位置"而非"上限"，`schedule.c:199-209`），第 1 步时机在 fork 回执之后（`pm/main.c:373`）。

### 1.5 一次 NO_QUANTUM 的闭环（时间片耗尽）

| # | 位置 | 动作 | 锚点 |
|---|------|------|------|
| 1 | kernel 时钟 | `p_cpu_time_left` 归零 → `proc_no_time(p)` | `proc.c:421-422`（调用点）、`proc.c:1893` |
| 2 | kernel | 非内核调度 **且** 带 `PREEMPTIBLE` → `notify_scheduler`；否则只续时间片 | `proc.c:1895-1909`；`priv.h:45-50` |
| 3 | kernel | 置 `RTS_NO_QUANTUM` 出队；填 `m_source = p->p_endpoint`（**内核代填被调度者**）与 7 个统计字段 | `proc.c:1868,1874-1882` |
| 4 | kernel | `reset_proc_accounting(p)` 后 `mini_send(p, p_scheduler->p_endpoint, &m, FROM_KERNEL)`，失败 panic | `proc.c:1885-1890` |
| 5 | SCHED | 主循环判 `IPC_FLG_MSG_FROM_KERNEL` 通过 | `main.c:70-71` |
| 6 | SCHED | `do_noquantum`：`sched_isokendpt(m_source)` → `priority < MIN_USER_Q` 则 +1 → 局部下发 | `schedule.c:92-105` |
| 7 | SCHED | `schedule_process_local` = 只带优先级与时间片（不动 CPU） | `schedule.c:32-33` |
| 8 | kernel | `sched_proc` 写入并重新入队，进程继续运行 | `system.c:680-699` |
| 9 | — | **不回复**（`main.c:76` 的 `continue`） | |

### 1.6 闹钟支线真序（A1~A3）

| # | 动作 | 锚点 |
|---|------|------|
| A1 | 启动时 `sys_setalarm(balance_timeout, 0)` | `schedule.c:340` |
| A2 | 闹钟到期 → CLOCK 通知 → `balance_queues()`：扫全表，凡 `priority > max_priority` 者 -1 并局部下发 | `main.c:47-49`、`schedule.c:353-365` |
| A3 | 再次 `sys_setalarm`，形成自持循环；失败 panic | `schedule.c:367-368` |

### 1.7 序差表（教学序 ≠ 运行时序，逐条备案）

| 运行时事实（锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---|---|---|
| `sys_schedctl` 在 `do_start_scheduling` 内部被调用（`schedule.c:218`），在"登记"之后 | 新 07（接管）置于新 11（登记）**之前** | 登记的三件事里"接力"最需要前置知识；先讲契约，再讲调用它的人 | 新 11 §交接一步只留锚点 |
| `pick_cpu` 由 `schedule_process` 内部调用（`schedule.c:302`），是下发的一环 | 新 10（选核）置于新 08（下发）之后、新 11 之前 | 选核的输入（机器信息、负载台账）与输出（CPU 字段）都要先有下发框架才讲得清 | 新 08 §"CPU 字段"一句下放 |
| `notify_scheduler`（`proc.c:1860`）先于任何 SCHED 代码发生 | 新 09（回传）紧随新 08，仍在处理函数之前 | 内核契约三篇成组，读者一次读完"下发/接管/回传"三个方向 | 新 12（NO_QUANTUM 处理）只引用 |
| `init_scheduling` 在启动段执行（`main.c:133`） | 新 14（定时与平衡）放在处理函数之后 | 闹钟机制要用到"局部下发"与"上限"两个已讲概念 | 新 02 §装闹钟只给动作 + 锚点 |
| `do_nice` 与 `do_noquantum` 在 C 里是两个函数，触发者不同（PM / 内核） | 合为新 12 | 两者都是"改优先级并局部下发"，一个是改当前位置、一个是改上限，配对讲最省 | 新 12 内分两节，各自保留触发者 |
| RS 的申请（S1）发生在 SCHED 启动之前 | 新 17 放在客户端之后 | 申请面要先用新 15 的 libsys 三路选路 | 新 01 §启动因果链预告 |
| `balance_queues` 由通知触发，与请求处理同级 | 新 14 独立成篇 | 它是唯一"SCHED 主动"的机制，属于支线 | 主线路径标注可跳读 |

---

## 2. 知识点全集（存量池 + 新增）

来源类型：**存** = 现有文档已讲；**新** = 现有文档没讲但 C / 制品 / 理论承载。

### A 组：定位与模型

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-001 | 双层调度模型（内核执行面 / 用户策略面） | 概念 | 存 | 00 核心点、01 §1.1、99 | `main.c:1-3`；`do_schedule.c:20` | 回答"为什么调度器跑在用户态" |
| K-002 | 五进程表分工（proc/mproc/vmproc/fproc/schedproc） | 概念 | 存 | 03 §1.1、99 | `kernel/proc.h`；`schedproc.h:23` | 回答"调度记录为什么单独一张表" |
| K-003 | 与 Kernel/PM/RS 的职责边界 | 概念 | 存 | 00 边界、12/13/14 | `main.c:44-87`；`pm/schedule.c`；`rs/utility.c:364` | 回答"一件事该找谁" |
| K-004 | 策略可替换性（改策略只重编 SCHED） | 概念 | 存 | 01 §1.1 | `schedule.c:348-352` 注释 | 回答"为什么值得多一次 IPC" |
| K-005 | SCHED 在启动因果链中的位置（RS 加载 → boot_image → sef → 循环） | 机制 | 新 | 00（一句话） | `kernel/table.c` boot_image；`00-master-plan/README.md:24,40`；`rs/main.c:376` | 回答"SCHED 什么时候出现在系统里" |
| K-006 | 单线程事件循环执行模型（用户态服务器共守） | 约束 | 存 | 各篇 §3 散见 | AGENTS.md 执行模型条目 | 回答"能否用锁/线程" |
| K-007 | 调度者与被调度者的分离（`p_scheduler` 指向谁） | 概念 | 存 | 12 §1.2 | `proc.h:34`、`do_schedctl.c:42` | 回答"谁来收耗尽通知" |
| K-008 | 多调度器转派的预留口（回复可写他人端点） | 架构演进 | 存 | 06 §1.6、`sched_start.c:34-39` | `schedule.c:239-248` | 回答"为什么回复要带 scheduler 字段" |

### B 组：启动与主循环

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-010 | SEF 两回调注册（fresh / restart stateful） | 机制 | 存 | 01 §1.2、§2.7 | `main.c:111-121` | 回答"服务重启时 SCHED 做了什么"（答：什么都没做） |
| K-011 | restart 由 libsef 通用实现，SCHED 无对应代码 | 约束 | 存 | 01 §1.2 | `main.c:115` | 回答"为什么只报一个名字" |
| K-012 | `sys_getmachine` 读机器信息，失败 panic | 机制 | 存 | 01 §1.3、§2.8 | `main.c:130-131`；`type.h:123-124` | 回答"CPU 个数从哪来" |
| K-013 | `init_scheduling` 装平衡闹钟（5 × sys_hz） | 机制 | 存 | 01 §1.3、11 §2.2 | `schedule.c:334-343` | 回答"5 秒是谁的 5 秒" |
| K-014 | 主循环四步（收 / 分类 / 分发 / 回复） | 机制 | 存 | 01 §1.4、02 §1.1 | `main.c:35-96` | 回答"主循环骨架长什么样" |
| K-015 | `sef_receive_status` 与 `ipc_status`（来源 + 标志两位信息） | 接口 | 存 | 01 §2.2、02 §2.1 | `main.c:39` | 回答"除了消息体还收到了什么" |
| K-016 | 通知先于调用、通知一律不回复 | 约束 | 存 | 02 §1.3、§1.5 | `main.c:44-55` | 回答"通知和调用怎么分开" |
| K-017 | SUSPEND 判据是死分支（SCHED 从不返回 SUSPEND） | 事实 | **新** | 01 §1.5（只讲约定，未讲它永不产生） | `main.c:90`；`grep SUSPEND servers/sched/` 仅此一处 | 回答"这条判据有没有用" |
| K-018 | 回复失败只打印不 panic | 约束 | 存 | 01 §2.6 | `main.c:101-106` | 回答"发不出去会怎样" |
| K-019 | `no_sys` → ENOSYS | 接口 | 存 | 02 §2.6 | `utility.c:18-23` | 回答"未知调用号怎么回" |
| K-020 | 启动失败的 panic 纪律（机器信息、装闹钟两处） | 约束 | 存 | 01 §D2 | `main.c:131`、`schedule.c:341` | 回答"半知半解能不能进循环" |

### C 组：消息面

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-021 | 五种消息与编号（0xF01~0xF05） | 接口 | 存 | 02 §1.2、99、00 | `com.h:801-807`；`minix-types/src/types/com.rs:85-103` | 回答"SCHED 收哪几种消息" |
| K-022 | 五个消息体的字段与线格式 | 接口 | 存 | 02 §2.7、06/07/08 分散 | `ipc.h:272,1101,1112,1437,1444,1827,1912` | 回答"每个消息带什么" |
| K-023 | START 与 INHERIT 共入口 | 机制 | 存 | 02 §2.3、06 §1.1 | `main.c:58-61`、`schedule.c:146-147` | 回答"为什么两种消息一个函数" |
| K-024 | NO_QUANTUM 必须来自内核，否则 EPERM | 约束 | 存 | 02 §1.4、§2.4 | `main.c:68-84` | 回答"为什么要验来源" |
| K-025 | 内核代填 `m_source` = 被调度者端点 | 机制 | **新**（12 §2.3 只讲构造） | 12 §2.3 | `proc.c:1874`；`schedule.c:92` | 回答"SCHED 凭什么用 m_source 查表" |
| K-026 | 通知 vs 调用的三态分类 | 机制 | 存 | 02 §D2 | `main.c:45` | 回答"主循环怎么分流" |
| K-027 | 回复规则（回 / 不回 / 通知不回）三分 | 约束 | 存 | 02 §1.5、§D4 | `main.c:54,76,90` | 回答"什么时候不该回" |
| K-028 | 消息体在 minix-types 的权威落点 | 架构演进 | 存 | 02 §D6 | `minix-types/src/ipc/*` | 回答"线格式单一事实源在哪" |
| K-029 | 消息号常量不得本地旁路（P2-3 教训） | 约束 | 存 | todo.md §1 | `todo.md §1`；`07-stage-ds/.design/99-design.v1.md` | 回答"常量写在哪" |
| K-030 | 通知的发送者是 CLOCK，来源是内核代发的端点 | 机制 | 存 | 02 §2.2、11 §2.5 | `main.c:47`、`sys_setalarm.c` | 回答"谁敲的门" |

### D 组：记录与表

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-031 | `schedproc` 七字段 | 数据结构 | 存 | 03 全篇 | `schedproc.h:23-36` | 回答"调度器眼里的进程长什么样" |
| K-032 | `cpu_mask` 从未读写 + Rust 结构消除（ARCH S-3） | 架构演进 | 存 | 03 §1.6、§2.6 | `schedproc.h:33`；`schedule.c:185` FIXME | 回答"第八个字段去哪了" |
| K-033 | `IN_USE` 是唯一标志位 | 数据结构 | 存 | 03 §1.3 | `schedproc.h:39` | 回答"槽位占用怎么表示" |
| K-034 | 槽位号 = 进程号（`_ENDPOINT_P`） | 机制 | 存 | 04 §1.2 | `utility.c:31` | 回答"端点到槽位怎么换算" |
| K-035 | `sched_isokendpt` 四道判断 | 机制 | 存 | 04 §1.3、§2.1 | `utility.c:29-41` | 回答"在册进程怎么验" |
| K-036 | `sched_isemtyendpt` 三道镜像判断 | 机制 | 存 | 04 §1.4、§2.2 | `utility.c:46-56` | 回答"空槽怎么验" |
| K-037 | 白名单只放行 PM 与 RS | 约束 | 存 | 04 §1.5、§2.3 | `utility.c:61-74` | 回答"谁能向 SCHED 提申请" |
| K-038 | 错误码总表（EBADEPT/EDEADEPT/EINVAL/EPERM/ENOSYS/EBADCPU） | 接口 | 存（分散） | 04 §2.4 + 06/07/09 各一处 | `utility.c:33,35,37,39`；`errno` | 回答"每个门拒绝时回什么" |
| K-039 | 表长 `NR_PROCS` 与静态表 | 数据结构 | 存 | 03 §2.7 | `schedproc.h:36` | 回答"表有多大" |
| K-040 | `Priority` / `SlotState` 新类型（ARCH S-2） | 架构演进 | 存 | 03 §3、§4 | `schedproc.rs:27,43` | 回答"为什么不能是裸 u8" |

### E 组：参数模型

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-041 | 16 队列常量与取值范围 | 常量 | 存 | 05 §1.2 | `config.h:66-72` | 回答"优先级有几个档" |
| K-042 | 数值越小优先级越高（反直觉） | 概念 | 存 | 05 §1.2 | `config.h:63-65` | 回答"7 是高还是低" |
| K-043 | 时间片单位是毫秒（200 默认值） | 常量 | 存 | 05 §1.4 | `schedule.c:41`；`config.h:74` | 回答"200 是什么单位" |
| K-044 | `nice → 队列` 换算（41→16 缩放 + 钳位） | 机制 | 存 | 05 §1.5、13 §2 | `pm/utility.c:91-103` | 回答"nice -20..20 怎么落到 0..15" |
| K-045 | `niced = max_priority > USER_Q` | 机制 | 存 | 05 §1.5、09 §1.4 | `schedule.c:319` | 回答"niced 位怎么算" |
| K-046 | `MF_NICED` 影响内核记账分类（CP_NICE） | 机制 | 存 | 05 §2.5 | `proc.h:262`；`arch_clock.c:318`；`system.c:691-694` | 回答"niced 最终影响了什么" |
| K-047 | 系统进程判定 `parent == RS_PROC_NR` | 机制 | 存 | 05 §1.6、10 §1.2 | `schedule.c:44`；`com.h:61` | 回答"谁是系统进程" |
| K-048 | `USER_DEFAULT_CPU = -1`（不改 CPU） | 常量 | 存 | 05 §1.4 | `config.h:77` | 回答"-1 和 KEEP 的 -1 有什么区别" |
| K-049 | 上限 / 当前位置两字段的分工 | 概念 | 存 | 05 §1.3 | `schedproc.h:29-30` | 回答"为什么优先级要两个字段" |
| K-050 | `Nice` / `CpuChoice` 新类型（ARCH S-4/S-5） | 架构演进 | 存 | 05 §3 | `priority.rs:85,127` | 回答"哨兵值怎么进类型" |

### F 组：内核契约

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-051 | `sys_schedctl(flags=0)` → 调用者成为调度者（接管） | 机制 | 存 | 12 §2.2、06 §2.6 | `do_schedctl.c:40-43`；`libsys/sys_schedctl.c` | 回答"SCHED 怎么接管一个进程" |
| K-052 | `SCHEDCTL_FLAG_KERNEL=1` → 内核自任调度者，`p_scheduler=NULL` | 机制 | 存 | 12 §2.2 | `do_schedctl.c:28-39`；`sched_start.c:70-77` | 回答"交给内核走哪条路" |
| K-053 | flags 白名单校验（多余位 → EINVAL） | 约束 | 存 | 12 §2.1 | `do_schedctl.c:17-21` | 回答"标志位能不能乱填" |
| K-054 | `proc_kernel_scheduler` 谓词（NULL 或自身） | 机制 | 存 | 12 §2.6 | `proc.h:178-179` | 回答"内核调度怎么判定" |
| K-055 | `sys_schedule` 五参数 | 接口 | 存 | 09 §2.3 | `libsys/sys_schedule.c`；`ipc.h:1112` | 回答"下发一次带什么" |
| K-056 | `do_schedule` 权限门（caller == p_scheduler） | 约束 | 存 | 09 §2.4 | `do_schedule.c:20-21` | 回答"谁能改别人的参数" |
| K-057 | `sched_proc` 三道范围检查 | 机制 | 存 | 09 §1.5、§2.5 | `system.c:644-656` | 回答"参数越界在哪被拒" |
| K-058 | `KEEP = -1` 保持不变语义 | 接口 | 存 | 09 §1.3 | `schedule.c:304-317`；`system.c:680-688` | 回答"不带字段怎么表达" |
| K-059 | 三种变更掩码（LOCAL / MIGRATE / ALL） | 机制 | 存 | 09 §1.2、08/11 引用 | `schedule.c:22-35` | 回答"为什么有四个形状" |
| K-060 | 内核写入四步 + RTS_NO_QUANTUM 出队入队 | 机制 | 存 | 09 §2.5、12 §1.4 | `system.c:667-699` | 回答"改优先级为什么要先出队" |
| K-061 | `EBADCPU` 与 `cpu_is_ready` | 约束 | 存 | 09 §2.5、10 §1.4 | `system.c:654-655` | 回答"什么情况算 CPU 不可用" |
| K-062 | `proc_no_time` 两分支（PREEMPTIBLE → 通知；否则续时间片） | 机制 | 存 | 12 §1.6 | `proc.c:1893-1910`；`priv.h:45-50` | 回答"谁会被通知" |
| K-063 | 七统计字段与 `reset_proc_accounting` | 接口 | 存 | 12 §1.5 | `proc.c:1876-1886`；`proc.h:50-55` | 回答"内核多发了什么" |
| K-064 | **SCHED 侧只消费 m_source，六个统计字段无读者** | 事实 | 存（12 §1.5 已点明） | 12 §1.5 | `schedule.c:87-107` | 回答"为什么全发却不用" |
| K-065 | `mini_send(..., FROM_KERNEL)` 失败 panic | 机制 | 存 | 12 §2.3 | `proc.c:1887-1890` | 回答"通知发不出去会怎样" |
| K-066 | 内核侧两处 FIXME（抢占语义 / SMP 跨核） | 已知缺陷 | **新** | 无 | `system.c:663-666` | 回答"这段内核代码有什么保留意见" |
| K-067 | `sched_proc` 里 `RTS_SET` 重复两次（C 冗余） | 已知缺陷 | **新** | 无 | `system.c:667-678` | 回答"为什么同一件事写两遍" |
| K-068 | `do_update` 迁移 `p_scheduler`（Live Update 的调度面） | 机制 | **新** | 14（只取取消两处） | `do_update.c:252` | 回答"换版本时调度者归谁" |

### G 组：放置（SMP）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-070 | `pick_cpu` 三规则（单核→BSP / 系统进程→BSP / 最小负载非 BSP） | 机制 | 存 | 10 §1.2、§2.2 | `schedule.c:48-78` | 回答"进程落在哪个核" |
| K-071 | `cpu_proc[]` 负载台账（选 +1 / 停 -1） | 数据结构 | 存 | 10 §1.3 | `schedule.c:46,77,130` | 回答"负载记在哪" |
| K-072 | `CPU_DEAD = -1` 与无符号数组（`cpu_is_available` 恒真） | 机制 | 存 | 10 §1.4 | `schedule.c:37-39`；`schedule.c:46` 类型 | 回答"死亡标记到底怎么生效" |
| K-073 | 死核过滤实际发生在负载比较里（UINT_MAX 永远落选） | 机制 | 存 | 10 §1.4 | `schedule.c:71-74` | 回答"那道真门在哪" |
| K-074 | **`schedule_process` 入口无条件再调 `pick_cpu`（覆盖外层选择 + 再 +1）** | 机制 | 部分存（06 §1.3 讲覆盖，09 §1.1 讲顺序） | 06 §1.3、09 §1.1 | `schedule.c:302`、`schedule.c:226`、`schedule.c:230` | 回答"为什么 init 的 BSP 没了" |
| K-075 | 台账 ++/-- 不对称（每次下发都 ++，只有 stop --） | 已知缺陷 | **新** | 10 §1.3（只说收支） | `schedule.c:77` vs `schedule.c:130` | 回答"这个计数是真实负载吗"（答：不是） |
| K-076 | EBADCPU 重试环的**有界性存疑**（UINT_MAX 回绕 0 会清除死亡标记） | 已知缺陷 | **新**（06 §1.5 断言必定退出） | 06 §1.5 | `schedule.c:227-231` + `schedule.c:77` | 回答"这个循环会不会出不来" |
| K-077 | `CONFIG_SMP` 两套实现（非 SMP 写死 cpu=0） | 约束 | 存 | 10 §1.5、§2.3 | `schedule.c:78-80`；`schedproc.h:14-16` | 回答"单核和多核代码差别在哪" |
| K-078 | Rust 只在 `do_start` 调 `pick`，未复现每次下发重选 | 架构演进/差异 | **新** | 无 | `server.rs:378,389` vs `schedule.c:302` | 回答"Rust 与 C 在这里行为一致吗" |
| K-079 | `cpu_load()` 由内核回传但 SCHED 未消费 | 事实 | 存（12 §1.5） | 12 §1.5 | `proc.c:1882` | 回答"负载数据有没有闭环" |

### H 组：请求处理

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-080 | 登记的检查顺序（类型 → 白名单 → 空槽 → 边界） | 机制 | 存 | 06 §1.2 | `schedule.c:146-166` | 回答"先查什么后查什么" |
| K-081 | init 自父亲特例与临时值被无条件覆盖 | 机制 | 存 | 06 §1.3、§2.3 | `schedule.c:171-188` + `schedule.c:226` | 回答"那段临时值最后去哪了" |
| K-082 | START 显式给值（当前位置 = 上限，时间片 = 消息值） | 机制 | 存 | 06 §2.4 | `schedule.c:191-197` | 回答"系统进程为什么不继承" |
| K-083 | INHERIT 抄父进程当前位置与时间片 | 机制 | 存 | 06 §2.5 | `schedule.c:199-209` | 回答"fork 出来的孩子从哪起步" |
| K-084 | 交接与置位的次序（先 `sys_schedctl` 后 `IN_USE`） | 约束 | 存 | 06 §2.6 | `schedule.c:218-223` | 回答"失败了槽位脏不脏" |
| K-085 | `max_priority >= NR_SCHED_QUEUES` → EINVAL（双向拒绝） | 约束 | 存 | 06 §D3、08 §D4 | `schedule.c:164-166`、`schedule.c:273-275` | 回答"越界在哪被挡" |
| K-086 | 降级规则（`priority < MIN_USER_Q` 则 +1） | 机制 | 存 | 08 §1.3、§2.1 | `schedule.c:99-101` | 回答"用完时间片会怎样" |
| K-087 | 信任不对称（耗尽路径不查白名单，只验内核标志） | 约束 | 存 | 08 §1.2 | `main.c:70-71` vs `schedule.c:118,150,262` | 回答"为什么这条路径特殊" |
| K-088 | 改上限三步骤：边界 → 快照 → 下发 → 失败回滚两字段 | 机制 | 存 | 08 §1.4、§2.2-2.3 | `schedule.c:265-291` | 回答"下发失败怎么收场" |
| K-089 | 停止释放两样东西（减负载 + 清 IN_USE），不清 endpoint | 机制 | 存 | 07 §1.3、§2.3-2.4 | `schedule.c:129-134` | 回答"槽位怎么变回空" |
| K-090 | 停止的对称表（申请/取消配对） | 概念 | 存 | 07 §1.5 | `schedule.c:112-135` | 回答"生了怎么死" |
| K-091 | 重试环的类型化（Rust `EBADCPU` → `mark_dead` → 重选） | 架构演进 | 存 | 06 §D6、10 §2.5 | `server.rs:378-391`；`cpu.rs:101` | 回答"Rust 怎么保证有界" |

### I 组：定时与平衡

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-095 | 超时三个数（5 秒 × sys_hz） | 常量 | 存 | 11 §2.1 | `schedule.c:16-18,338` | 回答"5 是哪来的" |
| K-096 | 装闹钟—响铃—再装闹钟的自持循环 | 机制 | 存 | 11 §1.3、§2.2-2.3 | `schedule.c:340,367` | 回答"循环靠什么续命" |
| K-097 | 恢复规则（`priority > max_priority` 则 -1） | 机制 | 存 | 11 §2.3 | `schedule.c:358-365` | 回答"降级了怎么升回来" |
| K-098 | 降级与恢复的不对称（用尽一次一步，每 5 秒一步） | 概念 | 存 | 11 §1.2 | `schedule.c:99` vs `schedule.c:360` | 回答"为什么恢复这么慢" |
| K-099 | 恢复只走局部下发（不动 CPU） | 机制 | 存 | 11 §1.4 | `schedule.c:32-33,362` | 回答"恢复会不会搬家" |
| K-100 | 默认策略可换点（注释：policy will soon be changed） | 架构演进 | 存 | 11 §1.5 | `schedule.c:348-352` | 回答"将来改策略动哪里" |
| K-101 | `Balancer` 值对象 + `rebalance_one` 纯函数 | 架构演进 | 存 | 11 §3 | `balancer.rs:23,38,73` | 回答"Rust 怎么把策略封起来" |

### J 组：客户端与对端

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-105 | `sched_start` 三路选路（NONE / KERNEL / 用户态） | 机制 | 存 | 13 §1.2、§2.2 | `sched_start.c:57-77` | 回答"哪些路不用发消息" |
| K-106 | `sched_inherit` 四参数与"回复照收" | 机制 | 存 | 13 §1.3-1.4 | `sched_start.c:11-41` | 回答"继承带什么、回什么" |
| K-107 | `sched_stop` 两条短路 | 机制 | 存 | 13 §1.2、07 §2.6 | `sched_stop.c:16-17` | 回答"内核当调度者时要不要发" |
| K-108 | 客户端三个包（StartPack / InheritPack / StopPack） | 架构演进 | 存 | 13 §D2 | `client.rs:118,151,178` | 回答"Rust 怎么表达两种申请" |
| K-109 | PM `sched_init` 只接管 INIT（`IN_USE && !PRIV_PROC`） | 机制 | 存 | 13 §1.6、§2.4 | `pm/schedule.c:20-50` | 回答"第一个用户进程怎么进调度" |
| K-110 | PM fork 后 `sched_start_user`（父是 PRIV_PROC 则继承 INIT） | 机制 | 存 | 13 §1.3、§2.4 | `pm/schedule.c:55-84`；`pm/main.c:373` | 回答"系统进程的子进程找谁继承" |
| K-111 | PM `sched_nice`（KERNEL/NONE → EINVAL）与 `do_getsetpriority` 权限 | 机制 | 存 | 13 §2.4、§1.5 | `pm/schedule.c:89-112`；`pm/misc.c:265-290` | 回答"谁能改 nice" |
| K-112 | PM exit 的 `sched_stop` + `mp_scheduler = NONE` 时序 | 机制 | 存 | 13 §1.5、§2.5 | `pm/forkexit.c:425-441` | 回答"进程退出怎么销账" |
| K-113 | RS 两断言（用户无主 / 系统有主） | 约束 | 存 | 14 §1.2 | `rs/utility.c:370-372` | 回答"RS 凭什么申请" |
| K-114 | RS 六参数直达、父亲恒为 RS | 机制 | 存 | 14 §1.3、§1.5 | `rs/utility.c:375-377` | 回答"RS 和 PM 的申请差在哪" |
| K-115 | RS 取消两处（清理打警告继续 / 改槽带码返回） | 机制 | 存 | 14 §1.4 | `manager.c:461`；`request.c:342` | 回答"同样的错为什么两种处理" |
| K-116 | RS 槽配置来源（`SRV_OR_USR`）归 03-stage-rs | 边界 | 存 | 14 分工声明 | `rs/main.c:320-322` | 回答"r_* 四个值哪来的"（答：不在本 stage） |
| K-117 | Live Update 全流程归 03-stage-rs/16 | 边界 | 存 | 14 分工声明 | `03-stage-rs/16-rs-live-update.md` | 回答"换版本时调度怎么迁"（答：不在本 stage） |

### K 组：工程与实现

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-120 | crate 分层与 13 模块地图 | 架构 | 存（分散在各篇 §4.1） | 各篇 §4.1 | `lib.rs:8-20` | 回答"代码按什么分层" |
| K-121 | 双 seam（`IpcTransport` / `KernelApi` 两个 trait） | 架构 | 存 | 02 §D9 | `kernel_api/transport.rs:50,127` | 回答"消息线和内核线怎么分开接" |
| K-122 | `SchedServer` 单一所有者（ARCH S-11） | 架构 | 存 | 02 §D8 | `server.rs:79` | 回答"四个全局去哪了" |
| K-123 | `run_once` / `Step` 与 `run` 的失败上界（对 C panic 的偏离） | 架构演进 | 存 | 02 §D7、01 §D4 | `server.rs:138,161` | 回答"Rust 为什么不再 panic" |
| K-124 | `FreshReady` 就绪令牌保证启动顺序 | 架构 | 存 | 01 §D3 | `sef.rs:46` | 回答"没启动能不能收消息" |
| K-125 | ARCH S-1 ~ S-11 决策总表 | 架构演进 | 存（分散） | 各篇 §3 末 | 各篇 ARCH 表；`plan.md` | 回答"一共做了几个架构级决策" |
| K-126 | 纯函数化清单（pick / demote / rebalance_one / route_*） | 架构 | 存（分散） | 各篇 §3 | `cpu.rs:70`、`noquantum.rs:51`、`balancer.rs:73`、`client.rs:77` | 回答"哪些决策可以单测" |
| K-127 | 行为差异清单（C vs Rust） | 架构演进 | **新** | 无 | 见 3.5 F-4 | 回答"Rust 与 C 哪里不一样" |
| K-128 | C 侧构建（`Makefile` + `minix.service.mk` + boot_image 登记） | 工具与工程 | **新** | 无 | `servers/sched/Makefile`；`kernel/table.c` | 回答"SCHED 怎么编进镜像" |
| K-129 | Rust 侧构建（`Cargo.toml` 的 bin/lib 双目标） | 工具与工程 | **新** | 无 | `os/servers/sched/Cargo.toml` | 回答"为什么一个 crate 有两个目标" |
| K-130 | 测试基线（`cargo test -p minix-sched` 81 passed / clippy 0） | 测试性质 | 存（数字陈旧） | 各篇 §5.1（写 59） | `todo.md §0` | 回答"改完怎么验收" |
| K-131 | 覆盖率门（`design-coverage-check.sh --stage 06-stage-sched`） | 工具与工程 | 存 | todo.md §0 | `tools/design-coverage-check.sh` | 回答"文档完整性谁把关" |
| K-132 | edge 开口项（E-SCHEDNICED / E-PREEMPTFLAG / E-SCHEDSMP / E8） | 边界 | 存 | todo.md §2 | `edge_todo.md` | 回答"本 stage 还剩什么没做" |
| K-133 | `os/servers/sched/src/main.rs` 是唯一装配层 | 架构 | 存 | 01 §D4 | `main.rs:20-56` | 回答"决策在库里还是二进制里" |

### L 组：支线与全局

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-140 | 与其它 OS 的对照（Linux CFS / 用户态调度器 / 微内核两级调度） | 概念 | 存（每篇一节，共 14 处） | 01~14 各 §1.6/1.7 | OS 理论 | 回答"Minix3 的选择特殊在哪" |
| K-141 | 已知 C 缺陷清单（SUSPEND 死分支 / pick_cpu 重入 / 重试环有界性 / sched_proc 重复置位 / cpu_mask 空字段 / 台账不对称） | 已知缺陷 | **新** | 无 | 见 3.5 | 回答"C 源码里哪些地方不可信" |
| K-142 | 已声明不做（多调度器转派、真实负载迁移、cpu_mask 亲和） | 架构演进 | 存 | 11 §1.5、03 §1.6、06 §1.6 | `schedule.c:185,351` | 回答"哪些口子留着没实现" |
| K-143 | 常量 / 消息 / 错误码三张索引表 | 工具与工程 | 存（99 是骨架） | 99 | `config.h`、`com.h`、`errno` | 回答"某个数在哪定义" |
| K-144 | 与其它 stage 的边界索引（kernel 11 / pm 16 / rs 08,16） | 边界 | 存（分散） | 12/13/14 分工声明 | 三个 stage 的对应篇 | 回答"这个概念归谁讲" |

### 2.2 统计摘要

- 总条数 **118**（存量 96 / 新增 22）。
- 按类型：概念 14、机制 45、数据结构 6、接口与协议 16、约束与不变量 21、架构演进 12、工具与工程 4、测试性质 1、已知缺陷 6、边界 4。
- 按现有文档分布（主讲述点）：01=9、02=11、03=8、04=7、05=10、06=13、07=6、08=9、09=10、10=9、11=7、12=13、13=9、14=7、00/99=4、无归属（新增）=22。
- 重复展开 >1 处的主题：见 3.2（9 条）。

---

## 3. 覆盖审计

### 3.1 主题全集的四路来源

1. **C 符号**：`main.c`（main/reply/sef_local_startup/sef_cb_init_fresh）、`schedule.c`（pick_cpu / do_noquantum / do_stop_scheduling / do_start_scheduling / do_nice / schedule_process / init_scheduling / balance_queues）、`utility.c`（no_sys / sched_isokendpt / sched_isemtyendpt / accept_message）、`schedproc.h`（struct + IN_USE）、内核侧 `do_schedule` / `do_schedctl` / `sched_proc` / `proc_no_time` / `notify_scheduler` / `enqueue` / `dequeue`、客户端侧 `sched_start` / `sched_inherit` / `sched_stop` / `sys_schedule` / `sys_schedctl` / `sys_setalarm`、对端 `pm/schedule.c` / `rs/utility.c` / `rs/manager.c` / `rs/request.c`。
2. **OS 通用概念**：多级队列调度、优先级继承、时间片轮转、降级与恢复、负载均衡、用户态策略与内核机制分离、nice 语义。
3. **非 C 制品**：见 3.4。
4. **阶段边界契约**：`00-master-plan/README.md:24` 给本 stage 的"由 RS 加载，调度参数继承"；`edge_todo.md` 的 E-SCHEDNICED / E-PREEMPTFLAG / E-SCHEDSMP / E8。

### 3.1.1 覆盖缺口表

| # | 缺口主题 | 证据锚点 | 建议 |
|---|---------|---------|------|
| G-1 | SCHED 在系统启动因果链中的位置（boot_image 登记、RS 加载顺序、与 VM 解除 VMINHIBIT 的关系） | `kernel/table.c`；`00-master-plan/README.md:40,53` | 新建 01 §启动位置；不展开 kernel 启动（归 01-stage-kernel） |
| G-2 | 双 seam 与 crate 分层（读代码前的地图） | `kernel_api/transport.rs:50,127`；`lib.rs:8-20` | 新建 18 |
| G-3 | C 构建与镜像登记（Makefile / minix.service.mk / boot_image） | `servers/sched/Makefile`；`kernel/table.c` | 新建 19 |
| G-4 | Rust 构建与测试基建（bin/lib 双目标、81 passed 基线、clippy、coverage 门） | `Cargo.toml`；`todo.md §0` | 新建 19 |
| G-5 | C 已知缺陷与"不可信注释"清单 | `main.c:90`；`schedule.c:77,302,667-678`；`system.c:663-666` | 新建 90 支线 |
| G-6 | Rust 与 C 的行为差异清单（pick 重入、失败上界、SUSPEND） | `server.rs:378` vs `schedule.c:302`；`server.rs:138` vs `main.c:40` | 新建 18 §差异清单 |
| G-7 | 错误码单一总表（现分散在 5 篇） | `utility.c` 各 return；`errno` | 并入新 05 + 99 索引 |
| G-8 | 常量单一总表（现散在 05/09/10/11 各篇） | `config.h:66-77`；`schedule.c:16-46` | 并入新 06 + 99 索引 |
| G-9 | 测试基线与技术手段（每篇各写一遍统计，数字已不一致） | `todo.md §0`（81）vs 01 §5.1（59） | 并入新 19；各篇 §5 只列本篇测试名 |
| G-10 | `do_update` 对 `p_scheduler` 的迁移 | `do_update.c:252` | 新 17 一句话 + 指向 `03-stage-rs/16` |
| G-11 | 与其它 OS 的对照（14 处重复） | — | 收进 90，正文只保留 4 处 |
| G-12 | `niced` 端到端（wire → MF_NICED → CP_NICE）当前是否完整 | `todo.md §2` E-SCHEDNICED | 新 06 标注 edge 开口状态 |
| G-13 | 内核 PREEMPTIBLE 近似（priority != 0） | `todo.md §2` E-PREEMPTFLAG | 新 09 标注开口状态 |
| G-14 | 每 CPU 队列 / 迁移 / EBADCPU 三件套未通电 | `todo.md §2` E-SCHEDSMP | 新 10 标注开口状态 |

### 3.2 重复主题表

| 主题 | 现在讲了几次 | 保留哪篇作主讲述点 | 其余改为 |
|------|------------|------------------|---------|
| 五种消息总表 | 00、02、99 | 新 03 | 新 00 只给一行数字 + 指向；新 99 只给编号表 |
| `niced` 语义 | 05、09、12、13 | 新 06（定义与端到端） | 新 08（只讲它跟着每条消息走）、新 09（只讲它在内核落点）引用 |
| `pick_cpu` | 06、09、10 | 新 10 | 新 08 一句"CPU 字段由谁填"；新 11 只留重试环 |
| `schedule_process_local` 局部下发 | 08、09、11 | 新 08（掩码定义） | 新 12 / 新 14 只写"LOCAL"这个名字 |
| 端点校验三道/四道判断 | 04、06、07、08 | 新 05 | 其余只写"查已用槽 / 查空槽" |
| 错误码 | 04、06、07、08、09 | 新 05（总表） | 其余只写错误码名，不重复 errno 定义 |
| 与其它 OS 对照 | 14 篇各一节 | 新 90 | 正文保留 4 处（01/06/10/14），其余删除 |
| 双层调度模型 | 00、01、99 | 新 01 | 其余一行引用 |
| 七统计字段 | 12（两节）+ 08 | 新 09 | 新 12 一行引用 |

### 3.3 越界主题表

| 越界 | 现在在哪 | 正确归属 |
|------|---------|---------|
| 内核就绪队列 `enqueue`/`dequeue`/`pick_proc` 的完整机制 | 12 §1.6 展开 | `01-stage-kernel/11-scheduling-primitives.md`（本 stage 只引用"出队/入队"两个动作） |
| `nice↔queue` 换算的推导与 `do_getsetpriority` 权限 | 05 §2.4、13 §2.4 展开 | `04-stage-pm/16-scheduling.md`（本 stage 只取换算结果与调用点） |
| RS 槽位 `r_*` 配置装载 | 14 §1.1 提及 | `03-stage-rs/08-rs-slot-config.md` |
| Live Update 全流程 | 14 §1.4 提及取消 | `03-stage-rs/16-rs-live-update.md` |
| SEF 库自身的实现 | 01 §1.2 | libsef / 通用库 stage（本 stage 只讲"SCHED 注册了什么"） |

### 3.4 非 C 主题十项逐条回答

| 主题 | 本 stage 是否有 | 在哪里讲 | 理由 |
|------|---------------|---------|------|
| 链接与加载 | 有（边缘） | 新 19（`Makefile` + `minix.service.mk` + boot_image 登记） | SCHED 是被加载者，不展开 ELF 装载（归 01-stage / 03-stage-rs） |
| 镜像与内存布局 | 有（边缘） | 新 01 一句 + 新 19 | 只讲"SCHED 在 boot_image 里占一槽"，布局细节归 kernel |
| 汇编入口与陷阱进入 | 无 | 不在本 stage | 用户态服务器无汇编入口；`_taskcall`/`_kernel_call` 的陷入机制归 01-stage-kernel/13 |
| 启动装配 | 有 | 新 01（因果链）+ 新 02（sef 回调）+ 新 19（bin 装配层） | 三层各讲一层 |
| 构建与工具链 | 有 | 新 19 | 缺口 G-3/G-4 |
| 跨模块接口与线格式 | 有 | 新 03（五个消息体）+ 新 15（客户端三包）+ 新 99（索引） | 线格式是 SCHED 的核心资产 |
| 错误路径 | 有 | 新 05（总表）+ 各处理篇的失败分支 | 缺口 G-7 |
| 关闭与退出 | 有 | 新 13（STOP）+ 新 16（PM exit）+ 新 17（RS 清理） | 三条来路 |
| 并发与同步 | 部分 | 新 18（单线程事件循环 + 无锁）+ 新 10（SMP 台账） | 服务是单线程；SMP 台账是数据而非临界区 |
| 测试基建 | 有 | 新 19 | 缺口 G-4/G-9 |

### 3.5 事实纠错清单（B 相必须按此改写，逐条带证据）

| # | 现文档说法 | C / Rust 事实 | 判定 |
|---|-----------|--------------|------|
| F-1 | 01 §1.5 把 SUSPEND 讲成"这次不回复的约定" | `main.c:90` 是 SCHED 目录里唯一出现 SUSPEND 的地方，四个处理函数无一处返回它 | 约定成立但**在本服务内不可达**；须写明"这是从 PM 抄来的骨架，SCHED 侧恒真" |
| F-2 | 06 §1.5 断言重试环"一定能结束"（理由：标记只减不增） | `schedule.c:229` 写 `CPU_DEAD(-1)`；`cpu_proc` 是 `static unsigned`（`schedule.c:46`），-1 = UINT_MAX；`schedule.c:77` 的 `cpu_proc[cpu]++` 会让 UINT_MAX 回绕为 0，死亡标记被清除 | **断言存疑**：单核且核不可用时可能不终止。B 相须以 C 无符号语义复核，改为"界未保证"或给出反证（**推测，待验证**：验证命令 `sed -n '46p;77p;226,231p' minix3/minix/servers/sched/schedule.c`） |
| F-3 | 10 §1.4 说"死核的计数值被标成无符号最大值，严格小于的比较永远选不中它" | 与 `schedule.c:71` 的 `cpu_load > cpu_proc[c]` 一致 | 成立，保留 |
| F-4 | 各篇 §5.1 测试统计写"59 passed" | `todo.md §0` 基线 81 passed（2026-09-09，2026-09-14 复核未回退） | 数字陈旧，须以实测为准重写 |
| F-5 | 09 §2.2 描述"按掩码挑字段"但未提 `schedule_process` 内部**无条件再调 `pick_cpu`** | `schedule.c:302`；Rust 侧 `server.rs` 只在 `do_start` 调 `pick`（`378,389`） | C 每次下发都重选并 +1；Rust 未复现。属**行为差异**，须在新 08 / 新 10 / 新 18 三处一致标注 |
| F-6 | 12 §2.5 引 `proc.c:1893-1910` | 现源码 `proc_no_time` 在 `proc.c:1893`（`notify_scheduler` 在 1860） | 锚点仍有效，但须补"非抢占进程只续时间片"这一支 |
| F-7 | 03 §2.x 出现 `schedproc.h:CONFIG_MAX_CPUS（L24，工具生成）` 形式的锚点 | 实际文件 39 行，字段在 `schedproc.h:23-36` | **锚点被工具污染**，B 相全部改回行号锚点 `schedproc.h:24,26,29-32,33,36,39` |

---

## 4. 新目录

### 4.1 新篇章总表

| 编号 | 标题 | 一句话定位 | 分组 |
|------|------|-----------|------|
| 00 | SCHED 总览与文档导航 | 给一张启动主线图、一张文件分组地图、一张文档导航表 | 入口 |
| 01 | 双层调度模型与服务定位 | 讲清 SCHED 为什么存在、站在系统何处、与谁分工 | A 模型 |
| 02 | 启动装配与主循环 | 讲清 SCHED 怎么起来、主循环怎么转、什么时候不回复 | B 启动 |
| 03 | 消息面与五种消息 | 讲清 SCHED 收什么、谁来敲门、回不回 | B 启动 |
| 04 | schedproc 记录结构 | 讲清调度器眼里的一个进程由哪七个字段构成 | C 数据 |
| 05 | 槽位换算、门禁与错误码 | 讲清端点到槽位的换算、三道门、六种拒绝 | C 数据 |
| 06 | 优先级、时间片与 nice 模型 | 讲清 16 个队列、两个优先级字段、单位体系与 nice 换算 | C 数据 |
| 07 | 内核契约（一）：接管 sys_schedctl | 讲清 SCHED 怎么成为某个进程的调度者 | D 契约 |
| 08 | 内核契约（二）：下发 sys_schedule | 讲清一次下发带什么、内核怎么查、怎么写入 | D 契约 |
| 09 | 内核契约（三）：NO_QUANTUM 回传与记账 | 讲清时间片耗尽时内核推什么过来、SCHED 用了哪一部分 | D 契约 |
| 10 | CPU 选择与 SMP 放置 | 讲清进程落在哪个核、负载台账怎么记、死核怎么退场 | E 放置 |
| 11 | 请求处理：START 与 INHERIT | 讲清一个进程怎么被登记进 SCHED | F 处理 |
| 12 | 请求处理：NO_QUANTUM 降级与 SET_NICE | 讲清在册进程的优先级怎么被改写 | F 处理 |
| 13 | 请求处理：STOP 与槽位释放 | 讲清一个进程怎么从 SCHED 退场 | F 处理 |
| 14 | 定时与队列平衡 | 讲清 5 秒一次的自持循环把谁拉回来 | G 策略 |
| 15 | 客户端面：libsys 的选路与三种包 | 讲清申请方怎么选路、组装什么、怎么读回复 | H 对端 |
| 16 | 对端一：PM | 讲清用户进程的生、变、死在 PM 侧怎么发起 | H 对端 |
| 17 | 对端二：RS | 讲清系统进程的申请与取消 | H 对端 |
| 18 | Rust 实现导览与行为差异 | 讲清 crate 分层、双 seam、ARCH 决策总表、与 C 的差异 | I 工程 |
| 19 | 构建、测试与覆盖基建 | 讲清 C 与 Rust 两侧怎么编、怎么测、怎么验收 | I 工程 |
| 90 | 支线：对照、已知缺陷与已声明不做 | 收容不适合进主线的材料 | 支线 |
| 99 | 全局概念与索引 | 常量表、消息表、错误码表、边界索引 | 索引 |

### 4.2 阅读路径

- **主线（第一次读，逐篇顺序）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17。
- **支线（主线读完再读）**：18（读代码前）、19（动手改之前）、90（有余力）、99（随时查）。
- **可跳读**：90、99；17（只关心 PM 时）；14（只关心单请求生命周期时）。
- **并行体的组织**：五种消息不是线性序，新 03 给"统一框架 + 五成员差异表"，并声明触发时机（谁在什么时候发起），把逐条实现下放 11/12/13 —— 符合"汇聚点加触发时机"规则。
- **代表成员精讲**：请求处理三篇里，11（START）是代表成员（检查最全、分支最多），12/13 用"与 11 的差异表"收束（同一张检查顺序表，只列出不同的门）。

### 4.3 两条硬规则在新目录里的执行方式

- **前置**：只允许指向更早编号（新目录共 22 篇，全部契约的"前置"字段已逐条扫描，见 §9 G3）。
- **下放**：允许指向更晚编号，但本篇正文不依赖其内容——契约里用"不讲什么 / 下放给谁"表达，与"前置"严格分开。

---

## 5. 每篇契约

> 约定：每篇保留项目七节模板（概念 / C 源码分析 / Rust 设计决策 / 实现详解 / 测试要点 / 过渡 / 参见），但对三节施加约束：**Rust 设计决策**每篇最多 4 条且只讲本篇映射（ARCH 总表归 18）；**测试要点**只列本篇测试函数名与统计，不重复基线（基线归 19）；**与其它 OS 的对照**只在 01/06/10/14 四篇保留，其余并入 90。

### 00-SCHED 总览与文档导航

- 一句话定位：给读者一张"我在哪、接下来读什么"的地图。
- 讲什么：K-001、K-005、K-006、K-143、K-144（各一行，不展开）。
- 不讲什么：一切机制细节（→ 01~17）；对照（→ 90）；测试与构建数字（→ 19）。
- 前置：无。
- 后置：全部 21 篇。
- 事实底线：`servers/sched/{main,schedule,utility}.c`、`schedproc.h`、`Makefile`；`os/servers/sched/Cargo.toml`；`00-master-plan/README.md:24,40`；写法模板 `05-stage-vfs/00-vfs-overview.md`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-001 | 双层调度模型 | 概念 | `main.c:1-3` | 一行定位需要它 | 存：旧 00 核心点 |
| K-005 | 启动因果链位置 | 机制 | `rs/main.c:376`；`00-master-plan/README.md:24` | 主线图的起点 | 新 |
| K-006 | 单线程事件循环 | 约束 | AGENTS.md | 全阶段共守 | 存 |
| K-143 | 三张索引表指针 | 工程 | 99 | 导航的一部分 | 存：旧 99 |
| K-144 | 跨 stage 边界索引 | 边界 | 12/13/14 分工声明 | 导航的一部分 | 存 |

- 验收标准：① 有一张从"RS 加载"到"主循环"的 ASCII 主线图，每站标注篇号；② 有一张 C 文件到篇号的分组地图（6 个文件全覆盖）；③ 有一张 22 行的文档导航表（编号 / 标题 / 一句话 / 前置）；④ 读者读完 00 能不看目录说出下一步读哪篇。

### 01-双层调度模型与服务定位

- 一句话定位：回答"Minix3 为什么把调度策略放到一个用户态进程里，以及 SCHED 站在系统的哪个位置"。
- 讲什么：K-001、K-002、K-003、K-004、K-005、K-007、K-008。
- 不讲什么：主循环骨架（→ 02）；消息编号（→ 03）；就绪队列机制（→ `01-stage-kernel/11-scheduling-primitives.md`）；`nice` 换算（→ `04-stage-pm/16-scheduling.md`）。
- 前置：00。
- 后置：02、03、07、15、16、17。
- 事实底线：`main.c:1-3`；`do_schedule.c:20-21`；`do_schedctl.c:28-43`；`proc.h:34,178-179`；`sched_start.c:34-39`；`00-master-plan/README.md:40,53`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-001 | 双层调度模型 | 概念 | `main.c:1-3` | 本篇主干 | 存：旧 00/01/99 |
| K-002 | 五进程表分工 | 概念 | `schedproc.h:23` | 定位需要对照 | 存：旧 03 §1.1 |
| K-003 | 三方职责边界 | 概念 | `main.c:44-87` | 定位需要对照 | 存：旧 00 边界 |
| K-004 | 策略可替换性 | 概念 | `schedule.c:348-352` | 解释"为什么值得" | 存：旧 01 §1.1 |
| K-005 | 启动因果链位置 | 机制 | `rs/main.c:376`；`00-master-plan/README.md:24` | 补旧 00 的一句话 | 新 |
| K-007 | 调度者/被调度者分离 | 概念 | `do_schedctl.c:42` | 模型的一半 | 存：旧 12 §1.2 |
| K-008 | 多调度器转派预留口 | 架构演进 | `schedule.c:239-248` | 模型的边界 | 存：旧 06 §1.6 |

- 验收标准：① 能画出"内核 / SCHED / PM / RS"四方与三类箭头（下发、通知、申请）；② 能回答"改调度策略要不要重编内核"并给出代码证据；③ 明确写出内核调度与用户态调度在 `p_scheduler` 上的两种表示。

### 02-启动装配与主循环

- 一句话定位：回答"SCHED 怎么起来、起来后每轮做什么、什么时候不回复"。
- 讲什么：K-010、K-011、K-012、K-013、K-014、K-015、K-016、K-017、K-018、K-019、K-020。
- 不讲什么：消息编号与消息体（→ 03）；平衡机制本身（→ 14，本篇只给装闹钟这一动作）；机器信息的用途（→ 10）；处理函数实现（→ 11/12/13）。
- 前置：00、01。
- 后置：03、10、11、12、13、14、18。
- 事实底线：`main.c:22-96,101-136`；`schedule.c:16-18,334-343`；`type.h:122-125`；Rust `main.rs:20-56`、`sef.rs:19-56`、`server.rs:128-161`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-010 | SEF 两回调注册 | 机制 | `main.c:111-121` | 启动第一件事 | 存：旧 01 §1.2 |
| K-011 | restart 由 libsef 代办 | 约束 | `main.c:115` | 解释"为什么没代码" | 存：旧 01 §1.2 |
| K-012 | sys_getmachine + panic | 机制 | `main.c:130-131` | 启动第二件事 | 存：旧 01 §1.3 |
| K-013 | init_scheduling 装闹钟 | 机制 | `schedule.c:334-343` | 启动第三件事 | 存：旧 01 §1.3、旧 11 §2.2 |
| K-014 | 主循环四步 | 机制 | `main.c:35-96` | 本篇主干 | 存：旧 01 §1.4 |
| K-015 | ipc_status 两位信息 | 接口 | `main.c:39` | 分流的前提 | 存：旧 01 §2.2 |
| K-016 | 通知先于调用 | 约束 | `main.c:44-55` | 循环的第一道门 | 存：旧 02 §1.3 |
| K-017 | SUSPEND 死分支 | 事实 | `main.c:90` | 纠正旧 01 §1.5 | 新（F-1） |
| K-018 | 回复失败只打印 | 约束 | `main.c:101-106` | 失败路径 | 存：旧 01 §2.6 |
| K-019 | no_sys → ENOSYS | 接口 | `utility.c:18-23` | 兜底分支 | 存：旧 02 §2.6 |
| K-020 | 启动 panic 纪律 | 约束 | `main.c:131`；`schedule.c:341` | 与 Rust 的对比点 | 存：旧 01 §D2 |

- 验收标准：① 真序表 S0~S8 与 L1~L12 逐条有锚点；② 明确写出"SCHE D 从不返回 SUSPEND"并给出 grep 证据；③ 说明 Rust 的 `run` 为什么不再无条件 panic（`server.rs:138`）以及这是偏离还是演进。

### 03-消息面与五种消息

- 一句话定位：回答"SCHED 收什么、谁有资格敲哪扇门、什么时候回"。
- 讲什么：K-021、K-022、K-023、K-024、K-025、K-026、K-027、K-028、K-029、K-030。
- 不讲什么：字段语义的上限/单位（→ 06）；槽位校验的实现（→ 05）；各处理函数（→ 11/12/13）；内核侧构造细节（→ 09）。
- 前置：01、02。
- 后置：05、06、09、11、12、13、15。
- 事实底线：`com.h:801-807`；`ipc.h:272,1101,1112,1437,1444,1827,1912`；`main.c:44-87`；`proc.c:1874`；`minix-types/src/types/com.rs:85-103`；Rust `dispatch.rs:18-108`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-021 | 五种消息与编号 | 接口 | `com.h:801-807` | 本篇主干 | 存：旧 02 §1.2 |
| K-022 | 五个消息体线格式 | 接口 | `ipc.h` 七处 | 汇聚点 | 存：旧 02 §2.7 |
| K-023 | START/INHERIT 共入口 | 机制 | `main.c:58-61` | 分派规则 | 存：旧 02 §2.3 |
| K-024 | NO_QUANTUM 来源校验 | 约束 | `main.c:68-84` | 唯一需要验来源的门 | 存：旧 02 §1.4 |
| K-025 | 内核代填 m_source | 机制 | `proc.c:1874`；`schedule.c:92` | 补旧文档的空白 | 新 |
| K-026 | 三态分类 | 机制 | `main.c:45` | 分流规则 | 存：旧 02 §D2 |
| K-027 | 回复三分规则 | 约束 | `main.c:54,76,90` | 本篇出口 | 存：旧 02 §1.5 |
| K-028 | minix-types 权威落点 | 架构演进 | `minix-types/src/ipc/*` | 单一事实源 | 存：旧 02 §D6 |
| K-029 | 禁止本地旁路常量 | 约束 | `todo.md §1` | 纪律 | 存 |
| K-030 | CLOCK 通知来源 | 机制 | `main.c:47` | 通知门的实例 | 存：旧 02 §2.2 |

- 验收标准：① 五成员差异表（编号 / 发送者 / 消息体 / 是否回复 / 处理函数 / 在哪篇讲）；② 一张"触发时机表"（谁在什么时候发起，含 PM fork、RS 加载、内核时钟、PM nice、PM/RS exit）；③ 明确写出 m_source 在 NO_QUANTUM 里不是发送者。

### 04-schedproc 记录结构

- 一句话定位：回答"调度器眼里的一个进程由哪七个字段构成、第八个为什么没了"。
- 讲什么：K-031、K-032、K-033、K-039、K-040、K-049。
- 不讲什么：字段取值的语义与单位（→ 06）；字段谁写（→ 11/12/13）；表管理与门禁（→ 05）。
- 前置：01、03。
- 后置：05、06、11、12、13。
- 事实底线：`schedproc.h:23-39`（行号锚点，替换被污染的 `CONFIG_MAX_CPUS（L24）` 形式）；`schedule.c:185`；Rust `schedproc.rs:18,27,43,72`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-031 | 七字段 | 数据结构 | `schedproc.h:23-36` | 本篇主干 | 存：旧 03 |
| K-032 | cpu_mask 消除 | 架构演进 | `schedproc.h:33`；`schedule.c:185` | 需要单独论证 | 存：旧 03 §1.6 |
| K-033 | IN_USE 唯一标志 | 数据结构 | `schedproc.h:39` | 占用语义 | 存：旧 03 §1.3 |
| K-039 | 表长 NR_PROCS | 数据结构 | `schedproc.h:36` | 结构的一部分 | 存：旧 03 §2.7 |
| K-040 | Priority/SlotState 新类型 | 架构演进 | `schedproc.rs:27,43` | 类型的落点 | 存：旧 03 §3 |
| K-049 | 上限/当前位置分工 | 概念 | `schedproc.h:29-30` | 字段关系 | 存：旧 05 §1.3（移到本篇只讲"有两个"） |

- 验收标准：① 七字段表（名 / 类型 / 谁写 / 谁读 / 在哪篇讲）；② 明确写出 cpu_mask 从未被读写（并给出"没有任何消息携带亲和性"的证据）；③ 锚点全部是 `schedproc.h:NN` 行号形式（F-7）。

### 05-槽位换算、门禁与错误码

- 一句话定位：回答"端点怎么变成槽位、三道门各拦什么、被拒时回什么"。
- 讲什么：K-034、K-035、K-036、K-037、K-038、K-033（引用）。
- 不讲什么：字段语义（→ 04/06）；各处理函数里"为什么查这道门"（→ 11/12/13）；errno 的 POSIX 定义（→ 99 索引）。
- 前置：03、04。
- 后置：11、12、13、16、17。
- 事实底线：`utility.c:29-74`；`errno` 六处；Rust `table.rs:17-91`、`valid.rs:14-42`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-034 | _ENDPOINT_P 换算 | 机制 | `utility.c:31` | 门禁第一步 | 存：旧 04 §1.2 |
| K-035 | isokendpt 四道判断 | 机制 | `utility.c:29-41` | 查已用槽 | 存：旧 04 §1.3 |
| K-036 | isemtyendpt 三道镜像 | 机制 | `utility.c:46-56` | 查空槽 | 存：旧 04 §1.4 |
| K-037 | PM/RS 白名单 | 约束 | `utility.c:61-74` | 唯一放行的两人 | 存：旧 04 §1.5 |
| K-038 | 错误码总表 | 接口 | `utility.c:33,35,37,39`；`system.c:646,649,653,655` | 缺口 G-7 | 存（旧 04 §2.4 + 分散四处） |

- 验收标准：① 一张"门 → 判断 → 错误码 → 触发条件"四列总表，覆盖 SCHED 侧与内核侧全部拒绝点；② 明确写出"任务进程（负槽位）永远进不来"；③ 三个 Rust 函数与 C 三个函数的一一对应表。

### 06-优先级、时间片与 nice 模型

- 一句话定位：回答"16 个队列怎么用、上限和当前位置怎么分、200 是什么单位、nice 怎么落进来"。
- 讲什么：K-041、K-042、K-043、K-044、K-045、K-046、K-047、K-048、K-050、K-049（承接）。
- 不讲什么：换算的推导与 `getpriority` 权限（→ `04-stage-pm/16-scheduling.md`）；谁写这两个字段（→ 11/12）；下发格式（→ 08）。
- 前置：04、05。
- 后置：07、08、09、10、11、12、14、16。
- 事实底线：`config.h:63-77`；`schedule.c:41,44,319`；`pm/utility.c:91-103`；`proc.h:262`；`arch_clock.c:318`；`system.c:691-694`；Rust `priority.rs:26-191`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-041 | 16 队列常量 | 常量 | `config.h:66-72` | 本篇主干 | 存：旧 05 §1.2 |
| K-042 | 小数=高优先级 | 概念 | `config.h:63-65` | 反直觉点 | 存：旧 05 §1.2 |
| K-043 | 毫秒单位与 200 | 常量 | `schedule.c:41` | 单位体系 | 存：旧 05 §1.4 |
| K-044 | nice→队列换算 | 机制 | `pm/utility.c:91-103` | 入口语义 | 存：旧 05 §1.5 |
| K-045 | niced 判定 | 机制 | `schedule.c:319` | 端到端起点 | 存：旧 05 §1.5 |
| K-046 | MF_NICED → CP_NICE | 机制 | `arch_clock.c:318` | 端到端终点 | 存：旧 05 §2.5 |
| K-047 | 系统进程判定 | 机制 | `schedule.c:44` | 影响放置 | 存：旧 05 §1.6 |
| K-048 | USER_DEFAULT_CPU -1 | 常量 | `config.h:77` | 与 KEEP 的区别 | 存：旧 05 §1.4 |
| K-050 | Nice/CpuChoice 新类型 | 架构演进 | `priority.rs:85,127` | 类型落点 | 存：旧 05 §3 |

- 验收标准：① 常量表（名 / 值 / 出处 / 用途）覆盖 `config.h:66-77` 全部；② 一张"nice -20 → 0，0 → 7，20 → 15"的换算样例；③ `niced` 端到端链路图（nice → maxprio → `niced` 位 → `MF_NICED` → CP_NICE），并标注 E-SCHEDNICED 的当前开口状态（G-12）。

### 07-内核契约（一）：接管 sys_schedctl

- 一句话定位：回答"SCHED 怎么成为某个进程的调度者、内核那一侧发生了什么"。
- 讲什么：K-051、K-052、K-053、K-054、K-007（引用）、K-068。
- 不讲什么：登记流程的其它步骤（→ 11）；下发参数（→ 08）；就绪队列机制（→ `01-stage-kernel/11`）；Live Update 全流程（→ `03-stage-rs/16`）。
- 前置：06。
- 后置：09、11、15。
- 事实底线：`do_schedctl.c:7-46`；`libsys/sys_schedctl.c`；`proc.h:34,178-179`；`sched_start.c:70-77`；`do_update.c:252`；Rust `kernel_api/schedctl.rs:27-100`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-051 | flags=0 → 调用者接管 | 机制 | `do_schedctl.c:40-43` | 本篇主干 | 存：旧 12 §2.2 |
| K-052 | KERNEL 标志 → p_scheduler=NULL | 机制 | `do_schedctl.c:28-39` | 另一个分支 | 存：旧 12 §2.2 |
| K-053 | flags 白名单校验 | 约束 | `do_schedctl.c:17-21` | 第一道门 | 存：旧 12 §2.1 |
| K-054 | proc_kernel_scheduler 谓词 | 机制 | `proc.h:178-179` | 归属判定 | 存：旧 12 §2.6 |
| K-068 | do_update 迁移 p_scheduler | 机制 | `do_update.c:252` | 缺口 G-10 | 新 |

- 验收标准：① 能说清"接管"在内核里就是写一个指针；② 两个分支的入口/出口状态表（flags / priority / quantum / cpu / p_scheduler 变化）；③ 明确 `sys_schedctl(0, ep, 0,0,0)` 里后三个 0 为什么无害（flags≠KERNEL 时不读它们）。

### 08-内核契约（二）：下发 sys_schedule

- 一句话定位：回答"一次下发带什么、内核怎么查、怎么写入、什么情况返回 EBADCPU"。
- 讲什么：K-055、K-056、K-057、K-058、K-059、K-060、K-061、K-074（事实部分）。
- 不讲什么：选核策略（→ 10）；CPU 字段从哪来（→ 10）；谁来调用它（→ 11/12/14）；就绪队列与 pick_proc（→ `01-stage-kernel/11`）。
- 前置：05、06、07。
- 后置：09、10、11、12、14。
- 事实底线：`schedule.c:22-35,297-328`；`libsys/sys_schedule.c`；`ipc.h:1112`；`do_schedule.c:8-30`；`system.c:642-699`；Rust `kernel_api/schedule.rs:55-121`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-055 | 五参数 | 接口 | `sys_schedule.c`；`ipc.h:1112` | 线格式 | 存：旧 09 §2.3 |
| K-056 | caller == p_scheduler | 约束 | `do_schedule.c:20-21` | 权限门 | 存：旧 09 §2.4 |
| K-057 | 三道范围检查 | 机制 | `system.c:644-656` | 内核侧检查 | 存：旧 09 §1.5 |
| K-058 | KEEP = -1 | 接口 | `schedule.c:304-317`；`system.c:680-688` | 哨兵约定 | 存：旧 09 §1.3 |
| K-059 | 三种掩码 | 机制 | `schedule.c:22-35` | 本篇主干 | 存：旧 09 §1.2 |
| K-060 | 写入四步 + 出队入队 | 机制 | `system.c:667-699` | 内核侧写入 | 存：旧 09 §2.5 |
| K-061 | EBADCPU 与 cpu_is_ready | 约束 | `system.c:654-655` | 失败码来源 | 存：旧 09/10 |
| K-074 | 入口无条件再调 pick_cpu | 机制 | `schedule.c:302` | 差异 F-5 | 部分新 |

- 验收标准：① 掩码四形状表（名字 / 位组合 / 带什么 / 谁在用）；② 内核"三查四写"逐步表；③ 明确标注 `schedule_process` 内部重选 CPU 这一 C 事实，以及 Rust 未复现它（`server.rs:378`）——三处一致（08/10/18）。

### 09-内核契约（三）：NO_QUANTUM 回传与记账

- 一句话定位：回答"时间片耗尽时内核推什么过来、SCHED 用了哪一部分、哪些字段目前没人读"。
- 讲什么：K-062、K-063、K-064、K-065、K-025（承接）、K-066、K-067、K-079。
- 不讲什么：降级规则本身（→ 12）；就绪队列与 dequeue（→ `01-stage-kernel/11`）；SCHED 主循环的分派（→ 03）。
- 前置：07、08。
- 后置：12、18。
- 事实底线：`proc.c:1860-1919`、`proc.h:50-55,262`、`priv.h:45-50`、`const.h:143`、`system.c:663-678`、`proc.c:1882`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-062 | proc_no_time 两分支 | 机制 | `proc.c:1893-1910` | 回传的触发条件 | 存：旧 12 §1.6 |
| K-063 | 七统计字段 + reset | 接口 | `proc.c:1876-1886` | 回传内容 | 存：旧 12 §1.5 |
| K-064 | 六字段无读者 | 事实 | `schedule.c:87-107` | 解释"为什么全发" | 存：旧 12 §1.5 |
| K-065 | mini_send FROM_KERNEL | 机制 | `proc.c:1887-1890` | 投递方式 | 存：旧 12 §2.3 |
| K-066 | 内核两处 FIXME | 已知缺陷 | `system.c:663-666` | 缺口 G-5 | 新 |
| K-067 | RTS_SET 重复两次 | 已知缺陷 | `system.c:667-678` | 缺口 G-5 | 新 |
| K-079 | cpu_load 回传未消费 | 事实 | `proc.c:1882` | 闭环缺口 | 存：旧 12 §1.5 |

- 验收标准：① 七字段表（名 / 类型 / 内核来源 / SCHED 是否消费）；② 一张"时钟中断 → proc_no_time → 通知 → 出队"的时序图；③ 明确写出非抢占进程走"直接续时间片"分支，且内核调度的进程永不发通知（并给出断言 `proc.c:1865`）；④ 标注 E-PREEMPTFLAG 的开口状态（G-13）。

### 10-CPU 选择与 SMP 放置

- 一句话定位：回答"进程落在哪个核、负载台账记了什么、死核怎么退场、重试环有没有界"。
- 讲什么：K-070、K-071、K-072、K-073、K-074、K-075、K-076、K-077、K-078、K-079（引用）。
- 不讲什么：下发消息本身（→ 08）；登记里为什么调它（→ 11）；内核每 CPU 队列与迁移（→ `01-stage-kernel/11`、`01-stage-kernel/16`）；机器信息怎么读（→ 02）。
- 前置：02、06、08。
- 后置：11、18。
- 事实底线：`schedule.c:37-81,226-231`；`type.h:122-125`；`schedproc.h:14-16`；`system.c:651-655`；Rust `cpu.rs:24-125`、`server.rs:378-391`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-070 | pick_cpu 三规则 | 机制 | `schedule.c:48-78` | 本篇主干 | 存：旧 10 §1.2 |
| K-071 | cpu_proc 台账 | 数据结构 | `schedule.c:46,77,130` | 主干输入 | 存：旧 10 §1.3 |
| K-072 | CPU_DEAD 与无符号 | 机制 | `schedule.c:37-39,46` | 反直觉点 | 存：旧 10 §1.4 |
| K-073 | 真过滤在负载比较 | 机制 | `schedule.c:71-74` | 反直觉点 | 存：旧 10 §1.4 |
| K-074 | 每次下发都重选 | 机制 | `schedule.c:302` | 跨篇一致 | 部分新 |
| K-075 | 台账 ++/-- 不对称 | 已知缺陷 | `schedule.c:77` vs `:130` | 缺口 G-5 | 新 |
| K-076 | 重试环有界性存疑 | 已知缺陷 | `schedule.c:227-231` + `:77` | 纠正旧 06 §1.5（F-2） | 新，**待验证** |
| K-077 | CONFIG_SMP 两套实现 | 约束 | `schedule.c:78-80` | 编译开关 | 存：旧 10 §1.5 |
| K-078 | Rust 只在 do_start 选核 | 差异 | `server.rs:378,389` | 差异 F-5 | 新 |

- 验收标准：① 三规则 + 回退路径的决策表（含"没有可用核时"这一列）；② 台账收支表（++ 三处 / -- 一处），并明确"它不是真实负载"；③ 重试环的界：C 侧给出"存疑 + 复核命令"，Rust 侧给出"死核是 `None`，候选集单调收缩，故有界"的论证；④ 标注 E-SCHEDSMP 开口状态（G-14）。

### 11-请求处理：START 与 INHERIT

- 一句话定位：回答"一个进程怎么被登记进 SCHED、三种填值方式差在哪、失败了槽位脏不脏"。
- 讲什么：K-080、K-081、K-082、K-083、K-084、K-085、K-091、K-008（承接）。
- 不讲什么：接管的内核实现（→ 07）；下发的内核实现（→ 08）；选核策略（→ 10）；客户端怎么组装（→ 15/16/17）。
- 前置：05、06、07、08、10。
- 后置：12、13、16、17。
- 事实底线：`schedule.c:140-249`；Rust `scheduling/start.rs:28-148`、`server.rs:289-411`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-080 | 检查顺序 | 机制 | `schedule.c:146-166` | 本篇主干 | 存：旧 06 §1.2 |
| K-081 | init 自父亲特例与覆盖 | 机制 | `schedule.c:171-188` + `:226` | 最容易误解的点 | 存：旧 06 §1.3 |
| K-082 | START 显式给值 | 机制 | `schedule.c:191-197` | 三种填值之一 | 存：旧 06 §2.4 |
| K-083 | INHERIT 抄父 | 机制 | `schedule.c:199-209` | 三种填值之一 | 存：旧 06 §2.5 |
| K-084 | 交接先于置位 | 约束 | `schedule.c:218-223` | 失败语义 | 存：旧 06 §2.6 |
| K-085 | 上限越界 EINVAL | 约束 | `schedule.c:164-166` | 门 | 存：旧 06 §D3 |
| K-091 | 重试环类型化 | 架构演进 | `server.rs:378-391` | Rust 侧 | 存：旧 06 §D6 |

- 验收标准：① 逐步检查表（步骤 / 门 / 失败码 / 槽位状态）；② 三种填值的对照表（来源 / 谁触发 / 覆盖关系）；③ 明确"init 的临时值被无条件覆盖"并给出 `schedule.c:226` 证据；④ 一张"fork 次主线路径图"（PM fork → 内核 → SCHED → 内核 → 子），与旧 06 §1.7 同规格。

### 12-请求处理：NO_QUANTUM 降级与 SET_NICE

- 一句话定位：回答"在册进程的优先级怎么被改写——一个改当前位置，一个改上限"。
- 讲什么：K-086、K-087、K-088、K-085（引用）、K-059（引用）。
- 不讲什么：内核怎么发通知（→ 09）；分发与来源校验（→ 03）；PM 侧权限（→ 16）；局部下发的掩码定义（→ 08）。
- 前置：03、05、06、08、09。
- 后置：14、16。
- 事实底线：`schedule.c:87-107`（降级）、`schedule.c:254-292`（改上限）；`main.c:68-84`；`pm/schedule.c:89-112`；Rust `scheduling/noquantum.rs:40-51`、`scheduling/nice.rs:54-83`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-086 | 降级 +1 | 机制 | `schedule.c:99-101` | 一半主干 | 存：旧 08 §2.1 |
| K-087 | 信任不对称 | 约束 | `main.c:70-71` vs `schedule.c:118,150,262` | 为什么这条特殊 | 存：旧 08 §1.2 |
| K-088 | 改上限四步与回滚 | 机制 | `schedule.c:265-291` | 另一半主干 | 存：旧 08 §2.2-2.3 |

- 验收标准：① 两个写操作的配对表（触发者 / 改哪个字段 / 是否查白名单 / 是否回复 / 失败后果）；② 回滚的字段清单（哪两个、为什么是这两个）；③ 明确"降级到 `MIN_USER_Q` 就不再降"的边界条件并给锚点。

### 13-请求处理：STOP 与槽位释放

- 一句话定位：回答"一个进程怎么从 SCHED 退场、释放了哪两样东西、三条来路各是什么后果"。
- 讲什么：K-089、K-090、K-071（减负载部分）。
- 不讲什么：客户端选路与短路（→ 15）；PM/RS 侧的调用位置（→ 16/17）；重登记（→ 11）。
- 前置：05、08、11。
- 后置：16、17。
- 事实底线：`schedule.c:112-135`；`ipc.h:1444`；`libsys/sched_stop.c`；`pm/forkexit.c:425-441`；`rs/manager.c:461`；`rs/request.c:342`；Rust `scheduling/stop.rs`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-089 | 释放两样东西 | 机制 | `schedule.c:129-134` | 本篇主干 | 存：旧 07 §1.3 |
| K-090 | 申请/取消对称表 | 概念 | `schedule.c:112-135` | 收束 | 存：旧 07 §1.5 |
| K-071 | 减负载 | 数据结构 | `schedule.c:130` | 台账出口 | 存：旧 07 §2.3 |

- 验收标准：① 对称表（登记七步 ↔ 释放三步）；② 明确"endpoint 字段不清、但 `isemtyendpt` 只看 IN_USE，所以槽位可复用"；③ 三条来路表（PM exit / RS 清理 / RS 改槽），含失败处理差异。

### 14-定时与队列平衡

- 一句话定位：回答"5 秒一次的自持循环把谁拉回来、靠什么续命、将来换策略动哪里"。
- 讲什么：K-095、K-096、K-097、K-098、K-099、K-100、K-101、K-086（对照引用）。
- 不讲什么：装闹钟发生在启动时（→ 02）；局部下发的掩码定义（→ 08）；降级的触发者（→ 09/12）。
- 前置：02、06、08。
- 后置：18。
- 事实底线：`schedule.c:16-18,334-343,353-369`；`main.c:47-49`；`sys_setalarm.c`；Rust `balancer.rs:23-73`、`server.rs:218-240`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-095 | 超时三个数 | 常量 | `schedule.c:16-18,338` | 主干输入 | 存：旧 11 §2.1 |
| K-096 | 自持循环 | 机制 | `schedule.c:340,367` | 本篇主干 | 存：旧 11 §1.3 |
| K-097 | 恢复 -1 | 机制 | `schedule.c:358-365` | 策略本体 | 存：旧 11 §2.3 |
| K-098 | 降级/恢复不对称 | 概念 | `schedule.c:99` vs `:360` | 需要对照 | 存：旧 11 §1.2 |
| K-099 | 只走局部下发 | 机制 | `schedule.c:32-33,362` | 边界 | 存：旧 11 §1.4 |
| K-100 | 策略可换点 | 架构演进 | `schedule.c:348-352` | 将来 | 存：旧 11 §1.5 |
| K-101 | Balancer 值对象 | 架构演进 | `balancer.rs:23,38,73` | Rust 侧 | 存：旧 11 §3 |

- 验收标准：① 一张"装闹钟—响铃—扫表—下发—再装闹钟"的环图，标注失败 panic 的两处；② 能回答"一个被降到 15 的进程多久回到上限"（给出推导）；③ 明确策略替换的落点（`Balancer` / `rebalance_one`）。

### 15-客户端面：libsys 的选路与三种包

- 一句话定位：回答"申请方怎么选路、组装什么、回复里的调度器为什么要照收"。
- 讲什么：K-105、K-106、K-107、K-108、K-051（引用）、K-052（引用）。
- 不讲什么：PM 侧的生命周期（→ 16）；RS 侧的两断言与配置来源（→ 17、`03-stage-rs/08`）；服务端检查（→ 05/11）；`_taskcall` 陷入机制（→ `01-stage-kernel/13`）。
- 前置：03、05、07。
- 后置：16、17。
- 事实底线：`libsys/sched_start.c:11-98`；`libsys/sched_stop.c:9-30`；Rust `client.rs:31-199`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-105 | 三路选路 | 机制 | `sched_start.c:57-77` | 本篇主干 | 存：旧 13 §1.2 |
| K-106 | 继承四参数 + 回复照收 | 机制 | `sched_start.c:11-41` | 本篇主干 | 存：旧 13 §1.3-1.4 |
| K-107 | 停止两条短路 | 机制 | `sched_stop.c:16-17` | 对称的一半 | 存：旧 13 §1.2 |
| K-108 | Rust 三个包 | 架构演进 | `client.rs:118,151,178` | 类型落点 | 存：旧 13 §D2 |

- 验收标准：① 三路决策表（调度器身份 / 走哪条 / 是否发消息 / 回复写谁）；② 明确"回复里的调度器可能不是你请求的那一个"并给 `sched_start.c:34-39` 证据；③ 三个 Rust 包与三个 C 函数的一一对应表。

### 16-对端一：PM

- 一句话定位：回答"用户进程的生、变、死在 PM 侧分别在什么时候发起"。
- 讲什么：K-109、K-110、K-111、K-112。
- 不讲什么：`nice↔queue` 推导与 `getpriority` 权限（→ `04-stage-pm/16-scheduling.md`）；服务端处理（→ 11/12/13）；客户端线格式（→ 15）。
- 前置：15、11、12、13。
- 后置：17。
- 事实底线：`pm/schedule.c:20-112`；`pm/main.c:241,373`；`pm/forkexit.c:425-441`；`pm/misc.c:265-290`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-109 | sched_init 只接管 INIT | 机制 | `pm/schedule.c:20-50` | 生 | 存：旧 13 §2.4 |
| K-110 | fork 后继承（PRIV_PROC → INIT） | 机制 | `pm/schedule.c:55-84`；`pm/main.c:373` | 生 | 存：旧 13 §2.4 |
| K-111 | sched_nice 与权限 | 机制 | `pm/schedule.c:89-112`；`pm/misc.c:265-290` | 变 | 存：旧 13 §2.4 |
| K-112 | exit 的停止时序 | 机制 | `pm/forkexit.c:425-441` | 死 | 存：旧 13 §2.5 |

- 验收标准：① 生命周期三点表（事件 / PM 函数 / 发什么消息 / SCHED 哪一篇处理 / 失败后果）；② 明确"fork 失败要拆掉子进程，且先置 `mp_scheduler = NONE` 再停止"的次序；③ 与 `04-stage-pm/16-scheduling.md` 的分界线写进"不讲什么"。

### 17-对端二：RS

- 一句话定位：回答"系统进程的申请为什么申请先行、两断言是什么、取消为什么两种处理"。
- 讲什么：K-113、K-114、K-115、K-116（边界）、K-117（边界）。
- 不讲什么：槽配置装载（→ `03-stage-rs/08-rs-slot-config.md`）；Live Update 全流程（→ `03-stage-rs/16-rs-live-update.md`）；服务端检查（→ 11）；客户端线格式（→ 15）。
- 前置：15、16。
- 后置：90。
- 事实底线：`rs/utility.c:364-382`；`rs/type.h:92`；`rs/main.c:320-322,376`；`rs/manager.c:461`；`rs/request.c:342`；Rust `os/servers/rs/src/sched.rs`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-113 | 两断言 | 约束 | `rs/utility.c:370-372` | 本篇主干 | 存：旧 14 §1.2 |
| K-114 | 六参数直达、父亲恒 RS | 机制 | `rs/utility.c:375-377` | 本篇主干 | 存：旧 14 §1.3 |
| K-115 | 取消两处 | 机制 | `manager.c:461`；`request.c:342` | 同样错两种处理 | 存：旧 14 §1.4 |
| K-116 | 槽配置归 03-stage-rs | 边界 | `rs/main.c:320-322` | 划边界 | 存：旧 14 分工声明 |
| K-117 | Live Update 归 03-stage-rs/16 | 边界 | `03-stage-rs/16-rs-live-update.md` | 划边界 | 存：旧 14 分工声明 |

- 验收标准：① 与 PM 的对照表（生的地方 / 时机 / 参数来源 / 父亲 / 是否有继承）；② 明确"父亲栏是系统进程的证明"；③ 取消两处的差异说明（清理时无位可保，改槽时槽位未动）；④ 两个出站引用的目标文件存在性验证命令写进参见。

### 18-Rust 实现导览与行为差异

- 一句话定位：给读代码的人一张分层地图、一份 ARCH 决策总表、一份"与 C 哪里不一样"的清单。
- 讲什么：K-120、K-121、K-122、K-123、K-124、K-125、K-126、K-127、K-133、K-078（引用）。
- 不讲什么：任何机制的教学（→ 01~17）；构建与测试命令（→ 19）；对照（→ 90）。
- 前置：02、03、08、10、11、14。
- 后置：19。
- 事实底线：`os/servers/sched/src/lib.rs:8-20`、`server.rs:45-161`、`kernel_api/transport.rs:50,127`、`main.rs:20-56`、`sef.rs:46`、各模块符号表（见 0.4 的 `grep` 输出）。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-120 | 模块地图 | 架构 | `lib.rs:8-20` | 本篇主干 | 存（分散在各篇 §4.1） |
| K-121 | 双 seam | 架构 | `transport.rs:50,127` | 本篇主干 | 存：旧 02 §D9 |
| K-122 | 单一所有者 | 架构 | `server.rs:79` | 本篇主干 | 存：旧 02 §D8 |
| K-123 | run_once/Step 与失败上界 | 架构演进 | `server.rs:138,161` | 偏离点 | 存：旧 02 §D7 |
| K-124 | FreshReady 令牌 | 架构 | `sef.rs:46` | 顺序保证 | 存：旧 01 §D3 |
| K-125 | ARCH S-1~S-11 总表 | 架构演进 | 各篇 ARCH 表；`plan.md` | 缺口 G-2 | 存（分散） |
| K-126 | 纯函数清单 | 架构 | `cpu.rs:70` 等四处 | 可测性 | 存（分散） |
| K-127 | 与 C 行为差异清单 | 差异 | 见 3.5 F-5 | 缺口 G-6 | 新 |
| K-133 | main.rs 是唯一装配层 | 架构 | `main.rs:20-56` | 分层 | 存：旧 01 §D4 |

- 验收标准：① 一张模块到概念的映射表（13 个模块 / 职责 / 对应篇号）；② ARCH 决策总表一次列全（S-1~S-11，含 `plan.md` 出处）；③ 行为差异表至少 5 行（pick 重入、失败上界、SUSPEND、cpu_mask、死核表示），每行带 C 锚点与 Rust 锚点；④ 明确写出"Rust 是单线程事件循环，无锁无原子"这一约束。

### 19-构建、测试与覆盖基建

- 一句话定位：回答"C 与 Rust 两侧怎么编、怎么跑、改完怎么验收"。
- 讲什么：K-128、K-129、K-130、K-131、K-132。
- 不讲什么：机制（→ 01~17）；代码分层（→ 18）；edge 条目的技术内容（→ `edge_todo.md`）。
- 前置：18。
- 后置：90、99。
- 事实底线：`minix3/minix/servers/sched/Makefile`、`minix.service.mk`、`kernel/table.c` boot_image、`os/servers/sched/Cargo.toml`、`todo.md §0`、`tools/design-coverage-check.sh`、`edge_todo.md`。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-128 | C 构建与镜像登记 | 工程 | `servers/sched/Makefile`；`kernel/table.c` | 缺口 G-3 | 新 |
| K-129 | bin/lib 双目标 | 工程 | `os/servers/sched/Cargo.toml` | 缺口 G-4 | 新 |
| K-130 | 测试基线 | 测试 | `todo.md §0`（81 passed / clippy 0） | 纠正旧各篇的 59（F-4） | 存（数字错） |
| K-131 | 覆盖率门 | 工程 | `tools/design-coverage-check.sh` | 验收 | 存 |
| K-132 | edge 开口项 | 边界 | `edge_todo.md`；`todo.md §2` | 剩余工作 | 存 |

- 验收标准：① 给出三条可复制命令（`cargo test -p minix-sched`、`cargo clippy -p minix-sched --all-targets`、`tools/design-coverage-check.sh ...`）与期望输出；② 说明 bin/lib 双目标的原因（库可测、二进制只装配）；③ 列出 edge 开口项与本 stage 篇号的对应表。

### 90-支线：对照、已知缺陷与已声明不做

- 一句话定位：收容不适合进主线的材料，让主线保持单语义。
- 讲什么：K-140、K-141、K-142。
- 不讲什么：任何机制教学。
- 前置：01（可选，支线）。
- 后置：无。
- 事实底线：`main.c:90`；`schedule.c:77,185,302,667-678`(system.c)；`system.c:663-666`；OS 理论。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-140 | 与其它 OS 对照 | 概念 | OS 理论 | 缺口 G-11 收容 | 存（14 处） |
| K-141 | 已知缺陷清单 | 已知缺陷 | 见 3.5 | 缺口 G-5 | 新 |
| K-142 | 已声明不做 | 架构演进 | `schedule.c:185,351` | 收容 | 存 |

- 验收标准：① 缺陷清单至少 6 条，每条带锚点与"是否影响本 stage 行为"；② 对照部分明确只保留四处正文对照（01/06/10/14），其余在此收容；③ 声明本篇可跳读。

### 99-全局概念与索引

- 一句话定位：一本随时查的手册页（常量 / 消息 / 错误码 / 边界）。
- 讲什么：K-143、K-144、K-021（编号表）、K-041（常量表）、K-038（错误码表）。
- 不讲什么：一切解释性内容（只给表与指针）。
- 前置：00。
- 后置：无。
- 事实底线：`config.h:66-77`；`com.h:801-807`；`ipc.h` 七处消息体；`errno` 六处；三个邻居 stage 的篇路径。
- 知识点清单：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-143 | 三张索引表 | 工程 | `config.h`、`com.h`、`errno` | 本篇本体 | 存：旧 99 骨架 |
| K-144 | 跨 stage 边界索引 | 边界 | kernel 11 / pm 16 / rs 08,16 | 本篇本体 | 存（分散） |

- 验收标准：① 常量表覆盖 `config.h:66-77` 与 `schedule.c:16-46` 的全部数值；② 五消息编号表 + 五消息体字段表；③ 六错误码表（值 / 触发 / 在哪篇讲）；④ 边界索引表列出四个邻居篇并注明"已实测存在"。

---

## 6. 变更表

| # | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---|------|--------|--------|------|-----------|----------|
| C-01 | 拆分 | 旧 00（20 行骨架） | 新 00（导航） + 新 01（模型与定位） | 骨架承载不了导航；模型需要一个自己的"为什么"篇 | K-001~K-008 | 00 的核心点 → 新 01 §概念；00 的导航职责 → 新 00 重写（原料：`draft/00`、`05-stage-vfs/00-vfs-overview.md` 写法） |
| C-02 | 重排 | 旧 01 | 新 02 | 内容基本不变，位置顺延 | K-010~K-020 | 原样搬移 + 补 K-017 |
| C-03 | 重排 | 旧 02 | 新 03 | 顺延 | K-021~K-030 | 原样搬移 + 补 K-025 |
| C-04 | 重排 | 旧 03 | 新 04 | 顺延 | K-031~K-033,K-039,K-040 | 原样搬移 + 锚点修正（F-7） |
| C-05 | 合并 | 旧 04 + 旧 06/07/08/09 里的错误码片段 | 新 05 | 错误码现在分散五处 | K-034~K-038 | 旧 04 全篇 → 新 05 主体；其余四篇的错误码句子 → 新 05 §总表，原处改为错误码名 |
| C-06 | 重排 | 旧 05 | 新 06 | 顺延 | K-041~K-050 | 原样搬移 |
| C-07 | 拆分 | 旧 12（内核接口） | 新 07（接管） + 新 09（回传与记账） | 旧 12 把两个方向（SCHED→内核的接管、内核→SCHED 的回传）混在一篇，且位置靠后被 06/08 依赖 | K-051~K-054,K-068 / K-062~K-067,K-079 | 旧 12 §1.2,2.1,2.2,2.6 → 新 07；旧 12 §1.4,1.5,1.6,2.3,2.4,2.5 → 新 09 |
| C-08 | 重排 | 旧 09（schedule_process） | 新 08 | 位置提前到处理函数之前（消除 11/12/14 对它的前向引用） | K-055~K-061,K-074 | 原样搬移 + 明确"CPU 字段"一句下放新 10 |
| C-09 | 重排+合并 | 旧 10 | 新 10 | 顺延；吸收旧 06 的重试环 | K-070~K-078 | 旧 06 §1.5,§2.7 的重试环 → 新 10 §重试环；旧 10 全篇 → 新 10 主体 |
| C-10 | 重排 | 旧 06（start） | 新 11 | 顺延；交出内核交接与重试环 | K-080~K-085,K-091 | 旧 06 §2.6 → 新 07；§1.5,§2.7 → 新 10；余下 → 新 11 |
| C-11 | 重排 | 旧 08（noquantum + nice） | 新 12 | 顺延；交出内核接口内容 | K-086~K-088 | 旧 08 §2.5（内核标记检查）→ 新 03；§2.4、§1.5（局部下发）→ 新 08；余下 → 新 12 |
| C-12 | 重排 | 旧 07（stop） | 新 13 | 按生命周期"生→变→死"排序 | K-089,K-090 | 原样搬移 |
| C-13 | 重排 | 旧 11（balance） | 新 14 | 顺延 | K-095~K-101 | 原样搬移 |
| C-14 | 拆分 | 旧 13（PM） | 新 15（libsys 客户端） + 新 16（PM 对端） | 旧 13 同时讲"库怎么发包"和"PM 什么时候发包"，两个语义 | K-105~K-108 / K-109~K-112 | 旧 13 §1.2,1.4,2.1,2.2,2.3 → 新 15；旧 13 §1.1,1.5,1.6,2.4,2.5,2.6 → 新 16 |
| C-15 | 拆分 | 旧 14（RS） | 新 17（RS 对端） + 新 15（客户端形状） | 旧 14 §1.3 讲的是 libsys 的形状，与 PM 共用 | K-113~K-117 / K-108 | 旧 14 §1.3 → 新 15（与旧 13 的客户端内容合并）；余下 → 新 17 |
| C-16 | 新建 | — | 新 18（Rust 导览） | 缺口 G-2/G-6；每篇重复 §3/§4 需要一个总览落点 | K-120~K-127,K-133 | 来源：各旧篇 §3 的 ARCH 表、`lib.rs:8-20`、`server.rs`、`transport.rs` |
| C-17 | 新建 | — | 新 19（构建测试基建） | 缺口 G-3/G-4/G-9 | K-128~K-132 | 来源：`servers/sched/Makefile`、`Cargo.toml`、`todo.md §0`、`tools/design-coverage-check.sh`、`edge_todo.md` |
| C-18 | 新建 | — | 新 90（支线） | 缺口 G-5/G-11 | K-140~K-142 | 来源：各旧篇 §1.6/1.7 对照、3.5 缺陷清单、`schedule.c` 的 FIXME 注释 |
| C-19 | 重写 | 旧 99（20 行骨架） | 新 99（实体索引） | 骨架无内容 | K-143,K-144 | 来源：`draft/99`、各旧篇的常量/错误码片段 |
| C-20 | 归档 | `draft/00`、`draft/01`、`draft/02`、`draft/03`、`draft/04`、`draft/99` | 不进正式目录 | 已被新 00/04/11/12/99 取料取代 | — | B 相取料后归档（不删） |

---

## 7. 缺漏新篇（非 C 主题逐项落实）

| 主题 | 为什么重要 | 原料在哪 | 归哪一篇 | 验收标准 |
|------|-----------|---------|---------|---------|
| 链接与加载 | 读者不知道 SCHED 是怎么进内存的 | `servers/sched/Makefile`、`minix.service.mk`、`kernel/table.c` | 新 19 | 能说出 SCHED 的三个源文件与它在 boot_image 里的位置 |
| 镜像与内存布局 | 决定 SCHED 与其它服务的启动先后 | `kernel/table.c`；`00-master-plan/README.md:40` | 新 01（一句）+ 新 19 | 能说出 SCHED 排在 ds/rs/pm 之后 |
| 汇编入口与陷阱进入 | 申请方发消息要陷入内核 | `01-stage-kernel/13-syscall-dispatch.md` | **不在本 stage**（→ 01-stage-kernel/13） | 新 15 写明"`_taskcall` 的陷入机制不在本 stage" |
| 启动装配 | 三处分散（C 的 sef、内核的登记、Rust 的 bin） | `main.c:32`；`rs/main.c:376`；`main.rs:20-56` | 新 01 + 新 02 + 新 19 | 三篇各讲一层，无重复 |
| 构建与工具链 | 改完不知道怎么编 | `Makefile`；`Cargo.toml` | 新 19 | 两条构建命令可复制 |
| 跨模块接口与线格式 | SCHED 的核心资产 | `ipc.h` 七处；`minix-types` | 新 03 + 新 15 + 新 99 | 五消息体的字段表齐全 |
| 错误路径 | 分散五处 | `utility.c`、`system.c` | 新 05（总表）+ 各处理篇 | 六错误码全覆盖 |
| 关闭与退出 | 三条来路 | `pm/forkexit.c:425`；`rs/manager.c:461`；`rs/request.c:342` | 新 13 + 新 16 + 新 17 | 三条来路表 |
| 并发与同步 | 读者会误用内核标尺 | AGENTS.md；`server.rs` | 新 18 | 明确写出单线程、无锁 |
| 测试基建 | 改完不知道怎么验收 | `todo.md §0`；`tools/design-coverage-check.sh` | 新 19 | 三条命令 + 期望输出 |

---

## 8. 锚点迁移与断链成本

### 8.1 旧编号 → 新编号映射（批量替换表）

| 旧 | 新 | 旧 | 新 |
|---|---|---|---|
| `00-sched-overview.md` | `00-sched-overview.md`（**文件名不变**） | `08-noquantum-nice.md` | `12-noquantum-set-nice.md` |
| `01-sched-init-main.md` | `02-sched-startup-main-loop.md` | `09-schedule-process.md` | `08-schedule-downlink.md` |
| `02-sched-message-surface.md` | `03-sched-message-surface.md` | `10-pick-cpu-smp.md` | `10-pick-cpu-smp.md`（**文件名不变**） |
| `03-schedproc-struct.md` | `04-schedproc-struct.md` | `11-balance-queues.md` | `14-balance-queues.md` |
| `04-schedproc-table.md` | `05-schedproc-table-gates.md` | `12-kernel-interface.md` | `07-kernel-takeover.md` + `09-kernel-noquantum.md` |
| `05-priority-timeslice-model.md` | `06-priority-timeslice-model.md` | `13-pm-interaction.md` | `15-sched-client-libsys.md` + `16-pm-interaction.md` |
| `06-start-scheduling.md` | `11-start-scheduling.md` | `14-rs-interaction.md` | `17-rs-interaction.md` |
| `07-stop-scheduling.md` | `13-stop-scheduling.md` | `99-global-concepts.md` | `99-global-concepts.md`（**文件名不变**） |

> 建议：**文件名带编号**，编号即顺序（与现有约定一致），文件名随编号变。三个文件名不变的篇（00/10/99）可直接就地改写。

### 8.2 锚点迁移表（按旧篇逐节）

| 旧位置 | 旧内容（一句话） | 新位置 | 迁移类型 | 断链风险 |
|---|---|---|---|---|
| 01 §1.1 | 为什么策略在用户态 | 新 01 §1 | 改写 | 低（概念搬家，无外部引用） |
| 01 §1.2 | fresh/restart 注册 | 新 02 §1 | 原样 | 低 |
| 01 §1.3 | 读机器信息 + 装闹钟 | 新 02 §1（读）/ 新 14 §2（装） | 拆分 | 中（闹钟节搬家，01 §1.3 的引用需改指向） |
| 01 §1.4 | 主循环骨架 | 新 02 §1 | 原样 | 低 |
| 01 §1.5 | SUSPEND 约定 | 新 02 §1（补死分支事实） | 改写 | 低 |
| 01 §2.1~2.8 | C 源码逐段 | 新 02 §2 | 原样 | 低 |
| 01 §D1~D5 / ARCH 表 | Rust 决策 | 新 02 §3（精简）+ 新 18 §总表 | 拆分 | 中 |
| 01 §5.1 | 测试统计（59） | 新 02 §5（改 81，来源新 19） | 改写 | 低 |
| 02 §1.2 | 五种消息总表 | 新 03 §1 | 原样 | 低 |
| 02 §1.4 | 耗尽消息来源校验 | 新 03 §1 | 原样 + 补 K-025 | 低 |
| 02 §2.7 | 七个消息体位置 | 新 03 §2 + 新 99 | 合并 | 低 |
| 03 §2.2~2.7 | 字段锚点（`CONFIG_MAX_CPUS（L24）` 污染形式） | 新 04 §2（改行号锚点） | 改写 | 中（锚点形式必须全换，否则 Gate E 不通过） |
| 04 §2.4 | 错误码 | 新 05 §2（扩成总表） | 合并 | 中（另四处需同步删） |
| 05 §1.5 / §2.4 | nice 换算 | 新 06 §1（结论）+ 指向 `04-stage-pm/16` | 改写 | 低（出站引用保留） |
| 05 §2.5 | niced 与内核检查 | 新 06 §2（端到端） | 改写 | 低 |
| 06 §1.3 / §2.3 | init 临时值与覆盖 | 新 11 §1 / §2 | 原样 | 低 |
| 06 §1.5 / §2.7 | 重试环 | 新 10 §重试环 | 拆分 | **高**（此节是 06 与 10 的双向引用点，两处都要改） |
| 06 §2.6 | 交接与置标记 | 新 07 §2（内核侧）+ 新 11 §2（调用侧） | 拆分 | 中 |
| 07 全篇 | STOP | 新 13 | 原样 | 低 |
| 08 §1.5 / §2.4 | 局部下发的名字 | 新 08 §1 | 合并 | 低 |
| 08 §2.5 | 内核标记检查 | 新 03 §1 | 拆分 | 中 |
| 08 §2.1~2.3 | 降级 / 改上限 / 回滚 | 新 12 §1~§2 | 原样 | 低 |
| 09 全篇 | schedule_process | 新 08 | 原样 + 补 K-074 | 低 |
| 10 §1.3~1.4 | 台账与死亡写法 | 新 10 §1（补 K-075/K-076） | 改写 | 中 |
| 11 全篇 | balance_queues | 新 14 | 原样 | 低 |
| 12 §1.2 / §2.1~2.2 / §2.6 | 接管与归属 | 新 07 | 拆分 | **高**（12 篇被 06/08 两篇引用） |
| 12 §1.4~1.6 / §2.3~2.5 | 回传、七字段、抢占分支 | 新 09 | 拆分 | **高** |
| 13 §1.2~1.4 / §2.1~2.3 | 客户端选路与包 | 新 15 | 拆分 | 中 |
| 13 §1.1 / §1.5~1.6 / §2.4~2.6 | PM 生命周期 | 新 16 | 拆分 | 中 |
| 14 §1.3 | 六参数直达（客户端形状） | 新 15 §2 | 拆分 | 中 |
| 14 §1.1~1.2 / §1.4~1.6 / §2 | RS 申请与取消 | 新 17 | 原样 | 低 |
| 各篇 §1.6/1.7 对照 | 与其它 OS 对照（14 处） | 新 90（保留 4 处） | 合并 | 中 |
| 各篇 §4.1 模块结构 | 模块地图（14 处） | 新 18 §1 | 合并 | 中 |
| 99 骨架 | 四行核心点 | 新 99（三张表 + 边界索引） | 重写 | 低 |

### 8.3 引用迁移表

| 旧引用位置 | 引用了什么 | 新目标 | 验证方式 |
|---|---|---|---|
| `os/servers/sched/src/main.rs:4` | `06-stage-sched/01-sched-init-main.md` | `06-stage-sched/02-sched-startup-main-loop.md` | `grep -n "06-stage-sched" os/servers/sched/src/main.rs` |
| `os/servers/sched/src/lib.rs:8` 及 12 处模块注释 | `01-sched-init-main.md` 与 01~14 篇名（12 处） | 按 8.1 映射表逐个替换 | `grep -rn "0[0-9]-[a-z-]*\.md" os/servers/sched/src/` 逐个核对 |
| `os/servers/sched/src/*.rs` 各模块头 | 篇名（如 `schedproc.rs:4` 引 `03-schedproc-struct.md` → `04`） | 同上 | 同上 |
| `os/servers/rs/src/boot.rs:123` | 只提 `06-stage-sched`（无篇名） | 不变 | 无需改动 |
| `07-stage-ds/doc_rerank_qwen.md:24`、`doc_rerank_glm.md:29` | `../06-stage-sched/00-sched-overview.md` | 不变（文件名不变） | 零断链 |
| `edge3.md:63,87` | `06-stage-sched/todo.md` | 不变 | 零断链 |
| `07-stage-ds/todo.md:187` | `06-stage-sched/todo.md V2 §4.1` | 不变 | 零断链 |
| 本目录 15 篇之间的约 222 处交叉引用 | 篇名 | 按 8.1 映射表批量替换 | `rg -o "[0-9][0-9]-[a-z0-9-]*\.md" 06-stage-sched/*.md \| sort \| uniq -c` 替换后应无旧名残留 |
| `.review/codex/sched/*/scan.md` 等 | 篇名（review 中间产物） | **建议不迁移**（review 快照是历史记录） | 在 00 篇脚注说明"旧编号见 8.1 映射表" |
| 旧 13/14 的出站引用 | `../04-stage-pm/16-scheduling.md`、`../03-stage-rs/08-rs-slot-config.md`、`../03-stage-rs/16-rs-live-update.md` | **保持不变** | `ls` 三个目标（已实测存在） |

### 8.4 断链成本摘要

- **内部交叉引用**：约 222 处（热点：旧 05=23、旧 06=19、旧 02=18、旧 07/14=17、旧 03=16）。全部是篇名字符串，可用 8.1 的 15 行映射表做一次脚本替换，替换后用 `rg` 断言"旧篇名计数为零"。
- **代码注释**：`os/servers/sched/src` 内约 15 处（`main.rs:4`、`lib.rs:8` + 13 个模块头），逐个替换即可。
- **外部正式文档**：**零**。外部只引用 `00-sched-overview.md`（文件名不变）与 `todo.md`（不变），没有任何一篇外部文档引用 01~14 的篇名。
- **review 中间产物**：`.review/codex/sched/*` 引用旧编号，建议不迁移（历史快照），只在 00 篇留映射脚注。
- **总判定**：断链成本 = 一次脚本替换 + 15 处代码注释，属于"看清了成本、成本可接受"，重建可以开工。

---

## 9. 验证与自检门

### 9.1 四种机械检查

| 检查 | 做法 | 结果 |
|---|---|---|
| 前向引用扫描 | 逐篇读契约的"前置"字段，断言只指向更小编号 | **通过**：00(无) / 01(00) / 02(00,01) / 03(01,02) / 04(01,03) / 05(03,04) / 06(04,05) / 07(06) / 08(05,06,07) / 09(07,08) / 10(02,06,08) / 11(05,06,07,08,10) / 12(03,05,06,08,09) / 13(05,08,11) / 14(02,06,08) / 15(03,05,07) / 16(15,11,12,13) / 17(15,16) / 18(02,03,08,10,11,14) / 19(18) / 90(01) / 99(00) —— 全部指向更早编号 |
| 依赖关系图无环 | 由上述前置关系构图 | **通过**（编号单调，天然无环） |
| 覆盖率 100% | 118 个知识点逐条落进某篇契约的"知识点清单" | **通过**：A=8、B=11、C=10、D=6、E=9、F=17、G=10、H=11、I=7、J=13、K=14、L=5，合计 121 条次（3 条被两篇共享引用：K-008、K-025、K-074），118 条全覆盖；无删除项 |
| 断链成本统计 | 见 8.4 | **通过**（外部零断链） |

### 9.2 自检门 G1~G9

| 门 | 检查内容 | 结果 |
|---|---|---|
| G1 | C 真序逐条可核对（抽十条核锚点） | **通过**。抽查：`main.c:39`(L1)、`main.c:45`(L3)、`main.c:47-49`(L4)、`main.c:70-71`(L9)、`main.c:90`(L12)、`schedule.c:130`(L?/K-071)、`schedule.c:218`(步骤5)、`schedule.c:302`(K-074)、`proc.c:1874`(K-025)、`system.c:654`(K-061) —— 均已读原文核对 |
| G2 | 每个 C 文件、每个非 C 制品有归属或排除理由 | **通过**。6 个 C 文件全覆盖（main.c→02/03、schedule.c→07/08/10/11/12/13/14、utility.c→03/05、schedproc.h→04、sched.h/proto.h→00/99）；非 C 制品十项见 3.4，全部有落点或"不在本 stage"理由 |
| G3 | 新目录前向引用为零 | **通过**（见 9.1 第一项） |
| G4 | 依赖图无环 | **通过** |
| G5 | 覆盖率 100%，新增条目有证据锚点，删除项单列 | **通过**。22 条新增全部带锚点（K-005、K-017、K-025、K-066、K-067、K-068、K-074、K-075、K-076、K-078、K-120~K-129 等）；**无删除项**（全部知识点都有去向） |
| G6 | 拆分/合并写清存量去向，新建写清来源（抽查十处） | **通过**。抽查 C-05（错误码四处合并）、C-07（旧 12 拆两篇，逐节标注）、C-09、C-10、C-11、C-14、C-15（旧 13/14 三处互拆）、C-16、C-17、C-18 —— 每处都写了来源或去向 |
| G7 | 每篇契约七要素齐全 | **通过**。22 篇均含：一句话定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 + 验收标准 |
| G8 | 锚点迁移表覆盖所有变化文档的每一节；引用迁移表覆盖文档与代码注释 | **通过**。8.2 覆盖 15 篇的全部实质小节；8.3 覆盖代码注释 2 处 + 外部文档 4 处 + 批量替换 1 类 + 保持不变 3 类 |
| G9 | 事实断言有锚点；推测项已标注 | **通过**。所有 C/Rust 断言带 `文件:行`；唯一推测项 F-2（重试环有界性）已显式标注"推测，待验证"并给出复核命令 |

### 9.3 结论

**蓝图完成**，可直接进入 B 相。三项需在 B 相开工前处理的事项：

1. **F-2 必须先验证**：`sed -n '46p;77p;226,231p' minix3/minix/servers/sched/schedule.c`，按 C 无符号语义确认重试环是否有界；结论写进新 10，并同步改掉旧 06 §1.5 的"必定退出"断言。
2. **F-7 锚点污染**：旧 03 篇的 `schedproc.h:CONFIG_MAX_CPUS（L24，工具生成）` 形式锚点必须全部替换为行号锚点，否则 Gate E 不通过。
3. **F-4 测试基线**：新 19 落地后，各篇 §5 的数字统一以实测为准（不得沿用 59）。

### 9.4 待用户裁决的问题

| # | 问题 | 我的建议 |
|---|------|---------|
| Q-1 | 文件名是否随编号全改（8.1 方案）还是只改编号不改文件名 | 建议**随编号改**（编号即顺序，混用会让"编号=顺序"的约定失效）；00/10/99 三篇编号不变，文件名自然也不变 |
| Q-2 | 每篇是否保留"与其它 OS 的对照"小节 | 建议**只保留 4 处**（01/06/10/14），其余收进 90；这是对"单篇单语义"的让步最小方案 |
| Q-3 | 每篇是否保留"Rust 设计决策"与"实现详解"两节 | 建议**保留但瘦身**：每篇最多 4 条 D、ARCH 总表归 18、模块地图归 18 |
| Q-4 | 新 18/19 是否算"正式编号篇"还是应放 90 系列 | 建议**正式编号**（18/19），因为它们是读者动手前的必经路径，不是可跳读材料 |
