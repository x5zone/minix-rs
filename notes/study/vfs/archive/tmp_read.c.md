# read.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vfs/read.c`
> 
> **行数**: 393 行
> 
> **核心内容**: read/write 系统调用的核心实现，处理管道、字符设备、块设备、socket、普通文件

---

## 文件概述

`read.c` 是 VFS 中**最核心的 I/O 文件**，实现了：
1. **do_read**：read 系统调用入口
2. **read_write**：所有文件类型的统一 I/O 处理（管道、字符设备、块设备、socket、普通文件）
3. **do_getdents**：目录读取
4. **rw_pipe**：管道读写

设计哲学：将读写请求分割为不跨越块边界的块，每种文件类型有不同的处理路径。

---

## 逐行讲解

### 第 1-11 行：文件头注释

```c
/* This file contains the heart of the mechanism used to read (and write)
 * files.  Read and write requests are split up into chunks that do not cross
 * block boundaries.  Each chunk is then processed in turn.  Reads on special
 * files are also detected and handled.
 *
 * The entry points into this file are
 *   do_read:	 perform the READ system call by calling read_write
 *   do_getdents: read entries from a directory (GETDENTS)
 *   read_write: actually do the work of READ and WRITE
 *
 */
```

**注释翻译**：
- `This file contains the heart of the mechanism used to read (and write) files` → 此文件包含用于读（和写）文件机制的核心
- `Read and write requests are split up into chunks that do not cross block boundaries` → 读写请求被分割为不跨越块边界的块
- `Each chunk is then processed in turn` → 每个块依次处理
- `Reads on special files are also detected and handled` → 特殊文件的读也会被检测和处理

**设计思路**：
read.c 是 VFS 的"心脏"。它不直接读写数据，而是根据文件类型将请求路由到正确的处理路径。这是 VFS 作为"路由器"角色的典型体现。

---

### 第 13-24 行：头文件包含

```c
#include "fs.h"
#include <minix/callnr.h>
#include <minix/com.h>
#include <minix/u64.h>
#include <minix/vfsif.h>
#include <assert.h>
#include <sys/dirent.h>
#include <fcntl.h>
#include <unistd.h>
#include "file.h"
#include "vnode.h"
#include "vmnt.h"
```

**逐个讲解**：

| 头文件 | 作用 |
|--------|------|
| `fs.h` | VFS 主头文件 |
| `minix/callnr.h` | 系统调用号 |
| `minix/com.h` | 通信定义 |
| `minix/u64.h` | 64 位整数操作 |
| `minix/vfsif.h` | VFS 接口定义 |
| `sys/dirent.h` | 目录条目结构（getdents） |
| `fcntl.h` | 文件标志（O_APPEND、O_NONBLOCK） |
| `unistd.h` | POSIX 标准 |
| `file.h` | filp 结构 |
| `vnode.h` | vnode 结构 |
| `vmnt.h` | vmnt 结构 |

---

### 第 27-43 行：do_read 函数

```c
/*===========================================================================*
 *				do_read					     *
 *===========================================================================*/
int do_read(void)
{

  /*
   * This field is currently reserved for internal usage only, and must be set
   * to zero by the caller.  We may use it for future SA_RESTART support just
   * like we are using it internally now.
   */
  if (job_m_in.m_lc_vfs_readwrite.cum_io != 0)
	return(EINVAL);

  return(do_read_write_peek(READING, job_m_in.m_lc_vfs_readwrite.fd,
	job_m_in.m_lc_vfs_readwrite.buf, job_m_in.m_lc_vfs_readwrite.len));
}
```

**注释翻译**：
- `This field is currently reserved for internal usage only, and must be set to zero by the caller` → 此字段目前仅供内部使用，调用者必须设为零
- `We may use it for future SA_RESTART support just like we are using it internally now` → 我们可能用它来支持未来的 SA_RESTART，就像我们现在内部使用它一样

**是什么**：read 系统调用的 VFS 入口。

**为什么**：
- **`cum_io != 0` 检查**：防止用户态传入非零值，该字段保留给内部恢复使用
- **`do_read_write_peek(READING, ...)`**：与 write 共享实现，仅方向不同

---

### 第 46-71 行：BSF 锁管理

