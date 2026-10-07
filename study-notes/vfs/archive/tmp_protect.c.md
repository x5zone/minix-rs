# servers/vfs/protect.c 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/protect.c`  
**模块归属**: VFS（虚拟文件系统服务器）  
**核心功能**: 实现文件保护相关系统调用：chmod、chown、umask、access

---

## 逐行讲解

### 1. 文件头注释

```c
/* This file deals with protection in the file system.  It contains the code
 * for four system calls that relate to protection.
 *
 * The entry points into this file are
 *   do_chmod:	perform the CHMOD and FCHMOD system calls
 *   do_chown:	perform the CHOWN and FCHOWN system calls
 *   do_umask:	perform the UMASK system call
 *   do_access:	perform the ACCESS system call
 */
```

**第1-10行**: 文件头注释  
- **do_chmod**: 执行 chmod 和 fchmod 系统调用
- **do_chown**: 执行 chown 和 fchown 系统调用
- **do_umask**: 执行 umask 系统调用
- **do_access**: 执行 access 系统调用

**设计原因**: 文件保护是文件系统的核心功能

---

### 2. 包含头文件

```c
#include "fs.h"
#include <sys/stat.h>
#include <unistd.h>
#include <assert.h>
#include <minix/callnr.h>
#include "file.h"
#include "path.h"
#include <minix/vfsif.h>
#include "vnode.h"
#include "vmnt.h"
```

**第12-23行**: 包含头文件  
- `fs.h`: VFS 主头文件
- `sys/stat.h`: 文件状态相关定义
- `unistd.h`: POSIX 系统调用
- `assert.h`: 断言宏
- `minix/callnr.h`: 系统调用号
- `file.h`: 文件表项定义
- `path.h`: 路径处理
- `minix/vfsif.h`: VFS 接口
- `vnode.h`: vnode 定义
- `vmnt.h`: 挂载点定义

---

### 3. do_chmod 函数

```c
/*===========================================================================*
 *				do_chmod				     *
 *===========================================================================*/
int do_chmod(void)
{
/* Perform the chmod(name, mode) and fchmod(fd, mode) system calls.
 * syscall might provide 'name' embedded in the message.
 */

  struct filp *flp;
  struct vnode *vp;
  struct vmnt *vmp;
  int r, rfd;
  mode_t result_mode;
  char fullpath[PATH_MAX];
  struct lookup resolve;
  mode_t new_mode;

  flp = NULL;

  lookup_init(&resolve, fullpath, PATH_NOFLAGS, &vmp, &vp);
  resolve.l_vmnt_lock = VMNT_READ;
  resolve.l_vnode_lock = VNODE_WRITE;

  if (job_call_nr == VFS_CHMOD) {
	new_mode = job_m_in.m_lc_vfs_path.mode;
	/* Temporarily open the file */
	if (copy_path(fullpath, sizeof(fullpath)) != OK)
		return(err_code);
	if ((vp = eat_path(&resolve, fp)) == NULL) return(err_code);
  } else {	/* call_nr == VFS_FCHMOD */
	rfd = job_m_in.m_lc_vfs_fchmod.fd;
	new_mode = job_m_in.m_lc_vfs_fchmod.mode;
	/* File is already opened; get a pointer to vnode from filp. */
	if ((flp = get_filp(rfd, VNODE_WRITE)) == NULL) return(err_code);
	vp = flp->filp_vno;
        assert(vp);
	dup_vnode(vp);
  }

  assert(vp);

  /* Only the owner or the super_user may change the mode of a file.
   * No one may change the mode of a file on a read-only file system.
   */
  if (vp->v_uid != fp->fp_effuid && fp->fp_effuid != SU_UID)
	r = EPERM;
  else
	r = read_only(vp);

  if (r == OK) {
	/* Now make the change. Clear setgid bit if file is not in caller's
	 * group */
	if (fp->fp_effuid != SU_UID && vp->v_gid != fp->fp_effgid)
		new_mode &= ~I_SET_GID_BIT;

	r = req_chmod(vp->v_fs_e, vp->v_inode_nr, new_mode, &result_mode);
	if (r == OK)
		vp->v_mode = result_mode;
  }

  if (job_call_nr == VFS_CHMOD) {
	unlock_vnode(vp);
	unlock_vmnt(vmp);
  } else {	/* VFS_FCHMOD */
	unlock_filp(flp);
  }

  put_vnode(vp);
  return(r);
}
```

