# 19-debug-serial: 内核调试基础设施

> **分类**: Kernel 多核与补全
> **源码**: `minix3/minix/kernel/debug.c`(563行), `arch_system.c` ser_dump_* 系列, `serial.h`
> **说明**: ser_debug 串口输出、ser_dump_proc、dump 全部队列——内核无 printf 时的调试手段

---

## 1. 概述

### 1.1 概念定义/作用

**内核调试基础设施**是 Minix3 内核在无标准输出（`printf`）环境下进行诊断和调试的工具集。由于内核运行在最高特权级，无法使用用户态的 C 库函数，Minix3 提供了以下调试机制：

1. **串口输出（serial debug）**：通过 COM1/COM2 串口输出调试信息，不依赖显示驱动
2. **进程转储（proc dump）**：打印进程表、调度队列、IPC 状态等内核数据结构
3. **调度队列一致性检查**：验证就绪队列的完整性，检测调度错误
4. **消息追踪（IPC dump）**：条件编译下记录和打印所有 IPC 消息传递
5. **内核消息缓冲区（kmessages）**：环形缓冲区记录内核输出，供 `dmesg` 读取

这些机制在内核 panic、死锁诊断、调度错误检测等场景下至关重要。

### 1.2 与 Minix3 的对应关系

| 功能 | 函数 | 源文件 |
|------|------|--------|
| 调度队列检查 | `runqueues_ok()` | debug.c:16 |
| RTS 标志字符串 | `rtsflagstr()` | debug.c:136 |
| MISC 标志字符串 | `miscflagstr()` | debug.c:163 |
| 打印进程信息 | `print_proc()` | debug.c:249 |
| 递归打印依赖 | `print_proc_recursive()` | debug.c:309 |
| IPC 消息打印 | `printmsg()` | debug.c:386 |
| 消息类型名 | `mtypename()` | debug.c:315 |
| 串口输出 | `ser_debug()` | arch_system.c |
| 进程转储 | `ser_dump_proc()` | arch_system.c |
| 栈回溯 | `proc_stacktrace()` | arch_system.c |
| 内核消息缓冲区 | `kmess` | main.c |

### 1.3 关键状态/机制说明

**条件编译控制**：大部分调试功能通过条件编译宏控制：

- `DEBUG_SCHED_CHECK`：调度队列一致性检查
- `DEBUG_DUMPIPC`：IPC 消息追踪
- `DEBUG_DUMPIPCF`：IPC 过滤器追踪
- `DEBUG_SERIAL`：串口调试输出
- `DEBUG_TIME_LOCK`：锁持有时间统计

**串口输出**：`ser_debug()` 直接操作 COM1 的 I/O 端口（0x3F8），不依赖任何内核子系统。这使得串口输出在内核崩溃的任何阶段都可用——即使页表损坏或中断控制器失效。

**内核消息缓冲区**：`kinfo.kmess` 是一个环形缓冲区，`printf()` 的内核实现将输出同时写入此缓冲区。用户进程通过 `sys_getinfo(GET_KMESS)` 读取缓冲区内容，实现 `dmesg` 功能。

### 1.4 行为规则

1. **串口输出不依赖内核子系统**：`ser_debug()` 仅使用 I/O 端口操作，不使用自旋锁、内存分配等
2. **调度检查在关键点调用**：`runqueues_ok()` 在 `enqueue()`/`dequeue()` 等调度操作后被调用（条件编译）
3. **进程转储在 panic 时调用**：内核 panic 时自动调用 `ser_dump_proc()` 输出全部进程状态
4. **消息追踪不影响正确性**：IPC 消息追踪仅记录和打印，不修改消息内容
5. **内核消息缓冲区环形覆盖**：缓冲区满后覆盖最旧的消息

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 调试条件编译宏

| 宏 | 含义 |
|-----|------|
| `DEBUG_SCHED_CHECK` | 启用调度队列一致性检查 |
| `DEBUG_DUMPIPC` | 启用 IPC 消息追踪 |
| `DEBUG_DUMPIPCF` | 启用 IPC 过滤器追踪 |
| `DEBUG_SERIAL` | 启用串口调试输出 |
| `DEBUG_TIME_LOCK` | 启用锁持有时间统计 |
| `DEBUG_DUMPIPC_NAMES` | IPC 追踪中过滤特定进程名 |

