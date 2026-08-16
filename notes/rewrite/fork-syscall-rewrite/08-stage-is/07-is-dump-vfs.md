# 07-is-dump-vfs: VFS 数据域转储（dmp_fs.c）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 转储域
> **源码**: `minix3/minix/servers/is/dmp_fs.c`（83 行）
> **Rust 模块**: `dump_vfs.rs`
> **draft 素材**: `draft/tmp_dmp_fs.c.md`（逐行素材）

## 核心点

- `fproc_dmp`（25）：`getsysinfo(VFS_PROC_NR, SI_PROC_TAB)` → fproc 表格式化（pid/tty/umask/uid/gid/SESLDR/fd 计数/blocked_on/REVIVED）
- `dtab_dmp`（67）：`getsysinfo(VFS_PROC_NR, SI_DMAP_TAB)` → 设备↔驱动映射（NONE 跳过）
- fproc/dmap 布局 ABI（A-4）：`struct fproc`（NR_PROCS）、`struct dmap`（NR_DEVICES，`../05-stage-vfs` 布局契约）
- FP_SESLDR/FP_REVIVED/FP_BLOCKED_ON_CDEV 位语义、`major()/minor()` 设备号解码、OPEN_MAX fd 计数
- 22 行分页 + 静态 prev_i 游标（A-5）

## 边界

- **前置依赖**: 04 + VFS 布局（`05-stage-vfs`）
- **不覆盖（移交）**: VFS 服务器内部语义（`05-stage-vfs`）
