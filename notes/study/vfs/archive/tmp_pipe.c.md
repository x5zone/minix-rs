# servers/vfs/pipe.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vfs/pipe.c`
> **核心功能**: 管道实现

---

## 文件概述

这个文件实现了管道和进程挂起/恢复机制。

**核心概念**: 管道，进程挂起，阻塞 I/O。

---

## 逐行讲解

### 文件注释

```c
/* This file deals with the suspension and revival of processes.  A process can
 * be suspended because it wants to read or write from a pipe and can't, or
 * because it wants to read or write from a special file and can't.
 */
```

**讲解**:
- 进程因管道读写阻塞而挂起
- 条件满足后恢复执行

---

### do_pipe2 入口

```c
int do_pipe2(void)
{
  int r, flags;
  int fil_des[2];

  flags = job_m_in.m_lc_vfs_pipe2.flags;
  flags |= job_m_in.m_lc_vfs_pipe2.oflags;

  r = create_pipe(fil_des, flags);
  if (r == OK) {
	job_m_out.m_vfs_lc_fdpair.fd0 = fil_des[0];
	job_m_out.m_vfs_lc_fdpair.fd1 = fil_des[1];
  }
  return(r);
}
```

**讲解**:
- 创建管道
- 返回两个文件描述符
- 读端和写端

---

## 要点总结

1. **pipe**: 创建管道
2. **阻塞**: 读写阻塞时挂起进程
3. **恢复**: 条件满足后恢复

---

## 互动自测

1. **问题**: 管道读写何时阻塞？
   **答案**: 读空管道或写满管道时阻塞。
