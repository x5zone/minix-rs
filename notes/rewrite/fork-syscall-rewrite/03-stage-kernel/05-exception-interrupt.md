# 05-exception-interrupt: 异常与中断处理

> **分类**: Kernel 保护与中断
> **源码**: `minix3/minix/kernel/arch/i386/exception.c`(386行), `minix3/minix/kernel/interrupt.c`(177行)
> **说明**: 异常帧解析、页错误转发 VM、IRQ 路由——从硬件信号到内核处理的全链路

---

## 1. 概述

### 1.1 概念定义与作用

CPU 在执行指令过程中会产生两类硬件信号，内核必须正确处理它们才能维持系统运行：

| 信号类型 | 触发源 | 特点 | Minix3 处理方式 |
|---------|--------|------|----------------|
| **异常（Exception）** | CPU 内部：指令执行错误（除零、缺页、保护违例等） | 同步、精确，与当前指令关联 | 用户态→发信号；内核态→panic |
| **中断（Interrupt）** | 外部硬件：时钟、键盘、磁盘等设备 | 异步，与当前指令无关 | 调用驱动注册的钩子函数 |

**为什么异常和中放在一起？** 在 x86 架构中，异常和硬件中断共享同一套入口机制——IDT 门描述符。CPU 通过向量号索引 IDT，跳转到内核入口点。Minix3 在汇编层（mpx.S）区分两条路径：异常走 `exception_entry`，硬件中断走 `hwint_master`/`hwint_slave`，但最终都汇入 C 层的 `exception_handler()` 或 `irq_handle()`。

**核心设计原则**：Minix3 将异常和中断严格区分处理——异常是"当前进程出了问题"，中断是"外部设备有事通知"。异常影响进程状态（发信号、阻塞），中断影响设备交互（调用驱动钩子）。

### 1.2 与 Minix3 启动流程的对应关系

异常/中断处理在内核启动的以下阶段被激活：

| 阶段 | 调用点 | 函数 | 作用 |
|------|--------|------|------|
| **cstart()** | main.c:411 | `prot_init()` → `idt_copy_vectors()` | 在 IDT 中注册异常和硬件中断门描述符 |
| **cstart()** | main.c:411 | `intr_init()` | 初始化 8259A 中断控制器，屏蔽所有 IRQ |
| **main() 循环** | 各驱动初始化 | `put_irq_handler()` | 驱动通过 `SYS_IRQCTL` 系统调用注册 IRQ 钩子 |
| **运行时** | 硬件触发 | `exception_handler()` / `irq_handle()` | 处理异常和中断 |

**关键时序**：`prot_init()` 在 `cstart()` 中完成 IDT 填充后，异常和中断入口才生效。但此时 8259A 仍屏蔽所有 IRQ（`intr_init()` 的结果），直到驱动通过 `SYS_IRQCTL` → `put_irq_handler()` 显式注册并启用特定 IRQ 线。

### 1.3 关键状态与机制说明

#### 1.3.1 异常帧（exception_frame）

当异常发生时，CPU 自动将关键寄存器压入当前栈。Minix3 在 `arch_proto.h:72-80` 定义了异常帧结构，用于 C 层解析：

| 字段 | 含义 | 压入者 |
|------|------|--------|
| `vector` | 异常向量号 | 汇编入口（mpx.S） |
| `errcode` | 错误码（无错误码的异常由汇编压入 0） | CPU 或汇编入口 |
| `eip` | 异常发生时的指令指针 | CPU |
| `cs` | 异常发生时的代码段 | CPU |
| `eflags` | 异常发生时的标志寄存器 | CPU |
| `esp` | 异常发生时的栈指针（嵌套异常时无效） | CPU |
| `ss` | 异常发生时的栈段（嵌套异常时无效） | CPU |

**嵌套异常**：当异常在内核态发生时（`is_nested=1`），CPU 不会压入 `esp`/`ss`（因为不发生栈切换），此时这两个字段的值是栈上的残留数据，不可使用。

#### 1.3.2 异常分类表（ex_data[]）

Minix3 将 20 个 x86 异常向量映射到 POSIX 信号（exception.c:19-39）：

| 向量 | 异常名称 | 信号 | 最低处理器 |
|------|---------|------|-----------|
| 0 | Divide error | SIGFPE | 86 |
| 1 | Debug exception | SIGTRAP | 86 |
| 2 | Nonmaskable interrupt | SIGBUS | 86 |
| 3 | Breakpoint | SIGEMT | 86 |
| 4 | Overflow | SIGFPE | 86 |
| 5 | Bounds check | SIGFPE | 186 |
| 6 | Invalid opcode | SIGILL | 186 |
| 7 | Coprocessor not available | SIGFPE | 186 |
| 8 | Double fault | SIGBUS | 286 |
| 9 | Coprocessor segment overrun | SIGSEGV | 286 |
| 10 | Invalid TSS | SIGSEGV | 286 |
| 11 | Segment not present | SIGSEGV | 286 |
| 12 | Stack exception | SIGSEGV | 286 |
| 13 | General protection | SIGSEGV | 286 |
| 14 | Page fault | SIGSEGV | 386 |
| 15 | (软件陷阱) | SIGILL | 0 |
| 16 | Coprocessor error | SIGFPE | 386 |
| 17 | Alignment check | SIGBUS | 386 |
| 18 | Machine check | SIGBUS | 386 |
| 19 | SIMD exception | SIGFPE | 386 |

**页错误（向量 14）是特殊异常**：它不走通用的"发信号"路径，而是有独立的 `pagefault()` 函数处理——将缺页信息通过 `mini_send()` 转发给 VM 进程，并设置 `RTS_PAGEFAULT` 标志阻塞当前进程。

#### 1.3.3 IRQ 钩子链（irq_hook_t）

Minix3 允许同一 IRQ 线被多个驱动共享。每个驱动注册一个 `irq_hook_t` 钩子，同一 IRQ 的钩子组成链表（interrupt.c:23）：

| 字段 | 类型 | 含义 |
|------|------|------|
| `next` | `struct irq_hook *` | 链表下一项 |
| `handler` | `int (*)(struct irq_hook *)` | 中断处理回调 |
| `irq` | `int` | IRQ 向量号 |
| `id` | `int` | 唯一标识（位掩码，用于 `irq_actids`） |
| `proc_nr_e` | `endpoint_t` | 注册进程的端点 |
| `notify_id` | `irq_id_t` | 通知标识（返回给驱动） |
| `policy` | `irq_policy_t` | 策略位掩码（如 `IRQ_REENABLE`） |

**id 分配策略**：`put_irq_handler()` 为每个钩子分配最小的未使用位（1, 2, 4, 8, ...），用于在 `irq_actids[]` 中标记该钩子是否活跃。由于 `id` 是 `int` 类型，同一 IRQ 最多支持 32 个钩子（实际受 `NR_IRQ_HOOKS=16/64` 限制）。

#### 1.3.4 中断控制器抽象（hw_intr 宏）

Minix3 通过宏抽象中断控制器操作（hw_intr.h），支持 8259A PIC 和 IOAPIC 两种硬件：

| 宏 | 8259A PIC 实现 | IOAPIC 实现 |
|----|---------------|-------------|
| `hw_intr_mask(irq)` | `irq_8259_mask(irq)` | `ioapic_mask_irq(irq)` |
| `hw_intr_unmask(irq)` | `irq_8259_unmask(irq)` | `ioapic_unmask_irq(irq)` |
| `hw_intr_ack(irq)` | `irq_8259_eoi(irq)` | `ioapic_eoi(irq)` |
| `hw_intr_used(irq)` | (空) | `ioapic_set_irq(irq)` |
| `hw_intr_not_used(irq)` | (空) | `ioapic_unset_irq(irq)` |
| `hw_intr_disable_all()` | (空) | `ioapic_disable_all()` + `ioapic_reset_pic()` + `lapic_disable()` |

**8259A 的 `hw_intr_used`/`hw_intr_not_used` 为空**：因为 8259A 是固定 16 个 IRQ，无需动态配置路由。IOAPIC 需要显式设置 IRQ 路由到哪个 CPU。

#### 1.3.5 页错误转发机制

页错误是 Minix3 内核与 VM 进程交互的核心通道。当用户态进程触发缺页时：

1. CPU 将错误地址写入 CR2 寄存器
2. 内核读取 CR2，构造 `VM_PAGEFAULT` 消息
3. 通过 `mini_send()` 将消息发送给 VM 进程
4. 设置 `RTS_PAGEFAULT` 阻塞当前进程
5. VM 处理缺页后通过 `SYS_VMCTL` 清除 `RTS_PAGEFAULT`，恢复进程

**VM 自身不能缺页**：如果 VM 进程触发页错误，内核直接 panic（exception.c:100-113）。这是设计上的必然——VM 负责为所有进程管理内存，如果 VM 自身需要缺页处理，就形成了循环依赖。

### 1.4 行为规则

1. **异常在用户态进程**：转换为 POSIX 信号，通过 `cause_sig()` 发送给进程的信号管理器
2. **异常在内核态（非嵌套）**：调用 `inkernel_disaster()` 打印诊断信息并 panic
3. **异常在内核态（嵌套）**：特殊处理——
   - 在 `phys_copy`/`phys_memset` 中的页错误：跳转到故障恢复点
   - 在 `copy_msg_to_user`/`copy_msg_from_user` 中的页错误/保护违例：跳转到指针故障恢复点
   - 在 `fxrstor`/`frstor` 中的异常：跳转到 FPU 恢复故障点
   - 调试异常且进程被追踪：清除 TF 位后返回
   - 其他：panic
4. **硬件中断**：调用该 IRQ 线上所有注册钩子的回调函数，如果所有钩子都返回完成则重新启用该 IRQ
5. **伪中断**：IRQ 线上无注册钩子时，打印警告并保持屏蔽
6. **页错误转发**：用户态进程缺页→阻塞进程→发消息给 VM→VM 处理后恢复进程

---

## 2. C 源码分析

### 2.1 相关定义

#### 2.1.1 异常向量号（archconst.h:41-93）

```c
#define BOUNDS_VECTOR        5   /* bounds check failed */
#define INVAL_OP_VECTOR      6   /* invalid opcode */
#define COPROC_NOT_VECTOR    7   /* coprocessor not available */
#define DOUBLE_FAULT_VECTOR  8
#define COPROC_SEG_VECTOR    9   /* coprocessor segment overrun */
#define INVAL_TSS_VECTOR    10   /* invalid TSS */
#define SEG_NOT_VECTOR      11   /* segment not present */
#define STACK_FAULT_VECTOR  12   /* stack exception */
#define PROTECTION_VECTOR   13   /* general protection */
#define PAGE_FAULT_VECTOR   14
#define COPROC_ERR_VECTOR   16   /* coprocessor error */
#define ALIGNMENT_CHECK_VECTOR  17
#define MACHINE_CHECK_VECTOR    18
#define SIMD_EXCEPTION_VECTOR   19
```

向量 0-4 定义在 `interrupt.h:21-25`（`DIVIDE_VECTOR`/`DEBUG_VECTOR`/`NMI_VECTOR`/`BREAKPOINT_VECTOR`/`OVERFLOW_VECTOR`）。

#### 2.1.2 硬件中断向量布局（interrupt.h:34-59）

```c
#define IRQ0_VECTOR     0x50   /* IRQ0-7 重定位到的向量 */
#define IRQ8_VECTOR     0x70   /* IRQ8-15 无需移动 */
#define NR_IRQ_VECTORS  16     /* PIC 模式 / 64 (APIC 模式) */
#define VECTOR(irq)  (((irq) < 8 ? IRQ0_VECTOR : IRQ8_VECTOR) + ((irq) & 0x07))
```

**为什么 IRQ 向量要重定位？** x86 的异常向量占用了 0-31，8259A 的默认 IRQ 基向量是 8（主片）和 112（从片），与异常向量冲突。Minix3 将主片重定位到 0x50（80），从片保持 0x70（112），避免冲突。

#### 2.1.3 8259A 中断控制器端口（interrupt.h:9-15）

```c
#define INT_CTL         0x20   /* 主片 I/O 端口 */
#define INT_CTLMASK     0x21   /* 主片屏蔽寄存器 */
#define INT2_CTL        0xA0   /* 从片 I/O 端口 */
#define INT2_CTLMASK    0xA1   /* 从片屏蔽寄存器 */
#define END_OF_INT      0x20   /* EOI 命令码 */
```

#### 2.1.4 页错误消息格式（com.h:773-775）

```c
#define VM_PAGEFAULT    (VM_RQ_BASE+0xff)
# define VPF_ADDR       m1_i1   /* 缺页地址（来自 CR2） */
# define VPF_FLAGS      m1_i2   /* 错误码（来自 CPU） */
```

#### 2.1.5 进程运行时标志（proc.h:152）

```c
#define RTS_PAGEFAULT   0x400   /* 进程有未处理的页错误 */
```

