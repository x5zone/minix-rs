# 05-syscall-mechanism: syscall 封装机制

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 — 系统调用机制
> **源码**: `minix3/minix/lib/libc/sys/syscall.c`、`loadname.c`、`libsys/kernel_call.c`
> **Rust 模块**: `os/libs/minix-sys`（sendrec 之上的 syscall 族）
> **draft 素材**: 无（新建）

## 核心点

- _syscall 协议：m_type=callnr、ipc_sendrec 自身失败→m_type=status、回复负值→errno=-m_type（syscall.c:9-24）
- _kernel_call：ENOTREADY 重试 + tickdelay 退避（kernel_call.c:7-19）
- _loadname：M_PATH_STRING_MAX=40 内联/指针双路径（loadname.c）
- Rust 建模：minix-sys syscall 族的统一错误路径（Errno 映射，A-5）

## 边界

- **前置依赖**: 04（IPC 原语）
- **不覆盖（移交）**: IPC 原语本身（04）、errno 常量值（13）、各服务消息字段（08~12）
