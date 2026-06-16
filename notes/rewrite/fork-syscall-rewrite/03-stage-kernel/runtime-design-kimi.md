# runtime-design-kimi: 03-stage-kernel 运行时文档规划

> **创建**: 2026-06-13
> **前置**: [kboot-new.md](kboot-new.md) — 01~07 覆盖 boot 阶段，07 结束于 `switch_to_user()`
> **范围**: `switch_to_user()` 之后，kernel 的全部运行时机制
> **核心原则**: 严格线性叙事——读者按顺序阅读，无需前后跳跃

---

## 0. 问题诊断：为什么 tmp 文件读起来跳跃

在规划之前，先诊断已有 tmp 文件的问题。tmp 文件是**按子系统切割**的——调度、IPC、系统调用、信号各成一篇。这种切割方式对"已经理解全貌"的作者是自然的，但对"第一次读"的读者是灾难：

| 跳跃类型 | 例子 | 问题 |
|---------|------|------|
| **前向引用** | "`MF_KCALL_RESUME` 标志详见 12-kernel-call" | 读到第10篇时，12还没读 |
| **后向依赖** | "假设读者已理解 RTS 标志" | 但 RTS 分散在多篇中 |
| **概念碎片化** | `proc` 结构体字段分散在 06/08/09/10 | 读完整套才能拼出全貌 |
| **机制与服务混杂** | VMCTL 既是 boot 协议又是运行时调用 | 同一段代码在两个上下文解释 |

**根本原因**：tmp 的规划以"作者整理知识"为视角，而非"读者首次学习"为视角。

### 0.1 本规划的修正策略

1. **先状态、再调度、再通信、再事件、再服务**
   - 状态机（进程长什么样）→ 调度（谁运行）→ IPC（进程间如何说话）→ 中断/异常（外部事件如何介入）→ 系统调用分派（请求如何进入内核）→ 具体服务
   - 这是人类理解操作系统的自然顺序，也是 Minix3 源码的实际依赖顺序

2. **概念首次出现即完整解释**
   - 每个概念在第一次出现时给出完整语义，后续文档只引用不复述
   - 例如：`struct proc` 的全部字段在 08 中一次性讲透，09-21 不再重复解释字段含义

3. **禁止前向引用**
   - 文档 N 中不得出现"详见文档 N+X"
   - 如果某个机制需要前置知识，把它移到前面

4. **一本账原则——但更严格**
   - 每个 C 函数只属于一篇文档
   - 每篇文档覆盖一个"读者独立阅读"的语义单元

---

## 1. 内核运行时的三层结构

从 `switch_to_user()` 开始，内核不再执行"主函数"，而是进入**反应式循环**。所有内核代码的执行路径可归为三层：

```
┌─────────────────────────────────────────┐
│  第一层：事件入口（怎么进内核）           │
│  ─────────────────────────────          │
│  系统调用陷阱 → kernel_call()           │
│  硬件中断     → irq_handle()            │
│  CPU 异常     → exception_handler()     │
└─────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────┐
│  第二层：状态转换（进程怎么动）           │
│  ─────────────────────────────          │
│  调度：pick_proc / enqueue / dequeue    │
│  IPC：mini_send / mini_receive          │
│  信号：cause_sig / sig_delay_done       │
└─────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────┐
│  第三层：基础设施（状态存在哪里）         │
│  ─────────────────────────────          │
│  struct proc（进程状态）                │
│  struct priv（权限状态）                │
│  RTS / MiscFlags（状态编码）            │
│  调度队列 / IPC 队列（状态组织）        │
└─────────────────────────────────────────┘
```

**叙事顺序与上述层次相反**：先讲第三层（基础设施，08），再讲第二层（状态转换，09-11），再讲第一层（事件入口，12），最后讲基于这些入口的具体服务（13-21）。

这是因为：不理解 `struct proc` 和 RTS 标志，就无法理解调度；不理解调度，就无法理解 IPC 阻塞；不理解 IPC，就无法理解系统调用的 VMSUSPEND 机制。

---

## 2. 文档总览与编号

| 编号 | 文件名 | 核心问题 | C 源码核心 | 行数 |
|------|--------|---------|-----------|------|
| 08 | `08-process-state.md` | 进程在内核中如何被描述？ | `proc.h` 全字段 + `priv.h` | ~400 |
| 09 | `09-scheduling.md` | 谁运行、运行多久、怎么切换？ | `proc.c:299-474, 1592-1870` | ~500 |
| 10 | `10-sync-ipc.md` | 进程间如何同步通信？ | `proc.c:479-1196` | ~700 |
| 11 | `11-async-ipc.md` | 异步消息怎么发？ | `proc.c:1200-1550` | ~400 |
| 12 | `12-exception-interrupt.md` | 外部事件如何打断执行？ | `exception.c` + `interrupt.c` + `clock.c:70+` | ~500 |
| 13 | `13-kernel-call-dispatch.md` | 系统调用怎么进内核、怎么分派？ | `system.c:52-167` | ~400 |
| 14 | `14-syscall-process.md` | 进程怎么创建和销毁？ | `do_fork/exec/clear/exit/privctl/runctl/update/statectl` | ~600 |
| 15 | `15-syscall-copy.md` | 跨进程内存怎么拷贝？ | `do_safecopy/umap/vircopy/phys copy/vumap` | ~600 |
| 16 | `16-syscall-vmctl.md` | VM 怎么控制内核？ | `do_vmctl` + `arch_do_vmctl` | ~500 |
| 17 | `17-syscall-signal.md` | 信号怎么产生和投递？ | `do_kill/getksig/endksig/sigsend/sigreturn` + `cause_sig` | ~500 |
| 18 | `18-syscall-device.md` | 设备 I/O 怎么做？ | `do_irqctl/devio/vdevio/sdevio` | ~400 |
| 19 | `19-syscall-clock.md` | 时钟和定时器怎么管理？ | `do_times/setalarm/stime/vtimer` + `clock.c` | ~400 |
| 20 | `20-smp.md` | 多核怎么协同？ | `smp.c` + `apic.c` + `arch_smp.c` | ~500 |
| 21 | `21-syscall-misc.md` | 其他杂项调用是什么？ | `do_abort/getinfo/diagctl/trace/schedctl/schedule/mcontext/sprof/setgrant/safememset/memset/readbios/iopenable` | ~400 |

**编号规则**：08-21 与 kboot 的 01-07 连续，形成 03-stage-kernel 的完整文档序列。

---

## 3. 各文档详细规划

---

### 08-process-state: 进程状态机

> **核心问题**: 一个进程在内核中的完整描述是什么？
> **前置**: 07（boot 完成，进程表已填充，读者已知进程存在）
> **后置**: 09-21 全部依赖本篇——所有字段在此一次性定义
> **C 源码**: `proc.h`（struct proc, RTS, MiscFlags）, `priv.h`（struct priv）, `proc.c:119-160`（proc_init）

#### 叙事策略

08 的目标是：读完这一篇，读者对"进程在内核中是什么"有完整认知，后续文档不再解释字段含义。