```c
/*===========================================================================*
 *				lock_bsf				     *
 *===========================================================================*/
void lock_bsf(void)
{
  struct worker_thread *org_self;

  if (mutex_trylock(&bsf_lock) == 0)
	return;

  org_self = worker_suspend();

  if (mutex_lock(&bsf_lock) != 0)
	panic("unable to lock block special file lock");

  worker_resume(org_self);
}

/*===========================================================================*
 *				unlock_bsf				     *
 *===========================================================================*/
void unlock_bsf(void)
{
  if (mutex_unlock(&bsf_lock) != 0)
	panic("failed to unlock block special file lock");
}
```

**是什么**：块特殊文件（BSF）全局锁的获取和释放。

**为什么**：
- **BSF 锁**：保护对块设备文件的并发访问
- **trylock + suspend 模式**：先尝试非阻塞获取，失败后挂起线程再阻塞等待
- 与 `lock_proc` 使用相同的模式，避免工作线程阻塞

---

### 第 73-87 行：check_bsf_lock 调试函数

```c
/*===========================================================================*
 *				check_bsf				     *
 *===========================================================================*/
void check_bsf_lock(void)
{
	int r = mutex_trylock(&bsf_lock);

	if (r == -EBUSY)
		panic("bsf_lock locked");
	else if (r != 0)
		panic("bsf_lock weird state");

	/* r == 0 */
	unlock_bsf();
}
```

**是什么**：调试函数，检查 BSF 锁是否未被持有。

**为什么**：
- **`-EBUSY`**：锁已被持有，说明有 bug
- **`r != 0`**：其他异常状态
- **`r == 0`**：成功获取锁，立即释放以验证锁可用

---

### 第 89-122 行：actual_read_write_peek 函数

```c
/*===========================================================================*
 *				actual_read_write_peek			     *
 *===========================================================================*/
int actual_read_write_peek(struct fproc *rfp, int rw_flag, int fd,
	vir_bytes buf, size_t nbytes)
{
/* Perform read(fd, buffer, nbytes) or write(fd, buffer, nbytes) call. */
  struct filp *f;
  tll_access_t locktype;
  int r;
  int ro = 1;

  if(rw_flag == WRITING) ro = 0;

  locktype = rw_flag == WRITING ? VNODE_WRITE : VNODE_READ;
  if ((f = get_filp2(rfp, fd, locktype)) == NULL)
	return(err_code);

  assert(f->filp_count > 0);

  if (((f->filp_mode) & (ro ? R_BIT : W_BIT)) == 0) {
	unlock_filp(f);
	return(EBADF);
  }
  if (nbytes == 0) {
	unlock_filp(f);
	return(0);	/* so char special files need not check for 0*/
  }

  r = read_write(rfp, rw_flag, fd, f, buf, nbytes, who_e);

  unlock_filp(f);
  return(r);
}
```

**注释翻译**：
- `Perform read(fd, buffer, nbytes) or write(fd, buffer, nbytes) call` → 执行 read 或 write 调用
- `so char special files need not check for 0` → 因此字符特殊文件不需要检查 0

**是什么**：实际执行读写的核心函数，处理 filp 获取、模式检查、调用 read_write。

**逐行讲解**：

**第 99 行**：`ro = 1` 表示只读，写操作时设为 0。

**第 101 行**：确定锁类型——写操作需要 `VNODE_WRITE`（独占），读操作需要 `VNODE_READ`（共享）。

**第 102-103 行**：`get_filp2` 根据 fd 获取 filp 并加 vnode 锁。失败返回 err_code。

**第 105 行**：断言 filp 引用计数大于 0，确保 filp 有效。

**第 107-110 行**：检查打开模式。读操作需要 `R_BIT`，写操作需要 `W_BIT`。不匹配返回 `EBADF`。

**第 111-114 行**：零字节读取直接返回 0。这是优化——避免不必要的 IPC 调用。注释说明字符特殊文件不需要检查零长度。

**第 116 行**：调用 `read_write` 执行实际 I/O。

**第 118-119 行**：解锁 filp 并返回结果。

---

### 第 124-130 行：do_read_write_peek 包装函数

