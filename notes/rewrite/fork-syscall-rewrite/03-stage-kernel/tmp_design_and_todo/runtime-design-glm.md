# runtime-design-glm: 03-stage-kernel 运行时文档规划

> **创建**: 2026-06-13
> **前置**: [kboot-new.md](kboot-new.md) — 01~07 覆盖 boot 阶段，07 结束于 `switch_to_user()`
> **范围**: `switch_to_user()` 之后，kernel 的全部运行时机制
> **方法**: 以"读者第一次理解运行时"的顺序组织，每篇文档解决一个独立问题，严格线性依赖
> **原则**: 一本账原则 —— 每个 C 函数只属于一篇文档，每篇文档覆盖一个语义独立的子系统

---

## 0. 核心洞察：运行时不是 boot 的延续，而是三个循环

boot 阶段（01~07）是**线性叙事**：kmain 从头到尾执行一遍，switch_to_user() 是终点。

运行时阶段完全不同——它是**三个并发循环**的交织：

| 循环 | 入口 | 触发 | 核心函数 | C 源码 |
|------|------|------|----------|--------|
| **调度循环** | `switch_to_user()` | 永不退出 | `pick_proc()` → `restore_user_context()` | proc.c:299-474 |
| **IPC 循环** | `do_ipc()` | 进程陷入 | `mini_send/receive/notify` | proc.c:479-698 |
| **中断循环** | `exception_handler()` / `irq_handle()` | 硬件异步 | `pagefault()` / `timer_int_handler()` | exception.c:180+, interrupt.c:116+, clock.c:70 |

**叙事策略**：不能按"循环"组织（读者会迷失在交叉引用中），必须按**依赖关系**线性展开：
1. 先理解进程状态（谁在跑、谁在等）
2. 再理解调度（怎么选进程）
3. 然后理解 IPC（进程间如何通信）
4. 再理解中断/异常（外部事件如何打断）
5. 最后理解系统调用（用户态如何请求内核服务）

---

## 1. 运行时子系统全图

### 1.0 依赖关系图

```
08-process-state ─────────────────────────────────────┐
  │ 进程状态机 (RtsFlags/MiscFlags/VmSuspend)         │
  │ proc.h: struct proc 全字段语义                     │
  ▼                                                    │
09-scheduling ────────────────────────────────────────┤
  │ switch_to_user / pick_proc / enqueue / dequeue    │
  │ 优先级队列 / 量子 / 抢占 / BKL / SMP IPI          │
  ▼                                                    │
10-sync-ipc ──────────────────────────────────────────┤
  │ do_ipc / mini_send / mini_receive / mini_notify   │
  │ 死锁检测 / 权限检查 / 消息投递                     │
  ▼                                                    │
11-exception-interrupt ───────────────────────────────┤
  │ exception_handler / pagefault / irq_handle        │
  │ 中断入口 → 通知 → 调度 → 返回用户态               │
  ▼                                                    │
12-kernel-call-dispatch ──────────────────────────────┤
  │ kernel_call / kernel_call_dispatch / call_vec[]   │
  │ VMSUSPEND 挂起-恢复协议                           │
  ▼                                                    │
13-syscall-process ───────────────────────────────────┤
  │ SYS_FORK / SYS_EXEC / SYS_CLEAR / SYS_EXIT       │
  │ SYS_PRIVCTL / SYS_RUNCTL / SYS_UPDATE / SYS_STATECTL │
  ▼                                                    │
14-syscall-signal ────────────────────────────────────┤
  │ SYS_KILL / cause_sig / do_getksig / do_sigsend   │
  │ 信号挂起 / 信号投递 / POSIX 信号帧                │
  ▼                                                    │
15-syscall-copy ──────────────────────────────────────┤
  │ SYS_VIRCOPY / SYS_PHYSCOPY / SYS_SAFECOPYFROM/TO │
  │ SYS_VSAFECOPY / SYS_UMAP / SYS_VUMAP             │
  │ grant 验证 / 跨进程拷贝 / Direct Map 替代         │
  ▼                                                    │
16-syscall-vmctl ─────────────────────────────────────┤
  │ SYS_VMCTL: VMCTL_SETADDRSPACE / VMCTL_GET_PDBR   │
  │ VMCTL_MEMREQ_GET/REPLY / VMCTL_VMINHIBIT_SET/CLEAR│
  │ VMCTL_CLEAR_PAGEFAULT / VMCTL_KERN_PHYSMAP        │
  ▼                                                    │
17-syscall-device ────────────────────────────────────┤
  │ SYS_IRQCTL / SYS_DEVIO / SYS_VDEVIO / SYS_SDEVIO │
  │ IRQ hook / I/O 端口 / 向量化 I/O                  │
  ▼                                                    │
18-syscall-clock ─────────────────────────────────────┤
  │ SYS_TIMES / SYS_SETALARM / SYS_STIME / SYS_VTIMER │
  │ 时钟记账 / 同步闹钟 / 虚拟定时器                  │
  ▼                                                    │
19-syscall-misc ──────────────────────────────────────┘
  │ SYS_ABORT / SYS_GETINFO / SYS_DIAGCTL / SYS_TRACE│
  │ SYS_SCHEDULE / SYS_SCHEDCTL / SYS_SETMCONTEXT     │
  │ SYS_GETMCONTEXT / SYS_SPROF / SYS_SETGRANT       │
  │ SYS_SAFEMEMSET / SYS_MEMSET / SYS_READBIOS       │
  │ SYS_IOPENABLE / SYS_STATECTL                      │
  └────────────────────────────────────────────────────┘
```

### 1.1 文档编号与 boot 阶段的关系

