# 05-vnode-struct: VNode 虚拟节点

> 本文档分析 `minix3/minix/servers/vfs/vnode.h` 和 `vnode.c` 中的 vnode 结构体及引用计数机制。

---

## 1. 概述

### 1.1 VNode 的角色

VNode 是 VFS 对文件系统 inode 的统一抽象——不同文件系统（MFS, ext2 等）的 inode 被统一映射为 vnode
VNode 使得 VFS 可以透明地操作不同文件系统（MFS, ext2, 等）——VFS 只操作 vnode，底层细节由 FS 驱动
vnode 是全局静态数组 `vnode[NR_VNODES]`——NR_VNODES=1024，所有进程共享

### 1.2 设计原则

引用计数模型：`v_ref_count == 0` 表示 slot 空闲，非零表示被引用
双重引用计数：v_ref_count (VFS 层) vs v_fs_count (底层 FS 层)——VFS 和底层 FS 独立管理各自的引用计数
与 filp 的关系：filp 通过 filp_vno 指向 vnode，filp_count 管理 filp 引用，v_ref_count 管理 vnode 引用

---

## 2. VNode 结构体字段

### 2.1 文件系统标识字段

#### 2.1.1 v_fs_e

`endpoint_t v_fs_e`——所属文件系统的端点号，VFS 通过此端点与底层 FS 通信
VFS 通过此端点与底层 FS 通信——所有文件操作请求（read/write/creat 等）通过 fs_sendrec(v_fs_e, ...) 发送

#### 2.1.2 v_inode_nr

`ino_t v_inode_nr`——inode 号，唯一标识底层 FS 上的一个文件
与底层 FS 的 inode 一一对应——v_fs_e + v_inode_nr 唯一确定一个 vnode

### 2.2 映射字段 (union mount)

#### 2.2.1 v_mapfs_e

`endpoint_t v_mapfs_e`——映射 FS 端点，union mount 场景下使用
union mount 场景下使用——当文件系统以 union 方式挂载时，v_mapfs_e 记录覆盖层的 FS 端点

#### 2.2.2 v_mapinode_nr

`ino_t v_mapinode_nr`——映射 inode 号，union mount 中覆盖层的 inode 号

#### 2.2.3 v_mapfs_count

`int v_mapfs_count`——映射 FS 引用计数，管理 union mount 中覆盖层 FS 的引用

### 2.3 文件属性字段

#### 2.3.1 v_mode

`mode_t v_mode`——文件类型和权限，包含文件类型（常规/目录/设备等）和 rwx 权限位

#### 2.3.2 v_uid / v_gid

`uid_t v_uid` / `gid_t v_gid`——文件属主的 UID/GID，用于权限检查

#### 2.3.3 v_size

`off_t v_size`——文件大小（字节），用于 read/write 的边界检查

### 2.4 引用计数字段

#### 2.4.1 v_ref_count

`int v_ref_count`——**核心字段**，VFS 层引用计数，记录有多少个 filp/fproc 指向此 vnode
fork 时通过 dup_vnode() 递增——对 fp_rd 和 fp_wd 调用 dup_vnode()
close 时通过 put_vnode() 递减——filp_count 降到 0 时调用 put_vnode()

#### 2.4.2 v_fs_count

`int v_fs_count`——底层 FS 引用计数，记录底层 FS 上打开的 inode 引用数
与 v_ref_count 的区别——v_ref_count 是 VFS 层的引用（filp/fproc 指向 vnode），v_fs_count 是底层 FS 层的引用（FS 上的 inode 打开计数）

### 2.5 设备字段

#### 2.5.1 v_bfs_e

`endpoint_t v_bfs_e`——块特殊文件的 FS 端点，用于块设备的 I/O 操作

#### 2.5.2 v_dev

`dev_t v_dev`——inode 所在设备号，标识文件所在的块设备

#### 2.5.3 v_sdev

`dev_t v_sdev`——特殊文件的设备号，字符/块设备文件的 rdev 值

### 2.6 挂载与锁字段

