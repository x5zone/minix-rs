# 11-system-init - 系统调用初始化

> 本文档分析 `minix3/minix/kernel/system.c` 第 1-80 行，讲解系统调用初始化。

---

## 1. 概述

系统调用初始化（`system_init`）是内核启动过程中的关键步骤，负责建立系统调用处理的基础设施。其主要作用包括：

**核心职责**：
- **调用向量初始化**：将 `call_vec` 数组初始化为安全默认值（NULL），然后注册所有系统调用处理函数
- **IRQ 钩子初始化**：标记所有中断钩子为可用状态
- **告警定时器初始化**：为所有特权结构初始化定时器

**设计理念**：
- **安全默认**：先置 NULL 再注册，确保未注册的系统调用有明确的失败路径
- **编译期检查**：`map` 宏使用 `assert` 确保调用号在有效范围内
- **模块化**：每个系统调用有独立的处理函数，便于维护和扩展

### 1.1 系统调用机制

MINIX 采用**消息传递**作为系统调用的核心机制，而非传统的寄存器传参方式。

**系统调用流程**：

```
用户进程                    内核                      系统任务
    │                        │                          │
    │  1. 填充 message       │                          │
    │  2. int SYSVEC         │                          │
    │ ─────────────────────► │                          │
    │                        │  3. kernel_call()        │
    │                        │  4. kernel_call_dispatch │
    │                        │  5. call_vec[call_nr]()  │
    │                        │ ────────────────────────►│
    │                        │                          │  6. do_xxx() 处理
    │                        │ ◄────────────────────────│
    │                        │  7. 返回结果              │
    │ ◄───────────────────── │                          │
    │  8. copy_msg_to_user   │                          │
```

**关键组件**：
- **`call_vec`**：函数指针数组，映射系统调用号到处理函数
- **`kernel_call_dispatch`**：分发函数，验证权限并调用处理函数
- **`kernel_call_finish`**：完成函数，处理结果返回和 VM 挂起情况

**权限检查**：
```c
if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr)) {
    result = ECALLDENIED;  // 进程无权调用此系统调用
}
```

### 1.2 与 fork 的关系

fork 系统调用通过 `map` 宏在 `system_init` 中注册：

```c
/* Process management. */
map(SYS_FORK, do_fork);     // 注册 fork 系统调用
```

**注册过程**：

1. **宏展开**：`map(SYS_FORK, do_fork)` 展开为：
   ```c
   {
       int call_index = SYS_FORK - KERNEL_CALL;
       assert(call_index >= 0 && call_index < NR_SYS_CALLS);
       call_vec[call_index] = do_fork;
   }
   ```

2. **索引计算**：`SYS_FORK` 是系统调用号，减去 `KERNEL_CALL` 基址得到数组索引

3. **编译期检查**：`assert` 确保索引在有效范围内，非法调用号会导致编译失败

4. **函数指针赋值**：将 `do_fork` 函数指针存入 `call_vec` 数组

**调用路径**：
```
用户调用 fork()
    ↓
int SYSVEC (软中断)
    ↓
kernel_call() → kernel_call_dispatch()
    ↓
call_vec[SYS_FORK - KERNEL_CALL]()
    ↓
do_fork(caller, msg)
```

---

## 2. C 源码分析

本节深入分析 `minix3/minix/kernel/system.c` 中的系统调用初始化代码，包括：

- **文件头部注释**：理解系统任务的设计意图和入口点列表
- **头文件包含**：分析依赖关系和接口定义
- **调用向量定义**：理解 `call_vec` 数组和 `map` 宏的实现
- **`system_init` 函数**：详细分析初始化流程的三个阶段

### 2.1 文件头部注释

`system.c` 文件头部包含详细的注释，说明了该文件的用途、功能以及系统任务提供的各种入口点。

#### 2.1.1 系统调用概述

文件头部注释说明 `system.c` 是内核中系统任务（System Task）的实现文件。系统任务是 MINIX 内核中的一个特殊进程，负责处理系统调用请求。

```c
/* This file contains the system task. When a system call is made, the kernel
 * does what it can and then sends a message to the system task to handle the
 * request.
 */
```

系统调用的整体流程：
1. 用户进程发起系统调用（通过 `int SYSVEC` 或 `syscall` 指令）
2. 内核捕获中断，进行基本处理后，发送消息给系统任务
3. 系统任务通过 `kernel_call` 函数处理请求
4. 处理完成后，通过 IPC 机制返回结果给用户进程

#### 2.1.2 入口点列表

文件头部注释详细列出了系统任务提供的各种入口点（Entry Points）：

**进程管理类：**
- `do_fork` - 创建子进程（fork 系统调用）
- `do_exec` - 执行新程序（exec 系统调用）
- `do_exit` - 进程退出（exit 系统调用）
- `do_wait` - 等待子进程状态变化
- `do_sigreturn` - 从信号处理程序返回

**信号处理类：**
- `do_kill` - 发送信号给进程

**设备 I/O 类：**
- `do_irqctl` - IRQ 控制（启用/禁用中断）
- `do_devio` - 设备 I/O（端口读写）
- `do_sdevio` - 字符串设备 I/O
- `do_vdevio` - 向量设备 I/O

**内存管理类：**
- `do_vircopy` - 虚拟地址空间复制（单个）
- `do_virvcopy` - 虚拟地址空间复制（批量）
- `do_umap` - 地址转换（用户到物理）
- `do_umap_remote` - 远程地址转换

