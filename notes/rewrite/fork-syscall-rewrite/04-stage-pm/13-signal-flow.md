# 13: signal-flow

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 信号系统
> **源码**: minix3/minix/servers/pm/signal.c:226-293/652-776
> **Rust 模块**: 延迟与恢复机制（未实现，A-10 缺口）
> **draft 素材**: 无

## 核心点

check_pending（解除阻塞后投递）、restart_sigs（VFS/事件回复后）、unpause（WAITING/SIGSUSPENDED/VFS UNPAUSE 三条路径）、stop_proc（may_delay/DELAY_CALL）、try_resume_proc、PROC_STOPPED 双用途、SIGSNDELAY 延迟调用恢复

## 边界

- **前置依赖**: 05/06/11/12
- **不覆盖（移交）**: 信号语义（11/12）、VFS 协议（05）、内核延迟调用实现（01-stage-kernel）
