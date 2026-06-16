# runtime-design-ds: 03-stage-kernel 运行时文档规划

> **创建**: 2026-06-13
> **前置**: [kboot-new.md](kboot-new.md) — 01~07 覆盖 boot 阶段，07 结束于 `switch_to_user()`
> **范围**: `switch_to_user()` 之后，kernel 的全部运行时机制
> **方法**: 以「状态机」视角组织——进程有哪些状态、状态如何转换、转换由谁触发

---

## 0. 核心洞察：内核运行时是一个「事件驱动的状态机」

### 0.1 boot 与 runtime 的本质区别

boot 阶段（01~07）是**线性叙事**——kmain 从头到尾执行一遍，switch_to_user() 是终点。

runtime 阶段是**反应式系统**——内核不再有「主函数执行完毕」的概念。switch_to_user() 之后，内核进入一个永不退出的循环。所有内核代码的触发源只有三个：

| 触发源 | 入口点 | 本质 |
|--------|--------|------|
| **系统调用** | `sys_call` → `do_ipc()` / `kernel_call()` | 用户进程主动请求内核服务 |
| **硬件中断** | `irq_handle()` / `timer_int_handler()` | 外部设备异步通知 |
| **CPU 异常** | `exception_handler()` → `pagefault()` | 当前进程执行出错 |

这三个入口是内核代码的**全部执行路径**。没有「主循环」——内核代码总是在响应某个事件。

### 0.2 状态机视角

从这个视角看，内核运行时的核心问题是：**进程在哪些状态之间转换，每次转换由哪个入口触发。**

因此，理解内核运行时可以分为四层：

1. **状态定义层**：进程有哪些状态（RTS 标志位）、状态保存在哪里（`struct proc`）、谁有权限（`struct priv`）
2. **状态转换层**：调度器如何让进程「运行」或「等待」、IPC 如何让进程「阻塞」或「唤醒」
3. **事件源层**：系统调用（同步请求）、中断/异常（异步事件）如何触发状态转换
4. **服务层**：内核提供哪些具体服务（fork、safecopy、信号、VMCTL 等）

### 0.3 文档组织原则

遵循从底层到上层、从机制到服务的顺序：

```
08-process-state     ← 第一层：状态定义（谁、什么状态）
09-scheduling         ← 第二层：状态转换（如何选择谁运行）
10-sync-ipc           ← 第二层：状态转换（如何因通信而阻塞/唤醒）
11-exception-interrupt ← 第三层：事件源（硬件如何触发转换）
12-kernel-call        ← 第三层：事件源（系统调用如何触发转换）
13-syscall-vmctl      ← 第四层：服务（VM 控制协议）
14-syscall-copy       ← 第四层：服务（跨进程内存拷贝）
15-syscall-fork-exec  ← 第四层：服务（进程创建/销毁）
16-syscall-signal     ← 第四层：服务（信号发送/投递）
17-syscall-device     ← 第四层：服务（设备 I/O）
18-syscall-clock      ← 第四层：服务（时钟与定时器）
19-syscall-misc       ← 第四层：服务（杂项系统调用）
20-smp                ← 横切：SMP 并发
```

**每篇文档只依赖编号更小的文档。** 读者可以顺序阅读而无需前后跳跃。

---

## 1. Minix3 C 源码运行时全图

### 1.1 内核源文件统计

| 文件 | 行数 | 核心职责 |
|------|------|---------|
| `proc.c` | 1980 | 调度、IPC、进程管理——运行时核心 |
| `system.c` | ~900 | 系统调用分派、特权管理、信号基础设施 |
| `system/do_*.c` | 40 个文件 | 每个系统调用的具体实现 |
| `clock.c` | ~310 | 时钟中断处理、定时器管理 |
| `interrupt.c` | ~170 | IRQ hook 注册/分发 |
| `smp.c` | ~205 | SMP 启动、BKL、CPU 间通信 |
| `debug.c` | ~563 | 内核调试输出 |
| `watchdog.c` | ~112 | 看门狗定时器 |
| `profile.c` | — | 性能统计 |
| `cpulocals.c` | — | CPU 本地变量 |
| `utility.c` | 93 | 通用工具函数 |
| `usermapped_data.c` | 15 | 用户态可读的内核数据页 |

### 1.2 系统调用完整清单（system_init 注册的 46 个调用）

```
进程管理（10个）：
  SYS_FORK, SYS_EXEC, SYS_CLEAR, SYS_EXIT, SYS_PRIVCTL,
  SYS_TRACE, SYS_SETGRANT, SYS_RUNCTL, SYS_UPDATE, SYS_STATECTL

信号（5个）：
  SYS_KILL, SYS_GETKSIG, SYS_ENDKSIG, SYS_SIGSEND, SYS_SIGRETURN

设备 I/O（4个，x86 相关）：
  SYS_IRQCTL, SYS_DEVIO, SYS_VDEVIO, SYS_SDEVIO

内存/拷贝（11个）：
  SYS_MEMSET, SYS_VMCTL,
  SYS_UMAP, SYS_UMAP_REMOTE, SYS_VUMAP,
  SYS_VIRCOPY, SYS_PHYSCOPY,
  SYS_SAFECOPYFROM, SYS_SAFECOPYTO, SYS_VSAFECOPY,
  SYS_SAFEMEMSET

时钟（5个）：
  SYS_TIMES, SYS_SETALARM, SYS_STIME, SYS_SETTIME, SYS_VTIMER

系统控制（4个）：
  SYS_ABORT, SYS_GETINFO, SYS_DIAGCTL, SYS_SPROF

调度（2个）：
  SYS_SCHEDULE, SYS_SCHEDCTL

机器状态（2个）：
  SYS_SETMCONTEXT, SYS_GETMCONTEXT

架构特定（3个）：
  SYS_READBIOS, SYS_IOPENABLE, SYS_PADCONF(ARM)
```

### 1.3 proc.c 运行时函数一览

| 函数 | 行号 | 职责 |
|------|------|------|
| `idle()` | L176 | CPU 空闲时 halt 等待中断 |
| `vm_suspend()` | L234 | 挂起进程等待 VM 处理缺页 |
| `delivermsg()` | L263 | 将消息拷贝到用户空间 |
| `switch_to_user()` | L299 | **调度循环入口——永不返回** |
| `do_sync_ipc()` | L479 | 同步 IPC 分派（SEND/RECEIVE/NOTIFY/SENDREC/SENDNB） |
| `do_ipc()` | L599 | IPC 系统调用入口（含 SENDA/MINIX_KERNINFO） |
| `deadlock()` | L703 | 死锁环检测 |
| `has_pending*()` | L773 | 检查挂起通知/异步消息 |
| `mini_send()` | L870 | 阻塞发送实现 |
| `mini_receive()` | L967 | 阻塞接收实现 |
| `mini_notify()` | L1122 | 非阻塞通知实现 |
| `try_deliver_senda()` | L1200 | 异步批量发送 |
| `mini_senda()` | L1331 | SENDA 入口 |
| `try_async()` | L1348 | 尝试投递挂起异步消息 |
| `try_one()` | L1390 | 尝试投递单个异步消息 |
| `cancel_async()` | L1510 | 取消异步消息 |
| `enqueue()` | L1595 | 进程入就绪队列 |
| `enqueue_head()` | L1670 | 进程入就绪队列头部 |
| `dequeue()` | L1716 | 进程出就绪队列 |
| `pick_proc()` | L1785 | 选择下一个运行进程 |
| `endpoint_lookup()` | L1818 | endpoint → proc 指针 |
| `isokendpt_f()` | L1831 | endpoint 有效性验证 |
| `notify_scheduler()` | L1860 | 量子耗尽通知调度器 |
| `proc_no_time()` | L1893 | 量子耗尽处理 |
| `ser_dump_proc()` | L1970 | 调试用进程表 dump |

