# servers/vfs/lock.c 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/lock.c`  
**模块归属**: VFS（虚拟文件系统服务器）  
**核心功能**: 实现 POSIX 建议性文件锁定

---

## 逐行讲解

### 1. 文件头注释

```c
/* This file handles advisory file locking as required by POSIX.
 *
 * The entry points into this file are
 *   lock_op:	perform locking operations for FCNTL system call
 *   lock_revive: revive processes when a lock is released
 */
```

**第1-6行**: 文件头注释  
- **建议性文件锁定**: POSIX 要求的建议性锁定
- **入口点**: lock_op、lock_revive

**设计原因**: 实现 POSIX 文件锁定

---

### 2. 包含头文件

```c
#include "fs.h"
#include <minix/com.h>
#include <minix/u64.h>
#include <fcntl.h>
#include <unistd.h>
#include <assert.h>
#include "file.h"
#include "lock.h"
#include "vnode.h"
```

**第8-18行**: 包含头文件  
- `fs.h`: VFS 主头文件
- `minix/com.h`: Minix 通信
- `minix/u64.h`: 64 位整数
- `fcntl.h`: 文件控制
- `unistd.h`: POSIX 系统调用
- `assert.h`: 断言宏
- `file.h`: 文件表项定义
- `lock.h`: 文件锁定义
- `vnode.h`: vnode 定义

---

### 3. lock_op 函数

```c
/*===========================================================================*
 *				lock_op					     *
 *===========================================================================*/
int lock_op(int fd, int req, vir_bytes arg)
{
/* Perform the advisory locking required by POSIX. */
  int r, ltype, i, conflict = 0, unlocking = 0;
  mode_t mo;
  off_t first, last;
  struct filp *f;
  struct flock flock;
  struct file_lock *flp, *flp2, *empty;

  assert(req == F_GETLK || req == F_SETLK || req == F_SETLKW);

  f = fp->fp_filp[fd];
  assert(f != NULL);

  /* Fetch the flock structure from user space. */
  r = sys_datacopy_wrapper(who_e, arg, VFS_PROC_NR, (vir_bytes)&flock,
      sizeof(flock));
  if (r != OK) return(EINVAL);

  /* Make some error checks. */
  ltype = flock.l_type;
  mo = f->filp_mode;
  if (ltype != F_UNLCK && ltype != F_RDLCK && ltype != F_WRLCK) return(EINVAL);
  if (req == F_GETLK && ltype == F_UNLCK) return(EINVAL);
  if (!S_ISREG(f->filp_vno->v_mode) && !S_ISBLK(f->filp_vno->v_mode))
	return(EINVAL);
  if (req != F_GETLK && ltype == F_RDLCK && (mo & R_BIT) == 0) return(EBADF);
  if (req != F_GETLK && ltype == F_WRLCK && (mo & W_BIT) == 0) return(EBADF);
```

**第20-53行**: 执行建议性锁定操作  
- **参数**: 
  - `fd`: 文件描述符
  - `req`: 请求类型（F_GETLK、F_SETLK、F_SETLKW）
  - `arg`: flock 结构地址
- **断言**: 检查请求类型
- **获取文件表项**: 从进程文件描述符表获取
- **拷贝 flock**: 从用户空间拷贝
- **错误检查**: 
  - 锁类型有效
  - 文件类型有效（常规文件或块设备）
  - 访问权限匹配

**设计原因**: 
- **POSIX 兼容**: 实现 POSIX 文件锁定
- **错误检查**: 全面检查错误

---

### 4. 计算锁定范围

```c
  /* Compute the first and last bytes in the lock region. */
  switch (flock.l_whence) {
    case SEEK_SET:	first = 0; break;
    case SEEK_CUR:	first = f->filp_pos; break;
    case SEEK_END:	first = f->filp_vno->v_size; break;
    default:	return(EINVAL);
  }

  /* Check for overflow. */
  if (((long) flock.l_start > 0) && ((first + flock.l_start) < first))
	return(EINVAL);
  if (((long) flock.l_start < 0) && ((first + flock.l_start) > first))
	return(EINVAL);
  first = first + flock.l_start;
  last = first + flock.l_len - 1;
  if (flock.l_len == 0) last = MAX_FILE_POS;
  if (last < first) return(EINVAL);
```

**第55-74行**: 计算锁定区域的起始和结束字节  
- **whence**: 根据 `l_whence` 计算起始位置
  - `SEEK_SET`: 从文件开头
  - `SEEK_CUR`: 从当前位置
  - `SEEK_END`: 从文件末尾
- **溢出检查**: 检查加法溢出
- **计算**: 计算起始和结束字节
- **特殊处理**: 如果 `l_len == 0`，锁定到文件末尾

