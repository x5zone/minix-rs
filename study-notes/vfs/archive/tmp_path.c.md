# path.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vfs/path.c`
> 
> **行数**: 933 行
> 
> **核心内容**: 路径名解析，处理挂载点、符号链接、跨文件系统查找

---

## 文件概述

`path.c` 是 VFS 中**最核心的路径解析文件**，实现了：
1. **advance**：解析路径组件到 vnode
2. **eat_path**：完整路径解析入口
3. **last_dir**：解析到最后一级目录
4. **lookup**：相对路径解析的核心逻辑，处理挂载点和符号链接
5. **get_name**：根据 inode 查找目录条目名称
6. **canonical_path**：获取规范路径
7. **do_socketpath**：socket 文件路径操作

---

## 逐行讲解

### 第 1-4 行：文件头注释

```c
/* lookup() is the main routine that controls the path name lookup. It
 * handles mountpoints and symbolic links. The actual lookup requests
 * are sent through the req_lookup wrapper function.
 */
```

**注释翻译**：
- `lookup() is the main routine that controls the path name lookup` → lookup() 是控制路径名查找的主要例程
- `It handles mountpoints and symbolic links` → 它处理挂载点和符号链接
- `The actual lookup requests are sent through the req_lookup wrapper function` → 实际的查找请求通过 req_lookup 包装函数发送

**设计思路**：路径解析是 VFS 最复杂的功能之一，需要处理挂载点穿越、符号链接解析、跨文件系统操作。

---

### 第 6-21 行：头文件包含

```c
#include "fs.h"
#include <string.h>
#include <minix/callnr.h>
#include <minix/com.h>
#include <minix/const.h>
#include <minix/endpoint.h>
#include <stddef.h>
#include <unistd.h>
#include <assert.h>
#include <minix/vfsif.h>
#include <sys/param.h>
#include <sys/stat.h>
#include <sys/dirent.h>
#include "vmnt.h"
#include "vnode.h"
#include "path.h"
```

**关键头文件**：
- `sys/dirent.h`：目录条目结构
- `sys/param.h`：系统参数（如 `_POSIX_SYMLOOP_MAX`）
- `path.h`：`struct lookup` 定义
- `minix/vfsif.h`：VFS 接口定义（req_lookup 等）

---

### 第 23-31 行：POSIX 路径解析开关

```c
/* Set to following define to 1 if you really want to use the POSIX definition
 * (IEEE Std 1003.1, 2004) of pathname resolution. POSIX requires pathnames
 * with a traling slash (and that do not entirely consist of slash characters)
 * to be treated as if a single dot is appended. This means that for example
 * mkdir("dir/", ...) and rmdir("dir/") will fail because the call tries to
 * create or remove the directory '.'. Historically, Unix systems just ignore
 * trailing slashes.
 */
#define DO_POSIX_PATHNAME_RES	0
```

**注释翻译**：
- `Set to following define to 1 if you really want to use the POSIX definition of pathname resolution` → 如果你真的想使用 POSIX 路径名解析定义，将以下定义设为 1
- `POSIX requires pathnames with a traling slash to be treated as if a single dot is appended` → POSIX 要求以斜杠结尾的路径名被视为附加了一个点
- `This means that for example mkdir("dir/", ...) and rmdir("dir/") will fail` → 这意味着例如 mkdir("dir/", ...) 和 rmdir("dir/") 将失败
- `Historically, Unix systems just ignore trailing slashes` → 历史上，Unix 系统只是忽略尾随斜杠

**设计思路**：
作者选择**不遵循 POSIX 标准**，而是使用传统的 Unix 行为（忽略尾随斜杠）。这是一个务实的设计决策——大多数 Unix 程序期望尾随斜杠被忽略。

---

### 第 33-34 行：lookup 函数原型

```c
static int lookup(struct vnode *dirp, struct lookup *resolve,
	node_details_t *node, struct fproc *rfp);
```

**是什么**：声明内部路径查找函数。

---

### 第 36-127 行：advance 函数