---

## 2. 各文档详细规划

### 08-process-state: 进程状态机

> **核心问题**: 内核如何描述一个进程？进程有哪些状态？
> **前置**: 07（boot 完成，进程表已填充）
> **后置**: 所有后续文档的基础——调度、IPC、信号全部依赖进程状态
> **C 源码**: `proc.h:1-290`, `priv.h`, `proc.c:119-160`

#### 2.0.1 覆盖范围

**struct proc 按语义分组**（非按声明顺序）：

| 分组 | 字段 | 用途 |
|------|------|------|
| 标识 | `p_nr`, `p_endpoint`, `p_name`, `p_magic` | 内核内唯一标识 |
| 寄存器 | `p_reg`（stackframe_s：pc/sp/retreg/...） | 上下文切换时保存/恢复 |
| 段/页表 | `p_seg`（segframe_s：cr3, fpu_state, ...） | 地址空间根 |
| 调度状态 | `p_rts_flags`（16 位标志） | 进程能否被调度 |
| 调度参数 | `p_priority`, `p_quantum_size_ms`, `p_cpu_time_left`, `p_cpu` | 调度策略输入 |
| 运行时杂项 | `p_misc_flags`（20+ 位标志） | 调度中需处理的临时条件 |
| IPC 队列 | `p_nextready`, `p_caller_q`, `p_q_link`, `p_getfrom_e`, `p_sendto_e` | 就绪队列/发送者队列 |
| 消息 | `p_sendmsg`, `p_delivermsg`, `p_delivermsg_vir` | IPC 消息载体 |
| VM 挂起 | `p_vmrequest`（type, target, params, vmresult, nextrequestor） | 缺页挂起协议 |
| 记账 | `p_accounting`, `p_user_time`, `p_sys_time`, `p_cycles` | CPU 时间统计 |
| 特权 | `p_priv` → `struct priv` | 权限和 IPC 掩码 |
| 虚拟定时器 | `p_virt_left`, `p_prof_left` | 用户态定时器 |

**RTS 标志位完整语义**（`proc.h:142-166`）：

```
RTS_SLOT_FREE    0x00001 — 槽位空闲
RTS_PROC_STOP    0x00002 — 被停止（boot 初始状态）
RTS_SENDING      0x00004 — 阻塞于 SEND
RTS_RECEIVING    0x00008 — 阻塞于 RECEIVE
RTS_SIGNALED     0x00010 — 有内核信号到达
RTS_SIG_PENDING  0x00020 — 信号处理中，暂不可调度
RTS_P_STOP       0x00040 — 被 ptrace 停止
RTS_NO_PRIV      0x00080 — fork 后等待特权设置
RTS_NO_ENDPOINT  0x00100 — 端点失效，不可 IPC
RTS_VMINHIBIT    0x00200 — VM 尚未设置页表
RTS_PAGEFAULT    0x00400 — 有未处理缺页
RTS_VMREQUEST    0x00800 — VM 内存请求发起者
RTS_VMREQTARGET  0x01000 — VM 内存请求目标
RTS_PREEMPTED    0x04000 — 被高优先级抢占
RTS_NO_QUANTUM   0x08000 — 量子耗尽
RTS_BOOTINHIBIT  0x10000 — boot 阶段等待 VM
```

**核心不变量**：进程可运行当且仅当 `p_rts_flags == 0`。任何 `RTS_*` 置位意味着进程不在就绪队列中。

**MiscFlags 语义**（`proc.h:234-262`）：
- 这些标志不影响调度（进程仍在就绪队列），但在 `switch_to_user()` 的 `check_misc_flags` 循环中处理
- `MF_DELIVERMSG`：有待投递消息，运行前先 `delivermsg()`
- `MF_KCALL_RESUME`：内核调用因 VMSUSPEND 中断，运行前先 `kernel_call_resume()`
- `MF_SC_DEFER`/`MF_SC_TRACE`/`MF_SC_ACTIVE`：ptrace 相关的系统调用追踪
- `MF_REPLY_PEND`：SENDREC 的回复待收，阻止通知打断
- `MF_MSGFAILED`：消息投递首次失败，再失败则 SIGSEGV
- `MF_FLUSH_TLB`：(SMP) TLB 需刷新
- `MF_SENDING_FROM_KERNEL`：消息来自内核（设置 IPC_FLG_MSG_FROM_KERNEL）

**struct priv**（`priv.h`）：
- `s_flags`: PREEMPTIBLE / BILLABLE / SYS_PROC / DYN_PRIV_ID / CHECK_IO_PORT / CHECK_IRQ / CHECK_MEM / ROOT_SYS_PROC / VM_SYS_PROC / LU_SYS_PROC / RST_SYS_PROC
- `s_trap_mask`: 允许的 IPC 陷阱号位图
- `s_ipc_to`: 允许发送的目标位图
- `s_k_call_mask`: 允许的内核调用位图
- `s_notify_pending` / `s_asyn_pending`: 挂起通知/异步消息位图
- `s_sig_mgr` / `s_bak_sig_mgr`: 信号管理器 endpoint
- `s_alarm_timer`: 同步闹钟
- `s_grant_table` / `s_grant_entries`: grant 表
- `s_stack_guard`: 内核栈保护

#### 2.0.2 C 源码坐标

| 符号 | 文件:行 |
|------|--------|
| `struct proc` | `proc.h:1-120` |
| `RTS_*` 宏 | `proc.h:142-166` |
| `MF_*` 宏 | `proc.h:234-262` |
| `RTS_SET/UNSET/ISSET` | `proc.h:202-227` |
| `proc_init()` | `proc.c:119-160` |
| `struct priv` | `priv.h` |
| `idle_priv` | `proc.c:64` |

#### 2.0.3 与 tmp-06-proc-struct.md 的关系

tmp-06 已覆盖 struct proc 的所有字段但未充分展开 RTS 标志位的运行时语义。08 应聚焦于「这些标志如何决定进程的行为」而非逐字段罗列。

---

### 09-scheduling: 调度循环

> **核心问题**: switch_to_user() 如何选择下一个进程？进程如何进出运行队列？
> **前置**: 08（需要理解 RTS 标志位和 struct proc 的调度字段）
> **C 源码**: `proc.c:299-474` (switch_to_user), `proc.c:1595-1870` (enqueue/dequeue/pick_proc)

#### 2.0.4 覆盖范围

**switch_to_user() 的完整控制流**（`proc.c:299-474`）：

