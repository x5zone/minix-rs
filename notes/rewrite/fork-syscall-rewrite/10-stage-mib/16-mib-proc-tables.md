# 16-mib-proc-tables: 进程表快照

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 10 进程信息（依赖前置）
> **源码**: `proc.c:1-225`
> **Rust 模块**: `proc/tables.rs`
> **draft 素材**: 无（新建）

## 核心点

- 三表快照：`proc_tab[NR_TASKS+NR_PROCS]`（`sys_getproctab` + PMAGIC 校验）、`mproc_tab[NR_PROCS]`（`getsysinfo(SI_PROC_TAB)` + MP_MAGIC）、`fproc_tab[NR_PROCS]`（`getsysinfo(SI_PROCLIGHT_TAB)`）
- `update_tables`：每 clock tick 至多一次（tabs_updated 节流）、失败闩锁（tabs_valid=FALSE 后不再重试）、EXTRA_PROCS=8 预留
- PID 哈希表：`HASH_SLOTS=NR_PROCS/4` + `hnext_tab` 链 + `get_mslot`
- 辅助：`ticks_to_timeval`、`fill_wmesg`（ANY/SELF/NONE/进程名/endpoint 兜底 + ipc 括号语义）
- A-6：跨服务器表布局契约（kernel/PM/VFS 重写后需同步）

## 边界

- **前置依赖**: 02 + kernel `06` + PM/VFS 表布局
- **不覆盖（移交）**: LWP/PROC2/ARGS 具体填充（17~19）
