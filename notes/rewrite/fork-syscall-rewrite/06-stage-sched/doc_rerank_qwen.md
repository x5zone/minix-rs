# 06-stage-sched 文档重建蓝图（qwen）

## 0. 元数据

```text
执行者     = qwen
日期       = 2026-09-19
目标目录   = notes/rewrite/fork-syscall-rewrite/06-stage-sched/
仓库根目录 = /home/xzhao/github/minix-rs
当前提交号 = 606e97607（git rev-parse --short=9 HEAD）
任务       = R 相·重建蓝图：只产出本文件，不改任何正文。
```

### 0.1 审查范围

- **算文档**：编号文档 00-14 共 15 篇 + `99-global-concepts.md`（全部逐篇通读；00 与 99 是 20 行 pending 骨架）。
- **算参考材料**：`plan.md`（383 行，现行目录契约与 ARCH 表）、`todo.md`（60 行，两轮 review 闭环指针）、`draft/`（6 份旧草稿：00/99 + 01-sched-struct / 02-sched-inherit / 03-sched-start / 04-sched-quantum）、`archive/`（两轮 review 归档）。参考材料不入知识点池的正文来源，只作线索。
- **范围外**：`doc_rerank_deepseek.md`、`doc_rerank_glm.md`（其它 AI 产物，按任务纪律未读、未引用）。`.design/`、`tmp_design_and_todo/` 未引用。
- **范围外发现**（单列）：内核侧队列机制的展开讲述归 `../01-stage-kernel/11-scheduling-primitives.md`（其 §2.6 proc_no_time、§3.8/§4.6 sched_proc、§4.5 notify 联动已教过）；RS 槽配置与 Live Update 归 `../03-stage-rs/08-rs-slot-config.md`、`../03-stage-rs/16-rs-live-update.md`；PM 的 nice 系统调用用户面归 `../04-stage-pm/16-scheduling.md`（旧 13/14 篇头已有主权分工声明，本蓝图保留）。

### 0.2 读取清单

| 类别 | 内容 |
|------|------|
| 本 stage 文档 | 00-14、99 全部 + plan.md、todo.md、draft/ 目录清单 |
| C 源码（服务器） | `minix3/minix/servers/sched/`：main.c(138)、schedule.c(370)、utility.c(75)、schedproc.h(40)、sched.h(18)、proto.h(21)、Makefile(9) —— 全文通读，关键函数逐行核对 |
| C 源码（内核对端） | `kernel/system/do_schedctl.c`(47)、`kernel/system/do_schedule.c`(31)、`kernel/system.c:630-730`（sched_proc 实为 642-700）、`kernel/proc.c:1850-1915`（notify_scheduler 1860-1891、proc_no_time 1893-1910） |
| C 源码（客户端） | `lib/libsys/sched_start.c`(99)、`sched_stop.c`(31)、`servers/pm/schedule.c`(113)、`servers/pm/utility.c:91-101`（nice_to_priority）、`servers/pm/forkexit.c:102/425/441`、`servers/pm/main.c:199/213/372-380`、`servers/rs/utility.c`（sched_init_proc ~363-381）、`rs/manager.c:461`、`rs/request.c:342`、`rs/type.h:92-95` |
| 常量与线格式 | `minix3/include/minix/com.h`（61/63/92/449/801-807/1151）、`minix3/include/minix/config.h:66-77`、`minix/include/minix/ipc.h:2568-2612`（消息 union 成员）与 1440-1444/1822-1827（结构体）、`minix/include/minix/ipcconst.h:28-30`（IPC_FLG_MSG_FROM_KERNEL）、`minix/kernel/ipc.h:12`（FROM_KERNEL 0x0100）、`minix/include/minix/const.h:143` + `minix/include/minix/priv.h:45-50`（PREEMPTIBLE 及其模板） |
| 非 C 制品 | sched `Makefile`（链接 libsys）、`minix3/minix/kernel/table.c:56`（boot_image 登记 `"sched"`）、Rust 构建 `os/servers/sched/Cargo.toml` + `src/`（21 文件 4525 行）、`os/qemu-tests/`（无 sched 场景）、`os/libs/minix-sys`（DirectTrapTransport/DirectKernelCallTransport 现状） |
| 边界材料 | `00-master-plan/README.md`（启动因果链：SCHED=06 stage，RS 加载）、`edge_todo.md` E5(e)(194)、E8(258-270)、E-SCHEDNICED(439-458)、E-PREEMPTFLAG(462-483)、E-SCHEDSMP(487-501)、E-MINTYPES-SYS(505-524)、`edge1.md` K1/K2（2026-09-18 已闭单）、`edge3.md` S27/S41 |
| 前 stage 对照 | `05-stage-vfs/00-vfs-overview.md`（组织范式：先总后分、禁止前向引用、单篇单语义）；`01-stage-kernel/11-scheduling-primitives.md` 标题结构（内核对端已教内容） |
| 写法范例 | `01-stage-kernel/06-todo.md`（只借契约写法，不搬结论） |

### 0.3 使用的命令与关键输出（证据摘录）

```text
$ wc -l notes/.../06-stage-sched/*.md
00:20  01:209  02:273  03:208  04:191  05:218  06:249  07:213  08:208  09:220
10:197  11:204  12:214  13:212  14:217  99:20   plan:383  todo:60

$ grep -o "第 NN 篇|`NN-xxx" *.md | wc -l        # stage 内部引用
307

$ grep -rn "<doc-stem>" os/servers/sched/src     # Rust 源码注释对文档的引用
21 处（见 §8.2 引用迁移表，全部是模块头的 owned-by 声明）

$ grep -n "sef_receive_status" minix3/minix/servers/sched/main.c
39:  if (sef_receive_status(ANY, &m_in, &ipc_status) != OK)   # 接收不限来源