#### Ch1: 概念——进程描述的三张表

**进程表 `struct proc`**：按语义分组（不按源码声明顺序）：

| 分组 | 字段 | 一句话语义 |
|------|------|-----------|
| 身份 | `p_nr`, `p_endpoint`, `p_name`, `p_magic` | 这个进程是谁 |
| 寄存器 | `p_reg`（stackframe_s） | 上次切出时 CPU 寄存器的快照 |
| 地址空间 | `p_seg`（segframe_s: cr3, fpu_state, ldt） | 进程的地址空间根在哪里 |
| 调度 | `p_priority`, `p_quantum_size_ms`, `p_cpu_time_left`, `p_cpu`, `p_scheduler` | 调度器怎么对待它 |
| 运行状态 | `p_rts_flags`（16 位） | 为什么它现在不能运行 |
| 运行时标记 | `p_misc_flags`（20+ 位） | 运行时需要处理的临时条件 |
| IPC 链表 | `p_nextready`, `p_caller_q`, `p_q_link`, `p_getfrom_e`, `p_sendto_e` | 它在哪些队列里、等谁、给谁发 |
| 消息缓冲 | `p_sendmsg`, `p_delivermsg`, `p_delivermsg_vir` | 正在发/待投递的消息 |
| VM 挂起 | `p_vmrequest` | 缺页时挂起的请求上下文 |
| 记账 | `p_accounting`, `p_user_time`, `p_sys_time`, `p_cycles` | 用了多少 CPU 时间 |
| 特权 | `p_priv` → `struct priv` | 它能做什么 |
| 定时器 | `p_virt_left`, `p_prof_left` | 用户态/性能分析定时器 |

**RTS 标志位**（进程不可运行的原因）：

```
RTS_SLOT_FREE      — 槽位空闲
RTS_PROC_STOP      — 被停止（boot 初始状态 / 调试）
RTS_SENDING        — 阻塞于 SEND，等对方接收
RTS_RECEIVING      — 阻塞于 RECEIVE，等对方发送
RTS_SIGNALED       — 有信号待处理
RTS_SIG_PENDING    — 信号处理中，暂时不可运行
RTS_P_STOP         — 被 ptrace 停止
RTS_NO_PRIV        — fork 后等待特权分配
RTS_NO_ENDPOINT    — 端点失效，不可 IPC
RTS_VMINHIBIT      — VM 尚未设置页表
RTS_PAGEFAULT      — 有未处理缺页
RTS_VMREQUEST      — 是 VM 内存请求的发起者
RTS_VMREQTARGET    — 是 VM 内存请求的目标
RTS_PREEMPTED      — 被高优先级抢占，待恢复
RTS_NO_QUANTUM     — 时间片耗尽
RTS_BOOTINHIBIT    — boot 阶段等待 VM
```

**核心不变量**：`proc_is_runnable(p)` 为真当且仅当 `p_rts_flags == 0`。

**MiscFlags**（不影响调度，但在 `switch_to_user` 中处理）：
- `MF_DELIVERMSG` — 有待投递消息（由 `delivermsg()` 在恢复用户态前拷贝）
- `MF_KCALL_RESUME` — 内核调用被 VMSUSPEND 中断，需恢复
- `MF_SC_DEFER` / `MF_SC_ACTIVE` / `MF_SC_TRACE` — 系统调用延迟/活跃/跟踪
- `MF_REPLY_PEND` — SENDREC 的回复还没收到
- `MF_MSGFAILED` — 消息投递失败（缺页导致）
- `MF_CONTEXT_SET` — 上下文已设置（sigreturn 后）
- `MF_FLUSH_TLB` — TLB 需刷新（SMP）
- `MF_SENDING_FROM_KERNEL` — 消息来自内核（非用户进程）
- `MF_VIRT_TIMER` / `MF_PROF_TIMER` — 虚拟/性能定时器活跃
- `MF_FPU_INITIALIZED` — FPU 状态已初始化
- `MF_SENDA_VM_MISS` — 异步发送因 VM 修改地址空间失败（SMP）

**特权结构 `struct priv`**：
- `s_proc_nr` — 关联的进程号
- `s_flags` — `PREEMPTIBLE` / `BILLABLE` / `SYS_PROC` / `DYN_PRIV_ID`
- `s_trap_mask` — 允许的 IPC 原语掩码
- `s_ipc_to` — 允许发送的目标位图
- `s_k_call_mask` — 允许的内核调用位图
- `s_notify_pending` / `s_asyn_pending` — 挂起通知/异步消息位图
- `s_sig_mgr` / `s_bak_sig_mgr` — 信号管理器 / 备份信号管理器
- `s_alarm_timer` — 同步闹钟定时器
- `s_grant_table` / `s_grant_entries` — grant 表（safecopy 用）
- `s_io_ranges[]`, `s_mem_ranges[]`, `s_irq_hooks[]` — I/O 端口、内存、IRQ 权限

**特权表组织**：
- `priv[NR_SYS_PROCS]` — 全局数组，静态 ID + 动态 ID
- `ppriv_addr[id]` — 快速索引，O(1) 由 ID 查指针
- 系统进程：每个独立 `struct priv`
- 用户进程：全部共享 `USER_PRIV_ID` 指向的同一个 `struct priv`

#### Ch2: C 源码分析

- `proc.h:1-290` — struct proc + RTS/MiscFlags 宏
- `priv.h:1-105` — struct priv + 权限检查宏
- `proc.c:119-160` — `proc_init()`：进程表初始化

#### Ch3: Rust 设计决策

| C 机制 | Rust 演进 | 理由 |
|--------|----------|------|
| `struct proc` | `KProcess`，字段分组为子结构体 | 类型安全，语义清晰 |
| RTS 位标志 | `RtsFlags` bitflags | Rust 惯用法 |
| MiscFlags 位标志 | `MiscFlags` bitflags | 同上 |
| `struct priv` | `KPriv` + `PrivTable` | 独立管理权限生命周期 |
| `priv[]` 全局数组 | `PrivTable` 结构体封装 | 避免裸数组越界 |
| `p_vmrequest` | `VmSuspendContext` enum | 类型安全的挂起状态机 |
| 64 位适配 | `p_cr3: u64`，FPU → XSAVE | 硬件演进 |

#### Ch4: 实现要点

- `KProcess` 的内存布局（考虑 Cache line 对齐）
- `RtsFlags` 的 `is_runnable()` 方法
- `PrivTable::get_or_allocate_dynamic()` 动态分配

---

### 09-scheduling: 调度循环

> **核心问题**: 内核如何选择下一个运行的进程？
> **前置**: 08（已理解 struct proc、RTS 标志、调度字段、队列指针）
> **后置**: 10（同步 IPC 需要理解调度队列操作）
> **C 源码**: `proc.c:299-474`（switch_to_user）, `proc.c:1592-1870`（enqueue/dequeue/pick_proc）

#### 叙事策略

09 假设读者已理解 08 的全部字段。不再解释 `p_rts_flags` 或 `p_nextready` 的含义，只讲它们如何被调度器使用。

