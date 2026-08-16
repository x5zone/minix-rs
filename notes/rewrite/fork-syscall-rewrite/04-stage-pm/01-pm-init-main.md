# 01: pm-init-main

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 启动与进程模型
> **源码**: minix3/minix/servers/pm/main.c（main/sef_local_startup/sef_cb_init_fresh/reply 调用点）、minix3/minix/servers/pm/schedule.c:sched_init（调用点）
> **Rust 模块**: main.rs/init.rs（未实现）
> **draft 素材**: draft/README.md（素材）

## 核心点

main()/sef_local_startup()/sef_cb_init_fresh()：mproc 表初始化、信号集合构建、sys_getmonparams/sys_getimage、boot image 填充（INIT + 系统进程）、VFS_PM_INIT 同步、system_hz、sched_init 调用点

## 边界

- **前置依赖**: 00
- **不覆盖（移交）**: get_nice_value 细节（16）、信号集合语义（11/99）、sched_init 协议（16）
