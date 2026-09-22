# kernel/system/do_clear.c 逐行讲解

> **文件路径**: `minix3/minix/kernel/system/do_clear.c`
> **核心功能**: 清理进程系统调用

---

## 文件概述

这个文件实现了进程退出后的清理工作。

**核心概念**: 进程退出时释放所有资源。

---

## 逐行讲解

### 释放地址空间

```c
int do_clear(struct proc * caller, message * m_ptr)
{
  struct proc *rc;
  int exit_p;
  int i;

  if(!isokendpt(m_ptr->m_lsys_krn_sys_clear.endpt, &exit_p)) {
      return EINVAL;
  }
  rc = proc_addr(exit_p);

  release_address_space(rc);

  if(isemptyp(rc)) return OK;
```

**讲解**:
- 获取退出的进程
- 释放地址空间
- 检查是否已清理

---

### 释放 IRQ 钩子

```c
  for (i=0; i < NR_IRQ_HOOKS; i++) {
      if (rc->p_endpoint == irq_hooks[i].proc_nr_e) {
        rm_irq_handler(&irq_hooks[i]);
        irq_hooks[i].proc_nr_e = NONE;
      }
  }
```

**讲解**:
- 遍历所有 IRQ 钩子
- 释放属于该进程的钩子

---

### 清理端点和定时器

```c
  clear_endpoint(rc);
  reset_kernel_timer(&priv(rc)->s_alarm_timer);

  RTS_SETFLAGS(rc, RTS_SLOT_FREE);

  release_fpu(rc);
  rc->p_misc_flags &= ~MF_FPU_INITIALIZED;
```

**讲解**:
- 清除端点
- 重置定时器
- 标记槽位为空闲
- 释放 FPU

---

### 释放特权结构

```c
  if (priv(rc)->s_flags & SYS_PROC) priv(rc)->s_proc_nr = NONE;

  return OK;
}
```

**讲解**:
- 如果是系统进程，释放特权结构
- 标记为未使用

---

## 要点总结

1. **资源释放**: 地址空间、IRQ、定时器、FPU
2. **槽位标记**: RTS_SLOT_FREE 标记为空闲
3. **特权释放**: 系统进程释放特权结构

---

## 互动自测

1. **问题**: 为什么需要释放 IRQ 钩子？
   **答案**: 防止中断发送到已退出的进程。
