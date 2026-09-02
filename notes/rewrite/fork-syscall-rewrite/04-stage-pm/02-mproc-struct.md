# 02-mproc-struct: 进程结构——mproc 的字段语义与状态分层

> **状态**: 完整版（2026-08-17 首版）
> **定位**: 阶段 1 启动与进程模型——`sef_cb_init_fresh` 第 1 步（`minix3/minix/servers/pm/main.c:146-152`）建立的数据结构
> **源码**: `minix3/minix/servers/pm/mproc.h`（struct mproc + 19 flag 位 + mpsigact）
> **Rust 模块**: `os/servers/pm/src/mproc/`（mproc.rs、lifecycle.rs、block.rs、wait.rs、guardianship.rs、trace.rs、signal.rs、credentials.rs、context.rs、constants.rs、table.rs、fork.rs）
> **前置依赖**: 01-pm-init-main（启动链第 1 步）
> **不覆盖（移交）**: 表操作/slot 分配/endpoint 验证（03）、主循环与分发（04）、状态机流转（09/10/11~13）

---

## 1. 概念：PM 的进程结构为什么长这样

### 1.0 章节引言

PM 是 Minix3 的"进程语义权威"——它拥有进程的生命周期、信号、凭证语义。
但 PM 并不持有进程的全部状态：内核持有调度与寄存器、VM 持有地址空间、
VFS 持有文件描述符。因此 PM 需要一张**自己的进程表**，而这张表的每一行
（`struct mproc`）就是本档的主题。

本档回答三个问题：

1. **为什么** Minix3 要把进程表拆成多份副本、PM 单独持有一份？
2. **为什么** `mproc` 的单个槽位是一坨约 60 字段的巨型结构，其中约 80% 的体积
   还要被单独隔离出去？
3. **为什么** 19 个正交 flag 位必须被 Rust 重表达为"互斥枚举 + 组合子"的分层模型？

### 1.1 多副本进程表：微内核信任边界的自然产物

Minix3 的进程状态分布在四个服务中，每个服务维护一张按进程索引的表，
通过 **endpoint**（`_ENDPOINT(generation, slot)`）互相引用：

| 表 | 持有者 | 内容 |
|----|--------|------|
| `proc[]` | 内核 | 调度、IPC、寄存器保存、地址空间（内核视角） |
| `vmproc[]` | VM | 虚拟内存、页表、region |
| `fproc[]` | VFS | 文件描述符、当前目录、根目录 |
| `mproc[]` | PM | 生命周期、信号、凭证、等待、父子关系 |

这不是设计冗余，而是**微内核信任边界**的直接结果：内核把"进程管理语义"
交给用户态 PM，把"文件语义"交给 VFS，把"内存语义"交给 VM——每一份副本
对应一个服务对进程的视图。四张表靠 endpoint 对齐，不需要共享内存。

> **与 Monolithic 的对比**：Linux 把这一切塞进单个 `task_struct`
> （fd 表、cred、信号、调度字段都在一个结构里）；Redox 把进程状态集中在
> 内核 `Context`。Minix3 的分布式表是"多权威"架构（详见 §3.8）。

### 1.2 单槽巨型结构的问题

`struct mproc`（mproc.h:24-83）约 60 字段，语义上分为四族：

- **身份**：pid、endpoint、进程组、名字、父、tracer；
- **状态**：19 个 flag 位 + 退出状态 + 等待目标；
- **资源**：uid/gid、7 个信号集合、定时器、nice、调度器、时间统计；
- **IPC**：事件订阅者、回复消息、执行帧。

C 代码把所有字段平铺在一个结构里，字段之间靠注释和命名约定区分。
两个实际问题随之而来：

1. **组合爆炸**：19 个正交 flag 位任意组合（mproc.h:86-104），哪些组合合法
   只写在注释和 assert 里（如 `signal.c:46` 的
   `assert(!(mp->mp_flags & (PROC_STOPPED | VFS_CALL | UNPAUSED | EVENT_CALL)))`）。
2. **体积膨胀**：信号动作表 `mpsigact[NR_PROCS][_NSIG]` 占 per-process state
   约 80%（mproc.h:16-20 注释明言），如果不隔离，MIB 服务会被迫拉入整个表。

### 1.3 字段族的自然分组：分层模型的依据

字段分层的依据不是风格偏好，而是**变更频率与组合规则**：

| 族 | 变更频率 | 组合规则 |
|----|---------|---------|
| 身份 | 几乎只写一次（创建时） | 无组合，纯数据 |
| 状态 | 高频变迁 | **互斥**（如 ZOMBIE vs TOLD_PARENT）与**可组合**（如 EXITING\|VFS_CALL）两类 |
| 资源 | 随功能模块演进 | 各子模块内部一致（信号集合、凭证、定时器） |
| IPC | 随协议走 | 与具体调用绑定（VFS 调用/事件订阅/延迟信号） |

"互斥"与"可组合"的区分是 Rust 分层的核心：互斥维度用**枚举**
（一个进程只能处于一个生命周期状态），可组合维度用**组合子**
（阻塞可以叠加在 Running/Exiting 之上）。这就是 plan.md §4 ARCH A-1/A-2
的落地。

### 1.4 状态为什么必须分层

看三个 C 中的合法组合：

- `EXITING | VFS_CALL`：进程退出中，VFS 还没完成清理（forkexit.c:374 保留 VFS_CALL）；
- `IN_USE | PROC_STOPPED`：运行中被信号停止（stop_proc 置位）；
- `WAITING`：**父进程**在等子进程（不是子进程自身状态！）。

再看两个非法组合（C 靠注释和流程保证）：

- `ZOMBIE | TOLD_PARENT`：mproc.h:92 注释 "parent wait() completed, ZOMBIE off"；
- `EXITING | ZOMBIE`：退出与僵尸互斥。

C 的 19 位正交模型无法表达"这些组合不可能"。Rust 用互斥枚举消灭非法状态、
用结构体组合子显式允许合法组合——把运行时断言变成编译期事实。

### 1.5 独立 mpsigact 表：80% 体积的隔离

`mpsigact[NR_PROCS][_NSIG]`（mproc.h:21-22）是独立于 `mproc[]` 的静态大表，
每个进程槽的 `mp_sigact` 只是指向自己那一行的指针。动机（mproc.h:16-20 注释）：

> The per-process sigaction structures are stored outside of the mproc table,
> so that the MIB service can avoid pulling them in, as they account for
> roughly 80% of the per-process state.

