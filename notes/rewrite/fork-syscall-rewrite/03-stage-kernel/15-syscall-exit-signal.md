# 15-syscall-exit-signal: sys_exit / sys_kill 与信号处理

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/system.c`: sys_exit / sys_kill + sig_delay_done(454) + cause_sig(389)
> **说明**: 进程退出、信号发送与分发——kernel 负责机制（信号传递），PM 负责策略（信号处理）

---

## 1. 概述

### 1.1 概念定义/作用

**进程退出与信号处理**是 Minix3 内核中进程生命周期管理和异步事件通知的核心机制。内核负责信号传递的**机制**（将信号标记为待处理、通知信号管理器、设置信号处理帧），PM 负责信号处理的**策略**（决定如何处理信号——终止、忽略、捕获）。

**sys_exit**：系统进程请求退出。内核向调用方自身发送 `SIGABRT` 信号，触发正常的信号处理流程。

**sys_kill**：向指定进程发送信号。内核将信号标记为待处理，并通知该进程的信号管理器。

**信号处理流程**涉及四个内核调用：
- `sys_getksig`：信号管理器获取下一个待处理信号
- `sys_sigsend`：将信号处理帧推送到目标进程的用户栈
- `sys_sigreturn`：信号处理函数返回后恢复原始上下文
- `sys_endksig`：信号管理器完成一个信号的处理

**sys_clear**：PM 在进程退出后清理内核进程表槽位。释放地址空间、取消 IRQ 钩子、清除端点、释放特权结构。

### 1.2 与 Minix3 的对应关系

| 功能 | 函数 | 源文件 |
|------|------|--------|
| 系统进程退出 | `do_exit()` | system/do_exit.c |
| 发送信号 | `do_kill()` | system/do_kill.c |
| 信号分发核心 | `cause_sig()` | system.c:389 |
| 信号延迟完成 | `sig_delay_done()` | system.c:454 |
| 获取待处理信号 | `do_getksig()` | system/do_getksig.c |
| 结束信号处理 | `do_endksig()` | system/do_endksig.c |
| 推送信号处理帧 | `do_sigsend()` | system/do_sigsend.c |
| 信号处理返回 | `do_sigreturn()` | system/do_sigreturn.c |
| 清理进程槽位 | `do_clear()` | system/do_clear.c |

### 1.3 关键状态/机制说明

**信号管理器（signal manager）**：每个进程有一个关联的信号管理器（`s_sig_mgr`），负责处理该进程的信号。用户进程的信号管理器是 PM，系统进程可以有自定义的信号管理器。当信号产生时，内核通知信号管理器，由信号管理器决定如何处理。

**RTS_SIGNALED / RTS_SIG_PENDING**：当进程有待处理信号时，内核设置 `RTS_SIGNALED`（标记有信号）和 `RTS_SIG_PENDING`（标记信号正在处理中）。`RTS_SIG_PENDING` 使进程保持不可运行状态，直到信号管理器完成处理（`sys_endksig` 清除）。

**信号处理帧（sigframe）**：`sys_sigsend` 在目标进程的用户栈上构建一个信号处理帧，包含当前寄存器上下文的快照和信号处理函数的入口信息。进程恢复执行时跳转到信号处理函数，处理完成后通过 `sys_sigreturn` 从帧中恢复原始上下文。

**致命信号的备份管理器**：若进程是自己的信号管理器且收到致命信号，内核尝试将信号转发给备份信号管理器（`s_bak_sig_mgr`）。若无备份管理器，内核 panic。

### 1.4 行为规则

1. **sys_exit 不回复**：`do_exit()` 返回 `EDONTREPLY`，调用方不会收到回复
2. **sys_exit 发送 SIGABRT**：系统进程退出通过向自身发送 SIGABRT 实现，走正常信号处理流程
3. **信号不可发给内核任务**：`do_kill()` 拒绝向内核任务发送信号（`iskerneln` 检查）
4. **信号去重**：`cause_sig()` 检查信号是否已在 `p_pending` 中，避免重复通知信号管理器
5. **RTS_SIGNALED 与 RTS_SIG_PENDING 配合**：`RTS_SIGNALED` 标记有信号待处理，`RTS_SIG_PENDING` 阻塞进程直到信号管理器完成
6. **sys_sigsend 的幂等性**：信号处理帧复制可能因页缺失失败（VMSUSPEND），代码设计为可安全重入
7. **sys_sigreturn 验证 sc_magic**：恢复上下文前检查 `SC_MAGIC`，防止损坏的信号上下文
8. **sys_clear 释放全部资源**：清理地址空间、IRQ 钩子、端点、定时器、FPU、特权结构

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 信号相关消息字段

| 字段宏 | 含义 |
|--------|------|
| `m_sigcalls.endpt` | 目标进程 endpoint |
| `m_sigcalls.sig` | 信号编号 |
| `m_sigcalls.map` | 待处理信号位图 |
| `m_sigcalls.sigctx` | sigcontext 结构指针 |

#### 2.1.2 信号相关 RTS 标志

| 标志 | 值 | 信号含义 |
|------|-----|---------|
| `RTS_SIGNALED` | 0x10 | 进程有待处理的内核信号 |
| `RTS_SIG_PENDING` | 0x20 | 信号正在被信号管理器处理，进程不可运行 |

#### 2.1.3 信号相关 MISC_FLAGS

| 标志 | 值 | 信号含义 |
|------|-----|---------|
| `MF_SIG_DELAY` | 0x080 | 进程发送完成后需发送信号 |
| `MF_CONTEXT_SET` | 0x4000 | 上下文已设置（ARM） |

#### 2.1.4 特殊信号值

| 常量 | 含义 |
|------|------|
| `SIGKSIG` | 内核通知信号管理器有新信号 |
| `SIGKSIGSM` | 信号管理器自身的信号通知 |
| `SIGSNDELAY` | 停止延迟结束信号 |
| `SC_MAGIC` | sigcontext 结构的魔数（校验完整性） |

### 2.2 核心数据结构

#### 2.2.1 信号处理帧（struct sigframe_sigcontext）

`sys_sigsend` 在目标进程用户栈上构建的帧结构：

| 字段 | 含义 |
|------|------|
| `sf_sc` | `struct sigcontext`——保存的寄存器上下文 |
| `sf_scp` | 指向 `sf_sc` 的指针 |
| `sf_fp` | 帧指针 |
| `sf_signum` | 信号编号 |
| `sf_ra` | 返回地址（原始 PC） |
| `sf_ra_sigreturn` | sigreturn 库函数地址 |
| `sf_scpcopy` | sigcontext 指针副本 |

#### 2.2.2 信号上下文（struct sigcontext）

保存在信号处理帧中的寄存器快照：

| 字段（x86） | 含义 |
|-------------|------|
| `sc_gs/fs/es/ds` | 段寄存器 |
| `sc_edi/esi/ebp/ebx/edx/ecx/eax` | 通用寄存器 |
| `sc_eip` | 原始程序计数器 |
| `sc_eflags` | 原始标志寄存器 |
| `sc_esp` | 原始栈指针 |
| `sc_cs/sc_ss` | 代码/栈段 |
| `sc_mask` | 信号掩码 |
| `sc_flags` | FPU 初始化标志 |
| `sc_fpu_state` | FPU 状态 |
| `sc_magic` | 魔数（SC_MAGIC） |
| `trap_style` | 陷阱风格 |

### 2.3 关键函数分析

#### 2.3.1 do_exit()——系统进程退出

`minix3/minix/kernel/system/do_exit.c:14-24`

```c
int do_exit(struct proc *caller, message *m_ptr)
```

**功能**：系统进程请求退出。

**行为**：调用 `cause_sig(caller->p_nr, SIGABRT)` 向调用方自身发送 SIGABRT 信号，返回 `EDONTREPLY`。

**设计**：系统进程退出不是直接终止，而是通过信号机制。SIGABRT 是致命信号，信号管理器（通常是 PM）收到后会执行进程清理流程。这确保了系统进程退出走统一的信号处理路径。

#### 2.3.2 do_kill()——发送信号

`minix3/minix/kernel/system/do_kill.c:17-38`

```c
int do_kill(struct proc *caller, message *m_ptr)
```

**功能**：向指定进程发送信号。

**行为**：
1. 验证目标进程 endpoint（`isokendpt`）
2. 验证信号编号（`sig_nr < _NSIG`）
3. 拒绝向内核任务发送信号（`iskerneln` → `EPERM`）
4. 调用 `cause_sig(proc_nr, sig_nr)` 分发信号

#### 2.3.3 cause_sig()——信号分发核心

`minix3/minix/kernel/system.c:389-449`

```c
void cause_sig(proc_nr_t proc_nr, int sig_nr)
```

**功能**：将信号标记为待处理并通知信号管理器。

**行为**（两条路径）：

**路径 A——目标是自身的信号管理器**（`rp->p_endpoint == sig_mgr`）：
1. 若信号是致命的（`SIGS_IS_LETHAL`），尝试转发给备份信号管理器（`s_bak_sig_mgr`）
2. 若无备份管理器，panic
3. 若信号非致命，添加到 `s_sig_pending`，通过 `send_sig()` 发送 `SIGKSIGSM` 通知

**路径 B——目标有独立信号管理器**：
1. 检查信号是否已在 `p_pending` 中（去重）
2. 若信号未待处理：`sigaddset(&rp->p_pending, sig_nr)`
3. 若 `RTS_SIGNALED` 未设置：`RTS_SET(rp, RTS_SIGNALED | RTS_SIG_PENDING)`，通过 `send_sig()` 通知信号管理器

**关键设计**：`RTS_SIGNALED` 和 `RTS_SIG_PENDING` 同时设置。`RTS_SIGNALED` 标记"有信号"，`RTS_SIG_PENDING` 阻塞进程。信号管理器通过 `sys_getksig` 清除 `RTS_SIGNALED`，通过 `sys_endksig` 清除 `RTS_SIG_PENDING`。

#### 2.3.4 do_getksig()——获取待处理信号

`minix3/minix/kernel/system/do_getksig.c:18-42`

```c
int do_getksig(struct proc *caller, message *m_ptr)
```

**功能**：信号管理器获取下一个待处理的信号。

**行为**：
1. 遍历所有用户进程，找到第一个 `RTS_SIGNALED` 置位且信号管理器是调用方的进程
2. 返回进程 endpoint 和待处理信号位图
3. 清除进程的 `p_pending` 和 `RTS_SIGNALED`
4. 若无待处理信号，返回 `endpt = NONE`

#### 2.3.5 do_sigsend()——推送信号处理帧

`minix3/minix/kernel/system/do_sigsend.c:19-163`

```c
int do_sigsend(struct proc *caller, message *m_ptr)
```

**功能**：在目标进程的用户栈上构建信号处理帧，使进程恢复执行时跳转到信号处理函数。

**行为**：
1. 从调用方地址空间复制 `sigmsg` 结构
2. 计算用户栈上的帧位置：`frp = sp - sizeof(sigframe)`
3. 保存当前寄存器上下文到 `sigcontext`
4. 保存 FPU 状态（若进程使用过 FPU）
5. 将 `sigframe` 复制到用户栈（`data_copy_vmcheck`，可能 VMSUSPEND）
6. **修改进程寄存器**：`pc = sighandler`, `sp = frp`（必须在帧复制成功后执行）
7. 清除 `MF_FPU_INITIALIZED`

**幂等性设计**：帧复制可能因页缺失失败，导致函数重入。因此寄存器修改必须在帧复制成功后执行——否则重入时寄存器会被多次修改导致状态损坏。

#### 2.3.6 do_sigreturn()——信号处理返回

`minix3/minix/kernel/system/do_sigreturn.c:19-96`

```c
int do_sigreturn(struct proc *caller, message *m_ptr)
```

**功能**：信号处理函数返回后恢复原始上下文。

**行为**：
1. 从目标进程地址空间复制 `sigcontext` 结构
2. 恢复通用寄存器（x86 下仅恢复非段寄存器）
3. 恢复标志寄存器（仅用户位，保留系统位）
4. 调用 `arch_proc_setcontext()` 设置完整上下文
5. 验证 `sc_magic == SC_MAGIC`
6. 若 `sc_flags & MF_FPU_INITIALIZED`，恢复 FPU 状态

#### 2.3.7 do_endksig()——结束信号处理

`minix3/minix/kernel/system/do_endksig.c:15-38`

```c
int do_endksig(struct proc *caller, message *m_ptr)
```

**功能**：信号管理器完成一个信号的处理。

**行为**：
1. 验证调用方是目标进程的信号管理器
2. 验证 `RTS_SIG_PENDING` 置位
3. 若 `RTS_SIGNALED` 未设置（无新信号到达），清除 `RTS_SIG_PENDING`

**关键**：若在信号处理期间有新信号到达（`RTS_SIGNALED` 重新设置），`RTS_SIG_PENDING` 不被清除——进程保持阻塞，等待信号管理器处理新信号。

#### 2.3.8 do_clear()——清理进程槽位

`minix3/minix/kernel/system/do_clear.c:17-78`

```c
int do_clear(struct proc *caller, message *m_ptr)
```

**功能**：进程退出后清理内核进程表槽位。

**行为**：
1. 释放地址空间：`release_address_space(rc)`
2. 若槽位已空闲，直接返回
3. 释放 IRQ 钩子：遍历 `irq_hooks[]`，移除属于该进程的钩子
4. 清除端点：`clear_endpoint(rc)`（取消所有 IPC 关联）
5. 重置闹钟定时器：`reset_kernel_timer(&priv(rc)->s_alarm_timer)`
6. 标记槽位空闲：`RTS_SETFLAGS(rc, RTS_SLOT_FREE)`
7. 释放 FPU：`release_fpu(rc)`，清除 `MF_FPU_INITIALIZED`
8. 释放特权结构：若为系统进程，`priv(rc)->s_proc_nr = NONE`

### 2.4 调用关系/调用点分析

#### 2.4.1 信号处理完整路径

```
信号产生（内核异常/系统调用/硬件中断）
  └─ cause_sig(proc_nr, sig_nr)
       ├─ sigaddset(&rp->p_pending, sig_nr)
       ├─ RTS_SET(RTS_SIGNALED | RTS_SIG_PENDING)
       └─ send_sig(sig_mgr, SIGKSIG) 通知信号管理器

