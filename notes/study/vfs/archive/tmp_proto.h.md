# servers/vfs/proto.h 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/proto.h`  
**功能**: VFS（虚拟文件系统）函数原型声明  
**设计思想**: 集中声明所有 VFS 函数原型，提供模块化接口

---

## 逐行讲解

### 头文件保护

```c
#ifndef __VFS_PROTO_H__
#define __VFS_PROTO_H__

/* Function prototypes. */

#include <minix/rs.h>
#include <minix/timers.h>

#include "request.h"
#include "threads.h"
#include "tll.h"
#include "type.h"
```

**逐词拆解**:
- `__VFS_PROTO_H__`: 头文件保护宏
- `Function prototypes`: 函数原型
- 各种依赖头文件

**设计原因**:
- 防止头文件重复包含
- 包含必要的类型定义

---

### 前向声明

```c
/* Structs used in prototypes must be declared as such first. */
struct filp;
struct fproc;
struct timespec;
struct vmnt;
struct vnode;
struct lookup;
struct worker_thread;
struct job;
```

**逐词拆解**:
- `filp`: 文件指针结构
- `fproc`: 文件进程结构
- `timespec`: 时间规格
- `vmnt`: 虚拟挂载点
- `vnode`: 虚拟节点
- `lookup`: 查找结构
- `worker_thread`: 工作线程
- `job`: 任务结构

**设计原因**:
- 前向声明避免循环依赖
- 允许在函数原型中使用这些类型

---

### bdev.c - 块设备函数

```c
/* bdev.c */
int bdev_open(dev_t dev, int access);
int bdev_close(dev_t dev);
int bdev_ioctl(dev_t dev, endpoint_t proc_e, unsigned long req, vir_bytes buf);
void bdev_reply(void);
void bdev_up(devmajor_t major);
```

**逐词拆解**:
- `bdev_open`: 打开块设备
- `bdev_close`: 关闭块设备
- `bdev_ioctl`: 块设备控制
- `bdev_reply`: 块设备回复
- `bdev_up`: 块设备上线

**功能详解**:
1. **bdev_open**: 打开块设备，返回文件描述符
2. **bdev_close**: 关闭块设备
3. **bdev_ioctl**: 执行块设备控制操作
4. **bdev_reply**: 处理块设备驱动回复
5. **bdev_up**: 块设备驱动上线

---

### cdev.c - 字符设备函数

```c
/* cdev.c */
dev_t cdev_map(dev_t dev, struct fproc *rfp);
int cdev_open(int fd, dev_t dev, int flags);
int cdev_close(dev_t dev);
int cdev_io(int op, dev_t dev, endpoint_t proc_e, vir_bytes buf, off_t pos,
	unsigned long bytes, int flags);
int cdev_select(dev_t dev, int ops);
int cdev_cancel(dev_t dev, endpoint_t endpt, cp_grant_id_t grant);
void cdev_reply(void);
```

**逐词拆解**:
- `cdev_map`: 字符设备映射
- `cdev_open`: 打开字符设备
- `cdev_close`: 关闭字符设备
- `cdev_io`: 字符设备 I/O
- `cdev_select`: 字符设备选择
- `cdev_cancel`: 字符设备取消
- `cdev_reply`: 字符设备回复

**功能详解**:
1. **cdev_map**: 映射字符设备到进程
2. **cdev_open**: 打开字符设备
3. **cdev_close**: 关闭字符设备
4. **cdev_io**: 执行字符设备 I/O 操作
5. **cdev_select**: 字符设备选择操作
6. **cdev_cancel**: 取消字符设备操作
7. **cdev_reply**: 处理字符设备驱动回复

---

### comm.c - 通信函数

```c
/* comm.c */
int drv_sendrec(endpoint_t drv_e, message *reqm);
void fs_cancel(struct vmnt *vmp);
int fs_sendrec(endpoint_t fs_e, message *reqm);
int vm_sendrec(message *reqm);
void fs_sendmore(struct vmnt *vmp);
void send_work(void);
int vm_vfs_procctl_handlemem(endpoint_t ep, vir_bytes mem, vir_bytes len, int flags);
```

**逐词拆解**:
- `drv_sendrec`: 发送接收驱动消息
- `fs_cancel`: 文件系统取消
- `fs_sendrec`: 发送接收文件系统消息
- `vm_sendrec`: 发送接收 VM 消息
- `fs_sendmore`: 发送更多文件系统消息
- `send_work`: 发送工作
- `vm_vfs_procctl_handlemem`: VM VFS 进程控制处理内存

**功能详解**:
1. **drv_sendrec**: 与驱动通信
2. **fs_cancel**: 取消文件系统操作
3. **fs_sendrec**: 与文件系统通信
4. **vm_sendrec**: 与 VM 通信
5. **fs_sendmore**: 发送更多数据
6. **send_work**: 发送工作请求
7. **vm_vfs_procctl_handlemem**: 处理内存控制

---

### device.c - 设备函数

```c
/* device.c */
int do_ioctl(void);
cp_grant_id_t make_ioctl_grant(endpoint_t driver_e, endpoint_t user_e,
	vir_bytes buf, unsigned long request);
```

**逐词拆解**:
- `do_ioctl`: 处理 ioctl 系统调用
- `make_ioctl_grant`: 创建 ioctl 授权

**功能详解**:
1. **do_ioctl**: 处理设备控制操作
2. **make_ioctl_grant**: 创建 ioctl 授权，允许驱动访问用户缓冲区

---

### dmap.c - 设备映射函数

```c
/* dmap.c */
void lock_dmap(struct dmap *dp);
void unlock_dmap(struct dmap *dp);
int do_mapdriver(void);
void init_dmap(void);
int dmap_driver_match(endpoint_t proc, devmajor_t major);
void dmap_endpt_up(endpoint_t proc_nr, int is_blk);
struct dmap *get_dmap_by_endpt(endpoint_t proc_e);
struct dmap *get_dmap_by_major(devmajor_t major);
void dmap_unmap_by_endpt(endpoint_t proc_nr);
int map_service(struct rprocpub *rpub);
```