#### 2.1.2 串口 I/O 端口

| 端口 | 用途 |
|------|------|
| `0x3F8` | COM1 数据寄存器 |
| `0x3F9` | COM1 中断使能寄存器 |
| `0x3FA` | COM1 中断标识寄存器 |
| `0x3FB` | COM1 线路控制寄存器 |
| `0x3FD` | COM1 线路状态寄存器 |

### 2.2 核心数据结构

#### 2.2.1 struct kmessages（内核消息缓冲区）

| 字段 | 类型 | 含义 |
|------|------|------|
| `km_buf[_KMESS_BUF_SIZE]` | `char[]` | 环形缓冲区 |
| `km_next` | `int` | 下一个写入位置 |
| `km_size` | `int` | 缓冲区中有效数据量 |

#### 2.2.2 proc.p_found（调试用字段）

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_found` | `int` | 调度队列检查中标记进程是否在队列中找到 |

### 2.3 关键函数分析

#### 2.3.1 runqueues_ok()——调度队列一致性检查

`minix3/minix/kernel/debug.c:16-107`

```c
int runqueues_ok_cpu(unsigned cpu)
int runqueues_ok(void)
```

**功能**：验证指定 CPU 的调度队列是否一致。

**行为**（逐项检查）：

1. **头尾一致性**：`rdy_head[q]` 非空则 `rdy_tail[q]` 必须非空，反之亦然
2. **尾节点无后继**：`rdy_tail[q]->p_nextready` 必须为 NULL
3. **指针合法性**：链表中的每个 `xp` 必须在进程表范围内、对齐正确、`p_magic == PMAGIC`
4. **非空闲进程**：链表中不应有 `RTS_SLOT_FREE` 的进程
5. **可运行性**：链表中的进程必须 `proc_is_runnable()` 为真
6. **优先级匹配**：进程的 `p_priority` 必须等于所在队列号 `q`
7. **无重复**：每个进程在链表中最多出现一次（通过 `p_found` 标记检测）
8. **尾节点正确**：链表最后一个节点必须是 `rdy_tail[q]`
9. **无遗漏**：所有可运行进程必须在某个队列中

返回 1 表示一致，0 表示发现错误。

#### 2.3.2 rtsflagstr() / miscflagstr()——标志位字符串化

`minix3/minix/kernel/debug.c:136-174`

```c
char *rtsflagstr(const u32_t flags)
char *miscflagstr(const u32_t flags)
```

**功能**：将 RTS/MISC 标志位转换为可读字符串。

**行为**：遍历所有标志位，若置位则追加标志名到静态字符串。例如 `rtsflagstr(0x8004)` 返回 `"RTS_SENDING RTS_NO_QUANTUM "`。

#### 2.3.3 print_proc()——打印进程信息

`minix3/minix/kernel/debug.c:249-275`

```c
void print_proc(struct proc *pp)
```

**功能**：打印进程的关键信息。

**输出格式**：
```
proc_nr: name endpoint prio priority time user/sys cycles 0x... cpu N pdbr 0x... rts FLAGS misc FLAGS sched scheduler sigmgr endpoint [blocked on: endpoint]
```

包含进程号、名称、endpoint、优先级、用户/系统时间、CPU 周期、CPU 编号、页表根、RTS 标志、MISC 标志、调度器、信号管理器、阻塞目标。

#### 2.3.4 print_proc_recursive()——递归打印依赖链

`minix3/minix/kernel/debug.c:277-312`

```c
void print_proc_recursive(struct proc *pp)
```

**功能**：递归打印进程及其阻塞依赖链。

**行为**：
1. 打印进程信息和栈回溯
2. 通过 `P_BLOCKEDON()` 找到进程阻塞在哪个 endpoint 上
3. 递归打印阻塞目标进程的信息
4. 递归深度限制为 `NR_PROCS`（检测循环依赖）

**使用场景**：死锁诊断——查看哪些进程互相阻塞。

#### 2.3.5 mtypename()——消息类型名称

`minix3/minix/kernel/debug.c:315-354`

```c
static const char *mtypename(int mtype, int *possible_callname)
```

**功能**：将消息类型编号转换为可读名称。

**行为**：使用 `extracted-mtype.h` 和 `extracted-errno.h` 生成的 switch-case 匹配消息类型名和错误码名。若同时匹配，返回 `"ERRNAME / CALLNAME"` 格式。

#### 2.3.6 printmsg()——打印 IPC 消息

`minix3/minix/kernel/debug.c:386-460`

```c
void printmsg(message *msg, struct proc *src, struct proc *dst,
    char operation, int printparams)
