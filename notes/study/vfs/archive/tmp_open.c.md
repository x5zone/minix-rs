# open.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vfs/open.c`
> 
> **行数**: 727 行
> 
> **核心内容**: open/creat/close/lseek/mknod/mkdir 系统调用实现

---

## 文件概述

`open.c` 是 VFS 中**最复杂的文件之一**，实现了文件生命周期管理的核心操作：
1. **do_open / do_creat**：打开/创建文件
2. **common_open**：统一的打开逻辑（核心）
3. **new_node**：创建新 inode
4. **pipe_open**：管道打开和阻塞/唤醒
5. **do_mknod / do_mkdir**：创建设备节点和目录
6. **do_lseek / actual_lseek**：文件定位
7. **do_close / close_fd**：关闭文件

---

## 逐行讲解

### 第 1-10 行：文件头注释

```c
/* This file contains the procedures for creating, opening, closing, and
 * seeking on files.
 *
 * The entry points into this file are
 *   do_open:	perform the OPEN system call
 *   do_mknod:	perform the MKNOD system call
 *   do_mkdir:	perform the MKDIR system call
 *   do_close:	perform the CLOSE system call
 *   do_lseek:  perform the LSEEK system call
 */
```

**注释翻译**：
- `This file contains the procedures for creating, opening, closing, and seeking on files` → 此文件包含创建、打开、关闭和定位文件的过程

**设计思路**：open.c 涵盖了文件的完整生命周期：创建 → 打开 → 读写 → 定位 → 关闭。

---

### 第 12-28 行：头文件包含

```c
#include "fs.h"
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>
#include <unistd.h>
#include <minix/callnr.h>
#include <minix/com.h>
#include <minix/u64.h>
#include "file.h"
#include "lock.h"
#include <sys/dirent.h>
#include <assert.h>
#include <minix/vfsif.h>
#include "vnode.h"
#include "vmnt.h"
#include "path.h"
```

**关键头文件**：
- `sys/stat.h`：文件模式宏（S_IFREG、S_IFDIR 等）
- `fcntl.h`：打开标志（O_CREAT、O_TRUNC、O_APPEND 等）
- `lock.h`：文件锁定义
- `path.h`：路径解析结构（`struct lookup`）

---

### 第 29 行：模式映射表

```c
static char mode_map[] = {R_BIT, W_BIT, R_BIT|W_BIT, 0};
```

**是什么**：将 O_ACCMODE 映射到读写权限位。

**为什么**：
- `O_ACCMODE` 值为 0-3：`O_RDONLY=0`、`O_WRONLY=1`、`O_RDWR=2`、`无效=3`
- `mode_map[0] = R_BIT`：只读 → 读权限
- `mode_map[1] = W_BIT`：只写 → 写权限
- `mode_map[2] = R_BIT|W_BIT`：读写 → 读写权限
- `mode_map[3] = 0`：无效 → 0（用于错误检测）

---

### 第 31-33 行：静态函数原型

```c
static struct vnode *new_node(struct lookup *resolve, int oflags,
	mode_t bits);
static int pipe_open(int fd, struct vnode *vp, mode_t bits, int oflags);
```

**是什么**：声明内部辅助函数。

---

### 第 35-53 行：do_open 函数

```c
/*===========================================================================*
 *				do_open					     *
 *===========================================================================*/
int do_open(void)
{
/* Perform the open(name, flags) system call with O_CREAT *not* set. */
  int open_flags;
  char fullpath[PATH_MAX];

  open_flags = job_m_in.m_lc_vfs_path.flags;

  if (open_flags & O_CREAT)
	return EINVAL;

  if (copy_path(fullpath, sizeof(fullpath)) != OK)
	return(err_code);

  return common_open(fullpath, open_flags, 0 /*omode*/, FALSE /*for_exec*/);
}
```

**注释翻译**：
- `Perform the open(name, flags) system call with O_CREAT *not* set` → 执行 open(name, flags) 系统调用，O_CREAT 未设置

