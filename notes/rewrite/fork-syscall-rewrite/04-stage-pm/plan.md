# 04-stage-pm 文档重组计划（plan.md）

> **状态**: 生效中（2026-08-16 首版，深度 review + minix3 源码回归 review 后定稿）
> **范围**: `notes/rewrite/fork-syscall-rewrite/04-stage-pm/`
> **目标**: 以 **PM server 启动顺序为主线**重组 PM 全部文档；`fork` 系统调用降为次主线；最终覆盖 Minix3 PM server 全部语义（15 个 .c + 6 个 .h，~4,747 行 C，47 个 PM 调用），支撑 PM server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`minix3/minix/servers/pm/`（ground truth）、`os/servers/pm/src/`（Rust 实现）、`notes/study/pm/`（早期学习笔记，素材）

---

## 1. 背景与动机

### 1.1 旧主线（fork 主线）的问题

旧文档（已移入 `draft/`）以 fork 系统调用为主线：README + 9 篇文档沿 fork 执行路径展开（mproc 结构 → PID 生成器 → do_fork → PM↔VM fork → PM↔VFS fork → exit → wait → srv_fork → 集成测试）。实践发现三类问题：

1. **覆盖严重不足**——PM 语义主体（信号 855 行 / misc 447 行 / event 353 行 / alarm 344 行 / trace 276 行 / getset 223 行 / exec 200 行 / time 131 行 / schedule 112 行）**完全没有进入文档**。fork 只是 `table.c` 注册的 47 个 PM 调用中的 1 个；以 fork 为主线的文档对"PM 是什么"的回答是残缺的。
2. **组件与流程错位**——`mproc` 结构、进程表、PID 生成本应在 PM 启动时按序初始化（`main.c:sef_cb_init_fresh`），却被"fork 需要什么"的倒推逻辑打散；`do_exit`/`do_wait4` 与 fork 同属进程生命周期（`forkexit.c` 同一文件），却按"fork 之后"排序；PM 运行的"心脏"（`main.c` 主循环、`table.c` 分发、`main.c:handle_vfs_reply` VFS 异步协议）完全缺席。
3. **架构演进（ARCH）标注分散/缺失**——`mproc` 分层重构、`mp_flags` → 状态机枚举、全局变量 → `PmContext`、message union → 类型化 IPC 等 minix-rs 演进散见于 `draft/mproc-design.md`，未形成统一的 ARCH 清单。

### 1.2 新主线：PM server 启动顺序

与 `01-stage-kernel`、`02-stage-vm` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。PM 的启动链（`main.c`）：

```
kernel 初始化完成后启动 PM（boot image，见 01-stage-kernel/06-proc-init-boot-proc.md）
  │
  ▼  main.c:49  main()
  └─ sef_local_startup()                ← 01：SEF 回调注册（init_fresh + signal_manager）
       └─ sef_startup() → sef_cb_init_fresh()
            ├─ 初始化 mproc 表 + timers  ← 02/03：进程模型（MP_MAGIC/mp_sigact/mp_eventsub）
            ├─ 构建信号集合               ← 11 前置：core_sset/ign_sset/noign_sset
            ├─ sys_getmonparams()        ← 01：启动参数（monitor_params）
            ├─ sys_getimage()            ← 01：boot image 表
            ├─ 填充 mproc：INIT + 系统进程 ← 01/02/03：身份/父/PRIV_PROC/nice
            ├─ VFS_PM_INIT 同步          ← 01/05：与 VFS 交换进程表
            ├─ system_hz = sys_hz()      ← 14/19 前置：时钟频率
            └─ sched_init()              ← 16：为 INIT 指定用户态调度器
  │
  ▼  main.c:59-110  主循环（运行时）
  ├─ sef_receive_status(ANY)             ← 04：收消息
  ├─ is_ipc_notify：CLOCK → expire_timers ← 14：定时器到期
  ├─ pm_isokendpt()                      ← 03：caller 验证
  ├─ EXITING 进程的延迟调用直接丢弃        ← 04/09：退出中不响应
  ├─ 分发：
  │   ├─ IS_VFS_PM_RS → handle_vfs_reply ← 05：VFS 异步回复（11 种）
  │   ├─ PROC_EVENT_REPLY → do_proc_event_reply ← 06：事件订阅者回复
  │   └─ IS_PM_CALL → call_vec → do_xxx  ← 04/07~20：47 个 PM 调用
  └─ result != SUSPEND → reply()         ← 04/05：回复（SUSPEND = 异步稍后回复）
```

**每篇文档必须能回答一个问题：它位于 PM 启动时序（sef_cb_init_fresh）或主循环（dispatch）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则。

### 1.3 fork 次主线

fork 不再充当"概念引入的驱动"，而是作为**阶段 3 的服务之一**展开，其路径图在 `07-pm-fork` 内部绘制：