| 编号 | 文件名 | 覆盖 | C 源码核心 | 行数 |
|------|--------|------|-----------|------|
| 01~07 | (kboot-new 已规划) | boot 阶段 | main.c + arch init | — |
| **08** | `08-process-state.md` | 进程状态机 | proc.h + proc.c:119-160 | ~300 |
| **09** | `09-scheduling.md` | 调度循环 | proc.c:299-474, 1592-1870 | ~500 |
| **10** | `10-sync-ipc.md` | 同步 IPC | proc.c:479-1196 | ~700 |
| **11** | `11-exception-interrupt.md` | 异常与中断 | exception.c + interrupt.c + clock.c | ~400 |
| **12** | `12-kernel-call-dispatch.md` | 系统调用分派 | system.c:1-167 | ~300 |
| **13** | `13-syscall-process.md` | 进程管理调用 | do_fork/exec/clear/exit/privctl/runctl/update/statectl | ~600 |
| **14** | `14-syscall-signal.md` | 信号系统 | do_kill/getksig/endksig/sigsend/sigreturn + cause_sig | ~400 |
| **15** | `15-syscall-copy.md` | 跨进程拷贝 | do_safecopy/umap/vircopy/copy + verify_grant | ~600 |
| **16** | `16-syscall-vmctl.md` | VM 控制调用 | do_vmctl + arch_do_vmctl | ~400 |
| **17** | `17-syscall-device.md` | 设备 I/O 调用 | do_irqctl/devio/vdevio/sdevio | ~400 |
| **18** | `18-syscall-clock.md` | 时钟调用 | do_times/setalarm/stime/vtimer | ~300 |
| **19** | `19-syscall-misc.md` | 杂项调用 | do_abort/getinfo/diagctl/trace/schedctl/schedule/sprof/mcontext/setgrant/safememset/memset | ~500 |

---

## 2. 各文档详细规划

### 08-process-state: 进程状态机

> **核心问题**: 一个进程在内核中有哪些状态？状态之间如何转换？
> **前置**: 07 (boot 完成，进程表已填充)
> **C 源码**: `proc.h` (struct proc 全字段), `proc.c:119-160` (proc_init)

#### Ch1: 概念

- 进程表结构：`struct proc` 全字段语义分组
  - 寄存器保存区：`p_reg` (stackframe_s), `p_seg` (segframe: CR3/FPU)
  - 身份字段：`p_nr`, `p_endpoint`, `p_name`
  - 状态标志：`p_rts_flags` (16 位标志), `p_misc_flags` (运行时杂项)
  - 调度字段：`p_priority`, `p_cpu_time_left`, `p_quantum_size_ms`, `p_scheduler`, `p_cpu`
  - IPC 字段：`p_nextready`, `p_caller_q`, `p_q_link`, `p_getfrom_e`, `p_sendto_e`
  - 消息字段：`p_sendmsg`, `p_delivermsg`, `p_delivermsg_vir`
  - VM 挂起字段：`p_vmrequest` (type/target/params/vmresult/nextrequestor/nextrestart)
  - 记账字段：`p_accounting`, `p_user_time`, `p_sys_time`, `p_cycles`

- RtsFlags 完整语义（进程可运行 iff `p_rts_flags == 0`）：
  ```
  RTS_SLOT_FREE    0x001  — 槽位空闲
  RTS_PROC_STOP    0x002  — 进程被停止（boot 初始状态）
  RTS_SENDING      0x004  — 阻塞于发送
  RTS_RECEIVING    0x008  — 阻塞于接收
  RTS_SIGNALED     0x010  — 有内核信号到达
  RTS_SIG_PENDING  0x020  — 信号处理中，暂不可调度
  RTS_P_STOP       0x040  — 被 ptrace 停止
  RTS_NO_PRIV      0x080  — fork 后等待特权设置
  RTS_NO_ENDPOINT  0x100  — 端点失效，不可 IPC
  RTS_VMINHIBIT    0x200  — VM 尚未设置页表
  RTS_PAGEFAULT    0x400  — 有未处理缺页
  RTS_VMREQUEST    0x800  — VM 内存请求发起者
  RTS_VMREQTARGET  0x1000 — VM 内存请求目标
  RTS_PREEMPTED    0x4000 — 被高优先级抢占
  RTS_NO_QUANTUM   0x8000 — 量子耗尽
  RTS_BOOTINHIBIT  0x10000 — boot 阶段等待 VM
  ```

- MiscFlags 语义（不阻塞调度，但影响行为）：
  ```
  MF_DELIVERMSG       — 有待投递消息
  MF_KCALL_RESUME     — 内核调用需恢复（VMSUSPEND 后）
  MF_SC_DEFER         — 系统调用延迟（ptrace）
  MF_SC_ACTIVE        — 系统调用活跃
  MF_SC_TRACE         — 系统调用跟踪
  MF_REPLY_PEND       — SENDREC 的回复待收
  MF_MSGFAILED        — 消息投递失败（缺页）
  MF_CONTEXT_SET      — 上下文已设置（sigreturn）
  MF_FLUSH_TLB        — TLB 需刷新（SMP）
  MF_SENDING_FROM_KERNEL — 消息来自内核
  MF_VIRT_TIMER       — 虚拟定时器活跃
  MF_PROF_TIMER       — 性能定时器活跃
  MF_FPU_INITIALIZED  — FPU 状态已初始化
  MF_SENDA_VM_MISS    — 异步发送因 VMINHIBIT 失败（SMP）
  ```

- 特权结构 `struct priv`：系统进程独占，用户进程共享
  - `s_flags`: PREEMPTIBLE / BILLABLE / SYS_PROC
  - `s_trap_mask`: 允许的 IPC 陷阱号
  - `s_ipc_to`: 允许发送的目标位图
  - `s_k_call_mask`: 允许的内核调用位图
  - `s_notify_pending` / `s_asyn_pending`: 挂起通知/异步消息
  - `s_sig_mgr` / `s_bak_sig_mgr`: 信号管理器
  - `s_alarm_timer`: 同步闹钟
  - `s_grant_table` / `s_grant_entries`: grant 表

- VmSuspend 机制概览（详细在 12 和 16）
  - `p_vmrequest.type`: VMSTYPE_KERNELCALL / DELIVERMSG / MAP
  - `p_vmrequest.req_type`: VMPTYPE_CHECK
  - 挂起链表：`vmrequest` 全局指针

#### Ch2: C 源码分析

- `proc.h:1-230` — struct proc 定义 + RtsFlags + MiscFlags 宏
- `proc.c:119-160` — proc_init(): 初始化进程表
- `priv.h:1-105` — struct priv 定义 + 权限宏
- `vm.h:6` — `#define VMSUSPEND (-996)`

#### Ch3: 设计决策

