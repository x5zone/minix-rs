# do_sigsend.c 详细讲解

**文件位置**: `minix3/minix/kernel/system/do_sigsend.c`

**总行数**: 166 行

**作用**: 实现 `SYS_SIGSEND` 系统调用，向进程发送信号并设置信号处理环境

---

## 一、文件概述

### 1.1 是什么（What）

`do_sigsend.c` 实现了 MINIX3 的**信号发送系统调用**：

| 系统调用 | 功能 |
|---------|------|
| `SYS_SIGSEND` | 向进程发送信号，设置信号处理环境 |

**核心功能**：
- 在用户栈上构建信号帧（sigframe）
- 保存当前寄存器状态到信号上下文
- 设置进程执行信号处理程序
- 支持 x86 和 ARM 两种架构

### 1.2 为什么需要（Why）

**设计原因**：

POSIX 信号处理需要保存和恢复进程上下文：

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号处理流程                                                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  正常执行:  用户程序 → [收到信号] → 信号处理程序 → [返回] → 用户程序    │
│                                                                         │
│  需要保存:                                                              │
│  1. 当前寄存器状态（PC, SP, 通用寄存器）                                │
│  2. 信号掩码                                                            │
│  3. FPU 状态                                                            │
│                                                                         │
│  保存位置: 用户栈上的信号帧                                              │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**为什么信号帧在用户栈上？**

1. **用户态访问**：信号处理程序运行在用户态，需要访问信号帧
2. **透明性**：对内核透明，不需要额外的内核数据结构
3. **可移植性**：符合 POSIX 标准

### 1.3 使用场景（When）

| 场景 | 调用者 | 说明 |
|------|--------|------|
| 进程间信号 | PM（进程管理器） | kill() 系统调用 |
| 定时器信号 | 内核 | SIGALRM, SIGVTALRM |
| 异常信号 | 内核 | SIGSEGV, SIGFPE |
| 子进程状态 | PM | SIGCHLD |

---

## 二、逐行讲解

### 2.1 文件头注释（第 1-9 行）

```c
/* The kernel call that is implemented in this file:
 *	m_type: SYS_SIGSEND
 *
 * The parameters for this kernel call are:
 * 	m_sigcalls.endpt	# process to call signal handler
 *	m_sigcalls.sigctx	# pointer to sigcontext structure
 *
 */
```

**翻译**：
```
本文件实现的内核调用：
  m_type: SYS_SIGSEND

此内核调用的参数：
  m_sigcalls.endpt   - 要调用信号处理程序的进程
  m_sigcalls.sigctx  - sigcontext 结构的指针
```

**参数详解**：

| 字段 | 方向 | 类型 | 含义 |
|------|------|------|------|
| `endpt` | 输入 | `endpoint_t` | 目标进程端点 |
| `sigctx` | 输入 | `vir_bytes` | sigmsg 结构的指针（在调用者空间） |

### 2.2 头文件包含（第 11-14 行）

```c
#include "kernel/system.h"
#include <signal.h>
#include <string.h>
```

| 头文件 | 作用 |
|--------|------|
| `"kernel/system.h"` | 系统调用框架、进程结构定义 |
| `<signal.h>` | 信号相关定义（sigset_t 等） |
| `<string.h>` | memset, memcpy 函数 |

### 2.3 条件编译（第 16 行）

```c
#if USE_SIGSEND
```

**设计原因**：允许在编译时禁用此功能以减小内核大小。

### 2.4 函数注释（第 18-21 行）

```c
/*===========================================================================*
 *			      do_sigsend				     *
 *===========================================================================*/
int do_sigsend(struct proc * caller, message * m_ptr)
{
/* Handle sys_sigsend, POSIX-style signal handling. */
```

**翻译注释**：`Handle sys_sigsend, POSIX-style signal handling` = "处理 sys_sigsend，POSIX 风格的信号处理"

### 2.5 局部变量声明（第 23-29 行）

