# kernel/system/do_mcontext.c 逐行讲解

> **文件路径**: `minix3/minix/kernel/system/do_mcontext.c`
> **核心功能**: 实现机器上下文的获取与设置系统调用
> **系统调用号**: SYS_GETMCONTEXT, SYS_SETMCONTEXT

---

## 一、文件概述

### 1.1 功能说明（是什么）

`do_mcontext.c` 实现了两个系统调用：`SYS_GETMCONTEXT` 和 `SYS_SETMCONTEXT`，用于获取和设置进程的**机器上下文**（machine context）。

**什么是机器上下文？**

机器上下文是进程执行状态的完整快照，包含：
- **通用寄存器**：eax, ebx, ecx, edx, esi, edi, ebp, esp, eip, eflags
- **段寄存器**：cs, ds, es, fs, gs, ss
- **浮点寄存器**：x87 FPU 寄存器、XMM 寄存器（SSE）

**生活类比**：想象你在玩一个电子游戏，机器上下文就是"存档"——它记录了游戏角色的所有状态（位置、装备、技能点等），让你可以在之后恢复到这个状态继续游戏。

### 1.2 设计原因（为什么）

**微内核架构的必然需求**：

1. **用户态线程库支持**：像 pthreads 这样的用户态线程库需要在不陷入内核的情况下切换线程。这需要能够保存和恢复线程的执行状态。

2. **协程/纤程实现**：现代异步编程框架（如 Go 的 goroutine、Rust 的 async/await）需要在用户态管理多个执行上下文。

3. **信号处理**：信号处理函数执行完毕后需要恢复原来的执行状态。

4. **调试器支持**：调试器需要读取和修改被调试进程的寄存器状态。

### 1.3 应用场景（什么情景使用）

| 使用者 | 系统调用 | 用途 |
|--------|----------|------|
| pthreads 库 | GET/SETMCONTEXT | 用户态线程切换 |
| 协程库 | GET/SETMCONTEXT | 协程上下文切换 |
| 调试器 | GETMCONTEXT | 读取进程寄存器 |
| 信号处理 | SETMCONTEXT | 恢复信号前的状态 |

---

## 二、逐行详细讲解

### 2.1 文件头注释

```c
/* The kernel calls that are implemented in this file:
 *   m_type:	SYS_SETMCONTEXT
 *   m_type:	SYS_GETMCONTEXT
 *
 * The parameters for SYS_SETMCONTEXT kernel call are:
 *   m_lsys_krn_sys_setmcontext.endpt	# proc endpoint doing call
 *   m_lsys_krn_sys_setmcontext.ctx_ptr	# pointer to mcontext structure
 *
 * The parameters for SYS_GETMCONTEXT kernel call are:
 *   m_lsys_krn_sys_getmcontext.endpt	# proc endpoint doing call
 *   m_lsys_krn_sys_getmcontext.ctx_ptr	# pointer to mcontext structure
 */
```

**逐行解释**：

- **第1-3行**：说明本文件实现两个内核调用：`SYS_SETMCONTEXT`（设置上下文）和 `SYS_GETMCONTEXT`（获取上下文）。

- **第5-8行**：描述 `SYS_SETMCONTEXT` 的参数：
  - `endpt`：目标进程的端点号。
  - `ctx_ptr`：指向 `mcontext_t` 结构的指针（用户空间地址）。

- **第10-13行**：描述 `SYS_GETMCONTEXT` 的参数，格式相同。

**设计思路**：这两个系统调用是对称的——一个读取状态，一个写入状态。参数结构也保持一致，便于使用。

---

### 2.2 头文件包含

```c
#include "kernel/system.h"
#include <string.h>
#include <assert.h>
#include <machine/mcontext.h>

#if USE_MCONTEXT 
```

**逐行解释**：

- **第1行**：`#include "kernel/system.h"` — 包含内核系统调用的核心定义，如 `struct proc`、`message` 等。

- **第2行**：`#include <string.h>` — 包含 `memcpy()` 函数，用于复制 FPU 状态。

- **第3行**：`#include <assert.h>` — 包含 `assert()` 宏，用于运行时验证。

- **第4行**：`#include <machine/mcontext.h>` — 包含 `mcontext_t` 结构体定义。这是**架构相关**的头文件，不同 CPU 架构有不同的实现。

