# servers/vfs/exec.c 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/exec.c`  
**模块归属**: VFS（虚拟文件系统服务器）  
**核心功能**: 实现 exec 系统调用，加载并执行新程序

---

## 逐行讲解

### 1. 文件头注释

```c
/* This file handles the EXEC system call.  It performs the work as follows:
 *    - see if the permissions allow the file to be executed
 *    - read the header and extract the sizes
 *    - fetch the initial args and environment from the user space
 *    - allocate the memory for the new process
 *    - copy the initial stack from PM to the process
 *    - read in the text and data segments and copy to the process
 *    - take care of setuid and setgid bits
 *    - fix up 'mproc' table
 *    - tell kernel about EXEC
 *    - save offset to initial argc (for ps)
 *
 * The entry points into this file are:
 *   pm_exec:	 perform the EXEC system call
 */
```

**第1-17行**: 文件头注释  
- **权限检查**: 检查文件是否可执行
- **读取头部**: 读取文件头并提取大小
- **获取参数**: 从用户空间获取初始参数和环境
- **分配内存**: 为新进程分配内存
- **拷贝栈**: 从 PM 拷贝初始栈到进程
- **加载段**: 读取代码段和数据段并拷贝到进程
- **setuid/setgid**: 处理 setuid 和 setgid 位
- **修复进程表**: 修复 mproc 表
- **通知内核**: 告诉内核关于 EXEC
- **保存 argc**: 保存初始 argc 的偏移（用于 ps）

**设计原因**: exec 是进程管理的核心操作

---

### 2. 包含头文件

```c
#include "fs.h"
#include <sys/stat.h>
#include <sys/mman.h>
#include <minix/callnr.h>
#include <minix/endpoint.h>
#include <minix/com.h>
#include <minix/u64.h>
#include <lib.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <sys/dirent.h>
#include <sys/exec.h>
#include <sys/param.h>
#include "path.h"
#include "vnode.h"
#include "file.h"
#include <minix/vfsif.h>
#include <machine/vmparam.h>
#include <assert.h>
#include <fcntl.h>

#define _KERNEL	/* for ELF_AUX_ENTRIES */
#include <libexec.h>
```

**第19-44行**: 包含头文件  
- `fs.h`: VFS 主头文件
- `sys/stat.h`: 文件状态
- `sys/mman.h`: 内存映射
- `minix/callnr.h`: 系统调用号
- `minix/endpoint.h`: 端点定义
- `minix/com.h`: 通信相关
- `minix/u64.h`: 64 位整数
- `lib.h`: 库函数
- `signal.h`: 信号
- `stdlib.h`: 标准库
- `string.h`: 字符串操作
- `sys/dirent.h`: 目录项
- `sys/exec.h`: 执行相关
- `sys/param.h`: 参数
- `path.h`: 路径处理
- `vnode.h`: vnode 定义
- `file.h`: 文件表项定义
- `minix/vfsif.h`: VFS 接口
- `machine/vmparam.h`: VM 参数
- `assert.h`: 断言宏
- `fcntl.h`: 文件控制
- `libexec.h`: 执行库

---

### 3. 结构体定义

```c
/* fields only used by elf and in VFS */
struct vfs_exec_info {
    struct exec_info args;		/* libexec exec args */
    struct vnode *vp;			/* Exec file's vnode */
    struct vmnt *vmp;			/* Exec file's vmnt */
    struct stat sb;			/* Exec file's stat structure */
    int userflags;			/* exec() flags from userland */
    int is_dyn;				/* Dynamically linked executable */
    int elf_main_fd;			/* Dyn: FD of main program execuatble */
    char execname[PATH_MAX];		/* Full executable invocation */
    int vmfd;
    int vmfd_used;
};
```

**第46-60行**: VFS 执行信息结构  
- `args`: libexec 执行参数
- `vp`: 执行文件的 vnode
- `vmp`: 执行文件的挂载点
- `sb`: 执行文件的 stat 结构
- `userflags`: 用户态 exec 标志
- `is_dyn`: 是否动态链接
- `elf_main_fd`: 主程序文件描述符
- `execname`: 完整执行路径
- `vmfd`: VM 文件描述符
- `vmfd_used`: VM 文件描述符是否使用

**设计原因**: 封装执行所需的所有信息

---

### 4. 静态函数声明

```c
static int patch_stack(struct vnode *vp, char stack[ARG_MAX],
	size_t *stk_bytes, char path[PATH_MAX], vir_bytes *vsp);
static int is_script(struct vfs_exec_info *execi);
static int insert_arg(char stack[ARG_MAX], size_t *stk_bytes, char *arg,
	vir_bytes *vsp, char replace);
static void clo_exec(struct fproc *rfp);
static int stack_prepare_elf(struct vfs_exec_info *execi,
	char *curstack, size_t *frame_len, vir_bytes *vsp);
static int map_header(struct vfs_exec_info *execi);
static int read_seg(struct exec_info *execi, off_t off, vir_bytes seg_addr, size_t seg_bytes);
```