MIB（管理信息库）服务按槽位遍历进程表时不需要信号动作细节；把它们
隔离出去使 MIB 的映射体积缩小约 80%。fork 时指针重指 + `memcpy`
（forkexit.c:88-89）——语义是"子进程获得父进程动作表的深拷贝"。

### 1.6 本章小结

- mproc 是 PM 对进程的私有视图，与内核/VM/VFS 三表靠 endpoint 关联；
- 单槽巨型结构的问题 = 组合无约束 + 体积无隔离；
- 字段族的分组（身份/状态/资源/IPC）直接决定 Rust 分层；
- 状态分层的判据是"互斥用枚举、可组合用组合子"；
- mpsigact 独立表是"体积隔离"的 C 先例，Rust 用 `Box` 保持同构。

---

## 2. C 源码分析：mproc.h 逐字段

> 本档全部行号均 grep/awk 实证（2026-08-17）。字段族分组与 §1.3 一致。

### 2.1 struct mproc 总览（mproc.h:24-83）

```c
EXTERN struct mproc {
  char mp_exitstatus;		/* storage for status when process exits */
  char mp_sigstatus;		/* storage for signal # for killed procs */
  char mp_eventsub;		/* process event subscriber, or NO_EVENTSUB */
  pid_t mp_pid;			/* process id */
  endpoint_t mp_endpoint;	/* kernel endpoint id */
  pid_t mp_procgrp;		/* pid of process group (used for signals) */
  pid_t mp_wpid;		/* pid this process is waiting for */
  vir_bytes mp_waddr;		/* struct rusage address while waiting */
  int mp_parent;		/* index of parent process */
  int mp_tracer;		/* index of tracer process, or NO_TRACER */

  /* Child user and system times. Accounting done on child exit. */
  clock_t mp_child_utime;	/* cumulative user time of children */
  clock_t mp_child_stime;	/* cumulative sys time of children */

  /* Real, effective, and saved user and group IDs. */
  uid_t mp_realuid; uid_t mp_effuid; uid_t mp_svuid;
  gid_t mp_realgid; gid_t mp_effgid; gid_t mp_svgid;

  /* Supplemental groups. */
  int mp_ngroups;
  gid_t mp_sgroups[NGROUPS_MAX];

  /* Signal handling information. */
  sigset_t mp_ignore; sigset_t mp_catch; sigset_t mp_sigmask;
  sigset_t mp_sigmask2; sigset_t mp_sigpending; sigset_t mp_ksigpending;
  sigset_t mp_sigtrace;
  ixfer_sigaction *mp_sigact;	/* pointer into mpsigact */
  vir_bytes mp_sigreturn;
  minix_timer_t mp_timer;
  clock_t mp_interval[NR_ITIMERS];
  clock_t mp_started;

  unsigned mp_flags;		/* flag bits */
  unsigned mp_trace_flags;	/* trace options */
  message mp_reply;		/* reply message to be sent to one */

  vir_bytes mp_frame_addr;	/* ptr to proc's initial stack arguments */
  size_t mp_frame_len;		/* size of proc's initial stack arguments */

  signed int mp_nice;
  endpoint_t mp_scheduler;	/* scheduler endpoint id */
  char mp_name[PROC_NAME_LEN];
  int mp_magic;			/* sanity check, MP_MAGIC */
} mproc[NR_PROCS];
```

按字段族归类：

| 族 | 字段 | 语义 |
|----|------|------|
| 退出/等待 | `mp_exitstatus`/`mp_sigstatus` | 退出码/致死信号（char） |
| 身份 | `mp_pid`/`mp_endpoint`/`mp_procgrp`/`mp_name` | 进程号/内核 endpoint/进程组/名字 |
| 等待 | `mp_wpid`/`mp_waddr` | wait4 目标 pid / rusage 地址 |
| 监护 | `mp_parent`/`mp_tracer` | 父/跟踪者（**槽索引**，不是 pid） |
| 时间统计 | `mp_child_utime`/`mp_child_stime` | 子进程累计用户/系统时间（子退出时记账） |
| 凭证 | `mp_realuid`/`mp_effuid`/`mp_svuid` + gid 三件 + `mp_ngroups`/`mp_sgroups[16]` | 真实/有效/保存 uid+gid、补充组 |
| 信号 | `mp_ignore`/`mp_catch`/`mp_sigmask`/`mp_sigmask2`/`mp_sigpending`/`mp_ksigpending`/`mp_sigtrace` + `mp_sigact`/`mp_sigreturn` | 7 个集合 + 动作表指针 + sigreturn 地址 |
| 定时 | `mp_timer`/`mp_interval[3]`/`mp_started` | watchdog 定时器/间隔/启动时间 |
| 状态 | `mp_flags`/`mp_trace_flags` | 19 位正交位（§2.2）/跟踪选项 |
| IPC | `mp_eventsub`/`mp_reply`/`mp_frame_addr`/`mp_frame_len` | 事件订阅者/回复消息/执行帧（procfs 用） |
| 调度 | `mp_nice`/`mp_scheduler` | nice 值/用户态调度器 endpoint |
| 校验 | `mp_magic` | `MP_MAGIC 0xC0FFEE0`（mproc.h:106） |

### 2.2 19 个 flag 位（mproc.h:86-104）

```c
#define IN_USE		0x00001	/* set when 'mproc' slot in use */
#define WAITING		0x00002	/* set by WAIT4 system call */
#define ZOMBIE		0x00004	/* waiting for parent to issue WAIT4 call */
#define PROC_STOPPED	0x00008	/* process is stopped in the kernel */
#define ALARM_ON	0x00010	/* set when SIGALRM timer started */
#define EXITING		0x00020	/* set by EXIT, process is now exiting */
#define TOLD_PARENT	0x00040	/* parent wait() completed, ZOMBIE off */
#define TRACE_STOPPED	0x00080	/* set if process stopped for tracing */
#define SIGSUSPENDED	0x00100	/* set by SIGSUSPEND system call */
#define VFS_CALL       	0x00400	/* set if waiting for VFS (normal calls) */
#define NEW_PARENT	0x00800	/* process's parent changed during VFS call */
#define UNPAUSED	0x01000	/* VFS has replied to unpause request */
#define PRIV_PROC	0x02000	/* system process, special privileges */
#define PARTIAL_EXEC	0x04000	/* process got a new map but no content */
#define TRACE_EXIT	0x08000	/* tracer is forcing this process to exit */
#define TRACE_ZOMBIE	0x10000	/* waiting for tracer to issue WAIT4 call */
#define DELAY_CALL	0x20000	/* waiting for call before sending signal */
#define TAINTED		0x40000 /* process is 'tainted' */
#define EVENT_CALL	0x80000	/* waiting for process event subscriber */
```

