# 21-unported-symbols: 未迁移 C 函数清点

> **分类**: Kernel 多核与补全
> **源码**: 覆盖 kernel/ 中其余未覆盖的小文件和调试函数
> **说明**: 类似 02-stage-vm 的 §10——列出现有 C 源码中不需要迁移到 Rust 的函数并标注 ARCH 理由

---

## 1. 概述

### 1.1 概念定义/作用

本文档对 Minix3 内核中**尚未被 06~20 号文档覆盖**的 C 函数进行系统性清点，并按 ARCH（架构相关）理由分类标注哪些函数**不需要迁移到 Rust**。这些函数的共同特征是：它们要么完全依赖 x86-32 硬件语义、要么仅在特定架构的启动/异常路径中使用、要么属于调试/诊断基础设施，在 minix-rs 的 64 位架构下将被全新的 Rust 实现替代。

"未迁移"不等于"被丢弃"——每个未迁移函数的语义需求必须在 minix-rs 中有对应的 Rust 实现，只是不再逐行翻译 C 代码。

### 1.2 与 Minix3 的对应关系

Minix3 内核源码按目录组织为三层：

| 层 | 路径 | 说明 |
|---|------|------|
| 顶层 | `kernel/*.c` | 架构无关的核心逻辑 |
| 系统调用 | `kernel/system/*.c` | `call_vec[]` 注册的内核调用处理函数 |
| 架构层 | `kernel/arch/i386/*.c` | x86-32 特定的硬件操作 |

06~20 号文档已覆盖的函数属于"核心逻辑"范畴（进程结构、调度、IPC、端点、特权、系统调用分发、内存操作、fork/exec/exit/signal、时钟、主初始化、SMP、调试串口、ACPI/看门狗）。本文档覆盖的是**剩余部分**：未在上述文档中分析的 system/ 处理函数、interrupt.c、utility.c、profile.c、usermapped_data.c、table.c，以及 arch/i386/ 下除 acpi/apic/arch_smp/arch_watchdog 之外的文件。

### 1.3 关键状态/机制说明

未迁移函数按 ARCH 理由分为以下类别：

| ARCH 理由 | 缩写 | 含义 |
|-----------|------|------|
| x86-32 硬件指令 | HW | 直接使用 `inb/outb/inl/outl` 等 x86 I/O 端口指令或 x86 特定寄存器操作 |
| x86 分段/分页 | SEG | 操作 GDT/IDT/TSS/LDT 等 x86 保护模式数据结构 |
| x86 异常帧 | EXC | 依赖 `struct exception_frame` 等 x86 特定的栈帧布局 |
| 启动阶段专用 | BOOT | 仅在 `pre_init` / `cstart` 等分页启用前的极早期启动中使用 |
| 条件编译守卫 | COND | 被 `#if USE_xxx` / `#if SPROFILE` 等条件编译包裹，非核心路径 |
| 调试/诊断 | DBG | 仅用于内核调试输出，生产内核可完全禁用 |
| 语义替代 | ALT | 功能需要在 minix-rs 中存在，但 64 位架构下实现方式完全不同 |

### 1.4 行为规则

1. **每个未迁移函数必须标注 ARCH 理由**——没有"无理由丢弃"
2. **语义保留原则**：即使 C 函数不迁移，其功能必须在 minix-rs 中有对应实现（可能是全新设计）
3. **已覆盖函数不重复**：06~20 号文档已详细分析的函数不在本文档范围内
4. **arch/earm/ 不迁移**：ARM 架构文件整体不迁移，minix-rs 仅面向 x86-64

---

## 2. C 源码分析

### 2.1 相关定义

#### 2.1.1 中断相关常量

| 常量 | 定义位置 | 值/说明 |
|------|---------|---------|
| `NR_IRQ_VECTORS` | `include/minix/com.h` | 系统支持的最大 IRQ 向量数 |
| `NR_IRQ_HOOKS` | `include/minix/com.h` | 全局 IRQ hook 槽位数 |
| `IRQ_REENABLE` | `include/minix/com.h` | hook policy 标志：中断处理后自动重新使能 |
| `DIAG_BUFSIZE` | `kernel/system/do_diagctl.c` | 诊断缓冲区大小 |

#### 2.1.2 设备 I/O 相关常量

| 常量 | 定义位置 | 说明 |
|------|---------|------|
| `_DIO_TYPEMASK` | `include/minix/devio.h` | I/O 类型掩码（byte/word/long） |
| `_DIO_DIRMASK` | `include/minix/devio.h` | I/O 方向掩码（input/output） |
| `_DIO_BYTE/_DIO_WORD/_DIO_LONG` | `include/minix/devio.h` | I/O 宽度类型 |
| `_DIO_SAFE` | `include/minix/devio.h` | 安全设备 I/O 标志（使用 grant） |
| `VDEVIO_BUF_SIZE` | `kernel/system/do_vdevio.c` | 批量设备 I/O 缓冲区大小 |

#### 2.1.3 特权控制请求码

| 请求码 | 说明 |
|--------|------|
| `SYS_PRIV_ALLOW` | 允许进程运行（清除 RTS_NO_PRIV） |
| `SYS_PRIV_YIELD` | 允许目标运行，挂起调用者 |
| `SYS_PRIV_DISALLOW` | 禁止进程运行（设置 RTS_NO_PRIV） |
| `SYS_PRIV_CLEAR_IPC_REFS` | 清除进程的 IPC 引用 |
| `SYS_PRIV_SET_SYS` | 设置系统进程的特权结构 |
| `SYS_PRIV_SET_USER` | 设置用户进程的特权结构 |
| `SYS_PRIV_ADD_IO` | 添加 I/O 端口范围 |
| `SYS_PRIV_ADD_MEM` | 添加内存范围 |
| `SYS_PRIV_ADD_IRQ` | 添加 IRQ |
| `SYS_PRIV_QUERY_MEM` | 查询内存范围权限 |
| `SYS_PRIV_UPDATE_SYS` | 更新系统进程特权结构 |

