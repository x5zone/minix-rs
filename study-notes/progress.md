# MINIX3 源代码阅读进度

> **核心理念**：理解操作系统架构（OS architecture understanding），而非系统启动工程（OS bring-up engineering）。
> 
> **学习原则**：机制优先，硬件分离。抽象 OS 需要的机制，而不是描述硬件细节。

## 📊 整体阅读进度

**总进度: 61/178 文件 (34.3%)**

> **注意**: 所有路径均相对于 `minix3/minix/` 目录

---

## 📁 kernel 目录覆盖率

| 子目录 | 已读 | 总数 | 覆盖率 |
|--------|------|------|--------|
| kernel/*.c | 5 | 14 | 35.7% |
| kernel/*.h | 13 | 21 | 61.9% |
| kernel/system/*.c | 11 | 38 | 28.9% |
| kernel/arch/i386/*.c | 0 | 14 | 0% |
| kernel/arch/i386/*.S | 1 | 12 | 8.3% |
| kernel/arch/i386/*.h | 0 | 9 | 0% |

### kernel 目录已读文件列表

| 文件 | 状态 | 核心内容 | 讲解笔记 |
|------|------|----------|----------|
| `kernel/proc.c` | ✅ 已读 | IPC 核心: mini_send/mini_receive/mini_notify, 调度, FPU 管理 | [📖](../tmp/ipc/tmp_proc_c.md) |
| `kernel/proc.h` | ✅ 已读 | 进程控制块结构体 (p_reg, p_priv, p_sched, p_rts_flags) | [📖](../tmp/ipc/tmp_proc.h.md) |
| `kernel/system.c` | ✅ 已读 | 系统调用分发中心、信号管理、进程清理、IPC 过滤器管理 | [📖](../tmp/ipc/tmp_system_c.md) |
| `kernel/system.h` | ✅ 已读 | 系统调用框架头文件、sys_call_t 函数指针、信号处理完整流程 | [📖](../tmp/ipc/tmp_system.h.md) |
| `kernel/clock.c` | ✅ 已读 | 时钟中断处理、时间记账（BILLABLE）、虚拟定时器、负载统计 | [📖](../tmp/ipc/tmp_clock.c.md) |
| `kernel/clock.h` | ✅ 已读 | 时钟子系统头文件、ms_2_cpu_time/cycles 计费、TSC 稳定性假设、现代 OS 对比 | [📖](../tmp/ipc/tmp_clock.h.md) |
| `kernel/interrupt.c` | ✅ 已读 | 中断处理程序管理、共享中断（位图 ID）、虚假中断处理 | [📖](../tmp/ipc/tmp_interrupt.c.md) |
| `kernel/interrupt.h` | ✅ 已读 | 中断子系统头文件、hw_intr.h 委托模式、Rust trait 抽象设计 | [📖](../tmp/ipc/tmp_interrupt.h.md) |
| `kernel/ipc.h` | ✅ 已读 | IPC 常量和宏定义 (WILLRECEIVE/CANRECEIVE/FROM_KERNEL/MF_REPLY_PEND) | [📖](../tmp/ipc/tmp_ipc.h.md) |
| `kernel/ipc_filter.h` | ✅ 已读 | IPC 消息过滤器（黑名单/白名单） | [📖](../tmp/ipc/tmp_ipc_filter.h.md) |
| `kernel/priv.h` | ✅ 已读 | 进程特权结构体、能力隔离、IPC 权限模型、现代 64 位硬件改进 | [📖](../tmp/ipc/tmp_priv.h.md) |
| `kernel/cpulocals.h` | ✅ 已读 | CPU 本地变量结构体、SMP/单核宏抽象、Per-CPU 运行队列、FPU 惰性切换 | [📖](../tmp/ipc/tmp_cpulocals.h.md) |
| `kernel/cpulocals.c` | ✅ 已读 | CPU 本地变量数组定义（3 行代码） | [📖](../tmp/ipc/tmp_cpulocals.c.md) |
| `kernel/config.h` | ✅ 已读 | 系统调用开关、资源限制配置、Feature flags 重构 | [📖](../tmp/kernel_init/tmp_config.h.md) |
| `kernel/main.c` | ✅ 已读 | 内核初始化、启动流程、关机流程、现代 UEFI 引导 | [📖](../tmp/kernel_init/tmp_main.c.md) |
| `kernel/arch/i386/mpx.S` | ✅ 已读 | 中断/异常/IPC 入口、上下文恢复、SMP 启动、现代 64 位硬件演进 | [📖](../tmp/ipc/tmp_mpx.S.md) |

---

## 📚 模块阅读指南

### 模块组织原则

```
┌─────────────────────────────────────────────────────────────────┐
│  【核心层】机制抽象（纯逻辑，优先学习）                          │
│    模块 1: IPC 核心机制                                         │
│    模块 2: 进程管理与调度机制                                   │
│    模块 3: 系统调用机制                                         │
│                                                                 │
│  【服务层】用户态服务（架构级理解）                              │
│    模块 4: PM（进程管理服务器）                                  │
│    模块 5: VM（虚拟内存服务器）                                  │
│    模块 6: VFS（虚拟文件系统服务器）                             │
│    模块 7: 其他服务（RS/DS/Sched 等）                           │
│                                                                 │
│  【参考层】硬件抽象（可选参考）                                  │
│    附录 A: 中断与时钟（硬件相关）                                │
│    附录 B: 启动流程（硬件相关）                                  │
│    附录 C: 架构相关（i386 特定）                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

# 【核心层】机制抽象

---

## 模块 1: IPC 核心机制 ✅ 已完成

> **学习目标**: 理解消息传递的本质，而非汇编入口的实现细节。
> 
> **核心问题**: 内核如何安全地在进程间传递消息？

### 1.1 IPC 消息定义（纯逻辑）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `include/minix/ipcconst.h` | ✅ 已读 | IPC 系统调用号 (SEND/RECEIVE/NOTIFY/SENDNB/SENDA) | [📖](../tmp/ipc/tmp_ipcconst.h.md) |
| 2 | `include/minix/ipc.h` | ✅ 已读 | 消息结构体定义 (mess_1, mess_2, ..., message, asynmsg_t) | [📖](../tmp/ipc/tmp_ipc.h.md) |
| 3 | `include/minix/com.h` | ✅ 已读 | IPC 消息类型常量 (进程编号、消息类型范围、系统调用号) | [📖](../tmp/ipc/tmp_com.h.md) |
| 4 | `include/minix/type.h` | ✅ 已读 | 基本数据类型定义 (地址类型、I/O 向量、信号、负载、时钟) | [📖](../tmp/ipc/tmp_type.h.md) |
| 5 | `include/minix/const.h` | ✅ 已读 | 系统常量定义 (布尔值、内存页、文件类型、权限位、进程特权) | [📖](../tmp/ipc/tmp_const.h.md) |
| 6 | `kernel/ipc.h` | ✅ 已读 | IPC 常量和宏定义 (WILLRECEIVE/CANRECEIVE/FROM_KERNEL/MF_REPLY_PEND) | [📖](../tmp/ipc/tmp_ipc.h.md) |
| 7 | `kernel/ipc_filter.h` | ✅ 已读 | IPC 消息过滤器（黑名单/白名单） | [📖](../tmp/ipc/tmp_ipc_filter.h.md) |
| 8 | `sys/sys/ipc.h` | ✅ 已读 | System V IPC 权限结构体 (struct ipc_perm) | [📖](../tmp/ipc/tmp_sys_ipc.h.md) |
| 9 | `include/minix/ipc_filter.h` | ✅ 已读 | 用户空间 IPC 过滤器接口 | [📖](../ipc/ipc-detail.md#4-includeminixipc_filterh) |

### 1.2 IPC 核心实现（纯逻辑）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 10 | `kernel/proc.h` | ✅ 已读 | 进程控制块结构体 (p_reg, p_priv, p_sched, p_rts_flags) | [📖](../tmp/ipc/tmp_proc.h.md) |
| 11 | `kernel/proc.c` | ✅ 已读 | IPC 核心: mini_send/mini_receive/mini_notify, 调度, FPU 管理 | [📖](../tmp/ipc/tmp_proc_c.md) |

### 1.3 系统调用分发框架

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 12 | `kernel/system.c` | ✅ 已读 | 系统调用分发中心、信号管理、进程清理、IPC 过滤器管理 | [📖](../tmp/ipc/tmp_system_c.md) |
| 13 | `kernel/system.h` | ✅ 已读 | 系统调用框架头文件、sys_call_t 函数指针、信号处理完整流程 | [📖](../tmp/ipc/tmp_system.h.md) |

### 1.4 用户态 IPC 入口（参考性质）

> ⚠️ **注意**: 以下文件涉及硬件细节（软件中断），仅作为理解 IPC 入口的参考。

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 14 | `lib/libc/arch/i386/sys/_ipc.S` | ✅ 已读 | 用户态 IPC 汇编入口（软件中断 `int $33`） | [📖](../tmp/ipc/tmp__ipc.S.md) |
| 15 | `lib/libc/sys/syscall.c` | ✅ 已读 | 系统调用包装函数，m_type 双重语义、弱别名、错误码转换 | [📖](../tmp/ipc/tmp_syscall.c.md) |

---

## 模块 2: 进程管理与调度机制 ✅ 已完成

> **学习目标**: 理解进程状态转换和调度算法，而非 CPU 本地变量的实现。
> 
> **核心问题**: 内核如何决定哪个进程运行？进程状态如何转换？

### 2.1 进程特权与能力

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `kernel/priv.h` | ✅ 已读 | 进程特权结构体、能力隔离、IPC 权限模型、现代 64 位硬件改进 | [📖](../tmp/ipc/tmp_priv.h.md) |

### 2.2 内核配置与常量

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 2 | `kernel/config.h` | ✅ 已读 | 系统调用开关、资源限制配置、Feature flags 重构 | [📖](../tmp/kernel_init/tmp_config.h.md) |
| 3 | `kernel/const.h` | ✅ 已读 | 内核常量定义（端点验证、位图操作、用户地址限制） | [📖](../process/process-detail.md#六-kernelconsth-总结) |
| 4 | `kernel/type.h` | ✅ 已读 | 内核类型定义（proc_nr_t、sys_map_t、irq_hook_t） | [📖](../process/process-detail.md#七-kerneltypeh-总结) |

### 2.3 CPU 本地变量（参考性质）

> ⚠️ **注意**: 以下文件涉及 SMP 硬件细节，重点理解 Per-CPU 数据的概念。

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 5 | `kernel/cpulocals.h` | ✅ 已读 | CPU 本地变量结构体、SMP/单核宏抽象、Per-CPU 运行队列、FPU 惰性切换 | [📖](../tmp/ipc/tmp_cpulocals.h.md) |
| 6 | `kernel/cpulocals.c` | ✅ 已读 | CPU 本地变量数组定义（3 行代码） | [📖](../tmp/ipc/tmp_cpulocals.c.md) |

---

## 模块 3: 系统调用机制 ⭐ 当前模块

> **学习目标**: 理解每个系统调用的语义和实现逻辑。
> 
> **核心问题**: 内核提供了哪些服务？如何安全地提供这些服务？

### 📚 系统调用文档进度

> **文档重构状态**: 已完成语义化重写，以代码为唯一真相源

| 模块文档 | 状态 | 核心内容 | 逐行分析 |
|---------|------|----------|---------|
| [syscall.md](../syscall/syscall.md) | ✅ 完成 | 系统调用总览、模块架构 | - |
| [syscall-infrastructure.md](../syscall/syscall-infrastructure.md) | ✅ 完成 | 系统调用框架、分发机制、错误处理 | ✅ 完成 |
| [syscall-process.md](../syscall/syscall-process.md) | ✅ 完成 | 进程管理（fork/exec/exit/kill/clear） | ✅ 完成 |
| [syscall-schedule.md](../syscall/syscall-schedule.md) | ✅ 完成 | 调度控制（schedctl/schedule/runctl/statectl） | ✅ 完成 |
| [syscall-privilege.md](../syscall/syscall-privilege.md) | ✅ 完成 | 特权管理（privctl/setgrant） | ✅ 完成 |
| [syscall-memory.md](../syscall/syscall-memory.md) | ✅ 完成 | 内存操作（copy/safecopy/umap/vmctl） | ✅ 完成 |
| [syscall-interrupt.md](../syscall/syscall-interrupt.md) | ✅ 完成 | 中断与设备（irqctl/devio/vdevio） | ✅ 完成 |
| [syscall-time.md](../syscall/syscall-time.md) | ✅ 完成 | 时间管理（setalarm/vtimer/times/stime/settime） | ✅ 完成 |
| [syscall-signal.md](../syscall/syscall-signal.md) | ✅ 完成 | 信号处理（sigsend/getksig/endksig/sigreturn） | ✅ 完成 |
| [syscall-debug.md](../syscall/syscall-debug.md) | ✅ 完成 | 调试支持（trace/mcontext/getinfo/sprofile） | ✅ 完成 |
| [syscall-control.md](../syscall/syscall-control.md) | ✅ 完成 | 系统控制（abort/diagctl/update） | ✅ 完成 |

**文档特点**:
- ✅ 逐行级别代码分析
- ✅ 内存布局图示
- ✅ 设计原因阐述
- ✅ Rust 重构建议
- ✅ 模块化文档结构清晰

### 3.1 进程管理相关

> **详细文档**: [syscall-process.md](../syscall/syscall-process.md)

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 1 | `kernel/system/do_fork.c` | ✅ 已读 | fork 系统调用（PCB 复制、端点代数、特权处理） |
| 2 | `kernel/system/do_exec.c` | ✅ 已读 | exec 系统调用（替换进程映像、FPU 重置） |
| 3 | `kernel/system/do_exit.c` | ✅ 已读 | exit 系统调用（系统进程退出、SIGABRT） |
| 4 | `kernel/system/do_kill.c` | ✅ 已读 | kill 系统调用（信号发送、权限检查） |
| 5 | `kernel/system/do_clear.c` | ✅ 已读 | 清理进程（资源释放、IRQ钩子、端点清除） |

### 3.2 调度相关

> **详细文档**: [syscall-schedule.md](../syscall/syscall-schedule.md)

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 6 | `kernel/system/do_schedctl.c` | ✅ 已读 | 调度控制（内核/用户调度器、p_scheduler） |
| 7 | `kernel/system/do_schedule.c` | ✅ 已读 | 调度参数设置（权限检查、sched_proc） |
| 8 | `kernel/system/do_runctl.c` | ✅ 已读 | 运行控制（停止/恢复、RTS_PROC_STOP、RC_DELAY） |
| 9 | `kernel/system/do_statectl.c` | ✅ 已读 | 状态控制（IPC 过滤器、IPC 引用清理） |

### 3.3 特权与安全

> **详细文档**: [syscall-privilege.md](../syscall/syscall-privilege.md)

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 10 | `kernel/system/do_privctl.c` | ✅ 已读 | 特权控制（SYS_PRIV_SET_SYS、11 种请求类型、资源管理） |
| 11 | `kernel/system/do_setgrant.c` | ✅ 已读 | 设置授权表（Grant Table、s_grant_table/entries/endpoint） |

### 3.4 内存相关

> **详细文档**: [syscall-memory.md](../syscall/syscall-memory.md)

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 12 | `kernel/system/do_copy.c` | ✅ 已读 | 内存拷贝（SYS_VIRCOPY/PHYSCOPY、vir_addr、virtual_copy） |
| 13 | `kernel/system/do_safecopy.c` | ✅ 已读 | 安全拷贝（Grant机制、MAGIC授权、granter重定向、Live Update） |
| 14 | `kernel/system/do_umap.c` | ✅ 已读 | 地址映射（虚拟地址→物理地址、Grant验证、连续性检查） |
| 15 | `kernel/system/do_umap_remote.c` | ✅ 已读 | 远程地址映射（MEM_GRANT授权、vm_lookup、DMA支持） |
| 16 | `kernel/system/do_vumap.c` | ✅ 已读 | 批量虚拟地址映射（scatter-gather DMA、vumap_vir/phys、动态内存分配） |
| 17 | `kernel/system/do_memset.c` | ✅ 已读 | 基础内存填充（无权限检查、仅供VM使用） |
| 18 | `kernel/system/do_safememset.c` | ✅ 已读 | 安全内存填充（Grant验证、CPF_WRITE权限） |
| 19 | `kernel/system/do_vmctl.c` | ✅ 已读 | VM控制接口（页错误处理、内存请求队列、VM抑制） |

### 3.5 中断与设备

> **详细文档**: [syscall-interrupt.md](../syscall/syscall-interrupt.md)

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 20 | `kernel/system/do_irqctl.c` | ✅ 已读 | 中断控制（IRQ注册/启用/禁用、generic_handler、权限检查） |
| 21 | `kernel/system/do_devio.c` | ✅ 已读 | 设备I/O（单端口读写、端口权限检查、对齐检查） |
| 22 | `kernel/system/do_vdevio.c` | ✅ 已读 | 向量设备I/O（批量端口操作、静态缓冲区、权限检查） |

### 3.6 时间相关

> **详细文档**: [syscall-time.md](../syscall/syscall-time.md)

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 23 | `kernel/system/do_setalarm.c` | ✅ 已读 | 同步闹钟（通知消息、cause_alarm回调、SYS_PROC权限） |
| 24 | `kernel/system/do_vtimer.c` | ✅ 已读 | 虚拟定时器（VT_VIRTUAL/VT_PROF、SIGVTALRM/SIGPROF信号） |
| 25 | `kernel/system/do_times.c` | ✅ 已读 | 时间统计（CPU时间、monotonic/realtime/boot_time） |
| 26 | `kernel/system/do_stime.c` | ✅ 已读 | 设置启动时间（set_boottime） |
| 27 | `kernel/system/do_settime.c` | ✅ 已读 | 设置系统时间（adjtime微调、settimeofday直接设置） |

### 3.7 信号相关

> **详细文档**: [syscall-signal.md](../syscall/syscall-signal.md)

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 28 | `kernel/system/do_sigsend.c` | ✅ 已读 | 发送信号（sigmsg、sigframe、sigcontext、保存寄存器到用户栈） |
| 29 | `kernel/system/do_getksig.c` | ✅ 已读 | 获取内核信号（RTS_SIGNALED、s_sig_mgr权限检查） |
| 30 | `kernel/system/do_endksig.c` | ✅ 已读 | 结束内核信号（RTS_SIG_PENDING清除） |
| 31 | `kernel/system/do_sigreturn.c` | ✅ 已读 | 信号返回（从sigcontext恢复寄存器、FPU状态恢复） |

### 3.8 其他

> **详细文档**: [syscall-debug.md](../syscall/syscall-debug.md) | [syscall-process.md](../syscall/syscall-process.md) | [syscall-control.md](../syscall/syscall-control.md)

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 32 | `kernel/system/do_getinfo.c` | ✅ 已读 | 系统信息查询（SYS_GETINFO、GET_PROCTAB/KINFO/MACHINE/WHOAMI/RANDOMNESS） |
| 33 | `kernel/system/do_mcontext.c` | ✅ 已读 | 机器上下文（SYS_GETMCONTEXT/SETMCONTEXT、FPU状态、用户态线程） |
| 34 | `kernel/system/do_trace.c` | ✅ 已读 | 进程跟踪（SYS_TRACE、T_STOP/GETINS/SETUSER/STEP/SYSCALL、调试支持） |
| 35 | `kernel/system/do_sprofile.c` | ✅ 已读 | 统计性能分析（SYS_SPROF、PROF_START/STOP、RTC/NMI、采样频率） |
| 36 | `kernel/system/do_update.c` | ✅ 已读 | 系统更新（SYS_UPDATE、进程槽位交换、权限继承、热更新） |
| 37 | `kernel/system/do_diagctl.c` | ✅ 已读 | 诊断控制（DIAGCTL_CODE_DIAG/STACKTRACE/REGISTER、SIGKMESS） |
| 38 | `kernel/system/do_abort.c` | ✅ 已读 | 系统中止（SYS_ABORT、prepare_shutdown、RBT_REBOOT/POWER_OFF） |

---

# 【服务层】用户态服务

---

## 模块 4: PM（进程管理服务器） ✅ 已完成

> **学习目标**: 理解用户态进程管理的实现，POSIX 接口如何通过 IPC 实现。
> 
> **核心问题**: fork/exec/exit 如何在用户态实现？

> **详细文档**: [pm/pm.md](../pm/pm.md)

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/pm/pm.h` | ✅ 已读 | PM 主头文件 | [📖](../pm/pm-core-architecture.md) |
| 2 | `servers/pm/mproc.h` | ✅ 已读 | PM 进程结构 | [📖](../pm/pm-core-architecture.md) |
| 3 | `servers/pm/type.h` | ✅ 已读 | PM 类型定义 | [📖](../pm/pm-core-architecture.md) |
| 4 | `servers/pm/proto.h` | ✅ 已读 | PM 函数原型 | [📖](../pm/pm-core-architecture.md) |
| 5 | `servers/pm/glo.h` | ✅ 已读 | PM 全局变量 | [📖](../pm/pm-core-architecture.md) |
| 6 | `servers/pm/const.h` | ✅ 已读 | PM 常量 | [📖](../pm/pm-core-architecture.md) |
| 7 | `servers/pm/main.c` | ✅ 已读 | PM 主循环 | [📖](../pm/pm-core-architecture.md) |
| 8 | `servers/pm/forkexit.c` | ✅ 已读 | fork/exit 实现 | [📖](../pm/process-lifecycle.md) |
| 9 | `servers/pm/exec.c` | ✅ 已读 | exec 实现 | [📖](../pm/process-lifecycle.md) |
| 10 | `servers/pm/signal.c` | ✅ 已读 | 信号处理 | [📖](../pm/signal-event.md) |
| 11 | `servers/pm/schedule.c` | ✅ 已读 | 调度支持 | [📖](../pm/schedule-time.md) |
| 12 | `servers/pm/alarm.c` | ✅ 已读 | 定时器 | [📖](../pm/schedule-time.md) |
| 13 | `servers/pm/getset.c` | ✅ 已读 | get/set 系统调用 | [📖](../pm/syscall-interface.md) |
| 14 | `servers/pm/misc.c` | ✅ 已读 | 杂项系统调用 | [📖](../pm/syscall-interface.md) |
| 15 | `servers/pm/trace.c` | ✅ 已读 | ptrace 支持 | [📖](../pm/debug-profile.md) |
| 16 | `servers/pm/event.c` | ✅ 已读 | 事件处理 | [📖](../pm/signal-event.md) |
| 17 | `servers/pm/time.c` | ✅ 已读 | 时间相关 | [📖](../pm/schedule-time.md) |
| 18 | `servers/pm/profile.c` | ✅ 已读 | 性能分析 | [📖](../pm/debug-profile.md) |
| 19 | `servers/pm/table.c` | ✅ 已读 | 表管理 | [📖](../pm/pm-core-architecture.md) |
| 20 | `servers/pm/utility.c` | ✅ 已读 | 工具函数 | [📖](../pm/utility-functions.md) |

---

## 模块 5: VM（虚拟内存服务器）

> **学习目标**: 理解用户态虚拟内存管理的实现，地址空间如何通过 IPC 管理。
> 
> **核心问题**: 页错误如何处理？mmap 如何实现？

### 5.1 VM 相关头文件（跨目录）

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 1 | `kernel/vm.h` | ⏳ 待读 | 内核 VM 接口（内核侧） |
| 2 | `include/minix/vm.h` | ⏳ 待读 | 用户态 VM 接口（用户侧） |

### 5.2 VM 服务器头文件

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 3 | `servers/vm/vm.h` | ⏳ 待读 | VM 服务器主头文件 |
| 4 | `servers/vm/vmproc.h` | ⏳ 待读 | VM 进程结构 |
| 5 | `servers/vm/region.h` | ⏳ 待读 | 内存区域定义 |
| 6 | `servers/vm/pt.h` | ⏳ 待读 | 页表管理 |
| 7 | `servers/vm/memtype.h` | ⏳ 待读 | 内存类型 |
| 8 | `servers/vm/proto.h` | ⏳ 待读 | VM 函数原型 |
| 9 | `servers/vm/glo.h` | ⏳ 待读 | VM 全局变量 |
| 10 | `servers/vm/util.h` | ⏳ 待读 | VM 工具函数 |
| 11 | `servers/vm/cache.h` | ⏳ 待读 | VM 缓存 |
| 12 | `servers/vm/sanitycheck.h` | ⏳ 待读 | 健全性检查 |
| 13 | `servers/vm/unavl.h` | ⏳ 待读 | AVL 树 |
| 14 | `servers/vm/regionavl.h` | ⏳ 待读 | 区域 AVL 树 |
| 15 | `servers/vm/regionavl_defs.h` | ⏳ 待读 | 区域 AVL 树定义 |
| 16 | `servers/vm/cavl_if.h` | ⏳ 待读 | 通用 AVL 接口 |
| 17 | `servers/vm/cavl_impl.h` | ⏳ 待读 | 通用 AVL 实现 |
| 18 | `servers/vm/phys_region.h` | ⏳ 待读 | 物理区域 |
| 19 | `servers/vm/memlist.h` | ⏳ 待读 | 内存列表 |
| 20 | `servers/vm/fdref.h` | ⏳ 待读 | 文件描述符引用 |

### 5.3 VM 核心实现

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 21 | `servers/vm/main.c` | ⏳ 待读 | VM 主循环 |
| 22 | `servers/vm/alloc.c` | ⏳ 待读 | 内存分配 |
| 23 | `servers/vm/pagetable.c` | ⏳ 待读 | 页表管理 |
| 24 | `servers/vm/region.c` | ⏳ 待读 | 区域管理 |
| 25 | `servers/vm/pagefaults.c` | ⏳ 待读 | 页错误处理 |
| 26 | `servers/vm/fork.c` | ⏳ 待读 | fork 支持 |
| 27 | `servers/vm/exit.c` | ⏳ 待读 | exit 支持 |
| 28 | `servers/vm/break.c` | ⏳ 待读 | heap 管理 (sbrk) |
| 29 | `servers/vm/mmap.c` | ⏳ 待读 | mmap 实现 |
| 30 | `servers/vm/vfs.c` | ⏳ 待读 | VFS 接口 |
| 31 | `servers/vm/rs.c` | ⏳ 待读 | RS 接口 |
| 32 | `servers/vm/regionavl.c` | ⏳ 待读 | 区域 AVL 树实现 |
| 33 | `servers/vm/slaballoc.c` | ⏳ 待读 | 分配器 |
| 34 | `servers/vm/utility.c` | ⏳ 待读 | 工具函数 |
| 35 | `servers/vm/fdref.c` | ⏳ 待读 | 文件描述符引用 |
| 36 | `servers/vm/mem_cache.c` | ⏳ 待读 | 内存缓存 |
| 37 | `servers/vm/mem_shared.c` | ⏳ 待读 | 共享内存 |
| 38 | `servers/vm/acl.c` | ⏳ 待读 | 访问控制 |
| 39 | `servers/vm/cache.c` | ⏳ 待读 | 缓存 |
| 40 | `servers/vm/mem_anon.c` | ⏳ 待读 | 匿名内存 |
| 41 | `servers/vm/mem_file.c` | ⏳ 待读 | 文件映射 |
| 42 | `servers/vm/mem_directphys.c` | ⏳ 待读 | 直接物理内存 |
| 43 | `servers/vm/mem_anon_contig.c` | ⏳ 待读 | 连续匿名内存 |
| 44 | `servers/vm/pb.c` | ⏳ 待读 | 页框管理 |

---

## 模块 6: VFS（虚拟文件系统服务器）

> **学习目标**: 理解文件系统抽象层的实现，VFS 如何通过 IPC 与具体文件系统交互。
> 
> **核心问题**: open/read/write 如何通过 IPC 实现？

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 1 | `servers/vfs/fs.h` | ⏳ 待读 | VFS 文件系统头文件 |
| 2 | `servers/vfs/fproc.h` | ⏳ 待读 | VFS 进程结构 |
| 3 | `servers/vfs/vnode.h` | ⏳ 待读 | 虚拟节点 |
| 4 | `servers/vfs/vmnt.h` | ⏳ 待读 | 挂载点 |
| 5 | `servers/vfs/type.h` | ⏳ 待读 | VFS 类型定义 |
| 6 | `servers/vfs/proto.h` | ⏳ 待读 | VFS 函数原型 |
| 7 | `servers/vfs/glo.h` | ⏳ 待读 | VFS 全局变量 |
| 8 | `servers/vfs/const.h` | ⏳ 待读 | VFS 常量 |
| 9 | `servers/vfs/dmap.h` | ⏳ 待读 | 设备映射 |
| 10 | `servers/vfs/request.h` | ⏳ 待读 | 请求头文件 |
| 11 | `servers/vfs/path.h` | ⏳ 待读 | 路径头文件 |
| 12 | `servers/vfs/tll.h` | ⏳ 待读 | TLL 头文件 |
| 13 | `servers/vfs/threads.h` | ⏳ 待读 | 线程头文件 |
| 14 | `servers/vfs/file.h` | ⏳ 待读 | 文件头文件 |
| 15 | `servers/vfs/lock.h` | ⏳ 待读 | 锁头文件 |
| 16 | `servers/vfs/main.c` | ⏳ 待读 | VFS 主循环 |
| 17 | `servers/vfs/open.c` | ⏳ 待读 | open 实现 |
| 18 | `servers/vfs/read.c` | ⏳ 待读 | read 实现 |
| 19 | `servers/vfs/write.c` | ⏳ 待读 | write 实现 |
| 20 | `servers/vfs/exec.c` | ⏳ 待读 | exec 支持 |
| 21 | `servers/vfs/mount.c` | ⏳ 待读 | mount 实现 |
| 22 | `servers/vfs/vmnt.c` | ⏳ 待读 | 挂载点管理 |
| 23 | `servers/vfs/smap.c` | ⏳ 待读 | 统计映射 |
| 24 | `servers/vfs/socket.c` | ⏳ 待读 | socket 实现 |
| 25 | `servers/vfs/pipe.c` | ⏳ 待读 | pipe 实现 |
| 26 | `servers/vfs/stadir.c` | ⏳ 待读 | stat 实现 |
| 27 | `servers/vfs/time.c` | ⏳ 待读 | 时间相关 |
| 28 | `servers/vfs/worker.c` | ⏳ 待读 | 工作线程 |
| 29 | `servers/vfs/table.c` | ⏳ 待读 | 表管理 |
| 30 | `servers/vfs/select.c` | ⏳ 待读 | select 实现 |
| 31 | `servers/vfs/sdev.c` | ⏳ 待读 | 特殊设备 |
| 32 | `servers/vfs/vnode.c` | ⏳ 待读 | 虚拟节点管理 |
| 33 | `servers/vfs/tll.c` | ⏳ 待读 | TLL 实现 |
| 34 | `servers/vfs/protect.c` | ⏳ 待读 | 保护机制 |
| 35 | `servers/vfs/path.c` | ⏳ 待读 | 路径处理 |
| 36 | `servers/vfs/utility.c` | ⏳ 待读 | 工具函数 |
| 37 | `servers/vfs/request.c` | ⏳ 待读 | 请求处理 |
| 38 | `servers/vfs/bdev.c` | ⏳ 待读 | 块设备 |
| 39 | `servers/vfs/device.c` | ⏳ 待读 | 设备管理 |
| 40 | `servers/vfs/link.c` | ⏳ 待读 | 链接实现 |
| 41 | `servers/vfs/misc.c` | ⏳ 待读 | 杂项函数 |
| 42 | `servers/vfs/coredump.c` | ⏳ 待读 | 核心转储 |
| 43 | `servers/vfs/cdev.c` | ⏳ 待读 | 字符设备 |
| 44 | `servers/vfs/gcov.c` | ⏳ 待读 | GCOV 支持 |
| 45 | `servers/vfs/dmap.c` | ⏳ 待读 | 设备映射管理 |
| 46 | `servers/vfs/comm.c` | ⏳ 待读 | 通信 |
| 47 | `servers/vfs/filedes.c` | ⏳ 待读 | 文件描述符 |
| 48 | `servers/vfs/lock.c` | ⏳ 待读 | 锁实现 |

---

## 模块 7: 其他服务

> **学习目标**: 理解其他系统服务的实现，服务间如何协作。

### 7.1 RS（重启服务器）

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 1 | `servers/rs/inc.h` | ⏳ 待读 | RS 包含头文件 |
| 2 | `servers/rs/proto.h` | ⏳ 待读 | RS 函数原型 |
| 3 | `servers/rs/glo.h` | ⏳ 待读 | RS 全局变量 |
| 4 | `servers/rs/const.h` | ⏳ 待读 | RS 常量 |
| 5 | `servers/rs/type.h` | ⏳ 待读 | RS 类型定义 |
| 6 | `servers/rs/main.c` | ⏳ 待读 | RS 主循环 |
| 7 | `servers/rs/manager.c` | ⏳ 待读 | 服务管理 |
| 8 | `servers/rs/exec.c` | ⏳ 待读 | 服务执行 |
| 9 | `servers/rs/error.c` | ⏳ 待读 | 错误处理 |
| 10 | `servers/rs/utility.c` | ⏳ 待读 | 工具函数 |
| 11 | `servers/rs/update.c` | ⏳ 待读 | 热更新 |
| 12 | `servers/rs/request.c` | ⏳ 待读 | 请求处理 |
| 13 | `servers/rs/table.c` | ⏳ 待读 | 表管理 |

### 7.2 Sched（调度服务器）

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 1 | `servers/sched/sched.h` | ⏳ 待读 | 调度器头文件 |
| 2 | `servers/sched/schedproc.h` | ⏳ 待读 | 调度进程结构 |
| 3 | `servers/sched/proto.h` | ⏳ 待读 | 调度函数原型 |
| 4 | `servers/sched/main.c` | ⏳ 待读 | 调度器主循环 |
| 5 | `servers/sched/schedule.c` | ⏳ 待读 | 调度实现 |

### 7.3 DS（数据存储服务器）

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 1 | `servers/ds/inc.h` | ⏳ 待读 | DS 包含头文件 |
| 2 | `servers/ds/proto.h` | ⏳ 待读 | DS 函数原型 |
| 3 | `servers/ds/store.h` | ⏳ 待读 | 存储头文件 |
| 4 | `servers/ds/main.c` | ⏳ 待读 | DS 主循环 |
| 5 | `servers/ds/store.c` | ⏳ 待读 | 数据存储 |

### 7.4 IPC 服务器（System V IPC）

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 1 | `servers/ipc/inc.h` | ⏳ 待读 | IPC 服务器包含头文件 |
| 2 | `servers/ipc/main.c` | ⏳ 待读 | IPC 服务器主循环 |
| 3 | `servers/ipc/shm.c` | ⏳ 待读 | 共享内存 |
| 4 | `servers/ipc/sem.c` | ⏳ 待读 | 信号量 |
| 5 | `servers/ipc/utility.c` | ⏳ 待读 | 工具函数 |

### 7.5 IS（信息服务器）

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 1 | `servers/is/inc.h` | ⏳ 待读 | IS 包含头文件 |
| 2 | `servers/is/proto.h` | ⏳ 待读 | IS 函数原型 |
| 3 | `servers/is/glo.h` | ⏳ 待读 | IS 全局变量 |
| 4 | `servers/is/main.c` | ⏳ 待读 | IS 主循环 |
| 5 | `servers/is/dmp.c` | ⏳ 待读 | 转储实现 |
| 6 | `servers/is/dmp_pm.c` | ⏳ 待读 | PM 转储 |
| 7 | `servers/is/dmp_kernel.c` | ⏳ 待读 | 内核转储 |
| 8 | `servers/is/dmp_fs.c` | ⏳ 待读 | FS 转储 |
| 9 | `servers/is/dmp_vm.c` | ⏳ 待读 | VM 转储 |
| 10 | `servers/is/dmp_rs.c` | ⏳ 待读 | RS 转储 |
| 11 | `servers/is/dmp_ds.c` | ⏳ 待读 | DS 转储 |

### 7.6 MIB（管理信息库）

| 序号 | 文件 | 状态 | 说明 |
|------|------|------|------|
| 1 | `servers/mib/mib.h` | ⏳ 待读 | MIB 头文件 |
| 2 | `servers/mib/main.c` | ⏳ 待读 | MIB 主循环 |
| 3 | `servers/mib/proc.c` | ⏳ 待读 | 进程相关 |
| 4 | `servers/mib/minix.c` | ⏳ 待读 | Minix 相关 |
| 5 | `servers/mib/hw.c` | ⏳ 待读 | 硬件相关 |
| 6 | `servers/mib/remote.c` | ⏳ 待读 | 远程相关 |
| 7 | `servers/mib/kern.c` | ⏳ 待读 | 内核相关 |
| 8 | `servers/mib/tree.c` | ⏳ 待读 | 树管理 |
| 9 | `servers/mib/vm.c` | ⏳ 待读 | VM 相关 |

### 7.7 Devman（设备管理服务器）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/devman/proto.h` | ⏳ 待读 | Devman 函数原型 | - |
| 2 | `servers/devman/devman.h` | ⏳ 待读 | Devman 头文件 | - |
| 3 | `servers/devman/devinfo.h` | ⏳ 待读 | 设备信息头文件 | - |
| 4 | `servers/devman/main.c` | ⏳ 待读 | Devman 主循环 | - |
| 5 | `servers/devman/device.c` | ⏳ 待读 | 设备管理 | - |
| 6 | `servers/devman/bind.c` | ⏳ 待读 | 绑定实现 | - |
| 7 | `servers/devman/buf.c` | ⏳ 待读 | 缓冲区管理 | - |

### 7.8 Input（输入服务器）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/input/input.h` | ⏳ 待读 | 输入头文件 | - |
| 2 | `servers/input/input.c` | ⏳ 待读 | 输入实现 | - |

---

# 【参考层】硬件抽象

> ⚠️ **重要提示**: 以下内容涉及硬件细节，仅作为理解机制实现的参考。
> 
> **学习建议**: 重点理解"机制是什么"，而非"硬件怎么操作"。现代 OS（如 Rust 实现）应使用 trait 抽象硬件，避免直接依赖具体硬件。

---

## 附录 A: 中断与时钟（硬件相关）

> **机制关注点**: 中断如何触发调度？时钟如何驱动时间片轮转？
> 
> **可跳过的硬件细节**: 8259/APIC 寄存器操作、I/O 端口编程。

### A.1 中断管理

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `kernel/interrupt.h` | ✅ 已读 | 中断子系统头文件、hw_intr.h 委托模式、Rust trait 抽象设计 | [📖](../tmp/ipc/tmp_interrupt.h.md) |
| 2 | `kernel/interrupt.c` | ✅ 已读 | 中断处理程序管理、共享中断（位图 ID）、虚假中断处理 | [📖](../tmp/ipc/tmp_interrupt.c.md) |

### A.2 时钟管理

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 3 | `kernel/clock.h` | ✅ 已读 | 时钟子系统头文件、ms_2_cpu_time/cycles 计费、TSC 稳定性假设、现代 OS 对比 | [📖](../tmp/ipc/tmp_clock.h.md) |
| 4 | `kernel/clock.c` | ✅ 已读 | 时钟中断处理、时间记账（BILLABLE）、虚拟定时器、负载统计 | [📖](../tmp/ipc/tmp_clock.c.md) |

### A.3 中断入口（汇编）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 5 | `kernel/arch/i386/mpx.S` | ✅ 已读 | 中断/异常/IPC 入口、上下文恢复、SMP 启动、现代 64 位硬件演进 | [📖](../tmp/ipc/tmp_mpx.S.md) |

### A.4 中断控制器（硬件）

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 6 | `kernel/arch/i386/i8259.c` | ⏳ 待读 | 8259 中断控制器 | - |
| 7 | `kernel/arch/i386/apic.c` | ⏳ 待读 | APIC 支持 | - |
| 8 | `kernel/arch/i386/apic.h` | ⏳ 待读 | APIC 头文件 | - |
| 9 | `kernel/arch/i386/apic_asm.S` | ⏳ 待读 | APIC 汇编 | - |
| 10 | `kernel/arch/i386/apic_asm.h` | ⏳ 待读 | APIC 汇编头文件 | - |
| 11 | `kernel/arch/i386/arch_clock.c` | ⏳ 待读 | 架构相关时钟 | - |
| 12 | `kernel/arch/i386/exception.c` | ⏳ 待读 | 异常处理 | - |
| 13 | `kernel/arch/i386/io_intr.S` | ⏳ 待读 | 中断 I/O | - |

---

## 附录 B: 启动流程（硬件相关）

> **机制关注点**: 内核初始化的顺序是什么？各子系统如何初始化？
> 
> **可跳过的硬件细节**: GDT/IDT 设置、实模式到保护模式切换、页表初始化。

### B.1 内核入口

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `kernel/main.c` | ✅ 已读 | 内核初始化、启动流程、关机流程、现代 UEFI 引导 | [📖](../tmp/kernel_init/tmp_main.c.md) |
| 2 | `kernel/arch/i386/head.S` | ⏳ 待读 | 内核启动入口（汇编） | - |
| 3 | `kernel/arch/i386/pre_init.c` | ⏳ 待读 | 早期初始化 | - |

### B.2 内核核心文件

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 4 | `kernel/proto.h` | ⏳ 待读 | 内核函数原型 | - |
| 5 | `kernel/glo.h` | ⏳ 待读 | 内核全局变量 | - |
| 6 | `kernel/kernel.h` | ⏳ 待读 | 内核主头文件 | - |
| 7 | `kernel/table.c` | ⏳ 待读 | 内核数据表 | - |

### B.3 调试与工具

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 8 | `kernel/debug.c` | ⏳ 待读 | 调试支持 | - |
| 9 | `kernel/debug.h` | ⏳ 待读 | 调试头文件 | - |
| 10 | `kernel/utility.c` | ⏳ 待读 | 工具函数 | - |
| 11 | `kernel/usermapped_data.c` | ⏳ 待读 | 用户映射数据 | - |

### B.4 其他

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 12 | `kernel/watchdog.c` | ⏳ 待读 | 看门狗 | - |
| 13 | `kernel/watchdog.h` | ⏳ 待读 | 看门狗头文件 | - |
| 14 | `kernel/profile.c` | ⏳ 待读 | 性能分析 | - |
| 15 | `kernel/profile.h` | ⏳ 待读 | 性能分析头文件 | - |
| 16 | `kernel/spinlock.h` | ⏳ 待读 | 自旋锁 | - |

---

## 附录 C: 架构相关（i386 特定）

> **机制关注点**: 无。这些是纯硬件相关代码，仅作为参考。
> 
> **学习建议**: 现代实现应使用 trait 抽象，避免直接依赖 i386 特性。

### C.1 启动与入口

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `kernel/arch/i386/head.S` | ⏳ 待读 | 内核启动入口（汇编） | - |
| 2 | `kernel/arch/i386/mpx.S` | ✅ 已读 | 中断/异常/IPC 入口、上下文恢复、SMP 启动 | [📖](../tmp/ipc/tmp_mpx.S.md) |
| 3 | `kernel/arch/i386/klib.S` | ⏳ 待读 | 内核库函数（汇编） | - |
| 4 | `kernel/arch/i386/pre_init.c` | ⏳ 待读 | 早期初始化 | - |
| 5 | `kernel/arch/i386/usermapped_glo_ipc.S` | ⏳ 待读 | 用户映射全局 IPC 数据 | - |

### C.2 内存与保护

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 6 | `kernel/arch/i386/memory.c` | ⏳ 待读 | 内存管理 | - |
| 7 | `kernel/arch/i386/protect.c` | ⏳ 待读 | 保护模式设置 | - |
| 8 | `kernel/arch/i386/pg_utils.c` | ⏳ 待读 | 页表工具 | - |

### C.3 I/O 端口

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 9 | `kernel/arch/i386/io_inb.S` | ⏳ 待读 | I/O 输入字节 | - |
| 10 | `kernel/arch/i386/io_inw.S` | ⏳ 待读 | I/O 输入字 | - |
| 11 | `kernel/arch/i386/io_inl.S` | ⏳ 待读 | I/O 输入长字 | - |
| 12 | `kernel/arch/i386/io_outb.S` | ⏳ 待读 | I/O 输出字节 | - |
| 13 | `kernel/arch/i386/io_outw.S` | ⏳ 待读 | I/O 输出字 | - |
| 14 | `kernel/arch/i386/io_outl.S` | ⏳ 待读 | I/O 输出长字 | - |

### C.4 SMP 支持

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 15 | `kernel/arch/i386/arch_smp.c` | ⏳ 待读 | SMP 架构支持 | - |
| 16 | `kernel/arch/i386/trampoline.S` | ⏳ 待读 | SMP 启动跳板 | - |
| 17 | `kernel/smp.c` | ⏳ 待读 | SMP 核心实现 | - |
| 18 | `kernel/smp.h` | ⏳ 待读 | SMP 头文件 | - |

### C.5 其他

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 19 | `kernel/arch/i386/acpi.c` | ⏳ 待读 | ACPI 支持 | - |
| 20 | `kernel/arch/i386/acpi.h` | ⏳ 待读 | ACPI 头文件 | - |
| 21 | `kernel/arch/i386/debugreg.S` | ⏳ 待读 | 调试寄存器 | - |
| 22 | `kernel/arch/i386/debugreg.h` | ⏳ 待读 | 调试寄存器头文件 | - |
| 23 | `kernel/arch/i386/breakpoints.c` | ⏳ 待读 | 断点支持 | - |
| 24 | `kernel/arch/i386/arch_system.c` | ⏳ 待读 | 架构相关系统调用 | - |
| 25 | `kernel/arch/i386/arch_do_vmctl.c` | ⏳ 待读 | 架构相关 VM 控制 | - |
| 26 | `kernel/arch/i386/arch_watchdog.c` | ⏳ 待读 | 看门狗 | - |
| 27 | `kernel/arch/i386/arch_reset.c` | ⏳ 待读 | 系统重启 | - |
| 28 | `kernel/arch/i386/do_readbios.c` | ⏳ 待读 | 读取 BIOS | - |
| 29 | `kernel/arch/i386/do_iopenable.c` | ⏳ 待读 | I/O 权限 | - |
| 30 | `kernel/arch/i386/do_sdevio.c` | ⏳ 待读 | 安全设备 I/O | - |
| 31 | `kernel/arch/i386/oxpcie.c` | ⏳ 待读 | OxPCIe 支持 | - |
| 32 | `kernel/arch/i386/oxpcie.h` | ⏳ 待读 | OxPCIe 头文件 | - |
| 33 | `kernel/arch/i386/direct_tty_utils.c` | ⏳ 待读 | 直接 TTY 工具 | - |
| 34 | `kernel/arch/i386/usermapped_data_arch.c` | ⏳ 待读 | 用户映射数据架构相关 | - |
| 35 | `kernel/arch/i386/sconst.h` | ⏳ 待读 | 架构常量 | - |
| 36 | `kernel/arch/i386/glo.h` | ⏳ 待读 | 架构全局变量 | - |
| 37 | `kernel/arch/i386/serial.h` | ⏳ 待读 | 串口头文件 | - |

---

## � 阅读笔记

### 已总结文件

| 文件 | 核心概念 |
|------|----------|
| `sys/sys/ipc.h` | IPC 权限管理 (uid/gid/mode) |
| `kernel/ipc.h` | IPC 常量和宏定义 (WILLRECEIVE/CANRECEIVE/FROM_KERNEL/MF_REPLY_PEND) |
| `kernel/ipc_filter.h` | 消息过滤器 (黑名单/白名单) |
| `kernel/system.c` | 系统调用分发、信号管理、进程清理、调度更新、IPC 过滤器、权限管理 |
| `include/minix/ipcconst.h` | IPC 系统调用号 (SEND/RECEIVE/NOTIFY) |
| `include/minix/ipc.h` | IPC 消息结构体定义 (64字节固定大小，union消息类型) |
| `include/minix/com.h` | IPC 消息类型常量 (进程编号、消息类型范围、系统调用号) |
| `include/minix/type.h` | 基本数据类型定义 (地址类型、I/O 向量、信号、负载、时钟、资源管理) |
| `include/minix/const.h` | 系统常量定义 (布尔值、内存页、文件类型、权限位、进程特权、启动选项、网络参数) |
| `kernel/proc.h` | 进程控制块结构体 (p_reg, p_priv, p_sched, p_rts_flags) |
| `kernel/clock.c` | 时钟中断处理、时间记账（BILLABLE 机制）、虚拟定时器、负载统计、多核时间同步 |
| `kernel/interrupt.c` | 中断处理程序管理、共享中断（位图 ID）、虚假中断处理、调用者管理内存 |
| `kernel/proc.c` | 进程调度、IPC 消息传递、异步消息处理、FPU 状态管理 |
| `lib/libc/sys/syscall.c` | 系统调用包装函数、m_type 双重语义、弱别名机制、错误码转换 |
| `lib/libc/arch/i386/sys/_ipc.S` | 用户态 IPC 汇编入口（int $33） |

### proc.c 核心发现

#### 1. IPC 核心机制
- **同步消息**：mini_send/mini_receive，零拷贝优化，发送队列排队
- **异步消息**：senda 系统调用，用户空间表，延迟投递
- **轻量通知**：mini_notify，位图存储，非阻塞

#### 2. 时钟与调度
- **时间记账**：BILLABLE 机制，微内核服务时间归属用户进程
- **虚拟定时器**：ITIMER_VIRTUAL（用户态）/ ITIMER_PROF（用户态+系统态）
- **多核时间同步**：BSP 维护全局时间，AP 只维护本地定时器
- **NTP 平滑调整**：每 2 tick 调整 1 tick，避免时间跳变
- **负载统计**：循环缓冲区，150 槽位×6 秒，支持 getloadavg()

#### 3. 进程调度
- **多级反馈队列**：每个优先级一个队列
- **位图加速**：快速找到最高优先级就绪进程
- **RTS 标志**：自动入队/出队机制

#### 4. FPU 状态管理
- **惰性保存**：只在进程切换时保存
- **每进程保存区**：fpu_state[NR_PROCS] 全局数组
- **异常处理**：copr_not_available_handler，不调度直接返回

#### 5. BKL 保护机制
- **获取时机**：进入内核时（context_stop(用户进程)）
- **释放时机**：返回用户态时（context_stop(KERNEL)）
- **保护范围**：所有系统调用、IPC、调度、异步消息

#### 6. 中断管理
- **共享中断**：数组+链表混合结构，轮询所有处理程序
- **位图 ID**：1, 2, 4, 8... 快速状态检查，最多 32 个处理程序
- **虚假中断**：自动屏蔽，指数退避报告（100/200/400...）
- **调用者内存**：钩子结构体由调用者提供，内核不分配
- **中断到 IPC**：处理程序发送 `mini_notify`，复杂处理延迟到进程上下文

#### 7. 发现的 Bug
- **enqueue 中的 enter_queue 记录错误**：IPC 场景中记录了错误的进程时间
- **异步消息表语义混淆**：`-1` 既表示"初始状态"又表示"处理中状态"

---

## �� 文件统计

| 分类 | 文件数 | 已读 | 待读 |
|------|--------|------|------|
| 【核心层】机制抽象 | 51 | 36 | 15 |
| 【服务层】用户态服务 | 127 | 20 | 107 |
| 【参考层】硬件抽象 | 53 | 6 | 47 |
| **总计** | **231** | **62** | **169** |

> **注意**: 部分文件在多个模块中重复引用，实际独立文件数为 178 个。

---

*更新于: 2025*