逐位归类（互斥 = 任意时刻至多一个；组合 = 可叠加在其他状态上）：

| 位 | 值 | 类别 | 语义要点 |
|----|-----|------|---------|
| IN_USE | 0x00001 | 互斥（槽占用） | slot 在用的总开关 |
| EXITING | 0x00020 | 互斥（生命周期） | 退出中，可组合 VFS_CALL/PROC_STOPPED（forkexit.c:374） |
| ZOMBIE | 0x00004 | 互斥（生命周期） | 等父 wait4 |
| TRACE_ZOMBIE | 0x10000 | 互斥（生命周期） | 等 tracer wait4 |
| TOLD_PARENT | 0x00040 | 互斥（生命周期） | 父已完成 wait，ZOMBIE 已清 |
| WAITING | 0x00002 | 组合（**父进程侧**） | wait4 挂起的是父进程 |
| PROC_STOPPED | 0x00008 | 组合（阻塞） | 内核停止（信号/跟踪），可叠加 Running/Exiting |
| VFS_CALL | 0x00400 | 组合（阻塞） | 等待 VFS 回复（tell_vfs 置位，utility.c:131-138） |
| EVENT_CALL | 0x80000 | 组合（阻塞） | 等待事件订阅者回复（event.c:349） |
| DELAY_CALL | 0x20000 | 组合（阻塞） | 停止请求遇 EBUSY，等内核 SIGSNDELAY（signal.c:250-255/344-351） |
| UNPAUSED | 0x01000 | 组合（阻塞） | VFS 已回复 unpause 请求（main.c:410） |
| NEW_PARENT | 0x00800 | 组合（**仅随 VFS_CALL**） | VFS 调用期间父被收养，回复须交新父（forkexit.c:402-404） |
| TRACE_STOPPED | 0x00080 | 组合（跟踪） | 为调试器停止（trace_stop） |
| TRACE_EXIT | 0x08000 | 组合（跟踪） | tracer 强制退出 |
| SIGSUSPENDED | 0x00100 | 组合（信号） | sigsuspend 挂起中 |
| PRIV_PROC | 0x02000 | 属性（特权） | 系统进程 |
| ALARM_ON | 0x00010 | 属性（定时） | SIGALRM 定时器已启动（alarm.c:306-309） |
| PARTIAL_EXEC | 0x04000 | 属性（exec） | 新映射已建、内容未加载（exec.c:120/163） |
| TAINTED | 0x40000 | 属性（凭证/exec） | 进程被"污染"（exec.c:84-108、getset.c:81） |

> **关键不变量**（VFS/EVENT 族的共同处理路径）：`VFS_CALL` 与 `EVENT_CALL`
> 是互斥的两种"调用阻塞"，但 C 在判别"能否立即处理信号/清理"时总是一起检查
> （如 `signal.c:279` 的 `(VFS_CALL | EVENT_CALL | EXITING)`）。Rust 用
> `Option<IpcBlockReason>` 的互斥枚举表达"至多一种调用阻塞"，见 §3.3。

### 2.3 信号位图与 sigaction

- `sigset_t` 是 128 位结构 `__uint32_t __bits[4]`（`minix3/sys/sys/sigtypes.h`）；
  `__sigismember(s, n)` 取 bit `(n-1)`——**信号号 1 对应 bit 0**。
  `_NSIG = 64`（`minix3/sys/sys/signal.h:45`），实际只用低 64 位。
- `struct sigaction`（signal.h:126-141）：`union { handler, sigaction }` +
  `sigset_t sa_mask` + `int sa_flags`；`SIG_DFL = 0`、`SIG_IGN = 1`。
- `do_sigaction`（signal.c:40-86）在 67-78 行维护 `mp_ignore`/`mp_catch`：
  `SIG_IGN → ignore |= bit`；`SIG_DFL → 两集都清`；用户 handler → `catch |= bit`。
  消费方：`exec.c:179-180`（exec 成功后重置 caught 位 + 动作表）、
  `signal.c:483-509`（check_sig 先查 ignore、再查 mask、再查 catch）。

### 2.4 初始化契约（main.c:146-152）

```c
  /* Initialize process table, including timers. */
  for (rmp=&mproc[0]; rmp<&mproc[NR_PROCS]; rmp++) {
	init_timer(&rmp->mp_timer);
	rmp->mp_magic = MP_MAGIC;
	rmp->mp_sigact = mpsigact[rmp - mproc];
	rmp->mp_eventsub = NO_EVENTSUB;
  }
```

四件事：定时器初始化、魔数写入、sigact 指针定位、事件订阅者置 `NO_EVENTSUB`。
其余字段（pid/endpoint/flags/name/uid…）在 boot image 填充（main.c:177-229，
见 01-pm-init-main）与后续系统调用中按需写入。

**MP_MAGIC 是跨服务表校验令牌**：PM 只在初始化时写入（main.c:149），
自己从不读取；真正读它的是 **MIB 服务**——MIB 通过
`getsysinfo(SI_PROC_TAB)` 拉取 PM 的整张 mproc 表，逐槽校验
`mp_magic != MP_MAGIC` 判定表槽是否有效（`minix3/minix/servers/mib/proc.c:90,98`）。
这正是 mpsigact 独立存放（§1.5）的配套设计：外部读者只拉瘦表 + 魔数校验，
不拉 80% 的信号动作数据。Rust 进程结构内部不需要魔数（类型系统保证槽位合法，
ARCH A-11）；MIB 的跨服务读表协议归 MIB 服务文档（非 PM 范围，DEFERRED）。

### 2.5 fork 的字段继承（forkexit.c）

普通 fork（forkexit.c:88-108）：

```c
  *rmc = *rmp;			/* copy parent's process slot to child's */
  rmc->mp_sigact = mpsigact[next_child];	/* restore mp_sigact ptr */
  memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child]));
  rmc->mp_parent = who_p;
  if (!(rmc->mp_trace_flags & TO_TRACEFORK)) {
	rmc->mp_tracer = NO_TRACER;
	rmc->mp_trace_flags = 0;
	sigemptyset(&rmc->mp_sigtrace);
  }
  if (rmc->mp_flags & PRIV_PROC) {	/* 系统进程的普通 fork */
	assert(rmc->mp_scheduler == NONE);
	rmc->mp_scheduler = SCHED_PROC_NR;
  }
  rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);	/* 只继承这三个位 */
  rmc->mp_child_utime = 0; rmc->mp_child_stime = 0;
  rmc->mp_exitstatus = 0; rmc->mp_sigstatus = 0;
  rmc->mp_endpoint = child_ep;
  for (i = 0; i < NR_ITIMERS; i++) rmc->mp_interval[i] = 0;
  rmc->mp_started = getticks();
```

