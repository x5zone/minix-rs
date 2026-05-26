# 06-proc-struct: struct proc 进程控制块

> **分类**: Kernel 进程抽象
> **源码**: `minix3/minix/kernel/proc.h`(~160行结构体) + `const.h` RTS/misc 标志位
> **说明**: proc 结构体的所有字段 + RTS_FLAGS 逐位解释——内核"认识"一个进程的唯一方式

---

## 1. 概述

### 1.1 概念定义/作用

**进程控制块（Process Control Block, PCB）** 是操作系统内核用于描述和管理进程的核心数据结构。在 Minix3 中，`struct proc` 就是进程控制块的实现——内核通过它"认识"每一个进程，记录进程的运行状态、寄存器上下文、调度信息、IPC 通信状态、内存映射关系等全部元数据。

Minix3 是微内核架构，内核仅负责进程调度和 IPC 消息传递，不实现文件系统、设备驱动等功能。因此 `struct proc` 的设计聚焦于两个核心职责：

1. **进程调度**：记录优先级、时间片、就绪队列链接等，让内核决定"谁运行、运行多久"
2. **IPC 消息传递**：记录发送/接收状态、待传递消息、阻塞目标等，让内核完成进程间同步通信

`struct proc` 是内核中最重要的数据结构，几乎所有内核操作——从时钟中断处理到系统调用分发——都需要访问进程表。

### 1.2 与 Minix3 的对应关系

Minix3 的进程表是一个全局静态数组 `proc[NR_TASKS + NR_PROCS]`，定义在 `proc.h` 中：

- **前 NR_TASKS 个槽位**（索引 0 ~ NR_TASKS-1）：内核任务（IDLE、CLOCK、SYSTEM 等），`p_nr` 为负数
- **后 NR_PROCS 个槽位**（索引 NR_TASKS ~ NR_TASKS+NR_PROCS-1）：用户进程（PM、VFS、VM、INIT 等），`p_nr` 为非负数

关键常量：
- `NR_TASKS = 5`：内核任务数（ASYNCM=-5, IDLE=-4, CLOCK=-3, SYSTEM=-2, KERNEL=-1）
- `NR_PROCS = 256`：最大用户进程数
- `NR_SYS_PROCS = 64`：系统特权结构数

进程表通过 `proc_addr(n)` 宏按进程号快速索引，`BEG_PROC_ADDR` / `END_PROC_ADDR` 标记表边界，`BEG_USER_ADDR` 标记用户进程起始位置。

### 1.3 关键状态/机制说明

`struct proc` 的核心状态由两个标志位字段控制：

**p_rts_flags（运行时标志）**——决定进程是否可运行。规则极为简洁：**`p_rts_flags == 0` 时进程可运行，任何位被置位则进程不可运行**。这是 Minix3 进程状态管理的核心设计：不是用枚举值表示状态，而是用位图表示"不可运行的原因"。一个进程可以同时有多个不可运行原因（如正在发送且页缺失），只有所有原因都清除后才恢复可运行。

**p_misc_flags（杂项标志）**——记录不影响进程可运行性的辅助状态，如待投递消息、FPU 初始化、系统调用追踪等。

这两个字段的操作必须通过 `RTS_SET` / `RTS_UNSET` 宏完成——这些宏在修改标志位的同时自动维护调度队列：置位导致不可运行时自动 dequeue，清位导致可运行时自动 enqueue。

### 1.4 行为规则

1. **可运行性规则**：`p_rts_flags == 0` ⇔ 进程可运行。这是唯一判据，没有例外
2. **标志位原子性**：`RTS_SET` 置位时，若进程从可运行变为不可运行，自动调用 `dequeue()` 将其移出就绪队列；`RTS_UNSET` 清位时，若进程从不可运行变为可运行，自动调用 `enqueue()` 将其加入就绪队列
3. **多原因叠加**：一个进程可同时置位多个 RTS 标志（如 `RTS_SENDING | RTS_PAGEFAULT`），必须全部清除才可运行
4. **特权分离**：系统进程拥有独立的 `struct priv` 结构，用户进程共享一个默认特权结构。通过 `p_priv` 指针间接访问
5. **Endpoint 与进程号**：`p_nr` 是进程槽位号（不变），`p_endpoint` 是含 generation 的进程标识（slot 重用时递增 generation）
6. **魔数校验**：`p_magic` 固定为 `PMAGIC(0xC0FFEE1)`，用于运行时校验 proc 指针有效性

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 进程表规模常量

