# kernel/system/do_fork.c 逐行讲解

> **文件路径**: `minix3/minix/kernel/system/do_fork.c`
> **核心功能**: fork 系统调用的内核实现

---

## 文件概述

这个文件实现了 `fork()` 系统调用的内核部分。它负责：
1. **复制进程控制块**: 将父进程的 PCB 复制给子进程
2. **设置子进程状态**: 初始化子进程的各种状态
3. **处理特权**: 如果父进程是系统进程，子进程需要重新设置特权

**核心概念**: fork 创建子进程，复制父进程的上下文。

---

## 逐行讲解

### 文件头注释

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_FORK
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_fork.endpt		(parent, process that forked)
 *   m_lsys_krn_sys_fork.slot		(child's process table slot)
 *   m_lsys_krn_sys_fork.flags		(fork flags)
 *   m_krn_lsys_sys_fork.endpt		(endpoint of the child)
 *   m_krn_lsys_sys_fork.msgaddr	(new memory map for the child)
 */
```

**讲解**:
- **SYS_FORK**: fork 系统调用号
- **endpt**: 父进程端点
- **slot**: 子进程的进程表槽位
- **flags**: fork 标志（如 PFF_VMINHIBIT）

---

### 头文件包含

```c
#include "kernel/system.h"
#include "kernel/vm.h"
#include <signal.h>
#include <string.h>
#include <assert.h>

#include <minix/endpoint.h>
#include <minix/u64.h>

#if USE_FORK
```

**讲解**:
- **USE_FORK**: 配置选项，控制是否编译此功能
- **system.h**: 系统调用框架
- **vm.h**: VM 相关定义

---

## do_fork 函数

### 函数签名和参数

```c
/*===========================================================================*
 *				do_fork					     *
 *===========================================================================*/
int do_fork(struct proc * caller, message * m_ptr)
{
/* Handle sys_fork().
 * m_lsys_krn_sys_fork.endpt has forked.
 * The child is m_lsys_krn_sys_fork.slot.
 */
#if defined(__i386__)
  char *old_fpu_save_area_p;
#endif
  register struct proc *rpc;		/* child process pointer */
  struct proc *rpp;			/* parent process pointer */
  int gen;
  int p_proc;
  int namelen;
```

**讲解**:
- **caller**: 调用者进程（通常是 PM）
- **m_ptr**: 消息指针，包含 fork 参数
- **rpc**: 子进程指针
- **rpp**: 父进程指针

**变量说明**:
| 变量 | 类型 | 说明 |
|------|------|------|
| rpc | proc* | 子进程指针 |
| rpp | proc* | 父进程指针 |
| gen | int | 端点代数 |
| p_proc | int | 父进程编号 |

---

### 参数验证

```c
  if(!isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc))
	return EINVAL;

  rpp = proc_addr(p_proc);
  rpc = proc_addr(m_ptr->m_lsys_krn_sys_fork.slot);
  if (isemptyp(rpp) || ! isemptyp(rpc)) return(EINVAL);

  assert(!(rpp->p_misc_flags & MF_DELIVERMSG));
```

**讲解**:
- **isokendpt**: 验证父进程端点是否有效
- **proc_addr**: 根据进程编号获取进程指针
- **isemptyp**: 检查进程槽是否为空

**验证逻辑**:
1. 父进程端点必须有效
2. 父进程槽必须非空
3. 子进程槽必须为空（待分配）

---

### 接收状态检查

```c
  /* needs to be receiving so we know where the message buffer is */
  if(!RTS_ISSET(rpp, RTS_RECEIVING)) {
	printf("kernel: fork not done synchronously?\n");
	return EINVAL;
  }
```

**讲解**:
- **RTS_RECEIVING**: 父进程必须处于接收状态
- 这是为了确保同步 fork
- PM 在调用 fork 前会阻塞父进程

**为什么需要接收状态？**
- fork 需要原子性
- 父进程在 fork 期间不能运行
- 确保 fork 完成后再继续

---

### FPU 状态保存

```c
  /* make sure that the FPU context is saved in parent before copy */
  save_fpu(rpp);
```

**讲解**:
- **save_fpu**: 保存父进程的 FPU 状态
- 确保复制时 FPU 状态完整
- 子进程需要继承父进程的 FPU 状态

---

### 复制进程控制块

```c
  /* Copy parent 'proc' struct to child. And reinitialize some fields. */
  gen = _ENDPOINT_E(rpc->p_endpoint);
#if defined(__i386__)
  old_fpu_save_area_p = rpc->p_seg.fpu_state;
#endif
  *rpc = *rpp;				/* copy 'proc' struct */
```

**讲解**:
- **gen**: 保存子进程的端点代数
- ***rpc = *rpp**: 整体复制 PCB

**PCB 复制**:
```
父进程 PCB              子进程 PCB
┌─────────────┐        ┌─────────────┐
│ p_reg       │  ───►  │ p_reg       │
│ p_seg       │  ───►  │ p_seg       │
│ p_priority  │  ───►  │ p_priority  │
│ ...         │        │ ...         │
└─────────────┘        └─────────────┘
```

---

### FPU 状态恢复

```c
#if defined(__i386__)
  rpc->p_seg.fpu_state = old_fpu_save_area_p;
  if(proc_used_fpu(rpp))
	memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);
