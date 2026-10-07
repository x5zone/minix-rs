# 04-stage-pm 文档重建蓝图（glm）

```text
your_name(AI agent name) = glm
target_dir(关注的工作目录) = 04-stage-pm
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 04-stage-pm/doc_rerank_glm.md，不改任何正文。
约束 = 未引用 .design/ 与 tmp_design_and_todo/；本产物带 _glm 后缀；
       未读取任何其它 AI 的 doc_rerank_* 产物。
```

---

## 0. 元数据

- **执行者**: glm
- **日期**: 2026-09-19
- **目标目录**: `notes/rewrite/fork-syscall-rewrite/04-stage-pm/`
- **仓库根目录**: `/home/xzhao/github/minix-rs`
- **当前提交号**: `4a8bc90bd`（分支 `rewrite`，2026-09-19 05:24 +0800；工作树中 `os/servers/pm/` 无未提交变更，`git status --short os/servers/pm/` 输出为空）
- **执行模型**: GLM（bigmodel），独立 bagging 执行，未读取其它 AI 的同类产物

### 0.1 审查范围

**属于正式文档（重建对象）**: `00-pm-overview.md`、`01-pm-init-main.md` ～ `20-misc-queries.md`、`99-global-concepts.md`，共 22 篇。

**属于参考材料（只取料，不参与编号体系）**: `plan.md`（文档重组计划，446 行）、`todo.md`（三轮架构审查与接线台账，590 行）、`draft/`（旧 fork 主线素材 8 篇，已声明停止维护）。

**范围外**: 其它 stage 目录（01-stage-kernel、02-stage-vm、03-stage-rs、05-stage-vfs、06-stage-sched 等）的正文；`minix3/` 内核侧 `sys_*` 系统任务实现；`os/libs/minix-types`、`os/libs/minix-sys` 的内部实现（只取 PM 消费的接口面）；`notes/study/pm/` 早期笔记。

### 0.2 读取清单

| 类别 | 内容 |
|------|------|
| 正式文档 | 22 篇全部读完头部声明；00/99/plan/todo 全文精读；02/03/04/05/06/07/08/09/10/11/12/13/14/15/16/17/18/19/20 精读概念章与边界声明，正文按需取材 |
| C 源码 | `minix3/minix/servers/pm/` 全部 15 个 .c + 6 个本地 .h（4747 行）**全文精读**，另核对外部头 `minix/callnr.h`、`minix/com.h`、`minix/ipc.h`（消息 union 字段族 `m_lc_pm_*`）、`minix/vm.h`、`minix/sched.h`、`minix/timers.h`、`minix/syslib.h`（PROC_EVENT_*）、`minix/sys_config.h`（`_NR_PROCS 256`）、`sys/sys/signal.h`（SIGSNDELAY/SIGS_IS_LETHAL）、`sys/sys/ptrace.h` |
| 边界材料 | `00-master-plan/README.md`（全读）、`edge_todo.md`（PM 相关条目检索）、`03-stage-rs/00-rs-overview.md`（前序 stage 边界确认）、`04-stage-pm/plan.md` + `todo.md`（全读） |
| Rust 实现 | `os/servers/pm/src/` 38 个文件共约 19049 行：重点核对 `ipc/calls.rs`（47 臂分派与 ENOSYS 兜底）、`ipc/dispatcher.rs`（ReplyIntent）、`init.rs`（启动链与 run_once）、`ipc/decode.rs`（S11 解码单点）、`mproc/wire.rs`（D-29 C-ABI 镜像）等模块的存在性与职责；实现细节以 `todo.md` §10–§12 的修复账目为索引交叉验证 |
| 写法范例 | `01-stage-kernel/06-todo.md`（只学"新文档契约"的写法，未搬内容） |

### 0.3 使用的命令与关键输出（证据摘录）

```bash
# 文档清单与行数
wc -l *.md draft/*.md        # 22 篇正式文档 9,352 行；draft/ 4,621 行
# C 源码面
ls minix3/minix/servers/pm/*.c | wc -l    # 15（与 plan.md §5.1 的 15 文件映射表一致）
# Rust 接线现状（关键证据，修复了 00 文档的失真断言）
grep -n "PmCall::" os/servers/pm/src/ipc/calls.rs    # 47 个变体齐全
grep -n "PmCall::SysUname\|PmCall::GetPriority\|PmCall::SetPriority\|PmCall::GetRUsage\|PmCall::Reboot\|PmCall::SvrCtl\|PmCall::SProf\|PmCall::GetSysInfo" os/servers/pm/src/ipc/calls.rs
# → 8 个调用零命中：即仍落 "_ => ReplyIntent::Reply(ENOSYS)" 兜底臂（calls.rs:786 附近）
#   其余 39 个调用已有真实分发臂。
# 文档互引统计（引用行数 / 被引文档数）
#   引用行最多: 01(39) 06(24) 20(22) 07(26) 13(26)；被引最多: 02(18 篇) 04(15) 11(13) 03(12) 05(11)
# 代码注释引用 PM 文档编号
grep -rn "0[0-9]-pm-\|0[0-9]-mproc\|04-ipc\|05-vfs-inter\|06-event-sub\|..." os/servers/pm/src --include="*.rs"
#   30+ 处，热点文件: mproc/mproc.rs、ipc/{calls,dispatcher,vfs}.rs、mproc/{table,pid_gen,context,trace}.rs、main.rs
# 跨 stage 引用
grep -rn "04-stage-pm/[0-9][0-9]-" --include="*.md" notes/rewrite/fork-syscall-rewrite/ | grep -v "^.../04-stage-pm/" | wc -l
#   31 处（05-stage-vfs 14、06-stage-sched 10、02-stage-vm 2、edge_todo 1、其余零星）
git log --oneline -12 -- os/servers/pm/   # S1–S11 批次接线 + D-29 + decode 单点化等 12 个近期提交
```

### 0.4 重建的三大动因（为什么重建而不是搬移）

1. **阅读路线与编号的矛盾是结构性的**。`00-pm-overview.md` §2.3 声明的推荐阅读路线是"99 → 02/03 → 01/04 → 05/06 → …"，而目录编号是 01 启动链在最前、99 词汇表在最后。任何一方迁就另一方，都意味着全部编号重排。这不是某几篇内部能修的局部问题。
2. **"实施现状"层整体过时**。00 §3.1 断言"8 个调用真实接线、其余 40 个 ENOSYS"（2026-09-09 时点）；实测当前是 **39/47 接线**，仅 SysUname(25)/GetPriority(26)/SetPriority(27)/GetRUsage(36)/Reboot(37)/SvrCtl(38)/SProf(39)/GetSysInfo(47) 落 ENOSYS 兜底臂。各机制文档的 Rust 模块标注大量写"（未实现）"，而对应模块（time.rs、timer.rs、sched.rs、misc.rs、credentials.rs、decode.rs、wire.rs）均已存在并有生产实现。旧文档的"现状"叙述已不可作为依据，重建时必须从代码重新取材。
3. **知识点重复与主次缺位**。VFS 异步延续（tell_vfs/VFS_CALL/SUSPEND）在 04/05/07/08/09/15/17 至少七篇中被再次叙述（多数带交叉引用，如 07 §2.6 复述三段式时标注"05 §2.2"，但机制讲解本身重复出现）；僵尸状态（ZOMBIE/TRACE_ZOMBIE）的生产与消费在 09/10/18 三篇间反复互指；TAINTED 在 15 与 17 双讲。交叉引用缓解了失联风险，但没有任何一篇被指定为唯一权威讲述点，同一机制的讲解在多篇重复维护，漂移只是时间问题。

---

## 1. C 真序

### 1.1 阶段类型判定

**判定：服务事件循环型（主），兼有启动链段。** 依据：PM 的 C 源码由 `main.c` 的一次性初始化（`sef_cb_init_fresh`，main.c:131-244）加一条永不返回的消息循环（main.c:59-107）构成；其余 14 个 .c 文件全部是循环内被分派的调用处理函数或回调。因此按提示词第九部分"服务事件循环型"的骨架组织：**服务为什么存在 → 诞生与初始化 → 消息接口与核心数据结构 → 按场景分组的请求处理 → 查询与杂项 → 与邻接服务的协议**。叙事主轴采用"一次 PM 调用的生命周期"（到达 → 验证 → 分发 → 同步回复或挂起延续 → 异步收口），它比源码文件顺序更能解释 SUSPEND/VFS_CALL 这条贯穿全部机制的主线。

### 1.2 运行时真序表

#### 1.2.1 启动段（一次性）

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| B0 | 内核按 boot image 启动 PM，控制权进入 `main` | main.c:49 `main(void)` | PM 是 boot_image 登记的系统服务之一（kernel/table.c 登记，PM_PROC_NR=0） |
| B1 | `sef_local_startup`：注册 fresh/restart 初始化回调与信号管理回调 | main.c:115-126（`sef_setcb_init_fresh` :118、`SEF_CB_INIT_RESTART_STATEFUL` :119、`sef_setcb_signal_manager(process_ksig)` :122） | SEF 框架实现在 `minix3/minix/lib/libsys/sef.c`、`sef_init.c`、`sef_signal.c`（非 C 制品，见 §3.5） |
| B2 | `sef_startup` → `sef_cb_init_fresh` 第 1 步：清表并初始化每槽 timer/magic/sigact 指针/eventsub | main.c:147-152 | `mp_sigact = mpsigact[slot]`（mpsigact 是独立于 mproc 的 `[_NSIG]` 每槽数组，mproc.h:20）；`mp_eventsub = NO_EVENTSUB` |
| B3 | 第 2 步：构建 core_sset / ign_sset / noign_sset 三个信号集合 | main.c:137-141（成员表）、:157-165（建集合） | 成员：core={QUIT,ILL,TRAP,ABRT,EMT,FPE,BUS,SEGV}，ign={CHLD,WINCH,CONT,INFO}，noign={ILL,TRAP,EMT,FPE,BUS,SEGV} |
| B4 | 第 3 步：`sys_getmonparams` 取 boot monitor 参数进 `monitor_params` | main.c:169-170 | 缓冲 `MULTIBOOT_PARAM_BUF_SIZE`，glo.h:21 声明 |
| B5 | 第 4 步：`sys_getimage` 取内核 boot image 表 | main.c:175-176 | `struct boot_image image[NR_BOOT_PROCS]`（minix/type.h） |
| B6 | 第 5 步：遍历 image 填充 mproc（fill_boot_procs 语义） | main.c:177-229 | `proc_nr>=0` 才填；INIT 特例（自父、pid=INIT_PID、scheduler=KERNEL、`get_nice_value(USR_Q)`，:188-201）；系统进程（parent=RS（RS 自身 parent=INIT）、`get_free_pid()`、`IN_USE\|PRIV_PROC`、scheduler=NONE、`get_nice_value(SRV_Q)`，:202-215）；每进程 `ipc_send(VFS_PM_INIT)` 逐条告知 VFS（:221-227） |
| B7 | 第 6 步：VFS_PM_INIT 屏障——`ENDPT=NONE` 的 sendrec 与 VFS 同步"没有更多系统进程" | main.c:231-236 | 双向握手失败即 panic |
| B8 | 第 7 步：`system_hz = sys_hz()` | main.c:238 | 之后全部 tick↔时间换算的基准 |
| B9 | 第 8 步：`sched_init()`——扫描全表，把唯一的用户进程 INIT 交给 SCHED 服务 | schedule.c:20-50（过滤 `IN_USE && !PRIV_PROC` :33、`sched_start(SCHED_PROC_NR, INIT, 父, USER_Q, USER_QUANTUM, -1, &mp_scheduler)` :37-43） | 语义细节归调度篇；此处只有调用点 |

#### 1.2.2 循环段（每次迭代）

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| L1 | `sef_receive_status(ANY, &m_in, &ipc_status)` 收消息 | main.c:61-62 | 失败 panic |
| L2 | notify 分派：`is_ipc_notify(ipc_status)` 为真时，源为 CLOCK 则 `expire_timers(m_in.m_notify.timestamp)`，随后 `continue` | main.c:65-71 | CLOCK notify 是 ITIMER_REAL 的驱动源；notify 不走调用分派、不回复 |
| L3 | 提取 caller：`who_e=m_in.m_source`，`pm_isokendpt` 验证，`mp=&mproc[who_p]`，`call_nr=m_in.m_type` | main.c:74-78 | 验证失败 panic（invalid endpoint） |
| L4 | EXITING 进程的消息直接丢弃 | main.c:81-82 | 延迟调用（DELAY_CALL 在途）从退出进程到达时的保护 |
| L5 | 三路分发：① `IS_VFS_PM_RS(call_nr) && who_e==VFS` → `handle_vfs_reply()`，result=SUSPEND；② `call_nr==PROC_EVENT_REPLY` → `do_proc_event_reply()`；③ `IS_PM_CALL(call_nr)` → `call_vec[call_nr-PM_BASE]()` | main.c:84-103 | ③ 内越界或空槽返回 ENOSYS（:100-101）；`calls_stats` 计数挂在 `ENABLE_SYSCALL_STATS` 下（:95-97，misc.c:63-65 定义） |
| L6 | `result != SUSPEND` 才 `reply(who_p, result)` | main.c:106 | SUSPEND 语义 = 本轮不回复；回复改由 handle_vfs_reply / tell_parent / tell_tracer / srv_fork 直发完成 |

#### 1.2.3 请求处理函数族（循环内可达的全部语义，按 C 文件分组）