**是什么**：不带 O_CREAT 的 open 系统调用入口。

**为什么**：
- **O_CREAT 检查**：如果设置了 O_CREAT，返回 EINVAL（应使用 do_creat）
- **copy_path**：从用户态复制路径到内核态缓冲区
- **common_open**：调用统一的打开逻辑，omode=0（无创建模式），for_exec=FALSE

---

### 第 55-78 行：do_creat 函数

```c
/*===========================================================================*
 *				do_creat				     *
 *===========================================================================*/
int do_creat(void)
{
/* Perform the open(name, flags, mode) system call with O_CREAT set. */
  int open_flags, create_mode;
  char fullpath[PATH_MAX];
  vir_bytes vname;
  size_t vname_length;

  vname = job_m_in.m_lc_vfs_creat.name;
  vname_length = job_m_in.m_lc_vfs_creat.len;
  open_flags = job_m_in.m_lc_vfs_creat.flags;
  create_mode = job_m_in.m_lc_vfs_creat.mode;

  if (!(open_flags & O_CREAT))
	return(EINVAL);

  if (fetch_name(vname, vname_length, fullpath) != OK)
	return(err_code);

  return common_open(fullpath, open_flags, create_mode, FALSE /*for_exec*/);
}
```

**注释翻译**：
- `Perform the open(name, flags, mode) system call with O_CREAT set` → 执行 open(name, flags, mode) 系统调用，O_CREAT 已设置

**是什么**：带 O_CREAT 的 open 系统调用入口。

**为什么**：
- **O_CREAT 检查**：必须设置 O_CREAT，否则返回 EINVAL
- **fetch_name**：从用户态获取路径名
- **create_mode**：文件创建模式（权限位）

---

### 第 80-104 行：common_open 函数头

```c
/*===========================================================================*
 *				common_open				     *
 *===========================================================================*/
int common_open(char path[PATH_MAX], int oflags, mode_t omode, int for_exec)
{
/* Common code from do_creat and do_open. */
  int b, r, exist = TRUE;
  devmajor_t major_dev;
  dev_t dev;
  mode_t bits;
  struct filp *filp, *filp2;
  struct vnode *vp;
  struct vmnt *vmp;
  struct dmap *dp;
  struct lookup resolve;
  int fd, start = 0;

  /* Remap the bottom two bits of oflags. */
  bits = (mode_t) mode_map[oflags & O_ACCMODE];
  if (!bits) return(EINVAL);

  /* See if file descriptor and filp slots are available. */
  if ((r = get_fd(fp, start, bits, &fd, &filp)) != OK)
	return(r);

  lookup_init(&resolve, path, PATH_NOFLAGS, &vmp, &vp);
```

**注释翻译**：
- `Common code from do_creat and do_open` → do_creat 和 do_open 的公共代码
- `Remap the bottom two bits of oflags` → 重新映射 oflags 的低两位
- `See if file descriptor and filp slots are available` → 检查文件描述符和 filp 槽位是否可用

**是什么**：open/creat 的统一实现，处理所有文件类型的打开逻辑。

**为什么**：
- **mode_map 映射**：将 O_ACCMODE 转换为 R_BIT/W_BIT 权限
- **get_fd**：分配文件描述符和 filp 槽位
- **lookup_init**：初始化路径解析结构

---

### 第 107-130 行：O_CREAT 处理

```c
  /* If O_CREATE is set, try to make the file. */
  if (oflags & O_CREAT) {
        omode = I_REGULAR | (omode & ALLPERMS & fp->fp_umask);
	vp = new_node(&resolve, oflags, omode);
	r = err_code;
	if (r == OK) exist = FALSE;	/* We just created the file */
	else if (r != EEXIST) {		/* other error */
		if (vp) unlock_vnode(vp);
		unlock_filp(filp);
		return(r);
	}
	else exist = !(oflags & O_EXCL);/* file exists, if the O_EXCL
					   flag is set this is an error */
  } else {
	/* Scan path name */
	resolve.l_vmnt_lock = VMNT_READ;
	resolve.l_vnode_lock = VNODE_OPCL;
	if ((vp = eat_path(&resolve, fp)) == NULL) {
		unlock_filp(filp);
		return(err_code);
	}

	if (vmp != NULL) unlock_vmnt(vmp);
  }
```

