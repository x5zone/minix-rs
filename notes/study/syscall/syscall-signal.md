# 信号处理系统调用

> **模块概述**: 本模块涵盖 Minix3 内核中的信号处理机制，包括信号的发送、捕获和返回。
> 
> **涉及文件**:
> - [do_sigsend.c](../../../minix3/minix/kernel/system/do_sigsend.c) - 设置信号处理上下文
> - [do_getksig.c](../../../minix3/minix/kernel/system/do_getksig.c) - 获取待处理信号
> - [do_endksig.c](../../../minix3/minix/kernel/system/do_endksig.c) - 结束信号处理
> - [do_sigreturn.c](../../../minix3/minix/kernel/system/do_sigreturn.c) - 从信号处理返回

---

## 模块架构总览

### Minix3 信号机制的设计哲学

Minix3 的信号处理机制体现了微内核架构的核心思想：**内核只负责上下文切换，信号管理策略在用户态实现**。

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Minix3 信号处理架构 vs 传统 Unix                                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  传统 Unix (宏内核):                                                     │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  内核空间                                                        │  │
│  │  ┌────────────────────────────────────────────────────────────┐ │  │
│  │  │  信号产生 → 信号管理 → 信号发送 → 信号处理返回             │ │  │
│  │  │      ↑_______________全部在内核________________↓            │ │  │
│  │  └────────────────────────────────────────────────────────────┘ │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  Minix3 (微内核):                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  内核空间                                                        │  │
│  │  ┌────────────────────────────────────────────────────────────┐ │  │
│  │  │  信号产生 → 上下文保存/恢复 → 信号返回                     │ │  │
│  │  │      ↑________最小化内核职责________↑                      │ │  │
│  │  └────────────────────────────────────────────────────────────┘ │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  用户空间 (PM - 进程管理器)                                      │  │
│  │  ┌────────────────────────────────────────────────────────────┐ │  │
│  │  │  信号管理策略：                                             │ │  │
│  │  │  - 检查信号权限                                             │ │  │
│  │  │  - 查找信号处理函数                                         │ │  │
│  │  │  - 构造信号消息                                             │ │  │
│  │  │  - 协调信号处理流程                                         │ │  │
│  │  └────────────────────────────────────────────────────────────┘ │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  优势:                                                                  │
│  ✓ 内核更小、更安全                                                     │
│  ✓ 信号策略可定制                                                       │
│  ✓ 易于扩展和调试                                                       │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 信号处理的四个阶段

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号处理的四个阶段                                                      │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  阶段一: 信号产生 (内核)                                                 │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  触发源: kill() 系统调用、硬件异常、定时器到期                    │  │
│  │  内核动作: 设置 p_pending 位图，RTS_SIGNALED 标志                │  │
│  │  数据结构: p_pending |= (1 << (sig - 1))                         │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  阶段二: 信号获取 (PM ← 内核)                                            │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  PM 动作: 调用 sys_getksig() 获取待处理信号                      │  │
│  │  内核动作: 返回进程端点和信号位图                                 │  │
│  │  状态转换: RTS_SIGNALED → RTS_SIG_PENDING                       │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  阶段三: 信号发送 (PM → 内核)                                            │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  PM 动作: 构造 sigmsg，调用 sys_sigsend()                        │  │
│  │  内核动作: 保存寄存器到用户栈，修改 PC 指向信号处理函数          │  │
│  │  数据结构: sigframe_sigcontext 保存到用户栈                     │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  阶段四: 信号返回 (PM → 内核)                                            │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  用户动作: 信号处理函数返回，调用 sigreturn()                    │  │
│  │  PM 动作: 转发为 sys_sigreturn()                                 │  │
│  │  内核动作: 从用户栈恢复寄存器，清除 RTS_SIG_PENDING             │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 系统调用关系图

```
                    ┌──────────────────────────────────────┐
                    │         信号产生源                   │
                    │  kill() / 异常 / 定时器              │
                    └──────────────┬───────────────────────┘
                                   │
                                   ▼
                    ┌──────────────────────────────────────┐
                    │      cause_sig() [内核]              │
                    │      设置 p_pending, RTS_SIGNALED    │
                    └──────────────┬───────────────────────┘
                                   │
                    ┌──────────────┴───────────────┐
                    │                              │
            ┌───────▼────────┐           ┌────────▼────────┐
            │  SYS_GETKSIG   │           │       PM        │
            │  (获取信号)     │◄──────────┤  (信号管理器)    │
            └───────┬────────┘           └────────┬────────┘
                    │                              │
                    │         返回信号信息         │
                    └──────────────┬───────────────┘
                                   │
                    ┌──────────────▼───────────────┐
                    │       PM 处理信号            │
                    │  - 查找处理函数              │
                    │  - 构造 sigmsg               │
                    └──────────────┬───────────────┘
                                   │
                    ┌──────────────▼───────────────┐
                    │      SYS_SIGSEND             │
                    │  (设置信号处理上下文)         │
                    └──────────────┬───────────────┘
                                   │
                    ┌──────────────▼───────────────┐
                    │      用户信号处理函数         │
                    └──────────────┬───────────────┘
                                   │
                    ┌──────────────▼───────────────┐
                    │      SYS_SIGRETURN           │
                    │  (恢复进程上下文)             │
                    └──────────────┬───────────────┘
                                   │
                    ┌──────────────▼───────────────┐
                    │      SYS_ENDKSIG             │
                    │  (结束信号处理)               │
                    └──────────────────────────────┘
```

---

## 信号产生与标记

### 内核中的信号标记机制

当信号产生时，内核并不立即处理，而是标记进程有待处理信号：

```c
/* 内核中信号标记的实现 */
void cause_sig(int proc_nr, int sig)
{
    struct proc *rp = proc_addr(proc_nr);
    
    /* 设置信号位图 */
    rp->p_pending |= (1 << (sig - 1));
    
    /* 标记进程有待处理信号 */
    RTS_SET(rp, RTS_SIGNALED);
}
```

**关键数据结构**:

```
进程控制块 (struct proc):
┌────────────────────────────────────────────────────────────────┐
│ struct proc {                                                  │
│     ...                                                        │
│     sigset_t p_pending;      /* 待处理信号位图 (4-8 字节) */   │
│     ...                                                        │
│     u64_t p_rts_flags;       /* 运行时状态标志 (8 字节) */     │
│         /* 包含: RTS_SIGNALED, RTS_SIG_PENDING */              │
│     ...                                                        │
│ }                                                              │
└────────────────────────────────────────────────────────────────┘

信号位图 (sigset_t):
┌────────────────────────────────────────────────────────────────┐
│  位 0: SIGHUP    位 1: SIGINT    位 2: SIGQUIT   位 3: SIGILL  │
│  位 4: SIGTRAP   位 5: SIGABRT   位 6: SIGBUS    位 7: SIGFPE  │
│  位 8: SIGKILL   位 9: SIGUSR1   ...                            │
│  ...                                                           │
│  最多 64 个信号 (取决于 sigset_t 大小)                         │
└────────────────────────────────────────────────────────────────┘

运行时状态标志 (p_rts_flags):
┌────────────────────────────────────────────────────────────────┐
│  RTS_SIGNALED:     进程有待处理信号                             │
│  RTS_SIG_PENDING:  PM 正在处理信号                              │
│  RTS_PROC_STOP:    进程已停止（用于调试）                       │
│  ...                                                           │
└────────────────────────────────────────────────────────────────┘
```

### 信号标记的内存布局

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号标记在进程控制块中的位置                                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  进程控制块 (struct proc):                                              │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  偏移 0x00: p_endpoint (4 字节)                                │    │
│  │  偏移 0x04: p_priority (4 字节)                                │    │
│  │  ...                                                           │    │
│  │  偏移 0x40: p_pending (8 字节) ← 信号位图                      │    │
│  │  偏移 0x48: p_rts_flags (8 字节) ← 运行时状态                  │    │
│  │  ...                                                           │    │
│  │  偏移 0x80: p_reg (用户寄存器)                                 │    │
│  │  ...                                                           │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  内存位置: 内核静态分配的进程表中                                       │
│  更新时机: cause_sig() 被调用时                                        │
│  检查时机: PM 调用 sys_getksig() 时                                   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 核心函数逐行分析

### do_sigsend() - 设置信号处理上下文

**源码位置**: [do_sigsend.c](../../../minix3/minix/kernel/system/do_sigsend.c)

#### 完整源码与逐行讲解

```c
/* 第 1-9 行：文件头注释 */
/* The kernel call that is implemented in this file:
 *	m_type: SYS_SIGSEND
 *
 * The parameters for this kernel call are:
 * 	m_sigcalls.endpt	# process to call signal handler
 *	m_sigcalls.sigctx	# pointer to sigcontext structure
 */
```

**逐行讲解**:
- **第 1-3 行**: 说明本文件实现 `SYS_SIGSEND` 系统调用
- **第 5-8 行**: 列出消息参数：
  - `endpt`: 目标进程端点号
  - `sigctx`: 指向 sigcontext 结构的指针（在 PM 的地址空间中）

```c
/* 第 10-13 行：头文件包含 */
#include "kernel/system.h"
#include <signal.h>
#include <string.h>
```

**逐行讲解**:
- **第 10 行**: 内核系统调用通用头文件
- **第 11 行**: 信号相关定义，如信号编号
- **第 12 行**: 字符串操作函数，如 `memset`、`memcpy`

```c
/* 第 14 行：条件编译 */
#if USE_SIGSEND
```

```c
/* 第 16-19 行：函数头注释 */
/*===========================================================================*
 *			      do_sigsend				     *
 *===========================================================================*/
int do_sigsend(struct proc * caller, message * m_ptr)
```

```c
/* 第 20-26 行：局部变量声明 */
/* Handle sys_sigsend, POSIX-style signal handling. */

  struct sigmsg smsg;
  register struct proc *rp;
  struct sigframe_sigcontext fr, *frp;
  int proc_nr, r;
#if defined(__i386__)
  reg_t new_fp;
#endif
```

**逐行讲解**:
- **第 20 行**: 注释说明函数功能：POSIX 风格信号处理
- **第 22 行**: `struct sigmsg smsg` - 信号消息结构（约 32 字节，栈上分配）
  - 包含：信号编号、信号处理函数地址、信号掩码等
- **第 23 行**: `register struct proc *rp` - 目标进程指针（8 字节）
- **第 24 行**: `struct sigframe_sigcontext fr` - 信号帧结构（约 512 字节，栈上分配）
  - `*frp` - 指向用户栈上信号帧的指针
- **第 25 行**: `int proc_nr, r` - 进程号和返回值
- **第 26-28 行**: x86 架构特有的变量
  - `new_fp` - 新的帧指针

**内存布局**:
```
栈帧 (do_sigsend):
┌─────────────────────────────────────┐
│ caller (参数)         [8 字节]      │
│ m_ptr (参数)          [8 字节]      │
│ smsg                  [32 字节]     │
│ rp                    [8 字节]      │
│ fr                    [512 字节]    │
│ frp                   [8 字节]      │
│ proc_nr               [4 字节]      │
│ r                     [4 字节]      │
│ new_fp (x86)          [8 字节]      │
└─────────────────────────────────────┘
总计：约 600 字节
```

```c
/* 第 28-31 行：参数验证 */
  if (!isokendpt(m_ptr->m_sigcalls.endpt, &proc_nr)) return EINVAL;
  if (iskerneln(proc_nr)) return EPERM;
  rp = proc_addr(proc_nr);
```

**逐行讲解**:
- **第 28 行**: 验证端点号并获取进程号
  - `isokendpt()`: 检查端点号是否有效
  - 如果无效，返回 `EINVAL`
- **第 29 行**: 检查是否为内核进程
  - `iskerneln()`: 检查进程号是否在内核进程范围内
  - 如果是内核进程，返回 `EPERM`（不允许向内核进程发送信号）
- **第 30 行**: 获取进程控制块指针

**设计动机**:
- 内核进程不处理信号，它们通过消息机制通信
- 用户进程才能有信号处理函数

```c
/* 第 33-37 行：拷贝 sigmsg 结构 */
  /* Get the sigmsg structure into our address space.  */
  if ((r = data_copy_vmcheck(caller, caller->p_endpoint,
		(vir_bytes)m_ptr->m_sigcalls.sigctx, KERNEL,
		(vir_bytes)&smsg, (phys_bytes) sizeof(struct sigmsg))) != OK)
	return r;
```

**逐行讲解**:
- **第 33 行**: 注释说明将 sigmsg 结构拷贝到内核地址空间
- **第 34-36 行**: 调用 `data_copy_vmcheck` 拷贝数据
  - `caller->p_endpoint`: 源进程（PM）
  - `m_ptr->m_sigcalls.sigctx`: 源地址（PM 的地址空间）
  - `KERNEL`: 目标进程（内核）
  - `&smsg`: 目标地址（内核栈）
  - `sizeof(struct sigmsg)`: 拷贝大小