**复制类：**
- `do_vircopy` / `do_virvcopy` - 跨进程内存复制

**时钟类：**
- `do_setalarm` - 设置定时器
- `do_times` - 获取进程时间信息

**系统控制类：**
- `do_abort` - 系统中止（panic）
- `do_getinfo` - 获取系统信息
- `do_diagctl` - 诊断控制

这些入口点函数遵循统一的签名规范：
```c
int do_xxx(struct proc *caller, message *m_ptr);
```
- `caller`：发起系统调用的进程指针
- `m_ptr`：包含系统调用参数的消息指针
- 返回值：操作结果（0 表示成功，负数为错误码）

### 2.2 头文件包含

`system.c` 包含两类头文件：内核内部头文件和标准库/系统头文件。

```c
#include "kernel/system.h"    // 系统任务核心定义
#include "kernel/vm.h"        // VM 相关接口
#include "kernel/clock.h"     // 时钟相关接口
#include <stdlib.h>           // 标准库
#include <stddef.h>           // 标准定义
#include <assert.h>           // 断言宏
#include <signal.h>           // 信号定义
#include <unistd.h>           // POSIX 接口
#include <minix/endpoint.h>   // 端点类型定义
#include <minix/safecopies.h> // 安全复制接口
```

**分类**：

| 类别 | 头文件 | 作用 |
|------|--------|------|
| **内核内部** | `kernel/system.h` | 系统任务函数声明、数据结构 |
| **内核内部** | `kernel/vm.h` | VM 交互接口 |
| **内核内部** | `kernel/clock.h` | 时钟和定时器接口 |
| **标准库** | `stdlib.h`, `stddef.h` | 基础类型和函数 |
| **调试** | `assert.h` | 编译期和运行时断言 |
| **进程控制** | `signal.h`, `unistd.h` | 信号和 POSIX 接口 |
| **MINIX 特定** | `minix/endpoint.h` | `endpoint_t` 类型定义 |
| **MINIX 特定** | `minix/safecopies.h` | 安全内存复制接口 |

#### 2.2.1 kernel/system.h

`kernel/system.h` 是系统任务的核心头文件，定义了所有系统调用处理函数的原型。

**核心功能**：

1. **函数原型声明**：声明所有 `do_xxx` 处理函数
   ```c
   int do_fork(struct proc * caller, message *m_ptr);
   int do_exec(struct proc * caller, message *m_ptr);
   int do_exit(struct proc * caller, message *m_ptr);
   // ... 更多处理函数
   ```

2. **条件编译控制**：根据配置启用/禁用系统调用
   ```c
   int do_fork(struct proc * caller, message *m_ptr);
   #if ! USE_FORK
   #define do_fork NULL   // 禁用时置为 NULL
   #endif
   ```

3. **统一签名**：所有处理函数遵循相同签名
   ```c
   int do_xxx(struct proc *caller, message *m_ptr);
   // caller: 调用进程指针
   // m_ptr: 消息指针（包含参数）
   // 返回: OK 或错误码
   ```

**设计优势**：
- **可配置性**：通过 `USE_xxx` 宏控制功能裁剪
- **类型安全**：函数原型确保参数类型正确
- **NULL 安全**：禁用的调用自动映射为 NULL，`call_vec` 分发时会返回 `EBADREQUEST`

#### 2.2.2 kernel/vm.h

`kernel/vm.h` 定义了内核与 VM（虚拟内存管理器）交互相关的常量和宏。

**伪错误码定义**：

```c
#define VMSUSPEND       (-996)  // 系统调用被 VM 挂起
#define EFAULT_SRC      (-995)  // 源地址错误
#define EFAULT_DST      (-994)  // 目标地址错误
```

**VMSUSPEND 的作用**：

当系统调用需要 VM 参与但 VM 暂时无法响应时（如内存分配需要等待），返回 `VMSUSPEND`：

```c
if(result == VMSUSPEND) {
    // 保存消息，等待 VM 回复后恢复
    caller->p_vmrequest.saved.reqmsg = *msg;
    caller->p_misc_flags |= MF_KCALL_RESUME;
}
```

**物理复制宏**：

```c
#define PHYS_COPY_CATCH(src, dst, size, a) {  \
    catch_pagefaults++;                       \
    a = phys_copy(src, dst, size);            \
    catch_pagefaults--;                       \
}
```

此宏用于在复制过程中捕获缺页异常，便于 VM 处理。

#### 2.2.3 minix/endpoint.h

`minix/endpoint.h` 定义了端点（endpoint）类型及其操作宏，是 MINIX IPC 的核心标识机制。

**端点结构**：

端点由两部分组成：进程槽位号（slot）和代数（generation）。

```
┌─────────────────────────────────────────────────────────┐
│                    endpoint_t (32-bit)                  │
├──────────────────────┬──────────────────────────────────┤
│   generation (高17位) │      slot (低15位)               │
├──────────────────────┴──────────────────────────────────┤
│  _ENDPOINT_GENERATION_SHIFT = 15                        │
└─────────────────────────────────────────────────────────┘
```

**核心宏定义**：

```c
#define _ENDPOINT(g, p)  (((g) << 15) + (p))  // 从 generation 和 slot 构造 endpoint
#define _ENDPOINT_G(e)   (((e)+MAX_NR_TASKS) >> 15)  // 提取 generation
#define _ENDPOINT_P(e)   ((((e)+MAX_NR_TASKS) & 0x7FFF) - MAX_NR_TASKS)  // 提取 slot
```

**特殊端点值**：

