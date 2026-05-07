# 01-proc-struct-basic - 进程结构体基本字段

> 本文档分析 `minix3/minix/kernel/proc.h` 第 1-50 行，讲解进程结构体的基本字段。

---

## 1. 概述

`struct proc` 是 Minix3 内核中最核心的数据结构，它包含了进程运行所需的全部状态信息。每个进程（包括用户进程和内核任务）在内核中都有一个对应的 `struct proc` 实例，这些实例组成进程表（process table）。

进程结构体的设计遵循以下原则：

1. **完整性**：包含进程运行所需的全部信息——寄存器状态、内存映射、调度信息、IPC 状态、统计信息等
2. **快速访问**：关键字段（如 `p_nr`、`p_endpoint`）直接存储，避免间接查找
3. **状态驱动**：通过 `p_rts_flags` 标志位精确控制进程的可运行性
4. **汇编友好**：字段偏移量在 `sconst.h` 中定义，供汇编代码直接访问

### 1.1 文件位置

`proc.h` 位于内核源码树的根目录下：

```
minix3/minix/kernel/
├── proc.h          ← 进程结构体定义（本文档分析）
├── priv.h          ← 特权结构体定义
├── const.h         ← 内核常量
├── type.h          ← 基本类型定义
├── ipc.h           ← IPC 相关定义
├── proc.c          ← 进程管理实现
├── system.c        ← 系统调用框架
├── system/         ← 系统调用实现
│   └── do_fork.c   ← fork 实现
└── arch/           ← 架构相关代码
```

该文件定义了 `struct proc` 进程结构体及其相关宏和常量，是内核进程管理的核心头文件。

### 1.2 进程表概念

进程表是一个静态数组，存储所有进程的 `struct proc` 实例：

```c
EXTERN struct proc proc[NR_TASKS + NR_PROCS];  /* process table */
```

**进程表布局**：

```
索引:    0          NR_TASKS              NR_TASKS + NR_PROCS
         │             │                        │
         ▼             ▼                        ▼
         ├─────────────┼────────────────────────┤
         │ 内核任务区   │      用户进程区         │
         │ (tasks)     │    (user processes)    │
         │ p_nr < 0    │      p_nr >= 0         │
         ├─────────────┼────────────────────────┤
         │ CLOCK, ...  │  PM, VM, VFS, 用户进程  │
         └─────────────┴────────────────────────┘
```

**设计理念**：

1. **静态分配**：进程表大小固定为 `NR_TASKS + NR_PROCS`，编译时确定
2. **分区管理**：前 `NR_TASKS` 个槽位用于内核任务（`p_nr` 为负数），后续槽位用于用户进程（`p_nr` 为非负数）
3. **快速索引**：通过 `proc_addr(n)` 宏直接计算进程地址，无需查找
4. **地址稳定性**：进程表是静态数组，进程结构体地址永不改变

### 1.3 与 fork 的关系

fork 系统调用通过 **整体复制** 进程结构体来创建子进程：

```c
*rpc = *rpp;    /* copy 'proc' struct */
```

这行 C 代码将父进程 `rpp` 的整个 `struct proc` 复制到子进程 `rpc`。复制后，内核再对子进程进行必要的修正：

| 字段 | 处理方式 |
|------|---------|
| `p_endpoint` | 生成新端点（generation 递增） |
| `p_nr` | 恢复为子进程槽位号（被复制覆盖） |
| `p_reg.retreg` | 设为 0（子进程 fork 返回值） |
| `p_user_time`, `p_sys_time` | 清零（不继承父进程时间统计） |
| `p_rts_flags` | 添加 `RTS_NO_QUANTUM`（等待调度） |
| `p_misc_flags` | 清除定时器相关标志 |
| `p_priv` | 特权进程降级为用户特权 |

**核心要点**：fork 的进程结构复制是"先整体复制，再个别修正"的模式，而非逐字段选择性复制。

---

## 2. C 源码分析

本节分析 `struct proc` 的基本字段，这些字段是进程结构体最核心的部分：

```c
struct proc {
  struct stackframe_s p_reg;    /* 进程寄存器，保存在栈帧中 */
  struct segframe p_seg;        /* 段描述符 */
  proc_nr_t p_nr;               /* 进程号（快速访问） */
  struct priv *p_priv;          /* 系统特权结构指针 */
  volatile u32_t p_rts_flags;   /* 运行时状态标志，为零时可运行 */
  volatile u32_t p_misc_flags;  /* 杂项标志（不阻塞进程） */
  // ... 更多字段在后续章节分析
};
```

这六个字段构成了进程结构体的"头部"，在 fork 过程中全部被复制到子进程。

