# 09-vfs-syscalls: VFS 文件系统 syscall 封装

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 — syscall 封装（次主线·按服务分组）
> **源码**: `minix3/minix/lib/libc/sys/read.c` 等 VFS 相关 45+ 文件、`vectorio.c`
> **Rust 模块**: `os/libs/minix-sys`（vfs 模块）、`os/libs/minix-types`（ipc/vfs.rs）
> **draft 素材**: 无（新建）

## 核心点

- VFS 调用全清单（callnr.h VFS_BASE+0~63）：READ/WRITE/LSEEK/OPEN/CREAT/CLOSE/IOCTL/FCNTL/PIPE2/SELECT/GETDENTS/MOUNT/STAT 族等
- 文件描述符族：open（O_CREAT 双消息 VFS_CREAT）/close/read/write/lseek/dup/dup2（fcntl F_DUPFD 组合）
- 路径与元数据族：stat/fstat/lstat/chmod/chown/link/unlink/rename/mkdir/rmdir/symlink/readlink/truncate/mkfifo（mknod 组合）/access/umask
- 目录与流族：chdir/fchdir/getdents/fsync/sync/select/poll（select 组合）/readv/writev
- 组合函数标注：pread/pwrite=lseek+read/write、fstatfs=fstatvfs、poll=select
- socket 族排除声明（17-stage-net）

## 边界

- **前置依赖**: 05
- **不覆盖（移交）**: 网络 socket 族（排除→17-stage-net）、消息布局（99）、VFS server 实现（05-stage-vfs）
