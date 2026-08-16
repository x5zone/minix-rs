# 06-vmnt-struct: VMnt 挂载点结构

> 本文档分析 `minix3/minix/servers/vfs/vmnt.h` 中的 vmnt 结构体。

---

## 1. 概述

### 1.1 VMnt 的角色

VMnt 代表一个挂载的文件系统实例——每个 mount() 调用创建一个 vmnt 条目
vmnt 是全局静态数组 `vmnt[NR_MNTS]`——NR_MNTS=16，系统最多同时挂载 16 个文件系统
与 fork 的间接关系——子进程继承的 vnode 属于某个 vmnt，通过 v_vmnt 指针关联

---

## 2. VMnt 结构体字段

### 2.1 文件系统标识字段

#### 2.1.1 m_fs_e

`int m_fs_e`——FS 进程的内核端点号，VFS 通过此端点与底层 FS 通信

#### 2.1.2 m_label

`char m_label[LABEL_MAX]`——FS 进程标签，用于标识文件系统服务
LABEL_MAX = 16，标签最长 15 个字符

#### 2.1.3 m_fstype

`char m_fstype[FSTYPE_MAX]`——文件系统类型，如 "mfs", "ext2", "procfs" 等

### 2.2 设备与挂载字段

#### 2.2.1 m_dev

`dev_t m_dev`——设备号，标识挂载的块设备

#### 2.2.2 m_mounted_on

`struct vnode *m_mounted_on`——挂载点 vnode，指向根文件系统中被挂载的目录
挂载点 vnode 是根文件系统中的目录——路径遍历遇到此 vnode 时跳转到 m_root_node

#### 2.2.3 m_root_node

`struct vnode *m_root_node`——挂载文件系统的根 vnode，路径遍历跳转的目标

#### 2.2.4 m_mount_path

`char m_mount_path[PATH_MAX]`——挂载路径，如 "/mnt/usb"

#### 2.2.5 m_mount_dev

`char m_mount_dev[PATH_MAX]`——设备路径，如 "/dev/c0d0p0s0"

### 2.3 标志字段

#### 2.3.1 m_flags

`unsigned int m_flags`——挂载标志，位图编码挂载状态
VMNT_READONLY (只读挂载) / VMNT_CALLBACK (回调挂起) / VMNT_MOUNTING (正在挂载) / VMNT_FORCEROOTBSF (强制根 BSF) / VMNT_CANSTAT (可 statvfs)

#### 2.3.2 m_fs_flags

`unsigned int m_fs_flags`——FS 能力标志，记录底层 FS 支持的功能（如符号链接、硬链接等）

### 2.4 锁与通信字段

#### 2.4.1 m_lock

`tll_t m_lock`——三级锁（TLL），保护 vmnt 的并发访问
VMNT_READ (TLL_READ) / VMNT_WRITE (TLL_READSER) / VMNT_EXCL (TLL_WRITE)——三级锁映射

#### 2.4.2 m_comm

`comm_t m_comm`——通信结构，管理 VFS 与底层 FS 之间的请求队列
请求队列和并发控制——详见 [18-comm](18-comm.md)

### 2.5 缓存统计字段

#### 2.5.1 m_stats

`struct statvfs_cache m_stats`——缓存的 statvfs 数据，避免频繁向底层 FS 请求统计信息
statvfs_cache 的各字段含义——缓存块大小、总块数、空闲块数、总 inode 数、空闲 inode 数等

---

## 3. VMnt 锁类型

### 3.1 锁类型常量

VMNT_READ (TLL_READ) / VMNT_WRITE (TLL_READSER) / VMNT_EXCL (TLL_WRITE)——读/写/排他三级锁
挂载点锁的使用场景——读操作（statvfs）获取 VMNT_READ，写操作（mount/umount）获取 VMNT_WRITE，排他操作（fsync）获取 VMNT_EXCL

---

## 4. VMnt 与 fork

### 4.1 间接关系

fork 不直接操作 vmnt——fork 只复制 fproc，不涉及 vmnt 结构
子进程通过 fp_rd/fp_wd 间接引用 vmnt（v_vmnt 字段）——vnode.v_vmnt 指向 vmnt，子进程继承 vnode 指针后自然关联到同一个 vmnt

### 4.2 vmnt_unmap_by_endpt()

进程退出时如何清理 vmnt 中与该进程相关的条目——pm_exit() 关闭所有文件描述符，释放 filp 和 vnode 引用，但不直接操作 vmnt
与 pm_exit 的关系——详见 [19-pm-exit](19-pm-exit.md)

---

## 5. C 源码

**文件**: `minix3/minix/servers/vfs/vmnt.h`

```c
EXTERN struct vmnt {
  int m_fs_e;
  tll_t m_lock;
  comm_t m_comm;
  dev_t m_dev;
  unsigned int m_flags;
  unsigned int m_fs_flags;
  struct vnode *m_mounted_on;
  struct vnode *m_root_node;
  char m_label[LABEL_MAX];
  char m_mount_path[PATH_MAX];
  char m_mount_dev[PATH_MAX];
  char m_fstype[FSTYPE_MAX];
  struct statvfs_cache m_stats;
} vmnt[NR_MNTS];
```