要点：

1. **whole-slot copy**：`*rmc = *rmp` 全量复制，再逐项修正——这是 C 的
   "显式逐字段"的反面（隐藏复制）；Rust 用 `Process::fork_from` 逐字段构造
   （§4.6）。
2. **信号动作表深拷贝**：指针重指 + memcpy（forkexit.c:88-89）。
3. **PRIV_PROC 不继承**：普通 fork 的子进程永远是用户进程；但系统父进程的
   子进程调度器改为 `SCHED_PROC_NR`（forkexit.c:96-100，RS 派生恢复脚本场景）。
4. **DELAY_CALL 继承是伪语义**：见 §3.5 的不可达论证。

srv_fork（forkexit.c:191-200）：掩码为 `(IN_USE|PRIV_PROC|DELAY_CALL)`
——系统进程的 fork 保留 PRIV_PROC、**不**保留 TAINTED（新服务从清白状态开始），
uid/gid 直接来自消息参数。完整流程归 08-pm-srv-fork.md。

### 2.6 本档覆盖的符号清单

| 符号 | 位置 | 本档覆盖 |
|------|------|---------|
| `struct mproc` 全部字段 | mproc.h:24-83 | ✅ §2.1 |
| 19 个 flag 位 | mproc.h:86-104 | ✅ §2.2 |
| `MP_MAGIC` | mproc.h:106 | ✅ §2.4 |
| `mpsigact`/`mp_sigact` | mproc.h:21-22/57 | ✅ §1.5/§2.4 |
| 初始化循环 | main.c:146-152 | ✅ §2.4 |
| fork 继承掩码 | forkexit.c:88-108/191-200 | ✅ §2.5 |
| 常量（NR_PIDS/NO_PID/INIT_PID/NO_TRACER/NO_EVENTSUB） | const.h:3-13 | ✅ 交 99/03 深化 |

---

## 3. Rust 设计决策：用类型系统重表达状态分层

### 3.1 D1：`Process` 四层分层模型（ARCH A-1）

- **C**：`struct mproc` 单结构约 60 字段（mproc.h:24-83）。
- **Rust**（`os/servers/pm/src/mproc/mproc.rs`）：

```rust
pub struct Process {
    pub identity: ProcessIdentity,   // mp_pid/endpoint/procgrp/name
    pub state: ProcessState,         // lifecycle/block/wait/guardianship/trace
    pub resources: ProcessResources, // privilege/signals/timer/nice/...
    pub ipc: ProcessIpc,             // reply/event_subscriber/frame
}
```

- **为什么**：字段族的变更频率与组合规则不同（§1.3）。分层让每个子结构
  有自己的不变量：`ProcessState` 的内部枚举保证状态合法，`ProcessResources`
  按功能模块（凭证/信号/定时）演进互不干扰，`ProcessIpc` 随协议走。
- **替代方案**：保持单结构 + 裸字段（translate）。否决——19 位正交模型的
  非法组合在 C 靠 assert，在 Rust 可以直接消灭。
- **行为契约**：`Process::default()` = 全空（`Lifecycle::Unused`、空位图、
  `pid=0`=NO_PID、`ipc.reply=None`）；`Process::new(index, pid)` 只设身份，
  用于测试构造；表槽由 `ProcTable`（03）管理。

### 3.2 D2：19 flags → 状态机枚举 + 组合子逐位归类（ARCH A-2）

逐位映射（16/19 完全归类，3/19 保留）：

| C 位 | Rust 归属 | 建模 |
|------|----------|------|
| IN_USE | `Lifecycle::is_in_use()`（`!Unused`） | 枚举隐含 |
| EXITING | `Lifecycle::Exiting { exit_code, sig_status }` | 互斥枚举 |
| ZOMBIE | `Lifecycle::Zombie { .. }` | 互斥枚举 |
| TRACE_ZOMBIE | `Lifecycle::TraceZombie { .. }` | 互斥枚举 |
| TOLD_PARENT | `Lifecycle::ToldParent { .. }` | 互斥枚举 |
| WAITING | `WaitState.waiting`（**父进程槽**） | 组合子字段 |
| PROC_STOPPED | `BlockState.stopped` | 组合子字段 |
| VFS_CALL | `BlockState.ipc_blocked = Some(VfsCall { .. })` | 互斥枚举变体 |
| EVENT_CALL | `BlockState.ipc_blocked = Some(EventCall)` | 互斥枚举变体 |
| DELAY_CALL | `BlockState.ipc_blocked = Some(DelayedSignal)` | 互斥枚举变体 |
| NEW_PARENT | `VfsCall { reply_to_new_parent: true }` | 变体载荷（D3） |
| UNPAUSED | `BlockState.unpaused` | 组合子字段 |
| TRACE_STOPPED | `TraceState.stopped` | 组合子字段 |
| TRACE_EXIT | `Guardianship::Traced { trace_exit, .. }` | 枚举变体载荷 |
| SIGSUSPENDED | `SignalState.suspended` | 组合子字段 |
| PRIV_PROC | `Privilege::Kernel` | 属性枚举 |
| ALARM_ON | `RemainingFlags::ALARM_ON`（0x10） | 保留（归 14） |
| PARTIAL_EXEC | `RemainingFlags::PARTIAL_EXEC`（0x4000） | 保留（归 17） |
| TAINTED | `RemainingFlags::TAINTED`（0x40000） | 保留（归 15/17） |

- **为什么**：互斥维度（生命周期 5 位）用枚举消灭"ZOMBIE|TOLD_PARENT"这类
  非法组合；可组合维度（阻塞/跟踪/等待）用结构体组合子显式允许
  "EXITING|VFS_CALL"这类合法组合；纯属性位（PRIV_PROC/ALARM_ON/PARTIAL_EXEC/
  TAINTED）归入属性枚举或 `RemainingFlags`。
- **行为契约**：`RemainingFlags` 的位值与 C 完全一致（0x00010/0x04000/0x40000），
  且**不再包含** `VFS_CALL`/`EVENT_CALL`/`DELAY_CALL`/`NEW_PARENT`——它们
  全部由 `BlockState` 单一建模（single-truth，消除重复表示）。

### 3.3 D3：`IpcBlockReason::VfsCall { reply_to_new_parent }`——把 flag 组合变成类型