`RTS_SET(pr, RTS_PAGEFAULT)` 将进程从运行队列移除，直到 VM 通过 `SYS_VMCTL` 清除此标志（do_vmctl.c:34-35）。

#### 2.1.6 内核陷阱样式（archconst.h:167-173）

```c
#define KTS_NONE       1  /* 无效 */
#define KTS_INT_HARD   2  /* 异常 / 硬件中断 */
#define KTS_INT_ORIG   3  /* 软中断（libc） */
#define KTS_INT_UM     4  /* 软中断（usermapped） */
#define KTS_FULLCONTEXT 5 /* 需恢复完整上下文 */
#define KTS_SYSENTER   6  /* SYSENTER 指令 */
#define KTS_SYSCALL    7  /* SYSCALL 指令 */
```

`p_kern_trap_style` 记录进程进入内核的方式，用于异常处理时判断上下文（如栈回溯需要根据入口方式选择正确的帧指针）。

#### 2.1.7 IRQ 策略常量（com.h:304-308）

```c
#define IRQ_SETPOLICY  1   /* 注册 IRQ 钩子 */
#define IRQ_RMPOLICY   2   /* 移除 IRQ 钩子 */
#define IRQ_ENABLE     3   /* 启用中断 */
#define IRQ_DISABLE    4   /* 禁用中断 */
#define IRQ_REENABLE   0x001  /* 中断处理后自动重新启用 */
```

#### 2.1.8 IRQ 钩子池大小（config.h:59-61）

```c
#define NR_IRQ_HOOKS   16   /* 非 SMP 配置 */
#define NR_IRQ_HOOKS   64   /* SMP 配置 */
```

### 2.2 核心数据结构

#### 2.2.1 exception_frame — 异常帧（arch_proto.h:72-80）

```c
struct exception_frame {
    reg_t   vector;     /* 触发的中断向量号 */
    reg_t   errcode;    /* 错误码，无错误码的异常由汇编压入 0 */
    reg_t   eip;        /* 异常发生时的指令指针 */
    reg_t   cs;         /* 异常发生时的代码段选择符 */
    reg_t   eflags;     /* 异常发生时的标志寄存器 */
    reg_t   esp;        /* 异常发生时的栈指针（嵌套时无效） */
    reg_t   ss;         /* 异常发生时的栈段（嵌套时无效） */
};
```

**字段布局与硬件压栈的对应**：CPU 在异常发生时自动压入 `ss`→`esp`→`eflags`→`cs`→`eip`→`errcode`（部分异常），汇编入口再压入 `vector`（和补零的 `errcode`）。C 层看到的 `exception_frame` 是从栈顶向下排列的，因此 `vector` 在最低地址。

**嵌套异常的陷阱**：当异常在内核态发生时（`is_nested=1`），CPU 不发生栈切换，不压入 `esp`/`ss`。此时 `exception_frame` 中的 `esp`/`ss` 字段实际上是之前栈上的残留数据，不可使用。汇编入口 `exception_entry_nested`（mpx.S:375）通过 `pusha` 保存通用寄存器，然后直接将调整后的栈指针作为 `exception_frame *` 传递给 C 层。

#### 2.2.2 ex_s — 异常描述（exception.c:13-17）

```c
struct ex_s {
    char *msg;          /* 异常描述字符串 */
    int signum;         /* 对应的 POSIX 信号 */
    int minprocessor;   /* 最低处理器型号（86=8086, 186=80186, ...） */
};
```

`ex_data[]` 数组（exception.c:19-39）以向量号为索引，共 20 项（向量 0-19）。向量 15 的 `msg` 为 NULL，表示"可能是软件陷阱"。

#### 2.2.3 irq_hook_t — IRQ 钩子（type.h:15-28）

```c
typedef unsigned long irq_policy_t;
typedef unsigned long irq_id_t;

typedef struct irq_hook {
    struct irq_hook *next;          /* 链表下一项 */
    int (*handler)(struct irq_hook *);  /* 中断处理回调 */
    int irq;                        /* IRQ 向量号 */
    int id;                         /* 唯一标识（位掩码） */
    endpoint_t proc_nr_e;           /* 注册进程的端点（NONE=未使用） */
    irq_id_t notify_id;             /* 通知标识 */
    irq_policy_t policy;            /* 策略位掩码 */
} irq_hook_t;

typedef int (*irq_handler_t)(struct irq_hook *);
```

**id 的位掩码设计**：`id` 取值为 1, 2, 4, 8, ...（2 的幂），用于在 `irq_actids[]` 中通过位运算快速标记/清除钩子的活跃状态。`put_irq_handler()` 分配最小的未使用位（interrupt.c:49-50），最多支持 `sizeof(int) * 8 = 32` 个钩子共享同一 IRQ。

#### 2.2.4 全局 IRQ 状态（glo.h:48-50, interrupt.c:23）

```c
EXTERN irq_hook_t irq_hooks[NR_IRQ_HOOKS];   /* 全局钩子池 */
EXTERN int irq_actids[NR_IRQ_VECTORS];        /* 每个 IRQ 的活跃位图 */
EXTERN int irq_use;                           /* 所有在用 IRQ 的位图 */
static irq_hook_t* irq_handlers[NR_IRQ_VECTORS]; /* 每个 IRQ 的钩子链表头 */
```

- `irq_hooks[]`：全局钩子池，`do_irqctl()` 从中分配（`proc_nr_e == NONE` 表示空闲），最多 `NR_IRQ_HOOKS` 个
- `irq_actids[]`：每个 IRQ 一个 `int`，位掩码标记哪些钩子正在处理中断。所有位清零后才能重新启用该 IRQ
- `irq_use`：位图标记哪些 IRQ 线已被使用
- `irq_handlers[]`：每个 IRQ 的钩子链表头指针，`put_irq_handler()`/`rm_irq_handler()` 维护

### 2.3 关键函数分析

#### 2.3.1 exception_handler() — 异常总入口（exception.c:180-283）

**位置**：exception.c:180-283

**原型**：`void exception_handler(int is_nested, struct exception_frame *frame)`

**调用者**：汇编入口 `exception_entry`（mpx.S:347，用户态异常）和 `exception_entry_nested`（mpx.S:375，内核态异常）

**处理流程**：

```
exception_handler(is_nested, frame)
  │
  ├─ 保存 proc_ptr（可能被调试语句修改）
  │
  ├─ 向量 2（NMI）？→ 打印 "spurious NMI" 并返回
  │
  ├─ is_nested？
  │   ├─ 在 copy_msg_to_user/copy_msg_from_user 中？
  │   │   └─ 页错误/保护违例 → 跳转到 __user_copy_msg_pointer_failure
  │   ├─ 在 fxrstor/frstor 中？
  │   │   └─ 跳转到 __frstor_failure
  │   └─ 调试异常 + 进程被追踪 + KTS_NONE？
  │       └─ 清除 TF 位并返回
  │
  ├─ 向量 14（页错误）？→ pagefault() 并返回
  │
  ├─ !is_nested && 用户态进程？
  │   └─ cause_sig(proc_nr, ep->signum) 并返回
  │
  └─ 内核态异常 → inkernel_disaster() → panic
```

**关键设计决策**：

1. **NMI 特殊处理**：向量 2 是不可屏蔽中断，在某些机器上会产生伪 NMI。Minix3 选择忽略它（exception.c:191-194）。

2. **嵌套异常的容错路径**：内核态异常通常是致命的，但 Minix3 为三种特定场景提供了恢复路径：
   - **消息拷贝中的页错误**：当内核代表用户进程拷贝 IPC 消息时，用户提供的指针可能无效。此时跳转到 `__user_copy_msg_pointer_failure`，让 IPC 函数返回错误码。
   - **FPU 状态恢复中的异常**：`fxrstor`/`frstor` 可能因无效的 FPU 状态触发异常。跳转到 `__frstor_failure`，将异常作为 FPU 信号转发给进程。
   - **调试异常**：如果被追踪的进程通过 SYSENTER/SYSCALL 进入内核，TF 位未被清除，会在内核第一条指令触发调试异常。此时清除 TF 位并返回。

3. **页错误独立处理**：向量 14 不走通用的"发信号"路径，而是调用 `pagefault()` 转发给 VM。

#### 2.3.2 pagefault() — 页错误处理（exception.c:49-130）

**位置**：exception.c:49-130

**原型**：`static void pagefault(struct proc *pr, struct exception_frame *frame, int is_nested)`

**处理流程**：

```
pagefault(pr, frame, is_nested)
  │
  ├─ 读取 CR2 获取缺页地址
  │
  ├─ 在 phys_copy/phys_memset 中 && catch_pagefaults？
  │   ├─ is_nested → 跳转到 phys_copy_fault_in_kernel / memset_fault_in_kernel
  │   └─ !is_nested → 设置 pc=phys_copy_fault, retreg=cr2
  │   └─ 返回
  │
  ├─ is_nested（内核态页错误，非 phys_copy）？
  │   └─ inkernel_disaster() → panic
  │
  ├─ pr == VM_PROC_NR？
  │   └─ 打印诊断信息 → panic("pagefault in VM")
  │
  ├─ RTS_SET(pr, RTS_PAGEFAULT)  — 阻塞进程
  │
  └─ mini_send(pr, VM_PROC_NR, &m_pagefault, FROM_KERNEL)
      ├─ m_type = VM_PAGEFAULT
      ├─ VPF_ADDR = cr2
      └─ VPF_FLAGS = frame->errcode
```

**phys_copy 容错机制**：内核在执行 `phys_copy()`/`phys_memset()` 时可能触发页错误（访问无效的物理地址）。`catch_pagefaults` 全局变量（glo.h:75）是一个计数器，在 `PHYS_COPY_CATCH` 宏（vm.h:10-14）中递增/递减，标记当前是否在"可捕获页错误"的上下文中。

**64 位演进影响**：在 64 位架构下，`phys_copy`/`phys_memset` 的容错机制可能不再需要——Direct Map 将所有物理内存映射到固定虚拟地址，内核通过虚拟地址直接访问物理内存，不再需要 `phys_copy` 这类物理地址拷贝函数。此机制是否保留需要在设计阶段决策。

#### 2.3.3 inkernel_disaster() — 内核态异常诊断（exception.c:132-178）

**位置**：exception.c:132-178

**原型**：`static void inkernel_disaster(struct proc *saved_proc, struct exception_frame *frame, struct ex_s *ep, int is_nested)`

此函数仅在 `USE_SYSDEBUG` 编译条件下有实质内容。它打印异常信息（向量号、错误码、EIP、CS、EFLAGS）、内核寄存器快照、内核栈回溯，然后 panic。如果 `saved_proc` 非空，还会打印被中断的进程信息和其栈回溯。

#### 2.3.4 put_irq_handler() — 注册 IRQ 钩子（interrupt.c:29-73）

**位置**：interrupt.c:29-73

**原型**：`void put_irq_handler(irq_hook_t *hook, int irq, irq_handler_t handler)`

**处理流程**：

1. 检查 `irq` 范围（0 ≤ irq < NR_IRQ_VECTORS）
2. 遍历 `irq_handlers[irq]` 链表，检查是否已注册（`hook == *line` 则直接返回）
3. 收集已使用的 `id` 位图
4. 分配最小的未使用 `id`（1, 2, 4, 8, ...）
5. 如果 `id == 0`（所有位已用完），panic
6. 将钩子插入链表末尾
7. 如果该 IRQ 无活跃钩子（`irq_actids[irq] & ~hook->id == 0`），调用 `hw_intr_used()` + `hw_intr_unmask()` 启用该 IRQ

**id 分配算法**（interrupt.c:49-50）：

```c
for (id = 1; id != 0; id <<= 1)
    if (!(bitmap & id)) break;
```

从最低位开始扫描，找到第一个空闲位。这保证了同一 IRQ 上钩子的 `id` 互不重叠，可以独立标记活跃状态。

#### 2.3.5 rm_irq_handler() — 注销 IRQ 钩子（interrupt.c:75-106）

**位置**：interrupt.c:75-106

**原型**：`void rm_irq_handler(const irq_hook_t *hook)`

**处理流程**：

1. 检查 `irq` 范围
2. 遍历链表，找到 `id` 匹配的节点并移除
3. 如果被移除的钩子是活跃的，清除其在 `irq_actids[]` 中的位
4. 如果链表为空，调用 `hw_intr_mask()` + `hw_intr_not_used()` 禁用该 IRQ
5. 如果链表非空但无活跃钩子，调用 `hw_intr_unmask()` 重新启用

#### 2.3.6 irq_handle() — 硬件中断分发（interrupt.c:116-157）

**位置**：interrupt.c:116-157

**原型**：`void irq_handle(int irq)`

**调用者**：汇编入口 `hwint_master`/`hwint_slave`（mpx.S），通过 `PIC_IRQ_HANDLER(irq)` 宏调用