```
PM_FORK 到达（主循环 dispatch，04）
  ├─ 03 进程表：pm_isokendpt / procs_in_use 检查 / next_child 槽位
  ├─ 07：vm_fork（→ 02-stage-vm/18-vm-fork.md）
  ├─ 02：*rmc = *rmp 复制 + mp_sigact 重指 + flags 继承（IN_USE|DELAY_CALL|TAINTED）
  ├─ 03/07：get_free_pid 分配子 PID
  ├─ 05：VFS_PM_FORK → tell_vfs（VFS_CALL 置位）
  ├─ 11：tracer 存在 → sig_proc(SIGSTOP, trace)
  ├─ 05：VFS_PM_FORK_REPLY → handle_vfs_reply → sched_start_user（16）
  └─ 04：reply(parent, child_pid) / reply(child, OK)（NEW_PARENT 保护）
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。原 `draft/` 文档保留旧编号（作为素材），新编号在顶层重新建立。
>
> **拆分结果：22 篇文档**（00 + 01~20 + 99），按 10 个语义阶段划分。拆分原则 = **每篇一个语义单元**（读者可独立阅读），单元粒度对齐 C 文件职责边界 + Rust 模块边界：主循环/分发（04）、VFS 异步协议（05）、事件订阅（06）各自独立是因为它们是 PM 的"运行时骨架"，被所有服务共享；信号按"生成与分发（11）/handler 系统调用（12）/延迟与恢复机制（13）"拆三篇，因为 `signal.c` 855 行是 PM 最大的文件，且三块机制相互独立；`getset`/`time`/`misc`/`schedule`/`trace`/`alarm`/`exec` 各自成篇，对齐 C 文件天然边界。

### 阶段总览

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | draft 来源 | 变更 |
|------|------|------|---------|--------|-----------|-----------|------|
| 0 总览 | 00 | `00-pm-overview.md` | PM 是什么、启动主线图、文档导航、与 kernel/VM/VFS 的分工 | `servers/pm/` 全部 | 全部 | `draft/README.md` + `draft/mproc-design.md` 部分 | **重写**导航（§2 改为启动主线叙事） |
| 1 启动与进程模型 | 01 | `01-pm-init-main.md` | main()/SEF/init_fresh/启动参数/boot image 填充/VFS_PM_INIT 同步/sched_init 调用点 | `main.c`（`get_nice_value` 定义于 main.c:276，语义归 16；`sched_init` 定义于 schedule.c，调用点归 01） | （未实现）`main.rs`/`init.rs` | `draft/README.md` | **新建**（原无启动文档） |
| 1 | 02 | `02-mproc-struct.md` | struct mproc 全部字段、mp_flags 正交位、mpsigact 独立表、MP_MAGIC | `mproc.h` | `mproc/mproc.rs`、`mproc/{lifecycle,block,wait,guardianship,trace,signal,credentials}.rs` | `draft/mproc-design.md` | 沿用 + 分层建模（A-1） |
| 1 | 03 | `03-mproc-table.md` | mproc[NR_PROCS]、procs_in_use、pm_isokendpt、find_proc、PID 生成（get_free_pid） | `glo.h`、`utility.c:pm_isokendpt/find_proc/get_free_pid` | `mproc/table.rs`、`mproc/pid_gen.rs` | `draft/mproc-design.md` + `draft/pid-generator.md` | **合并**：表 + PID 身份管理成一篇 |
| 2 主循环与异步协议 | 04 | `04-ipc-dispatch.md` | 主循环、call_vec 分发、reply、SUSPEND、EXITING 丢弃、CLOCK notify、调用统计 | `main.c:49/59-110/250-274`、`table.c`、`callnr.h` | `ipc/dispatcher.rs` | 无（draft 无主循环文档） | **新建** |
| 2 | 05 | `05-vfs-interaction.md` | tell_vfs、VFS_CALL、handle_vfs_reply 全部 11 种 reply、NEW_PARENT/UNPAUSED、VFS_PM_* 协议 | `main.c:295-424`、`utility.c:tell_vfs`、`com.h:VFS_PM_*` | （未实现）VFS 客户端/回复状态机 | `draft/pm-call-vfs-fork.md` | **扩充**：fork 单点 → 全协议 |
| 2 | 06 | `06-event-subscription.md` | PROC_EVENT 订阅/发布、NR_SUBS=4 串行化、do_proceventmask、do_proc_event_reply、publish_event、resume_event | `event.c` | （未实现）event 订阅模块 | 无 | **新建** |
| 3 进程生命周期（fork 次主线） | 07 | `07-pm-fork.md` | do_fork 全流程、槽位分配、vm_fork、mproc 复制、get_free_pid、VFS_PM_FORK、tracer SIGSTOP、fork 路径图 | `forkexit.c:do_fork`、`utility.c:get_free_pid` | `fork.rs`、`mproc/fork.rs`、`mproc/pid_gen.rs` | `draft/do-fork-impl.md` + `draft/pid-generator.md` + `draft/pm-call-vm-fork.md` | **合并**：PM 侧 fork 全链路成一篇 |
| 3 | 08 | `08-pm-srv-fork.md` | do_srv_fork、RS 专用、PRIV_PROC 继承、UID/GID 消息注入、VFS_PM_SRV_FORK | `forkexit.c:do_srv_fork` | （部分）`fork.rs` | `draft/srv-fork-impl.md` | 沿用 |
| 3 | 09 | `09-pm-exit.md` | do_exit、exit_proc、exit_restart、zombify、check_parent、disinherit/INIT 收养、SIGHUP、TRACE_EXIT、system 进程直毁 | `forkexit.c:246-417`、`zombify/check_parent/tracer_died` | `exit.rs`、`mproc/lifecycle.rs` | `draft/exit-impl.md` | **扩充**：僵尸状态机 + 收养语义 |
| 3 | 10 | `10-pm-wait.md` | do_wait4、wait_test、tell_parent/tell_tracer、rusage 累计、TOLD_PARENT、cleanup | `forkexit.c:475-570/670-806` | `wait.rs`、`mproc/wait.rs` | `draft/wait-impl.md` | **扩充**：tracer 伪父 + rusage 语义 |
| 4 信号系统 | 11 | `11-signal-core.md` | do_kill/do_srv_kill/check_sig/sig_proc/process_ksig/sig_proc_exit、权限检查、广播、系统进程信号、SIGS_IS_LETHAL | `signal.c:do_kill(197)/do_srv_kill(207)/process_ksig(294)/sig_proc(384)/sig_proc_exit(546)/check_sig(568)` | `signal.rs`、`mproc/signal.rs` | 无 | **新建** |
| 4 | 12 | `12-signal-handlers.md` | do_sigaction/sigprocmask/sigpending/sigsuspend/sigreturn、sig_send、sys_sigsend、SA_* 标志、sigmask 语义、SIG_IGN/DFL/CATCH 三态 | `signal.c:40-196/776-855` | （未实现）信号 handler 状态 | 无 | **新建** |
| 4 | 13 | `13-signal-flow.md` | check_pending/restart_sigs/unpause/stop_proc/try_resume、DELAY_CALL/SIGSNDELAY、PROC_STOPPED、VFS/event 协同 | `signal.c:226-293/652-776` | （未实现）延迟与恢复机制 | 无 | **新建** |
| 5 定时器 | 14 | `14-itimer.md` | do_itimer、set_alarm、check_vtimer、ticks 转换、cause_sigalrm、CLOCK notify 接线、ITIMER_REAL/VIRTUAL/PROF | `alarm.c` | （未实现）timer 模块 | 无 | **新建** |
| 6 身份与凭证 | 15 | `15-credentials.md` | do_get/do_set、uid/gid/groups/setsid/setpgrp/getsid/issetugid、TAINTED、VFS 转发（VFS_PM_SETUID/SETGID/SETSID/SETGROUPS） | `getset.c` | `mproc/credentials.rs` | 无 | **新建** |
| 7 调度 | 16 | `16-scheduling.md` | sched_init/sched_start_user/sched_nice/nice_to_priority/get_nice_value、do_getsetpriority、SCHED_* 协议 | `schedule.c`、`misc.c:do_getsetpriority`、`main.c:get_nice_value` | （未实现）sched 客户端 | 无 | **新建** |
| 8 exec | 17 | `17-exec.md` | do_exec/do_newexec/exec_restart/do_execrestart、PARTIAL_EXEC、setuid/TAINTED、exec 后 sig 重置、tracer SIGTRAP/SIGSTOP、frame 保存 | `exec.c` | `exec.rs`（stub） | 无 | **新建** |
| 9 ptrace | 18 | `18-trace.md` | do_trace 全部 T_*、trace_stop、trace_flags、TRACE_STOPPED、tracer 死亡、W_STOPCODE | `trace.c` | `mproc/trace.rs` | 无 | **新建** |
| 10 时间与系统信息 | 19 | `19-time.md` | do_time/stime/getres/gettime/settime、CLOCK_REALTIME/MONOTONIC、boottime | `time.c` | （未实现）time 模块 | 无 | **新建** |
| 10 | 20 | `20-misc-queries.md` | sysuname/getsysinfo/getprocnr/getepinfo/svrctl/reboot/getrusage/sprofile/mcontext、uts_val、monitor_params、find_param、calls_stats | `misc.c`、`profile.c`、`mcontext.c`、`utility.c:find_param` | （未实现）misc/query 模块 | 无 | **新建** |
| 99 全局概念 | 99 | `99-global-concepts.md` | endpoint/generation、NR_PIDS/INIT_PID 常量、信号集合（core/ign/noign）、全局状态表、PROC_NAME_LEN | `pm.h`/`const.h`/`type.h`/`glo.h`/`proto.h` | `mproc/constants.rs`、`minix-types` | `draft/mproc-design.md` 部分 | 沿用 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序中的位置与下一阶段的入口：

```
01（启动）→ 02/03（进程模型：结构 + 表 + PID）
→ 04（主循环与分发：运行时心脏）
→ 05/06（VFS 异步协议 + 事件订阅：所有服务共享的回复路径）
→ 07~10（fork 次主线 + 进程生命周期：fork/srv_fork/exit/wait）
→ 11~13（信号系统：core/handlers/flow）
→ 14（定时器）→ 15（凭证）→ 16（调度）→ 17（exec）→ 18（ptrace）→ 19/20（时间/杂项查询）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel` 一致（见 `03-kmain-cstart.md`、`04-platform-discovery.md` 等）：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明
6. **过渡**——在启动时序/主循环中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 组织原则（继承 01-stage-kernel §3）

1. **概念首次出现即完整解释**——后续只引用不复述
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：进程模型（02/03）在生命周期（07~10）之前，信号核心（11）在定时器（14）之前，VFS 协议（05）在 fork（07）之前）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在 sef_cb_init_fresh / 主循环分发的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `07-pm-fork.md` §VFS 协调）
- 与 kernel 文档交叉引用时用 `../01-stage-kernel/NN-*.md`（如 PM 启动 → `06-proc-init-boot-proc.md`；syscall 转发 → `17-syscall-process.md`；信号 → `19-syscall-signal.md`）
- 与 VM 文档交叉引用时用 `../02-stage-vm/NN-*.md`（如 `07-pm-fork.md` → `18-vm-fork.md`）
- 对 draft 素材的引用一律指向 `draft/NN-*.md`，并标注"素材"；正式文档绝不引用 review 产物

### 3.4 每篇文档边界声明（执行级）

> 写作时必须包含"前置依赖/职责/不覆盖"边界声明，防止内容交叉（模式 45 教训，02-stage-vm plan D-19）。

| 文档 | 前置依赖 | 职责 | 不覆盖（移交） |
|------|---------|------|---------------|
| 00 | 无 | 全局叙事 + 导航 | 一切机制 |
| 01 | 00 | 启动时序、SEF、boot image、VFS 同步 | sched_init 细节（16）、信号集合语义（11/99） |
| 02 | 01 | 结构字段、状态分层 | 表操作（03）、信号状态机流转（11~13） |
| 03 | 01/02 | 表、endpoint 验证、PID 分配 | 槽位使用方（07/09/10） |
| 04 | 01~03 | 主循环、分发、回复 | VFS 回复处理（05）、事件回复（06） |
| 05 | 04 | VFS 协议、回复状态机 | 具体服务流程（07~20） |
| 06 | 04 | 事件订阅/发布 | 信号/退出的具体处理（11~13/09） |
| 07 | 03/04/05 | do_fork 全流程 | VM 侧复制（02-stage-vm/18）、VFS 侧 fd（05-stage-vfs） |
| 08 | 07 | RS 专用 fork | 普通 fork（07） |
| 09 | 03/05/06 | 退出流程、僵尸、收养 | wait 回收（10） |
| 10 | 03/09 | wait4、rusage、tracer 回收 | 退出产生僵尸（09） |
| 11 | 03/04 | 信号生成、分发、权限 | handler 机制（12）、恢复机制（13） |
| 12 | 11 | sigaction 族系统调用 | 内核 sigframe 细节（01-stage-kernel/19） |
| 13 | 11/12/05/06 | 延迟、停止、恢复 | 信号语义（11/12） |
| 14 | 04/11 | itimer、alarm 到期 | 内核定时器实现（01-stage-kernel/15） |
| 15 | 02/05 | uid/gid/groups/session | 调度（16）、exec setuid（17） |
| 16 | 01/04 | 调度协议、nice | 内核调度器（01-stage-kernel/11） |
| 17 | 05/15/11 | exec 全流程 | VFS 可执行加载、信号重置的接收方（12） |
| 18 | 03/10/11 | ptrace 命令 | 内核 sys_trace（01-stage-kernel） |
| 19 | 01/04 | 时间系统调用 | 内核时钟（01-stage-kernel/15/21） |
| 20 | 03/04 | 信息/控制/杂项调用 | 具体服务（07~19） |
| 99 | 00 | 全局常量、状态表、术语 | 一切机制 |

### 3.5 测试基线（截至 2026-08-16）

- `cargo test -p minix-pm --lib`：**91 passed / 0 failed**（实测基线，2026-08-17；01 完成时 87，02 新增 4 个：位值对齐/NEW_PARENT 载荷/ignored-caught/fork 无 DELAY_CALL；定向模块：`mproc::table` 8、`mproc::trace` 2、`mproc::wait` 4、`mproc::pid_gen` 7 等）
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.6 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/pm/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 04-stage-pm 范围内的 ARCH 项，写文档时必须逐项落实。状态以 `os/servers/pm/src/` 实际代码为准。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| A-1 | **mproc 结构分层** | `struct mproc` 单结构 + `mp_flags` 正交位一锅炖（`mproc.h:24-83`） | `Process` 分层模型：Identity / State（Lifecycle+BlockState+WaitState+Guardianship+TraceState）/ Resources（Credentials+SignalState）/ Context（`mproc/mproc.rs` 等） | 02 | 已实现 |
| A-2 | **flags 正交位 → 状态机枚举** | `IN_USE/WAITING/ZOMBIE/...` 19 个正交位（`mproc.h:86-104`，另有 `MP_MAGIC` 魔数），任意组合 | `Lifecycle` 互斥枚举（Unused/Running/Exiting/Zombie/TraceZombie/ToldParent）+ `BlockState`/`WaitState` 组合子（`mproc/lifecycle.rs`、`block.rs`、`wait.rs`） | 02/09/10 | 已实现（位→枚举映射须逐位对照） |
| A-3 | **全局状态 → PmContext** | `mp`/`who_p`/`who_e`/`call_nr`/`mproc` 文件级全局（`glo.h:16-23`） | `PmContext<'a>` 显式传参 + `ProcTable`（`mproc/context.rs`、`table.rs`），借用检查器作编译期锁 | 03/04 | 已实现（04 完成显式参数调用点：run_once/reply 用 `UserSlot`，init.rs:279/337） |
| A-4 | **message union → 类型化 IPC** | `m_in.m_lc_pm_*`/`m_pm_lc_*` 手写 union 字段（`com.h`） | `PmRequest`/`PmResponse`/`PmError` + codec trait（`minix-types/src/ipc/pm.rs`），errno 映射 | 04/07 | 部分实现（目前仅 Fork 变体） |
| A-5 | **call_vec 函数指针表 → match 分发** | `call_vec[NR_PM_CALLS]` 表 + `call_index`（`table.c`） | `dispatch_pm_call` match 分发（`ipc/calls.rs`，47 变体 `#[repr(i32)]`） | 04 | 已实现（`dispatch_pm_call` 47 项，calls.rs:201） |
| A-6 | **SUSPEND 显式化** | `return SUSPEND` 表示"本次不回复，稍后 reply()"（`main.c:106`） | 异步回复模型：dispatch 返回 `ReplyLater`/`NoReply` 变体 | 04/05 | 已实现（`ReplyIntent` 三变体，dispatcher.rs:45；handler 具体路径归 05/09+） |
| A-7 | **定时器抽象** | `minix_timer_t` + `set_timer`/`expire_timers`，CLOCK notify 驱动（`alarm.c`、`main.c:65-67`） | 类型化 `Timeout`/`Clock` + 定时器队列（minix-types），到期回调 | 14 | 未实现 |
| A-8 | **用户态调度协议** | `sched_start/inherit/stop/nice` 经 `_taskcall` 到 SCHED 服务（`schedule.c`、`minix/sched.h`） | sched 客户端模块 + 类型化 `SchedulingRequest` | 16 | 未实现 |
| A-9 | **进程事件订阅** | `subs[NR_SUBS]` + 串行化 EVENT_CALL 往返（`event.c`） | 事件订阅表（若实现）或显式缺口契约 | 06 | **缺口**：未实现，标注 fail-closed |
| A-10 | **延迟调用机制** | `DELAY_CALL`/`SIGSNDELAY` 内核延迟调用（`signal.c`、kernel `system.c:463`） | 依赖内核支持；Rust 侧 `DelayCall` 状态或显式缺口契约 | 13 | **缺口**：未实现，标注 defer |
| A-11 | **64 位类型映射** | `pid_t`/`uid_t`/`gid_t`/`clock_t`/`endpoint_t` 平台相关 | `Pid`/`Uid`/`Gid`/`Clock`/`Endpoint`（`minix-types`），`UserSlot` 表达槽位 | 02/03/99 | 已实现 |
| A-12 | **INIT 收养 / 双监护建模** | `mp_parent`/`mp_tracer` 两个监护指针 + `tracer_died`/`NEW_PARENT`（`forkexit.c:760-795`） | `Guardianship { parent, tracer }` 建模（`mproc/guardianship.rs`） | 09/10/18 | 部分实现 |
| A-13 | **进程组/会话语义** | `mp_procgrp` int + pid 相等即会话领导者（`getset.c`、`forkexit.c:exit_proc`） | 类型化 `ProcessGroup`/会话标记（设计决策） | 09/15 | 未实现（设计层） |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7.2。