C 中 `NEW_PARENT`（mproc.h:96）有两个事实：

1. **只在 VFS_CALL 置位时被设置**（forkexit.c:402-404：`if (rmp->mp_flags & VFS_CALL) rmp->mp_flags |= NEW_PARENT;`）；
2. **与 VFS_CALL 同时清除**（main.c:327-328）。

即"NEW_PARENT 是 VFS_CALL 的修饰信息"——C 用两个正交位表达，Rust 合并为：

```rust
pub enum IpcBlockReason {
    VfsCall { reply_to_new_parent: bool },  // VFS_CALL (+ NEW_PARENT)
    EventCall,                              // EVENT_CALL
    DelayedSignal,                          // DELAY_CALL
}
```

- **为什么**：C 的不变量（NEW_PARENT ⇒ VFS_CALL）在 Rust 里变成
  "非法组合不可表示"——`Some(VfsCall { .. })` 存在则载荷必伴随，
  `reply_to_new_parent` 无处可依附时类型系统直接拒绝。
- **取舍**：C 的 `signal.c`/`main.c` 用位掩码一次检查 `(VFS_CALL|EVENT_CALL|EXITING)`
  （signal.c:279/425/672/693），Rust 消费方改为 `matches!(ipc_blocked, None)`
  + `lifecycle.is_exiting()`——语义等价，表达更明确。
- **行为契约**：`is_vfs_blocked()`/`is_event_blocked()` 匹配变体；
  `Display` 输出 `vfs_blocked(new_parent)` 等；fork 后子进程 `ipc_blocked = None`。

### 3.4 D4：`SignalState` 补齐 `ignored`/`caught`（mp_ignore/mp_catch）

原 Rust `SignalState` 漏映射 C 的两个集合。补齐后（`os/servers/pm/src/mproc/signal.rs`）：

```rust
pub struct SignalState {
    pub ignored: SigSet,        // mp_ignore
    pub caught: SigSet,         // mp_catch
    pub mask: SigSet,           // mp_sigmask
    pub mask_saved: SigSet,     // mp_sigmask2
    pub pending: SigSet,        // mp_sigpending
    pub kernel_pending: SigSet, // mp_ksigpending
    pub trace_mask: SigSet,     // mp_sigtrace
    pub suspended: bool,        // SIGSUSPENDED
    pub sigreturn_addr: VirBytes, // mp_sigreturn
    pub actions: Box<[SigAction; _NSIG]>, // mp_sigact 指向的整行
}
```

- **为什么**：`ignored`/`caught` 是信号的**处置分类**（忽略/捕获），与
  `mask`（阻塞）、`pending`（挂起）正交；`do_sigaction`（signal.c:67-78）维护、
  `exec.c:179-180` 与 `check_sig`（signal.c:483-509）消费——结构缺了它们，
  信号状态机（11~13）无法落地。
- **位语义**：`SigSet = u64`，bit `signo - 1`（信号 1 → bit 0），与 C
  `__sigismember(s, n)` 的 bit `(n-1)` 完全一致（§2.3）。
- **行为契约**：默认全 0；`is_ignored(signo)`/`is_caught(signo)` 与
  `is_blocked` 同边界（0 与 >64 返回 false）；`SigAction.sa_handler`
  `0=SIG_DFL`、`1=SIG_IGN`、其他=用户 handler 地址。

### 3.5 D5：`RemainingFlags` 收窄 + fork 继承修正

**收窄**：删除原 `RemainingFlags` 中重复建模的 `DELAY_CALL = 0x00040` 与
`VFS_CALL = 0x00200`——两者的语义归属 `BlockState::ipc_blocked`（D3），
且原值与 C 不符（C 是 0x20000 / 0x00400）。保留的 3 位值全部对齐 C。

**fork 继承**（`os/servers/pm/src/mproc/fork.rs` 的 `Process::fork_from`）：

- 子进程 `flags` 仅继承 `TAINTED`（forkexit.c:106）；
- 子进程 `block = BlockState::default()`，即 `ipc_blocked = None`；
- 子进程 `privilege = Privilege::User(..)`（普通 fork 不继承 PRIV_PROC）；
- 调度器：系统父进程（PRIV_PROC）的普通 fork 子进程 → `Endpoint::SCHED`
  （forkexit.c:96-100）；其余继承父进程。

**DELAY_CALL 不可达论证**（为什么显式重置而非复刻 C 的继承）：

1. `DELAY_CALL` 由 `stop_proc` 在 `sys_delay_stop` 返回 `EBUSY` 时置位
   （signal.c:250-255）——此时进程处于内核 mid-send；
2. 清除只在内核发送 `SIGSNDELAY` 通知后（signal.c:344-351）；
3. 置位→清除窗口内，进程被内核阻塞在消息发送中，**不能执行用户态代码，
   因而不能发起 fork 系统调用**；
4. 所以 C 掩码里的 `DELAY_CALL` 继承（forkexit.c:106）在可达状态集合中
   恒为 0，是 whole-slot copy 的伪语义——minix-rs 显式重置并文档化，
   不复制不可达行为。

### 3.6 D6：`actions: Box<[SigAction; _NSIG]>`——mpsigact 的 Rust 同构

- **C**：`mpsigact[NR_PROCS][_NSIG]` 独立静态表（§1.5），`mp_sigact` 是行指针。
- **Rust**：`SignalState.actions: Box<[SigAction; _NSIG]>`——每进程私有、
  堆分配（`os/servers/pm/src/lib.rs:31 extern crate alloc`），fork 时克隆
  （对应 C 的 memcpy）。
- **为什么**：语义等价（独立存储 + 深拷贝）、无指针（`mp_sigact` 的
  别名问题消失）；`Box` 保持 `Process` 结构小，`ProcTable` 静态数组维持
  ~22.5KB（`table.rs` 注释），与 C"MIB 避免拉入"的体积隔离动机同构。
- **行为契约**：`SigAction { sa_handler: usize, sa_mask: u64, sa_flags: i32 }`
  （`#[repr(C)]`，但仅语义级对齐——C 的 `sa_mask` 是 128 位，Rust 用 64 位
  u64 覆盖 `_NSIG=64`，无 FFI 拷贝，A-11）。

### 3.7 ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-1 mproc 分层 | `Process` 四层（D1） | `mproc.rs` 注释 + 本档 §3.1 + plan.md §4 |
| A-2 flag → 枚举 | 19 位逐位归类（D2/D3/D4/D5） | `lifecycle.rs`/`block.rs`/`signal.rs` 注释 + 本档 §3.2 + plan.md §4 |
| A-11 64 位类型 | `Pid`/`Endpoint`/`UserSlot`/`SigSet=u64`（D4/D6） | `signal.rs` 位语义注释 + 本档 §3.4/§3.6 + plan.md §4 |
| A-3 全局 → 显式结构 | `PmContext`（继承自 01 先例） | `context.rs` + 本档 §4.1 + plan.md §4 |