$ sed -n '45,55p' main.c      # CLOCK 通知 → balance_queues() 无条件调用
$ sed -n '46p'  schedule.c    # static unsigned cpu_proc[CONFIG_MAX_CPUS];（零初始化，无显式初值）
$ sed -n '262,263p' schedule.c # do_nice 有 accept_message（信任检查在 handler 内，Rust 挪到外层是设计变更）
```

锚点漂移实测（旧文/plan 与 C 源码不符，重建时以实测为准）：

| 旧断言 | 实测 |
|--------|------|
| plan.md：主循环 `main.c:52-98` | 循环体是 `main.c:35-94`（while 在 35，收信在 39） |
| plan.md：`system.c:642-723` sched_proc | sched_proc 实际 642-700 |
| plan.md：do_schedctl.c 49 行 | 实际 47 行 |
| 旧 03/04 等篇 "（工具生成）" 锚点，如 `schedproc.h:CONFIG_MAX_CPUS（L36）` | L36 是 schedproc 表声明，不是 CONFIG_MAX_CPUS（宏在 L14-16）；同类工具噪声多处 |
| 旧 12 篇 PREEMPTIBLE 引 `priv.h:45-50` | 内容属实，但完整路径是 `minix3/minix/include/minix/priv.h:45-50`（位定义在 `const.h:143`）；本蓝图已复核并补全双锚点 |

---

## 1. C 真序

**阶段类型判定**：本 stage 是标准的**服务事件循环型**（prompt §九第二条）。SCHED 是一个用户态服务器进程：没有线性启动链之外的分支，核心内容是一次请求的生命周期和五种消息的处理；时间片账本、CPU 台账这些机制都挂在"消息到达"这个触发点上。因此真序分两段：启动段（进程诞生 → 进入循环）与循环段（接收 → 分派 → 处理 → 回复），另加一节"触发时机"（谁在什么运行时刻发出这五种消息）。

### 1.1 启动段（T 表）

| # | 动作 | C 函数与锚点 | 说明 |
|---|------|-------------|------|
| T-01 | 内核 boot 期把 sched 登记进启动表 | `minix3/minix/kernel/table.c:56` `{SCHED_PROC_NR, "sched"}` | 槽位序号 4（SCHED_PROC_NR=4，`minix3/include/minix/com.h:63`）；真正执行加载的是 RS（归 03-stage-rs） |
| T-02 | sched 进程入口 | `servers/sched/main.c:22-32` `main()` | 局部变量 message/call_nr/who_e/result，第一件事是 `sef_local_startup()`（:32） |
| T-03 | SEF 注册生命周期回调 | `main.c:111-121` `sef_local_startup` | 注册 `sef_cb_init`（指向 sef_cb_init_fresh）与 `SEF_CB_INIT_RESTART_STATEFUL`（live-update 时保留状态的回调；本 stage 无状态保留需求，走 fresh 路） |
| T-04 | fresh 初始化：读机器信息 | `main.c:126-136`；`sys_getmachine(&machine)`（:130），失败 `panic`（:131） | machine 结构提供 boot CPU 与 CPU 数（后续选核依赖，`pick_cpu` 用）；收信失败同样 panic（:40）——启动约定被破坏就崩溃，是本服务的错误策略基线 |
| T-05 | fresh 初始化：设第一轮平衡闹钟 | `main.c:133` → `schedule.c:334-342` `init_scheduling` | `balance_timeout = BALANCE_TIMEOUT * sys_hz()`（:338，BALANCE_TIMEOUT=5 秒，:18）；`sys_setalarm(balance_timeout, 0)`（:340），失败 panic |
| T-06 | 进入主循环 | `main.c:35` `while (TRUE)` | :95 的 `return(OK)` 实际不可达——SCHED 没有正常退出路径 |

### 1.2 循环段（L 表，一轮一步）

| # | 动作 | C 函数与锚点 | 说明 |
|---|------|-------------|------|
| L-01 | 阻塞等待任意来源消息 | `main.c:39-42` `sef_receive_status(ANY, &m_in, &ipc_status)` | 收信失败 panic；取 `who_e = m_source`（:41）、`call_nr = m_type`（:42）。注意：接收侧**不限来源**，信任判定全部后置 |
| L-02 | 通知分流 | `main.c:45-55`；`is_ipc_notify` 宏 `com.h:92` | 是通知：`who_e == CLOCK` 就**无条件**调 `balance_queues()`（:47-49），其余通知忽略（:50-52），然后 `continue` 不回复（:54）。节奏由闹钟本身维持（见 L-06），循环里没有超时判断 |
| L-03 | 调用分发 | `main.c:57-87` switch(call_nr) | `SCHEDULING_INHERIT` 与 `SCHEDULING_START` **共用入口** case 贯穿（:58-61）；STOP（:62-64）、SET_NICE（:65-67）各归各 handler |
| L-04 | NO_QUANTUM 信任门 | `main.c:68-84`；`IPC_STATUS_FLAGS_TEST(ipc_status, IPC_FLG_MSG_FROM_KERNEL)`（:70-71，位定义 `minix/include/minix/ipcconst.h:28-30`） | 带内核旗标：调 `do_noquantum`，失败**只 printf 警告**（:72-75），无论成败 `continue` 不回复（:76）；不带旗标：printf 揭穿"伪造 NO_QUANTUM"（:79-81），`result = EPERM` 走回复（:82-83） |
| L-05 | 未知调用号拒收 | `main.c:85-87` → `utility.c:18-23` `no_sys` | 返回 ENOSYS（并 printf），照常回复 |
| L-06 | 回复规则 | `main.c:90-93`；`reply()` `main.c:101-106` | `result != SUSPEND`（SUSPEND=-998，`com.h:1151`）才把 result 写回 `m_type` 并回给 `who_e`；SUSPEND 是 handler 声明"这次挂起、以后另回"的约定——当前五个 handler 都不用它，但约定是 SEF 服务共通的 |

### 1.3 各 handler 的内部真序（H 表）

| handler | 真序（逐步，锚点 `servers/sched/schedule.c`） |
|---------|-----------------------------------------------|
| do_start_scheduling（START/INHERIT 双入口） | ① 类型断言（:145-147）→ ② `accept_message` 白名单（:149-151 → utility.c:61-74）→ ③ `sched_isemtyendpt` 取空槽（:153-157）→ ④ 先填 endpoint/parent/max_priority 三字段（:160-163），**再**做 `max_priority >= NR_SCHED_QUEUES` 边界检查（:164-166，检查发生在部分写入之后）→ ⑤ init 自父特例：endpoint==parent 时填 USER_Q/DEFAULT_USER_TIME_SLICE（+SMP 下 cpu=bsp_id）（:168-187）→ ⑥ 按消息分叉：START 显式给值 priority=max_priority、time_slice=quantum（:191-197，**覆盖**⑤的临时值）；INHERIT 再走 `sched_isokendpt(parent)` 后从父槽抄 priority/time_slice（:199-209）→ ⑦ `sys_schedctl(0, endpoint, 0,0,0)` 向内核要调度权（:216-222），成功才 `flags = IN_USE`（:223）→ ⑧ `pick_cpu`（:226）+ `schedule_process(SCHEDULE_CHANGE_ALL)`，返回 EBADCPU 就把该核记死 `cpu_proc[cpu]=CPU_DEAD` 重选重试（:227-231），其他错误 printf 后带码返回（:233-237）→ ⑨ 回复里写 `scheduler = SCHED_PROC_NR`（:239-246） |
| do_stop_scheduling | ① accept_message（:117-119）→ ② `sched_isokendpt(消息里的 endpoint)`，失败 EBADEPT（:121-126）→ ③ SMP 编译下 `cpu_proc[rmp->cpu]--`（:129-131，单核不编入）→ ④ `rmp->flags = 0` 整字清零（:132，注释保留 `&= ~IN_USE` 痕迹）→ ⑤ 返回 OK。**不通知内核**：进程由内核保证消亡或换调度器（见 K-102） |
| do_noquantum | ① 校验 `m_source`（谁被扣光时间片）`sched_isokendpt`，失败 EBADEPT（:92-96）→ ② `priority < MIN_USER_Q` 则 `priority += 1` 降一级（:98-101，封顶第 15 队列）→ ③ `schedule_process_local`（宏 = PRIO|QUANTUM 掩码，:32-33）下发（:103-105）。**无 accept_message**：信任由 L-04 的内核旗标完成 |
| do_nice | ① accept_message（:261-263）→ ② `sched_isokendpt(endpoint)` 失败 EBADEPT（:265-269）→ ③ `maxprio >= NR_SCHED_QUEUES` 拒 EINVAL（:272-275）→ ④ 快照旧 priority/max_priority（:277-279）→ ⑤ `max_priority = priority = new_q` 双写（:282）→ ⑥ `schedule_process_local` 失败则**回滚**快照（:284-289）→ ⑦ 返回 rv |
| schedule_process（三条处理路径共用的下发出口） | ① **每次都先** `pick_cpu(rmp)`（:302，即使掩码不含 CPU 也会重选）→ ② 按掩码 SCHEDULE_CHANGE_PRIO/QUANTUM/CPU（:22-35）挑字段，未选中填 -1（"保持不变"哨兵，:304-317）→ ③ `niced = (max_priority > USER_Q)`（:319）→ ④ `sys_schedule(endpoint, prio, quantum, cpu, niced)`，失败 printf（**前缀误写 "PM:"**，:323，C 源码复制痕迹）并透传错误码（:321-327） |
| balance_queues | ① 全表扫 IN_USE 槽（:358-359）→ ② `priority > max_priority` 则 `priority -= 1` 升一级（:360-361）→ ③ `schedule_process_local(rmp)` 下发，**返回值不检查**（:362，下发失败本轮丢失、只 printf）→ ④ 重设下一轮 `sys_setalarm(balance_timeout, 0)`（:367-368，失败 panic）。"当前策略很快会换"的作者注释在 :348-352 |
| pick_cpu | ① 机器只有一个 CPU → 直接用 machine 的 boot cpu（:54-57）→ ② 系统进程（`is_system_proc`，:44）→ 固定 BSP（:60-63）→ ③ 其余选负载最低且可用的核（:66-75）→ ④ 选中后 `cpu_proc[cpu]++`（:77）；SMP 关闭时 :78-80 恒返 0 |

### 1.4 触发时机（谁在什么时候让这段代码跑起来）

| 消息 | 发送链（C 锚点） | 运行时刻 |
|------|-----------------|---------|
| SCHEDULING_START | ① PM 启动时替 INIT 申请：`pm/schedule.c:20-50` sched_init → `libsys/sched_start.c:46-98`（USER_Q/USER_QUANTUM/-1）；② RS 每生一个系统进程：`rs/utility.c` sched_init_proc（~363-381，6 参直发、parent 恒 RS_PROC_NR=2，`com.h:61`） | 系统启动早期（PM 接管 INIT 是第一笔用户消息）与 RS 每次拉起服务 |
| SCHEDULING_INHERIT | PM fork 子进程：`pm/forkexit.c:102` 默认 mp_scheduler=SCHED_PROC_NR → `pm/schedule.c:55-84` sched_start_user → `sched_start.c:11-41` sched_inherit（3 字段，不带时间片） | 每次 fork |
| SCHEDULING_STOP | `libsys/sched_stop.c:9-30`（KERNEL/NONE 短路 :16-17 不来电）；调用点 `pm/forkexit.c:425`（exit 路径，失败仅 warn）、`rs/manager.c:461`（清理服务）、`rs/request.c:342`（改槽） | 进程退出 / 服务被撤换 |
| SCHEDULING_SET_NICE | PM setpriority 系：`pm/schedule.c:89-112` sched_nice（KERNEL/NONE 进程直接 EINVAL :98-99） | 用户调 nice |
| SCHEDULING_NO_QUANTUM | 内核时钟路径：`kernel/proc.c:1893-1910` proc_no_time →（非内核调度 && PREEMPTIBLE）→ `proc.c:1860-1891` notify_scheduler 组通知（mini_send 带 FROM_KERNEL 旗标 :1887-1890，发送失败 panic） | 进程时间片耗尽的每一刻 |
| CLOCK 通知（非 SCHEDULING_*） | 自己 `sys_setalarm` 的闹钟（schedule.c:340/367）经内核转 NOTIFY | 每 5 秒 |

内核侧对端（服务器行为成立的前提）：`do_schedctl.c:16-43`（旗标检查→endpoint 检查→KERNEL 分支直接落参/注册分支把 p_scheduler 指向来电者）、`do_schedule.c:14-29`（验来电者是 p_scheduler 才许改表）、`system.c:642-700` sched_proc（priority 检查用 `>` 而非 `>=`，见 K-056）。

---

## 2. 知识点全集（存量去重 + 审计新增）

编号规则：`K-0xx` 按**新目录归属**分段（00 段=0xx，01 段=01x…99 段=16x）。类型取值：概念 / 机制 / 数据结构 / 协议 / 约束 / 架构 / 工程 / 测试 / 对照。

### 2.1 知识点池总表

| 编号 | 名称 | 类型 | 来源 | 现有位置（重复全记） | 锚点 | 读者收益 |
|------|------|------|------|--------------------|------|----------|
| K-001 | 双层调度模型：内核执行面 vs 用户态策略面 | 概念 | 存量 | 00 核心点、01 §1.1、99 核心点 | schedule.c 全篇 + system.c:642-700 | 回答"策略为什么不写进内核" |
| K-002 | 用户态调度器的隔离与演化论据（策略 bug 不炸内核） | 概念 | 存量 | 01 §1.1、02 §1.1 | main.c 事件循环形态 | 同上，论证层 |
| K-003 | SCHED 在启动因果链的位置：boot_image 登记、RS 执行加载 | 概念 | 存量 | 00、01 §1 | kernel/table.c:56、com.h:63-64 | 知道这个进程从哪来 |
| K-004 | 五进程表一致性（proc/mproc/vmproc/fproc/schedproc 各管一段） | 概念 | 存量 | 99 核心点 | 各表头文件 | 全仓视角定位本表 |
| K-005 | 全 stage 导航与阅读路径（主线/支线/可跳读） | 工程 | 存量 | 00、plan.md §3 | 本蓝图 §4 | 按图检索 |
| K-006 | 一次请求的生命周期总览（触发方→消息→handler→下发→回复） | 概念 | 存量 | 02 §1.1、01 §1.4 | main.c:34-94 | 全 stage 主线心智图 |
| K-007 | 各 OS 对照（Linux CFS、Redox 等，散布于每篇 §1.x"与其他 OS 对照"小节） | 对照 | 存量 | 01-14 各篇对照小节 | 见 §3.5 处理规则 | 横向参照系 |
| K-010 | sef_local_startup：注册 fresh 与 restart-stateful 两类回调 | 机制 | 存量 | 01 §1.2、§2.7 | main.c:111-121 | SCHED 如何接入 SEF 生命周期 |
| K-011 | init_fresh：sys_getmachine + 失败 panic | 机制 | 存量 | 01 §1.3、§2.8 | main.c:126-136 | 启动第一步做什么、失败策略 |
| K-012 | machine 结构（boot CPU 与 CPU 数的来源） | 数据结构 | 存量 | 01 §2.8、10 §2.4 | type.h（旧引 122-125 待重定位） | 选核的数据从哪里来 |
| K-013 | init_scheduling：把 5 秒换成 ticks 并设第一只闹钟 | 机制 | 存量 | 01 §2.8、11 §2.2 | schedule.c:334-342 | 平衡节奏的起点 |
| K-014 | "先注册后循环"的启动结构与不可达 return | 约束 | 存量 | 01 §2.1 | main.c:22-35、:95 | 启动段与循环段的分界 |
| K-015 | 构建制品：Makefile（三 .c + 链 libsys，minix.service.mk） | 工程 | 新增 | —— | servers/sched/Makefile:1-9 | 消息 wrapper 为什么全在 libsys |
| K-016 | Rust 启动装配决策（startup 枚举、机器信息透传、就绪令牌、二进制层只装配） | 架构 | 存量 | 01 §3 D1-D5 | os/servers/sched/src/main.rs、sef.rs | C 语义如何落到 Rust 模块 |
| K-020 | 五种调度消息与调用号（SCHEDULING_BASE 0xF00，+1..+5） | 协议 | 存量 | 02 §1.2、§2.3 | com.h:801-807 | 本服务的接口全集 |
| K-021 | 通知 vs 同步调用的区别（is_ipc_notify） | 协议 | 存量 | 02 §1.3、§2.2 | com.h:92、main.c:45 | 为什么有的消息不回 |
| K-022 | FROM_KERNEL 信任门：内核旗标验证与伪造举报（EPERM） | 约束 | 存量 | 02 §1.4、§2.4、08 §1.2、§2.5 | main.c:68-84、ipcconst.h:28-30 | NO_QUANTUM 为什么免白名单 |
| K-023 | accept_message 发送者白名单（只认 PM、RS） | 协议 | 存量 | 04 §1.5、§2.3、07 §2.1、09 §2.1 | utility.c:61-74 | 谁有权向 SCHED 下单 |
| K-024 | 消息体字段与 56 字节线形状（start/stop/nice/inherit 四结构 + 回复 scheduler 字段） | 协议 | 存量+新增 | 02 §2.7、07 §2.5、08 §2.6 | ipc.h:2568-2612（union）、1440-1444、1822-1827 | 逐字段知道线上传什么 |
| K-025 | _taskcall 同步往返语义（客户端阻塞等回复；trap 层机制不在本 stage） | 机制 | 新增 | —— | sched_start.c:30/87；对 ../01-stage-kernel/13-syscall-dispatch.md | 消息怎么跨过内核到服务器 |
| K-026 | no_sys 拒收：未知调用号回 ENOSYS | 机制 | 存量 | 02 §2.6 | utility.c:18-23 | 接口外的来电怎么处置 |
| K-027 | 回复约定：result 写回 m_type、reply(who_e)、SUSPEND 例外 | 协议 | 存量 | 01 §2.5-2.6、02 §2.5、§1.5 | main.c:89-96、101-106 | 一次调用的收尾 |
| K-028 | 回复 scheduler 字段的照单全收语义（可能不是被问的那个调度器） | 协议 | 存量 | 06 §1.6、13 §1.4 | schedule.c:239-246、sched_start.c:34-39/91-96 | 委托调度为何可能 |
| K-029 | Rust 消息建模决策（消息枚举、三态通知、校验布尔、回复规则建模、消息体上移 minix-types） | 架构 | 存量 | 02 §3 D1-D6 | os/servers/sched/src/dispatch.rs、minix-types ipc/message.rs | 协议面在 Rust 的落点 |
| K-030 | 主循环骨架：收信→通知分流→分发→回复（唯一主讲述点） | 约束 | 存量 | 01 §2.2-2.5、02 §2.1-2.3/2.5（双重覆盖） | main.c:34-94 | 一轮循环的四拍 |
| K-031 | 分发四入口与 START/INHERIT case 贯穿 | 机制 | 存量 | 02 §2.3 | main.c:57-87 | 双入口在循环层的证据 |
| K-032 | NO_QUANTUM 路径：失败仅 printf、恒不回复 | 机制 | 存量 | 01 §2.4、02 §2.4、08 §2.5 | main.c:68-84 | 内核消息的处理闭环 |
| K-033 | SUSPEND=-998 伪返回值约定 | 约束 | 存量 | 01 §1.5 | com.h:1151 | 服务"挂起不回"的通用语言 |
| K-034 | CLOCK 通知 → balance_queues 无条件调用（节奏在闹钟不在循环） | 机制 | 存量 | 02 §2.2、11 §2.5 | main.c:45-55 | 循环与平衡的真实分工 |
| K-035 | Rust 循环所有者（SchedServer 单所有者 [ARCH S-11]、run_once 单步、双 transport trait 接线） | 架构 | 存量 | 02 §3 D7-D9 | os/servers/sched/src/server.rs、kernel_api/transport.rs | 全局四件套收进一个对象 |
| K-036 | 单线程事件循环并发模型（!Send/Rc/RefCell 合法判据；内核侧才是 SMP） | 约束 | 新增 | 各篇隐含（AGENTS.md 执行模型） | server.rs 模块头声明 | 为什么本 crate 无锁 |
| K-040 | schedproc 八字段逐个（endpoint/parent/flags/max_priority/priority/time_slice/cpu/cpu_mask） | 数据结构 | 存量 | 03 §1.2-1.6、§2.2-2.6 | schedproc.h:23-36 | 服务器私有状态的字段地图 |
| K-041 | IN_USE 标记与 flags=0 整字清零（登记置位 :223 / 释放清零 :132） | 机制 | 存量 | 03 §1.3、07 §2.4 | schedproc.h:39、schedule.c:223/132 | 槽位生死的唯一判据 |
| K-042 | cpu_mask 死字段消除 [ARCH S-3]（C 有字段无消费，FIXME 残留） | 架构 | 存量 | 03 §1.6、§2.6 | schedproc.h:34-35、schedule.c:185 | 删字段的论证方法 |
| K-043 | _MAIN extern 模式与 CONFIG_MAX_CPUS 默认值 | 数据结构 | 存量 | 03 §2.1、10 §2.3 | schedproc.h:9-16 | C 全局表怎么跨编译单元 |
| K-044 | schedproc[NR_PROCS] 表本体（按 proc 号定槽） | 数据结构 | 存量 | 03 §2.7 | schedproc.h:36 | 表大小与索引方式 |
| K-045 | sched_isokendpt 四道判断及其顺序（<0 / ≥NR_PROCS / endpoint 失配 / 未占用） | 机制 | 存量 | 04 §1.3、§2.1 | utility.c:29-41 | "确认有人在册"的完整检查 |
| K-046 | sched_isemtyendpt 镜像逻辑（槽必须空或属于同一进程） | 机制 | 存量 | 04 §1.4、§2.2 | utility.c:46-56 | "确认可登记"的检查 |
| K-047 | 校验失败的错误码语义（EBADEPT/EINVAL/EDEADEPT 分工） | 约束 | 存量 | 04 §2.4、02 各表 | utility.c + errno | 排障时的错误码词典 |
| K-048 | Rust 表设计决策（身份透传、标记枚举、优先级新类型、单位写明、死字段删除；槽校验枚举化） | 架构 | 存量 | 03 §3、04 §3 | schedproc.rs、table.rs、valid.rs | 数据结构在 Rust 的落点 |
| K-050 | 队列常量族（NR_SCHED_QUEUES=16/TASK_Q=0/MAX_USER_Q=0/USER_Q 派生/MIN_USER_Q=15/USER_QUANTUM=200/USER_DEFAULT_CPU=-1） | 数据结构 | 存量 | 05 §1.2-1.4、§2.1 | config.h:66-77 | 策略词汇的数值底座 |
| K-051 | 上限与当前值双字段语义（max_priority vs priority） | 概念 | 存量 | 05 §1.3 | schedproc.h:29-30 | MLFQ 的"天花板"与"现在" |
| K-052 | 时间片毫秒单位 [ARCH S-6] 与双 200 双源（DEFAULT_USER_TIME_SLICE schedule.c:41 / USER_QUANTUM config.h:74） | 约束 | 存量 | 05 §1.4、§2.2 | 同锚点 | 同名不同源的坑 |
| K-053 | nice→队列线性换算（nice_to_priority 公式与钳位；越界 EINVAL） | 机制 | 存量 | 05 §2.4、13 | pm/utility.c:91-101 | setpriority 之后数字怎么变 |
| K-054 | 系统进程判定宏 is_system_proc（endpoint==RS_PROC_NR） | 机制 | 存量 | 05 §2.3、10 §1.2 | schedule.c:44、com.h:61 | 选核第二规则的输入 |
| K-055 | niced 谓词（max_priority > USER_Q）与内核 MF_NICED 记账消费链 | 机制 | 存量 | 05 §2.5、09 §1.4 | schedule.c:319、system.c:691-694、arch_clock.c:318 | "nice 进程"在内核留痕 |
| K-056 | 内核优先级校验用 `>`、服务器用 `>=`（priority=16 能过内核被服务器拒的 16 差异） | 约束 | 存量 | 05 §2.5、09 §2.5（浅） | system.c:645 vs schedule.c:164/273 | 双侧校验不对称的实例 |
| K-057 | Rust 模型层决策（常量集中、Nice 新类型、系统进程谓词、时间片单位命名、哨兵枚举、niced 跟上限） | 架构 | 存量 | 05 §3 D1-D6 | priority.rs | 模型在 Rust 的落点 |
| K-060 | 调度归属两状态（p_scheduler 指向内核约定 vs 用户端点） | 概念 | 存量 | 12 §1.2、§2.6 | proc.h:178-179、do_schedctl.c:39/42 | "谁说了算"的内核表示 |
| K-061 | do_schedctl：旗标检查→endpoint 检查→两分支（KERNEL 直接落参 / 注册来电者为调度器） | 机制 | 存量 | 12 §1.3、§2.1-2.2、06 §1.4/§2.6（侧写） | do_schedctl.c:16-43 | 接管请求的内核处理 |
| K-062 | SCHEDCTL_FLAG_KERNEL 与参数字段先读后写顺序 | 协议 | 存量 | 12 §2.1、06 §2.6 | com.h:449、do_schedctl.c:28-39 | 旗标语义与内核写入节拍 |
| K-063 | do_schedule 入口三检查（isokendpt→EINVAL；caller≠p_scheduler→EPERM；niced !! 强转） | 约束 | 存量 | 09 §2.4 | do_schedule.c:14-27 | 只有当前调度器能改表 |
| K-064 | sched_proc：三道检查 + 出队/写字段/MF_NICED/按新队列回队 + RTS_NO_QUANTUM 处理 + 换核迁移 | 机制 | 存量 | 09 §2.5（旧引 642-699，实为 642-700） | system.c:642-700 | 参数落进内核表的完整动作 |
| K-065 | notify_scheduler：出队、填 7 统计字段、清零计数、mini_send 带内核旗标、失败 panic | 机制 | 存量 | 12 §1.4、§2.3 | proc.c:1860-1891 | NO_QUANTUM 消息的内核侧诞生 |
| K-066 | 七个统计字段：全发、服务器只读来源 | 约束 | 存量 | 12 §1.5、§2.4 | proc.c:1876-1882、proc.h:50-55 | 消息载荷的"备用账本"现状 |
| K-067 | proc_no_time 两分支（PREEMPTIBLE+用户调度→通知；否则内核续时间片） | 机制 | 存量 | 12 §1.6、§2.5 | proc.c:1893-1910 | 降级的触发闸门 |
| K-068 | PREEMPTIBLE 特权位（位定义 + SRV_F/USR_F/IMM_F 模板 + Rust 侧已修状态） | 约束 | 存量+修正 | 12 §1.6 | const.h:143、priv.h:45-50；Rust edge1 K1 ✅2026-09-18 | 时间片通知的真正门槛 |
| K-069 | 与 01-stage-kernel/11 的内核侧边界（队列内部操作只引用不重讲） | 工程 | 存量 | 12 头分工声明 | ../01-stage-kernel/11-scheduling-primitives.md §2.6/§3.8/§4.5 | 跨 stage 不重复 |
| K-070 | Rust 内核接缝决策（旗标集合化、归属两态、接管组装、统计字段只描述不建模） | 架构 | 存量 | 12 §3 D1-D4 | kernel_api/schedctl.rs、schedule.rs | 契约在 Rust 的落点 |
| K-075 | pick_cpu 三条有序规则（单核短路→系统进程钉 BSP→负载最低） | 机制 | 存量 | 10 §1.2、§2.2 | schedule.c:48-81 | 选核的全部决策 |
| K-076 | cpu_proc 负载台账（选中 ++、释放 --、死亡赋值）与零初始化 | 数据结构 | 存量+修正 | 10 §1.3、§2.1 | schedule.c:46/77/130/229 | 三处写点的账本 |
| K-077 | CPU_DEAD=-1 与 cpu_is_available 对无符号数恒真的陷阱（真筛藏在负载比较里） | 约束 | 存量 | 10 §1.4、§2.1 | schedule.c:37-39 | C 瑕疵的读码课 |
| K-078 | 编译开关到运行时判断的退化 [ARCH S-5]（CONFIG_MAX_CPUS/SMP ifdef → 运行时 ncpu） | 架构 | 存量 | 10 §1.5、§2.3、01 §1.3 | schedproc.h:14-16、schedule.c:78-80 | 单核/多核一套代码 |
| K-079 | Rust 选核决策（三规则纯函数、死亡用 None、计数饱和加减、开关改参数） | 架构 | 存量 | 10 §3 D1-D4 | cpu.rs | 选核在 Rust 的落点 |
| K-082 | 下发掩码族（SCHEDULE_CHANGE_PRIO/QUANTUM/CPU/ALL/LOCAL 宏） | 机制 | 存量 | 09 §2.1、08 §2.4、11 §2.4（三处） | schedule.c:22-35 | "带哪些字段"的表达方式 |
| K-083 | -1 保持语义（未选字段填 -1，内核据此跳过） | 机制 | 存量 | 09 §1.3、§2.2 | schedule.c:304-317、system.c:680-688 | 局部下发的哨兵 |
| K-084 | schedule_process 每次调用都先重 pick_cpu（含 LOCAL 路径） | 约束 | 存量 | 09 §2.2 | schedule.c:302 | 隐蔽的副作用点 |
| K-085 | sys_schedule 打包发送与错误 printf 透传（含 "PM:" 前缀笔误） | 机制 | 存量 | 09 §2.3 | schedule.c:321-327、libsys/sys_schedule.c | 下发的最后一公里 |
| K-086 | schedule_process（服务器）与 sched_proc（内核）命名对照防混淆 | 概念 | 存量 | 09 §1.6、§2.6 | 两侧锚点并列 | 读码避坑 |
| K-087 | Rust 下发层决策（掩码 flags 类型、None 保持位、niced 复用谓词、检查归内核、打包发送分离） | 架构 | 存量 | 09 §3 D1-D5 | kernel_api/schedule.rs | 下发在 Rust 的落点 |
| K-090 | START/INHERIT 双入口的类型断言 | 机制 | 存量 | 06 §1.1、§2.1 | schedule.c:145-147 | 共口的第一道保险 |
| K-091 | do_start 四道检查顺序 + 边界检查发生在部分写入之后 | 约束 | 存量 | 06 §1.2、§2.1-2.2 | schedule.c:149-166 | 顺序不可换的原因 |
| K-092 | 三种填值方式（init 自父特例 / START 显式 / INHERIT 抄父） | 机制 | 存量 | 06 §1.3、§2.3-2.5 | schedule.c:168-214 | 参数从哪来全景 |
| K-093 | init 临时值被 START 分支覆盖（USER_Q 与 maxprio、两个 200 的双源细节） | 机制 | 存量 | 06 §1.3、10 §1.1（双述） | schedule.c:174-175 vs 195-196 | 相等值不同源的教学案例 |
| K-094 | sys_schedctl(0,ep,0,0,0) 交接语义 + 成功才置 IN_USE | 机制 | 存量 | 06 §1.4、§2.6 | schedule.c:216-223 | 内核改判"谁说了算" |
| K-095 | EBADCPU 重试环（标死→重选→重下；每轮只多一个死核） | 机制 | 存量 | 06 §1.5、§2.7、10 §2.5 | schedule.c:226-231 | 坏核自愈环 |
| K-096 | fork 次主线路径图（fork→INHERIT→接管→下发→回复） | 概念 | 存量 | 06 §1.7 | 组合锚（forkexit+schedule.c） | 一张图串起全 stage |
| K-097 | Rust do_start 决策（双消息收进类型、检查顺序照抄、双向边界拒绝、三填值、置标记留调用者、重试收进类型） | 架构 | 存量 | 06 §3 D1-D6 | scheduling/start.rs、server.rs | 接管在 Rust 的落点 |
| K-100 | do_stop 两道检查（白名单 + 在册校验 EBADEPT） | 机制 | 存量 | 07 §2.1-2.2 | schedule.c:112-126 | 释放前的资格检查 |
| K-101 | 释放做的两件事（SMP 负载减一 + flags 整字清零；单核 ifdef 不编入） | 机制 | 存量 | 07 §1.3、§2.3-2.4 | schedule.c:128-134 | 释放的最小动作 |
| K-102 | 释放"没有的东西"：不通知内核、不清 p_scheduler（隐式消亡链） | 机制 | 存量 | 07 §1.4 | schedule.c:112-135 缺失动作 + do_schedctl.c:28-39 | 对称性缺口的解释 |
| K-103 | start/stop 对称表（登记字段 vs 释放动作） | 概念 | 存量 | 07 §1.5 | 两函数并排 | 成对接口的心智模型 |
| K-104 | exit 的三条来路（PM exit 来电、RS 撤换来电、KERNEL/NONE 短路不来电） | 机制 | 存量 | 07 §1.6、§2.6 | sched_stop.c:16-17、forkexit.c:425、manager.c:461 | 谁保证槽位一定被清 |
| K-105 | Rust do_stop 决策（单字段消息、检查顺序、两释放、对称写进代码） | 架构 | 存量 | 07 §3 D1-D4 | scheduling/stop.rs | 释放在 Rust 的落点 |
| K-110 | 信任不对称：内核旗标门（NO_QUANTUM 免白名单）vs 用户白名单门 | 约束 | 存量 | 08 §1.2、02 §1.4 | main.c:68-84 + utility.c:61-74 | 同一循环两套门 |
| K-111 | do_noquantum 全链（源校验 EBADEPT → 降级封顶 MIN_USER_Q → LOCAL 下发 → 失败仅 warn） | 机制 | 存量 | 08 §1.3、§2.1 | schedule.c:87-107、main.c:72-76 | 降级主路径 |
| K-112 | do_nice 五步（白名单→在册→边界 EINVAL→双写快照→失败回滚） | 机制 | 存量 | 08 §1.4、§2.2-2.3 | schedule.c:254-292 | 改上限与回滚 |
| K-113 | SET_NICE 来路（PM sched_nice 组装 + KERNEL/NONE 进程 EINVAL 挡在客户端） | 协议 | 存量 | 08 §2.6、13 | pm/schedule.c:89-112 | 一条消息的两端 |
| K-114 | Rust 降级/改上限决策（发送者检查外移、降级纯函数、改上限三段、双向边界、局部下发只讲意） | 架构 | 存量 | 08 §3 D1-D5 | scheduling/noquantum.rs、nice.rs | 两个 handler 在 Rust 的落点 |
| K-120 | 平衡节拍三数（balance_timeout 静态秒→ticks；BALANCE_TIMEOUT=5；sys_hz） | 机制 | 存量 | 11 §2.1 | schedule.c:16-18、sysutil.h:60 | 5 秒怎么变成滴答 |
| K-121 | balance 扫描（IN_USE ∧ priority>max_priority → 升一级 → LOCAL 下发） | 机制 | 存量 | 11 §2.3 | schedule.c:358-363 | 回升的判据 |
| K-122 | 降得快、升得慢的防振荡不对称 | 概念 | 存量 | 11 §1.2 | schedule.c:99-101 vs 360-361 对照 | MLFQ 稳定性的根 |
| K-123 | 闹钟循环三拍（设→响→重设；重设失败 panic） | 机制 | 存量 | 11 §1.3 | schedule.c:340/367-368、main.c:47-48 | 自维持周期 |
| K-124 | 恢复下发只动优先级+时间片（LOCAL 掩码用法） | 机制 | 存量 | 11 §1.4 | schedule.c:32-33/362 | 掩码的应用位点 |
| K-125 | 默认策略可替换位（作者注释 "will soon be changed"；Rust 把策略做成结构体） | 概念 | 存量 | 11 §1.5、§3 D4 | schedule.c:348-352、balancer.rs | 扩展点识别 |
| K-126 | balance 回升的下发返回值不检查（失败本轮静默丢失） | 约束 | 新增 | —— | schedule.c:362 | C 瑕疵与 Rust 对齐度 |
| K-130 | libsys sched_start 三路（NONE 直返 OK / KERNEL 走 sys_schedctl / 用户态发 START） | 机制 | 存量 | 13 §1.2、§2.2 | sched_start.c:46-98 | 一个 wrapper 三种命运 |
| K-131 | sched_inherit 3 字段 vs sched_start 4 字段（时间片不上传） | 协议 | 存量 | 13 §1.3、§2.3 | sched_start.c:24-27/80-84 | START/INHERIT 载荷差 |
| K-132 | sched_stop 短路（KERNEL/NONE 返回 OK 不发邮件） | 机制 | 存量 | 13 §2.2、07 §2.6 | sched_stop.c:14-17 | STOP 的静默半 |
| K-133 | 回复调度器字段的消费约定（*newscheduler_e 为准，覆盖本地记录） | 机制 | 存量 | 13 §1.4 | sched_start.c:34-39/91-96、forkexit/pm 消费 | 委托语义闭环 |
| K-134 | PM sched_init：接管 INIT（IN_USE∧非 PRIV、自父断言、USER_Q/USER_QUANTUM/-1、失败仅 warn） | 机制 | 存量 | 13 §1.6、§2.4 | pm/schedule.c:20-50 | 第一笔用户消息 |
| K-135 | PM sched_start_user：nice 换算 + PRIV 父进程时改从 INIT 继承 | 机制 | 存量 | 13 §2.4 | pm/schedule.c:55-84 | 子进程参数来路 |
| K-136 | PM sched_nice：KERNEL/NONE 挡 EINVAL、换算、组 SET_NICE 消息 | 机制 | 存量 | 13、08 §2.6 | pm/schedule.c:89-112 | 用户 setpriority 半程 |
| K-137 | PM 生死来回（fork 默认 SCHED、exit 发 STOP 失败 warn、特权子进程 NONE） | 机制 | 存量 | 13 §1.5、§2.5 | forkexit.c:102/425/438-441 | mproc.mp_scheduler 一生 |
| K-138 | PM 启动接线（INIT→KERNEL、system 进程→NONE、fork 后申请失败即拆台） | 机制 | 存量 | 13 §2.6 | pm/main.c:199/213/372-380 | 申请失败的处理强度 |
| K-139 | RS sched_init_proc：双断言 + 6 参直发（parent 恒 RS 自己） | 机制 | 存量 | 14 §1.2-1.3、§2.1-2.2 | rs/utility.c:363-381、com.h:61 | 系统进程申请形状 |
| K-140 | RS 槽内四值（r_scheduler/r_priority/r_quantum/r_cpu 来自配置文件） | 数据结构 | 存量 | 14 §2.3 | rs/type.h:92-95 | 申请的参数储处 |
| K-141 | RS 取消按位置两处理（清理时失败 warn 继续；改槽时失败带码返回） | 机制 | 存量 | 14 §1.4、§2.4-2.5 | manager.c:461、request.c:342 | 同一 API 两种失败策略 |
| K-142 | 生源分工：系统进程生在 RS 申请先行 vs 用户进程生在 PM 申请后到 | 概念 | 存量 | 14 §1.1 | 两路并排 | 消息量差别的根因 |
| K-143 | 主权文档分工声明（PM nice 用户面→04-16；RS 槽配置/LiveUpdate→03-08、03-16） | 工程 | 存量 | 13/14 头部 | 两篇头分工声明段 | 跨 stage 不重复展开 |
| K-144 | Rust 客户端镜像归属（client.rs 形状 PM/RS 共用；RS 侧实施在 os/servers/rs） | 架构 | 存量 | 14 头实现归属段、§3 | client.rs、rs/src/sched.rs | 跨 crate 归属声明 |
| K-145 | SCHED 无正常退出路径（while(TRUE)；换身由 RS live-update 承接） | 约束 | 新增 | ——（旧文未明说） | main.c:34-95（:95 不可达）+ ../03-stage-rs/16-rs-live-update.md | "关闭与退出"的正确回答 |
| K-150 | 宿主测试基建：mock 传输、CannedTransport 回放、81 passed 基线、clippy 0 告警（crate 本体） | 测试 | 存量+新增 | 各篇 §5/§5.1（分散） | os/servers/sched/src/*、todo.md 基线 | 全测于 mock 的含义 |
| K-151 | 真实通电现状：trap 底座真机已验（test-user-trap PASS）、SCHED 生产二进制通电 = E8 余项 S27 | 工程 | 新增 | —— | edge_todo E8:258-270、edge3 S27 | 离线完备与上线之间差什么 |
| K-152 | E5(e) 冒烟三链（init START 接管链 / fork INHERIT 抄父链 / NO_QUANTUM 回环） | 测试 | 新增 | —— | edge_todo E5(e):194 | 端到端验收清单 |
| K-153 | 边条指针现状：E-SCHEDNICED ✅、E-PREEMPTFLAG live 半 ✅ + enqueue Phase 3 ✅（edge1 K1 2026-09-18）、E-SCHEDSMP 三环 ✅（edge1 K2 2026-09-18）、E-MINTYPES-SYS ✅（SUSPEND/SYS_* 上移收敛） | 工程 | 存量+更新 | todo.md §2、各篇"过渡" | edge1.md:17-18、edge_todo 各条 | 文档不能继续抄旧"未修"状态 |
| K-154 | 文档-代码锚点同步纪律（模块头 owned-by 注释 21 处；plan/todo 引用） | 工程 | 新增 | —— | os/servers/sched/src 21 处引用 | B 相迁移必改清单 |
| K-160 | 常量速查表（com.h/config.h/ipcconst.h 全族一表） | 参考 | 存量 | 02/05/99 分散 | 各锚点 | 检索字典 |
| K-161 | 错误码速查表（EPERM/EINVAL/EBADEPT/EDEADEPT/EBADCPU/ENOSYS/SUSPEND） | 参考 | 存量 | 各篇 §2 分散 | utility.c/schedule.c/system.c | 排障字典 |
| K-162 | ARCH 决策总表 S-1..S-11（各篇 ARCH 小表汇总） | 架构 | 存量 | plan.md §4 + 每篇 §3 末 ARCH 表 | plan.md:§4 | 偏离 C 的总账 |
| K-163 | Rust crate 模块地图（21 文件 4525 行 ↔ 新目录对照） | 参考 | 新增 | —— | os/servers/sched/src（wc 实测） | 代码↔文档导航 |
| K-164 | 每篇"实现详解"三件套（模块结构/核心符号表/不变量）与"测试要点"节的组织惯例 | 工程 | 存量 | 01-14 各 §4/§5 | 各篇 | B 相沿用骨架 |
| K-165 | 各篇"过渡/参见"导航节的组织惯例 | 工程 | 存量 | 01-14 各 §6/§7 | 各篇 | 迁移后按新序重生成 |

统计摘要：共 **101 条**（存量 84、新增 17：K-015/025/036/126/145/150-154/163 + K-056/068/076/093/133 的部分增量按主条目计入存量）。按类型：概念 11、机制 33、数据结构 9、协议 8、约束 13、架构 12、工程/测试/参考 15。按旧文档分布：00/99 骨架贡献 6 条核心点，01→12 条，02→17 条，03→9 条，04→7 条，05→9 条，06→9 条，07→7 条，08→7 条，09→8 条，10→6 条，11→8 条，12→9 条，13→8 条，14→7 条（重复条目只记主来源）。

### 2.2 重复知识点的主讲述点标记

| 知识点 | 全部现有位置 | 新主讲述点 | 其余处置 |
|--------|--------------|-----------|----------|
| K-030 主循环逐段走查 | 01 §2.2-2.5、02 §2.1-2.3/2.5 | 新 03 | 两处旧文合并为唯一版本 |
| K-022 FROM_KERNEL 信任门 | 02 §1.4/2.4、08 §1.2/2.5 | 新 02（定义）| 新 11 一句话回指 |
| K-082 下发掩码族 | 08 §2.4、09 §2.1、11 §2.4 | 新 08 | 新 11/12 回指 |
| K-093 init 临时值覆盖 | 06 §1.3、10 §1.1 | 新 09 | 新 07 只在 machine 语境提一句 |
| K-028 回复 scheduler 照单全收 | 06 §1.6、13 §1.4 | 新 09（服务端写入）+ 新 13（客户端消费）各一次、角度不同不属重复 | 原两处去重 |
| K-034/K-123 闹钟-平衡链 | 02 §2.2、11 §2.2/2.5 | 新 03（循环分支事实）+ 新 12（完整机制）| 边界写明"循环只调用不判断" |
| K-055 niced | 05 §2.5、09 §1.4 | 新 05（谓词定义）| 新 08 应用位点回指 |
| K-130-133 libsys 三路/字段/短路/回复 | 07 §2.6、13 全篇 | 新 13 | 新 10（stop）只留"来路三型"表 |
| K-013 init_scheduling | 01 §2.8、11 §2.2 | 新 01（设闹钟动作）| 新 12 从"闹钟响了"起讲 |
| K-160-162 常量/错误码/ARCH 表 | 各篇分散 | 新 99 汇总 + 各篇首现处仍完整讲 | 汇总≠转移：首现即完整原则保留 |

---

## 3. 覆盖审计

### 3.1 主题全集与来源（四路）

1. **C 符号**：servers/sched 全部 3 个 .c + 3 个 .h 的每个函数/宏/结构/常量（§0.2 清单实测：main 1、reply 1、setreply 1（声明未定义，proto.h:8——Rust 侧同样未实现，归缺口 G-9）、schedule.c 内 PRIVATE/static 9 个（pick_cpu、do_noquantum、do_stop、do_start、do_nice、schedule_process、init_scheduling、balance_queues、cpu_is_available）+ 宏 6 组、utility.c 4 个、schedproc.h 结构+旗标）；内核对端 5 个（do_schedctl、do_schedule、sched_proc、notify_scheduler、proc_no_time）；客户端 7 个（sched_start、sched_inherit、sched_stop、sys_schedule、sys_schedctl、nice_to_priority、sched_init/sched_start_user/sched_nice）。
2. **OS 通用概念**：优先级反转与降级、MLFQ 防振荡、负载记账、信任边界（内核旗标 vs 白名单）、同步 RPC 往返、服务器无退出路径、单线程事件循环并发模型。
3. **非 C 制品**：Makefile 构建、boot_image 登记、56 字节消息线格式、_taskcall/trap 往返、Rust crate 布局与 mock 测试基建、通电 edge 族（E8/E5(e)）。
4. **边界契约**：master-plan README（SCHED=06、RS 加载、调度参数继承定位）、edge_todo 六条（E-SCHEDNICED/E-PREEMPTFLAG/E-SCHEDSMP/E-MINTYPES-SYS/E5(e)/E8）、前 stage 已教清单（01-stage-kernel/11 教过内核队列、15 教过时钟、16 教过 SMP）。

### 3.2 覆盖缺口表（→ 新知识点入池）

| # | 缺口主题 | 判定 | 落实 |
|---|---------|------|------|
| G-1 | 00 与 99 是 20 行 pending 骨架，导航与速查内容实际不存在 | 新建（改写） | 新 00、新 99（原料：两篇核心点 + K-001/003/004/005/160-163） |
| G-2 | 测试基建与真实通电没有专篇（81 测试分散在各篇 §5，E8/E5(e) 状态无文档承接） | 新建篇章 | 新 14（原料：K-150~153 + 各篇 §5 上收 + edge_todo/edge1 实测状态） |
| G-3 | 构建制品（Makefile 链 libsys、minix.service.mk）无人讲 | 并入某篇 | 新 01 一节（K-015）；Cargo 侧并入新 14 |
| G-4 | 56 字节线格式与 _taskcall 往返只在客户端语境内侧写 | 并入某篇 | 新 02（K-024/025；trap 层内部判归 01-stage-kernel，边界声明） |
| G-5 | 单线程事件循环并发模型没有显式定型篇 | 并入某篇 | 新 03（K-036；这是"为什么本 crate 无 Arc/Mutex"的唯一答案） |
| G-6 | balance 回升下发不检查返回值（schedule.c:362）未入文 | 新增知识点 | 新 12（K-126） |
| G-7 | SCHED 无退出路径未明说（固定清单"关闭与退出"项） | 新增知识点 | 新 10（K-145） |
| G-8 | cpu_proc 初值（零初始化，无显式初值）未明说 | 新增细节 | 新 07（K-076 修正条目） |
| G-9 | proto.h:8 声明 setreply 全树无定义（C 死声明） | 新增细节 | 新 01 一节脚注（并入 K-014；判"死声明，不翻译"） |
| G-10 | 判定属于其它 stage：内核对端队列机制展开（→01-11）、RS 加载流程（→03）、PM 表 mproc（→04）、live-update（→03-16） | 明确不做 | 各契约"不讲什么"落点 |
| G-11 | 明确不做：内核 sys_hz/时钟源实现（→01-15）；IPC 投递内部（→01-12）；machine 结构完整字段（boot 期已讲，本 stage 只用到 boot cpu 与核数） | 明确不做 | 同上 |

### 3.3 重复主题表

见 §2.2 末表（10 组，全部给出主讲述点与回指处置）。重复的根因是旧目录把"主循环"同时划给了 01 和 02、把"掩码"同时划给了 08/09/11——新目录以"机制唯一归属 + 应用位点回指"消除。

### 3.4 越界主题表

| 旧位置 | 越界内容 | 正确归属 |
|--------|---------|---------|
| 旧 02 §3 D7/D8/D9 | 循环转法、SchedServer 所有者、双 transport 接线——消息版面篇夹带循环实现手册 | 新 03（K-035）；transport 接线与 mock 关系入新 14 |
| 旧 09 §2.4/§2.5 | 内核 do_schedule 三检查与 sched_proc 完整走查（内核内部机制在 01-stage-kernel/11 已教） | 新 06 收"契约面"（检查与写入节拍），队列内部一句话回指 01-11 |
| 旧 12 §2.3-2.5 | notify_scheduler/proc_no_time 展开与 01-11 §2.6/§4.5 重叠 | 新 06 从"服务器能假设什么"角度重写，内部动作回指 01-11 |
| 旧 14 §3 | os/servers/rs 侧实施决策（D0/D1 落点在 rs crate） | 新 13 只保 RS 客户端行为契约 + 归属声明（K-143/144），实施决策主权在 03-stage-rs |
| 旧 11 §1.6 | 引用 Redox RSoC 2026 新闻外链作为事实论据（外部信息未复核） | 删除外链；对照只保留可自证的概念层（K-007 处理规则） |
| 旧 01 §1.1/02 §1.1 | "为什么策略放用户态"两篇各讲一遍 | 新 00 唯一主讲述点（K-001/002） |

### 3.5 对照类内容的统一处理规则（K-007）

每篇保留一个"其他系统的同类设计"小节，但断言分级：Linux/Redox/seL4 的**公开稳定机制**（CFS 虚拟时间、Redox daemon 单点等）可作概念对照；**时效性外部资讯**（新闻链接、赛季项目状态）一律降级为"待验证，仅作背景提及"或删除。旧 11 篇 RSoC 外链按此规则删（B 相执行时核对）。

### 3.6 非 C 主题逐项回答（固定清单）

| 主题 | 在哪里讲 / 为什么不在本 stage |
|------|------------------------------|
| 链接与加载 | 新 01：sched 是 RS 按 boot_image 槽位加载的普通用户 ELF（table.c:56）；链接产物 Makefile（K-015）。ELF 装载器与页表建立归 01-stage-kernel/03-stage-rs/02-stage-vm |
| 镜像与内存布局 | 不在本 stage：sched 无专用链接脚本与内存布局约束（构建走通用 minix.service.mk，`servers/sched/Makefile` 全文 9 行无特殊段）；Rust 侧 bin 产物由 workspace 统一构建（os/servers/sched/Cargo.toml）。新 01 一句话声明即可 |
| 汇编入口与陷阱进入 | 不在本 stage：_taskcall → int 33/SYSCALL 的 trap 层归 01-stage-kernel（及 edge E1，真机已通，edge3 记录）；新 02 只讲"同步调用会阻塞到回复"这一层语义（K-025） |
| 启动装配 | 新 01 全篇（T 表）；boot_image 登记序在 00 定位（K-003） |
| 构建与工具链 | 新 01（C 侧 Makefile）+ 新 14（Rust 侧 Cargo/clippy/测试命令） |
| 跨模块接口与线格式 | 新 02 主（消息 union 成员 56 字节、字段表，K-024）；新 99 汇总表 |
| 错误路径 | 各 handler 首现即完整（新 09/10/11）；全 stage 错误码矩阵汇总在新 99（K-161） |
| 关闭与退出 | 新 10：SCHED 自身无退出路径（K-145）；被调度进程的退出链（STOP 三来路，K-104）；live-update 换身归 03-stage-rs（边界声明） |
| 并发与同步 | 新 03：单线程事件循环模型（K-036）；内核侧 BKL/SMP 归 01-stage-kernel/11/16 |
| 测试基建 | 新 14 全篇（K-150~152）；os/qemu-tests 现无 sched 场景（实测 grep），通电验收挂 E8 余项/E5(e) 的结论入该篇 |

---

## 4. 新目录

**组织型**：服务事件循环型（§1 判定）。骨架按 prompt §九第二条：为什么存在（00）→ 诞生与初始化（01）→ 消息接口（02）→ 循环（03）→ 核心数据结构（04）→ 策略词汇（05）→ 对内核的双向契约（06）→ 策略机制（07-08）→ 按场景分组的请求处理（09-12）→ 邻接客户端（13）→ 工程收尾（14）→ 附录（99）。

与旧目录最大的两个结构差异：**(a)** 内核契约上移到 handler 之前（旧 12 在末尾，handler 篇反复向前借用其结论）；**(b)** 五个 handler 按"共用出口在前（08），入口按复杂度排（09 最重、10 最轻、11 成对、12 自触发）"排列。

### 4.1 新篇章总表

| 新编号 | 文件名 | 一句话定位 | 分组 |
|--------|--------|-----------|------|
| 00 | 00-sched-overview.md | 为什么调度策略住在用户态：双层模型、本服务边界与全部篇章导航 | Ⅰ 定位与诞生 |
| 01 | 01-sched-birth-and-init.md | 从 boot 表里的一行登记到第一次收信之前 | Ⅰ |
| 02 | 02-sched-message-contract.md | 五种消息、谁能发、发什么、回什么 | Ⅰ |
| 03 | 03-sched-main-loop.md | 主循环的一轮：收、分、做、回，以及不回的三种情况 | Ⅰ |
| 04 | 04-schedproc-record-and-table.md | 调度记录表：字段、槽位生死、四道查表检查 | Ⅱ 数据与词汇 |
| 05 | 05-priority-timeslice-model.md | 16 级队列、nice、时间片、niced：策略的数字语言 | Ⅱ |
| 06 | 06-kernel-contract.md | SCHED 与内核的双向承诺：接管、下发、通知 | Ⅲ 对内核 |
| 07 | 07-pick-cpu.md | 给进程选核：三条规则与一本负载账 | Ⅲ |
| 08 | 08-schedule-process.md | 策略下发的唯一出口：掩码、-1、打包 | Ⅲ |
| 09 | 09-start-scheduling.md | 接管一个进程：三种填值、内核交接、坏核重试 | Ⅳ 场景处理 |
| 10 | 10-stop-scheduling.md | 释放一个进程：清什么、不清什么 | Ⅳ |
| 11 | 11-noquantum-nice.md | 两条降级入口：内核报警与用户改 nice | Ⅳ |
| 12 | 12-balance-queues.md | 五秒一轮的回升：降得快、升得慢 | Ⅳ |
| 13 | 13-clients-pm-rs.md | 消息的另一端：libsys 包装、PM 三种申请、RS 直发 | Ⅴ 协作与工程 |
| 14 | 14-tests-and-enablement.md | 81 个测试测到了什么、真机通电还差什么 | Ⅴ |
| 99 | 99-appendix-constants.md | 常量/消息/错误码/ARCH/模块地图速查 | 附录 |

### 4.2 阅读路径

- **主线**（理解一个请求的完整生命）：00 → 01 → 02 → 03 → 04 → 05 → 06 → 08 → 09 → 13。
- **策略支线**（MLFQ 与选核）：05 → 07 → 08 → 11 → 12。
- **可跳读**：99（字典）、14（工程收尾，需要 02/03/06 的结论）、10（最短，随时读完）。

### 4.3 并行体说明

五个消息处理是并行入口（不存在必然先后），但共享两条汇聚线：下发出口（新 08）与信任门（新 02）。新目录按 §5.2 规则先给统一框架（02/03/06/08），再按触发者分组处理篇（09/10 用户来电、11 双源、12 自触发），组内以 do_start（最重、调用面最广）为代表性成员讲透，其余以对照/回指收束。

### 4.4 序差表（运行时序 vs 教学序）

| # | 运行时序事实（锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---------------------|-----------|------|-------------|
| D-1 | 消息到达顺序由触发方决定：PM 的 START 最早（pm/schedule.c:37），NO_QUANTUM 稍后（proc.c:1887） | 契约与机制篇先行（02/06/08），handler 后至（09-12） | 被调方先讲，调用点只留锚（首现即完整） | 09/10/11 篇首时间线一句 |
| D-2 | init_scheduling 在启动段执行（main.c:133），balance_queues 五秒后才跑 | 拆两篇：新 01 讲"设闹钟"，新 12 讲"响铃干活" | 同一循环周期被生命周期阶段切断 | 新 12 §1 回指新 01；新 01 §末预告 |
| D-3 | 循环内 CLOCK 分支直接调 balance_queues（main.c:47-48） | 新 03 只讲分支形状（无条件调用、不回复），平衡内容归新 12 | 循环完整性不依赖平衡语义 | 新 03 分支表标"→12"；新 12 首段回指 |
| D-4 | pick_cpu 最先被 do_start 用到（schedule.c:226），但 schedule_process 每次调用也先跑它（:302） | 新 07 在新 08/09 之前 | 共用机制唯一归属 | 新 08/09 各回指一句 |
| D-5 | 客户端调用（libsys/PM/RS）在时间上早于服务器处理任何逻辑 | 新 13 放在全部 handler 之后 | 先懂服务端才能读懂客户端的短路和字段 | 新 09/10/11 的"来路"句反向指针 |
| D-6 | 内核侧 do_schedule/sched_proc 在每次下发的下游同步执行 | 新 06 上移至 handler 前，队列内部回指 01-stage-kernel/11 | 服务器篇需要"内核对端承诺"做地基 | 新 08/09/11 前置含新 06 |
| D-7 | START 与 INHERIT 在循环层 case 贯穿、运行时无先后 | 新 09 合一篇内分节 | 双入口是同一语义单元 | 无 |

---

## 5. 每篇契约

约定：所有契约的正文模板沿用旧目录七章制（概念 → C 源码分析 → Rust 设计决策 → 实现详解 → 测试要点 → 过渡 → 参见；即 K-164/165 惯例），每篇 200-400 行为常规（用户约束：软上限，单一概念确属复杂可放宽；本 stage 无一篇需要 3000 行级例外——最重的新 09 预计 350-450 行）。Rust 设计决策一律带 `[ARCH S-n]` 标注并保持三处一致纪律；全部"工具生成"锚点按 §0.3 漂移表重定位。

### 00-sched-overview

- **一句话定位**：回答"SCHED 是什么、为什么不是内核在做决定、这个 stage 按什么顺序读"。
- **讲什么**：K-001（双层模型主述）、K-002（隔离/演化论据）、K-003（启动因果链位置）、K-004（五表一致性定位）、K-005（导航+三条阅读路径）、K-006（一次请求生命周期心智图）、K-162（ARCH 总表索引位）。
- **不讲什么**：任何机制细节（SEF 回调→01；消息字段→02；循环→03；handler→09-12）。内核对端队列机制去向 `../01-stage-kernel/11-scheduling-primitives.md`；IPC 线格式 internals 去向 `../13-stage-ipc/`（概念层提及一次即可）。
- **前置**：无（全 stage 起点；假设读者已读完 01-stage-kernel 与 03/04-stage 概览）。
- **后置**：全部篇章引用其分层图。
- **事实底线**：`main.c` 整体形态（138 行、无阻塞于 I/O 的设备交互）；`system.c:642-700`（执行面）与 `schedule.c`（策略面）的分工；`kernel/table.c:56` boot 登记；Rust `os/servers/sched/`（bin+lib 双目标、21 文件 4525 行实测）。
- **知识点清单**：K-001~006、K-162（来源=旧 00/99 核心点 + 01 §1.1 上收；K-005/006 来源=旧 plan.md §3 + draft/00）。
- **验收标准**：读者能画出"PM/RS→SCHED→内核"与"内核→SCHED"两箭头信息流并标注消息名；导航表与实际篇章一一对应；对"为什么不用内核内策略"给出至少两条带锚点的论据（可演化：schedule.c:348-352 作者注释；故障隔离：main.c:22-96 全篇无硬件访问）。

### 01-sched-birth-and-init

- **一句话定位**：SCHED 进程的诞生段——从 boot 表一行登记到进入主循环之前，每一步失败都 panic 的启动哲学。
- **讲什么**：K-010（SEF 注册）、K-011（getmachine+panic）、K-012（machine 结构用到什么）、K-013（init_scheduling 设闹钟）、K-014（先注册后循环结构 + setreply 死声明脚注）、K-015（Makefile 构建制品）、K-016（Rust 启动装配决策）。
- **不讲什么**：循环体（→03）；消息是什么（→02，本篇只允许列名字）；平衡机制（→12，只讲"设了只闹钟"）；RS 如何加载镜像（→03-stage-rs 主权）；machine 结构全字段（→01-stage-kernel boot 期）。
- **前置**：00。跨 stage 前置：SEF 概念首现于 ../01-stage-kernel/（boot 完成后首个用户进程）；引用而不重讲。
- **后置**：03（接手循环）、12（接手闹钟的另一半）、07（machine 数据的消费方）。
- **事实底线**：`main.c:22-32`、`:111-121`、`:126-136`；`schedule.c:334-342`（:338 乘 sys_hz、:340 sys_setalarm）；`servers/sched/Makefile` 全文；`kernel/table.c:56`；Rust `src/main.rs`（58 行，含 owned-by 注释 3 处）、`src/sef.rs`（93 行）。
- **知识点清单**：上列 7 条（来源见池表），另含 1 条新增：启动期失败策略归纳（panic 三连：getmachine 失败 main.c:131、setalarm 失败 schedule.c:341、收信失败 main.c:40——"启动约定被破坏就死给你看"）。
- **验收标准**：T-01~T-06 六步每步带锚；读者能回答"如果 sys_getmachine 失败会发生什么、为什么不是降级继续"；对照 Redox 服务器的 init 失败处理（概念级）；machine/ncpu 字段仅"取用清单"不展开。

### 02-sched-message-contract

- **一句话定位**：SCHED 的门面契约——五种消息的编号、载荷、发送资格、信任旗标、回复规矩，一张表查清。
- **讲什么**：K-020（调用号族）、K-021（通知 vs 调用）、K-022（FROM_KERNEL 信任门主述）、K-023（白名单主述）、K-024（四消息体+回复体逐字段与 56B 线形状）、K-025（_taskcall 同步往返语义）、K-026（no_sys）、K-029（Rust 消息建模决策 D1-D6 平移）。
- **不讲什么**：循环怎么转（→03）；各 handler 内部检查顺序（→09/10/11，本篇只给"资格规则"）；槽位检查函数（→04）；trap 汇编层与内核 IPC 投递（→../01-stage-kernel/12-ipc-core.md、13-syscall-dispatch.md，边界声明）；minix-types 消息结构的 Rust 实现细节（→新 99 表 + 新 03 决策区）。
- **前置**：01（SEF 同步调用语义）。
- **后置**：03（分发用本篇资格规则）、09/10/11（handler 检查复用）、13（客户端组装本篇消息的镜像面）。
- **事实底线**：`com.h:801-807`（SCHEDULING_BASE 0xF00 与五偏移）、`com.h:92`（is_ipc_notify）、`ipcconst.h:28-30`（IPC_FLG_MSG_FROM_KERNEL）、`main.c:68-84`（旗标门与伪造举报 printf 原文）、`utility.c:61-74`（accept_message 逐行：仅 PM/RS 两枚来源）、`ipc.h:2568-2569/2612`（union 成员）及 1440-1444（stop 单字段+padding 52）、1822-1827（set_nice 双字段+padding 48）；start/inherit 共用 `mess_lsys_sched_scheduling_start`（字段清单：endpoint/parent/maxprio/quantum，回复体 m_sched_lsys_scheduling_start.scheduler——B 相逐字段带行号，旧引 ipc.h:1432-1444 区段重定位）。Rust：`dispatch.rs`（165 行）、`minix-types/src/ipc/message.rs`、`com.rs`。
- **知识点清单**：8 条 + 1 新增（消息×资格×旗标×回复的 5 行总矩阵——三主题首次合成，原料 K-020/022/023/027）。
- **验收标准**：矩阵能被读者独立复推；"为什么白名单不给 NO_QUANTUM 用"有_proc.c:1887_ 发送端证据；每条线格式断言带 ipc.h 行号；对照 Linux syscall 表 / seL4 端点权限（概念级各一段）。

### 03-sched-main-loop

- **一句话定位**：一轮循环的四拍（收/分/做/回）与"不回"的三种情况，以及这个循环为什么不需要锁。
- **讲什么**：K-030（循环走查唯一主述）、K-031（四入口与 case 贯穿）、K-032（NO_QUANTUM 恒不回复）、K-033（SUSPEND 约定主述）、K-034（CLOCK 分支形状）、K-027（回复规则主述）、K-035（SchedServer/run_once/双 trait [ARCH S-11]）、K-036（单线程事件循环并发模型）。
- **不讲什么**：每个 case 的 handler 内部（→09-12，分支表各留一行行为摘要+指针，序差 D-3）；accept_message 检查逻辑（→02，已教）；setreply 多进程攒回复模式（本服务 reply 即时、不适用——一句话说明，概念主权 ../01-stage-kernel/12-ipc-core.md）；内核通知投递机制（→01-stage-kernel）。
- **前置**：01、02。
- **后置**：09/10/11/12（handler 篇全部回指本篇分支表）、14（run_once 是测试驱动点）。
- **事实底线**：`main.c:34-94`（L 表全部行号）、`:101-106`（reply 函数）、`com.h:1151`（SUSPEND）；Rust `server.rs`（1133 行，SchedServer 单所有者、run_once；模块头 ARCH S-11 声明与 todo §1.2 八条执行侧语义）。
- **知识点清单**：8 条（K-035 来源=旧 02 §3 D7-D9 迁入；K-036 新增，来源=server.rs 模块头声明 + AGENTS 执行模型约束）。
- **验收标准**：L-01~L-06 每步锚点齐全；"三种不回复"（通知 continue / NO_QUANTUM continue / SUSPEND 约定）列全并各带锚；读者能回答"为什么 rc/RefCell 在这个 crate 合法而同样的类型放进 kernel crate 是 P0"；含一张一轮循环时序图。

### 04-schedproc-record-and-table

- **一句话定位**：服务器的私有账本——schedproc 每个字段、槽位从登记到清零的一生、查表前必过的四道检查（旧 03+04 两篇合并）。
- **讲什么**：K-040（八字段逐个）、K-041（IN_USE 与整字清零）、K-042（cpu_mask 消除 [ARCH S-3]）、K-043（_MAIN/CONFIG_MAX_CPUS 模式）、K-044（表本体）、K-045（isokendpt 四检查顺序）、K-046（isemtyendpt 镜像）、K-047（错误码三兄弟分工）、K-048（Rust 表设计决策合并平移）。
- **不讲什么**：字段怎么被 handler 填（→09）；accept_message 白名单（→02 主述，旧 04 §2.3 迁出）；endpoint↔proc 换算的 IPC 原理（→../01-stage-kernel/12-ipc-core.md，引用）；内核 p_rts_flags（→01-stage-kernel/11）。
- **前置**：01（知道表在 fresh 前清零依赖 _MAIN 语义）、02（endpoint/消息概念）。
- **后置**：05（两个优先级字段的语义展开）、09/10/11（读写字段的全部消费方）、12（负载账的槽位来源）。
- **事实底线**：`schedproc.h:9-40` 全文件（结构 23-36、表 36、IN_USE 39、_MAIN 9-12、CONFIG_MAX_CPUS 14-16）、`schedule.c:132`（flags=0）、`utility.c:29-56`（两个检查函数逐行）、`com.h:63-64`（SCHED_PROC_NR/NR_PROCS 关系）；Rust `schedproc.rs`（158）、`table.rs`（153）、`valid.rs`（69）。
- **知识点清单**：9 条（来源=旧 03 全篇 + 旧 04 除 §2.3/§1.5）。
- **验收标准**：八字段表每字段有"谁写/谁读/清零时机"三列且全部可锚（写者仅 09/11 的 handler 与 07 的 cpu 字段）；四道检查顺序图与 utility.c 行号一致；"为什么 flags=0 而不是 &=~IN_USE"给出 schedproc.h:39 单旗标论证；合并后全篇不超过 400 行（两旧篇 399 行的信息密度允许达成）。

### 05-priority-timeslice-model

- **一句话定位**：策略的数字语言——16 级队列、上下限双字段、毫秒时间片、nice 换算、niced 判定，一次讲清。
- **讲什么**：K-050（常量族）、K-051（上限 vs 现值）、K-052（毫秒单位 [ARCH S-6] + 双 200 双源）、K-053（nice 换算公式与钳位）、K-054（is_system_proc）、K-055（niced 谓词与内核记账链，含 E-SCHEDNICED 已修状态）、K-056（`>` vs `>=` 双侧差异）、K-057（Rust 模型决策）。
- **不讲什么**：字段的存储（→04）；谁在什么时候改优先级（→11/12）；内核对 priority 的消费（队列插入细节 →01-stage-kernel/11，本篇只讲 sched_proc 入口检查那一行）；PM 侧 getpriority 系统调用（→../04-stage-pm/16-scheduling.md 主权）。
- **前置**：04。
- **后置**：07（is_system_proc 消费）、08（niced 随每条消息）、09-12（全部 handler 的词汇表）。
- **事实底线**：`config.h:66-77`（全族逐行）、`schedule.c:18/37/41/44/319`、`pm/utility.c:91-101`（换算+双向钳位）、`system.c:645`（`>` 号实测）、`arch_clock.c:315-319`（CP_NICE 消费）；Rust `priority.rs`（288 行）。
- **知识点清单**：8 条。
- **验收标准**：常量表七行全带 config.h 行号；nice→queue 公式给数值例（nice=-1/0/1 各落哪级）；"16 能过内核对端却被服务器拒"作为双侧不对称案例明写；MAX_USER_Q==TASK_Q==0 与 E-PREEMPTFLAG 的历史坑（priority!=0 近似为何错）交代为约束条目（代码状态引用 edge1 K1 ✅）。

### 06-kernel-contract

- **一句话定位**：服务器的地基层——内核向 SCHED 承诺什么、SCHED 能假设什么（schedctl 接管、schedule 下发、notify 报警三个面的对端行为）。
- **讲什么**：K-060（归属两状态）、K-061（do_schedctl 检查序+两分支）、K-062（KERNEL 旗标与参数先读后写）、K-063（do_schedule 三检查）、K-064（sched_proc 检查与写入节拍：出队/写/MF_NICED/回队/-1 跳过/迁移）、K-065（notify_scheduler 构造全程）、K-066（7 字段全发只读来源）、K-067（proc_no_time 两分支）、K-068（PREEMPTIBLE 位与模板，锚点修正版）、K-069（与 01-stage-kernel/11 的边界）、K-070（Rust 接缝决策）。
- **不讲什么**：就绪队列 enqueue/dequeue 内部与 RTS 位图（→../01-stage-kernel/11-scheduling-primitives.md §2.2-§2.5，一句话回指）；服务器侧 handler（→09-12）；sys_schedule/sys_schedctl 的 wrapper 实现（→13 客户端篇一句话带过，本篇讲对端语义）；SMP 迁移全机制（→01-stage-kernel/16-smp.md，仅 sched_proc 的迁移调用归本篇）。
- **前置**：02（NO_QUANTUM 旗标）、04（endpoint 语义）、05（priority/quantum/niced 词汇）。
- **后置**：08（下发的另一端）、09（接管的另一端）、11（报警的另一端）、14（契约测试的 mock 依据）。
- **事实底线**：`do_schedctl.c:16-43`（47 行全文件，plan.md"49 行"作废）、`do_schedule.c:14-29`、`system.c:642-700`（sched_proc 完整行界，642-723 作废）、`proc.c:1860-1891`、`proc.c:1893-1910`、`proc.h:50-55/178-179`、`const.h:143`、`minix/include/minix/priv.h:45-50`；Rust `kernel_api/schedctl.rs`（217）、`kernel_api/schedule.rs`（256）、内核侧 `os/kernel/src/syscall.rs` dispatch_schedctl/dispatch_schedule、`sched.rs` sched_proc。
- **知识点清单**：11 条（旧 12 全篇 9 条 + 旧 09 §2.4/2.5 迁入 2 条）。
- **验收标准**：三个面（接管/下发/报警）各一张双侧序列图（服务器调用点↔内核行号）；"服务器能假设什么"清单至少四条（p_scheduler 独占写权、旗标可信、时间片耗尽必达（PREEMPTIBLE 前提）、-1 保持语义）；与 01-stage-kernel/11 的重复度自检（队列内部操作零展开，只引用）。

### 07-pick-cpu

- **一句话定位**：第一次选核与它的三本账——三条有序规则、cpu_proc 台账、以及"死核"如何被永久排除。
- **讲什么**：K-075（三规则顺序）、K-076（台账三点写位+零初始化修正）、K-077（CPU_DEAD 与无符号恒真陷阱）、K-078（编译开关退化 [ARCH S-5]）、K-079（Rust 决策）。
- **不讲什么**：运行中换核（sched_proc 的迁移臂 →新 06；跨 CPU 迁移全机制 →../01-stage-kernel/16-smp.md）；EBADCPU 重试环的完整故事（→09，本篇只给死核标记的数据结构）；machine 结构全字段（→01 已交代的取用清单）。
- **前置**：01（machine）、04（sp_cpu 字段）、05（is_system_proc）。
- **后置**：08（每次下发先重 pick 的消费方）、09（重试环）。
- **事实底线**：`schedule.c:37-46`（CPU_DEAD 宏、cpu_is_available、cpu_proc 定义）、`:48-81`（函数全体的 54-57/60-63/66-75/77/78-80 五段）、`schedproc.h:14-16`、`type.h` 的 machine 消费段（旧引 122-125，B 相重定位后带行号）；Rust `cpu.rs`（222 行）；`edge_todo.md` E-SCHEDSMP 与 `edge1.md` K2（2026-09-18 三环闭单——重试环生产可达性已恢复，文档不得再写"永不触发"）。
- **知识点清单**：5 条。
- **验收标准**：三规则顺序图 + 每条规则的"为什么不能换序"一句；恒真陷阱给 C 表达式原文与后果推演（单核/多核各一）；台账三写位（++/--/=CPU_DEAD）与读者对账练习。

### 08-schedule-process

- **一句话定位**：策略离开服务器的唯一出口——掩码挑字段、-1 表保持、niced 随行、sys_schedule 打包，一处定义三处引用。
- **讲什么**：K-082（掩码族主述）、K-083（-1 语义）、K-084（每次重 pick 约束）、K-085（打包发送与 "PM:" 笔误）、K-086（命名对照）、K-087（Rust 决策）；新增一条：LOCAL/MIGRATE/ALL 三种掩码组合与三个调用者的对照表（do_noquantum/do_nice→LOCAL，do_start→ALL，balance→LOCAL；stop 不下发——缺口 G 系列证据合成）。
- **不讲什么**：调用者的业务语义（→09/10/11/12）；内核对端收到后做什么（→06 已教，回指）；sys_schedule 的 wrapper 层（→13 一句）。
- **前置**：05（niced）、06（sched_proc 对端）、07（pick_cpu）。
- **后置**：09/10/11/12（全部下发方）。
- **事实底线**：`schedule.c:22-35`（宏定义全段）、`:297-328`（函数全体）、`system.c:680-688`（内核侧 -1 跳过的对应行）、`libsys/sys_schedule.c`；Rust `kernel_api/schedule.rs`（wire_niced :152-154 现状）。
- **知识点清单**：6 条（旧 09 除 §2.4/§2.5 迁 06 外全保留）。
- **验收标准**：掩码×调用者×效果三列表与 C 行号一致；"为什么 -1 能当哨兵"（内核侧消费证据 system.c:680-688）双向讲通；命名对照表（schedule_process/sched_proc/do_schedule 三者）出现且与 06 一致。

### 09-start-scheduling

- **一句话定位**：全 stage 最重的一次交互——START/INHERIT 双入口如何在十步内完成"验人、填表、要权、下发、回执"，以及坏核自愈环。
- **讲什么**：K-090（断言）、K-091（检查顺序+部分写入后的边界检查）、K-092（三填值方式）、K-093（init 覆盖细节主述）、K-094（schedctl 交接+置标记）、K-095（EBADCPU 重试环）、K-096（fork 次主线路径图）、K-028（scheduler 字段写入侧）、K-097（Rust 决策）。
- **不讲什么**：客户端怎么发出 START（→13）；sys_schedctl 内核内部（→06）；pick_cpu 细节（→07 回指）；schedule_process 内部（→08 回指）；INHERIT 的父槽检查函数本体（→04 回指）。
- **前置**：02/03/04/05/06/07/08 全链路——本篇是全 stage 机制的第一次合成。
- **后置**：13（客户端镜像）、14（start.rs 测试簇）。
- **事实底线**：`schedule.c:140-249`（H 表十步逐行）、`main.c:58-61`（case 贯穿）、`do_schedctl.c:40-43`（注册分支对端）、`ipc.h` start 结构；Rust `scheduling/start.rs`（330 行，含 0..15 放行门 :116——E-PREEMPTFLAG 坑位关联）、`server.rs` 重试环（:391-399 区段现状按 E-SCHEDSMP 闭单后代码复核）。
- **知识点清单**：9 条 + 掩码合成图 1 新增（并入 K-096 图）。
- **验收标准**：H 表①-⑨ 每步带行号且顺序不可换的因果讲通（至少：为什么 isemtyendpt 在填字段前、边界检查却在填之后——C 瑕疵明写不美化）；fork 路径图覆盖 PM fork→INHERIT→抄父→回复四站；"init 为什么值相等却必须双源"一段完整；重试环终止性论证（死核数单调增且有界）。

### 10-stop-scheduling

- **一句话定位**：登记的逆操作——两道检查、两件释放、一样不释放的东西，以及"谁保证每个槽位最终都被释放"。
- **讲什么**：K-100（两道检查）、K-101（两释放+单核 ifdef 事实）、K-102（不清 p_scheduler 的隐式链）、K-103（对称表）、K-104（exit 三来路）、K-145（SCHED 无退出路径——"关闭与退出"固定项的落点）、K-105（Rust 决策）。
- **不讲什么**：sched_stop wrapper 短路细节（→13 主述，本篇来路表引用）；进程在内核里的消亡（→../01-stage-kernel/17-syscall-process.md）；live-update 换身（→03-stage-rs/16）。
- **前置**：04（flags 清零语义）、08（掩码对照：stop 是唯一不下发的 handler——缺口补全点）、02（白名单）。
- **后置**：13（三来路的发送端）、09（对称表另一半）。
- **事实底线**：`schedule.c:112-135`（全函数）、`schedule.c:34-35`（SCHEDULE_MIGRATE 宏定义但全文件零使用——实测 pick_cpu/schedule_process 均未用该宏，属 C 死宏，B 相复核后按死代码消除处理）；Rust `scheduling/stop.rs`（168 行）。
- **知识点清单**：7 条 + 1 新增（"STOP 不下发内核"与 K-102 合并论证：槽位清零与内核 p_scheduler 残留的一致性依赖进程消亡或下次 START 覆盖——锚 do_schedctl.c:40-43）。
- **验收标准**：对称表左右各≥5 行；三来路各带调用点锚；读者能回答"进程死了但 STOP 丢了会怎样"（槽位泄漏路径+现实中为何罕见：flags=0 依赖来电，PM exit 失败仅 warn——forkexit.c:425 证据）。

### 11-noquantum-nice

- **一句话定位**：同一个降级的两个入口——内核报警（免白名单、不回复、失败静默）与用户改 nice（白名单、回复、失败回滚），信任不对称的活教材。
- **讲什么**：K-110（信任不对称应用位点）、K-111（do_noquantum 全链）、K-112（do_nice 五步含回滚）、K-113（SET_NICE 来路一句+指针 13）、K-114（Rust 决策含"发送者检查外移"——注意 C do_nice **有** accept_message :261-263，Rust 外移属结构调整，须与 K-022 的 C 事实区分，旧篇此处易被读成"C 没有检查"）。
- **不讲什么**：FROM_KERNEL 旗标本体（→02 已教，回指）；schedule_process/LOCAL 掩码内部（→08 回指）；proc_no_time 何时触发（→06 已教）；PM setpriority 系统调用面（→04-stage-pm/16 主权）。
- **前置**：02/05/08。
- **后置**：12（降级的账由平衡来还）、13（NICE 消息发送端）、14（两 handler 测试簇）。
- **事实底线**：`schedule.c:87-107`、`:254-292`、`main.c:68-84`（两篇共用主述锚）、`pm/schedule.c:89-112`；Rust `scheduling/noquantum.rs`（99）、`nice.rs`（161）、`dispatch.rs`（检查外移落点）。
- **知识点清单**：5 条 + 1 新增（两 handler 七维对照表：触发者/信任门/槽位检查/降级动作/下发掩码/失败处理/回复语义——表内每格带锚，作为"首现即完整"的收束件）。
- **验收标准**：对照表 7×2 全格有锚；回滚的三段式（快照/双写/失败还原）行号级讲通；"do_noquantum 失败为什么只 printf"给 main.c:72-76 证据与后果分析（进程仍被内核降过级吗——否：sys_schedule 未发，进程保持原队列但已出队重入？此问必须在新 06 的 sched_proc 消费证据基础上答对）。

### 12-balance-queues

- **一句话定位**：MLFQ 的另一半——每五秒一次的慢速回升，防振荡节奏与"策略将来会换"的扩展位。
- **讲什么**：K-120（三数换算）、K-121（扫描判据）、K-122（不对称防振荡）、K-123（闹钟三拍循环）、K-124（LOCAL 应用）、K-125（可换策略位）、K-126（回升下发不检查返回值——新增约束）、K-007 处理规则示范（对照小节按 §3.5 重写，删 RSoC 外链）。
- **不讲什么**：sys_setalarm/时钟源（→../01-stage-kernel/15-clock-timer.md）；循环里的 CLOCK 分支本体（→03 回指）；降级的触发侧（→11，已教）。
- **前置**：03（分支形状）、05（上下限词汇）、08（LOCAL 掩码）、11（降级先行发生）。
- **后置**：13（无）、14（balancer 测试簇）。
- **事实底线**：`schedule.c:16-18`（balance_timeout/BALANCE_TIMEOUT）、`:334-342`（回指 01）、`:348-369`（注释+函数全体，:358-363 扫描、:362 不检查、:367-368 重设 panic）、`sysutil.h:60`（sys_hz）；Rust `balancer.rs`（138 行，策略结构体 D4）。
- **知识点清单**：8 条。
- **验收标准**：一张"设→响→重设"闭环图（跨 01/03/12 三篇的锚拼合，作为回指补偿件）；"降快升慢为什么防振荡"给极值推演（连续降级 15 次 vs 回升节奏）；:362 静默失败的后果明写并与 :340/:367 的 panic 处理对比（同为失败、两种态度，为什么）。

### 13-clients-pm-rs

- **一句话定位**：消息的另一端——libsys 三个 wrapper 的短路艺术、PM 的三种申请与生死来回、RS 的六参直发（旧 13+14 合并）。
- **讲什么**：K-130~K-144 全部 15 条（含 K-143 分工声明保留、K-144 归属声明保留）；代表性成员=PM（五种消息它涉及四种），RS 按差异表收束（只用 START/STOP、parent 恒自己、无继承）。
- **不讲什么**：服务端 handler（→09/10/11 回指）；PM 的 mproc 表与 fork 全流程（→04-stage-pm 主权）；RS 槽配置与加载（→03-stage-rs 主权）；minix-sys 通电状态（→14）；_taskcall 内部（→02 边界）。
- **前置**：02（消息形状）、09/10/11（服务端对每类消息做了什么——先懂服务端）。
- **后置**：14（客户端镜像测试）、00（fork 次主线图的素材来源之一）。
- **事实底线**：`libsys/sched_start.c` 全 99 行（断言群 18-22/62-67、短路 58-59/70-77、字段 80-84、回复消费 39/96）、`sched_stop.c` 全 31 行、`pm/schedule.c` 全 113 行、`pm/utility.c:91-101`、`pm/forkexit.c:102/425/438-441`、`pm/main.c:199/213/372-380`、`rs/utility.c:363-381`、`rs/type.h:92-95`、`rs/manager.c:461`、`rs/request.c:342`；Rust `client.rs`（293 行镜像）。
- **知识点清单**：15 条（两旧篇全量去重合并；旧 14 的 §1.6 对照表升级为本篇组织骨架）。
- **验收标准**：三路分岔图（NONE/KERNEL/用户态）在 sched_start 与 sched_stop 各一张；PM 与 RS 差异表≥6 行（生源/参数/断言/失败处理/继承/取消）；"申请失败 PM 拆台、RS 清理只 warn"的对比带 pm/forkexit、rs/manager 双侧锚；篇长≤450 行（两旧篇 429 行合并去重后应更短）。

### 14-tests-and-enablement

- **一句话定位**：这套文档讲的机制在 Rust 里"测到了什么、什么还只能对着 mock 演"——测试基建与真实通电的全景与缺口账。
- **讲什么**：K-150（mock/CannedTransport/81 基线/clippy 0）、K-151（E8 现状与 S27 余项）、K-152（E5(e) 冒烟三链设计）、K-153（四条 edge 的最新闭单状态——防止文档抄旧账）、K-154（owned-by 锚点同步纪律）、K-163（crate 模块地图）。
- **不讲什么**：各机制本身（全部回指 02-13）；E1 trap 层实现（→01-stage-kernel/edge E1）；内核测试体系（→01-stage-kernel）。
- **前置**：01-13 全 stage（本篇是合成位）；允许跳读（新读者可先只看 §1）。
- **后置**：无（stage 收尾；99 收字典）。
- **事实底线**：`os/servers/sched/src/tests 与模块内 #[cfg(test)]`（81 passed 基线，todo.md 记录 + E-MINTYPES-SYS 闭单验证 "sched 81"）；`os/libs/minix-sys/src/ipc.rs:529-559`（DirectTrapTransport EIO，行号按 edge_todo E8 引用时复核）、`syscall.rs:144-148`（DirectKernelCallTransport -EIO，同上）；`edge_todo.md` E8 段 :258-270（含 2026-09-16 trap 底座真机 PASS 记录）、`edge1.md:17-18`（K1/K2 2026-09-18 闭单）、`edge3.md:63`（S27 仍开放）；`os/qemu-tests/` 实测无 sched 场景（grep 无 sched 服务器级脚本）。
- **知识点清单**：6 条（全部新增/更新条目，池内已带锚）。
- **验收标准**：79/81 基线、mock 覆盖矩阵（五消息×失败臂）与源码实测一致；"通电还差什么"三行内可答（差 E8 余项的服务器参战场景化验证，前置 E1 底座已备）；edge 状态引用全部带日期与闭单标记，无一处抄旧账。

