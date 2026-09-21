# 进程管理详细文档

## 目录

1. [kernel/proc.h](#一-kernelproch)
2. [RTS_PREEMPTED 深度分析](#二-rts_preempted-深度分析)
3. [sched_proc 函数详解](#三-sched_proc-函数详解)
4. [priv.h 总结](#四-privh-总结)
5. [cpulocals.h/c 总结](#五-cpulocalshc-总结)
6. [kernel/const.h 总结](#六-kernelconsth-总结)
7. [kernel/type.h 总结](#七-kerneltypeh-总结)

---

# 一、kernel/proc.h

**文件位置**: `/minix3/minix/kernel/proc.h`

**作用**: 定义进程控制块结构体 `struct proc` 和运行时标志位

**核心概念**: MINIX 的进程控制块包含寄存器快照、调度信息、IPC 状态、VM 请求等

## 结构体字段分类

### 核心执行状态

| 字段 | 类型 | 说明 |
|------|------|------|
| `p_reg` | `struct stackframe_s` | 寄存器快照 |
| `p_seg` | `struct segframe` | 内存段描述符 |
| `p_nr` | `proc_nr_t` | 进程槽位编号 |
| `p_priv` | `struct priv *` | 特权结构指针 |
| `p_rts_flags` | `volatile u32_t` | **运行标志（0=可运行）** |
| `p_misc_flags` | `volatile u32_t` | 杂项标志 |

### 调度相关

| 字段 | 类型 | 说明 |
|------|------|------|
| `p_priority` | `char` | 当前优先级（1字节）|
| `p_cpu_time_left` | `u64_t` | 剩余 CPU 时间配额 |
| `p_quantum_size_ms` | `unsigned` | 时间片大小（FIXME: 可能移除）|
| `p_scheduler` | `struct proc *` | 调度器进程指针 |
| `p_cpu` | `unsigned` | 当前运行的 CPU |

### 统计信息

| 字段 | 类型 | 说明 |
|------|------|------|
| `p_accounting` | `struct` | 入队时间、等待时间、IPC 次数等 |
| `p_user_time` | `clock_t` | 用户态 CPU 时间 |
| `p_sys_time` | `clock_t` | 系统态 CPU 时间 |
| `p_cycles` | `u64_t` | 总 CPU 周期数 |

### IPC 核心字段（最重要！）

| 字段 | 类型 | 说明 |
|------|------|------|
| `p_nextready` | `struct proc *` | 就绪队列链表下一项 |
| `p_caller_q` | `struct proc *` | 发送者等待队列头 |
| `p_q_link` | `struct proc *` | 发送队列链表下一项 |
| `p_getfrom_e` | `endpoint_t` | 想从谁那接收？|
| `p_sendto_e` | `endpoint_t` | 想发给谁？|
| `p_sendmsg` | `message` | 发送消息缓冲区（64字节）|
| `p_delivermsg` | `message` | 接收消息缓冲区（64字节）|
| `p_delivermsg_vir` | `vir_bytes` | 进程指定的接收地址 |

### 其他字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `p_name[PROC_NAME_LEN]` | `char[]` | 进程名称（固定长度）|
| `p_endpoint` | `endpoint_t` | 端点号（带版本号）|
| `p_pending` | `sigset_t` | 待处理信号位图 |
| `p_vmrequest` | `struct` | VM 请求挂起状态 |
| `p_defer` | `struct {r1,r2,r3}` | 延迟 IPC 执行 |
| `p_magic` | `int` | 魔数（验证指针有效性）|

## 运行时标志位（RTS Flags）

**核心规则**: `p_rts_flags == 0` 表示进程可运行

### 基础阻塞标志

| 标志 | 值 | 含义 |
|------|------|------|
| `RTS_SLOT_FREE` | 0x01 | 进程槽空闲 |
| `RTS_PROC_STOP` | 0x02 | 进程被停止 |
| `RTS_SENDING` | 0x04 | 阻塞在发送 |
| `RTS_RECEIVING` | 0x08 | 阻塞在接收 |

### 信号相关

| 标志 | 值 | 含义 |
|------|------|------|
| `RTS_SIGNALED` | 0x10 | 内核信号到达 |
| `RTS_SIG_PENDING` | 0x20 | 信号处理中 |
| `RTS_P_STOP` | 0x40 | 被调试跟踪 |
| `RTS_NO_PRIV` | 0x80 | fork 后阻止特权继承 |

### VM 相关

| 标志 | 值 | 含义 |
|------|------|------|
| `RTS_VMINHIBIT` | 0x200 | 等待 VM 设置页表 |
| `RTS_PAGEFAULT` | 0x400 | 有未处理缺页 |
| `RTS_VMREQUEST` | 0x800 | 发起 VM 请求 |
| `RTS_BOOTINHIBIT` | 0x10000 | 等待 VM 初始化 |

### 调度相关

| 标志 | 值 | 含义 |
|------|------|------|
| `RTS_PREEMPTED` | 0x4000 | 被抢占（放回头部）|
| `RTS_NO_QUANTUM` | 0x8000 | 时间片用完（放回尾部）|

## Misc 标志位（MF Flags）

**特点**: 不影响调度，只是标记状态

| 标志 | 值 | 含义 |
|------|------|------|
| `MF_REPLY_PEND` | 0x001 | 等待回复消息 |
| `MF_VIRT_TIMER` | 0x002 | 虚拟定时器运行中 |
| `MF_DELIVERMSG` | 0x040 | 提前复制消息 |
| `MF_SIG_DELAY` | 0x080 | 发送完成后再发信号 |
| `MF_FPU_INITIALIZED` | 0x1000 | FPU 已使用需保存 |
| `MF_FLUSH_TLB` | 0x10000 | SMP 下需刷新 TLB |

## 关键宏

### 进程状态判断

```c
#define proc_is_runnable(p)  ((p)->p_rts_flags == 0)
#define P_BLOCKEDON(p)       // 返回阻塞目标
```

### 标志操作

```c
#define RTS_SET(rp, f)       // 设置标志，如阻塞则移出队列
#define RTS_UNSET(rp, f)     // 清除标志，如可运行则加入队列
```

### 进程类型判断

```c
#define iskernelp(p)   ((p) < BEG_USER_ADDR)  // 系统进程
#define isuserp(p)     ((p) >= BEG_USER_ADDR) // 用户进程
#define isemptyp(p)   ((p)->p_rts_flags == RTS_SLOT_FREE) // 空闲
```

## 设计亮点

1. **内嵌消息缓冲区**: `p_sendmsg` 和 `p_delivermsg` 直接在进程结构中，避免堆分配
2. **端点号版本号**: `p_endpoint` 包含版本号，防止旧进程消息被新进程接收
3. **零锁宏设计**: RTS_SET/UNSET 依赖上层调用者加锁，代码简洁
4. **条件编译**: `CONFIG_SMP` 下才包含 CPU 亲和性字段
5. **防御性编程**: `p_magic` 魔数验证指针有效性

## IPC 队列结构图

```
进程 B 的 p_caller_q 队列:
p_caller_q
    ↓
┌────────┐    p_q_link    ┌────────┐
│ 进程 A │ ────────────▶ │ 进程 C │ ──▶ NULL
└────────┘                └────────┘
 (A 想发给 B)            (C 想发给 B)
```

**一句话概括**: proc.h 是 MINIX 进程管理的核心，定义了进程控制块的所有状态，包括 IPC 队列、调度信息、VM 请求等。

---

# 二、RTS_PREEMPTED 深度分析

## 问题背景

在 `kernel/proc.c` 的 `switch_to_user()` 函数中，存在一段令人困惑的代码：

```c
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
```

**核心矛盾**：
- `runnable` 定义：`p->p_rts_flags == 0`
- `RTS_PREEMPTED` 是一个 RTS flag
- `RTS_SET(rp, flag)` 会自动 dequeue
- 因此：被设置 `RTS_PREEMPTED` 的进程**不在队列中**
- `pick_proc()` 只从队列中选择进程
- `proc_ptr` 指向当前进程

**问题**：为什么 `switch_to_user()` 能看到 `proc_is_preempted(p) == true`？

## 关键代码路径分析

### RTS_PREEMPTED 在哪里被设置？

**位置 1**：`kernel/proc.c:1630-1639`（enqueue 函数内）

```c
void enqueue(register struct proc *rp)  // rp 是新就绪的进程 B
{
    // ... 将 rp 加入队列 ...
    
    if (cpuid == rp->p_cpu) {
        struct proc * p;
        p = get_cpulocal_var(proc_ptr);  // p 是当前运行的进程 A
        assert(p);
        if((p->p_priority > rp->p_priority) &&  // A 优先级低于 B
           (priv(p)->s_flags & PREEMPTIBLE))    // A 可抢占
            RTS_SET(p, RTS_PREEMPTED); /* calls dequeue() */
    }
}
```

**位置 2**：`kernel/smp.c:202`（SMP IPI 处理）

```c
void smp_ipi_sched_handler(void)
{
    struct proc * curr;
    ipi_ack();
    curr = get_cpulocal_var(proc_ptr);
    if (curr->p_endpoint != IDLE) {
        RTS_SET(curr, RTS_PREEMPTED);
    }
}
```

### 设置后是否一定发生 dequeue？

**是的！** 查看 `RTS_SET` 宏定义：

```c
#define RTS_SET(rp, f)                          \
    do {                                        \
        const int rts = (rp)->p_rts_flags;      \
        (rp)->p_rts_flags |= (f);               \
        if(rts_f_is_runnable(rts) && !proc_is_runnable(rp)) { \
            dequeue(rp);                        \
        }                                       \
    } while(0)
```

**逻辑**：
- 如果进程从 `runnable`（rts == 0）变为 `not runnable`（rts != 0）
- 自动调用 `dequeue(rp)`

### proc_ptr 在哪些位置被赋值？

| 位置 | 赋值 | 时机 |
|------|------|------|
| `main.c:57` | `idle_proc` | 内核初始化 |
| `main.c:261` | `rp` | 启动进程初始化（仅当 proc_ptr 为空）|
| `proc.c:186` | `idle_proc` | idle() 函数内 |
| `proc.c:343` | `p` | **switch_to_user() 最后，pick_proc() 之后** |
| `arch_smp.c:243` | `idle_proc` | SMP 启动 |

**关键发现**：`proc_ptr` 在 `switch_to_user()` **最后**才更新！

## 严格执行路径分析

### 场景：进程 A 正在运行，高优先级进程 B 变为就绪

```
┌─────────────────────────────────────────────────────────────┐
│           执行路径时间线                                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  T1: 进程 A 在用户态运行                                    │
│      → proc_ptr = A                                        │
│      → A 在运行队列中                                      │
│                                                             │
│  T2: 中断/系统调用发生                                      │
│      → 进入内核态                                          │
│      → proc_ptr 仍指向 A                                   │
│                                                             │
│  T3: 某处理导致 B 变为就绪                                  │
│      → 调用 enqueue(B)                                     │
│      → B 加入队列                                          │
│      → 检查：B 优先级 > A 优先级？                         │
│      → 是！调用 RTS_SET(A, RTS_PREEMPTED)                │
│      → A 被设置 RTS_PREEMPTED (p_rts_flags |= 0x4000)    │
│      → A 从队列中 dequeue()                                │
│      → **但 A 仍在内核态执行！proc_ptr 仍指向 A！**       │
│                                                             │
│  T4: 中断处理完成                                           │
│      → 调用 switch_to_user()                              │
│      → p = proc_ptr = A                                    │
│      → proc_is_runnable(A) = false（有 RTS_PREEMPTED）   │
│      → goto not_runnable_pick_new                         │
│                                                             │
│  T5: not_runnable_pick_new:                                 │
│      → proc_is_preempted(A) = true                        │
│      → 清除 RTS_PREEMPTED                                  │
│      → A 变为 runnable (p_rts_flags == 0)                │
│      → enqueue_head(A) 或 enqueue(A)                      │
│                                                             │
│  T6: pick_proc()                                            │
│      → 从队列中选择优先级最高的进程                        │
│      → B 优先级更高，选中 B                               │
│      → proc_ptr = B                                        │
│      → B 被调度执行                                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## 回答核心问题

### (A) 被 RTS_PREEMPTED 的进程是否在 run queue 中？

**不在！** 设置 `RTS_PREEMPTED` 时会自动 `dequeue()`。

### (B) 在什么执行路径下 switch_to_user() 能看到 RTS_PREEMPTED 的 p？

**当进程 A 在内核态执行时被标记为抢占**：
1. A 进入内核（中断/系统调用）
2. 处理过程中 B 变为就绪
3. `enqueue(B)` 发现 B 优先级更高
4. 设置 `RTS_SET(A, RTS_PREEMPTED)`，A 被 dequeue
5. **但 A 仍在执行内核代码**
6. 处理完成，调用 `switch_to_user()`

---

# 三、sched_proc 函数详解

**文件位置**: `minix3/minix/kernel/system.c:642-701`

## 3.1 核心语义

**一句话概括：**
> `sched_proc` 的作用是**原子性地修改进程的调度属性**。

**三个步骤：**
```
1. 如果进程在 runqueue 中 → 先摘掉
2. 修改属性（priority, quantum, cpu, niced）
3. 再放回 runqueue（可能换 CPU）
```

**复杂性来自 SMP 的跨 CPU 处理。**

---

## 3.2 函数签名

```c
int sched_proc(struct proc *p, int priority, int quantum, int cpu, int niced)
```

| 参数 | 含义 | 特殊值 |
|-----|------|--------|
| `p` | 目标进程 | - |
| `priority` | 新优先级 | `-1` 不修改 |
| `quantum` | 新时间片（毫秒） | `-1` 不修改 |
| `cpu` | 目标 CPU（SMP） | `-1` 不修改 |
| `niced` | nice 标志 | 0/非0 |

**返回值：** `OK` / `EINVAL` / `EBADCPU`

---

## 3.3 RTS_SET / RTS_UNSET 核心语义

| 宏 | 语义 | 触发条件 |
|---|------|---------|
| `RTS_SET(p, f)` | 让进程暂时不可调度 | 从 runqueue 移除 |
| `RTS_UNSET(p, f)` | 恢复可调度 | 按**当前** `p_cpu` + `priority` 入队 |

**关键点：** `RTS_UNSET` 不是"恢复原状态"，而是"用新配置重新入队"。

```c
#define RTS_SET(rp, f)                          \
do {                                            \
    const int rts = (rp)->p_rts_flags;          \
    (rp)->p_rts_flags |= (f);                   \
    if(rts_f_is_runnable(rts) &&                \
       !proc_is_runnable(rp)) {                 \
        dequeue(rp);                            \
    }                                           \
} while(0)

#define RTS_UNSET(rp, f)                        \
do {                                            \
    int rts;                                    \
    rts = (rp)->p_rts_flags;                    \
    (rp)->p_rts_flags &= ~(f);                  \
    if(!rts_f_is_runnable(rts) &&               \
       proc_is_runnable(rp)) {                  \
        enqueue(rp);                            \
    }                                           \
} while(0)
```

---

## 3.4 执行流程

### 情况一：不需要跨 CPU 迁移（常见）

```
proc_is_runnable(p) == true
    │
    └─► RTS_SET(RTS_NO_QUANTUM) → 出队

修改属性

RTS_UNSET(RTS_NO_QUANTUM) → 入队（使用新属性）
```

### 情况二：需要跨 CPU 迁移（SMP）

**条件：** 进程在 CPU0，`sched_proc` 在 CPU1 执行，目标 CPU 是 CPU2

```
smp_schedule_migrate_proc(p, cpu)
    │
    ├─► 发送 IPI 到 CPU0
    ├─► CPU0 停止进程，设置 RTS_PROC_STOP
    ├─► 设置 p->p_cpu = dest_cpu
    └─► RTS_UNSET(RTS_PROC_STOP) → 入队到新 CPU

返回后继续 sched_proc：
    │
    ├─► RTS_SET(RTS_NO_QUANTUM) → 出队
    ├─► 修改属性
    └─► RTS_UNSET(RTS_NO_QUANTUM) → 入队（使用新属性）
```

---

## 3.5 smp_schedule_migrate_proc 的本质

**它的目标不是：**
- ❌ "帮你完成调度"
- ❌ "帮你 enqueue"

**而是：**
- ✔ "把执行流停住，交还给调度系统"

---

## 3.6 BKL 与 IPI 的交互

**问题：** CPU1 等待 CPU0 完成 IPI，但 CPU0 需要 BKL 才能执行 IPI handler。

**解决：** `smp_schedule_sync` 内部释放 BKL：

```c
BKL_UNLOCK();
while (sched_ipi_data[cpu].flags != 0) {
    // 等待目标 CPU 完成
}
BKL_LOCK();
```

---

## 3.7 总结

| 方面 | 内容 |
|------|------|
| **核心设计** | RTS_SET → 出队 → 修改 → RTS_UNSET → 入队 |
| **SMP 迁移本质** | 通过 IPI 让目标 CPU 自己停止进程 |
| **两次 RTS_SET** | 第二次几乎不会执行，历史遗留 |
| **Minix3 SMP 特点** | BKL + IPI + 同步等待，正确但保守 |

---

# 四、priv.h 总结

> **文件位置**: `kernel/priv.h`
> **核心功能**: 进程特权结构体定义、能力隔离、权限管理

## 4.1 文件概述

priv.h 定义了 Minix3 的进程特权结构体，是微内核安全模型的核心：

```
┌─────────────────────────────────────────────────────────────────┐
│                    priv 结构体的核心职责                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ 1. 能力隔离：                                                    │
│    - 哪些进程可以发送 IPC 给谁                                   │
│    - 哪些系统调用可以调用                                        │
│    - 哪些 I/O 端口可以访问                                       │
│    - 哪些内存区域可以访问                                        │
│                                                                 │
│ 2. 权限分离：                                                    │
│    - 系统进程：每个进程有独立的 priv 结构体                       │
│    - 用户进程：共享一个 priv 结构体                              │
│                                                                 │
│ 3. 事件待处理：                                                  │
│    - 待处理的通知（s_notify_pending）                           │
│    - 待处理的信号（s_sig_pending）                              │
│    - 待处理的中断（s_int_pending）                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 4.2 核心数据结构

### 4.2.1 priv 结构体

```c
struct priv {
  /* 进程关联 */
  proc_nr_t s_proc_nr;          /* 关联的进程号 */
  sys_id_t s_id;                /* 系统结构索引 */
  short s_flags;                /* PREEMPTIBLE, BILLABLE 等 */
  int s_init_flags;             /* 初始化标志 */

  /* 异步发送 */
  vir_bytes s_asyntab;          /* 异步发送表地址 */
  size_t s_asynsize;            /* 表大小 */
  endpoint_t s_asynendpoint;    /* 表所属端点 */

  /* IPC 权限 */
  short s_trap_mask;            /* 允许的系统调用陷阱 */
  sys_map_t s_ipc_to;           /* 允许的 IPC 目标进程位图 */
  bitchunk_t s_k_call_mask[SYS_CALL_MASK_SIZE]; /* 允许的内核调用 */

  /* 信号管理 */
  endpoint_t s_sig_mgr;         /* 信号管理器 */
  endpoint_t s_bak_sig_mgr;     /* 备份信号管理器 */

  /* 待处理事件 */
  sys_map_t s_notify_pending;   /* 待处理通知位图 */
  sys_map_t s_asyn_pending;     /* 待处理异步消息位图 */
  irq_id_t s_int_pending;       /* 待处理硬件中断 */
  sigset_t s_sig_pending;       /* 待处理信号 */
  ipc_filter_t *s_ipcf;         /* IPC 过滤器 */

  /* 定时器 */
  minix_timer_t s_alarm_timer;  /* 同步闹钟定时器 */
  reg_t *s_stack_guard;         /* 栈保护字 */

  /* I/O 权限 */
  int s_nr_io_range;            /* I/O 端口范围数 */
  struct io_range s_io_tab[NR_IO_RANGE];

  /* 内存权限 */
  int s_nr_mem_range;           /* 内存范围数 */
  struct minix_mem_range s_mem_tab[NR_MEM_RANGE];

  /* IRQ 权限 */
  int s_nr_irq;                 /* IRQ 线数 */
  int s_irq_tab[NR_IRQ];

  /* Grant 表 */
  vir_bytes s_grant_table;      /* Grant 表地址 */
  int s_grant_entries;          /* 条目数 */
};
```

### 4.2.2 字段分类

| 分类 | 字段 | 作用 |
|------|------|------|
| **进程关联** | s_proc_nr, s_id, s_flags | 标识进程和特权级别 |
| **IPC 权限** | s_trap_mask, s_ipc_to, s_k_call_mask | 控制可以调用什么、发送给谁 |
| **待处理事件** | s_notify_pending, s_sig_pending, s_int_pending | 延迟处理的事件 |
| **I/O 权限** | s_io_tab, s_nr_io_range | 允许访问的 I/O 端口 |
| **内存权限** | s_mem_tab, s_nr_mem_range | 允许访问的内存区域 |
| **IRQ 权限** | s_irq_tab, s_nr_irq | 允许处理的中断线 |

## 4.3 特权标志

```c
/* s_flags 的可能值 */
#define PREEMPTIBLE   0x01    /* 可被抢占 */
#define BILLABLE      0x02    /* 计费（时间记在它头上） */
#define SYS_PROC      0x04    /* 系统进程 */
#define PRIV_PROC     0x08    /* 特权进程 */
```

**标志组合示例**：

| 进程类型 | 标志 | 说明 |
|---------|------|------|
| 用户进程 | 无 | 共享默认 priv，无特权 |
| PM | SYS_PROC \| PRIV_PROC | 可以管理其他进程 |
| VM | SYS_PROC \| PRIV_PROC | 可以管理内存 |
| 时钟任务 | SYS_PROC \| PREEMPTIBLE | 系统进程，可抢占 |
| IDLE | SYS_PROC | 系统进程，不可抢占 |

## 4.4 IPC 权限模型

### 4.4.1 目标进程位图

```c
sys_map_t s_ipc_to;  /* 允许发送 IPC 的目标进程 */

/* 检查权限的宏 */
#define may_send_to(rp, nr) (get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr)))
```

```
s_ipc_to 位图结构：
┌─────────────────────────────────────────────────────────────────┐
│ 位图中的每一位代表一个进程                                       │
│                                                                 │
│ 位 0: PM        → 可以发送 IPC 给 PM                           │
│ 位 1: VM        → 可以发送 IPC 给 VM                           │
│ 位 2: VFS       → 可以发送 IPC 给 VFS                          │
│ ...                                                             │
│ 位 N: 进程 N    → 可以发送 IPC 给进程 N                        │
│                                                                 │
│ 设置位 = 允许发送                                               │
│ 清除位 = 禁止发送                                               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 4.4.2 系统调用掩码

```c
short s_trap_mask;  /* 允许的系统调用陷阱 */
bitchunk_t s_k_call_mask[SYS_CALL_MASK_SIZE];  /* 允许的内核调用 */
```

**两层检查**：

```
用户进程发起系统调用：
    │
    ▼
第一层：s_trap_mask
    - 检查是否允许使用该陷阱门
    - 例如：SEND, RECEIVE, NOTIFY
    │
    ▼
第二层：s_k_call_mask
    - 检查是否允许调用特定内核函数
    - 例如：sys_irqctl, sys_vmctl
    │
    ▼
执行或拒绝
```

## 4.5 待处理事件机制

```
┌─────────────────────────────────────────────────────────────────┐
│                    待处理事件的工作流程                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ 场景：进程 A 向进程 B 发送通知，但 B 正在运行                   │
│                                                                 │
│ 1. A 调用 mini_notify(B)                                        │
│    - B 正在运行，无法立即接收                                   │
│                                                                 │
│ 2. 设置待处理位                                                 │
│    - priv(B)->s_notify_pending |= (1 << A)                     │
│    - 记录"A 发送了通知给 B"                                     │
│                                                                 │
│ 3. B 调用 receive()                                             │
│    - 检查 s_notify_pending                                      │
│    - 发现有待处理通知                                           │
│    - 立即返回通知                                               │
│                                                                 │
│ 4. 清除待处理位                                                 │
│    - priv(B)->s_notify_pending &= ~(1 << A)                    │
│                                                                 │
│ 优点：                                                          │
│   - 发送方不阻塞                                                │
│   - 接收方不会丢失消息                                          │
│   - 异步通知机制                                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 4.6 与 IPC 的关系

```
┌─────────────────────────────────────────────────────────────────┐
│                    IPC 权限检查流程                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ mini_send(caller, target, msg):                                 │
│     │                                                           │
│     ▼                                                           │
│ 检查 s_ipc_to 位图                                              │
│     │                                                           │
│     ├─→ 不允许 → 返回 EPERM                                    │
│     │                                                           │
│     ▼                                                           │
│ 检查 IPC 过滤器 (s_ipcf)                                        │
│     │                                                           │
│     ├─→ 被过滤 → 返回 EPERM                                    │
│     │                                                           │
│     ▼                                                           │
│ 执行发送                                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 4.7 要点速查表

| 字段 | 作用 | 类型 |
|------|------|------|
| `s_flags` | 特权标志 | 位掩码 |
| `s_ipc_to` | IPC 目标权限 | 位图 |
| `s_k_call_mask` | 内核调用权限 | 位图数组 |
| `s_notify_pending` | 待处理通知 | 位图 |
| `s_io_tab` | I/O 端口权限 | 数组 |
| `s_mem_tab` | 内存范围权限 | 数组 |
| `s_irq_tab` | IRQ 线权限 | 数组 |

## 4.8 灾难预演

### 如果 s_ipc_to 检查被绕过
- 进程可以向任意进程发送 IPC
- 可能导致权限提升攻击
- 系统安全模型失效

### 如果 s_notify_pending 丢失
- 异步通知丢失
- 进程可能永远等待
- 系统死锁

### 如果 s_io_tab 权限过大
- 进程可以访问任意 I/O 端口
- 可能破坏硬件状态
- 系统崩溃

## 4.9 互动自测

1. 为什么系统进程和用户进程使用不同的 priv 结构体？
2. s_ipc_to 位图如何实现 IPC 权限控制？
3. 待处理事件机制如何避免消息丢失？
4. Rust 重构时如何利用 IOMMU 替代软件 I/O 权限检查？
5. Per-CPU 数据如何优化待处理事件的访问？

---

# 五、cpulocals.h/c 总结

> **文件位置**: `kernel/cpulocals.h`, `kernel/cpulocals.c`
> **核心功能**: CPU 本地变量定义与实现，为 SMP 系统提供每 CPU 独立的数据存储

## 5.1 文件概述

### cpulocals.h

定义 CPU 本地变量的结构体和访问宏：

```c
/* SMP 模式 */
#define CPULOCAL_ARRAY  [CONFIG_MAX_CPUS]

/* 单 CPU 模式 */
#define CPULOCAL_ARRAY

/* 访问宏 */
#define get_cpu_var(cpu, name)      __cpu_local_vars[cpu].name
#define get_cpulocal_var(name)      get_cpu_var(cpuid, name)
```

### cpulocals.c

定义 CPU 本地变量数组（仅 3 行）：

```c
#include "kernel/kernel.h"

struct __cpu_local_vars __cpu_local_vars CPULOCAL_ARRAY;
```

## 5.2 核心数据结构

### __cpu_local_vars 结构体

```c
struct __cpu_local_vars {
    /* 进程调度信息 */
    struct proc *proc_ptr;    /* 当前运行的进程指针 */
    struct proc *bill_ptr;    /* 计费进程指针 */
    struct proc idle_proc;    /* 空闲进程存根 */

    /* 缺页处理标志 */
    int pagefault_handled;    /* 是否正在处理缺页 */

    /* 页表所有者 */
    struct proc *ptproc;      /* 当前加载的页表所属进程 */

    /* CPU 私有运行队列 */
    struct proc *run_q_head[NR_SCHED_QUEUES]; /* 就绪队列头 */
    struct proc *run_q_tail[NR_SCHED_QUEUES]; /* 就绪队列尾 */
    int cpu_is_idle;          /* CPU 是否空闲 */

    int idle_interrupted;     /* 空闲是否被中断 */

    /* 时间统计 */
    u64_t tsc_ctr_switch;     /* 时间记账切换时间戳 */
    u64_t cpu_last_tsc;       /* 上次 TSC 值 */
    u64_t cpu_last_idle;      /* 上次空闲时间 */

    /* FPU 管理 */
    char fpu_presence;        /* CPU 是否有 FPU */
    struct proc *fpu_owner;   /* FPU 当前所有者 */
};
```

### 字段分类

| 分类 | 字段 | 作用 |
|------|------|------|
| **进程调度** | proc_ptr, bill_ptr, idle_proc | 当前进程、计费进程、空闲进程 |
| **缺页处理** | pagefault_handled | 检测递归缺页 |
| **页表管理** | ptproc | 当前加载的页表所属进程 |
| **调度队列** | run_q_head, run_q_tail, cpu_is_idle | CPU 私有运行队列 |
| **时间统计** | tsc_ctr_switch, cpu_last_tsc, cpu_last_idle | 时间记账 |
| **FPU 管理** | fpu_presence, fpu_owner | FPU 惰性切换 |

## 5.3 SMP vs 单 CPU

### 内存布局对比

**SMP 模式**：

```
__cpu_local_vars[CONFIG_MAX_CPUS] 数组
┌─────────────────────────────────────────────────────────┐
│ CPU 0 的本地变量 │ CPU 1 的本地变量 │ ... │ CPU N-1 的本地变量 │
├───────────────────┼───────────────────┼─────┼───────────────────┤
│ proc_ptr          │ proc_ptr          │ ... │ proc_ptr          │
│ bill_ptr          │ bill_ptr          │ ... │ bill_ptr          │
│ idle_proc         │ idle_proc         │ ... │ idle_proc         │
│ ...               │ ...               │ ... │ ...               │
└───────────────────┴───────────────────┴─────┴───────────────────┘
```

**单 CPU 模式**：

```
__cpu_local_vars (单个结构体)
┌─────────────────────────────────┐
│ proc_ptr                        │
│ bill_ptr                        │
│ idle_proc                       │
│ ...                             │
└─────────────────────────────────┘
```

### 访问方式对比

| 模式 | 宏展开 | 说明 |
|------|--------|------|
| SMP | `__cpu_local_vars[cpuid].proc_ptr` | 通过 cpuid 索引数组 |
| 单 CPU | `__cpu_local_vars.proc_ptr` | 直接访问结构体成员 |

## 5.4 关键字段详解

### proc_ptr vs bill_ptr

```
场景: 用户进程调用系统服务
1. 用户进程进入内核
2. 内核切换到系统服务（如 VFS）
3. 时钟中断发生
4. 时间应该记在用户进程账上

proc_ptr = VFS (实际运行)
bill_ptr = 用户进程 (计费对象)
```

**为什么需要分离？**
- 公平记账：系统调用时间记给调用者
- 性能分析：准确统计用户态/内核态时间

### 运行队列

```
run_q_head[NR_SCHED_QUEUES]:
┌─────────────────────────────────┐
│ [0]: 最高优先级队列头            │ → proc → proc → NULL
├─────────────────────────────────┤
│ [1]: 次高优先级队列头            │ → proc → proc → proc → NULL
├─────────────────────────────────┤
│ ...                             │
├─────────────────────────────────┤
│ [NR_SCHED_QUEUES-1]: 最低优先级  │ → NULL (空)
└─────────────────────────────────┘
```

**为什么每 CPU 独立运行队列？**
- 无锁调度：CPU 只操作自己的队列
- 缓存友好：数据在本地 CPU 缓存
- 可扩展：增加 CPU 不增加竞争

### FPU 惰性切换

```
进程 A 使用 FPU:
1. fpu_owner = A
2. A 的 FPU 状态在硬件中

切换到进程 B (不使用 FPU):
1. 不保存 A 的 FPU 状态
2. fpu_owner 仍为 A

进程 B 尝试使用 FPU:
1. 触发 #NM 异常
2. 检查 fpu_owner
3. 保存 A 的 FPU 状态
4. 恢复 B 的 FPU 状态（或初始化）
5. fpu_owner = B
```

**每 CPU FPU 的好处**：
- SMP 系统：CPU0 和 CPU1 可以同时使用 FPU
- 无全局锁

## 5.5 与 IPC 的关系

### Per-CPU 待处理事件

```
┌─────────────────────────────────────────────────────────────────┐
│                    CPU 本地变量与 IPC                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ priv.h 中的待处理事件：                                          │
│   - s_notify_pending: 待处理通知位图                            │
│   - s_asyn_pending: 待处理异步消息位图                          │
│   - s_int_pending: 待处理硬件中断                               │
│                                                                 │
│ 这些字段可以优化为 Per-CPU 数据：                                │
│   - 避免跨核缓存一致性开销                                       │
│   - 无锁访问                                                    │
│   - 更好的扩展性                                                │
│                                                                 │
│ 访问方式：                                                       │
│   get_cpulocal_var(pending_notifications)                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### IPC 快速路径

```c
/* 检查当前进程是否有待处理通知 */
if (get_cpulocal_var(proc_ptr)->p_priv->s_notify_pending) {
    /* 处理待处理通知 */
}
```

## 5.6 要点速查表

| 字段 | 作用 | 访问宏 |
|------|------|--------|
| `proc_ptr` | 当前进程 | get_cpulocal_var(proc_ptr) |
| `bill_ptr` | 计费进程 | get_cpulocal_var(bill_ptr) |
| `run_q_head` | 运行队列头 | get_cpulocal_var(run_q_head) |
| `fpu_owner` | FPU 所有者 | get_cpulocal_var(fpu_owner) |
| `cpu_is_idle` | CPU 空闲标志 | get_cpulocal_var(cpu_is_idle) |

## 5.7 灾难预演

### 如果缓存行争用（FIXME 未修复）

```
SMP 系统，两个 CPU 同时修改各自的 proc_ptr

后果：
1. CPU 0 修改 __cpu_local_vars[0].proc_ptr
2. 缓存行失效，CPU 1 必须重新加载
3. 频繁的缓存失效导致性能大幅下降
```

### 如果 proc_ptr 指向已释放的进程

```
后果：
1. 访问无效内存
2. 内核崩溃
3. 数据损坏
```

### 如果 pagefault_handled 未正确清除

```
后果：
1. 后续的页故障检测到 pagefault_handled = 1
2. 认为是递归页故障，panic
3. 系统无法恢复
```

## 5.8 互动自测

1. 为什么 `proc_ptr` 和 `bill_ptr` 可能不同？
2. 为什么运行队列要每 CPU 独立？
3. FPU 惰性保存如何工作？
4. 为什么单核版本仍然提供 `get_cpu_var` 宏？
5. 如何在 Rust 中实现 Per-CPU 变量的类型安全访问？


---

# 六、kernel/const.h 总结

**文件位置**: `minix3/minix/kernel/const.h`

**作用**: 定义内核通用宏和常量，是内核基础设施的一部分

## 6.1 端点号验证宏

```c
#define isokendpt(e,p) isokendpt_d((e),(p),0)
#define okendpt(e,p)   isokendpt_d((e),(p),1)
```

| 宏 | 功能 | 失败时 |
|----|------|--------|
| `isokendpt(e,p)` | 验证端点号，返回成功/失败 | 返回 0 |
| `okendpt(e,p)` | 验证端点号，失败时 panic | 内核崩溃 |

**参数**：
- `e`: 端点号 (endpoint_t)
- `p`: 输出参数，存储转换后的进程槽位号

**使用场景**：
```c
// 安全检查 - 失败返回错误
if (!isokendpt(endpoint, &proc_nr)) {
    return EINVAL;
}

// 断言检查 - 失败崩溃（用于不可恢复的错误）
if (!okendpt(endpoint, &proc_nr)) {
    // 已经 panic 了
}
```

## 6.2 虚拟拷贝方向常量

```c
#define _SRC_  0  // 源方向
#define _DST_  1  // 目标方向
```

**用途**：`virtual_copy()` 函数中标识拷贝方向

```c
// 示例：从进程 A 拷贝到进程 B
virtual_copy(src_addr, src_proc, _SRC_,
             dst_addr, dst_proc, _DST_, size);
```

## 6.3 系统位图操作宏

```c
#define get_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) & (1 << CHUNK_OFFSET(bit) ))
#define set_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) |= (1 << CHUNK_OFFSET(bit) ))
#define unset_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) &= ~(1 << CHUNK_OFFSET(bit) ))
```

**用途**：操作 `sys_map_t` 类型的位图

| 操作 | 功能 |
|------|------|
| `get_sys_bit(map, bit)` | 读取指定位 |
| `set_sys_bit(map, bit)` | 设置指定位为 1 |
| `unset_sys_bit(map, bit)` | 清除指定位为 0 |

**典型应用**：
- 进程槽位分配（`priv.h` 中的 `s_id_bit_map`）
- IPC 权限位图
- 信号位图

## 6.4 用户空间地址限制

```c
#define USR_DATATOP         0xF0000000  // 用户数据段上限
#define USR_STACKTOP        USR_DATATOP // 用户栈上限
#define USR_DATATOP_COMPACT USR_DATATOP // 紧凑模式数据上限
#define USR_STACKTOP_COMPACT 0x50000000 // 紧凑模式栈上限
```

**内存布局**（32 位系统）：
```
0x00000000 ┌────────────────────────────┐
           │    用户代码段/数据段        │
           │                            │
0x50000000 ├────────────────────────────┤ ← USR_STACKTOP_COMPACT
           │    （紧凑模式栈区域）        │
           │                            │
0xE0000000 ├────────────────────────────┤ ← _MINIX_MAGIC 模式
           │    内核空间                 │
0xF0000000 ├────────────────────────────┤ ← USR_DATATOP
           │    内核空间                 │
0xFFFFFFFF └────────────────────────────┘
```

## 6.5 其他常量

```c
#define END_OF_KMESS  0  // 内核消息结束标记
```

---

## 6.6 现代 64 位硬件演进

### 地址空间变化

| 方面 | 32 位 MINIX | 64 位现代系统 |
|------|-------------|---------------|
| **用户空间上限** | 0xF0000000 (3.75GB) | 0x00007FFFFFFFFFFF (128TB) |
| **内核空间起始** | 0xF0000000 | 0xFFFF800000000000 |
| **地址宽度** | 32 位 | 48 位（当前实现）/ 57 位（5 级页表） |

### 64 位内存布局

```
0x0000000000000000 ┌────────────────────────────┐
                   │    用户空间（低半区）        │
                   │    128TB (47 位寻址)        │
0x00007FFFFFFFFFFF ├────────────────────────────┤ ← 用户空间上限
                   │    非规范地址区（非法）      │
0xFFFF800000000000 ├────────────────────────────┤ ← 内核空间起始
                   │    内核空间（高半区）        │
                   │    128TB                    │
0xFFFFFFFFFFFFFFFF └────────────────────────────┘
```

---

## 6.7 Rust 重构建议

### 1. 端点号验证的类型安全

**C 语言问题**：宏没有类型检查，容易传错参数。

```c
// C: 编译通过，运行时错误
isokendpt("wrong_type", &proc_nr);  // 字符串当端点号？
```

**Rust 重构**：使用泛型约束和 Result。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint(i32);

#[derive(Clone, Copy, Debug)]
pub struct ProcNr(u16);

impl Endpoint {
    pub fn validate(&self) -> Result<ProcNr, EndpointError> {
        let proc_nr = self.0 & 0xFFFF;
        if proc_nr >= NR_PROCS {
            return Err(EndpointError::InvalidSlot);
        }
        Ok(ProcNr(proc_nr as u16))
    }
    
    pub fn validate_or_panic(&self) -> ProcNr {
        self.validate().expect("Invalid endpoint")
    }
}

// 使用
let proc_nr = endpoint.validate()?;  // 安全验证
let proc_nr = endpoint.validate_or_panic();  // 断言验证
```

### 2. 位图操作的封装

**C 语言问题**：宏操作全局状态，难以追踪。

```c
set_sys_bit(priv->s_id_bit_map, id);  // 哪个位图？什么含义？
```

**Rust 重构**：封装为类型安全的结构体。

```rust
pub struct Bitmap<const N: usize> {
    chunks: [u32; N],
}

impl<const N: usize> Bitmap<N> {
    pub fn get(&self, bit: usize) -> bool {
        let chunk = bit / 32;
        let offset = bit % 32;
        (self.chunks[chunk] & (1 << offset)) != 0
    }
    
    pub fn set(&mut self, bit: usize) {
        let chunk = bit / 32;
        let offset = bit % 32;
        self.chunks[chunk] |= 1 << offset;
    }
    
    pub fn clear(&mut self, bit: usize) {
        let chunk = bit / 32;
        let offset = bit % 32;
        self.chunks[chunk] &= !(1 << offset);
    }
}

// 使用
let mut id_map: Bitmap<8> = Bitmap::new();  // 256 位
id_map.set(proc_id);
if id_map.get(proc_id) {
    // ...
}
```

### 3. 用户地址限制的常量

**C 语言问题**：硬编码魔数，难以理解。

```c
if (addr >= 0xF0000000) {  // 这是什么？
    return EFAULT;
}
```

**Rust 重构**：使用命名常量和类型。

```rust
/// 用户空间地址范围
pub struct UserAddressSpace {
    pub data_top: usize,
    pub stack_top: usize,
}

#[cfg(target_arch = "x86_64")]
impl UserAddressSpace {
    pub const fn new() -> Self {
        Self {
            data_top: 0x0000_7FFF_FFFF_FFFF,  // 128TB
            stack_top: 0x0000_7FFF_FFFF_FFFF,
        }
    }
    
    pub fn is_user_address(&self, addr: usize) -> bool {
        addr < self.data_top
    }
}

// 使用
const USER_SPACE: UserAddressSpace = UserAddressSpace::new();
if !USER_SPACE.is_user_address(addr) {
    return Err(MemoryError::InvalidUserAddress);
}
```

### 4. 拷贝方向的枚举

**C 语言问题**：0/1 魔数，语义不清。

```c
virtual_copy(src, src_proc, 0, dst, dst_proc, 1, size);  // 0? 1?
```

**Rust 重构**：使用枚举。

```rust
#[derive(Clone, Copy, Debug)]
pub enum CopyDirection {
    Source,
    Destination,
}

fn virtual_copy(
    src_addr: *const u8,
    src_proc: &Process,
    src_dir: CopyDirection,
    dst_addr: *mut u8,
    dst_proc: &Process,
    dst_dir: CopyDirection,
    size: usize,
) -> Result<(), CopyError> {
    // ...
}

// 使用
virtual_copy(src, &src_proc, CopyDirection::Source,
             dst, &dst_proc, CopyDirection::Destination, size)?;
```

---

## 6.8 要点速查表

| 常量/宏 | 值 | 用途 |
|---------|-----|------|
| `isokendpt(e,p)` | - | 安全验证端点号 |
| `okendpt(e,p)` | - | 断言验证端点号 |
| `_SRC_` | 0 | 拷贝源方向 |
| `_DST_` | 1 | 拷贝目标方向 |
| `USR_DATATOP` | 0xF0000000 | 用户数据段上限（32位） |
| `END_OF_KMESS` | 0 | 内核消息结束标记 |

---

## 6.9 灾难预演

### 如果 isokendpt 返回错误但未检查

```
后果：
1. proc_nr 包含垃圾值
2. 访问 proc[garbage] 越界
3. 内核崩溃或数据损坏
```

### 如果 USR_DATATOP 设置错误

```
后果：
1. 用户程序可以访问内核内存
2. 安全漏洞
3. 系统不稳定
```

### 如果位图操作越界

```
后果：
1. 写入超出数组边界
2. 损坏相邻内存
3. 难以调试的随机崩溃
```

---

## 6.10 互动自测

1. `isokendpt` 和 `okendpt` 的区别是什么？何时使用哪个？
2. 为什么用户空间上限是 0xF0000000 而不是 0xFFFFFFFF？
3. 64 位系统上用户空间上限应该是多少？
4. 如何在 Rust 中实现类型安全的位图？
5. `_SRC_` 和 `_DST_` 为什么定义为 0 和 1？

---

# 七、kernel/type.h 总结

**文件位置**: `minix3/minix/kernel/type.h`

**作用**: 定义内核核心类型：进程号、系统 ID 位图、中断钩子结构体

## 7.1 进程号类型

```c
typedef int proc_nr_t;    // 进程表槽位号
typedef short sys_id_t;   // 系统进程索引
```

**内存布局**：
```
proc_nr_t (int, 4 bytes)
┌────────────────────────────────────────┐
│  符号位  │      槽位编号 (31位)         │
│   1 bit  │         31 bits             │
└────────────────────────────────────────┘

sys_id_t (short, 2 bytes)
┌─────────────────────────┐
│  符号位  │  索引 (15位) │
│   1 bit  │    15 bits   │
└─────────────────────────┘
```

**使用场景**：
- `proc_nr_t`: 数组索引，访问 `proc[NR_PROCS]`
- `sys_id_t`: 系统服务标识，范围更小

## 7.2 系统位图类型

```c
typedef struct {
    bitchunk_t chunk[BITMAP_CHUNKS(NR_SYS_PROCS)];
} sys_map_t;
```

**内存布局**：
```
sys_map_t (大小取决于 NR_SYS_PROCS)
┌─────────────────────────────────────────┐
│  chunk[0]  │  chunk[1]  │  ...  │chunk[N]│
│  32 bits   │  32 bits   │       │ 32 bits│
└─────────────────────────────────────────┘

每个 chunk 是一个 bitchunk_t (u32)
每位代表一个系统进程的状态
```

**典型应用**：
- 进程特权位图（`s_id_bit_map`）
- IPC 权限检查
- 资源分配追踪

## 7.3 中断钩子结构体

```c
typedef unsigned long irq_policy_t;  // 中断策略位掩码
typedef unsigned long irq_id_t;      // 中断 ID

typedef struct irq_hook {
    struct irq_hook *next;           // 链表下一项
    int (*handler)(struct irq_hook *); // 中断处理函数
    int irq;                         // IRQ 向量号
    int id;                          // 钩子 ID
    endpoint_t proc_nr_e;            // 关联进程端点
    irq_id_t notify_id;              // 通知 ID
    irq_policy_t policy;             // 策略位掩码
} irq_hook_t;

typedef int (*irq_handler_t)(struct irq_hook *);
```

**内存布局**（32 位系统）：
```
irq_hook_t (28 bytes)
┌─────────────────────────────────────────────────────┐
│ next (4B) │ handler (4B) │ irq (4B) │ id (4B)      │
├─────────────────────────────────────────────────────┤
│ proc_nr_e (4B) │ notify_id (4B) │ policy (4B)      │
└─────────────────────────────────────────────────────┘
```

**字段详解**：

| 字段 | 类型 | 用途 |
|------|------|------|
| `next` | `irq_hook*` | 共享中断的钩子链表 |
| `handler` | 函数指针 | 中断处理函数 |
| `irq` | `int` | IRQ 号（0-255） |
| `id` | `int` | 钩子唯一标识（位图索引） |
| `proc_nr_e` | `endpoint_t` | 关联的进程 |
| `notify_id` | `irq_id_t` | 返回给进程的通知 ID |
| `policy` | `irq_policy_t` | 中断策略（共享/独占等） |

**共享中断链表示意图**：
```
IRQ 5 共享中断链表
┌──────────┐     ┌──────────┐     ┌──────────┐
│ hook_1   │────►│ hook_2   │────►│ hook_3   │────► NULL
│ handler A│     │ handler B│     │ handler C│
│ id = 1   │     │ id = 2   │     │ id = 4   │
└──────────┘     └──────────┘     └──────────┘
```

---

## 7.4 现代 64 位硬件演进

### 类型大小变化

| 类型 | 32 位 | 64 位 | 说明 |
|------|-------|-------|------|
| `proc_nr_t` | 4 bytes | 4 bytes | 保持不变（槽位号足够） |
| `sys_id_t` | 2 bytes | 2 bytes | 保持不变 |
| `irq_policy_t` | 4 bytes | 8 bytes | 扩展为 64 位 |
| `irq_id_t` | 4 bytes | 8 bytes | 扩展为 64 位 |
| 指针 (`next`) | 4 bytes | 8 bytes | 64 位地址空间 |

### 64 位 irq_hook_t 布局

```
irq_hook_t (48 bytes on x86_64)
┌──────────────────────────────────────────────────────────┐
│ next (8B) │ handler (8B) │ irq (4B) │ id (4B) │ padding  │
├──────────────────────────────────────────────────────────┤
│ proc_nr_e (4B) │ pad (4B) │ notify_id (8B) │ policy (8B)│
└──────────────────────────────────────────────────────────┘
```

---

## 7.5 Rust 重构建议

### 1. 类型安全的进程号

**C 语言问题**：`proc_nr_t` 是裸 `int`，可能越界访问。

```c
proc_nr_t nr = 1000;  // 超出 NR_PROCS？
process = &proc[nr];  // 越界访问？
```

**Rust 重构**：使用 newtype 模式。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcNr(u16);

impl ProcNr {
    pub const MAX: u16 = NR_PROCS as u16;
    
    pub fn new(nr: u16) -> Result<Self, ProcNrError> {
        if nr < Self::MAX {
            Ok(Self(nr))
        } else {
            Err(ProcNrError::OutOfRange(nr))
        }
    }
    
    pub fn get(&self) -> usize {
        self.0 as usize
    }
}

// 使用
let nr = ProcNr::new(5)?;
let process = &proc_table[nr.get()];
```

### 2. 位图类型封装

**C 语言问题**：位图操作是宏，无类型安全。

```c
sys_map_t map;
set_sys_bit(map, id);  // 哪个位图？什么含义？
```

**Rust 重构**：泛型位图。

```rust
pub struct SysMap<const N: usize> {
    chunks: [u32; N],
}

impl<const N: usize> SysMap<N> {
    pub fn new() -> Self {
        Self { chunks: [0; N] }
    }
    
    pub fn set(&mut self, id: sys_id_t) {
        let chunk = (id as usize) / 32;
        let bit = (id as usize) % 32;
        self.chunks[chunk] |= 1 << bit;
    }
    
    pub fn clear(&mut self, id: sys_id_t) {
        let chunk = (id as usize) / 32;
        let bit = (id as usize) % 32;
        self.chunks[chunk] &= !(1 << bit);
    }
    
    pub fn test(&self, id: sys_id_t) -> bool {
        let chunk = (id as usize) / 32;
        let bit = (id as usize) % 32;
        (self.chunks[chunk] & (1 << bit)) != 0
    }
}

// 类型别名
pub type SysIdMap = SysMap<{ (NR_SYS_PROCS + 31) / 32 }>;
```

### 3. 中断钩子的安全封装

**C 语言问题**：函数指针和裸指针，无生命周期检查。

```c
struct irq_hook *next;  // 可能悬空
int (*handler)(struct irq_hook *);  // 函数签名不安全
```

**Rust 重构**：使用 trait 和生命周期。

```rust
pub trait IrqHandler: Send {
    fn handle(&self, hook: &IrqHook) -> IrqResult;
}

pub struct IrqHook {
    irq: u8,
    id: u32,
    proc_endpoint: Endpoint,
    notify_id: u64,
    policy: IrqPolicy,
    next: Option<Box<IrqHook>>,  // 链表
}

impl IrqHook {
    pub fn new(irq: u8, id: u32) -> Self {
        Self {
            irq,
            id,
            proc_endpoint: Endpoint::NONE,
            notify_id: 0,
            policy: IrqPolicy::empty(),
            next: None,
        }
    }
    
    pub fn call_handler(&self, handler: &dyn IrqHandler) -> IrqResult {
        handler.handle(self)
    }
}

// 使用
struct MyHandler;
impl IrqHandler for MyHandler {
    fn handle(&self, hook: &IrqHook) -> IrqResult {
        // 处理中断
        IrqResult::Handled
    }
}
```

### 4. 中断策略的位标志

**C 语言问题**：`irq_policy_t` 是裸 `unsigned long`。

```c
#define IRQ_POLICY_SHARED  0x01
policy = IRQ_POLICY_SHARED;  // 魔数
```

**Rust 重构**：使用 bitflags crate。

```rust
bitflags::bitflags! {
    pub struct IrqPolicy: u64 {
        const SHARED    = 0b0000_0001;
        const EXCLUSIVE = 0b0000_0010;
        const REENABLE  = 0b0000_0100;
        const NOTIFY    = 0b0000_1000;
    }
}

// 使用
let policy = IrqPolicy::SHARED | IrqPolicy::NOTIFY;
if policy.contains(IrqPolicy::SHARED) {
    // 共享中断
}
```

### 5. 中断钩子池

**C 语言问题**：全局静态数组，手动管理。

```c
irq_hook_t irq_hooks[NR_IRQ_HOOKS];
```

**Rust 重构**：Arena 分配器。

```rust
pub struct IrqHookPool {
    hooks: Vec<Option<IrqHook>>,
    free_list: VecDeque<usize>,
}

impl IrqHookPool {
    pub fn new(capacity: usize) -> Self {
        let mut free_list = VecDeque::with_capacity(capacity);
        for i in 0..capacity {
            free_list.push_back(i);
        }
        Self {
            hooks: vec![None; capacity],
            free_list,
        }
    }
    
    pub fn allocate(&mut self, irq: u8) -> Option<&mut IrqHook> {
        self.free_list.pop_front().map(|idx| {
            self.hooks[idx] = Some(IrqHook::new(irq, idx as u32));
            self.hooks[idx].as_mut().unwrap()
        })
    }
    
    pub fn deallocate(&mut self, id: u32) {
        if let Some(hook) = &mut self.hooks[id as usize] {
            self.free_list.push_back(id as usize);
        }
        self.hooks[id as usize] = None;
    }
}
```

---

## 7.6 要点速查表

| 类型 | 大小（32位） | 用途 |
|------|-------------|------|
| `proc_nr_t` | 4 bytes | 进程槽位号 |
| `sys_id_t` | 2 bytes | 系统进程索引 |
| `sys_map_t` | 变长 | 系统进程位图 |
| `irq_policy_t` | 4 bytes | 中断策略 |
| `irq_id_t` | 4 bytes | 中断 ID |
| `irq_hook_t` | 28 bytes | 中断钩子结构 |

---

## 7.7 灾难预演

### 如果 proc_nr_t 越界

```
后果：
1. 访问 proc[invalid_nr]
2. 越界读取/写入
3. 内核崩溃或数据损坏
```

### 如果中断钩子链断裂

```
后果：
1. next 指针损坏
2. 共享中断的后续处理程序不被调用
3. 设备无响应
```

### 如果 irq_hook 被提前释放

```
后果：
1. 悬空指针
2. 中断发生时访问已释放内存
3. 随机崩溃
```

---

## 7.8 互动自测

1. `proc_nr_t` 和 `sys_id_t` 有什么区别？为什么需要两种类型？
2. `sys_map_t` 的内存布局是什么？如何计算大小？
3. 共享中断是如何通过 `irq_hook_t` 链表实现的？
4. 如何在 Rust 中实现类型安全的进程号？
5. 64 位系统上 `irq_hook_t` 的大小是多少？为什么需要填充？