- **第6行**：`#if USE_MCONTEXT` — 条件编译开关。如果系统配置不需要机器上下文支持，可以禁用此功能以减小内核体积。

---

### 2.3 mcontext_t 结构体详解

在深入代码之前，先理解 `mcontext_t` 的结构（以 i386 架构为例）：

```c
typedef struct {
	__gregset_t	__gregs;      /* 通用寄存器，19个int */
	__fpregset_t	__fpregs;     /* 浮点寄存器 */
	__greg_t	_mc_tlsbase;  /* TLS基地址 */
#ifdef __minix
	int	mc_magic;            /* 魔数，用于验证 */
	int	mc_flags;            /* 状态标志 */
#endif
} mcontext_t;
```

**通用寄存器布局**：

```c
#define _NGREG		19
typedef	int		__greg_t;
typedef	__greg_t	__gregset_t[_NGREG];

#define _REG_GS		0    /* 段寄存器 */
#define _REG_FS		1
#define _REG_ES		2
#define _REG_DS		3
#define _REG_EDI	4    /* 通用寄存器 */
#define _REG_ESI	5
#define _REG_EBP	6    /* 栈帧指针 */
#define _REG_ESP	7    /* 栈指针 */
#define _REG_EBX	8
#define _REG_EDX	9
#define _REG_ECX	10
#define _REG_EAX	11   /* 累加器，返回值 */
#define _REG_TRAPNO	12   /* 陷阱号 */
#define _REG_ERR	13   /* 错误码 */
#define _REG_EIP	14   /* 指令指针 */
#define _REG_CS		15   /* 代码段 */
#define _REG_EFL	16   /* 标志寄存器 */
#define _REG_UESP	17   /* 用户栈指针 */
#define _REG_SS		18   /* 栈段 */
```

**内存图示**：

```
mcontext_t 结构体 (i386):
┌─────────────────────────────────────────────────────────────┐
│ __gregs[19] (76 字节)                                       │
│ ┌────────┬────────┬────────┬────────┬────────┬────────┐    │
│ │ GS(0)  │ FS(1)  │ ES(2)  │ DS(3)  │ EDI(4) │ ESI(5) │    │
│ ├────────┼────────┼────────┼────────┼────────┼────────┤    │
│ │ EBP(6) │ ESP(7) │ EBX(8) │ EDX(9) │ ECX(10)│ EAX(11)│    │
│ ├────────┼────────┼────────┼────────┼────────┼────────┤    │
│ │TRAP(12)│ ERR(13)│ EIP(14)│ CS(15) │ EFL(16)│ UESP(17)│   │
│ ├────────┴────────┴────────┴────────┴────────┴────────┤    │
│ │ SS(18)                                               │    │
│ └─────────────────────────────────────────────────────┘    │
├─────────────────────────────────────────────────────────────┤
│ __fpregs (644 字节)                                         │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ __fp_reg_set (联合体)                                   ││
│ │ - __fpchip_state: x87 格式 (108 字节)                   ││
│ │ - __fp_xmm_state: FXSAVE 格式 (512 字节) ← 常用         ││
│ │ - __fp_fpregs: 原始数组 (512 字节)                      ││
│ └─────────────────────────────────────────────────────────┘│
│ __fp_pad[33] (132 字节填充)                                 │
├─────────────────────────────────────────────────────────────┤
│ _mc_tlsbase (4 字节)                                        │
├─────────────────────────────────────────────────────────────┤
│ mc_magic (4 字节)                                           │
├─────────────────────────────────────────────────────────────┤
│ mc_flags (4 字节)                                           │
│ - _MC_FPU_SAVED (0x001): FPU 状态有效                       │
└─────────────────────────────────────────────────────────────┘
总大小 ≈ 800+ 字节
```

---

### 2.4 do_getmcontext 函数 - 开头

```c
/*===========================================================================*
 *			      do_getmcontext				     *
 *===========================================================================*/
int do_getmcontext(struct proc * caller, message * m_ptr)
{
/* Retrieve machine context of a process */

  register struct proc *rp;
  int proc_nr, r;
  mcontext_t mc;
```

**逐行解释**：

- **第1-3行**：函数头注释，使用 Minix 标准格式。