### 5.1 C 源文件 → 新文档映射（15 个 .c）

| C 源文件 | 行数 | 语义 | 新文档 | 说明 |
|---------|------|------|--------|------|
| `main.c` | 424 | 启动 + 主循环 + 回复 + VFS 回复 | 01/04/05/16 | `get_nice_value` 归 16；`handle_vfs_reply` 归 05；其余归 01/04 |
| `table.c` | 62 | call_vec 分发表 | 04 | 47 个调用完整列出（§5.3） |
| `forkexit.c` | 807 | fork/srv_fork/exit/wait | 07/08/09/10 | 按函数拆分 |
| `signal.c` | 855 | 信号全系统 | 11/12/13 | 按生成/handler/机制拆分 |
| `alarm.c` | 344 | itimer/定时器 | 14 | 整文件 |
| `exec.c` | 200 | exec 流程 | 17 | 整文件 |
| `getset.c` | 223 | uid/gid/groups/session | 15 | 整文件 |
| `event.c` | 353 | 进程事件订阅 | 06 | 整文件 |
| `misc.c` | 447 | 信息/控制/杂项 | 16/20 | `do_getsetpriority` 归 16，其余归 20 |
| `time.c` | 131 | 时间系统调用 | 19 | 整文件 |
| `trace.c` | 276 | ptrace | 18 | 整文件 |
| `schedule.c` | 112 | 调度协议 | 16 | 整文件 |
| `utility.c` | 156 | 工具函数 | 03/05/07/10/16/20 | `get_free_pid` 归 03；`tell_vfs` 归 05；`set_rusage_times` 归 10；`nice_to_priority` 归 16；`find_param` 归 20 |
| `profile.c` | 45 | 统计 profile | 20 | 整文件 |
| `mcontext.c` | 27 | 机器上下文 | 20 | 整文件 |

