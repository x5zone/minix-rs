# 13 · 系统调用分派：双腿的另一条与结果回写

> **C 真值源**：`minix3/minix/kernel/system.c`（`kernel_call`/`kernel_call_dispatch`/`kernel_call_finish`/`kernel_call_resume`/`call_vec`/`map`）、`minix3/minix/include/minix/com.h`（`KERNEL_CALL`、`SYS_*` 族、`NR_SYS_CALLS`、`SYSTEM`）、`minix3/minix/kernel/proc.h`（`MF_KCALL_RESUME`、RTS 位、`p_vmrequest`）、`minix3/minix/kernel/vm.h`（`VMSUSPEND`）、`minix3/sys/sys/errno.h`（`EDONTREPLY`/`EBADCALL`/`ECALLDENIED`/`EBADREQUEST`）、`minix3/minix/include/minix/bitmap.h`（`GET_BIT`）、`minix3/minix/kernel/arch/i386/mpx.S`（两腿的陷入入口汇编）、`minix3/minix/include/arch/i386/include/ipcconst.h`（陷入号族）、`minix3/minix/lib/libsys/kernel_call.c`（用户侧装号）。
> **Rust 实现**：`os/kernel/src/syscall.rs`（`Syscall`/`KcallResult`/`kernel_call`/`kernel_call_dispatch`/`kernel_call_dispatch_inner`/`kernel_call_finish` 族/`kernel_call_resume`/`dispatch_ipc_entry`/`syscall_leg_wire`）、`os/kernel/src/trap_dispatch.rs`（`x86_syscall_dispatch_body` 与 riscv64/aarch64 的 syscall 腿体）、`os/kernel/src/lib.rs`（调度循环 Stage 3a 的 KCALL_RESUME 消费块、`set_kbill_kcall_with`）、`os/kernel/src/vm.rs`（循环侧的 `kernel_call_resume` 读结果半、`VmSuspendState`）、`os/kernel/src/ipc_filter.rs`（`kcall_filter_check`）、`os/kernel/src/syscall_signal.rs`（`cause_signal`）、`os/kernel/src/proc_table.rs`（`process_misc_flags` 的同名臂兜底）、`os/libs/minix-types/src/ipc/kernel_call.rs`（调用号共享权威族）。
> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。引用位置由符号名定位（`path:func`、`path:fn NAME`、`path:enum NAME`、`path:struct NAME`），不使用行号；逐字引用块的块级区间题注带行区间与复算命令；§2.5/§3.4 归纳表内的行号是当前代码快照，锚点仍以符号名为准。
> **本篇主题**：用户态进入内核的第二条腿——syscall 指令腿（内核调用）从陷入到结果回写的完整周期：入口的回信地址登记与消息拷入、分派的三道门与表路由、完成半的 VMSUSPEND 识别与回复拷贝、被 VM 挂起的调用如何重派；以及 Rust 用 `enum Syscall`+`match`、六变体 `KcallResult` 与三函数拆分如何承接这套周期。
> **前置**：[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.1（注册时刻——enum 即表，本篇讲分派时刻）、§3.2（BKL 取得点）、§4.3（kerninfo 页的建立与发布）、[10-switch-to-user.md](10-switch-to-user.md) §2.1（五标志循环的 KCALL_RESUME 臂）、§3.3（Stage 3a 四防护——本篇讲内核侧 resume 本体）、[12-ipc-core.md](12-ipc-core.md) §2.1（int-33 腿的分派——本篇讲 syscall 指令腿）、§4.3（delivermsg 的两次失败策略）、[11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2 与 §4.2（RTS_SENDING/RECEIVING 的队列半——本篇讲位为什么变）、[09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3（MEMREQ 的 VMSUSPEND 消费端——本篇讲 VMSUSPEND 的生产端识别半）。
> **后续**：`14-exception-interrupt`（trap 入口汇编，尚未重生成）、`21-26` 方向（各内核调用的 handler 本体，尚未重生成）、`23-ipc-filter`（IPC 过滤决策，尚未重生成）、`24-cross-space-runtime`（VMSUSPEND 恢复协议与跨空间拷贝运行时，尚未重生成）。

[12-ipc-core.md](12-ipc-core.md) 讲完了第一条腿：int-33 陷入把 SEND/RECEIVE 一族原语送进 `do_ipc`，内核在两个进程之间转手消息。本篇讲另一条腿：同一段用户态代码改用 syscall 指令（或 `int 32`）陷入时，走的是完全不同的请求——"帮我建一个新进程""把这段内存清零""把我的优先级改掉"。这类请求的共同点是收件人不是任何进程，而是内核自己；它们从陷入到用户拿到结果，要经过入口拷贝、三道门、handler 执行、结果回写四个阶段，中途还可能被 VM 拽去处理坏页、挂起、再重派。

本篇走完这条腿的全周期：C 的四个函数怎么写（第 2 章）、Rust 用什么形状承接（第 3 章）、逐函数怎么落地（第 4 章）、靠什么测试钉住（第 5 章）、卡住时怎么查（第 6 章）。

---

## 1. 概述

### 1.1 双腿的另一条：内核调用与 IPC 原语的分野

MINIX 的用户态→内核请求有两条独立的陷入通道。陷入号表把这两条腿写在同一张头文件里：

```c
// minix3/minix/include/arch/i386/include/ipcconst.h —— 逐字引用（1-12 行，未删节）
#ifndef _I386_IPCCONST_H_
#define _I386_IPCCONST_H_

#define KERVEC_INTR 32     /* syscall trap to kernel */
#define IPCVEC_INTR 33     /* ipc trap to kernel  */

#define KERVEC_UM 34     /* syscall trap to kernel, user-mapped code */
#define IPCVEC_UM 35     /* ipc trap to kernel, user-mapped code  */

#define IPC_STATUS_REG		bx

#endif  /* _I386_IPCCONST_H_ */
```

（区间复算：`sed -n '1,12p' minix3/minix/include/arch/i386/include/ipcconst.h | wc -l` = 12。锚点：`minix3/minix/include/arch/i386/include/ipcconst.h:KERVEC_INTR`、`:IPCVEC_INTR`、`:KERVEC_UM`、`:IPCVEC_UM`。）

32 号（与它的 user-mapped 变体 34 号）通向内核调用，33 号（与变体 35 号）通向 IPC——12 篇的 `do_ipc` 就是 33 号腿的入口（`minix3/minix/kernel/arch/i386/mpx.S` 的 `syscall_sysenter_common` 段按 `%edi` 里的 `IPCVEC_UM`/`KERVEC_UM` 把两条腿分向 `do_ipc` 与 `kernel_call`）。两条腿的分野不在实现细节，而在语义：IPC 是"进程 ↔ 进程"的消息转手，内核自己只是搬运工；内核调用是"进程 → 内核服务"的请求-回复，内核自己是干活的人。前者问"你帮我把这条消息递给他"，后者问"你替我做这件事"。

第二条腿的请求号有自己的编址空间。基址与向量大小声明在 `com.h` 的内核调用段：

```c
// minix3/minix/include/minix/com.h:KERNEL_CALL —— 逐字引用（201-205 行，未删节）
/* System library calls are dispatched via a call vector, so be careful when 
 * modifying the system call numbers. The numbers here determine which call
 * is made from the call vector.
 */ 
#define KERNEL_CALL	0x600	/* base for kernel calls to SYSTEM */ 
```

（区间复算：`sed -n '201,205p' minix3/minix/include/minix/com.h | wc -l` = 5。锚点：`minix3/minix/include/minix/com.h:KERNEL_CALL`。）

用户态的包装函数把请求号装进消息头再陷入：

```c
// minix3/minix/lib/libsys/kernel_call.c:_kernel_call —— 逐字引用（7-21 行，未删节）
int _kernel_call(int syscallnr, message *msgptr)
{
  int t, r;
  t = 1;
  while(1) {
      msgptr->m_type = syscallnr;
      do_kernel_call(msgptr);
      r = msgptr->m_type;
      if(r != ENOTREADY) {
          break;
      }
      tickdelay(t++);
  }
  return r;
}
```

（区间复算：`sed -n '7,21p' minix3/minix/lib/libsys/kernel_call.c | wc -l` = 15。锚点：`minix3/minix/lib/libsys/kernel_call.c:_kernel_call`。）

`msgptr->m_type = syscallnr` 这一行就是编址的装载点：调用号以 `KERNEL_CALL + 偏移` 的绝对值形式住进 `m_type`，随消息一起过陷入。内核侧在分派时减掉基座还原成偏移（§2.2），两侧共用同一个常数，谁也不必猜测对方的约定。`minix3/minix/kernel/system.c` 的文件头注释把整条腿的形状概括成一句："System calls are transformed into request messages"——一次内核调用在协议层就是一条发给 SYSTEM 的请求消息，做完之后再回一条结果消息。

"发给 SYSTEM"不是修辞。回信的发送者字段写死在完成半里——`minix3/minix/kernel/system.c:kernel_call_finish` 的 else 分支执行 `msg->m_source = SYSTEM`（§2.3 逐字），而 SYSTEM 这个端点是内核任务族的伪端点之一：

```c
// minix3/minix/include/minix/com.h —— 逐字引用（46-52 行，未删节）
/* Kernel tasks. These all run in the same address space. */
#define ASYNCM	((endpoint_t) -5) /* notifies about finished async sends */
#define IDLE    ((endpoint_t) -4) /* runs when no one else can run */
#define CLOCK  	((endpoint_t) -3) /* alarms and other clock functions */
#define SYSTEM  ((endpoint_t) -2) /* request system functionality */
#define KERNEL  ((endpoint_t) -1) /* pseudo-process for IPC and scheduling */
#define HARDWARE     KERNEL	/* for hardware interrupt handlers */
```

（区间复算：`sed -n '46,52p' minix3/minix/include/minix/com.h | wc -l` = 7。锚点：`minix3/minix/include/minix/com.h:SYSTEM`、`:KERNEL`。）

内核调用因此有一个 IPC 原语没有的收件人形状：目标恒为 SYSTEM 槽（`SYSTEM = (endpoint_t) -2`），用户态看到的回信永远"来自 SYSTEM"，不管真正干活的是 fork 路径还是时钟路径。它不是发给某个进程的消息，是发给内核本身的请求；SYSTEM 是全部内核调用的共同伪接收者。

把 SYS_FORK 塞回 IPC 六原语的编址模型里，第一个卡住的就是目标端点：fork 的语义是"创建一个尚不存在的进程"，六原语的 `dest` 字段只能填一个活着（或即将收信）的进程端点，装不下"目标是内核里一个还没发生的动作"。反过来，IPC 也不该走内核调用通道——SEND/RECEIVE 的高频往返若每个都过三道门与 handler 表，协议层的轻量性就被请求层的重量拖住。两条腿各管一段语义，入口的分离不是历史包袱，是两种请求形状的边界。

两条腿的分工画成一张机制图，这就是全篇的行进地图：

```
              用户态的一条陷入指令，陷入号决定走哪条腿
                        │
        ┌───────────────┴───────────────────────────┐
        │ 33 / 35 号（IPCVEC_INTR / IPCVEC_UM）      │ 32 / 34 号（KERVEC_INTR / KERVEC_UM）
        ▼                                           ▼
  mpx.S: ipc_entry_common                    mpx.S: kernel_call_entry_common
        ▼                                           ▼
  do_ipc(r1, r2, r3)                          kernel_call(m_user, caller)
  （12 篇：SEND/RECEIVE/SENDREC/              （本篇五步：记地址→拷入→分派→
    SENDNB/NOTIFY/SENDA 六原语）                 记账→finish，§2.0 展开）
        │                                           │
  目标 = 另一个进程（端点编址，                  目标 = 内核本身（KERNEL_CALL+偏移
  回信来自对方进程）                            编址，回信恒来自 SYSTEM 槽）
        └───────────────┬───────────────────────────┘
                        ▼
            共同的出口 switch_to_user（10 篇）
```

本章收束一遍框架：用户态有两条陷入腿——33 号走 IPC 原语（12 篇），32 号走内核调用（本篇）；内核调用的请求号住在 `m_type` 里，以 `KERNEL_CALL + 偏移` 编址，向量大小 58；回信恒来自 SYSTEM 槽。第 2 章按这份地图逐个函数看 C 怎么落实"陷入→拷贝→权限→路由→回写"。

### 1.2 本篇范围与可以安全跳过的内容

| 主题 | 本篇怎么办 | 去处 |
|---|---|---|
| 各 SYS_* handler 的内部逻辑（fork/exec/copy/sig… 各自做什么） | 只讲到"路由到 handler"与"handler 返回值怎么回写"，handler 本体不展开 | `21-26` 方向（各 handler 篇，均尚未重生成） |
| trap 指令层的汇编入口（寄存器搬运、每 CPU 向量、KTS 样式记录） | 只从 `kernel_call(m_user, caller)` 的 C 签名讲起 | `14-exception-interrupt`（trap 入口汇编篇，尚未重生成）、`16-smp` 方向 |
| IPC 六原语的语义（SEND/RECEIVE/… 各自的停与不停） | 已在 12 篇讲完，本篇只把双腿并排画一次 | [12-ipc-core.md](12-ipc-core.md) §1.1、§2.1-§2.6 |
| IPC 过滤的决策面（谁能发给谁的链式规则） | 只讲权限门里 `k_call_mask` 这一位怎么测 | `23-ipc-filter`（过滤决策篇，尚未重生成） |
| VMSUSPEND 的恢复协议与 VM 侧（memreq 怎么服务、VM 怎么回复） | 只讲完成半里的识别与挂起半；恢复的循环侧消费只回指 | [09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3、[10-switch-to-user.md](10-switch-to-user.md) §3.3、`24-cross-space-runtime`（尚未重生成） |
| kerninfo 页的内容与布局 | 只在 §4.4 提 `MINIX_KERNINFO` 臂的交付形状 | [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §4.3、[12-ipc-core.md](12-ipc-core.md) §2.1 |
| 调度循环 Stage 3a 的四防护逐条 | 已在 10 篇 §3.3 立全，本篇只回指并把内核侧 resume 本体对上 | [10-switch-to-user.md](10-switch-to-user.md) §3.3 |

认知减负声明：本章不必深究各 SYS_* handler 的内部逻辑（`21-26` 方向）、trap 指令层汇编入口（`14-exception-interrupt` 方向）、过滤决策（`23-ipc-filter` 方向）、VMSUSPEND 恢复协议（`24-cross-space-runtime` 方向）、调度循环侧 resume 消费（10 篇 §3.3 已讲）——本篇保证"陷入→拷贝→权限→路由→回写"这条框架线索读完整。

---

## 2. C 源码分析

### 2.0 本章地图：kernel_call 的五步周期

内核调用的全部 C 实现集中在 `minix3/minix/kernel/system.c`（入口、分派、完成三个函数在文件头部，重派函数在 `sched_proc` 之前），围绕一个入口函数组织。`kernel_call` 是 syscall 腿的唯一入口（mpx.S 的 `kernel_call_entry_common` 段 `call kernel_call` 直接落到它，陷入时 BKL 已在手里——取得点在 `minix3/minix/kernel/main.c` 的启动段，[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.2 已立）；它按五个步骤把一次请求送完全程：记回信地址→拷入→分派→记账→finish。分派函数 `kernel_call_dispatch` 设三道门（范围、权限、表项非空），过了门就查 `call_vec` 表直接调用 handler；完成函数 `kernel_call_finish` 按返回值分两条路（VMSUSPEND 挂起、其余回写）；被挂起的调用由 `kernel_call_resume` 在 VM 服务完成后重派。画出五步周期与它的三个附属装置：

```
        kernel_call(m_user, caller)  ── syscall 腿唯一入口（同步执行，不排队）
            │
            ├─ ① 记回信地址    p_delivermsg_vir = m_user
            ├─ ② 拷入          copy_msg_from_user(m_user, &msg)
            │        └─ 失败 → printf + cause_sig(SIGSEGV) + return（③④⑤全不走）
            ├─ ③ 分派          kernel_call_dispatch(caller, &msg)
            │        ├─ 门一：call_nr 越界？        → EBADREQUEST
            │        ├─ 门二：k_call_mask 无此位？  → ECALLDENIED
            │        ├─ 门三：call_vec[call_nr] 空？→ EBADREQUEST
            │        └─ 过门 → result = handler(caller, msg)   ←同步调用
            ├─ ④ 记账          kbill_kcall = caller
            └─ ⑤ 回写          kernel_call_finish(caller, &msg, result)
                     ├─ result == VMSUSPEND：存 reqmsg + 置 MF_KCALL_RESUME
                     │        └─（VM 服务后）→ kernel_call_resume：
                     │             三断言 → 重派 dispatch（标志仍在）
                     │             → 清 MF_KCALL_RESUME → 再走 ⑤
                     └─ 其余：result != EDONTREPLY 才回拷
                              （m_source=SYSTEM，m_type=result → 用户缓冲）
```

这张图里有一个组织方式上的事实：第 ③ 步是同步的。`call_vec[call_nr](caller, msg)` 是一次直接的函数调用，没有中间队列、没有工作线程——handler 在调用者的内核栈上下文里执行，此时 BKL 持有中（启动时取得、约定恒持，见 [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.2）。handler 返回的 `int` 就是第 ⑤ 步手里的 `result`，中间没有第二段延迟。整条腿的全部异步性都压缩在一个点：handler 自己返回 VMSUSPEND 时，第 ⑤ 步把调用挂起来，等 VM 之后再从 `kernel_call_resume` 重进第 ③ 步。

向量表本身与它的装配宏：

```c
// minix3/minix/kernel/system.c:call_vec 与 :map —— 逐字引用（46-57 行，未删节）
/* Declaration of the call vector that defines the mapping of system calls
 * to handler functions. The vector is initialized in sys_init() with map(),
 * which makes sure the system call numbers are ok. No space is allocated,
 * because the dummy is declared extern. If an illegal call is given, the
 * array size will be negative and this won't compile.
 */
static int (*call_vec[NR_SYS_CALLS])(struct proc * caller, message *m_ptr);

#define map(call_nr, handler) 					\
    {	int call_index = call_nr-KERNEL_CALL; 				\
    	assert(call_index >= 0 && call_index < NR_SYS_CALLS);			\
    call_vec[call_index] = (handler)  ; }
```

（区间复算：`sed -n '46,57p' minix3/minix/kernel/system.c | wc -l` = 12。锚点：`minix3/minix/kernel/system.c:map`（`call_vec` 数组声明与 `map` 宏同住本块区间）。）

锚点：`minix3/minix/kernel/system.c:kernel_call`、`:kernel_call_dispatch`、`:kernel_call_finish`、`:kernel_call_resume`。

### 2.1 kernel_call 入口：回信地址、TOCTOU 防御与记账（本篇主走查一）

入口函数全貌如下（头注释与函数体共 35 行，全函数逐字）：

```c
// minix3/minix/kernel/system.c:kernel_call —— 逐字引用（129-163 行，未删节）
/*===========================================================================*
 *				kernel_call				     *
 *===========================================================================*/
/*
 * this function checks the basic syscall parameters and if accepted it
 * dispatches its handling to the right handler
 */
void kernel_call(message *m_user, struct proc * caller)
{
  int result = OK;
  message msg;

  caller->p_delivermsg_vir = (vir_bytes) m_user;
  /*
   * the ldt and cr3 of the caller process is loaded because it just've trapped
   * into the kernel or was already set in switch_to_user() before we resume
   * execution of an interrupted kernel call
   */
  if (copy_msg_from_user(m_user, &msg) == 0) {
	  msg.m_source = caller->p_endpoint;
	  result = kernel_call_dispatch(caller, &msg);
  }
  else {
	  printf("WARNING wrong user pointer 0x%08x from process %s / %d\n",
			  m_user, caller->p_name, caller->p_endpoint);
	  cause_sig(proc_nr(caller), SIGSEGV);
	  return;
  }


  /* remember who invoked the kcall so we can bill it its time */
  kbill_kcall = caller;

  kernel_call_finish(caller, &msg, result);
}
```

（区间复算：`sed -n '129,163p' minix3/minix/kernel/system.c | wc -l` = 35。锚点：`minix3/minix/kernel/system.c:kernel_call`。）

对照五步逐行读。第一步是函数体的第一行：`caller->p_delivermsg_vir = (vir_bytes) m_user`——在还没有做任何检查之前，先把用户态消息缓冲的地址记进进程表。这一行排在最前面有一个看得见的理由：finish 的回写按 `caller->p_delivermsg_vir` 找目标缓冲（§2.3 的 `copy_msg_to_user(msg, (message *)caller->p_delivermsg_vir)`），就算第 ② 步拷入失败、第 ③ 步被门拒绝，第 ⑤ 步只要执行回写，都靠这一行登记的地址找到用户缓冲。登记先于一切检查，回写才有出处。

第二步是整条腿的安全底座：`copy_msg_from_user(m_user, &msg)` 把用户缓冲的整条消息拷进内核栈上的局部变量 `msg`。此后 dispatch 与 finish 摸的都是这份内核副本——参数校验时读到的值与 handler 使用时读到的值是同一份字节。这类"先拷贝再处理"防的攻击面有通用的名字：TOCTOU（time-of-check to time-of-use，检查时值与使用时值不一致的竞态窗口）。设想把这句拷贝拿掉——把 `copy_msg_from_user` 换成 handler 直接拿着 `m_user` 指针读：校验通过后、handler 读书前，用户代码若在另一个 CPU 上改写同一段缓冲（或者干脆利用缺页让内核中途睡眠），handler 拿到的数据就与校验时不一致——放行时它是合法参数，使用时已经是别的字节。先拷入内核副本，用户此后怎么改都与本次调用无关。

拷入失败的分支同样值得逐行读：`printf` 一条 WARNING（把坏指针值、进程名、端点号全打出来）之后 `cause_sig(proc_nr(caller), SIGSEGV)`，然后 `return`。这个 return 砍掉的不只是 dispatch——第 ④ 步的 `kbill_kcall = caller` 与第 ⑤ 步的 `kernel_call_finish` 都不执行。拷不进来的消息没有分派的资格，也没有回写的必要；坏指针本身通过信号通道交给信号管理器处置。

第三步之前的 `msg.m_source = caller->p_endpoint` 是一处容易滑过去的赋值：进来的消息还没有发送者字段（它是用户填的，不可信），内核在这里用进程表里的权威端点把它盖上。这个字段此刻多干一件远期的事——被 VMSUSPEND 挂起的调用会把整条消息存进 `p_vmrequest.saved.reqmsg`，而 `kernel_call_resume` 的第三条断言正是 `saved.reqmsg.m_source == caller->p_endpoint`（§2.4）：入口盖的章，是重派时验明正身的凭据。

第四步 `kbill_kcall = caller` 在 dispatch 之后、finish 之前。注释写明动机："remember who invoked the kcall so we can bill it its time"——内核时间记账要归到发起调用的进程头上。它排在 dispatch 之后意味着：被三道门拒绝的调用也记（处理拒绝本身的内核时间仍是调用者的开销）；它排在 finish 之前且 EFAULT 分支不经过，意味着只有真正进了分派的调用才有记账标记。

还有一处字段复用要单独说：`p_delivermsg_vir` 不是内核调用腿的私有字段。IPC 的延迟交付也用同一个字段定位用户缓冲——`minix3/minix/kernel/proc.c:delivermsg` 按它拷出待交付消息（proc.c:270 附近），SENDA 的提交路径也写它（`caller_ptr->p_delivermsg_vir = (vir_bytes) m_buff_usr`，proc.c:983）。IPC 消息与内核调用回信是两种交付，共用同一个"这个进程当前的消息缓冲在哪"定位机制——字段复用是设计而非巧合，两种交付在进程表里只需要一个投递地址。

把入口的两处省略各推一遍，坏处各落在一个能指认的对象上。其一，把第 ② 步的拷入换成 handler 直读用户指针：TOCTOU 窗口打开，校验与使用读到不同字节，上一段已推过。其二，把第一行的 `p_delivermsg_vir` 登记删掉、改成 finish 时才从参数里取：finish 的签名里没有 `m_user`——它只拿 `caller` 与 `msg`，用户缓冲地址只存在进程表字段里；这条登记删掉，回写就无处可写，`copy_msg_to_user` 会拿一个上一次调用残留的旧地址当目标。

### 2.2 kernel_call_dispatch：三道门与表路由

分派函数全貌如下（33 行，全函数逐字）：

```c
// minix3/minix/kernel/system.c:kernel_call_dispatch —— 逐字引用（95-127 行，未删节）
static int kernel_call_dispatch(struct proc * caller, message *msg)
{
  int result = OK;
  int call_nr;

#if DEBUG_IPC_HOOK
	hook_ipc_msgkcall(msg, caller);
#endif
  call_nr = msg->m_type - KERNEL_CALL;

  /* See if the caller made a valid request and try to handle it. */
  if (call_nr < 0 || call_nr >= NR_SYS_CALLS) {	/* check call number */
	  printf("SYSTEM: illegal request %d from %d.\n",
			  call_nr,msg->m_source);
	  result = EBADREQUEST;			/* illegal message type */
  }
  else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr)) {
	  printf("SYSTEM: denied request %d from %d.\n",
			  call_nr,msg->m_source);
	  result = ECALLDENIED;			/* illegal message type */
  } else {
	  /* handle the system call */
	  if (call_vec[call_nr])
		  result = (*call_vec[call_nr])(caller, msg);
	  else {
		  printf("Unused kernel call %d from %d\n",
				  call_nr, caller->p_endpoint);
		  result = EBADREQUEST;
	  }
  }

  return result;
}
```

（区间复算：`sed -n '95,127p' minix3/minix/kernel/system.c | wc -l` = 33。锚点：`minix3/minix/kernel/system.c:kernel_call_dispatch`。）

第一行完成归一化：`call_nr = msg->m_type - KERNEL_CALL`——§1.1 里用户态装的绝对值（`0x600 + 偏移`）在这里减掉基座，还原成 0 到 57 的表下标。同一个减法也出现在装配宏 `map()` 里（§2.0 的 `int call_index = call_nr-KERNEL_CALL`）：注册侧与分派侧各减一次，两侧的下标语义因此对齐。

三道门按次序排成 if-else 链，各拦一类请求：

| 门 | 判定 | 拒绝动作 | 拦的是什么 |
|---|---|---|---|
| 一：范围 | `call_nr < 0 ‖ call_nr >= NR_SYS_CALLS` | printf "SYSTEM: illegal request" + `EBADREQUEST` | 编号根本不在向量里的请求 |
| 二：权限 | `!GET_BIT(priv(caller)->s_k_call_mask, call_nr)` | printf "SYSTEM: denied request" + `ECALLDENIED` | 编号合法但调用者无权发起 |
| 三：表项 | `call_vec[call_nr] == NULL` | printf "Unused kernel call" + `EBADREQUEST` | 有权但该号没有装配 handler |

权限门的位测试原语与位图本体：`GET_BIT` 定义在 `minix3/minix/include/minix/bitmap.h:GET_BIT`（`MAP_CHUNK(map,bit) & (1 << CHUNK_OFFSET(bit))`），掩码数组 `s_k_call_mask[SYS_CALL_MASK_SIZE]` 住在特权结构里（`minix3/minix/kernel/priv.h` 的 `struct priv`，bitchunk_t 数组，每个进程一份）。谁能开哪些位是注册期的事——boot 模板与 SYS_PRIVCTL 的授权路径各写一批（`minix3/minix/kernel/system/do_privctl.c`、`minix3/minix/kernel/main.c:kmain` 的模板下发段），分派期只读不写。

三道门的次序是代码里真实存在的形状，次序本身有可推导的约束。把次序倒过来排，先看最坏的一档：若把表项检查放在范围门之前——`call_vec[call_nr]` 的下标来自用户填的 `m_type` 减基座，越界的 `call_nr` 直接拿去读数组就是越界读，行为未定义；范围门的存在让这次读永远不会发生，所以它必须是第一道。若把权限门放在表项门之后：无权限的调用者会先探到"这个号是否装配了 handler"——表项门对空槽与非空槽给出不同的 printf 文案（"Unused kernel call" 只对过权限门的调用者出现），权限后置让无权者以试探方式区分装配表的内容。源码没有写这次序的动机陈述，上面两句是从代码形状推导出的次序约束；可以确认的部分是：越界位测试本身是未定义行为，范围门先于一切下标使用是硬约束。

第三道门的 NULL 臂对应向量里的空洞。46 个 `SYS_*` 号经 `map()` 装配（`grep -c 'KERNEL_CALL +' minix3/minix/include/minix/com.h` = 46），其余 12 个下标（11、12、20、29、30、37、38、41、42、47、48、49——从 `com.h:207-267` 的偏移逐号对照得出）从未分配过名字，`call_vec` 静态数组的这些槽保持 NULL。一个有权限的进程发起空洞号调用，走到第三道门拿到 `EBADREQUEST`——与范围门的错误码相同，但 printf 文案不同，前者说"编号非法"，后者说"这个号存在语义位置但没有 handler"。向量大小与基础调用集声明在常量段尾部：

```c
// minix3/minix/include/minix/com.h:NR_SYS_CALLS —— 逐字引用（269-278 行，未删节）
/* Total */
#define NR_SYS_CALLS	58	/* number of kernel calls */

#define SYS_CALL_MASK_SIZE BITMAP_CHUNKS(NR_SYS_CALLS)

/* Basic kernel calls allowed to every system process. */
#define SYS_BASIC_CALLS \
    SYS_EXIT, SYS_SAFECOPYFROM, SYS_SAFECOPYTO, SYS_VSAFECOPY, SYS_GETINFO, \
    SYS_TIMES, SYS_SETALARM, SYS_SETGRANT, \
    SYS_DIAGCTL, SYS_STATECTL, SYS_SAFEMEMSET
```

（区间复算：`sed -n '269,278p' minix3/minix/include/minix/com.h | wc -l` = 10。锚点：`minix3/minix/include/minix/com.h:NR_SYS_CALLS`、`:SYS_BASIC_CALLS`。）

门外的 `#if DEBUG_IPC_HOOK hook_ipc_msgkcall(msg, caller)` 是调试挂钩的编译开关位，默认编译不进去，不影响三道门的形状。

### 2.3 kernel_call_finish：VMSUSPEND 的识别与回复拷贝

完成函数全貌如下（35 行，全函数逐字）：

```c
// minix3/minix/kernel/system.c:kernel_call_finish —— 逐字引用（59-93 行，未删节）
static void kernel_call_finish(struct proc * caller, message *msg, int result)
{
  if(result == VMSUSPEND) {
	  /* Special case: message has to be saved for handling
	   * until VM tells us it's allowed. VM has been notified
	   * and we must wait for its reply to restart the call.
	   */
	  assert(RTS_ISSET(caller, RTS_VMREQUEST));
	  assert(caller->p_vmrequest.type == VMSTYPE_KERNELCALL);
	  caller->p_vmrequest.saved.reqmsg = *msg;
	  caller->p_misc_flags |= MF_KCALL_RESUME;
  } else {
	  /*
	   * call is finished, we could have been suspended because of VM,
	   * remove the request message
	   */
	  caller->p_vmrequest.saved.reqmsg.m_source = NONE;
	  if (result != EDONTREPLY) {
		  /* copy the result as a message to the original user buffer */
		  msg->m_source = SYSTEM;
		  msg->m_type = result;		/* report status of call */
#if DEBUG_IPC_HOOK
	hook_ipc_msgkresult(msg, caller);
#endif
		  if (copy_msg_to_user(msg, (message *)caller->p_delivermsg_vir)) {
			  printf("WARNING wrong user pointer 0x%08x from "
					  "process %s / %d\n",
					  caller->p_delivermsg_vir,
					  caller->p_name,
					  caller->p_endpoint);
			  cause_sig(proc_nr(caller), SIGSEGV);
		  }
	  }
  }
}
```

（区间复算：`sed -n '59,93p' minix3/minix/kernel/system.c | wc -l` = 35。锚点：`minix3/minix/kernel/system.c:kernel_call_finish`。）

函数按 `result == VMSUSPEND` 分成两条路。VMSUSPEND 是一个专门的伪错误码，不是 errno 值域的成员：

```c
// minix3/minix/kernel/vm.h:VMSUSPEND —— 逐字引用（5-8 行，未删节）
/* Pseudo error codes */
#define VMSUSPEND       (-996)
#define EFAULT_SRC	(-995)
#define EFAULT_DST	(-994)
```

（区间复算：`sed -n '5,8p' minix3/minix/kernel/vm.h | wc -l` = 4。锚点：`minix3/minix/kernel/vm.h:VMSUSPEND`、`:EFAULT_SRC`、`:EFAULT_DST`。）

handler 返回 -996 的含义写在分支注释里："message has to be saved for handling until VM tells us it's allowed"——handler 做到一半发现需要 VM 出手（典型：跨地址空间拷贝撞上未驻留页），VM 已经被通知了，内核必须等它的回复再重启这次调用。分支体做三件事：两条断言钉住挂起前提（`RTS_VMREQUEST` 已置——挂起标记由 `vm_suspend` 路径负责，`minix3/minix/kernel/proc.c:vm_suspend`；挂起类型是 `VMSTYPE_KERNELCALL`），然后把整条消息存进 `p_vmrequest.saved.reqmsg`、把 `MF_KCALL_RESUME` 标志立起来。存消息与立标志是重派的全部输入：VM 服务完之后，`kernel_call_resume` 靠标志认出"有调用要重派"，靠存下的消息重跑第 ③ 步。分支体里没有回复拷贝——挂起的调用此刻对用户态不存在，用户还在同步地停在陷入里。

else 分支处理全部其余返回值。先无条件执行 `caller->p_vmrequest.saved.reqmsg.m_source = NONE`——注释说这是"call is finished, we could have been suspended because of VM, remove the request message"：不管这次是不是从挂起恢复来的，把残留请求的发送者抹成 NONE，挂起登记作废。然后是第二个哨兵分岔：`result != EDONTREPLY` 才回写。EDONTREPLY 也是伪错误码（`minix3/minix/sys/sys/errno.h:EDONTREPLY`，`_SIGN 203`，注释自述"pseudo-code: don't send a reply"）——handler 已经自己把回复发出去了（或自己安排了交付），finish 不代发，否则同一次调用收到两条回复。三个伪错误码同段而居：

```c
// minix3/sys/sys/errno.h:EBADCALL —— 逐字引用（205-208 行，未删节）
#define EBADCALL     (_SIGN 209 )  /* illegal system call number */
#define ECALLDENIED  (_SIGN 210 )  /* no permission for system call */
#define ETRAPDENIED  (_SIGN 211 )  /* IPC trap not allowed */
#define EBADREQUEST  (_SIGN 212 )  /* destination cannot handle request */
```

（区间复算：`sed -n '205,208p' minix3/sys/sys/errno.h | wc -l` = 4。锚点：`minix3/sys/sys/errno.h:EBADCALL`、`:ECALLDENIED`、`:ETRAPDENIED`、`:EBADREQUEST`；`EDONTREPLY` 在同文件 `:EDONTREPLY`（199 行）。）

回写的三条语句是整条腿的出口契约：`msg->m_source = SYSTEM`（回信的"发件人"恒为 SYSTEM 槽，§1.1 的伪接收者）、`msg->m_type = result`（handler 的返回值**原样**变成用户看到的 `m_type`——errno 或成功码，中间没有翻译层）、`copy_msg_to_user(msg, (message *)caller->p_delivermsg_vir)`（按 §2.1 第一行登记的地址写回用户缓冲）。坏指针的处置与入口拷入的失败分支同文案：printf "WARNING wrong user pointer" + `cause_sig(SIGSEGV)`。入口与出口各有一次用户缓冲触达，两次的失败处置互为镜像。

VMSUSPEND 分支缺席的推演，两处各坏在一个能指认的对象上。其一，把 `if (result == VMSUSPEND)` 整个分支删掉：挂起等待 VM 的调用落进 else 路径，-996 被当作 errno 原样回写——`msg->m_type = -996`，用户态读到 -996 这个值域外的荒谬负数当返回码，而真正该做的事（挂起、登记、等 VM）一件都没发生；更糟的是 `MF_KCALL_RESUME` 没立、`reqmsg` 没存，这次调用永远无法重派，进程拿到一个假结果继续跑，状态与真实意图从此分叉。其二，把 `result != EDONTREPLY` 的检查删掉：handler 已自行回复的调用被 finish 再回一次——同一缓冲被写两遍，第二次写的是 `m_type = 203` 这样的伪错误码，用户态把哨兵当数据。

### 2.4 kernel_call_resume：重派本体

重派函数全貌如下（含分节横幅共 29 行，全函数逐字；`/* ... */` 包住的 printf 是源码里自带的注释掉的调试输出，按原文保留）：

```c
// minix3/minix/kernel/system.c:kernel_call_resume —— 逐字引用（609-637 行，未删节；内含源码注释掉的 printf，按原文保留）
/*===========================================================================*
 *                              kernel_call_resume                           *
 *===========================================================================*/
void kernel_call_resume(struct proc *caller)
{
	int result;

	assert(!RTS_ISSET(caller, RTS_SLOT_FREE));
	assert(!RTS_ISSET(caller, RTS_VMREQUEST));

	assert(caller->p_vmrequest.saved.reqmsg.m_source == caller->p_endpoint);

	/*
	printf("KERNEL_CALL restart from %s / %d rts 0x%08x misc 0x%08x\n",
			caller->p_name, caller->p_endpoint,
			caller->p_rts_flags, caller->p_misc_flags);
	 */

	/* re-execute the kernel call, with MF_KCALL_RESUME still set so
	 * the call knows this is a retry.
	 */
	result = kernel_call_dispatch(caller, &caller->p_vmrequest.saved.reqmsg);
	/*
	 * we are resuming the kernel call so we have to remove this flag so it
	 * can be set again
	 */
	caller->p_misc_flags &= ~MF_KCALL_RESUME;
	kernel_call_finish(caller, &caller->p_vmrequest.saved.reqmsg, result);
}
```

（区间复算：`sed -n '609,637p' minix3/minix/kernel/system.c | wc -l` = 29。锚点：`minix3/minix/kernel/system.c:kernel_call_resume`。）

函数是四步的直线。前三步是断言，各验一件挂起登记的完整性：槽没在回收（`!RTS_SLOT_FREE`——槽都自由了，没有可重派的调用者）；VM 已经服务完（`!RTS_VMREQUEST`——请求标志还在说明 VM 尚未回复，重派为时过早）；存下的消息确实是这个调用者发出的（`reqmsg.m_source == caller->p_endpoint`——§2.1 入口盖的章在这里兑现，消息不属于这个进程就说明挂起登记被写坏）。第四步本体是三行：重派 `kernel_call_dispatch(caller, &caller->p_vmrequest.saved.reqmsg)`——注意消息来源是进程表里存的那份，不是重新从用户空间拷；然后 `p_misc_flags &= ~MF_KCALL_RESUME` 清位，注释说清位的原因是"so it can be set again"——重派若再次撞上坏页（多页拷贝的第一页好了、第二页还坏），handler 再次返回 VMSUSPEND，finish 的 VMSUSPEND 分支要能把标志重新立起来；最后 `kernel_call_finish` 走完成半。重派与首次调用共享同一条完成路径。

`MF_KCALL_RESUME` 标志位的定义把"重派时标志仍在"写成了注释原文：

```c
// minix3/minix/kernel/proc.h:MF_KCALL_RESUME —— 逐字引用（237-242 行，未删节）
#define MF_KCALL_RESUME 0x008	/* processing a kernel call was interrupted,
				   most likely because we need VM to resolve a
				   problem or a long running copy was preempted.
				   We need to resume the kernel call execution
				   now
				 */
```

（区间复算：`sed -n '237,242p' minix3/minix/kernel/proc.h | wc -l` = 6。锚点：`minix3/minix/kernel/proc.h:MF_KCALL_RESUME`。）

重派调用与首次调用的唯一差别就在这个标志上。dispatch 三道门对此无感知——门只看编号、权限与表项；差异的消费者是 handler 自己：重派时 `MF_KCALL_RESUME` 仍在（清位动作排在 dispatch **之后**），handler 可以测它并跳过首次已完成的步骤。把清位与重派的先后对调一次：先清 `MF_KCALL_RESUME` 再调 dispatch——重派在 handler 眼里与首次调用完全相同，"已做过的步骤"无从识别：首次挂起前已扣掉的资源（已发出的请求、已登记的链入）在重派里再做一遍，多段挂起的每次恢复都重复一次半截工作。

### 2.5 本章归纳：内核调用总表

把本章四个函数的全景并成一张表，每个函数的失败分支与正常路径各占一行：

| 函数 | 步骤/分支 | 证据 | 出口 |
|---|---|---|---|
| `kernel_call` | ①记回信地址 | `p_delivermsg_vir = m_user`（system.c:141） | 登记进进程表，finish 凭它回写 |
| | ②拷入（TOCTOU 防御） | `copy_msg_from_user`（system.c:147） | 成功→③；失败→printf+SIGSEGV+return（③④⑤全不走） |
| | 盖发送者 | `msg.m_source = caller->p_endpoint`（system.c:148） | dispatch 看到可信来源；重派断言凭它验身 |
| | ④记账 | `kbill_kcall = caller`（system.c:160） | 分派后 finish 前，无条件（被拒也算调用者的内核工作） |
| `kernel_call_dispatch` | 门一：范围 | `call_nr < 0 ‖ >= NR_SYS_CALLS`（system.c:106） | EBADREQUEST |
| | 门二：权限 | `!GET_BIT(s_k_call_mask, call_nr)`（system.c:111） | ECALLDENIED |
| | 门三：表项 | `call_vec[call_nr] == NULL`（system.c:117-123） | printf "Unused kernel call" + EBADREQUEST |
| | 路由 | `result = (*call_vec[call_nr])(caller, msg)`（system.c:118） | 同步调用，返回值即 result |
| `kernel_call_finish` | VMSUSPEND 臂 | 断言 RTS_VMREQUEST+VMSTYPE_KERNELCALL（system.c:66-67） | 存 reqmsg + 置 MF_KCALL_RESUME，无回写 |
| | EDONTREPLY | `result != EDONTREPLY` 才回写（system.c:76） | 只清 reqmsg.m_source，不回拷 |
| | 回写臂 | `m_source=SYSTEM`、`m_type=result`、`copy_msg_to_user`（system.c:78-90） | 坏指针→printf+SIGSEGV |
| `kernel_call_resume` | 三断言 | system.c:616-619 | 任一不过=登记损坏，assert 停机 |
| | 重派（标志仍在） | system.c:630（清位在其后） | handler 可感知重试 |
| | 清位 + finish | system.c:635-636 | 回到完成半，与首次调用同路 |

带着这张表进第 3 章：C 把一个周期写在四个静态函数里，Rust 要在它的类型系统与锁纪律下重新安排同一批步骤。

---

## 3. Rust 设计决策

### 3.1 KcallResult 六变体与 C 整数哨兵的类型化（→§2.3）

C 里真实存在的语义是（§2.3）：`kernel_call_dispatch` 与 handler 链返回一个 `int result`，它的值域混装三类东西——正常 errno（`OK`/`EPERM`/`EFAULT`…）、伪错误码（`VMSUSPEND`=-996、`EDONTREPLY`=203）与簿记错误码（`EBADREQUEST`=212、`ECALLDENIED`=210）。完成半靠数值相等来分岔（`result == VMSUSPEND`、`result != EDONTREPLY`），类型系统对"哪些值是 errno、哪些是控制哨兵"一无所知。Rust 把这个值域收进枚举：

```rust
// os/kernel/src/syscall.rs:enum KcallResult —— 逐字引用（189-217 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
/// Kernel call dispatch result.
///
/// C: return values from kernel_call_dispatch() + kernel_call_finish()
/// - Positive/zero values: OK result code
/// - VMSUSPEND (-996): call needs VM assistance
/// - EDONTREPLY: no reply should be sent
/// - EBADREQUEST (212): invalid syscall number
/// - ECALLDENIED (210): no permission for system call
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KcallResult {
    /// Call completed with an **error** return code (C: `return(EPERM)` etc.).
    /// SYSCALL 腿线上交付时取负（见 `syscall_leg_wire`），用户态 `reply < 0` 拦截。
    Ok(i32),
    /// Call completed with a **data** return code (C: `VMPTYPE_CHECK` 等正值语义码）。
    /// SYSCALL 腿线上交付时**不取负**——用户态用 `reply == expected_data` 判别，
    /// 与 C 的 `m_type = result` 直接传递正值等价。NK4-C F10b：与 `Ok(i32)` 区分，
    /// 使 ENOENT（空队列数据信号）、VMPTYPE_CHECK（1）、GetPdbr 物理地址等正值
    /// 不被 F10 取负，从而保持 `reply == ENOENT` 等 C 语义的用户态判断有效。
    Data(i32),
    /// Call requires VM assistance (C: VMSUSPEND = -996).
    VmSuspend,
    /// No reply should be sent (C: EDONTREPLY).
    NoReply,
    /// Invalid or unimplemented syscall number (C: EBADREQUEST = 212).
    BadCall,
    /// Caller lacks permission for this system call (C: ECALLDENIED = 210).
    /// C: `!GET_BIT(priv(caller)->s_k_call_mask, call_nr)` — system.c:111
    CallDenied,
}
```

（区间复算：`sed -n '189,217p' os/kernel/src/syscall.rs | wc -l` = 29。锚点：`os/kernel/src/syscall.rs:enum KcallResult`。）

枚举与 C 哨兵的映射，一行对一个：

| C 的 `int result` 值 | `KcallResult` 变体 | 语义 |
|---|---|---|
| `OK` / errno 值（`return(EPERM)` 等） | `Ok(i32)` | 错误/成功码，回写给用户 |
| 正值语义码（`VMPTYPE_CHECK`、`GetPdbr` 物理地址等） | `Data(i32)` | 数据码，回写给用户且**不取负** |
| `VMSUSPEND`（-996） | `VmSuspend` | 挂起等 VM，不回写 |
| `EDONTREPLY`（203） | `NoReply` | handler 自行回复，不回写 |
| `EBADREQUEST`（212） | `BadCall` | 越界/空洞号（§2.2 门一、门三） |
| `ECALLDENIED`（210） | `CallDenied` | 权限位未开（§2.2 门二） |

枚举落地后有两个形状，各说一句。第一，`VmSuspend` 是显式变体，不是 `Ok/Err` 里的某个负数值——它不会到达用户态（finish 的挂起臂拦下它转成登记），把它混进 errno 值域是 C 的类型让步：-996 恰好不与任何 errno 冲突是事实上的幸运，不是类型上的保证。Rust 若把 `VmSuspend` 塞进 `Result` 的 `Err` 臂：每个拿到返回值的调用方都得先排除这个假错误才能看真 errno——控制流哨兵混进错误值域，检查义务就散给全部调用方。第二，`Ok` 与 `Data` 的分立接住了一类真实的线上事故：错误码与数据码共用 `i32`，`Ok(ENOENT)` 与"空队列的 ENOENT 数据信号"在数值上无法区分，线上取负约定会把后者误伤成错误（枚举注释记了这段事故与分立的来历，§5.2 的判别测试把它钉死）。分立之后，"回写时取不取负"由变体身份决定，不再由数值猜。

决定线上形式的两个方法与取负函数本体：

```rust
// os/kernel/src/syscall.rs:impl KcallResult::reply_wire —— 逐字引用（236-252 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
    /// SYSCALL 腿的**线上值**（F10b）：已按交付约定处理完毕，写入
    /// `frame.rax`/eager `m_type`/`set_ipc_return_code` 时直接使用，不再取负。
    ///
    /// - `Ok(code)`：错误码，线上取负（`syscall_leg_wire`）→ 用户态 `reply < 0` 拦截。
    /// - `Data(v)`：数据码（VMPTYPE_CHECK、ENOENT-as-empty、GetPdbr 地址等正值），
    ///   原样传递 → 用户态 `reply == expected` 判别，与 C 的 `m_type = result` 同义。
    /// - `BadCall`/`CallDenied`：簿记错误码，同样取负。
    /// - `NoReply`/`VmSuspend`：`None`（本腿不交付回执）。
    pub(crate) fn reply_wire(&self) -> Option<i32> {
        match self {
            KcallResult::Ok(ret) => Some(syscall_leg_wire(*ret)),
            KcallResult::Data(v) => Some(*v),
            KcallResult::BadCall => Some(syscall_leg_wire(EBADREQUEST)),
            KcallResult::CallDenied => Some(syscall_leg_wire(ECALLDENIED)),
            KcallResult::NoReply | KcallResult::VmSuspend => None,
        }
    }
```

（区间复算：`sed -n '236,252p' os/kernel/src/syscall.rs | wc -l` = 17。锚点：`os/kernel/src/syscall.rs:impl KcallResult::reply_wire`。）

```rust
// os/kernel/src/syscall.rs:const fn syscall_leg_wire —— 逐字引用（255-272 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
/// NK4-C F10（P0-wire）：SYSCALL 腿（kernel_call）回执的**线上形式**。
///
/// 内核内部统一携带正 errno（C `do_privctl` 等 handler 的 `return(EPERM)`
/// 同形）；但交付给用户态的那一腿按本项目 ABI 契约走**负 errno**——
/// `servers/rs/src/trap_api.rs` 头注第 2 条「`sys_*` 内核调用 wrapper：
/// 裸 i32，负 = errno」，用户态 21+ 处 `reply < 0` 门控都建在这条契约上
///（s13b 实证：privctl SET_SYS 的 EFAULT=14 以正数上线后被 `< 0` 门控
/// 吞掉，RS 静默把失败当成功，boot 尾 10 服务器卡 NO_PRIV）。
///
/// 与 C 的关系如实登记：C 线上是正 errno + 调用方 `!= OK` 判定
///（system.c:79 `msg->m_type = result` + libsys 全仓 `r != OK`）；minix-rs
/// 用户态选择负 errno 契约后，腿内取负是两边自洽的最小改动面（改动点
/// 集中在这一条腿的交付侧，而不是散落的用户态判定）。int33 陷阱腿
/// 不取负——它的 rax 车道携带 IPC 状态位（正 bitfield，见
/// `kernel_call_finish_ipc_door` 的门纪律文档）。
pub(crate) const fn syscall_leg_wire(code: i32) -> i32 {
    code.wrapping_neg()
}
```

（区间复算：`sed -n '255,272p' os/kernel/src/syscall.rs | wc -l` = 18。锚点：`os/kernel/src/syscall.rs:const fn syscall_leg_wire`。块内注释含仓库内部阶段编号，按原文引用。）

这一处要如实登记与 C 的差异，来龙去脉在 `syscall_leg_wire` 的文档注释里写全了：C 的线上形式是**正 errno**——finish 执行 `msg->m_type = result`，正数 errno 原样上线，C 用户态包装以"值不等于 OK"判定成败；minix-rs 的用户态契约选了**负 errno**（`os/servers/rs/src/trap_api.rs` 头注释："sys_* 内核调用 wrapper：裸 i32，负 = errno"，用户态 `reply < 0` 门控建在这条契约上，全仓 `grep -rn 'reply < 0' os/ | wc -l` = 66 处）。两边自洽的最小改动面是腿内取负：错误码（`Ok`/`BadCall`/`CallDenied`）上线取负，数据码（`Data`）原样传递以保住 C 语义的 `reply == expected` 判别。这不是"C 错了 Rust 改正"——C 的约定在自己的用户态里自洽；这是同一语义在两套用户态契约下的两种线上形式，取负发生在交付侧的一个点上（`reply_wire`），不散落在用户态。

反向推演这个类型化的缺省形状：若 `VmSuspend` 作 `Err` 变体——上一段已推（假错误检查散给调用方）；再把 `Data` 并回 `Ok`——正数据码在取负约定下被当错误码翻成负数，用户态的 `reply == expected` 判别全部失效，§5.2 的判别测试会当场变红。

### 3.2 kernel_call 的拆分：入口、分派与 finish 的三个函数（→§2.1/§2.2/§2.3）

C 里真实存在的语义是（§2.0）：`kernel_call` 一体五步，BKL 在陷入时就已在手里（启动期取得、约定恒持，[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.2），四个函数谁也不需要关心"锁从哪来"。Rust 的 BKL 是显式自旋锁，取得点必须落在某个函数里；落在哪个函数，决定了调度循环重派时能不能复用分派逻辑。Rust 把 C 的一体五步拆成三个函数，锁线从拆分形状里长出来：

| 函数 | 承接 C 的哪几步（§2.1-§2.3） | 锁动作 |
|---|---|---|
| `os/kernel/src/syscall.rs:fn kernel_call` | ①记地址 + ②拷入 + 盖 m_source（+④⑤经调用下属） | 不取锁（锁在 dispatch 里取） |
| `os/kernel/src/syscall.rs:fn kernel_call_dispatch` | ③分派 + ④记账 | `bkl_lock()` 取锁，`transfer()` 把锁交给后手 |
| `os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner` | 归一化 + 三道门 + 路由（③的纯逻辑半） | 不取锁——吃一个 `BklSection` 见证参数 |

分派函数的锁段全貌（锁的取、用、交三段都在这 31 行里）：

```rust
// os/kernel/src/syscall.rs:fn kernel_call_dispatch —— 逐字引用（584-614 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
pub fn kernel_call_dispatch(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    // Acquire BKL — C: BKL_LOCK() in mpx.S kernel_call_entry_common
    // R-03: Keep the guard alive and derive a BklSection witness for
    // compile-time BKL proof on global accessor calls (irq_manager_with, etc.).
    // R-05/B1: BklGuard is RAII (Drop releases BKL); transfer() hands
    // ownership to the ambient held-BKL scope because BKL must stay held
    // until kernel_call_finish() releases it.
    let bkl_guard = crate::smp::bkl_lock();
    let result = {
        let bkl_section = bkl_guard.section();
        kernel_call_dispatch_inner(caller_nr, proc_table, msg, priv_table, clock_state, &bkl_section)
    };
    // D-9 (C system.c:160) — the kernel call is in flight for `caller`:
    // attribute kernel time to it at the next context_stop. C sets the
    // marker after dispatch, before finish, unconditionally (a failed
    // call's kernel work is still the caller's); kernel_call_resume does
    // not re-set it.
    crate::set_kbill_kcall_with(caller_nr, &bkl_guard.section());
    // BKL is NOT released here — transfer() suppressed the guard's Drop.
    // BKL is released in:
    //   1. kernel_call_finish() — for normal completion (before switch_to_user)
    //   2. switch_to_user() — before returning to user mode
    bkl_guard.transfer();
    result
}
```

（区间复算：`sed -n '584,614p' os/kernel/src/syscall.rs | wc -l` = 31。锚点：`os/kernel/src/syscall.rs:fn kernel_call_dispatch`。块内注释含仓库内部阶段编号，按原文引用。）

拆分的约束集中在锁线上：`kernel_call_dispatch` 取锁并 `transfer()`（RAII 守卫的 Drop 被抑制，所有权交给"环境已持锁"的见证），锁的真正释放在 `kernel_call_finish`（正常完成）或 `switch_to_user`（10 篇 §3.4 的记账点）——transfer 的注释把这两个释放点写明了。`kernel_call_dispatch_inner` 不取锁，它把"当前已持锁"作为类型参数（`&BklSection`）写进签名：没有见证就调不了它，锁纪律从约定变成签名。

为什么取锁点必须在 dispatch 而不能上提到入口 `kernel_call`：调度循环的重派（Stage 3a，[10-switch-to-user.md](10-switch-to-user.md) §3.3）在**已持锁**的环境里执行——循环头一次 `assume_held`，重派若走任何打包了 `bkl_lock()` 的入口，非重入自旋锁当场自死锁（真机实测过一次：VM 服务完成后重派走了带锁入口，RS 挂死在两处之间、无任何输出——10 篇 §3.3 防护点二记录的实证）。把 `bkl_lock` 放在 dispatch、把纯逻辑放进 inner，重派才有"绕开取锁、复用门与路由"的入口可走。C 不需要这个拆分，因为 C 的锁从来不在分派路径上取；拆分是 Rust 锁模型落在同一条调用链上的形状变化，语义（分派全程持锁）不变。

finish 侧的对应拆分是一族三个函数：`os/kernel/src/syscall.rs:fn kernel_call_finish`（正常形式：持锁变体全参 `release_bkl=true, eager_reply_copy=true` 的薄包装）、`os/kernel/src/syscall.rs:fn kernel_call_finish_ipc_door`（int-33 门形式：`eager_reply_copy=false`——IPC 陷阱腿不做 eager 直写，§4.2 展开）、`os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl`（全参本体：调度循环重派以 `release_bkl=false` 调用，防止把循环的锁放掉）。C 的一个 finish 在 Rust 是三张签名，对应三种锁与门的组合场景；[10-switch-to-user.md](10-switch-to-user.md) §3.3 防护点三已引过持锁变体的调用侧，本篇 §4.2 讲本体。

反向推演这次拆分：若 finish 不分变体、一律放锁——调度循环重派路径上完成半放掉的是**循环的**锁，后续循环拍全部跑在无锁状态，持锁见证的签名链当场断裂（debug 断言 `bkl_is_locked` 会先喊）；若 finish 一律不放锁——trap 入口路径的锁无人释放，下一拍取锁自死锁。三个变体是两种释放点与两种门组合的交集，少一个都有调用方对不上。

### 3.3 dispatch_ipc_entry 与双腿的 BKL 同形（→§2.0）

C 里真实存在的语义是（§2.0 与 §1.1）：双腿在 C 里共享同一条锁约定——BKL 在 `minix3/minix/kernel/main.c:kmain` 的启动段一次取得（`BKL_LOCK()`，08 篇 §3.2 已立），此后整个内核在 BSP 上恒持锁运行，mpx.S 的两条陷入路径（`ipc_entry_common` 与 `kernel_call_entry_common`）进入时锁早已在手里，分派与完成全程持锁。这里要先修正一处 Rust 注释里的 C 指认，它出现在两个地方：`kernel_call_dispatch` 的锁段注释写"C: BKL_LOCK() in mpx.S kernel_call_entry_common"（§3.2 引文首行），`dispatch_ipc_entry` 的锁段注释写"C: BKL_LOCK() in mpx.S ipc_entry assembly"（§4.4 引文）——而本仓 C 的 mpx.S 里 `grep -n 'BKL_LOCK' minix3/minix/kernel/arch/i386/mpx.S` 零命中，C 的 `BKL_LOCK` 取得点：BSP 启动段一次取得（`minix3/minix/kernel/main.c:149`），AP/IPI 与时钟路径另有取得（`minix3/minix/kernel/smp.c`、`minix3/minix/kernel/arch/i386/arch_clock.c`、`minix3/minix/kernel/arch/i386/apic.c`、`minix3/minix/kernel/arch/i386/arch_smp.c`）——陷入分派路径（mpx.S）不在其中。注释指认的"分派期持锁"语义成立，指认的取得位置不成立；下面按 C 真码的形状讲。

Rust 的 BKL 不是"启动后恒持"，它要每个进入点自己取得。双腿各自的 Rust 入口于是长成同一个形状：**取锁→分派→transfer**。`kernel_call_dispatch` 的锁段已在 §3.2 逐字；int-33 腿的入口 `dispatch_ipc_entry` 的锁段与之同构（§4.4 全函数逐字）：`bkl_lock()` → `dispatch_ipc(...)` → `bkl_guard.transfer()`，transfer 后的注释逐字重复"released in kernel_call_finish or switch_to_user"两行。入口的文档注释把同形写明："Acquires BKL before dispatching (same pattern as `kernel_call_dispatch`)"（`os/kernel/src/syscall.rs:fn dispatch_ipc_entry` 的 BKL ownership 段）。

同形不是巧合的一致，是纪律的缝合：双腿共享同一条完成路径（finish 释放锁）与同一个调度出口（switch_to_user），锁的释方不区分来路——那么取方也不能有第二套时序。设想双腿各自设计取锁时机：int-33 腿若在 IPC 引擎内部某处取锁、syscall 腿在 dispatch 取——两腿的持锁区间错开，一腿 transfer 之后在"环境已持锁"假象下跑的代码，另一腿的取锁点恰好落在同一窗口时，自旋锁的持有者状态与见证签名对不上；共享的 finish 无法知道这次该按哪条腿的锁线放锁。同形让 finish 只需面对一种锁线。

C 到 Rust 的取锁层次变化也在这里如实登记：C 的"锁恒在手"让 mpx.S 汇编与四个 C 函数都不见锁；Rust 把"分派期持锁"这条语义从进程生命周期级的约定，改型为两个入口函数里的显式取得——汇编层不出现锁，锁住在 Rust 函数形状里。语义不变（分派+完成全程持锁），形状从全局约定收敛为局部签名。

### 3.4 处置表（→第 2 章各节）

C 的内核调用面逐项去向如下。每行给出验证方式——测试名见第 5 章，走查节见第 4 章：

| C 的东西（出处） | 处置 | Rust 对应 | 验证 |
|---|---|---|---|
| `call_vec[58]` 函数指针数组 + `map()` 运行时 assert（system.c:52-57） | **消失**（被 enum+match 吸收，08 篇 §3.1 已立） | `os/kernel/src/syscall.rs:enum Syscall` + `TryFrom<u16>` + `const` 编译期断言（判别式 0-57，空洞号直接不设成员） | `test_syscall_enum_tracks_minix_types_kernel_call_family`（§5.2）；编译期断言在 enum 定义后 |
| 46 个 `SYS_*` 宏（com.h:207-267） | 保留（共享权威化） | `os/libs/minix-types/src/ipc/kernel_call.rs` 的 46 个 `pub const`（曾缺位、后补齐的共享权威化产物——调度器本地镜像抄 `0x600+n` 的事故模式是登记动机，现全仓消费这一个权威） | 同上 pin 测试 46 对逐值对账 |
| `KERNEL_CALL` 基座 + `NR_SYS_CALLS`=58（com.h:205/270） | 保留 | `minix-types::KERNEL_CALL`、`minix-types::NR_SYS_CALLS`；`os/kernel/src/syscall.rs:const NR_SYS_CALLS` 另有内核侧副本 | §6.2 手算命令 |
| `kernel_call` 五步（§2.1） | 拆分 | `os/kernel/src/syscall.rs:fn kernel_call`（①②+盖章）+ `:fn kernel_call_dispatch`（③④）+ `:fn kernel_call_finish`（⑤） | §4.1 走查；`test_kernel_call_wrapper` |
| `copy_msg_from_user` + EFAULT→printf+SIGSEGV | 保留（信号路径统一） | `UserCopy::copy_msg_from_user` + `crate::syscall_signal::cause_signal(SIGSEGV)`——printf 的告警语义并入 cause_signal 的信号闭环（12 篇 §4.3 的 delivermsg 同族） | `test_kernel_call_wrapper` 的 UserCopy 注入 |
| 三道门（§2.2） | 保留（次序不变） | inner 的归一化→`Syscall::try_from`→`kcall_filter_check`→match 空洞吸收 | `test_kernel_call_dispatch_bad_call`/`_call_denied_no_priv`/`test_padconf_unused_kernel_call_replies_ebadrequest`（§5.2） |
| `s_k_call_mask` 位测试 | 改型 | `os/kernel/src/ipc_filter.rs:fn kcall_filter_check`（`KCallMask` 位集；无特权结构的调用者一律拒绝） | 同上 denied 测试 |
| `kernel_call_finish` 两分支（§2.3） | 保留 + 拆变体 | `:fn kernel_call_finish_holding_bkl`（VMSUSPEND 臂逐行对位）+ `:fn kernel_call_finish`/`:fn kernel_call_finish_ipc_door` 两个薄包装 | §4.2 走查 |
| `m_source=SYSTEM`/`m_type=result` 回写 | 改型（线上形式） | `reply.m_source = Endpoint::SYSTEM`；`m_type = reply_wire()`（错误码取负、数据码原样——§3.1 的契约差异） | `test_syscall_leg_wire_negates_nonzero_errno` |
| EDONTREPLY 哨兵 | 保留（改型为变体） | `KcallResult::NoReply`；finish 不回写 + trap 腿不写 RAX | `test_kcall_result_variants`；trap 腿 NoReply 早退（§4.2） |
| `kernel_call_resume` 四步（§2.4） | 拆分 | `:fn kernel_call_resume`（三断言+重派+清+finish 的全形）+ 调度循环 Stage 3a 块（生产重派路径，内联同样四步+两个 Rust 防护） | §4.3 走查；vm.rs 两条 resume 测试 |
| `p_vmrequest.saved.reqmsg` 挂起登记 | 改型 | `p_vm_suspend` 上下文的 `saved_msg`（`os/kernel/src/vm.rs:struct VmSuspendContext`，进程表字段经 `os/kernel/src/proc.rs` 引用） | vm.rs `memreq_reply_*` 族测试 |
| `kbill_kcall = caller`（§2.1 第④步） | 保留 | `crate::set_kbill_kcall_with`（`os/kernel/src/lib.rs:fn set_kbill_kcall_with`，dispatch 内、finish 前） | `test_kernel_call_dispatch_sets_kbill_marker`（§5.2） |
| 挂起后重派+完成半写 RAX（C 侧恢复只走消息回拷，RAX 交付为 64 位移植新形状） | 新增 | trap 腿 VmSuspend 臂：存现场、记样式、重入调度器；完成后 `set_ipc_return_code` 写 RAX（§4.2） | trap 腿臂走查（§4.2）；真机联调 |

共享权威的头段与基座常量（18 行，逐字）：

```rust
// os/libs/minix-types/src/ipc/kernel_call.rs —— 逐字引用（1-18 行，未删节）
//! Kernel-call number family — the single Rust authority for C's
//! `KERNEL_CALL` vector (`minix3/minix/include/minix/com.h:204-267`).
//!
//! Every `SYS_*` consumer (minix-sys wrappers, server transport mirrors,
//! kernel dispatch linkage) imports from here instead of hand-copying
//! `0x600 + n`. The numbers decide which entry of the kernel's call
//! vector fires, so a hand-copy drift is a wrong-kernel-call bug —
//! `E-MINTYPES-SYS` registered exactly that pattern in sched's local
//! mirror. Redox/Linux analogy: one uapi header, every consumer imports.
//!
//! Coverage: the full C family including gaps (call numbers 11/12, 20,
//! 29/30, 37/38, 41/42, 47-49 are unallocated in C and stay undefined
//! here — a consumer needing them is a ground-truth problem, not a
//! constant away). `SYS_BASIC_CALLS` moved here from `ipc/sysinfo.rs`
//! (it is a member selection of this family, not a sysinfo vocabulary).

/// Base for kernel calls to SYSTEM. C: `KERNEL_CALL` — com.h:205.
pub const KERNEL_CALL: i32 = 0x600;
```

（区间复算：`sed -n '1,18p' os/libs/minix-types/src/ipc/kernel_call.rs | wc -l` = 18。锚点：`os/libs/minix-types/src/ipc/kernel_call.rs:const KERNEL_CALL`。）

表的口径备注：46 对 pin 的成员数与 §2.2 的 46 个 `KERNEL_CALL +` 偏移宏互为同一集合（`grep -c 'KERNEL_CALL +' minix3/minix/include/minix/com.h` = 46；`grep 'pub const SYS_' os/libs/minix-types/src/ipc/kernel_call.rs | grep -cv BASIC` = 46——不带 `grep -cv BASIC` 会数出 47，多出的一项是 `SYS_BASIC_CALLS` 数组常量）；消失项只有 `call_vec` 数组与 `map` 宏一对——handler 函数本身全部活着，只是从"表项"改挂到 match 臂（按子模块分组：`os/kernel/src/syscall_process.rs`/`syscall_copy.rs`/`syscall_device.rs`/`misc.rs` 等，getinfo/privctl/vmctl/trace 四族留在 `syscall.rs` 大臂内）。

---

## 4. 实现详解

### 4.1 kernel_call 与 dispatch 走查（→§3.2）

Rust 入口对位 C 的五步，先看全函数（48 行，全函数逐字；块内注释含仓库内部阶段编号，按原文引用）：

```rust
// os/kernel/src/syscall.rs:fn kernel_call —— 逐字引用（535-582 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
pub fn kernel_call(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    m_user: minix_types::VirBytes,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
    user_copy: &dyn crate::ipc::UserCopy,
) -> KcallResult {
    // C system.c:141 — save the user-space reply address (kernel stores it
    // in the caller's p_delivermsg_vir; reply construction writes through it).
    proc_table
        .get_mut(caller_nr)
        .expect("kernel_call: caller slot must exist")
        .p_delivermsg_vir = m_user;

    // C system.c:147 — copy the message from user space (TOCTOU defense).
    let msg = match user_copy.copy_msg_from_user(m_user) {
        Ok(m) => m,
        Err(_) => {
            // C system.c:152-155 — printf WARNING + cause_sig(SIGSEGV).
            // Rust: route to cause_signal(SIGSEGV) — same signal closed
            // loop as D-45/D-43 in process_misc_flags.
            crate::syscall_signal::cause_signal(
                caller_nr,
                crate::syscall_signal::SIGSEGV,
                proc_table,
                priv_table,
            );
            return KcallResult::Ok(EFAULT);
        }
    };

    // C system.c:148 — stamp the sender's endpoint.
    let mut msg = msg;
    msg.m_source = proc_table
        .get(caller_nr)
        .map(|c| c.p_endpoint)
        .expect("kernel_call: caller slot must exist");

    // C system.c:149 — dispatch.
    let result = kernel_call_dispatch(caller_nr, proc_table, &mut msg, priv_table, clock_state);

    // C system.c:160 — kbill_kcall = caller (D-9, inside dispatch).

    // C system.c:162 — finish (VMSUSPEND / reply / BKL release).
    kernel_call_finish(caller_nr, proc_table, &msg, result, priv_table);
    result
}
```

（区间复算：`sed -n '535,582p' os/kernel/src/syscall.rs | wc -l` = 48。锚点：`os/kernel/src/syscall.rs:fn kernel_call`。引文注释尾行含仓库内部编号，按原文引用。）

对照 §3.2 的拆分表逐段对回 C：第一段写 `p_delivermsg_vir`（C system.c:141 的对位，注释自引行号）；第二段 `user_copy.copy_msg_from_user(m_user)` 失败时走 `crate::syscall_signal::cause_signal(caller_nr, SIGSEGV, ...)`——与 C 的 printf+`cause_sig(SIGSEGV)` 同闭环，差异只在告警的通道（C 先 printf 再发信号；Rust 的信号路径统一进 `os/kernel/src/syscall_signal.rs:fn cause_signal`，12 篇 §4.3 的 delivermsg 失败策略同族），然后返回 `KcallResult::Ok(EFAULT)`。注意这里与 C 的一个真实差异：C 的 EFAULT 分支 `return` 后 finish 不执行、kbill 不登记；Rust 的 `kernel_call` 在 EFAULT 时同样**不调** dispatch/finish（`return` 直接出函数），差异只在外层——trap 腿体拿到返回值后按 `reply_wire` 交付 RAX（§4.2），C 的 EFAULT 路径没有回执交付（信号接管）。返回值类型 `KcallResult` 让 EFAULT 能作为结果上行，这是拆分后的形状收益。

第三段盖章 `msg.m_source`（C system.c:148 对位）、第四段调 `kernel_call_dispatch`、然后 finish——C 的第④步（kbill）在 Rust 里搬进了 dispatch 内部（§3.2 的表），入口注释自证这一点（"C system.c:160 — kbill_kcall = caller (D-9, inside dispatch)"）。

分派的纯逻辑半在 inner，锁后的归一化与三道门（38 行，逐字；块内注释含仓库内部阶段编号，按原文引用）：

```rust
// os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner —— 逐字引用（631-668 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
    // R-16 (2026-08-12): SAFETY: `msg.m_type` is `i32`; the cast keeps the low
    // 16 bits. All legitimate kernel-call numbers are `< NR_SYS_CALLS` which
    // fits in u16, so valid calls survive intact. Any out-of-range (or
    // truncated) value is rejected by `Syscall::try_from` below as `BadCall`,
    // and in-range survivors are still gated by the per-caller `kcall_mask`.
    //
    // Base normalization (C mpx.S `subl $KERNEL_CALL` parity): libc issues
    // ABSOLUTE call numbers (com.h `SYS_*` = KERNEL_CALL + offset — the
    // E2/E6 wrapper precedent), while the dispatch tables index by the
    // RELATIVE offset. Host tests feed relative numbers directly; both
    // shapes normalize here — anything below the base passes through.
    let call_nr = if msg.m_type >= minix_types::KERNEL_CALL {
        (msg.m_type - minix_types::KERNEL_CALL) as u16
    } else {
        msg.m_type as u16
    };

    let syscall = match Syscall::try_from(call_nr) {
        Ok(s) => s,
        Err(()) => return KcallResult::BadCall,
    };
    // C: `else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr))` — system.c:111
    // Check if the caller has permission to invoke this system call.
    // Processes without an assigned privilege (priv_id == None) are denied
    // all kernel calls — this should not happen for running processes.
    //
    // Composed as `Option::and_then` + `is_none_or(...)`:  `is_none_or` has
    // the same semantics as `map_or(true, ...)` (None → deny, i.e. true):
    //   - `None` (no priv_id, or priv_id not in table) → deny (true)
    //   - `Some(priv)` → deny iff `kcall_filter_check` returns false
    let call_denied = proc_table
        .get(caller_nr)
        .and_then(|c| c.priv_id)
        .and_then(|id| priv_table.get(id))
        .is_none_or(|caller_priv| !kcall_filter_check(caller_priv, call_nr as u32));
    if call_denied {
        return KcallResult::CallDenied;
    }
```

（区间复算：`sed -n '631,668p' os/kernel/src/syscall.rs | wc -l` = 38。锚点：`os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner`。）

归一化这一段与 C 有两处要如实指出的差异。其一，引文注释写「C mpx.S `subl $KERNEL_CALL` parity」——本仓 mpx.S 里 `KERNEL_CALL` 零命中（§3.3 已登记同族），C 的减法实际发生在 `system.c` 的 dispatch 与 `map()` 宏里，语义对位成立、位置指认不成立。其二，形状差异：C 的 dispatch 无条件减基座（`call_nr = msg->m_type - KERNEL_CALL`，§2.2），负数落到范围门被拦；Rust 的归一化是条件式——`m_type >= KERNEL_CALL` 才减，低于基座的数原样当作相对号走 `try_from`。注释写明动机：宿主测试直接喂相对号，绝对号与相对号两种形状在此归一。C 的 `m_type` 是有符号 `int`，负的 m_type 减基座后落进范围门；Rust 的 `msg.m_type` 是 `i32`，条件归一后经 `as u16` 截断——截断的安全性由该注释自身的位数论证承担（合法号都 < 58，装得进 u16；越界/截断值被 `try_from` 以 `BadCall` 拒绝，活下来的还要过掩码门）。三道门在这里的对应：范围+空洞合并成 `Syscall::try_from`（越界与 12 个空洞号都返回 `Err(())` → `BadCall`）；权限门是 `kcall_filter_check`（`os/kernel/src/ipc_filter.rs:fn kcall_filter_check`——`Option` 链先取调用者特权结构，`is_none_or` 让"无特权"与"位未开"同判拒绝）；门三的 NULL 臂在 enum 形状下没有运行时对应——空洞号根本不是枚举成员，"装配表里的 NULL 槽"被编译期吸收，`test_padconf_unused_kernel_call_replies_ebadrequest` 钉的是它在非 ARM 架构上的对位（§5.2）。match 的 46 条臂把路由收敛成编译期穷尽，漏一条臂编译失败——C 的 NULL 检查在 Rust 分成"编译期（空洞号）+ trait 默认（架构不支持的成员返回 BadCall）"两层。

被拒调用的记账时点在 dispatch 里、inner 之外——重看 §3.2 的 dispatch 引文：`set_kbill_kcall_with` 排在 inner 返回之后、transfer 之前，注释写明"C sets the marker after dispatch, before finish, unconditionally (a failed call's kernel work is still the caller's)"。被门拒的调用在 Rust 里同样记账——`test_kernel_call_dispatch_sets_kbill_marker` 用一次 `CallDenied` 断言标记已在飞（§5.2）。

TOCTOU 防御的闭环与 12 篇同族：入口拷入（`UserCopy` 注入）→ handler 摸内核副本 → finish 的回写走另一条用户缓冲触达（§4.2）。两次触达的失败处置都在 `cause_signal` 家族里闭环。

### 4.2 finish 与 VMSUSPEND 走查（→§3.1/§3.2）

finish 的本体是 `kernel_call_finish_holding_bkl`，两个薄包装（`kernel_call_finish`/`kernel_call_finish_ipc_door`）各差一个布尔参数。本节按执行顺序走本体。VMSUSPEND 臂（29 行，逐字；块内注释含仓库内部阶段编号，按原文引用）：

```rust
// os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl —— 逐字引用（3513-3541 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
    // B1: the dispatch entry transferred a held BKL into this chain
    // (kernel_call_dispatch / dispatch_ipc_entry via `transfer()`); the
    // VmSuspend branch below releases it. A lost lock must fail loudly.
    debug_assert!(crate::smp::bkl_is_locked(), "kernel_call_finish entered without the BKL held");
    // VmSuspend path: save msg + set MF_KCALL_RESUME + release BKL.
    // C: system.c:60-63 — `if (result == VMSUSPEND) { saved.reqmsg = *msg;
    // p_misc_flags |= MF_KCALL_RESUME; }`
    if matches!(result, KcallResult::VmSuspend) {
        if let Some(ctx) = proc_table
            .get_mut(caller_nr)
            .expect("kernel_call_finish: caller slot must exist")
            .p_vm_suspend
            .as_mut()
        {
            ctx.saved_msg = Some(*msg);
            // 门纪律（NK4-C S3）：IPC 陷阱腿的挂起要把门标记带上——
            // 补完成时同样不得 eager 直写回执（否则恢复后向陈旧
            // p_delivermsg_vir 落写）。重派再次挂起时新建的 ctx 不继承
            // 此标记，门归属由 stage 3a 从旧 ctx 读出后以 eager=false
            // 重新落到新 ctx 上（见 lib.rs KCALL_RESUME 消费块）。
            if !eager_reply_copy {
                ctx.resume_skip_eager_reply = true;
            }
        }
        proc_table
            .get_mut(caller_nr)
            .expect("kernel_call_finish: caller slot must exist")
            .p_misc_flags
            .set(MiscFlagsBits::KCALL_RESUME);
```

（区间复算：`sed -n '3513,3541p' os/kernel/src/syscall.rs | wc -l` = 29。锚点：`os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl`。）

入口的 `debug_assert!(bkl_is_locked)` 是 Rust 侧对锁线的自查——C 无需此断言（锁恒在手），Rust 的持锁见证若断（打包入口漏取锁），这里先喊。VMSUSPEND 臂与 C 逐行对位：`ctx.saved_msg = Some(*msg)` 对 C 的 `saved.reqmsg = *msg`；`p_misc_flags.set(MiscFlagsBits::KCALL_RESUME)` 对 C 的 `|=`。C 的两条断言（`RTS_VMREQUEST`/`VMSTYPE_KERNELCALL`）在 Rust 由挂起登记的建立路径保证（`vm_suspend` 链负责置 RTS_VMREQUEST 与挂起上下文，09 篇 §2.3 的消费端对这条链），本体不再重复断言。臂内多出的 Rust 形状：`resume_skip_eager_reply` 门标记（int-33 门挂起的调用补完成时同样不得 eager 直写——门纪律随挂起上下文携带）与臂尾的 `vm_enqueue_and_notify_vm`（挂 VM 请求链并唤醒 VM——C 里这半段由 `vm_suspend` 完成，Rust 收进 finish 臂内）与 `release_bkl` 条件放锁（挂起即让出 CPU，锁必须放；调度循环重派形式带 `release_bkl=false` 进来，循环的锁不放）。C 的 VMSUSPEND 分支不回写用户缓冲——Rust 同：臂的 return 在回写代码之前。

非 VMSUSPEND 路径的统一半与回写构造（56 行，逐字；块内注释含仓库内部阶段编号，按原文引用）：

```rust
// os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl —— 逐字引用（3578-3633 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
    // Non-VmSuspend path (Ok / NoReply / BadCall / CallDenied):
    // C: system.c:64-89 — single else-branch handles all non-VMSUSPEND cases
    // uniformly: clear saved_msg + optional reply + (BKL released below).
    //
    // Previous implementation scattered this across 4 match arms with 4×
    // `bkl_unlock()` and 3× duplicated reply construction; the unified path
    // also fixes a latent bug where BadCall/CallDenied skipped
    // `saved_msg = None` cleanup (harmless in practice because BadCall/
    // CallDenied cannot follow a VmSuspend, but diverges from C semantics).
    if let Some(ctx) = proc_table
        .get_mut(caller_nr)
        .expect("kernel_call_finish: caller slot must exist")
        .p_vm_suspend
        .as_mut()
    {
        ctx.saved_msg = None;
    }

    // NoReply = the caller is now blocked. The IPC engine set the blocking
    // RTS flag with the primitive `RtsFlags::set` (it holds a procs slice,
    // not the run queues) — complete the block by dequeuing the caller,
    // the dequeue half of C's `RTS_SET` macro. Without this the blocked
    // caller stays queued and the scheduler spin-picks it forever
    // (observed on real machine: 86k picks of a receiver parked in
    // `RTS_RECEIVING`, test-sysboot C-27 carrier). Both IPC doors
    // (int-33 and the syscall leg) route their NoReply through here.
    if matches!(result, KcallResult::NoReply) {
        proc_table.dequeue_if_blocked(caller_nr);
    }

    if let Some(wire) = result.reply_wire().filter(|_| {
        // 门纪律（NK4-C S3，见 `kernel_call_finish_ipc_door` 文档）：
        // eager 回执直写只属于 kernel_call()/SYSCALL 腿；IPC 陷阱腿
        //（eager_reply_copy=false）与被它挂起的调用恢复重派
        //（resume_skip_eager_reply）都不写——错误已经随 RAX 交付，
        // 真回执由 MF_DELIVERMSG 投递机制负责。
        let door_skip = proc_table.get(caller_nr).is_some_and(|p| {
            p.p_vm_suspend
                .as_ref()
                .is_some_and(|c| c.resume_skip_eager_reply)
        });
        eager_reply_copy && !door_skip
    }) {
        let mut reply = *msg;
        reply.m_source = Endpoint::SYSTEM;
        // NK4-C F10b（P0-wire）：SYSCALL 腿线上值由 `reply_wire()` 给出：
        // 错误码（Ok）取负，数据码（Data，如 VMPTYPE_CHECK/ENOENT-as-data）原样传递。
        reply.m_type = wire;
        // C system.c:71-77 对位：结果消息 phys_copy 直写调用者用户缓冲
        // （p_delivermsg_vir），不经 DELIVERMSG。旧实现经 p_delivermsg+
        // DELIVERMSG 投递——调用者（VM）的 sef_receive 随即消费自己的
        // 回执（src=SYSTEM/type=errno）形成接收自旋，饿死其他进程
        // （NK4-A C-3 真机：VM exec 后 receive 空转、RS 永久饿死，
        // 2026-09-22）。
        let root = proc_table.get(caller_nr).map(|p| p.p_seg.phys_root);
        let buf_va = proc_table.get(caller_nr).map(|p| p.p_delivermsg_vir.0);
```

（区间复算：`sed -n '3578,3633p' os/kernel/src/syscall.rs | wc -l` = 56。锚点：`os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl`。）

逐段对回 C 的 else 分支（§2.3）：清残留（`ctx.saved_msg = None` 对 C 的 `reqmsg.m_source = NONE`）；EDONTREPLY 分岔改成 `reply_wire()` 的 `Option`——`NoReply`/`VmSuspend` 映射为 `None`，天然不回写；回写构造三行对位整齐：`reply.m_source = Endpoint::SYSTEM`（C 的 `msg->m_source = SYSTEM`）、`reply.m_type = wire`（C 的 `msg->m_type = result`，wire 按 §3.1 的取负契约）、随后的直写循环（C 的 `copy_msg_to_user`）。注释里那段直写与投递两种形状的对位说明解释了为什么必须直写：经投递机制交付回执的形状，会让调用者的 sef_receive 消费自己刚收到的回执、形成接收自旋——C 的 phys_copy 直写语义在 Rust 里逐字保留。`NoReply` 臂的出队（`dequeue_if_blocked`）是 C 之外的新形状：C 的阻塞语义由 RTS 位与队列宏在 handler 内完成（11 篇 §2.2 的 RTS_SET 跃迁检测），Rust 的 IPC 引擎持表不持队列，阻塞的出队半由 finish 补齐——不补则停排的进程留在就绪队列里被调度循环空转选中。

直写循环的骨架与它的物理翻译（16+13 行，逐字两段；两段之间的装置注释段略——见段后说明）：

```rust
// os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl —— 逐字引用（3645-3660 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
        if let (Some(root), Some(buf_va)) = (root, buf_va) {
            let bytes = core::mem::size_of::<minix_types::Message>();
            // NK4-C S2e 现场打印（task1-close 裁决删除）：RS 回执直写的
            // 每 chunk 目标与写入首字——去重探针会掩盖重复写，抹写
            // 收尾阶段需要无条件逐次证据（S2g 提到 4096：48 条在启动期
            // 耗尽，崩溃窗口无现场）。
            #[cfg(not(feature = "mock"))]
            #[cfg(target_arch = "x86_64")]
            let rs_trace = proc_table
                .get(caller_nr)
                .is_some_and(|p| p.p_endpoint.0 == 2);
            let mut off = 0usize;
            while off < bytes {
                let va = buf_va + off as u64;
                // 调用者页表翻译（其 CR3 未激活时经 DM 直写物理页）。
                match crate::pte_walk::walk_x86_64(root, minix_types::VirBytes(va)) {
```

（区间复算：`sed -n '3645,3660p' os/kernel/src/syscall.rs | wc -l` = 16。锚点：`os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl`。）

```rust
// os/kernel/src/syscall.rs:fn kernel_call_finish_holding_bkl —— 逐字引用（3713-3725 行，未删节）
                        let dm = <minix_arch::CurrentDirectMap as minix_arch::DirectMapArch>::kernel_phys_to_virt(minix_types::PhysBytes(pa.0)).0;
                        // SAFETY: DM 窗口覆盖全部物理内存；页为调用者
                        // 驻留的消息缓冲；跨页按页界分块。
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                (&reply as *const minix_types::Message as *const u8).add(off),
                                dm as *mut u8,
                                chunk,
                            );
                        }
                        off += chunk;
                    }
                    None => break, // 尾页未驻留：C phys_copy 同样静默失败
```

（区间复算：`sed -n '3713,3725p' os/kernel/src/syscall.rs | wc -l` = 13。锚点：同上。）

两段之间省略的是页界分块的算术——`chunk = min(bytes - off, 0x1000 - (va & 0xfff))`——与几段 `#[cfg]` 门控的调试探针：RS 回执直写的逐次打印、riscv64 的双走表对账，块内注释自述已按裁决裁撤到空体，本篇不展开。循环的语义与 C 的 `copy_msg_to_user`（phys_copy）对齐：调用者页表翻译（`pte_walk`），翻译成功经 DM（直接映射窗口）逐块拷贝，尾页未驻留则停——`None => break` 的注释自认这是 C phys_copy 的"静默失败"语义。C 版本此处失败会 printf+SIGSEGV（§2.3 的坏指针分支），Rust 把 SIGSEGV 的触发留给了后续触达同一段缓冲的路径——这一处是已知的行为差，宿主测不到（§5.3），真机上由调用者后续的合法访问补上信号。

完成半的锁释放与 trap 腿的交付。本体尾段按 `release_bkl` 放锁；交付的另一半在 trap 腿体里——`os/kernel/src/trap_dispatch.rs:fn x86_syscall_dispatch_body` 拿到 `kernel_call` 的返回值后把线上码写进 RAX。先看 RDI 进来的形状与返回值的交付点（13+12 行，逐字两段）：

```rust
// os/kernel/src/trap_dispatch.rs:fn x86_syscall_dispatch_body —— 逐字引用（1142-1154 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
    // User message pointer ABI: RDI (first SysV argument register).
    let m_user = VirBytes::new(frame.rdi);

    let table = unsafe { crate::proc_table_boot_unchecked() };
    // K20 (caller-by-nr): no laundering — the caller travels as its nr.
    let result = crate::syscall::kernel_call(
        cur_nr,
        table,
        m_user,
        unsafe { crate::priv_table_boot_unchecked() },
        unsafe { crate::clock_state_boot_unchecked() },
        &crate::ipc::KernelUserCopy,
    );
```

（区间复算：`sed -n '1142,1154p' os/kernel/src/trap_dispatch.rs | wc -l` = 13。锚点：`os/kernel/src/trap_dispatch.rs:fn x86_syscall_dispatch_body`。块内注释含仓库内部阶段编号，按原文引用。）

```rust
// os/kernel/src/trap_dispatch.rs:fn x86_syscall_dispatch_body —— 逐字引用（1156-1167 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
    // Reply code → RAX for the sysret leg. VmSuspend：调用已通过
    // vm_suspend 停排（RTS_VMREQUEST + memreq 入队 + VM 收 SIGKMEM）——
    // 保存完整上下文（RDI=消息指针经 saved_m_user 另存）、记返回样式、
    // 重入调度器；VM 服务完成后 KCALL_RESUME 阶段重派调用并经
    // set_ipc_return_code 交付 RAX（NK4-A C-3 迭代6，此前此臂 panic）。
    // NoReply 不在此腿出现（receive 型调用走 int-33）。
    // NK4-C F10b（P0-wire）：SYSCALL 腿线上取负由 `reply_wire()` 统一处理：
    // 错误码（Ok）取负，数据码（Data）原样传递；int33 腿不取负（rax 携带 IPC
    // 状态位语义，用户态按正 errno 消费）。
    match result.reply_wire() {
        Some(wire) => frame.rax = wire as i64 as u64,
        None => {
```

（区间复算：`sed -n '1156,1167p' os/kernel/src/trap_dispatch.rs | wc -l` = 12。锚点：同上。块内注释含仓库内部阶段编号，按原文引用。）

`match` 的 `Some` 臂把线上码写进 RAX；`None` 臂是 NoReply 与 VmSuspend 的共同入口，它的第一个出口是一条显式早退（6 行，逐字）：

```rust
// os/kernel/src/trap_dispatch.rs:fn x86_syscall_dispatch_body —— 逐字引用（1174-1179 行，未删节）
                return;
            }
            if !matches!(result, KcallResult::VmSuspend) {
                panic!("trap_dispatch: syscall returned {result:?} with no reply code");
            }
            {
```

（区间复算：`sed -n '1174,1179p' os/kernel/src/trap_dispatch.rs | wc -l` = 6。锚点：同上。）

两个出口之间的源注释把早退的来历写全：它对位 C 的 `result != EDONTREPLY` 判定——do_exit/SYS_EXIT 一类调用以 NoReply 返回、不写回返回寄存器，调用者状态机已由 dispatch 内的 cause_signal 推进；注释同时记录了一次真机证伪：早期实现假设 NoReply 不会出现在这条腿、直接 panic，真机的退出路径把假设打破后改为显式早退。注释含仓库内部工单号，本篇节引到此为止。RDI 载着用户消息指针进来（SysV 首参寄存器——i386 的 C 用 EAX 载同一物，`minix3/minix/lib/libc/arch/i386/sys/_do_kernel_call_intr.S` 的 `movl 4(%esp), %eax`；64 位是新移植，寄存器选择不继承 32 位），`kernel_call` 返回后 `reply_wire()` 决定 RAX：错误码取负、数据码原样、`None`（NoReply/VmSuspend）不写 RAX——NoReply 早退对位 C 的 `result != EDONTREPLY` 契约（调用者状态机已由 dispatch 内的 cause_signal 推进，`do_exit` 即此路）；VmSuspend 臂保存完整现场（RDI 另存进挂起上下文的 `saved_m_user`）、记返回样式、重入调度器——用户态的同步停等由"进程停排"表达，恢复后的 RAX 交付在 Stage 3a 重派完成时补上（下一节）。C 侧没有对应的寄存器交付臂：挂起恢复后结果仍由完成半的消息回拷送到用户缓冲（§2.4 的 finish 对位），返回寄存器不参与交付——这是 64 位移植新增的形状，处置表已登记。

syscall 腿与 int-33 腿在三架构上各有各的陷入载体，"机制各异、语义同一"在这张表里对齐（各列的裁定与锚点：`os/kernel/src/syscall.rs:fn dispatch_ipc_entry` 的 Architecture dispatch 文档段、`os/kernel/src/trap_dispatch.rs:fn x86_syscall_dispatch_body`）：

| 陷入源 | x86-64 | ARM64 | RISC-V |
|---|---|---|---|
| int-33 腿进 `dispatch_ipc_entry` | IDT 33 号门 → `ipc_entry_softint_orig` 汇编 | SVC 处理器读 r3，`r3 == IPCVEC_INTR` → `ipc_entry` 汇编 | ecall 处理器读 a7，`a7 < 17`（IPC 号 1-16） |
| syscall 腿进 `kernel_call` | SYSCALL 腿体，消息指针在 RDI | SVC 腿体 | ecall 腿体（`trap_dispatch.rs` 的 riscv64 kernel/user 双腿） |

三个列的陷入载体互不相同，承载的语义同一：陷入→存现场→按陷入号分腿→分派→结果交付→回调度器。Rust 不把三种陷入的 Mechanics 塞进分派框架——腿体的职责到"调 `kernel_call`/`dispatch_ipc_entry`、交付返回值"为止，三边的差异全部留在 `os/kernel/src/trap_dispatch.rs` 的三个腿体函数里。

### 4.3 resume 走查（→§2.4）

C 的 resume 四步（三断言→重派→清位→finish，§2.4）在 Rust 有两个承载者，先讲全形本体，再对生产路径。全形（45 行，全函数逐字）：

```rust
// os/kernel/src/syscall.rs:fn kernel_call_resume —— 逐字引用（3762-3806 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
pub fn kernel_call_resume(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
) {
    // C: system.c:616-619 — three invariants + our MF_KCALL_RESUME marker.
    // K20 caller-by-nr: the slot is re-borrowed for each read.
    let caller = proc_table
        .get(caller_nr)
        .expect("kernel_call_resume: caller slot must exist");
    debug_assert!(!caller.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE),
        "kernel_call_resume: caller slot is being freed");
    debug_assert!(!caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST),
        "kernel_call_resume: VM has not finished processing the fault");
    debug_assert!(caller.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME),
        "kernel_call_resume: MF_KCALL_RESUME not set (no prior VmSuspend)");

    // C: system.c:619 — `saved.reqmsg.m_source == caller->p_endpoint`.
    // The saved message must be sourced from this caller.
    // Using `expect` instead of `unwrap_or_default` so that an invariant
    // violation (missing p_vm_suspend or saved_msg) panics loudly rather
    // than silently dispatching an empty message — the original
    // `unwrap_or_default()` masked corruption bugs.
    let saved_msg = caller.p_vm_suspend.as_ref()
        .and_then(|ctx| ctx.saved_msg)
        .expect("kernel_call_resume: p_vm_suspend.saved_msg must exist \
                 (VmSuspend path in kernel_call_finish always sets it)");
    debug_assert_eq!(saved_msg.m_source, caller.p_endpoint,
        "kernel_call_resume: saved_msg.m_source mismatch");

    let mut msg_copy = saved_msg;

    // C: system.c:627-630 — re-execute the kernel call with MF_KCALL_RESUME
    // still set so the call handler knows this is a retry. The flag is cleared
    // *after* dispatch returns (system.c:635) so it can be set again on a
    // subsequent VMSUSPEND within the same call.
    let result = kernel_call_dispatch(caller_nr, proc_table, &mut msg_copy, priv_table, clock_state);
    proc_table
        .get_mut(caller_nr)
        .expect("kernel_call_resume: caller slot must exist")
        .p_misc_flags
        .clear(MiscFlagsBits::KCALL_RESUME);
    kernel_call_finish(caller_nr, proc_table, &msg_copy, result, priv_table);
}
```

（区间复算：`sed -n '3762,3806p' os/kernel/src/syscall.rs | wc -l` = 45。锚点：`os/kernel/src/syscall.rs:fn kernel_call_resume`。）

四步逐一对回 §2.4：断言半是三条 `debug_assert`（槽非 SLOT_FREE、无 VMREQUEST、KCALL_RESUME 已立）加一条存档校验（`saved_msg.m_source == caller.p_endpoint`——C 的第三条断言在 Rust 拆成"取存档时的 expect"与"验发送者"两段，注释写明为什么用 `expect` 不用 `unwrap_or_default`：缺存档是登记损坏，要炸响不要拿空消息派发）。重派半调 `kernel_call_dispatch`（**带锁的全形**——这与 C 的形状差在锁模型：C 的 resume 跑在恒持锁环境，重派无锁可谈；Rust 全形自带 `bkl_lock`，适用于无锁环境调用它）；清位半在 dispatch 之后（对 C system.c:635 的"清位在后、handler 可感知重试"）；finish 半与首次同路。它与生产路径的关系要如实披露：全仓检索 `syscall::kernel_call_resume` 与 `kernel::syscall::kernel_call_resume` 的调用形式，命中的只有 `os/kernel/src/proc_table.rs` 的一处注释（"The full re-dispatch (syscall::kernel_call_resume) is deferred to switch_to_user"，且该注释指引的 switch_to_user 现码并不调用它）——**全形本体目前没有生产调用方**，它是对 C 形状的完整对位与可测形态；生产重派在调度循环的 Stage 3a 块内联完成。

生产路径（[10-switch-to-user.md](10-switch-to-user.md) §3.3 已立四防护，本篇只对内核侧本体）的执行序：VM 服务完成（`memreq_reply` 置 KCALL_RESUME、清 VMREQUEST，09 篇 §2.3 的对端）→ 被挂起进程被调度循环选中 → Stage 3a 消费块。看消费块的状态守卫与重派两段（14+31 行，逐字；块内注释含仓库内部阶段编号，按原文引用）：

```rust
// os/kernel/src/lib.rs:fn scheduler_loop 的 Stage 3a 消费块 —— 逐字引用（4115-4128 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
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
```

（区间复算：`sed -n '4115,4128p' os/kernel/src/lib.rs | wc -l` = 14。锚点：`os/kernel/src/lib.rs` 的调度循环 Stage 3a 消费块（`os/kernel/src/lib.rs:fn scheduler_loop` 内）。）

```rust
// os/kernel/src/lib.rs:fn scheduler_loop 的 Stage 3a 消费块 —— 逐字引用（4146-4176 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
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
```

（区间复算：`sed -n '4146,4176p' os/kernel/src/lib.rs | wc -l` = 31。锚点：同上。）

与 C 四步的对位关系：C 的三断言在此拆成两半——状态守卫（`VmSuspendState::Completed` 才重派，`Pending`/`Fetched` 说明 VM 还在服务，本轮 `continue` 让 CPU，多段挂起是合法形状——10 篇 §3.3 防护点一）对应"VMREQUEST 已清"断言的语义半，`vm::kernel_call_resume`（`os/kernel/src/vm.rs:fn kernel_call_resume`——读结果+清位半）里的 `debug_assert` 承接其余断言；重派用 `kernel_call_dispatch_inner` + 循环的锁见证（`&section`）——§3.2 记录的真机死锁实证：带锁全形在已持锁的循环里重派就是自死锁；清位在 `vm::kernel_call_resume` 内完成（先于重派——与 C 的"清位在 dispatch 后"次序不同：这里读出的 VM 结果决定成败，标志的清除与重派解耦，重派若再挂起由 finish 的 VMSUSPEND 臂重新置位，§4.2 臂内的门标记重置注释即此）；finish 用持锁变体（`release_bkl=false`）——C 的 resume 尾部 finish 对位完成。完成后的 RAX 交付（`set_ipc_return_code`）是 C 没有的臂：C 的 32 位腿不重写返回寄存器（结果走消息回拷），64 位 Rust 的重派完成把线上码写回 RAX（§4.2 trap 腿的对称半）。生产路径清位先于重派（本节开头登记的次序差异）还有一个 C 侧推演没有覆盖的余问：清位后 handler 拿不到「这是重试」的标志，重试感知由谁承担——答案在挂起上下文里：`p_vm_suspend` 的 `saved_msg` 与 `state` 本身就是重试证据（handler 读到的 `msg` 来自存档而非用户缓冲，`VmSuspendState::Completed` 携带 VM 的裁决），重派语义由数据来源而非标志位承载。

全形与生产形各自的对错判据都在测试里：全形有 vm.rs 的两条状态机测试（§5.2），生产形由 Stage 3a 的守卫逻辑与 10 篇 §4.1 的循环走查覆盖。

### 4.4 dispatch_ipc_entry 走查（→§3.3）

int-33 腿的 Rust 入口全函数（37 行，全函数逐字；块内注释含仓库内部阶段编号，按原文引用）：

```rust
// os/kernel/src/syscall.rs:fn dispatch_ipc_entry —— 逐字引用（751-787 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
pub fn dispatch_ipc_entry(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &mut Message,
    priv_table: &mut PrivTable,
) -> KcallResult {
    // Decode IPC call number from m_type.
    // C: proc.c:602 — `int call_nr = (int) r1;` where r1 is the syscall
    // number register. In Rust, m_type holds the call number (set by arch
    // trap entry before calling this function).
    let call_nr = msg.m_type;
    let ipc_call = match crate::ipc::IpcCall::from_raw(call_nr) {
        Some(c) => c,
        None => return KcallResult::Ok(crate::errno::EBADCALL),
    };

    // Extract caller_idx before borrowing the procs slice (FIX-21, Phase
    // 1C: avoids split-borrow aliasing).
    let caller_idx = crate::proc_table::nr_to_idx(caller_nr)
        .expect("dispatch_ipc_entry: caller_nr out of range") as usize;

    // Acquire BKL — C: BKL_LOCK() in mpx.S ipc_entry assembly.
    // R-05/B1: the guard is RAII; `transfer()` hands ownership to the
    // ambient held-BKL scope because BKL must stay held until
    // kernel_call_finish() releases it.
    let bkl_guard = crate::smp::bkl_lock();
    // `caller` is derived from `proc_table` by the trap entry (raw-pointer
    // global access); passing `proc_table` (not a slice) lets dispatch_ipc
    // run the scheduler-aware sig_delay_done protocol.
    let result = dispatch_ipc(proc_table, caller_idx, msg, priv_table, ipc_call);
    // BKL is NOT released here — transfer() suppressed the guard's Drop.
    // BKL is released in:
    //   1. kernel_call_finish() — for normal completion (before switch_to_user)
    //   2. switch_to_user() — before returning to user mode
    bkl_guard.transfer();
    result
}
```

（区间复算：`sed -n '751,787p' os/kernel/src/syscall.rs | wc -l` = 37。锚点：`os/kernel/src/syscall.rs:fn dispatch_ipc_entry`。）

函数三段。解码段：`IpcCall::from_raw(call_nr)` 把 m_type 里的 1-16 号原语译成枚举，译不出（0、7-15、17+）直接 `KcallResult::Ok(EBADCALL)`——对位 C 的 `do_ipc` 默认分支（proc.c:602 附近，12 篇 §2.1 的入口走查），Rust 把"非法原语号"拦在进引擎之前，BKL 都不取。锁段：`bkl_lock()` → `dispatch_ipc` → `transfer()`——§3.3 的同形证据，transfer 后两行注释与 `kernel_call_dispatch` 逐字相同（"BKL is NOT released here…released in kernel_call_finish or switch_to_user"）。这里有一处锁段注释的 C 指认要按 §3.3 的结论重申：注释写"C: BKL_LOCK() in mpx.S ipc_entry assembly"，本仓 mpx.S 无 `BKL_LOCK`（grep 反证），C 的取得点在 `main.c:149`——同形语义成立，注释的位置指认不成立。执行段：`dispatch_ipc`（12 篇 §4.1-§4.4 已走查的 IPC 引擎入口，KERNINFO 臂在 [12-ipc-core.md](12-ipc-core.md) §4.4、SENDA 拆装在其 §4.3）跑完把结果交回，锁随 transfer 交给 finish 或 switch_to_user。

双腿至此合龙：两条腿在 C 里共享"陷入即持锁"的全局约定，在 Rust 里共享"入口取锁→分派→transfer→finish 放锁"的函数形状——§3.3 的同形在两个全函数逐字并排后可自行核对：`kernel_call_dispatch`（§3.2 引文）与 `dispatch_ipc_entry` 的锁段三行形状一致，transfer 注释两行逐字相同。

---

## 5. 测试要点

### 5.1 清单与读数（→第 4 章）

分派框架的直接测试住在三处：`os/kernel/src/syscall.rs` 的 `mod tests`（宿主单元测试，走 `--features mock` 形态）、`os/kernel/src/vm.rs` 的恢复状态机测试，以及 `os/kernel/src/trap_dispatch.rs` 的一组邻接测试（腿体本身的陷入半没有宿主测试直接调用，§5.3）。直接相关项按下表枚举：

| 测试（`os/kernel/src/`） | 钉住什么 | 走查 |
|---|---|---|
| `syscall.rs:test_kernel_call_dispatch_bad_call` | 越界号 → `BadCall`（门一） | §5.2 |
| `syscall.rs:test_kernel_call_dispatch_call_denied_no_priv` | 无特权 → `CallDenied`（门二） | §5.2 |
| `syscall.rs:test_kernel_call_dispatch_sets_kbill_marker` | 被拒调用也记账（④的时点） | §5.2 细读表 |
| `syscall.rs:test_kernel_call_wrapper` | 入口五步 + TOCTOU 拷入 + 地址登记 | §4.1 |
| `syscall.rs:test_kcall_result_variants` | 变体可构造性：Ok/VmSuspend/NoReply/BadCall/CallDenied 五个（Data 由 leg_wire 测试覆盖） | §3.1 |
| `syscall.rs:test_syscall_enum_tracks_minix_types_kernel_call_family` | enum 46 成员 ↔ minix-types 46 常量逐值对账 | §5.2 |
| `syscall.rs:test_padconf_unused_kernel_call_replies_ebadrequest` | 空洞/架构缺位号的 `BadCall`→212 对位（门三的编译期吸收） | §5.2 |
| `syscall.rs:test_syscall_leg_wire_negates_nonzero_errno` | 线上取负契约 + `Data` 原样传递 | §3.1 |
| `syscall.rs:test_dispatch_ipc_entry_*`（5 条）+ `test_dispatch_ipc_receive_ends_sender_sig_delay`（1 条，不匹配 `_entry_*` 前缀） | int-33 腿入口：坏号 EBADCALL、KERNINFO 两态、SEND 路由、BKL 取得；receive 完成时补发 SIGSNDELAY | §4.4 |
| `vm.rs:kernel_call_resume_returns_ok_on_success` | Completed→读结果+清位（恢复状态机） | §5.2 |
| `vm.rs:kernel_call_resume_returns_fault_on_failure` | Fault 结果的清位对称半 | §5.2 |
| `proc_table.rs:test_process_misc_flags_clears_kcall_resume` | 循环兜底臂的清位 | 10 篇 §4.1 |
| `trap_dispatch.rs:reply_code_none_is_a_wiring_bug_marker` | `reply_code()` 的 None 语义（NoReply/VmSuspend 不携带回执码） | §3.1 |

读数（全套件口径，实跑）：单元测试 `cargo test -p minix-kernel --features mock` → **832 passed / 0 failed / 8 ignored**；集成测试 `os/kernel/tests/boot_integration.rs` → 3 passed / 0 failed / 3 ignored；doc-test 1 ignored。分派框架直接相关项的计数口径按上表枚举：syscall.rs 单列 8 条 + `test_dispatch_ipc_*` 组 6 条（5 条 `_entry_*` + 1 条 receive_ends）= 14 条，加 vm.rs 两条、proc_table.rs 一条、trap_dispatch.rs 一条，共 18 条。检索只当入口，不当覆盖：`grep -c '#\[test\]' os/kernel/src/syscall.rs` = 44（本模块全部，其余 30 条属各 handler 的语义测试）；`grep -nE 'fn test.*(ipc_entry|wire|kcall|kernel_call)' os/kernel/src/syscall.rs | wc -l` = 14——它恰好多算 1 条不在本表的 `test_dispatch_schedule_niced_wire_bit_sets_mf_niced`（调度参数测试，名字带 wire）、漏算 1 条在本表的 `test_dispatch_ipc_receive_ends_sender_sig_delay`（名字不含 ipc_entry）；trap_dispatch.rs 的 13 条测试名一律不带 test_ 前缀，`grep -n 'fn test' os/kernel/src/trap_dispatch.rs` 返回 0 是检索陷阱而非零测试，按 `grep -c '#\[test\]'` 计才是 13。

过滤运行陷阱如实披露：`cargo test -p minix-kernel --features mock kernel_call` 按"测试名含 kernel_call"过滤，实跑 8 passed / 0 failed（832 filtered out）——它恰好漏掉 `test_kcall_result_variants`、`test_syscall_leg_wire_negates_nonzero_errno`、6 条 `test_dispatch_ipc_entry_*` 与 `test_process_misc_flags_clears_kcall_resume`（名字里没有"kernel_call"子串）。过滤读数只能当冒烟，不能当覆盖证据；本篇一律报全套件口径。

### 5.2 关键测试细读（→§3.1/§3.2/§4.3）

细读一：门二的权限拒绝。测试全函数（13 行，逐字）：

```rust
// os/kernel/src/syscall.rs:fn test_kernel_call_dispatch_call_denied_no_priv —— 逐字引用（4539-4551 行，未删节）
    #[test]
    fn test_kernel_call_dispatch_call_denied_no_priv() {
        // Process without priv_id should be denied
        let mut msg = Message::default();
        // proc.priv_id is None by default
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut clock_state = crate::clock::ClockState::new();
        let result = kernel_call_dispatch(ProcNr(0), &mut proc_table, &mut msg, &mut priv_table, &mut clock_state);
        assert_eq!(result, KcallResult::CallDenied);
        // Same as above: release BKL acquired by kernel_call_dispatch.
        crate::smp::bkl_unlock();
    }
```

（区间复算：`sed -n '4539,4551p' os/kernel/src/syscall.rs | wc -l` = 13。锚点：`os/kernel/src/syscall.rs:fn test_kernel_call_dispatch_call_denied_no_priv`。）

测试夹具的两个事实先行：`test_proc_table()` 建的 0 号进程默认 `priv_id: None`（注释自证），而 §4.1 的 inner 引文里权限门是 `Option` 链——`None` 走 `is_none_or` 的 None 臂直接判拒。断言 `KcallResult::CallDenied` 钉住门二的变体身份（不是某个 errno 数值）；尾部的 `bkl_unlock()` 是测试自我清理——dispatch 取了锁不还（§3.2 的 transfer 语义），只测分派的测试必须手动放锁，否则毒化后续测试的锁状态。变红条件：权限门被挪到归一化之后（越界号先崩）、`None` 特权改判放行、或门二返回值改成裸 errno——三者的任一都会让断言失败。

细读二：门三的编译期吸收在非 ARM 架构上的对位。测试全函数（22 行，逐字；块内注释含仓库内部阶段编号，按原文引用）：

```rust
// os/kernel/src/syscall.rs:fn test_padconf_unused_kernel_call_replies_ebadrequest —— 逐字引用（4973-4994 行，未删节）
    #[test]
    fn test_padconf_unused_kernel_call_replies_ebadrequest() {
        // C ground truth: the SYS_PADCONF map entry is `#if defined(__arm__)`
        // only (system.c:251-253) and do_padconf.c exists only under
        // arch/earm/. On every non-arm C build the call_vec entry is NULL and
        // kernel_call answers `EBADREQUEST` (system.c:120-123). The Rust
        // kernel has no arm32 target, so the trait default's `BadCall` —
        // which `reply_code()` maps to EBADREQUEST (212) — IS the C-parity
        // reply. This test pins both links of that chain.
        let msg = Message::default();
        let mut proc_table = crate::test_helpers::test_proc_table();

        let result = <CurrentArchSyscall as ArchSyscall>::dispatch_padconf(
            ProcNr(0),
            &mut proc_table,
            &msg,
        );
        assert_eq!(result, KcallResult::BadCall,
            "padconf on a non-arm32 kernel is the C NULL-entry case");
        assert_eq!(result.reply_code(), Some(EBADREQUEST),
            "user-visible reply must be EBADREQUEST (212), matching system.c:123");
    }
```

（区间复算：`sed -n '4973,4994p' os/kernel/src/syscall.rs | wc -l` = 22。锚点：`os/kernel/src/syscall.rs:fn test_padconf_unused_kernel_call_replies_ebadrequest`。）

这条测试钉的是两段链：`dispatch_padconf` 的 trait 默认实现返回 `BadCall`（x86-64 等 arch 不覆写 ARM 专属号——§3.4 处置表的 trait 默认形状），`reply_code()` 把 `BadCall` 映射成 212。对位关系在注释里写全了：C 的 `SYS_PADCONF` map 装配包在 `#if defined(__arm__)` 里（system.c 的装配段），非 ARM 的 C 构建里 `call_vec` 该槽是 NULL、分派走 "Unused kernel call" 臂回 212（§2.2 门三）；Rust 没有 arm32 目标，"该号无 handler"的语义由 trait 默认承担，`reply_code()==Some(EBADREQUEST)` 正是 C 门三的用户可见结果。变红条件：有人给 `Padconf` 补了非 ARM 的真实现、或 `reply_code()` 的 `BadCall` 臂改值。

细读三：VMSUSPEND 恢复的状态机半（生产路径的读结果+清位）。测试全函数（29 行，逐字）：

```rust
// os/kernel/src/vm.rs:fn kernel_call_resume_returns_ok_on_success —— 逐字引用（1927-1955 行，未删节；块内注释含仓库内部阶段编号，按原文引用）
    #[test]
    fn kernel_call_resume_returns_ok_on_success() {
        // K20 caller-by-nr: build the suspended state directly on the
        // caller's table slot (the standalone handle no longer applies).
        let mut table = crate::test_helpers::test_proc_table();
        {
            let proc = table.get_mut(ProcNr(0)).unwrap();
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            proc.suspend_for_vm(
                VmSuspendType::KernelCall,
                Endpoint::from_generation_slot(1, 99),
                VmCheckParams {
                    start: VirBytes::new(0x1000),
                    length: VirBytes::new(0x100),
                    write_flag: true,
                },
                None,
            );
            // The reply path cleared RTS_VMREQUEST before the resume.
            proc.p_rts_flags.clear(RtsFlagsBits::VMREQUEST);
            let ctx = proc.p_vm_suspend.as_mut().unwrap();
            ctx.state = VmSuspendState::Completed(VmCheckResult::Ok);
            proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        }

        let result = kernel_call_resume(ProcNr(0), &mut table);
        assert_eq!(result, VmCheckResult::Ok);
        assert!(!table.get(ProcNr(0)).unwrap().p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));
    }
```

（区间复算：`sed -n '1927,1955p' os/kernel/src/vm.rs | wc -l` = 29。锚点：`os/kernel/src/vm.rs:fn kernel_call_resume_returns_ok_on_success`。）

测试在 0 号槽上手工搭出"VM 已回复"的完整挂起现场：`suspend_for_vm` 建上下文、手动清 `VMREQUEST`（模拟 memreq_reply 的清标志半）、把状态推进到 `Completed(VmCheckResult::Ok)`、立 `KCALL_RESUME`——四个前置正是 C resume 三断言在 Rust 的状态面。断言两连：返回值等于 VM 的判定；标志已清（`vm::kernel_call_resume` 的清位半）。变红条件：清位挪到读结果之前（失败路径丢标志）、`Completed` 守卫放宽到 `Pending`（10 篇 §3.3 防护点一坏）、或返回值改从别处取。

细读四：enum 与共享权威的 46 对逐值对账。测试的判定循环（7 行，逐字）：

```rust
// os/kernel/src/syscall.rs:fn test_syscall_enum_tracks_minix_types_kernel_call_family —— 逐字引用（4048-4054 行，未删节）
        for (member, wire) in pairs {
            assert_eq!(
                KERNEL_CALL + member as i32,
                wire,
                "枚举成员 {member:?} 与共享权威 {wire:#x} 不符"
            );
        }
```

（区间复算：`sed -n '4048,4054p' os/kernel/src/syscall.rs | wc -l` = 7。锚点：`os/kernel/src/syscall.rs:fn test_syscall_enum_tracks_minix_types_kernel_call_family`。）

循环之前的 `pairs` 数组声明 `(Syscall, i32); 46`——46 对成员把 `Syscall` 全部变体与 `minix_types` 的 46 个 `SYS_*` 常量逐个配对（数组成员表见 `os/kernel/src/syscall.rs` 该测试全函数，46 行配对与 §2.2 的 46 个偏移宏同名同序）。断言式 `KERNEL_CALL + member as i32 == wire` 把两侧锁死：枚举判别式漂移或共享权威抄错，第一个不一致的成员带着名字爆出。变红条件：任一侧增删改号——两侧任一漂移即红，这正是 §3.4 处置表登记的"调度器抄号事故模式"的防复发钉。

### 5.3 测试边界与诚实披露（→第 3 章）

宿主单元测测不到的三段，各有明确的去处：

第一，真 syscall 指令往返。`os/kernel/src/trap_dispatch.rs:fn x86_syscall_dispatch_body` 及其 riscv64/aarch64 对应腿体在宿主 `--features mock` 测试里零调用（全仓检索 `x86_syscall_dispatch_body` 且 `--include='*.rs'`，命中只有三处形态：`trap_dispatch.rs` 的定义、`os/kernel/src/lib.rs` 的 init_protection 注册行、以及 `os/qemu-tests/test-kernels/kernel/bootstrap/` 下 test-timer-irq/test-paging-faultloop/test-proc-init 三个真机测试内核的 main.rs——宿主单元测试不在命中之列）。腿体需要真实的 trap 帧与调度器起的 `proc_ptr`，宿主测试直接调 `kernel_call`，跳过整个陷入半。指令级往返（用户态 `ecall`/`svc` → trap → 分派 → RAX 回读）由真机测试内核覆盖：`os/qemu-tests/test-kernels/user/rt-birth/src/main.rs` 的用户半经 SYS_DIAGCTL 打诊断行——那一次输出本身就是一次完整的内核调用往返，三架构真机门（`os/qemu-tests/README.md`）跑通它。

第二，VMSUSPEND 全链。挂起→VM 服务→重派→RAX 交付的整链需要 VM 服务器参与：宿主测试钉住了链的两端（finish 的 VMSUSPEND 臂靠 handler 级测试间接触达——`syscall_copy.rs:test_dispatch_memset_valid_process_returns_vm_suspend`、`syscall.rs:test_dispatch_getmcontext_user_target_suspends` 等；恢复端是 §5.2 的状态机测试），链的中间（VM 的 memreq 取数与回复，09 篇 §2.3/§4.1）与两端的衔接在宿主没有一次性走查。全链验证在真机回归。

第三，坏回写指针的信号差。C 的 `copy_msg_to_user` 失败会 printf+SIGSEGV（§2.3）；Rust 直写循环尾页未驻留时 `None => break` 静默停（§4.2 引文的注释自认"C phys_copy 同样静默失败"），SIGSEGV 留给调用者后续触达同段缓冲的路径补发。这一行为差在宿主测不到（需要真实页表翻译失败），如实登记为已知差异，验证归真机。

宿主/真机分界一句话：宿主测的是"分派框架的状态机与决策面"（枚举、门、finish 簿记、恢复读数），真机测的是"腿"（陷入、寄存器、页表、VM 在场）。两边的读数口径分开报，本篇 §5.1 只报宿主全套件。

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状 | 怀疑对象 | 确认命令与预期关键行 |
|---|---|---|
| 进程的内核调用被拒，拿到 -210（ECALLDENIED） | `s_k_call_mask` 对应位未开——调用者的特权模板没授权这个号，或 SYS_PRIVCTL 授权路径没走到 | C 侧串口行：`SYSTEM: denied request <nr> from <ep>.`（`minix3/minix/kernel/system.c:kernel_call_dispatch` 的 printf）。宿主侧复核门的形状：`cargo test -p minix-kernel --features mock test_kernel_call_dispatch_call_denied` 预期 `ok`；位图的授权写入点定位：`grep -n 's_k_call_mask' minix3/minix/kernel/system/do_privctl.c minix3/minix/kernel/main.c` 命中模板下发与 privctl 两条路径 |
| 拿到 -212（EBADREQUEST） | 门一（越界号）或门三（空洞号/架构缺位号）——两者同码不同因 | C 侧区分文案：`SYSTEM: illegal request`（门一）vs `Unused kernel call`（门三）。宿主侧：`cargo test -p minix-kernel --features mock test_kernel_call_dispatch_bad_call` 与 `cargo test -p minix-kernel --features mock test_padconf_unused_kernel_call` 双双预期 `ok`；空洞号清单手算见 §6.2 |
| 调用返回码到了，但回信缓冲内容不对/缺 | `p_delivermsg_vir` 错位（入口登记被中途覆盖）或用户缓冲坏指针 | 入口登记唯一写点核对：`grep -n 'p_delivermsg_vir =' os/kernel/src/syscall.rs` 预期只命中 `fn kernel_call` 内一行；宿主侧：`cargo test -p minix-kernel --features mock test_kernel_call_wrapper` 预期 `ok`（其中断言登记值等于传入地址）。C 侧坏指针串口行：`WARNING wrong user pointer 0x... from process <name> / <ep>.` |
| 被挂起的调用永不恢复，进程停排 | Stage 3a 状态守卫反复 `continue`（VM 未到 `Completed`）——VM 侧没回或回复链断（10 篇 §3.3 防护点一同族：`Pending`/`Fetched` 是合法等待，长期不动才是病） | 守卫读数定位：`grep -n 'state_completed' os/kernel/src/lib.rs` 命中 Stage 3a 守卫块；恢复端状态机：`cargo test -p minix-kernel --features mock kernel_call_resume` 预期 2 passed（vm.rs 两条）；VM 侧对端（memreq 回复置标志+清 VMREQUEST）走 09 篇 §2.3 的链路核对 |

四行的共同前提：宿主读数只能证明框架决策面，腿与 VM 在场的病要按 §5.3 的分界上真机（`os/qemu-tests/README.md` 的三架构回归）。

### 6.2 手算与常量核对命令

逐条实跑过，命令与输出如下：

```bash
$ grep -n 'define KERNEL_CALL' minix3/minix/include/minix/com.h
205:#define KERNEL_CALL	0x600	/* base for kernel calls to SYSTEM */

$ grep -c 'KERNEL_CALL +' minix3/minix/include/minix/com.h
46

$ grep -n 'define NR_SYS_CALLS' minix3/minix/include/minix/com.h
270:#define NR_SYS_CALLS	58	/* number of kernel calls */

$ grep 'pub const SYS_' os/libs/minix-types/src/ipc/kernel_call.rs | grep -cv BASIC
46

$ grep -n 'pub const KERNEL_CALL' os/libs/minix-types/src/ipc/kernel_call.rs
18:pub const KERNEL_CALL: i32 = 0x600;

$ grep -n 'enum KcallResult' os/kernel/src/syscall.rs
198:pub enum KcallResult {

$ grep -n 'pub fn kernel_call\|fn kernel_call_dispatch\|fn kernel_call_finish\|fn kernel_call_resume\|fn dispatch_ipc_entry' os/kernel/src/syscall.rs
535:pub fn kernel_call(
584:pub fn kernel_call_dispatch(
623:pub(crate) fn kernel_call_dispatch_inner(
751:pub fn dispatch_ipc_entry(
3458:pub fn kernel_call_finish(
3488:pub fn kernel_call_finish_ipc_door(
3504:pub(crate) fn kernel_call_finish_holding_bkl(
3762:pub fn kernel_call_resume(
```

两组数的口径说明：46（C 侧）与 46（Rust 侧）都是"带 `KERNEL_CALL +` 偏移的调用号常量"——Rust 侧若不带 `grep -cv BASIC` 会数出 47，多出的一项是 `SYS_BASIC_CALLS` 数组常量（成员选择，不是调用号）。空洞号手算：58 个下标减 46 个已定义号，缺的 12 个是 11、12、20、29、30、37、38、41、42、47、48、49（对 `minix3/minix/include/minix/com.h` 的 207-267 行逐号点名得出；`minix-types` 侧同 12 个号不设常量，`os/libs/minix-types/src/ipc/kernel_call.rs` 的模块注释自述"gaps … stay undefined"）。哨兵值核对：`sed -n '6p' minix3/minix/kernel/vm.h` → `#define VMSUSPEND       (-996)`；`EDONTREPLY`/`EBADCALL`/`ECALLDENIED`/`EBADREQUEST` 四值在 `minix3/sys/sys/errno.h:EDONTREPLY`（199 行起）与 `minix3/sys/sys/errno.h:EBADCALL`（205-208 行段）。

---

## 7. 参见

- [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.1（注册时刻——enum 即表，本篇讲分派时刻）、§3.2（BKL 取得点：`main.c:149` 一次取得、故意泄漏）、§4.3（kerninfo 页的建立与发布——`MINIX_KERNINFO` 臂交付的页从这来）、§5.1（kbill 记账测试的登记行）
- [10-switch-to-user.md](10-switch-to-user.md) §2.1（五标志循环的 KCALL_RESUME 臂——同名兜底臂）、§3.3（Stage 3a 四防护——本篇 §4.3 的生产重派路径在那边立全）、§3.4（finish_and_restore 的 BKL 释放点——`transfer()` 注释里两个释放点之一）
- [12-ipc-core.md](12-ipc-core.md) §2.1（int-33 腿的分派与 KERNINFO 臂）、§4.3（delivermsg 的两次失败策略——EFAULT/SIGSEGV 同族闭环）
- [11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2 与 §4.2（RTS_SENDING/RECEIVING 的队列半——NoReply 出队半的队列后果在那边）
- [09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3（MEMREQ 的 VMSUSPEND 消费端——本篇 §2.3/§4.2 的挂起识别是同一条链的生产端半）
- `14-exception-interrupt`（trap 入口汇编：mpx.S 两腿分向与寄存器搬运，尚未重生成）
- `21-26` 方向（各 SYS_* 的 handler 本体：fork/exec/copy/signal/device/vmctl，均尚未重生成）
- `23-ipc-filter`（IPC 过滤决策与 `k_call_mask` 的授权链，尚未重生成）
- `24-cross-space-runtime`（VMSUSPEND 恢复协议与跨空间拷贝运行时，尚未重生成）
- C 源码：`minix3/minix/kernel/system.c:kernel_call`、`:kernel_call_dispatch`、`:kernel_call_finish`、`:kernel_call_resume`、`:map`；`minix3/minix/include/minix/com.h:KERNEL_CALL`、`:NR_SYS_CALLS`、`:SYSTEM`；`minix3/minix/kernel/proc.h:MF_KCALL_RESUME`；`minix3/minix/kernel/vm.h:VMSUSPEND`；`minix3/minix/include/minix/bitmap.h:GET_BIT`；`minix3/minix/lib/libsys/kernel_call.c:_kernel_call`
- Rust：`os/kernel/src/syscall.rs`（`enum Syscall`、`enum KcallResult`、`fn kernel_call`、`fn kernel_call_dispatch`、`fn kernel_call_dispatch_inner`、`fn kernel_call_finish` 族、`fn kernel_call_resume`、`fn dispatch_ipc_entry`、`const fn syscall_leg_wire`）、`os/kernel/src/trap_dispatch.rs`（`fn x86_syscall_dispatch_body` 等三架构腿体）、`os/kernel/src/lib.rs`（调度循环 Stage 3a 消费块、`fn set_kbill_kcall_with`）、`os/kernel/src/vm.rs`（`fn kernel_call_resume` 循环侧半）、`os/kernel/src/ipc_filter.rs`（`fn kcall_filter_check`）、`os/libs/minix-types/src/ipc/kernel_call.rs`（调用号共享权威族）
