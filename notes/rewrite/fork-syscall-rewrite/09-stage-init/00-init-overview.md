# 00-init-overview: INIT 整体概览

> **状态**: 正文 v1（2026-09-18，随 init 实体接线轮改写）
> **定位**: 总览——init 是什么、boot 链位置、状态机主线图、文档导航
> **源码**: `minix3/sbin/init/`（init.c 1902 行）+ `minix3/minix/kernel/table.c:64` + `minix3/minix/servers/rs/table.c:28`
> **Rust 模块**: `os/commands/sbin/init/`（crate `minix-init`，19 个模块文件：业务模块 + driver/host/signal_state/wait/password 五个接线轮新模块）

## 核心点

- init 是 boot 链路的**终点**：boot_image 最后一项（`kernel/table.c:64`），USR_F 用户进程（`rs/table.c:28`），由 VM `exec_bootproc` 加载 ELF 后由 kernel 调度运行
- **执行模型**：init 不是 IPC 事件循环服务（无 SEF/CALLMAP）——它是 waitpid(-1) + 信号驱动的**状态机**（`transition()` 主循环）
- 状态机主线图：`'s'` single_user → `'r'` runcom → `'t'` read_ttys → `'m'` multi_user（稳态）↔ `'T'` clean_ttys / `'c'` catatonia / `'d'` death
- 次主线：登录会话生命周期（07~11）
- 设计原则：读者学习顺序 = 系统实际执行顺序（继承 `01-stage-kernel/00-kernel-overview.md §3`）

## Rust 实现形态（2026-09-18 实体接线轮后）

- **决策与副作用分离**：`InitHost` 单接缝（**[ARCH: init-host-seam]**，plan.md §4 A-11）承载全部机器动作——fork/exec/waitpid/kill 直通 minix-sys，缺失封装诚实 ENOSYS；`ScriptHost` 剧本宿主供测试
- **单所有者全局状态**：C 的十个全局收敛进 `DriverState`，按状态切成 `ChildCollector`（收割）与 `Ledger`（台账）两个视图传递
- **运行时现状**：主循环真跑；信号安装/setsid/控制终端/uid/文件 IO 的 live 半等 minix-sys 客户端面（edge E-INITSYS ①②），期间逐项诚实告警降级

## 边界

- **前置依赖**: 无（读 kernel/PM 前置文档：`../01-stage-kernel/09-vm-boot-protocol.md`、`../01-stage-kernel/10-switch-to-user.md`）
- **不覆盖（移交）**: 一切机制细节（01~14/99）
