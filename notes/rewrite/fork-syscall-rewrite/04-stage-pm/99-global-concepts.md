# 99: global-concepts

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（跨文档共享）
> **源码**: minix3/minix/servers/pm/（pm.h/const.h/type.h/glo.h/proto.h）、minix3/minix/include/minix/（callnr.h/com.h）
> **Rust 模块**: mproc/constants.rs、minix-types
> **draft 素材**: draft/mproc-design.md 部分（素材）

## 核心点

endpoint/generation 编码、NR_PIDS/INIT_PID/NO_PID/NO_TRACER/NO_EVENTSUB/NR_ITIMERS 常量、core_sset/ign_sset/noign_sset、全局状态表（m_in/who_p/who_e/call_nr/mp/system_hz/abort_flag/monitor_params）、PROC_NAME_LEN、47 个调用号索引

## 边界

- **前置依赖**: 00
- **不覆盖（移交）**: 机制细节（一切机制文档）
