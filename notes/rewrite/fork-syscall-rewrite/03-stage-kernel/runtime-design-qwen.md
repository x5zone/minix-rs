# runtime-design-qwen: 03-stage-kernel 运行时设计规划

> **创建**: 2026-06-13
> **触发**: kboot 01-06 已完成，需要规划 kboot 之后 kernel 运行时的全部文档
> **方法**: 逐行追踪 Minix3 C 源码 proc.c / system.c / clock.c / interrupt.c / smp.c 及 arch/i386/ 下的架构特定文件，以"switch_to_user 之后发生了什么"为叙事主线
> **原则**: 叙事优先——读者从 06 的 `switch_to_user()` 走出来后，应该能沿着一条线读完整个 kernel 运行时，不需要前后跳跃

---

## 0. 问题诊断：为什么需要一份新规划

### 0.1 overview 的叙事缺陷

`00-kernel-overview.md` 把 kernel 的全部内容按"主题"分成 21 个文档（01-21），每个主题独立成篇。这种组织方式对"查阅"友好，但对"阅读"是灾难性的——读者读完 06-proc-struct 后跳到 09-sync-ipc，中间缺少"为什么 IPC 在 proc-struct 之后"的因果链；读完 12-syscall-dispatch 后跳到 14-syscall-fork-exec，又缺少"fork 为什么在 syscall-dispatch 之后"的过渡。

更严重的问题：overview 的"阶段 1-7"划分是**逻辑分组**，不是**时间线**。读者无法从"阶段 3: 进程抽象"自然过渡到"阶段 4: IPC"——因为在实际运行中，进程抽象和 IPC 是交织在一起的（`mini_send` 操作 `p_caller_q`，这是进程结构的一部分，也是 IPC 的一部分）。

### 0.2 tmp- 文件的教训

tmp-04 到 tmp-21 这 18 个文件是基于第一版 overview 规划写的。它们的内容本身没有事实错误，但存在三个结构性问题：

1. **叙事起点不统一**：tmp-04 从"保护模式是什么"开始，tmp-07 从"调度队列是什么"开始，tmp-09 从"IPC 原语是什么"开始——每个文件都在重新建立上下文，读者需要反复切换心智模型。
2. **因果关系缺失**：tmp-07 讲调度但没有解释"为什么调度在 IPC 之前"；tmp-12 讲系统调用路由但没有解释"为什么路由在 IPC 之后"。
3. **运行时 vs 初始化混淆**：tmp-02 讲的是运行时跨空间机制（createpde/lin_lin_copy），但它的编号（02）暗示它是 boot 序列的一部分。

### 0.3 本文的立场

本文以 **`switch_to_user()` 之后 CPU 在做什么** 为唯一叙事主线。每个子系统不是"独立主题"，而是"CPU 在执行用户代码时必然遇到的下一个事件"。

叙事逻辑：**内核醒来 → 选进程 → 用户态跑 → 中断来 → 系统调用 → 进程间通信 → 跨空间拷贝 → VM 协商 → 进程生灭 → 多核 → 辅助设施**。

---

## 1. 内核的激活模型：没有主循环的程序

### 1.1 三种激活路径

VM/PM/VFS 是用户态服务——它们有明确的 `while(1) { receive(); dispatch(); send(); }` 主循环。Kernel 没有。

Kernel 从 `switch_to_user()` 开始，永远运行用户态代码。Kernel 自身只在以下三种情况被激活：

```
用户态进程 A 正在执行
  │
  ├─ [路径 1] 硬件中断 ──→ CPU 自动压栈 → IDT[IRQ+32] → interrupt_handler()
  │                         → irq_handle() → 设备 handler → 返回用户态
  │
  ├─ [路径 2] 异常 ──────→ CPU 自动压栈 → IDT[vec] → exception_handler()
  │                         → 页错误? → 转发 VM
  │                         → 其他? → cause_sig() → 返回用户态或 panic
  │
  └─ [路径 3] 系统调用 ──→ INT 0x80/SYSENTER → sys_call 汇编入口
                            → do_ipc() 或 kernel_call()
                            → 处理完毕 → 返回用户态
```

**关键洞察**：三条路径的出口都是同一个——回到用户态，继续执行被打断的进程（或切换到另一个进程）。Kernel 不存在"自己做自己的事"的时刻——它永远是在**响应用户态的需求**。

### 1.2 汇聚点：switch_to_user()

三条路径的处理完成后，最终都汇聚到 `switch_to_user()`（proc.c:299）。这个函数是内核的"调度决策点"——每次从用户态陷入内核，处理完毕后，都要经过 `switch_to_user()` 决定下一个运行的进程。

`switch_to_user()` 不是"初始化函数"——它是**运行时循环的核心**。每次时钟中断触发调度、每次 IPC 阻塞触发切换、每次系统调用返回用户态，都经过这个函数。

```
switch_to_user() 的状态机：

  1. proc_ptr 可运行？
     ├── 是 → 检查 misc_flags（MF_KCALL_RESUME / MF_DELIVERMSG / MF_SC_DEFER ...）
     │        ├── 有需要处理的 → 逐个处理 → 处理后可运行？→ 回到 1
     │        └── 没有 → 检查 quantum → 跳到 3
     └── 否 → pick_proc()
              ├── 有进程 → 更新 proc_ptr → switch_address_space() → 跳到 1
              └── 无进程 → idle() → 等中断 → 回到 pick_proc()

  2. （misc_flags 处理循环）
     ├── MF_KCALL_RESUME → kernel_call_resume() → 重新执行被 VMSUSPEND 挂起的系统调用
     ├── MF_DELIVERMSG   → delivermsg() → 把待投递消息拷贝到用户缓冲区
     ├── MF_SC_DEFER     → arch_do_syscall() → 执行被 trace 延迟的系统调用
     ├── MF_SC_TRACE     → cause_sig(SIGTRAP) → 通知 tracer 系统调用退出
     └── MF_SC_ACTIVE    → 清除标记 → break

  3. 最终准备
     ├── proc_no_time() → 时间片耗尽处理
     ├── arch_finish_switch_to_user() → 架构特定的最终切换
     ├── context_stop(KERNEL) → 停止内核时间记账
     ├── FPU 异常控制 → enable/disable_fpu_exception()
     ├── restart_local_timer() → 重武装时钟中断
     └── restore_user_context(p) → 恢复寄存器 → IRET/SYSEXIT → 回到用户态
```

**这就是 kernel 运行时的心脏。** 后续所有子系统——调度、IPC、异常、系统调用——都是这个心脏的组成部分。

### 1.3 文档叙事的第一原则

> **每篇文档都应该能回答"它在 switch_to_user 循环中的哪个位置"。**

如果一个机制不在 `switch_to_user()` 循环中直接出现，那它一定是被循环中的某个步骤**调用**的（如 `mini_send` 被 `do_ipc` 调用，`do_ipc` 被 `sys_call` 触发，`sys_call` 是路径 3 的入口）。

---

## 2. 进程调度：谁该运行

### 2.1 调度在 switch_to_user 中的位置

`switch_to_user()` 的第一步就是检查 `proc_ptr` 是否可运行。如果不可运行（或不存在），调用 `pick_proc()` 从就绪队列中选一个。

调度不是"独立子系统"——它是 `switch_to_user()` 的核心分支。没有调度，`switch_to_user()` 无法决定恢复哪个进程的上下文。

### 2.2 多级优先级队列

Minix3 的调度策略是**固定优先级 + 时间片轮转**：