```c
#define ANY   ((endpoint_t) (_ENDPOINT_SLOT_TOP - 1))  // 任意进程
#define NONE  ((endpoint_t) (_ENDPOINT_SLOT_TOP - 2))  // 无进程
#define SELF  ((endpoint_t) (_ENDPOINT_SLOT_TOP - 3))  // 当前进程
```

**设计目的**：
- **槽位复用安全**：每次槽位重用时 generation 递增，防止与旧进程混淆
- **IPC 路由**：通过 endpoint 唯一标识通信目标
- **可读性**：generation 为 0 时，endpoint 等于 slot 号


### 2.3 调用向量声明

调用向量（`call_vec`）是系统调用分发的核心数据结构，将系统调用号映射到处理函数。

```c
static int (*call_vec[NR_SYS_CALLS])(struct proc * caller, message *m_ptr);
```

**声明解析**：

| 部分 | 含义 |
|------|------|
| `static` | 文件内部链接，仅 `system.c` 可见 |
| `int (*)` | 函数指针，返回 `int` |
| `[NR_SYS_CALLS]` | 数组大小，系统调用总数 |
| `(struct proc *, message *)` | 函数参数类型 |

**内存布局**：

```
call_vec[0] ──► NULL 或 do_xxx 函数地址
call_vec[1] ──► NULL 或 do_yyy 函数地址
    ...
call_vec[NR_SYS_CALLS-1] ──► NULL 或 do_zzz 函数地址
```

**索引计算**：系统调用号减去 `KERNEL_CALL` 基址得到数组索引。

```c
int call_index = call_nr - KERNEL_CALL;
result = (*call_vec[call_index])(caller, msg);
```

#### 2.3.1 call_vec 数组

`call_vec[NR_SYS_CALLS]` 定义了一个固定大小的函数指针数组。

**数组大小**：

```c
#define NR_SYS_CALLS  58   // 系统调用总数
```

数组大小为 58，对应 MINIX 内核支持的所有系统调用。

**初始化流程**：

```c
// 1. 先置为安全默认值
for (i = 0; i < NR_SYS_CALLS; i++) {
    call_vec[i] = NULL;
}

// 2. 注册具体处理函数
map(SYS_FORK, do_fork);   // call_vec[SYS_FORK - KERNEL_CALL] = do_fork;
map(SYS_EXEC, do_exec);
// ...
```

**NULL 处理**：

当调用未注册的系统调用时，分发函数会检查 NULL：

```c
if (call_vec[call_nr])
    result = (*call_vec[call_nr])(caller, msg);
else
    result = EBADREQUEST;  // 未注册的调用返回错误
```

#### 2.3.2 函数指针类型

`int (*)(struct proc *, message *)` 是系统调用处理函数的统一签名类型。

**类型分解**：

```c
int (*handler)(struct proc *caller, message *m_ptr);
//  │    │
//  │    └── 函数指针，指向处理函数
//  └── 返回值类型：int（OK 或错误码）
```

**参数说明**：

| 参数 | 类型 | 用途 |
|------|------|------|
| `caller` | `struct proc *` | 调用进程的进程结构指针，包含进程状态、权限等信息 |
| `m_ptr` | `message *` | 消息指针，包含系统调用参数和返回值 |

**返回值约定**：

```c
#define OK          0     // 成功
#define EBADREQUEST (-22) // 无效请求
#define ECALLDENIED (-23) // 权限拒绝
// ... 其他错误码
```

**统一签名的优势**：
- **类型安全**：所有处理函数签名一致，编译器可检查
- **简化分发**：`call_vec` 数组类型统一，调用方式一致
- **扩展性**：新增系统调用只需实现相同签名的函数

### 2.4 map 宏

`map` 宏是系统调用注册的核心机制，将系统调用号映射到处理函数。

**宏定义**：

```c
#define map(call_nr, handler)                   \
    {   int call_index = call_nr - KERNEL_CALL;         \
        assert(call_index >= 0 && call_index < NR_SYS_CALLS);  \
        call_vec[call_index] = (handler); }
```

**参数说明**：

| 参数 | 含义 |
|------|------|
| `call_nr` | 系统调用号（如 `SYS_FORK`） |
| `handler` | 处理函数名（如 `do_fork`） |

**使用示例**：

```c
map(SYS_FORK, do_fork);   // 注册 fork 系统调用
map(SYS_EXEC, do_exec);   // 注册 exec 系统调用
map(SYS_EXIT, do_exit);   // 注册 exit 系统调用
```

#### 2.4.1 宏展开

以 `map(SYS_FORK, do_fork)` 为例，展示宏展开过程：

**展开前**：
```c
map(SYS_FORK, do_fork);
```

**展开后**：
```c
{
    int call_index = SYS_FORK - KERNEL_CALL;
    assert(call_index >= 0 && call_index < NR_SYS_CALLS);
    call_vec[call_index] = (do_fork);
}
```

**具体值代入**：

假设 `SYS_FORK = 4`，`KERNEL_CALL = 0`，`NR_SYS_CALLS = 58`：

```c
{
    int call_index = 4 - 0;           // call_index = 4
    assert(4 >= 0 && 4 < 58);         // 检查通过
    call_vec[4] = (do_fork);          // 注册处理函数
}
```

**展开结果**：`call_vec[4]` 指向 `do_fork` 函数。

#### 2.4.2 调用号检查

`map` 宏通过 `assert` 检查调用号的有效性：

```c
assert(call_index >= 0 && call_index < NR_SYS_CALLS);
```