### 2.1 结构体定义开头

`struct proc` 的定义包含重要的头文件依赖和注释：

```c
#ifndef PROC_H
#define PROC_H

#include <minix/const.h>
#include <sys/cdefs.h>

#ifndef __ASSEMBLY__

/* Here is the declaration of the process table.  It contains all process
 * data, including registers, flags, scheduling priority, memory map, 
 * accounting, message passing (IPC) information, and so on. 
 *
 * Many assembly code routines reference fields in it.  The offsets to these
 * fields are defined in the assembler include file sconst.h. When changing
 * struct proc, be sure to change sconst.h to match.
 */
#include <minix/com.h>
#include <minix/portio.h>
#include "const.h"
#include "priv.h"
```

**关键点**：

1. `#ifndef __ASSEMBLY__` 确保汇编代码可以包含此头文件获取常量定义，但跳过结构体定义
2. 注释明确指出：修改 `struct proc` 时必须同步更新 `sconst.h` 中的偏移量
3. 依赖 `priv.h` 表明进程与特权结构紧密关联

### 2.2 p_reg 字段

`p_reg` 是 `struct stackframe_s` 类型，保存进程被中断时的所有寄存器状态：

```c
struct stackframe_s {
    u16_t gs;                     /* last item pushed by save */
    u16_t fs;                     /*  ^ */
    u16_t es;                     /*  | */
    u16_t ds;                     /*  | */
    reg_t di;                     /* di through cx are not accessed in C */
    reg_t si;                     /* order is to match pusha/popa */
    reg_t fp;                     /* bp */
    reg_t bx;                     /*  | */
    reg_t dx;                     /*  | */
    reg_t cx;                     /*  | */
    reg_t retreg;                 /* ax and above are all pushed by save */
    reg_t pc;                     /*  ^  last item pushed by interrupt */
    reg_t cs;                     /*  | */
    reg_t psw;                    /*  | */
    reg_t sp;                     /*  | */
    reg_t ss;                     /* these are pushed by CPU during interrupt */
};
```

**作用**：当进程被中断或进行系统调用时，内核将 CPU 寄存器保存到 `p_reg`；当进程恢复运行时，从 `p_reg` 恢复寄存器。这是实现上下文切换的核心机制。

#### 2.2.1 寄存器保存

当进程被中断或执行系统调用时，汇编宏 `SAVE_PROCESS_CTX` 将寄存器保存到 `p_reg`：

```asm
#define SAVE_PROCESS_CTX(displ, trapcode) \
    cld                         /* 设置方向标志为已知状态 */  \
    push    %ebp                                            \
    movl    (CURR_PROC_PTR + 4 + displ)(%esp), %ebp         \
    SAVE_GP_REGS(%ebp)          /* 保存通用寄存器 */          \
    movl    $trapcode, P_KERN_TRAP_STYLE(%ebp)              \
    pop     %esi                /* 恢复原 %ebp 并保存 */      \
    mov     %esi, BPREG(%ebp)                               \
    RESTORE_KERNEL_SEGS         /* 恢复内核段寄存器 */        \
    SAVE_TRAP_CTX(displ, %ebp, %esi)  /* 保存陷阱上下文 */
```

**保存顺序**：

1. CPU 自动压入 `ss`, `sp`, `psw`, `cs`, `pc`（中断/异常时）
2. `SAVE_GP_REGS` 保存 `eax`, `ecx`, `edx`, `ebx`, `esi`, `edi`
3. `SAVE_TRAP_CTX` 从栈中提取并保存 `pc`, `cs`, `psw`, `sp`

#### 2.2.2 上下文切换

上下文切换时，`p_reg` 是恢复进程执行的核心数据结构。`switch_to_user()` 函数选择下一个运行的进程后，调用 `arch_finish_switch_to_user()` 完成恢复：

```c
struct proc * arch_finish_switch_to_user(void)
{
    struct proc * p;
    p = get_cpulocal_var(proc_ptr);
    
    /* 确保中断标志位开启 */
    p->p_reg.psw |= IF_MASK;
    
    /* 设置单步调试标志 */
    if(p->p_misc_flags & MF_STEP)
        p->p_reg.psw |= TRACEBIT;
    else
        p->p_reg.psw &= ~TRACEBIT;
    
    return p;
}
```

**恢复流程**（汇编层面）：

1. 将进程指针 `p` 压入内核栈顶
2. 通过 `iret` 指令从 `p_reg` 恢复 `ss`, `sp`, `psw`, `cs`, `pc`
3. 通用寄存器在返回用户态前从 `p_reg` 恢复

