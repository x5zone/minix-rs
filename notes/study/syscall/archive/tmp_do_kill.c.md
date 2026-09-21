# kernel/system/do_kill.c 逐行讲解

> **文件路径**: `minix3/minix/kernel/system/do_kill.c`
> **核心功能**: kill 系统调用的内核实现

---

## 文件概述

这个文件实现了 kill 系统调用，用于向进程发送信号。

**核心概念**: 信号发送机制，支持用户进程和系统进程。

---

## 逐行讲解

### 函数签名

```c
int do_kill(struct proc * caller, message * m_ptr)
{
  proc_nr_t proc_nr, proc_nr_e;
  int sig_nr = m_ptr->m_sigcalls.sig;

  proc_nr_e = (proc_nr_t)m_ptr->m_sigcalls.endpt;
```

**讲解**:
- **sig_nr**: 信号编号
- **proc_nr_e**: 目标进程端点

---

### 参数验证

```c
  if (!isokendpt(proc_nr_e, &proc_nr)) return(EINVAL);
  if (sig_nr >= _NSIG) return(EINVAL);
  if (iskerneln(proc_nr)) return(EPERM);
```

**讲解**:
- 验证端点有效性
- 验证信号编号范围
- 禁止向内核进程发送信号

---

### 发送信号

```c
  /* Set pending signal to be processed by the signal manager. */
  cause_sig(proc_nr, sig_nr);

  return(OK);
}
```

**讲解**:
- **cause_sig**: 设置待处理信号
- 信号管理器会处理信号

---

## 要点总结

1. **参数验证**: 端点、信号编号、目标类型
2. **cause_sig**: 设置待处理信号
3. **信号管理器**: 负责信号投递

---

## 互动自测

1. **问题**: 为什么不能向内核进程发送信号？
   **答案**: 内核进程不是普通的可信号进程，信号机制不适用。
