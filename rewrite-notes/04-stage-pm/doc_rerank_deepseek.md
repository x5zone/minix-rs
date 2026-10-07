# 04-stage-pm 文档重建蓝图（deepseek）

## 0. 元数据

```text
your_name(AI agent name) = deepseek
target_dir(关注的工作目录) = notes/rewrite/fork-syscall-rewrite/04-stage-pm
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
当前提交号 = 7bc7f0c219c4a1bca7845ffe109af0efb680f06c（2026-09-19）
任务 = R 相·重建蓝图：只产出本文件，不改任何正文。
本轮修订 = 2026-09-19（补做轮）：订正 K-05-09 的 tell_vfs 锚点——PM 目录下没有 `exit.c`，`exit.c:359` 改为并入 `forkexit.c:130,230,359`（`forkexit.c:359` 实测为 `tell_vfs(rmp, &m);`，七个调用点计数不变）；订正 `lib/libsys/getuptime.c:9-26` 为 `9-24`（该文件共 24 行）。
```

### 0.1 审查范围

- **范围内文档**：`04-stage-pm/` 下全部 22 个编号文档，即 `00-pm-overview.md`、`01`–`20` 共 21 篇编号文档与 `99-global-concepts.md` 词典 1 篇（实际文件与行数见 §0.3）。
- **范围内代码**：`minix3/minix/servers/pm/`（15 个 `.c`、6 个头文件，4747 行）、PM 协议面头文件 `minix3/minix/include/minix/{callnr.h,com.h,ipc.h,const.h,type.h,config.h,sysinfo.h,vm.h}`、`minix3/sys/sys/{signal.h,sigtypes.h,reboot.h,wait.h,ptrace.h,time.h,resource.h,syslimits.h}`、内核协同面 `minix3/minix/kernel/{main.c,table.c}`、`minix3/minix/kernel/system/do_fork.c`、SEF 运行库 `minix3/minix/lib/libsys/{sef.c,sef_init.c,sef_signal.c,asynsend.c,getuptime.c,clock_time.c}`、以及 `os/servers/pm/`（38 个模块，19280 行）。
- **范围内非 C 制品**：`minix3/etc/system.conf` 的 `service pm` 条目、`minix3/minix/servers/pm/Makefile`、`minix3/share/mk/bsd.own.mk`（`USE_*` 宏）、Rust 构建文件 `os/servers/pm/Cargo.toml`（`syscall_stats`/`sprofile` feature）、集成测试 `os/servers/pm/tests/run_once_integration.rs`、`os/qemu-tests/`。
- **边界材料**：`../00-master-plan/README.md`（阶段划分与启动因果链）；`../03-stage-rs/00-rs-overview.md`（前序 stage 已讲概念：SEF 生命周期、RS boot 四步、服务创建/发布/ready、priv/send mask、`RS_UP` 协议）；`../edge_todo.md` 的 E5/E6/E7（PM↔VM 联调、minix-sys PM wrapper 扩充、minix-types PM 协议面）；本目录 `plan.md` 与 `todo.md`（V3 收敛快照，作为线索而非结论）。
- **参考材料（不算正式文档）**：`plan.md`（446 行）、`todo.md`（589 行）、`draft/README.md`、`draft/pm-call-vfs-fork.md`。
- **明确范围外**：目标目录下的隐藏设计目录与临时设计目录（项目规范：中间产物，不读取、不引用）、其它 AI 的 `doc_rerank_*` 产物（未读取；本文件写入期间出现的任何同类产物一律不引用）、`tmp_design_and_todo/`、运行中的其它 stage 目录正文、内核侧 `sys_*` 实现与 VM/VFS 侧对端实现（只作契约引用）。

### 0.2 读取清单

| 类别 | 对象 | 读取方式 |
|------|------|---------|
| 旧文档 | 22 个编号文档全部小节标题 + 头部声明 + 正文（逐篇） | 全文精读或等价抽取（00–20、99 逐篇；通过 4 个并行抽取任务覆盖全文，关键小节回读） |
| C 源码 | `main.c`(424)、`forkexit.c`(807)、`signal.c`(855)、`alarm.c`(344)、`exec.c`(200)、`event.c`(353)、`getset.c`(223)、`misc.c`(447)、`schedule.c`(112)、`time.c`(131)、`trace.c`(276)、`utility.c`(156)、`table.c`(62)、`profile.c`(45)、`mcontext.c`(27) | 全文精读 |
| C 头文件 | `servers/pm/{mproc.h,glo.h,const.h,type.h,pm.h,proto.h}`、`include/minix/{callnr.h,com.h,ipc.h,config.h,sysinfo.h}`、`sys/sys/{signal.h,reboot.h,wait.h,ptrace.h,time.h,resource.h}` | 全文精读关键段 |
| 非 C 制品 | `minix3/etc/system.conf`（`service pm`）、`servers/pm/Makefile`、`os/servers/pm/Cargo.toml`、`os/servers/pm/tests/`、`os/qemu-tests/` | 定向读取 |
| Rust | `os/servers/pm/src/` 全部 38 个模块（模块头与公开面盘点）、`os/libs/minix-types` 与 `os/libs/minix-sys` 相关模块 | 盘点 + 抽查 |
| 边界 | master-plan README、edge_todo E5/E6/E7、03-stage-rs 旧总览、本目录 plan.md 结构 | 定向读取 |

### 0.3 使用的命令与关键输出（证据摘录）

```text
$ wc -l minix3/minix/servers/pm/*.c
 344 alarm.c  353 event.c  200 exec.c  807 forkexit.c  223 getset.c  424 main.c
 447 misc.c    45 profile.c 112 schedule.c 855 signal.c  62 table.c  131 time.c
 276 trace.c  156 utility.c  27 mcontext.c                （合计 4747）

$ ls 04-stage-pm/[0-9]*.md | wc -l     → 22（00–20 共 21 篇 + 99）
$ wc -l 04-stage-pm/[0-9]*.md          → 最小 93 行（99），最大 723 行（02）

# 旧文档之间的文件名引用（每目标计数）
04 41；11 39；02 38；05 37；03 33；01 25；09 21；06 20；13/16 18；07 16；
18 14；10/12/14 12；00 10；08 10；15 8；17 7；20 6；19 5；99 0；合计 ≈402 处

# 仓库其它位置引用 PM 文档
含 PM 文档文件名或 `04-stage-pm` 的其它 .md 227 处（17-stage-net、10-stage-mib、
08-stage-is、09-stage-init、06-stage-sched、edge3/edge4 等）
含引用的 .rs 文件 20 个（热点：pm/src/init.rs 6、vfs/src/ipc/dispatcher.rs 4）

# 违规引用：编号文档正文引用隐藏设计目录
11/12/13/14/15/16/17/18/19/20/05/06/07/08/09/10 各 1–2 处，合计约 21 处

# 关键事实复核
minix3/minix/kernel/table.c:52-53,55   ds → rs → pm 登记顺序（PM_PROC_NR=0）
minix3/minix/kernel/main.c:196,253,265  仅内核任务/RS/VM 立即可调度；其余 NO_PRIV|NO_QUANTUM|VMINHIBIT
minix3/sys/sys/signal.h:45            _NSIG = 64（不是 32）
minix3/sys/sys/signal.h:281-286       SIGS_IS_LETHAL = ILL/BUS/FPE/SEGV/EMT/ABRT；TERMINATION 再加 KILL/PIPE
minix3/minix/include/minix/sysinfo.h:11,14   SI_PROC_TAB=2、SI_CALL_STATS=9（不是 0/1）
minix3/sys/sys/reboot.h:54            RB_POWERDOWN = RB_HALT|0x800 = 0x808（不是 1）
minix3/minix/include/minix/com.h:1151 SUSPEND = -998
minix3/minix/include/minix/config.h:66-74  NR_SCHED_QUEUES=16、USER_Q=7、USER_QUANTUM=200
minix3/minix/lib/libsys/asynsend.c:17 ASYN_NR = 2*_NR_PROCS = 512（用户态表）
```

### 0.4 本次执行对旧文档的整体判断（结论先行）

旧目录（22 篇）的结构比 RS 目录合理：它已经按"启动 → 进程模型 → 生命周期 → 信号 → 外围"组织，plan.md 的"PM server 启动顺序 + fork 次主线"也基本成立。本蓝图的判断是：**旧目录的主要问题不是顺序，而是三件事**：

1. **事实错误密度高，且集中在容易伤人的 ABI 常量上**。本次逐条核对发现至少 12 处 P0/P1 级错误：`SI_PROC_TAB=0`（真值 2）、`RB_POWERDOWN=1`（真值 0x808）、`SCHEDULING_START=0/SET_NICE=5`（真值 0xF02/0xF04）、`SIGS_IS_LETHAL` 集合写错、`_NSIG=32`（真值 64）、`TO_NOEXEC=0x1`（真值 0x4）、`MAX_SECS=100M`（真值 `TMRDIFF_MAX/hz`）、`GID_MAX=0xFFFFFFFF`（真值 0x7FFFFFFF）、`VFS_PM_UNPAUSE` 锚到 `DS_RQ_BASE`、`SETSID` 回复值写错、`VFS_PM_SRV_FORK_REPLY=0x989`（真值 0x988）、nice 映射举例错误（1→7→0，真值 1→8→2）。其中 `SI_PROC_TAB` 与 `RB_POWERDOWN` 的**同一个错误也存在于 Rust 代码**（`os/servers/pm/src/misc.rs`），是待通电批次会踩中的潜伏 bug。
2. **锚点系统腐蚀**：旧文档大量使用 `manager.c:rproc（Lxxx，工具生成）`式自动标签，符号名与行号频繁错位（例如把 `cleanup` 标成 `tracer_died（L795）`、把 `exit_proc` 标成 `do_exit（L267）`、把 `do_fork` 的调度分支标成 L96）。本次抽查的锚点错误率在信号组（11/12/13/14/15）与 forkexit 组（07/09/10）最高。
3. **与实现进度强耦合的叙述仍在正文里**：`批次 A–H`、`Fix #NN`、"已接线 8 个/其余 40 个 ENOSYS"（当前实现已接线 41/47）、测试计数快照（116/226/242/260/278/320/381 各不相同且全部过期）、`SigSet=u64` 装不下内核信号位等。这些内容随实现推进立刻腐化，应移出正文或改为不写。

结构上值得改的点：**信号三篇（11/12/13）互为前后引用且各重述一遍处置链**；**fork 与 srv_fork（07/08）重复九步骨架**；**exit 与 wait（09/10）是同一条生产–消费管线**；**外部调用签名散落在十几篇里没有唯一权威面**；**99 词典过薄且过期**。因此本蓝图做三组合并、一处新建、一处扩写（详见 §4/§6）。

---

## 1. C 真序

### 1.0 阶段类型判定

PM 与 RS 同属**服务事件循环型**，但它的启动段多一层"被别人放行"的语义：

| 特征 | 证据 | 处理方式 |
|------|------|---------|
| 服务事件循环型（主） | `main.c:59-107` 永不终止的 receive → 分派 → reply | 运行时主线 = 主循环 + 一类请求的完整生命周期 |
| 启动链型（受控） | 内核装载全部 boot ELF 但仅放行内核任务/RS/VM；PM 由 RS 在 boot Step 2 放行并收到 `RS_INIT` | 单独成篇（新 01），严格按 kernel → VM → RS → PM 的因果链 |
| 管线型 | fork / exec / exit / wait 是跨 VM/VFS/内核的多阶段协议 | 生命周期按因果拆篇：创建（新 08）→ 终止回收（新 09）→ exec（新 14） |
| 集合型 | 47 个调用、T_* 全族、19 个旗标位 | 按"公共骨架 + 分组表 + 代表成员"组织（新 04/15/99） |

**与 RS 的关键差别**：PM 不在"谁先运行"的问题上自举——它的 ELF 已由内核装载，只是被 `RTS_NO_PRIV`/`RTS_VMINHIBIT` 抑制；RS 放行它之后，它面对的是一个**需要与 VFS 对齐两张进程表**的世界（`VFS_PM_INIT`），以及与 VM 建立内存协同的世界（`vm_fork`/`vm_willexit`/`vm_exit`）。

### 1.1 真序表

> 说明：本表是后续《序差表》（§4.4）与契约事实底线的来源。每步给出可核对的 C 锚点。

#### A 组：PM 进程的诞生（外部前置，kernel/VM/RS/SEF 侧）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| A1 | `boot_image[]` 按 ds → rs → pm → sched → vfs → … 登记；PM 为 `PM_PROC_NR=0` | `kernel/table.c:52-53,55`；`include/minix/com.h:62` | 登记序 ≠ 执行序 |
| A2 | 内核为**每个** boot 进程装载 ELF（`arch_boot_proc`），但只有内核任务、RS、VM 立即可调度 | `kernel/main.c:196-198,257-266` | 其余进程置 `RTS_NO_PRIV + RTS_NO_QUANTUM`（253）与 `RTS_VMINHIBIT + RTS_BOOTINHIBIT`（265-266） |
| A3 | VM 先运行，为 PM 等建立页表并解除抑制 | `01-stage-kernel/09-vm-boot-protocol.md`；`02-stage-vm/01-vm-init-main.md` | 前序 stage 结论 |
| A4 | RS boot Step 2 对 PM：`sched_init_proc` + `sys_privctl(SYS_PRIV_ALLOW)` + `init_service`（发 `RS_INIT`） | `servers/rs/main.c:375-389` | 前序 stage 结论（03-stage-rs 新 09） |
| A5 | PM 从 SEF 的 `_start` 进入；`sef_startup()` 拦截 init/ping/signal 请求 | `lib/libsys/sef.c:185-230`；`sef_signal.c` | SEF 运行库循环 |
| A6 | PM 注册回调：fresh/restart 两个 init 回调 + `process_ksig` 信号管理器（无 LU 回调、无 response 回调、无 signal handler） | `main.c:115-127` | 与 RS 的七回调形成对照 |
| A7 | SEF 收到 `RS_INIT` 后调用 `sef_cb_init_fresh`（`SEF_INIT_FRESH` 类型） | `lib/libsys/sef_init.c`；`main.c:131` | 若 PM 被 RS 重启则走 `SEF_CB_INIT_RESTART_STATEFUL`（A12） |

#### B 组：`sef_cb_init_fresh` 八步（`main.c:131-243`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| B1 | 初始化 `mproc[NR_PROCS]`：`init_timer`、`mp_magic=MP_MAGIC`、`mp_sigact=&mpsigact[i]`、`mp_eventsub=NO_EVENTSUB` | `main.c:146-152` | 空槽语义三件套 |
| B2 | 运行时构建三个信号集合：`core_sset`（8 个）、`ign_sset`（4 个）、`noign_sset`（6 个） | `main.c:137-165`；`glo.h:21-23` | 信号默认处置的输入 |
| B3 | `sys_getmonparams(monitor_params)` 取 boot monitor 参数 | `main.c:169` | `find_param` 的数据源 |
| B4 | `sys_getimage(image)` 取 boot image 表；`procs_in_use` 从 0 开始按表填充 | `main.c:175-177` | 第一代进程清单来源 |
| B5 | 逐条建槽：负槽号（内核任务）跳过；INIT 特例（自为父、pid=1、`KERNEL`、nice=0）；系统进程（父=RS/INIT、`IN_USE + PRIV_PROC`、`scheduler=NONE`、nice 由 `SRV_Q`） | `main.c:178-215` | 第一代进程树 |
| B6 | 每条目向 VFS 发 `VFS_PM_INIT`（slot/pid/endpoint），末尾以 `endpoint=NONE` 的同步调用作屏障 | `main.c:220-236` | 两表对齐 |
| B7 | `system_hz = sys_hz()` | `main.c:238` | 时间换算基准 |
| B8 | `sched_init()`：把 `IN_USE && !PRIV_PROC` 的进程（此刻只有 INIT）交给 SCHED | `main.c:241`；`schedule.c:20-50` | 用户态调度接管的起点 |

#### C 组：主循环（`main.c:59-107`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| C1 | `sef_receive_status(ANY, &m_in, &ipc_status)`；失败 panic | `main.c:61-62` | 唯一接收原语 |
| C2 | notify 分支（在 endpoint 校验之前）：CLOCK → `expire_timers(timestamp)`；其余通知忽略并 continue | `main.c:65-71` | 内核任务源不能过 `pm_isokendpt` |
| C3 | `who_e = m_in.m_source`；`pm_isokendpt` 失败 panic；`mp=&mproc[who_p]`；`call_nr=m_in.m_type` | `main.c:74-78` | 调用者上下文 |
| C4 | `EXITING` 调用者的延迟调用丢弃 | `main.c:80-82` | 退出竞态防护 |
| C5 | 第一路：`IS_VFS_PM_RS(call_nr) && who_e==VFS_PROC_NR` → `handle_vfs_reply()`，`result=SUSPEND` | `main.c:84-87` | VFS 回复面 |
| C6 | 第二路：`PROC_EVENT_REPLY` → `do_proc_event_reply()` | `main.c:88-89` | 事件订阅面 |
| C7 | 第三路：`IS_PM_CALL(call_nr)` → `call_index=call_nr-PM_BASE`，查 `call_vec`；越界/NULL → `ENOSYS` | `main.c:90-103` | 47 调用分派 |
| C8 | `result != SUSPEND` → `reply(who_p, result)` | `main.c:106`；`main.c:249-270` | 回复（允许 `SUSPEND` 悬挂） |

#### D 组：通知与内核信号（SEF 侧 + PM 侧）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| D1 | 内核给信号管理器发 SYSTEM 通知（位图），其中 `SIGKSIG=74` 表示有内核信号待取 | `sys/sys/signal.h:273-274`；`include/minix/com.h:601` | 内核信号入口 |
| D2 | SEF 拦截 SIGKSIG：`process_sigmgr_signals()` 循环 `sys_getksig`（取 target+位图）→ 逐信号调用注册的信号管理器回调 → `sys_endksig` 确认 | `lib/libsys/sef_signal.c`（`do_sef_signal_request` + `process_sigmgr_signals`） | SEF 运行库侧循环 |
| D3 | PM 侧回调 `process_ksig(target, signo)`：`pm_isokendpt`+`IN_USE + EXITING` 双检；`mp=mproc[0]` 伪装信号源；按 signo 决定 id（组播/精确/系统级）；`check_sig` | `signal.c:294-335` | 内核信号进入统一判定 |
| D4 | `SIGVTALRM/SIGPROF` 先 `check_vtimer` 重设虚拟定时器；`SIGSNDELAY` 在尾部兑现早先的 `DELAY_CALL` | `signal.c:326-328,344-369` | 定时器与延迟停止的接缝 |
| D5 | `SIGSNDELAY` 分支：清 `DELAY_CALL`；若当前有 `VFS_CALL + EVENT_CALL` 则 `stop_proc(may_delay=FALSE)`；否则 `check_pending` | `signal.c:344-369` | 延迟停止兑现 |

#### E 组：信号判定与处置（`signal.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| E1 | `do_kill` → `check_sig(pid, signo, ksig=FALSE)`；`do_srv_kill` 仅 RS 可用且 `ksig=TRUE` | `signal.c:197-221` | 用户/RS 两条入口 |
| E2 | `check_sig` 前置：signo 范围；`INIT_PID+SIGKILL→EINVAL`；广播 SIGTERM 先 `sys_kill(RS)` | `signal.c:582-589` | 两个特例 |
| E3 | 逆序全表扫描（`NR_PROCS-1..0`）：pid 四态选择（>0/0→组/-1/<-1）、广播 SIGKILL 跳过 PRIV_PROC、永远跳过 VM、权限四重判断 | `signal.c:597-629` | 目标集合与权限 |
| E4 | 每个命中：`count++`；`signo==0` 或 `EXITING` 跳过；否则 `sig_proc(rmp, signo, trace=TRUE, ksig)`；`>0` 命中后即停 | `signal.c:631-640` | 投递循环 |
| E5 | 自杀检测：调用者已 `EXITING` → 返回 `SUSPEND`（不许从坟里回话） | `signal.c:644` | 回复特例 |
| E6 | `sig_proc` 处置链（按序）：tracer 优先（`mp_sigtrace` + `trace_stop`）；`VFS_CALL` 或 `EVENT_CALL` 暂存 + `stop_proc`；PRIV_PROC 系统信号三分支（跳过 PM、非 ksig 回环 `sys_kill`、ksig 时栈回溯/消息化/退出）；用户进程 `badignore` 判定、ignore、mask、`TRACE_STOPPED` 暂存、caught（`unpause` + `sig_send`）、默认忽略集合、终止 | `signal.c:411-539` | 十条分支的优先级 |
| E7 | `sig_proc_exit`：记 `mp_sigstatus`；core 集合内且非 PRIV_PROC 打印 + 栈回溯 → `exit_proc(..., dump_core=TRUE)`；否则普通 `exit_proc` | `signal.c:546-563` | 信号触发的退出 |
| E8 | `check_pending`：`pending & !mask` 逐位投递；一旦新产生 `VFS_CALL + EVENT_CALL` 即 break（等 VFS 回复后重检） | `signal.c:651-682` | 解阻塞后的补投 |
| E9 | `restart_sigs`：`TRACE_EXIT` 优先 → `exit_proc`；否则 `PROC_STOPPED` → `check_pending` + `try_resume_proc` | `signal.c:687-714` | VFS 回复后的信号续作 |

#### F 组：信号安装与投递辅助（`signal.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| F1 | `do_sigaction`：KILL 直接 OK；越界 EINVAL；可选拷出旧动作；`SIG_IGN/DFL/handler` 三态维护 `ignore/catch`（IGN 同时清 pending/ksigpending）；`sa_mask` 剥离 KILL/STOP；存 `mp_sigreturn` | `signal.c:40-86` | 安装语义 |
| F2 | `do_sigpending`：回写 `mp_sigpending` | `signal.c:91-97` | 只读查询 |
| F3 | `do_sigprocmask`：四种 `how`（BLOCK/UNBLOCK/SETMASK/INQUIRE）；BLOCK 剥离 KILL/STOP 但 UNBLOCK 不剥；UNBLOCK/SETMASK 后 `check_pending` | `signal.c:102-155` | 掩码语义 |
| F4 | `do_sigsuspend`：`mask2` 保存旧掩码、置 `SIGSUSPENDED`、`check_pending`、返回 `SUSPEND` | `signal.c:160-171` | 原子等待 |
| F5 | `do_sigreturn`：恢复调用者掩码（剥 KILL/STOP）→ `sys_sigreturn` → `check_pending` | `signal.c:176-192` | 处理器返回 |
| F6 | `sig_send`：构造 `sigmsg`（`sm_mask` 取 `mask2` 或当前掩码）；把 `sa_mask` 并入 `mp_sigmask`；`SA_NODEFER` 决定是否自阻塞；`SA_RESETHAND` 复位；清 pending；`sys_sigsend`（EFAULT/ENOMEM → FALSE 让进程被杀，其他错误 panic） | `signal.c:776-830` | 位图 → 栈帧 |
| F7 | `sig_send` 后：`WAITING + SIGSUSPENDED` → 清位、`reply(EINTR)`、`try_resume_proc`；否则断言 `UNPAUSED`（由 `restart_sigs` 收尾） | `signal.c:832-851` | 两条收尾路径 |
| F8 | `stop_proc`：`sys_delay_stop`；OK → 置 `PROC_STOPPED`；EBUSY → `may_delay` 决定置 `DELAY_CALL` 还是 panic | `signal.c:226-261` | 停止原语 |
| F9 | `unpause`：`UNPAUSED` 已置即成功；`DELAY_CALL` 则先等；`WAITING + SIGSUSPENDED` 本地停；否则 `VFS_PM_UNPAUSE` 异步询问 | `signal.c:719-770` | 三路径 |
| F10 | `try_resume_proc`：`VFS_CALL + EVENT_CALL + EXITING` 不恢复；否则 `sys_resume` + 清 `PROC_STOPPED + UNPAUSED`（panic 契约） | `signal.c:266-289` | 恢复原语 |
| F11 | `trace_stop`：`sys_trace(T_STOP)` + 置 `TRACE_STOPPED`；tracer 正在 wait 则立即回 `W_STOPCODE(signo)` | `trace.c:255-276` | 调试暂停 |

#### G 组：fork 与 srv_fork（`forkexit.c:44-240`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| G1 | `do_fork` 容量闸门：满表或 `procs_in_use >= NR_PROCS-LAST_FEW` 且非 root → `EAGAIN` | `forkexit.c:59-65` | 保留席 |
| G2 | `next_child` 轮转找空槽（双 panic 守卫） | `forkexit.c:68-75` | 槽位复用 |
| G3 | `vm_fork(parent_e, next_child, &child_ep)` 同步；失败即返回（此时尚未改表） | `forkexit.c:78-80` | 可失败窗口 |
| G4 | 此后不可失败：`procs_in_use++`、整槽复制 `*rmc=*rmp`、`mp_sigact` 指针复原 + 动作表深拷贝、父槽记录、tracer 条件继承 | `forkexit.c:82-95` | 不可失败窗口 |
| G5 | 旗标收窄：普通 fork 继承 `IN_USE + DELAY_CALL + TAINTED`；PRIV_PROC 父 → 子 `scheduler=SCHED_PROC_NR`；计时/退出状态/interval 清零；`mp_started=getticks` | `forkexit.c:97-116` | 继承规则 |
| G6 | `get_free_pid()` 分配 PID（单调递增、跳过占用 pid/procgrp） | `forkexit.c:118-120`；`utility.c:34-51` | 命名 |
| G7 | 发 `VFS_PM_FORK`（child/parent endpoint、child pid；REUID/REGID=-1）→ `tell_vfs`（`asynsend3 AMF_NOREPLY` + 置 `VFS_CALL`） | `forkexit.c:122-130`；`utility.c:123-139` | 异步登记 |
| G8 | tracer 存在 → `sig_proc(child, SIGSTOP, trace=TRUE)`；返回 `SUSPEND` 等 VFS 回复 | `forkexit.c:132-139` | |
| G9 | VFS 回 `VFS_PM_FORK_REPLY` → `handle_vfs_reply`：可选 `sched_start_user`；失败则 `exit_proc` 拆子 + 父得 -1；成功回子 OK、回父 pid（`NEW_PARENT` 时抑制父回复） | `main.c:369-396` | fork 的后半 |
| G10 | `do_srv_fork`：RS 专有（否则 EPERM）；与 do_fork 同骨架；差异 = `PRIV_PROC` 保留、六字段 uid/gid 注入、`VFS_PM_SRV_FORK` 带真实 REUID/REGID、立刻 `reply(child, OK)` 并把 pid 作为返回值直接回复 RS（不 SUSPEND） | `forkexit.c:145-239` | 服务孵化器 |
| G11 | `VFS_PM_SRV_FORK_REPLY` 到达时空处理（子进程已可运行） | `main.c:398-401` | |

#### H 组：exit 与 wait（`forkexit.c:245-806`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| H1 | `do_exit`：`PRIV_PROC` 进程调 exit → 打印 + `sys_kill(SIGKILL)`；否则 `exit_proc(status, dump_core=FALSE)`；一律返回 `SUSPEND` | `forkexit.c:245-262` | 系统服务不许自杀 |
| H2 | `exit_proc` 前置：setuid 与 PRIV_PROC 双重抑制 core；记住 session leader 的 `procgrp`；有 alarm 则 `set_alarm(0)`；`sys_times` 记账进 `mp_child_*` | `forkexit.c:284-309` | 收尾前记账 |
| H3 | 若未停则 `sys_stop` 强制停 + `vm_willexit`；INIT 死亡打印栈回溯后 return；VFS 死亡 panic | `forkexit.c:326-345` | 顺序敏感 |
| H4 | 发 `VFS_PM_EXIT`（或 core 时 `VFS_PM_DUMPCORE` + term_sig/path）；`PRIV_PROC` 立即 `sys_clear`（不等 VFS）；旗标收窄为 `IN_USE + VFS_CALL + PRIV_PROC + TRACE_EXIT + PROC_STOPPED` 再置 `EXITING`；存 `mp_exitstatus`；非 core 立即 `zombify` | `forkexit.c:350-385` | 两阶段分界 |
| H5 | 收养：遍历全表，tracer 死亡 → `tracer_died`；父子关系改挂 INIT，若子在 `VFS_CALL` 置 `NEW_PARENT`，已是僵尸则 `check_parent(try_cleanup)`；session leader 死 → 对 `procgrp` 广播 `SIGHUP` | `forkexit.c:388-412` | 孤儿与挂断 |
| H6 | VFS 回复（EXIT/CORE）→ `publish_event(rmp)` 后 return（不重启信号）；事件链走完后 `exit_restart`：`sched_stop`、`scheduler=NONE`、core 时 `zombify`、非 PRIV 则 `sys_clear`、`vm_exit`、`TRACE_EXIT` 回复 tracer、`TOLD_PARENT` 则 `cleanup` | `main.c:356-367`；`forkexit.c:418-469` | 第二阶段 |
| H7 | `do_wait4`：`pidarg` 四态归一；全表扫描三重过滤（`IN_USE + TOLD_PARENT`、父或 tracer、父非 tracer 时僵尸可见性）；三环优先级 `TRACE_ZOMBIE` → `TRACE_STOPPED`（`W_STOPCODE`）→ `ZOMBIE`（`tell_parent`）；`WNOHANG` 返回 0；无子 `ECHILD`；有子未退则置 `WAITING` + 存 `wpid/waddr` + `SUSPEND` | `forkexit.c:474-563` | 三环 |
| H8 | `tell_parent`：可选 `sys_datacopy` rusage（失败 → 回错误、子留僵尸）；回 `W_EXITCODE(status, sigstatus)`、清父 `WAITING`、子 `ZOMBIE→TOLD_PARENT`、父子时间累加 | `forkexit.c:670-725` | 消费 |
| H9 | `zombify`：tracer 独立于父时先 `TRACE_ZOMBIE` 并 `tell_tracer`，否则直接 `ZOMBIE`；随后 `check_parent(try_cleanup=FALSE)`；`check_parent`：父在 wait → `tell_parent`，否则 `sig_proc(SIGCHLD)` | `forkexit.c:593-665` | 两级僵尸 |
| H10 | `tell_tracer`：tracer 等则回 `W_EXITCODE`；`TRACE_ZOMBIE→ZOMBIE` 交给真父 | `forkexit.c:731-754` | |
| H11 | `tracer_died` 三分支：清 tracer；子未 EXITING → `SIGKILL`；子 `TRACE_ZOMBIE` → 转 `ZOMBIE` + `check_parent(TRUE)` | `forkexit.c:759-790` | |
| H12 | `cleanup`：清 pid/flags/child times、`procs_in_use--`（不碰 endpoint/generation） | `forkexit.c:795-806` | 槽位释放 |