- **第4行**：`int do_getmcontext(struct proc * caller, message * m_ptr)` — 函数签名。
  - `caller`：调用者进程指针。
  - `m_ptr`：消息指针，包含请求参数。
  - 返回值：成功返回 `OK`，失败返回错误码。

- **第5行**：注释说明功能——获取进程的机器上下文。

- **第6行**：`register struct proc *rp;` — 声明进程指针。
  - `register` 关键字提示编译器将变量放入寄存器以优化访问速度（现代编译器通常忽略此提示）。

- **第7行**：`int proc_nr, r;` — 声明进程号和返回值变量。

- **第8行**：`mcontext_t mc;` — 声明本地机器上下文结构体。
  - **内存位置**：栈上，约 800 字节。
  - **生命周期**：函数调用期间。

**栈帧布局**：

```
do_getmcontext 栈帧:
┌─────────────────────────────────┐ ← 高地址
│ 返回地址                         │
├─────────────────────────────────┤
│ caller (指针, 4 字节)            │
│ m_ptr (指针, 4 字节)             │
├─────────────────────────────────┤
│ rp (指针, 4 字节)                │
│ proc_nr (4 字节)                 │
│ r (4 字节)                       │
├─────────────────────────────────┤
│ mc (mcontext_t, ~800 字节)       │ ← 占用大部分栈空间
│   __gregs[19] (76 字节)          │
│   __fpregs (644 字节)            │
│   _mc_tlsbase (4 字节)           │
│   mc_magic (4 字节)              │
│   mc_flags (4 字节)              │
└─────────────────────────────────┘ ← 低地址 (栈顶)
```

---

### 2.5 参数验证

```c
  if (!isokendpt(m_ptr->m_lsys_krn_sys_getmcontext.endpt, &proc_nr))
	return(EINVAL);
  if (iskerneln(proc_nr)) return(EPERM);
  rp = proc_addr(proc_nr);
```

**逐行解释**：

- **第1-2行**：`if (!isokendpt(...)) return(EINVAL);` — 验证端点号有效性。
  - `isokendpt()` 检查端点号是否有效，并转换为进程槽位号。
  - 如果无效，返回 `EINVAL`（无效参数）。

- **第3行**：`if (iskerneln(proc_nr)) return(EPERM);` — 检查是否是内核进程。
  - `iskerneln()` 检查进程号是否属于内核任务。
  - 内核任务的状态不应该被用户态读取，返回 `EPERM`（权限不足）。

- **第4行**：`rp = proc_addr(proc_nr);` — 获取进程指针。
  - `proc_addr()` 是宏，展开为 `(&(proc[NR_TASKS + (n)]))`。

**安全考虑**：

1. **端点验证**：防止恶意进程传入非法端点号导致越界访问。

2. **内核保护**：内核任务的上下文包含敏感信息，不应暴露给用户态。

---

### 2.6 FPU 状态检查（i386 特定）

```c
#if defined(__i386__)
  if (!proc_used_fpu(rp))
	return(OK);	/* No state to copy */
#endif
```

**逐行解释**：

- **第1行**：`#if defined(__i386__)` — 条件编译，仅对 x86 架构有效。

- **第2-3行**：`if (!proc_used_fpu(rp)) return(OK);` — 检查进程是否使用过 FPU。
  - `proc_used_fpu(p)` 是宏：`((p)->p_misc_flags & (MF_FPU_INITIALIZED))`
  - 如果进程从未使用 FPU，直接返回成功，不需要复制 FPU 状态。

**设计原因**：

FPU 状态保存是昂贵的操作：
- FXSAVE 指令需要保存 512 字节
- 如果进程从未使用 FPU，这些数据都是无效的
- 直接返回成功可以避免不必要的开销

**生活类比**：就像搬家时，如果某个房间从未使用过（比如空着的储藏室），就不需要打包里面的东西。

---

### 2.7 获取用户态 mcontext

```c
  /* Get the mcontext structure into our address space.  */
  if ((r = data_copy(m_ptr->m_lsys_krn_sys_getmcontext.endpt,
		m_ptr->m_lsys_krn_sys_getmcontext.ctx_ptr, KERNEL,
		(vir_bytes) &mc, (phys_bytes) sizeof(mcontext_t))) != OK)
	return(r);

  mc.mc_flags = 0;
```