**fork 中的应用**：子进程的 `p_reg.retreg` 被设为 0，因此子进程从 fork 返回时得到返回值 0。

### 2.3 p_seg 字段

`p_seg` 是 `struct segframe` 类型，存储进程的内存管理相关信息：

```c
typedef struct segframe {
    reg_t   p_cr3;            /* 页表根地址（CR3 寄存器值） */
    u32_t  *p_cr3_v;          /* 页表根地址的内核虚拟地址 */
    char   *fpu_state;        /* FPU 状态保存区指针 */
    int     p_kern_trap_style; /* 内核陷阱类型（用于返回方式选择） */
} segframe_t;
```

**作用**：

1. `p_cr3`：进程的页表基址，切换进程时加载到 CR3 寄存器
2. `p_cr3_v`：页表的内核虚拟地址，用于内核访问页表
3. `fpu_state`：FPU/ SIMD 状态保存区，上下文切换时保存/恢复浮点状态
4. `p_kern_trap_style`：记录进程如何进入内核（中断、sysenter、syscall 等），决定返回方式

#### 2.3.1 段描述符

段描述符 `struct segdesc_s` 定义了 x86 保护模式的段属性：

```c
struct segdesc_s {      /* segment descriptor for protected mode */
  u16_t limit_low;      /* 段界限低 16 位 */
  u16_t base_low;       /* 段基址低 16 位 */
  u8_t base_middle;     /* 段基址中 8 位 */
  u8_t access;          /* |P|DL|1|X|E|R|A| 访问权限 */
  u8_t granularity;     /* |G|X|0|A|LIMT| 粒度和界限高位 */
  u8_t base_high;       /* 段基址高 8 位 */
} __attribute__((packed));
```

**作用**：段描述符用于 GDT（全局描述符表）和 LDT（局部描述符表），定义代码段和数据段的基地址、界限和访问权限。在 Minix3 中，每个进程有独立的 LDT，存储在 `p_seg` 相关结构中（具体实现在 `protect.c`）。

**注意**：`struct segframe` 本身不直接包含段描述符，段描述符通过 LDT 机制管理。`p_cr3` 字段用于分页模式下的页表管理。

#### 2.3.2 FPU 状态

FPU 状态通过 `p_seg.fpu_state` 指针管理。每个用户进程有独立的 FPU 状态保存区：

```c
static char fpu_state[NR_PROCS][FPU_XFP_SIZE] __aligned(FPUALIGN);

void arch_proc_reset(struct proc *pr)
{
    assert(pr->p_nr < NR_PROCS);
    if(pr->p_nr >= 0) {
        v = fpu_state[pr->p_nr];  /* 分配 FPU 状态区 */
        memset(v, 0, FPU_XFP_SIZE);
    }
    pr->p_seg.fpu_state = v;
}
```

**保存机制**：

1. `save_fpu(pr)`：在进程切换前保存 FPU 状态到 `pr->p_seg.fpu_state`
2. `restore_fpu(pr)`：在进程恢复运行时从保存区恢复 FPU 状态
3. 惰性保存：只有进程使用过 FPU（`proc_used_fpu(pr)` 为真）才进行保存/恢复

**fork 中的应用**：

```c
save_fpu(rpp);  /* 确保父进程 FPU 状态已保存 */
*rpc = *rpp;    /* 复制进程结构体 */
if(proc_used_fpu(rpp))
    memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);
```

### 2.4 p_nr 字段

`p_nr` 是 `proc_nr_t` 类型，即进程号（process number）：

```c
typedef int proc_nr_t;    /* process table entry number */
```

**作用**：`p_nr` 是进程在进程表中的索引号，用于快速定位进程结构体。

**取值范围**：

| 进程类型 | p_nr 范围 | 说明 |
|---------|----------|------|
| 内核任务 | -NR_TASKS ~ -1 | 负数，如 CLOCK、SYSTEM 等 |
| 用户进程 | 0 ~ NR_PROCS-1 | 非负数，包括 PM、VM、VFS 和普通用户进程 |

**与进程表索引的关系**：

```c
#define proc_addr(n)    (&(proc[NR_TASKS + (n)]))
```

进程表索引 = `NR_TASKS + p_nr`，因此 `p_nr` 可以是负数（内核任务）。

#### 2.4.1 进程号

进程号 `p_nr` 是进程的"槽位标识"，具有以下特性：

1. **唯一性**：每个进程槽位有唯一的 `p_nr`，进程生命周期内不变
2. **可重用**：进程退出后，其槽位可被新进程重用，`p_nr` 也随之重用
3. **与 PID 不同**：`p_nr` 是内核层面的槽位号，PID 是用户层面的进程标识