| C 文件（行数） | 函数与关键分支 | 锚点 |
|---------------|----------------|------|
| table.c (62) | `call_vec[47]` 分发表：47 个 `CALL(PM_x)=handler` 注册，含 do_get/do_set 一对多复用（GETPID/GETUID/GETGID/GETGROUPS/GETPGRP/GETSID/ISSETUGID→do_get；SETUID/SETEUID/SETGID/SETEGID/SETGROUPS/SETSID→do_set） | table.c:14-62；调用号 `minix/callnr.h:9` PM_BASE=0 起 47 个 |
| forkexit.c (807) | `do_fork`：容量门（`procs_in_use==NR_PROCS` 或非 root 且 `>=NR_PROCS-LAST_FEW(2)`→EAGAIN，:60-65）→ next_child 轮转找槽（:68-75）→ `vm_fork`（:78-80，此后不可失败窗口）→ `procs_in_use++` + `*rmc=*rmp` 整槽复制 + mp_sigact 重指与拷贝（:86-89）→ TO_TRACEFORK 条件继承 tracer/trace_flags/sigtrace（:91-95）→ PRIV_PROC 子经普通 fork 时 `scheduler=SCHED_PROC_NR`（:100-103）→ flags 过滤 `IN_USE\|DELAY_CALL\|TAINTED` + 计账字段清零 + endpoint/intervals/started 重置（:106-114）→ eventsub 断言（:116）→ `get_free_pid`（:119-120）→ `tell_vfs(VFS_PM_FORK)`（:122-130）→ tracer 存在则 `sig_proc(SIGSTOP,trace=TRUE)`（:133-134）→ 返回 SUSPEND（:139） | forkexit.c:44-140 |
| forkexit.c | `do_srv_fork`：与 do_fork 同骨架，5 处差异——RS 权限门（endpoint!=RS_PROC_NR→EPERM，:158-160）；flags 保留 `IN_USE\|PRIV_PROC\|DELAY_CALL`（:200）；凭证六字段从消息注入 uid/gid（:206-211）；VFS_PM_SRV_FORK 载荷携带真实 REUID/REGID（:223-228）；立即 `reply(child,OK)` 并返回 pid，双回复不经 SUSPEND（:236-239） | forkexit.c:145-240 |
| forkexit.c | `do_exit`：PRIV_PROC 违规走 `sys_kill(SIGKILL)`（:253-257），否则 `exit_proc(mp, status, FALSE)`；恒返回 SUSPEND（"can't communicate from beyond the grave"，:261） | forkexit.c:245-262 |
| forkexit.c | `exit_proc`：core dump 双重豁免（setuid 进程不 dump :285-286；PRIV_PROC 不 dump :291-292）→ 记忆会话首进程组（:298）→ `ALARM_ON` 则 `set_alarm(rmp,0)` 撤钟（:301）→ `sys_times` 计账进 child 桶（:306-309）→ 未停则 `sys_stop` 强停（:326-330）→ `vm_willexit`（:332-334）→ INIT 特例打印并 stacktrace 返回（:336-341）、VFS 死亡 panic（:342-345）→ `tell_vfs(VFS_PM_EXIT/VFS_PM_DUMPCORE)`（:350-359）→ PRIV_PROC 立即 `sys_clear` 直毁不等 VFS（:361-369）→ flags 过滤留 `IN_USE\|VFS_CALL\|PRIV_PROC\|TRACE_EXIT\|PROC_STOPPED` + 置 EXITING（:374-375）→ 非 dump 先 `zombify`（:384-385）→ 遍历全表：tracer_died + disinherit 重挂 INIT + VFS_CALL 者记 NEW_PARENT + 已 ZOMBIE 者 `check_parent(try_cleanup=TRUE)`（:388-409）→ 会话首则 `check_sig(-procgrp, SIGHUP)`（:411-412） | forkexit.c:267-413；顺序依据注释 :311-315 |
| forkexit.c | `exit_restart`：`sched_stop`（失败仅报告，:425-434）→ `mp_scheduler=NONE`（:441）→ core dump 场景此处补 `zombify`（:444-445）→ 非 PRIV `sys_clear`（:447-452）→ `vm_exit` 释放内存（:455-457）→ `TRACE_EXIT` 则唤醒 tracer 完成 ptrace(T_EXIT)（:459-464）→ `TOLD_PARENT` 则 `cleanup`（:467-468） | forkexit.c:418-469 |
| forkexit.c | `do_wait4`：pidarg 四态归一化（0→-procgrp，:490-493）→ 全表扫描三过滤器（IN_USE 且非 TOLD_PARENT :502；parent 或 tracer 匹配 :503；tracer 扫描时 ZOMBIE 伪父跳过 :504）→ tracer 环：TRACE_ZOMBIE→`tell_tracer`+`check_parent(TRUE)`+SUSPEND（:513-518），TRACE_STOPPED→扫 `mp_sigtrace` 逐个 `W_STOPCODE(i)` 直接以 pid 为返回值回复（:519-534）→ 父环：ZOMBIE→`tell_parent`+条件 cleanup+SUSPEND（:537-547）→ 无可收但有子：WNOHANG 返 0（:553-555），否则置 WAITING/wpid/waddr 挂起（:556-559）；无子 ECHILD（:562） | forkexit.c:474-564 |
| forkexit.c | `wait_test`：`WAITING && (pidarg==-1 \|\| ==child_pid \|\| -pidarg==child_procgrp)`（:569-588）；`zombify`：tracer 非父先 TRACE_ZOMBIE 通知 tracer（tracer 未 wait 则停在 TRACE_ZOMBIE），否则置 ZOMBIE，再 `check_parent(FALSE)`（:593-624）；`check_parent`：父 EXITING 跳过 / 父在 wait 则 `tell_parent`+条件 cleanup / 否则 `sig_proc(父,SIGCHLD)`（:629-665）；`tell_parent`：rusage 两字段 `sys_datacopy`（失败 reply(errno) 留僵尸返 FALSE，:694-709）→ `W_EXITCODE` 填 `mp_reply` + `reply(pid)` + 清 WAITING + ZOMBIE→TOLD_PARENT + child 计账累计进父（:711-725）；`tell_tracer`：`W_EXITCODE(exitstatus, sigstatus&0377)` + TRACE_ZOMBIE→ZOMBIE 转正（:731-754）；`tracer_died`：清 tracer/TRACE_EXIT；非 EXITING 子 `sig_proc(SIGKILL)` 级联（:775-779）；TRACE_ZOMBIE 子转 ZOMBIE 交 real parent（:784-789）；`cleanup`：pid=0、flags=0、child 计账清零、`procs_in_use--`（:795-806） | forkexit.c:569-806 |
| signal.c (855) | `do_sigaction`：SIGKILL 恒 OK（:49）；边界 1.._NSIG（:50）；oact 非零先拷出旧值（:53-57）；act 为零仅查询（:59-60）；三态位图联动——SIG_IGN 置 ignore 清 pending/ksigpending/catch（:67-71），SIG_DFL 清 ignore/catch（:72-74），其余置 catch 清 ignore（:75-78）；写 sa_mask（剥 KILL/STOP）与 sa_flags，记 `mp_sigreturn`（:79-84） | signal.c:40-86 |
| signal.c | `do_sigpending`/`do_sigprocmask`/`do_sigsuspend`/`do_sigreturn`：sigpending 直读位图（:91-97）；sigprocmask 四 how（SIG_BLOCK 剥 KILL/STOP 后并集 :123-130；SIG_UNBLOCK 逐位清除后 `check_pending` :132-138；SIG_SETMASK 同剥后整替+check_pending :140-145；SIG_INQUIRE :147；默认 EINVAL）；sigsuspend 存 mask2→换 mask（剥 KILL/STOP）→置 SIGSUSPENDED→check_pending→SUSPEND（:160-171）；sigreturn 恢复 mask 后 `sys_sigreturn(ctx)`+check_pending（:176-192）；四处共用前置断言 `!(PROC_STOPPED\|VFS_CALL\|UNPAUSED\|EVENT_CALL)`（:46/:93/:117/:162/:183） | signal.c:88-192 |
| signal.c | `do_kill`：`check_sig(pid, sig, ksig=FALSE)`（:197-202）；`do_srv_kill`：RS 门（:212-213）后以 ksig=TRUE 调 check_sig（:207-221） | signal.c:194-221 |
| signal.c | `stop_proc(rmp, may_delay)`：`sys_delay_stop`；OK→置 PROC_STOPPED 返 TRUE；EBUSY→may_delay 才置 DELAY_CALL 返 FALSE，否则 panic（"unexpected delay call"）；其它 panic（:226-261）；`try_resume_proc`：VFS_CALL/EVENT_CALL/EXITING 不恢复，否则 `sys_resume` 并清 PROC_STOPPED/UNPAUSED（:266-289） | signal.c:223-289 |
| signal.c | `process_ksig(ep, signo)`（SEF 信号管理回调，main.c:122 注册；libsys/sef_signal.c 在 receive 路径拦 SIGKSIG 唤入）：endpoint/IN_USE 双检失败返 EDEADEPT（:300-310）；借 mproc[0] 伪造 PM 调用者上下文并借 procgrp（:312-313）；SIGINT/QUIT/WINCH/INFO→id=0 广播到进程组，SIGVTALRM/SIGPROF 先 `check_vtimer` 重设再落 default（:320-333）；`check_sig(id, signo, ksig=TRUE)`（:334）；SIGSNDELAY 尾部：清 DELAY_CALL，VFS/EVENT 在途则 `stop_proc(FALSE)` 后返回，否则 `check_pending`（:344-369）；末尾存活复检决定 OK/EDEADEPT（:372-377） | signal.c:294-378 |
| signal.c | `sig_proc(rmp, signo, trace, ksig)` 九判定链：IN_USE 断言（:407）；trace 先行——tracer 存在且非 SIGKILL 则 `sigaddset(mp_sigtrace)`+未 TRACE_STOPPED 则 `trace_stop`（:411-423）；VFS_CALL/EVENT_CALL→记 pending（ksig 另记 ksigpending）+未停未延迟则 `stop_proc(FALSE)`（:425-445）；PRIV_PROC 四分支——PM 自身广播跳过（:450-452）、非 ksig 一律 `sys_kill` 内核回环（:458-461）、stacktrace 信号打栈（:464-466）、非终止信号转 `SIGS_SIGNAL_RECEIVED` asynsend 消息、终止信号 `sig_proc_exit`（:468-479）；badignore=ksig∧noign∧(ignore∨mask)（:483-485）；ignore 丢弃（:487-490）；block 记 pending（:491-497）；TRACE_STOPPED 仅 SIGKILL 例外放行、其余记 pending（:499-508）；catch→`unpause` 失败记 pending 返回，成功 `sig_send`，失败（EFAULT/ENOMEM）打印并落到终止（:509-532）；ign_sset 默认忽略（:533-536）；终止 `sig_proc_exit`（:539） | signal.c:384-540 |
| signal.c | `sig_proc_exit`：记 sigstatus；core_sset 成员且非 PRIV 则 stacktrace + `exit_proc(rmp,0,dump_core=TRUE)`，否则 FALSE（:546-563）；`check_sig(proc_id, signo, ksig)`：界检 EINVAL（:582）；INIT+SIGKILL EINVAL（:585）；SIGTERM 广播先杀 RS（:588-589）；**逆序**全表扫描（:597）；选择四态（>0 精确 / ==0 同进程组 / ==-1 除 pid<=INIT / <-1 组匹配，:601-604）；广播 SIGKILL 跳过 PRIV（:607-608）；VM 跳过防死锁（:613）；非 ksig 致命信号对 PRIV 记 EPERM（:616-619）；权限五重（eff==SUPER_USER 或 real/eff 交叉匹三，:622-629）；signo==0 或 EXITING 只计数（:632）；`sig_proc(rmp,signo,TRUE,ksig)`，pid>0 即 break（:638-640）；自杀者 SUSPEND 不回复（:644）；count 决定 OK/ESRCH/EPERM（:645） | signal.c:545-646 |
| signal.c | `check_pending`：1.._NSIG 扫 pending∧!mask，取 ksig 位后逐个 `sig_proc(FALSE trace)`；进入新 VFS/EVENT 调用即 break（PROC_STOPPED 兼作复查指示，:651-682）；`restart_sigs`：VFS/EVENT/EXITING 早退；TRACE_EXIT→`exit_proc(saved status)`；PROC_STOPPED→`check_pending`+`try_resume_proc`（:687-714）；`unpause`：UNPAUSED 已就绪返 TRUE；DELAY_CALL 忙返 FALSE；WAITING/SIGSUSPENDED 则 `stop_proc(FALSE)` 返 TRUE；其余未停则 `stop_proc(TRUE)`（失败 FALSE），发 VFS_PM_UNPAUSE 返 FALSE（:719-770）；`sig_send`：断言 PROC_STOPPED；组 sigmsg（mask2/sigmask 起底、handler、sigreturn；SA_NODEFER/RESETHAND 处理；清 pending 双位）；`sys_sigsend` EFAULT/ENOMEM 合法失败返 FALSE，其它错误 panic；WAITING/SIGSUSPENDED 则清两标志 + `reply(EINTR)` + `try_resume_proc`，否则断言 UNPAUSED（由 restart_sigs 稍后恢复）（:775-855） | signal.c:651-855 |
| alarm.c (344) | `do_itimer`：which 界检 NR_ITIMERS=3（:101）；set/get 至少其一（:107-110）；set 则 datacopy 拷入 + `is_sane_timeval`（:115-123）；REAL→get/set_realtimer；VIRTUAL/PROF→`getset_vtimer`（:125-144）；get 则拷出 ovalue（:147-151）；`ticks_from_timeval` 向上取整+溢出钳 LONG_MAX（:33-65）；`timeval_from_ticks`（:70-76）；`get_realtimer` 剩余=exp_time-uptime，已过期返 interval（:246-273）；`set_realtimer`→`set_alarm`+记 interval（:278-294）；`set_alarm`：ticks>0 则 `set_timer`+置 ALARM_ON，否则 ALARM_ON 时 `cancel_timer` 清位（:299-311）；`cause_sigalrm`（timer 回调）：endpoint 校验/存活/ALARM_ON 三检（:323-331），有 interval 则重挂否则清位（:337-339），借 mproc[0] 伪造 PM 上下文后 `check_sig(pid, SIGALRM, ksig=FALSE)`（:341-343）；`getset_vtimer`：nptr/optr 双指针喂 `sys_vtimer`（:159-216）；`check_vtimer`：SIGVTALRM/SIGPROF→VT_VIRTUAL/VT_PROF，有 interval 则重挂（:221-241） | alarm.c 全文 |
| getset.c (223) | `do_get` 七调用复用（GETGROUPS 二阶段：num==0 返计数、num<现有 EINVAL、datacopy 拷出返计数 :29-50；GETUID/GID 返 real + reply 载荷带 eff :51-59；GETPID 返 pid + 载荷带父 pid :61-64；GETPGRP :66-68；GETSID pid 零取自身否则 find_proc，返 procgrp :70-79；ISSETUGID 返 TAINTED 位 :80-82）；`do_set` 六调用统一以 tell_vfs+SUSPEND 收尾（SETUID 三元全替，判据 real!=uid∧eff!=root EPERM :111-126；SETEUID 仅 eff，判据加 svuid :128-141；SETGID/SETEGID 同型 :143-170；SETGROUPS root 门 + ngroups 界检 + datacopy 拷入 + GID_MAX 逐项校验 + 尾部清零 + VFS_PM_SETGROUPS 载荷 :172-204；SETSID procgrp==pid 即已是首进程 EPERM，否则 procgrp=pid :205-212） | getset.c 全文 |
| exec.c (200) | `do_exec`：六字段转发 VFS_PM_EXEC + SUSPEND（:37-56）；`do_newexec`：VFS/RS caller 门（:70-71）；endpoint 校验+`exec_info` datacopy（:73-81）；tracer==NO_TRACER 才允许 setuid（:86-89）；allow_setuid 时 eff_uid/eff_gid 取 args（:91-94）；svuid/svgid 恒随 eff（:96-98）；TAINTED 二重——setuid 位或 eff!=real 置位，否则清位（:84/:100-109）；progname 写 mp_name（:111-113）；frame_addr=stack_high-frame_len（:115-117）；置 PARTIAL_EXEC 哨兵（:120）；`do_execrestart`：RS 门 + `exec_restart`（:130-151）；`exec_restart`：失败时 PARTIAL_EXEC 已置→`sys_kill(SIGKILL)` 自毁，否则 reply 错误（:161-171）；成功清 PARTIAL_EXEC（:173）；catch 全表重置 DFL+清 sa_mask（:178-184）；tracer 存在且无 TO_NOEXEC→SIGTRAP（或 TO_ALTEXEC→SIGSTOP）`check_sig`，先于 sys_exec（:189-194）；`sys_exec(sp, name, pc, ps_str)`（:197-198） | exec.c 全文；ESCRIPT 宏 :31 为死代码（定义后零使用） |
| trace.c (276) | `do_trace`：T_OK（tracer 非空 EBUSY；`mp_tracer=mp_parent` :55-60）；T_ATTACH（find_proc/EXITING ESRCH :63-64；非 root 五重凭证门 :67-71；root 才可 trace PRIV :74-75；PRIV 不可 trace 人 :78；self/PM/VM 禁 :81-82；已 traced EBUSY :85；置 tracer+TO_NOEXEC+`sig_proc(SIGSTOP,trace)` :87-90）；T_STOP 对用户返回 EINVAL（:95-99）；T_READB_INS/T_WRITEB_INS（root 门在通用守卫**之前**，:101-134）；其余命令通用守卫（find_proc/EXITING/tracer==who_p/TRACE_STOPPED 否则 EBUSY，:140-143）；T_EXIT 置 TRACE_EXIT，VFS/EVENT 在途暂存 exitstatus 否则 `exit_proc`，SUSPEND（:146-159）；T_SETOPT 直写 trace_flags（:161-165）；T_GETRANGE/T_SETRANGE（pr 校验 + `sys_vircopy` 双向 :167-188）；T_DETACH（信号界检；清 tracer；sigtrace 全量重放 `check_sig`；data>0 再 `sig_proc`；清 TRACE_STOPPED/trace_flags；`check_pending`，:190-215）；T_RESUME/T_STEP/T_SYSCALL（data>0 发信号；sigtrace 残留则伪成功提前 OK；清 TRACE_STOPPED+check_pending，:217-242）；落到 `sys_trace` 内核透传，读值回填 data（:244-249）；`trace_stop`：`sys_trace(T_STOP)`+置 TRACE_STOPPED；tracer 在 wait 则 `W_STOPCODE(signo)` 回复（:255-276） | trace.c 全文 |
| time.c (131) | `do_gettime`：`getuptime(&ticks,&realtime,&boottime)`；REALTIME→realtime / MONOTONIC→ticks / 其它 EINVAL；sec=boottime+clock/hz，nsec=(clock%hz)*1e9/hz（:21-47）；`do_getres`：两时钟返 nsec=1e9/hz（:52-65）；`do_settime`：root 门；REALTIME→`sys_settime(now,clk,sec,nsec)`；MONOTONIC/其它 EINVAL（:70-88）；`do_time`：`clock_time(&tv)` 直通（:93-104）；`do_stime`：root 门；boottime=sec-realtime/hz 重定锚点；`sys_stime(boottime)`（:109-131） | time.c 全文 |
| misc.c (447) | `do_sysuname`：field 界检+uts_tbl[8] 间接（NULL 槽 EINVAL；req==0 拷出串、req 其它 EINVAL；:71-100）；`do_getsysinfo`：effuid!=0 EPERM+stacktrace（:116-122）；SI_PROC_TAB→整个 mproc 表（:125-128）、SI_CALL_STATS→calls_stats（宏内 :129-134）；size 精确匹配+datacopy（:139-143）；`do_getprocnr`：RS 门+find_proc+回 endpoint（:149-164）；`do_getepinfo`：pm_isokendpt+回 uid/gid 四元+ngroups 全量回填但拷出数截断+groups datacopy+返 pid（:169-193）；`do_reboot`：root 门；abort_flag=how；RB_POWERDOWN 经 DS 找 readclock 发 RTCDEV_PWR_OFF（:210-216）；**定序注释**——先 `check_sig(-1,SIGKILL)` 杀全部用户进程、再 `sys_stop(INIT)`、再 `tell_vfs(VFS_PM_REBOOT)`、SUSPEND 永不回复（:218-232）；VFS 回信 VFS_PM_REBOOT_REPLY 在 handle_vfs_reply 里 `sys_abort(abort_flag)`（main.c:304-312）；`do_getsetpriority`：仅 PRIO_PROCESS（:250-252）；who==0 取自身否则 find_proc（:254-258）；权限 eff==root∨eff∨real 三重（:260-262）；GET 返 `nice-PRIO_MIN`（:265-267）；SET 提优需 root（EACCES :270-271）→`sched_nice`+写 mp_nice（:280-285）；`do_svrctl`：IOCGROUP 'P'/'M' 门（:307）；PM(SE)TPARAM/OPM(SE)PARAM 四命令——sysgetenv datacopy 拷入（EFAULT :322-323）；SET：local_params<2（ENOSPC）、keylen/vallen 边界（EINVAL）、双 datacopy 落 local_param_overrides（:326-350）；GET：keylen==0 全表拷 monitor_params，否则 search_key 拷入+终结符+先查 local 覆盖再 `find_param`（ESRCH）；val_len>vallen E2BIG；MIN 截断拷出（:352-389）；`do_getrusage`：who 界检；SELF→`sys_times`、CHILDREN→child 桶；`set_rusage_times` tick→usec；`vm_getrusage` 补 VM 字段；datacopy 拷出（:400-447） | misc.c 全文；uts 兼容块 :31-61 |
| schedule.c (112) | `sched_init`（见 B9）；`sched_start_user(ep,rmp)`：`nice_to_priority(mp_nice,&maxprio)`；父为 PRIV_PROC 则 inherit_from=INIT 否则父 endpoint；`sched_inherit(ep, self, inherit_from, maxprio, &mp_scheduler)`（:55-84）；`sched_nice`：scheduler==KERNEL/NONE EINVAL；`nice_to_priority`；`_taskcall(scheduler, SCHEDULING_SET_NICE)`（:89-112） | schedule.c 全文 |
| utility.c (156) | `get_free_pid`：静态 next_pid 自 INIT_PID+1 起先自增再回绕（相位=外部可观察契约）；跳过与 mp_pid **或 mp_procgrp** 冲突的值（:34-51）；`find_param`：monitor_params 键值线性扫描（:56-71）；`find_proc`：IN_USE+pid 扫描（:76-86）；`nice_to_priority`：界检 EINVAL；线性缩放 `MAX_USER_Q + (nice-PRIO_MIN)*(MIN-MAX+1)/(PRIO_MAX-PRIO_MIN+1)` + 双向钳位（:91-103）；`pm_isokendpt`：槽界 EINVAL→endpoint 不符 EDEADEPT→非 IN_USE EDEADEPT→OK（:108-118）；`tell_vfs`：VFS_CALL/EVENT_CALL 在途 panic（"not idle"）；`asynsend3(VFS, AMF_NOREPLY)`；置 VFS_CALL（:123-139）；`set_rusage_times`：tick→usec→sec/usec 双字段（:144-156） | utility.c 全文 |
| profile.c (45) | `do_sprofile`：`#if SPROFILE` PROF_START/PROF_STOP 透传 `sys_sprof`；默认编译返回 ENOSYS（:20-45） | profile.c |
| mcontext.c (27) | `do_getmcontext`/`do_setmcontext`：`sys_getmcontext/sys_setmcontext` 直透（全文） | mcontext.c |
| main.c 余量 | `reply`：槽界 panic；预填 `rmp->mp_reply.m_type=result` 后 `ipc_sendnb(endpoint)`，失败仅 printf（:249-270）；`get_nice_value(queue)`：queue→nice 线性缩放+钳位（:275-289）；`handle_vfs_reply`：见 §1.2.4 | main.c:249-289 |

