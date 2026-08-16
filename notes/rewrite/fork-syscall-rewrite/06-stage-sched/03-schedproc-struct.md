# 03-schedproc-struct: SchedProc 结构体

> **状态**: pending（最小骨架，待改写）
> **定位**: 数据结构（所有服务的共享状态）
> **源码**: `minix3/minix/servers/sched/schedproc.h`
> **Rust 模块**: `schedproc.rs`
> **draft 素材**: `draft/01-sched-struct.md`（素材）

## 核心点

- `schedproc` 全字段：`endpoint`/`parent`/`flags`/`max_priority`/`priority`/`time_slice`/`cpu`/`cpu_mask[]`
- `IN_USE` 标志（schedproc.h:35）
- `cpu_mask[]` 死字段（schedproc.h:33，C 全库零写入）——S-3 结构消除决策（plan.md §7.3）
- 优先级类型建模（S-2：`Priority` newtype，对齐 kernel 11-scheduling-primitives §3.3）
- 与 Kernel `proc`/PM `mproc`/VM `vmproc`/VFS `fproc` 的定位差异（最轻量）

## 边界

- **前置依赖**: 00
- **不覆盖（移交）**: 表管理/endpoint 验证（04）、调度参数字段语义（05）