**示例**：

```
进程表槽位:  [0]    [1]    [2]    [3]    ...    [NR_TASKS]    [NR_TASKS+1]
p_nr:       -4     -3     -2     -1     ...    0             1
进程:       CLOCK  SYSTEM IDLE   KERNEL ...    PM            VM
```

**fork 中的处理**：子进程继承父进程的 `p_nr` 字段（通过 `*rpc = *rpp`），但随后被修正：

```c
rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;  /* 恢复为子进程槽位号 */
```

#### 2.4.2 与 endpoint 的区别

`p_nr` 和 `p_endpoint` 都标识进程，但有以下关键区别：

| 特性 | p_nr (进程号) | p_endpoint (端点) |
|------|--------------|------------------|
| 组成 | 仅槽位号 | 槽位号 + 代数(generation) |
| 唯一性 | 槽位唯一，可重用 | 全局唯一，不可重用 |
| 用途 | 内核内部索引 | IPC 通信标识 |

**端点结构**：

```c
#define _ENDPOINT(g, p) \
    ((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
```

端点 = `(generation << 15) + p_nr`

**代数的作用**：当进程槽位被重用时，代数递增，确保新进程的端点与旧进程不同。这样即使持有旧端点的消息也不会误发给新进程。

**fork 中的处理**：

```c
gen = _ENDPOINT_G(rpc->p_endpoint);  /* 获取当前代数 */
if(++gen >= _ENDPOINT_MAX_GENERATION) gen = 1;
rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);  /* 生成新端点 */
```

### 2.5 p_priv 字段

`p_priv` 指向进程的特权结构 `struct priv`，定义进程的系统权限：

```c
struct priv {
  proc_nr_t s_proc_nr;      /* 关联的进程号 */
  sys_id_t s_id;            /* 系统结构索引 */
  short s_flags;            /* PREEMPTIBLE, BILLABLE 等标志 */
  int s_init_flags;         /* 初始化标志 */
  short s_trap_mask;        /* 允许的系统调用陷阱 */
  sys_map_t s_ipc_to;       /* 允许发送消息的目标进程 */
  bitchunk_t s_k_call_mask[SYS_CALL_MASK_SIZE];  /* 允许的内核调用 */
  // ... 更多字段
};
```

**特权结构的作用**：

1. **权限控制**：定义进程可以执行哪些系统调用、可以向哪些进程发送消息
2. **资源隔离**：系统进程有独立的特权结构，用户进程共享 `USER_PRIV`
3. **中断管理**：记录待处理的中断和通知

#### 2.5.1 特权指针

`p_priv` 是指向特权结构的指针，通过宏快速访问：

```c
#define priv(rp)        ((rp)->p_priv)
#define priv_addr(i)    (ppriv_addr)[(i)]
#define priv_id(rp)     ((rp)->p_priv->s_id)
```

**特权指针的作用**：

1. **快速访问**：通过 `priv(rp)` 宏直接获取进程的特权结构
2. **权限检查**：通过 `may_send_to(rp, nr)` 检查进程是否有权向目标发送消息
3. **区分进程类型**：比较 `p_priv` 与 `priv_addr(USER_PRIV_ID)` 判断是否为系统进程

**特权表结构**：

```c
EXTERN struct priv priv[NR_SYS_PROCS];        /* 系统属性表 */
EXTERN struct priv *ppriv_addr[NR_SYS_PROCS]; /* 直接槽位指针 */
```

系统进程有独立的特权槽位，用户进程共享 `USER_PRIV_ID` 对应的特权结构。

#### 2.5.2 fork 时的特权处理

fork 时，子进程的特权处理遵循"最小权限原则"：

```c
/* If the parent is a privileged process, take away the privileges from the 
 * child process and inhibit it from running by setting the NO_PRIV flag.
 * The caller should explicitly set the new privileges before executing.
 */
if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;
}
```

**处理逻辑**：

| 父进程类型 | 子进程特权 | 子进程状态 |
|-----------|-----------|-----------|
| 用户进程 | 继承 `USER_PRIV` | 正常（无 `RTS_NO_PRIV`） |
| 系统进程 | 降级为 `USER_PRIV` | 设置 `RTS_NO_PRIV`，等待特权设置 |

**原因**：系统进程的特权（如访问特定 I/O 端口、执行特权内核调用）不应自动继承给子进程。PM 需要显式为子进程设置新的特权结构后，子进程才能运行。

### 2.6 p_rts_flags 字段

`p_rts_flags` 是运行时状态标志位，是进程调度的核心控制字段：