**设计原因**: 
- **灵活**: 支持多种起始位置
- **安全**: 检查溢出

---

### 5. 检查冲突

```c
  /* Check if this region conflicts with any existing lock. */
  empty = NULL;
  for (flp = &file_lock[0]; flp < &file_lock[NR_LOCKS]; flp++) {
	if (flp->lock_type == 0) {
		if (empty == NULL) empty = flp;
		continue;	/* 0 means unused slot */
	}
	if (flp->lock_vnode != f->filp_vno) continue;	/* different file */
	if (last < flp->lock_first) continue;	/* new one is in front */
	if (first > flp->lock_last) continue;	/* new one is afterwards */
	if (ltype == F_RDLCK && flp->lock_type == F_RDLCK) continue;
	if (ltype != F_UNLCK && flp->lock_pid == fp->fp_pid) continue;

	/* There might be a conflict.  Process it. */
	conflict = 1;
	if (req == F_GETLK) break;

	/* If we are trying to set a lock, it just failed. */
	if (ltype == F_RDLCK || ltype == F_WRLCK) {
		if (req == F_SETLK) {
			/* For F_SETLK, just report back failure. */
			return(EAGAIN);
		} else {
			/* For F_SETLKW, suspend the process. */
			fp->fp_flock.fd = fd;
			fp->fp_flock.cmd = req;
			fp->fp_flock.arg = arg;
			suspend(FP_BLOCKED_ON_FLOCK);
			return(SUSPEND);
		}
	}
```

**第76-111行**: 检查是否与现有锁冲突  
- **遍历**: 遍历所有文件锁
- **跳过**: 
  - 未使用的槽位
  - 不同文件
  - 不重叠的区域
  - 读锁与读锁不冲突
  - 同一进程的锁不冲突
- **冲突处理**: 
  - `F_GETLK`: 返回冲突锁信息
  - `F_SETLK`: 返回 `EAGAIN`
  - `F_SETLKW`: 挂起进程

**设计原因**: 
- **冲突检测**: 检测锁冲突
- **挂起**: 支持阻塞等待

---

### 6. 解锁处理

```c
	/* We are clearing a lock and we found something that overlaps. */
	unlocking = 1;
	if (first <= flp->lock_first && last >= flp->lock_last) {
		flp->lock_type = 0;	/* mark slot as unused */
		nr_locks--;		/* number of locks is now 1 less */
		continue;
	}

	/* Part of a locked region has been unlocked. */
	if (first <= flp->lock_first) {
		flp->lock_first = last + 1;
		continue;
	}

	if (last >= flp->lock_last) {
		flp->lock_last = first - 1;
		continue;
	}

	/* Bad luck. A lock has been split in two by unlocking the middle. */
	if (nr_locks == NR_LOCKS) return(ENOLCK);
	for (i = 0; i < NR_LOCKS; i++)
		if (file_lock[i].lock_type == 0) break;
	flp2 = &file_lock[i];
	flp2->lock_type = flp->lock_type;
	flp2->lock_pid = flp->lock_pid;
	flp2->lock_vnode = flp->lock_vnode;
	flp2->lock_first = last + 1;
	flp2->lock_last = flp->lock_last;
	flp->lock_last = first - 1;
	nr_locks++;
  }
  if (unlocking) lock_revive();
```

**第113-149行**: 处理解锁  
- **完全覆盖**: 如果解锁区域完全覆盖锁，删除锁
- **部分解锁**: 
  - 前部分解锁：调整起始位置
  - 后部分解锁：调整结束位置
  - 中间解锁：分割锁
- **分割**: 如果解锁中间部分，需要分割锁
- **唤醒**: 调用 `lock_revive` 唤醒等待进程

**设计原因**: 
- **灵活解锁**: 支持部分解锁
- **分割**: 支持锁分割

---

### 7. F_GETLK 处理

```c
  if (req == F_GETLK) {
	if (conflict) {
		/* GETLK and conflict. Report on the conflicting lock. */
		flock.l_type = flp->lock_type;
		flock.l_whence = SEEK_SET;
		flock.l_start = flp->lock_first;
		flock.l_len = flp->lock_last - flp->lock_first + 1;
		flock.l_pid = flp->lock_pid;

	} else {
		/* It is GETLK and there is no conflict. */
		flock.l_type = F_UNLCK;
	}

	/* Copy the flock structure back to the caller. */
	r = sys_datacopy_wrapper(VFS_PROC_NR, (vir_bytes)&flock, who_e, arg,
```

**第151-169行**: 处理 F_GETLK 请求  
- **冲突**: 如果有冲突，返回冲突锁信息
- **无冲突**: 如果无冲突，返回 `F_UNLCK`
- **拷贝**: 拷贝 flock 结构回用户空间

