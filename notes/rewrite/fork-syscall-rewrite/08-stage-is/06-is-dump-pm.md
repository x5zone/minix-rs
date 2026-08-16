# 06-is-dump-pm: PM 数据域转储（dmp_pm.c）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 转储域
> **源码**: `minix3/minix/servers/is/dmp_pm.c`（109 行）
> **Rust 模块**: `dump_pm.rs`
> **draft 素材**: `draft/tmp_dmp_pm.c.md`（逐行素材）

## 核心点

- `mproc_dmp`（41）：`getsysinfo(PM_PROC_NR, SI_PROC_TAB)` → mproc 表格式化（进程/pid/parent/tracer/uid/gid/nice/flags）
- `sigaction_dmp`（75）：同表 + `getticks()` 计算 alarm 剩余时间
- `flags_str`（21）：WAITING/ZOMBIE/ALARM_ON/EXITING/TRACE_STOPPED/SIGSUSPENDED/VFS_CALL/PROC_STOPPED/PRIV_PROC/PARTIAL_EXEC/DELAY_CALL 位编码（A-12）
- mproc 布局 ABI（A-4）：`struct mproc`（NR_PROCS 表，`../04-stage-pm` 布局契约）
- 22 行分页 + 静态 prev_i 游标 + `--more--\r`（A-5）

## 边界

- **前置依赖**: 04 + PM `mproc` 布局（`04-stage-pm`）
- **不覆盖（移交）**: PM 服务器内部语义（`04-stage-pm`）