```
CPU 0 的就绪队列：
  run_q_head[0] → [proc A] → [proc B] → NULL     ← 最高优先级
  run_q_head[1] → NULL
  ...
  run_q_head[6] → [proc C] → NULL                  ← 服务进程优先级
  ...
  run_q_head[15] → [proc D] → NULL                 ← 最低优先级

CPU 1 的就绪队列：（独立）
  run_q_head[0] → ...
```

- `NR_SCHED_QUEUES = 16` 个优先级，0 最高，15 最低
- 每个队列是单向链表，通过 `p_nextready` 串联
- 每个 CPU 有独立的队列数组（`cpulocals.h` 中的 `run_q_head` / `run_q_tail`）
- `pick_proc()` 从队列 0 开始扫描，返回第一个非空队列的队首

### 2.3 入队与出队

| 操作 | 函数 | 触发时机 |
|------|------|---------|
| 入队（队尾） | `enqueue(rp)` | 进程变为可运行（IPC 完成、信号处理完毕、时间片重置） |
| 入队（队首） | `enqueue_head(rp)` | 被抢占的进程恢复——保证公平性 |
| 出队 | `dequeue(rp)` | 进程阻塞（IPC 等待、VMREQUEST）、被停止 |
| 选择 | `pick_proc()` | `switch_to_user()` 中当前进程不可运行时 |

**抢占机制**：`enqueue()` 在入队后检查新进程优先级是否高于当前 `proc_ptr`。若当前进程可抢占（`priv(p)->s_flags & PREEMPTIBLE`），设置 `RTS_PREEMPTED`——这会让 `switch_to_user()` 重新选进程。

### 2.4 时间片管理

每个进程有 `p_cpu_time_left`（剩余 CPU 时间，单位是 TSC tick）和 `p_quantum_size_ms`（时间片大小，单位是毫秒）。

时钟中断每次递减当前进程的 `p_cpu_time_left`。当减到 0 时，`switch_to_user()` 调用 `proc_no_time(p)`：

```
proc_no_time(p):
  if 进程有用户空间调度器 && 可抢占:
    → notify_scheduler(p)
      → RTS_SET(p, RTS_NO_QUANTUM)   // 出队
      → mini_send(p, scheduler, SCHEDULING_NO_QUANTUM)  // 通知调度器
  else:
    → 直接重置时间片: p_cpu_time_left = ms_2_cpu_time(p_quantum_size_ms)
```

### 2.5 IDLE 进程

当所有就绪队列为空时，`pick_proc()` 返回 NULL。`switch_to_user()` 进入 `idle()` 循环：

```
idle():
  proc_ptr = idle_proc
  switch_address_space(VM)     // SMP 下切换到 VM 的地址空间
  restart_local_timer()        // 或 stop_local_timer()（AP 上）
  context_stop(KERNEL)
  halt_cpu()                   // HLT 指令，等待中断
  // 中断到来后从这里继续 → 回到 switch_to_user → pick_proc()
```

`idle_proc` 是每个 CPU 独立的进程结构（`CONFIG_MAX_CPUS` 个），共享 `idle_priv`（flags=IDL_F）。

### 2.6 文档定位

调度是 `switch_to_user()` 的核心分支。它应该是 kboot 之后的**第一篇运行时文档**——因为没有调度，后续一切（IPC、系统调用、异常处理）都无法发生。

---

## 3. 中断与时钟：内核的心跳

### 3.1 中断在 switch_to_user 循环中的位置

`switch_to_user()` 最后一步是 `restore_user_context(p)`——恢复寄存器、开中断、跳到用户态。用户态代码执行直到下一次"陷阱"（中断/异常/系统调用）。

**时钟中断是唯一必然到来的事件**。键盘可以一直不按，网卡可以没有数据，但时钟每 10ms（100Hz）一定触发。时钟中断是调度的驱动力——没有时钟中断，时间片不会耗尽，进程不会被抢占。

### 3.2 中断处理的完整路径

```
硬件设备触发 IRQ
  → CPU 自动压栈（SS/ESP/EFLAGS/CS/EIP）
  → IDT[IRQ + 32] 对应的中断门
  → 汇编 stub（保存通用寄存器、调用 C handler）
  → irq_handle(irq)（interrupt.c）
    → 遍历 irq_handlers[irq] 链表
    → 调用每个 handler（返回 TRUE 表示已处理）
    → hw_intr_ack(irq)
  → 汇编 stub（恢复寄存器、IRET）
  → 回到用户态（或 switch_to_user 如果调度被触发）
```

**时钟中断 handler 链**：`timer_int_handler()`（clock.c:78）是时钟 IRQ 的 C handler。它做以下事情：

1. `kclockinfo.uptime++`（BSP 上）
2. `adjtime_delta` 调整实时时钟
3. 当前进程 `p_user_time++`
4. 如果不是 billable 进程，`bill_ptr->p_sys_time++`
5. 递减虚拟定时器（`p_virt_left`、`p_prof_left`）
6. `vtimer_check()` 检查定时器是否到期
7. `load_update()` 更新负载统计
8. 检查时钟定时器队列是否到期 → `tmrs_exptimers()`
9. `arch_timer_int_handler()` → APIC/8259 EOI

**关键**：时钟中断 handler 在**中断上下文**中运行——它不是任何进程的一部分。它修改的是当前进程的记账数据，但调度决策要等到下一次 `switch_to_user()` 才发生。

### 3.3 中断控制器：8259 PIC vs APIC

Minix3 支持两种中断控制器：

| | 8259 PIC | APIC |
|---|---|---|
| 初始化 | `intr_init(0)` → i8259.c | `lapic_init()` + `ioapic_init()` → apic.c |
| IRQ 路由 | 固定 IRQ 0-15 | 可配置向量号 |
| SMP | 不支持 | 支持（IPI + IOAPIC） |
| 时钟源 | IRQ 0（8254 PIT） | Local APIC Timer |
| 切换时机 | `cstart()` 中 `intr_init(0)` | `smp_init()` 成功后 |

`intr_init(0)` 在 `cstart()` 中调用（04 文档已覆盖）。APIC 初始化在 `smp_init()` 中——如果 SMP 失败，回退到单 CPU + 8259 PIC。

### 3.4 文档定位

中断处理是 `switch_to_user()` 循环的**输入驱动力**。它和调度是同一层级的概念——调度决定"谁跑"，中断决定"什么时候换"。

中断处理应该紧跟调度文档——因为时钟中断是调度的直接驱动力，而其他中断是 IPC/系统调用的触发源。

---

## 4. 异常处理：当用户态出错

### 4.1 异常 vs 中断

| | 异常 (Exception) | 中断 (Interrupt) |
|---|---|---|
| 触发 | 当前指令导致 | 外部设备异步触发 |
| 可预测 | 是（执行某条指令必然触发） | 否（随时可能来） |
| 可重试 | 部分可以（缺页修复后重执行） | 不可以（设备状态已变） |
| 向量 | 0-31（CPU 固定） | 32+（OS/硬件定义） |

### 4.2 异常处理的完整路径

`exception_handler()`（exception.c:213）是所有异常的 C 入口。它根据异常发生的位置（用户态 vs 内核态）和异常类型，走不同的分支：

```
exception_handler(is_nested, frame):
  saved_proc = proc_ptr
  ep = ex_data[frame->vector]     // 异常元数据（名称、信号、最低 CPU）

  // 特殊情况：嵌套异常
  if is_nested:
    // 在 copy_msg_to_user/from_user 中缺页 → 跳转到失败处理
    // 在 fxrstor/frstor 中异常 → 跳转到 FPU 失败处理
    // 调试向量 + trace bit → 清除 trace bit，返回
    // 其他嵌套异常 → panic

  // 页错误 → 特殊处理（见 §4.3）
  if frame->vector == PAGE_FAULT_VECTOR:
    pagefault(saved_proc, frame, is_nested)
    return

  // 用户态异常 → 转信号
  if !is_nested && !iskernelp(saved_proc):
    cause_sig(proc_nr(saved_proc), ep->signum)
    return

  // 内核态异常 → panic
  inkernel_disaster(saved_proc, frame, ep, is_nested)
  panic("return from inkernel_disaster")
```