#### 2.1.4 状态控制请求码

| 请求码 | 说明 |
|--------|------|
| `SYS_STATE_CLEAR_IPC_REFS` | 清除 IPC 引用 |
| `SYS_STATE_SET_STATE_TABLE` | 设置状态表 |
| `SYS_STATE_ADD_IPC_BL_FILTER` | 添加 IPC 黑名单过滤器 |
| `SYS_STATE_ADD_IPC_WL_FILTER` | 添加 IPC 白名单过滤器 |
| `SYS_STATE_CLEAR_IPC_FILTERS` | 清除 IPC 过滤器 |

#### 2.1.5 其他常量

| 常量 | 定义位置 | 说明 |
|------|---------|------|
| `ARE_PANICING` | `kernel/utility.c:17` | `0xDEADC0FF`，panic 重入检测标记 |
| `SAMPLE_BUFFER_SIZE` | `include/minix/config.h` | 统计性能分析采样缓冲区大小 |
| `MAPVEC_NR` | `kernel/system/do_vumap.c` | vumap 向量最大元素数 |

### 2.2 核心数据结构

#### 2.2.1 irq_hook_t（中断钩子）

定义于 `include/minix/com.h`，用于将硬件中断映射到进程通知：

```c
struct irq_hook {
  irq_hook_t *next;        /* 钩子链的下一个节点 */
  int irq;                  /* IRQ 向量号 */
  int id;                   /* 位标识（1/2/4/8...），用于 irq_actids */
  int notify_id;            /* 通知消息中的位偏移 */
  endpoint_t proc_nr_e;     /* 被通知的进程端点 */
  int policy;               /* IRQ_REENABLE 等 */
  irq_handler_t handler;    /* 处理函数指针 */
};
```

关键设计：同一 IRQ 线上可挂多个 hook（共享中断），通过 `id` 位图追踪哪些 handler 仍在处理中。`irq_actids[irq]` 的位图在所有 handler 完成后清零，此时才重新 unmask 该 IRQ。

#### 2.2.2 kmessages（内核消息环形缓冲区）

定义于 `kernel/glo.h`，声明于 `kernel/usermapped_data.c`，通过 `__section(".usermapped")` 放入用户可映射段：

```c
struct kmessages {
  int km_size;              /* 缓冲区有效字节数 */
  int km_next;              /* 环形缓冲区写入位置 */
  char km_buf[_KMESS_BUF_SIZE]; /* 环形缓冲区 */
  int blpos;                /* 线性缓冲区写入位置 */
  char kmess_buf[_KMESS_BUF_SIZE]; /* 线性缓冲区 */
};
```

双缓冲设计：`km_buf` 是环形缓冲区（永不丢失旧数据的开头），`kmess_buf` 是线性缓冲区（保留最近 N 字节）。`kputc()` 同时写入两个缓冲区。

#### 2.2.3 boot_image（启动映像表）

定义于 `kernel/table.c`，静态声明了内核启动时预加载的所有进程：

```c
struct boot_image image[NR_BOOT_PROCS] = {
  {ASYNCM,        "asyncm"},   /* 异步消息伪进程 */
  {IDLE,          "idle"},     /* 空闲任务 */
  {CLOCK,         "clock"},    /* 时钟任务 */
  {SYSTEM,        "system"},   /* 系统任务 */
  {HARDWARE,      "kernel"},   /* 硬件中断伪进程 */
  {DS_PROC_NR,    "ds"},       /* 数据存储服务器 */
  {RS_PROC_NR,    "rs"},       /* 重启服务器 */
  {PM_PROC_NR,    "pm"},       /* 进程管理器 */
  {SCHED_PROC_NR, "sched"},    /* 调度服务器 */
  {VFS_PROC_NR,   "vfs"},      /* 虚拟文件系统 */
  {MEM_PROC_NR,   "memory"},   /* 内存驱动 */
  {TTY_PROC_NR,   "tty"},      /* 终端驱动 */
  {MIB_PROC_NR,   "mib"},      /* MIB 服务器 */
  {VM_PROC_NR,    "vm"},       /* 虚拟内存管理器 */
  {PFS_PROC_NR,   "pfs"},      /* 伪文件系统 */
  {MFS_PROC_NR,   "mfs"},      /* Minix 文件系统 */
  {INIT_PROC_NR,  "init"},     /* 初始化进程 */
};
```

顺序关键：内核任务（ASYNCM~HARDWARE）必须在前，系统服务器（DS~PFS）在后，INIT 最后。此顺序影响 NOTIFY 消息的优先级投递。

#### 2.2.4 minix_kerninfo（内核信息导出结构）

定义于 `kernel/usermapped_data.c`，通过 `.usermapped` 段将内核数据结构导出给用户空间服务器：

```c
struct minix_kerninfo minix_kerninfo __section(".usermapped");
struct kinfo kinfo __section(".usermapped");
struct machine machine __section(".usermapped");
struct kmessages kmessages __section(".usermapped");
struct loadinfo loadinfo __section(".usermapped");
struct kuserinfo kuserinfo __section(".usermapped");
struct arm_frclock arm_frclock __section(".usermapped");
struct kclockinfo kclockinfo __section(".usermapped");
```

`.usermapped` 段在分页初始化时被映射到用户空间固定地址，使 VM/PM 等服务器可直接读取内核数据而无需系统调用。

#### 2.2.5 ex_s（x86 异常描述表）

定义于 `kernel/arch/i386/exception.c`，将 x86 异常向量映射为 POSIX 信号：

```c
struct ex_s {
  char *msg;           /* 异常描述文本 */
  int signum;          /* 对应的 POSIX 信号 */
  int minprocessor;    /* 最低处理器级别（86/186/286/386） */
};
```