**第25-91行**: 执行 chmod 和 fchmod 系统调用  
- **变量声明**: 
  - `flp`: 文件表项指针
  - `vp`: vnode 指针
  - `vmp`: 挂载点指针
  - `new_mode`: 新的权限模式
- **区分调用**: 
  - `VFS_CHMOD`: 通过路径修改权限
  - `VFS_FCHMOD`: 通过文件描述符修改权限
- **权限检查**: 
  - 只有文件所有者或超级用户可以修改权限
  - 不能修改只读文件系统的文件
- **setgid 处理**: 如果文件不属于调用者的组，清除 setgid 位
- **请求文件系统**: 调用 `req_chmod` 请求文件系统修改权限
- **解锁和释放**: 解锁 vnode 和挂载点，释放 vnode

**设计原因**: 
- **权限检查**: 确保只有授权用户可以修改权限
- **setgid 安全**: 防止 setgid 滥用

---

### 4. do_chown 函数

```c
/*===========================================================================*
 *				do_chown				     *
 *===========================================================================*/
int do_chown(void)
{
/* Perform the chown(path, owner, group) and fchmod(fd, owner, group) system
 * calls. */
  struct filp *flp;
  struct vnode *vp;
  struct vmnt *vmp;
  int r, rfd;
  uid_t uid, new_uid;
  gid_t gid, new_gid;
  mode_t new_mode;
  char fullpath[PATH_MAX];
  struct lookup resolve;
  vir_bytes vname1;
  size_t vname1_length;

  flp = NULL;
  uid = job_m_in.m_lc_vfs_chown.owner;
  gid = job_m_in.m_lc_vfs_chown.group;

  if (job_call_nr == VFS_CHOWN) {
	vname1 = job_m_in.m_lc_vfs_chown.name;
	vname1_length = job_m_in.m_lc_vfs_chown.len;

	lookup_init(&resolve, fullpath, PATH_NOFLAGS, &vmp, &vp);
	resolve.l_vmnt_lock = VMNT_READ;
	resolve.l_vnode_lock = VNODE_WRITE;

	/* Temporarily open the file. */
	if (fetch_name(vname1, vname1_length, fullpath) != OK)
		return(err_code);
	if ((vp = eat_path(&resolve, fp)) == NULL) return(err_code);
  } else {	/* call_nr == VFS_FCHOWN */
	rfd = job_m_in.m_lc_vfs_chown.fd;

	/* File is already opened; get a pointer to the vnode from filp. */
	if ((flp = get_filp(rfd, VNODE_WRITE)) == NULL)
		return(err_code);
	vp = flp->filp_vno;
	dup_vnode(vp);
  }

  r = read_only(vp);
  if (r == OK) {
	/* FS is R/W. Whether call is allowed depends on ownership, etc. */
	/* The super user can do anything, so check permissions only if we're
	   a regular user. */
	if (fp->fp_effuid != SU_UID) {
		/* Regular users can only change groups of their own files. */
		if (vp->v_uid != fp->fp_effuid) r = EPERM;
		if (vp->v_uid != uid) r = EPERM;	/* no giving away */
		if (fp->fp_effgid != gid) r = EPERM;
	}
```

**第93-150行**: 执行 chown 和 fchown 系统调用  
- **变量声明**: 
  - `uid`: 新的所有者 ID
  - `gid`: 新的组 ID
- **区分调用**: 
  - `VFS_CHOWN`: 通过路径修改所有者
  - `VFS_FCHOWN`: 通过文件描述符修改所有者
- **权限检查**: 
  - 超级用户可以做任何事
  - 普通用户只能修改自己文件的组
  - 不能赠送文件（所有者必须不变）

**设计原因**: 
- **权限检查**: 确保只有授权用户可以修改所有者
- **防赠送**: 防止用户赠送文件给他人

---