- **第 37 行**: 如果拷贝失败，返回错误码

**data_copy_vmcheck 的特殊性**:
```c
/* data_copy_vmcheck 与 data_copy 的区别 */
int data_copy_vmcheck(
    struct proc *caller,        /* 调用进程 */
    endpoint_t src_endpt,       /* 源端点 */
    vir_bytes src_addr,         /* 源地址 */
    endpoint_t dst_endpt,       /* 目标端点 */
    vir_bytes dst_addr,         /* 目标地址 */
    phys_bytes size             /* 大小 */
);
```

**返回值**:
- `OK`: 拷贝成功
- `VMSUSPEND`: 目标内存被换出，需要等待换入
  - 此时系统调用会返回，稍后重试
  - **关键**: 如果在拷贝前修改了寄存器，重试时会导致寄存器被多次修改

**VMSUSPEND 的处理流程**:
```
PM 调用 sys_sigsend()
    │
    ├─> 内核: do_sigsend()
    │   └─> data_copy_vmcheck()
    │       └─> 发现目标内存被换出
    │           └─> 返回 VMSUSPEND
    │
    ├─> 内核: 保存当前执行状态
    │   └─> 标记进程等待 VM
    │
    ├─> VM: 换入目标内存
    │
    └─> 内核: 重试 sys_sigsend()
        └─> 再次执行 do_sigsend()
            └─> 如果在第一次执行时修改了寄存器，
                第二次执行会再次修改，导致错误
```

```c
/* 第 39-44 行：警告注释 */
  /* WARNING: the following code may be run more than once even for a single
   * signal delivery. Do not change registers here. See the comment below.
   */
```

**逐行讲解**:
- **第 39-42 行**: 多行警告注释
  - **关键警告**: 以下代码可能运行多次
  - **禁止**: 在此修改寄存器
  - **原因**: 参见下方的注释

```c
/* 第 46-48 行：计算用户栈指针 */
  /* Compute the user stack pointer where sigframe will start. */
  smsg.sm_stkptr = arch_get_sp(rp);
  frp = (struct sigframe_sigcontext *) smsg.sm_stkptr - 1;
```

**逐行讲解**:
- **第 46 行**: 注释说明计算信号帧在用户栈上的位置
- **第 47 行**: 获取进程的当前栈指针
  - `arch_get_sp(rp)`: 架构相关的函数，返回 `rp->p_reg.sp`
- **第 48 行**: 计算信号帧的地址
  - `smsg.sm_stkptr - 1`: 栈指针减 1 个 sigframe 大小
  - **原理**: 栈向下增长，新帧在当前栈顶下方

**栈布局**:
```
用户栈:
高地址
    │
    ├─────────────────────┐
    │  原有栈帧           │
    ├─────────────────────┤ ← 原始 SP (smsg.sm_stkptr)
    │  sigframe           │
    │  - sf_sc            │
    │  - sf_fp            │
    │  - sf_signum        │
    │  - ...              │
    ├─────────────────────┤ ← 新 SP (frp)
    │  信号处理函数栈帧   │
    │                     │
低地址
```

```c
/* 第 50-53 行：初始化 sigframe */
  /* Copy the registers to the sigcontext structure. */
  memset(&fr, 0, sizeof(fr));
  fr.sf_scp = &frp->sf_sc;
```

**逐行讲解**:
- **第 50 行**: 注释说明将寄存器拷贝到 sigcontext 结构
- **第 51 行**: 清零 sigframe 结构
  - `memset(&fr, 0, sizeof(fr))`: 将 fr 的所有字节设置为 0
- **第 52 行**: 设置 sigcontext 指针
  - `fr.sf_scp = &frp->sf_sc`: 指向用户栈上的 sigcontext

**为什么指向用户栈上的 sigcontext**:
```
信号处理函数需要访问 sigcontext:
  1. 信号处理函数参数：void handler(int sig, siginfo_t *info, void *ctx)
  2. ctx 参数指向用户栈上的 sigcontext
  3. 用户可以通过 ctx 修改寄存器（在 sigreturn 时恢复）
```

```c
/* 第 55-79 行：x86 架构寄存器保存 */
#if defined(__i386__)
  fr.sf_sc.sc_gs = rp->p_reg.gs;
  fr.sf_sc.sc_fs = rp->p_reg.fs;
  fr.sf_sc.sc_es = rp->p_reg.es;
  fr.sf_sc.sc_ds = rp->p_reg.ds;
  fr.sf_sc.sc_edi = rp->p_reg.di;
  fr.sf_sc.sc_esi = rp->p_reg.si;
  fr.sf_sc.sc_ebp = rp->p_reg.fp;
  fr.sf_sc.sc_ebx = rp->p_reg.bx;
  fr.sf_sc.sc_edx = rp->p_reg.dx;
  fr.sf_sc.sc_ecx = rp->p_reg.cx;
  fr.sf_sc.sc_eax = rp->p_reg.retreg;
  fr.sf_sc.sc_eip = rp->p_reg.pc;
  fr.sf_sc.sc_cs = rp->p_reg.cs;
  fr.sf_sc.sc_eflags = rp->p_reg.psw;
  fr.sf_sc.sc_esp = rp->p_reg.sp;
  fr.sf_sc.sc_ss = rp->p_reg.ss;
  fr.sf_fp = rp->p_reg.fp;
  fr.sf_signum = smsg.sm_signo;
  new_fp = (reg_t) &frp->sf_fp;
  fr.sf_scpcopy = fr.sf_scp;
  fr.sf_ra_sigreturn = smsg.sm_sigreturn;
  fr.sf_ra= rp->p_reg.pc;

  fr.sf_sc.trap_style = rp->p_seg.p_kern_trap_style;
```

**逐行讲解**:
- **第 55 行**: 条件编译，仅 x86 架构
- **第 56-71 行**: 保存所有寄存器到 sigcontext
  - 段寄存器：`gs`, `fs`, `es`, `ds`
  - 通用寄存器：`edi`, `esi`, `ebp`, `ebx`, `edx`, `ecx`, `eax`
  - 指令指针：`eip` (PC)
  - 段寄存器：`cs`, `ss`
  - 标志寄存器：`eflags` (PSW)
  - 栈指针：`esp`
- **第 72 行**: 保存帧指针到 sigframe
- **第 73 行**: 保存信号编号
- **第 74 行**: 计算新的帧指针
  - `&frp->sf_fp`: 指向用户栈上 sigframe 的 sf_fp 字段
- **第 75 行**: 保存 sigcontext 指针的副本
- **第 76 行**: 保存 sigreturn 函数地址
  - 信号处理函数返回时，会跳转到这个地址
- **第 77 行**: 保存返回地址
  - 原进程的 PC，用于信号处理完成后恢复
- **第 79 行**: 保存陷阱类型
  - 用于标识进程是如何进入内核的（系统调用、中断、异常）

**寄存器映射关系**:
```
x86 寄存器 → p_reg 字段 → sigcontext 字段
─────────────────────────────────────────────
GS         → gs          → sc_gs
FS         → fs          → sc_fs
ES         → es          → sc_es
DS         → ds          → sc_ds
EDI        → di          → sc_edi
ESI        → si          → sc_esi
EBP        → fp          → sc_ebp
EBX        → bx          → sc_ebx
EDX        → dx          → sc_edx
ECX        → cx          → sc_ecx
EAX        → retreg      → sc_eax
EIP        → pc          → sc_eip
CS         → cs          → sc_cs
EFLAGS     → psw         → sc_eflags
ESP        → sp          → sc_esp
SS         → ss          → sc_ss
```

```c
/* 第 81-85 行：检查陷阱类型 */
  if (fr.sf_sc.trap_style == KTS_NONE) {
  	printf("do_sigsend: sigsend an unsaved process\n");
	return EINVAL;
  }
```

**逐行讲解**:
- **第 81-84 行**: 检查陷阱类型
  - `KTS_NONE`: 表示进程没有保存上下文（未进入内核）
  - 如果是 `KTS_NONE`，说明进程状态不一致，返回错误

**陷阱类型定义**:
```c
/* kernel/proc.h */
#define KTS_NONE        0   /* 未保存上下文 */
#define KTS_INT         1   /* 硬件中断 */
#define KTS_SYSCALL     2   /* 系统调用 */
#define KTS_EXCEPTION   3   /* 异常 */
```

```c
/* 第 87-91 行：保存 FPU 状态 */
  if (proc_used_fpu(rp)) {
	/* save the FPU context before saving it to the sig context */
	save_fpu(rp);
	memcpy(&fr.sf_sc.sc_fpu_state, rp->p_seg.fpu_state, FPU_XFP_SIZE);
  }
```

**逐行讲解**:
- **第 87 行**: 检查进程是否使用过 FPU
  - `proc_used_fpu(rp)`: 检查 `rp->p_misc_flags & MF_FPU_INITIALIZED`
- **第 89 行**: 保存 FPU 状态到进程控制块
  - `save_fpu(rp)`: 执行 `fxsave` 指令
- **第 90 行**: 拷贝 FPU 状态到 sigcontext
  - `FPU_XFP_SIZE`: 512 字节（x86 XSAVE 区域大小）

**FPU 状态保存的必要性**:
```
场景：
  1. 进程使用 FPU 计算
  2. 信号到达，进入信号处理函数
  3. 信号处理函数也可能使用 FPU
  4. 信号处理完成后，需要恢复原来的 FPU 状态

如果不保存：
  信号处理函数会破坏原进程的 FPU 状态
```

```c
/* 第 93-114 行：ARM 架构寄存器保存 */
#if defined(__arm__)
  fr.sf_sc.sc_spsr = rp->p_reg.psr;
  fr.sf_sc.sc_r0 = rp->p_reg.retreg;
  fr.sf_sc.sc_r1 = rp->p_reg.r1;
  fr.sf_sc.sc_r2 = rp->p_reg.r2;
  fr.sf_sc.sc_r3 = rp->p_reg.r3;
  fr.sf_sc.sc_r4 = rp->p_reg.r4;
  fr.sf_sc.sc_r5 = rp->p_reg.r5;
  fr.sf_sc.sc_r6 = rp->p_reg.r6;
  fr.sf_sc.sc_r7 = rp->p_reg.r7;
  fr.sf_sc.sc_r8 = rp->p_reg.r8;
  fr.sf_sc.sc_r9 = rp->p_reg.r9;
  fr.sf_sc.sc_r10 = rp->p_reg.r10;
  fr.sf_sc.sc_r11 = rp->p_reg.fp;
  fr.sf_sc.sc_r12 = rp->p_reg.r12;
  fr.sf_sc.sc_usr_sp = rp->p_reg.sp;
  fr.sf_sc.sc_usr_lr = rp->p_reg.lr;
  fr.sf_sc.sc_svc_lr = 0;	/* ? */
  fr.sf_sc.sc_pc = rp->p_reg.pc;	/* R15 */
#endif
```

**逐行讲解**:
- **第 93 行**: 条件编译，仅 ARM 架构
- **第 94-112 行**: 保存 ARM 寄存器
  - `spsr`: 程序状态寄存器
  - `r0-r12`: 通用寄存器
  - `sp`: 栈指针
  - `lr`: 链接寄存器
  - `pc`: 程序计数器

```c
/* 第 116-119 行：完成 sigcontext 初始化 */
  /* Finish the sigcontext initialization. */
  fr.sf_sc.sc_mask = smsg.sm_mask;
  fr.sf_sc.sc_flags = rp->p_misc_flags & MF_FPU_INITIALIZED;
  fr.sf_sc.sc_magic = SC_MAGIC;
```

**逐行讲解**:
- **第 116 行**: 注释说明完成 sigcontext 初始化
- **第 117 行**: 保存信号掩码
  - `smsg.sm_mask`: PM 传递的信号掩码
  - 信号处理函数执行期间，这些信号被阻塞
- **第 118 行**: 保存 FPU 标志
  - 用于 sigreturn 时恢复 FPU 状态
- **第 119 行**: 设置魔数
  - `SC_MAGIC`: 用于验证 sigcontext 的完整性

**魔数的作用**:
```c
/* sigreturn 时检查魔数 */
if (sc.sc_magic != SC_MAGIC) {
    printf("kernel sigreturn: corrupt signal context\n");
}
```

```c
/* 第 121-122 行：初始化 sigframe */
  /* Initialize the sigframe structure. */
  fpu_sigcontext(rp, &fr, &fr.sf_sc);
```

**逐行讲解**:
- **第 121 行**: 注释说明初始化 sigframe 结构
- **第 122 行**: 架构相关的 FPU sigcontext 初始化
  - `fpu_sigcontext()`: 设置 FPU 状态的特定字段