#endif
```

**讲解**:
- 恢复子进程的 FPU 保存区域指针
- 如果父进程使用过 FPU，复制 FPU 状态

**FPU 状态复制**:
```
父进程 FPU 状态         子进程 FPU 状态
┌─────────────┐        ┌─────────────┐
│ 寄存器状态   │  ───►  │ 寄存器状态   │
│ 控制字      │  ───►  │ 控制字      │
│ 状态字      │  ───►  │ 状态字      │
└─────────────┘        └─────────────┘
```

---

### 端点代数更新

```c
  if(++gen >= _ENDPOINT_MAX_GENERATION)	/* increase generation */
	gen = 1;			/* generation number wraparound */
  rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;
  rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);	/* new endpoint of slot */
```

**讲解**:
- **gen++**: 增加端点代数
- **_ENDPOINT_MAX_GENERATION**: 最大代数
- **_ENDPOINT(gen, nr)**: 组合生成端点

**端点代数机制**:
```
端点 = (代数 << 16) | 进程号

父进程: endpoint = (5 << 16) | 10 = 0x5000A
子进程: endpoint = (6 << 16) | 11 = 0x6000B
```

**为什么需要代数？**
- 防止旧端点引用新进程
- 进程重用时代数增加
- 类似进程 ID 的版本号

---

### 重置子进程字段

```c
  rpc->p_cpu_time_left = 0;
  rpc->p_cycles = 0;
  rpc->p_kcall_cycles = 0;
  rpc->p_kipc_cycles = 0;

  rpc->p_tick_cycles = 0;
  cpuavg_init(&rpc->p_cpuavg);
```

**讲解**:
- 重置 CPU 时间统计
- 子进程从零开始计时
- 初始化 CPU 平均值

---

### 进程名称修改

```c
  /* Mark process name as being a forked copy */
  namelen = strlen(rpc->p_name);
#define FORKSTR "*F"
  if(namelen+strlen(FORKSTR) < sizeof(rpc->p_name))
	strcat(rpc->p_name, FORKSTR);
```

**讲解**:
- 在子进程名称后添加 "*F"
- 标记这是 fork 的副本
- 便于调试识别

**示例**:
```
父进程: "my_process"
子进程: "my_process*F"
```

---

### 设置不可运行状态

```c
  /* the child process is not runnable until it's scheduled. */
  RTS_SET(rpc, RTS_NO_QUANTUM);
  reset_proc_accounting(rpc);
```

**讲解**:
- **RTS_NO_QUANTUM**: 子进程没有时间片
- 需要调度器分配时间片后才能运行
- **reset_proc_accounting**: 重置统计信息

---

### 特权处理

```c
  /* If the parent is a privileged process, take away the privileges from the 
   * child process and inhibit it from running by setting the NO_PRIV flag.
   * The caller should explicitly set the new privileges before executing.
   */
  if (priv(rpp)->s_flags & SYS_PROC) {
      rpc->p_priv = priv_addr(USER_PRIV_ID);
      rpc->p_rts_flags |= RTS_NO_PRIV;
  }
```

**讲解**:
- **SYS_PROC**: 系统进程标志
- 如果父进程是系统进程，子进程需要重新设置特权
- **USER_PRIV_ID**: 用户特权级别

**为什么需要重新设置特权？**
- 系统进程有高特权
- 子进程不应该继承高特权
- 防止权限泄露

---

### 返回值设置

```c
  /* Calculate endpoint identifier, so caller knows what it is. */
  m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint;
  m_ptr->m_krn_lsys_sys_fork.msgaddr = rpp->p_delivermsg_vir;