**第62-74行**: 静态函数声明  
- `patch_stack`: 修补栈
- `is_script`: 判断是否是脚本
- `insert_arg`: 插入参数
- `clo_exec`: 处理 FD_CLOEXEC
- `stack_prepare_elf`: 准备 ELF 栈
- `map_header`: 映射文件头
- `read_seg`: 读取段

---

### 5. 执行加载器数组

```c
#define PTRSIZE	sizeof(char *) /* Size of pointers in argv[] and envp[]. */

/* Array of loaders for different object file formats */
typedef int (*exechook_t)(struct vfs_exec_info *execpackage);
typedef int (*stackhook_t)(struct vfs_exec_info *execi, char *curstack,
	size_t *frame_len, vir_bytes *vsp);
struct exec_loaders {
	libexec_exec_loadfunc_t load_object;	 /* load executable into memory */
	stackhook_t setup_stack; /* prepare stack before argc and argv push */
};

static const struct exec_loaders exec_loaders[] = {
	{ libexec_load_elf,  stack_prepare_elf },
	{ NULL, NULL }
};
```

**第76-94行**: 执行加载器数组  
- **PTRSIZE**: 指针大小
- **exechook_t**: 执行钩子函数类型
- **stackhook_t**: 栈钩子函数类型
- **exec_loaders**: 执行加载器结构
  - `load_object`: 加载可执行文件到内存
  - `setup_stack`: 准备栈
- **数组**: 目前只支持 ELF 格式

**设计原因**: 
- **可扩展**: 支持多种可执行文件格式
- **模块化**: 每种格式有独立的加载器

---

### 6. 锁宏定义

```c
#define lock_exec() lock_proc(fproc_addr(VM_PROC_NR))
#define unlock_exec() unlock_proc(fproc_addr(VM_PROC_NR))
```

**第96-97行**: 锁宏定义  
- **lock_exec**: 锁定 VM 进程
- **unlock_exec**: 解锁 VM 进程

**设计原因**: exec 需要与 VM 同步

---

### 7. get_read_vp 函数

```c
/*===========================================================================*
 *				get_read_vp				     *
 *===========================================================================*/
static int get_read_vp(struct vfs_exec_info *execi,
  char *fullpath, int copyprogname, int sugid, struct lookup *resolve, struct fproc *fp)
{
/* Make the executable that we want to exec() into the binary pointed
 * to by 'fullpath.' This function fills in necessary details in the execi
 * structure, such as opened vnode. It unlocks and releases the vnode if
 * it was already there. This makes it easy to change the executable
 * during the exec(), which is often necessary, by calling this function
 * more than once. This is specifically necessary when we discover the
 * executable is actually a script or a dynamically linked executable.
 */
	int r;

	/* Caller wants to switch vp to the file in 'fullpath.'
	 * unlock and put it first if there is any there.
	 */
	if(execi->vp) {
		unlock_vnode(execi->vp);
		put_vnode(execi->vp);
		execi->vp = NULL;
	}

	/* Remember/overwrite the executable name if requested. */
	if(copyprogname) {
		char *cp = strrchr(fullpath, '/');
		if(cp) cp++;
		else cp = fullpath;
		strlcpy(execi->args.progname, cp, sizeof(execi->args.progname));
		execi->args.progname[sizeof(execi->args.progname)-1] = '\0';
	}

	/* Open executable */
	if ((execi->vp = eat_path(resolve, fp)) == NULL)
		return err_code;

	unlock_vmnt(execi->vmp);

	if (!S_ISREG(execi->vp->v_mode))
		return ENOEXEC;
	else if ((r = forbidden(fp, execi->vp, X_BIT)) != OK)
		return r;
	else
		r = req_stat(execi->vp->v_fs_e, execi->vp->v_inode_nr,
			VFS_PROC_NR, (vir_bytes) &(execi->sb));

	if (r != OK) return r;

	/* If caller wants us to, honour suid/guid mode bits. */
        if (sugid) {
		/* Deal with setuid/setgid executables */
		if (execi->vp->v_mode & I_SET_UID_BIT) {
			execi->args.new_uid = execi->vp->v_uid;
			execi->args.allow_setuid = 1;
		}
		if (execi->vp->v_mode & I_SET_GID_BIT) {
			execi->args.new_gid = execi->vp->v_gid;
			execi->args.allow_setuid = 1;
		}
        }

	/* Read in first chunk of file. */
	if((r=map_header(execi)) != OK)
```

