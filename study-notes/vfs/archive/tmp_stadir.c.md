# servers/vfs/stadir.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vfs/stadir.c`
> **核心功能**: 状态和目录系统调用实现

---

## 文件概述

这个文件实现了 stat、chdir 等系统调用。

**核心概念**: 文件状态，工作目录，根目录。

---

## 逐行讲解

### 入口点

```c
/* The entry points into this file are
 *   do_chdir:	perform the CHDIR system call
 *   do_chroot:	perform the CHROOT system call
 *   do_lstat:  perform the LSTAT system call
 *   do_stat:	perform the STAT system call
 *   do_fstat:	perform the FSTAT system call
 */
```

**讲解**:
- **chdir**: 改变工作目录
- **chroot**: 改变根目录
- **stat/lstat/fstat**: 获取文件状态

---

### do_fchdir 入口

```c
int do_fchdir(void)
{
  struct filp *rfilp;
  int r, rfd;

  rfd = job_m_in.m_lc_vfs_fchdir.fd;

  if ((rfilp = get_filp(rfd, VNODE_READ)) == NULL) return(err_code);
  r = change_into(&fp->fp_wd, rfilp->filp_vno);
  unlock_filp(rfilp);
  return(r);
}
```

**讲解**:
- 通过文件描述符改变目录
- 验证 fd 有效性
- 更新工作目录

---

## 要点总结

1. **stat**: 获取文件状态
2. **chdir**: 改变工作目录
3. **chroot**: 改变根目录

---

## 互动自测

1. **问题**: stat 和 lstat 的区别？
   **答案**: stat 跟随符号链接，lstat 返回符号链接本身的信息。