- RtsFlags → bitflags 结构体（Rust 惯用法）
- MiscFlags → bitflags 结构体
- struct proc → KProcess（字段分组为子结构体：SchedFields, Accounting, TimeStats 等）
- struct priv → KPriv + PrivTable
- VmSuspend → VmSuspendContext enum（类型安全的状态机）
- 64 位适配：p_cr3 扩展为 u64，FPU 状态扩展为 XSAVE 区域

#### Ch4: 实现

- 对应 Rust 代码：`os/kernel/src/proc.rs` (KProcess), `os/kernel/src/kpriv.rs` (KPriv/PrivTable)
- RtsFlagsBits / MiscFlagsBits 已实现
- VmSuspendContext / VmRequestQueue 已实现

---

### 09-scheduling: 调度循环

> **核心问题**: switch_to_user() 如何选择下一个进程？进程如何进出运行队列？
> **前置**: 08 (理解进程状态)
> **C 源码**: `proc.c:299-474` (switch_to_user), `proc.c:1592-1870` (enqueue/dequeue/pick_proc)

#### Ch1: 概念

- **调度循环**：switch_to_user() 是内核的"主循环"
  ```
  while (true) {
      1. 当前进程可运行？→ 检查 misc_flags
      2. 不可运行？→ pick_proc() 选新进程
      3. pick_proc() 返回 NULL？→ idle() 等待中断
      4. 切换地址空间
      5. 处理 misc_flags（消息投递/内核调用恢复/ptrace）
      6. 检查量子
      7. restore_user_context() — 永不返回
  }
  ```

- **优先级队列**：NR_SCHED_QUEUES 个 FIFO 队列
  - TASK_Q (0): 内核任务（时钟、系统任务）— 不可抢占
  - SERVER_Q (1~2): 系统服务（VM、PM、VFS、RS）
  - USER_Q (3~15): 用户进程
  - IDLE_Q (15): 空闲进程

- **enqueue/dequeue**：
  - enqueue: 尾部插入；如果优先级高于当前进程且当前可抢占 → 设 RTS_PREEMPTED
  - enqueue_head: 头部插入（抢占后恢复用）
  - dequeue: 从队列中移除（阻塞/退出时）

- **pick_proc**：从最高优先级非空队列取队首

- **量子管理**：
  - `p_cpu_time_left` 以 CPU 周期为单位
  - 量子耗尽 → RTS_NO_QUANTUM → 通知调度器 → 重新 enqueue

- **SMP 调度**：
  - 每个CPU 有独立的 run_q_head/run_q_tail
  - 跨 CPU enqueue → smp_schedule() IPI 唤醒目标 CPU
  - 进程 CPU 亲和性：`p_cpu`, `p_cpu_mask`
  - BKL (Big Kernel Lock)：`big_kernel_lock` 自旋锁
    - 进入内核时获取，离开时释放
    - 保证同一时刻只有一个 CPU 在内核态
    - 自旋锁中禁止睡眠/调度

- **idle 进程**：
  - 所有 CPU 共享一个 idle_priv
  - 无进程可运行时执行 idle()：hlt 等待中断
  - 时钟中断后重新 pick_proc

#### Ch2: C 源码分析

- `proc.c:299-474` — switch_to_user()
- `proc.c:1592-1712` — enqueue() / enqueue_head()
- `proc.c:1716-1780` — dequeue()
- `proc.c:1785-1810` — pick_proc()
- `proc.c:173-230` — idle()
- `smp.c:1-50` — BKL 定义 + SMP 初始化

#### Ch3: 设计决策

- 调度器 trait 抽象 vs 直接实现
  - Minix3 调度策略简单（固定优先级 + FIFO），不需要策略/机制分离
  - 但外部调度器（PM/RS）通过 SYS_SCHEDULE 介入
  - Rust: Scheduler struct 封装 run_q_head/run_q_tail

- BKL 在 Rust 中的表达
  - `SpinLock<()>` 或专门的 BKL 类型
  - 进入/退出点：所有内核入口（syscall/exception/IRQ）
  - 关键约束：持锁期间禁止调度/睡眠

- 64 位适配
  - p_cpu_time_left: u32 → u64（TSC 周期）
  - SMP: p_cpu_mask 从 bitchunk_t → Bitmap<u64>

#### Ch4: 实现

- 对应 Rust 代码：`os/kernel/src/sched.rs` (Scheduler)
- 已实现：pick_proc, enqueue_queue_tail, enqueue_queue_head, dequeue_from_queue

---

### 10-sync-ipc: 同步 IPC

> **核心问题**: 进程间如何发送/接收消息？阻塞语义是什么？
> **前置**: 08 (进程状态), 09 (调度 — IPC 阻塞导致 dequeue)
> **C 源码**: `proc.c:479-1196` (do_ipc/mini_send/mini_receive/mini_notify)

#### Ch1: 概念

- **IPC 入口**：do_ipc(r1, r2, r3)
  - r1 = call_nr (SEND/RECEIVE/SENDREC/NOTIFY/SENDNB/SENDA/MINIX_KERNINFO)
  - r2 = src_dst endpoint
  - r3 = message pointer

- **权限检查**（do_sync_ipc 前置）：
  1. call_nr 范围检查 (0~IPCNO_HIGHEST, < 32)
  2. endpoint 有效性 (isokendpt)
  3. may_send_to 检查 (s_ipc_to 位图)
  4. trap_mask 检查 (s_trap_mask)
  5. 内核进程只接受 SENDREC