```c
/*===========================================================================*
 *				advance					     *
 *===========================================================================*/
struct vnode *
advance(struct vnode *dirp, struct lookup *resolve, struct fproc *rfp)
{
/* Resolve a path name starting at dirp to a vnode. */
  int r;
  int do_downgrade = 1;
  struct vnode *new_vp, *vp;
  struct vmnt *vmp;
  struct node_details res = {0,0,0,0,0,0,0};
  tll_access_t initial_locktype;

  assert(dirp);
  assert(resolve->l_vnode_lock != TLL_NONE);
  assert(resolve->l_vmnt_lock != TLL_NONE);

  if (resolve->l_vnode_lock == VNODE_READ)
	initial_locktype = VNODE_OPCL;
  else
	initial_locktype = resolve->l_vnode_lock;

  /* Get a free vnode and lock it */
  if ((new_vp = get_free_vnode()) == NULL) return(NULL);
  lock_vnode(new_vp, initial_locktype);

  /* Lookup vnode belonging to the file. */
  if ((r = lookup(dirp, resolve, &res, rfp)) != OK) {
	err_code = r;
	unlock_vnode(new_vp);
	return(NULL);
  }

  /* Check whether we already have a vnode for that file */
  if ((vp = find_vnode(res.fs_e, res.inode_nr)) != NULL) {
	unlock_vnode(new_vp);	/* Don't need this anymore */
	do_downgrade = (lock_vnode(vp, initial_locktype) != EBUSY);

	if (vp->v_ref_count == 0) { /* vnode vanished! */
		vp->v_fs_count = 1;
		if (vp->v_mapfs_e != NONE) vp->v_mapfs_count = 1;
	} else {
		vp->v_fs_count++;
	}

  } else {
	/* Vnode not found, fill in the free vnode's fields */
	new_vp->v_fs_e = res.fs_e;
	new_vp->v_inode_nr = res.inode_nr;
	new_vp->v_mode = res.fmode;
	new_vp->v_size = res.fsize;
	new_vp->v_uid = res.uid;
	new_vp->v_gid = res.gid;
	new_vp->v_sdev = res.dev;

	if( (vmp = find_vmnt(new_vp->v_fs_e)) == NULL)
		  panic("advance: vmnt not found");

	new_vp->v_vmnt = vmp;
	new_vp->v_dev = vmp->m_dev;
	new_vp->v_fs_count = 1;

	vp = new_vp;
  }

  dup_vnode(vp);
  if (do_downgrade) {
	*(resolve->l_vnode) = vp;
	if (initial_locktype != resolve->l_vnode_lock)
		tll_downgrade(&vp->v_lock);
  }

  return(vp);
}
```

**注释翻译**：
- `Resolve a path name starting at dirp to a vnode` → 将从 dirp 开始的路径名解析为 vnode
- `Get a free vnode and lock it` → 获取空闲 vnode 并锁定
- `Lookup vnode belonging to the file` → 查找属于文件的 vnode
- `Check whether we already have a vnode for that file` → 检查我们是否已有该文件的 vnode
- `Don't need this anymore` → 不再需要这个了
- `vnode vanished!` → vnode 消失了！
- `Vnode not found, fill in the free vnode's fields` → 未找到 vnode，填充空闲 vnode 的字段

**是什么**：解析路径组件到 vnode 的核心函数。

**为什么**：
- **initial_locktype**：读锁请求升级为 OPCL 锁，因为打开/关闭操作需要串行化
- **get_free_vnode + lock**：预分配并锁定 vnode，防止并发冲突
- **find_vnode 检查**：如果已有 vnode，复用而不是创建新的
- **v_ref_count == 0 处理**：vnode 在查找期间被释放的竞态条件处理
- **dup_vnode**：增加引用计数
- **tll_downgrade**：将锁从 OPCL 降级为请求的锁类型

**设计思路**：
advance 是路径解析的"原子操作"——解析路径中的一个组件。它处理了复杂的并发情况：两个线程同时查找同一个文件时，第二个线程找到第一个线程创建的 vnode。

---

### 第 129-140 行：eat_path 函数

```c
/*===========================================================================*
 *				eat_path				     *
 *===========================================================================*/
struct vnode *
eat_path(struct lookup *resolve, struct fproc *rfp)
{
/* Resolve path to a vnode. advance does the actual work. */
  struct vnode *start_dir;

  start_dir = (resolve->l_path[0] == '/' ? rfp->fp_rd : rfp->fp_wd);
  return advance(start_dir, resolve, rfp);
}
```

**注释翻译**：
- `Resolve path to a vnode. advance does the actual work` → 将路径解析为 vnode。advance 做实际工作

