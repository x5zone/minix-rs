# servers/vfs/link.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vfs/link.c`
> **核心功能**: link/unlink 系统调用实现

---

## 文件概述

这个文件实现了 link、unlink、rename 等系统调用。

**核心概念**: 硬链接，符号链接，文件删除。

---

## 逐行讲解

### 文件注释

```c
/* This file handles the LINK and UNLINK system calls.  It also deals with
 * deallocating the storage used by a file when the last UNLINK is done.
 */
```

**讲解**:
- link: 创建硬链接
- unlink: 删除文件
- rename: 重命名文件

---

### do_link 入口

```c
int do_link(void)
{
  int r = OK;
  struct vnode *vp = NULL, *dirp = NULL;
  struct vmnt *vmp1 = NULL, *vmp2 = NULL;
  char fullpath[PATH_MAX];
  struct lookup resolve;
  vir_bytes vname1, vname2;

  vname1 = job_m_in.m_lc_vfs_link.name1;
  vname2 = job_m_in.m_lc_vfs_link.name2;

  lookup_init(&resolve, fullpath, PATH_NOFLAGS, &vmp1, &vp);
```

**讲解**:
- name1: 原文件路径
- name2: 新链接路径
- 查找原文件 vnode

---

## 要点总结

1. **link**: 创建硬链接
2. **unlink**: 删除链接
3. **引用计数**: 文件删除时机

---

## 互动自测

1. **问题**: 硬链接和符号链接的区别？
   **答案**: 硬链接指向同一 inode，符号链接是独立文件存储目标路径。