**处理流程**：

1. `hw_intr_mask(irq)` — 屏蔽该 IRQ（防止中断重入）
2. 检查 `irq_handlers[irq]` 是否为 NULL（伪中断检测）
3. 遍历钩子链表，对每个钩子：
   - `irq_actids[irq] |= hook->id` — 标记为活跃
   - 调用 `hook->handler(hook)` — 执行回调
   - 如果回调返回非零，`irq_actids[hook->irq] &= ~hook->id` — 清除活跃标记
4. 如果 `irq_actids[irq] == 0`（所有钩子都已完成），`hw_intr_unmask(irq)` — 重新启用
5. `hw_intr_ack(irq)` — 发送 EOI

**伪中断处理**：如果 `irq_handlers[irq]` 为 NULL，说明该 IRQ 没有注册钩子。Minix3 打印警告信息并保持 IRQ 屏蔽（interrupt.c:126-134）。警告频率使用指数退避（初始每 100 次报告一次，之后翻倍）。

**活跃标记的作用**：`irq_actids[]` 追踪哪些钩子仍在处理中。如果钩子回调返回 0（表示"还没处理完"），其 `id` 位保持设置，IRQ 不会被重新启用。驱动通过 `enable_irq()` 显式清除活跃位来重新启用 IRQ。这实现了"中断处理未完成时不重入"的语义。

#### 2.3.7 enable_irq() / disable_irq() — 启用/禁用中断（interrupt.c:161-177）

**位置**：interrupt.c:161-177

```c
void enable_irq(const irq_hook_t *hook) {
    if((irq_actids[hook->irq] &= ~hook->id) == 0) {
        hw_intr_unmask(hook->irq);
    }
}

int disable_irq(const irq_hook_t *hook) {
    if(irq_actids[hook->irq] & hook->id)  /* already disabled */
        return 0;
    irq_actids[hook->irq] |= hook->id;
    hw_intr_mask(hook->irq);
    return TRUE;
}
```

`enable_irq()` 清除钩子的活跃位，如果该 IRQ 上所有钩子都不活跃，则取消屏蔽。`disable_irq()` 设置钩子的活跃位并屏蔽 IRQ，返回 1 表示成功禁用，0 表示已经处于禁用状态。

#### 2.3.8 cause_sig() — 发送信号（system.c:389-449）

**位置**：system.c:389-449

**原型**：`void cause_sig(proc_nr_t proc_nr, int sig_nr)`

此函数由 `exception_handler()` 调用，将异常转换为信号。核心逻辑：

1. 查找进程的信号管理器（`priv(rp)->s_sig_mgr`）
2. 如果目标是自身的信号管理器：
   - 致命信号→尝试切换到备份信号管理器→失败则 panic
   - 非致命信号→添加到 `s_sig_pending`，通知信号管理器
3. 如果目标不是自身的信号管理器：
   - 添加到进程的 `p_pending` 信号集
   - 设置 `RTS_SIGNALED | RTS_SIG_PENDING` 阻塞进程
   - 通过 `send_sig()` 通知信号管理器

#### 2.3.9 generic_handler() — 通用 IRQ 回调（do_irqctl.c:143-171）

**位置**：do_irqctl.c:143-171

**原型**：`static int generic_handler(irq_hook_t *hook)`

这是 `do_irqctl()` 中 `IRQ_SETPOLICY` 注册的默认回调。它不直接处理中断，而是：

1. 收集随机性（`get_randomness()`）
2. 在进程的 `s_int_pending` 中设置对应位
3. 通过 `mini_notify()` 从 HARDWARE 进程发送通知给驱动
4. 返回 `hook->policy & IRQ_REENABLE`（如果设置了 `IRQ_REENABLE`，`irq_handle()` 会自动清除活跃位）

**驱动中断处理的两种模式**：
- **IRQ_REENABLE**：中断处理后自动重新启用 IRQ 线。适用于简单驱动。
- **手动控制**：驱动在处理完中断后显式调用 `sys_irqenable()` → `enable_irq()`。适用于需要延迟重新启用的驱动。

#### 2.3.10 do_irqctl() — IRQ 系统调用处理（do_irqctl.c:23-139）

**位置**：do_irqctl.c:23-139

**原型**：`int do_irqctl(struct proc *caller, message *m_ptr)`

处理 `SYS_IRQCTL` 系统调用的四种请求：

| 请求 | 操作 | 权限检查 |
|------|------|---------|
| `IRQ_SETPOLICY` | 分配钩子、注册回调、启用 IRQ | 检查 `CHECK_IRQ` 权限 |
| `IRQ_RMPOLICY` | 移除钩子、禁用 IRQ | 检查调用者是否为钩子所有者 |
| `IRQ_ENABLE` | 调用 `enable_irq()` | 检查调用者是否为钩子所有者 |
| `IRQ_DISABLE` | 调用 `disable_irq()` | 检查调用者是否为钩子所有者 |

#### 2.3.11 copr_not_available_handler() — FPU 不可用处理（proc.c:1922-1959）

**位置**：proc.c:1922-1959

**原型**：`void copr_not_available_handler(void)`

当 CPU 执行浮点指令但 CR0.TS=1 时触发。此函数不在 `exception_handler()` 的主路径中，而是由汇编入口 `copr_not_available`（mpx.S:540-548）直接调用：

1. 禁用 FPU 异常（`disable_fpu_exception()` → `clts()`）
2. 保存当前 FPU 所有者的状态（`save_local_fpu()`）
3. 恢复当前进程的 FPU 状态（`restore_fpu()`）
4. 如果恢复失败，发送 SIGFPE 信号

#### 2.3.12 enable_fpu_exception() / disable_fpu_exception()（exception.c:375-386）

**位置**：exception.c:375-386

```c
void enable_fpu_exception(void) {
    u32_t cr0 = read_cr0();
    if(!(cr0 & I386_CR0_TS))
        write_cr0(cr0 | I386_CR0_TS);
}

void disable_fpu_exception(void) {
    clts();
}
```

`enable_fpu_exception()` 设置 CR0.TS 位，使下一条浮点指令触发 #NM 异常。`disable_fpu_exception()` 通过 `clts` 指令清除 CR0.TS。

### 2.4 调用关系/调用点分析

#### 2.4.1 异常处理调用链

```
CPU 异常
  → IDT 门描述符跳转
    → mpx.S 异常入口（EXCEPTION_ERR_CODE / EXCEPTION_NO_ERR_CODE）
      → exception_entry（用户态）/ exception_entry_nested（内核态）
        → exception_handler(is_nested, frame)
          ├─ 向量 14 → pagefault()
          │    ├─ phys_copy 容错 → 修改 EIP 返回
          │    ├─ VM 页错误 → panic
          │    └─ 用户态 → RTS_SET(RTS_PAGEFAULT) + mini_send(VM)
          ├─ 用户态异常 → cause_sig()
          └─ 内核态异常 → inkernel_disaster() → panic
```

#### 2.4.2 硬件中断调用链

```
硬件设备 → IRQ 线
  → 8259A / IOAPIC
    → CPU 中断
      → IDT 门描述符跳转
        → mpx.S hwint_master / hwint_slave
          → SAVE_PROCESS_CTX + context_stop
            → irq_handle(irq)
              ├─ hw_intr_mask(irq)
              ├─ 遍历 irq_handlers[irq] 链表
              │   └─ hook->handler(hook)
              │       └─ generic_handler() → mini_notify() + get_randomness()
              ├─ hw_intr_unmask(irq)（条件：无活跃钩子）
              └─ hw_intr_ack(irq)
            → switch_to_user
```

#### 2.4.3 IRQ 注册/注销调用链

```
驱动 → sys_irqctl() 系统调用
  → do_irqctl()
    ├─ IRQ_SETPOLICY → put_irq_handler() → hw_intr_used() + hw_intr_unmask()
    ├─ IRQ_RMPOLICY  → rm_irq_handler()  → hw_intr_mask() + hw_intr_not_used()
    ├─ IRQ_ENABLE    → enable_irq()       → hw_intr_unmask()
    └─ IRQ_DISABLE   → disable_irq()      → hw_intr_mask()
```

#### 2.4.4 关键函数调用点汇总

| 函数 | 定义位置 | 调用者 |
|------|---------|--------|
| `exception_handler()` | exception.c:180 | mpx.S:347, mpx.S:375 |
| `pagefault()` | exception.c:49 | exception_handler() |
| `inkernel_disaster()` | exception.c:132 | exception_handler(), pagefault() |
| `cause_sig()` | system.c:389 | exception_handler(), proc.c, system.c |
| `irq_handle()` | interrupt.c:116 | mpx.S PIC_IRQ_HANDLER 宏 |
| `put_irq_handler()` | interrupt.c:29 | do_irqctl() |
| `rm_irq_handler()` | interrupt.c:75 | do_irqctl() |
| `enable_irq()` | interrupt.c:161 | do_irqctl() |
| `disable_irq()` | interrupt.c:169 | do_irqctl() |
| `generic_handler()` | do_irqctl.c:143 | irq_handle()（通过钩子链） |
| `copr_not_available_handler()` | proc.c:1922 | mpx.S:545（直接调用，不走 exception_handler） |
| `enable_fpu_exception()` | exception.c:375 | proc.c（FPU 上下文切换） |
| `disable_fpu_exception()` | exception.c:383 | copr_not_available_handler() |
| `intr_init()` | i8259.c:28 | cstart() |
| `proc_stacktrace()` | exception.c:333 | 多处调试代码 |

### 2.5 设计要点/特殊处理

#### 2.5.1 嵌套异常的容错设计

Minix3 内核对嵌套异常（内核态中发生的异常）采取"尽力恢复"策略，为三种特定场景提供了恢复路径：

| 场景 | 恢复方式 | 目的 |
|------|---------|------|
| `phys_copy`/`phys_memset` 中的页错误 | 修改 EIP 跳转到故障恢复标签 | 让物理拷贝函数返回错误而非 panic |
| `copy_msg_to_user`/`copy_msg_from_user` 中的页错误/保护违例 | 跳转到 `__user_copy_msg_pointer_failure` | 让 IPC 函数返回 EFAULT |
| `fxrstor`/`frstor` 中的异常 | 跳转到 `__frstor_failure` | 将 FPU 异常转发为信号 |

这些恢复路径的核心思想是：**内核代表用户进程执行操作时，用户提供的无效数据不应导致内核 panic**。但其他内核态异常（如真正的内核 bug）仍然会 panic。

**64 位演进影响**：在 64 位架构下，`phys_copy`/`phys_memset` 可能被 Direct Map 取代，其容错机制可能不再需要。但 `copy_msg_to_user`/`copy_msg_from_user` 的容错机制仍然必要——内核在拷贝 IPC 消息时，用户态指针可能无效。

#### 2.5.2 IRQ 共享与活跃追踪

Minix3 的 IRQ 共享机制通过 `irq_actids[]` 数组实现"部分完成"语义：

- `irq_handle()` 在调用每个钩子前设置其活跃位
- 如果钩子回调返回非零（完成），立即清除活跃位
- 如果钩子回调返回零（未完成），活跃位保持设置
- 只有所有活跃位都清除时，IRQ 才被重新启用

这允许慢速驱动延迟 IRQ 重新启用，同时不影响同一 IRQ 线上的其他驱动。驱动通过 `enable_irq()` 显式通知内核"我处理完了"。

#### 2.5.3 catch_pagefaults 计数器

`catch_pagefaults`（glo.h:75）是一个全局计数器，用于标记当前是否在"可捕获页错误"的上下文中。`PHYS_COPY_CATCH` 宏（vm.h:11-14）在 `phys_copy()` 调用前后递增/递减此计数器：

```c
#define PHYS_COPY_CATCH(src, dst, size, a) {  \
    catch_pagefaults++;                        \
    a = phys_copy(src, dst, size);             \
    catch_pagefaults--;                        \
}
```

`pagefault()` 检查 `catch_pagefaults && (in_physcopy || in_memset)` 来决定是否尝试恢复。这是一个"软信号"机制——不使用信号量或锁，仅靠计数器保证正确性。在单内核、非抢占的执行模型下，这是安全的。

#### 2.5.4 VM 页错误的不可恢复性

VM 进程的页错误直接 panic（exception.c:100-113），这是架构上的必然选择：

1. VM 负责为所有进程管理物理内存和页表
2. 如果 VM 自身缺页，需要另一个实体来处理——但没有这样的实体
3. VM 的地址空间在启动时由内核完全映射（`arch_boot_proc()`），运行时不应缺页

**64 位演进影响**：在 64 位架构下，Direct Map 机制使得内核（包括 VM）可以通过固定虚拟地址访问所有物理内存，VM 自身的代码/数据段映射在启动时完成，缺页的可能性进一步降低。

#### 2.5.5 8259A 初始化与 EOI 时序

