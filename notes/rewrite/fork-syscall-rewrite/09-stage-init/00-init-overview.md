# 00-init-overview: INIT 整体概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 总览——init 是什么、boot 链位置、状态机主线图、文档导航
> **源码**: `minix3/sbin/init/`（init.c 1902 行）+ `minix3/minix/kernel/table.c:64` + `minix3/minix/servers/rs/table.c:28`
> **Rust 模块**: `os/commands/sbin/init/`（stub）
> **draft 素材**: `draft/README.md`（素材）

## 核心点

- init 是 boot 链路的**终点**：boot_image 最后一项（`kernel/table.c:64`），USR_F 用户进程（`rs/table.c:28`），由 VM `exec_bootproc` 加载 ELF 后由 kernel 调度运行
- **执行模型**：init 不是 IPC 事件循环服务（无 SEF/CALLMAP）——它是 waitpid(-1) + 信号驱动的**状态机**（`transition()` 主循环）
- 状态机主线图：`'s'` single_user → `'r'` runcom → `'t'` read_ttys → `'m'` multi_user（稳态）↔ `'T'` clean_ttys / `'c'` catatonia / `'d'` death
- 次主线：登录会话生命周期（07~11）
- 设计原则：读者学习顺序 = 系统实际执行顺序（继承 `01-stage-kernel/00-kernel-overview.md §3`）

## 边界

- **前置依赖**: 无（读 kernel/PM 前置文档：`../01-stage-kernel/09-vm-boot-protocol.md`、`../01-stage-kernel/10-switch-to-user.md`）
- **不覆盖（移交）**: 一切机制细节（01~14/99）