```c
  struct sigmsg smsg;
  register struct proc *rp;
  struct sigframe_sigcontext fr, *frp;
  int proc_nr, r;
#if defined(__i386__)
  reg_t new_fp;
#endif
```

| 变量 | 类型 | 大小 | 用途 |
|------|------|------|------|
| `smsg` | `struct sigmsg` | ~40 字节 | 信号消息结构（从用户空间复制） |
| `rp` | `struct proc *` | 8 字节 | 目标进程指针 |
| `fr` | `struct sigframe_sigcontext` | ~512 字节 | 信号帧结构（内核栈上） |
| `frp` | `struct sigframe_sigcontext *` | 8 字节 | 用户栈上的信号帧指针 |
| `proc_nr` | `int` | 4 字节 | 进程号 |
| `r` | `int` | 4 字节 | 返回值 |
| `new_fp` | `reg_t` | 4/8 字节 | 新的帧指针（x86） |

**内存布局**：

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号帧在用户栈上的布局                                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  用户栈（高地址在上）:                                                   │
│  ┌──────────────────┐                                                   │
│  │ 原始栈内容        │                                                   │
│  ├──────────────────┤                                                   │
│  │ ...              │                                                   │
│  ├──────────────────┤ ← 原始 SP                                         │
│  │ sigframe_sigcontext │  ← frp（新 SP）                                │
│  │  ├─ sf_sc (sigcontext) │                                             │
│  │  │  ├─ 通用寄存器    │                                                │
│  │  │  ├─ PC, SP       │                                                │
│  │  │  ├─ 信号掩码     │                                                │
│  │  │  └─ FPU 状态     │                                                │
│  │  ├─ sf_fp         │                                                  │
│  │  ├─ sf_signum     │                                                  │
│  │  └─ sf_ra_sigreturn │                                                │
│  └──────────────────┘                                                   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.6 参数验证（第 31-34 行）

```c
  if (!isokendpt(m_ptr->m_sigcalls.endpt, &proc_nr)) return EINVAL;
  if (iskerneln(proc_nr)) return EPERM;
  rp = proc_addr(proc_nr);
```

**验证步骤**：
1. `isokendpt()` - 验证端点有效并获取进程号
2. `iskerneln()` - 检查是否是内核进程（不能向内核进程发信号）
3. `proc_addr()` - 获取进程结构指针

**设计原因**：
- 内核进程没有用户态，无法处理信号
- 防止向无效进程发送信号

### 2.7 复制 sigmsg 结构（第 36-40 行）

```c
  /* Get the sigmsg structure into our address space.  */
  if ((r = data_copy_vmcheck(caller, caller->p_endpoint,
		(vir_bytes)m_ptr->m_sigcalls.sigctx, KERNEL,
		(vir_bytes)&smsg, (phys_bytes) sizeof(struct sigmsg))) != OK)
	return r;
```

**翻译注释**：`Get the sigmsg structure into our address space` = "将 sigmsg 结构复制到我们的地址空间"

**sigmsg 结构内容**：
```c
struct sigmsg {
    int sm_signo;           // 信号编号
    sigset_t sm_mask;       // 信号掩码
    vir_bytes sm_sighandler; // 信号处理程序地址
    vir_bytes sm_sigreturn;  // sigreturn 存根地址
    vir_bytes sm_stkptr;     // 栈指针（输出）
};
```

**data_copy_vmcheck 说明**：
- 检查虚拟内存是否可访问
- 如果进程被换出，返回 VMSUSPEND

### 2.8 警告注释（第 42-45 行）

```c
  /* WARNING: the following code may be run more than once even for a single
   * signal delivery. Do not change registers here. See the comment below.
   */
```

**翻译注释**：
```
警告：以下代码可能对单个信号投递运行多次。
不要在这里修改寄存器。参见下面的注释。
```

**设计原因**：
- `data_copy_vmcheck` 可能返回 VMSUSPEND
- 进程被换入后会重新执行
- 如果提前修改寄存器，会导致状态不一致