```c
volatile u32_t p_rts_flags;  /* process is runnable only if zero */
```

**核心规则**：进程可运行当且仅当 `p_rts_flags == 0`。

**主要标志位**：

| 标志 | 值 | 含义 |
|------|-----|------|
| `RTS_SLOT_FREE` | 0x01 | 进程槽空闲 |
| `RTS_PROC_STOP` | 0x02 | 进程已停止 |
| `RTS_SENDING` | 0x04 | 发送消息阻塞 |
| `RTS_RECEIVING` | 0x08 | 接收消息阻塞 |
| `RTS_NO_PRIV` | 0x80 | 系统进程 fork 后禁止运行 |
| `RTS_VMINHIBIT` | 0x200 | 等待 VM 设置页表 |
| `RTS_NO_QUANTUM` | 0x8000 | 时间片用完 |

**操作宏**：

```c
#define RTS_ISSET(rp, f)  (((rp)->p_rts_flags & (f)) == (f))
#define RTS_SET(rp, f)    do { (rp)->p_rts_flags |= (f); if(rts == 0) dequeue(rp); } while(0)
#define RTS_UNSET(rp, f)  do { rts = (rp)->p_rts_flags; (rp)->p_rts_flags &= ~(f); ... } while(0)
```

#### 2.6.1 运行时状态标志

运行时状态标志控制进程的调度行为。每个标志位代表一种"阻塞原因"，只有所有标志位都清除，进程才能被调度运行。

**标志位分类**：

1. **槽位管理**：`RTS_SLOT_FREE` - 进程槽未使用
2. **进程控制**：`RTS_PROC_STOP`（停止）、`RTS_P_STOP`（被追踪）
3. **IPC 阻塞**：`RTS_SENDING`、`RTS_RECEIVING`
4. **信号处理**：`RTS_SIGNALED`、`RTS_SIG_PENDING`
5. **资源等待**：`RTS_NO_PRIV`（等待特权）、`RTS_VMINHIBIT`（等待 VM）、`RTS_NO_QUANTUM`（等待时间片）

**设计理念**：使用位图而非枚举，允许多个阻塞原因同时存在。例如，进程可能同时在等待 VM 和时间片。

#### 2.6.2 进程可运行条件

进程可运行的唯一条件是 `p_rts_flags == 0`：

```c
#define proc_is_runnable(rp)    ((rp)->p_rts_flags == 0)
#define proc_not_runnable(rp)   ((rp)->p_rts_flags != 0)
```

**调度器行为**：

- `RTS_SET` 设置标志时，若进程原可运行，则从运行队列移除
- `RTS_UNSET` 清除标志时，若进程变为可运行，则加入运行队列

**fork 中的应用**：

```c
rpc->p_rts_flags = RTS_SLOT_FREE;  /* 初始状态：槽位空闲 */
RTS_UNSET(rpc, RTS_SLOT_FREE);     /* 清除空闲标志，加入调度 */
```

### 2.7 p_misc_flags 字段

`p_misc_flags` 是杂项标志位，与 `p_rts_flags` 不同，它**不会阻塞进程运行**：

```c
volatile u32_t p_misc_flags;  /* flags that do not suspend the process */
```

**主要标志位**：

| 标志 | 值 | 含义 |
|------|-----|------|
| `MF_REPLY_PEND` | 0x001 | IPC_REQUEST 的回复待处理 |
| `MF_VIRT_TIMER` | 0x002 | 进程虚拟定时器运行中 |
| `MF_DELIVERMSG` | 0x040 | 运行前需要投递消息 |
| `MF_SIG_DELAY` | 0x080 | 发送完成后发送信号 |
| `MF_FPU_INITIALIZED` | 0x1000 | FPU 已初始化 |
| `MF_FLUSH_TLB` | 0x10000 | 运行前需刷新 TLB |
| `MF_NICED` | 0x100000 | 用户降低了进程优先级 |

**与 p_rts_flags 的区别**：

- `p_rts_flags`：阻塞进程调度
- `p_misc_flags`：记录进程状态，不影响调度

---

## 3. Rust 设计决策

本节讨论如何用 Rust 实现进程结构体的基本字段。Rust 的类型系统和所有权模型为内核开发提供了内存安全保证，但也带来了一些设计挑战。

**核心挑战**：

1. **`volatile` 字段**：C 中的 `volatile u32_t` 在 Rust 中需要使用 `AtomicU32`
2. **指针安全**：`p_priv` 等裸指针需要用引用或智能指针替代；`p_nextready`/`p_caller_q`/`p_q_link` 等 C 裸指针用 `Option<ProcNr>` 索引替代
3. **可变性控制**：进程结构体在多处被访问，需要精细的可变性控制
4. **硬件抽象**：寄存器保存等硬件相关操作需要通过 trait 抽象（当前 `p_reg`/`p_seg` 暂未实现，`p_ext_reg_state` 通过 `ExtRegState` 抽象）