**逐词拆解**:
- `lock_dmap`: 锁定设备映射
- `unlock_dmap`: 解锁设备映射
- `do_mapdriver`: 映射驱动
- `init_dmap`: 初始化设备映射
- `dmap_driver_match`: 设备映射驱动匹配
- `dmap_endpt_up`: 设备映射端点上线
- `get_dmap_by_endpt`: 根据端点获取设备映射
- `get_dmap_by_major`: 根据主设备号获取设备映射
- `dmap_unmap_by_endpt`: 根据端点取消映射
- `map_service`: 映射服务

**功能详解**:
1. **lock_dmap/unlock_dmap**: 设备映射表锁定/解锁
2. **do_mapdriver**: 映射驱动程序
3. **init_dmap**: 初始化设备映射表
4. **dmap_driver_match**: 检查驱动是否匹配
5. **dmap_endpt_up**: 驱动上线
6. **get_dmap_by_endpt**: 根据端点查找设备映射
7. **get_dmap_by_major**: 根据主设备号查找设备映射
8. **dmap_unmap_by_endpt**: 取消驱动映射
9. **map_service**: 映射服务到设备

---

### elf_core_dump.c - ELF 核心转储函数

```c
/* elf_core_dump.c */
void write_elf_core_file(struct filp *f, int csig, char *exe_name);
```

**逐词拆解**:
- `write_elf_core_file`: 写入 ELF 核心文件

**功能详解**:
1. **write_elf_core_file**: 生成 ELF 格式的核心转储文件

---

### exec.c - 执行函数

```c
/* exec.c */
int pm_exec(vir_bytes path, size_t path_len, vir_bytes frame, size_t frame_len,
	vir_bytes *pc, vir_bytes *newsp, vir_bytes *ps_str);
```

**逐词拆解**:
- `pm_exec`: PM 执行

**功能详解**:
1. **pm_exec**: 执行新程序

---

### filedes.c - 文件描述符函数

```c
/* filedes.c */
void check_filp_locks(void);
void check_filp_locks_by_me(void);
void init_filps(void);
struct filp *find_filp(struct vnode *vp, mode_t bits);
struct filp *find_filp_by_sock_dev(dev_t dev);
int check_fds(struct fproc *rfp, int nfds);
int get_fd(struct fproc *rfp, int start, mode_t bits, int *k,
	struct filp **fpt);
struct filp *get_filp(int fild, tll_access_t locktype);
struct filp *get_filp2(struct fproc *rfp, int fild, tll_access_t locktype);
void lock_filp(struct filp *filp, tll_access_t locktype);
void unlock_filp(struct filp *filp);
void unlock_filps(struct filp *filp1, struct filp *filp2);
void invalidate_filp(struct filp *);
void invalidate_filp_by_endpt(endpoint_t proc_e);
void invalidate_filp_by_char_major(devmajor_t major);
void invalidate_filp_by_sock_drv(unsigned int num);
int close_filp(struct filp *fp, int may_suspend);
int do_copyfd(void);
```

**逐词拆解**:
- `check_filp_locks`: 检查文件指针锁
- `init_filps`: 初始化文件指针
- `find_filp`: 查找文件指针
- `get_fd`: 获取文件描述符
- `get_filp`: 获取文件指针
- `lock_filp`: 锁定文件指针
- `unlock_filp`: 解锁文件指针
- `close_filp`: 关闭文件指针
- `do_copyfd`: 复制文件描述符

**功能详解**:
1. **check_filp_locks**: 检查所有文件指针锁
2. **init_filps**: 初始化文件指针表
3. **find_filp**: 查找文件指针
4. **get_fd**: 获取空闲文件描述符
5. **get_filp**: 根据文件描述符获取文件指针
6. **lock_filp/unlock_filp**: 锁定/解锁文件指针
7. **close_filp**: 关闭文件指针
8. **do_copyfd**: 复制文件描述符

---

### link.c - 链接函数

```c
/* link.c */
int do_link(void);
int do_unlink(void);
int do_rename(void);
int do_truncate(void);
int do_ftruncate(void);
int truncate_vnode(struct vnode *vp, off_t newsize);
int rdlink_direct(char *orig_path, char *link_path, struct fproc *rfp);
```

**逐词拆解**:
- `do_link`: 处理 link 系统调用
- `do_unlink`: 处理 unlink 系统调用
- `do_rename`: 处理 rename 系统调用
- `do_truncate`: 处理 truncate 系统调用
- `do_ftruncate`: 处理 ftruncate 系统调用
- `truncate_vnode`: 截断 vnode
- `rdlink_direct`: 直接读取链接

**功能详解**:
1. **do_link**: 创建硬链接
2. **do_unlink**: 删除文件
3. **do_rename**: 重命名文件
4. **do_truncate**: 截断文件
5. **do_ftruncate**: 截断已打开的文件
6. **truncate_vnode**: 截断 vnode 到指定大小
7. **rdlink_direct**: 直接读取符号链接

---

### lock.c - 锁定函数

```c
/* lock.c */
int lock_op(int fd, int req, vir_bytes arg);
void lock_revive(void);
```