**注释翻译**：
- `If O_CREATE is set, try to make the file` → 如果设置了 O_CREATE，尝试创建文件
- `We just created the file` → 我们刚创建了文件
- `other error` → 其他错误
- `file exists, if the O_EXCL flag is set this is an error` → 文件存在，如果设置了 O_EXCL 标志则是错误
- `Scan path name` → 扫描路径名

**是什么**：根据 O_CREAT 标志决定是创建新文件还是打开已有文件。

**为什么**：
- **O_CREAT 分支**：`I_REGULAR | (omode & ALLPERMS & fp->fp_umask)` 创建普通文件，应用 umask；`new_node()` 尝试创建新 inode
- **非 O_CREAT 分支**：`eat_path()` 解析完整路径，获取 vnode

---

### 第 132-138 行：分配文件描述符

```c
  /* Claim the file descriptor and filp slot and fill them in. */
  fp->fp_filp[fd] = filp;
  filp->filp_count = 1;
  filp->filp_vno = vp;
  filp->filp_flags = oflags;
  if (oflags & O_CLOEXEC)
	FD_SET(fd, &fp->fp_cloexec_set);
```

**注释翻译**：
- `Claim the file descriptor and filp slot and fill them in` → 声明文件描述符和 filp 槽位并填充

**是什么**：初始化 filp 结构。

**为什么**：
- **filp_count = 1**：初始引用计数
- **filp_vno = vp**：关联 vnode
- **filp_flags = oflags**：保存打开标志
- **O_CLOEXEC**：设置 FD_CLOEXEC 位，exec 时自动关闭

---

### 第 140-161 行：已有文件的权限检查和类型处理

```c
  /* Only do the normal open code if we didn't just create the file. */
  if (exist) {
	if ((r = forbidden(fp, vp, for_exec ? X_BIT : bits)) == OK) {
		switch (vp->v_mode & S_IFMT) {
		   case S_IFREG:
			if (oflags & O_TRUNC) {
				if ((r = forbidden(fp, vp, W_BIT)) != OK)
					break;
				upgrade_vnode_lock(vp);
				truncate_vnode(vp, 0);
			}
			break;
		   case S_IFDIR:
			r = (bits & W_BIT ? EISDIR : OK);
			break;
```

**注释翻译**：
- `Only do the normal open code if we didn't just create the file` → 只有在我们没有刚创建文件时才执行正常的打开代码
- `Truncate regular file if O_TRUNC` → 如果 O_TRUNC 则截断普通文件
- `Directories may be read but not written` → 目录可以读但不能写

**是什么**：对已有文件进行权限检查和文件类型特定处理。

**为什么**：
- **forbidden()**：检查进程是否有权限打开文件
- **S_IFREG**：O_TRUNC 时截断文件为零长度
- **S_IFDIR**：只允许读，写返回 EISDIR

---

### 第 162-169 行：字符设备打开

```c
		   case S_IFCHR:
			dev = vp->v_sdev;
			r = cdev_open(fd, dev, bits | (oflags & O_NOCTTY));
			vp = filp->filp_vno;
			break;
```

**注释翻译**：
- `Invoke the driver for special processing` → 调用驱动进行特殊处理
- `TTY needs to know about the O_NOCTTY flag` → TTY 需要知道 O_NOCTTY 标志
- `Might be updated by cdev_open after cloning` → 可能被 cdev_open 在克隆后更新

**是什么**：打开字符设备（如终端）。

**为什么**：
- **cdev_open**：通知字符设备驱动有进程打开设备
- **O_NOCTTY**：防止设备成为控制终端
- **vp 更新**：cdev_open 可能克隆设备（如 PTY），更新 vnode 指针

---

### 第 170-220 行：块设备打开

