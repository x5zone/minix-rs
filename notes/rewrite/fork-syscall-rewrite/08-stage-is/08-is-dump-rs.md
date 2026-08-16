# 08-is-dump-rs: RS 数据域转储（dmp_rs.c）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 转储域
> **源码**: `minix3/minix/servers/is/dmp_rs.c`（74 行）
> **Rust 模块**: `dump_rs.rs`
> **draft 素材**: `draft/tmp_dmp_rs.c.md`（逐行素材）

## 核心点

- `rproc_dmp`（26）：双表 `getsysinfo(RS_PROC_NR, SI_PROCPUB_TAB + SI_PROC_TAB)` → label/endpoint/pid/flags/dev/period/alive_tm/restarts/args
- `s_flags_str`（61）：RS_ACTIVE/RS_UPDATING/RS_EXITING/RS_NOPINGREPLY + SF_USE_COPY/SF_USE_REPL 位编码（A-12）
- rprocpub/rproc 布局 ABI（A-4）：`struct rprocpub`/`struct rproc`（NR_SYS_PROCS，`../03-stage-rs` 布局契约）
- RS_IN_USE 过滤语义
- 22 行分页 + 静态 prev_i 游标（A-5）

## 边界

- **前置依赖**: 04 + RS 布局（`03-stage-rs`）
- **不覆盖（移交）**: RS 服务器内部语义（`03-stage-rs`）