**逐词拆解**:
- `lock_op`: 锁定操作
- `lock_revive**: 锁定恢复

**功能详解**:
1. **lock_op**: 文件锁定操作
2. **lock_revive**: 恢复被阻塞的锁定请求

---

### main.c - 主函数

```c
/* main.c */
int main(void);
void lock_proc(struct fproc *rfp);
void unlock_proc(struct fproc *rfp);
void reply(message *m_out, endpoint_t whom, int result);
void replycode(endpoint_t whom, int result);
void service_pm_postponed(void);
void thread_cleanup(void);
```

**逐词拆解**:
- `main`: VFS 主函数
- `lock_proc`: 锁定进程
- `unlock_proc`: 解锁进程
- `reply`: 回复消息
- `replycode`: 回复代码
- `service_pm_postponed`: 服务 PM 延迟
- `thread_cleanup`: 线程清理

**功能详解**:
1. **main**: VFS 主循环
2. **lock_proc/unlock_proc**: 锁定/解锁进程
3. **reply**: 发送回复消息
4. **replycode**: 发送回复代码
5. **service_pm_postponed**: 处理延迟的 PM 请求
6. **thread_cleanup**: 清理线程资源

---

### misc.c - 杂项函数

```c
/* misc.c */
void pm_exit(void);
int do_fcntl(void);
void pm_fork(endpoint_t pproc, endpoint_t cproc, pid_t cpid);
void pm_setgid(endpoint_t proc_e, int egid, int rgid);
void pm_setuid(endpoint_t proc_e, int euid, int ruid);
void pm_setgroups(endpoint_t proc_e, int ngroups, gid_t *addr);
void pm_setsid(endpoint_t proc_e);
int do_sync(void);
int do_fsync(void);
void pm_reboot(void);
int do_svrctl(void);
int do_getsysinfo(void);
int do_vm_call(void);
int pm_dumpcore(int sig, vir_bytes exe_name);
void ds_event(void);
int dupvm(struct fproc *fp, int pfd, int *vmfd, struct filp **f);
int do_getrusage(void);
```

**逐词拆解**:
- `pm_exit`: PM 退出
- `do_fcntl`: 处理 fcntl 系统调用
- `pm_fork`: PM fork
- `pm_setgid`: PM 设置 GID
- `pm_setuid`: PM 设置 UID
- `pm_setgroups`: PM 设置组
- `pm_setsid`: PM 设置会话
- `do_sync`: 处理 sync 系统调用
- `do_fsync`: 处理 fsync 系统调用
- `pm_reboot`: PM 重启
- `do_svrctl`: 处理 svrctl 系统调用
- `do_getsysinfo`: 获取系统信息
- `do_vm_call`: VM 调用
- `pm_dumpcore`: PM 核心转储
- `ds_event`: DS 事件
- `dupvm`: 复制 VM
- `do_getrusage`: 获取资源使用

**功能详解**:
1. **pm_exit**: 进程退出处理
2. **do_fcntl**: 文件控制操作
3. **pm_fork**: fork 处理
4. **pm_setgid/setuid**: 设置 GID/UID
5. **pm_setgroups**: 设置补充组
6. **pm_setsid**: 创建新会话
7. **do_sync/fsync**: 同步文件系统
8. **pm_reboot**: 重启处理
9. **do_svrctl**: 服务器控制
10. **do_getsysinfo**: 获取系统信息
11. **do_vm_call**: VM 调用
12. **pm_dumpcore**: 核心转储
13. **ds_event**: 数据存储事件
14. **dupvm**: 复制 VM 文件描述符
15. **do_getrusage**: 获取资源使用情况

---

### mount.c - 挂载函数

```c
/* mount.c */
int do_mount(void);
int do_umount(void);
int is_nonedev(dev_t dev);
void mount_pfs(void);
int mount_fs(dev_t dev, char mount_dev[PATH_MAX], char mount_path[PATH_MAX],
	endpoint_t fs_e, int rdonly, char mount_type[FSTYPE_MAX],
	char mount_label[LABEL_MAX]);
int unmount(dev_t dev, char label[LABEL_MAX]);
void unmount_all(int force);
```

**逐词拆解**:
- `do_mount`: 处理 mount 系统调用
- `do_umount`: 处理 umount 系统调用
- `is_nonedev`: 是否为无设备
- `mount_pfs`: 挂载 PFS
- `mount_fs`: 挂载文件系统
- `unmount`: 卸载
- `unmount_all`: 卸载所有

**功能详解**:
1. **do_mount**: 挂载文件系统
2. **do_umount**: 卸载文件系统
3. **is_nonedev**: 检查是否为无设备文件系统
4. **mount_pfs**: 挂载进程文件系统
5. **mount_fs**: 挂载文件系统
6. **unmount**: 卸载文件系统
7. **unmount_all**: 卸载所有文件系统

---

### open.c - 打开函数

```c
/* open.c */
int do_close(void);
int close_fd(struct fproc *rfp, int fd_nr, int may_suspend);
int common_open(char path[PATH_MAX], int oflags, mode_t omode, int for_exec);
int do_creat(void);
int do_lseek(void);
int do_mknod(void);
int do_mkdir(void);
int do_open(void);
int do_creat(void);
int do_slink(void);
int actual_lseek(struct fproc *rfp, int seekfd, int seekwhence, off_t offset,
	off_t *newposp);
```

**逐词拆解**:
- `do_close`: 处理 close 系统调用
- `close_fd`: 关闭文件描述符
- `common_open`: 通用打开
- `do_creat`: 处理 creat 系统调用
- `do_lseek`: 处理 lseek 系统调用
- `do_mknod`: 处理 mknod 系统调用
- `do_mkdir`: 处理 mkdir 系统调用
- `do_open`: 处理 open 系统调用
- `do_slink`: 处理 symlink 系统调用
- `actual_lseek`: 实际 lseek

**功能详解**:
1. **do_close**: 关闭文件
2. **close_fd**: 关闭指定文件描述符
3. **common_open**: 通用打开函数
4. **do_creat**: 创建文件
5. **do_lseek**: 移动文件指针
6. **do_mknod**: 创建特殊文件
7. **do_mkdir**: 创建目录
8. **do_open**: 打开文件
9. **do_slink**: 创建符号链接
10. **actual_lseek**: 实际的 lseek 实现

---

### path.c - 路径函数

```c
/* path.c */
struct vnode *advance(struct vnode *dirp, struct lookup *resolve, struct
	fproc *rfp);
