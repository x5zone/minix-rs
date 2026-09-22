# servers/vfs/select.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vfs/select.c`
> **核心功能**: select 系统调用实现

---

## 文件概述

这个文件实现了 select 系统调用。

**核心概念**: I/O 多路复用，文件描述符监控，阻塞等待。

---

## 逐行讲解

### 文件注释

```c
/* Implement entry point to select system call.
 *
 * The select code uses minimal locking, so that the replies from character
 * drivers can be processed without blocking.
 */
```

**讲解**:
- select 用于 I/O 多路复用
- 监控多个文件描述符
- 最小化锁使用

---

### 选择表结构

```c
static struct selectentry {
  struct fproc *requestor;	/* slot is free iff this is NULL */
  endpoint_t req_endpt;
  fd_set readfds, writefds, errorfds;
  fd_set ready_readfds, ready_writefds, ready_errorfds;
  struct filp *filps[OPEN_MAX];
  int type[OPEN_MAX];
  int nfds, nreadyfds;
  int error;
  char block;
```

**讲解**:
- **requestor**: 请求进程
- **readfds/writefds/errorfds**: 监控的 fd 集合
- **ready_***: 就绪的 fd 集合

---

## 要点总结

1. **select**: I/O 多路复用
2. **fd_set**: 文件描述符集合
3. **阻塞**: 等待就绪事件

---

## 互动自测

1. **问题**: select 和 poll 的区别？
   **答案**: select 有 fd 数量限制，poll 无限制；select 使用 fd_set，poll 使用数组。