#### 1.2.4 VFS 异步回复收口（handle_vfs_reply，main.c:295-424）

| 步 | 动作 | 锚点 |
|----|------|------|
| V1 | VFS_PM_REBOOT_REPLY 特判：无进程关联，`sys_abort(abort_flag)` 后等待 HARD_STOP notify，直接 return | main.c:304-312 |
| V2 | 取 `VFS_PM_ENDPT`，pm_isokendpt 失败 panic；无 VFS_CALL 则 panic（"reply without request"） | main.c:315-325 |
| V3 | 读 NEW_PARENT 后连同 VFS_CALL 一起清除；UNPAUSED 在入口处断言为空 | main.c:327-331 |
| V4 | 十一路 switch：SETUID/SETGID/SETGROUPS_REPLY→reply(OK)；SETSID_REPLY→reply(mp_procgrp)；EXEC_REPLY→`exec_restart(status,pc,sp,ps_str)`；CORE_REPLY→status==OK 置 WCOREFLAG 后**落穿** EXIT_REPLY；EXIT_REPLY→断言 EXITING+`publish_event`+return（续行交事件机制）；FORK_REPLY→`sched_start_user`（scheduler 非 KERNEL/NONE），失败则拆进程 `exit_proc(-1)` 并（非 NEW_PARENT）reply(父,-1)，成功 reply(子,OK)+reply(父,pid)；SRV_FORK_REPLY→空；UNPAUSE_REPLY→断言 PROC_STOPPED+置 UNPAUSED+`publish_event`+return | main.c:334-419 |
| V5 | 尾部：`(IN_USE\|EXITING)==IN_USE` 才 `restart_sigs(rmp)` | main.c:421-423 |

### 1.3 真序表的使用说明

上表共 9（启动）+ 6（循环）+ 47 调用面（按 C 文件与函数罗列）+ 5（VFS 收口）个可核对条目，每条带 `文件:行` 锚点。它是第 5 节每篇契约"事实底线"与第 8 节序差表的事实来源。B 相写正文时**必须**回到这些锚点重读源码，不得转述本表的文字。

---

## 2. 知识点全集

编号规则：`K-NNN`，stage 内唯一。类型取：概念 / 机制 / 数据结构 / 接口协议 / 约束不变量 / 架构演进 / 工具工程 / 测试性质。来源类型：**存量**（现有文档已讲，需回答去向）或**新增**（现有文档未讲，需证据锚点）。"主" 标记重复讲述点中的权威篇。现有位置以 `篇号§` 简写（如 05§2 = 05-vfs-interaction.md 第 2 章）。

### 2.1 存量知识点（来自现有 22 篇文档）

