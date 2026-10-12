# 10 · 调度循环：所有内核工作流的唯一出口

> **C 真值源**：`minix3/minix/kernel/proc.c`（`switch_to_user`/`idle`/`switch_address_space_idle`/`pick_proc`/`delivermsg`/`copr_not_available_handler`）、`minix3/minix/kernel/proc.h`（MF_* 杂项标志全表）、`minix3/minix/kernel/system.c:kernel_call_resume`、`minix3/minix/kernel/arch/i386/arch_system.c:restore_user_context`、`minix3/minix/kernel/arch/i386/arch_clock.c:context_stop`（BKL 释放点）、`minix3/minix/kernel/arch/i386/mpx.S`（进入时的风格记录与三条恢复腿）、`minix3/minix/kernel/arch/i386/include/archconst.h`（KTS_* 常量）。
> **Rust 实现**：`os/kernel/src/lib.rs`（`switch_to_user`/`scheduler_loop`/`finish_and_restore`/`idle`/`pick_and_bill`/`requeue_if_preempted`/`switch_address_space`）、`os/kernel/src/proc_table.rs:fn process_misc_flags`、`os/kernel/src/proc_table.rs:fn check_quantum`、`os/kernel/src/sched.rs:fn pick_proc`、`os/kernel/src/vm.rs:fn kernel_call_resume`、`os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner`、`os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl`、`os/arch/src/arch/trap_style.rs:enum TrapStyle`、`os/arch/src/x86_64/trap_return.rs:fn restore_to_user`。
> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。引用位置由符号名定位（`path:func`、`path:fn NAME`、`path:enum NAME`），不使用行号。
> **本篇主题**：所有内核工作流处理完毕后汇入的同一个出口——调度循环。为什么调度器不是一个线程；循环的每一拍在做什么、哪两处可能推倒重来；出口的三条恢复腿怎么选。
> **前置**：[08-system-init-boot-finish.md](08-system-init-boot-finish.md)（§4.6 的终拍把 CPU 交进本篇；§3.4 的 BKL 约定）、[09-vm-boot-protocol.md](09-vm-boot-protocol.md)（VM 先行被调度；其 §2.3 的 MEMREQ 臂与本篇 §3.3 的循环侧消费互为两端）、[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4（RTS 位族与 boot 进程按住）、[07-cross-space-init.md](07-cross-space-init.md) §1.4（ptproc 的切表侧判据——`switch_address_space` 的根切换语义）。
> **后续**：`11-scheduling-primitives`（调度原语篇，尚未重生成）、`12-ipc-core`（IPC 核心篇，尚未重生成）、`13-syscall-dispatch`（系统调用分派篇，尚未重生成）、`14-exception-interrupt`（异常与中断入口篇，尚未重生成）、`16-smp`（SMP 篇，尚未重生成）、`17-clock-timer`（时钟与定时器运行篇，尚未重生成）。

[08-system-init-boot-finish.md](08-system-init-boot-finish.md) 的最后一步，`minix3/minix/kernel/main.c:bsp_finish_booting` 的终拍执行了一句 `switch_to_user()`，CPU 从此离开启动代码，内核再也不回到 `kmain`——那一句交出的控制权，落点就是本篇的循环。`minix3/minix/kernel/arch/i386/arch_smp.c` 的 AP 收尾段里也有一句同样的调用：每个新 CPU 跑完自己的启动序列，末尾同样把 CPU 交给这条循环。从这一拍起，内核里不再有"调度器在运行"这回事：运行着的是一个个中断、陷入与系统调用的处理过程，它们各自办完自己的事后走进同一段代码，由它决定下一个上 CPU 的是谁、以哪种方式把 CPU 交出去。[09-vm-boot-protocol.md](09-vm-boot-protocol.md) 里 VM 拿到第一个 CPU，是这条循环的第一次分派；此后每一次回到用户态，都要再过一遍它。

本篇走完这个出口：C 的循环怎么写（第 2 章）、Rust 用什么形状承接（第 3 章）、逐阶段怎么落地（第 4 章）、靠什么测试钉住（第 5 章）、卡住时怎么查（第 6 章）。

---

## 1. 概述

### 1.1 汇聚点模型：为什么调度器不是一个线程

先把结论的形状摆出来：调度不是与中断、系统调用并列的第四种工作流，而是所有工作流处理完毕后的共同出口。内核里没有独立的调度实体、没有调度栈、也没有"上下文切换到调度器"这个动作——每个内核入口在办完自己的事后，顺路走进同一段代码；这段代码的最后一拍把 CPU 直接交给用户进程。C 给这段代码起的名字是 `minix3/minix/kernel/proc.c:switch_to_user`，它的函数形状把这件事说在明处——入口无参数（不需要被告知"该调度了"），出口无返回（交出去之后自己就不存在了）：

```c
// minix3/minix/kernel/proc.c:switch_to_user —— 逐字引用（函数头与直通检查）
void switch_to_user(void)
{
	/* This function is called an instant before proc_ptr is
	 * to be scheduled again.
	 */
	struct proc * p;
#ifdef CONFIG_SMP
	int tlb_must_refresh = 0;
#endif

	p = get_cpulocal_var(proc_ptr);
	/*
	 * if the current process is still runnable check the misc flags and let
	 * it run unless it becomes not runnable in the meantime
	 */
	if (proc_is_runnable(p))
		goto check_misc_flags;
```

（`minix3/minix/kernel/proc.c:switch_to_user`，全函数 176 行，`sed -n '299,474p' minix3/minix/kernel/proc.c | wc -l` 复算。函数体尾部的 `NOT_REACHABLE;` 哨兵与 `restore_user_context` 调用在 §2.1 的终段逐字出现。）

把三条来路、一个出口、三条恢复腿画在一张图上，就是全篇的行进地图：

```
   用户态                                        内核态
  ─────────                                    ─────────────
   │    │    │
   │    │    └── 系统调用指令 ───► 系统调用入口 ──┐
   │    └─────── IPC 软陷入 ─────► IPC 陷入入口 ──┤  每个入口把"怎么进来的"
   └────────── 硬件中断 / 异常 ──► 中断入口 ──────┘  记进进程的保存帧
                                          │          （mpx.S 记录 KTS_*）
                                          v
                        各自的 handler 办完自己的事
                  （IPC 语义归 12 方向、系统调用语义归 13 方向、
                    中断与异常语义归 14 方向、tick 记账归 17 方向）
                                          │
                                          v
                 ┌─────────────────────────────────────────┐
                 │        switch_to_user：调度循环           │
                 │  挑进程或直通 → 重排与 idle → 切地址空间    │
                 │  → 五标志补完 → 量子检查 → 记账 + 放 BKL   │
                 └─────────────────────────────────────────┘
                                          │
                        按保存帧里记录的进入风格选恢复腿（进出配对）
                     ┌────────────┬────────────┐
                     v            v            v
               sysenter 腿    syscall 腿     int 腿
                     └────────────┴────────────┘
                                          │
                                          v
                                       用户态
```

这张图回答了本节的验收问题：为什么调度器不是一个线程。假想另一种设计——一个独立运行的调度器线程，谁该上 CPU 由它裁决。那么每次调度决策都要付出两次额外的切换：先把 CPU 从刚处理完事件的上下文切给调度器线程，再由调度器切给目标进程；调度器线程自己的栈与状态成为第三份需要维护的上下文，它跟其他内核活动的并发关系还要额外的锁来管。MINIX3 的形状把"决策"折叠进"返回"里：处理过程办完事，就地走进循环，循环的最后一拍直接落到目标进程的用户态——两次切换变成零次，也不存在第三份上下文。

两个后果跟着这个形状一起成立，它们都不是显然的，单独说出来。其一，循环永远运行在"刚刚处理完某个事件"的那个内核栈上：调度器复用上一个陷入的栈，自己没有栈——哪个进程的陷入把内核带进来，循环就踩着哪个进程的内核栈做决策。其二，BKL（大内核锁）从陷入入口一路持有到记账点（§2.4 与 [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.4 的约定），"调度中"与"处理中断中"之间没有锁状态的分界——循环不是"拿到锁再干活"的另一段代码，它就是持锁处理过程的尾段。释放点的位置在 `minix3/minix/kernel/arch/i386/arch_clock.c:context_stop`：为内核段记完账才放锁，§2.4 逐字对它。

还有一个边界要说准，免得"唯一出口"被读过头：`restore_user_context`（真正执行模式切换的那个函数）在 C 里有两个调用点。一个是 `switch_to_user` 的最后一拍；另一个在 `minix3/minix/kernel/proc.c:copr_not_available_handler`——FPU 陷阱处理发现陷阱的进程本该拥有 FPU 时，保存旧拥有者的状态、装上它的状态，然后直接恢复同一个进程继续跑，它的注释写着 "do not schedule!"：那次恢复不做"下一个是谁"的决策，只是让被打断的执行接续。调度决策本身只发生在这条循环里；无决策的续跑不在此列。

### 1.2 三条进入路径，一个出口

从用户态进内核有三扇门——硬件中断（16 方向）、IPC 软陷入（12 方向）、系统调用（13 方向）——出去时却只有一个门框。§1.1 的图里，出口下方分出了三条恢复腿；本节讲清"进与出怎么配对"。配对的载体是进程保存帧里的一个记录位：C 的 `p_kern_trap_style`，常量定义在 `minix3/minix/kernel/arch/i386/include/archconst.h`：

```c
// minix3/minix/kernel/arch/i386/include/archconst.h —— 逐字引用（trap style 常量）
/* trap styles recorded on kernel entry and exit */
#define KTS_NONE	1 /* invalid */
#define KTS_INT_HARD	2 /* exception / hard interrupt */
#define KTS_INT_ORIG	3 /* soft interrupt from libc */
#define KTS_INT_UM	4 /* soft interrupt from usermapped code */
#define KTS_FULLCONTEXT	5 /* must restore full context */
#define KTS_SYSENTER	6 /* SYSENTER instruction (usermapped) */
#define KTS_SYSCALL	7 /* SYSCALL instruction (usermapped) */
```

（`minix3/minix/kernel/arch/i386/include/archconst.h` 的 KTS 常量段。）

进入的那一侧，`minix3/minix/kernel/arch/i386/mpx.S` 的入口宏把风格值写进保存帧：中断与异常入口经 `SAVE_PROCESS_CTX(0, KTS_INT_HARD)` 一类宏记录（mpx.S 多处），快速系统调用入口直接 `movl $KTS_SYSCALL, P_KERN_TRAP_STYLE(%ebp)`，SYSENTER 入口同理记录 `KTS_SYSENTER`。退出的那一侧，`minix3/minix/kernel/arch/i386/arch_system.c:restore_user_context` 读出这个记录、把它清掉、按值分派到三条汇编腿之一。腿与值的对应关系是本篇的第一张查表：

| 进入方式（来路归口） | 进入时记录 | 恢复腿 | 恢复腿的 ENTRY |
|---|---|---|---|
| 硬件中断 / 异常（14 方向） | `KTS_INT_HARD` | `restore_user_context_int` | `minix3/minix/kernel/arch/i386/mpx.S:restore_user_context_int` |
| IPC 软陷入（12 方向，libc 的 trap 指令） | `KTS_INT_ORIG` | `restore_user_context_int` | 同上 |
| 快速系统调用（13 方向） | `KTS_SYSCALL` 或 `KTS_SYSENTER` | `restore_user_context_syscall` / `restore_user_context_sysenter` | `minix3/minix/kernel/arch/i386/mpx.S:restore_user_context_syscall`、`minix3/minix/kernel/arch/i386/mpx.S:restore_user_context_sysenter` |
| 进程重置/初建经 `arch_proc_setcontext` 记全上下文（`minix3/minix/kernel/arch/i386/arch_system.c:arch_proc_reset` 写 FULLCONTEXT）；信号返回经同一函数重录 sigcontext 里保存的原风格 | `KTS_FULLCONTEXT` 或原风格 | `restore_user_context_int` | 同 INT 腿 |
| 无记录 | `KTS_NONE` | —— | `panic("no entry trap style known")` |

这张表里藏着本节的非显然事实：恢复腿不由调度器挑。循环在出口处只做一件事——照录。怎么进的，就怎么出；腿是在**上一次进入时**就定下的，出口的 `restore_user_context` 只是把配对执行完。三个 ENTRY 的定位命令与行号读数在 §6.2 给出（`grep -n 'ENTRY(restore_user_context' minix3/minix/kernel/arch/i386/mpx.S`）。

把配对机制反过来推演：设想调度器统一选最快的一条腿（sysenter 腿）返回，不看记录。sysenter 腿的期望帧布局是"瘦帧"——它只从保存帧里取 EIP、ESP、EAX、EBX 与 PSW，其余寄存器假定用户态自我保存（§2.3 逐字可见）；而从 int 陷入进来的帧是全上下文帧，段寄存器与完整标志位都在帧里。拿全上下文帧喂瘦帧恢复序列，寄存器恢复错位：段寄存器没人装、PSW 的恢复位置对不上，恢复出来的用户态带着半个旧现场。进出配对让帧布局与恢复序列一一对应——记录位就是两边唯一共享的契约。

### 1.3 循环总览：C 五拍与它的形态

从"决定谁上"到"真的交出去"，中间隔着哪些拍？`minix3/minix/kernel/proc.c:switch_to_user` 的全函数（176 行）按次序分五拍：

```
 拍一  挑或直通      读 proc_ptr；仍可运行 → 直跳拍四（check_misc_flags）
        │
 拍二  重排与兜底    PREEMPTED 清位重排（有余时头插、无余时尾插）
        │            while (!(p = pick_proc())) idle();
        v
 拍三  切地址空间    proc_ptr 更新 →（SMP：tlb_must_refresh 预计算）→ switch_address_space
        │
 拍四  五标志循环    KCALL_RESUME / DELIVERMSG / SC_DEFER / SC_TRACE / SC_ACTIVE
        │            逐臂消费；每轮末尾重验 runnable，不可运行 → 跳回拍二
        v
 拍五  量子与终段    量子耗尽 → proc_no_time；再验 runnable（不可运行 → 跳回拍二）
                     → arch_finish_switch_to_user → context_stop(KERNEL)（记账+放 BKL）
                     → FPU 归属 → 清 CONTEXT_SET →（SMP：刷 TLB）→ 重启本地定时器
                     → restore_user_context（NOT_REACHABLE，永不返回）
```

两处"跳回拍二"的出口都在拍四与拍五之内，这个位置不是随手放的。处理标志的过程本身可能把进程变得不可运行：KCALL_RESUME 臂补完被挂起的内核调用时可能再挂起（进程停下等 VM）、DELIVERMSG 臂拷贝消息遇到坏指针会给进程发 SIGSEGV、SC_DEFER 臂补执行的系统调用可能阻塞在 SEND 上。所以"选人"与"交出去"之间没有不变量——每一步消费之后都要重验一次 runnable，验不过就回拍二重新挑人。第 2 章按这五拍逐字走代码。

### 1.4 本篇范围与可以安全跳过的内容

| 主题 | 本篇怎么办 | 去处 |
|---|---|---|
| 就绪队列的维护、enqueue 头/尾插本体、优先级算法 | 只讲循环怎么调用 `pick_proc`（接口一行、返回一个进程号） | `11-scheduling-primitives`（调度原语篇，尚未重生成） |
| 五标志各臂的完整 callee 语义 | 只讲循环在哪个时机消费它们（§2.1 每臂一句话） | `12-ipc-core`（IPC 核心篇，尚未重生成）、`13-syscall-dispatch`（系统调用分派篇，尚未重生成） |
| trap 入口与异常处理的完整链路 | 只讲进入时的风格记录（§1.2） | `14-exception-interrupt`（异常与中断入口篇，尚未重生成） |
| tick 到达后的 quantum 递减与 bill 记账本体 | 只讲循环侧的记账调用点与 BKL 释放点 | `17-clock-timer`（时钟与定时器运行篇，尚未重生成） |
| SMP 的 AP 启动与 IPI | 只讲 AP 收尾也进同一条循环（§1.1） | `16-smp`（SMP 篇，尚未重生成） |
| VM 地址空间的建立与 bootstrap 根 | 引用不重复 | [07-cross-space-init.md](07-cross-space-init.md)、[09-vm-boot-protocol.md](09-vm-boot-protocol.md) |

认知减负声明：本章不必深究就绪队列怎么维护、五标志各臂 callee 内部做什么、tick 递减 quantum 的细节——本篇只保证读者能回答"循环在什么时机、以什么前提调用它们"；队列算法归 `11-scheduling-primitives`（调度原语篇，尚未重生成），IPC 与系统调用语义归 `12-ipc-core`/`13-syscall-dispatch`（两篇，均尚未重生成），记账本体归 `17-clock-timer`（时钟与定时器运行篇，尚未重生成）。

把第 1 章的图按同一框架收一次：三扇门进来（硬件中断、IPC 软陷入、系统调用），各自把"怎么进来的"记进保存帧；各自的 handler 办完事，顺路走进 `switch_to_user`；循环五拍走完，按帧里的记录照录进出配对，从三条恢复腿之一回到用户态。调度器不是一个线程——它是每个内核出口的公共段。第 2 章按五拍逐字走 C 代码：先看全函数的分段，再看主走查。

## 2. C 源码分析

### 2.0 本章地图

`minix3/minix/kernel/proc.c:switch_to_user` 全函数 176 行，按 §1.3 的五拍自然分成四段：

| 段 | 内容 | 对应拍 |
|---|---|---|
| 直通检查 | 读 `proc_ptr`，仍可运行则跳过重挑 | 拍一 |
| 重挑与切表 | PREEMPTED 重排、idle 兜底循环、`switch_address_space` | 拍二、拍三 |
| 五标志循环 | `check_misc_flags` 标签起的 while 循环，逐臂消费 | 拍四 |
| 量子与终段 | `proc_no_time`、六连收尾、`restore_user_context` | 拍五 |

本章先走主函数（§2.1，四段逐字），再看无活可干时的 idle（§2.2），然后是循环调用的两个配角——选人的 `pick_proc` 与出去的 `restore_user_context`（§2.3），最后把硬次序归纳成一张总表（§2.4）。

### 2.1 switch_to_user 主函数走查（本篇主走查）

**第一段：直通检查。** §1.1 已引函数头：循环先读每 CPU 的 `proc_ptr`，若当前进程仍可运行，直接跳到五标志循环——当前进程没有被挡住的理由时，"重新挑一次"这个动作整个跳过。这是循环的第一层快捷路径：同一个进程连续两次进出内核（比如两次系统调用）之间，只有五标志消费和终段在跑，没有重挑、没有切表。

**第二段：重挑与切表。** 直通没走成的（进程被挡住、或刚被抢占），落到 `not_runnable_pick_new` 标签。这段做三件事：

```c
// minix3/minix/kernel/proc.c:switch_to_user —— 逐字引用（重挑与切表段）
	/*
	 * if a process becomes not runnable while handling the misc flags, we
	 * need to pick a new one here and start from scratch. Also if the
	 * current process wasn't runnable, we pick a new one here
	 */
not_runnable_pick_new:
	if (proc_is_preempted(p)) {
		p->p_rts_flags &= ~RTS_PREEMPTED;
		if (proc_is_runnable(p)) {
			if (p->p_cpu_time_left)
				enqueue_head(p);
			else
				enqueue(p);
		}
	}

	/*
	 * if we have no process to run, set IDLE as the current process for
	 * time accounting and put the cpu in an idle state. After the next
	 * timer interrupt the execution resumes here and we can pick another
	 * process. If there is still nothing runnable we "schedule" IDLE again
	 */
	while (!(p = pick_proc())) {
		idle();
	}

	/* update the global variable */
	get_cpulocal_var(proc_ptr) = p;

#ifdef CONFIG_SMP
	if (p->p_misc_flags & MF_FLUSH_TLB && get_cpulocal_var(ptproc) == p)
		tlb_must_refresh = 1;
#endif
	switch_address_space(p);
```

（`minix3/minix/kernel/proc.c:switch_to_user` 的 `not_runnable_pick_new` 段，从标签到 `switch_address_space`。）

PREEMPTED 的重排值得逐行读。被抢占的进程带着 `RTS_PREEMPTED` 位站在原地，重排时先把这个位清掉，再看它是否因此回到可运行；可运行的话，按**剩余量子**决定插队位置——`p_cpu_time_left` 非零（量子没用完，是被更急的事打断的）就 `enqueue_head` 插到本优先级队列的队首，把剩下的时间片跑完；为零（时间片自然耗尽）就 `enqueue` 排到队尾。队列与优先级本身归 `11-scheduling-primitives`（调度原语篇，尚未重生成），这里只需要看到：重排决策发生在调度循环内，且头/尾的选择依据是余时。把头/尾决策反过来推演：若统一用尾插，一个只跑了半拍的交互进程会排到一整批满量子进程的后面，被抢占反而比正常让出等得更久——响应性反转；头/尾按余时决策是优先级队列之外的补偿。

接下去的 `while (!(p = pick_proc())) idle();` 是 idle 兜底：队列空了就把 CPU 交给 IDLE 进程停机等中断，醒来再试。`pick_proc` 与 `idle` 的函数体在 §2.2/§2.3。`proc_ptr` 更新之后是 SMP 分支的 `tlb_must_refresh` 预计算——这个临时变量为什么必须在切表**之前**算，§2.4 的硬次序里说。

**第三段：五标志循环。** `check_misc_flags` 标签起，这是全函数最长的一段，也是本篇主走查的核心：

```c
// minix3/minix/kernel/proc.c:switch_to_user —— 逐字引用（五标志循环）
check_misc_flags:

	assert(p);
	assert(proc_is_runnable(p));
	while (p->p_misc_flags &
		(MF_KCALL_RESUME | MF_DELIVERMSG |
		 MF_SC_DEFER | MF_SC_TRACE | MF_SC_ACTIVE)) {

		assert(proc_is_runnable(p));
		if (p->p_misc_flags & MF_KCALL_RESUME) {
			kernel_call_resume(p);
		}
		else if (p->p_misc_flags & MF_DELIVERMSG) {
			TRACE(VF_SCHEDULING, printf("delivering to %s / %d\n",
				p->p_name, p->p_endpoint););
			delivermsg(p);
		}
		else if (p->p_misc_flags & MF_SC_DEFER) {
			/* Perform the system call that we deferred earlier. */

			assert (!(p->p_misc_flags & MF_SC_ACTIVE));

			arch_do_syscall(p);

			/* If the process is stopped for signal delivery, and
			 * not blocked sending a message after the system call,
			 * inform PM.
			 */
			if ((p->p_misc_flags & MF_SIG_DELAY) &&
					!RTS_ISSET(p, RTS_SENDING))
				sig_delay_done(p);
		}
		else if (p->p_misc_flags & MF_SC_TRACE) {
			/* Trigger a system call leave event if this was a
			 * system call. We must do this after processing the
			 * other flags above, both for tracing correctness and
			 * to be able to use 'break'.
			 */
			if (!(p->p_misc_flags & MF_SC_ACTIVE))
				break;

			p->p_misc_flags &=
				~(MF_SC_TRACE | MF_SC_ACTIVE);

			/* Signal the "leave system call" event.
			 * Block the process.
			 */
			cause_sig(proc_nr(p), SIGTRAP);
		}
		else if (p->p_misc_flags & MF_SC_ACTIVE) {
			/* If MF_SC_ACTIVE was set, remove it now:
			 * we're leaving the system call.
			 */
			p->p_misc_flags &= ~MF_SC_ACTIVE;

			break;
		}

		/*
		 * the selected process might not be runnable anymore. We have
		 * to checkit and schedule another one
		 */
		if (!proc_is_runnable(p))
			goto not_runnable_pick_new;
	}
```

（`minix3/minix/kernel/proc.c:switch_to_user` 的 `check_misc_flags` 循环，从标签到循环收尾。）

五个标志的位值与一句话语义（全表 18 条 `MF_*` 定义在 `minix3/minix/kernel/proc.h`，`grep -c '^#define MF_' minix3/minix/kernel/proc.h` 复算；循环条件引用其中 5 条）：

| 标志 | 位值 | 消费动作 | 一句话语义 |
|---|---|---|---|
| `MF_KCALL_RESUME` | 0x008 | `kernel_call_resume(p)` | 内核调用的执行曾被中断（典型：等 VM 补页），趁回到用户态之前补完 |
| `MF_DELIVERMSG` | 0x040 | `delivermsg(p)` | 有条消息要拷进这个进程的用户空间，趁它还停在这里拷完（`minix3/minix/kernel/proc.c:delivermsg`） |
| `MF_SC_DEFER` | 0x200 | `arch_do_syscall(p)` + 条件 `sig_delay_done` | 被推迟的系统调用此刻补执行；补完若发现进程曾为信号交付被停、且没有阻塞在发送上，通知 PM 解除 |
| `MF_SC_TRACE` | 0x400 | 清两标志位 + `cause_sig(SIGTRAP)` | 跟踪语义的"离开系统调用"事件：发 SIGTRAP 把进程停下交给跟踪者 |
| `MF_SC_ACTIVE` | 0x100 | 清位 + `break` | 跟踪语义的另一半：正处于系统调用中，出循环前把位清掉 |

这五臂有一个共同的形状，也是本段的非显然事实：**五标志的处理发生在用户态恢复之前**。被挂起的内核调用（KCALL_RESUME）、待拷贝的消息（DELIVERMSG），都在进程重新见到用户态之前补完——进程下一次运行时，欠它的事已经办完，用户态看到的世界是连贯的。SC_TRACE 与 SC_ACTIVE 是跟踪语义的进出配对：SC_TRACE 臂开头的 `if (!(p->p_misc_flags & MF_SC_ACTIVE)) break;` 依赖 SC_ACTIVE 先清——若 SC_ACTIVE 还没被消费过，"离开事件"无从谈起，直接出循环。而 SC_DEFER 臂开头的 `assert (!(p->p_misc_flags & MF_SC_ACTIVE))` 从反面钉住同一件事：推迟的系统调用补执行时，不可能同时处于"系统调用活动"状态。

每轮末尾的重验是这段的第二件非显然事实：任一臂的 callee 都可能把进程变得不可运行——KCALL_RESUME 臂的 `kernel_call_resume` 补完时可能再次挂起等 VM、DELIVERMSG 臂拷贝遇到坏指针会 `cause_sig(SIGSEGV)` 停掉进程、SC_DEFER 臂补执行的系统调用可能阻塞在 SEND 上。重验不过就 `goto not_runnable_pick_new`，整个"选人"过程推倒重来。C 用两个标签和两条 goto 表达这件事；这个控制流形状 Rust 怎么接，§3.3/§4.1 说。

**第四段：量子检查与终段。** 循环正常走完后，CPU 已经决定交给这个进程，交出去之前还有一串收尾：

```c
// minix3/minix/kernel/proc.c:switch_to_user —— 逐字引用（量子检查与终段）
	/*
	 * check the quantum left before it runs again. We must do it only here
	 * as we are sure that a possible out-of-quantum message to the
	 * scheduler will not collide with the regular ipc
	 */
	if (!p->p_cpu_time_left)
		proc_no_time(p);
	/*
	 * After handling the misc flags the selected process might not be
	 * runnable anymore. We have to checkit and schedule another one
	 */
	if (!proc_is_runnable(p))
		goto not_runnable_pick_new;

	TRACE(VF_SCHEDULING, printf("cpu %d starting %s / %d "
				"pc 0x%08x\n",
		cpuid, p->p_name, p->p_endpoint, p->p_reg.pc););
#if DEBUG_TRACE
	p->p_schedules++;
#endif

	p = arch_finish_switch_to_user();
	assert(p->p_cpu_time_left);

	context_stop(proc_addr(KERNEL));

	/* If the process isn't the owner of FPU, enable the FPU exception */
	if (get_cpulocal_var(fpu_owner) != p)
		enable_fpu_exception();
	else
		disable_fpu_exception();

	/* If MF_CONTEXT_SET is set, don't clobber process state within
	 * the kernel. The next kernel entry is OK again though.
	 */
	p->p_misc_flags &= ~MF_CONTEXT_SET;

#if defined(__i386__)
  	assert(p->p_seg.p_cr3 != 0);
#elif defined(__arm__)
	assert(p->p_seg.p_ttbr != 0);
#endif
#ifdef CONFIG_SMP
	if (p->p_misc_flags & MF_FLUSH_TLB) {
		if (tlb_must_refresh)
			refresh_tlb();
		p->p_misc_flags &= ~MF_FLUSH_TLB;
	}
#endif
	
	restart_local_timer();
	
	/*
	 * restore_user_context() carries out the actual mode switch from kernel
	 * to userspace. This function does not return
	 */
	restore_user_context(p);
	NOT_REACHABLE;
}
```

（`minix3/minix/kernel/proc.c:switch_to_user` 的量子检查与终段，到函数收尾。）

量子检查的位置有注释自答：必须在五标志循环**之后**查余时，因为"量子耗尽"要给调度器发一条消息，这条消息若早于五标志消费发出，会跟常规 IPC 撞车。查完还要再验一次 runnable（第三重验），之后是终段六连，逐项列表：

| 步 | 动作 | 依据 |
|---|---|---|
| 1 | `arch_finish_switch_to_user()`：把进程指针存到内核栈顶（汇编恢复路径取用），并把保存 PSW 的中断允许位置 1 | `minix3/minix/kernel/arch/i386/arch_system.c:arch_finish_switch_to_user` |
| 2 | `context_stop(proc_addr(KERNEL))`：给刚结束的内核段记 TSC 账，并**在这里释放 BKL** | `minix3/minix/kernel/arch/i386/arch_clock.c:context_stop`（§2.4 逐字） |
| 3 | FPU 归属：当前进程不是 FPU 拥有者则打开 FPU 异常（下一条浮点指令会陷阱），是则关闭 | `minix3/minix/kernel/proc.c:switch_to_user` 的 `enable/disable_fpu_exception` 对 |
| 4 | 清 `MF_CONTEXT_SET`：内核期间若有人整体重设过进程上下文（信号返回一类），这个位挡住内核再碰现场；出了内核就不需要挡了 | `minix3/minix/kernel/arch/i386/arch_system.c:arch_proc_setcontext` 是置位点 |
| 5 | SMP：若 `MF_FLUSH_TLB` 且重挑时预计算判定本 CPU 缓存过它的翻译，`refresh_tlb()` 后清位 | 第二段的 `tlb_must_refresh` 在此消费 |
| 6 | `restart_local_timer()`，然后 `restore_user_context(p)`——按 §1.2 的配对分派三条腿之一，永不返回 | `minix3/minix/kernel/arch/i386/arch_system.c:restore_user_context` |

终段次序里的硬约束有两处，先记在这里，§2.4 归纳：记账（含放 BKL）必须先于 `restore_user_context`——恢复是最后的动作，之后的路径不再需要锁；FPU 归属必须先于恢复——陷阱使能在恢复的用户态里才第一次生效。

### 2.2 idle 与 switch_address_space_idle

拍二的 `while (!(p = pick_proc())) idle();` 把"没有活干"也纳入同一条循环：idle 不是独立的调度状态，而是循环内的一个函数调用，返回后接着重挑。全函数 54 行（`sed -n '176,229p' minix3/minix/kernel/proc.c | wc -l` 复算），做四件事：

```c
// minix3/minix/kernel/proc.c:idle —— 逐字引用（全函数）
static void idle(void)
{
	struct proc * p;

	/* This function is called whenever there is no work to do.
	 * Halt the CPU, and measure how many timestamp counter ticks are
	 * spent not doing anything. This allows test setups to measure
	 * the CPU utilization of certain workloads with high precision.
	 */

	p = get_cpulocal_var(proc_ptr) = get_cpulocal_var_ptr(idle_proc);
	if (priv(p)->s_flags & BILLABLE)
		get_cpulocal_var(bill_ptr) = p;

	switch_address_space_idle();

#ifdef CONFIG_SMP
	get_cpulocal_var(cpu_is_idle) = 1;
	/* we don't need to keep time on APs as it is handled on the BSP */
	if (cpuid != bsp_cpu_id)
		stop_local_timer();
	else
#endif
	{
		/*
		 * If the timer has expired while in kernel we must
		 * rearm it before we go to sleep
		 */
		restart_local_timer();
	}

	/* start accounting for the idle time */
	context_stop(proc_addr(KERNEL));
#if !SPROFILE
	halt_cpu();
#else
	if (!sprofiling)
		halt_cpu();
	else {
		volatile int * v;

		v = get_cpulocal_var_ptr(idle_interrupted);
		interrupts_enable();
		while (!*v)
			arch_pause();
		interrupts_disable();
		*v = 0;
	}
#endif
	/*
	 * end of accounting for the idle task does not happen here, the kernel
	 * is handling stuff for quite a while before it gets back here!
	 */
}
```

（`minix3/minix/kernel/proc.c:idle`，全函数逐字。）

四件事按次序：一，`proc_ptr` 与（IDLE 可计账时的）`bill_ptr` 都指向 IDLE——停机期间的时间账记在 IDLE 头上，不是悬空；二，`switch_address_space_idle()` 切地址空间；三，SMP 下置 `cpu_is_idle` 并按本 CPU 是否 AP 决定停掉还是重arm本地定时器（AP 的时钟由 BSP 管，停掉的 AP 不需要本地 tick）；四，`context_stop(KERNEL)` 给内核段记账，然后 `halt_cpu()` 停机等中断。函数头注释里那句 TSC 测量说明顺带交代了动机：停机时长用时间戳计数器量出来，测试环境据此算 CPU 利用率。函数尾的注释把记账边界讲反着说了一遍：IDLE 段的记账终结**不**发生在这里——从停机醒来要到内核处理完一大圈事务才回到重挑，所以 idle 只结内核段的账，唤醒后的账由后续路径结。

第二件事的函数短到只有一条语句，但注释把一个容易踩空的假设写在了明处：

```c
// minix3/minix/kernel/proc.c:switch_address_space_idle —— 逐字引用（全函数）
static void switch_address_space_idle(void)
{
#ifdef CONFIG_SMP
	/*
	 * currently we bet that VM is always alive and its pages available so
	 * when the CPU wakes up the kernel is mapped and no surprises happen.
	 * This is only a problem if more than 1 cpus are available
	 */
	switch_address_space(proc_addr(VM_PROC_NR));
#endif
}
```

（`minix3/minix/kernel/proc.c:switch_address_space_idle`，全函数逐字。）

非显然事实在注释里：idle 停机前把地址空间切到 **VM 的页表**。停机指令醒来靠中断，而中断处理需要内核映射在位；单核构型里这整段被 `CONFIG_SMP` 编译掉——C 注释给的理由是"这只在多于一个 CPU 时才是问题"（单核没有"别的 CPU 换了表"这回事），而不是"切表是空操作"。多核时上一个进程的表可能在位、但 VM 的页可能正被换出——注释原话 "VM is always alive" 承认这是个赌注：赌 VM 活着且它的页可用，切到 VM 的表保证唤醒中断的内核半与 VM 页同时可达。反向推演：若 idle 不切表直接停机——停机时在位的是上一个用户进程的表，内核映射每张表都有，所以唤醒中断的内核半还能跑；但"VM 的页可用"这个前提断了——VM 可能正被换出，多核唤醒路径里任何要摸 VM 页的代码踩空。切 VM 表把这个赌注收敛到一处，赌输的范围只剩"VM 不在"这一种情况。

### 2.3 pick_proc 与 restore_user_context：选人与出去

循环调用的两个配角，接口都薄得惊人。先是选人：

```c
// minix3/minix/kernel/proc.c:pick_proc —— 逐字引用（全函数）
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

（`minix3/minix/kernel/proc.c:pick_proc`，全函数逐字。`NR_SCHED_QUEUES` 的值是 16，`minix3/minix/include/minix/config.h` 定义。）

选人的接口就是一层扫描：本 CPU 的队列数组按优先级从 0 号（最高）扫起，第一个非空队列取队首，可计账的顺手把 `bill_ptr` 记上，返回；全空返回 NULL，调用方进 idle。优先级与队列的算法本体归 `11-scheduling-primitives`（调度原语篇，尚未重生成）；本篇只需要看到循环拿到的接口形状——一行调用、一个返回值、账随人选出。

然后是出去。`minix3/minix/kernel/arch/i386/arch_system.c:restore_user_context` 全函数 47 行（`sed -n '566,612p' … | wc -l` 复算），函数头是一段 `#if 0` 的统计代码（调试遗存，编译器不编译），真正的工作从读取记录开始：

```c
// minix3/minix/kernel/arch/i386/arch_system.c:restore_user_context —— 节选（函数头的 #if 0 统计块省略）
void restore_user_context(struct proc *p)
{
	int trap_style = p->p_seg.p_kern_trap_style;
#if 0
	...（统计代码块省略）
#endif

	p->p_seg.p_kern_trap_style = KTS_NONE;

	if(trap_style == KTS_SYSENTER) {
		restore_user_context_sysenter(p);
		NOT_REACHABLE;
        }

	if(trap_style == KTS_SYSCALL) {
		restore_user_context_syscall(p);
		NOT_REACHABLE;
	}

        switch(trap_style) {
                case KTS_NONE:
                        panic("no entry trap style known");
                case KTS_INT_HARD:
                case KTS_INT_UM:
                case KTS_FULLCONTEXT:
                case KTS_INT_ORIG:
			restore_user_context_int(p);
			NOT_REACHABLE;
                default:
                        panic("unknown trap style recorded");
                        NOT_REACHABLE;
        }

        NOT_REACHABLE;
}
```

（`minix3/minix/kernel/arch/i386/arch_system.c:restore_user_context`，节选自函数头；省略号处为 `#if 0` 包住的统计代码块（调试遗存，编译器不编译），其余逐行未动。真正的工作从读取记录开始：）

这段有两个非显然事实。其一，**读后即清**：`trap_style` 先读进局部变量，随即将帧里的记录清回 `KTS_NONE`，然后才分派——配对状态是单次的，同一次进入不会被二次恢复。其二，`KTS_NONE` 走到恢复点直接 panic：没有进入记录却到了出口，说明内核不变量已破（或者像信号返回路径那样，有人构造了一个没有内核进入的"假恢复"），这种情况宁可停机也不猜。反面推演两头：若记录不清，下一次恢复会拿上次进入的帧布局来卸载这一次的现场——帧是旧形状，寄存器错位；若 `KTS_NONE` 静默返回，一个没有现场的"恢复"悄悄放跑，破坏比崩溃难查得多。

三条腿的形状，看 sysenter 腿就够代表"瘦帧"一侧，int 腿代表"全上下文"一侧：

```asm
// minix3/minix/kernel/arch/i386/mpx.S:restore_user_context_sysenter —— 逐字引用（全腿）
ENTRY(restore_user_context_sysenter)
	/* return to userspace using sysexit.
	 * most of the context saving the userspace process is
	 * responsible for, we just have to take care of the right EIP
	 * and ESP restoring here to resume execution, and set EAX and
	 * EBX to the saved status values.
	 */
	mov	4(%esp), %ebp		/* retrieve proc ptr arg */
	movw	$USER_DS_SELECTOR, %ax
	movw	%ax, %ds
	mov	PCREG(%ebp), %edx	/* sysexit restores EIP using EDX */
	mov	SPREG(%ebp), %ecx	/* sysexit restores ESP using ECX */
	mov	AXREG(%ebp), %eax	/* trap return value */
	mov	BXREG(%ebp), %ebx	/* secondary return value */

        /* restore PSW */
        movl    PSWREG(%ebp), %edi      /* load desired PSW to EDI */
        push    %edi
        popf

	sti				/* enable interrupts */
	sysexit				/* jump to EIP in user */
```

```asm
// minix3/minix/kernel/arch/i386/mpx.S:restore_user_context_int —— 节选（段寄存器恢复之后的通用寄存器装回与 iret 本体省略）
ENTRY(restore_user_context_int)
	mov	4(%esp), %ebp	/* will assume P_STACKBASE == 0 */

	/* reconstruct the stack for iret */
	push	$USER_DS_SELECTOR	/* ss */
	movl	SPREG(%ebp), %eax
	push	%eax
	movl	PSWREG(%ebp), %eax
	push	%eax
	push	$USER_CS_SELECTOR	/* cs */
	movl	PCREG(%ebp), %eax
	push	%eax

	/* Restore segments as the user should see them. */
	movw	$USER_DS_SELECTOR, %si
        movw    %si, %ds
        movw    %si, %es
        movw    %si, %fs
        movw    %si, %gs
```

（两条腿的注释把各自负责什么讲在明处：sysenter 腿只装 EIP/ESP/EAX/EBX/PSW，"most of the context saving the userspace process is responsible for"；int 腿在栈上重建完整的 iret 载荷——ss、esp、psw、cs、eip 五件——再把段寄存器逐个装回。§1.2 的帧布局配对论证，证据就在这两段的注释里。三条 ENTRY 的行号定位命令在 §6.2。）

### 2.4 本章归纳：C 循环总表

把 §2.1-§2.3 的五拍收进一张表——拍、锚点、状态后果、回跳条件：

| 拍 | 锚点 | 状态后果 | 回跳条件 |
|---|---|---|---|
| 直通检查 | `minix3/minix/kernel/proc.c:switch_to_user` 开头 | 无（读 `proc_ptr` 判断） | 可运行 → 跳拍四 |
| 重排与兜底 | 同上 `not_runnable_pick_new` 标签 | PREEMPTED 清位、头/尾插队；队列空则 idle 停机 | ——（顺序执行） |
| 切地址空间 | 同上；`switch_address_space` | 根寄存器指向新进程的表；`tlb_must_refresh` 预计算 | —— |
| 五标志循环 | 同上 `check_misc_flags` 标签 | 五臂逐个消费；callee 可能改 RTS 位 | 每轮末尾不可运行 → 跳回拍二 |
| 量子检查 | 同上 `proc_no_time` 调用 | 量子耗尽则通知调度器并按策略处理 | 不可运行 → 跳回拍二 |
| 终段六连 | `minix3/minix/kernel/arch/i386/arch_clock.c:context_stop` 与 `minix3/minix/kernel/arch/i386/arch_system.c:restore_user_context` | 记账、放 BKL、FPU 归属、清 CONTEXT_SET、刷 TLB、重启定时器 | ——（恢复即离开） |

全表的硬次序两条，每条都有反面的代价。第一条：**地址空间切换先于标志处理**。五标志消费的 callee 可能经 VM 补页（KCALL_RESUME 臂挂起等 VM、DELIVERMSG 臂页失败挂起），补页要看的是目标进程自己的表——切表在先，处理中的页表访问才落在正确的翻译上。第二条：**BKL 记账释放先于 restore**。`context_stop` 为内核段记完账就放锁，`restore_user_context` 是最后的动作；恢复之后的路径（用户态）不持内核锁，锁的持有期到记账点为止。第一条若反过来——先处理标志后切表——处理中补页查的是旧表的翻译，补出来的页映射进错误的地址空间；第二条若反过来——带着锁恢复用户态——锁要到下一次陷入才有人放，期间所有其他 CPU 的内核活动全部堵死。

放锁点的证据是 `context_stop` 的 KERNEL 分支：

```c
// minix3/minix/kernel/arch/i386/arch_clock.c:context_stop —— 节选（KERNEL 分支与放锁标志）
void context_stop(struct proc * p)
{
	u64_t tsc, tsc_delta;
	u64_t * __tsc_ctr_switch = get_cpulocal_var_ptr(tsc_ctr_switch);
	unsigned int cpu, tpt, counter;
#ifdef CONFIG_SMP
	int must_bkl_unlock = 0;

	cpu = cpuid;

	/*
	 * This function is called only if we switch from kernel to user or idle
	 * or back. Therefore this is a perfect location to place the big kernel
	 * lock which will hopefully disappear soon.
	 *
	 * If we stop accounting for KERNEL we must unlock the BKL. If account
	 * for IDLE we must not hold the lock
	 */
	if (p == proc_addr(KERNEL)) {
		u64_t tmp;

		read_tsc_64(&tsc);
		tmp = tsc - *__tsc_ctr_switch;
		kernel_ticks[cpu] = kernel_ticks[cpu] + tmp;
		p->p_cycles = p->p_cycles + tmp;
		must_bkl_unlock = 1;
	} else {
```

（`minix3/minix/kernel/arch/i386/arch_clock.c:context_stop` 的函数头与 KERNEL 分支；省略号处为 else 分支——为即将上 CPU 的进程取 BKL 并计 bkl_ticks，以及随后的 USER/IDLE 分支。注释原话把放锁规则讲在明处："If we stop accounting for KERNEL we must unlock the BKL"。记账细节本体归 `17-clock-timer`（时钟与定时器运行篇，尚未重生成），本篇只取放锁时点。）

带着这张表进第 3 章：Rust 的循环把五拍重排成五个阶段，两个 goto 换成了循环回边，BKL 释放点钉在同一个位置——每处对应关系都回指本章的具体小节。

## 3. Rust 设计决策

### 3.1 汇聚点的类型化：`-> !` 与持锁见证（→§1.1、[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.4）

C 里真实存在的语义是（§1.1/§2.1）：`switch_to_user` 永不返回——`NOT_REACHABLE` 是写给人看的注释哨兵，编译器不做任何检查；BKL 从陷入入口一路持有到 `context_stop` 的记账点（§2.4），循环的整个执行期都在锁内。这两个事实 Rust 都换成了更硬的表达。先看入口：

```rust
// os/kernel/src/lib.rs —— 逐字引用（switch_to_user 的文档注释头）
/// Entry point for the scheduling loop — never returns to the caller.
///
/// C: `switch_to_user()` — proc.c:299-474 (`NOT_REACHABLE` at 473). All
/// three kernel re-entry paths (hardware interrupt, CPU exception,
/// syscall) funnel into this loop; it is the single place where the kernel
/// hands the CPU back to user code. Design decision D10-1: `-> !` — Rust
/// expresses "never returns" in the type system instead of C's
/// `NOT_REACHABLE` comment.
```

```rust
// os/kernel/src/lib.rs:fn switch_to_user —— 逐字引用（全函数）
#[allow(dead_code)] // reachable only from the divergent boot path / asm entry
pub fn switch_to_user() -> ! {
    // T1 契约标记（test-cmd-smoke.sh stage-3 门）：内核进入调度循环、
    // 将 CPU 交接给用户态进程排程的落点（C main.c:73 bsp_finish_booting
    // 尾部进入调度循环）。
    boot_stage!("entering scheduler\n");
    // C main.c:73 — bsp_finish_booting tails into switch_to_user with the
    // BKL held. S-7: the loop body is the shared `scheduler_loop`; the BSP
    // flavor only names its own CPU (seed + loop are per-CPU now).
    let cpu = unsafe { crate::smp_state_boot_unchecked() }.bsp_cpu_id();
    scheduler_loop(cpu)
}
```

（两块内含仓库内部编号（D10-1、T1、S-7 等），按原文引用。`os/kernel/src/lib.rs:fn switch_to_user` 是薄入口：一条串口路标 + 取 BSP 号 + 进共享循环。"永不返回"三层贯通——`switch_to_user`、`os/kernel/src/lib.rs:fn scheduler_loop`、`os/kernel/src/lib.rs:fn finish_and_restore` 的签名都是 `-> !`，类型系统替代注释哨兵。）

持锁这件事，Rust 没有照搬"C 约定 + 大家自觉"，而是把约定变成编译器能看的见证：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop —— 逐字引用（函数头与锁见证段）
pub(crate) fn scheduler_loop(cpu: crate::proc::CpuId) -> ! {
    use crate::proc::proc_nr;

    // A1: the BKL is held on entry (boot: acquired before smp_init per
    // C main.c:149; AP: acquired in smp_ap_tail after ap_finish_booting).
    // `assume_held` turns that convention into a debug-asserted witness;
    // everything below consumes `*_with(&section)` accessors, so a lost
    // lock panics instead of silently corrupting the tables.
    let section = unsafe { crate::smp::BklSection::assume_held() };
    let table = crate::proc_table_with(&section);
    let smp = crate::smp_state_with(&section);
    let priv_table = crate::priv_table_with(&section);

    // S-7 L5 observability: record that this CPU entered the loop (the L5
    // assertion reads these flags from the BSP after smp_init).
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.sched_loop_entered = true;
    }
```

（内含仓库内部编号（A1、S-7、L5），按原文引用。）

`assume_held` 不是取锁：它把"调用者已持锁"这个 C 里的口头约定转成一个调试期断言的见证对象——`os/kernel/src/smp.rs:fn assume_held` 在 debug 构建下断言锁确实在持有中，然后发出一个 `BklSection`；循环内对进程表、SMP 状态、特权表的一切访问都走 `*_with(&section)` 形态的访问器，把见证当凭证出示。锁还在时这些访问畅通；锁丢了，debug 断言当场 panic，而不是让无锁并发把表写坏。BSP 与 AP 共用同一条循环体——`scheduler_loop(cpu)` 用参数区分 CPU，对应 C 里 `minix3/minix/kernel/arch/i386/arch_smp.c` 的 AP 收尾段与 `minix3/minix/kernel/main.c:bsp_finish_booting` 各自调一句 `switch_to_user()` 的同构形状（§1.1 开头提到的两处调用点）。

三条取舍：

| 选择 | 否掉的替代 | 代价 |
|---|---|---|
| 三层 `-> !` 类型化 | C 的 `NOT_REACHABLE` 注释哨兵（零运行期检查） | 测试内核复用入口需 `#[allow(dead_code)]` 显式豁免 |
| 循环头一次 `assume_held` 见证 | 循环内每步 `bkl_lock()` 重取 | 见证只在 debug 断言生效，release 仍靠纪律 |
| `*_with(&section)` 访问器 | 直接读全局静态 | 每个访问点多传一个凭证参数，签名变长 |

反向推演两头。若循环内改用 `os/kernel/src/smp.rs:fn bkl_lock` 重取——BKL 是非重入的自旋锁，循环已经持有它，重取就是自己等自己，自旋死锁；§3.3 的重派路径在真机上踩过同一族问题，取证见该节。若不用见证、直接读全局——丢锁不再 panic，两个 CPU 各自"以为"自己持锁地改同一张进程表，腐坏静默发生，查无现场。

### 3.2 首次分派合并进每次分派（→§2.1、[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §4.6 的 Step 2）

C 里真实存在的形状是（§2.1 第二段）：第一次分派不走特殊分支——`minix3/minix/kernel/main.c:bsp_finish_booting` 的第 4 拍把 `bill_ptr` 与 `proc_ptr` 两个每 CPU 变量都指到 idle（`minix3/minix/kernel/main.c` 的种子两行），而 idle 在 `minix3/minix/kernel/proc.c:proc_init` 里被按死 `RTS_PROC_STOP`（注释原话 "must not let idle ever get scheduled"）且永不入队。于是第一次进 `switch_to_user` 时，直通检查看 idle 不可运行，落进重挑路径，挑出 VM——"第一次"与"第 N 次"是同一条代码路径。Rust 保留这个形状，把种子挪到循环头：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop —— 逐字引用（种子段）
    // Seed proc_ptr = IDLE (C: main.c:54 — bsp_finish_booting step 2's
    // per-CPU half; the accounting half lives in `set_bill_to_idle`).
    // IDLE is never queued (RTS_PROC_STOP), so the first pass falls
    // through to the pick path — the same first-dispatch behavior as C.
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.proc_ptr = Some(proc_nr::IDLE);
    }
```

（内含仓库内部编号（无），按原文引用。）

种子的记账半与切换半分居两处：`os/kernel/src/proc_table.rs:fn set_bill_to_idle` 在 BSP 收尾（[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §4.6 的 Step 2）里把账本指针安好，`proc_ptr = IDLE` 在循环头由每个 CPU 自己种——对应 C 里 `minix3/minix/kernel/main.c` 那两行紧挨着的种子，Rust 把它们拆到了各自自然的归属地。拆开后"第一次把 CPU 交给 VM"不需要任何特判代码：种子值让首轮的 `need_pick` 判定为真，走 pick 路径，VM 是当时唯一可运行的进程。

| 选择 | 否掉的替代 | 代价 |
|---|---|---|
| 种子 `proc_ptr = IDLE`，首轮自然走 pick | 首轮写一个"从队列取第一个进程"的特殊分支 | 循环体多两行种子代码 |
| `current: Option<ProcNr>` 表达"没挑过" | 引入独立的"已初始化"标志位 | `None` 的语义负担：它只表达"没挑过"一个意思 |

反向推演：若没有种子、`current` 初始为 `None` 且处理不齐——循环第一拍解引用当前进程就会 panic， panic 点在第一次分派而非任何可诊断的位置；种子让 `None` 只承载"这颗 CPU 还没挑过进程"一个语义，首轮与后续轮次共用全部代码。

### 3.3 Stage 3a：被挂起内核调用的循环侧重派（→§2.1 的 KCALL_RESUME 臂、[09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3 的对端）

C 里真实存在的语义是（§2.1 第三段）：五标志循环的 `MF_KCALL_RESUME` 臂在用户态恢复之前补完被挂起的内核调用——`minix3/minix/kernel/system.c:kernel_call_resume` 重派那次调用、清位、做完成半。[09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3 讲了这条链的 VM 一端（MEMREQ 请求怎么停车、VM 怎么服务）；本节讲循环这端怎么接。Rust 把它立为循环里一个独立的阶段（Stage 3a），因为重派在 Rust 的类型与锁环境下多出四个防护点，每个都对应一段真实踩过的坑。先看阶段头与状态守卫：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop —— 逐字引用（Stage 3a 阶段头与状态守卫）
        // ── Stage 3a: KCALL_RESUME consume（C system.c:612-638
        // kernel_call_resume）──
        // VM 服务完成（memreq_reply 置 KCALL_RESUME + 清 VMREQUEST）后，
        // 被挂起的内核调用在此重派：内核侧重跑 kernel_call（此刻目标内存
        // 已由 VM 填充，check_resumed_caller 短路二次挂起），结果经
        // set_ipc_return_code 写回 RAX，随后正常 restore。Fault 臂对位
        // C 的 vmresult≠OK → SIGSEGV。须在 process_misc_flags 之前消费
        //（其后同名臂只做 Fault 兜底）。
        if table
            .get(picked)
            .is_some_and(|p| p.p_misc_flags.is_set(crate::proc::MiscFlagsBits::KCALL_RESUME))
        {
            // 状态守卫：仅在 Completed（VM 已回复）时重派；Pending/Fetched
            // 说明 VM 仍在服务——本轮跳过派发（continue 重挑，VM 可运行、
            // 会被轮到），等下一轮 Completed 再恢复。
            let state_completed = table
                .get(picked)
                .and_then(|p| p.p_vm_suspend.as_ref())
                .map(|c| matches!(c.state, crate::vm::VmSuspendState::Completed(_)));
            if state_completed != Some(true) {
                continue;
            }
            let resumed = crate::vm::kernel_call_resume(picked, table);
```

（内含仓库内部编号（无），按原文引用。）

**防护点一：状态守卫。** `os/kernel/src/vm.rs:fn kernel_call_resume` 只在挂起上下文处于 `Completed`（VM 已回复）时读出结果并清位；`Pending`/`Fetched` 说明 VM 还在服务。守卫不通过时执行 `continue`——注意这不是失败路径：多段挂起（一次拷多页、多区间）是合法形状，本轮把 CPU 让出去（本进程已停排不会被选中，VM 可运行、会得到 CPU），等 VM 服务完的下一轮再恢复。若没有守卫直接重派——对 `Pending` 态的调用重跑，要么对尚未由 VM 填充的目标内存做拷贝（拷进垃圾），要么二次挂起把同一条请求登记两遍。

**防护点二：重派不能走打包了取锁的分派入口。** 调度循环已持锁（循环头一次 `assume_held`，§3.1），而 Rust 的常规分派入口把 `bkl_lock()` 打包在分派前——经它重派就是非重入自旋锁自死锁。所以重派走 `os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner`，把循环的锁见证 `&section` 一并传入：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop —— 逐字引用（Stage 3a 的重派与锁策略）
                    let clock_state = unsafe { crate::clock_state_boot_unchecked() };
                    // C: kernel_call_dispatch 本体不取 BKL（BKL_LOCK 只在
                    // mpx.S 陷入入口）；Rust 的 kernel_call/kernel_call_
                    // dispatch 把 bkl_lock() 打包在 dispatch 前——调度循环
                    // 已持锁（&section，loop 前一次 assume_held），经它们
                    // 重派 = 非重入自旋锁自死锁（真机 c8a 轮实证：memreq
                    // 服务完成后 RS 挂死在 sa1-after 与 pre-restore 之间，
                    // 无任何输出）。故走 inner + 环境 BKL 见证；finish 用
                    // 持锁变体（两处 bkl_unlock 会丢掉调度循环的锁）。
                    let result = crate::syscall::kernel_call_dispatch_inner(
                        picked,
                        table,
                        &mut msg,
                        priv_table,
                        clock_state,
                        &section,
                    );
```

（内含仓库内部编号（c8a），按原文引用。注释里那句"真机实证"记录的挂死现象——memreq 服务完成后 RS 停在恢复前、无任何输出——正是非重入自死锁的现场形状。C 没有这个问题：`minix3/minix/kernel/system.c:kernel_call_resume` 重派用的 `kernel_call_dispatch` 本体不取 BKL，取锁在 `minix3/minix/kernel/arch/i386/mpx.S` 的陷入入口；Rust 的打包版分派入口改变了这个前提，于是重派必须换 `inner` 变体。）

**防护点三：完成半用持锁变体，门归属从挂起上下文读。** 重派完成后的 reply 处理由 `os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl` 承担——持锁变体，因为它内部不再放锁（放锁只会发生在 `finish_and_restore` 的记账点，§3.4）。"门归属"参数决定补完成的 reply 是否立即直写：IPC 腿挂起的调用补完成时不 eager 直写（门标记从挂起上下文读出后传给 finish）；重派再次挂起时，finish 的 VmSuspend 臂在新挂起上下文上凭该参数重新置位，多段挂起路径由此闭合：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop —— 逐字引用（Stage 3a 的完成半与再挂起臂）
                    // C: kernel_call_resume 尾部 kernel_call_finish
                    // （system.c:636）——完成半的 reply 拷贝 / VmSuspend
                    // 再挂起簿记 / NoReply 出队都由它做。
                    // S3 评审修复（P1）：门归属从挂起上下文读出后驱动
                    // finish——IPC 腿挂起的调用补完成时不 eager 直写；
                    // 重派再次挂起时（suspend_for_vm 新建 ctx，不继承
                    // 门标记）finish 的 VmSuspend 臂凭 eager=false 在
                    // 新 ctx 上重新置位，封死多段挂起路径。
                    let door_skip = table
                        .get(picked)
                        .and_then(|p| p.p_vm_suspend.as_ref())
                        .is_some_and(|c| c.resume_skip_eager_reply);
                    crate::syscall::kernel_call_finish_holding_bkl(
                        picked, table, &msg, result, priv_table, false, !door_skip,
                    );
                    match result.reply_wire() {
                        // 完成：交付 RAX + 清挂起态，随后正常 restore。
                        // NK4-C F10b（P0-wire）：本路径是 SYSCALL 腿的延迟
                        // 完成，RAX 与同腿 eager 回执同号（`reply_wire()` 已
                        // 按错误码/数据码约定处理，不再取负）。
                        Some(wire) => {
                            if let Some(p) = table.get_mut(picked) {
                                crate::proc::set_ipc_return_code(
                                    p,
                                    wire as i64,
                                );
                            }
                            if let Some(p) = table.get_mut(picked) {
                                p.clear_vm_suspend();
                            }
                        }
                        // 重派再次挂起（多页/多区间）：finish 的 VmSuspend
                        // 臂已存 saved_msg + 置 KCALL_RESUME + 入队通知 VM
                        // （持锁变体，锁仍归调度循环）。此处不得递归重入
                        // scheduler_loop（每层挂起嵌套一整层调度器帧，内核
                        // 栈会耗尽）——continue 平级重挑：本进程已停排
                        // 不会被选中，VM 可运行、会得到 CPU。
                        None => {
                            continue;
                        }
                    }
```

（内含仓库内部编号（S3、P1、NK4-C、F10b），按原文引用。）

`reply_wire()` 返回 `None` 的分支就是"重派再次挂起"：注释同时立了一条禁令——此处不得递归重入 `scheduler_loop`。反向推演这条禁令：每层挂起若重入一次循环，内核栈上就叠一整层调度器帧（循环体、各阶段局部变量、见证对象）；多段拷贝挂起几层，栈就耗尽几层深。`continue` 是平级重挑——不加深栈，靠下一轮循环接续。

**防护点四：Fault 臂发 SIGSEGV。** VM 若判定地址确实无效，重派返回 `Fault`，进程收到 SIGSEGV——对位 C 里 `minix3/minix/kernel/system.c:kernel_call_resume` 的完成半在 vmresult 非 OK 时的信号路径：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop —— 节选（Fault 臂；注释有删节）
                crate::vm::VmCheckResult::Fault => {
                    #[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
                    {
                        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                        static RFS: AtomicUsize = AtomicUsize::new(0);
                        if RFS.fetch_add(1, AtomicOrd::Relaxed) < 16 {
                            let (ep, pdmv) = table
                                .get(picked)
                                .map_or((0i32, 0u64), |q| (q.p_endpoint.0, q.p_delivermsg_vir.0));
                            let root = crate::current_root_phys().map(|r| r.0).unwrap_or(0);
                        }
                    }
                    crate::syscall_signal::cause_signal(
                        picked,
                        crate::syscall_signal::SIGSEGV,
                        table,
                        priv_table,
                    );
                }
```

（本块为节选：原码在这段代码上方有一条多行注释、块内还有一个 `if` 的空体——那是一条诊断探针，专供 riscv64 非 mock 构建生效，计数上限 16 次，读出的 `(ep, pdmv, root)` 三个绑定值在块内没有消费者（`let` 之后未使用）。探针注释内含仓库内部编号，按本篇纪律不转录，要点如实登记于此：它是取证残迹，不是功能逻辑；删去它不影响行为。除删去其上方那条注释外，本块与源文件逐行一致——块内的空探针体是源码原样保留，不是删除项。）

四处防护与 C 的对应关系收进一张表：

| 防护点 | C 侧对应 | Rust 的额外约束来源 |
|---|---|---|
| 状态守卫（仅 Completed 重派，否则 continue） | C 无此守卫——`kernel_call_resume` 的断言假设恢复时 VM 必已回复 | Rust 把挂起态建模为显式枚举，非 Completed 态在类型上可辨 |
| `kernel_call_dispatch_inner` + 环境锁见证 | C 的 dispatch 本体不取 BKL，重派天然安全 | Rust 打包版入口自带取锁，重派必须绕开 |
| `kernel_call_finish_holding_bkl` + 门归属参数 | C 的 `kernel_call_finish` 完成半 | 持锁变体避免中途放锁；门归属封多段挂起的 reply 直写时序 |
| 再挂起不递归（continue 平级重挑） | C 无此形状——C 的恢复不重入调度循环 | 递归会按嵌套层数消耗内核栈 |
| Fault 臂发 SIGSEGV | C 的完成半在 vmresult 非 OK 时走信号路径 | 重派的 Fault 变体在类型上可辨，信号在循环侧补发 |

### 3.4 finish_and_restore 的步骤化与 BKL 释放点（→§2.1 终段）

C 里真实存在的负担是（§2.1 第四段）：终段六连挤在 `switch_to_user` 的尾巴里，六件事没有名字、只有次序；BKL 的释放藏在 `context_stop` 内部（§2.4 的 `must_bkl_unlock`），从函数签名上看不出"这个调用会放锁"。Rust 把终段立成独立函数 `os/kernel/src/lib.rs:fn finish_and_restore`，按语义数是七步；编号注释在源码里只落到步 2-7（步 1 的量子断言注释无编号，"步 1"是本篇赋予的口径），每步一条注释回指 C 的对应行；放锁显式化。步 1 是量子断言，步 2 是记账与放锁：

```rust
// os/kernel/src/lib.rs:fn finish_and_restore —— 逐字引用（步 1-2：量子断言、记账与放锁）
    // C:438 — debug_assert(p->p_cpu_time_left). After the quantum stage
    // every path that reaches here has time left: kernel-scheduled
    // processes had their quantum reset (`sched_proc_no_time`), and an
    // exhausted preemptible process was dequeued (not runnable → the loop
    // re-picked before dispatch).
    debug_assert!(
        table
            .get(picked)
            .is_some_and(|p| p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0),
        "finish_and_restore: picked process has no quantum left"
    );

    // 2. context_stop(KERNEL) — C:440. `smp` and `kernel` are distinct
    // objects (SmpState owns per-CPU scheduler data, not the process
    // table), so the two &mut borrows — `smp` via reborrow and `kernel`
    // out of `table` — never alias (same disjointness argument as
    // clock::decrement_quantum_in's doc comment).
    let tsc = crate::clock::read_tsc();
    let kernel = table
        .get_mut(crate::proc::proc_nr::KERNEL)
        .expect("finish_and_restore: KERNEL pseudo-process slot must exist");
    let (_exhausted, tsc_delta) = crate::clock::decrement_quantum_in_with_delta(smp, kernel, tsc);
    // S-6.4/I-16 + C-25/C-26: shared context_stop KERNEL-branch accounting
    // (C:440 site — the pre-S-6.4 code here missed BOTH the per-state bucket
    // and the p_cycles accumulation that the idle site had).
    account_process_stop(table, smp, crate::proc::proc_nr::KERNEL, tsc_delta, section);
    // D-9 (C arch_clock.c:279-281) — consume kbill_kcall with the same
    // whole-delta context_stop uses; must run before the BKL release
    // below (see consume_kbill_kcall doc).
    if tsc_delta > 0 {
        // C-25：与上面 idle 同序（C arch_clock.c:274-281 的两块并列）。
        consume_kbill_ipc(table, tsc_delta, section);
        consume_kbill_kcall(table, tsc_delta, section);
    }
    // C releases the BKL inside context_stop (must_bkl_unlock,
    // arch_clock.c:226-233); the restore below is the last kernel act.
    crate::smp::bkl_unlock();
```

（内含仓库内部编号（S-6.4、I-16、C-25、C-26、D-9），按原文引用。记账四连——量子扣减、状态桶与总周期、kbill 的 ipc 与 kcall 两笔——用**同一个** `tsc_delta`，对应 C 的 `minix3/minix/kernel/arch/i386/arch_clock.c:context_stop` 里 KERNEL 分支的公共尾；delta 拆开算，四笔账之间就会出现不重不漏被破坏的缝隙。放锁在记账之后、恢复之前，注释明说这是 C 的释放点。）

步 3 是 FPU 归属，三架构策略不同，riscv64 有一次修正：

```rust
// os/kernel/src/lib.rs:fn finish_and_restore —— 节选（步 3：FPU 归属；注释有删节）
    // 3. FPU ownership — C:443-446.
    // Per-CPU bookkeeping (C reads `get_cpulocal_var(fpu_owner)`): the
    // owner is tracked on the CPU that will run the picked process, not
    // the BSP — on SMP the stale read made non-BSP CPUs see the BSP's
    // owner and skip the disable, leaving both CPUs' user FP unlocked.
    let fpu_owner = smp
        .cpu_local(crate::current_cpu_id())
        .and_then(|l| l.fpu_owner);
    let fpu = CurrentFpuArch::default();
    if fpu_owner != Some(picked) {
        fpu.disable(); // non-owner: next FP instruction traps (#NM) — C: enable_fpu_exception
    } else {
        fpu.enable(); // owner keeps the FPU — C: disable_fpu_exception (clts)
    }
    ...
    // AF-8（P0·回溯审计 2026-10-01 定谳）：旧实现非 owner 误写 FS=Initial
    // (0b01)。但本 crate 门控谓词 `riscv64_illegal_insn_is_fpu_trap`
    // （trap_dispatch.rs）要求 `sstatus.FS==Off(0b00)` 才判 FP 门控陷阱；
    // FS=Initial 时「FPU available 不陷阱」（fpu.rs 头 doc + RISC-V Priv
    // ISA FS 字段语义）⇒ 非 owner 返回 user 后首条 FP **不陷阱**⇒ 轮转腿
    // 不可达死代码、riscv user↔user FP 未受保护（禁 FP 只护 kernel↔user）。
    // 修正＝非 owner 用 FS_OFF。aarch64 的活写足够（CPACR 不在帧内），riscv
    // 必须走 ctx——两架构策略点在各自载体上对位 C proc.c:443-446。
    #[cfg(target_arch = "riscv64")]
    if let Some(p) = table.get_mut(picked) {
        let fs = riscv_lazy_fs_field(fpu_owner == Some(picked));
        p.cpu_context.set_fs_field(fs);
    }
```

（本块为节选：`...` 处省略一条多行注释——那条注释登记了 riscv FS 位修正的另一个来源记录，内含仓库内部编号，不转录；其结论与下面保留的修正注释一致：非 owner 必须写 FS=Off。内含仓库内部编号（AF-8）与日期，按原文引用。）

这条修正值得读全：riscv 的 FPU 使能位不住在独立的硬件开关里，而是住在 `sstatus` 的 FS 字段——而 `sstatus` 会被陷阱出口的帧回装冲销，所以活写没有载体，FPU 门控的载体必须是进程的 CPU 上下文（ctx）。归属策略因此是：拥有者写 FS=Dirty（活寄存器延续、不陷阱），非拥有者写 FS=Off（首条浮点指令陷阱，进 C `minix3/minix/kernel/proc.c:copr_not_available_handler` 对应的轮转腿——保存旧拥有者、装回自己、移交归属）。反向推演修正前的形状：非拥有者若写 FS=Initial——FS=Initial 的语义是"FPU 可用、不陷阱"，非拥有者的首条浮点指令直接执行，轮转腿永远不被触发成死代码，用户态之间的浮点保护失效。x86_64 与 aarch64 的载体不同（CR0 的 TS 位 / CPACR 寄存器都不在恢复帧内），活写有效——同一语义、三种载体，差异留在各自的 arch 层。

步 4-6 与步 7 的分派在 §4.2 逐步走。本节的取舍表：

| 选择 | 否掉的替代 | 代价 |
|---|---|---|
| 终段立为 `finish_and_restore` 七步函数 | 六连留在循环体内无名字 | 多一层函数调用与参数传递 |
| 放锁显式（记账后 `bkl_unlock()`） | 像 C 那样藏在记账函数内部 | 释放点从"函数副作用"变成"调用方职责"，调用方必须记得放 |
| riscv FS 载体选 ctx（owner→Dirty / 非 owner→Off） | 活写 sstatus 寄存器 | 每次分派多一次 ctx 写 |

### 3.5 处置表（→第 2 章各节）

C 循环里的每件东西，去向如下。每行给出验证方式——测试名见第 5 章，走查节见第 4 章：

| C 的东西（出处） | 处置 | Rust 对应 | 验证 |
|---|---|---|---|
| 直通检查 `goto check_misc_flags`（§2.1 第一段） | 保留（语义） | Stage 1+2 的 `need_pick` 判定 | §4.1 走查 |
| `not_runnable_pick_new` 标签与两处 goto（§2.1 第二、三、四段） | 改型 | 外层 `loop` 的 `continue`（布尔返回替代 goto） | §4.1 走查 |
| PREEMPTED 头/尾重排（§2.1 第二段） | 保留 | `os/kernel/src/lib.rs:fn requeue_if_preempted` | `test_requeue_preempted_*` 三条（第四条同族测试不含该子串，§5.1 按前缀口径计 4） |
| idle 兜底（§2.2） | 保留 | `os/kernel/src/lib.rs:fn idle` | §4.3 走查 |
| `switch_address_space` 与 `tlb_must_refresh` 预计算（§2.1 第二段） | 保留（含次序） | Stage 1+2 内先算后切 | §4.1 走查 |
| 五标志 while 循环（§2.1 第三段） | 保留 | `os/kernel/src/proc_table.rs:fn process_misc_flags` | §4.1 走查 |
| `MF_KCALL_RESUME` 臂（§2.1 第三段） | 拆分 | Stage 3a 全量重派 + `process_misc_flags` 同名臂兜底 | §3.3 四防护 |
| 其余四臂 callee（§2.1 第三段） | 保留 | `process_misc_flags` 的 DELIVERMSG/SC_DEFER/SC_TRACE/SC_ACTIVE 臂 | 语义归 `12-ipc-core`/`13-syscall-dispatch`（两篇，尚未重生成） |
| 量子检查 `proc_no_time`（§2.1 第四段） | 折叠 | `os/kernel/src/proc_table.rs:fn check_quantum`（含量子后重验） | §4.1 走查 |
| 终段六连（§2.1 第四段） | 步骤化 | `os/kernel/src/lib.rs:fn finish_and_restore` 七步 | §4.2 走查 |
| `p_kern_trap_style` 记录位与 KTS 常量（§1.2） | 改型 | `os/arch/src/arch/trap_style.rs:enum TrapStyle`（枚举化） | `test_trap_style_*` 五条 |
| `NOT_REACHABLE` 哨兵（§2.1 第四段） | 消失 | 被三层 `-> !` 吸收 | §3.1 |
| 函数头 `#if 0` 统计块（§2.3） | 消失 | 无对应（调试遗存不继承） | —— |
| `KTS_INT_UM`/`KTS_SYSENTER`（§1.2） | 消失 | 64 位无 usermapped 入口，枚举不建模（`os/arch/src/arch/trap_style.rs` 的模块注释明言） | `test_trap_style_from_raw_rejects_unknown_and_unmodeled` |

一处披露口径要汇总登记：本篇逐字引用的 Rust 注释里带着多处 C 坐标，其中一批已与现码漂移——`main.c:54`（proc_ptr/bill 的种子两行现码在 :56-57，[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §4.6 已登记同族）、注释里的 `C main.c:73`（bsp 尾部的 switch_to_user 调用现码在 :107 附近）、idle 注释里的 `C:185`/`C:186-187`（现码顺延一行）、`C:207`（context_stop 现码 :208）、Stage 3a 注释里的 `system.c:636`（kernel_call_finish 现码 :637）、恢复分派注释里的 `arch_system.c:597-598`（panic 语句现码 :599）。引文按原文保留、坐标以现码为准——与 08/09 两篇的登记口径一致。

## 4. 实现详解

### 4.1 scheduler_loop 逐阶段走查（→§3.1-§3.3）

§3.1-§3.3 的三个决策——类型化出口、种子合并、Stage 3a 重派——都落在 `os/kernel/src/lib.rs:fn scheduler_loop` 一个函数体里。函数的文档注释自带一张 C↔Rust 对照表，先借它当地图：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop —— 节选（文档注释的五阶段对照表）
/// C threads one function body through two labels
/// (`not_runnable_pick_new`, `check_misc_flags`). Rust re-expresses the
/// same control flow as an outer loop whose body runs the five stages in
/// order; the two C `goto not_runnable_pick_new` exits become `continue`.
/// Stage mapping:
///
/// | Stage | C lines | Rust |
/// |-------|---------|------|
/// | probe current process | proc.c:309-315 | runnability check on `proc_ptr` |
/// | not_runnable_pick_new | proc.c:321-349 | requeue + pick-or-idle + `switch_address_space` |
/// | check_misc_flags | proc.c:351-415 | `process_misc_flags` (bool return replaces goto) |
/// | quantum check | proc.c:421-428 | `check_quantum` (folded re-check) |
/// | finish + restore | proc.c:437-474 | `finish_and_restore` |
```

（节选自 `os/kernel/src/lib.rs:fn scheduler_loop` 的文档注释，对应决策见 §3.1。表内的 C 行号区间是源码注释原文，按原文引用。）

循环体从种子（§3.2 已引）后的 `loop {` 开始。Stage 1+2 对应 C 的前三拍：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop —— 逐字引用（Stage 1+2）
    loop {
        // ── Stage 1+2: probe current process / pick a new one ──
        // C: proc.c:309-349. `current` is the per-CPU proc_ptr snapshot;
        // None (or a non-runnable process) routes into the pick path.
        let mut current: Option<crate::proc::ProcNr> = smp
            .cpu_local(cpu)
            .and_then(|l| l.proc_ptr);

        // C: proc.c:314 — `if (proc_is_runnable(p)) goto check_misc_flags;`
        // The current process is re-dispatched only when it is still
        // runnable; anything else (None seed, blocked, stopped) enters the
        // pick path below.
        let need_pick = current
            .is_none_or(|nr| {
                !table.get(nr).is_some_and(|p| p.is_runnable())
            });
        // C: proc.c:306 — `tlb_must_refresh` is schedule()-local.
        let mut tlb_must_refresh = false;
        if need_pick {
            // not_runnable_pick_new — C: proc.c:321-330.
            if let Some(cur) = current {
                requeue_if_preempted(table, cur);
            }
            // C: proc.c:338-340 — `while (!(p = pick_proc())) idle();`
            let picked = loop {
                if let Some(p) = pick_and_bill(table, smp, priv_table, cpu) {
                    break p;
                }
                idle(&section, table, smp, priv_table, cpu);
            };
            // C: proc.c:343 — `get_cpulocal_var(proc_ptr) = p;`
            current = Some(picked);
            if let Some(local) = smp.cpu_local_mut(cpu) {
                local.proc_ptr = Some(picked);
            }
            // C: proc.c:345-347 — if the picked process carries MF_FLUSH_TLB
            // and IS this CPU's ptproc, its translations are resident here
            // and possibly stale: switch_to_user must refresh the TLB even
            // when the root looks unchanged. Computed BEFORE
            // switch_address_space, which may retarget ptproc (C's order).
            tlb_must_refresh = table
                .get(picked)
                .is_some_and(|p| p.needs_tlb_refresh(crate::current_ptproc_nr()));
            // C: proc.c:349 — switch_address_space(p).
            switch_address_space(table, picked);
        }
        let picked = current.expect("scheduler loop: proc_ptr seeded or picked above");
```

（内含仓库内部编号（无），按原文引用。块内的 C 行号是源码注释原文；C 侧语义的逐字依据在 §2.1 第二段。）

逐段对上：`need_pick` 判定是 C 直通检查的类型化版本——`None`（种子未挑）或不可运行都进 pick 路径；`requeue_if_preempted` 对应 PREEMPTED 重排（§4.3 逐字）；pick-or-idle 内层循环对应 `while (!(p = pick_proc())) idle();`，`os/kernel/src/lib.rs:fn pick_and_bill` 与 `os/kernel/src/lib.rs:fn idle` 的函数体在 §4.3；`proc_ptr` 更新、TLB 预计算、`os/kernel/src/lib.rs:fn switch_address_space` 依次对应 C 的三行。次序约束在这里落地：`tlb_must_refresh` 必须在 `switch_address_space` **之前**算——切表会改每 CPU 的 ptproc 记录，预计算要回答的问题是"被选中进程的翻译是否常驻本 CPU"，判据是切表前的 ptproc；顺序颠倒，刷新判定就基于新 ptproc 做，该刷的不刷，进程带着陈旧翻译上 CPU 取指错位。C 的 `minix3/minix/kernel/proc.c:switch_to_user` 也是这个次序（声明在函数头、赋值在切表前），§2.1 第二段逐字可见。

Stage 3/4/5 是循环的收尾三拍，两个 `continue` 就是 C 两条 goto 的 Rust 形态：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop —— 逐字引用（Stage 3/4/5）
        // ── Stage 3: misc flags (check_misc_flags) ──
        // C: proc.c:351-415. Runs under the BKL (the contract documented
        // on process_misc_flags / arch_do_syscall). A `false` return is
        // C's `goto not_runnable_pick_new` (proc.c:413-414): the process
        // became non-runnable while being serviced.
        if !table.process_misc_flags(picked, &crate::ipc::KernelUserCopy, priv_table) {
            continue;
        }

        // ── Stage 4: quantum check ──
        // C: proc.c:421-428. `check_quantum` folds both C checks:
        // `proc_no_time` when the quantum is exhausted (with its
        // scheduler-notify policy split) and the post-quantum runnability
        // re-check (C:427-428) — one `false` exit back to the pick path.
        if !table.check_quantum(picked, priv_table, &section) {
            continue;
        }

        // ── Stage 5: finish + restore (never returns) ──
        finish_and_restore(table, smp, picked, tlb_must_refresh, &section);
    }
}
```

（内含仓库内部编号（无），按原文引用。）

Stage 3 的 `os/kernel/src/proc_table.rs:fn process_misc_flags` 把 C 的五标志 while 循环收进一个布尔返回的函数：返回 `false` 等价于 C 的 `goto not_runnable_pick_new`（进程在服务过程中变得不可运行），循环侧以 `continue` 回到 Stage 1 重新挑人；返回 `true` 则五臂消费完毕、无臂把进程弄停。Stage 4 的 `os/kernel/src/proc_table.rs:fn check_quantum` 把 C 的两查折叠成一次调用——量子耗尽时按调度策略处理（含通知），再做量子后的可运行重验；一个 `false` 出口回到 pick 路径，对应 C 量子检查后的第二条 goto。Stage 5 交出 `tlb_must_refresh` 的预计算结果，进入永不返回的 `finish_and_restore`。

### 4.2 finish_and_restore 七步逐步（→§3.4）

步 1（量子断言）与步 2（记账四连 + 放锁）的代码已在 §3.4 逐字引过，这里补齐其余各步并按七步顺序对表。先列总表（决策见 §3.4，C 侧逐字在 §2.1 第四段）：

| 步 | 动作 | C 对应 | 备注 |
|---|---|---|---|
| 1 | 量子断言（`debug_assert`：余时必须为正） | `minix3/minix/kernel/proc.c:switch_to_user` 终段的 `assert(p->p_cpu_time_left)` | 走到这里的每条路径都已保证有余时——内核调度进程的量子已被重置，耗尽的可抢占进程已被出队重挑 |
| 2 | 记账四连（同一 delta）+ `bkl_unlock()` | `minix3/minix/kernel/arch/i386/arch_clock.c:context_stop` 的 KERNEL 分支 | §3.4 逐字；delta 拆开则四笔账不重不漏被破坏——四笔都是"同一内核段时长"的分摊，各算各的 delta 就各量各的段 |
| 3 | FPU 归属（owner→enable，非 owner→disable；riscv FS 位走 ctx） | 同上 `enable/disable_fpu_exception` 对 | §3.4 逐字（含修正记录） |
| 4 | 清 `MF_CONTEXT_SET` | C 终段的 `p->p_misc_flags &= ~MF_CONTEXT_SET` | 置位点在 `minix3/minix/kernel/arch/i386/arch_system.c:arch_proc_setcontext` |
| 5 | `MF_FLUSH_TLB` 消费（`tlb_must_refresh` 为真才真刷，位一律清） | C 终段的 `MF_FLUSH_TLB` 分支 | 预计算来自 Stage 1+2（§4.1） |
| 6 | `restart_local_timer()` | C 终段的同名调用 | 自动重装时钟源上是空操作 |
| 7 | 读进入风格 → 消费记录 → 分派恢复 | `minix3/minix/kernel/arch/i386/arch_system.c:restore_user_context` | 本节逐字 |

步 4-6 的代码：

```rust
// os/kernel/src/lib.rs:fn finish_and_restore —— 逐字引用（步 4-6）
    // 4. Clear MF_CONTEXT_SET — C:451.
    if let Some(p) = table.get_mut(picked) {
        p.p_misc_flags.clear(crate::proc::MiscFlagsBits::CONTEXT_SET);
    }

    // 5. MF_FLUSH_TLB consume — C:458-464: when the pick point found the
    // process's translations resident on this CPU and flagged stale, flush
    // the local TLB; the flag clears either way.
    if let Some(p) = table.get_mut(picked) {
        consume_flush_tlb_flag(p, tlb_must_refresh, || {
            // SAFETY: paging is enabled (kernel code is executing, about to
            // return to user mode); the flush is CPU-local — C refresh_tlb
            // (earm cpufunc.h:162-168; i386 write_cr3(read_cr3())).
            unsafe { CurrentTlbArch::flush_all() };
        });
    }
    // 6. restart_local_timer — no-op on auto-reloading sources.
    restart_local_timer();
```

（内含仓库内部编号（无），按原文引用。）

步 7 是恢复分派本体。先消费记录（读风格、清回 NoEntry——§2.3 的"读后即清"配对语义原样保留），再按风格选恢复序列：

```rust
// os/kernel/src/lib.rs:fn finish_and_restore —— 逐字引用（步 7：记录消费与分派）
    // 7. Dispatch on the recorded entry style, then rebuild the frame from
    // the arch-private context and restore.
    // C: restore_user_context (arch_system.c:577-610) — read the recorded
    // trap style, clear it (C:585), and select the register restore sequence.
    let (ctx, return_seq, fault_flush_va) = {
        let p = table
            .get_mut(picked)
            .expect("finish_and_restore: picked ProcNr out of table range");
        let style = p.trap_style;
        // Consume the record so the next dispatch cannot reuse this entry's
        // style (C:585 — p_kern_trap_style = KTS_NONE before branching).
        p.trap_style = TrapStyle::NoEntry;
        // NK4-A C-3 迭代4（F2 内核辅助 flush 的 clear 点最小闭合）：故障
        // 填充恢复时，在本进程 CR3 已激活的此刻 invlpg 其故障页。INVLPG
        // 只击落当前 CR3 标签的非 G 项——clear 时点（VM 的 CR3）杀不到
        // 本进程的陈旧项，必须在 iretq 前的原上下文里做（2026-09-22 真机
        // 证据：RS 同址 0x2246c0 err=0x15 永久重震，内存 PTE 已是
        // P|U|X）。
        let fault_flush_va = p.p_fault_addr;
        p.p_fault_addr = None;
        (p.cpu_context, style.return_sequence(), fault_flush_va)
    };
    match return_seq {
        Some(ReturnSequence::FullContext) => {}
        // Fast-syscall entries save a skinny frame; returning through the
        // full-context path would feed the mode switch user-influenced
        // garbage. The pairing fast return lands with the asm entry work
        // (smp_todo S-8) — until then no entry path records Syscall, so
        // this arm converts silent corruption into a loud stop.
        Some(ReturnSequence::FastSyscall) => {
            panic!("restore_user_context: fast-syscall return sequence lands with S-8")
        }
        // C: arch_system.c:597-598 — panic("no entry trap style known").
        None => panic!("no entry trap style known"),
    }
    let mut frame = <CurrentCpuContextArch as CpuContextArch>::TrapFrame::default();
    <CurrentCpuContextArch as CpuContextArch>::apply_to_trap_frame(&ctx, &mut frame);
```

（内含仓库内部编号（NK4-A、C-3、S-8），按原文引用。`fault_flush_va` 的语义：VM 辅助填页完成后的恢复，要在此刻——本进程 CR3 已激活、iretq 之前的原上下文里——对故障页做一次局部失效；那条注释里的真机证据说明了为什么失效点不能更早。三个 arm 与 C 的对应：`FullContext` 对位 C 的 INT 族 case；`FastSyscall` 眼下是响亮的占位——没有入口路径记录 Syscall 风格，一旦出现就直接停机，把"静默腐坏"换成"大声停下"；`None` 对位 C 的 `panic("no entry trap style known")`。）

`fault_flush_va` 的消费在恢复前的最后一点：x86_64 对故障页执行 `invlpg`；riscv64 的对应位置做全地址空间刷新（`sfence.vma zero, zero`——fill 写 PTE 的路径在 riscv 上没有随写刷新，被恢复进程的 TLB 里缓存着填页前的旧平移，恢复后 CPU 重跑同一条指令会反复故障，这段修正的源码注释内含仓库内部编号，按本篇纪律不转录，语义如实登记于此）。

### 4.3 idle、pick_and_bill 与 requeue_if_preempted（→§2.2/§2.3；决策回指 §3.2）

三个辅件对应 C 的三个函数。先是选人的薄接口（§2.3 的 `pick_proc` 对应物）：

```rust
// os/kernel/src/lib.rs:fn pick_and_bill —— 逐字引用（全函数）
fn pick_and_bill(
    table: &mut crate::proc_table::ProcessTable,
    smp: &mut crate::smp::SmpState,
    priv_table: &crate::kpriv::PrivTable,
    cpu: crate::proc::CpuId,
) -> Option<crate::proc::ProcNr> {
    let picked = table.sched_for_cpu(cpu).pick_proc(table.procs_slice())?;

    // C: proc.c:1808-1809 — `if (priv(rp)->s_flags & BILLABLE)
    // get_cpulocal_var(bill_ptr) = rp;`
    if is_billable(table, priv_table, picked) {
        if let Some(local) = smp.cpu_local_mut(cpu) {
            local.bill_ptr = Some(picked);
        }
    }
    Some(picked)
}
```

（内含仓库内部编号（无），按原文引用。队列扫描本体在 `os/kernel/src/sched.rs:fn pick_proc`——按优先级从 0 号队列扫到 15 号（`NR_SCHED_QUEUES = 16`），取第一个非空队列的队首；算法本体归 `11-scheduling-primitives`（调度原语篇，尚未重生成），循环拿到的就是这一层接口。）

再是被抢占进程的重排（§2.1 第二段的头/尾决策）：

```rust
// os/kernel/src/lib.rs:fn requeue_if_preempted —— 逐字引用（全函数）
/// Re-queue a PREEMPTED process according to its remaining quantum.
///
/// C: proc.c:322-330 (inside `not_runnable_pick_new`). The flag is cleared
/// with the raw flag primitive — deliberately NOT `rts_unset`, whose
/// auto-enqueue is tail-only; C re-decides head-vs-tail from
/// `p_cpu_time_left` (a process preempted mid-quantum re-enters at the
/// HEAD of its priority queue to finish its slice). Both the head and tail
/// re-enqueue target the process's assigned CPU (`rp->p_cpu`, C
/// proc.c:1688/1614), not a hardcoded one.
///
/// A process that is not runnable after the clear (blocked again by the
/// preempting work) is left alone — C: proc.c:324 guards the enqueue the
/// same way.
fn requeue_if_preempted(
    table: &mut crate::proc_table::ProcessTable,
    nr: crate::proc::ProcNr,
) {
    use crate::proc::RtsFlagsBits;
    use core::sync::atomic::Ordering;

    let preempted = table
        .get(nr)
        .is_some_and(|p| p.p_rts_flags.is_set(RtsFlagsBits::PREEMPTED));
    if !preempted {
        return;
    }
    if let Some(p) = table.get_mut(nr) {
        p.p_rts_flags.clear(RtsFlagsBits::PREEMPTED);
    }
    if table.get(nr).is_some_and(|p| p.is_runnable()) {
        let has_quantum = table
            .get(nr)
            .is_some_and(|p| p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0);
        // C routes both the head and tail re-enqueue to the process's own
        // CPU (`rp->p_cpu`), never a hardcoded BSP.
        let cpu_id = table.get(nr).map_or(crate::proc::CpuId::BSP, |p| {
            crate::proc::CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire))
        });
        if has_quantum {
            table.sched_enqueue_head(nr, cpu_id);
        } else {
            // C proc.c:326-330 — the slice is spent, so the process
            // re-enters at the TAIL via enqueue(p), which also evaluates
            // preemption against the CPU-local current.
            table.sched_enqueue(nr, cpu_id);
        }
    }
}
```

（内含仓库内部编号（无），按原文引用。文档注释里那句 "deliberately NOT `rts_unset`" 是本节的约束所在：位清除用裸原语，因为 `rts_unset` 的自动入队只有尾插一条路——用了它，余时头插语义就丢了；§2.1 第二段的响应性反转推演在这里获得 Rust 侧的对应物。）

最后是 idle。函数体逐段引（两条内含仓库内部编号的注释按本篇纪律省略，要点在正文转述）：

```rust
// os/kernel/src/lib.rs:fn idle —— 节选（步 1-3；注释有删节）
    // 1. proc_ptr = idle_proc (C:185).
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.proc_ptr = Some(idle_nr);
    }
    // bill_ptr = idle_proc if BILLABLE (C:186-187).
    if is_billable(table, priv_table, idle_nr)
        && let Some(local) = smp.cpu_local_mut(cpu)
    {
        local.bill_ptr = Some(idle_nr);
    }

    // 2. switch_address_space_idle() (C: proc.c:190 → 161-171) — `CONFIG_SMP`
    //    in C. Before halting, point this CPU at VM's address space so the
    //    kernel + VM pages are guaranteed mapped when the wake interrupt runs;
    //    C's own comment is explicit that this matters "only if more than 1
    //    cpus are available" — exactly the `-smp4` path the standard launcher
    //    exercises. Gated on `ncpus > 1` to mirror C's `#ifdef CONFIG_SMP` and
    //    preserve the single-CPU (`-smp1`) boot milestone. Runs under the BKL
    //    (released only at step 4 below), so the root switch is serialized.
    //    Premise: `switch_address_space` records the root on the hardware
    //    `current_cpu_id()` slot — in production a CPU only ever runs its own
    //    `idle`, so `cpu` (the loop parameter) == the executing hardware CPU.
    if smp.ncpus() > 1 {
        switch_address_space(table, proc_nr::VM_PROC_NR);
    }

    // 3. cpu_is_idle = 1 (C:192-193) + the AP stop-local-timer branch
    //    (C: proc.c:194-196 — `if (cpuid != bsp_cpu_id) stop_local_timer();
    //    else restart_local_timer();`). C masks a halted AP's LAPIC timer so
    //    the AP stays quiet until an interrupt wakes it (C's `context_stop_idle`
    //    at interrupt entry then restarts it). The one-shot local timer is
    //    re-armed unconditionally by `local_tick` on every CPU, so without this
    //    mask a halted AP keeps taking timer ticks forever and never truly
    ...
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.cpu_is_idle = true;
    }
    if smp.ncpus() > 1 && cpu != smp.bsp_cpu_id() {
        crate::clock::stop_local_timer();
    } else {
        restart_local_timer();
    }
