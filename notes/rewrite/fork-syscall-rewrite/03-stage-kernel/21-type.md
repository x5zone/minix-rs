# 21-type - 基本类型定义

> 本文档分析 `minix3/minix/kernel/type.h`，讲解基本类型定义。

---

## 1. 概述

`type.h` 定义了内核使用的基本类型，包括进程号、系统索引、位图、IRQ 相关类型等。

**核心作用**：
- **进程标识**：`proc_nr_t` 表示进程表条目号
- **系统索引**：`sys_id_t` 表示系统进程索引
- **位图结构**：`sys_map_t` 用于权限和状态位图
- **中断处理**：`irq_hook_t`、`irq_handler_t` 用于中断管理

### 1.1 类型定义目的

类型定义的主要目的：

1. **语义清晰**：用有意义的名称替代原始类型，提高代码可读性
2. **类型安全**：编译器可以区分不同用途的整数类型
3. **可移植性**：便于在不同平台间移植，只需修改类型定义
4. **抽象封装**：隐藏实现细节，便于后续修改

**设计原则**：

```c
// 不好的写法
int proc_nr;     // 不清楚这是进程号还是其他整数

// 好的写法
proc_nr_t proc_nr;  // 明确表示进程表条目号
```

### 1.2 与 fork 的关系

fork 系统调用使用以下类型：

| 类型 | 用途 |
|------|------|
| `proc_nr_t` | 父进程和子进程的槽位号 |
| `endpoint_t` | 父进程和子进程的端点 |
| `sys_map_t` | 子进程继承的权限位图 |

**在 do_fork 中的使用**：

```c
int do_fork(struct proc * caller, message * m_ptr)
{
    int p_proc;              // 父进程槽位号（实际应为 proc_nr_t）
    struct proc *rpp;        // 父进程指针
    struct proc *rpc;        // 子进程指针

    // 使用 proc_nr_t 类型的槽位号
    if(!isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc))
        return EINVAL;

    rpp = proc_addr(p_proc);  // 通过槽位号获取进程指针
    rpc = proc_addr(m_ptr->m_lsys_krn_sys_fork.slot);
    // ...
}
```

---

## 2. C 源码分析

本节分析 `minix3/minix/kernel/type.h` 中的类型定义，包括：

- **进程标识类型**：`proc_nr_t`、`sys_id_t`
- **位图类型**：`sys_map_t`
- **中断类型**：`irq_policy_t`、`irq_id_t`、`irq_hook_t`、`irq_handler_t`

### 2.1 proc_nr_t 类型

`proc_nr_t` 是进程表条目号的类型定义。

**定义** (`type.h`)：

```c
typedef int proc_nr_t;    /* process table entry number */
```

**特点**：
- 底层类型为 `int`，支持正负数
- 正数表示用户进程
- 负数表示内核任务（kernel tasks）

#### 2.1.1 typedef int proc_nr_t

`typedef int proc_nr_t` 定义进程表条目号类型。

**为什么使用 int**：

1. **支持负数**：内核任务使用负数槽位号（如 -1, -2, ...）
2. **范围足够**：`int` 范围远超进程表大小
3. **效率高**：`int` 是机器字长，访问效率最高

**槽位号分配**：

```
槽位号范围            用途
───────────────────────────────
负数 (-NR_TASKS ~ -1)  内核任务
0                     IDLE 进程
正数 (1 ~ NR_PROCS)   用户进程
```

#### 2.1.2 进程表条目号

进程表条目号是进程在进程表中的索引。

**含义**：

1. **数组索引**：`proc[nr]` 访问进程表中的特定条目
2. **唯一标识**：在进程生命周期内，槽位号不变
3. **快速定位**：O(1) 时间访问进程结构

**与 endpoint 的关系**：

```
proc_nr_t (槽位号)     endpoint_t (端点)
      │                      │
      │    ┌─────────────────┘
      │    │
      ▼    ▼
┌─────────────────────────────────┐
│  endpoint = (gen << 15) + slot  │
│  slot = proc_nr                 │
└─────────────────────────────────┘
```

**示例**：

```c
// 进程表结构
struct proc proc[NR_TASKS + NR_PROCS];

// 通过槽位号访问
struct proc *rp = proc_addr(proc_nr);  // &proc[NR_TASKS + proc_nr]
```