**检查逻辑**：

| 条件 | 含义 |
|------|------|
| `call_index >= 0` | 系统调用号不能小于 `KERNEL_CALL` 基址 |
| `call_index < NR_SYS_CALLS` | 系统调用号不能超出数组范围 |

**有效性保证**：

- **下界检查**：防止负索引访问数组
- **上界检查**：防止越界访问，避免内存损坏
- **编译期捕获**：非法调用号在编译时就会被发现

**错误示例**：

```c
// 假设 NR_SYS_CALLS = 58
map(100, do_something);  // 编译错误：assert(100 >= 0 && 100 < 58) 失败
map(-1, do_something);   // 编译错误：assert(-1 >= 0 && ...) 失败
```

#### 2.4.3 编译时断言

`assert` 在 `map` 宏中起到编译期和运行期双重保障作用。

**assert 的双重作用**：

| 阶段 | 行为 | 效果 |
|------|------|------|
| **编译期** | 常量表达式断言失败 → 编译错误 | 静态调用号错误立即发现 |
| **运行期** | 动态表达式断言失败 → 程序中止 | 防止运行时越界访问 |

**编译期示例**：

```c
// 编译时常量，断言失败导致编译错误
map(SYS_INVALID, do_xxx);  // SYS_INVALID 超出范围 → 编译失败
```

**运行期示例**：

```c
// 动态调用号（虽然不常见），断言失败导致运行时中止
int dynamic_call = get_call_from_user();
map(dynamic_call, do_xxx);  // 如果 dynamic_call 无效 → 运行时断言失败
```

**设计优势**：

- **早期发现**：编译期捕获大部分错误
- **防御性编程**：运行期保护防止意外情况
- **零运行时开销**：编译期常量断言可被优化掉

---

## 3. system_init 函数

`system_init` 是系统调用子系统的初始化入口，在内核启动时调用。它完成三个主要任务：

**函数定义**：

```c
void system_init(void)
{
  register struct priv *sp;
  int i;

  // 1. 初始化 IRQ 钩子
  for (i=0; i<NR_IRQ_HOOKS; i++) {
      irq_hooks[i].proc_nr_e = NONE;
  }

  // 2. 初始化告警定时器
  for (sp=BEG_PRIV_ADDR; sp < END_PRIV_ADDR; sp++) {
    tmr_inittimer(&(sp->s_alarm_timer));
  }

  // 3. 初始化调用向量并注册系统调用
  for (i=0; i<NR_SYS_CALLS; i++) {
      call_vec[i] = NULL;
  }
  map(SYS_FORK, do_fork);
  // ... 更多注册
}
```

**三个初始化阶段**：

| 阶段 | 任务 | 目的 |
|------|------|------|
| **1. IRQ 钩子** | 标记所有钩子为可用 | 为中断处理做准备 |
| **2. 告警定时器** | 初始化所有进程的定时器 | 为定时器系统调用做准备 |
| **3. 调用向量** | 注册所有系统调用处理函数 | 建立系统调用分发机制 |

### 3.1 函数签名

```c
void system_init(void)
```

**签名分析**：

| 部分 | 含义 |
|------|------|
| `void` 返回值 | 初始化函数不返回值，失败时直接 panic |
| `void` 参数 | 无需参数，所有数据通过全局变量访问 |

**设计特点**：

- **无参数**：初始化所需的所有资源（`irq_hooks`、`call_vec` 等）都是全局静态变量
- **无返回值**：初始化失败被视为致命错误，内核直接中止
- **单次调用**：仅在内核启动时调用一次

**调用时机**：

```
内核启动
    ↓
main() 入口
    ↓
各子系统初始化
    ↓
system_init()  ← 在此调用
    ↓
系统调用机制就绪
```

### 3.2 IRQ 钩子初始化

IRQ 钩子用于管理硬件中断处理程序的注册。

**初始化代码**：

```c
/* Initialize IRQ handler hooks. Mark all hooks available. */
for (i=0; i<NR_IRQ_HOOKS; i++) {
    irq_hooks[i].proc_nr_e = NONE;
}
```

**目的**：将所有 IRQ 钩子标记为"未使用"状态，为后续中断处理注册做准备。

**与 fork 的关系**：fork 系统调用本身不直接使用 IRQ 钩子，但子进程可能继承父进程的中断处理能力（如驱动程序进程 fork）。

#### 3.2.1 irq_hooks 数组

`irq_hooks` 是全局数组，存储所有 IRQ 钩子结构。

**定义**：

```c
EXTERN irq_hook_t irq_hooks[NR_IRQ_HOOKS];  /* hooks for general use */
```

**irq_hook_t 结构**：

```c
typedef struct irq_hook {
    endpoint_t proc_nr_e;    // 注册此钩子的进程端点
    int notify_id;           // 通知标识符
    int policy;              // 中断处理策略
    // ... 其他字段
} irq_hook_t;
```

**作用**：

| 功能 | 说明 |
|------|------|
| **中断注册** | 驱动程序通过 `IRQ_SETPOLICY` 注册中断处理 |
| **中断分发** | 中断发生时，内核通知注册的进程 |
| **资源管理** | 跟踪哪些进程使用了哪些中断线 |

**生命周期**：

```
驱动初始化 → IRQ_SETPOLICY → irq_hooks[i].proc_nr_e = driver_endpoint
    ↓
中断发生 → 内核查找 irq_hooks → 通知驱动进程
    ↓
驱动退出 → IRQ_RMPOLICY → irq_hooks[i].proc_nr_e = NONE
```

