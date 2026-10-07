# do_getksig.c 详细讲解

**文件位置**: `minix3/minix/kernel/system/do_getksig.c`

**总行数**: 43 行

**作用**: 实现 `SYS_GETKSIG` 系统调用，获取待处理的内核信号

---

## 一、文件概述

### 1.1 是什么（What）

`do_getksig.c` 实现了 MINIX3 的**获取内核信号系统调用**：

| 系统调用 | 功能 |
|---------|------|
| `SYS_GETKSIG` | 获取有待处理信号的进程信息 |

**核心功能**：
- 查找有待处理内核信号的进程
- 返回进程端点和信号位图
- 清除内核中的信号位图

### 1.2 为什么需要（Why）

**设计原因**：

MINIX3 采用微内核架构，信号处理由用户态进程管理器（PM）负责：

```
┌─────────────────────────────────────────────────────────────────────────┐
│  MINIX3 信号处理架构                                                     │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────┐      SYS_GETKSIG      ┌─────────────┐                  │
│  │   内核      │ ──────────────────→  │     PM      │                  │
│  │             │                       │ (信号管理器) │                  │
│  │ 信号位图    │ ←──────────────────  │             │                  │
│  │ RTS_SIGNALED│      信号信息         │ 处理信号    │                  │
│  └─────────────┘                       └─────────────┘                  │
│                                                                         │
│  流程：                                                                 │
│  1. 内核设置信号位图和 RTS_SIGNALED 标志                                │
│  2. PM 调用 SYS_GETKSIG 获取信号信息                                    │
│  3. PM 处理信号（调用 SYS_SIGSEND）                                     │
│  4. PM 调用 SYS_ENDKSIG 结束信号处理                                    │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**为什么需要内核信号？**

1. **内核产生的信号**：如 SIGKILL、SIGSEGV
2. **任务产生的信号**：如时钟任务的 SIGALRM
3. **异步通知**：内核需要通知 PM 处理信号

### 1.3 使用场景（When）

| 场景 | 触发者 | 说明 |
|------|--------|------|
| PM 轮询 | PM | 定期检查待处理信号 |
| 信号通知 | 内核 | 内核发送通知后 PM 调用 |
| 进程终止 | 内核 | SIGKILL 处理 |

---

## 二、逐行讲解

### 2.1 文件头注释（第 1-9 行）

```c
/* The kernel call that is implemented in this file:
 *	m_type: SYS_GETKSIG
 *
 * The parameters for this kernel call are:
 *	m_sigcalls.endpt	# process with pending signals
 *	m_sigcalls.map		# bit map with pending signals
 */
```

**翻译**：
```
本文件实现的内核调用：
  m_type: SYS_GETKSIG

此内核调用的参数：
  m_sigcalls.endpt  - 有待处理信号的进程
  m_sigcalls.map    - 待处理信号的位图
```

**参数详解**：

| 字段 | 方向 | 类型 | 含义 |
|------|------|------|------|
| `endpt` | 输出 | `endpoint_t` | 有待处理信号的进程端点 |
| `map` | 输出 | `sigset_t` | 待处理信号的位图 |

### 2.2 头文件包含（第 11-14 行）

```c
#include "kernel/system.h"
#include <signal.h>
#include <minix/endpoint.h>
```

| 头文件 | 作用 |
|--------|------|
| `"kernel/system.h"` | 系统调用框架、进程结构定义 |
| `<signal.h>` | 信号相关定义（sigset_t 等） |
| `<minix/endpoint.h>` | 端点类型定义 |

### 2.3 条件编译（第 16 行）

```c
#if USE_GETKSIG
```

**设计原因**：允许在编译时禁用此功能以减小内核大小。

### 2.4 函数注释（第 18-21 行）

```c
/*===========================================================================*
 *			      do_getksig				     *
 *===========================================================================*/