### 4.3 页错误：内核态 vs 用户态

页错误（vector 14）是最复杂的异常，因为它有**两种完全不同的处理路径**：

**用户态页错误 → 转发 VM**：
```
pagefault(pr, frame, is_nested):
  cr2 = read_cr2()                    // 缺页地址
  if pr == VM_PROC_NR:
    panic("pagefault in VM")          // VM 自己缺页 = 死锁

  RTS_SET(pr, RTS_PAGEFAULT)          // 挂起进程
  m_pagefault.m_source = pr->endpoint
  m_pagefault.m_type = VM_PAGEFAULT
  m_pagefault.VPF_ADDR = cr2
  m_pagefault.VPF_FLAGS = frame->errcode
  mini_send(pr, VM_PROC_NR, &m_pagefault, FROM_KERNEL)
```

**内核态页错误（在 cross_space_copy 中）→ 异常恢复**：
```
// 在 phys_copy / phys_memset 中缺页：
// → 修改 frame->eip 指向 phys_copy_fault / memset_fault
// → 返回后从 fault handler 继续，返回错误码
```

这是 Minix3 的一个精巧设计：内核在跨进程拷贝时，通过设置 `catch_pagefaults` 标志和 fault recovery 地址，把内核态缺页转化为可控的错误返回，而不是 panic。

### 4.4 文档定位

异常处理是 `switch_to_user()` 循环的**另一条输入路径**（和中断并列）。页错误尤其重要——它是内核和 VM 交互的关键通道。

异常处理应该在中断文档之后——因为它们共享 IDT 基础设施，且页错误转发使用了 IPC（`mini_send`）。

---

## 5. 系统调用入口：用户态请求内核服务

### 5.1 系统调用 vs IPC

在 Minix3 中，"系统调用"和"IPC"是两个不同层次的概念：

- **IPC**（`do_ipc`）：进程间的消息传递。SEND/RECEIVE/SENDREC/NOTIFY 是 IPC 原语。内核只做消息中转——不执行业务逻辑。
- **系统调用**（`kernel_call`）：进程请求内核执行特权操作。SYS_VMCTL/SYS_FORK/SYS_IRQCTL 等是内核调用。内核直接执行业务逻辑。

两者共享同一个入口：`sys_call`（汇编）。`do_ipc()` 根据 call_nr 分发：

```
do_ipc(r1, r2, r3):
  call_nr = r1
  switch(call_nr):
    case SENDREC/SEND/RECEIVE/NOTIFY/SENDNB:
      → do_sync_ipc()           // IPC 原语
    case SENDA:
      → mini_senda()            // 异步 IPC
    case MINIX_KERNINFO:
      → 设置 secondary IPC return
    default:
      → EBADCALL
```

但 `kernel_call()` 是另一条路径——它不是通过 `do_ipc` 进入的，而是通过 `INT 0x80` + 特殊的 `KERNEL_CALL` 消息类型：

```
用户态: sys_call(SYS_VMCTL, ...)
  → INT 0x80 / SYSENTER
  → 汇编 sys_call 入口
  → 检查消息类型
  → if m_type >= KERNEL_CALL:
      → kernel_call(m_user, caller)
        → kernel_call_dispatch()
          → call_vec[call_nr - KERNEL_CALL](caller, msg)
        → kernel_call_finish()
    else:
      → do_ipc(call_nr, r2, r3)
```

**实际上，Minix3 的系统调用有两种触发方式**：
1. IPC 方式：系统服务发送 `SYS_VMCTL` 类型的 IPC 消息给 SYSTEM 任务
2. 直接方式：通过 `sys_call` 汇编入口直接触发 `kernel_call()`

### 5.2 kernel_call 的完整路径

```
kernel_call(m_user, caller):
  caller->p_delivermsg_vir = m_user
  copy_msg_from_user(m_user, &msg)     // 从用户空间拷贝消息到内核栈
  msg.m_source = caller->p_endpoint
  result = kernel_call_dispatch(caller, &msg)
  kbill_kcall = caller                  // 记账
  kernel_call_finish(caller, &msg, result)

kernel_call_dispatch(caller, msg):
  call_nr = msg->m_type - KERNEL_CALL
  if call_nr 越界 → EBADREQUEST
  if 权限检查失败 → ECALLDENIED
  if call_vec[call_nr] == NULL → EBADREQUEST
  result = call_vec[call_nr](caller, msg)    // 调用处理函数
  return result

kernel_call_finish(caller, msg, result):
  if result == VMSUSPEND:
    // 保存请求消息，等待 VM 处理
    caller->p_vmrequest.saved.reqmsg = *msg
    caller->p_misc_flags |= MF_KCALL_RESUME
  else:
    if result != EDONTREPLY:
      msg->m_source = SYSTEM
      msg->m_type = result
      copy_msg_to_user(msg, caller->p_delivermsg_vir)  // 写回结果
```

### 5.3 call_vec 初始化：system_init()

`system_init()`（system.c:168）在 boot 阶段（T8）被调用。它做三件事：

1. **IRQ hook 初始化**：`irq_hooks[i].proc_nr_e = NONE`
2. **Alarm timer 初始化**：遍历所有 priv 结构的 `s_alarm_timer`
3. **call_vec 注册**：先全部置 NULL，然后逐个 `map(SYS_XXX, do_xxx)`

call_vec 的完整注册表（从 system.c:196-277）：

| 类别 | 系统调用 | 处理函数 |
|------|---------|---------|
| **进程管理** | SYS_FORK | do_fork |
| | SYS_EXEC | do_exec |
| | SYS_CLEAR | do_clear |
| | SYS_EXIT | do_exit |
| | SYS_PRIVCTL | do_privctl |
| | SYS_TRACE | do_trace |
| | SYS_SETGRANT | do_setgrant |
| | SYS_RUNCTL | do_runctl |
| | SYS_UPDATE | do_update |
| | SYS_STATECTL | do_statectl |
| **信号** | SYS_KILL | do_kill |
| | SYS_GETKSIG | do_getksig |
| | SYS_ENDKSIG | do_endksig |
| | SYS_SIGSEND | do_sigsend |
| | SYS_SIGRETURN | do_sigreturn |
| **设备 I/O** | SYS_IRQCTL | do_irqctl |
| | SYS_DEVIO (x86) | do_devio |
| | SYS_VDEVIO (x86) | do_vdevio |
| **内存** | SYS_MEMSET | do_memset |
| | SYS_VMCTL | do_vmctl |
| **拷贝** | SYS_UMAP | do_umap |
| | SYS_UMAP_REMOTE | do_umap_remote |
| | SYS_VUMAP | do_vumap |
| | SYS_VIRCOPY | do_vircopy |
| | SYS_PHYSCOPY | do_copy |
| | SYS_SAFECOPYFROM | do_safecopy_from |
| | SYS_SAFECOPYTO | do_safecopy_to |
| | SYS_VSAFECOPY | do_vsafecopy |
| | SYS_SAFEMEMSET | do_safememset |
| **时钟** | SYS_TIMES | do_times |
| | SYS_SETALARM | do_setalarm |
| | SYS_STIME | do_stime |
| | SYS_SETTIME | do_settime |
| | SYS_VTIMER | do_vtimer |
| **系统控制** | SYS_ABORT | do_abort |
| | SYS_GETINFO | do_getinfo |
| | SYS_DIAGCTL | do_diagctl |
| **性能分析** | SYS_SPROF | do_sprofile |
| **调度** | SYS_SCHEDULE | do_schedule |
| | SYS_SCHEDCTL | do_schedctl |
| **机器状态** | SYS_SETMCONTEXT | do_setmcontext |
| | SYS_GETMCONTEXT | do_getmcontext |
| **x86 特定** | SYS_READBIOS | do_readbios |
| | SYS_IOPENABLE | do_iopenable |
| | SYS_SDEVIO | do_sdevio |