### 5.2 头文件覆盖

| 头文件 | 内容 | 新文档 |
|--------|------|--------|
| `mproc.h` | struct mproc + mp_flags + mpsigact | 02/03/99 |
| `glo.h` | 全局变量声明 | 03/04/99 |
| `pm.h` | 主头（含系统常量） | 99 |
| `const.h` | NR_PIDS/INIT_PID/NO_EVENTSUB/NR_ITIMERS 等 | 03/14/99 |
| `type.h` | 本地类型（空） | 99 |
| `proto.h` | 全函数原型 | 各文档（函数索引） |
| `minix/callnr.h` | PM_BASE + 47 个调用号 | 04/99 |
| `minix/com.h` | VFS_PM_* / PROC_EVENT / SCHEDULING_* 消息 | 04/05/06/16/99 |
| `minix/sched.h` | sched_start/inherit/stop/nice 客户端 | 16 |
| `minix/type.h`（boot_image） | 启动映像结构 | 01 |

### 5.3 语义模块覆盖清单（函数级）

> 逐函数 grep 实证（命令见 §7.2）。函数 → 文档归属如下：

| 新文档 | 覆盖函数 |
|--------|---------|
| 01 | `main.c:main/sef_local_startup/sef_cb_init_fresh`；`schedule.c:sched_init`（调用点）；boot image 循环（`main.c:178-243`） |
| 02 | `mproc.h` 全字段 + 19 个 flag 位 + `mpsigact` |
| 03 | `utility.c:get_free_pid/pm_isokendpt/find_proc`；`glo.h` 全局 |
| 04 | `main.c:main(主循环)/reply/get_nice_value`；`table.c:call_vec`；`callnr.h` 47 个调用号 |
| 05 | `main.c:handle_vfs_reply`；`utility.c:tell_vfs`；`com.h:VFS_PM_*`（RQ 12 个 + RS 11 个） |
| 06 | `event.c:do_proceventmask/do_proc_event_reply/publish_event/resume_event/remove_sub` |
| 07 | `forkexit.c:do_fork`；`utility.c:get_free_pid`（使用点） |
| 08 | `forkexit.c:do_srv_fork` |
| 09 | `forkexit.c:do_exit/exit_proc/exit_restart/zombify/check_parent/tracer_died` |
| 10 | `forkexit.c:do_wait4/wait_test/tell_parent/tell_tracer/cleanup`；`utility.c:set_rusage_times` |
| 11 | `signal.c:do_kill/do_srv_kill/check_sig/sig_proc/sig_proc_exit/process_ksig` |
| 12 | `signal.c:do_sigaction/do_sigpending/do_sigprocmask/do_sigreturn/do_sigsuspend/sig_send` |
| 13 | `signal.c:check_pending/restart_sigs/unpause/stop_proc/try_resume_proc` |
| 14 | `alarm.c:do_itimer/set_alarm/check_vtimer/ticks_from_timeval/timeval_from_ticks/is_sane_timeval/getset_vtimer/get_realtimer/set_realtimer/cause_sigalrm` |
| 15 | `getset.c:do_get/do_set` |
| 16 | `schedule.c:sched_init/sched_start_user/sched_nice`；`utility.c:nice_to_priority`；`main.c:get_nice_value`；`misc.c:do_getsetpriority` |
| 17 | `exec.c:do_exec/do_newexec/do_execrestart/exec_restart` |
| 18 | `trace.c:do_trace/trace_stop` |
| 19 | `time.c:do_time/do_stime/do_getres/do_gettime/do_settime` |
| 20 | `misc.c:do_sysuname/do_getsysinfo/do_getprocnr/do_getepinfo/do_reboot/do_svrctl/do_getrusage`；`profile.c:do_sprofile`；`mcontext.c:do_getmcontext/do_setmcontext`；`utility.c:find_param` |
| 99 | `pm.h/const.h/type.h/glo.h/proto.h` 常量与全局 |