#### 3.2.2 proc_nr_e 初始化

通过将 `proc_nr_e` 设置为 `NONE` 来标记钩子为可用。

**初始化代码**：

```c
for (i=0; i<NR_IRQ_HOOKS; i++) {
    irq_hooks[i].proc_nr_e = NONE;
}
```

**NONE 的含义**：

```c
#define NONE  ((endpoint_t) (_ENDPOINT_SLOT_TOP - 2))  // 特殊值，表示"无进程"
```

**标记逻辑**：

| `proc_nr_e` 值 | 状态 | 说明 |
|----------------|------|------|
| `NONE` | 可用 | 钩子未分配，可被新驱动注册 |
| 其他端点 | 已占用 | 钩子已分配给对应进程 |

**查找可用钩子**：

```c
for (i=0; i<NR_IRQ_HOOKS; i++) {
    if (irq_hooks[i].proc_nr_e == NONE) {
        // 找到可用钩子
        return i;
    }
}
return ENOSPC;  // 无可用钩子
```

### 3.3 告警定时器初始化

告警定时器用于实现 `SYS_SETALARM` 系统调用，允许进程设置定时通知。

**初始化代码**：

```c
/* Initialize all alarm timers for all processes. */
for (sp=BEG_PRIV_ADDR; sp < END_PRIV_ADDR; sp++) {
    tmr_inittimer(&(sp->s_alarm_timer));
}
```

**目的**：为每个特权结构初始化一个告警定时器，支持进程级别的定时功能。

**与 fork 的关系**：fork 系统调用不直接涉及告警定时器，但子进程继承父进程的定时器设置（由 PM 处理）。

#### 3.3.1 遍历特权结构

特权结构（`priv`）数组存储所有系统进程的特权信息。

**遍历代码**：

```c
for (sp=BEG_PRIV_ADDR; sp < END_PRIV_ADDR; sp++) {
    tmr_inittimer(&(sp->s_alarm_timer));
}
```

**地址宏定义**：

```c
#define BEG_PRIV_ADDR  (&priv[0])           // 数组起始地址
#define END_PRIV_ADDR  (&priv[NR_SYS_PROCS]) // 数组结束地址
```

**遍历逻辑**：

```
priv[0] ──► s_alarm_timer 初始化
priv[1] ──► s_alarm_timer 初始化
    ...
priv[NR_SYS_PROCS-1] ──► s_alarm_timer 初始化
```

**设计特点**：
- 使用指针遍历，效率高
- 遍历所有系统进程的特权结构
- 每个特权结构包含一个告警定时器

#### 3.3.2 tmr_inittimer

`tmr_inittimer` 是一个宏，用于初始化定时器结构。

**宏定义**：

```c
#define tmr_inittimer(tp) (void)((tp)->tmr_func = NULL, (tp)->tmr_next = NULL)
```

**初始化内容**：

| 字段 | 初始值 | 含义 |
|------|--------|------|
| `tmr_func` | `NULL` | 无定时器回调函数 |
| `tmr_next` | `NULL` | 不在定时器链表中 |

**使用场景**：

```c
// 初始化特权结构的告警定时器
tmr_inittimer(&(sp->s_alarm_timer));

// 后续可通过 tmrs_settimer 设置定时器
tmrs_settimer(&timers, &sp->s_alarm_timer, expire_time, callback, arg, &old, &new);
```

**设计特点**：
- 使用宏而非函数，零调用开销
- 简单的字段清零，确保定时器处于未激活状态

### 3.4 调用向量初始化

调用向量初始化分为两步：先清空所有槽位，再注册具体处理函数。

**初始化代码**：

```c
/* Initialize the call vector to a safe default handler. Some system calls
 * may be disabled or nonexistant. Then explicitly map known calls to their
 * handler functions.
 */
for (i=0; i<NR_SYS_CALLS; i++) {
    call_vec[i] = NULL;
}
```

**两阶段初始化**：

| 阶段 | 操作 | 目的 |
|------|------|------|
| **1. 清空** | 所有槽位置 NULL | 安全默认，未注册调用返回错误 |
| **2. 注册** | `map(SYS_xxx, do_xxx)` | 建立调用号到处理函数的映射 |

**与 fork 的关系**：fork 系统调用在第二阶段通过 `map(SYS_FORK, do_fork)` 注册。

#### 3.4.1 清空调用向量

通过循环将所有调用向量槽位置为 NULL。

**清空代码**：

```c
for (i=0; i<NR_SYS_CALLS; i++) {
    call_vec[i] = NULL;
}
```

**清空后的状态**：

```
call_vec[0] = NULL
call_vec[1] = NULL
    ...
call_vec[NR_SYS_CALLS-1] = NULL
```

**为什么需要清空**：
- 静态数组未初始化时内容不确定
- 确保未注册的系统调用有明确的失败路径
- 防止调用随机内存地址导致系统崩溃

#### 3.4.2 安全默认处理

安全默认处理确保未注册或禁用的系统调用有明确的失败路径。

**分发时的 NULL 检查**：

```c
if (call_vec[call_nr])
    result = (*call_vec[call_nr])(caller, msg);
else {
    printf("Unused kernel call %d from %d\n", call_nr, caller->p_endpoint);
    result = EBADREQUEST;
}
```

**安全默认的原因**：