```

（本块为节选：步 3 注释自"停掉的 AP 若无掩码会一直收 tick"之后的部分省略——那句登记了 AP 定时器掩码与相邻步骤的补齐关系，内含仓库内部编号；`if` 分支上方还有一整段 CONTRACT 注释（启用 AP 跑用户工作前必须先落地的三个配套臂的契约清单），同样内含仓库内部编号，两处一并省略，契约要点转述于下段。其余与源文件逐行一致。）

被省略的 CONTRACT 清单的要点：停掉 AP 本地定时器只是 C 设计的一半，C 还给它配了三个臂——中断入口的 `minix3/minix/kernel/arch/i386/arch_clock.c:context_stop_idle`（清 idle 标志并重启定时器）、入队唤醒臂（往空闲 CPU 的队列放可运行进程时发 IPI 叫醒它）、以及一个把非 BSP 请求钳到 BSP 的守卫；三个臂的现有落地状态与启用条件以源码注释为准。今天的形状下每个进程都固定在 BSP，AP 只会走到 idle，掩码因此安全。

步 4-5（记账与停机）：

```rust
// os/kernel/src/lib.rs:fn idle —— 逐字引用（步 4-5）
    // 4. context_stop(KERNEL) — charge the kernel-execution delta and
    // advance the TSC baseline (C:207; the quantum decrement itself is
    // skipped for the endpoint < 0 pseudo-process, arch_clock.c:314).
    // # I-6① resolution (2026-09-09): C also accumulates `kernel_ticks[cpu]`
    // and `p->p_cycles` here (arch_clock.c:231-232). `kernel_ticks[cpu]`
    // (glo.h:86) is deliberately NOT wired — a whole-C-tree grep shows no
    // reader anywhere (declare-only in glo.h, accumulate-only in
    // arch_clock.c), so wiring it would be write-only state (the D-34
    // standard). `p_cycles` IS observable (GET_PROC, misc.rs) and the
    // KERNEL-branch delta is accumulated below.
    let tsc = crate::clock::read_tsc();
    let kernel = table
        .get_mut(proc_nr::KERNEL)
        .expect("idle: KERNEL pseudo-process slot must exist");
    let (_exhausted, tsc_delta) = crate::clock::decrement_quantum_in_with_delta(smp, kernel, tsc);
    // S-6.4/I-16 + C-25/C-26: context_stop KERNEL 分支的**全部**周期账
    // （状态桶 + 总周期 + cpuavg）——单点、不重不漏。
    account_process_stop(table, smp, crate::proc::proc_nr::KERNEL, tsc_delta, section);
    // D-9 — idle 的 context_stop 等价同样消费 kbill（C 的消费块是
    // context_stop 公共尾部，不区分 USER/KERNEL/IDLE 分支）。
    if tsc_delta > 0 {
        // C-25：C 的两块并列（先 ipc 后 kcall），同一个 delta 各记一份。
        consume_kbill_ipc(table, tsc_delta, section);
        consume_kbill_kcall(table, tsc_delta, section);
    }

    // 5. BKL release (C: context_stop's must_bkl_unlock — arch_clock.c:
    // 226-233) then halt with interrupts enabled until the next interrupt.
    crate::smp::bkl_unlock();
    minix_arch::CurrentSmpArch::idle_halt();

    // Re-acquire the BKL: the halt window released it and the wake
    // interrupt's handler has returned. Every state access below the
    // return point (pick_proc, queues, priv table) must hold the BKL.
    crate::smp::bkl_lock().transfer();