`intr_init()`（i8259.c:28-54）初始化 8259A 的时序要求严格：

1. 向主片发送 ICW1（开始初始化）
2. 向主片发送 ICW2（设置基向量 = IRQ0_VECTOR = 0x50）
3. 向主片发送 ICW3（告知从片级联在 IRQ2）
4. 向主片发送 ICW4（设置 8086 模式、正常 EOI）
5. 屏蔽主片所有 IRQ（除级联 IRQ2）
6. 对从片重复步骤 1-5（基向量 = IRQ8_VECTOR = 0x70）

**EOI 时序**：在 `hwint_master`/`hwint_slave` 汇编入口中，EOI 在 `irq_handle()` 返回后、`switch_to_user` 之前发送（mpx.S:83-84）。这保证了在 EOI 发送前，IRQ 仍被屏蔽，不会产生中断重入。

**64 位演进影响**：x86-64 系统通常使用 APIC 而非 8259A。8259A 仅在单处理器或非常旧的硬件上使用。在 64 位实现中，8259A 初始化代码可能不再需要，但 APIC 的初始化逻辑需要替代它。

---

## 3. Rust 设计决策

> 本章解释从 Ch1&2 的 C 源码到 Rust 设计的每一个关键选择。每个决策给出：为什么选这条路径、替代方案有哪些、为什么否决替代方案。

### 3.1 三 trait 拆分：InterruptController + ExceptionArch + IrqManager

**C 源码依据**：§1.3 分析了异常和中断的两条处理路径——异常走 `exception_handler()`，中断走 `irq_handle()`。§2.3.1-2.3.3 分析了 `exception_handler()`、`pagefault()`、`irq_handle()` 的职责划分。§2.2.3 分析了 `irq_hook_t` 钩子链的数据结构。

**决策**：将异常与中断相关硬件操作拆分为三个 trait/模块：

| trait/类型 | 职责 | x86-64 实现 | ARM64 实现 | RISC-V 实现 |
|-----------|------|------------|-----------|-------------|
| `InterruptController` | 中断控制器硬件操作（mask/unmask/ack/eoi） | LAPIC + IOAPIC | GICv2/GICv3 | PLIC + CLINT |
| `ExceptionArch` | 异常帧解析 + CR2 读取 + 故障恢复点 | x86-64 exception frame | ARM64 ESR_EL1 + FAR_EL1 | RISC-V scause + stval |
| `IrqManager`（非 trait） | IRQ 钩子链管理 + 活跃追踪 + 分发 | 架构无关 | 架构无关 | 架构无关 |

**为什么是三个而非一个**：

考虑过单 trait 方案（`InterruptArch` 覆盖全部中断/异常操作），但否决了。原因有三：

1. **中断控制器是纯硬件操作**，与"异常帧解析"完全不同。中断控制器的 mask/unmask/ack 是对硬件寄存器的直接操作，而异常帧解析是对 CPU 压栈数据的结构化访问。合并会导致 trait 承担两个不相关的职责。

2. **异常帧是架构特定的**，而 IRQ 钩子链是架构无关的。`exception_frame` 在 x86-64 和 ARM64 上完全不同（字段、布局、压栈方式），但 `irq_hook_t` 链表在所有架构上逻辑相同——注册钩子、遍历链表、追踪活跃位。将架构无关的 `IrqManager` 纳入 trait，会导致 ARM64/RISC-V 的实现与 x86-64 完全相同，违反"trait 每个方法在不同架构上实现真的不同"标准（review-code-skill §2.5）。

3. **初始化时序不同**：中断控制器需要在 `intr_init()` 阶段初始化（屏蔽所有 IRQ），而异常帧解析不需要初始化——它是 CPU 硬件自动构建的，只需要定义解析方法。

**为什么 IrqManager 不是 trait**：

`IrqManager` 管理 IRQ 钩子链（`put_irq_handler`/`rm_irq_handler`/`irq_handle`），其逻辑在所有架构上完全相同：分配位掩码 ID、维护链表、追踪活跃位、在所有钩子完成后重新启用 IRQ。唯一的架构依赖是通过 `InterruptController` trait 调用 mask/unmask/ack——这通过泛型约束 `IC: InterruptController` 解决，不需要 `IrqManager` 本身是 trait。

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| 单 trait `InterruptArch` 覆盖全部 | 职责混乱：硬件操作 + 数据解析 + 钩子管理混在一起 |
| 两 trait（`InterruptController` + `ExceptionArch`），IrqManager 是 ExceptionArch 的一部分 | IrqManager 架构无关，不应属于架构特定 trait |
| IrqManager 也是 trait | 所有架构实现相同，违反"方法在不同架构上实现真的不同"标准 |

### 3.2 InterruptController trait：中断控制器抽象

**C 源码依据**：§2.5.5 分析了 `hw_intr` 宏的抽象机制——8259A 和 IOAPIC 通过同一套宏接口（`hw_intr_mask`/`hw_intr_unmask`/`hw_intr_ack`）提供不同实现。§2.3.4 分析了 `irq_handle()` 中 `hw_intr_mask`→处理→`hw_intr_unmask`→`hw_intr_ack` 的调用时序。

**决策**：将 C 的 `hw_intr` 宏替换为 `InterruptController` trait，提供统一的中断控制器操作接口。

**推理过程**：

Minix3 的 `hw_intr.h` 已经在做"中断控制器抽象"——通过宏在编译时选择 8259A 或 IOAPIC 实现。但宏是文本替换，没有类型安全，也不支持运行时切换。Rust 的 trait 天然解决这个问题：

```rust
trait InterruptController {
    fn init(&mut self);
    fn mask(&mut self, irq: IrqVector);
    fn unmask(&mut self, irq: IrqVector);
    fn ack(&mut self, irq: IrqVector);
    fn eoi(&mut self, irq: IrqVector);
    fn mask_all(&mut self);
}
```

**方法名选择——为什么不用 x86 语义**：

| OS 语义方法 | x86 语义 | 为什么不用 x86 语义 |
|------------|---------|-------------------|
| `mask()` | `irq_8259_mask()` / `ioapic_mask_irq()` | ARM64 称为 "disable interrupt"，RISC-V 称为 "claim complete" |
| `unmask()` | `irq_8259_unmask()` / `ioapic_unmask_irq()` | ARM64 称为 "enable interrupt" |
| `ack()` | `irq_8259_eoi()` / `ioapic_eoi()` | EOI (End of Interrupt) 是 x86 特有概念；ARM64 用 "deactivate"，RISC-V 用 "complete" |
| `eoi()` | 同 `ack()` | 保留 `eoi()` 作为语义更明确的别名，因为 Minix3 代码中 EOI 和 ack 有微妙区别 |

**为什么 `ack()` 和 `eoi()` 都保留**：在 Minix3 的 `irq_handle()` 中，`hw_intr_ack(irq)` 在所有钩子处理完成后调用一次。这是 x86 的 EOI 语义——通知中断控制器"此中断已处理完成"。但在某些架构上，"确认收到中断"（ack）和"通知处理完成"（eoi）是两个不同操作。ARM64 的 GICv3 有 "ACK"（读取 IAR 寄存器获取中断 ID）和 "EOI"（写入 EOIR 寄存器）两个步骤。保留两个方法允许架构实现精确表达这个差异。

**IrqVector 类型**：`IrqVector(u8)` 封装 IRQ 向量号，与 `InterruptVector` 不同。`InterruptVector` 是 IDT 向量号（0-255），而 `IrqVector` 是硬件中断号（0-15 for 8259A, 0-23 for IOAPIC, 0-1023 for GICv3）。x86-64 上需要将 `IrqVector` 映射到 `InterruptVector`（通过加偏移 `IRQ0_VECTOR`），这个映射是 `InterruptController` 实现的内部细节。

### 3.3 ExceptionArch trait：异常帧解析与故障恢复

**C 源码依据**：§2.2.1 分析了 `exception_frame` 结构体——包含向量号、错误码、EIP、CS、EFLAGS 等字段。§2.3.1 分析了 `exception_handler()` 如何解析异常帧判断异常类型和来源。§2.3.2 分析了 `pagefault()` 如何读取 CR2 获取缺页地址。§2.5.1 分析了嵌套异常的故障恢复机制——修改 EIP 跳转到恢复标签。

**决策**：定义 `ExceptionArch` trait，封装异常帧解析和故障恢复点查询。异常帧本身作为 trait 的关联类型，因为不同架构的异常帧布局完全不同。

**推理过程**：

异常帧是 CPU 在异常发生时自动压入内核栈的数据结构。不同架构的异常帧差异极大：

| 字段 | x86-64 | ARM64 | RISC-V |
|------|--------|-------|--------|
| 向量号 | `vector` (pushed by asm) | 从 ESR_EL1 读取 | 从 scause 读取 |
| 错误码 | `errcode` (CPU or 0) | ESR_EL1.ISS | stval (部分) |
| 指令指针 | `rip` | ELR_EL1 | sepc |
| 栈指针 | `rsp` (from TSS) | SP_EL0 | sp (从 sscratch 恢复) |
| 特权级 | CS.RPL | SPSR_EL1.M | sstatus.SPP |

如果用公共结构体，需要覆盖所有架构的字段，大部分在某架构上是无效的。用关联类型让每个架构定义自己的异常帧：

```rust
trait ExceptionArch {
    type Frame;

    fn vector(frame: &Self::Frame) -> InterruptVector;
    fn error_code(frame: &Self::Frame) -> u64;
    fn instruction_pointer(frame: &Self::Frame) -> VirBytes;
    fn is_user_mode(frame: &Self::Frame) -> bool;
    fn page_fault_address() -> VirBytes;
    fn set_instruction_pointer(frame: &mut Self::Frame, ip: VirBytes);
    fn set_return_value(frame: &mut Self::Frame, value: u64);
}
```

**为什么 `page_fault_address()` 是静态方法**：x86-64 的缺页地址存储在 CR2 寄存器中，与异常帧无关——CR2 是 CPU 在页错误发生时自动设置的独立寄存器。ARM64 的缺页地址存储在 FAR_EL1 中，RISC-V 存储在 stval 中。这些都不是异常帧的一部分，因此 `page_fault_address()` 不需要 `frame` 参数。

**为什么需要 `set_instruction_pointer()` 和 `set_return_value()`**：§2.5.1 分析了嵌套异常的故障恢复机制——`pagefault()` 修改 `frame->eip` 跳转到恢复标签，并设置 `pr->p_reg.retreg = cr2` 传递缺页地址。这两个操作在 Rust 中需要通过 trait 方法完成，因为异常帧的布局是架构特定的。

**为什么不用 `From<RawFrame>` 转换**：考虑过将原始异常帧（CPU 压栈的原始数据）转换为公共的 `ExceptionInfo` 结构体。但转换会丢失架构特定信息（如 x86 的错误码位域解析、ARM64 的 ESR_EL1 ISS 字段），而这些信息在故障恢复中需要。关联类型 + 方法的方式既保留了架构特定信息，又提供了架构无关的访问接口。

### 3.4 IrqManager：架构无关的 IRQ 钩子管理

**C 源码依据**：§2.2.3 分析了 `irq_hook_t` 结构体和钩子链机制。§2.3.3-2.3.4 分析了 `put_irq_handler()`、`rm_irq_handler()`、`irq_handle()` 的实现。§2.5.2 分析了 `irq_actids[]` 活跃追踪机制。

**决策**：`IrqManager` 是 kernel crate 中的泛型结构体，通过 `IC: InterruptController` 约束访问中断控制器操作。不纳入任何 trait。

**推理过程**：

`IrqManager` 的核心逻辑在所有架构上完全相同：

1. **钩子注册**（`put_irq_handler`）：遍历链表分配最小未使用位掩码 ID，挂入链表尾部
2. **钩子移除**（`rm_irq_handler`）：从链表中摘除，清除活跃位，若无剩余钩子则 mask IRQ
3. **中断分发**（`irq_handle`）：mask IRQ → 遍历钩子链调用 handler → 追踪活跃位 → 全部完成后 unmask → eoi
4. **活跃追踪**（`irq_actids`）：钩子返回 0 表示"未完成"，活跃位保持；返回非零表示"已完成"，清除活跃位

唯一的架构依赖是步骤 3 中的 mask/unmask/eoi 调用——通过 `IC: InterruptController` 泛型约束解决。

**C 的 `irq_hook_t` → Rust 的 `IrqHook`**：

| C 字段 | Rust 类型 | 变化 |
|--------|----------|------|
| `next` | `Option<Box<IrqHook<IC>>>` | 链表用 Option 表达"无下一项" |
| `handler` | `fn(&IrqHook<IC>) -> IrqAction` | 函数指针改为返回枚举 |
| `irq` | `IrqVector` | newtype 替代裸 int |
| `id` | `IrqId(u32)` | newtype 替代裸 int |
| `proc_nr_e` | `Endpoint` | 使用已有的 Endpoint 类型 |
| `notify_id` | `IrqNotifyId(u32)` | newtype 替代裸 unsigned long |
| `policy` | `IrqPolicy` (bitflags) | bitflags 替代裸 unsigned long |