| 场景 | 无安全默认 | 有安全默认 |
|------|-----------|-----------|
| **调用未注册系统调用** | 调用随机地址 → 崩溃 | 返回 `EBADREQUEST` |
| **配置禁用系统调用** | 未定义行为 | 返回 `EBADREQUEST` |
| **恶意调用号** | 可能利用漏洞 | 安全拒绝 |

**设计原则**：
- **Fail-safe**：失败时系统仍能正常运行
- **明确错误**：调用者收到明确的错误码
- **可调试**：打印警告信息便于排查

### 3.5 系统调用注册

系统调用注册通过 `map` 宏完成，按功能类别分组。

**注册代码结构**：

```c
/* Process management. */
map(SYS_FORK, do_fork);
map(SYS_EXEC, do_exec);
map(SYS_CLEAR, do_clear);
map(SYS_EXIT, do_exit);
// ...

/* Signal handling. */
map(SYS_KILL, do_kill);
// ...

/* Device I/O. */
map(SYS_IRQCTL, do_irqctl);
// ...

/* Memory management. */
map(SYS_VMCTL, do_vmctl);
// ...

/* Copying. */
map(SYS_VIRCOPY, do_vircopy);
// ...

/* Clock. */
map(SYS_SETALARM, do_setalarm);
// ...

/* System control. */
map(SYS_ABORT, do_abort);
// ...
```

**注册顺序**：注册顺序不影响功能，仅影响代码可读性。

#### 3.5.1 进程管理类

进程管理类系统调用负责进程生命周期管理。

**注册代码**：

```c
/* Process management. */
map(SYS_FORK, do_fork);       // 创建子进程
map(SYS_EXEC, do_exec);       // 执行新程序
map(SYS_CLEAR, do_clear);     // 清理进程资源
map(SYS_EXIT, do_exit);       // 进程退出
map(SYS_PRIVCTL, do_privctl); // 特权控制
map(SYS_TRACE, do_trace);     // 跟踪操作
map(SYS_SETGRANT, do_setgrant); // 授权设置
map(SYS_RUNCTL, do_runctl);   // 运行控制
map(SYS_UPDATE, do_update);   // 进程更新
map(SYS_STATECTL, do_statectl); // 状态控制
```

**与 fork 的关系**：`SYS_FORK` 是进程管理的核心系统调用，位于进程管理类注册列表的首位。

##### 3.5.1.1 SYS_FORK 注册

`map(SYS_FORK, do_fork)` 将 fork 系统调用号映射到处理函数。

**语义分解**：

| 部分 | 含义 |
|------|------|
| `SYS_FORK` | fork 系统调用号（常量） |
| `do_fork` | fork 处理函数（函数指针） |
| `map` | 注册宏，建立映射关系 |

**展开效果**：

```c
// map(SYS_FORK, do_fork) 展开为：
{
    int call_index = SYS_FORK - KERNEL_CALL;
    assert(call_index >= 0 && call_index < NR_SYS_CALLS);
    call_vec[call_index] = do_fork;
}
```

**结果**：当用户调用 `fork()` 时，内核通过 `call_vec[SYS_FORK - KERNEL_CALL]` 找到 `do_fork` 函数并执行。

##### 3.5.1.2 fork 系统调用的位置

fork 系统调用位于进程管理类注册列表的**首位**。

**注册顺序**：

```c
/* Process management. */
map(SYS_FORK, do_fork);     // ← 第一位
map(SYS_EXEC, do_exec);
map(SYS_CLEAR, do_clear);
map(SYS_EXIT, do_exit);
// ...
```

**位置意义**：

| 方面 | 说明 |
|------|------|
| **代码组织** | 进程创建是进程管理的起点，逻辑上应排在首位 |
| **无功能影响** | 注册顺序不影响运行时行为，`call_vec` 是随机访问数组 |
| **可读性** | 将核心操作放在前面，便于理解代码结构 |

#### 3.5.2 信号处理类

信号处理类系统调用负责进程间信号传递和处理。

**注册代码**：

```c
/* Signal handling. */
map(SYS_KILL, do_kill);       // 发送信号
map(SYS_GETKSIG, do_getksig); // 获取内核信号
map(SYS_ENDKSIG, do_endksig); // 结束信号处理
map(SYS_SIGSEND, do_sigsend); // 发送 POSIX 信号
map(SYS_SIGRETURN, do_sigreturn); // 信号返回
```

**与 fork 的关系**：fork 创建的子进程继承父进程的信号处理设置，由 PM 模块处理。

#### 3.5.3 设备 I/O 类

设备 I/O 类系统调用负责硬件设备访问和中断控制。

**注册代码**：

```c
/* Device I/O. */
map(SYS_IRQCTL, do_irqctl);   // 中断控制
#if defined(__i386__)
map(SYS_DEVIO, do_devio);     // 设备 I/O（x86 特有）
map(SYS_VDEVIO, do_vdevio);   // 向量设备 I/O（x86 特有）
#endif
```

**平台相关性**：`SYS_DEVIO` 和 `SYS_VDEVIO` 仅在 x86 架构上注册，其他架构可能使用不同的 I/O 机制。

**与 fork 的关系**：驱动程序进程 fork 时，子进程通常不继承父进程的中断处理能力。

#### 3.5.4 内存管理类

内存管理类系统调用负责内存操作和 VM 交互。

**注册代码**：

```c
/* Memory management. */
map(SYS_MEMSET, do_memset);   // 内存设置
map(SYS_VMCTL, do_vmctl);     // VM 控制
```

**与 fork 的关系**：`SYS_VMCTL` 在 fork 中被 VM 模块调用，用于设置子进程的页表和内存映射。fork 的内存管理主要由 VM 模块处理，内核仅负责进程表操作。

