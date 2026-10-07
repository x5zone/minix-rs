# servers/vfs/table.c 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/table.c`  
**模块归属**: VFS（虚拟文件系统服务器）  
**核心功能**: 定义系统调用分发表，将系统调用号映射到处理函数

---

## 逐行讲解

### 1. 文件头注释

```c
/* This file contains the table used to map system call numbers onto the
 * routines that perform them.
 */
```

**第1-3行**: 文件头注释  
- **map system call numbers**: 映射系统调用号
- **onto the routines**: 到处理函数

**设计原因**: 系统调用分发机制

---

### 2. 宏定义

```c
#define _TABLE
```

**第5行**: 宏定义  
- `_TABLE`: 标记此文件定义表
- 可能用于条件编译

**设计原因**: 区分表定义和表使用

---

### 3. 包含头文件

```c
#include "fs.h"
#include <minix/callnr.h>
#include <minix/com.h>
#include "file.h"
#include "lock.h"
#include "vnode.h"
#include "vmnt.h"
```

**第7-14行**: 包含头文件  
- `fs.h`: VFS 主头文件
- `minix/callnr.h`: 系统调用号定义
- `minix/com.h`: 通信相关定义
- `file.h`: 文件表项定义
- `lock.h`: 锁相关定义
- `vnode.h`: vnode 定义
- `vmnt.h`: 挂载点定义

**设计原因**: 引入系统调用号和函数声明

---

### 4. CALL 宏定义

```c
#define CALL(n) [((n) - VFS_BASE)]
```

**第16行**: CALL 宏定义  
- **参数**: `n` 系统调用号
- **展开**: `[(n - VFS_BASE)]`
- **用途**: 数组索引初始化

**设计原因**: 
- **C99 指定初始化**: 使用 `[index] = value` 语法
- **偏移**: 系统调用号从 `VFS_BASE` 开始

---

### 5. 系统调用分发表