struct vnode *eat_path(struct lookup *resolve, struct fproc *rfp);
struct vnode *last_dir(struct lookup *resolve, struct fproc *rfp);
void lookup_init(struct lookup *resolve, char *path, int flags, struct
	vmnt **vmp, struct vnode **vp);
int get_name(struct vnode *dirp, struct vnode *entry, char *_name);
int canonical_path(char *orig_path, struct fproc *rfp);
int do_socketpath(void);
```

**逐词拆解**:
- `advance`: 前进
- `eat_path`: 吃掉路径
- `last_dir`: 最后目录
- `lookup_init`: 查找初始化
- `get_name`: 获取名称
- `canonical_path`: 规范路径
- `do_socketpath`: 处理 socketpath

**功能详解**:
1. **advance**: 在目录中前进到下一项
2. **eat_path**: 解析路径并返回 vnode
3. **last_dir**: 获取路径的最后一个目录
4. **lookup_init**: 初始化查找结构
5. **get_name**: 获取目录项名称
6. **canonical_path**: 获取规范路径
7. **do_socketpath**: 处理 socket 路径

---

### pipe.c - 管道函数

```c
/* pipe.c */
int do_pipe2(void);
int map_vnode(struct vnode *vp, endpoint_t fs_e);
void unpause(void);
int pipe_check(struct filp *filp, int rw_flag, int oflags, int bytes,
	int notouch);
void release(struct vnode *vp, int op, int count);
void revive(endpoint_t proc_e, int returned);
void suspend(int why);
void pipe_suspend(int callnr, int fd, vir_bytes buf, size_t size,
	size_t cum_io);
void unsuspend_by_endpt(endpoint_t proc_e);
```

**逐词拆解**:
- `do_pipe2`: 处理 pipe2 系统调用
- `map_vnode`: 映射 vnode
- `unpause`: 取消暂停
- `pipe_check`: 管道检查
- `release`: 释放
- `revive`: 恢复
- `suspend`: 挂起
- `pipe_suspend`: 管道挂起
- `unsuspend_by_endpt`: 根据端点取消挂起

**功能详解**:
1. **do_pipe2**: 创建管道
2. **map_vnode**: 映射 vnode 到文件系统
3. **unpause**: 取消暂停进程
4. **pipe_check**: 检查管道状态
5. **release**: 释放管道资源
6. **revive**: 恢复阻塞的进程
7. **suspend**: 挂起当前进程
8. **pipe_suspend**: 管道操作挂起
9. **unsuspend_by_endpt**: 取消指定进程的挂起

---

### protect.c - 保护函数

```c
/* protect.c */
int do_access(void);
int do_chmod(void);
int do_chown(void);
int do_umask(void);
int forbidden(struct fproc *rfp, struct vnode *vp, mode_t
	access_desired);
int read_only(struct vnode *vp);
```

**逐词拆解**:
- `do_access`: 处理 access 系统调用
- `do_chmod`: 处理 chmod 系统调用
- `do_chown`: 处理 chown 系统调用
- `do_umask`: 处理 umask 系统调用
- `forbidden`: 禁止
- `read_only`: 只读

**功能详解**:
1. **do_access**: 检查文件访问权限
2. **do_chmod**: 修改文件权限
3. **do_chown**: 修改文件所有者
4. **do_umask**: 设置文件创建掩码
5. **forbidden**: 检查访问是否被禁止
6. **read_only**: 检查文件系统是否只读

---

### read.c - 读函数

```c
/* read.c */
int do_read(void);
int do_getdents(void);
void lock_bsf(void);
void unlock_bsf(void);
void check_bsf_lock(void);
int do_read_write_peek(int rw_flag, int fd, vir_bytes buf, size_t bytes);
int actual_read_write_peek(struct fproc *rfp, int rw_flag, int fd,
	vir_bytes buf, size_t bytes);
int read_write(struct fproc *rfp, int rw_flag, int fd, struct filp *f,
	vir_bytes buffer, size_t nbytes, endpoint_t for_e);
int rw_pipe(int rw_flag, endpoint_t usr, struct filp *f, int callnr, int fd,
	vir_bytes buf, size_t nbytes, size_t cum_io);
```

**逐词拆解**:
- `do_read`: 处理 read 系统调用
- `do_getdents`: 处理 getdents 系统调用
- `lock_bsf`: 锁定 BSF
- `unlock_bsf`: 解锁 BSF
- `check_bsf_lock`: 检查 BSF 锁
- `do_read_write_peek`: 读/写/查看
- `actual_read_write_peek`: 实际读/写/查看
- `read_write`: 读/写
- `rw_pipe`: 管道读/写

**功能详解**:
1. **do_read**: 读取文件
2. **do_getdents**: 获取目录项
3. **lock_bsf/unlock_bsf**: 锁定/解锁块特殊文件
4. **check_bsf_lock**: 检查块特殊文件锁
5. **do_read_write_peek**: 读/写/查看操作
6. **actual_read_write_peek**: 实际的读/写/查看实现
7. **read_write**: 读/写操作
8. **rw_pipe**: 管道读/写

---

### request.c - 请求函数

```c
/* request.c */
int req_breadwrite(endpoint_t fs_e, endpoint_t user_e, dev_t dev, off_t pos,
	unsigned int num_of_bytes, vir_bytes user_addr, int rw_flag,
	off_t *new_posp, size_t *cum_iop);
int req_chmod(endpoint_t fs_e, ino_t inode_nr, mode_t rmode,
	mode_t *new_modep);
int req_chown(endpoint_t fs_e, ino_t inode_nr, uid_t newuid, gid_t newgid,
	mode_t *new_modep);
