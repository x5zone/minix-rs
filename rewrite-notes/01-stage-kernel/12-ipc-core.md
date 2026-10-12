# 12 · IPC 核心：无共享内存的消息交换

> **C 真值源**：`minix3/minix/kernel/proc.c`（`do_ipc`/`do_sync_ipc`/`deadlock`/`mini_send`/`mini_receive`/`mini_notify`/`try_deliver_senda`/`try_async`/`try_one`/`delivermsg`/`BuildNotifyMessage`/`has_pending` 族）、`minix3/minix/kernel/proc.h`（`P_BLOCKEDON`、RTS 位、`p_defer`、记账字段）、`minix3/minix/kernel/ipc.h`（`WILLRECEIVE`/`CANRECEIVE`/`NON_BLOCKING`/`FROM_KERNEL`/IPC 状态宏）、`minix3/minix/include/minix/ipcconst.h`（trap 号）、`minix3/minix/include/minix/com.h`（`NOTIFY_MESSAGE`、`NR_TASKS`）、`minix3/minix/include/minix/endpoint.h`（`ANY`/`NONE`/`SELF`）、`minix3/minix/kernel/main.c`（`IPCNAME` 表）、`minix3/minix/kernel/arch/i386/arch_system.c:arch_set_secondary_ipc_return`。
> **Rust 实现**：`os/kernel/src/ipc.rs`（`IpcCall`/`IpcOutcome`/`IpcError`/`SendFlags`/`DeadlockCycle`/`UserCopy`/`KernelUserCopy`/`delivermsg`/`caller_q_*`/`IpcEngine`/`mini_notify_core`/`build_notify_message`）、`os/kernel/src/syscall.rs:fn dispatch_ipc`（KERNINFO 臂与 SENDA 参数拆装）、`os/kernel/src/trap_dispatch.rs`（int-33 门的 bodyless 判定）、`os/kernel/src/kerninfo.rs`（kerninfo 页的建立与发布）、`os/kernel/src/proc.rs`（`ipc_status_add_call`/`ipc_status_add_flags`/`set_ipc_return_code`）、`os/kernel/src/globals.rs`（`kbill_ipc` 对位）、`os/arch/src/x86_64/boot.rs:fn set_secondary_ipc_return`（二次返回通道的 x86-64 载体）。
> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。引用位置由符号名定位（`path:func`、`path:fn NAME`、`path:enum NAME`、`path:struct NAME`），不使用行号；逐字引用块的块级区间题注带行区间与复算命令。
> **本篇主题**：微内核的基石——两个没有共享内存的进程如何可靠地交换一条消息。六个原语各自的停与不停、接收侧的三级匹配序、通知的永不丢失与永不阻塞、环等待死锁的机器判定、延迟投递的两段式协议；以及 Rust 的 `IpcEngine`、侵入式 FIFO 与结构化死锁返回如何承接这套协议。
> **前置**：[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.8（endpoint、generation 与槽位常量体系——通信身份的前提）、[09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3（MEMREQ GET/REPLY 是 VM 拉取的回信形状；FROM_KERNEL 的推送形状在缺页臂，`minix3/minix/kernel/arch/earm/exception.c`）、[10-switch-to-user.md](10-switch-to-user.md) §2.1（五标志循环的 DELIVERMSG 臂=本篇 delivermsg 的消费点）、[11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2 与 §4.2（RTS_SENDING/RECEIVING 的队列半——本篇讲位为什么变，那边讲位变化的队列后果）。
> **后续**：`13-syscall-dispatch`（trap 入口传参，尚未重生成）、`22-syscall-copy`（拷贝机制本体，尚未重生成）、`23-ipc-filter`（IPC 过滤决策，尚未重生成）、`24-cross-space-runtime`（VMSUSPEND 与跨空间拷贝运行时，尚未重生成）。

[10-switch-to-user.md](10-switch-to-user.md) 的循环每拍到 DELIVERMSG 臂，都在向本篇的一个函数要答案：那条挂在进程上的待交付消息是谁放的、放进去的时候双方各处于什么状态。本篇回答这些问题：两个地址空间毫无交集的进程，一条消息从发送者的缓冲走到接收者的缓冲要经过哪几只手；哪些情况发送者会停、接收者会停、两者都不停；以及内核怎样在一圈互相等待的进程里认出真正的死结。

本篇走完这组协议：C 的原语与机制函数怎么写（第 2 章）、Rust 用什么形状承接（第 3 章）、逐函数怎么落地（第 4 章）、靠什么测试钉住（第 5 章）、卡住时怎么查（第 6 章）。

---

## 1. 概述

### 1.1 无共享内存的消息交换：六原语与两条规则

两个进程之间没有一块共享内存。它们唯一共同拥有的东西，是内核进程表里各自的槽位：发送消息时，内核把消息字节从发送者的地址空间读出来，写进内核中转位置，再在某个时机写向接收者的缓冲——数据两次过内核的手，地址空间从头到尾没有被对方看见。这套拷贝语义是隔离的代价，也是隔离的目的：一个进程能对另一个进程做的事，被压缩成「往内核槽里放一段定长字节」这一种。

在这个前提下，内核向用户态开放的通信原语只有两个方向——发与收。C 源码在分派函数的注释里把这层关系写成一段枚举：

```c
  /* Now check if the call is known and try to perform the request. The only
   * system calls that exist in MINIX are sending and receiving messages.
   *   - SENDREC: combines SEND and RECEIVE in a single system call
   *   - SEND:    sender blocks until its message has been delivered
   *   - RECEIVE: receiver blocks until an acceptable message has arrived
   *   - NOTIFY:  asynchronous call; deliver notification or mark pending
   *   - SENDA:   list of asynchronous send requests
   */
```

（区间复算：`sed -n '646,653p' minix3/minix/kernel/proc.c | wc -l` = 8。锚点：`minix3/minix/kernel/proc.c:do_ipc`。）

六个原语对这两条规则各取所需：SEND、RECEIVE、SENDREC 直走规则（SENDREC 在一次陷入里先后走两条），SENDNB 把规则一换成一次探测，NOTIFY 与 SENDA 各有一条旁路。把关系画成一张机制图，就是全篇的行进地图：

```
              两条阻塞规则（等待语义的全部来源）
        ┌────────────────────────────────────────────────┐
        │ 规则一（发送侧）：目标没在等我 → 我停            │
        │ 规则二（接收侧）：没有可收的消息 → 我停          │
        └────────────────────────────────────────────────┘
             ▲ 直走规则的原语                ▲ 绕开或组合规则的原语
  SEND     ── 规则一：目标不在就停进等待队列
  RECEIVE  ── 规则二：没消息就停成可收状态
  SENDREC  ── 一次陷入里先走规则一再走规则二（发一半+收一半）
  SENDNB   ── 规则一的前半问一句：不停就带着「目标不在」返回
  NOTIFY   ── 旁路一：永不停；目标没在等 → 通知记入接收者的位图，等它下次来收
  SENDA    ── 旁路二：永不停；整批消息 → 投得出去的立即投，投不出去的记账留待接收者来取
```

 SENDREC 不是第三种语义——它是 SEND 与 RECEIVE 两个原语的组合封装：C 的分派把 SENDREC 交给 `do_sync_ipc`，其 switch 里 SENDREC 臂先走 `mini_send`、随后 `/* fall through */` 落进 RECEIVE 臂走 `mini_receive`（§2.3 引文的函数 `do_sync_ipc` 尾部 switch，proc.c:568-583）；进程常说的「等回信（REPLY）」，在内核里就是这次组合后半段的 RECEIVE，REPLY 是 RECEIVE 的特例说法而非独立原语。会停的原语全部停回那两条规则：SEND 停成可发状态、RECEIVE 停成可收状态、SENDREC 先后经历两者；SENDNB 与两个旁路不同，它走的是规则一的位置但只问不停——问出「目标不在」就带着错误码返回。

反方向的推演，坏在一个能指认的对象上。设想消息改成引用传递——内核只传一个指向发送者缓冲的指针，接收者直接读那份缓冲。发送者发送完成后继续运行、退出、缓冲被复用或页被回收，接收者读到的是改写后的字节；更坏的情形是发送者进程先于接收者终止，指针落进一片已被回收的地址空间。微内核的服务器隔离性建立在「地址空间即隔离边界」上：客户与服务器的全部契约只有消息字节，一旦引用可以跨过这条边界，任何一个进程的内存布局失误都变成另一个进程的读越界。拷贝语义把这条边界守住——代价是每次消息两次搬运，收益是两边互不可见。

### 1.2 通信的身份前提：端点即地址（回指 06）

发消息要写「发给谁」，收消息要写「从谁收」。这个地址不是进程号，而是 endpoint：低 15 位是槽位号，高位是 generation 计数（编码细节归 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.8）。槽位会被复用——一个服务器崩溃重启后住进同一个槽——generation 让旧端点在新进程入住后自动失效：旧客户手里攥着的旧端点指向的 generation 对不上，内核在端点解析时直接拒绝。防伪因此成为机制而不是纪律：没有任何一处代码需要「记得」这个槽换过人。

三个特殊端点值与普通端点共用一个类型，定义在端点头文件的尾部：

```c
/* The special endpoint numbers, and the resulting maximum slot number. */
#define ANY		((endpoint_t) (_ENDPOINT_SLOT_TOP - 1))
#define NONE		((endpoint_t) (_ENDPOINT_SLOT_TOP - 2))
#define SELF		((endpoint_t) (_ENDPOINT_SLOT_TOP - 3))
```

（区间复算：`sed -n '53,56p' minix3/minix/include/minix/endpoint.h | wc -l` = 4。锚点：`minix3/minix/include/minix/endpoint.h:ANY`、`:NONE`、`:SELF`。）

| 特殊值 | 取值来历 | 语义 |
|---|---|---|
| `ANY` | 槽位区顶部减 1 | 只在 receive 侧合法的通配源：愿意收任何来源 |
| `NONE` | 槽位区顶部减 2 | 无效端点：阻塞判定的「链到此为止」哨兵 |
| `SELF` | 槽位区顶部减 3 | 自指：SENDA 表项里的「发给我自己」，内核交付时替换成调用者自己的端点 |

三者在协议里各守一段：`ANY` 通配接收（发送侧填它会被拒绝，§2.1 的权限段可见对应检查）；`NONE` 是死锁链走查的终止哨兵（§2.2）；`SELF` 只在 SENDA 表里出现（§2.6）。把 generation 防伪反过来推演：设想裸进程号编址——一个被 kill 后重启的服务器，重启后收到旧客户发给「前任」的请求，槽位号完全一致，内核无从分辨这是新进程还是旧进程的消息，语义错乱无声地发生：新服务器把陈年请求当真处理，而那个客户早已不在。generation 把「同一个槽的不同一生」编码进地址本身，错投在解析时就被拦下。

收束一遍本章的框架：消息交换的全部等待只有两条规则——发送侧「目标没在等我」、接收侧「没有可收的消息」；六个原语要么直走这两条规则（SEND/RECEIVE/SENDREC/SENDNB），要么绕开它们（NOTIFY 的位图旁路、SENDA 的整批旁路）；而每一次「发给谁」「从谁收」都凭带 generation 的端点编址，`ANY`/`NONE`/`SELF` 三个特殊值分别承担通配、终止与自指。第 2 章按这份地图逐个函数看 C 怎么落实。

### 1.3 本篇范围与可以安全跳过的内容

| 主题 | 本篇怎么办 | 去处 |
|---|---|---|
| 一次 IPC 从 trap 指令到 `do_ipc` 之间的寄存器搬运 | 只从 `do_ipc` 的三个参数讲起 | `13-syscall-dispatch`（系统调用分派篇，尚未重生成） |
| 消息字节的物理拷贝路径（页表走查、跨地址空间写） | 只讲「消息在两进程槽之间转手」的协议层 | `22-syscall-copy`/`24-cross-space-runtime`（两篇，均尚未重生成；本篇 §3.4 只引用 Rust 侧的边界抽象） |
| IPC 过滤的决策面（谁能发给谁的链式规则） | 只讲过滤判定挂在匹配路径的哪个位置 | `23-ipc-filter`（过滤决策篇，尚未重生成） |
| 坏缓冲导致的 VM 挂起与恢复本体 | 只讲协议层的两次失败策略（§2.7） | `24-cross-space-runtime`（跨空间运行时篇，尚未重生成） |
| IPC 状态控制（statectl） | 不讲 | `21` 方向（statectl 篇，尚未重生成） |
| kerninfo 页的内容与布局 | 只讲 `do_ipc` 的查询臂怎么交付页地址 | [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §4.3 |
| RTS 位变化后的就绪队列后果 | 只讲位为什么变 | [11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2、§4.2 |

认知减负声明：本章不必深究 IPC 过滤决策（`23-ipc-filter` 方向）、消息字节的物理拷贝路径（`22-syscall-copy`/`24-cross-space-runtime` 方向）、trap 入口传参（`13-syscall-dispatch` 方向）——本篇保证「位变 → 队列动 → 消息转手」这条协议层线索读完整。

---

## 2. C 源码分析

### 2.0 本章地图：两个入口、六个原语、三类旁路

IPC 的全部 C 实现住在 `minix3/minix/kernel/proc.c` 里，围绕一个入口函数组织。`do_ipc` 是所有消息原语的唯一入口（trap 参数 r1/r2/r3 怎么送进来的归 `13-syscall-dispatch` 方向）；它把调用分派给一个同步通道（`do_sync_ipc`）与两条旁路（SENDA 表、KERNINFO 查询）；同步通道再落到三个机制函数 `mini_send`/`mini_receive`/`mini_notify`；SENDA 旁路由三个函数经办（`try_deliver_senda` 提交整批，`try_async`/`try_one` 在接收侧逐条兑现）；最后 `delivermsg` 负责把内核槽里的待交付消息拷给用户。画出函数关系图：

```
                     do_ipc(r1,r2,r3)  ── 唯一入口：记账、跟踪预占、残留检查
        ┌──────────────┬───────────────┴───────────────┐
        │ 同步五原语     │ SENDA（异步整批）              │ MINIX_KERNINFO（查询）
        ▼              ▼                               ▼
   do_sync_ipc    mini_senda ──► try_deliver_senda    页地址走二次返回
   （权限门+分派）        （提交：逐条 A_RETR/A_INSRT）      （不走消息体）
        │                        ▲
        │ SEND/SENDREC/SENDNB    │ 接收侧复用：mini_receive 的第二级
        │ RECEIVE       │        │ 也走 try_async / try_one 兑现遗留表项
        ▼               ▼        │
   mini_send  mini_receive ──────┘
        │        │  │
        │        │  └─ 第三级：扫 p_caller_q 同步链（CANRECEIVE 逐个验）
        │        └─ 第二级：has_pending_asend → try_async / try_one
        │             第一级：has_pending_notify（通知位图）
        ▼
   deadlock 预检（两个停车点各插一次：mini_send / mini_receive）
        │
        ▼
   mini_notify（通知：立即交付或记位图）
        │
        ▼
   delivermsg（MF_DELIVERMSG 置位后，循环拍把消息拷给用户）
```

这张图里藏着一个组织方式上的事实：SENDA 的接收侧复用。`mini_receive` 的第二级匹配（异步表）发现接收者有遗留的异步消息时，走的正是 `try_async`/`try_one` 这两个函数——与 SENDA 提交时逐条兑现的是同一套代码。一份表项从提交到兑现可能隔着很多次调度，但经办人只有这一组。

锚点：`minix3/minix/kernel/proc.c:do_ipc`、`:do_sync_ipc`、`:mini_send`、`:mini_receive`、`:mini_notify`、`:try_deliver_senda`、`:try_async`、`:try_one`、`:delivermsg`、`:deadlock`。

### 2.1 do_ipc：分派、跟踪预占与 KERNINFO 臂（本篇主走查一）

入口函数全貌如下（100 行，全函数逐字）：

```c
int do_ipc(reg_t r1, reg_t r2, reg_t r3)
{
  struct proc *const caller_ptr = get_cpulocal_var(proc_ptr);	/* get pointer to caller */
  int call_nr = (int) r1;

  assert(!RTS_ISSET(caller_ptr, RTS_SLOT_FREE));

  /* bill kernel time to this process. */
  kbill_ipc = caller_ptr;

  /* If this process is subject to system call tracing, handle that first. */
  if (caller_ptr->p_misc_flags & (MF_SC_TRACE | MF_SC_DEFER)) {
	/* Are we tracing this process, and is it the first sys_call entry? */
	if ((caller_ptr->p_misc_flags & (MF_SC_TRACE | MF_SC_DEFER)) ==
							MF_SC_TRACE) {
		/* We must notify the tracer before processing the actual
		 * system call. If we don't, the tracer could not obtain the
		 * input message. Postpone the entire system call.
		 */
		caller_ptr->p_misc_flags &= ~MF_SC_TRACE;
		assert(!(caller_ptr->p_misc_flags & MF_SC_DEFER));
		caller_ptr->p_misc_flags |= MF_SC_DEFER;
		caller_ptr->p_defer.r1 = r1;
		caller_ptr->p_defer.r2 = r2;
		caller_ptr->p_defer.r3 = r3;

		/* Signal the "enter system call" event. Block the process. */
		cause_sig(proc_nr(caller_ptr), SIGTRAP);

		/* Preserve the return register's value. */
		return caller_ptr->p_reg.retreg;
	}

	/* If the MF_SC_DEFER flag is set, the syscall is now being resumed. */
	caller_ptr->p_misc_flags &= ~MF_SC_DEFER;

	assert (!(caller_ptr->p_misc_flags & MF_SC_ACTIVE));

	/* Set a flag to allow reliable tracing of leaving the system call. */
	caller_ptr->p_misc_flags |= MF_SC_ACTIVE;
  }

  if(caller_ptr->p_misc_flags & MF_DELIVERMSG) {
	panic("sys_call: MF_DELIVERMSG on for %s / %d\n",
		caller_ptr->p_name, caller_ptr->p_endpoint);
  }

  /* Now check if the call is known and try to perform the request. The only
   * system calls that exist in MINIX are sending and receiving messages.
   *   - SENDREC: combines SEND and RECEIVE in a single system call
   *   - SEND:    sender blocks until its message has been delivered
   *   - RECEIVE: receiver blocks until an acceptable message has arrived
   *   - NOTIFY:  asynchronous call; deliver notification or mark pending
   *   - SENDA:   list of asynchronous send requests
   */
  switch(call_nr) {
  	case SENDREC:
  	case SEND:			
  	case RECEIVE:			
  	case NOTIFY:
  	case SENDNB:
  	{
  	    /* Process accounting for scheduling */
	    caller_ptr->p_accounting.ipc_sync++;

  	    return do_sync_ipc(caller_ptr, call_nr, (endpoint_t) r2,
			    (message *) r3);
  	}
  	case SENDA:
  	{
 	    /*
  	     * Get and check the size of the argument in bytes as it is a
  	     * table
  	     */
  	    size_t msg_size = (size_t) r2;
  
  	    /* Process accounting for scheduling */
	    caller_ptr->p_accounting.ipc_async++;
 
  	    /* Limit size to something reasonable. An arbitrary choice is 16
  	     * times the number of process table entries.
  	     */
  	    if (msg_size > 16*(NR_TASKS + NR_PROCS))
	        return EDOM;
  	    return mini_senda(caller_ptr, (asynmsg_t *) r3, msg_size);
  	}
  	case MINIX_KERNINFO:
	{
		/* It might not be initialized yet. */
	  	if(!minix_kerninfo_user) {
			return EBADCALL;
		}

  		arch_set_secondary_ipc_return(caller_ptr, minix_kerninfo_user);
  		return OK;
	}
  	default:
	return EBADCALL;		/* illegal system call */
  }
}
```

（区间复算：`sed -n '599,698p' minix3/minix/kernel/proc.c | wc -l` = 100。锚点：`minix3/minix/kernel/proc.c:do_ipc`。`p_defer` 三寄存器是进程结构体的内嵌字段（`minix3/minix/kernel/proc.h`，名 p_defer）；`minix_kerninfo_user` 是内核全局声明（`minix3/minix/kernel/glo.h`，名 minix_kerninfo_user）；`arch_set_secondary_ipc_return` 的 i386 实现在 `minix3/minix/kernel/arch/i386/arch_system.c:arch_set_secondary_ipc_return`。）

逐段读这个函数，四件事按序发生。

第一件，记账。`kbill_ipc = caller_ptr` 把「内核此刻在为谁做 IPC」记进全局账目字段——时钟中断的记账半据此把这段内核时间算到调用者头上（记账字段族见 `minix3/minix/kernel/proc.h:ipc_sync` 与 `:ipc_async`，分派时分别累计同步与异步两类的调用次数）。入口处的 `assert(!RTS_ISSET(caller_ptr, RTS_SLOT_FREE))` 声明一个前提：能发出 IPC 陷入的进程槽必然在用。

第二件，跟踪预占。被跟踪的进程第一次带着 IPC 陷入进来时，这次调用并不执行：

```
   首进（MF_SC_TRACE 置位）                     恢复后再进（MF_SC_DEFER 置位）
   ────────────────────────                     ────────────────────────────
   清 TRACE、置 DEFER                            清 DEFER
   三个 trap 参数存进 p_defer(r1,r2,r3)          置 SC_ACTIVE（为「离开调用」
   cause_sig(SIGTRAP)——进程停下，                 的跟踪留标记）
     跟踪者获得观察输入消息的机会                 按 p_defer 里存的三个参数
   原样返回调用者的返回寄存器                     真正执行本次 IPC
```

跟踪者的语义是「事件发生**前**停下看一眼」：SIGTRAP 之后进程停住，跟踪者可以读它的消息缓冲；等跟踪者放行，进程恢复运行、重新陷入进来，第二段才把挂在 `p_defer` 里的参数取出来执行。`p_defer` 的三个寄存器就是暂停点的现场——没有它们，第二次进来时调用参数已经不在手里。

反向推演这一段：设想不推迟、直接执行调用，SIGTRAP 后停下的时刻消息已经被处理完、接收方可能已经醒来——跟踪者拿到的只是一次已经生效的通信的残迹，「事件前停」的语义被破坏。推迟把「进 IPC」拆成两段执行，第一段只做登记与停下，第二段才动手。

第三件，残留检查。`MF_DELIVERMSG` 在位却来发起新 IPC 是协议违约（上一条消息还没拷给用户），直接 panic——这条检查的存在本身就是「交付分两段」协议的自证，两段式的本体在 §2.7。

第四件，分派。同步五原语（SENDREC/SEND/RECEIVE/NOTIFY/SENDNB）走 `do_sync_ipc`；SENDA 带着表尺寸走 `mini_senda`，尺寸超过 `16*(NR_TASKS + NR_PROCS)` 直接返回 EDOM；KERNINFO 是第三条路。`NR_TASKS` 取 5、进程槽容量取 256（`minix3/minix/include/minix/com.h:NR_TASKS`、`minix3/minix/include/minix/sys_config.h:_NR_PROCS`），上限即 16×261 = 4176 个表项。

KERNINFO 臂是六原语之外唯一也从 `do_ipc` 进来的调用，而它不是通信，是查询——把内核信息页的用户态地址交给调用者。交付方式刻意绕开了消息体：

```
   用户态                            do_ipc 的 KERNINFO 臂
   ────────                          ─────────────────────
   陷入，call_nr = MINIX_KERNINFO    minix_kerninfo_user 是否已发布？
     （不携带消息缓冲）                 │ 未发布 → 返回 EBADCALL
                                      │ 已发布 ↓
                                     arch_set_secondary_ipc_return(caller, 页地址)
                                      │ i386 实现：p_reg.bx = 页地址
                                      ▼
   恢复用户态时该寄存器随保存帧带回 ──► 用户从约定寄存器读到页地址
   消息缓冲自始至终未被读写
```

页地址走的是「二次返回通道」——一个与普通返回值并行的寄存器车道，i386 上由 `arch_set_secondary_ipc_return` 写进 `p_reg.bx`（`minix3/minix/kernel/arch/i386/arch_system.c:arch_set_secondary_ipc_return`，函数体一行赋值）。为什么不让页地址作为消息发回来？因为此刻调用者可能根本没有合法的消息缓冲——这个调用的用途之一是在用户态库初始化早期取页地址，而那时缓冲管理未必就绪；把交付压到一个寄存器上，调用对用户态的唯一要求就是「陷进来」。`minix_kerninfo_user` 未发布时返回 EBADCALL 的分支对应启动早期：页还没有用户态映射，查询来得太早。

同步五原语进入 `do_sync_ipc` 后还要过一串权限检查（名字表、ANY 通配、端点有效性、发送白名单、trap 掩码、内核任务限制——共六道）。其中决定性的尾门（trap 掩码与内核任务限制）如下：

```c
  /* Check if the process has privileges for the requested call. Calls to the 
   * kernel may only be SENDREC, because tasks always reply and may not block 
   * if the caller doesn't do receive(). 
   */
  if (!(priv(caller_ptr)->s_trap_mask & (1 << call_nr))) {
#if DEBUG_ENABLE_IPC_WARNINGS
      printf("sys_call: %s not allowed, caller %d, src_dst %d\n", 
          callname, proc_nr(caller_ptr), src_dst_p);
#endif
	return(ETRAPDENIED);		/* trap denied by mask or kernel */
  }

  if (call_nr != SENDREC && call_nr != RECEIVE && iskerneln(src_dst_p)) {
#if DEBUG_ENABLE_IPC_WARNINGS
      printf("sys_call: trap %s not allowed, caller %d, src_dst %d\n",
           callname, proc_nr(caller_ptr), src_dst_e);
#endif
	return(ETRAPDENIED);		/* trap denied by mask or kernel */
  }
```

（区间复算：`sed -n '548,566p' minix3/minix/kernel/proc.c | wc -l` = 19。本块为节选（548-566 行连续未删节），省略号处为 `do_sync_ipc` 的入口段——call_nr 名字表检查、ANY 通配检查（非 RECEIVE 填 ANY 即 EINVAL）、端点有效性检查与 `may_send_to` 白名单检查——以及尾部的五原语 switch 分派（SENDA 在入口被断言挡死，不走此通道）。锚点：`minix3/minix/kernel/proc.c:do_sync_ipc`。）

尾门第二条把一个限制写成一行：对内核任务的调用只允许 SENDREC 与 RECEIVE。理由写在注释里——任务总是回信，若允许对任务裸 SEND，调用者可能永远等不到对方「不忙」的时刻；限定成 SENDREC（发完必收）或 RECEIVE（只是等），调用者自己就是那个最终醒来的人。`do_sync_ipc` 的入口还有一道 `assert(call_nr != SENDA)`（`minix3/minix/kernel/proc.c:do_sync_ipc` 内首行断言）：SENDA 不走同步通道，它有自己的权限语义（§2.6）。

### 2.2 deadlock：环等待检测与 XOR 豁免（本篇主走查二）

死锁检测回答一个问题：一圈进程互相等，机器怎么在没有旁观者的内核里认出这个环？答案分两半——怎么沿着等待关系走（一个宏），走到头怎么下结论（一个函数）。

等待关系的载体是 `P_BLOCKEDON` 宏：

```c
/* Macro to return: on which process is a certain process blocked?
 * return endpoint number (can be ANY) or NONE. It's important to
 * check RTS_SENDING first, and then RTS_RECEIVING, as they could
 * both be on (if a ipc_sendrec() blocks on sending), and p_getfrom_e
 * could be nonsense even though RTS_RECEIVING is on.
 */
#define P_BLOCKEDON(p)							\
	(								\
		((p)->p_rts_flags & RTS_SENDING) ? 			\
		(p)->p_sendto_e : 					\
		(							\
			(						\
				((p)->p_rts_flags & RTS_RECEIVING) ?	\
				(p)->p_getfrom_e : 			\
				NONE					\
			) 						\
		)							\
	)
```

（区间复算：`sed -n '181,198p' minix3/minix/kernel/proc.h | wc -l` = 18。锚点：`minix3/minix/kernel/proc.h:P_BLOCKEDON`。）

宏的注释自己强调了次序：先查 SENDING 再查 RECEIVING。这不是品味问题——SENDREC 的进程可能两位同置（发送半停着、收一半已登记），此时 `p_getfrom_e` 里的值未必有意义，先查 SENDING 才能把「它阻塞在给谁发」如实答出。宏返回的值分三档：阻塞在发送 → `p_sendto_e`；阻塞在接收 → `p_getfrom_e`；没阻塞 → `NONE`。

检测函数沿着这个宏走链：

```c
static int deadlock(
  int function,				/* trap number */
  register struct proc *cp,		/* pointer to caller */
  endpoint_t src_dst_e			/* src or dst process */
)
{
/* Check for deadlock. This can happen if 'caller_ptr' and 'src_dst' have
 * a cyclic dependency of blocking send and receive calls. The only cyclic 
 * dependency that is not fatal is if the caller and target directly SEND(REC)
 * and RECEIVE to each other. If a deadlock is found, the group size is 
 * returned. Otherwise zero is returned. 
 */
  register struct proc *xp;			/* process pointer */
  int group_size = 1;				/* start with only caller */
#if DEBUG_ENABLE_IPC_WARNINGS
  static struct proc *processes[NR_PROCS + NR_TASKS];
  processes[0] = cp;
#endif

  while (src_dst_e != ANY) { 			/* check while process nr */
      int src_dst_slot;
      okendpt(src_dst_e, &src_dst_slot);
      xp = proc_addr(src_dst_slot);		/* follow chain of processes */
      assert(proc_ptr_ok(xp));
      assert(!RTS_ISSET(xp, RTS_SLOT_FREE));
#if DEBUG_ENABLE_IPC_WARNINGS
      processes[group_size] = xp;
#endif
      group_size ++;				/* extra process in group */

      /* Check whether the last process in the chain has a dependency. If it 
       * has not, the cycle cannot be closed and we are done.
       */
      if((src_dst_e = P_BLOCKEDON(xp)) == NONE)
	return 0;

      /* Now check if there is a cyclic dependency. For group sizes of two,  
       * a combination of SEND(REC) and RECEIVE is not fatal. Larger groups
       * or other combinations indicate a deadlock.  
       */
      if (src_dst_e == cp->p_endpoint) {	/* possible deadlock */
	  if (group_size == 2) {		/* caller and src_dst */
	      /* The function number is magically converted to flags. */
	      if ((xp->p_rts_flags ^ (function << 2)) & RTS_SENDING) { 
	          return(0);			/* not a deadlock */
	      }
	  }
#if DEBUG_ENABLE_IPC_WARNINGS
	  {
		int i;
		printf("deadlock between these processes:\n");
		for(i = 0; i < group_size; i++) {
			printf(" %10s ", processes[i]->p_name);
		}
		printf("\n\n");
		for(i = 0; i < group_size; i++) {
			print_proc(processes[i]);
			proc_stacktrace(processes[i]);
		}
	  }
#endif
          return(group_size);			/* deadlock found */
      }
  }
  return(0);					/* not a deadlock */
}
```

（区间复算：`sed -n '703,768p' minix3/minix/kernel/proc.c | wc -l` = 66。锚点：`minix3/minix/kernel/proc.c` 的 `deadlock`（上行逐字块即全函数）。）

走链规则逐条拆：从目标的端点出发，每步用 `P_BLOCKEDON` 问「当前节点在等谁」；`ANY` 终止（目标在收任何来源，链没有指向）返回 0；`NONE` 终止（链断在某个不阻塞的进程）返回 0；链回到调用者自己的端点 → 找到环，返回环的规模 `group_size`。两个调用点只传两种 trap 号：`mini_send` 停车前传 `SEND`（两个函数都在 `minix3/minix/kernel/proc.c`，调用行在各自函数体内）。

XOR 豁免是全函数最密的一行，把 trap 号当位模式用。先看编码规律——trap 号常量与 RTS 位的对齐：

```c
 /* System call numbers that are passed when trapping to the kernel. */
#define SEND		   1	/* blocking send */
#define RECEIVE		   2	/* blocking receive */
#define SENDREC	 	   3  	/* SEND + RECEIVE */
#define NOTIFY		   4	/* asynchronous notify */
#define SENDNB             5    /* nonblocking send */
#define MINIX_KERNINFO     6    /* request kernel info structure */
#define SENDA		   16	/* asynchronous send */
#define IPCNO_HIGHEST	SENDA
```

（区间复算：`sed -n '6,14p' minix3/minix/include/minix/ipcconst.h | wc -l` = 9。锚点：`minix3/minix/include/minix/ipcconst.h:SEND`。RTS 位值见 `minix3/minix/kernel/proc.h` 的 `RTS_SENDING`/`RTS_RECEIVING`（0x04/0x08）。）

| 原语 | trap 号 | `function << 2` | 对齐的 RTS 位 |
|---|---|---|---|
| SEND | 1 | 0x04 | RTS_SENDING（0x04） |
| RECEIVE | 2 | 0x08 | RTS_RECEIVING（0x08） |
| SENDREC | 3 | 0x0C | SENDING 与 RECEIVING 同帧 |
| NOTIFY / SENDNB / SENDA | 4 / 5 / 16 | 从不进入 deadlock | （两者都不停） |

trap 号左移两位恰好落在 RTS 标志字的对应位上——这就是源码注释里那句「The function number is magically converted to flags」的全部机关。`deadlock` 的两个调用点只传 SEND 与 RECEIVE，SENDREC 的编码（0x0C）在现源没有直接的调用点；SENDREC 的两个半各自经 `mini_send`/`mini_receive` 传入 1 或 2，恰好各对齐一个位。

有了编码，豁免判定 `(xp->p_rts_flags ^ (function << 2)) & RTS_SENDING` 可以逐位读懂：XOR 在「链尾进程的实际等待方向」与「调用者本次调用的方向」不同的时候置起 SENDING 位。三个具体手推：

其一（豁免，SENDREC 对 RECEIVE 的互补对）。A 执行 SENDREC→B，在发送半停车前做预检：`deadlock(SEND, A, B)`。B 此刻阻塞在 RECEIVE←A（`RTS_RECEIVING` = 0x08）。链走一步：`P_BLOCKEDON(B)` = `p_getfrom_e` = A，回到调用者，`group_size` = 2。XOR：`(0x08 ^ 0x04) & 0x04` = 0x04，非零 → 判非死锁，A 停进 B 的等待队列。语义：B 在收、A 的消息投得出去，环自解——A 醒来的时刻就是 B 的 RECEIVE 命中队列的时刻。

其二（判死，互发）。A SEND→B 且 B SEND→A：`deadlock(SEND, A, B)` 走链回到 A，`group_size` = 2；B 的标志是 `RTS_SENDING`（0x04），`(0x04 ^ 0x04) & 0x04` = 0 → 死锁成立，A 得 ELOCKED。两人互发没有任何一方在收，这条环永远解不开。

其三（判死，三方环）。A SEND→B、B SEND→C、C SEND→A：`deadlock(SEND, A, B)` 从 B 走到 C 再走回 A，`group_size` = 3，豁免分支要求 `group_size == 2`，不成立 → 返回 3，A 得 ELOCKED。环的规模不影响结论，只影响返回值——而返回值除调试打印外没有消费者（§3.2 回来收这条线）。

豁免判定的两个对称失败形态也各推一个：RECEIVE 调用点的互补对——A RECEIVE←B 且 B SEND→A（B 的发送半因过滤未命中而停在队列上）：`(0x04 ^ 0x08) & 0x04` = 0x04 → 豁免；互收——A RECEIVE←B 且 B RECEIVE←A：`(0x08 ^ 0x08) & 0x04` = 0 → 死锁。

反向推演这个设计：把 XOR 换成 AND——判定变成「链尾在发 **且** 调用方向对齐 SENDING 位」，SENDREC 对 RECEIVE 的互补对里调用方向是 SEND、链尾是 RECEIVING，AND 出 0，互补对被误判死锁；SENDREC 的全部使用场景（先发后收的请求-回信模型）在第一次预检就被 ELOCKED 拦死。不检查 `group_size == 2`——三方环里也套用 XOR：三方 SEND 环的每一对相邻节点方向都「互补」（前者发、后者收），XOR 会置位，三方环被误判豁免放行，真死结留在了系统里。

### 2.3 mini_send：直接交付与阻塞排队（主走查三）

mini_send 全函数逐字如下：

```c
int mini_send(
  register struct proc *caller_ptr,	/* who is trying to send a message? */
  endpoint_t dst_e,			/* to whom is message being sent? */
  message *m_ptr,			/* pointer to message buffer */
  const int flags
)
{
/* Send a message from 'caller_ptr' to 'dst'. If 'dst' is blocked waiting
 * for this message, copy the message to it and unblock 'dst'. If 'dst' is
 * not waiting at all, or is waiting for another source, queue 'caller_ptr'.
 */
  register struct proc *dst_ptr;
  register struct proc **xpp;
  int dst_p;
  dst_p = _ENDPOINT_P(dst_e);
  dst_ptr = proc_addr(dst_p);

  if (RTS_ISSET(dst_ptr, RTS_NO_ENDPOINT))
  {
	return EDEADSRCDST;
  }

  /* Check if 'dst' is blocked waiting for this message. The destination's 
   * RTS_SENDING flag may be set when its SENDREC call blocked while sending.  
   */
  if (WILLRECEIVE(caller_ptr->p_endpoint, dst_ptr, (vir_bytes)m_ptr, NULL)) {
	int call;
	/* Destination is indeed waiting for this message. */
	assert(!(dst_ptr->p_misc_flags & MF_DELIVERMSG));	

	if (!(flags & FROM_KERNEL)) {
		if(copy_msg_from_user(m_ptr, &dst_ptr->p_delivermsg))
			return EFAULT;
	} else {
		dst_ptr->p_delivermsg = *m_ptr;
		IPC_STATUS_ADD_FLAGS(dst_ptr, IPC_FLG_MSG_FROM_KERNEL);
	}

	dst_ptr->p_delivermsg.m_source = caller_ptr->p_endpoint;
	dst_ptr->p_misc_flags |= MF_DELIVERMSG;

	call = (caller_ptr->p_misc_flags & MF_REPLY_PEND ? SENDREC
		: (flags & NON_BLOCKING ? SENDNB : SEND));
	IPC_STATUS_ADD_CALL(dst_ptr, call);

	if (dst_ptr->p_misc_flags & MF_REPLY_PEND)
		dst_ptr->p_misc_flags &= ~MF_REPLY_PEND;

	RTS_UNSET(dst_ptr, RTS_RECEIVING);

#if DEBUG_IPC_HOOK
	hook_ipc_msgsend(&dst_ptr->p_delivermsg, caller_ptr, dst_ptr);
	hook_ipc_msgrecv(&dst_ptr->p_delivermsg, caller_ptr, dst_ptr);
#endif
  } else {
	if(flags & NON_BLOCKING) {
		return(ENOTREADY);
	}

	/* Check for a possible deadlock before actually blocking. */
	if (deadlock(SEND, caller_ptr, dst_e)) {
		return(ELOCKED);
	}

	/* Destination is not waiting.  Block and dequeue caller. */
	if (!(flags & FROM_KERNEL)) {
		if(copy_msg_from_user(m_ptr, &caller_ptr->p_sendmsg))
			return EFAULT;
	} else {
		caller_ptr->p_sendmsg = *m_ptr;
		/*
		 * we need to remember that this message is from kernel so we
		 * can set the delivery status flags when the message is
		 * actually delivered
		 */
		caller_ptr->p_misc_flags |= MF_SENDING_FROM_KERNEL;
	}

	RTS_SET(caller_ptr, RTS_SENDING);
	caller_ptr->p_sendto_e = dst_e;

	/* Process is now blocked.  Put in on the destination's queue. */
	assert(caller_ptr->p_q_link == NULL);
	xpp = &dst_ptr->p_caller_q;		/* find end of list */
	while (*xpp) xpp = &(*xpp)->p_q_link;	
	*xpp = caller_ptr;			/* add caller to end */

#if DEBUG_IPC_HOOK
	hook_ipc_msgsend(&caller_ptr->p_sendmsg, caller_ptr, dst_ptr);
#endif
  }
  return(OK);
}
```

（区间复算：`sed -n '870,962p' minix3/minix/kernel/proc.c | wc -l` = 93。锚点：`minix3/minix/kernel/proc.c` 的 `mini_send`（上行逐字块即全函数）。`WILLRECEIVE`/`CANRECEIVE` 宏与两个发送标志定义在 `minix3/minix/kernel/ipc.h`。）

```c
/* Masks and flags for system calls. */
#define NON_BLOCKING    0x0080  /* do not block if target not ready */
#define FROM_KERNEL     0x0100  /* message from kernel on behalf of a process */

#define WILLRECEIVE(src_e,dst_ptr,m_src_v,m_src_p) \
	((RTS_ISSET(dst_ptr, RTS_RECEIVING) && \
	!RTS_ISSET(dst_ptr, RTS_SENDING)) && \
	CANRECEIVE(dst_ptr->p_getfrom_e,src_e,dst_ptr,m_src_v,m_src_p))

#define CANRECEIVE(receive_e,src_e,dst_ptr,m_src_v,m_src_p) \
	(((receive_e) == ANY || (receive_e) == (src_e)) && \
	(priv(dst_ptr)->s_ipcf == NULL || \
	allow_ipc_filtered_msg(dst_ptr,src_e,m_src_v,m_src_p)))
```

（区间复算：`sed -n '10,22p' minix3/minix/kernel/ipc.h | wc -l` = 13。锚点：`minix3/minix/kernel/ipc.h:WILLRECEIVE`、`:CANRECEIVE`。）

两个宏先拆开。`WILLRECEIVE(源端点, 目标, ...)` 问的是「目标此刻愿意收**我**这条吗」：目标停成可收状态（`RTS_RECEIVING` 置位且未在发送）、且它的 `p_getfrom_e` 匹配我的端点（收任何来源或正好收我）、且过滤判定放行（`CANRECEIVE` 的后半：无过滤配置、或过滤链允许这条消息——过滤链的决策面归 `23-ipc-filter` 方向，本篇只认它在匹配路径上的位置）。`NON_BLOCKING`（0x0080）与 `FROM_KERNEL`（0x0100）是 `mini_send` 的两个调用形态标志：前者把「停」换成「带错返回」，后者表示消息已在内核手里、不必再从用户空间拷。

函数体分成两条路径，画在一起：

```
                    mini_send(caller, dst_e, m, flags)
                                │
                 dst 置 RTS_NO_ENDPOINT？──是──► 返回 EDEADSRCDST
                                │ 否
                 WILLRECEIVE(caller, dst) 命中？
                    │ 是                          │ 否
          【路径一：直接交付】              flags 带 NON_BLOCKING？
     消息从用户拷进 dst 的 p_delivermsg      │ 是 → 返回 ENOTREADY
     m_source = caller 端点                 │ 否
     置 MF_DELIVERMSG                  deadlock(SEND) 预检命中？
     IPC_STATUS 记 call 类型             │ 是 → 返回 ELOCKED
     清 dst 的 RTS_RECEIVING（唤醒）       │ 否
          │                         【路径二：阻塞排队】
          ▼                         消息先拷进 caller 自己的 p_sendmsg
      返回 OK                       置 caller 的 RTS_SENDING + p_sendto_e
                                    挂进 dst 的 p_caller_q 链尾
                                        │
                                        ▼
                                    返回 OK（调用者已停）
```

三条失败返回各对应一个检查点：EDEADSRCDST（目标端点解析后是 `RTS_NO_ENDPOINT` 槽）、ENOTREADY（非阻塞探测发现目标不在等）、ELOCKED（死锁预检命中）；外加拷贝失败的一条 EFAULT（用户指针坏，`copy_msg_from_user` 失败）。

路径一里藏着交付协议的第一段：消息落在 `dst->p_delivermsg`（接收者槽内的内核中转位置），`m_source` 由内核盖上发送者端点，`MF_DELIVERMSG` 置位宣告「有待交付」，然后才清接收者的 `RTS_RECEIVING` 唤醒它——拷到用户缓冲的动作不在这里做，为什么能推迟、推迟到哪里，§2.7 用 `delivermsg` 收这条线。`IPC_STATUS_ADD_CALL` 把这次交付的 call 类型（SENDREC/SENDNB/SEND 三选一，按发送者的 `MF_REPLY_PEND` 与 flags 判定）记进接收者的 IPC 状态寄存器，用户态库据此知道刚收到的这条要不要回信。

路径二里藏着本节单独成句的那个事实：阻塞的 SEND 把消息**先拷进发送者自己的内核槽**——`copy_msg_from_user(m_ptr, &caller_ptr->p_sendmsg)` 在置位与排队之前执行。停着的时候消息已经离手：发送者的用户缓冲此后怎么改写都不影响这条消息的内容，接收者将来从队列上取走的是 `p_sendmsg` 这份内核副本。直接交付路径的拷贝（`copy_msg_from_user(m_ptr, &dst_ptr->p_delivermsg)`）是两段协议里的第一段就完成了全部搬运——消息没有中途停留，接收者被唤醒后只剩「从内核槽拷到自己用户缓冲」这半段。

反向推演：设想不预拷、停着时保留用户指针，等接收者来取时再从 `m_ptr` 读。发送者虽然停在 IPC 上，它的地址空间语义在这段等待里并不受保证——它的页可能被 VM 挪动（`24-cross-space-runtime` 方向的 VMSUSPEND 场景正是处理「内核访问用户页时发现页不在」的机制），缓冲内容也可能在此期间被信号处理或其它机制改写。内核副本把协议的可靠性锚在拷贝发生的时刻：只要拷进了 `p_sendmsg`，这条消息的内容就冻结了。

`FROM_KERNEL` 变体把拷贝换成槽间赋值（消息已在内核），并置 `MF_SENDING_FROM_KERNEL` 让交付时能在 IPC 状态里补记来源标志。这个形态的既有消费点在缺页转发臂：页错误处理以 FROM_KERNEL 形状代被中断的进程向 VM 推送消息（`minix3/minix/kernel/arch/earm/exception.c` 的 `m_pagefault` 构造）；`09-vm-boot-protocol.md` §2.3 的 MEMREQ GET/REPLY 则是 VM 拉取方向的回信形状（内核把请求字段填进回信、由 VM 取走），两者是 FROM_KERNEL 语义的两个方向。本篇在 §4.1 看 Rust 侧的对应物。

### 2.4 mini_receive：三级匹配序（主走查四）

```c
static int mini_receive(struct proc * caller_ptr,
			endpoint_t src_e, /* which message source is wanted */
			message * m_buff_usr, /* pointer to message buffer */
			const int flags)
{
/* A process or task wants to get a message.  If a message is already queued,
 * acquire it and deblock the sender.  If no message from the desired source
 * is available block the caller.
 */
  register struct proc **xpp;
  int r, src_id, found, src_proc_nr, src_p;
  endpoint_t sender_e;

  assert(!(caller_ptr->p_misc_flags & MF_DELIVERMSG));

  /* This is where we want our message. */
  caller_ptr->p_delivermsg_vir = (vir_bytes) m_buff_usr;

  if(src_e == ANY) src_p = ANY;
  else
  {
	okendpt(src_e, &src_p);
	if (RTS_ISSET(proc_addr(src_p), RTS_NO_ENDPOINT))
	{
		return EDEADSRCDST;
	}
  }


  /* Check to see if a message from desired source is already available.  The
   * caller's RTS_SENDING flag may be set if SENDREC couldn't send. If it is
   * set, the process should be blocked.
   */
  if (!RTS_ISSET(caller_ptr, RTS_SENDING)) {

    /* Check if there are pending notifications, except for SENDREC. */
    if (! (caller_ptr->p_misc_flags & MF_REPLY_PEND)) {

	/* Check for pending notifications */
        src_id = has_pending_notify(caller_ptr, src_p);
        found = src_id != NULL_PRIV_ID;
        if(found) {
            src_proc_nr = id_to_nr(src_id);		/* get source proc */
            sender_e = proc_addr(src_proc_nr)->p_endpoint;
        }

        if (found && CANRECEIVE(src_e, sender_e, caller_ptr, 0,
          &m_notify_buff)) {

#if DEBUG_ENABLE_IPC_WARNINGS
	    if(src_proc_nr == NONE) {
		printf("mini_receive: sending notify from NONE\n");
	    }
#endif
	    assert(src_proc_nr != NONE);
            unset_notify_pending(caller_ptr, src_id);	/* no longer pending */

            /* Found a suitable source, deliver the notification message. */
	    assert(!(caller_ptr->p_misc_flags & MF_DELIVERMSG));	
	    assert(src_e == ANY || sender_e == src_e);

	    /* assemble message */
	    BuildNotifyMessage(&caller_ptr->p_delivermsg, src_proc_nr, caller_ptr);
	    caller_ptr->p_delivermsg.m_source = sender_e;
	    caller_ptr->p_misc_flags |= MF_DELIVERMSG;

	    IPC_STATUS_ADD_CALL(caller_ptr, NOTIFY);

	    goto receive_done;
        }
    }

    /* Check for pending asynchronous messages */
    if (has_pending_asend(caller_ptr, src_p) != NULL_PRIV_ID) {
        if (src_p != ANY)
		r = try_one(src_e, proc_addr(src_p), caller_ptr);
        else
        	r = try_async(caller_ptr);

	if (r == OK) {
            IPC_STATUS_ADD_CALL(caller_ptr, SENDA);
            goto receive_done;
        }
    }

    /* Check caller queue. Use pointer pointers to keep code simple. */
    xpp = &caller_ptr->p_caller_q;
    while (*xpp) {
	struct proc * sender = *xpp;
	endpoint_t sender_e = sender->p_endpoint;

        if (CANRECEIVE(src_e, sender_e, caller_ptr, 0, &sender->p_sendmsg)) {
            int call;
	    assert(!RTS_ISSET(sender, RTS_SLOT_FREE));
	    assert(!RTS_ISSET(sender, RTS_NO_ENDPOINT));

	    /* Found acceptable message. Copy it and update status. */
  	    assert(!(caller_ptr->p_misc_flags & MF_DELIVERMSG));
	    caller_ptr->p_delivermsg = sender->p_sendmsg;
	    caller_ptr->p_delivermsg.m_source = sender->p_endpoint;
	    caller_ptr->p_misc_flags |= MF_DELIVERMSG;
	    RTS_UNSET(sender, RTS_SENDING);

	    call = (sender->p_misc_flags & MF_REPLY_PEND ? SENDREC : SEND);
	    IPC_STATUS_ADD_CALL(caller_ptr, call);

	    /*
	     * if the message is originally from the kernel on behalf of this
	     * process, we must send the status flags accordingly
	     */
	    if (sender->p_misc_flags & MF_SENDING_FROM_KERNEL) {
		IPC_STATUS_ADD_FLAGS(caller_ptr, IPC_FLG_MSG_FROM_KERNEL);
		/* we can clean the flag now, not need anymore */
		sender->p_misc_flags &= ~MF_SENDING_FROM_KERNEL;
	    }
	    if (sender->p_misc_flags & MF_SIG_DELAY)
		sig_delay_done(sender);

#if DEBUG_IPC_HOOK
            hook_ipc_msgrecv(&caller_ptr->p_delivermsg, *xpp, caller_ptr);
#endif
		
            *xpp = sender->p_q_link;		/* remove from queue */
	    sender->p_q_link = NULL;
	    goto receive_done;
	}
	xpp = &sender->p_q_link;		/* proceed to next */
    }
  }

  /* No suitable message is available or the caller couldn't send in SENDREC. 
   * Block the process trying to receive, unless the flags tell otherwise.
   */
  if ( ! (flags & NON_BLOCKING)) {
      /* Check for a possible deadlock before actually blocking. */
      if (deadlock(RECEIVE, caller_ptr, src_e)) {
          return(ELOCKED);
      }

      caller_ptr->p_getfrom_e = src_e;		
      RTS_SET(caller_ptr, RTS_RECEIVING);
      return(OK);
  } else {
	return(ENOTREADY);
  }

receive_done:
  if (caller_ptr->p_misc_flags & MF_REPLY_PEND)
	  caller_ptr->p_misc_flags &= ~MF_REPLY_PEND;
  return OK;
}
```

（区间复算：`sed -n '967,1117p' minix3/minix/kernel/proc.c | wc -l` = 151。锚点：`minix3/minix/kernel/proc.c` 的 `mini_receive`（上行逐字块即全函数）；`:has_pending`、`:has_pending_notify`、`:has_pending_asend`、`:unset_notify_pending`。）

函数入口先记下两件事：用户缓冲地址存进 `p_delivermsg_vir`（本篇 §2.7 的延迟拷贝用它当目的地）；指定来源时先验端点有效性。真正的匹配是三级瀑布，从轻到重：

| 级 | 检查什么 | 命中后做什么 | 代码位置（函数内） |
|---|---|---|---|
| 第一级 | 通知位图（`has_pending_notify`），整级被 `MF_REPLY_PEND` 跳过 | 组装通知消息（`BuildNotifyMessage`）、清对应位、置 `MF_DELIVERMSG`、状态寄存器记 NOTIFY | 三级瀑布首段 |
| 第二级 | 异步遗留（`has_pending_asend`） | 指定来源走 `try_one`、ANY 走 `try_async`，成功则状态寄存器记 SENDA | 三级瀑布中段 |
| 第三级 | 同步等待队列 `p_caller_q` | 逐个验 `CANRECEIVE`，命中者：拷 `p_sendmsg`、清发送者 `RTS_SENDING`、摘链、补 FROM_KERNEL 状态、`MF_SIG_DELAY` 者补信号协议 | 三级瀑布尾段 |

三级的次序是固定的：**通知 > 异步 > 同步**。来源优先级跟着这个次序走——一个先阻塞在队列上的发送者，可能被后来的一次通知抢先：第三级还没轮到它，第一级已经交付并返回了。这个次序的理由藏在两类消息的成本差里：通知是内核代投的轻量消息（内容由内核组装，不占发送者的运行），若排在队列之后，一个被大量请求淹没的服务器可能永远收不到时钟或信号这类「不可拖延的状态更新」——通知语义就此失效。反向推演：设想同步优先于通知——HARDWARE 的时钟通知排在第 N 个排队请求之后，每个请求都携带真实工作，队列消化完之前时钟通知递不出去，依赖通知做节拍的上层机制全部迟到。

三个非显然细节各单独记。其一，整段瀑布包在 `!RTS_ISSET(caller_ptr, RTS_SENDING)` 之内：SENDREC 的调用者若发送半还停着（自己排在自己目标的队列上），它跳过全部三级直接落进停车段——不跳的话，它会在扫队列时撞见别人的排队请求，把别人的消息当自己的回信收走。其二，第三级命中后 `RTS_UNSET(sender, RTS_SENDING)` 唤醒发送者，但发送者此刻停在自己的陷入里，它的返回值不在这里写——阻塞 SEND 的「返回 OK」早在停车时就写好了（§4.1 回来看 Rust 侧怎么对齐这条）。其三，瀑布全落空才做 `deadlock(RECEIVE)` 预检，然后停车（`p_getfrom_e` + `RTS_RECEIVING`）；`NON_BLOCKING` 形态不停，带 ENOTREADY 返回。

出口处 `receive_done` 统一撤销 `MF_REPLY_PEND`——无论消息从哪一级交付，SENDREC 的「通知禁入」保护在收到消息的那一刻解除。

### 2.5 mini_notify：立即交付与位图缓冲

```c
int mini_notify(
  const struct proc *caller_ptr,	/* sender of the notification */
  endpoint_t dst_e			/* which process to notify */
)
{
  register struct proc *dst_ptr;
  int src_id;				/* source id for late delivery */
  int dst_p;

  if (!isokendpt(dst_e, &dst_p)) {
	util_stacktrace();
	printf("mini_notify: bogus endpoint %d\n", dst_e);
	return EDEADSRCDST;
  }

  dst_ptr = proc_addr(dst_p);

  /* Check to see if target is blocked waiting for this message. A process 
   * can be both sending and receiving during a SENDREC system call.
   */
  if (WILLRECEIVE(caller_ptr->p_endpoint, dst_ptr, 0, &m_notify_buff) &&
    !(dst_ptr->p_misc_flags & MF_REPLY_PEND)) {
      /* Destination is indeed waiting for a message. Assemble a notification 
       * message and deliver it. Copy from pseudo-source HARDWARE, since the
       * message is in the kernel's address space.
       */ 
      assert(!(dst_ptr->p_misc_flags & MF_DELIVERMSG));

      BuildNotifyMessage(&dst_ptr->p_delivermsg, proc_nr(caller_ptr), dst_ptr);
      dst_ptr->p_delivermsg.m_source = caller_ptr->p_endpoint;
      dst_ptr->p_misc_flags |= MF_DELIVERMSG;

      IPC_STATUS_ADD_CALL(dst_ptr, NOTIFY);
      RTS_UNSET(dst_ptr, RTS_RECEIVING);

      return(OK);
  } 

  /* Destination is not ready to receive the notification. Add it to the 
   * bit map with pending notifications. Note the indirectness: the privilege id
   * instead of the process number is used in the pending bit map.
   */ 
  src_id = priv(caller_ptr)->s_id;
  set_sys_bit(priv(dst_ptr)->s_notify_pending, src_id); 
  return(OK);
}
```

（区间复算：`sed -n '1122,1167p' minix3/minix/kernel/proc.c | wc -l` = 46。锚点：`minix3/minix/kernel/proc.c:mini_notify`。）

NOTIFY 的两条路画在一起：

```
    mini_notify(caller, dst_e)
              │
    dst 在收（WILLRECEIVE）且不在 SENDREC 中？
       │ 是                             │ 否
  【立即交付】                      【位图记账】
  BuildNotifyMessage 组装            src_id = priv(caller)->s_id
    （HARDWARE 携中断位图、           set_sys_bit(priv(dst)->
     SYSTEM 携待决信号，见 §2.7）        s_notify_pending, src_id)
  落进 dst 的 p_delivermsg
  清 dst 的 RTS_RECEIVING（唤醒）      （永不阻塞、永不丢失）
       │
       ▼
    返回 OK
```

两条路的共同点：调用者永不阻塞。差别在「目标不在等」时消息去哪——这就是位图。位图的索引选择写在源码注释里：**Note the indirectness**——记录挂进接收者的特权结构 `s_notify_pending`，位的编号用的是**发送者的特权 id**（`priv(caller)->s_id`），不是进程号。特权 id 与进程的绑定比槽位稳定：特权结构在进程创建时授予、生命周期与身份绑定，而进程槽会复用。已记账的通知因此不随槽位复用漂移——它属于「那个特权身份」，不属于「那个槽」。

反方向推演：设想位图按进程号索引。时钟任务对进程 17 记了一次「时钟已走」的通知，进程 17 退出，槽位复用给一个全新进程；新进程第一次 RECEIVE，位图里那位还亮着，内核把旧通知投给它——新进程收到一条自己从未订阅过的「时钟已走」，语义灾难发生在没有任何代码出错的情况下。按特权 id 索引后，通知跟着身份走：槽位换了人，新进程的特权结构里没有那位，旧通知仍归旧身份（重启后的同一身份才收得回，这正是期望）。

「永不丢失」的另一半在消费端：`mini_receive` 第一级扫位图命中即清位、组装、交付（§2.4）；`unset_notify_pending` 的时机在组装之前，同一次陷入内完成，窗口不存在。通知因此既是旁路（不阻塞任何人）又是账目（不丢任何一次）。

### 2.6 SENDA：异步表与三方函数（try_deliver_senda/try_async/try_one）

SENDA 是六原语里唯一的「整批」形态：调用者一次提交一张表，每项一条异步消息，内核逐条办、逐条写回结果。先看两个承载逐条协议的宏：

```c
#define ASCOMPLAIN(caller, entry, field)	\
	printf("kernel:%s:%d: asyn failed for %s in %s "	\
	"(%d/%zu, tab 0x%lx)\n",__FILE__,__LINE__,	\
field, caller->p_name, entry, priv(caller)->s_asynsize, priv(caller)->s_asyntab)

#define A_RETR(entry) do {			\
  if (data_copy(				\
  		caller_ptr->p_endpoint, table_v + (entry)*sizeof(asynmsg_t),\
  		KERNEL, (vir_bytes) &tabent,	\
  		sizeof(tabent)) != OK) {	\
  			ASCOMPLAIN(caller_ptr, entry, "message entry");	\
  			r = EFAULT;		\
	                goto asyn_error; \
  }						\
  else if(tabent.dst == SELF) { \
      tabent.dst = caller_ptr->p_endpoint; \
  } \
  			 } while(0)

#define A_INSRT(entry) do {			\
  if (data_copy(KERNEL, (vir_bytes) &tabent,	\
  		caller_ptr->p_endpoint, table_v + (entry)*sizeof(asynmsg_t),\
  		sizeof(tabent)) != OK) {	\
  			ASCOMPLAIN(caller_ptr, entry, "message entry");	\
			/* Do NOT set r or goto asyn_error here! */ \
  }						\
  			  } while(0)	
```

（区间复算：`sed -n '1169,1195p' minix3/minix/kernel/proc.c | wc -l` = 27。锚点：`minix3/minix/kernel/proc.c:A_RETR`、`minix3/minix/kernel/proc.c:A_INSRT`（宏定义于 proc.c 头部区）。）

`A_RETR(entry)` 把用户表的一条表项拷进内核（拷失败打印告警后**续跑下一条**，本条不写回结果）；`A_INSRT(entry)` 把结果写回用户表的同一条（拷失败**不**终止——宏注释原文 「Do NOT set r or goto asyn_error here!」：单条结果写不回去，不能拖死其余合法表项）。两个宏的不对称各 protect 一面：A_RETR 失败时，内核连这条表项的目的与旗标都没读到——既无从投递、也无从写一个有意义的结果，更不能替它标 DONE（标了就永久吞掉一条可能合法的消息），唯一安全的动作是留下它等下一次重试，所以既不终止也不写回；A_INSRT 失败时消息可能已经投递成功，结果写不回只是发送者看不到回执，终止整批会把破坏半径从一条扩大到整批。两个宏共同指向一个结构性事实：SENDA 表常驻**用户空间**，内核不缓存副本——提交时逐条 `A_RETR` 拷入处理、逐条 `A_INSRT` 写回结果，表的状态权威始终在用户内存里，内核只做裁判。

提交函数全貌：

```c
int try_deliver_senda(struct proc *caller_ptr,
				asynmsg_t *table,
				size_t size)
{
  int r, dst_p, done, do_notify;
  unsigned int i;
  unsigned flags;
  endpoint_t dst;
  struct proc *dst_ptr;
  struct priv *privp;
  asynmsg_t tabent;
  const vir_bytes table_v = (vir_bytes) table;
  message *m_ptr = NULL;

  privp = priv(caller_ptr);

  /* Clear table */
  privp->s_asyntab = -1;
  privp->s_asynsize = 0;
  privp->s_asynendpoint = caller_ptr->p_endpoint;

  if (size == 0) return(OK);  /* Nothing to do, just return */

  /* Scan the table */
  do_notify = FALSE;
  done = TRUE;

  /* Limit size to something reasonable. An arbitrary choice is 16
   * times the number of process table entries.
   *
   * (this check has been duplicated in sys_call but is left here
   * as a sanity check)
   */
  if (size > 16*(NR_TASKS + NR_PROCS)) {
    r = EDOM;
    return r;
  }

  for (i = 0; i < size; i++) {
	/* Process each entry in the table and store the result in the table.
	 * If we're done handling a message, copy the result to the sender. */

	dst = NONE;
	/* Copy message to kernel */
	A_RETR(i);
	flags = tabent.flags;
	dst = tabent.dst;

	if (flags == 0) continue; /* Skip empty entries */

	/* 'flags' field must contain only valid bits */
	if(flags & ~(AMF_VALID|AMF_DONE|AMF_NOTIFY|AMF_NOREPLY|AMF_NOTIFY_ERR)) {
		r = EINVAL;
		goto asyn_error;
	}
	if (!(flags & AMF_VALID)) { /* Must contain message */
		r = EINVAL;
		goto asyn_error;
	}
	if (flags & AMF_DONE) continue;	/* Already done processing */

	r = OK;
	if (!isokendpt(tabent.dst, &dst_p)) 
		r = EDEADSRCDST; /* Bad destination, report the error */
	else if (iskerneln(dst_p)) 
		r = ECALLDENIED; /* Asyn sends to the kernel are not allowed */
	else if (!may_asynsend_to(caller_ptr, dst_p))
		r = ECALLDENIED; /* Send denied by IPC mask */
	else 	/* r == OK */
		dst_ptr = proc_addr(dst_p);

	/* XXX: RTS_NO_ENDPOINT should be removed */
	if (r == OK && RTS_ISSET(dst_ptr, RTS_NO_ENDPOINT)) {
		r = EDEADSRCDST;
	}

	/* Check if 'dst' is blocked waiting for this message.
	 * If AMF_NOREPLY is set, do not satisfy the receiving part of
	 * a SENDREC.
	 */
	if (r == OK && WILLRECEIVE(caller_ptr->p_endpoint, dst_ptr,
	    (vir_bytes)&table[i].msg, NULL) &&
	    (!(flags&AMF_NOREPLY) || !(dst_ptr->p_misc_flags&MF_REPLY_PEND))) {
		/* Destination is indeed waiting for this message. */
		dst_ptr->p_delivermsg = tabent.msg;
		dst_ptr->p_delivermsg.m_source = caller_ptr->p_endpoint;
		dst_ptr->p_misc_flags |= MF_DELIVERMSG;
		IPC_STATUS_ADD_CALL(dst_ptr, SENDA);
		RTS_UNSET(dst_ptr, RTS_RECEIVING);
#if DEBUG_IPC_HOOK
		hook_ipc_msgrecv(&dst_ptr->p_delivermsg, caller_ptr, dst_ptr);
#endif
	} else if (r == OK) {
		/* Inform receiver that something is pending */
		set_sys_bit(priv(dst_ptr)->s_asyn_pending, 
			    priv(caller_ptr)->s_id); 
		done = FALSE;
		continue;
	} 

	/* Store results */
	tabent.result = r;
	tabent.flags = flags | AMF_DONE;
	if (flags & AMF_NOTIFY)
		do_notify = TRUE;
	else if (r != OK && (flags & AMF_NOTIFY_ERR))
		do_notify = TRUE;
	A_INSRT(i);	/* Copy results to caller; ignore errors */
	continue;

asyn_error:
	if (dst != NONE)
		printf("KERNEL senda error %d to %d\n", r, dst);
	else
		printf("KERNEL senda error %d\n", r);
  }

  if (do_notify) 
	mini_notify(proc_addr(ASYNCM), caller_ptr->p_endpoint);

  if (!done) {
	privp->s_asyntab = (vir_bytes) table;
	privp->s_asynsize = size;
  }

  return(OK);
}
```

（区间复算：`sed -n '1200,1326p' minix3/minix/kernel/proc.c | wc -l` = 127。锚点：`minix3/minix/kernel/proc.c:try_deliver_senda`。）

表的生命周期据此可以画全：

```
   提交（try_deliver_senda）                兑现（接收侧，mini_receive 第二级）
   ────────────────────────                ──────────────────────────────────
   清特权结构的表登记                        has_pending_asend 发现遗留位
   （s_asyntab=-1, s_asynsize=0）                  │
        │                                    try_one(src, ...)：
   逐条循环：                                 逐条 A_RETR 拷入
     A_RETR 拷入 → 校验 flags                 dst 不匹配/CANRECEIVE 拒 → 跳过
     ├ 空项/已办结 → 跳过                     命中 → 直接交付 + A_INSRT 写回
     ├ 立即交付（目标在收）                   全表办结 → 清登记
     │   + A_INSRT 写回 OK                    有遗留 → 位图重挂，登记恢复
     ├ 投不出去 → s_asyn_pending 记位          （s_asyntab/s_asynsize）
     │   done=FALSE，本条不写回                      │
     ├ flags 违例 → 告警、不写回、留待重试              │
     └ 目的端错误 → A_INSRT 写回错误码                 ▼
        │                                  接收者再收时又走左列的兑现列
   收尾：done? 清登记收工
        : 登记恢复（s_asyntab/s_asynsize）
          + 对自己补发一次 ASYNCM 通知
```

逐条办的四条分支各有一句要说。目标恰在收 → 立即交付（同 §2.3 路径一的形状，状态寄存器记 SENDA）；投不出去 → 在目标特权结构 `s_asyn_pending` 上记发送者的位，`done` 置否，本条**不写回结果**（还没办完，何谈结果）；flags 违例（EINVAL）→ 与拷入失败同一条退出道：打印告警、**不写回**、表项停在未办结态等重试——「写回错误码」只发生在目的端错误那组分支（EDEADSRCDST/ECALLDENIED 落入 Store results 段）；`dst == SELF` 的自指项在 `A_RETR` 宏里就地替换成调用者端点。收尾的两个对称动作把生命周期闭合：全部办结 → 表登记清掉（`s_asyntab = -1`、`s_asynsize = 0`）；有遗留 → 登记恢复（地址+尺寸存回特权结构），接收者将来扫异步位图时按登记找到这张表。收尾那行 `mini_notify(proc_addr(ASYNCM), ...)` 里的 `ASYNCM` 是一个专用于异步投递的伪发送者端点（`minix3/minix/include/minix/com.h` 定义）：给自己补发的这条通知借它的名字，把「你有一批异步消息办结/有进展」用通知通道捎回去。

兑现侧的两个函数：

```c
static int try_async(struct proc * caller_ptr)
{
  int r;
  struct priv *privp;
  struct proc *src_ptr;
  sys_map_t *map;

  map = &priv(caller_ptr)->s_asyn_pending;

  /* Try all privilege structures */
  for (privp = BEG_PRIV_ADDR; privp < END_PRIV_ADDR; ++privp)  {
	if (privp->s_proc_nr == NONE)
		continue;

	if (!get_sys_bit(*map, privp->s_id)) 
		continue;

	src_ptr = proc_addr(privp->s_proc_nr);

#ifdef CONFIG_SMP
	/*
	 * Do not copy from a process which does not have a stable address space
	 * due to VM fiddling with it
	 */
	if (RTS_ISSET(src_ptr, RTS_VMINHIBIT)) {
		src_ptr->p_misc_flags |= MF_SENDA_VM_MISS;
		continue;
	}
#endif

	assert(!(caller_ptr->p_misc_flags & MF_DELIVERMSG));
	if ((r = try_one(ANY, src_ptr, caller_ptr)) == OK)
		return(r);
  }

  return(ESRCH);
}
```

（区间复算：`sed -n '1348,1384p' minix3/minix/kernel/proc.c | wc -l` = 37。锚点：`minix3/minix/kernel/proc.c:try_async`。）

```c
static int try_one(endpoint_t receive_e, struct proc *src_ptr,
    struct proc *dst_ptr)
{
/* Try to receive an asynchronous message from 'src_ptr' */
  int r = EAGAIN, done, do_notify;
  unsigned int flags, i;
  size_t size;
  endpoint_t dst, src_e;
  struct proc *caller_ptr;
  struct priv *privp;
  asynmsg_t tabent;
  vir_bytes table_v;

  privp = priv(src_ptr);
  if (!(privp->s_flags & SYS_PROC)) return(EPERM);
  size = privp->s_asynsize;
  table_v = privp->s_asyntab;

  /* Clear table pending message flag. We're done unless we're not. */
  unset_sys_bit(priv(dst_ptr)->s_asyn_pending, privp->s_id);

  if (size == 0) return(EAGAIN);
  if (privp->s_asynendpoint != src_ptr->p_endpoint) return EAGAIN;
  if (!may_asynsend_to(src_ptr, proc_nr(dst_ptr))) return (ECALLDENIED);
```

（区间复算：`sed -n '1390,1413p' minix3/minix/kernel/proc.c | wc -l` = 24。本块为节选（1390-1413 行连续未删节），省略号处为 `try_one` 的发送者赋值、表扫描主体与结果写回、收尾段——下一块继续。锚点：`minix3/minix/kernel/proc.c` 的 `try_one`（两块合计覆盖函数主体）。）

```c

	/* Clear done flag. The sender is done sending when all messages in the
	 * table are marked done or empty. However, we will know that only
	 * the next time we enter this function or when the sender decides to
	 * send additional asynchronous messages and manages to deliver them
	 * all.
	 */
	done = FALSE;

	if (r == EINVAL)
		goto store_result;

	/* Message must be directed at receiving end */
	if (dst != dst_ptr->p_endpoint) continue;

	if (!CANRECEIVE(receive_e, src_e, dst_ptr,
		table_v + i*sizeof(asynmsg_t) + offsetof(struct asynmsg,msg),
		NULL)) {
		continue;
	}

	/* If AMF_NOREPLY is set, then this message is not a reply to a
	 * SENDREC and thus should not satisfy the receiving part of the
	 * SENDREC. This message is to be delivered later.
	 */
	if ((flags & AMF_NOREPLY) && (dst_ptr->p_misc_flags & MF_REPLY_PEND)) 
		continue;

	/* Destination is ready to receive the message; deliver it */
	r = OK;
	dst_ptr->p_delivermsg = tabent.msg;
	dst_ptr->p_delivermsg.m_source = src_ptr->p_endpoint;
	dst_ptr->p_misc_flags |= MF_DELIVERMSG;
#if DEBUG_IPC_HOOK
	hook_ipc_msgrecv(&dst_ptr->p_delivermsg, src_ptr, dst_ptr);
#endif

store_result:
	/* Store results for sender. We may just have started delivering a
	 * message, so we must not return an error to the caller in the case
	 * that storing the results triggers an error!
	 */
	tabent.result = r;
	tabent.flags = flags | AMF_DONE;
	if (flags & AMF_NOTIFY) do_notify = TRUE;
	else if (r != OK && (flags & AMF_NOTIFY_ERR)) do_notify = TRUE;
	A_INSRT(i);	/* Copy results to sender; ignore errors */

	break;
  }

  if (do_notify) 
	mini_notify(proc_addr(ASYNCM), src_ptr->p_endpoint);

  if (done) {
	privp->s_asyntab = -1;
	privp->s_asynsize = 0;
  } else {
	set_sys_bit(priv(dst_ptr)->s_asyn_pending, privp->s_id);
  }

asyn_error:
  return(r);
}
```

（区间复算：`sed -n '1442,1505p' minix3/minix/kernel/proc.c | wc -l` = 64。接上一块，省略的是扫描循环开头的逐条 `A_RETR` 与 flags 校验段（与 `try_deliver_senda` 同形）；本块覆盖命中判定、交付、结果写回与收尾。锚点：`minix3/minix/kernel/proc.c` 的 `try_one`。）

`try_async` 扫的是「谁欠我消息」：遍历全部特权结构，凡在接收者 `s_asyn_pending` 位图上挂了位的发送者，逐个交给 `try_one` 兑现，兑现成功即返回。`try_one` 办一张表：入口先清接收者位图上这位发送者的位（「办完了，除非办不完」——办不完在尾部重新挂回）；逐条 `A_RETR` 后做四道判定（dst 是否指向本次接收者、`CANRECEIVE` 是否放行、`AMF_NOREPLY` 项不得顶替 SENDREC 的收一半、flags 是否违例），命中一条即交付并写回，**一次只投一条**（循环里的 break）——一次 receive 至多消费一条异步消息。尾部的对称收尾与提交侧同形：全表办结清登记，有遗留重挂位图。

SMP 构建下还有一条补投支线：源在 VM 操作下地址空间被抑制（`RTS_VMINHIBIT`）时，`has_pending`/`try_async` 不从它的用户表拷数据，改为在源进程上置 `MF_SENDA_VM_MISS`（`minix3/minix/kernel/proc.c:has_pending` 与 `:try_async` 的 CONFIG_SMP 块）；等 VM 解除抑制（VMINHIBIT_CLEAR——位的定义在 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.2 的位族表，清腿在 [09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3 的 SETADDRSPACE/VMINHIBIT_CLEAR），补投机制把错过的那批重新兑现。

反向推演两处。其一，设想内核缓存整张表的副本——表尺寸上限 4176 项、每项一条消息，缓存即两份状态：用户表与内核副本随时可能不一致（用户态在两次内核介入之间改表是合法操作），一致性维护要么加锁要么放弃，两者都比「内核只做裁判、状态权威在用户」贵。其二，设想 `A_INSRT` 失败即终止整批——第 K 条的写回 EFAULT 拖死第 K+1 到最后一条本来合法的消息，一批里一条坏指针的破坏半径从一条扩大到整批；宏注释的「Do NOT set r」把破坏半径钉在单条。

### 2.7 delivermsg 延迟拷贝与 IPCNAME 表

```c
static void delivermsg(struct proc *rp)
{
        assert(!RTS_ISSET(rp, RTS_VMREQUEST));
        assert(rp->p_misc_flags & MF_DELIVERMSG);
        assert(rp->p_delivermsg.m_source != NONE);

        if (copy_msg_to_user(&rp->p_delivermsg,
                                (message *) rp->p_delivermsg_vir)) {
                if(rp->p_misc_flags & MF_MSGFAILED) {
                        /* 2nd consecutive failure means this won't succeed */
                        printf("WARNING wrong user pointer 0x%08lx from "
                                "process %s / %d\n",
                                rp->p_delivermsg_vir,
                                rp->p_name,
                                rp->p_endpoint);
                        cause_sig(rp->p_nr, SIGSEGV);
                } else {
                        /* 1st failure means we have to ask VM to handle it */
                        vm_suspend(rp, rp, rp->p_delivermsg_vir,
                                sizeof(message), VMSTYPE_DELIVERMSG, 1);
                        rp->p_misc_flags |= MF_MSGFAILED;
                }
        } else {
                /* Indicate message has been delivered; address is 'used'. */
                rp->p_delivermsg.m_source = NONE;
                rp->p_misc_flags &= ~(MF_DELIVERMSG|MF_MSGFAILED);

                if(!(rp->p_misc_flags & MF_CONTEXT_SET)) {
                        rp->p_reg.retreg = OK;
                }
        }
}
```

（区间复算：`sed -n '263,294p' minix3/minix/kernel/proc.c | wc -l` = 32。锚点：`minix3/minix/kernel/proc.c:delivermsg`。）

这个函数短得与它的地位不相称：直接交付与通知交付路径把消息放进接收者的**内核槽**（`p_delivermsg`）并置 `MF_DELIVERMSG`，真正把消息写进用户缓冲的动作发生在这里——而它的调用点在调度循环里（[10-switch-to-user.md](10-switch-to-user.md) §2.1 的五标志循环 DELIVERMSG 臂）。交付协议分成两段：第一段（mini_send/mini_notify/mini_receive 的各交付点）只做槽间搬运与置位；第二段（本函数）在接收者被调度选中、即将返回用户态时才把消息拷向用户。

分两段的理由从接收者的处境读出来：第一段发生的时刻，接收者还没上 CPU——它的用户页是否可写，要在**它的地址空间被激活之后**才有意义。第一段若一次拷到底，内核写的是「当前激活地址空间」里的同名虚拟地址，而那块缓冲属于**接收者**的地址空间；跨地址空间写需要专门的机制（建临时映射或走直接映射窗口，归 `22-syscall-copy`/`24-cross-space-runtime` 方向），协议层把这一步留给消费点——接收者自己上 CPU 时，当前地址空间恰好就是它的，一次普通拷贝就对了。

失败策略是两次机会：第一次拷贝失败 → `vm_suspend` 把进程挂给 VM 处理坏页，`MF_MSGFAILED` 记下「失败过一次」；恢复后循环再次走到这里重试，第二次还失败 → 不再挂起，`cause_sig(SIGSEGV)` 直接杀——连续两次坏页说明这个指针不是暂时不可入，而是坏的。成功路径清 `MF_DELIVERMSG` 与 `MF_MSGFAILED`，并在上下文未被内核重写过（`MF_CONTEXT_SET` 未置）时把返回寄存器写成 OK——被通知唤醒的停车 receive 的「返回 OK」在这一刻才落进现场。

通知消息的组装函数与通知类型常量：

```c
#define BuildNotifyMessage(m_ptr, src, dst_ptr) \
	memset((m_ptr), 0, sizeof(*(m_ptr)));				\
	(m_ptr)->m_type = NOTIFY_MESSAGE;				\
	(m_ptr)->m_notify.timestamp = get_monotonic();		\
	switch (src) {							\
	case HARDWARE:							\
		(m_ptr)->m_notify.interrupts =			\
			priv(dst_ptr)->s_int_pending;			\
		priv(dst_ptr)->s_int_pending = 0;			\
		break;							\
	case SYSTEM:							\
		memcpy(&(m_ptr)->m_notify.sigset,			\
			&priv(dst_ptr)->s_sig_pending,			\
			sizeof(sigset_t));				\
		sigemptyset(&priv(dst_ptr)->s_sig_pending);		\
		break;							\
	}

static message m_notify_buff = { 0, NOTIFY_MESSAGE };
```

（区间复算：`sed -n '99,117p' minix3/minix/kernel/proc.c | wc -l` = 19。锚点：`minix3/minix/kernel/proc.c:BuildNotifyMessage`。）

```c
 */
#define NOTIFY_MESSAGE		  0x1000
/* FIXME the old is_notify(a) should be replaced by is_ipc_notify(status). */
#define is_ipc_notify(ipc_status) (IPC_STATUS_CALL(ipc_status) == NOTIFY)
#define is_notify(a)		  ((unsigned) ((a) - NOTIFY_MESSAGE) < 0x100)
```

（区间复算：`sed -n '89,93p' minix3/minix/include/minix/com.h | wc -l` = 5。锚点：`minix3/minix/include/minix/com.h:NOTIFY_MESSAGE`。）

组装规则按来源分派：普通进程来源只填时间戳；HARDWARE 来源额外把接收者特权结构的中断挂起位图整体搬进消息并清零（中断通知的负载就是「哪些中断发生过」）；SYSTEM 来源搬待决信号集并清零。`m_type` 恒为 `NOTIFY_MESSAGE`（0x1000），用户态库以它分辨「这条是通知」——`is_notify` 的判定区间（0x1000 起 0x100 内）与 `is_ipc_notify` 的状态寄存器判定都建在这个常量上。

IPCNAME 是本章最后一个装置，它不属于协议，属于可诊断性：

```c
#define IPCNAME(n) { \
	assert((n) >= 0 && (n) <= IPCNO_HIGHEST); \
	assert(!ipc_call_names[n]);	\
	ipc_call_names[n] = #n; \
}

  arch_post_init();

  IPCNAME(SEND);
  IPCNAME(RECEIVE);
  IPCNAME(SENDREC);
  IPCNAME(NOTIFY);
  IPCNAME(SENDNB);
  IPCNAME(SENDA);
```

（区间复算：`sed -n '277,290p' minix3/minix/kernel/main.c | wc -l` = 14。锚点：`minix3/minix/kernel/main.c:IPCNAME`。）

宏把六个原语的名字字符串填进 `ipc_call_names[]` 名字表，两条 `assert` 保证区间合法与槽位不重复；启动时逐个登记。消费点在 `do_sync_ipc` 的入口检查（§2.1 的节选块可见 `ipc_call_names[call_nr]`）：trap 号越界或无名字 → ETRAPDENIED。名字表把「trap 号 1」翻译成「SEND」，调试输出与权限告警因此可读。调用点在启动期（[08-system-init-boot-finish.md](08-system-init-boot-finish.md) 引过它的登记时机），本篇只讲语义。

### 2.8 本章归纳：原语总表与阻塞行为分类

六章走完，六原语的停与不停收进一张总表（死锁检测的插入点一并标出）：

| 原语 | trap 号 | 何时停 | 停在哪 | 怎么醒 | deadlock 预检 |
|---|---|---|---|---|---|
| SEND | 1 | 目标不在收 | RTS_SENDING + 挂目标 p_caller_q | 接收者扫队列命中（§2.4 第三级） | 停车前，mini_send 内 |
| RECEIVE | 2 | 没有可收的消息 | RTS_RECEIVING + p_getfrom_e | 发送者直接交付或通知交付 | 停车前，mini_receive 内 |
| SENDREC | 3 | 发送半按 SEND 规则停；收一半按 RECEIVE 规则停 | 先 SENDING 后 RECEIVING（两位可同置） | 各半各自的唤醒路径 | 两个半各插一次 |
| NOTIFY | 4 | 永不停 | ——（不在等则记位图） | 接收者下次 receive 的第一级 | 不预检（不停） |
| SENDNB | 5 | 永不停 | —— | —— | 不预检，ENOTREADY 先行 |
| SENDA | 16 | 永不停 | ——（投不出的记 s_asyn_pending 位） | 接收者下次 receive 的第二级（try_async/try_one） | 不预检（不停） |

表里 SENDNB 那一行与 SEND 的真实差别还要再补一条：它不是「不死锁版 SEND」——它连 deadlock 预检都不经过：`mini_send` 的代码序是「非阻塞探测在前、死锁预检在后」（§2.3 函数体内先查 `NON_BLOCKING` 返回 ENOTREADY，才到 `deadlock`），探测发现目标不在就立刻返回，根本走不到预检那一行。SENDNB 的调用者拿到 ENOTREADY 后自己决定何时重试，环等待在它手里不成立，因为它没有等。

表内逐行的反推各收一句：SEND 行若去掉预检，互发环成立且无人报错，两个进程互相停在对方队列上直到永远；RECEIVE 行若去掉预检，互收环同理——这一行在 Rust 侧的对应物恰是现状（§4.1 末段的登记）：receive 向预检未接线，互收环在生产路径静默停车；SENDREC 行的两个半若不共享 `MF_REPLY_PEND` 保护，收一半会被排队的通知顶替（§2.4 的瀑布跳过规则就是这道保护）；NOTIFY 行若允许阻塞，通知的发送者被任意一个忙碌目标拖停，内核代投的轻量语义作废；SENDA 行若整批原子化，一条坏表项的风险半径扩到整批（§2.6 已推）。第 3 章换到 Rust 侧，看这些语义各以什么形状落地。

---

## 3. Rust 设计决策

模块头自己登记了与本篇的对应关系——设计决策集中在 `12-ipc-core.md §3`（即本章），并附 C 源区间清单：

```rust
//! Kernel IPC core module.
//!
//! Implements the six IPC primitives (SEND, RECEIVE, SENDREC, NOTIFY, SENDNB, SENDA)
//! and supporting mechanisms (deadlock detection, sender queues, delayed delivery).
//!
//! **Zero-heap contract**: see `lib.rs` for the kernel-wide contract. This
//! module allocates nothing at runtime — sender queues use an intrusive
//! FIFO through the process-table slots (caller_q_head/tail in target slot,
//! send_q_link in sender slot).
//!
//! # Module Organization
//!
//! - Types: `IpcCall`, `IpcOutcome`, `IpcError`, `SendFlags`,
//!   `IpcEngine`, `DeadlockCycle`, `UserCopy`, `DeliverResult`
//! - Sender wait queue: free functions `caller_q_push` / `caller_q_find`
//!   (shared walk, `accept` predicate plugs in at C's CANRECEIVE position) /
//!   `caller_q_remove` / `caller_q_remove_by_nr` (intrusive FIFO through
//!   the process-table slots — no heap)
//! - SENDA flags: `AMF_*` constants
//!
//! Design decisions are documented in `12-ipc-core.md` §3.
//! C source: `minix3/minix/kernel/proc.c:263-294, 479-597, 599-698, 703-768,
//! 870-962, 967-1117, 1122-1167, 1200-1326, 1331-1346`.
```

（区间复算：`sed -n '1,23p' os/kernel/src/ipc.rs | wc -l` = 23。锚点：`os/kernel/src/ipc.rs:fn delivermsg`（模块内首个公开函数锚点，全模块符号随 §3/§4 各节给出）。）

### 3.1 IpcEngine 与侵入式 FIFO：零堆的消息中转（→§2.3/§2.4）

C 里真实存在的负担是 §2.3 路径二末尾那段二级指针走查：`xpp = &dst_ptr->p_caller_q; while (*xpp) xpp = &(*xpp)->p_q_link; *xpp = caller_ptr;`——链头住在目标槽、链环住在发送者槽，C 用「指向指针的指针」把两种所有权缝在一条循环里。这个形状在 Rust 的借用模型下不能照抄：进程表是一个切片，槽里的 `&mut` 链接字段无法同时持有指向前后的引用而不违反别名规则。承接的形状是**索引化侵入式 FIFO**——链环不用指针，用 `Option<ProcNr>` 槽号；队列头尾两个字段放在目标槽，单个链接字段放在发送者槽，与 C 的字段布局一一对应（`os/kernel/src/ipc.rs` 的 `caller_q_head`/`caller_q_tail`/`send_q_link`，声明于进程槽结构）。四个自由函数经办这条链：

```rust
/// Find the first queued sender on `dst_idx`'s queue matching
/// `src_endpoint` AND satisfying `accept` (head-first scan). Returns the
/// sender's slot index. This is the single walk implementation for the
/// caller queue — production (`IpcEngine::caller_q_find_allowed`, which
/// passes the filter check as `accept`) and tests share it.
///
/// C: `while (*xpp) { if (CANRECEIVE(...)) break; }` — proc.c:1077-1105.
/// `accept` plugs in at exactly the `CANRECEIVE` position of the C loop
/// (in C the filter check lives inside `CANRECEIVE`; in Rust D-16 splits
/// it into `can_receive`). `Endpoint::ANY` matches the head (C: first
/// queue entry).
pub(crate) fn caller_q_find(
    procs: &[KProcess],
    dst_idx: usize,
    src_endpoint: Endpoint,
    mut accept: impl FnMut(usize) -> bool,
) -> Option<usize> {
    let mut cur = procs[dst_idx].caller_q_head;
    while let Some(nr) = cur {
        let idx = nr_to_idx(nr)?;
        let endpoint_match = src_endpoint == Endpoint::ANY || procs[idx].p_endpoint == src_endpoint;
        if endpoint_match && accept(idx) {
            return Some(idx);
        }
        cur = procs[idx].send_q_link;
    }
    None
}
```

（区间复算：`sed -n '879,906p' os/kernel/src/ipc.rs | wc -l` = 28。锚点：`os/kernel/src/ipc.rs:fn caller_q_find`。）

`caller_q_find` 的签名里有一个 C 侧没有直接对应的参数：`accept` 谓词。它插的位置，正是 C 队列扫描里 `CANRECEIVE` 宏所在的位置——C 的 `mini_receive` 第三级一边走链一边逐个验「这条消息接收者现在收不收得」（§2.4 引文中的 `while (*xpp) { if (CANRECEIVE(...)) ... }` 段），谓词与走查是同一段循环；Rust 把走查抽成共享实现，把「收不收得」做成注入的闭包：生产路径传过滤判定（`os/kernel/src/ipc.rs:fn caller_q_find_allowed`，把接收者的过滤链判定作为 accept 传入），测试传恒真闭包。一处走查、两种谓词——C 的入队走尾与命中扫描是两处各自手写的循环（`mini_send` 的链尾走查、`mini_receive` 的命中扫描），Rust 收成 push/find/remove 三个共享函数。

```rust
/// Design decision §3.3 / ARCH-3: NOT a ZST namespace — holds `&mut [KProcess]`.
///
/// C: free functions (`mini_send`, `mini_receive`, ...) taking `struct proc*`
/// on every call. Rust: `IpcEngine` holds the borrow once, methods don't
/// repeat it — eliminating the per-call `&mut [KProcess]` parameter.
///
/// # BKL (Big Kernel Lock)
///
/// **Precondition**: every method requires the caller to hold the BKL.
/// The `&mut [KProcess]` borrow is the Rust-level proxy for "BKL held":
/// it statically prevents aliasing mutable access across CPUs, matching
/// C's `big_kernel_lock` spinlock guarantee that only one CPU at a time
/// may mutate the process table.
///
/// # Lifetime
///
/// `'a` ties the engine to the borrow of `procs` / `priv_table` /
/// `user_copy`. The engine is short-lived (constructed per syscall
/// dispatch, dropped when the syscall returns).
pub struct IpcEngine<'a> {
    /// Process table slice (mutable — IPC modifies RTS flags, queues, etc.).
    procs: &'a mut [KProcess],
    /// Privilege table (mutable — notify updates `s_notify_pending` bitmap).
    priv_table: &'a mut PrivTable,
    /// User-space copy abstraction (arch-specific impl injected).
    user_copy: &'a dyn UserCopy,
    /// IPC filter pool (D-16, optional — `None` = no filtering available
    /// and every message is allowed, the pre-D-16 behavior; production
    /// dispatch wires the global `IPC_FILTER_POOL` via
    /// [`Self::with_filter_pool`]).
    filter_pool: Option<&'a crate::ipc_filter::IpcFilterPool>,
    /// Sender whose message was just delivered while `MF_SIG_DELAY` was set.
    ///
    /// C: proc.c:1082-1083 — `if (sender->p_misc_flags & MF_SIG_DELAY)
    /// sig_delay_done(sender)`. The delay-end notification
    /// (`sig_delay_done` → `cause_sig(SIGSNDELAY)`) must run at the
    /// `ProcessTable` level because `cause_signal` sets `RTS_SIGNALED`
    /// through the scheduler-aware `rts_set` (dequeue). The engine only has
    /// a slice, so it records the sender here and the `ProcessTable`-level
    /// dispatcher (`dispatch_ipc`) completes the protocol after `do_ipc`
    /// returns. At most one sender is delivered per IPC operation.
    sig_delay_sender: Option<ProcNr>,
```

（区间复算：`sed -n '981,1022p' os/kernel/src/ipc.rs | wc -l` = 42。本块为节选（981-1022 行连续未删节），截断于引擎最后一个字段 `wake_targets` 的文档之前——该字段是 §3.5 差异登记的主题，叙述在后面。锚点：`os/kernel/src/ipc.rs:impl IpcEngine`。）

`IpcEngine` 把 C 的自由函数形态（`mini_send(caller_ptr, ...)` 每次都带进程表指针）收成一个持有借用的引擎：进程表切片、特权表、用户拷贝实现、过滤池四个引用在构造时接好，方法不再重复参数。`&mut [KProcess]` 借用同时是 BKL 持有的类型级见证——同一时刻只有一个调用者能可变借用进程表，与 C 的「持大内核锁才能动进程表」在同一条纪律上（BKL 取得点与持锁约定归 [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.2）。

零堆契约在这条链上兑现为三条性质：其一，入队只写已经存在于槽内的 `Option<ProcNr>` 字段，运行期零分配；其二，入队不会失败——每个进程至多阻塞在一个发送上（`RTS_SENDING` 停的是整个进程），所以每个槽至多出现在一条链上，总量以进程数为上界，没有溢出分支；其三，C 的 O(n) 走尾在 Rust 侧用显式尾指针换成 O(1) 追加，FIFO 次序不变。取舍如下：

| 选项 | 否掉什么 | 代价 |
|---|---|---|
| 侵入式 FIFO（选定） | ——（C 形状的直系继承） | 摘链需走查找前驱（C 的 find+unlink 一趟并两步，Rust 拆 find/remove 两趟，均为 O(n)，n 以进程表为上界） |
| `VecDeque<ProcNr>` 队列 | —— | 运行期分配违反内核零堆存储模型；分配失败路径在内核无处可去 |
| 双向链表槽间互链 | —— | 每槽两个链接字段，槽体积换一次 O(1) 摘链；单链足矣 |

反向推演：设想用 `Vec` 承接等待队列——每次入队可能触发堆分配，而内核从启动到关机没有堆可用（存储模型归 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.0）；就算给队列预分配容量，进程阻塞在发送上的数目以进程表为上界，静态数组方案回退一步就是「每目标一个定容数组」，容量上界同样要人给。进程槽内嵌链接是零堆前提下等价于 C 的形态，不是偏好问题：分配能力不存在，能住的只有已经分好的槽。

### 3.2 detect_deadlock：从整数到结构化返回（→§2.2）

C 里真实存在的语义是 §2.2 结尾留下的那条线：`deadlock` 返回 `group_size` 整数，零表示无死锁，非零表示环的规模——而这个返回值在全部调用点只当布尔用（`if (deadlock(...)) return ELOCKED;`），环里有谁、朝哪个方向转，整数一概没说；唯一真正消费成员清单的地方是调试打印（`DEBUG_ENABLE_IPC_WARNINGS` 块里的 `processes[]` 数组）。Rust 的对应物把「发生过死锁」从返回码升级成返回值：

```rust
/// Deadlock cycle descriptor.
///
/// C: `deadlock()` — proc.c:703-768. Returns `Some` when a cyclic
/// dependency is found along the `P_BLOCKEDON` chain.
///
/// `direction` records which IPC call type produced the cycle so the
/// caller can apply the 2-cycle SEND↔RECEIVE exception (which is NOT a
/// deadlock — it is the normal request/reply pattern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockCycle {
    /// Process numbers (in walk order) forming the cycle, starting at the caller.
    /// Maximum chain length is bounded by PROC_TABLE_SIZE.
    pub chain: [ProcNr; PROC_TABLE_SIZE],
    /// Number of valid entries in `chain`.
    pub chain_len: usize,
    /// Direction that produced the cycle. C: corresponds to `function`
    /// passed to `deadlock()` (SEND or RECEIVE).
    pub direction: DeadlockDirection,
    /// Number of processes in the cycle. C: `group_size` return value.
    /// `2` triggers the SEND↔RECEIVE check at the caller.
    pub group_size: usize,
}

/// Deadlock direction.
///
/// Kept distinct from `IpcCall` to prevent the deadlock detector from
/// being semantically coupled with the IPC dispatcher: `IpcCall` is the
/// user-facing API call number, `DeadlockDirection` is the engine's
/// detected wait direction. The two are 1:1 today but the type boundary
/// keeps future extensions (e.g. a third wait direction) local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockDirection {
    /// SEND-direction cycle (caller is SENDING, waiting for receiver).
    /// C: `deadlock(SEND, ...)` path.
    Send,
    /// RECEIVE-direction cycle (caller is RECEIVING, waiting for sender).
    /// C: `deadlock(RECEIVE, ...)` path.
    Receive,
}
```

（区间复算：`sed -n '241,279p' os/kernel/src/ipc.rs | wc -l` = 39。锚点：`os/kernel/src/ipc.rs:struct DeadlockCycle`、`:enum DeadlockDirection`。）

`Some(DeadlockCycle)` 携带方向与成员链，`None` 对应 C 的返回 0；XOR 豁免的移植体在走查里逐行对（§4.2）。方向单独建一个枚举而不复用 `IpcCall`，理由写在文档注释里：检测器的「等待方向」与用户的「调用号」今天是 1:1，但它们是两个概念——把类型分开，将来出现第三种等待方向时改动只落在检测器内部。取舍：

| 选项 | 否掉什么 | 代价 |
|---|---|---|
| 返回 `Option<DeadlockCycle>`（选定） | C 的整数编码 | 成员链需要调用方持有（栈上定宽数组，长度以进程表为上界） |
| 沿用 C 的 group_size 整数 | —— | 调用方无从区分环成员，诊断面只能再走一遍链 |
| 返回 `Result<(), DeadlockCycle>` | —— | 「有环」在这个调用点上不是可恢复错误而是拒绝理由，Option 的语义刚好 |

反向推演：C 形状直译——调用方拿到 `group_size` 之后若要打印环成员（诊断一个 ELOCKED 误报时的第一件事），只能以调用者参数为起点把 `P_BLOCKEDON` 链重新走一遍，而那一刻链可能已经被别的 CPU 改动（SMP 下的检测与重走之间没有一致性）。结构化返回把检测时刻的现场冻结在值里。

### 3.3 send/receive/notify 的形状差异：IpcOutcome 与 IPC 状态编码（→§2.1/§2.3/§2.4）

C 里真实存在的语义是 §2.3 路径二的出口：阻塞排队的 SEND 返回 `OK`——「消息已送出」与「调用者已停进队列」共用同一个返回值，差别只活在副作用里（返回值是 OK、但 `RTS_SENDING` 已置、人已在队列上）。调用方（调度循环的 IPC 臂）要知道「停了」，得回头再查进程的标志位。Rust 把这次调用的三种结局收进一个枚举：

```rust
/// IPC operation outcome. Replaces C's errno return pattern.
///
/// Design decision §3.1 / AT-1: distinguish "blocked (normal)" from
/// "error (exception)". C returns a single `OK` for both "message
/// delivered" and "caller is now blocked (RTS_SENDING set)" — the
/// dispatcher infers which from side effects. Rust makes the two states
/// explicit so `switch_to_user` can match without inspecting RTS flags.
///
/// Mapping:
/// - C `OK` + (dst woken or caller enqueued) → `Delivered` or `Blocked`
/// - C `ELOCKED`/`ENOTREADY`/`EDEADSRCDST`/`EFAULT`/`ECALLDENIED`/`ETRAPDENIED`
///   → `Error(IpcError)`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcOutcome {
    /// Message delivered to target (or notification recorded in bitmap).
    /// C: `return OK` from `mini_send`/`mini_receive`/`mini_notify` when
    /// the destination was in RECEIVE and the message was copied to
    /// `p_delivermsg`.
    Delivered,
    /// Caller is now blocked (`RTS_SENDING` or `RTS_RECEIVING` set).
    /// C: implicit — `mini_send` returns `OK` but caller is enqueued on
    /// `p_caller_q`. Rust surfaces this as a distinct state so the
    /// scheduler knows to switch without re-checking RTS flags.
    Blocked,
    /// IPC failed with error. The caller remains runnable.
    Error(IpcError),
}
```

（区间复算：`sed -n '133,159p' os/kernel/src/ipc.rs | wc -l` = 27。锚点：`os/kernel/src/ipc.rs:enum IpcOutcome`。）

`Blocked` 是合法结果而不是错误——它就是 SEND/RECEIVE 两条阻塞规则本身的兑现（§1.1 的规则一与规则二）。把它塞进 `Result::Err` 的反推：调度循环的每个 IPC 调用点都要先排除「假错误」才能继续——排错的代码与判断停车的代码搅在一起，而「调用者现在停着」根本不是异常。C 用「返回值 + 调用方状态」两个通道表达同一件事，Rust 用一个三分枚举把两个通道并成一个。

发送形态标志同样收进类型：

```rust
// IPC send flags. C: `minix3/minix/kernel/ipc.h:11-12`.
//
// Design decision §3.4 / AT-4: `bitflags!` macro (not bare `u32`).
//
// # P0 FIX (FIX-1 / FIX-2)
//
// Previous code defined `NON_BLOCKING=0x01` and `FROM_KERNEL=0x02`,
// both wrong. The correct values aligned with C source are
// `NON_BLOCKING=0x0080` and `FROM_KERNEL=0x0100`.
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SendFlags: u32 {
        /// Non-blocking mode. C: `NON_BLOCKING=0x0080` — ipc.h:11
        const NON_BLOCKING = 0x0080;
        /// Message from kernel. C: `FROM_KERNEL=0x0100` — ipc.h:12
        const FROM_KERNEL = 0x0100;
        /// Internal: this send originates from SENDA (batch async send).
        /// Not a C flag — Rust uses it to set the correct IPC status call
        /// type (SENDA instead of SEND) when `send` is reused by `senda`.
        /// C's `mini_senda` has its own delivery path; Rust reuses `send`.
        const SENDA = 0x0001;
    }
}

impl SendFlags {
    /// Empty flags (blocking send from user space).
    pub const NONE: Self = Self::empty();
}
```

（区间复算：`sed -n '212,239p' os/kernel/src/ipc.rs | wc -l` = 28。锚点：`os/kernel/src/ipc.rs:struct SendFlags`（宏派生项）。块内注释含仓库内部阶段编号，按原文引用。）

`NON_BLOCKING`/`FROM_KERNEL` 两个位与 C 的 ipc.h 定义同值（0x0080/0x0100，§2.3 的引文可对）；第三个位 `SENDA` 是 Rust 侧新增的内部标记——C 的 `mini_senda` 有自己的交付路径，Rust 复用 `send` 办 SENDA 的立即交付时用它把 IPC 状态的 call 类型记成 SENDA 而不是 SEND（§4.3 消费）。交付时写进接收者状态寄存器的编码与 C 的 IPC 状态宏族对位：call 类型走低 6 位、标志走 16 位以上（`os/kernel/src/ipc.rs:const IPC_STATUS_CALL_SHIFT`、`:const IPC_STATUS_FLAGS_SHIFT`、`:const IPC_FLG_MSG_FROM_KERNEL`，编码函数 `os/kernel/src/ipc.rs:fn ipc_status_call_to` 与 `:fn ipc_status_flags`）。

取舍：

| 选项 | 否掉什么 | 代价 |
|---|---|---|
| `IpcOutcome` 三分（选定） | C 的「返回值+副作用」双通道 | 枚举多一个变体要维护（`Delivered` 与 `Blocked` 的分界在两路径交界处要判准，§4.1 的 sendrec 落点可见） |
| `Result<IpcOutcome, IpcError>` 把 Blocked 并进 Err | —— | 调用方每个 IPC 调用点都要排除假错误（§1.1 的阻塞规则是常态而非异常） |
| 裸布尔 + 查标志位（C 形状直译） | —— | 「停了没有」的判断散回每个调用点，与 §2.4 其二那条「返回值早已写好」的坑同源 |

### 3.4 delivermsg 与拷贝边界：KernelUserCopy 与仓库内比喻词的受控引注（→§2.7）

C 里真实存在的负担是 §2.7 那两段交付依赖的拷贝原语：`copy_msg_to_user`/`copy_msg_from_user` 直接对用户指针解引用，坏指针靠陷阱重定向兜底（异常入口把出错地址折回拷贝函数的失败分支）。这套机制绑死在 i386 的段级保护与异常表上；换成页表式保护的三架构，等价机制不存在现成对应物。Rust 把「内核 ↔ 用户消息拷贝」收成一个边界 trait，内核 IPC 层只依赖抽象，页表细节留给各架构实现：

```rust
pub trait UserCopy {
    /// Copy a `Message` from user space. C: `copy_msg_from_user` —
    /// proc.c:901, 936. Returns `Err(CopyError::PageFault)` on page
    /// fault, `Err(CopyError::OutOfBounds)` if the address is outside
    /// user space.
    fn copy_msg_from_user(&self, src: VirBytes) -> Result<Message, CopyError>;

    /// Copy a `Message` to user space. C: `copy_msg_to_user` — proc.c:185.
    /// Same error semantics as `copy_msg_from_user`.
    fn copy_msg_to_user(&self, dst: VirBytes, msg: &Message) -> Result<(), CopyError>;
```

（区间复算：`sed -n '294,303p' os/kernel/src/ipc.rs | wc -l` = 10。本块为节选（294-303 行连续未删节），省略号处为 trait 文档头部的登记段与 `read_senda_entry`/`write_senda_result` 两个 SENDA 表项方法（§4.3 消费）。锚点：`os/kernel/src/ipc.rs:trait UserCopy`。）

边界线画在两处。第一处是**先验证后触碰**：拷贝路径先做用户半定界（上界常量 `os/kernel/src/ipc.rs` 引 `minix_types::USER_VA_LIMIT`）与逐页翻译校验（每页必须用户可访问、写方向还要可写），确认全部通过才动内存——坏指针在这里变成 `Err`，不再有内核态陷阱。第二处是**访问通道**：即便校验通过，字节搬运也不直接解引用用户虚拟地址，而是逐页翻译成物理地址后经直接映射窗口读写。

```rust
            let page = dv.0 as *mut u8;
            // 只保留一次可变再借用（commit 后评审 P1：同时构造
            // as_ptr/as_mut_ptr 按 Stacked Borrows 会使前者标签失效）；
            // 可变指针可作 *const 读源，两臂共用 base。
            let base = buf.as_mut_ptr().add(done as usize);
            // NK4-C 1.12e：方向按 doc 契约——to_kernel=true 把 buf 写入
            // 用户页，false 把用户页读进 buf。此前两臂互换：读腿
            // （read_senda_entry/write_senda_result 的 head 预读传 false）
            // 实际把清零缓冲写进用户槽位（A_RETR 变破坏性写），flags
            // 恒解出 0=AMF_EMPTY → senda 一条不投（s18 系列全零签名）。
            if to_kernel {
                core::ptr::copy_nonoverlapping(base, page, chunk);
            } else {
                core::ptr::copy_nonoverlapping(page as *const u8, base, chunk);
            }
        }
```

（区间复算：`sed -n '493,508p' os/kernel/src/ipc.rs | wc -l` = 16。逐字引用，注释按原文保留。注：「腿」为本仓自造比喻（台账登记受控），对应通行说法的发送半/接收半/路径段；本块引文内出现的该词按原文引用、叙述层一律用「读方向/写方向」。锚点：`os/kernel/src/ipc.rs:fn copy_via_root_pages`。）

这段注释记录的正是边界纪律的一次失守与修复：读方向与写方向两个分支曾在实现里互换，读方向的调用把清零缓冲写进了用户槽位——方向参数与语义的对应关系靠测试与真机签名钉住。`KernelUserCopy` 本身是宿主测试与启动早期的桩实现（`os/kernel/src/ipc.rs:struct KernelUserCopy`），生产注入由各架构提供。取舍：

| 选项 | 否掉什么 | 代价 |
|---|---|---|
| trait + 先验证后触碰 + 直接映射通道（选定） | C 的陷阱重定向依赖 | 每页一次页表走查，比直接解引用慢一个常数因子 |
| 先拷后查 | —— | 写穿内核的形状：坏指针指向内核半区时，校验来不及拦，内核内存已被写（先验证后触碰的契约由 `user_copy_range_mapped` 的文档注释登记，`os/kernel/src/ipc.rs` 原文可对） |
| 每个 IPC 函数自带拷贝代码 | —— | 三架构 × 两方向 × 若干调用点的组合爆炸，边界规则无一处单一权威 |

反向推演先拷后查：拷贝动作本身在坏指针上触发页错误，此刻内核已经在写目标地址——若目标落在内核半区，写穿已经发生，事后的错误处理管不住已写入的字节。先验证把「地址可不可信」的判定挪到第一次触碰之前，验证失败连一个字节都不动。

### 3.5 处置表（→Ch2 各节）

C 的 IPC 面逐项去向（档位：保留=语义与形状同迁；改型=语义保留、形状重造；差异=Rust 与 C 行为不同，单独占行登记，不并入保留；新增=Rust 侧才有）：

| C 项 | Rust 对应物 | 档位 | 落点 |
|---|---|---|---|
| `do_ipc` 六原语分派 | `os/kernel/src/ipc.rs:fn do_ipc` | 保留 | §4.1 |
| kbill_ipc 记账 + ipc_sync/ipc_async 计数 | `os/kernel/src/globals.rs`（kbill_ipc 对位）+ `os/kernel/src/proc.rs:struct Accounting` | 保留 | §4.1 |
| `do_sync_ipc` 三道权限门 | `os/kernel/src/ipc.rs:fn check_ipc_permission` | 保留 | §4.1 |
| 跟踪预占的 `p_defer` 三寄存器 | 进程槽 `p_defer` 字段保留，int-33 分派层用同一字段做参数暂存 | 改型 | §4.1 |
| `deadlock` + `P_BLOCKEDON` | `os/kernel/src/ipc.rs:fn detect_deadlock` + `KProcess::blocked_on` | 改型（整数→结构化） | §3.2、§4.2 |
| `mini_send` 两路径 | `os/kernel/src/ipc.rs:fn send` | 保留 | §4.1 |
| `mini_receive` 三级匹配序 | `os/kernel/src/ipc.rs:fn receive` 的四个 Phase | 保留（分级命名变体：C 的瀑布包在 SENDING 门外，Rust 同形） | §4.1 |
| `mini_notify` 两分支 | `os/kernel/src/ipc.rs:fn mini_notify_core` | 保留 | §4.3 |
| `try_deliver_senda`/`try_async`/`try_one` | `os/kernel/src/ipc.rs:fn senda` + `:fn deliver_async` | 保留（无内核侧表缓存） | §4.3 |
| `delivermsg` 两段交付 + 两次失败策略 | `os/kernel/src/ipc.rs:fn delivermsg` + `:enum DeliverResult` | 保留 | §4.3 |
| `BuildNotifyMessage` + `NOTIFY_MESSAGE` 0x1000 | `os/kernel/src/ipc.rs:impl IpcEngine::build_notify_message` + `:const NOTIFY_MESSAGE` | 保留 | §4.3 |
| `IPCNAME` 名字表 | **无对位**（如实披露，§4.4） | 淘汰（现码无对位） | §4.4 |
| KERNINFO 查询臂 | `os/kernel/src/syscall.rs:fn dispatch_ipc` 的 KERNINFO 分支 + `os/kernel/src/kerninfo.rs` | 保留（载体寄存器 i386 bx → x86-64 R10） | §4.4 |
| `IPC_STATUS_ADD_*` 状态编码 | `os/kernel/src/ipc.rs:fn ipc_status_call_to` 族 | 保留 | §3.3 |
| sender 队列 `p_caller_q`/`p_q_link` | `caller_q_head`/`caller_q_tail`/`send_q_link`（索引化） | 改型 | §3.1 |

差异登记（每行单独成条）：

1. **receive 入口的已沉降交付**。C 的 `mini_receive` 入口断言 `MF_DELIVERMSG` 未置（§2.4 引文首行 assert）——C 的模型里接收者被唤醒后走调度循环的 delivermsg 臂消费交付，不会再进 `mini_receive`。Rust 的 `receive` 入口先消费已沉降的交付（Phase 0）：引擎模型里接收者自己陷入内完成交付的路径存在（§4.1），带沉降再入时把沉降消费掉而不是断言失败。
2. **ELOCKED 尺寸超限的错误码**。C 的 SENDA 尺寸超限返回 EDOM（§2.1 分派段）；Rust 的 `IpcError` 无 EDOM 变体，`do_ipc` 与 `senda` 两处尺寸检查都映射到 `BadCall`（os/kernel/src/ipc.rs 的 `do_ipc`/`senda` 尺寸臂）。
3. **无特权结构调用者的 NOTIFY**。C 的 `mini_notify` 无条件 `priv(caller)->s_id` 取位编号（§2.5）；Rust 对无 `priv_id` 的调用者静默丢弃通知、对位号不在 64 位内的调用者同样丢弃（`os/kernel/src/ipc.rs:fn mini_notify_core` 尾段）。C 的特权结构人人有、位图宽 `NR_SYS_PROCS`=64，两条丢弃路径在 C 不存在。
4. **唤醒的原子性拆分**。C 的 `RTS_UNSET` 宏在清位现场立即入队（[11-scheduling-primitives.md](11-scheduling-primitives.md) §1.1 的跃迁检测）；Rust 引擎持有的是切片借用、够不着就绪队列，清位后把被唤醒者记进 `wake_targets`（4 槽记录），由分派层在 `do_ipc` 返回后统一补入队。单次 IPC 至多唤醒两个进程（sendrec 的两半），4 槽是防御性容量。
5. **死锁返回的形状**。C 的 `group_size` 整数（仅调试消费）→ `DeadlockCycle` 结构化（§3.2 已立，表内引用）。

第 4 章逐函数落地，每个实现回指本章的对应决策。

---

## 4. 实现详解

### 4.1 IpcEngine send/receive 走查（→§3.1/§3.3）

send 的两条路径承接 §2.3 的两路径图，实现对应 §3.1 的侵入式 FIFO 与 §3.3 的三分结果。先看路径一（直接交付）：

```rust
        // Phase 2: WILLRECEIVE check (path A — direct delivery).
        // D-16: C 的 WILLRECEIVE 含 CANRECEIVE（ipc.h:19-22）——被过滤的
        // 消息不投递，发送方落入 Path B（阻塞/排队，proc.c:895→925+），
        // 即被过滤的发送者在队列中等待，与"未命中 receive"同形。
        if Self::is_willing_to_receive(&self.procs[dst_idx], caller_endpoint) {
            // C: `copy_msg_from_user` (user path) or direct copy (FROM_KERNEL).
            let m = if !flags.contains(SendFlags::FROM_KERNEL) {
                // User-origin send: route through UserCopy trait.
                // C: proc.c:901-906.
                let user_src = self.procs[caller_idx].p_delivermsg_vir;
                match self.user_copy.copy_msg_from_user(user_src) {
                    Ok(m) => m,
                    Err(_) => return IpcOutcome::Error(IpcError::Fault),
                }
            } else {
                *msg
            };
            if !self.can_receive(dst_idx, caller_endpoint, m.m_type) {
                // Filtered → fall through to Path B (block/queue).
            } else {
            self.procs[dst_idx].p_delivermsg = m;
            self.procs[dst_idx].p_delivermsg.m_source = caller_endpoint;
            self.procs[dst_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
            if flags.contains(SendFlags::FROM_KERNEL) {
                self.procs[dst_idx].p_misc_flags.set(MiscFlagsBits::SENDING_FROM_KERNEL);
                // C: proc.c:905 — `IPC_STATUS_ADD_FLAGS(dst_ptr, IPC_FLG_MSG_FROM_KERNEL)`
                crate::proc::ipc_status_add_flags(&mut self.procs[dst_idx], IPC_FLG_MSG_FROM_KERNEL);
            }
            // C: proc.c:911-913 — determine call type and add to IPC status.
            //   call = (caller_ptr->p_misc_flags & MF_REPLY_PEND ? SENDREC
            //       : (flags & NON_BLOCKING ? SENDNB : SEND));
            //   IPC_STATUS_ADD_CALL(dst_ptr, call)
            //
            // Rust extension: SENDA flag overrides to IpcCall::SendA, matching
            // C's `mini_senda` direct delivery (proc.c:1287 sets SENDA, not SEND).
            let delivered_call = if flags.contains(SendFlags::SENDA) {
                IpcCall::SendA
            } else if self.procs[caller_idx].p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND) {
                IpcCall::SendRec
            } else if flags.contains(SendFlags::NON_BLOCKING) {
                IpcCall::SendNb
            } else {
                IpcCall::Send
            };
            crate::proc::ipc_status_add_call(&mut self.procs[dst_idx], delivered_call);
            // C: `RTS_UNSET(dst, RTS_RECEIVING)` — wake up target.
            self.procs[dst_idx].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
            let woken = self.procs[dst_idx].p_nr;
            self.record_wake_target(woken);
            // E1 slice 2: the woken receiver's RECEIVE completes with OK
            // (same completion-path return-code rule as the sender wake).
            crate::proc::set_ipc_return_code(&mut self.procs[dst_idx], OK as i64);
            // NK4-C B12: Path A delivery completing a sendrec's receive half.
            // In C, `mini_sendrec` ends with `MF_CLREPLYPRIV(pr)` (proc.c) that
            // clears MF_REPLY_PEND unconditionally at sendrec completion.  In the
            // Rust async model, the sendrec's receive half completes exactly here
            // when the reply is delivered via Path A to a parked receiver.  Without
            // this clear, a stale REPLY_PEND causes VM drain to mis-park the
            // process on its next page fault (forward_pagefault_to_vm → Path B →
            // VM receive → drain sees REPLY_PEND → parks RECEIVING+getfrom=VM
            // forever).
            self.procs[dst_idx].p_misc_flags.clear(MiscFlagsBits::REPLY_PEND);
            return IpcOutcome::Delivered;
            }
        }
```

（区间复算：`sed -n '1332,1396p' os/kernel/src/ipc.rs | wc -l` = 65。锚点：`os/kernel/src/ipc.rs:fn send`。块内注释含仓库内部阶段编号与 C 行号引用，按原文引用。）

逐段对 C：`is_willing_to_receive` 是 `WILLRECEIVE` 宏的直译（目标在收、不在发、来源端点匹配——`os/kernel/src/ipc.rs:fn is_willing_to_receive`；过滤半边由 §3.1 的谓词机制另注入，被过滤的发送落进路径二排队，与 C 的 CANRECEIVE 未命中同形）。消息来源按 `FROM_KERNEL` 分流：用户来源走 §3.4 的 trait 拷贝，内核来源直接槽间赋值并补状态标志。`IPC_STATUS` 的 call 类型判定与 C 的三元式逐字对位（SENDREC/SENDNB/SEND 三选一），`SENDA` 内部位优先（§3.3 已述）。交付落点 `p_delivermsg` + `m_source` + `MF_DELIVERMSG` 与 §2.3 路径一逐项同；唤醒目标时先清 `RTS_RECEIVING`，再把被唤醒者记进 `wake_targets`（§3.5 差异 4 的拆分点），并就地写返回码 OK。

块内注释还记录了一个 Rust 侧动作：sendrec 的收一半恰以直接交付完成时，清掉交付目标（即 sendrec 调用者自己）残留的 `MF_REPLY_PEND`。注释把 C 的对应动作说成「sendrec 完成处统一做（MF_CLREPLYPRIV）」——这句对本仓 C 真值源不成立：`MF_CLREPLYPRIV` 与独立的 `mini_sendrec` 函数在 `minix3/` 树检索零命中（sendrec 内联于 `do_sync_ipc`），C 的同一清除就在 `mini_send` 直接交付现场（§2.3 引文的 `if (dst_ptr->p_misc_flags & MF_REPLY_PEND) ... &= ~MF_REPLY_PEND` 两行）。所以这不是差异，是同形移植；引文内的这句注辞按原文保留、以现码为准（漏清的后果——VM 排水误停车——注释里有真机记录，方向仍是真实的）。

路径二（阻塞排队）：

```rust
        // Path B: caller must block.
        // C: `if (flags & NON_BLOCKING) return ENOTREADY;` — proc.c:925-927.
        if flags.contains(SendFlags::NON_BLOCKING) {
            return IpcOutcome::Error(IpcError::NotReady);
        }

        // C: `if (deadlock(SEND, caller, dst_e)) return ELOCKED;` — proc.c:930-932.
        if self.detect_deadlock(IpcCall::Send, caller_nr, dst_endpoint).is_some() {
            return IpcOutcome::Error(IpcError::Deadlock);
        }

        // Cache message + set RTS_SENDING + enqueue.
        // C: proc.c:938-960.
        if !flags.contains(SendFlags::FROM_KERNEL) {
            let user_src = self.procs[caller_idx].p_delivermsg_vir;
            match self.user_copy.copy_msg_from_user(user_src) {
                Ok(m) => self.procs[caller_idx].p_sendmsg = m,
                Err(_) => return IpcOutcome::Error(IpcError::Fault),
            }
        } else {
            self.procs[caller_idx].p_sendmsg = *msg;
            self.procs[caller_idx].p_misc_flags.set(MiscFlagsBits::SENDING_FROM_KERNEL);
        }
        self.procs[caller_idx].p_rts_flags.set(RtsFlagsBits::SENDING);
        self.procs[caller_idx].p_sendto_e = dst_endpoint;

        caller_q_push(self.procs, dst_idx, caller_idx);
        IpcOutcome::Blocked
    }
```

（区间复算：`sed -n '1398,1426p' os/kernel/src/ipc.rs | wc -l` = 29。锚点：`os/kernel/src/ipc.rs:fn send`。）

四步与 §2.3 路径二逐项对应：非阻塞探测先于死锁预检（`IpcCall::Send` 传给 `detect_deadlock`，对应 C 的 `deadlock(SEND, ...)`）；消息先拷进**发送者自己**的 `p_sendmsg`（内核副本锚，§2.3 的非显然事实同款）；置 `RTS_SENDING` + `p_sendto_e`；`caller_q_push` 挂目标链尾（§3.1 的 O(1) 追加）。返回 `IpcOutcome::Blocked`——§3.3 的合法结果。

receive 的走查从两道门前置开始。第一道是 §2.4 其一的 SENDING 门（SENDREC 发送半还停着时跳过全部匹配级）：

```rust
        if self.procs[caller_idx]
            .p_rts_flags
            .is_set(RtsFlagsBits::SENDING)
        {
            // NK4-C 1.48 取证探针（c39，task1-close 裁决删除）：INIT 经
            // SENDING 门停车（B24 receive 半）的现场：src + pdmv。
            #[cfg(not(feature = "mock"))]
            if self.procs[caller_idx].p_endpoint.0 == 0xb {
            }
            self.procs[caller_idx].p_getfrom_e = src_endpoint;
            self.procs[caller_idx]
                .p_rts_flags
                .set(RtsFlagsBits::RECEIVING);
            return IpcOutcome::Blocked;
        }
```

（区间复算：`sed -n '1485,1499p' os/kernel/src/ipc.rs | wc -l` = 15。锚点：`os/kernel/src/ipc.rs:fn receive`。块内注释含仓库内部阶段编号，按原文引用。）

第二道是 §3.5 差异 1 的消费点：入口若已有沉降交付（直接交付路径抢在接收者陷入之前把消息放进槽里并清了可收位），本次 receive 把它消费掉、清 `MF_REPLY_PEND` 残留、返回 `Delivered`——C 模型没有这条路径（入口断言沉降不存在），Rust 模型必须收下它。

三级匹配的主体依次是：Phase 1 通知（`os/kernel/src/ipc.rs:fn pick_allowed_notify` 按特权 id 低位到高位扫位图，被过滤的候选跳过且位保留，命中即组装+同步交付+记状态）；Phase 2 异步遗留（`os/kernel/src/ipc.rs:fn take_pending_async` 取位、`os/kernel/src/ipc.rs:fn deliver_async` 兑现表项，§4.3 细走）；Phase 3 同步队列：

```rust
        if let Some(sender_idx) = self.caller_q_find_allowed(caller_idx, src_endpoint) {
            caller_q_remove(self.procs, caller_idx, sender_idx);
            // NK4-C 1.11d（getuid 无回执根因）：同步拷贝前必须盖 m_source。
            // C proc.c:1071-1075 把 sender->p_sendmsg 拷进 p_delivermsg 后
            // `p_delivermsg.m_source = sender->p_endpoint`——发送者的用户
            // 缓冲里 m_source 恒为 0（用户态不填，内核投递时盖章）。F14
            // 同步拷贝绕过 p_delivermsg 直写用户缓冲时漏了这一步：PM 收到
            // init 的 GETUID 但 m_source=0 → 回错槽位 → init 永停 receive
            // 半（s17n `pm 00600` 实锤：mt=6 src=00，而 init 实际端点 0xb）。
            // 拷贝失败回退臂（下方）早已盖章，唯同步臂漏。
            let mut sender_msg = self.procs[sender_idx].p_sendmsg;
            let sender_ep = self.procs[sender_idx].p_endpoint;
            sender_msg.m_source = sender_ep;
            let sender_from_kernel = self.procs[sender_idx]
                .p_misc_flags
                .is_set(MiscFlagsBits::SENDING_FROM_KERNEL);
```

（区间复算：`sed -n '1578,1593p' os/kernel/src/ipc.rs | wc -l` = 16。锚点：`os/kernel/src/ipc.rs:fn caller_q_find_allowed`（走查复用 §3.1 的共享走查）。块内注释含仓库内部阶段编号，按原文引用。）

与 §2.4 第三级逐项对：`caller_q_find_allowed` 带过滤谓词走链（accept 谓词位置=C 的 CANRECEIVE 位置，§3.1 已立）；命中后摘链（`caller_q_remove`，前驱链接与头尾指针修完）；盖 `m_source`（发送者用户缓冲里这个字段恒为垃圾，内核交付时盖——注释记录的正是漏盖一次的真机事故）；拷贝去向按「接收者自己的陷入里」分流：先试同步拷给用户缓冲（此刻当前地址空间恰是接收者的，§2.7 的两段协议在此段成立），失败才退回沉降+`MF_DELIVERMSG` 让循环拍兜底——退回条件与 C 的「拷贝失败进 delivermsg」同形。随后是唤醒发送者与返回码的两个门控：

```rust
            if !sender_reply_pend
                && !sender_from_kernel
            {
                crate::proc::set_ipc_return_code(&mut self.procs[sender_idx], OK as i64);
            }
```

（区间复算：`sed -n '1665,1669p' os/kernel/src/ipc.rs | wc -l` = 5。锚点：`os/kernel/src/ipc.rs:fn receive`。）

写返回码的两个例外各有来历：SENDREC 发送者的调用还没完（收一半还在等回信，返回码由回信交付时写），内核代发的伪发送者根本没有 IPC 陷阱帧，写 OK 会改掉用户态陷阱上下文里的寄存器——两条例外分别对应 C 侧「该 OK 由停车点后续跑路径注定产生」与「无返回路径」两个语义。全落空的出口：

```rust
        self.procs[caller_idx].p_getfrom_e = src_endpoint;
        self.procs[caller_idx]
            .p_rts_flags
            .set(RtsFlagsBits::RECEIVING);
        IpcOutcome::Blocked
```

（区间复算：`sed -n '1741,1745p' os/kernel/src/ipc.rs | wc -l` = 5。锚点：`os/kernel/src/ipc.rs:fn receive`。）

`p_getfrom_e` + `RTS_RECEIVING` + `Blocked`，与 §2.4 的停车段逐字对应；死锁预检在停车前的位置同样保留（`receive` 内 `detect_deadlock(IpcCall::Receive, ...)` 调用，对应 C 的 `deadlock(RECEIVE, ...)`）。权限检查在引擎入口之前由 `os/kernel/src/ipc.rs:fn check_ipc_permission` 统一把守，共四层：ANY 通配仅 RECEIVE 合法、发送白名单、trap 掩码、内核任务限制——C 的六道在 Rust 合并为四层（名字表检查由 `IpcCall::from_raw` 的穷举匹配承担，端点有效性与 ANY 检查合并进端点解析），与本篇 §2.1 所述的 C 序一一对应。

### 4.2 detect_deadlock 走查（→§3.2）

本节实现回指 §3.2 的结构化返回决策。全函数如下：

```rust
        // The 2-cycle check uses C's encoding: SEND=1 → 0x04, RECEIVE=2 → 0x08.
        // `function << 2` must align with `RTS_SENDING = 0x04` for the XOR test.
        let direction = match function {
            IpcCall::Send | IpcCall::SendRec | IpcCall::SendNb => DeadlockDirection::Send,
            IpcCall::Receive => DeadlockDirection::Receive,
            _ => return None,
        };

        let caller_idx = self.idx_of(caller_nr)?;
        let caller_endpoint = self.procs[caller_idx].p_endpoint;

        let mut chain: [ProcNr; PROC_TABLE_SIZE] = [ProcNr(NONE_PROC_NR); PROC_TABLE_SIZE];
        let mut chain_len: usize = 0;
        chain[chain_len] = caller_nr;
        chain_len += 1;

        let mut current_ep = dst_endpoint;
        let mut group_size: usize = 1;

        loop {
            if current_ep == Endpoint::ANY {
                return None;
            }
            let target_idx = self.idx_by_endpoint(current_ep)?;
            let target_nr = self.procs[target_idx].p_nr;
            group_size += 1;
            chain[chain_len] = target_nr;
            chain_len += 1;

            let next_ep = Self::blocked_on(&self.procs[target_idx])?;

            if next_ep == caller_endpoint {
                if group_size == 2 {
                    // 2-cycle exception. C: `(xp->p_rts_flags ^ (function << 2)) & RTS_SENDING`
                    // — proc.c:746. If the XOR is non-zero on the SENDING bit, this is
                    // SEND↔RECEIVE (normal request/reply) and NOT a deadlock.
                    let xp_rts = self.procs[target_idx].p_rts_flags.get().bits();
                    let function_shifted = (function as u32) << 2;
                    // NK4-C 1.10m 取证探针（task1-close 裁决删除）：2-cycle
                    // 判定现场（caller/fn/xp/xp_rts）——PM send ELOCKED 误报
                    // 定位。
                    #[cfg(not(feature = "mock"))]
                    {
                        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                        static DD_N: AtomicUsize = AtomicUsize::new(0);
                        if DD_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                        }
                    }
                    if (xp_rts ^ function_shifted) & RtsFlagsBits::SENDING.bits() != 0 {
                        return None;
                    }
                }
                // NK4-C 1.10n 逐环仪器化（task1-close 裁决删除）：死锁判定
                // 现场补发双方消息类型——caller 在发的 m_type 与 xp 在发的
                // m_type（p_sendmsg），定位协议互撞的具体消息。
                #[cfg(not(feature = "mock"))]
                {
                    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                    static DD2_N: AtomicUsize = AtomicUsize::new(0);
                    if DD2_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                    }
                }
                return Some(DeadlockCycle {
                    chain,
                    chain_len,
                    direction,
                    group_size,
                });
            }
            current_ep = next_ep;
        }
    }