```c
int (* const call_vec[NR_VFS_CALLS])(void) = {
	CALL(VFS_READ)		= do_read,		/* read(2) */
	CALL(VFS_WRITE)		= do_write,		/* write(2) */
	CALL(VFS_LSEEK)		= do_lseek,		/* lseek(2) */
	CALL(VFS_OPEN)		= do_open,		/* open(2) */
	CALL(VFS_CREAT)		= do_creat,		/* creat(2) */
	CALL(VFS_CLOSE)		= do_close,		/* close(2) */
	CALL(VFS_LINK)		= do_link,		/* link(2) */
	CALL(VFS_UNLINK)	= do_unlink,		/* unlink(2) */
	CALL(VFS_CHDIR)		= do_chdir,		/* chdir(2) */
	CALL(VFS_MKDIR)		= do_mkdir,		/* mkdir(2) */
	CALL(VFS_MKNOD)		= do_mknod,		/* mknod(2) */
	CALL(VFS_CHMOD)		= do_chmod,		/* chmod(2) */
	CALL(VFS_CHOWN)		= do_chown,		/* chown(2) */
	CALL(VFS_MOUNT)		= do_mount,		/* mount(2) */
	CALL(VFS_UMOUNT)	= do_umount,		/* umount(2) */
	CALL(VFS_ACCESS)	= do_access,		/* access(2) */
	CALL(VFS_SYNC)		= do_sync,		/* sync(2) */
	CALL(VFS_RENAME)	= do_rename,		/* rename(2) */
	CALL(VFS_RMDIR)		= do_unlink,		/* rmdir(2) */
	CALL(VFS_SYMLINK)	= do_slink,		/* symlink(2) */
	CALL(VFS_READLINK)	= do_rdlink,		/* readlink(2) */
	CALL(VFS_STAT)		= do_stat,		/* stat(2) */
	CALL(VFS_FSTAT)		= do_fstat,		/* fstat(2) */
	CALL(VFS_LSTAT)		= do_lstat,		/* lstat(2) */
	CALL(VFS_IOCTL)		= do_ioctl,		/* ioctl(2) */
	CALL(VFS_FCNTL)		= do_fcntl,		/* fcntl(2) */
	CALL(VFS_PIPE2)		= do_pipe2,		/* pipe2(2) */
	CALL(VFS_UMASK)		= do_umask,		/* umask(2) */
	CALL(VFS_CHROOT)	= do_chroot,		/* chroot(2) */
	CALL(VFS_GETDENTS)	= do_getdents,		/* getdents(2) */
	CALL(VFS_SELECT)	= do_select,		/* select(2) */
	CALL(VFS_FCHDIR)	= do_fchdir,		/* fchdir(2) */
	CALL(VFS_FSYNC)		= do_fsync,		/* fsync(2) */
	CALL(VFS_TRUNCATE)	= do_truncate,		/* truncate(2) */
	CALL(VFS_FTRUNCATE)	= do_ftruncate,		/* ftruncate(2) */
	CALL(VFS_FCHMOD)	= do_chmod,		/* fchmod(2) */
	CALL(VFS_FCHOWN)	= do_chown,		/* fchown(2) */
	CALL(VFS_UTIMENS)	= do_utimens,		/* [fl]utime[n]s(2) */
	CALL(VFS_VMCALL)	= do_vm_call,
	CALL(VFS_GETVFSSTAT)	= do_getvfsstat,	/* getvfsstat(2) */
	CALL(VFS_STATVFS1)	= do_statvfs,		/* statvfs(2) */
	CALL(VFS_FSTATVFS1)	= do_fstatvfs,		/* fstatvfs(2) */
	CALL(VFS_GETRUSAGE)	= do_getrusage,		/* (obsolete) */
	CALL(VFS_SVRCTL)	= do_svrctl,		/* svrctl(2) */
	CALL(VFS_GCOV_FLUSH)	= do_gcov_flush,	/* gcov_flush(2) */
	CALL(VFS_MAPDRIVER)	= do_mapdriver,		/* mapdriver(2) */
	CALL(VFS_COPYFD)	= do_copyfd,		/* copyfd(2) */
	CALL(VFS_SOCKETPATH)	= do_socketpath,	/* socketpath(2) */
	CALL(VFS_GETSYSINFO)	= do_getsysinfo,	/* getsysinfo(2) */
	CALL(VFS_SOCKET)	= do_socket,		/* socket(2) */
	CALL(VFS_SOCKETPAIR)	= do_socketpair,	/* socketpair(2) */
	CALL(VFS_BIND)		= do_bind,		/* bind(2) */
	CALL(VFS_CONNECT)	= do_connect,		/* connect(2) */
	CALL(VFS_LISTEN)	= do_listen,		/* listen(2) */
	CALL(VFS_ACCEPT)	= do_accept,		/* accept(2) */
	CALL(VFS_SENDTO)	= do_sendto,		/* sendto(2) */
	CALL(VFS_SENDMSG)	= do_sockmsg,		/* sendmsg(2) */
	CALL(VFS_RECVFROM)	= do_recvfrom,		/* recvfrom(2) */
	CALL(VFS_RECVMSG)	= do_sockmsg,		/* recvmsg(2) */
	CALL(VFS_SETSOCKOPT)	= do_setsockopt,	/* setsockopt(2) */
	CALL(VFS_GETSOCKOPT)	= do_getsockopt,	/* getsockopt(2) */
	CALL(VFS_GETSOCKNAME)	= do_getsockname,	/* getsockname(2) */
	CALL(VFS_GETPEERNAME)	= do_getpeername,	/* getpeername(2) */
	CALL(VFS_SHUTDOWN)	= do_shutdown,		/* shutdown(2) */
};
```

**第18-82行**: 系统调用分发表  
- **类型**: `int (* const call_vec[NR_VFS_CALLS])(void)`
  - `int (*)(void)`: 函数指针，返回 int，无参数
  - `const`: 常量数组，不可修改
  - `NR_VFS_CALLS`: 数组大小