#### Ch1: 概念——调度不是"选一个进程"，而是一个状态机

**`switch_to_user()` 的主循环**（内核的"心脏"）：

```
entry:
  p = current_proc
  if p 仍然可运行:
    goto check_misc_flags

not_runnable_pick_new:
  if p 被抢占过:
    清除 RTS_PREEMPTED
    if p 仍可运行:
      根据时间片决定 enqueue 还是 enqueue_head

  while 没有可运行进程:
    idle()  // 停机等中断

  p = pick_proc()        // 选最高优先级就绪进程
  current_proc = p
  switch_address_space(p)  // 切 CR3

check_misc_flags:
  while p 有 misc flags 待处理:
    if MF_KCALL_RESUME:   kernel_call_resume(p)
    if MF_DELIVERMSG:     delivermsg(p)
    if MF_SC_DEFER:       arch_do_syscall(p)
    if MF_SC_TRACE:       触发 SIGTRAP
    ...

  restore_user_context(p)  // iretq
```

**关键洞察**：`switch_to_user` 不是简单的"选进程→切上下文"。它是一个**检查点循环**：每次进入都检查当前进程是否还该继续运行，如果不是就选新的，然后处理所有待办杂项，最后恢复用户态。

**多级优先级队列**：
- `NR_SCHED_QUEUES = 16` 个队列，0 最高，15 最低
- 每个 CPU 独立队列（`cpulocals.h` 中定义）
- `pick_proc()`：从 0 开始扫描，返回第一个非空队列的队首
- `enqueue(p)`：加到队尾，若优先级高于当前进程则抢占
- `enqueue_head(p)`：被抢占的进程放回队首，保证下次优先
- `dequeue(p)`：从所在队列移除

**时间片管理**：
- `p_quantum_size_ms`：时间片大小（毫秒）
- `p_cpu_time_left`：剩余时间
- 时钟中断减少 `p_cpu_time_left`，归零时设置 `RTS_NO_QUANTUM`
- 用户空间调度器管理的进程：通知调度器分配新时间片
- 内核调度或不可抢占进程：直接重置时间片继续

#### Ch2: C 源码分析

- `cpulocals.h:37-75` — CPU 局部变量定义
- `proc.c:176` — `idle()`：停机等待中断
- `proc.c:299-474` — `switch_to_user()` 完整逻辑
- `proc.c:1595-1668` — `enqueue()` / `enqueue_head()`
- `proc.c:1716-1783` — `dequeue()`
- `proc.c:1785-1816` — `pick_proc()`
- `proc.c:1860-1891` — `notify_scheduler()`：时间片耗尽通知
- `proc.c:1893-1910` — `proc_no_time()`：时间片耗尽处理

#### Ch3: Rust 设计决策

| C 机制 | Rust 演进 | 理由 |
|--------|----------|------|
| 全局 `run_q_head[]` | `PerCpu::run_queues` | SMP 下每个 CPU 独立 |
| `pick_proc()` 扫描 | 维护 `highest_non_empty_queue` 索引 | O(1) 替代 O(NR_SCHED_QUEUES) |
| `enqueue()` 抢占检查 | `enqueue()` 返回 `PreemptAction` | 显式表达抢占决策 |
| `switch_to_user` | `Scheduler::switch_to_user()` 方法 | 封装状态机 |
| BKL（SMP） | `BigKernelLock` spinlock | 临界区禁止睡眠 |

#### Ch4: 实现要点

- `Scheduler` 结构体持有 `PerCpu` 引用
- `pick_proc` 的 O(1) 优化（维护优先级位图）
- 抢占的边界条件（当前进程不可抢占时如何处理）

---

### 10-sync-ipc: 同步 IPC

> **核心问题**: 进程间如何同步地发送和接收消息？
> **前置**: 08（进程字段）+ 09（调度队列操作、阻塞与唤醒）
> **后置**: 11（异步 IPC 是同步 IPC 的补充）, 13（系统调用分派需要理解 VMSUSPEND）
> **C 源码**: `proc.c:479-1196`（do_ipc, do_sync_ipc, mini_send/receive/notify）

#### 叙事策略

10 假设读者已理解：进程为什么阻塞（RTS_SENDING/RTS_RECEIVING）、阻塞后如何被唤醒（dequeue + enqueue）、以及 `switch_to_user` 中的 `check_misc_flags`。只讲 IPC 的协议逻辑。

#### Ch1: 概念——握手式消息传递

**四个原语**：

| 原语 | 语义 | 阻塞条件 |
|------|------|---------|
| SEND | 发送消息 | 目标未在 RECEIVE |
| RECEIVE | 接收消息 | 无匹配消息可用 |
| SENDREC | 先 SEND 再 RECEIVE | SEND 阻塞或 RECEIVE 阻塞 |
| NOTIFY | 发送轻量通知 | **永不阻塞** |
| SENDNB | 非阻塞发送 | 目标未就绪则返回 `ENOTREADY` |

**消息投递的延迟拷贝设计**：
- IPC 调用时，消息**不直接**写入接收方用户空间
- 而是存入内核缓冲区 `p_delivermsg`，设置 `MF_DELIVERMSG`
- `switch_to_user` 在恢复进程前调用 `delivermsg()` 完成实际拷贝
- 好处：简化 IPC 代码路径，统一处理缺页（VMSUSPEND）

**阻塞与队列**：
- SEND 阻塞：设置 `RTS_SENDING`，加入目标的 `p_caller_q`
- RECEIVE 阻塞：设置 `RTS_RECEIVING`，记录 `p_getfrom_e`
- 唤醒：对方完成匹配后，清除 RTS 标志，调用 `enqueue()`

**NOTIFY 的待处理位图**：
- 目标未在 RECEIVE 时，通知不丢失
- 存入发送方特权结构的 `s_notify_pending` 位图
- 目标下次 RECEIVE 时，`has_pending_notify()` 发现位图非空，立即投递

**死锁检测**：
- `deadlock()` 在 SEND/RECEIVE 阻塞前检查
- 跟踪 `p_caller_q` 链，发现环则返回 `ELOCKED`

**RECEIVE 的消息来源优先级**：
1. 待处理通知（`s_notify_pending`）
2. 待处理异步消息（`s_asyn_pending`，详见 11）
3. 同步发送者（`p_caller_q` 中的进程）
4. 若指定 `ANY`，按上述顺序；若指定具体 endpoint，只匹配该进程

#### Ch2: C 源码分析

- `ipcconst.h` — IPC 调用号、状态码
- `proc.c:479-598` — `do_sync_ipc()`：原语分派
- `proc.c:599-698` — `do_ipc()`：IPC 入口（含 SENDA）
- `proc.c:703-771` — `deadlock()`：死锁环检测
- `proc.c:773-841` — `has_pending*()`：待处理消息检查
- `proc.c:870-965` — `mini_send()`：阻塞发送
- `proc.c:967-1120` — `mini_receive()`：阻塞接收
- `proc.c:1122-1196` — `mini_notify()`：非阻塞通知
- `proc.c:263-297` — `delivermsg()`：消息拷贝到用户空间