### 2.9 计算信号帧位置（第 47-49 行）

```c
  /* Compute the user stack pointer where sigframe will start. */
  smsg.sm_stkptr = arch_get_sp(rp);
  frp = (struct sigframe_sigcontext *) smsg.sm_stkptr - 1;
```

**翻译注释**：`Compute the user stack pointer where sigframe will start` = "计算信号帧开始的用户栈指针"

**计算逻辑**：
- `arch_get_sp(rp)` - 获取进程当前的栈指针
- `- 1` - 向下移动一个 sigframe_sigcontext 大小

**内存布局**：
```
原始 SP:     0x7fff1000
frp:         0x7fff1000 - sizeof(sigframe_sigcontext)
             = 0x7fff0e00（示例）
```

### 2.10 初始化信号帧（第 51-53 行）

```c
  /* Copy the registers to the sigcontext structure. */
  memset(&fr, 0, sizeof(fr));
  fr.sf_scp = &frp->sf_sc;
```

**翻译注释**：`Copy the registers to the sigcontext structure` = "将寄存器复制到 sigcontext 结构"

**初始化**：
- `memset` - 清零整个信号帧
- `sf_scp` - 设置 sigcontext 指针（指向用户栈上的位置）

### 2.11 x86 架构寄存器保存（第 55-77 行）

```c
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
```

**寄存器映射**：

| sigcontext 字段 | 进程寄存器 | 用途 |
|----------------|-----------|------|
| `sc_gs` | `p_reg.gs` | 数据段寄存器 GS |
| `sc_fs` | `p_reg.fs` | 数据段寄存器 FS |
| `sc_es` | `p_reg.es` | 数据段寄存器 ES |
| `sc_ds` | `p_reg.ds` | 数据段寄存器 DS |
| `sc_edi` | `p_reg.di` | 目标索引寄存器 |
| `sc_esi` | `p_reg.si` | 源索引寄存器 |
| `sc_ebp` | `p_reg.fp` | 帧指针 |
| `sc_ebx` | `p_reg.bx` | 基址寄存器 |
| `sc_edx` | `p_reg.dx` | 数据寄存器 |
| `sc_ecx` | `p_reg.cx` | 计数寄存器 |
| `sc_eax` | `p_reg.retreg` | 累加器/返回值 |
| `sc_eip` | `p_reg.pc` | 程序计数器 |
| `sc_cs` | `p_reg.cs` | 代码段寄存器 |
| `sc_eflags` | `p_reg.psw` | 处理器状态字 |
| `sc_esp` | `p_reg.sp` | 栈指针 |
| `sc_ss` | `p_reg.ss` | 栈段寄存器 |

**特殊字段**：
- `sf_signum` - 信号编号
- `sf_ra_sigreturn` - sigreturn 存根地址（信号处理程序返回时调用）
- `sf_ra` - 原始返回地址

### 2.12 检查 trap_style（第 79-83 行）

```c
  fr.sf_sc.trap_style = rp->p_seg.p_kern_trap_style;

  if (fr.sf_sc.trap_style == KTS_NONE) {
  	printf("do_sigsend: sigsend an unsaved process\n");
	return EINVAL;
  }
```

**trap_style 说明**：

| 值 | 含义 |
|----|------|
| `KTS_NONE` | 进程状态未保存（无效） |
| `KTS_INT` | 中断上下文 |
| `KTS_SYSCALL` | 系统调用上下文 |
| `KTS_EXCEPTION` | 异常上下文 |

**设计原因**：
- 进程状态必须在信号处理前保存
- 未保存状态的进程不能接收信号

### 2.13 FPU 状态保存（第 85-90 行）

```c
  if (proc_used_fpu(rp)) {
	/* save the FPU context before saving it to the sig context */
	save_fpu(rp);
	memcpy(&fr.sf_sc.sc_fpu_state, rp->p_seg.fpu_state, FPU_XFP_SIZE);
  }
#endif
```