### 99-appendix-constants

- **一句话定位**：本 stage 的字典与总账——常量、消息、错误码、ARCH 偏离、模块地图，全部表格化，不载新论证。
- **讲什么**：K-160（常量速查：com.h/config.h/ipcconst.h/priv.h 族）、K-161（错误码矩阵：出现位置×触发条件×行号）、K-162（ARCH S-1..S-11 总表：含 S-3/S-5/S-6/S-11 各篇小表汇总）、K-163（Rust 模块地图：21 文件×行数×归属新篇号）、K-033/K-047 的词条镜像。
- **不讲什么**：任何机制叙述（每格一句话+指针；论证在所属篇）。
- **前置**：无（字典可独立检索；但表外概念一律指向篇章）。
- **后置**：无。
- **事实底线**：池表全部 C 锚点 + `plan.md §4`（ARCH 表素材）+ `wc -l os/servers/sched/src/*` 实测数据。
- **知识点清单**：4 条。
- **验收标准**：错误码矩阵覆盖 EPERM/EINVAL/EBADEPT/EDEADEPT/EBADCPU/ENOSYS/SUSPEND 且每行≥2 处使用锚；ARCH 表与 01-13 各篇 ARCH 小表三向对账一致；模块地图与 owned-by 注释一致（B 相同步后重跑 grep 验证）。