### 5.4 文档定位

系统调用路由是 `switch_to_user()` 循环中**路径 3（系统调用入口）的核心分发逻辑**。它应该在异常处理之后——因为异常和系统调用共享陷阱基础设施（IDT、压栈、特权级切换），且系统调用的 VMSUSPEND 机制依赖异常处理中的页错误转发。

---

## 6. IPC 机制：进程间消息传递

### 6.1 IPC 在 switch_to_user 循环中的位置

IPC 原语（SEND/RECEIVE/SENDREC/NOTIFY）是通过系统调用入口（`do_ipc`）触发的。但 IPC 的结果直接影响 `switch_to_user()` 的行为：

- **SEND 阻塞**：`RTS_SET(caller, RTS_SENDING)` → caller 不可运行 → `switch_to_user()` 选另一个进程
- **RECEIVE 阻塞**：`RTS_SET(caller, RTS_RECEIVING)` → 同上
- **SEND 成功**：目标进程的 `MF_DELIVERMSG` 被设置 → 目标进程在 `switch_to_user()` 中会投递消息
- **NOTIFY 成功**：目标进程的 `s_notify_pending` 位图被设置 → 目标进程在 `mini_receive()` 中检查

### 6.2 四个同步原语

**mini_send(caller, dst_e, m_ptr, flags)**（proc.c:870）：
```
if 目标正在 RECEIVE 且愿意接收本进程的消息:
  copy_msg_from_user(m_ptr, &dst->p_delivermsg)   // 拷贝消息到目标
  dst->p_delivermsg.m_source = caller->p_endpoint
  dst->p_misc_flags |= MF_DELIVERMSG               // 标记待投递
  RTS_UNSET(dst, RTS_RECEIVING)                    // 唤醒目标
else:
  if NON_BLOCKING: return ENOTREADY
  deadlock(SEND, caller, dst_e) → 检测死锁
  copy_msg_from_user(m_ptr, &caller->p_sendmsg)    // 保存消息到 caller
  RTS_SET(caller, RTS_SENDING)                     // 阻塞 caller
  caller->p_sendto_e = dst_e
  将 caller 加入 dst->p_caller_q 链表              // 排队
```

**mini_receive(caller, src_e, m_buff, flags)**（proc.c:967）：
```
检查顺序：
  1. 待处理通知（s_notify_pending）→ 组装 NOTIFY 消息
  2. 待处理异步消息（s_asyn_pending）→ try_async()
  3. caller 的发送队列（p_caller_q）→ 找到匹配的发送者
如果都没有:
  if NON_BLOCKING: return ENOTREADY
  deadlock(RECEIVE, caller, src_e)
  RTS_SET(caller, RTS_RECEIVING)
  caller->p_getfrom_e = src_e
```

**mini_notify(caller, dst_e)**（proc.c:1122）：
```
if 目标正在 RECEIVE 且可以接收:
  BuildNotifyMessage(&dst->p_delivermsg, ...)      // 组装通知消息
  dst->p_misc_flags |= MF_DELIVERMSG
  RTS_UNSET(dst, RTS_RECEIVING)
else:
  set_sys_bit(priv(dst)->s_notify_pending, src_id) // 标记待处理
```

**SENDREC** = SEND + RECEIVE 的组合。设置 `MF_REPLY_PEND` 防止 NOTIFY 在 SEND 和 RECEIVE 之间插入。

### 6.3 消息投递：delivermsg()

`delivermsg()`（proc.c:252）在 `switch_to_user()` 的 misc_flags 循环中被调用。它把 `p_delivermsg` 拷贝到用户空间缓冲区 `p_delivermsg_vir`：

```
delivermsg(rp):
  if copy_msg_to_user(&rp->p_delivermsg, rp->p_delivermsg_vir) != OK:
    if MF_MSGFAILED:                                // 第二次失败
      cause_sig(rp->p_nr, SIGSEGV)                  // 进程地址空间有问题
    else:                                           // 第一次失败
      vm_suspend(rp, rp, rp->p_delivermsg_vir,      // 请求 VM 修复页表
                 sizeof(message), VMSTYPE_DELIVERMSG, 1)
      rp->p_misc_flags |= MF_MSGFAILED
  else:
    rp->p_delivermsg.m_source = NONE                // 标记已投递
    rp->p_misc_flags &= ~(MF_DELIVERMSG | MF_MSGFAILED)
    if !(rp->p_misc_flags & MF_CONTEXT_SET):
      rp->p_reg.retreg = OK                         // 设置返回值
```

**关键**：消息投递可能失败（用户缓冲区页面不在）。这时不是 panic，而是通过 `vm_suspend()` 挂起进程、通知 VM 修复页表，等 VM 完成后恢复。这就是 VMSUSPEND 机制的一个实例。

### 6.4 异步 IPC：SENDA

异步 IPC（`mini_senda`）允许进程一次性提交一批消息。消息表（`asynmsg_t[]`）在用户空间，内核逐个检查并投递。未投递成功的消息记录在 `priv->s_asyntab` 中，等目标进程 RECEIVE 时通过 `try_async()` 重试。

### 6.5 文档定位

IPC 是微内核的脊柱——所有服务间通信都通过 IPC。它应该在系统调用路由之后——因为 IPC 原语和系统调用共享 `sys_call` 入口，且 IPC 的阻塞/唤醒机制直接操作 `switch_to_user()` 的状态。

---

## 7. 跨空间内存操作：内核如何访问进程内存

### 7.1 问题的本质

Kernel 运行在自己的地址空间中（高地址，ring 0）。用户进程运行在各自的地址空间中（用户地址范围，ring 3）。当内核需要拷贝进程 A 的消息到进程 B 的缓冲区时，它必须**跨越两个不同的地址空间**执行内存操作。

在 32 位 Minix3 C 中，这是一个复杂的问题——内核没有直接映射物理内存的手段，必须通过临时页表映射（`createpde`）来"借道"访问其他进程的页面。

在 64 位 Rust 中，Direct Map 让这个问题大幅简化——`kernel_phys_to_virt(pa)` 一行加法就能把物理地址转为内核虚拟地址。但 Ch1-2 必须忠实记录 C 源码行为。

### 7.2 Minix3 C 的方案：createpde 临时映射

Minix3 在 boot 阶段（06 文档）分配了 2 个空闲 PDE 槽位（`freepdes[2]`）。运行时跨空间拷贝时：

```
createpde(pr, linaddr, bytes, free_pde_idx, changed):
  if pr == 当前进程 || pr == 内核:
    return linaddr                     // 直接可访问，无需映射
  // 从 pr 的页表中取出对应地址的 PDE 值
  pdeval = pr->p_seg.p_cr3_v[pde_index(linaddr)]
  // 把 PDE 值写入内核页表的 freepde 槽位
  ptproc->p_seg.p_cr3_v[freepdes[free_pde_idx]] = pdeval
  // 刷新 TLB
  reload_cr3()
  // 返回内核虚拟地址
  return KERNEL_VM_BASE + pde_index * 4MB + offset
```

