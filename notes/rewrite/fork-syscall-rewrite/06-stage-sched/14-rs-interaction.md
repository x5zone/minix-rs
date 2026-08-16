# 14-rs-interaction: RS 客户端契约

> **状态**: pending（最小骨架，待改写）
> **定位**: 客户端契约（RS 调用面，系统进程接管）
> **源码**: `minix3/minix/servers/rs/utility.c:364-384`、`rs/manager.c:461`、`rs/request.c:342`
> **Rust 模块**: `rs/sched.rs` 相关（03-stage-rs）
> **draft 素材**: 无（新增）

## 核心点

- `sched_init_proc`（rs/utility.c:364）：系统进程 `parent=RS_PROC_NR`、`SCHEDULING_START` 显式参数路径（`r_priority`/`r_quantum`/`r_cpu`）
- `r_scheduler`/`r_priority`/`r_quantum`/`r_cpu` 槽配置（跨 03-stage-rs/08-rs-slot-config.md）
- RS `sched_stop` 终止路径（manager.c:461、request.c:342，系统服务退出/Live Update）
- 与 PM 路径的差异对照：START 显式参数 vs INHERIT 父继承

## 边界

- **前置依赖**: 02/04/05
- **不覆盖（移交）**: RS 槽配置细节（03-stage-rs/08）、Live Update 全流程（03-stage-rs/16）