#### Ch3: Rust 设计决策

| C 机制 | Rust 演进 | 理由 |
|--------|----------|------|
| `p_caller_q` 链表 | `SenderQueue` 封装 | 类型安全，避免手动链表操作 |
| `deadlock()` | `detect_deadlock()` 返回 `Option<Cycle>` | 显式表达 |
| `mini_send/receive` | `IpcEngine::send/receive` | 封装 IPC 状态机 |
| 消息拷贝 | `copy_msg_to_user()` 返回 `Result<(), PageFault>` | 统一错误处理 |

---

### 11-async-ipc: 异步 IPC

> **核心问题**: 发送方不想阻塞时，怎么发消息？
> **前置**: 10（已理解同步 IPC 的阻塞语义、RECEIVE 的消息来源优先级、延迟投递）
> **后置**: 无（异步 IPC 是 IPC 的终点）
> **C 源码**: `proc.c:1200-1550`（try_deliver_senda, mini_senda, try_async, try_one, cancel_async）

#### 叙事策略

11 假设读者已理解同步 IPC 的全部机制。异步 IPC 是同步 IPC 的"变体"——同样的队列、同样的延迟投递、同样的权限检查，只是发送方不阻塞。差异点重点讲，相同点不再重复。

#### Ch1: 概念——异步消息表

**`asynmsg_t` 表**：
- 发送方在用户空间维护的数组
- 每个条目：`flags`, `dst`, `result`, `msg`
- 发送方填充后调用 `SENDA`，内核扫描表尝试投递
- 内核设置 `AMF_DONE` 和 `result`，发送方轮询

**投递策略**：
- 扫描表中所有 `AMF_VALID` 条目
- 对每个条目尝试 `mini_notify` 或 `mini_send`（取决于标志）
- 若目标未就绪，设置 `s_asyn_pending` 位图（同 NOTIFY 机制）
- 目标 RECEIVE 时，`try_async()` 重新扫描发送方的表

**AMF 标志**：
- `AMF_VALID` — 条目有效
- `AMF_DONE` — 内核已处理
- `AMF_NOTIFY` — 处理完成后发通知
- `AMF_NOREPLY` — 不匹配 SENDREC 的接收部分
- `AMF_NOTIFY_ERR` — 仅失败时通知

**限制**：
- 仅系统进程可用（`s_flags & SYS_PROC`）
- 表大小上限 `16 * (NR_TASKS + NR_PROCS)`
- 目标不能是内核任务
- SMP 下若发送方地址空间被 VM 修改，跳过并设 `MF_SENDA_VM_MISS`

**ASYNCM 伪进程**：
- endpoint = -5，专门用于异步完成通知
- 当 `AMF_NOTIFY` 或 `AMF_NOTIFY_ERR` 触发时，内核向 ASYNCM 发 NOTIFY

#### Ch2: C 源码分析

- `minix/include/minix/ipc.h:2745-2762` — `asynmsg_t` 和 AMF 标志
- `proc.c:1200-1329` — `try_deliver_senda()`
- `proc.c:1331-1346` — `mini_senda()`
- `proc.c:1348-1388` — `try_async()`
- `proc.c:1390-1508` — `try_one()`
- `proc.c:1510-1550` — `cancel_async()`

#### Ch3: Rust 设计决策

- `AsyncMsgTable` 封装用户空间表的扫描
- `AsynFlags` bitflags
- 扫描实现为迭代器，避免索引越界

---

### 12-exception-interrupt: 异常与中断

> **核心问题**: 硬件事件如何打断进程执行、进入内核？
> **前置**: 08（进程状态）+ 09（调度循环）+ 10（IPC 阻塞/唤醒）
> **后置**: 13（系统调用分派——异常是进入内核的三条路之一）
> **C 源码**: `exception.c` + `interrupt.c` + `clock.c:70+` + `arch/i386/apic.c`（I/O APIC 部分）

#### 叙事策略

12 讲"外部事件如何介入"。此时读者已理解：进程在什么状态下运行、调度器如何选择进程、IPC 如何阻塞和唤醒。现在引入第三种力量：硬件。

#### Ch1: 概念——三条进入内核的路

| 入口 | 触发条件 | 处理函数 | 特点 |
|------|---------|---------|------|
| **系统调用** | `int SYS386` / `syscall` 指令 | `kernel_call()` | 同步，用户主动请求 |
| **硬件中断** | IRQ 线信号 | `irq_handle()` | 异步，外部设备触发 |
| **CPU 异常** | 除零/缺页/GPF 等 | `exception_handler()` | 同步，执行指令出错 |

**中断处理流程**：
```
IRQ 到达 → 汇编入口保存上下文 → `irq_handle(irq)`
  → 查 `irq_hooks[]` 找到注册进程
  → `mini_notify(irq_proc, HARDWARE)` 发送硬件通知
  → 若该进程优先级更高，可能抢占当前进程
  → `switch_to_user()` 重新调度
```

**时钟中断（特殊的中断）**：
- `timer_int_handler()` 减少当前进程 `p_cpu_time_left`
- 检查虚拟/性能定时器
- 若时间片耗尽，设置 `RTS_NO_QUANTUM`
- 调用 `sched_clock()` 进行调度决策
- 重新计算闹钟队列

**异常处理**：
- 除零 → `SIGFPE`
- 缺页 → `pagefault()` → 可能 VMSUSPEND（等 VM）或 SIGSEGV
- 通用保护故障(GPF) → `SIGBUS`
- 段错误 → `SIGSEGV`

**IRQ Hook 机制**：
- 系统进程通过 `SYS_IRQCTL` 注册 IRQ hook
- `irq_hooks[NR_IRQ_HOOKS]` 全局数组
- 每个 hook 记录：进程号、IRQ 号、策略（重新启用/禁用）
- 中断发生时，遍历 hook 找到匹配的进程

#### Ch2: C 源码分析

- `exception.c` — `exception_handler()` + `pagefault()`
- `interrupt.c` — `irq_handle()` + `put_irq_handler()`
- `clock.c:70+` — `timer_int_handler()` + `clock_stop()`
- `arch/i386/apic.c` — I/O APIC 路由（若 SMP）

#### Ch3: Rust 设计决策

| C 机制 | Rust 演进 | 理由 |
|--------|----------|------|
| `irq_hooks[]` | `IrqTable` 结构体 | 封装注册/注销/查询 |
| 异常分发 | `ExceptionVector` enum | 类型安全 |
| pagefault | `PageFaultHandler::handle()` | 封装 VMSUSPEND 决策 |
| 中断上下文 | `unsafe` + `no_std` 约束 | 中断处理不能分配内存 |

---

### 13-kernel-call-dispatch: 系统调用分派

> **核心问题**: 用户态请求怎么进入内核、怎么找到处理函数？
> **前置**: 08（特权、s_k_call_mask）+ 10（IPC 消息格式）+ 12（异常/中断入口）
> **后置**: 14-21（所有具体系统调用文档都依赖本篇的分派机制）
> **C 源码**: `system.c:52-167`（kernel_call, kernel_call_dispatch, kernel_call_finish, map 宏）

