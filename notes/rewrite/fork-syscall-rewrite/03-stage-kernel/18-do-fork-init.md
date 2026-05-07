# 18-do-fork-init - do_fork 子进程初始化

> 本文档分析 `minix3/minix/kernel/system/do_fork.c` 第 111-130 行，讲解 do_fork 函数的子进程初始化部分。

---

## 1. 概述

子进程初始化是 fork 系统调用的最后阶段，确保子进程以正确的状态开始运行。在完成进程结构复制、端点生成、返回值设置等操作后，内核需要对子进程进行一系列初始化：

1. **运行状态设置**：子进程初始不可运行，等待调度器分配时间片
2. **记账重置**：清零子进程的调度统计信息
3. **CPU 时间重置**：清零子进程的 CPU 周期计数
4. **特权处理**：如果父进程是系统进程，子进程需要降级为普通用户权限
5. **返回值设置**：向调用者（PM）返回子进程的端点和消息地址

这些初始化操作确保子进程从干净的状态开始，避免继承父进程的运行时状态，同时为 PM 后续的进程管理提供必要信息。

### 1.1 子进程状态

子进程初始化完成后处于以下状态：

| 状态项 | 值 | 说明 |
|--------|-----|------|
| `p_rts_flags` | `RTS_NO_QUANTUM` | 无时间片，不可运行 |
| `p_user_time` | 0 | 用户态时间清零 |
| `p_sys_time` | 0 | 内核态时间清零 |
| `p_cpu_time_left` | 0 | 剩余 CPU 时间清零 |
| `p_cycles` | 0 | 总周期数清零 |
| `p_endpoint` | 新端点 | 唯一标识符 |
| `p_reg.retreg` | 0 | 返回值为 0 |
| `p_name` | `父进程名*F` | 名称追加标记 |

如果父进程是系统进程：
| 状态项 | 值 | 说明 |
|--------|-----|------|
| `p_priv` | `USER_PRIV_ID` | 降级为用户权限 |
| `p_rts_flags` | `RTS_NO_QUANTUM | RTS_NO_PRIV` | 等待特权设置 |

子进程需要等待 PM 设置新的特权后才能运行。

### 1.2 与 fork 的关系

子进程初始化在 fork 流程中的位置：

```
fork 流程
│
├── 1. 参数验证（15-do-fork-validate）
│
├── 2. 进程结构复制（16-do-fork-copy）
│       ├── FPU 状态保存
│       ├── 整体复制
│       ├── FPU 指针恢复
│       ├── 端点代数递增
│       └── 端点生成
│
├── 3. 返回值设置（17-do-fork-endpoint）
│       ├── retreg = 0
│       ├── 时间统计重置
│       ├── 杂项标志清除
│       └── 进程名称修改
│
├── 4. 子进程初始化（本节）
│       ├── RTS_NO_QUANTUM 设置
│       ├── 记账重置
│       ├── CPU 时间重置
│       ├── 特权处理
│       └── 返回值设置
│
└── 5. 后续处理（19-do-fork-priv）
        ├── VM inhibit 设置
        ├── 信号状态清理
        └── 页表清零
```

子进程初始化是 fork 的核心阶段，完成子进程运行状态的最终设置。

---

## 2. C 源码分析

本节逐行分析 `do_fork.c` 第 89-112 行的代码，涵盖运行状态设置、记账重置、CPU 时间重置、特权处理和返回值设置。这些操作确保子进程以正确的初始状态等待调度。

### 2.1 运行状态设置

运行状态设置对应源码第 89-91 行：

```c
RTS_SET(rpc, RTS_NO_QUANTUM);
reset_proc_accounting(rpc);
```

第一条语句设置 `RTS_NO_QUANTUM` 标志，表示子进程没有时间片，不能被调度运行。第二条语句重置进程的记账信息。

子进程初始不可运行是必要的：调度器尚未为其分配时间片，直接运行会导致调度错误。

#### 2.1.1 RTS_NO_QUANTUM 设置

