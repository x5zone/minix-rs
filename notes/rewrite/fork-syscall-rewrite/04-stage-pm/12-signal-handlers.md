# 12: signal-handlers

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 信号系统
> **源码**: minix3/minix/servers/pm/signal.c:40-196/776-855
> **Rust 模块**: 信号 handler 状态（未实现）
> **draft 素材**: 无

## 核心点

do_sigaction（SIG_IGN/DFL/CATCH 三态、sa_mask/sa_flags、SIGKILL/SIGSTOP 保护）、do_sigprocmask（BLOCK/UNBLOCK/SETMASK/INQUIRE）、do_sigpending、do_sigsuspend（sigmask2 保存）、do_sigreturn、sig_send（sigmsg 构建、SA_NODEFER/SA_RESETHAND、sys_sigsend、EINTR 唤醒）

## 边界

- **前置依赖**: 11
- **不覆盖（移交）**: 内核 sigframe 推送细节（01-stage-kernel/19-syscall-signal.md）、停止/恢复（13）