#### 2.1.3 fork 时的使用

在 fork 系统调用中，`proc_nr_t` 用于标识父进程和子进程的槽位。

**使用场景** (`do_fork.c`)：

```c
int do_fork(struct proc * caller, message * m_ptr)
{
    int p_proc;  // 父进程槽位号（proc_nr_t）

    // 从端点提取父进程槽位号
    if(!isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc))
        return EINVAL;

    // 通过槽位号获取进程指针
    rpp = proc_addr(p_proc);                      // 父进程
    rpc = proc_addr(m_ptr->m_lsys_krn_sys_fork.slot);  // 子进程

    // 设置子进程的槽位号
    rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;
    // ...
}
```

**关键操作**：

| 操作 | 说明 |
|------|------|
| `isokendpt(ep, &p_proc)` | 从端点提取槽位号 |
| `proc_addr(p_proc)` | 通过槽位号获取进程指针 |
| `rpc->p_nr = slot` | 设置子进程槽位号 |

### 2.2 sys_id_t 类型

`sys_id_t` 是系统进程索引的类型定义。

**定义** (`type.h`)：

```c
typedef short sys_id_t;    /* system process index */
```

**特点**：
- 底层类型为 `short`（16位）
- 用于索引特权进程表（priv table）
- 范围比 `proc_nr_t` 小，节省空间

#### 2.2.1 typedef short sys_id_t

`typedef short sys_id_t` 定义系统进程索引类型。

**为什么使用 short**：

1. **范围足够**：系统进程数量有限（通常 < 100）
2. **节省空间**：`short` 只需 2 字节，`int` 需要 4 字节
3. **位图优化**：在位图中使用更紧凑

**与 proc_nr_t 的区别**：

| 类型 | 用途 | 范围 |
|------|------|------|
| `proc_nr_t` | 进程表索引 | 所有进程 |
| `sys_id_t` | 特权表索引 | 仅系统进程 |

#### 2.2.2 系统进程索引

系统进程索引用于索引特权结构体表（priv table）。

**含义**：

1. **特权表索引**：`priv(sys_id)` 访问进程的特权结构
2. **IPC 权限**：控制进程可发送消息的目标
3. **资源限制**：定义系统进程的资源配额

**特权表结构**：

```c
struct priv priv_table[NR_SYS_PROCS];

// 通过 sys_id 访问
#define priv(p)  (&priv_table[(p)->p_priv->s_id])
```

**系统进程分类**：

| 类型 | 示例 | 特权级别 |
|------|------|---------|
| 内核任务 | CLOCK, SYSTEM | 最高 |
| 系统服务 | VM, PM, VFS | 高 |
| 驱动程序 | TTY, DISK | 中 |
| 用户进程 | init, shell | 低 |

#### 2.2.3 fork 时的使用

在 fork 系统调用中，`sys_id_t` 用于子进程特权结构的继承。

**使用场景**：

```c
// 子进程继承父进程的特权结构
// 如果父进程是系统进程，子进程也需要相应的特权

// 特权进程降级逻辑（如果需要）
if (priv(rpp)->s_flags & SYS_PROC) {
    // 父进程是系统进程
    // 子进程可能需要降级为普通进程
    // 具体逻辑见 do_fork.c
}
```

**注意**：普通用户进程 fork 时，子进程继承父进程的特权级别，通常不涉及 `sys_id_t` 的直接操作。

### 2.3 sys_map_t 类型

`sys_map_t` 是系统索引位图的类型定义。

**定义** (`type.h`)：

```c
typedef struct {            /* bitmap for system indexes */
  bitchunk_t chunk[BITMAP_CHUNKS(NR_SYS_PROCS)];
} sys_map_t;
```

**特点**：
- 封装位数组，提供类型安全
- 用于权限控制和状态标记
- 支持高效的位操作

#### 2.3.1 结构体定义

`sys_map_t` 结构体封装了一个位数组。

**字段解析**：

```c
typedef struct {
  bitchunk_t chunk[BITMAP_CHUNKS(NR_SYS_PROCS)];
} sys_map_t;
```

| 字段 | 类型 | 含义 |
|------|------|------|
| `chunk` | `bitchunk_t[]` | 位数组，每个元素存储 32 位 |

