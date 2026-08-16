# 10-stage-mib（临时占位 README）

> **状态**：目录为空占位（2026-08-14 补建，补齐全部 11 个 server 的 stage 目录）。正式文档待写。

## 服务

**MIB — Management Information Base（系统信息库服务器）**

Minix3 的 MIB 服务器：以对象树（OID）形式维护系统配置/统计信息，供用户态通过 `/mib` 挂载点或系统调用查询与设置（类似 SNMP 信息模型的内核版）。

## 源码

- C：`minix3/minix/servers/mib/`（8 个 .c 文件，4990 行）
- Rust：`os/servers/mib/`（已建目录，待实现）

## 启动顺序

| 证据 | 位置 |
|------|------|
| boot_image 登记 | `minix3/minix/kernel/table.c:60`（`{MIB_PROC_NR, "mib"}`）—— **kernel 直接 boot 的 server 之一** |
| 执行语义 | 与 DS/IS 同类（系统信息服务），无加载依赖；VM 建页表解除 `RTS_VMINHIBIT`（`main.c:265-267`）后可运行 |

## 依赖关系

- 前置：01-stage-kernel（syscall/IPC）、02-stage-vm（页表）
- 后置：14-stage-integration（/mib 挂载点与用户态工具集成）

## 编号说明

2026-08-14 重排：新 server 目录插入 10-13（mib/devman/input/ipc），收尾 stage 后移（10-stage-integration→14、11-redesign→15）。MIB 在 boot_image 中（`table.c:60`），语义位置与 DS/IS 相邻。
