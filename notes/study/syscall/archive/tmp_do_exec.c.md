# kernel/system/do_exec.c 逐行讲解

> **文件路径**: `minix3/minix/kernel/system/do_exec.c`
> **核心功能**: exec 系统调用的内核实现

---

## 文件概述

这个文件实现了 `exec()` 系统调用的内核部分。它负责：
1. **设置新程序入口**: 更新进程的指令指针和栈指针
2. **更新进程名称**: 保存程序名称用于调试
3. **重置 FPU 状态**: 新程序需要重新初始化 FPU

**核心概念**: exec 替换进程的执行映像，保持进程 ID 不变。

---

## 逐行讲解

### 文件头注释

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_EXEC
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_exec.endpt		(process that did exec call)
 *   m_lsys_krn_sys_exec.stack		(new stack pointer)
 *   m_lsys_krn_sys_exec.name		(pointer to program name)
 *   m_lsys_krn_sys_exec.ip		(new instruction pointer)
 *   m_lsys_krn_sys_exec.ps_str		(struct ps_strings *)
 */
```

**讲解**:
- **SYS_EXEC**: exec 系统调用号
- **endpt**: 执行 exec 的进程端点
- **stack**: 新的栈指针
- **ip**: 新的指令指针（程序入口）
- **name**: 程序名称指针
- **ps_str**: ps_strings 结构（用于 ps 命令）

---

### 头文件包含

```c
#include "kernel/system.h"
#include <string.h>
#include <minix/endpoint.h>

#if USE_EXEC
```

**讲解**:
- **USE_EXEC**: 配置选项，控制是否编译此功能

---

## do_exec 函数

### 函数签名和参数

```c
/*===========================================================================*
 *				do_exec					     *
 *===========================================================================*/
int do_exec(struct proc * caller, message * m_ptr)
{
/* Handle sys_exec().  A process has done a successful EXEC. Patch it up. */
  register struct proc *rp;
  int proc_nr;
  char name[PROC_NAME_LEN];
```

**讲解**:
- **caller**: 调用者进程（通常是 PM）
- **m_ptr**: 消息指针，包含 exec 参数
- **rp**: 目标进程指针
- **name**: 程序名称缓冲区

---

### 参数验证

```c
  if(!isokendpt(m_ptr->m_lsys_krn_sys_exec.endpt, &proc_nr))
	return EINVAL;

  rp = proc_addr(proc_nr);
```

**讲解**:
- **isokendpt**: 验证进程端点是否有效
- **proc_addr**: 根据进程编号获取进程指针

---

### 清除待投递消息

```c
  if(rp->p_misc_flags & MF_DELIVERMSG) {
	rp->p_misc_flags &= ~MF_DELIVERMSG;
  }
```

**讲解**:
- **MF_DELIVERMSG**: 有消息待投递标志
- exec 后旧消息无效
- 清除此标志

**为什么需要清除？**
- exec 替换了整个进程映像
- 旧的消息缓冲区地址无效
- 防止投递到错误地址

---

### 复制程序名称

```c
  /* Save command name for debugging, ps(1) output, etc. */
  if(data_copy(caller->p_endpoint, m_ptr->m_lsys_krn_sys_exec.name,
	KERNEL, (vir_bytes) name,
	(phys_bytes) sizeof(name) - 1) != OK)
  	strncpy(name, "<unset>", PROC_NAME_LEN);

  name[sizeof(name)-1] = '\0';
```

**讲解**:
- **data_copy**: 从用户空间复制数据到内核
- **caller->p_endpoint**: 源进程（PM）
- **KERNEL**: 目标是内核
- 失败时使用默认名称 "<unset>"

**程序名称用途**:
- 调试时识别进程
- ps 命令显示
- 系统日志

---

### 架构相关初始化

```c
  /* Set process state. */
  arch_proc_init(rp,
	(u32_t) m_ptr->m_lsys_krn_sys_exec.ip,
	(u32_t) m_ptr->m_lsys_krn_sys_exec.stack,
	(u32_t) m_ptr->m_lsys_krn_sys_exec.ps_str, name);
```

**讲解**:
- **arch_proc_init**: 架构相关的进程初始化
- 设置新的指令指针（程序入口）
- 设置新的栈指针
- 设置 ps_strings 结构
- 保存程序名称

**arch_proc_init 做什么？**
```c
// 设置寄存器
rp->p_reg.pc = ip;           // 程序计数器
rp->p_reg.sp = stack;        // 栈指针
// 清除其他寄存器
// 设置段寄存器
// 保存程序名称
strlcpy(rp->p_name, name, sizeof(rp->p_name));
```

---

### 清除接收状态

```c
  /* No reply to EXEC call */
  RTS_UNSET(rp, RTS_RECEIVING);
```

**讲解**:
- **RTS_RECEIVING**: 进程正在接收消息
- exec 后不需要回复 EXEC 调用
- 进程从新入口开始执行

**为什么不需要回复？**
- exec 替换了整个进程映像
- 旧的调用上下文不存在了
- 进程从新入口开始执行

---

### 重置 FPU 状态

```c
  /* Mark fpu_regs contents as not significant, so fpu
   * will be initialized, when it's used next time. */
  rp->p_misc_flags &= ~MF_FPU_INITIALIZED;
  /* force reloading FPU if the current process is the owner */
  release_fpu(rp);
  return(OK);
}
#endif /* USE_EXEC */
```

**讲解**:
- **MF_FPU_INITIALIZED**: FPU 已初始化标志
- 清除此标志，表示 FPU 状态无效
- **release_fpu**: 释放 FPU 所有权

**为什么需要重置 FPU？**
- 新程序不应该继承旧的 FPU 状态
- 首次使用 FPU 时会触发异常
- 异常处理程序会初始化 FPU

**FPU 状态重置流程**:
```
exec 执行
    ↓