`lin_lin_copy()`（pg_utils.c）用 `createpde` 在源和目标之间建立 4MB 窗口，然后在窗口内 `memcpy`。大拷贝被拆分为多个 4MB 窗口。

`vm_memset()` 类似——在目标的地址空间建立临时映射，然后 `memset`。

`mem_clear_mapcache()` 在每次跨空间操作后清理临时 PDE——防止内核页表中残留其他进程的映射。

### 7.3 VMSUSPEND：跨空间操作的异常路径

跨空间拷贝可能触发页错误（目标页面不在物理内存中）。Minix3 的处理方式：

```
virtual_copy_f() / data_copy():
  createpde() 建立临时映射
  memcpy
  if 页错误:
    → catch_pagefaults 机制捕获
    → 返回错误码
    → vm_suspend(caller, target, addr, len, type, writeflag)
      → RTS_SET(caller, RTS_VMREQUEST)
      → 设置 p_vmrequest 参数
      → 加入 vmrequest 链表
      → send_sig(VM_PROC_NR, SIGKMEM)    // 通知 VM
```

VM 收到通知后，通过 `SYS_VMCTL(VMCTL_MEMREQ_GET)` 获取请求详情，修复页表，然后通过 `SYS_VMCTL(VMCTL_MEMREQ_REPLY)` 告诉内核结果。内核恢复被挂起的进程（`MF_KCALL_RESUME`），重新执行系统调用。

### 7.4 64 位 Direct Map 的影响

| 32 位 C 机制 | 64 位 Rust 替代 | 说明 |
|-------------|----------------|------|
| `createpde()` | `kernel_phys_to_virt(pa)` | 一行加法替代临时 PDE |
| `freepdes[2]` | 不需要 | Direct Map 常驻 |
| `lin_lin_copy()` | `phys_to_virt() + memcpy` | 普通内存拷贝 |
| `vm_memset()` | `phys_to_virt() + memzero` | 普通内存清零 |
| `mem_clear_mapcache()` | 不需要 | 没有临时映射需要清理 |
| `vm_lookup()` | 保留 | 页表遍历仍然需要 |
| `VMSUSPEND` | 保留 | 缺页挂起协议与 Direct Map 无关 |

### 7.5 文档定位

跨空间内存操作是 IPC 和系统调用的**基础设施**——`mini_send` 需要 `copy_msg_from_user`，`delivermsg` 需要 `copy_msg_to_user`，`do_vircopy`/`do_safecopy` 需要 `virtual_copy_f`。

它应该在 IPC 之后——因为读者已经理解了"消息需要从 A 拷贝到 B"，现在需要知道"内核怎么做到这个拷贝"。

---

## 8. 特权与权限：谁有权做什么

### 8.1 struct priv 的核心角色

`struct priv`（priv.h）是 Minix3 的权限矩阵。每次 IPC 和系统调用都检查权限：

- `s_trap_mask`：允许哪些 IPC 原语（SEND/RECEIVE/SENDREC/NOTIFY）
- `s_ipc_to`（`sys_map_t`）：允许向哪些进程发送消息
- `s_k_call_mask`：允许哪些系统调用
- `s_io_tab`：允许访问哪些 I/O 端口
- `s_mem_tab`：允许映射哪些内存区域
- `s_irq_tab`：允许使用哪些 IRQ 线

### 8.2 两类进程

| | 系统进程（SYS_PROC） | 用户进程 |
|---|---|---|
| priv 结构 | 独立 `priv[i]` | 共享 `priv[USER_PRIV_ID]` |
| 数量 | `NR_SYS_PROCS` 个槽位 | 1 个共享 |
| 权限 | 细粒度（每个服务不同） | 最小权限 |
| 分配 | boot 时静态分配 + 运行时 RS 动态分配 | fork 时继承 USER_PRIV_ID |

### 8.3 权限检查在运行时发生的位置

```
mini_send(caller, dst):
  may_send_to(caller, dst_p)          // 检查 s_ipc_to

do_sync_ipc(caller, call_nr, ...):
  priv(caller)->s_trap_mask & (1 << call_nr)   // 检查 IPC 原语权限

kernel_call_dispatch(caller, msg):
  GET_BIT(priv(caller)->s_k_call_mask, call_nr) // 检查系统调用权限

do_irqctl(caller, msg):
  priv(caller)->s_irq_tab[]           // 检查 IRQ 权限

do_devio(caller, msg):
  priv(caller)->s_io_tab[]            // 检查 I/O 端口权限
```

### 8.4 文档定位

特权结构体是 IPC 和系统调用的**权限基础设施**。它应该和系统调用/IPC 一起讨论，而不是作为独立主题——因为 `priv` 的每个字段都在 IPC/系统调用的代码路径中被检查。

考虑到叙事的连贯性，priv 可以作为 IPC 和系统调用文档中的一个核心章节，而不是独立成篇。但如果内容量足够（priv 结构体有 ~60 行字段，加上权限检查逻辑），独立成篇也合理。

---

## 9. VM 协商协议：内核和 VM 的对话

### 9.1 为什么需要协商

Kernel 不独立管理页表——页表的建立和修改由 VM 决策。但内核需要 VM 帮它做事（建立 direct map、修复缺页），VM 也需要内核帮它做事（切换 CR3、清除页错误）。这是一个**双向协议**。

### 9.2 do_vmctl 的子命令

`do_vmctl()`（system/do_vmctl.c）是 VM 向内核发出请求的处理函数。它的子命令分为几类：

**页错误处理**：
- `VMCTL_CLEAR_PAGEFAULT`：VM 修复了页表，清除进程的 `RTS_PAGEFAULT`

**内存请求队列**：
- `VMCTL_MEMREQ_GET`：VM 获取下一个挂起的内存请求
- `VMCTL_MEMREQ_REPLY`：VM 告诉内核内存请求的处理结果

**地址空间管理**（arch_do_vmctl）：
- `VMCTL_SETADDRSPACE`：设置进程的 CR3（页目录物理地址）
- `VMCTL_GET_PDBR`：获取进程的 CR3
- `VMCTL_FLUSHTLB`：刷新 TLB
- `VMCTL_I386_INVLPG`：刷新指定页面的 TLB 条目

**内核物理映射**（arch_do_vmctl / arch/i386）：
- `VMCTL_KERN_PHYSMAP`：内核请求 VM 映射物理区域
- `VMCTL_KERN_MAP_REPLY`：VM 告诉内核映射的虚拟地址

**VM 启动协议**：
- `VMCTL_VMINHIBIT_CLEAR`：VM 为进程建好页表后，清除 `RTS_VMINHIBIT`

### 9.3 VM 启动后的协商序列

`switch_to_user()` 之后，VM 是第一个被调度的进程（唯一没有 `RTS_VMINHIBIT` 的 boot 进程）。VM 启动后执行以下协商：

```
VM 运行 → 初始化自身
  → init_page_table() → map_kernel()
    → 建立 kernel direct map（KERNEL_DIRECT_MAP_BASE, U/S=0, G=1）
  → SYS_VMCTL(VMCTL_SETADDRSPACE, VM's CR3)
    → setcr3(): 内核切换到 VM 的真实页表
    → arch_enable_paging(): 开启分页
    → RTS_UNSET(VM, RTS_VMINHIBIT)
  → SYS_VMCTL(VMCTL_KERN_PHYSMAP)
    → 内核声明需要映射的物理区域
  → SYS_VMCTL(VMCTL_KERN_MAP_REPLY)
    → 内核获得虚拟地址
  → VM 为 PM/VFS/RS 等创建页表
  → SYS_VMCTL(VMCTL_VMINHIBIT_CLEAR, each_proc)
    → 解除 PM/VFS/RS 的 RTS_VMINHIBIT
    → 其他 boot 进程开始可调度
  → vm_running = 1
```

