# kernel/system/do_runctl.c 逐行讲解

**文件路径**: `minix3/minix/kernel/system/do_runctl.c`

**总行数**: 76 行

**作用**: 实现 `SYS_RUNCTL` 系统调用，控制进程的运行/停止状态

---

## 1. 头文件和注释

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_RUNCTL
 *
 * The parameters for this kernel call are:
 *    m1_i1:	RC_ENDPT	process number to control
 *    m1_i2:	RC_ACTION	stop or resume the process
 *    m1_i3:	RC_FLAGS	request flags
 */

#include "kernel/system.h"
#include <assert.h>
```

**消息参数**：

| 参数 | 字段 | 含义 |
|------|------|------|
| `RC_ENDPT` | `m1_i1` | 目标进程号 |
| `RC_ACTION` | `m1_i2` | 操作（停止/恢复） |
| `RC_FLAGS` | `m1_i3` | 标志位 |

---

## 2. 函数签名和变量声明

```c
int do_runctl(struct proc * caller, message * m_ptr)
{
  int proc_nr, action, flags;
  register struct proc *rp;
```

**变量说明**：

| 变量 | 类型 | 含义 |
|------|------|------|
| `proc_nr` | int | 目标进程号（槽位索引） |
| `action` | int | 操作：RC_STOP 或 RC_RESUME |
| `flags` | int | 标志位（如 RC_DELAY） |
| `rp` | struct proc * | 目标进程指针（寄存器变量，提高访问速度） |

---

## 3. 端点验证

```c
  if (!isokendpt(m_ptr->RC_ENDPT, &proc_nr)) return(EINVAL);
  if (iskerneln(proc_nr)) return(EPERM);
  rp = proc_addr(proc_nr);
```

**三层验证**：

| 验证 | 作用 | 失败返回值 |
|------|------|-----------|
| `isokendpt` | 端点是否有效 | `EINVAL` |
| `!iskerneln` | 不能是内核进程 | `EPERM` |
| `proc_addr` | 获取进程指针 | - |

**为什么不能停止内核进程？**

```
内核进程（IDLE, CLOCK, SYSTEM）：
- 是内核的一部分
- 停止内核进程 = 停止系统
- 可能导致崩溃
```

---

## 4. 提取参数

```c
  action = m_ptr->RC_ACTION;
  flags = m_ptr->RC_FLAGS;
```

**action 的可能值**：

| 值 | 含义 | 英文 |
|----|------|------|
| `RC_STOP` | 停止进程 | Resume → Stop |
| `RC_RESUME` | 恢复进程 | Stop → Resume |
| 其他 | 无效 | Invalid |

**flags 的可能值**：

| 值 | 含义 | 英文 |
|----|------|------|
| `RC_DELAY` | 延迟停止 | Delay stop |

---

## 5. 延迟停止机制（RC_DELAY）

```c
  if (action == RC_STOP && (flags & RC_DELAY)) {
	if (RTS_ISSET(rp, RTS_SENDING) || (rp->p_misc_flags & MF_SC_DEFER))
		rp->p_misc_flags |= MF_SIG_DELAY;

	if (rp->p_misc_flags & MF_SIG_DELAY)
		return (EBUSY);
  }
```

**为什么要延迟？**

```
场景：PM 想要停止一个正在发送消息的进程

问题：
┌─────────────────────────────────────────────────────────────────────┐
│  进程 P 正在发送消息                                                  │
│      │                                                              │
│      ▼                                                              │
│  PM 调用 do_runctl(P, RC_STOP)                                       │
│      │                                                              │
│      ▼                                                              │
│  如果立即设置 RTS_PROC_STOP：                                         │
│  - 进程 P 被标记为"已停止"                                           │
│  - 但 P 正在等待接收方回复                                            │
│  - 接收方回复后，P 无法处理（因为已停止）                             │
│  - 消息丢失或死锁                                                     │
└─────────────────────────────────────────────────────────────────────┘

解决方案（RC_DELAY）：
┌─────────────────────────────────────────────────────────────────────┐
│  1. 检查进程是否正在发送消息                                          │
│     - RTS_ISSET(rp, RTS_SENDING)                                    │
│     - MF_SC_DEFER 标志                                               │
│                                                                     │
│  2. 如果正在发送：                                                   │
│     - 设置 MF_SIG_DELAY 标志                                         │
│     - 返回 EBUSY（表示"忙，稍后再试"）                               │
│                                                                     │
│  3. PM 会稍后重试 do_runctl()                                        │
│     - 直到进程完成发送                                                │
│     - 然后设置 RTS_PROC_STOP                                         │
└─────────────────────────────────────────────────────────────────────┘
```

**内存模型**：

```
进程 P 的状态：
┌─────────────────────────────────────────────────────────────────────┐
│  正常情况：                                                         │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  p_misc_flags                                               │   │
│  │  └── MF_SIG_DELAY = 0                                       │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  正在发送消息时：                                                    │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  p_misc_flags                                               │   │
│  │  └── MF_SIG_DELAY = 1  ← 设置这个标志                      │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  进程完成发送后（由其他地方处理）：                                   │
│  - 清除 MF_SIG_DELAY                                               │
│  - PM 可以再次调用 do_runctl() 成功停止                             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 6. SMP 支持

```c
  switch (action) {
  case RC_STOP:
#if CONFIG_SMP
	  if (rp->p_cpu != cpuid) {
		  smp_schedule_stop_proc(rp);
		  break;
	  }
#endif
```

**什么是 SMP？**

```
SMP = Symmetric Multi-Processing（对称多处理）

┌─────────────────────────────────────────────────────────────────────┐
│  单核系统：                                                          │
│  ┌─────────┐                                                        │
│  │  CPU 0  │                                                        │
│  └─────────┘                                                        │
│       │                                                              │
│       ▼                                                              │
│  所有进程都在同一个 CPU 上运行                                        │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│  多核系统（SMP）：                                                   │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐             │
│  │  CPU 0  │  │  CPU 1  │  │  CPU 2  │  │  CPU 3  │             │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘             │
│       │              │              │              │              │
│       ▼              ▼              ▼              ▼              │
│  进程 A          进程 B          进程 C          进程 D          │
└─────────────────────────────────────────────────────────────────────┘
```

**问题场景**：

```
CPU 0 上的进程想要停止 CPU 2 上运行的进程：

┌─────────────────────────────────────────────────────────────────────┐
│  CPU 0                        CPU 2                                │
│  ┌─────────────────┐          ┌─────────────────┐                 │
│  │  PM (调用者)     │          │  进程 P (目标)   │                 │
│  │  do_runctl()    │   IPI    │  正在运行        │                 │
│  └─────────────────┘ ───────► └─────────────────┘                 │
│                              │                                      │
│                              │ smp_schedule_stop_proc()            │
│                              ▼                                      │
│                         进程 P 被标记为停止                         │
└─────────────────────────────────────────────────────────────────────┘

IPI = Inter-Processor Interrupt（处理器间中断）
```

---

## 7. 停止进程（RC_STOP）

```c
	  RTS_SET(rp, RTS_PROC_STOP);
	break;
```

**RTS_PROC_STOP 标志**：

```
RTS = RunTiMe flags（运行时标志）

┌─────────────────────────────────────────────────────────────────────┐
│  struct proc {                                                      │
│      ...                                                            │
│      int p_rts_flags;        // 运行时标志                           │
│      ...                                                            │
│  };                                                                 │
└─────────────────────────────────────────────────────────────────────┘

常用 RTS_ 标志：
  - RTS_PROC_STOP    = 进程已停止
  - RTS_SENDING      = 进程正在发送消息
  - RTS_RECEIVING     = 进程正在接收消息
  - RTS_SIGNALED     = 进程有信号待处理
  - RTS_SLOT_FREE    = 进程槽位空闲
```

**设置 RTS_PROC_STOP 后**：

```
┌─────────────────────────────────────────────────────────────────────┐
│  进程状态变化：                                                      │
│                                                                     │
│  before:  ┌─────────────┐                                          │
│           │   RUNNING    │  ← 进程正在运行                          │
│           └─────────────┘                                          │
│                    │                                                │
│                    ▼ RTS_SET(rp, RTS_PROC_STOP)                     │
│                   after:                                             │
│           ┌─────────────┐                                          │
│           │   STOPPED   │  ← 进程被停止                            │
│           └─────────────┘                                          │
│                                                                     │
│  影响：                                                              │
│  - 调度器不会选择此进程                                              │
│  - 进程不会获得 CPU 时间片                                            │
│  - 等待恢复（RC_RESUME）                                             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 8. 恢复进程（RC_RESUME）

```c
  case RC_RESUME:
	assert(RTS_ISSET(rp, RTS_PROC_STOP));
	RTS_UNSET(rp, RTS_PROC_STOP);
	break;
```

**assert 的作用**：

```c
assert(RTS_ISSET(rp, RTS_PROC_STOP));
```

```
为什么要 assert？
- 恢复进程时，检查进程是否真的被停止了
- 如果进程没有被停止，说明有 bug
- assert 会在调试版本中捕获这个问题

场景：
┌─────────────────────────────────────────────────────────────────────┐
│  PM 调用 do_runctl(P, RC_RESUME)                                     │
│      │                                                              │
│      ▼                                                              │
│  assert(RTS_ISSET(P, RTS_PROC_STOP))                                │
│      │                                                              │
│      ├──► 如果 P 确实被停止：assert 通过，继续执行                  │
│      │                                                              │
│      └──► 如果 P 没有被停止：assert 失败，程序崩溃（调试）           │
│              说明：PM 的状态管理有问题                                │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 9. 完整流程图

```
┌─────────────────────────────────────────────────────────────────────┐
│                    do_runctl 完整流程                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  开始                                                                │
│    │                                                                │
│    ▼                                                                │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 1. 验证端点有效性                                             │   │
│  │    isokendpt(endpoint, &proc_nr)                             │   │
│  └─────────────────────────────────────────────────────────────┘   │
│    │                                                                │
│    ▼                                                                │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 2. 检查不是内核进程                                           │   │
│  │    !iskerneln(proc_nr)                                       │   │
│  └─────────────────────────────────────────────────────────────┘   │
│    │                                                                │
│    ▼                                                                │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 3. 提取参数                                                   │   │
│  │    action, flags                                             │   │
│  └─────────────────────────────────────────────────────────────┘   │
│    │                                                                │
│    ▼                                                                │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 4. RC_DELAY 检查（仅 RC_STOP）                                │   │
│  │    if (action == RC_STOP && (flags & RC_DELAY))               │   │
│  │        - 进程正在发送？→ MF_SIG_DELAY → EBUSY                 │   │
│  │        - 否则继续                                             │   │
│  └─────────────────────────────────────────────────────────────┘   │
│    │                                                                │
│    ▼                                                                │
│  ┌───────────────────┐    ┌───────────────────┐                   │
│  │   RC_STOP         │    │   RC_RESUME       │                   │
│  ├───────────────────┤    ├───────────────────┤                   │
│  │ SMP 检查：         │    │ assert(已停止)   │                   │
│  │ - 同 CPU？直接停    │    │ RTS_UNSET       │                   │
│  │ - 不同 CPU？IPI   │    │ 清除停止标志     │                   │
│  │ RTS_SET           │    │                   │                   │
│  │ 设置停止标志       │    │                   │                   │
│  └───────────────────┘    └───────────────────┘                   │
│    │                                                                │
│    ▼                                                                │
│  return OK                                                          │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 10. 使用场景

**场景 1：调试器停止进程**

```
调试器（ptrace）需要停止目标进程来检查状态：

┌─────────────────────────────────────────────────────────────────────┐
│  调试器                      目标进程                              │
│     │                            │                                  │
│     │  ptrace(PTRACE_CONT)      │                                  │
│     │ ─────────────────────────► │                                  │
│     │                            │                                  │
│     │                     设置 RTS_PROC_STOP                       │
│     │                     进程停止运行                               │
│     │                            │                                  │
│     │  读取寄存器/内存           │                                  │
│     │ ◄───────────────────────── │                                  │
│     │                            │                                  │
│     │  ptrace(PTRACE_CONT)      │                                  │
│     │ ─────────────────────────► │                                  │
│     │                            │                                  │
│     │                     清除 RTS_PROC_STOP                       │
│     │                     进程恢复运行                               │
│     │                            │                                  │
└─────────────────────────────────────────────────────────────────────┘
```

**场景 2：作业控制（Shell）**

```
Shell 的作业控制：

┌─────────────────────────────────────────────────────────────────────┐
│  $ sleep 100 &        ← 后台运行                                   │
│    │                                                                  │
│    ▼                                                                  │
│  Shell 调用 do_runctl(sleep_pid, RC_STOP)  ← 暂停                  │
│    │                                                                  │
│    ▼                                                                  │
│  sleep 进程被停止（Ctrl+Z）                                         │
│    │                                                                  │
│    ▼                                                                  │
│  $ fg              ← 恢复前台                                       │
│    │                                                                  │
│    ▼                                                                  │
│  Shell 调用 do_runctl(sleep_pid, RC_RESUME)  ← 恢复                 │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 11. 要点总结

1. **RTS_PROC_STOP**：进程停止标志
2. **RC_DELAY**：延迟停止机制，避免竞态条件
3. **MF_SIG_DELAY**：信号延迟标志
4. **SMP 支持**：多核间处理器中断

---

## 12. 灾难预演

### 如果不检查内核进程

```
后果：
1. 内核进程被停止
2. 内核崩溃
3. 系统死机
```

### 如果不使用 RC_DELAY

```
后果：
1. 停止正在发送消息的进程
2. 接收方回复后无法处理
3. 消息丢失或死锁
```

### 如果恢复未停止的进程

```
后果：
1. assert 失败（调试版本）
2. 状态不一致
3. 可能导致调度错误
```

---

## 13. 互动自测

1. **问题**：为什么要用 RC_DELAY？
   **答案**：避免停止正在发送消息的进程，导致消息丢失或死锁。

2. **问题**：RTS_PROC_STOP 标志的作用是什么？
   **答案**：告诉调度器不要选择此进程，进程不会获得 CPU 时间。

3. **问题**：SMP 下为什么要发 IPI？
   **答案**：停止在其他 CPU 上运行的进程需要通过处理器间中断通知该 CPU。

4. **问题**：MF_SIG_DELAY 什么时候清除？
   **答案**：当进程完成发送操作后，由其他地方清除（不是 do_runctl）。

5. **问题**：assert 的作用是什么？
   **答案**：调试时检查假设是否成立，防止状态不一致的错误。

---

## 14. Rust 重构建议

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum RunCtlAction {
    Stop = RC_STOP,
    Resume = RC_RESUME,
}

bitflags! {
    pub struct RunCtlFlags: i32 {
        const DELAY = RC_DELAY;
    }
}

pub enum RunCtlError {
    InvalidEndpoint,
    PermissionDenied,
    Busy,  // RC_DELAY 时进程正在发送
    InvalidAction,
}

pub fn do_runctl(
    caller: &Proc,
    request: &RunCtlRequest,
) -> Result<(), RunCtlError> {
    let target = validate_endpoint(request.endpoint)
        .map_err(|_| RunCtlError::InvalidEndpoint)?;

    if target.is_kernel_process() {
        return Err(RunCtlError::PermissionDenied);
    }

    let action = RunCtlAction::from_raw(request.action)
        .map_err(|_| RunCtlError::InvalidAction)?;

    let flags = RunCtlFlags::from_bits(request.flags);

    if action == RunCtlAction::Stop && flags.contains(RunCtlFlags::DELAY) {
        if target.is_sending() || target.has_misc_flag(MF_SC_DEFER) {
            target.set_misc_flag(MF_SIG_DELAY);
            return Err(RunCtlError::Busy);
        }
    }

    match action {
        RunCtlAction::Stop => {
            target.stop()?;
        }
        RunCtlAction::Resume => {
            target.resume()?;
        }
    }

    Ok(())
}
```

---

**文档版本**: 2026-03-30