```
switch_to_user():
  p = proc_ptr
  if proc_is_runnable(p): goto check_misc_flags

not_runnable_pick_new:
  if was_preempted: clear PREEMPTED, re-enqueue
  while (p = pick_proc()) == NULL: idle()
  proc_ptr = p
  switch_address_space(p)

check_misc_flags:
  while p.misc_flags & (KCALL_RESUME|DELIVERMSG|SC_DEFER|SC_TRACE|SC_ACTIVE):
    process misc flags → 可能变为 non-runnable → goto not_runnable_pick_new
  if no quantum: notify_scheduler
  if !runnable: goto not_runnable_pick_new
  arch_finish_switch_to_user()
  restart_local_timer()
  restore_user_context(p)  ← 永不返回
```

**关键细节**：
1. `switch_to_user()` 不是「从 idle 中跳出」——它是从内核态**返回用户态**的最后一站。调用者总是内核异常/中断/syscall 的出口路径。
2. `check_misc_flags` 循环可能在处理 `MF_DELIVERMSG` 时触发 `vm_suspend()`，导致进程变为 non-runnable，然后 goto 回到 pick_proc。
3. `restore_user_context()` 执行 iret/sysret——之后 CPU 在用户态运行，直到下一次陷入内核。

**优先级队列**（NR_SCHED_QUEUES = 16）：
- TASK_Q (0): 内核任务（CLOCK、SYSTEM）— 不可抢占
- SERVER_Q (1~2): 系统服务（VM、PM、VFS、RS）
- USER_Q (3~14): 用户进程
- IDLE_Q (15): 空闲进程

**pick_proc()**（`proc.c:1785`）：从最高优先级非空队列取队首。O(1)。

**enqueue()**（`proc.c:1595`）：尾部插入。如果优先级高于当前进程且当前可抢占 → `RTS_PREEMPTED`。SMP 下跨 CPU enqueue → `smp_schedule()` IPI。

**enqueue_head()**（`proc.c:1670`）：头部插入（抢占后恢复用）。

**dequeue()**（`proc.c:1716`）：从队列移除，更新记账。

**量子管理**（`proc.c:1860-1912`）：
- `p_cpu_time_left` 以 CPU 周期为单位
- 量子耗尽 → `RTS_NO_QUANTUM` → `notify_scheduler()` → 进程不可运行

**idle()**（`proc.c:176`）：halt CPU，等待时钟中断后重新 pick_proc。

**SMP 调度**：
- 每个 CPU 独立的 `run_q_head[]` / `run_q_tail[]`
- 进程 CPU 亲和性：`p_cpu`, `p_cpu_mask`
- BKL (`big_kernel_lock`)：自旋锁，保证同一时刻只有一个 CPU 在内核态
- BKL 在 `switch_to_user` 出口释放（`restore_user_context` 前），在中断/syscall 入口获取

#### 2.0.5 C 源码坐标

| 函数 | 文件:行 | 行数 |
|------|--------|------|
| `switch_to_user()` | `proc.c:299-474` | ~175 |
| `enqueue()` | `proc.c:1595-1668` | ~73 |
| `enqueue_head()` | `proc.c:1670-1714` | ~44 |
| `dequeue()` | `proc.c:1716-1783` | ~67 |
| `pick_proc()` | `proc.c:1785-1810` | ~25 |
| `idle()` | `proc.c:176-230` | ~54 |
| `notify_scheduler()` | `proc.c:1860-1891` | ~31 |
| `proc_no_time()` | `proc.c:1893-1910` | ~17 |

---

### 10-sync-ipc: 同步 IPC

> **核心问题**: 进程间如何发送/接收消息？阻塞和唤醒如何工作？
> **前置**: 08（进程状态）, 09（调度——IPC 阻塞导致 dequeue）
> **C 源码**: `proc.c:479-1590`（do_ipc → mini_send/receive/notify → SENDA → cancel_async）

#### 2.0.6 覆盖范围

**IPC 入口**（`proc.c:599`）：`do_ipc(r1, r2, r3)`
- r1 = call_nr: SEND / RECEIVE / SENDREC / NOTIFY / SENDNB / SENDA / MINIX_KERNINFO
- r2 = src_dst endpoint
- r3 = message pointer

**权限检查**（`do_sync_ipc` 前半，`proc.c:479-560`）：
1. call_nr 范围检查（< 32, ≤ IPCNO_HIGHEST）
2. endpoint 有效性（`isokendpt`）
3. `may_send_to` 检查（`s_ipc_to` 位图）
4. `trap_mask` 检查（`s_trap_mask` 位图）
5. 内核进程只接受 SENDREC

**mini_send()**（`proc.c:870`）——SEND 的核心：
```
if WILLRECEIVE(dst):
    copy_msg_from_user → dst.p_delivermsg
    MF_DELIVERMSG = 1
    RTS_UNSET(dst, RTS_RECEIVING)  ← 唤醒目标
else:
    if NON_BLOCKING: return ENOTREADY
    deadlock 检测
    copy_msg_from_user → caller.p_sendmsg
    RTS_SET(caller, RTS_SENDING)  ← 阻塞自己
    加入 dst.p_caller_q 队列
```

**mini_receive()**（`proc.c:967`）——RECEIVE 的核心：
```
1. 检查挂起通知 (has_pending_notify) → 找到则投递
2. 检查挂起异步消息 (has_pending_asend) → 找到则投递
3. 遍历 p_caller_q 队列找匹配发送者 → 找到则投递
4. 无消息可用:
   if NON_BLOCKING: return ENOTREADY
   deadlock 检测
   RTS_SET(caller, RTS_RECEIVING)  ← 阻塞自己
```

**mini_notify()**（`proc.c:1122`）——NOTIFY 的核心：
```
if WILLRECEIVE && !MF_REPLY_PEND:
    组装通知消息 → MF_DELIVERMSG → RTS_UNSET(dst, RTS_RECEIVING)
else:
    设置 dst.s_notify_pending 位图（延迟投递）
```

**SENDREC**: SEND + RECEIVE 原子组合。设置 `MF_REPLY_PEND` 防止通知打断。

**deadlock 检测**（`proc.c:703`）：沿 `P_BLOCKEDON` 链检查环。组大小=2 且 SEND↔RECEIVE 对不视为死锁。

**SENDA 异步批量发送**（`proc.c:1200-1588`）：
- `try_deliver_senda()`: 遍历 asynmsg 表，逐个投递
- `try_async()`: 从 `s_asyn_pending` 位图中找到匹配的异步消息
- `cancel_async()`: 取消挂起的异步消息

#### 2.0.7 C 源码坐标