```c
/*===========================================================================*
 *				do_read_write_peek			     *
 *===========================================================================*/
int do_read_write_peek(int rw_flag, int fd, vir_bytes buf, size_t nbytes)
{
	return actual_read_write_peek(fp, rw_flag, fd, buf, nbytes);
}
```

**是什么**：简单的包装函数，将全局 `fp` 传递给 `actual_read_write_peek`。

**为什么**：
- 分离了"获取当前进程上下文"和"实际执行 I/O"的逻辑
- `actual_read_write_peek` 接受 `rfp` 参数，可被其他上下文调用（如 pipe 恢复）

---

### 第 132-177 行：read_write 函数（文件类型分发）

```c
/*===========================================================================*
 *				read_write				     *
 *===========================================================================*/
int read_write(struct fproc *rfp, int rw_flag, int fd, struct filp *f,
	vir_bytes buf, size_t size, endpoint_t for_e)
{
  register struct vnode *vp;
  off_t position, res_pos;
  size_t cum_io, res_cum_io;
  size_t cum_io_incr;
  int op, r;
  dev_t dev;

  position = f->filp_pos;
  vp = f->filp_vno;
  r = OK;
  cum_io = 0;

  assert(rw_flag == READING || rw_flag == WRITING || rw_flag == PEEKING);

  if (size > SSIZE_MAX) return(EINVAL);
```

**注释翻译**：
- `Perform read(fd, buffer, nbytes) or write(fd, buffer, nbytes) call` → 执行读写调用

**是什么**：read/write 的核心实现，根据 vnode 模式分发到不同处理路径。

**初始化**：
- `position`：文件当前位置
- `vp`：vnode 指针
- `cum_io`：累积 I/O 字节数

**第 150 行**：断言操作方向有效。

**第 152 行**：检查大小不超过 `SSIZE_MAX`（有符号 size_t 最大值），防止溢出。

---

### 第 154-161 行：管道处理

```c
  if (S_ISFIFO(vp->v_mode)) {		/* Pipes */
	if(rw_flag == PEEKING) {
	  	printf("read_write: peek on pipe makes no sense\n");
		return EINVAL;
	}
	assert(fd != -1);
	op = (rw_flag == READING ? VFS_READ : VFS_WRITE);
	r = rw_pipe(rw_flag, for_e, f, op, fd, buf, size, 0 /*cum_io*/);
```

**注释翻译**：`Pipes` → 管道

**是什么**：管道（FIFO）的读写处理。

**为什么**：
- **peek 无意义**：管道是流式数据，不支持 peek
- **`fd != -1`**：管道操作需要有效的文件描述符
- **`rw_pipe`**：专门的管道读写函数，处理阻塞、部分写入等复杂逻辑

---

### 第 162-201 行：字符设备处理

```c
  } else if (S_ISCHR(vp->v_mode)) {	/* Character special files. */
	if(rw_flag == PEEKING) {
	  	printf("read_write: peek on char device makes no sense\n");
		return EINVAL;
	}

	if (vp->v_sdev == NO_DEV)
		panic("VFS: read_write tries to access char dev NO_DEV");

	dev = vp->v_sdev;
	op = (rw_flag == READING ? CDEV_READ : CDEV_WRITE);

	r = cdev_io(op, dev, for_e, buf, position, size, f->filp_flags);
	if (r >= 0) {
		/* This should no longer happen: all calls are asynchronous. */
		printf("VFS: I/O to device %llx succeeded immediately!?\n", dev);
		cum_io = r;
		position += r;
		r = OK;
	} else if (r == SUSPEND) {
		/* FIXME: multiple read/write operations on a single filp
		 * should be serialized. They currently aren't; in order to
		 * achieve a similar effect, we optimistically advance the file
		 * position here. This works under the following assumptions:
		 * - character drivers that use the seek position at all,
		 *   expose a view of a statically-sized range of bytes, i.e.,
		 *   they are basically byte-granular block devices;
		 * - if short I/O or an error is returned, all subsequent calls
		 *   will return (respectively) EOF and an error;
		 * - the application never checks its own file seek position,
		 *   or does not care that it may end up having seeked beyond
		 *   the number of bytes it has actually read;
		 * - communication to the character driver is FIFO (this one
		 *   is actually true! whew).
		 * Many improvements are possible here, but in the end,
		 * anything short of queuing concurrent operations will be
		 * suboptimal - so we settle for this hack for now.
		 */
		position += size;
	}
```