```c
/* 第 124-129 行：拷贝 sigframe 到用户栈 */
  /* Copy the sigframe structure to the user's stack. */
  if ((r = data_copy_vmcheck(caller, KERNEL, (vir_bytes)&fr,
		m_ptr->m_sigcalls.endpt, (vir_bytes)frp,
		(vir_bytes)sizeof(struct sigframe_sigcontext))) != OK)
      return r;
```

**逐行讲解**:
- **第 124 行**: 注释说明拷贝 sigframe 到用户栈
- **第 125-128 行**: 调用 `data_copy_vmcheck` 拷贝
  - `KERNEL`: 源进程（内核）
  - `&fr`: 源地址（内核栈）
  - `m_ptr->m_sigcalls.endpt`: 目标进程（用户进程）
  - `frp`: 目标地址（用户栈）
  - `sizeof(struct sigframe_sigcontext)`: 大小
- **第 129 行**: 如果拷贝失败，返回错误码

**关键点**: 这是最后一个可能返回 `VMSUSPEND` 的操作。此后代码可以安全地修改寄存器。

```c
/* 第 131-136 行：警告注释 */
  /* WARNING: up to the statement above, the code may run multiple times, since
   * copying out the frame/context may fail with VMSUSPEND the first time. For
   * that reason, changes to process registers *MUST* be deferred until after
   * this last copy -- otherwise, these changes will be made several times,
   * possibly leading to corrupted process state.
   */
```

**逐行讲解**:
- **第 131-135 行**: 多行警告注释
  - **关键**: 直到上面的语句，代码可能运行多次
  - **原因**: `data_copy_vmcheck` 可能返回 `VMSUSPEND`
  - **要求**: 修改进程寄存器必须延迟到最后一次拷贝之后
  - **后果**: 否则寄存器会被多次修改，破坏进程状态

```c
/* 第 138-140 行：修改进程寄存器 */
  /* Reset user registers to execute the signal handler. */
  rp->p_reg.sp = (reg_t) frp;
  rp->p_reg.pc = (reg_t) smsg.sm_sighandler;
```

**逐行讲解**:
- **第 138 行**: 注释说明重置用户寄存器以执行信号处理函数
- **第 139 行**: 设置新的栈指针
  - `frp`: 指向用户栈上的 sigframe
- **第 140 行**: 设置新的程序计数器
  - `smsg.sm_sighandler`: 信号处理函数地址

**修改寄存器的效果**:
```
进程恢复执行时：
  1. SP 指向 sigframe
  2. PC 指向信号处理函数
  3. 执行信号处理函数
  4. 信号处理函数返回时，跳转到 sigreturn
```

```c
/* 第 142-157 行：x86 架构特定设置 */
#if defined(__i386__)
  rp->p_reg.fp = new_fp;
#elif defined(__arm__)
  /* use the ARM link register to set the return address from the signal
   * handler
   */
  rp->p_reg.lr = (reg_t) smsg.sm_sigreturn;
  if(rp->p_reg.lr & 1) { printf("sigsend: LSB LR makes no sense.\n"); }

  /* pass signal handler parameters in registers */
  rp->p_reg.retreg = (reg_t) smsg.sm_signo;
  rp->p_reg.r1 = 0;	/* sf_code */
  rp->p_reg.r2 = (reg_t) fr.sf_scp;
  rp->p_misc_flags |= MF_CONTEXT_SET;
#endif
```

**逐行讲解**:
- **第 142-143 行**: x86 架构设置帧指针
- **第 144-157 行**: ARM 架构设置
  - **第 147 行**: 设置链接寄存器为 sigreturn 地址
  - **第 148 行**: 检查 LR 的最低位（ARM Thumb 模式标志）
  - **第 151-153 行**: 设置信号处理函数参数
    - `r0`: 信号编号
    - `r1`: sf_code (0)
    - `r2`: sigcontext 指针
  - **第 154 行**: 设置上下文已设置标志

**x86 vs ARM 信号处理函数调用约定**:
```
x86:
  参数通过栈传递
  返回地址通过 sigframe 中的 sf_ra_sigreturn 设置

ARM:
  参数通过寄存器传递 (r0, r1, r2)
  返回地址通过 LR (r14) 设置
```

```c
/* 第 159 行：清除 FPU 标志 */
  /* Signal handler should get clean FPU. */
  rp->p_misc_flags &= ~MF_FPU_INITIALIZED;
```

**逐行讲解**:
- **第 159 行**: 注释说明信号处理函数应该获得干净的 FPU
- **第 160 行**: 清除 FPU 初始化标志
  - 下次使用 FPU 时会重新初始化

```c
/* 第 162-166 行：调试检查 */
  if(!RTS_ISSET(rp, RTS_PROC_STOP)) {
	printf("system: warning: sigsend a running process\n");
	printf("caller stack: ");
	proc_stacktrace(caller);
  }
```

**逐行讲解**:
- **第 162 行**: 检查进程是否已停止
  - `RTS_PROC_STOP`: 进程被调试器停止
- **第 163-166 行**: 如果进程未停止，打印警告
  - 正常情况下，信号处理前进程应该已停止
  - 如果进程还在运行，说明状态不一致

```c
/* 第 168 行：返回成功 */
  return OK;
```

```c
/* 第 170 行：条件编译结束 */
#endif /* USE_SIGSEND */
```

#### 关键设计点总结

1. **VMSUSPEND 处理**: 必须在最后一次 `data_copy_vmcheck` 后修改寄存器
2. **架构抽象**: x86 和 ARM 有不同的寄存器保存和参数传递方式
3. **FPU 状态**: 保存和恢复 FPU 状态，避免信号处理函数破坏
4. **栈帧布局**: sigframe 保存在用户栈，包含完整的寄存器状态
5. **返回地址**: 通过 sigframe 或 LR 设置信号处理函数的返回地址

---

### do_getksig() - 获取待处理信号

**源码位置**: [do_getksig.c](../../../minix3/minix/kernel/system/do_getksig.c)

#### 完整源码与逐行讲解

```c
/* 第 1-8 行：文件头注释 */
/* The kernel call that is implemented in this file:
 *	m_type: SYS_GETKSIG
 *
 * The parameters for this kernel call are:
 *	m_sigcalls.endpt	# process with pending signals
 *	m_sigcalls.map		# bit map with pending signals
 */
```

**逐行讲解**:
- **第 1-3 行**: 说明本文件实现 `SYS_GETKSIG` 系统调用
- **第 5-7 行**: 列出消息参数：
  - `endpt`: 有待处理信号的进程端点号（输出）
  - `map`: 待处理信号位图（输出）

```c
/* 第 10-13 行：头文件包含 */
#include "kernel/system.h"
#include <signal.h>
#include <minix/endpoint.h>
```

```c
/* 第 14 行：条件编译 */
#if USE_GETKSIG
```

```c
/* 第 16-19 行：函数头注释 */
/*===========================================================================*
 *			      do_getksig				     *
 *===========================================================================*/
int do_getksig(struct proc * caller, message * m_ptr)
```

```c
/* 第 20-24 行：函数注释 */
/* The signal manager is ready to accept signals and repeatedly does a kernel
 * call to get one. Find a process with pending signals. If no signals are
 * available, return NONE in the process number field.
 */
```

**逐行讲解**:
- **第 20-23 行**: 多行注释说明函数功能
  - 信号管理器（PM）准备好接收信号
  - 重复调用内核获取信号
  - 查找有待处理信号的进程
  - 如果没有信号，返回 `NONE`

```c
/* 第 25-26 行：局部变量声明 */
  register struct proc *rp;
```

**逐行讲解**:
- **第 25 行**: `register struct proc *rp` - 进程指针
  - `register`: 建议编译器优化

```c
/* 第 28-40 行：查找有待处理信号的进程 */
  /* Find the next process with pending signals. */
  for (rp = BEG_USER_ADDR; rp < END_PROC_ADDR; rp++) {
      if (RTS_ISSET(rp, RTS_SIGNALED)) {
          if (caller->p_endpoint != priv(rp)->s_sig_mgr) continue;
	  /* store signaled process' endpoint */
          m_ptr->m_sigcalls.endpt = rp->p_endpoint;
          m_ptr->m_sigcalls.map = rp->p_pending;	/* pending signals map */
          (void) sigemptyset(&rp->p_pending); 	/* clear map in the kernel */
	  RTS_UNSET(rp, RTS_SIGNALED);		/* blocked by SIG_PENDING */
          return(OK);
      }
  }
```

**逐行讲解**:
- **第 28 行**: 注释说明查找下一个有待处理信号的进程
- **第 29 行**: 遍历所有用户进程
  - `BEG_USER_ADDR`: 第一个用户进程的地址
  - `END_PROC_ADDR`: 最后一个进程的地址
- **第 30 行**: 检查进程是否有待处理信号
  - `RTS_ISSET(rp, RTS_SIGNALED)`: 检查 `RTS_SIGNALED` 标志
- **第 31 行**: 检查调用者是否是该进程的信号管理器
  - `priv(rp)->s_sig_mgr`: 进程的信号管理器端点号
  - 如果不是，跳过该进程
  - **设计动机**: 每个进程可以有不同的信号管理器（通常是 PM）
- **第 33 行**: 存储进程端点号到消息
- **第 34 行**: 存储信号位图到消息
  - `rp->p_pending`: 进程的待处理信号位图
- **第 35 行**: 清空内核中的信号位图
  - `sigemptyset(&rp->p_pending)`: 将位图清零
  - `(void)`: 忽略返回值
- **第 36 行**: 清除 `RTS_SIGNALED` 标志
  - `RTS_UNSET(rp, RTS_SIGNALED)`: 清除标志
  - 注释说明被 `SIG_PENDING` 阻塞
- **第 37 行**: 返回成功

**信号管理器的概念**:
```
每个进程都有一个信号管理器（通常是 PM）:
  struct priv {
      ...
      endpoint_t s_sig_mgr;  /* 信号管理器端点号 */
      ...
  };

作用：
  - 只有信号管理器才能获取进程的待处理信号
  - 防止其他进程窃取信号信息
  - 支持不同的信号管理策略
```

```c
/* 第 42-45 行：没有找到待处理信号 */
  /* No process with pending signals was found. */
  m_ptr->m_sigcalls.endpt = NONE;
  return(OK);
```

**逐行讲解**:
- **第 42 行**: 注释说明没有找到待处理信号
- **第 43 行**: 设置端点号为 `NONE`
  - `NONE`: 表示没有信号
- **第 44 行**: 返回成功

```c
/* 第 45 行：条件编译结束 */
#endif /* USE_GETKSIG */
```

#### 关键设计点总结

1. **轮询机制**: PM 通过轮询获取待处理信号
2. **权限检查**: 只有信号管理器才能获取信号
3. **状态转换**: `RTS_SIGNALED` → `RTS_SIG_PENDING`
4. **位图清空**: 获取信号后清空内核中的位图
5. **返回值**: 返回进程端点号和信号位图

---

### do_endksig() - 结束信号处理

**源码位置**: [do_endksig.c](../../../minix3/minix/kernel/system/do_endksig.c)

#### 完整源码与逐行讲解

```c
/* 第 1-7 行：文件头注释 */
/* The kernel call that is implemented in this file:
 *	m_type: SYS_ENDKSIG
 *
 * The parameters for this kernel call are:
 *	m_sigcalls.endpt	# process for which PM is done
 */
```

**逐行讲解**:
- **第 1-3 行**: 说明本文件实现 `SYS_ENDKSIG` 系统调用
- **第 5-6 行**: 消息参数：`endpt` - PM 处理完信号的进程

```c
/* 第 9-10 行：头文件包含 */
#include "kernel/system.h"
```

```c
/* 第 11 行：条件编译 */
#if USE_ENDKSIG 
```

```c
/* 第 13-16 行：函数头注释 */
/*===========================================================================*
 *			      do_endksig				     *
 *===========================================================================*/
int do_endksig(struct proc * caller, message * m_ptr)
```

```c
/* 第 17-21 行：函数注释 */
/* Finish up after a kernel type signal, caused by a SYS_KILL message or a 
 * call to cause_sig by a task. This is called by a signal manager after
 * processing a signal it got with SYS_GETKSIG.
 */
```

**逐行讲解**:
- **第 17-20 行**: 多行注释说明函数功能
  - 完成内核类型信号的处理
  - 由 `SYS_KILL` 消息或任务的 `cause_sig` 调用引起
  - 信号管理器在处理完 `SYS_GETKSIG` 获取的信号后调用

```c
/* 第 22-24 行：局部变量声明 */
  register struct proc *rp;
  int proc_nr;
```