| 函数 | 文件:行 | 行数 |
|------|--------|------|
| `do_sync_ipc()` | `proc.c:479-598` | ~119 |
| `do_ipc()` | `proc.c:599-698` | ~99 |
| `deadlock()` | `proc.c:703-770` | ~67 |
| `has_pending()` | `proc.c:773-841` | ~68 |
| `has_pending_notify()` | `proc.c:843-850` | ~7 |
| `has_pending_asend()` | `proc.c:852-859` | ~7 |
| `unset_notify_pending()` | `proc.c:861-868` | ~7 |
| `mini_send()` | `proc.c:870-965` | ~95 |
| `mini_receive()` | `proc.c:967-1120` | ~153 |
| `mini_notify()` | `proc.c:1122-1196` | ~74 |
| `try_deliver_senda()` | `proc.c:1200-1330` | ~130 |
| `mini_senda()` | `proc.c:1331-1346` | ~15 |
| `try_async()` | `proc.c:1348-1388` | ~40 |
| `try_one()` | `proc.c:1390-1507` | ~117 |
| `cancel_async()` | `proc.c:1510-1588` | ~78 |

---

### 11-exception-interrupt: 异常与中断

> **核心问题**: 硬件事件如何打断进程？内核如何响应？
> **前置**: 08（进程状态）, 10（IPC——中断通过 mini_notify 唤醒进程）
> **C 源码**: `interrupt.c:1-170`, `clock.c:70-199`

#### 2.0.8 覆盖范围

本文档覆盖**硬件事件的入口和路由**，不展开异常帧的细节（那是 arch 文档的职责）。

**中断子系统**（`interrupt.c:1-170`）：

```
中断到达
  → IDT entry → 保存寄存器
  → BKL_LOCK()
  → irq_handle(irq)
    → 遍历 irq_handlers[irq] 链表
    → 调用 hook->handler 回调
    → 如果策略不是 reenable → 保持屏蔽
    → 如果所有 active ID 位已清除 → hw_intr_unmask(irq)
  → hw_intr_ack(irq)
  → switch_to_user()
```

**IRQ hook 管理**：
- `put_irq_handler(hook, irq, handler)`: 注册，自动分配 ID 位
- `rm_irq_handler(hook)`: 注销，无 hook 时自动屏蔽 IRQ
- `enable_irq(hook)` / `disable_irq(hook)`: 运行时使能/禁用
- `irq_actids[irq]`: 活跃 ID 位图——所有位清零才 unmask IRQ

**时钟中断**（`clock.c:70-199`）——`timer_int_handler()`：
```
1. BSP 更新 realtime/uptime（AP 只做本地记账）
2. 更新当前进程 user/sys time
3. 递减虚拟/性能定时器
4. 检查 alarm timer 超时 → tmr_exptimers
5. 更新 load average
6. arch_timer_int_handler()
```

时钟中断是调度的驱动力——量子消耗在时钟中断中更新，`RTS_NO_QUANTUM` 在 `switch_to_user()` 中检查。

**中断到调度的完整路径**：
```
硬件中断 → IDT → BKL_LOCK → irq_handle → mini_notify(proc)
  → 唤醒等待进程 (RTS_UNSET(RECEIVING))
  → switch_to_user → pick_proc → restore_user_context
```

#### 2.0.9 与 tmp-05-exception-interrupt.md 的关系

tmp-05 同时覆盖了异常和中断。11 应聚焦于**中断路由和时钟中断**。异常帧解析（exception_handler, pagefault）已部分在 kboot 04 中覆盖；本文档只需覆盖运行时中断路径。

缺页转发到 VM 的路径（`exception_handler → pagefault → mini_send(VM) → RTS_PAGEFAULT`）应在本文档中覆盖，因为这是**运行时**的缺页处理（不同于 boot 阶段的内存初始化）。

#### 2.0.10 C 源码坐标

| 函数 | 文件:行 |
|------|--------|
| `put_irq_handler()` | `interrupt.c:29-69` |
| `rm_irq_handler()` | `interrupt.c:75-107` |
| `irq_handle()` | `interrupt.c:116-160` |
| `enable_irq()` / `disable_irq()` | `interrupt.c:162-170` |
| `timer_int_handler()` | `clock.c:70-199` |
| `init_clock()` | `clock.c:48-74` |

---

### 12-kernel-call: 系统调用分派

> **核心问题**: 用户态进程如何请求内核服务？VMSUSPEND 挂起-恢复协议如何工作？
> **前置**: 10（IPC——kernel_call 通过 IPC 消息传递请求）, 11（中断——syscall 入口在中断/异常路径中）
> **C 源码**: `system.c:59-167` (kernel_call, kernel_call_dispatch, kernel_call_finish), `system.c:612-638` (kernel_call_resume)

#### 2.0.11 覆盖范围

**系统调用入口**（`system.c:136-167`）——`kernel_call(m_user, caller)`：
```
1. copy_msg_from_user(m_user, &msg) — 复制请求消息到内核空间
   失败 → cause_sig(SIGSEGV)
2. msg.m_source = caller.p_endpoint
3. result = kernel_call_dispatch(caller, &msg)
4. kernel_call_finish(caller, &msg, result)
```

**分派**（`system.c:95-128`）——`kernel_call_dispatch(caller, msg)`：
```
call_nr = msg.m_type - KERNEL_CALL
if call_nr 越界 → EBADREQUEST
if !s_k_call_mask[call_nr] → ECALLDENIED
result = call_vec[call_nr](caller, msg)
```

**VMSUSPEND 协议**（`system.c:59-94`）——`kernel_call_finish` 的关键分支：
```
if result == VMSUSPEND:
    // 内核调用需要 VM 介入（例如 safecopy 遇到缺页）
    保存请求消息到 p_vmrequest.saved.reqmsg
    MF_KCALL_RESUME = 1
    // 进程保持 RTS_VMREQUEST，等待 VM 回复
else:
    复制结果消息回用户空间
    失败 → cause_sig(SIGSEGV)
```

**恢复**（`system.c:612-638`）——`kernel_call_resume(caller)`：
```
1. 重新执行 kernel_call_dispatch（使用保存的消息）
2. 清除 MF_KCALL_RESUME
3. kernel_call_finish（可能再次 VMSUSPEND）
```

**system_init()**（`system.c:168-278`）：已在 kboot 07 中覆盖，本文档只需引用。

**signal 基础设施**（`system.c:364-465`）：
- `send_sig(ep, sig_nr)`: 向信号管理器发送 SIGKMEM
- `cause_sig(proc_nr, sig_nr)`: 设置 `p_pending` 位图 + RTS_SIGNALED + send_sig
- `sig_delay_done(rp)`: 进程确认不再发送直接消息后，解除 SIG_DELAY

#### 2.0.12 C 源码坐标

| 函数 | 文件:行 |
|------|--------|
| `kernel_call_finish()` | `system.c:59-94` |
| `kernel_call_dispatch()` | `system.c:95-128` |
| `kernel_call()` | `system.c:136-167` |
| `system_init()` | `system.c:168-278` |
| `send_sig()` | `system.c:364-386` |
| `cause_sig()` | `system.c:389-451` |
| `sig_delay_done()` | `system.c:454-465` |
| `kernel_call_resume()` | `system.c:612-638` |
| `clear_endpoint()` | `system.c:540-574` |
| `clear_ipc()` | `system.c:509-537` |
| `clear_ipc_refs()` | `system.c:577-610` |

---

### 13-syscall-vmctl: VM 控制协议

> **核心问题**: 内核与 VM 之间通过什么接口协作管理页表？
> **前置**: 12（系统调用分派）
> **C 源码**: `system/do_vmctl.c:1-173`