**内存布局**：

```
sys_map_t
┌───────────────────────────────────────┐
│ chunk[0]  │ chunk[1]  │ ... │ chunk[n] │
│ (32 bits) │ (32 bits) │     │ (32 bits)│
└───────────────────────────────────────┘
     位 0-31     位 32-63         位 n*32-n*32+31
```

**BITMAP_CHUNKS 宏**：

```c
#define BITMAP_CHUNKS(nr_bits) (((nr_bits)+BITCHUNK_BITS-1)/BITCHUNK_BITS)
```

#### 2.3.2 系统索引位图

系统索引位图用于标记系统进程的状态或权限。

**用途**：

1. **IPC 权限**：`s_ipc_to` 标记可发送消息的目标
2. **信号待处理**：`s_sig_pending` 标记待处理信号
3. **异步消息**：`s_asyn_pending` 标记待发送的异步通知

**位图操作**：

```c
// 设置位
set_sys_bit(map, sys_id);

// 清除位
unset_sys_bit(map, sys_id);

// 测试位
if (get_sys_bit(map, sys_id)) {
    // 位已设置
}
```

**在特权结构中的使用**：

```c
struct priv {
    sys_map_t s_ipc_to;        // 可发送 IPC 的目标
    sys_map_t s_sig_pending;   // 待处理信号
    sys_map_t s_asyn_pending;  // 待发送异步消息
    // ...
};
```

#### 2.3.3 BITMAP_CHUNKS 宏

`BITMAP_CHUNKS` 宏计算存储指定位数所需的 chunk 数量。

**定义** (`bitmap.h`)：

```c
#define BITMAP_CHUNKS(nr_bits) (((nr_bits)+BITCHUNK_BITS-1)/BITCHUNK_BITS)
```

**计算原理**：

```
BITMAP_CHUNKS(nr_bits) = ceil(nr_bits / BITCHUNK_BITS)

例如：NR_SYS_PROCS = 64, BITCHUNK_BITS = 32
BITMAP_CHUNKS(64) = (64 + 31) / 32 = 95 / 32 = 2
```

**向上取整技巧**：

```c
// 普通除法向下取整
64 / 32 = 2

// 向上取整公式
(nr_bits + BITCHUNK_BITS - 1) / BITCHUNK_BITS
// 等价于 ceil(nr_bits / BITCHUNK_BITS)
```

**示例**：

| nr_bits | BITCHUNK_BITS | 结果 |
|---------|---------------|------|
| 64 | 32 | 2 |
| 65 | 32 | 3 |
| 32 | 32 | 1 |
| 1 | 32 | 1 |

### 2.4 irq_policy_t 类型

`irq_policy_t` 是 IRQ 策略的类型定义。

**定义** (`type.h`)：

```c
typedef unsigned long irq_policy_t;
```

**特点**：
- 底层类型为 `unsigned long`（32或64位）
- 用于存储 IRQ 处理策略标志
- 支持位标志组合

#### 2.4.1 typedef unsigned long irq_policy_t

`typedef unsigned long irq_policy_t` 定义 IRQ 策略类型。

**为什么使用 unsigned long**：

1. **位标志**：策略是多个标志位的组合
2. **平台适配**：`unsigned long` 大小与平台一致
3. **位操作友好**：便于进行位运算

**策略标志示例**：

```c
// IRQ 策略标志（示意）
#define IRQ_REENABLE    0x01    // 自动重新启用
#define IRQ_EXCLUSIVE   0x02    // 独占模式
#define IRQ_SHARE       0x04    // 共享模式
```

#### 2.4.2 IRQ 策略

IRQ 策略定义中断处理的行为模式。

**策略含义**：

| 标志 | 含义 | 说明 |
|------|------|------|
| `IRQ_REENABLE` | 自动重新启用 | 处理完成后自动启用中断 |
| `IRQ_EXCLUSIVE` | 独占模式 | 只允许一个处理程序 |
| `IRQ_SHARE` | 共享模式 | 允许多个处理程序 |

**使用示例**：

```c
// 设置 IRQ 策略
irq_policy_t policy = IRQ_REENABLE | IRQ_SHARE;

// 检查策略
if (policy & IRQ_REENABLE) {
    // 自动重新启用中断
}
```