```c
/* 第 26-31 行：参数验证 */
  /* Get process pointer and verify that it had signals pending. If the 
   * process is already dead its flags will be reset. 
   */
  if(!isokendpt(m_ptr->m_sigcalls.endpt, &proc_nr))
	return EINVAL;

  rp = proc_addr(proc_nr);
```

**逐行讲解**:
- **第 26-28 行**: 多行注释说明获取进程指针并验证
  - 如果进程已死，标志会被重置
- **第 29-30 行**: 验证端点号并获取进程号
- **第 32 行**: 获取进程控制块指针

```c
/* 第 33-34 行：权限和状态检查 */
  if (caller->p_endpoint != priv(rp)->s_sig_mgr) return(EPERM);
  if (!RTS_ISSET(rp, RTS_SIG_PENDING)) return(EINVAL);
```

**逐行讲解**:
- **第 33 行**: 检查调用者是否是信号管理器
  - 如果不是，返回 `EPERM`
- **第 34 行**: 检查进程是否在信号处理中
  - `RTS_SIG_PENDING`: 表示 PM 正在处理信号
  - 如果没有设置，返回 `EINVAL`

```c
/* 第 36-39 行：清除信号处理状态 */
  /* The signal manager has finished one kernel signal. Is the process ready? */
  if (!RTS_ISSET(rp, RTS_SIGNALED)) 		/* new signal arrived */
	RTS_UNSET(rp, RTS_SIG_PENDING);	/* remove pending flag */
  return(OK);
```

**逐行讲解**:
- **第 36 行**: 注释说明信号管理器完成了一个内核信号
- **第 37 行**: 检查是否有新信号到达
  - `RTS_SIGNALED`: 表示有新的待处理信号
- **第 38 行**: 如果没有新信号，清除 `SIG_PENDING` 标志
  - **关键逻辑**: 如果有新信号，不清除标志，PM 会继续处理

**多信号处理的逻辑**:
```
场景 1: 单个信号
  1. 信号到达：RTS_SIGNALED = 1
  2. PM 获取信号：RTS_SIGNALED = 0, RTS_SIG_PENDING = 1
  3. PM 处理信号
  4. PM 结束信号：RTS_SIG_PENDING = 0
  结果：进程恢复运行

场景 2: 多个信号（信号处理期间到达新信号）
  1. 信号 1 到达：RTS_SIGNALED = 1
  2. PM 获取信号 1：RTS_SIGNALED = 0, RTS_SIG_PENDING = 1
  3. PM 处理信号 1 期间，信号 2 到达：RTS_SIGNALED = 1
  4. PM 结束信号 1：检查 RTS_SIGNALED = 1，不清除 SIG_PENDING
  5. PM 再次调用 sys_getksig()，获取信号 2
  结果：PM 继续处理下一个信号
```

```c
/* 第 40 行：条件编译结束 */
#endif /* USE_ENDKSIG */
```

#### 关键设计点总结

1. **权限检查**: 只有信号管理器才能结束信号处理
2. **状态检查**: 必须在 `RTS_SIG_PENDING` 状态下调用
3. **多信号处理**: 如果有新信号，不清除 `SIG_PENDING`
4. **原子操作**: 状态检查和清除是原子的

---

### do_sigreturn() - 从信号处理返回

**源码位置**: [do_sigreturn.c](../../../minix3/minix/kernel/system/do_sigreturn.c)

#### 完整源码与逐行讲解

```c
/* 第 1-9 行：文件头注释 */
/* The kernel call that is implemented in this file:
 *	m_type: SYS_SIGRETURN
 *
 * The parameters for this kernel call are:
 *	m_sigcalls.endp		# process returning from handler
 *	m_sigcalls.sigctx	# pointer to sigcontext structure
 */
```

**逐行讲解**:
- **第 1-3 行**: 说明本文件实现 `SYS_SIGRETURN` 系统调用
- **第 5-8 行**: 列出消息参数：
  - `endpt`: 从信号处理函数返回的进程
  - `sigctx`: 指向 sigcontext 结构的指针

```c
/* 第 11-14 行：头文件包含 */
#include "kernel/system.h"
#include <string.h>
#include <machine/cpu.h>
```

**逐行讲解**:
- **第 13 行**: `<machine/cpu.h>` - CPU 相关定义，如 `X86_FLAGS_USER`

```c
/* 第 15 行：条件编译 */
#if USE_SIGRETURN 
```

```c
/* 第 17-20 行：函数头注释 */
/*===========================================================================*
 *			      do_sigreturn				     *
 *===========================================================================*/
int do_sigreturn(struct proc * caller, message * m_ptr)
```

```c
/* 第 21-25 行：函数注释 */
/* POSIX style signals require sys_sigreturn to put things in order before 
 * the signalled process can resume execution
 */
```

**逐行讲解**:
- **第 21-23 行**: 多行注释说明函数功能
  - POSIX 风格信号需要 `sys_sigreturn` 恢复状态
  - 在信号处理进程恢复执行前调用

```c
/* 第 26-30 行：局部变量声明 */
  struct sigcontext sc;
  register struct proc *rp;
  int proc_nr, r;
```

**逐行讲解**:
- **第 26 行**: `struct sigcontext sc` - 信号上下文结构（约 512 字节，栈上分配）
- **第 27 行**: `register struct proc *rp` - 进程指针
- **第 28 行**: `int proc_nr, r` - 进程号和返回值

```c
/* 第 32-36 行：参数验证 */
  if (!isokendpt(m_ptr->m_sigcalls.endpt, &proc_nr)) return EINVAL;
  if (iskerneln(proc_nr)) return EPERM;
  rp = proc_addr(proc_nr);
```

```c
/* 第 38-42 行：拷贝 sigcontext 结构 */
  /* Copy in the sigcontext structure. */
  if ((r = data_copy(m_ptr->m_sigcalls.endpt,
		 (vir_bytes)m_ptr->m_sigcalls.sigctx, KERNEL,
		 (vir_bytes)&sc, sizeof(struct sigcontext))) != OK)
	return r;
```

**逐行讲解**:
- **第 38 行**: 注释说明拷贝 sigcontext 结构
- **第 39-41 行**: 调用 `data_copy` 拷贝
  - `m_ptr->m_sigcalls.endpt`: 源进程（用户进程）
  - `m_ptr->m_sigcalls.sigctx`: 源地址（用户栈）
  - `KERNEL`: 目标进程（内核）
  - `&sc`: 目标地址（内核栈）
  - `sizeof(struct sigcontext)`: 大小
- **第 42 行**: 如果拷贝失败，返回错误码

**注意**: 这里使用 `data_copy` 而不是 `data_copy_vmcheck`
- `data_copy`: 简单的内存拷贝，不检查 VM 状态
- `data_copy_vmcheck`: 检查 VM 状态，可能返回 `VMSUSPEND`
- **原因**: sigreturn 时，进程的内存应该已经在内存中

```c
/* 第 44-48 行：x86 标志寄存器保护 */
#if defined(__i386__)
  /* Restore user bits of psw from sc, maintain system bits from proc. */
  sc.sc_eflags  =  (sc.sc_eflags & X86_FLAGS_USER) |
                (rp->p_reg.psw & ~X86_FLAGS_USER);
#endif
```

**逐行讲解**:
- **第 44 行**: 条件编译，仅 x86 架构
- **第 45 行**: 注释说明恢复用户标志，保留系统标志
- **第 46-47 行**: 标志寄存器保护
  - `sc_eflags & X86_FLAGS_USER`: 保留用户可修改的标志
  - `rp->p_reg.psw & ~X86_FLAGS_USER`: 保留系统标志
  - 合并两者，得到新的标志寄存器值

**x86 标志寄存器的保护**:
```
用户可修改的标志 (X86_FLAGS_USER):
  - CF (进位标志)
  - PF (奇偶标志)
  - AF (辅助进位)
  - ZF (零标志)
  - SF (符号标志)
  - TF (陷阱标志)
  - DF (方向标志)
  - OF (溢出标志)

系统保护的标志 (~X86_FLAGS_USER):
  - IF (中断标志)
  - IOPL (I/O 特权级)
  - NT (嵌套任务)
  - RF (恢复标志)
  - VM (虚拟 8086 模式)
  - ...

保护目的:
  防止用户通过信号处理修改系统标志
  例如：用户不能禁用中断 (IF=0)
```

```c
/* 第 50-64 行：x86 架构寄存器恢复 */
#if defined(__i386__)
  /* Write back registers we allow to be restored, i.e.
   * not the segment ones.
   */
  rp->p_reg.di = sc.sc_edi;
  rp->p_reg.si = sc.sc_esi;
  rp->p_reg.fp = sc.sc_ebp;
  rp->p_reg.bx = sc.sc_ebx;
  rp->p_reg.dx = sc.sc_edx;
  rp->p_reg.cx = sc.sc_ecx;
  rp->p_reg.retreg = sc.sc_eax;
  rp->p_reg.pc = sc.sc_eip;
  rp->p_reg.psw = sc.sc_eflags;
  rp->p_reg.sp = sc.sc_esp;
#endif
```

**逐行讲解**:
- **第 50-52 行**: 多行注释说明恢复允许恢复的寄存器
  - 不包括段寄存器
- **第 53-63 行**: 恢复所有通用寄存器
  - 注意：段寄存器（`cs`, `ds`, `es`, `fs`, `gs`, `ss`）不恢复
  - **设计动机**: 防止用户修改段寄存器，破坏内存保护

**为什么不恢复段寄存器**:
```
段寄存器控制内存访问权限:
  - CS: 代码段，决定可执行权限
  - DS: 数据段，决定数据访问权限
  - SS: 栈段，决定栈访问权限

如果允许用户修改:
  用户可以设置 CS 为内核代码段
  用户可以执行内核代码
  破坏系统安全

因此:
  段寄存器由内核控制，用户不能修改
```

```c
/* 第 66-83 行：ARM 架构寄存器恢复 */
#if defined(__arm__)
  rp->p_reg.psr = sc.sc_spsr;
  rp->p_reg.retreg = sc.sc_r0;
  rp->p_reg.r1 = sc.sc_r1;
  rp->p_reg.r2 = sc.sc_r2;
  rp->p_reg.r3 = sc.sc_r3;
  rp->p_reg.r4 = sc.sc_r4;
  rp->p_reg.r5 = sc.sc_r5;
  rp->p_reg.r6 = sc.sc_r6;
  rp->p_reg.r7 = sc.sc_r7;
  rp->p_reg.r8 = sc.sc_r8;
  rp->p_reg.r9 = sc.sc_r9;
  rp->p_reg.r10 = sc.sc_r10;
  rp->p_reg.fp = sc.sc_r11;
  rp->p_reg.r12 = sc.sc_r12;
  rp->p_reg.sp = sc.sc_usr_sp;
  rp->p_reg.lr = sc.sc_usr_lr;
  rp->p_reg.pc = sc.sc_pc;
#endif
```

**逐行讲解**:
- **第 66 行**: 条件编译，仅 ARM 架构
- **第 67-84 行**: 恢复所有 ARM 寄存器
  - `psr`: 程序状态寄存器
  - `r0-r12`: 通用寄存器
  - `sp`: 栈指针
  - `lr`: 链接寄存器
  - `pc`: 程序计数器

```c
/* 第 85-86 行：恢复寄存器 */
  /* Restore the registers. */
  arch_proc_setcontext(rp, &rp->p_reg, 1, sc.trap_style);
```

**逐行讲解**:
- **第 85 行**: 注释说明恢复寄存器
- **第 86 行**: 调用架构相关的函数设置上下文
  - `rp`: 进程指针
  - `&rp->p_reg`: 寄存器结构
  - `1`: 恢复上下文标志
  - `sc.trap_style`: 陷阱类型

**arch_proc_setcontext 的作用**:
```c
/* 架构相关的上下文设置 */
void arch_proc_setcontext(
    struct proc *rp,        /* 进程指针 */
    struct proc *regs,      /* 寄存器结构 */
    int restore_context,    /* 恢复上下文标志 */
    int trap_style          /* 陷阱类型 */
);
```

```c
/* 第 88 行：验证魔数 */
  if(sc.sc_magic != SC_MAGIC) { printf("kernel sigreturn: corrupt signal context\n"); }
```

**逐行讲解**:
- **第 88 行**: 验证 sigcontext 的魔数
  - 如果不匹配，打印警告
  - **注意**: 不返回错误，继续执行
  - **设计动机**: 即使魔数错误，也尽量恢复进程

```c
/* 第 90-97 行：恢复 FPU 状态 */
#if defined(__i386__)
  if (sc.sc_flags & MF_FPU_INITIALIZED)
  {
	memcpy(rp->p_seg.fpu_state, &sc.sc_fpu_state, FPU_XFP_SIZE);
	rp->p_misc_flags |=  MF_FPU_INITIALIZED; /* Restore math usage flag. */
	/* force reloading FPU */
	release_fpu(rp);
  }
#endif
```

