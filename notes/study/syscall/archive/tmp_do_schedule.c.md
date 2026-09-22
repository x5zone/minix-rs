# kernel/system/do_schedule.c 逐行讲解

> **文件路径**: `minix3/minix/kernel/system/do_schedule.c`
> **核心功能**: 调度参数设置系统调用

---

## 文件概述

这个文件实现了调度参数设置，由用户态调度器调用。

**核心概念**: 用户态调度器设置进程的调度参数。

---

## 逐行讲解

### 参数验证

```c
int do_schedule(struct proc * caller, message * m_ptr)
{
	struct proc *p;
	int proc_nr;
	int priority, quantum, cpu, niced;

	if (!isokendpt(m_ptr->m_lsys_krn_schedule.endpoint, &proc_nr))
		return EINVAL;

	p = proc_addr(proc_nr);

	/* Only this process' scheduler can schedule it */
	if (caller != p->p_scheduler)
		return(EPERM);
```

**讲解**:
- 只有进程的调度器才能设置参数
- 验证进程端点有效性

---

### 设置调度参数

```c
	priority = m_ptr->m_lsys_krn_schedule.priority;
	quantum = m_ptr->m_lsys_krn_schedule.quantum;
	cpu = m_ptr->m_lsys_krn_schedule.cpu;
	niced = !!(m_ptr->m_lsys_krn_schedule.niced);

	return sched_proc(p, priority, quantum, cpu, niced);
}
```

**讲解**:
- **priority**: 优先级
- **quantum**: 时间片
- **cpu**: 绑定的 CPU
- **niced**: 是否 nice 过

---

## 要点总结

1. **权限检查**: 只有调度器才能设置参数
2. **调度参数**: 优先级、时间片、CPU
3. **sched_proc**: 实际设置函数

---

## 互动自测

1. **问题**: 为什么需要检查 caller == p->p_scheduler？
   **答案**: 防止其他进程随意修改调度参数。