**与 fork 的关系**：IRQ 相关类型在 fork 中通常不直接使用，因为用户进程一般不处理硬件中断。

### 2.5 irq_id_t 类型

`irq_id_t` 是 IRQ 标识符的类型定义。

**定义** (`type.h`)：

```c
typedef unsigned long irq_id_t;
```

**特点**：
- 底层类型为 `unsigned long`
- 用于唯一标识一个中断钩子
- 在中断通知中返回给进程

#### 2.5.1 typedef unsigned long irq_id_t

`typedef unsigned long irq_id_t` 定义 IRQ 标识符类型。

**作用**：

1. **唯一标识**：区分同一 IRQ 上的不同钩子
2. **通知关联**：中断发生时，进程收到对应的 `irq_id_t`
3. **资源管理**：用于取消中断钩子

**使用示例**：

```c
// 注册中断钩子
irq_id_t id = irq_setpolicy(irq, policy, proc_e);

// 中断发生时，通知中包含此 id
// 进程可以根据 id 判断是哪个中断源
```

#### 2.5.2 IRQ 标识符

IRQ 标识符用于关联中断通知与注册的钩子。

**工作流程**：

```
驱动程序注册中断钩子
       │
       ▼
irq_setpolicy() 返回 irq_id
       │
       ▼
中断发生，内核发送通知
       │
       ▼
通知消息中包含 irq_id
       │
       ▼
驱动程序根据 irq_id 判断中断源
```

**与 IRQ 号的区别**：

| 概念 | 说明 |
|------|------|
| IRQ 号 | 硬件中断线编号（如 IRQ 0-15） |
| IRQ ID | 软件标识符，用于区分同一 IRQ 上的多个钩子 |

**示例**：

```c
// 同一 IRQ 上可能有多个钩子
irq_id_t id1 = irq_setpolicy(5, IRQ_SHARE, proc_e);  // IRQ 5 的第一个钩子
irq_id_t id2 = irq_setpolicy(5, IRQ_SHARE, proc_e);  // IRQ 5 的第二个钩子
// id1 != id2，用于区分不同的中断源
```

### 2.6 irq_hook_t 结构体

`irq_hook_t` 是中断钩子结构体，用于管理中断处理程序。

**定义** (`type.h`)：

```c
typedef struct irq_hook {
  struct irq_hook *next;       /* next hook in chain */
  int (*handler)(struct irq_hook *);  /* interrupt handler */
  int irq;                     /* IRQ vector number */
  int id;                      /* id of this hook */
  endpoint_t proc_nr_e;        /* (endpoint) NONE if not in use */
  irq_id_t notify_id;          /* id to return on interrupt */
  irq_policy_t policy;         /* bit mask for policy */
} irq_hook_t;
```

**结构体作用**：

- 链接同一 IRQ 上的多个处理程序
- 存储中断处理所需的所有信息
- 支持共享中断和独占中断

#### 2.6.1 next 字段

`next` 指针用于链接同一 IRQ 上的多个钩子。

**定义**：

```c
struct irq_hook *next;    /* next hook in chain */
```

**作用**：

1. **链表结构**：将同一 IRQ 的多个钩子串联
2. **共享中断**：支持多个驱动程序共享同一中断线
3. **遍历处理**：中断发生时依次调用所有处理程序

**链表示意**：

```
IRQ 5 的钩子链表:
┌─────────┐    ┌─────────┐    ┌─────────┐
│ hook_1  │───►│ hook_2  │───►│ hook_3  │───► NULL
│ (tty)   │    │ (mouse) │    │ (audio) │
└─────────┘    └─────────┘    └─────────┘
```

#### 2.6.2 handler 字段

`handler` 是中断处理函数的指针。

**定义**：

```c
int (*handler)(struct irq_hook *);    /* interrupt handler */
```

**作用**：

1. **处理函数**：指向实际的中断处理代码
2. **回调机制**：内核通过此指针调用驱动程序的处理函数
3. **参数传递**：处理函数接收钩子指针，可访问钩子信息

**函数签名**：

```c
int handler(struct irq_hook *hook) {
    // 处理中断
    // 返回 1 表示已处理，返回 0 表示未处理
    return 1;
}
```