```

（内含仓库内部编号（I-6、D-34、S-6.4、I-16、C-25、C-26、D-9），按原文引用。步 4 的记账四连与 `finish_and_restore` 步 2 同构（§3.4）；步 5 的放锁对应 C 的 `minix3/minix/kernel/arch/i386/arch_clock.c:context_stop`（§2.4 逐字），停机窗口放锁、唤醒中断重新取锁——循环的其余状态访问重新持锁后才开始。步 4 注释里 `kernel_ticks[cpu]` 不接线的理由是注释原文：全 C 树检索无读者，接了就是只写不读的状态。）

idle 的四件事与 C 的四件事逐项对上（§2.2 的四件事表）：指针安放、切 VM 表（`ncpus > 1` 门控，对应 C 的 `CONFIG_SMP`）、idle 标志与 AP 定时器分支、记账后停机。函数返回后回到 §4.1 Stage 1+2 的 pick-or-idle 内层循环，重挑再试——idle 是循环里的普通调用，不是独立调度状态。

### 4.4 恢复的三条腿（→§1.2；决策回指 §3.4 的步 7）

C 的 KTS 记录-清除-分派（§2.3）在 Rust 里由 `os/arch/src/arch/trap_style.rs:enum TrapStyle` 承接：风格是枚举而不是裸整数，每个风格映射到一个恢复序列：

```rust
// os/arch/src/arch/trap_style.rs:fn return_sequence —— 逐字引用（映射函数与序列枚举）
    /// The register-restore sequence this style requires at the
    /// return-to-user dispatch.
    ///
    /// `None` = no entry recorded: dispatching to user would restore an
    /// arbitrary register file, which C answers with
    /// `panic("no entry trap style known")` (arch_system.c:597-598).
    pub fn return_sequence(self) -> Option<ReturnSequence> {
        match self {
            Self::IntHard | Self::IntOrig | Self::FullContext => Some(ReturnSequence::FullContext),
            Self::Syscall => Some(ReturnSequence::FastSyscall),
            Self::NoEntry => None,
        }
    }
}