#### 2.0.13 覆盖范围

**VMCTL 子命令完整语义**（`do_vmctl.c`）：

| 子命令 | 行为 | 调用者 |
|--------|------|--------|
| `VMCTL_SETADDRSPACE` | 设置进程 CR3 + 切换地址空间 | VM |
| `VMCTL_GET_PDBR` | 获取进程 CR3 | VM |
| `VMCTL_CLEAR_PAGEFAULT` | 清除 RTS_PAGEFAULT | VM |
| `VMCTL_MEMREQ_GET` | 获取挂起的内存请求（遍历 vmrequest 链表） | VM |
| `VMCTL_MEMREQ_REPLY` | 回复内存请求结果 → 恢复挂起进程 | VM |
| `VMCTL_VMINHIBIT_SET` | 设置 RTS_VMINHIBIT（禁止调度） | VM |
| `VMCTL_VMINHIBIT_CLEAR` | 清除 RTS_VMINHIBIT（允许调度） | VM |
| `VMCTL_KERN_PHYSMAP` | 内核声明需要映射的物理区域 | Kernel |
| `VMCTL_KERN_MAP_REPLY` | 内核获得虚拟地址 | Kernel |
| `VMCTL_CLEARMAPCACHE` | 清除内核缓存的临时映射 | VM |
| `VMCTL_BOOTINHIBIT_CLEAR` | 清除 RTS_BOOTINHIBIT | VM |

**VMCTL_MEMREQ_GET 的关键细节**（`do_vmctl.c:43-78`）：
- 遍历 `vmrequest` 全局链表
- 受 IPC filter 约束（`allow_ipc_filtered_memreq`）
- 从链表中摘除匹配的请求
- 返回请求参数（target, addr, length, writeflag, requestor）

**VMCTL_MEMREQ_REPLY 的恢复路径**（`do_vmctl.c:81-101`）：
- `VMSTYPE_KERNELCALL` → `MF_KCALL_RESUME`（switch_to_user 中重试 kernel_call）
- `VMSTYPE_DELIVERMSG` → 保持 `MF_DELIVERMSG`（switch_to_user 中重试 delivermsg）
- `VMSTYPE_MAP` → 仅清除 `RTS_VMREQUEST`

#### 2.0.14 C 源码坐标

| 函数 | 文件:行 |
|------|--------|
| `do_vmctl()` | `system/do_vmctl.c:21-173` |

---

### 14-syscall-copy: 跨进程拷贝

> **核心问题**: 内核如何安全地在进程间拷贝数据？grant 机制如何工作？
> **前置**: 12（系统调用分派）, 13（VMCTL——safecopy 缺页触发 VMSUSPEND）
> **C 源码**: `system/do_safecopy.c:1-448`, `system/do_umap.c`, `system/do_umap_remote.c`, `system/do_vumap.c`, `system/do_copy.c`

#### 2.0.15 覆盖范围

**拷贝原语层次**：
```
SYS_PHYSCOPY     — 物理地址到物理地址（内核专用）
SYS_VIRCOPY      — 虚拟地址到虚拟地址（跨进程）
SYS_SAFECOPYFROM — 通过 grant 从目标进程拷贝（安全）
SYS_SAFECOPYTO   — 通过 grant 向目标进程拷贝（安全）
SYS_VSAFECOPY    — 向量化 safecopy（批量）
SYS_UMAP         — 虚拟地址到物理地址映射
SYS_UMAP_REMOTE  — 非调用者的 umap
SYS_VUMAP        — 向量化 umap
SYS_SAFEMEMSET   — 安全内存清零
SYS_MEMSET       — 内存清零
```

**verify_grant()**（`do_safecopy.c:46-148`）——safecopy 的安全核心：
1. 验证 granter endpoint 有效
2. 验证 grant ID 范围
3. 支持间接 grant（最多 5 层 `MAX_INDIRECT_DEPTH`）
4. 权限检查（CPF_READ / CPF_WRITE）
5. 偏移量计算
6. CPF_TRY 标志：软故障（不杀死进程，只返回错误）

**Direct Map 对拷贝的影响**：
- 64 位 Direct Map 下，`kernel_phys_to_virt()` 替代 Minix3 的 `createpde`/`lin_lin_copy`
- 跨进程拷贝退化为：VA→PA 翻译 → `kernel_phys_to_virt(pa)` → memcpy
- grant 验证逻辑不变（权限检查与地址映射无关）
- 缺页仍触发 VMSUSPEND（等待 VM 建立映射后重试）

#### 2.0.16 C 源码坐标

| 函数 | 文件:行 |
|------|--------|
| `verify_grant()` | `system/do_safecopy.c:46-148` |
| `safecopy()` | `system/do_safecopy.c:148-393` |
| `do_safecopy_from()` | `system/do_safecopy.c:395-420` |
| `do_safecopy_to()` | `system/do_safecopy.c:422-448` |
| `do_umap()` | `system/do_umap.c` |
| `do_umap_remote()` | `system/do_umap_remote.c:1-122` |
| `do_vumap()` | `system/do_vumap.c:1-131` |
| `do_copy()` | `system/do_copy.c:1-91` |
| `do_vircopy()` | `system/do_copy.c` |
| `do_safememset()` | `system/do_safememset.c` |
| `do_memset()` | `system/do_memset.c` |

---

### 15-syscall-fork-exec: 进程创建与销毁

> **核心问题**: 内核如何创建/销毁进程？
> **前置**: 12（系统调用分派）, 08（进程状态）
> **C 源码**: `system/do_fork.c:1-136`, `system/do_exec.c`, `system/do_clear.c`, `system/do_exit.c`, `system/do_privctl.c:1-371`, `system/do_runctl.c`, `system/do_update.c:1-340`, `system/do_statectl.c`

#### 2.0.17 覆盖范围

**SYS_FORK**（`do_fork.c:1-136`）：
1. 验证父进程正在 RECEIVE（同步 fork 前提）
2. 复制父进程 proc 结构到子进程槽位
3. 新 endpoint（generation + 1），新进程名
4. 子进程 `retreg = 0`
5. FPU 状态复制（`save_fpu` + `fpu_save_area_p` 拷贝）
6. `RTS_NO_PRIV`（系统进程 fork 后需 PRIVCTL 设置特权）
7. 返回子进程 endpoint

**SYS_EXEC**（`do_exec.c`）：
- 设置新 PC/SP/ps_strings
- 更新进程名
- 清除 `MF_DELIVERMSG`
- `arch_proc_init` 重置寄存器

**SYS_CLEAR**（`do_clear.c`）：
- `clear_endpoint`: RTS_NO_ENDPOINT + clear_ipc + clear_ipc_refs
- 清除所有 IPC 状态（发送队列、接收状态、通知位图、异步消息）

**SYS_EXIT**（`do_exit.c`）：
- 标记系统进程不再运行
- 通知 PM/RS

**SYS_PRIVCTL**（`do_privctl.c:1-371`）：
- SYS_PRIV_ALLOW: 解除 RTS_NO_PRIV + 分配 priv 槽
- SYS_PRIV_SET: 设置完整特权结构
- SYS_PRIV_ADD_IRQ / ADD_IO / ADD_MEM: 增量添加权限