### 9.4 文档定位

VM 协商协议是系统调用的一个实例（`SYS_VMCTL`），但它的时序特殊性（发生在 boot 之后、其他进程运行之前）使其值得单独讨论。

建议：VM 协商作为跨空间内存操作文档的延伸——因为读者已经理解了"内核需要 VM 帮它管页表"，VM 协商就是这个协议的具体内容。

---

## 10. 进程生命周期：fork / exit / clear

### 10.1 内核在 fork 中的角色

Minix3 的 fork 工作量分布：**VM 90%（页表/CoW/mmap）+ PM 10%（生命周期协调）+ Kernel <5%（proc 槽分配）**。

内核的 fork（`do_fork`，system/do_fork.c）做以下事情：

```
do_fork(caller, msg):
  rpp = proc_addr(parent)               // 父进程
  rpc = proc_addr(msg->slot)            // 子进程槽位
  save_fpu(rpp)                          // 保存 FPU 状态
  *rpc = *rpp                            // 拷贝整个 proc 结构
  gen++                                  // generation 递增
  rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr)  // 新 endpoint
  rpc->p_reg.retreg = 0                  // 子进程返回 0
  rpc->p_user_time = rpc->p_sys_time = 0 // 清零记账
  RTS_SET(rpc, RTS_NO_QUANTUM)           // 不可调度（等调度器分配时间片）
  if 父进程是特权进程:
    rpc->p_priv = priv_addr(USER_PRIV_ID) // 子进程默认用户权限
    RTS_SET(rpc, RTS_NO_PRIV)
  if PFF_VMINHIBIT:
    RTS_SET(rpc, RTS_VMINHIBIT)          // 等 VM 建页表
  msg->endpt = rpc->p_endpoint           // 返回子进程 endpoint
  msg->msgaddr = rpp->p_delivermsg_vir   // 返回消息地址
```

**关键**：内核的 fork 只创建 proc 结构——不拷贝内存（VM 的 CoW）、不加载 ELF（PM 的 exec）。子进程是父进程的**精确副本**（`*rpc = *rpp`），但 endpoint generation 递增、记账清零、权限降级。

### 10.2 exit 和 clear

**do_exit**：系统进程请求退出。内核标记进程状态，通知 PM。

**do_clear**：PM 告诉内核清理进程槽位。`clear_endpoint()` 做以下事情：
1. `RTS_SET(rc, RTS_NO_ENDPOINT)` — 进程不能再通信
2. `clear_ipc(rc)` — 从目标的发送队列中移除
3. `clear_ipc_refs(rc, ret)` — 通知所有依赖此进程的其他进程
4. `clear_memreq(rc)` — 从 vmrequest 队列中移除

### 10.3 信号机制

信号在内核中的路径：

```
cause_sig(proc_nr, sig_nr):
  rp = proc_addr(proc_nr)
  sig_mgr = priv(rp)->s_sig_mgr         // 查找信号管理器
  if rp == sig_mgr:                      // 自己是自己的信号管理器
    sigaddset(&priv(rp)->s_sig_pending, sig_nr)
    send_sig(rp->p_endpoint, SIGKSIGSM)  // 通知自己
  else:
    sigaddset(&rp->p_pending, sig_nr)    // 标记待处理
    RTS_SET(rp, RTS_SIGNALED | RTS_SIG_PENDING)
    send_sig(sig_mgr, SIGKSIG)           // 通知信号管理器
```

信号相关的系统调用：
- `SYS_KILL` → `do_kill`：发送信号
- `SYS_GETKSIG` → `do_getksig`：信号管理器获取待处理信号
- `SYS_ENDKSIG` → `do_endksig`：信号管理器处理完毕
- `SYS_SIGSEND` → `do_sigsend`：启动 POSIX 信号处理
- `SYS_SIGRETURN` → `do_sigreturn`：信号处理返回

### 10.4 文档定位

进程生命周期是系统调用的**应用实例**——`do_fork`/`do_exit`/`do_clear` 都是 call_vec 中的条目。但 fork 的特殊性（涉及 endpoint generation、proc 结构拷贝、RTS 标志设置）使其值得单独讨论。

建议在系统调用路由文档之后，作为"系统调用家族"的一部分。

---

## 11. SMP 运行时：多核的并发模型

### 11.1 BKL（Big Kernel Lock）

Minix3 的 SMP 模型极其简单：**大内核锁**。同一时刻只有一个 CPU 可以执行内核代码。

```
smp.c:
  SPINLOCK_DEFINE(big_kernel_lock)

  // 内核入口：
  BKL_LOCK()    // 获取锁
  // ... 执行内核操作 ...
  BKL_UNLOCK()  // 释放锁
```

BKL 在以下路径中被**释放**（形成并发窗口）：
- 时钟中断处理（`arch_clock.c`）
- APIC IPI 处理（`smp.c`）
- `wait_for_APs_to_finish_booting()`（boot 阶段）

### 11.2 CPU-local 变量

每个 CPU 有独立的：
- `run_q_head[NR_SCHED_QUEUES]` / `run_q_tail[NR_SCHED_QUEUES]` — 调度队列
- `proc_ptr` — 当前运行的进程
- `bill_ptr` — 当前被记账的进程
- `idle_proc` — IDLE 进程结构
- `fpu_owner` — 当前 FPU 状态的拥有者
- `ptproc` — 当前页表所属进程
- `cpu_is_idle` — CPU 是否处于 idle 状态

CPU-local 变量通过 `get_cpulocal_var()` / `get_cpu_var()` 访问。在 x86-64 上通常通过 GS/FS 段基址或 per-CPU 数据区实现。

### 11.3 IPI（处理器间中断）

IPI 用于跨 CPU 通信：

| IPI 类型 | 用途 | 处理函数 |
|---------|------|---------|
| Schedule IPI | 唤醒 idle CPU 调度新进程 | `smp_ipi_sched_handler()` |
| Sync IPI | 停止进程 / VMINHIBIT / 保存上下文 | `smp_sched_handler()` |
| TLB flush IPI | 跨 CPU 刷新 TLB | — |
| Halt IPI | 停止 AP（shutdown 时） | `smp_ipi_halt_handler()` |

`smp_schedule_sync()` 是同步 IPI——发送者等待目标 CPU 确认完成。在等待期间，发送者释放 BKL 并处理自己收到的 IPI（避免死锁）。

### 11.4 AP Boot 流程

```
BSP:
  smp_init()
    → 检测 CPU 数量（ACPI MADT / MP Table）
    → start_all_aps()
      → 发送 INIT IPI 给所有 AP
      → 发送 STARTUP IPI（指定 trampoline 地址）
      → 等待 ap_cpus_booted == ncpus - 1

AP:
  trampoline 代码（实模式 → 保护模式 → 长模式）
    → arch_mp_start_percpu()
      → 初始化 CPU-local 变量
      → 初始化 Local APIC
      → cpu_set_flag(cpu, CPU_IS_READY)
      → ap_boot_finished(cpu)
      → switch_address_space(VM)
      → BKL_LOCK() → 进入调度循环
```

### 11.5 文档定位

SMP 是内核运行时的**并发模型**。它影响所有其他子系统——调度队列是 per-CPU 的，BKL 在 IPC/系统调用中持有，IPI 是跨 CPU 调度的手段。

SMP 应该在所有单核子系统之后讨论——因为读者需要先理解单核的调度/IPC/系统调用，才能理解 SMP 如何扩展它们。

---

## 12. 辅助子系统

### 12.1 时钟定时器

除了时钟中断驱动的调度外，Minix3 还有三类定时器：