#### 3.5.5 复制类

复制类系统调用负责跨进程内存复制和地址转换。

**注册代码**：

```c
/* Copying. */
map(SYS_UMAP, do_umap);           // 地址映射
map(SYS_UMAP_REMOTE, do_umap_remote); // 远程地址映射
map(SYS_VUMAP, do_vumap);         // 向量地址映射
map(SYS_VIRCOPY, do_vircopy);     // 虚拟地址复制
map(SYS_PHYSCOPY, do_copy);       // 物理地址复制
map(SYS_SAFECOPYFROM, do_safecopy_from); // 安全复制（从）
map(SYS_SAFECOPYTO, do_safecopy_to);     // 安全复制（到）
```

**与 fork 的关系**：fork 不直接使用复制类系统调用，但 VM 模块在设置子进程内存时可能使用这些功能。

#### 3.5.6 时钟类

时钟类系统调用负责定时器和时间相关功能。

**注册代码**：

```c
/* Clock. */
map(SYS_SETALARM, do_setalarm); // 设置告警定时器
map(SYS_TIMES, do_times);       // 获取进程时间
```

**与 fork 的关系**：fork 创建的子进程不继承父进程的告警定时器，定时器由 PM 模块在 fork 后重置。

#### 3.5.7 系统控制类

系统控制类系统调用负责系统级操作和诊断。

**注册代码**：

```c
/* System control. */
map(SYS_ABORT, do_abort);     // 系统中止
map(SYS_GETINFO, do_getinfo); // 获取系统信息
map(SYS_DIAGCTL, do_diagctl); // 诊断控制
```

**与 fork 的关系**：系统控制类系统调用与 fork 无直接关系，主要用于系统管理和调试。

---

## 4. Rust 设计决策

将 C 语言的系统调用初始化机制迁移到 Rust 需要考虑以下关键问题：

**核心挑战**：

| C 语言特性 | Rust 对应 | 挑战 |
|-----------|----------|------|
| 函数指针数组 | `[fn(&Proc, &Message) -> i32; N]` | 类型安全、生命周期 |
| 宏注册 | `const fn` 或 `build.rs` | 编译期执行 |
| 全局静态数组 | `static mut` 或 `OnceCell` | 安全性封装 |

**设计目标**：
- **类型安全**：利用 Rust 类型系统防止运行时错误
- **零成本抽象**：不引入额外的运行时开销
- **编译期检查**：尽可能在编译期捕获错误

### 4.1 系统调用注册

Rust 中有三种主要的系统调用注册方式：

**方案对比**：

| 方案 | 实现 | 优点 | 缺点 |
|------|------|------|------|
| **运行时注册** | `HashMap` 或数组 | 灵活、动态 | 运行时开销 |
| **宏注册** | 声明宏 | 编译期生成、零开销 | 语法复杂 |
| **const fn** | 常量函数 | 编译期计算、类型安全 | 功能受限 |

**推荐方案：宏注册 + 静态数组**

```rust
// 定义系统调用处理函数类型
type SyscallHandler = fn(&mut Proc, &Message) -> i32;

// 调用向量静态数组
static mut CALL_VEC: [Option<SyscallHandler>; NR_SYS_CALLS] = [None; NR_SYS_CALLS];

// 注册宏
macro_rules! map {
    ($call_nr:expr, $handler:expr) => {
        unsafe {
            CALL_VEC[$call_nr as usize] = Some($handler);
        }
    };
}
```

### 4.2 函数指针

Rust 的函数指针类型安全，但需要注意一些限制。

**函数指针定义**：

```rust
// 系统调用处理函数类型
type SyscallHandler = fn(&mut Proc, &Message) -> i32;

// 具体处理函数
fn do_fork(caller: &mut Proc, msg: &Message) -> i32 {
    // 实现...
    0
}
```

**与 C 的区别**：

| 特性 | C | Rust |
|------|---|------|
| 类型安全 | 弱（可强制转换） | 强（类型必须匹配） |
| 闭包支持 | 无 | 有（但函数指针不能捕获环境） |
| 空指针 | `NULL` | `Option<fn(...)>` |

**安全封装**：

```rust
// 使用 Option 替代 NULL
let handler: Option<SyscallHandler> = Some(do_fork);

// 调用时安全检查
if let Some(h) = handler {
    h(caller, msg);
} else {
    return Err(SyscallError::NotRegistered);
}
```

### 4.3 初始化顺序

内核初始化有严格的顺序依赖，Rust 需要确保正确的初始化顺序。

**初始化依赖图**：

```
内核启动
    ↓
内存管理初始化
    ↓
进程表初始化
    ↓
system_init()  ← 系统调用初始化
    ↓
其他子系统初始化
    ↓
启动第一个用户进程
```

**Rust 中的管理方式**：

```rust
// 使用 OnceCell 确保单次初始化
use spin::Once;

static SYSTEM_INIT: Once = Once::new();

pub fn system_init() {
    SYSTEM_INIT.call_once(|| {
        // 1. 初始化 IRQ 钩子
        irq_hooks_init();
        // 2. 初始化告警定时器
        alarm_timers_init();
        // 3. 注册系统调用
        syscalls_register();
    });
}
```

**编译期检查**：使用 `const fn` 在编译期验证依赖关系。

---

## 5. 实现

本节给出系统调用初始化的 Rust 实现代码，包括：
- 系统调用向量定义
- 注册宏实现
- 初始化函数

