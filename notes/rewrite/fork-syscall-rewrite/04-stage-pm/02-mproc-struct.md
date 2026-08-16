# 02: mproc-struct

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 启动与进程模型
> **源码**: minix3/minix/servers/pm/mproc.h
> **Rust 模块**: mproc/mproc.rs、mproc/{lifecycle,block,wait,guardianship,trace,signal,credentials,context}.rs
> **draft 素材**: draft/mproc-design.md（素材）

## 核心点

struct mproc 全部字段语义、19 个 mp_flags 正交位、mpsigact 独立表、MP_MAGIC、字段→Rust 分层模型（Identity/State/Resources/Context）映射（A-1/A-2）

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: 表操作/slot 分配（03）、具体状态机流转（09/10/11~13）