| 编号 | 名称 | 类型 | 现有位置 | C/制品锚点 | 读者收益 |
|------|------|------|---------|-----------|---------|
| K-001 | 微内核把进程语义放在用户态：PM 的角色边界 | 概念 | 00§1.1 | main.c 全文结构 | 理解 PM 与内核/VM/VFS 的分工 |
| K-002 | PM 四重权威（进程表所有者/信号管理器/生命周期编排者/记账方） | 概念 | 00§1.2 | mproc.h、main.c:122、forkexit.c | 定位任何 PM 代码的职责归属 |
| K-003 | 主循环骨架（收消息→notify→验证→丢弃→三路分发→条件回复） | 机制 | 00§1.3、04 | main.c:59-107 | 建立一切请求处理的坐标系 |
| K-004 | 启动链八步（sef_cb_init_fresh） | 机制 | 01 | main.c:131-244 | 理解第一代进程世界如何建立 |
| K-005 | SEF 回调注册与信号管理器注册 | 机制 | 01 | main.c:115-126 | 理解 PM 如何被框架包装与重启 |
| K-006 | boot image 填充循环（INIT 特例/系统进程/VFS 逐条告知） | 机制 | 01 | main.c:177-229 | 理解 mproc 初始内容来源 |
| K-007 | VFS_PM_INIT 握手与 ENDPT=NONE 屏障 | 接口协议 | 01 | main.c:221-236 | 理解 PM↔VFS 启动同步协议 |
| K-008 | `struct mproc` 全字段语义（约 40 字段） | 数据结构 | 02 | mproc.h:22-83 | 进程元数据的权威地图 |
| K-009 | 19 个 mp_flags 正交位 + MP_MAGIC | 数据结构 | 02 | mproc.h:85-107 | 状态判定的位级事实 |
| K-010 | mpsigact 独立表（80% 体积隔离，MIB 不拉入） | 数据结构 | 02 | mproc.h:17-20 | 理解 sigaction 存储为何外置 |
| K-011 | Rust 四层 Process 模型（Identity/State/Resources/Context） | 架构演进 | 02 | mproc/mproc.rs:334 等 | 理解位 flags→类型化分层的映射 |
| K-012 | flags→互斥枚举重构（Lifecycle/BlockState/WaitState） | 架构演进 | 02 | mproc/{lifecycle,block,wait}.rs | A-2 演进的对照读法 |
| K-013 | 三层身份：slot/endpoint/pid | 概念 | 03 | endpoint.h、utility.c:108 | IPC 身份与 POSIX 身份的分离 |
| K-014 | endpoint generation 代际编码 | 概念 | 03、99 | endpoint.h | 防陈旧 endpoint 引用 |
| K-015 | `pm_isokendpt` 三级验证 | 机制 | 03 | utility.c:108-118 | caller 验证的唯一入口 |
| K-016 | PID 生成器：NR_PIDS=30000 轮转 + 先自增再返回相位 + 组冲突跳过 | 机制 | 03 | utility.c:34-51、const.h:3 | 理解 pid 相位是外部可观察契约 |
| K-017 | `find_proc` pid→slot 线性查找 | 机制 | 03 | utility.c:76-86 | 身份反查 |
| K-018 | C 全局七件套（m_in/who_p/who_e/call_nr/mp/system_hz/abort_flag）及其 Rust 显式化（A-3） | 架构演进 | 99、03 | glo.h:16-23 | 理解重入不可分析→借用检查器作审计 |
| K-019 | 事件循环模型：单线程服务器永不返回 | 概念 | 04 | main.c:59 | 理解一切"挂起"的本质 |
| K-020 | SUSPEND 语义与三子类（等 VFS 回复/等子进程/永不回复） | 机制 | 04（主）、05 | main.c:87/:106、plan §7.3 | 回复时机的判别 |
| K-021 | ReplyIntent 三变体建模（A-6） | 架构演进 | 04 | ipc/dispatcher.rs | SUSPEND 的类型化 |
| K-022 | call_vec 47 项分发表与 do_get/do_set 一对多复用 | 数据结构 | 04 | table.c:14-62 | 调用面全景 |
| K-023 | EXITING 丢弃延迟调用 | 约束不变量 | 04 | main.c:81-82、forkexit.c:317-325 | 退出进程消息保护 |
| K-024 | CLOCK notify 驱动 expire_timers（载荷 timestamp） | 机制 | 04、14 | main.c:65-71 | 定时器驱动源 |
| K-025 | reply 预填 mp_reply 机制 | 机制 | 04 | main.c:249-270 | 回复载荷的组装方式 |
| K-026 | PM↔VFS 为什么必须异步（双向同步死锁） | 概念 | 05 | utility.c:134 | VFS_CALL 存在的理由 |
| K-027 | tell_vfs 三段式（NotIdle panic→asynsend3→置 VFS_CALL） | 机制 | 05（主） | utility.c:123-139 | 全部 VFS 转发的公共路径 |
| K-028 | VFS_PM_RQ 12 请求 / VFS_PM_RS 11 回复消息面 | 接口协议 | 05 | com.h:513-542 | 跨服务线格式 |
| K-029 | handle_vfs_reply 十一路状态机（含 CORE→EXIT 落穿、NEW_PARENT 保护） | 机制 | 05（主） | main.c:295-424 | 异步收口的全部路径 |
| K-030 | VFS_CALL 延续（continuation）编程风格 | 概念 | 05 | mproc.h flags | 用标志位替代阻塞的状态延续 |
| K-031 | 进程事件订阅的存在理由（SysV IPC 阻塞打断） | 概念 | 06 | event.c:1-46 | 理解 EVENT_CALL 面 |
| K-032 | NR_SUBS=4 串行化投递（有界异步槽论证） | 机制 | 06 | event.c:52-67 | 设计取舍的证据 |
| K-033 | subs[]/mp_eventsub 游标推进与 remove_sub 回退 | 机制 | 06 | event.c:97-160 | 订阅表维护 |
| K-034 | do_proceventmask（PRIV 门/mask 替换/延迟删除） | 机制 | 06 | event.c:170-211 | 订阅注册 |
| K-035 | do_proc_event_reply 五重校验 | 机制 | 06 | event.c:218-309 | 事件回复防伪 |
| K-036 | publish_event（EXIT/SIGNAL 事件判定 + 订阅者死亡摘除） | 机制 | 06（主） | event.c:316-353 | 事件入口 |
| K-037 | resume_event 终止分派（exit_restart/restart_sigs） | 机制 | 06 | event.c:74-123 | 事件与生命周期/信号的衔接 |
| K-038 | fork 是多服务协同（VM 地址空间/VFS fd 表/内核 proc/PM mproc 四表） | 概念 | 07 | forkexit.c、com.h | 容器复制的全局图 |
| K-039 | do_fork 全链路（容量门→槽轮转→vm_fork→复制→PID→VFS→tracer→SUSPEND） | 机制 | 07（主） | forkexit.c:44-140 | 进程创建的主路径 |
| K-040 | LAST_FEW=2 非 root 预留 | 约束不变量 | 07 | forkexit.c:32/:60-65 | 容量门的 root 特权 |
| K-041 | vm_fork 后不可失败窗口 | 约束不变量 | 07 | forkexit.c:82 注释 | 两阶段不可回滚边界 |
| K-042 | next_child 轮转槽位 | 机制 | 07 | forkexit.c:68-75 | 槽分配策略 |
| K-043 | TO_TRACEFORK 条件继承（tracer/flags/sigtrace 三重置 vs 保留） | 约束不变量 | 07 | forkexit.c:91-95 | 调试跨 fork 的语义 |
| K-044 | do_srv_fork 五差异（权限门/flags 保留/凭证注入/VFS 载荷/立即双回复） | 机制 | 08 | forkexit.c:145-240 | 系统服务孵化 |
| K-045 | PRIV_PROC 的 scheduler=NONE vs fork 子 SCHED_PROC_NR 接管 | 约束不变量 | 08 | forkexit.c:100-103/:200 | 两族进程的调度归属 |
| K-046 | 退出两阶段（exit_proc 前 9 步/exit_restart 后 5 步，VFS_PM_EXIT 为界） | 机制 | 09（主） | forkexit.c:267-469 | 顺序敏感的解绑 |
| K-047 | 解绑顺序注释（先 kernel stop→VFS→再 kernel clear） | 约束不变量 | 09 | forkexit.c:311-315 | 驱动取消请求窗口 |
| K-048 | zombify 两级僵尸（TRACE_ZOMBIE→ZOMBIE 转正） | 机制 | 09（主） | forkexit.c:593-624 | 僵尸生产 |
| K-049 | check_parent 三分支（父退出中/父在 wait/发 SIGCHLD） | 机制 | 09（主） | forkexit.c:629-665 | 僵尸通知 |
| K-050 | disinherit 收养 INIT + NEW_PARENT 记忆 | 机制 | 09 | forkexit.c:388-409 | 孤儿处理 |
| K-051 | tracer_died 三分支（SIGKILL 级联/TRACE_ZOMBIE 转正/清 TRACE_EXIT） | 机制 | 09、18 | forkexit.c:759-790 | 调试器死亡处理 |
| K-052 | 会话首 SIGHUP 广播（procgrp 记忆→check_sig(-procgrp)） | 机制 | 09 | forkexit.c:298/:411-412 | 会话终止语义 |
| K-053 | PRIV_PROC 直毁不等 VFS | 约束不变量 | 09 | forkexit.c:361-369 | 块设备驱动死锁规避 |
| K-054 | wait4 pidarg 四态归一化 | 机制 | 10 | forkexit.c:490-508 | POSIX wait 语义 |
| K-055 | do_wait4 三环扫描（TRACE_ZOMBIE→TRACE_STOPPED→ZOMBIE） | 机制 | 10（主） | forkexit.c:501-548 | 回收优先序 |
| K-056 | wait_test 双条件（WAITING∧right_child） | 机制 | 10 | forkexit.c:569-588 | wait 匹配谓词 |
| K-057 | tell_parent（rusage 两字段 datacopy 失败留僵尸/W_EXITCODE/TOLD_PARENT/计账累计） | 机制 | 10 | forkexit.c:670-726 | 僵尸消费与防重 |
| K-058 | tell_tracer（W_STOPCODE/0377 截断/TRACE_ZOMBIE→ZOMBIE） | 机制 | 10、18 | forkexit.c:731-754 | tracer 伪父回收 |
| K-059 | cleanup 与 procs_in_use 配对（07++/10--） | 约束不变量 | 10 | forkexit.c:795-806 | 表计数守恒 |
| K-060 | 信号四处置与 SignalState 位图（ignore/catch/mask/pending+ksigpending） | 数据结构 | 11、12（主 12） | mproc.h:57-66 | 信号契约的物化 |
| K-061 | do_kill/do_srv_kill 与 pid 四态选择 | 机制 | 11 | signal.c:197-221/:568-646 | 信号生成入口 |
| K-062 | check_sig 权限五重与逆序扫描（RS 先杀/VM 跳过/PRIV 致命 EPERM/广播 SIGKILL 跳 PRIV） | 机制 | 11（主） | signal.c:597-646 | 信号权限模型 |
| K-063 | sig_proc 九判定链（trace 先行→VFS/EVENT 挂起→PRIV 消息化→badignore→ignore→block→TRACE_STOPPED→catch→terminate） | 机制 | 11（主） | signal.c:384-540 | 唯一投递函数 |
| K-064 | 三信号集合 core/ign/noign（建构在 init，消费在 sig_proc） | 数据结构 | 99、11、01 | main.c:137-165 | 默认处置的权威 |
| K-065 | badignore（ksig∧noign∧(ignore∨mask)） | 约束不变量 | 11 | signal.c:483-485 | 致命信号不可逃避 |
| K-066 | PRIV_PROC 信号消息化（SIGS_SIGNAL_RECEIVED asynsend / sys_kill 回环 / stacktrace） | 机制 | 11 | signal.c:448-480 | 系统进程信号通道 |
| K-067 | sig_proc_exit 与 core dump 判定 | 机制 | 11（主） | signal.c:546-563 | 终止与 dump 分岔 |
| K-068 | process_ksig（EDEADEPT 双检/借用 mproc[0]/进程组广播/vtimer 重设/SIGSNDELAY 尾部） | 机制 | 11（主） | signal.c:294-378 | 内核信号处理 |
| K-069 | do_sigaction 三态位图联动与 sa_mask 剥 KILL/STOP | 机制 | 12（主） | signal.c:40-86 | 安装语义 |
| K-070 | sigprocmask 四 how（UNBLOCK/SETMASK 触发 check_pending） | 机制 | 12 | signal.c:102-155 | 掩码语义 |
| K-071 | sigsuspend/sigreturn 对（mask2 保存/SIGSUSPENDED/EINTR 打断） | 机制 | 12 | signal.c:160-192/:832-852 | 原子等待对 |
| K-072 | sig_send（sigmsg 组装/SA_NODEFER/RESETHAND/EFAULT-ENOMEM 分档/WAITING 打断） | 机制 | 12（主） | signal.c:775-855 | 捕获投递 |
| K-073 | stop_proc/try_resume_proc（EBUSY→DELAY_CALL/may_delay 契约） | 机制 | 13（主） | signal.c:226-289 | 内核停等原语 |
| K-074 | unpause 三路径（UNPAUSED 就绪/DELAY_CALL 忙/WAITING 停住/VFS_PM_UNPAUSE） | 机制 | 13 | signal.c:719-770 | 中断阻塞调用 |
| K-075 | check_pending（pending∧!mask 重投/遇 VFS 即 break） | 机制 | 13（主） | signal.c:651-682 | 挂起信号复查 |
| K-076 | restart_sigs（TRACE_EXIT 优先/PROC_STOPPED 复查+恢复） | 机制 | 13 | signal.c:687-714 | VFS 回复后的信号续行 |
| K-077 | DELAY_CALL 与 SIGSNDELAY 配对 | 约束不变量 | 13、11 | signal.c:239-256/:344-369 | 延迟停止兑现 |
| K-078 | PROC_STOPPED 双用途（已停状态 + restart_sigs 复查指示） | 约束不变量 | 13 | signal.c:430-436 注释 | 位语义重载 |
| K-079 | itimer 三族时钟源分野（REAL 本地 timer/VIRTUAL-PROF 内核 vtimer） | 概念 | 14 | alarm.c、sys/time.h | 三种计时语义 |
| K-080 | ticks 换算（向上取整/溢出钳 LONG_MAX/MAX_SECS） | 机制 | 14 | alarm.c:33-87 | 无溢出变换 |
| K-081 | set_alarm/cause_sigalrm/ALARM_ON 生命周期 | 机制 | 14（主） | alarm.c:299-344 | REAL 定时闭环 |
| K-082 | getset_vtimer 双指针与 check_vtimer 重挂 | 机制 | 14 | alarm.c:159-241 | 虚拟定时闭环 |
| K-083 | 凭证三元组 real/eff/saved + 16 补充组 | 数据结构 | 15（主） | mproc.h:43-53、getset.c | 身份模型 |
| K-084 | do_get/do_set 13 调用族与判据差异（NetBSD/BSD 语义注释） | 机制 | 15 | getset.c 全文 | 凭证调用面 |
| K-085 | GETGROUPS 二阶段协议（先问数再拷） | 接口协议 | 15 | getset.c:29-50 | 组读取协议 |
| K-086 | 凭证修改的 VFS 双副本协同（SET* → SUSPEND → reply） | 机制 | 15 | getset.c:218-222、main.c:335-347 | 身份一致性 |
| K-087 | TAINTED 污染位（issetugid 消费） | 约束不变量 | 15、17 | mproc.h、exec.c:100-109 | 特权污染追踪 |
| K-088 | 用户态调度交接（内核只给原语、SCHED 持策略） | 概念 | 16 | schedule.c、minix/sched.h | 策略/机制分离 |
| K-089 | sched_init/sched_start_user/sched_nice 与 KERNEL/NONE 守卫 | 机制 | 16（主） | schedule.c 全文 | 调度协议客户端 |
| K-090 | nice↔queue 双向线性换算（get_nice_value / nice_to_priority，16:41 缩放） | 机制 | 16、01 | main.c:275-289、utility.c:91-103 | 双函数辨析 |
| K-091 | do_getsetpriority（PRIO_PROCESS 唯一/三重权限/EACCES 提优限制） | 机制 | 16 | misc.c:239-286 | 优先级调用 |
| K-092 | exec 三段式（VFS 权限判断/PM 凭证更新/kernel 装载） | 概念 | 17（主） | exec.c | exec 分工 |
| K-093 | do_newexec（allow_setuid 与 tracer 门/TAINTED 二重/svuid 随 eff/frame 保存） | 机制 | 17 | exec.c:62-125 | exec 回调 |
| K-094 | PARTIAL_EXEC 哨兵与 SIGKILL 自毁 | 约束不变量 | 17 | exec.c:120/:161-171 | 半初始化态 |
| K-095 | exec 后 catch 重置（DFL+清 sa_mask，不清 sa_flags） | 约束不变量 | 17、12 | exec.c:178-184 | 新镜像信号重置 |
| K-096 | exec 的 tracer SIGTRAP/SIGSTOP 先于 sys_exec（TO_NOEXEC/TO_ALTEXEC） | 机制 | 17、18 | exec.c:189-194 | 调试 exec 拦截 |
| K-097 | ptrace 双入口（T_OK 自声明/T_ATTACH 主动附着权限链） | 机制 | 18（主） | trace.c:55-93 | 调试建立 |
| K-098 | TRACE_STOPPED 独立于 PROC_STOPPED（调试暂停 vs VFS 停） | 约束不变量 | 18 | trace.c:266 | 双轨暂停 |
| K-099 | mp_sigtrace 位图缓冲与 check_sig 全量重放（DETACH/RESUME） | 机制 | 18 | trace.c:190-242 | tracer 信号缓冲 |
| K-100 | trace_stop（sys_trace(T_STOP)/W_STOPCODE 通知） | 机制 | 18 | trace.c:255-276 | 调试停止 |
| K-101 | T_GETRANGE/T_SETRANGE 与 T_READB/WRITEB_INS（root 门位置差异） | 机制 | 18 | trace.c:101-188 | 内存读写族 |
| K-102 | 双时钟（REALTIME 可设/MONOTONIC 不可设）与 boottime 锚点 | 概念 | 19（主） | time.c | 时间语义 |
| K-103 | 五时间调用与无溢出分解（sec=boottime+clock/hz） | 机制 | 19 | time.c:21-131 | 时间调用面 |
| K-104 | uts_tbl[8] 兼容间接与 uts_val 单例 | 数据结构 | 20 | misc.c:31-61 | uname 兼容层 |
| K-105 | do_getsysinfo（SI_PROC_TAB 全表泄露 root 门/size 精确匹配） | 机制 | 20 | misc.c:107-144 | 信息边界 |
| K-106 | endpoint↔pid 双向解析（getprocnr/getepinfo） | 机制 | 20 | misc.c:149-193 | 身份解析调用 |
| K-107 | do_reboot 定序（kill→stop(INIT)→VFS_REBOOT→sys_abort 回收） | 机制 | 20（主）、05 | misc.c:198-233、main.c:304-312 | 关机因果链 |
| K-108 | do_svrctl 参数覆盖层（local_param_overrides[2]/四命令/E2BIG 判据） | 机制 | 20 | misc.c:291-395 | boot 参数控制面 |
| K-109 | getrusage 双源（sys_times/child 桶）+ VM 补字段 + tick→usec | 机制 | 20 | misc.c:400-447、utility.c:144 | 记账查询 |
| K-110 | sprofile 与 mcontext 直透 | 机制 | 20 | profile.c、mcontext.c | 透传调用 |
| K-111 | monitor_params/find_param 启动参数链 | 机制 | 20、01 | utility.c:56-71、glo.h:21 | 参数查找 |
| K-112 | 常量语义半径（NR_PIDS 轮转域/NO_TRACER 依赖槽 0/NO_EVENTSUB char 有符号） | 约束不变量 | 99 | const.h | 不可"现代化"的哨兵值 |
| K-113 | 47 调用号索引（PM_BASE 起连续） | 接口协议 | 99、04 | callnr.h | 调用号速查 |
| K-114 | mp_eventsub==NO_EVENTSUB 不变量 | 约束不变量 | 06、07 | forkexit.c:116/:216 | 事件订阅空态 |
| K-115 | draft/ 素材的定位（旧 fork 主线，停止维护，只取料） | 工具工程 | plan §1.1 | draft/README.md | 素材边界 |
| K-116 | 测试基建（lib 单测 + tests/run_once_integration.rs + mock 端口） | 测试性质 | 各篇 §5 | os/servers/pm/tests/ | 验证策略 |
| K-117 | ARCH A-1~A-13 清单（mproc 分层/枚举化/PmContext/类型化 IPC/match 分发/ReplyIntent/定时器抽象/SCHED 客户端/EventRegistry/DELAY_CALL 缺口/64 位类型/Guardianship/进程组会话） | 架构演进 | plan §4（正文散布各篇） | 各 Rust 模块 | 演进全景 |
| K-118 | Guardianship 双监护建模（parent/tracer）与 TO_TRACEFORK 继承 | 架构演进 | 02、09、18 | mproc/guardianship.rs | A-12 |
| K-119 | KernelGateway 中央端口与窄 trait 双风格规约（supertrait 组合先例） | 架构演进 | plan §4.1 | exit.rs:25、signal_flow.rs:269 | 端口架构 |
| K-120 | 错误保真规约（透传型调用错误带 Kernel(i32) 上抛，禁 map_err 折叠） | 架构演进 | plan §4.2 | 各错误枚举 | errno 是外部契约 |

### 2.2 新增知识点（现有文档未讲，审计发现，附证据锚点）

| 编号 | 名称 | 类型 | 证据锚点 | 发现路径 |
|------|------|------|---------|---------|
| K-121 | 内核信号回环入口：SEF signal manager→SIGKSIG→SYSTEM notify→process_sigmgr_signals 拉取循环（getksig/endksig） | 机制 | main.c:122；`minix/lib/libsys/sef_signal.c`（receive 路径拦 SIGKSIG/SIGKSIGSM）；os/servers/pm/src/signal.rs `process_sigmgr_signals`（todo.md V3-P1-2 / Fix #45：SIGSNDELAY=70、SIGKSIG=74 常量与 C 对齐 sys/sys/signal.h:264） | C 侧存在于 main.c 但文档只一笔带过；Rust 侧 2026-09-09 后新增 |
| K-122 | ipc/decode.rs 解码单点化：每调用一个 decode 函数收拢 unsafe 消息解码 + C ipc.h 字段对照注释 | 架构演进 | os/servers/pm/src/ipc/decode.rs（S11 提交 d18966fb7）；todo.md §12.3 观察 2 | Rust 新模块，文档零覆盖 |
| K-123 | mproc/wire.rs：ProcTable → C-ABI 464 字节 repr(C) mproc 镜像序列化（getsysinfo SI_PROC_TAB 真实拷出） | 架构演进 | os/servers/pm/src/mproc/wire.rs（提交 fbb4f5cd9，D-29 闭环）；minix-types types/mproc.rs | Rust 新模块，20 文档仍写"拷出恒零/未接线" |
| K-124 | 接线现状：39/47 调用已接真实分发臂，仅 8 个落 ENOSYS（25/26/27/36/37/38/39/47） | 工具工程 | os/servers/pm/src/ipc/calls.rs 分发臂逐条 grep（本次执行实测）；git log S1–S8 批次提交 | 00 文档断言 8/47，已失真 |
| K-125 | 全部 47 调用的 PmCall 枚举 `#[repr(i32)]` 与 from_call_nr 反查（calls.rs:141-187）已齐备，wire 载荷逐族落地中（MessPmLcWait4/SigMsgWire 等） | 接口协议 | calls.rs；minix-sys syscall.rs（34 个 sys_ wrapper） | 各篇"wire 未落地"表述过时 |
| K-126 | 批次 H 之后 notify 分派双源：CLOCK→expire_timers；SYSTEM→内核信号拉取 | 机制 | os/servers/pm/src/init.rs notify 分支（Fix #45）；C 侧同型（main.c:65-71 + sef_signal.c） | 04/05 文档未覆盖 SYSTEM 臂 |
| K-127 | MINIX3 PM 无锁并发的全部理由：单线程事件循环 + VFS_CALL/EVENT_CALL 延续标志替代锁 | 概念 | main.c 单循环结构；event.c:17-25 串行化论证 | 散见各篇但无一处集中声明 |
| K-128 | GetRUsage 等余下 8 调用的挂起原因（VM 对端/SCHED 服务器/wire 依赖），挂 edge 条目 | 工具工程 | edge_todo.md E5/E6/E7 清单；todo.md §11.1.1 批次表 | 各篇"未实现"需改为"挂起+原因" |