**实现原则**：
- 遵循 Minix3 的设计语义
- 利用 Rust 类型系统增强安全性
- 保持零运行时开销

### 5.1 系统调用向量定义

```rust
use crate::proc::Proc;
use crate::message::Message;

pub const NR_SYS_CALLS: usize = 58;

/// 系统调用处理函数类型
pub type SyscallHandler = fn(&mut Proc, &Message) -> i32;

/// 系统调用向量
/// 
/// 使用 `Option<SyscallHandler>` 替代裸指针：
/// - `Some(handler)` 表示已注册
/// - `None` 表示未注册或禁用
pub struct SyscallVec {
    handlers: [Option<SyscallHandler>; NR_SYS_CALLS],
}

impl SyscallVec {
    pub const fn new() -> Self {
        Self {
            handlers: [None; NR_SYS_CALLS],
        }
    }

    pub fn register(&mut self, call_nr: usize, handler: SyscallHandler) {
        assert!(call_nr < NR_SYS_CALLS, "invalid syscall number");
        self.handlers[call_nr] = Some(handler);
    }

    pub fn dispatch(&self, call_nr: usize, caller: &mut Proc, msg: &Message) -> i32 {
        match self.handlers.get(call_nr).and_then(|h| *h) {
            Some(handler) => handler(caller, msg),
            None => {
                log::warn!("unregistered syscall {} from {}", call_nr, caller.endpoint());
                -22 // EBADREQUEST
            }
        }
    }
}
```

### 5.2 注册宏实现

```rust
/// 系统调用注册宏
/// 
/// 对应 C 语言的 `map(SYS_FORK, do_fork)`
/// 
/// # 示例
/// 
/// ```rust
/// map!(SYS_FORK, do_fork);
/// ```
macro_rules! map {
    ($call_nr:expr, $handler:expr) => {
        {
            const CALL_NR: usize = $call_nr as usize;
            assert!(CALL_NR < NR_SYS_CALLS, "syscall number out of range");
            CALL_VEC.register(CALL_NR, $handler);
        }
    };
}

/// 全局系统调用向量
static mut CALL_VEC: SyscallVec = SyscallVec::new();

/// 获取系统调用向量的安全访问
pub fn syscall_vec() -> &'static mut SyscallVec {
    unsafe { &mut CALL_VEC }
}
```

**与 C 宏的对比**：

| 特性 | C `map` | Rust `map!` |
|------|---------|-------------|
| 编译期检查 | `assert`（运行时） | `const` + `assert!`（编译期） |
| 类型安全 | 弱 | 强 |
| 空间分配 | 无 | 无 |

### 5.3 初始化函数

```rust
use spin::Once;

static INIT_DONE: Once = Once::new();

/// 系统调用子系统初始化
/// 
/// 对应 C 语言的 `system_init()`
pub fn system_init() {
    INIT_DONE.call_once(|| {
        // 1. 初始化 IRQ 钩子
        irq_hooks_init();
        
        // 2. 初始化告警定时器
        alarm_timers_init();
        
        // 3. 注册系统调用
        syscalls_register();
    });
}

fn irq_hooks_init() {
    // 初始化 IRQ 钩子数组
    crate::irq::init_hooks();
}

fn alarm_timers_init() {
    // 初始化所有进程的告警定时器
    crate::timer::init_alarm_timers();
}

fn syscalls_register() {
    let vec = syscall_vec();
    
    // 进程管理类
    map!(SYS_FORK, do_fork);
    map!(SYS_EXEC, do_exec);
    map!(SYS_CLEAR, do_clear);
    map!(SYS_EXIT, do_exit);
    
    // 信号处理类
    map!(SYS_KILL, do_kill);
    
    // 设备 I/O 类
    map!(SYS_IRQCTL, do_irqctl);
    
    // 内存管理类
    map!(SYS_VMCTL, do_vmctl);
    
    // 复制类
    map!(SYS_VIRCOPY, do_vircopy);
    
    // 时钟类
    map!(SYS_SETALARM, do_setalarm);
    
    // 系统控制类
    map!(SYS_GETINFO, do_getinfo);
}
```

### 5.4 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_syscall_vec_new() {
        let vec = SyscallVec::new();
        // 所有槽位应为 None
        for i in 0..NR_SYS_CALLS {
            assert!(vec.handlers[i].is_none());
        }
    }
    
    #[test]
    fn test_syscall_vec_register() {
        let mut vec = SyscallVec::new();
        
        fn test_handler(_caller: &mut Proc, _msg: &Message) -> i32 {
            42
        }
        
        vec.register(0, test_handler);
        assert!(vec.handlers[0].is_some());
    }
    
    #[test]
    fn test_syscall_vec_dispatch_unregistered() {
        let vec = SyscallVec::new();
        let mut proc = Proc::new_test();
        let msg = Message::empty();
        
        let result = vec.dispatch(0, &mut proc, &msg);
        assert_eq!(result, -22); // EBADREQUEST
    }
    
    #[test]
    #[should_panic(expected = "invalid syscall number")]
    fn test_syscall_vec_register_out_of_range() {
        let mut vec = SyscallVec::new();
        
        fn test_handler(_caller: &mut Proc, _msg: &Message) -> i32 {
            0
        }
        
        vec.register(NR_SYS_CALLS + 1, test_handler);
    }
}
```

---

## 6. 参见

- [12-kernel-call](12-kernel-call.md) - kernel_call 函数
- [15-do-fork-validate](15-do-fork-validate.md) - do_fork 实现
