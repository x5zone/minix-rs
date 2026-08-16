# 18: trace

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 9 ptrace
> **源码**: minix3/minix/servers/pm/trace.c（42/256）
> **Rust 模块**: mproc/trace.rs
> **draft 素材**: 无

## 核心点

do_trace 全部 T_*：T_OK/T_ATTACH（权限检查）/T_READB_INS/T_WRITEB_INS/T_EXIT（TRACE_EXIT）/T_SETOPT/T_GETRANGE/T_SETRANGE/T_DETACH/T_RESUME/T_STEP/T_SYSCALL、trace_stop（W_STOPCODE + wait_test）、mp_sigtrace 过滤、TRACE_STOPPED、tracer 死亡（tracer_died 在 09）

## 边界

- **前置依赖**: 03/10/11
- **不覆盖（移交）**: 内核 sys_trace 实现（01-stage-kernel）、tracer 死亡处理（09）、wait4 的 tracer 分支（10）