**是什么**：完整路径解析的入口函数。

**为什么**：
- **绝对路径**（`/` 开头）：从根目录 `fp_rd` 开始
- **相对路径**：从工作目录 `fp_wd` 开始
- **调用 advance**：实际解析工作由 advance 完成

---

### 第 142-378 行：last_dir 函数

```c
/*===========================================================================*
 *				last_dir				     *
 *===========================================================================*/
struct vnode *
last_dir(struct lookup *resolve, struct fproc *rfp)
{
/* Parse a path, as far as the last directory, fetch the vnode
 * for the last directory into the vnode table, and return a pointer to the
 * vnode. In addition, return the final component of the path in 'string'. */
```

**注释翻译**：
- `Parse a path, as far as the last directory` → 解析路径，直到最后一级目录
- `fetch the vnode for the last directory into the vnode table` → 获取最后一级目录的 vnode 到 vnode 表
- `return the final component of the path in 'string'` → 在 'string' 中返回路径的最后组件

**是什么**：解析路径到最后一级目录，返回最后组件名。用于创建文件（open with O_CREAT、mkdir、mknod）。

**关键逻辑**：

**第 170 行**：检查是否需要返回符号链接本身（不解析）
```c
ret_on_symlink = !!(resolve->l_flags & PATH_RET_SYMLINK);
```

**第 172-188 行**：确定起始目录，处理空路径
```c
start_dir = (resolve->l_path[0] == '/' ? rfp->fp_rd : rfp->fp_wd);
if (len == 0) { err_code = ENOENT; break; }
```

**第 190-196 行**：去除尾随斜杠（非 POSIX 模式）
```c
while (len > 1 && resolve->l_path[len-1] == '/') {
    len--;
    resolve->l_path[len]= '\0';
}
```

**第 198-223 行**：分离最后组件名
```c
cp = strrchr(resolve->l_path, '/');
if (cp == NULL) {
    // 当前目录中的条目， prepend "./"
} else if (cp[1] == '\0') {
    // 路径以斜杠结尾，目录条目是 '.'
} else {
    // 目录路径 + 目录条目
}
```

**第 231-239 行**：解析到最后一级目录
```c
resolve->l_flags &= ~PATH_RET_SYMLINK;
if ((res_vp = advance(start_dir, resolve, rfp)) == NULL) break;
```

**第 256-347 行**：处理符号链接和挂载点穿越
```c
lookup_init(&symlink, resolve->l_path, PATH_RET_SYMLINK, ...);
sym_vp = advance(res_vp, &symlink, rfp);

if (S_ISLNK(sym_vp->v_mode)) {
    // 符号链接：读取链接内容，重新解析
    r = req_rdlink(...);
    if (resolve->l_path[0] != '/') {
        loop_start = res_vp;  // 相对符号链接
    } else {
        // 绝对符号链接，从根重新开始
    }
    continue;
}
```

**第 349 行**：符号链接循环限制
```c
} while (symloop < _POSIX_SYMLOOP_MAX);
```

**为什么**：
- **PATH_RET_SYMLINK**：某些操作（如 mknod、lstat）需要返回符号链接本身而不是解析后的目标
- **符号链接循环**：防止无限循环（A→B→A），限制为 `_POSIX_SYMLOOP_MAX`（通常 8）
- **相对符号链接**：相对于符号链接所在目录解析
- **绝对符号链接**：从根目录重新解析
- **挂载点穿越**：如果最后组件跨越挂载点，返回挂载点的根节点

---

### 第 380-569 行：lookup 函数

```c
/*===========================================================================*
 *				lookup					     *
 *===========================================================================*/
static int
lookup(struct vnode *start_node, struct lookup *resolve, node_details_t *result_node, struct fproc *rfp)
{
/* Resolve a path name relative to start_node. */
```

**注释翻译**：
- `Resolve a path name relative to start_node` → 相对于 start_node 解析路径名

**是什么**：路径解析的核心逻辑，处理挂载点穿越和符号链接。

**关键逻辑**：

**第 415-419 行**：确定起始 FS 和挂载点
```c
fs_e = start_node->v_fs_e;
dir_ino = start_node->v_inode_nr;
vmpres = find_vmnt(fs_e);
if (vmpres == NULL) return(EIO);
```

