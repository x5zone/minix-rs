# 04-stage-pm 文档重建蓝图（HY4）

## 0. 元数据

### 0.1 执行头部

```text
your_name(AI agent name) = HY4
target_dir(关注的工作目录) = 04-stage-pm
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_HY4.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _HY4 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

- **日期**：2026-09-19
- **当前提交号**：`2d9d1f0aa`（`git log --oneline -1`）
- **交付物**：本文件一份。本蓝图未修改、未移动、未删除任何现有文件。

> **执行期副作用声明**：执行过程中运行了 `tools/coverage-extract/coverage-extract.py pm ...`
> 以取得 C 符号覆盖证据，该工具在 `.review/pm/SYMBOLS.md` 落了一份输出（该目录被
> `.gitignore:27` 的 `.review/` 规则忽略）。本蓝图不引用其中内容，仅引用其汇总数字；
> B 相开工前建议删除该目录以保持中间产物纪律。

### 0.2 审查范围

| 类别 | 内容 | 处置 |
|------|------|------|
| **编号文档（重建对象）** | `00-pm-overview.md` ~ `20-misc-queries.md`、`99-global-concepts.md`，共 22 篇，13,281 行 | 全部进入知识点池，全部参与去向裁决 |
| **参考材料（不是重建对象，但抽料）** | `plan.md`（446 行）、`todo.md`（589 行） | 作为"实施台账 / ARCH 清单 / 排除项"的原料；其**易腐烂内容**（接线批次、修复账目）在重建后集中到新 `25` 篇，正文不再承载 |
| **素材（draft/）** | 9 篇（README + mproc-design 2748 行 + do-fork-impl 1467 行 + pid-generator 620 行 + integration-test 355 行 + pm-call-vfs-fork 354 行 + exit-impl 189 行 + srv-fork-impl 170 行 + wait-impl 172 行） | 知识来源之一；B 相归档不删。其中 `draft/README.md` 引用了不存在的 `pm-call-vm-fork.md`（旧断链），重建后不再引用 |
| **范围外** | `doc_rerank_deepseek.md` / `doc_rerank_glm.md` / `doc_rerank_qwen.md` | 按任务规则**未读取** |
| **范围外** | `.design/`（69 个快照）、`tmp_design_and_todo/` | 按项目规范不引用；仅在 §8 说明其随编号迁移需重生成 |
| **范围外（上游/下游 stage）** | `../01-stage-kernel/`、`../02-stage-vm/`、`../03-stage-rs/`、`../05-stage-vfs/`、`../06-stage-sched/`、`../14-stage-runtime/` | 只作交叉引用目标，不重写 |

### 0.3 读取清单

**文档**：22 篇全读头部声明，正文按知识点抽取精读。

**C 源码（15 个 .c + 6 个本地 .h，4,764 行，全量读）**：

```
minix3/minix/servers/pm/
  main.c 424 / forkexit.c 807 / signal.c 855 / misc.c 447 / event.c 353
  alarm.c 344 / exec.c 200 / trace.c 276 / getset.c 223 / utility.c 156
  time.c 131 / schedule.c 112 / table.c 62 / mcontext.c 27 / profile.c 45
  mproc.h 106 / proto.h 96 / glo.h 31 / pm.h 27 / const.h 20 / type.h 5
  Makefile 17（SRCS 列出 15 个 .c）