**第99-167行**: 获取可执行文件的 vnode  
- **参数**: 
  - `execi`: 执行信息
  - `fullpath`: 完整路径
  - `copyprogname`: 是否拷贝程序名
  - `sugid`: 是否处理 setuid/setgid
  - `resolve`: 查找结构
  - `fp`: 进程指针
- **释放旧 vnode**: 如果已有 vnode，释放它
- **拷贝程序名**: 如果需要，拷贝程序名
- **打开文件**: 调用 `eat_path` 打开文件
- **检查类型**: 检查是否是普通文件
- **权限检查**: 检查执行权限
- **获取状态**: 调用 `req_stat` 获取文件状态
- **setuid/setgid**: 如果需要，处理 setuid/setgid 位
- **读取头部**: 调用 `map_header` 读取文件头

**设计原因**: 
- **可重入**: 支持多次调用（脚本、动态链接）
- **安全**: 检查权限和 setuid/setgid

---

## 要点总结

### 1. 核心知识点

1. **exec**: 加载并执行新程序
2. **可执行文件格式**: 支持 ELF 等格式
3. **setuid/setgid**: 处理特权程序

### 2. 设计亮点

- **可扩展**: 支持多种可执行文件格式
- **可重入**: 支持脚本和动态链接
- **安全**: 检查权限和 setuid/setgid

### 3. 内存模型

```
进程内存:
┌─────────────────────────────────┐
│ 代码段                          │
│ 数据段                          │
│ 堆                              │
│ 栈                              │
│   ├─ argc                       │
│   ├─ argv[]                     │
│   └─ envp[]                     │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 权限不足

**后果**: 
- 返回 `EACCES`
- exec 失败

**症状**: 程序无法执行

### 场景 2: 文件格式错误

**后果**: 
- 返回 `ENOEXEC`
- exec 失败

**症状**: 程序无法执行

### 场景 3: setuid 滥用

**后果**: 
- 权限提升
- 安全漏洞

**症状**: 攻击者获得特权

---

## 互动自测

### 问题 1: exec 流程

**问**: exec 的主要流程是什么？

**答**: 
1. 检查权限
2. 读取文件头
3. 获取参数和环境
4. 分配内存
5. 加载代码段和数据段
6. 处理 setuid/setgid
7. 通知内核

### 问题 2: 可执行文件格式

**问**: 为什么支持多种格式？

**答**: 
- **兼容性**: 支持不同格式的程序
- **扩展性**: 可以添加新格式
- **模块化**: 每种格式独立处理

### 问题 3: setuid/setgid

**问**: 为什么需要处理 setuid/setgid？

**答**: 
- **特权**: 允许普通用户执行特权操作
- **安全**: 需要严格控制
- **功能**: 实现如 passwd 等功能

---

## Rust 实现对比

### C 版本（原始）

```c
struct vfs_exec_info {
    struct exec_info args;
    struct vnode *vp;
    struct vmnt *vmp;
    struct stat sb;
    int userflags;
    int is_dyn;
    int elf_main_fd;
    char execname[PATH_MAX];
    int vmfd;
    int vmfd_used;
};
```

### Rust 版本（安全抽象）

```rust
struct VfsExecInfo {
    args: ExecInfo,
    vp: Option<Arc<Vnode>>,
    vmp: Option<Arc<Vmnt>>,
    sb: Stat,
    userflags: i32,
    is_dyn: bool,
    elf_main_fd: Option<i32>,
    execname: [u8; PATH_MAX],
    vmfd: Option<i32>,
    vmfd_used: bool,
}
```

### 关键改进

1. **Option**: 使用 `Option` 表示可能为空
2. **Arc**: 使用 `Arc` 实现共享所有权
3. **bool**: 使用 `bool` 代替 `int`

---

## 理论关联

### 1. exec 系统调用

**操作系统概念**: exec 加载新程序替换当前进程

**Minix3 实现**:
- VFS 负责加载可执行文件
- PM 负责进程管理
- VM 负责内存分配

### 2. 可执行文件格式

**操作系统概念**: 可执行文件格式定义程序结构

**Minix3 实现**:
- 支持 ELF 格式
- 使用加载器数组支持多种格式
- 模块化设计

### 3. setuid/setgid

**操作系统概念**: setuid/setgid 允许程序获得特权

**Minix3 实现**:
- 检查文件权限位
- 修改进程的有效 UID/GID
- 严格控制

---

## 总结

`exec.c` 实现了 Minix3 VFS 的 exec 系统调用。通过可扩展的加载器、可重入设计、安全检查等，实现了安全、灵活的程序加载。理解 exec 的流程和 setuid/setgid 处理是理解进程管理的关键。