/// The register-restore sequence a recorded style requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnSequence {
    /// The mode-switch frame carries all user state (iretq/eret/sret
    /// class). C: `restore_user_context_int` — arch_system.c:600-603.
    FullContext,
    /// The entry saved a skinny frame; the return must pair it
    /// (sysret-class). C: `restore_user_context_syscall` —
    /// arch_system.c:592-595. Lands with the asm entry work (S-8).
    FastSyscall,
}
```

（内含仓库内部编号（S-8），按原文引用。C 的五类值收成三个分支：INT 族三种风格共用 `FullContext`——对应 C 的 switch 落穿；`Syscall` 对应瘦帧腿；`NoEntry` 对应 `KTS_NONE` 的 panic。三个原始值的划分与判定测试见第 5 章。）

x86_64 的恢复实现是 `FullContext` 序列的样本——iretq 载荷在内核栈上重建，寄存器从统一上下文成批装回：

```rust
// os/arch/src/x86_64/trap_return.rs:fn restore_to_user —— 节选（函数头与 iretq 载荷装配的开头）
    unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> ! {
        // Compile-time correspondence check between the hard-coded gp_regs
        // strides below and the indexed constants (a mismatch fails the
        // build here rather than corrupting registers at runtime).
        const _: () = assert!(GP_RAX == 0 && GP_R15 == 13);

        // SAFETY (full contract in TrapReturnArch::restore_to_user):
        // - `frame`/`regs` come from the scheduler's dispatch path and
        //   describe the picked process's saved user state. They are bound
        //   to RDI/RSI explicitly: the sequence clobbers every GP register
        //   it loads, and `options(noreturn)` makes the clobber set
        //   irrelevant (no code follows the mode switch).
        // - The current CPU's kernel stack is active; the pushed iret
        //   payload is abandoned on the mode switch (RSP0 reload, contract 3).
        // - The BKL has been released by the caller before this call.
        // - Register sequencing: every scratch register used below (RAX,
        //   RCX, RDX, R10, R11) is reloaded from `regs.gp_regs` afterwards,
        //   so no user value is clobbered; RBX and RSI are loaded after
        //   their last use as a source (RSI is the load base — see the
        //   step-4 comment) and never used as scratch afterwards.
        unsafe {
        core::arch::asm!(
            // ── 1. Build the iretq payload on the kernel stack ──
            // Frame layout (repr(C)): vector(0) errcode(8) rip(16) cs(24)
            // rflags(32) rsp(40) ss(48). Skip vector+errcode: iretq does
            // not consume them.
            "mov rax, [rdi + {rflags_off}]",
            "or  rax, {if_mask}",                 // interrupts-enabled guarantee
```

（节选到 iretq 载荷装配的头两条指令；随后的成批装回与 `iretq` 本体按原文继续。SAFETY 注释里的"BKL has been released by the caller"与 §3.4 步 2 的放锁点互为印证。aarch64 的同位实现以 `eret` 收尾、riscv64 以 `sret` 收尾（`os/arch/src/arm64/trap_return.rs:fn restore_to_user`、`os/arch/src/riscv64/trap_return.rs:fn restore_to_user`）——`ReturnSequence::FullContext` 文档注释里"iretq/eret/sret class"的三个成员。三架构的进入风格记录位随陷入入口工作落地，落地前每个进程都携带 `NoEntry`，恢复门的 `None` 臂把无记录的分派挡成停机。）

与 C 的对照收成一张表（§1.2 的进出配对表在 Rust 侧的落位）：

| C 的形状 | Rust 的形状 | 配对语义 |
|---|---|---|
| `p_kern_trap_style` 裸整数 + 7 个 `KTS_*` 常量 | `TrapStyle` 枚举（5 个变体；`KTS_INT_UM`/`KTS_SYSENTER` 不建模——64 位无 usermapped 入口） | 记录位即 ABI：原始值进 sigcontext 往返，编号与 C 一致 |
| 入口宏写帧、`restore_user_context` 读帧 | 陷入入口（待落地）写、`finish_and_restore` 步 7 读 | 读后即清保留：步 7 消费后记录回 `NoEntry`，同一次进入不会二次恢复 |
| `KTS_NONE` → `panic("no entry trap style known")` | `None` → 同文 panic | 无记录到出口 = 内核不变量破坏，宁可停机 |
| 三条汇编腿（sysenter/syscall/int） | 两个 `ReturnSequence` + 三架构恢复实现 | 快速腿的配对恢复随陷入入口工作落地，落地前 `FastSyscall` 臂响亮停机 |

反向推演两头（§2.3 的推演在此的 Rust 对应）：记录不清——步 7 若不把 `trap_style` 写回 `NoEntry`，下一次分派会拿上一次进入的帧布局卸载这一次的现场，过期帧复用；`None` 若静默返回——没有现场的"恢复"放跑，破坏比崩溃难查。

## 5. 测试要点

### 5.1 清单与读数（→第 4 章）

循环相关的宿主测试（`cfg(test)` + mock 特性）按对象分八组，共 21 条。计数口径：`os/kernel/src/lib.rs` 测试模块内、函数名匹配 `requeue_`/`pick_and_bill`/`switch_address_space`/`idle_marks`/`finish_and_restore`/`needs_tlb`/`consume_flush_tlb`/`switch_to_user`/`consume_kbill`/`kbill_ipc` 前缀的测试；复算命令 `grep -cE 'fn test_(requeue_|pick_and_bill|switch_address_space|idle_marks|finish_and_restore|needs_tlb|consume_flush_tlb|switch_to_user|consume_kbill|kbill_ipc)' os/kernel/src/lib.rs`，实测 21（lib.rs 测试模块共 56 条，`grep -c 'fn test_' os/kernel/src/lib.rs` 复算）：

| 组 | 条数 | 钉住的形状 | 对应节 |
|---|---|---|---|
| `test_requeue_*`（前缀口径） | 4 | 余时头插、无余时尾插、不可运行不入队、无 PREEMPTED 位空操作 | §4.3 |
| `test_pick_and_bill_*` | 2 | 可计账进程记 `bill_ptr`、空队列返回 None | §4.3 |
| `test_switch_address_space_*` | 3 | 内核任务不切表、装根并记 ptproc、同根同步镜像 | §4.1 |
| `test_idle_marks_cpu_idle_and_bills_idle_proc` | 1 | idle 标志与账本归属 | §4.3 |
| `test_finish_and_restore_*` | 3 | 恢复可达、无记录拒派、快速腿响亮占位 | §4.2/§4.4 |
| `test_needs_tlb_refresh_*` + `test_consume_flush_tlb_flag_*` | 4 | 刷新判定两半缺一不可、消费三态 | §4.1/§4.2 |
| `test_switch_to_user_full_loop_*` | 1 | 全循环端到端（§5.2 细读） | §4.1 |
| `test_consume_kbill_*` + `test_kbill_ipc_and_kcall_*` | 3 | kbill 两笔账的消费与独立性 | §3.4 |

kbill 标记的置位端（分派后、finish 前置标记）已在 [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §5.1 的测试表对过账；上表最后一组是消费端——`consume_kbill_ipc`/`consume_kbill_kcall` 用与 `context_stop` 等价的同一 delta 记账（§3.4 步 2），两条 attribute 测试各钉一笔，第三条钉两笔账互不串位。

观测点有二。其一，`os/kernel/src/smp.rs` 的 `sched_loop_entered` 字段：循环头置位（§3.1 的引块），SMP 测试据此断言 BSP 确实进过循环。其二，`boot_stage!("entering scheduler\n")` 串口路标（§3.1 引块）：真机门 `os/qemu-tests/test-cmd-smoke.sh` 在串口日志里检索这个字样作为通过判据——内核进了调度循环、CPU 交给了用户态进程。三架构的真机门是三份 boot 测试（`os/qemu-tests/test-rt-birth.sh` 与 `-aarch64`/`-riscv64` 变体）：跑完生产启动阶段后以与生产 `kmain` 尾段相同的方式调 `switch_to_user()`（测试内核 `os/qemu-tests/test-kernels/kernel/bootstrap/test-rt-birth/src/main.rs` 的收尾两行打印同款路标再进循环）。

### 5.2 关键测试细读（→§4.1/§4.4）

第一条：全循环端到端。生产主循环在宿主测试里真实可达——这条测试驱动的就是 §4.1 走查的那个 `switch_to_user` 本体，不是替身；现行 `os/kernel/src/lib.rs` 里不存在占位自旋形态的调度循环（反证：`grep -n 'spin_loop' os/kernel/src/lib.rs` 仅命中 mock `kmain` 的发散替身与一条测试内的等待循环，都不是循环本体）。测试的"成功信号"是一条 panic——恢复分派到 mock 实现时按契约停机：

```rust
// os/kernel/src/lib.rs —— 节选（test_switch_to_user_full_loop_dispatches_first_runnable_process；
// 省略函数上方的 #[test] 与 #[should_panic(expected = "MockTrapReturn::restore_to_user")] 两行属性，其余逐行）
    fn test_switch_to_user_full_loop_dispatches_first_runnable_process() {
        // End-to-end: seed proc_ptr = IDLE → stage 1 sees a non-runnable
        // current → requeue (no-op) → pick finds process 0 → address-
        // space install → misc flags (none) → quantum (plenty) → restore
        // diverges. The panic is the success signal; reaching any other
        // outcome (spin, wrong process) would hang or mis-panic instead.
        bkl_acquire_for_test();
        reset_root_mirrors_for_test();

        // Global state the real loop reads: seed the process table with
        // one runnable BILLABLE process.
        // SAFETY: single-threaded test (RUST_TEST_THREADS=1); the global
        // statics are initialized lazily exactly as the boot path does.
        unsafe {
            *SMP_STATE.get() = Some(SmpState::new_single_cpu());
        }
        {
            let table = unsafe { crate::proc_table_boot_unchecked() };
            let priv_table = unsafe { crate::priv_table_boot_unchecked() };
            // Global-state hygiene (B6): the shared scheduler run queue is
            // not part of the root-mirror reset. A prior test that drives a
            // real scheduler-aware primitive against the shared PROC_TABLE
            // (e.g. `clock_irq_handler` waking an expired-alarm owner through
            // the notify enqueue half, NK4-C B6) legitimately leaves a process
            // queued. Drain every run queue so this loop's pick sees only the
            // single runnable+queued process seeded below (the test's stated
            // precondition), immune to upstream ordering.
            table.drain_run_queues_for_test();
            make_runnable_billable(table, priv_table, ProcNr(0), crate::proc::priority::USER_Q, 5000);
            table.get_mut(ProcNr(0)).unwrap().p_seg.phys_root = minix_types::PhysBytes(0x6000);
            // Simulate a prior kernel entry (see test_finish_and_restore_
            // reaches_mock_restore): the return gate refuses NoEntry.
            table.get_mut(ProcNr(0)).unwrap().trap_style = TrapStyle::IntHard;
            // make_runnable_billable only clears SLOT_FREE; the run queue
            // must be populated explicitly (C's boot path reaches the same
            // state via RTS_UNSET(PROC_STOP) auto-enqueue — proc.h:216-224).
            table.sched_enqueue_with(ProcNr(0), None, CpuId::BSP);
        }

        super::switch_to_user();
    }
```

（内含仓库内部编号（B6、NK4-C），按原文引用。注释把测试要经过的七站列在函数头：种子 → Stage 1 判定 → 重排空操作 → 挑中 0 号 → 装地址空间 → 五标志（无）→ 量子（充足）→ 恢复发散。变红条件：若种子失效、挑人选错、恢复门误拒，测试或挂死或 panic 消息不是 mock 恢复的那条——两种都不是预期输出。）

第二条：恢复门拒派。没有进入记录的进程走到出口，必须停机而不是恢复（§4.4 的 `None` 臂）：

```rust
// os/kernel/src/lib.rs —— 逐字引用（test_finish_and_restore_refuses_dispatch_without_entry_style，全测试）
    #[should_panic(expected = "no entry trap style known")]
    fn test_finish_and_restore_refuses_dispatch_without_entry_style() {
        // C: arch_system.c:597-598 — restore_user_context panics on a
        // process with no recorded entry style: dispatching would restore
        // an arbitrary register file. A fresh slot has never entered the
        // kernel, so trap_style stays NoEntry and the gate must fire before
        // the mock restore (whose panic is the success signal of
        // test_finish_and_restore_reaches_mock_restore).
        bkl_acquire_for_test();
        reset_root_mirrors_for_test();
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut smp = SmpState::new_single_cpu();
        make_runnable_billable(&mut table, &mut priv_table, ProcNr(0), crate::proc::priority::USER_Q, 5000);
        table.get_mut(ProcNr(0)).unwrap().p_seg.phys_root = minix_types::PhysBytes(0x5000);
        // trap_style intentionally left NoEntry.

        let section = unsafe { crate::smp::BklSection::assume_held() };
        super::finish_and_restore(&mut table, &mut smp, ProcNr(0), false, &section);
    }
```

（逐字引用。变红条件：若恢复门漏掉 `NoEntry` 判定，panic 消息不会是预期的 "no entry trap style known"，而是 mock 恢复实现的停机消息——两条测试的预期 panic 互不相同，这正是它们能互相作证的原因：一条证明门放行后恢复可达，一条证明门不放行时先停。）

### 5.3 测试边界与诚实披露（→第 3 章/第 4 章）

宿主测试测不到的面，逐项给反证：

1. **真实恢复指令的语义**。iretq/eret/sret 的模式切换在宿主上不存在——mock 的恢复实现收到分派即停机（`os/arch/src/arch/trap_return.rs` 的 mock impl，函数体就是一条 `panic!("MockTrapReturn::restore_to_user: real dispatch attempted on host …")`）。所以"恢复后用户态从正确 EIP/PC 继续"只有真机门能证。
2. **idle 的停机窗口**。宿主不真停机；`test_idle_marks_cpu_idle_and_bills_idle_proc` 只钉标志位与账本归属，停机-唤醒的完整闭环（`idle_halt` 放中断、唤醒中断重取锁）由真机门覆盖。
3. **Stage 3a 的真机挂起重派全链**。Pending→Completed 的跨服务器时序需要 VM 真实服务请求；宿主 mock 里挂起态由测试手工摆。§3.3 四防护中的守卫与 continue 在宿主可测（状态枚举可手工构造），死锁取证类的问题（注释里那次真机实证）只在真机复现。

真机与宿主的分界线由此清楚：**状态的形状与门的判定归宿主，指令的语义与时序归真机**。三架构 boot 门（§5.1）是两类测试之间的桥——它在真机上把循环跑到第一次恢复，宿主测试则把循环每一拍的判定逻辑钉死。

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状 | 怀疑对象 | 确认命令与预期关键行 |
|---|---|---|
| 串口出现 "entering scheduler" 后无进展 | Stage 1+2 的挑人/idle 环节（就绪队列空且唤醒失败）或量子异常 | 跑 `bash os/qemu-tests/test-cmd-smoke.sh` 抓串口日志；预期关键行 `entering scheduler`。有路标无后续：查 `os/kernel/src/lib.rs:fn idle` 的停机-唤醒路径与队列非空性。宿主侧注意运行口径：`cd os && cargo test -p minix-kernel --features mock`（全套件）预期 `831 passed; 0 failed`——循环相关测试依赖套件内先行测试初始化的共享全局态（`os/.cargo/config.toml` 自述 SMP_STATE 等由套件共享），**单过滤运行会因全局态未初始化而失败**，这不是被诊断代码的错。全套件通过后，循环全链已通（全循环测试的恢复分派在套件内走到 mock 恢复的停机点，panic 消息含 `MockTrapReturn::restore_to_user`） |
| 进程装载完成但不被调度，或跑过一次不再被调度 | 五标志臂把进程弄停（DELIVERMSG 坏指针发 SIGSEGV、SC_DEFER 阻塞在 SEND）；Stage 3a 守卫 `continue` 后 VM 侧未服务完 | 宿主：全套件内 `test_finish_and_restore` 三条与 `test_switch_to_user_full_loop_*` 一条通过（运行口径同上——单独过滤会因共享态未初始化而红，不代表恢复门有问题）；真机串口查进程是否带挂起态（`p_vm_suspend` 非 None 即在等 VM——判别读数：串口路标停在 `entering scheduler` 之后且无进程名打印，对照 §4.1 Stage 3a 的守卫注释） |
| 浮点指令行为异常（该陷阱的不陷阱，或不该陷阱的陷阱） | riscv64 的 FS 位载体与归属策略 | 全套件内 `test_riscv_lazy_fs_field_gating` 通过（四条断言：非 owner 的 FS 字段为 `0b00`（Off，首条 FP 陷阱）、owner 为 `0b11`（Dirty，不陷阱）、非 owner 的 FS 会被判成 FP 门控陷阱（轮转腿可达）、owner 不陷阱——§3.4 步 3 的修正） |
| 切进程后取指错位 / 反复同一页故障 | `tlb_must_refresh` 预计算漏算或次序颠倒；故障页失效点不对 | 全套件内 `test_needs_tlb_refresh_*` 与 `test_consume_flush_tlb_flag_*` 四条通过（"旗标+ptproc"两半判定与消费三态无恙，§4.1 的次序约束：预计算在 `switch_address_space` 之前）；真机反复同地址故障对照 §4.2 步 7 的 `fault_flush_va` 消费点 |

前两行的判别读数靠两个观测点（§5.1）：串口路标区分"进没进循环"，`sched_loop_entered` 标志区分"哪颗 CPU 进过"。第 3、4 行的确认全部可以在宿主完成——状态形状可构造，指令时序不可（§5.3 的分界）。

### 6.2 手算与常量核对命令

五标志的位值与定义行（对照 §2.1 的臂表）：

```bash
grep -n 'MF_KCALL_RESUME\|MF_DELIVERMSG\|MF_SC_DEFER\|MF_SC_TRACE\|MF_SC_ACTIVE' minix3/minix/kernel/proc.h
# 预期关键行：:237 MF_KCALL_RESUME 0x008 / :243 MF_DELIVERMSG 0x040 /
# :245 MF_SC_ACTIVE 0x100 / :246 MF_SC_DEFER 0x200 / :247 MF_SC_TRACE 0x400
grep -c '^#define MF_' minix3/minix/kernel/proc.h
# 预期：18（杂项标志全表条数，§2.1 口径）
```

三条恢复腿的 ENTRY 定位（对照 §1.2/§2.3）：

```bash
grep -n 'ENTRY(restore_user_context' minix3/minix/kernel/arch/i386/mpx.S
# 预期：391（sysenter）/ 414（syscall）/ 434（int）
```

Rust 五阶段锚点定位（对照 §4.1 的走查次序）：

```bash
grep -n '── Stage [0-9]' os/kernel/src/lib.rs
# 预期关键行：4011（Stage 1+2）/ 4058（Stage 3a）/ 4183（Stage 3）/ 4192（Stage 4）/ 4201（Stage 5）
# （更宽的 pattern 'Stage [0-9]' 还会命中测试与注释里的 3 处，不作为锚点用）
```

C 侧三个区间读数（对照 §2.0/§2.2/§2.3）：

```bash
grep -n 'void switch_to_user' minix3/minix/kernel/proc.c                 # 预期 :299
sed -n '299,474p' minix3/minix/kernel/proc.c | wc -l                     # 预期 176（全函数行数）
grep -n 'static struct proc \* pick_proc' minix3/minix/kernel/proc.c     # 预期两行：:60（前向声明）/:1785（定义）
sed -n '566,612p' minix3/minix/kernel/arch/i386/arch_system.c | wc -l    # 预期 47（restore_user_context 行数）
```

## 7. 参见

- 前后篇：[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §4.6（终拍与 BKL 交付——本篇开头的回指对象）、[09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3（MEMREQ GET/REPLY 臂——REPLY 置 MF_KCALL_RESUME，与本篇 §3.3 互为两端）、[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4（RTS 位族——PREEMPTED/PROC_STOP 的语义基座）、[07-cross-space-init.md](07-cross-space-init.md) §1.4（ptproc 的切表侧判据——`switch_address_space` 的根切换语义）。
- 相邻方向（均尚未重生成）：`11-scheduling-primitives`（调度原语篇——就绪队列与优先级算法本体）、`12-ipc-core`（IPC 核心篇——DELIVERMSG 臂的 callee 语义）、`13-syscall-dispatch`（系统调用分派篇——`kernel_call_resume` 本体与 SC_DEFER 腿）、`14-exception-interrupt`（异常与中断入口篇——trap 入口与风格记录的写入半）、`16-smp`（SMP 篇——AP 收尾与 IPI）、`17-clock-timer`（时钟与定时器运行篇——tick 递减与记账本体）。
- C 源码：`minix3/minix/kernel/proc.c:switch_to_user`、`minix3/minix/kernel/proc.c:idle`、`minix3/minix/kernel/proc.c:switch_address_space_idle`、`minix3/minix/kernel/proc.c:pick_proc`、`minix3/minix/kernel/proc.c:delivermsg`、`minix3/minix/kernel/proc.c:copr_not_available_handler`、`minix3/minix/kernel/system.c:kernel_call_resume`、`minix3/minix/kernel/arch/i386/arch_system.c:restore_user_context`、`minix3/minix/kernel/arch/i386/arch_clock.c:context_stop`、`minix3/minix/kernel/arch/i386/mpx.S`（三条恢复腿）、`minix3/minix/kernel/proc.h`（MF_* 全表）。
- Rust：`os/kernel/src/lib.rs:fn switch_to_user`、`os/kernel/src/lib.rs:fn scheduler_loop`、`os/kernel/src/lib.rs:fn finish_and_restore`、`os/kernel/src/lib.rs:fn idle`、`os/kernel/src/lib.rs:fn pick_and_bill`、`os/kernel/src/lib.rs:fn requeue_if_preempted`、`os/kernel/src/lib.rs:fn switch_address_space`、`os/kernel/src/proc_table.rs:fn process_misc_flags`、`os/kernel/src/proc_table.rs:fn check_quantum`、`os/kernel/src/sched.rs:fn pick_proc`、`os/kernel/src/vm.rs:fn kernel_call_resume`、`os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner`、`os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl`、`os/arch/src/arch/trap_style.rs:enum TrapStyle`、`os/arch/src/x86_64/trap_return.rs:fn restore_to_user`。