int req_create(endpoint_t fs_e, ino_t inode_nr, int omode, uid_t uid,
	gid_t gid, char *path, node_details_t *res);
int req_flush(endpoint_t fs_e, dev_t dev);
int req_statvfs(endpoint_t fs_e, struct statvfs *buf);
int req_ftrunc(endpoint_t fs_e, ino_t inode_nr, off_t start, off_t end);
int req_getdents(endpoint_t fs_e, ino_t inode_nr, off_t pos, vir_bytes buf,
	size_t size, off_t *new_pos, int direct);
int req_inhibread(endpoint_t fs_e, ino_t inode_nr);
int req_link(endpoint_t fs_e, ino_t link_parent, char *lastc,
	ino_t linked_file);
int req_lookup(endpoint_t fs_e, ino_t dir_ino, ino_t root_ino, uid_t uid,
	gid_t gid, struct lookup *resolve, lookup_res_t *res,
	struct fproc *rfp);
int req_mkdir(endpoint_t fs_e, ino_t inode_nr, char *lastc, uid_t uid,
	gid_t gid, mode_t dmode);
int req_mknod(endpoint_t fs_e, ino_t inode_nr, char *lastc, uid_t uid,
	gid_t gid, mode_t dmode, dev_t dev);
int req_mountpoint(endpoint_t fs_e, ino_t inode_nr);
int req_newnode(endpoint_t fs_e, uid_t uid, gid_t gid, mode_t dmode,
	dev_t dev, struct node_details *res);
int req_putnode(int fs_e, ino_t inode_nr, int count);
int req_rdlink(endpoint_t fs_e, ino_t inode_nr, endpoint_t proc_e,
	vir_bytes buf, size_t len, int direct);
int req_readsuper(struct vmnt *vmp, char *driver_name, dev_t dev, int readonly,
	int isroot, struct node_details *res_nodep, unsigned int *fs_flags);
int req_readwrite(endpoint_t fs_e, ino_t inode_nr, off_t pos, int rw_flag,
	endpoint_t user_e, vir_bytes user_addr, unsigned int num_of_bytes,
	off_t *new_posp, size_t *cum_iop);
int req_bpeek(endpoint_t fs_e, dev_t dev, off_t pos, unsigned int num_of_bytes);
int req_peek(endpoint_t fs_e, ino_t inode_nr, off_t pos, unsigned int bytes);
int req_rename(endpoint_t fs_e, ino_t old_dir, char *old_name, ino_t new_dir,
	char *new_name);
int req_rmdir(endpoint_t fs_e, ino_t inode_nr, char *lastc);
int req_slink(endpoint_t fs_e, ino_t inode_nr, char *lastc, endpoint_t proc_e,
	vir_bytes path_addr, size_t path_length, uid_t uid, gid_t gid);
int req_stat(endpoint_t fs_e, ino_t inode_nr, endpoint_t proc_e, vir_bytes buf);
int req_sync(endpoint_t fs_e);
int req_unlink(endpoint_t fs_e, ino_t inode_nr, char *lastc);
int req_unmount(endpoint_t fs_e);
int req_utime(endpoint_t fs_e, ino_t inode_nr, struct timespec * actv,
	struct timespec * modtv);
int req_newdriver(endpoint_t fs_e, dev_t dev, char *label);
```

**逐词拆解**:
- `req_*`: 请求函数系列
- 各种文件系统操作请求

**功能详解**:
1. **req_breadwrite**: 块读/写请求
2. **req_chmod**: 修改权限请求
3. **req_chown**: 修改所有者请求
4. **req_create**: 创建文件请求
5. **req_flush**: 刷新请求
6. **req_statvfs**: 获取文件系统状态请求
7. **req_ftrunc**: 截断文件请求
8. **req_getdents**: 获取目录项请求
9. **req_inhibread**: 禁止读请求
10. **req_link**: 创建链接请求
11. **req_lookup**: 查找请求
12. **req_mkdir**: 创建目录请求
13. **req_mknod**: 创建特殊文件请求
14. **req_mountpoint**: 挂载点请求
15. **req_newnode**: 新节点请求
16. **req_putnode**: 放置节点请求
17. **req_rdlink**: 读链接请求
18. **req_readsuper**: 读超级块请求
19. **req_readwrite**: 读/写请求
20. **req_bpeek**: 块查看请求
21. **req_peek**: 查看请求
22. **req_rename**: 重命名请求
23. **req_rmdir**: 删除目录请求
24. **req_slink**: 创建符号链接请求
25. **req_stat**: 状态请求
26. **req_sync**: 同步请求
27. **req_unlink**: 删除文件请求
28. **req_unmount**: 卸载请求
29. **req_utime**: 修改时间请求
30. **req_newdriver**: 新驱动请求

---

### sdev.c - Socket 设备函数

```c
/* sdev.c */
int sdev_socket(int domain, int type, int protocol, dev_t *dev, int pair);
int sdev_bind(dev_t dev, vir_bytes addr, unsigned int addr_len,
	int filp_flags);
int sdev_connect(dev_t dev, vir_bytes addr, unsigned int addr_len,
	int filp_flags);
int sdev_listen(dev_t dev, int backlog);
int sdev_accept(dev_t dev, vir_bytes addr, unsigned int addr_len,
	int filp_flags, int fd);
int sdev_readwrite(dev_t dev, vir_bytes data_buf, size_t data_len,
	vir_bytes ctl_buf, unsigned int ctl_len, vir_bytes addr_buf,
	unsigned int addr_len, int flags, int rw_flag, int filp_flags,
	vir_bytes user_buf);
int sdev_ioctl(dev_t dev, unsigned long request, vir_bytes buf,
	int filp_flags);
int sdev_setsockopt(dev_t dev, int level, int name, vir_bytes addr,
	unsigned int len);