**第 423-426 行**：设置 chroot inode
```c
if (rfp->fp_rd->v_dev == start_node->v_dev)
    root_ino = rfp->fp_rd->v_inode_nr;
else
    root_ino = 0;
```

**第 429-430 行**：设置用户/组 ID
```c
uid = (job_call_nr == VFS_ACCESS ? rfp->fp_realuid : rfp->fp_effuid);
gid = (job_call_nr == VFS_ACCESS ? rfp->fp_realgid : rfp->fp_effgid);
```

**第 449 行**：发送查找请求到 FS
```c
r = req_lookup(fs_e, dir_ino, root_ino, uid, gid, resolve, &res, rfp);
```

**第 451-552 行**：处理特殊返回码（挂载点和符号链接）
```c
while (r == EENTERMOUNT || r == ELEAVEMOUNT || r == ESYMLINK) {
    // 更新路径偏移
    path_off = res.char_processed;
    memmove(resolve->l_path, &resolve->l_path[path_off], ...);

    if (r == ESYMLINK) {
        // 符号链接：从根目录重新解析
        dir_vp = rfp->fp_rd;
    } else if (r == EENTERMOUNT) {
        // 进入新挂载点：查找挂载点的根节点
        for (vmp = &vmnt[0]; vmp != &vmnt[NR_MNTS]; ++vmp) {
            if (vmp->m_mounted_on->v_inode_nr == res.inode_nr &&
                vmp->m_mounted_on->v_fs_e == res.fs_e) {
                dir_vp = vmp->m_root_node;
                break;
            }
        }
    } else {
        // 离开挂载点：找到父挂载点
        dir_vp = vmp->m_mounted_on;
    }

    // 重新发送查找请求
    r = req_lookup(fs_e, dir_ino, root_ino, uid, gid, resolve, &res, rfp);
}
```

**为什么**：
- **EENTERMOUNT**：FS 报告路径进入挂载点，VFS 需要切换到挂载的 FS
- **ELEAVEMOUNT**：FS 报告路径离开挂载点（遇到 `..`），VFS 需要切换到父 FS
- **ESYMLINK**：FS 报告遇到符号链接，VFS 需要解析符号链接
- **循环处理**：这三种情况可能需要多次迭代（嵌套挂载点、链式符号链接）

**设计思路**：
lookup 是 VFS 中最复杂的函数之一。它实现了 **VFS-FS 协作路径解析**：FS 负责实际的路径组件查找，但当遇到挂载点或符号链接时，返回特殊错误码让 VFS 处理。这种设计将复杂性分散到 VFS 和 FS 之间。

---

### 第 571-588 行：lookup_init 函数

```c
void
lookup_init(struct lookup *resolve, char *path, int flags, struct vmnt **vmp, struct vnode **vp)
{
  resolve->l_path = path;
  resolve->l_flags = flags;
  resolve->l_vmp = vmp;
  resolve->l_vnode = vp;
  resolve->l_vmnt_lock = TLL_NONE;
  resolve->l_vnode_lock = TLL_NONE;
  *vmp = NULL;
  *vp = NULL;
}
```

**是什么**：初始化路径解析结构。

**为什么**：
- 设置路径、标志、结果指针
- 锁类型初始化为 NONE，由调用者设置
- 结果初始化为 NULL

---

### 第 590-642 行：get_name 函数

```c
int
get_name(struct vnode *dirp, struct vnode *entry, char ename[NAME_MAX + 1])
{
#define DIR_ENTRIES 8
#define DIR_ENTRY_SIZE (sizeof(struct dirent) + NAME_MAX)
  off_t pos, new_pos;
  int r, consumed, totalbytes, name_len;
  char buf[DIR_ENTRY_SIZE * DIR_ENTRIES];
  struct dirent *cur;

  pos = 0;

  if (!S_ISDIR(dirp->v_mode)) return(EBADF);

  do {
	r = req_getdents(dirp->v_fs_e, dirp->v_inode_nr, pos, (vir_bytes)buf,
		sizeof(buf), &new_pos, 1);

	if (r == 0) return(ENOENT);
	else if (r < 0) return(r);

	consumed = 0;
	totalbytes = r;

	do {
		cur = (struct dirent *) (buf + consumed);
		name_len = cur->d_reclen - offsetof(struct dirent, d_name) - 1;

		if (entry->v_inode_nr == cur->d_fileno) {
			int copylen = MIN(name_len + 1, NAME_MAX + 1);
			strlcpy(ename, cur->d_name, copylen);
			ename[NAME_MAX] = '\0';
			return(OK);
		}

		consumed += cur->d_reclen;
	} while (consumed < totalbytes);

	pos = new_pos;
  } while (1);
}
```