### 3.1 结构体定义

Rust 中 `KProcess` 已在 `os/kernel/src/proc.rs` 中实现，包含基本字段和调度字段：

```rust
use minix_types::{Endpoint, Message, VirBytes};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicI8, Ordering};
use crate::arch::ExtRegState;

/// Process number type (corresponds to C's `proc_nr_t`).
pub type ProcNr = i32;

/// Runtime status flags (wraps atomic operations).
#[derive(Debug)]
pub struct RtsFlags(AtomicU32);

/// Miscellaneous flags (wraps atomic operations).
#[derive(Debug)]
pub struct MiscFlags(AtomicU32);

/// Kernel process structure.
#[derive(Debug)]
pub struct KProcess {
    pub p_nr: ProcNr,
    pub p_endpoint: Endpoint,
    pub p_rts_flags: RtsFlags,
    pub p_misc_flags: MiscFlags,
    pub p_sched: SchedFields,
    pub p_accounting: Accounting,
    pub p_time: TimeStats,
    pub p_cycles: CyclesStats,
    pub p_nextready: Option<ProcNr>,
    pub p_caller_q: Option<ProcNr>,
    pub p_q_link: Option<ProcNr>,
    pub p_getfrom_e: Endpoint,
    pub p_sendto_e: Endpoint,
    pub p_pending: SigSet,
    pub p_name: ProcName,
    pub p_sendmsg: Message,
    pub p_delivermsg: Message,
    pub p_delivermsg_vir: VirBytes,
    pub p_ext_reg_state: ExtRegState,
}
```

**设计要点**：

1. **类型别名**：`ProcNr` 提供语义化的进程号类型
2. **原子类型**：`AtomicU32` 替代 `volatile u32_t`，提供内存顺序保证
3. **封装标志**：`RtsFlags` 和 `MiscFlags` 封装原子操作，提供类型安全的 API
4. **索引替代指针**：`p_nextready`/`p_caller_q`/`p_q_link` 用 `Option<ProcNr>` 替代 C 的 `struct proc *`，避免指针安全问题
5. **硬件抽象**：`p_ext_reg_state` 通过 `ExtRegState` 抽象（定义在 `arch.rs`），`p_reg`/`p_seg` 暂未实现

### 3.2 原子性

C 中的 `volatile` 关键字在 Rust 中应使用原子类型替代：

**C 代码**：
```c
volatile u32_t p_rts_flags;
volatile u32_t p_misc_flags;
```

**Rust 实现**：
```rust
use core::sync::atomic::{AtomicU32, Ordering};

pub struct RtsFlags(AtomicU32);

impl RtsFlags {
    pub fn new(value: u32) -> Self {
        Self(AtomicU32::new(value))
    }

    pub fn load(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    pub fn store(&self, value: u32) {
        self.0.store(value, Ordering::Release);
    }

    pub fn is_runnable(&self) -> bool {
        self.load() == 0
    }

    // TODO: RTS_SET/RTS_UNSET in Minix3 also call dequeue/enqueue when
    // the process transitions between runnable/non-runnable. Once the
    // scheduler is implemented, these methods need scheduling integration.
    pub fn set(&self, flags: u32) {
        self.0.fetch_or(flags, Ordering::AcqRel);
    }

    pub fn clear(&self, flags: u32) {
        self.0.fetch_and(!flags, Ordering::AcqRel);
    }
}
```

**内存顺序选择**：

| 操作 | 顺序 | 原因 |
|------|------|------|
| `load` | `Acquire` | 后续操作依赖加载的值 |
| `store` | `Release` | 确保之前的写操作可见 |
| `fetch_or/and` | `AcqRel` | 同时需要读写语义 |

**待办**：`set`/`clear` 目前只修改标志位，不做调度联动（缺少 `dequeue`/`enqueue` 调用）。调度器实现后需要补全。

### 3.3 生命周期

进程结构体的生命周期与进程本身绑定，但内核中存在多处引用：

**生命周期挑战**：

1. **进程表是静态数组**：进程槽位在编译时确定
2. **多处引用**：调度器、IPC 系统、中断处理都持有进程引用
3. **不可移动**：进程结构体地址在运行期间固定

**当前方案**：进程表尚未实现（TODO）。设计方向是使用 `ProcNr` 索引替代指针，避免 Rust 借用检查器与内核多引用冲突。C 代码中的 `struct proc *p_nextready` 等链表指针用 `Option<ProcNr>` 索引替代。