| 类型 | 结构 | 用途 |
|------|------|------|
| **内核定时器** | `minix_timer_t` + `clock_timers` 链表 | 内核内部超时（如 shutdown 定时器） |
| **进程虚拟定时器** | `p_virt_left` / `p_prof_left` | ITIMER_VIRTUAL / ITIMER_PROF |
| **同步闹钟** | `priv->s_alarm_timer` | `SYS_SETALARM` — 系统服务的定时唤醒 |

### 12.2 调试基础设施

`debug.c`（~563 行）提供内核调试输出：
- `direct_print()` / `direct_cls()` — 直接写 VGA 缓冲区（不经过 VFS）
- `ser_dump_proc()` — 串口输出进程表状态
- `kprintf()` — 内核格式化输出

### 12.3 ACPI 与看门狗

- `acpi.c`（~410 行）：ACPI 表解析（MADT 用于 SMP 检测）、电源管理
- `watchdog.c`：硬件看门狗——检测内核死锁（如果时钟中断停止，看门狗触发重启）

### 12.4 文档定位

这些辅助子系统应该放在最后——它们是锦上添花，不影响核心叙事线。

---

## 13. 文档编号方案

### 13.1 设计原则

1. **延续 kboot 编号**：01-06 已完成，07 从 kboot 的最后一篇开始
2. **叙事顺序**：每篇文档的"前置"指向上一篇，形成链式依赖
3. **一本账原则**：每个 C 函数只属于一篇文档
4. **粒度平衡**：每篇文档覆盖 ~300-700 行 C 源码（Ch1-4 约 300-600 行文档）

### 13.2 完整编号表

| 编号 | 文件名 | 覆盖内容 | C 源码 | 前置 |
|------|--------|---------|--------|------|
| 01 | `01-boot-shim-bootstrap.md` | boot-shim → 分页开启 | pre_init.c + pg_utils.c | — |
| 02 | `02-higher-half-kernel.md` | 链接脚本 + ELF 加载 + 高地址跳转 | head.S + 链接脚本 | 01 |
| 03 | `03-kmain-cstart.md` | kmain 入口 + prot_init | main.c:115-147 + protect.c:321-367 | 02 |
| 04 | `04-clock-interrupt-init.md` | init_clock + intr_init + arch_init | clock.c:48-74 + i8259.c + arch_system.c | 03 |
| 05 | `05-proc-init-boot-proc.md` | proc_init + arch_boot_proc + 特权分配 | proc.c:119-160 + main.c:157-282 | 04 |
| 06 | `06-cross-space-init.md` | arch_post_init + memory_init | protect.c:370-377 + memory.c:707-717 | 05 |
| **07** | **`07-system-init-boot-finish.md`** | **system_init + add_memmap + bsp_finish_booting → switch_to_user** | **system.c:168-278 + pg_utils.c:86-125 + main.c:38-117** | **06** |
| **08** | **`08-scheduling.md`** | **pick_proc + enqueue/dequeue + switch_to_user 状态机 + idle + 时间片** | **proc.c: 调度部分 ~700 行** | **07** |
| **09** | **`09-interrupt-exception.md`** | **irq_handle + timer_int_handler + exception_handler + pagefault** | **interrupt.c + clock.c:timer_int_handler + exception.c** | **08** |
| **10** | **`10-syscall-dispatch.md`** | **kernel_call + kernel_call_dispatch + call_vec + kernel_call_finish + VMSUSPEND** | **system.c:59-165** | **09** |
| **11** | **`11-sync-ipc.md`** | **do_ipc + mini_send + mini_receive + mini_notify + deadlock + delivermsg** | **proc.c: do_ipc/mini_send/mini_receive/mini_notify ~600 行** | **10** |
| **12** | **`12-cross-space.md`** | **createpde + lin_lin_copy + vm_memset + vm_suspend + virtual_copy_f** | **arch/i386/memory.c + pg_utils.c + proc.c:vm_suspend** | **11** |
| **13** | **`13-privilege.md`** | **struct priv 全字段 + 权限检查 + do_privctl + priv_add_irq/io/mem** | **priv.h + system.c:priv相关** | **11** |
| **14** | **`14-vm-protocol.md`** | **do_vmctl 全部子命令 + VM 启动协商 + VMCTL_SETADDRSPACE + map_kernel** | **system/do_vmctl.c + arch_do_vmctl.c** | **12** |
| **15** | **`15-proc-lifecycle.md`** | **do_fork + do_exit + do_clear + clear_endpoint + 信号机制** | **system/do_fork.c + system.c:exit/clear/sig** | **10** |
| **16** | **`16-async-ipc.md`** | **mini_senda + try_deliver_senda + try_async + try_one + cancel_async** | **proc.c: 异步 IPC 部分 ~400 行** | **11** |
| **17** | **`17-endpoint.md`** | **isokendpt_f + endpoint_lookup + generation 机制** | **proc.c:endpoint部分 + endpoint.h** | **08** |
| **18** | **`18-smp-runtime.md`** | **BKL + CPU-local + IPI + AP boot + smp_schedule_*** | **smp.c + arch_smp.c + apic.c** | **09** |
| **19** | **`19-timer.md`** | **内核定时器 + 虚拟定时器 + 同步闹钟 + do_times/do_setalarm/do_vtimer** | **clock.c + system/do_times.c + do_setalarm.c + do_vtimer.c** | **09** |
| **20** | **`20-copy-syscalls.md`** | **do_vircopy + do_copy + do_safecopy + do_umap + do_vumap + verify_grant** | **system/do_copy.c + do_safecopy.c + do_umap.c + do_vumap.c** | **12** |
| **21** | **`21-misc-syscalls.md`** | **do_irqctl + do_devio + do_memset + do_abort + do_getinfo + do_diagctl + do_trace + 其余** | **system/do_irqctl.c + do_devio.c + do_memset.c + 其余小文件** | **10** |
| **22** | **`22-debug-acpi-watchdog.md`** | **debug.c + acpi.c + watchdog.c + 串口** | **debug.c + acpi.c + watchdog.c + arch_watchdog** | **18** |
| **99** | **`99-global-concepts.md`** | **RTS 标志位表 + misc_flags 表 + priv 标志表 + 常量定义** | **proc.h + const.h + priv.h** | — |

### 13.3 编号说明

**07 是 kboot 的终点**：system_init + add_memmap + bsp_finish_booting。这是 boot 时间线的最后一站。

**08-09 是运行时的起点**：调度和中断是 `switch_to_user()` 循环的核心——调度是循环的决策点，中断是循环的输入驱动力。

**10-11 是内核服务的入口**：系统调用路由和 IPC 是用户态请求内核服务的两条路径。

**12-14 是内存管理**：跨空间操作、VM 协议、特权结构——它们围绕"内核如何安全地访问进程内存"展开。

**15-17 是进程管理**：生命周期、异步 IPC、endpoint——它们是 IPC 和系统调用的应用层。

**18-22 是扩展和补全**：SMP、定时器、拷贝系统调用、杂项系统调用、调试设施。

### 13.4 叙事链路图