```c
		   case S_IFBLK:
			lock_bsf();
			dev = vp->v_sdev;
			r = bdev_open(dev, bits);
			if (r != OK) {
				unlock_bsf();
				break;
			}

			major_dev = major(vp->v_sdev);
			dp = &dmap[major_dev];
			if (dp->dmap_driver == NONE) {
				printf("VFS: block driver disappeared!\n");
				unlock_bsf();
				r = ENXIO;
				break;
			}

			vp->v_bfs_e = ROOT_FS_E;
			for (vmp = &vmnt[0]; vmp < &vmnt[NR_MNTS]; ++vmp)
				if (vmp->m_dev == vp->v_sdev &&
				    !(vmp->m_flags & VMNT_FORCEROOTBSF)) {
					vp->v_bfs_e = vmp->m_fs_e;
				}

			if (vp->v_bfs_e != ROOT_FS_E) {
				unlock_bsf();
				break;
			}

			if (req_newdriver(vp->v_bfs_e, vp->v_sdev,
					dp->dmap_label) != OK) {
				printf("VFS: error sending driver label\n");
				bdev_close(dev);
				r = ENXIO;
			}
			unlock_bsf();
			break;
```

**注释翻译**：
- `Check whether the device is mounted or not. If so, then that FS is responsible for this device. Otherwise we default to ROOT_FS` → 检查设备是否已挂载。如果是，则该 FS 负责此设备。否则默认为 ROOT_FS
- `Send the driver label to the file system that will handle the block I/O requests` → 将驱动标签发送给将处理块 I/O 请求的文件系统

**是什么**：打开块设备（如磁盘分区）。

**为什么**：
- **lock_bsf()**：块设备操作需要全局锁
- **bdev_open()**：通知块设备驱动
- **v_bfs_e 设置**：确定哪个 FS 进程处理此块设备的 I/O
- **req_newdriver**：通知根 FS 新的驱动信息

---

### 第 222-264 行：管道（FIFO）打开

```c
		   case S_IFIFO:
			upgrade_vnode_lock(vp);
			r = map_vnode(vp, PFS_PROC_NR);
			if (r == OK) {
				if (vp->v_ref_count == 1) {
					if (vp->v_size != 0)
						r = truncate_vnode(vp, 0);
				}
				oflags |= O_APPEND;
				filp->filp_flags = oflags;
			}
			if (r == OK) {
				r = pipe_open(fd, vp, bits, oflags);
			}
			if (r != ENXIO) {
				b = (bits & R_BIT ? R_BIT : W_BIT);
				filp->filp_count = 0;
				if ((filp2 = find_filp(vp, b)) != NULL) {
				    fp->fp_filp[fd] = filp2;
				    filp2->filp_count++;
				    filp2->filp_vno = vp;
				    filp2->filp_flags = oflags;
				    unlock_vnode(vp);
				    put_vnode(vp);
				} else {
				    filp->filp_count = 1;
				}
			}
			break;
```

**注释翻译**：
- `Create a mapped inode on PFS which handles reads and writes to this named pipe` → 在 PFS 上创建映射 inode 来处理对此命名管道的读写
- `force append mode` → 强制追加模式
- `See if someone else is doing a rd or wt on the FIFO` → 查看是否有其他人在 FIFO 上进行读或写
- `If so, use its filp entry so the file position will be automatically shared` → 如果是，使用其 filp 条目以便文件位置自动共享

**是什么**：打开命名管道（FIFO）。

**为什么**：
- **map_vnode**：在管道文件系统（PFS）上创建映射 inode
- **O_APPEND**：管道强制追加模式
- **find_filp**：查找已有的共同读者/写者，共享 filp 以共享位置
- **引用计数修正**：FS 不知道我们复用 filp，需要修正引用计数

---

### 第 266-293 行：Socket 和错误处理