#### 2.6.1 v_vmnt

`struct vmnt *v_vmnt`——所属挂载点，vnode 通过此指针找到其所在的文件系统挂载信息
挂载点如何关联 vnode——v_vmnt 指向 vmnt 结构体，vmnt 的 m_root_node 指向该文件系统的根 vnode，m_mounted_on 指向挂载点目录 vnode

#### 2.6.2 v_lock

`tll_t v_lock`——三级锁（TLL），保护 vnode 的并发访问
详见 [07-tll-lock](07-tll-lock.md)

---

## 3. VNode 锁类型

### 3.1 锁类型常量

VNODE_NONE (无锁) / VNODE_READ (读锁) / VNODE_OPCL (open/close 互斥锁) / VNODE_WRITE (写锁)
与 TLL 锁级别的映射关系——VNODE_READ 对应 TLL_READ，VNODE_OPCL 和 VNODE_WRITE 对应 TLL_WRITE

---

## 4. VNode 操作函数

### 4.1 dup_vnode()

`vnode.c` 中的 `dup_vnode()` 函数——递增 vnode 的 v_ref_count
核心操作：`vnode->v_ref_count++`，简单递增引用计数
fork 时对 fp_rd 和 fp_wd 调用此函数——pm_fork() 中 `if (cp->fp_rd) dup_vnode(cp->fp_rd)`

### 4.2 put_vnode()

`vnode.c` 中的 `put_vnode()` 函数——递减 vnode 的 v_ref_count
核心操作：`vnode->v_ref_count--`，递减引用计数
v_ref_count 降到 0 时通知底层 FS 释放 inode——调用 req_putnode() 向底层 FS 发送 putnode 请求，递减 v_fs_count

### 4.3 get_vnode()

分配空闲 vnode slot 的逻辑——遍历 vnode[NR_VNODES] 寻找 v_ref_count==0 的空闲槽位
v_ref_count == 0 的 slot 可被分配——get_vnode() 找到空闲槽位后初始化字段并设置 v_ref_count=1

---

## 5. fork 时的 VNode 处理

### 5.1 fp_rd 和 fp_wd

fork 时 fproc 整体复制后 fp_rd/fp_wd 指向父进程的 vnode——子进程的 fp_rd/fp_wd 与父进程指向相同的 vnode
必须调用 dup_vnode() 递增 v_ref_count——否则父进程 close 后 vnode 被释放，子进程的指针悬空

### 5.2 不涉及的 vnode

fork 不影响 filp 指向的 vnode 的 v_ref_count——fork 增加的是 filp_count，不是 v_ref_count
因为 fork 增加的是 filp_count，不是 v_ref_count——filp_count 和 v_ref_count 是独立的引用计数
v_ref_count 只在 filp_count 降到 0 导致 put_vnode() 时才递减——这是引用计数链的传递释放机制

### 5.3 引用计数关系图

```
fork 前:
  fproc.parent
    fp_filp[0] ──→ filp (count=1) ──→ vnode (ref=1)
    fp_rd ──→ vnode_rd (ref=1)

fork 后:
  fproc.parent                    fproc.child
    fp_filp[0] ──┐                 fp_filp[0] ──┘  → filp (count=2) ──→ vnode (ref=1, 不变!)
    fp_rd ───────┐                 fp_rd ───────┘   → vnode_rd (ref=2)
```

---

## 6. C 源码

**文件**: `minix3/minix/servers/vfs/vnode.h`

```c
EXTERN struct vnode {
  endpoint_t v_fs_e;
  endpoint_t v_mapfs_e;
  ino_t v_inode_nr;
  ino_t v_mapinode_nr;
  mode_t v_mode;
  uid_t v_uid;
  gid_t v_gid;
  off_t v_size;
  int v_ref_count;
  int v_fs_count;
  int v_mapfs_count;
  endpoint_t v_bfs_e;
  dev_t v_dev;
  dev_t v_sdev;
  struct vmnt *v_vmnt;
  tll_t v_lock;
} vnode[NR_VNODES];
```