共 20 项，从除零错误（SIGFPE）到 SIMD 异常（SIGFPE）。Page fault 映射为 SIGSEGV，General Protection 映射为 SIGSEGV。

### 2.3 关键函数分析

#### 2.3.1 中断管理（interrupt.c）

**put_irq_handler**（`kernel/interrupt.c:29`）：注册中断处理钩子。遍历 `irq_handlers[irq]` 链表收集已用 id 位图，分配最低可用 id（位扫描），将 hook 插入链表尾部，然后 unmask IRQ 线。

**rm_irq_handler**（`kernel/interrupt.c:75`）：注销中断处理钩子。从链表中移除匹配 id 的节点，清除 `irq_actids` 中的对应位。若链表为空则 mask IRQ 线；若链表非空但无活跃 handler 则 unmask。

**irq_handle**（`kernel/interrupt.c:116`）：硬件中断入口。先 mask IRQ 防止重入，遍历调用所有 hook 的 handler，每个 handler 返回非零则清除其 actid 位。所有 handler 完成后 unmask IRQ。对空链表（spurious interrupt）做计数报告。

**enable_irq / disable_irq**（`kernel/interrupt.c:161/169`）：操作 `irq_actids` 位图来使能/禁用特定 hook。disable 返回 TRUE 表示之前是启用的。

#### 2.3.2 工具函数（utility.c）

**panic**（`kernel/utility.c:22`）：内核致命错误处理。用 `ARE_PANICING`（`0xDEADC0FF`）检测重入（二次 panic 直接 `reset()`），打印格式化消息和栈追踪，调用 `minix_shutdown(0)`。

**kputc**（`kernel/utility.c:55`）：内核字符输出。同时写入 `km_buf`（环形）和 `kmess_buf`（线性）两个缓冲区。遇到 `END_OF_KMESS` 时向诊断进程发送 `SIGKMESS` 通知。串口调试模式下额外输出到串口。

**_exit**（`kernel/utility.c:88`）：内核内不应调用 `_exit`，调用即 panic。

#### 2.3.3 未覆盖的 system/ 内核调用处理函数

##### do_irqctl（`kernel/system/do_irqctl.c:23`）— IRQ 控制

处理 `SYS_IRQCTL` 请求，支持三个子命令：
- `IRQ_SETPOLICY`：分配 `irq_hooks[]` 槽位，调用 `put_irq_handler` 注册 `generic_handler`，返回 hook_id+1 给调用者
- `IRQ_ENABLE` / `IRQ_DISABLE`：使能/禁用指定 hook
- `IRQ_RMPOLICY`：调用 `rm_irq_handler` 注销 hook

`generic_handler`（`kernel/interrupt.c:143`）是所有驱动 IRQ 的统一处理函数：收集随机性熵、在 `s_int_pending` 中置位、调用 `mini_notify` 通知目标进程，返回 `hook->policy & IRQ_REENABLE` 决定是否自动重新使能。

##### do_devio（`kernel/system/do_devio.c:19`）— 单次设备 I/O

处理 `SYS_DEVIO` 请求。先通过 `priv(caller)->s_io_tab` 检查 I/O 端口权限（`CHECK_IO_PORT` 标志），再根据 `_DIO_TYPEMASK` 和 `_DIO_DIRMASK` 执行 `inb/inw/inl/outb/outw/outl`。检查端口对齐。

##### do_vdevio（`kernel/system/do_vdevio.c:25`）— 批量设备 I/O

处理 `SYS_VDEVIO` 请求。从用户空间复制 (port,value) 对数组到内核静态缓冲区 `vdevio_buf`，逐个检查端口权限，批量执行 I/O 操作，输入操作完成后将结果复制回用户空间。

##### do_sdevio（`kernel/arch/i386/do_sdevio.c:24`）— 安全设备 I/O

处理 `SYS_SDEVIO` 请求，是 `do_devio` 的安全变体。支持 grant 地址映射（`_DIO_SAFE` 标志），通过 `verify_grant` 验证权限后切换到目标进程地址空间执行 `phys_insb/phys_insw/phys_outsb/phys_outsw`，完成后切回调用者地址空间。

##### do_getinfo（`kernel/system/do_getinfo.c:47`）— 内核信息查询

处理 `SYS_GETINFO` 请求，支持 16 种信息类型：`GET_MACHINE`、`GET_KINFO`、`GET_LOADINFO`、`GET_CPUINFO`、`GET_HZ`、`GET_IMAGE`、`GET_IRQHOOKS`、`GET_PROCTAB`、`GET_PRIVTAB`、`GET_PROC`、`GET_PRIV`、`GET_REGS`、`GET_WHOAMI`、`GET_MONPARAMS`、`GET_RANDOMNESS`、`GET_RANDOMNESS_BIN`、`GET_IRQACTIDS`、`GET_IDLETSC`、`GET_CPUTICKS`。通过 `data_copy_vmcheck` 将内核数据复制到调用者地址空间。`GET_PROCTAB` 前先调用 `update_idle_time` 汇总所有 CPU 的 idle 时间。

##### do_privctl（`kernel/system/do_privctl.c:26`）— 特权控制

处理 `SYS_PRIVCTL` 请求，11 个子命令。核心逻辑：
- `SYS_PRIV_SET_SYS`：为目标进程分配 `priv` 结构（调用 `get_priv`），从调用者复制特权模板，重置 pending 状态，设置默认特权标志（`DSRV_F`/`DSRV_T`/`DSRV_M`/`DSRV_KC`/`DSRV_SM`），可选地从用户空间覆盖特权值（`update_priv`）
- `SYS_PRIV_SET_USER`：将用户进程链接到共享的 `USER_PRIV_ID` 特权结构
- `SYS_PRIV_ADD_IO/MEM/IRQ`：调用 `priv_add_io/priv_add_mem/priv_add_irq`
- `SYS_PRIV_QUERY_MEM`：检查物理内存范围是否在允许列表中
- `SYS_PRIV_UPDATE_SYS`：更新已有系统进程的特权结构