**为什么 `handler` 返回 `IrqAction` 枚举而非 `int`**：

C 中钩子回调返回非零表示"已完成"（清除活跃位），返回零表示"未完成"（保持活跃位）。这是典型的哨兵值模式。Rust 用枚举更清晰：

```rust
enum IrqAction {
    Completed,
    NotCompleted,
}
```

**为什么用 `Vec<IrqHookSlot>` 而非链表**：

C 用链表是因为 `irq_hook_t` 是静态全局数组 `irq_hooks[NR_IRQ_HOOKS]`，链表连接空闲和已用槽位。Rust 可以用 `Vec<Option<IrqHookSlot>>` 或 `Vec<IrqHookSlot>` + `used` 标记，更符合 Rust 惯例。但考虑到 `no_std` 环境和固定大小的约束，使用固定大小数组更合适：

```rust
struct IrqManager<IC: InterruptController> {
    hooks: [Option<IrqHookSlot>; NR_IRQ_HOOKS],
    handlers: [Option<usize>; NR_IRQ_VECTORS],  // 每向量的钩子链头索引
    actids: [IrqIdBitmap; NR_IRQ_VECTORS],       // 每向量的活跃位图
    controller: IC,
}
```

**为什么 `IrqPolicy` 用 bitflags**：`IRQ_REENABLE` 是可组合的标志位——钩子可以同时设置多个策略。bitflags 的 `|` 操作对策略组合有意义（与特权级不同，特权级是互斥的）。

### 3.5 ExceptionDispatcher：异常分发与页错误转发

**C 源码依据**：§2.3.1 分析了 `exception_handler()` 的分发逻辑——根据向量号和 `is_nested` 标志走不同路径。§2.3.2 分析了 `pagefault()` 的页错误转发机制——读取 CR2、构造 `VM_PAGEFAULT` 消息、设置 `RTS_PAGEFAULT` 标志。

**决策**：`ExceptionDispatcher` 是 kernel crate 中的结构体，封装异常分发逻辑。页错误转发通过 `VmPagefaultIn` 类型（已在 minix-types 中定义）构造消息。

**推理过程**：

`exception_handler()` 的分发逻辑是架构无关的——根据向量号判断异常类型，根据 `is_user_mode()` 判断来源，然后走"发信号"或"panic"路径。唯一的架构依赖是异常帧解析——通过 `ExceptionArch` trait 解决。

**C 的 `ex_data[]` → Rust 的 `ExceptionClass` 枚举**：

C 用静态数组 `ex_data[]` 将向量号映射到信号。Rust 用枚举 + match 更清晰：

```rust
enum ExceptionClass {
    NonMaskableInterrupt,
    PageFault,
    Debug,
    Signal(Signal),
}
```

页错误（向量 14）和 NMI（向量 2）有特殊处理路径，不映射到信号。Debug 异常在特定条件下有特殊处理（清除 TF 位）。其余异常映射到 POSIX 信号。

**页错误转发的 Rust 表达**：

C 中 `pagefault()` 构造 `message` 结构体并调用 `mini_send()`。Rust 中使用已有的 `VmPagefaultIn` 类型：

```rust
// C: m_pagefault.m_type = VM_PAGEFAULT;
//     m_pagefault.VPF_ADDR = cr2;
//     m_pagefault.VPF_FLAGS = frame->errcode;
// Rust:
let request = VmPagefaultIn {
    endpoint: proc.endpoint(),
    vaddr: EA::page_fault_address(),
    write: EA::error_code(frame).is_write_fault(),
};
```

**为什么 `write` 字段从 `errcode` 解析**：C 中 `VPF_FLAGS = frame->errcode`，VM 进程自行解析错误码的位 1（Write bit）。Rust 中 `VmPagefaultIn` 已经将 `write` 提取为 `bool` 字段，因此需要在构造时解析。这个解析逻辑是架构特定的（x86-64 错误码位 1 = Write，ARM64 FAR_EL1 的 WnR 位），属于 `ExceptionArch` 的职责。

### 3.6 嵌套异常容错的 Rust 表达

**C 源码依据**：§2.5.1 分析了三种嵌套异常恢复路径：`phys_copy`/`phys_memset`、`copy_msg_to_user`/`copy_msg_from_user`、`fxrstor`/`frstor`。

**决策**：用 `FaultRecovery` 枚举表达故障恢复点，替代 C 的地址范围比较。

**推理过程**：

C 中 `pagefault()` 通过比较 `frame->eip` 与函数地址范围判断是否在 `phys_copy` 中：

```c
in_physcopy = (frame->eip > (vir_bytes) phys_copy) &&
              (frame->eip < (vir_bytes) phys_copy_fault);
```

这是典型的"用指令指针地址范围判断执行上下文"——不安全、不可维护、依赖链接器分配的地址。Rust 有更好的表达方式：

**方案 A：全局故障恢复上下文（推荐）**

```rust
enum FaultContext {
    Normal,
    PhysCopy,
    Memset,
    UserCopyMsg,
    FpuRestore,
}

static mut CURRENT_FAULT_CONTEXT: FaultContext = FaultContext::Normal;
```

在进入 `phys_copy` 等函数前设置上下文，退出时恢复。`pagefault()` 检查上下文而非地址范围。这与 C 的 `catch_pagefaults` 计数器思路类似，但更精确——不仅知道"在可捕获页错误中"，还知道"在哪个具体函数中"。

**为什么用 `static mut` 而非更安全的方式**：单线程内核假设下，`static mut` 是安全的。`CURRENT_FAULT_CONTEXT` 只在中断处理路径中读写，不会被并发访问。使用 `Cell<FaultContext>` 需要包装为 `UnsafeCell`，在 `no_std` 下不如 `static mut` 直接。

**为什么不用 `catch_pagefaults` 计数器**：C 的 `catch_pagefaults` 只能表达"在/不在可捕获上下文中"，不能区分具体是哪个函数。Rust 的枚举可以精确表达上下文类型，让恢复路径更清晰。

**64 位演进影响**：§2.5.1 指出 `phys_copy`/`phys_memset` 可能被 Direct Map 取代。如果 Direct Map 消除了物理拷贝中的页错误可能，`FaultContext::PhysCopy` 和 `FaultContext::Memset` 可以删除。但 `UserCopyMsg` 仍然需要——内核在拷贝 IPC 消息时，用户态指针可能无效。

### 3.7 64 位架构演进

**C 源码依据**：§2.2.1 分析的 `exception_frame` 是 32 位结构。§2.5.5 分析的 8259A 初始化在 64 位下被 APIC 取代。

**决策**：minix-rs 仅考虑 64 位现代硬件，32 位遗留机制全部删除。

**32 位 → 64 位变化清单**：

| 方面 | Minix3 (32 位) | minix-rs (64 位) | 影响 |
|------|---------------|-----------------|------|
| 异常帧 | `exception_frame`（7 个 `reg_t` 字段） | x86-64：RIP/RFLAGS/RSP 等扩展为 64 位，新增 CS/SS 段寄存器压栈 | 异常帧结构完全重写 |
| 中断控制器 | 8259A PIC（16 个 IRQ） | LAPIC + IOAPIC（最多 256 个 IRQ） | `InterruptController` 实现从 8259A 切换到 APIC |
| IRQ 向量数 | `NR_IRQ_VECTORS = 16` (PIC) / `64` (APIC) | 64 位系统统一使用 APIC，`NR_IRQ_VECTORS = 64` | 常量统一 |
| 错误码 | 32 位 `errcode` | 64 位，页错误码格式不变（bit 0=P, bit 1=W/R, bit 2=U/S） | 页错误码解析逻辑不变 |
| CR2 | 32 位 `read_cr2()` | 64 位 `read_cr2()` | 返回值从 `reg_t`(u32) 变为 u64 |
| `hw_intr` 宏 | 编译时选择 PIC/APIC | trait 运行时多态 | 更灵活，支持检测 APIC 存在后动态选择 |
| FPU 异常 | `copr_not_available_handler`（设备不可用异常向量 7） | x86-64 上 CR0.TS 机制仍存在，但 FPU 指令集扩展（SSE/AVX） | 保留 `DEVICE_NOT_AVAILABLE` 向量处理 |

**8259A 删除的理由**：x86-64 系统标配 LAPIC + IOAPIC。8259A 仅在非常旧的硬件上使用，且 Intel 已在较新处理器中废弃 8259A（需要软件兼容模式模拟）。64 位内核应直接使用 APIC。

### 3.8 IrqVector 与 InterruptVector 的区分

**C 源码依据**：§2.1 分析了中断向量号的定义——CPU 异常（0-19）、系统调用（32-35）、硬件中断（0x50-0x77）。§2.5.5 分析了 `IRQ0_VECTOR = 0x50` 的偏移映射。

**决策**：定义两种向量号类型，明确区分 IDT 向量号和硬件 IRQ 号。

| 类型 | 含义 | 范围 | 使用场景 |
|------|------|------|---------|
| `InterruptVector` | IDT 向量号（已在 04-protection.md §3.8 定义） | 0-255 | 异常帧中的向量号、IDT 门描述符索引 |
| `IrqVector` | 硬件 IRQ 号 | 0-NR_IRQ_VECTORS-1 | `InterruptController` 操作、`IrqManager` 管理 |

**为什么需要两种类型**：x86-64 上 IRQ 0 对应 IDT 向量 0x50，存在偏移映射。ARM64 和 RISC-V 上 IRQ 号和异常向量号是不同的命名空间。如果只用一种类型，OS 代码需要知道"这个向量号是 IRQ 还是异常"，容易混淆。两种 newtype 在类型层面防止误用。

**映射方法**：x86-64 的 `InterruptController` 实现提供 `irq_to_vector()` 和 `vector_to_irq()` 内部方法，将 `IrqVector` 映射到 `InterruptVector`。这是实现细节，不暴露给 OS 层。

### 3.9 错误码对齐

**C 源码依据**：§2.3.2 分析了 `pagefault()` 中 `mini_send()` 失败时直接 panic。§2.3.3 分析了 `put_irq_handler()` 中无效 IRQ 号直接 panic。

**决策**：异常/中断处理中的不可恢复错误直接 panic（与 C 一致）。可恢复错误返回 `Result`。

**错误码映射**：

| 场景 | Minix3 行为 | Rust 行为 |
|------|-----------|----------|
| 无效 IRQ 号 | `panic("invalid call to put_irq_handler: %d", irq)` | `panic!` — 同 C |
| 钩子 ID 耗尽 | `panic("Too many handlers for irq: %d", irq)` | `panic!` — 同 C |
| VM 页错误 | `panic("pagefault in VM")` | `panic!` — 同 C |
| `mini_send` 失败 | `panic("WARNING: pagefault: mini_send returned %d")` | `panic!` — 同 C |
| 注册钩子时进程无效 | `panic("invalid interrupt handler")` | `panic!` — 同 C |
| 伪中断 | 打印警告，保持屏蔽 | 返回 `Err(IrqError::Spurious)`，OS 层决定处理方式 |

**伪中断处理的变化**：C 中 `irq_handle()` 对伪中断打印警告并保持 IRQ 屏蔽。Rust 中返回 `Err(IrqError::Spurious)`，让调用者决定处理方式（打印日志、计数、重新屏蔽等）。这是"策略与机制分离"的体现——`IrqManager` 提供机制（检测伪中断），OS 层决定策略（如何处理）。

### 3.10 BKL 保护下的单线程假设

**C 源码依据**：`exception_handler()` 和 `irq_handle()` 在内核主循环中被调用，此时内核持有 BKL（Big Kernel Lock），无并发问题。`irq_hooks[]` 和 `irq_actids[]` 是全局数组，无锁保护。

**决策**：异常/中断处理代码假设单线程执行，不引入同步机制。`IrqManager` 不是 `Send`/`Sync`——它只在内核主循环中使用。

**推理过程**：Minix3 内核是非抢占的——在中断处理期间，CPU 不会切换到另一个进程。`irq_handle()` 中的 `irq_actids[]` 读写不需要原子操作，因为只有当前 CPU 会访问。APIC 环境下，每个 CPU 有自己的 LAPIC，但 IRQ 钩子链仍然是全局共享的——Minix3 的 `irq_handle()` 假设同一 IRQ 不会在多个 CPU 上同时触发。这个假设在 minix-rs 中保持不变。

---

## 4. 实现详解

> 每个结构的引导语解释核心思路，代码注释标注 C 源码对应。实现对应 Ch3 的设计决策。

### 4.1 InterruptController trait

> 设计决策：§3.2（中断控制器抽象）、§3.7（64 位 APIC 替代 8259A）

