# 启动流程详细文档

## 目录

1. [config.h 总结](#一configh-总结)
2. [main.c 总结](#二mainc-总结)

---

# 一、config.h 总结

> **文件位置**: `kernel/config.h`
> **核心功能**: 内核配置文件，控制系统调用启用/禁用和系统资源配置

## 1.1 文件概述

config.h 是 Minix3 内核的编译时配置文件，主要功能：

```
┌─────────────────────────────────────────────────────────────────┐
│                    config.h 的核心职责                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ 1. 系统调用开关：                                                │
│    - USE_FORK: 是否编译 fork 系统调用                           │
│    - USE_EXEC: 是否编译 exec 系统调用                           │
│    - USE_EXIT: 是否编译 exit 系统调用                           │
│    - ... 共 28 个系统调用开关                                   │
│                                                                 │
│ 2. 系统资源配置：                                                │
│    - NR_IRQ_HOOKS: 中断钩子数量                                 │
│    - VDEVIO_BUF_SIZE: 向量 I/O 缓冲区大小                       │
│    - K_PARAM_SIZE: 内核参数大小                                 │
│                                                                 │
│ 3. 架构特定配置：                                                │
│    - ARM 架构的 USE_PADCONF                                    │
│    - APIC 模式的 NR_IRQ_HOOKS                                  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 1.2 系统调用开关

### 完整列表

```c
/* 进程管理 */
#define USE_FORK        1    /* fork a new process */
#define USE_NEWMAP      1    /* set a new memory map */
#define USE_EXEC        1    /* update process after execute */
#define USE_CLEAR       1    /* clean up after process exit */
#define USE_EXIT        1    /* a system process wants to exit */
#define USE_TRACE       1    /* process information and tracing */

/* 信号管理 */
#define USE_GETKSIG     1    /* retrieve pending kernel signals */
#define USE_ENDKSIG     1    /* finish pending kernel signals */
#define USE_KILL        1    /* send a signal to a process */
#define USE_SIGSEND     1    /* send POSIX-style signal */
#define USE_SIGRETURN   1    /* sys_sigreturn(proc_nr, ctxt_ptr, flags) */

/* 系统控制 */
#define USE_ABORT       1    /* shut down MINIX */
#define USE_GETINFO     1    /* retrieve a copy of kernel data */
#define USE_TIMES       1    /* get process and system time info */
#define USE_SETALARM    1    /* schedule a synchronous alarm */
#define USE_VTIMER      1    /* set or retrieve a process-virtual timer */

/* I/O 操作 */
#define USE_DEVIO       1    /* read or write a single I/O port */
#define USE_VDEVIO      1    /* process vector with I/O requests */
#define USE_SDEVIO      1    /* perform I/O request on a buffer */
#define USE_IRQCTL      1    /* set an interrupt policy */

/* 特权控制 */
#define USE_PRIVCTL     1    /* system privileges control */

/* 内存操作 */
#define USE_UMAP        1    /* map virtual to physical address */
#define USE_UMAP_REMOTE 1    /* sys_umap on behalf of another process */
#define USE_VUMAP       1    /* vectored virtual to physical mapping */
#define USE_VIRCOPY     1    /* copy using virtual addressing */
#define USE_PHYSCOPY    1    /* copy using physical addressing */
#define USE_MEMSET      1    /* write char to a given memory area */

/* 进程状态 */
#define USE_RUNCTL      1    /* control stop flags of a process */
#define USE_STATECTL    1    /* let a process control its state */
#define USE_MCONTEXT    1    /* enable getting/setting of machine context */

/* 架构特定 */
#if defined(__arm__)
#define USE_PADCONF     1    /* configure pinmux */
#endif
```

### 分类汇总

| 分类 | 系统调用 | 数量 |
|------|----------|------|
| **进程管理** | FORK, NEWMAP, EXEC, CLEAR, EXIT, TRACE | 6 |
| **信号管理** | GETKSIG, ENDKSIG, KILL, SIGSEND, SIGRETURN | 5 |
| **系统控制** | ABORT, GETINFO, TIMES, SETALARM, VTIMER | 5 |
| **I/O 操作** | DEVIO, VDEVIO, SDEVIO, IRQCTL | 4 |
| **特权控制** | PRIVCTL | 1 |
| **内存操作** | UMAP, UMAP_REMOTE, VUMAP, VIRCOPY, PHYSCOPY, MEMSET | 6 |
| **进程状态** | RUNCTL, STATECTL, MCONTEXT | 3 |
| **架构特定** | PADCONF (ARM) | 1 |
| **总计** | | **31** |

## 1.3 系统资源配置

```c
/* 中断钩子数量 */
#if defined(USE_APIC)
#define NR_IRQ_HOOKS    64    /* APIC 模式：64 个钩子 */
#else
#define NR_IRQ_HOOKS    16    /* PIC 模式：16 个钩子 */
#endif

/* 向量 I/O 缓冲区大小 */
#define VDEVIO_BUF_SIZE  64   /* 最多 64 个 I/O 请求 */

/* 内核参数大小 */
#define K_PARAM_SIZE     1024 /* 内核参数缓冲区 */
```

## 1.4 设计原因

### 为什么使用编译时配置？

| 方面 | 编译时配置 | 运行时配置 |
|------|-----------|-----------|
| **二进制大小** | ✅ 未使用的代码不编译 | ❌ 所有代码都编译 |
| **性能** | ✅ 无运行时检查 | ❌ 每次调用都检查 |
| **灵活性** | ❌ 需要重新编译 | ✅ 可动态调整 |
| **安全性** | ✅ 攻击面小 | ❌ 攻击面大 |

**Minix3 选择**：编译时配置，优先保证：
1. 内核最小化（微内核原则）
2. 性能优化（无运行时开销）
3. 安全性（减少攻击面）

### 系统调用开关的工作原理

```c
/* system.c 中的条件编译 */
#if USE_FORK
int do_fork(struct proc *caller, message *m_ptr) {
    // fork 实现
}
#endif

/* 分发表 */
int (*call_vec[NR_SYS_CALLS])(message *m_ptr) = {
#if USE_FORK
    [SYS_FORK] = do_fork,
#endif
    // ...
};
```

**效果**：
- `USE_FORK = 0`：`do_fork` 函数不编译，分发表中对应位置为 NULL
- `USE_FORK = 1`：`do_fork` 函数编译，分发表中对应位置指向函数

## 1.5 与现代 OS 的对比

| 方面 | Minix3 | Linux | Rust OS |
|------|--------|-------|---------|
| **配置方式** | 宏定义 | Kconfig + .config | Cargo features |
| **粒度** | 系统调用级 | 子系统级 | 模块级 |
| **依赖管理** | 手动 | 自动 | 自动 |
| **条件编译** | #if | #ifdef CONFIG_X | #[cfg(feature = "x")] |

### Rust 的 Cargo Features

```toml
# Cargo.toml
[features]
default = ["fork", "exec"]
fork = []
exec = []
syscall-trace = ["fork", "exec"]
```

```rust
// lib.rs
#[cfg(feature = "fork")]
pub fn do_fork() -> Result<Pid, Error> {
    // fork 实现
}

#[cfg(not(feature = "fork"))]
pub fn do_fork() -> Result<Pid, Error> {
    Err(Error::NotSupported)
}
```

**优势**：
- 类型安全：编译期检查所有代码路径
- 依赖管理：自动处理 feature 依赖
- 文档生成：自动生成 feature 文档

## 1.6 要点速查表

| 配置项 | 默认值 | 作用 |
|--------|--------|------|
| `USE_FORK` | 1 | 启用 fork 系统调用 |
| `USE_EXEC` | 1 | 启用 exec 系统调用 |
| `USE_EXIT` | 1 | 启用 exit 系统调用 |
| `NR_IRQ_HOOKS` | 16/64 | 中断钩子数量 |
| `VDEVIO_BUF_SIZE` | 64 | 向量 I/O 缓冲区大小 |

## 1.7 灾难预演

### 如果禁用了关键系统调用

```
假设 USE_FORK = 0

后果：
1. fork() 系统调用返回 ENOSYS
2. shell 无法创建子进程
3. 系统无法启动新进程
4. 系统基本不可用
```

### 如果 NR_IRQ_HOOKS 设置过小

```
假设 NR_IRQ_HOOKS = 4

后果：
1. 只能注册 4 个中断处理程序
2. 驱动程序注册失败
3. 硬件设备无法工作
4. 系统功能受限
```

### 如果 NR_IRQ_HOOKS 设置过大

```
假设 NR_IRQ_HOOKS = 1024

后果：
1. 静态分配过多内存
2. 内核内存占用增加
3. 大部分空间浪费
```

## 1.8 互动自测

1. 为什么 Minix3 使用编译时配置而不是运行时配置？
2. 系统调用开关如何影响内核大小？
3. 如何确定 NR_IRQ_HOOKS 的合理值？
4. Rust 的 feature flags 如何帮助减小二进制大小？
5. 如何在现代硬件上自动检测中断钩子数量？

---

# 二、main.c 总结

> **文件位置**: `kernel/main.c`
> **核心功能**: 内核主函数、启动流程、关机流程

## 2.1 文件概述

main.c 是 Minix3 内核的核心入口文件，包含：

```
┌─────────────────────────────────────────────────────────────────┐
│                    main.c 的核心职责                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ 1. 内核初始化：                                                  │
│    - kmain(): 内核主函数                                        │
│    - cstart(): C 运行时启动                                     │
│    - bsp_finish_booting(): 完成启动                             │
│                                                                 │
│ 2. 进程管理：                                                    │
│    - 初始化进程表                                                │
│    - 设置启动进程特权                                            │
│    - 调度启动进程                                                │
│                                                                 │
│ 3. 系统控制：                                                    │
│    - announce(): 打印启动横幅                                   │
│    - prepare_shutdown(): 准备关机                               │
│    - minix_shutdown(): 执行关机                                 │
│                                                                 │
│ 4. 辅助函数：                                                    │
│    - get_value(): 解析引导参数                                  │
│    - env_get(): 内核版 getenv                                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 2.2 启动流程详解

### 2.2.1 完整启动流程

```
┌─────────────────────────────────────────────────────────────────┐
│                    Minix3 内核启动流程                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ 1. 汇编入口 (head.S)                                            │
│    ↓                                                            │
│ 2. pre_init() - 早期初始化                                      │
│    ↓                                                            │
│ 3. kmain() - 内核主函数                                         │
│    ├── BSS 段检查                                               │
│    ├── 保存引导参数                                             │
│    ├── cstart() - C 运行时启动                                  │
│    │   ├── prot_init() - 保护模式初始化                         │
│    │   ├── init_clock() - 时钟初始化                            │
│    │   └── intr_init() - 中断初始化                             │
│    ├── proc_init() - 进程表初始化                               │
│    ├── 设置启动进程特权                                         │
│    ├── memory_init() - 内存初始化                               │
│    ├── system_init() - 系统调用初始化                           │
│    └── bsp_finish_booting() - 完成启动                          │
│        ├── cpu_identify() - CPU 识别                            │
│        ├── announce() - 打印启动横幅                            │
│        ├── 启动引导进程                                         │
│        ├── 时钟初始化                                           │
│        ├── FPU 初始化                                           │
│        └── switch_to_user() - 切换到用户态                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2.2 进程启动顺序

```
┌─────────────────────────────────────────────────────────────────┐
│                    进程启动顺序                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ 第一阶段：内核任务（立即可运行）                                 │
│   ├── IDLE (空闲进程)                                           │
│   ├── CLOCK (时钟任务)                                          │
│   └── SYSTEM (系统任务)                                         │
│                                                                 │
│ 第二阶段：VM（虚拟内存管理器）                                   │
│   - VM 没有 RTS_VMINHIBIT 标志                                  │
│   - VM 可以最先运行                                             │
│   - VM 为其他进程设置页表                                       │
│                                                                 │
│ 第三阶段：其他系统服务                                           │
│   ├── RS (重生服务器) - 设置其他进程特权                        │
│   ├── PM (进程管理器)                                           │
│   ├── VFS (虚拟文件系统)                                        │
│   └── 其他服务                                                   │
│                                                                 │
│ 第四阶段：用户进程                                               │
│   └── INIT (初始化进程) - 启动用户会话                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 2.3 关键函数详解

### kmain()

```c
void kmain(void)
{
    /* 1. BSS 段检查 */
    if (BSS_END != 0) {
        panic("BSS not cleared");
    }
    
    /* 2. 保存引导参数 */
    save_boot_params();
    
    /* 3. C 运行时启动 */
    cstart();
    
    /* 4. 进程表初始化 */
    proc_init();
    
    /* 5. 设置启动进程特权 */
    for (rp = BEG_PROC_ADDR; rp < END_PROC_ADDR; rp++) {
        priv(rp)->s_flags = boot_image[...].flags;
        // ...
    }
    
    /* 6. 内存初始化 */
    memory_init();
    
    /* 7. 系统调用初始化 */
    system_init();
    
    /* 8. 完成启动 */
    bsp_finish_booting();
}
```

### bsp_finish_booting()

```c
static void bsp_finish_booting(void)
{
    /* 1. CPU 识别 */
    cpu_identify();
    
    /* 2. 打印启动横幅 */
    announce();
    
    /* 3. 启动引导进程 */
    for (rp = BEG_PROC_ADDR; rp < END_PROC_ADDR; rp++) {
        if (rp->p_rts_flags == 0) {
            RTS_UNSET(rp, RTS_BOOTINHIBIT);
        }
    }
    
    /* 4. 时钟初始化 */
    boot_cpu_init_timer(system_hz);
    
    /* 5. FPU 初始化 */
    fpu_init();
    
    /* 6. 切换到用户态 */
    switch_to_user();
}
```

## 2.4 关机流程详解

### prepare_shutdown()

```c
void prepare_shutdown(int how)
{
    /* 1. 通知所有进程 */
    for (rp = BEG_PROC_ADDR; rp < END_PROC_ADDR; rp++) {
        if (rp->p_rts_flags == 0) {
            cause_sig(rp, SIGTERM);
        }
    }
    
    /* 2. 等待进程退出 */
    // ...
    
    /* 3. 执行关机 */
    minix_shutdown(how);
}
```

### minix_shutdown()

```c
void minix_shutdown(unsigned int how)
{
    /* 1. 停止时钟 */
    stop_local_timer();
    
    /* 2. 禁用中断 */
    intr_disable();
    
    /* 3. 执行架构关机 */
    arch_shutdown(how);
}
```

## 2.5 与 IPC 的关系

### 启动期间的 IPC 设置

```
┌─────────────────────────────────────────────────────────────────┐
│                    启动期间的 IPC 设置                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ 1. 进程表初始化 (proc_init):                                    │
│    - 初始化 p_sendmsg, p_delivermsg 缓冲区                     │
│    - 设置 p_getfrom_e, p_sendto_e 为 NONE                      │
│    - 清空 p_caller_q 发送者队列                                 │
│                                                                 │
│ 2. 特权设置 (kmain):                                            │
│    - 设置 s_ipc_to 位图（允许 IPC 的目标）                      │
│    - 设置 s_trap_mask（允许的系统调用）                         │
│    - 设置 s_k_call_mask（允许的内核调用）                       │
│                                                                 │
│ 3. 启动进程 (bsp_finish_booting):                               │
│    - 清除 RTS_BOOTINHIBIT 标志                                  │
│    - 进程变为可运行                                             │
│    - 开始 IPC 通信                                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 2.6 要点速查表

| 函数 | 作用 | 调用时机 |
|------|------|----------|
| `kmain()` | 内核主函数 | 启动时 |
| `cstart()` | C 运行时启动 | kmain 内部 |
| `bsp_finish_booting()` | 完成启动 | kmain 最后 |
| `announce()` | 打印启动横幅 | bsp_finish_booting 内部 |
| `prepare_shutdown()` | 准备关机 | 关机时 |
| `minix_shutdown()` | 执行关机 | prepare_shutdown 内部 |

## 2.7 灾难预演

### 如果 BSS 段未清零

```
后果：
1. 全局变量初始值不确定
2. 内核行为不可预测
3. 可能导致崩溃或安全漏洞
```

### 如果进程启动顺序错误

```
假设 VM 在 RS 之前运行

后果：
1. VM 需要特权设置，但 RS 还没运行
2. VM 无法正常工作
3. 后续进程无法获得页表
4. 系统启动失败
```

### 如果关机时未通知进程

```
后果：
1. 进程未保存数据
2. 文件系统损坏
3. 数据丢失
```

## 2.8 互动自测

1. 为什么 VM 要在其他系统服务之前运行？
2. `RTS_BOOTINHIBIT` 标志的作用是什么？
3. 关机流程为什么要先通知进程再停止？
4. 如何确保进程启动顺序正确？
5. 如果某个进程启动失败，系统如何处理？

---

**文档版本**: 2026-03-30
**涵盖文件**: kernel/config.h, kernel/main.c