#### 叙事策略

13 是"运行时服务"的起点。读者至此已理解：进程状态、调度、IPC、硬件事件。现在问：用户态服务（PM/VM/VFS）如何请求内核执行特权操作？答案是 kernel call。

#### Ch1: 概念——从消息到函数的完整路径

**系统调用不是直接函数调用，而是 IPC 消息**：
- 用户态服务发送消息给 SYSTEM 任务（endpoint = 0）
- 消息类型 = `SYS_XXX`，附带参数
- 内核通过 `kernel_call()` → `kernel_call_dispatch()` → `do_xxx()` 处理

**调用向量表 `call_vec[]`**：
- `system_init()` 中通过 `map(SYS_XXX, do_xxx)` 填充
- `kernel_call_dispatch()` 查表分派
- 权限检查：`s_k_call_mask` 对应位必须置位

**VMSUSPEND 机制**（核心难点）：
- 某些调用（如 `sys_vircopy`）可能访问未映射的页
- `do_xxx()` 返回 `VMSUSPEND`（-996）
- `kernel_call_finish()` 保存请求到 `p_vmrequest.saved.reqmsg`
- 设置 `MF_KCALL_RESUME`
- 进程被挂起，VM 处理缺页
- VM 完成后，内核在 `switch_to_user` 的 `check_misc_flags` 中调用 `kernel_call_resume()`
- 恢复执行 `do_xxx()`，此时页已映射

**EDONTREPLY 语义**：
- 处理函数返回 `EDONTREPLY` 时，不写回返回值
- 调用方不会收到回复消息
- 用于 `do_exit()` 等不需要回复的场景

**消息拷贝的两次跨越**：
1. `kernel_call()`：从用户空间拷贝消息到内核 `m_in`
2. `kernel_call_finish()`：将结果从 `m_in` 拷贝回用户空间

#### Ch2: C 源码分析

- `system.c:52` — `call_vec[]` 定义
- `system.c:54` — `map()` 宏
- `system.c:59-93` — `kernel_call_finish()`
- `system.c:95-134` — `kernel_call_dispatch()`
- `system.c:136-167` — `kernel_call()`
- `system.c:168-317` — `system_init()`：46 个调用号的注册

#### Ch3: Rust 设计决策

| C 机制 | Rust 演进 | 理由 |
|--------|----------|------|
| `call_vec[]` | `CallVector` 结构体，内部 `&'static [HandlerFn]` | 类型安全分派 |
| `map()` 宏 | `CallVector::register()` | 编译时检查调用号 |
| VMSUSPEND | `KernelCallStateMachine` enum | `Running / Suspended { req }` |
| `do_xxx` 函数指针 | `trait KernelCall` + 动态分发 | 或保留函数指针数组（简单） |

---

### 14-syscall-process: 进程管理调用

> **核心问题**: 进程怎么被创建、替换和销毁？
> **前置**: 08（struct proc / priv）+ 13（kernel_call 分派机制）
> **后置**: 15（跨进程拷贝常伴随 fork/exec）, 17（信号与进程退出相关）
> **C 源码**: `system/do_fork.c`, `do_exec.c`, `do_clear.c`, `do_exit.c`, `do_privctl.c`, `do_runctl.c`, `do_update.c`, `do_statectl.c`

#### Ch1: 概念——进程生命周期

**`SYS_FORK`**：
- 分配新 `struct proc` 槽位
- 复制父进程的寄存器、段、特权结构
- 新进程 `p_reg.retreg = 0`（子进程返回值），父进程 `retreg = child_pid`
- 继承父进程的 `s_ipc_to` 和 `s_k_call_mask`
- 设置 `RTS_NO_PRIV`（等待 `privctl` 确认）
- 加入调度队列

**`SYS_EXEC`**：
- 不创建新进程，替换当前进程的映像
- 更新 `p_reg.pc` 为新入口点
- 重置栈指针
- 通知 VM 释放旧地址空间、建立新地址空间
- 清除 `MF_FPU_INITIALIZED`

**`SYS_EXIT`**：
- 系统进程请求退出
- 向自身发送 `SIGABRT`
- 返回 `EDONTREPLY`

**`SYS_CLEAR`**：
- PM 通知内核清理已退出进程
- 释放地址空间（通知 VM）
- 取消 IRQ hook
- 清除端点（`RTS_NO_ENDPOINT`）
- 释放动态特权结构
- 释放进程表槽位（`RTS_SLOT_FREE`）

**`SYS_PRIVCTL`**：
- 设置/修改进程特权
- 分配动态特权 ID
- 更新 `s_k_call_mask`, `s_ipc_to`
- 添加 I/O 端口/内存/IRQ 权限
- 清除 `RTS_NO_PRIV`

#### Ch2: C 源码分析

逐文件分析 do_fork/exec/clear/exit/privctl/runctl/update/statectl。

#### Ch3: Rust 设计决策

- `ProcessLifecycle` trait 封装 fork/exec/exit
- `PrivCtlOp` enum 表示 privctl 的子操作

---

### 15-syscall-copy: 跨进程拷贝

> **核心问题**: 内核怎么在不同进程地址空间之间搬运数据？
> **前置**: 08（struct proc, 地址空间）+ 13（VMSUSPEND 机制）
> **后置**: 14（fork 需要拷贝）, 16（VMCTL 与地址空间管理相关）
> **C 源码**: `system/do_safecopy.c`, `do_vcopyf.c`, `do_umap.c`, `do_vumap.c`, `do_copy.c`

#### Ch1: 概念——五种拷贝语义

| 调用 | 地址空间 | 权限检查 | 典型用途 |
|------|---------|---------|---------|
| `SYS_VIRCOPY` | 虚拟地址 → 虚拟地址 | `s_k_call_mask` | 进程间消息传递 |
| `SYS_PHYSCOPY` | 物理地址 → 物理地址 | `s_k_call_mask` | 早期 boot / 驱动 DMA |
| `SYS_SAFECOPYFROM` |  granted 权限 → 本地 | grant 验证 | 驱动访问用户缓冲区 |
| `SYS_SAFECOPYTO` | 本地 → granted 权限 | grant 验证 | 驱动写用户缓冲区 |
| `SYS_VSAFECOPY` | 向量化的 safecopy | grant 验证 | 批量 I/O |

**`SYS_UMAP`**：虚拟地址 → 物理地址转换（用于验证用户指针）
**`SYS_UMAP_REMOTE`**：为其他进程做 umap
**`SYS_VUMAP`**：向量化的 umap

**Grant 机制**：
- 进程通过 `SYS_SETGRANT` 注册 grant 表
- 每个 grant 记录：远程进程可访问的本地内存范围
- `safecopy` 时验证 grant 的权限、范围、有效期

**VMSUSPEND 与拷贝**：
- 拷贝过程中可能触及未映射页
- `do_vircopy()` 返回 `VMSUSPEND`
- VM 映射页后恢复拷贝