信号管理器（PM）处理信号
  ├─ sys_getksig() → 获取待处理信号
  │    └─ 清除 RTS_SIGNALED
  ├─ PM 决定如何处理信号（终止/忽略/捕获）
  ├─ [捕获?]
  │    └─ sys_sigsend() → 推送信号处理帧
  │         ├─ 保存寄存器到 sigcontext
  │         ├─ 构建信号处理帧到用户栈
  │         └─ 修改 PC = sighandler, SP = frame
  ├─ [信号处理函数返回]
  │    └─ sys_sigreturn() → 恢复原始上下文
  └─ sys_endksig() → 结束信号处理
       └─ [无新信号?] → 清除 RTS_SIG_PENDING
```

#### 2.4.2 进程退出路径

```
用户进程调用 exit()
  └─ PM 处理退出请求
       ├─ PM 清理用户空间资源
       └─ sys_clear(proc_ep)
            ├─ release_address_space()
            ├─ 释放 IRQ 钩子
            ├─ clear_endpoint()
            ├─ reset_kernel_timer()
            ├─ RTS_SETFLAGS(RTS_SLOT_FREE)
            ├─ release_fpu()
            └─ [系统进程?] priv->s_proc_nr = NONE

系统进程调用 sys_exit()
  └─ do_exit()
       └─ cause_sig(caller, SIGABRT)
            └─ PM 收到信号 → 执行清理流程