#### I 组：exec（`exec.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| I1 | `do_exec`：六字段 `VFS_PM_EXEC`（path/path_len/frame/frame_len/ps_str/endpt）→ `tell_vfs` → `SUSPEND` | `exec.c:38-56` | VFS 判定权限 |
| I2 | VFS 回 `VFS_PM_EXEC_REPLY` → `exec_restart(rmp, status, pc, newsp, newps_str)` | `main.c:349-354` | |
| I3 | 或 VFS/RS 直接调 `PM_EXEC_NEW`：仅 VFS/RS 可调；`sys_datacopy` 取 `exec_info`；`allow_setuid` = 无 tracer 且 VFS 允许；可失败注入 creds；`svuid/svgid=eff`；`TAINTED` 二重（setuid 位或 eff≠real）；存 `mp_name`/`mp_frame_addr/len`；置 `PARTIAL_EXEC`；回复带 suid 标志 | `exec.c:62-125` | 半态哨兵 |
| I4 | `exec_restart` 失败：`PARTIAL_EXEC` 时 `sys_kill(SIGKILL)`，否则回错误 | `exec.c:156-171` | 半态自毁 |
| I5 | `exec_restart` 成功：清 `PARTIAL_EXEC`；重置 caught 集合（`catch` 清位 + handler=DFL + 清 `sa_mask`）；tracer 存在且未 `TO_NOEXEC` 时先投 `SIGTRAP`（`TO_ALTEXEC` 时 `SIGSTOP`）；`sys_exec(endpt, sp, name, pc, ps_str)` | `exec.c:173-199` | 内核交接 |
| I6 | RS 专用 `do_execrestart`：仅 RS 可调；用保存的 `mp_frame_addr` 作为 sp 调 `exec_restart` | `exec.c:130-151` | 服务重启装载 |

#### J 组：凭证、调度、定时器、时间（`getset.c`/`schedule.c`/`alarm.c`/`time.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| J1 | `do_get` 七分支：GETGROUPS（0 查询/不足 EINVAL/拷出）、GETUID/GETGID（real 作返回 + eff 填 reply）、GETPID（含父 pid）、GETPGRP、GETSID（p==0 自取，否则 `find_proc` ESRCH）、ISSETUGID（TAINTED） | `getset.c:19-89` | 只读面 |
| J2 | `do_set` 六分支：SETUID（BSD 全置，real 不等且非 root → EPERM）、SETEUID（三重校验）、SETGID/SETEGID、SETGROUPS（仅 root、ngroups 上限、逐元素 `GID_MAX`、尾部清零）、SETSID（已是组长 EPERM）；成功后统一 `tell_vfs(VFS_PM_SET*)` + `SUSPEND` | `getset.c:94-222` | 写面必同步 VFS |
| J3 | VFS 回复：SETUID/GID/GROUPS → `reply(OK)`；SETSID → `reply(mp_procgrp)` | `main.c:334-347` | 回复值不同 |
| J4 | `sched_init`：boot 后把 INIT 交给 SCHED（`sched_start(SCHED, INIT, INIT, USER_Q, USER_QUANTUM, cpu=-1)`） | `schedule.c:20-50` | 启动接管 |
| J5 | `sched_start_user`：`nice_to_priority`；父是 PRIV_PROC 时继承自 INIT；`sched_inherit` | `schedule.c:55-84` | fork 子进程调度 |
| J6 | `sched_nice`：`KERNEL + NONE` → EINVAL；`SCHEDULING_SET_NICE` `_taskcall` | `schedule.c:89-112` | nice 变更 |
| J7 | `nice_to_priority`/`get_nice_value`：`[PRIO_MIN,PRIO_MAX]` ↔ `[MAX_USER_Q,MIN_USER_Q]` 的 41/16 线性缩放 + 钳位 | `utility.c:91-103`；`main.c:275-289` | 双向映射 |
| J8 | `do_getsetpriority`：仅 PRIO_PROCESS；who=0 自取否则 `find_proc`；eff 三重；GET 返回 `nice-PRIO_MIN`；降低 nice 需 root（EACCES）；SET 走 `sched_nice` + 存 `mp_nice` | `misc.c:239-286` | 优先级调用 |
| J9 | `do_itimer`：三族；setval/getval 至少一个非零；`is_sane_timeval`；REAL 走 `get/set_realtimer`，VIRTUAL/PROF 走 `getset_vtimer`（`sys_vtimer`）；可拷贝旧值回用户 | `alarm.c:92-154` | 定时器入口 |
| J10 | `set_alarm`（`set_timer`+`ALARM_ON`）/`cause_sigalrm`（三重守卫、interval 重设、`check_sig(SIGALRM)`） | `alarm.c:299-343` | REAL 到期 |
| J11 | CLOCK notify → `expire_timers(timestamp)` → 到期回调（含 `cause_sigalrm`） | `main.c:65-67` | 时钟驱动 |
| J12 | `do_gettime`（REALTIME/MONOTONIC → `boottime+clock/hz`、`(clock%hz)*1e9/hz`）、`do_getres`（`1e9/hz`）、`do_settime`（root；REALTIME → `sys_settime`，MONOTONIC → EINVAL）、`do_time`（`clock_time`）、`do_stime`（root；`boottime=sec-realtime/hz` → `sys_stime`） | `time.c:22-131` | 五调用 |
| J13 | `getset_vtimer`：旧值回绕规则（`oldticks<=0 → interval`）；`check_vtimer` 到期重设 | `alarm.c:160-241` | 虚拟定时器 |

#### K 组：ptrace（`trace.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| K1 | `do_trace` 序言按 req 分派：T_OK（子自声明，已 tracer → EBUSY）、T_ATTACH（七重守卫）、T_STOP（EINVAL）、T_READB/WRITEB_INS（仅 root，透传 `sys_trace`） | `trace.c:42-135` | 双入口 |
| K2 | 其余命令共同守卫：`find_proc`、非 EXITING、`tracer==who_p`、`TRACE_STOPPED` | `trace.c:140-143` | |
| K3 | T_EXIT（置 `TRACE_EXIT`；有 VFS/EVENT 调用则存 exitstatus 延后，否则 `exit_proc`；`SUSPEND`）、T_SETOPT、T_GETRANGE/SETRANGE（`TS_INS/DATA`、`sys_vircopy`）、T_DETACH（重放 `mp_sigtrace` + 可选投信号 + 清 `TRACE_STOPPED` + `check_pending`）、T_RESUME/STEP/SYSCALL（可选投信号；仍有 sigtrace 则伪装成功） | `trace.c:145-242` | |
| K4 | 尾部 `sys_trace` 透传 + reply data | `trace.c:244-249` | |
| K5 | `trace_stop` 见 F11 | `trace.c:255-276` | |

#### L 组：杂项（`misc.c`/`profile.c`/`mcontext.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| L1 | `do_sysuname`：`uts_tbl` 9 槽（3 个 NULL）；越界/不支持字段 EINVAL；拷出按调用者长度截断，返回拷贝字节数 | `misc.c:71-100` | 兼容块 |
| L2 | `do_getsysinfo`：非 root → 打印 + 栈回溯 + EPERM；`SI_PROC_TAB`（整表）/`SI_CALL_STATS`（feature）；`len != size → EINVAL`；`sys_datacopy` | `misc.c:107-144` | 表导出 |
| L3 | `do_getprocnr`：仅 RS；pid → endpoint | `misc.c:149-164` | RS 专用 |
| L4 | `do_getepinfo`：endpoint → pid/uid/euid/gid/egid/ngroups（组列表按调用者容量截断拷出） | `misc.c:169-193` | |
| L5 | `do_reboot`：仅 root；`abort_flag`；POWERDOWN 时通知 readclock；`check_sig(-1,SIGKILL)` → `sys_stop(INIT)` → `tell_vfs(VFS_PM_REBOOT)` → `SUSPEND`；VFS 回复后 `sys_abort` | `misc.c:198-233`；`main.c:304-312` | 定序 |
| L6 | `do_svrctl`：IOCGROUP 'P'/'M'；PMSETPARAM（最多 2 条本地覆盖）/PMGETPARAM（keylen=0 拷贝全部；否则本地覆盖优先 → `find_param`；E2BIG/ESRCH） | `misc.c:291-395` | 参数访问 |
| L7 | `do_getrusage`：SELF → `sys_times`；CHILDREN → `mp_child_*`；`set_rusage_times` 换算；VM 补内存字段；拷出 | `misc.c:400-447` | |
| L8 | `do_sprofile`：`SPROFILE` 关闭时 ENOSYS；开启时 `sys_sprof(PROF_START/STOP)` | `profile.c:22-45` | feature |
| L9 | `do_get/setmcontext`：直接透传 `sys_get/setmcontext` | `mcontext.c:15-25` | |
| L10 | `find_param`：monitor_params 的 `key=value\0` 扫描 | `utility.c:57-71` | |
| L11 | `set_rusage_times`：ticks×1e6/hz 的 u64 分解 | `utility.c:144-156` | |

#### M 组：事件订阅（`event.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| M1 | 订阅表 `subs[NR_SUBS=4]` + `nsubs` + `nested`；事件两种：EXIT(0x01)/SIGNAL(0x02) | `event.c:58-67`；`syslib.h:289-293` | 数据结构 |
| M2 | `do_proceventmask`：仅 PRIV_PROC；已存在则更新掩码或（mask=0 且无等待）删除；新订阅追加；满 ENOMEM | `event.c:170-211` | 订阅管理 |
| M3 | `publish_event`：服务死亡时先清其订阅；置 `EVENT_CALL` + 游标 0；`resume_event` | `event.c:316-353` | 发布 |
| M4 | `resume_event`：按 `EXITING`/`UNPAUSED` 选事件；从游标起找下一个掩码匹配订阅者 `asynsend3`，否则清 `EVENT_CALL` 并转 `exit_restart`/`restart_sigs` | `event.c:74-123` | 串行推进 |
| M5 | `do_proc_event_reply`：非 PRIV_PROC → ENOSYS；七步校验；`waiting--`；必要时 `remove_sub`；否则游标++ + `resume_event`；一律 `SUSPEND` | `event.c:218-309` | 回复 |
| M6 | `remove_sub`：有序删除 + 全表游标修正（命中者立即 `resume_event`，`nested` 守卫） | `event.c:130-161` | 删除 |

#### N 组：PM 被重启（`SEF_INIT_RESTART`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| N1 | RS 重启 PM 时 SEF 走 `SEF_CB_INIT_RESTART_STATEFUL`（SEF 库通用状态迁移） | `main.c:119`；`lib/libsys/sef_init.c` | PM 未自定义 restart 回调 |
| N2 | 重启后的 PM 重新执行八步 init，并与 VFS 重对齐进程表 | 同 B 组 | 真实性与恢复语义是旧目录空白（见 §3 缺口 G-14） |

### 1.2 真序核对记录（G1 抽样十条）

| 抽样 | 断言 | 核对命令 | 结果 |
|------|------|---------|------|
| 1 | PM 在 boot_image 第 3 个服务（ds→rs→pm） | `grep -n PM_PROC_NR minix3/minix/kernel/table.c` | 一致（55 行） |
| 2 | 只有内核任务/RS/VM 立即可调度 | `sed -n '196p' kernel/main.c` | 一致 |
| 3 | `_NSIG=64` | `grep -n "_NSIG" sys/sys/signal.h` | 一致（45 行） |
| 4 | `SI_PROC_TAB=2` | `grep -n SI_PROC_TAB include/minix/sysinfo.h` | 一致（11 行） |
| 5 | `RB_POWERDOWN=0x808` | `grep -n RB_POWERDOWN sys/sys/reboot.h` | 一致（54 行） |
| 6 | `SUSPEND=-998` | `grep -n "define SUSPEND" include/minix/com.h` | 一致（1151 行） |
| 7 | `VFS_PM_UNPAUSE=0x909` | `sed -n '520,532p' include/minix/com.h` | 一致（528 行） |
| 8 | `ASYN_NR=2*_NR_PROCS` 且是用户态表 | `sed -n '15,20p' lib/libsys/asynsend.c` | 一致 |
| 9 | `get_free_pid` 首 PID 相位（初值 2，先 ++ 得 3） | `sed -n '34,50p' servers/pm/utility.c` | 一致 |
| 10 | `cleanup` 在 forkexit.c:795-806 | `sed -n '795,806p' servers/pm/forkexit.c` | 一致 |

---

## 2. 知识点全集

### 2.0 编号与列说明

- 编号 `K-<新篇章号>-<序号>`：stage 内唯一，且直接编码去向。§5 契约的知识点清单只引用编号，完整行在本节。
- **类型**：概念 / 机制 / 数据结构 / 接口协议 / 约束不变量 / 架构演进 / 工具工程 / 测试。
- **来源**：`存量`（来自旧文档）或 `新增`（旧文档没有，由 §3 覆盖审计发现并追加入池）。存量条目要回答"搬到哪里去"，新增条目要回答"证据锚点在哪里"。
- **备注**：标注"主讲述点"与跨篇合并项；`修正` 表示旧文档该处与 C 源冲突、新篇必须写对（依据见 §3.5 事实勘误表）。

### 2.1 新 00（总览）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-00-01 | PM 四重权威（表/信号/生命周期/记账） | 概念 | 存量 | 00 §1.2 | `mproc[NR_PROCS]`；`main.c:122`；`forkexit.c` | 建立全 stage 心智模型 | 主讲述点 |
| K-00-02 | 微内核只留调度与 IPC，进程语义在用户态 | 概念 | 存量 | 00 §1.1 | `pm/` 全部；`kernel/main.c` | 回答"为什么有 PM" | 主讲述点 |
| K-00-03 | PM 在 boot 链的位置（装载→抑制→RS 放行） | 概念 | 存量 | 00 §1.1；01 §1.1 | `kernel/main.c:196,253,265`；`kernel/table.c:55` | 与 RS stage 衔接 | 主讲述点；详见新 01 |
| K-00-04 | 主循环三问（谁/要什么/怎么回） | 概念 | 存量 | 00 §1.3 | `main.c:59-107` | 阅读锚 | 详见新 04 |
| K-00-05 | 22 篇旧目录的映射与问题诊断 | 工具工程 | 存量 | 00 §2.1 | 旧目录 | 说明重建理由 | 主讲述点；本蓝图 §0.4 |
| K-00-06 | Rust 三层镜像（mproc 状态层/逻辑层/ipc 分发层） | 架构演进 | 存量 | 00 §2.2 | `os/servers/pm/src/` | 代码导航 | 主讲述点 |
| K-00-07 | 47 个调用号空间与接线模型 | 概念 | 存量 | 00 §3.1 | `callnr.h:14-60`；`ipc/calls.rs` | 回答"PM 对外多大" | 修正：不写具体接线计数 |
| K-00-08 | 跨阶段依赖 E1/E2/E6/E7 | 工具工程 | 存量 | 00 §3.2 | `edge_todo.md` | 实现边界 | 主讲述点；不写快照数字 |
| K-00-09 | 新目录导航表与阅读路线 | 工具工程 | 存量 | 00 §2.3/§6 | 新目录 TOC | 导航枢纽 | 主讲述点 |
| K-00-10 | KernelGateway 单归属 + 域窄 trait 视图 | 架构演进 | 存量 | 00 §4.1；plan §4.1 | `exit.rs`；`edge_todo.md` E6 | 内核能力边界规约 | 主讲述点；细节归新 18 |

### 2.2 新 01（启动与初始化）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-01-01 | PM 的装载与放行链（内核装载全部 ELF；抑制；RS 放行） | 概念 | 存量 | 01 §1.1 | `kernel/main.c:196,253,265-266`；`table.c:55` | 回答"PM 从哪来" | 主讲述点；修正"由 RS 装载"的模糊说法 |
| K-01-02 | SEF 运行库与 PM 的回调集（仅 fresh/restart + signal manager） | 接口协议 | 存量 | 01 §1.2；11 §4.1 | `main.c:115-127`；`sef_signal.c` | 与 RS 七回调对照 | 主讲述点 |
| K-01-03 | 八步初始化依赖链与两不变量（先本地后对端、先静态后动态） | 机制 | 存量 | 01 §1.3 | `main.c:131-243` | 启动顺序依据 | 主讲述点 |
| K-01-04 | 空槽初始化四件套（timer/magic/sigact/eventsub） | 机制 | 存量 | 01 §2.4；02 §2.4 | `main.c:146-152`；`mproc.h:16-22,106` | 空槽语义 | 与 02 合并讲，01 只给调用点 |
| K-01-05 | 三个信号集合的运行时构建 | 数据结构 | 存量 | 01 §2.4；99 §2.3 | `main.c:137-165`；`glo.h:21-23` | 信号默认处置输入 | 主讲述点 |
| K-01-06 | boot monitor 参数与 boot image 获取 | 接口协议 | 存量 | 01 §2.4 | `main.c:169,175` | 数据来源 | 主讲述点；GETMONPARAMS 缺口见 18 |
| K-01-07 | 第一代进程树（INIT 自父、系统进程挂 RS、INIT_PID） | 机制 | 存量 | 01 §1.4/§2.4 | `main.c:178-215` | 回答"谁是谁的父" | 主讲述点 |
| K-01-08 | INIT 的调度与 nice 初值（KERNEL → sched_init 接管） | 机制 | 存量 | 01 §2.4/§2.6 | `main.c:199-200`；`schedule.c:20-50` | 首进程调度 | 机制细节归 07 |
| K-01-09 | 系统进程的 PRIV_PROC 与 scheduler=NONE | 约束不变量 | 存量 | 01 §2.4 | `main.c:202-215` | 特权边界 | 主讲述点 |
| K-01-10 | `VFS_PM_INIT` 逐条 + 末条屏障 | 接口协议 | 存量 | 01 §1.5/§2.4 | `main.c:220-236`；`com.h:520,547-551` | 两表对齐 | 主讲述点；协议面详 05 |
| K-01-11 | `system_hz = sys_hz()` | 数据结构 | 存量 | 01 §2.4 | `main.c:238`；`glo.h:25` | 时间换算基准 | 与 16 合并边界 |
| K-01-12 | `sched_init` 的扫描与两断言 | 机制 | 存量 | 01 §2.6；16 §2.1 | `schedule.c:20-50` | 用户态调度起点 | 主讲述点归 07 |
| K-01-13 | `reply`/`get_nice_value`/`handle_vfs_reply` 的归属锚 | 工具工程 | 存量 | 01 §2.5 | `main.c:249-270`；`main.c:275-289`；`main.c:294-424` | 函数归属纪律 | 各自归 04/07/05 |
| K-01-14 | PM 重启路径（`SEF_CB_INIT_RESTART_STATEFUL`） | 机制 | 新增 | —（仅 01 §1.2 一句带过） | `main.c:119`；`lib/libsys/sef_init.c` | 回答"PM 被 RS 重启后发生什么" | 新增（缺口 G-14） |
| K-01-15 | `BootParams` 显式启动契约（Rust） | 架构演进 | 存量 | 01 §3.1 | `init.rs BootParams` | Rust 形态 | 主讲述点 |
| K-01-16 | `PmServer` 构造即空表 + `init()` 分步 | 架构演进 | 存量 | 01 §3.2 | `init.rs` | Rust 形态 | 主讲述点 |
| K-01-17 | 启动期的 VFS 同步客户端（Rust `vfs_init_sync`） | 架构演进 | 存量 | 01 §3.5/§4.5 | `init.rs` | Rust 形态 | 与 05 引用 |

### 2.3 新 02（进程结构 mproc）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-02-01 | 四副本进程表（proc/vmproc/fproc/mproc）与 endpoint 对齐 | 概念 | 存量 | 02 §1.1 | `mproc.h:1-5` | 微内核信任边界 | 主讲述点 |
| K-02-02 | 单槽巨型结构的问题与字段族分组依据 | 概念 | 存量 | 02 §1.2/§1.3 | `mproc.h:24-83` | 分层准则 | 主讲述点 |
| K-02-03 | 状态分层：互斥用枚举、可组合用组合子 | 概念 | 存量 | 02 §1.4 | `forkexit.c:374` | A-1/A-2 判据 | 主讲述点 |
| K-02-04 | `mpsigact` 独立表的体积隔离（约 80%）与深拷贝 | 数据结构 | 存量 | 02 §1.5 | `mproc.h:16-22`；`forkexit.c:88-89` | 回答"为什么动作表在槽外" | 主讲述点 |
| K-02-05 | `struct mproc` 全字段（修正为 41 个字段） | 数据结构 | 存量 | 02 §2.1 | `mproc.h:24-83` | 字段全景 | 修正：旧文写"约 60" |
| K-02-06 | 19 个旗标位值与语义 | 数据结构 | 存量 | 02 §2.2 | `mproc.h:86-104` | 状态位权威 | 主讲述点 |
| K-02-07 | `VFS_CALL`/`EVENT_CALL` 的联合检查语义 | 约束不变量 | 存量 | 02 §2.2 | `signal.c:279,425,672,693` | 阻塞联合语义 | 主讲述点 |
| K-02-08 | `SigSet` 位语义（bit=(signo-1)，`_NSIG=64`） | 数据结构 | 存量 | 02 §2.3；99 | `sys/sigtypes.h:60-71`；`signal.h:45` | 位图契约 | 修正：旧 11 写 `_NSIG=32` |
| K-02-09 | 四个信号位图（ignore/catch/mask/mask2/pending/ksigpending/sigtrace）分工 | 数据结构 | 存量 | 02 §2.3 | `mproc.h:50-58` | 信号状态词汇 | 主讲述点 |
| K-02-10 | 初始化契约与空槽默认值（NO_PID/NO_TRACER/NO_EVENTSUB） | 约束不变量 | 存量 | 02 §2.4/§4.5 | `main.c:146-152`；`const.h:8-13` | 空槽语义 | 与 01/03 合并 |
| K-02-11 | `MP_MAGIC` 与 MIB 校验（PM 只写不读） | 机制 | 存量 | 02 §2.4 | `mproc.h:106`；`main.c:149` | 跨服务令牌 | 主讲述点 |
| K-02-12 | fork 的字段继承与修正项 | 机制 | 存量 | 02 §2.5 | `forkexit.c:87-114` | 继承规则 | 主讲述点；流程归 08 |
| K-02-13 | Rust `Process` 四层分层 | 架构演进 | 存量 | 02 §3.1 | `mproc/mproc.rs` | Rust 形态 | 主讲述点 |
| K-02-14 | 19 flags → 状态机枚举 + 组合子 | 架构演进 | 存量 | 02 §3.2 | `mproc/{lifecycle,block}.rs` | Rust 形态 | 主讲述点 |
| K-02-15 | `SignalState` 与 `Box<[SigAction; _NSIG]>` | 架构演进 | 存量 | 02 §3.4/§3.6 | `mproc/signal.rs` | Rust 形态 | 主讲述点 |
| K-02-16 | `mproc` 的 C 字节镜像（Rust `mproc/wire.rs`，D-29） | 架构演进 | 新增 | —（旧文未提） | `os/servers/pm/src/mproc/wire.rs`；`getsysinfo` SI_PROC_TAB | 跨服务表导出契约 | 新增（缺口 G-04） |

### 2.4 新 03（进程表与身份）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-03-01 | 三层身份：slot / endpoint / PID | 概念 | 存量 | 03 §1.1 | `endpoint.h:45-69` | 索引模型 | 主讲述点 |
| K-03-02 | slot 是四表对齐的唯一坐标 | 约束不变量 | 存量 | 03 §1.2 | `mproc.h:1-5` | 跨服务一致性 | 主讲述点 |
| K-03-03 | `NR_PIDS=30000` 的 short 兼容来源；`NO_PID=0`/`INIT_PID=1` 保留 | 约束不变量 | 存量 | 03 §1.3 | `const.h:3-5,8-9` | PID 边界 | 主讲述点 |
| K-03-04 | PID 分配与进程组借用的冲突扫描 | 机制 | 存量 | 03 §1.3/§2.4 | `utility.c:34-51` | killpg 正确性 | 主讲述点；首相位 3 |
| K-03-05 | endpoint generation 防陈旧引用 | 机制 | 存量 | 03 §1.4 | `endpoint.h:45-51` | 防伪机制 | 主讲述点；编码公式归 99 |
| K-03-06 | generation 由内核在 `sys_fork` 递增，PM 只存验 | 约束不变量 | 存量 | 03 §2.6 | `kernel/system/do_fork.c`；`main.c:218`；`forkexit.c:111` | 所有权归属 | 主讲述点 |
| K-03-07 | 容量纪律 `procs_in_use` + `LAST_FEW=2` | 机制 | 存量 | 03 §1.5 | `forkexit.c:32,59-64` | 服务预留席 | 主讲述点 |
| K-03-08 | `pm_isokendpt` 三层检查与 errno 契约 | 接口协议 | 存量 | 03 §2.2 | `utility.c:108-118` | 调用者校验 | 主讲述点 |
| K-03-09 | `find_proc` 线性扫描 | 机制 | 存量 | 03 §2.3 | `utility.c:76-86` | pid→slot | 主讲述点 |
| K-03-10 | `cleanup` 释放槽但不 bump generation | 约束不变量 | 存量 | 03 §2.5/§3.3 | `forkexit.c:795-806` | 释放语义 | 修正符号误标（旧文标 `tracer_died`） |
| K-03-11 | `ProcTable` 聚合 + `EndpointError` 类型化 | 架构演进 | 存量 | 03 §3.1/§3.2 | `mproc/{table,pid_gen}.rs` | Rust 形态 | 主讲述点 |
| K-03-12 | 特殊 endpoint 范围（SELF/NONE/ANY）与 EINVAL 边界 | 约束不变量 | 存量 | 03 §4.3 | `endpoint.h:50-57`；`com.h:55` | 边界值 | 主讲述点 |

### 2.5 新 04（主循环与消息分发）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-04-01 | 单线程事件循环与三问 | 概念 | 存量 | 04 §1.1 | `main.c:59-107` | 执行模型 | 主讲述点 |
| K-04-02 | 消息空间分段（PM 0x000/VFS 0x100/VFS_PM 0x900/PROC_EVENT 0xE00/NOTIFY） | 接口协议 | 存量 | 04 §1.2 | `callnr.h:9`；`com.h:513-514,598,619` | 协议空间 | 主讲述点 |
| K-04-03 | 三路分发（VFS 回复 / 事件回复 / PM 调用） | 机制 | 存量 | 04 §1.3 | `main.c:84-103` | 分发本质 | 主讲述点 |
| K-04-04 | `SUSPEND(-998)` 的三子情形（等待/异步/永不回复） | 接口协议 | 存量 | 04 §1.4 | `com.h:1151`；`forkexit.c:559`；`exec.c:55`；`getset.c:222` | 回复契约 | 主讲述点 |
| K-04-05 | ENOSYS 兜底与 panic fail-fast 的分界 | 约束不变量 | 存量 | 04 §1.5 | `main.c:61-62,75-76,94-103` | 错误策略 | 主讲述点 |
| K-04-06 | notify 处理在 endpoint 校验之前 | 约束不变量 | 存量 | 04 §2.2 | `main.c:65-71` | 内核任务负 endpoint | 主讲述点 |
| K-04-07 | `EXITING` 调用者的延迟调用丢弃 | 机制 | 存量 | 04 §2.3 | `main.c:80-82` | 退出竞态防护 | 主讲述点 |
| K-04-08 | `call_vec` 分发表与索引规则 | 数据结构 | 存量 | 04 §2.6 | `table.c:14-62` | 分发表原型 | 主讲述点 |
| K-04-09 | 47 个调用号 → handler → 文档归属全表 | 工具工程 | 存量 | 04 §2.7 | `callnr.h:14-60`；`table.c` | 全局导航 | 主讲述点；数值归 99 |
| K-04-10 | `reply` 的三要点与 `mp_reply` 预填载荷 | 机制 | 存量 | 04 §2.5 | `main.c:249-270` | 回复语义 | 主讲述点 |
| K-04-11 | `ENABLE_SYSCALL_STATS` 统计与 `SI_CALL_STATS` | 工具工程 | 存量 | 04 §2.8；20 §2.2 | `main.c:34-36,95-97`；`misc.c:129-133` | 调试面 | 归 99 排除项 |
| K-04-12 | Rust `ReplyIntent`/`PmCall`/`run_once` 拆分 | 架构演进 | 存量 | 04 §3.2-§3.4 | `ipc/{dispatcher,calls}.rs`；`init.rs` | Rust 形态 | 主讲述点 |