---

## 6. 变更表

操作五类：重排 / 拆分 / 合并 / 新建 / 归档。存量看去向、新增看来源（双方向规则）。

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源备注 |
|------|------|--------|--------|------|-----------|--------------|
| O-01 | 重写（归档+新建） | 00（20 行骨架） | 新 00 | 骨架无正文，导航与实际篇章脱节 | K-001~006、K-162 | 存量核心点全部有去向；素材另取 draft/00 与 plan §3（线索非结论） |
| O-02 | 拆分 | 01 | 新 01 + 新 03（+新 00 收 §1.1） | 一篇同时讲启动段与循环段，且循环走查与旧 02 整段重复 | K-010~016、K-030~036、K-001/002、K-033 | §2.2-2.6 并入新 03 唯一版本；D5 常量决策去新 05/99 |
| O-03 | 拆分 | 02 | 新 02 + 新 03（+新 14 收 mock 相关） | 消息版面篇夹带循环实现手册（D7-D9 越界，§3.4） | K-020~029、K-030~036 | §2.1-2.3/2.5 →新 03；D7/D8/D9 →新 03；§2.6/2.7 →新 02 |
| O-04 | 合并 | 03 + 04 | 新 04 | 结构定义与查表检查是同一账本的两面，两旧篇互为强制前置（04 前置含 03），合并消环 | K-040~048 | accept_message（旧 04 §2.3/§1.5）改归新 02 主述 |
| O-05 | 重排+增补 | 05 | 新 05 | 位置不变；增补双侧不对称与 edge 最新状态 | K-050~057 | 新增 K-056 显式化；E-SCHEDNICED/PREEMPTFLAG 状态按闭单更新 |
| O-06 | 重排（上移）+吸收 | 12 + 09 §2.4/§2.5 | 新 06 | 内核契约是全部 handler 的地基，旧目录放末尾造成 handler 篇系统性前向引用 | K-060~070 | 队列内部展开删回指（→01-11），不迁原文 |
| O-07 | 重排 | 10 | 新 07 | 选核须先于 schedule_process（每次重 pick）与 do_start（重试环） | K-075~079 | 修正 cpu_proc 初值（K-076）与 E-SCHEDSMP 旧状态 |
| O-08 | 重排+瘦身 | 09 | 新 08 | 下发出口须先于其三个调用者 | K-082~087 | §2.4/2.5 迁出到新 06；掩码三处重复归一 |
| O-09 | 重排 | 06 | 新 09 | handler 组内以机制依赖排（需 06/07/08 全部就位） | K-090~097、K-028 | K-093 主述权自旧 10 §1.1 收拢 |
| O-10 | 重排+增补 | 07 | 新 10 | 释放比降级/改 nice 更先讲：它只依赖 04/08，且解释"槽位为何可复用" | K-100~105、K-145、K-113 | 新增 K-145（无退出路径）与死宏证据（schedule.c:34-35 schedule_process_migrate 零调用，实测 grep） |
| O-11 | 重排+增补 | 08 | 新 11 | 两个降级 handler 同篇保留（旧组合合理），补七维对照表 | K-110~114 | FROM_KERNEL/掩码段落改回指（主述在 02/08） |
| O-12 | 重排+修订 | 11 | 新 12 | 平衡须在新 11（降级）之后：回升的"账"先于"还" | K-120~126 | 删 RSoC 外链（§3.5）；新增 K-126 |
| O-13 | 合并 | 13 + 14 | 新 13 | 两篇同为"消息发起方"，旧 14 仅 4 个 C 锚点不支撑独立一篇；PM 为代表成员、RS 差异表 | K-130~144 | 旧 14 §3 的 rs 实施决策主权移交 03-stage-rs（归属声明保留） |
| O-14 | 新建 | —— | 新 14 | 测试基建与通电现状无承接篇（缺口 G-2；固定清单"测试基建"项） | K-150~154、K-163 | 新增来源：各篇 §5/§5.1 上收 + edge_todo E8/E5(e) + edge1/edge3 实测 + wc 统计 |
| O-15 | 重写（归档+新建） | 99（20 行骨架） | 新 99 | 速查层缺失；核心点五条已被 00 与各篇吸收 | K-160~163 | 存量核心点去向：双层模型→00、五表→00、常量/消息→99 表、边界→00 导航 |
| O-16 | 归档 | 旧 00-14、99 全部 16 篇 | archive/（B 相执行，不删除） | 内容被新目录逐节吸收（§8.1 迁移表全覆盖） | 全部 | 归档目录若为 `archive/` 需带 rerank 迁移说明索引 |

