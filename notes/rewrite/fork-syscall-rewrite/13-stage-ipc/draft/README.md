# 13-stage-ipc（临时占位 README）

> **状态**：目录为空占位（2026-08-14 补建，补齐全部 11 个 server 的 stage 目录）。正式文档待写。

## 服务

**IPC — IPC 服务器**

Minix3 的用户态 IPC 基础设施服务器：提供进程间通信的注册/通道管理服务（区别于 kernel 内的 IPC 机制——kernel 的 `ipc.c`/`do_sendrec` 是底层机制，本服务是其上层的用户态管理服务）。

## 源码

- C：`minix3/minix/servers/ipc/`（4 个 .c 文件，1690 行）
- Rust：`os/servers/ipc-server/`（已建目录，待实现；注意 Rust 目录名为 `ipc-server` 避免与 kernel 的 ipc 概念混淆）

## 启动顺序

| 证据 | 位置 |
|------|------|
| boot_image 登记 | **不在 boot_image**（`table.c:44-64` 无 ipc 条目）—— 由 RS 运行时加载 |
| 位置说明 | 归类于 RS 加载组（与 IS/devman/input 同类），无固定 boot 序号 |

## 依赖关系

- 前置：01-stage-kernel（kernel 内 IPC 机制：`os/kernel/src/ipc.rs`、syscall send/receive）、02-stage-vm、03-stage-rs（加载方）
- 后置：19-stage-integration（跨服务 IPC 端到端验证）

## 编号说明

2026-08-14 重排：新 server 目录插入 10-13（mib/devman/input/ipc），收尾 stage 后移（10-stage-integration→14、11-redesign→15）。IPC 由 RS 运行时加载（不在 boot_image），属 RS 加载组。
