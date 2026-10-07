# servers/vfs/request.c 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/request.c`  
**模块归属**: VFS（虚拟文件系统服务器）  
**核心功能**: 实现向文件系统进程发送请求和接收响应的包装函数

---

## 逐行讲解

### 1. 文件头注释

```c
/* This file contains the wrapper functions for issuing a request
 * and receiving response from FS processes.
 * Each function builds a request message according to the request
 * parameter, calls the most low-level fs_sendrec, and copies
 * back the response.
 */
```

**第1-7行**: 文件头注释  
- **wrapper functions**: 包装函数
- **issuing a request**: 发送请求
- **receiving response**: 接收响应
- **FS processes**: 文件系统进程

**设计原因**: 封装与文件系统进程的通信

---

### 2. 包含头文件

```c
#include "fs.h"
#include <minix/com.h>
#include <minix/const.h>
#include <minix/endpoint.h>
#include <minix/u64.h>
#include <minix/vfsif.h>
#include <sys/dirent.h>
#include <sys/stat.h>
#include <sys/statvfs.h>
#include <assert.h>
#include <stddef.h>
#include <string.h>
#include <unistd.h>
#include <time.h>
#include "path.h"
#include "vmnt.h"
#include "vnode.h"
```

**第9-28行**: 包含头文件  
- `fs.h`: VFS 主头文件
- `minix/com.h`: 通信相关
- `minix/const.h`: 常量定义
- `minix/endpoint.h`: 端点定义
- `minix/u64.h`: 64 位整数
- `minix/vfsif.h`: VFS 接口
- `sys/dirent.h`: 目录项
- `sys/stat.h`: 文件状态
- `sys/statvfs.h`: 文件系统状态
- 其他标准库和 VFS 内部头文件

---

### 3. req_breadwrite_actual 函数

```c
/*===========================================================================*
 *			req_breadwrite_actual				     *
 *===========================================================================*/
static int req_breadwrite_actual(endpoint_t fs_e, endpoint_t user_e, dev_t dev, off_t pos,
        unsigned int num_of_bytes, vir_bytes user_addr, int rw_flag,
        off_t *new_pos, size_t *cum_iop, int cpflag)
{
  int r;
  cp_grant_id_t grant_id;
  message m;

  grant_id = cpf_grant_magic(fs_e, user_e, user_addr, num_of_bytes,
			(rw_flag == READING ? CPF_WRITE : CPF_READ) | cpflag);
  if(grant_id == -1)
	  panic("req_breadwrite: cpf_grant_magic failed");

  /* Fill in request message */
  m.m_type = rw_flag == READING ? REQ_BREAD : REQ_BWRITE;
  m.m_vfs_fs_breadwrite.device = dev;
  m.m_vfs_fs_breadwrite.grant = grant_id;
  m.m_vfs_fs_breadwrite.seek_pos = pos;
  m.m_vfs_fs_breadwrite.nbytes = num_of_bytes;

  /* Send/rec request */
  r = fs_sendrec(fs_e, &m);

  if (cpf_revoke(grant_id) == GRANT_FAULTED) return(ERESTART);

  if (r != OK) return(r);

  /* Fill in response structure */
  *new_pos = m.m_fs_vfs_breadwrite.seek_pos;
  *cum_iop = m.m_fs_vfs_breadwrite.nbytes;

  return(OK);
}
```

**第30-64行**: 实际的块设备读写请求  
- **参数**: 
  - `fs_e`: 文件系统端点
  - `user_e`: 用户端点
  - `dev`: 设备号
  - `pos`: 位置
  - `num_of_bytes`: 字节数
  - `user_addr`: 用户地址
  - `rw_flag`: 读写标志
  - `new_pos`: 新位置（输出）
  - `cum_iop`: 累计 I/O（输出）
  - `cpflag`: 授权标志
- **创建授权**: 调用 `cpf_grant_magic` 创建授权
- **填充消息**: 设置消息类型和参数
- **发送请求**: 调用 `fs_sendrec` 发送请求
- **撤销授权**: 调用 `cpf_revoke` 撤销授权
- **提取响应**: 从响应消息中提取新位置和字节数

**设计原因**: 
- **授权机制**: 使用授权安全传递缓冲区
- **统一接口**: 统一处理读写请求

---