```

### 2.5 设计要点/特殊处理

#### 2.5.1 内核负责机制，PM 负责策略

Minix3 的信号处理严格分离机制和策略：

- **内核（机制）**：标记信号待处理、通知信号管理器、构建信号处理帧、恢复上下文
- **PM（策略）**：决定如何处理信号（终止、忽略、捕获）、选择信号处理函数

内核不知道信号的含义，只知道如何传递信号。PM 根据信号类型和进程的信号处置决定处理方式。

#### 2.5.2 cause_sig 的去重机制

`cause_sig()` 在添加信号前检查 `sigismember(&rp->p_pending, sig_nr)`。若信号已在位图中，不重复添加也不重复通知信号管理器。这避免了同一信号被多次投递，但代价是标准信号（非实时信号）可能丢失——后到的同号信号被去重丢弃。

#### 2.5.3 致命信号的备份管理器

若进程是自己的信号管理器且收到致命信号，正常流程会导致死锁（信号管理器无法处理自己的致命信号）。`cause_sig()` 通过备份信号管理器机制解决：先尝试将信号转发给 `s_bak_sig_mgr`，若无备份则 panic。

#### 2.5.4 sys_sigsend 的幂等性

`do_sigsend()` 中帧复制可能因页缺失返回 `VMSUSPEND`，导致函数重入。代码设计为：所有寄存器修改在帧复制成功后执行。这样即使函数重入，寄存器也只被修改一次。

#### 2.5.5 sys_sigreturn 的安全恢复

`do_sigreturn()` 恢复标志寄存器时，仅恢复用户位（`X86_FLAGS_USER`），保留系统位。这防止用户进程通过 sigreturn 修改特权标志（如 IF 中断使能标志）。

#### 2.5.6 do_clear 的全面清理

`do_clear()` 清理进程的所有内核资源：地址空间、IRQ 钩子、端点关联、定时器、FPU、特权结构。这确保了进程退出后不会遗留任何内核状态。特权结构的释放（`s_proc_nr = NONE`）使得该特权 ID 可被新进程重用。