**明确删除项（3 条，含理由）**：
1. 旧 11 §1.6 Redox RSoC 2026 新闻外链引用——删除。理由：仓库内不可验证的外部时效信息作事实论据，违反锚点纪律；概念对照层保留。
2. 旧 03/04 等篇的"（工具生成）"错误锚点（如 `schedproc.h:CONFIG_MAX_CPUS（L36）`）——不迁移，B 相按 §0.3 实测重定位。理由：符号解析错误（L36 实为表声明行），照抄即固化坏锚。
3. 旧 plan.md §6.1 状态表对 11-14 篇的 pending 标记——过时（四篇实际存在 197-217 行成文）。B 相重写 plan.md 时一并修正，不属于文档正文迁移范围。

---

## 7. 缺漏新篇（固定清单逐项落实，不留待定）

步骤 3 的缺口在此收口。每条：主题 / 为什么重要 / 原料 / 归哪篇 / 验收。

1. **服务总览篇实体化（G-1a）**——00 是读者进入 stage 的唯一地图，骨架状态使全部"先总后分"约定落空。原料：旧 00/99 核心点、旧 01 §1.1、plan §3 分组表。归新 00。验收：见 00 契约。
2. **全局字典篇实体化（G-1b）**——错误码与常量散落 14 篇造成重复与漏项。原料：池表全部锚点、plan §4 ARCH 表。归新 99。验收：见 99 契约。
3. **测试基建与真实通电（G-2，固定清单"测试基建"）**——81 测试全在 mock 上、生产通电为 E8 余项，这是本 stage 结论成立范围的边界，必须成文防止"文档=能跑"误读。原料：各篇 §5、edge_todo E8/E5(e)、edge1 K1/K2、edge3 S27、Cargo/clippy 命令面。归新 14。验收：见 14 契约。
4. **构建制品（G-3，固定清单"构建与工具链"）**——Makefile 揭示"服务器只链 libsys、一切内核往来走 wrapper"，是 13 篇客户端分工的因果根。原料：servers/sched/Makefile、Cargo.toml。归新 01（C 侧一段）+ 新 14（Rust 侧一段）。验收：读者能答"SCHED 为什么自己不构造消息字节"。
5. **线格式与同步往返（G-4，固定清单"跨模块接口与线格式"）**——56 字节 union 与 _taskcall 阻塞语义在旧文只侧写。原料：ipc.h 段、sched_start.c:30/87。归新 02。验收：02 契约字段表。
6. **并发模型显式化（G-5，固定清单"并发与同步"）**——无锁正确性的判据需要唯一出处。原料：server.rs 声明、AGENTS 执行模型。归新 03。验收：03 契约。
7. **退出路径声明（G-7，固定清单"关闭与退出"）**——SCHED 无退出是事实而非疏漏，须明写并交代 live-update 归属。原料：main.c:34-95、03-stage-rs/16 指针。归新 10。验收：10 契约。
8. **balance 静默失败（G-6）与 cpu_proc 初值（G-8）、setreply 死声明（G-9）、schedule_process_migrate 死宏（新 10 实测）**——四条 C 源码级小缺口，各归所属篇一段（12/07/01/10），不另立篇章。验收：四行进入新 99 的"瑕疵与死代码登记表"（新增表，池 K-161 扩展）。
9. **镜像与内存布局、汇编入口（固定清单两项）**——判定不在本 stage（§3.6 已给理由），由新 01/02 的边界声明承接，验收即边界句存在。
10. **链接与加载、启动装配、错误路径、trap 入口**——分别归新 01、新 01、新 99+各篇、新 02（边界指向 01-stage-kernel）。至此固定清单十项全部有"在哪里讲"的显式答案（§3.6 表为总账）。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（逐节覆盖；"重写再生"= 位置有主、文字按新契约重写，内容不丢）