`RTS_SET(rpc, RTS_NO_QUANTUM)` 设置子进程的 `RTS_NO_QUANTUM` 标志。

`RTS_SET` 宏定义为：
```c
#define RTS_SET(p, f)  ((p)->p_rts_flags |= (f))
```

`RTS_NO_QUANTUM` 标志的含义：
- 进程没有分配时间片（quantum）
- 进程不可运行（`p_rts_flags != 0` 意味着不可运行）
- 需要调度器分配时间片后才能运行

这是子进程初始状态的正确设置：fork 完成后，PM 会通知调度器为子进程分配时间片，调度器清除 `RTS_NO_QUANTUM` 标志，子进程才能被调度。

#### 2.1.2 子进程不可运行

子进程初始不可运行的原因：

1. **时间片未分配**：调度器负责分配时间片，fork 时子进程尚未获得时间片

2. **避免调度错误**：如果子进程立即可运行，调度器可能选择执行它，但时间片为 0 会导致立即时钟中断

3. **同步初始化**：PM 需要在子进程运行前完成其他初始化（如内存映射、文件描述符等）

4. **调度器控制**：让调度器决定何时让子进程运行，而非内核直接调度

Minix3 的调度模型：
- 内核只负责低级调度（选择下一个运行的进程）
- 调度器服务（SCHED）负责高级调度（分配时间片、设置优先级）
- PM 负责进程生命周期管理

子进程需要等待 PM 和 SCHED 完成初始化后才能运行。

#### 2.1.3 等待调度

子进程等待调度器调度的流程：

1. **fork 完成**：子进程处于 `RTS_NO_QUANTUM` 状态

2. **PM 通知 SCHED**：PM 调用 `sched_fork` 通知调度器有新进程

3. **SCHED 分配时间片**：调度器为子进程分配时间片，调用内核的 `sched_set_quantum`

4. **内核清除标志**：内核清除 `RTS_NO_QUANTUM`，子进程变为可运行

5. **调度器调度**：调度器在合适的时机选择子进程运行

这种设计将进程创建和调度决策分离，使调度器可以灵活地控制进程的启动时机和优先级。

### 2.2 记账重置

记账重置对应源码第 91 行：

```c
reset_proc_accounting(rpc);
```

这个函数调用重置子进程的调度记账信息。记账信息用于统计进程的调度行为，如入队次数、IPC 次数、被抢占次数等。

#### 2.2.1 reset_proc_accounting 调用

`reset_proc_accounting(rpc)` 重置进程的调度记账字段。

记账字段包括：
- `p_accounting.enter_queue`：进入队列的时间戳
- `p_accounting.time_in_queue`：在队列中等待的总时间
- `p_accounting.dequeues`：出队次数
- `p_accounting.ipc_sync`：同步 IPC 次数
- `p_accounting.ipc_async`：异步 IPC 次数
- `p_accounting.preempted`：被抢占次数

重置这些字段确保子进程的调度统计从零开始，不继承父进程的调度历史。

在 Rust 实现中，这对应 `Accounting::new()` 或 `Accounting::reset()` 方法。

#### 2.2.2 记账字段

记账字段重置的含义：

| 字段 | 重置值 | 说明 |
|------|--------|------|
| `enter_queue` | 0 | 未在队列中 |
| `time_in_queue` | 0 | 累计等待时间为 0 |
| `dequeues` | 0 | 出队次数为 0 |
| `ipc_sync` | 0 | 同步 IPC 次数为 0 |
| `ipc_async` | 0 | 异步 IPC 次数为 0 |
| `preempted` | 0 | 被抢占次数为 0 |

这些统计信息用于：
- 调度器决策（如优先级调整）
- 性能分析
- 系统监控

子进程从零开始统计，确保数据的准确性和公平性。

### 2.3 CPU 时间重置

CPU 时间重置对应源码第 93-96 行：

```c
rpc->p_cpu_time_left = 0;
rpc->p_cycles = 0;
rpc->p_kcall_cycles = 0;
rpc->p_kipc_cycles = 0;
```