**返回值含义**：

| 返回值 | 含义 |
|--------|------|
| 1 | 中断已处理，停止遍历链表 |
| 0 | 中断未处理，继续遍历链表 |

#### 2.6.3 irq 字段

`irq` 存储硬件中断线编号。

**定义**：

```c
int irq;    /* IRQ vector number */
```

**作用**：

1. **中断线标识**：标识此钩子对应的中断线
2. **索引查找**：用于在全局钩子表中定位
3. **日志记录**：便于调试和日志输出

**典型值**：

| IRQ | 用途 |
|-----|------|
| 0 | 定时器 |
| 1 | 键盘 |
| 14 | 主 IDE 控制器 |
| 15 | 从 IDE 控制器 |

#### 2.6.4 id 字段

`id` 是钩子的唯一标识符。

**定义**：

```c
int id;    /* id of this hook */
```

**作用**：

1. **唯一标识**：区分同一 IRQ 上的不同钩子
2. **资源管理**：用于取消钩子时查找
3. **内部使用**：内核内部管理钩子时使用

**与 notify_id 的区别**：

| 字段 | 用途 |
|------|------|
| `id` | 内核内部使用的钩子标识 |
| `notify_id` | 返回给用户进程的通知标识 |

#### 2.6.5 proc_nr_e 字段

`proc_nr_e` 存储注册此钩子的进程端点。

**定义**：

```c
endpoint_t proc_nr_e;    /* (endpoint) NONE if not in use */
```

**作用**：

1. **进程关联**：标识哪个进程注册了此钩子
2. **通知目标**：中断发生时通知此进程
3. **槽位管理**：`NONE` 表示此钩子槽位空闲

**使用示例**：

```c
// 注册钩子时设置
hook->proc_nr_e = caller_endpoint;

// 检查钩子是否在使用
if (hook->proc_nr_e == NONE) {
    // 槽位空闲，可以使用
}

// 进程退出时清理
if (hook->proc_nr_e == exiting_process) {
    hook->proc_nr_e = NONE;  // 标记为空闲
}
```

#### 2.6.6 notify_id 字段

`notify_id` 是返回给用户进程的通知标识。

**定义**：

```c
irq_id_t notify_id;    /* id to return on interrupt */
```

**作用**：

1. **通知标识**：中断发生时，此值包含在通知消息中
2. **区分源**：进程可据此判断是哪个中断源
3. **用户可见**：这是用户空间可见的标识符

**工作流程**：

```
中断发生
    │
    ▼
内核查找钩子链表
    │
    ▼
发送通知给 proc_nr_e
    │
    ▼
通知消息包含 notify_id
    │
    ▼
进程根据 notify_id 判断中断源
```

#### 2.6.7 policy 字段

`policy` 存储中断处理策略标志。

**定义**：

```c
irq_policy_t policy;    /* bit mask for policy */
```

**作用**：

1. **策略控制**：定义中断处理的行为
2. **位标志组合**：可组合多个策略标志
3. **运行时决策**：内核根据策略决定如何处理中断

**策略标志**：

| 标志 | 含义 |
|------|------|
| `IRQ_REENABLE` | 处理后自动重新启用中断 |
| `IRQ_EXCLUSIVE` | 独占模式，不允许其他钩子 |
| `IRQ_SHARE` | 共享模式，允许多个钩子 |

**使用示例**：

```c
// 设置共享 + 自动重新启用
hook->policy = IRQ_SHARE | IRQ_REENABLE;

// 检查策略
if (hook->policy & IRQ_REENABLE) {
    enable_irq(hook->irq);  // 重新启用中断
}
```

### 2.7 irq_handler_t 类型

`irq_handler_t` 是中断处理函数的类型定义。

**定义** (`type.h`)：

```c
typedef int (*irq_handler_t)(struct irq_hook *);
```

**特点**：
- 函数指针类型
- 指向中断处理函数
- 返回 int 表示处理结果

#### 2.7.1 函数指针类型

`typedef int (*irq_handler_t)(struct irq_hook *)` 定义函数指针类型。

**语法解析**：

```c
typedef int (*irq_handler_t)(struct irq_hook *);
         │   │              │
         │   │              └── 参数类型
         │   └── 函数指针名
         └── 返回类型
```