### 4. req_breadwrite 函数

```c
int req_breadwrite(endpoint_t fs_e, endpoint_t user_e, dev_t dev, off_t pos,
        unsigned int num_of_bytes, vir_bytes user_addr, int rw_flag,
        off_t *new_pos, size_t *cum_iop)
{
	int r;

	r = req_breadwrite_actual(fs_e, user_e, dev, pos, num_of_bytes,
		user_addr, rw_flag, new_pos, cum_iop, CPF_TRY);

	if (r == ERESTART) {
		if((r=vm_vfs_procctl_handlemem(user_e, user_addr, num_of_bytes,
			rw_flag == READING)) != OK) {
			return r;
		}

		r = req_breadwrite_actual(fs_e, user_e, dev, pos, num_of_bytes,
			user_addr, rw_flag, new_pos, cum_iop, 0);
	}

	return r;
}
```

**第66-84行**: 块设备读写请求  
- **参数**: 同 `req_breadwrite_actual`（除了 `cpflag`）
- **第一次尝试**: 调用 `req_breadwrite_actual`，使用 `CPF_TRY` 标志
- **处理 ERESTART**: 如果返回 `ERESTART`，调用 VM 处理内存，然后重试
- **重试**: 重试时不使用 `CPF_TRY` 标志

**设计原因**: 
- **页面错误处理**: 处理用户内存的页面错误
- **重试机制**: 自动重试失败的请求

---

### 5. req_bpeek 函数

```c
/*===========================================================================*
 *			req_bpeek					       *
 *===========================================================================*/
int req_bpeek(endpoint_t fs_e, dev_t dev, off_t pos, unsigned int num_of_bytes)
{
  message m;

  memset(&m, 0, sizeof(m));

  /* Fill in request message */
  m.m_type = REQ_BPEEK;
  m.m_vfs_fs_breadwrite.device = dev;
  m.m_vfs_fs_breadwrite.seek_pos = pos;
  m.m_vfs_fs_breadwrite.nbytes = num_of_bytes;

  /* Send/rec request */
  return fs_sendrec(fs_e, &m);
}
```

**第86-102行**: 块设备预读请求  
- **参数**: 
  - `fs_e`: 文件系统端点
  - `dev`: 设备号
  - `pos`: 位置
  - `num_of_bytes`: 字节数
- **填充消息**: 设置消息类型和参数
- **发送请求**: 调用 `fs_sendrec` 发送请求

**设计原因**: 预读优化

---

### 6. req_chmod 函数

```c
/*===========================================================================*
 *				req_chmod	      			     *
 *===========================================================================*/
int req_chmod(
  endpoint_t fs_e,
  ino_t inode_nr,
  mode_t rmode,
  mode_t *new_modep
)
{
  message m;
  int r;

  /* Fill in request message */
  m.m_type = REQ_CHMOD;
  m.m_vfs_fs_chmod.inode = inode_nr;
  m.m_vfs_fs_chmod.mode = rmode;

  /* Send/rec request */
  r = fs_sendrec(fs_e, &m);

  /* Copy back actual mode. */
  *new_modep = m.m_fs_vfs_chmod.mode;

  return(r);
}
```

**第104-128行**: 修改文件权限请求  
- **参数**: 
  - `fs_e`: 文件系统端点
  - `inode_nr`: inode 号
  - `rmode`: 新权限
  - `new_modep`: 实际权限（输出）
- **填充消息**: 设置消息类型和参数
- **发送请求**: 调用 `fs_sendrec` 发送请求
- **提取响应**: 从响应消息中提取实际权限

**设计原因**: 封装权限修改请求

---

### 7. req_chown 函数

```c
/*===========================================================================*
 *				req_chown          			     *
 *===========================================================================*/
int req_chown(
  endpoint_t fs_e,
  ino_t inode_nr,
  uid_t newuid,
  gid_t newgid,
  mode_t *new_modep
)
{
  message m;
  int r;

  /* Fill in request message */
  m.m_type = REQ_CHOWN;
  m.m_vfs_fs_chown.inode = inode_nr;
  m.m_vfs_fs_chown.uid = newuid;
```

**第130-150行**: 修改文件所有者请求  
- **参数**: 
  - `fs_e`: 文件系统端点
  - `inode_nr`: inode 号
  - `newuid`: 新用户 ID
  - `newgid`: 新组 ID
  - `new_modep`: 实际权限（输出）
