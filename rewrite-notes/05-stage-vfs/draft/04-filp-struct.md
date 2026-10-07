# 04-filp-struct: Filp 文件表条目

> 本文档分析 `minix3/minix/servers/vfs/file.h` 中的 filp 结构体及其在 fork 中的角色。

---

## 1. 概述

### 1.1 Filp 的角色

Filp 是文件描述符 (fp_filp[]) 和 vnode 之间的中间层——fproc.fp_filp[fd] 指向 filp，filp.filp_vno 指向 vnode
Filp 实现了 fork 后父子进程共享文件偏移量的核心语义——fork 后父子进程的 fp_filp[fd] 指向同一个 filp，共享 filp_pos
filp 是全局静态数组 `filp[NR_FILPS]`——NR_FILPS=1024，所有进程共享

### 1.2 设计原则

引用计数模型：`filp_count == 0` 表示 slot 空闲，非零表示被引用
共享语义：多个 fproc 可以指向同一个 filp（fork 后），filp_count 记录引用数
独立偏移：filp_pos 是共享的，父子进程看到同一个文件偏移——一方 seek 影响另一方

---

## 2. Filp 结构体字段

### 2.1 filp_mode

`mode_t filp_mode`——文件打开模式 (RW 位)，R_BIT=4, W_BIT=2
FILP_CLOSED (0) 表示关联设备已关闭/消失，filp_mode 为 0 时该 filp 不可用
fork 时不修改——filp_mode 描述的是文件的打开模式，fork 不改变打开模式

### 2.2 filp_flags

`int filp_flags`——open/fcntl 标志，如 O_APPEND, O_NONBLOCK, O_CLOEXEC 等
fork 时不修改——filp_mode 描述的是文件的打开模式，fork 不改变打开模式

### 2.3 filp_count

`int filp_count`——**核心字段**，引用计数，记录有多少个文件描述符指向此 filp
fork 时 filp_count++ 的意义——子进程的 fp_filp[fd] 指向同一个 filp，引用计数必须递增以防止过早释放
close 时 filp_count--，到 0 时释放 vnode——close_filp() 递减 filp_count，若降到 0 则调用 put_vnode(filp_vno) 释放 vnode 引用

### 2.4 filp_vno

`struct vnode *filp_vno`——指向关联的 vnode，filp 通过此指针访问文件的元数据
fork 后两个 filp 指针指向同一个 vnode——v_ref_count 通过 dup_vnode() 递增
filp_count 降到 0 时调用 put_vnode(filp_vno)——释放 vnode 引用，递减 v_ref_count

### 2.5 filp_pos

`off_t filp_pos`——文件偏移量，记录当前读写位置
**关键语义**: fork 后父子进程共享 filp_pos，一方 seek 影响另一方——这是 POSIX 规定的行为
这是 POSIX 规定的行为——POSIX.1-2008 Section 2.5.1: File Sharing: fork 后父子进程共享文件描述符，包括文件偏移量

### 2.6 filp_lock

`mutex_t filp_lock`——filp 互斥锁，保护 filp 结构体的并发访问
VFS 多线程环境下保护 filp 的并发访问——worker thread 模型下，filp_lock 确保 filp_pos 和 filp_count 的原子更新

### 2.7 filp_softlock

`struct fproc *filp_softlock`——软锁指针，指向持有该 filp 软锁的进程
当 filp 未直接锁住 vnode 时的软锁机制——软锁用于防止在 close 过程中其他线程操作该 filp

### 2.8 filp_ioctl_fp

`struct fproc *filp_ioctl_fp`——ioctl 锁定进程，指向正在执行 ioctl 的进程
进行中的 IOCTL 调用如何锁定 filp——ioctl 开始时设置 filp_ioctl_fp，结束时清除。其他操作检查此字段避免冲突

---

## 3. select 相关字段

### 3.1 filp_selectors

`int filp_selectors`——select 等待进程数，记录有多少进程在 select 此文件
fork 后子进程不继承 select 状态——select 是进程主动注册的，子进程需要自己调用 select

### 3.2 filp_select_ops

`int filp_select_ops`——感兴趣的 SEL_* 操作位图，如 SEL_RD/SEL_WR/SEL_ERR

### 3.3 filp_select_flags

`int filp_select_flags`——select 标志，管理 select 请求的状态
FSF_UPDATE/FSF_BUSY/FSF_RD_BLOCK/FSF_WR_BLOCK/FSF_ERR_BLOCK 标志——控制 select 请求的处理流程

### 3.4 filp_pipe_select_ops

管道专用 select 字段 filp_pipe_select_ops——记录管道上感兴趣的 select 操作

### 3.5 filp_select_dev

字符/socket 设备 select 字段 filp_select_dev——记录 select 等待的设备号

---

## 4. Filp 生命周期

### 4.1 创建 (open)

open 系统调用分配 filp slot——遍历 filp[NR_FILPS] 寻找 filp_count==0 的空闲槽位
filp_count 初始为 1——open 成功后，该 filp 被一个文件描述符引用

### 4.2 共享 (fork)

fork 时 filp_count++ 的过程——详见 [13-pm-fork-filp](13-pm-fork-filp.md)
共享后的语义——父子进程共享 filp_pos，一方 seek/读写影响另一方的文件偏移

### 4.3 复制 (dup/dup2)

dup 系统调用增加 filp_count——dup/dup2 在同一进程内创建新的文件描述符指向同一个 filp，filp_count++
与 fork 共享的区别——dup 在同一进程内，fork 是跨进程共享

### 4.4 释放 (close)

close 时 filp_count-- 的过程——详见 [17-filedes](17-filedes.md)
filp_count == 0 时的清理操作——调用 put_vnode(filp_vno) 释放 vnode 引用，filp_vno 置 NULL，filp 槽位变为空闲

---

## 5. filp 全局表

### 5.1 表大小

`NR_FILPS = 1024`（来自 const.h）——系统最多同时打开 1024 个文件
这是系统级限制，所有进程共享——与 fproc 不同，filp 表没有按进程划分

### 5.2 空闲检测

`filp_count == 0` 是空闲判断标准——与 fproc 使用 fp_pid==PID_FREE 不同
与 fproc 的空闲判断不同（fproc 使用 fp_pid == PID_FREE）——filp 没有专用的空闲标记字段，直接用引用计数判断

---

## 6. C 源码

**文件**: `minix3/minix/servers/vfs/file.h`

```c
EXTERN struct filp {
  mode_t filp_mode;		/* RW bits, telling how file is opened */
  int filp_flags;		/* flags from open and fcntl */
  int filp_count;		/* how many file descriptors share this slot?*/
  struct vnode *filp_vno;	/* vnode belonging to this file */
  off_t filp_pos;		/* file position */
  mutex_t filp_lock;		/* lock to gain exclusive access */
  struct fproc *filp_softlock;
  struct fproc *filp_ioctl_fp;
  int filp_selectors;
  int filp_select_ops;
  int filp_select_flags;
  int filp_pipe_select_ops;
  dev_t filp_select_dev;
} filp[NR_FILPS];
```