```

**C 头文件 / 线格式（外部）**：
- `minix/include/minix/callnr.h`：PM_BASE=0x000、IS_PM_CALL、PM_EXIT(1) ~ PM_GETSYSINFO(47)
- `minix/include/minix/com.h:513-583`（VFS_PM_RQ_BASE 0x900 / RS_BASE 0x980 / 12 RQ + 11 RS / 字段宏）、`:70-72`（INIT_PROC_NR=11）
- `minix/include/minix/ipc.h`（`mess_lc_pm_*` / `mess_pm_lc_*` / `mess_lsys_*` / `mess_rs_*` wire 结构）
- `minix/include/minix/sched.h`（sched_stop / sched_start / sched_inherit 三原型）
- `minix/include/minix/syslib.h:289-293`（PROC_EVENT_EXIT=0x01 / PROC_EVENT_SIGNAL=0x02）
- `minix/include/minix/config.h:31/68-74`（NR_PROCS / MAX_USER_Q=0 / USER_Q / MIN_USER_Q / USER_QUANTUM=200）
- `minix/include/minix/priv.h:93-100`（SRV_Q / USR_Q 均等于 USER_Q）
- `minix/include/minix/param.h:9`（NR_BOOT_PROCS）、`minix/include/minix/type.h:145`（PROC_NAME_LEN=16）
- `sys/sys/signal.h:45`（_NSIG=64）、`:52-83`（信号号表）、`:264/274`（SIGSNDELAY / SIGKSIG）
- `sys/sys/ptrace.h:37-55`（PT_*）、`:209-211`（TO_TRACEFORK=0x1 / TO_ALTEXEC=0x2 / TO_NOEXEC=0x4）、`:226-250`（T_* 全集）
- `minix/lib/libsys/sched_start.c`（sched 客户端实现位置）

**Rust 入口**：`os/servers/pm/src/` 37 个 .rs（19,847 行）+ `os/servers/pm/tests/run_once_integration.rs`（425 行 / 27 个测试函数）+ `os/servers/pm/Cargo.toml`（features: `syscall_stats` / `sprofile`）+ `os/libs/minix-types/src/ipc/{pm,vfs,event,message,notify,sysinfo}.rs` + `os/libs/minix-sys/src/syscall.rs`。

**阶段边界材料**：`../00-master-plan/README.md`（启动因果链：Kernel → VM → RS → PM/SCHED/VFS/DS/MIB → …）、`../edge_todo.md`（E1/E2/E5/E6/E7/E9 等）、本目录 `plan.md` §5.4（排除项）、`todo.md` §12（V3 轮）。

**前一个 stage**：`../03-stage-rs/00-rs-overview.md`（已讲：RS 是 root sysproc、boot 两层顺序语义、服务生命周期、`RS_PROC_NR=2`、进程编号区间 PM=0/VFS=1/RS=2/…/INIT=11）——本 stage **不重复展开** RS 的加载机制，只在启动篇声明"PM 由 RS 加载"并回指。

### 0.4 使用的命令与关键输出（证据摘录）

```bash
# 1) 文档与 C 源码清单
wc -l 04-stage-pm/*.md                 # 22 篇 13,281 行
wc -l minix3/minix/servers/pm/*        # 15 .c + 6 .h = 4,764 行
ls minix3/minix/servers/pm/*.c | wc -l # 15

# 2) 47 个调用号与分发表
grep -n 'define PM_' minix3/minix/include/minix/callnr.h   # PM_EXIT(1) … PM_GETSYSINFO(47)
grep -n 'CALL('     minix3/minix/servers/pm/table.c        # 47 行 CALL(PM_xxx) = do_yyy
grep -n 'PmCall::'  os/servers/pm/src/ipc/calls.rs         # Rust 侧 1..=47 全量枚举 + 分发臂

# 3) 覆盖率（C 符号面）
python3 tools/coverage-extract/coverage-extract.py pm \
  notes/rewrite/fork-syscall-rewrite/04-stage-pm \
  --rust-dir os --c-dir minix3/minix/servers/pm \
  --semantic-map tools/coverage-extract/pm-semantic-map.json
# → Total C symbols: 109 (71 funcs, 38 macros)
# → Doc covered: 109 (100.0%)   Rust covered (name-match): 102 (93.6%)

# 4) 测试基线（本蓝图实测，用于核对文档声称）
cd os && cargo test -p minix-pm --lib --tests
# → lib: 403 passed; 0 failed
# → integration: 11 passed; 0 failed
# → 注：cargo test -p minix-pm（含 doctest）因 lib.rs 的 doc 示例引用未导入符号而失败

# 5) 引用关系（断链成本）
grep -ho '[0-9][0-9]-[a-z0-9-]*\.md' 04-stage-pm/{00..20,99}*.md | wc -l  # 377
grep -ro '[0-9][0-9]-[a-z0-9-]*\.md' os/servers/pm/src | wc -l            # 53（init.rs 23 处为热点）

# 6) 真值核对（抽三条，用于纠正旧文档）
grep -n 'define TO_' minix3/sys/sys/ptrace.h   # TO_NOEXEC = 0x4（旧 18-trace.md 写 0x1 → 错）
grep -n 'define T_'  minix3/sys/sys/ptrace.h   # T_STOP=-1 / T_READB_INS=100 / … / T_SETRANGE=107
sed -n '295,424p' minix3/minix/servers/pm/main.c  # handle_vfs_reply 11 路 + 尾部 restart_sigs
```

### 0.5 范围外发现（留给其它 stage / edge 条目）

1. `lib.rs` 的 rustdoc 示例引用了未导入符号（`RemainingFlags`、`TraceOptions` 等），`cargo test -p minix-pm` 的 **doctest 失败**，而 `--lib --tests` 全绿。属 PM crate 卫生问题，不是文档重建问题。
2. `os/libs/minix-types/src/ipc/message.rs` 单文件承载全部 union 臂（PM/VFS/RS/内核各族混居），`edge_todo.md` 的 E-MINTYPES-RS 已登记"按族拆分"的可重构观察。本蓝图只在 `06` 篇声明"布局权威在 minix-types，不在此复制"。
3. `minix-types` 的 C-ABI `struct mproc` 464B 镜像（D-29 闭环）使 `SI_PROC_TAB` 有了真实数据路径；旧 `20-misc-queries.md` 与 `00-pm-overview.md` 仍写着"假数据 / 未接线"口径，属**文档腐烂**，由 `22` + `25` 纠正。

---

## 1. C 真序（运行时真序重建）

### 1.1 阶段类型判定

**判定：服务事件循环型（主导）+ 启动链型（首段）+ 系统调用集合型（请求面）。**

- `main.c:59-107` 是 `while (TRUE)` 的 `receive → 分派 → reply` 循环，PM 永不返回 → 事件循环型；
- `main.c:49-56 → sef_local_startup() → sef_cb_init_fresh()` 是一次性启动链，"第一代进程世界"就在这条链上建成 → 启动链型；
- `table.c` 注册的 47 个 PM 调用是一组**并行请求族**（生命周期 / 信号 / 凭证 / 时间 / 定时器 / 调度 / exec / ptrace / 杂项），彼此无线性先后 → 集合型。

按 §九 补充提示：**主线取"一次请求的生命周期"**，启动链作为主线第 0 段；非启动路径的机制（调试工具、统计、已声明不做）集中为支线。

### 1.2 真序表·启动段（一次性）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| S1 | `main()` 调 `sef_local_startup()` | `main.c:49-56` | 用户态入口；之后进入主循环不再返回 |
| S2 | 注册 `sef_cb_init_fresh` | `main.c:118` | 首次启动回调 |
| S3 | 注册 `SEF_CB_INIT_RESTART_STATEFUL` | `main.c:119` | 重启回调（本 stage 只声明存在） |
| S4 | 注册 `sef_setcb_signal_manager(process_ksig)` | `main.c:122` | 内核→PM 信号回环的**唯一注册点**；实际拦截在 `libsys/sef_signal.c` 的 receive 路径 |
| S5 | `sef_startup()` 触发 `sef_cb_init_fresh()` | `main.c:125` / `:131` | 八步初始化入口 |
| S6 | 遍历 `mproc[0..NR_PROCS]`：`init_timer`、`mp_magic=MP_MAGIC`、`mp_sigact=mpsigact[i]`、`mp_eventsub=NO_EVENTSUB` | `main.c:147-152` | 表初始化（含定时器） |
| S7 | `sigemptyset` + `sigaddset` 构建 `core_sset` / `ign_sset` / `noign_sset` | `main.c:137-165` | 三集合**运行时**构建，非编译期常量 |
| S8 | `sys_getmonparams(monitor_params, sizeof)` | `main.c:169` | 失败即 panic |
| S9 | `sys_getimage(image)` | `main.c:175` | 向内核索取 boot image 表（`NR_BOOT_PROCS` 项） |
| S10 | `procs_in_use = 0`；遍历 image，`proc_nr >= 0` 才处理（task 为负号） | `main.c:177-179` | 只填用户/系统进程 |
| S11 | INIT 特例：`strlcpy(mp_name)`、清三个信号集、`mp_parent=INIT_PROC_NR`、`mp_procgrp=mp_pid=INIT_PID`、`IN_USE`、`mp_scheduler=KERNEL`、`mp_nice=get_nice_value(USR_Q)` | `main.c:184-201` | **INIT 是自己的父亲**（C 注释自承 "not really OK"） |
| S12 | 系统进程：`mp_parent = (RS ? INIT_PROC_NR : RS_PROC_NR)`、`mp_pid=get_free_pid()`、`IN_USE\|PRIV_PROC`、`mp_scheduler=NONE`、`mp_nice=get_nice_value(SRV_Q)` | `main.c:202-215` | RS 自己挂 INIT，其余挂 RS |
| S13 | `mp_endpoint = ip->endpoint` | `main.c:218` | 与内核 endpoint 对齐 |
| S14 | 逐条 `ipc_send(VFS_PROC_NR, VFS_PM_INIT{SLOT,PID,ENDPT})` | `main.c:221-227` | 每进程一条，失败 panic |
| S15 | 末条 `VFS_PM_INIT{ENDPT=NONE}` 用 `ipc_sendrec` | `main.c:232-236` | **屏障**：VFS 收到 NONE 才回，两表在此对齐 |
| S16 | `system_hz = sys_hz()` | `main.c:238` | 时钟频率 |
| S17 | `sched_init()` | `main.c:241` / `schedule.c:20-50` | 为 INIT 接管用户态调度 |

### 1.3 真序表·循环段（每轮）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| L1 | `sef_receive_status(ANY, &m_in, &ipc_status)` | `main.c:61` | 失败 panic |
| L2 | `is_ipc_notify`；`_ENDPOINT_P(m_source)==CLOCK` → `expire_timers(m_in.m_notify.timestamp)`；`continue` | `main.c:65-71` | 通知**不做** endpoint 校验，也不 reply |
| L2′ | （Rust 侧等价物）SYSTEM 源通知 → `getksig`/`endksig` 拉取循环 → 逐信号 `process_ksig` | C：`main.c:122` 注册 + `libsys/sef_signal.c` 拦截；Rust：`init.rs` notify 分支 + `signal.rs::process_sigmgr_signals` | 见 §7 缺漏新篇 N-4 |
| L3 | `who_e = m_in.m_source` | `main.c:74` | |
| L4 | `pm_isokendpt(who_e,&who_p)`，非 OK 则 panic | `main.c:75-76` | |
| L5 | `mp = &mproc[who_p]`；`call_nr = m_in.m_type` | `main.c:77-78` | C 的"当前进程"全局指针 |
| L6 | `mp_flags & EXITING` → `continue` | `main.c:81-82` | **退出中进程的延迟调用被丢弃** |
| L7 | `IS_VFS_PM_RS(call_nr) && who_e==VFS_PROC_NR` → `handle_vfs_reply()`；`result = SUSPEND` | `main.c:84-87` | 第一路：VFS 异步回复 |
| L8 | `call_nr == PROC_EVENT_REPLY` → `do_proc_event_reply()` | `main.c:88-89` | 第二路：事件订阅者回复 |
| L9 | `IS_PM_CALL` → `call_index = call_nr - PM_BASE`；`calls_stats[call_index]++`；`call_vec[call_index]()`，越界/空槽 → `ENOSYS` | `main.c:90-101` | 第三路：47 个 PM 调用 |
| L10 | 其余 → `ENOSYS` | `main.c:102-103` | |
| L11 | `result != SUSPEND` → `reply(who_p, result)` | `main.c:106` | `reply` 本体 `main.c:249-270` |

### 1.4 真序表·代表请求的一次生命周期（主线骨架）

| 请求 | 生命周期（带锚点） |
|------|------------------|
| **fork** | `do_fork` 容量预检(`forkexit.c:60-65`) → `next_child` 轮转(`:68-75`) → `vm_fork`(`:78`) → `*rmc=*rmp` 复制 + 字段重整(`:86-114`) → `get_free_pid`(`:119`) → `VFS_PM_FORK` + `tell_vfs`(`:122-130`) → tracer `sig_proc(SIGSTOP)`(`:133-134`) → `return SUSPEND`(`:139`) → **异步**：`VFS_PM_FORK_REPLY` → `handle_vfs_reply`(`main.c:369-396`) → `sched_start_user` 或失败拆解 → `reply(child,OK)` + `reply(parent,pid)` |
| **exit** | `do_exit`：`PRIV_PROC` → `sys_kill(SIGKILL)`，否则 `exit_proc`(`forkexit.c:246-262`) → `exit_proc` 九步(`:267-413`) → `VFS_PM_EXIT`/`VFS_PM_DUMPCORE` + `tell_vfs`(`:350-359`) → `zombify` → `check_parent` → `disinherit` → `SIGHUP` → **异步**：`VFS_PM_EXIT_REPLY` → `publish_event`(`main.c:365`) → 订阅者轮 → `exit_restart`(`forkexit.c:418-469`) → `sched_stop`/`sys_clear`/`vm_exit`/`cleanup` |
| **wait** | `do_wait4` 扫描三环(`forkexit.c:500-548`) → 命中则 `tell_parent`/`tell_tracer` 并 `SUSPEND`；未命中则 `WAITING` + `SUSPEND`(`:551-563`)；无合格子进程 → `ECHILD`(`:562`) → **异步唤醒**：`zombify`/`check_parent` 侧调 `tell_parent` 时回复 |
| **kill** | `do_kill` → `check_sig(pid,signo,ksig=FALSE)`(`signal.c:197-201`) → 倒序扫表 + 四重权限(`signal.c:597-641`) → `sig_proc` 九判定链(`:383-540`) → 忽略 / 阻塞 / 捕获(`unpause`+`sig_send`) / 终止(`sig_proc_exit`) → 调用者自毁 → `SUSPEND`(`signal.c:644`) |
| **exec** | `do_exec` → `VFS_PM_EXEC` + `tell_vfs`(`exec.c:38-56`) → **异步**：VFS 装载后 `do_newexec`(`:62-125`) 更新凭证/`TAINTED`/`name`/`frame`/`PARTIAL_EXEC` → `VFS_PM_EXEC_REPLY` → `exec_restart`(`:156-199`)：重置 `catch` → tracer `SIGTRAP`/`SIGSTOP` → `sys_exec` |

### 1.5 47 个调用的分派表结构（集合型骨架）

- 判别：`IS_PM_CALL(type) == (((type) & ~0xff) == PM_BASE)`，`PM_BASE = 0x000`（`callnr.h:9/11`）。
- 分派：`call_vec[NR_PM_CALLS]`（`table.c:14-61`），索引 = `call_nr - PM_BASE`，未注册槽为 `NULL` → `ENOSYS`。
- Rust：`PmCall` 枚举 `#[repr(i32)]`，判别值即调用号（`ipc/calls.rs:141-187`），`dispatch_pm_call` 穷尽 match（`calls.rs:228-991`），`_ => ReplyIntent::Reply(ENOSYS)` 兜底（`calls.rs:991`）。

按主题分组（与 §4 的 Part 划分一致）：

| 组 | 调用号 | 新篇章 |
|----|--------|--------|
| 生命周期 | 1 Exit, 2 Fork, 3 Wait4, 41 SrvFork | 09 / 10 / 11 |
| exec 族 | 14 Exec, 43 ExecNew, 44 ExecRestart | 16 |
| 信号发送 | 11 Kill, 42 SrvKill | 13 |
| 信号控制 | 20 SigAction, 21 SigSuspend, 22 SigPending, 23 SigProcMask, 24 SigReturn | 12（安装与掩码）/ 15（投递闭环） |
| 凭证 | 4, 5, 6, 9, 10, 12, 13, 15, 16, 29, 30, 31, 32 | 18 |
| 定时器 | 17 Itimer | 17 |
| 调度 | 26 GetPriority, 27 SetPriority | 19 |
| ptrace | 8 Ptrace | 20 |
| 时间 | 7 Stime, 28 GetTimeOfDay, 33/34/35 Clock_* | 21 |
| 杂项查询 | 18, 19, 25, 36, 37, 38, 39, 45, 46, 47 | 22 |
| 事件订阅 | 40 ProcEventMask（+ 非调用号的 `PROC_EVENT_REPLY` 通知面） | 08 |

---

## 2. 知识点全集（存量池 + 新增）

> 编号规则：stage 内唯一 `K-xxx`；多 AI 汇总的对齐键 = **名称 + 锚点**。
> 来源类型：**存量** = 现有 22 篇文档承载；**新增** = 现有文档没有/讲得不足，但 C 源码、非 C 制品或 OS 理论承载，由 §3 覆盖审计追加入池。

### 2.1 常量、身份编码与全局状态（旧 99 / 旧 01 §3.3）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-001 | `NR_PIDS=30000` 轮转域 | 常量 | 存量 | 99 §2.1；03 §1.3 | `const.h:3` | 回答"pid 上限从哪来、为何不能改成 2^31" |
| K-002 | `NO_PID=0` / `INIT_PID=1` | 常量 | 存量 | 99 §2.1 | `const.h:8/9` | 回答"0 号 pid 是谁" |
| K-003 | `NO_TRACER=0` 哨兵与"槽位 0 是 boot 首项"的隐式依赖 | 常量/约束 | 存量 | 99 §1.1；02 §2.2 | `const.h:11` | 回答"为什么 0 能同时表示'无 tracer'" |
| K-004 | `NO_EVENTSUB=(char)-1` 与 char 有符号性依赖 | 常量/约束 | 存量 | 99 §1.1；06 §2.1 | `const.h:13` | 回答"为什么这个哨兵在 Rust 里必须变成 `Option`" |
| K-005 | `NR_ITIMERS=3` | 常量 | 存量 | 99 §2.1；14 §1.1 | `const.h:17` | 回答"三族定时器数组宽从哪来" |
| K-006 | `MAX_SECS = TMRDIFF_MAX/system_hz`（依赖运行时 hz） | 常量/约束 | 存量 | 14 §1.2 | `const.h:15` | 回答"setitimer 上界为什么不是编译期常量" |
| K-007 | `SEND_PRIORITY=1` / `SEND_TIME_SLICE=2` —— **PM 内零使用** | 常量/死代码 | 存量（plan §5.4） | 仅 plan/todo | `const.h:19/20` | 回答"这两个常量为什么不实现" |
| K-008 | `PROC_NAME_LEN=16` | 常量 | 存量 | 99 §2.1 | `minix/type.h:145` | 回答"进程名为什么会被截断" |
| K-009 | `_NSIG=64` 位图宽 | 常量 | 存量 | 99 §2.1 | `sys/signal.h:45` | 回答"`sigset_t` 为什么是 64 位" |
| K-010 | `NR_PROCS = _NR_PROCS` | 常量 | 存量 | 99 §2.1 | `minix/config.h:31` | 回答"进程槽总数由谁定" |
| K-011 | `INIT_PROC_NR=11`（= `LAST_SPECIAL_PROC_NR`） | 常量 | 存量 | 99 §2.1；01 §2.4 | `com.h:70/72` | 回答"INIT 的槽位号与 pid 号为何不同" |
| K-012 | endpoint 代际编码 `_ENDPOINT(generation,slot)` 与防 ABA | 数据结构/协议 | 存量 | 99 §2.2；03 §1.4 | `minix/endpoint.h`；`utility.c:108-118` | 回答"槽位复用后旧引用为何立刻失效" |
| K-013 | 信号三集合 `core_sset`/`ign_sset`/`noign_sset` 与 `badignore` | 数据结构/机制 | 存量 | 99 §2.3；01 §3.3；11 §1.4 | `main.c:137-165`；`signal.c:483-485` | 回答"哪些信号默认 core / 默认忽略 / 忽略也无效" |
| K-014 | 信号号表 1..29 + 内核信号 `SIGSNDELAY=70` / `SIGKSIG=74` | 常量 | 存量 | 99 §2.3 | `sys/signal.h:52-83/264/274` | 回答"70/74 为何超出 `_NSIG` 却合法" |
| K-015 | 全局七件套（A-3）：`m_in`/`who_p`/`who_e`/`call_nr`/`mp` + `system_hz`/`abort_flag`/`monitor_params` | 架构演进 | 存量 | 99 §1.2/§2.4；04 §3.5 | `glo.h:8-30` | 回答"Rust 为什么能用借用检查器替代 C 的文件级全局" |
| K-016 | `MP_MAGIC = 0xC0FFEE0` | 常量 | 存量 | 02 §2.1 | `mproc.h:106` | 回答"魔数在什么场景被检查" |

### 2.2 启动链（旧 01）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-020 | PM 在启动因果链的位置（Kernel → VM → RS → PM） | 概念 | 存量 | 01 §1.1；00 §1.1 | `main.c:49`；`../00-master-plan/README.md` | 回答"PM 之前必须有什么" |
| K-021 | SEF 三类回调注册（init_fresh / init_restart_stateful / signal_manager） | 机制 | 存量 | 01 §1.2/§2.2 | `main.c:114-126` | 回答"PM 的生命周期由谁托管" |
| K-022 | `sef_cb_init_fresh` 八步及其顺序依赖 | 机制 | 存量 | 01 §1.3/§2.4 | `main.c:131-244` | 回答"每步为什么必须在这个位置" |
| K-023 | 表初始化：`init_timer` + `MP_MAGIC` + `mp_sigact` 指向 + `NO_EVENTSUB` | 机制 | 存量 | 01 §2.4；02 §2.4 | `main.c:147-152` | 回答"空表长什么样" |
| K-024 | 三集合的**运行时**构建（非编译期常量） | 机制 | 存量 | 01 §2.4；99 §2.3 | `main.c:154-165` | 回答"Rust 为何能用编译期位图等价替代" |
| K-025 | `sys_getmonparams` → `monitor_params` | 接口 | 存量 | 01 §2.4 | `main.c:169` | 回答"启动参数从哪来、谁消费" |
| K-026 | `sys_getimage` → boot image 表（负 `proc_nr` 跳过） | 接口 | 存量 | 01 §2.4 | `main.c:175`；`param.h:9` | 回答"PM 怎么知道有哪些进程要填" |
| K-027 | INIT 特例：parent=self、pid=procgrp=1、scheduler=KERNEL、nice=USR_Q | 机制/约束 | 存量 | 01 §1.4/§2.4 | `main.c:188-201` | 回答"INIT 为什么是自己的父亲" |
| K-028 | 系统进程：parent=RS（RS 自身挂 INIT）、`PRIV_PROC`、scheduler=NONE、nice=SRV_Q | 机制 | 存量 | 01 §2.4 | `main.c:202-215`；`priv.h:93-95` | 回答"系统进程与用户进程的初始差异" |
| K-029 | `VFS_PM_INIT` 逐条 send + `ENDPT=NONE` 的 `sendrec` 屏障 | 协议 | 存量 | 01 §1.5；05 §2.1 | `main.c:221-236` | 回答"两表何时对齐、屏障为何是 sendrec" |
| K-030 | `system_hz = sys_hz()` | 机制 | 存量 | 01 §2.4；19 §2.10 | `main.c:238` | 回答"hz 为何启动期取一次" |
| K-031 | `sched_init()` 为 INIT 接管用户态调度 | 机制 | **越界**（旧 01 §2.6） | 01 §2.6 | `main.c:241`；`schedule.c:20-50` | 回答"INIT 何时被用户态调度器接管" |
| K-032 | `get_nice_value(queue)` 的反向缩放（queue→nice） | 机制 | **越界**（旧 01 §2.5） | 01 §2.5；16 §1.6 | `main.c:275-289` | 回答"它为何是 `nice_to_priority` 的逆" |
| K-033 | `BootParams::placeholder()` 与 D-02 缺口 | 约束/缺口 | 存量 | 01 §3.1/§4.2 | `main.rs:16` | 回答"Rust 侧启动参数的诚实缺口在哪" |

### 2.3 进程结构与状态分层（旧 02）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-040 | `struct mproc` 全字段与字段族分组 | 数据结构 | 存量 | 02 §2.1 | `mproc.h:24-83` | 回答"PM 对一个进程记住了什么" |
| K-041 | 19 个 `mp_flags` 位语义（含 0x00400 跳空） | 数据结构 | 存量 | 02 §2.2 | `mproc.h:86-104` | 回答"每个位由谁置、由谁清" |
| K-042 | `mpsigact[NR_PROCS][_NSIG]` 独立表（约 80% 体积，MIB 规避） | 数据结构/架构 | 存量 | 02 §1.5 | `mproc.h:16-22` | 回答"sigaction 为什么不在 mproc 里" |
| K-043 | `mp_timer` / `mp_interval[3]` / `ALARM_ON` | 数据结构 | 存量 | 02 §2.1；14 §1.3 | `mproc.h:62-63` | 回答"定时器状态放在哪" |
| K-044 | `mp_reply` 持久回复消息与"预填载荷" | 数据结构/协议 | 存量 | 02 §2.1；04 §2.5 | `mproc.h:68`；`main.c:249-270` | 回答"多字段返回值怎么带出去" |
| K-045 | `mp_frame_addr` / `mp_frame_len`（procfs 用） | 数据结构 | 存量 | 02 §2.1；17 §1.6 | `mproc.h:70-72` | 回答"exec 后 ps 为何还能读到命令行" |
| K-046 | `mp_scheduler` / `mp_nice` | 数据结构 | 存量 | 02 §2.1；16 §1.1 | `mproc.h:74-78` | 回答"调度归属存在哪一侧" |
| K-047 | A-1：`Process` 四层分层（Identity / State / Resources / Context） | 架构演进 | 存量 | 02 §3.1 | `mproc/mproc.rs` | 回答"Rust 如何把巨型结构拆成可审计的层" |
| K-048 | A-2：19 位 → `Lifecycle`/`BlockState`/`WaitState` 互斥枚举 + 组合子 | 架构演进 | 存量 | 02 §3.2/§3.3 | `mproc/{lifecycle,block,wait}.rs` | 回答"为何 C 的任意位组合在 Rust 里不可表示" |
| K-049 | fork 的字段继承/重置清单（`mp_flags &= (IN_USE\|DELAY_CALL\|TAINTED)` 等） | 机制 | **越界**（旧 02 §2.5） | 02 §2.5；07 §2.4 | `forkexit.c:86-120` | 回答"子进程继承了什么、丢了什么" |

### 2.4 进程表与身份管理（旧 03）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-060 | `procs_in_use` 与容量纪律（alloc/release 配对） | 数据结构/不变量 | 存量 | 03 §1.5/§2.5 | `glo.h:9`；`forkexit.c:86/805` | 回答"表满判定与泄漏检测" |
| K-061 | `LAST_FEW=2` 非 root 预留 | 约束 | 存量 | 03 §1.5；07 §2.1 | `forkexit.c:32/60-65` | 回答"为什么 root 能在表满时仍 fork" |
| K-062 | `next_child` 轮转槽位与双 `panic` 守卫 | 机制 | 存量 | 03 §2.5；07 §2.2 | `forkexit.c:51/68-75`、`153/174-181` | 回答"槽位分配为何不是线性扫描" |
| K-063 | `pm_isokendpt` 三层检查与 errno 分工（`EINVAL` / `EDEADEPT` ×2） | 接口/不变量 | 存量 | 03 §2.2；04 §2.3 | `utility.c:108-118` | 回答"PM 如何拒绝陈旧 endpoint" |
| K-064 | `find_proc`（PID → 槽位，只认 `IN_USE`） | 接口 | 存量 | 03 §2.3 | `utility.c:76-85` | 回答"哪些调用靠它定位目标" |
| K-065 | `get_free_pid` 双字段扫描（pid **与** procgrp）+ 先自增相位（起于 `INIT_PID+2=3`） | 机制/契约 | 存量 | 03 §2.4；07 §2.5 | `utility.c:34-51` | 回答"pid 为何从 3 开始、进程组为何也占位" |
| K-066 | `cleanup` / `release_slot` 与"不 bump generation"的裁决 | 机制/架构 | 存量 | 03 §3.3/§4.5；10 §2.9 | `forkexit.c:795-806` | 回答"槽位释放时 endpoint 代数由谁推进" |
| K-067 | `PidGenerator` 单一事实源（表即状态，无位图） | 架构演进 | 存量 | 03 §3.4/§4.6 | `mproc/pid_gen.rs` | 回答"为什么 Rust 不需要额外位图" |
| K-068 | `PmContext` / `ProcTable` 聚合（A-3 落点） | 架构演进 | 存量 | 03 §3.1；04 §3.5 | `mproc/context.rs`、`mproc/table.rs` | 回答"借用检查器如何充当并发审计器" |

### 2.5 主循环与分发（旧 04）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-080 | 事件循环：服务器永不返回 | 概念 | 存量 | 04 §1.1 | `main.c:59` | 回答"PM 为何没有 shutdown 路径" |
| K-081 | `sef_receive_status` + `is_ipc_notify` | 机制 | 存量 | 04 §2.2 | `main.c:61/65` | 回答"通知与请求如何区分" |
| K-082 | CLOCK notify → `expire_timers(m_in.m_notify.timestamp)`（**时间戳取通知载荷**） | 机制/纠错 | 存量 + 新增（V3-P2-10） | 04 §2.2；14 §1.6 | `main.c:66-67`；`init.rs` notify 分支 | 回答"主循环拥塞时到期判定为何不整体后移" |
| K-083 | EXITING 进程的消息丢弃 | 机制/不变量 | 存量 | 04 §2.3；09 §2.7 | `main.c:81-82` | 回答"延迟调用为何可能石沉大海" |
| K-084 | 三路分发（VFS 回复 / 事件回复 / PM 调用） | 机制 | 存量 | 04 §1.3/§2.4 | `main.c:84-103` | 回答"为何不能合成一张表" |
| K-085 | A-5：`call_vec` → `PmCall` 穷尽 match | 架构演进 | 存量 | 04 §3.3 | `table.c:14-61`；`ipc/calls.rs` | 回答"函数指针表在 Rust 里变成什么" |
| K-086 | 47 个调用号与 `IS_PM_CALL` 判别 | 接口 | 存量 | 04 §2.7；99 §3 | `callnr.h:9-60` | 回答"调用号空间如何分区" |
| K-087 | A-6：`SUSPEND` → `ReplyIntent{Reply,ReplyLater,NoReply}` | 架构演进 | 存量 | 04 §1.4/§3.2 | `main.c:106`；`ipc/dispatcher.rs` | 回答"三类异步回复路径如何被一个枚举覆盖" |
| K-088 | `reply()` 与 `mp_reply` 预填载荷复用 | 机制 | 存量 | 04 §2.5 | `main.c:249-270` | 回答"多字段返回值如何发出" |
| K-089 | `ENOSYS` 兜底与 fail-fast（`pm_isokendpt` 失败 panic） | 错误路径 | 存量 | 04 §1.5/§3.6 | `main.c:75-76/100-103` | 回答"哪些返回错误、哪些直接 panic" |
| K-090 | `ENABLE_SYSCALL_STATS` 的 `calls_stats` 与 Rust `syscall_stats` feature | 工具/工程 | 存量（plan §5.4） | 04 §2.8；20 §D7 | `main.c:34-36/95-97`；`Cargo.toml` | 回答"统计口径为何默认关" |
| K-091 | （新增）SYSTEM notify → SIGKSIG 拉取循环 | 机制 | **新增** | 无（todo.md §12 批次 H 已闭环，未成篇） | `main.c:122`；`init.rs` notify 分支、`signal.rs::process_sigmgr_signals` | 回答"内核起源的信号怎么进 PM" |
| K-092 | `run_once` / `run` 拆分与外部驱动（集成测试） | 测试性质 | 存量 | 04 §3.4 | `init.rs::run_once`；`tests/run_once_integration.rs` | 回答"主循环如何被单步测试" |

### 2.6 消息线格式与解码（新增）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-100 | `message` union 与 `m_type` 判别（`m7_i*`/`m7_p*`/`m7_l*` 分区） | 接口/协议 | 新增 | 散见 04 §2.7 与各篇 §2.x | `minix/ipc.h` | 回答"一条消息的字段为何有 i/p/l 三种宽度" |
| K-101 | 47 个调用的入站载荷布局（`mess_lc_pm_*` / `mess_lsys_pm_*` / `mess_rs_pm_*`） | 接口/协议 | 新增 | 散见各篇 | `minix/ipc.h` | 回答"调用参数的字节布局权威在哪" |
| K-102 | 出站回复臂布局（`mess_pm_lc_*`）与"tag + typed body"契约 | 接口/协议 | 新增 | 07 §D8、10 §2.6 零星 | `minix/ipc.h`；`ipc/calls.rs` 各臂 | 回答"返回值放 `m_type` 还是载荷" |
| K-103 | unsafe 解码单点 `ipc/decode.rs` + 布局断言（`size_of <= MESSAGE_PAYLOAD_SIZE`） | 架构演进 | 新增 | 无（Rust 已有，文档未讲） | `ipc/decode.rs:1-60` | 回答"union 读取的 unsafe 面如何收敛到单文件" |
| K-104 | E7 wire 系统化（跨阶段）与本 stage 的消费端纪律 | 跨模块契约 | 新增 | `edge_todo.md` E7 | `../fork-syscall-rewrite/edge_todo.md` | 回答"wire 类型终局在哪、现在怎么过渡" |

### 2.7 VFS 异步协议（旧 05）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-120 | 为什么必须异步（PM 不能阻塞在 VFS 上） | 概念 | 存量 | 05 §1.2 | `utility.c:123-139` | 回答"为什么不是 sendrec" |
| K-121 | `asynsend3(VFS_PROC_NR, …, AMF_NOREPLY)` + `VFS_CALL` | 机制 | 存量 | 05 §2.2 | `utility.c:134-138` | 回答"在途请求如何被记住" |
| K-122 | `tell_vfs` 的"非空闲即 panic"不变量 | 不变量 | 存量 | 05 §2.2 | `utility.c:131-132` | 回答"为何一个进程不能同时有两个 VFS 请求" |
| K-123 | `VFS_PM_RQ_BASE=0x900` / `RS_BASE=0x980` 与 `IS_VFS_PM_RS` 判别 | 协议 | 存量 | 05 §2.1 | `com.h:513-517` | 回答"请求与回复如何共用一套编号" |
| K-124 | 11 路回复路由表（`handle_vfs_reply` switch） | 机制 | 存量 | 05 §2.3 | `main.c:334-419` | 回答"每路回复把控制权交给谁" |
| K-125 | `NEW_PARENT` 生命周期（exit 收养时置、VFS 回复时消费/清） | 机制 | 存量 | 05 §2.5；07/09 | `main.c:327-328`；`forkexit.c:402-403` | 回答"为何 fork 不能回复给原父" |
| K-126 | `UNPAUSED` 生命周期（UNPAUSE 回复置、`try_resume_proc` 清） | 机制 | 存量 | 05 §2.5；13 | `main.c:330/407-410`；`signal.c:288` | 回答"解暂停与恢复如何区分" |
| K-127 | `VFS_PM_REBOOT_REPLY` → `sys_abort(abort_flag)` 特例 | 机制 | 存量 | 05 §2.3 | `main.c:304-312` | 回答"为何它必须先于 endpoint 解析" |
| K-128 | 7 类 `tell_vfs` 调用点（fork / srv_fork / exit / dumpcore / unpause / set* / setsid / exec / reboot） | 机制 | 存量 | 05 §2.4 | 各服务文件 | 回答"PM 哪些动作需要 VFS 参与" |
| K-129 | 尾部 `restart_sigs`（VFS 回复后的挂起信号重查） | 机制 | 存量 | 05 §2.3；13 §1.6 | `main.c:421-423` | 回答"信号为何不在回复前处理" |
| K-130 | （新增）`asynsend` 容量与 `ASYN_NR`（并行 vs 串行的容量论证） | 约束 | 新增 | 06 §2.8 提及 | `libsys/asynsend.c` | 回答"异步发送槽位为何是稀缺资源" |

### 2.8 进程事件订阅（旧 06）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-140 | `PROC_EVENT_EXIT=0x01` / `PROC_EVENT_SIGNAL=0x02` 两事件 | 协议 | 存量 | 06 §2.1 | `syslib.h:292-293` | 回答"PM 对外发布什么" |
| K-141 | `subs[NR_SUBS=4]` 的 `endpt`/`mask`/`waiting` 三元组 | 数据结构 | 存量 | 06 §2.1 | `event.c:58-67` | 回答"订阅表为何只有 4 个槽" |
| K-142 | `mp_eventsub` 游标 + `NO_EVENTSUB` 与 `EVENT_CALL` 配对 | 数据结构/机制 | 存量 | 06 §2.1/§D3 | `event.c:97/116-117` | 回答"一个进程如何记住通知到第几个订阅者" |
| K-143 | 串行化的容量论证（`NR_PROCS` vs `NR_PROCS*NR_SUBS` vs 无界异步） | 架构/约束 | 存量 | 06 §1.3 | `event.c:17-25` | 回答"为何不用并行或全异步" |
| K-144 | `resume_event` 的推进与终止分派（`exit_restart` / `restart_sigs`） | 机制 | 存量 | 06 §2.2 | `event.c:74-123` | 回答"最后一个订阅者回复后发生什么" |
| K-145 | `remove_sub` 的有序删除、游标回退与 `nested` 守卫 | 机制/不变量 | 存量 | 06 §2.3/§D6 | `event.c:130-161` | 回答"订阅者中途退订如何不跳过他人" |
| K-146 | `do_proceventmask`（`PRIV_PROC` 门、更新/退订/新增三态、`ENOMEM`） | 接口 | 存量 | 06 §2.4 | `event.c:170-211` | 回答"谁能订阅、掩码如何变更" |
| K-147 | `do_proc_event_reply` 的六重校验与五种 `SUSPEND` 返回 | 接口/错误路径 | 存量 | 06 §2.5 | `event.c:218-309` | 回答"错配回复为何一律 SUSPEND" |
| K-148 | `publish_event` 的入口断言与"订阅者已死"清理 | 机制 | 存量 | 06 §2.6/§D5 | `event.c:316-353` | 回答"退订竞态如何收尾" |
| K-149 | 不按进程过滤的竞态约束（必须在非受影响调用里订阅） | 约束 | 存量 | 06 §1.4 | `event.c:27-41` | 回答"为何 semget 可订阅而 semop 不行" |

### 2.9 生命周期：fork / srv_fork（旧 07 + 旧 08）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-160 | fork 为何是多服务协同（PM/VM/VFS/内核四张表的一致性窗口） | 概念 | 存量 | 07 §1.2 | `forkexit.c:45-140` | 回答"fork 为何不是一个函数能完成的事" |
| K-161 | 容量预检（`EAGAIN` 的两个条件） | 接口/约束 | 存量 | 07 §2.1 | `forkexit.c:60-65` | 回答"fork 何时返回 EAGAIN" |
| K-162 | `next_child` 轮转 + 双 panic 守卫 | 机制 | 存量 | 07 §2.2 | `forkexit.c:68-75` | 回答"槽位分配的失败为何是 panic" |
| K-163 | `vm_fork` 同步段与"PM 此后不可失败"窗口 | 机制/不变量 | 存量 | 07 §2.3 | `forkexit.c:78-82` | 回答"为何字段设置在 vm_fork 之后" |
| K-164 | `*rmc = *rmp` 全量复制 + `mp_sigact` 重指 + `memcpy` | 机制 | 存量 | 07 §2.4 | `forkexit.c:87-89` | 回答"复制后哪个字段必须立刻修正" |
| K-165 | `mp_parent = who_p` | 机制 | 存量 | 07 §2.4 | `forkexit.c:90` | — |
| K-166 | `TO_TRACEFORK` 条件继承（tracer / trace_flags / sigtrace 三件套） | 机制 | 存量 | 07 §D7；18 §1.1 | `forkexit.c:91-95`；`ptrace.h:209` | 回答"何时子进程自动被 trace" |
| K-167 | （新增）`PRIV_PROC` 子进程的 `mp_scheduler = SCHED_PROC_NR`（`assert(NONE)` 前置） | 机制 | **新增** | 无 | `forkexit.c:100-103` | 回答"系统服务器调普通 fork 时谁负责调度" |
| K-168 | flags 继承掩码 `IN_USE\|DELAY_CALL\|TAINTED`（**不含 PRIV_PROC**） | 机制 | 存量 | 07 §2.4 | `forkexit.c:106` | 回答"fork 后子进程丢了哪些状态" |
| K-169 | `child_utime/stime`、`exitstatus`、`sigstatus` 重置；`mp_interval[]=0`；`mp_started=getticks()` | 机制 | 存量 | 07 §2.4 | `forkexit.c:107-114` | 回答"记账为何从零起" |
| K-170 | `assert(rmc->mp_eventsub == NO_EVENTSUB)` | 不变量 | 存量 | 07 §2.4 | `forkexit.c:116` | 回答"为何新进程不能有在途事件" |
| K-171 | `get_free_pid` 分配（使用点） | 机制 | 存量 | 07 §2.5 | `forkexit.c:119` | — |
| K-172 | `VFS_PM_FORK` 构造（`ENDPT/PENDPT/CPID/REUID=-1/REGID=-1`）与 `tell_vfs` | 协议 | 存量 | 07 §2.6 | `forkexit.c:122-130` | 回答"VFS 需要哪些字段才能建 fproc" |
| K-173 | tracer `sig_proc(rmc, SIGSTOP, TRUE, FALSE)` | 机制 | 存量 | 07 §2.7 | `forkexit.c:133-134` | — |
| K-174 | `SUSPEND` 挂在子进程，回复在 `VFS_PM_FORK_REPLY`（child=OK / parent=pid） | 机制/协议 | 存量 | 07 §1.5/§3.8 | `main.c:369-396` | 回答"fork 的两个返回值怎么发出" |
| K-175 | `do_srv_fork`：`mp_endpoint != RS_PROC_NR → EPERM` | 接口/约束 | 存量 | 08 §2 | `forkexit.c:159-160` | 回答"谁可以创建系统服务" |
| K-176 | `do_srv_fork` 的 uid/gid 消息注入（real=eff=sv 三组全填） | 协议 | 存量 | 08 §2 | `forkexit.c:206-211` | 回答"系统服务的身份由谁指定" |
| K-177 | `do_srv_fork` 的 flags 掩码含 `PRIV_PROC`，且**立即** `reply(child,OK)` 并返回 pid | 机制 | 存量 | 08 §2 | `forkexit.c:200/237-239` | 回答"srv_fork 与 fork 的回复协议差异" |
| K-178 | `VFS_PM_FORK_REPLY` 的失败拆解：`mp_scheduler=NONE` → `exit_proc(rmp,-1,FALSE)` → 非 NEW_PARENT 则 `reply(parent,-1)` | 错误路径 | 存量 | 07 §2.8；05 §2.3 | `main.c:376-394` | 回答"调度器拒绝时 fork 如何回滚" |

### 2.10 生命周期：exit（旧 09）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-190 | 两阶段退出（VFS 回复前 / 后）与顺序敏感 | 概念 | 存量 | 09 §1.1 | `forkexit.c:267-469` | 回答"为何不能一口气清完" |
| K-191 | `PRIV_PROC` 走 `sys_kill(SIGKILL)` 而非 `exit_proc` | 机制/约束 | 存量 | 09 §2.1 | `forkexit.c:253-257` | 回答"系统进程为何不能自己 exit" |
| K-192 | `dump_core` 双抑制（setuid 执行 / PRIV_PROC） | 机制 | 存量 | 09 §2.2 | `forkexit.c:285-292` | 回答"何时不生成 core" |
| K-193 | 会话领导者的 `procgrp` 记忆 | 机制 | 存量 | 09 §2.2 | `forkexit.c:298` | 回答"SIGHUP 广播的触发条件" |
| K-194 | `ALARM_ON` → `set_alarm(rmp, 0)` | 机制 | 存量 | 09 §2.2 | `forkexit.c:301` | — |
| K-195 | `sys_times` 记账累加进 `mp_child_utime/stime`（POSIX 禁止 wait 前累加到父） | 机制/契约 | 存量 | 09 §2.2；10 §D5 | `forkexit.c:306-309` | 回答"CPU 时间为何先存在死者身上" |
| K-196 | 未停止则强制 `sys_stop` 并置 `PROC_STOPPED`（含延迟调用风险的 C TODO） | 机制/不变量 | 存量 | 09 §2.3 | `forkexit.c:326-330` | 回答"为何退出要先停进程" |
| K-197 | `vm_willexit` 预告 | 协议 | 存量 | 09 §2.3 | `forkexit.c:332` | — |
| K-198 | INIT 死亡（打印 + stacktrace + return）与 VFS 死亡（panic）特例 | 错误路径 | 存量 | 09 §2.3 | `forkexit.c:336-345` | 回答"关键进程死亡时系统如何反应" |
| K-199 | `VFS_PM_EXIT` / `VFS_PM_DUMPCORE`（带 `TERM_SIG` 与 `PATH`）投递 | 协议 | 存量 | 09 §2.3 | `forkexit.c:350-359` | — |
| K-200 | `PRIV_PROC` 不等 VFS 直接 `sys_clear`（避免 VFS 阻塞在块设备驱动） | 机制/约束 | 存量 | 09 §1.2/§2.3 | `forkexit.c:361-369` | 回答"为何系统进程要被直毁" |
| K-201 | `EXITING` 保留位掩码 `IN_USE\|VFS_CALL\|PRIV_PROC\|TRACE_EXIT\|PROC_STOPPED` | 数据结构/不变量 | 存量 | 09 §2.4 | `forkexit.c:374-375` | 回答"退出中进程还保留什么" |
| K-202 | `zombify` 两级僵尸（tracer 优先 → `TRACE_ZOMBIE`，再转 `ZOMBIE`） | 机制 | 存量 | 09 §2.6/§D5 | `forkexit.c:593-624` | 回答"为何僵尸有两级" |
| K-203 | `check_parent`：父正在等 → `tell_parent`+`cleanup`；否则 `sig_proc(SIGCHLD)` | 机制 | 存量 | 09 §2.6/§D6 | `forkexit.c:629-665` | 回答"父不等时如何被通知" |
| K-204 | `disinherit` → INIT 收养 + `NEW_PARENT`（在途 VFS 调用时） | 机制 | 存量 | 09 §2.4/§D7 | `forkexit.c:387-409` | 回答"孤儿进程的父是谁" |
| K-205 | SIGHUP 会话组广播 `check_sig(-procgrp, SIGHUP, FALSE)` | 机制 | 存量 | 09 §2.4/§D7 | `forkexit.c:412` | — |
| K-206 | `exit_restart` 六步：`sched_stop` → `mp_scheduler=NONE` → core 情形补 `zombify` → `sys_clear` → `vm_exit` → `TRACE_EXIT` 唤醒 / `TOLD_PARENT` 则 `cleanup` | 机制 | 存量 | 09 §2.5/§D8 | `forkexit.c:418-469` | 回答"VFS 回复后还剩什么" |
| K-207 | `tracer_died`：解监护 + 非 EXITING 则 SIGKILL、TRACE_ZOMBIE 则转 ZOMBIE | 机制 | 存量 | 09 §2.6 | `forkexit.c:759-790` | 回答"tracer 死了被跟进程怎么办" |
| K-208 | `cleanup`：`mp_pid=0`、`mp_flags=0`、记账清零、`procs_in_use--` | 机制/不变量 | 存量 | 09 §2.6；03 §2.5 | `forkexit.c:795-806` | — |

### 2.11 生命周期：wait（旧 10）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-220 | `pidarg` 四态（`>0` / `0→-procgrp` / `-1` / `<-1`）与归一化 | 接口 | 存量 | 10 §1.2/§2.1 | `forkexit.c:490-493` | 回答"waitpid 的参数语义" |
| K-221 | 三条件过滤与 `children` 计数 | 机制 | 存量 | 10 §2.2 | `forkexit.c:500-510` | 回答"哪些子进程算'合格'" |
| K-222 | 三环：TRACE_ZOMBIE → TRACE_STOPPED → ZOMBIE | 机制 | 存量 | 10 §1.4/§2.3 | `forkexit.c:512-547` | 回答"tracer 与 parent 的回收顺序" |
| K-223 | `W_STOPCODE(i)` 与 `mp_sigtrace` 扫描 | 协议 | 存量 | 10 §2.3；18 §2.3 | `forkexit.c:523-533` | 回答"wait 为何能返回'停止'状态" |
| K-224 | `WNOHANG` → 返回 0 | 接口 | 存量 | 10 §2.4 | `forkexit.c:553-555` | — |
| K-225 | `ECHILD`（无合格子进程） | 接口 | 存量 | 10 §2.4 | `forkexit.c:562` | — |
| K-226 | `WAITING` + `mp_wpid` + `mp_waddr` 三件套 | 数据结构 | 存量 | 10 §1.3/§2.4 | `forkexit.c:556-558` | 回答"异步等待需要记住什么" |
| K-227 | `wait_test`（tracer 视为伪父） | 机制 | 存量 | 10 §2.5 | `forkexit.c:569-588` | — |
| K-228 | `tell_parent`：`sys_datacopy(rusage)` → `W_EXITCODE` → `reply(pid)` → 清 `WAITING` → `ZOMBIE→TOLD_PARENT` → 时间累加 → 返回 TRUE/FALSE | 机制/协议 | 存量 | 10 §2.6/§D5 | `forkexit.c:670-726` | 回答"拷贝失败时子进程为何仍是僵尸" |
| K-229 | `tell_tracer`：`TRACE_ZOMBIE→ZOMBIE` 的伪父转换 | 机制 | 存量 | 10 §2.7/§D6 | `forkexit.c:731-754` | — |
| K-230 | `set_rusage_times` 的 `sys_hz()` 换算（仅填 `ru_utime`/`ru_stime`） | 机制 | 存量 | 10 §2.8；20 §2.11 | `utility.c:144-156` | 回答"为何 rusage 只填两个字段" |
| K-231 | POSIX 约束：子进程未被 wait 前不得累加到父 | 契约 | 存量 | 10 §D5；09 §2.2 | `forkexit.c:303-305` | — |

### 2.12 信号：模型与安装（旧 12 前半）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-250 | 信号作为异步通知的模型（生成 / 投递 / 处置三段） | 概念 | 存量 | 11 §1.1；12 §1.1 | `signal.c:1-19` | 回答"信号与 IPC 的差别" |
| K-251 | 七张位图：`mp_ignore`/`mp_catch`/`mp_sigmask`/`mp_sigmask2`/`mp_sigpending`/`mp_ksigpending`/`mp_sigtrace` | 数据结构 | 存量 | 12 §1.2；02 §2.3 | `mproc.h:53-59` | 回答"每个位图的读写者是谁" |
| K-252 | `sigaction` 三态与位图联动（IGN→ignore+清 pending/ksigpending/catch；DFL→清 ignore/catch；捕获→清 ignore+置 catch） | 机制 | 存量 | 12 §1.2/§2 | `signal.c:67-83` | 回答"为何安装 handler 会顺带清 pending" |
| K-253 | `mp_sigreturn`（C 库 `__sigreturn` 地址） | 数据结构/协议 | 存量 | 12 §2 | `signal.c:84` | 回答"handler 返回后怎么回到内核" |
| K-254 | KILL/STOP 三重封堵（不可捕获 / 不可忽略 / 不可屏蔽） | 契约 | 存量 | 12 §1.3 | `signal.c:80-81/124-125/141-142/166-167/186-187` | 回答"封堵发生在哪几处" |
| K-255 | `sigprocmask` 四种 `how` 与改掩码后必查挂起 | 机制 | 存量 | 12 §1.4/§2 | `signal.c:102-155` | 回答"为何改掩码要复查挂起信号" |
| K-256 | `sigpending` 查询 | 接口 | 存量 | 12 §2 | `signal.c:91-97` | — |
| K-257 | `sigsuspend`：`mp_sigmask2` 保存 → 换 mask → 置 `SIGSUSPENDED` → `check_pending` → `SUSPEND` | 机制 | 存量 | 12 §1.5/§2 | `signal.c:160-171` | 回答"原子等待如何实现" |
| K-258 | `sigreturn`：恢复 mask → `sys_sigreturn` → `check_pending` | 机制 | 存量 | 12 §1.5/§2 | `signal.c:176-192` | — |
| K-259 | `sa_mask` / `SA_NODEFER` / `SA_RESETHAND` / `sa_flags` | 接口 | 存量 | 12 §2；15 §D3 | `signal.c:800-813` | 回答"投递时掩码如何变化" |
| K-260 | 入口断言（非 `PROC_STOPPED\|VFS_CALL\|UNPAUSED\|EVENT_CALL`） | 不变量 | 存量 | 12 §2 | `signal.c:46/93/117/162/183` | 回答"为何这些调用不能在挂起态到达" |
| K-261 | 位基单点 `sig_bit(s) = 1 << (s-1)`（C `__sigmask`） | 架构演进 | 存量 | 99 §3 | `sigtypes.h`；`init.rs::sig_bit` | 回答"位运算为何必须收敛到一处" |

### 2.13 信号：生成与分派（旧 11）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-275 | `do_kill`（ksig=FALSE）/ `do_srv_kill`（RS 专属 + ksig=TRUE） | 接口 | 存量 | 11 §2.1 | `signal.c:197-221` | 回答"ksig 真假改变了什么" |
| K-276 | `check_sig` 四种 pid 语义 + **倒序**扫表 | 机制 | 存量 | 11 §1.2/§2 | `signal.c:597-603` | 回答"为何从表尾往前扫" |
| K-277 | INIT 保护：`proc_id==INIT_PID && signo==SIGKILL → EINVAL` | 契约 | 存量 | 11 §2 | `signal.c:585` | — |
| K-278 | SIGTERM 广播先 `sys_kill(RS_PROC_NR, SIGTERM)` | 机制 | 存量 | 11 §2 | `signal.c:588-589` | 回答"为何 RS 必须第一个知道关机" |
| K-279 | SIGKILL 广播跳过 `PRIV_PROC` | 契约 | 存量 | 11 §1.3/§2 | `signal.c:607-608` | — |
| K-280 | 完全跳过 VM（`VM_PROC_NR`，避免与其信号管理器死锁） | 契约 | 存量 | 11 §2 | `signal.c:613` | 回答"为何 VM 收不到广播信号" |
| K-281 | 非 ksig 的致命信号到 `PRIV_PROC` → `EPERM` | 契约 | 存量 | 11 §1.3/§2 | `signal.c:616-619` | — |
| K-282 | 权限四重比较（eff/real × 双方） | 契约 | 存量 | 11 §2 | `signal.c:622-629` | 回答"什么条件下允许发信号" |
| K-283 | 返回值语义：`count>0 ? OK : error_code`；调用者自毁 → `SUSPEND` | 接口 | 存量 | 11 §2 | `signal.c:644-645` | — |
| K-284 | `sig_proc` 九判定链（tracer → VFS/EVENT 挂起 → PRIV_PROC → badignore → ignore → mask → TRACE_STOPPED → catch → ign_sset → 终止） | 机制 | 存量 | 11 §1.4/§2 | `signal.c:383-540` | 回答"信号处置的优先级顺序" |
| K-285 | `sig_proc_exit` 与 `core_sset` / `WCOREFLAG` | 机制 | 存量 | 11 §2；09 | `signal.c:545-563` | 回答"何时触发 core dump" |
| K-286 | tracer 分支：`sigaddset(mp_sigtrace)` + `trace_stop` | 机制 | 存量 | 11 §2；18 | `signal.c:411-423` | — |

### 2.14 信号：停止、延迟与恢复（旧 13）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-300 | `sys_delay_stop` 与 `EBUSY` → `DELAY_CALL` 的配对 | 机制 | 存量 | 13 §1.1 | `signal.c:226-261` | 回答"进程正发消息时如何停止" |
| K-301 | `PROC_STOPPED` 的双用途（已停止 + 需重检） | 数据结构/契约 | 存量 | 13 §1.2 | `signal.c:437/699/704` | 回答"为何一个位能当两个用" |
| K-302 | `stop_proc(rmp, may_delay)` 与 `may_delay=FALSE` 时的 panic | 机制/不变量 | 存量 | 13 §1.3 | `signal.c:226-261` | 回答"调用者何时可断言无延迟调用" |
| K-303 | `SIGSNDELAY=70` 与延迟调用的恢复路径（`process_ksig` 尾部） | 机制 | 存量 | 13 §1.1；11 §2 | `signal.c:344-369` | 回答"延迟调用完成后谁接手" |
| K-304 | `try_resume_proc`：`VFS_CALL\|EVENT_CALL\|EXITING` 时不恢复，否则 `sys_resume` 并清 `PROC_STOPPED\|UNPAUSED` | 机制 | 存量 | 13 §2 | `signal.c:266-289` | 回答"什么情况下不能恢复" |
| K-305 | `unpause` 三路径 | 机制 | 存量 | 13 §1.4/§2 | `signal.c:719-770` | 回答"PM 睡与 VFS 睡为何不同" |
| K-306 | `VFS_PM_UNPAUSE` 请求与 `UNPAUSED` 置位 | 协议 | 存量 | 13 §2；05 §2.3 | `signal.c:763-767`；`main.c:403-415` | — |
| K-307 | `check_pending` 循环到 `VFS_CALL\|EVENT_CALL` 就 `break` | 机制/不变量 | 存量 | 13 §1.5/§2 | `signal.c:651-682` | 回答"为何不能一次投递完" |
| K-308 | `restart_sigs` 的 `TRACE_EXIT` 优先于信号重检 | 机制 | 存量 | 13 §1.6/§2 | `signal.c:687-714` | — |
| K-309 | （新增）`DELAY_CALL` 在 Rust 互斥建模下不可表示（C 半边守卫的等价性论证） | 架构演进 | 新增 | todo.md §12（V3-P1-3） | `signal.rs` 注释 | 回答"为何 Rust 少一个位却等价" |

### 2.15 信号：投递与内核回环（新增）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-320 | `sigmsg` 组装（`sm_mask` / `sm_signo` / `sm_sighandler` / `sm_sigreturn`） | 协议 | 存量（散在 12/13） | 12 §1.6 | `signal.c:784-799`；`type.h:71-77` | 回答"投递给内核的载荷是什么" |
| K-321 | `SIGSUSPENDED` 时 `sm_mask` 以 `mp_sigmask2` 起底、而 `mp_sigmask` 以当前 mask 起底（两值不同） | 机制/纠错 | 新增 | 无（todo.md Fix #49） | `signal.c:792-795` | 回答"为何两处掩码取值不同" |
| K-322 | `sa_mask` 并入 `mp_sigmask` | 机制 | 存量 | 12 §2 | `signal.c:800-803` | — |
| K-323 | `SA_NODEFER` / `SA_RESETHAND` 的投递时效应 | 机制 | 存量 | 12 §2 | `signal.c:805-813` | — |
| K-324 | `sys_sigsend` 与 `EFAULT`/`ENOMEM` 的**合法**失败分档（其余错误 panic） | 错误路径 | 存量 | 12 §2 | `signal.c:818-829` | 回答"为何这两个 errno 不是 bug" |
| K-325 | `WAITING\|SIGSUSPENDED` 时清位 + `reply(slot, EINTR)` + `try_resume_proc` | 机制 | 存量 | 12 §2；13 | `signal.c:831-844` | 回答"EINTR 从哪来" |
| K-326 | 非 PM 睡时的 `assert(UNPAUSED)`（来自 `restart_sigs` 路径） | 不变量 | 存量 | 12 §2 | `signal.c:845-851` | — |
| K-327 | `sef_setcb_signal_manager(process_ksig)` 注册点 | 机制 | **新增** | 01 §2.2 只列注册，未讲语义 | `main.c:122` | 回答"信号管理器回调如何被安装" |
| K-328 | SIGKSIG / SIGKSIGSM 拦截与 `sys_getksig`/`sys_endksig` 拉取循环（NONE 终止） | 机制 | **新增** | 无 | `libsys/sef_signal.c`；`signal.rs::process_sigmgr_signals` | 回答"内核积累的信号如何被取回" |
| K-329 | `process_ksig` 的 `EDEADEPT` 双检（入口 + 出口） | 错误路径 | 存量 | 11 §1.5/§2 | `signal.c:294-310/371-377` | 回答"为何检查两次" |
| K-330 | `process_ksig` 的 signo 分派：INT/QUIT/WINCH/INFO → 组广播（id=0）；VTALRM/PROF → `check_vtimer` 后落穿；SIGKILL → 全域广播（id=-1）；其余 → id=proc_id | 机制 | 存量 | 11 §2 | `signal.c:320-334` | 回答"内核信号的 pid 语义如何被翻译" |
| K-331 | `mp = &mproc[0]` 伪装信号源 + `mp_procgrp` 借位 + 还原 | 机制/技巧 | **新增** | 无 | `signal.c:312-313/335`；`alarm.c:341` | 回答"为何 PM 槽位 0 被临时改写" |
| K-332 | 内核侧 sigframe/sigcontext（`sys_sigsend` 对端） | 跨模块契约 | 存量（回指） | 12 §D* | `../01-stage-kernel/` | 回答"栈帧由谁构造" |

### 2.16 exec（旧 17）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-345 | `do_exec` 转发 `VFS_PM_EXEC`（PATH/PATH_LEN/FRAME/FRAME_LEN/PS_STR） | 协议 | 存量 | 17 §1.1/§2 | `exec.c:38-56` | 回答"权限判定为何在 VFS" |
| K-346 | `do_newexec` 的调用者门（`who_e` 必须是 VFS 或 RS） | 接口/约束 | 存量 | 17 §2 | `exec.c:70-71` | — |
| K-347 | `exec_info` 经 `sys_datacopy` 拷入（失败 panic） | 协议 | 存量 | 17 §2 | `exec.c:79-81` | — |
| K-348 | `allow_setuid` 与 tracer 约束（有 tracer 则不允许 setuid） | 契约 | 存量 | 17 §1.2/§2 | `exec.c:86-89` | 回答"为何被调试的程序不能提权" |
| K-349 | `mp_svuid`/`mp_svgid` 无条件更新为当前 eff | 机制 | 存量 | 17 §2 | `exec.c:97-98` | — |
| K-350 | `TAINTED` 二重判定（setuid 位 / eff≠real） | 机制 | 存量 | 17 §1.2/§2；15 §1.6 | `exec.c:103-109` | — |
| K-351 | `mp_name` 更新（截断到 `PROC_NAME_LEN-1`）与 `mp_frame_addr = stack_high - frame_len` | 机制 | 存量 | 17 §1.6/§2 | `exec.c:112-117` | — |
| K-352 | `PARTIAL_EXEC` 哨兵（已换映射、未装内容） | 数据结构/契约 | 存量 | 17 §1.3/§2 | `exec.c:120` | 回答"exec 失败后为何必须杀进程" |
| K-353 | `exec_restart` 失败分支：`PARTIAL_EXEC` → `sys_kill(SIGKILL)`；否则 `reply(result)` | 错误路径 | 存量 | 17 §2 | `exec.c:161-171` | — |
| K-354 | `exec` 后 `catch` 重置为 `SIG_DFL`（`ignore` 保留，不清 `sa_flags`） | 机制/纠错 | 存量 | 17 §1.4/§2 | `exec.c:178-184` | 回答"为何 ignore 保留而 handler 不保留" |
| K-355 | tracer 的 `SIGTRAP`/`SIGSTOP`（`TO_ALTEXEC` / `TO_NOEXEC`），且必须在 `sys_exec` **之前** | 机制 | 存量 | 17 §1.5/§2 | `exec.c:189-194`；`ptrace.h:210-211` | 回答"exec 后调试器如何拿到断点" |
| K-356 | `sys_exec(endpoint, sp, name, pc, ps_str)`（失败 panic） | 接口 | 存量 | 17 §2 | `exec.c:197-198` | — |
| K-357 | `do_execrestart`（RS 专用，走 `mp_frame_addr`） | 接口 | 存量 | 17 §2 | `exec.c:130-151` | 回答"RS 与普通进程的 exec 路径差异" |
| K-358 | `ESCRIPT` 死代码（`#define ESCRIPT (-2000)` 全源零使用） | 死代码/排除项 | 存量 | 17 §2；plan §5.4 | `exec.c:31` | 回答"为何 Rust 不实现它" |

### 2.17 定时器（旧 14）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-370 | `ITIMER_REAL` / `VIRTUAL` / `PROF` 三族分野 | 概念 | 存量 | 14 §1.1 | `alarm.c:125-144` | 回答"三族计时口径的差别" |
| K-371 | REAL 走 `set_timer`/`cancel_timer`（`minix_timer_t`）+ `cause_sigalrm` | 机制 | 存量 | 14 §1.3/§2 | `alarm.c:297-311` | — |
| K-372 | VIRTUAL/PROF 走 `sys_vtimer`（`VT_VIRTUAL`/`VT_PROF`） | 机制 | 存量 | 14 §1.3/§2 | `alarm.c:196-216` | 回答"为何虚拟时钟必须由内核计" |
| K-373 | `ticks_from_timeval` 的**向上取整**与 `LONG_MAX` 钳位 | 机制/契约 | 存量 | 14 §1.2/§2 | `alarm.c:33-65` | 回答"为何不能简单乘 hz" |
| K-374 | `timeval_from_ticks`（`sec = ticks/hz`，`usec = (ticks%hz)*1e6/hz`） | 机制 | 存量 | 14 §2 | `alarm.c:70-76` | — |
| K-375 | `is_sane_timeval` + `MAX_SECS` | 接口/约束 | 存量 | 14 §2 | `alarm.c:81-87`；`const.h:15` | — |
| K-376 | `setval`/`getval` 双指针语义（至少一个非零；双向 `sys_datacopy`） | 接口 | 存量 | 14 §1.4/§2 | `alarm.c:107-123/147-151` | 回答"为何两个指针都空要 EINVAL" |
| K-377 | `getset_vtimer` 的 `oldticks <= 0 → interval` 回绕 | 机制 | 存量 | 14 §1.5/§2 | `alarm.c:208-214` | 回答"已到期时为何返回 interval" |
| K-378 | `get_realtimer` 的 `remaining = exptime - uptime`（同样回绕） | 机制 | 存量 | 14 §2 | `alarm.c:253-266` | — |
| K-379 | `cause_sigalrm`：周期重挂或清 `ALARM_ON`，再 `check_sig(mp_pid, SIGALRM, FALSE)` | 机制 | 存量 | 14 §1.6/§2 | `alarm.c:313-344` | 回答"周期定时器如何在回调里重挂" |
| K-380 | CLOCK notify → `expire_timers`（驱动源） | 机制 | 存量 | 14 §1.6；05 §2.2 | `main.c:65-67` | — |

### 2.18 凭证（旧 15）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-395 | real / effective / saved 三元组 | 概念 | 存量 | 15 §1.1 | `mproc.h:40-46` | 回答"三个 id 各自的用途" |
| K-396 | `setuid` 的 BSD 全置语义；非 root 且 `real≠uid` → EPERM | 机制/契约 | 存量 | 15 §1.2/§2 | `getset.c:111-119` | 回答"为何 setuid 不是只改 eff" |
| K-397 | `seteuid`（`svuid` 参与判定）/ `setgid` / `setegid` | 机制 | 存量 | 15 §2 | `getset.c:128-170` | — |
| K-398 | `setgroups`：root 门、`NGROUPS_MAX`、`GID_MAX`、尾零填充、EFAULT 判定 | 接口/契约 | 存量 | 15 §2 | `getset.c:172-197` | 回答"为何要补零" |
| K-399 | `getgroups` 二阶段（`ngroups==0` 返回数量；`ngroups < 实际` → EINVAL） | 接口 | 存量 | 15 §1.4/§2 | `getset.c:29-50` | — |
| K-400 | `setsid`：`mp_procgrp == mp_pid → EPERM` | 契约 | 存量 | 15 §1.5/§2 | `getset.c:205-207` | — |
| K-401 | `getsid`（走 `find_proc`）/ `getpid`（带 parent_pid 载荷）/ `getpgrp` | 接口 | 存量 | 15 §2 | `getset.c:61-79` | — |
| K-402 | `issetugid = !!(mp_flags & TAINTED)` | 接口 | 存量 | 15 §1.6/§2；17 §1.2 | `getset.c:80-82` | 回答"LD_PRELOAD 防注入的依据" |
| K-403 | VFS 转发四请求（SETUID/SETGID/SETGROUPS/SETSID）+ `SUSPEND` | 协议 | 存量 | 15 §1.3/§2 | `getset.c:219-222` | 回答"为何改凭证要通知 VFS" |
| K-404 | `VFS_PM_SET*_REPLY` → `reply(OK)` / SETSID 回 `reply(procgrp)` | 协议 | 存量 | 15 §2；05 §2.3 | `main.c:335-347` | — |

### 2.19 调度（旧 16）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-420 | 用户态调度交接（内核只提供原语，策略在 SCHED） | 概念 | 存量 | 16 §1.1 | `schedule.c:1-15`；`minix/sched.h` | 回答"PM 为何不管调度算法" |
| K-421 | `sched_init`：只接管 `IN_USE && !PRIV_PROC`，两个 `assert` | 机制 | 存量 | 16 §2；01 §2.6 | `schedule.c:20-50` | 回答"为何循环里实际只处理 INIT" |
| K-422 | `sched_start_user`：`nice_to_priority` + `inherit_from`（PRIV_PROC 父 → INIT）+ `sched_inherit` | 机制/纠错 | 存量 + 新增（V3-P2-3） | 16 §1.3/§2 | `schedule.c:55-84`；`main.c:370-373` | 回答"系统进程的子进程为何从 INIT 继承" |
| K-423 | `sched_nice`：`KERNEL`/`NONE` 调度器 → EINVAL；否则发 `SCHEDULING_SET_NICE` | 机制 | 存量 | 16 §1.4/§2 | `schedule.c:89-112` | 回答"为何内核调度的进程不能改 nice" |
| K-424 | `nice_to_priority` 的线性缩放与钳位（41 档 nice vs 16 队列的非整数比） | 机制 | 存量 | 16 §1.2/§2 | `utility.c:91-103` | 回答"换算为何必然有量化误差" |
| K-425 | `get_nice_value` 是上者的逆（queue→nice，`USER_Q → 0`） | 机制 | 存量 | 16 §1.6 | `main.c:275-289` | — |
| K-426 | `do_getsetpriority`：只支持 `PRIO_PROCESS`；权限三重；降 nice 需 root（EACCES）；GET 返回 `nice - PRIO_MIN` | 接口/契约 | 存量 | 16 §1.5/§2 | `misc.c:238-286` | 回答"读与写的权限为何不对称" |
| K-427 | `sched_stop` 在 `exit_restart` 的调用点与"调度器拒绝时只打印" | 错误路径 | 存量 | 16 §2；09 §2.5 | `forkexit.c:425-434` | — |
| K-428 | `SCHEDULING_SET_NICE` 消息与 `_taskcall` 通道 | 协议 | 存量 | 16 §2 | `schedule.c:105-109` | — |
| K-429 | `SEND_PRIORITY` / `SEND_TIME_SLICE` 未使用（未来扩展位） | 契约/排除项 | 新增 | 仅 plan §5.4 | `const.h:19-20` | — |

### 2.20 ptrace（旧 18）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-445 | `T_*` 18 个请求常量与真值（`T_STOP=-1`、`T_READB_INS=100`…`T_SETRANGE=107`；`T_OK=0`、`T_GETINS=1`、`T_GETDATA=2`、`T_SETINS=4`、`T_SETDATA=5`、`T_RESUME=7`、`T_EXIT=8`、`T_ATTACH=9`、`T_DETACH=10`、`T_SYSCALL=14`） | 常量/契约 | 存量（**旧 18 有事实错误**） | 18 §2 | `sys/ptrace.h:37-55/226-250` | 回答"请求码真值与哪里对齐" |
| K-446 | `TO_TRACEFORK=0x1` / `TO_ALTEXEC=0x2` / `TO_NOEXEC=0x4` | 常量 | 存量（**旧 18 §2.14 写 TO_NOEXEC=0x1 → 错**） | 18 §2.14；09/17 | `sys/ptrace.h:209-211` | — |
| K-447 | `T_OK` 自声明（`mp_tracer = mp_parent`，已有 tracer → EBUSY） | 接口 | 存量 | 18 §1.1/§2 | `trace.c:55-60` | — |
| K-448 | `T_ATTACH` 六重权限链 | 契约 | 存量 | 18 §2 | `trace.c:62-93` | 回答"ptrace 的权限边界" |
| K-449 | `T_STOP` 不对用户程序开放（返回 EINVAL） | 契约 | 存量 | 18 §2 | `trace.c:95-99` | — |
| K-450 | `T_READB_INS` / `T_WRITEB_INS` 的 **root 专属门在通用守卫之前** | 契约 | 存量 | 18 §2 | `trace.c:101-134` | 回答"为何这两支要提前返回" |
| K-451 | `T_GETRANGE` / `T_SETRANGE`：`ptrace_range` 拷入、`TS_INS`/`TS_DATA` 校验、`pr_size` 上界、`sys_vircopy` 双向 | 接口 | 存量 | 18 §1.6/§2 | `trace.c:167-188` | — |
| K-452 | `T_EXIT`：`TRACE_EXIT` 置位 + `VFS\|EVENT` 在途则存 `mp_exitstatus` 延后，否则 `exit_proc` | 机制 | 存量 | 18 §1.4/§2 | `trace.c:146-159` | — |
| K-453 | `T_SETOPT`（`mp_trace_flags = data`） | 接口 | 存量 | 18 §2 | `trace.c:161-165` | — |
| K-454 | `T_DETACH`：解监护 → `mp_sigtrace` 逐信号 `check_sig` 重放 → `data>0` 再投 → 清 `TRACE_STOPPED`/`trace_flags` → `check_pending` → 落穿内核 `sys_trace` | 机制 | 存量 | 18 §1.5/§2 | `trace.c:190-215` | 回答"detach 时挂起的 tracer 信号去哪了" |
| K-455 | `T_RESUME`/`T_STEP`/`T_SYSCALL`：data 信号 → 若仍有 `mp_sigtrace` 则"假装成功"返回 → 否则清 `TRACE_STOPPED` + `check_pending` → 落穿 `sys_trace` | 机制 | 存量 | 18 §2 | `trace.c:217-243` | 回答"为何有信号时返回 OK 却不恢复" |
| K-456 | 通用守卫（`find_proc` / `EXITING` / `tracer==who_p` / `TRACE_STOPPED` → EBUSY） | 契约 | 存量 | 18 §2 | `trace.c:140-143` | — |
| K-457 | `trace_stop`：`sys_trace(T_STOP)` → 置 `TRACE_STOPPED` → `wait_test` 命中则 `W_STOPCODE` + `reply(tracer, pid)` | 机制/协议 | 存量 | 18 §2；10 §2.3 | `trace.c:255-276` | — |
| K-458 | `TRACE_STOPPED` 独立于 `PROC_STOPPED`（调试暂停 vs 异步暂停） | 数据结构/契约 | 存量 | 18 §1.2 | `mproc.h:89/93` | — |
| K-459 | `mp_sigtrace` 作为 tracer 的 pending 缓冲 | 数据结构 | 存量 | 18 §1.3 | `mproc.h:59` | — |
| K-460 | 内核 `sys_trace` 透传与错误保真（不折叠为 EINVAL） | 错误路径 | 存量 | 18 §2；23 | `trace.c:244-249` | — |

### 2.21 时间（旧 19）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-475 | `CLOCK_REALTIME` / `CLOCK_MONOTONIC` 双时钟 | 概念 | 存量 | 19 §1.1 | `time.c:31-40` | — |
| K-476 | `getuptime` 三值（ticks / realtime / boottime） | 接口 | 存量 | 19 §2.6 | `libsys/getuptime.c` | — |
| K-477 | `sec = boottime + clock/hz`、`nsec = (clock%hz)*1e9/hz`（先取余防溢出） | 机制 | 存量 | 19 §1.2 | `time.c:42-44` | 回答"为何不能先乘" |
| K-478 | `do_getres`：`tv_sec=0`、`tv_nsec=1e9/hz` | 接口 | 存量 | 19 §1.3 | `time.c:53-65` | — |
| K-479 | `do_settime`：root 门；MONOTONIC 不可变；`now` 分叉 | 接口/契约 | 存量 | 19 §1.5/§2 | `time.c:71-88` | — |
| K-480 | `do_stime`：`boottime = sec - realtime/hz` → `sys_stime` | 机制 | 存量 | 19 §1.4/§2 | `time.c:110-131` | — |
| K-481 | `do_time`（`clock_time`）与 `do_gettime(REALTIME)` 的 sec 同值、nsec 精度分叉 | 机制 | 存量 | 19 §1.6/§2 | `time.c:94-104` | — |
| K-482 | `system_hz` 的单点依赖（启动期取一次） | 契约 | 存量 | 19 §2.10 | `main.c:238` | — |

### 2.22 杂项查询与控制（旧 20）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-495 | `uts_tbl[8]` 兼容间接与 `sysuname` 的 `req` 方向门（req≠0 → EINVAL） | 接口/兼容层 | 存量 | 20 §1.1/§2.1 | `misc.c:31-61/72-100` | 回答"为何 uname 有张间接表" |
| K-496 | `getsysinfo`：`effuid==0` 门 + `size` 精确匹配；`SI_PROC_TAB` / `SI_CALL_STATS` | 接口/契约 | 存量（**数据路径已闭环，文档口径陈旧**） | 20 §1.2/§2.2 | `misc.c:108-144`；`minix-types/src/types/mproc.rs` | 回答"谁能读整张进程表" |
| K-497 | `getprocnr`：RS 专属 + `find_proc` | 接口/契约 | 存量 | 20 §1.3/§2.3 | `misc.c:149-164` | — |
| K-498 | `getepinfo`：`ngroups` 填**全量**、拷贝数单独截断 | 接口/纠错 | 存量（V3-P2-4 修正） | 20 §1.3/§2.4 | `misc.c:169-193` | 回答"返回值与拷贝数为何不等" |
| K-499 | `reboot` 定序：`SIGKILL` 广播 → `sys_stop(INIT)` → `VFS_PM_REBOOT`；`RB_POWERDOWN` 时经 DS 查 `readclock.drv` | 机制/协议 | 存量 | 20 §1.4/§2.5 | `misc.c:198-233` | 回答"为何先杀后重启" |
| K-500 | `svrctl`：`IOCGROUP` 门（'P'/'M'）+ `sysgetenv` 拷入 + `local_param_overrides[2]` + `find_param` | 接口/机制 | 存量 | 20 §1.5/§2.6 | `misc.c:291-395` | 回答"启动参数如何被运行时覆盖" |
| K-501 | `getrusage`：SELF 走 `sys_times`、CHILDREN 走 `mp_child_*`；`set_rusage_times`；`vm_getrusage`；拷出 | 机制/协议 | 存量 | 20 §1.7/§2.7 | `misc.c:400-447` | 回答"三段式数据从哪来" |
| K-502 | `sprofile`：`#if SPROFILE` 门，默认 `ENOSYS`；Rust `sprofile` feature | 工具/排除项 | 存量 | 20 §2.8 | `profile.c:22-45` | — |
| K-503 | `mcontext` 双向透传 | 接口 | 存量 | 20 §2.9 | `mcontext.c:12-26` | — |
| K-504 | `find_param` 的 `monitor_params` 线性遍历 | 机制 | 存量 | 20 §2.10 | `utility.c:56-71` | — |

### 2.23 失败模型、不变量与 DEFERRED 契约（新增）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-520 | panic 全集与其语义（"不可恢复的不变式"而非错误处理） | 错误路径 | 新增 | 散见各篇 §2 | 全源 `panic(` | 回答"哪些失败 C 选择直接死" |
| K-521 | errno 面（EINVAL/EPERM/ESRCH/ECHILD/EAGAIN/EBUSY/EDEADEPT/ENOSYS/EINTR/EFAULT/E2BIG/ENOSPC/EACCES/ENOMEM）与触发条件 | 错误路径 | 新增 | 散见 | 各 handler | 回答"用户态能观察到哪些错误" |
| K-522 | 错误保真规约：透传型调用用 `Kernel(i32)` 原样上抛，只有 PM 自身判定才产生语义化变体 | 架构演进/契约 | 新增 | `plan.md:231-233`（规约在，未成篇） | `SchedError::Kernel` 等 | 回答"为何禁止 `map_err(\|_\| Inval)`" |
| K-523 | DEFERRED 契约格式与 D-01..D-32 台账 | 工程/契约 | 新增 | `todo.md:135-167`（台账在，正文无） | `todo.md:135-167` | 回答"未实现部分如何被诚实登记" |
| K-524 | fail-closed vs fail-open 的选择（getsysinfo 返回 ENOSYS 而非全零表） | 契约/架构 | 新增 | `todo.md` §12 V3-P1-4 | `misc.rs` | 回答"假数据为何比报错更糟" |
| K-525 | 断言的 Rust 对应（`assert!` / `debug_assert!` / `panic!` 分层） | 架构演进 | 新增 | 无 | `os/servers/pm/src/**` | 回答"C 的 assert 在 Rust 里落在哪一层" |
| K-526 | 诊断输出口径（C `printf` = 内核 `SYS_DIAGCTL`；Rust 当前 `cfg(test)`，D-31） | 跨模块契约 | 新增 | `todo.md` §12 V3-P3 | `todo.md:523` | 回答"no_std 用户态如何打印" |

### 2.24 对端服务协议面（新增）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-540 | 内核能力清单（`sys_kill/clear/abort/times/runctl/vircopy/datacopy/trace/getksig/endksig/sigsend/sigreturn/setalarm/vtimer/exec/stime/settime/setmcontext/getmcontext/sprof/diagctl/hz/getmonparams/getimage/uptime`） | 跨模块接口 | 新增（`plan.md §4.1` 有表，未成篇） | `plan.md §4.1` | `minix-sys/src/syscall.rs` | 回答"PM 能用内核的哪些能力" |
| K-541 | `KernelGateway` 与窄 trait 的双风格规约（三条） | 架构演进 | 新增 | `plan.md §4.1`；00 §4.1 | `exit.rs`；`signal_flow.rs` | 回答"30 个端口 trait 为何这样分层" |
| K-542 | VM 协议面（`vm_fork` / `vm_willexit` / `vm_exit` / `vm_getrusage`） | 跨模块接口 | 新增 | 07 §2.3；09 §2.3 | `minix/vm.h` | 回答"PM 需要 VM 做哪四件事" |
| K-543 | VFS 协议面（12 RQ + 11 RS 全表） | 跨模块接口 | 存量 | 05 §2.1 | `com.h:520-544` | — |
| K-544 | RS 协议面（`srv_fork` / `srv_kill` / `exec_restart` / `getprocnr` / INIT 的父身份 / `PRIV_PROC` 继承） | 跨模块接口 | 新增 | 08；17 §2；20 §2.3 | `forkexit.c:159`；`exec.c:136` | 回答"PM 与 RS 的全部接触点" |
| K-545 | SCHED 协议面（`sched_start` / `inherit` / `stop` / `nice` + `SCHEDULING_SET_NICE`） | 跨模块接口 | 存量 | 16 §2 | `minix/sched.h`；`libsys/sched_start.c` | — |
| K-546 | MIB 协议面（`SI_PROC_TAB` 的 C-ABI 464B 镜像序列化） | 跨模块接口 | 新增 | 20 §D2（提及未展开） | `minix-types/src/types/mproc.rs`；`mproc/wire.rs` | 回答"进程表如何跨服务导出" |
| K-547 | 用户态启动与运行时装载（PM 由 RS 加载、ELF 装载、用户态入口、`no_std` runtime） | 非 C 制品/跨模块 | 新增 | 无（属 03/14-stage） | `../03-stage-rs/`；`../14-stage-runtime/` | 回答"PM 的二进制怎么变成进程" |

### 2.25 测试基建与实施台账（新增）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-548 | 测试三层：单元（`--lib`，403）/ 集成（11）/ doctest（当前失败） | 测试性质 | 新增（数字散在各篇 §5 且已腐烂） | 00 §3.3 + 各篇 §5 | `os/servers/pm/` | 回答"怎么跑、跑什么" |
| K-549 | 覆盖率 Gate（A：109 C 符号 / 100% 文档覆盖 / 93.6% 名称匹配；E：文档测试名对账） | 工具/工程 | 新增 | `todo.md` §12 | `tools/coverage-extract/` | 回答"覆盖度怎么度量" |
| K-550 | 47 调用接线批次台账（A–H）与 ENOSYS 兜底臂现状 | 工程/易腐烂 | 存量（00 §3.1 已腐烂：说 8 个接线，实测 47 臂全接） | 00 §3.1；`todo.md` §11.1.1 | `ipc/calls.rs` | 回答"还有多少没接完" |
| K-551 | 测试名对账纪律（声称名必须 grep 命中；数量声明带日期与命令） | 工程/契约 | 新增 | `todo.md` §12 V3-P1-5 | — | 回答"如何防止文档谎报测试" |

### 2.26 统计摘要

| 维度 | 数量 |
|------|------|
| 知识点总条数 | **171**（K-001 ~ K-551，编号不连续） |
| 存量（现有文档承载） | 148 |
| 新增（C / 非 C 制品 / OS 理论承载，文档缺失） | 23 |
| 按类型：概念 / 机制 / 数据结构 / 接口与协议 / 约束与不变量 / 架构演进 / 工具与工程 / 测试性质 | 12 / 63 / 18 / 33 / 24 / 16 / 12 / 3 |
| 按现有文档分布 Top 5 | 07-pm-fork 21、09-pm-exit 20、10-pm-wait 15、03-mproc-table 14、04-ipc-dispatch 13 |
| 现有文档承载为 0 的知识点 | 23（全部为新增） |

---

## 3. 覆盖审计

### 3.1 主题全集（四路来源）

1. **C 源码符号面**：`coverage-extract.py` 实测 **109 个**（71 函数 + 38 宏），15 个 .c + 6 个 .h 全部纳入；文档覆盖率 100%，Rust 名称匹配 102（93.6%）。7 个未匹配逐条核实：`NO_EVENTSUB`（有语义表达 `Option<EventCursor>`）、`SEND_PRIORITY`/`SEND_TIME_SLICE`（真缺口，挂调度器扩展 / E7）、`ESCRIPT`（C 死代码）、`EXTERN`/`_SYSTEM`/`_TABLE`（C 编译宏）。
2. **OS 通用概念**：进程状态机、僵尸与孤儿、PID 轮转域、权限模型（real/eff/saved）、信号三段模型（生成/投递/处置）、进程组与会话、rusage 记账、间隔定时器三族、nice↔优先级队列、ptrace 的停止-检视-恢复模型。
3. **非 C 制品**：`minix.service.mk` / `Makefile`（15 个 SRCS）、`os/servers/pm/Cargo.toml`（bin+lib 双目标 + 2 features）、`minix/ipc.h` wire 结构、`minix-sys` 的 `SYS_*` trap 面、`tests/run_once_integration.rs`、`tools/coverage-extract/pm-semantic-map.json`、`tools/design-coverage-check.sh`。
4. **阶段边界契约**：`edge_todo.md` 的 E1（trap 层）、E2/E6（`SYS_*` wrapper）、E5（联调）、E7（wire 系统化）、E9（Api 三分域）；`plan.md` §5.4 排除项。

### 3.2 重复主题表

| # | 主题 | 重复位置 | 新目录主讲述点 | 其余改为 |
|---|------|---------|--------------|---------|
| R-1 | 主循环骨架（真序 L1-L11） | 00 §1.3、01 §2.1、04 §2.1 | **05-pm-main-loop** | 00 只给一句话 + 指向；01 只讲到"进入循环为止" |
| R-2 | `SUSPEND` / `ReplyIntent` 语义 | 04 §1.4、05 §2.4、07 §3.8、09 §2.7、10 §2.10 | **05-pm-main-loop** | 各服务篇只写"本调用返回哪种意图、由谁回复" |
| R-3 | `get_free_pid` 相位与双字段扫描 | 03 §2.4、07 §2.5 | **03-pm-identity-table** | 09 只写调用点 |
| R-4 | `pm_isokendpt` 三层检查 | 03 §2.2、04 §2.3、07 §2.x | **03-pm-identity-table** | 05 只写调用步骤 |
| R-5 | 信号三集合 | 99 §2.3、01 §3.3、11 §1.4 | **01-pm-constants**（定义与构建） | 12 只引用；13 只写 `badignore` 判定使用 |
| R-6 | endpoint 代际编码 | 99 §2.2、03 §1.4 | **01-pm-constants**（编码算术） | 03 只写 `pm_isokendpt` 校验门 |
| R-7 | 容量纪律（`procs_in_use` / `LAST_FEW`） | 03 §1.5、07 §2.1、08 §2 | **03-pm-identity-table** | 09 只写调用点 |
| R-8 | `NEW_PARENT` | 05 §2.5、07 §2.6/§3.8、09 §2.4 | **07-pm-vfs-protocol**（生命周期） | 09/10 只写置位点与消费点 |
| R-9 | `wait_test`（tracer 伪父） | 10 §2.5、18 §2（trace_stop 用） | **11-pm-wait** | 20 只写调用点 |
| R-10 | `check_sig` 的权限与广播规则 | 11 §1.2/§2、13、09（SIGHUP）、17（tracer 信号） | **13-pm-signal-dispatch** | 其余只写"谁在什么时机调用、传什么参数" |
| R-11 | `sig_send` | 12 §1.6/§2、13（unpause 耦合） | **15-pm-signal-delivery** | 12 只讲安装与掩码 |
| R-12 | 测试基线数字与测试名清单 | 00 §3.3 + 21 篇 §5 | **25-pm-test-and-status**（唯一真值源） | 各篇 §5 改为"验证锚点"（命令 + 断言名 + 日期），不写总数 |
| R-13 | 实施现状 / 接线批次 / 跨阶段依赖 | 00 §3.1-3.3 | **25-pm-test-and-status** | 00 完全剥离 |
| R-14 | `unpause` 与 `stop_proc` | 13 §1.3/§1.4、12（被 sig_send 引用） | **14-pm-signal-suspend-resume** | 15 只写"调用前置条件" |

### 3.3 越界主题表

| # | 越界内容 | 现位置 | 正确归属 |
|---|---------|--------|---------|
| O-1 | 实施现状、接线批次、跨阶段依赖、测试基线总数 | 00 §3 | 新 `25`；00 只留导航与角色 |
| O-2 | `sched_init` 细节 | 01 §2.6 | 新 `19-pm-scheduling` |
| O-3 | `get_nice_value` 完整公式与逆函数论证 | 01 §2.5 | 新 `19-pm-scheduling` |
| O-4 | fork 的字段继承/重置清单 | 02 §2.5 | 新 `09-pm-fork` |
| O-5 | 表槽位分配/释放的使用方（fork/exit/wait 三处） | 03 §2.5 | 原语留 `03`，使用点归 `09`/`10`/`11` |
| O-6 | `calls_stats` 的实现细节 | 04 §2.8 | 计数点留 `05`，`SI_CALL_STATS` 消费归 `22`，feature 门归 `25` |
| O-7 | `handle_vfs_reply` 各路的**业务语义** | 05 §2.3/§2.4 | 路由与标志生命周期留 `07`，业务语义归 `09`/`10`/`16`/`18` |
| O-8 | `sig_send` 与 `sys_sigsend` 投递 | 12 §1.6/§2 | 新 `15-pm-signal-delivery` |
| O-9 | `unpause` 与 `stop_proc` 的机制 | 12（被 sig_send 引用） | 新 `14-pm-signal-suspend-resume` |
| O-10 | `TO_NOEXEC = 0x1`（**事实错误**，C 为 0x4） | 18 §2.14 | 按 `sys/ptrace.h:211` 修正，归新 `20-pm-ptrace` |
| O-11 | 逐测试名清单（含 14 个虚构名与 31 处漂移） | 各篇 §5 | 降为"验证锚点"；清单归 `25` |
| O-12 | 尾部 `restart_sigs` 的业务语义 | 05 §2.3 | 调用点与条件留 `07`，语义归 `14` |
| O-13 | `set_rusage_times` 的换算实现 | 10 §2.8 与 20 §2.11 重复 | 归 `11-pm-wait`，`22` 只引用 |

### 3.4 覆盖缺口表（→ 已落实为新增知识点并追加入池）

| # | 缺口 | 证据 | 建议 | 落实 |
|---|------|------|------|------|
| G-1 | 47 个调用的**入站/出站线格式**与解码纪律没有独立篇章 | `minix/ipc.h`；`ipc/decode.rs`；`edge_todo.md` E7 | 新建 `06-pm-wire-format` | K-100~K-104 |
| G-2 | 失败模型（panic 全集 / errno 全集 / DEFERRED 契约 / 错误保真）散落各篇 | 全源 `panic(`；`plan.md §4.2`；`todo.md §6` | 新建 `23-pm-failure-model` | K-520~K-526 |
| G-3 | 对端服务协议面（内核 `sys_*`、VM、RS、SCHED、MIB）只在 plan.md 有表 | `plan.md §4.1`；`minix/sched.h`；`minix/vm.h` | 新建 `24-pm-peer-protocols` | K-540~K-547 |
| G-4 | 测试基建与实施台账无处安放，导致 00 与各篇 §5 腐烂 | `tests/run_once_integration.rs`；00 §3.3 声称 381 而实测 403+11 | 新建 `25-pm-test-and-status` | K-548~K-551 |
| G-5 | 内核信号入口（SIGKSIG 拉取循环）已在代码闭环但无篇章 | `main.c:122`；`init.rs` notify 分支；`todo.md §12` 批次 H | 新建 `15-pm-signal-delivery` | K-327~K-332 |
| G-6 | `mp = &mproc[0]` 伪装信号源的技巧没有文档解释 | `signal.c:312-313/335`；`alarm.c:341` | 并入 `15` | K-331 |
| G-7 | fork 中 `PRIV_PROC` 子进程的 `mp_scheduler = SCHED_PROC_NR` | `forkexit.c:100-103` | 并入 `09` | K-167 |
| G-8 | CLOCK notify 的时间戳来源（通知载荷 vs 处理时刻） | `main.c:66-67`；V3-P2-10 | 并入 `05` | K-082 |
| G-9 | `asynsend` 容量与 `ASYN_NR` 的约束 | `libsys/asynsend.c`；`event.c:17-25` | 并入 `07`/`08` | K-130 |
| G-10 | INIT 死亡 / VFS 死亡特例的系统级反应 | `forkexit.c:336-345` | 并入 `10` | K-198 |
| G-11 | `RB_POWERDOWN` → 经 DS 查 `readclock.drv` 通知断电 | `misc.c:210-216` | 并入 `22` | K-499 |
| G-12 | `SEND_PRIORITY`/`SEND_TIME_SLICE` 的零使用事实 | `const.h:19-20`；grep 全源零使用 | 并入 `19` 标注排除 | K-007 / K-429 |
| G-13 | `sched_start_user` 的 `inherit_from` 语义（V3-P2-3 修正） | `schedule.c:66-76` | 并入 `19` | K-422 |
| G-14 | `DELAY_CALL` 在 Rust 互斥建模下的不可表示性 | `signal.rs` 注释；`todo.md §12` V3-P1-3 | 并入 `14` | K-309 |
| G-15 | 用户态启动与运行时装载（PM 如何成为进程） | `../00-master-plan/README.md` | **明确不在本 stage**，只在 `04` 声明 + 回指 | K-547 |

### 3.5 非 C 主题逐项回答（固定清单，不允许留空）

| 主题 | 在本 stage 讲什么 | 归属 |
|------|-----------------|------|
| **链接与加载** | **不在本 stage**。C 侧 `minix3/minix/servers/pm/Makefile`（15 个 SRCS，`-lsys -ltimers`）与 Rust 侧 `os/servers/pm/Cargo.toml`（`[[bin]] minix-pm` + `[lib] minix_pm`）只在 `04-pm-startup` 各列一行作为"制品形态"；ELF 装载与进程创建由 RS/VM 完成 | `../03-stage-rs/`、`../02-stage-vm/`、`../14-stage-runtime/` |
| **镜像与内存布局** | PM 是 boot image 的**消费者**（`sys_getimage` → `image[NR_BOOT_PROCS]`，负 `proc_nr` 跳过）；image 表权威在内核 | `04-pm-startup`；布局本体在 `../01-stage-kernel/` |
| **汇编入口与陷阱进入** | **不在本 stage**。PM 无汇编入口；全部内核调用经 `minix-sys` 的 `SYS_*`（trap 层，edge E1/E6）。本 stage 只在 `24` 给"能力 ↔ trait ↔ wrapper ↔ 内核对端"四列对照表 | `../01-stage-kernel/`、`../14-stage-runtime/` |
| **启动装配** | `sef_local_startup` 的三类回调注册 + `sef_cb_init_fresh` 八步 → **`04-pm-startup`**（核心篇章）；SEF 库本体只讲注册面与 signal_manager 的拦截语义 | 本 stage |
| **构建与工具链** | C `Makefile` 与 Rust `Cargo.toml` 的 `syscall_stats` / `sprofile` 两个 feature（对齐 C 的 `ENABLE_SYSCALL_STATS` / `SPROFILE`）→ `22`（语义）+ `25`（如何跑 `cargo check --features`） | 本 stage |
| **跨模块接口与线格式** | → 新建 `06-pm-wire-format`（入站/出站布局、解码单点、E7 过渡）+ 新建 `24-pm-peer-protocols`（内核/VM/VFS/RS/SCHED/MIB） | 本 stage（wire 终局挂 edge E7） |
| **错误路径** | → 新建 `23-pm-failure-model`（panic 全集、errno 全集、DEFERRED 契约、错误保真、fail-closed 选择） | 本 stage |
| **关闭与退出** | 系统关机 = `do_reboot` 定序（`22`）；PM 自身关闭 = 收到 `HARD_STOP` 通知（`sys_abort` 后等通知，`05` + `07`） | 本 stage |
| **并发与同步** | PM 是**单线程事件循环**：C 用文件级全局（`glo.h`），Rust 用 `&mut ProcTable` 的借用检查（A-3）；无锁、无原子、无 `Send`/`Sync` 要求。内核侧 SMP 不在本 stage | `03`（A-3）+ `05`（循环结构） |
| **测试基建** | → 新建 `25-pm-test-and-status`（三层测试、覆盖率 Gate A/E、接线批次台账、测试名对账纪律） | 本 stage |

---

## 4. 新目录

### 4.1 设计原则与对旧目录的三处结构性修正

1. **编号 = 阅读序**（旧目录：`99-global-concepts` 编号最后，`00` 的推荐路线却要求最先读它）。新目录把常量词汇表前移为 `01`。
2. **数据结构先于启动链**（旧目录：`01` 启动 → `02/03` 结构，但启动每一步都在操作这些结构）。新目录 `02`（结构）→ `03`（表与身份）→ `04`（启动），并在 `04` 开头写回指补偿。这是本次最重的一处重排，理由见 §4.4 序差表 X-1。
3. **信号按"状态 → 决策 → 原语 → 投递"重排**（旧目录 11 core → 12 handlers → 13 flow 存在环：`sig_send`(12) 依赖 `unpause`(13)，`restart_sigs`(13) 依赖 `sig_proc`(11)）。新序 12 模型 → 13 分派 → 14 停止/恢复 → 15 投递，**无环**。理由见 §4.4 序差表 X-3。

### 4.2 新篇章总表（26 篇）

| 编号 | 标题 | 一句话定位 | 分组 |
|------|------|-----------|------|
| `00-pm-overview` | PM 是谁 | PM 在微内核里的角色边界、四重权威、与其它服务的分工、26 篇导航与三条阅读路径 | Part 0 导航 |
| `01-pm-constants` | 常量、身份编码与全局状态 | 每个魔数的语义半径、endpoint 代际编码、信号三集合、全局七件套的 Rust 归宿 | Part 0 地基 |
| `02-pm-mproc-struct` | 进程结构 `mproc` | 一个槽位记住什么：字段族、19 个 flag 位、独立 sigaction 表，以及 Rust 的四层分层 | Part 1 进程模型 |
| `03-pm-identity-table` | 进程表与身份 | 槽位 / endpoint / PID 三层身份、容量纪律、三个索引原语与 PID 轮转契约 | Part 1 进程模型 |
| `04-pm-startup` | 启动链 | 从 `main()` 到进入主循环：SEF 回调、八步初始化、boot image 填充、VFS 握手、调度接管 | Part 2 运行时骨架 |
| `05-pm-main-loop` | 主循环与分发 | 一轮消息走完 `receive → 通知 → 校验 → 三路分发 → reply` 的全过程与回复意图模型 | Part 2 运行时骨架 |
| `06-pm-wire-format` | 消息线格式与解码 | 47 个调用的入站/出站字节布局、union 的 unsafe 单点解码、E7 过渡纪律 | Part 2 运行时骨架 |
| `07-pm-vfs-protocol` | 与 VFS 的异步协议 | 请求为何异步、`tell_vfs`、11 路回复的**路由表**、`VFS_CALL`/`NEW_PARENT`/`UNPAUSED` 的生命周期 | Part 2 运行时骨架 |
| `08-pm-event-subscription` | 进程事件订阅与发布 | 订阅表 + 游标 + 串行投递的完整状态机，以及为什么必须是串行 | Part 2 运行时骨架 |
| `09-pm-fork` | fork 与 srv_fork | 一次 fork 如何跨越 PM/VM/VFS 三张表，以及 RS 专用 fork 的差异 | Part 3 生命周期 |
| `10-pm-exit` | 退出 | 两阶段退出、僵尸两级、孤儿收养、core dump 与系统进程直毁 | Part 3 生命周期 |
| `11-pm-wait` | 等待与回收 | `wait4` 的三环扫描、tracer 伪父、rusage 跨地址空间拷贝与 `TOLD_PARENT` | Part 3 生命周期 |
| `12-pm-signal-model` | 信号模型与处理器安装 | 七张位图、`sigaction` 三态、掩码四种操作、KILL/STOP 三重封堵 | Part 4 信号 |
| `13-pm-signal-dispatch` | 信号的生成与分派 | `kill` 到 `sig_proc` 九判定链：谁能发给谁、信号最终落入哪种处置 | Part 4 信号 |
| `14-pm-signal-suspend-resume` | 停止、延迟与恢复 | `stop_proc` / `DELAY_CALL` / `SIGSNDELAY` / `unpause` / `check_pending` / `restart_sigs` | Part 4 信号 |
| `15-pm-signal-delivery` | 信号投递与内核回环 | `sig_send` 如何把位图变成栈帧，以及内核积累的信号如何经 SIGKSIG 回到 PM | Part 4 信号 |
| `16-pm-exec` | exec | 权限在 VFS、凭证在 PM：转发的 exec、`PARTIAL_EXEC` 哨兵与重启路径 | Part 5 程序替换 |
| `17-pm-itimer` | 定时器 | 三族定时器、REAL 与 VIRTUAL/PROF 两套后端、`ticks↔timeval` 的取整与钳位 | Part 6 场景分组 |
| `18-pm-credentials` | 凭证 | 三元组 id、BSD 全置语义、补充组、会话，以及为何每次成功都要通知 VFS | Part 6 场景分组 |
| `19-pm-scheduling` | 调度 | nice 与优先级队列的双射、用户态调度器的接管与让渡 | Part 6 场景分组 |
| `20-pm-ptrace` | 调试 | `T_*` 全族、两套守卫（root 专属与通用）、`trace_stop` 与 tracer 的停止通知 | Part 6 场景分组 |
| `21-pm-time` | 时间 | 双时钟、`boottime + clock/hz` 的分解、读与写的权限不对称 | Part 7 查询与杂项 |
| `22-pm-misc-queries` | 杂项查询与控制 | uname / sysinfo / procnr / epinfo / reboot / svrctl / rusage / sprofile / mcontext | Part 7 查询与杂项 |
| `23-pm-failure-model` | 失败模型与不变量 | 什么情况 panic、什么情况返回 errno、什么情况登记 DEFERRED，以及错误保真规约 | Part 8 横切 |
| `24-pm-peer-protocols` | 对端服务协议面 | PM 与内核 / VM / VFS / RS / SCHED / MIB 的全部接触点与四列对照表 | Part 8 横切 |
| `25-pm-test-and-status` | 测试基建与实施台账 | 三层测试怎么跑、覆盖率 Gate、47 调用接线批次、测试名对账纪律 | Part 8 支线（可跳读） |

### 4.3 阅读路径

- **主线（必读，00 → 22 顺序）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17~20 → 21 → 22。
- **支线（理解 PM 的工程现实）**：23（失败模型，读完 05 即可随时插入）、24（对端协议，读完 07 即可随时插入）。
- **可跳读**：25（实施台账与测试，只在需要跑测试或对账时读）。
- **并行体（Part 6：17/18/19/20）**：四篇彼此独立；主线建议 17 → 18 → 19 → 20（定时器与凭证是另两篇的前置概念）。
- **按角色的最短路径**：
  - 只想理解 fork：00 → 01 → 02 → 03 → 05 → 07 → 09。
  - 只想理解信号：00 → 01 → 02（信号位图部分）→ 05 → 12 → 13 → 14 → 15。
  - 只想做接线/测试：00 → 05 → 06 → 25。

### 4.4 序差表（教学序 ≠ 运行时序的地方）

| # | 运行时序事实（带锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---------------------|-----------|------|-------------|
| X-1 | `sef_cb_init_fresh` 第 1 步就初始化 `mproc` 表（`main.c:147-152`），第 4 步就分配 pid（`main.c:209` 调 `get_free_pid`） | 先讲结构（02）与身份（03），再讲启动（04） | 启动每一步都是"往结构里写什么、按什么身份分配"；先认识被操作对象才读得懂步骤 | `04-pm-startup` 开头声明："本篇假设读者已认识 `mproc` 字段（02）与槽位/endpoint/PID（03），此处只讲启动做了什么、为何是这个顺序" |
| X-2 | 常量在 C 里没有独立位置，散落在 `const.h`/`mproc.h`/`signal.h`/`com.h`，且三集合是**运行时构建**（`main.c:154-165`） | 前置为 `01-pm-constants` | 常量是全部机制篇的引用底层；不前置则每篇都要重复定义 | `01` 内明确"三集合虽在运行时构建，但取值是编译期可枚举的封闭集合，Rust 以编译期位图等价替代" |
| X-3 | `sig_send`（旧 12）与 `unpause`/`stop_proc`（旧 13）互相调用；`restart_sigs`（旧 13）依赖 `sig_proc`（旧 11） | 12 模型 → 13 分派 → 14 停止/恢复原语 → 15 投递 | 旧序存在环；新序先给状态与决策、再给原语、最后给真正的投递，环被拆掉 | `15` 开头声明："本篇的 `sig_send` 依赖 14 的停止/恢复原语；13 的 `sig_proc` 到达捕获分支时把控制权交给本篇" |
| X-4 | fork 中 tracer 存在即调 `sig_proc(SIGSTOP)`（`forkexit.c:133-134`）；exit 中调 `check_sig(-procgrp, SIGHUP)` 与 `sig_proc(SIGCHLD)`（`forkexit.c:412`、`:663`） | 生命周期（09/10/11）在信号（12~15）之前 | 生命周期里的信号调用是**支线触发点**（参数固定、语义单一）；信号里的"终止"支线才是核心路径，放在后面才能引用已讲过的 exit | `09 §2.7` / `10 §2.4` 对这三处标注："信号的分派与处置机制见 13/15，此处只讲触发条件与参数" |
| X-5 | `handle_vfs_reply` 的 FORK/EXIT/EXEC 分支会调 `sched_start_user` / `exit_restart` / `exec_restart`（分属 19/10/16） | VFS 协议（07）在生命周期与 exec 之前 | `handle_vfs_reply` 是"异步回复的路由器"，其业务语义属于各服务；07 只讲路由与标志生命周期即可自洽 | `07` 的"11 路回复路由表"每行标注"业务语义归哪一篇"；不展开分支体 |
| X-6 | `resume_event` 最后一步调 `exit_restart` / `restart_sigs`（`event.c:119-122`） | 事件订阅（08）在生命周期之前 | 同 X-5：08 讲"事件如何逐个投递、投递完后把控制权交给谁"，不是退出/信号的业务 | `08` 的终止分派表标注归属 |
| X-7 | 运行时 fork 之后通常立刻 exec（09 → 16），但教学序把 exec 排到信号之后（16） | exec 排在 15 之后 | `exec_restart` 的语义核心之一是 tracer 的 `SIGTRAP`/`SIGSTOP`（`exec.c:189-194`），必须引用 13/14 | `09` 结尾过渡声明："exec 的替换语义见 16，排在信号之后是因为它的 tracer 信号语义依赖 13/14" |
| X-8 | `sched_init` 是启动链第 8 步（`main.c:241`），但调度篇排在第 19 位 | 04 只讲调用点，语义在 19 | 启动链只需知道"这一步存在、为何在最后" | `04` 第 8 步标注"语义见 19" |
| X-9 | `expire_timers` 由主循环 CLOCK 分支驱动（`main.c:66-67`），但定时器篇在 17 | 05 只讲"时钟通知触发定时器到期"，17 讲到期后的因果 | 主循环只需知道"时钟通知做了一件事" | `05` 的 notify 步骤标注"到期处理见 17" |

---

## 5. 每篇契约

> 每篇契约七要素：定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 + 验收标准。

### 00-pm-overview

- **一句话定位**：回答"PM 是什么、它在微内核里为什么存在、26 篇怎么读"。
- **讲什么**：微内核的进程语义下沉；PM 的四重权威（进程表唯一所有者 / 信号管理器 / 生命周期编排者 / 记账方）；PM 与 kernel / VM / VFS / RS / SCHED 的分工一句话各一条；26 篇导航表；三条阅读路径与三种角色的最短路径。
- **不讲什么**：任何机制细节（→ 各篇）；主循环骨架（→ 05）；实施现状、接线批次、测试基线总数、跨阶段依赖（→ 25，**本次最硬的一条越界纠正**）；常量定义（→ 01）。
- **前置**：无（`../01-stage-kernel/00-kernel-overview.md` 与 `../00-master-plan/README.md` 作为外部背景，不计入本目录前置）。
- **后置**：01（导航指向）、04（"PM 由 RS 加载"的背景）、24（分工表的展开版）。
- **事实底线**：`main.c:49`（入口）、`main.c:59`（循环）、`main.c:118/119/122`（三类 SEF 回调）、`table.c:14-61`（47 调用）；`os/servers/pm/Cargo.toml`（bin + lib 双目标）；`../00-master-plan/README.md`（启动因果链）。
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|------------|------|
  | K-020 | PM 在启动因果链的位置 | 概念 | `main.c:49` | 本篇是唯一回答"PM 之前/之后是谁"的地方 | 存量：01 §1.1、00 §1.1 |

- **验收标准**：读者读完能回答 5 个问题——(1) 内核不做哪四件事而交给 PM？(2) PM 的进程表为什么是"权威"而不是"副本"？(3) fork 一次要跨几个服务？(4) 想懂 fork 最短要读哪几篇？(5) 本篇为什么不再给测试总数？导航表 26 行全部可点达。

### 01-pm-constants

- **一句话定位**：给全部机制篇提供"魔数的权威定义 + 语义半径"，并说明 C 的全局状态在 Rust 里去了哪。
- **讲什么**：K-001~K-016。重点三件事：常量的**语义半径**（它约束哪些行为）、endpoint 的代际编码算术与防 ABA、全局七件套的 Rust 归宿（A-3）。
- **不讲什么**：信号处置的三态语义（→ 12）；进程槽位分配算法（→ 03）；`mproc` 字段（→ 02）。
- **前置**：00。
- **后置**：02、03、05、12、13、17、21（全部机制篇引用）。
- **事实底线**：`const.h:3-20`；`minix/type.h:145`；`sys/signal.h:45/52-83/264/274`；`minix/config.h:31`；`com.h:70/72`；`mproc.h:106`；`glo.h:8-30`；`main.c:137-165`；Rust `mproc/constants.rs`、`init.rs::sig_bit`、`minix-types::Endpoint`。
- **知识点清单**：K-001 ~ K-016（16 条，见 §2.1）。
- **验收标准**：随机抽 10 个常量，每个都能回答"值 / C 锚点 / 约束了什么行为 / Rust 里叫什么"；`NO_TRACER` 与 `NO_EVENTSUB` 必须各写出"它依赖了什么隐式事实、Rust 如何让这个依赖消失"。

### 02-pm-mproc-struct

- **一句话定位**：一个槽位记住什么——字段族、19 个状态位，以及 Rust 为什么把它拆成四层。
- **讲什么**：K-040~K-048。字段按族讲完（身份 / 信号状态 / 记账 / 定时器 / 调度 / 执行帧 / 回复消息），19 个 flag 位逐个讲"谁置、谁清、意味着什么"，`mpsigact` 独立表的体积动机，A-1 与 A-2 的逐位映射。
- **不讲什么**：fork 的继承/重置清单（→ 09）；位图在信号投递中的变化（→ 12/15）；表级操作（→ 03）。
- **前置**：00、01。
- **后置**：03、04、09、10、12、14、17、20。
- **事实底线**：`mproc.h:16-22`（mpsigact）、`mproc.h:24-83`（字段）、`mproc.h:86-106`（19 位 + MP_MAGIC）；Rust `mproc/mproc.rs`（分层 + 映射表）、`mproc/{lifecycle,block,wait}.rs`。
- **知识点清单**：K-040 ~ K-049（10 条，其中 K-049 为越界项，本篇只写"继承清单见 09"一句）。
- **验收标准**：给出"19 个 C 位 → Rust 类型表达"的完整对照表，逐位标注主讲述点或归属篇；`mpsigact` 的隔离必须给出"约 80% 体积"的来源与 MIB 规避的因果。

### 03-pm-identity-table

- **一句话定位**：三层身份（槽位 / endpoint / PID）如何被索引、分配与释放。
- **讲什么**：K-060~K-068。`procs_in_use` 与 `LAST_FEW` 的容量纪律；`next_child` 轮转与双 panic 守卫；`pm_isokendpt` 的三层 errno 分工；`find_proc`；`get_free_pid` 的双字段扫描与**先自增相位**（外部可观察契约）；`cleanup`/`release_slot` 与"不 bump generation"的裁决；A-3 的 `ProcTable`。
- **不讲什么**：fork/exit/wait 里这些原语的使用点业务（→ 09/10/11）；endpoint 编码算术（→ 01）。
- **前置**：00、01、02。
- **后置**：04、05、09、10、11、18、20、22。
- **事实底线**：`glo.h:8-9`；`utility.c:34-51`、`:76-85`、`:108-118`；`forkexit.c:32/51/68-75/86/795-806`；Rust `mproc/table.rs`、`mproc/pid_gen.rs`、`mproc/context.rs`。
- **知识点清单**：K-060 ~ K-068（9 条）。
- **验收标准**：读者能回答"pid 为什么从 3 开始"、"伪造一个陈旧 endpoint 会在第几层被拒、返回什么 errno"、"表满时 root 与普通用户的差别"；`get_free_pid` 必须给出一个"进程组号也占位"的具体反例。

### 04-pm-startup

- **一句话定位**：从 `main()` 到进入主循环，PM 如何建成"第一代进程世界"。
- **讲什么**：K-020~K-030（K-031/K-032 只写调用点与一句归属）。八步顺序依赖是骨架：为何先初始化表、再构建信号集合、再取启动参数、再取 image、再填充、再同步 VFS、再取 hz、最后接管调度。INIT 与系统进程两条填充分支的差异。`VFS_PM_INIT` 的 `sendrec` 屏障语义。
- **不讲什么**：`sched_init` 的实现（→ 19）；`get_nice_value` 公式（→ 19）；主循环（→ 05）；`mproc` 字段语义（→ 02，开头回指）；槽位/PID 分配算法（→ 03，开头回指）。
- **前置**：00、01、02、03。
- **后置**：05、07、09、19。
- **事实底线**：`main.c:48-56`、`:114-126`、`:131-244`、`:188-215`、`:221-236`、`:238`、`:241`；`glo.h:16-30`；Rust `init.rs`（`init()`、`fill_boot_procs()`、`vfs_init_sync()`）、`main.rs:16`。
- **知识点清单**：K-020 ~ K-033（14 条）。
- **验收标准**：给出"八步 × 每步依赖前一步的什么"的表格（依赖列不能为空）；必须解释"为什么 INIT 是自己的父亲"与"为什么最后一条 `VFS_PM_INIT` 用 `sendrec` 而不是 `send`"。

### 05-pm-main-loop

- **一句话定位**：一轮消息走完 `receive → 通知 → 校验 → 三路分发 → reply`，以及"不回复"的三种意图。
- **讲什么**：K-080~K-092。含：通知分支（CLOCK → 到期，时间戳取**通知载荷**）、SYSTEM 通知的拉取循环（只讲触发点与归属，语义归 15）、`pm_isokendpt` 调用点与 panic、EXITING 丢弃、三路分发的判别式、`call_vec → PmCall` 的 A-5、`SUSPEND → ReplyIntent` 的 A-6（**本目录唯一主讲述点**）、`reply` 与预填载荷、`ENOSYS` 兜底、`run_once`/`run` 拆分。
- **不讲什么**：`handle_vfs_reply` 内容（→ 07）；`do_proc_event_reply`（→ 08）；具体 handler（→ 09~22）；`calls_stats` 的消费者（→ 22）。
- **前置**：00、01、02、03。
- **后置**：06、07、08、09~22（全部）。
- **事实底线**：`main.c:59-107`、`:65-71`、`:84-103`、`:106`、`:249-270`、`:34-36/95-97`；`callnr.h:9/11`；`table.c:14-61`；Rust `init.rs::run_once`、`ipc/dispatcher.rs`（`ReplyIntent`）、`ipc/calls.rs`（47 臂 + `:991` 兜底）。
- **知识点清单**：K-080 ~ K-092（13 条）。
- **验收标准**：给出"一轮消息的 11 步判定表"，每步标注 C 行号；`ReplyIntent` 必须列出它的**三类回复路径**分别对应 C 的哪一处 `reply()` 调用（主循环尾 / `handle_vfs_reply` 内 / `tell_parent` 内）。

### 06-pm-wire-format（新建）

- **一句话定位**：47 个调用的字节布局权威在哪，以及 union 读取的 unsafe 面如何收敛到一个文件。
- **讲什么**：K-100~K-104。`message` 的 `m7_i*`/`m7_p*`/`m7_l*` 分区与 `m_type` 判别；入站载荷结构族；出站回复臂与"tag(`m_type`) + typed body(载荷)"契约；`ipc/decode.rs` 的每调用一函数 + `size_of` 布局断言；E7 终局与本 stage 的过渡纪律。
- **不讲什么**：每个调用的业务语义（→ 09~22）；内核侧 `sys_*` 的 trap 机制（→ `../01-stage-kernel/`、24）；wire 类型本身的重新设计（→ edge E7）。
- **前置**：00、01、05。
- **后置**：07、09~22（各篇只引用本篇的布局编号，不重复贴结构）。
- **事实底线**：`minix/ipc.h`（`mess_lc_pm_exit` :446-450、`mess_lc_pm_wait4` :585-591、`mess_lsys_pm_srv_fork` :1422-1427、`m_pm_lsys_getepinfo` :517-524 等）；`os/servers/pm/src/ipc/decode.rs:1-60`（含 7 条 `const _: () = assert!(size_of::<T>() <= MESSAGE_PAYLOAD_SIZE)`）；`os/libs/minix-types/src/ipc/{pm,vfs,event,message}.rs`。
- **知识点清单**：K-100 ~ K-104（5 条，全部为**新增**）。
- **验收标准**：给出"47 调用 × 入站结构 × 出站臂"的索引表（不贴结构定义，只给锚点）；必须写出"为什么 decode 必须是单点"的理由，并把 7 条 `size_of` 断言逐一列出。

### 07-pm-vfs-protocol

- **一句话定位**：PM 与 VFS 之间的异步请求/回复协议——为什么异步、在途状态怎么记、11 路回复如何路由。
- **讲什么**：K-120~K-130。异步的必然性（`asynsend3` + `AMF_NOREPLY` + `VFS_CALL`）；`tell_vfs` 的"非空闲即 panic"；RQ/RS 两个 base 与判别式；**11 路回复路由表**（每行只写"交给谁 + 业务语义归哪篇"）；`NEW_PARENT` 与 `UNPAUSED` 的完整生命周期（本目录唯一主讲述点）；`VFS_PM_REBOOT_REPLY` → `sys_abort` 特例；尾部 `restart_sigs` 的**调用点与条件**（语义归 14）；`asynsend` 容量约束。
- **不讲什么**：FORK/EXIT/EXEC/SET* 各路回复的**业务语义**（→ 09/10/16/18）；`restart_sigs` 内部（→ 14）；订阅者回复（→ 08）。
- **前置**：00、01、02、03、05。
- **后置**：08、09、10、16、18、24。
- **事实底线**：`utility.c:123-139`（tell_vfs）；`main.c:294-424`（handle_vfs_reply，11 路在 `:335-419`）；`com.h:513-544`；`libsys/asynsend.c`；Rust `ipc/vfs.rs`。
- **知识点清单**：K-120 ~ K-130（11 条）。
- **验收标准**：11 路路由表每行有"m_type / 触发它的请求 / 控制权交给谁 / 业务语义在哪篇"四列；`NEW_PARENT` 与 `UNPAUSED` 必须各画出"置位 → 消费 → 清除"的三点链并带行号。

### 08-pm-event-subscription

- **一句话定位**：进程退出与捕获信号两件事如何被串行地广播给订阅者。
- **讲什么**：K-140~K-149。两种事件；`subs[NR_SUBS=4]` 三元组；`mp_eventsub` 游标与 `NO_EVENTSUB`；**串行化的容量论证**（`NR_PROCS` vs `NR_PROCS×NR_SUBS` vs 无界异步）；`resume_event` 的推进与终止分派（只写"交给谁"）；`remove_sub` 的有序删除、游标回退与 `nested` 守卫；`do_proceventmask` 三态；`do_proc_event_reply` 的六重校验与五种 SUSPEND；`publish_event` 与订阅者死亡清理；不按进程过滤的竞态约束。
- **不讲什么**：`exit_restart` 与 `restart_sigs` 的内部（→ 10 / 14）；`asynsend` 的通用机制（→ 07）。
- **前置**：00、01、02、05、07。
- **后置**：10、14、24。
- **事实底线**：`event.c:17-25`、`:58-67`、`:74-123`、`:130-161`、`:170-211`、`:218-309`、`:316-353`；`syslib.h:292-293`；`const.h:13`；Rust `event.rs`、`mproc/block.rs`。
- **知识点清单**：K-140 ~ K-149（10 条）+ K-130（引用）。
- **验收标准**：画出"一次事件发布 → N 个订阅者 → 终止分派"的时序图；必须解释"为什么退订要等 `waiting==0`"与"为什么 `remove_sub` 要回退其它进程的游标"，各带行号。

### 09-pm-fork

- **一句话定位**：一次 fork 如何跨 PM / VM / VFS 三张表完成，以及 RS 专用 fork 差在哪。
- **讲什么**：K-160~K-178。容量预检 → 槽位轮转 → `vm_fork` 同步段（"此后不可失败"窗口）→ 全量复制与字段重整（**继承/重置清单的主讲述点**）→ `TO_TRACEFORK` 条件继承 → `PRIV_PROC` 子进程的调度器改写 → `get_free_pid` → `VFS_PM_FORK` → tracer SIGSTOP → `SUSPEND`；`VFS_PM_FORK_REPLY` 的双回复与失败拆解；`do_srv_fork` 的 RS 门、uid/gid 注入、flags 掩码差异、立即回复。
- **不讲什么**：`vm_fork` 对端实现（→ `../02-stage-vm/`）；VFS 侧 fproc 建立（→ `../05-stage-vfs/`）；`sig_proc` 的判定链（→ 13，只写触发点与参数）；`get_free_pid` 算法（→ 03）。
- **前置**：00、01、02、03、05、07。
- **后置**：10、11、16、19。
- **事实底线**：`forkexit.c:32/45-140`（do_fork）、`:145-240`（do_srv_fork）；`main.c:369-396`；`sys/ptrace.h:209`；Rust `fork.rs`、`mproc/fork.rs`。
- **知识点清单**：K-160 ~ K-178（19 条）。
- **验收标准**：给出"fork 的同步段 / 异步段"分界图，标出"不可失败窗口"的起止行号；`do_srv_fork` 与 `do_fork` 必须给出一张**差异表**（至少 6 行：调用者门 / flags 掩码 / uid-gid 来源 / 调度器 / 回复协议 / VFS 消息），而不是重复讲一遍流程。

### 10-pm-exit

- **一句话定位**：退出为什么是两阶段，以及僵尸、孤儿与 core dump 如何被安置。
- **讲什么**：K-190~K-208。`PRIV_PROC` 走 `sys_kill`；`dump_core` 双抑制；会话领导者记忆；`ALARM_ON` 取消；`sys_times` 记账（**主讲述点**）；强制 `sys_stop` 与延迟调用风险；`vm_willexit`；INIT/VFS 死亡特例；`VFS_PM_EXIT`/`VFS_PM_DUMPCORE`；`PRIV_PROC` 直毁；`EXITING` 保留位；`zombify` 两级僵尸；`check_parent` 两分支；`disinherit` + `NEW_PARENT`；SIGHUP 广播；`exit_restart` 六步；`tracer_died`；`cleanup`。
- **不讲什么**：`check_sig` 的判定链（→ 13，只写"广播 SIGHUP/SIGCHLD 的触发点与参数"）；`publish_event` 的机制（→ 08，只写调用点）；`tell_parent` 的细节（→ 11）。
- **前置**：00、01、02、03、05、07、08、09。
- **后置**：11、13、14、19、20。
- **事实底线**：`forkexit.c:245-262`、`:267-413`、`:418-469`、`:593-624`、`:629-665`、`:759-790`、`:795-806`；Rust `exit.rs`、`mproc/lifecycle.rs`、`mproc/guardianship.rs`。
- **知识点清单**：K-190 ~ K-208（19 条）。
- **验收标准**：画出"exit 两阶段"时序图并标出 VFS 回复与事件订阅在其中的位置；必须解释"为什么系统进程要被直毁"与"为什么 CPU 时间先记在死者身上"，各带行号。

### 11-pm-wait

- **一句话定位**：`wait4` 如何把僵尸消费掉，以及 tracer 作为伪父的回收顺序。
- **讲什么**：K-220~K-231。`pidarg` 四态归一化；三条件过滤与 `children` 计数；三环扫描；`W_STOPCODE` 与 `mp_sigtrace`；`WNOHANG` / `ECHILD`；`WAITING` 三件套；`wait_test`；`tell_parent` 六步（含 `sys_datacopy` 失败时子进程仍是僵尸）；`tell_tracer` 的伪父转换；`set_rusage_times`；POSIX 的累加约束。
- **不讲什么**：僵尸如何产生（→ 10）；`trace_stop` 如何置 `TRACE_STOPPED`（→ 20）；`find_proc`（→ 03）。
- **前置**：00、01、02、03、05、07、09、10。
- **后置**：14、20、22。
- **事实底线**：`forkexit.c:474-564`、`:569-588`、`:670-726`、`:731-754`、`:795-806`；`utility.c:144-156`；Rust `wait.rs`、`mproc/wait.rs`。
- **知识点清单**：K-220 ~ K-231（12 条）。
- **验收标准**：给出"三环 × 命中条件 × 返回值 × 回复时机"的矩阵；必须解释"`tell_parent` 返回 FALSE 时子进程处于什么状态"，带行号。

### 12-pm-signal-model

- **一句话定位**：信号的七张位图与"安装/掩码"四个系统调用，不含投递。
- **讲什么**：K-250~K-261，外加对 K-013 的引用。七张位图各自的读写者；`sigaction` 三态与位图联动；`mp_sigreturn`；KILL/STOP 三重封堵（列出全部封堵点）；`sigprocmask` 四种 `how` 与"改掩码后必查挂起"；`sigpending`；`sigsuspend` 的原子等待；`sigreturn` 的恢复；`sa_mask`/`SA_NODEFER`/`SA_RESETHAND` 的**声明**（投递时效应归 15）；入口断言；位基单点。
- **不讲什么**：`sig_send` 与 `sys_sigsend`（→ 15）；`unpause` / `stop_proc`（→ 14）；`check_sig` / `sig_proc`（→ 13）。
- **前置**：00、01、02、05。
- **后置**：13、14、15、16、20。
- **事实底线**：`signal.c:40-86`、`:91-97`、`:102-155`、`:160-171`、`:176-192`；`mproc.h:53-59`；`main.c:137-165`；Rust `signal_handlers.rs`、`mproc/signal.rs`、`init.rs::sig_bit`。
- **知识点清单**：K-250 ~ K-261（12 条）+ K-013（引用）。
- **验收标准**：给出"七张位图 × 谁写 × 谁读 × 在哪个系统调用被改"的矩阵；KILL/STOP 的三重封堵必须列出**全部 5 处**代码点（三处 `sigdelset(set, KILL/STOP)` + 两处 `sigdelset(&mp_sigmask, ...)`），带行号。

### 13-pm-signal-dispatch

- **一句话定位**：从 `kill` 到 `sig_proc` 的九判定链：谁能发给谁，信号最终落入哪种处置。
- **讲什么**：K-275~K-286。`do_kill` / `do_srv_kill` 与 `ksig` 语义；`check_sig` 四种 pid 语义与倒序扫表；INIT 保护；SIGTERM 先通知 RS；SIGKILL 跳过 PRIV_PROC；完全跳过 VM；非 ksig 致命信号到 PRIV_PROC → EPERM；权限四重比较；返回值语义（含"调用者自毁 → SUSPEND"）；`sig_proc` 九判定链（逐条讲判定，处置细节分别回指 14/15/10）；`sig_proc_exit` 与 `core_sset`/`WCOREFLAG`；tracer 分支。
- **不讲什么**：`unpause` / `stop_proc` 的实现（→ 14，本篇只写"调用 `stop_proc(rmp, FALSE)`"及其条件）；`sig_send` 的栈帧（→ 15）；`exit_proc`（→ 10）。
- **前置**：00、01、02、05、12。
- **后置**：14、15、16、20、22。
- **事实底线**：`signal.c:197-221`、`:383-540`、`:545-563`、`:568-646`；Rust `signal.rs`。
- **知识点清单**：K-275 ~ K-286（12 条）。
- **验收标准**：给出"九判定链"的顺序表（每步：判据 / 命中后做什么 / 归属篇）；必须解释"为什么从表尾往前扫"与"为什么 VM 被完全跳过"，各带行号。

### 14-pm-signal-suspend-resume

- **一句话定位**：为了让信号能被安全地投递，PM 需要先把进程停下来——以及停不下来时怎么办。
- **讲什么**：K-300~K-309。`sys_delay_stop` 与 `EBUSY → DELAY_CALL`；`PROC_STOPPED` 的双用途；`stop_proc(may_delay)` 与 panic 条件；`SIGSNDELAY` 与延迟调用的恢复；`try_resume_proc` 的三类不恢复条件；`unpause` 的三路径（本目录唯一主讲述点）；`VFS_PM_UNPAUSE`；`check_pending` 的 break 条件；`restart_sigs` 的 TRACE_EXIT 优先；`DELAY_CALL` 在 Rust 下的不可表示性。
- **不讲什么**：`sig_send` 与 `sys_sigsend`（→ 15）；`check_sig` 的广播（→ 13）；`handle_vfs_reply` 的路由（→ 07，只写调用点）。
- **前置**：00、01、02、05、07、12、13。
- **后置**：15、16、20。
- **事实底线**：`signal.c:226-289`、`:344-369`、`:651-682`、`:687-714`、`:719-770`；`main.c:421-423`；Rust `signal_flow.rs`、`signal.rs` 的 stop/resume 桥接。
- **知识点清单**：K-300 ~ K-309（10 条）。
- **验收标准**：画出"`stop_proc` 的三种返回值 → 三条后续路径"的判定图；必须解释"`PROC_STOPPED` 为什么能同时表示'已停止'和'需重检'"，并给出 `restart_sigs` 依赖这一点的行号。

### 15-pm-signal-delivery（新建）

- **一句话定位**：把位图变成用户态栈帧（`sig_send`），以及把内核积累的信号取回 PM（`process_ksig`）。
- **讲什么**：K-320~K-332。`sigmsg` 四字段组装；`SIGSUSPENDED` 时 `sm_mask` 与 `mp_sigmask` 的**不同起底**；`sa_mask` 并入；`SA_NODEFER`/`SA_RESETHAND` 的投递时效应；`sys_sigsend` 与 EFAULT/ENOMEM 的合法失败分档（其余 panic）；`WAITING|SIGSUSPENDED` 时清位 + `reply(EINTR)` + 恢复；非 PM 睡时的 `assert(UNPAUSED)`；**注册点** `sef_setcb_signal_manager`；SIGKSIG/SIGKSIGSM 拦截与 `getksig`/`endksig` 拉取循环；`process_ksig` 的 `EDEADEPT` 双检；signo → pid 语义的翻译表；`mp = &mproc[0]` 伪装信号源与还原；内核 sigframe 的归属回指。
- **不讲什么**：信号处置的判定（→ 13）；停止原语（→ 14）；内核 sigframe 构造（→ `../01-stage-kernel/`）；`check_vtimer` 内部（→ 17）。
- **前置**：00、01、02、05、12、13、14。
- **后置**：16、17、20。
- **事实底线**：`signal.c:294-378`、`:775-855`；`main.c:122`；`minix/type.h:71-77`；Rust `signal.rs::process_sigmgr_signals`、`signal_handlers.rs::sig_send`、`init.rs` notify 分支、`minix-sys` 三个 wrapper。
- **知识点清单**：K-320 ~ K-332（13 条，其中 K-327/K-328/K-331 为**新增**）。
- **验收标准**：给出"`sig_send` 的 8 步组装顺序"表并标出"第几步可能失败、失败后进程会怎样"；必须解释"`sm_mask` 与 `mp_sigmask` 在 `SIGSUSPENDED` 时为何取不同值"，带行号。

### 16-pm-exec

- **一句话定位**：权限在 VFS、凭证在 PM 的一次程序替换，以及失败时必须杀掉自己的 `PARTIAL_EXEC` 哨兵。
- **讲什么**：K-345~K-358。`do_exec` 转发；`do_newexec` 的调用者门与 `exec_info` 拷入；`allow_setuid` 与 tracer 约束；`svuid/svgid` 无条件更新；`TAINTED` 二重判定；`mp_name` 与 frame 保存；`PARTIAL_EXEC` 哨兵；`exec_restart` 的失败分支；`catch` 重置而 `ignore` 保留；tracer 的 `SIGTRAP`/`SIGSTOP` 且必须在 `sys_exec` 之前；`sys_exec`；`do_execrestart`；`ESCRIPT` 死代码。
- **不讲什么**：VFS 侧装载（→ `../05-stage-vfs/`）；`check_sig` 的判定链（→ 13，只写触发点与参数）；`TAINTED` 的读侧 `issetugid`（→ 18）。
- **前置**：00、01、02、05、07、12、13、14、15。
- **后置**：18、20。
- **事实底线**：`exec.c:31`、`:38-56`、`:62-125`、`:130-151`、`:156-199`；`sys/ptrace.h:210-211`；Rust `exec.rs`。
- **知识点清单**：K-345 ~ K-358（14 条）。
- **验收标准**：画出"exec 的三方握手时序图"（PM → VFS → PM → 内核）并标出 `PARTIAL_EXEC` 的置位与清除点；必须解释"`catch` 重置而 `ignore` 保留"的 POSIX 依据与行号。

### 17-pm-itimer

- **一句话定位**：三族间隔定时器如何经两套后端（本地 `set_timer` / 内核 `sys_vtimer`）驱动，以及 `ticks ↔ timeval` 的取整与钳位。
- **讲什么**：K-370~K-380。三族分野；后端差异；`ticks_from_timeval` 的向上取整与 `LONG_MAX` 钳位；`timeval_from_ticks`；`is_sane_timeval` + `MAX_SECS`；`setval`/`getval` 双指针语义；`getset_vtimer` 的 `oldticks<=0 → interval` 回绕；`get_realtimer` 的 remaining；`cause_sigalrm` 的周期重挂与 `check_sig(SIGALRM)`；CLOCK notify 驱动。
- **不讲什么**：`check_sig` 的判定链（→ 13）；CLOCK 通知的分派（→ 05）；内核 `sys_vtimer` 实现（→ `../01-stage-kernel/`）。
- **前置**：00、01、02、05、13。
- **后置**：无（叶子篇）。
- **事实底线**：`alarm.c:19`、`:33-65`、`:70-76`、`:81-87`、`:92-154`、`:159-216`、`:221-241`、`:246-294`、`:297-311`、`:313-344`；`const.h:15/17`；Rust `timer.rs`、`mproc/mproc.rs`。
- **知识点清单**：K-370 ~ K-380（11 条）。
- **验收标准**：给出"三族 × 后端 × 计时口径 × 到期信号"的对照表；必须解释"`ticks_from_timeval` 为什么必须向上取整、以及溢出时为什么钳到 `LONG_MAX` 而不是报错"，带行号。

### 18-pm-credentials

- **一句话定位**：三组 id、补充组与会话的读写语义，以及为什么每次成功都要通知 VFS。
- **讲什么**：K-395~K-404。三元组；`setuid` 的 BSD 全置语义与权限；`seteuid`/`setgid`/`setegid`；`setgroups` 的 root 门、`NGROUPS_MAX`、`GID_MAX`、尾零填充；`getgroups` 二阶段；`setsid` 的会话首判定；`getsid`/`getpid`/`getpgrp`；`issetugid = TAINTED`；VFS 转发四请求与 `SUSPEND`；`VFS_PM_SET*_REPLY` 的两种回复。
- **不讲什么**：`TAINTED` 的**置位**（→ 16，本篇只讲读侧）；`find_proc`（→ 03）；VFS 侧 fproc 更新（→ `../05-stage-vfs/`）。
- **前置**：00、01、02、03、05、07。
- **后置**：16、20、22。
- **事实底线**：`getset.c:18-89`、`:94-223`；`main.c:335-347`；Rust `credentials.rs`、`mproc/credentials.rs`。
- **知识点清单**：K-395 ~ K-404（10 条）。
- **验收标准**：给出"13 个调用 × 权限判据 × 是否转发 VFS × 回复载荷"的矩阵；必须解释"`setuid` 为什么把三个 id 一起改"与"`getgroups` 为什么要求 `ngroups >= 实际`"，带行号。

### 19-pm-scheduling

- **一句话定位**：PM 不实现调度算法，只负责把进程交给哪个调度器，以及 nice 与优先级队列的双射。
- **讲什么**：K-420~K-429。用户态调度交接的分工；`sched_init` 的两个 assert 与实际只处理 INIT 的事实；`sched_start_user` 的 `inherit_from`（PRIV_PROC 父 → INIT）；`sched_nice` 的 KERNEL/NONE 拒绝；`nice_to_priority` 的线性缩放与钳位；`get_nice_value` 是逆函数；`do_getsetpriority` 的权限与 EACCES；`sched_stop` 在退出的调用点；`SCHEDULING_SET_NICE` 消息；`SEND_PRIORITY`/`SEND_TIME_SLICE` 的零使用事实。
- **不讲什么**：SCHED 服务器的调度算法（→ `../06-stage-sched/`）；内核调度队列（→ `../01-stage-kernel/`）。
- **前置**：00、01、02、03、04、05。
- **后置**：09、10、24。
- **事实底线**：`schedule.c:20-50`、`:55-84`、`:89-112`；`utility.c:91-103`；`main.c:241/275-289`；`misc.c:238-286`；`minix/sched.h`；`minix/config.h:68-74`；`minix/priv.h:93-100`；`const.h:19-20`；`libsys/sched_start.c`；Rust `sched.rs`。
- **知识点清单**：K-420 ~ K-429（10 条）。
- **验收标准**：给出"`nice ↔ queue` 双向换算"的两个公式并标出量化误差的测试位置；必须解释"为什么 `sched_start_user` 对 `PRIV_PROC` 父进程要从 INIT 继承"，带行号。

### 20-pm-ptrace

- **一句话定位**：`T_*` 全族请求、两套守卫（root 专属与通用），以及被跟进程如何停下来通知 tracer。
- **讲什么**：K-445~K-460。**先给真值表**（本篇必须按 `sys/ptrace.h` 全量对账，纠正旧文档的 `TO_NOEXEC` 与 `T_*` 错误）；`T_OK` 自声明；`T_ATTACH` 六重权限链；`T_STOP` 不对用户开放；`T_READB_INS`/`T_WRITEB_INS` 的 root 门**在通用守卫之前**；`T_GETRANGE`/`T_SETRANGE`；`T_EXIT` 的 `TRACE_EXIT` 与在途分叉；`T_SETOPT`；`T_DETACH` 的四步重放；`T_RESUME`/`T_STEP`/`T_SYSCALL` 的"假装成功"；通用守卫四条；`trace_stop` 与 `W_STOPCODE`；`TRACE_STOPPED` vs `PROC_STOPPED`；`mp_sigtrace`；内核透传与错误保真。
- **不讲什么**：`wait4` 的扫描（→ 11，只写 `wait_test` 调用点与 `W_STOPCODE` 载荷契约）；`sig_proc` 的判定链（→ 13）；内核 `sys_trace` 实现（→ `../01-stage-kernel/`）。
- **前置**：00、01、02、03、05、11、12、13、14。
- **后置**：无（叶子篇）。
- **事实底线**：`sys/ptrace.h:37-55`、`:209-211`、`:226-250`；`trace.c:41-250`、`:255-276`；Rust `trace.rs`、`mproc/trace.rs`、`mproc/guardianship.rs`。
- **知识点清单**：K-445 ~ K-460（16 条）。
- **验收标准**：给出"18 个 `T_*` × 真值 × 处理位置（PM 独办 / PM 半办 / 内核透传）"的完整表，真值必须与 `sys/ptrace.h` 逐条一致；必须解释"为什么 root 专属的两支必须在通用守卫之前返回"，带行号。

### 21-pm-time

- **一句话定位**：双时钟读写的分解式，以及"先取余再乘"为什么是防溢出而不是风格问题。
- **讲什么**：K-475~K-482。`CLOCK_REALTIME`/`MONOTONIC`；`getuptime` 三值；`sec = boottime + clock/hz` 与 `nsec = (clock%hz)*1e9/hz`；`do_getres` 的 `1e9/hz`；`do_settime` 的 root 门、MONOTONIC 不可变与 `now` 分叉；`do_stime` 的 boottime 重定；`do_time` 与 `do_gettime` 的 nsec 精度分叉；`system_hz` 的单点依赖。
- **不讲什么**：内核时钟设备与定时器中断（→ `../01-stage-kernel/`）；间隔定时器（→ 17）。
- **前置**：00、01、02、05。
- **后置**：22。
- **事实底线**：`time.c:22-47`、`:53-65`、`:71-88`、`:94-104`、`:110-131`；`libsys/getuptime.c`、`libsys/clock_time.c`；`sys/time.h:283/288`；Rust `time.rs`、`minix-types/src/types/clock.rs`。
- **知识点清单**：K-475 ~ K-482（8 条）。
- **验收标准**：给出"五个调用 × 时钟 id × 权限 × 返回值载荷"的矩阵；必须用一个具体数值演示"先乘后除会溢出、先取余不会"。

### 22-pm-misc-queries

- **一句话定位**：PM 的九个"查询与控制"调用，每一个都有自己的权限门与载荷契约。
- **讲什么**：K-495~K-504。`uts_tbl[8]` 兼容间接与 `req` 方向门；`getsysinfo` 的 `effuid==0` 门 + size 精确匹配 + `SI_PROC_TAB`（**数据路径已闭环，须按当前实现写**）/ `SI_CALL_STATS`；`getprocnr` 的 RS 专属；`getepinfo` 的全量 `ngroups` + 截断拷贝；`reboot` 定序与 `RB_POWERDOWN` 的 DS 查表；`svrctl` 的 `IOCGROUP` 门 + `sysgetenv` + `local_param_overrides[2]` + `find_param`；`getrusage` 三段；`sprofile` 门；`mcontext` 透传；`find_param` 遍历。
- **不讲什么**：`set_rusage_times` 的实现（→ 11，只写调用点）；DS 服务本身（→ `../07-stage-ds/`）；MIB 的序列化布局全表（→ 24）。
- **前置**：00、01、02、03、05、07、11。
- **后置**：24、25。
- **事实底线**：`misc.c:31-61`、`:72-100`、`:108-144`、`:149-164`、`:169-193`、`:198-233`、`:291-395`、`:400-447`；`profile.c:22-45`；`mcontext.c:12-26`；`utility.c:56-71`；`os/servers/pm/Cargo.toml`；Rust `misc.rs`、`minix-types/src/types/mproc.rs`、`mproc/wire.rs`。
- **知识点清单**：K-495 ~ K-504（10 条）。
- **验收标准**：给出"九个调用 × 权限门 × 载荷方向 × 失败 errno"的矩阵；必须纠正旧文档的"`getsysinfo` 拷零字节"口径——写出当前真实数据路径（C-ABI 464B 镜像序列化）的锚点。

### 23-pm-failure-model（新建）

- **一句话定位**：PM 的失败分三类——panic、errno、DEFERRED——本篇给出判定规约。
- **讲什么**：K-520~K-526。panic 全集（按"哪类不变式被打碎"分组，而不是按文件罗列）；errno 全集与触发条件；错误保真规约（`Kernel(i32)` 透传 vs PM 自身判定）；DEFERRED 契约格式与 D-01..D-32 台账指针；fail-closed vs fail-open 的判据；C `assert` 在 Rust 的对应层次；诊断输出口径（C `printf` = 内核 `SYS_DIAGCTL`，Rust 当前 `cfg(test)`，D-31）。
- **不讲什么**：各调用的具体错误分支（→ 各服务篇，本篇只给规约与索引）；DEFERRED 条目的逐条论证（→ 25 的台账 / todo.md）。
- **前置**：00、01、05。（本目录其余篇只引用本篇的条目编号，不依赖其内容，故放在 Part 8 不构成前向引用。）
- **后置**：无（横切篇）。
- **事实底线**：全源 `panic(` 调用点（须 grep 统计，不得凭记忆）；`plan.md:231-233`；`todo.md:135-167`；Rust `SchedError::Kernel` / `TraceError::Kernel` / `MiscError::Kernel`；`misc.rs`（fail-closed 落点）。
- **知识点清单**：K-520 ~ K-526（7 条，全部为**新增**）。
- **验收标准**：给出"panic 全集表"（不少于 20 条，每条带 `file:line` 与被打碎的不变式），以及"errno 全集表"（不少于 14 个 errno × 触发条件）；必须解释"`map_err(|_| Inval)` 为什么被禁止"，并给出至少一个反例。

### 24-pm-peer-protocols（新建）

- **一句话定位**：PM 与内核 / VM / VFS / RS / SCHED / MIB 的全部接触点，以及内核能力的四列对照表。
- **讲什么**：K-540~K-547。内核能力清单与四列对照（能力 ↔ trait ↔ `minix-sys` wrapper ↔ 内核对端）；`KernelGateway` 与窄 trait 的双风格规约（三条书面规约）；VM 四个调用；VFS 的 12 RQ + 11 RS 全表；RS 的五个接触点；SCHED 的四个客户端函数与消息；MIB 的 `SI_PROC_TAB` 序列化；PM 如何被装载（只声明 + 回指，不展开）。
- **不讲什么**：对端服务的**实现**（VM 的 `vm_fork` 内部、VFS 的 fproc 建立、SCHED 的算法、MIB 的存储——各归对应 stage）；内核 trap 层机制（→ `../01-stage-kernel/`、E1）。
- **前置**：00、01、05、07。
- **后置**：无（横切篇）。
- **事实底线**：`minix-sys/src/syscall.rs`；`minix/vm.h`；`com.h:520-544`；`minix/sched.h` + `libsys/sched_start.c`；`minix/sysinfo.h`；`plan.md:213-229`；`os/servers/pm/src/exit.rs`（`KernelGateway`）、`signal_flow.rs`/`exec.rs`（supertrait 组合先例）；`../00-master-plan/README.md`。
- **知识点清单**：K-540 ~ K-547（8 条，全部为**新增**）。
- **验收标准**：四列对照表不少于 20 行，每行四列都不能为空；必须解释"新内核能力一律进 `KernelGateway`"这一规约的两个先例（`RestartServices` / `ExecRestartServices`）在什么情况下被创立。

### 25-pm-test-and-status（新建，支线/可跳读）

- **一句话定位**：PM 的测试怎么跑、覆盖度怎么量、47 个调用还剩多少没接通——**本目录唯一允许出现易腐烂数字的地方**。
- **讲什么**：K-548~K-551。三层测试（单元 `--lib` / 集成 `tests/run_once_integration.rs` / doctest 当前失败的事实）；覆盖率 Gate（A：109 C 符号 / 100% 文档覆盖 / 93.6% 名称匹配；E：文档测试名对账）；47 调用接线批次台账（A–H）与 ENOSYS 兜底臂现状；测试名对账纪律（声称名必须 grep 命中；数量声明必须带日期与生成命令）。
- **不讲什么**：任何机制语义（→ 各篇）；DEFERRED 条目的技术论证（→ todo.md）。
- **前置**：00、05、06。
- **后置**：无（支线篇）。
- **事实底线**：`os/servers/pm/tests/run_once_integration.rs`（425 行 / 27 个测试函数）；`tools/coverage-extract/coverage-extract.py` + `pm-semantic-map.json`；`tools/design-coverage-check.sh`；`os/servers/pm/Cargo.toml`；`todo.md:135-167/267-278`。
- **知识点清单**：K-548 ~ K-551（4 条，全部为**新增**）。
- **验收标准**：每个数字都必须写出"生成命令 + 实测日期"；接线批次表必须写明"当前 47 臂全部有 match 分支，兜底臂只服务于未注册调用号"这一现状（**纠正旧 00 §3.1 的'8 个接线'口径**）。

---

## 6. 变更表

| 操作编号 | 操作类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 / 来源 |
|---------|---------|--------|--------|------|-----------|------------|
| C-01 | 重排 | `99-global-concepts.md` | `01-pm-constants.md` | 编号必须等于阅读序；旧 99 编号最后却被要求最先读 | K-001~K-016 | 存量：全部迁移，补"语义半径"写法 |
| C-02 | 重排 | `01-pm-init-main.md` | `04-pm-startup.md` | 结构/身份必须先于启动（X-1） | K-020~K-033 | 存量：八步与两条填充分支迁移；K-031/K-032 摘出至 19 |
| C-03 | 重排 | `02-mproc-struct.md` | `02-pm-mproc-struct.md` | 位置不变（编号巧合一致） | K-040~K-049 | 存量：字段与 19 位迁移；K-049 摘出至 09 |
| C-04 | 重排 | `03-mproc-table.md` | `03-pm-identity-table.md` | 位置不变 | K-060~K-068 | 存量：原样迁移；使用点摘出 |
| C-05 | 重排 | `04-ipc-dispatch.md` | `05-pm-main-loop.md` | 让主循环紧跟启动 | K-080~K-092 | 存量：骨架迁移；K-090 消费者摘出至 22 |
| C-06 | 重排 | `05-vfs-interaction.md` | `07-pm-vfs-protocol.md` | 与事件订阅成对，同属"异步协议" | K-120~K-130 | 存量：协议面与状态机迁移；各路业务语义摘出 |
| C-07 | 重排 | `06-event-subscription.md` | `08-pm-event-subscription.md` | 紧随 VFS 协议 | K-140~K-149 | 存量：原样迁移 |
| C-08 | 重排 | `07-pm-fork.md` | `09-pm-fork.md` | 生命周期组起始 | K-160~K-178 | 存量：迁移 |
| C-09 | **合并** | `07-pm-fork.md` + `08-pm-srv-fork.md` | `09-pm-fork.md` | srv_fork 是 fork 的差异子集（332 行，语义为 fork 的 6 处差异），分篇违反"单篇单语义"且把同一流程讲两遍 | K-175~K-178 | 存量去向：旧 08 §1（动机）、§2（五步）、§3（D7 同构、D8 空分支对照）、§4（编排层/复制层/协议层）→ 新 09 的"§srv_fork 差异表 + 五步简述"；旧 08 §5 测试矩阵 → 新 25。**删除**：旧 08 中"5 步同构"的完整复述（与新 09 的 fork 段逐字重复） |
| C-10 | 重排 | `09-pm-exit.md` | `10-pm-exit.md` | 生命周期组 | K-190~K-208 | 存量：迁移 |
| C-11 | 重排 | `10-pm-wait.md` | `11-pm-wait.md` | 生命周期组 | K-220~K-231 | 存量：迁移 |
| C-12 | 重排（拆环的一部分） | `11-signal-core.md` | `13-pm-signal-dispatch.md` | 见 C-13/C-14 | K-275~K-286 | 存量：生成/分派/终止全部迁至新 13 |
| C-13 | **拆分** | `12-signal-handlers.md` | `12-pm-signal-model.md`（前半）+ `15-pm-signal-delivery.md`（后半） | 旧 12 的 `sig_send` 依赖旧 13 的 `unpause`，构成环；拆为"模型与安装"（前置）与"投递"（后置） | K-250~K-261 → 12；K-320~K-326 → 15 | 存量去向：旧 12 §1.1-1.5 与 sigaction/sigpending/sigprocmask/sigsuspend/sigreturn → 新 12；旧 12 §1.6（`sig_send` 是翻译器）与 sig_send 段 → 新 15。**删除**：旧 12 §5 中三个 wire roundtrip 测试名的超前声称（V3-P1-5 已改为 forward-reference，重建后归 25 的 E7 前置说明） |
| C-14 | 重排（拆环的一部分） | `13-signal-flow.md` | `14-pm-signal-suspend-resume.md` | 停止/恢复原语必须排在投递之前（X-3） | K-300~K-309 | 存量：迁移；K-303（SIGSNDELAY 尾部）与旧 11 的 `process_ksig` 段（K-329/K-330/K-331）摘出至新 15 |
| C-15 | 重排 | `17-exec.md` | `16-pm-exec.md` | exec 必须在信号之后（X-7） | K-345~K-358 | 存量：迁移 |
| C-16 | 重排 | `14-itimer.md` | `17-pm-itimer.md` | 进入"场景分组"并行体 | K-370~K-380 | 存量：迁移 |
| C-17 | 重排 | `15-credentials.md` | `18-pm-credentials.md` | 同上 | K-395~K-404 | 存量：迁移 |
| C-18 | 重排 + **合并** | `16-scheduling.md` + 旧 `01 §2.5/§2.6` | `19-pm-scheduling.md` | 同上；并把越界的 `get_nice_value`/`sched_init` 收回归属 | K-420~K-429 + K-031/K-032 | 存量去向：旧 01 §2.5（get_nice_value 公式）与 §2.6（sched_init 细节）原样并入新 19；旧 01 保留一句调用点 |
| C-19 | 重排 | `18-trace.md` | `20-pm-ptrace.md` | 同上；且必须排在 wait 之后（依赖 `wait_test`） | K-445~K-460 | 存量：迁移；**纠错**：`TO_NOEXEC` 按 `ptrace.h:211` 改为 0x4；`T_*` 全量按 `ptrace.h:226-250` 对账 |
| C-20 | 重排 | `19-time.md` | `21-pm-time.md` | 查询与杂项组 | K-475~K-482 | 存量：迁移 |
| C-21 | 重排 | `20-misc-queries.md` | `22-pm-misc-queries.md` | 同上 | K-495~K-504 | 存量：迁移；**纠错**：`getsysinfo` 数据路径按当前实现重写 |
| C-22 | **拆分** | `00-pm-overview.md` §3（实施现状）+ §4.1 | `25-pm-test-and-status.md` + `24-pm-peer-protocols.md` | 易腐烂内容不得进导航篇（O-1） | K-548~K-551；K-540/K-541 | 存量去向：旧 00 §3.1（分发与接线）→ 25 批次台账；§3.2（跨阶段依赖）→ 25 的 edge 指针；§3.3（测试基线）→ 25 的实测命令与日期；§4.1（端口面规约）→ 24 |
| C-23 | **新建** | — | `06-pm-wire-format.md` | 缺口 G-1 | K-100~K-104 | 新增来源：`minix/ipc.h` 各 wire 结构 + `ipc/decode.rs` + `minix-types/src/ipc/*` + edge E7 |
| C-24 | **新建** | — | `15-pm-signal-delivery.md` | 缺口 G-5/G-6；且拆环需要 | K-320~K-332 | 新增来源：`signal.c:294-378/775-855` + `main.c:122` + `libsys/sef_signal.c` + `init.rs` notify 分支 / `signal.rs::process_sigmgr_signals` / `minix-sys` 三个 wrapper；存量：旧 12 §1.6 与旧 11 的 `process_ksig` 段 |
| C-25 | **新建** | — | `23-pm-failure-model.md` | 缺口 G-2 | K-520~K-526 | 新增来源：全源 `panic(` grep + `plan.md:231-233` + `todo.md:135-167` + Rust 三个 `Kernel(i32)` 变体 |
| C-26 | **新建** | — | `24-pm-peer-protocols.md` | 缺口 G-3 | K-540~K-547 | 新增来源：`plan.md:213-229` 四列表 + `minix/sched.h` + `minix/vm.h` + `com.h:520-544` + `minix/sysinfo.h` + `minix-sys/src/syscall.rs` + `libsys/sched_start.c` + 旧 00 §4.1 |
| C-27 | **新建** | — | `25-pm-test-and-status.md` | 缺口 G-4 | K-548~K-551 | 新增来源：`tests/run_once_integration.rs` + `tools/coverage-extract/` + `tools/design-coverage-check.sh` + `todo.md:135-167/267-278` + 旧 00 §3 |
| C-28 | **归档** | `draft/` 9 篇 | 归档不删 | 已被 22 篇正式文档吸收；`draft/README.md` 引用不存在的 `pm-call-vm-fork.md`（断链）不再被引用 | — | 素材：仅 B 相取料用；正式文档不引用 |
| C-29 | **归档** | 旧 22 篇编号文档 | 归档不删 | B 相按新编号重写，旧篇退出正式目录 | — | 全部知识点已在 §2 建池并有去向 |

**统计**：重排 18 处、合并 2 处（2→1、1+1→1）、拆分 2 处（2→4）、新建 5 篇、归档 2 组。净变化：22 篇 → 26 篇。

---

## 7. 缺漏新篇（按非 C 主题固定清单逐项落实）

| # | 主题 | 为什么重要 | 原料在哪 | 归哪一篇 | 验收标准 |
|---|------|-----------|---------|---------|---------|
| N-1 | 跨模块接口与线格式 | 47 个调用的参数/返回值布局是 PM 与外界的**唯一契约面**；当前散落在 21 篇的 §2.x，读者无法一次查全，`ipc/decode.rs` 的 unsafe 收敛纪律也无处可查 | `minix/ipc.h`；`ipc/decode.rs`；`minix-types/src/ipc/{pm,vfs,event,message}.rs`；`edge_todo.md` E7 | **06-pm-wire-format**（新建） | 47 调用 × 入站结构 × 出站臂的索引表齐全；7 条 `size_of` 断言逐条列出 |
| N-2 | 错误路径 | PM 的失败分三类（panic / errno / DEFERRED），三类各有规约；当前无处可查，导致"8 个接线"、"假数据"之类失真口径长期留存 | 全源 `panic(`；`plan.md:231-233`；`todo.md:135-167`；Rust `SchedError::Kernel` 等 | **23-pm-failure-model**（新建） | panic 全集 ≥20 条带 `file:line` 与不变式说明；errno 全集 ≥14 个 |
| N-3 | 对端服务协议面 | PM 的每一个语义动作都要跨服务器完成，但"内核/VM/VFS/RS/SCHED/MIB 各自给 PM 什么"只在 plan.md 有表、正文缺失 | `plan.md:213-229`；`minix/vm.h`；`minix/sched.h`；`com.h:520-544`；`minix/sysinfo.h`；`minix-sys/src/syscall.rs` | **24-pm-peer-protocols**（新建） | 四列对照表 ≥20 行，四列均非空；两个 supertrait 先例有解释 |
| N-4 | 用户态启动与运行时装载 | **明确不在本 stage 展开**。PM 由 RS 加载，ELF 装载与地址空间由 VM/RS 完成，属 03/02/14-stage | `../00-master-plan/README.md`；`../03-stage-rs/`；`../02-stage-vm/`；`../14-stage-runtime/` | **否决**（判为其它 stage）；`04-pm-startup` 只写一句"PM 由 RS 加载，加载机制见 …"并回指 | `04` 的 §前置 必须出现该回指，且不得展开 ELF/页表内容 |
| N-5 | 测试基建 | 三层测试、覆盖率 Gate、接线批次、测试名对账纪律无处安放，导致 00 §3.3 与各篇 §5 同时腐烂 | `tests/run_once_integration.rs`；`tools/coverage-extract/`；`tools/design-coverage-check.sh`；`todo.md:135-167/267-278` | **25-pm-test-and-status**（新建，支线） | 每个数字带生成命令与日期；接线表写明 47 臂全接的现状 |
| N-6 | 链接与加载 / 镜像与内存布局 / 汇编入口与陷阱进入 | 三项均判为**其它 stage**（见 §3.5）；本 stage 只在 `04`（制品形态 + boot image 消费）与 `24`（trap 面四列表）各给一行 | `Makefile`；`Cargo.toml`；`minix-sys` | 否决（不在本 stage） | `04` 与 `24` 各出现一行且带回指；不得展开 |
| N-7 | 构建与工具链 | C 的两个编译宏与 Rust 的两个 feature 是"同一开关的两侧"，读者需要知道怎么开 | `Makefile`；`Cargo.toml`；`misc.c:64/131-132`；`profile.c` | **22-pm-misc-queries**（语义）+ **25**（跑法） | `22` 给出宏↔feature 对照；`25` 给出 `cargo check --features` 命令 |
| N-8 | 关闭与退出 | 系统关机（`do_reboot`）与 PM 自身终止（`HARD_STOP`）是两条不同路径，当前混在杂项篇里一句带过 | `misc.c:198-233`；`main.c:304-312` | **22**（reboot 定序）+ **05/07**（HARD_STOP 通知） | 两条路径各有一段且互不混淆 |
| N-9 | 并发与同步 | PM 是单线程事件循环，这一事实决定了"为什么 Rust 侧可以用 `&mut ProcTable`"——需要一处显式论证，否则读者会误以为需要锁 | `glo.h`；`main.c:59-107`；`mproc/context.rs` | **03**（A-3）+ **05**（循环结构） | 两篇各一段，明确"无锁、无原子、无 `Send`/`Sync` 要求" |
| N-10 | 非 C 制品索引 | 读者需要一张"除了 .c 之外还应该读哪些文件"的清单 | 见 §9 G2 的非 C 制品表 | **24**（索引附录）+ **06**（wire） | 清单 ≥18 项，每项带路径 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（旧 22 篇 → 新 26 篇，逐篇 + 关键小节）

| 旧位置 | 旧内容（一句话） | 新位置 | 迁移类型 | 备注（断链风险） |
|--------|-----------------|--------|---------|-----------------|
| `99-global-concepts.md` §1.1 | 常量的语义半径 | `01-pm-constants.md` §概念 | 原样搬移 | **编号从 99 变 01，全目录引用最多的一处之一**（文档内 4 处 + 跨篇多处） |
| `99` §2.1 | 身份与容量常量表 | `01` §常量表 | 原样搬移 | 与 `03` 旧 §1.3 有重复，合并时以 `01` 为准 |
| `99` §2.2 | endpoint 代际编码 | `01` §编码 | 原样搬移 | 旧 `03 §1.4` 改写为"校验门"引用 |
| `99` §2.3 | 信号三集合 | `01` §三集合 | 原样搬移 | 旧 `01 §3.3`、`11 §1.4` 改为引用 |
| `99` §2.4 | 全局七件套 | `01` §全局状态 | 原样搬移 | ARCH A-3 标注随迁 |
| `00-pm-overview.md` §1 | 微内核角色与四重权威 | `00-pm-overview.md` §概念 | 改写 | 保留 |
| `00` §2.1/§2.2/§2.3 | 源码地图 / Rust 模块镜像 / 阅读路线 | `00` §导航 | 改写（22 → 26 行） | 导航表全量重写 |
| `00` §3.1 | 分发与接线（"8 个接线"） | `25-pm-test-and-status.md` §接线批次 | **拆分 + 改写** | **事实纠错**：实测 47 臂全接 |
| `00` §3.2 | 跨阶段依赖 | `25` §edge 指针 | 拆分 | 回指 `edge_todo.md` |
| `00` §3.3 | 测试基线（381+11） | `25` §实测基线 | 拆分 + 改写 | **事实纠错**：实测 403 + 11 |
| `00` §4.1 | 端口面规约 | `24-pm-peer-protocols.md` §双风格规约 | 拆分 | 四列对照表随迁 |
| `01-pm-init-main.md` §1.1-1.6 | 启动链概念 | `04-pm-startup.md` §概念 | 原样搬移 | 编号 01→04，风险中等（文档内 18 处引用） |
| `01` §2.1 | `main()` 与主循环骨架 | `05-pm-main-loop.md` §骨架 | **拆分** | 与 `04-ipc-dispatch §2.1` 是同一内容的第三份副本，三合一 |
| `01` §2.2 | `sef_local_startup` | `04` §SEF 回调 | 原样搬移 | |
| `01` §2.4 | `sef_cb_init_fresh` 八步 | `04` §八步 | 原样搬移 | 行号锚点需按 `main.c:131-244` 复核（旧文档有 3-4 行漂移记录） |
| `01` §2.5 | `get_nice_value` | `19-pm-scheduling.md` | **拆分（越界归位）** | |
| `01` §2.6 | `sched_init` | `19-pm-scheduling.md` | **拆分（越界归位）** | |
| `01` §3 | Rust 设计决策 D1-D6 | `04` §Rust 决策 | 原样搬移 | ARCH 标注随迁 |
| `02-mproc-struct.md` §2.1/§2.2/§2.3 | mproc 字段 / 19 位 / 信号位图 | `02-pm-mproc-struct.md` 同名小节 | 原样搬移 | 编号不变，风险低（但文档内 35 处引用需改文件名） |
| `02` §2.5 | fork 的字段继承 | `09-pm-fork.md` §复制与重整 | **拆分（越界归位）** | |
| `03-mproc-table.md` §2.2/§2.3/§2.4 | `pm_isokendpt` / `find_proc` / `get_free_pid` | `03-pm-identity-table.md` 同名小节 | 原样搬移 | 编号不变，风险低（30 处引用需改文件名） |
| `03` §2.5 | slot 分配/释放 | `03`（原语）+ `09/10/11`（使用点） | 拆分 | |
| `04-ipc-dispatch.md` §1.3/§2.4 | 三路分发 | `05-pm-main-loop.md` §分发 | 原样搬移 | **文档内引用最多的旧篇（36 处）**，文件名变更影响最大 |
| `04` §1.4/§3.2 | SUSPEND / `ReplyIntent` | `05` §回复意图 | 原样搬移 | 各服务篇的重复段落改为引用 |
| `04` §2.8 | 调用统计 | `22`（消费）+ `25`（feature） | 拆分 | |
| `05-vfs-interaction.md` §2.1-§2.5 | 协议面 / tell_vfs / handle_vfs_reply / NEW_PARENT | `07-pm-vfs-protocol.md` 同名小节 | 原样搬移 | 编号 05→07（31 处引用） |
| `06-event-subscription.md` §2.1-§2.9 | 全部 | `08-pm-event-subscription.md` | 原样搬移 | 编号 06→08（15 处引用） |
| `07-pm-fork.md` §2.1-§2.10 | do_fork 全链路 | `09-pm-fork.md` §fork 主体 | 原样搬移 | 编号 07→09（9 处引用） |
| `08-pm-srv-fork.md` 全文 | do_srv_fork | `09-pm-fork.md` §srv_fork 差异 | **合并** | 旧篇整体退出（6 处引用需改指向） |
| `09-pm-exit.md` §2.1-§2.9 | 退出全链路 | `10-pm-exit.md` | 原样搬移 | 编号 09→10（18 处引用） |
| `10-pm-wait.md` §2.1-§2.10 | wait 全链路 | `11-pm-wait.md` | 原样搬移 | 编号 10→11（10 处引用） |
| `11-signal-core.md` §2.1-§2.x | kill / check_sig / sig_proc / process_ksig | `13-pm-signal-dispatch.md`（主体）+ `15-pm-signal-delivery.md`（process_ksig） | **拆分** | 编号 11→13（34 处引用，第二热点） |
| `12-signal-handlers.md` §2 sigaction 族 | 安装与掩码 | `12-pm-signal-model.md` | 原样搬移（编号 12 保持） | 文件名变更，10 处引用需改 |
| `12` §1.6 + §2 sig_send | 投递 | `15-pm-signal-delivery.md` | **拆分** | |
| `13-signal-flow.md` §2 | stop_proc / unpause / check_pending / restart_sigs | `14-pm-signal-suspend-resume.md` | 原样搬移（编号 13→14） | 16 处引用需改 |
| `14-itimer.md` 全文 | 定时器 | `17-pm-itimer.md` | 原样搬移（编号 14→17） | 10 处引用需改 |
| `15-credentials.md` 全文 | 凭证 | `18-pm-credentials.md` | 原样搬移（编号 15→18） | 6 处引用需改 |
| `16-scheduling.md` 全文 | 调度 | `19-pm-scheduling.md` | 原样搬移（编号 16→19） | 13 处引用需改 |
| `17-exec.md` 全文 | exec | `16-pm-exec.md` | 原样搬移（编号 17→16） | 5 处引用需改 |
| `18-trace.md` 全文 | ptrace | `20-pm-ptrace.md` | 原样搬移 + **纠错** | 8 处引用需改；`TO_NOEXEC` 与 `T_*` 真值必须改 |
| `19-time.md` 全文 | 时间 | `21-pm-time.md` | 原样搬移（编号 19→21） | 3 处引用需改 |
| `20-misc-queries.md` 全文 | 杂项 | `22-pm-misc-queries.md` | 原样搬移 + **纠错** | ≤5 处引用；`getsysinfo` 口径必须改 |
| 各篇 §5 测试矩阵 | 逐测试名 + 总数 | 各篇 §验证锚点 + `25` | **改写** | 21 篇 §5 全部重写 |

### 8.2 引用迁移表（跨文档 + 代码注释）

**文档内引用（04-stage-pm 22 篇正文，实测 377 处）** —— 迁移规则为纯编号映射，可批量完成：

| 旧引用（出现次数） | 新目标 | 验证方式 |
|------------------|--------|---------|
| `04-ipc-dispatch.md`（36） | `05-pm-main-loop.md` | `rg -l '04-ipc-dispatch' 04-stage-pm/ ` 重建后应为 0（归档区除外） |
| `02-mproc-struct.md`（35） | `02-pm-mproc-struct.md` | 同上 |
| `11-signal-core.md`（34） | `13-pm-signal-dispatch.md`（或按小节分到 `15`） | 同上 |
| `05-vfs-interaction.md`（31） | `07-pm-vfs-protocol.md` | 同上 |
| `03-mproc-table.md`（30） | `03-pm-identity-table.md` | 同上 |
| `09-pm-exit.md`（18）/ `01-pm-init-main.md`（18） | `10-pm-exit.md` / `04-pm-startup.md` | 同上 |
| `13-signal-flow.md`（16） | `14-pm-signal-suspend-resume.md` | 同上 |
| `06-event-subscription.md`（15） | `08-pm-event-subscription.md` | 同上 |
| `16-scheduling.md`（13） | `19-pm-scheduling.md` | 同上 |
| `14-itimer.md` / `12-signal-handlers.md` / `10-pm-wait.md`（各 10） | `17` / `12` / `11` | 同上 |
| `07-pm-fork.md`（9） | `09-pm-fork.md` | 同上 |
| `18-trace.md`（8）/ `15-credentials.md`（6）/ `08-pm-srv-fork.md`（6） | `20` / `18` / `09`（**合并指向**） | 同上 |
| `17-exec.md`（5） | `16-pm-exec.md` | 同上 |
| `99-global-concepts.md`（4）/ `00-pm-overview.md`（4） | `01-pm-constants.md` / `00` | 同上 |
| `19-time.md`（3）/ `20-misc-queries.md`（≤5） | `21` / `22` | 同上 |
| 跨 stage 引用（`19-syscall-signal.md` 14、`06-proc-init-boot-proc.md` 8、`18-vm-fork.md` 6、`15-clock-timer.md` 8、`21-clock-device.md` 5 等） | 不变（指向 `../01-stage-kernel/` 等） | 复核路径仍存在 |

**代码注释引用（实测 53 处，全在 `os/servers/pm/src/`）**：

| 热点文件 | 处数 | 主要旧引用 | 新目标 |
|---------|------|-----------|--------|
| `init.rs` | 23 | `01-pm-init-main.md`、`04-ipc-dispatch.md`、`14-itimer.md` | `04-pm-startup.md`、`05-pm-main-loop.md`、`17-pm-itimer.md` |
| `mproc/mproc.rs` | 7 | `02-mproc-struct.md` | `02-pm-mproc-struct.md` |
| `ipc/dispatcher.rs` | 4 | `04-ipc-dispatch.md` | `05-pm-main-loop.md` |
| `mproc/table.rs` | 3 | `03-mproc-table.md` | `03-pm-identity-table.md` |
| `ipc/vfs.rs` | 3 | `05-vfs-interaction.md` | `07-pm-vfs-protocol.md` |
| `mproc/pid_gen.rs` / `main.rs` / `ipc/calls.rs` / `exit.rs` / `event.rs` | 各 2 | 按上表映射 | 按上表映射 |
| `mproc/trace.rs` / `mproc/context.rs` / `fork.rs` | 各 1 | 按上表映射 | 按上表映射 |

**其它需要同步的地方**：

1. `os/libs/minix-types/src/ipc/{pm,vfs,event}.rs` 的注释里也引用了 PM 文档编号（旧 05/06 的 14 个测试名归属错位即源于此），按同表迁移并标注 crate。
2. 各篇的 `.design/` 三快照（69 个文件，以旧编号命名）：B 相按 Step 0.3 重新生成，不沿用旧快照；本蓝图不引用其内容。
3. `.review/**` 下的历史 scan/structure/VERIFY-CHECK 产物引用旧编号（如 `.review/codex/sched/13-pm-interaction/scan.md` 引用 `../04-stage-pm/16-scheduling.md`）——属只读历史产物，建议 B 相不做批量改写，只在新一轮 review 中按新编号引用。

### 8.3 断链成本摘要

| 项 | 数量 | 说明 |
|----|------|------|
| 文档内编号引用（22 篇正文） | **377** | 纯编号映射，可用 `sed`/脚本批量替换，逐条无歧义（旧编号 → 新编号一一对应，仅 `07+08 → 09` 与 `11 → 13/15`、`12 → 12/15`、`00§3 → 24/25` 五处需要人工判读） |
| PM crate 代码注释引用 | **53** | 热点 `init.rs`（23），单文件可人工完成 |
| minix-types 注释引用 | ≥14 | 需标注 crate 归属 |
| 需人工判读的合并/拆分点 | **5** | 旧 `08`→`09`、`11`→`13`或`15`、`12`→`12`或`15`、`00§3`→`24`或`25`、`01§2.5/2.6`→`19` |
| 需事实纠错（不只是换名） | **4** | `TO_NOEXEC`（18 篇）、`T_*` 真值（18 篇）、`getsysinfo` 数据路径（20/22）、接线数与测试基线（00/25） |
| `.design/` 快照 | 69 | 重生成，不迁移 |
| `.review/**` 历史产物 | 数十 | 建议不批量改写 |

**建议的批量修改方式**：
1. 先做**纯换名**（22 条旧文件名 → 新文件名的一一映射，除 5 个判读点外零歧义），用脚本一次性替换 `04-stage-pm/*.md` 与 `os/servers/pm/src/**/*.rs` 与 `os/libs/minix-types/src/ipc/*.rs` 中的反引号文件名；
2. 再**人工处理 5 个判读点**（合并/拆分指向）；
3. 最后做**4 处事实纠错**，必须逐条回到 C 源码复核（不得只改编号不改内容）；
4. 全程以 `rg -l '<旧文件名>' .` 归零作为验收。

> **成本判断**：377 处引用中约 370 处是纯换名，可脚本化；真正需要人读的只有 5 个判读点 + 4 处事实纠错。断链成本**可控且值得**——对比停在原地的代价（编号与阅读序长期不一致、12 处重复主题、13 处越界、4 处已知事实错误），重建的收益明显更高。

---

## 9. 验证与自检门

### 9.1 四种机械检查（§5.4）

1. **前向引用扫描**：逐篇检查契约的"前置"字段，只允许指向更早编号。

   | 篇 | 前置 | 是否指向更早 |
   |----|------|------------|
   | 00 | 无 | ✅ |
   | 01 | 00 | ✅ |
   | 02 | 00, 01 | ✅ |
   | 03 | 00, 01, 02 | ✅ |
   | 04 | 00, 01, 02, 03 | ✅ |
   | 05 | 00, 01, 02, 03 | ✅ |
   | 06 | 00, 01, 05 | ✅ |
   | 07 | 00, 01, 02, 03, 05 | ✅ |
   | 08 | 00, 01, 02, 05, 07 | ✅ |
   | 09 | 00, 01, 02, 03, 05, 07 | ✅ |
   | 10 | 00, 01, 02, 03, 05, 07, 08, 09 | ✅ |
   | 11 | 00, 01, 02, 03, 05, 07, 09, 10 | ✅ |
   | 12 | 00, 01, 02, 05 | ✅ |
   | 13 | 00, 01, 02, 05, 12 | ✅ |
   | 14 | 00, 01, 02, 05, 07, 12, 13 | ✅ |
   | 15 | 00, 01, 02, 05, 12, 13, 14 | ✅ |
   | 16 | 00, 01, 02, 05, 07, 12, 13, 14, 15 | ✅ |
   | 17 | 00, 01, 02, 05, 13 | ✅ |
   | 18 | 00, 01, 02, 03, 05, 07 | ✅ |
   | 19 | 00, 01, 02, 03, 04, 05 | ✅ |
   | 20 | 00, 01, 02, 03, 05, 11, 12, 13, 14 | ✅ |
   | 21 | 00, 01, 02, 05 | ✅ |
   | 22 | 00, 01, 02, 03, 05, 07, 11 | ✅ |
   | 23 | 00, 01, 05 | ✅ |
   | 24 | 00, 01, 05, 07 | ✅ |
   | 25 | 00, 05, 06 | ✅ |

   两处例外需显式声明：**(a)** 各服务篇会写"错误保真规约见 `23`"、"四列对照表见 `24`"——这是**条目编号引用**（读者不必先读即可理解当前段落），不构成前向引用；**(b)** §4.4 序差表 X-4/X-5/X-6/X-7 记录的四处支线触发点，均已规定"只写触发条件与参数 + 标注归属"，读者不需要先读后续篇。

2. **依赖关系图检查**（由"前置"构图）：

   ```
   00 ──► 01 ──► 02 ──► 03 ──┬──► 04 ──► 05 ──┬──► 06 ──► {09..22, 25}
                              │                ├──► 07 ──► 08 ──► 10
                              │                └──► 09 ──► 10 ──► 11
                              └────────────────────────────────► 12 ──► 13 ──► 14 ──► 15 ──► 16
                                                                                      └──► 17/18/19/20/21/22
                                                        05 ──► 23 / 24（横切）
   ```

   **无环**。被拆掉的旧环：`sig_send(旧12) → unpause(旧13) → restart_sigs(旧13) → sig_proc(旧11) → sig_send(旧12)`，拆解方式见 C-13/C-14。

3. **覆盖率检查**：171 条知识点逐条有去向（见 §2.26 与 §6 变更表的"去向/来源"列）。显式删除项仅 2 条：

   | 删除项 | 理由 |
   |--------|------|
   | 旧 `08-pm-srv-fork.md` 中"5 步同构"的完整复述 | 与新 `09` 的 fork 段逐字重复；合并后以"差异表"替代，信息不丢失 |
   | 旧 `12-signal-handlers.md` §5 的三个 wire roundtrip 测试名（`test_mess_lc_pm_sig_roundtrip` 等） | 超前声称（wire 尚不存在，属 E7 前置产物）；改为 25 篇的"E7 前置说明"，不再以测试名声称 |

   23 条新增知识点全部有证据锚点（§2.6/§2.15/§2.23/§2.24/§2.25 的锚点列非空）。

4. **断链成本统计**：见 §8.3（377 / 53 / 69 / 5 判读点 / 4 纠错点）。

### 9.2 自检门（G1-G9）

| 门 | 检查内容 | 结果 |
|----|---------|------|
| **G1** | C 真序逐条可核对（抽十条核对锚点） | ✅ 通过。抽样与核对结果：S6(`main.c:147-152` 表初始化)✅；S11(`main.c:188-201` INIT 分支)✅；S15(`main.c:232-236` sendrec 屏障)✅；L2(`main.c:65-71` CLOCK notify)✅；L6(`main.c:81-82` EXITING 丢弃)✅；L11(`main.c:106` SUSPEND 判据)✅；分派 47 臂(`table.c:14-61` + `calls.rs:141-187`)✅；`T_*` 真值(`sys/ptrace.h:226-250`)✅；`TO_NOEXEC=0x4`(`sys/ptrace.h:211`，与旧 18 的 0x1 冲突，旧文档错)✅；`get_free_pid` 相位(`utility.c:34-51`)✅ |
| **G2** | 每个 C 文件、每个非 C 制品都有归属或明确排除理由 | ✅ 通过。见下表 §9.3 / §9.4 |
| **G3** | 新目录前向引用为零 | ✅ 通过（见 9.1 第 1 项；26/26 篇的前置全部指向更早） |
| **G4** | 依赖图无环 | ✅ 通过；旧环已识别并给出拆解方案（C-13/C-14） |
| **G5** | 覆盖率 100%（每条有去向或删除理由；新增条目有锚点） | ✅ 通过；171 条全部有去向，2 条显式删除并有理由，23 条新增全部有锚点 |
| **G6** | 拆分/合并写清存量知识点去向；新建写清新增来源（抽查十处） | ✅ 通过。抽查：C-09（合并，K-175~K-178 去向明确 + 1 条删除）✅；C-13（拆分，K-250~K-261 与 K-320~K-326 分向明确 + 1 条删除）✅；C-14（重排，K-303/K-329~K-331 摘出）✅；C-18（合并，K-031/K-032 归位）✅；C-22（拆分，K-540/541→24、K-548~551→25）✅；C-23（新建，K-100~104 来源为 ipc.h + decode.rs + E7）✅；C-24（新建，K-327/328/331 来源为 main.c:122 + sef_signal.c + init.rs）✅；C-25（新建，K-520~526 来源为 panic grep + plan.md:231-233 + todo.md:135-167）✅；C-26（新建，K-540~547 来源为 plan.md:213-229 等）✅；C-27（新建，K-548~551 来源为 tests/ + tools/）✅ |
| **G7** | 每篇契约七要素齐全 | ✅ 通过；26/26 篇均有（定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 + 验收标准） |
| **G8** | 锚点迁移表覆盖所有变化文档的每一节；引用迁移表覆盖文档与代码注释 | ✅ 通过。§8.1 覆盖旧 22 篇的全部一级/关键二级小节（22 篇 × 平均 3-5 行）；§8.2 覆盖 377 处文档引用 + 53 处 PM crate 注释 + minix-types 注释 + `.design/` 与 `.review/` 的处置说明 |
| **G9** | 事实断言都有锚点（抽十条核对）；推测项已标注 | ✅ 通过。抽查：`NR_PIDS=30000`(`const.h:3`)✅；`INIT_PROC_NR=11`(`com.h:72`)✅；`PROC_NAME_LEN=16`(`minix/type.h:145`)✅；`_NSIG=64`(`sys/signal.h:45`)✅；`VFS_PM_RQ_BASE=0x900`(`com.h:513`)✅；`NR_SUBS=4`(`event.c:58`)✅；`T_STOP=-1`(`ptrace.h:238`)✅；`MAX_USER_Q=0`(`config.h:68`)✅；`SRV_Q==USER_Q`(`priv.h:93`)✅；`PROC_EVENT_EXIT=0x01`(`syslib.h:292`)✅。**已标注为推测/待验证的项**：(a) `libsys/sef_signal.c` 的 SIGKSIG 拦截行号未在本次逐行核对（只确认 `main.c:122` 注册点 + Rust 侧实现），B 相写 `15` 篇前须 `sed` 复核；(b) `libsys/asynsend.c` 的 `ASYN_NR` 具体值未读（只引用"容量约束"这一语义），B 相写 `07/08` 篇前须复核；(c) `sys/signal.h:52-83` 的信号号表为区间锚点，逐号对照须在 `01` 篇完成时做 |

### 9.3 C 制品归属表（G2 证据）

| 文件 | 行数 | 归属 |
|------|------|------|
| `main.c` | 424 | `04`（启动/八步）+ `05`（循环/reply）+ `07`（handle_vfs_reply 路由）+ `19`（get_nice_value） |
| `forkexit.c` | 807 | `09`（fork/srv_fork）+ `10`（exit 链）+ `11`（wait 链） |
| `signal.c` | 855 | `13`（kill/check_sig/sig_proc/sig_proc_exit）+ `12`（sigaction 四调用）+ `14`（stop_proc/try_resume/unpause/check_pending/restart_sigs）+ `15`（sig_send/process_ksig） |
| `misc.c` | 447 | `22`（sysuname/getsysinfo/getprocnr/getepinfo/reboot/svrctl/getrusage）+ `19`（do_getsetpriority） |
| `event.c` | 353 | `08` |
| `alarm.c` | 344 | `17` |
| `exec.c` | 200 | `16` |
| `trace.c` | 276 | `20` |
| `getset.c` | 223 | `18` |
| `utility.c` | 156 | `03`（get_free_pid/find_proc/pm_isokendpt）+ `07`（tell_vfs）+ `19`（nice_to_priority）+ `11`（set_rusage_times）+ `22`（find_param） |
| `time.c` | 131 | `21` |
| `schedule.c` | 112 | `19` |
| `table.c` | 62 | `05` |
| `mcontext.c` | 27 | `22` |
| `profile.c` | 45 | `22` |
| `mproc.h` | 106 | `02`（+ `01` 的 MP_MAGIC / `03` 的表声明） |
| `glo.h` | 31 | `01`（全局七件套）+ `03`（procs_in_use） |
| `pm.h` | 27 | `01`（头组织与常量包含面） |
| `const.h` | 20 | `01` |
| `type.h` | 5 | `01` |
| `proto.h` | 96 | `05`（调用面函数索引附录） |

### 9.4 非 C 制品归属表（G2 证据）

| 制品 | 归属 |
|------|------|
| `minix/include/minix/callnr.h` | `05`（47 调用号）+ `01`（PM_BASE 判别） |
| `minix/include/minix/com.h` | `07`（VFS_PM_*）+ `08`（PROC_EVENT）+ `24`（协议面）+ `01`（INIT_PROC_NR） |
| `minix/include/minix/ipc.h` | `06`（wire 布局） |
| `minix/include/minix/sched.h` | `19` + `24` |
| `minix/include/minix/syslib.h` | `08`（PROC_EVENT_*） |
| `minix/include/minix/type.h`（boot_image） | `04` |
| `minix/include/minix/config.h` / `priv.h` / `param.h` | `19`（队列常量）/ `04`（NR_BOOT_PROCS）/ `01`（NR_PROCS） |
| `sys/sys/signal.h` | `01`（_NSIG/信号号/内核信号） |
| `sys/sys/ptrace.h` | `20`（T_* / TO_*） |
| `sys/sys/resource.h` / `wait.h` / `svrctl.h` / `reboot.h` / `sysinfo.h` / `time.h` | `22`（rusage / W_EXITCODE / PMSETPARAM / RB_* / SI_* / CLOCK_*）+ `11`（W_EXITCODE/W_STOPCODE） |
| `minix/servers/pm/Makefile` | `04`（制品形态） |
| `os/servers/pm/Cargo.toml` | `22`（feature 语义）+ `25`（跑法） |
| `os/servers/pm/tests/run_once_integration.rs` | `25` |
| `tools/coverage-extract/{coverage-extract.py,pm-semantic-map.json}` | `25` |
| `tools/design-coverage-check.sh` | `25` |
| `os/libs/minix-types/src/ipc/{pm,vfs,event,message,notify,sysinfo}.rs` | `06` / `07` / `08` |
| `os/libs/minix-types/src/types/{mproc,clock,endpoint,signal}.rs` | `01` / `22` / `21` |
| `os/libs/minix-sys/src/syscall.rs` | `24` |
| `minix/lib/libsys/sched_start.c` | `19` |
| `minix/lib/libsys/asynsend.c` | `07` / `08` |
| `minix/lib/libsys/{getuptime.c,clock_time.c}` | `21` |
| `minix/lib/libsys/sef_signal.c` | `15` |
| `os/servers/pm/src/**`（37 .rs） | 各篇 §事实底线已逐篇列出 |

### 9.5 结论与待裁决问题

**结论：本蓝图已完成**（四类机械检查全部通过，G1-G9 全部通过）。

**待用户裁决的三个问题**：

1. **篇数从 22 增到 26** —— 新增的 5 篇里，`23`（失败模型）与 `24`（对端协议）是横切篇、`25`（测试与台账）是可跳读支线。若希望控制篇数，可把 `23` 并入 `05`（代价：`05` 会同时承载"循环骨架"与"失败哲学"两个语义，违反单篇单语义），或把 `25` 的内容退回 `plan.md`/`todo.md`（代价：00 与各篇 §5 的腐烂问题无解）。**推荐维持 26 篇**。
2. **`06-pm-wire-format` 与 edge E7 的边界** —— 本篇只做"消费端索引 + 解码纪律"，不重新设计 wire 类型；若 E7 在本 stage 之前落地，本篇需改为"委托 wire 类型"。**推荐按本蓝图先写消费端**，E7 落地后调用点零改动。
3. **`25-pm-test-and-status` 是否算正式文档** —— 它承载全部易腐烂数字。若项目规范不允许正式目录出现易腐烂内容，可把它降级为 `plan.md` 的一节（代价：`plan.md` 会膨胀约 200 行，且阅读路径断裂）。**推荐留在正式目录但标为可跳读支线**。