`update_priv`（`do_privctl.c:280`）：根据 `CHECK_IRQ/CHECK_IO_PORT/CHECK_MEM` 标志分别复制 IRQ 表、I/O 范围表、内存范围表，以及 trap_mask、ipc_to 位图、k_call_mask。

##### do_copy（`kernel/system/do_copy.c:22`）— 虚拟/物理复制

处理 `SYS_VIRCOPY` 和 `SYS_PHYSCOPY`。解析源/目标虚拟地址，验证 endpoint，检查溢出，调用 `virtual_copy_vmcheck` 执行复制。`CP_FLAG_TRY` 标志下仅 VFS 可调用，返回 EFAULT 而非触发 VM 请求。

##### do_trace（`kernel/system/do_trace.c:20`）— 进程追踪

处理 `SYS_TRACE` 请求，实现 ptrace 的内核部分。支持 `T_STOP`（设置 RTS_P_STOP）、`T_GETINS/T_GETDATA`（从进程地址空间读取）、`T_SETINS/T_SETDATA`（写入进程地址空间）、`T_GETUSER/T_SETUSER`（读写 proc 结构/stackframe）、`T_RESUME`（清除 RTS_P_STOP）、`T_STEP`（设置 MF_STEP）、`T_SYSCALL`（设置 MF_SC_TRACE）。`T_SETUSER` 对段寄存器有写保护。

##### do_runctl（`kernel/system/do_runctl.c:18`）— 运行控制

处理 `SYS_RUNCTL` 请求。`RC_STOP` 设置 `RTS_PROC_STOP`（SMP 下需跨 CPU 停止），`RC_RESUME` 清除 `RTS_PROC_STOP`。`RC_DELAY` 标志用于安全信号投递：若目标正在发送消息或被 syscall 追踪，设置 `MF_SIG_DELAY` 延迟停止。

##### do_schedctl（`kernel/system/do_schedctl.c:7`）— 调度器控制

处理 `SYS_SCHEDCTL` 请求。`SCHEDCTL_FLAG_KERNEL` 标志下内核接管调度（调用 `sched_proc` 并置 `p_scheduler = NULL`），否则调用者成为目标进程的调度器（`p_scheduler = caller`）。

##### do_schedule（`kernel/system/do_schedule.c:8`）— 调度参数设置

处理 `SYS_SCHEDULE` 请求。验证调用者是目标进程的调度器（`caller == p->p_scheduler`），提取 priority/quantum/cpu/niced 参数，调用 `sched_proc`。

##### do_update（`kernel/system/do_update.c:37`）— 进程槽位交换

处理 `SYS_UPDATE` 请求，用于 RS（重启服务器）执行 live update。交换两个进程的 proc 和 priv 槽位，继承源进程的 IRQ/I/O/内存权限到目标，调整异步消息表，可选地中止源进程的挂起 IPC 发送（`SYS_UPD_ROLLBACK`），交换 VM 请求链中的指针，SMP 下标记 stale TLB。

辅助函数：`inherit_priv_irq/io/mem` 逐项调用 `priv_add_irq/io/mem`；`abort_proc_ipc_send` 从发送队列中移除进程；`adjust_proc_slot` 保留 endpoint/nr/priv/scheduler；`adjust_priv_slot` 保留 id/pending 状态；`swap_memreq` 交换 VM 请求链中的指针。

##### do_statectl（`kernel/system/do_statectl.c:15`）— 状态控制

处理 `SYS_STATECTL` 请求。5 个子命令：清除 IPC 引用、设置状态表、添加 IPC 黑/白名单过滤器、清除 IPC 过滤器。

##### do_abort（`kernel/system/do_abort.c:16`）— 系统中止

处理 `SYS_ABORT` 请求。直接调用 `prepare_shutdown(how)`。

##### do_diagctl（`kernel/system/do_diagctl.c:18`）— 诊断控制

处理 `SYS_DIAGCTL` 请求。4 个子命令：
- `DIAGCTL_CODE_DIAG`：从调用者复制诊断文本到内核缓冲区，逐字符调用 `kputc`，最后发送 `END_OF_KMESS`
- `DIAGCTL_CODE_STACKTRACE`：打印指定进程的栈追踪
- `DIAGCTL_CODE_REGISTER`：设置 `s_diag_sig = TRUE`，立即发送 `SIGKMESS`（若日志非空）
- `DIAGCTL_CODE_UNREGISTER`：设置 `s_diag_sig = FALSE`

##### do_setgrant（`kernel/system/do_setgrant.c:15`）— 设置 grant 表

处理 `SYS_SETGRANT` 请求。验证调用者有特权结构后，调用 `_K_SET_GRANT_TABLE` 设置 grant 表地址和大小。

##### do_stime（`kernel/system/do_stime.c:15`）— 设置启动时间

处理 `SYS_STIME` 请求。直接调用 `set_boottime()`。

##### do_settime（`kernel/system/do_settime.c:18`）— 设置系统时间

处理 `SYS_SETTIME` 请求。仅支持 `CLOCK_REALTIME`。`now==0` 时执行 adjtime 渐进调整（`set_adjtime_delta`），`now!=0` 时直接设置 realtime（计算 `boottime` 差值转换为 ticks）。

##### do_getmcontext / do_setmcontext（`kernel/system/do_mcontext.c:23/74`）— 机器上下文

处理 `SYS_GETMCONTEXT` 和 `SYS_SETMCONTEXT`。`do_getmcontext` 保存 FPU 状态到 `mcontext_t.__fpregs`（先 `save_fpu` 确保上下文已存入 proc 结构），设置 `_MC_FPU_SAVED` 标志。`do_setmcontext` 恢复 FPU 状态，强制 `release_fpu` 触发下次访问时重新加载。