```

**讲解**:
- 返回子进程的端点
- 返回消息地址

---

### VM 抑制标志

```c
  /* Don't schedule process in VM mode until it has a new pagetable. */
  if(m_ptr->m_lsys_krn_sys_fork.flags & PFF_VMINHIBIT) {
  	RTS_SET(rpc, RTS_VMINHIBIT);
  }
```

**讲解**:
- **PFF_VMINHIBIT**: VM 抑制标志
- 子进程需要 VM 设置新的页表
- 设置页表后才能运行

---

### 信号处理

```c
  /* 
   * Only one in group should have RTS_SIGNALED, child doesn't inherit tracing.
   */
  RTS_UNSET(rpc, (RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP));
  (void) sigemptyset(&rpc->p_pending);
```

**讲解**:
- **RTS_SIGNALED**: 信号待处理标志
- **RTS_SIG_PENDING**: 有挂起的信号
- **RTS_P_STOP**: 进程被停止（如 SIGSTOP）
- **sigemptyset**: 清空挂起信号集

**子进程不继承信号**:
```
父进程可能有信号待处理：
  p_rts_flags |= RTS_SIGNALED
  p_pending = { SIGTERM }

子进程必须清除这些：
  p_rts_flags &= ~RTS_SIGNALED
  p_pending = {}

原因：
  信号是发给特定进程的
  子进程不应该收到父进程的信号
```

---

### 页表指针清空

```c
#if defined(__i386__)
  rpc->p_seg.p_cr3 = 0;
  rpc->p_seg.p_cr3_v = NULL;
#elif defined(__arm__)
  rpc->p_seg.p_ttbr = 0;
  rpc->p_seg.p_ttbr_v = NULL;
#endif
```

**讲解**:
- **p_cr3 / p_ttbr**: 页表物理地址
- **p_cr3_v / p_ttbr_v**: 页表虚拟地址

**为什么要清空？**
```
子进程需要新的页表：
1. VM 会为子进程创建新的地址空间
2. 在此之前，子进程不应该使用父进程的页表
3. 清空指针，防止误用
```

---

### 返回成功

```c
  return OK;
}

#endif /* USE_FORK */
```

**讲解**:
- fork 系统调用完成
- 返回 OK 表示成功

---

## 设计总结

### 1. fork 流程

```
PM 调用 do_fork
    ↓
验证父进程和子进程槽
    ↓
保存父进程 FPU 状态
    ↓
复制 PCB
    ↓
更新端点代数
    ↓
重置统计信息
    ↓
设置子进程状态（不可运行）
    ↓
处理特权
    ↓
返回子进程端点
```

### 2. 端点代数

**设计思路**:
- 端点包含代数信息
- 进程重用时代数增加
- 防止旧端点引用新进程

### 3. 特权继承

**设计思路**:
- 普通进程：继承父进程特权
- 系统进程：子进程需要重新设置特权
- 防止权限泄露

---

## 要点总结

1. **PCB 复制**: 整体复制父进程的 PCB 给子进程
2. **端点代数**: 子进程获得新的端点，代数增加
3. **特权处理**: 系统进程的子进程需要重新设置特权

---

## 灾难预演

**如果忘记保存 FPU 状态**:
- 子进程 FPU 状态不完整
- 可能导致 FPU 计算错误
- 难以调试

**如果端点代数不增加**:
- 旧端点可能引用新进程
- IPC 发送到错误的进程
- 安全问题

**如果忘记设置 RTS_NO_QUANTUM**:
- 子进程可能立即运行
- 没有时间片
- 调度器混乱

---

## 互动自测

1. **问题**: 为什么父进程必须处于接收状态？
   **答案**: 确保 fork 的原子性，父进程在 fork 期间不能运行，保证 fork 完成后再继续。

2. **问题**: 端点代数的作用是什么？
   **答案**: 防止旧端点引用新进程，进程重用时代数增加，类似进程 ID 的版本号。

3. **问题**: 为什么系统进程的子进程需要重新设置特权？
   **答案**: 系统进程有高特权，子进程不应该继承高特权，防止权限泄露。