这四条语句清零子进程的 CPU 时间相关字段。这些字段记录进程的 CPU 使用情况，子进程应该从零开始独立统计。

#### 2.3.1 p_cpu_time_left 重置

`rpc->p_cpu_time_left = 0` 将子进程的剩余 CPU 时间清零。

`p_cpu_time_left` 表示进程当前时间片的剩余 CPU 周期数。当时间片用完（此值降为 0）时，时钟中断处理程序会设置 `RTS_NO_QUANTUM` 标志，触发重新调度。

清零的含义：
1. 子进程没有剩余时间片
2. 与 `RTS_NO_QUANTUM` 标志一致
3. 等待调度器分配新时间片

这是子进程初始状态的正确表示：尚未获得时间片分配。

#### 2.3.2 p_cycles 重置

`rpc->p_cycles = 0` 将子进程的总 CPU 周期数清零。

`p_cycles` 记录进程自创建以来消耗的总 CPU 周期数（通过读取 TSC 或类似计数器）。这是一个高精度的 CPU 使用度量。

清零的含义：
1. 子进程的 CPU 使用从 fork 时刻开始计算
2. 不继承父进程的 CPU 使用量
3. 用于精确的性能分析和资源统计

与 `p_user_time` 和 `p_sys_time`（以时钟滴答为单位）不同，`p_cycles` 以 CPU 周期为单位，精度更高。

#### 2.3.3 p_kcall_cycles 重置

`rpc->p_kcall_cycles = 0` 将子进程的内核调用周期数清零。

`p_kcall_cycles` 记录进程在内核调用（系统调用）中消耗的 CPU 周期数。这用于区分用户态和内核态的 CPU 使用。

清零的含义：
1. 子进程的系统调用开销独立统计
2. 不继承父进程的系统调用开销
3. 用于系统调用性能分析

这个字段与 `p_sys_time` 类似，但以 CPU 周期为单位，精度更高。

#### 2.3.4 p_kipc_cycles 重置

`rpc->p_kipc_cycles = 0` 将子进程的 IPC 周期数清零。

`p_kipc_cycles` 记录进程在内核 IPC 操作中消耗的 CPU 周期数。IPC（进程间通信）是微内核的核心操作，单独统计有助于分析通信开销。

清零的含义：
1. 子进程的 IPC 开销独立统计
2. 不继承父进程的 IPC 开销
3. 用于 IPC 性能分析和优化

Minix3 作为微内核系统，IPC 频繁发生，单独统计 IPC 开销对性能优化很重要。

### 2.4 周期计数重置

周期计数重置对应源码第 98-99 行：

```c
rpc->p_tick_cycles = 0;
cpuavg_init(&rpc->p_cpuavg);
```

这两条语句重置子进程的 tick 周期计数和 CPU 平均负载。

#### 2.4.1 p_tick_cycles 重置

`rpc->p_tick_cycles = 0` 将子进程的 tick 周期数清零。

`p_tick_cycles` 记录进程在时钟中断处理中消耗的 CPU 周期数。时钟中断是内核的核心定时机制，单独统计有助于分析定时开销。

清零的含义：
1. 子进程的时钟处理开销独立统计
2. 不继承父进程的时钟开销
3. 用于定时系统性能分析

时钟中断周期性发生（通常每秒 60-100 次），累积的开销可能相当可观。

#### 2.4.2 p_cpuavg 初始化

`cpuavg_init(&rpc->p_cpuavg)` 初始化子进程的 CPU 平均负载结构。

`p_cpuavg` 是一个结构体，用于计算进程的 CPU 使用率平均值。平均负载通常使用指数移动平均（EMA）算法，需要初始化历史数据。

初始化的作用：
1. 清零历史数据
2. 设置初始平均值为 0
3. 准备好接收新的采样数据

CPU 平均负载用于：
- `top`、`ps` 等工具显示进程 CPU 使用率
- 调度器决策（如识别 CPU 密集型进程）
- 资源监控和限制

---

## 3. 特权处理

特权处理对应源码第 101-108 行：

```c
if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;
}
```