### 2.3 统计摘要

- 存量 120 条（K-001~K-120）：概念 18、机制 63、数据结构 16、接口协议 6、约束不变量 15、架构演进 8（K-117 为聚合条）、工具工程 1、测试性质 1。重复标记：K-020/K-027/K-029/K-036/K-039/K-046/K-048/K-049/K-055/K-060/K-062/K-063/K-067/K-068/K-069/K-072/K-073/K-075/K-081/K-083/K-089/K-092/K-097/K-102/K-107 等条目在多篇重复出现，主讲述点已在第 3.3 节重复主题表统一裁决。
- 新增 8 条（K-121~K-128），全部有代码或 C 锚点，无推测项。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

主题全集四路来源：(1) C 源码符号——15 个 .c 的全部函数（见 §1.2.3 表，47 个调用 handler + 17 个内部函数 + 启动/回复/工具）、mproc 40 字段 + 19 flag 位 + mpsigact、subs[4]/call_vec/uts_tbl/monitor_params/local_param_overrides；(2) 操作系统通用概念——进程状态机、地址空间移交、凭证模型、POSIX 信号语义、ptrace 协议、interval timer、僵尸与收养；(3) 非 C 制品主题——见 §3.5 逐项；(4) 阶段边界契约——master-plan 启动因果链中 PM 的位置、edge_todo.md 的 E1/E5/E6/E7 挂起项、plan.md §5.4 排除表。

### 3.2 覆盖缺口表

| # | 缺口主题 | 证据 | 建议 | 落实 |
|---|---------|------|------|------|
| Q1 | 内核信号入口（notify SYSTEM 臂 + process_sigmgr_signals 拉取循环） | K-121 | 新增知识点并入信号篇与主循环篇 | 05（notify 双源）+ 11（回环旅程） |
| Q2 | decode.rs 解码单点 | K-122 | 并入主循环篇的 Rust 层 | 05 |
| Q3 | wire.rs C-ABI 镜像与 getsysinfo 真实数据路径 | K-123 | 并入杂项篇 + 结构篇 | 20 + 02 |
| Q4 | 39/47 接线现状 | K-124 | 全部 21 篇的"实施现状"层按当前代码重写 | 各篇 |
| Q5 | 错误保真规约正文缺位（只在 plan.md） | K-120 | 升格为主循环篇的横切规约一节 | 05 |
| Q6 | KernelGateway 双风格规约与四列对照表缺位（只在 plan.md） | K-119 | 并入主循环篇骨架层 | 05 |
| Q7 | exec 族 43/44 的 caller 门与 S6 耦合解除现状 | todo.md S6 | exec 篇 Rust 层刷新 | 17 |
| Q8 | 时间族生产接线（S4：sys_settime/stime/GETUPTIME ClockSource）现状 | git 6b948e2ae | 时间篇 Rust 层刷新 | 19 |
| Q9 | 剩余 8 调用的挂起原因与解除条件 | K-128 | 杂项/调度/凭证篇各挂"挂起契约"小节 | 20、16、15 |
| Q10 | 信号集合建构步骤在启动链中的叙述归属不清（01 讲建构、99 讲语义、11 讲消费） | K-064 | 主讲述点定为信号篇，启动篇只记步骤并回指 | 04、11 |
| Q11 | SEF 框架作为非 C 制品（sef.c/sef_init.c/sef_signal.c）的接口契约无系统交代 | §3.5 | 启动篇增"SEF 壳"一节 | 04 |
| Q12 | 无锁并发声明（K-127） | §2.2 | 主循环篇模型声明一节 | 05 |

### 3.3 重复主题表（主讲述点裁决）

| 主题 | 现有重复位置 | 新目录主讲述点 | 其余位置处置 |
|------|-------------|---------------|-------------|
| tell_vfs / VFS_CALL / SUSPEND 挂起 | 04§、05§、07§、08§、09§、15§、17§ | 06（VFS 异步协议） | 各机制篇只写"经 tell_vfs 挂起（见 06）"+ 自身特有分支 |
| SUSPEND 三子类与回复时机 | 04、05、plan §7.3 | 05（主循环） | 06 只写 VFS 收口子类；机制篇引用 |
| handle_vfs_reply 十一路 | 05（全文） | 06 | 05 只留三路分发骨架中的一路钩子 |
| publish_event 调用点 | 05§、06§、09§ | 06 | 09 只写"退出事件经 publish_event（见 06/07）" |
| ZOMBIE/TRACE_ZOMBIE 生产消费 | 09、10、18 | 09（生产）+ 10（消费），18 引用 | 18 只写 TRACE_* 位与 wait 交互 |
| 三信号集合 core/ign/noign | 01、11、99 | 11（投递语义） | 04 只记建构步骤；01 只给成员常量表 |
| TAINTED | 15、17 | 17（置位语义，唯一置位点 exec.c） | 15 只写 issetugid 消费 |
| nice↔queue 双向换算 | 01、16、20 | 16（双函数辨析） | 04 只记 get_nice_value 调用点 |
| get_free_pid | 01、03、07 | 03（算法与相位） | 04/08 引用 |
| endpoint generation 编码 | 03、99 | 01（词汇表：编码与代数） | 03 写验证消费 |
| 47 调用号索引 | 04、99 | 05（分发表正文） | 01 保留速查表 |
| sig_proc 九判定链 | 11、12、13 互相引用 | 11 | 12/13 只写各自环节的调用点 |
| exit 两阶段 | 09（主） | 09 | 06 写 EXIT_REPLY 收口半、10 写 TOLD_PARENT 消费半，均引用 |
| Guardianship/收养 | 02、09、10、18 | 09（收养链） | 02 只给数据结构；18 引用 |

### 3.4 越界主题表

| 位置 | 越界内容 | 正确归属 |
|------|---------|---------|
| 旧 01 §源码声明 | `get_nice_value` 函数体（main.c:276）随启动篇展开 | 16（调度），01/新04 只留调用点（plan D-4 裁决保留） |
| 旧 20 § | `do_getsetpriority` 曾归杂项（plan D-7 已裁决归 16） | 16；20 保留转发说明 |
| 旧 15 § | TAINTED 置位语义展开 | 17（唯一置位点在 exec.c:100-109） |
| 旧 99 § | 47 调用号索引与常量混编 | 速查表留词汇篇，正文讲述归 05 |

### 3.5 非 C 主题逐项回答

| 非 C 主题 | 在哪里讲 | 理由 |
|-----------|---------|------|
| 链接与加载 | 不在本 stage | 内核职责（01-stage-kernel）；PM 的 ELF 装载经 VFS/exec 协议（17 篇只讲 PM 侧转发） |
| 镜像与内存布局 | 不在本 stage | 内核/boot 职责；PM 只消费 boot_image 表（04 篇 B5-B6 步） |
| 汇编入口与陷阱进入 | 不在本 stage | 内核/trap 层职责（edge E1）；PM 侧只有 minix-sys wrapper 消费面（00 现状节列点） |
| 启动装配 | 04（SEF 壳与八步初始化） | PM 是 boot 链的被动方，主动权在内核/RS（03-stage-rs）；本 stage 只讲"被启动后做什么" |
| 构建与工具链 | 00 一节声明 | C 侧 `minix3/minix/servers/pm/Makefile` 归 C 世界；Rust 侧 cargo workspace 成员；不做展开 |
| 跨模块接口与线格式 | 05（47 调用号与 decode）、06（VFS_PM_* 23 条）、07（PROC_EVENT/PROC_EVENT_REPLY）、16（SCHEDULING_*）、00（minix-sys wrapper 清单） | 各协议的 PM 侧视点；对端实现归各 stage |
| 错误路径 | 05（错误保真规约）+ 各机制篇 handler 错误分支 | errno 是外部可观察契约 |
| 关闭与退出 | 09（进程退出）+ 20（reboot 定序）+ 18（tracer 死亡级联） | 关闭语义分散在三处但各有主面 |
| 并发与同步 | 05（无锁模型声明：单线程事件循环 + 延续标志） | PM 无锁是事实而非缺口，集中声明一次 |
| 测试基建 | 00（测试基线一节）+ 各篇 §测试 | lib/integration 两层 + mock 端口策略 |

---

## 4. 新目录

### 4.1 新篇章总表（21 篇）

| 新编号 | 标题 | 一句话定位 | 分组 | 旧来源 |
|--------|------|-----------|------|--------|
| 00 | pm-overview：PM 的边界、分层与阅读路线 | 总览导航：四重权威、模块分层、实施现状快照、21 篇导航 | 导航 | 00（改写） |
| 01 | global-concepts：词汇表——常量、身份编码与全局状态 | 全部魔数的权威定义与语义半径 | 第一部分 词汇与数据 | 99（重排） |
| 02 | mproc-struct：进程结构——字段语义与状态分层 | mproc 一行的解剖与 Rust 四层建模 | 第一部分 | 02（保留，内容刷新） |
| 03 | mproc-table：进程表——槽位、endpoint 与 PID 的身份管理 | 表级操作与三层身份 | 第一部分 | 03（保留，内容刷新） |
| 04 | pm-init-main：启动链——SEF 壳与 sef_cb_init_fresh 八步 | PM 如何把自己初始化成进程语义权威 | 第二部分 骨架 | 01（重排） |
| 05 | ipc-dispatch：主循环与分发——事件循环、47 调用与回复模型 | run_once 六步、SUSPEND 模型、notify 双源、端口规约 | 第二部分 | 04（重排） |
| 06 | vfs-interaction：PM 与 VFS 的异步 IPC 协议 | tell_vfs 三段式与 handle_vfs_reply 十一路收口 | 第二部分 | 05（重排） |
| 07 | event-subscription：进程事件的发布与订阅 | NR_SUBS 串行化事件面 | 第二部分 | 06（重排） |
| 08 | fork-family：进程创建——do_fork 与 do_srv_fork | 同族调用：公共创建骨架（本蓝图重枚举为 12 原子步）+ 五差异表 | 第三部分 生命周期 | 07+08（合并） |
| 09 | pm-exit：退出路径与僵尸收养链 | 两阶段退出、两级僵尸、收养与会话广播 | 第三部分 | 09（保留，内容刷新） |
| 10 | pm-wait：等待与回收 | 三环扫描与僵尸消费 | 第三部分 | 10（保留，内容刷新） |
| 11 | signal-core：信号生成与分发核心 | 三个生成源→check_sig→sig_proc→sig_proc_exit 核心闭环 | 第四部分 信号 | 11（保留，内容刷新+内核回环） |
| 12 | signal-handlers：信号处理器的安装与投递 | sigaction 族、sig_send 与栈帧翻译 | 第四部分 | 12（保留，内容刷新） |
| 13 | signal-flow：延迟、停止与恢复 | DELAY_CALL/PROC_STOPPED/restart_sigs 状态机 | 第四部分 | 13（保留，内容刷新） |
| 14 | itimer：定时器 | 三族时钟源与 ticks 闭环 | 第五部分 单机制 | 14（保留，内容刷新） |
| 15 | credentials：身份与凭证 | 13 调用族与 VFS 双副本协同 | 第五部分 | 15（保留，内容刷新） |
| 16 | scheduling：调度 | 用户态调度交接与 nice↔queue 双射 | 第五部分 | 16（保留，内容刷新） |
| 17 | exec：执行替换 | VFS 转发三段式与 PARTIAL_EXEC 哨兵 | 第五部分 | 17（保留，内容刷新） |
| 18 | trace：调试（ptrace） | T_* 全族与 Traced 状态机 | 支线（调试） | 18（保留，内容重写） |
| 19 | time：时间 | 双时钟与五调用 | 第五部分 | 19（保留，内容刷新） |
| 20 | misc-queries：杂项与查询 | 信息边界与控制面（10+ 调用分组） | 第六部分 杂项 | 20（保留，内容刷新） |

说明：**没有新建篇**。第 6 节（缺漏新篇）逐项裁决了全部缺口主题的去向——它们全部并入现有篇章成为新增小节，因为每一条都寄生于某个已有机制（如内核信号回环寄生于信号分发的 `check_sig`），单独成篇会制造只有引用没有正文的空壳。

### 4.2 阅读路径

- **主线（必读序）**: 00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13。这条线覆盖词汇、数据、骨架、生命周期与信号全部主干，编号即阅读顺序，无前向引用。
- **第五部分（单机制，可按需选读）**: 14 → 15 → 16 → 17 → 19；每篇独立声明前置（均只指向主线内的更早编号），彼此无依赖。
- **支线（可跳读）**: 18（ptrace，调试场景才需要）；20（杂项查询，工具与信息面）。
- **WONTFIX 内容**: 集中在各篇"明确不做"小节（ESCRIPT 死代码、ENABLE_SYSCALL_STATS 计数、SPROFILE 默认 ENOSYS、rusage 其余字段 TODO），不散布。

### 4.3 并行主题的分组与代表成员

- 47 个 PM 调用是典型并行体：统一框架在 05（分派与回复模型）+ 06（异步收口框架），此后按场景分组——生命周期族（08/09/10）、信号族（11/12/13）、定时器（14）、凭证族（15）、调度（16）、exec 族（17）、调试族（18）、时间族（19）、信息控制族（20）。
- 组内代表精讲 + 差异表收束：fork 家族代表 do_fork（srv_fork 五差异表）；凭证族代表 setuid/seteuid（其余判据差异表）；杂项族代表 reboot（定序）与 getsysinfo（信息边界）；信号族代表 SIGTERM 常规终止路径（SIGKILL/SIGSTOP 特例差异表）。
- 主线/支线/可跳读已声明（§4.2）。

---

## 5. 每篇契约

> 每篇七要素：定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 + 验收标准。B 相按契约直接取料写正文。"（刷新）"表示编号不变但正文需按当前代码重写实施现状层与校准锚点。

### 00-pm-overview

- **一句话定位**: 让读者 10 分钟内建立"PM 是什么、管什么、代码在哪、读什么"的全局地图。
- **讲什么**: K-001、K-002（四重权威）；源码地图（15 .c → 21 篇导航表）；Rust 模块镜像；实施现状快照（**K-124：39/47 接线 + 剩余 8 调用名单**，快照日期必须写明）；测试基线一节（K-116）；工程位置（构建与工具链声明，§3.5 非 C 清单的"构建"行）；推荐阅读路线（主线/支线声明）。
- **不讲什么**: 一切机制细节（各篇）；接线台账与修复账目（指向 todo.md，注明其为活动文档）；SEF/内核对端（指向 04/11 与对应 stage）。
- **前置**: 无（本 stage 入口）。跨 stage 背景：boot 链见 `00-master-plan/README.md` 与 03-stage-rs overview（以"延伸阅读"形式给出，不构成依赖）。
- **后置**: 全部 21 篇。
- **事实底线**: `minix3/minix/servers/pm/` 文件清单与行数；`os/servers/pm/src/` 模块清单；calls.rs 分发臂实测（写文档当天重跑 grep）；测试基线以 `cargo test -p minix-pm` 当天实测为准。
- **知识点清单**: K-001、K-002、K-115、K-116、K-117（A-1~A-13 索引）、K-124、K-125（速览）、K-127（一句话）。
- **验收标准**: 导航表中每个 C 文件与每篇新文档一一对应无缺漏；"实施现状"数字与当天代码一致并注明快照日期；读者能据本篇回答"某个 errno 由哪个调用族产生"。