| 常量 | 值 | 定义位置 | 含义 |
|------|-----|---------|------|
| `NR_TASKS` | 5 | `minix3/minix/include/minix/com.h:56` | 内核任务数 |
| `NR_PROCS` | 256 | `minix3/minix/include/minix/sys_config.h:8` | 最大用户进程数 |
| `NR_SYS_PROCS` | 64 | `minix3/minix/include/minix/sys_config.h:9` | 系统特权结构数 |
| `MAX_NR_TASKS` | 1023 | `minix3/minix/include/minix/com.h:55` | endpoint 布局允许的最大任务数 |
| `PROC_NAME_LEN` | 16 | `minix3/minix/include/minix/type.h:145` | 进程名最大长度（含 `\0`） |
| `PMAGIC` | 0xC0FFEE1 | `minix3/minix/include/minix/const.h:164` | proc 指针有效性魔数 |

#### 2.1.2 内核任务 endpoint 定义

| 宏名 | endpoint 值 | 含义 |
|------|------------|------|
| `ASYNCM` | -5 | 异步消息通知任务 |
| `IDLE` | -4 | 空闲任务 |
| `CLOCK` | -3 | 时钟任务 |
| `SYSTEM` | -2 | 系统服务任务 |
| `KERNEL` | -1 | 内核伪进程 |

#### 2.1.3 RTS_FLAGS 运行时标志位

| 标志位 | 值 | 含义 | 置位场景 |
|--------|-----|------|---------|
| `RTS_SLOT_FREE` | 0x01 | 进程槽位空闲 | 进程退出或初始化时 |
| `RTS_PROC_STOP` | 0x02 | 进程被停止 | `sys_stop()` 或 IDLE 初始化 |
| `RTS_SENDING` | 0x04 | 进程阻塞于发送 | `mini_send()` 目标未就绪 |
| `RTS_RECEIVING` | 0x08 | 进程阻塞于接收 | `mini_receive()` 无消息可用 |
| `RTS_SIGNALED` | 0x10 | 有新内核信号到达 | 信号管理器发送信号 |
| `RTS_SIG_PENDING` | 0x20 | 信号处理中暂不可运行 | 信号处理流程中 |
| `RTS_P_STOP` | 0x40 | 进程被追踪（ptrace） | 调试器 attach |
| `RTS_NO_PRIV` | 0x80 | fork 的系统进程尚未获得特权 | `sys_fork()` 后特权未就绪 |
| `RTS_NO_ENDPOINT` | 0x100 | 进程不能发送/接收消息 | endpoint 未分配 |
| `RTS_VMINHIBIT` | 0x200 | 等待 VM 设置页表 | fork/exec 后页表未就绪 |
| `RTS_PAGEFAULT` | 0x400 | 进程有未处理的页缺失 | 访问未映射内存 |
| `RTS_VMREQUEST` | 0x800 | VM 内存请求的发起者 | 发起 VM 内存请求 |
| `RTS_VMREQTARGET` | 0x1000 | VM 内存请求的目标 | 作为 VM 内存请求目标 |
| `RTS_PREEMPTED` | 0x4000 | 被更高优先级进程抢占 | 调度时发现更高优先级 |
| `RTS_NO_QUANTUM` | 0x8000 | 时间片用完 | 时钟中断检测到量子耗尽 |
| `RTS_BOOTINHIBIT` | 0x10000 | 启动阶段等待 VM 就绪 | 系统启动初始化 |

#### 2.1.4 MISC_FLAGS 杂项标志位