- **mini_send(caller, dst_e, m_ptr, flags)**:
  ```
  if 目标正在等待此消息 (WILLRECEIVE):
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

- **mini_receive(caller, src_e, m_buff_usr, flags)**:
  ```
  1. 检查挂起通知 (has_pending_notify)
     → 找到 → 组装通知消息 → MF_DELIVERMSG → 返回
  2. 检查挂起异步消息 (has_pending_asend → try_async)
     → 找到 → 投递 → 返回
  3. 检查发送者队列 (p_caller_q)
     → 找到匹配 → 投递 → RTS_UNSET(sender, RTS_SENDING) → 返回
  4. 无消息可用:
     if NON_BLOCKING: return ENOTREADY
     deadlock 检测
     RTS_SET(caller, RTS_RECEIVING)  ← 阻塞自己
  ```

- **mini_notify(caller, dst_e)**:
  ```
  if 目标正在等待且非 MF_REPLY_PEND:
      组装通知消息 → MF_DELIVERMSG → RTS_UNSET(dst, RTS_RECEIVING)
  else:
      设置 dst.s_notify_pending 位图
  ```

- **SENDREC**: SEND + RECEIVE 原子组合
  - 设置 MF_REPLY_PEND 防止通知打断
  - SEND 失败 → 不 RECEIVE
  - SEND 成功 → 自动进入 RECEIVE

- **SENDNB**: 非阻塞 SEND
  - 目标不等待 → 返回 ENOTREADY（不阻塞）

- **SENDA**: 异步批量发送
  - 扫描 asynmsg_t 表，逐个 try_deliver_senda
  - 不阻塞发送者

- **死锁检测**：deadlock() 函数
  - 沿着等待链检查是否形成环
  - SEND: caller → dst → dst.p_getfrom_e → ...
  - RECEIVE: 不检测（接收不形成环）

- **消息投递**：delivermsg()
  - copy_msg_to_user → 成功则清除 MF_DELIVERMSG
  - 失败 → vm_suspend(VMSTYPE_DELIVERMSG) → 等待 VM 修复

- **WILLRECEIVE / CANRECEIVE 宏**：
  - WILLRECEIVE: 目标正在接收且源匹配
  - CANRECEIVE: 源端点匹配（ANY 或特定端点）

#### Ch2: C 源码分析

- `proc.c:479-598` — do_sync_ipc()
- `proc.c:599-698` — do_ipc()
- `proc.c:703-770` — deadlock()
- `proc.c:770-860` — has_pending/has_pending_notify/has_pending_asend
- `proc.c:870-965` — mini_send()
- `proc.c:967-1117` — mini_receive()
- `proc.c:1122-1196` — mini_notify()
- `proc.c:1197-1330` — try_deliver_senda()
- `proc.c:1331-1387` — mini_senda() / try_async()
- `proc.c:1390-1507` — try_one()
- `proc.c:1507-1590` — cancel_async()

#### Ch3: 设计决策

- IPC trait vs 函数
  - Minix3 IPC 是内核全局函数，不是对象方法
  - Rust: IpcTransport trait（抽象 send/receive/notify）
  - 用户态服务通过 minix-sys::sendrec() 调用 → 内核陷入 → do_ipc()

- 消息类型
  - Minix3: `message` 联合体（定长 64 字节）
  - Rust: `Message` 结构体 + 类型安全的访问器

- 死锁检测
  - 保留：防止用户进程死锁内核服务
  - 优化：环检测可以用 BFS 替代递归

- 异步 IPC (SENDA)
  - 保留：驱动程序需要异步通知
  - Rust: asynmsg_t → AsyncMessageTable 结构

#### Ch4: 实现

- 对应 Rust 代码：尚未实现（依赖 IpcTransport trait 设计）
- VM 侧：`os/servers/vm/src/ipc_dispatch.rs` (stub)

---

### 11-exception-interrupt: 异常与中断

> **核心问题**: 硬件事件如何打断进程？内核如何响应？
> **前置**: 08 (进程状态), 09 (调度), 10 (IPC — 通知机制)
> **C 源码**: `exception.c`, `interrupt.c`, `clock.c`, `arch/i386/apic.c`

#### Ch1: 概念

- **异常入口**：exception_handler(is_nested, frame)
  - is_nested=0: 用户态异常 → cause_sig 或 pagefault
  - is_nested=1: 内核态异常 → inkernel_disaster 或特殊处理

- **缺页处理**：pagefault(pr, frame, is_nested)
  ```
  1. 读 CR2 获取缺页地址
  2. 内核态缺页:
     - phys_copy 范围内 → 跳转到 fault handler
     - 其他 → panic
  3. VM 进程缺页 → panic（VM 不能缺页）
  4. 用户态缺页:
     - RTS_SET(pr, RTS_PAGEFAULT)
     - mini_send(pr, VM_PROC_NR, &m_pagefault, FROM_KERNEL)
     - VM 处理后 → VMCTL_CLEAR_PAGEFAULT
  ```

- **中断处理**：irq_handle(irq)
  ```
  1. 遍历 irq_handlers[irq] 链表
  2. 调用 hook->handler 或 mini_notify(hook->proc_nr_e)
  3. 如果不是重新使能策略 → disable_irq
  ```

- **时钟中断**：timer_int_handler()
  ```
  1. 更新 realtime / boottime / monotonic
  2. 检查 alarm timers → 通知超时进程
  3. 更新当前进程的 user/sys time
  4. 消耗量子 → RTS_NO_QUANTUM
  5. 虚拟/性能定时器检查
  ```

- **中断入口到调度的完整路径**：
  ```
  硬件中断 → IDT entry → 保存寄存器
    → BKL_LOCK()
    → irq_handle(irq) → mini_notify(target)
    → switch_to_user() → pick_proc()
    → BKL_UNLOCK() (在 restore_user_context 前)
    → restore_user_context()
  ```

- **SMP 中断路由**：
  - APIC: 可编程中断路由到特定 CPU
  - IPI: CPU 间中断（smp_schedule, smp_schedule_vminhibit）

#### Ch2: C 源码分析

- `exception.c:180-286` — exception_handler()
- `exception.c:49-131` — pagefault()
- `interrupt.c:25-70` — put_irq_handler()
- `interrupt.c:75-107` — rm_irq_handler()
- `interrupt.c:116-160` — irq_handle()
- `clock.c:70-174` — timer_int_handler()
- `arch/i386/apic.c` — APIC 编程

#### Ch3: 设计决策

- 异常/中断 trait 抽象
  - `ExceptionHandler` trait: handle_exception(vector, frame)
  - `InterruptController` trait: enable/disable/ack/eoi
  - `ClockSource` trait: init/set_frequency/get_time

- 缺页 → VM 通知路径
  - 保留：内核不处理缺页，委托给 VM
  - Rust: pagefault → VmRequestQueue.enqueue_and_notify()

- 时钟子系统
  - 保留 alarm timer 语义
  - Rust: TimerWheel 或 TimerQueue

#### Ch4: 实现

- 对应 Rust 代码：`os/kernel/src/irq_manager.rs` (IrqManager)
- 已实现：register_hook, remove_hook, dispatch, enable_irq, disable_irq

---

### 12-kernel-call-dispatch: 系统调用分派

> **核心问题**: 用户态如何请求内核服务？VMSUSPEND 挂起-恢复协议如何工作？
> **前置**: 10 (IPC — kernel_call 通过消息传递), 11 (异常 — syscall 入口)
> **C 源码**: `system.c:1-167`

#### Ch1: 概念

- **系统调用入口**：kernel_call(m_user, caller)
  ```
  1. copy_msg_from_user(m_user, &msg) — 复制请求消息
     失败 → cause_sig(SIGSEGV)
  2. msg.m_source = caller.p_endpoint
  3. result = kernel_call_dispatch(caller, &msg)
  4. kernel_call_finish(caller, &msg, result)
  ```

- **分派**：kernel_call_dispatch(caller, msg)
  ```
  call_nr = msg.m_type - KERNEL_CALL
  if call_nr 越界 → EBADREQUEST
  if !s_k_call_mask[call_nr] → ECALLDENIED
  result = call_vec[call_nr](caller, msg)
  ```

- **VMSUSPEND 协议**：kernel_call_finish 中的关键分支
  ```
  if result == VMSUSPEND:
      保存请求消息到 p_vmrequest.saved.reqmsg
      MF_KCALL_RESUME = 1
      // 进程保持 RTS_VMREQUEST 状态，等待 VM 回复
  else:
      复制结果消息回用户空间
      失败 → cause_sig(SIGSEGV)
  ```

- **恢复**：kernel_call_resume(caller)
  ```
  1. 重新执行 kernel_call_dispatch（使用保存的消息）
  2. 清除 MF_KCALL_RESUME
  3. kernel_call_finish（可能再次 VMSUSPEND）
  ```

- **系统调用向量**：call_vec[NR_SYS_CALLS]
  - 38 个内核调用，按功能分组（见 system_init 中的 map 宏）

#### Ch2: C 源码分析

- `system.c:59-94` — kernel_call_finish()
- `system.c:95-128` — kernel_call_dispatch()
- `system.c:136-167` — kernel_call()
- `system.c:168-278` — system_init() (call_vec 注册)
- `system.c:612-638` — kernel_call_resume()

#### Ch3: 设计决策

- call_vec → 分派表 trait
  - 每个 do_xxx 函数 → 对应的 trait 方法
  - 按功能分组为子 trait：ProcessSyscall, SignalSyscall, CopySyscall, ...

- VMSUSPEND → Result<VmCheckResult>
  - VMSUSPEND 不是错误，是"需要 VM 介入"的信号
  - Rust: `enum CrossSpaceResult { Ok, VmSuspend, Fault }`

- 消息复制安全
  - copy_msg_from_user 可能触发缺页
  - 内核态缺页处理：特殊 fault handler

#### Ch4: 实现

- 对应 Rust 代码：`os/kernel/src/vm.rs` (VmRequestHandler, kernel_call_resume)
- 已实现：memreq_get, memreq_reply, kernel_call_resume, try_deliver_message

---

### 13-syscall-process: 进程管理调用

> **核心问题**: 内核如何创建/销毁/修改进程？
> **前置**: 08 (进程状态), 12 (系统调用分派)
> **C 源码**: `system/do_fork.c`, `system/do_exec.c`, `system/do_clear.c`, `system/do_exit.c`, `system/do_privctl.c`, `system/do_runctl.c`, `system/do_update.c`, `system/do_statectl.c`

#### Ch1: 概念

- **SYS_FORK** (do_fork): 创建子进程
  - 前置：父进程必须 RTS_RECEIVING（同步 fork）
  - 复制父进程 proc 结构到子进程槽位
  - 新 endpoint（generation +1）
  - 子进程 retreg = 0
  - RTS_NO_PRIV（系统进程 fork 后需 PRIVCTL 设置特权）
  - FPU 状态复制
  - VM 侧：页表复制（CoW）由 VM 处理，不在内核

- **SYS_EXEC** (do_exec): 执行新程序
  - 设置新 PC/SP/ps_strings
  - 更新进程名
  - 清除 MF_DELIVERMSG
  - arch_proc_init 重置寄存器

- **SYS_CLEAR** (do_clear): 清理进程槽位
  - clear_endpoint: RTS_NO_ENDPOINT + clear_ipc + clear_ipc_refs
  - 清除所有 IPC 状态（发送队列、接收状态、通知位图）

- **SYS_EXIT** (do_exit): 系统进程退出
  - 标记进程不再运行
  - 通知 PM/RS

- **SYS_PRIVCTL** (do_privctl): 特权控制
  - SYS_PRIV_ALLOW: 解除 RTS_NO_PRIV
  - SYS_PRIV_SET: 设置完整特权结构（trap_mask, ipc_to, k_call_mask, io, mem, irq）
  - SYS_PRIV_ADD: 增量添加（IRQ/I/O/MEM）
  - 只有 SYS_PROC 可以调用

- **SYS_RUNCTL** (do_runctl): 运行控制
  - 设置/清除 RTS_PROC_STOP
  - 停止/恢复进程

- **SYS_UPDATE** (do_update): 进程更新（live update）
  - 替换进程映像但保持 endpoint

- **SYS_STATECTL** (do_statectl): 状态控制
  - 核心转储、进程状态查询

#### Ch2: C 源码分析

- `system/do_fork.c:1-136` — do_fork()
- `system/do_exec.c:1-50` — do_exec()
- `system/do_clear.c:1-80` — do_clear()
- `system/do_exit.c` — do_exit()
- `system/do_privctl.c:1-371` — do_privctl()
- `system/do_runctl.c` — do_runctl()
- `system/do_update.c:1-340` — do_update()
- `system/do_statectl.c` — do_statectl()
- `system.c:540-574` — clear_endpoint()
- `system.c:509-537` — clear_ipc()

#### Ch3: 设计决策

- fork 的类型安全
  - 前置条件检查 → Result 类型
  - KProcess::fork_from() 已实现

- 特权管理
  - KPriv + PrivTable 已实现
  - configure_boot_priv() 已实现

- clear_ipc 的完整性
  - 必须清除：发送队列、接收状态、通知位图、异步消息
  - Rust: clear_ipc_refs() 遍历所有进程

---

### 14-syscall-signal: 信号系统

> **核心问题**: 内核如何向进程发送信号？信号如何投递？
> **前置**: 08 (进程状态), 10 (IPC — 通知机制), 12 (系统调用分派)
> **C 源码**: `system/do_kill.c`, `system/do_getksig.c`, `system/do_endksig.c`, `system/do_sigsend.c`, `system/do_sigreturn.c`, `system.c:364-465` (cause_sig/sig_delay_done)

#### Ch1: 概念

- **信号流程**（三阶段）：
  ```
  1. 产生: cause_sig(proc_nr, sig_nr)
     → 设置 p_pending 位图
     → RTS_SIGNALED
     → 通知信号管理器 (PM): send_sig(sig_mgr, SIGKMEM)

  2. 查询: PM 调用 SYS_GETKSIG
     → 遍历进程表找 RTS_SIGNALED 的进程
     → 返回 endpoint + pending 位图
     → RTS_SIGNALED → RTS_SIG_PENDING

  3. 投递: PM 调用 SYS_SIGSEND
     → 设置信号帧 (sigframe) 在用户栈
     → 修改进程 PC 跳转到信号处理函数
     → 处理完成后 SYS_SIGRETURN 恢复上下文
  ```

- **内核信号 vs POSIX 信号**：
  - 内核信号：通知机制，通过 IPC 消息传递给信号管理器
  - POSIX 信号：用户态信号处理，通过信号帧修改进程上下文

- **sig_delay_done**: 进程确认不再发送直接消息后，解除 SIG_DELAY

#### Ch2: C 源码分析

- `system.c:364-386` — send_sig()
- `system.c:389-451` — cause_sig()
- `system.c:454-465` — sig_delay_done()
- `system/do_kill.c:1-41` — do_kill()
- `system/do_getksig.c:1-43` — do_getksig()
- `system/do_endksig.c` — do_endksig()
- `system/do_sigsend.c:1-166` — do_sigsend()
- `system/do_sigreturn.c:1-98` — do_sigreturn()

#### Ch3: 设计决策

- 信号管理器抽象
  - `s_sig_mgr` 指定每个进程的信号管理器（通常是 PM）
  - Rust: SignalManager trait 或简单的 endpoint 引用

- 信号帧
  - 架构相关：x86_64 的 ucontext_t / sigframe 布局
  - Rust: SigContext 结构体，由 arch 模块定义

---

### 15-syscall-copy: 跨进程拷贝

> **核心问题**: 内核如何安全地在进程间拷贝数据？grant 机制如何工作？
> **前置**: 08 (进程状态), 12 (系统调用分派), 09-runtime-cross-space (Direct Map)
> **C 源码**: `system/do_safecopy.c`, `system/do_umap.c`, `system/do_umap_remote.c`, `system/do_vumap.c`, `system/do_copy.c`, `system/do_safememset.c`

#### Ch1: 概念

- **拷贝原语层次**：
  ```
  SYS_PHYSCOPY    — 物理地址到物理地址（内核专用）
  SYS_VIRCOPY     — 虚拟地址到虚拟地址（跨进程）
  SYS_SAFECOPYFROM — 通过 grant 从目标进程拷贝
  SYS_SAFECOPYTO  — 通过 grant 向目标进程拷贝
  SYS_VSAFECOPY   — 向量化 safecopy
  SYS_UMAP        — 虚拟地址到物理地址映射
  SYS_UMAP_REMOTE — 非调用者的 umap
  SYS_VUMAP       — 向量化 umap
  SYS_SAFEMEMSET  — 安全内存清零
  SYS_MEMSET      — 内存清零
  ```

- **Grant 机制**（safecopy 的核心）：
  - cp_grant_id_t: 授权标识符
  - grant 表：每个进程有一个 grant 表（s_grant_table）
  - verify_grant(): 验证授权范围和权限
  - 间接 grant：最多 5 层间接
  - CPF_TRY 标志：软故障（不杀死进程）

- **Direct Map 替代**：
  - 64 位下 `kernel_phys_to_virt()` 替代 `createpde`/`lin_lin_copy`
  - 但 grant 验证逻辑不变（权限检查与地址映射无关）
  - umap 退化为 `virt_to_phys()` + 权限检查

#### Ch2: C 源码分析

- `system/do_safecopy.c:1-448` — safecopy/verify_grant (最大单文件)
- `system/do_umap.c` — do_umap()
- `system/do_umap_remote.c:1-122` — do_umap_remote()
- `system/do_vumap.c:1-131` — do_vumap()
- `system/do_copy.c:1-91` — do_copy() / do_vircopy()
- `system/do_safememset.c` — do_safememset()
- `system/do_memset.c` — do_memset()

#### Ch3: 设计决策

- Grant 系统
  - 保留：权限验证是安全核心
  - Rust: GrantTable 结构体 + verify_grant() → Result

- Direct Map 下的拷贝
  - `cross_space_copy()` 已在 vm.rs 中设计
  - VA→PA 翻译 → `kernel_phys_to_virt()` → memcpy
  - 缺页 → VmSuspend

---

### 16-syscall-vmctl: VM 控制调用

> **核心问题**: 内核与 VM 之间的控制接口是什么？
> **前置**: 08 (进程状态), 12 (系统调用分派), 09-runtime-cross-space (Direct Map)
> **C 源码**: `system/do_vmctl.c`, `arch/i386/arch_do_vmctl.c`

#### Ch1: 概念

- **VMCTL 子命令**：

  | 子命令 | 功能 | 调用者 |
  |--------|------|--------|
  | VMCTL_GET_PDBR | 获取进程 CR3 | VM |
  | VMCTL_SETADDRSPACE | 设置进程 CR3 + 切换地址空间 | VM |
  | VMCTL_CLEAR_PAGEFAULT | 清除缺页状态 | VM |
  | VMCTL_MEMREQ_GET | 获取挂起的内存请求 | VM |
  | VMCTL_MEMREQ_REPLY | 回复内存请求结果 | VM |
  | VMCTL_VMINHIBIT_SET | 设置 VMINHIBIT（禁止调度） | VM |
  | VMCTL_VMINHIBIT_CLEAR | 清除 VMINHIBIT（允许调度） | VM |
  | VMCTL_KERN_PHYSMAP | 内核物理映射声明 | Kernel (via VM) |
  | VMCTL_KERN_MAP_REPLY | 内核物理映射回复 | Kernel (via VM) |
  | VMCTL_FLUSHTLB | 刷新 TLB | VM |
  | VMCTL_I386_INVLPG | 单页 TLB 失效 | VM |

- **VMCTL_SETADDRSPACE 的关键性**：
  - VM 设置进程的页目录基址（CR3）
  - 如果目标进程是 ptproc（当前地址空间所有者）→ 立即 write_cr3
  - 如果目标进程是 VM → arch_enable_paging（切换到 VM 的真实页表）
  - 清除 RTS_VMINHIBIT

- **VMCTL_MEMREQ_GET/REPLY 协议**：
  ```
  1. VM 调用 MEMREQ_GET → 内核返回第一个挂起请求
  2. VM 处理请求（映射页面/分配物理内存）
  3. VM 调用 MEMREQ_REPLY(result) → 内核恢复挂起进程
     - VMSTYPE_KERNELCALL → MF_KCALL_RESUME
     - VMSTYPE_DELIVERMSG → 重试消息投递
     - VMSTYPE_MAP → 重试映射操作
  ```

#### Ch2: C 源码分析

- `system/do_vmctl.c:1-173` — do_vmctl()
- `arch/i386/arch_do_vmctl.c:1-67` — arch_do_vmctl() + setcr3()

#### Ch3: 设计决策

- VMCTL → 枚举 + trait
  - VmCtlCommand enum
  - VmCtlHandler trait

- CR3 操作的安全性
  - setcr3 必须在 BKL 内
  - 切换地址空间后 TLB 一致性

- Rust 已实现
  - VmRequestHandler::memreq_get/memreq_reply
  - VmCtlError enum

---

### 17-syscall-device: 设备 I/O 调用

> **核心问题**: 驱动程序如何通过内核访问硬件？
> **前置**: 08 (进程状态), 11 (中断), 12 (系统调用分派)
> **C 源码**: `system/do_irqctl.c`, `system/do_devio.c`, `system/do_vdevio.c`, `arch/i386/do_sdevio.c`, `arch/i386/do_iopenable.c`, `arch/i386/do_readbios.c`

#### Ch1: 概念

- **SYS_IRQCTL**: 中断控制
  - IRQ_SETPOLICY: 注册 IRQ hook + 设置策略
  - IRQ_ENABLE/DISABLE: 使能/禁用 IRQ
  - IRQ_RCVHOOKID: 获取 hook ID
  - generic_handler: 通用中断处理 → mini_notify

- **SYS_DEVIO**: I/O 端口访问（x86 only）
  - 单次 inb/inw/inl/outb/outw/outl

- **SYS_VDEVIO**: 向量化 I/O（x86 only）
  - 批量 I/O 操作

- **SYS_SDEVIO**: 安全向量化 I/O（x86 only）
  - 通过 grant 验证的 I/O

- **SYS_IOPENABLE**: 启用 I/O 权限（x86 only）
  - 设置 IOPL 位

- **SYS_READBIOS**: 读取 BIOS 数据（x86 only）

#### Ch2: C 源码分析

- `system/do_irqctl.c:1-174` — do_irqctl() + generic_handler()
- `system/do_devio.c:1-107` — do_devio()
- `system/do_vdevio.c:1-165` — do_vdevio()
- `arch/i386/do_sdevio.c:1-163` — do_sdevio()
- `arch/i386/do_iopenable.c` — do_iopenable()
- `arch/i386/do_readbios.c` — do_readbios()

#### Ch3: 设计决策

- IRQ 管理
  - IrqManager<IC> 已实现（泛型化中断控制器）
  - IrqHook → 结构体 + IrqId

- I/O 端口
  - x86 特有，其他架构不需要
  - Rust: 条件编译 + trait 抽象

---

### 18-syscall-clock: 时钟调用

> **核心问题**: 进程如何获取时间/设置定时器？
> **前置**: 08 (进程状态), 11 (时钟中断), 12 (系统调用分派)
> **C 源码**: `system/do_times.c`, `system/do_setalarm.c`, `system/do_stime.c`, `system/do_settime.c`, `system/do_vtimer.c`

#### Ch1: 概念

- **SYS_TIMES**: 获取进程时间和系统 uptime
- **SYS_SETALARM**: 设置同步闹钟（到期后通知）
- **SYS_STIME**: 设置 boottime
- **SYS_SETTIME**: 设置 realtime
- **SYS_VTIMER**: 设置/查询虚拟定时器
  - 虚拟定时器：仅在用户态运行时递减
  - 性能定时器：用户态 + 内核态都递减

#### Ch2: C 源码分析

- `system/do_times.c` — do_times()
- `system/do_setalarm.c` — do_setalarm()
- `system/do_stime.c` — do_stime()
- `system/do_settime.c` — do_settime()
- `system/do_vtimer.c:1-103` — do_vtimer()

---

### 19-syscall-misc: 杂项调用

> **核心问题**: 不属于上述分类的内核调用
> **前置**: 12 (系统调用分派)
> **C 源码**: 多个小文件

#### 覆盖

- **SYS_ABORT**: 紧急停机
- **SYS_GETINFO**: 获取内核信息（进程表、特权表、内存映射等）
- **SYS_DIAGCTL**: 诊断控制
- **SYS_TRACE**: ptrace 跟踪
- **SYS_SCHEDULE**: 调度请求
- **SYS_SCHEDCTL**: 调度器控制
- **SYS_SPROF**: 统计性能分析
- **SYS_SETMCONTEXT / SYS_GETMCONTEXT**: 机器上下文切换
- **SYS_SETGRANT**: 设置 grant 表
- **SYS_SAFEMEMSET**: 安全内存清零
- **SYS_MEMSET**: 内存清零

---

## 3. 叙事线验证

### 3.1 依赖链完整性

```
08 (进程状态)
 → 09 (调度) — 需要理解 RtsFlags 才能理解 enqueue/dequeue
 → 10 (IPC) — 需要理解调度才能理解 IPC 阻塞
 → 11 (异常/中断) — 需要理解 IPC 才能理解通知机制
 → 12 (系统调用分派) — 需要理解异常入口才能理解 syscall 入口
 → 13~19 (各子系统调用) — 需要理解分派才能理解具体调用