**逐行解释**：

- **第1行**：注释说明目的——将用户态的 mcontext 结构复制到内核空间。

- **第2-5行**：`data_copy(...)` — 执行跨地址空间复制：
  - 源：用户进程的 `ctx_ptr` 地址。
  - 目的：内核栈上的 `mc` 变量。
  - 大小：`sizeof(mcontext_t)`。

- **第6行**：`mc.mc_flags = 0;` — 清除标志位。
  - 为什么要清除？因为我们接下来会根据实际情况重新设置这个标志。

**为什么需要先复制用户态的 mcontext？**

这是一个有趣的设计。`GETMCONTEXT` 的语义是"获取进程的当前状态"，但这里先读取了用户态提供的 mcontext 结构。原因是：

1. **部分更新**：用户可能只想更新部分字段，保留其他字段不变。
2. **结构体初始化**：确保 mcontext 结构体中的其他字段（如 mc_magic）被正确初始化。

**注意**：实际上，这个设计可能存在问题。更合理的做法应该是完全由内核填充 mcontext，而不是先读取用户态的值。

---

### 2.8 FPU 状态保存

```c
#if defined(__i386__)
  /* Copy FPU state */
  if (proc_used_fpu(rp)) {
	/* make sure that the FPU context is saved into proc structure first */
	save_fpu(rp);
	mc.mc_flags = (rp->p_misc_flags & MF_FPU_INITIALIZED) ? _MC_FPU_SAVED : 0;
	assert(sizeof(mc.__fpregs.__fp_reg_set) == FPU_XFP_SIZE);
	memcpy(&(mc.__fpregs.__fp_reg_set), rp->p_seg.fpu_state, FPU_XFP_SIZE);
  } 
#endif
```

**逐行解释**：

- **第1行**：`#if defined(__i386__)` — x86 架构特定代码。

- **第2行**：注释说明目的——复制 FPU 状态。

- **第3行**：`if (proc_used_fpu(rp))` — 检查进程是否使用过 FPU。

- **第5行**：`save_fpu(rp);` — **关键步骤！** 确保 FPU 状态已保存到进程结构中。
  - 如果进程当前正在使用 FPU（FPU 所有权属于该进程），需要执行 FXSAVE 指令将 FPU 状态保存到内存。
  - `save_fpu()` 的实现：
    ```c
    void save_fpu(struct proc *pr)
    {
        if (get_cpulocal_var(fpu_owner) == pr) {
            disable_fpu_exception();
            save_local_fpu(pr, TRUE /*retain*/);
        }
    }
    ```

- **第6行**：`mc.mc_flags = (...) ? _MC_FPU_SAVED : 0;` — 设置标志位。
  - `_MC_FPU_SAVED` 定义为 `0x001`。
  - 如果进程的 FPU 已初始化，设置此标志表示 mcontext 中包含有效的 FPU 状态。

- **第7行**：`assert(sizeof(mc.__fpregs.__fp_reg_set) == FPU_XFP_SIZE);` — 编译时验证。
  - 确保 mcontext 中的 FPU 缓冲区大小与内核期望的大小一致。
  - `FPU_XFP_SIZE` 通常是 512 字节（FXSAVE 格式）。

- **第8行**：`memcpy(...)` — 复制 FPU 状态。
  - 源：`rp->p_seg.fpu_state`（进程结构中的 FPU 状态缓冲区）。
  - 目的：`mc.__fpregs.__fp_reg_set`（mcontext 中的 FPU 状态字段）。

**FPU 状态保存流程图**：