> A-12（双监护）本档落地结构（`Guardianship` 枚举），`tracer_died`/收养流程归 09/10/18；
> A-13（进程组）本档落地字段（`identity.procgrp`），会话/组语义归 09/15；
> A-4~A-10 不在本档字段范围，随对应文档（04/05/06/13/14/16）落地。

### 3.8 对照：Redox / Linux 的进程状态模型

本档的核心机制——"进程状态分布多表 + 单槽巨型结构 + 正交位"——是 Minix3
微内核架构的产物。对照主流 OS 的建模，能看出哪些是必然约束、哪些是 Rust
可改善的点：

| 维度 | Redox | Linux | Minix3（本档） |
|------|-------|-------|---------------|
| 进程表归属 | 内核单一 `Context` 表 | 内核单一 `task_struct` | **PM 语义权威 + 四表副本**（§1.1） |
| 状态表达 | `Status` 枚举：`Runnable` / `Blocked` / `HardBlocked { reason }` / `Dead { excp }` | `task_struct.state`（`TASK_RUNNING`/`TASK_INTERRUPTIBLE`/`TASK_ZOMBIE`…）+ 独立位标志 | **19 正交位任意组合** |
| 阻塞细节 | `HardBlockedReason`：`Stopped` / `AwaitingMmap` / `NotYetStarted` | 等待队列 + `state` 位 | `PROC_STOPPED`/`VFS_CALL`/`EVENT_CALL`/`DELAY_CALL` |
| 结构组织 | `Context` 内聚内核视角（`addr_space`/`files`/`sig` 分字段） | `task_struct` 巨型结构（fd/cred/signal 全挂） | `mproc` 巨型结构 + `mpsigact` 独立表 |
| 组合约束 | 枚举 + `status_reason: &'static str` | 位标志 + 内核流程约定 | assert + 流程约定 |

**对照结论**：

1. **Redox 的 `Status` 枚举与 minix-rs 的 `Lifecycle` 同思路**——都是把
   "互斥的主状态"枚举化。差异在"可组合的从状态"：Redox 用 `HardBlocked`
   的 reason 载荷（`Stopped`/`AwaitingMmap`），minix-rs 用
   `BlockState` 组合子（`stopped`/`ipc_blocked`/`unpaused` 独立字段）。
   Redox 的单枚举更紧凑，但"一个进程同时 stopped 且 awaiting-mmap"这类
   组合需要嵌套 reason；minix-rs 的组合子允许 `stopped` 与
   `ipc_blocked` 并存（对应 C 的 `PROC_STOPPED | VFS_CALL`，signal.c:279 场景），
   语义粒度更细。
2. **Linux 的 `task_struct` 是"巨型结构"路线的极致**——Minix3 的 `mproc`
   是其微内核版；两者都靠流程/注释约束状态组合。minix-rs 的分层是对
   "C 平铺结构"的直接回应：把 Linux/Redox 都用过的"结构内部再组织"
   （Linux 的 `cred`/`signal` 子结构、Redox 的 `addr_space`/`files`/`sig`）
   用 Rust 类型固化为不变量。
3. **mpsigact 隔离与 Redox 的 `sig: Option<SignalState>` 同理**——都是
   "信号细节不内联进主结构"的体积/关注点隔离；minix-rs 的
   `Box<[SigAction; _NSIG]>` 同时满足"独立存储 + 深拷贝语义"。