**等价写法**：

```c
// 使用 typedef
irq_handler_t handler = my_handler;

// 不使用 typedef
int (*handler)(struct irq_hook *) = my_handler;
```

**类型安全**：

```c
// 编译器会检查函数签名
irq_handler_t h = some_function;  // some_function 必须匹配签名
```

#### 2.7.2 IRQ 处理函数

IRQ 处理函数的签名定义了其接口规范。

**函数签名**：

```c
int handler(struct irq_hook *hook);
```

**参数**：

| 参数 | 类型 | 含义 |
|------|------|------|
| `hook` | `struct irq_hook *` | 指向当前钩子的指针 |

**返回值**：

| 返回值 | 含义 |
|--------|------|
| `1` | 中断已处理，停止遍历链表 |
| `0` | 中断未处理，继续遍历链表 |

**实现示例**：

```c
int my_irq_handler(struct irq_hook *hook)
{
    // 通过 hook 访问钩子信息
    int irq = hook->irq;
    endpoint_t proc = hook->proc_nr_e;

    // 处理中断...

    // 返回 1 表示已处理
    return 1;
}
```

---

## 3. Rust 设计决策

本节讨论如何用 Rust 实现基本类型，重点关注类型安全和零成本抽象。

### 3.1 类型别名

Rust 使用 `type` 关键字定义类型别名，但更推荐使用新类型模式。

**类型别名**：

```rust
// 简单别名（不推荐，无类型安全）
type ProcNr = i32;
type SysId = i16;

// 使用时无类型检查
let nr: ProcNr = 5;
let id: SysId = 5;
// nr 和 id 可以混用，不安全
```

**新类型模式（推荐）**：

```rust
// 新类型模式（类型安全）
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ProcNr(i32);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SysId(i16);

// 编译器会区分不同类型
let nr = ProcNr(5);
let id = SysId(5);
// nr != id  // 编译错误！类型不匹配
```

**优势**：

| 特性 | 类型别名 | 新类型模式 |
|------|----------|------------|
| 类型安全 | 否 | 是 |
| 零运行时开销 | 是 | 是 |
| 编译期检查 | 弱 | 强 |

### 3.2 位图类型

Rust 中位图类型可以使用泛型实现，提供类型安全和边界检查。

**泛型位图设计**：

```rust
/// 位图 trait，定义位图操作接口
pub trait Bitmap {
    fn get(&self, bit: usize) -> bool;
    fn set(&mut self, bit: usize);
    fn clear(&mut self, bit: usize);
}

/// 固定大小位图
#[derive(Clone, Default)]
pub struct SysMap<const N: usize> {
    chunks: [u32; N],
}

impl<const N: usize> Bitmap for SysMap<N> {
    fn get(&self, bit: usize) -> bool {
        let chunk_idx = bit / 32;
        let offset = bit % 32;
        (self.chunks[chunk_idx] & (1 << offset)) != 0
    }

    fn set(&mut self, bit: usize) {
        let chunk_idx = bit / 32;
        let offset = bit % 32;
        self.chunks[chunk_idx] |= 1 << offset;
    }

    fn clear(&mut self, bit: usize) {
        let chunk_idx = bit / 32;
        let offset = bit % 32;
        self.chunks[chunk_idx] &= !(1 << offset);
    }
}
```

**优势**：

1. **编译期大小**：`const N` 在编译期确定位图大小
2. **类型安全**：通过 trait 约束操作接口
3. **零开销**：编译后与 C 位图性能相同

### 3.3 函数指针

Rust 中函数指针和闭包的处理比 C 更安全。

**函数指针类型**：

```rust
// 函数指针类型
type IrqHandler = fn(&mut IrqHook) -> i32;

// 使用
fn my_handler(hook: &mut IrqHook) -> i32 {
    // 处理中断
    1
}

let handler: IrqHandler = my_handler;
```

**使用 trait 更灵活**：

```rust
// 定义中断处理 trait
pub trait IrqHandler {
    fn handle(&mut self, hook: &mut IrqHook) -> i32;
}

// 实现 trait
struct MyHandler;

impl IrqHandler for MyHandler {
    fn handle(&mut self, hook: &mut IrqHook) -> i32 {
        // 处理中断
        1
    }
}
```