| 标志位 | 值 | 含义 |
|--------|-----|------|
| `MF_REPLY_PEND` | 0x001 | IPC_REQUEST 的回复待处理 |
| `MF_VIRT_TIMER` | 0x002 | 进程虚拟定时器运行中 |
| `MF_PROF_TIMER` | 0x004 | 进程 profile 定时器运行中 |
| `MF_KCALL_RESUME` | 0x008 | 内核调用被中断需恢复 |
| `MF_DELIVERMSG` | 0x040 | 有消息待投递给此进程 |
| `MF_SIG_DELAY` | 0x080 | 发送完成后需发送信号 |
| `MF_SC_ACTIVE` | 0x100 | 系统调用追踪：正在系统调用中 |
| `MF_SC_DEFER` | 0x200 | 系统调用追踪：延迟系统调用 |
| `MF_SC_TRACE` | 0x400 | 系统调用追踪：触发系统调用事件 |
| `MF_FPU_INITIALIZED` | 0x1000 | FPU 寄存器已初始化（已使用浮点） |
| `MF_SENDING_FROM_KERNEL` | 0x2000 | 消息来自内核 |
| `MF_CONTEXT_SET` | 0x4000 | 不修改上下文 |
| `MF_SPROF_SEEN` | 0x8000 | profile 已观测此进程 |
| `MF_FLUSH_TLB` | 0x10000 | 运行前需刷新 TLB（SMP） |
| `MF_SENDA_VM_MISS` | 0x20000 | 异步发送因 VM 修改地址空间而失败 |
| `MF_STEP` | 0x40000 | 单步执行 |
| `MF_MSGFAILED` | 0x80000 | 消息传递失败 |
| `MF_NICED` | 0x100000 | 用户降低了进程最大优先级 |

#### 2.1.5 特权标志（s_flags）

| 标志位 | 值 | 含义 |
|--------|-----|------|
| `PREEMPTIBLE` | 0x002 | 进程可被抢占 |
| `BILLABLE` | 0x004 | 进程可被计费 |
| `DYN_PRIV_ID` | 0x008 | 特权 ID 动态分配 |
| `SYS_PROC` | 0x010 | 系统进程（拥有独立 priv 结构） |
| `CHECK_IO_PORT` | 0x020 | 检查 I/O 端口权限 |
| `CHECK_IRQ` | 0x040 | 检查 IRQ 权限 |
| `CHECK_MEM` | 0x080 | 检查内存映射权限 |
| `ROOT_SYS_PROC` | 0x100 | 根系统进程实例 |
| `VM_SYS_PROC` | 0x200 | VM 系统进程实例 |
| `LU_SYS_PROC` | 0x400 | 热更新系统进程实例 |
| `RST_SYS_PROC` | 0x800 | 重启的系统进程实例 |

### 2.2 核心数据结构

#### 2.2.1 struct proc（进程控制块）

定义于 `minix3/minix/kernel/proc.h:22-137`，按功能可分为以下字段组：

**寄存器与上下文**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_reg` | `struct stackframe_s` | 进程寄存器保存帧，上下文切换时保存/恢复 |
| `p_seg` | `struct segframe` | 段描述符（x86 下含 CR3 页表根指针、FPU 状态） |

**进程标识**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_nr` | `proc_nr_t` (int) | 进程槽位号，负数为内核任务，非负为用户进程，生命周期不变 |
| `p_priv` | `struct priv *` | 指向特权结构，系统进程有独立实例，用户进程共享默认实例 |
| `p_endpoint` | `endpoint_t` (int) | 含 generation 的进程标识，slot 重用时 generation 递增 |
| `p_name` | `char[16]` | 进程名（含 `\0`） |

