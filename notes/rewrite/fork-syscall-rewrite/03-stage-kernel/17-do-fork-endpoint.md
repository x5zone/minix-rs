# 17-do-fork-endpoint - do_fork 端点生成

> 本文档分析 `minix3/minix/kernel/system/do_fork.c` 第 91-110 行，讲解 do_fork 函数的端点生成部分。

---

## 1. 概述

端点生成是 fork 系统调用中确定子进程身份的关键步骤。在 Minix3 微内核架构中，端点（endpoint）是进程的唯一标识符，用于 IPC 通信和进程管理。fork 创建的子进程必须拥有与父进程不同的端点，否则系统无法区分两个进程。

端点生成涉及两个层面：
1. **代数递增**：子进程槽位的端点代数递增，确保新端点与旧端点不同
2. **字段修正**：子进程需要独立的返回值、时间统计、定时器状态等

本节分析的代码（do_fork.c 第 74-87 行）涵盖返回值设置、时间统计重置、杂项标志清除、虚拟定时器重置和进程名称修改。这些操作确保子进程以正确的初始状态开始运行，同时保持与父进程的必要区分。

### 1.1 端点机制

Minix3 的端点机制是微内核 IPC 的核心。每个进程拥有一个唯一的端点标识符（`endpoint_t`），用于：

- **IPC 通信**：`send(endpoint, msg)`、`receive(endpoint, msg)` 等系统调用使用端点指定通信对象
- **进程查找**：内核通过端点快速定位进程结构体
- **身份验证**：端点包含代数（generation），防止与已退出进程的端点混淆

端点的结构为 `(generation << 15) + slot`，其中 slot 是进程表槽位号，generation 是槽位重用计数器。这种设计使得：
- 同一槽位的不同进程实例拥有不同端点
- 旧端点引用在进程退出后自动失效
- 服务重启后客户端能检测到对端变化

关于端点机制的完整分析，参见 [20-endpoint](20-endpoint.md) 和 [endpoint 概念](../../concepts/endpoint.md)。

### 1.2 fork 时的端点

fork 时子进程端点的生成已在 [16-do-fork-copy](16-do-fork-copy.md) 中详细分析。本节关注的是端点生成后的其他字段修正：

1. **返回值设置**：子进程的 `p_reg.retreg = 0`，使其从 fork 返回 0，与父进程返回子进程 PID 区分
2. **时间统计重置**：子进程的 `p_user_time` 和 `p_sys_time` 清零，开始独立的时间统计
3. **杂项标志清除**：清除虚拟定时器、性能分析、单步调试等标志
4. **虚拟定时器重置**：子进程不继承父进程的虚拟定时器
5. **进程名称修改**：在进程名后追加 `"*F"` 标记 fork 来源

这些修正操作确保子进程以干净、独立的状态开始运行，避免继承父进程的运行时状态。

---

## 2. C 源码分析

本节逐行分析 `do_fork.c` 第 74-87 行的代码，涵盖返回值设置、时间统计重置、杂项标志清除、虚拟定时器重置和进程名称修改。这些操作在端点生成之后执行，完成子进程字段的最终修正。

### 2.1 返回值设置

返回值设置对应源码第 74 行：

```c
rpc->p_reg.retreg = 0;   /* child sees pid = 0 to know it is child */
```

这条语句将子进程的返回寄存器设置为 0。在 fork 系统调用中，父进程和子进程都会从同一个系统调用返回点继续执行，但返回值不同：
- **父进程**：返回子进程的 PID（正整数）
- **子进程**：返回 0

这种设计使得进程可以通过返回值判断自己是父进程还是子进程，从而执行不同的代码路径。

#### 2.1.1 p_reg.retreg 设置

`rpc->p_reg.retreg = 0` 设置子进程系统调用返回值为 0。`p_reg` 是进程的寄存器保存区，`retreg` 是其中用于存放系统调用返回值的寄存器。

在 i386 架构中，`retreg` 对应 `eax` 寄存器。系统调用返回时，内核将 `p_reg.retreg` 的值恢复到 `eax`，用户态通过读取 `eax` 获取返回值。

fork 的语义要求：
- 父进程从 fork 返回子进程的 PID
- 子进程从 fork 返回 0

父进程的返回值由 PM（进程管理器）设置，子进程的返回值由内核在此处设置。

#### 2.1.2 子进程返回值

子进程从 fork 返回 0 是 Unix/POSIX 的标准语义。这个设计有几个目的：