```
进程使用 FPU:
┌─────────────────────────────────────────────────────────────┐
│ CPU FPU 寄存器                                              │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ ST0-ST7 (x87 栈寄存器)                                  ││
│ │ XMM0-XMM7 (SSE 寄存器)                                  ││
│ │ MXCSR (SSE 控制寄存器)                                  ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
                        │
                        │ save_fpu() → FXSAVE 指令
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ rp->p_seg.fpu_state (内核内存)                              │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ FXSAVE 格式 (512 字节)                                  ││
│ │ - FCW, FSW, FTW, FOP                                   ││
│ │ - FIP, FDP, FCS, FDS                                   ││
│ │ - ST0-ST7 (每16字节)                                    ││
│ │ - XMM0-XMM7 (每16字节)                                  ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
                        │
                        │ memcpy()
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ mc.__fpregs.__fp_reg_set (栈上)                             │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ 同上 (512 字节)                                         ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

---

### 2.9 返回 mcontext 到用户态

```c
  /* Copy the mcontext structure to the user's address space. */
  if ((r = data_copy(KERNEL, (vir_bytes) &mc,
	m_ptr->m_lsys_krn_sys_getmcontext.endpt,
	m_ptr->m_lsys_krn_sys_getmcontext.ctx_ptr,
	(phys_bytes) sizeof(mcontext_t))) != OK)
	return(r);

  return(OK);
}
```

**逐行解释**：

- **第1行**：注释说明目的——将 mcontext 复制回用户空间。

- **第2-6行**：`data_copy(...)` — 执行跨地址空间复制：
  - 源：内核栈上的 `mc` 变量。
  - 目的：用户进程的 `ctx_ptr` 地址。

- **第8行**：`return(OK);` — 返回成功。

**完整流程图**：

```
用户态                           内核态
┌──────────────────┐           ┌──────────────────┐
│ ctx_ptr          │           │ mc (栈变量)       │
│ ┌──────────────┐ │           │ ┌──────────────┐ │
│ │ 未初始化      │ │ ──读取──→ │ │ 初始化        │ │
│ └──────────────┘ │           │ │ 设置 mc_flags │ │
│                  │           │ │ 复制 FPU 状态 │ │
│                  │           │ └──────────────┘ │
│ ┌──────────────┐ │           │                  │
│ │ 完整 mcontext │ │ ←─写入─── │                  │
│ └──────────────┘ │           │                  │
└──────────────────┘           └──────────────────┘
```

---

### 2.10 do_setmcontext 函数 - 开头

```c
/*===========================================================================*
 *			      do_setmcontext				     *
 *===========================================================================*/
int do_setmcontext(struct proc * caller, message * m_ptr)
{
/* Set machine context of a process */

  register struct proc *rp;
  int proc_nr, r;
  mcontext_t mc;

  if (!isokendpt(m_ptr->m_lsys_krn_sys_setmcontext.endpt, &proc_nr)) return(EINVAL);
  rp = proc_addr(proc_nr);
```

**逐行解释**：

- 与 `do_getmcontext` 类似，但功能相反——设置进程的机器上下文。

- **注意**：`do_setmcontext` **没有** `iskerneln()` 检查！
  - 这意味着可以设置内核任务的上下文？
  - 实际上，这可能是一个潜在的安全问题，或者是因为调用者已经被验证为特权进程。

---

### 2.11 获取用户态 mcontext

```c
  /* Get the mcontext structure into our address space.  */
  if ((r = data_copy(m_ptr->m_lsys_krn_sys_setmcontext.endpt,
		m_ptr->m_lsys_krn_sys_setmcontext.ctx_ptr, KERNEL,
		(vir_bytes) &mc, (phys_bytes) sizeof(mcontext_t))) != OK)
	return(r);
```

**逐行解释**：

- 从用户态复制 mcontext 结构到内核空间。
- 这是 `SETMCONTEXT` 的核心——用户态提供新的上下文，内核将其应用到进程。

---

### 2.12 FPU 状态恢复

```c
#if defined(__i386__)
  /* Copy FPU state */
  if (mc.mc_flags & _MC_FPU_SAVED) {
	rp->p_misc_flags |= MF_FPU_INITIALIZED;
	assert(sizeof(mc.__fpregs.__fp_reg_set) == FPU_XFP_SIZE);
	memcpy(rp->p_seg.fpu_state, &(mc.__fpregs.__fp_reg_set), FPU_XFP_SIZE);
  } else
	rp->p_misc_flags &= ~MF_FPU_INITIALIZED;
  /* force reloading FPU in either case */
  release_fpu(rp);
#endif

  return(OK);
}

#endif
```

**逐行解释**：

- **第1行**：`#if defined(__i386__)` — x86 架构特定代码。

- **第2行**：注释说明目的——复制 FPU 状态。

- **第3行**：`if (mc.mc_flags & _MC_FPU_SAVED)` — 检查 mcontext 是否包含有效的 FPU 状态。

