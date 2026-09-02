# 08-pm-syscalls: PM 进程管理 syscall 封装

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 — syscall 封装（次主线·按服务分组）
> **源码**: `minix3/minix/lib/libc/sys/fork.c` 等 PM 相关 40+ 文件、`libsys/srv_fork.c`/`srv_kill.c`、`libc/gen/raise.c`/`clock.c`/`waitpid.c`/`sleep.c`/`execle.c`/`sigsetops.c`、`lib/libc/gen/times.c`
> **Rust 模块**: `os/libs/minix-sys`（pm 模块）、`os/libs/minix-types`（ipc/pm.rs）
> **draft 素材**: 无（新建）

## 核心点

- PM 调用全清单（callnr.h PM_BASE+1~47）：FORK/EXIT/WAIT4/EXEC/KILL/SIGACTION/GETTIMEOFDAY/GETRUSAGE/GETPRIORITY/ITIMER/CLOCK_*/STIME/PTRACE/REBOOT/SRV_FORK/SRV_KILL 等
- 进程生命周期族：fork/execve/exit/wait4/vfork（PM_FORK 变体）+ exec 栈构建衔接（01）
- 信号族：sigaction/sigprocmask/sigpending/sigsuspend/sigreturn + _mcontext/_ucontext（ARCH A-9）
- 身份与权限族：uid/gid/setuid/setgroups/issetugid/getpid/getpgrp/getsid
- 时间与资源族：gettimeofday/clock_gettime/getrusage/times/itimer/rlimit（含组合面标注）
- 组合函数标注：raise=kill、waitpid=wait4、times=getrusage、sleep/usleep=nanosleep→select、execle=execve

## 边界

- **前置依赖**: 05
- **不覆盖（移交）**: 消息布局细节（99）、常量值（13）、PM server 实现（04-stage-pm）