**SYS_RUNCTL**（`do_runctl.c`）：
- 设置/清除 RTS_PROC_STOP

**SYS_UPDATE**（`do_update.c:1-340`）：
- 替换进程映像但保持 endpoint（live update）

#### 2.0.18 C 源码坐标

| 函数 | 文件:行 |
|------|--------|
| `do_fork()` | `system/do_fork.c:25-136` |
| `do_exec()` | `system/do_exec.c` |
| `do_clear()` | `system/do_clear.c` |
| `do_exit()` | `system/do_exit.c` |
| `do_privctl()` | `system/do_privctl.c:21-371` |
| `do_runctl()` | `system/do_runctl.c` |
| `do_update()` | `system/do_update.c:21-340` |
| `do_statectl()` | `system/do_statectl.c` |
| `get_priv()` | `system.c:274-305` |
| `set_sendto_bit()` | `system.c:307-333` |
| `unset_sendto_bit()` | `system.c:335-347` |
| `fill_sendto_mask()` | `system.c:349-362` |

---

### 16-syscall-signal: 信号系统

> **核心问题**: 内核如何向进程发送信号？信号如何投递？
> **前置**: 12（系统调用分派）, 10（IPC——信号通过 mini_notify/send_sig 传递）
> **C 源码**: `system/do_kill.c`, `system/do_getksig.c`, `system/do_endksig.c`, `system/do_sigsend.c:1-166`, `system/do_sigreturn.c:1-98`

#### 2.0.19 覆盖范围

**信号流程（三阶段）**：
```
阶段 1: 产生
  cause_sig(proc_nr, sig_nr)
    → 设置 p_pending 位图
    → RTS_SIGNALED
    → send_sig(sig_mgr, SIGKMEM)  — 通知信号管理器（PM）

阶段 2: 查询
  PM 调用 SYS_GETKSIG
    → 遍历进程表找 RTS_SIGNALED 的进程
    → 返回 endpoint + pending 位图
    → RTS_SIGNALED → RTS_SIG_PENDING

阶段 3: 投递
  PM 调用 SYS_SIGSEND
    → 设置信号帧 (sigframe) 在用户栈
    → 修改进程 PC 跳转到信号处理函数
    → 处理完成后 SYS_SIGRETURN 恢复上下文
```

**关键区分**：
- 内核信号：通知机制，通过 `send_sig(SIGKMEM)` 传递给信号管理器
- POSIX 信号：用户态信号处理，通过信号帧修改进程上下文
- `cause_sig` 是统一的信号产生入口——无论是缺页导致的 SIGSEGV 还是 do_kill 产生的任意信号

**SYS_KILL**（`do_kill.c:1-41`）：`cause_sig(proc_nr, sig_nr)` 的 syscall 封装。

**SYS_GETKSIG**（`do_getksig.c`）：信号管理器查询待处理信号。

**SYS_SIGSEND**（`do_sigsend.c:1-166`）：设置信号帧、修改 PC。

**SYS_SIGRETURN**（`do_sigreturn.c:1-98`）：恢复信号处理前的上下文。

#### 2.0.20 C 源码坐标

| 函数 | 文件:行 |
|------|--------|
| `do_kill()` | `system/do_kill.c:17-41` |
| `do_getksig()` | `system/do_getksig.c:12-43` |
| `do_endksig()` | `system/do_endksig.c` |
| `do_sigsend()` | `system/do_sigsend.c:1-166` |
| `do_sigreturn()` | `system/do_sigreturn.c:1-98` |

---

### 17-syscall-device: 设备 I/O

> **核心问题**: 驱动程序如何通过内核访问硬件？
> **前置**: 12（系统调用分派）, 11（中断）
> **C 源码**: `system/do_irqctl.c:1-174`, `system/do_devio.c:1-107`, `system/do_vdevio.c:1-165`

#### 2.0.21 覆盖范围

**SYS_IRQCTL**（`do_irqctl.c:1-174`）：
- IRQ_SETPOLICY: 注册 IRQ hook + 设置策略（reenable/notify）
- IRQ_ENABLE/DISABLE: 运行时使能/禁用
- IRQ_RCVHOOKID: 获取 hook ID
- generic_handler: 通用中断处理 → mini_notify

**SYS_DEVIO**（`do_devio.c:1-107`）：单次 inb/inw/inl/outb/outw/outl（x86 only）

**SYS_VDEVIO**（`do_vdevio.c:1-165`）：批量 I/O 操作（x86 only）

**SYS_SDEVIO**：安全向量化 I/O，通过 grant 验证（x86 only）

**架构依赖标注**：
- x86 独占：SYS_DEVIO, SYS_VDEVIO, SYS_SDEVIO, SYS_IOPENABLE, SYS_READBIOS
- ARM 独占：SYS_PADCONF
- 跨架构：SYS_IRQCTL

#### 2.0.22 C 源码坐标

| 函数 | 文件:行 |
|------|--------|
| `do_irqctl()` | `system/do_irqctl.c:21-174` |
| `generic_handler()` | `system/do_irqctl.c` |
| `do_devio()` | `system/do_devio.c:1-107` |
| `do_vdevio()` | `system/do_vdevio.c:1-165` |
| `do_sdevio()` | arch 特定文件 |
| `priv_add_irq()` | `system.c:918-943` |
| `priv_add_io()` | `system.c:945-971` |

---

### 18-syscall-clock: 时钟调用

> **核心问题**: 进程如何获取时间/设置定时器？
> **前置**: 12（系统调用分派）, 11（时钟中断——定时器在中断中检查）
> **C 源码**: `system/do_times.c`, `system/do_setalarm.c`, `system/do_stime.c`, `system/do_settime.c`, `system/do_vtimer.c:1-103`

#### 2.0.23 覆盖范围

| 调用 | 功能 |
|------|------|
| SYS_TIMES | 获取进程 user/sys time + 系统 uptime |
| SYS_SETALARM | 设置同步闹钟（s_alarm_timer，到期后 send_sig(SIGALRM)） |
| SYS_STIME | 设置 boottime |
| SYS_SETTIME | 设置 realtime |
| SYS_VTIMER | 设置/查询虚拟定时器（仅在用户态运行时递减） |

**虚拟定时器 vs 闹钟**：
- 虚拟定时器：在每次时钟中断中递减（`timer_int_handler`）
- 闹钟：挂在内核 timer 队列（`clock_timers`），到期时由 `tmrs_exptimers` 触发

#### 2.0.24 C 源码坐标

| 函数 | 文件:行 |
|------|--------|
| `do_times()` | `system/do_times.c` |
| `do_setalarm()` | `system/do_setalarm.c` |
| `do_stime()` | `system/do_stime.c` |
| `do_settime()` | `system/do_settime.c` |
| `do_vtimer()` | `system/do_vtimer.c:1-103` |

---

### 19-syscall-misc: 杂项系统调用