1. **区分父子进程**：返回值是区分父子进程的唯一方式，因为 fork 后两个进程的代码、数据、堆栈完全相同

2. **简化编程模型**：进程可以写简单的条件判断：
   ```c
   pid_t pid = fork();
   if (pid == 0) {
       // 子进程代码
   } else {
       // 父进程代码
   }
   ```

3. **避免 PID 冲突**：如果子进程返回自己的 PID，父进程和子进程可能返回相同的值（当子进程 PID 等于父进程返回的 PID 时），导致混淆

返回 0 是一个特殊的"无效 PID"，因为 PID 0 通常保留给内核或 idle 进程。

#### 2.1.3 父子进程区分

通过 fork 的返回值区分父子进程是 Unix 的经典设计：

| 进程 | fork 返回值 | 含义 |
|------|------------|------|
| 父进程 | > 0（子进程 PID） | 成功创建了子进程 |
| 父进程 | -1 | fork 失败 |
| 子进程 | 0 | 我是新创建的子进程 |

这种设计的精妙之处在于：

1. **单次调用，两次返回**：fork 是唯一一个"调用一次，返回两次"的系统调用

2. **对称性**：父子进程从完全相同的状态开始执行，唯一的区别是返回值

3. **信息传递**：父进程通过返回值获得子进程的 PID，便于后续的 wait、kill 等操作

4. **子进程自识别**：子进程返回 0，可以立即知道自己是子进程，无需额外查询

### 2.2 时间统计重置

时间统计重置对应源码第 75-76 行：

```c
rpc->p_user_time = 0;    /* set all the accounting times to 0 */
rpc->p_sys_time = 0;
```

这两条语句将子进程的用户态时间和内核态时间清零。时间统计用于进程记账、资源限制和性能分析，子进程应该从零开始独立统计，而非继承父进程的累计时间。

#### 2.2.1 p_user_time 重置

`rpc->p_user_time = 0` 将子进程的用户态运行时间清零。

`p_user_time` 记录进程在用户态执行的时钟滴答数。每次时钟中断时，如果当前进程正在用户态执行，内核会递增其 `p_user_time`。

用户态时间用于：
- **进程记账**：统计进程消耗的 CPU 时间
- **资源限制**：`setrlimit(RLIMIT_CPU)` 设置的 CPU 时间限制
- **性能分析**：`times()` 系统调用返回进程的时间统计

子进程清零 `p_user_time` 确保其 CPU 时间统计从 fork 时刻开始，独立于父进程。

#### 2.2.2 p_sys_time 重置

`rpc->p_sys_time = 0` 将子进程的内核态运行时间清零。

`p_sys_time` 记录进程在内核态执行的时钟滴答数。进程执行系统调用时，其执行时间计入 `p_sys_time` 而非 `p_user_time`。

内核态时间用于：
- **系统调用开销分析**：区分用户态和内核态的 CPU 时间
- **资源限制**：某些系统可能对内核态时间也设限
- **性能调优**：识别系统调用密集的进程

子进程清零 `p_sys_time` 与清零 `p_user_time` 的原因相同：独立统计，不继承父进程的累计时间。

#### 2.2.3 子进程时间统计

子进程的时间统计从 0 开始是 fork 语义的自然要求：

1. **独立性**：子进程是独立的执行实体，其资源消耗应该独立计算

2. **公平性**：如果子进程继承父进程的时间统计，可能导致：
   - 子进程刚创建就超过 CPU 时间限制
   - 时间统计无法反映子进程的实际消耗

3. **记账准确**：父进程和子进程的时间统计之和，等于 fork 后两个进程各自的实际消耗

4. **POSIX 语义**：`times()` 系统调用返回的是"从进程创建以来"的时间，子进程从 fork 时刻算起

Minix3 中，时间统计的单位是时钟滴答（clock ticks），典型值为每秒 60 或 100 次。

### 2.3 杂项标志清除

杂项标志清除对应源码第 78-79 行：

```c
rpc->p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | MF_SC_TRACE | MF_SPROF_SEEN | MF_STEP);
```

这条语句清除子进程的多个杂项标志位。这些标志与定时器、调试和性能分析相关，子进程不应该继承父进程的这些状态。

#### 2.3.1 标志清除操作

`rpc->p_misc_flags &= ~(...)` 是位清除操作，将括号中列出的标志位清零，其他位保持不变。