**运行时状态**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_rts_flags` | `volatile u32_t` | 运行时标志，== 0 时可运行 |
| `p_misc_flags` | `volatile u32_t` | 杂项标志，不影响可运行性 |

**调度信息**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_priority` | `char` | 当前优先级 |
| `p_cpu_time_left` | `u64_t` | 剩余 CPU 时间 |
| `p_quantum_size_ms` | `unsigned` | 分配的时间量子（毫秒） |
| `p_scheduler` | `struct proc *` | 用户空间调度器进程，NULL 表示内核默认调度 |
| `p_cpu` | `unsigned` | 进程运行的 CPU 编号 |
| `p_nextready` | `struct proc *` | 就绪队列中下一个进程 |

**调度统计（p_accounting）**

| 字段 | 类型 | 含义 |
|------|------|------|
| `enter_queue` | `u64_t` | 入队时刻（CPU 周期） |
| `time_in_queue` | `u64_t` | 队列中等待时间 |
| `dequeues` | `unsigned long` | 出队次数 |
| `ipc_sync` | `unsigned long` | 同步 IPC 次数 |
| `ipc_async` | `unsigned long` | 异步 IPC 次数 |
| `preempted` | `unsigned long` | 被抢占次数 |

**时间统计**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_dequeued` | `clock_t` | 最近一次出队的 uptime |
| `p_user_time` | `clock_t` | 用户态时间（tick） |
| `p_sys_time` | `clock_t` | 内核态时间（tick） |
| `p_virt_left` | `clock_t` | 虚拟定时器剩余 tick |
| `p_prof_left` | `clock_t` | profile 定时器剩余 tick |
| `p_cycles` | `u64_t` | 消耗的 CPU 周期 |
| `p_kcall_cycles` | `u64_t` | 内核调用消耗的周期 |
| `p_kipc_cycles` | `u64_t` | IPC 消耗的周期 |
| `p_tick_cycles` | `u64_t` | 一个 tick 内累积的周期 |
| `p_cpuavg` | `struct cpuavg` | 运行 CPU 平均值（供 ps(1) 使用） |

**IPC 消息传递**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_caller_q` | `struct proc *` | 向此进程发送消息的进程队列头 |
| `p_q_link` | `struct proc *` | 发送等待队列中的链接 |
| `p_getfrom_e` | `endpoint_t` | 想从谁接收（RECEIVING 时有效） |
| `p_sendto_e` | `endpoint_t` | 想向谁发送（SENDING 时有效） |
| `p_pending` | `sigset_t` | 待处理的内核信号位图 |
| `p_sendmsg` | `message` | 发送方消息（SENDING 时有效） |
| `p_delivermsg` | `message` | 待投递给此进程的消息（MF_DELIVERMSG 时有效） |
| `p_delivermsg_vir` | `vir_bytes` | 消息投递目标虚拟地址 |

**VM 请求（p_vmrequest）**

| 字段 | 类型 | 含义 |
|------|------|------|
| `nextrestart` | `struct proc *` | VM 重启链中下一个进程 |
| `nextrequestor` | `struct proc *` | VM 请求链中下一个请求者 |
| `type` | `int` | 挂起操作类型（VMSTYPE_KERNELCALL/DELIVERMSG/MAP） |
| `saved.reqmsg` | `message` | 挂起的请求消息 |
| `req_type` | `int` | VM 请求类型 |
| `target` | `endpoint_t` | VM 请求目标 |
| `params.check.start` | `vir_bytes` | 内存范围起始 |
| `params.check.length` | `vir_bytes` | 内存范围长度 |
| `params.check.writeflag` | `u8_t` | 写访问标志 |
| `vmresult` | `int` | VM 处理结果 |