**旧 00（20 行）**：核心点 5 条 → 新 00 §1（双层模型/边界）、新 99 §表（常量/消息）、新 00 §3（职责边界）；边界节 → 新 00 导航。全部重写再生。

**旧 01**：§1.0/§1.7 → 各归新 01 §1.0/§1.x 再生；§1.1 → 新 00 §1（主述，改写）；§1.2 → 新 01 §1；§1.3 → 新 01 §1；§1.4 → 新 00 §2（心智图）+新 03；§1.5 SUSPEND → 新 03 §1（主述）+新 99 词条；§1.6 对照 → 新 01/新 03 对照节（改写）；§2.1 → 新 01 §2；§2.2-2.5 → **新 03 §2（合并唯一版本）**；§2.6 → 新 03 §2；§2.7/2.8 → 新 01 §2；§3 D1-D4 → 新 01 §3；D5 → 新 05 §3+新 99；ARCH 总表 → 新 01 ARCH 节+汇入新 99；§4/§5/§6/§7 → 新 01 与新 03 各自 §4/§5，§6/§7 再生。

**旧 02**：§1.0/1.7/§6/§7 → 再生；§1.1 → 新 00（并入 K-002）；§1.2 → 新 02 §1；§1.3 → 新 02 §1；§1.4 → 新 02 §1（主述）；§1.5 回复三态 → 新 02 §1（规矩）+新 03 §2（执行）；§1.6 → 新 02 对照；§2.1/2.2/2.3/2.5 → **新 03 §2（与旧 01 重复段合并）**；§2.4 → 新 02 §2（主述）；§2.6 → 新 02 §2；§2.7 → 新 02 §2（扩 56B 字段表）；§3 D1-D6 → 新 02 §3；D7-D9 → 新 03 §3；ARCH 表 → 新 02/03 + 汇入新 99；§4/§5 → 新 02/03 拆分再生；§5.1 统计 → 新 14 §2 汇总+各篇保留。