int do_getksig(struct proc * caller, message * m_ptr)
{
/* The signal manager is ready to accept signals and repeatedly does a kernel
 * call to get one. Find a process with pending signals. If no signals are
 * available, return NONE in the process number field.
 */
```

**翻译注释**：
```
信号管理器准备好接收信号，并重复执行内核调用来获取信号。
查找有待处理信号的进程。如果没有可用信号，在进程号字段返回 NONE。
```

**设计思路**：
- PM 轮询内核获取信号
- 每次调用返回一个进程的信号
- 没有信号时返回 NONE

### 2.5 局部变量声明（第 23 行）

```c
  register struct proc *rp;
```

| 变量 | 类型 | 大小 | 用途 |
|------|------|------|------|
| `rp` | `struct proc *` | 8 字节 | 进程指针（遍历用） |

### 2.6 遍历进程表（第 25-35 行）

```c
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

**翻译注释**：`Find the next process with pending signals` = "查找下一个有待处理信号的进程"

**遍历逻辑**：

```
┌─────────────────────────────────────────────────────────────────────────┐
│  进程遍历流程                                                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  for (rp = BEG_USER_ADDR; rp < END_PROC_ADDR; rp++)                    │
│       │                                                                 │
│       ├── 检查 RTS_SIGNALED 标志                                        │
│       │   └── 如果未设置，继续下一个进程                                 │
│       │                                                                 │
│       ├── 检查信号管理器是否匹配                                         │
│       │   └── if (caller->p_endpoint != priv(rp)->s_sig_mgr) continue  │
│       │   └── 如果不匹配，继续下一个进程                                 │
│       │                                                                 │
│       └── 找到匹配的进程：                                               │
│           ├── 返回进程端点                                              │
│           ├── 返回信号位图                                              │
│           ├── 清除内核中的信号位图                                       │
│           └── 清除 RTS_SIGNALED 标志                                    │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**关键点详解**：

1. **BEG_USER_ADDR / END_PROC_ADDR**：
   - 进程表的起始和结束地址
   - 只遍历用户进程，不包括内核任务

2. **RTS_SIGNALED 标志**：
   - 表示进程有待处理的内核信号
   - 由 `cause_sig()` 或 `inform()` 设置

3. **信号管理器检查**：
   - 每个进程有指定的信号管理器（通常是 PM）
   - 只有对应的信号管理器才能获取信号

4. **清除操作**：
   - `sigemptyset(&rp->p_pending)` - 清除信号位图
   - `RTS_UNSET(rp, RTS_SIGNALED)` - 清除标志

**为什么清除信号位图？**

信号信息已经传递给 PM，内核不再需要保存。PM 会负责处理这些信号。

### 2.7 无信号情况（第 37-39 行）

```c
  /* No process with pending signals was found. */
  m_ptr->m_sigcalls.endpt = NONE;
  return(OK);
}
```

**翻译注释**：`No process with pending signals was found` = "未找到有待处理信号的进程"

**返回值**：
- `endpt = NONE` - 表示没有待处理信号
- `return(OK)` - 调用成功，只是没有信号

### 2.8 条件编译结束（第 40 行）

```c
#endif /* USE_GETKSIG */
```

---

## 三、信号管理器机制

### 3.1 信号管理器分配

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号管理器分配                                                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  每个进程都有一个信号管理器（s_sig_mgr）：                                │
│                                                                         │
│  ┌─────────────────┬──────────────────┐                                │
│  │ 进程类型         │ 信号管理器        │                                │
│  ├─────────────────┼──────────────────┤                                │
│  │ 用户进程         │ PM (进程管理器)   │                                │
│  │ 系统服务         │ 可能是自身或 PM   │                                │
│  │ 内核任务         │ 无（不处理信号）  │                                │
│  └─────────────────┴──────────────────┘                                │
│                                                                         │
│  priv(rp)->s_sig_mgr 存储信号管理器的端点                               │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 RTS 标志关系

```
┌─────────────────────────────────────────────────────────────────────────┐
│  信号相关 RTS 标志                                                       │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  RTS_SIGNALED:                                                          │
│  ├── 由内核设置，表示有待处理的内核信号                                  │
│  ├── 由 SYS_GETKSIG 清除                                                │
│  └── 阻止进程运行                                                        │
│                                                                         │
│  RTS_SIG_PENDING:                                                       │
│  ├── 由 SYS_GETKSIG 设置                                                │
│  ├── 表示信号正在被 PM 处理                                              │
│  └── 由 SYS_ENDKSIG 清除                                                │
│                                                                         │
│  状态转换：                                                              │
│  正常 → RTS_SIGNALED → RTS_SIG_PENDING → 正常                          │
│         (信号产生)    (GETKSIG)          (ENDKSIG)                      │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 四、Rust 重构建议

```rust
use core::result::Result;

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SigcallsResult {
    pub endpt: Endpoint,
    pub map: Sigset,
}

pub fn do_getksig(caller: &Proc, m_ptr: &mut Message) -> Result<(), Errno> {
    for rp in PROC_TABLE.iter() {
        if rp.rts_flags.contains(RtsFlags::SIGNALED) {
            if caller.p_endpoint != rp.priv_ref.s_sig_mgr {
                continue;
            }

            m_ptr.sigcalls.endpt = rp.p_endpoint;
            m_ptr.sigcalls.map = rp.p_pending;

            rp.p_pending.clear();
            rp.rts_flags.remove(RtsFlags::SIGNALED);

            return Ok(());
        }
    }

    m_ptr.sigcalls.endpt = Endpoint::NONE;
    Ok(())
}
```

---

## 五、要点总结

### 核心知识点

1. **信号轮询机制**：
   - PM 通过轮询获取内核信号
   - 每次调用返回一个进程的信号
   - 无信号时返回 NONE

2. **信号管理器**：
   - 每个进程有指定的信号管理器
   - 只有信号管理器才能获取信号
   - 通常由 PM 担任

3. **状态转换**：
   - RTS_SIGNALED → RTS_SIG_PENDING
   - 清除内核中的信号位图

---

## 六、灾难预演

### 场景 1：如果不清除信号位图

```
后果：
1. PM 会重复处理同一信号
2. 进程收到重复信号
3. 信号处理混乱
```

**防护**：`sigemptyset(&rp->p_pending)` 清除位图。

### 场景 2：如果信号管理器检查失败

```
后果：
1. 错误的进程获取信号
2. 信号被错误处理
3. 安全漏洞
```

**防护**：`caller->p_endpoint != priv(rp)->s_sig_mgr` 检查。

### 场景 3：如果 PM 崩溃

```
后果：
1. 内核信号无法处理
2. 进程永久阻塞在 RTS_SIGNALED
3. 系统死锁
```

**防护**：系统需要监控 PM 状态，必要时重启。

---

## 七、互动自测

1. **问题**：为什么需要信号管理器检查？
   **答案**：每个进程有指定的信号管理器，只有对应的信号管理器才能获取和处理该进程的信号。这保证了信号处理的安全性和正确性。

2. **问题**：RTS_SIGNALED 和 RTS_SIG_PENDING 有什么区别？
   **答案**：RTS_SIGNALED 表示进程有待处理的内核信号，由内核设置。RTS_SIG_PENDING 表示信号正在被 PM 处理，由 SYS_GETKSIG 设置。前者表示"有信号"，后者表示"正在处理"。

3. **问题**：为什么返回 NONE 而不是错误？
   **答案**：没有待处理信号是正常情况，不是错误。PM 会定期轮询，返回 NONE 表示"暂时没有信号需要处理"。

---

*讲解者：Minix-rs 学习助手*