`InterruptController` 抽象中断控制器的硬件操作——mask/unmask/ack/eoi。OS 代码通过此 trait 管理中断控制器，无需了解 8259A/IOAPIC/GIC/PLIC 的寄存器细节。

```rust
/// Architecture abstraction for interrupt controller operations.
///
/// Manages masking, unmasking, acknowledging, and signaling end-of-interrupt
/// for hardware interrupt lines.
///
/// # Architecture mapping
///
/// | Method       | x86-64 (APIC)        | ARM64 (GICv3)      | RISC-V (PLIC)    |
/// |-------------|----------------------|--------------------|--------------------|
/// | `init()`    | Initialize LAPIC +   | Initialize GIC     | Initialize PLIC    |
/// |             | IOAPIC, mask all     | distributor +      | + CLINT, mask all  |
/// |             |                      | redistributors     |                    |
/// | `mask()`    | IOAPIC mask bit      | GICD_ICENABLER     | PLIC enable=0      |
/// | `unmask()`  | IOAPIC unmask bit    | GICD_ISENABLER     | PLIC enable=1      |
/// | `ack()`     | LAPIC EOI            | Read IAR (ACK)     | Read claim (ACK)   |
/// | `eoi()`     | LAPIC EOI write      | Write EOIR         | Write complete     |
/// | `mask_all()`| IOAPIC mask all      | GICD_ICENABLER=all | PLIC threshold=max |
pub trait InterruptController: Sized {
    /// Initialize the interrupt controller.
    ///
    /// Called once during kernel startup. After this call, all IRQ lines
    /// are masked and no interrupts will be delivered.
    ///
    /// C: intr_init() — i8259.c:28 (PIC) / apic.c (APIC)
    fn init(&mut self);

    /// Mask (disable) an IRQ line.
    ///
    /// After this call, the specified IRQ will not generate interrupts.
    ///
    /// C: hw_intr_mask(irq) — hw_intr.h:22/45
    fn mask(&mut self, irq: IrqVector);

    /// Unmask (enable) an IRQ line.
    ///
    /// After this call, the specified IRQ will generate interrupts.
    ///
    /// C: hw_intr_unmask(irq) — hw_intr.h:23/46
    fn unmask(&mut self, irq: IrqVector);

    /// Acknowledge receipt of an interrupt.
    ///
    /// On some architectures, this reads the interrupt ID from the
    /// controller (e.g., ARM64 GIC IAR register). On x86-64, this
    /// is the same as `eoi()`.
    ///
    /// C: hw_intr_ack(irq) — hw_intr.h:24/47
    fn ack(&mut self, irq: IrqVector);

    /// Signal end-of-interrupt processing.
    ///
    /// Called after all handlers for this IRQ have completed.
    /// On x86-64, this writes to the LAPIC EOI register.
    ///
    /// C: hw_intr_ack(irq) — hw_intr.h:24/47
    fn eoi(&mut self, irq: IrqVector);

    /// Mask all IRQ lines.
    ///
    /// Called during early boot to ensure no interrupts fire
    /// before handlers are registered.
    ///
    /// C: hw_intr_disable_all() — hw_intr.h:33/50
    fn mask_all(&mut self);
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 | minix-rs |
|------|--------|----------|
| 初始化 | `intr_init()` 全局函数 | `InterruptController::init()` 方法 |
| 屏蔽 IRQ | `hw_intr_mask(irq)` 宏 | `mask(irq)` 方法 |
| 启用 IRQ | `hw_intr_unmask(irq)` 宏 | `unmask(irq)` 方法 |
| 确认中断 | `hw_intr_ack(irq)` 宏 | `ack(irq)` + `eoi(irq)` 方法 |
| 屏蔽全部 | `hw_intr_disable_all()` 宏 | `mask_all()` 方法 |
| 运行时选择 PIC/APIC | `#ifdef USE_APIC` 编译时 | trait 运行时多态 |

### 4.2 ExceptionArch trait

> 设计决策：§3.3（异常帧解析与故障恢复）、§3.7（64 位异常帧变化）

`ExceptionArch` 抽象异常帧的解析和故障恢复点操作。异常帧作为关联类型，因为不同架构的布局完全不同。OS 代码通过此 trait 访问异常信息，无需了解 x86-64 的 `exception_frame` 或 ARM64 的 `ESR_EL1`/`FAR_EL1`。

```rust
/// Architecture abstraction for exception frame parsing and fault recovery.
///
/// Provides methods to extract information from the CPU-pushed exception
/// frame and to modify it for fault recovery (e.g., redirecting execution
/// to a recovery point after a nested page fault in phys_copy).
///
/// # Architecture mapping
///
/// | Method                  | x86-64                    | ARM64              | RISC-V          |
/// |------------------------|---------------------------|--------------------|-----------------|
/// | `vector()`             | frame->vector             | ESR_EL1.EC         | scause.code      |
/// | `error_code()`         | frame->errcode            | ESR_EL1.ISS        | stval            |
/// | `instruction_pointer()`| frame->rip                | ELR_EL1            | sepc             |
/// | `is_user_mode()`       | frame->cs & 3             | SPSR_EL1.M[3:0]    | sstatus.SPP      |
/// | `page_fault_address()` | read CR2                  | read FAR_EL1       | read stval       |
/// | `set_instruction_      | frame->rip = addr         | ELR_EL1 = addr     | sepc = addr      |
/// |  pointer()`            |                           |                    |                  |
/// | `set_return_value()`   | p_reg.retreg = value      | x0 = value         | a0 = value       |
pub trait ExceptionArch {
    /// Architecture-specific exception frame type.
    ///
    /// x86-64: struct with vector, errcode, rip, cs, rflags, rsp, ss
    /// ARM64:  struct with esr, far, elr, spsr, sp
    /// RISC-V: struct with scause, stval, sepc, sstatus, sp
    type Frame;

    /// Get the interrupt/exception vector number from the frame.
    ///
    /// C: frame->vector — exception.c:182
    fn vector(frame: &Self::Frame) -> InterruptVector;

    /// Get the error code pushed by the CPU (or 0 if none).
    ///
    /// C: frame->errcode — exception.c:183
    fn error_code(frame: &Self::Frame) -> u64;

    /// Get the instruction pointer where the exception occurred.
    ///
    /// C: frame->eip — exception.c:186
    fn instruction_pointer(frame: &Self::Frame) -> VirBytes;

    /// Check whether the exception occurred in user mode.
    ///
    /// C: (frame->cs & 3) == USER_PRIVILEGE — exception.c:191
    fn is_user_mode(frame: &Self::Frame) -> bool;

    /// Read the page fault address from the CPU's fault address register.
    ///
    /// On x86-64, this reads CR2. On ARM64, this reads FAR_EL1.
    /// On RISC-V, this reads stval.
    ///
    /// C: read_cr2() — exception.c:59
    fn page_fault_address() -> VirBytes;

    /// Check if the page fault was caused by a write access.
    ///
    /// On x86-64, checks bit 1 of the error code.
    /// On ARM64, checks the WnR bit of ESR_EL1.ISS.
    ///
    /// C: (frame->errcode & 2) — interpreted by VM
    fn is_write_fault(frame: &Self::Frame) -> bool;

    /// Modify the instruction pointer in the frame for fault recovery.
    ///
    /// Used by nested exception handlers to redirect execution to
    /// a recovery point (e.g., phys_copy_fault_in_kernel).
    ///
    /// C: frame->eip = (reg_t) phys_copy_fault_in_kernel — exception.c:68
    fn set_instruction_pointer(frame: &mut Self::Frame, ip: VirBytes);

    /// Set a return value in the frame (architecture-specific register).
    ///
    /// On x86-64, this sets the return register (eax/rax).
    /// Used to pass the fault address back to the recovery point.
    ///
    /// C: pr->p_reg.retreg = pagefaultcr2 — exception.c:72
    fn set_return_value(frame: &mut Self::Frame, value: u64);
}
```

**为什么 `set_return_value` 操作的是 `Frame` 而非 `KProcess`**：C 中 `pr->p_reg.retreg = pagefaultcr2` 修改的是进程的寄存器保存区，而非异常帧。但在 Rust 中，非嵌套异常时异常帧就是进程的寄存器保存区（CPU 在异常入口将寄存器压入内核栈，调度器在切换时保存/恢复同一区域）。嵌套异常时，异常帧是栈上的临时数据，修改它不影响进程状态——这正是 C 中 `is_nested` 分支修改 `frame->eip` 而非 `pr->p_reg.pc` 的原因。因此 `set_return_value` 的行为取决于是否嵌套，由调用者（`ExceptionDispatcher`）控制。

### 4.3 IrqVector 与 IrqPolicy 类型

> 设计决策：§3.8（IrqVector 与 InterruptVector 区分）、§3.4（IrqPolicy bitflags）

```rust
/// Hardware IRQ vector number.
///
/// Distinct from `InterruptVector` (IDT vector index). On x86-64,
/// `IrqVector(0)` maps to `InterruptVector(0x50)` via `IRQ0_VECTOR`.
/// On ARM64/RISC-V, the mapping is architecture-specific.
///
/// C: interrupt.h:35-37 (NR_IRQ_VECTORS)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IrqVector(pub u8);

impl IrqVector {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

/// IRQ hook identifier (bitmask for active tracking).
///
/// Each hook on an IRQ line gets a unique bit in the `irq_actids` bitmap.
/// Allocated by finding the lowest unset bit.
///
/// C: hook->id — type.h:22
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqId(u32);

/// IRQ policy flags.
///
/// C: IRQ_REENABLE — com.h:308
bitflags::bitflags! {
    pub struct IrqPolicy: u32 {
        /// Re-enable IRQ line after handler returns Completed.
        const REENABLE = 0x001;
    }
}

/// IRQ handler action (return value from handler callback).
///
/// C: generic_handler() returns hook->policy & IRQ_REENABLE
///    (non-zero = completed, zero = not completed)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqAction {
    /// Handler finished processing; clear active bit.
    Completed,
    /// Handler still processing; keep active bit set.
    NotCompleted,
}
```

**C 行为 vs Rust 行为对照**：

| C 概念 | Rust 类型 | 变化 |
|--------|----------|------|
| `int irq` | `IrqVector(u8)` | newtype 替代裸 int |
| `int id` (bitmask) | `IrqId(u32)` | newtype 替代裸 int |
| `irq_policy_t` (unsigned long) | `IrqPolicy` (bitflags) | bitflags 替代裸 unsigned long |
| handler 返回 `int` (0=not done, non-zero=done) | `IrqAction` 枚举 | 枚举替代哨兵值 |
| `irq_id_t` (unsigned long) | `IrqNotifyId(u32)` | newtype 替代裸 unsigned long |

### 4.4 IrqManager — IRQ 钩子管理

> 设计决策：§3.4（架构无关的 IRQ 钩子管理）、§3.10（单线程假设）

`IrqManager` 管理 IRQ 钩子链——注册、移除、分发、活跃追踪。核心逻辑与 C 的 `interrupt.c` 完全等价，但用 Rust 类型系统消除哨兵值和裸整数。

```rust
/// Maximum number of IRQ hooks (system-wide).
///
/// C: NR_IRQ_HOOKS — config.h:59/61
const NR_IRQ_HOOKS: usize = 64;

/// Maximum number of IRQ vectors.
///
/// C: NR_IRQ_VECTORS — interrupt.h:35-37
/// 64-bit: always 64 (APIC mode)
const NR_IRQ_VECTORS: usize = 64;

/// Bitmap for tracking active IRQ hooks per vector.
///
/// C: irq_actids[NR_IRQ_VECTORS] — glo.h:49
type IrqIdBitmap = u32;

/// An IRQ hook slot in the global hook pool.
struct IrqHookSlot {
    /// Index of the next hook in the chain, or None if last.
    next: Option<usize>,
    /// Handler callback function.
    handler: fn(IrqVector, IrqId) -> IrqAction,
    /// IRQ vector this hook is registered for.
    irq: IrqVector,
    /// Unique bitmask ID for active tracking.
    id: IrqId,
    /// Endpoint of the registering process.
    proc_endpoint: Endpoint,
    /// Notification ID returned to the driver.
    notify_id: IrqNotifyId,
    /// Policy flags (e.g., IRQ_REENABLE).
    policy: IrqPolicy,
}