**旧 03**：§1.1 → 新 04 §1；§1.2-1.6 → 新 04 §1/§2 对应字段；§1.7 对照/§1.8 → 新 04 再生；§2.1 → 新 04 §2；§2.2-2.6 → 新 04 §2（**全部"工具生成"锚重定位**）；§2.7 → 新 04 §2；§3 D1-D5 → 新 04 §3；§4/§5/§5.1 → 新 04 §4/§5+汇入 14；§6/§7 再生。

**旧 04**：§1.1-1.4 → 新 04 §1；§1.5 白名单 → **新 02 §1（主述迁出）**；§1.6/1.7 → 新 04 再生；§2.1/2.2 → 新 04 §2；§2.3 → 新 02 §2；§2.4 → 新 04 §2+汇入新 99 错误码表；§3 D1-D5 → 新 04 §3；§4/§5 → 新 04；§6/§7 再生。

**旧 05**：§1.1-1.7 → 新 05 同名各节（§1.2 → K-050；§1.3 → K-051；§1.4 → K-052；§1.5 → K-053；§1.6 → K-054）；§1.7 对照 → 新 05 再生；§2.1-2.5 → 新 05 §2（§2.5 增补 K-055 记账链与 K-056 不对称、E-SCHEDNICED 闭单状态）；§3 D1-D6 → 新 05 §3；§4/§5/§5.1 → 新 05 + 汇入 14；§6/§7 再生。