int sdev_getsockopt(dev_t dev, int level, int name, vir_bytes addr,
	unsigned int *len);
int sdev_getsockname(dev_t dev, vir_bytes addr, unsigned int *addr_len);
int sdev_getpeername(dev_t dev, vir_bytes addr, unsigned int *addr_len);
int sdev_shutdown(dev_t dev, int how);
int sdev_close(dev_t dev, int may_suspend);
int sdev_select(dev_t dev, int ops);
void sdev_stop(struct fproc *rfp);
void sdev_cancel(void);
void sdev_reply(void);
```

**逐词拆解**:
- `sdev_*`: Socket 设备函数系列
- 各种 socket 操作

**功能详解**:
1. **sdev_socket**: 创建 socket
2. **sdev_bind**: 绑定地址
3. **sdev_connect**: 连接
4. **sdev_listen**: 监听
5. **sdev_accept**: 接受连接
6. **sdev_readwrite**: 读/写
7. **sdev_ioctl**: 控制
8. **sdev_setsockopt**: 设置选项
9. **sdev_getsockopt**: 获取选项
10. **sdev_getsockname**: 获取 socket 名称
11. **sdev_getpeername**: 获取对端名称
12. **sdev_shutdown**: 关闭
13. **sdev_close**: 关闭
14. **sdev_select**: 选择
15. **sdev_stop**: 停止
16. **sdev_cancel**: 取消
17. **sdev_reply**: 回复

---

### smap.c - Socket 映射函数

```c
/* smap.c */
void init_smap(void);
int smap_map(const char *label, endpoint_t endpt, const int *domains,
	unsigned int ndomains);
void smap_unmap_by_endpt(endpoint_t endpt);
void smap_endpt_up(endpoint_t endpt);
dev_t make_smap_dev(struct smap *sp, sockid_t sockid);
struct smap *get_smap_by_endpt(endpoint_t endpt);
struct smap *get_smap_by_domain(int domain);
struct smap *get_smap_by_dev(dev_t dev, sockid_t * sockidp);
```

**逐词拆解**:
- `smap_*`: Socket 映射函数系列

**功能详解**:
1. **init_smap**: 初始化 socket 映射
2. **smap_map**: 映射 socket 服务
3. **smap_unmap_by_endpt**: 根据端点取消映射
4. **smap_endpt_up**: 端点上线
5. **make_smap_dev**: 创建 socket 映射设备
6. **get_smap_by_endpt**: 根据端点获取映射
7. **get_smap_by_domain**: 根据域获取映射
8. **get_smap_by_dev**: 根据设备获取映射

---

### socket.c - Socket 函数

```c
/* socket.c */
int do_socket(void);
int do_socketpair(void);
int do_bind(void);
int do_connect(void);
int do_listen(void);
int do_accept(void);
void resume_accept(struct fproc *rfp, int status, dev_t dev,
	unsigned int addr_len, int listen_fd);
int do_sendto(void);
int do_recvfrom(void);
void resume_recvfrom(struct fproc *rfp, int status, unsigned int addr_len);
int do_sockmsg(void);
void resume_recvmsg(struct fproc *rfp, int status, unsigned int ctl_len,
	unsigned int addr_len, int flags, vir_bytes msg_buf);
int do_setsockopt(void);
int do_getsockopt(void);
int do_getsockname(void);
int do_getpeername(void);
int do_shutdown(void);
```

**逐词拆解**:
- `do_*`: Socket 系统调用处理函数
- `resume_*`: 恢复函数

**功能详解**:
1. **do_socket**: 创建 socket
2. **do_socketpair**: 创建 socket 对
3. **do_bind**: 绑定地址
4. **do_connect**: 连接
5. **do_listen**: 监听
6. **do_accept**: 接受连接
7. **resume_accept**: 恢复接受
8. **do_sendto**: 发送数据
9. **do_recvfrom**: 接收数据
10. **resume_recvfrom**: 恢复接收
11. **do_sockmsg**: socket 消息
12. **resume_recvmsg**: 恢复接收消息
13. **do_setsockopt**: 设置选项
14. **do_getsockopt**: 获取选项
15. **do_getsockname**: 获取 socket 名称
16. **do_getpeername**: 获取对端名称
17. **do_shutdown**: 关闭

---

### stadir.c - 状态和目录函数

```c
/* stadir.c */
int do_chdir(void);
int do_fchdir(void);
int do_chroot(void);
int do_fstat(void);
int do_stat(void);
int do_statvfs(void);
int do_fstatvfs(void);
int do_getvfsstat(void);
int do_rdlink(void);
int do_lstat(void);
int update_statvfs(struct vmnt *vmp, struct statvfs *buf);
```

**逐词拆解**:
- `do_*`: 状态和目录系统调用处理函数

**功能详解**:
1. **do_chdir**: 改变当前目录
2. **do_fchdir**: 通过文件描述符改变目录
3. **do_chroot**: 改变根目录
4. **do_fstat**: 获取文件状态
5. **do_stat**: 获取文件状态
6. **do_statvfs**: 获取文件系统状态
7. **do_fstatvfs**: 通过文件描述符获取文件系统状态
8. **do_getvfsstat**: 获取 VFS 状态
9. **do_rdlink**: 读符号链接
10. **do_lstat**: 获取链接状态
11. **update_statvfs**: 更新文件系统状态

---

### time.c - 时间函数

```c
/* time.c */
int do_utimens(void);
```

**逐词拆解**:
- `do_utimens`: 处理 utimens 系统调用

**功能详解**:
1. **do_utimens**: 修改文件时间戳

---

### tll.c - 三级锁函数

```c
/* tll.c */
void tll_downgrade(tll_t *tllp);
int tll_haspendinglock(tll_t *tllp);
void tll_init(tll_t *tllp);
int tll_islocked(tll_t *tllp);
int tll_lock(tll_t *tllp, tll_access_t locktype);
int tll_locked_by_me(tll_t *tllp);
void tll_lockstat(tll_t *tllp);
int tll_unlock(tll_t *tllp);
void tll_upgrade(tll_t *tllp);
```

**逐词拆解**:
- `tll_*`: 三级锁函数系列

**功能详解**:
1. **tll_downgrade**: 降级锁
2. **tll_haspendinglock**: 检查是否有待处理锁
3. **tll_init**: 初始化锁
4. **tll_islocked**: 检查是否锁定
5. **tll_lock**: 锁定
6. **tll_locked_by_me**: 检查是否由我锁定
7. **tll_lockstat**: 锁状态
8. **tll_unlock**: 解锁
9. **tll_upgrade**: 升级锁

---

### utility.c - 工具函数

```c
/* utility.c */
int copy_path(char *dest, size_t size);
int fetch_name(vir_bytes path, size_t len, char *dest);
int isokendpt_f(const char *f, int l, endpoint_t e, int *p, int ft);
int in_group(struct fproc *rfp, gid_t grp);
int sys_datacopy_wrapper(endpoint_t src, vir_bytes srcv, endpoint_t dst,
	vir_bytes dstv, size_t len);

