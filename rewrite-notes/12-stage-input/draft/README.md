# 12-stage-input（临时占位 README）

> **状态**：目录为空占位（2026-08-14 补建，补齐全部 11 个 server 的 stage 目录）。正式文档待写。

## 服务

**INPUT — 输入服务器**

Minix3 的键盘/输入事件服务器：统一汇聚键盘等输入设备事件，向用户态进程提供输入流（配合 TTY/驱动层的输入采集）。

## 源码

- C：`minix3/minix/servers/input/`（1 个 .c 文件，704 行）
- Rust：`os/servers/input/`（已建目录，待实现）

## 启动顺序

| 证据 | 位置 |
|------|------|
| boot_image 登记 | **不在 boot_image**（`table.c:44-64` 无 input 条目）—— 由 RS 运行时加载 |
| 位置说明 | 归类于 RS 加载组（与 IS/devman/ipc 同类），无固定 boot 序号 |

## 依赖关系

- 前置：01-stage-kernel、02-stage-vm、03-stage-rs（加载方）
- 后置：19-stage-integration（输入链端到端验证）

## 编号说明

2026-08-14 重排：新 server 目录插入 10-13（mib/devman/input/ipc），收尾 stage 后移（10-stage-integration→14、11-redesign→15）。INPUT 由 RS 运行时加载（不在 boot_image），属 RS 加载组。