**翻译注释**：`save the FPU context before saving it to the sig context` = "在保存到 sigcontext 之前保存 FPU 上下文"

**FPU 保存流程**：
1. `proc_used_fpu()` - 检查进程是否使用过 FPU
2. `save_fpu()` - 将 FPU 寄存器保存到进程结构
3. `memcpy()` - 复制到信号帧

### 2.14 ARM 架构寄存器保存（第 92-110 行）

```c
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

**ARM 寄存器映射**：

| sigcontext 字段 | 进程寄存器 | 用途 |
|----------------|-----------|------|
| `sc_spsr` | `p_reg.psr` | 程序状态寄存器 |
| `sc_r0` | `p_reg.retreg` | 参数/返回值 |
| `sc_r1-r3` | `p_reg.r1-r3` | 参数寄存器 |
| `sc_r4-r11` | `p_reg.r4-r11` | 通用寄存器 |
| `sc_r12` | `p_reg.r12` | IP 寄存器 |
| `sc_usr_sp` | `p_reg.sp` | 用户栈指针 |
| `sc_usr_lr` | `p_reg.lr` | 用户链接寄存器 |
| `sc_pc` | `p_reg.pc` | 程序计数器 |

### 2.15 完成 sigcontext 初始化（第 112-115 行）

```c
  /* Finish the sigcontext initialization. */
  fr.sf_sc.sc_mask = smsg.sm_mask;
  fr.sf_sc.sc_flags = rp->p_misc_flags & MF_FPU_INITIALIZED;
  fr.sf_sc.sc_magic = SC_MAGIC;
```

**翻译注释**：`Finish the sigcontext initialization` = "完成 sigcontext 初始化"

**字段说明**：
- `sc_mask` - 信号掩码（阻塞哪些信号）
- `sc_flags` - FPU 初始化标志
- `sc_magic` - 魔数（用于验证 sigcontext 完整性）

### 2.16 初始化信号帧（第 117-118 行）

```c
  /* Initialize the sigframe structure. */
  fpu_sigcontext(rp, &fr, &fr.sf_sc);
```

**翻译注释**：`Initialize the sigframe structure` = "初始化 sigframe 结构"

**fpu_sigcontext 函数**：设置 FPU 相关的信号帧字段。

### 2.17 复制信号帧到用户栈（第 120-124 行）

```c
  /* Copy the sigframe structure to the user's stack. */
  if ((r = data_copy_vmcheck(caller, KERNEL, (vir_bytes)&fr,
		m_ptr->m_sigcalls.endpt, (vir_bytes)frp,
		(vir_bytes)sizeof(struct sigframe_sigcontext))) != OK)
      return r;
```

**翻译注释**：`Copy the sigframe structure to the user's stack` = "将 sigframe 结构复制到用户栈"

**复制方向**：
- 源：内核栈上的 `fr`
- 目标：用户栈上的 `frp`

### 2.18 关键警告注释（第 126-132 行）

```c
  /* WARNING: up to the statement above, the code may run multiple times, since
   * copying out the frame/context may fail with VMSUSPEND the first time. For
   * that reason, changes to process registers *MUST* be deferred until after
   * this last copy -- otherwise, these changes will be made several times,
   * possibly leading to corrupted process state.
   */
```

**翻译注释**：
```
警告：直到上面的语句，代码可能运行多次，因为复制帧/上下文
第一次可能因 VMSUSPEND 失败。因此，对进程寄存器的修改*必须*
延迟到最后一次复制之后——否则，这些修改会被执行多次，
可能导致进程状态损坏。
```

**设计原因**：
- `data_copy_vmcheck` 可能返回 VMSUSPEND
- 进程被换入后会重新执行整个函数
- 寄存器修改必须在最后一步

### 2.19 设置进程执行信号处理程序（第 134-136 行）

```c
  /* Reset user registers to execute the signal handler. */
  rp->p_reg.sp = (reg_t) frp;
  rp->p_reg.pc = (reg_t) smsg.sm_sighandler;
```