**调试与校验**

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_found` | `int` | 一致性检查变量 |
| `p_magic` | `int` | 有效性魔数（PMAGIC = 0xC0FFEE1） |
| `p_defer` | `struct { r1, r2, r3 }` | 延迟系统调用参数（MF_SC_DEFER 时有效） |

#### 2.2.2 struct stackframe_s（寄存器保存帧）

定义于 `minix3/minix/include/arch/i386/include/stackframe.h:17-36`，x86-32 架构：

| 字段 | 类型 | 含义 |
|------|------|------|
| `gs` | `u16_t` | 附加段寄存器 |
| `fs` | `u16_t` | 附加段寄存器 |
| `es` | `u16_t` | 附加段寄存器 |
| `ds` | `u16_t` | 数据段寄存器 |
| `di` | `reg_t` (u32) | 目标索引寄存器 |
| `si` | `reg_t` | 源索引寄存器 |
| `fp` | `reg_t` | 帧指针（bp） |
| `bx` | `reg_t` | 通用寄存器 |
| `dx` | `reg_t` | 通用寄存器 |
| `cx` | `reg_t` | 通用寄存器 |
| `retreg` | `reg_t` | 返回值寄存器（ax），也用于 IPC 状态码 |
| `pc` | `reg_t` | 程序计数器 |
| `cs` | `reg_t` | 代码段寄存器 |
| `psw` | `reg_t` | 程序状态字（标志寄存器） |
| `sp` | `reg_t` | 栈指针 |
| `ss` | `reg_t` | 栈段寄存器 |

字段排列顺序与 `pusha`/`popa` 指令及中断压栈顺序一致，确保汇编代码高效保存/恢复上下文。

#### 2.2.3 struct segframe（段/页表帧）

定义于 `minix3/minix/include/arch/i386/include/archtypes.h:32-37`，x86-32 架构：

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_cr3` | `reg_t` (u32) | 页表根物理地址（CR3 寄存器值） |
| `p_cr3_v` | `u32_t *` | 页表根虚拟地址 |
| `fpu_state` | `char *` | FPU 状态保存区指针 |
| `p_kern_trap_style` | `int` | 内核陷阱风格 |

#### 2.2.4 struct priv（特权结构）