#define okendpt(e, p) isokendpt_f(__FILE__, __LINE__, (e), (p), 1)
#define isokendpt(e, p) isokendpt_f(__FILE__, __LINE__, (e), (p), 0)
```

**逐词拆解**:
- `copy_path`: 复制路径
- `fetch_name`: 获取名称
- `isokendpt_f`: 验证端点
- `in_group`: 检查组
- `sys_datacopy_wrapper`: 系统数据复制包装器
- `okendpt`: 验证端点宏
- `isokendpt`: 验证端点宏

**功能详解**:
1. **copy_path**: 复制路径字符串
2. **fetch_name**: 从用户空间获取名称
3. **isokendpt_f**: 验证端点有效性
4. **in_group**: 检查进程是否在组中
5. **sys_datacopy_wrapper**: 数据复制包装器
6. **okendpt/isokendpt**: 验证端点宏

---

### vmnt.c - 虚拟挂载点函数

```c
/* vmnt.c */
void check_vmnt_locks(void);
void check_vmnt_locks_by_me(struct fproc *rfp);
void mark_vmnt_free(struct vmnt *vmp);
struct vmnt *get_free_vmnt(void);
struct vmnt *find_vmnt(endpoint_t fs_e);
struct vmnt *get_locked_vmnt(struct fproc *rfp);
void init_vmnts(void);
int lock_vmnt(struct vmnt *vp, tll_access_t locktype);
void unlock_vmnt(struct vmnt *vp);
void vmnt_unmap_by_endpt(endpoint_t proc_e);
void fetch_vmnt_paths(void);
void upgrade_vmnt_lock(struct vmnt *vmp);
void downgrade_vmnt_lock(struct vmnt *vmp);
```

**逐词拆解**:
- `vmnt_*`: 虚拟挂载点函数系列

**功能详解**:
1. **check_vmnt_locks**: 检查挂载点锁
2. **check_vmnt_locks_by_me**: 检查我的挂载点锁
3. **mark_vmnt_free**: 标记挂载点为空闲
4. **get_free_vmnt**: 获取空闲挂载点
5. **find_vmnt**: 查找挂载点
6. **get_locked_vmnt**: 获取锁定的挂载点
7. **init_vmnts**: 初始化挂载点
8. **lock_vmnt/unlock_vmnt**: 锁定/解锁挂载点
9. **vmnt_unmap_by_endpt**: 根据端点取消映射
10. **fetch_vmnt_paths**: 获取挂载点路径
11. **upgrade_vmnt_lock/downgrade_vmnt_lock**: 升级/降级挂载点锁

---

### vnode.c - 虚拟节点函数

```c
/* vnode.c */
void check_vnode_locks(void);
void check_vnode_locks_by_me(struct fproc *rfp);
struct vnode *get_free_vnode(void);
struct vnode *find_vnode(int fs_e, ino_t inode);
void init_vnodes(void);
int is_vnode_locked(struct vnode *vp);
int lock_vnode(struct vnode *vp, tll_access_t locktype);
void unlock_vnode(struct vnode *vp);
void dup_vnode(struct vnode *vp);
void put_vnode(struct vnode *vp);
void vnode_clean_refs(struct vnode *vp);
void upgrade_vnode_lock(struct vnode *vp);
```

**逐词拆解**:
- `vnode_*`: 虚拟节点函数系列

**功能详解**:
1. **check_vnode_locks**: 检查 vnode 锁
2. **check_vnode_locks_by_me**: 检查我的 vnode 锁
3. **get_free_vnode**: 获取空闲 vnode
4. **find_vnode**: 查找 vnode
5. **init_vnodes**: 初始化 vnode
6. **is_vnode_locked**: 检查 vnode 是否锁定
7. **lock_vnode/unlock_vnode**: 锁定/解锁 vnode
8. **dup_vnode**: 复制 vnode
9. **put_vnode**: 释放 vnode
10. **vnode_clean_refs**: 清理 vnode 引用
11. **upgrade_vnode_lock**: 升级 vnode 锁

---

### write.c - 写函数

```c
/* write.c */
int do_write(void);
```

**逐词拆解**:
- `do_write`: 处理 write 系统调用

**功能详解**:
1. **do_write**: 写入文件

---

### gcov.c - GCOV 函数

```c
/* gcov.c */
int do_gcov_flush(void);
#if ! USE_COVERAGE
#define do_gcov_flush NULL
#endif
```

**逐词拆解**:
- `do_gcov_flush`: GCOV 刷新

**功能详解**:
1. **do_gcov_flush**: 刷新 GCOV 数据

---

### select.c - 选择函数

```c
/* select.c */
int do_select(void);
void init_select(void);
void select_callback(struct filp *, int ops);
void select_forget(void);
void select_cdev_reply1(endpoint_t driver_e, devminor_t minor, int status);
void select_cdev_reply2(endpoint_t driver_e, devminor_t minor, int status);
void select_sdev_reply1(dev_t dev, int status);
void select_sdev_reply2(dev_t dev, int status);
void select_unsuspend_by_endpt(endpoint_t proc);
void select_dump(void);
```

**逐词拆解**:
- `select_*`: 选择函数系列

**功能详解**:
1. **do_select**: 处理 select 系统调用
2. **init_select**: 初始化选择
3. **select_callback**: 选择回调
4. **select_forget**: 忘记选择
5. **select_cdev_reply1/2**: 字符设备选择回复
6. **select_sdev_reply1/2**: socket 设备选择回复
7. **select_unsuspend_by_endpt**: 取消选择挂起
8. **select_dump**: 选择转储

---

### worker.c - 工作线程函数

```c
/* worker.c */
void worker_init(void);
void worker_cleanup(void);
int worker_idle(void);
int worker_available(void);
void worker_allow(int allow);
struct worker_thread *worker_get(thread_t worker_tid);
void worker_signal(struct worker_thread *worker);
int worker_can_start(struct fproc *rfp);
void worker_start(struct fproc *rfp, void (*func)(void), message *m_ptr,
	int use_spare);