```c
		   case S_IFSOCK:
			r = EOPNOTSUPP;
			break;
		   default:
			printf("VFS: attempt to open file <%llu,%llu> of "
			    "type 0%o\n", vp->v_dev, vp->v_inode_nr,
			    vp->v_mode & S_IFMT);
			r = EIO;
		}
	}
  }

  unlock_filp(filp);

  if (r != OK) {
	if (r != SUSPEND) {
		fp->fp_filp[fd] = NULL;
		filp->filp_count = 0;
		filp->filp_vno = NULL;
		put_vnode(vp);
	}
  } else {
	r = fd;
  }

  return(r);
}
```

**注释翻译**：
- `If error, release inode` → 如果出错，释放 inode

**是什么**：处理 socket（不支持）、错误清理、返回结果。

**为什么**：
- **S_IFSOCK**：socket 不能通过 open() 打开
- **错误清理**：释放 filp 和 vnode
- **SUSPEND 不释放**：进程被阻塞时，保持资源
- **成功返回 fd**：返回文件描述符

---

### 第 296-477 行：new_node 函数

```c
static struct vnode *new_node(struct lookup *resolve, int oflags, mode_t bits)
{
/* Try to create a new inode and return a pointer to it. If the inode already
   exists, return a pointer to it as well, but set err_code accordingly. */
```

**注释翻译**：
- `Try to create a new inode and return a pointer to it` → 尝试创建新 inode 并返回指针
- `If the inode already exists, return a pointer to it as well, but set err_code accordingly` → 如果 inode 已存在，也返回指针，但相应设置 err_code

**是什么**：创建新文件的核心函数，处理 O_CREAT、O_EXCL、符号链接等复杂情况。

**关键逻辑**：
1. **last_dir**：解析到最后一级目录
2. **advance**：尝试解析最后组件
3. **符号链接处理**：如果路径包含符号链接，递归重新解析
4. **req_create**：向 FS 发送创建请求
5. **悬空符号链接**：处理指向不存在文件的符号链接

---

### 第 480-508 行：pipe_open 函数

```c
static int pipe_open(int fd, struct vnode *vp, mode_t bits, int oflags)
{
  if ((bits & (R_BIT|W_BIT)) == (R_BIT|W_BIT)) return(ENXIO);

  if (find_filp(vp, bits & W_BIT ? R_BIT : W_BIT) == NULL) {
	if (oflags & O_NONBLOCK) {
		if (bits & W_BIT) return(ENXIO);
	} else {
		fp->fp_popen.fd = fd;
		suspend(FP_BLOCKED_ON_POPEN);
		return(SUSPEND);
	}
  } else if (susp_count > 0) {
	release(vp, VFS_OPEN, susp_count);
  }
  return(OK);
}
```

**注释翻译**：
- `This function is called from common_open. It checks if there is at least one reader/writer pair for the pipe` → 此函数从 common_open 调用。它检查管道是否至少有一对读者/写者
- `if not it suspends the caller, otherwise it revives all other blocked processes hanging on the pipe` → 如果没有，挂起调用者，否则唤醒所有其他阻塞在管道上的进程

**是什么**：管道打开时的阻塞/唤醒逻辑。

**为什么**：
- **ENXIO**：同时以读写打开管道无意义
- **find_filp**：查找管道另一端（读者找写者，写者找读者）
- **O_NONBLOCK**：非阻塞模式，写端无读者直接返回 ENXIO
- **suspend**：阻塞等待另一端
- **release**：唤醒所有等待的进程

---

### 第 511-559 行：do_mknod 函数

```c
int do_mknod(void)
{
  /* Only the super_user may make nodes other than fifos. */
  if (!super_user && !S_ISFIFO(mode_bits))
	return(EPERM);

  bits = (mode_bits & S_IFMT) | (mode_bits & ACCESSPERMS & fp->fp_umask);

  if ((vp = last_dir(&resolve, fp)) == NULL) return(err_code);

  if (!S_ISDIR(vp->v_mode)) {
	r = ENOTDIR;
  } else if ((r = forbidden(fp, vp, W_BIT|X_BIT)) == OK) {
	r = req_mknod(vp->v_fs_e, vp->v_inode_nr, fullpath, fp->fp_effuid,
		      fp->fp_effgid, bits, dev);
  }

  unlock_vnode(vp);
  unlock_vmnt(vmp);
  put_vnode(vp);
  return(r);
}
```

