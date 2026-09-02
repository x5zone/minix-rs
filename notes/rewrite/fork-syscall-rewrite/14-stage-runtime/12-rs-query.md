# 12-rs-query: RS 服务发现与查询

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 — syscall 封装（次主线·按服务分组）
> **源码**: `minix3/minix/lib/libc/sys/minix_rs.c`、`libsys/getepinfo.c`/`getprocnr.c`/`getsysinfo.c`、`minix/include/minix/rs.h`
> **Rust 模块**: `os/libs/minix-sys`（rs 模块）
> **draft 素材**: 无（新建）

## 核心点

- minix_rs_lookup：RS_LOOKUP 消息（m_rs_req.name/name_len → m_rs_req.endpoint，minix_rs.c）
- getepinfo（PM_GETEPINFO）、getprocnr（PM_GETPROCNR）、getsysinfo（PM_GETSYSINFO，who/what/where 协议）
- RS endpoint 解析与缓存语义

## 边界

- **前置依赖**: 05
- **不覆盖（移交）**: RS server 实现（03-stage-rs）、服务注册协议细节（03-stage-rs）