##### do_safememset（`kernel/system/do_safememset.c:20`）— 安全内存填充

处理 `SYS_SAFEMEMSET` 请求。通过 `verify_grant` 验证写权限（`CPF_WRITE`），然后调用 `vm_memset` 执行物理内存填充。

##### do_umap_remote（`kernel/system/do_umap_remote.c:26`）— 远程地址映射

处理 `SYS_UMAP_REMOTE` 请求。对 grant 类型地址先通过 `verify_grant` 解析，再 `vm_lookup` 转换为物理地址。验证物理连续性（`vm_lookup_range`）。

##### do_vumap（`kernel/system/do_vumap.c:22`）— 向量地址映射

处理 `SYS_VUMAP` 请求。将虚拟地址/grant 向量映射为物理地址向量，供驱动 DMA 使用。分批处理（`MAPVEC_NR`），对每个 grant 调用 `verify_grant`，再 `vm_lookup_range` 拆分为物理段。遇到未映射页时若为写操作则调用 `vm_check_range` 触发 VM 分配。

##### do_sprofile（`kernel/system/do_sprofile.c:36`）— 统计性能分析

处理 `SYS_SPROF` 请求（条件编译 `SPROFILE`）。`PROF_START` 启动采样（RTC 或 NMI 中断源），`PROF_STOP` 停止采样并复制结果到用户空间。采样数据包含进程端点、PC 值和分类计数。

##### do_iopenable（`kernel/arch/i386/do_iopenable.c:19`）— I/O 权限开启

处理 `SYS_IOPENABLE` 请求。调用 `enable_iop` 在目标进程的 I/O 位图中开启所有端口权限（i386 TSS IOPB）。

##### do_readbios（`kernel/arch/i386/do_readbios.c:15`）— BIOS 区域读取

处理 `SYS_READBIOS` 请求。验证读取范围在 BIOS 内存区域（`BIOS_MEM_BEGIN~BIOS_MEM_END` 或 `BASE_MEM_TOP~UPPER_MEM_END`）内，然后 `virtual_copy_vmcheck` 复制数据。

##### do_sdevio（`kernel/arch/i386/do_sdevio.c:24`）— 安全设备 I/O

见上文分析。x86-32 特定，使用 `phys_insb/phys_insw/phys_outsb/phys_outsw` 和地址空间切换。

#### 2.3.4 统计性能分析（profile.c）

**init_profile_clock**（`kernel/profile.c:27`）：调用 `arch_init_profile_clock` 注册 CMOS 定时器中断，通过 `put_irq_handler` 注册 `profile_clock_handler`。

**stop_profile_clock**（`kernel/profile.c:42`）：调用 `arch_stop_profile_clock`，`disable_irq` + `rm_irq_handler` 注销。

**profile_clock_handler**（`kernel/profile.c:115`）：每次定时器中断调用 `profile_sample` 采样当前进程的 PC 值。

**nmi_sprofile_handler**（`kernel/profile.c:128`）：NMI 中断的采样入口。若中断发生在内核态且当前非 IDLE，采样 KERNEL 进程的 PC；否则采样当前进程。

#### 2.3.5 架构特定函数（arch/i386/）

##### 保护模式与分段（protect.c）

**prot_init**（`kernel/arch/i386/protect.c:321`）：初始化 GDT/IDT，设置内核代码/数据段描述符，为每个 CPU 创建 TSS，初始化 IDT 中的异常/中断门。这是 x86-32 保护模式的核心初始化。

**tss_init**（`kernel/arch/i386/protect.c:154`）：为指定 CPU 创建 TSS 描述符，设置内核栈指针、IOPB 偏移。

**enable_iop**（`kernel/arch/i386/protect.c:44`）：在进程的 I/O 位图（IOPB）中清除所有位，允许进程访问全部 65536 个 I/O 端口。

**idt_init/idt_reload**（`kernel/arch/i386/protect.c:260/268`）：初始化 IDT 并加载 IDTR。

**arch_boot_proc**（`kernel/arch/i386/protect.c:388`）：为启动映像中的进程设置初始寄存器和段描述符。

**arch_post_init**（`kernel/arch/i386/protect.c:370`）：保护模式初始化后阶段，设置 USER 代码/数据段描述符。

##### 内存与分页（memory.c, pg_utils.c）

**umap_virtual**（`kernel/arch/i386/memory.c:282`）：将进程虚拟地址转换为物理地址。先检查 mapcache，未命中则 `vm_lookup`，成功后缓存结果。

**vm_lookup**（`kernel/arch/i386/memory.c:325`）：x86-32 两级页表查找。遍历页目录和页表，返回物理地址和页表项标志。

**vm_lookup_range**（`kernel/arch/i386/memory.c:377`）：查找虚拟地址范围的物理连续长度，用于验证 safecopy 的连续性。

**virtual_copy_f**（`kernel/arch/i386/memory.c:592`）：核心虚拟内存复制函数。对源和目标分别 `umap_virtual` 获取物理地址，然后 `phys_copy` 执行复制。支持 VM 请求挂起（VMSUSPEND）。

**arch_proc_init**（`kernel/arch/i386/memory.c:722`）：设置进程的初始栈指针、指令指针和页目录。

**arch_enable_paging**（`kernel/arch/i386/memory.c:940`）：为进程启用分页，设置 CR3。

**pg_identity/pg_mapkernel/vm_enable_paging**（`kernel/arch/i386/pg_utils.c`）：启动阶段页表构建——恒等映射、内核映射、启用分页。

##### 异常处理（exception.c）