**注释翻译**：
- `Only the super_user may make nodes other than fifos` → 只有超级用户可以创建 FIFO 以外的节点

**是什么**：创建设备节点或 FIFO。

**为什么**：
- **super_user 检查**：只有 root 可以创建设备节点（FIFO 除外）
- **umask 应用**：权限位应用 umask
- **req_mknod**：向 FS 发送创建请求

---

### 第 561-598 行：do_mkdir 函数

```c
int do_mkdir(void)
{
  bits = I_DIRECTORY | (dirmode & RWX_MODES & fp->fp_umask);
  if ((vp = last_dir(&resolve, fp)) == NULL) return(err_code);

  if (!S_ISDIR(vp->v_mode)) {
	r = ENOTDIR;
  } else if ((r = forbidden(fp, vp, W_BIT|X_BIT)) == OK) {
	r = req_mkdir(vp->v_fs_e, vp->v_inode_nr, fullpath, fp->fp_effuid,
		      fp->fp_effgid, bits);
  }

  unlock_vnode(vp);
  unlock_vmnt(vmp);
  put_vnode(vp);
  return(r);
}
```

**是什么**：创建目录。

**为什么**：
- **I_DIRECTORY**：标记为目录类型
- **RWX_MODES**：只保留读/写/执行权限位
- **req_mkdir**：向 FS 发送创建目录请求

---

### 第 600-669 行：lseek 实现

```c
int actual_lseek(struct fproc *rfp, int seekfd, int seekwhence, off_t offset,
	off_t *newposp)
{
  if ( (rfilp = get_filp2(rfp, seekfd, VNODE_READ)) == NULL)
	return(err_code);

  /* No lseek on pipes. */
  if (S_ISFIFO(rfilp->filp_vno->v_mode)) {
	unlock_filp(rfilp);
	return(ESPIPE);
  }

  switch(seekwhence) {
    case SEEK_SET: pos = 0; break;
    case SEEK_CUR: pos = rfilp->filp_pos; break;
    case SEEK_END: pos = rfilp->filp_vno->v_size; break;
    default: unlock_filp(rfilp); return(EINVAL);
  }

  newpos = pos + offset;

  /* Check for overflow. */
  if ((offset > 0) && (newpos <= pos)) {
	r = EOVERFLOW;
  } else if ((offset < 0) && (newpos >= pos)) {
	r = EOVERFLOW;
  } else {
	if (newposp != NULL) *newposp = newpos;
	if (newpos != rfilp->filp_pos) {
		rfilp->filp_pos = newpos;
		r = req_inhibread(rfilp->filp_vno->v_fs_e,
				  rfilp->filp_vno->v_inode_nr);
	}
  }

  unlock_filp(rfilp);
  return(r);
}
```

**注释翻译**：
- `No lseek on pipes` → 管道不能 lseek
- `The value of 'whence' determines the start position to use` → 'whence' 的值决定使用的起始位置
- `Check for overflow` → 检查溢出
- `Inhibit read ahead request` → 禁止预读请求

**是什么**：文件定位（lseek）实现。

**为什么**：
- **SEEK_SET/CUR/END**：三种定位基准
- **溢出检查**：防止 offset 溢出
- **req_inhibread**：lseek 后禁止预读，因为位置改变了

---

### 第 671-727 行：close 实现