定义于 `minix3/minix/kernel/priv.h:21-66`，系统进程拥有独立实例：

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_proc_nr` | `proc_nr_t` | 关联的进程号 |
| `s_id` | `sys_id_t` | 特权结构索引 |
| `s_flags` | `short` | 特权标志（PREEMPTIBLE/BILLABLE/SYS_PROC 等） |
| `s_init_flags` | `int` | 初始化标志 |
| `s_asyntab` | `vir_bytes` | 异步发送表地址（进程地址空间内） |
| `s_asynsize` | `size_t` | 异步发送表元素数 |
| `s_asynendpoint` | `endpoint_t` | 异步表所属的 endpoint |
| `s_trap_mask` | `short` | 允许的系统调用陷阱掩码 |
| `s_ipc_to` | `sys_map_t` | 允许的 IPC 目标进程位图 |
| `s_k_call_mask` | `bitchunk_t[]` | 允许的内核调用掩码 |
| `s_sig_mgr` | `endpoint_t` | 信号管理器 |
| `s_bak_sig_mgr` | `endpoint_t` | 备份信号管理器 |
| `s_notify_pending` | `sys_map_t` | 待处理通知位图 |
| `s_asyn_pending` | `sys_map_t` | 待处理异步消息位图 |
| `s_int_pending` | `irq_id_t` | 待处理硬件中断 |
| `s_sig_pending` | `sigset_t` | 待处理信号 |
| `s_ipcf` | `ipc_filter_t *` | IPC 过滤器 |
| `s_alarm_timer` | `minix_timer_t` | 同步闹钟定时器 |
| `s_stack_guard` | `reg_t *` | 内核任务栈保护字 |
| `s_diag_sig` | `char` | 诊断到达时是否发 SIGKMESS |
| `s_nr_io_range` | `int` | 允许的 I/O 端口范围数 |
| `s_io_tab` | `struct io_range[]` | I/O 端口范围表 |
| `s_nr_mem_range` | `int` | 允许的内存范围数 |
| `s_mem_tab` | `struct minix_mem_range[]` | 内存范围表 |
| `s_nr_irq` | `int` | 允许的 IRQ 线数 |
| `s_irq_tab` | `int[]` | IRQ 表 |
| `s_grant_table` | `vir_bytes` | grant 表地址 |
| `s_grant_entries` | `int` | grant 表条目数 |
| `s_grant_endpoint` | `endpoint_t` | grant 表所属 endpoint |
| `s_state_table` | `vir_bytes` | 状态表地址 |
| `s_state_entries` | `int` | 状态表条目数 |

#### 2.2.5 struct cpuavg（CPU 平均值）

定义于 `minix3/minix/include/minix/type.h:80-85`：

| 字段 | 类型 | 含义 |
|------|------|------|
| `ca_base` | `clock_t` | 当前秒槽起始时刻 |
| `ca_run` | `uint32_t` | 当前秒内运行 tick（FSCALE 定点） |
| `ca_last` | `uint32_t` | 上一秒运行 tick（FSCALE 定点） |
| `ca_avg` | `uint32_t` | 衰减平均 CPU 利用率（FSCALE 定点） |

### 2.3 关键函数分析

#### 2.3.1 proc_init()——进程表初始化

`minix3/minix/kernel/proc.c:119-159`

```c
void proc_init(void)
```

**功能**：初始化进程表和特权结构表，在内核启动早期调用。

**行为**：
1. 遍历 `proc[0]` ~ `proc[NR_TASKS + NR_PROCS - 1]`，对每个槽位：
   - 置 `p_rts_flags = RTS_SLOT_FREE`（标记空闲）
   - 置 `p_magic = PMAGIC`
   - 设 `p_nr` 从 `-NR_TASKS` 递增（内核任务为负，用户进程为非负）
   - 初始化 `p_endpoint = _ENDPOINT(0, p_nr)`（generation 为 0）
   - 清空调度器指针、优先级、时间片
   - 调用 `arch_proc_reset(rp)` 做架构相关初始化
2. 遍历 `priv[0]` ~ `priv[NR_SYS_PROCS - 1]`，对每个特权结构：
   - 置 `s_proc_nr = NONE`（标记空闲）
   - 设 `s_id` 为索引值
   - 建立 `ppriv_addr` 快速索引
   - 清空信号管理器
3. 初始化 IDLE 进程：每个 CPU 一个，设置 `p_endpoint = IDLE`，`p_priv = &idle_priv`，`p_rts_flags |= RTS_PROC_STOP`（永不调度）

#### 2.3.2 RTS_SET / RTS_UNSET / RTS_SETFLAGS 宏

`minix3/minix/kernel/proc.h:206-231`

**RTS_SET(rp, f)**：置位标志并自动出队

```c
#define RTS_SET(rp, f)
    do {
        const int rts = (rp)->p_rts_flags;
        (rp)->p_rts_flags |= (f);
        if(rts_f_is_runnable(rts) && !proc_is_runnable(rp)) {
            dequeue(rp);
        }
    } while(0)
```

- 先保存旧标志，置位新标志
- 若进程从可运行变为不可运行（旧标志为 0，新标志非 0），自动调用 `dequeue()` 将其移出就绪队列
- 使用 `do...while(0)` 确保宏在 if/else 中安全使用

**RTS_UNSET(rp, f)**：清位标志并自动入队

```c
#define RTS_UNSET(rp, f)
    do {
        int rts;
        rts = (rp)->p_rts_flags;
        (rp)->p_rts_flags &= ~(f);
        if(!rts_f_is_runnable(rts) && proc_is_runnable(rp)) {
            enqueue(rp);
        }
    } while(0)
```

- 先保存旧标志，清位指定标志
- 若进程从不可运行变为可运行（旧标志非 0，新标志为 0），自动调用 `enqueue()` 将其加入就绪队列

**RTS_SETFLAGS(rp, f)**：直接设置标志值

```c
#define RTS_SETFLAGS(rp, f)
    do {
        if(proc_is_runnable(rp) && (f)) { dequeue(rp); }
        (rp)->p_rts_flags = (f);
    } while(0)