### 3.4 fork 语义映射

C 的 `*rpc = *rpp` 是整体复制后逐字段修正。Rust 实现必须显式处理每个字段，并正确映射 C 的修正逻辑：

| 字段 | C 修正 | Rust 实现 |
|------|--------|-----------|
| `p_rts_flags` | `RTS_SET(NO_QUANTUM)` + `RTS_UNSET(SIGNALED\|SIG_PENDING\|P_STOP)` | 复制后位运算修正 |
| `p_misc_flags` | `&= ~(VIRT_TIMER\|PROF_TIMER\|SC_TRACE\|SPROF_SEEN\|STEP)` | 复制后位运算修正 |
| `p_time` | `virt_left=0, prof_left=0` | 全部清零 |
| `p_pending` | `sigemptyset()` | `SigSet::empty()` |
| `p_cycles` | `p_cycles=0, p_kcall_cycles=0, p_kipc_cycles=0` | 全部清零 |
| `p_nextready/p_caller_q/p_q_link` | 指针被复制 | `None`（子进程尚未入队） |
| `p_ext_reg_state` | FPU 状态按需复制 | 父进程已初始化则复制 |

---

## 4. 实现

本节给出进程结构体基本字段的 Rust 实现代码。代码位于 `os/kernel/src/proc.rs`。

**实现范围**：

1. 进程号和端点字段
2. 运行时状态标志（`RtsFlags`）+ 全部 RTS 标志常量
3. 杂项标志（`MiscFlags`）+ 全部 MF 标志常量
4. 调度字段（`SchedFields`、`Quantum`、`Priority`）
5. 统计字段（`Accounting`、`TimeStats`、`CyclesStats`）
6. IPC 队列指针（用 `Option<ProcNr>` 索引）
7. 信号集（`SigSet`）
8. 进程名（`ProcName`）
9. 消息缓冲（`Message`、`VirBytes`）
10. 扩展寄存器状态（`ExtRegState`）
11. fork 实现（`fork_from` + `complete_fork_setup`）

**暂不实现**：

- `p_reg`：需要硬件抽象（后续章节）
- `p_seg`：需要硬件抽象（后续章节）
- `p_priv`：需要特权结构体定义（后续章节）
- `p_vmrequest`：VM 请求状态（后续章节）
- `p_dequeued`：调度统计字段（需统一时间类型）

### 4.1 标志常量

```rust
/// Runtime status flags (complete set, matching Minix3's RTS_*).
pub mod rts {
    pub const SLOT_FREE: u32 = 0x01;
    pub const PROC_STOP: u32 = 0x02;
    pub const SENDING: u32 = 0x04;
    pub const RECEIVING: u32 = 0x08;
    pub const SIGNALED: u32 = 0x10;
    pub const SIG_PENDING: u32 = 0x20;
    pub const P_STOP: u32 = 0x40;
    pub const NO_PRIV: u32 = 0x80;
    pub const NO_ENDPOINT: u32 = 0x100;
    pub const VMINHIBIT: u32 = 0x200;
    pub const PAGEFAULT: u32 = 0x400;
    pub const VMREQUEST: u32 = 0x800;
    pub const VMREQTARGET: u32 = 0x1000;
    pub const PREEMPTED: u32 = 0x4000;
    pub const NO_QUANTUM: u32 = 0x8000;
    pub const BOOTINHIBIT: u32 = 0x10000;
}

/// Miscellaneous flags (complete set, matching Minix3's MF_*).
pub mod mf {
    pub const REPLY_PEND: u32 = 0x001;
    pub const VIRT_TIMER: u32 = 0x002;
    pub const PROF_TIMER: u32 = 0x004;
    pub const KCALL_RESUME: u32 = 0x008;
    pub const DELIVERMSG: u32 = 0x040;
    pub const SIG_DELAY: u32 = 0x080;
    pub const SC_ACTIVE: u32 = 0x100;
    pub const SC_DEFER: u32 = 0x200;
    pub const SC_TRACE: u32 = 0x400;
    pub const EXT_REG_INITIALIZED: u32 = 0x1000;  // was MF_FPU_INITIALIZED in C
    // ... more flags
}
```

**Rewrite 决策**：`MF_FPU_INITIALIZED` 重命名为 `MF_EXT_REG_INITIALIZED`，因为现代 64 位架构不使用独立 FPU，浮点/SIMD 操作使用扩展寄存器（x86-64 的 XSAVE、ARM64 的 NEON、RISC-V 的 F/D）。

### 4.2 fork 实现

