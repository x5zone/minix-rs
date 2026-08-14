# 11-stage-devman（临时占位 README）

> **状态**：目录为空占位（2026-08-14 补建，补齐全部 11 个 server 的 stage 目录）。正式文档待写。

## 服务

**DEVMAN — Device Manager（设备管理器）**

Minix3 的设备管理服务器：维护设备树/设备状态，处理设备驱动注册与设备接口管理（`/dev` 设备节点与驱动实例的生命周期协调）。

## 源码

- C：`minix3/minix/servers/devman/`（4 个 .c 文件，847 行）
- Rust：`os/servers/devman/`（已建目录，待实现）

## 启动顺序

| 证据 | 位置 |
|------|------|
| boot_image 登记 | **不在 boot_image**（`table.c:44-64` 无 devman 条目）—— 由 RS 运行时加载 |
| 位置说明 | 归类于 RS 加载组（与 IS/input/ipc 同类），无固定 boot 序号 |

## 依赖关系

- 前置：01-stage-kernel、02-stage-vm、03-stage-rs（加载方）
- 后置：14-stage-integration（设备驱动端到端接入）

## 编号说明

2026-08-14 重排：新 server 目录插入 10-13（mib/devman/input/ipc），收尾 stage 后移（10-stage-integration→14、11-redesign→15）。DEVMAN 由 RS 运行时加载（不在 boot_image），属 RS 加载组。