> **核心问题**: 剩下的系统调用做什么？
> **前置**: 12（系统调用分派）
> **C 源码**: `system/do_abort.c`, `system/do_getinfo.c`, `system/do_diagctl.c`, `system/do_trace.c`, `system/do_schedule.c`, `system/do_schedctl.c`, `system/do_mcontext.c`, `system/do_setgrant.c`, `system/do_sprofile.c`, `system/do_safememset.c`, `system/do_memset.c`

#### 2.0.25 覆盖范围

| 调用 | 功能 | 文件 |
|------|------|------|
| SYS_ABORT | 终止 MINIX | `do_abort.c` |
| SYS_GETINFO | 获取内核信息（内存布局等） | `do_getinfo.c` |
| SYS_DIAGCTL | 诊断控制 | `do_diagctl.c` |
| SYS_TRACE | ptrace 操作 | `do_trace.c` |
| SYS_SCHEDULE | 重新调度进程（外部调度器使用） | `do_schedule.c` |
| SYS_SCHEDCTL | 设置进程调度参数 | `do_schedctl.c` |
| SYS_SETMCONTEXT | 设置机器上下文（用于 sigreturn 等） | `do_mcontext.c` |
| SYS_GETMCONTEXT | 获取机器上下文 | `do_mcontext.c` |
| SYS_SETGRANT | 设置 grant 表 | `do_setgrant.c` |
| SYS_SPROF | 统计性能分析 | `do_sprofile.c` |
| SYS_SAFEMEMSET | 安全内存清零（通过 grant） | `do_safememset.c` |
| SYS_MEMSET | 内存清零 | `do_memset.c` |
| SYS_READBIOS | 读取 BIOS 数据（x86） | arch 文件 |
| SYS_IOPENABLE | 启用 I/O 权限（x86） | arch 文件 |
| SYS_PADCONF | 引脚配置（ARM） | arch 文件 |

由于这些大多是 thin wrapper，本文档可以合并覆盖而不需要各写独立章节。

---

### 20-smp: SMP 支持

> **核心问题**: 多核如何协调运行？
> **前置**: 09（调度）, 11（中断）
> **C 源码**: `smp.c:1-205`, `proc.c` SMP 条件编译部分

#### 2.0.26 覆盖范围

**BKL（Big Kernel Lock）**：
- 定义：`smp.c` 中的 `big_kernel_lock` 自旋锁
- 获取：所有内核入口（syscall/中断/异常）
- 释放：`switch_to_user → restore_user_context` 之前
- 约束：持锁期间禁止睡眠/调度，但中断处理可以临时释放 BKL（`clock.c` 的 `timer_int_handler`）

**CPU 本地变量**：
- `get_cpulocal_var()` / `get_cpu_var()` 宏
- 每个 CPU 独立的 `proc_ptr`, `bill_ptr`, `run_q_head/tail[]`
- `cpulocals.c` 管理

**跨 CPU 调度**：
- `enqueue()` 中检测目标 CPU 是否 idle → `smp_schedule()` IPI
- `VMCTL_VMINHIBIT_SET` 中检测目标进程是否在别的 CPU → `smp_schedule_vminhibit()`

**SMP 特有的 TLB 管理**：
- `MF_FLUSH_TLB`：进程被别的 CPU 修改页表后设置
- `switch_to_user` 中检查并 `refresh_tlb()`

#### 2.0.27 C 源码坐标

| 函数 | 文件:行 |
|------|--------|
| `smp_init()` | `smp.c` |
| `smp_schedule()` | `smp.c` |
| `smp_schedule_vminhibit()` | `smp.c` |
| BKL 定义 | `smp.h:48` |
| BKL 释放点 | `arch_clock.c:92,107,118` + `smp.c:44,86-94` |

---

## 3. 与 kbood-new.md 的衔接

### 3.1 boot 阶段（01~07）的终点

kboot-new.md 规划的 07-system-init-boot-finish 结束于 `switch_to_user()`。在这一刻：
- 进程表已初始化（proc_init）
- boot 进程已加载（arch_boot_proc）
- VM 已被标记为无 VMINHIBIT（可调度）
- PM/VFS/RS 等被标记为 VMINHIBIT + BOOTINHIBIT
- 系统调用向量已注册（system_init）
- 时钟中断已启动

### 3.2 runtime 阶段（08~20）的起点

`switch_to_user()` 被调用后：
1. 调度器选择 VM（唯一没有 VMINHIBIT 的进程）
2. VM 运行，初始化自身
3. VM 调用 `map_kernel()` 建立 kernel direct map
4. VM → VMCTL_SETADDRSPACE → 内核切换 CR3
5. VM → VMCTL_VMINHIBIT_CLEAR → 其他 boot 进程可调度
6. 此后系统进入稳定运行时状态

### 3.3 不在 runtime 文档中覆盖的内容

以下内容属于 arch 层或已在 kboot 中覆盖：

| 内容 | 原因 |
|------|------|
| HigherHalf 切换 | 已在 kboot 02 中覆盖 |
| GDT/IDT/TSS 初始化 | 已在 kboot 03 中覆盖 |
| 时钟芯片初始化（i8259/APIC） | 已在 kboot 04 中覆盖 |
| VM ELF 加载（libexec） | 已在 kboot 05 中覆盖 |
| freepdes 分配 | 已在 kboot 06 中覆盖 |
| system_init 注册 call_vec | 已在 kboot 07 中覆盖 |
| bsp_finish_booting 全流程 | 已在 kboot 07 中覆盖 |
| VM 的 `init_page_table`/`map_kernel` | 属于 02-stage-vm |
| 异常帧解析（arch 细节） | 属于 arch 文档 |

---

## 4. 与 runtime-design-glm.md 的差异

### 4.1 组织原则的差异

| 维度 | runtime-design-glm | runtime-design-ds（本文） |
|------|-------------------|--------------------------|
| 核心隐喻 | **三个并发循环**：调度/IPC/中断 | **状态机**：状态定义→状态转换→事件源→服务 |
| 叙事顺序 | 进程状态→调度→IPC→中断→syscall→fork→signal→copy→... | 同左，但强调每篇文档只依赖编号更小的 |
| VMCTL 位置 | 在 syscall 组中（16） | 在 syscall 组中（13），作为「第一个运行时 syscall」先行 |
| fork/exec 范围 | 合并 fork+exec+clear+exit+privctl+runctl 为一篇 | 合并为一篇（15），强调共同主题「进程生命周期」 |
| 中断覆盖 | 异常+中断+时钟一篇 | 中断路由+时钟中断（不含异常帧细节） |
| SMP | 嵌入调度文档 | 独立文档（20），因涉及跨文档横切 |

### 4.2 覆盖差

| 内容 | runtime-design-glm | runtime-design-ds（本文） |
|------|-------------------|--------------------------|
| proc.h 全字段分群 | 按字段列 | 按语义分组（标识/寄存器/调度/IPC/VM/记账） |
| RTS 标志位 | 逐位列出 | 逐位+核心不变量（runnable iff flags==0） |
| MiscFlags 与 check_misc_flags 的关系 | 未强调 | 强调——这是理解 switch_to_user 循环的关键 |
| IPC filter（`allow_ipc_filtered_memreq`） | 未覆盖 | 在 VMCTL MEMREQ_GET 中提及 |
| `has_pending` 的 `MF_SENDA_VM_MISS` 细节 | 未覆盖 | 在 SMP 部分提及 |
| kernel_call vs do_ipc 的区别 | 未区分 | 区分——IPC 调用走 do_ipc，其他 syscall 走 kernel_call |
| `priv_add_irq/io/mem` | 未独立 | 分别归属 device/fork 文档 |