- **第4行**：`rp->p_misc_flags |= MF_FPU_INITIALIZED;` — 设置进程的 FPU 已初始化标志。

- **第5行**：`assert(...)` — 验证大小匹配。

- **第6行**：`memcpy(...)` — 复制 FPU 状态到进程结构。
  - 源：`mc.__fpregs.__fp_reg_set`（用户提供的 FPU 状态）。
  - 目的：`rp->p_seg.fpu_state`（进程的 FPU 状态缓冲区）。

- **第8行**：`else rp->p_misc_flags &= ~MF_FPU_INITIALIZED;` — 如果没有 FPU 状态，清除初始化标志。

- **第10行**：`release_fpu(rp);` — **关键步骤！** 释放 FPU 所有权。
  - `release_fpu()` 的实现：
    ```c
    void release_fpu(struct proc * p) {
        struct proc ** fpu_owner_ptr;
        fpu_owner_ptr = get_cpu_var_ptr(p->p_cpu, fpu_owner);
        if (*fpu_owner_ptr == p)
            *fpu_owner_ptr = NULL;
    }
    ```
  - 这确保下次进程使用 FPU 时，会从 `p_seg.fpu_state` 加载状态（通过 FXRSTOR 指令）。

- **第13行**：`return(OK);` — 返回成功。

**为什么需要 release_fpu？**

FPU 是共享资源，同一时刻只有一个进程可以"拥有" FPU：
1. 如果当前进程拥有 FPU，其状态在 CPU 寄存器中，不在内存中。
2. `release_fpu()` 放弃所有权，确保 FPU 状态在内存中（`p_seg.fpu_state`）。
3. 下次进程使用 FPU 时，会触发 Device Not Available 异常，内核会加载保存的状态。

**FPU 状态恢复流程图**：

```
用户态 mcontext:
┌─────────────────────────────────────────────────────────────┐
│ mc.__fpregs.__fp_reg_set                                    │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ FXSAVE 格式 (512 字节)                                  ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
                        │
                        │ memcpy()
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ rp->p_seg.fpu_state (内核内存)                              │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ FXSAVE 格式 (512 字节)                                  ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
                        │
                        │ release_fpu() → 放弃 FPU 所有权
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ 下次进程使用 FPU 时:                                        │
│ 1. 触发 #NM 异常                                           │
│ 2. 内核执行 FXRSTOR 从 p_seg.fpu_state 加载状态            │
│ 3. 进程继续执行，FPU 状态已恢复                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 三、理论关联

### 3.1 操作系统概念映射

| 代码结构 | 操作系统概念 | 说明 |
|----------|--------------|------|
| `mcontext_t` | 进程上下文 | 进程执行的完整状态 |
| `GETMCONTEXT` | 上下文保存 | 用于线程/协程切换 |
| `SETMCONTEXT` | 上下文恢复 | 恢复之前保存的状态 |
| `save_fpu()` | 惰性 FPU 保存 | 只在需要时保存 FPU 状态 |
| `release_fpu()` | FPU 所有权管理 | 确保状态一致性 |

### 3.2 惰性 FPU 上下文切换

Minix3 采用**惰性 FPU 上下文切换**（Lazy FPU Context Switch）：

1. **不主动保存**：进程切换时不保存 FPU 状态，因为大多数进程不使用 FPU。

2. **按需保存**：当新进程尝试使用 FPU 时，触发 #NM（Device Not Available）异常。

3. **内核介入**：异常处理程序保存旧进程的 FPU 状态，加载新进程的状态。

**优点**：
- 减少上下文切换开销（不使用 FPU 的进程不需要保存/恢复）
- 提高系统整体性能

---

## 四、Rust 实现与对比

### 4.1 数据结构定义

```rust
#![no_std]

use core::mem::size_of;

#[repr(C)]
pub struct Mcontext {
    pub gregs: [i32; 19],
    pub fpregs: Fpregset,
    pub tlsbase: i32,
    pub magic: i32,
    pub flags: i32,
}

#[repr(C)]
pub struct Fpregset {
    pub fp_reg_set: FpRegSet,
    pub fp_pad: [i32; 33],
}