**注释翻译**：
- `Character special files` → 字符特殊文件
- `This should no longer happen: all calls are asynchronous` → 这不应该再发生：所有调用都是异步的
- `FIXME: multiple read/write operations on a single filp should be serialized` → 单个 filp 上的多次读写操作应该序列化
- `They currently aren't; in order to achieve a similar effect, we optimistically advance the file position here` → 它们目前没有；为了达到类似效果，我们在这里乐观地推进文件位置
- `character drivers that use the seek position at all, expose a view of a statically-sized range of bytes` → 使用搜索位置的字符驱动，暴露静态大小字节范围的视图
- `they are basically byte-granular block devices` → 它们本质上是字节粒度的块设备
- `if short I/O or an error is returned, all subsequent calls will return (respectively) EOF and an error` → 如果返回短 I/O 或错误，所有后续调用将分别返回 EOF 和错误
- `the application never checks its own file seek position, or does not care that it may end up having seeked beyond the number of bytes it has actually read` → 应用程序从不检查自己的文件搜索位置，或不关心可能超出实际读取字节数
- `communication to the character driver is FIFO (this one is actually true! whew)` → 与字符驱动的通信是 FIFO（这个实际上是真的！呼）
- `Many improvements are possible here, but in the end, anything short of queuing concurrent operations will be suboptimal - so we settle for this hack for now` → 这里可能有很多改进，但最终，任何不如排队并发操作的都是次优的——所以我们暂时接受这个 hack

**是什么**：字符设备（如 `/dev/null`、终端）的读写。

**为什么**：
- **`cdev_io`**：向字符设备驱动发送 I/O 请求
- **异步返回**：`r >= 0` 表示同步完成（不应再发生），`r == SUSPEND` 表示异步挂起
- **FIXME 注释**：作者承认这是一个 hack。并发读写没有正确序列化，乐观地推进文件位置。这是一个已知的限制。

---

### 第 202-212 行：Socket 处理

```c
  } else if (S_ISSOCK(vp->v_mode)) {
	if (rw_flag == PEEKING) {
		printf("VFS: read_write tries to peek on sock dev\n");
		return EINVAL;
	}

	if (vp->v_sdev == NO_DEV)
		panic("VFS: read_write tries to access sock dev NO_DEV");

	r = sdev_readwrite(vp->v_sdev, buf, size, 0, 0, 0, 0, 0, rw_flag,
	    f->filp_flags, 0);
```

**是什么**：socket 文件的读写。

**为什么**：
- **`sdev_readwrite`**：向 socket 驱动发送请求
- socket 不支持 peek（通过 read/write 接口）

---

### 第 213-230 行：块设备处理

```c
  } else if (S_ISBLK(vp->v_mode)) {	/* Block special files. */
	if (vp->v_sdev == NO_DEV)
		panic("VFS: read_write tries to access block dev NO_DEV");

	lock_bsf();

	if(rw_flag == PEEKING) {
		r = req_bpeek(vp->v_bfs_e, vp->v_sdev, position, size);
	} else {
		r = req_breadwrite(vp->v_bfs_e, for_e, vp->v_sdev, position,
		       size, buf, rw_flag, &res_pos, &res_cum_io);
		if (r == OK) {
			position = res_pos;
			cum_io += res_cum_io;
		}
	}

	unlock_bsf();
```

**注释翻译**：`Block special files` → 块特殊文件

**是什么**：块设备（如磁盘分区）的读写。

**为什么**：
- **`lock_bsf()`**：块设备操作需要全局锁，防止并发访问同一设备
- **`req_bpeek`**：块设备 peek 操作
- **`req_breadwrite`**：块设备读写，向 FS 进程发送请求
- **`vp->v_bfs_e`**：处理块设备的 FS 进程 endpoint
- **`unlock_bsf()`**：操作完成后释放锁

---

### 第 231-251 行：普通文件处理