**逐行讲解**:
- **第 90 行**: 条件编译，仅 x86 架构
- **第 91 行**: 检查是否需要恢复 FPU 状态
  - `sc.sc_flags & MF_FPU_INITIALIZED`: 信号处理前使用了 FPU
- **第 93 行**: 拷贝 FPU 状态
  - 从 sigcontext 拷贝到进程控制块
- **第 94 行**: 设置 FPU 初始化标志
- **第 96 行**: 强制重新加载 FPU
  - `release_fpu(rp)`: 标记 FPU 需要重新加载

**release_fpu 的作用**:
```c
/* 释放 FPU，下次使用时重新加载 */
void release_fpu(struct proc *rp)
{
    /* 清除当前 FPU 所有者 */
    if (fpu_owner == rp)
        fpu_owner = NULL;
}
```

```c
/* 第 98 行：返回成功 */
  return OK;
```

```c
/* 第 99 行：条件编译结束 */
#endif /* USE_SIGRETURN */
```

#### 关键设计点总结

1. **标志寄存器保护**: 用户只能修改用户标志，系统标志受保护
2. **段寄存器保护**: 不恢复段寄存器，防止破坏内存保护
3. **FPU 状态恢复**: 恢复信号处理前的 FPU 状态
4. **魔数验证**: 检测 sigcontext 是否被破坏
5. **架构抽象**: x86 和 ARM 有不同的寄存器恢复方式

---

## 数据结构详解

### sigmsg - 信号消息结构

**定义位置**: [include/minix/sigcontext.h](../../../minix3/minix/include/minix/sigcontext.h)

```c
struct sigmsg {
    int sm_signo;           /* signal number */
    sigset_t sm_mask;       /* signal mask to apply */
    vir_bytes sm_sighandler; /* pointer to signal handler */
    vir_bytes sm_sigreturn;  /* pointer to sigreturn code */
    vir_bytes sm_stkptr;    /* stack pointer before signal */
};
```

**字段详解**:

| 字段 | 类型 | 大小 | 说明 |
|------|------|------|------|
| `sm_signo` | `int` | 4 字节 | 信号编号 |
| `sm_mask` | `sigset_t` | 8 字节 | 信号掩码 |
| `sm_sighandler` | `vir_bytes` | 4/8 字节 | 信号处理函数地址 |
| `sm_sigreturn` | `vir_bytes` | 4/8 字节 | sigreturn 代码地址 |
| `sm_stkptr` | `vir_bytes` | 4/8 字节 | 信号前的栈指针 |

**内存布局**:
```
struct sigmsg (32 位系统: 28 字节, 64 位系统: 40 字节)
┌────────────────────────────────────────────────────────────┐
│ sm_signo      [4 字节]  ──► 信号编号 (如 SIGINT = 2)      │
├────────────────────────────────────────────────────────────┤
│ sm_mask       [8 字节]  ──► 信号掩码 (阻塞的信号)         │
├────────────────────────────────────────────────────────────┤
│ sm_sighandler [4/8 字节] ──► 信号处理函数地址             │
├────────────────────────────────────────────────────────────┤
│ sm_sigreturn  [4/8 字节] ──► sigreturn 代码地址           │
├────────────────────────────────────────────────────────────┤
│ sm_stkptr     [4/8 字节] ──► 信号前的栈指针               │
└────────────────────────────────────────────────────────────┘
```

---

### sigframe_sigcontext - 信号帧结构

**定义位置**: [include/minix/sigcontext.h](../../../minix3/minix/include/minix/sigcontext.h)

```c
struct sigframe_sigcontext {
    struct sigcontext sf_sc;    /* signal context */
    void *sf_scp;               /* pointer to sigcontext */
    reg_t sf_fp;                /* frame pointer */
    int sf_signum;              /* signal number */
    void *sf_scpcopy;           /* copy of sf_scp */
    vir_bytes sf_ra_sigreturn;  /* return address for sigreturn */
    vir_bytes sf_ra;            /* return address for normal return */
};
```

**字段详解**:

| 字段 | 类型 | 大小 | 说明 |
|------|------|------|------|
| `sf_sc` | `struct sigcontext` | ~512 字节 | 信号上下文 |
| `sf_scp` | `void *` | 8 字节 | 指向 sigcontext 的指针 |
| `sf_fp` | `reg_t` | 4/8 字节 | 帧指针 |
| `sf_signum` | `int` | 4 字节 | 信号编号 |
| `sf_scpcopy` | `void *` | 8 字节 | sf_scp 的副本 |
| `sf_ra_sigreturn` | `vir_bytes` | 4/8 字节 | sigreturn 返回地址 |
| `sf_ra` | `vir_bytes` | 4/8 字节 | 正常返回地址 |

**内存布局**:
```
用户栈上的 sigframe_sigcontext:
┌────────────────────────────────────────────────────────────┐
│ sf_sc         [~512 字节] ──► 保存的寄存器状态             │
│   ├─ sc_gs, sc_fs, sc_es, sc_ds (段寄存器)                │
│   ├─ sc_edi, sc_esi, sc_ebp, sc_ebx (通用寄存器)          │
│   ├─ sc_edx, sc_ecx, sc_eax (通用寄存器)                  │
│   ├─ sc_eip (PC), sc_esp (SP)                             │
│   ├─ sc_eflags (PSW)                                      │
│   ├─ sc_mask (信号掩码)                                   │
│   ├─ sc_fpu_state (FPU 状态)                              │
│   └─ sc_magic (魔数)                                      │
├────────────────────────────────────────────────────────────┤
│ sf_scp        [8 字节]  ──► 指向 sf_sc                     │
├────────────────────────────────────────────────────────────┤
│ sf_fp         [8 字节]  ──► 帧指针                         │
├────────────────────────────────────────────────────────────┤
│ sf_signum     [4 字节]  ──► 信号编号                       │
├────────────────────────────────────────────────────────────┤
│ sf_scpcopy    [8 字节]  ──► sf_scp 的副本                  │
├────────────────────────────────────────────────────────────┤
│ sf_ra_sigreturn [8 字节] ──► sigreturn 代码地址           │
├────────────────────────────────────────────────────────────┤
│ sf_ra         [8 字节]  ──► 原进程返回地址                 │
└────────────────────────────────────────────────────────────┘
```

---

## 调用链分析

### 完整的信号处理调用链

```
用户进程 A                     内核                    PM
    │                          │                       │
    │  执行中...               │                       │
    │                          │                       │
    │                          │◄── kill(A, SIGUSR1) ──┤
    │                          │                       │
    │                          │  cause_sig(A, SIGUSR1)
    │                          │  p_pending |= (1 << (SIGUSR1-1))
    │                          │  RTS_SET(rp, RTS_SIGNALED)
    │                          │                       │
    │                          │◄── sys_getksig() ─────┤
    │                          │                       │
    │                          ├─── return (A, SIGUSR1)►
    │                          │  p_pending = 0        │
    │                          │  RTS_UNSET(RTS_SIGNALED)
    │                          │  RTS_SET(RTS_SIG_PENDING)
    │                          │                       │
    │                          │                       │  PM 查找信号处理函数
    │                          │                       │  构造 sigmsg:
    │                          │                       │    sm_signo = SIGUSR1
    │                          │                       │    sm_sighandler = handler_addr
    │                          │                       │    sm_sigreturn = sigreturn_addr
    │                          │                       │    sm_mask = signal_mask
    │                          │                       │
    │                          │◄── sys_sigsend(A, &sigmsg) ──┤
    │                          │                       │
    │                          │  data_copy_vmcheck()  │
    │                          │    拷贝 sigmsg        │
    │                          │                       │
    │                          │  arch_get_sp(rp)      │
    │                          │    获取当前 SP        │
    │                          │                       │
    │                          │  构造 sigframe:       │
    │                          │    保存所有寄存器     │
    │                          │    设置 sf_signum     │
    │                          │    设置 sf_ra_sigreturn
    │                          │                       │
    │                          │  data_copy_vmcheck()  │
    │                          │    拷贝 sigframe 到用户栈
    │                          │                       │
    │                          │  修改寄存器:          │
    │                          │    SP = frp           │
    │                          │    PC = sm_sighandler │
    │                          │    FP = new_fp        │
    │                          │                       │
    │◄── 调度恢复 ─────────────┤                       │
    │                          │                       │
    │  执行信号处理函数        │                       │
    │  handler(SIGUSR1, ...)   │                       │
    │  ...                     │                       │
    │  return (跳转到 sigreturn)│                       │
    │                          │                       │
    │                          │◄── sys_sigreturn(A, &sc) ──┤
    │                          │                       │
    │                          │  data_copy()          │
    │                          │    拷贝 sigcontext    │
    │                          │                       │
    │                          │  恢复寄存器:          │
    │                          │    从 sc 恢复所有寄存器
    │                          │    保护系统标志       │
    │                          │    恢复 FPU 状态      │
    │                          │                       │
    │◄── 调度恢复 ─────────────┤                       │
    │                          │                       │
    │  继续执行（信号前位置）  │                       │
    │                          │                       │
    │                          │◄── sys_endksig(A) ────┤
    │                          │                       │
    │                          │  检查 RTS_SIGNALED    │
    │                          │  如果为 0:            │
    │                          │    RTS_UNSET(RTS_SIG_PENDING)
    │                          │                       │
```

---

## 注意事项与易错点

### 1. VMSUSPEND 导致的寄存器多次修改

**问题描述**:
`data_copy_vmcheck` 可能返回 `VMSUSPEND`，导致代码重试。如果在拷贝前修改寄存器，重试时会导致寄存器被多次修改。

**示例**:
```c
/* 错误示例 */
rp->p_reg.sp = (reg_t) frp;  /* 第一次修改 */
if ((r = data_copy_vmcheck(...)) != OK)  /* 返回 VMSUSPEND */
    return r;
/* 系统调用重试 */
rp->p_reg.sp = (reg_t) frp;  /* 第二次修改，SP 被修改两次！ */
```

**解决方案**:
```c
/* 正确示例 */
if ((r = data_copy_vmcheck(...)) != OK)  /* 可能返回 VMSUSPEND */
    return r;
/* 只有拷贝成功后才修改寄存器 */
rp->p_reg.sp = (reg_t) frp;  /* 只修改一次 */
```

---

### 2. 标志寄存器保护不当

**问题描述**:
如果直接恢复用户提供的标志寄存器，用户可能修改系统标志。

**示例**:
```c
/* 错误示例 */
rp->p_reg.psw = sc.sc_eflags;  /* 直接恢复，可能包含 IF=0 */
```

**后果**:
- 用户可以禁用中断（IF=0）
- 系统可能死锁
- 安全漏洞

**解决方案**:
```c
/* 正确示例 */
sc.sc_eflags = (sc.sc_eflags & X86_FLAGS_USER) |
               (rp->p_reg.psw & ~X86_FLAGS_USER);
rp->p_reg.psw = sc.sc_eflags;
```

---

### 3. 段寄存器恢复

**问题描述**:
如果恢复用户提供的段寄存器，用户可能修改段选择子，破坏内存保护。

**示例**:
```c
/* 错误示例 */
rp->p_reg.cs = sc.sc_cs;  /* 用户可能设置为内核代码段 */
rp->p_reg.ds = sc.sc_ds;  /* 用户可能设置为内核数据段 */
```

**后果**:
- 用户可以访问内核内存
- 用户可以执行内核代码
- 完全破坏系统安全

**解决方案**:
```c
/* 正确示例：不恢复段寄存器 */
/* 只恢复通用寄存器 */
rp->p_reg.di = sc.sc_edi;
rp->p_reg.si = sc.sc_esi;
/* ... */
/* 不恢复 cs, ds, es, fs, gs, ss */
```

---

### 4. FPU 状态不一致

**问题描述**:
如果信号处理函数使用了 FPU，但没有恢复原进程的 FPU 状态，会导致 FPU 状态不一致。

**示例**:
```c
/* 错误示例：不保存 FPU 状态 */
/* do_sigsend 中不保存 */
/* do_sigreturn 中不恢复 */
```

**后果**:
- 信号处理函数破坏原进程的 FPU 状态
- 原进程恢复后，FPU 计算结果错误

**解决方案**:
```c
/* do_sigsend 中保存 */
if (proc_used_fpu(rp)) {
    save_fpu(rp);
    memcpy(&fr.sf_sc.sc_fpu_state, rp->p_seg.fpu_state, FPU_XFP_SIZE);
}

/* do_sigreturn 中恢复 */
if (sc.sc_flags & MF_FPU_INITIALIZED) {
    memcpy(rp->p_seg.fpu_state, &sc.sc_fpu_state, FPU_XFP_SIZE);
    rp->p_misc_flags |= MF_FPU_INITIALIZED;
    release_fpu(rp);
}
```