### 2.6 新 05（PM↔VFS 异步协议）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-05-01 | 六类协作场景（setuid/fork/exec/exit/unpause/reboot） | 概念 | 存量 | 05 §1.1 | `com.h:520-531` | 协议存在理由 | 主讲述点 |
| K-05-02 | 异步必要性与双向同步死锁 | 概念 | 存量 | 05 §1.2 | `utility.c:134` | 核心动机 | 主讲述点 |
| K-05-03 | `VFS_CALL` 作 continuation 挂载点 | 机制 | 存量 | 05 §1.2 | `mproc.h:95` | C 隐式延续 | 主讲述点 |
| K-05-04 | 12 请求 / 11 回复的编号与寻址键 | 接口协议 | 存量 | 05 §2.1 | `com.h:517-544,547` | 协议面 | 主讲述点；数值归 99 |
| K-05-05 | 载荷字段布局（`m7_i*`/`m7_p*` 复用） | 接口协议 | 存量 | 05 §2.1 | `com.h:549-583` | 编解码契约 | 主讲述点 |
| K-05-06 | `tell_vfs` 三段式与 not-idle panic | 机制 | 存量 | 05 §2.2 | `utility.c:123-139` | 发送方不变量 | 主讲述点 |
| K-05-07 | `handle_vfs_reply` 四段（REBOOT 特例/解析/清旗标/分支） | 机制 | 存量 | 05 §2.3 | `main.c:294-424` | 收口总览 | 主讲述点 |
| K-05-08 | 11 路回复行为表（含 SETSID 回 procgrp） | 机制 | 存量 | 05 §2.3 | `main.c:334-419` | 回复语义速查 | 修正 SETSID 行；FORK 行 |
| K-05-09 | 七个 `tell_vfs` 调用点（含 srv_fork 的正确归属） | 工具工程 | 存量 | 05 §2.4 | `forkexit.c:130,230,359`；`getset.c:219`；`exec.c:52`；`signal.c:767`；`misc.c:230` | 发出后行为地图 | 修正：旧文把 srv_fork 行写成 VFS_PM_EXIT |
| K-05-10 | `NEW_PARENT` 与 `UNPAUSED` 生命周期 | 机制 | 存量 | 05 §2.5 | `forkexit.c:402-403`；`main.c:410` | 收养与解除暂停 | 主讲述点 |
| K-05-11 | VFS 启动握手（init 逐条 + 屏障；Rust `vfs_init_sync`） | 接口协议 | 存量 | 01 §1.5；05 §2.1 | `main.c:220-236` | 从 boot 到运行期 | 主讲述点（从 01 并入） |
| K-05-12 | Rust `VfsCall`/`VfsReply` 类型化与端口 trait | 架构演进 | 存量 | 05 §3 | `minix-types/src/ipc/vfs.rs`；`pm/src/ipc/vfs.rs` | Rust 形态 | 主讲述点 |
| K-05-13 | 错误保真规约（V3-P2-6：透传 errno 不吞） | 约束不变量 | 存量 | plan §4.2 | `plan.md:231` | 跨服务错误语义 | 主讲述点；归 plan/18 |

### 2.7 新 06（进程事件发布/订阅）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-06-01 | 事件的用途（SysV IPC 阻塞中断 + 资源清理） | 概念 | 存量 | 06 §1.1 | `event.c:1-9` | 存在理由 | 主讲述点 |
| K-06-02 | 两种事件 EXIT/SIGNAL 与 `PROC_EVENT*` wire | 数据结构 | 存量 | 06 §1.1/§2.1 | `syslib.h:289-293`；`com.h:597-619` | 事件字母表 | 主讲述点 |
| K-06-03 | 串行投递的理由与容量上界（`ASYN_NR=2*_NR_PROCS`，用户态表） | 约束不变量 | 存量 | 06 §1.3 | `event.c:10-25`；`lib/libsys/asynsend.c:17` | 有界性论证 | 修正：`ASYN_NR` 是用户态表而非内核预留 |
| K-06-04 | 不支持按进程订阅（竞态）与掩码变更脆弱性 | 约束不变量 | 存量 | 06 §1.4 | `event.c:27-41` | 订阅语义 | 主讲述点 |
| K-06-05 | 事件生命周期链（publish→resume→reply→remove） | 机制 | 存量 | 06 §1.5 | `event.c:74-123,218-309,316-353` | 心智模型 | 主讲述点 |
| K-06-06 | `EVENT_CALL` + `mp_eventsub` 游标 | 数据结构 | 存量 | 06 §1.5/§2.1 | `mproc.h:27,104`；`const.h:13` | 延续编码 | 主讲述点 |
| K-06-07 | `nested` 重入守卫 | 约束不变量 | 存量 | 06 §1.5/§2.3 | `event.c:67,148-153` | 重入安全 | 主讲述点 |
| K-06-08 | `subs[NR_SUBS]` 紧凑前缀表与 `waiting` | 数据结构 | 存量 | 06 §2.1 | `event.c:58-67` | 表布局 | 主讲述点 |
| K-06-09 | `resume_event` 三段推进 | 机制 | 存量 | 06 §2.2 | `event.c:74-123` | 串行引擎 | 主讲述点 |
| K-06-10 | `remove_sub` 有序删除与游标修正 | 机制 | 存量 | 06 §2.3 | `event.c:130-161` | 不漏订阅者 | 主讲述点 |
| K-06-11 | `do_proceventmask` 订阅/退订/更新矩阵 | 接口协议 | 存量 | 06 §2.4 | `event.c:170-211` | 边界用例 | 主讲述点 |
| K-06-12 | `do_proc_event_reply` 七步校验与"不校对掩码" | 接口协议 | 存量 | 06 §2.5 | `event.c:218-309` | 防御性解析 | 主讲述点 |
| K-06-13 | 服务死亡时的订阅清理 | 机制 | 存量 | 06 §2.6 | `event.c:316-353` | 卫生 | 主讲述点 |
| K-06-14 | Rust `EventRegistry`/`EventCursor`/`ReplyIntent` | 架构演进 | 存量 | 06 §3 | `pm/src/event.rs`；`mproc/block.rs` | Rust 形态 | 主讲述点 |

### 2.8 新 07（调度交接）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-07-01 | 内核只提供原语，策略在 SCHED | 概念 | 存量 | 16 §1.1 | `minix/sched.h:7-12`；`schedule.c:37-43` | 交接理由 | 主讲述点 |
| K-07-02 | `sched_init` 把 INIT 交给 SCHED | 机制 | 存量 | 16 §2.1 | `schedule.c:20-50` | 启动接管 | 主讲述点 |
| K-07-03 | `IN_USE && !PRIV_PROC` 过滤与两断言 | 约束不变量 | 存量 | 16 §2.1 | `schedule.c:33-36` | 只接管 INIT | 主讲述点 |
| K-07-04 | `sched_start`/`sched_inherit`/`sched_stop` 协议与参数 | 接口协议 | 存量 | 16 §2.7 | `minix/sched.h:7-12` | SCHED 契约 | 主讲述点 |
| K-07-05 | `sched_start_user`（fork 子进程继承；PRIV_PROC 父→INIT） | 机制 | 存量 | 16 §2.2 | `schedule.c:55-84` | fork 调度 | 主讲述点 |
| K-07-06 | `sched_nice` 与 `KERNEL/NONE→EINVAL` 守卫 | 机制 | 存量 | 16 §2.3 | `schedule.c:89-112` | nice 变更 | 主讲述点 |
| K-07-07 | `SCHEDULING_*` 操作码真值（BASE 0xF00 / NO_QUANTUM 0xF01 / START 0xF02 / STOP 0xF03 / SET_NICE 0xF04 / INHERIT 0xF05） | 数据结构 | 存量 | 16 §1.1/§2.7 | `com.h:801-807` | 协议常量 | **修正**：旧文写 START 0 / SET_NICE 5 |
| K-07-08 | `nice ↔ queue` 的 41/16 线性缩放与钳位 | 机制 | 存量 | 16 §1.2/§2.4 | `utility.c:91-103`；`main.c:275-289`；`config.h:66-74` | 双射公式 | **修正**：旧文举例 1→7→0 应为 1→8→2 |
| K-07-09 | `do_getsetpriority` 的五重判定 | 机制 | 存量 | 16 §2.6 | `misc.c:239-286` | 优先级调用 | 主讲述点 |
| K-07-10 | `SEND_PRIORITY`/`SEND_TIME_SLICE` 常量 | 数据结构 | 新增 | —（旧文未提） | `servers/pm/const.h:19-20` | 调度消息码 | 新增（缺口 G-04） |
| K-07-11 | Rust `NiceMapping`/`SchedCtl`/`may_*` | 架构演进 | 存量 | 16 §3 | `pm/src/sched.rs` | Rust 形态 | 主讲述点 |

### 2.9 新 08（进程创建：fork 与 srv_fork）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-08-01 | fork = 容器浅复制（地址空间委托 VM） | 概念 | 存量 | 07 §1.1 | `forkexit.c:44-139` | 语义模型 | 主讲述点 |
| K-08-02 | 三表分离与一致性窗口 | 概念 | 存量 | 07 §1.2 | `01`/`02-stage-vm`/`05-stage-vfs` | 多服务协同 | 主讲述点 |
| K-08-03 | 两阶段 fork（同步可失败 + 异步 SUSPEND） | 机制 | 存量 | 07 §1.3/§2.8 | `forkexit.c:56,78-82,139` | 核心设计轴 | 主讲述点 |
| K-08-04 | `vm_fork` 后不可失败窗口 | 约束不变量 | 存量 | 07 §1.3/§2.3 | `forkexit.c:82` 注释 | 不留孤儿 VM 状态 | 主讲述点 |
| K-08-05 | slot/PID/endpoint 三身份正交 | 数据结构 | 存量 | 07 §1.4 | `endpoint.h:45-69`；`utility.c:34-50` | 命名 vs 坐标 | 主讲述点 |
| K-08-06 | 延续挂在子进程的 `VFS_CALL` | 机制 | 存量 | 07 §1.5 | `forkexit.c:130-139` | UNIX 语义落差 | 主讲述点 |
| K-08-07 | 容量闸门与 `LAST_FEW` | 机制 | 存量 | 07 §2.1 | `forkexit.c:32,59-65` | EAGAIN 条件 | 主讲述点（与 03 互引） |
| K-08-08 | `next_child` 轮转与双 panic 守卫 | 机制 | 存量 | 07 §2.2 | `forkexit.c:51,68-75` | 槽位复用纪律 | 主讲述点 |
| K-08-09 | 整槽复制 + `mpsigact` 深拷贝 + tracer 条件继承 | 机制 | 存量 | 07 §2.4 | `forkexit.c:84-95` | 复制语义 | 主讲述点 |
| K-08-10 | 普通 fork 继承掩码（`IN_USE` + `DELAY_CALL` + `TAINTED`） | 约束不变量 | 存量 | 07 §2.4 | `forkexit.c:106` | 特权边界 | 主讲述点 |
| K-08-11 | PRIV_PROC 父 → 子挂 SCHED | 约束不变量 | 存量 | 07 §2.4 | `forkexit.c:100-103` | RS 派生普通子进程 | 主讲述点 |
| K-08-12 | `VFS_PM_FORK` payload 与 `tell_vfs` | 接口协议 | 存量 | 07 §2.6 | `forkexit.c:122-130`；`com.h:527,578-580` | wire 契约 | 主讲述点 |
| K-08-13 | fork 后半：`handle_vfs_reply` 的双回复与 `NEW_PARENT` 抑制 | 机制 | 存量 | 07 §2.6；05 §2.3 | `main.c:369-396` | 异步收尾 | 主讲述点 |
| K-08-14 | `do_srv_fork` 五差异（RS 门/PRIV 保留/六字段注入/真实 REUID/立即双回复） | 机制 | 存量 | 08 全篇 | `forkexit.c:145-239` | 服务孵化器 | 合并旧 07+08；九步只讲一次 |
| K-08-15 | `VFS_PM_SRV_FORK_REPLY` 空分支 | 接口协议 | 存量 | 08 §2.8/D8 | `main.c:398-401`；`com.h:541` | 回复面 | **修正**：旧文写 0x989，真值 0x988 |
| K-08-16 | Rust `Process::fork_from`/`ReplyLater`/`VfsCall::Fork` 类型化 | 架构演进 | 存量 | 07 §3；08 §3 | `mproc/fork.rs`；`ipc/vfs.rs` | Rust 形态 | 主讲述点 |
| K-08-17 | Rust `srv_fork_from` 的 PRIV_PROC 缺口（文档不得写"已保留"） | 架构演进 | 新增 | —（旧 08 声称已保留） | `mproc/fork.rs`（`Privilege::User` + 空旗标） | 如实标注实现差异 | 新增；修正旧 08 的伪称 |
| K-08-18 | `fork` 的 VFS 回复中 `sched_start_user` 调用点 | 机制 | 存量 | 07 §2.6；05 §2.3 | `main.c:372-374` | 调度接缝 | 引用 07 篇 |

### 2.10 新 09（终止与回收：exit 与 wait）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-09-01 | 两阶段退出（VFS_PM_EXIT 为分界） | 概念 | 存量 | 09 §1.1 | `forkexit.c:311-330,416-468` | 核心心智模型 | 主讲述点 |
| K-09-02 | 顺序敏感的理由（VFS 先取消驱动拷贝） | 概念 | 存量 | 09 §1.1 | `forkexit.c:311-330` 注释 | 为什么不能一步 | 主讲述点 |
| K-09-03 | PRIV_PROC 直毁与死锁避免 | 约束不变量 | 存量 | 09 §1.2/§2.3 | `forkexit.c:361-369` | 系统服务退出 | 主讲述点 |
| K-09-04 | 僵尸生产–消费模型 | 概念 | 存量 | 09 §1.3；10 §1.1 | `forkexit.c:593-624,670-725` | exit/wait 关系 | 主讲述点 |
| K-09-05 | `ZOMBIE`/`TRACE_ZOMBIE`/`TOLD_PARENT` 状态位 | 数据结构 | 存量 | 09 §1.3 | `mproc.h:88,92,101` | 状态编码 | 主讲述点 |
| K-09-06 | 收养 + `NEW_PARENT` 记忆 | 机制 | 存量 | 09 §1.4/§2.4 | `forkexit.c:394-408` | 孤儿处理 | 主讲述点 |
| K-09-07 | session leader 死亡的 `SIGHUP` 广播 | 机制 | 存量 | 09 §1.4/§2.4 | `forkexit.c:298,412`；`signal.c:568` | 会话语义 | 主讲述点 |
| K-09-08 | 事件在两阶段之间发布 | 机制 | 存量 | 09 §1.5 | `main.c:355-366` | 09/06 接缝 | 主讲述点 |
| K-09-09 | `do_exit` 的 PRIV_PROC→SIGKILL 门与 SUSPEND | 机制 | 存量 | 09 §2.1 | `forkexit.c:245-262` | 系统服务不许 exit | 主讲述点 |
| K-09-10 | core dump 双抑制（setuid/PRIV） | 约束不变量 | 存量 | 09 §2.2 | `forkexit.c:285-292` | 安全 | 主讲述点 |
| K-09-11 | `procgrp` 记忆 / `ALARM_ON` 取消 / `sys_times` 记账 | 机制 | 存量 | 09 §2.2 | `forkexit.c:294-309` | 收尾前记账 | 主讲述点 |
| K-09-12 | 强制停止 + `vm_willexit`；INIT/VFS 特例 | 机制 | 存量 | 09 §2.3 | `forkexit.c:326-345` | 内核解绑 | 主讲述点 |
| K-09-13 | `VFS_PM_EXIT`/`VFS_PM_DUMPCORE` payload | 接口协议 | 存量 | 09 §2.3 | `forkexit.c:350-359`；`com.h:524-525` | wire 契约 | 主讲述点 |
| K-09-14 | 旗标收窄五保留位 + `EXITING` | 约束不变量 | 存量 | 09 §2.4 | `forkexit.c:374-375` | 状态清理规则 | 主讲述点 |
| K-09-15 | `exit_restart` 五步 | 机制 | 存量 | 09 §2.5 | `forkexit.c:418-469` | 第二阶段 | 主讲述点 |
| K-09-16 | `zombify`/`check_parent` 两级僵尸 | 机制 | 存量 | 09 §2.6；10 §2.6 | `forkexit.c:593-665` | 通知引擎 | 主讲述点（两篇合一） |
| K-09-17 | `tracer_died` 三分支 | 机制 | 存量 | 09 §2.6 | `forkexit.c:759-790` | 调试器崩溃 | 主讲述点 |
| K-09-18 | `cleanup` 释放槽 | 机制 | 存量 | 09 §2.6；10 §2.9 | `forkexit.c:795-806` | 槽位回收 | 修正旧文符号误标 |
| K-09-19 | `do_wait4` 的 `pidarg` 四态归一 | 接口协议 | 存量 | 10 §1.2/§2.1 | `forkexit.c:490-493` | wait 语义 | 主讲述点 |
| K-09-20 | 主扫描三重过滤 | 机制 | 存量 | 10 §2.2 | `forkexit.c:501-510` | 目标选择 | 主讲述点 |
| K-09-21 | 三环优先级（TRACE_ZOMBIE→TRACE_STOPPED→ZOMBIE） | 机制 | 存量 | 10 §2.3 | `forkexit.c:512-548` | 消费顺序 | 主讲述点 |
| K-09-22 | `wait_test` 双条件 | 机制 | 存量 | 10 §2.5 | `forkexit.c:569-588` | 唤醒条件 | 主讲述点 |
| K-09-23 | `tell_parent` 的 rusage 拷贝 / `W_EXITCODE` / 状态迁移 | 机制 | 存量 | 10 §2.6 | `forkexit.c:670-725` | 消费动作 | 主讲述点 |
| K-09-24 | `tell_tracer` 的伪父转换 | 机制 | 存量 | 10 §2.7 | `forkexit.c:731-754` | ptrace wait | 主讲述点 |
| K-09-25 | `WNOHANG`/`ECHILD` 同步返回 | 接口协议 | 存量 | 10 §2.4 | `forkexit.c:551-563` | 非阻塞语义 | 主讲述点 |
| K-09-26 | `set_rusage_times`（ticks×1e6/hz） | 机制 | 存量 | 10 §2.8；20 §2.11 | `utility.c:144-156` | 换算 | 归 09；20 引用 |
| K-09-27 | Rust `Lifecycle`/`Guardianship`/`WaitState`/`WaitScanner` | 架构演进 | 存量 | 09 §3；10 §3 | `mproc/{lifecycle,guardianship,wait}.rs`；`wait.rs` | Rust 形态 | 主讲述点 |

### 2.11 新 10（信号模型：生成、判定与处置）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-10-01 | 信号的两种来源（用户 kill / 内核 ksig）与 PM 唯一权威 | 概念 | 存量 | 11 §1.1 | `signal.c:197-221,294-378` | 入口模型 | 主讲述点 |
| K-10-02 | `pid` 四态选择（>0/0→组/-1/<-1） | 机制 | 存量 | 11 §1.2/§2.3 | `signal.c:601-604` | 目标集合 | 主讲述点 |
| K-10-03 | 逆序全表扫描与系统进程靠后分析 | 机制 | 存量 | 11 §2.3 | `signal.c:597` | 顺序理由 | 主讲述点 |
| K-10-04 | 广播 SIGTERM 先杀 RS | 机制 | 存量 | 11 §1.2/§2.2 | `signal.c:588-589` | 关机定序 | 主讲述点 |
| K-10-05 | `INIT_PID + SIGKILL → EINVAL` | 约束不变量 | 存量 | 11 §2.2 | `signal.c:585`；`const.h:9` | INIT 不死 | 主讲述点 |
| K-10-06 | 权限四重（effuid/realuid 交叉匹配） | 约束不变量 | 存量 | 11 §1.3/§2.3 | `signal.c:622-628` | 权限语义 | 主讲述点 |
| K-10-07 | `PRIV_PROC` 致命信号门与广播跳过 | 约束不变量 | 存量 | 11 §1.3/§2.3 | `signal.c:607-608,616-618` | 系统服务保护 | 主讲述点 |
| K-10-08 | VM 永远跳过（信号管理器页错误死锁） | 约束不变量 | 存量 | 11 §1.3 | `signal.c:613`；`com.h:67` | 死锁规避 | **修正**：旧文 `VM_PROC_NR` 锚到 61 |
| K-10-09 | `count`/`error_code` 返回语义与自杀 SUSPEND | 接口协议 | 存量 | 11 §2.3 | `signal.c:595-596,631,644-645` | 返回值契约 | 主讲述点 |
| K-10-10 | `sig_proc` 十条处置分支的优先级 | 机制 | 存量 | 11 §1.4/§2.4 | `signal.c:411-539` | 核心判定链 | 主讲述点；修正"9 链"为 10 分支 |
| K-10-11 | tracer-first（`mp_sigtrace` + `trace_stop`） | 约束不变量 | 存量 | 11 §2.4 | `signal.c:411-422` | 调试优先 | 主讲述点 |
| K-10-12 | `VFS_CALL` 或 `EVENT_CALL` 暂存 + `stop_proc` | 机制 | 存量 | 11 §2.4 | `signal.c:425-444` | 不丢信号 | 主讲述点 |
| K-10-13 | PRIV_PROC 系统信号三分支（跳过 PM / 非 ksig 回环 / ksig 栈回溯·消息·退出） | 机制 | 存量 | 11 §2.4 | `signal.c:448-480` | 系统进程语义 | 主讲述点 |
| K-10-14 | `badignore` 与 `noign_sset` | 约束不变量 | 存量 | 11 §1.4/§2.4 | `signal.c:483-485`；`main.c:140-141` | 不可忽略集合 | 主讲述点 |
| K-10-15 | 默认忽略集合 `ign_sset` | 数据结构 | 存量 | 11 §2.4 | `signal.c:533-535`；`main.c:139` | SIGCHLD 语义 | 主讲述点 |
| K-10-16 | `TRACE_STOPPED` 暂存（除 SIGKILL） | 机制 | 存量 | 11 §2.4 | `signal.c:499-507` | 调试不扰动 | 主讲述点 |
| K-10-17 | caught 路径：`unpause` → `sig_send` → 失败则杀 | 机制 | 存量 | 11 §2.4 | `signal.c:509-531` | 投递兜底 | 主讲述点；细节归 11 篇 |
| K-10-18 | `sig_proc_exit` 与 core 集合 | 机制 | 存量 | 11 §2.5 | `signal.c:546-563`；`main.c:137-138` | 信号致死 | 主讲述点 |
| K-10-19 | `SIGS_IS_LETHAL`/`TERMINATION`/`STACKTRACE` 真集合 | 数据结构 | 存量 | 11 §1.6；06 §2.6 | `sys/sys/signal.h:281-286` | 信号分类 | **修正**：旧 11 把 KILL 算进 LETHAL、说与 core 正交 |
| K-10-20 | `process_ksig` 双 `EDEADEPT` 检查与 `mproc[0]` 伪装 | 机制 | 存量 | 11 §1.5/§2.6 | `signal.c:300-313,335` | 内核信号入口 | 主讲述点 |
| K-10-21 | 内核 signo→id 映射（组播/精确/vtimer） | 机制 | 存量 | 11 §2.6 | `signal.c:320-332` | 来源语义 | 主讲述点 |
| K-10-22 | `SIGSNDELAY` 尾部兑现 `DELAY_CALL` | 机制 | 存量 | 11 §2.6；13 §2.6 | `signal.c:344-369` | 延迟停止 | 主讲述点；修正"kill 也能产生" |
| K-10-23 | `do_kill` vs `do_srv_kill`（ksig 真假与 RS 门） | 接口协议 | 存量 | 11 §2.1 | `signal.c:197-221` | 两条入口 | 主讲述点 |
| K-10-24 | 系统进程的 `SIGS_SIGNAL_RECEIVED` 消息化 | 机制 | 存量 | 11 §2.4/§2.7 | `signal.c:468-474`；`com.h:601` | 系统信号投递 | 修正锚点 597→601 |
| K-10-25 | 内核信号回环（SEF `process_sigmgr_signals` 的 getksig/endksig 循环） | 机制 | 新增 | 11 §4.1（Rust 侧一笔） | `lib/libsys/sef_signal.c`；`sys/sys/signal.h:273-274` | 回答"SIGKSIG 之后发生什么" | 新增（缺口 G-03）；PM 侧只留回调点 |
| K-10-26 | Rust `SignalTarget`/`SignalClass`/`can_signal` | 架构演进 | 存量 | 11 §3 | `pm/src/signal.rs` | Rust 形态 | 主讲述点 |

### 2.12 新 11（信号安装、投递与恢复）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-11-01 | 安装是处置的生产者 | 概念 | 存量 | 12 §1.1 | `signal.c:67-78` vs `411-539` | 契约闭环 | 主讲述点 |
| K-11-02 | 三态不对称（IGN 清 pending；DFL 保留；catch 清 ignore） | 约束不变量 | 存量 | 12 §1.2 | `signal.c:67-78` | 安装语义 | 主讲述点 |
| K-11-03 | `sa_mask` 剥离 KILL/STOP | 约束不变量 | 存量 | 12 §1.2 | `signal.c:80-81` | 不可屏蔽 | 主讲述点 |
| K-11-04 | SIGKILL 早退（无法重装） | 约束不变量 | 存量 | 12 §1.3 | `signal.c:49` | 内核保证 | 主讲述点；SIGSTOP 的可忽略是 C 的宽松点，如实写 |
| K-11-05 | 四种 `how` 与 BLOCK/UNBLOCK 的非对称剥离 | 接口协议 | 存量 | 12 §1.4 | `signal.c:122-153` | POSIX 掩码 | 主讲述点 |
| K-11-06 | `check_pending` 的触发点（UNBLOCK/SETMASK/sigreturn/sigsuspend） | 机制 | 存量 | 12 §1.4 | `signal.c:137,144,169,190` | 补投时机 | 主讲述点 |
| K-11-07 | `SIG_INQUIRE` 只读查询 | 接口协议 | 存量 | 12 §1.4 | `signal.c:147-148` | 查询语义 | 主讲述点 |
| K-11-08 | `sigsuspend` 的 `mask2` 保存与原子等待 | 机制 | 存量 | 12 §1.5 | `signal.c:164-170` | 竞态规避 | 主讲述点 |
| K-11-09 | `sigreturn` 顺序（恢复掩码→内核恢复→补投） | 机制 | 存量 | 12 §2.5 | `signal.c:185-191` | 上下文恢复 | 主讲述点 |
| K-11-10 | `sigaction` 的 oact/act 双 NULL 协议与双向 datacopy | 接口协议 | 存量 | 12 §2.1 | `signal.c:53-65` | 读写协议 | 主讲述点 |
| K-11-11 | `sigmsg` 五字段与 `sm_mask` 来源（`mask2` 或当前掩码） | 数据结构 | 存量 | 12 §1.6/§2.6 | `signal.c:792-799`；`type.h:71-77` | 栈帧载荷 | **修正**：旧文写 4 字段 |
| K-11-12 | `sa_mask` 并入的是 `mp_sigmask`（不是 `sigmsg.sm_mask`） | 机制 | 存量 | 12 §1.6/§2.6 | `signal.c:800-803` | 处理器期间阻塞 | **修正**：旧文代码块写错目标 |
| K-11-13 | `SA_NODEFER` / `SA_RESETHAND` | 机制 | 存量 | 12 §1.6/§2.6 | `signal.c:805-813` | 两位旗标 | 主讲述点 |
| K-11-14 | `sys_sigsend` 错误分类（EFAULT/ENOMEM → 杀；其他 panic） | 接口协议 | 存量 | 12 §1.6/§2.6 | `signal.c:818-829` | 失败语义 | 主讲述点 |
| K-11-15 | `WAITING` 或 `SIGSUSPENDED` → `EINTR` + `try_resume` | 机制 | 存量 | 12 §2.6 | `signal.c:832-844` | 中断阻塞 | 主讲述点 |
| K-11-16 | 否则断言 `UNPAUSED`（VFS 确认路径） | 约束不变量 | 存量 | 12 §2.6 | `signal.c:845-851` | 路径纪律 | 主讲述点 |
| K-11-17 | `stop_proc` 的 `may_delay` 契约与 `sys_delay_stop` 返回 | 接口协议 | 存量 | 13 §1.1/§2.1 | `signal.c:226-261` | 停止原语 | 主讲述点 |
| K-11-18 | `EBUSY ↔ SIGSNDELAY` 配对 | 机制 | 存量 | 13 §1.1 | `signal.c:239-242,344` | 中途发送的停止 | 主讲述点 |
| K-11-19 | `PROC_STOPPED` 双用途（已停止/需重检） | 概念 | 存量 | 13 §1.2 | `signal.c:430-434` | 状态复用 | 主讲述点 |
| K-11-20 | `unpause` 三路径（已解/正忙/PM 睡或 VFS 睡） | 机制 | 存量 | 13 §1.4/§2.5 | `signal.c:719-770` | 解除阻塞 | 主讲述点 |
| K-11-21 | `VFS_PM_UNPAUSE` 往返与 `UNPAUSED` 瞬态 | 接口协议 | 存量 | 13 §2.5 | `signal.c:763-767`；`com.h:528,541` | 异步中断 | **修正**：旧文把请求锚到 `DS_RQ_BASE` |
| K-11-22 | `check_pending` 的 break 语义 | 机制 | 存量 | 13 §1.5/§2.3 | `signal.c:664-679` | 有界重投 | 主讲述点 |
| K-11-23 | `restart_sigs` 的 `TRACE_EXIT` 优先 | 约束不变量 | 存量 | 13 §1.6/§2.4 | `signal.c:693-712` | 调试退出优先 | 主讲述点 |
| K-11-24 | `try_resume_proc` 守卫与 panic 契约 | 机制 | 存量 | 13 §2.2 | `signal.c:266-289` | 恢复原语 | 主讲述点 |
| K-11-25 | `SIGSNDELAY` 不可经 kill 产生（70 ≥ `_NSIG`=64） | 约束不变量 | 存量 | 13 §2.6 | `signal.c:582`；`signal.h:45,264` | 防止误解 | **修正**：旧 13 说 kill 也会产生 |
| K-11-26 | Rust `MayDelay`/`UnpauseOutcome`/`RestartAction`/`KernelStop` | 架构演进 | 存量 | 13 §3；12 §3 | `pm/src/signal_flow.rs`；`signal_handlers.rs` | Rust 形态 | 主讲述点 |