**旧 06**：§1.1 → 新 09 §1；§1.2 → 新 09 §1（K-091）；§1.3 → 新 09 §1（K-092/093，覆盖细节主述）；§1.4 → 新 09 §1 +对端回指新 06；§1.5 → 新 09 §1（K-095）；§1.6 → 新 09 §1（K-028 服务端侧）；§1.7 路径图 → 新 09 §1（K-096）+新 00 简版；§1.8/§1.9 → 新 09 再生；§2.1-2.8 → 新 09 §2 逐节对应；§3 D1-D6 → 新 09 §3；§4/§5 → 新 09；§6/§7 再生。

**旧 07**：§1.1-1.5 → 新 10 §1 对应；§1.6 三条来路 → 新 10 §1（K-104）+发送端新 13；§1.7/1.8 → 再生；§2.1-2.4 → 新 10 §2；§2.5 消息形状 → 新 02 §2（字段表吸收）；§2.6 来路发送端 → 新 13 §2（主述迁出）；§3 D1-D4 → 新 10 §3；§4/§5 → 新 10；§6/§7 再生。增补：新 10 收 K-145 与死宏段。

**旧 08**：§1.1 → 新 11 §1；§1.2 信任不对称 → 新 11 §1（定义回指新 02，主述在 02）；§1.3/1.4 → 新 11 §1；§1.5 局部下发 → 新 08（回指）；§1.6/1.7 → 再生；§2.1/2.2/2.3 → 新 11 §2；§2.4 掩码 → 新 08 §2（主述迁出）；§2.5 内核标记检查 → 新 02/新 03（主述迁出，本篇回指）；§2.6 NICE 来路 → 新 13 §2；§3 D1-D5 → 新 11 §3（D1 补"C 有 accept"事实校正）；§4/§5 → 新 11；§6/§7 再生。

**旧 09**：§1.1 两层下发 → 新 08 §1；§1.2 掩码 → 新 08 §1（主述）；§1.3 -1 → 新 08 §1；§1.4 niced → 新 08 §1（谓词回指 05）；§1.5 内核三检查四步 → **新 06 §2（迁出）**；§1.6 命名对照 → 新 08 §1；§1.7/1.8 → 新 08 再生；§2.1 → 新 08 §2；§2.2 → 新 08 §2；§2.3 → 新 08 §2；§2.4 do_schedule → **新 06 §2（迁出）**；§2.5 sched_proc → **新 06 §2（迁出，锚正 642-700）**；§2.6 对照表 → 新 08 §2；§3 D1-D5 → 新 08 §3（D4 随迁新 06）；§4/§5 → 新 08；§6/§7 再生。

**旧 10**：§1.1 临时值故事 → 新 09 §1（主述收拢，去重）；§1.2 三条规则 → 新 07 §1；§1.3 台账 → 新 07 §1；§1.4 死亡两种写法 → 新 07 §1；§1.5 编译开关 → 新 07 §1；§1.6/1.7 → 再生；§2.1 → 新 07 §2（增补初值事实）；§2.2 → 新 07 §2；§2.3 → 新 07 §2；§2.4 machine → 新 07 §2（锚重定位，标待验证）；§2.5 标死联动 → 新 09 §2（重试环主述）+新 07 引用；§3 D1-D4 → 新 07 §3；§4/§5 → 新 07；§6/§7 再生。

**旧 11**：§1.1-1.5 → 新 12 §1 对应；§1.6 对照（含外链）→ 新 12 对照节按 §3.5 删链改写；§1.7 → 再生；§2.1 → 新 12 §2；§2.2 → 新 01 §2（设闹钟）+新 12 回指；§2.3 → 新 12 §2（增补 K-126）；§2.4 掩码 → 新 08（主述）+新 12 回指；§2.5 闹钟链路 → 新 03 §2（主述）+新 12 图；§2.6 降级对照 → 新 11 §2（主述）+新 12 对照段；§3 D1-D4 → 新 12 §3；§4/§5 → 新 12；§6/§7 再生。

**旧 12**：§1.1-1.7 → 新 06 §1 对应（§1.6/2.5 抢占分支锚点改 `proc.c:1893-1910 + const.h:143 + priv.h:45-50` 三锚并引，并更新 Rust 已修状态）；§1.8 → 再生；§2.1-2.5 → 新 06 §2；§2.6 → 新 06 §2；§3 D1-D4 → 新 06 §3；§4/§5 → 新 06；§6/§7 再生。**锚点修正随行**：全篇 `main.c:52-98`→`35-94`、`642-723`→`642-700`、do_schedctl"49 行"→47 行。

**旧 13**：§1.1-1.7 → 新 13 §1 对应；§1.8/2.x → 新 13 §1/§2；§2.1-2.6 → 新 13 §2；§3 D1-D4 → 新 13 §3（PM 段）；§4/§5 → 新 13；§6/§7 再生。
**旧 14**：§1.1 → 新 13 §1（生源分工）；§1.2-1.6 → 新 13 §1 差异表；§1.7/1.8 → 新 13 再生；§2.1-2.6 → 新 13 §2（RS 段）；§3 D0-D3 → 新 13 §3 RS 段（rs 侧实施主权移交声明保留）；§4/§5 → 新 13/新 14；§6/§7 再生；头部"实现归属"段 → 新 13 头（原文保留并按新编号改写指向）。

**旧 99（20 行）**：核心点 → 新 00（模型/边界）+ 新 99（常量/消息/错误码表）；边界节 → 新 99 头再生。

### 8.2 引用迁移表（全仓对旧文件名/编号的引用）

Rust 源码 owned-by 注释（21 处，实测 grep；B 相批量修改后重跑同一条 grep 验证）：

| 旧引用（file:line） | 指向旧篇 | 新目标 | 验证方式 |
|---------------------|---------|--------|---------|
| main.rs:4、lib.rs:8（全路径）、sef.rs:5 | 01-sched-init-main | 01-sched-birth-and-init | `grep -rn "sched-init-main" os/` 应零命中 |
| main.rs:5、dispatch.rs:5 | 02-sched-message-surface | 02 面 → 新 02-sched-message-contract；循环面 → 新 03-sched-main-loop（按注释语义拆分指向） | 同上式 |
| main.rs:44 | 11-balance-queues | 新 12-balance-queues | grep "owned by" |
| schedproc.rs:4 | 03-schedproc-struct | 新 04-schedproc-record-and-table | 同上 |
| table.rs:5、valid.rs:4 | 04-schedproc-table | 新 04（合并篇） | 同上 |
| priority.rs:7 | 05-priority-timeslice-model | 新 05（同名保留） | 免改，仅核对 |
| scheduling/start.rs:5 | 06-start-scheduling | 新 09-start-scheduling | 同上 |
| scheduling/stop.rs:5 | 07-stop-scheduling | 新 10-stop-scheduling | 同上 |
| scheduling/noquantum.rs:5、nice.rs:5 | 08-noquantum-nice | 新 11-noquantum-nice | 同上 |
| kernel_api/schedule.rs:8 | 09-schedule-process | 新 08-schedule-process | 同上 |
| cpu.rs:6 | 10-pick-cpu-smp | 新 07-pick-cpu | 同上 |
| balancer.rs:6 | 11-balance-queues | 新 12-balance-queues | 同上 |
| kernel_api/schedctl.rs:6 | 12-kernel-interface | 新 06-kernel-contract | 同上 |
| client.rs:5-6 | 13-pm-interaction / 14-rs-interaction | 均指新 13-clients-pm-rs（双故事一句合并） | 同上 |
| server.rs:8 "plan.md ARCH table, doc 02 D8" | 旧 02 决策号 D8 | 新 03 §3；B 相若重排决策号，此注释同步改号并留迁移注记 | 人工核对 |

文档/材料层引用：

| 引用处 | 内容 | 处置 | 验证 |
|--------|------|------|------|
| os/servers/rs/src/boot.rs:123 | "(06-stage-sched)" stage 目录级 | 免改 | —— |
| edge_todo.md:453 "12 篇契约（06-stage-sched）" | 旧 12 → 新 06 | B 相后更新该句 | grep "12 篇契约" |
| edge_todo.md:501 "06 篇重试环与 10 篇 pick" | 旧 06→新 09、旧 10→新 07 | 同上 | grep |
| edge1.md:17-18、edge3.md:63/87 | 指向 06-stage-sched/**todo.md** §2/§3（文件级，不涉篇号） | 免改（todo.md 保留原位） | —— |
| 07-stage-ds/todo.md:187 | 指向 06-stage-sched/todo.md §4.1 | 免改 | —— |
| 00-master-plan/10-phase5-sched-guide.md | 引用 00-sched-overview（旧主线 defer 件） | 不改（master-plan README §本目录状态：整目录 defer） | —— |
| 本 stage 内部"见 NN 篇 / `NN-stem`"引用 | **307 处**（实测 grep -o 计数，不含本蓝图） | 按 §8.1 映射表批量迁移（热点：旧 06=40、旧 10=30、旧 02=27、旧 09=27、旧 05=25） | 迁移脚本 dry-run 前后计数一致 |
| plan.md §3.4/§5/§6.1、todo.md 各节 | 旧编号全量 | B 相重写 plan.md；todo.md 追加一轮记录 | —— |

### 8.3 断链成本摘要

- 受影响引用总量 ≈ **335**（内部 307 + Rust 注释 22 + 材料层 6）；其中必改 ≈ 325（免改 10 处为 stage 目录级/同名保留）。
- 热点文件：内部引用密度最高在旧 06/10/02（合计 ~97 处指向）；代码注释热点 `main.rs`（3 处）与 `client.rs`（2 篇指向）。
- 批量方式：B 相用 `tools/anchor-migrate.sh`/`anchor-resolve.sh`（仓内既有工具）加载旧→新映射表执行，先 dry-run 比对计数，再落盘；落盘后重跑 §8.2 全部 grep 断言 + `tools/check-rs-unwired.sh` 兜底。
- 风险点：`plan.md`/`todo.md` 与各篇"过渡/参见"的旧编号混排期（新旧目录并存时）——建议 B 相一次性完成 16 篇 + 引用迁移，不留半迁移状态。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：逐篇核对契约"前置"字段——00(无)、01(00)、02(01)、03(01,02)、04(01,02)、05(04)、06(02,04,05)、07(01,04,05)、08(05,06,07)、09(02-08)、10(02,04,08)、11(02,05,08)、12(03,05,08,11)、13(02,09,10,11)、14(全)、99(无)——全部只指向更早编号，**前向引用 0**。唯一例外机制：新 03 分支表对 handler 的"一行摘要+指针"（序差 D-3 登记，摘要自足不依赖后文术语）。
2. **依赖图无环**：上列前置关系构成以 00→…→14 为主链的 DAG，拓扑序存在（即新编号序本身）；旧目录的 03↔04 互依赖（04 前置含 03、03 引用 04 检查函数）已由合并（O-04）消解；旧 12↔09 的循环借用已由契约上移（O-06）消解。**无环。**
3. **覆盖率**：池 101 条逐条对照 §5 各契约"知识点清单"，每条有去向（新篇+小节）；删除项 3 条单独列出（§6 末）且各给理由；新增 17 条全部带 C 源码/制品/实测证据锚点。**100%。**
4. **断链统计**：§8.3（总量 335、热点、批量方式、验证命令）。

### 9.2 自检门 G1-G9

| 门 | 结果与证据 |
|----|-----------|
| G1 | **通过**。真序表 T/L/H 全锚来自本次逐行阅读：抽查 10 条复核——main.c:39 sef_receive_status(ANY)（§0.3 输出）、:47-48 CLOCK 无条件调用（sed 44-46 实测）、:70-71 旗标门、schedule.c:164 `>=NR_SCHED_QUEUES`、:174-175/:195-196 覆盖、:262-263 do_nice 有 accept、:302 每次 pick、:334-342/:367-368 闹钟、system.c:645 `>`、do_schedctl.c 全文 47 行——10/10 与文件一致。 |
| G2 | **通过**。C 清单 12 文件（§0.2）每个函数/宏/结构入池或入缺口表（setreply 死声明 G-9、schedule_process_migrate 死宏 O-10 行实测）；非 C 制品十项逐条回答（§3.6）；排除项均给归属（G-10/G-11）。 |
| G3 | **通过**（9.1-1）。 |
| G4 | **通过**（9.1-2），并附两处旧环的拆解手段。 |
| G5 | **通过**（9.1-3）。 |
| G6 | **通过**。抽查 10 处：O-02/O-03（旧 01/02 §2 重复段合并去向逐节）、O-04（accept_message 迁出）、O-06（09 §2.4/2.5 迁出）、O-13（14 全篇并入）、O-14（新建来源四路）、O-08/O-11（掩码归一）——存量去向齐全、新增来源带锚。 |
| G7 | **通过**。16 篇契约七要素（定位/讲/不讲/前置/后置/事实底线/清单+验收）逐篇齐备；新 14/99 为新建/改写一篇不缺项。 |
| G8 | **基本通过，带一项诚实缺口**。锚点迁移表覆盖全部 16 旧篇每一节（§8.1，分组行显式注明范围）；引用迁移表覆盖代码注释 21+1 处与材料层 6 处、内部 307 处给出总量/热点/批量方案而非逐条列举（内部引用逐条清单交由迁移脚本 dry-run 报告生成，属 B 相工具产物）。 |
| G9 | **通过，标注待验证 3 处**。全部事实断言带锚或显式标注：(a) machine 结构字段行号（type.h:122-125 系旧引、boot_cpu/ncpu 名未在 minix/include/minix/type.h 直接命中，已标"待重定位"）；(b) ipc.h start/inherit 结构体精确行号（union 成员 2568-2569 已实测，结构体本体行界待 B 相 sed 复核）；(c) minix-sys 两处 stub 的行号引自 edge_todo 记录（B 相引用时复核）。推测项无未标注者。 |

### 9.3 结论

蓝图完成，四问齐答：真序（§1，服务事件循环型双段+触发方）、知识池（§2，101 条含 17 新增）、新目录（§4/§5，16 篇契约+序差 7 条）、旧到新（§6/§8，16 操作+全节迁移+335 处断链账）。机械检查四项全过，G8/G9 附注执行注意。**本蓝图可直接进入共识汇总。**

### 9.4 待用户裁决的问题

1. **文档数与合并尺度**：O-04（03+04 合并）、O-13（13+14 合并）使正文篇数 14→12，另增工程篇 14 与实体化 00/99。若偏好"一 handler 一篇"的极限拆分（新 11 拆回两篇），契约可平移，影响仅限 §4-§6。
2. **新 14 的目录资格**：测试基建/通电全景非 C 内容，按 prompt 允许为 C 之外主题新建篇章；若认为应降级为新 99 附表，缺口 G-2 的落点需改判。
3. **Rust 决策编号迁移**：代码注释锚定 "doc 02 D8" 类旧决策号；方案 A=新篇保留旧 D 号并注迁移史（推荐，注释改动最小）、方案 B=各篇重排 D 号+全量同步 21 处注释。默认按 A 执行，选 B 请在共识时声明。
4. **对照小节存废**：§3.5 保留每篇一段跨 OS 对照（旧目录惯例）；若按"最小化不可验证内容"进一步收紧为仅 00/05/12 三篇，请在汇总时裁决。
