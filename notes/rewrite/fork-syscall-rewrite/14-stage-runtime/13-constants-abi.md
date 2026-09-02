# 13-constants-abi: 常量与 ABI 对齐

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 6 — 常量 ABI
> **源码**: `minix3/include/errno.h`、`minix3/sys/sys/errno.h`、`sys/sys/termios.h`、`signal.h`、`fcntl.h`、`stat.h`、`ioctl.h`、`wait.h`、`resource.h`、`times.h`、`utsname.h`、`minix/include/minix/callnr.h`、`com.h`
> **Rust 模块**: `os/libs/minix-types`（errno/endpoint/com 等）
> **draft 素材**: 无（新建）

## 核心点

- errno 全值表（sys/sys/errno.h，Minix3 编号与 Linux 不同，如 ENOSYS=78）
- termios 全常量（sys/sys/termios.h）
- signal 编号、fcntl/stat/ioctl/wait/resource/times/utsname 常量
- callnr.h 全表（PM_BASE/VFS_BASE）与 com.h endpoint/服务号常量
- ARCH A-5/A-7：Errno 类型与 64 位字段适配（minix-types 现状核对）

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 常量如何被使用（各文档）