### 2.13 新 12（定时器）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-12-01 | ITIMER_REAL/VIRTUAL/PROF 三族分野 | 概念 | 存量 | 14 §1.1 | `sys/sys/time.h:264-266`；`const.h:17` | 时钟语义 | 主讲述点 |
| K-12-02 | REAL 走 PM 本地 timer（`mp_timer` + `ALARM_ON`） | 机制 | 存量 | 14 §1.3 | `alarm.c:299-343` | 本地定时器 | 主讲述点 |
| K-12-03 | VIRTUAL/PROF 走内核 `sys_vtimer` | 接口协议 | 存量 | 14 §1.3 | `alarm.c:205-206` | 内核委派 | 主讲述点 |
| K-12-04 | `ticks_from_timeval` 向上取整 | 机制 | 存量 | 14 §1.2/§2.1 | `alarm.c:53-60` | 1us 不归零 | 主讲述点 |
| K-12-05 | 乘法溢出的 div 校验与 `LONG_MAX` 钳位 | 约束不变量 | 存量 | 14 §1.2/§2.1 | `alarm.c:55-62` | 防溢出 | 主讲述点 |
| K-12-06 | `timeval_from_ticks` 先模后乘 | 机制 | 存量 | 14 §2.2 | `alarm.c:74-75` | 分解 | 主讲述点 |
| K-12-07 | `is_sane_timeval` 边界 | 约束不变量 | 存量 | 14 §2.3 | `alarm.c:85-86`；`const.h:15` | 输入校验 | 主讲述点 |
| K-12-08 | `do_itimer` 的 which/setval/getval 规则 | 接口协议 | 存量 | 14 §1.4/§2.4 | `alarm.c:100-110` | 入口协议 | 主讲述点 |
| K-12-09 | 双向 `sys_datacopy`（新值进/旧值出） | 机制 | 存量 | 14 §2.4 | `alarm.c:116-118,148-150` | 跨空间拷贝 | 主讲述点 |
| K-12-10 | `getset_vtimer` 的 NULL 指针语义与旧值回绕 | 机制 | 存量 | 14 §1.5/§2.5 | `alarm.c:169-215` | get/set 合一 | 主讲述点 |
| K-12-11 | 取消即零 interval | 约束不变量 | 存量 | 14 §2.5/§2.8 | `alarm.c:188-189,289` | 周期语义 | 主讲述点 |
| K-12-12 | `check_vtimer` 到期重设 | 机制 | 存量 | 14 §2.6 | `alarm.c:222-241` | 周期虚拟定时器 | 主讲述点 |
| K-12-13 | `get/set_realtimer` 的剩余时间计算 | 机制 | 存量 | 14 §2.7/§2.8 | `alarm.c:247-294` | 读旧值 | 主讲述点 |
| K-12-14 | `set_alarm` 的置位/取消 | 机制 | 存量 | 14 §2.9 | `alarm.c:299-311` | ALARM_ON 唯一真源 | 主讲述点 |
| K-12-15 | `cause_sigalrm` 三重守卫 + interval 重设 | 机制 | 存量 | 14 §1.6/§2.10 | `alarm.c:317-343` | 到期回调 | 主讲述点 |
| K-12-16 | `MAX_SECS = TMRDIFF_MAX/system_hz` | 数据结构 | 存量 | 14 §1.2/§2.3 | `servers/pm/const.h:15`；`minix/timers.h:45` | 范围上限 | **修正**：旧文写 100M / 位置错；Rust 硬编码 100_000_000 也是错的 |
| K-12-17 | `TMRDIFF_MAX` 断言与内核队列边界 | 约束不变量 | 存量 | 14 §2.9 | `alarm.c:304` | 队列范围 | 主讲述点 |
| K-12-18 | CLOCK notify 驱动 `expire_timers` | 机制 | 存量 | 14 §2.11 | `main.c:65-67` | 时钟心跳 | 主讲述点 |
| K-12-19 | Rust `TicksConv`/`AlarmState`/`VTimerCtl` | 架构演进 | 存量 | 14 §3 | `pm/src/timer.rs`；`mproc/mproc.rs` | Rust 形态 | 主讲述点 |

### 2.14 新 13（凭证与身份变更）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-13-01 | real/effective/saved 三元组 | 概念 | 存量 | 15 §1.1 | `mproc.h:41-46` | 审计/切换/回退 | 主讲述点 |
| K-13-02 | SETUID 的 BSD 全置语义 | 约束不变量 | 存量 | 15 §1.2/§2.9 | `getset.c:111-119` | 原子三元组 | 主讲述点 |
| K-13-03 | SETEUID 的三重校验 | 约束不变量 | 存量 | 15 §2.10 | `getset.c:128-134` | 回退语义 | 主讲述点 |
| K-13-04 | gid 判定统一用 effuid | 约束不变量 | 存量 | 15 §2.11 | `getset.c:145,160` | 权限谓词 | 主讲述点 |
| K-13-05 | set 成功必须同步 VFS（双副本一致） | 机制 | 存量 | 15 §1.3/§2.14 | `getset.c:219-222`；`main.c:334-347` | fproc 一致 | 主讲述点；**修正** SETSID 回复值 |
| K-13-06 | GETGROUPS 的 0 查询二阶段 | 接口协议 | 存量 | 15 §1.4/§2.2 | `getset.c:29-49` | 先问再拷 | 主讲述点 |
| K-13-07 | 组数上限与不足即 EINVAL（不截断） | 约束不变量 | 存量 | 15 §2.2 | `getset.c:31-41`；`syslimits.h:59` | 组语义 | 主讲述点 |
| K-13-08 | `GID_MAX` 真值与逐元素校验 | 约束不变量 | 存量 | 15 §2.12 | `sys/sys/syslimits.h:53`（2147483647U） | id 合法性 | **修正**：旧文写 `sys/limits.h`/0xFFFFFFFF |
| K-13-09 | SETGROUPS 的 root 门与空指针 EFAULT | 约束不变量 | 存量 | 15 §2.12 | `getset.c:173-182` | 权限 | 主讲述点 |
| K-13-10 | 缩容时尾部清零 | 机制 | 存量 | 15 §2.12 | `getset.c:194-196` | 无残留 | 主讲述点 |
| K-13-11 | `setsid` 的组长 EPERM | 约束不变量 | 存量 | 15 §1.5/§2.13 | `getset.c:206-207` | 会话首语义 | 主讲述点 |
| K-13-12 | `GETSID` 自取/`find_proc` ESRCH | 机制 | 存量 | 15 §2.6 | `getset.c:70-78` | 查询语义 | 主讲述点 |
| K-13-13 | `TAINTED`/`issetugid` 防注入 | 机制 | 存量 | 15 §1.6/§2.7 | `getset.c:80-81`；`exec.c:100-109` | 安全位 | 主讲述点；exec 侧归 14 |
| K-13-14 | `GETUID`/`GETGID` 的 real 返回 + eff 填 reply | 接口协议 | 存量 | 15 §2.3 | `getset.c:51-58` | 双通道 ABI | 主讲述点 |
| K-13-15 | 13 个 get/set 调用的完整枚举 | 数据结构 | 存量 | 15 §2.15 | `getset.c:19-222`；`callnr.h:19-46` | 调用清单 | **修正**：旧文"13 个"但只列 11 个 |
| K-13-16 | Rust `GetOp`/`SetOp`/`Credentials`/`CopyGroups` | 架构演进 | 存量 | 15 §3 | `pm/src/credentials.rs`；`mproc/credentials.rs` | Rust 形态 | 主讲述点 |

### 2.15 新 14（exec）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-14-01 | VFS 判权限、PM 换凭证的分工 | 概念 | 存量 | 17 §1.1 | `exec.c:38-56,62-125` | 存在理由 | 主讲述点 |
| K-14-02 | `do_exec` 六字段 `VFS_PM_EXEC` 转发 + `SUSPEND` | 接口协议 | 存量 | 17 §2.1 | `exec.c:44-55` | 请求 wire | 主讲述点 |
| K-14-03 | VFS/RS 才可调 `PM_EXEC_NEW` | 约束不变量 | 存量 | 17 §2.2 | `exec.c:70-71` | 调用门 | 主讲述点 |
| K-14-04 | `exec_info` 的 `sys_datacopy` 取入 | 机制 | 存量 | 17 §2.2 | `exec.c:79` | 参数获取 | **修正**：定义在 `lib/libexec/libexec.h` 而非 `minix/vm.h` |
| K-14-05 | `allow_setuid` 判定（无 tracer 且 VFS 允许） | 机制 | 存量 | 17 §2.3 | `exec.c:83-94` | 提权抑制 | 主讲述点 |
| K-14-06 | `svuid/svgid = eff` 与 creds 更新 | 机制 | 存量 | 17 §2.3 | `exec.c:91-98` | 凭证原子性 | 主讲述点 |
| K-14-07 | `TAINTED` 二重（setuid 位或 eff≠real） | 约束不变量 | 存量 | 17 §1.2/§2.4 | `exec.c:100-109` | 安全位 | 主讲述点 |
| K-14-08 | `mp_name`/`mp_frame_addr/len` 保存 | 数据结构 | 存量 | 17 §1.6/§2.5 | `exec.c:111-117` | ps/procfs | 主讲述点 |
| K-14-09 | `PARTIAL_EXEC` 哨兵与半态 | 约束不变量 | 存量 | 17 §1.3/§2.5 | `exec.c:120`；`mproc.h:99` | 半初始化标记 | 主讲述点 |
| K-14-10 | suid 标志回给 VFS | 接口协议 | 存量 | 17 §2.5 | `exec.c:122` | 权限确认 | 主讲述点 |
| K-14-11 | `do_execrestart` 的 RS 专用路径 | 机制 | 存量 | 17 §2.6 | `exec.c:130-151` | 服务重启装载 | 主讲述点 |
| K-14-12 | 失败路径：`PARTIAL_EXEC` → `SIGKILL`，否则回错误 | 约束不变量 | 存量 | 17 §2.7 | `exec.c:161-170` | 半态自毁 | 主讲述点 |
| K-14-13 | catch 重置 / ignore 保留 | 约束不变量 | 存量 | 17 §1.4/§2.8 | `exec.c:178-184` | POSIX 语义 | 主讲述点 |
| K-14-14 | tracer 的 `SIGTRAP/SIGSTOP` 时序 | 机制 | 存量 | 17 §1.5/§2.8 | `exec.c:189-194` | 调试器断点 | 主讲述点 |
| K-14-15 | `sys_exec(sp, pc, name, ps_str)` 四元 | 接口协议 | 存量 | 17 §2.8 | `exec.c:197` | 内核交接 | 主讲述点 |
| K-14-16 | `TO_NOEXEC=0x4`/`TO_ALTEXEC=0x2`/`TO_TRACEFORK=0x1` | 数据结构 | 存量 | 17 §1.5/§2.9 | `sys/sys/ptrace.h:209-211` | 旗标真值 | **修正**：旧 17 写 NOEXEC 0x1 |
| K-14-17 | Rust `ExecState`/`ExecCreds`/`FrameRegion`/`KernelExec` | 架构演进 | 存量 | 17 §3 | `pm/src/exec.rs`；`mproc/mproc.rs` | Rust 形态 | 主讲述点 |

### 2.16 新 15（ptrace）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-15-01 | 双入口（子进程 T_OK 自声明 vs 调试器 T_ATTACH） | 概念 | 存量 | 18 §1.1 | `trace.c:50-60,62-93` | ptrace 模型 | 主讲述点 |
| K-15-02 | T_OK 的 EBUSY 与 `tracer=parent` | 机制 | 存量 | 18 §2.2 | `trace.c:55-60` | 自声明 | 主讲述点 |
| K-15-03 | T_ATTACH 七重守卫（存在/EXITING/id 匹配/PRIV 双向/self·PM·VM/已跟踪） | 机制 | 存量 | 18 §2.3 | `trace.c:62-93` | attach 策略 | 主讲述点 |
| K-15-04 | `TO_NOEXEC` 于 attach 时置位 | 机制 | 存量 | 18 §2.3 | `trace.c:88` | exec 信号策略 | 主讲述点 |
| K-15-05 | attach 即 `sig_proc(SIGSTOP)` | 机制 | 存量 | 18 §2.3 | `trace.c:90` | 目标暂停 | 主讲述点 |
| K-15-06 | T_STOP 不暴露（EINVAL） | 约束不变量 | 存量 | 18 §2.4 | `trace.c:95-99` | 接口边界 | 主讲述点 |
| K-15-07 | T_READB_INS/T_WRITEB_INS（仅 root + 透传） | 机制 | 存量 | 18 §2.5 | `trace.c:101-134` | 文本段补丁 | 主讲述点 |
| K-15-08 | 后半命令共同守卫（tracer 匹配 + TRACE_STOPPED） | 约束不变量 | 存量 | 18 §2.6 | `trace.c:140-143` | 状态纪律 | 主讲述点 |
| K-15-09 | T_EXIT 的 `TRACE_EXIT` 与延后退出 | 机制 | 存量 | 18 §2.7 | `trace.c:146-159` | 调试退出 | 主讲述点 |
| K-15-10 | T_SETOPT 整字写 `mp_trace_flags` | 机制 | 存量 | 18 §2.8 | `trace.c:161-165` | 选项存储 | 主讲述点 |
| K-15-11 | T_GETRANGE/SETRANGE 的 `TS_INS/DATA` 与长度校验 | 机制 | 存量 | 18 §2.9 | `trace.c:167-188` | 范围读写 | 主讲述点 |
| K-15-12 | T_DETACH 的三步（重放 sigtrace / 可选投信号 / 清状态 + `check_pending`） | 机制 | 存量 | 18 §2.10 | `trace.c:190-215` | 脱离语义 | 主讲述点 |
| K-15-13 | T_RESUME/STEP/SYSCALL 的 sigtrace 短路 | 机制 | 存量 | 18 §2.11 | `trace.c:217-242` | 假成功 | 主讲述点 |
| K-15-14 | `sys_trace` 透传尾部 | 接口协议 | 存量 | 18 §2.12 | `trace.c:244-249` | 内核完成 | 主讲述点 |
| K-15-15 | `trace_stop` 的顺序（内核停→置位→可选回 wait） | 机制 | 存量 | 18 §2.13 | `trace.c:255-276` | 停止顺序 | 主讲述点 |
| K-15-16 | `TRACE_STOPPED` 与 `PROC_STOPPED` 双轨 | 概念 | 存量 | 18 §1.2 | `mproc.h:89,93` | 两种暂停 | 主讲述点 |
| K-15-17 | `mp_sigtrace` 作 tracer 的 pending 缓冲 | 数据结构 | 存量 | 18 §1.3 | `signal.c:417` | 信号缓冲 | 主讲述点 |
| K-15-18 | T_* / TO_* 常量全表 | 数据结构 | 存量 | 18 §2.14 | `sys/sys/ptrace.h:226-250,209-211` | ABI | 归 99；本文引用 |
| K-15-19 | Rust `PtraceReq`/`TraceState`/`replay_sigtrace` | 架构演进 | 存量 | 18 §3 | `pm/src/trace.rs`；`mproc/trace.rs` | Rust 形态 | 主讲述点 |

### 2.17 新 16（时间）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-16-01 | REALTIME vs MONOTONIC 双时钟 | 概念 | 存量 | 19 §1.1 | `sys/sys/time.h:283,288`；`time.c:31-39` | wall vs 滴答 | 主讲述点 |
| K-16-02 | MONOTONIC 不可设置 | 约束不变量 | 存量 | 19 §2.3 | `time.c:84-86` | EINVAL 边界 | 主讲述点 |
| K-16-03 | `getuptime` 三值读取 | 接口协议 | 存量 | 19 §2.6 | `lib/libsys/getuptime.c:9-24` | 数据来源 | 主讲述点 |
| K-16-04 | `sec = boottime + clock/hz` | 机制 | 存量 | 19 §2.1 | `time.c:42` | 组合 | 主讲述点 |
| K-16-05 | `nsec = (clock%hz)*1e9/hz` 先模后乘 | 机制 | 存量 | 19 §1.2 | `time.c:44` | 无溢出分解 | 主讲述点 |
| K-16-06 | `getres` 的 0/1e9/hz | 机制 | 存量 | 19 §1.3/§2.2 | `time.c:58-60` | 分辨率 | 主讲述点 |
| K-16-07 | `settime` 的 root 门与 now 分支 | 接口协议 | 存量 | 19 §1.5/§2.3 | `time.c:75-83` | 渐变 vs 跳变 | 主讲述点 |
| K-16-08 | `do_time` 直接 `clock_time` | 机制 | 存量 | 19 §2.4 | `time.c:94-104` | 简化路径 | 主讲述点 |
| K-16-09 | `stime` 的 boottime 重定 | 机制 | 存量 | 19 §1.4/§2.5 | `time.c:119-128` | 锚点 | 主讲述点 |
| K-16-10 | `clock_time` 的 32 位规避算法 | 机制 | 存量 | 19 §2.7 | `lib/libsys/clock_time.c:13-40` | 溢出规避 | 主讲述点 |
| K-16-11 | hz 上限与 `LONG_MAX/40000` 分界 | 约束不变量 | 存量 | 19 §1.2/§2.7 | `clock_time.c:33` | 边界 | **修正**：旧文"hz=5e4 退化"结论错误 |
| K-16-12 | 五调用分派表（5 个 switch/直通） | 工具工程 | 存量 | 19 §2.1-2.5 | `time.c:22-131` | 调用地图 | 主讲述点 |
| K-16-13 | Rust `ClockId`/`decompose_clock`/`ClockSource` | 架构演进 | 存量 | 19 §3 | `pm/src/time.rs`；`minix-types/types/clock.rs` | Rust 形态 | 主讲述点 |

### 2.18 新 17（杂项与查询）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-17-01 | `uts_tbl` 兼容间接（9 槽、3 个 NULL） | 机制 | 存量 | 20 §1.1/§2.1 | `misc.c:32-60,79-83` | 字段表 | **修正**：旧文写 8 槽/4 NULL |
| K-17-02 | uname 截断 vs EINVAL 规则 | 接口协议 | 存量 | 20 §2.1 | `misc.c:85-99` | 拷贝语义 | 主讲述点 |
| K-17-03 | `getsysinfo` 的表导出与 root 门 | 机制 | 存量 | 20 §1.2/§2.2 | `misc.c:107-144`；`sysinfo.h:11` | mproc 导出 | **修正**：`SI_PROC_TAB=2`（旧文写 0） |
| K-17-04 | size 精确匹配（EINVAL） | 约束不变量 | 存量 | 20 §2.2 | `misc.c:139-140` | 表 ABI | 主讲述点 |
| K-17-05 | `SI_CALL_STATS` 与 feature 门 | 工具工程 | 存量 | 20 §1.2/§2.2 | `misc.c:129-133`；`sysinfo.h:14` | 统计面 | **修正**：值 9；归 99 排除项 |
| K-17-06 | `getprocnr` 仅 RS | 约束不变量 | 存量 | 20 §2.3 | `misc.c:149-164` | RS 专用解析 | 主讲述点 |
| K-17-07 | `getepinfo` 四 id + 组列表截断 | 机制 | 存量 | 20 §2.4 | `misc.c:169-193` | 身份解析 | 主讲述点 |
| K-17-08 | `reboot` 定序（kill → stop INIT → VFS → abort） | 机制 | 存量 | 20 §1.4/§2.5 | `misc.c:198-233`；`main.c:304-312` | 关机 | 主讲述点 |
| K-17-09 | `RB_POWERDOWN` 通知 readclock | 机制 | 存量 | 20 §2.5 | `misc.c:210-215`；`sys/sys/reboot.h:54` | 断电 | **修正**：值 0x808（旧文写 1） |
| K-17-10 | `svrctl` 的本地覆盖与三级查找 | 机制 | 存量 | 20 §1.5/§2.6 | `misc.c:291-395` | 参数访问 | 主讲述点 |
| K-17-11 | `PMGETPARAM` 的 E2BIG/ESRCH | 接口协议 | 存量 | 20 §2.6 | `misc.c:375,380` | 错误语义 | 主讲述点 |
| K-17-12 | `getrusage` 双源 + VM 补充 + 拷出 | 机制 | 存量 | 20 §1.6/§2.7 | `misc.c:400-447` | 资源统计 | 主讲述点 |
| K-17-13 | `sprofile` 的 feature 语义 | 工具工程 | 存量 | 20 §2.8 | `profile.c:22-45` | ENOSYS 边界 | 归 99 排除项 |
| K-17-14 | `mcontext` 直通 | 接口协议 | 存量 | 20 §2.9 | `mcontext.c:15-25` | 架构上下文 | 主讲述点 |
| K-17-15 | `find_param` KVP 扫描 | 机制 | 存量 | 20 §2.10 | `utility.c:57-71` | 启动参数 | 主讲述点 |
| K-17-16 | Rust `misc.rs` 的两个常量 bug（`SI_PROC_TAB=0`、`RB_POWERDOWN=1`） | 架构演进 | 新增 | —（旧文未发现） | `os/servers/pm/src/misc.rs` | 通电前必修项 | 新增（缺口 G-11）；B 相修复或显式标注 |
| K-17-17 | 杂项调用号与 wire 对应表 | 数据结构 | 存量 | 20 §2.12 | `callnr.h:25,32,37-39,45-47,49-52`；`ipc.h` | 调用地图 | 数值归 99 |

### 2.19 新 18（外部接口契约）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-18-01 | 单一签名权威面的理由 | 概念 | 新增 | —（旧目录签名散落） | 全篇设计 | 防止十几篇重复 | 新增 |
| K-18-02 | 依赖面总图（sys/srv/vm/vfs/sched/kernel notify） | 概念 | 新增 | — | `pm/` 全部调用点 | 全局地图 | 新增 |
| K-18-03 | `sys_*` 内核调用表（kill/clear/abort/times/runctl/resume/trace/getksig/endksig/sigsend/setmcontext/getmcontext/setalarm/vtimer/settime/stime/sprof/diagctl/stop/vircopy/datacopy） | 接口协议 | 新增 | 分散于各篇 | `lib/libsys/*`；各 handler | 内核面清单 | 新增；E6 清单为原料 |
| K-18-04 | `srv_*`（PM 侧被调用由 VFS/RS 发起）与 `vm_*`（fork/willexit/exit/getrusage）契约 | 接口协议 | 新增 | 分散于 07/09/17/20 | `lib/libsys/vm_fork.c`；`02-stage-vm` | 对端清单 | 新增 |
| K-18-05 | `SCHED` 客户端协议（START/INHERIT/STOP/SET_NICE + `mess_pm_sched_*`） | 接口协议 | 新增 | 16 §2.7（部分） | `com.h:801-807`；`ipc.h:1820-1828` | 调度面清单 | 新增；值勘误后归此 |
| K-18-06 | VFS 协议面索引（指向 05；只列编号与字段名） | 接口协议 | 新增 | — | `com.h:513-583` | 单一入口 | 新增 |
| K-18-07 | 阻塞/非阻塞/异步规则（`tell_vfs` asynsend、`sched_nice` taskcall、`sys_*` 同步） | 约束不变量 | 新增 | 分散 | `utility.c:134`；`schedule.c:107` | 同步语义 | 新增 |
| K-18-08 | 启动参数缺口（`SYS_GETMONPARAMS`/`SYS_GETIMAGE` 双侧缺） | 架构演进 | 新增 | 01 §3.1（BootParams） | `edge_todo.md` E6；`main.rs:15` | 通电前置 | 新增（缺口 G-12） |
| K-18-09 | `KernelGateway` 与域窄 trait 清单 + 接线批次模型 | 架构演进 | 新增 | 00 §4.1；plan §4.1 | `exit.rs`；`todo.md §11.1.1` | 实现边界 | 新增；不写计数 |
| K-18-10 | Rust `mproc/wire.rs` 与 MIB/procfs 消费契约 | 架构演进 | 新增 | 02 §（未提） | `mproc/wire.rs`；`mib`/`procfs` | 跨服务表 ABI | 新增（缺口 G-04） |
| K-18-11 | 错误保真规约（透传 errno） | 约束不变量 | 存量 | plan §4.2 | `plan.md:231` | 跨服务错误 | 主讲述点 |
| K-18-12 | minix-types PM 协议面缺口（PmRequest/PmResponse 死代码、调用号双址、wire 未系统化） | 架构演进 | 新增 | —（todo P1-4/P2-3） | `edge_todo.md` E7 | 实现路线 | 新增（缺口 G-09） |

### 2.20 新 99（常量、调用号与 wire 词典）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-99-01 | 词典规则与单一权威 | 概念 | 存量 | 99 §1 | 全篇 | 查表入口 | 主讲述点 |
| K-99-02 | 身份与容量常量（`NR_PIDS`/`NO_PID`/`INIT_PID`/`NO_TRACER`/`NO_EVENTSUB`/`NR_PROCS`） | 数据结构 | 存量 | 99 §2.1 | `const.h:3-13`；`sys_config.h:8` | 常量表 | 主讲述点 |
| K-99-03 | 19 个旗标位值表 | 数据结构 | 存量 | 02 §2.2；99 | `mproc.h:86-104` | 位值权威 | 主讲述点 |
| K-99-04 | 信号集合与信号号（1–29 + 内核 70/73/74） | 数据结构 | 存量 | 99 §2.3 | `sys/sys/signal.h:52-83,264-277`；`main.c:137-141` | 信号常量 | 主讲述点 |
| K-99-05 | endpoint 编码（generation<<15 + slot 与任务偏移） | 数据结构 | 存量 | 99 §2.2 | `endpoint.h:45-51` | 身份编码 | 修正命名 |
| K-99-06 | 47 个调用号全表 | 数据结构 | 存量 | 04 §2.7；99 | `callnr.h:14-60` | 分派常量 | 主讲述点 |
| K-99-07 | `VFS_PM_*` 请求/回复全表 | 数据结构 | 存量 | 05 §2.1 | `com.h:517-544` | 协议常量 | 主讲述点；修正 `SRV_FORK_REPLY` 0x988 |
| K-99-08 | `SUSPEND`、`OK/errno` 回复约定 | 数据结构 | 存量 | 04 §1.4；99 | `com.h:1151` | 回复常量 | 主讲述点 |
| K-99-09 | `SI_*`/`RB_*`/`TO_*`/`T_*`/`CLOCK_*`/`PRIO_*`/`SA_*`/`how` | 数据结构 | 存量 | 99 §2.4；各篇 | 对应头文件 | 常量族 | **修正**：`SI_PROC_TAB=2`、`RB_POWERDOWN=0x808` |
| K-99-10 | 时间/容量魔数（`MAX_SECS`/`NR_ITIMERS`/`LAST_FEW`/`MP_MAGIC`/`USER_Q`/`USER_QUANTUM`/`NR_SCHED_QUEUES`） | 数据结构 | 存量 | 99 §2.1；14/16 | `const.h:15-20`；`mproc.h:106`；`config.h:66-74` | 参数表 | **修正**：`MAX_SECS` 定义 |
| K-99-11 | Rust 权威落点映射（每个常量的唯一 import 源） | 约束不变量 | 存量 | 99 §3 | `minix-types` 各模块；`mproc/constants.rs` | 防漂移 | 主讲述点 |
| K-99-12 | 显式排除项（`ENABLE_SYSCALL_STATS`/`SPROFILE`/`ESCRIPT`/uts 兼容块/内核侧实现） | 约束不变量 | 存量 | 99；plan §5.4 | `plan.md` §5.4；`profile.c:24` | 范围声明 | 主讲述点 |
| K-99-13 | PM 错误码族（各调用可能返回的 errno） | 数据结构 | 存量 | 各篇 | `sys/sys/errno.h` | 错误速查 | 主讲述点 |
| K-99-14 | 测试基建边界（Rust 单测随篇；端到端归 E5/E9） | 工具工程 | 存量 | 00 §3.3 | `edge_todo.md` E5 | 验证面归属 | 不写快照数字 |

### 2.21 池统计与合并说明