**翻译注释**：`Reset user registers to execute the signal handler` = "重置用户寄存器以执行信号处理程序"

**设置内容**：
- `sp` - 指向用户栈上的信号帧
- `pc` - 指向信号处理程序入口

### 2.20 x86 特定设置（第 138-140 行）

```c
#if defined(__i386__)
  rp->p_reg.fp = new_fp;
```

**帧指针设置**：设置新的帧指针，指向信号帧中的 sf_fp 字段。

### 2.21 ARM 特定设置（第 141-154 行）

```c
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

**翻译注释**：
- `use the ARM link register to set the return address from the signal handler` = "使用 ARM 链接寄存器设置从信号处理程序返回的地址"
- `pass signal handler parameters in registers` = "通过寄存器传递信号处理程序参数"

**ARM 信号处理程序参数**：
- `r0` = 信号编号
- `r1` = sf_code（通常为 0）
- `r2` = sigcontext 指针
- `lr` = sigreturn 存根地址

### 2.22 清除 FPU 标志（第 156-157 行）

```c
  /* Signal handler should get clean FPU. */
  rp->p_misc_flags &= ~MF_FPU_INITIALIZED;
```

**翻译注释**：`Signal handler should get clean FPU` = "信号处理程序应该获得干净的 FPU"

**设计原因**：信号处理程序开始时 FPU 状态应该是初始状态。

### 2.23 警告检查（第 159-163 行）

```c
  if(!RTS_ISSET(rp, RTS_PROC_STOP)) {
	printf("system: warning: sigsend a running process\n");
	printf("caller stack: ");
	proc_stacktrace(caller);
  }