fork 是进程结构体最重要的操作之一。Rust 实现必须精确映射 Minix3 的语义：

```rust
impl KProcess {
    /// Creates child process from parent (fork).
    ///
    /// Corresponds to `*rpc = *rpp` copy in Minix3's do_fork.c with subsequent field corrections.
    /// Does not copy p_nr and p_endpoint, specified by caller via parameters.
    /// Time stats, accounting info, signal set start from zero, IPC queue pointers cleared.
    pub fn fork_from(parent: &KProcess, child_nr: ProcNr, child_endpoint: Endpoint) -> Self {
        // Copy p_rts_flags then apply fork corrections:
        // RTS_SET(rpc, RTS_NO_QUANTUM) — child not runnable until scheduled
        // RTS_UNSET(rpc, RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP) — no signal inheritance
        let child_rts = {
            let flags = parent.p_rts_flags.load();
            let flags = flags | rts::NO_QUANTUM;
            let flags = flags & !(rts::SIGNALED | rts::SIG_PENDING | rts::P_STOP);
            RtsFlags::new(flags)
        };

        // Copy p_misc_flags then clear timer/trace flags:
        // rpc->p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | MF_SC_TRACE | MF_SPROF_SEEN | MF_STEP)
        let child_mf = {
            let flags = parent.p_misc_flags.load();
            let flags = flags & !(mf::VIRT_TIMER | mf::PROF_TIMER | mf::SC_TRACE | mf::SPROF_SEEN | mf::STEP);
            MiscFlags::new(flags)
        };

        let mut child = Self {
            p_nr: child_nr,
            p_endpoint: child_endpoint,
            p_rts_flags: child_rts,
            p_misc_flags: child_mf,
            p_sched: SchedFields { /* ... */ },
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            p_nextready: None,
            p_caller_q: None,
            p_q_link: None,
            p_getfrom_e: parent.p_getfrom_e,
            p_sendto_e: parent.p_sendto_e,
            p_pending: SigSet::empty(),
            p_name: parent.p_name,
            p_sendmsg: parent.p_sendmsg.clone(),
            p_delivermsg: parent.p_delivermsg.clone(),
            p_delivermsg_vir: parent.p_delivermsg_vir,
            p_ext_reg_state: ExtRegState::new(),
        };

        // Copy extended register state if parent has initialized it
        if parent.p_misc_flags.is_set(mf::EXT_REG_INITIALIZED) {
            child.p_ext_reg_state = parent.p_ext_reg_state.clone();
            child.p_misc_flags.set(mf::EXT_REG_INITIALIZED);
        }

        child
    }
}
```

**与 C 代码的对比**：

| C 代码 | Rust 代码 | 说明 |
|--------|-----------|------|
| `*rpc = *rpp` | 逐字段构造 | Rust 不能整体复制（类型不同） |
| `rpc->p_nr = slot` | `child_nr` 参数 | 调用方提供 |
| `RTS_SET(rpc, RTS_NO_QUANTUM)` | `flags \| rts::NO_QUANTUM` | 在构造时一并处理 |
| `RTS_UNSET(rpc, RTS_SIGNALED\|...)` | `flags & !(rts::SIGNALED\|...)` | 同上 |
| `rpc->p_misc_flags &= ~(...)` | `flags & !(mf::VIRT_TIMER\|...)` | 同上 |
| `sigemptyset(&rpc->p_pending)` | `SigSet::empty()` | 子进程不继承信号 |
| `RTS_SET(rpc, RTS_NO_PRIV)`（if sys proc） | `complete_fork_setup()` | 分离为独立函数 |

### 4.3 特权处理

特权处理从 `fork_from` 中分离为 `complete_fork_setup`，因为特权结构尚未实现：

```rust
/// Completes fork setup with privilege handling and flags.
pub fn complete_fork_setup(child: &mut KProcess, parent_is_sys_proc: bool, flags: u32) {
    if parent_is_sys_proc {
        child.p_rts_flags.set(rts::NO_PRIV);
    }
    if flags & fork_flags::VMINHIBIT != 0 {
        child.p_rts_flags.set(rts::VMINHIBIT);
    }
    child.p_name.push_suffix("*F");
}
```

**分离原因**：`p_priv` 字段尚未实现。当特权结构完成后，此函数会补充 `child.p_priv = USER_PRIV` 逻辑。

---

## 5. 参见

- [02-proc-struct-schedule](02-proc-struct-schedule.md) - 调度相关字段
- [06-proc-rts-flags](06-proc-rts-flags.md) - RTS 标志位
- [09-priv-struct](09-priv-struct.md) - 特权结构体