这段代码检查父进程是否是系统进程。如果是，子进程需要降级为普通用户权限，并设置 `RTS_NO_PRIV` 标志，等待 PM 设置新的特权。

### 3.1 系统进程检查

系统进程检查通过 `priv(rpp)->s_flags & SYS_PROC` 判断父进程是否是系统进程。

`priv(rpp)` 宏获取父进程的特权结构体指针，`s_flags` 是特权标志位，`SYS_PROC` 表示这是一个系统进程（如驱动、服务器）。

系统进程的特殊性：
1. 拥有更高的权限（如直接访问硬件）
2. 可以执行特权指令
3. 可能访问敏感数据

如果系统进程 fork，子进程不应该继承这些特权，否则可能导致安全问题。

#### 3.1.1 SYS_PROC 标志检查

`priv(rpp)->s_flags & SYS_PROC` 检查父进程的特权标志中是否设置了 `SYS_PROC`。

`SYS_PROC` 标志标识系统进程，包括：
- PM（进程管理器）
- VM（虚拟内存管理器）
- VFS（虚拟文件系统）
- RS（重启服务器）
- 各种驱动程序

这些进程拥有特殊权限，如：
- 执行特权系统调用
- 访问内核内存
- 直接操作硬件

如果普通用户进程能通过 fork 系统进程的子进程来获得这些权限，将是严重的安全漏洞。因此，系统进程 fork 的子进程必须降级。

#### 3.1.2 系统进程 fork

系统进程 fork 的特殊处理：

1. **特权降级**：子进程不继承父进程的系统特权，降级为普通用户权限

2. **设置 RTS_NO_PRIV**：子进程不可运行，直到 PM 设置新的特权

3. **等待 PM 处理**：PM 会为子进程分配适当的特权结构

这种处理的原因：
- **安全**：防止权限泄露
- **控制**：PM 决定子进程的最终权限
- **一致性**：所有进程的特权由 PM 统一管理

典型场景：
- 系统服务 fork 创建工作进程
- 工作进程需要不同的权限（通常更低）
- PM 根据配置设置工作进程的特权

### 3.2 特权降级

特权降级对应源码第 106-107 行：

```c
rpc->p_priv = priv_addr(USER_PRIV_ID);
rpc->p_rts_flags |= RTS_NO_PRIV;
```

第一条语句将子进程的特权指针设置为 `USER_PRIV_ID` 对应的特权结构。第二条语句设置 `RTS_NO_PRIV` 标志，表示子进程没有有效的特权设置。

#### 3.2.1 p_priv 设置

`rpc->p_priv = priv_addr(USER_PRIV_ID)` 将子进程的特权结构指针设置为用户进程的默认特权。

`priv_addr(USER_PRIV_ID)` 返回用户特权结构体的地址。`USER_PRIV_ID` 是预定义的特权 ID，对应普通用户进程的权限集。

用户特权包括：
- 基本的系统调用权限
- 受限的 IPC 权限
- 无硬件访问权限
- 无内核内存访问权限

这是"降级"操作：系统进程的子进程从高权限降级到普通用户权限。

#### 3.2.2 USER_PRIV_ID

`USER_PRIV_ID` 是预定义的特权 ID，标识普通用户进程的特权结构。

Minix3 的特权 ID 分配：

| ID | 用途 |
|-----|------|
| 0 | 内核任务 |
| 1-N | 系统服务（PM, VM, VFS, RS 等）|
| `USER_PRIV_ID` | 普通用户进程 |

`USER_PRIV_ID` 对应的特权结构定义了用户进程的权限边界：
- 允许的系统调用：`read`, `write`, `fork`, `exit` 等
- 允许的 IPC 目标：任意进程
- 禁止的操作：直接硬件访问、内核内存访问

系统进程 fork 的子进程被设置为这个"安全默认值"，然后由 PM 根据需要调整。

#### 3.2.3 RTS_NO_PRIV 设置

`rpc->p_rts_flags |= RTS_NO_PRIV` 设置子进程的 `RTS_NO_PRIV` 标志。

