# 09-globals-const: 全局变量与常量

> 本文档分析 `minix3/minix/servers/vfs/glo.h`, `const.h`, `type.h`, `fs.h`, `proto.h` 中的全局定义。

---

## 1. 概述

### 1.1 VFS 头文件组织

VFS 头文件的层次关系：fs.h 包含所有子头文件（fproc.h, file.h, vnode.h, vmnt.h, glo.h, const.h 等）
EXTERN 宏的作用——在 table.c 中定义为空（实际定义），其他文件中定义为 extern（声明）

---

## 2. 全局变量 (glo.h)

### 2.1 当前进程指针

#### 2.1.1 fp

`EXTERN struct fproc *fp`——当前调用者的 fproc 指针，工作线程通过 fp 访问当前进程的文件系统状态
工作线程模型下 fp 的含义——当前线程服务的进程，每个工作线程有自己的 fp 副本

#### 2.1.2 self

`EXTERN struct worker_thread *self`——当前工作线程指针，线程通过 self 访问自己的状态
self 在多线程环境下的线程局部性——self 是线程局部变量，每个线程有自己的 self 值

### 2.2 消息变量

#### 2.2.1 m_in

`EXTERN message m_in`——当前输入消息，存储从用户进程收到的系统调用请求
工作线程模型下每个线程有自己的 m_in 副本——m_in 是线程局部变量

#### 2.2.2 消息相关宏

- `who_p` 宏 — 当前进程的 fproc 索引：`#define who_p ((int)(fp - fproc))`
- `who_e` 宏 — 当前进程的 endpoint：`#define who_e (self ? fp->fp_endpoint : m_in.m_source)`
- `call_nr` 宏 — 当前系统调用号：`#define call_nr (m_in.m_type)`
- `fproc_addr(e)` 宏 — 从 endpoint 定位 fproc：`#define fproc_addr(e) (&fproc[_ENDPOINT_P(e)])`

### 2.3 系统状态变量

#### 2.3.1 susp_count

- `EXTERN int susp_count`——挂起在管道上的进程数，用于跟踪系统负载

#### 2.3.2 其他状态

- nr_locks (文件锁数量), reviving (待恢复进程标志), sending (正在发送消息), verbose (调试输出级别), err_code (最近错误码)

### 2.4 根文件系统变量

#### 2.4.1 ROOT_DEV / ROOT_FS_E

- 根设备号 (ROOT_DEV) 和根 FS 端点 (ROOT_FS_E)——VFS 启动时从 PM 获取，用于挂载根文件系统

### 2.5 工作线程变量

#### 2.5.1 workers[]

- `EXTERN struct worker_thread workers[NR_WTHREADS]`——工作线程数组，NR_WTHREADS=9

### 2.6 锁变量

#### 2.6.1 bsf_lock

- `EXTERN mutex_t bsf_lock`——块特殊文件全局锁，保护块设备的并发访问

---

## 3. 常量定义 (const.h)

### 3.1 表大小常量

- NR_FILPS=1024 (最大打开文件数), NR_LOCKS=8 (POSIX 文件锁数), NR_MNTS=16 (最大挂载数), NR_VNODES=1024 (最大 vnode 数)
- NR_WTHREADS=9 (工作线程数), NR_SOCKDEVS=8 (socket 设备数)

### 3.2 UID/GID 常量

- SU_UID=0 (超级用户 UID), SYS_UID=0 (系统用户 UID), SYS_GID=0 (系统组 GID)

### 3.3 阻塞常量

- FP_BLOCKED_ON_* 常量（详见 [02-fproc-flags](02-fproc-flags.md)）

### 3.4 select 常量

- SEL_RD (读就绪), SEL_WR (写就绪), SEL_ERR (错误), SEL_NOTIFY (通知)——select 操作的位标志
- 与 CDEV/SDEV 操作常量的关系——SEL_RD 对应 CDEV 读，SEL_WR 对应 CDEV 写，SEL_ERR 对应异常

### 3.5 其他常量


---

## 4. 类型定义 (type.h)

### 4.1 comm_t — 通信结构

  - c_max_reqs — FS 可同时处理的最大请求数
  - c_cur_reqs — 当前正在处理的请求数
  - c_req_queue — 等待发送消息的请求队列

### 4.2 statvfs_cache


### 4.3 smap — socket 映射


### 4.4 sockid_t


---

## 5. 主头文件 (fs.h)

### 5.1 包含关系


---

## 6. 函数原型 (proto.h)

### 6.1 fork 相关原型


### 6.2 vnode 相关原型


---

## 7. 与 fork 的关系

### 7.1 pm_fork 使用的全局变量

| 全局变量 | 用途 |
|---------|------|
| `fproc[]` | 访问父子进程的 fproc 结构 |
| `fp` | 间接使用（通过 fproc_addr 宏） |

### 7.2 pm_fork 使用的宏

| 宏 | 用途 |
|----|------|
| `fproc_addr(e)` | 从 endpoint 定位 fproc |
| `_ENDPOINT_P(e)` | 从 endpoint 提取 slot 号 |
| `okendpt(e, &nr)` | 验证 endpoint 有效性 |