**exception_handler**（`kernel/arch/i386/exception.c:180`）：x86 异常的统一入口。内核态异常调用 `inkernel_disaster`（panic），用户态异常根据 `ex_data[]` 表转换为 POSIX 信号（`cause_sig` 或 `sig_add`）。Page fault 特殊处理：若 `catch_pagefaults` 启用则直接返回（用于 `arch_enable_paging`）。

**proc_stacktrace**（`kernel/arch/i386/exception.c:333`）：遍历进程栈帧链（EBP 链），打印栈追踪。

##### 系统与 FPU（arch_system.c）

**fpu_init**（`kernel/arch/i386/arch_system.c:51`）：初始化 FPU，设置 CR0 中的 MP/EM/TS 位。

**save_fpu / save_local_fpu**（`kernel/arch/i386/arch_system.c:88/111`）：将 FPU 状态保存到 proc 结构的 `p_seg.fpu_state`。`save_local_fpu` 的 `retain` 参数控制是否保留 FPU 状态（0=释放 FPU 所有权）。

**restore_fpu**（`kernel/arch/i386/arch_system.c:189`）：恢复进程的 FPU 状态，清除 CR0.TS。

**arch_proc_reset**（`kernel/arch/i386/arch_system.c:146`）：重置进程的架构特定状态（清除 FPU 初始化标志、重置调试寄存器）。

**arch_do_syscall**（`kernel/arch/i386/arch_system.c:485`）：从用户态系统调用入口，保存用户栈指针，调用 `do_ipc` 或 `kernel_call`。

**restore_user_context**（`kernel/arch/i386/arch_system.c:566`）：恢复用户态上下文，从 `p_reg` 加载寄存器，iret 返回用户态。

**arch_proc_setcontext**（`kernel/arch/i386/arch_system.c:523`）：设置进程的寄存器上下文，用于 sigreturn。

**cpu_identify**（`kernel/arch/i386/arch_system.c:212`）：通过 CPUID 指令识别处理器特性（FPU、SSE、APIC 等）。

##### 时钟（arch_clock.c）

**init_8253A_timer / stop_8253A_timer**（`kernel/arch/i386/arch_clock.c:48/64`）：BSP 使用 8253A PIT 作为系统时钟。

**init_local_timer / stop_local_timer / restart_local_timer**（`kernel/arch/i386/arch_clock.c:131/155/168`）：AP 使用 Local APIC 定时器。

**cycles_accounting_init**（`kernel/arch/i386/arch_clock.c:196`）：初始化 TSC 周期计数，用于精确的 CPU 时间统计。

**context_stop**（`kernel/arch/i386/arch_clock.c:208`）：记录进程使用的 CPU 周期数，更新 `p_cycles` 和 `p_cpu_time_left`。

**cpu_load**（`kernel/arch/i386/arch_clock.c:381`）：计算当前 CPU 负载（1~1000），基于 idle 进程的周期占比。

##### 启动（pre_init.c）

**pre_init**（`kernel/arch/i386/pre_init.c:217`）：x86 极早期初始化入口（分页启用前）。解析 multiboot 信息，获取内存映射和启动参数。此函数在 `cstart` 中被调用。

**get_parameters**（`kernel/arch/i386/pre_init.c:94`）：从 multiboot 命令行解析内核参数。

##### 其他架构文件

**i8259.c**：8259A PIC 中断控制器驱动（`intr_init/irq_8259_unmask/irq_8259_mask/i8259_disable/irq_8259_eoi`）。x86-32 专用，x86-64 使用 APIC 替代。

**breakpoints.c**（`kernel/arch/i386/breakpoints.c:6`）：`breakpoint_set` 设置 x86 调试寄存器（DR0~DR3 + DR7）。

**oxpcie.c**（`kernel/arch/i386/oxpcie.c`）：Oxford PCIe 串口卡驱动（`oxpcie_set_vaddr/oxpcie_putc/oxpcie_in`）。

**direct_tty_utils.c**：VGA 文本模式直接输出（`direct_cls/direct_print/direct_print_char/direct_read_char`），用于分页启用前的早期控制台输出。

**arch_reset.c**：系统重置（键盘控制器脉冲、ACPI S5 关机、EFI 重置）。

**arch_do_vmctl.c**（`kernel/arch/i386/arch_do_vmctl.c:38`）：`arch_do_vmctl` 处理 VMCTL 的架构特定子命令（启用分页、设置进程栈指针等）。

**usermapped_data_arch.c / usermapped_glo_ipc.S**：架构特定的 usermapped 段数据。

### 2.4 调用关系/调用点分析

#### 2.4.1 中断处理调用链

```
硬件中断 → hwintXX (mpx.S) → irq_handle (interrupt.c)
  → hook->handler (generic_handler in do_irqctl.c)
    → get_randomness + s_int_pending 置位 + mini_notify
  → hw_intr_unmask (若所有 handler 完成)
```

#### 2.4.2 内核调用分发链（未覆盖的调用号）