位操作原理：
- `~(flags)` 产生一个掩码，其中要清除的位为 0，其他位为 1
- `&=` 将掩码与原值按位与，效果是清除指定位的值

例如，假设 `p_misc_flags = 0b11010110`，要清除 `MF_VIRT_TIMER (0b00000010)`：
```
~MF_VIRT_TIMER = 0b11111101
0b11010110 & 0b11111101 = 0b11010100
```

这种操作方式可以一次清除多个标志，且不影响其他标志的值。

#### 2.3.2 MF_VIRT_TIMER 清除

`MF_VIRT_TIMER` 标志表示进程设置了虚拟定时器（virtual timer）。虚拟定时器是 POSIX `setitimer(ITIMER_VIRTUAL)` 设置的定时器，仅在进程用户态执行时递减，到期时发送 `SIGVTALRM` 信号。

清除 `MF_VIRT_TIMER` 的含义：
1. 子进程不继承父进程的虚拟定时器设置
2. 子进程的虚拟定时器处于禁用状态
3. 子进程不会因为父进程的定时器到期而收到信号

这是合理的：定时器是进程特定的资源，子进程应该独立设置自己的定时器。

#### 2.3.3 MF_PROF_TIMER 清除

`MF_PROF_TIMER` 标志表示进程设置了性能分析定时器（profiling timer）。性能分析定时器是 POSIX `setitimer(ITIMER_PROF)` 设置的定时器，在进程用户态和内核态执行时都递减，到期时发送 `SIGPROF` 信号。

清除 `MF_PROF_TIMER` 的含义：
1. 子进程不继承父进程的性能分析定时器
2. 子进程不会因为父进程的性能分析而收到 `SIGPROF`
3. 如果需要对子进程进行性能分析，需要重新设置

性能分析定时器通常用于 gprof 等工具，子进程独立运行，不应继承父进程的分析状态。

#### 2.3.4 MF_SC_TRACE 清除

`MF_SC_TRACE` 标志表示进程正在被系统调用跟踪（system call tracing）。这通常由调试器或 strace 类工具设置，用于监控进程的系统调用。

清除 `MF_SC_TRACE` 的含义：
1. 子进程不被调试器跟踪
2. 子进程的系统调用不会被记录
3. 调试会话不会自动扩展到子进程

这是调试器语义的一部分：fork 创建的子进程默认不被调试，除非调试器显式跟踪。这避免了调试会话的意外扩散。

#### 2.3.5 MF_SPROF_SEEN 清除

`MF_SPROF_SEEN` 标志表示进程已被系统性能分析器（system profiler）观察到。这是一个内部状态标志，用于优化性能分析器的采样逻辑。

清除 `MF_SPROF_SEEN` 的含义：
1. 子进程对性能分析器来说是"新"进程
2. 性能分析器会在下次采样时记录子进程
3. 子进程的性能数据独立于父进程

这个标志的清除确保性能分析器能正确处理新创建的进程。

#### 2.3.6 MF_STEP 清除

`MF_STEP` 标志表示进程处于单步执行模式。单步执行是调试器的功能，每次执行一条指令后暂停，便于逐指令调试。

清除 `MF_STEP` 的含义：
1. 子进程不在单步模式下运行
2. 子进程可以正常连续执行
3. 调试器的单步状态不继承到子进程

与 `MF_SC_TRACE` 类似，这是调试器语义的一部分：子进程默认正常执行，除非调试器显式设置单步模式。

### 2.4 虚拟定时器重置

虚拟定时器重置对应源码第 80-81 行：

```c
rpc->p_virt_left = 0;    /* disable, clear the process-virtual timers */
rpc->p_prof_left = 0;
```

这两条语句将子进程的虚拟定时器和性能分析定时器的剩余时间清零。即使标志位被清除，定时器的剩余时间也需要重置，确保定时器完全禁用。

#### 2.4.1 p_virt_left 重置

`rpc->p_virt_left = 0` 将子进程的虚拟定时器剩余时间清零。

`p_virt_left` 存储虚拟定时器（ITIMER_VIRTUAL）的剩余时钟滴答数。当此值递减到 0 时，内核向进程发送 `SIGVTALRM` 信号。

清零的作用：
1. 定时器立即禁用（剩余时间为 0）
2. 不会触发信号
3. 如果进程后续调用 `setitimer(ITIMER_VIRTUAL, ...)`，定时器从新值开始

这是"禁用定时器"的标准方式：剩余时间清零 + 标志位清除。