```c
  } else {				/* Regular files */
	if (rw_flag == WRITING) {
		/* Check for O_APPEND flag. */
		if (f->filp_flags & O_APPEND) position = vp->v_size;
	}

	/* Issue request */
	if(rw_flag == PEEKING) {
		r = req_peek(vp->v_fs_e, vp->v_inode_nr, position, size);
	} else {
		off_t new_pos;
		r = req_readwrite(vp->v_fs_e, vp->v_inode_nr, position,
			rw_flag, for_e, buf, size, &new_pos,
			&cum_io_incr);

		if (r >= 0) {
			position = new_pos;
			cum_io += cum_io_incr;
		}
        }
  }
```

**注释翻译**：
- `Regular files` → 普通文件
- `Check for O_APPEND flag` → 检查 O_APPEND 标志
- `Issue request` → 发起请求

**是什么**：普通文件和目录的读写。

**为什么**：
- **`O_APPEND`**：追加模式写入时，位置设为文件大小
- **`req_peek`**：查看文件数据（不修改状态）
- **`req_readwrite`**：向 FS 进程发送读写请求
- **`vp->v_fs_e`**：文件所在 FS 进程的 endpoint
- **`vp->v_inode_nr`**：inode 编号

**设计思路**：
这是最常见的路径。VFS 通过 IPC 将请求发送给实际的 FS 进程（如 MFS、ext2），FS 进程执行实际的数据读写并返回结果。

---

### 第 253-277 行：结果处理

```c
  /* On write, update file size and access time. */
  if (rw_flag == WRITING) {
	if (S_ISREG(vp->v_mode) || S_ISDIR(vp->v_mode)) {
		if (position > vp->v_size) {
			vp->v_size = position;
		}
	}
  }

  f->filp_pos = position;

  if (r == EPIPE && rw_flag == WRITING) {
	/* Process is writing, but there is no reader. Tell the kernel to
	 * generate a SIGPIPE signal.
	 */
	if (!(f->filp_flags & O_NOSIGPIPE)) {
		sys_kill(rfp->fp_endpoint, SIGPIPE);
	}
  }

  if (r == OK) {
	return(cum_io);
  }
  return(r);
```

**注释翻译**：
- `On write, update file size and access time` → 写入时，更新文件大小和访问时间
- `Process is writing, but there is no reader. Tell the kernel to generate a SIGPIPE signal` → 进程正在写入，但没有读者。告诉内核生成 SIGPIPE 信号

**是什么**：更新文件位置、处理 SIGPIPE、返回结果。

**为什么**：
- **文件大小更新**：写入超过当前文件大小时，更新 `v_size`
- **`filp_pos` 更新**：无论成功失败，都更新文件位置
- **SIGPIPE**：写入管道但没有读者时，发送 SIGPIPE 信号（除非设置了 `O_NOSIGPIPE`）
- **返回值**：成功返回累积字节数，失败返回错误码

---

### 第 279-317 行：do_getdents 函数

```c
/*===========================================================================*
 *				do_getdents				     *
 *===========================================================================*/
int do_getdents(void)
{
/* Perform the getdents(fd, buf, size) system call. */
  int fd, r = OK;
  off_t new_pos;
  vir_bytes buf;
  size_t size;
  register struct filp *rfilp;

  /* This field must always be set to zero for getdents(). */
  if (job_m_in.m_lc_vfs_readwrite.cum_io != 0)
	return(EINVAL);

  fd = job_m_in.m_lc_vfs_readwrite.fd;
  buf = job_m_in.m_lc_vfs_readwrite.buf;
  size = job_m_in.m_lc_vfs_readwrite.len;

  /* Is the file descriptor valid? */
  if ( (rfilp = get_filp(fd, VNODE_READ)) == NULL)
	return(err_code);

  if (!(rfilp->filp_mode & R_BIT))
	r = EBADF;
  else if (!S_ISDIR(rfilp->filp_vno->v_mode))
	r = EBADF;

  if (r == OK) {
	r = req_getdents(rfilp->filp_vno->v_fs_e, rfilp->filp_vno->v_inode_nr,
	    rfilp->filp_pos, buf, size, &new_pos, 0);

	if (r > 0) rfilp->filp_pos = new_pos;
  }

  unlock_filp(rfilp);
  return(r);
}
```

