# 10: pm-wait

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 进程生命周期（fork 次主线）
> **源码**: minix3/minix/servers/pm/forkexit.c:475-570/670-806、minix3/minix/servers/pm/utility.c:set_rusage_times(145)
> **Rust 模块**: wait.rs（stub）、mproc/wait.rs
> **draft 素材**: draft/wait-impl.md（素材）

## 核心点

do_wait4：pidarg 过滤（>0/0/-1/<-1）、WNOHANG、WAITING 置位、wait_test、tell_parent（rusage 拷贝 + W_EXITCODE）、tell_tracer（W_STOPCODE/W_EXITCODE）、TOLD_PARENT、cleanup、子时间累计

## 边界

- **前置依赖**: 03/09
- **不覆盖（移交）**: 僵尸产生（09）、trace 停止语义（18）
