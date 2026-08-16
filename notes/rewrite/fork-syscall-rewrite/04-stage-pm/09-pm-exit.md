# 09: pm-exit

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 进程生命周期（fork 次主线）
> **源码**: minix3/minix/servers/pm/forkexit.c:246-417/594-630/760-795
> **Rust 模块**: exit.rs（stub）、mproc/lifecycle.rs
> **draft 素材**: draft/exit-impl.md（素材）

## 核心点

do_exit/exit_proc/exit_restart：PRIV_PROC 直毁、sys_times 记账、sys_stop、vm_willexit、VFS_PM_EXIT/DUMPCORE、zombify/check_parent、disinherit（INIT 收养 + NEW_PARENT）、SIGHUP 进程组、TRACE_EXIT、TOLD_PARENT 后 cleanup

## 边界

- **前置依赖**: 03/05/06
- **不覆盖（移交）**: wait4 回收（10）、事件发布衔接（06）、VFS core dump 细节（05）
