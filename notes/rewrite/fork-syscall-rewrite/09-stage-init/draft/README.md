# 09-stage-init（临时占位 README）

> **状态**：目录为空占位（git 不跟踪空目录，2026-08-13 创建后曾因空被删，2026-08-14 重建）。正式文档待写。

## 服务

**INIT — 用户态 init（第一个用户进程）**

boot 链路的终点：所有内核任务与系统服务就绪后，kernel 跳转到用户态运行 init，由 init 启动登录进程与用户环境（对应 `/sbin/init` 的角色）。`INIT_PROC_NR` 是 boot_image 中最后一个进程。

## 源码

- C：`minix3/sbin/init/init.c`（init 主程序；boot_image 仅登记进程号，`minix3/minix/kernel/table.c:64`）
- Rust：用户态进程，`os/` 中暂无对应实现目录（待定）

## 启动顺序

| 证据 | 位置 |
|------|------|
| boot_image 顺序 | `minix3/minix/kernel/table.c:64`（`{INIT_PROC_NR, "init"}` —— **boot_image 最后一项**） |
| 位置说明 | kernel → DS/RS/PM/SCHED/VFS/MEM/TTY/MIB/VM/PFS/MFS → **init**：所有服务加载完毕后，切换到用户态执行 init |

## 依赖关系

- 前置：全部 01-08 stage（kernel 基础设施 + 全部系统服务）
- 后置：14-stage-integration（init 启动后的端到端集成验证）

## 与真实 boot 顺序的关系（两层语义）

- **登记顺序**（`table.c:64`）：init 是 boot_image 中**最后一项** —— 一致
- **执行顺序**（`main.c:265-267`）：init 同样挂 `RTS_VMINHIBIT`，等 VM 建页表后可运行；作为 boot 链路终点，在所有系统服务就绪后切换到用户态执行 —— 一致
- 编号 09 与实际位置吻合；其后为补建的 server 目录（10-stage-mib / 11-stage-devman / 12-stage-input / 13-stage-ipc），14-stage-integration / 15-redesign 为收尾 stage。