### 01-global-concepts

- **一句话定位**: 机制篇里每个魔数的权威定义表——读者遇到任何常量/哨兵/编码先来这里。
- **讲什么**: K-112（常量语义半径：NR_PIDS/NO_PID/INIT_PID/NO_TRACER/NO_EVENTSUB/MP_MAGIC/NR_ITIMERS/MAX_SECS/SEND_PRIORITY/SEND_TIME_SLICE）；K-013/K-014（slot/endpoint/pid 编码与 generation 代数）；K-113（47 调用号速查表，PM_BASE=0 连续）；K-009（19 flag 位值表——**位值速查在此，位语义在消费篇**）；K-064 成员常量表（core/ign/noign 三集合的信号编号清单，语义归 11）；K-018（C 全局七件套清单与 Rust 显式化对照，展开归 05）；K-011/K-012 的名词表（四层模型术语，展开归 02）；PROC_NAME_LEN/NGROUPS_MAX/GID_MAX 等边界常量。
- **不讲什么**: 任何机制的行为（各机制篇）；位图的操作函数（02/11）；PID 算法（03）。
- **前置**: 00。
- **后置**: 02–20 全部（词汇被全员消费）。
- **事实底线**: `servers/pm/const.h`（全文 20 行）、`mproc.h:85-107` flag 位、`minix/callnr.h`（PM_BASE 起的 47 个）、`minix/com.h:513-542/:619`（VFS_PM_RQ_BASE 0x900 / VFS_PM_RS_BASE 0x980 / PROC_EVENT_REPLY）、`sys/sys/signal.h`（SIGSNDELAY=70 等）、`minix/include/minix/sys_config.h:8`（_NR_PROCS 256）、`minix/type.h:145`（PROC_NAME_LEN 16）、`glo.h`。
- **知识点清单**: K-009、K-013、K-014、K-064（常量面）、K-112、K-113、K-118（名词面）。
- **验收标准**: 每个常量一行"值 + C 锚点 + 语义半径一句话"；抽查 10 个魔数都能从机制篇反查到本篇条目；表中不存在无锚点条目。

### 02-mproc-struct（刷新）

- **一句话定位**: 进程表一行的完整解剖——每个字段是什么、为什么在那里、Rust 怎么分层。
- **讲什么**: K-008（全字段）、K-009（位语义，与 01 的位值表分工）、K-010（mpsigact 外置）、K-011（四层模型）、K-012（枚举化映射逐位对照）、K-118（Guardianship 数据结构面）、K-123（wire.rs 四层→C-ABI 464 字节镜像，新增小节）。
- **不讲什么**: 表级操作（03）；状态流转（08–10）；位图的消费语义（11–13）。
- **前置**: 01。
- **后置**: 03、05–20（一切持 mproc 的篇）。
- **事实底线**: `mproc.h` 全文；`os/servers/pm/src/mproc/mproc.rs`（字段映射表与结构体）、`mproc/{lifecycle,block,wait,guardianship,trace,signal,credentials}.rs`、`mproc/wire.rs`、`minix-types/src/types/mproc.rs`。
- **知识点清单**: K-008、K-009、K-010、K-011、K-012、K-118、K-123。
- **验收标准**: 字段表覆盖 mproc.h 全部字段无遗漏；19 个 flag 位逐一给出"C 位值 / Rust 归属枚举或字段"对照；wire 镜像小节给出 464 字节与逐段偏移的对应说明。

### 03-mproc-table（刷新）

- **一句话定位**: PM 如何在 256 个槽位里管理"谁活着、谁是谁"。
- **讲什么**: K-013/K-014 的消费面（验证时机）、K-015（pm_isokendpt 三级）、K-016（PID 算法与相位契约）、K-017（find_proc）、procs_in_use 计数、槽位轮转的表视角（算法细节归 08）。
- **不讲什么**: 槽位的具体分配场景（08）；启动期填充（04）；endpoint 编码教学（01）。
- **前置**: 01、02。
- **后置**: 04、05、08、09、10、18、20。
- **事实底线**: `utility.c:34-51/:76-86/:108-118`、`glo.h`、`minix/endpoint.h`、`mproc/table.rs`、`mproc/pid_gen.rs`、`mproc/context.rs`。
- **知识点清单**: K-015、K-016、K-017。
- **验收标准**: 读者能复述 pm_isokendpt 三级各自防什么；PID 相位（先自增再返回、INIT_PID+2 起步、组冲突跳过）有 C 锚点与 Rust 测试对照。

### 04-pm-init-main（重排自旧 01，刷新）

- **一句话定位**: PM 从被内核唤醒到进入主循环的全部步骤与顺序依据。
- **讲什么**: K-004（八步逐一，顺序依据）；K-005（SEF 壳：**新增"SEF 框架接口"小节**——sef_startup/sef_receive_status/sef_setcb_signal_manager 的契约与 sef_signal.c 的 SIGKSIG 拦截点，Q11）；K-006（填充循环）；K-007（VFS_PM_INIT 握手）；三集合建构步骤（只记步骤，语义回指 11，Q10）；sched_init 调用点（语义回指 16）；get_nice_value 只列调用不展开（Q 越界修正）。
- **不讲什么**: 主循环体（05）；mproc 字段语义（02）；sched_init 内部（16）；VFS 侧握手对端（05-stage-vfs）。
- **前置**: 01、02、03。
- **后置**: 05（循环从启动结束处接管）、16、19（system_hz 消费者）。
- **事实底线**: `main.c:49-289`（除主循环段）；`lib/libsys/sef.c`、`sef_init.c`、`sef_signal.c`（SEF 接口面）；`schedule.c:20-50`（调用点视角）；`os/servers/pm/src/main.rs`、`init.rs`（`PmServer::init`、`fill_boot_procs`、D-02 BootParams 占位契约）；minix-sys `SYS_GETMONPARAMS/SYS_GETIMAGE` 缺口现状。
- **知识点清单**: K-004、K-005、K-006、K-007、K-064（建构步骤）、K-090（调用点）。
- **验收标准**: 八步每步有"为什么在这个位置"的顺序论证（如 VFS 屏障必须在填充后、sched_init 必须在 system_hz 后）；Rust init 链与 C 八步逐步对照表；SEF 小节使读者不看 libsys 源码也能理解信号管理回调何时被调。

### 05-ipc-dispatch（重排自旧 04，大幅增补）

- **一句话定位**: PM 运行时的心脏——一次调用从到达到回复（或不回复）的完整骨架，与全部横切规约。
- **讲什么**: K-003（六步循环逐一）；K-019（事件循环模型）；K-127（**新增：无锁模型声明**——单线程 + 延续标志替代锁，Q12）；K-024 + **K-126（notify 双源：CLOCK→expire_timers；SYSTEM→process_sigmgr_signals，语义回指 11）**；K-020/K-021（SUSPEND 模型与 ReplyIntent）；K-022（call_vec 与 PmCall 枚举 + K-125 wire 载荷现状）；K-023（EXITING 丢弃）；K-025（reply 预填）；K-015 调用点；K-122（**新增：decode 单点化**小节）；K-119（**新增：KernelGateway 双风格规约与四列对照表**，从 plan.md §4.1 升格）；K-120（**新增：错误保真规约**，从 plan.md §4.2 升格）；ENOSYS 兜底语义与当前 8 个挂起调用名单。
- **不讲什么**: VFS 十一路状态机（06）；事件回复臂细节（07）；各调用 handler 内部（08–20）；内核信号拉取循环内部（11）。
- **前置**: 01、02、03、04。
- **后置**: 06–20（所有 handler 篇的分发前提）。
- **事实底线**: `main.c:48-109/:249-289`；`table.c`；`callnr.h`；`com.h`（IS_VFS_PM_RS/PROC_EVENT_REPLY/SUSPEND/is_ipc_notify）；`ipcconst.h`（IPC_STATUS_CALL）；Rust 侧 `init.rs`（run_once/reply）、`ipc/dispatcher.rs`（ReplyIntent）、`ipc/calls.rs`（47 臂与兜底）、`ipc/decode.rs`、`ipc/transport.rs`、`exit.rs`（KernelGateway）。
- **知识点清单**: K-003、K-015、K-019、K-020、K-021、K-022、K-023、K-024、K-025、K-119、K-120、K-122、K-125、K-126、K-127。
- **验收标准**: 循环六步与 C 逐行对照；SUSPEND 三子类各自的回复路径表；四列对照表（内核能力↔trait↔minix-sys↔C libsys）完整；当前 ENOSYS 名单与当天代码一致。

### 06-vfs-interaction（重排自旧 05，刷新）

- **一句话定位**: PM↔VFS 异步协议全貌——为什么异步、怎么发、十一路回复怎么收口。
- **讲什么**: K-026（死锁论证）；K-027（tell_vfs 三段式，主讲述点）；K-028（23 条消息面）；K-029（十一路状态机，主讲述点，含 CORE→EXIT 落穿）；K-030（延续风格）；K-036 调用点视角（publish_event 两处提前 return）；restart_sigs 尾部调用点（语义归 13）；Rust 侧 `ipc/vfs.rs` 的 `VfsForwarder`/回复状态机与 Fix #48 后的 restart_signals 真实委托。
- **不讲什么**: 各请求的业务语义（08/09/15/17 各自讲）；事件订阅表（07）；主循环骨架（05）。
- **前置**: 05、02、03。
- **后置**: 07、08、09、10、15、17、20（reboot 回收）。
- **事实底线**: `main.c:295-424`、`utility.c:123-139`、`com.h:513-542`、`ipc.h`（VFS_PM_* 字段族）；Rust `ipc/vfs.rs`。
- **知识点清单**: K-026、K-027、K-028、K-029、K-030。
- **验收标准**: 十一路每路一行的收口表（回复载荷/对 mproc 的副作用/是否续行）；双向死锁论证有 C 锚点；VFS_CALL 生命周期图（置位→清除→NEW_PARENT/UNPAUSED 交互）。

### 07-event-subscription（重排自旧 06，刷新）

- **一句话定位**: 进程事件发布/订阅设施——有界队列约束下的串行化通知。
- **讲什么**: K-031~K-037（全部）；PROC_EVENT_EXIT/SIGNAL 两事件类型；`waiting` 计数与上界断言；Rust EventRegistry（`event.rs`，含 Fix #54 非 mut 变体删除后的现状）。
- **不讲什么**: exit_restart/restart_sigs 的被调方语义（09/13）；内核 asynsend 队列实现（01-stage-kernel）；VFS 回复臂（06）。
- **前置**: 05、06、02、03。
- **后置**: 09、13（终止分派的被调方）。
- **事实底线**: `event.c` 全文（353 行）；`syslib.h:292-293`（事件类型）；`com.h:619`（PROC_EVENT_REPLY）；Rust `event.rs`、`ipc/vfs.rs` publish 调用点、`ipc/dispatcher.rs` 事件回复臂。
- **知识点清单**: K-031、K-032、K-033、K-034、K-035、K-036、K-037、K-114。
- **验收标准**: 串行化有界性论证（NR_PROCS vs NR_PROCS×NR_SUBS）完整复述；订阅表游标推进图覆盖 remove_sub 回退；resume_event 终止分派两分支的归属箭头明确。

### 08-fork-family（合并旧 07+08）

- **一句话定位**: 进程如何被创建——do_fork 全链路精讲，do_srv_fork 以差异表收束。
- **讲什么**: K-038（四表协同全景）；K-039（do_fork 十二原子步，主讲述点——以 §1.2.3 的 12 步为准重新编号，替代旧文档"9 步"口径）；K-040/K-041/K-042/K-043；K-044（srv_fork 五差异表）；K-045（两族调度归属）；tracer SIGSTOP 钩子（声明并回指 11）；回复语义对照（fork=SUSPEND 后双回复 vs srv_fork=立即双回复）。
- **不讲什么**: VM/VFS 对端（02-stage-vm、05-stage-vfs）；sched_start_user 内部（16）；sig_proc 内部（11）；PID 算法（03）。
- **前置**: 01、02、03、05、06。
- **后置**: 09（procs_in_use 配对）、10（TOLD_PARENT 消费端）、18（TO_TRACEFORK）。
- **事实底线**: `forkexit.c:44-240`；`utility.c:34-51`（使用点）；`com.h`（VFS_PM_FORK/SRV_FORK）；Rust `fork.rs`、`mproc/fork.rs`（inherit_guardianship）、`mproc/pid_gen.rs`（使用点）。
- **知识点清单**: K-038、K-039、K-040、K-041、K-042、K-043、K-044、K-045、K-114（不变量消费）。
- **验收标准**: 12 步每步带锚点与"失败时的行为"列；五差异表逐行给 C 锚点；不可回滚窗口前后界有图；"为什么 srv_fork 要原子孵化"的论证保留。

### 09-pm-exit（刷新）

- **一句话定位**: 进程如何死——两阶段解绑、两级僵尸、收养与广播。
- **讲什么**: K-046/K-047/K-048/K-049/K-050/K-051/K-052/K-053；ALARM_ON 撤钟调用点（回指 14）；publish_event 调用点（回指 06/07）；SIGCHLD/SIGHUP 投递钩子（回指 11）；dump_core 双重豁免；Rust `exit.rs`（KernelGateway 所在地）与 exit 链现状（Fix #11/#12/#24/#26 等已落）。
- **不讲什么**: wait4 消费（10）；VFS 侧 fproc 释放（05-stage-vfs）；vm_* 对端（02-stage-vm）；ptrace 细节（18）。
- **前置**: 01、02、03、06、07、08。
- **后置**: 10、11（SIGCHLD/SIGHUP 语义）、13、18。
- **事实底线**: `forkexit.c:242-469`；`main.c:356-367`（CORE/EXIT_REPLY 收口半）；Rust `exit.rs`、`mproc/lifecycle.rs`。
- **知识点清单**: K-046、K-047、K-048、K-049、K-050、K-051、K-052、K-053。
- **验收标准**: 两阶段分界图（VFS_PM_EXIT 为界）带 9+5 步锚点；僵尸状态转移图覆盖 TRACE_ZOMBIE↔ZOMBIE↔TOLD_PARENT 全部迁移；收养与 NEW_PARENT 的防错配论证完整。

### 10-pm-wait（刷新）

- **一句话定位**: 僵尸的消费端——wait4 的三环扫描与父/tracer 双回收路径。
- **讲什么**: K-054~K-059；W_STOPCODE 的 wait 载荷视角（trace 侧归 18）；rusage 两字段现状与 TODO 声明；Rust `wait.rs`/`mproc/wait.rs`（Fix #6/#22/#27 已落）。
- **不讲什么**: 僵尸生产（09）；SIGCHLD 语义（11）；trace_stop 的 W_STOPCODE 产生端（18）。
- **前置**: 01、02、03、06、07、09。
- **后置**: 18（tracer 伪父契约被引用）。
- **事实底线**: `forkexit.c:471-806`；`utility.c:92-93`（set_rusage_times 使用）、`:144-156`；`sys/wait.h`（W_EXITCODE/W_STOPCODE/WCOREFLAG）；Rust `wait.rs`、`mproc/wait.rs`。
- **知识点清单**: K-054、K-055、K-056、K-057、K-058、K-059。
- **验收标准**: pidarg 四态真值表；三环扫描流程图带"为何此序"论证；tell_parent 失败路径（datacopy 失败留僵尸）单列。

