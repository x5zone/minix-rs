# 11-misc-syscalls: 杂项与系统信息 syscall

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 — syscall 封装（次主线·按服务分组）
> **源码**: `minix3/minix/lib/libc/sys/nanosleep.c`、`__sysctl.c`、`svrctl.c`、`libsys/getticks.c`/`getuptime.c`/`clock_time.c`、`libc/gen/read_tsc_64.c`
> **Rust 模块**: `os/libs/minix-sys`（misc 模块）
> **draft 素材**: 无（新建）

## 核心点

- nanosleep（select 组合，VFS_SELECT 超时复用）
- __sysctl：MIB_PROC_NR/MIB_SYSCTL（com.h:1026，10-stage-mib 客户端）
- svrctl：PM_SVRCTL + VFS_SVRCTL 双路径（svrctl.c:20-22）
- getticks/getuptime/clock_time：kerninfo 直读（无 syscall，01 的消费面）
- read_tsc_64：TSC 直读（libc/gen/read_tsc_64.c）

## 边界

- **前置依赖**: 05
- **不覆盖（移交）**: 时钟 syscall 面（PM_CLOCK_* 归 08）、MIB server（10-stage-mib）、时钟服务端（06-stage-sched 等）