`RTS_NO_PRIV` 标志的含义：
- 进程没有有效的特权设置
- 进程不可运行
- 需要 PM 设置特权后才能运行

这个标志与 `RTS_NO_QUANTUM` 类似，都是阻止进程运行的标志。区别在于：
- `RTS_NO_QUANTUM`：等待时间片
- `RTS_NO_PRIV`：等待特权设置

两个标志可以同时设置，进程需要两个条件都满足才能运行。

#### 3.2.4 等待特权设置

子进程等待 PM 设置特权的流程：

1. **fork 完成**：子进程处于 `RTS_NO_QUANTUM | RTS_NO_PRIV` 状态

2. **PM 处理 fork**：PM 收到 fork 结果，知道子进程端点

3. **PM 设置特权**：PM 调用内核的 `sys_privctl` 为子进程设置特权

4. **内核清除 RTS_NO_PRIV**：特权设置成功后，内核清除标志

5. **PM 通知调度器**：PM 通知调度器为子进程分配时间片

6. **子进程可运行**：所有条件满足，子进程可以被调度

这种设计确保：
- PM 对所有进程的特权有完全控制
- 特权设置在进程运行前完成
- 安全性得到保证

---

## 4. 返回值设置

返回值设置对应源码第 110-112 行：

```c
m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint;
m_ptr->m_krn_lsys_sys_fork.msgaddr = rpp->p_delivermsg_vir;
```

这两条语句设置返回给调用者（PM）的信息：子进程的端点和消息缓冲区地址。

### 4.1 端点返回

端点返回对应源码第 111 行：

```c
m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint;
```

这条语句将子进程的端点写入返回消息。PM 通过这个端点识别子进程，并在后续操作中使用。

#### 4.1.1 m_krn_lsys_sys_fork.endpt 设置

`m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint` 将子进程的端点写入返回消息的 `endpt` 字段。

`m_ptr` 指向调用者（PM）发送的系统调用消息。内核在处理完系统调用后，将结果写入消息的输出字段。

`m_krn_lsys_sys_fork` 是消息结构中的一个联合体，用于 `SYS_FORK` 系统调用的输入和输出。`endpt` 字段是输出，返回子进程的端点。

PM 收到这个端点后：
1. 记录子进程的端点
2. 更新进程表
3. 通知其他服务（如 VM、调度器）

#### 4.1.2 子进程端点

返回子进程端点的含义：

1. **进程标识**：PM 需要知道子进程的端点才能管理它

2. **跨模块通信**：PM、VM、调度器等模块通过端点引用进程

3. **父子关系**：PM 记录父子进程关系，用于 `wait`、信号等操作

4. **返回给用户**：fork 系统调用返回给父进程的值就是子进程端点

端点是 Minix3 中进程的唯一标识，几乎所有进程相关操作都需要端点。返回端点是 fork 系统调用的核心输出。

### 4.2 消息地址返回

消息地址返回对应源码第 112 行：

```c
m_ptr->m_krn_lsys_sys_fork.msgaddr = rpp->p_delivermsg_vir;
```

这条语句将父进程的消息投递地址写入返回消息。这个地址用于 PM 理解父进程在 fork 时的 IPC 状态。

#### 4.2.1 m_krn_lsys_sys_fork.msgaddr 设置

`m_ptr->m_krn_lsys_sys_fork.msgaddr = rpp->p_delivermsg_vir` 将父进程的消息投递虚拟地址写入返回消息。

`p_delivermsg_vir` 是父进程准备接收消息的用户空间缓冲区地址。在 fork 前，父进程处于 `RTS_RECEIVING` 状态，正在等待消息。

返回这个地址的原因：
1. PM 需要知道父进程的 IPC 状态
2. VM 可能需要处理这个地址的内存映射
3. 用于调试和诊断

这是 fork 同步要求的一部分：父进程必须在接收状态，内核才能确定消息缓冲区位置。

#### 4.2.2 消息缓冲区地址

返回消息缓冲区地址的含义：

1. **IPC 状态记录**：PM 知道父进程正在等待消息