/// Architecture-independent IRQ hook chain manager.
///
/// Manages registration, removal, and dispatch of IRQ handlers.
/// Uses a fixed-size hook pool and per-vector chain heads.
///
/// C: interrupt.c — put_irq_handler(), rm_irq_handler(), irq_handle()
pub struct IrqManager<IC: InterruptController> {
    /// Global hook pool. None = free slot.
    hooks: [Option<IrqHookSlot>; NR_IRQ_HOOKS],
    /// Per-vector chain head index. None = no handlers.
    handlers: [Option<usize>; NR_IRQ_VECTORS],
    /// Per-vector active ID bitmap.
    actids: [IrqIdBitmap; NR_IRQ_VECTORS],
    /// Bitmask of IRQ vectors that have at least one handler.
    irq_use: u64,
    /// Interrupt controller instance.
    controller: IC,
}
```

**核心方法实现对照**：

#### 4.4.1 register_hook() — 对应 put_irq_handler()

```rust
impl<IC: InterruptController> IrqManager<IC> {
    /// Register an IRQ handler.
    ///
    /// Allocates the lowest unused bit ID for the new hook,
    /// appends it to the chain for the given IRQ, and unmask
    /// the IRQ if this is the first handler.
    ///
    /// C: put_irq_handler() — interrupt.c:29-73
    pub fn register_hook(
        &mut self,
        irq: IrqVector,
        handler: fn(IrqVector, IrqId) -> IrqAction,
        proc_endpoint: Endpoint,
        notify_id: IrqNotifyId,
        policy: IrqPolicy,
    ) -> Result<IrqId, IrqError> {
        let irq_idx = irq.get() as usize;
        if irq_idx >= NR_IRQ_VECTORS {
            panic!("invalid IRQ vector: {}", irq.get());
        }

        // Walk the chain to find tail and collect used IDs
        let mut bitmap: IrqIdBitmap = 0;
        let mut slot_idx = self.handlers[irq_idx];
        while let Some(idx) = slot_idx {
            let slot = self.hooks[idx].as_ref().unwrap();
            bitmap |= slot.id.0;
            slot_idx = slot.next;
        }

        // Allocate lowest unused bit
        let mut id = 1u32;
        while id != 0 && (bitmap & id) != 0 {
            id <<= 1;
        }
        if id == 0 {
            panic!("too many handlers for IRQ {}", irq.get());
        }

        // Find a free slot in the global pool
        let free_idx = self.hooks.iter().position(|s| s.is_none())
            .ok_or(IrqError::NoFreeSlots)?;

        self.hooks[free_idx] = Some(IrqHookSlot {
            next: None,
            handler,
            irq,
            id: IrqId(id),
            proc_endpoint,
            notify_id,
            policy,
        });

        // Append to chain tail
        self.append_to_chain(irq_idx, free_idx);

        // If first handler for this IRQ, unmask it
        if (self.actids[irq_idx] & id) == 0 {
            self.controller.unmask(irq);
        }

        Ok(IrqId(id))
    }
}
```

#### 4.4.2 dispatch() — 对应 irq_handle()

```rust
impl<IC: InterruptController> IrqManager<IC> {
    /// Dispatch an IRQ to all registered handlers.
    ///
    /// Masks the IRQ, walks the handler chain, tracks active IDs,
    /// and unmasks the IRQ when all handlers have completed.
    /// Sends EOI after all handlers finish.
    ///
    /// C: irq_handle() — interrupt.c:116-140
    pub fn dispatch(&mut self, irq: IrqVector) -> Result<(), IrqError> {
        let irq_idx = irq.get() as usize;
        if irq_idx >= NR_IRQ_VECTORS {
            return Err(IrqError::InvalidIrq);
        }

        // C: hw_intr_mask(irq)
        self.controller.mask(irq);

        // C: hook = irq_handlers[irq]
        let mut slot_idx = self.handlers[irq_idx];
        if slot_idx.is_none() {
            // Spurious interrupt — keep masked
            return Err(IrqError::Spurious(irq));
        }

        // C: while (hook != NULL) { ... }
        while let Some(idx) = slot_idx {
            let slot = self.hooks[idx].as_ref().unwrap();

            // C: irq_actids[irq] |= hook->id
            self.actids[irq_idx] |= slot.id.0;

            // C: if ((*hook->handler)(hook)) irq_actids[hook->irq] &= ~hook->id
            let action = (slot.handler)(irq, slot.id);
            if action == IrqAction::Completed {
                self.actids[irq_idx] &= !slot.id.0;
            }

            slot_idx = slot.next;
        }

        // C: if (irq_actids[irq] == 0) hw_intr_unmask(irq)
        if self.actids[irq_idx] == 0 {
            self.controller.unmask(irq);
        }

        // C: hw_intr_ack(irq)
        self.controller.eoi(irq);

        Ok(())
    }
}
```

**与 C 的关键差异**：

| 方面 | Minix3 C | minix-rs Rust |
|------|---------|--------------|
| 钩子链 | 指针链表 `irq_hook_t *next` | 索引链表 `Option<usize>` next |
| 全局池 | `irq_hooks[NR_IRQ_HOOKS]` + `proc_nr_e == NONE` 判断空闲 | `[Option<IrqHookSlot>]` |
| handler 返回值 | `int`（0=未完成，非零=完成） | `IrqAction` 枚举 |
| 伪中断 | 打印警告，保持屏蔽 | `Err(IrqError::Spurious)` |
| ID 分配 | `for (id = 1; id != 0; id <<= 1)` | 同样逻辑，但返回 `IrqId` newtype |

### 4.5 ExceptionDispatcher — 异常分发

> 设计决策：§3.5（异常分发与页错误转发）、§3.6（嵌套异常容错）

`ExceptionDispatcher` 封装 `exception_handler()` 的分发逻辑——根据异常向量号和来源（用户态/内核态）走不同路径。

```rust
/// Exception classification for dispatch.
///
/// C: ex_data[] — exception.c:19-39
enum ExceptionClass {
    /// Vector 2: Non-maskable interrupt — log and ignore.
    NonMaskableInterrupt,
    /// Vector 14: Page fault — forward to VM process.
    PageFault,
    /// Vector 1: Debug exception — special handling for traced processes.
    Debug,
    /// All other exceptions — deliver signal to user process.
    Signal(Signal),
}

/// Fault recovery context for nested exceptions.
///
/// C: catch_pagefaults + address range comparison — exception.c:59-73
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaultContext {
    Normal,
    PhysCopy,
    Memset,
    UserCopyMsg,
    FpuRestore,
}

/// Architecture-independent exception dispatcher.
///
/// Dispatches exceptions based on vector number and fault context.
/// Page faults are forwarded to VM; user-mode exceptions generate
/// signals; kernel-mode exceptions in recoverable contexts redirect
/// execution; all other kernel exceptions panic.
///
/// C: exception_handler() — exception.c:180-283
pub struct ExceptionDispatcher<EA: ExceptionArch> {
    _phantom: core::marker::PhantomData<EA>,
}

impl<EA: ExceptionArch> ExceptionDispatcher<EA> {
    /// Main exception dispatch entry point.
    ///
    /// C: exception_handler(is_nested, frame) — exception.c:180
    pub fn handle(
        frame: &mut EA::Frame,
        is_nested: bool,
        current_proc: &mut KProcess,
        fault_ctx: FaultContext,
    ) -> ExceptionOutcome {
        let vector = EA::vector(frame);
        let is_user = EA::is_user_mode(frame);

        // C: vector 2 (NMI) — exception.c:185
        if vector.get() == 2 {
            return ExceptionOutcome::SpuriousNmi;
        }

        // C: is_nested special cases — exception.c:189-249
        if is_nested {
            return Self::handle_nested(frame, vector, current_proc, fault_ctx);
        }

        // C: vector 14 (page fault) — exception.c:251
        if vector.get() == 14 {
            return Self::handle_page_fault(frame, is_nested, current_proc, fault_ctx);
        }

        // C: !is_nested && user mode — exception.c:224
        if is_user {
            let class = Self::classify(vector);
            return ExceptionOutcome::Signal(current_proc.endpoint(), class);
        }

        // C: kernel mode, not nested — inkernel_disaster() — exception.c:226
        ExceptionOutcome::KernelPanic(vector)
    }

    /// Handle nested (kernel-mode) exception.
    ///
    /// C: exception.c:189-249
    fn handle_nested(
        frame: &mut EA::Frame,
        vector: InterruptVector,
        _proc: &mut KProcess,
        fault_ctx: FaultContext,
    ) -> ExceptionOutcome {
        // C: copy_msg_to_user/copy_msg_from_user — exception.c:205-216
        if fault_ctx == FaultContext::UserCopyMsg {
            let is_pf_or_gpf = vector.get() == 14 || vector.get() == 13;
            if is_pf_or_gpf {
                // C: frame->eip = __user_copy_msg_pointer_failure
                // Redirect to IPC fault recovery point
                return ExceptionOutcome::RedirectToRecovery(
                    RecoveryPoint::UserCopyMsgFailure,
                );
            }
        }

        // C: fxrstor/frstor — exception.c:220-228
        if fault_ctx == FaultContext::FpuRestore {
            // C: frame->eip = __frstor_failure
            return ExceptionOutcome::RedirectToRecovery(
                RecoveryPoint::FpuRestoreFailure,
            );
        }

        // C: debug exception + traced process — exception.c:218-220
        if vector.get() == 1 {
            // Clear TF bit and return
            return ExceptionOutcome::ClearTrapFlag;
        }

        // C: other nested exceptions — inkernel_disaster()
        ExceptionOutcome::KernelPanic(vector)
    }

    /// Handle page fault.
    ///
    /// C: pagefault() — exception.c:49-130
    fn handle_page_fault(
        frame: &mut EA::Frame,
        is_nested: bool,
        proc: &mut KProcess,
        fault_ctx: FaultContext,
    ) -> ExceptionOutcome {
        let fault_addr = EA::page_fault_address();
        let is_write = EA::is_write_fault(frame);

        // C: catch_pagefaults && (in_physcopy || in_memset) — exception.c:62-73
        if (fault_ctx == FaultContext::PhysCopy || fault_ctx == FaultContext::Memset) {
            if is_nested {
                // C: frame->eip = phys_copy_fault_in_kernel / memset_fault_in_kernel
                let recovery = if fault_ctx == FaultContext::PhysCopy {
                    RecoveryPoint::PhysCopyFaultInKernel
                } else {
                    RecoveryPoint::MemsetFaultInKernel
                };
                return ExceptionOutcome::RedirectToRecovery(recovery);
            } else {
                // C: pr->p_reg.pc = phys_copy_fault; pr->p_reg.retreg = cr2
                return ExceptionOutcome::PhysCopyFault { fault_addr };
            }
        }

        // C: is_nested (not in recoverable context) — inkernel_disaster()
        if is_nested {
            return ExceptionOutcome::KernelPanic(InterruptVector::new(14));
        }

        // C: pr == VM_PROC_NR — panic("pagefault in VM")
        if proc.is_vm() {
            return ExceptionOutcome::VmPageFault;
        }

        // C: RTS_SET(pr, RTS_PAGEFAULT) + mini_send(VM)
        ExceptionOutcome::ForwardToVm(VmPagefaultIn {
            endpoint: proc.endpoint(),
            vaddr: fault_addr,
            write: is_write,
        })
    }

    /// Classify an exception vector into a dispatch category.
    ///
    /// C: ex_data[] — exception.c:19-39
    fn classify(vector: InterruptVector) -> ExceptionClass {
        match vector.get() {
            2 => ExceptionClass::NonMaskableInterrupt,
            14 => ExceptionClass::PageFault,
            1 => ExceptionClass::Debug,
            0 => ExceptionClass::Signal(Signal::FPE),   // Divide error
            6 => ExceptionClass::Signal(Signal::ILL),   // Invalid opcode
            13 => ExceptionClass::Signal(Signal::SEGV), // General protection
            _ => ExceptionClass::Signal(Signal::SEGV),  // Default
        }
    }
}

/// Outcome of exception dispatch.
///
/// Replaces C's mix of returns, panics, and direct function calls
/// with a structured enum that the caller can pattern-match on.
enum ExceptionOutcome {
    /// Spurious NMI — log and continue.
    SpuriousNmi,
    /// User-mode exception — deliver signal.
    Signal(Endpoint, ExceptionClass),
    /// Page fault — forward to VM process.
    ForwardToVm(VmPagefaultIn),
    /// Nested exception — redirect to recovery point.
    RedirectToRecovery(RecoveryPoint),
    /// Phys copy fault — set return value and redirect.
    PhysCopyFault { fault_addr: VirBytes },
    /// VM process page fault — panic.
    VmPageFault,
    /// Kernel panic — unrecoverable kernel-mode exception.
    KernelPanic(InterruptVector),
    /// Debug exception — clear trap flag.
    ClearTrapFlag,
}