**是什么**：根据 inode 号在目录中查找对应的文件名。

**为什么**：
- **批量读取目录条目**：一次读取 8 个条目，减少 IPC 调用
- **inode 匹配**：比较 `d_fileno` 与目标 inode 号
- **用于 canonical_path**：从叶子节点向上构建完整路径时需要获取每个组件的名称

---

### 第 644-798 行：canonical_path 函数

```c
int
canonical_path(char orig_path[PATH_MAX], struct fproc *rfp)
{
/* Find canonical path of a given path */
```

**注释翻译**：
- `Find canonical path of a given path` → 查找给定路径的规范路径

**是什么**：将路径转换为规范形式（解析所有符号链接，构建从根到文件的完整路径）。

**关键逻辑**：

**第 665-694 行**：解析到最后一级目录，处理符号链接
```c
do {
    dir_vp = last_dir(&resolve, rfp);
    strlcpy(orig_path, temp_path, NAME_MAX+1);
    r = rdlink_direct(orig_path, temp_path, rfp);
    if (r <= 0) break;
    strlcpy(orig_path, temp_path, PATH_MAX);
    symloop++;
} while (symloop < _POSIX_SYMLOOP_MAX);
```

**第 707-782 行**：从叶子向上构建规范路径
```c
while (dir_vp != rfp->fp_rd) {
    // 检查是否是文件系统根节点
    if (dir_vp->v_vmnt->m_root_node == dir_vp) {
        if (dir_vp->v_vmnt->m_mounted_on == NULL) break;
        // 穿越挂载点到父文件系统
        dir_vp = dir_vp->v_vmnt->m_mounted_on;
    }

    // 获取父目录
    parent_dir = advance(dir_vp, &resolve, rfp);

    // 获取当前目录在父目录中的名称
    get_name(parent_dir, dir_vp, component);

    // 将组件名添加到路径前面
    memmove(orig_path+strlen(component)+1, orig_path, strlen(orig_path)+1);
    memmove(orig_path, component, strlen(component));
    orig_path[strlen(component)] = '/';

    dir_vp = parent_dir;
}
```

**第 788-796 行**：添加前导斜杠
```c
memmove(orig_path+1, orig_path, len + 1);
orig_path[0] = '/';
```

**为什么**：
- **两阶段处理**：先解析符号链接，再向上构建路径
- **挂载点穿越**：遇到文件系统根节点时，切换到父挂载点
- **get_name**：反向查找（inode → 名称）需要扫描目录条目

---

### 第 800-933 行：do_socketpath 函数

```c
int do_socketpath(void)
{
/* Perform a path action on an on-disk socket file. This call may be performed
 * by the UDS service only. */
```

**注释翻译**：
- `Perform a path action on an on-disk socket file` → 在磁盘 socket 文件上执行路径操作
- `This call may be performed by the UDS service only` → 此调用只能由 UDS 服务执行

**是什么**：UDS（Unix Domain Socket）服务使用的路径操作，用于检查和创建 socket 文件。

**关键逻辑**：

**第 824 行**：权限检查（TODO: 应使用 ACL）
```c
if (!super_user) return EPERM;
```

**第 853-873 行**：SPATH_CHECK - 检查 socket 文件
```c
case SPATH_CHECK:
    vp = eat_path(&resolve, rfp);
    if (!S_ISSOCK(vp->v_mode)) r = ENOTSOCK;
    else r = forbidden(rfp, vp, R_BIT | W_BIT);
    if (r == OK) {
        job_m_out.m_vfs_lsys_socketpath.device = vp->v_dev;
        job_m_out.m_vfs_lsys_socketpath.inode = vp->v_inode_nr;
    }
```