```

### 3.2 C 源码覆盖验证

| C 源文件 | 行数 | 文档覆盖 |
|---------|------|---------|
| proc.c | 1980 | 08(状态) + 09(调度) + 10(IPC) |
| system.c | 997 | 12(分派) + 08(clear_ipc) + 14(cause_sig) |
| exception.c | 386 | 11 |
| interrupt.c | 177 | 11 |
| clock.c | 312 | 11 + 18 |
| smp.c | 205 | 09 |
| arch/i386/memory.c | 1020 | 09-runtime-cross-space (kboot-new) |
| arch/i386/arch_system.c | 673 | 09(FPU) + 11(arch_do_syscall) + 13(arch_proc_init) |
| arch/i386/exception.c | 386 | 11 |
| arch/i386/arch_do_vmctl.c | 67 | 16 |
| arch/i386/apic.c | 1304 | 11 |
| arch/i386/protect.c | 456 | boot (03) |
| system/do_*.c (38 files) | 4248 | 13~19 按功能分组 |

### 3.3 无遗漏检查

- `proc.c` 全部函数已分配到 08/09/10
- `system.c` 全部函数已分配到 12/14
- `system/do_*.c` 全部 38 个 do_xxx 已分配到 13~19
- `arch/i386/` 运行时文件已分配到对应文档
- `smp.c`, `watchdog.c`, `debug.c`, `profile.c`, `utility.c` — 辅助模块
  - smp.c → 09 (SMP 调度)
  - watchdog.c → 11 (中断子系统的一部分)
  - debug.c → 19 (诊断)
  - profile.c → 19 (SYS_SPROF)
  - utility.c → 工具函数，分散引用

---

## 4. 与 kboot-new 的衔接

### 4.1 07 → 08 的过渡

07 结束于 `switch_to_user()`。08 开始于"switch_to_user() 选择了谁？为什么？"

```
07: bsp_finish_booting() → 解除 boot_proc 的 RTS_PROC_STOP → switch_to_user()
08: 进程状态详解 — RtsFlags 的每个 bit 是什么意思
09: switch_to_user() 内部逻辑 — pick_proc() 如何选择
```

### 4.2 09-runtime-cross-space 的位置

kboot-new 规划的 `09-runtime-cross-space.md` 覆盖 createpde/lin_lin_copy/vm_lookup/VMSUSPEND。在本规划中：

- createpde/lin_lin_copy → 被 Direct Map 替代，放在 15-syscall-copy 的 Ch3 中
- vm_lookup → 放在 15-syscall-copy 中（umap 的底层机制）
- VMSUSPEND → 放在 12-kernel-call-dispatch 中（挂起-恢复协议）
- memory_init (freepdes) → 已在 06-cross-space-init 中

**结论**：kboot-new 的 09-runtime-cross-space 不再需要独立文档，其内容分散到 12 和 15 中。本规划的编号从 08 开始，不与 kboot-new 冲突。

---

## 5. 实现优先级

| 优先级 | 文档 | 理由 |
|--------|------|------|
| **P0** | 08-process-state | 所有运行时的基础 |
| **P0** | 09-scheduling | 调度循环是运行时的核心 |
| **P0** | 10-sync-ipc | IPC 是 Minix3 的灵魂 |
| **P1** | 11-exception-interrupt | 异常/中断是内核入口 |
| **P1** | 12-kernel-call-dispatch | 系统调用是用户态→内核态的桥梁 |
| **P1** | 13-syscall-process | 进程管理是核心功能 |
| **P2** | 14~19 | 各子系统调用，可按需撰写 |

---

## 6. Rust 实现现状映射

| 文档 | Rust 代码 | 状态 |
|------|----------|------|
| 08-process-state | `proc.rs` (KProcess, RtsFlagsBits, MiscFlagsBits) | 已实现核心 |
| 09-scheduling | `sched.rs` (Scheduler) | 已实现核心 |
| 10-sync-ipc | — | **未实现**（依赖 IpcTransport） |
| 11-exception-interrupt | `irq_manager.rs` (IrqManager) | 部分实现 |
| 12-kernel-call-dispatch | `vm.rs` (VmRequestHandler, kernel_call_resume) | 部分实现 |
| 13-syscall-process | `proc.rs` (fork_from, complete_fork_setup) | 部分实现 |
| 14-syscall-signal | — | **未实现** |
| 15-syscall-copy | `vm.rs` (cross_space_copy, cross_space_memset) | 框架已实现 |
| 16-syscall-vmctl | `vm.rs` (VmCtlError, VmRequestHandler) | 部分实现 |
| 17-syscall-device | `irq_manager.rs` | 部分实现 |
| 18-syscall-clock | — | **未实现** |
| 19-syscall-misc | — | **未实现** |

---

## 7. 关键设计决策记录

### 7.1 为什么不按"循环"组织？

三个循环（调度/IPC/中断）深度交织：
- 调度循环中处理 IPC 结果（delivermsg）
- IPC 中触发调度（阻塞/唤醒）
- 中断中触发 IPC（mini_notify）和调度（量子耗尽）

按循环组织会导致大量前向引用，读者无法线性阅读。

### 7.2 为什么系统调用拆成 7 篇（13~19）？

38 个 do_xxx 函数总计 ~4250 行 C 代码。如果放在一篇文档中：
- 文档过长（预计 2000+ 行）
- 读者只需要理解特定子系统时无法定位
- 不同子系统的前置知识不同（信号需要理解 IPC，设备需要理解中断）

按功能分组后，每篇 300~600 行，读者可以按需阅读。

### 7.3 为什么 IPC 放在调度之后？

Minix3 的 IPC 阻塞直接操作调度队列：
- mini_send → RTS_SENDING → dequeue
- mini_receive → RTS_RECEIVING → dequeue
- 消息投递 → RTS_UNSET(RECEIVING) → enqueue

不理解调度就无法理解 IPC 的阻塞语义。反之，调度不依赖 IPC 的内部逻辑（只关心 RtsFlags）。

### 7.4 08 为什么从"进程状态"开始而不是"调度"？

switch_to_user() 的每一步判断都基于 RtsFlags 和 MiscFlags：
- `proc_is_runnable(p)` → `p_rts_flags == 0`
- `MF_DELIVERMSG` → 投递消息
- `MF_KCALL_RESUME` → 恢复内核调用
- `RTS_PREEMPTED` → enqueue_head

不理解这些标志就无法理解调度循环的任何一步。