#### 2.4.2 p_prof_left 重置

`rpc->p_prof_left = 0` 将子进程的性能分析定时器剩余时间清零。

`p_prof_left` 存储性能分析定时器（ITIMER_PROF）的剩余时钟滴答数。当此值递减到 0 时，内核向进程发送 `SIGPROF` 信号。

清零的作用与 `p_virt_left` 相同：
1. 定时器立即禁用
2. 不会触发 `SIGPROF` 信号
3. 子进程可以独立设置自己的性能分析定时器

两个定时器的清零确保子进程的定时器状态完全干净。

#### 2.4.3 定时器不继承

子进程不继承父进程的定时器是 POSIX 标准的要求，原因如下：

1. **资源独立性**：定时器是进程特定的资源，每个进程应该有独立的定时器设置

2. **信号语义**：定时器到期发送信号给"设置定时器的进程"。如果子进程继承定时器，信号应该发给谁？

3. **避免意外**：父进程可能设置了短间隔定时器，子进程继承后可能立即收到信号，导致意外行为

4. **语义清晰**：`setitimer` 设置的是"当前进程"的定时器，fork 后子进程是"新进程"

POSIX 标准明确规定：
- `setitimer` 设置的定时器不继承
- `alarm` 设置的定时器不继承
- 定时器相关的信号处理函数可以继承（因为共享代码段）

Minix3 通过清除标志位和清零剩余时间来实现这一语义。

### 2.5 进程名称修改

进程名称修改对应源码第 83-87 行：

```c
namelen = strlen(rpc->p_name);
#define FORKSTR "*F"
if(namelen+strlen(FORKSTR) < sizeof(rpc->p_name))
    strcat(rpc->p_name, FORKSTR);
```

这段代码在子进程名称后追加 `"*F"` 字符串，标记这是一个 fork 创建的进程。进程名称用于调试和日志输出，追加标记有助于识别进程来源。

#### 2.5.1 namelen 计算

`namelen = strlen(rpc->p_name)` 计算子进程名称的当前长度。

子进程的 `p_name` 在整体复制 `*rpc = *rpp` 后与父进程相同。`strlen` 计算的是字符串的实际长度（不含结尾的 `\0`）。

这个长度用于后续的边界检查：确保追加 `"*F"` 后不会超过 `p_name` 数组的最大容量。`p_name` 是固定大小的字符数组（`PROC_NAME_LEN = 16`），追加前必须检查是否会溢出。

#### 2.5.2 FORKSTR 定义

`#define FORKSTR "*F"` 定义了追加到 fork 子进程名称的标记字符串。

`"*F"` 的含义：
- `*` 表示这是一个"派生"或"副本"
- `F` 表示来源是 fork

这个标记在调试时很有用：
- `ps` 命令可以看到进程是否是 fork 创建的
- 内核日志可以区分同名进程的不同实例
- 多次 fork 会累积标记，如 `"init*F*F"` 表示 fork 了两次

标记只有 2 个字符，设计为短小以适应 `p_name` 的 16 字节限制。

#### 2.5.3 名称追加

`strcat(rpc->p_name, FORKSTR)` 将 `"*F"` 追加到子进程名称末尾。

`strcat` 是 C 标准库函数，将源字符串追加到目标字符串末尾。操作过程：
1. 找到 `rpc->p_name` 的结尾（`\0` 位置）
2. 将 `"*F"` 复制到该位置
3. 新的字符串以 `\0` 结尾

例如：
- 原：`"init\0"`
- 后：`"init*F\0"`

如果进程多次 fork，名称会累积标记：`"init*F*F*F..."`，直到达到 16 字节限制。

#### 2.5.4 名称长度检查

`if(namelen+strlen(FORKSTR) < sizeof(rpc->p_name))` 检查追加后是否会超过数组容量。

检查逻辑：
- `namelen`：当前名称长度
- `strlen(FORKSTR)` = 2：要追加的字符串长度
- `sizeof(rpc->p_name)` = 16：数组总容量
- 条件 `<` 而非 `<=`：确保还有空间放 `\0`

如果条件不满足（名称已接近 16 字节），则不追加标记，避免缓冲区溢出。

这是一种防御性编程：宁可丢失标记，也不能破坏内存安全。在 Minix3 内核中，缓冲区溢出可能导致系统崩溃或安全漏洞。

---

## 3. 端点生成流程

端点生成的完整流程已在 [16-do-fork-copy](16-do-fork-copy.md) 中详细分析。本节简要回顾：

