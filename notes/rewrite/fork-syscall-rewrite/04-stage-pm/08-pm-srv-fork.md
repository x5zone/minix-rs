# 08: pm-srv-fork

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 进程生命周期（fork 次主线）
> **源码**: minix3/minix/servers/pm/forkexit.c:do_srv_fork(146)
> **Rust 模块**: fork.rs（部分）
> **draft 素材**: draft/srv-fork-impl.md（素材）

## 核心点

do_srv_fork：仅 RS 调用（EPERM）、PRIV_PROC 继承、UID/GID 从消息注入、VFS_PM_SRV_FORK、立即 reply（非 SUSPEND）、与 do_fork 差异对照

## 边界

- **前置依赖**: 07
- **不覆盖（移交）**: 普通 fork（07）、RS 侧调用逻辑（03-stage-rs）