```

- 若进程当前可运行且新值非 0，先 dequeue 再赋值
- 不做 enqueue——调用者需自行确保新状态正确

#### 2.3.3 辅助宏/函数

| 宏/函数 | 定义位置 | 功能 |
|---------|---------|------|
| `proc_is_runnable(p)` | proc.h:170 | `p->p_rts_flags == 0` |
| `proc_is_preempted(p)` | proc.h:172 | `p->p_rts_flags & RTS_PREEMPTED` |
| `proc_no_quantum(p)` | proc.h:173 | `p->p_rts_flags & RTS_NO_QUANTUM` |
| `proc_ptr_ok(p)` | proc.h:174 | `p->p_magic == PMAGIC` |
| `proc_used_fpu(p)` | proc.h:175 | `p->p_misc_flags & MF_FPU_INITIALIZED` |
| `proc_kernel_scheduler(p)` | proc.h:178-179 | `p->p_scheduler == NULL \|\| p->p_scheduler == p` |
| `P_BLOCKEDON(p)` | proc.h:187-198 | 返回进程阻塞在哪个 endpoint 上（优先检查 SENDING） |
| `proc_addr(n)` | proc.h:269 | `&proc[NR_TASKS + (n)]`，按进程号取 proc 指针 |
| `isokprocn(n)` | proc.h:272 | 检查进程号是否合法 |
| `isemptyp(p)` | proc.h:274 | `p->p_rts_flags == RTS_SLOT_FREE` |
| `iskernelp(p)` | proc.h:275 | `p < BEG_USER_ADDR`，判断是否为内核任务 |
| `priv(rp)` | priv.h:82 | `rp->p_priv`，取特权结构指针 |
| `may_send_to(rp, nr)` | priv.h:86 | 检查 IPC 发送权限 |

### 2.4 调用关系/调用点分析

#### 2.4.1 proc_init() 调用链

```
内核启动
  └─ cstart() / main()
       └─ proc_init()
            ├─ arch_proc_reset(rp)    // 每个槽位的架构相关初始化
            └─ set_idle_name()        // IDLE 进程命名