1. **提取代数**：从子进程槽位的当前端点提取 generation
2. **代数递增**：`++gen`，超过最大值时回绕为 1
3. **构造端点**：`_ENDPOINT(gen, p_nr)` 组合代数和进程号
4. **赋值端点**：设置子进程的 `p_endpoint`

这个流程确保子进程拥有唯一且有效的新端点。

### 3.1 代数递增

端点代数递增的逻辑为：

```c
if(++gen >= _ENDPOINT_MAX_GENERATION)
    gen = 1;
```

递增规则：
1. 先递增：`++gen`
2. 检查上限：`_ENDPOINT_MAX_GENERATION = 65534`
3. 超过则回绕：`gen = 1`（不使用 0）

代数从子进程槽位的当前端点提取，而非从父进程端点提取。这确保了：
- 同一槽位每次重用，代数递增
- 旧端点引用自动失效
- 新进程获得唯一的端点

详细分析参见 [16-do-fork-copy](16-do-fork-copy.md) 第 2.4 节。

### 3.2 端点组合

端点由 `_ENDPOINT(gen, p_nr)` 宏构造：

```c
#define _ENDPOINT(g, p) ((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
```

组合方式：
- `gen << 15`：代数左移 15 位，占据高位
- `+ p`：加上进程号，占据低位

结果是一个 32 位整数：
```
┌──┬────────────────┬────────────────┐
│符│  generation    │      slot      │
│号│  (16 bits)     │   (15 bits)    │
│位│  版本号        │   进程槽位     │
└──┴────────────────┴────────────────┘
 31 30            15 14             0
```

这种编码方式使得代数和进程号可以通过位运算高效提取。

### 3.3 唯一性保证

端点唯一性由以下机制保证：

1. **槽位唯一**：每个进程在进程表中占据唯一槽位，`p_nr` 不会重复

2. **代数递增**：同一槽位每次被新进程使用，代数递增

3. **代数回绕安全**：回绕为 1 而非 0，避免与硬编码端点冲突

4. **运行时验证**：`isokendpt` 宏验证端点的有效性

在 fork 场景中：
- 子进程槽位与父进程不同
- 子进程代数基于自己的槽位递增
- 子进程端点与父进程端点在两个维度上都不同

关于端点唯一性的完整讨论，参见 [20-endpoint](20-endpoint.md)。

---

## 4. Rust 设计决策

端点生成的 Rust 实现已在 [16-do-fork-copy](16-do-fork-copy.md) 第 5 节给出。本节讨论返回值设置、时间统计重置等操作的 Rust 实现。

### 4.1 端点类型

Rust 中端点类型的设计已在 `minix-types` crate 中实现：

```rust
#[repr(transparent)]
pub struct Endpoint(pub i32);

impl Endpoint {
    pub const fn from_generation_slot(generation: i32, slot: i32) -> Self;
    pub const fn slot(self) -> i32;
    pub const fn generation(self) -> i32;
    pub const fn fork_new_endpoint(current: Endpoint, child_slot: i32) -> Endpoint;
}
```

设计要点：
1. **Newtype 模式**：`Endpoint(i32)` 提供类型安全
2. **`#[repr(transparent)]`**：与 C 的 `endpoint_t` ABI 兼容
3. **`const fn`**：编译期可计算的运算
4. **封装代数逻辑**：`fork_new_endpoint` 封装递增和回绕

这种设计在保持与 Minix3 语义一致的同时，利用 Rust 的类型系统提供编译期检查。

### 4.2 代数管理

端点代数的管理方式：

1. **存储位置**：代数编码在端点值中，不单独存储

2. **提取方式**：通过 `generation()` 方法从端点提取

3. **递增时机**：
   - fork 创建子进程时
   - 进程退出后槽位被重用时

4. **回绕处理**：超过 `ENDPOINT_MAX_GENERATION` 时回绕为 1

Rust 实现中，代数管理封装在 `Endpoint` 类型中：

```rust
pub const fn fork_new_endpoint(current: Endpoint, child_slot: i32) -> Endpoint {
    let mut generation = current.generation();
    generation += 1;
    if generation >= Self::ENDPOINT_MAX_GENERATION {
        generation = 1;
    }
    Endpoint::from_generation_slot(generation, child_slot)
}
```

这种封装确保代数管理逻辑集中、可测试、不易出错。

### 4.3 返回值设计