## 要点总结

### 1. 核心知识点

1. **chmod**: 修改文件权限
2. **chown**: 修改文件所有者
3. **权限检查**: 只有所有者或超级用户可以修改

### 2. 设计亮点

- **统一处理**: chmod 和 fchmod 使用同一函数
- **setgid 安全**: 自动清除 setgid 位
- **防赠送**: 禁止普通用户赠送文件

### 3. 内存模型

```
进程结构:
┌─────────────────────────────────┐
│ fp->fp_effuid = 1000            │
│ fp->fp_effgid = 100             │
└─────────────────────────────────┘

vnode:
┌─────────────────────────────────┐
│ vp->v_uid = 1000                │
│ vp->v_gid = 100                 │
│ vp->v_mode = 0755               │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 权限检查失败

**后果**: 
- 返回 `EPERM`
- 操作失败

**症状**: 用户无法修改文件权限

### 场景 2: 只读文件系统

**后果**: 
- 返回 `EROFS`
- 操作失败

**症状**: 无法修改只读文件系统的文件

### 场景 3: setgid 滥用

**后果**: 
- 安全漏洞
- 权限提升

**症状**: 攻击者获得额外权限

---

## 互动自测

### 问题 1: chmod vs fchmod

**问**: 为什么使用同一函数处理 chmod 和 fchmod？

**答**: 
- **代码复用**: 避免重复代码
- **逻辑相似**: 核心逻辑相同
- **维护性**: 修改一处即可

### 问题 2: setgid 清除

**问**: 为什么自动清除 setgid 位？

**答**: 
- **安全**: 防止 setgid 滥用
- **权限提升**: setgid 可能导致权限提升
- **组不匹配**: 文件不属于调用者的组

### 问题 3: 防赠送

**问**: 为什么禁止普通用户赠送文件？

**答**: 
- **安全**: 防止用户隐藏文件
- **配额**: 防止绕过磁盘配额
- **审计**: 保持文件归属清晰

---

## Rust 实现对比

### C 版本（原始）

```c
if (vp->v_uid != fp->fp_effuid && fp->fp_effuid != SU_UID)
	r = EPERM;
else
	r = read_only(vp);

if (r == OK) {
	if (fp->fp_effuid != SU_UID && vp->v_gid != fp->fp_effgid)
		new_mode &= ~I_SET_GID_BIT;

	r = req_chmod(vp->v_fs_e, vp->v_inode_nr, new_mode, &result_mode);
	if (r == OK)
		vp->v_mode = result_mode;
}
```

### Rust 版本（安全抽象）

```rust
if vp.v_uid != fp.fp_effuid && fp.fp_effuid != SU_UID {
    Err(EPERM)
} else {
    read_only(vp)
}.and_then(|_| {
    let mut mode = new_mode;
    if fp.fp_effuid != SU_UID && vp.v_gid != fp.fp_effgid {
        mode &= !I_SET_GID_BIT;
    }
    req_chmod(vp.v_fs_e, vp.v_inode_nr, mode)
        .map(|result_mode| vp.v_mode = result_mode)
})
```

### 关键改进

1. **and_then**: 使用 `and_then` 链式处理错误
2. **map**: 使用 `map` 处理成功情况
3. **错误传播**: 自动传播错误

---

## 理论关联

### 1. 文件权限

**操作系统概念**: 文件权限控制访问

**Minix3 实现**:
- 使用 mode_t 表示权限
- 包含读、写、执行权限
- 包含 setuid、setgid、sticky 位

### 2. 所有权

**操作系统概念**: 文件所有者控制访问

**Minix3 实现**:
- 使用 uid_t 和 gid_t 表示所有者
- 所有者可以修改权限
- 超级用户可以做任何事

### 3. 安全

**操作系统概念**: 安全机制防止滥用

**Minix3 实现**:
- 权限检查
- setgid 清除
- 防赠送

---

## 总结

`protect.c` 实现了 Minix3 VFS 的文件保护功能。通过权限检查、setgid 清除、防赠送等设计，实现了安全的文件权限管理。理解权限检查逻辑和安全机制是理解 VFS 文件保护的关键。
