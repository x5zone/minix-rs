# 03: mproc-table

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 启动与进程模型
> **源码**: minix3/minix/servers/pm/glo.h、minix3/minix/servers/pm/utility.c:get_free_pid(34)/pm_isokendpt(108)/find_proc(76)
> **Rust 模块**: mproc/table.rs、mproc/pid_gen.rs、mproc/constants.rs
> **draft 素材**: draft/mproc-design.md + draft/pid-generator.md（素材）

## 核心点

mproc[NR_PROCS] 进程表、procs_in_use、pm_isokendpt、find_proc、PID 生成（get_free_pid：NR_PIDS/循环复用/冲突扫描）、endpoint generation

## 边界

- **前置依赖**: 01/02
- **不覆盖（移交）**: 槽位使用方（07/09/10）、PID 之外的身份字段（02）