```
用户态 SYS_IRQCTL → kernel_call → call_vec[SYS_IRQCTL] → do_irqctl
用户态 SYS_DEVIO  → kernel_call → call_vec[SYS_DEVIO]  → do_devio
用户态 SYS_VDEVIO → kernel_call → call_vec[SYS_VDEVIO] → do_vdevio
用户态 SYS_GETINFO → kernel_call → call_vec[SYS_GETINFO] → do_getinfo
用户态 SYS_PRIVCTL → kernel_call → call_vec[SYS_PRIVCTL] → do_privctl
用户态 SYS_COPY   → kernel_call → call_vec[SYS_VIRCOPY/SYS_PHYSCOPY] → do_copy
用户态 SYS_TRACE  → kernel_call → call_vec[SYS_TRACE]  → do_trace
用户态 SYS_RUNCTL → kernel_call → call_vec[SYS_RUNCTL] → do_runctl
用户态 SYS_SCHEDCTL → kernel_call → call_vec[SYS_SCHEDCTL] → do_schedctl
用户态 SYS_SCHEDULE → kernel_call → call_vec[SYS_SCHEDULE] → do_schedule
用户态 SYS_UPDATE → kernel_call → call_vec[SYS_UPDATE] → do_update
用户态 SYS_STATECTL → kernel_call → call_vec[SYS_STATECTL] → do_statectl
用户态 SYS_ABORT  → kernel_call → call_vec[SYS_ABORT]  → do_abort
用户态 SYS_DIAGCTL → kernel_call → call_vec[SYS_DIAGCTL] → do_diagctl
用户态 SYS_SETGRANT → kernel_call → call_vec[SYS_SETGRANT] → do_setgrant
用户态 SYS_STIME  → kernel_call → call_vec[SYS_STIME]  → do_stime
用户态 SYS_SETTIME → kernel_call → call_vec[SYS_SETTIME] → do_settime
用户态 SYS_GETMCONTEXT → kernel_call → call_vec[...] → do_getmcontext
用户态 SYS_SETMCONTEXT → kernel_call → call_vec[...] → do_setmcontext
用户态 SYS_SAFEMEMSET → kernel_call → call_vec[...] → do_safememset
用户态 SYS_UMAP_REMOTE → kernel_call → call_vec[...] → do_umap_remote
用户态 SYS_VUMAP  → kernel_call → call_vec[...] → do_vumap
用户态 SYS_SPROF  → kernel_call → call_vec[...] → do_sprofile
用户态 SYS_IOPENABLE → kernel_call → call_vec[...] → do_iopenable
用户态 SYS_READBIOS → kernel_call → call_vec[...] → do_readbios
用户态 SYS_SDEVIO → kernel_call → call_vec[...] → do_sdevio
```

#### 2.4.3 panic 调用链

```
panic (utility.c)
  → printf + va_start/vprintf
  → util_stacktrace
  → minix_shutdown(0) → arch_shutdown → reset/ACPI poweroff
```

#### 2.4.4 do_update 调用链

```
RS → SYS_UPDATE → do_update
  → inherit_priv_irq → priv_add_irq (逐项)
  → inherit_priv_io → priv_add_io (逐项)
  → inherit_priv_mem → priv_add_mem (逐项)
  → adjust_asyn_table → data_copy (传输异步消息表)
  → abort_proc_ipc_send (若 ROLLBACK)
  → swap proc/priv slots
  → adjust_proc_slot (保留 endpoint/nr/priv/scheduler)
  → adjust_priv_slot (保留 id/pending)
  → swap_proc_slot_pointer (修正 cpulocal ptproc)
  → swap_memreq (修正 VM 请求链)
```

### 2.5 设计要点/特殊处理

#### 2.5.1 未迁移函数 ARCH 理由分类总表