---

### 5. 魔数验证缺失

**问题描述**:
如果不验证 sigcontext 的魔数，无法检测用户是否破坏了 sigcontext 结构。

**示例**:
```c
/* 错误示例：不验证魔数 */
/* do_sigreturn 中不检查 sc_magic */
```

**后果**:
- 用户可能故意破坏 sigcontext
- 恢复错误的寄存器值
- 系统崩溃或安全漏洞

**解决方案**:
```c
/* do_sigreturn 中验证魔数 */
if (sc.sc_magic != SC_MAGIC) {
    printf("kernel sigreturn: corrupt signal context\n");
    /* 可以选择返回错误或继续执行 */
}
```

---

## 信号获取 (SYS_GETKSIG)

### PM 如何获取待处理信号

PM（进程管理器）作为信号管理器，需要主动查询哪些进程有待处理信号：

```c
/* do_getksig.c: 获取待处理信号 */
int do_getksig(struct proc * caller, message * m_ptr)
{
    register struct proc *rp;
    
    /* 遍历所有用户进程 */
    for (rp = BEG_USER_ADDR; rp < END_PROC_ADDR; rp++) {
        /* 检查进程是否有待处理信号 */
        if (RTS_ISSET(rp, RTS_SIGNALED)) {
            /* 验证调用者是否是该进程的信号管理器 */
            if (caller->p_endpoint != priv(rp)->s_sig_mgr)
                continue;
            
            /* 返回进程端点和信号位图 */
            m_ptr->m_sigcalls.endpt = rp->p_endpoint;
            m_ptr->m_sigcalls.map = rp->p_pending;
            
            /* 清除内核中的信号位图 */
            sigemptyset(&rp->p_pending);
            
            /* 状态转换: RTS_SIGNALED → RTS_SIG_PENDING */
            RTS_UNSET(rp, RTS_SIGNALED);
            
            return OK;
        }
    }
    
    /* 没有待处理信号 */
    m_ptr->m_sigcalls.endpt = NONE;
    return OK;
}
```

**关键设计点**:

1. **信号管理器验证**: 每个进程可以有不同的信号管理器（通过 `s_sig_mgr` 指定）
2. **状态转换**: 从 `RTS_SIGNALED` 转换为 `RTS_SIG_PENDING`，防止重复处理
3. **清空位图**: 返回信号位图后，清空内核中的 `p_pending`

### 信号管理器的角色

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号管理器的角色                                                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  进程类型与信号管理器:                                                   │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  用户进程 (如 /bin/sh):                                        │    │
│  │    s_sig_mgr = PM (进程管理器)                                 │    │
│  │    PM 负责处理该进程的信号                                     │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  系统进程 (如 VFS):                                            │    │
│  │    s_sig_mgr = SELF (自己)                                     │    │
│  │    自己处理信号                                                │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  权限检查流程:                                                          │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  PM 调用 sys_getksig()                                         │    │
│  │    ↓                                                           │    │
│  │  内核遍历所有进程                                               │    │
│  │    ↓                                                           │    │
│  │  检查 RTS_SIGNALED 标志                                        │    │
│  │    ↓                                                           │    │
│  │  验证 caller->p_endpoint == priv(rp)->s_sig_mgr               │    │
│  │    ↓                                                           │    │
│  │  返回信号信息给 PM                                              │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 信号发送 (SYS_SIGSEND)

### 核心机制：上下文保存与恢复

`SYS_SIGSEND` 是信号处理的核心，负责保存当前进程上下文并设置信号处理函数的执行环境：

```c
/* do_sigsend.c: 设置信号处理上下文 */
int do_sigsend(struct proc * caller, message * m_ptr)
{
    struct sigmsg smsg;
    register struct proc *rp;
    struct sigframe_sigcontext fr, *frp;
    int proc_nr, r;
    
    /* 参数验证 */
    if (!isokendpt(m_ptr->m_sigcalls.endpt, &proc_nr)) 
        return EINVAL;
    if (iskerneln(proc_nr)) 
        return EPERM;
    rp = proc_addr(proc_nr);
    
    /* 从 PM 获取 sigmsg 结构 */
    if ((r = data_copy_vmcheck(caller, caller->p_endpoint,
            (vir_bytes)m_ptr->m_sigcalls.sigctx, KERNEL,
            (vir_bytes)&smsg, sizeof(struct sigmsg))) != OK)
        return r;
    
    /* 计算信号帧在用户栈的位置 */
    smsg.sm_stkptr = arch_get_sp(rp);
    frp = (struct sigframe_sigcontext *) smsg.sm_stkptr - 1;
    
    /* 保存寄存器到 sigcontext */
    memset(&fr, 0, sizeof(fr));
    fr.sf_scp = &frp->sf_sc;
    
#if defined(__i386__)
    /* x86 架构：保存所有寄存器 */
    fr.sf_sc.sc_gs = rp->p_reg.gs;
    fr.sf_sc.sc_fs = rp->p_reg.fs;
    fr.sf_sc.sc_es = rp->p_reg.es;
    fr.sf_sc.sc_ds = rp->p_reg.ds;
    fr.sf_sc.sc_edi = rp->p_reg.di;
    fr.sf_sc.sc_esi = rp->p_reg.si;
    fr.sf_sc.sc_ebp = rp->p_reg.fp;
    fr.sf_sc.sc_ebx = rp->p_reg.bx;
    fr.sf_sc.sc_edx = rp->p_reg.dx;
    fr.sf_sc.sc_ecx = rp->p_reg.cx;
    fr.sf_sc.sc_eax = rp->p_reg.retreg;
    fr.sf_sc.sc_eip = rp->p_reg.pc;
    fr.sf_sc.sc_cs = rp->p_reg.cs;
    fr.sf_sc.sc_eflags = rp->p_reg.psw;
    fr.sf_sc.sc_esp = rp->p_reg.sp;
    fr.sf_sc.sc_ss = rp->p_reg.ss;
    
    /* 保存 FPU 状态 */
    if (proc_used_fpu(rp)) {
        save_fpu(rp);
        memcpy(&fr.sf_sc.sc_fpu_state, rp->p_seg.fpu_state, FPU_XFP_SIZE);
    }
#endif
    
    /* 初始化信号帧 */
    fr.sf_sc.sc_mask = smsg.sm_mask;
    fr.sf_sc.sc_flags = rp->p_misc_flags & MF_FPU_INITIALIZED;
    fr.sf_sc.sc_magic = SC_MAGIC;
    fr.sf_signum = smsg.sm_signo;
    fr.sf_ra_sigreturn = smsg.sm_sigreturn;
    fr.sf_ra = rp->p_reg.pc;
    
    /* 拷贝信号帧到用户栈 */
    if ((r = data_copy_vmcheck(caller, KERNEL, (vir_bytes)&fr,
            m_ptr->m_sigcalls.endpt, (vir_bytes)frp,
            sizeof(struct sigframe_sigcontext))) != OK)
        return r;
    
    /* 修改进程寄存器以执行信号处理函数 */
    rp->p_reg.sp = (reg_t) frp;          /* 新栈指针 */
    rp->p_reg.pc = (reg_t) smsg.sm_sighandler;  /* 信号处理函数 */
    
#if defined(__i386__)
    rp->p_reg.fp = (reg_t) &frp->sf_fp;  /* 新帧指针 */
#elif defined(__arm__)
    rp->p_reg.lr = (reg_t) smsg.sm_sigreturn;  /* 返回地址 */
    rp->p_reg.retreg = (reg_t) smsg.sm_signo;  /* 参数: 信号编号 */
    rp->p_reg.r1 = 0;                          /* 参数: sf_code */
    rp->p_reg.r2 = (reg_t) fr.sf_scp;          /* 参数: sigcontext 指针 */
#endif
    
    return OK;
}
```

### 信号帧的数据结构

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号帧结构 (struct sigframe_sigcontext)                                 │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  用户栈布局（信号处理前）:                                               │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  高地址                                                        │    │
│  │  ├── 局部变量                                                  │    │
│  │  ├── 返回地址                                                  │    │
│  │  ├── 调用者保存的寄存器                                        │    │
│  │  └── ...                                                       │    │
│  │  原栈指针 (SP) ──►                                             │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  用户栈布局（信号处理后）:                                               │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  高地址                                                        │    │
│  │  ├── 局部变量                                                  │    │
│  │  ├── 返回地址                                                  │    │
│  │  ├── 调用者保存的寄存器                                        │    │
│  │  └── ...                                                       │    │
│  │  ├── sigframe_sigcontext (信号帧)                              │    │
│  │  │   ├── sf_sc (sigcontext, 所有寄存器)                        │    │
│  │  │   │   ├── sc_gs, sc_fs, sc_es, sc_ds (段寄存器)             │    │
│  │  │   │   ├── sc_edi, sc_esi, sc_ebp (通用寄存器)               │    │
│  │  │   │   ├── sc_ebx, sc_edx, sc_ecx, sc_eax                    │    │
│  │  │   │   ├── sc_eip (原 PC)                                    │    │
│  │  │   │   ├── sc_eflags (原标志寄存器)                          │    │
│  │  │   │   ├── sc_esp (原栈指针)                                 │    │
│  │  │   │   ├── sc_mask (信号掩码)                                │    │
│  │  │   │   ├── sc_flags (标志)                                   │    │
│  │  │   │   └── sc_fpu_state (FPU 状态)                           │    │
│  │  │   ├── sf_scp (指向 sf_sc 的指针)                            │    │
│  │  │   ├── sf_fp (帧指针)                                        │    │
│  │  │   ├── sf_signum (信号编号)                                  │    │
│  │  │   ├── sf_ra (原返回地址)                                    │    │
│  │  │   └── sf_ra_sigreturn (sigreturn 地址)                      │    │
│  │  新栈指针 (SP) ──►                                             │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  大小: 约 512-1024 字节（取决于架构和 FPU 状态）                        │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 栈布局变化的可视化

```
信号处理前的栈:
┌─────────────────────────────────────┐
│  用户栈                              │
│  ├── 局部变量                        │
│  ├── 返回地址                        │
│  └── ...                             │
│  SP ──►                              │
└─────────────────────────────────────┘

信号处理时的栈:
┌─────────────────────────────────────┐
│  用户栈                              │
│  ├── 局部变量                        │
│  ├── 返回地址                        │
│  └── ...                             │
│  ├── sigframe_sigcontext             │
│  │   ├── sf_sc (所有寄存器)          │
│  │   ├── sf_signum                   │
│  │   ├── sf_ra (原 PC)               │
│  │   └── sf_ra_sigreturn             │
│  SP ──►                              │
└─────────────────────────────────────┘

信号处理函数执行:
┌─────────────────────────────────────┐
│  用户栈                              │
│  ├── 局部变量                        │
│  ├── 返回地址                        │
│  └── ...                             │
│  ├── sigframe_sigcontext             │
│  ├── 信号处理函数的局部变量          │
│  ├── 信号处理函数的返回地址          │
│  └── ...                             │
│  SP ──►                              │
└─────────────────────────────────────┘
```

### 关键警告：VMSUSPEND 问题

```c
/* WARNING: 以下代码可能运行多次 */
/* 因为 data_copy_vmcheck 可能返回 VMSUSPEND */

/* 错误示例：在拷贝前修改寄存器 */
rp->p_reg.sp = (reg_t) frp;  /* 错误！可能运行多次 */
if ((r = data_copy_vmcheck(...)) != OK)
    return r;

/* 正确示例：在拷贝成功后修改寄存器 */
if ((r = data_copy_vmcheck(...)) != OK)
    return r;
/* 只有拷贝成功后才修改寄存器 */
rp->p_reg.sp = (reg_t) frp;
rp->p_reg.pc = (reg_t) smsg.sm_sighandler;
```

**VMSUSPEND 的含义**: 当目标进程的内存被换出时，`data_copy_vmcheck` 会返回 `VMSUSPEND`，表示需要等待内存换入。此时系统调用会返回，但稍后会重试。如果在拷贝前修改寄存器，重试时会导致寄存器被多次修改，破坏进程状态。

---

## 信号返回 (SYS_SIGRETURN)

### 从信号处理函数返回

当信号处理函数执行完毕后，需要恢复进程到信号前的状态：