**对比**：

| 方式 | 灵活性 | 性能 | 安全性 |
|------|--------|------|--------|
| 函数指针 | 低 | 高 | 中 |
| Trait 对象 | 高 | 中 | 高 |
| 泛型 | 高 | 高 | 高 |

---

## 4. 实现

本节给出基本类型的 Rust 实现代码。

### 4.1 基本类型定义

```rust
//! 基本类型定义
//! 对应 minix3/minix/kernel/type.h

use crate::endpoint::Endpoint;

/// 进程表条目号
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ProcNr(i32);

impl ProcNr {
    pub const fn new(nr: i32) -> Self {
        Self(nr)
    }

    pub const fn get(&self) -> i32 {
        self.0
    }

    pub fn is_kernel_task(&self) -> bool {
        self.0 < 0
    }

    pub fn is_user_process(&self) -> bool {
        self.0 > 0
    }
}

/// 系统进程索引
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SysId(i16);

impl SysId {
    pub const fn new(id: i16) -> Self {
        Self(id)
    }

    pub const fn get(&self) -> i16 {
        self.0
    }
}

/// IRQ 策略
#[derive(Clone, Copy, Default)]
pub struct IrqPolicy(u32);

impl IrqPolicy {
    pub const REENABLE: u32 = 0x01;
    pub const EXCLUSIVE: u32 = 0x02;
    pub const SHARE: u32 = 0x04;

    pub fn new(policy: u32) -> Self {
        Self(policy)
    }

    pub fn contains(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }
}

/// IRQ 标识符
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct IrqId(u32);

impl IrqId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn get(&self) -> u32 {
        self.0
    }
}
```

### 4.2 IrqHook 结构体

```rust
use super::{IrqId, IrqPolicy};
use crate::endpoint::Endpoint;
use core::ptr;

/// 中断钩子链表节点
#[repr(C)]
pub struct IrqHook {
    pub next: *mut IrqHook,
    pub handler: Option<IrqHandlerFn>,
    pub irq: i32,
    pub id: i32,
    pub proc_nr_e: Endpoint,
    pub notify_id: IrqId,
    pub policy: IrqPolicy,
}

/// 中断处理函数类型
pub type IrqHandlerFn = fn(&mut IrqHook) -> i32;

impl IrqHook {
    pub const fn new() -> Self {
        Self {
            next: ptr::null_mut(),
            handler: None,
            irq: 0,
            id: 0,
            proc_nr_e: Endpoint::NONE,
            notify_id: IrqId::new(0),
            policy: IrqPolicy::default(),
        }
    }

    pub fn is_in_use(&self) -> bool {
        self.proc_nr_e != Endpoint::NONE
    }

    pub fn call_handler(&mut self) -> i32 {
        match self.handler {
            Some(h) => h(self),
            None => 0,
        }
    }
}
```

### 4.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proc_nr() {
        let nr = ProcNr::new(5);
        assert_eq!(nr.get(), 5);
        assert!(nr.is_user_process());
        assert!(!nr.is_kernel_task());

        let kernel_nr = ProcNr::new(-1);
        assert!(kernel_nr.is_kernel_task());
        assert!(!kernel_nr.is_user_process());
    }

    #[test]
    fn test_sys_id() {
        let id = SysId::new(10);
        assert_eq!(id.get(), 10);
    }

    #[test]
    fn test_irq_policy() {
        let policy = IrqPolicy::new(IrqPolicy::REENABLE | IrqPolicy::SHARE);
        assert!(policy.contains(IrqPolicy::REENABLE));
        assert!(policy.contains(IrqPolicy::SHARE));
        assert!(!policy.contains(IrqPolicy::EXCLUSIVE));
    }

    #[test]
    fn test_irq_id() {
        let id = IrqId::new(42);
        assert_eq!(id.get(), 42);
    }

    #[test]
    fn test_irq_hook() {
        let hook = IrqHook::new();
        assert!(!hook.is_in_use());
        assert_eq!(hook.irq, 0);
    }
}
```

---

## 5. 参见

- [20-endpoint](20-endpoint.md) - 端点机制
- [22-const](22-const.md) - 常量定义
- [09-priv-struct](09-priv-struct.md) - 特权结构体
