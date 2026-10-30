# 11 · 调度原语：可运行状态与就绪队列的强一致

> **C 真值源**：`minix3/minix/kernel/proc.c`（`enqueue`/`enqueue_head`/`dequeue`/`pick_proc`/`notify_scheduler`/`proc_no_time`）、`minix3/minix/kernel/proc.h`（RTS_SET/RTS_UNSET 宏、可运行判据、`proc_kernel_scheduler`）、`minix3/minix/kernel/system.c:sched_proc`、`minix3/minix/kernel/system/do_schedule.c:do_schedule`、`minix3/minix/kernel/system/do_schedctl.c:do_schedctl`、`minix3/minix/include/minix/config.h`（优先级常量）、`minix3/minix/include/minix/const.h`（`PREEMPTIBLE` 位值）、`minix3/minix/kernel/cpulocals.h`（每 CPU 队列声明与 `get_cpulocal_var` 宏）、`minix3/minix/include/minix/priv.h`（权限模板与调度者槽）、`minix3/minix/kernel/debug.c`（`runqueues_ok_cpu`）、`minix3/minix/kernel/proto.h`（`runqueues_ok_local` 宏）、`minix3/minix/kernel/arch/i386/arch_clock.c`（`CP_NICE` 消费链）、`minix3/minix/kernel/main.c`（`proc_ptr` 早期置位）。
> **Rust 实现**：`os/kernel/src/sched.rs`（`Scheduler` 纯队列操作、`EnqueueInfo`、`sched_proc`、`SchedParams`）、`os/kernel/src/proc_table.rs`（`rts_set`/`rts_unset` 队列半、`sched_enqueue`/`sched_enqueue_with`/`sched_enqueue_head`、`sched_dequeue`、`sched_proc_no_time`、`check_quantum`、`preemptible`）、`os/kernel/src/lib.rs`（`pick_and_bill`/`requeue_if_preempted`——循环侧调用点，[10-switch-to-user.md](10-switch-to-user.md) §4.3 已立）、`os/kernel/src/proc.rs`（`priority` 常量、`Accounting`）、`os/kernel/src/syscall.rs`（`dispatch_schedule` 的 niced 线）、`os/kernel/src/capability.rs`（`PREEMPTIBLE` 与模板组合常量）、`os/kernel/src/clock.rs`（`classify_cpu_state`）、`os/kernel/src/debug.rs`（`runqueues_ok_cpu`）、`os/kernel/Cargo.toml`（`sanity-checks` feature）。
> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。引用位置由符号名定位（`path:func`、`path:fn NAME`、`path:struct NAME`），**行号不作锚点**——行号只出现在四类场合：逐字引用块的块级区间题注（带复算命令）、被引注释内部（随原文保留）、§6.2 的核对命令、以及符号名旁为短小事实注明的取证行号。
> **本篇主题**：一条不变量——进程在就绪队列里 ⇔ 它可运行——与维护它的四个原语（入队两条路、出队、选人）；量子耗尽后的两条分支（内核原地续量 vs 交还用户态调度服务器）；两个已登记的现状面（每 CPU 队列的激活——唤醒生产者已落、接收侧未齐、BSP 钳制兜底；PREEMPTIBLE 门的现状形状）与一条已闭合链路（niced 记账消费）。
> **前置**：[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md)（§1.4 的 boot 期按住、§2.2 的 RTS 位族表、§2.3 的权限模板族）、[10-switch-to-user.md](10-switch-to-user.md)（§2.1 的循环五拍——四原语的最大调用方、§4.3 的循环侧调用点）、[08-system-init-boot-finish.md](08-system-init-boot-finish.md)（§3.2 的 BKL 取得点与持锁约定）。
> **后续**：`12-ipc-core`（IPC 核心篇）、`16-smp`（SMP 篇，尚未重生成）、`17-clock-timer`（时钟与定时器运行篇，尚未重生成）、`02-stage-sched` 方向（SCHED 服务器用户态本体，边界句见 §1.4）。
> **目标读者**：熟悉操作系统基本概念（进程、就绪队列、优先级调度、内核/用户态边界）与 C 语言的开发者；Rust 一侧读到 `Option`/原子类型/借用切片大意即可。
> **前置知识**：[10-switch-to-user.md](10-switch-to-user.md) 的循环五拍（四原语的最大调用方）、[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4/§2.2（boot 期按住与 RTS 位族）、[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.2（BKL 取得点）。
> **教学目标**：①能说出 INV-1 与 RTS 宏的跃迁检测形状；②能按 16 条优先级链复述队列结构与"挂在被指派 CPU"；③能复述量子耗尽两分支的判据与各自语义；④能按舞步复述 `sched_proc` 的改参往返；⑤能说出两个现状面（每 CPU 队列的唤醒生产者已在但接收侧未齐、BSP 钳制仍兜底；PREEMPTIBLE 门现状）与 niced 链已闭合的边界。
> **三档术语**：本篇的 Rust 实现相对 Minix3 C 属 Rewrite（保持外部行为）与 Refactor（不改语义的重构：机制/策略的类型级拆分、回执数据流）；`priority` 上界收紧等偏离在 §3.4 处置表单独占行；无 `[ARCH: ...]` 标注对象。
> **编号说明**：正文与引文里出现的 `R-15`、`U-2`、`S-6`、`D-52` 等编号是本仓的开发工作条目号与评审记录编号——原文保留，溯源按编号在本仓检索；`R11-T*` 为本文 §5.4 自含的交接编号。

[10-switch-to-user.md](10-switch-to-user.md) 的循环每一拍都在向本篇的四个函数要答案：PREEMPTED 清位后的重排问"头插还是尾插"，`while (!(p = pick_proc())) idle();` 问"下一个是谁"，拍五的量子检查问"时间用完了之后呢"。循环是调用方，本篇是被调用侧的本体：内核用什么数据结构表达"谁能跑"，用什么装置保证这份结构与"标志位说它能跑"永远一致，以及在时间片耗尽这个最常见的调度时刻，策略从哪里分岔。

本篇走完这组原语：C 的装置与策略怎么写（第 2 章）、Rust 用什么形状承接（第 3 章）、逐函数怎么落地（第 4 章）、靠什么测试钉住（第 5 章）、卡住时怎么查（第 6 章）。

---

## 1. 概述

### 1.1 核心不变量：在队列里 ⇔ 可运行

内核回答"这个进程能不能上 CPU"时，手里有两份彼此独立的记录。第一份是进程表里的运行时标志字：`p_rts_flags`，它的语义判据写在 `minix3/minix/kernel/proc.h:proc_is_runnable`——

```c
// minix3/minix/kernel/proc.h:proc_is_runnable —— 逐字引用（168-170 行，未删节）
/* A process is runnable iff p_rts_flags == 0. */
#define rts_f_is_runnable(flg)	((flg) == 0)
#define proc_is_runnable(p)	(rts_f_is_runnable((p)->p_rts_flags))
```

（区间复算：`sed -n '168,170p' minix3/minix/kernel/proc.h | wc -l` = 3。16 个位的逐位语义归 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.2 的位族表，本篇只引用"全零即可运行"这一条判据。）

第二份是就绪队列链：每颗 CPU 一组按优先级排列的单向链表，`pick_proc` 只从队列里挑人。判据是语义层的（"标志字说它能跑"），队列是结构层的（"它挂在链上"）。这两份记录必须时刻指向同一个结论：**进程在就绪队列里 ⇔ 它可运行**——本篇称之为 INV-1。判据自己不会把进程挂上链，队列自己也不会翻转标志位；在两者之间搬运一致性的装置，是 `minix3/minix/kernel/proc.h:RTS_SET` 与 `minix3/minix/kernel/proc.h:RTS_UNSET` 两个宏：

```c
// minix3/minix/kernel/proc.h:RTS_SET/RTS_UNSET —— 逐字引用（200-224 行，未删节）
/* These runtime flags can be tested and manipulated by these macros. */

#define RTS_ISSET(rp, f) (((rp)->p_rts_flags & (f)) == (f))


/* Set flag and dequeue if the process was runnable. */
#define RTS_SET(rp, f)							\
	do {								\
		const int rts = (rp)->p_rts_flags;			\
		(rp)->p_rts_flags |= (f);				\
		if(rts_f_is_runnable(rts) && !proc_is_runnable(rp)) {	\
			dequeue(rp);					\
		}							\
	} while(0)

/* Clear flag and enqueue if the process was not runnable but is now. */
#define RTS_UNSET(rp, f) 						\
	do {								\
		int rts;						\
		rts = (rp)->p_rts_flags;				\
		(rp)->p_rts_flags &= ~(f);				\
		if(!rts_f_is_runnable(rts) && proc_is_runnable(rp)) {	\
			enqueue(rp);					\
		}							\
	} while(0)
```

（区间复算：`sed -n '200,224p' minix3/minix/kernel/proc.h | wc -l` = 25。）

读这两个宏，各有一个不看源码想不到的形状。`RTS_SET` 先把旧值存进局部变量 `rts`，再置位，然后才做判定——判定比较的是"旧值可运行**并且**新值不可运行"。也就是说，宏执行的不是"置了某个位就出队"，而是**跃迁检测**：只有这次写操作真的把进程从可运行推过线变成不可运行，才调 `dequeue(rp)`。往一个已经停着的进程身上再叠一个阻塞位，宏什么队列动作都不做。`RTS_UNSET` 完全对称：存旧值、清位、检测"旧值不可运行并且新值可运行"，跃迁成立才 `enqueue(rp)`。

这个形状把一件事说在结构上：标志变化与队列变化是**同一个宏里的两个动作**，不是两处独立代码。任何绕开宏的写法——直接调 `dequeue` 而不置位、直接改 `p_rts_flags` 而不出队——都会造出两份记录各说各话的时刻，而内核里没有第三道机制去兜底。把双载体与装置画在一起：

```
        RTS 标志字（p_rts_flags）                 就绪队列链（run_q_head / run_q_tail）
        语义层判据：全零即可运行                   结构层事实：pick_proc 只认队列
               │                                        │
 RTS_SET(rp,f) │ 存旧值 → 置位                           │
  （进程停下）  │ 跃迁检测：旧值可运行 && 新值不可运行 ───────► dequeue(rp) 出队
               │                                        │
 RTS_UNSET(rp,f) 存旧值 → 清位                           │
  （进程醒来）  │ 跃迁检测：旧值不可运行 && 新值可运行 ───────► enqueue(rp) 入队
               │                                        │
               └─────────────── INV-1 ─────────────────┘
                  在队列里 ⇔ 可运行（两份记录同真同假）
```

两条反方向的推演，各坏在一个能指认的对象上。其一，设想 `RTS_UNSET` 只清位、忘了入队：进程的 `p_rts_flags` 归零，判据说它可运行；但 `run_q_head`/`run_q_tail` 的每一条链上都没有它。此后每一拍调度，`pick_proc` 从 0 号队列扫到 15 号，扫描只看队列数组（`minix3/minix/kernel/proc.c:pick_proc`），这个进程永远不在扫描结果里——默认构建下没有断言触发、没有输出、没有 panic，一个判据上可运行的进程被永久饿死（`sanity-checks` 开启时检查 5 会报，见 §2.4 同款推演的限定）。其二，设想 `RTS_SET` 只置位、忘了出队：一个已经阻塞的进程还挂在链上，`pick_proc` 会选中它，循环照样切给它 CPU——恢复出来的是一个 blocked 现场，它根本跑不动下一步（debug 构建下，坏进程一旦成为队首就会被 §2.2 的队首可运行断言拦下；未到队首则由 `sanity-checks` 的 unready-on-runq 检查报出）。

### 1.2 队列的形状：16 条优先级链挂在每颗 CPU 上

INV-1 里的"队列"具体长什么样？常量表是入口：

```c
// minix3/minix/include/minix/config.h —— 逐字引用（66-74 行，未删节）
#define NR_SCHED_QUEUES   16	/* MUST equal minimum priority + 1 */
#define TASK_Q		   0	/* highest, used for kernel tasks */
#define MAX_USER_Q  	   0    /* highest priority for user processes */   
#define USER_Q  	  ((MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q) /* default
						(should correspond to nice 0) */
#define MIN_USER_Q	  (NR_SCHED_QUEUES - 1)	/* minimum priority for user
						   processes */
/* default scheduling quanta */
#define USER_QUANTUM 200
```

（区间复算：`sed -n '66,74p' minix3/minix/include/minix/config.h | wc -l` = 9。）

16 条队列按优先级排列：0 号最高（`TASK_Q`，内核任务），15 号最低（`MIN_USER_Q = NR_SCHED_QUEUES - 1`）。每条链是一列单向链接的进程，头尾两个指针成对管理——声明在每 CPU 本地变量里：

```c
// minix3/minix/kernel/cpulocals.h —— 逐字引用（57-60 行，未删节）
/* CPU private run queues */
	struct proc * run_q_head[NR_SCHED_QUEUES]; /* ptrs to ready list headers */
	struct proc * run_q_tail[NR_SCHED_QUEUES]; /* ptrs to ready list tails */
	int cpu_is_idle; /* let the others know that you are idle */
```

（区间复算：`sed -n '57,60p' minix3/minix/kernel/cpulocals.h | wc -l` = 4。）

两组事实藏在常量表里，各单独说一句。第一，`USER_Q` 不是拍脑袋定的常数，它是**算出来的**：`(MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q`，代入 15 和 0 得 `(15 - 0) / 2 + 0`，C 的整数除法向下取整，值是 7——默认用户优先级在队列 7，注释自述"should correspond to nice 0"。推论跟着算式走：改 `NR_SCHED_QUEUES` 会改 `MIN_USER_Q`，再连带改 `USER_Q`，默认用户优先级随之漂移。表头那句 "MUST equal minimum priority + 1" 是约束声明：队列数必须恰好容纳 0 到最低优先级的每一档，少一档则 `MIN_USER_Q` 指向不存在的队列。第二，`USER_QUANTUM 200` 是默认时间片的毫秒数。内核 boot 只给 VM 与 root sys proc 下发 `SRV_QT`（`main.c:210`/`:233`），其余进程的量子由用户态链路落定（RS 与 minix-service 解析器读 `SRV_QT`/`DSRV_QT`/`USR_QT` 模板常量；PM 对用户进程直接传 `USER_QUANTUM`）（模板族归 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.3）。

队列挂在哪里，由入队与出队的公共注释给出答案——`minix3/minix/kernel/proc.c:enqueue` 的头注释写明 "This function can be used x-cpu as it always uses the queues of the cpu the process is assigned to"（§2.0 逐字引用）：队列挂在**进程被指派的 CPU** 上，不是当前执行代码的 CPU。跨 CPU 的代码路径因此天然可用：无论哪个 CPU 上的内核代码唤醒了一个进程，`RTS_UNSET` 触发的入队都按 `rp->p_cpu` 落到它自己的队列组里。

进程的优先级从三个入口写入，同一个字段 `p_priority` 决定它挂哪条链：

| 来源 | 时机 | 锚点 |
|---|---|---|
| boot 期特权模板 | 进程表初始化时按模板写 `p_priority`/`p_quantum_size_ms`（内核 `p_scheduler` 的初值是硬编码 NULL〔`minix3/minix/kernel/proc.c:134`〕；调度者槽模板（`priv.h` 的 `SRV_SCH`/`DSRV_SCH`/`USR_SCH`）由用户态 RS 消费、经 `sched_start→SYS_SCHEDCTL` 链路落到内核，§3.3 按名引用） | [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.3（模板族，不含调度者槽组） |
| SYS_SCHEDULE / SYS_SCHEDCTL 改参 | 用户态调度服务器运行期改优先级与时间片 | 本篇 §2.3（`minix3/minix/kernel/system.c:sched_proc`） |
| 调度服务器策略 | SCHED 服务器的 MLFQ 降级与回升决策，经上一行落到内核 | `02-stage-sched` 方向（SCHED 服务器用户态本体，尚未重生成） |

把"挂在被指派的 CPU"反过来推演：设想队列挂在**当前 CPU**——唤醒代码在 A CPU 上跑，就把进程挂进 A CPU 的队列组；而进程被指派给 B CPU，B CPU 的 `pick_proc` 只扫自己的 `run_q_head`，永远扫不到 A CPU 链上那个它该跑的进程。进程在 A CPU "被唤醒"这个事件发生之后，在 B CPU 的视角里仍然不存在——INV-1 的结构层半边（在队列里）在错误的 CPU 上成立，判据半边（可运行）指向的却是 B CPU 的 pick 行为。

### 1.3 量子耗尽后的两条分支

队列与原语是"换人"的装置，但调度最常见的一个时刻是"不换人"：时间片用完了。tick 到达后把当前进程的剩余时间扣减到 0 的时钟半归 `17-clock-timer`（时钟与定时器运行篇，尚未重生成）；扣到 0 之后内核怎么办，是本篇的分岔口。循环的拍五在重验可运行之前调用 `minix3/minix/kernel/proc.c:proc_no_time`（[10-switch-to-user.md](10-switch-to-user.md) §1.3 的拍五调用点），它的全部逻辑是 18 行里的一个二选一：

```c
// minix3/minix/kernel/proc.c:proc_no_time —— 逐字引用（1893-1910 行，未删节）
void proc_no_time(struct proc * p)
{
	if (!proc_kernel_scheduler(p) && priv(p)->s_flags & PREEMPTIBLE) {
		/* this dequeues the process */
		notify_scheduler(p);
	}
	else {
		/*
		 * non-preemptible processes only need their quantum to
		 * be renewed. In fact, they by pass scheduling
		 */
		p->p_cpu_time_left = ms_2_cpu_time(p->p_quantum_size_ms);
#if DEBUG_RACE
		RTS_SET(p, RTS_PREEMPTED);
		RTS_UNSET(p, RTS_PREEMPTED);
#endif
	}
}
```

（区间复算：`sed -n '1893,1910p' minix3/minix/kernel/proc.c | wc -l` = 18。）

分岔的判据是两个条件相与：`!proc_kernel_scheduler(p)`——这个进程不是内核默认策略在管（判据宏 `minix3/minix/kernel/proc.h:proc_kernel_scheduler`：`p_scheduler == NULL || p_scheduler == p`，即没有调度服务器、或自己调度自己）；**并且**特权标志带 `PREEMPTIBLE`（`priv(p)->s_flags & PREEMPTIBLE`，位在特权表里，模板归属 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.3）。两个条件同时成立，走 `notify_scheduler(p)`：出队这个进程，并代表它给它的调度服务器发一条"我 quantum 用完了"的消息（§2.3 逐字对它）。任何一个条件不成立，走 `else` 分支：`p->p_cpu_time_left = ms_2_cpu_time(p->p_quantum_size_ms)`——把剩余时间重置成整片，然后什么都不做。

| 分支 | 条件 | 动作 | 调度语义 |
|---|---|---|---|
| 交还调度服务器 | 非内核默认策略 **且** PREEMPTIBLE | 出队 + 给 `p_scheduler` 发 NO_QUANTUM 消息 | "用完交权"，下一个进程由服务器策略决定 |
| 原地续量 | 内核默认策略 **或** 不可抢占 | `p_cpu_time_left` 重置为整片 | 绕过调度——重新调度根本不发生 |

这条分界线上有一个不看源码想不到的事实：**大多数进程走的是"绕过"分支**。内核任务自不待言（它们 `p_scheduler == NULL`），即便在用户进程里，只要没被调度服务器接管、或者特权模板没给 PREEMPTIBLE 位，时间片耗尽都只是原地满血复活——`else` 分支的注释把它们点得明明白白："non-preemptible processes only need their quantum to be renewed. In fact, they by pass scheduling"。真正"用完交权"的，只有被用户态调度服务器接管且可抢占的那批进程。

反过来推演：设想内核任务也走交还分支。时钟任务的时间片到点后，它得等调度服务器裁决下一个是谁；可调度服务器自己也是一个进程，它的裁决要靠收到消息才能做出，而消息要靠 CPU 空出来才能被处理——裁决期间 tick 没人接，时基停摆，整个系统的时间维度先于任何进程饿死而崩塌。"绕过"分支不是对用户态调度的否定，它是给系统根基（时钟、内核任务）预留的不经裁决的续命通道。

把第 1 章的骨架收一次。三大件：一条不变量（INV-1，两份记录同真同假，装置是 RTS 宏的跃迁检测）、一组形状（16 条按优先级排列的单向链，挂在进程被指派的 CPU 上，优先级从三个入口写入）、一个分岔（量子耗尽后按"谁在管我 + 可不可抢占"分成交还与续量两路）。第 2 章按这三大件逐字走 C 代码：先看四原语与两个策略面的地图，再依次走入队的两条路、出队与选人、策略面三函数，最后合起来做 INV-1 的完整推演。

### 1.4 本篇范围与可以安全跳过的内容

| 主题 | 本篇怎么办 | 去处 |
|---|---|---|
| switch_to_user 循环本体 | 只以调用方身份出现（拍二重排、拍五量子检查） | [10-switch-to-user.md](10-switch-to-user.md) §2.1/§4.3 |
| IPC 阻塞/唤醒引起的队列出入语义 | 只讲"位变 → 队列动"的装置半（RTS 宏与包装层）；SEND/RECEIVE 何时置位不展开 | `12-ipc-core`（IPC 核心篇） |
| tick 到达后的 quantum 递减 | 只讲扣到 0 之后的分支；递减本体不讲 | `17-clock-timer`（时钟与定时器运行篇，尚未重生成） |
| IPI 跨 CPU 唤醒 | 只讲 C 入队里 SMP 半段的形状与现状（§3.3） | `16-smp`（SMP 篇，尚未重生成） |
| SCHED 服务器用户态本体 | 只给边界句：策略算法、消息协议在服务器侧 | `02-stage-sched` 方向（尚未重生成） |
| RTS 位族全表与 boot 期按住 | 引用不重复 | [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.2/§1.4 |

认知减负声明：本章不必深究 IPC 阻塞/唤醒的语义——本篇只保证读者能回答"标志位翻转的那一刻队列发生了什么"，至于哪个内核路径因为什么原因翻转它，归 `12-ipc-core`（IPC 核心篇）。也不必深究 tick 怎么把剩余时间扣到 0、IPI 怎么横跨 CPU、SCHED 服务器在用户态怎么决策——分别是 `17-clock-timer`、`16-smp`（两篇，均尚未重生成）与 `02-stage-sched` 方向的辖区。

### 1.5 C 侧覆盖边界（本篇叙述面逐项对账）

本篇的叙述面横跨三类材料，C 侧有无对照逐项列清——**无 C 对应物的分支不得读作"与 C 一致"**：

| 叙述面 | C 侧对照 | 边界说明 |
|---|---|---|
| 四原语（`enqueue`/`enqueue_head`/`dequeue`/`pick_proc`）与 RTS 宏 | **有**：`proc.c`/`proc.h` 是架构通用 C 代码 | 逐段走查以 i386 构建的行号定位（第 2 章） |
| 优先级常量与权限模板（`config.h`/`priv.h`） | **有** | 模板槽的运行期接管链在用户态（PM/RS/SCHED），见 §3.3 缺口二 |
| per-CPU 队列（`cpulocals.h` 双数组） | **有（CONFIG_SMP 构建）**：调用点按 `rp->p_cpu` 把 CPU 号传给 `get_cpu_var` | Rust 收拢成 `Scheduler` 结构、队列恒 BSP（§3.3 缺口一） |
| 机制/策略的类型级拆分（`Scheduler`/`EnqueueInfo`/回执数据流） | **无**：C 是注释级约定 | Rust 独有设计（§3.1） |
| `Option` 哨兵与 `SchedProcError` 枚举 | **无**：C 用 `-1` 哨兵与 errno | 改型（§3.4 处置表） |
| priority 上界收紧（Rust 拒 16） | **有意偏离**：C 放行 16（`rdy_head[16]` 越界缺陷） | 改型（修正），§3.4 表单独占行 |
| `enter_queue` 写入对象改指被入队进程 | **有意偏离**：C 写 `proc_ptr` 的账 | 改型（修正），§3.4 表单独占行 |
| `dequeue` 未命中 panic / 摘链后清 `p_nextready` | **有意偏离**：C 静默通过、不清被摘节点自己的 `p_nextready` | 审计更严面（§4.1 披露） |
| 抢占门空置跳过（`proc_ptr` 为空） | **无 C 对位**：C 保证 `proc_ptr` 非空（`main.c:57`/`:259-261`） | Rust 宿主/早启窗口的防御性新增（§3.3 缺口二登记） |
| `sanity-checks` feature 与 debug 断言两档 | **部分**：C 是 `#if DEBUG_SANITYCHECKS` 单档 | 形态对位、档位不同（§4.3） |

纪律：凡本表标"无 C 对位/有意偏离"的面，正文一律不写"与 C 一致"；偏离面的登记以 §3.4 处置表为主，另有几处只在 §3.3 缺口二/§4.1 披露（抢占门空置跳过、摘链清 `p_nextready`）。

---

## 2. C 源码分析

### 2.0 本章地图：四原语与两个策略面

本章的演员表：四个原语函数——`enqueue`（尾插，`minix3/minix/kernel/proc.c:enqueue`）、`enqueue_head`（头插，`minix3/minix/kernel/proc.c:enqueue_head`）、`dequeue`（摘链，`minix3/minix/kernel/proc.c:dequeue`）、`pick_proc`（选人，`minix3/minix/kernel/proc.c:pick_proc`）；两个策略面——RTS 宏（位变化触发的自动队列半，§1.1 已引）与 `proc_no_time`/`sched_proc`（时间与参数变化触发的策略函数）。四原语的函数头注释各自声明了职责边界，先看信息量最大的那条——`enqueue` 的：

```c
// minix3/minix/kernel/proc.c:enqueue —— 逐字引用（函数头注释，1599-1606 行，未删节）
/* Add 'rp' to one of the queues of runnable processes.  This function is 
 * responsible for inserting a process into one of the scheduling queues. 
 * The mechanism is implemented here.   The actual scheduling policy is
 * defined in sched() and pick_proc().
 *
 * This function can be used x-cpu as it always uses the queues of the cpu the
 * process is assigned to.
 */
```

（区间复算：`sed -n '1599,1606p' minix3/minix/kernel/proc.c | wc -l` = 8。其余三个原语的函数体在 §2.1/§2.2 的全函数引用里逐字可见；`enqueue_head` 的引导注释（"to be fair"）在 §2.1 正文转述。）

这条注释把机制与策略的分界写在明处：`enqueue` 只管"怎么放进队列"——接尾、修指针、记账；"放哪个队列"不归它决定，那是调用方已经写进 `p->p_priority` 的策略结果。`minix3/minix/kernel/proc.c:pick_proc` 的头注释对称地声明了另一半："Decide who to run now"，并且点出 `bill_ptr` 这个记账副作用。机制/策略分界画成图：

```
   策略面（决定"谁、哪个队列"）                    机制面（执行"怎么放/怎么取"）
   ──────────────────────────                    ─────────────────────────
   RTS_SET / RTS_UNSET 宏            跃迁触发 ─────►  dequeue(rp) / enqueue(rp)
   （位变化：阻塞、唤醒、抢占…）                        │ 尾插进 p_priority 号链
                                                      │ 查找式摘链
   proc_no_time                        量子耗尽 ─────►  notify_scheduler ──► RTS_SET(NO_QUANTUM)
   （时间片到点：交还 or 续量）            续量分支：原地重置，不碰队列
                                              │
   sched_proc                        改参请求 ─────►  NO_QUANTUM 置位-清位舞步
   （SYS_SCHEDULE / SYS_SCHEDCTL）                    （借宏制造一次合法的出队-入队往返）
                                              │
   （原语侧）pick_proc              每一拍循环 ─────►  从 0 号链扫到 15 号，取队首
```

这个地图回答"四原语各自的接口与职责边界"：`enqueue`/`enqueue_head` 只在尾部/头部插入；`dequeue` 只负责摘链；`pick_proc` 只负责扫描选人；所有"为什么、什么时机"都留在策略面。顺着这张图，§2.1 走入队的两条路，§2.2 走出队与选人，§2.3 走策略面三函数。

### 2.1 enqueue 与 enqueue_head：入队的两条路

`enqueue` 全函数（`minix3/minix/kernel/proc.c:enqueue`，65 行，`sed -n '1595,1659p' minix3/minix/kernel/proc.c | wc -l` 复算）：

```c
// minix3/minix/kernel/proc.c:enqueue —— 逐字引用（1595-1659 行，未删节）
void enqueue(
  register struct proc *rp	/* this process is now runnable */
)
{
/* Add 'rp' to one of the queues of runnable processes.  This function is 
 * responsible for inserting a process into one of the scheduling queues. 
 * The mechanism is implemented here.   The actual scheduling policy is
 * defined in sched() and pick_proc().
 *
 * This function can be used x-cpu as it always uses the queues of the cpu the
 * process is assigned to.
 */
  int q = rp->p_priority;	 		/* scheduling queue to use */
  struct proc **rdy_head, **rdy_tail;
  
  assert(proc_is_runnable(rp));

  assert(q >= 0);

  rdy_head = get_cpu_var(rp->p_cpu, run_q_head);
  rdy_tail = get_cpu_var(rp->p_cpu, run_q_tail);

  /* Now add the process to the queue. */
  if (!rdy_head[q]) {		/* add to empty queue */
      rdy_head[q] = rdy_tail[q] = rp; 		/* create a new queue */
      rp->p_nextready = NULL;		/* mark new end */
  } 
  else {					/* add to tail of queue */
      rdy_tail[q]->p_nextready = rp;		/* chain tail of queue */	
      rdy_tail[q] = rp;				/* set new queue tail */
      rp->p_nextready = NULL;		/* mark new end */
  }

  if (cpuid == rp->p_cpu) {
	  /*
	   * enqueueing a process with a higher priority than the current one,
	   * it gets preempted. The current process must be preemptible. Testing
	   * the priority also makes sure that a process does not preempt itself
	   */
	  struct proc * p;
	  p = get_cpulocal_var(proc_ptr);
	  assert(p);
	  if((p->p_priority > rp->p_priority) &&
			  (priv(p)->s_flags & PREEMPTIBLE))
		  RTS_SET(p, RTS_PREEMPTED); /* calls dequeue() */
  }
#ifdef CONFIG_SMP
  /*
   * if the process was enqueued on a different cpu and the cpu is idle, i.e.
   * the time is off, we need to wake up that cpu and let it schedule this new
   * process
   */
  else if (get_cpu_var(rp->p_cpu, cpu_is_idle)) {
	  smp_schedule(rp->p_cpu);
  }
#endif

  /* Make note of when this process was added to queue */
  read_tsc_64(&(get_cpulocal_var(proc_ptr)->p_accounting.enter_queue));


#if DEBUG_SANITYCHECKS
  assert(runqueues_ok_local());
#endif
}
```

按执行顺序拆成五段读。第一段，断言与定位：`assert(proc_is_runnable(rp))`——入队的先决是位已经清干净（位先变、队列后动，这个次序由 §1.1 的 `RTS_UNSET` 宏保证）；随后 `get_cpu_var(rp->p_cpu, run_q_head)` 按**被指派的 CPU** 取队列组（§1.2 的 x-cpu 注释在这里落成代码）。第二段，尾插本体：队列为空则头尾都指向自己，`rp->p_nextready = NULL` 标记新链尾；非空则旧尾接上新进程、尾指针挪过来。第三段，内嵌抢占判定——`if (cpuid == rp->p_cpu)` 只在"入队落在当前 CPU"时触发：`p->p_priority > rp->p_priority` 比较的是优先级数值，数值大意味着优先级低，所以这个条件读作"当前进程排不过新来的"；再过 `priv(p)->s_flags & PREEMPTIBLE` 的特权门，两关都过就 `RTS_SET(p, RTS_PREEMPTED)` 给**当前进程**置抢占位。注释 "calls dequeue()" 点名了原语之间的调用关系：`RTS_SET` 的跃迁检测成立时会在 `enqueue` 的执行途中递归调进 `dequeue`——原语不是一条直线，是网状的。第四段，SMP 半段：入队落在**别的** CPU 且那颗 CPU 正 idle（`cpu_is_idle`）时，`smp_schedule(rp->p_cpu)` 唤醒它来调度这个新进程。第五段，记账：`read_tsc_64` 把当前时刻写进 `enter_queue`。

第五段有一个对着代码才能发现的事实：写入的目标是 `get_cpulocal_var(proc_ptr)->p_accounting.enter_queue`——**当前进程**的账本，不是被入队的 `rp` 的账本。注释说 "Make note of when this process was added to queue"，"this process"在句法上指 `rp`，写入的却是 `proc_ptr`（当前正占着 CPU 的进程）。`enqueue_head` 里同型的一行（`get_cpulocal_var(proc_ptr->p_accounting.enter_queue)`）把同一个写法又用了一遍。时间戳记在谁的账上、谁的下一次出队才会把它结算掉，这两处在 C 里并不对齐；Rust 侧对这里做了修正并自称修的是 MINIX3 的 bug（§4.2 的 Phase 4 对读）。

抢占判定的三个条件各挡住一类错误，画成链路：

```
   enqueue(rp) 内嵌抢占判定（C: if (cpuid == rp->p_cpu) 臂）
        │
        ├─ cpuid == rp->p_cpu ？        ──否──► 跨 CPU 入队：本 CPU 不被打扰
        │                                    （转 SMP 半段：目标 idle 则 smp_schedule）
        ├─ p->p_priority > rp->p_priority ？ ──否──► 新来的不比当前高：排队等轮转
        │      （数值大 = 优先级低；相等也不算）
        └─ priv(p)->s_flags & PREEMPTIBLE ？ ──否──► 当前不可抢占（内核任务）：
                                               排队等当前量子自然耗尽
             │全过
             ▼
   RTS_SET(当前进程, RTS_PREEMPTED)   /* calls dequeue() */
   当前进程出队；循环拍二看见 PREEMPTED 位 → 有余时头插、无余时尾插
   （循环侧重排的调用点归 [10-switch-to-user.md](10-switch-to-user.md) §2.1/§4.3）
```

`enqueue_head` 全函数（`minix3/minix/kernel/proc.c:enqueue_head`，42 行，`sed -n '1670,1711p' minix3/minix/kernel/proc.c | wc -l` 复算）：

```c
// minix3/minix/kernel/proc.c:enqueue_head —— 逐字引用（1670-1711 行，未删节）
static void enqueue_head(struct proc *rp)
{
  const int q = rp->p_priority;	 		/* scheduling queue to use */

  struct proc **rdy_head, **rdy_tail;

  assert(proc_ptr_ok(rp));
  assert(proc_is_runnable(rp));

  /*
   * the process was runnable without its quantum expired when dequeued. A
   * process with no time left should have been handled else and differently
   */
  assert(rp->p_cpu_time_left);

  assert(q >= 0);


  rdy_head = get_cpu_var(rp->p_cpu, run_q_head);
  rdy_tail = get_cpu_var(rp->p_cpu, run_q_tail);

  /* Now add the process to the queue. */
  if (!rdy_head[q]) {		/* add to empty queue */
	rdy_head[q] = rdy_tail[q] = rp; 	/* create a new queue */
	rp->p_nextready = NULL;			/* mark new end */
  } else {					/* add to head of queue */
	rp->p_nextready = rdy_head[q];		/* chain head of queue */
	rdy_head[q] = rp;			/* set new queue head */
  }

  /* Make note of when this process was added to queue */
  read_tsc_64(&(get_cpulocal_var(proc_ptr->p_accounting.enter_queue)));


  /* Process accounting for scheduling */
  rp->p_accounting.dequeues--;
  rp->p_accounting.preempted++;

#if DEBUG_SANITYCHECKS
  assert(runqueues_ok_local());
#endif
}
```

头插函数的注释先给适用场景：进程被抢占、从就绪队列里摘了出去，等它能跑时**插回队首**——"to be fair"说的是公平的份额：它的量子没用完，余下的片该还给它。三个断言把"头插只用于这种场景"钉死：`proc_ptr_ok(rp)`（槽位魔数完好）、`proc_is_runnable(rp)`（位已清）、`rp->p_cpu_time_left` 非零——注释跟着说 "A process with no time left should have been handled else and differently"：没时间片的进程该走尾插那条路，头插一个空量子进程会破坏"队首进程马上要跑"的隐含契约。插入本体与 `enqueue` 对称，方向相反：非空队列时 `rp->p_nextready = rdy_head[q]` 接上旧队首，头指针挪到 `rp`。记账段比 `enqueue` 多两行：`dequeues--` 把出队时计的那一次减回去，`preempted++` 给这个进程的被抢计数加一——头插是"出队的逆操作"，账也要做平。

两个反向推演。设想把抢占判定从 `enqueue` 里删掉：高优先级进程入队后要等当前进程的量子自然耗尽（最长一整个 quantum，`USER_QUANTUM 200` 毫秒）才轮得到它，`pick_proc` 的扫描序还在，但优先级对延迟的意义退化成"轮到时的顺序"——交互进程的响应时间被压到量子长度。设想抢占判定不查 `PREEMPTIBLE`：时钟任务这种不可抢占的当前进程也会被掐断——它在临界区里（持着 BKL、改到一半的定时器队列），被切走后整个系统的时基与锁状态悬在半空；特权门是"谁可以被掐"的白名单，删掉它等于谁都可能被掐在半句话中间。

### 2.2 dequeue 与 pick_proc：出队与选人

`dequeue` 全函数（`minix3/minix/kernel/proc.c:dequeue`，65 行，`sed -n '1716,1780p' minix3/minix/kernel/proc.c | wc -l` 复算）：

```c
// minix3/minix/kernel/proc.c:dequeue —— 逐字引用（1716-1780 行，未删节）
void dequeue(struct proc *rp)
/* this process is no longer runnable */
{
/* A process must be removed from the scheduling queues, for example, because
 * it has blocked.  If the currently active process is removed, a new process
 * is picked to run by calling pick_proc().
 *
 * This function can operate x-cpu as it always removes the process from the
 * queue of the cpu the process is currently assigned to.
 */
  int q = rp->p_priority;		/* queue to use */
  struct proc **xpp;			/* iterate over queue */
  struct proc *prev_xp;
  u64_t tsc, tsc_delta;

  struct proc **rdy_tail;

  assert(proc_ptr_ok(rp));
  assert(!proc_is_runnable(rp));

  /* Side-effect for kernel: check if the task's stack still is ok? */
  assert (!iskernelp(rp) || *priv(rp)->s_stack_guard == STACK_GUARD);

  rdy_tail = get_cpu_var(rp->p_cpu, run_q_tail);

  /* Now make sure that the process is not in its ready queue. Remove the 
   * process if it is found. A process can be made unready even if it is not 
   * running by being sent a signal that kills it.
   */
  prev_xp = NULL;				
  for (xpp = get_cpu_var_ptr(rp->p_cpu, run_q_head[q]); *xpp;
		  xpp = &(*xpp)->p_nextready) {
      if (*xpp == rp) {				/* found process to remove */
          *xpp = (*xpp)->p_nextready;		/* replace with next chain */
          if (rp == rdy_tail[q]) {		/* queue tail removed */
              rdy_tail[q] = prev_xp;		/* set new tail */
	  }

          break;
      }
      prev_xp = *xpp;				/* save previous in chain */
  }

	
  /* Process accounting for scheduling */
  rp->p_accounting.dequeues++;

  /* this is not all that accurate on virtual machines, especially with
     IO bound processes that only spend a short amount of time in the queue
     at a time. */
  if (rp->p_accounting.enter_queue) {
	read_tsc_64(&tsc);
	tsc_delta = tsc - rp->p_accounting.enter_queue;
	rp->p_accounting.time_in_queue = rp->p_accounting.time_in_queue +
		tsc_delta;
	rp->p_accounting.enter_queue = 0;
  }

  /* For ps(1), remember when the process was last dequeued. */
  rp->p_dequeued = get_monotonic();

#if DEBUG_SANITYCHECKS
  assert(runqueues_ok_local());
#endif
}
```

开头的两个断言与 `enqueue` 方向相反：`assert(!proc_is_runnable(rp))`——出队的先决是位**已经置上**。这就是 §1.1 说的"位先变、队列后动"：调用方（通常是 RTS_SET 宏）先把标志位写成不可运行，再让 `dequeue` 摘链；队列动作是位变化的下游。另一个断言 `!iskernelp(rp) || *priv(rp)->s_stack_guard == STACK_GUARD` 顺带检查内核任务的栈哨兵——摘链的时机可能是内核栈刚被踩过的时刻，哨兵在这里顺手兜底。

摘链本体是**查找式**的：从 `run_q_head[q]` 起沿 `p_nextready` 线性扫，用二级指针 `xpp` 记"当前节点的接缝地址"，命中就 `*xpp = (*xpp)->p_nextready` 把前驱直接缝到后继。注释解释了为什么要容忍"找不到"："A process can be made unready even if it is not running by being sent a signal that kills it"——一个不在运行中的进程也可能被置成不可运行（比如被杀信号），此时它可能根本不在队列里，扫描落空就落空，静默通过。命中后还有一处容易看漏的修正：`if (rp == rdy_tail[q]) rdy_tail[q] = prev_xp`——摘掉的若是队尾，尾指针要退回前驱；漏掉这行，`rdy_tail[q]` 悬空指着一个已出队的节点，下一次尾插 `rdy_tail[q]->p_nextready = rp` 会把新进程链进孤儿节点后面，队列从那里断成两截。

记账三件：`dequeues++` 累计出队次数；`time_in_queue` 结算这次排队时长——前提是 `enter_queue` 非零，用当前 TSC 减入队时刻，累加进累计值后把 `enter_queue` 清零（注释自嘲这在虚拟机里不准，IO 密集进程每次排队都很短）；`p_dequeued = get_monotonic()` 给 ps(1) 记"最后一次出队是什么时候"。

`pick_proc` 全函数（`minix3/minix/kernel/proc.c:pick_proc`，29 行，`sed -n '1785,1813p' minix3/minix/kernel/proc.c | wc -l` 复算）：

```c
// minix3/minix/kernel/proc.c:pick_proc —— 逐字引用（1785-1813 行，未删节）
static struct proc * pick_proc(void)
{
/* Decide who to run now.  A new process is selected and returned.
 * When a billable process is selected, record it in 'bill_ptr', so that the 
 * clock task can tell who to bill for system time.
 *
 * This function always uses the run queues of the local cpu!
 */
  register struct proc *rp;			/* process to run */
  struct proc **rdy_head;
  int q;				/* iterate over queues */

  /* Check each of the scheduling queues for ready processes. The number of
   * queues is defined in proc.h, and priorities are set in the task table.
   * If there are no processes ready to run, return NULL.
   */
  rdy_head = get_cpulocal_var(run_q_head);
  for (q=0; q < NR_SCHED_QUEUES; q++) {	
	if(!(rp = rdy_head[q])) {
		TRACE(VF_PICKPROC, printf("cpu %d queue %d empty\n", cpuid, q););
		continue;
	}
	assert(proc_is_runnable(rp));
	if (priv(rp)->s_flags & BILLABLE)	 	
		get_cpulocal_var(bill_ptr) = rp; /* bill for system time */
	return rp;
  }
  return NULL;
}
```

选人薄到只有一层循环：从 0 号队列扫到 15 号，第一个非空队列的队首就是答案——优先级语义完全靠扫描序表达，函数自身不做任何优先级比较。队列头非空时先 `assert(proc_is_runnable(rp))`（INV-1 的运行时审计点：DEBUG 构建下，队列里挂着一个不可运行进程会当场停机），再查特权标志 `BILLABLE`：被选中的进程若可计账，就写进 `bill_ptr`，时钟中断据此把系统时间记到它头上。全空则返回 NULL，调用方落进 `idle()`（[10-switch-to-user.md](10-switch-to-user.md) §4.3）。选人薄的反面是调用方厚：谁该在队列里、排在哪，全是策略面提前做好的事——`pick_proc` 只是执行了已经确定的结论。

反向推演两处。`dequeue` 的断言若反过来写成 `assert(proc_is_runnable(rp))`：被杀信号路径（先置位后摘链的次序）在断言处直接停机——位已经置上、进程不可运行，断言"它可运行"必然失败，杀进程这个最基本的内核操作变成 panic。摘链若不修尾指针：§2.2 前段推演过的孤儿节点——`rdy_tail[q]` 指着已出队节点，下次尾插把链写断。

### 2.3 proc_no_time、notify_scheduler 与 sched_proc：策略面

策略面的三个函数把"什么时刻、按什么规则动队列"讲完。`proc_no_time` 的全函数已在 §1.3 逐字引过（18 行，两分支），这里补它调用的另一半——`notify_scheduler` 全函数（`minix3/minix/kernel/proc.c:notify_scheduler`，32 行，`sed -n '1860,1891p' minix3/minix/kernel/proc.c | wc -l` 复算）：

```c
// minix3/minix/kernel/proc.c:notify_scheduler —— 逐字引用（1860-1891 行，未删节）
static void notify_scheduler(struct proc *p)
{
	message m_no_quantum;
	int err;

	assert(!proc_kernel_scheduler(p));

	/* dequeue the process */
	RTS_SET(p, RTS_NO_QUANTUM);
	/*
	 * Notify the process's scheduler that it has run out of
	 * quantum. This is done by sending a message to the scheduler
	 * on the process's behalf
	 */
	m_no_quantum.m_source = p->p_endpoint;
	m_no_quantum.m_type   = SCHEDULING_NO_QUANTUM;
	m_no_quantum.m_krn_lsys_schedule.acnt_queue = cpu_time_2_ms(p->p_accounting.time_in_queue);
	m_no_quantum.m_krn_lsys_schedule.acnt_deqs      = p->p_accounting.dequeues;
	m_no_quantum.m_krn_lsys_schedule.acnt_ipc_sync  = p->p_accounting.ipc_sync;
	m_no_quantum.m_krn_lsys_schedule.acnt_ipc_async = p->p_accounting.ipc_async;
	m_no_quantum.m_krn_lsys_schedule.acnt_preempt   = p->p_accounting.preempted;
	m_no_quantum.m_krn_lsys_schedule.acnt_cpu       = cpuid;
	m_no_quantum.m_krn_lsys_schedule.acnt_cpu_load  = cpu_load();

	/* Reset accounting */
	reset_proc_accounting(p);

	if ((err = mini_send(p, p->p_scheduler->p_endpoint,
					&m_no_quantum, FROM_KERNEL))) {
		panic("WARNING: Scheduling: mini_send returned %d\n", err);
	}
}
```

进门先断言 `!proc_kernel_scheduler(p)`——这个函数只为"有外部调度者"的进程存在。第一个动作 `RTS_SET(p, RTS_NO_QUANTUM)` 借宏完成出队：置位前的进程可运行，置位后不可运行，跃迁检测成立，`dequeue` 执行——注释 "this dequeues the process"（§1.3 的 `proc_no_time` 里）说的就是这一下。随后装配消息：`m_source` 填**耗尽进程自己的端点**（服务器收到的消息看起来是被调度进程发来的），`m_type` 是 `SCHEDULING_NO_QUANTUM`，七个 `acnt_*` 字段把记账快照（排队时长换算成毫秒、出队次数、IPC 同步/异步计数、被抢次数、CPU 号、负载）一并交给服务器。`reset_proc_accounting(p)` 清账本，最后 `mini_send(p, p->p_scheduler->p_endpoint, &m_no_quantum, FROM_KERNEL)`——以进程 `p` 的身份、从内核发到它的调度者。发送失败的处置是 panic：内核代发的消息送不出去，说明调度者端点已死或表已坏，无可恢复。注释 "on the process's behalf" 把这次代发的法律关系说清了：消息在协议上是进程发的，内核只是邮差。

策略面的第三个函数是 `sched_proc`（`minix3/minix/kernel/system.c:sched_proc`，59 行，`sed -n '642,700p' minix3/minix/kernel/system.c | wc -l` 复算）——SYS_SCHEDULE 与 SYS_SCHEDCTL 两个系统调用的共同落点（`minix3/minix/kernel/system/do_schedule.c:do_schedule` 尾行与 `minix3/minix/kernel/system/do_schedctl.c:do_schedctl` 各自调进来，见后）：

```c
// minix3/minix/kernel/system.c:sched_proc —— 逐字引用（642-700 行，59 行，未删节；内含源码自带的 FIXME 注释，按原文引用）
int sched_proc(struct proc *p, int priority, int quantum, int cpu, int niced)
{
	/* Make sure the values given are within the allowed range.*/
	if ((priority < TASK_Q && priority != -1) || priority > NR_SCHED_QUEUES)
		return(EINVAL);

	if (quantum < 1 && quantum != -1)
		return(EINVAL);

#ifdef CONFIG_SMP
	if ((cpu < 0 && cpu != -1) || (cpu > 0 && (unsigned) cpu >= ncpus))
		return(EINVAL);
	if (cpu != -1 && !(cpu_is_ready(cpu)))
		return EBADCPU;
#endif

	/* In some cases, we might be rescheduling a runnable process. In such
	 * a case (i.e. if we are updating the priority) we set the NO_QUANTUM
	 * flag before the generic unset to dequeue/enqueue the process
	 */

	/* FIXME this preempts the process, do we really want to do that ?*/

	/* FIXME this is a problem for SMP if the processes currently runs on a
	 * different CPU */
	if (proc_is_runnable(p)) {
#ifdef CONFIG_SMP
		if (p->p_cpu != cpuid && cpu != -1 && cpu != p->p_cpu) {
			smp_schedule_migrate_proc(p, cpu);
		}
#endif

		RTS_SET(p, RTS_NO_QUANTUM);
	}

	if (proc_is_runnable(p))
		RTS_SET(p, RTS_NO_QUANTUM);

	if (priority != -1)
		p->p_priority = priority;
	if (quantum != -1) {
		p->p_quantum_size_ms = quantum;
		p->p_cpu_time_left = ms_2_cpu_time(quantum);
	}
#ifdef CONFIG_SMP
	if (cpu != -1)
		p->p_cpu = cpu;
#endif

	if (niced)
		p->p_misc_flags |= MF_NICED;
	else
		p->p_misc_flags &= ~MF_NICED;

	/* Clear the scheduling bit and enqueue the process */
	RTS_UNSET(p, RTS_NO_QUANTUM);

	return OK;
}
```

校验段先立规矩：`priority < TASK_Q && priority != -1` 或 `priority > NR_SCHED_QUEUES` → `EINVAL`；`quantum < 1 && quantum != -1` → `EINVAL`；`-1` 是"保持现值"的哨兵。核心是中间那段**置位-清位舞步**：注释开头自述动机——"In some cases, we might be rescheduling a runnable process"；对可运行进程，先 `RTS_SET(p, RTS_NO_QUANTUM)`（借宏出队），中间放心改 `p_priority`/`p_quantum_size_ms`/`p_cpu`，最后 `RTS_UNSET(p, RTS_NO_QUANTUM)`（跃迁检测成立，按**新**优先级重新入队）。源码自带两条 FIXME，第一条自问 "this preempts the process, do we really want to do that ?"——置位 NO_QUANTUM 的动作本身若触发抢占判定（改参的正是当前进程时），进程会被自己这次改参掐断；第二条承认 SMP 场景下进程在别的 CPU 上运行时这里没有防护。两条都是源码作者留给后人的未决题，本篇照录不裁决。舞步的时序画出来：

```
   sched_proc(p, priority, quantum, cpu, niced)
        │
        ├─ 校验 priority / quantum / cpu      （越界 → EINVAL / EBADCPU）
        │
        ├─ if (proc_is_runnable(p))
        │     ├─（SMP：跨 CPU 改派 → smp_schedule_migrate_proc 先停后搬）
        │     └─ RTS_SET(p, RTS_NO_QUANTUM)   ── 跃迁：可运行→不可运行
        │            └──► dequeue(p)：从【旧】优先级队列摘下
        │
        ├─ p->p_priority = priority            （字段改写，进程不在任何队列里）
        ├─ p->p_quantum_size_ms = quantum; p->p_cpu_time_left = ms_2_cpu_time(quantum)
        ├─ p->p_cpu = cpu
        ├─ niced ? 置 MF_NICED : 清 MF_NICED
        │
        └─ RTS_UNSET(p, RTS_NO_QUANTUM)        ── 跃迁：不可运行→可运行
               └──► enqueue(p)：挂进【新】优先级队列
```

舞步的本质一句话：它借 NO_QUANTUM 位制造了一次**合法的出队-入队往返**，让"给可运行进程改优先级"这件事分解成"先变成不可运行、改完再回来"，从而全程经过 §1.1 的装置。不这么做的后果能落到具体对象：直接改 `p->p_priority` 而不出队——进程还挂在旧优先级的链上，`pick_proc` 扫到旧队列会选出它，而抢占判定与下一次插入又会按新优先级对待它：队列位置（结构层）与优先级字段（语义层）从此脱钩，INV-1 的结构层半边被破坏。

两个系统调用怎么落到这里，各有各的门。SYS_SCHEDULE 的门在调用者身份：`minix3/minix/kernel/system/do_schedule.c:do_schedule` 检查 `caller != p->p_scheduler` 则 `EPERM`——只有这个进程自己的调度服务器能改它的参数：

```c
// minix3/minix/kernel/system/do_schedule.c:do_schedule —— 逐字引用（9-30 行，未删节）
{
	struct proc *p;
	int proc_nr;
	int priority, quantum, cpu, niced;

	if (!isokendpt(m_ptr->m_lsys_krn_schedule.endpoint, &proc_nr))
		return EINVAL;

	p = proc_addr(proc_nr);

	/* Only this process' scheduler can schedule it */
	if (caller != p->p_scheduler)
		return(EPERM);

	/* Try to schedule the process. */
	priority = m_ptr->m_lsys_krn_schedule.priority;
	quantum = m_ptr->m_lsys_krn_schedule.quantum;
	cpu = m_ptr->m_lsys_krn_schedule.cpu;
	niced = !!(m_ptr->m_lsys_krn_schedule.niced);

	return sched_proc(p, priority, quantum, cpu, niced);
}
```

SYS_SCHEDCTL 的门在接管方向：`minix3/minix/kernel/system/do_schedctl.c:do_schedctl` 的 `SCHEDCTL_FLAG_KERNEL` 臂决定调度权归谁：

```c
// minix3/minix/kernel/system/do_schedctl.c:do_schedctl —— 逐字引用（33-46 行，未删节；文件全长 46 行）
		quantum = m_ptr->m_lsys_krn_schedctl.quantum;
		cpu = m_ptr->m_lsys_krn_schedctl.cpu;

		/* Try to schedule the process. */
		if((r = sched_proc(p, priority, quantum, cpu, FALSE)) != OK)
			return r;
		p->p_scheduler = NULL;
	} else {
		/* the caller becomes the scheduler */
		p->p_scheduler = caller;
	}

	return(OK);
}
```

带 `SCHEDCTL_FLAG_KERNEL` 标志：先按普通改参走一遍 `sched_proc`（注意实参 `FALSE`——这条路径不置位、但会清位：`sched_proc` 的 else 分支执行 `MF_NICED &= ~`，内核收权顺带清掉历史 NICED），然后把 `p->p_scheduler = NULL`——调度权收回内核默认策略。不带标志：`p->p_scheduler = caller`——调用者成为这个进程的调度服务器，此后它的量子耗尽消息（§2.3 的 `notify_scheduler`）发给这个调用者。调用者身份门在系统调用的权限侧（SCHED 服务器经 `sys_schedctl` 发起；完整身份链归 `13-syscall-dispatch`），与 `do_schedule` 的 `caller != p->p_scheduler` 门不同向。

### 2.4 本章归纳：原语总表与 INV-1 的完整推演

把本章七段代码合成一张总表（断言方向 = 函数进门时的先决检查；触发者 = 谁在什么条件下调用它）：

| 函数 | 断言方向 | 队列动作 | 记账 | 触发者 |
|---|---|---|---|---|
| `RTS_SET`（宏） | 无（自身做跃迁检测） | 可运行→不可运行时 `dequeue` | 无 | 一切阻塞/置位路径 |
| `RTS_UNSET`（宏） | 无（自身做跃迁检测） | 不可运行→可运行时 `enqueue` | 无 | 一切唤醒/清位路径 |
| `enqueue` | `proc_is_runnable` | 尾插进 `p_priority` 号链 | `enter_queue`（写入点为 proc_ptr，§2.1 辨析） | RTS_UNSET 跃迁半 |
| `enqueue_head` | 可运行 + 余量非零 | 头插 | `enter_queue` + `dequeues--` + `preempted++` | 循环拍二的 PREEMPTED 重排（有余量时） |
| `dequeue` | `!proc_is_runnable` | 查找式摘链 + 尾指针修正 | `dequeues++` + `time_in_queue` 结算 + `p_dequeued` | RTS_SET 跃迁半 |
| `pick_proc` | 队首进程 `proc_is_runnable`（DEBUG） | 无（只读） | `BILLABLE` → `bill_ptr` | 循环每一拍 |
| `proc_no_time` | 无 | 经 `notify_scheduler` 出队，或原地续量不动队列 | notify 分支重置账本 | 循环拍五量子检查 |
| `sched_proc` | 无（参数校验） | NO_QUANTUM 舞步的出队-入队往返 | 无（`reset_proc_accounting` 在 notify 侧） | SYS_SCHEDULE / SYS_SCHEDCTL |

INV-1 的一致性靠三环咬合。第一环，宏的跃迁检测：只有"可运行状态真的翻转了"才动队列，重复置位/重复清位都是空操作，不会重复出队入队。第二环，原语的断言方向：入队断言可运行、出队断言不可运行，谁想绕过宏直接调原语，DEBUG 构建下断言立即暴露方向错误。第三环，记账与队列同步更新：`enter_queue` 与摘链同函数完成、`dequeues` 与头插的 `--` 做平——账本不落后于结构。

完整推演走一遍验收题："rts_unset 忘 enqueue，故障表现是什么？"设某个路径直接清了 `p_rts_flags` 的一位（比如 `RTS_SENDING`），没经过 `RTS_UNSET`。清完后 `p_rts_flags == 0`，判据说它可运行；但 `run_q_head` 的 16 条链上没有它。此后每一拍，`switch_to_user` 调 `pick_proc`，扫描 `if(!(rp = rdy_head[q]))` 只看队列数组——这个进程永远不在任何一条链上，永远不被选中。它的进程槽字段全部合法、端点有效、内存映射完好，默认构建下没有任何一处断言会触发：没有 panic、没有输出，一个系统认为可以运行的进程被静默地排除在 CPU 之外（`sanity-checks` 开启时 `runqueues_ok_cpu` 的检查 5 会当场报出——审计点正是为这一路设的），否则直到某个 debug 工具翻进程表才会发现它。这就是"隐性饿死"的完整形状，它的落点在 `pick_proc` 的扫描那一行。第 3 章带着这张表和这条推演链，看 Rust 怎么接。

---

## 3. Rust 设计决策

### 3.1 机制/策略的显式拆分：纯队列操作与包装层（→§2.0）

第 2.0 节的 `enqueue` 头注释里，C 用一句话声明机制与策略的分界——"The mechanism is implemented here. The actual scheduling policy is defined in sched() and pick_proc()"。这句分界是给读代码的人看的约定：语言层面没有任何东西阻止 `enqueue` 的实现顺手改写进程字段或翻看策略状态，C 的单体函数也确实把断言、插链、抢占判定、记账装在同一个函数体里（§2.1 的五段）。Rust 的承接把这个注释级分界升级成**类型级分界**：队列元数据单独住进一个结构体，它暴露的操作只碰自己的数组：

```rust
// os/kernel/src/sched.rs:struct Scheduler —— 逐字引用（41-44 行，未删节）
pub struct Scheduler {
    run_q_head: [Option<ProcNr>; priority::NR_SCHED_QUEUES],
    run_q_tail: [Option<ProcNr>; priority::NR_SCHED_QUEUES],
}
```

（`minix3/minix/kernel/cpulocals.h:run_q_head` 的对位——C 的每 CPU 双数组，Rust 收拢成 `Scheduler` 的两个 `Option<ProcNr>` 数组，挂进 `ProcessTable` 的每 CPU 槽位；结构体骨架即上面四行，没有更多字段。）

尾插原语返回一个"回执"，这是整个拆分的核心机制：

```rust
// os/kernel/src/sched.rs:fn enqueue_queue_tail —— 逐字引用（88-100 行，未删节）
    pub fn enqueue_queue_tail(&mut self, nr: ProcNr, q: usize) -> EnqueueInfo {
        let old_tail = self.run_q_tail[q];
        match old_tail {
            None => {
                self.run_q_head[q] = Some(nr);
                self.run_q_tail[q] = Some(nr);
            }
            Some(_) => {
                self.run_q_tail[q] = Some(nr);
            }
        }
        EnqueueInfo { queue: q, old_tail }
    }
```

```rust
// os/kernel/src/sched.rs:struct EnqueueInfo —— 逐字引用（211-214 行，未删节）
pub struct EnqueueInfo {
    pub queue: usize,
    pub old_tail: Option<ProcNr>,
}
```

对照 C 的同一段（§2.1 第二段）：C 在 `enqueue` 里直接 `rdy_tail[q]->p_nextready = rp`——旧尾节点的 `p_nextready` 字段被就地改写。Rust 的队列项是 `ProcNr` 值不是指针，"接链"这个动作要写**旧尾进程槽**的 `p_nextready` 字段，而进程槽不在 `Scheduler` 手里——它在包装层手里。于是原语把"接链所需的全部信息"打包成 `EnqueueInfo { queue, old_tail }` 交还调用方：`old_tail` 是 `None` 说明队列原本是空的（调用方只需设置新进程自己的 `p_nextready = None`），是 `Some(nr)` 说明调用方要把 `nr` 的 `p_nextready` 指向新进程。§2.1 的"插链写谁"在 Rust 里变成一个显式的数据流：机制层产出回执，策略层消费回执。

出队原语走了另一条路：它没有回执，而是直接带走进程切片。

```rust
// os/kernel/src/sched.rs:fn dequeue_from_queue —— 逐字引用（130-186 行，未删节）
    pub fn dequeue_from_queue(
        &mut self,
        nr: ProcNr,
        q: usize,
        procs: &mut [KProcess],
    ) {
        let _link_idx: Option<usize> = None;
        let mut link_is_head = true;
        let mut prev: Option<ProcNr> = None;

        // Walk the queue to find the node
        let mut current = self.run_q_head[q];
        let mut found = false;
        while let Some(cur_nr) = current {
            if cur_nr == nr {
                found = true;
                break;
            }
            prev = Some(cur_nr);
            let cur_idx = nr_to_idx(cur_nr);
            current = cur_idx
                .and_then(|i| procs.get(i))
                .and_then(|p| {
                    let v = p.p_nextready.load(Ordering::Relaxed);
                    if v == NONE_PROC_NR { None } else { Some(ProcNr(v)) }
                });
            link_is_head = false;
        }

        if !found {
            panic!("dequeue: process not found in its priority queue");
        }

        // Remove from linked list
        let next = nr_to_idx(nr)
            .and_then(|i| procs.get(i))
            .and_then(|p| {
                let v = p.p_nextready.load(Ordering::Relaxed);
                if v == NONE_PROC_NR { None } else { Some(ProcNr(v)) }
            });

        if link_is_head {
            self.run_q_head[q] = next;
        } else if let Some(prev_nr) = prev {
            let prev_idx = nr_to_idx(prev_nr).unwrap();
            procs[prev_idx].p_nextready.store(next.map(|n| n.0).unwrap_or(NONE_PROC_NR), Ordering::Relaxed);
        }

        // Update tail if needed
        if self.run_q_tail[q] == Some(nr) {
            self.run_q_tail[q] = prev;
        }

        // Clear the removed process's nextready
        let nr_idx = nr_to_idx(nr).unwrap();
        procs[nr_idx].p_nextready.store(NONE_PROC_NR, Ordering::Relaxed);
    }
```

C 的查找式摘链用二级指针 `struct proc **xpp` 记接缝（§2.2）；Rust 的等价物是拿着 `procs: &mut [KProcess]` 切片沿链走：`p_nextready` 读旧前驱、`store` 写新接缝、命中后修尾指针，最后清被摘进程自己的 `p_nextready`。这个函数**确实**碰了进程字段——它没有遵守"纯操作不碰进程"的字面约定。这里的取舍是每个原语各自选形，取舍表：

| 方案 | 用于 | 得到 | 付出 |
|---|---|---|---|
| C 式单体：原语直接改指针链 | （被否决的默认） | 与 C 同形，无双向数据流 | `Scheduler` 要同时持有队列与进程两个数组的访问权，机制/策略在签名上重新耦合 |
| 纯操作 + 回执（`EnqueueInfo`） | `enqueue_queue_tail` | 原语零进程知识，可独立单测；接链动作显式可见 | 调用方多一步"消费回执"，漏消费编译器不管（`old_tail` 是返回值字段，未读取不产生警告；当前由 `test_enqueue_*` 与 `runqueues_ok_cpu` 的链走查（last-element-not-tail）与检查 5（可运行者必在队）兜住） |
| 原语带走进程切片 | `dequeue_from_queue` | 摘链的前驱修正、尾修正、清链一次做完，与 C 的二级指针走查逐行对得上 | 原语签名挂上 `&mut [KProcess]`，独立测试要先造进程表 |

两张回执形状不对称的理由藏在动作本身里：尾插只需要"旧尾是谁"一个信息，回执就一个字段；摘链要读写链上每一个路过节点的前驱接缝，把每一步都拆成回执等于发明第二个二级指针——复杂度转移而非消失，不如把切片交出去。包装层 `os/kernel/src/proc_table.rs:fn sched_dequeue` 的注释把这层借用关系点明：`&mut self.sched[idx]` 与 `&mut self.procs` 是互不相交的字段借用，原语一次拿两个正好。

反向推演：设想尾插也改成带走进程切片、顺手把接链接了。`Scheduler` 的每个方法签名从此都要挂 `&mut [KProcess]`，它从"队列元数据的拥有者"变成半个进程表的拥有者——§2.0 注释里那条机制/策略分界，在类型层面失去载体；而且队列逻辑的单测每次都要先构造整张进程表才能跑。反过来设想包装层绕过纯操作、直接改 `Scheduler` 的数组：回执机制被架空，`enter_queue` 记账、断言、抢占判定散落到每个调用点——INV-1 的审计点从一处变成多处，§2.4 的三环断在第二环。

包装层的一个签名细节归 [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.2 的 BKL 约定：量子耗尽入口 `os/kernel/src/proc_table.rs:fn sched_proc_no_time` 的参数里有一个 `&BklSection<'_>`——持锁见证类型，编译器要求调用方证明自己持有 BKL 才能调进来。C 里"这段代码假设持锁"是注释与约定，Rust 里它是签名。

### 3.2 量子耗尽两分支的 Rust 形态与判据核对（→§2.3）

C 的 `proc_no_time` 用一个宏决定分支归属：`minix3/minix/kernel/proc.h:proc_kernel_scheduler` 展开为 `p_scheduler == NULL || p_scheduler == p`（§1.3 已引）——"没有调度服务器"或"自己调度自己"都归内核默认策略。（`p_sched` 是进程槽内的调度子结构：优先级/量子/归属 CPU/调度者四组字段——本节与 §3.3 的 `p_sched.*` 访问都指它。）Rust 侧的同名判据与宏逐项对应：

```rust
// os/kernel/src/proc_table.rs:fn sched_proc_no_time —— 逐字引用（1067-1104 行，未删节；注释含仓库内部编号，按原文引用）
    pub fn sched_proc_no_time(
        &mut self,
        nr: ProcNr,
        priv_table: &crate::kpriv::PrivTable,
        section: &crate::smp::BklSection<'_>,
    ) {
        let (kernel_scheduled, quantum_ms) = {
            // R-15 (2026-08-12): INVARIANT: `nr` is a runnable process that
            // exhausted its quantum, so it must be in the table; `get()` cannot
            // return None.
            let p = self.get(nr).expect("sched_proc_no_time: invalid proc nr");
            let ks = p.p_sched.scheduler.is_none() || p.p_sched.scheduler == Some(p.p_nr);
            let qms = p.p_sched.quantum.size_ms.load(Ordering::Acquire);
            (ks, qms)
        };
        // C: proc.c:1895 — the PREEMPTIBLE bit lives in the privilege table,
        // so this chase needs `priv_table` (borrow discipline: C chases
        // `priv(p)` directly; the Rust kernel keeps the tables separate).
        let preemptible = self.preemptible(nr, priv_table);

        if !kernel_scheduled && preemptible {
            // User-scheduled + preemptible: dequeue + notify scheduler.
            // C: `notify_scheduler(p)` — proc.c:1860-1891.
            self.rts_set(nr, RtsFlagsBits::NO_QUANTUM);
            self.notify_scheduler(nr, section);
        } else {
            // Kernel-scheduled or non-preemptible: reset quantum
            let cpu_time = ms_to_cpu_time(quantum_ms);
            // R-15 (2026-08-12): INVARIANT: `nr` was just read above in the same
            // function and is in the table; `get_mut()` cannot return None.
            self.get_mut(nr)
                .unwrap()
                .p_sched
                .quantum
                .cpu_time_left
                .store(cpu_time, Ordering::Release);
        }
    }
```

`kernel_scheduled = scheduler.is_none() || scheduler == Some(自身)`——第一支对应 `p_scheduler == NULL`，第二支对应 `p_scheduler == p`（`ProcNr` 值比较）。自调度两侧同判：一个进程宣布"用内核默认策略管我"，让它量子耗尽时给自己发通知消息是环路，无意义，所以两侧都把它并进续量分支。判据核对到此是一句话：**两侧一致，没有宽窄之分**。preemptible 的追查单独一行：`let preemptible = self.preemptible(nr, priv_table)`——`PREEMPTIBLE` 位在特权表里不在进程结构里（§2.3 的 `priv(p)->s_flags`），Rust 的两张表又是分开的数据结构，追查要穿越表间引用：

```rust
// os/kernel/src/proc_table.rs:fn preemptible —— 逐字引用（1048-1062 行，未删节）
    /// Whether a process's privilege grants preemption.
    ///
    /// C: `priv(p)->s_flags & PREEMPTIBLE` — proc.c:1638 (enqueue
    /// preemption) / proc.c:1895 (proc_no_time). The privilege flags
    /// decide, not the priority: `MAX_USER_Q == TASK_Q == 0`
    /// (config.h:67-68), so a priority-0 user process is legally
    /// preemptible (USR_F carries PREEMPTIBLE, priv.h:49), while kernel
    /// tasks (TSK_F) lack the bit. A missing priv slot or priv id means
    /// "not preemptible" — same fallback as `is_billable` (lib.rs).
    fn preemptible(&self, nr: ProcNr, priv_table: &crate::kpriv::PrivTable) -> bool {
        self.get(nr)
            .and_then(|p| p.priv_id)
            .and_then(|pid| priv_table.get(pid))
            .is_some_and(|k| k.is_preemptible())
    }
```

C 的 `priv(p)` 是进程结构里现成的指针，一步解引用；Rust 要 `priv_id` → `priv_table.get(pid)` → 位测试三步，且每步都可能落空。落空的处置与 `is_billable` 助手同形（[10-switch-to-user.md](10-switch-to-user.md) §4.3 的 `is_billable`）：缺槽、缺 id、表项缺失都判"不可抢占"。取舍表：

| 方案 | 得到 | 付出 |
|---|---|---|
| 查特权表，缺槽判 false（采用） | 与 `is_billable` 同构；无特权记录的进程天然归内核续量分支——C 语义的保守方向 | 一张被裁剪过的特权表会让进程静默失去抢占语义 |
| 查特权表，缺槽 panic | 缺表即暴露 | boot 早期、测试夹具里没有特权记录的进程会炸掉启动 |
| 在进程结构里缓存该位 | 追查零跳数 | 第二真相源：特权位变更（`SYS_PRIVCTL`）后要同步缓存，约十处挂接点各自承担同步义务 |

分支本体与 C 逐行对得上：`!kernel_scheduled && preemptible` → `rts_set(nr, NO_QUANTUM)`（借 §3.1 的包装层宏对应物出队）+ `notify_scheduler(nr, section)`；否则 `cpu_time_left` 重置整片。C 的 `reset_proc_accounting` 在 notify 尾部清账，Rust 的 `notify_scheduler`（`os/kernel/src/proc_table.rs:fn notify_scheduler`）用 `IpcEngine::send` + `SendFlags::FROM_KERNEL` 完成代发，发送失败同样 panic（`panic!("notify_scheduler: mini_send failed ...")`），与 C 的 `panic("WARNING: Scheduling: mini_send returned %d\n")` 对应。

循环侧的量子检查在 Rust 里折叠成一个函数——`os/kernel/src/proc_table.rs:fn check_quantum`：C 在 `switch_to_user` 拍五先判 `p_cpu_time_left` 是否耗尽、耗尽则调 `proc_no_time`、然后重验可运行（三步散在循环里，[10-switch-to-user.md](10-switch-to-user.md) §1.3 的拍五）；Rust 把"耗尽判断 + 调 `sched_proc_no_time` + 重验"折成一个返回 `bool` 的调用（被调侧走查在 §4.2）。折叠不改变语义，改变的是循环代码的形状：调用方一行 `if !table.check_quantum(...)` 完成三步。

抢占位判据的双向对账（C `minix3/minix/kernel/proc.c:1895` 的判据行 / Rust 本节引文的 `self.preemptible(nr, priv_table)` 一行）：C 判 `priv(p)->s_flags & PREEMPTIBLE`，Rust 判特权表项的 `is_preemptible()`——位的归属面（哪些进程真的带位）是本节判据核对的滞后前提，与 per-CPU 队列现状一并给在 §3.3。

反向推演：设想 `preemptible` 不查特权表、改读进程字段。`PREEMPTIBLE` 是特权模板位——`minix3/minix/include/minix/priv.h:USR_F` 的组成成分，进程表里没有这个字段可读；硬要在进程上表达，就得在每个进程初始化与特权变更路径上多维护一份拷贝，而两份拷贝必有一份先过期。

### 3.3 现状盘点：per-CPU 队列、PREEMPTIBLE 置位面与 niced 链（→§1.2/§2.3）

§1.2 说队列挂在进程被指派的 CPU 上。Rust 的承接把每 CPU 结构原样搬了过来：`ProcessTable` 持有一组 `Scheduler`，按 CPU 号索引取用——`os/kernel/src/proc_table.rs:fn sched_for_cpu`（包装层所有队列操作都按 `p_sched.cpu` 或当前 CPU 落位）；`os/kernel/src/proc_table.rs:fn sched_idx` 对越界号做回退（回退到 0 号 BSP，防的是损坏的 `p_sched.cpu` 值）。结构是齐的，流量不是。以下两个缺口是**已登记的现状**（第三个主题 niced 链今天已闭合，一并给出）——写在这里是给读者一张诚实的地图：哪些分支今天走不通、当前真实路径是什么。它们不是待修 bug 清单——登记归属与修复排期不在本篇裁决范围，本篇只对码。

**缺口一：每 CPU 队列恒 BSP。** 全部进程的 `p_sched.cpu` 都是 BSP（0 号），`sched_for_cpu` 拿到的永远是同一组队列——AP 的队列组是死结构。现状的真实路径：`os/kernel/src/sched.rs:fn sched_proc` 收到指派 AP 的请求时，范围与就绪校验照 C 做（`validate_cpu_param`，越界 `EINVAL`、未就绪 `EBADCPU`），随后 `os/kernel/src/sched.rs:fn clamp_cpu_to_bsp` 把目标一律钳回 BSP——请求被接受（返回 `Ok`），进程实际留在 BSP。钳制的过渡语义在 C 的非 SMP 构建里有对应物：`system.c:sched_proc` 的 `CONFIG_SMP` 段整段在条件编译里，非 SMP 构建对 cpu 参数照单全收、忽略不写——"接受请求、进程留原地"。C 入队里的跨 CPU 半段（§2.1 第四段的 `else if cpu_is_idle → smp_schedule`）在 Rust 已有生产者对应物：`os/kernel/src/proc_table.rs:fn sched_enqueue_with` 在目标核 `cpu_is_idle` 时先 `fence(Release)` 再 `send_sched_ipi`（对位 C `proc.c:1647-1650`）。复算 `grep -rn 'smp_schedule' os/kernel/src/ | wc -l` 现得 **14** 处，仍全是迁移/停进程族与 C 映射注释；唤醒腿以 `send_sched_ipi` 命名，`grep -rn 'send_sched_ipi' os/kernel/src/ | wc -l` 得 4 处（定义侧 1、生产者 1、测试/注释 2）。这条生产者当前是**惰性的**：C2C 止损恢复了 BSP 钳制，进程不会真的落到 AP；接收侧仍有硬缺口——aarch64 的 BSP SGI 0 已在 C2D-A64SGI 使能（GICD 基址也已接上，发送按 `GICD_CTLR.ARE_NS` 选 `ICC_SGI1R_EL1`），AP 已在 C2D-APENTRY 进入 `scheduler_loop` 并打开本核 SGI+定时器门；riscv 的生产 AP 也已在 C2D-RVAPENTRY 进入 `scheduler_loop` 并打开 `sie.SSIE`/`sie.STIE`（SSI 的实际跨核投递仍受固件未委托限制，见 `os/arch/src/riscv64/smp.rs` 的 SWI 取证注）；riscv64 还只清 `sip.SSIP` 而不应用挂起的调度标志；三架构的唤醒入口已接 `context_stop_idle`（清 `cpu_is_idle` + `restart_local_timer`，x86 LVT 解屏蔽+ICR、arm64 rearm+CNTP enable、riscv STIE+SBI 重装；BSP 保持 PIT）——riscv64 另缺 `os/arch/src/riscv64/cache_sync.rs` 记录的远端 `fence.i` 前置。现状是"生产者已在、aarch64/riscv 接收腿与 AP 进环已接、唤醒后定时器重启已接、riscv flag/fence 与 READY ack 未齐、钳制兜底"，不是"没有这半段"。

**缺口二：PREEMPTIBLE 门的现状形状。** §3.2 的对账给出装置，这里给"哪些进程真的带位"的置位面。位值与模板（C 侧逐字）：

```c
// minix3/minix/include/minix/priv.h —— 逐字引用（44-49 行，未删节）
#define TSK_F     (SYS_PROC)                       /* other kernel tasks */
#define SRV_F     (SYS_PROC | PREEMPTIBLE)         /* system services */
#define DSRV_F    (SRV_F | DYN_PRIV_ID)            /* dynamic system services */
#define RSYS_F    (SRV_F | ROOT_SYS_PROC)          /* root sys proc */
#define VM_F      (SYS_PROC | VM_SYS_PROC)         /* vm */
#define USR_F     (BILLABLE | PREEMPTIBLE)         /* user processes */
```

（位值 `PREEMPTIBLE 0x002` 定义在 `minix3/minix/include/minix/const.h:PREEMPTIBLE`；Rust 侧同值同模板：`os/kernel/src/capability.rs` 的 `ProcessCapability::PREEMPTIBLE = 0x0000_0002` 与 `TSK_F`/`SRV_F`/`USR_F` 组合常量。）

读模板：用户进程（`USR_F = BILLABLE | PREEMPTIBLE`）与系统服务（`SRV_F = SYS_PROC | PREEMPTIBLE`）带位；内核任务（`TSK_F = SYS_PROC`）不带；动态系统服务（`DSRV_F`）以 `SRV_F` 为底故也带。再看"谁的 `p_scheduler` 非空"：模板族的调度者槽定义在 `minix3/minix/include/minix/priv.h:DSRV_SCH`——动态系统服务的调度者指向 SCHED 服务器进程号，用户进程的调度者槽是 `NONE`（`USR_SCH`），系统服务的调度者是内核（`SRV_SCH`）。两个面相交，notify 分支（§1.3 左支）的真实可达面 = **被 SCHED 服务器接管且带 PREEMPTIBLE 位的进程**——运行期主体是**用户进程**：PM 在 boot（`sched_init` 对非特权进程调 `sched_start`）与 fork（子进程 `mp_scheduler = SCHED_PROC_NR`）两条路径把用户进程交给 SCHED，SCHED 经 `sys_schedctl` 让内核 `p->p_scheduler = caller`；动态系统服务（`DSRV_SCH` 槽）只是其中一类。模板槽口径与运行期接管路径两笔合起来才是全貌。Rust 侧装置已对齐（§3.2）：`check_quantum` ← 调度主循环是生产路径，`preemptible()` 查的是特权表。抢占门的另一半（§2.1 的 enqueue 内嵌判定）在 Rust 的现状是"门在、读不到当前进程时跳过"：`os/kernel/src/proc_table.rs:fn sched_enqueue` 自己读 CPU 本地的 `proc_ptr` 取当前进程（C 同位行为），SMP 状态未安装（早期启动、宿主测试）或 `proc_ptr` 还是空时跳过评估——跳过即"不抢占"。**登记**：C 侧不存在这个场景——`bsp_finish_booting()` 早期就把 `proc_ptr` 置为 idle（`main.c:57`）、`kmain()` 再兜底（`main.c:259-261`，注释原文 "scheduling functions depend on proc_ptr pointing somewhere."），enqueue 的抢占臂还带 `assert(p)`；Rust 的空置跳过是宿主/早启窗口的**防御性新增**（无 C 对位）。

**已闭合的链路：niced。** C 的链路三段：SYS_SCHEDULE 消息带的 niced 字段（`minix3/minix/kernel/system/do_schedule.c:do_schedule` 里 `niced = !!(m_ptr->m_lsys_krn_schedule.niced)`）→ `sched_proc` 置/清 `MF_NICED`（`minix3/minix/kernel/system.c:sched_proc`）→ 时钟记账分类时消费（`minix3/minix/kernel/arch/i386/arch_clock.c` 的 counter 选择链：`MF_NICED` → `CP_NICE`）。Rust 侧现状：前两段已端到端接通——`os/kernel/src/syscall.rs` 的 `dispatch_schedule` 读线上值做布尔强转（C 那行的直译），`os/kernel/src/sched.rs:fn sched_proc` 的 Step 8 置/清 `MiscFlagsBits::NICED`，并有测试钉住线上位到标志位的传递（`os/kernel/src/syscall.rs` 的 `test_dispatch_schedule_niced_wire_bit_sets_mf_niced`）。第三段今天已闭合：`os/kernel/src/clock.rs:fn classify_cpu_state`（读 `MF_NICED` → `CP_NICE` 桶）由 `account_process_stop` 在**生产路径**调用（中断入口/idle/`finish_and_restore` 三站，`os/kernel/src/lib.rs`），状态桶经 `account_tsc_per_state` 落账——线上位 → 进程标志 → 记账分类三段全通。C 的 counter 选择链在 Rust 换了形状（分类器 + 状态桶），消费端接口（GET_CPU_TICKS 一族）的展示面归 `17-clock-timer`。

边界一句话：单 CPU 现状下缺口一与缺口二不可观察——cpu 恒 0、SCHED 接管的进程尚未参战；niced 链已闭合（见上段）。把它们读丢的代价在将来：缺口一若不登记，读者会把每 CPU 队列读成"已支持多核负载均衡"，把 AP 队列当活路径排障；缺口二若不登记，notify 分支会被两种误读夹住——要么当成所有用户进程的通用路径、要么当成只服务动态系统服务的窄路径。

### 3.4 处置表（→Ch2 各节）

| C 侧（锚点） | 去向 | Rust 侧（锚点） | 说明 |
|---|---|---|---|
| `minix3/minix/kernel/proc.h:RTS_SET`/`:RTS_UNSET` 宏 | 改型 | `os/kernel/src/proc_table.rs:fn rts_set`/`:fn rts_unset`（§4.2） | 宏展开 → 方法；跃迁检测同形，另增 `is_in_scheduler` 守卫 |
| `minix3/minix/kernel/proc.h:RTS_SETFLAGS` | 改型 | `os/kernel/src/proc_table.rs:fn rts_setflags` | 整体赋值语义（不做跃迁检测）+ 先出队半；由 `test_rts_setflags_wipes_other_bits` 钉住（§4.2 未逐字引） |
| `minix3/minix/kernel/proc.c:enqueue` | 拆分 | `os/kernel/src/proc_table.rs:fn sched_enqueue`/`:fn sched_enqueue_with`（§4.2） | 队列半归 `Scheduler` 纯操作，字段半与抢占判定在包装层；SMP idle 唤醒生产者已在（`send_sched_ipi` + Release fence），接收侧未齐、BSP 钳制兜底（§3.3 缺口一） |
| `minix3/minix/kernel/proc.c:enqueue_head` | 拆分 | `os/kernel/src/proc_table.rs:fn sched_enqueue_head`（§4.2） | 同上拆分；`dequeues--`/`preempted++` 保留 |
| `minix3/minix/kernel/proc.c:dequeue` | 拆分 | `os/kernel/src/proc_table.rs:fn sched_dequeue`（§4.2） | 查找式摘链 → 切片链手术；C 的"未命中静默"改为 panic（§4.1 披露） |
| `minix3/minix/kernel/proc.c:pick_proc` | 保留 + 拆分 | `os/kernel/src/sched.rs:fn pick_proc` + `os/kernel/src/lib.rs:fn pick_and_bill`（10 篇 §4.3） | 扫描选人归 `Scheduler`；`bill_ptr` 侧效应拆到包装层 |
| `minix3/minix/kernel/proc.c:proc_no_time` | 保留 | `os/kernel/src/proc_table.rs:fn sched_proc_no_time`（§3.2/§4.2） | 判据两侧一致（§3.2 核对） |
| `minix3/minix/kernel/proc.c:notify_scheduler` | 保留 | `os/kernel/src/proc_table.rs:fn notify_scheduler` | `mini_send` → `IpcEngine::send`；失败同样 panic |
| `minix3/minix/kernel/system.c:sched_proc` | 改型 | `os/kernel/src/sched.rs:fn sched_proc`（§4.2） | `-1` 哨兵 → `Option`；errno 返回 → `SchedProcError` 枚举；BSP 钳制为 C2C 止损层（§3.3 缺口一） |
| `minix3/minix/kernel/system/do_schedule.c:do_schedule`/`do_schedctl.c:do_schedctl` | 保留 | `os/kernel/src/syscall.rs` 的对应 dispatch（niced 半见 §3.3 缺口三） | 身份门/收权门同形 |
| C 二级指针扫描（`struct proc **xpp`） | 消失 | `dequeue_from_queue` 的 `&mut [KProcess]` 切片链手术（§3.1） | 裸指针运算 → 借用检查的边界内等价物 |
| `#if DEBUG_SANITYCHECKS` 的 `runqueues_ok_local()`（`minix3/minix/kernel/proto.h`：非 SMP 构建映射到 `runqueues_ok`，SMP 构建映射到 `runqueues_ok_cpu(cpuid)`；本体 `minix3/minix/kernel/debug.c:runqueues_ok_cpu`） | 改型 | `os/kernel/src/debug.rs:fn runqueues_ok_cpu`（§4.3） | 完整移植（头尾配对、尾的 next 为空、成员可运行且优先级对号、无重复挂队、可运行者必在队）+ 三个包装层尾部的显式断言点，由 `sanity-checks` feature 门控；另配 debug 构建的 `debug_assert` 散点（`pick_proc` 队首可运行等） |
| `p_accounting`（`enter_queue`/`time_in_queue`/`dequeues`/`preempted`） | 保留 + 补齐 | `os/kernel/src/proc.rs:struct Accounting`（§4.3） | 另含 `ipc_sync`/`ipc_async`（notify 消息的字段源，§2.3） |
| `enter_queue` 写入对象 | 改型（修正） | `sched_enqueue_with` 的 Phase 4（§4.2） | C 写 `proc_ptr` 的账（§2.1 辨析），Rust 写被入队进程的账 |
| priority 上界 | 改型（修正） | `os/kernel/src/sched.rs` 的 Step 1 校验（§4.2；测试 `test_sched_proc_priority_overflow_rejected`） | C 判 `> NR_SCHED_QUEUES` 放行 16（随后 `enqueue` 以 `rdy_head[16]` 越界索引——C 的 off-by-one 缺陷）；Rust 收紧到 `> MIN_USER_Q` 拒绝 16 |

表里"改型（修正）"的偏离逐处单独占行、不藏在"保留"里：记账写入对象（`enter_queue`）与 priority 上界（本表新增行）两处；niced 的偏离在 `do_schedule/do_schedctl` 行内注明（§3.3 缺口三，今天三段已闭合）。

---

## 4. 实现详解

### 4.1 Scheduler 纯操作走查（→§3.1）

四个纯操作按 §3.1 的拆分逐个对读。选人（对照 §2.2 的 `pick_proc`）：

```rust
// os/kernel/src/sched.rs:fn pick_proc —— 逐字引用（65-80 行，未删节）
    pub fn pick_proc(&self, procs: &[KProcess]) -> Option<ProcNr> {
        for q in 0..priority::NR_SCHED_QUEUES {
            if let Some(nr) = self.run_q_head[q] {
                let idx = nr_to_idx(nr);
                if let Some(proc) = idx.and_then(|i| procs.get(i)) {
                    debug_assert!(
                        proc.is_runnable(),
                        "pick_proc: head of queue {} not runnable",
                        q
                    );
                }
                return Some(nr);
            }
        }
        None
    }
```

与 C 同构处：从 0 号扫到 `NR_SCHED_QUEUES - 1`，第一个非空队列的队首即答案，全空返回 `None`。审计半在这里：`debug_assert!(proc.is_runnable(), "pick_proc: head of queue {} not runnable", q)`——C 的同名断言是 `assert(proc_is_runnable(rp))`（§2.2），Rust 用 debug 构建断言；断言串本身是 §6.1 的判别读数。多出的一个形状是 `procs.get(i)` 的防御性取槽：队列里挂的 `ProcNr` 换算下标越界时断言静默跳过，不让选人路径 panic 在一个已损坏的队列上（损坏的暴露交给 §4.3 的 `runqueues_ok_cpu`）。

头插（对照 §2.2 前的 `enqueue_head` 插入段）：

```rust
// os/kernel/src/sched.rs:fn enqueue_queue_head —— 逐字引用（105-115 行，未删节）
    pub fn enqueue_queue_head(&mut self, nr: ProcNr, q: usize) {
        match self.run_q_head[q] {
            None => {
                self.run_q_head[q] = Some(nr);
                self.run_q_tail[q] = Some(nr);
            }
            Some(_) => {
                self.run_q_head[q] = Some(nr);
            }
        }
    }
```

空队列与 C 同（头尾都指自己）；非空时只挪头指针。注意它与尾插的**回执不对称**：头插没有 `EnqueueInfo` 返回值——§3.1 的取舍表说过，头插需要的接链信息（旧队首）在被插入进程自己的 `p_nextready` 里就能表达（新头指向旧头），包装层拿 `queue_head(q)` 读一下旧值即可（`os/kernel/src/proc_table.rs:fn sched_enqueue_head` 先读旧头再调本原语），无需回执。

出队的包装层入口（对照 §2.2 的 `dequeue`）：

```rust
// os/kernel/src/proc_table.rs:fn sched_dequeue —— 逐字引用（1016-1046 行，未删节）
    pub fn sched_dequeue(&mut self, nr: ProcNr, cpu_id: CpuId) {
        let idx = Self::sched_idx(cpu_id);
        let q = self.get(nr).map_or(0, |p| p.get_priority().get() as usize);

        // Phase 1: remove from queue + update linked list
        self.sched[idx].dequeue_from_queue(nr, q, &mut self.procs);

        // Phase 2: accounting
        let tsc = read_tsc();
        // R-15 (2026-08-12): INVARIANT: `nr` was validated by the caller and is
        // present in the table (dequeue only operates on existing processes);
        // `get_mut()` cannot return None.
        let acc = &mut self.get_mut(nr).unwrap().p_accounting;
        acc.record_dequeue(tsc);

        // R-15 (2026-08-12): INVARIANT: same as above — `nr` is an in-table
        // ProcNr; `get_mut()` cannot return None.
        self.get_mut(nr).unwrap().p_dequeued.store(
            get_monotonic(),
            Ordering::Release,
        );

        // U-2 / C proc.c:1778 (dequeue tail): assert(runqueues_ok_local()).
        #[cfg(all(feature = "sanity-checks", not(test)))]
        if let Some(smp) = unsafe { crate::try_smp_state() } {
            assert!(
                crate::debug::runqueues_ok_cpu(&*smp, self, crate::current_cpu_id()),
                "runqueues_ok: queue invariant broken after sched_dequeue"
            );
        }
    }
```

三段与 C 逐行对上：Phase 1 调纯操作做链手术（`dequeue_from_queue`，§3.1 已逐字引）；Phase 2 记账（`record_dequeue` 结算排队时长 + `p_dequeued` 写单调时钟，§4.3 对账字段）；尾部是 `sanity-checks` feature 门控的 `runqueues_ok_cpu` 断言（§4.3）。一个两侧行为**不同**的点必须单独披露：C 的摘链是查找式的，注释明说"找不到就落空静默通过"（§2.2 的被杀信号辨析）；Rust 的 `dequeue_from_queue` 在 `!found` 时 `panic!("dequeue: process not found in its priority queue")`。C 容忍的异常状态在 Rust 被当成内核 bug 当场暴露——审计更严的一面是故障提前到出错点；代价是若真出现"被杀信号竞态把进程弄到既不可运行又不在队列"的状态，C 静默续行、Rust 停机。当前生产路径上，摘链只由 RTS 包装层在跃迁检测成立后调用（§4.2），"位已置却不在队列"本就是 INV-1 破坏态，panic 的方向与 INV-1 的审计语义一致。第二处差异：Rust 在摘链后把被摘进程自己的 `p_nextready` 清零（`store(NONE_PROC_NR)`），C 不写这一笔（被摘节点的旧值留到下次入队才被覆盖）——Rust 的清零让"不在链上"在字段层面也可审计。

### 4.2 包装层与两分支走查（→§3.2/§3.3）

包装层是 INV-1 装置的 Rust 本体。先看置位侧（对照 §1.1 的 `RTS_SET` 宏）：

```rust
// os/kernel/src/proc_table.rs:fn rts_set —— 逐字引用（441-456 行，未删节）
    pub fn rts_set(&mut self, nr: ProcNr, flags: RtsFlagsBits) {
        let was_runnable = self.get(nr).is_some_and(|p| p.is_runnable());
        if let Some(p) = self.get_mut(nr) {
            p.p_rts_flags.set(flags);
        }
        let is_runnable = self.get(nr).is_some_and(|p| p.is_runnable());
        if was_runnable && !is_runnable
            && self.is_in_scheduler(nr) {
                // C uses `get_cpu_var(rp->p_cpu, run_q_head)` — the process's
                // assigned CPU determines which per-CPU queue to dequeue from.
                let cpu_id = self.get(nr)
                    .map(|p| CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire)))
                    .unwrap_or(CpuId::BSP);
                self.sched_dequeue(nr, cpu_id);
            }
    }
```

宏的三步（存旧值、置位、跃迁检测）展开成方法的四步：读旧态、写位、读新态、跃迁成立且进程还在队列里才出队。多出的守卫是 `is_in_scheduler(nr)`——C 的 `dequeue` 靠查找落空来容忍"位已置但不在队列"，Rust 的摘链会 panic（§4.1），所以包装层先做一次保守的成员检查（扫该进程所属 CPU 的 16 条队列头链，`os/kernel/src/proc_table.rs:fn is_in_scheduler`；注释自述保守性：纯队尾成员要靠扫到它才能确认）。清位侧对称（对照 `RTS_UNSET` 宏）：

```rust
// os/kernel/src/proc_table.rs:fn rts_unset —— 逐字引用（583-589 行，出队/入队半；省略 560-582 行的标志清写与调试探针——标志清写是跃迁半的前置、队列动作为其下游）
        if !was_runnable && is_runnable {
            let cpu_id = self.get(nr).map_or(CpuId::BSP, |p| {
                CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire))
            });
            self.sched_enqueue(nr, cpu_id);
        }
    }
```

入队按 `p_sched.cpu` 定位队列（C 的 `get_cpu_var(rp->p_cpu, ...)` 对位）——每 CPU 队列的"挂到被指派 CPU"（§1.2）在这两段里各落一次。

入队包装层的完整形状（对照 §2.1 的 `enqueue` 五段）：

```rust
// os/kernel/src/proc_table.rs:fn sched_enqueue —— 逐字引用（859-873 行，未删节；注释含仓库内部编号，按原文引用）
    pub fn sched_enqueue(&mut self, nr: ProcNr, cpu_id: CpuId) {
        // C proc.c:1633 — per-CPU anchor read; C takes no lock for it
        // either, and the anchor itself is the dispatch identity.
        let current = unsafe { crate::try_smp_state() }
            .and_then(|smp| smp.cpu_local(crate::current_cpu_id()))
            .and_then(|l| l.proc_ptr);
        // C proc.c:1638 — the gate is the CURRENT process's PREEMPTIBLE
        // privilege flag. `current` is `Some` only once the scheduler
        // hands out contexts (post-S-6), where every enqueue path runs
        // under the BKL; [`crate::priv_table`] is the matching
        // dual-context accessor.
        let current_preemptible =
            current.is_some_and(|cur_nr| self.preemptible(cur_nr, unsafe { crate::priv_table() }));
        self.sched_enqueue_with(nr, current.map(|c| (c, current_preemptible)), cpu_id)
    }
```

当前进程与它的 PREEMPTIBLE 标志从 CPU 本地状态取——C 的 `enqueue` 自己读 `get_cpulocal_var(proc_ptr)`（§2.1 第三段），这个包装层镜像了同一行为：状态未安装时读到 `None`，抢占评估跳过（§3.3 缺口二的现状形状）。带参核心是四段式：

```rust
// os/kernel/src/proc_table.rs:fn sched_enqueue_with —— 逐字引用（Phase 1-2，890-914 行，未删节；注释含仓库内部编号，按原文引用）
        let q = self.get(nr).map_or(0, |p| p.get_priority().get() as usize);
        debug_assert!(q < 16, "sched_enqueue: priority out of range");

        // Phase 1: update queue arrays on the process's assigned CPU
        // (C: `get_cpu_var(rp->p_cpu, ...)`, proc.c:1614).
        let info = self.sched[Self::sched_idx(cpu_id)].enqueue_queue_tail(nr, q);

        // Phase 2: update process fields
        {
            let procs = &mut self.procs;
            // Clear p_nextready of enqueued process
            // R-15 (2026-08-12): INVARIANT: `nr` is a caller-validated ProcNr
            // already resolved from the table; `nr_to_idx` cannot fail.
            let nr_idx = nr_to_idx(nr).unwrap();
            procs[nr_idx].p_nextready.store(NONE_PROC_NR, Ordering::Relaxed);

            // Link old tail to new process
            if let Some(tail_nr) = info.old_tail {
                // R-15 (2026-08-12): INVARIANT: `tail_nr` came from
                // `enqueue_queue_tail`, which only returns ProcNrs already
                // present in the table; `nr_to_idx` cannot fail.
                let tail_idx = nr_to_idx(tail_nr).unwrap();
                procs[tail_idx].p_nextready.store(nr.0, Ordering::Relaxed);
            }
        }
```

Phase 1 调纯操作拿回执，Phase 2 消费回执接链——§3.1 的回执数据流在这里落成代码：`info.old_tail` 是 `Some(tail)` 就把旧尾的 `p_nextready` 指向新进程，`None` 就什么都不接（空队列自建）。对照 C 的 `rdy_tail[q]->p_nextready = rp`：同一条链，C 在原语里就地写，Rust 由包装层拿着回执写。Phase 3 抢占门（对照 §2.1 的三条件链路图）：

```rust
// os/kernel/src/proc_table.rs:fn sched_enqueue_with —— 逐字引用（Phase 3，916-942 行，未删节；注释含仓库内部编号，按原文引用）
        // Phase 3: preemption check (only same CPU)
        //
        // C proc.c:1632-1645 — when the enqueue lands on the running
        // process's CPU and carries a higher priority (numerically lower
        // queue), the current process is preempted — but only when its
        // privilege flags say PREEMPTIBLE (proc.c:1638). The flag, not
        // the queue number, is what keeps kernel tasks (TSK_F) immune:
        // MAX_USER_Q == TASK_Q == 0 (config.h:67-68), so a queue number
        // alone cannot tell "user process legally at the top queue" from
        // "kernel task". The priority comparison also rules out
        // self-preemption (equal queue → not greater).
        if let Some((cur_nr, cur_preemptible)) = current {
            let (cur_prio, cur_cpu) = {
                // R-15 (2026-08-12): INVARIANT: `cur_nr` is the currently-running
                // process passed by the caller; it must be a valid in-table ProcNr,
                // so `get()` cannot return None.
                let cur = self.get(cur_nr).expect("sched_enqueue: invalid current nr");
                (
                    cur.get_priority().get(),
                    cur.p_sched.cpu.load(Ordering::Acquire),
                )
            };
            let new_prio = q as u8;
            if cur_cpu == cpu_id.raw() && cur_prio > new_prio && cur_preemptible {
                self.rts_set(cur_nr, RtsFlagsBits::PREEMPTED);
            }
        }
```

三条件依次落进一个 `if`：`cur_cpu == cpu_id.raw()`（本 CPU）、`cur_prio > new_prio`（数值比较方向与 C 一致——大者优先级低）、`cur_preemptible`（§3.2 的特权表追查，由 `sched_enqueue` 预先求好随元组传入）。命中即 `rts_set(cur_nr, PREEMPTED)`——递归进本族（§2.1 的网状调用关系在 Rust 同样成立：`rts_set` 的跃迁半会调 `sched_dequeue`）。Phase 4 记账是 §3.4 处置表里的修正行：`p_accounting.record_enqueue(tsc)` 写在**被入队进程** `nr` 的账本上——C 写的是 `proc_ptr` 的账本（§2.1 的辨析），`os/kernel/src/sched.rs` 的模块注释自称这一改修的是 MINIX3 的 bug；对照读法是：C 里 `enter_queue` 记在当前进程、下一次它被出队时结算，Rust 里记在被入队进程、它自己的下一次出队结算，账目主体与队列成员对齐。

量子耗尽两分支的被调侧（§3.2 已逐字引 `sched_proc_no_time` 与 `preemptible`，这里走折叠的入口）：

```rust
// os/kernel/src/proc_table.rs:fn check_quantum —— 逐字引用（1569-1582 行，未删节）
    pub fn check_quantum(
        &mut self,
        nr: ProcNr,
        priv_table: &crate::kpriv::PrivTable,
        section: &crate::smp::BklSection<'_>,
    ) -> bool {
        let has_time_left = self.get(nr).is_some_and(|p| {
            p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0
        });
        if !has_time_left {
            self.sched_proc_no_time(nr, priv_table, section);
        }
        self.get(nr).is_some_and(|p| p.is_runnable())
    }
```

三步折叠（§3.2 末段）：无剩余时间则调 `sched_proc_no_time`，最后统一重验可运行并返回——循环调用方拿 `false` 就回重排点（[10-switch-to-user.md](10-switch-to-user.md) §4.1 的循环侧消费）。C 侧对应散在 `switch_to_user` 拍五（`minix3/minix/kernel/proc.c` 的 416-428 行：耗尽判断 `proc_no_time` 调用与 `not_runnable_pick_new` 重验两段，`sed -n '416,428p'` 可复核）。

改参函数的 Rust 形状（对照 §2.3 的 `sched_proc` 与舞步时序图）。参数结构先看骨架：

```rust
// os/kernel/src/sched.rs:struct SchedParams —— 逐字引用（263-268 行，未删节）
pub struct SchedParams {
    pub priority: Option<u8>,
    pub quantum: Option<u32>,
    pub cpu: Option<u32>,
    pub niced: bool,
}
```

C 的 `-1` 哨兵（"保持现值"）被 `Option` 替代：`None` 保持、`Some(v)` 改写——哨兵值不再混进参数值域（C 里 `priority = -1` 合法而 `-2` 非法，值域里嵌着一个例外值；Rust 的例外值在类型外）。校验与舞步：

```rust
// os/kernel/src/sched.rs:fn sched_proc —— 逐字引用（签名，368-373 行，未删节）
pub fn sched_proc(
    table: &mut crate::proc_table::ProcessTable,
    nr: ProcNr,
    mut params: SchedParams,
) -> Result<(), SchedProcError> {
    use crate::proc::{MiscFlagsBits, RtsFlagsBits};
```

```rust
// os/kernel/src/sched.rs:fn sched_proc —— 逐字引用（校验段前半，382-386 行，未删节）
    // Step 1: validate priority range.
    // C: system.c:644-645:
    //   if ((priority < TASK_Q && priority != -1) || priority > NR_SCHED_QUEUES)
    //       return EINVAL;
    // In Rust: TASK_Q == 0, MIN_USER_Q == 15 (NR_SCHED_QUEUES-1).
```

```rust
// os/kernel/src/sched.rs:fn sched_proc —— 逐字引用（校验段后半，388-400 行，未删节；两段之间的 387 行是模块注释，其编号指向与本篇现行小节不一致的文档内引用，整行省略）
    // u8 is always >= 0, so we only check the upper bound.
    if let Some(v) = params.priority
        && v > priority::MIN_USER_Q {
            return Err(SchedProcError::InvalidArgument);
        }

    // Step 2: validate quantum range.
    // C: system.c:647-648:
    //   if (quantum < 1 && quantum != -1) return EINVAL;
    if let Some(v) = params.quantum
        && v < 1 {
            return Err(SchedProcError::InvalidArgument);
        }
```

**登记（上界口径差）**：C 的 `priority` 上界是 `NR_SCHED_QUEUES`（16）——`priority=16` 会被 C 放行并以 `rdy_head[16]` 越界索引（C 的 off-by-one 缺陷）；Rust 收紧到 15 拒绝（改型（修正），§3.4 表单独占行）。

```rust
// os/kernel/src/sched.rs:fn sched_proc —— 逐字引用（Step 4，459-469 行，未删节）
    // Step 4: preemption hint (RTS_NO_QUANTUM toggle).
    // C: system.c:668-677 sets RTS_NO_QUANTUM if the process is runnable
    // and the parameters are changing. We track this flag in the Rust
    // translation, but actual reschedule is handled by the scheduler.
    // C: system.c:668-677 — only a RUNNABLE process is preempted
    // (RTS_SET NO_QUANTUM dequeues it via the scheduler-aware wrapper);
    // a blocked process keeps its flags untouched here. `runnable` was
    // computed for the Step 3.5 migration guard.
    if runnable {
        table.rts_set(nr, RtsFlagsBits::NO_QUANTUM);
    }
```

```rust
// os/kernel/src/sched.rs:fn sched_proc —— 逐字引用（Step 8-9，510-528 行，未删节；Step 9 注释含仓库内部编号，按原文引用）
    // Step 8: apply niced flag.
    // C: system.c:695-698:
    //   if (niced) p->p_misc_flags |= MF_NICED;
    //   else p->p_misc_flags &= ~MF_NICED;
    if let Some(p) = table.get_mut(nr) {
        if params.niced {
            p.p_misc_flags.set(MiscFlagsBits::NICED);
        } else {
            p.p_misc_flags.clear(MiscFlagsBits::NICED);
        }
    }

    // Step 9 (D-52, C system.c:698): RTS_UNSET via the scheduler-aware
    // wrapper — a queued process is re-enqueued (tail) with the NEW
    // parameters; a blocked process just has the flag cleared.
    table.rts_unset(nr, RtsFlagsBits::NO_QUANTUM);

    Ok(())
}
```

（Step 编号在引文里跳档：引的是 Step 1/2/4/8/9——Step 3 是 CPU 范围与就绪校验、Step 3.5 是迁移门、Step 5-7 是字段改写段，未逐字引；完整 Step 注释见 `os/kernel/src/sched.rs:fn sched_proc`。）舞步与 §2.3 时序图逐拍对应：Step 4 对可运行进程 `rts_set(nr, NO_QUANTUM)`——§4.2 的置位侧跃迁成立，进程从旧优先级队列摘下；Step 5-7 改字段（§2.3 的中间段）；Step 8 置/清 NICED 位（§3.3 的 niced 链，三段全通）；Step 9 `rts_unset(nr, NO_QUANTUM)`——跃迁反向成立，进程按**新**优先级挂回队列。对照读法有一处要如实记：C 在第一个 `if` 块之后还有**第二个** `if (proc_is_runnable(p)) RTS_SET(p, RTS_NO_QUANTUM)`（引文内可见）。按控制流它**恒假**——第一次置位已把进程变成不可运行，第二次检查必然不成立（迁移发生在第一处置位**之前**，不会把状态改回可运行）；按现码读是防御性重复/历史遗留，行为等价于只置位一次。Rust 侧没有这第二处（Step 4 单次置位）。迁移分支在当前生产路径不可达（§3.3 缺口一的 BSP 钳制把改派请求全部钳回原 CPU——单 CPU 上进程的归属 CPU `homed`（即 `p_sched.cpu`）与 `cpuid` 恒同，迁移门恒假）。

指派 CPU 的过渡钳制（§3.3 缺口一的装置）：

```rust
// os/kernel/src/sched.rs:fn clamp_cpu_to_bsp —— 逐字引用（306-309 行，未删节）
#[inline]
fn clamp_cpu_to_bsp(cpu: Option<u32>, bsp: u32) -> Option<u32> {
    cpu.map(|_| bsp)
}
```

一行纯函数：请求过校验（`validate_cpu_param`，越界 `EINVAL`/未就绪 `EBADCPU` 照 C 的 `CONFIG_SMP` 段）、然后一律改写成 BSP——`None`（保持现值）原样通过。它的语义边界在 §3.3 缺口一：接受请求、进程留 BSP，对位 C 非 SMP 构建对 cpu 参数的忽略。

### 4.3 记账与调试审计（→§2.2/§2.4）

C 的记账字段（§2.2/§2.3 用到的）在 Rust 是一个纯数据结构：

```rust
// os/kernel/src/proc.rs:struct Accounting —— 逐字引用（608-617 行，未删节）
/// Scheduling statistics structure.
#[derive(Debug)]
pub struct Accounting {
    pub enter_queue: AtomicU64,
    pub time_in_queue: AtomicU64,
    pub dequeues: AtomicU32,
    pub ipc_sync: AtomicU32,
    pub ipc_async: AtomicU32,
    pub preempted: AtomicU32,
}
```

字段对照表（记账口径：C 三件套 + notify 消息字段 vs Rust 六字段）：

| 字段 | C 侧写入点 | Rust 侧写入点 | 用途 |
|---|---|---|---|
| `enter_queue` | `enqueue`/`enqueue_head` 尾部（写入 `proc_ptr` 的账，§2.1 辨析） | `record_enqueue`（写入 `nr` 的账，§4.2 Phase 4） | 排队起点时间戳 |
| `time_in_queue` | `dequeue` 的 TSC 差结算 | `record_dequeue` | notify 消息的 `acnt_queue` |
| `dequeues` | `dequeue` 的 `++` / `enqueue_head` 的 `--` | `record_dequeue` / `sched_enqueue_head` | notify 消息的 `acnt_deqs` |
| `preempted` | `enqueue_head` 的 `++` | `sched_enqueue_head` 的 `fetch_add` | notify 消息的 `acnt_preempt` |
| `p_dequeued`（进程字段，非账本） | `dequeue` 写 `get_monotonic()` | `sched_dequeue` 写 `get_monotonic()` | ps(1) 展示 |
| `ipc_sync`/`ipc_async` | IPC 路径记账 | `record_ipc_sync`/`record_ipc_async` | notify 消息的 `acnt_ipc_*` |

出入队记账的结算函数（对照 §2.2 的 `time_in_queue` 结算段）：

```rust
// os/kernel/src/proc.rs:fn record_enqueue/record_dequeue —— 逐字引用（657-669 行，未删节）
    pub fn record_enqueue(&self, tsc: CpuCycles) {
        self.enter_queue.store(tsc, Ordering::Release);
    }

    pub fn record_dequeue(&self, tsc: CpuCycles) {
        let enter = self.enter_queue.load(Ordering::Acquire);
        if enter > 0 && tsc > enter {
            let delta = tsc - enter;
            self.time_in_queue.fetch_add(delta, Ordering::AcqRel);
        }
        self.enter_queue.store(0, Ordering::Release);
        self.dequeues.fetch_add(1, Ordering::AcqRel);
    }
```

与 C 的结算段同构，但守卫多一个条件：C 是 `if (rp->p_accounting.enter_queue)`（非零即结算），Rust 是 `if enter > 0 && tsc > enter`。第二个条件挡的是无符号下溢：TSC 读取理论上可乱序（跨核不同步、虚拟机环境——C 的注释自己承认"not all that accurate on virtual machines"），一旦 `tsc < enter`，C 的 `tsc - enter` 在 `u64_t` 上回绕成一个天文数字直接累进 `time_in_queue`；Rust 的守卫把这种情况判为零收益跳过。反向推演：去掉 `tsc > enter` 守卫，一次时钟回读噪声就能把 `time_in_queue` 抬到需要几十年才能跑完的量级——notify 消息里 `cpu_time_2_ms` 换算出来的 `acnt_queue` 从此失真，调度服务器按这个排队时长做降级决策会把误判放大成策略行为。

队列不变量的审计本体（对照 §2.4 第三环）：

```rust
// os/kernel/src/debug.rs:fn runqueues_ok_cpu —— 逐字引用（文档注释与签名，35-51 行，未删节）
/// C: `runqueues_ok_cpu(cpu)` — debug.c:16-107.
///
/// Checks:
/// 1. Every non-empty queue has both head and tail set.
/// 2. Tail's `p_nextready` is None.
/// 3. Every process on a queue is runnable, not SLOT_FREE, and has the
///    correct priority for that queue.
/// 4. No process appears on two queues (checked via `p_found` marker).
/// 5. Every runnable process in the proc table is on some queue.
///
/// Returns `true` if all invariants hold, `false` otherwise.
/// On failure, prints a diagnostic message to the early console.
pub fn runqueues_ok_cpu(
    _smp_state: &SmpState,
    proc_table: &ProcessTable,
    cpu: CpuId,
) -> bool {
```

C 的本体在 `minix3/minix/kernel/debug.c:runqueues_ok_cpu`（92 行），调用点挂在 `enqueue`/`enqueue_head`/`dequeue` 三个函数尾部、由 `#if DEBUG_SANITYCHECKS` 门控（§2.1/§2.2 的引用块尾部各有一段）；`runqueues_ok_local` 是个按构建形态换名的宏——非 SMP 构建映射到全机检查、SMP 构建映射到本 CPU 检查（`minix3/minix/kernel/proto.h`）。Rust 的对位是同一条链：完整移植的 `runqueues_ok_cpu`（检查项清单见上面引用块的文档注释，与 C 的五项一致）挂在 `sched_enqueue_with`/`sched_enqueue_head`/`sched_dequeue` 三个包装层尾部，由 `sanity-checks` feature 门控（`os/kernel/Cargo.toml` 的 feature 表；生产默认关闭，显式开启时每次队列操作后自检）。叠加的第二层是 debug 构建的 `debug_assert` 散点：`Scheduler::pick_proc` 的队首可运行断言（§4.1）、`Scheduler` 队列访问器与 `sched_idx` 的范围断言、优先级范围断言——INV-1 的运行时审计在 C 是一组条件编译，在 Rust 是"debug 构建常开 + sanity-checks 显式开"两档。

---

## 5. 测试要点

### 5.1 清单与读数（→Ch4）

**全套件口径（权威读数）**——内核宿主测必须显式带形态：

```bash
$ cargo test -p minix-kernel --features mock
     Running unittests src/lib.rs
test result: ok. 832 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out; finished in 7.58s
     Running tests/boot_integration.rs
test result: ok. 3 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.82s
   Doc-tests minix_kernel
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

三段读数全绿（主测试二进制读数随测试集增长，以现跑为准）；下文所有"通过/失败"的读数一律以这套件为准。理由在 §5.3：本篇的队列相关测试有两条实测的过滤运行陷阱，单过滤跑会因共享全局态给出与全套件不同的结果。

队列侧测试清单（复算：`grep -c 'fn test_' os/kernel/src/sched.rs` = 29；`grep -c 'fn test_' os/kernel/src/proc_table.rs` = 41，其中队列侧 7 条）。sched.rs 的 29 条全部落在本篇三个主题上，每组钉点一句话：

| 组 | 测试（数量） | 各自钉住什么 |
|---|---|---|
| 入队/出队/选人（10 条） | `test_enqueue_empty_queue` | 空队尾插：头尾同指，`p_nextready` 终结 |
| | `test_enqueue_non_empty_queue` | 尾插接链：旧尾 `p_nextready` 指向新进程 |
| | `test_enqueue_head` | 头插：新头 `p_nextready` 指旧头（夹具先给足余量） |
| | `test_enqueue_preempts_preemptible_current` | 抢占门三条件全过 → 当前进程置 `PREEMPTED`（§5.2 细读一） |
| | `test_enqueue_spares_non_preemptible_current` | 特权门：无 `PREEMPTIBLE` 位的当前进程不置位 |
| | `test_enqueue_preemption_skipped_for_other_cpu` | 同 CPU 条件：异 CPU 入队不触发本 CPU 抢占 |
| | `test_enqueue_shell_without_cpu_local_still_enqueues` | 无 SMP 状态时入队本体照常、抢占评估跳过（§3.3 缺口二的宿主形态） |
| | `test_dequeue_only_process` | 经 `rts_set`（置 `PROC_STOP`）自动出队后队列空 |
| | `test_pick_proc_empty` | 全空返回 `None` |
| | `test_pick_proc_highest_priority` | 扫描序：0 号队列的进程先于 7 号队列被选中 |
| 量子耗尽两分支（4 条） | `test_proc_no_time_kernel_scheduled` | 无调度者 → 续量：`cpu_time_left` 重置为正 |
| | `test_proc_no_time_user_scheduled_preemptible` | 有调度者 + `USR_F` → `NO_QUANTUM` 置位（通知路径） |
| | `test_proc_no_time_priority_zero_preemptible_notifies_scheduler` | 0 号队列 + `USR_F` → 仍通知——钉住"位判据不是优先级判据"（§5.2 细读二） |
| | `test_proc_no_time_non_preemptible_priv_renews_quantum` | `TSK_F` 无位 → 续量、不置 `NO_QUANTUM` |
| 改参 `sched_proc`（15 条） | `test_sched_proc_priority_change` 等 | 校验四门（越界/零量子/CPU 范围与就绪）、`Option` 哨兵语义（None 保持现值）、量子更新重置剩余时间、NICED 位置/清、BSP 钳制幂等与 None 透传、错误码到 errno 的映射、可运行进程改参后的出队-重入队往返（`test_sched_proc_runnable_reenqueue_on_update`） |

proc_table.rs 的 7 条队列侧：`test_check_quantum_with_time_left_is_noop`（有余量时折叠入口不动分支）、`test_check_quantum_exhausted_delegates_to_no_time`（耗尽即调 `sched_proc_no_time`）、`test_rts_set_unset`/`test_rts_set_unset_multiple_flags`/`test_rts_set_idempotent`（跃迁检测与重复置位的空操作）、`test_rts_setflags_wipes_other_bits`（整体赋值语义 + 先出队半）、`test_set_bill_to_idle`（IDLE 槽 `enter_queue` 清零的初始记账态）。`test_vm_memreq_*` 四条也操作队列（重排/摘链），但其 C 语义归属在 [09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3 的 MEMREQ 通道（重排/摘链），不计入本篇清单口径。

### 5.2 关键测试细读（→§3.1/§3.2）

细读一：抢占门的注入式测试（`os/kernel/src/sched.rs` 的 `test_enqueue_preempts_preemptible_current`）：

```rust
// os/kernel/src/sched.rs —— 逐字引用（617-641 行，未删节）
    #[test]
    fn test_enqueue_preempts_preemptible_current() {
        // C proc.c:1638 — the gate is the current process's PREEMPTIBLE
        // privilege flag. A user process at queue 0 is legally preemptible
        // (MAX_USER_Q == TASK_Q == 0, config.h:67-68; USR_F carries
        // PREEMPTIBLE), so a higher-priority enqueue must preempt it —
        // the case the old `priority != 0` approximation got wrong.
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        if let Some(p) = priv_table.get_mut(0u16) {
            p.flags.s_flags |= crate::capability::ProcessCapability::USR_F;
        }
        make_runnable(&mut table, ProcNr(0), priority::USER_Q);
        table.get_mut(ProcNr(0)).unwrap().priv_id = Some(0u16);
        make_runnable(&mut table, ProcNr(1), priority::MAX_USER_Q);
        table.sched_enqueue_with(ProcNr(1), Some((ProcNr(0), true)), CpuId::BSP);
        assert!(
            table
                .get(ProcNr(0))
                .unwrap()
                .p_rts_flags
                .is_set(RtsFlagsBits::PREEMPTED),
            "preemptible current must be preempted by a higher-priority enqueue"
        );
    }
```

断言解码：夹具给 0 号特权槽位填 `USR_F`（带 `PREEMPTIBLE`）、把进程 0 放队列 7（`USER_Q`）当"当前进程"、再把进程 1 以队列 0（`MAX_USER_Q`）入队——通过 `sched_enqueue_with` 的 `Some((ProcNr(0), true))` 显式注入"当前进程与它的可抢占标志"。断言只查一件事：进程 0 的 `p_rts_flags` 带上 `PREEMPTED`。变红条件逐条：Phase 3 的同 CPU 条件被删（此测试的夹具里两者同在 BSP，删了它另有 `test_enqueue_preemption_skipped_for_other_cpu` 兜住异 CPU 反例）、优先级比较方向写反（若写成 `cur_prio < new_prio`：7 < 0 为假，不置位）、`rts_set(PREEMPTED)` 那一行没执行。这个测试的存在还钉住一个历史教训（注释里写明）：曾有一版实现用"优先级非 0"近似 PREEMPTIBLE，队列 0 的用户进程（`MAX_USER_Q == TASK_Q == 0`，合法登顶）被永久挡在抢占之外。

细读二：位判据对优先级判据的钉子（`test_proc_no_time_priority_zero_preemptible_notifies_scheduler`）：

```rust
// os/kernel/src/sched.rs —— 逐字引用（799-823 行，未删节）
    /// MAX_USER_Q == TASK_Q == 0（config.h:67-68）：优先级 0 的用户进程
    /// 合法可抢占。旧实现以 `priority != 0` 近似 PREEMPTIBLE，把这类进程
    /// 永久挡在 NO_QUANTUM 之外（SCHED 接管后经 START/NICE 可达优先级 0，
    /// 从此被内核无限续量）。C 的判据是 priv 标志位（proc.c:1895）——
    /// USR_F 携带 PREEMPTIBLE（priv.h:49），本测试钉住修正后的行为。
    #[test]
    fn test_proc_no_time_priority_zero_preemptible_notifies_scheduler() {
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        make_runnable(&mut table, ProcNr(0), priority::MAX_USER_Q);
        assign_priv_flags(&mut table, &mut priv_table, ProcNr(0),
            crate::capability::ProcessCapability::USR_F);
        table.get_mut(ProcNr(0)).unwrap().p_sched.scheduler = Some(proc_nr::SYSTEM);
        table.get_mut(ProcNr(0)).unwrap().p_sched.quantum.cpu_time_left.store(0, Ordering::Release);
        table.get_mut(ProcNr(0)).unwrap().p_sched.quantum.size_ms.store(200, Ordering::Release);

        table.sched_enqueue_with(ProcNr(0), None, CpuId::BSP);
        let section = crate::smp::bkl_lock_section();
        table.sched_proc_no_time(ProcNr(0), &priv_table, &section);
        crate::smp::bkl_unlock();

        assert!(table.get(ProcNr(0)).unwrap().p_rts_flags.is_set(RtsFlagsBits::NO_QUANTUM),
            "priority-0 + USR_F（PREEMPTIBLE）必须通知调度者，实际 {:?}",
            table.get(ProcNr(0)).unwrap().p_rts_flags);
    }
```

断言解码：夹具把进程 0 放**队列 0**（`MAX_USER_Q`）、挂 `USR_F` 特权、调度者设为 SYSTEM 槽，量子清零后调 `sched_proc_no_time`；断言 `NO_QUANTUM` 位已置——即通知分支被走到。变红条件：`preemptible` 判据退回"优先级非 0"近似——队列 0 的进程被判不可抢占、走进续量分支，`NO_QUANTUM` 不置位（正是该行注释描述的那个近似判据的行为）。另一方向的错误（特权表查空误判为 true）不在本条的守卫面内——由姊妹测试 `test_proc_no_time_non_preemptible_priv_renews_quantum` 用 `TSK_F` 无位夹具从反方向钉住。

### 5.3 测试边界与诚实披露（→Ch3）

宿主测不到的面，按缺口列：

- **跨 CPU 入队**：生产者已在（`os/kernel/src/proc_table.rs:fn sched_enqueue_with` 的 `cpu_is_idle` 检查 + `send_sched_ipi` + Release fence），但接收侧三件未齐、BSP 钳制使进程不落 AP，宿主测仍构造不出"目标核真的被唤醒并取到该进程"的路径（§3.3 缺口一）。反证更新：`grep -rn 'smp_schedule' os/kernel/src/` 现得 14 处，均为迁移/停进程族实现与 C 映射注释；唤醒腿改由 `send_sched_ipi` 命名。
- **notify 分支的真实消息投递**：单测把调度者指到表内槽位、走完 `IpcEngine::send` 的内核侧路径并断言标志位（§5.2 细读二）；但"调度服务器收到 NO_QUANTUM 后降级、再经 SYS_SCHEDULE 回写"的端到端回环需要 SCHED 服务器参战，属该服务器的 stage 验收面（`02-stage-sched` 方向），内核宿主测只保证到"消息发得出去、标志位对"为止。
- **抢占门的真机形态**：门读 CPU 本地 `proc_ptr`（§3.3 缺口二），宿主测试要么显式注入（`sched_enqueue_with` 的元组参数）、要么接受"SMP 状态未装 → 跳过评估"的空转形态；真机上的完整行为要靠带 `sanity-checks` 的真机跑观测（§4.3）。

过滤运行的两条实测陷阱（本篇所有读数因此只给全套件口径）：

1. `cargo test -p minix-kernel --features mock test_sched_proc_niced_flag_set_and_clear` 单过滤跑：**FAILED**，panic 点是时钟态的未初始化 expect（现码在 `os/kernel/src/lib.rs` 的 `CLOCK_STATE not initialized — init_clock_and_interrupts must run first` 三处之一：:2217/:2223/:2235；行号随源码演进，以现码为准）。全套件运行时同一测试通过——靠的是时钟组的测试先跑、先把全局态初始化了。单独复跑这一条得不到全套件结论。
2. `cargo test -p minix-kernel --features mock test_proc_no_time` 过滤跑：全套件 7.58 秒跑完的同一批测试，单过滤观察超过 10 分钟未返回（按挂起处理，终止了观察进程）。两族测试共享全局态（BKL 记分板、时钟、进程表静态），单过滤改变的是初始化次序与并发前提——**没有安全的单跑子集**，诊断一律以全套件为准。

真机/宿主分界：宿主测覆盖四原语的纯逻辑、跃迁装置、两分支判据、改参往返；真机门（三架构回归，`os/qemu-tests/README.md` 的配方）覆盖的才是"循环真的调用了它们"的完整路径——宿主侧 `pick_proc`/`requeue_if_preempted` 的循环侧消费已在 [10-switch-to-user.md](10-switch-to-user.md) §5 的真机门里跑过，本篇不重复立同一道验证。

### 5.4 覆盖状态反转表（→Ch4；宿主测试面的反面）

本章的测试面在宿主可证的边界之外还有五块，逐块给出状态与去向（"已覆盖"只指宿主测试；真机面的披露见 §5.3）：

| # | 面 | 现状 | 去向 |
|---|---|---|---|
| M1 | 跨 CPU 入队（C 的 SMP idle 唤醒臂） | 生产者已在（`sched_enqueue_with` 的 idle 臂）；接收侧三件未齐，BSP 钳制兜底（§3.3 缺口一） | 接收侧批次 → **R11-T01**（测试侧：目标 CPU idle 唤醒臂） |
| M2 | notify 分支的真实消息投递端到端（SCHED 降级 → SYS_SCHEDULE 回写） | 宿主只到"消息发得出去、标志位对"（§5.3） | `02-stage-sched` 的 stage 验收面 → **R11-T02** |
| M3 | 抢占门的真机形态（读 CPU 本地 `proc_ptr`） | 宿主靠注入或空转形态（§5.3） | 真机 `sanity-checks` 观测 → **R11-T03** |
| M4 | 单过滤运行的共享态陷阱（两条实测） | 无安全单跑子集（§5.3） | 测试基建治理（全局态初始化次序）→ **R11-T04** |
| M5 | 每 CPU 队列的真激活（AP 队列组参战） | 队列恒 BSP（C2C 止损恢复钳制，§3.3 缺口一） | 接收侧批次 → **R11-T05** |

（§5.3 的三条披露分别对到 M1/M2/M3；M4/M5 为本轮新增披露。两类表互为反转面。`R11-T01~05` 为本表定义的交接编号。）

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

四行表，每行给宿主确认命令（**全套件口径**，理由见 §5.3）与预期关键行：

| 症状 | 怀疑对象（§回指） | 确认命令与预期关键行 |
|---|---|---|
| 某进程永不运行：判据上可运行（标志位归零）却永远轮不到 | INV-1 结构层破坏——位被直接改写、没走 RTS 包装层，进程不在任何队列（§1.1 推演一、§2.4 完整推演） | `cargo test -p minix-kernel --features mock`（全套件须全绿，832 passed；读数随测试集增长，以现跑为准）；**判别读数需要审计开启**（默认构建下这一路静默）：`sanity-checks` feature 下 `runqueues_ok_cpu` 的检查 5（可运行者必在队）会报（`os/kernel/src/debug.rs`）；debug 构建的 `pick_proc` 队首断言只拦"在队却不可运行"的反方向 |
| 某进程被重复调度/队列出现幽灵节点 | 摘链未走或尾指针未修正——孤儿节点、自环（§2.2 推演二） | 同上一行命令；真机 `sanity-checks` 关键行：`runqueues_ok: tail->next not null in queue`、`runqueues_ok: loop in schedule queue`（§4.3 引文的清单第 2 项与队列循环上限守卫）；宿主侧以入队/出队/选人组十条（其中 `test_enqueue_*` 七条）钉住链形状 |
| 改优先级不生效：改参后进程仍按旧优先级被选出 | 改参舞步缺失——字段改了、队列没动，旧队列滞留（§2.3 反向推演） | `cargo test -p minix-kernel --features mock`（全套件）；钉点 `test_sched_proc_runnable_reenqueue_on_update`：可运行进程改参后必须可运行且 `NO_QUANTUM` 已清——舞步两端都在 |
| quantum 耗尽行为异常：该续量的被通知、该通知的被续量 | 两分支判据——`kernel_scheduled`（调度者字段）与 `preemptible`（特权表）各错一半（§3.2） | `cargo test -p minix-kernel --features mock`（全套件）；钉点 `test_proc_no_time` 四条（§5.1 表第二组）：通知分支置 `NO_QUANTUM`、续量分支 `cpu_time_left > 0`，判据常量在 `minix3/minix/include/minix/config.h` 与 `minix3/minix/include/minix/priv.h`（§3.3 的置位面） |

### 6.2 手算与常量核对命令

本节命令全部实跑过，读数即命令输出：

```bash
$ grep -nE '^(void enqueue\(|void dequeue|static struct proc \* pick_proc\(void\)$)' minix3/minix/kernel/proc.c
1595:void enqueue(
1716:void dequeue(struct proc *rp)
1785:static struct proc * pick_proc(void)

$ grep -n 'void proc_no_time\|static void notify_scheduler' minix3/minix/kernel/proc.c
1860:static void notify_scheduler(struct proc *p)
1893:void proc_no_time(struct proc * p)

$ sed -n '66,74p' minix3/minix/include/minix/config.h
#define NR_SCHED_QUEUES   16	/* MUST equal minimum priority + 1 */
#define TASK_Q		   0	/* highest, used for kernel tasks */
#define MAX_USER_Q  	   0    /* highest priority for user processes */   
#define USER_Q  	  ((MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q) /* default
						(should correspond to nice 0) */
#define MIN_USER_Q	  (NR_SCHED_QUEUES - 1)	/* minimum priority for user
						   processes */
/* default scheduling quanta */
#define USER_QUANTUM 200
```

`USER_Q` 的算式按 C 的整数除法语义复算（截断，不是四舍五入）：

```bash
$ awk 'BEGIN{printf "USER_Q = %d\n", int((15-0)/2)+0}'
USER_Q = 7
```

Rust 侧常量与 C 算式对得上（7，不是别的值）：

```bash
$ sed -n '/pub mod priority/,/^}/p' os/kernel/src/proc.rs
pub mod priority {
    pub const TASK_Q: u8 = 0;
    pub const MAX_USER_Q: u8 = 0;
    pub const USER_Q: u8 = 7;
    pub const MIN_USER_Q: u8 = 15;
    pub const NR_SCHED_QUEUES: usize = 16;
}
```

四原语的区间行数复算（逐段 65 + 42 + 65 + 29 = 201 行）：

```bash
$ sed -n '1595,1659p' minix3/minix/kernel/proc.c | wc -l
65
$ sed -n '1670,1711p' minix3/minix/kernel/proc.c | wc -l
42
$ sed -n '1716,1780p' minix3/minix/kernel/proc.c | wc -l
65
$ sed -n '1785,1813p' minix3/minix/kernel/proc.c | wc -l
29
```

两分支与改参的 Rust 锚点：

```bash
$ grep -n 'fn sched_proc_no_time\|fn check_quantum' os/kernel/src/proc_table.rs
1067:    pub fn sched_proc_no_time(
1569:    pub fn check_quantum(

$ wc -l os/kernel/src/sched.rs
1030 os/kernel/src/sched.rs
```

---

## 7. 参见

- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.2（RTS 位族 16 位逐位表——判据层 owning 篇）、§1.4（boot 期按住与 `RTS_UNSET` 解除语义）、§2.3（权限模板族——`USR_F`/`SRV_F` 的 `PREEMPTIBLE` 置位面；调度者槽初值的真值锚点是 `minix3/minix/include/minix/priv.h` 的槽位定义）
- [10-switch-to-user.md](10-switch-to-user.md) §2.1（循环拍二的 PREEMPTED 重排调用点）、§4.3（`pick_and_bill` 薄接口与 `requeue_if_preempted` 头/尾选择——循环侧 owning 篇）
- [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.2（BKL 取得点与持锁约定——包装层 `&BklSection` 见证的来源）
- `12-ipc-core`（IPC 核心篇）——阻塞/唤醒侧的位变化语义
- `16-smp`（SMP 篇，尚未重生成）——IPI 跨 CPU 唤醒与每 CPU 队列的激活
- `17-clock-timer`（时钟与定时器运行篇，尚未重生成）——tick 递减 quantum 的时钟半
- `02-stage-sched` 方向（SCHED 服务器用户态本体，尚未重生成）——NO_QUANTUM 消息的消费端与 MLFQ 策略
- C 源码：`minix3/minix/kernel/proc.c:enqueue`、`:enqueue_head`、`:dequeue`、`:pick_proc`、`:proc_no_time`、`:notify_scheduler`、`minix3/minix/kernel/system.c:sched_proc`、`minix3/minix/kernel/proc.h:RTS_SET`/`:RTS_UNSET`/`:proc_kernel_scheduler`、`minix3/minix/include/minix/config.h`（优先级常量）、`minix3/minix/kernel/cpulocals.h`（每 CPU 队列）、`minix3/minix/include/minix/priv.h`（模板族）、`minix3/minix/kernel/debug.c:runqueues_ok_cpu`
- Rust：`os/kernel/src/sched.rs`（`Scheduler`/`EnqueueInfo`/`sched_proc`/`SchedParams`）、`os/kernel/src/proc_table.rs`（`rts_set`/`rts_unset`/`sched_enqueue_with`/`sched_dequeue`/`sched_proc_no_time`/`check_quantum`/`preemptible`）、`os/kernel/src/proc.rs`（`priority` 常量、`Accounting`）、`os/kernel/src/debug.rs`（`runqueues_ok_cpu`）