```c
/* do_sigreturn.c: 从信号处理返回 */
int do_sigreturn(struct proc * caller, message * m_ptr)
{
    struct sigcontext sc;
    register struct proc *rp;
    int proc_nr, r;
    
    /* 参数验证 */
    if (!isokendpt(m_ptr->m_sigcalls.endpt, &proc_nr)) 
        return EINVAL;
    if (iskerneln(proc_nr)) 
        return EPERM;
    rp = proc_addr(proc_nr);
    
    /* 从用户栈拷贝 sigcontext */
    if ((r = data_copy(m_ptr->m_sigcalls.endpt,
            (vir_bytes)m_ptr->m_sigcalls.sigctx, KERNEL,
            (vir_bytes)&sc, sizeof(struct sigcontext))) != OK)
        return r;
    
#if defined(__i386__)
    /* 恢复用户标志，保留系统标志 */
    sc.sc_eflags = (sc.sc_eflags & X86_FLAGS_USER) |
                   (rp->p_reg.psw & ~X86_FLAGS_USER);
    
    /* 恢复寄存器 */
    rp->p_reg.di = sc.sc_edi;
    rp->p_reg.si = sc.sc_esi;
    rp->p_reg.fp = sc.sc_ebp;
    rp->p_reg.bx = sc.sc_ebx;
    rp->p_reg.dx = sc.sc_edx;
    rp->p_reg.cx = sc.sc_ecx;
    rp->p_reg.retreg = sc.sc_eax;
    rp->p_reg.pc = sc.sc_eip;
    rp->p_reg.psw = sc.sc_eflags;
    rp->p_reg.sp = sc.sc_esp;
#endif
    
    /* 恢复 FPU 状态 */
    if (sc.sc_flags & MF_FPU_INITIALIZED) {
        memcpy(rp->p_seg.fpu_state, &sc.sc_fpu_state, FPU_XFP_SIZE);
        rp->p_misc_flags |= MF_FPU_INITIALIZED;
        release_fpu(rp);
    }
    
    /* 验证魔数 */
    if (sc.sc_magic != SC_MAGIC) {
        printf("kernel sigreturn: corrupt signal context\n");
    }
    
    return OK;
}
```

### 标志寄存器的保护

```
┌─────────────────────────────────────────────────────────────────────────┐
│  x86 标志寄存器 (EFLAGS) 的保护                                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  EFLAGS 寄存器结构:                                                     │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  位 0:   CF (进位标志)           用户可修改                     │    │
│  │  位 2:   PF (奇偶标志)           用户可修改                     │    │
│  │  位 4:   AF (辅助进位)           用户可修改                     │    │
│  │  位 6:   ZF (零标志)             用户可修改                     │    │
│  │  位 7:   SF (符号标志)           用户可修改                     │    │
│  │  位 8:   TF (陷阱标志)           用户可修改                     │    │
│  │  位 9:   IF (中断标志)           受保护                         │    │
│  │  位 10:  DF (方向标志)           用户可修改                      │    │
│  │  位 11:  OF (溢出标志)           用户可修改                      │    │
│  │  位 12-13: IOPL (I/O 特权级)    受保护                          │    │
│  │  位 14:  NT (嵌套任务)           受保护                         │    │
│  │  ...                                                           │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  恢复策略:                                                              │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  sc_eflags = (sc_eflags & X86_FLAGS_USER) |                    │    │
│  │              (rp->p_reg.psw & ~X86_FLAGS_USER)                 │    │
│  │                                                                │    │
│  │  X86_FLAGS_USER: 用户可修改的标志位                            │    │
│  │  ~X86_FLAGS_USER: 系统标志位，从当前进程保留                   │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  目的: 防止用户通过信号处理修改系统标志（如 IF、IOPL）                  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 结束信号处理 (SYS_ENDKSIG)

### 清除信号处理状态

PM 在完成信号处理后，需要通知内核清除信号处理状态：

```c
/* do_endksig.c: 结束信号处理 */
int do_endksig(struct proc * caller, message * m_ptr)
{
    register struct proc *rp;
    int proc_nr;
    
    /* 参数验证 */
    if (!isokendpt(m_ptr->m_sigcalls.endpt, &proc_nr))
        return EINVAL;
    
    rp = proc_addr(proc_nr);
    
    /* 权限检查 */
    if (caller->p_endpoint != priv(rp)->s_sig_mgr)
        return EPERM;
    
    /* 状态检查 */
    if (!RTS_ISSET(rp, RTS_SIG_PENDING))
        return EINVAL;
    
    /* 如果没有新信号到达，清除 SIG_PENDING */
    if (!RTS_ISSET(rp, RTS_SIGNALED))
        RTS_UNSET(rp, RTS_SIG_PENDING);
    
    return OK;
}
```

**关键逻辑**: 如果在信号处理期间有新信号到达（`RTS_SIGNALED` 被设置），则不清除 `RTS_SIG_PENDING`，PM 会继续处理下一个信号。

---

## 信号处理完整时序

### 详细的时序图

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号处理完整时序                                                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  用户进程 A                     内核                    PM              │
│      │                          │                       │               │
│      │  执行中...               │                       │               │
│      │                          │                       │               │
│      │                          │◄── kill(A, SIGUSR1) ──┤               │
│      │                          │                       │               │
│      │                          │  cause_sig(A, SIGUSR1)│               │
│      │                          │  p_pending |= SIGUSR1 │               │
│      │                          │  RTS_SIGNALED = 1     │               │
│      │                          │                       │               │
│      │                          │◄── sys_getksig() ─────┤               │
│      │                          │                       │               │
│      │                          ├─── return (A, SIGUSR1)►               │
│      │                          │  p_pending = 0        │               │
│      │                          │  RTS_SIG_PENDING = 1  │               │
│      │                          │                       │               │
│      │                          │                       │  PM 查找信号处理函数
│      │                          │                       │  构造 sigmsg
│      │                          │                       │               │
│      │                          │◄── sys_sigsend(A) ────┤               │
│      │                          │                       │               │
│      │                          │  保存寄存器到栈       │               │
│      │                          │  修改 SP, PC          │               │
│      │                          │                       │               │
│      │◄── 调度恢复 ─────────────┤                       │               │
│      │                          │                       │               │
│      │  执行信号处理函数        │                       │               │
│      │  ...                     │                       │               │
│      │  return (调用 sigreturn) │                       │               │
│      │                          │                       │               │
│      │                          │◄── sys_sigreturn(A) ──┤               │
│      │                          │                       │               │
│      │                          │  从栈恢复寄存器       │               │
│      │                          │                       │               │
│      │◄── 调度恢复 ─────────────┤                       │               │
│      │                          │                       │               │
│      │  继续执行（信号前位置）  │                       │               │
│      │                          │                       │               │
│      │                          │◄── sys_endksig(A) ────┤               │
│      │                          │                       │               │
│      │                          │  RTS_SIG_PENDING = 0  │               │
│      │                          │                       │               │
└─────────────────────────────────────────────────────────────────────────┘
```

### 多信号处理的时序

```
┌─────────────────────────────────────────────────────────────────────────┐
│  多信号处理的时序                                                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  场景: 进程 A 正在处理 SIGUSR1 时，收到 SIGUSR2                         │
│                                                                         │
│  用户进程 A                     内核                    PM              │
│      │                          │                       │               │
│      │  执行 SIGUSR1 处理函数   │                       │               │
│      │                          │                       │               │
│      │                          │◄── kill(A, SIGUSR2) ──┤               │
│      │                          │                       │               │
│      │                          │  cause_sig(A, SIGUSR2)│               │
│      │                          │  p_pending |= SIGUSR2 │               │
│      │                          │  RTS_SIGNALED = 1     │               │
│      │                          │                       │               │
│      │  SIGUSR1 处理完成        │                       │               │
│      │  return                  │                       │               │
│      │                          │                       │               │
│      │                          │◄── sys_sigreturn(A) ──┤               │
│      │                          │                       │               │
│      │                          │  恢复寄存器           │               │
│      │                          │                       │               │
│      │                          │◄── sys_endksig(A) ────┤               │
│      │                          │                       │               │
│      │                          │  检查 RTS_SIGNALED    │               │
│      │                          │  发现为 1，不清除 SIG_PENDING        │
│      │                          │                       │               │
│      │                          │◄── sys_getksig() ─────┤               │
│      │                          │                       │               │
│      │                          ├─── return (A, SIGUSR2)►               │
│      │                          │                       │               │
│      │                          │◄── sys_sigsend(A) ────┤               │
│      │                          │                       │               │
│      │  执行 SIGUSR2 处理函数   │                       │               │
│      │  ...                     │                       │               │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 现代硬件适配建议

### 当前实现的局限性

| 问题 | 说明 | 现代硬件影响 |
|------|------|-------------|
| **架构相关代码** | x86/ARM 硬编码 | 可移植性差 |
| **FPU 状态保存** | 每次信号都保存 | 性能开销 |
| **无 SIMD 支持** | 无 AVX/SVE 保存 | 现代应用异常 |
| **固定大小栈帧** | 无法适应扩展寄存器 | 兼容性问题 |

### 适配方案

#### 1. 架构抽象层

```c
/* 统一的架构抽象接口 */
struct arch_sigcontext_ops {
    /* 保存寄存器到 sigcontext */
    void (*save)(const struct proc *rp, struct sigcontext *sc);
    
    /* 从 sigcontext 恢复寄存器 */
    void (*restore)(struct proc *rp, const struct sigcontext *sc);
    
    /* 设置信号帧 */
    int (*setup_frame)(struct proc *rp, struct sigframe *fr,
                       const struct sigmsg *smsg);
    
    /* 获取 sigcontext 大小 */
    size_t (*context_size)(void);
};
```

#### 2. 延迟 FPU 保存

```
Lazy FPU 保存策略:
┌────────────────────────────────────────────────────────────────┐
│  传统方式:                                                     │
│    每次信号都保存 FPU 状态 (~512 字节)                         │
│    开销: 约 1-2 μs                                             │
│                                                                │
│  延迟保存:                                                     │
│    只在进程使用过 FPU 时保存                                   │
│    信号处理函数可能不使用 FPU                                  │
│    开销: 仅在需要时                                            │
│                                                                │
│  实现:                                                         │
│    if (proc_used_fpu(rp)) {                                    │
│        save_fpu(rp);                                           │
│        sc->sc_flags |= MF_FPU_INITIALIZED;                     │
│    }                                                           │
└────────────────────────────────────────────────────────────────┘
```

#### 3. SIMD 状态支持

```
现代 SIMD 扩展:
┌────────────────────────────────────────────────────────────────┐
│  AVX-512 (x86):                                                │
│    - 32 个 512-bit 寄存器 (ZMM0-ZMM31)                        │
│    - 状态大小: ~2KB                                            │
│    - 需要保存: ZMM, YMM, XMM, MXCSR                           │
│                                                                │
│  SVE (ARM):                                                    │
│    - 可变长度向量 (最大 2048-bit)                              │
│    - 状态大小: 可变                                            │
│    - 需要保存: Z0-Z31, P0-P15, FFR, ZCR_EL1                   │
│                                                                │
│  实现:                                                         │
│    struct sigcontext {                                         │
│        ...                                                     │
│        void *sc_ext_state;     /* 扩展状态指针 */             │
│        size_t sc_ext_size;     /* 扩展状态大小 */             │
│        uint32_t sc_ext_type;   /* 扩展状态类型 */             │
│    };                                                          │
└────────────────────────────────────────────────────────────────┘
```

---

## Rust 重构建议

### 模块级架构设计

```rust
//! 信号处理模块
//! 
//! 提供信号的发送、捕获和返回功能

#![no_std]

use core::mem::size_of;

pub mod signal;
pub mod context;
pub mod frame;

/// 信号编号
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signal(u8);

impl Signal {
    pub const SIGHUP: Self = Self(1);
    pub const SIGINT: Self = Self(2);
    pub const SIGQUIT: Self = Self(3);
    pub const SIGILL: Self = Self(4);
    pub const SIGTRAP: Self = Self(5);
    pub const SIGABRT: Self = Self(6);
    pub const SIGBUS: Self = Self(7);
    pub const SIGFPE: Self = Self(8);
    pub const SIGKILL: Self = Self(9);
    pub const SIGUSR1: Self = Self(10);
    pub const SIGSEGV: Self = Self(11);
    pub const SIGUSR2: Self = Self(12);
    pub const SIGPIPE: Self = Self(13);
    pub const SIGALRM: Self = Self(14);
    pub const SIGTERM: Self = Self(15);
    // ...
    
    pub fn as_u8(&self) -> u8 {
        self.0
    }
}

/// 信号掩码
#[derive(Clone, Copy, Debug, Default)]
pub struct SigSet(u64);

impl SigSet {
    pub fn new() -> Self {
        Self(0)
    }
    
    pub fn add(&mut self, sig: Signal) {
        self.0 |= 1 << (sig.0 - 1);
    }
    
    pub fn remove(&mut self, sig: Signal) {
        self.0 &= !(1 << (sig.0 - 1));
    }
    
    pub fn contains(&self, sig: Signal) -> bool {
        (self.0 & (1 << (sig.0 - 1))) != 0
    }
    
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
    
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}
```

### 类型安全的信号上下文

```rust
pub mod context {
    use super::*;
    use crate::arch::UserRegs;
    use crate::memory::VirtAddr;
    
