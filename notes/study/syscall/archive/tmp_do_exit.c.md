# kernel/system/do_exit.c 逐行讲解

> **文件路径**: `minix3/minix/kernel/system/do_exit.c`
> **核心功能**: exit 系统调用的内核实现

---

## 文件概述

这个文件实现了 `exit()` 系统调用的内核部分。它非常简单：
1. **发送终止信号**: 向调用者发送 SIGABRT 信号
2. **不回复**: 进程即将终止，不需要回复

**核心概念**: 系统进程请求退出，内核发送信号终止它。

---

## 逐行讲解

### 文件头注释

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_EXIT
 */
```

**讲解**:
- **SYS_EXIT**: exit 系统调用号
- 系统进程请求退出

---

### 头文件包含

```c
#include "kernel/system.h"

#include <signal.h>

#if USE_EXIT
```

**讲解**:
- **signal.h**: 信号定义，如 SIGABRT
- **USE_EXIT**: 配置选项

---

## do_exit 函数

### 函数签名和参数

```c
/*===========================================================================*
 *				 do_exit				     *
 *===========================================================================*/
int do_exit(struct proc * caller, message * m_ptr)
{
/* Handle sys_exit. A system process has requested to exit. Generate a
 * self-termination signal.
 */
  int sig_nr = SIGABRT;
```

**讲解**:
- **caller**: 调用者进程（请求退出的系统进程）
- **m_ptr**: 消息指针（此调用不需要参数）
- **sig_nr**: 信号编号，使用 SIGABRT（异常终止）

**SIGABRT**:
- 值为 6
- 表示异常终止
- 通常由 abort() 函数触发

---

### 发送信号

```c
  cause_sig(caller->p_nr, sig_nr);      /* send a signal to the caller */

  return(EDONTREPLY);			/* don't reply */
}

#endif /* USE_EXIT */
```

**讲解**:
- **cause_sig**: 向进程发送信号
- **caller->p_nr**: 调用者的进程编号
- **EDONTREPLY**: 不回复消息

**为什么使用 EDONTREPLY？**
- 进程即将终止
- 不需要回复
- 避免向已终止进程发送消息

---

## cause_sig 函数说明

**cause_sig 做什么？**
```c
void cause_sig(proc_nr_t proc_nr, int sig_nr)
{
    struct proc *rp = proc_addr(proc_nr);
    
    // 设置待处理信号
    sigaddset(&rp->p_pending, sig_nr);
    
    // 设置信号待处理标志
    RTS_SET(rp, RTS_SIG_PENDING);
    
    // 如果进程在等待消息，唤醒它
    if (RTS_ISSET(rp, RTS_RECEIVING)) {
        RTS_UNSET(rp, RTS_RECEIVING);
    }
}
```

**信号处理流程**:
```
cause_sig 被调用
    ↓
设置待处理信号
    ↓
设置 RTS_SIG_PENDING 标志
    ↓
唤醒进程（如果在等待）
    ↓
进程被调度时处理信号
    ↓
信号处理程序执行
    ↓
进程终止
```

---

## 设计总结

### 1. 系统进程退出机制

**为什么系统进程需要 SYS_EXIT？**
- 系统进程不能直接调用 exit()
- 需要通过内核请求退出
- 内核发送信号终止进程

**用户进程 vs 系统进程退出**:
| 类型 | 退出方式 |
|------|----------|
| 用户进程 | 调用 exit() → PM 处理 |
| 系统进程 | 调用 sys_exit() → 内核发送信号 |

### 2. 信号终止

**为什么使用 SIGABRT？**
- 表示异常终止
- 产生核心转储（如果配置）
- 清晰的退出原因

### 3. 不回复消息

**为什么返回 EDONTREPLY？**
- 进程即将终止
- 回复消息无意义
- 避免资源浪费

---

## 要点总结

1. **发送信号**: 使用 cause_sig 发送 SIGABRT 信号
2. **不回复**: 返回 EDONTREPLY，进程即将终止
3. **系统进程退出**: 系统进程通过 SYS_EXIT 请求退出

---

## 灾难预演

**如果忘记返回 EDONTREPLY**:
- 内核尝试回复已终止进程
- 消息发送失败
- 可能导致内核错误

**如果使用其他信号**:
- 可能产生不同的退出行为
- 例如 SIGKILL 不产生核心转储
- 影响调试

---

## 互动自测

1. **问题**: 为什么系统进程需要 SYS_EXIT？
   **答案**: 系统进程不能直接调用 exit()，需要通过内核请求退出，内核发送信号终止进程。

2. **问题**: 为什么返回 EDONTREPLY？
   **答案**: 进程即将终止，回复消息无意义，避免资源浪费。

3. **问题**: cause_sig 做什么？
   **答案**: 设置待处理信号，设置 RTS_SIG_PENDING 标志，唤醒进程（如果在等待）。