- **总条数 329 条**（脚本对账：§2 表内 `^| K-` 行 329，唯一编号 329）。**存量 312，新增 17**（K-01-14、K-02-16、K-07-10、K-08-17、K-10-25、K-17-16、K-18-01…10（除 K-18-11）、K-18-12）。
- **按类型分布**（脚本对账）：机制 121、约束不变量 59、接口协议 42、数据结构 35、概念 32、架构演进 29、工具工程 11、测试 0（测试进度不入池）。
- **按新篇章分布**（脚本对账，每条行的去向即分组）：00×10、01×17、02×16、03×12、04×12、05×13、06×14、07×11、08×18、09×27、10×26、11×26、12×19、13×16、14×17、15×19、16×13、17×17、18×12、99×14。
- **合并说明（主讲述点）**：`fill_send_mask` 型跨篇重复在本 stage 较少，主要是：`handle_vfs_reply`（01/04/05 → 05）、`sig_proc` 处置链（11/12/13 → 10）、`exit_proc`/`zombify`（09/10 → 09）、`cleanup`（03/09/10 → 09）、`set_rusage_times`（10/20 → 09）、`do_getsetpriority`（16/20 → 07）、`srv_fork` 九步（07/08 → 08）、`init_service`/VFS 握手（01/05 → 05）、`nice_to_priority`/`get_nice_value`（01/16 → 07）、`SIGS_IS_*`（06/11/13 → 10 + 99）、信号集合（01/02/11/99 → 01 构建 + 99 数值）、`SIGSNDELAY`（11/13 → 10 入口 + 11 兑现）。
- **删除条目**：无知识点被删除。不进入新正文的只有三类过程性内容：① 测试计数与"Fix #NN/R#/批次 A–H"实现状态叙述；② 隐藏设计目录引用（编号文档约 21 处）；③ 已被 C 源证伪的断言（如"`ASYN_NR` 是内核预留队列""`SRV_FORK_INHERIT_FLAGS` 已保留 PRIV_PROC""`sigmsg.sm_mask` 被 sa_mask 累加"），这些不是删除而是**改写为正确表述**。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

主题全集由四路合成，再与知识点池、旧文档对账：

1. **C 源码符号**（逐文件核对，15 个 `.c` 共 4747 行）：`main.c` 启动/主循环/回复/VFS 回复/进程树；`forkexit.c` fork/srv_fork/exit/wait/zombie/收养；`signal.c` 生成/判定/处置/安装/投递/停止恢复；`alarm.c` itimer/REAL/VIRTUAL/PROF；`exec.c` 三阶段 exec；`event.c` 发布订阅；`getset.c` 凭证；`misc.c` 杂项查询；`schedule.c` 调度交接；`time.c` 双时钟；`trace.c` ptrace；`utility.c` PID/查找/isokendpt/tell_vfs/rusage；`table.c` 分发表；`profile.c`/`mcontext.c` 两个 feature 面。
2. **操作系统通用概念**：多副本进程表、pid/endpoint/slot 三层身份、代际防伪、fork 的写时复制委托、僵尸与收养、作业控制会话、信号四处置与不可捕获集合、异步系统调用的 continuation、发布/订阅、资源记账、时间双时钟、ptrace 状态机。
3. **非 C 制品主题**：`system.conf` 的 `service pm` 声明、`Makefile`、Cargo feature、集成测试、RS/VFS/VM/SCHED/内核五类对端协议。
4. **阶段边界契约**：master-plan 的启动因果链、03-stage-rs 的 RS→PM 放行协议、`edge_todo.md` E5/E6/E7、`05-stage-vfs` 的 fproc 对端、`02-stage-vm` 的 vm_fork/vm_exit 对端、`06-stage-sched` 的调度对端。

### 3.2 覆盖缺口表

| 编号 | 主题 | 为什么重要 | 原料与锚点 | 建议落实为 |
|------|------|-----------|-----------|-----------|
| G-01 | PM 的装载/抑制/放行链 | 旧文只说"PM 由 RS 加载"，实际内核装载所有 boot ELF、只抑制调度 | `kernel/main.c:196,253,265-266`；`kernel/table.c:55` | 01 §2（K-01-01） |
| G-02 | PM 重启语义（RS 重启 PM 后发生什么） | 旧文只登记 `SEF_CB_INIT_RESTART_STATEFUL` 一行；mproc/VFS 重对齐无叙述 | `main.c:119`；`lib/libsys/sef_init.c` | 01 §5（K-01-14）；跨端细节引 18 |
| G-03 | 内核信号回环（getksig/endksig + SEF 分发） | 旧文只在 Rust 接线块里一笔带过；这是"内核信号怎么到 PM"的完整答案 | `lib/libsys/sef_signal.c`；`sys/sys/signal.h:273-274` | 10 §2（K-10-25） |
| G-04 | `mproc` 的 C 字节镜像与消费契约 | MIB/procfs/getsysinfo 消费同一布局；Rust 已有 `mproc/wire.rs` | `mproc/wire.rs`；`misc.c:125-143` | 02 §4（K-02-16）+ 18 §4（K-18-10） |
| G-05 | PM↔VM 调用面（fork/willexit/exit/getrusage） | 签名散落；VM 侧文档是唯一对端来源 | `lib/libsys/vm_fork.c`；`02-stage-vm/18-vm-fork.md` | 18 §3（K-18-04） |
| G-06 | SCHED 客户端协议全表 | 旧 16 只有一句且常量值写错 | `com.h:801-807`；`ipc.h:1820-1828` | 07 §4（K-07-04/07）+ 18 §3（K-18-05） |
| G-07 | `SEND_PRIORITY`/`SEND_TIME_SLICE` | 调度消息码在旧目录完全缺失 | `servers/pm/const.h:19-20` | 07 §4（K-07-10） |
| G-08 | `system.conf` 的 PM 声明与 sigmgr 归属 | 解释"PM 的信号管理器是 RS、用户进程的管理器是 PM" | `minix3/etc/system.conf`（service pm）；`priv.h:82-85` | 01 §2（K-01-02/03 的配置对照）+ 18 §5 |
| G-09 | minix-types PM 协议面缺口与 wire 系统化 | 47 调用的载荷没有系统化类型；PmRequest/PmResponse 死代码 | `edge_todo.md` E7；`todo.md` P1-4/P2-3 | 18 §6（K-18-12） |
| G-10 | 启动参数来源缺口（GETMONPARAMS/GETIMAGE） | `BootParams::placeholder` 无真实对端，属通电前置 | `edge_todo.md` E6；`os/servers/pm/src/main.rs:15` | 01 §2（K-01-06）+ 18 §4（K-18-08） |
| G-11 | Rust 常量 bug（`SI_PROC_TAB=0`、`RB_POWERDOWN=1`）与 `MAX_SECS=100_000_000` 硬编码 | 通电后会直接产生错误 ABI 行为 | `os/servers/pm/src/misc.rs`；`os/servers/pm/src/timer.rs:19` | 17 §6（K-17-16）+ 12 §5（K-12-16）；列为 B 相必修 |
| G-12 | procfs 消费者（`mp_frame_addr/len`、`MP_MAGIC`、`mp_name`/`mp_nice`） | 跨服务可见性契约，旧文只有零散一句 | `exec.c:111-117`；`fs/procfs`；`mib` | 02 §4 + 18 §4 |
| G-13 | 进程组/会话语义汇总（A-13） | 散在 exit/credentials/signal 三篇；无一处完整 | `forkexit.c:298,412`；`getset.c:66-78,205-213` | 09 §4 + 13 §4 + 99 §2 |
| G-14 | 信号管理器自省（SIGKSIGSM）与 PM 自身信号 | SEF 分发对自身信号的路径未写 | `sys/sys/signal.h:273`；`sef_signal.c` | 10 §2（K-10-25 邻域） |
| G-15 | 47 调用完整枚举（每调用一行：号/名/handler/文档） | 旧 04 有表但行号锚点与 handler 归属多处过时 | `callnr.h:14-60`；`table.c:14-62` | 04 §3 + 99 §3（两处互引，单一权威在 99） |
| G-16 | 错误保真规约 | 只在 plan 里；是跨服务错误语义的硬约束 | `plan.md:231`；`edge_todo.md` E7 | 18 §5（K-18-11）；05 引用 |
| G-17 | `sys_datacopy` 的 SELF 语义与 PM 用法 | 多处拷贝（sigaction/groups/itimer/rusage/uts）依赖它 | `lib/libsys`；`misc.c:90,143,185` 等 | 18 §3（K-18-03） |
| G-18 | Rust 接线状态模型（批次/缺口）的"非易腐"表达 | 旧文写死"8 个接线/40 个 ENOSYS"已过期 | `ipc/calls.rs`；`todo.md §11.1.1` | 00 §3（K-00-07/08）、18 §6（K-18-09） |
| G-19 | `ESCRIPT`/`read_header` 相关死代码边界 | `exec.c:31` 定义了从不返回的 `ESCRIPT`；旧 plan 已排除但正文未提 | `exec.c:31`；`plan.md` §5.4 | 99 §6（K-99-12） |
| G-20 | `mproc` 体积/性能语义（表格大小、O(1) 期望） | 旧文两处未实测数字（"约 480B/75KB"） | `mproc.h`；字段类型 | 99 §6/§7；不写未测数字 |

### 3.3 重复主题表

| 主题 | 旧目录全部出现位置 | 新目录主讲述点 | 其余位置处置 |
|------|------------------|---------------|-------------|
| `handle_vfs_reply` 的 11 路语义 | 01 §2.5、04 §2.4、05 §2.3、09 §2.7、10 §2.10、13、15、16、17、18、20 | 05 §3 | 其余篇只写各自的应答分支与调用点 |
| `sig_proc` 处置链 | 11 §2.4、12 §1.2、13 §2 引言 | 10 §4 | 11/12 只引用，不重述优先级 |
| `exit_proc`/`zombify`/`check_parent`/`cleanup` | 09 §2.2-2.6、10 §2.5/§2.9、03 §2.5 | 09 §4-§6 | 03 只讲 `cleanup` 的表语义；10 只讲消费侧 |
| `set_rusage_times` | 10 §2.8、20 §2.11 | 09 §6 | 20 只写 rusage 调用链 |
| `get_free_pid` | 01 §2.4、03 §2.4、07 §2.5、08 §2.5 | 03 §3 | 07/08 只写调用点与相位 |
| `sched_start_user` | 05 §2.3、07 §2.6、16 §2.2 | 07 §3 | 05/08 只写触发条件 |
| `nice_to_priority`/`get_nice_value` | 01 §2.5、16 §1.2/§2.4/§2.5 | 07 §3 | 01 只列归属锚 |
| `do_getsetpriority` | 16 §2.6、20 §1.7 | 07 §4 | 20 不再展开 |
| srv_fork 九步骨架 | 08 全篇与 07 §2.1-2.7 重复 | 08 §2 | 合并后只讲一次 + 差异表 |
| 信号三集合（core/ign/noign） | 01 §2.4、02 §2.3、11 §1.4/§2.7、13 §2.7、99 §2.3 | 01 §2（构建）+ 99 §2（数值） | 信号篇只引用集合名 |
| `SIGS_IS_*` 宏 | 06 §2.6、11 §1.6/§2.4、13 §2.7 | 10 §5 + 99 | 其余引用 |
| `SIGSNDELAY` | 11 §2.6、13 §1.1/§2.6 | 10 §2（入口）+ 11 §5（兑现） | 两处分工明确 |
| `VFS_PM_*`/`PROC_EVENT*` wire 表 | 05 §2.1、06 §2.1/§2.8、13 §2.7、各篇消息小节 | 05/06 各自 + 99 数值 | 其余篇只给字段名 |
| `mp_flags` 位表 | 02 §2.2、09 §1.3、11 §2.7、13 §2.7、16 §1.4 | 02 §3 + 99 §3 | 其余引用 |
| 47 调用归属 | 04 §2.7、各 handler 文档 header、plan §5.3 | 04 §3 + 99 §3 | 单点权威 |
| 隐藏设计目录快照引用 | 全部机制篇（约 21 处） | — | 删除（规范禁引） |

### 3.4 越界主题表

| 旧位置 | 越界内容 | 正确归属 | 新目录处置 |
|--------|---------|---------|-----------|
| 01 §2.1/§2.4 | 主循环逐段与 47 调用分派 | 04 | 01 只给入口与回调注册 |
| 01 §2.6 | `sched_init` 完整机制 | 07 | 01 只给调用点 |
| 02 §2.5 | fork 字段继承的完整流程 | 08 | 02 只留字段语义与继承表 |
| 03 §2.4 | PID 分配算法细节 | 03（保留） + 08 调用点 | 无越界，跨篇只引用 |
| 04 §2.7 | 47 调用与 handler 归属表 | 04（骨架）+ 99（数值） | 保留但勘误 |
| 05 §2.3 | fork/exit/exec 的回复分支语义 | 08/09/14 | 05 只留协议状态机，分支指向各篇 |
| 06 §1.1 | SysV IPC 对端细节 | 13-stage-ipc | 只留"为什么需要事件" |
| 07 §2.6/§2.7 | `sched_start_user`/`sig_proc` 机制 | 07/10 | 只保留调用点 |
| 09 §2.7/§2.10 | 主循环 EXITING 丢弃 | 04 | 09 引一句 |
| 10 §1.6 | 其他 OS wait 语义对比 | —（支线） | 保留为短对比，不展开 |
| 11 §4.1 | Rust 接线状态与批次 | 18 | 移入实现边界模型，不写计数 |
| 12 §4.4 | minix-types 消息对齐实现 | 18/99 | 引用 |
| 13 §3 D6 等 | `SIGSNDELAY=70` 的 Rust 修复史 | 99（数值）+ 契约 | 删除过程叙述 |
| 14 §4.3/§4.4 | Rust `ALARM_ON` 与 init 接线 | 12 §5 | 保留形态，删过程 |
| 15 §2.15 | `ipc.h` 消息族全表 | 99 | 只留本节用到的几条 |
| 16 §2.7 | SCHED 协议全表 | 18 | 16/07 只留使用点 |
| 17 §2.9 | `exec_info` 定义位置 | 18 | 引用 |
| 18 §2.14 | T_*/TO_* 常量全表 | 99 | 引用 |
| 19 §2.8/§2.9 | 内核 `do_settime`/`do_stime` 实现 | 01-stage-kernel | 只留契约一句 |
| 20 §2.12 | 杂项调用号与消息全表 | 99 | 引用 |
| 99 §3/§4 | Rust 实现细节与测试点 | 18/各篇 | 99 只留映射规则 |

### 3.5 事实勘误表（B 相必须按 C 源写对）

| # | 旧位置 | 错误断言 | C 源事实 | 严重度 |
|---|--------|---------|---------|--------|
| E1 | 20 §1.2/§2.2/§2.12、Rust `misc.rs` | `SI_PROC_TAB = 0` | `SI_PROC_TAB = 2`（`include/minix/sysinfo.h:11`） | P0（代码同错） |
| E2 | 20 §2.12/§5.2、Rust `misc.rs` | `RB_POWERDOWN = 1` | `RB_POWERDOWN = RB_HALT\|0x800 = 0x808`（`sys/sys/reboot.h:54`） | P0（代码同错） |
| E3 | 16 §1.1/§2.7/D7/§4.2 | `SCHEDULING_START=0`、`SET_NICE=5` | `START=0xF02`、`SET_NICE=0xF04`（`com.h:801-807`） | P0 |
| E4 | 11 §1.3/§1.6/§2.4 | `SIGS_IS_LETHAL` 含 KILL/TERM；与 `core_sset` 正交 | LETHAL = ILL/BUS/FPE/SEGV/EMT/ABRT；KILL/PIPE 只在 TERMINATION；LETHAL ⊆ core_sset | P0 |
| E5 | 11 §2.7；10 §2.3 | `_NSIG = 32` | `_NSIG = 64`（`sys/sys/signal.h:45`） | P0 |
| E6 | 17 §1.5/§2.9 | `TO_NOEXEC = 0x1` | `TO_NOEXEC = 0x4`（`sys/sys/ptrace.h:211`；0x1=TRACEFORK、0x2=ALTEXEC） | P0 |
| E7 | 14 §1.2/§2.3 | `MAX_SECS = 100M`，定义在 `timers.h` | `MAX_SECS = (clock_t)(TMRDIFF_MAX/system_hz)`，定义在 `servers/pm/const.h:15` | P0 |
| E8 | 15 §2.12 | `GID_MAX = 0xFFFFFFFF`，定义在 `sys/limits.h` | `GID_MAX = 2147483647U`，定义在 `sys/sys/syslimits.h:53` | P0 |
| E9 | 13 §2.5/§7 | `VFS_PM_UNPAUSE` 锚到 `com.h:498`/`DS_RQ_BASE` | `VFS_PM_UNPAUSE = 0x909`（`com.h:528`）；`VFS_PM_UNPAUSE_REPLY = 0x989`（541） | P1（锚点） |
| E10 | 08 §1.3 | `VFS_PM_SRV_FORK_REPLY = 0x989` | `0x988`（`com.h:541`，= 0x980+8） | P1 |
| E11 | 15 §2.14 | SETSID 回复 `reply(OK)` | `reply(rmp->mp_procgrp)`（`main.c:345`） | P1 |
| E12 | 16 §1.2/§5.1 | "nice 1 → queue 7 → nice 0" | `(1+20)*16/41 = 8` → queue 8 → `(8-7)*41/16 = 2` | P1 |
| E13 | 12 §1.6/§2.6/D7 | `sa_mask` 累加进 `sigmsg.sm_mask` | 累加进 `rmp->mp_sigmask`（`signal.c:800-803`）；`sm_mask` 只在 792-795 复制 | P1 |
| E14 | 12 §1.6 | `struct sigmsg` 4 字段 | 5 字段（含 `sm_stkptr`，`type.h:71-77`） | P2 |
| E15 | 13 §2.6 | "`SIGSNDELAY` 经 kill 也会产生" | `SIGSNDELAY=70 ≥ _NSIG=64`，`check_sig` 直接 EINVAL；只由内核发 | P1 |
| E16 | 06 §1.3 | `ASYN_NR` 是"内核为 PM 预留的异步槽" | 是 libsys 用户态 `msgtable[ASYN_NR]`（`asynsend.c:17-18`），经 `ipc_senda` 提交内核 | P1（概念） |
| E17 | 07 §2.2/D2/§4.2 | `alloc_slot` 先于 `vm_fork` 并回滚；`TO_TRACEFORK` 恒清零 | Rust 实为"先找槽→vm_fork→手动 ++，零回滚"；`TO_TRACEFORK` 条件继承已落地 | P1（doc-code） |
| E18 | 08 §1.3/§2.4/§5.1 | Rust 已保留 `PRIV_PROC`、`SRV_FORK_INHERIT_FLAGS` 存在 | 代码为 `Privilege::User(...)` + 空旗标；无该符号 | P1（doc-code） |
| E19 | 20 §1.1/§2.1 | `uts_tbl[8]`、4 个 NULL | 9 个元素、3 个 NULL（`misc.c:22-36`）；字段 1/3/8 为 NULL | P1 |
| E20 | 20 §2.5 | `RB_KEXEC` 是已存在旗标 | 树内不存在 `RB_KEXEC` | P2 |
| E21 | 00 §3.1/§3.2、04 §3.6/§4.2/§5.2 | 接线 8 个/40 个 ENOSYS；多项"未落地" | 当前 41/47 已接线（6 个 ENOSYS：GetPriority/SetPriority/GetTimeOfDay/GetRUsage/Reboot/GetSysInfo） | P2（易腐，改写法） |
| E22 | 02 §1.2/§3.1 | `struct mproc` "约 60 字段" | 41 字段（`mproc.h:24-83`） | P2 |
| E23 | 09/10 多处小节标题锚点 | `do_srv_fork（L245）` 标 `do_exit`、`do_exit（L267）` 标 `exit_proc`、`tracer_died（L795）` 标 `cleanup`、`wait_test（L593）` 标 `zombify` | 正确：`do_exit` 245、`exit_proc` 266、`cleanup` 795、`zombify` 593、`wait_test` 569 | P2（锚点） |
| E24 | 05 §2.4 | `do_srv_fork` 行写 `→ VFS_PM_EXIT` | 应为 `VFS_PM_SRV_FORK`（`forkexit.c:222-230`） | P1 |
| E25 | 19 §1.2/§2.7 | "hz=5e4 时 `clock_time` 不取大乘分支导致 nsec 退化" | `LONG_MAX/40000≈53687 > 50000`（32 位），分支会取；50kHz 是支持上限 | P2 |
| E26 | 15 §2.15 | "13 个调用"但只列 11 个 | `do_get` 7 分支 + `do_set` 6 分支 = 13 | P2 |

> 上述 26 条全部有锚点；B 相写新正文时必须以本表为准复核。E1/E2/E3/E7 对应的 Rust 代码若在 B 相前未修，正文应如实写"实现与 C 不一致，待修"而不是照抄旧文。

### 3.6 非 C 主题逐项回答

| # | 主题 | 在本 stage 哪里讲（或为什么不在） |
|---|------|--------------------------------|
| 1 | 链接与加载 | PM 自身 ELF 由内核 boot 装载（01 §2 讲装载/抑制/放行链）；RS 为其发 `RS_INIT`（03-stage-rs 已讲）——本 stage 不再重讲链接细节。 |
| 2 | 镜像与内存布局 | `mproc` 表的 C 字节镜像（wire.rs + getsysinfo）在 02/17/18；`sigmsg`/`sigaction`/`rusage`/`uts` 的用户内存布局在各机制篇与 99；无独立"镜像布局"篇。 |
| 3 | 汇编入口与陷阱进入 | 不在本 stage：PM 从 SEF `_start` 进入（01 一句）；trap 桥属 01-stage-kernel 用户态入口专题。 |
| 4 | 启动装配 | 01 全篇（八步 + VFS 握手 + 进程树）；`Makefile`/boot image 组装见第 5 项。 |
| 5 | 构建与工具链 | C `Makefile` 与 `minix.service.mk` 属构建系统，按 plan §5.4 判 WONTFIX，99 §6 显式排除；Rust Cargo feature（`syscall_stats`/`sprofile`）在 17/99。 |
| 6 | 跨模块接口与线格式 | 18 全篇（sys/srv/vm/vfs/sched/kernel）；05/06/07 各协议的字段语义；99 数值与 wire 表。 |
| 7 | 错误路径 | 各篇"失败/边界"小节 + 99 §6 错误码族；错误保真规约在 18 §5；不单独成篇（会与各机制重复）。 |
| 8 | 关闭与退出 | 09（进程退出两阶段）+ 17（系统 reboot 定序）+ 04（EXITING 延迟调用丢弃）；PM 自身被 RS 重启在 01/18。 |
| 9 | 并发与同步 | PM 单线程事件循环（04 §1 执行模型）；无锁无 SMP 共享；异步 continuation 的"每进程一个未完成 VFS 请求"是唯一的并发约束（05 §2）。 |
| 10 | 测试基建 | Rust 单测随各篇验收；端到端归 E5（`edge_todo.md`）；C 侧 `minix3/minix/tests/` 无 PM 专属测试程序，空白显式声明。 |

---

## 4. 新目录

### 4.1 新篇章总表

旧目录 22 篇 → 新目录 20 篇。文件名沿用 `NN-<语义名>.md`；`00` 总览、`99` 词典的约定不变。**编号含义有变，详见 §8.2 引用迁移表**。

| 编号 | 标题（文件名） | 一句话定位 | 分组 | 前置 | 承载知识点 |
|------|---------------|-----------|------|------|-----------|
| 00 | `00-pm-overview.md` 总览与阅读地图 | PM 是谁、在 boot 链哪里、四重权威、新目录怎么读 | 0 导航 | 无 | K-00-01…10 |
| 01 | `01-pm-boot-init.md` 启动与初始化 | 从内核装载 ELF 到 RS 放行、八步初始化、VFS 握手、第一代进程树 | I 启动 | 00 | K-01-01…17 |
| 02 | `02-mproc-struct.md` 进程结构 | 一个 `mproc` 槽位里有什么、状态如何分层、动作表为何在槽外 | II 进程模型 | 00、01 | K-02-01…16 |
| 03 | `03-mproc-table.md` 进程表与身份 | slot/endpoint/PID 三层身份、代际防伪、容量纪律、查找与释放 | II | 02 | K-03-01…12 |
| 04 | `04-ipc-dispatch.md` 主循环与消息分发 | 47 个调用如何分派、三路分发、SUSPEND 回复协议、ENOSYS 兜底 | III 运行时骨架 | 01、02、03 | K-04-01…12 |
| 05 | `05-vfs-protocol.md` PM↔VFS 异步协议 | 12 请求/11 回复的线格式、状态机、continuation 与启动握手 | III | 04 | K-05-01…13 |
| 06 | `06-event-subscription.md` 进程事件发布/订阅 | 事件如何有界、串行、逐订阅者投递，以及死亡清理 | III | 02、05 | K-06-01…14 |
| 07 | `07-scheduling.md` 调度交接 | INIT 交给 SCHED、fork 子进程继承、nice↔queue 双射与优先级调用 | IV 生命周期 | 01、04 | K-07-01…11 |
| 08 | `08-process-create.md` 进程创建 | fork 与 srv_fork：九步骨架、两阶段、五差异 | IV | 03、05、07 | K-08-01…18 |
| 09 | `09-exit-wait.md` 终止与回收 | exit 两阶段、僵尸与收养、wait4 三环与 rusage | IV | 03、05、06、08 | K-09-01…27 |
| 10 | `10-signal-core.md` 信号模型 | 生成、判定、处置链与内核信号回环 | V 信号与定时器 | 04、05、09 | K-10-01…26 |
| 11 | `11-signal-handling.md` 信号安装、投递与恢复 | sigaction 安装语义、sig_send 翻译、停止/恢复/SIGSNDELAY | V | 05、10 | K-11-01…26 |
| 12 | `12-timer.md` 定时器 | itimer 三族、ticks↔timeval、SIGALRM 与虚拟定时器 | V | 04、10 | K-12-01…19 |
| 13 | `13-credentials.md` 凭证与身份变更 | real/eff/saved 三元组、组列表、TAINTED、VFS 同步 | VI 属性与工具面 | 02、05 | K-13-01…16 |
| 14 | `14-exec.md` exec | 三阶段执行替换：VFS 转发、半态哨兵、信号重置、tracer 信号 | VI | 05、11、13 | K-14-01…17 |
| 15 | `15-trace.md` ptrace | T_* 全族、双入口、双轨暂停、sigtrace 重放 | VI | 09、10、11、14 | K-15-01…19 |
| 16 | `16-time.md` 时间 | REALTIME/MONOTONIC 双时钟、五调用、溢出规避 | VI | 02、04 | K-16-01…13 |
| 17 | `17-misc-queries.md` 杂项与查询 | uname/getsysinfo/endpoint 解析/reboot/svrctl/rusage/sprofile/mcontext | VI | 03、05、09 | K-17-01…17 |
| 18 | `18-external-interfaces.md` 外部接口契约 | 全部外部调用与 wire 的唯一签名权威面（新建） | VII 参考 | 01–17 | K-18-01…12 |
| 99 | `99-global-concepts.md` 常量、调用号与 wire 词典 | 常量、47 调用、VFS/事件/SCHED/Ptrace 码值、Rust 落点 | VII | 无（词典） | K-99-01…14 |

新目录的**总依赖图**（只画跨部分依赖；篇内小节依赖见 §5 契约）：

```
00
└─► 01 ─► 02 ─► 03 ─┐
        │           ├─► 04 ─► 05 ─► 06
        │           │        │      │
        └───────────┘        ├─► 07 ─► 08 ─► 09 ─► 10 ─► 11
                             │                     │     │
                             │                     └─► 12│
                             ├─► 13 ─► 14 ─► 15 ◄───────┘
                             ├─► 16
                             └─► 17
18：全部篇的外部签名汇总（无前置，按需查）
99：常量词典（无前置，按需查）
```

**无环检查**：依赖边全部从小号指向大号（00→…→17），18/99 无出边 → 无环（G4 通过）。软性机制 deferral（08/09 引用 10 的 `sig_proc`、07/17 引用 13 的 privileged 谓词、14 引用 15 的 tracer 信号）记录在 §4.4 序差表，不构成重新进入更早篇的学习依赖。

### 4.2 阅读路径

- **完整首读主线**：`00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17`，最后按需查 `18`/`99`。
- **只理解启动链**：`00 → 01 → 04`（主循环入口）→ `05`（VFS 握手）。
- **只理解 fork 次主线**（旧 plan 的次主线继承）：`03 → 05 → 07 → 08 → 09`；信号细节按需跳 `10`。
- **只理解信号与调试**：`10 → 11 → 12`，再 `15`。
- **只理解身份/调度**：`02 → 03 → 13`，再 `07 → 14`。
- **运维/移植接口**：`17 → 18 → 99`。
- **支线与可跳读**：`06`（事件订阅，目前唯一消费方是 IPC server）、`12` 的 VIRTUAL/PROF 小节、`15`（ptrace）、`16`（时间）、`99`（词典随用随查）。

### 4.3 并行主题的分组与代表成员