2. **内存映射参考**：VM 可能需要复制或调整这个地址的映射

3. **同步验证**：确认父进程确实在正确的 IPC 状态

`p_delivermsg_vir` 是用户空间虚拟地址，指向父进程准备接收消息的缓冲区。fork 后，子进程继承了这个地址，但可能需要不同的处理。

PM 使用这个信息来：
- 更新进程的 IPC 状态
- 决定是否需要向子进程投递消息
- 处理 fork 期间的挂起 IPC

---

## 5. Rust 设计决策

子进程初始化的 Rust 实现需要考虑：

1. **状态管理**：使用原子操作或类型系统管理进程状态
2. **特权抽象**：通过 trait 或 newtype 抽象特权结构
3. **消息传递**：类型安全的消息结构

关键设计决策：
- 使用 `RtsFlags` 类型封装运行时状态标志
- 使用 `Privilege` 类型表示特权
- 使用 `Message` 类型表示 IPC 消息

### 5.1 初始化方法

子进程初始化方法的设计：

```rust
impl KProcess {
    /// 初始化 fork 创建的子进程
    ///
    /// 设置初始运行状态、重置记账信息、清零 CPU 时间。
    pub fn init_fork_child(&self) {
        // 设置 RTS_NO_QUANTUM
        self.p_rts_flags.set(rts::NO_QUANTUM);
        
        // 重置记账信息
        self.p_accounting.reset();
        
        // 清零 CPU 时间
        self.p_cpu_time_left.store(0, Ordering::Release);
        self.p_cycles.add_cycles(0);
    }
}
```

这个方法封装了子进程初始化的核心逻辑，在 `fork_from` 之后调用。

### 5.2 状态管理

进程状态管理的设计：

1. **RtsFlags 类型**：封装原子标志位操作

```rust
pub struct RtsFlags(AtomicU32);

impl RtsFlags {
    pub fn is_runnable(&self) -> bool { self.load() == 0 }
    pub fn set(&self, flags: u32) { self.0.fetch_or(flags, Ordering::AcqRel); }
    pub fn clear(&self, flags: u32) { self.0.fetch_and(!flags, Ordering::AcqRel); }
}
```

2. **状态检查**：提供便捷方法

```rust
impl KProcess {
    pub fn is_runnable(&self) -> bool { self.p_rts_flags.is_runnable() }
    pub fn needs_quantum(&self) -> bool { self.p_rts_flags.is_set(rts::NO_QUANTUM) }
    pub fn needs_priv(&self) -> bool { self.p_rts_flags.is_set(rts::NO_PRIV) }
}
```

这种设计将状态管理逻辑集中，避免散落的位操作。

### 5.3 特权处理

特权处理的设计：

1. **Privilege 类型**：表示进程特权

```rust
pub struct Privilege {
    pub flags: u32,
    pub allowed_calls: BitSet,
    pub allowed_targets: BitSet,
}
```

2. **特权检查**：类型安全的方法

```rust
impl KProcess {
    pub fn is_system_process(&self) -> bool {
        self.p_priv.flags & SYS_PROC != 0
    }
    
    pub fn downgrade_to_user(&mut self) {
        self.p_priv = Privilege::user_default();
        self.p_rts_flags.set(rts::NO_PRIV);
    }
}
```

3. **PM 接口**：特权设置系统调用

```rust
pub fn sys_privctl(endpoint: Endpoint, priv_id: PrivId) -> Result<(), Error> {
    // 设置进程特权
    // 清除 RTS_NO_PRIV
}
```

这种设计将特权管理集中化，便于安全审计和权限控制。

---

## 6. 实现

本节给出子进程初始化的 Rust 实现，包括状态设置、记账重置和特权处理。这些操作已在 `KProcess::fork_from` 方法中部分实现。

### 6.1 初始化方法

初始化方法的 Rust 实现：