```
01 boot-shim ──→ 02 higher-half ──→ 03 kmain-cstart ──→ 04 clock-interrupt
                                                              │
05 proc-init ←────────────────────────────────────────────────┘
  │
06 cross-space-init
  │
07 system-init-boot-finish ──→ switch_to_user()
  │
  ├──→ 08 scheduling ──→ 17 endpoint
  │      │
  │      ├──→ 09 interrupt-exception ──→ 18 smp-runtime
  │      │      │                              │
  │      │      ├──→ 10 syscall-dispatch ──→ 15 proc-lifecycle
  │      │      │      │                     │
  │      │      │      ├──→ 11 sync-ipc ──→ 16 async-ipc
  │      │      │      │      │
  │      │      │      │      ├──→ 12 cross-space ──→ 20 copy-syscalls
  │      │      │      │      │      │
  │      │      │      │      │      └──→ 14 vm-protocol
  │      │      │      │      │
  │      │      │      │      └──→ 13 privilege
  │      │      │      │
  │      │      │      └──→ 21 misc-syscalls
  │      │      │
  │      │      └──→ 19 timer
  │      │
  │      └──→ 22 debug-acpi-watchdog
  │
  └──→ 99 global-concepts（参考）
```

---

## 14. 与 tmp- 文件的关系

| tmp- 文件 | 内容 | 新归属 | 处理方式 |
|-----------|------|--------|---------|
| `tmp-02-page-table-kernel.md` | createpde / lin_lin_copy / vm_memset | → 12-cross-space | 重写，保持 Ch1-2 忠实 C 源码 |
| `tmp-04-protection.md` | prot_init (GDT/IDT/TSS) | → 已被 03 覆盖 | 删除或移入 reference/ |
| `tmp-05-exception-interrupt.md` | 异常/中断处理 | → 09-interrupt-exception | 重写，按叙事顺序组织 |
| `tmp-06-proc-struct.md` | struct proc 全字段 | → 05 已覆盖 boot 部分，运行时部分分散到 08/11/15 | 拆分到对应文档 |
| `tmp-07-scheduling.md` | 调度 | → 08-scheduling | 重写，强调 switch_to_user 状态机 |
| `tmp-08-endpoint.md` | endpoint | → 17-endpoint | 基本保持，调整编号 |
| `tmp-09-sync-ipc.md` | 同步 IPC | → 11-sync-ipc | 重写，强调在 switch_to_user 中的位置 |
| `tmp-10-async-ipc.md` | 异步 IPC | → 16-async-ipc | 基本保持，调整编号 |
| `tmp-11-privilege.md` | struct priv | → 13-privilege | 重写，与 IPC/syscall 权限检查结合 |
| `tmp-12-syscall-dispatch.md` | 系统调用路由 | → 10-syscall-dispatch | 重写，强调 call_vec 和 VMSUSPEND |
| `tmp-13-syscall-memory.md` | sys_vmctl / sys_vm_map | → 14-vm-protocol | 重写，包含完整 VMCTL 子命令 |
| `tmp-14-syscall-fork-exec.md` | fork/exec | → 15-proc-lifecycle | 重写，包含 exit/clear/signal |
| `tmp-15-syscall-exit-signal.md` | exit/signal | → 15-proc-lifecycle | 合并到 15 |
| `tmp-16-timer.md` | 时钟/定时器 | → 19-timer | 重写，区分 boot 初始化和运行时定时器 |
| `tmp-17-main-init.md` | kmain 全流程 | → 已被 03-07 覆盖 | 删除或移入 reference/ |
| `tmp-18-smp.md` | SMP | → 18-smp-runtime | 重写，区分 boot 初始化和运行时并发 |
| `tmp-19-debug-serial.md` | 调试 | → 22-debug-acpi-watchdog | 基本保持 |
| `tmp-20-acpi-watchdog.md` | ACPI/看门狗 | → 22-debug-acpi-watchdog | 合并到 22 |
| `tmp-21-unported-symbols.md` | 未移植符号 | → 各文档按需覆盖 | 取消独立文档 |

---

## 15. 每篇文档的内部结构约定

为保证叙事连贯性，每篇文档应遵循以下结构：

### 15.1 开头：在 switch_to_user 循环中的位置

每篇文档的第一节应该回答：**"本文覆盖的机制，在 switch_to_user() 循环中的哪个位置被触发？"**

例如：
- 08-scheduling：`switch_to_user()` 的第一步——检查 proc_ptr 可运行性
- 09-interrupt-exception：`restore_user_context()` 之后——用户态执行中被中断
- 10-syscall-dispatch：`sys_call` 汇编入口——用户态主动请求内核服务
- 11-sync-ipc：`do_ipc()` 被 `sys_call` 调用——IPC 是系统调用的一种

### 15.2 Ch1-4 的标准分层

- **Ch1（概念）**：这个机制是什么、为什么需要它、它在运行时的位置
- **Ch2（C 源码）**：Minix3 C 中的实现——逐函数分析，标注文件:行号
- **Ch3（设计决策）**：Rust 重写中的设计选择——哪些保留、哪些演进、为什么
- **Ch4（实现）**：Rust 代码结构——trait 定义、数据结构、关键函数签名

### 15.3 结尾：引出下一篇

每篇文档的最后一节应该回答：**"这个机制引出了什么下一个问题？"**

例如：
- 08-scheduling 结尾："调度器选出了下一个进程，但 CPU 怎么知道何时该切换？→ 09-interrupt-exception"
- 09-interrupt-exception 结尾："中断让内核能响应硬件事件，但用户态如何主动请求内核服务？→ 10-syscall-dispatch"

---

## 16. 实现优先级

| 优先级 | 任务 | 依赖 |
|--------|------|------|
| **P0** | 撰写 07-system-init-boot-finish | 06 完成 |
| **P0** | 撰写 08-scheduling | 07 完成 |
| **P0** | 撰写 09-interrupt-exception | 08 完成 |
| **P0** | 撰写 10-syscall-dispatch | 09 完成 |
| **P0** | 撰写 11-sync-ipc | 10 完成 |
| **P1** | 撰写 12-cross-space | 11 完成 |
| **P1** | 撰写 13-privilege | 11 完成 |
| **P1** | 撰写 14-vm-protocol | 12 完成 |
| **P1** | 撰写 15-proc-lifecycle | 10 完成 |
| **P2** | 撰写 16-22 | 15 完成后依次进行 |
| **P2** | 更新 00-kernel-overview | 全部完成 |
| **P2** | 清理 tmp- 文件 | 全部完成 |

---

## 17. 与 02-stage-vm / 01-stage-pm 的交叉引用

| 本文档概念 | VM 侧文档 | PM 侧文档 |
|-----------|----------|----------|
| 页错误转发（mini_send to VM） | 15-pagefault.md | — |
| VMCTL_SETADDRSPACE | 26-vm-init-main.md §4.3 | — |
| VMCTL_VMINHIBIT_CLEAR | 26-vm-init-main.md | — |
| map_kernel() 建立 direct map | 07-pagetable-ops.md | — |
| SYS_FORK 内核侧 vs PM 侧 | — | fork 相关文档 |
| SYS_EXIT / SYS_CLEAR 协调 | — | exit 相关文档 |
| 信号机制（cause_sig → sig_mgr） | — | signal 相关文档 |
| safecopy / grant table | — | — |

---

## 18. 自检清单

- [x] **叙事连续性**：从 switch_to_user 出发，每个子系统都有明确的"在循环中的位置"
- [x] **因果链**：每篇文档的前置和后续都有明确的因果关系
- [x] **C 源码覆盖**：proc.c / system.c / clock.c / interrupt.c / smp.c / exception.c 全部覆盖
- [x] **无跳跃**：读者不需要"先学 X 再回头补 Y"——所有内容为 switch_to_user 循环服务
- [x] **tmp- 文件处理**：每个 tmp- 文件都有明确的新归属
- [x] **交叉引用**：与 02-stage-vm 的交互点已标注
- [x] **64 位演进**：Direct Map 对跨空间操作的影响已说明

---

*本文档是 03-stage-kernel 运行时部分的规划。它不替代具体文档的内容——每篇文档的 Ch1-4 需要在实际撰写时展开。*
