# 11: signal-core

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 信号系统
> **源码**: minix3/minix/servers/pm/signal.c:197-220/294-383/546-650
> **Rust 模块**: signal.rs（stub）、mproc/signal.rs
> **draft 素材**: 无

## 核心点

do_kill/do_srv_kill、check_sig（pid 选择：>0/=0/-1/<-1、权限检查、INIT 保护、系统进程保护、广播 SIGTERM/SIGKILL 特例）、sig_proc（trace 先行/ignore/catch/block/VFS 中挂起/系统进程 ksig）、sig_proc_exit、process_ksig、SIGS_IS_LETHAL、core_sset/ign_sset/noign_sset

## 边界

- **前置依赖**: 03/04
- **不覆盖（移交）**: handler 安装语义（12）、停止/延迟/恢复机制（13）