### 5.4 明确排除 / 跳过的项

| 项 | 说明 | 处置 |
|----|------|------|
| `ENABLE_SYSCALL_STATS` 的 `calls_stats` / `SI_CALL_STATS` | 编译宏可选（`misc.c:64/131-132`、`main.c:35/96`） | 20 标注为 cfg feature（A-7 sanity 模式），WONTFIX 文档化 |
| `SPROFILE` 的 `do_sprofile` | `#if SPROFILE`，默认 `ENOSYS`（`profile.c`） | 20 标注，默认 ENOSYS 语义保留 |
| `uts_val` "COMPATIBILITY BLOCK" | 已废弃 uname 兼容块（`misc.c:33-70`） | 20 标注为兼容层，64 位下重新定义 |
| 内核侧 `sys_*` 接口实现 | `sys_times/sys_stop/sys_clear/sys_trace/sys_sigsend/...` | 交叉引用 `../01-stage-kernel/`，不在 PM 文档展开 |
| VM 侧 `vm_fork/vm_willexit/vm_exit/vm_getrusage` 实现 | 调用点在 PM，实现在对端 | 交叉引用 `../02-stage-vm/18-vm-fork.md` 等 |
| VFS 侧 `VFS_PM_*` 回复处理 | 对端实现 | 交叉引用 `../05-stage-vfs/`（如存在），PM 文档只写 PM 侧状态机 |
| `pm-call-vm-fork.md` 引用断裂 | draft README 引用不存在的 `pm-call-vm-fork.md`（旧文档缺失） | draft 素材不再维护；正式文档以 C 源码为准 |

