# 07-stage-ds（临时占位 README）

> **状态**：目录为空占位（git 不跟踪空目录，2026-08-13 创建后曾因空被删，2026-08-14 重建）。正式文档待写。

## 服务

**DS — Data Store（数据存储服务器）**

系统服务注册与查询：服务进程向 DS 发布键值对（订阅/发布模式），其他进程可订阅变化通知。Minix3 中的动态系统信息中心。

## 源码

- C：`minix3/minix/servers/ds/`（2 个 .c 文件，811 行）
- Rust：`os/servers/ds/`（已建目录，待实现）

## 启动顺序

| 证据 | 位置 |
|------|------|
| boot_image 顺序 | `minix3/minix/kernel/table.c:52`（`{DS_PROC_NR, "ds"}` —— **用户服务中第一个被加载**） |
| 位置说明 | kernel 任务（asyncm/idle/clock/system/kernel）之后最先加载的用户服务；RS 紧跟其后 |

## 依赖关系

- 前置：01-stage-kernel、02-stage-vm（DS 需要 syscall/IPC + 地址空间）
- 被依赖：RS 运行时加载的服务常向 DS 注册状态；RS 自身依赖 DS 发布服务状态

## 与真实 boot 顺序的关系（两层语义）

- **登记顺序**（`table.c:44-64`）：DS 是 boot_image 中**第一个用户服务**（`table.c:52`，在 RS 之前）
- **执行顺序**（`main.c:196` + `:265-267`）：DS 不在"立即可调度"集合（仅 kernel 任务/RS/VM），挂 `RTS_VMINHIBIT` 等 VM 建页表后才运行 —— 实际运行晚于 VM、RS
- master-plan 编号 07 采用阅读理解顺序：DS 功能简单、与主服务解耦，在讲完 PM/VFS/SCHED 后单独补述。若需严格对齐登记顺序，DS 应前移至 RS 之前（02 之后）。