#### Ch2: C 源码分析

- `do_safecopy.c` — `do_safecopy_from/to()`
- `do_vcopyf.c` — `do_vircopy()`, `virtual_copy_f()`
- `do_umap.c` — `do_umap()`, `do_umap_remote()`
- `do_vumap.c` — `do_vumap()`
- `do_copy.c` — `do_phys_copy()`

#### Ch3: Rust 设计决策

| C 机制 | Rust 演进 | 理由 |
|--------|----------|------|
| `phys_copy` | `kernel_phys_to_virt()` + `copy_nonoverlapping` | Direct Map 消除物理拷贝 |
| `vircopy` | `copy_to_user()` / `copy_from_user()` | 与 Linux 语义对齐 |
| grant 验证 | `GrantTable::verify()` | 封装验证逻辑 |
| VMSUSPEND | `CopyOperation::resume()` | 状态机保存进度 |

---

### 16-syscall-vmctl: VM 控制调用

> **核心问题**: VM 如何通过系统调用控制内核的地址空间？
> **前置**: 08（p_seg.cr3, RTS_VMINHIBIT）+ 13（kernel_call 分派）+ 15（umap 与地址翻译）
> **后置**: 无（VMCTL 是内存管理的终点）
> **C 源码**: `system/do_vmctl.c`, `arch/i386/do_vmctl.c` / `arch/x86_64/do_vmctl.c`

#### Ch1: 概念——VM 是内核地址空间的管理员

**VMCTL 子命令**：

| 子命令 | 功能 | 调用时机 |
|--------|------|---------|
| `VMCTL_SETADDRSPACE` | 内核切换 CR3 到 VM 建立的页表 | VM 初始化完成后 |
| `VMCTL_GET_PDBR` | 获取某进程的 CR3 物理地址 | VM 管理页表时 |
| `VMCTL_MEMREQ` | 注册/查询内存请求 | 缺页处理 |
| `VMCTL_VMINHIBIT_SET/CLEAR` | 设置/清除 RTS_VMINHIBIT | VM 创建/销毁页表 |
| `VMCTL_CLEAR_PAGEFAULT` | 清除 RTS_PAGEFAULT | VM 处理完缺页 |
| `VMCTL_KERN_PHYSMAP` | 内核声明需要映射的物理区域 | Direct Map 建立 |
| `VMCTL_KERN_MAP_REPLY` | VM 回复映射的虚拟地址 | Direct Map 建立 |
| `VMCTL_ENABLE_PAGING` | 启用分页（boot 阶段）| 01-02 已覆盖 |

**VM 启动时的地址空间协商**（07 的延续）：
1. VM 运行 → `init_page_table()` → `map_kernel()`
2. VM → `VMCTL_SETADDRSPACE`：内核切 CR3
3. VM → `VMCTL_KERN_PHYSMAP`：内核声明物理区域
4. VM → `VMCTL_KERN_MAP_REPLY`：VM 回复虚拟地址
5. VM → `VMCTL_VMINHIBIT_CLEAR`：解除 PM/VFS/RS 的 VMINHIBIT

**MEMREQ 协议**：
- 进程缺页 → 内核设置 `RTS_PAGEFAULT`
- 内核向 VM 发送 `VMCTL_MEMREQ`
- VM 处理（分配页 / 建立映射 / 发送 SIGSEGV）
- VM → `VMCTL_CLEAR_PAGEFAULT`

#### Ch2: C 源码分析

- `system/do_vmctl.c` — 架构无关的 VMCTL 处理
- `arch/i386/do_vmctl.c` — x86 特定的 CR3/分页操作

#### Ch3: Rust 设计决策

- `VmCtlOp` enum 包含所有子命令
- `AddressSpaceManager` trait 封装 CR3 切换
- Direct Map 下 `VMCTL_KERN_PHYSMAP` 退化（kernel 自己知道映射）

---

### 17-syscall-signal: 信号系统

> **核心问题**: 异步信号怎么从产生到投递？
> **前置**: 08（RTS_SIGNALED / RTS_SIG_PENDING）+ 10（NOTIFY 机制）+ 13（kernel_call 分派）
> **后置**: 14（exit 通过 SIGABRT 实现）
> **C 源码**: `system/do_kill.c`, `do_getksig.c`, `do_endksig.c`, `do_sigsend.c`, `do_sigreturn.c` + `system.c:389-454`（cause_sig, sig_delay_done）

#### Ch1: 概念——内核负责传递，PM 负责策略

**信号生命周期**：
```
产生（do_kill / cause_sig）
  → 标记 RTS_SIGNALED + p_pending 位图
  → NOTIFY 信号管理器（s_sig_mgr）
  → 信号管理器（PM）获取信号（SYS_GETKSIG）
  → PM 决定处理策略
  → PM 请求内核推送信号帧（SYS_SIGSEND）
  → 内核在目标用户栈构建 sigframe
  → 目标进程执行信号处理函数
  → 信号处理返回（SYS_SIGRETURN）
  → 恢复原始上下文
  → PM 通知内核结束（SYS_ENDKSIG）
  → 清除 RTS_SIG_PENDING
```

**信号管理器**：
- 每个进程有 `s_sig_mgr`（通常是 PM）
- 信号产生时，内核 NOTIFY 信号管理器
- 信号管理器决定：终止、忽略、捕获
- 若进程是自己的信号管理器且收到致命信号 → 转发给 `s_bak_sig_mgr`

**信号帧 `sigframe`**：
- 推送到目标进程用户栈
- 包含：原始寄存器上下文、信号编号、信号处理函数地址
- `SC_MAGIC` 验证防止损坏

**VMSUSPEND 与信号**：
- `do_sigsend` 拷贝 sigframe 到用户栈可能缺页
- 返回 VMSUSPEND，VM 映射页后恢复

#### Ch2: C 源码分析

- `system.c:389-453` — `cause_sig()`, `sig_delay_done()`
- `do_kill.c` — `do_kill()`
- `do_getksig.c` — `do_getksig()`
- `do_endksig.c` — `do_endksig()`
- `do_sigsend.c` — `do_sigsend()`
- `do_sigreturn.c` — `do_sigreturn()`

---

### 18-syscall-device: 设备 I/O 调用

> **核心问题**: 驱动怎么请求中断和访问 I/O 端口？
> **前置**: 12（IRQ hook 机制）+ 13（kernel_call 分派）
> **后置**: 19（时钟设备与定时器相关）
> **C 源码**: `system/do_irqctl.c`, `do_devio.c`, `do_vdevio.c`, `do_sdevio.c`

#### Ch1: 概念——驱动委托内核操作硬件

**`SYS_IRQCTL`**：
- `IRQ_SETPOLICY` — 注册 IRQ hook
- `IRQ_RMPOLICY` — 注销 IRQ hook
- `IRQ_ENABLE` / `IRQ_DISABLE` — 使能/屏蔽 IRQ
- `IRQ_ACK` — 中断确认（EOI）