```

**功能**：打印 IPC 消息的详细信息。

**行为**：
1. 打印操作类型（`S`=发送, `R`=接收, `N`=通知, `A`=异步）
2. 打印源和目标进程
3. 打印消息类型名称
4. 若 `printparams` 为真，打印消息字段值

### 2.4 调用关系/调用点分析

#### 2.4.1 调试功能调用点

| 调用者 | 调用的调试函数 | 场景 |
|--------|--------------|------|
| `enqueue()` / `dequeue()` | `runqueues_ok()` | 调度操作后验证（`DEBUG_SCHED_CHECK`） |
| `mini_send()` / `mini_receive()` | `printmsg()` | IPC 消息追踪（`DEBUG_DUMPIPC`） |
| `panic()` | `ser_dump_proc()` | 内核 panic 时 |
| `switch_to_user()` | `do_ser_debug()` | 每次调度时串口调试（`DEBUG_SERIAL`） |
| `printf()` 内核实现 | `kmess` 缓冲区 | 记录内核输出 |

#### 2.4.2 panic 时的调试输出

```
panic(reason)
  ├─ printf("kernel panic: %s", reason)
  ├─ ser_dump_proc()     → 打印全部进程状态
  ├─ proc_stacktrace()   → 打印当前栈回溯
  └─ arch_shutdown()     → 停机
```

### 2.5 设计要点/特殊处理

#### 2.5.1 串口输出的底层性

`ser_debug()` 直接操作 COM1 的 I/O 端口，不经过任何内核抽象层。这使得它在以下极端场景中仍可用：
- 页表损坏（无法访问内核数据结构）
- 中断控制器失效（无法使用 `printf` 的中断驱动输出）
- 自旋锁死锁（无法获取任何锁）

代价是串口输出速度慢（115200 bps），仅用于关键调试信息。

#### 2.5.2 调度队列检查的完备性

`runqueues_ok()` 检查了调度队列的几乎所有可能错误：头尾不一致、链表环、优先级错配、空闲进程在队列中、可运行进程不在队列中、重复入队等。这种完备性检查在开发和调试阶段非常有价值，但性能开销较大——每次 `enqueue`/`dequeue` 后都执行 O(N) 检查。

#### 2.5.3 extracted-mtype.h 的自动生成

`mtypename()` 使用的 `extracted-mtype.h` 和 `extracted-errno.h` 是构建时自动生成的头文件，包含所有消息类型和错误码的 `IDENT(x)` 宏展开。这避免了手动维护消息类型名称列表——添加新消息类型时自动出现在调试输出中。

#### 2.5.4 内核消息缓冲区的环形设计

`kmess` 缓冲区使用环形覆盖策略：写入位置 `km_next` 递增，到达缓冲区末尾后回绕到起始位置。`km_size` 记录有效数据量，最大为缓冲区大小。用户进程读取时从 `km_next - km_size` 开始读取 `km_size` 字节。这种设计保证了最新的消息总是可用的，但旧消息可能被覆盖。

#### 2.5.5 进程依赖链的递归打印

`print_proc_recursive()` 通过 `P_BLOCKEDON()` 追踪阻塞依赖链，递归打印每个进程的信息和栈回溯。这在死锁诊断中极为有用——可以直观看到"A 等 B，B 等 C，C 等 A"的循环依赖。递归深度限制为 `NR_PROCS` 防止无限递归。