- **初始化**: 使用 C99 指定初始化语法

**逐个解释**:

**文件操作**:
- `VFS_READ` → `do_read`: 读文件
- `VFS_WRITE` → `do_write`: 写文件
- `VFS_LSEEK` → `do_lseek`: 移动文件指针
- `VFS_OPEN` → `do_open`: 打开文件
- `VFS_CREAT` → `do_creat`: 创建文件
- `VFS_CLOSE` → `do_close`: 关闭文件

**目录操作**:
- `VFS_MKDIR` → `do_mkdir`: 创建目录
- `VFS_RMDIR` → `do_unlink`: 删除目录（复用 unlink）
- `VFS_CHDIR` → `do_chdir`: 改变当前目录
- `VFS_FCHDIR` → `do_fchdir`: 通过文件描述符改变目录

**文件属性**:
- `VFS_CHMOD` → `do_chmod`: 修改权限
- `VFS_CHOWN` → `do_chown`: 修改所有者
- `VFS_FCHMOD` → `do_chmod`: 通过文件描述符修改权限（复用）
- `VFS_FCHOWN` → `do_chown`: 通过文件描述符修改所有者（复用）

**文件系统操作**:
- `VFS_MOUNT` → `do_mount`: 挂载文件系统
- `VFS_UMOUNT` → `do_umount`: 卸载文件系统
- `VFS_SYNC` → `do_sync`: 同步文件系统

**链接操作**:
- `VFS_LINK` → `do_link`: 创建硬链接
- `VFS_UNLINK` → `do_unlink`: 删除文件
- `VFS_SYMLINK` → `do_slink`: 创建符号链接
- `VFS_READLINK` → `do_rdlink`: 读取符号链接
- `VFS_RENAME` → `do_rename`: 重命名文件

**状态查询**:
- `VFS_STAT` → `do_stat`: 获取文件状态
- `VFS_FSTAT` → `do_fstat`: 通过文件描述符获取状态
- `VFS_LSTAT` → `do_lstat`: 获取符号链接状态

**其他操作**:
- `VFS_IOCTL` → `do_ioctl`: 设备控制
- `VFS_FCNTL` → `do_fcntl`: 文件控制
- `VFS_PIPE2` → `do_pipe2`: 创建管道
- `VFS_UMASK` → `do_umask`: 设置文件创建掩码
- `VFS_CHROOT` → `do_chroot`: 改变根目录
- `VFS_GETDENTS` → `do_getdents`: 读取目录项
- `VFS_SELECT` → `do_select`: 多路复用

**套接字操作**:
- `VFS_SOCKET` → `do_socket`: 创建套接字
- `VFS_SOCKETPAIR` → `do_socketpair`: 创建套接字对
- `VFS_BIND` → `do_bind`: 绑定地址
- `VFS_CONNECT` → `do_connect`: 连接
- `VFS_LISTEN` → `do_listen`: 监听
- `VFS_ACCEPT` → `do_accept`: 接受连接
- `VFS_SENDTO` → `do_sendto`: 发送数据
- `VFS_RECVFROM` → `do_recvfrom`: 接收数据
- `VFS_SENDMSG` → `do_sockmsg`: 发送消息
- `VFS_RECVMSG` → `do_sockmsg`: 接收消息
- `VFS_SETSOCKOPT` → `do_setsockopt`: 设置套接字选项
- `VFS_GETSOCKOPT` → `do_getsockopt`: 获取套接字选项
- `VFS_GETSOCKNAME` → `do_getsockname`: 获取套接字地址
- `VFS_GETPEERNAME` → `do_getpeername`: 获取对端地址
- `VFS_SHUTDOWN` → `do_shutdown`: 关闭套接字