```

（区间复算：`sed -n '1220,1291p' os/kernel/src/ipc.rs | wc -l` = 72。锚点：`os/kernel/src/ipc.rs:fn detect_deadlock`。块内注释含仓库内部阶段编号，按原文引用。）

与 §2.2 逐段对：入口把调用号归约成方向（SEND/SENDNB/SENDREC 归 Send 向，RECEIVE 归 Receive 向——归约对应 C 两个调用点只传 1 或 2 的现实）；走链循环里 `blocked_on` 是 `P_BLOCKEDON` 宏的直系对位（`os/kernel/src/proc.rs:fn blocked_on`，按 RTS 位动态选 `p_sendto_e` 或 `p_getfrom_e`，次序与宏注释同：先 SENDING 后 RECEIVING）；`ANY` 返回 `None`、链断（`blocked_on` 得 `None`）返回 `None`；回到调用者端点时，`group_size == 2` 的 XOR 豁免逐字移植——`(xp_rts ^ function_shifted) & SENDING`，`function_shifted` 的取值约定注释里写明对齐 `RTS_SENDING = 0x04`。

§2.2 的三方环手推在 Rust 侧重放一遍：A SEND→B、B SEND→C、C SEND→A，`detect_deadlock(IpcCall::Send, A, B)` 从 B 出发，`blocked_on(B)` 得 C、`blocked_on(C)` 得 A——A 等于调用者端点，`group_size` 已累计到 3，豁免分支要求 2、不成立，返回 `Some(DeadlockCycle { group_size: 3, chain: [A, B, C], ... })`。互发例（A SEND→B 且 B SEND→A）：走一步回 A，`group_size == 2`，`xp` 为 B、`xp_rts = 0x04`、`function_shifted = 0x04`，XOR 后按位与得 0，豁免不成立 → `Some`，ELOCKED。互补对例（SENDREC 发送半预检遇 B 在收）：`xp_rts = 0x08`、`function_shifted = 0x04`，按位与非零 → `None`，放行停车。混合链的机器重放由测试钉住（§5.2 的 mixed-chain 测试）。

### 4.3 notify/SENDA/delivermsg 走查（→§3.3/§3.4）

notify 的核心是 §2.5 两分支的直译（实现回指 §3.3 的结果三分与 §3.5 的差异登记 3）：

```rust
    if IpcEngine::is_willing_to_receive(&procs[dst_idx], caller_endpoint)
        && !procs[dst_idx].p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND)
    {
        let src = NotifySource::from_caller_nr(caller_nr);
        // C proc.c:1148 — a process parked in RECEIVE must not already
        // carry a pending delivery when we write the notify into it.
        debug_assert!(!procs[dst_idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        build_notify_message(procs, priv_table, dst_idx, src);
        // C mini_notify（proc.c:1147 对位）：m_source = 调用者端点。
        // build_notify_message 置 Message::default() 后未补 m_source，
        // 通知以 src=NONE(0x7bff) 投递——sef 的 SYSTEM 信号路由
        // （source == SYSTEM_ENDPOINT → on_signal）失配，VM 的
        // SIGKMEM 唤醒被当普通消息丢弃（NK4-A C-3 迭代9 真机：
        // RS 挂起后 VM 收到 src=NONE/type=0 空消息无限自旋、RS 饿死）。
        procs[dst_idx].p_delivermsg.m_source = caller_endpoint;
        procs[dst_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        // C: proc.c:1154 — `IPC_STATUS_ADD_CALL(dst_ptr, NOTIFY)`
        crate::proc::ipc_status_add_call(&mut procs[dst_idx], IpcCall::Notify);
        procs[dst_idx].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
        return IpcOutcome::Delivered;
    }

    // C: `priv(dst)->s_notify_pending |= (1 << priv_id(caller))` — proc.c:1165.
    // The bit position is the CALLER's priv_id, not proc_nr.
    if let Some(caller_pid) = caller_priv_id
        && (caller_pid as u32) < 64
            && let Some(dst_priv_id) = procs[dst_idx].priv_id
                && let Some(dst_priv) = priv_table.get_mut(dst_priv_id) {
                    dst_priv.signals.s_notify_pending |= 1u64 << caller_pid;
                }
    // If the caller has no priv_id (user process), the notification
    // is silently dropped — matching Minix3 behavior where only
    // system processes have privilege entries. User-process
    // notifications route through PM.
    IpcOutcome::Delivered
```

（区间复算：`sed -n '2891,2925p' os/kernel/src/ipc.rs | wc -l` = 35。本块为节选（2891-2925 行连续未删节），省略号处为直接交付分支头上的判定文档（MF_REPLY_PEND 守卫的理由，语义正文已述）。块尾注释里那句「matching Minix3 behavior where only system processes have privilege entries」与本仓 C 真值源不符——C 的特权结构人人有（见 §3.5 登记 3），按原文引用、以登记为准。锚点：`os/kernel/src/ipc.rs:fn mini_notify_core`。）

直接交付分支多出一道 C 同款守卫：目标在 SENDREC 中（`MF_REPLY_PEND` 置位）时通知不得直投——§2.4 的瀑布跳过规则在发送侧的对偶（C 的 `mini_notify` 里同一个条件，§2.5 引文的 `&& !(dst_ptr->p_misc_flags & MF_REPLY_PEND)`）。消息组装委托 `os/kernel/src/ipc.rs:impl IpcEngine::build_notify_message`（`NotifySource` 三分类对 C 宏的 HARDWARE/SYSTEM/普通进程三路，`NOTIFY_MESSAGE` 常量同值 0x1000；HARDWARE 臂搬中断位图并清零、SYSTEM 臂搬待决信号集并清零，语义与 §2.7 的组装规则逐项同）。位图记账分支的差异已在 §3.5 登记（无特权结构调用者丢弃、位宽 64 与 C 的 `NR_SYS_PROCS` 同宽）。引擎包装 `os/kernel/src/ipc.rs:fn notify` 在直投命中时额外记录唤醒目标（§3.5 差异 4 的拆分点）。

SENDA 的 Rust 体是无内核缓存的逐条协议（实现回指 §3.5 处置表的保留项与差异 2）。提交侧头部：

```rust
        // C: mini_senda — SYS_PROC check (proc.c:1331-1342).
        macro_rules! sa_exit {
            ($tag:expr, $val:expr) => {{
                #[cfg(not(feature = "mock"))]
                {
                    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                    static SAX_N: AtomicUsize = AtomicUsize::new(0);
                    if SAX_N.fetch_add(1, AtomicOrd::Relaxed) < 4 {
                    }
                }
                $val
            }};
        }
        let caller_idx = match self.idx_of(caller_nr) {
            Some(i) => i,
            None => return sa_exit!("e1-idx", IpcOutcome::Error(IpcError::DeadSrcDst)),
        };
        let caller_priv_id = match self.procs[caller_idx].priv_id {
            Some(id) => id,
            // C: "caller has no privilege structure" → EPERM (proc.c:1337).
            None => return sa_exit!("e2-priv", IpcOutcome::Error(IpcError::Permission)),
        };
        let caller_is_sys = self
            .priv_table
            .get(caller_priv_id)
            .map(KPriv::is_sys_proc)
            .unwrap_or(false);
        if !caller_is_sys {
            return sa_exit!("e3-nosys", IpcOutcome::Error(IpcError::Permission));
        }
```

（区间复算：`sed -n '2227,2256p' os/kernel/src/ipc.rs | wc -l` = 30。锚点：`os/kernel/src/ipc.rs:fn senda`。）

SYS_PROC 门槛（对应 §2.6 `mini_senda` 的 EPERM 臂）、登记清零（对应 `try_deliver_senda` 开头的三行清登记）、尺寸上限检查（对应 EDOM 臂，错误码差异见 §3.5 登记 2）逐项在位。逐条循环的四个分支与 §2.6 的生命周期图同构：`read_senda_entry` 即 `A_RETR` 的对位（§3.4 trait 的两个 SENDA 方法），空项跳过、违例跳过、已办结跳过、`dst == SELF` 就地自指替换；目标在收则直接交付（状态寄存器记 SENDA，`SENDA` 内部标志在此消费）+ `write_senda_result` 写回 OK；投不出去则在目标 `s_asyn_pending` 记发送者特权位、`done = false`、本条不写回；收尾对称（全办结清登记、有遗留恢复登记 + ASYNCM 自通知）。接收侧兑现：

```rust
        // C: scan the table (proc.c:1422-1491). Delivery stops at the
        // first entry that matches (break — one message per receive).
        let mut done = true;
        let mut do_notify = false;
        let mut delivered = false;

        // 1.12：A_RETR 按发送者 root 读（deliver_async 跑在接收者的
        // receive 陷入里，current root 是接收者——C 用 umap(sender) 同理）。
        let sender_root = self.procs[sender_idx].p_seg.phys_root;
        for i in 0..size {
            // C: A_RETR(i) — per-entry copy-in; failure skips the entry.
            let (dst_ep, msg, flags) = match self.user_copy.read_senda_entry(sender_root, table, i) {
                Ok(t) => t,
                Err(_) => continue,
            };

            // C: flags == 0 → skip (proc.c:1434).
            if flags == AMF_EMPTY {
                continue;
            }
            // D-16 + NK4-C B8: CANRECEIVE 两半边（C ipc.h:19-22）——
            // 源过滤半边用本次 receive 的 `receive_e` 实参（C try_one
            // proc.c:1457 传的是形参 receive_e，**不是** dst->p_getfrom_e；
            // 读 p_getfrom_e 的是 WILLRECEIVE 宏 ipc.h:14-17，只用于第三方
            // 已阻塞接收者的判定，如 mini_senda 直投臂）。本函数跑在接收
            // 者自己 receive 陷入的 Phase 2，p_getfrom_e/RECEIVING 尚未被
            // 阻塞腿写入（下方 Phase 3 之后才写），旧版误用
            // is_willing_to_receive 读到握手期过期值 → RS_INIT 永拒 →
            // VFS 睡死（B8 boot 死锁）。RECEIVING/SENDING 两谓词在 receive
            // 陷入内部天然成立，不需搬入。
            if !(receive_src == Endpoint::ANY || receive_src == sender_ep_final) {
                continue;
            }
            if !self.can_receive(caller_idx, sender_ep_final, msg.m_type) {
                continue;
            }
```

（区间复算：`sed -n '1912,1947p' os/kernel/src/ipc.rs | wc -l` = 36。锚点：`os/kernel/src/ipc.rs:fn deliver_async`。逐字引用，注释按原文含该段受控比喻词（前注已释）。）

`deliver_async` 是 `try_one` 的对位，两个非显然点由注释里的真机事故钉住。其一，源过滤用**本次 receive 的实参**而不是接收者槽里存的 `p_getfrom_e`——`deliver_async` 跑在接收者自己 receive 陷入的匹配段，停车字段此时还没写，读它拿到的是握手期的过期值（C 的 `try_one` 同样传形参 `receive_e`，§2.6 引文可见）。其二，表项按**发送者**的页表根翻译（`sender_root`）——这段代码跑在接收者的陷入里，当前地址空间是接收者的，直接按用户地址解引用会落进错误的地址空间；`A_RETR` 的 C 实现经 `umap(sender)` 走发送者段，同一条纪律。尾部与 C 同构：一次只投一条；全表办结清登记、有遗留把发送者位重挂回接收者位图（对应 §2.6 引文块三的尾部收尾）。

delivermsg 的 Rust 体是两段协议第二段的直译（实现回指 §3.4 的边界 trait 与 §2.7 的两次失败策略）：

```rust
pub fn delivermsg(proc: &mut crate::proc::KProcess, user_copy: &dyn UserCopy) -> DeliverResult {
    debug_assert!(
        proc.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG),
        "delivermsg called without MF_DELIVERMSG"
    );

    let msg = proc.p_delivermsg;
    let user_addr = proc.p_delivermsg_vir;

    match user_copy.copy_msg_to_user(user_addr, &msg) {
        Ok(()) => {
            proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
            proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
            // C: proc.c:290-292 — the completed IPC returns OK to the user
            // caller. The wake half (mini_notify direct delivery) only sets
            // MF_DELIVERMSG; the return code lands here, when the pending
            // delivery is flushed at dispatch time (C switch_to_user's
            // delivermsg stage). Without it a blocked receiver woken by a
            // notify restores with whatever RAX held at trap entry (real
            // machine NK4-A C-3: VM woken by the SYSTEM SIGKMEM notify saw
            // its src argument, ANY=31744, as the receive result and
            // dropped the wake — RS starved on VMREQUEST).
            // MF_CONTEXT_SET means the kernel deliberately rewrote the
            // context (signal delivery); do not clobber the return reg.
            if !proc.p_misc_flags.is_set(MiscFlagsBits::CONTEXT_SET) {
                crate::proc::set_ipc_return_code(proc, OK as i64);
            }
            DeliverResult::Delivered
        }
        Err(CopyError::PageFault) => {
            if proc.p_misc_flags.is_set(MiscFlagsBits::MSGFAILED) {
                // Second consecutive failure → SIGSEGV. C: proc.c:278.
                proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
                proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
                DeliverResult::Segfault
            } else {
                // First failure → vm_suspend. C: proc.c:281-282.
                proc.p_misc_flags.set(MiscFlagsBits::MSGFAILED);
                DeliverResult::PageFault
            }
        }
        Err(CopyError::OutOfBounds) => {
            proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
            proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
            DeliverResult::Segfault
        }
    }
}
```

（区间复算：`sed -n '742,789p' os/kernel/src/ipc.rs | wc -l` = 48。锚点：`os/kernel/src/ipc.rs:fn delivermsg`。块内注释含仓库内部阶段编号，按原文引用。）

两次失败策略与 §2.7 逐项对：首败置 `MF_MSGFAILED` 并把 `DeliverResult::PageFault` 交给调用方路由挂起；连败清位并报 `Segfault`；越界直接报 `Segfault`。成功路径清两个标志、按 `MF_CONTEXT_SET` 门控写返回码——注释里那句「被通知唤醒的接收者若不写 OK 会带着陷入残值恢复」对应 §2.7 成功段同一行 C 代码（`rp->p_reg.retreg = OK`）。引擎侧包装 `os/kernel/src/ipc.rs:fn deliver_message` 委托同一个自由函数，循环拍（[10-switch-to-user.md](10-switch-to-user.md) §4.1 的 misc flags 消费点）与分派层共用一条交付路径。

### 4.4 KERNINFO 臂与 IPCNAME 的 Rust 对位（→§2.1/§2.7）

实现回指 §3.5 处置表的 KERNINFO 保留项。Rust 的 KERNINFO 臂在引擎构造**之前**拦截——C 的同名臂住在 `do_ipc` 的 switch 里，Rust 的住在分派层（`os/kernel/src/syscall.rs:fn dispatch_ipc`）：

```rust
    ipc_call: crate::ipc::IpcCall,
) -> KcallResult {
    use crate::ipc::{IpcEngine, IpcOutcome, IpcError, KernelUserCopy, SendFlags};
    use crate::errno::*;
    use minix_types::VirBytes;
    use minix_arch::{CurrentCpuContextArch, CpuContextArch};

    // ── MINIX_KERNINFO (6) ──
    // C: proc.c:685-693 — the kernel info page is handed over through the
    // secondary IPC return channel, not a message: check the page has been
    // published to user space, store its address in the caller's saved
    // context, return OK. No message buffer is read or written, and the
    // endpoint/SENDA argument decode below does not apply.
    if matches!(ipc_call, crate::ipc::IpcCall::KernInfo) {
        let page = crate::globals::MINIX_KERNINFO_USER.load(core::sync::atomic::Ordering::Relaxed);
        if page == crate::globals::KERNINFO_USER_UNSET {
            // C: proc.c:687-689 — "It might not be initialized yet."
            return KcallResult::Ok(EBADCALL);
        }
        let procs = proc_table.procs_slice_mut();
        <CurrentCpuContextArch as CpuContextArch>::set_secondary_ipc_return(
            &mut procs[caller_idx].cpu_context,
            page,
        );
        return KcallResult::Ok(OK);
```

（区间复算：`sed -n '828,852p' os/kernel/src/syscall.rs | wc -l` = 25。锚点：`os/kernel/src/syscall.rs:fn dispatch_ipc`。）

与 §2.1 的臂逐项对：未发布返回 EBADCALL（发布点的布尔在 C 是全局指针判空、Rust 是原子字段判哨兵值）；已发布则把页地址写进调用者保存上下文的**二次返回通道**，返回 OK，消息缓冲全程不碰。引擎侧的防御分支（引擎永不收到 KernInfo，match 穷举要求有一臂）：

```rust
            IpcCall::Notify => self.notify(caller_nr, dst_endpoint),
            // SendA was intercepted above the permission layer (C do_ipc
            // switch order, proc.c:673-684 before proc.c:503-541); this arm
            // is unreachable and keeps the match exhaustive.
            IpcCall::SendA => IpcOutcome::Error(IpcError::BadCall),
            // `MINIX_KERNINFO` never reaches the engine: `dispatch_ipc`
            // (syscall.rs) handles it before engine construction, mirroring
            // C where the arm sits in the same outer `do_ipc` switch but
            // returns via the secondary IPC register instead of a message
            // (proc.c:685-693). Defensive arm keeps the match exhaustive.
            IpcCall::KernInfo => IpcOutcome::Error(IpcError::BadCall),
```

（区间复算：`sed -n '2714,2724p' os/kernel/src/ipc.rs | wc -l` = 11。锚点：`os/kernel/src/ipc.rs:fn do_ipc`。）

二次返回通道的 x86-64 载体：

```rust
    fn set_secondary_ipc_return(ctx: &mut Self::CpuContext, value: u64) {
        // C: `arch_set_secondary_ipc_return` — arch_system.c:184-186,
        // i386 body `p->p_reg.bx = val`. Plain assignment (whole address,
        // not a flag merge).
        // NK4-C 1.54（B26）：车道从 RBX 改 R10。int33 立即返回臂只把
        // rax + R10（sync_status_register_to_frame）同步进返回帧，
        // ctx.rbx 永远达不到用户——写在 RBX 的页地址实测丢失（bn2/bn3：
        // 用户 r10 拿陷阱残留垃圾→ deref 崩）。C 的 i386 bx 本就是
        // 「返回路径同步的那条」且兼状态+secondary 两职；Task C 把
        // 状态迁到 caller-saved 的 R10 后，secondary 的 64 位对位就是
        // 同一条 R10：两职按调用互斥（状态只在交付 OR，secondary 只在
        // KERNINFO 写），与 C 同一语义。
        ctx.gp_regs[crate::x86_64::signal::GP_R10] = value;
    }
```

（区间复算：`sed -n '259,272p' os/arch/src/x86_64/boot.rs | wc -l` = 14。锚点：`os/arch/src/x86_64/boot.rs:fn set_secondary_ipc_return`（trait 实现项）。块内注释含仓库内部阶段编号，按原文引用。）

C 的 i386 载体是 `p_reg.bx`（§2.1 的通道图），x86-64 的载体换到 R10——注释里的分工约束（同一寄存器按调用互斥地兼状态车道与二次返回车道）与 C 的 i386 bx 兼两职同构。int-33 门的 bodyless 判定把 call 6 连同 SENDA、NOTIFY 归为「不携带消息缓冲」的调用（`os/kernel/src/trap_dispatch.rs` 的 is_bodyless 判定段），用户态以零缓冲陷入是合法形态。

页的建立与发布在 `os/kernel/src/kerninfo.rs`（页本体归 [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §4.3，本节只看发布点与查询臂的闭合）：

```rust
    // Interior fixups + magic, C memory.c:878-919 order (pointers, then
    // magic as the final content write before publication).
    let page = unsafe { &mut *KERNINFO_PAGE.get() };
    page.info.kuserinfo = KERNINFO_USER_VA + KUSERINFO_OFFSET as u64;
    page.info.ki_flags = MINIX_KIF_USERINFO;
    page.info.kerninfo_magic = KERNINFO_MAGIC;

    // Publication is the release point: from this store on, the
    // MINIX_KERNINFO dispatch arm returns OK with this address instead of
    // EBADCALL (proc.c:687-692). Relaxed matches the dispatch-side load;
    // the store happens once under the boot-time BKL before any user code
    // runs, so no cross-CPU ordering hazard exists at write time.
    MINIX_KERNINFO_USER.store(KERNINFO_USER_VA, Ordering::Relaxed);
```

（区间复算：`sed -n '151,163p' os/kernel/src/kerninfo.rs | wc -l` = 13。锚点：`os/kernel/src/kerninfo.rs:fn fill_kerninfo_page`（发布存储在该函数内）。）

发布存储是查询臂 EBADCALL/OK 分界的唯一开关：存储之前的所有 MINIX_KERNINFO 查询得 EBADCALL（C 侧同一分界的开关是 `minix_kerninfo_user` 全局指针，§2.1 引文的判空分支）。

IPCNAME 的 Rust 对位，如实披露：**没有**。检索范围内（`os/` 全树）未命中 `ipc_call_names` 或等价的名字表——C 的六原语名字→trap 号映射（§2.7 的 main.c 登记段）在 Rust 侧由 `IpcCall` 枚举的变体名承担，但那只服务于内核内代码，C 名字表的运行期用途（按 trap 号查名字字符串供调试输出与 `do_sync_ipc` 入口校验）没有对位物。检索模式覆盖了 `ipc_call_names`、`IPCNAME` 两种拼法（`grep -rn 'ipc_call_names\|IPCNAME' os/` 零命中）；据此本篇的表述是「在 os/ 树检索范围内未命中对位」，不是断言「永远不需要」——`do_sync_ipc` 用名字表做入口校验的那道门在 Rust 侧由 `from_raw` 的穷举匹配承担（无效 trap 号得 `None` → `BadCall`，`os/kernel/src/ipc.rs:fn from_raw`），调试输出面的缺口则真实存在。

---

## 5. 测试要点

### 5.1 清单与读数（→Ch4）

ipc.rs 的测试模块（`os/kernel/src/ipc.rs` 内的 tests 模块）按机制分组，60 个测试函数的分布如下（口径：宿主 mock 形态全套件一次运行的输出计数，`grep -c 'test ipc::tests' ` 于测试输出 = 60）：

| 分组 | 测试名（节选，全组可按前缀 grep） | 覆盖 |
|---|---|---|
| 类型与编码 | `test_ipc_call_variants`、`test_ipc_call_from_raw`、`test_ipc_error_variants`、`test_ipc_outcome_predicates`、`test_send_flags_p0_fix_values` | 枚举值与 C 常量对齐 |
| 发送者等待队列 | `test_caller_q_push_find_remove_fifo`、`test_caller_q_remove_by_nr_unlinks` | §3.1 的侵入式 FIFO |
| 死锁检测 | `test_deadlock_send_send_two_cycle_is_deadlock`、`test_deadlock_send_receive_two_cycle_not_deadlock`、`test_deadlock_receive_receive_two_cycle_is_deadlock`、`test_deadlock_three_proc_send_cycle`、`test_deadlock_mixed_chain_cycle` 等 9 个 | §3.2/§4.2 |
| send | `test_send_when_target_receiving`、`test_send_when_target_not_receiving`、`test_send_non_blocking_returns_not_ready`、`test_send_detects_deadlock` | §4.1 两路径 |
| receive | `test_receive_picks_notify_first`、`test_receive_skips_notify_when_reply_pend`、`test_receive_picks_caller_q_last`、`test_receive_picks_async_second`、`test_receive_blocks_when_no_match`、`test_receive_caller_q_drain_copies_to_user_buffer_not_deposit` 等 8 个（第九个相关测试 `test_do_ipc_receive_prologue_clears_reply_pend_and_status` 挂在 do_ipc 前缀下） | §4.1 四级 |
| notify | `test_notify_delivers_when_target_receiving`、`test_notify_defers_when_target_reply_pend`、`test_notify_records_bitmap_when_not_receiving`、`test_notify_never_blocks` | §4.3 两分支 |
| deliver | `test_deliver_message_success`、`test_deliver_message_first_page_fault`、`test_deliver_message_second_consecutive_fault`、`test_deliver_message_context_set_preserves_return_code` | §4.3 两次失败策略 |
| 过滤集成 | `test_d16_whitelist_allows_listed_blocks_unlisted`、`test_d16_notify_filtered_stays_pending`、`test_d16_senda_filtered_entry_stays_pending` | 谓词注入的过滤半边（§3.1） |
| 权限 | `test_check_ipc_permission_target_kernel_task_restriction` | §4.1 权限门 |
| SENDA | `test_senda_requires_sys_proc`、`test_senda_delivers_to_receiving_target`、`test_senda_pending_sets_bitmap_and_retries_via_receive` 等 5 个 | §4.3 无缓存协议 |
| 用户拷贝校验 | `user_copy_range_rejects_kernel_half_as_out_of_bounds` 等 8 个 | §3.4 先验证后触碰 |
| sendrec 集成 | `test_sendrec_to_blocked_receiver_stamps_source_endpoint`、`test_sendrec_parked_receive_half_getfrom_locks_to_destination`、`test_sendrec_fast_path_receive_half_scopes_to_destination` | sendrec 两半协同 |

计数口径分两条说清。源码侧：`grep -c 'fn test_' os/kernel/src/ipc.rs` = 53，其中 52 个是 `#[test]` 函数、1 个是辅助函数 `test_nr`（不算测试）；另有 8 个 `user_copy_range_*` 测试不带 `test_` 前缀，二者合计 60，与套件输出一致。套件侧（读数一律给全套件口径，勿用单模块计数）：

```bash
cd os && cargo test -p minix-kernel --features mock
```

读数（全套件实跑）：单元测试 831 passed / 0 failed / 8 ignored；集成测试（`os/kernel/tests/boot_integration.rs`）3 passed / 0 failed / 3 ignored；doc-test 1 ignored。运行陷阱如实披露：boot_integration 里 3 个 ignored 全部因宿主 x86_64 的诊断输出走真实 COM1 端口触发 SIGSEGV——这三项的判定逻辑只在 QEMU 下生效，宿主读数里它们不构成覆盖；单元测试的 8 个 ignored 是其它模块的既定豁免，与本篇无关。

### 5.2 关键测试细读：混合链死锁与零堆 FIFO（→§3.2/§3.1）

第一个细读对象是 §4.2 混合链重放的测试本体：

```rust
    #[test]
    fn test_deadlock_mixed_chain_cycle() {
        // P0 FIX-3 regression test: mixed-chain deadlock.
        // A send→B, B receive←C, C send→A.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        let mut c = make_test_proc(2, Endpoint(3));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
        b.p_getfrom_e = Endpoint(3);
        c.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        c.p_sendto_e = Endpoint(1);
        let mut procs = crate::test_helpers::scratch_procs([a, b, c]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(
            result.is_some(),
            "Mixed-chain deadlock (A send→B, B receive←C, C send→A) must be detected \
             by dynamic blocked_on() — FIX-3 regression"
        );
        let cycle = result.unwrap();
        assert_eq!(cycle.group_size, 3);
    }
```

（区间复算：`sed -n '3225,3248p' os/kernel/src/ipc.rs | wc -l` = 24。锚点：`os/kernel/src/ipc.rs:fn test_deadlock_mixed_chain_cycle`（宏派生项，测试模块内）。）

测试构造 A→B（发送方向待停）、B 收 C、C 发 A 的混合环，断言 `detect_deadlock` 能识别。它的变红条件写在历史里：链走查若把「阻塞在谁身上」固定成单一字段（发送者读 `p_sendto_e`、接收者读 `p_getfrom_e` 的静态二选一），B 这个接收节点会把链走断（B 没在发送，字段里没有有效指向），环漏判、测试红。现行实现经 `blocked_on` 按 RTS 位动态选字段，测试绿——这条对位在 §4.2 的逐段对里已指出（`P_BLOCKEDON` 宏的先 SENDING 后 RECEIVING 次序）。

第二个细读对象是零堆 FIFO 的队尾修正：

```rust

    #[test]
    fn test_caller_q_push_find_remove_fifo() {
        // Three slots; queue on slot 2 (target). FIFO order via
        // caller_q_push; find walks head-first; remove unlinks middle.
        let mut procs = crate::test_helpers::scratch_procs([
            make_test_proc(0, Endpoint(11)),
            make_test_proc(1, Endpoint(22)),
            make_test_proc(2, Endpoint(33)),
        ]);
        assert!(caller_q_is_empty(&procs, 2));
        caller_q_push(&mut procs, 2, 0);
        caller_q_push(&mut procs, 2, 1);
        assert_eq!(caller_q_len(&procs, 2), 2);
        // Chain: head=nr(0) → nr(1); tail=nr(1).
        assert_eq!(procs[2].caller_q_head, Some(test_nr(0)));
        assert_eq!(procs[2].caller_q_tail, Some(test_nr(1)));
        assert_eq!(procs[0].send_q_link, Some(test_nr(1)));
        assert_eq!(procs[1].send_q_link, None);
        // ANY matches the head.
        assert_eq!(caller_q_find(&procs, 2, Endpoint::ANY, |_| true), Some(0));
        // Specific endpoint match walks the chain.
        assert_eq!(caller_q_find(&procs, 2, Endpoint(22), |_| true), Some(1));
        // Remove the middle sender: head link must be fixed.
        assert!(caller_q_remove(&mut procs, 2, 1));
        assert_eq!(caller_q_len(&procs, 2), 1);
        assert_eq!(procs[2].caller_q_head, Some(test_nr(0)));
        assert_eq!(procs[2].caller_q_tail, Some(test_nr(0)));
        assert!(procs[1].send_q_link.is_none());
        // Remove the last sender: queue becomes empty.
        assert!(caller_q_remove(&mut procs, 2, 0));
        assert!(caller_q_is_empty(&procs, 2));
        // Removing a non-member returns false.
        assert!(!caller_q_remove_by_nr(&mut procs, 2, test_nr(1)));
    }
```

（区间复算：`sed -n '3033,3067p' os/kernel/src/ipc.rs | wc -l` = 35。锚点：`os/kernel/src/ipc.rs:fn test_caller_q_push_find_remove_fifo`（宏派生项）。）

测试走完一次完整的入队-命中-摘链：两个发送者进队后断言头尾两个指针与中间链接字段的每一个值；摘掉队中成员后断言前驱链接与头尾指针都修正到位；摘掉最后成员后断言队空；对非成员摘链断言返回 false。它的变红条件在摘链的前驱修正：`caller_q_remove` 若只把被摘者的链接接走而忘了改前驱的 `send_q_link`，队列从被摘点断成两截，队尾之后的发送者从此不可达——测试在「Remove the middle sender」一步的红就是这处缺陷的形状。

### 5.3 测试边界与诚实披露（→Ch3）

宿主 mock 测不到的三件事，各给一句与反证命令。其一，真 int-33 陷入往返：宿主没有陷入指令，消息缓冲指针从用户栈到内核的搬运由门逻辑模拟，往返语义（停车-唤醒-返回码落位）只有真机门覆盖——`grep -rn 'int.33\|int33' os/kernel/tests/ | wc -l` 为零命中：宿主测试面连模拟层的陷入注释都没有。其二，跨地址空间拷贝的物理路径：宿主的 `KernelUserCopy` 是桩（§3.4），页表走查与直接映射窗口的字节搬运在 mock 下不走——`grep -c 'copy_via_root_pages' os/kernel/src/ipc.rs` 的命中集中在生产 `#[cfg(not(test))]` 分支，宿主测试体不执行它们。其三，IPC 过滤的真机行为：过滤集成测试在宿主以手动装配的过滤池驱动（§5.1 的过滤组），生产分派接入的全局过滤池与三架构链式判定只有 QEMU 门可验。真机与宿主的分界即此：协议层（位变、队列、匹配序、死锁）宿主可验；陷入往返与物理搬运真机才验。

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

四个高频症状各给一条判别路径。全部命令在仓库根目录原样重放，预期关键行取自本文实跑输出。

**症状一：合法的 send 收到 ELOCKED（死锁误报感）。** 怀疑对象按序：XOR 豁免的编码前提是否还成立（trap 号与 RTS 位的对齐）、`P_BLOCKEDON` 的判断序是否被改动。确认命令与预期关键行：

```bash
grep -n 'define SEND\b\|define RECEIVE\b\|define SENDREC\b' minix3/minix/include/minix/ipcconst.h
grep -n 'define RTS_SENDING\|define RTS_RECEIVING' minix3/minix/kernel/proc.h
```

预期关键行：`7:#define SEND`（值 1）、`8:#define RECEIVE`（值 2）、`9:#define SENDREC`（值 3）、`144:#define RTS_SENDING	0x04`、`145:#define RTS_RECEIVING	0x08`。五个数对上，`function << 2` 的魔法才成立；若有人重排 trap 号或改 RTS 位值，§2.2 的全部手推作废，互补对会被误判死锁。第二步用 §2.2 的三个手推对现场：把卡住双方的 RTS 位与阻塞字段（`p_sendto_e`/`p_getfrom_e`）代进 XOR 算式，先人工判一次「该不该豁免」，再对引擎行为。

**症状二：消息投递不到达（接收者长期收不到）。** 怀疑对象按 §2.4 的三级瀑布逆查：通知位图有没有被 SENDREC 保护跳过、异步位有没有遗留、同步队列上的发送者有没有被 CANRECEIVE 拒绝（过滤在对面，`23-ipc-filter` 方向）。确认命令与预期关键行：

```bash
grep -n 'CANRECEIVE' minix3/minix/kernel/proc.c
```

预期关键行：`1013`（第一级通知的逐源验证）、`1058`（第三级同步队列逐个验证）、`1457`（异步兑现 try_one 内）。三处命中点就是过滤判定嵌进匹配路径的全部位置——「消息不到达」时按接收者的 receive 形态（收 ANY 还是收指定源、是否 SENDREC 中）定位它停在哪一级。

**症状三：NOTIFY 丢失感（通知像没发过）。** 怀疑对象：位图按特权 id 索引（§2.5 的「Note the indirectness」）——发送者的进程号变了但特权 id 没变时通知仍应可达；反过来，接收者侧的清位时机若被提前（组装之前清、组装失败不还位）就真丢。确认命令与预期关键行：

```bash
grep -n 's_notify_pending' minix3/minix/kernel/proc.c minix3/minix/kernel/priv.h
```

预期关键行：`proc.c:845`（has_pending_notify 的读侧）、`proc.c:863`（unset_notify_pending 的清侧）、`proc.c:1165`（mini_notify 的记侧）、`priv.h:42`（字段声明）。三个读数合起来是完整账目：记在 1165、读在 845、清在 863——清位只应发生在读命中之后。

**症状四：SENDA 表项卡死（一批里总有条目永远办不完）。** 怀疑对象：`A_RETR` 拷入失败（表所在页不可入，条目停在未办结态等重试）、VM 抑制期间的 MISS 补投（SMP，§2.6 末段）、或表项被 NOREPLY/过滤拦下而接收者不再收。确认命令与预期关键行：

```bash
grep -n 'A_RETR\|A_INSRT\|asyn_error' minix3/minix/kernel/proc.c
```

预期关键行：`1174`/`1188`（两个宏的定义）、`1244`/`1430`（提交侧与兑现侧的拷入点）、`1307`（写回点）、`1193`（写回失败不终止的注释）、`1310`（提交侧错误出口）。办不完的条目按「最后一次命中的是 1244 还是 1430」分流：前者是提交轮的拷入失败，后者是兑现轮的拷入失败。

### 6.2 手算与常量核对命令

trap 号到 RTS 位的对齐可现场复算：

```bash
awk 'BEGIN{printf "SEND=1 -> %d (0x%02X)  RECEIVE=2 -> %d (0x%02X)  SENDREC=3 -> %d (0x%02X)\n", 1*4, 4, 2*4, 8, 3*4, 12}'
```

预期输出一行：`SEND=1 -> 4 (0x04)  RECEIVE=2 -> 8 (0x08)  SENDREC=3 -> 12 (0x0C)`——与 §2.2 的解码表逐格一致，RTS_SENDING=0x04 即 4。

常量定位三连（尺寸上限、位图宽、通知类型）：

```bash
grep -n 'define NOTIFY_MESSAGE' minix3/minix/include/minix/com.h
grep -n 'define NR_SYS_PROCS' minix3/minix/include/minix/config.h; grep -n '_NR_SYS_PROCS' minix3/minix/include/minix/sys_config.h
grep -n 'define NR_TASKS' minix3/minix/include/minix/com.h; grep -n '_NR_PROCS' minix3/minix/include/minix/sys_config.h | head -1
```

预期关键行：`com.h:90`（`NOTIFY_MESSAGE 0x1000`）、`config.h:32` + `sys_config.h:9`（`NR_SYS_PROCS = 64`，通知与异步两张位图的宽）、`com.h:56` + `sys_config.h:8`（`NR_TASKS = 5`、`_NR_PROCS = 256`，SENDA 尺寸上限 16×261=4176 的两个因子）。

Rust 侧锚点定位（§3/§4 全部机制函数与类型）：

```bash
grep -n 'pub enum IpcCall\|pub enum IpcOutcome\|pub enum IpcError\|pub struct SendFlags\|pub struct DeadlockCycle\|pub enum DeadlockDirection\|trait UserCopy\|pub struct KernelUserCopy\|pub fn delivermsg\|fn caller_q_push\|fn caller_q_find\|pub struct IpcEngine\|pub fn detect_deadlock\|pub fn send(\|pub fn receive(\|pub fn notify(\|pub fn sendrec(\|pub fn senda(\|fn mini_notify_core\|fn build_notify_message' os/kernel/src/ipc.rs
```

预期命中覆盖本篇引用的全部符号：枚举三个（93/146/189 区）、位标志结构（223 区）、死锁类型对（250/272 区）、拷贝抽象（294 区）、交付函数（742）、队列族（862/890 区）、引擎结构体（1000 区）与它的六个方法（1214/1309/1454/2087/2125/2217 区）、通知对（2859/2792 区）。行号会随代码漂移，锚点判读以符号为准。

---

## 7. 参见

- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.8（endpoint 编码与 generation 防伪）、§2.0（内核零堆存储模型——§3.1 零堆契约的前提）
- [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §4.3（kerninfo 页本体；查询臂的发布端在 08 侧同节尾部）、本篇 §4.4（查询臂）、[08] §3.2（BKL 取得点与持锁约定——§3.1 引用即见证的类型前提）
- [09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3（MEMREQ GET/REPLY 的 VM 拉取回信形状）——FROM_KERNEL 的推送形状在缺页臂（C `arch/earm/exception.c`）
- [10-switch-to-user.md](10-switch-to-user.md) §2.1（五标志循环的 DELIVERMSG 臂=delivermsg 的消费点）、§4.1（循环侧 misc flags 消费的 Rust 落点）
- [11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2 与 §4.2（RTS_SENDING/RECEIVING 的队列半：本篇讲位为什么变，那边讲位变化的队列后果）
- `13-syscall-dispatch`（trap 传参，尚未重生成）、`22-syscall-copy`（拷贝机制本体，尚未重生成）、`23-ipc-filter`（IPC 过滤决策，尚未重生成）、`24-cross-space-runtime`（VMSUSPEND 与跨空间拷贝运行时，尚未重生成）、`02-stage-sched` 方向（SCHED 服务器侧的调度原语消费）
- C 源码：`minix3/minix/kernel/proc.c:do_ipc`、`:do_sync_ipc`、`:deadlock`、`:mini_send`、`:mini_receive`、`:mini_notify`、`:try_deliver_senda`、`:try_async`、`:try_one`、`:delivermsg`、`minix3/minix/kernel/proc.h:P_BLOCKEDON`、`minix3/minix/kernel/ipc.h:WILLRECEIVE`、`minix3/minix/include/minix/ipcconst.h:SEND`、`minix3/minix/include/minix/com.h:NOTIFY_MESSAGE`、`minix3/minix/kernel/main.c:IPCNAME`
- Rust：`os/kernel/src/ipc.rs`（IpcCall/IpcOutcome/IpcError/SendFlags/DeadlockCycle/UserCopy/KernelUserCopy/delivermsg/caller_q_*/IpcEngine/mini_notify_core/build_notify_message）、`os/kernel/src/syscall.rs:fn dispatch_ipc`、`os/kernel/src/kerninfo.rs`、`os/arch/src/x86_64/boot.rs:fn set_secondary_ipc_return`