| 并行体 | 规模 | 统一框架 | 分组方式 | 代表成员（讲透） | 差异表 |
|--------|------|---------|---------|----------------|--------|
| PM 调用 | 47 个调用号 | 04（分段/分派/reply） | ① 生命周期：08/09/14；② 信号与定时器：10/11/12；③ 身份与调度：07/13；④ 观测与控制：17；⑤ 调试：15 | `PM_FORK`（08）、`PM_SIGACTION`（11）、`PM_EXIT`（09）、`PM_PTRACE`（15） | 04 §3 调用总表 + 99 §3 数值 |
| VFS 协议消息 | 12 请求 + 11 回复 | 05（状态机 + `tell_vfs`） | 按触发时机：身份变更 / 生命周期 / 中断 / 其它 | `VFS_PM_FORK`（08）、`VFS_PM_EXIT`（09）、`VFS_PM_UNPAUSE`（11）、`VFS_PM_REBOOT`（17） | 05 §2 全表 |
| 信号集合与位图 | 3 个集合 + 7 个位图字段 | 02（字段）+ 10（语义） | 按用途：处置集合 / 进程位图 / 内核信号 | `core_sset`/`noign_sset`（10）、`mp_sigpending`（11）、`mp_sigtrace`（15） | 99 §2 数值表 |
| 外部依赖面 | 6 类（sys/srv/vm/vfs/sched/kernel-notify） | 18 §1 总图 | 按被调用者分组 | `sys_kill`（10）、`vm_fork`（08）、`sched_start`（07）、`tell_vfs`（05） | 18 各表的"调用点"列 |
| ptrace 命令族 | 19 个 T_* | 15（双入口 + 守卫） | 按处理方：PM 全处理 / PM+内核 / 内核透传 | `T_ATTACH`（15）、`T_EXIT`（15）、`T_RESUME`（15） | 15 §3 全表 + 99 §4 |

### 4.4 序差表（运行时序 vs 教学序）

| # | 运行时序事实（锚点） | 教学序选择 | 理由 | 回指补偿 |
|---|---------------------|-----------|------|---------|
| S-1 | `sched_init` 在 boot 八步的第 8 步执行（`main.c:241`） | 调度机制（07）排在启动篇（01）之后、生命周期之前 | 它是 fork 子进程继承（`sched_start_user`）与 `do_getsetpriority` 的共同机制；放 07 可让 08/17 只写调用点 | 01 §2 只给调用点与"接管 INIT"一句话 |
| S-2 | 信号在运行时由内核任意时刻送达 | 信号三篇（10/11/12）排在生命周期之后 | `sig_proc_exit` 依赖 `exit_proc`（09）；VFS/事件暂存依赖 05/06；先有生命周期才能讲信号致死 | 08/09 需要 `sig_proc` 处只写"投递见 10" |
| S-3 | `handle_vfs_reply` 的 FORK 分支在 VFS 回复时执行 | create 篇（08）引用 05 的回复状态机 | 协议先于用它的七类场景（身份/生命周期/中断/reboot） | 05 §2 给 11 路行为表，08 只回指 FORK 分支 |
| S-4 | `sig_send` 需要目标已 `PROC_STOPPED`（`signal.c:787`） | 安装（11）与停止/恢复同篇 | 拆开会让"投递"与"停止"互为前后；同篇按"安装→投递→停止恢复"叙述 | 10 §4 caught 分支回指 11 |
| S-5 | exec 的 catch 重置发生在 `exec_restart`（`exec.c:178-184`） | exec 篇（14）排在信号篇（11）之后 | 重置语义已在 11 定义；tracer 信号语义在 15 定义（14 只写时序） | 14 §4 回指 11/15 |
| S-6 | `do_getsetpriority` 与 `sched_init`/`sched_start_user` 同属调度机制但分居 `misc.c`/`schedule.c` | 合并进 07 一篇 | 单点权威（旧 16 已如此，继续沿用） | 17 不再展开该调用 |
| S-7 | `set_rusage_times` 被 exit 记账与 getrusage 共用（`forkexit.c:308`/`misc.c:438`） | 主讲述点放 09 | 它先是"子进程时间记账"的一部分，后被 rusage 消费 | 17 §rusage 回指 09 |
| S-8 | reboot 的 VFS 回复经主循环第一路进入（`main.c:304-312`） | 17 引用 05，不重讲状态机 | 单点权威 | 05 §2 表内列出 `VFS_PM_REBOOT_REPLY` |
| S-9 | `mproc` 字节镜像只在 MIB/procfs 消费时才有意义 | 02 只讲字段与不变量，镜像形态放 02 §末 + 18 | 避免在数据结构篇混入跨服务 ABI 细节 | 18 §4 给消费方清单 |

---

## 5. 每篇契约

> 说明：七要素齐全——定位、讲什么、不讲什么、前置、后置、事实底线、知识点清单 + 验收标准。知识点清单用"编号 + 名称 + 为什么归本篇"三列；类型/来源/锚点/读者收益见 §2 完整行。凡涉及 §3.5 勘误表的常量，验收标准中显式要求给出真值与锚点。

### 00-pm-overview：总览与阅读地图

- **一句话定位**：让读者在一篇内建立 PM 的全局心智模型——四重权威、boot 链位置、主循环骨架——并知道任何问题该翻哪一篇。
- **讲什么**：PM 的四重权威（K-00-01）；微内核进程语义在用户态的主张与代价（K-00-02）；boot 链位置（K-00-03）；主循环三问（K-00-04）；旧目录诊断与重建理由（K-00-05）；Rust 三层镜像与 KernelGateway（K-00-06/K-00-10）；47 调用空间（K-00-07）；跨阶段依赖（K-00-08）；新目录导航与阅读路线（K-00-09）。
- **不讲什么**：任何机制细节（全部下放：启动 01、结构 02、表 03、循环 04、协议 05/06、生命周期 07–09、信号 10–12、属性与工具 13–17、参考 18/99）；不重复 03-stage-rs 已讲的 RS boot 四步与 `RS_UP` 协议，只引用其结论。
- **前置**：无（建议先读 `../03-stage-rs/00-rs-overview.md` 或重建后的 RS 总览）。
- **后置**：全部 19 篇以本篇导航表为入口。
- **事实底线（ground truth）**：`main.c:59-107`（主循环）；`main.c:122`（信号管理器注册）；`mproc.h:24-83`（表）；`callnr.h:14-60`（47 调用）；`kernel/main.c:196,253,265`（装载/抑制）；`kernel/table.c:55`（PM 登记位）；新目录 TOC；`edge_todo.md` E5/E6/E7。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-00-01 | 四重权威 | 全 stage 第一个问题 |
| K-00-02 | 用户态进程语义主张 | 解释 PM 存在理由 |
| K-00-03 | boot 链位置 | 与 RS stage 衔接 |
| K-00-04 | 主循环三问 | 阅读锚 |
| K-00-05 | 旧目录诊断与重建理由 | 说明本蓝图的合法性 |
| K-00-06 | Rust 三层镜像 | 代码导航 |
| K-00-07 | 47 调用空间 | 规模认知 |
| K-00-08 | 跨阶段依赖 | 实现边界 |
| K-00-09 | 导航与阅读路线 | 导航枢纽 |
| K-00-10 | KernelGateway 规约 | 内核能力边界 |

- **验收标准**：① 读者能回答"PM 管什么、在 boot 链哪一环、47 调用分几类"；② 必须出现 boot 链位置图与主循环三活动图；③ 实施现状只写"接线模型 + 依赖清单"，不写任何接线计数与测试数字（E21）；④ 阅读路线不少于 5 条；⑤ 所有引用只用新编号。

### 01-pm-boot-init：启动与初始化

- **一句话定位**：PM 如何被装载、被放行、被要求初始化，以及它如何用八步建立"第一代进程世界"并与 VFS 对齐两张表。
- **讲什么**：装载/抑制/放行链（K-01-01）；SEF 运行库与 PM 回调集（K-01-02）；八步依赖链与两不变量（K-01-03）；空槽初始化与三信号集合（K-01-04/K-01-05）；boot 参数与 image 获取（K-01-06）；第一代进程树与 INIT/系统进程差异（K-01-07/K-01-08/K-01-09）；VFS 握手（K-01-10）；`system_hz`（K-01-11）；`sched_init` 调用点（K-01-12）；函数归属锚（K-01-13）；PM 重启路径（K-01-14）；BootParams/PmServer/VFS 同步客户端（K-01-15/K-01-16/K-01-17）。
- **不讲什么**：主循环与分派（04）；`sched_init` 的机制（07，本篇只给调用点）；信号判定（10，本篇只构建集合）；VFS 协议全表（05，本篇只讲握手）；`reply`/`get_nice_value` 实现（04/07）；`SEF_CB_INIT_RESTART_STATEFUL` 的 SEF 内部实现（18/SEF 库）。
- **前置**：00。**后置**：02/04/05/07/10/18。
- **事实底线（ground truth）**：`main.c:115-243`（回调注册与八步全文）、`main.c:249-424`（归属锚）；`schedule.c:20-50`；`kernel/main.c:196-266`；`kernel/table.c:52-55`；`mproc.h:16-22,106`；`sys/sys/signal.h:45`（`_NSIG=64`）；`lib/libsys/sef.c:185-230`、`sef_init.c`、`sef_signal.c`；`include/minix/com.h:520,547-551`；`minix3/etc/system.conf`（service pm）；Rust：`init.rs`、`main.rs`、`ipc/transport.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-01-01 | 装载/抑制/放行链 | 启动篇的开场 |
| K-01-02 | SEF 回调集 | 启动协议 |
| K-01-03 | 八步依赖链 | 主线 |
| K-01-04 | 空槽初始化 | 第一步 |
| K-01-05 | 三信号集合 | 全局输入 |
| K-01-06 | 参数与 image | 数据来源 |
| K-01-07 | 第一代进程树 | 进程世界 |
| K-01-08 | INIT 调度与 nice | 首进程 |
| K-01-09 | 系统进程特权 | 边界 |
| K-01-10 | VFS 握手 | 两表对齐 |
| K-01-11 | system_hz | 时间基准 |
| K-01-12 | sched_init 调用点 | 与 07 接缝 |
| K-01-13 | 函数归属锚 | 文档边界纪律 |
| K-01-14 | PM 重启路径 | 缺口补充 |
| K-01-15 | BootParams | Rust 契约 |
| K-01-16 | PmServer 构造 | Rust 形态 |
| K-01-17 | VFS 同步客户端 | Rust 形态 |

- **验收标准**：① 装载/抑制/放行链图（kernel→VM→RS→PM）每步带锚点；② 八步表：每步输入/动作/依赖；③ 三信号集合逐元素列出（与 `main.c:137-141` 一致）；④ `VFS_PM_INIT` 的字段与屏障模式有锚点；⑤ 明确写出 `sched_init` 的调用点与 `INIT` 的初值（`KERNEL`），细节指向 07；⑥ 重启路径如实写"SEF 通用状态迁移"，不编造恢复细节。

### 02-mproc-struct：进程结构

- **一句话定位**：把"一个进程"变成一个可检视的结构——41 个字段如何分组、19 个旗标位什么含义、信号动作表为何独立。
- **讲什么**：四副本表与信任边界（K-02-01）；单槽巨型结构问题与字段族分组（K-02-02/K-02-03）；`mpsigact` 隔离与深拷贝（K-02-04）；`struct mproc` 全字段（K-02-05）；19 旗标位（K-02-06）；阻塞旗标联合语义（K-02-07）；信号位图族（K-02-08/K-02-09）；初始化契约与空槽默认（K-02-10）；`MP_MAGIC`（K-02-11）；fork 继承（K-02-12）；Rust 分层/枚举/信号状态/字节镜像（K-02-13…K-02-16）。
- **不讲什么**：表操作与身份（03）；fork 流程（08）；旗标的状态机流转（09/10/11 各自消费）；信号安装语义（11）；`mproc` 导出协议（17/18，本篇只给字段与镜像形态）。
- **前置**：00、01。**后置**：03/04/05/06/07/08/09/10/11/13/14/15/17/18/99。
- **事实底线（ground truth）**：`mproc.h:16-22,24-83,86-104,106`；`main.c:146-152`；`forkexit.c:87-114`；`signal.c:279,425,672,693`；`sys/sys/sigtypes.h:60-71`；`sys/sys/signal.h:45`；`const.h:8-13`；Rust：`mproc/{mproc,lifecycle,block,signal,wire}.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-02-01 | 四副本表 | 信任边界 |
| K-02-02 | 巨型结构问题 | 分层动机 |
| K-02-03 | 状态分层原则 | 建模准则 |
| K-02-04 | 动作表隔离 | 反直觉设计 |
| K-02-05 | 全字段（41） | 字典 |
| K-02-06 | 19 旗标位 | 状态权威 |
| K-02-07 | 阻塞旗标联合 | 约束语义 |
| K-02-08 | SigSet 位语义 | 信号词汇 |
| K-02-09 | 信号位图族 | 信号状态 |
| K-02-10 | 空槽契约 | 初始化语义 |
| K-02-11 | MP_MAGIC | 跨服务令牌 |
| K-02-12 | fork 继承 | 字段规则 |
| K-02-13 | Rust 四层 | 实现映射 |
| K-02-14 | flags→枚举 | 实现映射 |
| K-02-15 | SignalState | 实现映射 |
| K-02-16 | 字节镜像 | 跨服务 ABI |

- **验收标准**：① 字段总数与 `mproc.h:24-83` 逐行对齐（修正"约 60"）；② 19 位旗标表含位值、语义、主要读者（哪篇消费）；③ 能解释 `mpsigact` 独立存储与 fork 深拷贝的理由；④ 空槽默认值与 `NO_*` 常量对齐；⑤ 字节镜像给出用途（MIB/procfs）与 Rust 模块，不复制偏移表（偏移表归 18）。

### 03-mproc-table：进程表与身份

- **一句话定位**：回答"PM 怎么索引活着的进程"——三层身份、代际防伪、PID 分配、容量纪律、五个查找与释放语义。
- **讲什么**：三层身份（K-03-01/K-03-02）；PID 空间与保留值（K-03-03/K-03-04）；endpoint generation（K-03-05/K-03-06）；容量纪律（K-03-07）；`pm_isokendpt`/`find_proc`（K-03-08/K-03-09）；`cleanup` 释放语义（K-03-10）；Rust 聚合与类型化（K-03-11）；特殊端点边界（K-03-12）。
- **不讲什么**：`mproc` 字段语义（02）；槽位在 fork/exit/wait 中的完整流程（08/09）；generation 的内核实现（01-stage-kernel 与 `kernel/system/do_fork.c`，本篇只写归属）；PID 之外的命名空间（进程组见 09/13）。
- **前置**：02。**后置**：04/05/08/09/10/11/13/15/17/18/99。
- **事实底线（ground truth）**：`utility.c:34-51,76-86,108-118`；`const.h:3-13`；`endpoint.h:45-57`；`forkexit.c:32,59-75,795-806`；`glo.h:9`；`kernel/system/do_fork.c`；Rust：`mproc/{table,pid_gen,context}.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-03-01 | 三层身份 | 索引模型 |
| K-03-02 | slot 四表坐标 | 一致性根基 |
| K-03-03 | PID 空间与保留值 | 边界 |
| K-03-04 | PID 分配算法 | 命名规则 |
| K-03-05 | generation 机制 | 防伪 |
| K-03-06 | generation 归属 | 所有权 |
| K-03-07 | 容量纪律 | 资源边界 |
| K-03-08 | pm_isokendpt | 调用者契约 |
| K-03-09 | find_proc | 查找 |
| K-03-10 | cleanup 语义 | 释放 |
| K-03-11 | Rust 聚合/类型化 | 实现映射 |
| K-03-12 | 特殊端点 | 边界值 |

- **验收标准**：① 三层身份图（slot↔endpoint↔pid 的分配者与生命周期）；② 五个查找函数的过滤差异表（label 型在 02/其他篇的接口不重复）；③ PID 分配的相位与冲突扫描含 `procgrp` 有锚点；④ `cleanup` 不 bump generation 的事实与理由；⑤ 特殊端点（SELF/NONE/ANY）范围有锚点。

### 04-ipc-dispatch：主循环与消息分发

- **一句话定位**：PM 的运行骨架——一轮循环如何接收、三路分发、选择同步或挂起回复，以及 47 个调用如何到达各自的 handler。
- **讲什么**：事件循环与三问（K-04-01）；消息空间分段（K-04-02）；三路分发（K-04-03）；`SUSPEND` 回复协议（K-04-04）；ENOSYS 与 panic 分界（K-04-05）；notify 前置处理（K-04-06）；EXITING 丢弃（K-04-07）；`call_vec` 与 47 调用表（K-04-08/K-04-09）；`reply` 语义（K-04-10）；统计 feature（K-04-11）；Rust 分派类型化（K-04-12）。
- **不讲什么**：每个 handler 的语义（指向各篇）；VFS 回复 11 路细节（05）；事件回复校验（06）；`expire_timers` 的定时器语义（12）；`pm_isokendpt` 的判定细节（03）。
- **前置**：01、02、03。**后置**：05/06/07/10/12/13/14/15/16/17/18/99。
- **事实底线（ground truth）**：`main.c:34-36,59-107,249-270`；`table.c:14-62`；`callnr.h:9-60`；`include/minix/com.h:515-517,1151`；`ipcconst.h`；Rust：`ipc/{calls,dispatcher,transport,decode}.rs`、`init.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-04-01 | 事件循环三问 | 骨架 |
| K-04-02 | 消息空间分段 | 协议空间 |
| K-04-03 | 三路分发 | 核心结构 |
| K-04-04 | SUSPEND 协议 | 回复契约 |
| K-04-05 | 错误策略 | 边界 |
| K-04-06 | notify 前置 | 顺序敏感 |
| K-04-07 | EXITING 丢弃 | 竞态防护 |
| K-04-08 | call_vec | 分发表 |
| K-04-09 | 47 调用总表 | 导航 |
| K-04-10 | reply 语义 | 回复 |
| K-04-11 | 统计 feature | 调试 |
| K-04-12 | Rust 分派 | 实现映射 |

- **验收标准**：① 一轮循环的分支图与 `main.c:65-106` 逐行对应；② 47 调用表每行给出"号 / 名字 / handler / 承载篇"（与 99 §3 的数值表互引，不重复数值）；③ `SUSPEND` 三类子情形各举一个真实调用点；④ 明确 `notify` 必须在 endpoint 校验前处理（`CLOCK`/`SYSTEM` 负 endpoint）；⑤ Rust 侧说明 `run_once`/`run` 拆分与 `ReplyIntent` 三态。

### 05-vfs-protocol：PM↔VFS 异步协议

- **一句话定位**：PM 与 VFS 之间唯一的一问一答通道——12 个请求、11 个回复、每进程一个 continuation、以及启动时的表对齐。
- **讲什么**：协作场景与异步必要性（K-05-01/K-05-02）；`VFS_CALL` continuation（K-05-03）；请求/回复编号与寻址键（K-05-04/K-05-05）；`tell_vfs` 三段式（K-05-06）；`handle_vfs_reply` 四段与 11 路行为（K-05-07/K-05-08）；七个调用点（K-05-09）；`NEW_PARENT`/`UNPAUSED`（K-05-10）；启动握手（K-05-11）；Rust 类型化（K-05-12）；错误保真规约（K-05-13）。
- **不讲什么**：各调用方的业务语义（08/09/10/11/13/14/17）；VFS 侧实现（`05-stage-vfs`）；`sys_datacopy` 等外部签名（18）。
- **前置**：04。**后置**：06/07/08/09/10/11/13/14/17/18/99。
- **事实底线（ground truth）**：`main.c:294-424`；`utility.c:123-139`；`include/minix/com.h:513-583`；`forkexit.c:130,230`；`exec.c:52`；`getset.c:219`；`signal.c:767`；`misc.c:230`；Rust：`minix-types/src/ipc/vfs.rs`；`pm/src/ipc/vfs.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-05-01 | 协作场景 | 动机 |
| K-05-02 | 异步必要性 | 动机 |
| K-05-03 | continuation | 机制 |
| K-05-04 | 编号与寻址键 | 协议面 |
| K-05-05 | 载荷布局 | wire |
| K-05-06 | tell_vfs | 发送方 |
| K-05-07 | handle_vfs_reply 四段 | 收口 |
| K-05-08 | 11 路行为 | 核心表 |
| K-05-09 | 七调用点 | 地图 |
| K-05-10 | 两会话旗标 | 状态纪律 |
| K-05-11 | 启动握手 | boot 接缝 |
| K-05-12 | Rust 类型化 | 实现映射 |
| K-05-13 | 错误保真规约 | 跨服务错误 |

- **验收标准**：① 12 请求/11 回复全表，每行给"值 / 触发时机 / 载荷字段 / 承载篇"（值只引 99）；② `tell_vfs` 的三段式与 not-idle panic 有锚点；③ 七个调用点表修正旧文把 `do_srv_fork` 写成 `VFS_PM_EXIT` 的错误，并给出正确行号；④ `NEW_PARENT`/`UNPAUSED` 的生命周期（何时置、何时清、入口断言）有锚点；⑤ 启动握手（逐条 + 屏障）与 Rust `vfs_init_sync` 对应。

### 06-event-subscription：进程事件发布/订阅

- **一句话定位**：非核心服务如何安全地知道"某进程被信号打断/正在退出"——一个有界、串行、可撤销的发布订阅设施。
- **讲什么**：事件用途（K-06-01）；事件与 wire（K-06-02）；串行与容量界（K-06-03，修正 `ASYN_NR` 归属）；订阅模型约束（K-06-04）；事件生命周期链（K-06-05/K-06-06）；重入守卫（K-06-07）；订阅表布局（K-06-08）；`resume_event`/`remove_sub`/`do_proceventmask`/`do_proc_event_reply`/服务死亡清理（K-06-09…K-06-13）；Rust 形态（K-06-14）。
- **不讲什么**：`exit_restart`/`restart_sigs` 的完整机制（09/11，本篇只写调用点）；`tell_vfs` 与异步容量共性（05）；`asynsend3` 的内核实现（01-stage-kernel）。
- **前置**：02、05。**后置**：09/18/99。
- **事实底线（ground truth）**：`event.c:1-353`；`mproc.h:27,104`；`const.h:13`；`include/minix/com.h:597-619`；`ipc.h:1806-1813`；`syslib.h:289-293`；`lib/libsys/asynsend.c:17-18`；Rust：`pm/src/event.rs`、`mproc/block.rs`、`minix-types/src/ipc/event.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-06-01 | 事件用途 | 动机 |
| K-06-02 | 事件与 wire | 协议 |
| K-06-03 | 串行与容量界 | 有界性 |
| K-06-04 | 订阅模型约束 | 语义限制 |
| K-06-05 | 生命周期链 | 心智模型 |
| K-06-06 | 游标 | 延续 |
| K-06-07 | 重入守卫 | 安全 |
| K-06-08 | 表布局 | 数据 |
| K-06-09 | resume_event | 引擎 |
| K-06-10 | remove_sub | 删除 |
| K-06-11 | 订阅矩阵 | 接口 |
| K-06-12 | 回复校验 | 接口 |
| K-06-13 | 死亡清理 | 卫生 |
| K-06-14 | Rust 形态 | 实现映射 |

- **验收标准**：① 用一次 EXIT 事件走完整链（publish→resume→reply→remove）并给每步锚点；② 有界性论证使用用户态 `msgtable[ASYN_NR]` 的事实（修正旧文的"内核预留队列"）；③ "不按进程订阅"的竞态原因写清；④ 回复校验的六条失败路径与 `SUSPEND` 语义完整；⑤ 服务死亡清理与 `nested` 守卫的关系讲清。

### 07-scheduling：调度交接

- **一句话定位**：把 INIT 和它的后代交给用户态 SCHED——接管、继承、改 nice 三条路径与 `nice↔queue` 双射。
- **讲什么**：内核原语 vs 策略（K-07-01）；`sched_init` 接管（K-07-02/K-07-03）；SCHED 协议与操作码真值（K-07-04/K-07-07）；`sched_start_user`（K-07-05）；`sched_nice`（K-07-06）；`nice↔queue` 双射（K-07-08）；`do_getsetpriority`（K-07-09）；`SEND_PRIORITY`/`SEND_TIME_SLICE`（K-07-10）；Rust 形态（K-07-11）。
- **不讲什么**：SCHED 服务内部策略（`06-stage-sched`）；内核调度器实现（01-stage-kernel）；fork 的完整流程（08，只给调用点）；权限判定的一般规则（13，本篇只用一个谓词）。
- **前置**：01、04。**后置**：08/17/18/99。
- **事实底线（ground truth）**：`schedule.c:20-112`；`main.c:199-200,213,275-289`；`utility.c:91-103`；`misc.c:239-286`；`include/minix/sched.h:7-12`；`include/minix/com.h:801-807`；`include/minix/config.h:66-74`；`sys/sys/resource.h:43-44`；`servers/pm/const.h:19-20`；Rust：`pm/src/sched.rs`、`mproc/mproc.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-07-01 | 原语与策略分离 | 动机 |
| K-07-02 | sched_init | 启动接管 |
| K-07-03 | 接管过滤 | 边界 |
| K-07-04 | SCHED 协议 | 接口 |
| K-07-05 | sched_start_user | fork 路径 |
| K-07-06 | sched_nice | 属性变更 |
| K-07-07 | 操作码真值 | 常量勘误 |
| K-07-08 | nice↔queue | 转换 |
| K-07-09 | 优先级调用 | 入口 |
| K-07-10 | 调度消息码 | 缺口补充 |
| K-07-11 | Rust 形态 | 实现映射 |

- **验收标准**：① `sched_init` 的扫描条件、两断言、失败只告警有锚点；② `SCHEDULING_*` 六个操作码给出 0xF00–0xF05 真值（E3 勘误）；③ `nice↔queue` 用两个具体例子验证（0→7→0、1→8→2），并解释 41/16 的量化误差；④ `sched_start_user` 的 `PRIV_PROC` 父→INIT 分支有锚点；⑤ `do_getsetpriority` 的五重判定（which/who/三重权限/EPERM/降低需 root）完整。

### 08-process-create：进程创建（fork 与 srv_fork）

- **一句话定位**：一个进程如何诞生——九步骨架只讲一次，srv_fork 用差异表收束，两阶段设计轴贯穿。
- **讲什么**：fork 语义与多服务协同（K-08-01/K-08-02）；两阶段与不可失败窗口（K-08-03/K-08-04）；三身份正交与延续（K-08-05/K-08-06）；容量与槽位轮转（K-08-07/K-08-08）；复制与继承（K-08-09/K-08-10/K-08-11）；VFS 投递与后半（K-08-12/K-08-13）；srv_fork 五差异（K-08-14/K-08-15）；Rust 形态与缺口（K-08-16/K-08-17）；调度调用点（K-08-18）。
- **不讲什么**：`vm_fork` 的 VM 侧实现（`02-stage-vm/18-vm-fork.md`）；`VFS_PM_FORK` 的收口状态机（05，本篇只给分支）；`sig_proc(SIGSTOP)` 的投递机制（10，只给调用点）；`get_free_pid` 算法（03）；`sched_start_user` 机制（07）。
- **前置**：03、05、07。**后置**：09/10/18/99。
- **事实底线（ground truth）**：`forkexit.c:44-240`；`main.c:369-396`；`utility.c:34-51,123-139`；`include/minix/com.h:527-528,541,578-580`；`kernel/system/do_fork.c`（generation）；Rust：`pm/src/fork.rs`、`mproc/fork.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-08-01 | fork 语义 | 概念 |
| K-08-02 | 三表一致窗口 | 动机 |
| K-08-03 | 两阶段 | 设计轴 |
| K-08-04 | 不可失败窗口 | 核心约束 |
| K-08-05 | 三身份正交 | 数据 |
| K-08-06 | 延续在子 | 机制 |
| K-08-07 | 容量闸门 | 边界 |
| K-08-08 | 槽位轮转 | 机制 |
| K-08-09 | 整槽复制 | 机制 |
| K-08-10 | 继承掩码 | 边界 |
| K-08-11 | PRIV_PROC 父 | 边界 |
| K-08-12 | VFS 投递 | 协议 |
| K-08-13 | fork 后半 | 收尾 |
| K-08-14 | srv_fork 差异 | 对比 |
| K-08-15 | SRV_FORK 回复 | 协议勘误 |
| K-08-16 | Rust 形态 | 实现映射 |
| K-08-17 | Rust 缺口 | 如实标注 |
| K-08-18 | 调度调用点 | 接缝 |

- **验收标准**：① 九步骨架表（每步输入/动作/失败回滚），srv_fork 用一张五差异表对照；② 两阶段分界（`vm_fork` 之后不失败）有锚点与理由；③ 普通 fork 与 srv_fork 的旗标继承差异正确（`IN_USE + DELAY_CALL + TAINTED` vs `IN_USE + PRIV_PROC + DELAY_CALL`，用 + 连接避免表格歧义）；④ `VFS_PM_SRV_FORK_REPLY` 值写 0x988（E10）；⑤ Rust 缺口如实写"代码当前丢弃 PRIV_PROC/无 `SRV_FORK_INHERIT_FLAGS`"（E18），不照抄旧文。

### 09-exit-wait：终止与回收