**注释翻译**：
- `Perform the getdents(fd, buf, size) system call` → 执行 getdents 系统调用
- `This field must always be set to zero for getdents()` → 此字段对 getdents 必须始终为零
- `Is the file descriptor valid?` → 文件描述符有效吗？

**是什么**：读取目录条目（getdents 系统调用）。

**为什么**：
- **`cum_io != 0` 检查**：与 read 相同，防止用户态传入非零值
- **`get_filp`**：获取 filp 并加读锁
- **模式检查**：必须是可读的目录
- **`req_getdents`**：向 FS 进程发送 getdents 请求
- **位置更新**：成功时更新文件位置

---

### 第 319-393 行：rw_pipe 函数

```c
/*===========================================================================*
 *				rw_pipe					     *
 *===========================================================================*/
int rw_pipe(int rw_flag, endpoint_t usr_e, struct filp *f, int callnr, int fd,
	vir_bytes buf, size_t nbytes, size_t cum_io)
{
  int r, oflags, partial_pipe = FALSE;
  size_t size;
  size_t cum_io_incr;
  struct vnode *vp;
  off_t  position, new_pos;

  /* Must make sure we're operating on locked filp and vnode */
  assert(tll_locked_by_me(&f->filp_vno->v_lock));
  assert(mutex_trylock(&f->filp_lock) == -EDEADLK);

  oflags = f->filp_flags;
  vp = f->filp_vno;
  position = 0;	/* Not actually used */

  assert(rw_flag == READING || rw_flag == WRITING);

  r = pipe_check(f, rw_flag, oflags, nbytes, 0);
  if (r <= 0) {
	if (r == SUSPEND)
		pipe_suspend(callnr, fd, buf, nbytes, cum_io);

	/* If pipe_check returns an error instead of suspending the call, we
	 * return that error, even if we are resuming a partially completed
	 * operation (ie, a large blocking write), to match NetBSD's behavior.
	 */
	return(r);
  }

  size = r;
  if (size < nbytes) partial_pipe = TRUE;

  /* Truncate read request at size. */
  if (rw_flag == READING && size > vp->v_size) {
	size = vp->v_size;
  }

  if (vp->v_mapfs_e == 0)
	panic("unmapped pipe");

  r = req_readwrite(vp->v_mapfs_e, vp->v_mapinode_nr, position, rw_flag, usr_e,
		    buf, size, &new_pos, &cum_io_incr);

  if (r != OK) {
	assert(r != SUSPEND);
	return(r);
  }

  cum_io += cum_io_incr;
  buf += cum_io_incr;
  nbytes -= cum_io_incr;

  if (rw_flag == READING)
	vp->v_size -= cum_io_incr;
  else
	vp->v_size += cum_io_incr;

  if (partial_pipe) {
	/* partial write on pipe with */
	/* O_NONBLOCK, return write count */
	if (!(oflags & O_NONBLOCK)) {
		/* partial write on pipe with nbytes > PIPE_BUF, non-atomic */
		pipe_suspend(callnr, fd, buf, nbytes, cum_io);
		return(SUSPEND);
	}
  }

  return(cum_io);
}
```

**注释翻译**：
- `Must make sure we're operating on locked filp and vnode` → 必须确保我们在已锁定的 filp 和 vnode 上操作
- `Not actually used` → 实际上未使用
- `If pipe_check returns an error instead of suspending the call, we return that error, even if we are resuming a partially completed operation (ie, a large blocking write), to match NetBSD's behavior` → 如果 pipe_check 返回错误而不是挂起调用，我们返回该错误，即使我们正在恢复部分完成的操作（即大型阻塞写入），以匹配 NetBSD 的行为
- `Truncate read request at size` → 在 size 处截断读请求
- `unmapped pipe` → 未映射的管道
- `partial write on pipe with O_NONBLOCK, return write count` → 带有 O_NONBLOCK 的管道部分写入，返回写入计数
- `partial write on pipe with nbytes > PIPE_BUF, non-atomic` → nbytes > PIPE_BUF 的管道部分写入，非原子

**是什么**：管道读写的核心实现。

**逐段讲解**：