**`SYS_DEVIO`**：x86 的 `inb/inw/inl/outb/outw/outl`
**`SYS_VDEVIO`**：向量化的 DEVIO（批量端口操作）
**`SYS_SDEVIO`**：字符串 I/O（`insb/insw/outsb/outsw`）

**权限检查**：
- 进程的 `s_io_ranges[]` 必须包含请求的端口
- 只有特定系统进程可以注册 IRQ

#### Ch2: C 源码分析

逐文件分析 do_irqctl/devio/vdevio/sdevio。

---

### 19-syscall-clock: 时钟服务调用

> **核心问题**: 进程怎么获取时间、设置闹钟？
> **前置**: 12（时钟中断处理）+ 13（kernel_call 分派）
> **后置**: 无
> **C 源码**: `system/do_times.c`, `do_setalarm.c`, `do_stime.c`, `do_vtimer.c` + `clock.c`

#### Ch1: 概念——三层时间

| 时间类型 | 获取方式 | 用途 |
|---------|---------|------|
| 实时时间 | `SYS_STIME` / `SYS_SETTIME` | 墙上时钟 |
| 进程时间 | `SYS_TIMES` | user/sys 时间统计 |
| 闹钟 | `SYS_SETALARM` | 一次性异步通知 |
| 虚拟定时器 | `SYS_VTIMER` | 用户态/性能分析定时器 |

**闹钟机制**：
- 每个 `struct priv` 有 `s_alarm_timer`
- 内核维护定时器队列（`tmrs_settimer`）
- 时钟中断检查队列，到期的发送 NOTIFY

**虚拟定时器**：
- `p_virt_left` — 用户态运行时间限制
- `p_prof_left` — 性能分析采样间隔
- 时钟中断递减，归零时发送 `SIGVTALRM` / `SIGPROF`

#### Ch2: C 源码分析

- `clock.c` — 定时器队列管理
- `do_times.c`, `do_setalarm.c`, `do_stime.c`, `do_vtimer.c`

---

### 20-smp: SMP 多处理器支持

> **核心问题**: 多核时内核怎么协同？
> **前置**: 08（CPU 局部变量）+ 09（调度队列）+ 12（APIC/中断）
> **后置**: 无（SMP 是横切机制的终点）
> **C 源码**: `smp.c`, `arch/i386/apic.c`, `arch/i386/arch_smp.c`, `cpulocals.h`

#### Ch1: 概念——大内核锁模型

**BKL（Big Kernel Lock）**：
- 全局自旋锁
- 任何 CPU 进入内核必须先获取 BKL
- 同一时刻只有一个 CPU 执行内核代码
- 从内核入口到 `switch_to_user()` 释放

**CPU 局部变量**：
- `__cpu_local_vars[NR_CPUS]`
- 包含：`proc_ptr`, `bill_ptr`, `run_q_head[]`, `fpu_owner`
- `get_cpulocal_var(name)` → `__cpu_local_vars[cpuid].name`

**AP（Application Processor）启动**：
1. BSP 解析 ACPI MADT，发现 AP
2. BSP 发送 INIT IPI + SIPI
3. AP 从实模式启动 → 保护模式 → 跳转到内核
4. AP 获取 BKL，进入 `switch_to_user()`

**IPI（Inter-Processor Interrupt）**：
- `smp_schedule()` — 请求目标 CPU 调度
- `smp_schedule_stop_proc()` — 跨 CPU 停止进程
- `smp_schedule_vminhibit()` — 跨 CPU 设置 VMINHIBIT
- `smp_schedule_migrate_proc()` — 跨 CPU 迁移进程

**sched_ipi_data**：
- 每个 CPU 一个
- 包含 `flags`（操作类型）和 `data`（目标进程）
- 发送 CPU 设置目标 CPU 的 `sched_ipi_data`，发 IPI

#### Ch2: C 源码分析

- `smp.c` — SMP 核心逻辑
- `apic.c` — Local APIC + I/O APIC
- `arch_smp.c` — AP 启动
- `cpulocals.h` — CPU 局部变量定义

#### Ch3: Rust 设计决策

| C 机制 | Rust 演进 | 理由 |
|--------|----------|------|
| BKL | `BigKernelLock` struct | 封装自旋锁，禁止睡眠检查 |
| CPU 局部变量 | `CpuLocal<T>` 类型 | 编译时检查访问模式 |
| IPI | `IpiMessage` enum | 类型安全的跨 CPU 通信 |
| AP 启动 | `ApTrampoline` 结构体 | 封装启动状态机 |

---

### 21-syscall-misc: 杂项调用

> **核心问题**: 那些"不好归类"的系统调用是做什么的？
> **前置**: 13（kernel_call 分派）
> **后置**: 无（本篇是运行时文档的终点）
> **C 源码**: `system/do_abort.c`, `do_getinfo.c`, `do_diagctl.c`, `do_trace.c`, `do_schedctl.c`, `do_schedule.c`, `do_setmcontext.c`, `do_getmcontext.c`, `do_sprofile.c`, `do_setgrant.c`, `do_safememset.c`, `do_memset.c`, `do_readbios.c`, `do_iopenable.c`, `do_statectl.c`

#### Ch1: 概念——杂项但不简单

| 调用 | 功能 | 关键设计点 |
|------|------|-----------|
| `SYS_ABORT` | 内核panic/重启 | 紧急终止，不写回 |
| `SYS_GETINFO` | 获取内核信息 | `kinfo` 结构拷贝到用户空间 |
| `SYS_DIAGCTL` | 诊断控制 | 内核日志级别等 |
| `SYS_TRACE` | ptrace 支持 | 设置/清除 MF_SC_TRACE |
| `SYS_SCHEDCTL` | 改变进程调度器 | 设置 `p_scheduler` |
| `SYS_SCHEDULE` | 重新调度进程 | 更新优先级/时间片 |
| `SYS_SETMCONTEXT` / `GETMCONTEXT` | 设置/获取机器上下文 | 用于信号/longjmp |
| `SYS_SPROF` | 性能分析 | 启动/停止统计采样 |
| `SYS_SETGRANT` | 设置 grant 表 | 用于 safecopy |
| `SYS_SAFEMEMSET` / `MEMSET` | 安全/普通内存填充 | 跨进程 memset |
| `SYS_READBIOS` | 读取 BIOS | x86 特定 |
| `SYS_IOPENABLE` | 启用 I/O 权限 | 设置 EFLAGS.IOPL |
| `SYS_STATECTL` | 进程状态控制 | 用户空间控制自身状态 |

**`SYS_GETINFO` 的信息类型**：
- `GET_KINFO` — 内核启动信息
- `GET_IMAGE` — boot image
- `GET_IRQHOOK` — IRQ hook 表
- `GET_PROCTAB` — 进程表（只读快照）
- `GET_PRIVTAB` — 特权表
- `GET_KMESSAGES` — 内核消息缓冲
- `GET_WHOAMI` — 当前进程的身份信息

#### Ch2: C 源码分析

逐文件简要分析。

---

## 4. 叙事线验证：是否还需要前后跳跃？