#[repr(C)]
pub union FpRegSet {
    pub fp_xmm_state: [u8; 512],
    pub fp_fpregs: [i32; 128],
}

pub const _MC_FPU_SAVED: i32 = 0x001;
pub const MF_FPU_INITIALIZED: u32 = 0x1000;

pub const _REG_EIP: usize = 14;
pub const _REG_ESP: usize = 7;
pub const _REG_EAX: usize = 11;
```

### 4.2 错误处理对比

**C 语言版本**：
```c
if (!isokendpt(...)) return(EINVAL);
// 问题：错误码可能被忽略
```

**Rust 版本**：
```rust
#[derive(Debug)]
pub enum McontextError {
    InvalidEndpoint,
    KernelProcess,
    DataCopyFailed,
}

fn validate_endpoint(endpt: i32) -> Result<usize, McontextError> {
    if !is_ok_endpoint(endpt) {
        return Err(McontextError::InvalidEndpoint);
    }
    Ok(endpoint_to_slot(endpt))
}
```

### 4.3 完整函数实现

```rust
#![no_std]

extern crate alloc;

use core::mem;
use core::ptr;

pub struct Proc {
    pub p_misc_flags: u32,
    pub p_seg: ProcSeg,
    pub p_cpu: u32,
}

pub struct ProcSeg {
    pub fpu_state: [u8; FPU_XFP_SIZE],
}

pub const FPU_XFP_SIZE: usize = 512;

impl Proc {
    pub fn used_fpu(&self) -> bool {
        (self.p_misc_flags & MF_FPU_INITIALIZED) != 0
    }
}

pub struct Kernel {
    pub proc_table: [Proc; NR_PROCS],
}

pub const NR_PROCS: usize = 64;

impl Kernel {
    pub fn getmcontext(
        &mut self,
        proc_nr: usize,
        user_ctx_ptr: *const Mcontext,
    ) -> Result<(), McontextError> {
        if proc_nr >= NR_PROCS {
            return Err(McontextError::InvalidEndpoint);
        }
        
        let rp = &self.proc_table[proc_nr];
        
        if !rp.used_fpu() {
            return Ok(());
        }
        
        let mut mc = Mcontext::default();
        
        if rp.used_fpu() {
            self.save_fpu(rp);
            mc.flags = if (rp.p_misc_flags & MF_FPU_INITIALIZED) != 0 {
                _MC_FPU_SAVED
            } else {
                0
            };
            
            assert!(size_of::<FpRegSet>() == FPU_XFP_SIZE);
            
            unsafe {
                ptr::copy_nonoverlapping(
                    rp.p_seg.fpu_state.as_ptr(),
                    mc.fpregs.fp_reg_set.fp_xmm_state.as_mut_ptr(),
                    FPU_XFP_SIZE,
                );
            }
        }
        
        self.data_copy_to_user(user_ctx_ptr, &mc)?;
        
        Ok(())
    }
    
    pub fn setmcontext(
        &mut self,
        proc_nr: usize,
        user_ctx_ptr: *const Mcontext,
    ) -> Result<(), McontextError> {
        if proc_nr >= NR_PROCS {
            return Err(McontextError::InvalidEndpoint);
        }
        
        let rp = &mut self.proc_table[proc_nr];
        
        let mc = self.data_copy_from_user(user_ctx_ptr)?;
        
        if (mc.flags & _MC_FPU_SAVED) != 0 {
            rp.p_misc_flags |= MF_FPU_INITIALIZED;
            
            unsafe {
                ptr::copy_nonoverlapping(
                    mc.fpregs.fp_reg_set.fp_xmm_state.as_ptr(),
                    rp.p_seg.fpu_state.as_mut_ptr(),
                    FPU_XFP_SIZE,
                );
            }
        } else {
            rp.p_misc_flags &= !MF_FPU_INITIALIZED;
        }
        
        self.release_fpu(rp);
        
        Ok(())
    }
    
    fn save_fpu(&self, _proc: &Proc) {
        // FXSAVE 指令实现
    }
    
    fn release_fpu(&mut self, proc: &mut Proc) {
        // 清除 FPU 所有权
    }
    
    fn data_copy_to_user(&self, _ptr: *const Mcontext, _mc: &Mcontext) -> Result<(), McontextError> {
        Ok(())
    }
    