清除 MF_FPU_INITIALIZED
    ↓
新程序首次使用 FPU 指令
    ↓
触发 #NM 异常
    ↓
内核初始化 FPU
    ↓
设置 MF_FPU_INITIALIZED
    ↓
继续执行
```

---

## 设计总结

### 1. exec 流程

```
PM 调用 do_exec
    ↓
验证进程端点
    ↓
清除待投递消息
    ↓
复制程序名称
    ↓
arch_proc_init 设置新状态
    ↓
清除接收状态
    ↓
重置 FPU 状态
    ↓
进程从新入口开始执行
```

### 2. exec vs fork

| 操作 | fork | exec |
|------|------|------|
| 进程 ID | 新进程，新 ID | 同一进程，ID 不变 |
| 内存 | 复制父进程 | 替换为新程序 |
| 寄存器 | 继承父进程 | 重置为新值 |
| FPU 状态 | 复制父进程 | 重置 |

### 3. 架构抽象

**设计思路**:
- 通用逻辑在 do_exec
- 架构相关逻辑在 arch_proc_init
- 便于移植到不同架构

---

## 要点总结

1. **设置新入口**: 更新进程的指令指针和栈指针
2. **清除旧状态**: 清除待投递消息和接收状态
3. **重置 FPU**: 新程序需要重新初始化 FPU

---

## 灾难预演

**如果忘记清除 MF_DELIVERMSG**:
- 可能投递消息到无效地址
- 内存访问错误
- 进程崩溃

**如果忘记重置 FPU**:
- 新程序继承旧的 FPU 状态
- 可能导致 FPU 计算错误
- 难以调试

**如果 arch_proc_init 设置错误**:
- 进程从错误地址开始执行
- 立即崩溃
- 无法运行新程序

---

## 互动自测

1. **问题**: 为什么 exec 后不需要回复 EXEC 调用？
   **答案**: exec 替换了整个进程映像，旧的调用上下文不存在了，进程从新入口开始执行。

2. **问题**: 为什么需要重置 FPU 状态？
   **答案**: 新程序不应该继承旧的 FPU 状态，首次使用 FPU 时会触发异常并初始化。

3. **问题**: arch_proc_init 的作用是什么？
   **答案**: 架构相关的进程初始化，设置新的指令指针、栈指针，保存程序名称。