逐篇检查是否存在"需后向回顾"或"需前向预习"的依赖：

| 文档 | 首次引入概念 | 依赖前面文档的概念 | 是否引入未来概念？ |
|------|-------------|-------------------|-----------------|
| 08 | struct proc, RTS, MiscFlags, struct priv, VmSuspend | 07 的 boot proc | ❌ 无 |
| 09 | 调度队列, switch_to_user, pick_proc, quantum | 08 全部 | ❌ 无 |
| 10 | SEND/RECEIVE/NOTIFY, caller_q, delivermsg, deadlock | 08+09 | ❌ 无 |
| 11 | asynmsg_t, AMF, SENDA | 08+10 | ❌ 无 |
| 12 | exception, irq_handle, timer_int, IRQ hook | 08+09+10 | ❌ 无 |
| 13 | kernel_call, call_vec, VMSUSPEND, EDONTREPLY | 08+10+12 | ❌ 无 |
| 14 | fork/exec/exit/clear/privctl | 08+13 | ❌ 无 |
| 15 | vircopy/physcopy/safecopy/umap, grant | 08+13 | ❌ 无 |
| 16 | VMCTL 子命令, MEMREQ, 地址空间协商 | 08+13+15 | ❌ 无 |
| 17 | 信号生命周期, sigframe, signal manager | 08+10+13 | ❌ 无 |
| 18 | IRQ hook 注册, devio | 12+13 | ❌ 无 |
| 19 | 闹钟, 虚拟定时器, times | 12+13 | ❌ 无 |
| 20 | BKL, CPU-local, IPI, AP 启动 | 08+09+12 | ❌ 无 |
| 21 | 各杂项调用 | 13 | ❌ 无 |

**结论**：严格按照 08→21 顺序阅读，读者不需要任何前后跳跃。每篇文档只使用**前面已读文档**引入的概念。

---

## 5. 与 tmp 文件的对应关系

| tmp 文件 | 内容归属 | 处理方式 |
|---------|---------|---------|
| `tmp-02-page-table-kernel.md` | createpde/lin_lin_copy/vm_memset/vm_lookup | → 09-runtime-cross-space.md（kboot 规划已覆盖） |
| `tmp-03-vm-request.md` | VM 请求协议 | → 16-syscall-vmctl.md §MEMREQ |
| `tmp-04-protection.md` | prot_init / GDT/IDT/TSS | → 已被 03-kmain-cstart 覆盖，移入 reference/ |
| `tmp-05-exception-interrupt.md` | 异常/中断处理 | → 12-exception-interrupt.md |
| `tmp-06-proc-struct.md` | struct proc 字段 | → 08-process-state.md |
| `tmp-07-scheduling.md` | pick_proc/enqueue/dequeue | → 09-scheduling.md |
| `tmp-08-endpoint.md` | endpoint 生成/验证 | → 08-process-state.md §身份字段 |
| `tmp-09-sync-ipc.md` | SEND/RECEIVE/NOTIFY | → 10-sync-ipc.md |
| `tmp-10-async-ipc.md` | SENDA / 异步消息 | → 11-async-ipc.md |
| `tmp-11-privilege.md` | struct priv / 权限矩阵 | → 08-process-state.md §特权 |
| `tmp-12-syscall-dispatch.md` | kernel_call / call_vec | → 13-kernel-call-dispatch.md |
| `tmp-13-syscall-memory.md` | safecopy/umap/vmctl/memset | → 15-syscall-copy.md + 16-syscall-vmctl.md + 21-syscall-misc.md |
| `tmp-14-syscall-fork-exec.md` | fork/exec/clear/exit | → 14-syscall-process.md |
| `tmp-15-syscall-exit-signal.md` | exit/kill/signal | → 14-syscall-process.md + 17-syscall-signal.md |
| `tmp-16-timer.md` | 时钟/定时器 | → 12-exception-interrupt.md §时钟中断 + 19-syscall-clock.md |
| `tmp-17-main-init.md` | kmain 全流程 | → 已被 03-07 拆分覆盖，移入 reference/ |
| `tmp-18-smp.md` | SMP/APIC/IPI | → 20-smp.md |
| `tmp-19-debug-serial.md` | 调试串口 | → 暂不纳入 runtime 主线，保留 tmp 或移入 reference/ |
| `tmp-20-acpi-watchdog.md` | ACPI/看门狗 | → 暂不纳入 runtime 主线 |
| `tmp-21-unported-symbols.md` | 未移植符号清单 | → 开发工具，不纳入文档主线 |

---

## 6. 设计决策汇总

| 决策点 | 选择 | 理由 |
|--------|------|------|
| 文档顺序 | 状态→调度→IPC→中断→分派→服务 | 人类理解自然顺序，严格线性依赖 |
| 08 是否合并 proc+priv | 合并为一篇 | 两者共同回答"进程是什么"，拆分会导致字段语义碎片化 |
| 10+11 是否合并 sync+async IPC | 不合并 | 同步 IPC 是核心机制，异步是优化变体；合并会模糊核心 |
| 14 是否拆分 fork/exec/exit | 不拆分 | 共同回答"进程生命周期"，语义连贯 |
| 16 VMCTL 是否独立 | 独立一篇 | VMCTL 是内核-VM 交互的核心协议，概念独立 |
| 20 SMP 放在最后 | 是 | SMP 是横切机制，理解它需要前面的全部基础 |
| tmp-19/20/21 是否纳入 | 不纳入主线 | 调试、ACPI、未移植符号不属于"运行时机制"核心 |
| Direct Map 的表述 | Ch1-2 忠实记录 C，Ch3-4 标注演进 | Ground Truth 优先 |

---

## 7. 实现优先级

| 优先级 | 任务 | 依赖 | 类型 |
|--------|------|------|------|
| P0 | 撰写 `08-process-state.md` | 07 完成 | 基础——所有后续文档依赖 |
| P0 | 撰写 `09-scheduling.md` | 08 | 基础——理解 IPC 和系统调用需要 |
| P1 | 撰写 `10-sync-ipc.md` | 09 | 核心机制 |
| P1 | 撰写 `11-async-ipc.md` | 10 | 核心机制补充 |
| P1 | 撰写 `12-exception-interrupt.md` | 10 | 核心机制 |
| P1 | 撰写 `13-kernel-call-dispatch.md` | 12 | 服务层入口 |
| P2 | 撰写 `14-syscall-process.md` | 13 | 具体服务 |
| P2 | 撰写 `15-syscall-copy.md` | 13 | 具体服务 |
| P2 | 撰写 `16-syscall-vmctl.md` | 15 | 具体服务 |
| P2 | 撰写 `17-syscall-signal.md` | 13 | 具体服务 |
| P2 | 撰写 `18-syscall-device.md` | 13 | 具体服务 |
| P2 | 撰写 `19-syscall-clock.md` | 13 | 具体服务 |
| P3 | 撰写 `20-smp.md` | 09+12 | 横切机制 |
| P3 | 撰写 `21-syscall-misc.md` | 13 | 收尾 |