    fn data_copy_from_user(&self, _ptr: *const Mcontext) -> Result<Mcontext, McontextError> {
        Ok(Mcontext::default())
    }
}

impl Default for Mcontext {
    fn default() -> Self {
        Self {
            gregs: [0; 19],
            fpregs: Fpregset {
                fp_reg_set: FpRegSet { fp_xmm_state: [0; 512] },
                fp_pad: [0; 33],
            },
            tlsbase: 0,
            magic: 0,
            flags: 0,
        }
    }
}
```

### 4.4 Rust 优势分析

| 方面 | C 语言 | Rust |
|------|--------|------|
| 类型安全 | 联合体需要手动管理 | `union` 仍然存在，但有更安全的替代方案 |
| 错误处理 | 返回码可能被忽略 | `Result<T, E>` 强制处理 |
| 内存安全 | `memcpy` 可能越界 | `copy_nonoverlapping` 需要 `unsafe` 块 |
| 大小验证 | `assert` 运行时检查 | `const` 泛型可在编译时验证 |

---

## 五、要点总结

### 5.1 核心知识点

1. **机器上下文**：`mcontext_t` 包含进程执行的完整状态——通用寄存器和浮点寄存器。

2. **FPU 惰性保存**：只在进程实际使用 FPU 时才保存/恢复状态，减少上下文切换开销。

3. **所有权管理**：`release_fpu()` 确保 FPU 状态在内存中而非寄存器中，保证一致性。

### 5.2 设计亮点

- **条件编译**：`#if defined(__i386__)` 使代码可移植到不同架构。
- **惰性检查**：`proc_used_fpu()` 避免不必要的 FPU 状态保存。
- **大小验证**：`assert` 确保结构体大小匹配。

---

## 六、灾难预演

### 6.1 如果删除 `save_fpu()` 调用

**后果**：如果进程当前拥有 FPU 所有权，其 FPU 状态还在 CPU 寄存器中，不在内存中。`memcpy` 会复制旧的/未初始化的数据。

**现象**：恢复上下文后，进程的浮点运算结果错误，可能导致科学计算程序崩溃。

### 6.2 如果删除 `release_fpu()` 调用

**后果**：进程的 FPU 状态在内存中更新了，但 FPU 所有权没有释放。下次进程使用 FPU 时，不会触发状态加载。

**现象**：进程继续使用 CPU 寄存器中的旧 FPU 状态，而不是刚设置的值。

### 6.3 如果 FPU 状态大小不匹配

**后果**：`assert` 会触发，内核恐慌。

**现象**：系统崩溃，需要重启。这是防御性编程的体现——尽早发现问题。

---

## 七、互动自测

### 问题 1：为什么 `do_getmcontext` 要先从用户态读取 mcontext？

<details>
<summary>点击查看答案</summary>

这是一个设计选择。可能的原因：
1. 保留用户态结构体中的其他字段（如 mc_magic）。
2. 部分更新语义——只更新寄存器部分，保留其他字段。

但实际上，更合理的做法可能是完全由内核填充 mcontext，避免潜在的安全问题。
</details>

### 问题 2：`proc_used_fpu()` 检查的是什么？

<details>
<summary>点击查看答案</summary>

检查进程是否曾经使用过 FPU。这是通过 `MF_FPU_INITIALIZED` 标志实现的：
- 如果进程从未使用 FPU，这个标志不会被设置。
- 如果进程使用过 FPU，这个标志会被设置，表示进程结构中有有效的 FPU 状态。

这个检查避免了为不使用 FPU 的进程保存/恢复 FPU 状态的开销。
</details>

### 问题 3：为什么 `SETMCONTEXT` 后需要 `release_fpu()`？

<details>
<summary>点击查看答案</summary>

因为 FPU 状态可能在两个地方：
1. **CPU 寄存器**：如果进程当前拥有 FPU 所有权。
2. **内存**：如果进程不拥有 FPU 所有权。

`SETMCONTEXT` 更新了内存中的 FPU 状态。如果不释放所有权，进程下次使用 FPU 时不会从内存加载新状态，而是继续使用 CPU 寄存器中的旧状态。

`release_fpu()` 确保进程放弃 FPU 所有权，下次使用时会从内存加载新状态。
</details>