- **一句话定位**：一条生产–消费管线——exit 把进程变成僵尸，wait 收取它；中间隔着 VFS 确认、收养、tracer 伪父与事件发布。
- **讲什么**：两阶段退出与顺序理由（K-09-01/K-09-02）；PRIV_PROC 直毁（K-09-03）；僵尸模型与状态位（K-09-04/K-09-05）；收养与 SIGHUP（K-09-06/K-09-07）；事件发布位置（K-09-08）；`do_exit` 门与 SUSPEND（K-09-09）；core 抑制与记账（K-09-10/K-09-11）；强制停止与特例（K-09-12）；EXIT/DUMPCORE payload（K-09-13）；旗标收窄（K-09-14）；`exit_restart`（K-09-15）；僵尸通知引擎与 tracer 死亡（K-09-16/K-09-17）；`cleanup`（K-09-18）；wait4 四态/三环/双条件/消费动作（K-09-19…K-09-25）；rusage（K-09-26）；Rust 形态（K-09-27）。
- **不讲什么**：`VFS_PM_EXIT_REPLY` 的状态机（05，本篇只给分支）；`sig_proc`/`SIGCHLD`/`SIGHUP` 的判定与投递（10）；`sched_stop`/`vm_exit` 的对端实现（07/`02-stage-vm`）；`publish_event` 的注册表机制（06）。
- **前置**：03、05、06、08。**后置**：10/11/15/17/18/99。
- **事实底线（ground truth）**：`forkexit.c:245-806`；`main.c:80-82,304-312,356-367`；`utility.c:144-156`；`include/minix/com.h:524-525`；`sys/sys/wait.h:55-79`；Rust：`pm/src/{exit,wait}.rs`、`mproc/{lifecycle,guardianship,wait}.rs`。
- **知识点清单**：K-09-01…27（见 §2.10；本篇是旧 09+10 的合并，清理重复后共 27 条）。
- **验收标准**：① 一张"从 exit 到回收"的时序图（do_exit → exit_proc 前半 → VFS_PM_EXIT → 事件 → exit_restart 后半 → zombify → wait4 三环 → cleanup）；② 两阶段各步骤与 C 行一一对应；③ 收养循环的 `NEW_PARENT` 与 `VFS_CALL` 交互有锚点；④ 三环优先级与 `TOLD_PARENT` 防重有锚点；⑤ rusage 只在 `tell_parent` 拷贝且失败留僵尸；⑥ 五保留位与 `EXITING` 的初始值写对。

### 10-signal-core：信号模型

- **一句话定位**：PM 作为全系统信号权威的核心判定——谁被杀、能不能杀、怎么处置。
- **讲什么**：两种来源与唯一权威（K-10-01）；`pid` 四态（K-10-02）；逆序扫描与两个特例（K-10-03/K-10-04/K-10-05）；权限四重与 PRIV_PROC 门（K-10-06/K-10-07）；VM 豁免（K-10-08）；返回值与自杀（K-10-09）；`sig_proc` 十条处置链（K-10-10…K-10-17）；`sig_proc_exit` 与 core 集合（K-10-18/K-10-19）；`process_ksig` 与内核信号回环（K-10-20…K-10-22）；两条入口（K-10-23）；系统信号消息化（K-10-24）；SEF getksig 循环（K-10-25）；Rust 形态（K-10-26）。
- **不讲什么**：安装语义与 `sigmsg`（11）；停止/恢复与 SIGSNDELAY 兑现（11）；itimer 的 SIGALRM 产生（12）；tracer 机制（15）；`SIGCHLD`/`SIGHUP` 的业务触发点（09）。
- **前置**：04、05、09。**后置**：11/12/14/15/18/99。
- **事实底线（ground truth）**：`signal.c:197-221,294-378,411-563,568-646`；`main.c:137-141`；`sys/sys/signal.h:45,281-286`；`include/minix/com.h:601`；`lib/libsys/sef_signal.c`；Rust：`pm/src/signal.rs`、`mproc/signal.rs`。
- **知识点清单**：K-10-01…26（见 §2.11）。
- **验收标准**：① `SIGS_IS_LETHAL`/`TERMINATION`/`STACKTRACE` 三集合逐元素写对（E4），并说明 LETHAL ⊆ core 集合；② `sig_proc` 十条分支按源码顺序成表（E-勘误：不是 9 链）；③ 权限四重与广播跳过 PRIV_PROC、VM 豁免各有锚点；④ `check_sig` 的返回语义（count/error_code/自杀 SUSPEND）完整；⑤ 内核信号回环画出"SYSTEM 通知 → SEF getksig/endksig → process_ksig"的三段图（K-10-25），并标注 `SIGSNDELAY` 不由 kill 产生（E15）。

### 11-signal-handling：信号安装、投递与恢复

- **一句话定位**：信号从"装好动作"到"推进栈帧"再到"从阻塞中恢复"的完整机器——含停止原语与 `SIGSNDELAY` 兑现。
- **讲什么**：安装语义与三态不对称（K-11-01/K-11-02/K-11-03/K-11-04）；掩码四 `how`（K-11-05…K-11-07）；`sigsuspend`/`sigreturn` 成对（K-11-08/K-11-09）；`sigaction` 协议（K-11-10）；`sig_send` 五步与旗标（K-11-11…K-11-14）；两条收尾路径（K-11-15/K-11-16）；`stop_proc` 与 EBUSY/SIGSNDELAY 配对（K-11-17/K-11-18）；`PROC_STOPPED` 双用途（K-11-19）；`unpause` 三路径与 VFS 往返（K-11-20/K-11-21）；`check_pending` break 与 `restart_sigs`（K-11-22/K-11-23）；`try_resume`（K-11-24）；SIGSNDELAY 不可 kill（K-11-25）；Rust 形态（K-11-26）。
- **不讲什么**：目标选择与权限（10）；`sig_proc` 的处置优先级（10，本篇从 caught 分支接手）；内核 `do_sigsend` 的栈帧推入（01-stage-kernel）；ITIMER 的到期产生（12）。
- **前置**：05、10。**后置**：12/14/15/18/99。
- **事实底线（ground truth）**：`signal.c:40-192,226-289,651-770,776-855`；`type.h:71-77`（`sigmsg` 五字段）；`sys/sys/signal.h:45,97-179,264-277`；`ipc.h:531-550`；Rust：`pm/src/{signal_handlers,signal_flow}.rs`、`mproc/{signal,block}.rs`。
- **知识点清单**：K-11-01…26（见 §2.12）。
- **验收标准**：① `sigaction` 三态转移表（IGN/DFL/handler × ignore/catch/pending）与 C 行对应；② `sigmsg` 五字段列出，且明确指出 `sa_mask` 累加进 `mp_sigmask`（E13）；③ `SIGKILL` 早退与 `SIGSTOP` 的宽松点都要如实写；④ `stop_proc` 的 OK/EBUSY/panic 三分支与 `may_delay` 契约有锚点；⑤ `SIGSNDELAY` 兑现路径（清 DELAY_CALL → VFS/EVENT 在途则 stop → 否则 check_pending）完整；⑥ `unpause` 四个判定顺序正确。

### 12-timer：定时器

- **一句话定位**：三个 interval timer 的两套后端与一条 ticks↔timeval 变换链，以及 SIGALRM 的到期投递。
- **讲什么**：三族分野（K-12-01）；两套后端（K-12-02/K-12-03）；取整与溢出钳位（K-12-04/K-12-05/K-12-06）；输入校验与上限（K-12-07/K-12-16/K-12-17）；`do_itimer` 协议（K-12-08/K-12-09）；vtimer 的 NULL 语义与回绕（K-12-10/K-12-11/K-12-12）；realtimer 读写（K-12-13/K-12-14）；`cause_sigalrm`（K-12-15）；CLOCK 驱动（K-12-18）；Rust 形态（K-12-19）。
- **不讲什么**：内核 timer 队列与 `TMRDIFF_MAX` 实现（01-stage-kernel/15）；信号处置（10/11）；`system_hz` 获取（01/16）。
- **前置**：04、10。**后置**：18/99。
- **事实底线（ground truth）**：`alarm.c:22-344`；`servers/pm/const.h:15,17`；`minix/timers.h:45`；`sys/sys/time.h:264-266`；Rust：`pm/src/timer.rs`、`mproc/mproc.rs`。
- **知识点清单**：K-12-01…19（见 §2.13）。
- **验收标准**：① `MAX_SECS` 按 `TMRDIFF_MAX/system_hz` 定义写对（E7），并指出 Rust `100_000_000` 硬编码与 C 不一致；② 取整/钳位/分解三个变换各给一个数值例子；③ REAL 与 VIRTUAL/PROF 的后端差异表（谁持有状态、走哪个调用）；④ `getset_vtimer` 的"到期回绕 interval"规则有锚点；⑤ `cause_sigalrm` 的三重守卫与周期重设有锚点；⑥ `check_vtimer` 在 `process_ksig` 的调用点回指 10。

### 13-credentials：凭证与身份变更

- **一句话定位**：PM 如何维持 real/effective/saved 三元组、组列表与 TAINTED 位，并在每次成功修改后同步 VFS。
- **讲什么**：三元组模型（K-13-01）；SETUID/SETEUID/SETGID/SETEGID 语义（K-13-02…K-13-04）；VFS 双副本同步（K-13-05）；GETGROUPS 两阶段与组数边界（K-13-06/K-13-07/K-13-08）；SETGROUPS（K-13-09/K-13-10）；setsid/getsid 会话语义（K-13-11/K-13-12）；TAINTED（K-13-13）；GETUID/GETGID 双通道（K-13-14）；13 调用枚举（K-13-15）；Rust 形态（K-13-16）。
- **不讲什么**：exec 的 setuid 位与 TAINTED 更新（14）；fork 的 TAINTED 继承（08）；信号权限四重（10）；VFS fproc 字段（`05-stage-vfs`）。
- **前置**：02、05。**后置**：14/17/18/99。
- **事实底线（ground truth）**：`getset.c:19-222`；`main.c:334-347`；`mproc.h:41-46,103`；`sys/sys/syslimits.h:53,59`；`ipc.h:456-475,529-550`；Rust：`pm/src/credentials.rs`、`mproc/credentials.rs`。
- **知识点清单**：K-13-01…16（见 §2.14）。
- **验收标准**：① 13 个调用逐一列出（7 个 `do_get` 分支 + 6 个 `do_set` 分支），修正旧文只列 11 个；② `SETUID` 的 BSD 全置与 `SETEUID` 三重校验的条件表达式与 C 行一致；③ 组列表上限与 `GID_MAX` 真值正确（E8）；④ SETSID 回复 `mp_procgrp`（E11）；⑤ VFS 转发使用统一 `tell_vfs` + `SUSPEND`，四类回复的唤醒值不同（SETSID 例外）有锚点。

### 14-exec：执行替换

- **一句话定位**：exec 三阶段——PM 转发给 VFS、VFS 回调建新地址空间、PM 收尾更新凭证与信号状态。
- **讲什么**：VFS/PM 分工（K-14-01）；`do_exec` 六字段（K-14-02）；调用门与参数取入（K-14-03/K-14-04）；`allow_setuid` 与凭证更新（K-14-05/K-14-06）；TAINTED 二重（K-14-07）；name/frame 保存（K-14-08）；`PARTIAL_EXEC`（K-14-09）；suid 回复（K-14-10）；`do_execrestart`（K-14-11）；失败路径（K-14-12）；catch 重置（K-14-13）；tracer 信号（K-14-14）；`sys_exec` 四元（K-14-15）；`TO_*` 真值（K-14-16）；Rust 形态（K-14-17）。
- **不讲什么**：VFS 侧 `read_header`/libexec 装载（`05-stage-vfs`）；VM 重新映射（`02-stage-vm`）；`reset_caught_for_exec` 的信号语义（11，本篇只给调用点）；tracer 状态机（15）。
- **前置**：05、11、13。**后置**：15/18/99。
- **事实底线（ground truth）**：`exec.c:21-200`；`main.c:349-354`；`mproc.h:71-72,99,103`；`sys/sys/ptrace.h:209-211`；`minix/lib/libexec/libexec.h:23-58`（`exec_info`）；`ipc.h:433-443,963-972`；Rust：`pm/src/exec.rs`、`mproc/{mproc,signal}.rs`。
- **知识点清单**：K-14-01…17（见 §2.15）。
- **验收标准**：① 三阶段时序图（`do_exec` → VFS → `do_newexec`/`exec_restart`）含 RS 专用分支；② `PARTIAL_EXEC` 的置位/清除/失败自毁三处有锚点；③ `TO_NOEXEC=0x4` 写对（E6）；④ `exec_info` 的定义位置写对（`libexec/libexec.h`，E-勘误）；⑤ catch 重置只清 caught、ignore 保留，有锚点；⑥ tracer 信号在 `sys_exec` 之前投递，有锚点。

### 15-trace：ptrace 调试

- **一句话定位**：PM 侧的调试状态机——调试器如何附着、被调试进程如何停在信号边界、19 个命令如何分流。
- **讲什么**：双入口与 attach 守卫（K-15-01…K-15-05）；T_STOP/READB/WRITEB_INS（K-15-06/K-15-07）；共同守卫（K-15-08）；T_EXIT/SETOPT/GETRANGE/SETRANGE/DETACH/RESUME（K-15-09…K-15-13）；透传尾部（K-15-14）；`trace_stop`（K-15-15）；双轨暂停与 sigtrace（K-15-16/K-15-17）；常量表（K-15-18）；Rust 形态（K-15-19）。
- **不讲什么**：内核 `sys_trace` 实现（01-stage-kernel `do_trace.c`）；`tracer_died` 的收养分支（09，本篇引用）；`wait4` 的 tracer 环（09）；信号处置链（10）。
- **前置**：09、10、11、14。**后置**：18/99。
- **事实底线（ground truth）**：`trace.c:42-276`；`signal.c:411-422`（TRACE-first）；`mproc.h:34,89,93,100`；`sys/sys/ptrace.h:209-211,226-250`；`sys/sys/wait.h:67`；Rust：`pm/src/trace.rs`、`mproc/{trace,guardianship}.rs`。
- **知识点清单**：K-15-01…19（见 §2.16）。
- **验收标准**：① T_* 19 个命令按"PM 全处理 / PM+内核 / 内核透传"三组分类，代表命令讲透；② T_ATTACH 七重守卫逐条对应 C 行；③ `TRACE_STOPPED` 与 `PROC_STOPPED` 的区别有锚点；④ T_DETACH 的 sigtrace 重放顺序有锚点；⑤ `trace_stop` 的"内核停→置位→可选回 wait"顺序有锚点。

### 16-time：时间

- **一句话定位**：PM 如何把内核的滴答（ticks/realtime/boottime）变成两种时钟的 `timespec`，以及五个时间调用的边界。
- **讲什么**：双时钟（K-16-01/K-16-02）；`getuptime` 三值（K-16-03）；组合与分解公式（K-16-04/K-16-05）；分辨率（K-16-06）；settime 的 now 分支与 root 门（K-16-07）；`do_time`/`do_stime`（K-16-08/K-16-09）；`clock_time` 溢出规避与 hz 上限（K-16-10/K-16-11）；五调用表（K-16-12）；Rust 形态（K-16-13）。
- **不讲什么**：内核 `do_settime` 的 adjtime 渐变与 `do_stime` 的 `set_boottime` 落点（01-stage-kernel，本篇只给契约一句）；itimer 的 ticks 变换（12）；rusage 的 ticks 换算（09）。
- **前置**：02、04。**后置**：18/99。
- **事实底线（ground truth）**：`time.c:22-131`；`lib/libsys/{getuptime.c,clock_time.c}`；`kernel/system/{do_settime.c,do_stime.c}`；`sys/sys/time.h:283,288`；`callnr.h:20,41,46-48`（PM 时间调用）；Rust：`pm/src/time.rs`、`minix-types/src/types/clock.rs`。
- **知识点清单**：K-16-01…13（见 §2.17）。
- **验收标准**：① 双时钟的 `clock_id` 值与默认 EINVAL 行为有锚点；② `sec`/`nsec` 分解公式用具体数值验证（例如 ticks=150、hz=100）；③ `clock_time` 的 32 位规避算法讲清（含 `LONG_MAX/40000` 分界），删除旧文的"hz=5e4 退化"错误结论（E25）；④ `monotonic` 不可设置、`settime` 的 now 参数语义有锚点；⑤ 五调用与各自调用号对齐。

### 17-misc-queries：杂项与查询

- **一句话定位**：PM 的十一个外围入口——两个查询、两个解析、一组控制面、资源统计与两个 feature 面。
- **讲什么**：`uts_tbl` 与 uname（K-17-01/K-17-02）；`getsysinfo`（K-17-03…K-17-05）；endpoint↔pid 解析（K-17-06/K-17-07）；reboot 定序（K-17-08/K-17-09）；svrctl（K-17-10/K-17-11）；getrusage（K-17-12）；sprofile/mcontext（K-17-13/K-17-14）；`find_param`（K-17-15）；Rust 常量缺口（K-17-16）；调用号表（K-17-17）。
- **不讲什么**：信号与调度相关调用（10/11/07）；`sys_*` 签名（18）；`VFS_PM_REBOOT` 的状态机（05）；`set_rusage_times` 的换算（09）。
- **前置**：03、05、09。**后置**：18/99。
- **事实底线（ground truth）**：`misc.c:32-60,71-100,107-144,149-193,198-233,239-286,291-395,400-447`；`profile.c:22-45`；`mcontext.c:15-25`；`utility.c:57-71`；`include/minix/sysinfo.h:11,14`；`sys/sys/reboot.h:41-54`；`callnr.h`；Rust：`pm/src/misc.rs`。
- **知识点清单**：K-17-01…17（见 §2.18）。
- **验收标准**：① `SI_PROC_TAB=2`、`SI_CALL_STATS=9` 写对（E1 勘误），并指出 Rust `misc.rs` 的 0/1 是待修 bug（K-17-16）；② `RB_POWERDOWN=0x808` 写对（E2），且说明 readclock 通知条件；③ `uts_tbl` 9 槽 3 NULL 写对（E19），字段合法集合 {0,2,4,5,6,7}；④ reboot 四步定序有锚点；⑤ getsysinfo 的 root 门与 size 精确匹配有锚点；⑥ `getepinfo` 的组列表截断规则有锚点。

### 18-external-interfaces：外部接口契约（新建）

- **一句话定位**：PM 所有对外调用的唯一签名权威面——其它篇只写调用点与语义，不复制签名。
- **讲什么**：单点权威理由与依赖面总图（K-18-01/K-18-02）；`sys_*` 表（K-18-03）；`srv_*`/`vm_*` 表（K-18-04）；SCHED 客户端表（K-18-05）；VFS 协议索引（K-18-06）；同步/异步规则（K-18-07）；启动参数缺口（K-18-08）；KernelGateway 与接线模型（K-18-09）；`mproc` 镜像与消费方（K-18-10）；错误保真规约（K-18-11）；minix-types 缺口（K-18-12）。
- **不讲什么**：任何机制语义（指向各篇）；内核/VM/VFS/SCHED 侧实现；常量数值（99）；Rust 实现细节（只在 §Rust 面盘点）。
- **前置**：01–17（签名汇总；按需查阅）。**后置**：无（参考面）。
- **事实底线（ground truth）**：`lib/libsys/*`（`vm_fork.c`、`asynsend.c`、`getuptime.c` 等）；`include/minix/{com.h,ipc.h,callnr.h,sched.h}`；各篇调用点锚点；`edge_todo.md` E5/E6/E7；`os/servers/pm/src/{exit.rs,ipc/transport.rs,ipc/vfs.rs,mproc/wire.rs,main.rs}`；`02-stage-vm/18-vm-fork.md`；`06-stage-sched`；`05-stage-vfs`。
- **知识点清单**：K-18-01…12（见 §2.19；本篇为新建，原料来自散落各篇与 edge_todo）。
- **验收标准**：① 六类面每类一张表：函数/消息、签名要点、PM 调用点（篇 + 函数 + 行）、失败语义；② 每个外部调用在机制篇都有"调用点"提示，且本篇是唯一写签名的地方（单点权威自检）；③ 同步/异步规则至少覆盖 `tell_vfs`（asynsend）、`sched_nice`（taskcall）、`sys_*`（同步）三类；④ 如实登记三个缺口：GETMONPARAMS/GETIMAGE 无对端、minix-types PM wire 未系统化、`SigSet` 装不下 70/73/74 的跨层问题；⑤ Rust 面给出 KernelGateway 与域 trait 清单（不写接线条数）。

### 99-global-concepts：常量、调用号与 wire 词典

- **一句话定位**：本 stage 的查表终点——身份/容量常量、19 旗标、信号集合、47 调用、四类协议码值与 Rust 落点集中一处。
- **讲什么**：词典规则（K-99-01）；身份与容量常量（K-99-02）；19 旗标位值（K-99-03）；信号集合与信号号（K-99-04）；endpoint 编码（K-99-05）；47 调用号（K-99-06）；`VFS_PM_*` 全表（K-99-07）；`SUSPEND` 与回复约定（K-99-08）；八个常量族（K-99-09）；时间/容量魔数（K-99-10）；Rust 落点映射（K-99-11）；排除项（K-99-12）；错误码族（K-99-13）；测试基建边界（K-99-14）。
- **不讲什么**：任何机制语义（各篇自述）；测试计数与实现状态；内核侧实现。
- **前置**：无（词典；正确使用需配合机制篇）。**后置**：全部篇的常量引用都指向本篇。
- **事实底线（ground truth）**：`servers/pm/{const.h,mproc.h,glo.h}`；`include/minix/{com.h,callnr.h,sysinfo.h}`；`sys/sys/{signal.h,sigtypes.h,reboot.h,wait.h,ptrace.h,time.h,resource.h,syslimits.h}`；`endpoint.h`；Rust `mproc/constants.rs` 与 minix-types 各模块；`Cargo.toml`。
- **知识点清单**：K-99-01…14（见 §2.20）。
- **验收标准**：① 每个常量族一张表：名字、值、C 锚点、Rust 落点；② 数值覆盖率可 grep 对账（19 旗标、47 调用、12 请求 + 11 回复、19 T_*、TO_*、SI_*、RB_*）；③ `SI_PROC_TAB=2`、`RB_POWERDOWN=0x808`、`SCHEDULING_*`、`MAX_SECS`、`GID_MAX`、`TO_NOEXEC` 六处勘误全部落表；④ 排除项逐条理由（`ENABLE_SYSCALL_STATS`、`SPROFILE`、`ESCRIPT`、uts 兼容块、C 构建文件）；⑤ 每个 Rust 落点唯一（无重复定义）。

---

## 6. 变更表

> 双方向规则：**存量方向看去向**——拆分/合并涉及的旧知识点逐条给出新位置；**新增方向看来源**——现有文档没有的知识点给证据锚点（§3.2 + §7）。旧文档在新目录落地后整体归档不删（B 相执行）。

| 操作编号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点（去向） |
|---------|------|--------|--------|------|------------------|
| O-01 | 重写保留 | 00 | 新 00 | 导航职责不变；删除过期接线/测试快照，改"接线模型" | K-00-01…10 |
| O-02 | 重写+补缺 | 01 | 新 01 | 修正 schedule/sef 锚点；补装载/抑制/放行链与重启路径 | K-01-01…17 |
| O-03 | 重写 | 02 | 新 02 | 字段数勘误；补 `mproc` 字节镜像 | K-02-01…16 |
| O-04 | 重写 | 03 | 新 03 | 修正 `cleanup` 符号误标；generation/容量讲清 | K-03-01…12 |
| O-05 | 重写 | 04 | 新 04 | 47 调用表与接线状态去过期；handler 归属改指新篇号 | K-04-01…12 |
| O-06 | 改名+重写 | 05 | 新 05 | `05-vfs-interaction` → `05-vfs-protocol`；修正 SRV_FORK 行与 SETSID 回复 | K-05-01…13 |
| O-07 | 重写 | 06 | 新 06 | 修正 `ASYN_NR` 归属；去测试快照；补死亡清理 | K-06-01…14 |
| O-08 | 重排+重写 | 16 | 新 07 | 调度前移（boot 第 8 步 + fork 继承）；修正 SCHEDULING 真值与 nice 例；补 `SEND_*` | K-07-01…11 |
| O-09 | 合并 | 07 + 08 | 新 08 | 九步骨架只讲一次 + 五差异表；修正 SRV_FORK_REPLY 与 Rust 缺口 | K-08-01…18 |
| O-10 | 合并 | 09 + 10 | 新 09 | exit/wait 是一条生产–消费管线；三环与僵尸引擎合讲 | K-09-01…27 |
| O-11 | 重写 | 11 | 新 10 | 修正 LETHAL 集合、`_NSIG`、VM 锚点、9→10 分支；补内核信号回环 | K-10-01…26 |
| O-12 | 合并 | 12 + 13 | 新 11 | 安装/投递/恢复一条机器；修正 `sm_mask` 目标与 sigmsg 字段；SIGSNDELAY 勘误 | K-11-01…26 |
| O-13 | 重排 | 14 | 新 12 | 编号前移；修正 MAX_SECS 与 Rust 硬编码 | K-12-01…19 |
| O-14 | 重排 | 15 | 新 13 | 编号前移；修正 GID_MAX/SETSID/13 调用枚举 | K-13-01…16 |
| O-15 | 重排 | 17 | 新 14 | 编号前移进生命周期段；修正 TO_NOEXEC 与 exec_info 位置 | K-14-01…17 |
| O-16 | 重排 | 18 | 新 15 | 编号前移；锚点与常量勘误 | K-15-01…19 |
| O-17 | 重排 | 19 | 新 16 | 编号前移；删除 clock_time 错误结论 | K-16-01…13 |
| O-18 | 重排 | 20 | 新 17 | 编号前移；SI/RB/uts/RB_KEXEC 勘误；登记 Rust 常量 bug | K-17-01…17 |
| O-19 | 新建 | —（散落） | 新 18 | 外部签名与 wire 的唯一权威面；填 G-05/G-06/G-07/G-09/G-10/G-12 | K-18-01…12 |
| O-20 | 重写+扩写 | 99 | 新 99 | 从 93 行薄词典扩为完整常量/调用号/wire 词典；六处勘误落表 | K-99-01…14 |
| O-21 | 归档 | 旧 `00`–`20`、`99` 共 22 个文件 | — | 重建完成后整体归档（B 相执行，不删） | 全部；迁移表见 §8.1 |

**删除条款（不进新正文，逐条给理由）**：

| 删除内容 | 旧位置 | 理由 |
|---------|--------|------|
| 测试计数与基线快照（91/101/116/166/208/226/242/260/278/320/328/369/381 等） | 几乎每篇 §5 | 易腐；抽查发现与当前实现不符；改为各篇验收标准 |
| "Fix #NN / R# / 批次 A–H / 接线落地"过程块 | 00 §3、04 §3.6/§4.2、06 §2.10、08 §2.4、09 §4、11 §4.1、13 §3/§4、14 §3.7、18 §4、20 §3 等 | 实现进度叙述不属于语义文档；有效结论已并回机制描述 |
| 隐藏设计目录引用（`*-design.v1.md`/`outline.v1.md`） | 约 21 处（05–20 各 1–2 处） | 项目规范：正式文档禁引中间产物 |
| 旧文档编号/文件名引用 | 全篇 | 重建后编号含义变化，按 §8.2 迁移 |
| `工具生成`式锚点标签（符号名错位） | 02/07/09/10/11/13/15 等大量 | 符号名与行号不可信；新篇锚点一律写"函数名 + 行号 + 文件" |
| 旧文对"40 个 ENOSYS/8 个接线"的具体计数 | 00 §3.1、04 §3.6/§5.2 | 与当前 41/47 不符；改写为接线模型 |

---

## 7. 缺漏新篇（缺口逐项落实）

> §3.2 的 20 项缺口全部落实（无"待定"）：18 为新篇，其余为其承载篇的指定小节。跨 stage 的显式排除项单列。

| 缺口 | 主题 | 为什么重要 | 原料（锚点） | 归哪一篇（小节） | 验收标准 |
|------|------|-----------|-------------|----------------|---------|
| G-01 | 装载/抑制/放行链 | 纠正"PM 由 RS 装载"的模糊叙述 | `kernel/main.c:196,253,265-266`；`table.c:55` | 新 01 §2 | 时序图三步带锚点 |
| G-02 | PM 重启语义 | 旧文只一行；影响恢复理解 | `main.c:119`；`sef_init.c` | 新 01 §5 | 明确 SEF 通用迁移与重对齐 |
| G-03 | 内核信号回环 | 内核信号到 PM 的完整路径 | `sef_signal.c`；`signal.h:273-274` | 新 10 §2 | 三段图 + 回调签名 |
| G-04 | mproc 字节镜像与消费 | 跨服务表 ABI | `mproc/wire.rs`；`misc.c:125-143` | 新 02 §4 + 新 18 §4 | 镜像用途与消费方清单 |
| G-05 | PM↔VM 调用面 | 签名散落 | `lib/libsys/vm_fork.c`；`02-stage-vm` | 新 18 §3 | 四调用各一行带调用点 |
| G-06 | SCHED 协议全表 | 旧 16 只有一句且值错 | `com.h:801-807`；`ipc.h:1820-1828` | 新 07 §4 + 新 18 §3 | 六操作码真值 + message 结构 |
| G-07 | `SEND_PRIORITY`/`SEND_TIME_SLICE` | 完全缺失 | `servers/pm/const.h:19-20` | 新 07 §4 | 两常量入 99 并说明用途 |
| G-08 | system.conf PM 声明与 sigmgr | 信号管理器归属链 | `etc/system.conf`；`priv.h:82-85` | 新 01 §2 + 新 18 §5 | 配置到 priv 字段的映射 |
| G-09 | minix-types PM 协议面缺口 | 实现路线 | `edge_todo.md` E7 | 新 18 §6 | 三缺口逐条 |
| G-10 | 启动参数缺口 | 通电前置 | `edge_todo.md` E6；`main.rs:15` | 新 01 §2 + 新 18 §4 | 明确无对端 |
| G-11 | Rust 常量 bug | 通电即错 | `misc.rs`；`timer.rs:19` | 新 17 §6 + 新 12 §5 | 与 C 对账并标"待修" |
| G-12 | procfs 消费者 | 可见性契约 | `exec.c:111-117`；`fs/procfs` | 新 02 §4 + 新 18 §4 | 消费字段清单 |
| G-13 | 进程组/会话汇总 | A-13 | `forkexit.c:298,412`；`getset.c:66-78,205-213` | 新 09 §4 + 新 13 §4 + 新 99 §2 | 一处完整语义链 |
| G-14 | SIGKSIGSM 与 PM 自身信号 | SEF 分发路径 | `signal.h:273`；`sef_signal.c` | 新 10 §2（K-10-25 邻域） | 两条内核信号分支 |
| G-15 | 47 调用完整枚举 | 导航与对账 | `callnr.h:14-60` | 新 04 §3 + 新 99 §3 | 每调用一行、两处互引 |
| G-16 | 错误保真规约 | 跨服务错误 | `plan.md:231` | 新 18 §5 | 规则 + 反例 |
| G-17 | `sys_datacopy` 用法 | 多处拷贝依赖 | `misc.c:90,143,185` 等 | 新 18 §3 | SELF 语义一句 + 调用点 |
| G-18 | 接线状态的非易腐表达 | 旧文数字过期 | `ipc/calls.rs`；`todo.md §11.1.1` | 新 00 §3 + 新 18 §6 | 只写模型不写计数 |
| G-19 | `ESCRIPT` 死代码边界 | 范围声明 | `exec.c:31`；`plan.md` §5.4 | 新 99 §6 | 排除项一行 |
| G-20 | 未实测数字清理 | 旧文两处体积断言 | `mproc.h` 字段 | 新 99 §6 | 不写未测数字 |