| 函数 | 源文件 | ARCH 理由 | minix-rs 替代方案 |
|------|--------|----------|-----------------|
| put_irq_handler | interrupt.c | ALT | Rust 中断框架，trait 抽象 |
| rm_irq_handler | interrupt.c | ALT | 同上 |
| irq_handle | interrupt.c | ALT | 同上 |
| enable_irq | interrupt.c | ALT | 同上 |
| disable_irq | interrupt.c | ALT | 同上 |
| generic_handler | do_irqctl.c | HW | x86-64 APIC 中断框架 |
| panic | utility.c | ALT | Rust panic handler + log crate |
| kputc | utility.c | ALT | Rust 串口/帧缓冲输出 |
| _exit | utility.c | ALT | Rust panic（不可达路径） |
| do_irqctl | do_irqctl.c | ALT | Rust 中断管理模块 |
| do_devio | do_devio.c | HW | x86-64 I/O 端口 trait |
| do_vdevio | do_vdevio.c | HW | 同 do_devio |
| do_sdevio | do_sdevio.c | HW/SEG | x86-64 安全设备 I/O + grant |
| do_getinfo | do_getinfo.c | ALT | Rust 内核信息导出（usermapped） |
| do_privctl | do_privctl.c | ALT | Rust 特权管理模块 |
| do_copy | do_copy.c | ALT | Rust 安全复制框架 |
| do_trace | do_trace.c | SEG/EXC | x86-64 调试寄存器 + ptrace |
| do_runctl | do_runctl.c | ALT | Rust 进程状态管理 |
| do_schedctl | do_schedctl.c | ALT | Rust 调度器接口 |
| do_schedule | do_schedule.c | ALT | Rust 调度接口 |
| do_update | do_update.c | ALT | Rust 进程槽位交换（live update） |
| do_statectl | do_statectl.c | ALT | Rust 状态控制 + IPC 过滤器 |
| do_abort | do_abort.c | ALT | Rust shutdown 框架 |
| do_diagctl | do_diagctl.c | ALT | Rust 诊断/日志框架 |
| do_setgrant | do_setgrant.c | ALT | Rust grant 表管理 |
| do_stime | do_stime.c | ALT | Rust 时钟管理 |
| do_settime | do_settime.c | ALT | Rust 时钟管理 |
| do_getmcontext | do_mcontext.c | SEG/EXC | x86-64 mcontext 保存/恢复 |
| do_setmcontext | do_mcontext.c | SEG/EXC | 同上 |
| do_safememset | do_safememset.c | ALT | Rust 安全内存操作 |
| do_umap_remote | do_umap_remote.c | ALT | Rust 地址映射框架 |
| do_vumap | do_vumap.c | ALT | Rust 向量地址映射 |
| do_sprofile | do_sprofile.c | COND | 条件编译，Rust 性能分析模块 |
| do_iopenable | do_iopenable.c | SEG | x86-64 I/O 权限位图 |
| do_readbios | do_readbios.c | BOOT/SEG | x86-64 BIOS 区域映射 |
| init_profile_clock | profile.c | COND | 条件编译 |
| stop_profile_clock | profile.c | COND | 条件编译 |
| nmi_sprofile_handler | profile.c | COND/HW | NMI 性能分析 |
| prot_init | protect.c | SEG | x86-64 GDT/IDT/TSS 初始化 |
| tss_init | protect.c | SEG | x86-64 TSS 设置 |
| enable_iop | protect.c | SEG | x86-64 IOPB 操作 |
| idt_init | protect.c | SEG | x86-64 IDT 初始化 |
| arch_boot_proc | protect.c | SEG | x86-64 进程启动设置 |
| umap_virtual | memory.c | SEG | x86-64 四级页表查找 |
| vm_lookup | memory.c | SEG | x86-64 页表遍历 |
| vm_lookup_range | memory.c | SEG | 同上 |
| virtual_copy_f | memory.c | SEG/ALT | Rust 安全复制 + 页表查找 |
| arch_proc_init | memory.c | SEG | x86-64 进程初始化 |
| arch_enable_paging | memory.c | SEG | x86-64 CR3 加载 |
| pg_identity | pg_utils.c | BOOT/SEG | x86-64 启动页表构建 |
| pg_mapkernel | pg_utils.c | BOOT/SEG | 同上 |
| vm_enable_paging | pg_utils.c | BOOT/SEG | 同上 |
| exception_handler | exception.c | EXC | x86-64 异常入口 + 信号转换 |
| proc_stacktrace | exception.c | EXC/DBG | x86-64 栈回溯 |
| fpu_init | arch_system.c | HW | x86-64 FPU/SSE/AVX 初始化 |
| save_fpu | arch_system.c | HW | x86-64 FPU 状态保存（XSAVE） |
| restore_fpu | arch_system.c | HW | x86-64 FPU 状态恢复（XRSTOR） |
| arch_proc_reset | arch_system.c | SEG | x86-64 进程重置 |
| arch_do_syscall | arch_system.c | SEG/EXC | x86-64 syscall/sysret 入口 |
| restore_user_context | arch_system.c | SEG/EXC | x86-64 用户态恢复 |
| arch_proc_setcontext | arch_system.c | SEG/EXC | x86-64 上下文设置 |
| cpu_identify | arch_system.c | HW | x86-64 CPUID 特性检测 |
| init_8253A_timer | arch_clock.c | HW | x86-64 使用 Local APIC 替代 |
| init_local_timer | arch_clock.c | HW | x86-64 APIC 定时器初始化 |
| cycles_accounting_init | arch_clock.c | HW | x86-64 TSC/APERF 初始化 |
| context_stop | arch_clock.c | HW | x86-64 周期统计 |
| cpu_load | arch_clock.c | ALT | Rust CPU 负载计算 |
| pre_init | pre_init.c | BOOT | x86-64 multiboot2 解析 |
| get_parameters | pre_init.c | BOOT | x86-64 启动参数解析 |
| intr_init | i8259.c | HW | x86-64 使用 APIC 替代 PIC |
| breakpoint_set | breakpoints.c | HW | x86-64 调试寄存器操作 |
| oxpcie_set_vaddr/putc/in | oxpcie.c | HW | x86-64 PCIe 串口驱动 |
| direct_cls/print/print_char | direct_tty_utils.c | BOOT/HW | x86-64 早期帧缓冲输出 |
| arch_do_vmctl | arch_do_vmctl.c | SEG | x86-64 VM 控制架构部分 |
| arch_reset | arch_reset.c | HW | x86-64 系统重置 |

#### 2.5.2 关键设计要点

1. **中断共享机制**：Minix3 通过 `irq_hook_t` 链表实现中断共享，`id` 位图机制允许多个驱动共享同一 IRQ 线。`irq_actids` 追踪哪些 handler 仍在处理中，只有全部完成后才 unmask。minix-rs 需要重新设计此机制以适配 x86-64 的 MSI/MSI-X 中断模型。

2. **I/O 端口权限检查**：`do_devio` 和 `do_vdevio` 通过 `priv(caller)->s_io_tab` 检查端口权限，`do_sdevio` 额外支持 grant 地址映射。x86-64 下 I/O 端口操作语义不变，但需要新的安全抽象。

3. **do_update 的槽位交换**：这是 Minix3 live update 的核心机制——交换两个进程的 proc/priv 槽位，保留各自的 endpoint 和调度关系。涉及 6 个辅助函数处理权限继承、IPC 中止、异步消息表传输、VM 请求链修复。minix-rs 需要重新设计进程槽位管理以支持类似功能。

4. **usermapped 段机制**：`__section(".usermapped")` 将内核数据结构映射到用户空间固定地址，使 VM/PM 可直接读取而无需系统调用。x86-64 下需要重新设计此映射机制（KASLR 兼容、页表布局变化）。

5. **双缓冲 kmessages**：`kputc` 同时维护环形和线性两个缓冲区，前者保留完整历史，后者保留最近内容。`END_OF_KMESS` 触发通知。minix-rs 可用 `log` crate + 环形缓冲区替代。

6. **条件编译守卫**：`do_sprofile` 和 `profile.c` 中的函数被 `#if SPROFILE` 包裹，属于可选的统计性能分析功能。minix-rs 中可作为可选 feature 实现。

7. **x86-32 分页硬编码**：`vm_lookup` 硬编码了两级页表查找（10+10+12 地址划分），`pg_utils.c` 硬编码了 32 位页表构建。x86-64 需要四级页表（9+9+9+9+12），这些函数必须完全重写。

8. **FPU 状态管理**：Minix3 使用 `fxsave/fxrstor` 保存/恢复 512 字节 FPU 状态。x86-64 支持 `xsave/xrstor` 扩展状态（AVX、MPX 等），状态区大小可变。minix-rs 必须使用 `xsave` 系列指令。