**断言检查**（第 333-334 行）：
- 确保 vnode 锁已由调用者持有
- 确保 filp 锁已由当前线程持有（`-EDEADLK` 表示已持有）

**pipe_check**（第 342-352 行）：
- 检查管道状态（满/空/关闭）
- `r <= 0` 表示错误或需要挂起
- `SUSPEND` 时调用 `pipe_suspend` 保存状态并挂起进程
- 匹配 NetBSD 行为：部分完成的操遇到错误时返回错误

**部分管道处理**（第 354-360 行）：
- `partial_pipe = TRUE`：请求大小超过管道可用空间
- 读操作时截断到管道当前大小

**实际 I/O**（第 362-371 行）：
- 通过 `v_mapfs_e` 和 `v_mapinode_nr` 向管道 FS 发送请求
- 管道使用映射的 FS endpoint（与普通文件不同）
- 断言不会返回 SUSPEND（管道 FS 应同步完成）

**更新状态**（第 373-380 行）：
- 累积 I/O 计数
- 调整缓冲区和剩余字节数
- 读操作减少 `v_size`，写操作增加 `v_size`

**部分写入挂起**（第 382-390 行）：
- 部分写入且非 O_NONBLOCK 时，挂起进程继续写入
- 这实现了管道的阻塞写入语义

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 VFS read_write | Linux VFS |
|------|----------------------|-----------|
| 架构 | 用户态，IPC 到 FS 进程 | 内核态，直接调用 |
| 文件类型分发 | if-else 链（S_ISFIFO/S_ISCHR/...） | `file->f_op->read_iter` |
| 管道实现 | 通过映射的 FS 进程 | 内核 pipe_buffer |
| 字符设备 | 异步 cdev_io + SUSPEND | 直接 file_operations |
| 块设备 | 全局 BSF 锁 | 块层 I/O 调度器 |
| 并发 | 乐观位置推进（hack） | 正确的锁和序列化 |

### Rust 重构建议

```rust
// Minix3 C 代码：if-else 链分发
// if (S_ISFIFO(vp->v_mode)) { ... }
// else if (S_ISCHR(vp->v_mode)) { ... }
// else if (S_ISSOCK(vp->v_mode)) { ... }
// else if (S_ISBLK(vp->v_mode)) { ... }
// else { /* regular file */ }

// Rust 改进：模式匹配 + trait
enum FileType {
    Pipe,
    CharDevice(DeviceId),
    Socket(SocketId),
    BlockDevice(DeviceId),
    RegularFile(InodeId),
}

trait FileOperations {
    async fn read(&self, buf: &mut [u8], pos: u64) -> IoResult<usize>;
    async fn write(&self, buf: &[u8], pos: u64) -> IoResult<usize>;
}

struct PipeFile { vnode: Arc<VNode> }
struct CharDeviceFile { dev: DeviceId }
struct RegularFile { inode: InodeId, fs: Arc<FsClient> }

impl FileOperations for PipeFile {
    async fn read(&self, buf: &mut [u8], _pos: u64) -> IoResult<usize> {
        // 管道读逻辑
    }
    async fn write(&self, buf: &[u8], _pos: u64) -> IoResult<usize> {
        // 管道写逻辑
    }
}

// 统一读写接口
async fn read_write(file: &dyn FileOperations, direction: IoDirection,
                    buf: &mut [u8], pos: u64) -> IoResult<usize> {
    match direction {
        IoDirection::Reading => file.read(buf, pos).await,
        IoDirection::Writing => file.write(buf, pos).await,
    }
}
```

---

## 总结

`read.c`（393 行）是 VFS 最核心的 I/O 文件：

1. **统一入口**：`do_read_write_peek` → `actual_read_write_peek` → `read_write`
2. **文件类型分发**：管道、字符设备、socket、块设备、普通文件
3. **管道实现**：通过映射的 FS 进程，支持阻塞/非阻塞、部分写入
4. **BSF 锁**：全局锁保护块设备并发访问
5. **已知限制**：字符设备并发读写未正确序列化（FIXME hack）

关键设计模式：
- **策略模式**：根据文件类型选择不同处理策略
- **异步 I/O**：字符/块设备通过 SUSPEND 机制异步处理
- **代码复用**：read/write 共享 `read_write` 核心逻辑