```

**检查原因**：正常情况下，进程应该在停止状态才能接收信号。

### 2.24 返回成功（第 165 行）

```c
  return OK;
}
```

### 2.25 条件编译结束（第 167 行）

```c
#endif /* USE_SIGSEND */
```

---

## 三、信号处理完整流程

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号处理完整流程                                                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. 信号产生                                                            │
│     └── kill(), 异常, 定时器 → PM → SYS_SIGSEND                        │
│                                                                         │
│  2. do_sigsend 处理                                                     │
│     ├── 保存当前寄存器到 sigcontext                                     │
│     ├── 在用户栈上构建 sigframe                                         │
│     ├── 设置 PC = 信号处理程序                                          │
│     └── 设置 SP = sigframe 地址                                         │
│                                                                         │
│  3. 信号处理程序执行                                                     │
│     ├── 用户态执行                                                      │
│     └── 返回时调用 sigreturn 存根                                       │
│                                                                         │
│  4. SYS_SIGRETURN 处理                                                  │
│     ├── 从 sigcontext 恢复寄存器                                        │
│     └── 进程恢复执行                                                    │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 四、Rust 重构建议

```rust
use core::mem::size_of;

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Sigcontext {
    pub sc_gs: u32,
    pub sc_fs: u32,
    pub sc_es: u32,
    pub sc_ds: u32,
    pub sc_edi: u32,
    pub sc_esi: u32,
    pub sc_ebp: u32,
    pub sc_ebx: u32,
    pub sc_edx: u32,
    pub sc_ecx: u32,
    pub sc_eax: u32,
    pub sc_eip: u32,
    pub sc_cs: u32,
    pub sc_eflags: u32,
    pub sc_esp: u32,
    pub sc_ss: u32,
    pub sc_mask: Sigset,
    pub sc_flags: u32,
    pub sc_magic: u32,
    pub trap_style: u32,
    pub sc_fpu_state: [u8; FPU_XFP_SIZE],
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SigframeSigcontext {
    pub sf_sc: Sigcontext,
    pub sf_fp: u32,
    pub sf_signum: i32,
    pub sf_scp: *const Sigcontext,
    pub sf_scpcopy: *const Sigcontext,
    pub sf_ra_sigreturn: *const u8,
    pub sf_ra: *const u8,
}

pub fn do_sigsend(caller: &Proc, m_ptr: &Message) -> Result<(), Errno> {
    let proc_nr = isokendpt(m_ptr.endpt)?;
    if iskerneln(proc_nr) {
        return Err(Errno::EPERM);
    }
    let rp = proc_addr(proc_nr);

    let smsg: Sigmsg = data_copy_vmcheck(
        caller,
        caller.p_endpoint,
        m_ptr.sigctx,
        Endpoint::KERNEL,
        size_of::<Sigmsg>(),
    )?;

    let stkptr = arch_get_sp(rp);
    let frp = (stkptr - size_of::<SigframeSigcontext>()) as *mut SigframeSigcontext;

    let mut fr = SigframeSigcontext::default();
    fr.sf_scp = &unsafe { (*frp).sf_sc };

    fr.sf_sc.sc_gs = rp.p_reg.gs;
    fr.sf_sc.sc_fs = rp.p_reg.fs;
    fr.sf_sc.sc_eip = rp.p_reg.pc;
    fr.sf_sc.sc_esp = rp.p_reg.sp;

    fr.sf_sc.trap_style = rp.p_seg.p_kern_trap_style;
    if fr.sf_sc.trap_style == TrapStyle::NONE {
        return Err(Errno::EINVAL);
    }

    if proc_used_fpu(rp) {
        save_fpu(rp);
        fr.sf_sc.sc_fpu_state.copy_from_slice(&rp.p_seg.fpu_state);
    }

    fr.sf_sc.sc_mask = smsg.sm_mask;
    fr.sf_sc.sc_magic = SC_MAGIC;

    data_copy_vmcheck(
        caller,
        Endpoint::KERNEL,
        &fr as *const _ as u64,
        m_ptr.endpt,
        frp as u64,
        size_of::<SigframeSigcontext>(),
    )?;

    rp.p_reg.sp = frp as u64;
    rp.p_reg.pc = smsg.sm_sighandler as u64;

    Ok(())
}
```

---

## 五、要点总结

### 核心知识点

1. **信号帧结构**：
   - 保存在用户栈上
   - 包含 sigcontext（寄存器状态）
   - 包含信号处理元数据

2. **多架构支持**：
   - x86：段寄存器 + 通用寄存器
   - ARM：通用寄存器 + 特殊寄存器

3. **VMSUSPEND 处理**：
   - 复制可能失败并重试
   - 寄存器修改必须延迟到最后

---

## 六、灾难预演

### 场景 1：如果信号帧写入失败

```
后果：
1. 进程状态已修改但信号帧未写入
2. 进程执行垃圾数据
3. 系统崩溃
```

**防护**：使用 `data_copy_vmcheck`，失败时不修改寄存器。

### 场景 2：如果向内核进程发信号

```
后果：
1. 内核进程没有用户栈
2. 无法构建信号帧
3. 内核崩溃
```

**防护**：`iskerneln()` 检查。

### 场景 3：如果进程状态未保存

```
后果：
1. 寄存器值无效
2. 信号处理程序返回后状态错误
3. 进程行为异常
```

**防护**：检查 `trap_style != KTS_NONE`。

---

## 七、互动自测

1. **问题**：为什么信号帧必须在用户栈上？
   **答案**：信号处理程序运行在用户态，需要访问信号帧。如果信号帧在内核空间，用户态无法访问。

2. **问题**：为什么寄存器修改必须在最后？
   **答案**：`data_copy_vmcheck` 可能因 VMSUSPEND 失败并重试。如果提前修改寄存器，重试时会导致状态不一致。

3. **问题**：`sf_ra_sigreturn` 的作用是什么？
   **答案**：信号处理程序返回时的地址。当信号处理程序返回时，会跳转到 sigreturn 存根，执行 SYS_SIGRETURN 系统调用恢复进程状态。

---

*讲解者：Minix-rs 学习助手*