**第 875-926 行**：SPATH_CREATE - 创建 socket 文件
```c
case SPATH_CREATE:
    bits = S_IFSOCK | (ACCESSPERMS & rfp->fp_umask);
    dirp = last_dir(&resolve, rfp);
    r = req_mknod(dirp->v_fs_e, dirp->v_inode_nr, path,
        rfp->fp_effuid, rfp->fp_effgid, bits, NO_DEV);
    if (r == OK) {
        // 查找刚创建的 socket 文件
        vp = advance(dirp, &resolve2, rfp);
        job_m_out.m_vfs_lsys_socketpath.device = vp->v_dev;
        job_m_out.m_vfs_lsys_socketpath.inode = vp->v_inode_nr;
    }
```

**为什么**：
- **UDS 服务专用**：只有 Unix Domain Socket 服务需要此功能
- **模拟 mknod**：创建 socket 文件类似于创建设备节点
- **返回 inode 信息**：UDS 服务需要 socket 文件的设备号和 inode 号

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 VFS path.c | Linux VFS |
|------|-------------------|-----------|
| 路径解析 | VFS-FS 协作（req_lookup） | 内核直接解析（nameidata） |
| 挂载点穿越 | EENTERMOUNT/ELEAVEMOUNT 错误码 | 自动跟随 mount 链 |
| 符号链接 | ESYMLINK 错误码 + 重新解析 | 自动跟随（限制 40 次） |
| 锁管理 | 手动 TLL 锁管理 | RCU + 细粒度锁 |
| 规范路径 | 手动向上遍历（get_name） | d_path 直接构建 |
| 并发安全 | 手动锁升级/降级 | lockref + RCU |

### Rust 重构建议

```rust
// Minix3 C 代码：手动路径解析循环
// while (r == EENTERMOUNT || r == ELEAVEMOUNT || r == ESYMLINK) {
//     if (r == EENTERMOUNT) { ... }
//     else if (r == ELEAVEMOUNT) { ... }
//     else if (r == ESYMLINK) { ... }
//     r = req_lookup(...);
// }

// Rust 改进：enum + 模式匹配
enum LookupResult {
    Found(NodeDetails),
    EnterMount { mount_point: InodeId, remaining: String },
    LeaveMount { parent_fs: FsEndpoint, remaining: String },
    Symlink { target: String, remaining: String },
    Error(IoError),
}

struct PathResolver {
    root: Arc<VNode>,
    cwd: Arc<VNode>,
    symloop_count: u32,
}

impl PathResolver {
    async fn resolve(&mut self, path: &str) -> Result<ResolvedPath> {
        let mut current = if path.starts_with('/') {
            self.root.clone()
        } else {
            self.cwd.clone()
        };

        for component in path.split('/') {
            match component {
                "" | "." => continue,
                ".." => current = self.parent(&current)?,
                name => {
                    match self.lookup(&current, name).await? {
                        LookupResult::Found(node) => current = self.get_or_create_vnode(&node),
                        LookupResult::EnterMount { mount_point, .. } => {
                            current = self.cross_mount_point(&current)?;
                        }
                        LookupResult::Symlink { target, .. } => {
                            self.symloop_count += 1;
                            if self.symloop_count > POSIX_SYMLOOP_MAX {
                                return Err(IoError::TooManySymlinks);
                            }
                            // 重新解析符号链接目标
                            return self.resolve(&target).await;
                        }
                        LookupResult::Error(e) => return Err(e),
                    }
                }
            }
        }

        Ok(ResolvedPath::new(current))
    }
}
```

---

## 总结

`path.c`（933 行）是 VFS 中最复杂的路径解析文件：

1. **advance**：解析单个路径组件到 vnode，处理 vnode 缓存和并发
2. **eat_path**：完整路径解析入口，决定从根目录还是工作目录开始
3. **last_dir**：解析到最后一级目录，处理符号链接和挂载点
4. **lookup**：核心路径查找，VFS-FS 协作处理挂载点和符号链接
5. **get_name**：反向查找（inode → 名称），用于规范路径构建
6. **canonical_path**：构建规范路径，解析所有符号链接
7. **do_socketpath**：UDS 服务专用的 socket 文件路径操作

关键设计模式：
- **协作路径解析**：FS 负责组件查找，VFS 处理挂载点和符号链接
- **特殊错误码**：EENTERMOUNT/ELEAVEMOUNT/ESYMLINK 驱动跨 FS 操作
- **符号链接循环检测**：限制为 `_POSIX_SYMLOOP_MAX`
- **手动锁管理**：TLL 锁的升级/降级/释放
