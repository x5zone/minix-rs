# 05: vfs-interaction

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 主循环与异步协议
> **源码**: minix3/minix/servers/pm/main.c:295-424（handle_vfs_reply）、minix3/minix/servers/pm/utility.c:tell_vfs(123)、minix3/minix/include/minix/com.h:VFS_PM_*（520-544）
> **Rust 模块**: VFS 客户端/回复状态机（未实现）
> **draft 素材**: draft/pm-call-vfs-fork.md（素材）

## 核心点

tell_vfs（VFS_CALL 置位）、handle_vfs_reply 全部 11 种 VFS_PM_*_REPLY 状态机、NEW_PARENT/UNPAUSED、VFS_PM_* 协议（RQ 12 + RS 11）、VFS_PM_REBOOT_REPLY 特例

## 边界

- **前置依赖**: 04
- **不覆盖（移交）**: 具体服务流程（07~20）、事件订阅（06）、对端 VFS 实现（05-stage-vfs）