```c
int do_close(void)
{
  fd = job_m_in.m_lc_vfs_close.fd;
  nblock = job_m_in.m_lc_vfs_close.nblock;
  return close_fd(fp, fd, !nblock /*may_suspend*/);
}

int close_fd(struct fproc * rfp, int fd_nr, int may_suspend)
{
  if ( (rfilp = get_filp2(rfp, fd_nr, VNODE_OPCL)) == NULL) return(err_code);

  vp = rfilp->filp_vno;

  /* first, make all future get_filp2()'s fail */
  rfp->fp_filp[fd_nr] = NULL;

  r = close_filp(rfilp, may_suspend);
  FD_CLR(fd_nr, &rfp->fp_cloexec_set);

  /* Check to see if the file is locked. If so, release all locks. */
  if (nr_locks > 0) {
	lock_count = nr_locks;
	for (flp = &file_lock[0]; flp < &file_lock[NR_LOCKS]; flp++) {
		if (flp->lock_type == 0) continue;
		if (flp->lock_vnode == vp && flp->lock_pid == rfp->fp_pid) {
			flp->lock_type = 0;
			nr_locks--;
		}
	}
	if (nr_locks < lock_count)
		lock_revive();
  }

  return(r);
}
```

**注释翻译**：
- `Perform the close(fd) or closenb(fd) system call` → 执行 close(fd) 或 closenb(fd) 系统调用
- `first, make all future get_filp2()'s fail; otherwise we might try to close the same fd in different threads` → 首先，使所有未来的 get_filp2() 失败；否则我们可能尝试在不同线程中关闭同一个 fd
- `one or more locks released` → 一个或多个锁已释放

**是什么**：关闭文件描述符，清理资源。

**为什么**：
- **fp_filp[fd_nr] = NULL**：先清除 fd 映射，防止并发关闭
- **close_filp**：减少 filp 引用计数，可能释放 vnode
- **FD_CLR**：清除 CLOEXEC 位
- **锁清理**：释放该进程对此文件的所有锁
- **lock_revive**：唤醒等待锁的进程

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 VFS open | Linux VFS |
|------|-----------------|-----------|
| 架构 | 用户态，IPC 到 FS | 内核态 `do_sys_open` |
| 路径解析 | eat_path/last_dir | path_lookupat |
| 文件创建 | req_create 到 FS | vfs_create → inode_operations |
| 管道打开 | map_vnode + PFS | pipe(2) 直接创建 |
| 权限检查 | forbidden() | inode_permission() |
| 并发安全 | 手动锁管理 | VFS 层自动锁 |

### Rust 重构建议

```rust
// Minix3 C 代码：switch-case 文件类型处理
// switch (vp->v_mode & S_IFMT) {
//     case S_IFREG: ...
//     case S_IFDIR: ...
//     case S_IFCHR: ...
// }

// Rust 改进：enum + trait
enum FileType {
    Regular,
    Directory,
    CharDevice(DeviceId),
    BlockDevice(DeviceId),
    Fifo,
    Socket,
}

trait FileOpener {
    fn open(&self, flags: OpenFlags, mode: FileMode) -> IoResult<OpenHandle>;
}

// 统一打开接口
async fn open_file(file_type: &FileType, flags: OpenFlags) -> IoResult<OpenHandle> {
    match file_type {
        FileType::Regular => RegularFileOpener { fs }.open(flags, mode).await,
        FileType::CharDevice(dev) => CharDeviceOpener { dev: *dev }.open(flags, mode).await,
        FileType::Fifo => FifoOpener { pfs }.open(flags, mode).await,
        _ => Err(IoError::OperationNotSupported),
    }
}
```

---

## 总结

`open.c`（727 行）是 VFS 中最复杂的文件之一，实现了：

1. **do_open/do_creat**：文件打开入口，共享 common_open
2. **common_open**：统一打开逻辑，处理 6 种文件类型
3. **new_node**：文件创建，处理 O_CREAT/O_EXCL/符号链接
4. **pipe_open**：管道打开的阻塞/唤醒机制
5. **do_mknod/do_mkdir**：创建设备节点和目录
6. **do_lseek**：文件定位，禁止预读
7. **do_close/close_fd**：关闭文件，清理锁和资源

关键设计模式：
- **策略模式**：根据文件类型选择不同打开策略
- **管道同步**：读者/写者配对机制
- **引用计数**：filp 和 vnode 的生命周期管理
- **锁升级**：truncate 等操作需要升级 vnode 锁
