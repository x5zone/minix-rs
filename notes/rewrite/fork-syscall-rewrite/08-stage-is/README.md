# 08-stage-is（临时占位 README）

> **状态**：目录为空占位（git 不跟踪空目录，2026-08-13 创建后曾因空被删，2026-08-14 重建）。正式文档待写。

## 服务

**IS — Information Server（信息服务器）**

系统信息查询服务：提供内核/系统运行信息的统一查询入口（经 `GET_INFO` 类系统调用协议），区别于 DS 的发布/订阅模型。

## 源码

- C：`minix3/minix/servers/is/`（8 个 .c 文件，1151 行）
- Rust：`os/servers/is/`（已建目录，待实现）

## 启动顺序

| 证据 | 位置 |
|------|------|
| boot_image 顺序 | **不在 kernel boot_image 中**（`table.c:44-64` 无 `is` 条目）—— 由 RS 运行时加载（`rs/` 的启动配置决定） |
| 位置说明 | 归类于 RS 加载组（PM/VFS/SCHED/DS/IS 等），无固定 boot 顺序 |

## 依赖关系

- 前置：01-stage-kernel（syscall 基础设施）、02-stage-vm、03-stage-rs（加载方）
- 后置：14-stage-integration（跨服务集成时 IS 作为信息查询端接入）

## 与真实 boot 顺序的关系

IS 不在 kernel 直接 boot 列表（`table.c:44-64` 全部 17 项中无 `is`），由 RS 于运行时加载，故无固定启动序号。master-plan 将其排在 DS 之后（08），对应"RS 加载组"的阅读理解顺序。同组还包括 devman/input/ipc（`11-stage-devman` / `12-stage-input` / `13-stage-ipc`）。