**设计原因**: 
- **查询**: 查询锁状态
- **测试**: 测试是否可以获取锁

---

## 要点总结

### 1. 核心知识点

1. **建议性锁定**: POSIX 建议性文件锁定
2. **字节范围锁**: 支持文件部分锁定
3. **冲突检测**: 检测锁冲突

### 2. 设计亮点

- **灵活**: 支持多种锁定方式
- **挂起**: 支持阻塞等待
- **分割**: 支持锁分割

### 3. 内存模型

```
文件锁冲突检测:
┌─────────────────────────────────┐
│ 现有锁: [100, 199]              │
│ 新锁:   [150, 249]              │
│ 冲突:   [150, 199]              │
└─────────────────────────────────┘

解锁分割:
┌─────────────────────────────────┐
│ 原锁:   [100, 299]              │
│ 解锁:   [150, 199]              │
│ 结果:   [100, 149], [200, 299]  │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 锁表满

**后果**: 
- 返回 `ENOLCK`
- 无法创建新锁

**症状**: 文件锁定失败

### 场景 2: 死锁

**后果**: 
- 进程永久挂起
- 系统死锁

**症状**: 进程挂起

### 场景 3: 进程崩溃

**后果**: 
- 锁未释放
- 其他进程无法锁定

**症状**: 文件锁定挂起

---

## 互动自测

### 问题 1: F_SETLK vs F_SETLKW

**问**: F_SETLK 和 F_SETLKW 有什么区别？

**答**: 
- **F_SETLK**: 非阻塞，立即返回
- **F_SETLKW**: 阻塞，等待锁释放
- **W**: Wait 的意思

### 问题 2: 读锁 vs 写锁

**问**: 读锁和写锁有什么区别？

**答**: 
- **读锁**: 共享锁，多个读锁可以共存
- **写锁**: 独占锁，与任何锁互斥
- **兼容**: 读锁与读锁兼容

### 问题 3: 建议锁

**问**: 为什么使用建议锁而不是强制锁？

**答**: 
- **灵活**: 进程可以选择是否遵守
- **性能**: 内核不强制检查，性能更好
- **兼容**: POSIX 标准要求

---

## Rust 实现对比

### C 版本（原始）

```c
int lock_op(int fd, int req, vir_bytes arg)
{
  int r, ltype, i, conflict = 0, unlocking = 0;
  mode_t mo;
  off_t first, last;
  struct filp *f;
  struct flock flock;
  struct file_lock *flp, *flp2, *empty;

  assert(req == F_GETLK || req == F_SETLK || req == F_SETLKW);

  f = fp->fp_filp[fd];
  assert(f != NULL);

  r = sys_datacopy_wrapper(who_e, arg, VFS_PROC_NR, (vir_bytes)&flock,
      sizeof(flock));
  if (r != OK) return(EINVAL);

  ltype = flock.l_type;
  mo = f->filp_mode;
  if (ltype != F_UNLCK && ltype != F_RDLCK && ltype != F_WRLCK) return(EINVAL);
  // ...
}
```

### Rust 版本（安全抽象）

```rust
fn lock_op(fd: i32, req: i32, arg: vir_bytes) -> Result<(), i32> {
    assert!(req == F_GETLK || req == F_SETLK || req == F_SETLKW);

    let f = fp.fp_filp[fd as usize].as_ref().ok_or(EBADF)?;

    let mut flock: Flock = unsafe { 
        core::ptr::read(arg as *const Flock) 
    };

    let ltype = flock.l_type;
    let mo = f.filp_mode;
    
    if ltype != F_UNLCK && ltype != F_RDLCK && ltype != F_WRLCK {
        return Err(EINVAL);
    }
    // ...
}
```

### 关键改进

1. **Result**: 使用 `Result` 返回错误
2. **Option**: 使用 `Option` 表示可能为空的指针
3. **安全**: 使用安全拷贝

---

## 理论关联

### 1. 文件锁定

**操作系统概念**: 文件锁定控制对文件的并发访问

**Minix3 实现**:
- 建议性锁定
- 字节范围锁
- 冲突检测

### 2. 建议锁

**操作系统概念**: 建议锁需要进程主动检查

**Minix3 实现**:
- 内核不强制检查
- 进程通过 fcntl 检查
- 依赖进程协作

### 3. 死锁

**操作系统概念**: 死锁是进程相互等待

**Minix3 实现**:
- F_SETLKW 可能导致死锁
- 需要进程小心使用
- 没有死锁检测

---

## 总结

`lock.c` 实现了 Minix3 VFS 的 POSIX 建议性文件锁定。通过字节范围锁、冲突检测、锁分割等设计，实现了灵活、兼容的文件锁定机制。理解文件锁的实现是理解 VFS 并发控制的关键。