**最佳实践结论**：三个系统在状态建模上收敛于"互斥主状态枚举化 +
可组合从状态结构化"。minix-rs 的增量是**把 C 依赖 assert 的组合规则
（NEW_PARENT ⇒ VFS_CALL、ZOMBIE ∥ TOLD_PARENT）编码进类型**
（D3 的变体载荷、D2 的互斥枚举），这是类型系统能提供而 C 不能的保证。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/pm/src/mproc/` 的 14 个文件，与字段族一一对应：

| 文件 | 内容 | 对应 C |
|------|------|--------|
| `mproc.rs` | `Process` 四层 + `ProcessIdentity`/`ProcessState`/`ProcessResources`/`ProcessIpc` + `Privilege` + `RemainingFlags` + `MinixTimer` | mproc.h:24-83 |
| `lifecycle.rs` | `Lifecycle` 互斥枚举（6 态） | IN_USE/EXITING/ZOMBIE/TRACE_ZOMBIE/TOLD_PARENT |
| `block.rs` | `BlockState` + `IpcBlockReason` 组合子 | PROC_STOPPED/VFS_CALL/EVENT_CALL/DELAY_CALL/NEW_PARENT/UNPAUSED |
| `wait.rs` | `WaitState` + `WaitTarget`（**父进程槽**） | WAITING/mp_wpid/mp_waddr |
| `guardianship.rs` | `Guardianship` + `TraceOptions` | mp_parent/mp_tracer/mp_trace_flags/TRACE_EXIT |
| `trace.rs` | `TraceState` | TRACE_STOPPED |
| `signal.rs` | `SignalState` + `SigAction` + `SigSet` | 信号 7 集 + mpsigact + mp_sigreturn |
| `credentials.rs` | `Credentials` + `NGROUPS_MAX` | uid/gid 三件 + sgroups |
| `constants.rs` | `NR_PIDS`/`INIT_PID`/`NO_PID`/`NO_TRACER_INDEX` | const.h |
| `context.rs` | `PmContext` | `mp` 宏（glo.h） |
| `table.rs` | `ProcTable`（数组 + 分配） | `mproc[NR_PROCS]`（03 深化） |
| `pid_gen.rs` | `PidGenerator` | `get_free_pid`（03 深化） |
| `fork.rs` | `do_fork_prepare` + `Process::fork_from` | do_fork（07 深化） |
| `mod.rs` | 模块导出 | — |

### 4.2 逐层映射表（本档核心交付物）

> C 位置全部实证；"对齐"列：✅=语义等价，🔶=架构演进（ARCH）。

| C 字段（mproc.h） | C 位置 | Rust 字段 | 对齐 |
|------------------|--------|-----------|------|
| mp_pid | :28 | `identity.id.pid: Pid` | ✅ |
| mp_endpoint | :29 | `identity.endpoint: Endpoint` | ✅ |
| mp_procgrp | :30 | `identity.procgrp: Pid` | ✅ |
| mp_name | :80 | `identity.name: [u8; 16]` | ✅ |
| mp_parent / mp_tracer | :33-34 | `state.guardianship: Normal{parent}` / `Traced{parent,tracer}` | 🔶 A-2 |
| mp_trace_flags | :67 | `Traced.trace_options: TraceOptions` | ✅ |
| mp_wpid / mp_waddr | :31-32 | `state.wait.target` / `rusage_addr` | ✅ |
| mp_exitstatus / mp_sigstatus | :25-26 | `Lifecycle` 变体载荷 `exit_code`/`sig_status` | 🔶 A-2 |
| mp_flags（19 位） | :86-104 | §3.2 逐位表 | 🔶 A-2 |
| mp_child_utime / stime | :37-38 | `resources.child_utime` / `child_stime` | ✅ |
| mp_realuid/effuid/svuid | :41-43 | `Credentials.user: IdSet<Uid>` | 🔶 A-11 |
| mp_realgid/effgid/svgid | :44-46 | `Credentials.group: IdSet<Gid>` | 🔶 A-11 |
| mp_ngroups / mp_sgroups | :49-50 | `Credentials.ngroups` / `supplemental_groups: [Gid; 16]` | ✅ |
| mp_ignore / mp_catch | :53-54 | `signals.ignored` / `caught` | ✅（D4 补齐） |
| mp_sigmask / mp_sigmask2 | :55-56 | `signals.mask` / `mask_saved` | ✅ |
| mp_sigpending / mp_ksigpending | :57-58 | `signals.pending` / `kernel_pending` | ✅ |
| mp_sigtrace | :59 | `signals.trace_mask` | ✅ |
| mp_sigact | :60 | `signals.actions: Box<[SigAction; _NSIG]>` | 🔶 A-1/D6 |
| mp_sigreturn | :61 | `signals.sigreturn_addr` | ✅ |
| mp_timer + ALARM_ON | :62 + flag | `resources.timer: Option<MinixTimer>` + `flags.ALARM_ON` | ⚠️ 部分（14 折叠） |
| mp_interval[3] | :63 | `resources.intervals: [Clock; 3]` | ✅ |
| mp_started | :64 | `resources.started: Clock` | ✅ |
| mp_eventsub | :27 | `ipc.event_subscriber: Option<UserSlot>` | ✅（None ↔ NO_EVENTSUB） |
| mp_reply | :68 | `ipc.reply: Option<Message>` | ✅ |
| mp_frame_addr / mp_frame_len | :71-72 | `ipc.frame_addr` / `frame_len` | ✅ |
| mp_nice | :75 | `resources.nice: i32` | ✅ |
| mp_scheduler | :78 | `resources.scheduler: Endpoint` | ✅ |
| mp_magic | :82 | （无字段——MIB 跨服务校验令牌，见 §2.4） | 🔶 A-11 |

### 4.3 不变量：哪些组合合法、哪些不可表示

| 组合 | C | Rust |
|------|---|------|
| EXITING + VFS_CALL（退出等 VFS 清理） | 合法（forkexit.c:374） | `Lifecycle::Exiting` + `ipc_blocked = Some(VfsCall)`（显式允许） |
| Running + PROC_STOPPED（被停止） | 合法 | `Lifecycle::Running` + `BlockState.stopped = true`（显式允许） |
| ZOMBIE + TOLD_PARENT | 非法（mproc.h:92 注释） | **不可表示**（互斥枚举） |
| EXITING + ZOMBIE | 非法（流程保证） | **不可表示**（互斥枚举） |
| NEW_PARENT 无 VFS_CALL | 非法（forkexit.c:402-404） | **不可表示**（变体载荷） |
| WAITING 挂子进程 | 非法（语义约束） | `WaitState` 只在父进程槽写（文档约束 + 03 表管理） |

### 4.4 `SigSet` 位语义与 `SigAction`

- `SigSet = u64`，bit `signo - 1`（信号 1 → bit 0，信号 64 → bit 63）。
  C 的 `__sigismember(s, n)` 用 bit `(n-1)`（sigtypes.h）——完全一致。
- `_NSIG = 64`（signal.h:45）。C 的 `sigset_t` 是 128 位（sigtypes.h），
  之所以比 64 宽，是为了容纳**内核信号**：`SIGSNDELAY=70`、
  `SIGKMEM=71`…`SIGKSIG=74`（signal.h:264-277）占据高位的 bit 69-73。
  但 PM 的 per-process 位图只接收**用户信号号**：`check_pending` 的消费
  循环是 `for (i = 1; i < _NSIG; i++)`（signal.c:667），`check_sig` 校验
  `signo < 0 || signo >= _NSIG`（signal.c:582）；内核信号由 signal manager
  路径（`process_ksig`，signal.c:294-378）直接分派，不进位图。
  因此 `SigSet = u64` + bit `signo-1` 覆盖全部 per-process 位图用法（A-11）；
  若未来内核信号需要进位图，`SigSet` 需扩宽——该取舍由 11 文档跟踪。
- `SigAction { sa_handler: usize, sa_mask: u64, sa_flags: i32 }`：
  `sa_handler` 0=SIG_DFL、1=SIG_IGN（signal.h 的 `SIG_DFL 0`/`SIG_IGN 1`）、
  其他=用户 handler 地址；`sa_mask` 在执行 handler 期间自动阻塞的集合；
  `sa_flags` 保留 `SA_*` 语义（12 深化）。

### 4.5 默认值与"空槽"语义

- `Process::default()`：`Lifecycle::Unused`（= IN_USE 清）、`pid=0`（= NO_PID，
  const.h:8）、`procgrp=0`、`endpoint=Endpoint::default()`、信号位图全 0、
  `timer=None`、`flags=RemainingFlags::empty()`、`ipc.reply=None`、
  `event_subscriber=None`（= NO_EVENTSUB，const.h:13）。
- `Guardianship::default()` = `Normal { parent: UserSlot(0) }`——注意
  `NO_TRACER = 0`（const.h:11）是**槽位哨兵**：槽 0 是 PM（系统进程，
  从不调用 PTRACE），不会与真实 tracer 冲突（INIT 在槽 11，不是槽 0）。
  Rust 的 `Guardianship::Normal` 没有 tracer 字段，`tracer()` 返回 `None`，
  语义与 C 的 `mp_tracer == NO_TRACER` 等价，且不再依赖"槽 0 永不
  tracer"的隐式哨兵约定（constants.rs 注释详述）。
- `MP_MAGIC` 无对应字段：它是 MIB 跨服务读表时的校验令牌（§2.4），PM 内部
  从不读取；Rust 进程结构内部由 `UserSlot` 索引 + `ProcTable` 生命周期借用
  保证"访问非法槽"是编译期错误（A-11）。

### 4.6 `Process::fork_from`：逐字段构造替代 whole-slot copy

C 的 `*rmc = *rmp` 是全量复制后修正（forkexit.c:88-108）；Rust 用
`Process::fork_from(&parent, child_index, child_pid, child_endpoint, parent_index)`
逐字段构造（`os/servers/pm/src/mproc/fork.rs`）：

| C 行为 | Rust 行为 |
|--------|----------|
| `*rmc = *rmp` | 逐字段构造（编译器强制检查新字段） |
| `rmc->mp_pid = new_pid` | `identity.id.pid = child_pid` |
| 掩码清 TRACE_EXIT/TRACE_STOPPED/TRACE_ZOMBIE | `guardianship = Normal{..}`、`trace.stopped = false`、`lifecycle = Running` |
| `rmc->mp_flags &= (IN_USE\|DELAY_CALL\|TAINTED)` | `flags` 仅继承 TAINTED；`ipc_blocked = None`（§3.5 论证） |
| `rmc->mp_flags &= ~PRIV_PROC` | `privilege = Privilege::User(..)` |
| `if (PRIV_PROC) scheduler = SCHED_PROC_NR` | 系统父 → `Endpoint::SCHED` |
| `rmc->mp_sigact` 重指 + memcpy | `signals = parent.signals.clone()`（含 Box 深拷贝） |
| `rmc->mp_child_utime/stime = 0`、`mp_interval[i] = 0` | 计数/间隔清零 |
| `rmc->mp_started = getticks()` | `started = getticks()`（当前占位 0，时钟 IPC 归 19） |

顶层 `os/servers/pm/src/fork.rs` 的 `copy_mproc` 同步同一语义：
`FORK_INHERIT_FLAGS = RemainingFlags::TAINTED`，系统父的子进程调度器
`Endpoint::SCHED`。两份 fork 代码的合并归 07-pm-fork.md（本档只保证
**结构继承语义一致**）。

---

## 5. 测试要点

### 5.1 本模块测试（`os/servers/pm/src/mproc/`）

| 测试 | 验证点 | 对应 C |
|------|--------|--------|
| `mproc::tests::test_remaining_flags_bits_match_c` | RemainingFlags 位值 = 0x10/0x4000/0x40000 | mproc.h:90/99/103 |
| `mproc::tests::test_process_default` / `test_process_new` / `test_process_lifecycle` / `test_process_guardianship` / `test_process_stopped` / `test_process_privilege` / `test_layered_structure`（8 个） | 默认空槽、生命周期互斥、guardianship、四层一致性 | mproc.h:86-104 |
| `lifecycle::tests`（5 个） | 5 态互斥枚举 + exit_code + is_zombie/is_exiting | IN_USE:86/ZOMBIE:88/EXITING:91/TOLD_PARENT:92/TRACE_ZOMBIE:101 |
| `block::tests::test_vfs_call_new_parent_payload` | VfsCall{reply_to_new_parent} 变体 | forkexit.c:402-404 |
| `block::tests::test_combined_state`（5 个） | stopped/ipc_blocked/unpaused 并存 | signal.c:279 场景 |
| `signal::tests::test_ignored_caught` | ignored/caught 位语义（bit signo-1） | sigtypes.h |
| `signal::tests::test_is_blocked` / `test_add_pending` / 边界（6 个） | mask/pending/边界（0、>64） | __sigismember |
| `fork::tests::test_fork_flags_inheritance` | 仅 TAINTED 继承 | forkexit.c:106 |
| `fork::tests::test_fork_no_delay_call` | ipc_blocked 不传染子进程 | forkexit.c:106（不可达论证） |
| `fork::tests::test_fork_privilege_scheduler` | 系统父 → User(0,0) + SCHED | forkexit.c:96-100 |
| `wait::tests`（4）/ `guardianship::tests`（5）/ `trace::tests`（2）/ `credentials::tests`（5） | 各组合子默认与访问器 | mproc.h 对应字段 |
| `table::tests`（8）/ `pid_gen::tests`（7） | 表/pid 生成（03 深化） | 03-mproc-table.md |

### 5.2 基线

- `cargo test -p minix-pm --lib`：**91 passed / 0 failed**（2026-08-17 实测；
  上一基线 87 passed，本档新增 4 个：位值对齐、NEW_PARENT 载荷、
  ignored/caught、fork 无 DELAY_CALL）。
- `cargo build -p minix-pm`：通过（新代码 0 新警告；存量警告均为历史文件）。
- 本档改动的测试集中在 mproc 模块；跨模块集成测试随 03/07 落地。

---

## 6. 过渡

本档回答了"PM 启动第 1 步（`main.c:146-152`）建立的数据结构长什么样"：
`Process` 四层模型替代 `struct mproc`，19 个正交位全部有了类型化归属。

在启动时序中，01 完成表初始化（含 magic/sigact/eventsub）与 boot image 填充；
本档定义这些填充写入的**结构形态**；下一步：

- **03-mproc-table.md** 承接：`ProcTable` 的 slot 分配/释放、endpoint 验证、
  PID 生成（`get_free_pid`）——本档的 `Process` 由 03 的表操作实例化；
- **04-ipc-dispatch.md** 承接：主循环如何用 `PmContext` 访问本档结构并分发；
- 状态机流转（09/10/11~13）消费本档的 `Lifecycle`/`BlockState`/`SignalState`。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/plan.md` §2/§3.4/§4（A-1/A-2/A-11）/§5.3/§7.3
- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/01-pm-init-main.md` §2.4 第 1 步（mproc 表初始化）与 §3.8（Redox/Linux 对照风格）
- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/draft/mproc-design.md`（旧主线素材，本档已按新主线重写）
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md`（boot image 与四表）
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/19-syscall-signal.md`（内核信号路径）
- `minix3/minix/servers/pm/mproc.h`、`main.c`、`forkexit.c`、`signal.c`、`exec.c`、`const.h`（ground truth）
- `minix3/sys/sys/sigtypes.h`、`minix3/sys/sys/signal.h`（sigset_t/sigaction 定义）
- `os/servers/pm/src/mproc/`（Rust 实现）
- Redox kernel：`src/context/context.rs`（`Status` 枚举 + `Context` 结构，gitlab.redox-os.org/redox-os/kernel）