子进程返回值的设计涉及寄存器状态管理：

1. **返回值寄存器**：在 i386 中是 `eax`，在 ARM 中是 `r0`

2. **设置时机**：fork 系统调用返回前，内核设置子进程的返回寄存器

3. **Rust 抽象**：通过 trait 抽象不同架构的寄存器访问

```rust
/// 进程寄存器状态 trait
pub trait ProcessRegs {
    /// 设置系统调用返回值
    fn set_return_value(&mut self, value: i32);
    
    /// 获取系统调用返回值
    fn get_return_value(&self) -> i32;
}
```

Mock 实现使用简单的字段存储：

```rust
pub struct MockRegs {
    retreg: i32,
}

impl ProcessRegs for MockRegs {
    fn set_return_value(&mut self, value: i32) {
        self.retreg = value;
    }
    fn get_return_value(&self) -> i32 {
        self.retreg
    }
}
```

这样，`fork_from` 可以通过 trait 方法设置返回值，无需关心具体架构。

---

## 5. 实现

本节给出返回值设置、时间统计重置、杂项标志清除等操作的 Rust 实现。这些操作已在 `KProcess::fork_from` 方法中实现，参见 [16-do-fork-copy](16-do-fork-copy.md) 第 5 节。

### 5.1 端点生成方法

端点生成方法已在 `minix-types` crate 中实现：

```rust
impl Endpoint {
    /// Endpoint 代数最大值
    pub const ENDPOINT_MAX_GENERATION: i32 = i32::MAX / ENDPOINT_GENERATION_SIZE - 1;

    /// 为 fork 生成子进程的新端点
    pub const fn fork_new_endpoint(current_endpoint: Endpoint, child_slot: i32) -> Endpoint {
        let mut generation = current_endpoint.generation();
        generation += 1;
        if generation >= Self::ENDPOINT_MAX_GENERATION {
            generation = 1;
        }
        Endpoint::from_generation_slot(generation, child_slot)
    }
}
```

这个方法封装了 Minix3 中代数递增和回绕的逻辑，作为纯函数实现，易于测试和复用。

### 5.2 返回值设置方法

返回值设置需要在 `KProcess` 中添加方法。由于返回值涉及架构相关的寄存器，应该通过 trait 抽象：

```rust
impl KProcess {
    /// 设置子进程的 fork 返回值为 0
    ///
    /// 对应 Minix3 的 `rpc->p_reg.retreg = 0`
    pub fn set_fork_child_return(&mut self) {
        // 在 Mock 实现中，直接设置 p_reg.retreg
        // 在真实架构实现中，设置对应的返回值寄存器
        self.p_reg.retreg = 0;
    }
}
```

注意：当前的 `KProcess` 结构体中 `p_reg` 字段尚未定义。在完整的实现中，`p_reg` 应该是一个泛型或 trait object，以支持不同架构的寄存器状态。

在 `fork_from` 方法中，返回值设置可以通过初始化时指定：子进程的返回值寄存器在创建时设为 0。

### 5.3 单元测试

端点生成的单元测试已在 `minix-types` crate 中实现：

```rust
#[test]
fn test_fork_new_endpoint_increment() {
    let current = Endpoint::from_generation_slot(5, 10);
    let new = Endpoint::fork_new_endpoint(current, 10);
    assert_eq!(new.generation(), 6);
    assert_eq!(new.slot(), 10);
}

#[test]
fn test_fork_new_endpoint_wraparound() {
    let max_gen = Endpoint::ENDPOINT_MAX_GENERATION;
    let current = Endpoint::from_generation_slot(max_gen, 10);
    let new = Endpoint::fork_new_endpoint(current, 10);
    assert_eq!(new.generation(), 1);
    assert_eq!(new.slot(), 10);
}

#[test]
fn test_fork_new_endpoint_different_slot() {
    let current = Endpoint::from_generation_slot(3, 5);
    let new = Endpoint::fork_new_endpoint(current, 20);
    assert_eq!(new.generation(), 4);
    assert_eq!(new.slot(), 20);
}
```

这些测试覆盖了：
- 正常递增
- 代数回绕
- 不同槽位

运行 `cargo test -p minix-types` 验证所有测试通过。

---

## 6. 参见

- [16-do-fork-copy](16-do-fork-copy.md) - 进程结构复制
- [18-do-fork-init](18-do-fork-init.md) - 子进程初始化
- [20-endpoint](20-endpoint.md) - 端点机制