---

## 6. 实施路线

### 6.1 文档改写状态跟踪

> 初始状态：全部为"最小骨架"（本计划交付物），按下列顺序逐篇改写为完整文档。改写顺序 = 阅读顺序（01 → 02 → ... → 20 → 99）。

| 编号 | 文档 | 状态 | 首次改写日期 | 最后 review 日期 |
|------|------|------|-------------|-----------------|
| 00 | `00-pm-overview.md` | 骨架 | — | — |
| 01 | `01-pm-init-main.md` | 完整 | 2026-08-17 | 2026-08-17 |
| 02 | `02-mproc-struct.md` | 完整 | 2026-08-17 | 2026-08-17 |
| 03 | `03-mproc-table.md` | 完整 | 2026-08-17 | 2026-08-17 |
| 04 | `04-ipc-dispatch.md` | 完整 | 2026-08-17 | 2026-08-17 |
| 05 | `05-vfs-interaction.md` | 完整 | 2026-09-02 | 2026-09-02 |
| 06 | `06-event-subscription.md` | 完整 | 2026-09-02 | 2026-09-02 |
| 07 | `07-pm-fork.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 08 | `08-pm-srv-fork.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 09 | `09-pm-exit.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 10 | `10-pm-wait.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 11 | `11-signal-core.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 12 | `12-signal-handlers.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 13 | `13-signal-flow.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 14 | `14-itimer.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 15 | `15-credentials.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 16 | `16-scheduling.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 17 | `17-exec.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 18 | `18-trace.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 19 | `19-time.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 20 | `20-misc-queries.md` | 完整 | 2026-09-03 | 2026-09-03 |
| 99 | `99-global-concepts.md` | 骨架 | — | — |

### 6.2 改写优先级

1. **01 → 03 → 04 → 05**（启动 + 进程模型 + 主循环 + VFS 协议）：PM 的骨架语义，其余文档的前置
2. **02 → 07 → 09 → 10**（结构 + fork/exit/wait 生命周期）：fork 次主线，Rust 已有实现可对照
3. **11 → 12 → 13**（信号系统）：PM 最大语义面，与 kernel `19-syscall-signal.md` 对照
4. **14 → 15 → 16 → 17 → 18 → 19 → 20**（剩余服务）
5. **99 → 00**（全局概念与总览收尾）

---

## 7. Review 记录

### 7.1 深度 review（2026-08-16）

**方法**：对照 `02-stage-vm/plan.md`（已定稿范本）+ `01-stage-kernel` 讲述结构，逐节审查：主线时序准确性、语义模块拆分合理性、覆盖完备性、ARCH 项与 Rust 现状一致性、边界声明完整性。

**证据**：

```bash
# PM C 源码函数面核对（15 个 .c 全部进入映射）
rg -n '^(int|void|pid_t|char|static|clock_t) [a-z_]+\(' minix3/minix/servers/pm/*.c
# 47 个调用号核对
rg -n 'PM_(EXIT|FORK|WAIT4|...|GETSYSINFO)' minix3/minix/include/minix/callnr.h
# Rust 现状核对
cargo test -p minix-pm --lib   # 77 passed / 0 failed
```

**发现与修复**：

| # | 发现 | 严重度 | 处置 |
|---|------|--------|------|
| D-1 | 旧 draft 文档缺失 `pm-call-vm-fork.md`（README 引用断裂） | P2 | §5.4 记录为素材缺陷，正式文档以 C 源码为准 |
| D-2 | `get_free_pid` 同时被 init（01）与 fork（07）使用，初稿归 07 会破坏 01 的自包含性 | P1 | 归 03（进程表 + 身份管理），07/01 引用 |
| D-3 | `handle_vfs_reply` 初稿归 04（主循环），但它是 11 种回复的独立状态机，与分发骨架粒度不符 | P1 | 独立成 05（VFS 异步协议），04 只保留分发骨架 |
| D-4 | `get_nice_value`（main.c:276）与 `nice_to_priority`（utility.c:91）双转换函数易混淆 | P2 | 均归 16，§5.3 明确两函数差异（queue→nice vs nice→queue） |
| D-5 | 信号拆分 11/12/13 三篇的边界需显式声明（`sig_proc_exit` 属 11、`sig_send` 属 12、`stop_proc` 属 13） | P1 | §3.4 边界表 + §5.3 函数归属表逐函数列出 |
| D-6 | A-9（事件订阅）与 A-10（延迟调用）为未实现缺口，须按 VM A-8 模式标注 fail-closed 契约 | P1 | §4 标注"缺口"，写文档时给出语义契约 |
| D-7 | `do_getsetpriority` 定义于 misc.c 但语义属调度，初稿归 20 与 `sched_*` 割裂 | P2 | 归 16，§5.1 misc.c 行注明 |
| D-8 | 缺每篇"前置依赖/职责/不覆盖"边界声明，写作时易内容交叉 | P1 | §3.4 边界表（22 篇全列） |
| D-9 | 无测试基线，§测试 无法对账 | P2 | §3.5 实测基线 77 passed / 0 failed |
| D-10 | 计数与行号漂移：文档数 21→22、C 文件 16→15、头文件 9→6、正交位 21→19；行号偏移：主循环 66→59、boot 循环 180→178、CLOCK notify 71-74→65-67、SUSPEND 特判 104→106 | P2 | 已修正（§1.2/§2/§3.4/§4/§5.3/§7.2），grep 实证见 §7.2 |

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `minix3/minix/servers/pm/` 全部 15 个 .c + 6 个本地 .h 逐一 grep 核对 §5.1/§5.2 映射表，并逐函数核对 §5.3 函数级清单。

**证据**：

```bash
ls minix3/minix/servers/pm/*.c                     # 15 个文件，与 §5.1 表一致
ls minix3/minix/servers/pm/*.h                     # 6 个本地头文件，与 §5.2 表一致
rg -n '^(int|void|pid_t|char|static|clock_t) [a-z_]+\(' minix3/minix/servers/pm/*.c   # 函数面核对
rg -n 'CALL\(' minix3/minix/servers/pm/table.c     # 47 个调用注册核对
rg -n 'VFS_PM_' minix3/minix/include/minix/com.h   # VFS_PM_* 协议面核对
rg -n 'PM_[A-Z_]+' minix3/minix/include/minix/callnr.h  # 调用号核对
```

**结论**：
- 15 个 .c 全部映射到新文档，无遗漏；6 个本地头文件 + 4 个外部头文件全部进入头文件覆盖表。
- `table.c` 注册的 47 个调用（`PM_EXIT`~`PM_GETSYSINFO`）逐一落到 04/07~20（分发表本身归 04，各 handler 归对应文档）：
  - 生命周期族（EXIT/FORK/WAIT4/SRV_FORK）→ 07~10
  - 信号族（KILL/SRV_KILL/SIGACTION/SIGSUSPEND/SIGPENDING/SIGPROCMASK/SIGRETURN）→ 11~13
  - 身份族（GETPID/SETUID/GETUID/SETGROUPS/GETGROUPS/SETGID/GETGID/SETSID/GETPGRP/SETEUID/SETEGID/ISSETUGID/GETSID）→ 15
  - 时间族（STIME/GETTIMEOFDAY/CLOCK_GETRES/CLOCK_GETTIME/CLOCK_SETTIME）→ 19
  - exec 族（EXEC/EXEC_NEW/EXEC_RESTART）→ 17
  - 调度族（GETPRIORITY/SETPRIORITY）→ 16
  - 杂项族（PTRACE/ITIMER/GETMCONTEXT/SETMCONTEXT/SYSUNAME/GETRUSAGE/REBOOT/SVRCTL/SPROF/PROCEVENTMASK/GETEPINFO/GETPROCNR/GETSYSINFO）→ 14/18/20/06
- ARCH 项（A-1~A-13）与 minix3 现状对照成立；A-9/A-10 显式标注为未实现缺口（fail-closed/defer 契约）。
- **覆盖完整性通过**：无 PM 语义（函数/调用/协议）遗漏。

### 7.3 写作前置设计决策：SUSPEND 语义契约（2026-08-16）

**决策**：PM 的 `SUSPEND`（`main.c:106`，"本次不回复，稍后由 reply()/异步路径回复"）在 Rust 中建模为 dispatch 返回的**显式回复意图枚举**：

```rust
enum ReplyIntent { Reply(i32), ReplyLater, NoReply }  // ReplyLater 对应 SUSPEND
```

**依据**（grep 实证）：
- `main.c:104-108`：`if (result != SUSPEND) reply(who_p, result);`——SUSPEND 是主循环层唯一特判（`main.c:106` 为实际特判行）。
- SUSPEND 的回复路径有三类：主循环尾部的 `reply()`（同步调用）、`handle_vfs_reply` 内的 `reply()`（VFS 回复后）、`tell_tracer/tell_parent` 内的直接 `reply()`（wait4 完成时）——三类路径都必须被 `ReplyIntent::ReplyLater` 覆盖。
- C 中 `do_wait4` 返回 SUSPEND 后由 `tell_parent` 回复（forkexit.c:670-730），`do_exec`/`do_set` 返回 SUSPEND 后由 `handle_vfs_reply` 回复（main.c:295-424），`do_exit` 返回 SUSPEND 且永不回复（forkexit.c:246-266）——三种子情形（等待中回复/异步回复/永不回复）必须由 04/05 两篇文档分别建模。

**同步**：plan.md §2 04/05 行语义模块列、§4 A-6、07-pm-fork §VFS 协调、checklist（后续建立）四处一致。

---

## 8. 参见

- `draft/` — 旧主线全部素材（README + 9 篇）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则/过渡章节）
- `../01-stage-kernel/19-syscall-signal.md` — 内核信号路径（PM 信号文档交叉参照）
- `../02-stage-vm/plan.md` — 同型重组范本（plan 结构/ARCH 清单/覆盖契约模式）
- `../00-master-plan/05-phase1-pm-guide.md` — 早期 PM 实现指南（素材）
- `notes/study/pm/` — 早期 PM 学习笔记（素材）
- `minix3/minix/servers/pm/` — C 源码（ground truth）
- `os/servers/pm/src/` — Rust 实现