**设计原因**: 
- **统一分发**: 通过数组索引快速分发
- **复用**: 相似操作复用同一函数（如 `chmod` 和 `fchmod`）

---

## 要点总结

### 1. 核心知识点

1. **系统调用分发表**: 数组索引快速分发
2. **C99 指定初始化**: 使用 `[index] = value` 语法
3. **函数复用**: 相似操作复用同一函数

### 2. 设计亮点

- **快速分发**: O(1) 时间复杂度
- **可读性**: 注释标明对应的系统调用
- **复用**: 减少代码重复

### 3. 内存模型

```
静态数据段:
┌─────────────────────────────────┐
│ call_vec[0] = NULL              │
│ call_vec[1] = NULL              │
│ ...                             │
│ call_vec[VFS_READ - VFS_BASE]   │
│   = do_read                     │
│ call_vec[VFS_WRITE - VFS_BASE]  │
│   = do_write                    │
│ ...                             │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 系统调用号越界

**后果**: 
- 访问数组越界
- 段错误

**症状**: VFS 崩溃

### 场景 2: 函数指针为 NULL

**后果**: 
- 调用空指针
- 段错误

**症状**: 系统调用失败

### 场景 3: 函数签名不匹配

**后果**: 
- 调用约定错误
- 栈损坏

**症状**: 随机崩溃

---

## 互动自测

### 问题 1: C99 指定初始化

**问**: 为什么使用 `CALL(n) = value` 语法？

**答**: 
- **可读性**: 明确索引和值的对应关系
- **灵活性**: 可以跳过某些索引
- **维护性**: 添加新系统调用时不需要调整顺序

### 问题 2: 函数复用

**问**: 为什么 `VFS_RMDIR` 和 `VFS_UNLINK` 使用同一函数？

**答**: 
- **相似性**: 删除目录和删除文件逻辑相似
- **减少代码**: 避免重复实现
- **参数区分**: 函数内部根据参数区分操作

### 问题 3: const 修饰

**问**: 为什么 `call_vec` 使用 `const`？

**答**: 
- **安全性**: 防止运行时修改
- **优化**: 编译器可以优化访问
- **意图**: 明确表是只读的

---

## Rust 实现对比

### C 版本（原始）

```c
int (* const call_vec[NR_VFS_CALLS])(void) = {
	CALL(VFS_READ)		= do_read,
	CALL(VFS_WRITE)		= do_write,
	// ...
};
```

### Rust 版本（安全抽象）

```rust
type SyscallFn = fn() -> i32;

const CALL_VEC: [Option<SyscallFn>; NR_VFS_CALLS] = {
    let mut table = [None; NR_VFS_CALLS];
    table[VFS_READ - VFS_BASE] = Some(do_read);
    table[VFS_WRITE - VFS_BASE] = Some(do_write);
    // ...
    table
};
```

### 关键改进

1. **Option**: 使用 `Option` 表示可能为空
2. **类型别名**: 使用 `type` 定义函数类型
3. **编译时检查**: Rust 编译器检查数组大小

---

## 理论关联

### 1. 系统调用分发

**操作系统概念**: 系统调用是用户态请求内核服务的接口

**Minix3 实现**:
- VFS 是用户态服务器
- 通过 IPC 接收系统调用请求
- 使用分发表快速找到处理函数

### 2. 函数指针数组

**C 语言模式**: 使用函数指针数组实现分发

**Minix3 实现**:
- `call_vec` 是函数指针数组
- 系统调用号作为索引
- O(1) 时间复杂度

### 3. 微内核架构

**操作系统概念**: 服务在用户态运行

**Minix3 实现**:
- VFS 是用户态进程
- 系统调用通过 IPC 转发到 VFS
- VFS 处理后返回结果

---

## 总结

`table.c` 定义了 Minix3 VFS 的系统调用分发表。通过函数指针数组和 C99 指定初始化，实现了高效、可读的系统调用分发。理解分发表是理解 VFS 系统调用处理的关键。