- **填充消息**: 设置消息类型和参数

**设计原因**: 封装所有者修改请求

---

## 要点总结

### 1. 核心知识点

1. **请求/响应模式**: 发送请求，等待响应
2. **授权机制**: 使用授权安全传递缓冲区
3. **重试机制**: 自动重试失败的请求

### 2. 设计亮点

- **封装**: 封装与文件系统进程的通信
- **安全**: 使用授权机制安全传递缓冲区
- **容错**: 自动重试失败的请求

### 3. 内存模型

```
VFS 进程:
┌─────────────────────────────────┐
│ 构造请求消息                     │
│ 创建授权                         │
│ 发送请求                         │
│ 等待响应                         │
│ 撤销授权                         │
│ 提取响应                         │
└─────────────────────────────────┘
         ↓ IPC
文件系统进程:
┌─────────────────────────────────┐
│ 接收请求                         │
│ 处理请求                         │
│ 发送响应                         │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 授权失败

**后果**: 
- `cpf_grant_magic` 返回 -1
- panic

**症状**: VFS 崩溃

### 场景 2: 文件系统崩溃

**后果**: 
- `fs_sendrec` 返回错误
- 请求失败

**症状**: 文件操作失败

### 场景 3: 页面错误

**后果**: 
- 返回 `ERESTART`
- 调用 VM 处理内存

**症状**: 自动重试

---

## 互动自测

### 问题 1: 授权机制

**问**: 为什么需要授权机制？

**答**: 
- **安全**: 防止文件系统访问未授权的内存
- **隔离**: VFS 和文件系统隔离
- **灵活**: 可以传递任意缓冲区

### 问题 2: 重试机制

**问**: 为什么需要重试机制？

**答**: 
- **页面错误**: 用户内存可能未映射
- **自动处理**: 自动处理页面错误
- **透明**: 对调用者透明

### 问题 3: 请求/响应

**问**: 为什么使用请求/响应模式？

**答**: 
- **同步**: 同步通信简单
- **可靠**: 确保请求被处理
- **简单**: 易于理解和实现

---

## Rust 实现对比

### C 版本（原始）

```c
int req_chmod(
  endpoint_t fs_e,
  ino_t inode_nr,
  mode_t rmode,
  mode_t *new_modep
)
{
  message m;
  int r;

  m.m_type = REQ_CHMOD;
  m.m_vfs_fs_chmod.inode = inode_nr;
  m.m_vfs_fs_chmod.mode = rmode;

  r = fs_sendrec(fs_e, &m);

  *new_modep = m.m_fs_vfs_chmod.mode;

  return(r);
}
```

### Rust 版本（安全抽象）

```rust
fn req_chmod(fs_e: endpoint_t, inode_nr: ino_t, rmode: mode_t) -> Result<mode_t, i32> {
    let mut m = Message::new();
    m.m_type = REQ_CHMOD;
    m.m_vfs_fs_chmod.inode = inode_nr;
    m.m_vfs_fs_chmod.mode = rmode;

    fs_sendrec(fs_e, &mut m)?;

    Ok(m.m_fs_vfs_chmod.mode)
}
```

### 关键改进

1. **Result**: 使用 `Result` 返回错误
2. **? 运算符**: 自动传播错误
3. **Message**: 使用结构体代替联合体

---

## 理论关联

### 1. 客户端-服务器模型

**操作系统概念**: VFS 是客户端，文件系统是服务器

**Minix3 实现**:
- VFS 发送请求
- 文件系统处理请求
- 文件系统返回响应

### 2. IPC

**操作系统概念**: 进程间通信

**Minix3 实现**:
- 使用消息传递
- 同步 IPC
- 授权机制

### 3. 授权机制

**操作系统概念**: 安全传递内存

**Minix3 实现**:
- 使用 grant table
- 文件系统通过授权访问用户内存
- 避免直接指针传递

---

## 总结

`request.c` 实现了 Minix3 VFS 与文件系统进程的通信包装函数。通过请求/响应模式、授权机制、重试机制等设计，实现了安全、可靠的文件系统通信。理解这些包装函数是理解 VFS 与文件系统交互的关键。