---

## 5. 文件归属：tmp- 文件的最终去向

| tmp- 文件 | 内容 | 归属 |
|-----------|------|------|
| `tmp-02-page-table-kernel.md` | createpde/lin_lin_copy | → **废弃**（Direct Map 下被消除），保留为 reference |
| `tmp-03-vm-request.md` | VMREQUEST 挂起/恢复 | → 合并到 13-syscall-vmctl + 12-kernel-call |
| `tmp-04-protection.md` | GDT/IDT/TSS 初始化 | → 已在 kboot 03 覆盖，移入 reference/ |
| `tmp-05-exception-interrupt.md` | 异常+中断 | → 重构为 11-exception-interrupt（仅运行时部分） |
| `tmp-06-proc-struct.md` | proc 结构体字段 | → 重构为 08-process-state（强调状态语义） |
| `tmp-07-scheduling.md` | 调度循环 | → 重构为 09-scheduling |
| `tmp-08-endpoint.md` | endpoint 机制 | → 合并到 08-process-state |
| `tmp-09-sync-ipc.md` | 同步 IPC | → 重构为 10-sync-ipc |
| `tmp-10-async-ipc.md` | 异步 IPC | → 合并到 10-sync-ipc |
| `tmp-11-privilege.md` | priv 结构体 | → 合并到 08-process-state |
| `tmp-12-syscall-dispatch.md` | 系统调用分派 | → 重构为 12-kernel-call |
| `tmp-13-syscall-memory.md` | SYS_VMCTL 等 | → 拆分为 13-syscall-vmctl + 14-syscall-copy |
| `tmp-14-syscall-fork-exec.md` | fork/exec | → 重构为 15-syscall-fork-exec |
| `tmp-15-syscall-exit-signal.md` | exit/signal | → 拆分为 15（exit）+ 16（signal） |
| `tmp-16-timer.md` | 时钟 | → 重构为 18-syscall-clock |
| `tmp-17-main-init.md` | kmain 全流程 | → 已在 kboot 03-07 拆分覆盖，移入 reference/ |
| `tmp-18-smp.md` | SMP | → 重构为 20-smp |
| `tmp-19-debug-serial.md` | 调试串口 | → 不属于「运行时机制」主线，移入 reference/ |
| `tmp-20-acpi-watchdog.md` | ACPI/看门狗 | → 不属于主线，移入 reference/ |
| `tmp-21-unported-symbols.md` | 未移植符号 | → 移入 reference/ |

---

## 6. 设计决策记录

### 6.1 为什么不按「三个循环」组织

按循环组织（调度循环、IPC 循环、中断循环）的假设前提是读者已经理解了三个循环分别是什么。但对于第一次阅读内核代码的人，「循环」并不是一个自然的组织单元——进程状态、调度、IPC 之间的依赖关系远比「循环」复杂。

状态机视角的好处：
- 每个文档回答一个具体问题（「进程有哪些状态？」→「如何选择下一个进程？」→「IPC 如何改变状态？」）
- 依赖关系是线性的（08→09→10→11→12→...）
- 与 C 源码的组织方式一致（proc.c 先定义状态，再定义调度，再定义 IPC）

### 6.2 为什么 VMCTL 放在拷贝之前

safecopy 中的缺页会触发 vm_suspend → VMCTL_MEMREQ_GET/REPLY 路径。读者如果不先理解 VMCTL 协议，就无法理解 safecopy 的缺页恢复。VMCTL 是内核与 VM 之间的「元协议」——它定义了内核如何请求 VM 介入。

### 6.3 为什么 SMP 是独立文档而不是嵌入各文档

SMP 是一个横切关注点：调度、IPC、中断、页表都有 SMP 条件编译分支。但如果在每篇文档中都展开 SMP 细节，会导致信息碎片化。将 SMP 相关约束和机制集中在一篇文档中，各文档只需引用「详见 20-smp」。

### 6.4 Direct Map 对各文档的影响

| 文档 | 影响 |
|------|------|
| 08-process-state | 无影响（struct proc 不依赖映射方式） |
| 09-scheduling | 无影响（调度与地址映射无关） |
| 10-sync-ipc | 无影响（IPC 逻辑与映射无关） |
| 11-exception-interrupt | 无影响（中断路由与映射无关） |
| 12-kernel-call | VMSUSPEND 协议不变（与映射方式无关） |
| 13-syscall-vmctl | KERN_PHYSMAP → Direct Map 下可能简化 |
| 14-syscall-copy | createpde/lin_lin_copy → kernel_phys_to_virt，grant 不变 |
| 15~19 | 无影响 |

---

## 7. 实现优先级

| 优先级 | 任务 | 依赖 |
|--------|------|------|
| **P0** | 撰写 08-process-state.md | kboot 07 完成 |
| **P0** | 撰写 09-scheduling.md | 08 完成 |
| **P0** | 撰写 10-sync-ipc.md | 08, 09 完成 |
| **P0** | 撰写 11-exception-interrupt.md | 10 完成 |
| **P0** | 撰写 12-kernel-call.md | 10, 11 完成 |
| **P1** | 撰写 13-syscall-vmctl.md | 12 完成 |
| **P1** | 撰写 14-syscall-copy.md | 12, 13 完成 |
| **P1** | 撰写 15~19（syscall 各组） | 12 完成 |
| **P1** | 撰写 20-smp.md | 09, 11 完成 |
| **P2** | 清理 tmp- 文件，移入 reference/ | 文档全部完成后 |
| **P2** | 更新 00-kernel-overview.md 的文档索引 | 文档全部完成后 |
| **P2** | 撰写 99-global-concepts.md（跨文档共享概念速查） | 文档全部完成后 |

---

## 8. 参考资料

- `minix3/minix/kernel/proc.h:1-290` — struct proc 完整定义
- `minix3/minix/kernel/proc.c:1-1980` — 调度 + IPC 全部实现
- `minix3/minix/kernel/system.c:1-900+` — 系统调用分派 + 特权管理
- `minix3/minix/kernel/system/` — 40 个 do_*.c 文件
- `minix3/minix/kernel/interrupt.c:1-170` — 中断子系统
- `minix3/minix/kernel/clock.c:1-312` — 时钟中断 + 定时器
- `minix3/minix/kernel/smp.c:1-205` — SMP 支持
- `minix3/minix/kernel/main.c:38-137` — bsp_finish_booting (boot 终点)
- 本文档所在目录:
  - [kboot-new.md](kboot-new.md) — boot 阶段文档规划（01~07）
  - [00-kernel-overview.md](00-kernel-overview.md) — 内核整体概览
  - [kboot-design.md](kboot-design.md) — boot 架构设计决策
  - 跨目录: [../02-stage-vm/](../02-stage-vm/) — VM 侧文档（VMCTL handler、页表操作）