    /// 信号上下文（架构无关）
    #[derive(Clone, Debug)]
    pub struct SigContext {
        /// 通用寄存器
        pub regs: GeneralRegs,
        /// 程序计数器
        pub pc: VirtAddr,
        /// 栈指针
        pub sp: VirtAddr,
        /// 标志寄存器
        pub flags: u64,
        /// 信号掩码
        pub mask: SigSet,
        /// 上下文标志
        pub context_flags: ContextFlags,
        /// 魔数（用于验证）
        pub magic: u32,
        /// FPU/SIMD 状态
        pub fpu_state: Option<FpuState>,
    }
    
    /// 通用寄存器（架构相关）
    #[derive(Clone, Debug)]
    #[cfg(target_arch = "x86_64")]
    pub struct GeneralRegs {
        pub rax: u64, pub rbx: u64, pub rcx: u64, pub rdx: u64,
        pub rsi: u64, pub rdi: u64, pub rbp: u64, pub rsp: u64,
        pub r8: u64, pub r9: u64, pub r10: u64, pub r11: u64,
        pub r12: u64, pub r13: u64, pub r14: u64, pub r15: u64,
    }
    
    #[derive(Clone, Debug)]
    #[cfg(target_arch = "arm")]
    pub struct GeneralRegs {
        pub r0: u32, pub r1: u32, pub r2: u32, pub r3: u32,
        pub r4: u32, pub r5: u32, pub r6: u32, pub r7: u32,
        pub r8: u32, pub r9: u32, pub r10: u32, pub r11: u32,
        pub r12: u32, pub sp: u32, pub lr: u32, pub pc: u32,
    }
    
    /// 上下文标志
    bitflags::bitflags! {
        #[derive(Clone, Copy, Debug)]
        pub struct ContextFlags: u32 {
            const FPU_INITIALIZED = 0x01;
            const EXTENDED_STATE = 0x02;
        }
    }
    
    /// FPU/SIMD 状态
    #[derive(Clone, Debug)]
    pub struct FpuState {
        #[cfg(target_arch = "x86_64")]
        pub data: [u8; 512],  /* FXSAVE 区域 */
        
        #[cfg(target_arch = "arm")]
        pub data: [u8; 256],  /* VFP 状态 */
    }
    
    /// 架构相关接口
    pub trait ArchSigContext {
        /// 从进程寄存器保存上下文
        fn save(regs: &UserRegs) -> SigContext;
        
        /// 恢复上下文到进程寄存器
        fn restore(&self, regs: &mut UserRegs);
        
        /// 设置信号帧
        fn setup_frame(
            sp: VirtAddr,
            handler: VirtAddr,
            sigreturn: VirtAddr,
            sig: Signal,
        ) -> Result<(VirtAddr, SigFrame), SignalError>;
    }
}
```

### 信号帧管理

```rust
pub mod frame {
    use super::*;
    use crate::memory::VirtAddr;
    
    /// 信号帧
    #[derive(Clone, Debug)]
    pub struct SigFrame {
        /// 信号上下文
        pub context: SigContext,
        /// 指向 context 的指针（用于用户态访问）
        pub context_ptr: VirtAddr,
        /// 帧指针
        pub frame_ptr: VirtAddr,
        /// 信号编号
        pub signum: Signal,
        /// 原返回地址
        pub return_addr: VirtAddr,
        /// sigreturn 函数地址
        pub sigreturn_addr: VirtAddr,
    }
    
    /// 信号消息（PM 传递给内核）
    #[derive(Clone, Debug)]
    pub struct SigMsg {
        /// 信号编号
        pub signo: Signal,
        /// 信号掩码
        pub mask: SigSet,
        /// 信号处理函数地址
        pub sighandler: VirtAddr,
        /// sigreturn 函数地址
        pub sigreturn: VirtAddr,
        /// 用户栈指针
        pub stack_ptr: VirtAddr,
    }
    
    /// 信号帧管理器
    pub struct SigFrameManager {
        /// 当前信号帧（用于调试）
        current_frame: Option<SigFrame>,
    }
    
    impl SigFrameManager {
        pub const fn new() -> Self {
            Self {
                current_frame: None,
            }
        }
        
        /// 设置信号帧
        pub fn setup(
            &mut self,
            process: &mut Process,
            msg: &SigMsg,
        ) -> Result<(), SignalError> {
            // 1. 保存当前寄存器
            let context = SigContext::save(&process.regs);
            
            // 2. 计算信号帧位置
            let frame_addr = msg.stack_ptr - size_of::<SigFrame>();
            
            // 3. 构造信号帧
            let frame = SigFrame {
                context,
                context_ptr: frame_addr + offset_of!(SigFrame, context),
                frame_ptr: frame_addr + offset_of!(SigFrame, frame_ptr),
                signum: msg.signo,
                return_addr: process.regs.pc,
                sigreturn_addr: msg.sigreturn,
            };
            
            // 4. 拷贝到用户栈
            process.write_user(frame_addr, &frame)?;
            
            // 5. 修改进程寄存器
            process.regs.sp = frame_addr;
            process.regs.pc = msg.sighandler;
            
            self.current_frame = Some(frame);
            Ok(())
        }
        
        /// 从信号帧恢复
        pub fn restore(
            &mut self,
            process: &mut Process,
            context_addr: VirtAddr,
        ) -> Result<(), SignalError> {
            // 1. 从用户栈读取 sigcontext
            let context: SigContext = process.read_user(context_addr)?;
            
            // 2. 验证魔数
            if context.magic != SC_MAGIC {
                return Err(SignalError::CorruptContext);
            }
            
            // 3. 恢复寄存器
            context.restore(&mut process.regs);
            
            self.current_frame = None;
            Ok(())
        }
    }
    
    const SC_MAGIC: u32 = 0x5A5A5A5A;
    
    #[derive(Debug)]
    pub enum SignalError {
        InvalidProcess,
        InvalidSignal,
        CorruptContext,
        MemoryFault,
        PermissionDenied,
    }
}
```

### 信号管理器

```rust
pub mod signal {
    use super::*;
    use crate::process::{Process, ProcessId};
    use crate::ipc::Endpoint;
    
    /// 信号管理器
    pub struct SignalManager {
        /// 信号管理器进程
        sig_mgr: Endpoint,
    }
    
    impl SignalManager {
        /// 获取待处理信号
        pub fn get_pending_signal(&self) -> Result<Option<(ProcessId, SigSet)>, SignalError> {
            let mut msg = Message::new(SYS_GETKSIG);
            
            self.call_kernel(&mut msg)?;
            
            let endpt = msg.get_endpt();
            if endpt == NONE {
                return Ok(None);
            }
            
            let map = msg.get_sigmap();
            Ok(Some((ProcessId::from(endpt), SigSet(map))))
        }
        
        /// 发送信号
        pub fn send_signal(
            &self,
            process: ProcessId,
            msg: &SigMsg,
        ) -> Result<(), SignalError> {
            let mut kmsg = Message::new(SYS_SIGSEND);
            kmsg.set_endpt(process.into());
            kmsg.set_sigctx(msg);
            
            self.call_kernel(&mut kmsg)
        }
        
        /// 从信号返回
        pub fn return_from_signal(
            &self,
            process: ProcessId,
            context_addr: VirtAddr,
        ) -> Result<(), SignalError> {
            let mut msg = Message::new(SYS_SIGRETURN);
            msg.set_endpt(process.into());
            msg.set_sigctx(context_addr);
            
            self.call_kernel(&mut msg)
        }
        
        /// 结束信号处理
        pub fn end_signal(&self, process: ProcessId) -> Result<(), SignalError> {
            let mut msg = Message::new(SYS_ENDKSIG);
            msg.set_endpt(process.into());
            
            self.call_kernel(&mut msg)
        }
        
        fn call_kernel(&self, msg: &mut Message) -> Result<(), SignalError> {
            // 调用内核系统调用
            // ...
            Ok(())
        }
    }
}
```

### Rust 重构的核心优势

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Rust 重构的核心优势                                                     │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. 类型安全的信号编号                                                   │
│     ┌──────────────────────────────────────────────────────────────┐   │
│     │  C: int sig ── 可能传入无效值                                 │   │
│     │  Rust: Signal enum ── 编译期保证有效性                        │   │
│     └──────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  2. 类型安全的信号上下文                                                 │
│     ┌──────────────────────────────────────────────────────────────┐   │
│     │  C: struct sigcontext ── 字段可能被错误访问                   │   │
│     │  Rust: SigContext struct ── 借用检查保证安全访问              │   │
│     └──────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  3. 架构抽象                                                             │
│     ┌──────────────────────────────────────────────────────────────┐   │
│     │  C: #if defined(__i386__) ── 编译时硬编码                     │   │
│     │  Rust: trait ArchSigContext ── 运行时多态                     │   │
│     └──────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  4. 显式错误处理                                                         │
│     ┌──────────────────────────────────────────────────────────────┐   │
│     │  C: return EINVAL ── 容易被忽略                               │   │
│     │  Rust: Result<(), SignalError> ── 强制处理错误                │   │
│     └──────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  5. 内存安全                                                             │
│     ┌──────────────────────────────────────────────────────────────┐   │
│     │  C: memcpy(&fr.sf_sc, ...) ── 可能越界                        │   │
│     │  Rust: process.write_user() ── 边界检查                       │   │
│     └──────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 要点总结

### 核心知识点

1. **微内核信号机制**
   - 内核只负责上下文切换
   - PM 负责信号管理策略
   - 更灵活、更安全

2. **信号帧的作用**
   - 保存进程的完整上下文
   - 设置信号处理函数的执行环境
   - 支持信号返回后恢复执行

3. **状态转换的时机**
   - `RTS_SIGNALED`: 信号产生时设置
   - `RTS_SIG_PENDING`: PM 获取信号时设置
   - 清除: PM 结束信号处理时

### 灾难预演

**场景 1: 在拷贝前修改寄存器**

```
后果:
  ✓ 如果 data_copy_vmcheck 返回 VMSUSPEND，系统调用会重试
  ✓ 寄存器被多次修改，破坏进程状态
  ✓ 进程恢复后行为异常

正确做法:
  确保在拷贝成功后才修改寄存器
```

**场景 2: 忘记验证魔数**

```
后果:
  ✓ 用户可能伪造 sigcontext
  ✓ 恢复时可能破坏进程状态
  ✓ 安全漏洞

正确做法:
  恢复前验证 sc_magic == SC_MAGIC
```

**场景 3: 不保护系统标志**

```
后果:
  ✓ 用户可能修改 IF 标志（中断使能）
  ✓ 用户可能修改 IOPL（I/O 特权级）
  ✓ 系统安全性被破坏

正确做法:
  恢复标志寄存器时，保留系统标志位
```

### 互动自测

1. **问题**: 为什么 Minix3 把信号管理放在用户态（PM）？

   <details>
   <summary>点击查看答案</summary>
   
   **答案**: 
   - **微内核哲学**: 内核只负责最核心的功能（上下文切换），策略在用户态实现
   - **灵活性**: PM 可以实现不同的信号策略，无需修改内核
   - **安全性**: 内核代码更小，攻击面更小
   - **可调试性**: 信号处理逻辑在用户态，更容易调试
   
   </details>

2. **问题**: 信号帧为什么保存在用户栈而不是内核栈？

   <details>
   <summary>点击查看答案</summary>
   
   **答案**: 
   - **支持嵌套信号**: 用户栈可以保存多个信号帧
   - **用户态访问**: 信号处理函数可能需要访问 sigcontext
   - **内核栈大小限制**: 内核栈通常很小（8KB），无法保存大量信号帧
   - **传统 Unix 兼容**: 符合 POSIX 标准
   
   </details>

3. **问题**: VMSUSPEND 是什么？为什么需要特别处理？

   <details>
   <summary>点击查看答案</summary>
   
   **答案**: 
   - **定义**: 当目标进程的内存被换出时，`data_copy_vmcheck` 返回 `VMSUSPEND`
   - **影响**: 系统调用会返回，但稍后会重试
   - **风险**: 如果在拷贝前修改寄存器，重试时会导致寄存器被多次修改
   - **解决方案**: 确保在拷贝成功后才修改进程状态
   
   </details>

---

## 参考链接

- [do_sigsend.c](../../../minix3/minix/kernel/system/do_sigsend.c) - 设置信号处理上下文
- [do_getksig.c](../../../minix3/minix/kernel/system/do_getksig.c) - 获取待处理信号
- [do_endksig.c](../../../minix3/minix/kernel/system/do_endksig.c) - 结束信号处理
- [do_sigreturn.c](../../../minix3/minix/kernel/system/do_sigreturn.c) - 从信号处理返回