/// Fault recovery points for nested exception handling.
///
/// C: phys_copy_fault_in_kernel, memset_fault_in_kernel,
///    __user_copy_msg_pointer_failure, __frstor_failure
enum RecoveryPoint {
    PhysCopyFaultInKernel,
    MemsetFaultInKernel,
    UserCopyMsgFailure,
    FpuRestoreFailure,
}
```

**C 行为 vs Rust 行为对照**：

| 方面 | Minix3 C | minix-rs Rust |
|------|---------|--------------|
| 分发逻辑 | `exception_handler()` 直接调用 `cause_sig()`/`pagefault()`/`inkernel_disaster()` | 返回 `ExceptionOutcome` 枚举，调用者决定后续操作 |
| 嵌套异常恢复 | 修改 `frame->eip` 跳转到 C 标签 | 返回 `RedirectToRecovery(RecoveryPoint)` |
| 页错误转发 | 直接调用 `mini_send()` | 返回 `ForwardToVm(VmPagefaultIn)` |
| 异常分类 | `ex_data[]` 静态数组 | `classify()` match 表达式 |
| 伪 NMI | 打印 "spurious NMI" 并返回 | `SpuriousNmi` 变体 |
| 嵌套调试异常 | 仅当 `TRACEBIT && KTS_NONE` 时清除 TF 位 | 同：`is_traced && kern_trap_style == None` → `ClearTrapFlag`，否则 `KernelPanic` |

**为什么 `ExceptionDispatcher` 返回枚举而非直接操作**：C 的 `exception_handler()` 直接调用 `cause_sig()`、`mini_send()`、`inkernel_disaster()` 等函数，导致异常处理与进程管理/IPC 紧耦合。Rust 将"决定做什么"（分发）和"执行决定"（操作）分离——`ExceptionDispatcher` 只决定做什么，调用者负责执行。这是"策略与机制分离"的体现。

### 4.6 x86-64 InterruptController 实现

> 设计决策：§3.2（InterruptController trait）、§3.7（64 位 APIC）

x86-64 使用 LAPIC + IOAPIC 作为中断控制器。LAPIC 负责接收中断、EOI；IOAPIC 负责路由外部设备中断到特定 CPU。

```rust
/// x86-64 APIC-based interrupt controller.
///
/// Combines Local APIC (per-CPU) and I/O APIC (system-wide) operations.
///
/// C: i8259.c (PIC) / apic.c (APIC)
pub struct X86_64InterruptController {
    /// Number of IRQ vectors supported.
    nr_irq_vectors: usize,
    /// IOAPIC register base addresses.
    ioapic_bases: [u64; 2],
}

impl InterruptController for X86_64InterruptController {
    fn init(&mut self) {
        // C: intr_init() — i8259.c:28
        // 64-bit: Initialize LAPIC + IOAPIC, mask all IRQs
        self.init_lapic();
        self.init_ioapic();
        self.mask_all();
    }

    fn mask(&mut self, irq: IrqVector) {
        // C: ioapic_mask_irq(irq) — apic.c
        self.ioapic_set_mask(irq.get(), true);
    }

    fn unmask(&mut self, irq: IrqVector) {
        // C: ioapic_unmask_irq(irq) — apic.c
        self.ioapic_set_mask(irq.get(), false);
    }

    fn ack(&mut self, _irq: IrqVector) {
        // On x86-64, ACK and EOI are the same operation
        self.lapic_eoi();
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // C: ioapic_eoi(irq) / lapic_eoi() — apic.c
        self.lapic_eoi();
    }

    fn mask_all(&mut self) {
        // C: hw_intr_disable_all() — hw_intr.h:33
        for irq in 0..self.nr_irq_vectors as u8 {
            self.mask(IrqVector::new(irq));
        }
    }
}
```

**为什么 `ack()` 和 `eoi()` 实现相同**：x86-64 的 LAPIC 只有一个 EOI 寄存器，写入即表示"中断处理完成"。ARM64 的 GICv3 则区分 ACK（读取 IAR 获取中断 ID）和 EOI（写入 EOIR 通知完成）。x86-64 的 `ack()` 和 `eoi()` 都写 LAPIC EOI 寄存器，但保留两个方法是为了 trait 接口的通用性。

### 4.7 x86-64 ExceptionArch 实现

> 设计决策：§3.3（异常帧解析）、§3.7（64 位异常帧）

x86-64 的异常帧与 32 位有显著差异：所有寄存器扩展为 64 位，CPU 在特权级切换时自动压入 SS/RSP。

```rust
/// x86-64 exception frame.
///
/// Pushed by CPU (SS, RSP, RFLAGS, CS, RIP, Error Code) and
/// assembly entry (vector number).
///
/// C: exception_frame — arch_proto.h:72-80 (32-bit version)
/// 64-bit: RIP/RFLAGS/RSP are 64-bit; SS/CS are 16-bit but
/// stored in 64-bit slots for alignment.
#[repr(C)]
pub struct X86_64ExceptionFrame {
    pub vector: u64,
    pub errcode: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl ExceptionArch for X86_64ExceptionFrame {
    type Frame = Self;

    fn vector(frame: &Self::Frame) -> InterruptVector {
        InterruptVector::new(frame.vector as u8)
    }

    fn error_code(frame: &Self::Frame) -> u64 {
        frame.errcode
    }

    fn instruction_pointer(frame: &Self::Frame) -> VirBytes {
        VirBytes::new(frame.rip)
    }

    fn is_user_mode(frame: &Self::Frame) -> VirBytes {
        // C: (frame->cs & 3) == USER_PRIVILEGE
        (frame.cs & 3) == 3
    }

    fn page_fault_address() -> VirBytes {
        // C: read_cr2() — klib.S:521
        let cr2: u64;
        unsafe {
            core::arch::asm!(
                "mov {}, cr2",
                out(reg) cr2,
                options(nomem, nostack, preserves_flags)
            );
        }
        VirBytes::new(cr2)
    }

    fn is_write_fault(frame: &Self::Frame) -> bool {
        // x86-64 page fault error code bit 1 = Write
        (frame.errcode & 2) != 0
    }

    fn set_instruction_pointer(frame: &mut Self::Frame, ip: VirBytes) {
        // C: frame->eip = (reg_t) phys_copy_fault_in_kernel
        frame.rip = ip.get();
    }

    fn set_return_value(frame: &mut Self::Frame, value: u64) {
        // C: pr->p_reg.retreg = pagefaultcr2
        // In x86-64, return value register is RAX.
        // This modifies the saved RAX in the exception frame.
        frame.rax = value;
    }
}
```

**注意**：`X86_64ExceptionFrame` 的 `set_return_value` 修改 `frame.rax`，但当前结构体定义中没有 `rax` 字段。这是因为 x86-64 的通用寄存器保存区不在异常帧中——异常帧只包含 CPU 自动压入的寄存器，通用寄存器由汇编入口在异常帧之前保存。完整的寄存器保存区需要扩展结构体或引用进程的 `p_reg` 区域。这是一个待完善的设计点。

### 4.8 启动流程集成

异常/中断处理在内核启动的以下阶段被初始化：

```rust
/// Kernel main — called after paging is enabled.
///
/// C: main() → cstart() → prot_init() + intr_init()
fn kmain(kernel_info: &KernelInfo) -> ! {
    // Step 1: Initialize protection (GDT/TSS)
    // C: prot_init() — protect.c:321
    let protection = X86_64Protection::init(0, kernel_stack_top);
    protection.load();

    // Step 2: Initialize trap entry (IDT)
    // C: idt_init() — protect.c:260
    let mut trap_entry = X86_64TrapEntry::init();
    trap_entry.configure_syscall(syscall_entry_point);
    trap_entry.load();

    // Step 3: Initialize interrupt controller
    // C: intr_init() — i8259.c:28
    let mut irq_manager = IrqManager::new(X86_64InterruptController::new());

    // Step 4: Enable interrupts
    // After this point, hardware interrupts can fire
    unsafe { core::arch::asm!("sti") };

    // Step 5: Main kernel loop
    loop {
        // Wait for interrupt or exception
        // C: main() loop — main.c
    }
}
```

**初始化时序**：

1. `ProtectionArch::init()` + `load()` 必须在 `TrapEntryArch::init()` 之前——IDT 门描述符引用的段选择符必须在 GDT 中有效
2. `TrapEntryArch::init()` + `load()` 必须在 `InterruptController::init()` 之前——中断控制器初始化后 IRQ 才能被正确路由
3. `InterruptController::init()` 屏蔽所有 IRQ——直到驱动通过 `IrqManager::register_hook()` 注册钩子后，IRQ 才被启用
4. `sti` 指令在所有初始化完成后执行——在此之前，CPU 不响应外部中断

---

## 5. 测试要点

### 5.1 InterruptController trait 测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| `init()` 后所有 IRQ 被屏蔽 | 任何 IRQ 不产生中断 | §3.2 |
| `mask()`/`unmask()` 配对 | 屏蔽后不产生中断，解除后产生 | §3.2 |
| `eoi()` 正确发送 | 中断处理完成后 LAPIC 接受新中断 | §3.2 |
| `mask_all()` 全局屏蔽 | 所有 IRQ 线路被屏蔽 | §3.2 |
| Mock 实现 | `MockInterruptController` 记录 mask/unmask/ack/eoi 调用序列 | §3.2 |

### 5.2 ExceptionArch trait 测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| `vector()` 正确提取向量号 | 从异常帧读取 vector 字段 | §3.3 |
| `is_user_mode()` 正确判断 | CS.RPL=3 → 用户态 | §3.3 |
| `page_fault_address()` 读取 CR2 | 返回正确的缺页地址 | §3.3 |
| `is_write_fault()` 解析错误码 | bit 1 = 1 → 写缺页 | §3.3 |
| `set_instruction_pointer()` 修改 RIP | 修改后 RIP 指向恢复点 | §3.3 |
| Mock 实现 | `MockExceptionFrame` 允许构造任意异常帧 | §3.3 |

### 5.3 IrqManager 测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| 注册钩子后链表正确 | 链表头指向新钩子，ID 分配正确 | §3.4 |
| 同一 IRQ 注册多个钩子 | 链表长度增加，ID 不重复 | §3.4 |
| 移除钩子后链表正确 | 链表不断裂，无剩余钩子时 mask | §3.4 |
| 分发时所有钩子被调用 | 遍历链表，活跃位追踪正确 | §3.4 |
| 部分完成语义 | 未完成钩子的活跃位保持，全部完成后 unmask | §3.4 |
| 伪中断返回错误 | 无钩子的 IRQ 返回 `Spurious` | §3.9 |
| ID 耗尽 panic | 超过 32 个钩子注册到同一 IRQ | §3.9 |

### 5.4 ExceptionDispatcher 测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| 用户态除零异常 | 返回 `Signal(FPE)` | §3.5 |
| 用户态页错误 | 返回 `ForwardToVm` | §3.5 |
| NMI | 返回 `SpuriousNmi` | §3.5 |
| 内核态异常 | 返回 `KernelPanic` | §3.5 |
| 嵌套异常 + PhysCopy 上下文 | 返回 `RedirectToRecovery` | §3.6 |
| 嵌套异常 + UserCopyMsg 上下文 | 返回 `RedirectToRecovery` | §3.6 |
| VM 进程页错误 | 返回 `VmPageFault` | §3.5 |
| 异常分类表完整性 | 所有 20 个 x86 异常向量有正确分类 | §3.5 |

### 5.5 64 位演进测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| 异常帧为 64 位格式 | 字段大小和布局正确 | §3.7 |
| 无 8259A 代码 | x86-64 实现中无 8259A 寄存器操作 | §3.7 |
| APIC 初始化 | LAPIC + IOAPIC 正确初始化 | §3.7 |
| CR2 返回 64 位地址 | `page_fault_address()` 返回 `VirBytes(u64)` | §3.7 |

### 5.6 类型安全测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| `IrqVector` 与 `InterruptVector` 不可混用 | 类型系统阻止误用 | §3.8 |
| `IrqAction` 枚举替代哨兵值 | 只有 `Completed`/`NotCompleted` 两个值 | §3.4 |
| `IrqPolicy` bitflags 组合 | `REENABLE` 标志可正确设置和检查 | §3.4 |
| `ExceptionOutcome` 覆盖所有路径 | match 穷尽检查 | §3.5 |

---

## 6. 参见

| 文档 | 关联 |
|------|------|
| [04-protection.md](04-protection.md) | 保护模式基础设施（`ProtectionArch`/`TrapEntryArch` trait 定义，IDT 门描述符配置） |
| [06-proc-struct.md](06-proc-struct.md) | 进程结构（`KProcess` 中的 `rts::PAGEFAULT` 标志，寄存器保存区 `p_reg`） |
| [07-ipc.md](07-ipc.md) | IPC 机制（`mini_send()`/`mini_notify()` 用于页错误转发和 IRQ 通知） |
| [08-signal.md](08-signal.md) | 信号机制（`cause_sig()` 用于用户态异常发送信号） |
| [11-privilege.md](11-privilege.md) | 特权级管理（`Privilege` 枚举，`is_user_mode()` 判断依据） |
| [18-smp.md](18-smp.md) | SMP 启动（per-CPU LAPIC 初始化，AP 中断控制器设置） |
| [99-global-concepts.md](99-global-concepts.md) | 全局概念（Direct Map 消除 `phys_copy` 容错需求，`VmPagefaultIn` 消息类型） |