### 11-signal-core（刷新 + 内核回环增补）

- **一句话定位**: 信号从三个生成源到终止/投递的核心闭环。
- **讲什么**: K-060（位图消费视角）、K-061~K-068；K-064（三集合投递语义，主讲述点，Q10）；**新增"内核信号回环"一节（K-121）：sef_signal.c 拦截→SYSTEM notify→process_sigmgr_signals 拉取循环→process_ksig**（含 EDEADEPT 双检、SIGSNDELAY=70 尾部、SigSet(u64) 位宽适配的 ARCH 注记）；badignore；do_kill/do_srv_kill。
- **不讲什么**: 安装与 sig_send（12）；停止/恢复机制内部（13）；ptrace 命令（18）；内核 do_kill 对端（01-stage-kernel）。
- **前置**: 01、02、03、05、09。
- **后置**: 12、13、14、17、18。
- **事实底线**: `signal.c:194-378/:384-646`；`main.c:122`；`lib/libsys/sef_signal.c`；`sys/sys/signal.h`（SIGS_* 宏、SIGSNDELAY）；Rust `signal.rs`（含 Fix #43/#45/#50 后状态）、`mproc/signal.rs`。
- **知识点清单**: K-060、K-061、K-062、K-063、K-064、K-065、K-066、K-067、K-068、K-121。
- **验收标准**: 九判定链流程图每个分支带 C 行号与"谁置位/谁消费"；三生成源（kill(2)/内核回环/RS srv_kill）入口表；权限五重与 EPERM/ESRCH/EINVAL 返回值矩阵。

### 12-signal-handlers（刷新）

- **一句话定位**: 信号的安装与投递——sigaction 族如何写位图，sig_send 如何翻译成内核栈帧。
- **讲什么**: K-069~K-072；K-060 主讲述点（四位图的生产者视角）；`mp_sigmask2` 保存掩码；四处前置断言的意义；Rust `signal_handlers.rs`（批次 B 后 sigaction 族已接线现状）。
- **不讲什么**: 生成与广播（11）；停止/恢复（13）；内核 sigframe 推送（01-stage-kernel/19-syscall-signal）。
- **前置**: 05、11、02。
- **后置**: 13、14、17。
- **事实底线**: `signal.c:40-192/:775-855`；Rust `signal_handlers.rs`、`mproc/signal.rs`（SigAction 堆分配）、minix-sys SigMsgWire。
- **知识点清单**: K-060、K-069、K-070、K-071、K-072。
- **验收标准**: 三态安装的位图变化表；sigmsg 六字段组装图与 SA_* 标志效果；EFAULT/ENOMEM 合法失败分档与终止兜底的论证。

### 13-signal-flow（刷新）

- **一句话定位**: 信号在阻塞夹缝中不丢失的机制——延迟、停止、挂起与恢复状态机。
- **讲什么**: K-073~K-078；unpause 三路径与 VFS_PM_UNPAUSE；restart_sigs 在 VFS 尾部/事件收口两处调用点；PROC_STOPPED 双用途；Rust `signal_flow.rs`（Fix #43/#48/#49 后的 GatewayStopBridge/restart_signals 现状）与 sys_delay_stop 的 pre-E6 占位契约。
- **不讲什么**: sig_proc 判定链本体（11）；sig_send（12）；VFS 状态机（06）。
- **前置**: 06、07、11、12、02。
- **后置**: 14、18（TRACE 分支被引用）。
- **事实底线**: `signal.c:226-289/:651-770`、`main.c:421-423`；Rust `signal_flow.rs`、`ipc/vfs.rs` restart_signals。
- **知识点清单**: K-073、K-074、K-075、K-076、K-077、K-078。
- **验收标准**: "延迟→停止→挂起→重检→恢复"状态机图覆盖全部迁移边；may_delay 契约两用法（FALSE 必停/TRUE 可延迟）的场景表；VFS 回复尾部 restart_sigs 的触发条件。

### 14-itimer（刷新）

- **一句话定位**: 三族定时器与 ticks 闭环——alarm/setitimer 的完整链路。
- **讲什么**: K-079~K-082；CLOCK notify 驱动（回指 05 的 notify 臂，含 Fix #57 载荷时间戳语义）；cause_sigalrm→check_sig 衔接（回指 11）；批次 D 后 TimerCtl/SysVTimerCtl 生产实现现状（git ee1145c0c）。
- **不讲什么**: 内核定时器队列（01-stage-kernel/15）；check_sig 内部（11）。
- **前置**: 05、11、02。
- **后置**: 无（叶子篇）。
- **事实底线**: `alarm.c` 全文；`minix/timers.h`；Rust `timer.rs`、init.rs notify 臂。
- **知识点清单**: K-079、K-080、K-081、K-082。
- **验收标准**: REAL 与 VIRTUAL/PROF 双后端对照表；ticks 换算的上取整与溢出边界用例；到期→投递→重设闭环图。

### 15-credentials（刷新）

- **一句话定位**: 13 个身份调用的判据矩阵与 VFS 双副本协同。
- **讲什么**: K-083~K-086；TAINTED 只写 issetugid 消费（置位归 17，Q 修正）；批次 A 后 13 调用全部接线现状（git e56618e20）；GETGROUPS/SETEPINFO 的 wire 面挂起项（D-30 余量）。
- **不讲什么**: exec 的 setuid 位（17）；调度 nice 检查（16）；VFS 状态机（06）。
- **前置**: 02、05、06。
- **后置**: 17（TAINTED 延伸）、20（getepinfo 复用凭证）。
- **事实底线**: `getset.c` 全文；`misc.c:169-193`（getepinfo）；Rust `credentials.rs`、`mproc/credentials.rs`。
- **知识点清单**: K-083、K-084、K-085、K-086、K-087（消费面）。
- **验收标准**: 13 调用判据矩阵表（每个调用的权限判据+修改哪些字段+是否走 VFS）；NetBSD/BSD 语义注释逐条保留带 C 锚点。

### 16-scheduling（刷新）

- **一句话定位**: 用户态调度交接与 nice↔queue 双射。
- **讲什么**: K-088~K-091；do_getsetpriority（越界修正后此处为主讲述点）；get_nice_value/nice_to_priority 双函数辨析（Q 修正）；sched_start_user 的继承语义与 Fix #51 修正后的继承实现；SCHED 服务器缺失的挂起契约（D-12/D-17、A-8）。
- **不讲什么**: 内核调度器（01-stage-kernel/11）；SCHED 服务器实现（06-stage-sched）。
- **前置**: 04、05、02。
- **后置**: 08（sched_start_user 调用点回指）、09（sched_stop 调用点回指）。
- **事实底线**: `schedule.c` 全文、`utility.c:91-103`、`main.c:275-289`、`misc.c:239-286`；`minix/sched.h`；Rust `sched.rs`。
- **知识点清单**: K-088、K-089、K-090、K-091。
- **验收标准**: nice↔queue 双向公式与量化误差示例；sched_init 的过滤条件与两个 assert 的时序依赖；get/setpriority 的权限与 EACCES 矩阵。

### 17-exec（刷新）

- **一句话定位**: 执行替换的三段式与半初始化自毁。
- **讲什么**: K-092~K-096；TAINTED 主讲述点（唯一置位点）；批次 E 后 14/43/44 三臂接线现状（git 06936df2d）；caller 门（VFS/RS）。
- **不讲什么**: VFS 加载与 `#!`（05-stage-vfs）；sys_exec 内核侧（01-stage-kernel）；sigaction 安装（12）。
- **前置**: 05、06、11、12、15、02。
- **后置**: 15（TAINTED 消费回指）、18（TO_NOEXEC 消费回指）。
- **事实底线**: `exec.c` 全文；`minix/vm.h`（exec_info）；Rust `exec.rs`、`ipc/vfs.rs` EXEC 臂。
- **知识点清单**: K-087（主）、K-092、K-093、K-094、K-095、K-096。
- **验收标准**: 三段式时序图（VFS 权限→do_newexec→handle_vfs_reply→exec_restart→sys_exec）；PARTIAL_EXEC 两种失败结局（SIGKILL 自毁 vs reply 错误）；TAINTED 二重判定真值表。

### 18-trace（重写）

- **一句话定位**: ptrace 的 16 命令在 PM 侧的全貌与 Traced 状态机。
- **讲什么**: K-097~K-101、K-051/K-058 引用面；T_* 常量以 `sys/sys/ptrace.h` 为准全量列出（旧文档 TO_NOEXEC 位值错误：C 为 0x4 `ptrace.h:211`，旧 18 写 0x1——**B 相必须以头文件为准重建常量表**）；trace_stop；Rust trace 域现状按 V3-P1-1 重构后的实际代码写（常量对账测试、分支矩阵），不再沿用失真叙述；T_READB/WRITEB_INS 的 root 门位置。
- **不讲什么**: 内核 sys_trace 实现；wait4 消费端（10）。
- **前置**: 03、10、11、02。
- **后置**: 09（tracer_died 回指）。
- **事实底线**: `trace.c` 全文；`sys/sys/ptrace.h`（T_* 与 TO_* 常量真值）；Rust `trace.rs`、`mproc/trace.rs`、`mproc/guardianship.rs`。
- **知识点清单**: K-097、K-098、K-099、K-100、K-101。
- **验收标准**: 16 命令×（权限门/前置状态/PM 动作/内核透传/回复载荷）矩阵；常量表逐个与 ptrace.h 对照；T_ATTACH 权限链逐步展开。

### 19-time（刷新）

- **一句话定位**: 双时钟与五个时间调用。
- **讲什么**: K-102/K-103；system_hz 来源（回指 04）；批次 C 后生产接线现状（git 6b948e2ae：sys_settime/stime/GETUPTIME ClockSource）；GetRUsage 随 VM 对端同批的挂起说明（与 20 交叉引用）。
- **不讲什么**: 内核 do_settime/adjtime_delta（01-stage-kernel/21）；VFS 时间戳；itimer（14）。
- **前置**: 04、05、02。
- **后置**: 无。
- **事实底线**: `time.c` 全文；`lib/libsys/clock_time.c`；`sys/time.h`（CLOCK_* 值）；Rust `time.rs`。
- **知识点清单**: K-102、K-103。
- **验收标准**: REALTIME/MONOTONIC 对照表（可设性/来源/跳变行为）；无溢出分解公式推导；五调用×（权限/时钟约束/内核调用）矩阵。

### 20-misc-queries（刷新）

- **一句话定位**: 信息查询与系统控制面——10+ 调用的权限边界与数据通路。
- **讲什么**: K-104~K-111、K-107（reboot 定序主讲述点）；按"信息边界"分组：身份解析（getprocnr/getepinfo）、系统信息（getsysinfo **含 K-123 wire 镜像真实拷出现状**、sysuname、svrctl）、控制（reboot）、记账（getrusage 挂起契约）、透传（sprofile/mcontext）；WONTFIX 集中节（ENABLE_SYSCALL_STATS/D-32、SPROFILE ENOSYS、uts 兼容块、ESCRIPT 引 plan 排除表）。
- **不讲什么**: do_getsetpriority（16）；MIB 服务的 mpsigact 消费（10-stage-mib）；VM rusage 字段（02-stage-vm）。
- **前置**: 01、03、04、05、06。
- **后置**: 无。
- **事实底线**: `misc.c` 全文、`profile.c`、`mcontext.c`、`utility.c:56-71`；Rust `misc.rs`（Fix #46/#52/#59 后状态 + D-29 激活）、`mproc/wire.rs`。
- **知识点清单**: K-104、K-105、K-106、K-107、K-108、K-109、K-110、K-111、K-123（消费面）、K-128（挂起原因）。
- **验收标准**: 每调用一行的"权限门/数据源/回复载荷"表；reboot 定序图（含为何先 kill 后 VFS）；当前 ENOSYS 调用在本篇的挂起契约完整。

---

## 6. 变更表

| 操作 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向说明 |
|------|--------|--------|------|-----------|---------|
| 重排 | 99-global-concepts.md | **01** | 修复"推荐路线（99 最先）与编号（99 最后）矛盾"；词汇前置满足"首次出现即完整" | K-009/013/014/064/112/113/118 | 全部小节原样迁移+重写锚点校准 |
| 重排 | 01-pm-init-main.md | **04** | 00 声明的收敛路线把 02/03 放在启动链前；数据模型先讲则启动链零前向引用 | K-004~K-007 等 | 全部小节迁移；新增 SEF 壳小节；get_nice_value 移交 16 |
| 重排 | 02-mproc-struct.md | **02（不变）** | 内容刷新（wire 镜像、锚点重校准） | K-008~K-012/118 + K-123 | 编号不变 |
| 重排 | 03-mproc-table.md | **03（不变）** | 内容刷新 | K-015~K-017 | 编号不变 |
| 重排 | 04-ipc-dispatch.md | **05** | 骨架连续性（启动→循环→VFS→事件） | K-003/015/019~K-025 + 新增 K-119/120/122/126/127 | 十一路展开移交 06；新增三规约小节与 notify SYSTEM 臂 |
| 重排 | 05-vfs-interaction.md | **06** | 同上 | K-026~K-030 | 全部迁移+实施现状刷新 |
| 重排 | 06-event-subscription.md | **07** | 同上 | K-031~K-037/114 | 全部迁移+实施现状刷新 |
| 合并 | 07-pm-fork.md + 08-pm-srv-fork.md | **08（fork-family）** | 并行体规则：同族调用代表精讲+差异表收束；两篇合计 741 行合并后仍在舒适区 | K-038~K-045 | do_fork 全链路为正文主干；do_srv_fork 收束为五差异表；两篇的"不讲什么"声明合并 |
| 保留 | 09～17、19、20 | **编号不变** | C 文件边界与语义单元已对齐；只需内容刷新 | 各篇 §2 已列 | 编号不变，正文按契约刷新 |
| 重写 | 18-trace.md | **18（编号不变，正文重写）** | 旧文常量位值失真（TO_NOEXEC 0x1 vs C 0x4）+ Rust trace 域经 V3-P1-1 重构后旧叙述失真 | K-097~K-101 | 以 ptrace.h 与重构后代码为准重建 |
| 归档（B 相执行） | 07/08 两旧文件 | 归档不删 | 合并后旧编号退出正式目录 | — | 旧文件移入归档区或加"已合并至 08"头注（B 相裁决，本蓝图只登记） |
| 不动 | plan.md / todo.md / draft/ | 参考材料 | plan 是历史裁决记录，todo 是活动台账，draft 是素材 | — | 正式文档引用它们时注明性质 |

**序差表**（运行时序 vs 教学序，全部带回指补偿）：

| # | 运行时事实（锚点） | 教学序选择 | 回指补偿位置 |
|---|------------------|-----------|-------------|
| 1 | 三集合建构在 init 第 2 步（main.c:157-165） | 语义主讲述在 11，建构步骤在 04 | 04 建构步骤→"语义见 11"；01 词汇表列成员常量 |
| 2 | sched_init 在 init 第 8 步执行（main.c:241） | 语义在 16 | 04 记调用点并声明后置 16 |
| 3 | exit_proc→check_parent→sig_proc(SIGCHLD)（forkexit.c:663） | 09 先于 11（因果序本身一致；机制细节后讲） | 09 写"投递动作见 11"的钩子声明 |
| 4 | do_fork 尾部 sig_proc(SIGSTOP)（forkexit.c:133-134） | 08 先于 11 | 08 钩子声明+11 后置声明 |
| 5 | exit_proc 撤钟 set_alarm(rmp,0)（forkexit.c:301） | 09 先于 14 | 09 记调用点，14 讲语义 |
| 6 | sig_proc 的 TRACE 分支调 trace_stop（signal.c:420） | 11 先于 18 | 11 钩子声明，18 兑现 |
| 7 | handle_vfs_reply 调 exec_restart/sched_start_user（main.c:350/:373） | 06 先于 16/17 | 06 收口表标注"语义见 16/17" |
| 8 | publish_event 终止分派调 exit_restart/restart_sigs（event.c:119-122） | 07 先于 09/13 | 07 分派箭头声明后置 |
| 9 | TAINTED 唯一置位点在 exec（exec.c:100-109），issetugid 在 15 消费 | 15 先于 17 | 15 消费小节声明"置位语义见 17" |
| 10 | do_getsetpriority 定义于 misc.c（misc.c:239） | 归 16 讲述（plan D-7 裁决） | 20 只留转发说明 |