```rust
impl KProcess {
    /// 初始化 fork 创建的子进程
    ///
    /// 对应 Minix3 do_fork.c 第 89-99 行。
    /// 设置 RTS_NO_QUANTUM、重置记账、清零 CPU 时间。
    pub fn init_fork_child(&self) {
        // 设置 RTS_NO_QUANTUM：子进程无时间片
        self.p_rts_flags.set(rts::NO_QUANTUM);
        
        // 重置记账信息
        self.p_accounting.reset();
        
        // 清零 CPU 时间相关字段
        // 注意：这些字段在 fork_from 中已通过 Accounting::new() 初始化
    }
    
    /// 处理系统进程 fork 的特权降级
    ///
    /// 对应 Minix3 do_fork.c 第 101-108 行。
    pub fn handle_priv_fork(&mut self, parent_is_system: bool) {
        if parent_is_system {
            // 降级为用户权限
            self.p_priv = Privilege::user_default();
            // 设置 RTS_NO_PRIV
            self.p_rts_flags.set(rts::NO_PRIV);
        }
    }
}
```

这些方法在 `fork_from` 中调用，完成子进程的完整初始化。

### 6.2 特权处理方法

特权处理方法的 Rust 实现：

```rust
/// 特权 ID 类型
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PrivId(pub u32);

impl PrivId {
    pub const KERNEL: PrivId = PrivId(0);
    pub const USER: PrivId = PrivId(USER_PRIV_ID);
}

/// 特权结构体
pub struct Privilege {
    pub id: PrivId,
    pub flags: u32,
}

impl Privilege {
    pub const SYS_PROC: u32 = 0x01;
    
    /// 创建默认用户特权
    pub fn user_default() -> Self {
        Self {
            id: PrivId::USER,
            flags: 0,
        }
    }
    
    /// 检查是否是系统进程
    pub fn is_system(&self) -> bool {
        self.flags & Self::SYS_PROC != 0
    }
}

impl KProcess {
    /// 检查父进程是否是系统进程，并处理特权降级
    pub fn fork_handle_priv(&mut self, parent_priv: &Privilege) {
        if parent_priv.is_system() {
            self.p_priv = Privilege::user_default();
            self.p_rts_flags.set(rts::NO_PRIV);
        }
    }
}
```

### 6.3 单元测试

子进程初始化的单元测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_fork_child_sets_no_quantum() {
        let proc = KProcess::new(5, Endpoint(5));
        proc.p_rts_flags.clear(rts::SLOT_FREE);
        
        proc.init_fork_child();
        
        assert!(proc.p_rts_flags.is_set(rts::NO_QUANTUM));
        assert!(!proc.is_runnable());
    }

    #[test]
    fn test_init_fork_child_resets_accounting() {
        let proc = KProcess::new(5, Endpoint(5));
        proc.p_accounting.record_ipc_sync();
        proc.p_accounting.record_ipc_sync();
        
        proc.init_fork_child();
        
        assert_eq!(proc.p_accounting.ipc_sync.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_fork_handle_priv_system_parent() {
        let mut child = KProcess::new(10, Endpoint::from_generation_slot(1, 10));
        let parent_priv = Privilege { id: PrivId(1), flags: Privilege::SYS_PROC };
        
        child.fork_handle_priv(&parent_priv);
        
        assert!(child.p_rts_flags.is_set(rts::NO_PRIV));
        assert_eq!(child.p_priv.id, PrivId::USER);
    }

    #[test]
    fn test_fork_handle_priv_user_parent() {
        let mut child = KProcess::new(10, Endpoint::from_generation_slot(1, 10));
        let parent_priv = Privilege { id: PrivId::USER, flags: 0 };
        
        child.fork_handle_priv(&parent_priv);
        
        assert!(!child.p_rts_flags.is_set(rts::NO_PRIV));
    }
}
```

这些测试验证：
- `RTS_NO_QUANTUM` 正确设置
- 记账信息正确重置
- 特权降级正确处理

---

## 7. 参见

- [17-do-fork-endpoint](17-do-fork-endpoint.md) - 端点生成
- [19-do-fork-priv](19-do-fork-priv.md) - 特权处理
- [09-priv-struct](09-priv-struct.md) - 特权结构体