**跨 stage 显式排除（不算缺口）**：内核 `sys_*` 侧实现与 `do_fork.c` 的 generation 递增（01-stage-kernel）；VM 侧 `vm_fork`/`vm_willexit`/`vm_exit`/`vm_getrusage` 实现（02-stage-vm）；VFS 侧 fproc 与 12 类回复的实现（05-stage-vfs）；SCHED 服务内部策略（06-stage-sched）；MIB/procfs 消费实现（10-stage-mib / 15-stage-fs）；端到端联调（E5/E9，19-stage-integration）；`/etc/rc` 与用户命令（18-stage-commands）。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（旧文档逐节目录 → 新位置）

> 同去向的连续小节合并为一行。迁移类型：原样搬移/改写/合并/拆分/删除（删除仅指不进新正文，旧文件仍归档）。

**旧 00 / 01 / 02 / 03**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 00 §1.1-§1.3 | 谁管理进程/四重权威/主循环骨架 | 新 00 §1-§3 | 改写 | 机制细节改指各篇 |
| 00 §2.1-§2.3 | 文件映射/Rust 镜像/阅读路线 | 新 00 §4 | 改写 | 换新编号 |
| 00 §3.1-§3.3 | 实施现状/跨阶段依赖/测试基线 | 新 00 §3 | 改写/删除 | 删计数（E21） |
| 00 §4.1 | 端口面规约 | 新 00 §5 + 新 18 §6 | 拆分 | 细节归 18 |
| 00 §5-§7 | 测试点/过渡/参见 | 新 00 §6-§7 | 改写 | |
| 01 §1.0-§1.5 | 启动链/SEF/八步顺序/进程树/VFS 握手 | 新 01 §1-§3 | 改写 | 补装载链（G-01） |
| 01 §1.6 | 小结 | 新 01 §7 | 改写 | |
| 01 §2.1-§2.2 | main/sef_local_startup | 新 01 §2 | 原样搬移 | 锚点勘误 |
| 01 §2.3 | SEF 库与 process_init | 新 01 §2 | 改写 | 与 K-01-14 合并 |
| 01 §2.4 | 八步 | 新 01 §3 | 原样搬移 | |
| 01 §2.5 | reply/get_nice/handle_vfs_reply 归属 | 新 01 §2（一句）+ 04/05/07 | 拆分 | |
| 01 §2.6 | sched_init 调用点 | 新 01 §3 + 新 07 §2 | 拆分 | 机制归 07 |
| 01 §2.7 | 符号清单 | — | 删除 | 工具生成，无正文价值 |
| 01 §3.1-§3.8 | Rust 决策与 Redox 对照 | 新 01 §5 | 改写 | 去过程 |
| 01 §4.1-§4.8 | 实现详解 | 新 01 §6 | 改写 | |
| 01 §5 | 测试要点 | — | 删除 | 计数过期 |
| 01 §6-§7 | 过渡/参见 | 新 01 §7 | 改写 | |
| 02 §1.0-§1.6 | 多副本表/巨型结构/字段分组/分层/动作表 | 新 02 §1-§2 | 原样搬移 | |
| 02 §2.1 | struct mproc 总览 | 新 02 §2 | 改写 | 字段数勘误 E22 |
| 02 §2.2 | 19 旗标 | 新 02 §3 + 新 99 §2 | 拆分 | 数值归 99 |
| 02 §2.3 | 信号位图与 sigaction | 新 02 §3 | 原样搬移 | |
| 02 §2.4 | 初始化契约 | 新 02 §3 | 原样搬移 | |
| 02 §2.5 | fork 字段继承 | 新 02 §3（表）+ 新 08 §3 | 拆分 | 流程归 08 |
| 02 §2.6 | 符号清单 | — | 删除 | |
| 02 §3.1-§3.8 | Rust 分层/枚举/信号状态 | 新 02 §4 | 改写 | 补 wire（G-04） |
| 02 §4.1-§4.6 | 实现详解 | 新 02 §4 | 改写 | |
| 02 §5 | 测试要点 | — | 删除 | |
| 02 §6-§7 | 过渡/参见 | 新 02 §6 | 改写 | |
| 03 §1.0-§1.6 | 三层身份/两类操作/PID 稀缺/代际/容量 | 新 03 §1-§2 | 原样搬移 | |
| 03 §2.1-§2.7 | glo.h/isokendpt/find_proc/get_free_pid/slot/generation | 新 03 §2-§3 | 原样搬移 | 修 cleanup 误标 E23 |
| 03 §3.1-§3.8 | Rust 聚合/类型化 | 新 03 §4 | 改写 | |
| 03 §4.1-§4.7 | 实现与不变量 | 新 03 §4 | 改写 | |
| 03 §5 | 测试要点 | — | 删除 | |
| 03 §6-§7 | 过渡/参见 | 新 03 §5 | 改写 | |

**旧 04 / 05 / 06 / 07 / 08**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 04 §1.0-§1.6 | 事件循环/消息空间/三路/SUSPEND/错误哲学 | 新 04 §1-§2 | 原样搬移 | |
| 04 §2.1-§2.6 | 主循环/通知/caller/三路/reply/call_vec | 新 04 §2-§3 | 原样搬移 | |
| 04 §2.7 | 47 调用表 | 新 04 §3 + 新 99 §3 | 拆分 | 数值归 99；勘误 |
| 04 §2.8 | 调用统计 | 新 04 §5 + 新 99 §6 | 拆分 | feature 排除 |
| 04 §3.1-§3.6 | Rust 分派决策 | 新 04 §5 | 改写 | 删接线计数 E21 |
| 04 §4.1-§4.5 | 实现与不变量 | 新 04 §5 | 改写 | |
| 04 §5-§7 | 测试/过渡/参见 | 新 04 §6-§7 | 删除/改写 | |
| 05 §1 | 协作/异步/OS 对比 | 新 05 §1 | 原样搬移 | |
| 05 §2.1 | 协议面 | 新 05 §2 + 新 99 §3 | 拆分 | |
| 05 §2.2 | tell_vfs | 新 05 §3 | 原样搬移 | |
| 05 §2.3 | handle_vfs_reply | 新 05 §3 | 原样搬移 | 修 SETSID |
| 05 §2.4 | 七调用点 | 新 05 §3 | 改写 | 修 SRV_FORK 行 E24 |
| 05 §2.5 | NEW_PARENT/UNPAUSED | 新 05 §3 | 原样搬移 | |
| 05 §3/§4 | Rust 协议与实现 | 新 05 §4 | 改写 | |
| 05 §5-§7 | 测试/过渡/参见 | 新 05 §5 | 删除/改写 | |
| 06 §1.0-§1.7 | 事件动机/串行/约束/生命周期/对比 | 新 06 §1-§2 | 原样搬移 | 修 ASYN_NR E16 |
| 06 §2.1-§2.6 | 数据面/resume/remove/mask/reply/publish | 新 06 §2-§4 | 原样搬移 | |
| 06 §2.7 | 主循环与 VFS 衔接 | 新 06 §4（引用）+ 04/09 | 拆分 | |
| 06 §2.8-§2.10 | 对端容量/不变式/前瞻 | 新 06 §4-§5 | 改写 | 删过程 |
| 06 §3-§5 | Rust 与实现/测试 | 新 06 §5 | 改写/删除 | |
| 06 §6-§7 | 过渡/参见 | 新 06 §6 | 改写 | |
| 07 §1.0-§1.7 | fork 动机/两阶段/身份/延续/对比 | 新 08 §1-§2 | 合并改写 | 与 08 合并 |
| 07 §2.1-§2.10 | 容量/槽位/vm_fork/复制/PID/VFS/tracer | 新 08 §2-§3 | 合并 | 九步只讲一次 |
| 07 §3-§5 | Rust 决策/实现/测试 | 新 08 §5 | 改写/删除 | 修 E17 |
| 07 §6-§7 | 过渡/参见 | 新 08 §6 | 改写 | |
| 08 §1.0-§1.7 | srv_fork 动机/RS 门/回复/凭证注入/差异 | 新 08 §4 | 合并改写 | 五差异表 |
| 08 §2.1-§2.10 | 逐段 C（大部分与 07 重复） | 新 08 §3（差异）+ §4 | 合并 | 重复段删除 |
| 08 §3-§5 | Rust 决策/实现/测试 | 新 08 §5 | 改写 | 修 E18 |
| 08 §6-§7 | 过渡/参见 | 新 08 §6 | 改写 | |

**旧 09 / 10 / 11 / 12 / 13**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 09 §1.0-§1.7 | 两阶段/直毁/僵尸/收养/事件/对比 | 新 09 §1-§3 | 合并改写 | 与 10 合并 |
| 09 §2.1-§2.9 | do_exit/exit_proc/exit_restart/zombify 族 | 新 09 §4-§6 | 合并 | 锚点勘误 E23 |
| 09 §3-§5 | Rust/实现/测试 | 新 09 §7 | 改写/删除 | |
| 09 §6-§7 | 过渡/参见 | 新 09 §8 | 改写 | |
| 10 §1.0-§1.7 | wait 动机/pidarg/异步/tracer/rusage | 新 09 §2-§3 | 合并 | |
| 10 §2.1-§2.10 | wait4 三环/wait_test/tell_parent/tell_tracer/cleanup | 新 09 §5-§6 | 合并 | |
| 10 §3-§5 | Rust/实现/测试 | 新 09 §7 | 改写/删除 | 修测试名漂移 |
| 10 §6-§7 | 过渡/参见 | 新 09 §8 | 改写 | |
| 11 §1.0-§1.7 | 信号动机/pid 四态/PRIV/9 链/EDEADEPT/对比 | 新 10 §1-§3 | 改写 | E4/E5 勘误 |
| 11 §2.1-§2.8 | kill/check_sig/sig_proc/sig_proc_exit/process_ksig | 新 10 §3-§5 | 改写 | 10 分支 |
| 11 §3-§5 | Rust/实现/测试 | 新 10 §6 | 改写/删除 | 接线块删 |
| 11 §6-§7 | 过渡/参见 | 新 10 §7 | 改写 | |
| 12 §1.0-§1.8 | 安装语义/三态/KILL 封堵/四 how/sigsuspend/sig_send | 新 11 §1-§3 | 合并改写 | 与 13 合并 |
| 12 §2.1-§2.8 | sigaction 族/sig_send | 新 11 §4-§5 | 合并 | E13/E14 勘误 |
| 12 §3-§5 | Rust/实现/测试 | 新 11 §6 | 改写/删除 | |
| 12 §6-§7 | 过渡/参见 | 新 11 §7 | 改写 | |
| 13 §1.0-§1.8 | EBUSY/SIGSNDELAY/PROC_STOPPED/unpause/check_pending | 新 11 §2-§3 | 合并 | E15 勘误 |
| 13 §2.1-§2.8 | stop_proc/try_resume/check_pending/restart_sigs/unpause | 新 11 §4-§5 | 合并 | |
| 13 §3-§5 | Rust/实现/测试 | 新 11 §6 | 改写/删除 | |
| 13 §6-§7 | 过渡/参见 | 新 11 §7 | 改写 | |

**旧 14 / 15 / 16 / 17**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 14 §1.0-§1.8 | 三族/取整/后端/which/cause_sigalrm | 新 12 §1-§2 | 原样搬移 | E7 勘误 |
| 14 §2.1-§2.12 | 变换/do_itimer/vtimer/realtimer/set_alarm | 新 12 §3-§5 | 原样搬移 | |
| 14 §3-§5 | Rust/实现/测试 | 新 12 §6 | 改写/删除 | Rust 硬编码修正 |
| 14 §6-§7 | 过渡/参见 | 新 12 §7 | 改写 | |
| 15 §1.0-§1.8 | 三元组/SETUID/VFS 同步/GROUPS/setsid/TAINTED | 新 13 §1-§3 | 原样搬移 | E8/E11 勘误 |
| 15 §2.1-§2.16 | do_get/do_set 全分支 | 新 13 §3-§5 | 原样搬移 | 13 调用枚举 |
| 15 §3-§5 | Rust/实现/测试 | 新 13 §6 | 改写/删除 | |
| 15 §6-§7 | 过渡/参见 | 新 13 §7 | 改写 | |
| 16 §1.0-§1.8 | 调度动机/nice 缩放/PRIV 父/nice 拒绝 | 新 07 §1-§2 | 重排+改写 | E3/E12 勘误 |
| 16 §2.1-§2.8 | sched_init/start_user/nice/nice_to_priority/getpriority | 新 07 §3-§4 | 重排 | 补 SEND_* |
| 16 §3-§5 | Rust/实现/测试 | 新 07 §5 | 改写/删除 | |
| 16 §6-§7 | 过渡/参见 | 新 07 §6 | 改写 | |
| 17 §1.0-§1.8 | exec 分工/TAINTED 二重/半态/catch/tracer/frame | 新 14 §1-§3 | 重排+改写 | E6/exec_info 勘误 |
| 17 §2.1-§2.10 | do_exec/newexec/execrestart/exec_restart | 新 14 §3-§5 | 重排 | |
| 17 §3-§5 | Rust/实现/测试 | 新 14 §6 | 改写/删除 | |
| 17 §6-§7 | 过渡/参见 | 新 14 §7 | 改写 | |

**旧 18 / 19 / 20 / 99**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 18 §1.0-§1.8 | 双入口/attach/TRACE_STOPPED/sigtrace/T_EXIT | 新 15 §1-§3 | 重排+改写 | |
| 18 §2.1-§2.15 | T_* 全族/trace_stop | 新 15 §3-§5 | 重排 | |
| 18 §3-§5 | Rust/实现/测试 | 新 15 §6 | 改写/删除 | |
| 18 §6-§7 | 过渡/参见 | 新 15 §7 | 改写 | |
| 19 §1.0-§1.8 | 双时钟/溢出分解/分辨率/stime/settime | 新 16 §1-§3 | 重排+改写 | E25 勘误 |
| 19 §2.1-§2.11 | 五调用/getuptime/clock_time/内核侧 | 新 16 §3-§5 | 重排 | 内核侧留契约 |
| 19 §3-§5 | Rust/实现/测试 | 新 16 §6 | 改写/删除 | |
| 19 §6-§7 | 过渡/参见 | 新 16 §7 | 改写 | |
| 20 §1.0-§1.8 | uname/getsysinfo/解析/reboot/svrctl/rusage | 新 17 §1-§3 | 重排+改写 | E1/E2/E19/E20 勘误 |
| 20 §2.1-§2.13 | 十一调用逐段 | 新 17 §3-§5 | 重排 | |
| 20 §3-§5 | Rust/实现/测试 | 新 17 §6 | 改写/删除 | 登记 Rust bug |
| 20 §6-§7 | 过渡/参见 | 新 17 §7 | 改写 | |
| 99 §1-§2.4 | 词典规则/身份/代际/信号集合/全局状态 | 新 99 §2-§4 | 扩写 | 六处勘误落表 |
| 99 §3-§5 | Rust 决策/实现/测试 | 新 99 §5-§6 | 改写/删除 | |

### 8.2 引用迁移表

#### 8.2.1 文件名映射（批量替换主键）

| 旧文件名 | 新文件名 | 说明 |
|---------|---------|------|
| `00-pm-overview.md` | `00-pm-overview.md` | 同名 |
| `01-pm-init-main.md` | `01-pm-boot-init.md` | 改名 |
| `02-mproc-struct.md` | `02-mproc-struct.md` | 同名 |
| `03-mproc-table.md` | `03-mproc-table.md` | 同名 |
| `04-ipc-dispatch.md` | `04-ipc-dispatch.md` | 同名 |
| `05-vfs-interaction.md` | `05-vfs-protocol.md` | 改名 |
| `06-event-subscription.md` | `06-event-subscription.md` | 同名 |
| `07-pm-fork.md` | `08-process-create.md` | **合并** |
| `08-pm-srv-fork.md` | `08-process-create.md` | **合并** |
| `09-pm-exit.md` | `09-exit-wait.md` | **合并** |
| `10-pm-wait.md` | `09-exit-wait.md` | **合并** |
| `11-signal-core.md` | `10-signal-core.md` | **编号 −1** |
| `12-signal-handlers.md` | `11-signal-handling.md` | **合并** |
| `13-signal-flow.md` | `11-signal-handling.md` | **合并** |
| `14-itimer.md` | `12-timer.md` | **编号 −2 + 改名** |
| `15-credentials.md` | `13-credentials.md` | **编号 −2** |
| `16-scheduling.md` | `07-scheduling.md` | **编号 −9（前移）** |
| `17-exec.md` | `14-exec.md` | **编号 −3** |
| `18-trace.md` | `15-trace.md` | **编号 −3** |
| `19-time.md` | `16-time.md` | **编号 −3** |
| `20-misc-queries.md` | `17-misc-queries.md` | **编号 −3** |
| `99-global-concepts.md` | `99-global-concepts.md` | 同名 |
| —（新建） | `18-external-interfaces.md` | 新篇 |

#### 8.2.2 特殊规则与热点

- **裸编号引用**（如"见 16"）：必须按主题重定向。旧 `16` → 新 `07`；旧 `17` → 新 `14`；旧 `09`/`10` → 新 `09`；旧 `12`/`13` → 新 `11`；旧 `07`/`08` → 新 `08`；旧 `14` → 新 `12`；旧 `15` → 新 `13`；旧 `18` → 新 `15`；旧 `19` → 新 `16`；旧 `20` → 新 `17`。
- **仓库其它文档引用**（227 处，热点）：`17-stage-net/{todo,plan,19-lwip-route}.md`、`10-stage-mib/{15-mib-subtree-minix,19-mib-proc-args,README,plan}.md`、`08-stage-is/{plan,06-is-dump-pm,00-is-overview}.md`、`09-stage-init/{plan,14-init-external-contracts}.md`、`06-stage-sched/06-start-scheduling.md`、`edge3.md`/`edge4.md`、`00-master-plan/README.md`、`03-stage-rs/{00-rs-overview,draft/README}.md`。多数引用"PM 的某机制/对外契约"，应转到新 18 或对应机制篇，不能只换文件名。
- **Rust 代码注释**（20 个文件，热点）：`pm/src/init.rs`（6）、`vfs/src/ipc/dispatcher.rs`（4）、`pm/src/ipc/calls.rs`（2）、`pm/src/event.rs`（2）、`minix-types/src/ipc/vfs.rs`（2），其余各 1（`mproc/{trace,table}.rs`、`ipc/vfs.rs`、`main.rs`、`tests/run_once_integration.rs`、`vfs/{protect,exec,coredump}.rs`、`sched/{priority,client}.rs`）。改名后语义变化（如旧 16→新 07）必须按 §8.1 修正小节号。
- **隐藏设计目录引用**（约 21 处）：05–20 各 1–2 处，全部删除。
- **验证方式**：迁移后执行两路 rg：一是文档文件名引用，期望全部命中 §8.2.1 新名集合；二是仓库约定的隐藏设计目录名，期望除 `doc_rerank_*` 外零命中。

### 8.3 断链成本摘要

| 指标 | 数量 | 说明 |
|------|------|------|
| 旧文档之间的文件名引用 | ≈402 处 | 逐目标计数见 §0.3；全部需改写 |
| 裸编号引用 | 未机械统计（与文件名引用同量级） | 需按主题人工重定向（规则见 §8.2.2） |
| 仓库其它文档引用 | 227 处（约 30 个文件） | 热点见 §8.2.2 |
| Rust 代码注释引用 | 20 个文件 | 模块头注释为主 |
| 违规隐藏设计目录引用 | 约 21 处 | 直接删除 |
| **合计** | **≈670 处** | 可机械替换 ≈620 处（文件名），需人工判断 ≈50 处（裸编号 + 章节号 + 语义改指） |

**风险热点**：① 旧 `16/17/18/19/20` 集体前移且 `16→07` 跨段，任何只按数字替换必错；② `07+08`、`09+10`、`12+13` 三组合并会让"指到旧篇"的引用必须落到合并篇的具体小节；③ 其它 stage 对 PM 的引用多按主题（fork 联调、sched 协议、mib 表导出、init 契约），应转新 18 或对应篇；④ Rust 注释是与"文档-代码同步"对账的证据，必须同步改。

**建议批量方式**：先应用 §8.2.1 的 23 条文件名映射（机械替换，含 Rust 注释）；再用裸编号规则人工重定向；最后按 §8.2.2 的两路 rg 对账，并把新目录写入 `00-pm-overview.md` 导航表作为引用唯一入口。

---

## 9. 验证与自检门

### 9.1 四种机械检查结果

**检查一：前向引用扫描（逐篇查"前置"字段）**

| 新篇 | 前置（只允许更小编号） | 结果 |
|------|----------------------|------|
| 00 | 无 | 通过 |
| 01 | 00 | 通过 |
| 02 | 00、01 | 通过 |
| 03 | 02 | 通过 |
| 04 | 01、02、03 | 通过 |
| 05 | 04 | 通过 |
| 06 | 02、05 | 通过 |
| 07 | 01、04 | 通过 |
| 08 | 03、05、07 | 通过 |
| 09 | 03、05、06、08 | 通过 |
| 10 | 04、05、09 | 通过 |
| 11 | 05、10 | 通过 |
| 12 | 04、10 | 通过 |
| 13 | 02、05 | 通过 |
| 14 | 05、11、13 | 通过 |
| 15 | 09、10、11、14 | 通过 |
| 16 | 02、04 | 通过 |
| 17 | 03、05、09 | 通过 |
| 18 | 01–17（签名汇总） | 通过（参考面，无理解依赖） |
| 99 | 无 | 通过 |

结论：**零前向引用**。软性机制 deferral（08/09 引用 10 的 `sig_proc`、07/17 引用 13 的 privileged 谓词、14 引用 15 的 tracer 信号）已在 §4.4 序差表登记，均不构成理解依赖。

**检查二：依赖关系图检查**

依赖边全部从小号指向大号（§4.1 图），18/99 无出边 → **无环**。

**检查三：覆盖率检查**

知识点池 329 条（§2.21）全部有去向（按编号分组直接对应新篇）；新增 17 条均有证据锚点；明确删除项列于 §6 删除条款。**覆盖率 100%**。

**检查四：断链成本统计**

见 §8.3：文件名引用 ≈402 处、仓库其它文档 227 处、Rust 20 个文件、隐藏设计目录引用约 21 处，合计 ≈670 处。

### 9.2 自检门（G1–G9）

| 门 | 检查内容 | 结果与证据 |
|----|---------|-----------|
| G1 | C 真序逐条可核对 | 通过；随机抽 10 条见 §1.2，全部与源文件行一致 |
| G2 | 知识点池完整：每个 C 文件、每个非 C 制品都有归属或明确排除 | 通过；C 文件归属：`main.c`→01/04，`forkexit.c`→03/08/09，`signal.c`→10/11，`alarm.c`→12，`exec.c`→14，`event.c`→06，`getset.c`→13，`misc.c`→07/17，`schedule.c`→07，`time.c`→16，`trace.c`→15，`utility.c`→03/05/07/09，`table.c`→04，`profile.c`/`mcontext.c`→17/99；头文件：`mproc.h`→02/99，`glo.h`/`const.h`/`type.h`/`pm.h`/`proto.h`→02/03/12/99，`callnr.h`→04/99，`com.h`→05/07/99，`ipc.h`→05/06/99，`sys/sys/*`→各机制篇与 99；非 C：`system.conf`（service pm）→01/18，`Makefile`→99 WONTFIX，`Cargo.toml`→17/99，集成测试→§3.6 第 10 项 |
| G3 | 新目录前向引用为零 | 通过；见 §9.1 检查一 |
| G4 | 依赖图无环 | 通过；边只向大号，18/99 无出边 |
| G5 | 覆盖率 100%，新增有锚点，删除项单列 | 通过；池 329 条按编号分组有去向；新增 17 条锚点见 §2；删除项见 §6 |
| G6 | 拆分/合并去向与新建来源抽查 | 通过；抽查十处：①旧 07+08 合并→08（K-08-01…18，九步只留一次）；②旧 09+10 合并→09（K-09-01…27）；③旧 12+13 合并→11（K-11-01…26）；④旧 16 前移→07（K-07-01…11）；⑤新建 18 来源 G-05/06/07/09/10/12；⑥旧 01 拆出循环/分派→04；⑦旧 04 的 47 表数值→99；⑧`SIGS_IS_*`→10+99；⑨`set_rusage_times`→09；⑩`do_getsetpriority`→07 |
| G7 | 每篇契约七要素齐全 | 通过；20 篇契约均含定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收标准；知识点清单以"编号+名称+归属理由"压缩引用 §2 |
| G8 | 锚点迁移表覆盖所有变化文档每一节；引用迁移覆盖文档与代码注释 | 通过；§8.1 覆盖旧 00–20、99 的全部小节目录（同去向连续小节合并行）；§8.2 含 23 条文件名映射、裸编号规则、227 处外部引用热点、Rust 20 文件、隐藏设计目录 21 处 |
| G9 | 事实断言都有锚点；推测项标注 | 通过；另抽 10 条核对：①`SI_PROC_TAB=2`（`sysinfo.h:11`）；②`RB_POWERDOWN=0x808`（`reboot.h:54`）；③`_NSIG=64`（`signal.h:45`）；④`SIGS_IS_LETHAL` 六信号（`signal.h:281-283`）；⑤`SCHEDULING_START=0xF02`（`com.h:804`）；⑥`TO_NOEXEC=0x4`（`ptrace.h:211`）；⑦`MAX_SECS=TMRDIFF_MAX/hz`（`servers/pm/const.h:15`）；⑧`GID_MAX=2147483647U`（`syslimits.h:53`）；⑨`LAST_FEW=2`（`forkexit.c:32`）；⑩`ASYN_NR=2*_NR_PROCS` 用户态表（`asynsend.c:17-18`）。**待验证/未实测项**：(a) 旧文 `mproc` 体积（≈480B/75KB）与 Rust 侧 `ProcTable` 体积断言未实测，新篇不写；(b) Rust 行号锚点（`init.rs:279` 等）只对"旧文已过期"作结论，未逐条给新行号（B 相机械重建）；(c) C 侧无 PM 专属测试程序按 `minix3/minix/tests/` 目录核对，未做全树按名穷举 |
| G10（附录） | 勘误表 26 条是否全部有锚点 | 通过；§3.5 每条给出 C 锚点与真值；其中 E1/E2/E7 同时是 Rust 代码 bug，已标注"通电前必修" |

### 9.3 结论与待用户裁决的问题

**结论**：本蓝图给出的新目录（20 篇）满足四条硬标准与 G1–G9 全部门；旧目录 329 条知识点全部有去向，新增 17 条全部有锚点；§3.5 的 26 条事实勘误全部落到新篇验收标准；引用迁移 ≈670 处的映射已可执行。蓝图达到"B 相按篇施工"的程度。

**待用户裁决的问题**：

1. **信号三篇并两篇是否接受**：新 10（模型）+ 新 11（安装/投递/恢复）各约 600–800 行，均为单一语义单元；若要求更细，可拆回三篇，但会重新引入互相前向引用。
2. **exit 与 wait 合并的篇幅**：新 09 预计 700–900 行（含两阶段、僵尸引擎、三环、rusage）；若拆，拆点建议在"僵尸/收养引擎"处，但会切裂生产–消费闭环。
3. **调度前移到 07 是否接受**：好处是 fork 继承与 boot 接管同篇；代价是旧 `16` 的引用全部跨段迁移。
4. **Rust 常量 bug 的处置**：`misc.rs` 的 `SI_PROC_TAB`/`RB_POWERDOWN`、`timer.rs` 的 `MAX_SECS` 建议在 B 相开工前或同轮修复；若暂不修，新篇必须如实写"实现与 C 不一致"。
5. **过程叙述清理政策**：测试计数、Fix/批次块、隐藏设计目录引用一律不进新正文（§6 删除条款）；若希望保留实现状态快照，建议集中到 `todo.md`。
6. **旧文档归档目录名**：建议 `04-stage-pm/archive/`（B 相执行）；若与仓库其它约定冲突按现状调整。
7. **G9 三条待验证项**：`mproc`/`ProcTable` 体积、Rust 行号锚点、C 测试穷举，建议 B 相按需补测或保持不写。