---

## 7. 缺漏新篇

> 逐项裁决（无"待定"）。结论：**不需要新建任何篇章**；全部缺口并入既有篇章的新增小节。理由：每个缺口主题都寄生于某个既有机制的骨架（单独成篇会变成只有引用没有正文的空壳），且新目录已把骨架篇（05）定位为横切规约的承载点。

| 缺口主题（§3.2 编号） | 裁决 | 落实位置 | 验收标准 |
|----------------------|------|---------|---------|
| Q1 内核信号回环入口 | 并入 | 05 notify 双源一段 + 11"内核信号回环"一节 | 拉取循环流程图 + EDEADEPT/SIGSNDELAY 尾部语义完整 |
| Q2 decode 单点化 | 并入 | 05 新小节 | decode 模块职责与 unsafe 收敛论证 |
| Q3 wire C-ABI 镜像 | 并入 | 02 镜像小节 + 20 getsysinfo 现状 | 464 字节逐段对照 |
| Q4 接线现状失真 | 全篇刷新 | 21 篇实施现状层 | 数字与当天代码一致并注明快照日期 |
| Q5 错误保真规约 | 升格 | 05 横切规约一节 | 规约正文 + 违例示例（map_err 折叠反例） |
| Q6 端口双风格规约 | 升格 | 05 横切规约一节 | 四列对照表完整 |
| Q7 exec 接线现状 | 刷新 | 17 | 三臂接线与 caller 门描述与代码一致 |
| Q8 时间族接线 | 刷新 | 19 | 五调用生产实现描述与代码一致 |
| Q9 余下 8 调用挂起契约 | 集中 | 20（杂项 5 个）+ 16（2 个）+ 15（0，已全接）+ 19 交叉引用 | 每个挂起调用有"原因+解除条件"两栏 |
| Q10 三集合归属 | 裁决 | 11 主讲 + 04 步骤 + 01 常量表 | 三处互指无重复展开 |
| Q11 SEF 壳 | 新增小节 | 04 | 读者不读 libsys 也能理解信号管理回调链 |
| Q12 无锁模型声明 | 新增小节 | 05 | 与 SMP/BKL 约束（AGENTS.md 执行模型）对照声明 |

---

## 8. 锚点迁移与断链成本

### 8.1 编号变更总表

旧→新：00→00，99→**01**，02→02，03→03，01→**04**，04→**05**，05→**06**，06→**07**，07+08→**08**，09→09，10→10，11→11，12→12，13→13，14→14，15→15，16→16，17→17，18→18，19→19，20→20。

**受影响编号只有 7 个：01、04、05、06、07、08、99**（其中 07/08 合并）。其余 14 篇编号不变，其被引用关系不受影响。

### 8.2 锚点迁移表（变更文档逐节）

| 旧位置 | 旧内容（一句话） | 新位置 | 迁移类型 | 断链风险 |
|--------|----------------|--------|---------|---------|
| 99§1.1-1.2 | 常量语义半径 / 全局状态显式化 | 01§1 | 原样搬移+锚点重校准 | 低 |
| 99§2.x | 常量分表（身份/容量/信号/调用号索引） | 01§2 | 原样搬移；47 调用索引保留速查表形态 | 低 |
| 99 其余 | endpoint 编码/三集合成员 | 01§2/§3 | 原样搬移；三集合**语义**移交 11（Q10） | 中：11 需接收语义段 |
| 01§1.x | 启动链概念与八步 | 04§1-2 | 原样搬移+刷新 | 低 |
| 01（正文散点） | get_nice_value 展开 | 16 | 改写（移交） | 中：01 旧读者线索断，新 04 留转发 |
| 01（正文散点） | 三集合建构叙述 | 04 记步骤+11 讲语义 | 拆分 | 中 |
| 01§（C 锚点整体） | main.c 行号（V3-P3-6 登记 -3~4 行漂移） | 04 全篇 | 改写（逐行重校准） | 低（本来就要修） |
| 04§1 | 事件循环与六步 | 05§1-2 | 原样搬移+SYSTEM notify 臂增补 | 低 |
| 04§2-3 | call_vec/调用统计/ENOSYS | 05§3 | 原样搬移+接线现状刷新 | 低 |
| 04§（SUSPEND 三子类） | SUSPEND 模型 | 05（主） | 原样搬移 | 低 |
| 05 全篇 | VFS 协议 | 06 全篇 | 原样搬移+刷新 | 低（整篇平移） |
| 06 全篇 | 事件订阅 | 07 全篇 | 原样搬移+刷新 | 低 |
| 07§1-2 | do_fork 概念与全链路 | 08§1-3 | 原样搬移+步骤重编号（9 步→12 步口径） | 中：步骤编号变化，正文内引用需同步 |
| 07§（VFS 协调段） | tell_vfs 使用视角 | 08（引用 06） | 改写（去重，主讲述在 06） | 低 |
| 08 全篇 | do_srv_fork | 08§4（差异表+独立小节） | 改写（合并收束） | 中：原 08 的独立阅读者需经新 08 §4 |
| 09~20 各篇 | 语义正文 | 编号不变 | 内容刷新（实施现状+锚点校准+缺口小节） | 低 |

### 8.3 引用迁移表

**文档内互引**（实测统计：受影响引用行合计约 **53 处**，分布如下；修复方式 = 按上表编号映射做文本替换 + 上下文复核）：

| 被引旧文档 | 被引次数（引用行） | 引用方（热点在前） | 新目标 |
|-----------|------------------|-------------------|--------|
| 04-ipc-dispatch.md | 15 行 | 06/07/10/11/12/13/14/16/20 | 05-ipc-dispatch.md |
| 01-pm-init-main.md | 9 行 | 04→（新）04 引用方为 09/14/16/19/20 等 | 04-pm-init-main.md |
| 05-vfs-interaction.md | 11 行 | 07/08/09/10/13/15/17 | 06-vfs-interaction.md |
| 06-event-subscription.md | 6 行 | 09/10/13 | 07-event-subscription.md |
| 07-pm-fork.md | 6 行 | 08/09/10/17 | 08-fork-family.md |
| 08-pm-srv-fork.md | 3 行 | 07/09 | 08-fork-family.md §4 |
| 99-global-concepts.md | 3 行 | 02/03/11 | 01-global-concepts.md |
| 00-pm-overview.md | 2 行 | 01/99 | 00（不变） |

**代码注释引用**（`os/servers/pm/src/` 内引用旧编号的注释，实测 30+ 处中受影响约 **10 处**）：

| 代码位置 | 旧引用 | 新引用 | 验证方式 |
|---------|--------|--------|---------|
| main.rs:4 | 01-pm-…、04-ipc | 04-…、05-… | `rg "01-pm-init|04-ipc" os/servers/pm/src/main.rs` |
| ipc/dispatcher.rs:3/:9/:11/:101 | 04-ipc、05-vfs-inter、06-event-sub | 05、06、07 | 同型 rg |
| ipc/vfs.rs:11/:432-433/:478 | 04-ipc、16-schedul、06-event-sub | 05、16、07 | rg |
| mproc/mproc.rs:143-146/:252/:291 | 14/15/17（不变）、06-event-sub→07、02（不变） | 07 | rg |
| mproc/table.rs:5/:27/:170、mproc/pid_gen.rs:47/:281、mproc/context.rs:126 | 03-mproc（不变）、04-stage-pm 目录名（不变） | 无需改 | rg 复核 |
| mproc/trace.rs:12 | 09-pm-（不变） | 无需改 | rg 复核 |

**跨 stage 引用**（31 处，受编号变更影响的部分）：

| 引用方 | 引用内容 | 处置 |
|--------|---------|------|
| 05-stage-vfs（14 处） | 多数引用 07/09/10/15/17/20（编号不变）；如引用 04/05/06 旧名则随映射改 | B 相批量 rg 后逐条改 |
| 06-stage-sched（10 处） | 引用 01/16 居多；01→04 需改 | 同上 |
| 02-stage-vm（2 处） | 引用 07（→08） | 同上 |
| edge_todo.md（1 处） | 待 rg 定位 | 同上 |

### 8.4 断链成本摘要

- **受影响引用总数**: 文档内互引约 53 行 + 代码注释约 10 处 + 跨 stage 约 10~15 处（31 处中编号不变者免修）≈ **75±10 处**。
- **热点文件**: 文档侧 06/07/09/10/13/15/17/20（引用旧 04/05/01 密集）；代码侧 `ipc/dispatcher.rs`、`ipc/vfs.rs`、`mproc/mproc.rs`。
- **建议批量修改方式**: (1) 先落新文件（B 相重建），旧文件整体归档；(2) 用 `rg -l '0[1456]-[a-z-]+\.md|99-global-concepts' notes/rewrite/fork-syscall-rewrite/ os/servers/pm/src` 生成受影响清单；(3) 按上表映射脚本化替换后逐处人工复核上下文（替换词可能是正文叙述而非引用）；(4) 跨 stage 引用单独一轮处理并在两 stage 的 todo.md 记账。
- **成本判断**: 约 75 处机械替换 + 合并篇的步骤重编号，属于一次性可控成本；不重建的替代成本是长期维持"路线与编号矛盾 + 实施现状失真"双缺陷。

---

## 9. 验证与自检门

### 9.1 四种机械检查结果

1. **前向引用扫描（G3）**: 按新目录逐篇检查 §5 契约"前置"字段——01←00；02←01；03←01,02；04←01,02,03；05←01-04；06←05,02,03；07←05,06,02,03；08←01,02,03,05,06；09←01-08；10←01,02,03,06,07,09；11←01,02,03,05,09；12←05,11,02；13←06,07,11,12,02；14←05,11,02；15←02,05,06；16←04,05,02；17←05,06,11,12,15,02；18←03,10,11,02；19←04,05,02；20←01,03,04,05,06。**全部指向更早编号，通过**。
2. **依赖关系图无环（G4）**: 上述前置关系构成 DAG（编号严格递增）。**通过**。
3. **覆盖率检查（G5）**: 知识点池 128 条（存量 120 + 新增 8）全部在第 5 节契约的"知识点清单"中出现至少一次（K-115/K-116/K-117/K-124/K-125 等聚合条归 00/05）；无删除项（无知识点被判丢弃）。**通过**。
4. **断链成本统计（G8 前置）**: 见 §8.4，总量与热点已列。**完成**。

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|----|------|------|
| G1 C 真序逐条可核对（抽 10 条） | **通过** | 抽样：①main.c:122 信号管理回调注册 ✓；②main.c:238 system_hz 在屏障后 ✓（我精读确认顺序：231-236 屏障、238 hz、241 sched_init）；③forkexit.c:60-65 LAST_FEW=2（:32 定义）✓；④forkexit.c:106 fork flags 过滤三元 IN_USE\|DELAY_CALL\|TAINTED ✓；⑤signal.c:585 INIT+SIGKILL→EINVAL ✓；⑥signal.c:344 SIGSNDELAY∧DELAY_CALL 分支 ✓；⑦alarm.c:343 cause_sigalrm 调 check_sig(pid,SIGALRM,FALSE) ✓；⑧utility.c:43 next_pid 先自增再回绕 ✓；⑨main.c:421-423 尾部 restart_sigs 条件 ✓；⑩misc.c:223-224 reboot 先 check_sig 后 sys_stop(INIT) ✓。全部在本次精读中逐一目验 |
| G2 知识点池完整：每个 C 文件/非 C 制品有归属或明确排除 | **通过** | 15 个 .c 全部进入 §1.2.3 表并映射到契约篇；6 个本地 .h 归 01/02；非 C 制品经 §3.5 逐项回答（10/10） |
| G3 前向引用为零 | **通过** | 见 9.1 第 1 条 |
| G4 依赖图无环 | **通过** | 见 9.1 第 2 条 |
| G5 覆盖率 100% | **通过** | 128 条全有去向；新增 8 条全部带代码/C 锚点；明确删除项：无（ESCRIPT/统计宏等以 WONTFIX 小节保留在 20，属"讲述其不存在"而非删除知识） |
| G6 拆合的存量去向/新建的来源（抽查 10 处） | **通过** | ①99→01（§8.2 行 1）；②01→04（行 2-4）；③01 的 get_nice_value 展开→16（移交行有去向）；④07+08→08（行 11-12，K-038~045 全列去向）；⑤05 新增三规约来源=plan §4.1/§4.2+代码（K-119/120/122 有锚）；⑥11 新增回环节来源=K-121（main.c:122+sef_signal.c+Fix #45）；⑦02 新增 wire 节来源=K-123（wire.rs+提交号）；⑧08 步骤重编号来源=§1.2.3 的 12 步锚点；⑨18 重写来源=ptrace.h 真值+V3-P1-1；⑩20 挂起契约来源=K-124/K-128（calls.rs 实测+edge 清单） |
| G7 契约七要素齐全（21 篇） | **通过** | §5 每篇均含定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收标准，逐一核对无缺 |
| G8 迁移表覆盖与引用迁移 | **通过** | §8.2 覆盖全部 7 个变更文档的节级去向；§8.3 覆盖文档互引/代码注释/跨 stage 三类 |
| G9 事实断言有锚点（抽 10 条）+ 推测标注 | **通过** | 抽样：VFS_PM_RQ_BASE=0x900（com.h:513）✓、VFS_PM_RS_BASE=0x980（:514）✓、PROC_EVENT_REPLY=COMMON_RS_BASE+0（:619）✓、_NR_PROCS=256（sys_config.h:8）✓、SIGSNDELAY=70（sys/sys/signal.h:264）✓、PROC_EVENT_EXIT=0x01（syslib.h:292）✓、LAST_FEW=2（forkexit.c:32）✓、NR_SUBS=4（event.c:58）✓、T_* 以 ptrace.h 为准的指示（18 契约，旧文档位值错误由 todo V3-P1-1 表佐证）✓、39/47 接线（calls.rs 逐臂 grep + 8 个 PmCall 零命中）✓。**推测项标注**：跨 stage 引用中"10~15 处受影响"为基于文件级 grep 的估计（未逐条打开核对），已在 §8.4 以"约"标注；其余断言均有直接锚点 |

### 9.3 结论与待用户裁决的问题

**结论：蓝图完成。** 新目录 21 篇（7 个编号变更 + 1 处合并 + 14 篇编号不变的内容刷新），知识点池 128 条全有去向，依赖无环，断链成本约 75±10 处已算清。B 相可按 §5 契约逐篇取料开工，建议顺序：01（词汇）→ 02/03 → 04 → 05（骨架四篇是全部机制篇的前置）→ 06/07 → 08（合并篇，B 相首个真正的"重写"）→ 09-13 → 14-20 → 00（总览最后写，锁定快照）。

**待用户裁决**：

1. **旧文件归档形态**（B 相执行）：合并后的 07/08 与全部重排旧篇是移入 `draft/` 同级归档目录，还是原地加"已由 NN 取代"头注？本蓝图未擅自定（禁止改文件），B 相需一个明确指令。
2. **跨 stage 引用修复的执行批次**：31 处跨 stage 引用涉及 05/06-stage 与 02-stage 的文档，随 B 相同轮修，还是独立批次？涉及他 stage 的对账，建议独立批次并记账。
3. **18-trace 的重写时点**：若 V3-P1-1 的 trace 域重构尚未完成（以 B 相开工日 `git log -- os/servers/pm/src/trace.rs` 为准），18 的"Rust 现状"节是按重构后代码写（等代码）还是按 C 语义写+现状留白？本蓝图推荐后者（C 语义是稳定底线）。