void worker_stop(struct worker_thread *worker);
void worker_stop_by_endpt(endpoint_t proc_e);
void worker_yield(void);
void worker_wait(void);
struct worker_thread *worker_suspend(void);
void worker_resume(struct worker_thread *org_self);
void worker_set_proc(struct fproc *rfp);
#endif
```

**逐词拆解**:
- `worker_*`: 工作线程函数系列

**功能详解**:
1. **worker_init**: 初始化工作线程
2. **worker_cleanup**: 清理工作线程
3. **worker_idle**: 工作线程空闲
4. **worker_available**: 工作线程可用
5. **worker_allow**: 允许工作线程
6. **worker_get**: 获取工作线程
7. **worker_signal**: 发送信号给工作线程
8. **worker_can_start**: 工作线程是否可以启动
9. **worker_start**: 启动工作线程
10. **worker_stop**: 停止工作线程
11. **worker_stop_by_endpt**: 根据端点停止工作线程
12. **worker_yield**: 工作线程让步
13. **worker_wait**: 工作线程等待
14. **worker_suspend**: 挂起工作线程
15. **worker_resume**: 恢复工作线程
16. **worker_set_proc**: 设置工作线程进程

---

## Rust 实现对比

### C 代码（原始）

```c
int do_open(void);
int do_close(void);
int do_read(void);
int do_write(void);
```

### Rust 代码（现代实现）

```rust
#![no_std]

pub trait VirtualFileSystem {
    fn do_open(&mut self, path: &str, flags: i32, mode: u32) -> Result<i32, VfsError>;
    fn do_close(&mut self, fd: i32) -> Result<(), VfsError>;
    fn do_read(&mut self, fd: i32, buf: &mut [u8]) -> Result<usize, VfsError>;
    fn do_write(&mut self, fd: i32, buf: &[u8]) -> Result<usize, VfsError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    InvalidFd,
    NotPermitted,
    NoSpace,
    IoError,
}

pub struct VfsServer {
    file_table: [Option<FileEntry>; 1024],
    mount_table: [Option<MountEntry>; 64],
}

impl VirtualFileSystem for VfsServer {
    fn do_open(&mut self, path: &str, flags: i32, mode: u32) -> Result<i32, VfsError> {
        // 实现细节
        Ok(0)
    }

    fn do_close(&mut self, fd: i32) -> Result<(), VfsError> {
        // 实现细节
        Ok(())
    }

    fn do_read(&mut self, fd: i32, buf: &mut [u8]) -> Result<usize, VfsError> {
        // 实现细节
        Ok(0)
    }

    fn do_write(&mut self, fd: i32, buf: &[u8]) -> Result<usize, VfsError> {
        // 实现细节
        Ok(0)
    }
}
```

### Rust 优势分析

**1. Trait 抽象**:
```rust
// C: 使用函数指针
int do_open(void);

// Rust: 使用 Trait
pub trait VirtualFileSystem {
    fn do_open(&mut self, path: &str, flags: i32, mode: u32) -> Result<i32, VfsError>;
}
```

**2. Result 类型**:
```rust
// C: 使用错误码
int do_open(void);

// Rust: 使用 Result
fn do_open(&mut self, path: &str, flags: i32, mode: u32) -> Result<i32, VfsError>;
```

**3. 类型安全**:
```rust
// C: 使用整数
int fd;

// Rust: 使用类型
pub struct FileDescriptor(i32);
```

---

## 要点总结

1. **模块化组织**: 按功能模块组织函数原型
2. **文件系统抽象**: VFS 提供统一的文件系统接口
3. **设备管理**: 块设备、字符设备、socket 设备

---

## 灾难预演

**如果删除 do_open 函数**:
- 无法打开文件
- 系统无法启动新程序
- 系统功能严重受限

**如果删除 do_read 函数**:
- 无法读取文件
- 数据无法加载
- 系统无法正常工作

---

## 互动自测

1. **问题**: proto.h 的作用是什么？
   **答案**: 集中声明所有 VFS 函数原型，提供模块化接口。

2. **问题**: VFS 的主要功能是什么？
   **答案**: 提供统一的文件系统接口，管理文件、设备、挂载点等。

3. **问题**: vnode 和 vmnt 的作用是什么？
   **答案**: vnode 表示虚拟节点（文件），vmnt 表示虚拟挂载点。