```

#### 2.4.2 RTS_SET / RTS_UNSET 调用场景

**RTS_SET 置位场景**（使进程不可运行）：

| 场景 | 置位标志 | 调用者 |
|------|---------|--------|
| 进程发送消息阻塞 | `RTS_SENDING` | `mini_send()` |
| 进程接收消息阻塞 | `RTS_RECEIVING` | `mini_receive()` |
| 页缺失 | `RTS_PAGEFAULT` | 缺页处理 |
| 时间片耗尽 | `RTS_NO_QUANTUM` | `proc_no_time()` |
| 被抢占 | `RTS_PREEMPTED` | 调度器 |
| VM 请求 | `RTS_VMREQUEST` | `vm_suspend()` |
| 进程停止 | `RTS_PROC_STOP` | `sys_stop()` |
| 启动等待 | `RTS_BOOTINHIBIT` / `RTS_VMINHIBIT` | 启动初始化 |

**RTS_UNSET 清位场景**（使进程恢复可运行）：

| 场景 | 清位标志 | 调用者 |
|------|---------|--------|
| 消息发送成功 | `RTS_SENDING` | `mini_send()` / `mini_notify()` |
| 消息接收成功 | `RTS_RECEIVING` | `mini_receive()` |
| 页缺失修复 | `RTS_PAGEFAULT` | VM 回复 |
| 新时间片分配 | `RTS_NO_QUANTUM` | 调度器 |
| VM 请求完成 | `RTS_VMREQUEST` | VM 回复 |
| 进程启动就绪 | `RTS_BOOTINHIBIT` / `RTS_VMINHIBIT` | VM 通知 |

#### 2.4.3 struct proc 字段访问热点

| 字段 | 主要访问者 | 频率 |
|------|-----------|------|
| `p_rts_flags` | 调度器、IPC、VM 请求 | 极高（每次调度/IPC） |
| `p_reg` | 上下文切换（`switch_to_user()`） | 极高（每次进程切换） |
| `p_nextready` | `enqueue()` / `dequeue()` / `pick_proc()` | 高（调度操作） |
| `p_priority` | `enqueue()` / `pick_proc()` | 高（调度决策） |
| `p_sendmsg` / `p_delivermsg` | `mini_send()` / `mini_receive()` / `delivermsg()` | 高（IPC 操作） |
| `p_caller_q` / `p_q_link` | `mini_send()` / `mini_receive()` | 高（IPC 队列操作） |
| `p_endpoint` | `isokendpt_f()` / `endpoint_lookup()` | 中（进程查找） |
| `p_vmrequest` | `vm_suspend()` / VM 回复处理 | 低（页缺失路径） |

### 2.5 设计要点/特殊处理

#### 2.5.1 位图式状态管理而非枚举式

Minix3 选择用 `p_rts_flags` 位图而非枚举值表示进程状态，核心优势是**支持多原因叠加**。一个进程可以同时"正在发送"且"页缺失"（`RTS_SENDING | RTS_PAGEFAULT`），只有两个原因都消除后才恢复可运行。如果用枚举，则需要复杂的状态机来处理组合情况。

代价是：某些状态组合在逻辑上不可能（如 `RTS_SLOT_FREE | RTS_SENDING`），但位图本身不做互斥检查，依赖内核代码的正确性。

#### 2.5.2 特权结构分离（struct priv）

系统进程和用户进程的特权信息被分离到独立的 `struct priv` 中，而非嵌入 `struct proc`。原因：

1. **空间效率**：`NR_SYS_PROCS(64)` 远小于 `NR_PROCS(256)`，特权结构包含大量字段（IPC 掩码、I/O 范围、IRQ 表等），用户进程不需要这些信息
2. **安全性**：特权信息与基本进程信息分离，减少内核代码意外修改特权字段的风险
3. **动态分配**：系统进程重启时可重新分配特权结构，用户进程始终使用共享的默认结构

#### 2.5.3 volatile 修饰 p_rts_flags 和 p_misc_flags

两个字段声明为 `volatile u32_t`，因为它们在中断处理程序和主内核代码中都被访问。SMP 配置下，一个 CPU 可能正在修改标志位，另一个 CPU 正在读取。`volatile` 确保编译器不会将读操作优化掉，但**不保证原子性**——SMP 安全性依赖自旋锁。

#### 2.5.4 p_vmrequest 子结构——VM 协作机制

当内核检测到进程访问的内存不在物理内存中（页缺失），需要请求 VM 进程处理。但 VM 本身也是进程，可能也在进程表中。`p_vmrequest` 子结构保存了挂起操作的完整上下文：

- `type`：挂起操作类型（内核调用/消息投递/内存映射）
- `saved.reqmsg`：原始请求消息的副本
- `req_type` / `target` / `params`：VM 请求参数
- `vmresult`：VM 处理结果

这使得内核在 VM 处理完页缺失后能恢复原始操作的执行，实现了内核与 VM 的协作式页缺失处理。

#### 2.5.5 P_BLOCKEDON 宏的优先级规则

```c
#define P_BLOCKEDON(p)
    (((p)->p_rts_flags & RTS_SENDING) ?
     (p)->p_sendto_e :
     (((p)->p_rts_flags & RTS_RECEIVING) ?
      (p)->p_getfrom_e : NONE))
```

当进程同时置位 `RTS_SENDING` 和 `RTS_RECEIVING`（`ipc_sendrec()` 阻塞在发送阶段时），优先返回 `p_sendto_e`。这是因为 `p_getfrom_e` 在此场景下可能包含无意义的值——进程尚未进入接收阶段，`p_getfrom_e` 未被正确设置。

#### 2.5.6 SMP 扩展字段

`#ifdef CONFIG_SMP` 条件编译下，`struct proc` 增加两个位图字段：

- `p_cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]`：进程允许运行的 CPU 集合
- `p_stale_tlb[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]`：哪些 CPU 上有此进程的过期 TLB 条目，需在下次调度时刷新

这两个字段是 SMP 特有的 CPU 亲和性和 TLB 一致性管理机制。
