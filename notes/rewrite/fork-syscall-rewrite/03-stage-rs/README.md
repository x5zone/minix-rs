# 03-stage-rs（临时占位 README）

> **状态**：目录为空占位（git 不跟踪空目录，2026-08-13 创建后曾因空被删，2026-08-14 重建）。正式文档待写。

## 服务

**RS — Reincarnation Server（复活/服务监管服务器）**

由 kernel 直接 boot（`table.c:53`），负责运行时加载并启动其余用户服务（PM/VFS/SCHED/DS/IS 等），监控服务崩溃并按需重启。

## 源码

- C：`minix3/minix/servers/rs/`（8 个 .c 文件，6307 行）
- Rust：`os/servers/rs/`（已建目录，待实现）

## 启动顺序

| 证据 | 位置 |
|------|------|
| boot_image 顺序 | `minix3/minix/kernel/table.c:53`（`{RS_PROC_NR, "rs"}`，紧跟在 DS 之后） |
| 位置说明 | kernel → DS → **RS** → PM/SCHED/VFS/…；RS 是其余用户服务的加载者 |

## 依赖关系

- 前置：01-stage-kernel（syscall/IPC 基础设施）、02-stage-vm（页表/地址空间）
- 后置：04-stage-pm、05-stage-vfs、06-stage-sched 等全部由 RS 加载的服务

## 与真实 boot 顺序的关系（两层语义）

- **登记顺序**（`table.c:44-64` boot_image 数组）：ds → rs → pm → sched → vfs → memory → tty → mib → **vm** → pfs → mfs → init —— RS 是第 2 个用户服务
- **执行顺序**（`main.c`）：`main.c:196` 仅 kernel 任务 + RS（root sysproc，`proc.h:279`）+ VM 立即可调度；`main.c:265-267` 非 VM 进程挂 `RTS_VMINHIBIT` 等 VM 建页表 → 实际运行顺序为 **kernel 任务 → VM（第一个运行的用户服务）→ RS → 其余**（VM 解除抑制后）
- master-plan 编号（vm=02、rs=03）采用**执行语义**：VM 因 ptproc 角色（boot 早期页表代理，`proc.c:169`）最先，RS 紧随其后与其"加载其余服务"的因果角色一致。
