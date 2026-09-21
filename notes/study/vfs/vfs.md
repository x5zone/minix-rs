# VFS（虚拟文件系统服务器）

> **学习目标**: 理解文件系统抽象层的实现，VFS 如何通过 IPC 与具体文件系统交互。
> 
> **核心问题**: open/read/write 如何通过 IPC 实现？

---

## 头文件

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/vfs/fs.h` | ⏳ 待读 | VFS 文件系统头文件 | - |
| 2 | `servers/vfs/fproc.h` | ⏳ 待读 | VFS 进程结构 | - |
| 3 | `servers/vfs/vnode.h` | ⏳ 待读 | 虚拟节点 | - |
| 4 | `servers/vfs/vmnt.h` | ⏳ 待读 | 挂载点 | - |
| 5 | `servers/vfs/type.h` | ⏳ 待读 | VFS 类型定义 | - |
| 6 | `servers/vfs/proto.h` | ⏳ 待读 | VFS 函数原型 | - |
| 7 | `servers/vfs/glo.h` | ⏳ 待读 | VFS 全局变量 | - |
| 8 | `servers/vfs/const.h` | ⏳ 待读 | VFS 常量 | - |
| 9 | `servers/vfs/dmap.h` | ⏳ 待读 | 设备映射 | - |
| 10 | `servers/vfs/request.h` | ⏳ 待读 | 请求头文件 | - |
| 11 | `servers/vfs/path.h` | ⏳ 待读 | 路径头文件 | - |
| 12 | `servers/vfs/tll.h` | ⏳ 待读 | TLL 头文件 | - |
| 13 | `servers/vfs/threads.h` | ⏳ 待读 | 线程头文件 | - |
| 14 | `servers/vfs/file.h` | ⏳ 待读 | 文件头文件 | - |
| 15 | `servers/vfs/lock.h` | ⏳ 待读 | 锁头文件 | - |

---

## 实现文件

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 16 | `servers/vfs/main.c` | ⏳ 待读 | VFS 主循环 | - |
| 17 | `servers/vfs/open.c` | ⏳ 待读 | open 实现 | - |
| 18 | `servers/vfs/read.c` | ⏳ 待读 | read 实现 | - |
| 19 | `servers/vfs/write.c` | ⏳ 待读 | write 实现 | - |
| 20 | `servers/vfs/exec.c` | ⏳ 待读 | exec 支持 | - |
| 21 | `servers/vfs/mount.c` | ⏳ 待读 | mount 实现 | - |
| 22 | `servers/vfs/vmnt.c` | ⏳ 待读 | 挂载点管理 | - |
| 23 | `servers/vfs/smap.c` | ⏳ 待读 | 统计映射 | - |
| 24 | `servers/vfs/socket.c` | ⏳ 待读 | socket 实现 | - |
| 25 | `servers/vfs/pipe.c` | ⏳ 待读 | pipe 实现 | - |
| 26 | `servers/vfs/stadir.c` | ⏳ 待读 | stat 实现 | - |
| 27 | `servers/vfs/time.c` | ⏳ 待读 | 时间相关 | - |
| 28 | `servers/vfs/worker.c` | ⏳ 待读 | 工作线程 | - |
| 29 | `servers/vfs/table.c` | ⏳ 待读 | 表管理 | - |
| 30 | `servers/vfs/select.c` | ⏳ 待读 | select 实现 | - |
| 31 | `servers/vfs/sdev.c` | ⏳ 待读 | 特殊设备 | - |
| 32 | `servers/vfs/vnode.c` | ⏳ 待读 | 虚拟节点管理 | - |
| 33 | `servers/vfs/tll.c` | ⏳ 待读 | TLL 实现 | - |
| 34 | `servers/vfs/protect.c` | ⏳ 待读 | 保护机制 | - |
| 35 | `servers/vfs/path.c` | ⏳ 待读 | 路径处理 | - |
| 36 | `servers/vfs/utility.c` | ⏳ 待读 | 工具函数 | - |
| 37 | `servers/vfs/request.c` | ⏳ 待读 | 请求处理 | - |
| 38 | `servers/vfs/bdev.c` | ⏳ 待读 | 块设备 | - |
| 39 | `servers/vfs/device.c` | ⏳ 待读 | 设备管理 | - |
| 40 | `servers/vfs/link.c` | ⏳ 待读 | 链接实现 | - |
| 41 | `servers/vfs/misc.c` | ⏳ 待读 | 杂项函数 | - |
| 42 | `servers/vfs/coredump.c` | ⏳ 待读 | 核心转储 | - |
| 43 | `servers/vfs/cdev.c` | ⏳ 待读 | 字符设备 | - |
| 44 | `servers/vfs/gcov.c` | ⏳ 待读 | GCOV 支持 | - |
| 45 | `servers/vfs/dmap.c` | ⏳ 待读 | 设备映射管理 | - |
| 46 | `servers/vfs/comm.c` | ⏳ 待读 | 通信 | - |
| 47 | `servers/vfs/filedes.c` | ⏳ 待读 | 文件描述符 | - |
| 48 | `servers/vfs/lock.c` | ⏳ 待读 | 锁实现 | - |

---

## 核心概念总结

### VFS 服务职责
- **文件操作**：open/read/write/close
- **目录操作**：mkdir/rmdir/readdir
- **文件系统**：mount/umount
- **进程管理**：exec 支持
- **设备管理**：块设备/字符设备

---

## 进度统计

| 分类 | 已读 | 待读 | 覆盖率 |
|------|------|------|--------|
| 头文件 | 0 | 15 | 0% |
| 实现文件 | 0 | 33 | 0% |
| **总计** | **0** | **48** | **0%** |
