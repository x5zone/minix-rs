# 15: credentials

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 6 身份与凭证
> **源码**: minix3/minix/servers/pm/getset.c（19/95）
> **Rust 模块**: mproc/credentials.rs、minix-types IdSet
> **draft 素材**: 无

## 核心点

do_get（GETUID/GETGID/GETGROUPS/GETPID/GETPGRP/GETSID/ISSETUGID）、do_set（SETUID/SETEUID/SETGID/SETEGID/SETGROUPS/SETSID）：权限检查、三元组更新、TAINTED、VFS 转发（VFS_PM_SETUID/SETGID/SETSID/SETGROUPS）+ SUSPEND

## 边界

- **前置依赖**: 02/05
- **不覆盖（移交）**: exec 的 setuid 位处理（17）、调度 nice 检查（16）
